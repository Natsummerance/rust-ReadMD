'use strict';
// Actual native host and HTTP 404, with six fixed synthetic page identities.
// No chooser, authentication, response replacement or model service.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {prepare}=require('./macos-native-system-host.cjs');
const root=path.resolve(__dirname,'..'),workspace=path.dirname(root),pause=ms=>new Promise(r=>setTimeout(r,ms));
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const body=i=>'# 恢复页面 '+i+'\n\n固定中文正文。🙂\n';
const bytes=i=>Buffer.from('\ufeff'+body(i).replace(/\n/g,'\r\n'));
function driver(phase,files){return String.raw`(async()=>{
 const phase=${JSON.stringify(phase)},files=${JSON.stringify(files)},checks=[];
 const check=(ok,name)=>{if(!ok)throw Error(name);checks.push(name);};
 const until=async fn=>{const end=Date.now()+25000;while(Date.now()<end){if(await fn())return;await new Promise(r=>setTimeout(r,40));}throw Error('condition_timeout');};
 try{
  await __readmdSessionReady;check(hasPy&&document.body.classList.contains('native-macos'),'actual_native_bridge');
  if(phase==='missing_retry'){
   const missing=getActiveTab();check(state.tabs.length===6&&missing.path===files[5],'six_page_identities_retained');
   check(missing.mode==='unavailable'&&missing.content===null&&missing.original===null,'failed_read_is_explicit_not_empty_source');
   check(!!document.querySelector('.restore-unavailable [data-restore-retry]'),'visible_retry_action');
   check($('btn-edit').disabled&&$('btn-saveas').disabled&&$('btn-print').disabled,'unavailable_edit_save_export_disabled');
   await toggleEdit();check(!state.editing,'direct_edit_entry_refused');
   check(await ReadMDReview.capture(missing)===null,'no_empty_ai_target');
   check(await saveAsSnapshot(documentSaveSnapshot())===false,'direct_save_as_refused');
   check(await ReadMDSession.flush()===true,'unavailable_descriptor_persisted');
   const history=await(await apiFetch('/api/documents/history')).json();check(!history.entries.some(e=>e.path===files[5]),'no_empty_recovery_draft_created');
   window.ipc.postMessage(JSON.stringify({readmd_native_qa_progress:{ok:true,phase:'await_owned_file',checks}}));
   await until(async()=>{const r=await apiFetch('/api/file?p='+encodeURIComponent(files[5]));return r.ok;});
   document.querySelector('[data-restore-retry]').click();
   await until(()=>getActiveTab()===missing&&!missing._restoreUnavailable&&missing.mode==='file');
   check(missing.content.replace(/^\uFEFF/,'').replace(/\r\n/g,'\n')===${JSON.stringify(body(5))},'retry_opens_real_newly_available_bytes');
   check(state.tabs.length===6&&!missing.isDirty,'retry_keeps_page_identity_clean');
  }else if(phase==='explicit_close'){
   check(state.tabs.length===6,'restart_does_not_drop_missing_page');
   const missing=state.tabs.find(tab=>tab.path===files[0]);check(!!missing?._restoreUnavailable,'real_deleted_fixture_reported_unavailable');
   await switchTab(missing.id);check(state.mode==='unavailable','unavailable_background_page_can_be_selected');
   await closeTab(missing.id,false,'button');check(!state.tabs.includes(missing)&&state.tabs.length===5,'explicit_close_removes_only_selected_identity');
   check(state.tabs.every(tab=>tab.path!==files[0]),'closed_unavailable_page_not_reintroduced');
  }else{
   check(state.tabs.length===5&&state.tabs.every(tab=>!tab._restoreUnavailable),'all_remaining_five_pages_reopen_ready');
   check(!state.tabs.some(tab=>tab.path===files[0]),'explicitly_closed_missing_page_stays_closed');
  }
  check(await ReadMDSession.flush()===true,'final_session_acknowledged');
  window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase,checks}}));await py.request_quit();
 }catch(error){checks.push('failed_'+String(error.message).replace(/[^a-z0-9_]/g,'_').slice(0,90));window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:false,phase,checks}}));window.ipc.postMessage('readmd:quit');}
})();`;}
async function run(app,out){
 assert(out.startsWith(workspace+path.sep));fs.mkdirSync(out,{mode:0o700});
 const dirs=Object.fromEntries(['profile','documents','tmp','native-home'].map(n=>{const p=path.join(out,n);fs.mkdirSync(p);return[n,p];}));
 const files=[];for(let i=0;i<6;i++){const folder=path.join(dirs.documents,i<3?'资料':'报告');fs.mkdirSync(folder,{recursive:true});const f=path.join(folder,'中文 页面 '+i+'.md');files.push(f);if(i<5)fs.writeFileSync(f,bytes(i),{flag:'wx'});}
 const session={version:1,tabs:files.map((file,i)=>({key:'file:owned-'+i,path:file,name:path.basename(file),dirty:false,source:'file'})),active:'file:owned-5',folder:''};
 fs.writeFileSync(path.join(dirs.profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1');
 fs.writeFileSync(path.join(dirs.profile,'settings.json'),JSON.stringify({lang:'zh-CN',closeToTray:false,autoCheckUpdate:false,petEnabled:false,session}));
 const report={nativeHost:'LaunchServices / Cocoa WKWebView',realHttp404:true,productionCsp:true,strictAuthentication:true,modelsRequested:false,bootSha256:sha(fs.readFileSync(path.join(root,'assets/readmd.boot.js'))),indexSha256:sha(fs.readFileSync(path.join(root,'assets/index.html'))),phases:[],passed:false};
 try{
  const host=prepare(app,out,{tmp:dirs.tmp,home:dirs['native-home']});report.binarySha256=host.binarySha256;
  for(const phase of ['missing_retry','explicit_close','reopen']){
   fs.writeFileSync(path.join(dirs.profile,'native-qa.js'),driver(phase,files));
   const instance=await host.launch({profile:dirs.profile,docs:dirs.documents,log:path.join(out,phase+'.log')});
   try{
    if(phase==='missing_retry'){
     const progress=path.join(dirs.profile,'native-qa-progress.json'),end=Date.now()+30000;
     while(!fs.existsSync(progress)&&!instance.launcherStopped&&Date.now()<end)await pause(50);
     const ready=JSON.parse(fs.readFileSync(progress));assert(ready.ok&&ready.phase==='await_owned_file');
     fs.renameSync(progress,path.join(out,'waiting-for-owned-file.json'));fs.writeFileSync(files[5],bytes(5),{flag:'wx'});
    }
    await instance.wait(60000);const resultFile=path.join(dirs.profile,'native-qa-result.json'),result=JSON.parse(fs.readFileSync(resultFile));fs.renameSync(resultFile,path.join(out,phase+'.json'));
    report.phases.push({phase,result,host:instance.receipt});assert.equal(result.ok,true,result.checks.join(','));
    for(let i=phase==='missing_retry'?0:1;i<6;i++)assert.deepEqual(fs.readFileSync(files[i]),bytes(i),'original BOM/CRLF fixture bytes unchanged');
   }finally{await instance.cleanup();}
   if(phase==='missing_retry'){assert.deepEqual(fs.readFileSync(files[0]),bytes(0));assert(fs.lstatSync(files[0]).isFile());fs.unlinkSync(files[0]);}
  }report.passed=true;
 }finally{
  fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx',mode:0o600});
  for(const name of ['tmp','native-home'])fs.rmSync(dirs[name],{recursive:true,force:true});
 }
 console.log(JSON.stringify({nativeSessionRecoveryPassed:true,phases:report.phases.length,checks:report.phases.reduce((n,p)=>n+p.result.checks.length,0)}));
}
if(require.main===module){const[app,out]=process.argv.slice(2);run(path.resolve(app),path.resolve(out)).catch(error=>{console.error(error.message);process.exitCode=1;});}
module.exports={driver,body,bytes};
