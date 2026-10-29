'use strict';
// Opt-in acceptance against an explicit prebuilt kernel and owned synthetic
// files only. Never builds, downloads, reads user files, or records tokens.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), net = require('node:net');
const { spawn } = require('node:child_process');
const assert = require('node:assert/strict');
const { options } = require('./v006-table-clipboard-kernel.cjs');
const { ROOT } = require('./v006-table-clipboard-host.cjs');
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const callId = () => crypto.randomUUID();
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function startKernel(binary, profile, workspace) {
  const socket = net.createServer();
  await new Promise((resolve, reject) => { socket.once('error', reject); socket.listen(0, '127.0.0.1', resolve); });
  const port = socket.address().port;
  await new Promise(resolve => socket.close(resolve));
  const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !/TOKEN|SECRET|CREDENTIAL|HERMES|PROXY|API_KEY/i.test(key)));
  Object.assign(env, { READMD_DATA_DIR: profile, READMD_EXTRA_ROOTS: '', READMD_REQUIRE_TOKEN: '1' });
  const child = spawn(binary, ['--no-window', '--port', String(port), '--require-token', '--data-dir', profile,
    '--workspace', workspace, '--assets', path.join(ROOT, 'assets')], { cwd: ROOT, windowsHide: true, stdio: 'ignore', env });
  let spawnError; child.on('error', error => { spawnError = error; });
  const base = `http://127.0.0.1:${port}`;
  const stop = async () => {
    if (child.exitCode !== null || child.signalCode !== null || spawnError) return;
    child.kill();
    const until = Date.now() + 10000;
    while (child.exitCode === null && child.signalCode === null && Date.now() < until) await delay(50);
    if (child.exitCode === null && child.signalCode === null) throw Error('Owned kernel did not stop');
  };
  try {
    const until = Date.now() + 30000;
    while (Date.now() < until) {
      if (spawnError || child.exitCode !== null || child.signalCode !== null) throw Error('Owned kernel exited before readiness');
      try {
        const page = await fetch(base, { signal: AbortSignal.timeout(1500) });
        const token = (await page.text()).match(/name="readmd-app-token"\s+content="([^"]+)"/)?.[1];
        if (page.ok && token) return { pid: child.pid, stop, api: (route, request = {}) => fetch(base + route, { ...request,
          headers: { ...request.headers, 'X-ReadMD-App-Token': token, 'X-ReadMD-Token': token }, signal: AbortSignal.timeout(15000) }) };
      } catch (_) { /* The owned server is still starting. */ }
      await delay(100);
    }
    throw Error('Owned kernel startup deadline exceeded');
  } catch (error) { await stop(); throw error; }
}
async function main() {
  const args = options(process.argv.slice(2)); assert(args.bin && !args.base, 'Explicit prebuilt binary required');
  const documents = path.join(args.evidence, 'documents'), profile = path.join(args.evidence, 'profile');
  fs.mkdirSync(documents); fs.mkdirSync(profile);
  const report = { schemaVersion: 1, platform: process.platform, syntheticOnly: true, compiled: false, downloaded: false,
    binarySha256: sha(fs.readFileSync(args.bin)), sourceSha256: sha(fs.readFileSync(path.join(__dirname, '../../rust/readmd-kernel/src/server.rs'))), checks: [] };
  let kernel, secondKernel, phase = 'startup';
  const file = name => path.join(documents, name);
  async function api(route, body) {
    const request = body === undefined ? {} : { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) };
    const response = await kernel.api(route, request), result = await response.json();
    return { status: response.status, body: result };
  }
  const get = p => api('/api/file?p=' + encodeURIComponent(p));
  const rename = (p, stem, descriptor, review, operationId = callId()) => api('/api/rename', { path: p, new_stem: stem,
    expected_revision: descriptor.revision, expected_disk_revision: descriptor.disk_revision,
    operation_id: operationId, ...(review ? { review_session: review } : {}) });
  const cas = s => ({ document_id: s.document_id, generation: s.generation, review_revision: s.review_revision, call_id: callId() });
  try {
    kernel = await startKernel(args.bin, profile, documents); report.kernelPids = [kernel.pid];
    phase = 'unopened file cannot be renamed';
    const unopened = file('unopened.md'); fs.writeFileSync(unopened, 'Not opened\n', { flag: 'wx' });
    const denied = await rename(unopened, 'unopened-renamed', { revision: sha('Not opened\n'), disk_revision: sha('Not opened\n') });
    assert(denied.status >= 400 || denied.body.ok === false); assert.equal(fs.existsSync(unopened), true);
    assert.equal(fs.existsSync(file('unopened-renamed.md')), false);
    report.checks.push('A request cannot move an unopened document merely by knowing its path');
    phase = 'clean opened document rename, immediate save, old authorization';
    const old = file('clean.md'), target = file('renamed.md'), original = '# Synthetic\nFirst paragraph.\n'; fs.writeFileSync(old, original, { flag: 'wx' });
    const descriptor = await get(old); assert.equal(descriptor.status, 200); assert(descriptor.body.revision);
    // The second process uses an ancestor of the profile as its workspace;
    // profile-managed files must still be excluded from document rename.
    secondKernel = await startKernel(args.bin, profile, args.evidence); report.kernelPids.push(secondKernel.pid);
    const secondOpen = await secondKernel.api('/api/file?p=' + encodeURIComponent(old)); assert.equal(secondOpen.status, 200);
    const operationId = callId();
    assert.match(descriptor.body.disk_revision, /^[0-9a-f]{64}$/);
    const moved = await rename(old, 'renamed', descriptor.body, undefined, operationId); assert.equal(moved.body.ok, true);
    assert.equal(fs.existsSync(old), false); assert.equal(fs.readFileSync(target, 'utf8'), original);
    assert(moved.body.review?.document_id);
    const edited = await api('/api/review', { action: 'edit', ...cas(moved.body.review), origin: 'editor',
      edits: [{ from: original.length, to: original.length, insert: 'Second paragraph.\n' }] });
    assert.equal(edited.status, 200);
    const saved = await api('/api/save', { path: target, content: original + 'Second paragraph.\n', encoding: 'utf-8',
      expected_revision: descriptor.body.revision, review_session: cas(edited.body) });
    report.clean = { renameOk: moved.body.ok, newSaveStatus: saved.status };
    assert.equal(saved.status, 200, 'Renamed file must save without an extra open');
    assert.equal(moved.body.state, 'committed');
    assert.equal(fs.readFileSync(target, 'utf8'), original + 'Second paragraph.\n');
    const stale = await api('/api/save', { path: old, content: 'stale writer', encoding: 'utf-8' });
    assert(stale.status >= 400 || stale.body.ok === false, 'Old authorization must not recreate the source'); assert.equal(fs.existsSync(old), false);
    const otherStale = await secondKernel.api('/api/save', { method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: old, content: 'stale second process', encoding: 'utf-8' }) });
    assert(otherStale.status >= 400 || (await otherStale.json()).ok === false, 'A second process cannot recreate the renamed source');
    assert.equal(fs.existsSync(old), false);
    const repeated = await rename(old, 'renamed', descriptor.body, undefined, operationId);
    assert.equal(repeated.body.ok, true); assert.equal(repeated.body.state, 'committed');
    const changedIntent = await rename(old, 'different', descriptor.body, undefined, operationId);
    assert(changedIntent.status >= 400 || changedIntent.body.ok === false, 'The same operation id cannot authorize another target');
    const newReview = await api('/api/review', { action: 'open', path: target }); assert.equal(newReview.status, 200);
    report.checks.push('Opened clean document saves immediately; stale authorization in two processes cannot recreate its source; the operation id is bound to one move');

    phase = 'target conflict and external source revision';
    const conflict = file('conflict.md'), existing = file('occupied.md'); fs.writeFileSync(conflict, 'Source\n'); fs.writeFileSync(existing, 'Third party\n');
    const conflictDoc = await get(conflict), occupied = await rename(conflict, 'occupied', conflictDoc.body);
    assert.equal(occupied.body.ok, false); assert.equal(fs.readFileSync(conflict, 'utf8'), 'Source\n'); assert.equal(fs.readFileSync(existing, 'utf8'), 'Third party\n');
    fs.writeFileSync(conflict, 'External change\n'); const changed = await rename(conflict, 'changed', conflictDoc.body);
    assert.equal(changed.body.ok, false); assert.equal(fs.readFileSync(conflict, 'utf8'), 'External change\n'); assert.equal(fs.existsSync(file('changed.md')), false);
    const unconfirmedSave = await api('/api/save', { path: conflict, content: 'Unconfirmed overwrite\n', encoding: 'utf-8' });
    assert.equal(unconfirmedSave.status, 409); assert.equal(fs.readFileSync(conflict, 'utf8'), 'External change\n');
    const confirmedSave = await api('/api/save', { path: conflict, content: 'Explicitly confirmed\n', encoding: 'utf-8', expected_revision: sha('External change\n') });
    assert.equal(confirmedSave.status, 200); assert.equal(fs.readFileSync(conflict, 'utf8'), 'Explicitly confirmed\n');
    const replaced = file('replaced.md'), relocated = file('replaced-original.md');
    fs.writeFileSync(replaced, 'Same bytes\n', { flag: 'wx' }); const replacedOpen = await get(replaced); assert.equal(replacedOpen.status, 200);
    fs.renameSync(replaced, relocated); fs.writeFileSync(replaced, 'Same bytes\n', { flag: 'wx' });
    const staleIdentity = await api('/api/save', { path: replaced, content: 'Must not overwrite\n', encoding: 'utf-8', expected_revision: replacedOpen.body.revision });
    assert(staleIdentity.status >= 400 || staleIdentity.body.ok === false);
    assert.equal(fs.readFileSync(replaced, 'utf8'), 'Same bytes\n'); assert.equal(fs.readFileSync(relocated, 'utf8'), 'Same bytes\n');
    report.checks.push('Conflicts preserve both files; only an explicit matching current revision permits saving an externally changed document');

    phase = 'profile-managed files cannot be renamed via an ancestor workspace';
    const managed = path.join(profile, 'capabilities'); fs.mkdirSync(managed, { recursive: true });
    const managedDoc = path.join(managed, 'managed.txt'); fs.writeFileSync(managedDoc, 'Synthetic managed file\n', { flag: 'wx' });
    const managedOpen = await secondKernel.api('/api/file?p=' + encodeURIComponent(managedDoc)); assert.equal(managedOpen.status, 200);
    const managedDescriptor = await managedOpen.json();
    const managedResponse = await secondKernel.api('/api/rename', { method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: managedDoc, new_stem: 'moved', operation_id: callId(),
        expected_revision: managedDescriptor.revision, expected_disk_revision: managedDescriptor.disk_revision }) });
    const managedResult = await managedResponse.json();
    assert.equal(managedResult.ok, false); assert.equal(fs.readFileSync(managedDoc, 'utf8'), 'Synthetic managed file\n');
    assert.equal(fs.existsSync(path.join(managed, 'moved.txt')), false);
    report.checks.push('A workspace containing the profile cannot authorize rename of managed capability files');

    phase = 'redirected parent must not rename its real target';
    const realDir = file('real-dir'), aliasDir = file('alias-dir'); fs.mkdirSync(realDir);
    const realDoc = path.join(realDir, 'linked.md'), aliasDoc = path.join(aliasDir, 'linked.md');
    fs.writeFileSync(realDoc, 'Synthetic linked content\n', { flag: 'wx' });
    fs.symlinkSync(realDir, aliasDir, process.platform === 'win32' ? 'junction' : 'dir');
    const aliasOpen = await get(aliasDoc); assert.equal(aliasOpen.status, 200);
    const aliasMove = await rename(aliasDoc, 'should-not-move', aliasOpen.body);
    assert.equal(aliasMove.body.ok, false); assert.equal(fs.readFileSync(realDoc, 'utf8'), 'Synthetic linked content\n');
    assert.equal(fs.existsSync(path.join(realDir, 'should-not-move.md')), false);
    report.checks.push('A redirected source parent is rejected before moving its real target');

    phase = 'abandoned operation cannot be revived by delayed retry';
    const abandonedPath = file('abandoned.md'); fs.writeFileSync(abandonedPath, 'Never moved\n', { flag: 'wx' });
    const abandonedDoc = await get(abandonedPath); assert.equal(abandonedDoc.status, 200); const abandonedId = callId();
    const abandoned = await api('/api/rename/abandon', { operation_id: abandonedId });
    assert.equal(abandoned.body.state, 'abandoned'); assert.equal(abandoned.body.operation_id, abandonedId);
    const delayed = await rename(abandonedPath, 'delayed', abandonedDoc.body, undefined, abandonedId);
    assert.equal(delayed.body.state, 'abandoned'); assert.equal(fs.readFileSync(abandonedPath, 'utf8'), 'Never moved\n');
    assert.equal(fs.existsSync(file('delayed.md')), false);
    report.checks.push('Durably abandoning an unstarted operation prevents a delayed retry from moving the document');

    phase = 'saved review, pending change, rename and withdraw';
    const reviewOld = file('review.md'), reviewNew = file('review-renamed.md'), initial = '# Original\n\nTail.\n';
    fs.writeFileSync(reviewOld, initial, { flag: 'wx' }); const beforeDoc = await get(reviewOld);
    const opened = await api('/api/review', { action: 'open', path: reviewOld }); assert.equal(opened.status, 200);
    const proposed = await api('/api/review', { action: 'propose', ...cas(opened.body), origin: 'ai', task_id: 'synthetic-task', edits: [{ from: 2, to: 10, insert: 'Edited' }] });
    assert.equal(proposed.status, 200); assert.equal(proposed.body.changes.length, 1); assert.equal(proposed.body.changes[0].decision, 'pending');
    const saveReview = await api('/api/save', { path: reviewOld, content: proposed.body.content, encoding: 'utf-8', expected_revision: beforeDoc.body.revision, review_session: cas(proposed.body) });
    assert.equal(saveReview.status, 200); const before = saveReview.body.review; assert.equal(before.document_id, opened.body.document_id);
    const reloaded = await get(reviewOld); const reviewMove = await rename(reviewOld, 'review-renamed', reloaded.body, cas(before));
    assert.equal(reviewMove.body.ok, true); assert.equal(reviewMove.body.state, 'committed');
    const after = reviewMove.body.review; assert.equal(after.document_id, before.document_id); assert.equal(after.content, before.content);
    assert.deepEqual(after.changes, before.changes); assert.equal(after.generation, before.generation); assert.equal(after.review_revision, before.review_revision);
    assert.equal(fs.existsSync(reviewOld), false); assert.equal(fs.readFileSync(reviewNew, 'utf8'), before.content);
    const read = await api('/api/review', { action: 'read', document_id: before.document_id }); assert.equal(read.status, 200); assert.equal(read.body.document_id, before.document_id);
    const withdrawn = await api('/api/review', { action: 'resolve', ...cas(read.body), decision: 'withdraw', ids: [read.body.changes[0].id] });
    assert.equal(withdrawn.status, 200); assert.equal(withdrawn.body.content, initial);
    const undone = await api('/api/review', { action: 'undo', ...cas(withdrawn.body) }); assert.equal(undone.status, 200); assert.equal(undone.body.content, before.content);
    report.checks.push('Saved review keeps document identity and pending original through rename, withdraw, and undo');

    phase = 'fresh process reopen';
    await secondKernel.stop(); secondKernel = undefined;
    await kernel.stop(); kernel = await startKernel(args.bin, profile, documents); report.kernelPids.push(kernel.pid); assert.notEqual(report.kernelPids[0], kernel.pid);
    const afterAbandonRestart = await api('/api/rename/status', { operation_id: abandonedId });
    assert.equal(afterAbandonRestart.body.state, 'abandoned');
    const afterRestart = await get(reviewNew); assert.equal(afterRestart.status, 200);
    const resumed = await api('/api/review', { action: 'resume', path: reviewNew }); assert.equal(resumed.status, 200); assert.equal(resumed.body.found, true);
    assert.equal(resumed.body.snapshot.document_id, before.document_id); assert.equal(resumed.body.snapshot.content, before.content);
    assert.equal(fs.existsSync(reviewOld), false); report.checks.push('A new kernel process reopens the renamed file and the same durable review');
    report.passed = true;
  } catch (error) {
    report.passed = false; report.failedStage = phase; report.failure = { name: error.name, message: error.message.slice(0, 600) }; process.exitCode = 1;
  } finally { if (secondKernel) await secondKernel.stop(); if (kernel) await kernel.stop(); fs.writeFileSync(path.join(args.evidence, 'report.json'), JSON.stringify(report, null, 2)); }
  console.log(JSON.stringify({ passed: report.passed, checks: report.checks.length, failedStage: report.failedStage, clean: report.clean, failure: report.failure }));
}
if (require.main === module) main().catch(error => { console.error(error.message); process.exitCode = 1; });
module.exports = { startKernel };
