'use strict';
// Current native Windows binary, formal CSP, real paths and real publication.
// Uses owned synthetic material only. Actual Office/independent EPUB readers
// remain separate gates, never inferred from archive validity.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),net=require('node:net');
const {chromium}=require('playwright'),{startNative}=require('./native-session.cjs'),zipParts=require('./zip-parts.cjs');
const root=path.resolve(__dirname,'..'),data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/math-matrix-semantics-native-'));
const docs=path.join(data,'documents');fs.mkdirSync(docs);const file=path.join(docs,'Matrix-workshop.md');
const hash=f=>crypto.createHash('sha256').update(fs.readFileSync(f)).digest('hex');
const matrix=(width,text='x')=>`matrix([${text}, ${text}], align: [left, left], column_gap: 6pt, properties: office.mPr(office.cSp(val: ${width})))`;
const slots=hide=>`matrix([x, empty()], [empty(), y], properties: office.mPr(office.cSp(val: 720)${hide?', office.plcHide()':''}))`;
const block=s=>'```readmd-math v1\n'+s+'\n```';
const sources=[matrix(0),matrix(720),matrix(0,'abcdefghijklmnop'),matrix(720,'abcdefghijklmnop'),slots(false),slots(true),matrix(720),matrix(720)];
const source=['# Matrix layout workshop','','Minimum width controls the column, not the gap. Long expressions retain their full natural width.','','## Natural and 36-point columns','',...sources.slice(0,2).flatMap(s=>[block(s),'']),'## Wider content remains intact','',...sources.slice(2,4).flatMap(s=>[block(s),'']),'## Empty cells: visible and hidden','',...sources.slice(4,6).flatMap(s=>[block(s),'']),'## Repeated formulas have independent identities','',...sources.slice(6).flatMap(s=>[block(s),''])].join('\n');
const report={platform:'windows-x64',nativeCsp:true,synthetic:true,passed:false,checks:[],geometry:[],pageErrors:[],wordRoundTrip:'unverified',independentEpubReader:'unverified'};
let host,page,stage='startup';
async function port(){const s=net.createServer();await new Promise(r=>s.listen(0,'127.0.0.1',r));const p=s.address().port;await new Promise(r=>s.close(r));return p;}
async function stopNative(){if(!host)return;const owned=host;host=null;await owned.browser.close().catch(()=>{});const child=owned.server.child;if(child.exitCode===null&&child.signalCode===null){await new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(Error('Owned native process did not stop')),5000);child.once('exit',()=>{clearTimeout(timer);resolve();});child.kill();});}}
async function until(check){const end=Date.now()+45000;while(!await check()){assert(Date.now()<end,'Timed out: '+stage);await new Promise(r=>setTimeout(r,80));}}
async function api(route,body){return page.evaluate(async({route,body})=>{const r=await apiFetch(route,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});return{status:r.status,data:await r.json()};},{route,body});}
function geometry(element){
 const svg=element.querySelector('[data-readmd-math-renderer="rust"] svg');if(!svg)throw Error('Shared Rust SVG required');
 const rect=e=>{const b=e.getBoundingClientRect();return{x:b.x,y:b.y,width:b.width,height:b.height,right:b.right,bottom:b.bottom};};
 const ink=svg.querySelector('[data-math-ink]');
 const cells=[...svg.querySelectorAll('[data-math-node="e"]')].map(e=>({id:e.id,rect:rect(e),ink:[...ink.querySelectorAll('[data-math-owners~="'+e.id+'"]')].map(rect)}));
 return{svg:rect(svg),ink:rect(ink),cells,lines:ink.querySelectorAll('path[stroke]').length,paths:ink.querySelectorAll('path:not([stroke])').length};
}
function near(x,y){assert(Math.abs(x-y)<0.7,`${x} != ${y}`);}
async function generatedMathml(dsl){const r=await api('/api/math/serialize',{source:dsl,dialect:'dsl',display:true,target:'mathml'});assert.equal(r.status,200,JSON.stringify(r.data));return r.data.output;}
async function mathmlGeometry(markup){return page.evaluate(markup=>{
 const el=document.createElement('div');el.style.cssText='position:fixed;left:0;top:0;z-index:99999;background:white;color:black;font-size:16px';el.innerHTML=markup;document.body.append(el);
 try{const cells=[...el.querySelectorAll('mtd')].map(c=>{const b=c.getBoundingClientRect(),ink=c.firstElementChild.getBoundingClientRect();return{x:b.x,contentX:ink.x,w:b.width,inkWidth:ink.width,minWidth:getComputedStyle(c).minWidth};});return{cells,placeholders:el.querySelectorAll('mspace[style*="outline"]').length,text:el.textContent};}finally{el.remove();}
 },markup);}
async function openFixture(){await page.evaluate(async f=>{await i18n.setLanguage('zh-CN',false);await loadFile(f);},file);await until(()=>page.locator('#content .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===8));}
(async()=>{try{
 assert(process.env.READMD_BIN&&process.env.READMD_FONTS_PACK&&process.env.READMD_PDF_PACK&&process.env.READMD_EPUBCHECK_JAR,'Explicit binary and locked offline packs required');
 process.env.READMD_WORKSPACE=docs;process.env.READMD_NATIVE_LOG_FILE=path.join(data,'native.log');fs.writeFileSync(file,source);
 report.binarySha256=hash(process.env.READMD_BIN);report.bootSha256=hash(path.join(root,'assets/readmd.boot.js'));report.scriptSha256=hash(__filename);
 host=await startNative(root,data,chromium,await port());page=host.page;page.on('pageerror',e=>report.pageErrors.push(e.message));
 await page.evaluate(()=>Promise.all([__readmdSessionReady,__readmdPreferencesReady]));await page.setViewportSize({width:1160,height:820});
 for(const[id,key]of [['fonts','READMD_FONTS_PACK'],['pdfium','READMD_PDF_PACK']])assert.equal((await api('/api/capabilities/packs',{action:'install',id,path:process.env[key]})).data.ok,true);
 await openFixture();
 stage='painted matrix geometry';const formulas=page.locator('#content .readmd-dsl-math');
 for(let i=0;i<8;i++)report.geometry.push(await formulas.nth(i).evaluate(geometry));
 const g=report.geometry;assert.equal(g[1].cells.length,2);const scale=g[1].svg.width/(78/12+0.02); // actual formula em, including antialias allowance
 near(g[1].cells[1].rect.x-g[1].cells[0].rect.x,42/12*scale);
 near(g[2].svg.width,g[3].svg.width);assert(g[3].svg.width>g[1].svg.width);
 for(const item of g){assert(item.ink.x>=item.svg.x-1&&item.ink.right<=item.svg.right+1,'Actual ink contained');}
 report.checks.push('Actual native vector cells observe 36pt minimum plus a separate 6pt gap; wide content is not clipped.');
 assert.equal(g[4].lines,8);assert.equal(g[5].lines,0);assert.equal(g[4].paths,2);assert.equal(g[5].paths,2);near(g[4].svg.width,g[5].svg.width);
 assert(g[5].cells.every(c=>c.rect.width>0),'Hidden empty slots remain addressable');
 report.checks.push('Two visible empty cells paint only thin lines; plcHide removes their ink while preserving geometry and source targets.');
 await formulas.nth(4).scrollIntoViewIfNeeded();await page.screenshot({path:path.join(data,'matrix-1160-light.png')});
 stage='generated MathML geometry';report.mathml=[];
 for(const i of [0,1,2,3,4,5]){const markup=await generatedMathml(sources[i]);fs.writeFileSync(path.join(data,`mathml-${i}.xml`),markup);report.mathml[i]=await mathmlGeometry(markup);}
 near(report.mathml[1].cells[1].contentX-report.mathml[1].cells[0].contentX,56);assert.equal(report.mathml[1].cells[0].minWidth,'48px');
 near(report.mathml[2].cells[1].contentX-report.mathml[2].cells[0].contentX,report.mathml[3].cells[1].contentX-report.mathml[3].cells[0].contentX);
 assert.equal(report.mathml[4].placeholders,2);assert.equal(report.mathml[5].placeholders,0);assert.equal(report.mathml[4].text,report.mathml[5].text);
 report.checks.push('The actual service-generated MathML uses a physical CSS minimum, preserves over-wide content and adds no placeholder text.');
 stage='precise property edit';await page.evaluate(async()=>{await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');});
 const target=page.locator('#preview-pane .readmd-dsl-math [data-readmd-math-renderer="rust"]').nth(7);await target.waitFor();await target.click();
 await until(()=>page.locator('.formula-inline-controls button').first().isEnabled());await page.locator('.formula-properties > summary').click();await page.locator('[data-math-container="mPr"] > summary').click();
 const input=page.locator('[data-math-focus="mPr.cSp"]');await input.fill('960');await input.press('Tab');await until(()=>input.inputValue().then(v=>v==='960'));await until(()=>page.locator('.formula-inline-controls button').first().isEnabled());await page.locator('.formula-inline-controls button').first().click();
 const changed=await page.evaluate(()=>getEditContent()),ranges=await page.evaluate(()=>ReadMDFormula.ranges(getEditContent()));assert.notEqual(changed,source);assert.equal(ranges[6].tex,matrix(720));assert(ranges[7].tex.includes('960'));
 await page.keyboard.press('Control+z');assert.equal(await page.evaluate(()=>getEditContent()),source);await page.keyboard.press('Control+y');assert.equal(await page.evaluate(()=>getEditContent()),changed);
 report.checks.push('The native advanced property changes only the second duplicate formula; a single body undo/redo restores exact source.');
 stage='save and reload';assert.equal(await page.evaluate(()=>saveEdit()),true);assert.equal(fs.readFileSync(file,'utf8'),changed);assert.deepEqual(fs.readdirSync(docs),['Matrix-workshop.md']);
 await page.evaluate(async f=>{await exitEdit();await loadFile(f,{force:true,discardConfirmed:true});},file);await until(()=>page.locator('#content .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===8));
 assert.equal(await page.evaluate(()=>hasUnsavedEditorChanges()),false);report.reopened=await formulas.nth(7).evaluate(geometry);assert(report.reopened.svg.width>g[7].svg.width);
 report.checks.push('Atomic save and native reload retain the changed width without side files or dirty state.');
 stage='edit a hidden empty cell';await page.evaluate(async()=>{await toggleEdit();setPvLayout('right');});
 await page.locator('#preview-pane .readmd-dsl-math [data-readmd-math-renderer="rust"]').nth(5).click();
 const emptyCell=page.locator('.formula-inline [data-math-node="e"][role="button"]').nth(1);await emptyCell.waitFor();await emptyCell.click();
 const slotInput=page.locator('.formula-inline-input');await until(()=>slotInput.isEnabled());await slotInput.fill('z');await slotInput.press('Enter');await until(()=>page.locator('.formula-inline-controls button').first().isEnabled());await page.locator('.formula-inline-controls button').first().click();
 const slotRanges=await page.evaluate(()=>ReadMDFormula.ranges(getEditContent()));assert.equal(slotRanges[4].tex,sources[4]);assert(slotRanges[5].tex.includes('z'));
 await page.keyboard.press('Control+z');assert.equal(await page.evaluate(()=>getEditContent()),changed);assert.equal(fs.readFileSync(file,'utf8'),changed);await page.evaluate(()=>exitEdit());
 report.checks.push('A hidden empty slot can actually be clicked, filled and applied; body undo restores the exact saved source without a disk write.');
 stage='actual publication';report.outputs={};for(const format of ['docx','epub','pdf']){
  const out_path=path.join(docs,'Matrix-workshop.'+format),r=await api('/api/export',{format,content:changed,baseDir:docs,options:{cover:{enabled:false},toc:{enabled:false},pdf:{engine:'rust'}},out_path});assert.equal(r.status,200,JSON.stringify(r.data));assert.equal(r.data.ok,true,JSON.stringify(r.data));assert(fs.statSync(out_path).size>1000);report.outputs[format]=path.relative(data,out_path);
 }
 const word=zipParts(fs.readFileSync(path.join(data,report.outputs.docx))).get('word/document.xml');assert(word.includes('<m:cSp m:val="960"/>'));assert(word.includes('<m:plcHide/>'));fs.writeFileSync(path.join(data,'document.xml'),word);
 const epub=[...zipParts(fs.readFileSync(path.join(data,report.outputs.epub)))].filter(([n])=>n.endsWith('.xhtml')).map(([,v])=>v).join('');assert(epub.includes('min-width:48pt'));assert(epub.includes('outline:0.04em'));fs.writeFileSync(path.join(data,'epub-math.xhtml'),epub);
 const {checkEpub}=await import('../tools/check-epub.mjs');assert.equal((await checkEpub([path.join(data,report.outputs.epub)],{jar:process.env.READMD_EPUBCHECK_JAR,output:path.join(data,'epubcheck.json')})).passed,true);
 const pdf=(await api('/api/pdf/workspace',{action:'open',path:path.join(data,report.outputs.pdf),width:1200})).data;assert.equal(pdf.ok,true,JSON.stringify(pdf));assert(pdf.pages>0&&pdf.image);fs.writeFileSync(path.join(data,'matrix-page-0.bmp'),Buffer.from(pdf.image,'base64'));await api('/api/pdf/workspace',{action:'close',session:pdf.session});
 report.checks.push('Actual DOCX retains native properties, EPUB passes locked offline EPUBCheck, and the real PDF reopens and renders through PDFium.');
 stage='small dark layout';await page.setViewportSize({width:1024,height:680});await page.evaluate(()=>{document.documentElement.dataset.theme='dark';document.body.dataset.theme='dark';});await formulas.nth(4).scrollIntoViewIfNeeded();await page.screenshot({path:path.join(data,'matrix-1024-dark.png')});
 stage='real process restart';await page.evaluate(()=>ReadMDSession.flush());await stopNative();host=await startNative(root,data,chromium,await port());page=host.page;page.on('pageerror',e=>report.pageErrors.push(e.message));await page.evaluate(()=>Promise.all([__readmdSessionReady,__readmdPreferencesReady]));await openFixture();
 assert.equal(fs.readFileSync(file,'utf8'),changed);assert.equal(await page.evaluate(()=>hasUnsavedEditorChanges()),false);assert((await page.locator('#content .readmd-dsl-math').nth(7).evaluate(geometry)).svg.width>g[7].svg.width);assert.deepEqual(report.pageErrors,[]);
 report.checks.push('A real owned-process restart retains saved formulas and addressable hidden slots under formal CSP.');report.passed=true;
}catch(error){report.failedStage=stage;report.error=error.stack;process.exitCode=1;}
finally{report.recordedAt=new Date().toISOString();fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2));try{await stopNative();}catch(error){report.passed=false;report.cleanupError=error.stack;process.exitCode=1;fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2));}console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,report:path.relative(root,path.join(data,'report.json')),failedStage:report.failedStage}));}
})();
