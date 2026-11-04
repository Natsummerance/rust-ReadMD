'use strict';
// Preparation is inert: importing this module never launches a process/app.
// An explicit run uses a fresh owned profile, genuine managed-pack importer,
// LaunchServices/Cocoa/WKWebView and existing UI entries. No picker/key shim,
// model inference, credential, daily profile, Rust build or source mutation.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const assert = require('node:assert/strict'), http = require('node:http'), net = require('node:net');
const { spawn } = require('node:child_process'), vm = require('node:vm');
const { prepare } = require('./macos-native-system-host.cjs');
const { nodePack, installNodePack } = require('../tools/test/node-runtime-fixture.cjs');
const repo = path.resolve(__dirname, '..'), workspace = path.dirname(repo);
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
const phases = ['node_run_save_quit', 'ai_stream_stop_quit', 'reopen_verify_close'];
const CODE = "console.log('Node 中文😀 42');", STDOUT = 'Node 中文😀 42';
const NODE_ORIGINAL = '# Node 固定文档 中文😀\n\n```javascript cmd output=true\n' + CODE + '\n```\n\n尾注必须保留。\n';
const NODE_EXPECTED = '# Node 固定文档 中文😀\n\n```javascript cmd output=true\n' + CODE + '\n```\n\n<!-- code_chunk_output -->\n\n' + STDOUT + '\n\n<!-- /code_chunk_output -->\n\n尾注必须保留。\n';
const AI_ORIGINAL = '# 聊天原件 中文😀\n\n普通聊天不能改正文。\n';
const FIRST = '第一段 中文', LAST = '，第二段 😀 完成', PARTIAL = '断流 partial 中文😀', STOPPED = '停止 partial 中文😀';
const PROVIDER_ID = 'custom:native-keyless-fixture', MODEL_ID = 'owned-protocol-fixture';
const encode = text => Buffer.from('\ufeff' + text.replaceAll('\n', '\r\n'));

function owned(input, kind, fresh = false) {
  assert.equal(typeof input, 'string', 'explicit absolute workspace input required');
  assert(path.isAbsolute(input) && path.resolve(input) === input, 'canonical absolute input required');
  assert(input.startsWith(workspace + path.sep), 'only workspace-owned inputs');
  let cursor = workspace;
  for (const part of path.relative(workspace, input).split(path.sep)) {
    cursor = path.join(cursor, part);
    if (!fs.existsSync(cursor)) { assert(fresh && cursor === input, 'owned parent must exist'); continue; }
    assert(!fs.lstatSync(cursor).isSymbolicLink(), 'owned paths must not traverse links');
  }
  if (fresh) assert(!fs.existsSync(input), 'new evidence directory required');
  else { const stat = fs.lstatSync(input); assert(kind === 'file' ? stat.isFile() : stat.isDirectory()); assert.equal(fs.realpathSync(input), input); }
  return input;
}
async function until(check, label, ms = 10000) {
  const end = Date.now() + ms;
  while (Date.now() < end) { if (await check()) return; await pause(20); }
  throw Error(label);
}
function cspFrom(assets) {
  const value = fs.readFileSync(path.join(assets, 'index.html'), 'utf8').match(/<meta\s+http-equiv="Content-Security-Policy"\s+content="([^"]+)"/i)?.[1];
  assert(value && value.includes("script-src 'self'")); assert(!/script-src[^;]*'unsafe-(?:inline|eval)'/.test(value)); return value;
}

// Only Rust talks to this server. There is no fixture route in the UI origin,
// no CORS grant and no browser fetch to the provider. These are protocol bytes,
// not model output. Each request has one deterministic, bounded plan.
async function localProvider() {
  const requests = [], failures = [], sockets = new Set(), timers = new Set();
  const server = http.createServer(async (req, res) => {
    try {
      assert(req.socket.remoteAddress === '127.0.0.1' || req.socket.remoteAddress === '::ffff:127.0.0.1');
      assert.equal(req.method, 'POST'); assert.equal(req.url, '/v1/chat/completions');
      for (const header of ['authorization', 'origin', 'sec-fetch-site', 'x-readmd-app-token', 'x-readmd-token']) assert.equal(req.headers[header], undefined);
      let text = ''; for await (const chunk of req) { text += chunk; assert(Buffer.byteLength(text) <= 1048576); }
      const body = JSON.parse(text), user = (body.messages || []).filter(m => m.role === 'user').map(m => m.content).join('\n');
      const name = user.match(/READMD_CASE_native_(normal|truncated|stop)/)?.[1];
      assert(name && body.stream === true && body.model === MODEL_ID); assert(!requests.some(r => r.name === name), 'no request replay or automatic continuation');
      const row = { name, stream: true, modelFixtureOnly: true, receivedAt: Date.now(), deltas: [], closedAt: null };
      requests.push(row); res.once('close', () => { row.closedAt = Date.now(); });
      res.writeHead(200, { 'Content-Type': 'text/event-stream; charset=utf-8', 'Cache-Control': 'no-store' }); res.flushHeaders();
      const frame = value => res.write('data: ' + JSON.stringify(value) + '\n\n');
      const delta = value => { row.deltas.push({ atMs: Date.now() - row.receivedAt, characters: [...value].length }); frame({ choices: [{ delta: { content: value } }] }); };
      delta(name === 'normal' ? FIRST : name === 'truncated' ? PARTIAL : STOPPED);
      if (name === 'stop') return;
      const timer = setTimeout(() => {
        timers.delete(timer); if (res.destroyed) return;
        if (name === 'truncated') { res.end(); return; }
        delta(LAST); frame({ choices: [], usage: { prompt_tokens: 1, completion_tokens: 3, total_tokens: 4 } });
        frame({ choices: [{ finish_reason: 'stop', delta: {} }] }); res.end('data: [DONE]\n\n');
      }, name === 'normal' ? 1600 : 300);
      timers.add(timer); res.once('close', () => { clearTimeout(timer); timers.delete(timer); });
    } catch (_) { failures.push('fixture_protocol_rejected'); res.destroy(); }
  });
  server.on('connection', socket => { sockets.add(socket); socket.once('close', () => sockets.delete(socket)); });
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  return { base: 'http://127.0.0.1:' + server.address().port, requests, failures,
    async stop() { for (const timer of timers) clearTimeout(timer); for (const socket of sockets) socket.destroy(); await new Promise(resolve => server.close(resolve)); } };
}

// The pre-install host receives the exact explicit assets/profile used later.
// The helper's authenticated chunk importer is the sole managed-directory writer.
async function startHeadless(binary, profile, docs, assets, dirs) {
  const probe = net.createServer(); await new Promise((resolve, reject) => { probe.once('error', reject); probe.listen(0, '127.0.0.1', resolve); });
  const port = probe.address().port; await new Promise(resolve => probe.close(resolve));
  const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !/TOKEN|SECRET|CREDENTIAL|API_KEY|PROXY|HERMES|READMD_NATIVE_QA/i.test(key)));
  Object.assign(env, { READMD_DATA_DIR: profile, READMD_WORKSPACE: docs, READMD_EXTRA_ROOTS: '', READMD_ASSETS_DIR: '', READMD_REQUIRE_TOKEN: '1', TMPDIR: dirs.tmp, SQLITE_TMPDIR: dirs.tmp, CFFIXED_USER_HOME: dirs['native-home'] });
  const child = spawn(binary, ['--no-window', '--port', String(port), '--require-token', '--data-dir', profile, '--workspace', docs, '--assets', assets], { cwd: repo, env, stdio: 'ignore' });
  let spawnError = false; child.once('error', () => { spawnError = true; }); const base = 'http://127.0.0.1:' + port;
  const stop = async () => {
    if (!spawnError && child.exitCode === null && child.signalCode === null) {
      child.kill('SIGTERM'); await until(() => child.exitCode !== null || child.signalCode !== null, 'owned_preinstall_host_did_not_stop', 10000);
    }
    return { ownedChild: true, pid: child.pid, code: child.exitCode, signal: child.signalCode, nativeNormalQuitClaimed: false };
  };
  try {
    let token = '';
    await until(async () => {
      assert(!spawnError && child.exitCode === null && child.signalCode === null, 'owned preinstall host exited');
      try { const r = await fetch(base, { signal: AbortSignal.timeout(1000) }); token = (await r.text()).match(/name="readmd-app-token"\s+content="([^"]+)"/)?.[1] || ''; return r.ok && Boolean(token); } catch (_) { return false; }
    }, 'owned_preinstall_readiness_timeout', 25000);
    return { stop, api: (route, request = {}) => fetch(base + route, { ...request, headers: { ...request.headers, 'X-ReadMD-App-Token': token, 'X-ReadMD-Token': token }, signal: AbortSignal.timeout(15000) }) };
  } catch (error) { await stop(); throw error; }
}

function driver(phase, files, config) {
  assert(phases.includes(phase));
  return String.raw`(async()=>{
 const phase=${JSON.stringify(phase)},files=${JSON.stringify(files)},config=${JSON.stringify(config)},checks=[],calls=[];
 const original=${JSON.stringify(NODE_ORIGINAL)},expected=${JSON.stringify(NODE_EXPECTED)},aiOriginal=${JSON.stringify(AI_ORIGINAL)};
 const first=${JSON.stringify(FIRST)},reply=${JSON.stringify(FIRST + LAST)},partial=${JSON.stringify(PARTIAL)},stopped=${JSON.stringify(STOPPED)};
 const check=(ok,label)=>{if(!ok)throw Error(label);checks.push(label);};
 const pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
 const until=async(fn,label,ms=12000)=>{const end=Date.now()+ms;while(Date.now()<end){if(await fn())return;await pause(25);}throw Error(label);};
 const report=ok=>window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok,phase,checks}}));
 const clean=value=>String(value||'').replace(/^\uFEFF/,'').replace(/\r\n?/g,'\n');
 const json=async(route,body)=>{const r=await apiFetch(route,body===undefined?{}:{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});check(r.ok,'authenticated_api_works');return r.json();};
 const originalFetch=apiFetch;
 apiFetch=async function(url,options){
  const route=new URL(typeof url==='string'?url:url.url,location.href).pathname,started=Date.now();let body;try{body=JSON.parse(options?.body||'null');}catch(_){}
  const kind=route==='/api/code/run'?'code':route==='/api/ai/chat'?'chat':route==='/api/task/cancel'?'cancel':route==='/api/review'&&body?.action==='code_target'?'target':null;
  const row=kind?{kind,body}:null;if(row)calls.push(row);
  const r=await originalFetch(url,options);
  if(row&&kind!=='chat')void r.clone().json().then(value=>{row.response=value;row.status=r.status;row.elapsedMs=Date.now()-started;}).catch(()=>{row.failed=true;});
  return r;
 };
 const history=async redo=>{const tab=getActiveTab(),before=ReadMDReview.snapshot(tab).review_revision;check(ReadMDReview.history(redo)===true,redo?'one_redo_entry':'one_undo_entry');await until(()=>!ReadMDReview.busy(tab)&&ReadMDReview.snapshot(tab).review_revision>before,'history_terminal_timeout');check(ReadMDReview.snapshot(tab).review_revision===before+1,'one_history_transaction');};
 const openAi=async()=>{await loadAiConfig();if($('ai-panel').classList.contains('hidden'))$('btn-ai').click();await until(()=>!$('ai-panel').classList.contains('hidden'),'ai_panel_not_visible');check($('ai-provider').value===config.providerId,'keyless_fixture_selected');check(!$('ai-key').value,'no_raw_key');const p=currentAiProvider();check(p&&isLocalAiProvider(p)&&!p.has_key&&!p.credential_id,'no_credential_reference');$('ai-stream').checked=true;};
 const send=name=>{$('ai-prompt').value='READMD_CASE_native_'+name;$('ai-prompt').dispatchEvent(new Event('input'));$('ai-run').click();};
 const idle=()=>!state.ai.busy&&!state.ai.preparing;
 try{
  await __readmdSessionReady;check(hasPy&&document.body.classList.contains('native-macos'),'actual_cocoa_wkwebview');
  check(document.querySelector('meta[http-equiv="Content-Security-Policy"]')?.content===config.csp,'production_csp_unchanged');
  check(Boolean(document.querySelector('meta[name="readmd-app-token"]')?.content),'nonempty_app_token');
  const anonymous=await fetch('/api/settings',{cache:'no-store'});check([401,403].includes(anonymous.status),'unauthenticated_api_refused');
  check(ReadMDCodeRuns.busy()===0&&!state.ai.busy&&!state.ai.preparing,'no_automatic_continuation');
  const node=await json('/api/capabilities/packs',{action:'status',id:'node'});check(node.ready===true,'managed_node_ready');
  if(phase==='node_run_save_quit'){
   check(state.tabs.length===0,'fresh_native_document_session');await loadFile(files.node);if(!state.editing)await toggleEdit();setPvLayout('right');await renderPreview();
   const owner=getActiveTab(),stamp=await ReadMDReview.capture(owner);check(owner.path===files.node&&cmView.state.doc.toString()===original&&!hasUnsavedEditorChanges(),'fixed_node_document_clean');
   await until(()=>document.querySelectorAll('#preview-pane .code-chunk-card').length===1,'node_card_timeout');const card=document.querySelector('#preview-pane .code-chunk-card');
   check(card._readmdCodeOwner===owner,'node_card_has_fixed_document_owner');card.querySelector('.code-chunk-run-btn').click();
   await until(()=>ReadMDCodeRuns.busy()===0&&cmView.state.doc.toString()===expected,'node_write_timeout');
   await until(()=>calls.some(c=>c.kind==='code'&&c.response),'node_receipt_timeout');
   const runs=calls.filter(c=>c.kind==='code'),targets=calls.filter(c=>c.kind==='target');
   check(runs.length===1&&runs[0].body.confirm===true&&runs[0].body.code.trim()===config.code,'one_real_single_block_request');
   check(runs[0].response.ok===true&&runs[0].response.stdout===config.stdout,'real_node_stdout');
   const runtime=runs[0].response.runtime;check(runtime?.id==='node'&&runtime.version===config.node.version&&runtime.manifest_sha256===config.node.manifestSha256,'real_managed_node_runtime');
   check(targets.length===1&&targets[0].body.document_id===stamp.document_id&&targets[0].body.source_line===3,'fixed_document_and_source_target');
   check(getActiveTab()===owner&&!hasUnsavedEditorChanges(),'node_output_saved_in_place');
   await history(false);check(cmView.state.doc.toString()===original,'one_undo_removes_node_output');
   await history(true);check(cmView.state.doc.toString()===expected,'one_redo_restores_node_output');check(await saveEdit()===true,'node_native_save_acknowledged');
   await closeTab(owner.id);check(state.tabs.length===0&&!state.editing,'node_document_closed_cleanly');
  }else if(phase==='ai_stream_stop_quit'){
   check(state.tabs.length===0,'node_close_survives_new_process');await loadFile(files.ai);const owner=getActiveTab();await ReadMDReview.capture(owner);const generation=ReadMDReview.snapshot(owner).generation;
   check(clean(getCurrentDocumentText())===aiOriginal,'fixed_ai_original_visible');await openAi();send('normal');
   await until(()=>state.ai.raw===first&&state.ai.busy&&$('ai-output').innerText.includes(first),'first_real_delta_not_visible');
   check($('ai-output').getBoundingClientRect().width>0,'first_real_delta_visible_before_completion');await pause(250);check(state.ai.raw===first&&state.ai.busy,'first_delta_remains_pending');
   await until(()=>idle()&&state.ai.sessionId,'normal_chat_save_timeout');check(state.ai.raw===reply&&state.ai.sessUsage.total_tokens===4,'complete_reply_and_one_final_usage');
   check(state.ai.messages.length===2&&!state.ai.messages[1].incomplete,'complete_normal_conversation');
   const saved=await json('/api/ai/history?id='+encodeURIComponent(state.ai.sessionId));check(saved.session.messages.at(-1).content===reply,'normal_session_saved_by_real_backend');
   $('ai-clear-ctx').click();send('truncated');await until(()=>idle()&&state.ai.messages.at(-1)?.incomplete,'truncated_chat_timeout');
   check(state.ai.raw===partial&&document.querySelector('#ai-output .is-error .ai-msg-body')?.innerText.includes(partial),'truncated_partial_visible_and_marked');check(state.ai.sessionId===null&&state.ai.sessUsage.total_tokens===0,'truncated_not_saved_complete');
   $('ai-clear-ctx').click();send('stop');await until(()=>state.ai.raw===stopped&&state.ai.busy,'stop_partial_timeout');
   const stopAt=Date.now();$('ai-stop').click();await until(()=>idle()&&state.ai.messages.at(-1)?.incomplete,'stop_ui_timeout',2000);check(Date.now()-stopAt<2000&&state.ai.raw===stopped,'stop_keeps_partial_and_releases_ui');
   await until(()=>calls.some(c=>c.kind==='cancel'&&c.response),'real_cancel_receipt_timeout',2000);const cancels=calls.filter(c=>c.kind==='cancel');
   check(cancels.every(c=>c.status===200&&['cancelling','unknown'].includes(c.response.state)),'real_fixed_task_cancel_receipt');
   check(state.ai.sessionId===null&&state.ai.sessUsage.total_tokens===0,'stopped_not_saved_complete');
   const list=await json('/api/ai/history');check(list.sessions.length===1&&list.sessions[0].id===saved.session.id,'only_complete_conversation_in_history');
   check(getActiveTab()===owner&&clean(getCurrentDocumentText())===aiOriginal&&ReadMDReview.snapshot(owner).generation===generation,'ordinary_chat_preserves_document_and_review');
  }else{
   check(state.tabs.length===1&&getActiveTab()?.path===files.ai,'new_process_restores_ai_document_only');
   await pause(600);check(!state.ai.busy&&!state.ai.preparing&&calls.every(c=>c.kind!=='chat'&&c.kind!=='code'),'restart_does_not_resume_node_or_ai');
   await loadFile(files.node);if(!state.editing)await toggleEdit();check(cmView.state.doc.toString()===expected&&!hasUnsavedEditorChanges(),'node_saved_bytes_reopen_clean');
   const nodeTab=getActiveTab();await closeTab(nodeTab.id);check(getActiveTab()?.path===files.ai,'closing_node_restores_ai_document');
   await openAi();await loadAiSessions();$('ai-history-open').click();
   const button=[...document.querySelectorAll('#ai-history-list .ai-history-load')].find(b=>b.textContent.includes('READMD_CASE_native_normal'));check(!!button,'saved_normal_history_button');button.click();
   await until(()=>state.ai.sessionId===config.sessionId&&!state.ai.loadingSessionId,'history_reopen_timeout');
   check(state.ai.messages.length===2&&state.ai.messages[1].content===reply&&!state.ai.messages[1].incomplete&&state.ai.sessUsage.total_tokens===4,'new_process_reopens_complete_conversation');
   check($('ai-output').innerText.includes(reply),'reopened_conversation_visible');await pause(600);check(calls.every(c=>c.kind!=='chat'&&c.kind!=='code'),'reopen_requires_explicit_new_send');
   check(clean(getCurrentDocumentText())===aiOriginal,'chat_original_reopens_unchanged');await closeTab(getActiveTab().id);check(state.tabs.length===0&&!state.editing,'last_original_document_closed_cleanly');
  }
  const modules=await json('/api/modules');check(modules.modules.ocr==='idle'&&modules.modules.transcribe==='idle','ocr_transcribe_stay_idle');
  check(await ReadMDSession.flush()===true,'native_session_acknowledged');apiFetch=originalFetch;report(true);await py.request_quit();
 }catch(error){apiFetch=originalFetch;checks.push('failed_'+(/^[a-z_0-9]{1,110}$/.test(error.message)?error.message:'native_ai_node_operation'));report(false);window.ipc.postMessage('readmd:quit');}
})();`;
}

async function run(app, out, assets, headlessBin) {
  assert.equal(process.platform, 'darwin'); owned(app, 'directory'); owned(assets, 'directory'); owned(headlessBin, 'file'); owned(out, 'directory', true);
  const proof = nodePack(); owned(proof.file, 'file');
  for (const file of ['catalog.json', `node-${proof.manifest.version}-${proof.manifest.platform}.lock.json`]) assert.equal(sha(fs.readFileSync(path.join(assets, 'capabilities', file))), sha(fs.readFileSync(path.join(repo, 'assets/capabilities', file))), 'explicit assets must bind the real importer catalog');
  const csp = cspFrom(assets), binding = { bootSha256: sha(fs.readFileSync(path.join(assets, 'readmd.boot.js'))), indexSha256: sha(fs.readFileSync(path.join(assets, 'index.html'))) };
  fs.mkdirSync(out, { mode: 0o700 });
  const dirs = Object.fromEntries(['documents', 'profile', 'tmp', 'native-home'].map(name => [name, path.join(out, name)]));
  for (const dir of Object.values(dirs)) fs.mkdirSync(dir, { mode: 0o700 });
  const scratch = ['tmp', 'native-home'].map(name => ({ path: dirs[name], identity: fs.lstatSync(dirs[name]) }));
  const files = { node: path.join(dirs.documents, 'Node 固定文档 中文😀.md'), ai: path.join(dirs.documents, '聊天原件 中文😀.md') };
  fs.writeFileSync(files.node, encode(NODE_ORIGINAL), { flag: 'wx' }); fs.writeFileSync(files.ai, encode(AI_ORIGINAL), { flag: 'wx' });
  fs.writeFileSync(path.join(dirs.profile, '.readmd-native-qa-owned'), 'readmd-synthetic-native-qa-v1', { flag: 'wx' });
  fs.writeFileSync(path.join(dirs.profile, 'settings.json'), JSON.stringify({ lang: 'zh-CN', closeToTray: false, autoCheckUpdate: false, petEnabled: false }), { flag: 'wx' });
  const report = { passed: false, nativeAcceptanceExecuted: false, nativeHost: 'LaunchServices / Cocoa WKWebView', scriptedUiEntries: true,
    pickerCertified: false, physicalKeysCertified: false, modelInferenceRequested: false, providerProtocolFixtureOnly: true, credentialOperationsRequested: false,
    productionCspUnchanged: false, strictAuthCertified: false, actualCocoaWkwebview: false, desktopBudgetMs: 180000, ...binding,
    harnessSha256: sha(fs.readFileSync(__filename)), headlessBinarySha256: sha(fs.readFileSync(headlessBin)), systemHostSha256: sha(fs.readFileSync(require.resolve('./macos-native-system-host.cjs'))),
    pack: { sha256: proof.sha256, manifestSha256: proof.manifestSha256, version: proof.manifest.version, platform: proof.manifest.platform, bytes: proof.size }, phases: [] };
  let provider, preparing, desktopStarted, normalHistoryBytes, sessionId;
  const verifyBytes = nodeText => {
    assert.deepEqual(fs.readFileSync(files.node), encode(nodeText)); assert.deepEqual(fs.readFileSync(files.ai), encode(AI_ORIGINAL));
    assert.equal(sha(fs.readFileSync(path.join(assets, 'readmd.boot.js'))), binding.bootSha256); assert.equal(sha(fs.readFileSync(path.join(assets, 'index.html'))), binding.indexSha256);
  };
  try {
    provider = await localProvider(); preparing = await startHeadless(headlessBin, dirs.profile, dirs.documents, assets, dirs);
    try {
      report.installed = await installNodePack(preparing, proof);
      const r = await preparing.api('/api/ai/config', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ providers: [{ id: PROVIDER_ID, name: 'Native keyless protocol fixture', category: 'local', format: 'openai', mode: 'chat', base_url: provider.base + '/v1', models: [MODEL_ID] }], current: { provider_id: PROVIDER_ID, model: MODEL_ID } }) });
      const value = await r.json(); assert.equal(r.status, 200); const configured = value.custom.find(p => p.id === PROVIDER_ID); assert(configured && !configured.has_key && !configured.credential_id);
    } finally { report.preinstallHostExit = await preparing.stop(); preparing = null; }
    verifyBytes(NODE_ORIGINAL); const host = prepare(app, out, { tmp: dirs.tmp, home: dirs['native-home'], assets });
    Object.assign(report, { binarySha256: host.binarySha256, helperSha256: host.helperSha256, bundleIdentifier: host.bundleIdentifier });
    desktopStarted = Date.now(); const deadline = desktopStarted + 175000;
    for (const phase of phases) {
      assert(Date.now() < deadline, 'native_desktop_budget');
      const script = driver(phase, files, { csp, providerId: PROVIDER_ID, node: report.installed, sessionId, code: CODE, stdout: STDOUT });
      assert(Buffer.byteLength(script) < 64 * 1024); new vm.Script(script); fs.writeFileSync(path.join(out, phase + '-driver.js'), script, { flag: 'wx', mode: 0o600 }); fs.writeFileSync(path.join(dirs.profile, 'native-qa.js'), script);
      const instance = await host.launch({ profile: dirs.profile, docs: dirs.documents, log: path.join(out, phase + '.log') });
      report.nativeAcceptanceExecuted = true; const entry = { phase, driverSha256: sha(script), host: instance.receipt, passed: false }; report.phases.push(entry);
      try {
        await instance.wait(Math.max(1, Math.min(45000, deadline - Date.now())));
        const receipt = path.join(dirs.profile, 'native-qa-result.json'); entry.result = JSON.parse(fs.readFileSync(receipt)); fs.renameSync(receipt, path.join(out, phase + '.json'));
        assert.equal(entry.result.phase, phase); assert.equal(entry.result.ok, true, entry.result.checks.join(','));
        assert.equal(instance.receipt.forcedCleanup, false); assert.equal(instance.receipt.nativeProcessGone, true); assert.equal(instance.receipt.launcherOutcome.code, 0);
        verifyBytes(NODE_EXPECTED);
        if (phase === 'node_run_save_quit') assert.equal(provider.requests.length, 0, 'Node phase must not send AI automatically');
        else {
          assert.equal(provider.failures.length, 0); assert.deepEqual(provider.requests.map(r => r.name), ['normal', 'truncated', 'stop']);
          await until(() => provider.requests.every(r => r.closedAt !== null), 'owned_provider_socket_not_closed', 2000);
          const file = path.join(dirs.profile, 'ai-history.json'), bytes = fs.readFileSync(file), sessions = JSON.parse(bytes);
          assert.equal(sessions.length, 1); assert.equal(sessions[0].messages.length, 2); assert.equal(sessions[0].messages[1].content, FIRST + LAST); assert(!sessions[0].messages[1].incomplete); assert.equal(sessions[0].usage.total_tokens, 4);
          if (phase === 'ai_stream_stop_quit') { sessionId = sessions[0].id; normalHistoryBytes = bytes; }
          else assert.deepEqual(bytes, normalHistoryBytes, 'reopening must not save/continue a new reply');
        }
        entry.passed = true;
      } finally { await instance.cleanup(); }
    }
    const normal = provider.requests.find(r => r.name === 'normal'); assert.equal(normal.deltas.length, 2); assert(normal.deltas[1].atMs - normal.deltas[0].atMs >= 1400);
    assert.equal(new Set(report.phases.map(p => p.host.initialAppContext.pid)).size, phases.length, 'each phase must be a new real process');
    report.desktopElapsedMs = Date.now() - desktopStarted; assert(report.desktopElapsedMs < report.desktopBudgetMs);
    report.actualCocoaWkwebview = report.phases.every(p => p.result.checks.includes('actual_cocoa_wkwebview'));
    report.productionCspUnchanged = report.phases.every(p => p.result.checks.includes('production_csp_unchanged'));
    report.strictAuthCertified = report.phases.every(p => p.result.checks.includes('unauthenticated_api_refused'));
    report.normalNativeExitCount = report.phases.filter(p => p.host.nativeProcessGone && !p.host.forcedCleanup && p.host.launcherOutcome.code === 0).length;
    report.documents = [{ name: path.basename(files.node), sha256: sha(encode(NODE_EXPECTED)), bytes: encode(NODE_EXPECTED).length }, { name: path.basename(files.ai), sha256: sha(encode(AI_ORIGINAL)), bytes: encode(AI_ORIGINAL).length }];
    report.exactBomCrlfVerified = true; report.noAutomaticContinuation = true; report.passed = true;
  } catch (error) {
    report.failure = { name: error.name, message: String(error.message).replaceAll(out, '<owned>').replaceAll(workspace, '<workspace>').slice(0, 700) };
    throw error;
  } finally {
    if (preparing) report.preinstallHostExit = await preparing.stop();
    if (provider) { report.provider = { requests: provider.requests, failures: provider.failures, credentialsUsed: false, browserProviderFetchPermittedByHarness: false }; await provider.stop(); }
    assert.equal(sha(fs.readFileSync(proof.file)), proof.sha256, 'original Node pack must remain unchanged');
    for (const item of scratch) { const now = fs.lstatSync(item.path); assert(now.isDirectory() && !now.isSymbolicLink() && now.ino === item.identity.ino && now.dev === item.identity.dev); fs.rmSync(item.path, { recursive: true }); }
    fs.writeFileSync(path.join(out, 'report.json'), JSON.stringify(report, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
    console.log(JSON.stringify({ passed: report.passed, nativeAcceptanceExecuted: report.nativeAcceptanceExecuted, phases: report.phases.length, checks: report.phases.reduce((n, p) => n + (p.result?.checks.length || 0), 0), normalNativeExitCount: report.normalNativeExitCount || 0 }));
  }
}
if (require.main === module) {
  const args = process.argv.slice(2);
  if (args.length !== 4) { console.error('Usage: READMD_TEST_NODE_PACK=/absolute/workspace/node.readmd-pack node ui-tests/macos-ai-node-system-native.cjs /absolute/NativeQA.app /absolute/new-output /absolute/assets /absolute/headless-readmd'); process.exitCode = 1; }
  else run(...args).catch(() => { console.error('Native AI/Node acceptance failed; inspect its fresh owned report.'); process.exitCode = 1; });
}
module.exports = { driver, run, phases, encode, NODE_ORIGINAL, NODE_EXPECTED, AI_ORIGINAL };
