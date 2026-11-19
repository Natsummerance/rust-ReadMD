'use strict';
// The actual Cocoa save selection is confirmed by the operator. All later
// editor/history actions run inside the owned production WKWebView host.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const {prepare}=require('./macos-native-system-host.cjs');
const repo=path.resolve(__dirname,'..'),workspace=path.dirname(repo);
const sha=b=>crypto.createHash('sha256').update(b).digest('hex'),pause=ms=>new Promise(r=>setTimeout(r,ms));
const original='😀\n\n![one](old.svg)\n\n重复正文。\n重复正文。\n\n![two](old.svg)\n';
const images={old:'<svg xmlns="http://www.w3.org/2000/svg"><circle r="4"/></svg>',candidate:'<svg xmlns="http://www.w3.org/2000/svg"><circle r="8"/></svg>',manual:'<svg xmlns="http://www.w3.org/2000/svg"><rect width="8" height="8"/></svg>'};
const encoded=s=>Buffer.from('\ufeff'+s.replace(/\n/g,'\r\n'));
function driver(phase,files){return String.raw`(async()=>{
 const phase=${JSON.stringify(phase)},files=${JSON.stringify(files)},images=${JSON.stringify(images)},checks=[],inputs=[];let dialogId=0;
 // Diagnostics belong only to the synthetic QA editor. Record no key, text,
 // clipboard, filename or input from another app/window.
 document.addEventListener('beforeinput',event=>{
  if(typeof cmView!=='undefined'&&cmView?.dom.contains(event.target)&&inputs.length<40)inputs.push({trusted:event.isTrusted,type:event.inputType||'',length:typeof event.data==='string'?event.data.length:null,ms:Math.round(performance.now())});
 },true);
 const check=(ok,label,record=true)=>{if(!ok)throw Error(label);if(record)checks.push(label);};
 const until=async fn=>{const end=Date.now()+300000;while(Date.now()<end){if(await fn())return;await new Promise(r=>setTimeout(r,50));}throw Error('native_history_condition_timeout');};
 const panel=async choice=>{const id=++dialogId;py.window_control('qa_native_dialog',{choice,path:files.target,id});await until(()=>window.__readmdNativeDialogResult?.id===id);return window.__readmdNativeDialogResult;};
 const report=ok=>{
  if(inputs.length)window.ipc.postMessage(JSON.stringify({readmd_native_qa_progress:{ok:true,phase:'editor_input_provenance',checks:inputs.map(v=>'input_'+(v.trusted?'trusted':'scripted')+'_'+v.type.toLowerCase().replace(/[^a-z_]/g,'')+'_length_'+(v.length??0)+'_ms_'+v.ms)}}));
  window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok,phase,checks}}));
 };
 const verifyImages=async(expected,label)=>{
  const text=cmView.state.doc.toString(),urls=[...text.matchAll(/!\[[^\]]*\]\(([^)]+)\)/g)].map(m=>m[1]);check(urls.length===expected.length,label+'_count',false);
  for(let i=0;i<urls.length;i++){check(urls[i].startsWith('readmd-assets/'),label+'_new_directory_'+i,false);const r=await apiFetch('/raw?p='+encodeURIComponent(files.dest+'/'+decodeURIComponent(urls[i])));check(r.ok&&await r.text()===images[expected[i]],label+'_actual_bytes_'+i,false);}
  check(true,label+'_count_paths_and_bytes');
 };
 const history=async redo=>{const tab=getActiveTab(),revision=ReadMDReview.snapshot(tab).review_revision;check(ReadMDReview.history(redo)===true,redo?'redo_command_accepted':'undo_command_accepted');await until(()=>!ReadMDReview.busy(tab)&&ReadMDReview.snapshot(tab).review_revision>revision);const head=ReadMDReview.snapshot(tab);check(head.review_revision===revision+1,'one_review_event_per_history_command');return head;};
 try{
  await __readmdSessionReady;check(hasPy&&document.body.classList.contains('native-macos'),'actual_cocoa_wkwebview_host');
  if(phase==='save_as'){
   await loadFile(files.source);await toggleEdit();const tab=getActiveTab();check(!hasUnsavedEditorChanges(),'open_and_enter_edit_are_clean');
   const stamp=await ReadMDReview.capture(tab),text=cmView.state.doc.toString(),needle='![one](old.svg)',from=text.indexOf(needle);
   check(from>=0,'first_image_exact_fixture_anchor');await ReadMDReview.propose([{from,to:from+needle.length,insert:'![one](candidate.svg)'}],stamp,tab);
   const at=cmView.state.doc.toString().indexOf('candidate.svg');cmView.dispatch({changes:{from:at,to:at+'candidate.svg'.length,insert:'manual.svg'}});check(await ReadMDReview.flush(tab),'manual_image_edit_persisted');
   let head=ReadMDReview.snapshot(tab);check(head.changes[0].taken_over&&head.changes[0].decision==='conflict','manual_edit_owns_ai_range');
   await ReadMDReview.resolve([head.changes[0].id],'accept',tab,'keep_current');
   cmView.dispatch({changes:{from:0,insert:'后来输入。\n'}});check(await ReadMDReview.flush(tab),'later_text_persisted_separately');
   const snapshot=documentSaveSnapshot(),review=await ReadMDReview.prepareSave(tab,snapshot.content);check(!!review,'exact_visible_save_head_prepared');
   try{
    const saving=py.save_as(snapshot.content,'历史另存.md',snapshot.assets,{result:true,dir:files.dest,base_dir:tab.dir,encoding:snapshot.encoding,review_session:review});
    await until(async()=>{const p=await panel('status');return p.available&&p.visible&&p.directory_matches&&p.name_matches;});
    window.ipc.postMessage(JSON.stringify({readmd_native_qa_progress:{ok:true,phase:'manual_history_save_ready',checks}}));
    const outcome=await saving;check(outcome?.ok===true&&outcome.path===files.target,'actual_powerbox_save_receipt');
    check(await commitDocumentSave(snapshot,outcome.path,outcome,{retarget:true})===true,'visible_document_adopts_real_save_receipt');
   }finally{ReadMDReview.finishSave(tab);}
   head=ReadMDReview.snapshot(tab);check(tab.path===files.target&&!hasUnsavedEditorChanges(),'new_identity_is_clean');
   check(head.changes[0].decision==='accepted'&&head.changes[0].taken_over,'save_preserves_manual_acceptance');
   check(head.changes[0].original==='![one](old.svg)','immutable_ai_original_retained');await verifyImages(['manual','old'],'saved_images');
   const length=cmView.state.doc.length;cmView.dispatch({changes:{from:length,insert:'取消另存专用。\n'}});check(await ReadMDReview.flush(tab),'post_save_edit_persisted');
   const cancelling=saveAs();await until(async()=>{const p=await panel('status');return p.available&&p.visible;});await panel('cancel');
   check(await cancelling===false&&hasUnsavedEditorChanges(),'native_cancel_keeps_unsaved_body');await history(false);
   check(!hasUnsavedEditorChanges()&&ReadMDReview.snapshot(tab).can_redo,'undo_new_input_returns_to_saved_body_with_redo');
  }else if(phase==='reopen_history'){
   if(files.resume&&state.tabs.length===0)await loadFile(files.target);
   check(state.tabs.length===1&&getActiveTab()?.path===files.target,files.resume?'new_process_opens_saved_identity':'new_process_restores_only_saved_identity');if(!state.editing)await toggleEdit();
   const tab=getActiveTab(),saved=cmView.state.doc.toString();check(ReadMDReview.snapshot(tab).can_redo,'redo_branch_survives_process_restart');
   await history(true);check(cmView.state.doc.toString().endsWith('取消另存专用。\n'),'redo_post_save_input');await history(false);check(cmView.state.doc.toString()===saved,'undo_post_save_input_exact');
   const expected=[['manual','accepted',true],['manual','conflict',true],['candidate','pending',false],['old','',false]];
   for(let i=0;i<expected.length;i++){const [image,decision,taken]=expected[i],head=await history(false);await verifyImages([image,'old'],'undo_state_'+i);check(decision?head.changes[0].decision===decision&&head.changes[0].taken_over===taken:head.changes.length===0,'undo_review_state_'+i);}
   for(let i=0;i<expected.length;i++){const [image,decision,taken]=[['candidate','pending',false],['manual','conflict',true],['manual','accepted',true],['manual','accepted',true]][i],head=await history(true);await verifyImages([image,'old'],'redo_state_'+i);check(head.changes[0].decision===decision&&head.changes[0].taken_over===taken,'redo_review_state_'+i);}
   check(cmView.state.doc.toString()===saved,'complete_undo_redo_returns_to_exact_saved_body');
   check(await saveEdit()===true&&!hasUnsavedEditorChanges(),'real_in_place_save_after_history_replay');await closeTab(tab.id);check(state.tabs.length===0&&!state.editing,'clean_saved_page_closes');
  }else check(state.tabs.length===0&&!getActiveTab(),'explicit_close_remains_closed_after_restart');
  const modules=await(await apiFetch('/api/modules')).json();check(modules.modules.ocr==='idle'&&modules.modules.transcribe==='idle','models_remain_deferred');
  check(await ReadMDSession.flush()===true,'session_persistence_acknowledged');report(true);await py.request_quit();
 }catch(error){checks.push('failed_'+(/^[a-z_0-9]{1,110}$/.test(error.message)?error.message:'native_history_operation'));report(false);window.ipc.postMessage('readmd:quit');}
})();`;}
async function run(app,out,mode='fresh'){
 assert(['fresh','resume'].includes(mode));const resume=mode==='resume';assert(out.startsWith(workspace+path.sep));
 if(resume)assert(fs.lstatSync(out).isDirectory()&&!fs.lstatSync(out).isSymbolicLink());else fs.mkdirSync(out,{mode:0o700});
 const runOut=resume?path.join(out,'resume-native'):out;if(resume)fs.mkdirSync(runOut,{mode:0o700});
 const dirs=Object.fromEntries(['documents','profile','tmp','native-home'].map(name=>[name,path.join(resume&&['tmp','native-home'].includes(name)?runOut:out,name)]));
 for(const [name,dir]of Object.entries(dirs))if(!resume||['tmp','native-home'].includes(name))fs.mkdirSync(dir,{mode:0o700});
 const source=path.join(dirs.documents,'原件 中文😀'),dest=path.join(dirs.documents,'另存目标 中文😀'),offline=path.join(dirs.documents,'原件离线');
 const files={source:path.join(source,'原件.md'),dest,target:path.join(dest,'历史另存.md'),resume};
 if(!resume){
  fs.mkdirSync(source);fs.mkdirSync(dest);fs.writeFileSync(files.source,encoded(original),{flag:'wx'});
  for(const [name,body]of Object.entries(images))fs.writeFileSync(path.join(source,name+'.svg'),body,{flag:'wx'});
  fs.writeFileSync(path.join(dirs.profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1',{flag:'wx'});
  fs.writeFileSync(path.join(dirs.profile,'settings.json'),JSON.stringify({lang:'zh-CN',closeToTray:false,autoCheckUpdate:false,petEnabled:false}));
 }else{assert.equal(fs.readFileSync(path.join(dirs.profile,'.readmd-native-qa-owned'),'utf8'),'readmd-synthetic-native-qa-v1');assert.equal(JSON.parse(fs.readFileSync(path.join(out,'save_as.json'))).ok,true);assert(fs.existsSync(offline));}
 const report={nativeHost:'LaunchServices / Cocoa WKWebView / genuine NSSavePanel',entry:'visible editor / durable review / native Save As / real file writer',manualNativeConfirmation:!resume,inheritsManualSave:resume,chooserShim:false,productionCsp:true,strictAuth:true,modelsRequested:false,oldDirectoryOfflineAfterSave:true,bootSha256:sha(fs.readFileSync(path.join(repo,'assets/readmd.boot.js'))),harnessSha256:sha(fs.readFileSync(__filename)),phases:[],passed:false};
 if(resume){const previous=JSON.parse(fs.readFileSync(path.join(out,'report.json')));assert.equal(previous.bootSha256,report.bootSha256);report.manualSaveEvidenceSha256=sha(fs.readFileSync(path.join(out,'save_as.json')));report.previousHarnessFailure='receipt_check_labels_exceeded_native_64_limit';report.previousBinarySha256=previous.binarySha256;}
 try{
  const host=prepare(app,runOut,{tmp:dirs.tmp,home:dirs['native-home']});report.binarySha256=host.binarySha256;if(resume)assert.equal(report.previousBinarySha256,report.binarySha256);
  for(const phase of (resume?['reopen_history','empty']:['save_as','reopen_history','empty'])){
   const script=driver(phase,files);new(require('node:vm').Script)(script);fs.writeFileSync(path.join(dirs.profile,'native-qa.js'),script);
   const instance=await host.launch({profile:dirs.profile,docs:dirs.documents,log:path.join(runOut,phase+'.log')});const entry={phase,host:instance.receipt,passed:false};report.phases.push(entry);
   try{
    await instance.wait(phase==='save_as'?360000:90000);const receipt=path.join(dirs.profile,'native-qa-result.json');entry.result=JSON.parse(fs.readFileSync(receipt));fs.renameSync(receipt,path.join(runOut,phase+'.json'));
    const progress=path.join(dirs.profile,'native-qa-progress.json');if(fs.existsSync(progress)){const p=JSON.parse(fs.readFileSync(progress));if(p.phase==='editor_input_provenance')entry.editorInputDiagnostics=p.checks;}
    assert.equal(entry.result.ok,true,entry.result.checks.join(','));
    const base=phase==='save_as'?source:offline;assert.deepEqual(fs.readFileSync(path.join(base,'原件.md')),encoded(original));
    for(const [name,body]of Object.entries(images)){assert.equal(fs.readFileSync(path.join(base,name+'.svg'),'utf8'),body);assert.equal(fs.readFileSync(path.join(dest,'readmd-assets',name+'-'+sha(Buffer.from(body)).slice(0,24)+'.svg'),'utf8'),body);}
    const current=fs.readFileSync(files.target);assert(current.subarray(0,3).equals(Buffer.from([239,187,191])));
    const comparable=current.toString('utf8').replace(/\]\((readmd-assets\/[^)]*)\)/g,(_,ref)=>']('+decodeURIComponent(ref)+')');
    const expected='后来输入。\n'+original.replace('![one](old.svg)','![one](manual.svg)').replaceAll('manual.svg','readmd-assets/manual-'+sha(Buffer.from(images.manual)).slice(0,24)+'.svg').replaceAll('old.svg','readmd-assets/old-'+sha(Buffer.from(images.old)).slice(0,24)+'.svg');
    assert.deepEqual(Buffer.from(comparable),encoded(expected));entry.passed=true;
    if(phase==='save_as'){
     const ack=path.join(out,'operator-replied.ack'),deadline=Date.now()+300000;while(!fs.existsSync(ack)){assert(Date.now()<deadline,'operator handoff reply timeout');await pause(100);}
     const st=fs.lstatSync(ack);assert(st.isFile()&&!st.isSymbolicLink()&&st.nlink===1);assert.equal(fs.readFileSync(ack,'utf8'),'operator-replied');fs.renameSync(source,offline);
    }
   }finally{await instance.cleanup();}
  }
  report.passed=true;
 }finally{
  for(const name of ['tmp','native-home'])fs.rmSync(dirs[name],{recursive:true,force:true});
  fs.writeFileSync(path.join(runOut,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx',mode:0o600});
 }
 console.log(JSON.stringify({nativeHistoryResourcesPassed:report.passed,phases:report.phases.length,checks:report.phases.reduce((n,p)=>n+(p.result?.checks.length||0),0)}));
}
if(require.main===module){const [app,out,mode]=process.argv.slice(2);run(path.resolve(app),path.resolve(out),mode).catch(error=>{console.error(error.message);process.exitCode=1;});}
module.exports={driver};
