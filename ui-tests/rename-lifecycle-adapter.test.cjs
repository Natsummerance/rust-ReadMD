'use strict';
// Frontend protocol tests only. The production adapters run unchanged in a VM;
// the DOM, bridge, file/review/task service and native settings are doubles.
// No filesystem move, Rust persistence, CSP, browser or native-window claim.
// All paths, document text and privacy sentinels below are synthetic.
// First direct run (2026-10-27): 33/36 passed. The pending-save assertion
// returned a CAS object instead of false; two cold tests stopped at a missing
// rendering stub and were not product failures. After that stub was supplied,
// a second run returned true for cold recovery of a foreign review document.
// The 41-case run then passed 39 and exposed both warm/cold finalizers clearing
// pending after a raw-byte revision changed while decoded content stayed equal.
// The 58-case loadFile run passed 57: a response issued before rename, delivered
// after commit cleared its flags, restored the old path/title on the same tab.
// The coordinator owns product fixes; all safety assertions remain live here.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { createHash } = require('node:crypto');
const sources = Object.fromEntries([
  ['tabs', 'assets/js/core/tabs.js'],
  ['review', 'assets/js/editor/review.js'],
  ['ai', 'assets/js/features/ai-harness.js'],
  ['history', 'assets/js/features/document-history.js'],
  ['reader', 'assets/js/reader/render.js'],
].map(([key, relative]) => [key, fs.readFileSync(path.join(__dirname, '..', relative), 'utf8')]));
const readerLoadSource = sources.reader.slice(sources.reader.indexOf('async function waitForSessionTransition('), sources.reader.indexOf('\nasync function openExternalFile('));
const OLD = 'C:/Synthetic/ReadMD-Rename/Original.md';
const NEW = 'C:/Synthetic/ReadMD-Rename/Renamed.md';
const OTHER = 'C:/Synthetic/ReadMD-Rename/Other.md';
const BODY = '# Synthetic rename fixture\nKeep this text and review history.\n';
const PRIVATE = 'SYNTHETIC_PRIVATE_SENTINEL_NOT_A_REAL_CREDENTIAL';
const clone = value => value === undefined ? undefined : JSON.parse(JSON.stringify(value));
const hash = value => createHash('sha256').update(value).digest('hex');
const REVISION = hash(BODY);
const DISK_REVISION = hash('\uFEFF' + BODY.replace(/\n/g, '\r\n'));
const normalizePath = value => String(value || '').replace(/\\/g, '/').toLowerCase();
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
const scenario = (name, work) => test('rename adapter: ' + name, { timeout: 5000 }, work);
function file(at, text = BODY) {
  return { path: at, dir: at.slice(0, at.lastIndexOf('/')), name: at.slice(at.lastIndexOf('/') + 1),
    original: text, content: text, revision: hash(text), disk_revision: text === BODY ? DISK_REVISION : hash(text),
    mtime: 23, size: text.length, encoding: 'utf-8-sig', ext: '.md', fixes: [], stats: {} };
}
function review(at = OLD, documentId = 'synthetic-document-a') {
  return { document_id: documentId, path: at, content: BODY, revision: REVISION, disk_revision: DISK_REVISION,
    generation: 3, review_revision: 7, last_event_id: 11, can_undo: true, can_redo: true,
    checkpoint: { generation: 2, review_revision: 4 },
    changes: [{ id: 'synthetic-change', from: 2, to: 11, original: 'Synthetic', replacement: 'Synthetic', decision: 'accepted', version: 1 }] };
}
function backend() {
  const files = new Map([[OLD, file(OLD)], [OTHER, file(OTHER, '# Other synthetic document\n')]]);
  const snapshots = new Map([['synthetic-document-a', review()]]);
  const receipts = new Map(), tasks = new Map(), history = new Map(), calls = [], timeline = [], sessionReceipts = new Map();
  let sessionVersion = { epoch: '00000000-0000-4000-8000-000000000001', generation: '0' };
  let settings = null;
  function atPath(at) { return [...snapshots.values()].find(item => normalizePath(item.path) === normalizePath(at)); }
  function commit(pending) {
    const source = files.get(pending.old_path);
    assert.ok(source, 'The synthetic source exists at the bridge mutation boundary');
    const destination = source.dir + '/' + pending.new_stem + '.md';
    assert.equal(files.has(destination), false, 'The protocol double does not overwrite targets');
    files.delete(pending.old_path); files.set(destination, { ...clone(source), path: destination, name: destination.slice(destination.lastIndexOf('/') + 1) });
    const snapshot = atPath(pending.old_path); assert.ok(snapshot); snapshot.path = destination;
    const receipt = { ok: true, operation_id: pending.operation_id, state: 'committed', path: destination, review: clone(snapshot) };
    receipts.set(pending.operation_id, clone(receipt)); timeline.push('service:committed'); return receipt;
  }
  return { files, snapshots, receipts, tasks, history, calls, timeline, atPath, commit, sessionReceipts,
    get sessionVersion() { return clone(sessionVersion); }, set sessionVersion(value) { sessionVersion = clone(value); },
    get settings() { return clone(settings); }, set settings(value) { settings = clone(value); } };
}
class Element {
  constructor() { this.dataset = {}; }
}
async function app(options = {}) {
  const server = options.server || backend(), toasts = [], titleCalls = [], eventListeners = new Map(), timers = new Map(), local = new Map();
  const initialCallIndex = server.calls.length;
  let serial = 0, saving = false;
  const tab = { id: 'synthetic-tab-a', mode: 'file', source: 'file', path: OLD, dir: file(OLD).dir,
    name: 'Original.md', title: 'Original.md', original: BODY, content: BODY, fixed: BODY,
    revision: REVISION, encoding: 'utf-8-sig', mtime: 23, originPath: OLD, isDirty: false, editGeneration: 0 };
  const other = { id: 'synthetic-tab-b', mode: 'file', source: 'file', path: OTHER, dir: file(OTHER).dir,
    name: 'Other.md', title: 'Other.md', original: '# Other synthetic document\n', content: '# Other synthetic document\n',
    revision: hash('# Other synthetic document\n'), isDirty: false };
  const state = { tabs: options.cold ? [] : [tab, other], activeTabId: options.cold ? null : tab.id,
    editing: false, mode: options.cold ? 'welcome' : 'file', file: options.cold ? '' : OLD,
    sourceName: 'Original.md', revision: REVISION, original: BODY, fixed: BODY, folder: 'C:/Synthetic/ReadMD-Rename',
    pagination: { enabled: false, mode: 'paged' }, scrollPos: {}, ai: { config: { privateCredential: PRIVATE } } };
  const listen = (type, callback) => { if (!eventListeners.has(type)) eventListeners.set(type, []); eventListeners.get(type).push(callback); };
  const document = { readyState: 'loading', activeElement: null, hidden: false, title: 'Original.md - ReadMD',
    getElementById: () => null, querySelectorAll: () => [], addEventListener: listen,
    createElement() { throw new Error('Unexpected visible DOM creation in the minimal protocol fixture'); } };
  const window = { READMD_ENGINE: 'rust', __STARTUP_PROBE__: true, innerWidth: 420,
    crypto: { randomUUID: () => '00000000-0000-4000-8000-' + String(++serial).padStart(12, '0') },
    i18n: { currentLang: 'fr', t: (key, params) => key + (params ? ':' + JSON.stringify(params) : '') },
    addEventListener: listen, dispatchEvent(event) { for (const callback of eventListeners.get(event.type) || []) callback(event); },
    ReadMDRecovery: { flush: async () => options.recoveryFlush !== false, sessionDrafts: async () => new Map(), mathDrafts: async () => [] } };
  function record(kind, data) { const entry = { kind, ...clone(data) }; server.calls.push(entry); server.timeline.push(kind); return entry; }
  async function apiFetch(url, request) {
    if (url === '/api/session') {
      if (!request?.body) { record('settings:get'); return { ok: true, status: 200, json: async () => ({ ok: true,
        protocol: 'readmd-session-cas-v1', session: server.settings, session_version: server.sessionVersion }) }; }
      const body = JSON.parse(request.body), encoded = JSON.stringify(body);
      record('settings:write', { value: { session: body.session }, body });
      const previous = server.sessionReceipts.get(body.call_id);
      if (previous) {
        assert.equal(encoded, previous.encoded, 'A replay retains the exact version and payload');
        return { ok: true, status: 200, json: async () => clone(previous.result) };
      }
      if (JSON.stringify(body.expected_session_version) !== JSON.stringify(server.sessionVersion))
        return { ok: false, status: 409, json: async () => ({ ok: false, error_code: 'session_conflict' }) };
      let result;
      try { result = options.saveSettings ? await options.saveSettings({ session: clone(body.session) }, server) : { ok: true, persisted: true }; }
      catch (_) { throw new Error('Synthetic session transport failure before the synthetic commit'); }
      if (result?.ok !== true || result.persisted !== true)
        return { ok: false, status: 507, json: async () => clone(result) };
      server.settings = body.session;
      server.sessionVersion = { ...server.sessionVersion, generation: String(BigInt(server.sessionVersion.generation) + 1n) };
      result = { ...result, protocol: 'readmd-session-cas-v1', call_id: body.call_id, session: clone(body.session), session_version: server.sessionVersion };
      server.sessionReceipts.set(body.call_id, { encoded, result: clone(result) }); server.timeline.push('settings:ack');
      return { ok: true, status: 200, json: async () => clone(result) };
    }
    if (url.startsWith('/api/file?')) {
      const query = new URL(url, 'https://synthetic.invalid').searchParams, at = query.get('p'), metadata = query.get('meta') === '1';
      record(metadata ? 'file:meta' : 'file:open', { path: at });
      if (options.fileRead) {
        const overridden = await options.fileRead({ path: at, metadata, server, tab, state });
        if (overridden !== undefined) return { ok: !!overridden, status: overridden ? 200 : 404, json: async () => clone(overridden) };
      }
      const value = server.files.get(at);
      return { ok: !!value, status: value ? 200 : 404, json: async () => clone(value || { ok: false }) };
    }
    const body = request?.body ? JSON.parse(request.body) : null;
    if (url === '/api/documents/history') {
      record('history:' + (body?.op || 'list'), { body });
      if (!body) return { ok: true, json: async () => ({ ok: true, entries: clone([...server.history.values()].map(row => row.entry)) }) };
      if (body.op === 'record') {
        const id = 'synthetic-history-' + (++serial), context = clone(body.context || {});
        if (body.kind === 'math_draft') { const payload = JSON.parse(body.content); context.math_slot = [payload.document_revision, payload.from, payload.to].join(':'); }
        const entry = { id, doc_key: body.key, path: body.path, name: body.name, kind: body.kind, context, created: serial };
        if (body.kind === 'draft') for (const [priorId, row] of server.history) if (row.entry.kind === 'draft' && row.entry.doc_key === body.key) server.history.delete(priorId);
        server.history.set(id, { entry, content: body.content }); return { ok: true, json: async () => ({ ok: true, entry: clone(entry) }) };
      }
      if (body.op === 'read') return { ok: true, json: async () => ({ ok: true, ...clone(server.history.get(body.id)) }) };
      if (body.op === 'clear_draft') {
        for (const [id, row] of server.history) if (row.entry.kind === 'draft' && row.entry.doc_key === body.key &&
            (body.content == null || row.content === body.content)) server.history.delete(id);
        return { ok: true, json: async () => ({ ok: true }) };
      }
      throw new Error('Unexpected synthetic history operation');
    }
    if (url === '/api/review') {
      record('review:' + body.action, { body });
      if (options.reviewRequest) { const overridden = await options.reviewRequest(body, server); if (overridden !== undefined) return overridden; }
      if (body.action === 'resume') { const snapshot = server.atPath(body.path); return { ok: true, json: async () => ({ found: !!snapshot, snapshot: clone(snapshot) }) }; }
      if (body.action === 'open') { const snapshot = server.atPath(body.path); assert.ok(snapshot); return { ok: true, json: async () => clone(snapshot) }; }
      if (body.action === 'read') return { ok: true, json: async () => clone(server.snapshots.get(body.document_id)) };
      throw new Error('Unexpected review mutation in a rename-only scenario: ' + body.action);
    }
    if (url === '/api/ai/tasks') {
      record('task:' + body.action, { body });
      if (options.taskRequest) { const overridden = await options.taskRequest(body, server); if (overridden !== undefined) return overridden; }
      if (body.action === 'list') return { ok: true, json: async () => ({ tasks: clone([...server.tasks.values()]) }) };
      if (body.action === 'start') {
        const task = { id: 'synthetic-task-' + serial, document_id: body.doc_id, state: 'running', rounds: 0, elapsed_ms: 0, last_event_id: 1, private_provider: PRIVATE };
        server.tasks.set(task.id, task); return { ok: true, json: async () => ({ task: clone(task) }) };
      }
      if (body.action === 'status') return { ok: true, json: async () => ({ task: clone(server.tasks.get(body.task_id)) }) };
      throw new Error('Rename must not pause, cancel or resume a synthetic AI job');
    }
    throw new Error('Unexpected synthetic API route');
  }
  const py = {
    async get_settings() { record('settings:get'); return { session: server.settings }; },
    async save_settings(value) {
      record('settings:write', { value });
      const result = options.saveSettings ? await options.saveSettings(clone(value), server) : { ok: true, persisted: true };
      if (result?.ok === true && result?.persisted === true) { server.settings = value.session; server.timeline.push('settings:ack'); }
      return result;
    },
    async rename_file(oldPath, stem, pending) {
      record('rename:request', { oldPath, stem, pending });
      const durable = server.settings?.tabs.find(item => item.path === oldPath)?.renamePending;
      assert.deepEqual(clone(durable), clone(pending), 'Exact operation metadata must have native settings acknowledgement before the bridge request');
      return options.rename ? options.rename(clone(pending), server) : server.commit(pending);
    },
    async rename_status(operationId) {
      record('rename:status', { operationId });
      return options.status ? options.status(operationId, server) : clone(server.receipts.get(operationId) || { state: 'not_found' });
    },
    async rename_abandon(operationId) {
      record('rename:abandon', { operationId });
      if (options.abandon) return options.abandon(operationId, server);
      const receipt = server.receipts.get(operationId) || { ok: false, state: 'abandoned', operation_id: operationId };
      server.receipts.set(operationId, clone(receipt)); return clone(receipt);
    },
  };
  const context = vm.createContext({ window, document, state, Element, TextEncoder, Uint32Array, crypto: window.crypto, hasPy: true, py,
    apiFetch, normalizePath, settingsSaveQueue: Promise.resolve(true), documentLoadEpoch: 0,
    localStorage: { getItem: key => local.get(key) || null, setItem: (key, value) => local.set(key, value) },
    CustomEvent: class { constructor(type, data) { this.type = type; Object.assign(this, data); } },
    setTimeout(callback, delay) { const id = ++serial; timers.set(id, { callback, delay }); return id; }, clearTimeout: id => timers.delete(id),
    requestAnimationFrame() {}, $: () => null,
    showToast: message => toasts.push(message), busy: value => { saving = value; }, setFileTitle: (...args) => titleCalls.push(args),
    getEditContent: () => state.tabs.find(item => item.id === state.activeTabId)?.content || '',
    syncActiveTabDirty() {}, updateUnloadGuard() {}, prepareDocCitations: async () => {}, renderPage() {},
    setFixes() {}, renderContent: async () => {}, updateStatus() {}, afterRender() {}, setProgress() {},
    beginDocumentLoad() { return ++context.documentLoadEpoch; }, isDocumentLoadCurrent: epoch => epoch === context.documentLoadEpoch,
    addRecent() {}, pushHistory() {}, saveLastFile() {}, clearAiOutput() {}, console: { error() {} },
    toggleEdit() { throw new Error('Native edit entry is outside this protocol fixture'); },
    resetHome() {}, openInitialFile: async () => {}, exitEdit() { state.editing = false; }, goHome: async () => {},
  });
  vm.runInContext(sources.review, context, { filename: 'assets/js/editor/review.js' });
  vm.runInContext(sources.ai, context, { filename: 'assets/js/features/ai-harness.js' });
  vm.runInContext(sources.tabs, context, { filename: 'assets/js/core/tabs.js' });
  vm.runInContext(readerLoadSource, context, { filename: 'assets/js/reader/render.js:loadFile' });
  if (options.realHistory) vm.runInContext(sources.history, context, { filename: 'assets/js/features/document-history.js' });
  window.__readmdSessionReady = window.ReadMDSession.restore();
  await window.__readmdSessionReady;
  if (!options.cold && options.ensureReview !== false) await window.ReadMDReview.ensure(tab);
  if (!options.cold && options.initialFlush !== false) assert.equal(await window.ReadMDSession.flush(), true);
  const startupCalls = clone(server.calls.slice(initialCallIndex));
  server.calls.length = 0; server.timeline.length = 0; toasts.length = 0;
  return { server, tab, other, state, window, context, document, py, toasts, titleCalls, local, timers, startupCalls,
    get busy() { return saving; }, calls: kind => server.calls.filter(call => call.kind === kind),
    rename: title => context.renameTab(tab.id, title || 'Renamed.md'), recover: owner => context.recoverPendingRename(owner || tab),
    async settled() { await Promise.resolve(); await context.settingsSaveQueue; await Promise.resolve(); } };
}
function unchanged(instance) {
  assert.equal(instance.tab.path, OLD); assert.equal(instance.tab.title, 'Original.md'); assert.equal(instance.tab.content, BODY);
  assert.equal(instance.state.file, OLD); assert.equal(instance.server.files.has(OLD), true); assert.equal(instance.server.files.has(NEW), false);
}
function pending(instance) {
  assert.ok(instance.tab._renamePending); assert.equal(instance.tab.path, OLD); assert.equal(instance.state.file, OLD);
  assert.equal(instance.window.ReadMDReview.snapshot(instance.tab).document_id, 'synthetic-document-a');
}
function noPrivate(value) { assert.equal(JSON.stringify(value).includes(PRIVATE), false, 'Synthetic private configuration must not escape into UI or persisted/request metadata'); }

for (const condition of ['active editor', 'dirty active file', 'dirty background file', 'review restore unresolved', 'already in flight']) {
  scenario('rejects ' + condition + ' before bridge or persistence work', async () => {
    const instance = await app();
    if (condition === 'active editor') instance.state.editing = true;
    if (condition.startsWith('dirty')) instance.tab.isDirty = true;
    if (condition === 'dirty background file') instance.state.activeTabId = instance.other.id;
    if (condition === 'review restore unresolved') instance.tab._reviewRestorePending = true;
    if (condition === 'already in flight') instance.tab._renameInFlight = true;
    await instance.rename(); unchanged(instance);
    assert.equal(instance.server.calls.length, 0); assert.equal(instance.toasts.length, 1);
  });
}

scenario('awaits exact intent persistence through production ReadMDSession.flush before any move request', async () => {
  const entered = deferred(), release = deferred();
  const instance = await app({ saveSettings: async value => { if (value.session.tabs.some(item => item.renamePending)) { entered.resolve(); await release.promise; } return { ok: true, persisted: true }; } });
  const work = instance.rename(); await entered.promise;
  try {
    assert.equal(instance.calls('rename:request').length, 0); assert.equal(instance.busy, true); pending(instance);
    const descriptor = JSON.parse(instance.local.get('readmd-session')).tabs.find(item => item.path === OLD);
    assert.match(descriptor.renamePending.operation_id, /^rename-[a-f0-9-]+$/);
    assert.equal(descriptor.renamePending.expected_revision, REVISION); assert.equal(descriptor.renamePending.expected_disk_revision, DISK_REVISION);
    assert.deepEqual(Object.keys(descriptor.renamePending.review_session).sort(), ['call_id', 'document_id', 'generation', 'review_revision']);
    noPrivate(descriptor); assert.equal(JSON.stringify(descriptor).includes(BODY), false, 'Session descriptors contain metadata, not document bodies');
  } finally { release.resolve(); }
  await work; await instance.settled();
  assert.ok(instance.server.timeline.indexOf('settings:ack') < instance.server.timeline.indexOf('rename:request'));
  assert.equal(instance.calls('rename:request').length, 1);
});

for (const mode of ['false', 'structured failure', 'missing persisted marker', 'throw', 'recovery flush failure']) {
  scenario('failed pre-request persistence (' + mode + ') sends no move and preserves source', async () => {
    const instance = await app({ initialFlush: false, recoveryFlush: mode !== 'recovery flush failure', saveSettings: async () => {
      if (mode === 'throw') throw new Error('synthetic_storage_failure');
      if (mode === 'missing persisted marker') return { ok: true };
      return mode === 'structured failure' ? { ok: false, error: 'synthetic_storage_failure' } : false;
    } });
    await instance.rename(); unchanged(instance);
    assert.equal(instance.calls('rename:request').length, 0);
    if (mode === 'recovery flush failure') assert.equal(instance.tab._renamePending, undefined);
    else assert.ok(instance.tab._renamePending, 'A failed session acknowledgement does not prove that its intent was never persisted');
    assert.equal(instance.tab._renameInFlight, undefined); assert.equal(instance.busy, false);
    const reservation = await instance.window.ReadMDReview.beginRename(instance.tab);
    if (mode === 'recovery flush failure') {
      assert.ok(reservation, 'A failure before the session request releases the review reservation');
      instance.window.ReadMDReview.endRename(instance.tab);
      assert.equal(await instance.window.ReadMDAiHarness.reserveTabForRename(instance.tab), true); instance.window.ReadMDAiHarness.releaseTab(instance.tab);
    } else {
      assert.equal(reservation, null, 'Uncertain persistence keeps the review reservation');
      assert.equal(await instance.window.ReadMDReview.prepareSave(instance.tab, BODY), false);
      await instance.context.closeTab(instance.tab.id, true); assert.ok(instance.state.tabs.includes(instance.tab));
    }
    assert.ok(instance.toasts.includes('review.persistenceFailed')); noPrivate(instance.toasts);
  });
}

for (const mode of ['decoded revision changed', 'raw revision absent', 'dirty during read', 'path changed during read']) {
  scenario('stale metadata preflight (' + mode + ') cannot send a move', async () => {
    const instance = await app({ fileRead: ({ metadata, server, tab }) => {
      if (!metadata) return undefined;
      if (mode === 'dirty during read') tab.isDirty = true;
      if (mode === 'path changed during read') tab.path = OTHER;
      const value = clone(server.files.get(OLD));
      if (mode === 'decoded revision changed') value.revision = hash('Different synthetic document');
      if (mode === 'raw revision absent') delete value.disk_revision;
      return value;
    } });
    await instance.rename();
    assert.equal(instance.calls('rename:request').length, 0); assert.equal(instance.server.files.has(OLD), true); assert.equal(instance.server.files.has(NEW), false);
    assert.equal(instance.tab._renamePending, undefined); assert.equal(instance.tab._renameInFlight, undefined); assert.equal(instance.busy, false);
  });
}

scenario('committed receipt reopens through normal file API and updates the same tab, active state and review identity', async () => {
  const instance = await app(), sameTab = instance.tab, sameOther = clone(instance.other), before = clone(instance.window.ReadMDReview.snapshot(instance.tab));
  await instance.rename(); await instance.settled();
  assert.equal(instance.state.tabs[0], sameTab); assert.equal(instance.tab.id, 'synthetic-tab-a'); assert.equal(instance.state.activeTabId, sameTab.id);
  assert.equal(instance.tab.path, NEW); assert.equal(instance.tab.originPath, NEW); assert.equal(instance.tab.name, 'Renamed.md'); assert.equal(instance.tab.title, 'Renamed.md');
  assert.equal(instance.tab.original, BODY); assert.equal(instance.tab.content, BODY); assert.equal(instance.tab.revision, REVISION);
  assert.equal(instance.state.file, NEW); assert.equal(instance.state.sourceName, 'Renamed.md'); assert.equal(instance.document.title, 'Renamed.md - ReadMD');
  assert.deepEqual(instance.titleCalls.at(-1), ['Renamed.md', true, NEW]); assert.deepEqual(clone(instance.other), sameOther);
  assert.deepEqual(clone(instance.window.ReadMDReview.snapshot(instance.tab)), { ...before, path: NEW });
  assert.equal(instance.tab.reviewSession.document_id, before.document_id); assert.equal(instance.tab._renamePending, undefined); assert.equal(instance.tab._renameInFlight, undefined);
  assert.equal(instance.server.files.has(OLD), false); assert.equal(instance.server.files.get(NEW).content, BODY);
  const moved = instance.server.timeline.indexOf('service:committed'), reopened = instance.server.timeline.indexOf('file:open'); assert.ok(moved < reopened);
  assert.equal(instance.calls('file:open')[0].path, NEW); assert.equal(instance.busy, false);
});

scenario('not_started rejection keeps file/tab/review unchanged and unlocks only after durable abandonment', async () => {
  const instance = await app({ rename: async () => ({ ok: false, state: 'not_started', error: PRIVATE }) });
  const original = clone(instance.window.ReadMDReview.snapshot(instance.tab)); await instance.rename(); unchanged(instance);
  assert.equal(instance.tab._renamePending, undefined); assert.equal(instance.tab._renameInFlight, undefined);
  assert.deepEqual(clone(instance.window.ReadMDReview.snapshot(instance.tab)), original); assert.equal(instance.calls('rename:request').length, 1);
  assert.equal(instance.calls('rename:status').length, 0); assert.equal(instance.calls('rename:abandon').length, 1);
  assert.equal(instance.calls('file:open')[0].path, OLD); noPrivate(instance.toasts);
  assert.equal(instance.server.receipts.get(instance.calls('rename:request')[0].pending.operation_id).state, 'abandoned');
});

scenario('cancelled receipt reopens the preserved source without changing durable review identity', async () => {
  const instance = await app({ rename: async (intent, server) => ({ operation_id: intent.operation_id, state: 'cancelled', path: OLD, review: clone(server.atPath(OLD)) }) });
  await instance.rename(); unchanged(instance); assert.equal(instance.tab._renamePending, undefined);
  assert.equal(instance.calls('file:open')[0].path, OLD); assert.equal(instance.tab.reviewSession.document_id, 'synthetic-document-a');
});

scenario('lost committed response retains the acknowledged pending intent and recovers by status plus authorized reopen', async () => {
  const instance = await app({ rename: async (intent, server) => { server.commit(intent); throw new Error(PRIVATE); } });
  await instance.rename(); pending(instance); noPrivate(instance.toasts);
  const retained = clone(instance.tab._renamePending), durable = instance.server.settings.tabs.find(item => item.path === OLD).renamePending;
  assert.deepEqual(durable, retained); assert.equal(instance.server.files.has(OLD), false); assert.equal(instance.server.files.has(NEW), true);
  assert.equal(await instance.recover(), true); await instance.settled();
  assert.equal(instance.tab.path, NEW); assert.equal(instance.tab._renamePending, undefined); assert.equal(instance.calls('rename:request').length, 1);
  assert.equal(instance.calls('rename:status')[0].operationId, retained.operation_id); assert.equal(instance.calls('file:open')[0].path, NEW);
  assert.equal(instance.tab.reviewSession.document_id, retained.review_session.document_id);
});

scenario('request never reaching service retries only the same durable operation after reopening the original', async () => {
  let attempts = 0;
  const instance = await app({ rename: async (intent, server) => { if (++attempts === 1) throw new Error('synthetic_response_lost'); return server.commit(intent); } });
  await instance.rename(); pending(instance); const retained = clone(instance.tab._renamePending);
  assert.equal(await instance.recover(), true); await instance.settled();
  const requests = instance.calls('rename:request'); assert.equal(requests.length, 2); assert.deepEqual(requests.map(call => call.pending), [retained, retained]);
  const opens = instance.calls('file:open'); assert.deepEqual(opens.map(call => call.path), [OLD, NEW]);
  assert.equal(instance.tab.path, NEW); assert.equal(instance.tab.reviewSession.document_id, retained.review_session.document_id);
});

scenario('not_found then retry not_started requires an acknowledged tombstone before unlocking', async () => {
  let attempts = 0;
  const instance = await app({ rename: async () => { if (++attempts === 1) throw new Error('synthetic_never_reached'); return { ok: false, state: 'not_started' }; } });
  await instance.rename(); pending(instance); const retained = clone(instance.tab._renamePending);
  assert.equal(await instance.recover(), true); unchanged(instance);
  assert.equal(instance.tab._renamePending, undefined); assert.equal(instance.tab._renameInFlight, undefined);
  assert.equal(instance.calls('rename:request').length, 2); assert.equal(instance.calls('rename:abandon')[0].operationId, retained.operation_id);
  assert.equal(instance.server.receipts.get(retained.operation_id).state, 'abandoned');
  assert.deepEqual(instance.calls('rename:request').map(call => call.pending), [retained, retained]);
  assert.ok(await instance.window.ReadMDReview.capture(instance.tab));
  assert.equal(await instance.window.ReadMDAiHarness.reserveTabForRename(instance.tab), true); instance.window.ReadMDAiHarness.releaseTab(instance.tab);
});

scenario('not_started without an abandonment bridge retains pending and keeps close/save blocked', async () => {
  const instance = await app({ rename: async () => ({ ok: false, state: 'not_started' }) }); delete instance.py.rename_abandon;
  await instance.rename(); pending(instance); unchanged(instance);
  await instance.context.closeTab(instance.tab.id, true); assert.equal(instance.state.tabs.includes(instance.tab), true);
  assert.equal(await instance.window.ReadMDReview.prepareSave(instance.tab, BODY), false);
});

scenario('abandon returning an already committed operation retains pending until destination reopen and later recovers safely', async () => {
  let accessible = false;
  const instance = await app({ rename: async () => ({ ok: false, state: 'not_started' }),
    abandon: async (id, server) => server.commit(server.calls.find(call => call.kind === 'rename:request' && call.pending.operation_id === id).pending),
    fileRead: ({ path: at, metadata }) => !metadata && at === NEW && !accessible ? null : undefined });
  await instance.rename(); pending(instance); const retained = clone(instance.tab._renamePending);
  assert.equal(instance.server.files.has(OLD), false); assert.equal(instance.server.files.get(NEW).content, BODY);
  assert.equal(instance.calls('rename:abandon').length, 1); assert.equal(instance.calls('file:open').some(call => call.path === OLD), false);
  accessible = true; assert.equal(await instance.recover(), true); await instance.settled();
  assert.equal(instance.tab.path, NEW); assert.equal(instance.tab._renamePending, undefined);
  assert.equal(instance.tab.reviewSession.document_id, retained.review_session.document_id); assert.equal(instance.calls('rename:request').length, 1);
});

for (const outcome of ['committed', 'abandoned']) {
  scenario(outcome + ' settlement stays pending when the resolved session cannot be durably saved', async () => {
    let failSettlement = false;
    const instance = await app({ rename: outcome === 'abandoned' ? async () => ({ ok: false, state: 'not_started' }) : undefined,
      saveSettings: async value => failSettlement && value.session.tabs.some(item =>
        item.key === 'file:synthetic-tab-a' && !item.renamePending) ? { ok: false, persisted: false } : { ok: true, persisted: true } });
    failSettlement = true; await instance.rename();
    const retained = clone(instance.calls('rename:request')[0].pending);
    assert.deepEqual(clone(instance.tab._renamePending), retained); assert.equal(instance.tab._renameInFlight, undefined);
    assert.equal(instance.tab.path, outcome === 'committed' ? NEW : OLD);
    assert.equal(instance.server.files.get(outcome === 'committed' ? NEW : OLD).content, BODY);
    const durable = instance.server.settings.tabs.find(item => item.key === instance.tab.recoveryKey);
    assert.deepEqual(durable.renamePending, retained, 'The last acknowledged native session retains the recoverable intent');
    assert.equal(await instance.window.ReadMDReview.prepareSave(instance.tab, BODY), false);
    await instance.context.closeTab(instance.tab.id, true); assert.equal(instance.state.tabs.includes(instance.tab), true);
    failSettlement = false; assert.equal(await instance.recover(), true); await instance.settled();
    assert.equal(instance.tab._renamePending, undefined); assert.equal(instance.calls('rename:request').length, 1);
    const resolved = instance.server.settings.tabs.find(item => item.key === instance.tab.recoveryKey);
    assert.equal(resolved.renamePending, null); assert.equal(resolved.path, outcome === 'committed' ? NEW : OLD);
  });
}

for (const change of ['decoded revision', 'raw revision']) {
  scenario('not_found recovery refuses replay after source ' + change + ' changed', async () => {
    const instance = await app({ rename: async () => { throw new Error('synthetic_never_sent'); } });
    await instance.rename(); const retained = clone(instance.tab._renamePending);
    instance.server.files.get(OLD)[change === 'raw revision' ? 'disk_revision' : 'revision'] = hash('Different synthetic bytes');
    assert.equal(await instance.recover(), false); assert.equal(instance.calls('rename:request').length, 1); pending(instance);
    assert.deepEqual(clone(instance.tab._renamePending), retained); assert.equal(instance.server.files.has(NEW), false);
  });
}

for (const mismatch of ['operation id', 'review document id', 'review generation', 'review content', 'reopened revision', 'reopened raw revision', 'destination missing']) {
  scenario('unconfirmed receipt (' + mismatch + ') retains pending and refuses tab rebinding', async () => {
    const instance = await app({ rename: async (intent, server) => {
      const receipt = server.commit(intent);
      if (mismatch === 'operation id') receipt.operation_id += '-different';
      if (mismatch === 'review document id') receipt.review.document_id = 'synthetic-foreign-review';
      if (mismatch === 'review generation') receipt.review.generation++;
      if (mismatch === 'review content') receipt.review.content = 'Different synthetic body';
      if (mismatch === 'reopened revision') server.files.get(NEW).revision = hash('Different synthetic body');
      if (mismatch === 'reopened raw revision') Object.assign(server.files.get(NEW), { disk_revision: hash(BODY), encoding: 'utf-8' });
      if (mismatch === 'destination missing') server.files.delete(NEW);
      return receipt;
    } });
    await instance.rename(); pending(instance); assert.equal(instance.tab._renameInFlight, undefined);
    assert.equal(instance.tab.content, BODY); assert.equal(instance.tab.reviewSession.document_id, 'synthetic-document-a');
    assert.equal(instance.toasts.some(message => message.startsWith('toast.renamedTo')), false);
  });
}

scenario('pending request blocks duplicate rename, forced/single/bulk close, proposal capture and review mutations', async () => {
  const entered = deferred(), release = deferred();
  const instance = await app({ rename: async (intent, server) => { entered.resolve(); await release.promise; return server.commit(intent); } });
  const stamp = await instance.window.ReadMDReview.capture(instance.tab), work = instance.rename(); await entered.promise;
  try {
    await instance.context.closeTab(instance.tab.id, true); await instance.context.closeOtherTabs(instance.other.id); await instance.context.closeAllTabs();
    assert.equal(instance.state.tabs.length, 2); assert.equal(instance.state.activeTabId, instance.tab.id); assert.equal(instance.tab.path, OLD);
    await instance.rename('Duplicate.md'); assert.equal(instance.calls('rename:request').length, 1); assert.equal(instance.calls('rename:status').length, 0);
    assert.equal(await instance.window.ReadMDReview.capture(instance.tab), null);
    await assert.rejects(instance.window.ReadMDReview.propose([{ from: 0, to: 0, insert: 'synthetic' }], stamp, instance.tab), error => error.code === 'review_busy');
    assert.equal(await instance.window.ReadMDReview.discard(instance.tab), false); assert.equal(instance.window.ReadMDReview.history(false), true);
    assert.equal(instance.server.calls.some(call => ['review:edit', 'review:propose', 'review:discard', 'review:undo'].includes(call.kind)), false);
    await assert.rejects(instance.window.ReadMDAiHarness.start({ request: 'synthetic request' }, stamp, instance.tab), error => error.code === 'review_stale_generation');
    assert.equal(instance.calls('task:start').length, 0);
  } finally { release.resolve(); }
  await work;
});

scenario('in-flight flag blocks closing even before the pending intent is constructed', async () => {
  const entered = deferred(), release = deferred();
  const instance = await app({ fileRead: async ({ metadata }) => { if (metadata) { entered.resolve(); await release.promise; } } });
  const work = instance.rename(); await entered.promise;
  try {
    assert.equal(instance.tab._renamePending, undefined); assert.equal(instance.tab._renameInFlight, true);
    await instance.context.closeTab(instance.tab.id, true); await instance.context.closeOtherTabs(instance.other.id); await instance.context.closeAllTabs();
    assert.equal(instance.state.tabs.length, 2); assert.equal(instance.calls('rename:request').length, 0);
  } finally { release.resolve(); }
  await work;
});

scenario('pending rename refuses durable save preparation as well as editing mutations', async () => {
  const instance = await app({ rename: async () => { throw new Error('synthetic_response_lost'); } });
  await instance.rename(); pending(instance);
  assert.equal(await instance.window.ReadMDReview.prepareSave(instance.tab, BODY), false, 'A pending rename must not issue an old-path save CAS');
});

for (const mode of ['ordinary', 'forced discard-confirmed', 'background']) {
  scenario('production loadFile refuses ' + mode + ' old-path refresh while rename is pending', async () => {
    const instance = await app({ rename: async (intent, server) => { server.commit(intent); throw new Error('synthetic_response_lost'); } });
    await instance.rename(); const retained = clone(instance.tab._renamePending), before = clone(instance.tab), active = instance.state.activeTabId;
    instance.server.files.set(OLD, file(OLD, '# A distinct file now owns the old pathname\n'));
    const options = mode === 'background' ? { backgroundRefresh: () => true } : mode === 'forced discard-confirmed' ? { force: true, discardConfirmed: true } : {};
    await instance.context.loadFile(OLD, options);
    assert.deepEqual(clone(instance.tab), before, 'Pending rename owns the tab body and metadata until its receipt settles');
    assert.deepEqual(clone(instance.tab._renamePending), retained); assert.equal(instance.state.activeTabId, active);
    assert.equal(instance.calls('file:open').length, 0, 'Blocked refresh must not fetch the recreated old pathname');
    assert.equal(instance.server.files.get(NEW).content, BODY); assert.equal(instance.server.files.get(OLD).content, '# A distinct file now owns the old pathname\n');
  });
}

scenario('production loadFile refuses old-path refresh during rename preflight before a pending intent exists', async () => {
  const entered = deferred(), release = deferred();
  const instance = await app({ fileRead: async ({ metadata }) => { if (metadata) { entered.resolve(); await release.promise; } } });
  const work = instance.rename(); await entered.promise;
  try {
    assert.equal(instance.tab._renameInFlight, true); assert.equal(instance.tab._renamePending, undefined);
    const before = clone(instance.tab); instance.server.files.set(OLD, file(OLD, '# Another synthetic file body\n'));
    await instance.context.loadFile(OLD, { force: true, discardConfirmed: true });
    assert.deepEqual(clone(instance.tab), before); assert.equal(instance.calls('file:open').length, 0);
  } finally {
    instance.server.files.set(OLD, file(OLD)); release.resolve(); await work;
  }
});

scenario('production loadFile refuses the old endpoint of a cold-restored virtual pending tab', async () => {
  const original = await app({ rename: async () => { throw new Error('synthetic_response_lost'); } }); await original.rename();
  const reopened = await app({ server: original.server, cold: true, status: async () => { throw new Error('synthetic_status_unavailable'); } });
  const owner = reopened.state.tabs.find(tab => tab.recoveryKey === original.tab.recoveryKey); assert.ok(owner);
  assert.equal(owner.path, null); assert.equal(owner.isVirtual, true); assert.equal(owner._renamePending.old_path, OLD);
  const before = clone(owner), count = reopened.state.tabs.length, active = reopened.state.activeTabId;
  reopened.server.files.set(OLD, file(OLD, '# Recreated old name during unresolved recovery\n'));
  await reopened.context.loadFile(OLD, { force: true, discardConfirmed: true });
  assert.deepEqual(clone(owner), before); assert.equal(reopened.state.tabs.length, count, 'An unresolved endpoint cannot create a second file tab');
  assert.equal(reopened.state.activeTabId, active); assert.equal(reopened.calls('file:open').length, 0);
});

for (const completion of ['during preflight', 'after commit']) {
  scenario('production loadFile response issued before rename cannot overwrite the tab ' + completion, async () => {
    const loadEntered = deferred(), loadRelease = deferred(), renameEntered = deferred(), renameRelease = deferred();
    const capturedResponse = file(OLD);
    const instance = await app({ fileRead: async ({ metadata, path: at }) => {
      if (!metadata && at === OLD) { loadEntered.resolve(); await loadRelease.promise; return capturedResponse; }
      if (metadata && completion === 'during preflight') { renameEntered.resolve(); await renameRelease.promise; }
    } });
    const loading = instance.context.loadFile(OLD); await loadEntered.promise;
    const renaming = instance.rename();
    try {
      if (completion === 'during preflight') { await renameEntered.promise; assert.equal(instance.tab._renameInFlight, true); }
      else { await renaming; assert.equal(instance.tab.path, NEW); assert.equal(instance.tab._renamePending, undefined); }
      const before = clone(instance.tab), active = instance.state.activeTabId;
      loadRelease.resolve(); await loading;
      assert.deepEqual(clone(instance.tab), before, 'The old response must not restore pre-rename paths, titles, editor state or text');
      assert.equal(instance.state.activeTabId, active); assert.equal(instance.state.file, completion === 'after commit' ? NEW : OLD);
    } finally { loadRelease.resolve(); renameRelease.resolve(); await loading; await renaming; }
  });
}

for (const taskState of ['queued', 'running']) {
  scenario('active ' + taskState + ' AI task refuses rename without secretly pausing/cancelling/resuming it', async () => {
    const server = backend(); server.tasks.set('synthetic-active-task', { id: 'synthetic-active-task', document_id: 'synthetic-document-a', state: taskState, rounds: 2, elapsed_ms: 12, last_event_id: 5, private_provider: PRIVATE });
    const instance = await app({ server }); await instance.rename(); unchanged(instance);
    assert.deepEqual(instance.server.calls.filter(call => call.kind.startsWith('task:')).map(call => call.kind), ['task:list']);
    assert.equal(instance.calls('rename:request').length, 0); assert.equal(server.tasks.get('synthetic-active-task').state, taskState);
    assert.equal(instance.tab._renameInFlight, undefined); assert.equal(instance.tab._renamePending, undefined); noPrivate(instance.window.ReadMDAiHarness.list()); noPrivate(instance.toasts);
  });
}

scenario('AI discovery failure refuses rename and performs no implicit task stop', async () => {
  const instance = await app({ taskRequest: async () => ({ ok: false, json: async () => ({ ok: false, error_code: 'synthetic_unavailable', error: PRIVATE }) }) });
  await instance.rename(); unchanged(instance); assert.equal(instance.calls('rename:request').length, 0); noPrivate(instance.toasts);
});

scenario('task already starting is awaited without cancellation and a running result prevents the move', async () => {
  const entered = deferred(), release = deferred();
  const instance = await app({ taskRequest: async body => { if (body.action === 'start') { entered.resolve(); await release.promise; } } });
  const stamp = await instance.window.ReadMDReview.capture(instance.tab);
  const starting = instance.window.ReadMDAiHarness.start({ request: 'Synthetic edit request', provider: 'opaque-provider', credential_id: 'opaque-connection' }, stamp, instance.tab);
  await entered.promise;
  const renaming = instance.rename(); await new Promise(resolve => setImmediate(resolve));
  try {
    assert.equal(instance.tab._renameInFlight, true); assert.equal(instance.calls('rename:request').length, 0);
    assert.equal(instance.server.calls.some(call => /task:(pause|cancel|resume)$/.test(call.kind)), false);
  } finally { release.resolve(); }
  assert.equal((await starting).state, 'running'); await renaming; unchanged(instance);
  assert.equal(instance.calls('rename:request').length, 0); assert.equal(instance.window.ReadMDAiHarness.list()[0].state, 'running');
});

scenario('a file cannot fall back to a cosmetic title rename when its native bridge is unavailable', async () => {
  const instance = await app(); instance.context.hasPy = false; await instance.rename(); unchanged(instance);
  assert.equal(instance.calls('rename:request').length, 0); assert.equal(instance.toasts.at(-1), 'review.persistenceFailed');
});

scenario('missing session adapter cannot send an unpersisted operation', async () => {
  const instance = await app(); delete instance.window.ReadMDSession; await instance.rename(); unchanged(instance);
  assert.equal(instance.calls('rename:request').length, 0); assert.equal(instance.tab._renamePending, undefined); assert.equal(instance.toasts.at(-1), 'review.persistenceFailed');
});

scenario('cold session recovery preserves review ID and opens committed destination without replaying the move', async () => {
  const original = await app({ rename: async (intent, server) => { server.commit(intent); throw new Error('synthetic_response_lost'); } });
  await original.rename(); const retained = clone(original.tab._renamePending);
  const reopened = await app({ server: original.server, cold: true });
  const restored = reopened.state.tabs.find(item => item.recoveryKey === original.tab.recoveryKey); assert.ok(restored);
  assert.equal(restored.path, NEW); assert.equal(restored.name, 'Renamed.md'); assert.equal(restored.content, BODY);
  assert.equal(restored._renamePending, undefined); assert.equal(restored.reviewSession.document_id, retained.review_session.document_id);
  assert.equal(reopened.state.activeTabId, restored.id); assert.equal(reopened.state.file, NEW);
  assert.equal(reopened.server.files.has(OLD), false); assert.equal(reopened.server.files.get(NEW).content, BODY);
  assert.equal(reopened.startupCalls.filter(call => call.kind === 'rename:request').length, 0);
  assert.equal(reopened.startupCalls.find(call => call.kind === 'rename:status').operationId, retained.operation_id);
  assert.ok(reopened.startupCalls.some(call => call.kind === 'file:open' && call.path === NEW));
  assert.equal(reopened.startupCalls.some(call => call.kind === 'file:open' && call.path === OLD), false);
  const statusIndex = reopened.startupCalls.findIndex(call => call.kind === 'rename:status');
  assert.ok(statusIndex < reopened.startupCalls.findIndex(call => call.kind === 'file:open' && call.path === NEW));
});

scenario('cold recovery keeps the operation pending if raw bytes changed while decoded content stayed the same', async () => {
  const original = await app({ rename: async (intent, server) => { server.commit(intent); throw new Error('synthetic_response_lost'); } });
  await original.rename(); const retained = clone(original.tab._renamePending);
  Object.assign(original.server.files.get(NEW), { disk_revision: hash(BODY), encoding: 'utf-8' });
  const reopened = await app({ server: original.server, cold: true });
  const restored = reopened.state.tabs.find(item => item.recoveryKey === original.tab.recoveryKey); assert.ok(restored);
  assert.deepEqual(clone(restored._renamePending), retained, 'Equal decoded content cannot confirm preservation of raw bytes');
  assert.equal(restored.isVirtual, true); assert.equal(restored.path, null);
});

scenario('cold pending recovery rejects a receipt for a different review document before rebinding the virtual tab', async () => {
  const original = await app({ rename: async () => { throw new Error('synthetic_response_lost'); }, status: async () => ({ state: 'pending' }) });
  await original.rename(); const retained = clone(original.tab._renamePending);
  const reopened = await app({ server: original.server, cold: true, status: async () => ({ state: 'pending' }) });
  const restored = reopened.state.tabs.find(item => item.recoveryKey === original.tab.recoveryKey); assert.ok(restored); assert.equal(restored.isVirtual, true);
  const receipt = original.server.commit(retained); receipt.review.document_id = 'synthetic-foreign-review';
  reopened.py.rename_status = async () => clone(receipt);
  assert.equal(await reopened.recover(restored), false, 'Pending expected review identity must survive an empty cold-restored review record');
  assert.deepEqual(clone(restored._renamePending), retained); assert.equal(restored.isVirtual, true); assert.equal(restored.path, null);
});

scenario('cold not_found then rejected retry abandons the original operation and restores its preserved source', async () => {
  let attempts = 0;
  const original = await app({ rename: async () => { if (++attempts === 1) throw new Error('synthetic_never_reached'); return { ok: false, state: 'not_started' }; } });
  await original.rename(); const retained = clone(original.tab._renamePending), key = original.tab.recoveryKey;
  const reopened = await app({ server: original.server, cold: true, rename: async () => ({ ok: false, state: 'not_started' }) });
  const restored = reopened.state.tabs.find(item => item.recoveryKey === key); assert.ok(restored);
  assert.equal(restored.path, OLD); assert.equal(restored._renamePending, undefined); assert.equal(restored.isVirtual, false);
  assert.equal(restored.reviewSession.document_id, retained.review_session.document_id);
  assert.equal(reopened.startupCalls.filter(call => call.kind === 'rename:request').length, 1);
  assert.equal(reopened.startupCalls.find(call => call.kind === 'rename:abandon').operationId, retained.operation_id);
  assert.equal(reopened.server.files.has(NEW), false); assert.equal(reopened.server.files.get(OLD).content, BODY);
});

scenario('cold committed recovery keeps pending if writing the restored session fails, then resolves after a durable retry', async () => {
  const original = await app({ rename: async (intent, server) => { server.commit(intent); throw new Error('synthetic_response_lost'); } });
  await original.rename(); const retained = clone(original.tab._renamePending), key = original.tab.recoveryKey;
  let failSettlement = true;
  const reopened = await app({ server: original.server, cold: true,
    saveSettings: async () => failSettlement ? { ok: false, persisted: false } : { ok: true, persisted: true } });
  const restored = reopened.state.tabs.find(item => item.recoveryKey === key); assert.ok(restored);
  assert.equal(restored.path, NEW); assert.deepEqual(clone(restored._renamePending), retained);
  assert.deepEqual(reopened.server.settings.tabs.find(item => item.key === key).renamePending, retained);
  assert.equal(await reopened.window.ReadMDReview.prepareSave(restored, BODY), false);
  await reopened.context.closeTab(restored.id, true); assert.equal(reopened.state.tabs.includes(restored), true);
  assert.equal(reopened.startupCalls.filter(call => call.kind === 'rename:request').length, 0);
  failSettlement = false; assert.equal(await reopened.recover(restored), true); await reopened.settled();
  assert.equal(restored._renamePending, undefined); assert.equal(restored.reviewSession.document_id, retained.review_session.document_id);
  assert.equal(reopened.server.settings.tabs.find(item => item.key === key).renamePending, null);
});

for (const legacyKey of [false, true]) {
  scenario((legacyKey ? 'legacy' : 'new') + ' recovery identity survives rename, opening a recreated old name, cold restore and scoped draft cleanup', async () => {
    const original = await app({ realHistory: true });
    if (legacyKey) { original.tab.recoveryKey = OLD; assert.equal(await original.window.ReadMDSession.flush(), true); }
    const originalKey = original.tab.recoveryKey;
    assert.equal(originalKey, legacyKey ? OLD : 'file:' + original.tab.id);
    const math = { schema: 1, document_revision: REVISION, from: 2, to: 11, original: 'Synthetic', dialect: 'latex', tex: 'x^2', input: 'x^2', selected: {} };
    assert.equal(await original.window.ReadMDRecovery.saveMathDraft(original.tab, math), true);
    await original.rename(); await original.settled(); assert.equal(original.tab.path, NEW); assert.equal(original.tab.recoveryKey, originalKey);

    const recreatedBody = '# Recreated old pathname\nA separate synthetic document.\n';
    original.server.files.set(OLD, file(OLD, recreatedBody));
    original.server.snapshots.set('synthetic-recreated-document', { ...review(OLD, 'synthetic-recreated-document'), content: recreatedBody,
      revision: hash(recreatedBody), disk_revision: hash(recreatedBody), generation: 1, review_revision: 0, changes: [] });
    await original.context.loadFile(OLD);
    const recreated = original.state.tabs.find(item => item.path === OLD); assert.ok(recreated, 'Production loadFile creates the independent old-name tab');
    assert.notEqual(recreated, original.tab); assert.notEqual(recreated.id, original.tab.id);
    assert.equal(recreated.recoveryKey, 'file:' + recreated.id); assert.notEqual(recreated.recoveryKey, originalKey);
    assert.equal(recreated.reviewSession.document_id, 'synthetic-recreated-document');
    assert.equal(await original.window.ReadMDSession.flush(), true);

    // Two recoverable rows can retain the same historical pathname. Their
    // explicit keys, not those old path strings, select their owning tabs.
    for (const [key, owner, text] of [[originalKey, original.tab, BODY], [recreated.recoveryKey, recreated, recreatedBody]]) {
      await original.context.apiFetch('/api/documents/history', { body: JSON.stringify({ op: 'record', key, path: OLD,
        name: owner.name, kind: 'draft', content: text, context: { review: { ...clone(owner.reviewSession), revision: owner.revision } } }) });
    }
    const descriptors = original.server.settings.tabs;
    assert.equal(descriptors.find(item => item.path === NEW).key, originalKey);
    assert.equal(descriptors.find(item => item.path === OLD).key, recreated.recoveryKey);

    const reopened = await app({ server: original.server, cold: true, realHistory: true });
    const renamed = reopened.state.tabs.find(item => item.recoveryKey === originalKey), oldName = reopened.state.tabs.find(item => item.recoveryKey === recreated.recoveryKey);
    assert.ok(renamed); assert.ok(oldName); assert.notEqual(renamed, oldName);
    assert.equal(renamed.path, NEW); assert.equal(renamed.content, BODY); assert.equal(renamed.reviewSession.document_id, 'synthetic-document-a');
    assert.equal(oldName.path, OLD); assert.equal(oldName.content, recreatedBody); assert.equal(oldName.reviewSession.document_id, 'synthetic-recreated-document');
    assert.equal(reopened.state.activeTabId, oldName.id);
    assert.equal((await reopened.window.ReadMDRecovery.mathDrafts(renamed)).length, 1);
    assert.equal((await reopened.window.ReadMDRecovery.mathDrafts(oldName)).length, 0);
    await reopened.window.ReadMDRecovery.saved({ recoveryKey: oldName.recoveryKey, content: recreatedBody, started: Date.now() }, oldName);
    const retainedRows = [...original.server.history.values()];
    assert.ok(retainedRows.some(row => row.entry.kind === 'draft' && row.entry.doc_key === originalKey));
    assert.equal(retainedRows.some(row => row.entry.kind === 'draft' && row.entry.doc_key === oldName.recoveryKey), false);
    assert.ok(retainedRows.some(row => row.entry.kind === 'math_draft' && row.entry.doc_key === originalKey));
  });
}

scenario('intent persistence is allowlisted and never serializes body, arbitrary tab fields, task credentials or backend error text', async () => {
  const instance = await app({ rename: async () => { throw new Error(PRIVATE); } });
  instance.tab.privateConfiguration = PRIVATE; await instance.rename(); pending(instance);
  noPrivate(instance.server.settings); noPrivate(instance.local.get('readmd-session')); noPrivate(instance.toasts);
  const outgoing = instance.calls('rename:request')[0]; noPrivate(outgoing);
  assert.deepEqual(Object.keys(outgoing.pending).sort(), ['expected_disk_revision', 'expected_revision', 'new_stem', 'old_path', 'operation_id', 'review_session']);
  assert.equal(JSON.stringify(outgoing.pending).includes(BODY), false); assert.equal(JSON.stringify(instance.server.settings).includes(BODY), false);
});
