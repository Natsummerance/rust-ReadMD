'use strict';
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {chromium}=require('playwright');
const {start}=require('./lib/serve-static.cjs');
(async()=>{
 const root=path.resolve(__dirname,'../dist');const hosted=await start(root);
 const current=JSON.parse(fs.readFileSync(path.join(root,'version.json'),'utf8'));
 const next={...current,version:'0.0.5',releaseTag:'V0.0.5',updatedAt:'2026-10-08T12:00:00Z'};
 next.assetsBaseUrl=current.assetsBaseUrl.replace(current.releaseTag,next.releaseTag);
 next.checksumUrl=next.assetsBaseUrl+'SHA256SUMS.txt';
 next.assets=current.assets.map(a=>({...a,name:a.name.replace(current.version,next.version),url:a.url.replace(current.releaseTag,next.releaseTag).replace(current.version+'.',next.version+'.')}));
 const browser=await chromium.launch();const results=[];
 try{
  for(const lang of ['','zh-cn/','zh-tw/','ja/'])for(const scenario of ['blocked','upgrade','incomplete']){
   const page=await browser.newPage({viewport:{width:1160,height:820}});const errors=[];
   page.on('pageerror',e=>errors.push(e.message));
   await page.route('https://api.github.com/**',route=>scenario==='blocked'?route.abort():route.fulfill({json:scenario==='incomplete'?{...next,assets:next.assets.slice(1)}:next}));
   await page.goto(hosted.url+'/'+lang+'download/');
   await page.waitForFunction(()=>!!window.__readmdReleaseReady);
   await page.evaluate(()=>window.__readmdReleaseReady);
   const expected=scenario==='upgrade'?'V0.0.5':current.releaseTag;
   const info=await page.evaluate(()=>({title:document.title,version:document.documentElement.dataset.publishedRelease,
    badge:document.querySelector('.latest-version-badge')?.textContent,
    urls:[...document.querySelectorAll('a[href*="/releases/download/"]')].map(a=>a.href),
    overflow:document.documentElement.scrollWidth>innerWidth,
    styles:[...document.styleSheets].some(s=>s.href?.endsWith('/assets/site.css')&&s.cssRules.length>0)}));
   assert.equal(info.version,expected);assert.ok(info.title.includes(expected),info.title);assert.equal(info.badge,expected);
   assert.equal(info.styles,true);assert.equal(info.overflow,false);assert.equal(errors.length,0,errors.join('\n'));
   const release=scenario==='upgrade'?next:current;
   assert.deepEqual(new Set(info.urls),new Set(release.assets.map(a=>a.url)));
   await page.locator('[data-mirror="ghfast"]').click();
   await page.waitForFunction(()=>[...document.querySelectorAll('[data-mirror-url]')].filter(a=>a.dataset.mirrorUrl.includes('/releases/download/')).every(a=>a.href.startsWith('https://ghfast.top/')));
   assert.ok((await page.locator('a[href*="SHA256SUMS.txt"]').first().getAttribute('href')).includes('/'+expected+'/SHA256SUMS.txt'));
   await page.locator('[data-mirror="direct"]').click();
   await page.waitForFunction(()=>[...document.querySelectorAll('[data-mirror-url]')].filter(a=>a.dataset.mirrorUrl.includes('/releases/download/')).every(a=>a.href===a.dataset.mirrorUrl));
   if(lang==='zh-cn/'&&scenario==='blocked'){
    for(const width of [390,768,1024,1440]){await page.setViewportSize({width,height:900});assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false,'overflow at '+width);}
    await page.setViewportSize({width:1440,height:1000});await page.locator('h1').scrollIntoViewIfNeeded();
    await page.waitForTimeout(1500); // Let the existing 1.1s title reveal finish before visual capture.
    assert.equal(await page.locator('h1').evaluate(el=>getComputedStyle(el).opacity),'1');
    await page.screenshot({path:'.cache/capability-upgrade/website-download-fixed.png',fullPage:false});
   }
   results.push({language:lang||'en',scenario,expected,links:info.urls.length,status:'passed'});await page.close();
  }
  fs.writeFileSync('.cache/capability-upgrade/website-download-browser.json',JSON.stringify({passed:results.length,results},null,2));
  console.log(JSON.stringify({passed:results.length,layouts:[390,768,1024,1160,1440],cspBypassed:false}));
 }finally{await browser.close();await new Promise(r=>hosted.server.close(r));}
})().catch(e=>{console.error(e);process.exitCode=1;});
