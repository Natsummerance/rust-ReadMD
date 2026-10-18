'use strict';
// Actual Windows host, packaged CSP, synthetic source and real exporters.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict'),net=require('node:net');
const {chromium}=require('playwright'),{startNative}=require('./native-session.cjs'),zipParts=require('./zip-parts.cjs');
const root=path.resolve(__dirname,'..'),data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/math-attachments-native-')),docs=path.join(data,'documents');fs.mkdirSync(docs);
const hash=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const report={passed:false,platform:'windows-x64',formalCsp:true,synthetic:true,checks:[],pageErrors:[],wordApplication:'unverified',independentEpubReader:'unverified'};
let host,page,stage='startup';
async function port(){const s=net.createServer();await new Promise(r=>s.listen(0,'127.0.0.1',r));const p=s.address().port;await new Promise(r=>s.close(r));return p;}
async function until(fn){const end=Date.now()+60000;while(!await fn()){assert(Date.now()<end,'Timed out: '+stage);await new Promise(r=>setTimeout(r,80));}}
const api=(route,body)=>page.evaluate(async({route,body})=>{const r=await apiFetch(route,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});return{status:r.status,data:await r.json()};},{route,body});
async function serialize(source,dialect,target){const r=await api('/api/math/serialize',{source,dialect,target,display:true});assert.equal(r.status,200,JSON.stringify(r.data));assert.equal(r.data.ok,true);return r.data.output;}
async function stop(){if(!host)return;await host.browser.close().catch(()=>{});const c=host.server.child;if(c.exitCode===null){const done=new Promise(r=>c.once('exit',r));c.kill();await Promise.race([done,new Promise(r=>setTimeout(r,2000))]);}}
(async()=>{try{
 assert(process.env.READMD_BIN&&process.env.READMD_FONTS_PACK,'Explicit current binary and offline font pack required');process.env.READMD_WORKSPACE=docs;
 report.binarySha256=hash(process.env.READMD_BIN);report.bootSha256=hash(path.join(root,'assets/readmd.boot.js'));report.scriptSha256=hash(__filename);
 host=await startNative(root,data,chromium,await port());page=host.page;page.on('pageerror',e=>report.pageErrors.push(e.message));
 await page.evaluate(()=>Promise.all([__readmdSessionReady,__readmdPreferencesReady]));await page.setViewportSize({width:1160,height:820});
 assert.equal((await api('/api/capabilities/packs',{action:'install',id:'fonts',path:process.env.READMD_FONTS_PACK})).data.ok,true);
 stage='MathML semantic adapters';const samples=[
  ['accent','<mover><mi>x</mi><mo>^</mo></mover>','acc'],
  ['vector','<mover><mi>x</mi><mo>⃗</mo></mover>','acc'],
  ['overbar','<mover accent="true"><mi>x</mi><mo>¯</mo></mover>','bar'],
  ['underbar','<munder accentunder="true"><mi>x</mi><mo>_</mo></munder>','bar'],
  ['underbrace','<munder><mrow><mi>a</mi><mo>+</mo><mi>b</mi></mrow><mo>⏟</mo></munder>','groupChr'],
  ['overbrace','<mover accent="true"><mi>x</mi><mo>⏞</mo></mover>','groupChr'],
  ['literal limit','<mover accent="false"><mi>x</mi><mo>^</mo></mover>','limUpp'],
  ['both','<munderover accentunder="true" accent="false"><mi>x</mi><mo>_</mo><mo>^</mo></munderover>','limUpp']
 ];report.adapters=[];
 for(const[name,body,kind]of samples){const mml='<math xmlns="http://www.w3.org/1998/Math/MathML">'+body+'</math>',dsl=await serialize(mml,'mathml','dsl'),xml=await serialize(dsl,'dsl','omml'),reopened=await serialize(await serialize(dsl,'dsl','mathml'),'mathml','omml');assert(xml.includes('<m:'+kind));assert(reopened.includes('<m:'+kind));report.adapters.push({name,dsl,kind});}
 report.checks.push('Eight real API imports distinguish accents, combining vector marks, bars, group characters, explicit-off limits and combined attachments through readable DSL and MathML reimport.');
 const duplicate='group_char(a + b + c, character: "⏟", position: below, vertJc: top)';
 const expressions=[duplicate,duplicate,...['top','bot'].flatMap(pos=>['top','bot'].map(jc=>'group_char(a + b, character: "'+(pos==='top'?'⏞':'⏟')+'", pos: "'+pos+'", vertJc: "'+jc+'")')), 'group_char('+('abcdefghij'.repeat(4))+', character: "⏟", position: below)',...report.adapters.filter(e=>['accent','overbar','underbar'].includes(e.name)).map(e=>e.dsl)];
 const source='# 公式装饰校订\n\n对比上下分组、基线、长表达式与重音；相同公式各自独立编辑。\n\n'+expressions.map((e,i)=>'## 样例 '+(i+1)+'\n\n'+String.fromCharCode(96).repeat(3)+'readmd-math v1\n'+e+'\n'+String.fromCharCode(96).repeat(3)+'\n').join('\n');
 const file=path.join(docs,'Formula-attachments.md');fs.writeFileSync(file,source);
 stage='native rendered formulas';await page.evaluate(async f=>{await i18n.setLanguage('en',false);await loadFile(f);await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');},file);
 await until(()=>page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===expressions.length));
 assert.equal(await page.locator('[data-mml-node="merror"]').count(),0);
 assert.equal(await page.locator('#preview-pane .readmd-dsl-math svg [data-math-ink]').count(),expressions.length);
 assert.equal(fs.readFileSync(file,'utf8'),source);
 report.checks.push('Ten actual formulas render vector ink in the native editor, including four baseline combinations and a 40-character horizontally assembled brace; entering edit does not write the source.');
 stage='precise repeated slot edit';await page.locator('#preview-pane .readmd-dsl-math').nth(1).click();
 await until(()=>page.locator('.formula-inline-controls button').first().isEnabled());
 await page.locator('.formula-inline').getByRole('button',{name:'Expression',exact:true}).first().click();
 await page.locator('.formula-inline-input').fill('α + 7');await page.locator('.formula-inline-input').press('Enter');
 await until(()=>page.locator('.formula-inline-controls button').first().isEnabled());await page.locator('.formula-inline-controls button').first().click();
 const changed=await page.evaluate(()=>getEditContent()),ranges=await page.evaluate(()=>ReadMDFormula.ranges(getEditContent()));
 assert.equal(ranges[0].tex,duplicate);assert(ranges[1].tex.includes('α'));const editedXml=await serialize(ranges[1].tex,'dsl','omml');
 assert(editedXml.includes('m:vertJc m:val="top"')&&editedXml.includes('m:chr m:val="⏟"'));
 await page.evaluate(()=>cmView.focus());await page.keyboard.press('Control+z');assert.equal(await page.evaluate(()=>getEditContent()),source);
 await page.keyboard.press('Control+y');assert.equal(await page.evaluate(()=>getEditContent()),changed);
 assert.equal(await page.evaluate(()=>saveEdit()),true);assert.equal(fs.readFileSync(file,'utf8'),changed);
 report.checks.push('Editing the rendered base changes only the second duplicate, retains grouping properties, supports one real Ctrl+Z/Y transaction and explicitly saves the changed source.');
 stage='actual exports';report.outputs={};
 for(const format of ['docx','epub','pdf']){const out=path.join(docs,'Formula-attachments.'+format),r=await api('/api/export',{format,content:changed,baseDir:docs,out_path:out,options:{cover:{enabled:false},toc:{enabled:false},pdf:{engine:'rust'}}});assert.equal(r.status,200,JSON.stringify(r.data));assert.equal(r.data.ok,true,JSON.stringify(r.data));assert(fs.statSync(out).size>1000);report.outputs[format]={path:path.relative(data,out),sha256:hash(out)};}
 const docx=zipParts(fs.readFileSync(path.join(data,report.outputs.docx.path))),body=docx.get('word/document.xml');assert(body.includes('m:groupChr')&&body.includes('m:vertJc')&&body.includes('m:bar')&&body.includes('m:acc'));fs.writeFileSync(path.join(data,'document.xml'),body);
 const epub=[...zipParts(fs.readFileSync(path.join(data,report.outputs.epub.path)))].filter(([n])=>n.endsWith('.xhtml')).map(([,s])=>s).join('\n');assert(epub.includes('accentunder="true"')&&epub.includes('accent="true"'));assert(epub.includes('accentunder="false"')&&epub.includes('accent="false"'));fs.writeFileSync(path.join(data,'epub-math.xhtml'),epub);
 const imported=await page.evaluate(async f=>{const r=await apiFetch('/api/convert?p='+encodeURIComponent(f)+'&preview=1');return{status:r.status,data:await r.json()};},path.join(data,report.outputs.docx.path));assert.equal(imported.status,200);assert(imported.data.content.includes('<!--readmd:math:v1:'));fs.writeFileSync(path.join(data,'reimport.md'),imported.data.content);
 report.checks.push('Real DOCX retains editable grouping/accents/bars and baseline properties; EPUB uses valid upper/lower accent attributes; Rust vector PDF is generated and the DOCX reimports through the real converter.');
 stage='dark narrow visual';await page.evaluate(async f=>{await exitEdit();await loadFile(f,{force:true,discardConfirmed:true});await i18n.setLanguage('zh-CN',false);state.theme='dark';applySettings();},file);
 await page.setViewportSize({width:1024,height:680});await until(()=>page.locator('#content .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===expressions.length));
 await page.locator('#content .readmd-dsl-math').nth(6).scrollIntoViewIfNeeded();
 report.longBrace=await page.locator('#content .readmd-dsl-math').nth(6).evaluate(e=>{const s=e.querySelector('svg'),ink=s.querySelector('[data-math-ink]'),a=s.getBoundingClientRect(),b=ink.getBoundingClientRect();return{svg:{left:a.left,right:a.right,top:a.top,bottom:a.bottom},ink:{left:b.left,right:b.right,top:b.top,bottom:b.bottom},paths:ink.querySelectorAll('path').length};});
 const {svg,ink}=report.longBrace;assert(ink.left>=svg.left-1&&ink.right<=svg.right+1&&ink.top>=svg.top-1&&ink.bottom<=svg.bottom+1);assert(report.longBrace.paths>45);
 await page.screenshot({path:path.join(data,'attachments-dark-1024.png')});assert.deepEqual(report.pageErrors,[]);
 report.checks.push('At 1024×680 in Chinese dark mode the long assembled brace remains inside its SVG without clipping or page errors.');
 stage='actual restart';await page.evaluate(()=>ReadMDSession.flush());await stop();host=await startNative(root,data,chromium,await port());page=host.page;
 await page.evaluate(()=>window.__readmdSessionReady);await page.evaluate(file=>loadFile(file),file);assert.equal(await page.evaluate(()=>state.original),changed);assert.equal(fs.readFileSync(file,'utf8'),changed);
 assert(!fs.existsSync(path.join(docs,'readmd-assets')));
 report.checks.push('A real host restart reopens the saved formula source without materializing resource side directories.');report.passed=true;
 }catch(e){report.failedStage=stage;report.error=String(e.stack||e.message).replaceAll(root,'<workspace>').slice(0,1900);if(page)await page.screenshot({path:path.join(data,'failure.png')}).catch(()=>{});process.exitCode=1;}
 finally{await stop();report.recordedAt=new Date().toISOString();report.scope='Attachment/default/baseline and horizontal construction increment only; complete OMML/MathML, actual Word application and independent EPUB reader remain unverified.';fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,report:path.relative(root,path.join(data,'report.json')),failedStage:report.failedStage}));}
})();
