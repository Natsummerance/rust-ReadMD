'use strict';
// Owned Word inputs and the actual Windows CSP host; no API mocks.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),net=require('node:net');
const {chromium}=require('playwright'),{startNative}=require('./native-session.cjs'),zipParts=require('./zip-parts.cjs');
const root=path.resolve(__dirname,'..'),data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/math-small-fractions-native-')),docs=path.join(data,'documents');fs.mkdirSync(docs);
const file=path.join(docs,'Small-fractions.md'),report={platform:'windows-x64',nativeCsp:true,synthetic:true,passed:false,checks:[],geometry:[],pageErrors:[],wordRoundTrip:'unverified',epubReader:'unverified'};
const hash=f=>crypto.createHash('sha256').update(fs.readFileSync(f)).digest('hex');let native,page,stage='startup';
async function port(){const s=net.createServer();await new Promise(r=>s.listen(0,'127.0.0.1',r));const p=s.address().port;await new Promise(r=>s.close(r));return p;}
async function until(check){const end=Date.now()+45000;while(!await check()){assert(Date.now()<end,'Timed out: '+stage);await new Promise(r=>setTimeout(r,80));}}
async function api(route,body){return page.evaluate(async({route,body})=>{const r=await apiFetch(route,body===undefined?{}:{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});return{status:r.status,data:await r.json()};},{route,body});}
function ink(element){const svg=element.querySelector('svg'),r=svg.querySelector('[data-math-ink]').getBoundingClientRect(),s=svg.getBoundingClientRect();return{width:r.width,height:r.height,x:r.x,y:r.y,right:r.right,bottom:r.bottom,svg:{x:s.x,y:s.y,right:s.right,bottom:s.bottom}};}
function contained(g){assert(g.x>=g.svg.x-1&&g.y>=g.svg.y-1&&g.right<=g.svg.right+1&&g.bottom<=g.svg.bottom+1,'Painted bounds fit the SVG');}
async function ready(){await until(()=>page.locator('.formula-inline-controls button').first().isEnabled());}
(async()=>{try{
 assert(process.env.READMD_BIN);process.env.READMD_WORKSPACE=docs;process.env.READMD_NATIVE_LOG_FILE=path.join(data,'native.log');
 report.binarySha256=hash(process.env.READMD_BIN);report.bootSha256=hash(path.join(root,'assets/readmd.boot.js'));report.scriptSha256=hash(__filename);
 require('./helpers/word-fixtures.cjs').smallFractions(docs);
 native=await startNative(root,data,chromium,await port());page=native.page;page.on('pageerror',e=>report.pageErrors.push(e.message));
 await page.evaluate(()=>Promise.all([__readmdSessionReady,__readmdPreferencesReady]));await page.setViewportSize({width:1160,height:820});await page.evaluate(()=>i18n.setLanguage('en',false));
 stage='actual Word settings import';let source='# Fraction typography\n\n';
 for(const name of ['Default','Small','Normal']){const input=path.join(docs,name+'.docx'),before=hash(input),r=await api('/api/convert?p='+encodeURIComponent(input)+'&preview=1');assert.equal(r.status,200,JSON.stringify(r.data));assert(!r.data.saved);assert.equal(hash(input),before);source+='## '+name+' display fraction\n\n'+r.data.content+'\n\n';}
 const inline=(property)=>':math[row(office.mathPr('+property+'), office.oMath(frac(a, b)))]';
 source+='## Inline comparison\n\n'+inline('office.smallFrac()')+' and '+inline('office.smallFrac(val: false)')+'\n';
 assert.deepEqual(fs.readdirSync(docs).sort(),['Default.docx','Normal.docx','Small.docx']);fs.writeFileSync(file,source);fs.writeFileSync(path.join(data,'legacy-source.md'),source);
 report.checks.push('Real DOCX preview imports default, val-less enabled and explicit-disabled settings through aliased XML and an alternate settings path, without writing MD/assets.');
 stage='real display and inline ink';await page.evaluate(async f=>loadFile(f),file);
 await until(()=>page.locator('#content mjx-container [data-readmd-math-renderer="rust"]').count().then(n=>n===3));await until(()=>page.locator('#content .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===2));
 for(let i=0;i<3;i++){const e=page.locator('#content mjx-container [data-readmd-math-renderer="rust"]').nth(i);await e.scrollIntoViewIfNeeded();const g=await e.evaluate(ink);contained(g);report.geometry.push({phase:'legacy-display',index:i,...g});}
 assert(report.geometry[1].width<report.geometry[0].width-1);assert(Math.abs(report.geometry[2].width-report.geometry[0].width)<.1);
 const inlineGeometry=[];for(let i=0;i<2;i++){const e=page.locator('#content .readmd-dsl-math').nth(i);await e.scrollIntoViewIfNeeded();inlineGeometry.push(await e.evaluate(ink));}
 assert(Math.abs(inlineGeometry[0].width-inlineGeometry[1].width)<.1);report.inlineGeometry=inlineGeometry;assert.equal(fs.readFileSync(file,'utf8'),source);
 await page.locator('#content mjx-container').first().scrollIntoViewIfNeeded();await page.screenshot({path:path.join(data,'legacy-reading-1160.png')});
 report.checks.push('Actual vector ink shrinks display arguments only; absent and explicit-off settings match, while inline enabled/off have identical widths.');
 stage='migration, save and reopen';await page.evaluate(async()=>{await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');});assert.equal(await page.evaluate(()=>saveEdit()),true);
 const migrated=fs.readFileSync(file,'utf8');assert(!migrated.includes('<!--readmd:math:v1:'));assert.equal((migrated.match(/office\.mathPr\(/g)||[]).length,5);
 await page.evaluate(()=>cmView.focus());await page.keyboard.press('Control+z');assert.equal(await page.evaluate(()=>getEditContent()),source);await page.keyboard.press('Control+y');assert.equal(await page.evaluate(()=>getEditContent()),migrated);
 await page.evaluate(async f=>{await exitEdit();await loadFile(f,{force:true,discardConfirmed:true});},file);await until(()=>page.locator('#content .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===5));
 for(let i=0;i<3;i++){const e=page.locator('#content .readmd-dsl-math').nth(i);await e.scrollIntoViewIfNeeded();const g=await e.evaluate(ink);contained(g);assert(Math.abs(g.width-report.geometry[i].width)<.2);report.geometry.push({phase:'reopened-dsl',index:i,...g});}
 assert.deepEqual(fs.readdirSync(docs).sort(),['Default.docx','Normal.docx','Small-fractions.md','Small.docx']);
 report.checks.push('Explicit save migrates to readable DSL as one Ctrl+Z/Y transaction; reopen preserves each real painted width and creates no side files.');
 stage='rendered slot edit';await page.evaluate(async()=>{await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');});await until(()=>page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===5));
 await page.locator('#preview-pane .readmd-dsl-math').nth(1).click();await ready();await page.locator('.formula-inline').getByRole('button',{name:'Numerator',exact:true}).click();await page.locator('.formula-inline-input').fill('α + 1');await page.locator('.formula-inline-input').press('Enter');await ready();await page.locator('.formula-inline-controls button').first().click();
 const changed=await page.evaluate(()=>getEditContent());assert.notEqual(changed,migrated);
 const parts=await page.evaluate(()=>ReadMDFormula.ranges(getEditContent()).map(r=>r.tex)),oldParts=await page.evaluate(s=>ReadMDFormula.ranges(s).map(r=>r.tex),migrated);assert.equal(parts.length,5);for(const i of [0,2,3,4])assert.equal(parts[i],oldParts[i]);assert(parts[1].includes('office.smallFrac('));assert(parts[1].includes('α'));
 await page.evaluate(()=>cmView.focus());await page.keyboard.press('Control+z');assert.equal(await page.evaluate(()=>getEditContent()),migrated);await page.keyboard.press('Control+y');assert.equal(await page.evaluate(()=>getEditContent()),changed);assert.equal(await page.evaluate(()=>saveEdit()),true);assert.equal(fs.readFileSync(file,'utf8'),changed);
 report.checks.push('Clicking the rendered small fraction edits only its numerator, keeps document defaults, supports one real keyboard undo/redo and saves the changed body.');
 stage='actual output engines';const exportSource='# Small fraction output\n\n```readmd-math v1\n'+parts[1]+'\n```\n';fs.writeFileSync(path.join(data,'export-source.md'),exportSource);report.outputs={};
 for(const format of ['docx','epub','pdf']){const out_path=path.join(docs,'Small-fraction-output.'+format),r=await api('/api/export',{format,content:exportSource,baseDir:docs,options:{cover:{enabled:false},toc:{enabled:false},pdf:{engine:'rust'}},out_path});assert.equal(r.status,200,JSON.stringify(r.data));assert.equal(r.data.ok,true,JSON.stringify(r.data));assert(fs.statSync(out_path).size>1000);report.outputs[format]=path.relative(data,out_path);}
 const word=zipParts(fs.readFileSync(path.join(data,report.outputs.docx))),settings=word.get('word/settings.xml'),body=word.get('word/document.xml');assert(settings.includes('smallFrac'));assert(!body.includes('mathPr'));assert(body.includes('m:f>'));fs.writeFileSync(path.join(data,'settings.xml'),settings);fs.writeFileSync(path.join(data,'document.xml'),body);
 const epub=[...zipParts(fs.readFileSync(path.join(data,report.outputs.epub)))].filter(([n])=>n.endsWith('.xhtml')).map(([,v])=>v).join('');assert.equal((epub.match(/scriptlevel="\+1"/g)||[]).length,2);fs.writeFileSync(path.join(data,'epub-math.xhtml'),epub);
 const imported=await api('/api/convert?p='+encodeURIComponent(path.join(data,report.outputs.docx))+'&preview=1');assert.equal(imported.status,200);assert(imported.data.content.includes('<!--readmd:math:v1:'));fs.writeFileSync(path.join(data,'output-reimport.md'),imported.data.content);
 report.checks.push('Actual DOCX keeps one settings smallFrac and editable fraction content, EPUB applies two child script steps, Rust PDF is generated and DOCX reimports through the real reader. Application-level Word and EPUB reader checks remain unverified.');
 stage='dark narrow reading';await page.evaluate(async f=>{await exitEdit();await loadFile(f,{force:true,discardConfirmed:true});await i18n.setLanguage('zh-CN',false);state.theme='dark';applySettings();},file);await page.setViewportSize({width:1024,height:680});await until(()=>page.locator('#content .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===5));await page.locator('#content .readmd-dsl-math').nth(1).scrollIntoViewIfNeeded();contained(await page.locator('#content .readmd-dsl-math').nth(1).evaluate(ink));await page.screenshot({path:path.join(data,'small-fractions-dark-1024.png')});
 assert.deepEqual(report.pageErrors,[]);report.passed=true;
}catch(e){report.failedStage=stage;report.error=String(e.stack||e.message).replaceAll(root,'<workspace>').slice(0,1600);if(page)await page.screenshot({path:path.join(data,'failure.png')}).catch(()=>{});process.exitCode=1;}
finally{if(native){await native.browser.close().catch(()=>{});native.server.child.kill();}report.recordedAt=new Date().toISOString();report.scope='Display smallFrac semantics through actual Word import, vector ink, readable migration, source-bound slot editing and real DOCX/EPUB/PDF outputs. Full global settings, actual Word application editing, independent EPUB reader and full B3 are unverified.';fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,report:path.relative(root,path.join(data,'report.json')),failedStage:report.failedStage}));}
})();
