'use strict';
// One frozen Cocoa candidate covers existing single-block flows, then a whole
// batch, persistent history, cancellation and explicit close/discard. No mouse,
// global keys, chooser substitution, model or daily application is involved.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), assert = require('node:assert/strict');
const { prepare } = require('./macos-native-system-host.cjs'), { driver: single } = require('./macos-code-output-system-native.cjs');
const repo = path.resolve(__dirname, '..'), workspace = path.dirname(repo), sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const original = '# Code 😀\n\n```sql cmd output=true\nSELECT 42;\n```\n\nSame.\n\n```sql cmd output=true\nSELECT 42;\n```\n\nTail.\n';
const proposed = original.replace('Same.', 'Pending.');
const singleExpected = '# Code 😀\n\n```sql cmd output=true\nSELECT 42;\n```\n\nPending.\n\n```sql cmd output=true\nSELECT 42;\n```\n\n<!-- code_chunk_output -->\n\n42\n--\n42\n\n<!-- /code_chunk_output -->\n\nTail.\n';
const each = '```sql cmd output=true\nSELECT 42;\n```\n\n<!-- code_chunk_output -->\n\n42\n--\n42\n\n<!-- /code_chunk_output -->\n';
const batchExpected = '# Code 😀\n\n' + each + '\nPending.\n\n' + each + '\nTail.\n';
const slow = '```sql cmd output=true\nWITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<1000000000) SELECT sum(x) FROM n;\n```\n';
const encoded = text => Buffer.from('\ufeff' + text.replaceAll('\n', '\r\n'));
function driver(phase, files) {
 if (!phase.startsWith('batch_')) return single(phase, files);
 return String.raw`(async()=>{
  const phase=${JSON.stringify(phase)},files=${JSON.stringify(files)},original=${JSON.stringify(original)},proposed=${JSON.stringify(proposed)},expected=${JSON.stringify(batchExpected)},checks=[];
  const check=(ok,label)=>{if(!ok)throw Error(label);checks.push(label);};
  const until=async fn=>{const end=Date.now()+15000;while(Date.now()<end){if(await fn())return;await new Promise(r=>setTimeout(r,25));}throw Error('native_batch_condition_timeout');};
  const report=ok=>window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok,phase,checks}}));
  const history=async redo=>{const tab=getActiveTab(),before=ReadMDReview.snapshot(tab).review_revision;check(ReadMDReview.history(redo)===true,redo?'native_batch_redo_entry':'native_batch_undo_entry');await until(()=>!ReadMDReview.busy(tab)&&ReadMDReview.snapshot(tab).review_revision>before);check(ReadMDReview.snapshot(tab).review_revision===before+1,'native_batch_one_history_transaction');};
  try{
   await __readmdSessionReady;check(hasPy&&document.body.classList.contains('native-macos'),'actual_cocoa_wkwebview');
   if(phase==='batch_run_save_quit'){
    check(state.tabs.length===0,'previous_explicit_closes_survive_before_batch');
    await loadFile(files.batch);if(!state.editing)await toggleEdit();const tab=getActiveTab();check(!hasUnsavedEditorChanges(),'batch_open_edit_is_clean');
    const stamp=await ReadMDReview.capture(tab),at=original.indexOf('Same.');
    await ReadMDReview.propose([{from:at,to:at+5,insert:'Pending.'}],stamp,tab);setPvLayout('right');await renderPreview();
    const before=ReadMDReview.snapshot(tab).generation;
    document.getElementById('btn-run-all-chunks').click();await until(()=>ReadMDCodeRuns.busy()===0);
    check(cmView.state.doc.toString()===expected,'native_menu_runs_both_duplicate_blocks');
    check(ReadMDReview.snapshot(tab).generation===before+1,'whole_batch_has_one_generation');
    check(!hasUnsavedEditorChanges(),'native_batch_save_acknowledged');
    check(ReadMDReview.snapshot(tab).changes[0].original==='Same.'&&ReadMDReview.snapshot(tab).changes[0].decision==='pending','native_batch_preserves_pending_original');
    await history(false);check(cmView.state.doc.toString()===proposed,'native_one_undo_removes_all_batch_outputs');
    await history(true);check(cmView.state.doc.toString()===expected,'native_one_redo_restores_all_batch_outputs');check(await saveEdit()===true,'native_batch_resave');
    await loadFile(files.boundary);if(!state.editing)await toggleEdit();setPvLayout('right');await renderPreview();
    const boundary=getActiveTab(),body=cmView.state.doc.toString(),from=body.indexOf('Same.'),manual=body.replace('Same.','Manual.');
    cmView.dispatch({changes:{from,to:from+5,insert:'Manual.'}});check(await ReadMDReview.flush(boundary),'native_boundary_manual_edit_durable');
    const executing=runAllCodeChunks(),closing=closeTab(boundary.id);
    await until(()=>!document.getElementById('close-confirm-modal').classList.contains('hidden'));
    await until(()=>ReadMDCodeRuns.batchFor(boundary)?.phase==='waiting-to-write');
    check(cmView.state.doc.toString()===manual,'completed_native_batch_cannot_change_open_close_decision');
    document.getElementById('close-confirm-discard').click();await closing;check(await executing===false,'discard_cancels_completed_unapplied_batch');
    check(!state.tabs.includes(boundary)&&state.tabs.length===1,'first_discard_closes_pending_batch_page');
    await loadFile(files.slow);if(!state.editing)await toggleEdit();setPvLayout('right');await renderPreview();
    const stopping=getActiveTab();document.getElementById('btn-run-all-chunks').click();await until(()=>ReadMDCodeRuns.batchFor(stopping)?.phase==='running');
    await closeTab(stopping.id);await until(()=>ReadMDCodeRuns.busy()===0);
    check(!state.tabs.includes(stopping)&&state.tabs.length===1,'native_batch_close_has_terminal_receipt');
   }else{
    check(state.tabs.length===1&&getActiveTab().path===files.batch,'native_new_process_restores_only_batch_page');
    check(ReadMDCodeRuns.busy()===0,'native_restart_does_not_resume_batch');if(!state.editing)await toggleEdit();
    check(cmView.state.doc.toString()===expected&&!hasUnsavedEditorChanges(),'native_saved_batch_reopens_clean');
    await history(false);check(cmView.state.doc.toString()===proposed,'native_new_process_one_batch_undo');
    await history(true);check(cmView.state.doc.toString()===expected,'native_new_process_one_batch_redo');check(await saveEdit()===true,'native_new_process_batch_save');
    const tab=getActiveTab();cmView.dispatch({changes:{from:cmView.state.doc.length,insert:'Discard batch.\n'}});check(await ReadMDReview.flush(tab),'batch_manual_edit_durable');
    const cancel=closeTab(tab.id);await until(()=>!document.getElementById('close-confirm-modal').classList.contains('hidden'));document.getElementById('close-confirm-cancel').click();await cancel;
    check(state.tabs.includes(tab)&&hasUnsavedEditorChanges(),'native_batch_close_cancel_keeps_input');
    const discard=closeTab(tab.id);await until(()=>!document.getElementById('close-confirm-modal').classList.contains('hidden'));document.getElementById('close-confirm-discard').click();await discard;
    check(state.tabs.length===0&&!state.editing,'native_batch_discard_closes_without_stuck_editor');
   }
   const modules=await(await apiFetch('/api/modules')).json();check(modules.modules.ocr==='idle'&&modules.modules.transcribe==='idle','models_stay_deferred');
   check(await ReadMDSession.flush()===true,'native_batch_session_acknowledged');report(true);await py.request_quit();
  }catch(error){checks.push('failed_'+(/^[a-z_0-9]{1,110}$/.test(error.message)?error.message:'native_batch_operation'));report(false);window.ipc.postMessage('readmd:quit');}
 })();`;
}
async function run(app, out, assets) {
 assert(out.startsWith(workspace + path.sep)); fs.mkdirSync(out,{mode:0o700});
 const dirs=Object.fromEntries(['documents','profile','tmp','native-home'].map(name=>[name,path.join(out,name)]));for(const dir of Object.values(dirs))fs.mkdirSync(dir,{mode:0o700});
 const files={source:path.join(dirs.documents,'重复代码 中文😀.md'),slow:path.join(dirs.documents,'退出停止.md'),batch:path.join(dirs.documents,'批量历史 中文😀.md'),boundary:path.join(dirs.documents,'关闭决定 中文😀.md')};
 fs.writeFileSync(files.source,encoded(original));fs.writeFileSync(files.slow,encoded(slow));fs.writeFileSync(files.batch,encoded(original));fs.writeFileSync(files.boundary,encoded(original));
 fs.writeFileSync(path.join(dirs.profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1',{flag:'wx'});
 fs.writeFileSync(path.join(dirs.profile,'settings.json'),JSON.stringify({lang:'zh-CN',closeToTray:false,autoCheckUpdate:false,petEnabled:false}));
 const report={passed:false,nativeHost:'LaunchServices / Cocoa WKWebView',scriptedUiEntries:true,physicalKeysCertified:false,realInPlaceSave:true,productionCsp:true,strictAuth:true,modelsRequested:false,dailyApplicationChanged:false,
  binarySha256:null,bootSha256:sha(fs.readFileSync(path.join(assets,'readmd.boot.js'))),harnessSha256:sha(fs.readFileSync(__filename)),singleDriverSha256:sha(fs.readFileSync(path.join(__dirname,'macos-code-output-system-native.cjs'))),phases:[]};
 try{
  const host=prepare(app,out,{tmp:dirs.tmp,home:dirs['native-home'],assets});report.binarySha256=host.binarySha256;
  let batchSaved=false;
  for(const phase of ['run_save_quit','reopen_undo_close','batch_run_save_quit','batch_reopen_undo_close','empty']){
   const script=driver(phase,files);new(require('node:vm').Script)(script);fs.writeFileSync(path.join(dirs.profile,'native-qa.js'),script);
   const instance=await host.launch({profile:dirs.profile,docs:dirs.documents,log:path.join(out,phase+'.log')});const entry={phase,host:instance.receipt,passed:false};report.phases.push(entry);
   try{
    await instance.wait(60000);const receipt=path.join(dirs.profile,'native-qa-result.json');entry.result=JSON.parse(fs.readFileSync(receipt));fs.renameSync(receipt,path.join(out,phase+'.json'));
    assert.equal(entry.result.ok,true,entry.result.checks.join(','));if(phase==='batch_run_save_quit')batchSaved=true;
    assert.deepEqual(fs.readFileSync(files.source),encoded(singleExpected));assert.deepEqual(fs.readFileSync(files.slow),encoded(slow));assert.deepEqual(fs.readFileSync(files.batch),encoded(batchSaved?batchExpected:original));assert.deepEqual(fs.readFileSync(files.boundary),encoded(original));entry.passed=true;
   }finally{await instance.cleanup();}
  }
  report.passed=true;
 }finally{
  for(const name of ['tmp','native-home'])fs.rmSync(dirs[name],{recursive:true,force:true});
  fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx',mode:0o600});
 }
 console.log(JSON.stringify({passed:report.passed,phases:report.phases.length,checks:report.phases.reduce((n,p)=>n+(p.result?.checks.length||0),0)}));
}
if(require.main===module){const[app,out,assets]=process.argv.slice(2);run(path.resolve(app),path.resolve(out),path.resolve(assets)).catch(()=>{console.error('Native code batch acceptance failed; inspect its owned receipt.');process.exitCode=1;});}
module.exports={driver};
