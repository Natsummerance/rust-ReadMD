'use strict';
// Dedicated native profile, actual Rust CAS and history. No model or chooser mocks.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {prepare}=require('./macos-native-system-host.cjs');
const root=path.resolve(__dirname,'..'),workspace=path.dirname(root),sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const original=Buffer.from('\ufeff# 中文固定原件\r\n\r\n原件不变。🙂\r\n'),body='# 找回的未保存正文\n\n只有真实草稿可恢复。🙂\n';
function driver(phase,raw){return String.raw`(async()=>{
 const phase=${JSON.stringify(phase)},raw=${JSON.stringify(raw)},checks=[];
 const check=(ok,name)=>{if(!ok)throw Error(name);checks.push(name);};
 const until=async fn=>{const end=Date.now()+15000;while(Date.now()<end){if(await fn())return;await new Promise(r=>setTimeout(r,30));}throw Error('native_condition_timeout');};
 const session=async()=>{const r=await apiFetch('/api/session');check(r.ok,'actual_session_read');return r.json();};
 try{
  await __readmdSessionReady;check(hasPy&&READMD_ENGINE==='rust'&&document.body.classList.contains('native-macos'),'actual_native_mac_and_rust_bridge');
  if(phase==='guard_and_quit'){
   const held=getActiveTab();check(state.tabs.length===2&&held.recoveryKey===raw.key&&!!held._restoreDescriptor,'missing_unsaved_page_remains_visible');
   check(held.content===null&&state.original===null&&!state.editing,'missing_text_is_not_an_empty_document');
   check(await ReadMDReview.capture(held)===null,'no_empty_review_or_ai_target');
   check(await saveAsSnapshot(documentSaveSnapshot())===false,'no_empty_save_as');
   check(await closeTab(held.id,true)===false&&state.tabs.includes(held),'ordinary_force_close_cannot_forget_missing_body');
   const clean=state.tabs.find(tab=>tab!==held);await closeTab(clean.id);check(state.tabs.length===1&&getActiveTab()===held,'other_clean_page_can_close');
   check(await ReadMDSession.flush()===true,'guarded_session_acknowledged');
   const before=await session();check(before.recovery_guards===true,'rust_explicitly_advertises_recovery_guards');
   check(JSON.stringify(before.session.tabs[0])===JSON.stringify(raw),'original_descriptor_preserved_exactly');
   const rejected=await apiFetch('/api/session',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({session:{version:1,tabs:[],active:'',folder:''},preserved_tabs:[raw],expected_session_version:before.session_version,call_id:'owned-forbidden-'+crypto.randomUUID()})});
   check(rejected.status===409,'actual_rust_rejects_descriptor_removal');
   const error=await rejected.json();check(!error.session_version&&!error.session,'conflict_does_not_expose_a_new_head');
   const after=await session();check(JSON.stringify(after.session_version)===JSON.stringify(before.session_version)&&JSON.stringify(after.session.tabs[0])===JSON.stringify(raw),'rejection_leaves_version_and_descriptor_unchanged');
   const history=await(await apiFetch('/api/documents/history')).json();check(history.entries.length===0,'no_invented_empty_recovery_record');
   check(await ReadMDRecovery.prepareInstall()===true,'normal_exit_barriers_preserve_missing_record');
  }else if(phase==='recover_checkpoint'){
   const held=getActiveTab();check(state.tabs.length===1&&!!held._restoreDescriptor,'missing_body_survives_a_real_restart');
   const recorded=await apiFetch('/api/documents/history',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({op:'record',kind:'draft',reason:'edit',key:raw.key,path:'',name:raw.name,content:${JSON.stringify(body)},context:{}})});
   check(recorded.ok,'real_recovery_body_committed');
   check(await ReadMDSession.retryRecovery(held)===true,'actual_body_retry_succeeds');
   check(getActiveTab().id===held.id&&!getActiveTab()._restoreDescriptor&&getEditContent()===${JSON.stringify(body)},'same_page_identity_gets_only_the_recovered_body');
   check(state.editing&&getActiveTab().isVirtual&&getActiveTab().unsavedCreation,'recovered_new_document_still_needs_explicit_save');
   check(await ReadMDReview.flushAll()===true&&await ReadMDRecovery.flush()===true&&await ReadMDSession.flush()===true,'all_recovery_checkpoint_layers_committed');
  }else if(phase==='reopen_discard'){
   check(state.tabs.length===1&&state.editing&&getEditContent()===${JSON.stringify(body)},'checkpoint_body_reopens_exactly');
   const closing=closeTab(getActiveTab().id);await until(()=>!$('close-confirm-modal').classList.contains('hidden'));$('close-confirm-discard').click();await closing;
   check(state.tabs.length===0&&!state.editing,'explicit_discard_closes_only_recovered_page');
   const list=await(await apiFetch('/api/documents/history')).json(),entry=list.entries.find(row=>row.kind==='discarded');check(!!entry,'discarded_recovery_body_retained');
   const recovered=await(await apiFetch('/api/documents/history',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({op:'read',id:entry.id})})).json();check(recovered.content===${JSON.stringify(body)},'discarded_body_remains_exactly_recoverable');
  }else check(state.tabs.length===0&&!getActiveTab(),'explicitly_closed_recovery_page_stays_closed');
  check(await ReadMDSession.flush()===true,'final_session_acknowledged');
  window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase,checks}}));
  if(phase==='recover_checkpoint')window.ipc.postMessage('readmd:quit');
  else await py.request_quit();
 }catch(error){checks.push('failed_'+String(error.message).replace(/[^a-z0-9_]/g,'_').slice(0,90));window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:false,phase,checks}}));window.ipc.postMessage('readmd:quit');}
})();`;}
async function run(app,out){
 assert(out.startsWith(workspace+path.sep));fs.mkdirSync(out,{mode:0o700});
 const dirs=Object.fromEntries(['profile','documents','tmp','native-home'].map(name=>{const p=path.join(out,name);fs.mkdirSync(p);return[name,p];}));
 const file=path.join(dirs.documents,'中文 原件.md');fs.writeFileSync(file,original,{flag:'wx'});
 const raw={key:'draft:owned-missing-body',path:'',name:'未保存正文.md',dirty:true,unsavedCreation:true,reviewPath:'draft:owned-missing-body',futureMetadata:{witness:'preserve-exactly'}};
 fs.writeFileSync(path.join(dirs.profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1');
 fs.writeFileSync(path.join(dirs.profile,'settings.json'),JSON.stringify({lang:'zh-CN',closeToTray:false,autoCheckUpdate:false,petEnabled:false,session:{version:1,tabs:[{key:'file:owned-original',path:file,name:'中文 原件.md',dirty:false},raw],active:raw.key,folder:''}}));
 const report={nativeHost:'LaunchServices / Cocoa WKWebView',bootSha256:sha(fs.readFileSync(path.join(root,'assets/readmd.boot.js'))),indexSha256:sha(fs.readFileSync(path.join(root,'assets/index.html'))),productionCsp:true,strictAuthentication:true,modelsRequested:false,controlledCheckpointExitIsPowerLoss:false,phases:[],passed:false};
 try{
  const host=prepare(app,out,{tmp:dirs.tmp,home:dirs['native-home']});report.binarySha256=host.binarySha256;
  for(const phase of ['guard_and_quit','recover_checkpoint','reopen_discard','empty']){
   fs.writeFileSync(path.join(dirs.profile,'native-qa.js'),driver(phase,raw));const instance=await host.launch({profile:dirs.profile,docs:dirs.documents,log:path.join(out,phase+'.log')});
   try{
    await instance.wait(60000);const resultFile=path.join(dirs.profile,'native-qa-result.json'),result=JSON.parse(fs.readFileSync(resultFile));fs.renameSync(resultFile,path.join(out,phase+'.json'));
    report.phases.push({phase,result,host:instance.receipt,normalUserQuit:phase!=='recover_checkpoint',controlledAfterCommittedCheckpoint:phase==='recover_checkpoint'});assert.equal(result.ok,true,result.checks.join(','));assert.deepEqual(fs.readFileSync(file),original);
   }finally{await instance.cleanup();}
  }report.passed=true;
 }finally{
  fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx',mode:0o600});for(const name of ['tmp','native-home'])fs.rmSync(dirs[name],{recursive:true,force:true});
 }
 console.log(JSON.stringify({nativeProtectedRecoveryPassed:true,phases:report.phases.length,checks:report.phases.reduce((n,p)=>n+p.result.checks.length,0),controlledCheckpointExit:true}));
}
if(require.main===module){const[app,out]=process.argv.slice(2);run(path.resolve(app),path.resolve(out)).catch(error=>{console.error(error.message);process.exitCode=1;});}
module.exports={driver,original,body};
