'use strict';
// Actual PowerBox selections require a human confirmation; no replacement
// result, synthetic acceptance or accessibility/global-input automation.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),vm=require('node:vm');
const {spawnSync}=require('node:child_process'),{prepare}=require('./macos-native-system-host.cjs');
const {driver,before,added,encode}=require('./macos-dialog-native.cjs');
const root=path.resolve(__dirname,'..'),workspace=path.dirname(root),sha=b=>crypto.createHash('sha256').update(b).digest('hex'),pause=ms=>new Promise(r=>setTimeout(r,ms));
function manual(files,openWithHuman=true){return String.raw`(async()=>{
 const files=${JSON.stringify(files)},checks=[],phase='pick_save';let id=0;
 const originalFetch=window.fetch;let saveObservation;
 // Passive observation only: dispatch the identical request and return the
 // identical Response. Do not log tokens, request bodies, paths or raw errors.
 window.fetch=function(...args){const pending=originalFetch.apply(this,args);try{
  if(new URL(typeof args[0]==='string'?args[0]:args[0].url,location.href).pathname==='/api/dialog/save-as')pending.then(async r=>{let d={};try{d=await r.clone().json();}catch(_){}if(!saveObservation)saveObservation={status:r.status,ok:d.ok===true,canceled:d.canceled===true,pathMatches:d.path===files.saved,error:typeof d.error_code==='string'&&/^[a-z0-9_]{1,90}$/.test(d.error_code)?d.error_code:null};}).catch(()=>{saveObservation??={transportError:true};});
 }catch(_){}return pending;};
 const progress=phase=>window.ipc.postMessage(JSON.stringify({readmd_native_qa_progress:{ok:true,phase,checks}}));
 const check=(ok,name)=>{if(!ok)throw Error(name);checks.push(name);};
 const until=async fn=>{const end=Date.now()+35000;while(Date.now()<end){if(await fn())return;await new Promise(r=>setTimeout(r,50));}throw Error('manual_panel_condition_timeout');};
 const panel=async(choice,p)=>{const mine=++id;py.window_control('qa_native_dialog',{choice,path:p,id:mine});await until(()=>window.__readmdNativeDialogResult?.id===mine);return window.__readmdNativeDialogResult;};
 const configure=async(p,label,initialName=false)=>{
  let status;await until(async()=>{status=await panel('status',p);return status.available&&status.visible;});
  // NSOpenPanel has a file-list selection, not the Save panel's filename
  // field. Only the directory is pre-positioned there; the human picks a row.
  if(!initialName&&(!status.open||!status.directory_matches))await panel('configure',p);
  try{await until(async()=>{status=await panel('status',p);return status.directory_matches&&(status.open||status.name_matches);});}
  catch(error){checks.push(status?.directory_matches?'native_directory_matches':'native_directory_differs',status?.name_matches?'native_name_matches':'native_name_differs');throw error;}
  progress(label);
 };
 async function actualChoice(pending){
  // A workspace marker only cancels this synthetic test. It never supplies a
  // selection, path or success to the production native panel completion.
  let stopped=false;const watch=setInterval(async()=>{if(stopped)return;try{const r=await apiFetch('/api/file?p='+encodeURIComponent(files.abort));if(r.ok){stopped=true;await panel('cancel');}}catch(_){}},500);
  try{return await pending;}finally{stopped=true;clearInterval(watch);}
 }
 try{
  await __readmdSessionReady;check(hasPy&&document.body.classList.contains('native-macos'),'actual_native_mac_bridge');
  // The same authenticated native route as the bridge, with its supported
  // initial-directory parameter pointing only at this owned synthetic folder.
  ${openWithHuman?`const choosing=apiFetch('/api/dialog/choose-any-file',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({dir:${JSON.stringify(path.dirname(files.input))}})}).then(r=>r.json());
  await configure(files.input,'manual_open_ready');const picked=await actualChoice(choosing);check(picked.path===files.input,'actual_native_open_selection_confirmed');
  await loadFile(picked.path);check(getActiveTab()?.path===files.input,'selected_document_opened');`:`await loadFile(files.input);check(getActiveTab()?.path===files.input,'owned_fixture_loaded');`}
  check(getActiveTab().content.replace(/\r\n/g,'\n')===${JSON.stringify(before)},'selected_body_exact');
  await toggleEdit();await until(()=>state.editing&&!!cmView);check(!hasUnsavedEditorChanges(),'selected_document_enters_edit_clean');
  cmView.dispatch({changes:{from:cmView.state.doc.length,insert:${JSON.stringify(added)}}});
  // Supply the real save service's suggested name before constructing the
  // secure panel. Post-presentation proxy setters can read back the new name
  // while the actual PowerBox selection still uses the original filename.
  const snapshot=documentSaveSnapshot();snapshot.name=${JSON.stringify(path.basename(files.saved))};
  const saving=ReadMDTask.run('document-save',()=>saveAsSnapshot(snapshot,{retarget:true}),{trigger:['edit-save','btn-saveas']});
  await configure(files.saved,'manual_save_ready',true);check(await actualChoice(saving)===true,'actual_native_save_as_committed');
  check(getActiveTab().path===files.saved,'save_as_creates_new_identity');check(!hasUnsavedEditorChanges()&&getEditContent().replace(/\r\n/g,'\n')===${JSON.stringify(before+added)},'save_as_keeps_current_visible_text');
  const from=cmView.state.doc.length;cmView.dispatch({changes:{from,insert:'\n专用未保存草稿。\n'}});const dirty=getEditContent(),cancelled=saveAs();await until(async()=>{const s=await panel('status');return s.available&&s.visible;});await panel('cancel');
  check(await cancelled===false&&getEditContent()===dirty&&hasUnsavedEditorChanges(),'native_save_cancel_keeps_user_draft');check(getActiveTab().path===files.saved,'cancel_preserves_new_identity');
  cmView.dispatch({changes:{from,to:cmView.state.doc.length,insert:''}});check(!hasUnsavedEditorChanges(),'explicit_revert_returns_to_saved_revision');check(await confirmExitEdit()===true,'saved_editor_closes_cleanly');
  const modules=await(await apiFetch('/api/modules')).json();check(modules.modules.ocr==='idle'&&modules.modules.transcribe==='idle','model_services_remain_deferred');check(await ReadMDSession.flush()!==false,'native_session_persisted');
  py.window_control('status');await until(()=>window.__readmdNativeWindowState?.chrome?.windowNumber>0);window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase:'capture_ready',checks:['owned_native_window_ready'],window_number:window.__readmdNativeWindowState.chrome.windowNumber}}));
  await until(async()=>{const r=await apiFetch('/api/file?p='+encodeURIComponent(files.ack));return r.ok;});window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase,checks}}));await py.request_quit();
 }catch(error){
  if(saveObservation){if(saveObservation.transportError)checks.push('observed_save_transport_error');else{checks.push('observed_save_http_'+saveObservation.status,'observed_save_ok_'+saveObservation.ok,'observed_save_canceled_'+saveObservation.canceled,'observed_save_expected_path_'+saveObservation.pathMatches);if(saveObservation.error)checks.push('observed_save_error_'+saveObservation.error);}}
  checks.push('failed_'+(/^[a-z_]{1,110}$/.test(error.message)?error.message:'manual_dialog'));window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:false,phase,checks}}));window.ipc.postMessage('readmd:quit');
 }finally{window.fetch=originalFetch;}
})();`;}
async function run(app,out,mode='cancel'){
 assert(['cancel','manual','manual-save'].includes(mode));assert(out.startsWith(workspace+path.sep));fs.mkdirSync(out,{mode:0o700});const docs=path.join(out,'documents'),profile=path.join(out,'profile'),tmp=path.join(out,'tmp'),home=path.join(out,'native-home');for(const d of [docs,profile,tmp,home])fs.mkdirSync(d,{mode:0o700});
 const files={input:path.join(docs,'中文 原件.md'),saved:path.join(docs,'另存为 新文档.md'),ack:path.join(docs,'capture.ack.md'),abort:path.join(docs,'abort.ack.md')};fs.writeFileSync(files.input,encode(before),{flag:'wx'});fs.writeFileSync(path.join(profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1',{flag:'wx',mode:0o600});fs.writeFileSync(path.join(profile,'settings.json'),JSON.stringify({lang:'zh-CN',closeToTray:false,autoCheckUpdate:false}));
 const report={nativeHost:'LaunchServices / Cocoa WKWebView / NSOpenPanel / NSSavePanel',defaultAssociationsChanged:false,modelsRequested:false,productionCsp:true,strictAuth:true,chooserShim:false,syntheticConfirmation:false,cancelOnly:mode==='cancel',manualNativeConfirmation:mode!=='cancel',openSelectionCovered:mode==='manual',suggestedNameBeforePanel:mode!=='cancel',passiveTransportObserver:mode!=='cancel',harnessSha256:sha(fs.readFileSync(__filename)),driverSha256:sha(fs.readFileSync(require.resolve('./macos-dialog-native.cjs'))),hostSha256:sha(fs.readFileSync(require.resolve('./macos-native-system-host.cjs'))),bootSha256:sha(fs.readFileSync(path.join(root,'assets/readmd.boot.js'))),phases:[],passed:false};
 try{const host=prepare(app,out,{tmp,home});report.binarySha256=host.binarySha256;for(const phase of ['pick_save','reopen']){
  const script=mode!=='cancel'&&phase==='pick_save'?manual(files,mode==='manual'):driver(phase,files,mode!=='cancel'?'save':'cancel');new vm.Script(script);fs.writeFileSync(path.join(profile,'native-qa.js'),script,{mode:0o600});const resultFile=path.join(profile,'native-qa-result.json'),instance=await host.launch({profile,docs,log:path.join(out,phase+'.log')}),entry={phase,host:instance.receipt,passed:false};report.phases.push(entry);
  try{const waiting=instance.wait(mode!=='cancel'?900000:90000);waiting.catch(()=>{});if(phase==='pick_save'){
    const end=Date.now()+(mode!=='cancel'?900000:90000);while(!fs.existsSync(resultFile)&&!instance.launcherStopped&&Date.now()<end)await pause(100);const ready=JSON.parse(fs.readFileSync(resultFile));entry.firstResult=ready;assert.equal(ready.ok,true,ready.checks.join(','));assert.equal(ready.phase,'capture_ready');
    const helper=path.join(workspace,'.readmd-local/native-window-capture');if(fs.existsSync(helper)){const p=spawnSync(helper,[String(instance.pid),String(ready.window_number)],{encoding:'utf8',timeout:10000});assert.equal(p.status,0);report.captureProbe=JSON.parse(p.stdout);assert(report.captureProbe.ownedWindowOnScreen);if(report.captureProbe.captureAllowed){const f=path.join(out,'native-reader.png'),captured=spawnSync('/usr/sbin/screencapture',['-x','-o','-l',String(ready.window_number),f],{timeout:10000});assert.equal(captured.status,0);report.screenshotSha256=sha(fs.readFileSync(f));}}
    fs.renameSync(resultFile,path.join(out,'capture-ready.json'));fs.writeFileSync(files.ack,'continue',{flag:'wx'});
   }
   await waiting;const result=JSON.parse(fs.readFileSync(resultFile));fs.renameSync(resultFile,path.join(out,phase+'.json'));entry.result=result;assert.equal(result.ok,true,result.checks.join(','));assert.equal(result.phase,phase);assert.deepEqual(fs.readFileSync(files.input),encode(before),'original bytes never change');
   if(mode!=='cancel')assert.deepEqual(fs.readFileSync(files.saved),encode(before+added),'Save As commits visible body with BOM and CRLF');else assert(!fs.existsSync(files.saved),'cancel never creates a new file');entry.passed=true;
  }finally{await instance.cleanup();}
 }report.passed=true;
 }finally{
  for(const [f,expected]of [[files.ack,'continue'],[files.abort,'cancel']])if(fs.existsSync(f)){
   const s=fs.lstatSync(f);if(s.isFile()&&!s.isSymbolicLink()&&s.nlink===1&&fs.readFileSync(f,'utf8')===expected)fs.unlinkSync(f);
   else(report.unrecognizedControlFilesRetained??=[]).push(path.basename(f));
  }
  for(const d of [tmp,home])fs.rmSync(d,{recursive:true,force:true});fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx',mode:0o600});
 }
 console.log(JSON.stringify({nativeSystemDialogsPassed:report.passed,cancelOnly:report.cancelOnly,phases:report.phases.length,checks:report.phases.reduce((n,p)=>n+p.result.checks.length,0)}));
}
if(require.main===module){const args=process.argv.slice(2);assert(args.length===2||args.length===3);run(path.resolve(args[0]),path.resolve(args[1]),args[2]).catch(e=>{console.error(e.message);process.exitCode=1;});}
module.exports={manual};
