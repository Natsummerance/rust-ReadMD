'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { randomUUID } = require('node:crypto');
const adapterSource = fs.readFileSync(path.join(__dirname, '../assets/js/editor/review.js'), 'utf8');
const recoverySource = fs.readFileSync(path.join(__dirname, '../assets/js/features/document-history.js'), 'utf8');
const tabsSource = fs.readFileSync(path.join(__dirname, '../assets/js/core/tabs.js'), 'utf8');
const sessionSource = tabsSource.slice(tabsSource.indexOf('/* Persist the open page set'));
const aiSource = fs.readFileSync(path.join(__dirname, '../assets/js/features/ai.js'), 'utf8');
const selection = require('../assets/js/features/ai-selection.js');
const targetSource = aiSource.slice(aiSource.indexOf('function captureAiSelection()'), aiSource.indexOf('\nfunction setAiBusy('));
const clone = value => JSON.parse(JSON.stringify(value));
const tick = () => new Promise(resolve => setImmediate(resolve));
function deferred() { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; }
function editText(text, edits) {
  let result = text;
  for (const edit of [...edits].reverse()) result = result.slice(0, edit.from) + edit.insert + result.slice(edit.to);
  return result;
}

// This is a deterministic protocol double, not evidence for Rust persistence
// or a real provider. Expected document strings below are independently fixed.
function service() {
  const calls = [], sessions = new Map(), receipts = new Map();
  const tasks = new Set(), cancelled = new Set();
  let hold = null, fail = null, nextId = 0;
  function map(at, edits, end = false) {
    let delta = 0;
    for (const edit of edits) {
      if (at < edit.from || at === edit.from && end) break;
      if (at < edit.to) return edit.from + delta;
      delta += edit.insert.length - edit.to + edit.from;
    }
    return at + delta;
  }
  async function fetch(url, options) {
    const body = JSON.parse(options.body); calls.push({ url, body: clone(body) });
    if (body.action === 'resume') {
      const snapshot = [...sessions.values()].find(value => value.path === body.path);
      return { ok: true, status: 200, json: async () => snapshot ? { found: true, snapshot: clone(snapshot) } : { found: false } };
    }
    if (url === '/api/task/cancel') {
      if (tasks.has(body.id)) cancelled.add(body.id);
      return { ok: true, status: 200, json: async () => ({ ok: true, state: tasks.has(body.id) ? 'cancelling' : 'unknown' }) };
    }
    if (url.endsWith('/ai')) tasks.add(body.task_id);
    if (hold?.action === (url.endsWith('/ai') ? 'ai' : body.action)) { const barrier = hold; hold = null; barrier.entered.resolve(); await barrier.release.promise; }
    if (url.endsWith('/ai') && cancelled.has(body.task_id)) return { ok: false, status: 409, json: async () => ({ error_code: 'cancelled', error: 'cancelled' }) };
    if (fail && fail.action === body.action) { const error = fail; fail = null; return { ok: false, status: 503, json: async () => ({ error_code: error.code, error: 'simulated persistence failure' }) }; }
    let snapshot;
    if (body.action === 'open') {
      snapshot = [...sessions.values()].find(value => value.path === body.path);
      if (!snapshot) {
        snapshot = { document_id: 'document-' + (++nextId), path: body.path, content: body.content,
          generation: 1, revision: 'content-1', review_revision: 0, disk_revision: 'disk-original',
          encoding: body.encoding, line_ending: 'crlf', changes: [], last_event_id: 0 };
        sessions.set(snapshot.document_id, snapshot);
      }
      return { ok: true, status: 200, json: async () => clone(snapshot) };
    }
    if (receipts.has(body.call_id)) return { ok: true, status: 200, json: async () => clone(receipts.get(body.call_id)) };
    snapshot = sessions.get(body.document_id);
    if (body.action === 'read') return { ok: true, status: 200, json: async () => clone(snapshot) };
    if (snapshot.generation !== body.generation || snapshot.review_revision !== body.review_revision) return { ok: false, status: 409, json: async () => ({ error_code: 'review_stale_generation', error: 'stale' }) };
    const before = snapshot.content;
    let edits = body.edits || [];
    if (body.action === 'resolve') {
      const selected = snapshot.changes.filter(change => body.ids.includes(change.id));
      if (selected.some(change => change.taken_over) && !(body.decision === 'accept' && body.resolution === 'keep_current')) return { ok: false, status: 409, json: async () => ({ error_code: 'review_conflict', error: 'overlap' }) };
      edits = body.decision === 'withdraw' ? selected.filter(change => change.application === 'applied' && change.decision !== 'withdrawn').map(change => ({ from: change.from, to: change.to, insert: change.original })).sort((a, b) => a.from - b.from) : [];
      selected.forEach(change => { change.decision = body.decision === 'accept' ? 'accepted' : 'withdrawn'; change.version++; });
    }
    if (url.endsWith('/ai')) edits = [{ from: 0, to: 1, insert: 'AI' }];
    if (body.action === 'undo') edits = [{ from: 0, to: snapshot.content.length, insert: snapshot.undoText }];
    if (body.action === 'redo') edits = [{ from: 0, to: snapshot.content.length, insert: snapshot.redoText }];
    if (body.action === 'discard') edits = [{ from: 0, to: snapshot.content.length, insert: snapshot.diskText || '' }];
    if (edits.length) {
      for (const change of snapshot.changes) {
        if (change.decision === 'withdrawn') continue;
        if (body.action === 'edit' && edits.some(edit => Math.max(edit.from, change.from) < Math.min(edit.to, change.to) || edit.from === edit.to && change.from < edit.from && edit.from < change.to)) { change.taken_over = true; change.decision = 'conflict'; }
        change.from = map(change.from, edits); change.to = Math.max(change.from, map(change.to, edits, true));
      }
      snapshot.content = editText(snapshot.content, edits); snapshot.generation++;
    }
    if (body.action === 'propose' || url.endsWith('/ai')) {
      let delta = 0;
      for (const edit of edits) {
        const from = edit.from + delta;
        snapshot.changes.push({ id: 'change-' + (++nextId), from, to: from + edit.insert.length,
          original: before.slice(edit.from, edit.to), replacement: edit.insert, decision: 'pending', application: 'applied',
          version: 1, origin: 'ai', task_id: null, taken_over: false });
        delta += edit.insert.length - edit.to + edit.from;
      }
    }
    snapshot.review_revision++; snapshot.last_event_id++;
    snapshot.revision = 'content-' + snapshot.generation;
    receipts.set(body.call_id, clone(snapshot));
    return { ok: true, status: 200, json: async () => clone(snapshot) };
  }
  return { calls, sessions, fetch,
    pause(action) { const barrier = { action, entered: deferred(), release: deferred() }; hold = barrier; return barrier; },
    fail(action, code = 'review_persistence_failed') { fail = { action, code }; } };
}
function app(server, initial, options = {}) {
  const tab = { id: options.id || 'tab-a', path: options.path ?? 'Review.md', reviewPath: options.reviewPath, name: 'Review.md', original: options.original ?? initial,
    content: initial, fixed: initial, encoding: 'utf-8-sig', editGeneration: 0, isDirty: options.isDirty ?? false,
    _recoveryContent: options.recoveryContent, _recoveryReview: options.recoveryReview };
  const state = { editing: options.editing ?? true, tabs: [tab], activeTabId: tab.id }, toasts = [];
  const document = options.document || { getElementById: () => null, createElement: () => ({ setAttribute() {}, addEventListener() {}, appendChild() {} }) };
  const events = [], listeners = new Map();
  const window = { crypto: { randomUUID }, i18n: { t: key => key }, cmView: null,
    addEventListener(type, listener) { if (!listeners.has(type)) listeners.set(type, []); listeners.get(type).push(listener); },
    dispatchEvent(event) { events.push(event); for (const listener of listeners.get(event.type) || []) listener(event); },
    ReadMDCodeMirror: { Transaction: { userEvent: { of: value => ({ event: value }) }, addToHistory: { of: value => ({ history: value }) } } } };
  const context = vm.createContext({ window, document, state, TextEncoder, Uint32Array, setTimeout, CustomEvent: class { constructor(type, value) { this.type = type; Object.assign(this, value); } },
    getActiveTab: () => state.tabs.find(item => item.id === state.activeTabId), apiFetch: server.fetch,
    showToast: value => toasts.push(value), renderTabsBar() {}, updateUnloadGuard() {} });
  vm.runInContext(adapterSource, context, { filename: 'review.js' });
  const review = window.ReadMDReview;
  function makeView(owner, text) {
    const view = { text, focus() {}, state: null };
    const documentValue = text => ({ length: text.length, toString: () => text });
    const setState = () => { view.state = { doc: documentValue(view.text), sliceDoc: (from, to) => view.text.slice(from, to), selection: { main: { head: 0, from: 0, to: 0, empty: true } } }; };
    setState();
    view.dispatch = spec => {
      if (!spec.changes) return;
      const edits = Array.isArray(spec.changes) ? spec.changes : [spec.changes];
      const before = view.state, event = spec.userEvent || spec.annotations?.find(item => item.event)?.event || 'input.type';
      view.text = editText(view.text, edits); setState();
      review.onEditorUpdate({ view, transactions: [{ docChanged: true, startState: before, newDoc: view.state.doc,
        isUserEvent: name => event === name || event.startsWith(name + '.'),
        changes: { iterChanges: callback => edits.forEach(edit => callback(edit.from, edit.to, 0, 0, documentValue(edit.insert))) } }] });
      owner.editGeneration++; owner.content = view.text;
    };
    return view;
  }
  const view = makeView(tab, review.lf(initial)); window.cmView = view;
  if (options.mount !== false) review.mount(view, tab, initial);
  return { tab, view, review, window, context, state, toasts, makeView, events,
    edit(edits) { view.dispatch({ changes: edits }); },
    async ready() { await review.ensure(tab); assert.equal(await review.flush(tab), true); },
    snapshot() { return review.snapshot(tab); } };
}

function reviewKeymap(instance) {
  const bindings = [], CM = instance.window.ReadMDCodeMirror;
  Object.assign(CM, { StateEffect: { define: () => ({ of: value => ({ value }) }) }, WidgetType: class {}, Decoration: { set: () => [] },
    ViewPlugin: { fromClass: () => ({}) }, Prec: { highest: value => value }, keymap: { of: value => { bindings.push(...value); return value; } } });
  instance.review.extension(CM, instance.tab);
  return key => { const binding = bindings.find(binding => binding.key === key); assert.ok(binding, 'production review keymap binding: ' + key); return binding.run(instance.view); };
}

function bindAiTargets(instance, target) {
  instance.state.ai = { capturedTarget: { ...target, tabId: instance.tab.id }, conversationTarget: { ...target, tabId: instance.tab.id } };
  instance.window.ReadMDAiSelection = selection;
  instance.context.cmView = instance.window.cmView;
  instance.context.$ = () => null;
  instance.context.getCurrentDocumentText = () => instance.window.cmView.text;
  instance.context.updateAiContextStrip = () => {};
  vm.runInContext(targetSource, instance.context, { filename: 'ai-target.js' });
  instance.window.addEventListener('readmd:document-transaction', instance.context.remapAiTargets);
  return () => instance.context.getAiTargetText();
}

function historyStore() {
  const calls = [], entries = new Map(); let nextId = 0, failedKind = null;
  async function fetch(_url, options = {}) {
    const body = options.body ? JSON.parse(options.body) : null; calls.push(clone(body));
    if (!body) return { ok: true, json: async () => ({ ok: true, entries: [...entries.values()].map(row => clone(row.entry)) }) };
    if (body.op === 'record') {
      if (failedKind === body.kind) return { ok: false, status: 503, json: async () => ({ ok: false, error: 'quota_or_storage_failure' }) };
      const entry = { id: 'history-' + (++nextId), doc_key: body.key, path: body.path, name: body.name,
        kind: body.kind, reason: body.reason, context: clone(body.context), created: nextId, bytes: body.content.length };
      if (body.kind === 'draft') for (const [id, row] of entries) if (row.entry.doc_key === body.key && row.entry.kind === 'draft') entries.delete(id);
      entries.set(entry.id, { entry, content: body.content });
      return { ok: true, json: async () => ({ ok: true, entry: clone(entry) }) };
    }
    if (body.op === 'read') return { ok: true, json: async () => ({ ok: true, ...clone(entries.get(body.id)) }) };
    if (body.op === 'clear_draft') {
      for (const [id, row] of entries) if (row.entry.doc_key === body.key && row.entry.kind === 'draft' && (body.content == null || body.content === row.content)) entries.delete(id);
      return { ok: true, json: async () => ({ ok: true }) };
    }
    throw new Error('Unexpected history operation');
  }
  return { calls, entries, fetch, fail(kind) { failedKind = kind; }, recover() { failedKind = null; } };
}
function installRecovery(instance, server, history) {
  instance.context.$ = () => null;
  instance.context.clearTimeout = clearTimeout;
  instance.context.getEditContent = () => instance.window.cmView.text;
  instance.context.syncActiveTabDirty = () => {
    const tab = instance.state.tabs.find(tab => tab.id === instance.state.activeTabId);
    if (tab) tab.isDirty = tab.content !== tab.original;
  };
  instance.context.apiFetch = (url, options) => url.startsWith('/api/documents/history') ? history.fetch(url, options) : server.fetch(url, options);
  vm.runInContext(recoverySource, instance.context, { filename: 'document-history.js' });
  return instance.window.ReadMDRecovery;
}
async function restoreSession(instance, file, { dirty = true, key = 'Review.md' } = {}) {
  const session = { version: 1, active: key, tabs: [{ key, path: 'Review.md', name: 'Review.md', dirty, revision: 'disk-revision' }] };
  const stored = new Map([['readmd-session', JSON.stringify(session)]]), previousFetch = instance.context.apiFetch;
  instance.context.apiFetch = (url, options) => url.startsWith('/api/file?') ? Promise.resolve({ ok: true, json: async () => ({ path: 'Review.md', name: 'Review.md', original: file, content: file, revision: 'disk-revision' }) }) : previousFetch(url, options);
  Object.assign(instance.context, { documentLoadEpoch: 0, hasPy: false, crypto: { randomUUID }, settingsSaveQueue: Promise.resolve(),
    normalizePath: value => value.replace(/\\/g, '/').toLowerCase(),
    localStorage: { getItem: key => stored.get(key), setItem: (key, value) => stored.set(key, value) },
    syncStateFromActiveTab() {}, prepareDocCitations: async () => {}, renderActiveTab: async () => {}, toggleEdit: async () => {},
    resetHome() {}, captureReaderState() {}, openInitialFile: async () => {} });
  instance.context.document.addEventListener ||= () => {};
  Object.assign(instance.state, { tabs: [], activeTabId: null, editing: false, mode: 'welcome', pagination: { enabled: false } });
  vm.runInContext(sessionSource, instance.context, { filename: 'tabs-session.js' });
  await instance.window.ReadMDSession.restore();
  assert.equal(instance.state.tabs.length, 1);
  return instance.state.tabs[0];
}

test('passive reader flush and close-all preparation never open unseen review records', async () => {
  const server = service(), instance = app(server, 'Original source', { mount: false, editing: false });
  instance.state.tabs.push({ id: 'other', path: 'Other.md', content: 'Other', original: 'Other' });
  assert.equal(await instance.review.restore(instance.tab), null);
  assert.equal(await instance.review.flush(instance.tab), true);
  assert.equal(await instance.review.flushAll(), true);
  assert.equal(server.sessions.size, 0);
  assert.deepEqual(server.calls.map(call => call.body.action), ['resume']);
});

test('explicit reader AI capture and save still initialize a session from author source', async () => {
  const server = service(), instance = app(server, 'Auto-fixed projection', { mount: false, editing: false, original: 'Original source' });
  const stamp = await instance.review.capture(instance.tab);
  assert.equal(stamp.content, 'Original source');assert.equal(server.sessions.size, 1);
  assert.equal(server.calls.filter(call => call.body.action === 'open').length, 1);
  assert.ok(await instance.review.prepareSave(instance.tab, 'Original source'));
  instance.review.finishSave(instance.tab);
});

test('A/B/C uses independent IDs and survives save/reopen before withdrawing only B', async () => {
  const server = service(), original = '\uFEFF甲😀 original\r\n\r\nHuman\r\n\r\n甲😀 original\r\n';
  const first = app(server, original); await first.ready();
  assert.equal(server.calls[0].body.content, '甲😀 original\n\nHuman\n\n甲😀 original\n');
  const stamp = await first.review.capture(first.tab);
  await first.review.propose([{ from: 4, to: 12, insert: 'AI-reviewed' }, { from: 25, to: 33, insert: 'AI-reviewed' }], stamp, first.tab);
  assert.equal(first.view.text, '甲😀 AI-reviewed\n\nHuman\n\n甲😀 AI-reviewed\n');
  const [a, b] = first.snapshot().changes; assert.notEqual(a.id, b.id);
  first.edit({ from: 22, to: 22, insert: ' HUMAN_STAYS' });
  assert.equal(await first.review.flush(first.tab), true);
  const generation = first.snapshot().generation;
  await first.review.resolve([a.id], 'accept', first.tab);
  assert.equal(first.snapshot().generation, generation, 'accepting an applied candidate must not insert it twice');
  const saved = first.view.text;
  const token = await first.review.prepareSave(first.tab); assert.equal(token.generation, generation);
  first.review.saved(first.tab, first.snapshot(), saved); first.review.finishSave(first.tab);
  const reopened = app(server, saved, { id: 'tab-restored' }); await reopened.ready();
  assert.equal(reopened.snapshot().changes.find(change => change.id === a.id).decision, 'accepted');
  assert.equal(reopened.snapshot().changes.find(change => change.id === b.id).decision, 'pending');
  await reopened.review.resolve([b.id], 'withdraw', reopened.tab);
  assert.equal(reopened.view.text, '甲😀 AI-reviewed\n\nHuman HUMAN_STAYS\n\n甲😀 original\n');
  assert.equal(reopened.snapshot().changes.find(change => change.id === a.id).decision, 'accepted');
});

test('FIFO manual edits do not let the first acknowledgement erase later typing', async () => {
  const server = service(), instance = app(server, 'abc'); await instance.ready();
  const barrier = server.pause('edit');
  instance.edit({ from: 3, to: 3, insert: '1' }); await barrier.entered.promise;
  instance.edit({ from: 4, to: 4, insert: '2' });
  barrier.release.resolve();
  assert.equal(await instance.review.flush(instance.tab), true);
  assert.equal(instance.view.text, 'abc12'); assert.equal(instance.snapshot().content, 'abc12');
  assert.deepEqual(server.calls.filter(call => call.body.action === 'edit').map(call => call.body.generation), [1, 2]);
});

test('late proposal rebases through following non-overlapping hand input', async () => {
  const server = service(), instance = app(server, 'A\n\nHuman'); await instance.ready();
  const stamp = await instance.review.capture(instance.tab), barrier = server.pause('propose');
  const proposal = instance.review.propose([{ from: 0, to: 1, insert: 'AI-reviewed' }], stamp, instance.tab);
  await barrier.entered.promise;
  instance.edit({ from: 8, to: 8, insert: ' C' }); barrier.release.resolve(); await proposal;
  assert.equal(await instance.review.flush(instance.tab), true);
  assert.equal(instance.view.text, 'AI-reviewed\n\nHuman C');
  assert.equal(instance.snapshot().content, 'AI-reviewed\n\nHuman C');
  assert.equal(server.calls.filter(call => call.body.action === 'edit').length, 1, 'review receipt is not logged again as manual input');
});

test('overlapping late proposal keeps hand text and durable candidate for conflict comparison', async () => {
  const server = service(), instance = app(server, 'original\nC'); await instance.ready();
  const stamp = await instance.review.capture(instance.tab), barrier = server.pause('propose');
  const proposal = instance.review.propose([{ from: 0, to: 8, insert: 'AI-reviewed' }], stamp, instance.tab);
  await barrier.entered.promise;
  instance.edit({ from: 0, to: 8, insert: 'HUMAN_STAYS' }); barrier.release.resolve(); await proposal;
  assert.equal(await instance.review.flush(instance.tab), true);
  assert.equal(instance.view.text, 'HUMAN_STAYS\nC');
  const change = instance.snapshot().changes[0];
  assert.equal(change.original, 'original'); assert.equal(change.replacement, 'AI-reviewed');
  assert.equal(change.decision, 'conflict');
  const compensation = server.calls.find(call => call.body.action === 'edit');
  assert.equal(compensation.body.origin, 'editor'); assert.equal(compensation.body.edits[0].insert, 'HUMAN_STAYS');
});

test('a late receipt updates its original tab without mounting or changing the new active tab', async () => {
  const server = service(), instance = app(server, 'A'); await instance.ready();
  const stamp = await instance.review.capture(instance.tab), barrier = server.pause('propose');
  const proposal = instance.review.propose([{ from: 0, to: 1, insert: 'Reviewed' }], stamp, instance.tab);
  await barrier.entered.promise;
  instance.review.detach(instance.view);
  const sentinel = { id: 'tab-sentinel', path: 'Sentinel.md', content: 'SENTINEL', original: 'SENTINEL' };
  instance.state.tabs.push(sentinel); instance.state.activeTabId = sentinel.id;
  const sentinelView = instance.makeView(sentinel, 'SENTINEL'); instance.window.cmView = sentinelView;
  barrier.release.resolve(); await proposal;
  assert.equal(sentinelView.text, 'SENTINEL'); assert.equal(sentinel.content, 'SENTINEL');
  assert.equal(instance.tab.content, 'Reviewed'); assert.equal(instance.state.activeTabId, sentinel.id);
});

test('same text after undo still has a new generation and rejects an old AI target', async () => {
  const server = service(), instance = app(server, 'A'); await instance.ready();
  const old = await instance.review.capture(instance.tab);
  instance.edit({ from: 1, to: 1, insert: 'B' }); await instance.review.flush(instance.tab);
  server.sessions.get(old.document_id).undoText = 'A';
  assert.equal(instance.review.history(false), true); await instance.review.flush(instance.tab);
  assert.equal(instance.view.text, 'A'); assert.ok(instance.snapshot().generation > old.generation);
  await assert.rejects(instance.review.propose([{ from: 0, to: 1, insert: 'Wrong' }], old, instance.tab), { code: 'review_stale_generation' });
  assert.equal(server.calls.filter(call => call.body.action === 'propose').length, 0);
  assert.equal(server.calls.filter(call => call.body.action === 'undo').length, 1);
});

test('queued Ctrl+Z behind an accepted completion waits for the edit receipt and restores the exact source', async () => {
  const server = service(), instance = app(server, '# Completion\r\n\r\nWe compare controlled evidence to '); await instance.ready();
  const before = instance.view.text, accepted = before + 'form a clear conclusion.', press = reviewKeymap(instance), gate = server.pause('edit');
  server.sessions.get(instance.snapshot().document_id).undoText = before;
  instance.view.dispatch({ changes: { from: before.length, to: before.length, insert: 'form a clear conclusion.' }, userEvent: 'input.complete' });
  await gate.entered.promise; assert.equal(instance.view.text, accepted); assert.equal(press('Mod-z'), true);
  assert.equal(server.calls.some(call => call.body.action === 'undo'), false, 'undo stays behind the in-flight edit');
  gate.release.resolve(); assert.equal(await instance.review.flush(instance.tab), true);
  assert.equal(instance.view.text, before); assert.equal(instance.snapshot().content, before); assert.deepEqual(instance.toasts, []);
  assert.deepEqual(server.calls.map(call => call.body.action), ['open', 'edit', 'undo']);
  assert.equal(server.calls.at(-1).body.generation, 2); assert.equal(server.calls.at(-1).body.review_revision, 1);
});

test('queued undo and redo commands preserve their order and acquire CAS only when sent after the completion receipt', async () => {
  const server = service(), instance = app(server, 'Original source'); await instance.ready();
  const before = instance.view.text, insert = ' + accepted completion', press = reviewKeymap(instance), gate = server.pause('edit');
  Object.assign(server.sessions.get(instance.snapshot().document_id), { undoText: before, redoText: before + insert });
  instance.view.dispatch({ changes: { from: before.length, to: before.length, insert }, userEvent: 'input.complete' });
  await gate.entered.promise; assert.equal(press('Mod-z'), true); assert.equal(press('Mod-y'), true);
  gate.release.resolve(); assert.equal(await instance.review.flush(instance.tab), true);
  const mutations = server.calls.filter(call => call.body.action !== 'open');
  assert.deepEqual(mutations.map(call => call.body.action), ['edit', 'undo', 'redo']);
  assert.deepEqual(mutations.map(call => [call.body.generation, call.body.review_revision]), [[1, 0], [2, 1], [3, 2]]);
  assert.equal(new Set(mutations.map(call => call.body.call_id)).size, 3); assert.equal(instance.view.text, before + insert); assert.deepEqual(instance.toasts, []);
});

test('hand input after queued completion undo survives the receipt and reaches Rust after that undo', async () => {
  const server = service(), instance = app(server, 'Original source'); await instance.ready();
  const before = instance.view.text, insert = ' + accepted completion', hand = ' + later hand input', press = reviewKeymap(instance), gate = server.pause('edit');
  server.sessions.get(instance.snapshot().document_id).undoText = before;
  instance.view.dispatch({ changes: { from: before.length, to: before.length, insert }, userEvent: 'input.complete' });
  await gate.entered.promise; assert.equal(press('Mod-z'), true);
  instance.edit({ from: instance.view.text.length, to: instance.view.text.length, insert: hand });
  gate.release.resolve(); assert.equal(await instance.review.flush(instance.tab), true);
  assert.equal(instance.view.text, before + hand); assert.equal(instance.snapshot().content, before + hand); assert.deepEqual(instance.toasts, []);
  const mutations = server.calls.filter(call => call.body.action !== 'open');
  assert.deepEqual(mutations.map(call => call.body.action), ['edit', 'undo', 'edit']);
  assert.deepEqual(mutations.map(call => [call.body.generation, call.body.review_revision]), [[1, 0], [2, 1], [3, 2]]);
  assert.deepEqual(mutations.at(-1).body.edits, [{ from: before.length, to: before.length, insert: hand }]);
});

test('save receipt maps resource edits once while typing waits behind its save fence', async () => {
  const server = service(), instance = app(server, '![x](old)\nHuman'); await instance.ready();
  const saveToken = await instance.review.prepareSave(instance.tab);
  instance.edit({ from: 15, to: 15, insert: ' C' }); await tick();
  assert.equal(server.calls.filter(call => call.body.action === 'edit').length, 0);
  const saved = server.sessions.get(saveToken.document_id); saved.content = '![x](assets/new)\nHuman'; saved.generation++; saved.review_revision++;
  instance.review.saved(instance.tab, clone(saved), '![x](old)\nHuman', [{ from: 5, to: 8, insert: 'assets/new' }]);
  instance.review.finishSave(instance.tab);
  assert.equal(await instance.review.flush(instance.tab), true);
  assert.equal(instance.view.text, '![x](assets/new)\nHuman C');
  assert.equal(server.calls.filter(call => call.body.action === 'edit').length, 1);
});

test('persistence failure retains draft and retries the exact operation id and payload', async () => {
  const server = service(), instance = app(server, 'A'); await instance.ready(); server.fail('edit');
  instance.edit({ from: 1, to: 1, insert: ' C' });
  assert.equal(await instance.review.flush(instance.tab), false);
  assert.equal(instance.view.text, 'A C'); assert.equal(await instance.review.prepareSave(instance.tab), false);
  assert.equal(await instance.review.retry(instance.tab), true);
  const attempts = server.calls.filter(call => call.body.action === 'edit');
  assert.equal(attempts.length, 2); assert.deepEqual(attempts[0].body, attempts[1].body);
  assert.equal(instance.snapshot().content, 'A C');
});

test('full answer diff splits separated exact changes and leaves emoji boundaries valid', async () => {
  const server = service(), instance = app(server, '甲😀 original\n\nHuman\n\n甲😀 original\n'); await instance.ready();
  const edits = instance.review.documentEdits(instance.view.text, '甲😀 revised\n\nHuman\n\n甲😀 revised\n');
  assert.equal(edits.length, 2);
  assert.equal(editText(instance.view.text, edits), '甲😀 revised\n\nHuman\n\n甲😀 revised\n');
  const emoji = instance.review.documentEdits('x😀y', 'x😁y');
  assert.deepEqual(clone(emoji), [{ from: 1, to: 3, insert: '😁' }]);
});

test('actual AI entry sends only the durable CAS and opaque provider identities', async () => {
  const server = service(), instance = app(server, 'A'); await instance.ready();
  const stamp = await instance.review.capture(instance.tab);
  await instance.review.runAi({ request: 'Revise A', provider: 'opaque-provider', credential_id: 'opaque-credential', model: 'selected-model', base_url: 'must-not-be-sent', headers: { Authorization: 'must-not-be-sent' } }, stamp, instance.tab);
  const call = server.calls.find(call => call.url === '/api/review/ai');
  assert.equal(call.body.document_id, stamp.document_id); assert.equal(call.body.generation, stamp.generation);
  assert.equal(call.body.base_url, undefined); assert.equal(call.body.headers, undefined);
  assert.equal(instance.view.text, 'AI'); assert.equal(instance.snapshot().changes[0].decision, 'pending');
});

test('AI cancellation targets its kernel task and drains later hand input without a proposal', async () => {
  const server = service(), instance = app(server, 'A'); await instance.ready();
  const stamp = await instance.review.capture(instance.tab), barrier = server.pause('ai'), controller = new AbortController();
  const proposal = instance.review.runAi({ request: 'Revise A', task_id: 'owned-review-ai', signal: controller.signal }, stamp, instance.tab);
  await barrier.entered.promise;
  instance.edit({ from: 1, to: 1, insert: ' C' }); controller.abort(); await tick();
  barrier.release.resolve();
  await assert.rejects(proposal, { code: 'cancelled' });
  assert.equal(await instance.review.flush(instance.tab), true);
  assert.equal(instance.view.text, 'A C'); assert.equal(instance.snapshot().changes.length, 0);
  assert.deepEqual(server.calls.find(call => call.url === '/api/task/cancel').body, { id: 'owned-review-ai' });
});

test('restored virtual tabs retain their durable document identity through reviewPath', async () => {
  const server = service(), first = app(server, 'A', { path: '', id: 'old-tab-id' }); await first.ready();
  const id = first.snapshot().document_id, reviewPath = first.tab.reviewPath;
  assert.equal(reviewPath, 'draft:old-tab-id');
  const reopened = app(server, 'A', { path: '', id: 'new-tab-id', reviewPath }); await reopened.ready();
  assert.equal(reopened.snapshot().document_id, id);
  assert.equal(server.calls.at(-1).body.path, 'draft:old-tab-id');
});

test('discard consumes the durable saved body and refuses to close over newer typing', async () => {
  const server = service(), instance = app(server, 'Saved'); await instance.ready();
  const stored = server.sessions.get(instance.snapshot().document_id); stored.diskText = 'Saved';
  instance.edit({ from: 5, to: 5, insert: ' draft' }); await instance.review.flush(instance.tab);
  const barrier = server.pause('discard'), discard = instance.review.discard(instance.tab);
  await barrier.entered.promise;
  instance.edit({ from: 11, to: 11, insert: ' NEW' }); barrier.release.resolve();
  assert.equal(await discard, false);
  assert.equal(await instance.review.flush(instance.tab), true);
  assert.equal(instance.view.text, 'Saved NEW'); assert.equal(instance.tab.original, 'Saved');
});

test('flushAll sends tab operations serially and propagates the first persistence failure', async () => {
  const server = service(), instance = app(server, 'A'); await instance.ready();
  const second = { id: 'second', path: 'Second.md', content: 'B', original: 'B' };
  instance.state.tabs.push(second); await instance.review.ensure(second);
  const secondId = instance.review.snapshot(second).document_id, barrier = server.pause('edit');
  instance.edit({ from: 1, to: 1, insert: ' C' }); await barrier.entered.promise;
  second.content = 'B C'; second.isDirty = true; const all = instance.review.flushAll(); await tick();
  assert.equal(server.calls.some(call => call.body.action === 'edit' && call.body.document_id === secondId), false);
  barrier.release.resolve(); assert.equal(await all, true);
  assert.deepEqual(server.calls.filter(call => call.body.action === 'edit').map(call => call.body.document_id), [instance.snapshot().document_id, secondId]);

  server.fail('edit'); instance.edit({ from: 3, to: 3, insert: ' fail' }); second.content = 'B C later';
  assert.equal(await instance.review.flushAll(), false);
  assert.equal(server.calls.filter(call => call.body.action === 'edit' && call.body.document_id === secondId).length, 1);
  assert.equal(instance.view.text, 'A C fail'); assert.equal(second.content, 'B C later');
});

test('ordered save stages preserve typing and pending B between separated asset rewrites', async () => {
  const server = service(), instance = app(server, '![a](a)\nB\nC\n![b](b)\n'); await instance.ready();
  await instance.review.propose([{ from: 8, to: 9, insert: 'B-reviewed' }], await instance.review.capture(instance.tab), instance.tab);
  const before = '![a](a)\nB-reviewed\nC\n![b](b)\n'; assert.equal(instance.view.text, before);
  const token = await instance.review.prepareSave(instance.tab), changeId = instance.snapshot().changes[0].id;
  instance.edit({ from: 20, to: 20, insert: ' HAND' });
  const stageOne = '![a](new-a)\nB-reviewed\nC\n![b](new-b)\n', stageTwo = '# Saved\n' + stageOne;
  const stages = [{ edits: [{ from: 5, to: 6, insert: 'new-a' }, { from: 26, to: 27, insert: 'new-b' }], content: stageOne },
    { edits: [{ from: 0, to: 0, insert: '# Saved\n' }], content: stageTwo }];
  const stored = server.sessions.get(token.document_id); stored.content = stageTwo; stored.generation += 2; stored.review_revision += 2;
  stored.changes[0].from = 20; stored.changes[0].to = 30;
  // A legacy broad diff is intentionally supplied: ordered review stages must
  // take precedence and retain untouched B/C source spans.
  instance.review.saved(instance.tab, clone(stored), before, [{ from: 0, to: before.length, insert: stageTwo }], stages);
  instance.review.finishSave(instance.tab); assert.equal(await instance.review.flushAll(), true);
  assert.equal(instance.view.text, '# Saved\n![a](new-a)\nB-reviewed\nC HAND\n![b](new-b)\n');
  const change = instance.snapshot().changes.find(item => item.id === changeId);
  assert.equal(change.decision, 'pending'); assert.equal(change.taken_over, false);
  assert.equal(server.calls.filter(call => call.body.action === 'edit').length, 1);
});

test('empty or invalid authoritative save stages never fall back to a broad replacement', async () => {
  const server = service(), instance = app(server, 'Original'); await instance.ready();
  const snapshot = { ...instance.snapshot(), content: 'Unexpected' };
  assert.throws(() => instance.review.saved(instance.tab, snapshot, 'Original', [{ from: 0, to: 8, insert: 'Unexpected' }], []), { code: 'review_conflict' });
  assert.equal(instance.view.text, 'Original');
});

test('read restore does not create a clean unseen session from the auto-fixed reader projection', async () => {
  const server = service(), instance = app(server, 'Reader projection', { original: 'Actual source', mount: false, editing: false });
  assert.equal(await instance.review.restore(instance.tab), null);
  assert.equal(server.sessions.size, 0); assert.equal(server.calls[0].body.action, 'resume');
  assert.equal(instance.tab.content, 'Reader projection'); assert.equal(instance.tab.isDirty, false);
});

test('read restore recovers a durable draft even if crash session metadata still says clean', async () => {
  const server = service(), first = app(server, 'Original'); await first.ready();
  first.edit({ from: 0, to: 8, insert: 'DURABLE DRAFT' }); await first.review.flushAll();
  const restarted = app(server, 'Auto-fixed projection', { original: 'Original', mount: false, editing: false });
  await restarted.review.restore(restarted.tab);
  assert.equal(restarted.tab.content, 'DURABLE DRAFT'); assert.equal(restarted.tab.fixed, 'DURABLE DRAFT');
  assert.equal(restarted.tab.original, 'Original'); assert.equal(restarted.tab.isDirty, true);
  assert.equal(restarted.snapshot().document_id, first.snapshot().document_id);
  assert.equal(server.calls.filter(call => call.body.action === 'open').length, 1);
});

test('100 fast keystrokes coalesce only unsent editor input behind the first in-flight edit', async () => {
  const server = service(), instance = app(server, 'A'); await instance.ready();
  const barrier = server.pause('edit'); instance.edit({ from: 1, to: 1, insert: 'x' }); await barrier.entered.promise;
  for (let i = 0; i < 100; i++) instance.edit({ from: 2 + i, to: 2 + i, insert: 'a' });
  barrier.release.resolve(); assert.equal(await instance.review.flushAll(), true);
  assert.equal(instance.view.text, 'Ax' + 'a'.repeat(100)); assert.equal(instance.snapshot().content, instance.view.text);
  const calls = server.calls.filter(call => call.body.action === 'edit'); assert.equal(calls.length, 2);
  assert.equal(calls[1].body.edits[0].insert, 'a'.repeat(100));
});

test('coalesced unsent edits retain untouched pending review spans between distant changes', async () => {
  const server = service(), instance = app(server, 'left\nB\nright'); await instance.ready();
  await instance.review.propose([{ from: 5, to: 6, insert: 'B-reviewed' }], await instance.review.capture(instance.tab), instance.tab);
  const barrier = server.pause('edit'); instance.edit({ from: 21, to: 21, insert: '\n' }); await barrier.entered.promise;
  instance.edit({ from: 0, to: 4, insert: 'LEFT' }); instance.edit({ from: 16, to: 21, insert: 'RIGHT' });
  barrier.release.resolve(); assert.equal(await instance.review.flushAll(), true);
  assert.equal(instance.view.text, 'LEFT\nB-reviewed\nRIGHT\n');
  const calls = server.calls.filter(call => call.body.action === 'edit'); assert.equal(calls.length, 2);
  assert.deepEqual(calls[1].body.edits, [{ from: 0, to: 4, insert: 'LEFT' }, { from: 16, to: 21, insert: 'RIGHT' }]);
  assert.equal(instance.snapshot().changes[0].decision, 'pending'); assert.equal(instance.snapshot().changes[0].taken_over, false);
});

test('typing then deleting back to identical text never submits an invalid empty coalesced edit', async () => {
  const server = service(), instance = app(server, 'A'); await instance.ready();
  const barrier = server.pause('edit'); instance.edit({ from: 1, to: 1, insert: 'x' }); await barrier.entered.promise;
  instance.edit({ from: 2, to: 2, insert: 'z' }); instance.edit({ from: 2, to: 3, insert: '' });
  barrier.release.resolve(); assert.equal(await instance.review.flushAll(), true);
  assert.equal(instance.view.text, 'Ax');
  const calls = server.calls.filter(call => call.body.action === 'edit');
  assert.equal(calls.length, 3); assert.ok(calls.every(call => call.body.edits.length > 0));
});

test('reader selection A follows its AI replacement into an editor-caret follow-up', async () => {
  const server = service(), instance = app(server, 'A\n\nB', { editing: false }); await instance.ready();
  const getTarget = bindAiTargets(instance, { text: 'A', from: 0, to: 1, isSelection: true, exact: true, source: 'A\n\nB' });
  instance.state.ai.lastSelection = { text: 'A', from: 0, to: 1, source: 'A\n\nB' };
  instance.state.editing = true;
  await instance.review.runAi({ request: 'Revise selected A', ranges: [{ from: 0, to: 1 }] }, await instance.review.capture(instance.tab), instance.tab);
  const staleReader = { contains: () => true };
  instance.context.$ = id => id === 'content' ? staleReader : null;
  instance.window.getSelection = () => ({ rangeCount: 1, isCollapsed: false, getRangeAt: () => ({ startContainer: {}, endContainer: {} }) });
  assert.equal(instance.context.captureAiSelection(), false, 'an editor caret preserves the captured scope');
  let target = getTarget();
  assert.equal(target.isSelection, true); assert.equal(target.exact, true);
  assert.equal(target.text, 'AI'); assert.equal(target.from, 0); assert.equal(target.to, 2); assert.equal(target.source, 'AI\n\nB');
  instance.state.ai.capturedTarget = null;
  target = getTarget(); assert.equal(target.text, 'AI'); assert.equal(target.isSelection, true, 'conversation scope also follows the exact replacement');
  assert.deepEqual(instance.state.ai.lastSelection, { text: 'A', from: 0, to: 1, source: 'A\n\nB' }, 'historical result stamps must remain stale');
});

test('manual prefix, selected typing and appended C map scope without absorbing adjacent text', async () => {
  const server = service(), instance = app(server, 'pre A post\nC'); await instance.ready();
  const getTarget = bindAiTargets(instance, { text: 'A', from: 4, to: 5, isSelection: true, source: 'pre A post\nC' });
  instance.edit({ from: 0, to: 0, insert: 'H' });
  instance.edit({ from: 5, to: 6, insert: 'AX' });
  instance.edit({ from: 14, to: 14, insert: ' manual' });
  instance.edit({ from: 7, to: 7, insert: '!' });
  const target = getTarget();
  assert.equal(target.text, 'AX'); assert.equal(target.from, 5); assert.equal(target.to, 7);
  assert.equal(target.source, 'Hpre AX! post\nC manual'); assert.equal(target.isSelection, true); assert.equal(target.exact, true);
  assert.equal(await instance.review.flushAll(), true);
  assert.equal(getTarget().text, 'AX', 'edit acknowledgements cannot apply a mapping twice');
  const transactions = instance.events.filter(event => event.type === 'readmd:document-transaction');
  assert.equal(transactions.length, 4); assert.ok(transactions.every(event => Object.isFrozen(event.detail) && Object.isFrozen(event.detail.edits) && Object.isFrozen(event.detail.edits[0])));
});

test('a late proposal maps selection through its projection after queued hand input C', async () => {
  const server = service(), instance = app(server, 'A\n\nC'); await instance.ready();
  const getTarget = bindAiTargets(instance, { text: 'A', from: 0, to: 1, isSelection: true, exact: true, source: 'A\n\nC' });
  const stamp = await instance.review.capture(instance.tab), barrier = server.pause('propose');
  const proposal = instance.review.propose([{ from: 0, to: 1, insert: 'AI selected' }], stamp, instance.tab);
  await barrier.entered.promise; instance.edit({ from: 4, to: 4, insert: ' hand' });
  barrier.release.resolve(); await proposal; assert.equal(await instance.review.flushAll(), true);
  const target = getTarget(); assert.equal(target.text, 'AI selected'); assert.equal(target.from, 0); assert.equal(target.to, 11);
  assert.equal(target.source, 'AI selected\n\nC hand'); assert.equal(target.isSelection, true); assert.equal(target.exact, true);
  const transactions = instance.events.filter(event => event.type === 'readmd:document-transaction');
  assert.equal(transactions.length, 2);
  assert.equal(transactions[1].detail.before, 'A\n\nC hand'); assert.equal(transactions[1].detail.after, 'AI selected\n\nC hand');
});

test('another document cannot inherit A scope and a new source selection replaces it', async () => {
  const server = service(), instance = app(server, 'A\n\nC'); await instance.ready();
  const getTarget = bindAiTargets(instance, { text: 'A', from: 0, to: 1, isSelection: true, exact: true, source: 'A\n\nC' });
  const second = { id: 'tab-b', path: 'B.md', original: 'B whole', content: 'B whole' };
  instance.state.tabs.push(second); instance.state.activeTabId = second.id;
  const secondView = instance.makeView(second, 'B whole'); instance.window.cmView = secondView; instance.context.cmView = secondView;
  let target = getTarget(); assert.equal(target.isSelection, false); assert.equal(target.text, 'B whole'); assert.equal(target.tabId, second.id);
  secondView.state.selection.main = { from: 2, to: 7, head: 7, empty: false };
  assert.equal(instance.context.captureAiSelection(), true);
  target = getTarget(); assert.equal(target.isSelection, true); assert.equal(target.text, 'whole'); assert.equal(target.from, 2); assert.equal(target.to, 7);
});

test('stale, inexact and boundary-crossing projections keep selection scope read-only', async () => {
  const server = service(), instance = app(server, 'x A y'); await instance.ready();
  const getTarget = bindAiTargets(instance, { text: 'A', from: 2, to: 3, isSelection: true, exact: true, source: 'x A y' });
  instance.view.text = 'changed A';
  let target = getTarget(); assert.equal(target.isSelection, true); assert.equal(target.exact, false); assert.equal(target.from, -1); assert.equal(target.text, 'A');
  instance.state.ai.capturedTarget = { ...target, source: 'changed A', from: 8, to: 9, text: 'A' };
  target = getTarget(); assert.equal(target.isSelection, true); assert.equal(target.exact, false); assert.equal(target.from, -1);
  instance.view.text = 'x A y';
  instance.state.ai.capturedTarget = { text: 'A', tabId: instance.tab.id, from: 2, to: 3, isSelection: true, exact: true, source: 'x A y' };
  instance.edit({ from: 0, to: 3, insert: 'new' });
  target = getTarget(); assert.equal(target.isSelection, true); assert.equal(target.exact, false); assert.equal(target.from, -1); assert.equal(target.text, 'A');
  assert.equal(await instance.review.flushAll(), true);
  assert.equal(getTarget().isSelection, true, 'an authoritative receipt cannot promote an inexact selection into full scope');
});

function reviewDom() {
  const make = tag => {
    const node = { tagName: tag.toUpperCase(), dataset: {}, children: [], attributes: {}, listeners: {}, open: false, focused: false,
      append(...children) { for (const child of children) { child.parentElement = node; node.children.push(child); } },
      appendChild(child) { node.append(child); return child; },
      setAttribute(name, value) { node.attributes[name] = value; }, removeAttribute(name) { delete node.attributes[name]; },
      addEventListener(name, listener) { node.listeners[name] = listener; }, focus() { node.focused = true; },
      matches(selector) { return selector === 'button' ? node.tagName === 'BUTTON' : selector === 'details' ? node.tagName === 'DETAILS' : selector === '[data-review-action]' ? Boolean(node.dataset.reviewAction) : selector.startsWith('.') && (node.className || '').split(/\s+/).includes(selector.slice(1)); },
      closest(selector) { return node.matches(selector) ? node : node.parentElement?.closest(selector) || null; },
      querySelectorAll(selector) { return node.children.flatMap(child => [...(child.matches(selector) ? [child] : []), ...child.querySelectorAll(selector)]); } };
    return node;
  };
  return { getElementById: () => null, createElement: make, createElementNS: (_ns, tag) => make(tag) };
}
function reviewWidgets(instance) {
  const CM = { ...instance.window.ReadMDCodeMirror, WidgetType: class {},
    ViewPlugin: { fromClass: value => value }, StateEffect: { define: () => ({ of: value => value }) },
    Decoration: { mark: spec => ({ range: (from, to) => ({ spec, from, to }) }), widget: spec => ({ range: at => ({ ...spec, at }) }), set: values => values },
    Prec: { highest: value => value }, keymap: { of: value => value } };
  instance.window.ReadMDCodeMirror = CM;
  const [Plugin] = instance.review.extension(CM, instance.tab);
  return new Plugin(instance.view).decorations.filter(value => value.widget).map(value => value.widget);
}
function clickReview(panel, action) {
  const target = panel.querySelectorAll('button').find(button => button.dataset.reviewAction === action);
  assert.ok(target, 'review action exists: ' + action);
  panel.listeners.click({ target, preventDefault() {}, stopPropagation() {} }); return target;
}

test('hand-owned comparison requires an explicit keep-current choice and retains accepted comparison access', async () => {
  const server = service(), instance = app(server, 'A', { document: reviewDom() }); await instance.ready();
  await instance.review.propose([{ from: 0, to: 1, insert: 'AI' }], await instance.review.capture(instance.tab), instance.tab);
  const id = instance.snapshot().changes[0].id;
  instance.edit({ from: 0, to: 2, insert: 'HUMAN' }); assert.equal(await instance.review.flushAll(), true);
  await assert.rejects(instance.review.resolve([id], 'accept', instance.tab), { code: 'review_conflict' });
  await assert.rejects(instance.review.resolve([id], 'withdraw', instance.tab), { code: 'review_conflict' });
  assert.equal(instance.view.text, 'HUMAN');
  const defaults = server.calls.filter(call => call.body.action === 'resolve'); assert.equal(defaults.length, 2);
  assert.ok(defaults.every(call => !Object.hasOwn(call.body, 'resolution')), 'default actions cannot grant keep-current consent');

  let widget = reviewWidgets(instance)[0], panel = widget.toDOM();
  assert.equal(widget.change.conflict, true); assert.equal(widget.change.canCompare, true);
  clickReview(panel, 'withdraw'); await tick();
  assert.equal(panel.querySelectorAll('details')[0].open, true);
  assert.equal(server.calls.filter(call => call.body.action === 'resolve').length, 2, 'withdraw opens comparison without a mutation');
  assert.equal(panel.querySelectorAll('button').find(button => button.dataset.reviewAction === 'keep-current').focused, true);
  clickReview(panel, 'keep-current'); await tick(); assert.equal(await instance.review.flushAll(), true);
  const accepted = instance.snapshot().changes[0];
  assert.equal(accepted.decision, 'accepted'); assert.equal(accepted.taken_over, true);
  assert.equal(accepted.original, 'A'); assert.equal(accepted.replacement, 'AI'); assert.equal(instance.view.text, 'HUMAN');
  const choice = server.calls.filter(call => call.body.action === 'resolve').at(-1).body;
  assert.equal(choice.decision, 'accept'); assert.equal(choice.resolution, 'keep_current'); assert.deepEqual(choice.ids, [id]);
  assert.ok(choice.call_id); assert.equal(choice.document_id, instance.snapshot().document_id);

  widget = reviewWidgets(instance)[0]; panel = widget.toDOM();
  assert.equal(widget.change.conflict, false); assert.equal(widget.change.canCompare, true);
  assert.notEqual(panel.attributes['aria-label'], 'review.pending');
  assert.equal(panel.querySelectorAll('.review-comparison').length, 1);
  clickReview(panel, 'withdraw'); await tick(); assert.equal(panel.querySelectorAll('details')[0].open, true);
  assert.equal(server.calls.filter(call => call.body.action === 'resolve').length, 3, 'accepted hand ownership remains protected');
});

test('session recovery reconciles a newer unacknowledged generic draft only against its exact review watermark', async () => {
  const server = service(), history = historyStore(), disk = 'A\n\nC';
  const first = app(server, disk); await first.ready();
  await first.review.propose([{ from: 0, to: 1, insert: 'A-reviewed' }], await first.review.capture(first.tab), first.tab);
  await first.review.resolve([first.snapshot().changes[0].id], 'accept', first.tab);
  const recovery = installRecovery(first, server, history), acknowledged = clone(first.snapshot());
  server.fail('edit'); first.edit({ from: 13, to: 13, insert: ' HAND' });
  assert.equal(await first.review.flush(first.tab), false); assert.equal(await recovery.flush(), true);
  const generic = [...history.entries.values()].find(row => row.entry.kind === 'draft');
  assert.equal(generic.content, 'A-reviewed\n\nC HAND');
  assert.deepEqual(generic.entry.context.review, { document_id: acknowledged.document_id, generation: acknowledged.generation, review_revision: acknowledged.review_revision, revision: acknowledged.revision });
  const restarted = app(server, disk, { mount: false, editing: false }); installRecovery(restarted, server, history);
  const tab = await restoreSession(restarted, disk, { key: generic.entry.doc_key });
  assert.deepEqual(clone(tab._recoveryReview), generic.entry.context.review);
  assert.equal(tab.content, 'A-reviewed\n\nC HAND'); assert.equal(await restarted.review.flushAll(), true);
  const snapshot = restarted.review.snapshot(tab); assert.equal(snapshot.content, tab.content); assert.equal(snapshot.changes[0].decision, 'accepted');
  assert.equal(snapshot.changes[0].taken_over, false, 'the separate accepted A span remains owned by its review');
  assert.deepEqual(server.calls.filter(call => call.body.action === 'edit').at(-1).body.edits, [{ from: 13, to: 13, insert: ' HAND' }]);
  assert.equal(history.calls.some(call => call?.kind === 'checkpoint'), false);
});

test('a failed matching-watermark reconciliation retains the newer body and retries the same CAS operation', async () => {
  const server = service(), disk = 'A\nC', first = app(server, disk); await first.ready();
  const mark = clone(first.snapshot()), body = 'A\nC HAND';
  const restarted = app(server, body, { original: disk, isDirty: true, mount: false, editing: false, recoveryContent: body, recoveryReview: mark });
  server.fail('edit', 'review_budget'); await restarted.review.restore(restarted.tab);
  assert.equal(restarted.tab.content, body); assert.equal(restarted.snapshot().content, disk); assert.equal(await restarted.review.flushAll(), false);
  assert.equal(await restarted.review.retry(restarted.tab), true); assert.equal(restarted.snapshot().content, body);
  const attempts = server.calls.filter(call => call.body.action === 'edit'); assert.equal(attempts.length, 2);
  assert.deepEqual(attempts[0].body, attempts[1].body);
});

for (const legacy of [false, true]) test((legacy ? 'missing' : 'stale') + ' generic watermark checkpoints the older body before adopting newer SQLite state', async () => {
  const server = service(), history = historyStore(), disk = 'Disk';
  const first = app(server, disk); await first.ready();
  const older = 'Older generic draft', newer = 'Newer reviewed body', olderMark = clone(first.snapshot());
  await history.fetch('/api/documents/history', { body: JSON.stringify({ op: 'record', key: 'Review.md', path: 'Review.md', name: 'Review.md', kind: 'draft', reason: 'edit', content: older, context: legacy ? {} : { review: olderMark } }) });
  await first.review.propose([{ from: 0, to: 4, insert: newer }], await first.review.capture(first.tab), first.tab);
  const restarted = app(server, disk, { mount: false, editing: false }); installRecovery(restarted, server, history);
  const tab = await restoreSession(restarted, disk, { dirty: !legacy });
  assert.equal(tab.content, newer); assert.equal(restarted.review.snapshot(tab).content, newer);
  const copies = [...history.entries.values()].filter(row => row.entry.kind === 'checkpoint');
  assert.equal(copies.length, 1); assert.equal(copies[0].content, older); assert.equal(copies[0].entry.reason, 'review_recovery_conflict');
  assert.equal(tab._reviewRestorePending, false); assert.equal(server.calls.some(call => call.body.action === 'edit'), false);
  assert.ok(restarted.toasts.includes('review.conflict'));
});

test('failed legacy preservation leaves its body visible, prevents draft clearing, and can retry after storage recovery', async () => {
  const server = service(), history = historyStore(), disk = 'Disk', first = app(server, disk); await first.ready();
  const older = 'Generic body must survive', newer = 'Newer SQLite view';
  await history.fetch('/api/documents/history', { body: JSON.stringify({ op: 'record', key: 'Review.md', path: 'Review.md', name: 'Review.md', kind: 'draft', reason: 'edit', content: older, context: {} }) });
  await first.review.propose([{ from: 0, to: 4, insert: newer }], await first.review.capture(first.tab), first.tab);
  history.fail('checkpoint');
  const restarted = app(server, disk, { mount: false, editing: false }), recovery = installRecovery(restarted, server, history);
  const tab = await restoreSession(restarted, disk, { dirty: false });
  assert.equal(tab.content, older); assert.equal(tab.isDirty, true); assert.equal(tab._reviewRestorePending, true);
  assert.equal(restarted.review.snapshot(tab), null, 'an unproven generic body cannot acquire the newer acknowledgement watermark');
  assert.equal(await recovery.flush(), false); assert.equal(await restarted.review.flushAll(), false);
  assert.equal(history.calls.some(call => call?.op === 'clear_draft'), false);
  assert.equal([...history.entries.values()].find(row => row.entry.kind === 'draft').content, older);
  tab.content = tab.fixed = older + ' NEW TYPING';
  assert.equal(await restarted.review.flush(tab), false);
  history.recover(); assert.equal(await restarted.review.retry(tab), true);
  assert.equal(tab.content, newer); assert.equal(tab._reviewRestorePending, false);
  const copies = [...history.entries.values()].filter(row => row.entry.kind === 'checkpoint');
  assert.deepEqual(copies.map(row => row.content), [older, older + ' NEW TYPING']);
  assert.ok(copies.every(row => row.entry.reason === 'review_recovery_conflict'));
  assert.equal(server.calls.some(call => call.body.action === 'edit'), false);
});

test('autosaving while the first open is in flight does not misclassify live typing as a restored legacy draft', async () => {
  const server = service(), history = historyStore(), instance = app(server, 'A', { mount: false });
  const recovery = installRecovery(instance, server, history), barrier = server.pause('open');
  instance.review.mount(instance.view, instance.tab, 'A'); await barrier.entered.promise;
  instance.edit({ from: 1, to: 1, insert: ' LIVE' }); assert.equal(await recovery.flush(), true);
  barrier.release.resolve(); assert.equal(await instance.review.flushAll(), true);
  assert.equal(instance.view.text, 'A LIVE'); assert.equal(instance.snapshot().content, 'A LIVE');
  assert.equal(history.calls.some(call => call?.kind === 'checkpoint'), false);
});
