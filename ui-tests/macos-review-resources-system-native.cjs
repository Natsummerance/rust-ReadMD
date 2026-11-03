'use strict';
// Genuine NSSavePanel confirmation is manual. The bridge's supported initial
// directory is supplied before presentation; no chooser result is replaced.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {prepare}=require('./macos-native-system-host.cjs');
const root=path.resolve(__dirname,'..'),workspace=path.dirname(root),sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const original='# 原生待审资源\n\n![原图](image.svg)\n\n重复正文。\n\n重复正文。\n';
const image='<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><circle cx="10" cy="10" r="8" fill="blue"/></svg>';
const encode=s=>Buffer.from('\ufeff'+s.replace(/\n/g,'\r\n'));
function driver(phase,files){return String.raw`(async()=>{
 const phase=${JSON.stringify(phase)},files=${JSON.stringify(files)},checks=[];let id=0;
 const check=(ok,name)=>{if(!ok)throw Error(name);checks.push(name);};
 const until=async fn=>{const end=Date.now()+300000;while(Date.now()<end){if(await fn())return;await new Promise(r=>setTimeout(r,50));}throw Error('native_resource_condition_timeout');};
 const panel=async choice=>{const mine=++id;py.window_control('qa_native_dialog',{choice,path:files.target,id:mine});await until(()=>window.__readmdNativeDialogResult?.id===mine);return window.__readmdNativeDialogResult;};
 const report=(ok,label=phase)=>window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok,phase:label,checks}}));
 try{
  await __readmdSessionReady;check(hasPy&&document.body.classList.contains('native-macos'),'actual_native_host');
  if(phase==='save_as'){
   await loadFile(files.source);await toggleEdit();check(!hasUnsavedEditorChanges(),'open_and_edit_are_clean');
   const tab=getActiveTab(),mark=await ReadMDReview.capture(tab),text=cmView.state.doc.toString(),needle='![原图](image.svg)',imageFrom=text.indexOf(needle);
   check(imageFrom>=0,'fixed_image_anchor_present');
   await ReadMDReview.propose([{from:imageFrom,to:imageFrom+needle.length,insert:'等待用户审阅的正文。'}],mark,tab);
   check(ReadMDReview.snapshot(tab).changes[0].decision==='pending','actual_rust_proposal_pending');
   const snapshot=documentSaveSnapshot();const review=await ReadMDReview.prepareSave(tab,snapshot.content);check(!!review,'durable_save_head_prepared');
   try{
    const saving=py.save_as(snapshot.content,'资源另存.md',snapshot.assets,{result:true,dir:files.dest,base_dir:tab.dir,encoding:snapshot.encoding,review_session:review});
    await until(async()=>{const p=await panel('status');return p.available&&p.visible&&p.directory_matches&&p.name_matches;});
    window.ipc.postMessage(JSON.stringify({readmd_native_qa_progress:{ok:true,phase:'manual_resource_save_ready',checks}}));
    const outcome=await saving;check(outcome?.ok===true&&outcome.path===files.target,'actual_powerbox_selection_and_save_receipt');
    check(await commitDocumentSave(snapshot,outcome.path,outcome,{retarget:true})===true,'real_save_receipt_adopted_by_visible_document');
   }finally{ReadMDReview.finishSave(tab);}
   check(getActiveTab()===tab&&tab.path===files.target&&!hasUnsavedEditorChanges(),'saved_target_identity_is_clean');
   check(ReadMDReview.snapshot(tab).changes[0].decision==='pending','save_does_not_accept_review');
   const snapshot2=ReadMDReview.snapshot(tab),reference=snapshot2.changes[0].restore_original.match(/\]\(([^)]+)\)/)?.[1];
   check(reference?.startsWith('readmd-assets/'),'hidden_original_has_projected_resource');
   const actual=await apiFetch('/raw?p='+encodeURIComponent(files.dest+'/'+decodeURIComponent(reference)));
   check(actual.ok&&await actual.text()===${JSON.stringify(image)},'hidden_resource_actual_bytes_match');
   const from=cmView.state.doc.length;cmView.dispatch({changes:{from,insert:'\n仅用于取消的未保存正文。\n'}});const dirty=getEditContent(),cancelled=saveAs();
   await until(async()=>{const p=await panel('status');return p.available&&p.visible;});await panel('cancel');
   check(await cancelled===false&&getEditContent()===dirty&&hasUnsavedEditorChanges(),'real_cancel_preserves_visible_draft');
   cmView.dispatch({changes:{from,to:cmView.state.doc.length,insert:''}});await ReadMDReview.flush(tab);
   check(!hasUnsavedEditorChanges(),'explicit_revert_returns_to_saved_body');
  }else if(phase==='remove_unplanned_input'){
   await loadFile(files.target);if(!state.editing)await toggleEdit();const tab=getActiveTab();
   const decoded=s=>s.replace(/\]\((readmd-assets\/[^)]*)\)/g,(_,ref)=>']('+decodeURIComponent(ref)+')');
   check(decoded(cmView.state.doc.toString())==='u'+files.expectedFinal,'only_known_unplanned_editor_input_present');
   check(ReadMDReview.history(false)===true,'actual_undo_command_accepted');
   await until(()=>!ReadMDReview.busy(tab)&&decoded(cmView.state.doc.toString())===files.expectedFinal);
   check(true,'durable_undo_removes_only_the_unplanned_input');
   check(await saveEdit()===true&&!hasUnsavedEditorChanges(),'corrected_body_saved_with_real_writer');
   await closeTab(tab.id);check(state.tabs.length===0&&!state.editing,'corrected_clean_page_closes');
  }else if(phase==='reopen_withdraw'||phase==='recover_withdrawn_save'){
   check(state.tabs.length===1&&getActiveTab()?.path===files.target,'saved_identity_restored_by_new_process');
   if(!state.editing)await toggleEdit();const tab=getActiveTab(),head=ReadMDReview.snapshot(tab);
   if(phase==='reopen_withdraw'){
    check(head.changes[0].decision==='pending','review_survives_actual_restart');await ReadMDReview.resolve([head.changes[0].id],'withdraw',tab);
   }else check(head.changes[0].decision==='withdrawn'&&hasUnsavedEditorChanges(),'withdrawn_unsaved_body_survives_controlled_test_exit');
   check(cmView.state.doc.toString().includes('![原图](readmd-assets/'),'withdrawal_restores_complete_image');
   check(await saveEdit()===true&&!hasUnsavedEditorChanges(),'withdrawn_body_saved_in_place');
   await closeTab(tab.id);check(state.tabs.length===0&&!state.editing,'clean_saved_page_closes');
  }else check(state.tabs.length===0&&!getActiveTab(),'explicit_close_stays_closed_after_restart');
  const modules=await(await apiFetch('/api/modules')).json();check(modules.modules.ocr==='idle'&&modules.modules.transcribe==='idle','models_remain_deferred');
  check(await ReadMDSession.flush()===true,'session_acknowledged');report(true);await py.request_quit();
 }catch(error){checks.push('failed_'+(/^[a-z_]{1,100}$/.test(error.message)?error.message:'native_resource_operation'));report(false);window.ipc.postMessage('readmd:quit');}
})();`;}
async function run(app,out,mode='fresh'){
 const resuming=mode!=='fresh';assert(['fresh','resume','audit-empty','reconcile-input'].includes(mode));assert(out.startsWith(workspace+path.sep));
 if(!resuming)fs.mkdirSync(out,{mode:0o700});else assert(!fs.lstatSync(out).isSymbolicLink());
 const runOut=resuming?path.join(out,mode==='reconcile-input'?'reconciled-native':mode==='audit-empty'?'final-empty-native':'resume-native'):out;if(resuming)fs.mkdirSync(runOut,{mode:0o700});
 const dirs=Object.fromEntries(['documents','profile','tmp','native-home'].map(n=>[n,path.join(out,n)]));
 if(resuming)for(const n of ['tmp','native-home'])dirs[n]=path.join(runOut,n);
 for(const [n,d]of Object.entries(dirs))if(!resuming||['tmp','native-home'].includes(n))fs.mkdirSync(d,{mode:0o700});
 const sourceDir=path.join(dirs.documents,'原件 中文😀'),dest=path.join(dirs.documents,'另存目标 中文😀');
 const files={source:path.join(sourceDir,'原件.md'),dest,target:path.join(dest,'资源另存.md')};
 files.expectedFinal=original.replace('image.svg','readmd-assets/image-'+sha(Buffer.from(image)).slice(0,24)+'.svg');
 if(!resuming){fs.mkdirSync(sourceDir);fs.mkdirSync(dest);fs.writeFileSync(files.source,encode(original),{flag:'wx'});fs.writeFileSync(path.join(sourceDir,'image.svg'),image,{flag:'wx'});
 fs.writeFileSync(path.join(dirs.profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1',{flag:'wx'});fs.writeFileSync(path.join(dirs.profile,'settings.json'),JSON.stringify({lang:'zh-CN',closeToTray:false,autoCheckUpdate:false,petEnabled:false}));}
 else {assert.equal(fs.readFileSync(path.join(dirs.profile,'.readmd-native-qa-owned'),'utf8'),'readmd-synthetic-native-qa-v1');assert.equal(JSON.parse(fs.readFileSync(path.join(out,'save_as.json'))).ok,true);}
 const report={nativeHost:'LaunchServices / Cocoa WKWebView / real NSSavePanel',entry:'production native bridge and document save adoption',manualNativeConfirmation:!resuming,inheritsManualSave:resuming,chooserShim:false,productionCsp:true,strictAuth:true,modelsRequested:false,bootSha256:sha(fs.readFileSync(path.join(root,'assets/readmd.boot.js'))),harnessSha256:sha(fs.readFileSync(__filename)),phases:[],passed:false};
 if(resuming){const previous=JSON.parse(fs.readFileSync(path.join(out,'report.json')));assert.equal(previous.bootSha256,report.bootSha256);report.manualSaveEvidenceSha256=sha(fs.readFileSync(path.join(out,'save_as.json')));report.controlledPreviousExit=true;report.previousHarnessFailure='undefined_saveFile_call_corrected_to_existing_saveEdit';}
 if(['audit-empty','reconcile-input'].includes(mode)){
  const evidence=path.join(out,'resume-native/recover_withdrawn_save.json');assert.equal(JSON.parse(fs.readFileSync(evidence)).ok,true);
  report.withdrawnSaveEvidenceSha256=sha(fs.readFileSync(evidence));report.previousOracleCorrection='Compare the fixed resource destination after URI decoding; retain exact BOM, CRLF and all other text bytes.';
 }
 if(mode==='reconcile-input')report.unplannedEditorInput={observed:true,sourceProven:false,onlyOwnedFixture:true,repairUsesActualUndo:true};
 try{const host=prepare(app,runOut,{tmp:dirs.tmp,home:dirs['native-home']});report.binarySha256=host.binarySha256;
  for(const phase of (mode==='reconcile-input'?['remove_unplanned_input','empty']:mode==='audit-empty'?['empty']:resuming?['recover_withdrawn_save','empty']:['save_as','reopen_withdraw','empty'])){
   const script=driver(phase,files);new(require('node:vm').Script)(script);fs.writeFileSync(path.join(dirs.profile,'native-qa.js'),script);const instance=await host.launch({profile:dirs.profile,docs:dirs.documents,log:path.join(runOut,phase+'.log')});
   const entry={phase,host:instance.receipt,passed:false};report.phases.push(entry);
   try{await instance.wait(phase==='save_as'?360000:60000);const f=path.join(dirs.profile,'native-qa-result.json');entry.result=JSON.parse(fs.readFileSync(f));fs.renameSync(f,path.join(runOut,phase+'.json'));assert.equal(entry.result.ok,true,entry.result.checks.join(','));
    assert.deepEqual(fs.readFileSync(files.source),encode(original));assert.equal(fs.readFileSync(path.join(sourceDir,'image.svg'),'utf8'),image);
    const resource='readmd-assets/image-'+sha(Buffer.from(image)).slice(0,24)+'.svg';assert.equal(fs.readFileSync(path.join(dest,resource),'utf8'),image);
    const expected=phase==='save_as'?original.replace('![原图](image.svg)','等待用户审阅的正文。'):original.replace('image.svg',resource);
    const actual=fs.readFileSync(files.target);
    // URI spelling may over-escape an unreserved filename character. Check
    // the decoded destination independently and keep every other byte exact.
    const comparable=phase==='save_as'?actual:Buffer.from(actual.toString('utf8').replace(/\]\((readmd-assets\/[^)]*)\)/g,(_,ref)=>']('+decodeURIComponent(ref)+')'));
    assert.deepEqual(comparable,encode(expected));entry.passed=true;
    if(phase==='save_as'){
     // Save selection is already verified above. This marker gates only a
     // subsequent focus-changing process launch, never the chooser outcome.
     const marker=path.join(out,'operator-replied.ack'),end=Date.now()+300000;
     while(!fs.existsSync(marker)){assert(Date.now()<end,'operator reply handoff timeout');await new Promise(r=>setTimeout(r,100));}
     const s=fs.lstatSync(marker);assert(s.isFile()&&!s.isSymbolicLink()&&s.nlink===1);assert.equal(fs.readFileSync(marker,'utf8'),'operator-replied');
    }
   }finally{await instance.cleanup();}
  }report.passed=true;
 }finally{for(const n of ['tmp','native-home'])fs.rmSync(dirs[n],{recursive:true,force:true});fs.writeFileSync(path.join(runOut,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx',mode:0o600});}
 console.log(JSON.stringify({nativeReviewResourcesPassed:report.passed,phases:report.phases.length,checks:report.phases.reduce((n,p)=>n+(p.result?.checks.length||0),0)}));
}
if(require.main===module){const [app,out,mode]=process.argv.slice(2);run(path.resolve(app),path.resolve(out),mode).catch(e=>{console.error(e.message);process.exitCode=1;});}
module.exports={driver};
