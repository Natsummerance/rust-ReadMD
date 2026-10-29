'use strict';
// Production session/tab/reader code with deterministic HTTP/DOM doubles.
// The established harness is evaluated only through its helper definitions;
// its tests are not registered here. No native durability or browser claim.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { createRequire } = require('node:module');
const { createHash } = require('node:crypto');
const harnessPath = path.join(__dirname, 'session-cas-adapter.test.cjs');
const harnessSource = fs.readFileSync(harnessPath, 'utf8');
const boundary = harnessSource.indexOf('\nscenario(');
assert(boundary > 0, 'The existing session harness has an explicit helper/test boundary');
const startup = 'window.__readmdSessionReady = window.ReadMDSession.restore();';
assert(harnessSource.slice(0, boundary).includes(startup), 'The existing harness restore entry is recognized');
const prefix = harnessSource.slice(0, boundary).replace(startup, `
  window.ReadMDRefactor = options.refactor || null;
  if (options.askChoice) context.askChoice = options.askChoice;
  if (options.initialOpen) context.openInitialFile = options.initialOpen;
  window.__readmdSessionReady = window.ReadMDSession.restore(options.initialFile);
`) + '\nmodule.exports = { app, backend, descriptor, session, A, B, BODY, REVISION, turn };';
const fixture = { exports: {} };
vm.runInNewContext(prefix, { require: createRequire(harnessPath), module: fixture, __dirname,
  setImmediate, console, URL, TextEncoder }, { filename: harnessPath });
const { app, backend, descriptor, session, A, B, BODY, REVISION, turn } = fixture.exports;
const C = 'C:/Synthetic/Session/Reference-C.md', NEW = 'C:/Synthetic/Session/Renamed.md';
const clone = value => value === undefined ? undefined : JSON.parse(JSON.stringify(value));
const sha = value => createHash('sha256').update(value).digest('hex');
const scenario = (name, run) => test('session refactor: ' + name, { timeout: 6000 }, run);
function deferred() { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; }
function intent(extra = {}) {
  return { operation_id: 'refactor-synthetic-session', old_path: A, new_stem: 'Renamed', expected_revision: REVISION,
    expected_disk_revision: REVISION, refactor: true, plan_hash: sha('synthetic-plan'), workspace_root: 'C:/Synthetic/Session',
    review_session: { document_id: 'synthetic-source', generation: 3, review_revision: 7, call_id: 'synthetic-review-call' }, ...extra };
}
function savedSession(pending = intent()) {
  // Reference descriptors precede the source to exercise the serial prepass.
  return session([descriptor(B), descriptor(C), descriptor(A, { renamePending: pending, revision: REVISION })]);
}
function refactorStub(options = {}) {
  const calls = [], finishes = [], idle = new Set();
  let occupied = false, restores = 0;
  const service = { calls, finishes,
    busy: () => occupied,
    whenIdle: () => occupied ? new Promise(resolve => idle.add(resolve)) : Promise.resolve(),
    setBusy(value) { occupied = value; if (!value) { for (const resolve of idle) resolve(); idle.clear(); } },
    async restoreDescriptor(item) {
      restores++; service.setBusy(true); calls.push({ kind: 'status:start', descriptor: clone(item), attempt: restores });
      options.timeline?.push('status:start');
      if (options.hold) { options.hold.entered.resolve(); await options.hold.release.promise; }
      calls.push({ kind: 'status:end', attempt: restores }); options.timeline?.push('status:end');
      if (options.unsettled === true || options.unsettled === restores) return { descriptor: item, file: null, settled: false };
      const file = { path: NEW, dir: 'C:/Synthetic/Session', name: 'Renamed.md', original: BODY, content: BODY, revision: REVISION, disk_revision: REVISION };
      return { descriptor: { ...item, path: NEW, revision: REVISION, renamePending: null }, file,
        receipt: { state: 'committed', operation_id: item.renamePending.operation_id, plan_hash: item.renamePending.plan_hash }, settled: true };
    },
    finishRestore(saved) { finishes.push(saved); options.timeline?.push('finish:' + saved); options.onFinish?.(saved); if (saved) service.setBusy(false); },
    suspendRestore() { calls.push({ kind: 'suspend' }); options.timeline?.push('suspend'); service.setBusy(false); return true; },
  };
  return service;
}

scenario('pending refactor metadata is persisted through the exact allowlist without document bodies', async () => {
  const a = await app(), pending = intent(), injected = { ...pending, before: BODY, after: 'Synthetic private draft', token: 'SYNTHETIC_NOT_A_REAL_TOKEN' };
  a.first._renamePending = injected;
  assert.equal(await a.window.ReadMDSession.flush(), true);
  const saved = a.server.value.tabs.find(row => row.path === A).renamePending;
  assert.deepEqual(clone(saved), pending);
  const serialized = JSON.stringify(a.posts()[0].body);
  assert(!serialized.includes('Synthetic private draft')); assert(!serialized.includes('SYNTHETIC_NOT_A_REAL_TOKEN'));
  assert.equal(a.renameCalls.length, 0);
});

for (const malformed of ['missing plan', 'missing root', 'invalid plan', 'invalid root']) scenario(malformed + ' cannot be downgraded into a legacy one-file replay', async () => {
  const pending = intent();
  if (malformed === 'missing plan') delete pending.plan_hash;
  if (malformed === 'missing root') delete pending.workspace_root;
  if (malformed === 'invalid plan') pending.plan_hash = 'not-a-sha256';
  if (malformed === 'invalid root') pending.workspace_root = 4;
  const original = savedSession(pending), server = backend(original), refactor = refactorStub();
  const a = await app({ server, cold: true, refactor, recoverRename: true, askChoice: async () => 'cancel' });
  assert.equal(refactor.calls.length, 0); assert.equal(a.renameCalls.length, 0); assert.equal(a.state.tabs.length, 0);
  assert.equal(await a.window.ReadMDSession.flush(), false); assert.equal(a.posts().length, 0); assert.deepEqual(clone(server.value), clone(original));
});

scenario('startup finishes the group status prepass before any reference GET', async () => {
  const timeline = [], hold = { entered: deferred(), release: deferred() }, original = savedSession(), server = backend(original);
  const refactor = refactorStub({ timeline, hold });
  const a = await app({ server, cold: true, refactor, deferRestore: true, fileRead: async at => timeline.push('file:' + at) });
  await hold.entered.promise;
  assert.deepEqual(clone(a.fileCalls), []); assert.equal(a.state.tabs.length, 0);
  assert.deepEqual(refactor.calls[0].descriptor.renamePending, intent());
  hold.release.resolve(); await a.window.__readmdSessionReady;
  assert(timeline.indexOf('status:end') < timeline.indexOf('file:' + B));
  assert(timeline.indexOf('status:end') < timeline.indexOf('file:' + C));
  assert.equal(a.fileCalls.includes(A), false, 'The resolved source uses the already verified file, without reopening its obsolete name');
  assert.deepEqual(clone(a.state.tabs.map(tab => tab.path).sort()), [B, C, NEW].sort());
  assert.deepEqual(refactor.finishes, [true]); assert.equal(refactor.busy(), false);
});

scenario('unresolved startup cancellation retains every authoritative descriptor and cannot CAS an empty workspace', async () => {
  const original = savedSession(), server = backend(original), refactor = refactorStub({ unsettled: true });
  const choices = [];
  const a = await app({ server, cold: true, refactor, askChoice: async (title, description, buttons) => { choices.push({ title, description, buttons }); return 'cancel'; } });
  assert.equal(choices.length, 1); assert.equal(choices[0].description, 'error.review_needs_recovery');
  assert.deepEqual(clone(a.fileCalls), []); assert.equal(a.state.tabs.length, 0); assert.equal(refactor.busy(), false);
  assert.deepEqual(clone(server.value), clone(original)); assert.equal(a.posts().length, 0);
  a.window.ReadMDSession.persist(); await a.settled(); assert.equal(await a.window.ReadMDSession.flush(), false);
  assert.equal(a.posts().length, 0); assert.deepEqual(clone(server.value), clone(original));
  assert.equal(refactor.calls.at(-1).kind, 'suspend');
});

scenario('Retry reconciles the same descriptor before reference reads and then publishes the full session', async () => {
  const timeline = [], server = backend(savedSession()), refactor = refactorStub({ unsettled: 1, timeline });
  const a = await app({ server, cold: true, refactor, askChoice: async () => 'retry', fileRead: async at => timeline.push('file:' + at) });
  const statuses = refactor.calls.filter(call => call.kind === 'status:start');
  assert.equal(statuses.length, 2); assert.deepEqual(statuses[0].descriptor, statuses[1].descriptor);
  assert(timeline.lastIndexOf('status:end') < timeline.indexOf('file:' + B));
  assert.equal(a.state.tabs.length, 3); assert.equal(server.value.tabs.length, 3); assert.deepEqual(refactor.finishes, [true]);
});

scenario('the final exact session CAS is acknowledged before finishRestore releases the group', async () => {
  const timeline = [], server = backend(savedSession());
  const originalRequest = server.request;
  server.request = async body => { const value = await originalRequest(body); if (body) timeline.push('session:ack'); return value; };
  const refactor = refactorStub({ timeline, onFinish(saved) {
    if (saved) { assert.equal(server.value.tabs.length, 3); assert(server.value.tabs.every(row => !row.renamePending)); }
  } });
  const a = await app({ server, cold: true, refactor });
  assert(timeline.indexOf('session:ack') < timeline.indexOf('finish:true'));
  assert.equal(a.state.tabs.find(tab => tab.path === NEW)._renamePending, undefined);
  assert.equal(a.state.tabs.find(tab => tab.path === NEW)._renameInFlight, undefined);
});

scenario('a failed final session CAS retains all restored tabs and the original pending operation', async () => {
  const original = savedSession(), server = backend(original); server.fail = () => true;
  const refactor = refactorStub(), a = await app({ server, cold: true, refactor });
  assert.equal(a.state.tabs.length, 3); assert.deepEqual(clone(server.value), clone(original));
  const source = a.state.tabs.find(tab => tab.path === NEW);
  assert.deepEqual(clone(source._renamePending), intent()); assert.deepEqual(refactor.finishes, [false]);
  assert.equal(refactor.busy(), true); assert.equal(a.window.ReadMDSession.busy(), true);
  assert(a.state.tabs.some(tab => tab.path === B)); assert(a.state.tabs.some(tab => tab.path === C));
});

for (const kind of ['call', 'session', 'epoch']) scenario('an unbound ' + kind + ' final CAS receipt cannot release the recovered group', async () => {
  const server = backend(savedSession());
  server.receipt = receipt => {
    if (kind === 'call') receipt.call_id += '-synthetic-foreign';
    if (kind === 'session') receipt.session.folder = 'C:/Synthetic/Foreign';
    if (kind === 'epoch') receipt.session_version.epoch = '00000000-0000-4000-8000-000000000001';
    return receipt;
  };
  const refactor = refactorStub(), a = await app({ server, cold: true, refactor });
  assert.equal(a.state.tabs.length, 3); assert.deepEqual(clone(a.state.tabs.find(tab => tab.path === NEW)._renamePending), intent());
  assert.deepEqual(refactor.finishes, [false]); assert.equal(refactor.busy(), true);
});

scenario('an explicit initial file is deferred until final group settlement without blocking restore completion', async () => {
  const timeline = [], server = backend(savedSession()), refactor = refactorStub({ timeline }), initial = 'C:/Synthetic/Session/Initial.md';
  let opened = 0;
  const a = await app({ server, cold: true, refactor, initialFile: initial, initialOpen: async at => {
    assert.equal(at, initial); assert.equal(refactor.busy(), false); opened++; timeline.push('initial:open');
  } });
  await a.settled();
  assert.equal(opened, 1); assert(timeline.indexOf('finish:true') < timeline.indexOf('initial:open'));
  assert.equal(a.state.tabs.length, 3);
});

scenario('single/bulk/forced close and switching are fenced while a group owns the session', async () => {
  const refactor = refactorStub(), a = await app({ refactor }); refactor.setBusy(true);
  const originalTabs = [...a.state.tabs], active = a.state.activeTabId;
  await a.context.closeTab(a.first.id, true); await a.context.closeOtherTabs(a.first.id); await a.context.closeAllTabs(); await a.context.switchTab(a.second.id);
  assert.equal(a.state.tabs.length, originalTabs.length); assert(a.state.tabs.every((tab, index) => tab === originalTabs[index]));
  assert.equal(a.state.activeTabId, active); assert.equal(a.posts().length, 0);
  let idle = false; const waiting = a.window.ReadMDSession.whenIdle().then(() => { idle = true; });
  await turn(); assert.equal(idle, false); refactor.setBusy(false); await waiting; assert.equal(idle, true);
});

scenario('a new load waits for the group fence and delivers the document after release', async () => {
  const refactor = refactorStub(), a = await app({ refactor, reader: true }), incoming = 'C:/Synthetic/Session/Incoming.md';
  refactor.setBusy(true); const opening = a.context.loadFile(incoming); await turn();
  assert.deepEqual(clone(a.fileCalls), []); assert.equal(a.state.tabs.length, 2);
  refactor.setBusy(false); await opening;
  assert(a.state.tabs.some(tab => tab.path === incoming)); assert.equal(a.fileCalls.filter(at => at === incoming).length, 1);
});

scenario('a response issued before group ownership is re-read after release and cannot apply while fenced', async () => {
  const entered = deferred(), release = deferred(), refactor = refactorStub(), incoming = 'C:/Synthetic/Session/Late.md'; let held = false;
  const a = await app({ refactor, reader: true, fileRead: async at => {
    if (at === incoming && !held) { held = true; entered.resolve(); await release.promise; }
  } });
  const opening = a.context.loadFile(incoming); await entered.promise; refactor.setBusy(true); release.resolve();
  await turn(); assert.equal(a.state.tabs.some(tab => tab.path === incoming), false);
  refactor.setBusy(false); await opening;
  assert.equal(a.fileCalls.filter(at => at === incoming).length, 2); assert(a.state.tabs.some(tab => tab.path === incoming));
});
