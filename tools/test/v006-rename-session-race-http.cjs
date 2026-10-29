'use strict';
// Opt-in acceptance, deliberately outside node --test. Uses only an explicit
// prebuilt kernel, fresh synthetic profile/documents and real HTTP/processes.
// Existing evidence is never reused; app tokens and settings bodies are not logged.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const { options } = require('./v006-table-clipboard-kernel.cjs');
const { startKernel } = require('./v006-rename-safe-http.cjs');

const clone = value => JSON.parse(JSON.stringify(value));
const id = () => crypto.randomUUID();
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const reviewCas = value => ({ document_id: value.document_id, generation: value.generation,
  review_revision: value.review_revision, call_id: id() });
const ERROR_STATUS = Object.freeze({
  session_version_conflict: 409,
  session_request_conflict: 409,
  session_version_invalid: 422,
  session_version_required: 428,
  settings_persistence_failed: 507,
});
async function fileSha(file) {
  const digest = crypto.createHash('sha256');
  for await (const chunk of fs.createReadStream(file)) digest.update(chunk);
  return digest.digest('hex');
}

async function main() {
  const args = options(process.argv.slice(2));
  assert(args.bin && !args.base && !args.playwright, 'An explicit prebuilt binary is required; this is not a browser fixture');
  const profile = path.join(args.evidence, 'profile');
  const documents = path.join(args.evidence, 'documents');
  fs.mkdirSync(profile); fs.mkdirSync(documents);
  const oldPath = path.join(documents, 'source.md');
  const newPath = path.join(documents, 'renamed.md');
  fs.writeFileSync(oldPath, '# Synthetic review document\n', { flag: 'wx' });
  const report = { schemaVersion: 2, platform: process.platform, syntheticOnly: true,
    nativeProcesses: true, tokenRequired: true, compiled: false, downloaded: false,
    scope: 'Real HTTP, separate native kernel PIDs and actual profile/document disk state; no browser or native-window claims',
    limitations: ['Direct HTTP does not exercise frontend startup/close barriers', 'No injected filesystem synchronization fault or machine power loss'],
    binarySha256: await fileSha(args.bin), scriptSha256: await fileSha(__filename),
    actors: [], checks: [], http: [], passed: false };
  const owned = new Map(), origins = new Map();
  let phase = 'startup';
  const check = name => report.checks.push(name);
  async function start(actor, actorProfile = profile, actorDocuments = documents) {
    const kernel = await startKernel(args.bin, actorProfile, actorDocuments);
    assert(!report.actors.some(item => item.pid === kernel.pid), 'Each acceptance actor must be a different native PID');
    report.actors.push({ actor, pid: kernel.pid, scenario: actorProfile === profile ? 'rename-lifecycle' : 'parallel-cas' });
    owned.set(actor, kernel); return kernel;
  }
  async function stop(actor) {
    const kernel = owned.get(actor); if (!kernel) return;
    await kernel.stop(); owned.delete(actor);
  }
  async function call(actor, route, body) {
    const kernel = owned.get(actor); assert(kernel, 'The synthetic acceptance actor is running');
    const request = body === undefined ? {} : { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) };
    const response = await kernel.api(route, request);
    origins.set(actor, new URL(response.url).origin);
    let value;
    try { value = await response.json(); }
    catch (_) { throw Error('Expected a JSON response at ' + route.split('?')[0] + ' (HTTP ' + response.status + ')'); }
    const errorCode = value?.error_code;
    report.http.push({ actor, method: body === undefined ? 'GET' : 'POST', route: route.split('?')[0], status: response.status,
      ...(typeof errorCode === 'string' ? { error_code: Object.hasOwn(ERROR_STATUS, errorCode) ? errorCode : 'unrecognized_error_code' } : {}) });
    if (route === '/api/session' && body !== undefined && response.status === 200) {
      assert.equal(value.call_id, body.call_id, 'The session receipt must acknowledge the exact submitted call');
      assert.deepEqual(value.session, body.session, 'The session receipt must acknowledge the exact submitted session');
    }
    return { status: response.status, value };
  }
  function validVersion(version) {
    assert(version && typeof version === 'object' && !Array.isArray(version), 'The session version must be a structured token');
    assert.deepEqual(Object.keys(version).sort(), ['epoch', 'generation']);
    assert.match(version.epoch, /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/);
    assert.match(version.generation, /^(0|[1-9][0-9]*)$/);
    assert(BigInt(version.generation) <= 18446744073709551615n, 'The decimal generation fits the native counter');
  }
  function rejected(response, errorCode) {
    assert.equal(response.status, ERROR_STATUS[errorCode], 'The HTTP status must match the stable session error');
    assert.equal(response.value.ok, false, 'A rejected request cannot report success');
    assert.equal(response.value.error_code, errorCode, 'The exact stable session error is required');
    assert.notEqual(response.value.persisted, true, 'A rejection cannot acknowledge persistence');
  }
  async function readSession(actor) {
    const response = await call(actor, '/api/session');
    assert.equal(response.status, 200, 'The dedicated session read must succeed');
    assert.equal(response.value.ok, true);
    assert(Object.hasOwn(response.value, 'session'), 'The dedicated read must expose session');
    assert(Object.hasOwn(response.value, 'session_version'), 'The dedicated read must expose its version');
    assert.notEqual(response.value.session_version, null, 'A session version cannot be null');
    assert.notEqual(response.value.session_version, '', 'A session version cannot be empty');
    validVersion(response.value.session_version);
    return clone(response.value);
  }
  function writeBody(session, version, callId = id()) {
    return { session: clone(session), expected_session_version: clone(version), call_id: callId };
  }
  function acknowledged(response, previousVersion, replayVersion) {
    assert.equal(response.status, 200);
    assert.equal(response.value.ok, true);
    assert.equal(response.value.persisted, true, 'Only a checked native durability acknowledgement permits progress');
    assert(Object.hasOwn(response.value, 'session_version'), 'A write receipt must expose its version');
    validVersion(response.value.session_version);
    if (replayVersion === undefined) {
      assert.equal(response.value.session_version.epoch, previousVersion.epoch, 'Writes preserve the profile epoch');
      assert.equal(BigInt(response.value.session_version.generation), BigInt(previousVersion.generation) + 1n, 'A new session write advances exactly one generation');
    }
    else assert.deepEqual(response.value.session_version, replayVersion, 'An idempotent current receipt does not advance its version');
    return clone(response.value.session_version);
  }
  async function unchanged(actor, version, session) {
    const current = await readSession(actor);
    assert.deepEqual(current.session_version, version, 'A rejected request cannot change the durable version');
    assert.deepEqual(current.session, session, 'A rejected request cannot change the durable session');
  }
  function sameReview(actual, expected) {
    for (const key of ['document_id', 'content', 'generation', 'review_revision', 'disk_revision', 'can_undo', 'can_redo', 'checkpoint'])
      assert.deepEqual(actual[key], expected[key], 'Rename/restart preserves review field ' + key);
    assert.deepEqual(actual.changes, expected.changes, 'The same durable pending change and original text survive');
    assert.equal(actual.changes.length, 1);
    assert.equal(actual.changes[0].decision, 'pending');
  }
  function documentPreserved(expectedBytes) {
    assert.equal(fs.existsSync(oldPath), false, 'No stale session action may recreate the old path');
    assert.deepEqual(fs.readFileSync(newPath), expectedBytes, 'Session actions do not rewrite document bytes');
  }
  try {
    await start('A');
    await start('B');
    const initial = await readSession('A');
    phase = 'session routes require an app token independently of the global token';
    const page = await fetch(origins.get('A'), { signal: AbortSignal.timeout(15000) });
    assert.equal(page.status, 200, 'The owned kernel provides its native bootstrap page');
    const token = (await page.text()).match(/name="readmd-app-token"\s+content="([^"]+)"/)?.[1];
    assert(typeof token === 'string' && token.length > 0, 'The bootstrap token stays in memory and must be present');
    for (const mode of ['no-token', 'lan-token-only']) for (const method of ['GET', 'POST']) {
      const headers = mode === 'lan-token-only' ? { 'X-ReadMD-Token': token } : {};
      const request = { method, headers, signal: AbortSignal.timeout(15000) };
      if (method === 'POST') {
        headers['Content-Type'] = 'application/json';
        request.body = JSON.stringify(writeBody(initial.session, initial.session_version));
      }
      const response = await fetch(origins.get('A') + '/api/session', request);
      report.http.push({ actor: mode, method, route: '/api/session', status: response.status });
      assert.equal(response.status, 403, 'Both session routes must reject a missing app token, even with a valid LAN token');
      await response.body?.cancel();
    }
    await unchanged('A', initial.session_version, initial.session);
    check('GET and POST session reject no-token and valid-LAN-token-only requests; authentication denial does not change session state');
    // B starts before A writes: a typed read must inspect disk, even when B's
    // legacy settings cache predates the subsequently persisted session.
    const cached = await call('B', '/api/settings'); assert.equal(cached.status, 200);
    phase = 'create and save a real pending review change';
    const opened = await call('A', '/api/file?p=' + encodeURIComponent(oldPath)); assert.equal(opened.status, 200);
    const reviewOpen = await call('A', '/api/review', { action: 'open', path: oldPath }); assert.equal(reviewOpen.status, 200);
    const proposed = await call('A', '/api/review', { action: 'propose', ...reviewCas(reviewOpen.value), origin: 'ai', task_id: 'synthetic-session-race-task',
      edits: [{ from: 2, to: 11, insert: 'Edited' }] });
    assert.equal(proposed.status, 200); assert.equal(proposed.value.changes[0].decision, 'pending');
    const saved = await call('A', '/api/save', { path: oldPath, content: proposed.value.content, encoding: 'utf-8',
      expected_revision: opened.value.revision, review_session: reviewCas(proposed.value) });
    assert.equal(saved.status, 200); assert.equal(saved.value.ok, true); assert(saved.value.review);
    const savedFile = await call('A', '/api/file?p=' + encodeURIComponent(oldPath)); assert.equal(savedFile.status, 200);
    const savedReview = clone(saved.value.review), savedBytes = fs.readFileSync(oldPath);
    assert.equal(savedReview.changes.length, 1); assert.equal(savedReview.changes[0].decision, 'pending');
    check('A real pending review change is saved through the native review and document HTTP APIs');

    phase = 'A and B read the same native session version';
    const base = { version: 1, tabs: [{ key: 'file:synthetic-session-race-tab', path: oldPath, name: 'source.md', dir: documents,
      source: 'file', revision: savedFile.value.revision, dirty: false }], active: 'file:synthetic-session-race-tab', folder: documents };
    const baselineRequest = writeBody(base, initial.session_version);
    const baselineVersion = acknowledged(await call('A', '/api/session', baselineRequest), initial.session_version);
    const aBase = await readSession('A'), bBase = await readSession('B');
    assert.deepEqual(aBase.session_version, baselineVersion); assert.deepEqual(bBase.session_version, baselineVersion);
    assert.deepEqual(aBase.session, base); assert.deepEqual(bBase.session, base);
    check('Two native PIDs read the same version; B sees a disk update made after its settings cache was created');

    const intent = { operation_id: 'rename-' + id(), old_path: oldPath, new_stem: 'renamed',
      expected_revision: savedFile.value.revision, expected_disk_revision: savedFile.value.disk_revision,
      review_session: reviewCas(savedReview) };
    const pending = { ...clone(base), tabs: [{ ...base.tabs[0], renamePending: intent }] };
    const pendingRequest = writeBody(pending, aBase.session_version);
    phase = 'persist pending intent, exact current replay and changed request rejection';
    const pendingVersion = acknowledged(await call('A', '/api/session', pendingRequest), baselineVersion);
    await unchanged('B', pendingVersion, pending);
    acknowledged(await call('B', '/api/session', pendingRequest), baselineVersion, pendingVersion);
    const changedPendingRequest = clone(pendingRequest); changedPendingRequest.session.tabs[0].name = 'different-content.md';
    rejected(await call('B', '/api/session', changedPendingRequest), 'session_request_conflict');
    await unchanged('A', pendingVersion, pending);
    check('The complete rename intent is durable before rename; current same-call replay is idempotent and changed content is rejected');

    phase = 'ordinary preferences from the stale process preserve the pending session';
    const preference = await call('B', '/api/settings', { theme: 'sepia', fontSize: 114 });
    assert.equal(preference.status, 200); assert.equal(preference.value.ok, true); assert.equal(preference.value.persisted, true);
    await unchanged('A', pendingVersion, pending);
    check('Ordinary preferences from a different process remain writable without advancing or erasing the pending session');

    phase = 'commit rename and terminate A before session settlement';
    const moved = await call('A', '/api/rename', { path: oldPath, ...intent });
    assert.equal(moved.status, 200); assert.equal(moved.value.state, 'committed'); assert.equal(moved.value.operation_id, intent.operation_id);
    sameReview(moved.value.review, savedReview); documentPreserved(savedBytes);
    await stop('A');
    check('A commits the actual no-replace rename, then exits before clearing its durable pending intent');

    phase = 'B stale-version and legacy session writes are refused';
    rejected(await call('B', '/api/session', writeBody(bBase.session, bBase.session_version)), 'session_version_conflict');
    rejected(await call('B', '/api/session', { session: bBase.session, call_id: id() }), 'session_version_required');
    rejected(await call('B', '/api/session', { session: bBase.session, expected_session_version: null, call_id: id() }), 'session_version_required');
    rejected(await call('B', '/api/session', { session: bBase.session, expected_session_version: bBase.session_version }), 'session_version_required');
    rejected(await call('B', '/api/session', { session: bBase.session, expected_session_version: bBase.session_version, call_id: '' }), 'session_version_required');
    for (const key of ['session', 'session_version', 'session_receipts', 'expected_session_version', '__readmd_session_version', '__readmd_session_receipts'])
      rejected(await call('B', '/api/settings', { [key]: key === 'session' ? bBase.session : null }), 'session_version_required');
    rejected(await call('B', '/api/session', { session: bBase.session, expected_session_version: { invalid: true }, call_id: id() }), 'session_version_invalid');
    await unchanged('B', pendingVersion, pending); documentPreserved(savedBytes);
    check('B cannot erase the intent with an old version, a missing/invalid version, or the legacy settings writer');

    phase = 'C restarts and recovers the same operation and pending review';
    await start('C');
    const recovery = await readSession('C');
    assert.deepEqual(recovery.session_version, pendingVersion); assert.deepEqual(recovery.session, pending);
    assert.deepEqual(recovery.session.tabs[0].renamePending, intent);
    const receipt = await call('C', '/api/rename/status', { operation_id: intent.operation_id });
    assert.equal(receipt.status, 200); assert.equal(receipt.value.state, 'committed'); assert.equal(receipt.value.operation_id, intent.operation_id);
    sameReview(receipt.value.review, savedReview);
    const newFile = await call('C', '/api/file?p=' + encodeURIComponent(newPath)); assert.equal(newFile.status, 200);
    assert.equal(newFile.value.revision, intent.expected_revision); assert.equal(newFile.value.disk_revision, intent.expected_disk_revision);
    const review = await call('C', '/api/review', { action: 'resume', path: newPath });
    assert.equal(review.status, 200); assert.equal(review.value.found, true); sameReview(review.value.snapshot, savedReview);
    documentPreserved(savedBytes);
    check('A fresh C PID finds the operation from session alone, reopens the real destination and resumes the identical pending review');

    phase = 'C durably settles the rename';
    const settled = { ...clone(base), tabs: [{ ...base.tabs[0], path: newPath, name: 'renamed.md', revision: newFile.value.revision }] };
    const settlementRequest = writeBody(settled, recovery.session_version);
    const settledVersion = acknowledged(await call('C', '/api/session', settlementRequest), pendingVersion);
    await unchanged('B', settledVersion, settled);
    // A historical success is unsafe after another version supersedes it.
    rejected(await call('B', '/api/session', pendingRequest), 'session_version_conflict');
    await unchanged('C', settledVersion, settled);
    check('Settlement publishes the destination without pending; an earlier successful intent receipt cannot be replayed as current');

    phase = 'C closes and delayed old writes cannot resurrect the tab';
    const closed = { ...clone(settled), tabs: [], active: '' };
    const closeRequest = writeBody(closed, settledVersion);
    const closedVersion = acknowledged(await call('C', '/api/session', closeRequest), settledVersion);
    acknowledged(await call('B', '/api/session', closeRequest), settledVersion, closedVersion);
    const changedCloseRequest = clone(closeRequest); changedCloseRequest.session = clone(settled);
    rejected(await call('B', '/api/session', changedCloseRequest), 'session_request_conflict');
    rejected(await call('B', '/api/session', writeBody(bBase.session, bBase.session_version)), 'session_version_conflict');
    rejected(await call('B', '/api/session', settlementRequest), 'session_version_conflict');
    rejected(await call('B', '/api/session', pendingRequest), 'session_version_conflict');
    rejected(await call('B', '/api/settings', { session: base }), 'session_version_required');
    await unchanged('C', closedVersion, closed); documentPreserved(savedBytes);
    check('Closing is durable; stale tabs, superseded receipts and same-call changed content cannot revive the closed tab');

    phase = 'normal preferences remain writable after closure';
    const afterClosePreference = await call('B', '/api/settings', { theme: 'dark', fontSize: 116 });
    assert.equal(afterClosePreference.status, 200); assert.equal(afterClosePreference.value.ok, true); assert.equal(afterClosePreference.value.persisted, true);
    await unchanged('B', closedVersion, closed);
    await stop('B'); await stop('C');

    phase = 'D reopens disk state after all earlier processes exit';
    await start('D');
    await unchanged('D', closedVersion, closed);
    const preferences = await call('D', '/api/settings'); assert.equal(preferences.status, 200);
    assert.equal(preferences.value.theme, 'dark'); assert.equal(preferences.value.fontSize, 116);
    const finalFile = await call('D', '/api/file?p=' + encodeURIComponent(newPath)); assert.equal(finalFile.status, 200);
    const finalReview = await call('D', '/api/review', { action: 'resume', path: newPath });
    assert.equal(finalReview.status, 200); assert.equal(finalReview.value.found, true); sameReview(finalReview.value.snapshot, savedReview);
    const finalReceipt = await call('D', '/api/rename/status', { operation_id: intent.operation_id });
    assert.equal(finalReceipt.status, 200); assert.equal(finalReceipt.value.state, 'committed');
    assert.equal(finalReceipt.value.operation_id, intent.operation_id); documentPreserved(savedBytes);
    check('After all prior PIDs exit, D retains the closed session, ordinary preferences, renamed bytes and the same durable pending review');

    phase = 'two real processes post different sessions against exactly the same version';
    const parallelProfile = path.join(args.evidence, 'parallel-profile');
    const parallelDocuments = path.join(args.evidence, 'parallel-documents');
    fs.mkdirSync(parallelProfile); fs.mkdirSync(parallelDocuments);
    await start('E', parallelProfile, parallelDocuments);
    await start('F', parallelProfile, parallelDocuments);
    const eBase = await readSession('E'), fBase = await readSession('F');
    assert.deepEqual(eBase.session_version, fBase.session_version, 'Both contenders capture one identical disk version before either POST starts');
    const contenders = ['E', 'F'].map(actor => ({ actor, session: { version: 1,
      tabs: [{ key: 'draft:synthetic-parallel-' + actor, path: '', name: 'synthetic-' + actor + '.md', source: 'recovery', dirty: false }],
      active: 'draft:synthetic-parallel-' + actor, folder: 'synthetic-writer-' + actor } }));
    const concurrent = await Promise.all(contenders.map(contender => call(contender.actor, '/api/session', writeBody(contender.session, eBase.session_version))));
    assert.deepEqual(concurrent.map(response => response.status).sort((left, right) => left - right), [200, 409], 'Exactly one native process wins and one conflicts');
    const winner = concurrent.findIndex(response => response.status === 200), loser = 1 - winner;
    const winningVersion = acknowledged(concurrent[winner], eBase.session_version);
    rejected(concurrent[loser], 'session_version_conflict');
    await unchanged('E', winningVersion, contenders[winner].session);
    await unchanged('F', winningVersion, contenders[winner].session);
    await stop('E'); await stop('F');
    await unchanged('D', closedVersion, closed); documentPreserved(savedBytes);
    check('Independent E/F native processes issue parallel different-session POSTs from the same version: exactly one wins, neither a merge nor a losing overwrite is persisted');
    check('The parallel-CAS scenario uses a separate synthetic profile and leaves the lifecycle session closed and document bytes unchanged');
    report.documentBytesSha256 = sha(savedBytes);
    report.passed = true;
  } catch (error) {
    // AssertionError.message may contain compared paths, token values or full
    // document diffs. Never serialize it, its actual/expected fields or stack.
    const names = new Set(['AssertionError', 'Error', 'TypeError', 'AbortError', 'TimeoutError']);
    const operators = new Set(['==', '===', '!==', 'deepStrictEqual', 'notDeepStrictEqual', 'match', 'fail']);
    report.failedStage = phase; report.failure = {
      name: names.has(error.name) ? error.name : 'Error',
      reason: 'Acceptance check or transport failed; inspect failedStage and the status-only HTTP trace',
      ...(operators.has(error.operator) ? { operator: error.operator } : {}),
    };
    process.exitCode = 1;
  } finally {
    for (const actor of [...owned.keys()]) {
      try { await stop(actor); }
      catch (_) { report.passed = false; report.stopFailed = true; process.exitCode = 1; }
    }
    fs.writeFileSync(path.join(args.evidence, 'report.json'), JSON.stringify(report, null, 2), { flag: 'wx' });
  }
  console.log(JSON.stringify({ passed: report.passed, checks: report.checks.length, nativeProcesses: report.actors.length,
    ...(report.failedStage ? { failedStage: report.failedStage, failure: report.failure } : {}),
    ...(report.stopFailed ? { stopFailed: true } : {}), report: 'report.json' }));
}
if (require.main === module) main().catch(() => {
  console.error(JSON.stringify({ passed: false, error: 'Acceptance setup failed; use a prebuilt binary and a new evidence directory' }));
  process.exitCode = 1;
});
