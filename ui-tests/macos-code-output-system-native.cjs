'use strict';
// Scripted entry actions inside a dedicated production Cocoa/WKWebView host.
// In-place save is real. No chooser shim, model, user profile or other app.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), assert = require('node:assert/strict');
const { prepare } = require('./macos-native-system-host.cjs');
const repo = path.resolve(__dirname, '..'), workspace = path.dirname(repo);
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const original = '# Code 😀\n\n```sql cmd output=true\nSELECT 42;\n```\n\nSame.\n\n```sql cmd output=true\nSELECT 42;\n```\n\nTail.\n';
const proposed = original.replace('Same.', 'Pending.');
const expected = '# Code 😀\n\n```sql cmd output=true\nSELECT 42;\n```\n\nPending.\n\n```sql cmd output=true\nSELECT 42;\n```\n\n<!-- code_chunk_output -->\n\n42\n--\n42\n\n<!-- /code_chunk_output -->\n\nTail.\n';
const slow = '```sql cmd output=true\nWITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<1000000000) SELECT sum(x) FROM n;\n```\n';
const encoded = text => Buffer.from('\ufeff' + text.replaceAll('\n', '\r\n'));
function driver(phase, files) { return String.raw`(async()=>{
 const phase=${JSON.stringify(phase)},files=${JSON.stringify(files)},checks=[];
 const original=${JSON.stringify(original)},proposed=${JSON.stringify(proposed)},expected=${JSON.stringify(expected)};
 const check=(ok,label)=>{if(!ok)throw Error(label);checks.push(label);};
 const until=async fn=>{const end=Date.now()+15000;while(Date.now()<end){if(await fn())return;await new Promise(r=>setTimeout(r,25));}throw Error('native_code_condition_timeout');};
 const report=ok=>window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok,phase,checks}}));
 const history=async redo=>{const tab=getActiveTab(),before=ReadMDReview.snapshot(tab).review_revision;check(ReadMDReview.history(redo)===true,redo?'redo_entry_accepted':'undo_entry_accepted');await until(()=>!ReadMDReview.busy(tab)&&ReadMDReview.snapshot(tab).review_revision>before);check(ReadMDReview.snapshot(tab).review_revision===before+1,'one_transaction_per_history_command');};
 try{
  await __readmdSessionReady;check(hasPy&&document.body.classList.contains('native-macos'),'actual_cocoa_wkwebview');
  if(phase==='run_save_quit'){
   await loadFile(files.source);if(!state.editing)await toggleEdit();check(!hasUnsavedEditorChanges(),'opening_and_editing_remain_clean');
   const tab=getActiveTab(),stamp=await ReadMDReview.capture(tab),from=original.indexOf('Same.');
   await ReadMDReview.propose([{from,to:from+5,insert:'Pending.'}],stamp,tab);setPvLayout('right');await renderPreview();
   const cards=()=>document.querySelectorAll('#preview-pane .code-chunk-card');await until(()=>cards().length===2);
   cards()[1].querySelector('.code-chunk-run-btn').click();await until(()=>ReadMDCodeRuns.busy()===0);
   check(cmView.state.doc.toString()===expected,'actual_second_duplicate_output_only');
   check(!hasUnsavedEditorChanges(),'actual_native_save_acknowledged');
   let head=ReadMDReview.snapshot(tab);check(head.changes[0].original==='Same.'&&head.changes[0].decision==='pending','code_save_preserves_pending_original');
   await history(false);check(cmView.state.doc.toString()===proposed,'single_undo_removes_only_output');
   await history(true);check(cmView.state.doc.toString()===expected,'single_redo_restores_exact_output');
   check(await saveEdit()===true,'native_resave_after_undo_redo');
   await loadFile(files.slow);if(!state.editing)await toggleEdit();setPvLayout('right');await renderPreview();
   const card=document.querySelector('#preview-pane .code-chunk-card');card.querySelector('.code-chunk-run-btn').click();
   await until(()=>ReadMDCodeRuns.busy()===1&&card.querySelector('.code-chunk-status').classList.contains('running'));
   check(true,'native_ui_code_job_pending_before_true_quit');
  }else if(phase==='reopen_undo_close'){
   check(state.tabs.length===2,'new_process_restores_both_page_identities');
   check(ReadMDCodeRuns.busy()===0,'new_process_does_not_resume_code');
   const tab=state.tabs.find(t=>t.path===files.source);check(!!tab,'saved_code_identity_restored');await switchTab(tab.id);if(!state.editing)await toggleEdit();
   check(cmView.state.doc.toString()===expected&&!hasUnsavedEditorChanges(),'saved_output_reopens_clean');
   await history(false);check(cmView.state.doc.toString()===proposed,'new_process_single_undo');
   await history(true);check(cmView.state.doc.toString()===expected,'new_process_single_redo');
   check(await saveEdit()===true,'new_process_native_save');
   cmView.dispatch({changes:{from:cmView.state.doc.length,insert:'Discard test.\n'}});check(await ReadMDReview.flush(tab),'manual_input_durable_before_close');
   const cancelling=closeTab(tab.id);await until(()=>!document.getElementById('close-confirm-modal').classList.contains('hidden'));
   document.getElementById('close-confirm-cancel').click();await cancelling;check(state.tabs.includes(tab)&&hasUnsavedEditorChanges(),'cancel_close_keeps_page_and_input');
   const closing=closeTab(tab.id);await until(()=>!document.getElementById('close-confirm-modal').classList.contains('hidden'));
   document.getElementById('close-confirm-discard').click();await closing;check(!state.tabs.includes(tab),'discard_close_has_terminal_receipt');
   await closeTab(getActiveTab().id);check(state.tabs.length===0&&!state.editing,'remaining_clean_page_closes');
  }else{check(state.tabs.length===0&&!getActiveTab(),'explicit_closes_survive_next_process');check(ReadMDCodeRuns.busy()===0,'no_late_or_resumed_code_job');}
  const modules=await(await apiFetch('/api/modules')).json();check(modules.modules.ocr==='idle'&&modules.modules.transcribe==='idle','models_stay_deferred');
  check(await ReadMDSession.flush()===true,'session_persistence_acknowledged');report(true);await py.request_quit();
 }catch(error){checks.push('failed_'+(/^[a-z_0-9]{1,110}$/.test(error.message)?error.message:'native_code_operation'));report(false);window.ipc.postMessage('readmd:quit');}
})();`; }
async function run(app, out) {
  assert(out.startsWith(workspace + path.sep)); fs.mkdirSync(out, { mode: 0o700 });
  const dirs = Object.fromEntries(['documents', 'profile', 'tmp', 'native-home'].map(name => [name, path.join(out, name)]));
  for (const dir of Object.values(dirs)) fs.mkdirSync(dir, { mode: 0o700 });
  const files = { source: path.join(dirs.documents, '重复代码 中文😀.md'), slow: path.join(dirs.documents, '退出停止.md') };
  fs.writeFileSync(files.source, encoded(original)); fs.writeFileSync(files.slow, encoded(slow));
  fs.writeFileSync(path.join(dirs.profile, '.readmd-native-qa-owned'), 'readmd-synthetic-native-qa-v1', { flag: 'wx' });
  fs.writeFileSync(path.join(dirs.profile, 'settings.json'), JSON.stringify({ lang: 'zh-CN', closeToTray: false, autoCheckUpdate: false, petEnabled: false }));
  const report = { passed: false, nativeHost: 'LaunchServices / Cocoa WKWebView', scriptedUiEntries: true, realInPlaceSave: true, strictAuth: true, productionCsp: true,
    modelsRequested: false, dailyApplicationChanged: false, binarySha256: null, bootSha256: sha(fs.readFileSync(path.join(repo, 'assets/readmd.boot.js'))), harnessSha256: sha(fs.readFileSync(__filename)), phases: [] };
  try {
    const host = prepare(app, out, { tmp: dirs.tmp, home: dirs['native-home'] }); report.binarySha256 = host.binarySha256;
    for (const phase of ['run_save_quit', 'reopen_undo_close', 'empty']) {
      const script = driver(phase, files); new (require('node:vm').Script)(script); fs.writeFileSync(path.join(dirs.profile, 'native-qa.js'), script);
      const instance = await host.launch({ profile: dirs.profile, docs: dirs.documents, log: path.join(out, phase + '.log') });
      const entry = { phase, host: instance.receipt, passed: false }; report.phases.push(entry);
      try {
        await instance.wait(60000); const receipt = path.join(dirs.profile, 'native-qa-result.json'); entry.result = JSON.parse(fs.readFileSync(receipt)); fs.renameSync(receipt, path.join(out, phase + '.json'));
        assert.equal(entry.result.ok, true, entry.result.checks.join(','));
        assert.deepEqual(fs.readFileSync(files.source), encoded(expected)); assert.deepEqual(fs.readFileSync(files.slow), encoded(slow)); entry.passed = true;
      } finally { await instance.cleanup(); }
    }
    report.passed = true;
  } finally {
    for (const name of ['tmp', 'native-home']) fs.rmSync(dirs[name], { recursive: true, force: true });
    fs.writeFileSync(path.join(out, 'report.json'), JSON.stringify(report, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
  }
  console.log(JSON.stringify({ passed: report.passed, phases: report.phases.length, checks: report.phases.reduce((n, p) => n + (p.result?.checks.length || 0), 0) }));
}
if (require.main === module) { const [app, out] = process.argv.slice(2); run(path.resolve(app), path.resolve(out)).catch(() => { console.error('Native code output acceptance failed; inspect the owned receipt.'); process.exitCode = 1; }); }
module.exports = { driver };
