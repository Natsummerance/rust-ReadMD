'use strict';
// Synthetic files, actual Rust exports and the packaged CSP. No app API mocks.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const assert = require('node:assert/strict'), net = require('node:net');
const { chromium } = require('playwright'), { startNative } = require('./native-session.cjs');
const zipParts = require('./zip-parts.cjs');
const root = path.resolve(__dirname, '..');
const data = fs.mkdtempSync(path.join(root, '.cache/capability-upgrade/publication-locale-toc-native-'));
const docs = path.join(data, 'documents'); fs.mkdirSync(docs);
const hash = f => crypto.createHash('sha256').update(fs.readFileSync(f)).digest('hex');
const source = '---\ntitle: Evidence and interpretation\nlanguage: de_DE\nbibliography: references.json\ncsl: dates.csl\n---\n# Evidence\n\nSource [@study].\n\n## Observation\n\nA repeatable observation supplies the evidence for a claim. We distinguish a measurement from its interpretation and record the assumptions used to compare alternative explanations.\n\n### Comparison\n\nAn independent example should challenge the explanation rather than merely repeat it. This is a substantive third-level section for navigation and outline verification.\n\n#### Detailed method\n\nThis paragraph belongs in the book even when the table of contents is restricted to three levels. We keep the source file unchanged while changing publication parameters.\n\n## Discussion\n\n| Criterion | Observation | Decision |\n| --- | --- | --- |\n| Repeatability | Independent material | Preserve evidence |\n| Clarity | Explicit assumptions | Revise interpretation |\n\n### Limitations\n\nThe conclusion is provisional and describes the limits of the evidence.\n';
const file = path.join(docs, 'Research.md'); fs.writeFileSync(file, source);
fs.writeFileSync(path.join(docs, 'references.json'), JSON.stringify([{id:'study',type:'book',title:'Evidence',author:[{family:'Research',given:'ReadMD'}],issued:{'date-parts':[[2026,1,13]]}}]));
fs.writeFileSync(path.join(docs, 'dates.csl'), '<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text"><info><title>Locale test</title><id>urn:readmd:test:native-locale</id><updated>2026-10-13T00:00:00Z</updated></info><citation><layout><date variable="issued"><date-part name="month" form="long"/></date></layout></citation></style>');
const report = {passed:false,synthetic:true,formalCsp:true,platform:'windows-x64',checks:[],outputs:[],pageErrors:[],wordApplication:'unverified',independentEpubReader:'unverified'};
let host, page, phase = 'startup';
async function freePort() {const s=net.createServer();await new Promise(r=>s.listen(0,'127.0.0.1',r));const p=s.address().port;await new Promise(r=>s.close(r));return p;}
async function stop() {if(!host)return;await host.browser.close().catch(()=>{});const c=host.server.child;if(c.exitCode===null){const ended=new Promise(r=>c.once('exit',r));c.kill();await Promise.race([ended,new Promise(r=>setTimeout(r,5000))]);}host=null;}
const api = (route, body) => page.evaluate(async ({route,body})=>{const r=await apiFetch(route,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});return{status:r.status,data:await r.json()};},{route,body});
const options = {cover:{enabled:false},toc:{enabled:true,depth:3,leader:'hyphen'},epub:{splitLevel:'h1'},typography:{font:'Arial',size:11}};
async function output(format,name,extra={}) {const out=path.join(docs,name+'.'+format);const r=await api('/api/export',{format,content:source,baseDir:docs,out_path:out,options:{...options,...extra}});assert.equal(r.data.ok,true,JSON.stringify(r.data));report.outputs.push({format,path:path.relative(data,out),sha256:hash(out),warnings:r.data.warns});return out;}
async function xmlValues(xml,kind) {return page.evaluate(({xml,kind})=>{const d=new DOMParser().parseFromString(xml,'application/xml');if(d.querySelector('parsererror'))return{invalid:true};if(kind==='word'){const w='http://schemas.openxmlformats.org/wordprocessingml/2006/main';return{instructions:[...d.getElementsByTagNameNS(w,'instrText')].map(n=>n.textContent),text:[...d.getElementsByTagNameNS(w,'t')].map(n=>n.textContent).join(' ')};}if(kind==='opf')return{language:d.getElementsByTagNameNS('http://purl.org/dc/elements/1.1/','language')[0]?.textContent};return{language:d.documentElement.getAttribute('lang'),text:d.documentElement.textContent,depths:[...d.querySelectorAll('nav a')].map(a=>{let level=0,n=a;while(n){if(n.localName==='ol')level++;n=n.parentElement;}return{text:a.textContent,level,href:a.getAttribute('href')};})};},{xml,kind});}
(async()=>{try{
  assert(process.env.READMD_BIN && process.env.READMD_FONTS_PACK && process.env.READMD_TYPST_PACK && process.env.READMD_PDF_PACK);
  process.env.READMD_WORKSPACE=docs;report.binarySha256=hash(process.env.READMD_BIN);report.bootSha256=hash(path.join(root,'assets/readmd.boot.js'));report.scriptSha256=hash(__filename);
  host=await startNative(root,data,chromium,await freePort());page=host.page;page.on('pageerror',e=>report.pageErrors.push(e.message));
  await page.evaluate(()=>Promise.all([__readmdSessionReady,__readmdPreferencesReady]));
  for(const [id,key] of [['fonts','READMD_FONTS_PACK'],['typst','READMD_TYPST_PACK'],['pdfium','READMD_PDF_PACK']])assert.equal((await api('/api/capabilities/packs',{action:'install',id,path:process.env[key]})).data.ok,true);
  phase='shared citation API';
  for(const [extra,locale,month] of [[{},'de-DE','Januar'],[{meta:{language:'fr_CA'}},'fr-CA','janvier'],[{meta:{language:'invalid--tag'}},'de-DE','Januar']]){
    const r=(await api('/api/academic/document',{content:source,baseDir:docs,options:extra})).data;
    assert.equal(r.ok,true,JSON.stringify(r));assert.equal(r.locale,locale);assert(r.content.includes(month));assert(r.content.includes('language: de_DE'));
  }
  report.checks.push('Real CSL endpoint shares document language and explicit export overrides; invalid overrides fall back to valid source metadata, without changing the author declaration.');
  phase='actual export settings';
  await page.setViewportSize({width:1160,height:820});
  await page.evaluate(async f=>{await i18n.setLanguage('zh-CN',false);await loadFile(f);await openExportModal();},file);
  await page.locator('#export-box .exp-fmt[data-fmt="docx"]').click();
  const preset=page.locator('#exp-preset'),presetDetails=preset.locator('xpath=ancestor::details[1]');
  if(!await presetDetails.evaluate(e=>e.open))await presetDetails.locator(':scope > summary').click();
  await preset.selectOption('__default__');
  const customize=page.locator('#export-box .export-customize');
  if(!await customize.evaluate(e=>e.open))await customize.locator(':scope > summary').click();
  const section=page.locator('#export-box .exp-sec').filter({has:page.locator('[data-k="toc.enabled"]')});
  if(!await section.evaluate(e=>e.classList.contains('open')))await section.locator('.exp-sec-head').click();
  if(!await page.locator('[data-k="toc.enabled"]').isChecked())await page.locator('[data-k="toc.enabled"]').check();
  const controls=await page.evaluate(()=>[...document.querySelectorAll('#export-box [data-k]')].filter(e=>/toc\./.test(e.dataset.k)).map(e=>({key:e.dataset.k,tag:e.tagName})));
  report.tocControls=controls;
  assert(controls.some(c=>c.key==='toc.depth')&&controls.some(c=>c.key==='toc.leader'));
  // An explicit change away from, then back to the optional default must persist.
  await page.locator('[data-k="toc.depth"]').fill('4');
  await page.locator('[data-k="toc.depth"]').fill('3');
  await page.locator('[data-k="toc.leader"]').selectOption('hyphen');
  const values=await page.evaluate(()=>collectExportOptions());
  await page.locator('#exp-save-preset').click();await page.locator('#exp-save-input').fill('Research outline');await page.locator('#exp-save-ok').click();await page.locator('#exp-save-name').waitFor({state:'hidden'});
  assert.equal(values.toc.depth,3);assert.equal(values.toc.leader,'hyphen');
  options.toc={...values.toc}; // Export the actual values collected from the controls.
  await page.screenshot({path:path.join(data,'export-1160.png')});
  report.checks.push('Existing export panel retains directory depth and leader values through DOM synchronization and collection, with no additional workspace UI.');
  phase='native Word field output';
  const word=zipParts(fs.readFileSync(await output('docx','Research-French',{meta:{language:'fr_CA'}})));
  const main=await xmlValues(word.get('word/document.xml'),'word');assert(main.text.includes('janvier'));assert(main.instructions.some(s=>/TOC.*1-3/.test(s)),JSON.stringify(main.instructions));
  assert(word.get('word/styles.xml').includes('w:leader="hyphen"'));
  report.checks.push('Actual DOCX contains French CSL output, a live TOC field limited to levels 1–3 and native hyphen leader tabs; Word application pagination is explicitly separate.');
  phase='actual EPUB hierarchy and language';
  const epubFile=await output('epub','Research-French-book',{meta:{language:'de-DE'},epub:{language:'fr_CA',splitLevel:'h1'}});
  const epub=zipParts(fs.readFileSync(epubFile));
  assert.equal((await xmlValues(epub.get('OEBPS/content.opf'),'opf')).language,'fr-CA');
  const navEntry=[...epub].find(([n])=>n.endsWith('nav.xhtml'));assert(navEntry);
  const nav=await xmlValues(navEntry[1],'xhtml');assert(nav.depths.some(n=>n.text.includes('Comparison')&&n.level>=3),JSON.stringify(nav.depths));assert(!nav.depths.some(n=>n.text.includes('Detailed method')));
  const chapters=[...epub].filter(([n])=>n.endsWith('.xhtml')&&!n.endsWith('nav.xhtml'));
  assert(chapters.some(([,text])=>text.includes('janvier')));assert(chapters.some(([,text])=>text.includes('Detailed method')));
  const {checkEpub}=await import('../tools/check-epub.mjs');
  assert.equal((await checkEpub([epubFile],{jar:process.env.READMD_EPUBCHECK_JAR,output:path.join(data,'epubcheck.json')})).passed,true);
  report.checks.push('Real EPUB uses its format-specific language for both citations and OPF, includes nested heading navigation through level three, preserves deeper body content, and passes offline EPUBCheck.');
  phase='real managed pagination';
  report.pdfLeaderVariants=[];
  for(const leader of ['dot','hyphen','underscore','none']) {
    const pdf=await output('pdf','Research-French-paper-'+leader,{meta:{language:'fr_CA'},pdf:{engine:'typst'},toc:{...options.toc,leader}});
    const opened=(await api('/api/pdf/workspace',{action:'open',path:pdf,width:1100})).data;
    assert.equal(opened.ok,true,JSON.stringify(opened));const texts=[];
    try {
      for(let index=0;index<opened.pages;index++) {
        const rendered=index===0?opened:(await api('/api/pdf/workspace',{action:'render',session:opened.session,revision:opened.revision,page:index,width:1100})).data;
        assert.equal(rendered.ok,true,JSON.stringify(rendered));assert(rendered.image);
        texts.push(rendered.objects.filter(o=>o.kind==='text').map(o=>o.text).join(' '));
        if(index===0)fs.writeFileSync(path.join(data,'actual-pdf-'+leader+'-page.bmp'),Buffer.from(rendered.image,'base64'));
      }
    } finally {await api('/api/pdf/workspace',{action:'close',session:opened.session});}
    assert(texts.join(' ').includes('janvier'),leader+' PDF loses localized citation');
    assert(texts.join(' ').includes('Detailed method'),leader+' PDF loses deeper body content');
    report.pdfLeaderVariants.push({leader,pages:opened.pages,sha256:hash(pdf),text:texts});
  }
  assert.equal(new Set(report.pdfLeaderVariants.map(p=>p.sha256)).size,4);
  report.checks.push('All four leader styles compile through the real managed offline Typst engine, then PDFium renders every page: localized references and deeper body sections survive, with four distinct output artifacts.');
  phase='small-window and restart';
  await page.setViewportSize({width:1024,height:680});await page.evaluate(()=>i18n.setLanguage('en',false));await page.screenshot({path:path.join(data,'export-1024.png')});
  await page.locator('#export-close').click();await page.evaluate(()=>ReadMDSession.flush());await stop();
  host=await startNative(root,data,chromium,await freePort());page=host.page;await page.evaluate(()=>__readmdSessionReady);await page.evaluate(f=>loadFile(f),file);
  assert.equal(await page.evaluate(()=>state.original),source);
  const presets=await page.evaluate(async()=>{const r=await apiFetch('/api/export/presets');return r.json();});
  assert.equal(presets.custom['Research outline'].toc.depth,3);assert.equal(presets.custom['Research outline'].toc.leader,'hyphen');
  assert.equal(fs.readFileSync(file,'utf8'),source);assert(!fs.existsSync(path.join(docs,'readmd-assets')));assert.deepEqual(report.pageErrors,[]);
  report.checks.push('Real process restart retains the original author file and reference inputs with no shared resource directory or Markdown copies.');
  report.passed=true;
}catch(e){report.failedStage=phase;report.error=String(e.stack||e).replaceAll(root,'<workspace>').slice(0,2600);if(page)await page.screenshot({path:path.join(data,'failure.png')}).catch(()=>{});process.exitCode=1;
}finally{await stop();report.recordedAt=new Date().toISOString();report.scope='Limited CSL-language and TOC-parameter increment. No certification of full RTL shaping, full CSL coverage, native Word layout or independent reading applications.';fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,report:path.relative(root,path.join(data,'report.json')),failedStage:report.failedStage}));}
})();
