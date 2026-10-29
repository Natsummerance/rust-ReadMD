'use strict';
// Production tab/review adapters, deterministic storage/HTTP/DOM doubles.
// These prove frontend ordering; native durability is tested separately.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { randomUUID, createHash } = require('node:crypto');
const tabsSource = fs.readFileSync(path.join(__dirname, '../assets/js/core/tabs.js'), 'utf8');
const reviewSource = fs.readFileSync(path.join(__dirname, '../assets/js/editor/review.js'), 'utf8');
const readerSource = fs.readFileSync(path.join(__dirname, '../assets/js/reader/render.js'), 'utf8');
const clone = value => value === undefined ? undefined : JSON.parse(JSON.stringify(value));
const turn = () => new Promise(resolve => setImmediate(resolve));
const A = 'C:/Synthetic/Session/A.md', B = 'C:/Synthetic/Session/B.md';
const BODY = '# Original synthetic body\n';
const REVISION = createHash('sha256').update(BODY).digest('hex');
function deferred() { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; }
const scenario = (name, work) => test('session CAS: ' + name, { timeout: 4000 }, work);
const response = (value, status = 200) => ({ ok: status < 400, status, json: async () => clone(value) });
function descriptor(at, extra = {}) {
  return { key: 'file:' + at, path: at, name: at.split('/').pop(), revision: 'synthetic-revision', ...extra };
}
function session(rows = [descriptor(A), descriptor(B)]) { return { version: 1, tabs: rows, active: rows[0]?.key || '', folder: 'C:/Synthetic/Session' }; }
function backend(initial = null) {
  const receipts = new Map(), calls = [], documents = new Map(), recovery = new Map();
  let serial = 0, barrier;
  const service = { value: clone(initial), version: { epoch: randomUUID(), generation: '9007199254740993' },
    calls, documents, recovery, fail: null, drop: false, failRead: false, receipt: null,
    pause(predicate, stage = 'after') { barrier = { predicate, stage, entered: deferred(), release: deferred() }; return barrier; },
    external(value) { service.value = clone(value); service.version.generation = String(BigInt(service.version.generation) + 1n); },
    async request(body) {
      if (!body) {
        calls.push({ method: 'GET' });
        if (service.failRead) throw new Error('Synthetic session read failure');
        return response({ ok: true, protocol: 'readmd-session-cas-v1', session: clone(service.value), session_version: clone(service.version) });
      }
      calls.push({ method: 'POST', body: clone(body) });
      const hold = barrier?.predicate(body) ? barrier : null;
      if (hold) barrier = null;
      if (hold?.stage === 'before') { hold.entered.resolve(body); await hold.release.promise; }
      const encoded = JSON.stringify(body), old = receipts.get(body.call_id);
      if (old) {
        assert.equal(encoded, old.encoded, 'An uncertain request is replayed byte-for-byte');
        if (JSON.stringify(old.result.session_version) !== JSON.stringify(service.version)) return response({ ok: false, error_code: 'session_conflict' }, 409);
        return response(service.receipt ? service.receipt(clone(old.result), body) : old.result);
      }
      if (JSON.stringify(body.expected_session_version) !== JSON.stringify(service.version)) return response({ ok: false, error_code: 'session_conflict' }, 409);
      if (service.fail?.(body)) return response({ ok: false, error_code: 'session_persistence_failed' }, 507);
      service.external(body.session);
      const result = { ok: true, persisted: true, protocol: 'readmd-session-cas-v1', call_id: body.call_id,
        session: clone(body.session), session_version: clone(service.version) };
      receipts.set(body.call_id, { encoded, result });
      if (hold?.stage === 'after') { hold.entered.resolve(body); await hold.release.promise; }
      if (service.drop) { service.drop = false; throw new Error('Synthetic lost acknowledgement after commit'); }
      return response(service.receipt ? service.receipt(clone(result), body) : result);
    },
    review(at) {
      if (!documents.has(at)) documents.set(at, { document_id: 'synthetic-document-' + (++serial), path: at,
        content: BODY, generation: 1, review_revision: 0, revision: REVISION, changes: [] });
      return documents.get(at);
    } };
  return service;
}
async function app(options = {}) {
  const server = options.server || backend(), local = new Map(), timers = new Map(), listeners = new Map(), drafts = new Map(), discarded = [], toasts = [], fileCalls = [], renameCalls = [];
  let serial = 0;
  if (options.mirror) local.set('readmd-session', JSON.stringify(options.mirror));
  function tab(at) { return { id: 'synthetic-tab-' + (++serial), mode: 'file', path: at, name: at.split('/').pop(),
    original: BODY, content: BODY, fixed: BODY, isDirty: false, revision: REVISION, editGeneration: 0 }; }
  const first = tab(A), second = tab(B);
  const state = { tabs: options.cold ? [] : [first, second], activeTabId: options.cold ? null : first.id,
    mode: options.cold ? 'welcome' : 'file', editing: false, folder: '', pagination: { enabled: false }, scrollPos: {} };
  const listen = (type, callback) => { if (!listeners.has(type)) listeners.set(type, []); listeners.get(type).push(callback); };
  const document = { body: { inert: false }, readyState: 'loading', activeElement: null, hidden: false,
    getElementById: () => null, addEventListener: listen, createElement: () => ({ setAttribute() {}, addEventListener() {} }) };
  const window = { READMD_ENGINE: options.legacy ? 'legacy' : 'rust', crypto: { randomUUID }, i18n: { t: key => key },
    addEventListener: listen, dispatchEvent(event) { for (const callback of listeners.get(event.type) || []) callback(event); },
    ReadMDAiHarness: { pauseTab: async () => true, reserveTabForRename: async () => true, releaseTab() {} },
    ReadMDRecovery: {
      schedule() {},
      async flush() {
        for (const owner of state.tabs) if (owner.isDirty) {
          const review = clone(window.ReadMDReview?.snapshot(owner));
          drafts.set(owner.id, { content: owner.content, review });
          if (owner.recoveryKey) server.recovery.set(owner.recoveryKey, { content: owner.content, entry: { context: { review } } });
        }
        return options.recoveryFlush ? options.recoveryFlush() : true;
      },
      async discard(owner) { discarded.push(owner); drafts.delete(owner.id); server.recovery.delete(owner.recoveryKey); return true; },
      async sessionDrafts(keys) { if (options.draftFailure) throw new Error('Synthetic draft read failure'); return new Map(keys.filter(key => server.recovery.has(key)).map(key => [key, clone(server.recovery.get(key))])); }, mathDrafts: async () => [] } };
  const py = { get_settings: async () => ({ session: server.value }),
    rename_file: async (...args) => { renameCalls.push(args); return { ok: false, state: 'not_started' }; }, save_settings: async value => {
    assert.ok(options.legacy, 'Rust native mode must use the authenticated apiFetch session endpoint'); server.value = clone(value.session); return { ok: true }; } };
  if (options.recoverRename) py.rename_status = async () => ({ state: 'not_found' });
  async function apiFetch(url, request) {
    if (url === '/api/session') return server.request(request?.body ? JSON.parse(request.body) : null);
    if (url.startsWith('/api/file?')) {
      const at = new URL(url, 'https://synthetic.invalid').searchParams.get('p');
      fileCalls.push(at);
      const file = { path: at, name: at.split('/').pop(), original: BODY, content: BODY, revision: REVISION, disk_revision: REVISION };
      if (options.fileRead) await options.fileRead(at);
      return response(file);
    }
    assert.equal(url, '/api/review');
    const body = JSON.parse(request.body);
    if (body.action === 'resume') return body.path.startsWith('draft:') && !server.documents.has(body.path)
      ? response({ found: false }) : response({ found: true, snapshot: server.review(body.path) });
    if (body.action === 'open') return response(server.review(body.path));
    const head = [...server.documents.values()].find(value => value.document_id === body.document_id); assert.ok(head);
    if (body.action === 'propose') {
      const edit = body.edits[0], original = head.content.slice(edit.from, edit.to);
      head.content = head.content.slice(0, edit.from) + edit.insert + head.content.slice(edit.to); head.generation++; head.review_revision++;
      head.changes.push({ id: 'synthetic-change', from: edit.from, to: edit.from + edit.insert.length,
        original, replacement: edit.insert, decision: 'pending', application: 'applied', version: 1 });
    } else assert.equal(body.action, 'read');
    return response(head);
  }
  const context = vm.createContext({ window, document, state, hasPy: options.local ? false : true, py, apiFetch,
    Element: class {}, TextEncoder, Uint32Array, crypto: { randomUUID },
    CustomEvent: class { constructor(type, value) { this.type = type; Object.assign(this, value); } },
    settingsSaveQueue: Promise.resolve(true), documentLoadEpoch: 0, normalizePath: value => value.replace(/\\/g, '/').toLowerCase(),
    localStorage: { getItem: key => local.get(key) || null, setItem: (key, value) => local.set(key, value) },
    setTimeout(callback, delay) { const id = ++serial; timers.set(id, { callback, delay }); return id; }, clearTimeout: id => timers.delete(id),
    requestAnimationFrame() {}, $: id => id === 'btn-reload' ? {} : null, showToast: text => toasts.push(text), busy() {}, updateUnloadGuard() {},
    getEditContent: () => state.tabs.find(owner => owner.id === state.activeTabId)?.content || '',
    prepareDocCitations: async () => {}, renderContent: async () => {}, setFixes() {}, setFileTitle() {}, updateStatus() {}, afterRender() {}, renderPage() {},
    setProgress() {}, addRecent() {}, pushHistory() {}, saveLastFile() {}, clearAiOutput() {},
    beginDocumentLoad: () => ++context.documentLoadEpoch, isDocumentLoadCurrent: epoch => context.documentLoadEpoch === epoch,
    resetHome() {}, openInitialFile: async () => {}, exitEdit() { state.editing = false; }, goHome: async () => { state.mode = 'welcome'; },
    console: { error() {} } });
  vm.runInContext(reviewSource, context); vm.runInContext(tabsSource, context);
  if (options.reader) {
    vm.runInContext(readerSource.slice(readerSource.indexOf('async function waitForSessionTransition('), readerSource.indexOf('\nasync function openExternalFile(')), context);
    vm.runInContext(readerSource.slice(readerSource.indexOf('async function renderVirtual('), readerSource.indexOf('\nasync function convertFile(')), context);
  }
  context.promptDirtyClose = async () => options.choice || 'cancel';
  window.__readmdSessionReady = window.ReadMDSession.restore();
  if (!options.deferRestore) await window.__readmdSessionReady;
  return { server, state, first, second, window, document, context, drafts, discarded, toasts, local, timers, fileCalls, renameCalls,
    posts: () => server.calls.filter(call => call.method === 'POST'),
    async settled() { await turn(); await context.settingsSaveQueue; },
    emit(type) { for (const callback of listeners.get(type) || []) callback({ type }); },
    async pendingReview(owner = first) {
      const mark = await window.ReadMDReview.capture(owner);
      await window.ReadMDReview.propose([{ from: 2, to: 10, insert: 'Reviewed' }], mark, owner);
      return window.ReadMDReview.snapshot(owner);
    } };
}

scenario('two native instances cannot erase another instance pending rename or refresh a conflicted token', async () => {
  const server = backend(), a = await app({ server }), b = await app({ server });
  a.first._renamePending = { operation_id: 'rename-synthetic', old_path: A, new_stem: 'Renamed', expected_revision: 'a'.repeat(64), expected_disk_revision: 'b'.repeat(64) };
  assert.equal(await a.window.ReadMDSession.flush(), true);
  const winning = clone(server.value), reads = server.calls.filter(call => call.method === 'GET').length;
  assert.equal(await b.window.ReadMDSession.flush(), false);
  await b.context.closeTab(b.first.id, true); b.emit('pagehide'); await b.settled();
  assert.equal(b.state.tabs.length, 2); assert.deepEqual(server.value, winning);
  assert.equal(server.calls.filter(call => call.method === 'GET').length, reads);
  assert.ok(!b.toasts.join(' ').includes('session_'));
});

scenario('queued writes bind opaque decimal-string versions when sent', async () => {
  const a = await app(), hold = a.server.pause(() => true);
  const first = a.window.ReadMDSession.flush(); await hold.entered.promise;
  a.state.folder = 'Latest synthetic folder'; const second = a.window.ReadMDSession.flush();
  hold.release.resolve(); assert.equal(await first, true); assert.equal(await second, true);
  assert.equal(a.posts()[0].body.expected_session_version.generation, '9007199254740993');
  assert.equal(a.posts()[1].body.expected_session_version.generation, '9007199254740994');
  assert.equal(a.server.value.folder, 'Latest synthetic folder');
});

scenario('a lost ACK replays the exact original before validating a newer local snapshot', async () => {
  const a = await app(); a.server.drop = true;
  assert.equal(await a.window.ReadMDSession.flush(), false);
  a.state.folder = 'Newer synthetic state'; assert.equal(await a.window.ReadMDSession.flush(), true);
  assert.deepEqual(a.posts()[1].body, a.posts()[0].body);
  assert.notEqual(a.posts()[2].body.call_id, a.posts()[0].body.call_id);
  assert.equal(a.server.value.folder, 'Newer synthetic state');
});

scenario('an old idempotent receipt cannot grant the current head after a competing writer', async () => {
  const a = await app(); a.server.drop = true; assert.equal(await a.window.ReadMDSession.flush(), false);
  const winner = session([descriptor(A, { renamePending: { operation_id: 'rename-winner' } })]); a.server.external(winner);
  assert.equal(await a.window.ReadMDSession.flush(), false);
  assert.deepEqual(a.posts()[1].body, a.posts()[0].body); assert.deepEqual(a.server.value, winner);
});

scenario('native read failure permits mirror recovery without granting write eligibility', async () => {
  const server = backend(session()); server.failRead = true;
  const a = await app({ server, cold: true, mirror: session() });
  assert.equal(a.state.tabs.length, 2); assert.equal(await a.window.ReadMDSession.flush(), false);
  await a.context.closeAllTabs(); assert.equal(a.state.tabs.length, 2); assert.equal(a.posts().length, 0);
});

scenario('an interrupted native restore cannot publish a partial workspace', async () => {
  const server = backend(session()), original = server.request;
  const entered = deferred(), release = deferred();
  server.request = async body => { if (body) return original(body); const captured = await original(body); entered.resolve(); await release.promise; return captured; };
  const a = await app({ server, cold: true, deferRestore: true }); await entered.promise;
  a.state.tabs = [a.second]; a.state.activeTabId = a.second.id; a.context.documentLoadEpoch++;
  a.emit('pagehide'); release.resolve(); await a.window.__readmdSessionReady;
  assert.equal(await a.window.ReadMDSession.flush(), false); assert.equal(a.posts().length, 0);
  assert.equal(a.state.tabs[0], a.second); assert.equal(server.value.tabs.length, 2);
});

for (const malformed of ['object', 'duplicate path', 'duplicate key', 'too many', 'invalid pending', 'invalid review']) scenario('unrestorable ' + malformed + ' never grants write eligibility', async () => {
  let value = session();
  if (malformed === 'object') value = {};
  if (malformed === 'duplicate path') value.tabs.push(descriptor(A, { key: 'duplicate' }));
  if (malformed === 'duplicate key') value.tabs[1].key = value.tabs[0].key;
  if (malformed === 'too many') value.tabs = Array.from({ length: 101 }, (_, i) => descriptor(A + i));
  if (malformed.startsWith('invalid')) value.tabs[0].renamePending = { operation_id: 'rename-bad', old_path: A, new_stem: 'New',
    expected_revision: 'a'.repeat(64), expected_disk_revision: 'b'.repeat(64), ...(malformed === 'invalid review' ? { review_session: { document_id: 4 } } : { operation_id: '?' }) };
  const a = await app({ server: backend(value), cold: true });
  assert.equal(await a.window.ReadMDSession.flush(), false); assert.equal(a.posts().length, 0);
});

scenario('failed draft lookup for a dirty descriptor leaves the native snapshot intact', async () => {
  const value = session([descriptor(A, { dirty: true })]), server = backend(value);
  const a = await app({ server, cold: true, draftFailure: true });
  assert.equal(await a.window.ReadMDSession.flush(), false); assert.equal(a.posts().length, 0); assert.deepEqual(server.value, value);
});

for (const kind of ['single', 'other', 'all']) scenario(kind + ' close waits for ACK and rejects reentry without reviving pages', async () => {
  const a = await app(); a.window.ReadMDSession.persist();
  const oldTimer = [...a.timers.values()].find(value => value.delay === 120)?.callback;
  const hold = a.server.pause(body => body.session.tabs.length < 2);
  const work = kind === 'single' ? a.context.closeTab(a.first.id, true) : kind === 'other' ? a.context.closeOtherTabs(a.second.id) : a.context.closeAllTabs();
  await hold.entered.promise;
  assert.equal(a.state.tabs.length, 2); assert.equal(a.document.body.inert, true); assert.equal(a.window.ReadMDSession.busy(), true);
  let idle = false; const waiting = a.window.ReadMDSession.whenIdle().then(() => { idle = true; });
  await turn(); assert.equal(idle, false, 'Readers wait for the close transaction to settle');
  const count = a.posts().length; await a.context.closeAllTabs(); await a.context.closeTab(a.first.id, true); assert.equal(a.posts().length, count);
  hold.release.resolve(); await work; await waiting; assert.equal(idle, true);
  if (oldTimer) oldTimer(); await a.settled();
  assert.equal(a.state.tabs.length, kind === 'all' ? 0 : 1); assert.equal(a.server.value.tabs.length, kind === 'all' ? 0 : 1);
  assert.equal(a.server.value.tabs.some(row => row.path === A), false); assert.equal(a.document.body.inert, false);
});

for (const kind of ['single', 'other', 'all']) scenario(kind + ' discard does not clear source or its only draft before close persistence', async () => {
  const a = await app({ choice: 'discard' }); await a.pendingReview(); const body = a.first.content;
  a.server.fail = request => request.session.tabs.length < 2;
  if (kind === 'single') await a.context.closeTab(a.first.id);
  else if (kind === 'other') await a.context.closeOtherTabs(a.second.id);
  else await a.context.closeAllTabs();
  assert.equal(a.state.tabs.length, 2); assert.equal(a.first.content, body); assert.equal(a.discarded.length, 0);
  assert.equal(a.drafts.get(a.first.id).content, body);
  assert.equal(a.window.ReadMDReview.snapshot(a.first).changes[0].decision, 'pending');
  assert.equal(a.document.body.inert, false); assert.equal(a.window.ReadMDSession.busy(), false);
});

scenario('dirty cancel leaves pages untouched and successful discard cleans only after ACK', async () => {
  const cancelled = await app(); cancelled.first.isDirty = true;
  await cancelled.context.closeTab(cancelled.first.id); assert.equal(cancelled.posts().length, 0); assert.equal(cancelled.state.tabs.length, 2);
  const a = await app({ choice: 'discard' }); await a.pendingReview();
  const hold = a.server.pause(body => body.session.tabs.length < 2), work = a.context.closeTab(a.first.id);
  await hold.entered.promise; assert.equal(a.discarded.length, 0); assert.ok(a.drafts.has(a.first.id));
  hold.release.resolve(); await work; assert.equal(a.discarded.length, 1); assert.equal(a.state.tabs.includes(a.first), false);
});

scenario('whenIdle also releases deferred readers when close persistence fails', async () => {
  const a = await app(); a.server.fail = body => body.session.tabs.length < 2;
  const hold = a.server.pause(body => body.session.tabs.length < 2, 'before'), work = a.context.closeTab(a.first.id, true);
  await hold.entered.promise; let idle = false;
  const waiting = a.window.ReadMDSession.whenIdle().then(() => { idle = true; });
  await turn(); assert.equal(idle, false); hold.release.resolve(); await work; await waiting;
  assert.equal(a.state.tabs.length, 2); assert.equal(idle, true); assert.equal(a.document.body.inert, false);
});

scenario('programmatic mutation plus a competing writer fails compensation while preserving pages, drafts and mirror', async () => {
  const a = await app(); await a.pendingReview(); const hold = a.server.pause(body => body.session.tabs.length < 2);
  const work = a.context.closeTab(a.first.id, true); await hold.entered.promise;
  a.first.content += 'programmatic synthetic edit'; a.first.editGeneration++;
  const winner = session([descriptor(B)]); a.server.external(winner); hold.release.resolve(); await work;
  assert.equal(a.state.tabs.includes(a.first), true); assert.ok(a.drafts.get(a.first.id).content.includes('programmatic'));
  assert.ok(JSON.parse(a.local.get('readmd-session')).tabs.some(row => row.path === A));
  assert.deepEqual(a.server.value, winner, 'A failed compensation cannot overwrite the competing authoritative session');
  assert.equal(a.window.ReadMDSession.busy(), false); assert.equal(a.document.body.inert, false);
});

for (const mode of ['local', 'legacy']) scenario(mode + ' fallback uses its existing store without the Rust session endpoint', async () => {
  const a = await app({ local: mode === 'local', legacy: mode === 'legacy' });
  assert.equal(await a.window.ReadMDSession.flush(), true); assert.equal(a.server.calls.length, 0);
  await a.context.closeAllTabs(); assert.equal(a.state.tabs.length, 0);
  assert.equal(mode === 'local' ? JSON.parse(a.local.get('readmd-session')).tabs.length : a.server.value.tabs.length, 0);
});

for (const operation of ['new file', 'late response', 'virtual import']) scenario('production reader defers ' + operation + ' until close settles without dropping the result', async () => {
  const entered = deferred(), release = deferred(), incoming = 'C:/Synthetic/Session/Incoming.md'; let held = false;
  const a = await app({ reader: true, fileRead: async at => {
    if (operation === 'late response' && at === incoming && !held) { held = true; entered.resolve(); await release.promise; }
  } });
  let opening;
  if (operation === 'late response') { opening = a.context.loadFile(incoming); await entered.promise; }
  const hold = a.server.pause(body => body.session.tabs.length < 2), closing = a.context.closeTab(a.first.id, true);
  await hold.entered.promise;
  if (operation === 'new file') opening = a.context.loadFile(incoming);
  if (operation === 'virtual import') opening = a.context.renderVirtual('convert', 'Imported.pdf', 'C:/Synthetic/Session', 'Synthetic converted result', [], {});
  if (operation === 'late response') release.resolve();
  await turn(); assert.equal(a.state.tabs.length, 2); assert.equal(a.state.tabs.some(tab => tab.path === incoming), false);
  hold.release.resolve(); await closing; await opening;
  assert.equal(a.state.tabs.includes(a.first), false);
  if (operation === 'virtual import') assert.ok(a.state.tabs.some(tab => tab.isVirtual && tab.content === 'Synthetic converted result'));
  else assert.ok(a.state.tabs.some(tab => tab.path === incoming));
  if (operation === 'late response') assert.equal(a.fileCalls.filter(at => at === incoming).length, 2, 'A delayed response is re-read after the transition');
});

function wrongReceipt(kind, receipt, request) {
  if (kind === 'protocol') receipt.protocol = 'synthetic-other-protocol';
  if (kind === 'call') receipt.call_id += '-other';
  if (kind === 'session') receipt.session = { ...receipt.session, folder: 'Different synthetic session' };
  if (kind === 'epoch') receipt.session_version.epoch = randomUUID();
  if (kind === 'generation') receipt.session_version.generation = String(BigInt(request.expected_session_version.generation) + 2n);
  return receipt;
}
for (const kind of ['protocol', 'call', 'session', 'epoch', 'generation']) {
  scenario('bad ' + kind + ' receipt never sends rename and retains the exact unconfirmed intent', async () => {
    const a = await app(); a.server.receipt = (receipt, request) => wrongReceipt(kind, receipt, request);
    await a.context.renameTab(a.first.id, 'Renamed.md');
    assert.equal(a.renameCalls.length, 0, 'A 200 response for another request cannot authorize a filesystem move');
    assert.ok(a.first._renamePending); assert.equal(a.first.content, BODY); assert.equal(a.first.path, A);
    assert.equal(await a.window.ReadMDSession.flush(), false);
    assert.deepEqual(a.posts()[1].body, a.posts()[0].body, 'Rejected success keeps the original exact request for reconciliation');
    await a.context.closeTab(a.first.id, true); assert.ok(a.state.tabs.includes(a.first));
  });
  scenario('bad ' + kind + ' close receipt preserves pages and pending-review drafts', async () => {
    const a = await app({ choice: 'discard' }); await a.pendingReview(); const body = a.first.content;
    a.server.receipt = (receipt, request) => request.session.tabs.length < 2 ? wrongReceipt(kind, receipt, request) : receipt;
    await a.context.closeTab(a.first.id);
    assert.equal(a.state.tabs.length, 2, 'An unbound close receipt cannot remove a tab');
    assert.equal(a.first.content, body); assert.equal(a.discarded.length, 0); assert.equal(a.drafts.get(a.first.id).content, body);
    assert.equal(a.window.ReadMDReview.snapshot(a.first).changes[0].decision, 'pending');
    const candidate = a.posts().find(call => call.body.session.tabs.length < 2).body;
    assert.equal(await a.window.ReadMDSession.flush(), false);
    assert.deepEqual(a.posts().at(-1).body, candidate);
  });
}

scenario('equivalent receipt JSON accepts different object-key order without generation precision loss', async () => {
  const server = backend(); server.version.generation = '18446744073709551613';
  const a = await app({ server });
  const reorder = value => Array.isArray(value) ? value.map(reorder) : value && typeof value === 'object'
    ? Object.fromEntries(Object.keys(value).reverse().map(key => [key, reorder(value[key])])) : value;
  server.receipt = receipt => reorder(receipt);
  assert.equal(await a.window.ReadMDSession.flush(), true);
  assert.equal(server.version.generation, '18446744073709551614');
  assert.equal(await a.window.ReadMDSession.flush(), true);
  assert.equal(server.version.generation, '18446744073709551615');
});

scenario('timer completing before a failed recovery barrier cannot publish a rename replayed by restart', async () => {
  const entered = deferred(), release = deferred();
  const a = await app({ recoveryFlush: async () => { entered.resolve(); await release.promise; return false; } });
  await a.pendingReview(a.second); const draft = a.second.content;
  const identifiers = [];
  a.context.crypto.randomUUID = () => { const id = randomUUID(); identifiers.push(id); return id; };
  const work = a.context.renameTab(a.first.id, 'Renamed.md'); await entered.promise;
  const originalIntent = 'rename-' + identifiers[0];
  const timer = [...a.timers.values()].find(value => value.delay === 120);
  assert.ok(timer, 'An already scheduled autosave runs during the delayed recovery barrier');
  timer.callback(); await a.settled();
  assert.ok(a.posts().length, 'The autosave really completed before recovery failed');
  release.resolve(); await work;
  const restarted = await app({ server: a.server, cold: true, recoverRename: true });
  assert.equal(restarted.renameCalls.length, 0, 'Recovery must not issue a rename whose preparation barrier failed');
  assert.equal(a.renameCalls.length, 0); assert.equal(a.first.path, A); assert.equal(a.first.content, BODY);
  assert.equal(a.server.value.tabs.some(tab => tab.renamePending), false);
  assert.equal(a.first._renamePending, undefined);
  assert.equal(a.posts().some(call => call.body.session.tabs.some(tab => tab.renamePending?.operation_id === originalIntent)), false);
  assert.equal(a.drafts.get(a.second.id).content, draft);
  assert.equal(restarted.state.tabs.find(tab => tab.path === B).content, draft);
  assert.equal(restarted.window.ReadMDReview.snapshot(restarted.state.tabs.find(tab => tab.path === B)).changes[0].id, 'synthetic-change');
});

scenario('lost intent ACK followed by a competing writer and replay conflict retains the original recovery restriction', async () => {
  const a = await app({ recoverRename: true }); await a.pendingReview(a.second);
  const draft = a.second.content; a.server.drop = true;
  await a.context.renameTab(a.first.id, 'Renamed.md');
  const original = clone(a.posts()[0].body), intent = clone(a.first._renamePending);
  assert.ok(intent); assert.equal(a.renameCalls.length, 0);
  const winner = clone(a.server.value); winner.folder = 'Other synthetic window has adopted the pending move';
  a.server.external(winner);
  assert.equal(await a.context.recoverPendingRename(a.first), false);
  assert.deepEqual(a.posts()[1].body, original, 'Conflict follows the exact accepted request rather than a new snapshot');
  await a.context.recoverPendingRename(a.first);
  assert.equal(a.renameCalls.length, 0, 'A conflicted reconciliation cannot release recovery into a real move');
  assert.equal(a.window.ReadMDSession.uncertainRename(intent.operation_id), true);
  assert.equal(a.first._renamePending.operation_id, intent.operation_id);
  await a.context.closeTab(a.first.id, true);
  assert.ok(a.state.tabs.includes(a.first)); assert.equal(a.first.content, BODY); assert.equal(a.first.path, A);
  assert.equal(a.drafts.get(a.second.id).content, draft); assert.deepEqual(a.server.value, winner);
});

scenario('a queued open refreshes an externally changed tab without waiting behind its own outer load', async () => {
  const a = await app({ reader: true }), idle = deferred(); let busy = true;
  a.window.ReadMDSession.busy = () => busy;
  a.window.ReadMDSession.whenIdle = () => idle.promise;
  a.second.externalChanged = true;
  const opening = a.context.loadFile(B), incoming = 'C:/Synthetic/Session/AfterRefresh.md';
  const following = a.context.loadFile(incoming);
  await turn(); assert.equal(a.fileCalls.length, 0);
  busy = false; idle.resolve(); await opening; await following;
  assert.equal(a.second.externalChanged, false);
  assert.ok(a.fileCalls.filter(at => at === B).length >= 1);
  assert.equal(a.fileCalls.at(-1), incoming);
  assert.ok(a.state.tabs.some(tab => tab.path === incoming));
});
