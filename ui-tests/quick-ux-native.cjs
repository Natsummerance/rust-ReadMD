'use strict';
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),http=require('node:http');
const {chromium}=require('playwright'),{startNative}=require('./native-session.cjs');
const root=path.resolve(__dirname,'..'),directory=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/quick-ux-native-'));
const profile=path.join(directory,'profile'),documents=path.join(directory,'documents');fs.mkdirSync(documents);
const file=path.join(documents,'Evidence-workshop.md');
const source='# 证据工作坊\n\n> [!NOTE]\n> 从观察出发，保留来源，再比较不同解释。\n\n> [!CAUTION]\n> 结论必须与样本范围一致，不要把相关误写成因果。\n\n第一次：Repeated evidence.\n\n第二次：Repeated evidence.\n\n```readmd-math v1\nfrac(α, β)\n```\n\n## 作者确认\n\n- [x] 保留原始观察\n- [ ] 检查论证边界\n';
fs.writeFileSync(file,source);
const hash=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const report={platform:'windows-x64',formalCsp:true,syntheticOnly:true,aiProvider:'local deterministic protocol fixture; not upstream model certification',checks:[],passed:false};
let native,relay,phase='startup';
async function until(check){const end=Date.now()+45000;while(!await check()){if(Date.now()>end)throw Error('Timed out: '+phase);await new Promise(r=>setTimeout(r,75));}}
(async()=>{try{
  report.binarySha256=hash(process.env.READMD_BIN);report.bootSha256=hash(path.join(root,'assets/readmd.boot.js'));
  process.env.READMD_WORKSPACE=documents;native=await startNative(root,profile,chromium,28939);const {page}=native;
  await page.evaluate(()=>Promise.all([window.__readmdSessionReady,window.__readmdPreferencesReady]));
  const errors=[];page.on('pageerror',e=>errors.push(e.name));
  const api=(route,body)=>page.evaluate(async({route,body})=>{const response=await apiFetch(route,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});return {status:response.status,body:await response.json()};},{route,body});
  await page.evaluate(()=>i18n.setLanguage('zh-CN',false));
  phase='formula leave, undo and disk save';
  await page.evaluate(async file=>{await loadFile(file);await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');},file);
  await until(()=>page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===1));
  await page.locator('#preview-pane .readmd-dsl-math mjx-container').click();
  await until(()=>page.locator('.formula-inline-controls button').first().isEnabled());
  await page.locator('.formula-inline-input').fill('θ');await page.locator('#preview-pane h2').click();
  await until(()=>page.locator('.formula-inline').count().then(n=>n===0));
  const changed=source.replace('frac(α, β)','frac(θ, β)');assert.equal(await page.evaluate(()=>getEditContent()),changed);assert.equal(fs.readFileSync(file,'utf8'),source);
  await page.evaluate(()=>cmUndo());assert.equal(await page.evaluate(()=>getEditContent()),source);
  await page.evaluate(()=>cmRedo());assert.equal(await page.evaluate(()=>saveEdit()),true);assert.equal(fs.readFileSync(file,'utf8'),changed);
  await page.evaluate(async file=>{await exitEdit();await loadFile(file,{force:true,discardConfirmed:true});},file);
  assert.equal(await page.evaluate(()=>state.original),changed);assert.deepEqual(fs.readdirSync(documents),['Evidence-workshop.md']);
  report.checks.push('native DSL leave applies the final input, one undo/redo, atomic save/reopen and no adjacent resource files');

  phase='automatic selection with actual kernel provider request';const requests=[];
  relay=http.createServer((req,res)=>{let raw='';req.on('data',chunk=>raw+=chunk);req.on('end',()=>{requests.push(JSON.parse(raw));res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify({choices:[{message:{content:'Verified evidence.'}}]}));});});
  await new Promise(resolve=>relay.listen(0,'127.0.0.1',resolve));
  const configured=await api('/api/ai/config',{providers:[{id:'custom:quick-native',name:'本机交互验收',category:'local',base_url:'http://127.0.0.1:'+relay.address().port+'/v1',mode:'chat',format:'openai',models:['interaction-fixture']}],current:{provider_id:'custom:quick-native',model:'interaction-fixture'}});assert.equal(configured.status,200);
  const selection=await page.evaluate(async()=>{
    await toggleEdit();setPvLayout('none');await loadAiConfig();$('ai-stream').checked=false;
    const source=cmView.state.doc.toString(),from=source.lastIndexOf('Repeated evidence.');cmView.dispatch({selection:{anchor:from,head:from+18}});cmView.focus();toggleAiPanel();return {from,to:from+18};
  });
  await page.locator('#ai-prompt').fill('只润色选中的句子。');await page.locator('#ai-run').click();
  await until(()=>page.locator('#ai-output .ai-msg.ai .ai-msg-body').textContent().then(s=>s.includes('Verified evidence.')));
  await until(()=>page.evaluate(()=>!state.ai.busy&&!state.ai.preparing));
  assert.equal(await page.locator('#ai-selection').count(),0);assert.equal(requests.length,1);
  assert.deepEqual(await page.evaluate(()=>({from:state.ai.lastSelection.from,to:state.ai.lastSelection.to})),selection);
  await page.evaluate(()=>applyAi('Verified evidence.',state.ai.lastSelection));
  const expected=changed.slice(0,selection.from)+'Verified evidence.'+changed.slice(selection.to);assert.equal(await page.evaluate(()=>getEditContent()),expected);
  await page.evaluate(()=>cmUndo());assert.equal(await page.evaluate(()=>getEditContent()),changed);assert.equal(fs.readFileSync(file,'utf8'),changed);
  report.checks.push('native composer captures the second repeated selection before focus; real Rust protocol request and precise replacement undo preserve the source');
  await page.evaluate(()=>{state.theme='dark';applySettings();});await page.screenshot({path:path.join(directory,'ai-context-dark.png')});

  phase='native PDF callout export and decode';
  await page.evaluate(()=>{state.theme='light';applySettings();$('ai-panel').classList.add('hidden');});
  const installed=await api('/api/capabilities/packs',{action:'install',path:process.env.READMD_PDF_PACK});assert.equal(installed.status,200);assert.equal(installed.body.installed,true);
  const options=await page.evaluate(async()=>{await openExportModal();return collectExportOptions();});options.pdf={engine:'rust'};
  assert.equal(options.calloutTitles.note,await page.evaluate(()=>i18n.t('reader.calloutNote')));
  assert.notEqual(options.calloutTitles.note,'reader.calloutNote');
  const output=path.join(documents,'Evidence-workshop.pdf'),exported=await api('/api/export',{format:'pdf',content:changed,baseDir:documents,options,out_path:output});assert.equal(exported.status,200);assert.equal(exported.body.ok,true);
  await page.evaluate(()=>{$('export-modal').classList.add('hidden');$('export-preview-modal').classList.add('hidden');});
  assert.equal(await page.evaluate(file=>ReadMDPdf.open(file),output),true);
  await until(()=>page.locator('#pdf-image').evaluate(i=>i.complete&&i.naturalHeight>0));
  await page.screenshot({path:path.join(directory,'pdf-callouts.png')});await page.locator('#pdf-close').click();
  report.checks.push('actual PDF export uses existing Chinese callout labels, preserves typed alerts and vector math, and reopens through locked PDFium in native WebView2');
  assert.equal(fs.readFileSync(file,'utf8'),changed);assert.deepEqual(errors,[]);report.passed=true;
}catch(error){report.failedStage=phase;report.error=error.name;report.message=String(error.message).replaceAll(root,'<workspace>').replaceAll(directory,'<owned-test>').slice(0,600);if(native)await native.page.screenshot({path:path.join(directory,'failure.png')}).catch(()=>{});process.exitCode=1;}
finally{
  if(relay){relay.closeAllConnections();await new Promise(resolve=>relay.close(resolve));}
  if(native){await native.browser.close().catch(()=>{});native.server.child.kill();}
  report.recordedAt=new Date().toISOString();fs.writeFileSync(path.join(directory,'report.json'),JSON.stringify(report,null,2));
  console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,failedStage:report.failedStage,report:path.relative(root,path.join(directory,'report.json'))}));
}})();
