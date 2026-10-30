'use strict';
// Actual AppKit panels + Cocoa/WKWebView + production file/save services.
// Model services remain deferred. The debug driver operates the real panel.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict'),vm=require('node:vm'),{spawn,spawnSync}=require('node:child_process');
const root=path.resolve(__dirname,'..'),workspace=path.dirname(root),sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const before='# Mac 原生文件选择\n\n这是专用测试文档。保留编码、换行与原件。🙂\n',added='\n另存为的新内容。🧭\n';
const encode=s=>Buffer.concat([Buffer.from([0xef,0xbb,0xbf]),Buffer.from(s.replace(/\n/g,'\r\n'))]);
function driver(phase,files,flow){return String.raw`(async()=>{
 const checks=[],phase=${JSON.stringify(phase)},files=${JSON.stringify(files)},flow=${JSON.stringify(flow)},expected=${JSON.stringify(flow==='cancel'?before:before+added)};
 const progress=(label=phase)=>window.ipc.postMessage(JSON.stringify({readmd_native_qa_progress:{ok:true,phase:label,checks}}));
 const check=(ok,name)=>{if(!ok)throw Error(name);checks.push(name);progress();};
 const sleep=ms=>new Promise(r=>setTimeout(r,ms));
 const until=async fn=>{const end=Date.now()+35000;while(Date.now()<end){if(await fn())return;await sleep(30);}throw Error('native_dialog_condition_timeout');};
 let id=0;
 const panel=async(choice,p)=>{const mine=++id;if(choice!=='status')progress('dialog_'+choice+'_requested');py.window_control('qa_native_dialog',{choice,path:p,id:mine});await until(()=>window.__readmdNativeDialogResult?.id===mine);if(choice!=='status')progress('dialog_'+choice+'_returned');return window.__readmdNativeDialogResult;};
 const choose=async(choice,p)=>{
  let available;await until(async()=>{available=await panel('status',p);return available.available&&available.visible;});check(true,'actual_appkit_panel_visible');
  if(choice==='accept'&&available.native_confirmation_automation_supported===false)throw Error('native_confirmation_requires_manual_input');
  // AppKit owns activation of its sheet. Forcing makeKeyAndOrderFront on a
  // remote macOS file-panel proxy can block; it is not a user operation.
  await panel('activate');await sleep(200);
  let last;
  if(choice==='accept'){
   last=await panel('configure',p);
   if(last.open&&!last.configured){
    await panel('go_to');await sleep(250);await panel('type_path',p);await sleep(100);await panel('return');await sleep(250);
   }
   try{await until(async()=>{last=await panel('status',p);return !last.available||last.configured;});}
   catch(error){checks.push(last?.directory_matches?'native_panel_directory_matches':'native_panel_directory_differs',last?.name_matches?'native_panel_name_matches':'native_panel_name_differs',last?.selected_matches?'native_panel_selection_matches':'native_panel_selection_differs','native_confirmation_requires_manual_input',last?.has_sheet?'native_panel_has_navigation_sheet':'native_panel_has_no_navigation_sheet');throw error;}
  }
  if(choice!=='accept'||last?.available!==false){const result=await panel(choice);if(choice==='accept')check(result.native_accept_succeeded===true,'native_key_panel_confirmation_input_dispatched');}
 };
 try{
  await __readmdSessionReady;
  check(hasPy&&document.body.classList.contains('native-macos'),'actual_native_mac_bridge');
  if(phase==='pick_save'){
   if(flow==='open'){
    const picking=py.choose_any_file();await choose('accept',files.input);check(await picking===files.input,'actual_open_panel_selected_chinese_path');
   }
   await loadFile(files.input);check(getActiveTab()?.path===files.input,'picked_document_opened_in_native_reader');
   check(getActiveTab().content.replace(/\r\n/g,'\n')===${JSON.stringify(before)},'picked_body_and_bom_decoded_exactly');
   const count=state.tabs.length,cancelPick=py.choose_any_file();await choose('cancel');
   check(await cancelPick===null&&state.tabs.length===count,'native_open_cancel_keeps_document_and_tabs');
   await toggleEdit();await until(()=>state.editing&&!!cmView);
   check(!hasUnsavedEditorChanges(),'opening_and_entering_edit_is_clean');
   cmView.dispatch({changes:{from:cmView.state.doc.length,insert:${JSON.stringify(added)}}});
   if(flow==='cancel'){
    const draft=getEditContent(),saving=saveAs();await choose('cancel');
    check(await saving===false&&getEditContent()===draft&&hasUnsavedEditorChanges(),'native_save_cancel_preserves_current_visible_draft');
    check(getActiveTab().path===files.input,'native_save_cancel_preserves_original_identity');
    cmView.dispatch({changes:{from:cmView.state.doc.length-${added.length},to:cmView.state.doc.length,insert:''}});
    check(!hasUnsavedEditorChanges(),'explicit_user_revert_returns_to_original_revision');
    check(await confirmExitEdit()===true,'reverted_editor_closes_without_confirmation');
   }else{
   const saving=saveAs();await choose('accept',files.saved);
   check(await saving===true&&getActiveTab().path===files.saved,'native_save_as_committed_and_retargeted');
   check(!hasUnsavedEditorChanges()&&getEditContent().replace(/\r\n/g,'\n')===expected,'saved_body_is_current_visible_text');
   const start=cmView.state.doc.length;cmView.dispatch({changes:{from:start,insert:'\n保留未保存的本人修改。\n'}});
   const dirty=getEditContent(),cancelSave=saveAs();await choose('cancel');
   check(await cancelSave===false&&getEditContent()===dirty&&hasUnsavedEditorChanges(),'native_save_cancel_preserves_user_draft');
   check(getActiveTab().path===files.saved,'save_cancel_keeps_document_identity');
   cmView.dispatch({changes:{from:start,to:cmView.state.doc.length,insert:''}});
   check(!hasUnsavedEditorChanges(),'explicit_user_revert_returns_to_saved_revision');
   check(await confirmExitEdit()===true,'clean_editor_closes_without_confirmation');
   }
  }else{
   await until(()=>getActiveTab()?.path===(flow==='cancel'?files.input:files.saved));
   check(getActiveTab().content.replace(/\r\n/g,'\n')===expected,flow==='cancel'?'original_document_restored_after_cancellation':'saved_as_document_restored_from_disk');
   await toggleEdit();await until(()=>state.editing&&!!cmView);
   check(!hasUnsavedEditorChanges(),'restored_document_is_clean_and_editable');
   check(await confirmExitEdit()===true,'restored_editor_closes_cleanly');
  }
  const r=await apiFetch('/api/modules'),m=await r.json();
  check(m.deferred_modules.includes('ocr')&&m.deferred_modules.includes('transcribe')&&m.modules.ocr==='idle'&&m.modules.transcribe==='idle','model_services_remain_deferred');
  check(await ReadMDSession.flush()!==false,'native_session_persisted');
  if(phase==='pick_save'){
   py.window_control('status');await until(()=>window.__readmdNativeWindowState?.chrome?.windowNumber>0);
   window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase:'capture_ready',checks:['owned_native_window_ready'],window_number:window.__readmdNativeWindowState.chrome.windowNumber}}));
   await until(async()=>{const r=await apiFetch('/api/file?p='+encodeURIComponent(files.ack));return r.ok&&(await r.json()).content==='continue';});
  }
  window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase,checks}}));await py.request_quit();
 }catch(error){const name=String(error.message||'native_dialog_failed');checks.push('failed_'+(/^[a-z_]{1,110}$/.test(name)?name:'native_dialog_failed'));window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:false,phase,checks}}));window.ipc.postMessage('readmd:quit');}
})()`;}
async function run(binary,out,flow='cancel'){
 assert(['open','save','cancel'].includes(flow));
 assert.equal(process.platform,'darwin');assert(path.isAbsolute(binary)&&out.startsWith(workspace+path.sep));fs.mkdirSync(out,{mode:0o700});
 const docs=path.join(out,'documents'),profile=path.join(out,'profile'),temp=path.join(out,'tmp'),home=path.join(out,'native-home');for(const p of [docs,profile,temp,home])fs.mkdirSync(p,{mode:0o700});
 const files={input:path.join(docs,'中文 原件.md'),saved:path.join(docs,'另存为 新文档.md'),ack:path.join(docs,'capture.ack.md')};
 fs.writeFileSync(files.input,encode(before),{flag:'wx'});fs.writeFileSync(path.join(profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1',{flag:'wx',mode:0o600});
 fs.writeFileSync(path.join(profile,'settings.json'),JSON.stringify({lang:'zh-CN',closeToTray:false,autoCheckUpdate:false}));
 const report={platform:'macos-'+process.arch,nativeHost:'Cocoa WKWebView / NSOpenPanel / NSSavePanel',openSelectionCovered:flow==='open',saveSelectionCovered:flow!=='cancel',cancelOnly:flow==='cancel',productionCsp:true,strictTokenMode:true,chooserShim:false,modelsRequested:false,binarySha256:sha(fs.readFileSync(binary)),bootSha256:sha(fs.readFileSync(path.join(root,'assets/readmd.boot.js'))),harnessSha256:sha(fs.readFileSync(__filename)),phases:[],passed:false};
 const policy='(version 1)(allow default)(deny file-write* (require-all (require-not (subpath '+JSON.stringify(workspace)+')) (require-not (subpath '+JSON.stringify('/System/Volumes/Data'+workspace)+')) (require-not (literal "/dev/null")) (require-not (literal "/dev/tty"))))';
 try{for(const phase of ['pick_save','reopen']){
  const script=driver(phase,files,flow);new vm.Script(script);fs.writeFileSync(path.join(profile,'native-qa.js'),script,{mode:0o600});const fd=fs.openSync(path.join(out,phase+'.log'),'wx'),resultFile=path.join(profile,'native-qa-result.json');
  const child=spawn('/usr/bin/sandbox-exec',['-p',policy,binary,'--port','0','--require-token','--data-dir',profile,'--workspace',docs,'--assets',path.join(root,'assets')],{cwd:out,env:{...process.env,READMD_NATIVE_QA:'1',READMD_STARTUP_PROBE:'1',READMD_DATA_DIR:profile,READMD_WORKSPACE:docs,READMD_EXTRA_ROOTS:'',TMPDIR:temp,SQLITE_TMPDIR:temp,CFFIXED_USER_HOME:home},stdio:['ignore',fd,fd]});fs.closeSync(fd);
  const exited=new Promise(resolve=>{child.once('exit',(code,signal)=>resolve({code,signal}));child.once('error',()=>resolve({code:null,error:'spawn_failed'}));});const timeout=setTimeout(()=>child.kill('SIGTERM'),90000);
  try{while(child.exitCode===null&&child.signalCode===null){if(fs.existsSync(resultFile)){
   const result=JSON.parse(fs.readFileSync(resultFile));if(result.phase==='capture_ready'){
    const probe=spawnSync(path.join(workspace,'.readmd-local/native-window-capture'),[String(child.pid),String(result.window_number)],{encoding:'utf8',timeout:10000});
    if(probe.status===0){report.captureProbe=JSON.parse(probe.stdout);const capture=spawnSync('/usr/sbin/screencapture',['-x','-o','-l',String(result.window_number),path.join(out,'native-reader.png')],{encoding:'utf8',timeout:10000});report.captureSucceeded=capture.status===0;}
    fs.renameSync(resultFile,path.join(out,'capture-ready.json'));fs.writeFileSync(files.ack,'continue',{flag:'wx'});
   }}await new Promise(r=>setTimeout(r,30));}}finally{clearTimeout(timeout);if(child.exitCode===null&&child.signalCode===null)child.kill('SIGTERM');}
  const outcome=await exited,result=fs.existsSync(resultFile)?JSON.parse(fs.readFileSync(resultFile)):{ok:false,phase,checks:['native_report_missing']};if(fs.existsSync(resultFile))fs.renameSync(resultFile,path.join(out,phase+'.json'));
  report.phases.push({...result,...outcome});assert.equal(outcome.code,0);assert.equal(result.ok,true,result.checks.join(','));
  assert.deepEqual(fs.readFileSync(files.input),encode(before));if(flow==='cancel')assert(!fs.existsSync(files.saved),'canceled Save As never creates output');else assert.deepEqual(fs.readFileSync(files.saved),encode(before+added));
 }
 report.passed=true;
 }finally{fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n');if(fs.existsSync(files.ack))fs.unlinkSync(files.ack);for(const p of [temp,home])fs.rmSync(p,{recursive:true,force:true});}
 console.log(JSON.stringify({nativeDialogsPassed:report.passed,phases:report.phases.length,checks:report.phases.reduce((n,p)=>n+p.checks.length,0),capture:report.captureSucceeded}));
}
if(require.main===module){const args=process.argv.slice(2);assert(args.length>=2&&args.length<=3,'absolute debug binary, new evidence directory, optional open/save flow required');run(path.resolve(args[0]),path.resolve(args[1]),args[2]).catch(e=>{console.error(e.message);process.exitCode=1;});}
