'use strict';
// Current, formal-CSP Windows host: real rendering, exact edits, persistence
// and publication. No mocked routes, external fonts or installed Office needed.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),net=require('node:net');
const {chromium}=require('playwright'),{startNative}=require('./native-session.cjs'),zipParts=require('./zip-parts.cjs');
const root=path.resolve(__dirname,'..'),data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/math-delimiters-native-'));
const docs=path.join(data,'documents');fs.mkdirSync(docs);const file=path.join(docs,'Mathematical-structure.md');
const hash=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const report={platform:'windows-x64',nativeCsp:true,synthetic:true,wordRoundTrip:'unverified',checks:[],pageErrors:[],passed:false};let native,page,stage='startup';
const source=[
 '# 数学结构工作坊 · 从符号到可复核的论证','',
 '> [!NOTE]','> 括号、算子与公式正文分别保存。改变一处公式，不应修改另一处相同的公式。','',
 '## 一、独立修改与撤销','',
 '原始定义 :math[delimiter(x, open: "(", close: ")")]；修订定义 :math[delimiter(x, open: "(", close: ")")]。','',
 '| 操作 | 数学语义 | 检查重点 |','|---|---|---|',
 '| 清空右括号 | 隐藏右侧符号 | 左括号与正文保留 |',
 '| 恢复默认 | 使用 Office 默认右括号 | 区分隐藏与继承 |',
 '| 禁止伸展 | 符号保持原始高度 | 矩阵内容完整 |','',
 '## 二、固定高度与内容贴合','',
 '```readmd-math v1','// 固定高度：不随矩阵伸展','delimiter(matrix([a, b], [c, d], [e, f]), grow: false)','```','',
 '```readmd-math v1','// 数学轴对称与紧贴内容分别建模','delimiter(superscript(x, matrix([1], [2], [3])), grow: true, shp: match)','```','',
 '## 三、大算子与有界伸展','',
 '```readmd-math v1','nary(matrix([a], [b], [c], [d], [e], [f]), character: "∫", from: 0, to: 1, grow: true)','```','',
 '## 四、空符号不是默认符号','',
 '```readmd-math v1','delimiter(a, b, open: "", close: "", separator: "")','```','',
 '完成结构校订后再保存；输出 Word 可编辑公式、EPUB MathML 与矢量 PDF。',''
].join('\n');
async function until(check){const deadline=Date.now()+45000;while(!await check()){if(Date.now()>deadline)throw Error('Timed out: '+stage);await new Promise(r=>setTimeout(r,80));}}
async function freePort(){const s=net.createServer();await new Promise(r=>s.listen(0,'127.0.0.1',r));const p=s.address().port;await new Promise(r=>s.close(r));return p;}
async function api(route,body){return page.evaluate(async({route,body})=>{const r=await apiFetch(route,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});return{status:r.status,data:await r.json()};},{route,body});}
async function ranges(){return page.evaluate(()=>ReadMDFormula.ranges(getEditContent()));}
async function propertyPanel(){await page.locator('#preview-pane .readmd-dsl-math [data-readmd-math-renderer]').nth(1).click();await until(()=>page.locator('.formula-inline-controls button').first().isEnabled());await page.locator('.formula-properties > summary').click();await page.locator('[data-math-container="dPr"] > summary').click();}
async function outputMath(source,target){const r=await api('/api/math/serialize',{source,dialect:'dsl',display:true,target});assert.equal(r.status,200,JSON.stringify(r.data));return r.data.output;}
(async()=>{try{
 assert(process.env.READMD_BIN&&process.env.READMD_FONTS_PACK,'Explicit current binary and pinned font pack required');process.env.READMD_WORKSPACE=docs;process.env.READMD_NATIVE_LOG_FILE=path.join(data,'native.log');fs.writeFileSync(file,source);
 report.binarySha256=hash(process.env.READMD_BIN);report.assets=Object.fromEntries(['assets/readmd.boot.js','assets/css/panels.css','assets/js/editor/formula-properties.js'].map(f=>[f,hash(path.join(root,f))]));report.scriptSha256=hash(__filename);
 const previousPath=process.env.PATH;process.env.PATH=path.join(process.env.WINDIR,'System32');
 try{native=await startNative(root,data,chromium,await freePort());}finally{process.env.PATH=previousPath;}
 report.childPath='System32-only';page=native.page;page.on('pageerror',e=>report.pageErrors.push(e.message));
 await page.evaluate(()=>Promise.all([__readmdSessionReady,__readmdPreferencesReady]));await page.setViewportSize({width:1160,height:820});
 stage='real reading and formula editor';await page.evaluate(async f=>{await i18n.setLanguage('en',false);await loadFile(f);await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');},file);
 await until(()=>page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===6));assert.equal(await page.locator('[data-mml-node="merror"]').count(),0);
 stage='hide the exact second fence';await propertyPanel();await page.locator('[data-math-property="dPr.endChr"] input').fill('');await page.locator('[data-math-property="dPr.endChr"] input').press('Tab');
 await until(()=>page.locator('[data-math-default="dPr.endChr"]').isVisible());await until(()=>page.locator('.formula-inline-controls button').first().isEnabled());
 assert.equal(await page.evaluate(()=>getEditContent()),source);assert.equal(fs.readFileSync(file,'utf8'),source);
 await page.locator('.formula-inline-controls button').first().click();const hidden=await page.evaluate(()=>getEditContent()),hiddenRanges=await ranges();
 assert.equal(hiddenRanges[0].tex,'delimiter(x, open: "(", close: ")")');assert.notEqual(hiddenRanges[1].tex,hiddenRanges[0].tex);
 assert.match(await outputMath(hiddenRanges[1].tex,'omml'),/<m:endChr m:val=""\/>/);
 assert.equal(hidden.slice(0,hiddenRanges[1].from),source.slice(0,hiddenRanges[1].from));
 const oldRanges=await page.evaluate(s=>ReadMDFormula.ranges(s),source);assert.equal(hidden.slice(hiddenRanges[1].to),source.slice(oldRanges[1].to));
 await page.keyboard.press('Control+z');assert.equal(await page.evaluate(()=>getEditContent()),source);await page.keyboard.press('Control+y');assert.equal(await page.evaluate(()=>getEditContent()),hidden);
 report.checks.push('Actual character input hides only the second repeated formula fence; body transaction undo/redo restores exact author text.');
 stage='restore an inherited Office fence';await until(()=>page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===6));await propertyPanel();
 const reset=page.locator('[data-math-default="dPr.endChr"]'),box=await reset.boundingBox();assert(box.width>=44&&box.height>=44);assert.match(await reset.getAttribute('aria-label'),/./);
 await reset.click();await until(()=>page.locator('[data-math-default="dPr.endChr"]').count().then(n=>n===0));await until(()=>page.locator('.formula-inline-controls button').first().isEnabled());await page.locator('.formula-inline-controls button').first().click();
 const restored=await page.evaluate(()=>getEditContent()),restoredRanges=await ranges();const restoredXml=await outputMath(restoredRanges[1].tex,'omml');assert(!restoredXml.includes('<m:endChr'));
 const restoredMml=await outputMath(restoredRanges[1].tex,'mathml');assert.match(restoredMml,/>\)<\/mo>/);assert(!restored.includes('readmd:math:v1:'));
 report.checks.push('The 44px localized reset restores the omitted Office default instead of a hidden character; DSL stays readable.');
 stage='save reopen and render';assert.equal(await page.evaluate(()=>saveEdit()),true);assert.equal(fs.readFileSync(file,'utf8'),restored);assert.deepEqual(fs.readdirSync(docs),['Mathematical-structure.md']);
 await page.evaluate(async f=>{await exitEdit();await loadFile(f,{force:true,discardConfirmed:true});},file);await until(()=>page.locator('#content .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===6));assert.equal(await page.evaluate(()=>hasUnsavedEditorChanges()),false);
 await page.locator('#content .readmd-dsl-math').nth(2).scrollIntoViewIfNeeded();await page.screenshot({path:path.join(data,'reading-1160.png')});
 report.checks.push('Real atomic save/reopen retains applied DSL with no side files or dirty state.');
 stage='actual render/export semantics';const all=restoredRanges.map(r=>r.tex),mml=[];for(const s of all)mml.push(await outputMath(s,'mathml'));
 assert(mml[2].includes('stretchy="false"'));assert(mml[3].includes('symmetric="false"'));assert(mml[4].includes('stretchy="true"'));assert(!mml[5].includes('<mo'));
 const fixed=await outputMath('delimiter(matrix([a],[b],[c]), grow: false)','mathml'),grown=await outputMath('delimiter(matrix([a],[b],[c]), grow: true)','mathml');assert.notEqual(fixed,grown);
 fs.writeFileSync(path.join(data,'mathml.json'),JSON.stringify(mml,null,2));const outputs={};
 stage='missing font protection and offline installation';const previous=path.join(docs,'Previous.pdf');fs.writeFileSync(previous,'Previous completed output');
 const missing=await api('/api/export',{format:'pdf',content:restored,baseDir:docs,options:{pdf:{engine:'rust'}},out_path:previous});assert.equal(missing.data.error_code,'publication_fonts_unavailable');assert.equal(fs.readFileSync(previous,'utf8'),'Previous completed output');
 const uploads=path.join(data,'uploads');fs.mkdirSync(uploads);const fonts=path.join(uploads,'Fonts.readmd-pack');fs.linkSync(path.resolve(process.env.READMD_FONTS_PACK),fonts);report.fontPackSha256=hash(fonts);
 const installed=await api('/api/capabilities/packs',{action:'install',id:'fonts',path:fonts});assert.equal(installed.status,200,JSON.stringify(installed.data));assert.equal(installed.data.installed,true);
 report.checks.push('Missing portable Chinese fonts preserve an existing PDF; the actual pinned offline pack installs into the isolated profile.');stage='actual render/export semantics';
 for(const format of ['docx','epub','pdf']){const out_path=path.join(docs,'Mathematical-structure.'+format);const r=await api('/api/export',{format,content:restored,baseDir:docs,options:{cover:{enabled:false},toc:{enabled:false},pdf:{engine:'rust'}},out_path});assert.equal(r.status,200,JSON.stringify(r.data));assert.equal(r.data.ok,true,JSON.stringify(r.data));assert(fs.statSync(out_path).size>1000);outputs[format]=path.relative(data,out_path);}
 report.outputs=outputs;const word=zipParts(fs.readFileSync(path.join(data,outputs.docx))).get('word/document.xml');assert.match(word,/<m:grow m:val="false"\/>/);assert.match(word,/<m:shp m:val="match"\/>/);assert.match(word,/<m:sepChr m:val=""\/>/);assert.match(word,/<m:grow m:val="true"\/>/);
 const epub=[...zipParts(fs.readFileSync(path.join(data,outputs.epub)))].filter(([n])=>n.endsWith('.xhtml')).map(([,v])=>v).join('');assert(epub.includes('stretchy="false"'));assert(epub.includes('symmetric="false"'));assert(epub.includes('<math'));fs.writeFileSync(path.join(data,'document.xml'),word);
 report.checks.push('Actual DOCX preserves editable grow/shape/empty properties; EPUB uses consistent MathML; built-in Rust creates a real PDF. Word re-edit remains unverified.');
 stage='responsive capability panel';await page.setViewportSize({width:1024,height:680});await page.evaluate(async()=>{await i18n.setLanguage('zh-CN',false);state.theme='dark';applySettings();openPluginModal();await refreshOfflineCapabilityPack();});await page.locator('#plugin-modal').waitFor({state:'visible'});
 const geometry=await page.locator('#plugin-modal .plugin-capability-item').evaluateAll(items=>items.filter(e=>e.getClientRects().length).map(e=>{const b=e.getBoundingClientRect();return{x:b.x,y:b.y,width:b.width,height:b.height};}));assert(geometry.length>=2);
 for(let i=1;i<geometry.length;i++)assert(geometry[i].y>=geometry[i-1].y+geometry[i-1].height-1,'Capability items should form distinct rows');
 for(const b of geometry)assert(b.x>=0&&b.x+b.width<=1025);report.capabilityRows=geometry;await page.screenshot({path:path.join(data,'capabilities-dark-1024.png')});
 report.checks.push('The actual 1024×680 dark capability panel keeps every engine on its own complete row.');
 stage='owned resource cleanup';const removed=await api('/api/capabilities/packs',{action:'uninstall',id:'fonts'});assert.equal(removed.status,200,JSON.stringify(removed.data));assert.equal(removed.data.installed,false);
 assert.deepEqual(report.pageErrors,[]);assert.equal(fs.readFileSync(file,'utf8'),restored);report.passed=true;
}catch(error){report.failedStage=stage;report.error=String(error.message).replaceAll(data,'<owned-test>').replaceAll(root,'<workspace>').slice(0,1800);if(page)await page.screenshot({path:path.join(data,'failure.png')}).catch(()=>{});process.exitCode=1;}
finally{if(native){await native.browser.close().catch(()=>{});native.server.child.kill();}report.recordedAt=new Date().toISOString();report.scope='Delimiter/n-ary semantic increment and actual Windows CSP edits/save/export. No full OMML, native Word/EPUB-reader, all-language or complete B3 certification.';fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,report:path.relative(root,path.join(data,'report.json')),failedStage:report.failedStage}));}
})();
