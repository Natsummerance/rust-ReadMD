// Opt-in real provider + actual WebView2, isolated synthetic documents only.
const fs=require('fs'),path=require('path'),assert=require('assert/strict');
const {chromium}=require('playwright');
const {startNative}=require('./native-session.cjs');
const root=path.resolve(__dirname,'..'), output=path.resolve(process.env.READMD_AI_AUDIT_ROOT||'.cache/v005-ai/native');
const data=path.join(output,'data');fs.mkdirSync(data,{recursive:true});
const config=JSON.parse(fs.readFileSync(process.env.READMD_AI_SOURCE_CONFIG,'utf8'));
fs.writeFileSync(path.join(data,'ai.json'),JSON.stringify(config));
const report={checks:[],errors:[],syntheticOnly:true,rawCredentialsLogged:false};
const record=(name,details)=>{report.checks.push({name,...details});console.log(JSON.stringify(report.checks.at(-1)));};
(async()=>{
 let session;
 try{
  session=await startNative(root,data,chromium,28845);const {page}=session;
  page.on('pageerror',e=>report.errors.push(e.message.slice(0,160)));
  const api=(route,body)=>page.evaluate(async({route,body})=>{const r=await apiFetch(route,{...(body===undefined?{}:{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)}),signal:AbortSignal.timeout(60000)});return {status:r.status,body:await r.json()};},{route,body});
  const models=await api('/api/ai/models',{});assert.equal(models.status,200);assert.equal(models.body.source,'provider');
  let model;
  const candidates=models.body.models.filter(m=>/gemini.*flash/i.test(m)).sort((a,b)=>b.localeCompare(a,undefined,{numeric:true}));
  for(const candidate of candidates){const probe=await api('/api/ai/chat',{model:candidate,stream:false,messages:[{role:'user',content:'Reply exactly READMD_REAL_AI_OK.'}],session:'synthetic-model-probe'});if(probe.status===200&&probe.body.content?.trim()==='READMD_REAL_AI_OK'){model=candidate;break;}}
  assert.ok(model,'Actual relay must pass a success request; model-list membership is insufficient');
  config.current.model=model;fs.writeFileSync(path.join(data,'ai.json'),JSON.stringify(config));
  record('real-model-discovery',{model,modelCount:models.body.models.length});
  const samples=[
   ['chat','Reply only READMD_REAL_AI_OK.'],
   ['summary','用一句中文总结：研究从观察开始，通过比较解释和反例检验得出结论。'],
   ['outline','为“证据与推理”写三个 Markdown 小标题。只返回标题。'],
   ['polish','润色这句话，仅返回结果：我们先看证据然后去比较那些解释。'],
   ['translate','Translate to English, return only translation: 哲学从提问开始。'],
   ['explain','用一句话解释“可证伪性”。'],
   ['questions','围绕“观察与解释的区别”提出两个简短研究问题。'],
  ];
  for(const[name,prompt]of (process.env.READMD_AI_GHOST_ONLY?[]:samples)){const start=Date.now();const r=await api('/api/ai/chat',{model,stream:false,messages:[{role:'user',content:prompt}],session:'synthetic-v005-'+name});assert.equal(r.status,200,name+': '+r.body.error_code);assert.ok(r.body.content?.trim(),name);assert.ok(!/model.*retired|模型.*下线|not found/i.test(r.body.content),name);if(name==='chat')assert.ok(r.body.content.includes('READMD_REAL_AI_OK'));record('real-'+name,{elapsedMs:Date.now()-start,characters:r.body.content.length});}
  const prefix='# 证据与推理\n\n研究从观察开始。为了避免主观偏见，我们应该',suffix='\n\n## 结论\n观点需要接受证据检验。';
  const start=Date.now(),completion=await api('/api/ai/complete',{model,prefix,suffix});
  assert.equal(completion.status,200);assert.ok(completion.body.completion.trim());
  record('real-context-completion',{elapsedMs:Date.now()-start,characters:completion.body.completion.length,contextHash:completion.body.context_revision});
  await page.evaluate(()=>py.show_window());await page.bringToFront();
  report.nativeRequests=[];page.on('response',r=>{if(r.url().endsWith('/api/ai/complete'))report.nativeRequests.push(r.status());});
  await page.evaluate(async({prefix,suffix})=>{await i18n.setLanguage('zh-CN',false);await renderVirtual('clipboard','证据与推理.md','',prefix+suffix,[]);await toggleEdit();setPvLayout('none');setEditorPref('aiCompletion',true);cmView.dispatch({selection:{anchor:prefix.length}});cmView.focus();}, {prefix,suffix});
  await page.screenshot({path:path.join(output,'before-real.png')});
  record('native-input-state',await page.evaluate(()=>({slash:typeof slash==='undefined'?null:slash.open,completion:ReadMDCodeMirror.completionStatus(cmView.state),selection:cmView.state.selection.main.toJSON(),dialogs:[...document.querySelectorAll('[role=dialog],.modal')].filter(e=>e.getClientRects().length&&getComputedStyle(e).visibility!=='hidden').map(e=>e.id)})));
  await page.keyboard.press('Control+Alt+Space');
  record('native-request-trigger',{...(await page.evaluate(()=>({hidden:document.hidden,focus:cmView.hasFocus,pref:editorPrefs.aiCompletion,controller:typeof ReadMDInlineCompletion,status:document.getElementById('status-ai-completion').textContent})))});
  await page.waitForSelector('.readmd-ai-ghost',{timeout:40000}).catch(async error=>{
    report.ghostDiagnostic=await page.evaluate(()=>({focus:cmView.hasFocus,documentFocus:document.hasFocus(),hidden:document.hidden,composing:cmView.composing,status:document.getElementById('status-ai-completion').textContent,editing:state.editing,pref:editorPrefs.aiCompletion,viewLength:cmView.state.doc.length}));throw error;
  });
  const ghost=await page.locator('.readmd-ai-ghost').textContent();assert.ok(ghost.trim());
  assert.equal(await page.evaluate(()=>getEditContent()),prefix+suffix);
  await page.screenshot({path:path.join(output,'ghost-real.png')});
  await page.keyboard.press('Tab');assert.equal(await page.evaluate(()=>getEditContent()),prefix+ghost+suffix);
  await page.keyboard.press('Control+z');assert.equal(await page.evaluate(()=>getEditContent()),prefix+suffix);
  record('native-ghost-tab-undo',{characters:ghost.length,originalDiskUntouched:true});
  await page.evaluate(()=>setEditorPref('aiCompletion',false));
  // The automatic-completion endpoint must not add entries to chat history.
  const history=await api('/api/ai/history');
  assert.ok(!JSON.stringify(history.body).includes('证据与推理\n\n研究从观察开始。为了避免'));
  record('completion-does-not-store-context',{passed:true});
  assert.deepEqual(report.errors,[]);
  report.passed=true;
 }catch(error){report.passed=false;report.failure=error.message.slice(0,250);process.exitCode=1;console.error(report.failure);}
 finally{fs.writeFileSync(path.join(output,'report.json'),JSON.stringify(report,null,2));if(session){await session.browser.close().catch(()=>{});session.server.child.kill();}fs.rmSync(path.join(data,'ai.json'),{force:true});}
})();
