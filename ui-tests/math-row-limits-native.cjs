'use strict';
// Real Windows host, formal CSP, actual glyph bounds and real save/export.
// Office application equivalence remains a separate acceptance gate.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),net=require('node:net');
const {chromium}=require('playwright'),{startNative}=require('./native-session.cjs'),zipParts=require('./zip-parts.cjs');
const root=path.resolve(__dirname,'..'),data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/math-row-limits-native-'));
const docs=path.join(data,'documents');fs.mkdirSync(docs);const file=path.join(docs,'Limits-and-spacing.md');
const hash=f=>crypto.createHash('sha256').update(fs.readFileSync(f)).digest('hex');
const matrix=value=>`matrix([x], [y], properties: office.mPr(office.rSpRule(val: 3), office.rSp(val: ${value})))`;
const equations=value=>`equations(x, y, properties: office.eqArrPr(office.rSpRule(val: 3), office.rSp(val: ${value})))`;
const block=s=>'```readmd-math v1\n'+s+'\n```';
const source=['# Mathematical layout workshop','','## Explicit inline limits','',
 ':math[sum(x, from: 0, to: 1, limits: above_below)] and :math[sum(x, from: 0, to: 1, limits: sub_sup)]','',
 '## Matrix spacing: zero and twelve points','',block(matrix(0)),'',block(matrix(12)),'',
 '## Equation array spacing: zero and twelve points','',block(equations(0)),'',block(equations(12)),'',
 '## Defaults distinguish summation from integration','',block('sum(x, from: 0, to: 1)'),' ',block('integral(x, from: 0, to: 1)'),''].join('\n');
const report={platform:'windows-x64',nativeCsp:true,synthetic:true,passed:false,checks:[],geometry:[],pageErrors:[],wordRoundTrip:'unverified'};
let native,page,stage='startup';
async function port(){const s=net.createServer();await new Promise(r=>s.listen(0,'127.0.0.1',r));const p=s.address().port;await new Promise(r=>s.close(r));return p;}
async function until(check){const end=Date.now()+45000;while(!await check()){assert(Date.now()<end,'Timed out: '+stage);await new Promise(r=>setTimeout(r,80));}}
async function api(route,body){return page.evaluate(async({route,body})=>{const r=await apiFetch(route,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});return{status:r.status,data:await r.json()};},{route,body});}
function measure(element){
 const svg=element.querySelector('[data-readmd-math-renderer="rust"] svg');if(!svg)throw Error('Shared Rust renderer required');
 const box=e=>{const r=e.getBoundingClientRect();return{x:r.x,y:r.y,width:r.width,height:r.height,right:r.right,bottom:r.bottom}};
 const union=elements=>{if(!elements.length)throw Error('Actual ink missing');const b=elements.map(box),x=Math.min(...b.map(v=>v.x)),y=Math.min(...b.map(v=>v.y)),right=Math.max(...b.map(v=>v.right)),bottom=Math.max(...b.map(v=>v.bottom));return{x,y,right,bottom,width:right-x,height:bottom-y};};
 const ink=svg.querySelector('[data-math-ink]'),rows=[...svg.querySelectorAll('[data-math-node="e"]')].map(e=>union([...ink.querySelectorAll('[data-math-owners~="'+e.id+'"]')])).sort((a,b)=>a.y-b.y);
 return{svg:box(svg),ink:box(ink),rows};
}
function contained(g){assert(g.ink.x>=g.svg.x-1&&g.ink.right<=g.svg.right+1&&g.ink.y>=g.svg.y-1&&g.ink.bottom<=g.svg.bottom+1,'Actual ink must fit SVG');}
const rowDistance=g=>{assert.equal(g.rows.length,2);return g.rows[1].y-g.rows[0].y;};
(async()=>{try{
 assert(process.env.READMD_BIN,'Explicit current binary required');process.env.READMD_WORKSPACE=docs;process.env.READMD_NATIVE_LOG_FILE=path.join(data,'native.log');fs.writeFileSync(file,source);
 report.binarySha256=hash(process.env.READMD_BIN);report.bootSha256=hash(path.join(root,'assets/readmd.boot.js'));report.scriptSha256=hash(__filename);
 native=await startNative(root,data,chromium,await port());page=native.page;page.on('pageerror',e=>report.pageErrors.push(e.message));
 await page.evaluate(()=>Promise.all([__readmdSessionReady,__readmdPreferencesReady]));await page.setViewportSize({width:1160,height:820});
 await page.evaluate(async f=>{await i18n.setLanguage('zh-CN',false);await loadFile(f);},file);
 await until(()=>page.locator('#content .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===8));
 stage='real inline limits';const formulas=page.locator('#content .readmd-dsl-math'),above=await formulas.nth(0).evaluate(measure),side=await formulas.nth(1).evaluate(measure);
 contained(above);contained(side);assert(above.ink.height>side.ink.height+2,'Explicit inline above/below must differ visibly');assert(above.ink.width<side.ink.width);
 report.geometry.push({kind:'inline-above',...above},{kind:'inline-side',...side});await formulas.nth(0).scrollIntoViewIfNeeded();await page.screenshot({path:path.join(data,'inline-limits-1160.png')});
 report.checks.push('Explicit inline above/below limits change actual painted height and width; both fit their viewport.');
 stage='LaTeX compatibility service';report.latex=[];for(const [text,display,location]of [
  ['\\sum_{i=0}^{n}',false,'subSup'],['\\sum_{i=0}^{n}',true,'undOvr'],
  ['\\prod_{i=0}^{n}',true,'undOvr'],['\\sum\\nolimits_{i=0}^{n}',true,'subSup'],
  ['\\int\\limits_0^1',false,'undOvr'],['x^{\\sum_{i=0}^{n}}',true,'subSup']]){
  const r=await api('/api/math/serialize',{source:text,dialect:'latex',display,target:'omml'});assert.equal(r.status,200,JSON.stringify(r.data));
  assert(r.data.output.includes('<m:limLoc m:val="'+location+'"/>'));report.latex.push({source:text,display,location,omml:r.data.output});
 }
 report.checks.push('The real shared service keeps ordinary LaTeX inline/script limits at the side, display sums/products above/below, and explicit limits/nolimits authoritative.');
 stage='actual row gaps';for(const [kind,index]of [['matrix',2],['equations',4]]){
  const zero=await formulas.nth(index).evaluate(measure),points=await formulas.nth(index+1).evaluate(measure);contained(zero);contained(points);
  report.geometry.push({kind:kind+'-zero',...zero},{kind:kind+'-12pt',...points});
  const delta=rowDistance(points)-rowDistance(zero);assert(Math.abs(delta-16)<1,kind+': 12pt must produce 16 CSS pixels, observed '+delta);
  await formulas.nth(index+1).scrollIntoViewIfNeeded();await page.screenshot({path:path.join(data,kind+'-12pt-1160.png')});
 }
 report.checks.push('Matrix and equation-array 12pt gaps change actual row ink separation by 16 CSS pixels.');
 stage='real advanced property edit';await page.evaluate(async()=>{await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');});
 const target=page.locator('#preview-pane .readmd-dsl-math [data-readmd-math-renderer="rust"]').nth(3);await target.waitFor();await target.click();await until(()=>page.locator('.formula-inline-controls button').first().isEnabled());
 await page.locator('.formula-properties > summary').click();await page.locator('[data-math-container="mPr"] > summary').click();const input=page.locator('[data-math-focus="mPr.rSp"]');await input.fill('24');await input.press('Tab');
 await until(()=>page.locator('[data-math-focus="mPr.rSp"]').inputValue().then(v=>v==='24'));await until(()=>page.locator('.formula-inline-controls button').first().isEnabled());
 await page.locator('.formula-inline-controls button').first().click();const changed=await page.evaluate(()=>getEditContent());
 assert.notEqual(changed,source);let ranges=await page.evaluate(()=>ReadMDFormula.ranges(getEditContent()));assert.equal(ranges[2].tex,matrix(0));assert(ranges[3].tex.includes('24'));assert.equal(ranges[5].tex,equations(12));
 await page.keyboard.press('Control+z');assert.equal(await page.evaluate(()=>getEditContent()),source);await page.keyboard.press('Control+y');assert.equal(await page.evaluate(()=>getEditContent()),changed);
 report.checks.push('The actual advanced property input changes only the intended matrix; one body undo/redo restores exact source.');
 stage='save reopen';assert.equal(await page.evaluate(()=>saveEdit()),true);assert.equal(fs.readFileSync(file,'utf8'),changed);assert.deepEqual(fs.readdirSync(docs),['Limits-and-spacing.md']);
 await page.evaluate(async f=>{await exitEdit();await loadFile(f,{force:true,discardConfirmed:true});},file);await until(()=>page.locator('#content .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===8));assert.equal(await page.evaluate(()=>hasUnsavedEditorChanges()),false);
 const reopened=await page.locator('#content .readmd-dsl-math').nth(3).evaluate(measure),zero=await page.locator('#content .readmd-dsl-math').nth(2).evaluate(measure);assert(Math.abs(rowDistance(reopened)-rowDistance(zero)-32)<1);
 report.geometry.push({kind:'reopened-matrix-24pt',...reopened});report.checks.push('Atomic save and native reopen retain the 24pt painted gap, with no side files or dirty state.');
 stage='real publication';report.outputs={};for(const format of ['docx','epub','pdf']){
  const out_path=path.join(docs,'Limits-and-spacing.'+format),r=await api('/api/export',{format,content:changed,baseDir:docs,options:{cover:{enabled:false},toc:{enabled:false},pdf:{engine:'rust'}},out_path});
  assert.equal(r.status,200,JSON.stringify(r.data));assert.equal(r.data.ok,true,JSON.stringify(r.data));assert(fs.statSync(out_path).size>1000);report.outputs[format]=path.relative(data,out_path);
 }
 const word=zipParts(fs.readFileSync(path.join(data,report.outputs.docx))).get('word/document.xml');assert.match(word,/<m:rSp m:val="24"\/>/);assert.match(word,/<m:limLoc m:val="undOvr"\/>/);fs.writeFileSync(path.join(data,'document.xml'),word);
 const epub=[...zipParts(fs.readFileSync(path.join(data,report.outputs.epub)))].filter(([n])=>n.endsWith('.xhtml')).map(([,v])=>v).join('');assert(epub.includes('rowspacing="24pt"'));assert(epub.includes('movablelimits="false"'));fs.writeFileSync(path.join(data,'epub-math.xhtml'),epub);
 report.checks.push('Actual DOCX keeps editable limit/spacing properties, EPUB includes fixed limits and point gaps, and Rust emits a real PDF.');
 stage='dark narrow renderer';await page.setViewportSize({width:1024,height:680});await page.evaluate(()=>{state.theme='dark';applySettings();});await page.locator('#content .readmd-dsl-math').nth(3).scrollIntoViewIfNeeded();await page.screenshot({path:path.join(data,'matrix-dark-1024.png')});
 contained(await page.locator('#content .readmd-dsl-math').nth(3).evaluate(measure));assert.deepEqual(report.pageErrors,[]);report.passed=true;
}catch(e){report.failedStage=stage;report.error=String(e.message).replaceAll(root,'<workspace>').slice(0,1800);if(page)await page.screenshot({path:path.join(data,'failure.png')}).catch(()=>{});process.exitCode=1;}
finally{if(native){await native.browser.close().catch(()=>{});native.server.child.kill();}report.recordedAt=new Date().toISOString();report.scope='Local OMML limits and row-spacing semantics, actual native ink/edit/undo/save/reopen/export. Document-level mathPr, Word re-edit, complete OMML and full batch acceptance remain unverified.';fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,report:path.relative(root,path.join(data,'report.json')),failedStage:report.failedStage}));}
})();
