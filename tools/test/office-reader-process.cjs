'use strict';
// Exercise the packaged engine and actual reader under the production CSP.
// Inputs are two owned, two-page fixtures. All scratch and profiles are isolated.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const {startKernel} = require('./v006-refactor-host.cjs');
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');

async function main() {
  const [binary, assets, word, slides, output, playwright] = process.argv.slice(2).map(p => path.resolve(p));
  assert(binary && assets && word && slides && output, 'Expected binary, assets, DOCX, PPTX, new evidence directory, [Playwright module]');
  fs.mkdirSync(output);
  const scratch = fs.mkdtempSync(path.join(output, 'scratch-'));
  const own = rel => path.join(scratch, rel);
  const docs = own('docs'), profile = own('profile');
  for (const dir of [docs, profile, own('tmp'), own('home')]) fs.mkdirSync(dir);
  const cases = [[word, '实验%20报告.docx'], [slides, '答辩%20演示.pptx']].map(([input,name])=>{
    const file = path.join(docs,name), original = fs.readFileSync(input);
    fs.writeFileSync(file,original);
    return {file,name,digest:hash(original)};
  });
  const sibling = path.join(docs,'实验%20报告.md');
  fs.writeFileSync(sibling,'# Existing Markdown');
  const previousTmp = process.env.TMPDIR; process.env.TMPDIR = own('tmp');
  let host, context;
  const results = [];
  try {
    host = await startKernel(binary,profile,docs,{assets,requestTimeoutMs:120000});
    const base = new URL((await host.api('/')).url).origin;
    const post = data=>host.api('/api/office/preview',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(data)});
    const denied = await fetch(base+'/api/office/preview',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({path:cases[0].file})});
    assert.equal(denied.status,403);
    const {chromium} = require(playwright || require.resolve('playwright',{paths:[path.resolve('ui-tests')]}));
    const env = Object.fromEntries(Object.entries(process.env).filter(([k])=>/^(PATH|SYSTEMROOT|WINDIR|COMSPEC|PATHEXT|LANG|LC_ALL)$/i.test(k)));
    Object.assign(env,{HOME:own('home'),TMPDIR:own('tmp'),CFFIXED_USER_HOME:own('home')});
    context = await chromium.launchPersistentContext(own('browser'),{channel:process.env.READMD_CODE_BROWSER || 'chrome',headless:true,env,
      viewport:{width:1160,height:820},args:['--disable-crash-reporter','--disable-breakpad','--disable-background-networking','--password-store=basic']});
    const page=context.pages()[0];
    page.setDefaultTimeout(90000);
    // Poll from the driver so Playwright never needs to compile a polling
    // predicate in the page after the production CSP has taken effect.
    const waitFor = async (predicate, value) => {
      const deadline = Date.now() + 30000;
      while (!await page.evaluate(predicate, value)) {
        if (Date.now() >= deadline) throw Error('Reader did not reach the requested state');
        await new Promise(resolve=>setTimeout(resolve,30));
      }
    };
    const conversions=[];
    page.on('request', req=>{if(new URL(req.url()).pathname==='/api/convert') conversions.push(1);});
    await page.goto(base);
    assert((await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content')).includes("script-src 'self'"));
    await waitFor(()=>window.__readmdAppReady);
    await page.evaluate(async dir=>{await window.__readmdSessionReady; if(!await listFolder(dir)) throw Error('Folder open failed');},docs);
    for(const item of cases){
      const result = await post({path:item.file});
      assert.equal(result.status,200,'PDF: '+(result.ok?'ok':await result.text()));
      assert.equal(result.headers.get('cache-control'),'no-store');
      const pdf=Buffer.from(await result.arrayBuffer());
      assert.equal(pdf.subarray(0,5).toString(),'%PDF-');
      fs.writeFileSync(path.join(output,item.name+'.pdf'),pdf);
      const waitPage = match => page.waitForResponse(r=>new URL(r.url()).pathname==='/api/office/preview' && match(r.request().postDataJSON()));
      if (!await page.locator('#side-files').isVisible()) await page.locator('#tab-files').click();
      const row=page.locator('#file-list [role="treeitem"]').filter({hasText:item.name}).last();
      await row.waitFor({state:'visible',timeout:10000});
      const [firstRes]=await Promise.all([waitPage(body=>body.path===item.file && body.page===0),row.click()]);assert.equal(firstRes.status(),200,'Page 1: '+(firstRes.ok()?'ok':await firstRes.text()));
      const view=firstRes.headers()['x-readmd-view'];assert.equal(firstRes.headers()['x-readmd-pages'],'2');assert(view);
      const image=page.locator('.office-preview-page');await image.waitFor({state:'visible'});
      await waitFor(()=>document.querySelector('.office-preview-page')?.naturalWidth>0);
      const displayedPage = async expected => {
        await waitFor(index => document.querySelector('#pg-page-select')?.value === String(index)
          && document.querySelector('.office-preview')?.getAttribute('aria-busy') === 'false'
          && document.querySelector('.office-preview-page')?.complete
          && document.querySelector('.office-preview-page')?.naturalWidth > 0,expected);
        return image.evaluate(async element => {
          const url = element.currentSrc;
          if (!url.startsWith('blob:')) throw Error('Expected a live reader page');
          const canvas = document.createElement('canvas');
          canvas.width = 512;
          canvas.height = Math.max(1, Math.round(canvas.width * element.naturalHeight / element.naturalWidth));
          const draw = canvas.getContext('2d');
          draw.drawImage(element,0,0,canvas.width,canvas.height);
          const pixels = draw.getImageData(0,0,canvas.width,canvas.height).data;
          const digest = new Uint8Array(await crypto.subtle.digest('SHA-256',pixels));
          return Array.from(digest,b => b.toString(16).padStart(2,'0')).join('');
        });
      };
      const firstImage=await displayedPage(0);
      assert.equal(await page.evaluate(()=>getActiveTab()?.mode),'office-preview');
      assert.equal(await page.evaluate(()=>state.editing),false);
      const firstTurnStarted=performance.now();
      const second = waitPage(body=>body.view===view && body.page===1);
      await page.locator('#pg-next-btn').click();
      const secondRes = await second;assert.equal(secondRes.status(),200);assert.equal(secondRes.headers()['x-readmd-page'],'1');
      const firstTurnMs=Math.round(performance.now()-firstTurnStarted);
      const lastImage=await displayedPage(1);assert.notEqual(firstImage,lastImage);
      await waitFor(()=>document.querySelector('.office-preview')?.getAttribute('aria-busy')==='false');
      // Returning to one of the pages in this still-open layout yields exactly
      // the already displayed PNG before any eviction or source revalidation.
      const repeatTurnMs=[];
      for (const [selector,index,pixels] of [['#pg-prev-btn',0,firstImage],['#pg-next-btn',1,lastImage]]) {
        const started=performance.now();
        const response=waitPage(body=>body.view===view && body.page===index);
        await page.locator(selector).click();assert.equal((await response).status(),200);
        repeatTurnMs.push(Math.round(performance.now()-started));
        assert.equal(await displayedPage(index),pixels);
      }
      await page.screenshot({path:path.join(output,item.name+'.png')});
      // Evict the current layout exactly as the server's idle/size limit would.
      // First allow an unchanged source to resume, then atomically replace the
      // file with identical bytes. Its document identity must still change.
      assert.equal((await post({view,close:true})).status,200);
      const resumed = page.waitForResponse(r=>new URL(r.url()).pathname==='/api/office/preview' &&
        r.request().postDataJSON()?.path===item.file && r.request().postDataJSON()?.page===0);
      await page.locator('#pg-prev-btn').click();
      const resumedRes=await resumed;assert.equal(resumedRes.status(),200);
      assert.equal(await displayedPage(0),firstImage);
      const resumedView=resumedRes.headers()['x-readmd-view'];assert(resumedView && resumedView!==view);
      await post({view:resumedView,close:true});
      const replacement=item.file+'.replacement';
      fs.copyFileSync(item.file,replacement);fs.renameSync(replacement,item.file);
      const conflict = page.waitForResponse(r=>new URL(r.url()).pathname==='/api/office/preview' &&
        r.request().postDataJSON()?.path===item.file && r.request().postDataJSON()?.page===1);
      await page.locator('#pg-next-btn').click();
      const refused=await conflict;assert.equal(refused.status(),409,'Expired view must refuse a replacement file');
      assert.equal((await refused.json()).error_code,'office_preview_changed');
      await page.locator('.office-preview-status:visible').waitFor();
      // A later attempt must keep the identity fence after its old view is gone.
      assert.equal(await page.evaluate(()=>ReadMDOffice.goToPage(1)),false);
      assert.equal(await page.evaluate(()=>getActiveTab().readerPage),0);
      await page.evaluate(()=>closeTab(getActiveTab().id));
      assert.equal((await post({view,page:0})).status,410);
      assert.equal(hash(fs.readFileSync(item.file)),item.digest);
      results.push({format:path.extname(item.name).slice(1),source:item.digest,pdf:hash(pdf),pages:[firstImage,lastImage],firstTurnMs,repeatTurnMs});
    }
    const workRecords=fs.readdirSync(path.join(profile,'worker-tasks-v1'),{withFileTypes:true});
    assert(!workRecords.some(entry=>entry.isDirectory()),'Completed reader tasks must return their reservations');
    assert.equal(conversions.length,0);
    assert.equal(fs.readFileSync(sibling,'utf8'),'# Existing Markdown');
    assert.deepEqual(fs.readdirSync(docs).sort(),[...cases.map(c=>c.name),path.basename(sibling)].sort());
    fs.writeFileSync(path.join(output,'result.json'),JSON.stringify({kernel:hash(fs.readFileSync(binary)),boot:hash(fs.readFileSync(path.join(assets,'readmd.boot.js'))),platform:process.platform,arch:process.arch,results},null,2)+'\n');
    console.log('Verified DOCX and PPTX pages, reader navigation, close, authentication and source preservation');
  } finally {
    try { await context?.close(); } finally {
      try {await host?.stop();} finally {
        if(previousTmp===undefined)delete process.env.TMPDIR;else process.env.TMPDIR=previousTmp;
        fs.rmSync(scratch,{recursive:true,force:true});
      }
    }
  }
}
main().catch(error=>{console.error(error.stack || error.message);process.exitCode=1;});
