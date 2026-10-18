'use strict';
// Real Windows host, formal CSP, installed offline worker and actual exported files.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),zlib=require('node:zlib');
const {chromium}=require('playwright'),{startNative}=require('./native-session.cjs');
const root=path.resolve(__dirname,'..'),data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/publication-links-')),docs=path.join(data,'documents');fs.mkdirSync(docs);
const source='# Evidence and reasoning {#sec:opening}\n\n[First copy](#same) · [Second copy](#same-1) · [API one](#API_V1) · [API two](#API-V1) · [Greek](#%CE%B1) · [Missing](#absent)\n\nWe keep observations separate from interpretations and record the source of every claim.\n\n| Stage | Evidence | Review |\n|---|---|---|\n| Observation | Field notes | Verified |\n| Counterexample | Workshop | Pending |\n\n$$P(H\\mid E)=\\frac{P(E\\mid H)P(H)}{P(E)}$$\n\n## Same\n\nFirst interpretation.\n\n<!-- pagebreak -->\n\n## Same\n\nSecond interpretation with a counterexample.\n\n## API one {#API_V1}\n\nEndpoint details.\n\n## API two {#API-V1}\n\nIndependent endpoint details.\n\n## Greek {#α}\n\nA stable Unicode identity.\n';
const file=path.join(docs,'Evidence-workshop.md');fs.writeFileSync(file,source);
const report={platform:'windows-x64',nativeCsp:true,synthetic:true,checks:[],passed:false};let native,phase='startup';const delay=ms=>new Promise(r=>setTimeout(r,ms));
async function until(check){const limit=Date.now()+20000;while(!await check()){if(Date.now()>limit)throw Error('Timed out at '+phase);await delay(60);}}
function zipParts(bytes){
  let end=bytes.length-22;while(end>=0&&bytes.readUInt32LE(end)!==0x06054b50)end--;assert.ok(end>=0);
  let at=bytes.readUInt32LE(end+16);const count=bytes.readUInt16LE(end+10),parts=new Map();
  for(let i=0;i<count;i++){
    assert.equal(bytes.readUInt32LE(at),0x02014b50);const method=bytes.readUInt16LE(at+10),size=bytes.readUInt32LE(at+20),n=bytes.readUInt16LE(at+28),extra=bytes.readUInt16LE(at+30),comment=bytes.readUInt16LE(at+32),local=bytes.readUInt32LE(at+42),name=bytes.subarray(at+46,at+46+n).toString();
    const start=local+30+bytes.readUInt16LE(local+26)+bytes.readUInt16LE(local+28),content=bytes.subarray(start,start+size);parts.set(name,(method===8?zlib.inflateRawSync(content):content).toString());at+=46+n+extra+comment;
  }return parts;
}
(async()=>{try{
  assert.ok(process.env.READMD_PDF_PACK,'locked offline pack is required');
  delete process.env.READMD_PDF_WORKER;delete process.env.READMD_PDFIUM_LIB;process.env.READMD_WORKSPACE=docs;
  report.binarySha256=crypto.createHash('sha256').update(fs.readFileSync(process.env.READMD_BIN)).digest('hex');report.packSha256=crypto.createHash('sha256').update(fs.readFileSync(process.env.READMD_PDF_PACK)).digest('hex');
  native=await startNative(root,data,chromium,28903);const page=native.page;
  const api=(route,body)=>page.evaluate(async({route,body})=>{const response=await apiFetch(route,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});return {status:response.status,data:await response.json()};},{route,body});
  await page.evaluate(async file=>{await i18n.setLanguage('zh-CN',false);await loadFile(file);},file);
  phase='reader and preview identities';await until(()=>page.locator('#content h2[id="same-1"]').count());await page.locator('#content').getByRole('link',{name:'Second copy',exact:true}).click();assert.equal(await page.locator('#content h2[id="same-1"]').evaluate(el=>el===document.activeElement),true);
  await page.evaluate(async()=>{await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');});await until(()=>page.locator('#preview-pane h2[id="same-1"]').count());await page.locator('#preview-pane a[href="#same-1"]').click();assert.equal(await page.locator('#preview-pane h2[id="same-1"]').evaluate(el=>el===document.activeElement),true);assert.equal(await page.evaluate(()=>getEditContent()),source);assert.equal(fs.readFileSync(file,'utf8'),source);
  report.checks.push('real reader and split preview jump within their own content and preserve the source');
  phase='actual DOCX export';const word=path.join(docs,'Evidence-workshop.docx'),pdf=path.join(docs,'Evidence-workshop.pdf');
  const options={toc:{enabled:true},pdf:{engine:'rust'}};
  let response=await api('/api/export',{format:'docx',content:source,baseDir:docs,options,out_path:word});assert.equal(response.status,200);assert.equal(response.data.ok,true);
  const xml=zipParts(fs.readFileSync(word)).get('word/document.xml'),bookmarks=[...xml.matchAll(/<w:bookmarkStart\b[^>]*w:name="([^"]+)"/g)].map(m=>m[1]);assert.equal(new Set(bookmarks).size,bookmarks.length);
  const links=[...xml.matchAll(/<w:hyperlink\b[^>]*w:anchor="([^"]+)"/g)].map(m=>m[1]);for(const name of links.slice(0,5))assert.ok(bookmarks.includes(name),'exact bookmark exists');assert.equal(new Set(links.slice(0,5)).size,5);
  report.checks.push('DOCX package has distinct live bookmarks for duplicate, case/punctuation and Unicode ids; Microsoft Word reopening remains separately unverified');
  phase='actual PDF export and warning';response=await api('/api/export',{format:'pdf',content:source,baseDir:docs,options,out_path:pdf});assert.equal(response.status,200);assert.equal(response.data.ok,true);assert.ok(response.data.warn_items.some(item=>item.code==='pdf_anchor_missing'&&item.params.key==='absent'));
  const warning=await page.evaluate(data=>warnMessages(data),response.data);assert.ok(warning.some(s=>s.includes('absent')&&!s.includes('pdf_anchor_missing')));
  report.checks.push('actual PDF export reports missing anchors in localized warnings and retains visible link text');
  phase='install offline engine';response=await api('/api/capabilities/packs',{action:'install',path:process.env.READMD_PDF_PACK});assert.equal(response.status,200);assert.equal(response.data.installed,true);
  phase='independent PDFium destination resolution';response=await api('/api/pdf/workspace',{action:'open',path:pdf,width:1200});assert.equal(response.status,200);let snapshot=response.data;assert.ok(snapshot.pages>=3);assert.ok(snapshot.links.length>=6);assert.ok(snapshot.links.every(link=>link.target.kind==='internal'&&link.target.page>0));
  const body=await api('/api/pdf/workspace',{action:'render',session:snapshot.session,revision:snapshot.revision,page:1,width:1200});assert.equal(body.status,200);assert.ok(body.data.links.some(link=>link.target.page===2));assert.ok(body.data.links.every(link=>link.bounds.height>8));
  await api('/api/pdf/workspace',{action:'close',session:snapshot.session});
  report.checks.push('installed PDFium independently resolves real TOC/body destinations and full text-height click regions');
  phase='native PDF link clicks';assert.equal(await page.evaluate(pdf=>ReadMDPdf.open(pdf),pdf),true);await until(()=>page.locator('#pdf-hits .pdf-link').count());const first=page.getByRole('button',{name:'跳转到第 2 页',exact:true}).first();await first.click();await until(async()=>await page.locator('#pdf-page').inputValue()==='2');
  const next=page.getByRole('button',{name:'跳转到第 3 页',exact:true}).first();await next.click({modifiers:['Alt']});assert.equal(await page.locator('#pdf-context').isVisible(),true);assert.ok((await page.locator('#pdf-text').inputValue()).length);assert.equal(await page.locator('#pdf-save').isDisabled(),true);await page.keyboard.press('Escape');await next.click();await until(async()=>await page.locator('#pdf-page').inputValue()==='3');
  await page.keyboard.press('Alt+ArrowLeft');await until(async()=>await page.locator('#pdf-page').inputValue()==='2');await page.keyboard.press('Alt+ArrowLeft');await until(async()=>await page.locator('#pdf-page').inputValue()==='1');assert.equal(await page.locator('#pdf-save').isDisabled(),true);
  report.checks.push('real PDF panel follows TOC and cross-page links, Alt-click selects linked text, and Alt-Left returns without changing the file');
  phase='locale and compact layout';await page.evaluate(()=>i18n.setLanguage('en',false));assert.ok((await page.locator('#pdf-hits .pdf-link').first().getAttribute('aria-label')).startsWith('Go to page '));
  const cdp=await page.context().newCDPSession(page);for(const[width,height]of[[1024,680],[1160,820]]){await cdp.send('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false});assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));await page.screenshot({path:path.join(data,'links-'+width+'.png')});}await cdp.detach();
  await page.locator('#pdf-close').click();assert.equal(fs.readFileSync(file,'utf8'),source);assert.deepEqual(fs.readdirSync(docs).sort(),['Evidence-workshop.docx','Evidence-workshop.md','Evidence-workshop.pdf']);
  report.checks.push('dynamic link localization and compact layouts; no automatic source writes or sibling temporary files');report.passed=true;
}catch(error){report.failedStage=phase;report.error=error.name;report.message=String(error.message).replaceAll(root,'<workspace>').replaceAll(data,'<test-profile>').slice(0,800);if(native)await native.page.screenshot({path:path.join(data,'failure.png')}).catch(()=>{});throw error;}
finally{if(native){await native.browser.close().catch(()=>{});native.server.child.kill();}report.recordedAt=new Date().toISOString();fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,report:path.relative(root,path.join(data,'report.json')),failedStage:report.failedStage}));}
})().catch(()=>{console.error('Native publication links failed; scoped evidence retained.');process.exitCode=1;});
