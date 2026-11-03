'use strict';
// Dedicated Cocoa/WKWebView only. Original native/HTTP SQL requests are run;
// their real result delivery is gated while a real dirty decision is visible.
// No chooser, global keyboard/mouse, model or daily profile is involved.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {prepare}=require(process.env.READMD_NATIVE_SYSTEM_HOST||'./macos-native-system-host.cjs');
const root=path.resolve(__dirname,'..'),workspace=path.dirname(root),sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const original='# Close 😀\n\nKeep.\n\n```sql cmd output=true\nSELECT 42;\n```\n',manual=original.replace('Keep.','Unsaved.');
const expected=manual+'\n<!-- code_chunk_output -->\n\n42\n--\n42\n\n<!-- /code_chunk_output -->\n';
const encoded=s=>Buffer.from('\ufeff'+s.replaceAll('\n','\r\n'));
function driver(phase,files){return String.raw`(async()=>{
 const phase=${JSON.stringify(phase)},files=${JSON.stringify(files)},original=${JSON.stringify(original)},manual=${JSON.stringify(manual)},expected=${JSON.stringify(expected)},checks=[];
 const check=(ok,label)=>{if(!ok)throw Error(label);checks.push(label);};
 const until=async fn=>{const end=Date.now()+15000;while(Date.now()<end){if(await fn())return;await new Promise(r=>setTimeout(r,20));}throw Error('native_close_condition_timeout');};
 const report=ok=>window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok,phase,checks}}));
 const history=async redo=>{const tab=getActiveTab(),before=ReadMDReview.snapshot(tab).review_revision;check(ReadMDReview.history(redo)===true,'actual_native_history_entry');await until(()=>!ReadMDReview.busy(tab)&&ReadMDReview.snapshot(tab).review_revision>before);check(ReadMDReview.snapshot(tab).review_revision===before+1,'native_single_history_transaction');};
 try{
  await __readmdSessionReady;check(hasPy&&document.body.classList.contains('native-macos'),'actual_cocoa_wkwebview');
  check(state.tabs.length===0,'native_explicit_closes_restore_empty_session');
  if(phase==='decisions'){
   let observed=0,pending=0,last;
   const persist=window.persistCodeChunkOutput;window.persistCodeChunkOutput=async function(...args){pending++;try{last=await persist.apply(this,args);observed++;return last;}finally{pending--;}};
   for(const action of ['tab','edit'])for(const choice of ['discard','cancel','save']){
    const key=action+'_'+choice;await loadFile(files[key]);if(!state.editing)await toggleEdit();setPvLayout('right');await renderPreview();
    const tab=getActiveTab(),at=cmView.state.doc.toString().indexOf('Keep.');cmView.dispatch({changes:{from:at,to:at+5,insert:'Unsaved.'}});check(await ReadMDReview.flush(tab),'native_manual_edit_durable_'+key);
    let release,arrived=false;const gate=new Promise(r=>{release=r;}),before=observed;
    const native=hasPy&&typeof py.run_code_chunk==='function',real=native?py.run_code_chunk:apiFetch;
    const wrapped=async function(...args){const response=await real.apply(this,args);if(native||String(args[0]).endsWith('/api/code/run')){const result=native?response:await response.clone().json();check(result.ok===true,'actual_sql_service_'+key);arrived=true;await gate;}return response;};
    if(native){py.run_code_chunk=wrapped;check(py.run_code_chunk===wrapped,'native_transport_delivery_gate');}else apiFetch=wrapped;
    try{
     document.querySelector('#preview-pane .code-chunk-run-btn').click();await until(()=>arrived);
     const closing=action==='tab'?closeTab(tab.id):confirmExitEdit();await until(()=>!document.getElementById('close-confirm-modal').classList.contains('hidden'));
     release();await until(()=>pending>0||observed>before);
     check(cmView.state.doc.toString()===manual,'native_pending_decision_body_stable_'+key);
     document.getElementById('close-confirm-'+choice).click();await closing;await until(()=>pending===0&&observed===before+1);
     if(choice==='cancel'){
      check(state.tabs.includes(tab)&&state.editing&&last===true,'native_cancel_resumes_original_target_'+key);
      check(cmView.state.doc.toString()===expected,'native_cancel_output_saved_'+key);
      await history(false);check(cmView.state.doc.toString()===manual,'native_one_undo_'+key);await history(true);check(cmView.state.doc.toString()===expected,'native_one_redo_'+key);check(await saveEdit()===true,'native_resave_'+key);await closeTab(tab.id);
     }else{
      check(last===false&&!state.editing,'native_late_write_refused_'+key);check(state.tabs.includes(tab)===(action==='edit'),'native_first_choice_settled_'+key);if(state.tabs.includes(tab))await closeTab(tab.id);
     }
    }finally{release();if(native)py.run_code_chunk=real;else apiFetch=real;}
   }
   window.persistCodeChunkOutput=persist;
  }else{
   await loadFile(files.tab_cancel);if(!state.editing)await toggleEdit();check(cmView.state.doc.toString()===expected&&!hasUnsavedEditorChanges(),'native_saved_output_reopens_clean');
   await history(false);check(cmView.state.doc.toString()===manual,'native_new_process_one_undo');await history(true);check(cmView.state.doc.toString()===expected,'native_new_process_one_redo');check(await saveEdit()===true,'native_new_process_save');await closeTab(getActiveTab().id);
  }
  check(state.tabs.length===0,'native_all_explicit_closes_complete');
  const modules=await(await apiFetch('/api/modules')).json();check(modules.modules.ocr==='idle'&&modules.modules.transcribe==='idle','models_remain_deferred');check(await ReadMDSession.flush()===true,'native_session_persisted');report(true);await py.request_quit();
 }catch(error){checks.push('failed_'+(/^[a-z_0-9]{1,110}$/.test(error.message)?error.message:'native_close_operation'));report(false);window.ipc.postMessage('readmd:quit');}
})();`;}
async function run(app,out,assets){
 assert(out.startsWith(workspace+path.sep));fs.mkdirSync(out,{mode:0o700});
 const dirs=Object.fromEntries(['documents','profile','tmp','native-home'].map(n=>[n,path.join(out,n)]));for(const dir of Object.values(dirs))fs.mkdirSync(dir,{mode:0o700});
 const files={};for(const action of ['tab','edit'])for(const choice of ['discard','cancel','save']){const key=action+'_'+choice;files[key]=path.join(dirs.documents,'关闭 中文😀 '+key+'.md');fs.writeFileSync(files[key],encoded(original));}
 fs.writeFileSync(path.join(dirs.profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1',{flag:'wx'});fs.writeFileSync(path.join(dirs.profile,'settings.json'),JSON.stringify({lang:'zh-CN',closeToTray:false,autoCheckUpdate:false,petEnabled:false}));
 const report={passed:false,nativeHost:'LaunchServices / Cocoa WKWebView',scriptedUiEntries:true,physicalKeysCertified:false,realInPlaceSave:true,productionCsp:true,strictAuth:true,realServiceDeliveryGateOnly:true,modelsRequested:false,dailyApplicationChanged:false,binarySha256:null,bootSha256:sha(fs.readFileSync(path.join(assets,'readmd.boot.js'))),harnessSha256:sha(fs.readFileSync(__filename)),phases:[]};
 try{
  const host=prepare(app,out,{tmp:dirs.tmp,home:dirs['native-home'],assets});report.binarySha256=host.binarySha256;
  for(const phase of ['decisions','reopen']){
   const script=driver(phase,files);new(require('node:vm').Script)(script);fs.writeFileSync(path.join(dirs.profile,'native-qa.js'),script);
   const instance=await host.launch({profile:dirs.profile,docs:dirs.documents,log:path.join(out,phase+'.log')}),entry={phase,host:instance.receipt,passed:false};report.phases.push(entry);
   try{await instance.wait(90000);const file=path.join(dirs.profile,'native-qa-result.json');entry.result=JSON.parse(fs.readFileSync(file));fs.renameSync(file,path.join(out,phase+'.json'));assert.equal(entry.result.ok,true,entry.result.checks.join(','));
    for(const[key,file]of Object.entries(files))assert.deepEqual(fs.readFileSync(file),encoded(key.endsWith('_cancel')?expected:key.endsWith('_save')?manual:original));entry.passed=true;
   }finally{await instance.cleanup();}
  }
  report.passed=true;
 }finally{for(const name of ['tmp','native-home'])fs.rmSync(dirs[name],{recursive:true,force:true});fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx',mode:0o600});}
 console.log(JSON.stringify({passed:report.passed,phases:report.phases.length,checks:report.phases.reduce((n,p)=>n+(p.result?.checks.length||0),0)}));
}
if(require.main===module){const[app,out,assets]=process.argv.slice(2);run(path.resolve(app),path.resolve(out),path.resolve(assets)).catch(()=>{console.error('Native code close acceptance failed; inspect the owned receipt.');process.exitCode=1;});}
module.exports={driver};
