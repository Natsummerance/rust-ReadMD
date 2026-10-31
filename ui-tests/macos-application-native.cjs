'use strict';
// Owned Cocoa delegate/menu events, actual WKWebView and durable file/session.
// No global input, chooser shim, credentials substitute or model inference.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict'),vm=require('node:vm'),{spawn}=require('node:child_process');
const root=path.resolve(__dirname,'..'),workspace=path.dirname(root),sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const original='# Mac 原生生命周期\n\n关闭窗口、取消退出与保存恢复。🙂\n',addition='\n这是本人尚未保存的修改。\n';
function driver(phase,file){return String.raw`(async()=>{
 const checks=[],phase=${JSON.stringify(phase)},file=${JSON.stringify(file)};
 const progress=()=>window.ipc.postMessage(JSON.stringify({readmd_native_qa_progress:{ok:true,phase,checks}}));
 const check=(ok,name)=>{if(!ok)throw Error(name);checks.push(name);progress();};
 const pause=ms=>new Promise(r=>setTimeout(r,ms));
 const until=async fn=>{const deadline=Date.now()+25000;while(Date.now()<deadline){py.window_control('status');if(await fn())return;await pause(40);}throw Error('native_lifecycle_timeout');};
 let id=0;
 const dispatch=async(channel,action,extra={})=>{
  const mine=++id;py.window_control(channel,{action,id:mine,...extra});
  const field=channel==='qa_tray'?'__readmdNativeTrayResult':'__readmdNativeApplicationResult';
  await until(()=>window[field]?.id===mine);check(window[field].performed,'native_'+action+'_dispatched');
 };
 const expectBody=${JSON.stringify(original+addition)};
 try{
  progress();await Promise.race([__readmdSessionReady,pause(25000).then(()=>{throw Error('session_restore_timeout');})]);check(hasPy&&document.body.classList.contains('native-macos'),'actual_native_mac_bridge');
  if(phase==='lifecycle'){
   check(!state.tabs.length,'owned_profile_starts_on_home');
   await dispatch('qa_application','open_file',{path:file});
   await until(()=>getActiveTab()?.path===file);
   check(getActiveTab().content===${JSON.stringify(original)},'native_delegate_opened_chinese_document');
   check(!getActiveTab().isDirty,'native_open_does_not_mark_document_dirty');
   await until(()=>window.__readmdNativeWindowState?.trayAvailable);
   check(true,'actual_native_status_item_available');
   state.closeToTray=true;syncWindowPreferences();
   py.window_control('close');await until(()=>window.__readmdNativeWindowState?.visible===false);
   check(getActiveTab()?.path===file,'close_to_menu_bar_keeps_document_session');
   await dispatch('qa_application','reopen');await until(()=>window.__readmdNativeWindowState?.visible===true);
   check(getActiveTab()?.content===${JSON.stringify(original)},'dock_reopen_restores_same_native_window_and_body');
   py.window_control('close');await until(()=>window.__readmdNativeWindowState?.visible===false);
   await dispatch('qa_tray','show');await until(()=>window.__readmdNativeWindowState?.visible===true);
   check(true,'native_menu_show_restores_hidden_reader');
   await toggleEdit();await until(()=>state.editing&&!!cmView);
   check(!hasUnsavedEditorChanges(),'entering_edit_stays_clean');
   cmView.dispatch({changes:{from:cmView.state.doc.length,insert:${JSON.stringify(addition)}}});
   check(await ReadMDReview.flush()!==false,'initial_edit_persisted_before_native_history');
   cmView.focus();await dispatch('qa_application','undo_menu');
   await until(()=>getEditContent()===${JSON.stringify(original)});
   check(!hasUnsavedEditorChanges(),'native_undo_returns_to_unchanged_saved_body');
   check(await ReadMDReview.flush()!==false,'native_undo_persisted_before_redo');
   check(ReadMDReview.snapshot()?.can_redo===true,'native_undo_retains_durable_redo');
   await dispatch('qa_application','redo_menu');await until(()=>getEditContent()===expectBody);
   check(hasUnsavedEditorChanges(),'native_redo_restores_edit_and_dirty_revision');
   for(const action of ['quit_key','terminate']){
    await dispatch('qa_application',action);
    await until(()=>!$('close-confirm-modal').classList.contains('hidden'));
    $('close-confirm-cancel').click();await until(()=>$('close-confirm-modal').classList.contains('hidden'));
    check(getEditContent()===expectBody&&hasUnsavedEditorChanges(),'cancel_'+action+'_preserves_unsaved_body');
   }
   await dispatch('qa_tray','exit');await until(()=>!$('close-confirm-modal').classList.contains('hidden'));
   $('close-confirm-cancel').click();await until(()=>$('close-confirm-modal').classList.contains('hidden'));
   check(getEditContent()===expectBody&&hasUnsavedEditorChanges(),'cancel_native_menu_quit_preserves_unsaved_body');
   cmView.focus();await dispatch('qa_application','save_key');
   await until(()=>!hasUnsavedEditorChanges());
   check(getEditContent()===expectBody,'native_command_s_commits_current_visible_body');
   check(!hasUnsavedEditorChanges(),'saved_document_returns_clean');
  }else{
   await until(()=>getActiveTab()?.path===file);
   check(getActiveTab().content===expectBody,'normal_native_quit_restores_saved_session');
   await toggleEdit();await until(()=>state.editing&&!!cmView);
   check(!hasUnsavedEditorChanges(),'restarted_editor_matches_saved_revision');
  }
  const r=await apiFetch('/api/modules'),m=await r.json();
  check(m.modules.ocr==='idle'&&m.modules.transcribe==='idle','model_services_stay_unloaded');
  check(await ReadMDSession.flush()!==false,'session_persisted_before_native_quit');
  window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase,checks}}));
  py.window_control('qa_application',{action:'terminate',id:++id});
 }catch(error){checks.push('failed_'+(/^[a-z_]{1,110}$/.test(error.message)?error.message:'native_lifecycle'));window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:false,phase,checks}}));window.ipc.postMessage('readmd:quit');}
})()`;}
async function run(binary,out){
 assert.equal(process.platform,'darwin');assert(out.startsWith(workspace+path.sep));fs.mkdirSync(out,{mode:0o700});
 const docs=path.join(out,'documents'),profile=path.join(out,'profile'),temp=path.join(out,'tmp'),home=path.join(out,'native-home');for(const p of [docs,profile,temp,home])fs.mkdirSync(p,{mode:0o700});
 const file=path.join(docs,'中文 原生菜单与保存.md');fs.writeFileSync(file,original,{flag:'wx'});
 fs.writeFileSync(path.join(profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1',{flag:'wx',mode:0o600});
 fs.writeFileSync(path.join(profile,'settings.json'),JSON.stringify({lang:'zh-CN',closeToTray:true,autoCheckUpdate:false}));
 const report={nativeHost:'Cocoa / WKWebView / NSApplication delegate / NSMenu / NSStatusItem',strictToken:true,productionCsp:true,globalInput:false,modelsRequested:false,binarySha256:sha(fs.readFileSync(binary)),bootSha256:sha(fs.readFileSync(path.join(root,'assets/readmd.boot.js'))),harnessSha256:sha(fs.readFileSync(__filename)),phases:[],passed:false};
 const policy='(version 1)(allow default)(deny file-write* (require-all (require-not (subpath '+JSON.stringify(workspace)+')) (require-not (subpath '+JSON.stringify('/System/Volumes/Data'+workspace)+')) (require-not (literal "/dev/null")) (require-not (literal "/dev/tty"))))';
 try{for(const phase of ['lifecycle','reopen']){
  const script=driver(phase,file);new vm.Script(script);fs.writeFileSync(path.join(profile,'native-qa.js'),script,{mode:0o600});
  const fd=fs.openSync(path.join(out,phase+'.log'),'wx');
  const child=spawn('/usr/bin/sandbox-exec',['-p',policy,binary,'--port','0','--require-token','--data-dir',profile,'--workspace',docs,'--assets',path.join(root,'assets')],{cwd:out,env:{...process.env,READMD_NATIVE_QA:'1',READMD_STARTUP_PROBE:'1',READMD_DATA_DIR:profile,READMD_WORKSPACE:docs,READMD_EXTRA_ROOTS:'',TMPDIR:temp,SQLITE_TMPDIR:temp,CFFIXED_USER_HOME:home},stdio:['ignore',fd,fd]});fs.closeSync(fd);
  const timer=setTimeout(()=>child.kill('SIGTERM'),90000);
  const outcome=await new Promise(resolve=>{child.once('exit',(code,signal)=>resolve({code,signal}));child.once('error',()=>resolve({code:null,error:'spawn_failed'}));});clearTimeout(timer);
  const resultFile=path.join(profile,'native-qa-result.json'),result=fs.existsSync(resultFile)?JSON.parse(fs.readFileSync(resultFile)):{ok:false,checks:['native_report_missing']};
  if(fs.existsSync(resultFile))fs.renameSync(resultFile,path.join(out,phase+'.json'));
  report.phases.push({...result,...outcome});assert.equal(outcome.code,0);assert.equal(result.ok,true,result.checks.join(','));assert.equal(fs.readFileSync(file,'utf8'),original+addition);
 }report.passed=true;}finally{fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n');for(const p of [temp,home])fs.rmSync(p,{recursive:true,force:true});}
 console.log(JSON.stringify({nativeApplicationPassed:report.passed,phases:report.phases.length,checks:report.phases.reduce((n,p)=>n+p.checks.length,0)}));
}
if(require.main===module){const args=process.argv.slice(2);assert.equal(args.length,2);run(...args.map(p=>path.resolve(p))).catch(e=>{console.error(e.message);process.exitCode=1;});}
module.exports={driver,original,addition};
