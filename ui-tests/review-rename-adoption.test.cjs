'use strict';
// These deterministic protocol tests exercise the production review and AI
// adapters. They do not establish filesystem, Rust or provider correctness.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const reviewSource = fs.readFileSync(path.join(__dirname, '../assets/js/editor/review.js'), 'utf8');
const aiSource = fs.readFileSync(path.join(__dirname, '../assets/js/features/ai-harness.js'), 'utf8');
const clone = value => JSON.parse(JSON.stringify(value));
const turn = () => new Promise(resolve => setImmediate(resolve));
const OLD = 'C:/Synthetic/Review-Rename/Original.md', NEW = 'C:/Synthetic/Review-Rename/Renamed.md';
const BODY = '# Synthetic review\n';
function deferred() {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
}
function response(value, ok = true) { return { ok, status: ok ? 200 : 503, json: async () => clone(value) }; }
const scenario = (name, work) => test('review rename adoption: ' + name, { timeout: 3000 }, work);

async function fixture({ ai = false } = {}) {
  const tab = { id: 'synthetic-tab', path: OLD, name: 'Original.md', original: BODY, content: BODY,
    fixed: BODY, isDirty: false, editGeneration: 0 };
  const other = { id: 'synthetic-other', path: 'C:/Synthetic/Review-Rename/Other.md', original: 'Other\n', content: 'Other\n' };
  const state = { tabs: [tab, other], activeTabId: tab.id, editing: false };
  const calls = [], toasts = [], events = [], listeners = new Map(), tasks = new Map(), timers = new Map();
  let serial = 0, hold, failRead = false;
  const server = { head: { document_id: 'synthetic-document', path: OLD, content: BODY,
    generation: 3, review_revision: 7, revision: 'synthetic-content-3', changes: [], can_undo: true, can_redo: true } };
  const listen = (type, callback) => { if (!listeners.has(type)) listeners.set(type, []); listeners.get(type).push(callback); };
  const window = { READMD_ENGINE: 'rust', __STARTUP_PROBE__: true,
    crypto: { randomUUID: () => 'synthetic-call-' + (++serial) }, i18n: { t: key => key },
    addEventListener: listen, dispatchEvent(event) {
      events.push(event);
      for (const callback of listeners.get(event.type) || []) callback(event);
    } };
  const document = { readyState: 'loading', getElementById: () => null, addEventListener: listen };
  async function apiFetch(url, options) {
    const body = JSON.parse(options.body); calls.push({ url, body: clone(body) });
    if (url === '/api/ai/tasks') {
      if (body.action === 'list') return response({ tasks: [...tasks.values()] });
      assert.equal(body.action, 'status', 'The fixture never starts or resumes a provider');
      return response({ task: tasks.get(body.task_id), snapshot: clone(server.head) });
    }
    assert.equal(url, '/api/review');
    let result;
    if (body.action === 'read' && failRead) {
      failRead = false;
      result = response({ error_code: 'review_persistence_failed', error: 'Synthetic read failure' }, false);
    } else {
      if (body.action === 'undo' || body.action === 'redo') server.head.review_revision++;
      else assert.ok(['open', 'read'].includes(body.action), 'Only explicit review protocol calls are permitted');
      result = response(clone(server.head));
    }
    // Capture the response before blocking its delivery, including the path.
    if (hold?.action === body.action) {
      const barrier = hold; hold = null;
      barrier.entered.resolve(); await barrier.release.promise;
    }
    return result;
  }
  const context = vm.createContext({ window, document, state, TextEncoder, Uint32Array, apiFetch,
    CustomEvent: class { constructor(type, value) { this.type = type; Object.assign(this, value); } },
    getActiveTab: () => state.tabs.find(value => value.id === state.activeTabId),
    showToast: value => toasts.push(value), renderTabsBar() {}, updateUnloadGuard() {},
    setTimeout(callback, delay) { const id = ++serial; timers.set(id, { callback, delay }); return id; },
    clearTimeout: id => timers.delete(id) });
  vm.runInContext(reviewSource, context, { filename: 'assets/js/editor/review.js' });
  const review = window.ReadMDReview;
  await review.ensure(tab); assert.equal(await review.flush(tab), true);
  if (ai) vm.runInContext(aiSource, context, { filename: 'assets/js/features/ai-harness.js' });
  calls.length = 0; events.length = 0;
  return { tab, other, state, server, review, window, calls, toasts, events, tasks,
    snapshot: () => review.snapshot(tab),
    reads: () => calls.filter(call => call.url === '/api/review' && call.body.action === 'read'),
    pause(action) { const barrier = { action, entered: deferred(), release: deferred() }; hold = barrier; return barrier; },
    failNextRead() { failRead = true; },
    move(to = NEW) {
      server.head.path = to;
      assert.equal(review.renamed(tab, clone(server.head)), true);
      tab.path = to; tab.name = to.split('/').pop();
    },
    aiResult() {
      server.head.content = '# Completed synthetic AI review\n';
      server.head.generation++; server.head.review_revision++;
      server.head.changes = [{ id: 'synthetic-candidate', from: 2, to: 11, original: 'Synthetic', replacement: 'Completed',
        decision: 'pending', application: 'applied', version: 1 }];
    } };
}

scenario('a queued adoption in the flush continuation prevents a rename reservation', async () => {
  const app = await fixture();
  let adoption;
  const reservation = app.review.beginRename(app.tab);
  // flush has passed its first adoption check, but beginRename has not resumed.
  queueMicrotask(() => { adoption = app.review.adopt(clone(app.server.head), app.tab); });
  assert.equal(await reservation, null);
  assert.equal(await adoption, true);
  assert.equal(app.snapshot().path, OLD);
  const retry = await app.review.beginRename(app.tab); assert.ok(retry);
  app.move(); app.review.endRename(app.tab);
  assert.equal(app.snapshot().path, NEW);
});

scenario('an in-flight read blocks receipt rebinding until its ACK is consumed', async () => {
  const app = await fixture(), barrier = app.pause('read');
  const adoption = app.review.adopt(clone(app.server.head), app.tab); await barrier.entered.promise;
  let reserved = false;
  const reservation = app.review.beginRename(app.tab).then(value => { reserved = true; return value; });
  await turn();
  assert.equal(reserved, false);
  assert.equal(app.review.renamed(app.tab, { ...clone(app.server.head), path: NEW }), false);
  barrier.release.resolve(); assert.equal(await adoption, true); assert.ok(await reservation);
  app.move(); app.review.endRename(app.tab);
  assert.equal(app.snapshot().path, NEW);
  assert.equal(app.toasts.length, 0);
});

scenario('adoption is fenced again when its queued callback starts', async () => {
  const app = await fixture();
  const adoption = app.review.adopt(clone(app.server.head), app.tab);
  app.tab._renamePending = { operation_id: 'synthetic-rename' };
  assert.equal(await adoption, false);
  assert.equal(app.reads().length, 0);
  app.move(); delete app.tab._renamePending; app.review.endRename(app.tab);
  await turn(); assert.equal(await app.review.flush(app.tab), true);
  assert.equal(app.reads().length, 1); assert.equal(app.snapshot().path, NEW);
});

for (const outcome of ['success', 'read error']) scenario('a delayed old-path ' + outcome + ' cannot apply after pending rename appears', async () => {
  const app = await fixture(), barrier = app.pause('read');
  if (outcome === 'read error') app.failNextRead();
  const adoption = app.review.adopt(clone(app.server.head), app.tab); await barrier.entered.promise;
  app.tab._renamePending = { operation_id: 'synthetic-rename' };
  barrier.release.resolve(); assert.equal(await adoption, false);
  assert.equal(app.snapshot().path, OLD); assert.equal(app.toasts.length, 0);
  app.move(); app.aiResult(); delete app.tab._renamePending; app.review.endRename(app.tab);
  await turn(); assert.equal(await app.review.flush(app.tab), true);
  assert.equal(app.snapshot().path, NEW); assert.equal(app.tab.content, '# Completed synthetic AI review\n');
  assert.equal(app.snapshot().changes[0].id, 'synthetic-candidate'); assert.equal(app.reads().length, 2);
});

scenario('rename arrivals coalesce and a completed AI task is reread at the new head', async () => {
  const app = await fixture({ ai: true });
  const task = { id: 'synthetic-task', document_id: app.snapshot().document_id, state: 'completed',
    rounds: 1, elapsed_ms: 5, last_event_id: 2 };
  app.tasks.set(task.id, task); assert.equal(await app.window.ReadMDAiHarness.init(), true);
  assert.ok(await app.review.beginRename(app.tab));
  app.tab._renamePending = { operation_id: 'synthetic-rename' };
  assert.equal((await app.window.ReadMDAiHarness.status(task.id)).state, 'completed');
  assert.equal(await app.review.adopt(clone(app.server.head), app.tab), false);
  assert.equal(await app.review.adopt(clone(app.server.head), app.tab), false);
  assert.equal(app.reads().length, 0); assert.equal(app.snapshot().content, BODY);
  app.move(); app.aiResult(); delete app.tab._renamePending;
  // The real tab lifecycle clears this after endRename returns. It cannot
  // prevent the post-settlement reread from making progress.
  app.tab._renameInFlight = true; app.review.endRename(app.tab);
  delete app.tab._renameInFlight;
  await turn(); assert.equal(await app.review.flush(app.tab), true);
  assert.equal(app.reads().length, 1); assert.equal(app.snapshot().path, NEW);
  assert.equal(app.snapshot().changes[0].id, 'synthetic-candidate');
  assert.equal(app.tab.content, '# Completed synthetic AI review\n');
  assert.equal(app.window.ReadMDAiHarness.list()[0].state, 'completed');
  assert.equal(app.calls.some(call => /start|resume|pause/.test(call.body.action)), false);
});

scenario('a failed deferred read keeps the completed result available for explicit retry', async () => {
  const app = await fixture(); assert.ok(await app.review.beginRename(app.tab));
  assert.equal(await app.review.adopt(clone(app.server.head), app.tab), false);
  app.move(); app.aiResult(); app.failNextRead(); app.review.endRename(app.tab);
  await turn();
  assert.equal(app.snapshot().content, BODY); assert.equal(app.toasts.length, 1);
  assert.equal(await app.review.retry(app.tab), true);
  assert.equal(app.reads().length, 2); assert.equal(app.snapshot().path, NEW);
  assert.equal(app.snapshot().changes[0].id, 'synthetic-candidate');
  assert.equal(app.tab.content, '# Completed synthetic AI review\n');
});

scenario('the running pump stops before its next write when rename becomes pending', async () => {
  const app = await fixture(), barrier = app.pause('undo');
  assert.equal(app.review.history(false), true); await barrier.entered.promise;
  assert.equal(app.review.history(true), true);
  app.tab._renamePending = { operation_id: 'synthetic-rename' }; barrier.release.resolve();
  await turn();
  assert.deepEqual(app.calls.map(call => call.body.action), ['undo']);
  assert.equal(await app.review.flush(app.tab), false, 'Flush must fail promptly instead of spinning on a fenced queue');
  delete app.tab._renamePending; app.review.endRename(app.tab);
  assert.equal(await app.review.flush(app.tab), true);
  assert.deepEqual(app.calls.map(call => call.body.action), ['undo', 'redo']);
});

scenario('deferred results stay scoped to their original background tab and two rename cycles', async () => {
  const app = await fixture(); app.state.activeTabId = app.other.id;
  for (const destination of [NEW, 'C:/Synthetic/Review-Rename/Again.md']) {
    assert.ok(await app.review.beginRename(app.tab));
    assert.equal(await app.review.adopt(clone(app.server.head), app.tab), false);
    app.move(destination); app.review.endRename(app.tab);
    await turn(); assert.equal(await app.review.flush(app.tab), true);
    assert.equal(app.snapshot().path, destination);
  }
  assert.equal(app.reads().length, 2); assert.equal(app.state.activeTabId, app.other.id);
  assert.equal(app.other.content, 'Other\n');
});

scenario('closing a tab with a deferred signal never reopens or mutates another document', async () => {
  const app = await fixture(); assert.ok(await app.review.beginRename(app.tab));
  assert.equal(await app.review.adopt(clone(app.server.head), app.tab), false);
  app.state.tabs = [app.other]; app.state.activeTabId = app.other.id; app.review.endRename(app.tab);
  await turn(); assert.equal(app.reads().length, 0);
  assert.equal(app.other.content, 'Other\n'); assert.equal(app.toasts.length, 0);
});
