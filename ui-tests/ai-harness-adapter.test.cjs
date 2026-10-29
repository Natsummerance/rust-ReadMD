'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { randomUUID } = require('node:crypto');
const harnessSource = fs.readFileSync(path.join(__dirname, '../assets/js/features/ai-harness.js'), 'utf8');
const aiSource = fs.readFileSync(path.join(__dirname, '../assets/js/features/ai.js'), 'utf8');
const reviewSource = fs.readFileSync(path.join(__dirname, '../assets/js/editor/review.js'), 'utf8');
const tabsSource = fs.readFileSync(path.join(__dirname, '../assets/js/core/tabs.js'), 'utf8');
const recoverySource = fs.readFileSync(path.join(__dirname, '../assets/js/features/document-history.js'), 'utf8');
const clone = value => JSON.parse(JSON.stringify(value));
function barrier() { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; }
class Element {
  constructor(name) { this.name = name; this.children = []; this.listeners = new Map(); this.hidden = false; this.textContent = ''; this.classes = new Set(); this.value = ''; }
  appendChild(child) { this.children.push(child); child.parent = this; return child; }
  replaceChildren() { this.children = []; }
  insertAdjacentElement(_position, child) { this.parent.appendChild(child); }
  setAttribute(name, value) { this[name] = value; }
  addEventListener(name, callback) { this.listeners.set(name, callback); }
  querySelector() { return null; }
  focus() {}
  get classList() { const set = this.classes; return { contains: name => set.has(name), add: name => set.add(name), remove: name => set.delete(name), toggle(name) { if (set.has(name)) { set.delete(name); return false; } set.add(name); return true; } }; }
}
function fixture(options = {}) {
  const calls = [], adopted = [], toasts = [], timers = new Map(), events = new Map(), service = new Map(); let serial = 0, holdStatus;
  const tabA = { id: 'tab-a', path: 'A.md', name: 'A.md', original: 'A source', content: 'A source', reviewSession: { document_id: 'document-a' } };
  const tabB = { id: 'tab-b', path: 'B.md', name: 'B.md', original: 'B source', content: 'B source', reviewSession: { document_id: 'document-b' } };
  const state = { tabs: [tabA, tabB], activeTabId: tabA.id, editing: false, ai: { config: {}, messages: [] } };
  const root = new Element('root'), output = root.appendChild(new Element('div')); output.id = 'ai-output';
  for (const id of ['ai-panel', 'ai-prompt']) { const child = root.appendChild(new Element('div')); child.id = id; }
  const find = (node, id) => node.id === id ? node : node.children.map(child => find(child, id)).find(Boolean);
  const document = { readyState: 'loading', activeElement: null, getElementById: id => find(root, id) || null, createElement: name => new Element(name),
    addEventListener(type, callback) { if (!events.has(type)) events.set(type, []); events.get(type).push(callback); } };
  const review = { capture: async tab => ({ tabId: tab.id, document_id: tab.reviewSession.document_id, content: tab.content, generation: 1, review_revision: 0, editGeneration: 0 }),
    snapshot: tab => tab ? { document_id: tab.reviewSession.document_id, content: tab.content, generation: 1, review_revision: 0, changes: [] } : null,
    adopt: async (snapshot, tab) => { adopted.push({ snapshot: clone(snapshot), tab }); return true; }, flush: async () => true, flushAll: async () => true };
  const window = { READMD_ENGINE: 'rust', crypto: { randomUUID }, ReadMDReview: review, i18n: { currentLang: 'zh-CN', t: (key, params) => key + (params ? JSON.stringify(params) : '') },
    addEventListener: document.addEventListener, dispatchEvent(event) { for (const listener of events.get(event.type) || []) listener(event); },
    ipc: { postMessage: value => calls.push({ action: 'ipc', value }) }, ReadMDSession: { flush: async () => true, persist() {} } };
  async function apiFetch(url, opts) {
    if (url === '/api/documents/history') return { ok: true, json: async () => ({ ok: true, entries: [] }) };
    assert.equal(url, '/api/ai/tasks');
    const body = JSON.parse(opts.body); calls.push(clone(body));
    if (body.action === 'list' && options.failList) return { ok: false, json: async () => ({ ok: false, error_code: 'ai_task_failed' }) };
    if (body.action === 'pause' && options.failPause) return { ok: false, json: async () => ({ ok: false, error_code: 'ai_task_failed' }) };
    let data;
    if (body.action === 'list') data = { tasks: [...service.values()] };
    else if (body.action === 'start') {
      const task = { id: 'task-' + (++serial), document_id: body.doc_id, state: options.completeOnStart ? 'completed' : 'running', rounds: 0, elapsed_ms: 0, last_event_id: 1,
        private_provider: 'https://private.example/secret' };
      service.set(task.id, task); data = { task, events: [] };
    } else {
      const task = service.get(body.task_id); assert.ok(task);
      if (body.action === 'pause') { task.state = 'paused'; task.last_event_id++; }
      if (body.action === 'resume') { task.state = 'running'; task.last_event_id++; }
      data = { task, events: [{ id: task.last_event_id, type: task.state, private_text: 'private tool body' }] };
      if (body.action === 'status' && options.receipt) data.snapshot = { document_id: task.document_id, content: 'AI changed source', generation: 2, review_revision: 1, changes: [] };
    }
    const captured = clone(data);
    if (body.action === 'status' && holdStatus) { const hold = holdStatus; holdStatus = null; hold.entered.resolve(); await hold.release.promise; }
    return { ok: true, json: async () => captured };
  }
  const context = vm.createContext({ window, document, state, Element, crypto: { randomUUID }, CustomEvent: class { constructor(type, value) { this.type = type; Object.assign(this, value); } },
    setTimeout(callback, delay) { const timer = ++serial; timers.set(timer, { callback, delay }); return timer; }, clearTimeout: timer => timers.delete(timer),
    getActiveTab: () => state.tabs.find(tab => tab.id === state.activeTabId), apiFetch, showToast: text => toasts.push(text),
    $: id => document.getElementById(id), moduleBlocked: () => false, captureAiSelection() {}, updateAiUsage() {}, loadAiPrompts() {}, loadAiSessions() {}, renderAiEmptyState() {}, updateAiContextStrip() {},
    exitEdit() { state.editing = false; }, renderTabsBar() {}, focusVisibleTab() {}, switchTab: async id => { state.activeTabId = id; },
    syncActiveTabDirty() {}, promptDirtyClose: async () => 'cancel', goHome: async () => {}, getEditContent: () => state.tabs.find(tab => tab.id === state.activeTabId)?.content || '',
    syncStateFromActiveTab() {}, renderActiveTab: async () => {}, resetHome() {}, setInterval() {},
  });
  vm.runInContext(harnessSource, context);
  vm.runInContext(aiSource.slice(aiSource.indexOf('function toggleAiPanel()'), aiSource.indexOf('\nfunction handleTopAiButtonClick')), context);
  vm.runInContext(tabsSource.slice(tabsSource.indexOf('async function closeTab('), tabsSource.indexOf('\nasync function closeOtherTabs(')), context);
  vm.runInContext(recoverySource, context);
  const harness = window.ReadMDAiHarness;
  return { harness, context, window, document, state, calls, adopted, toasts, service, tabA, tabB,
    async start(tab = tabA, overrides = {}) { const stamp = await review.capture(tab); return harness.start({ request: 'Polish exactly this selection', provider: 'configured-provider', credential_id: 'opaque-ref', model: 'configured-model', skill_id: 'user-selected-skill', ranges: [{ from: 0, to: 1 }], ...overrides }, stamp, tab); },
    holdStatus() { const hold = { entered: barrier(), release: barrier() }; holdStatus = hold; return hold; },
    emit(type) { window.dispatchEvent({ type }); },
  };
}

async function runModify(app, stream) {
  const root = app.document.getElementById('ai-output').parent;
  for (const id of ['ai-incognito', 'ai-model', 'ai-mode', 'ai-base-url', 'ai-stream', 'ai-endpoint-mode']) { const node = root.appendChild(new Element('input')); node.id = id; }
  app.document.getElementById('ai-stream').checked = stream; app.document.getElementById('ai-model').value = 'configured-model'; app.document.getElementById('ai-prompt').value = 'Polish the document';
  const provider = { id: 'configured-provider', credential_id: 'opaque-ref', models: ['configured-model'] }; app.state.editing = true;
  Object.assign(app.context, { ReadMDReview: app.window.ReadMDReview, getCurrentDocumentText: () => app.tabA.content,
    getAiTargetText: () => ({ text: app.tabA.content, source: app.tabA.content, isSelection: false, exact: true, from: 0, to: app.tabA.content.length }),
    ensureAiConfigured: async () => provider, currentAiProvider: () => provider, currentAiTemplate: () => ({ skill_id: 'user-selected-skill' }),
    saveAiSelection: async () => true, readAiCustomHeaders: () => ({}), setAiBusy: value => { app.state.ai.busy = value; }, updateAiRawButtons() {} });
  vm.runInContext(aiSource.slice(aiSource.indexOf('async function runAiRequest('), aiSource.indexOf('\nasync function testAiConnection(')), app.context);
  await app.context.runAiRequest('modify');
}

async function legacyReviewRequest(options) {
  const calls = [], tab = { id: 'legacy-tab', path: 'Legacy.md', original: 'Original source', content: 'Original source' };
  const snapshot = { document_id: 'legacy-document', generation: 1, review_revision: 0, content: tab.content, changes: [] };
  const window = { READMD_ENGINE: 'legacy', crypto: { randomUUID }, i18n: { t: key => key } };
  const context = vm.createContext({ window, state: { tabs: [tab], editing: false }, TextEncoder, Uint32Array,
    document: { getElementById: () => null }, getActiveTab: () => tab,
    apiFetch: async (url, request) => { calls.push({ url, body: JSON.parse(request.body) }); return { ok: true, json: async () => clone(snapshot) }; } });
  vm.runInContext(reviewSource, context); const stamp = await window.ReadMDReview.capture(tab);
  await window.ReadMDReview.runAi({ request: 'Polish the document', ...options }, stamp, tab);
  return calls.find(call => call.url === '/api/review/ai').body;
}

for (const stream of [false, true, undefined]) test('existing stream preference reaches task and review requests: ' + (stream === undefined ? 'unspecified' : stream), async () => {
  const options = stream === undefined ? {} : { stream };
  const assertStream = body => { assert.equal(Object.hasOwn(body, 'stream'), stream !== undefined); if (stream !== undefined) assert.equal(body.stream, stream); };
  const direct = fixture(); await direct.start(direct.tabA, options); assertStream(direct.calls.find(call => call.action === 'start'));
  const wrapped = fixture({ completeOnStart: true });
  vm.runInContext(reviewSource.slice(reviewSource.indexOf('  async function runAi('), reviewSource.indexOf('  async function cancelAi(')), wrapped.context);
  const stamp = await wrapped.window.ReadMDReview.capture(wrapped.tabA);
  await wrapped.context.runAi({ request: 'Polish the document', ...options }, stamp, wrapped.tabA); assertStream(wrapped.calls.find(call => call.action === 'start'));
  assertStream(await legacyReviewRequest(options));
  if (stream !== undefined) { const ui = fixture(); await runModify(ui, stream); assertStream(ui.calls.find(call => call.action === 'start')); assert.deepEqual(ui.toasts, []); }
});

test('startup discovery never starts/resumes a provider; paused tasks resume only through an explicit control', async () => {
  const app = fixture();
  app.service.set('restored', { id: 'restored', document_id: 'document-a', state: 'needs_user_resume', rounds: 13, elapsed_ms: 10100, last_event_id: 14 });
  assert.equal(await app.harness.init(), true);
  assert.deepEqual(app.calls.map(call => call.action), ['list']);
  const host = app.document.getElementById('ai-task-status'), control = host.children[0].children[1];
  assert.equal(control.textContent, 'ai.taskResume');
  await control.listeners.get('click')();
  assert.deepEqual(app.calls.map(call => call.action), ['list', 'resume']);
  assert.equal(app.harness.list()[0].state, 'running');
});

test('a failed network task remains recoverable in history and resumes its original task and cumulative rounds only after an explicit click', async () => {
  const app = fixture(), originalTranslate = app.window.i18n.t;
  app.window.i18n.t = (key, params) => key === 'error.ai_request_failed' ? 'Localized network failure' : originalTranslate(key, params);
  for (let index = 0; index < 3; index++) app.service.set('newer-paused-' + index, { id: 'newer-paused-' + index, document_id: 'document-a', state: 'paused', rounds: 1, elapsed_ms: 10, last_event_id: 2 });
  app.service.set('failed-original', { id: 'failed-original', document_id: 'document-a', state: 'failed', error_code: 'ai_request_failed', rounds: 7, elapsed_ms: 12000, last_event_id: 8,
    provider: 'original-configured-provider', model: 'original-configured-model', skill_id: 'original-selected-skill' });
  await app.harness.init(); await app.harness.status('failed-original'); app.harness.render();
  assert.deepEqual(app.calls.map(call => call.action), ['list', 'status'], 'discovery and polling never restart a failed task');
  const host = app.document.getElementById('ai-task-status'), history = host.children.find(node => node.name === 'details'); assert.ok(history);
  const row = history.children.find(node => node.children[0]?.textContent.includes('ai.taskFailed')); assert.ok(row);
  assert.ok(row.children[0].textContent.includes('Localized network failure')); assert.equal(row.children[1].textContent, 'ai.taskResume');
  app.state.ai.config.current = { provider_id: 'newly-selected-provider', model: 'newly-selected-model' };
  await row.children[1].listeners.get('click')();
  const resume = app.calls.find(call => call.action === 'resume'); assert.equal(resume.task_id, 'failed-original');
  assert.deepEqual(Object.keys(resume).sort(), ['action', 'call_id', 'task_id']); assert.equal(app.calls.some(call => call.action === 'start'), false);
  const resumed = app.harness.list().find(task => task.id === 'failed-original'); assert.equal(resumed.state, 'running'); assert.equal(resumed.rounds, 7); assert.equal(resumed.document_id, 'document-a');
  assert.equal(app.service.get(resumed.id).provider, 'original-configured-provider'); assert.equal(app.service.get(resumed.id).model, 'original-configured-model'); assert.equal(app.service.get(resumed.id).skill_id, 'original-selected-skill');
  Object.assign(app.service.get(resumed.id), { state: 'completed', last_event_id: 10 }); await app.harness.status(resumed.id);
  const completed = host.children.find(node => node.children[0]?.textContent.includes('ai.taskCompleted')); assert.equal(completed.children.length, 1);
  await app.harness.resume(resumed.id); assert.equal(app.calls.filter(call => call.action === 'resume').length, 1, 'completed stays terminal');
});

test('start forwards exact UTF-16 ranges and selected Skill using opaque configuration references only', async () => {
  const app = fixture(); await app.start();
  const start = app.calls.find(call => call.action === 'start');
  assert.equal(start.doc_id, 'document-a'); assert.equal(start.generation, 1); assert.equal(start.review_revision, 0);
  assert.equal(start.skill_id, 'user-selected-skill'); assert.deepEqual(start.ranges, [{ from: 0, to: 1 }]);
  assert.equal(start.language, 'zh-CN'); assert.equal(start.credential_id, 'opaque-ref');
  assert.ok(!Object.keys(start).some(key => /header|api_key|base_url/.test(key)));
  assert.ok(!JSON.stringify(app.harness.list()).includes('private'));
});

test('navigation, panel close and the launch AbortSignal leave a document job running', async () => {
  const app = fixture(), controller = new AbortController(), task = await app.start(app.tabA, { signal: controller.signal });
  app.context.toggleAiPanel(); app.context.toggleAiPanel();
  app.state.activeTabId = app.tabB.id; app.emit('readmd:document-loaded'); controller.abort();
  assert.equal(app.harness.list().find(item => item.id === task.id).state, 'running');
  assert.equal(app.calls.filter(call => call.action === 'pause').length, 0);
  await app.harness.status(task.id);
  assert.equal(app.calls.find(call => call.action === 'status').after_event_id, 1);
});

test('closing the original document pauses only its job and does not touch the active other document', async () => {
  const app = fixture(), first = await app.start(app.tabA), second = await app.start(app.tabB);
  app.state.activeTabId = app.tabB.id;
  await app.context.closeTab(app.tabA.id);
  assert.equal(app.state.tabs.length, 1); assert.equal(app.state.tabs[0], app.tabB);
  assert.equal(app.harness.list().find(item => item.id === first.id).state, 'paused');
  assert.equal(app.harness.list().find(item => item.id === second.id).state, 'running');
  assert.deepEqual(app.calls.filter(call => call.action === 'pause').map(call => call.task_id), [first.id]);
});

test('cancelled dirty-close leaves the document job running', async () => {
  const app = fixture(), task = await app.start(); app.tabA.isDirty = true;
  await app.context.closeTab(app.tabA.id);
  assert.equal(app.state.tabs.includes(app.tabA), true); assert.equal(app.harness.list().find(item => item.id === task.id).state, 'running');
  assert.equal(app.calls.some(call => call.action === 'pause'), false);
});

test('app quit pauses all live jobs before posting quit IPC', async () => {
  const app = fixture(); await app.start(app.tabA); await app.start(app.tabB);
  assert.equal(await app.window.ReadMDRecovery.prepareClose(), true);
  const actions = app.calls.map(call => call.action);
  assert.equal(actions.filter(action => action === 'pause').length, 2);
  assert.equal(actions.at(-1), 'ipc'); assert.equal(app.calls.at(-1).value, 'readmd:quit');
  assert.ok(app.harness.list().every(task => task.state === 'paused'));
});

test('pauseAll leaves other processes running and only pauses owned_here booleans or tasks started by this page', async () => {
  const app = fixture();
  for (const [id, ownership] of [['foreign', false], ['legacy-foreign', undefined], ['invalid-owner', 'true'], ['owned', true]]) app.service.set(id, {
    id, document_id: 'document-a', state: 'running', rounds: 0, elapsed_ms: 0, last_event_id: 1, ...(ownership === undefined ? {} : { owned_here: ownership }),
  });
  await app.harness.init(); const local = await app.start(); await app.harness.status('foreign');
  assert.equal(app.harness.list().find(task => task.id === 'owned').owned_here, true);
  assert.equal(app.harness.list().find(task => task.id === 'invalid-owner').owned_here, false);
  assert.equal(await app.harness.pauseAll(), true);
  assert.deepEqual(app.calls.filter(call => call.action === 'pause').map(call => call.task_id).sort(), ['owned', local.id].sort());
  for (const id of ['foreign', 'legacy-foreign', 'invalid-owner']) assert.equal(app.service.get(id).state, 'running');
});

test('unavailable optional task storage allows an ordinary document to close but preserves a known active job', async () => {
  const ordinary = fixture({ failList: true }); await ordinary.context.closeTab(ordinary.tabA.id);
  assert.equal(ordinary.state.tabs.includes(ordinary.tabA), false); assert.deepEqual(ordinary.toasts, []);
  const active = fixture({ failList: true }), task = await active.start(); await active.context.closeTab(active.tabA.id);
  assert.equal(active.state.tabs.includes(active.tabA), true); assert.equal(active.harness.list().find(item => item.id === task.id).state, 'running');
  assert.equal(active.toasts.length, 1); assert.equal(active.calls.some(call => call.action === 'pause'), false);
});

test('pauseAll tolerates failed discovery but requires each known owned active task to persist its pause', async () => {
  const ordinary = fixture({ failList: true }); assert.equal(await ordinary.harness.pauseAll(), true); assert.deepEqual(ordinary.toasts, []);
  const active = fixture({ failList: true }), task = await active.start(); assert.equal(await active.harness.pauseAll(), true);
  assert.equal(active.service.get(task.id).state, 'paused'); assert.deepEqual(active.toasts, []);
  const failed = fixture({ failList: true, failPause: true }), unsaved = await failed.start(); assert.equal(await failed.harness.pauseAll(), false);
  assert.equal(failed.service.get(unsaved.id).state, 'running'); assert.equal(failed.toasts.length, 1);
});

test('a status request sent before Pause cannot restore its older running state', async () => {
  const app = fixture(), task = await app.start(), hold = app.holdStatus();
  const polling = app.harness.status(task.id); await hold.entered.promise;
  await app.harness.pause(task.id); hold.release.resolve(); await polling;
  assert.equal(app.harness.list().find(item => item.id === task.id).state, 'paused');
});

test('background receipts go to their original document after navigation and omit private event text from UI', async () => {
  const app = fixture({ receipt: true }), task = await app.start(), hold = app.holdStatus();
  const polling = app.harness.status(task.id); await hold.entered.promise;
  app.state.activeTabId = app.tabB.id; app.tabB.content = 'New input in B'; app.emit('readmd:document-loaded');
  hold.release.resolve(); await polling;
  assert.equal(app.adopted.length, 1); assert.equal(app.adopted[0].tab, app.tabA);
  assert.equal(app.tabB.content, 'New input in B'); assert.equal(app.document.getElementById('ai-task-status').hidden, true);
  app.state.activeTabId = app.tabA.id; app.harness.render();
  const text = app.document.getElementById('ai-task-status').children.flatMap(row => row.children.map(node => node.textContent)).join(' ');
  assert.ok(!text.includes('private')); assert.ok(text.includes('ai.taskRunning'));
});

test('a completion receipt binds to a reopened tab for the same durable document', async () => {
  const app = fixture({ receipt: true }), task = await app.start();
  app.state.tabs = [app.tabB];
  const reopened = { ...app.tabA, id: 'reopened-a' }; app.state.tabs.push(reopened);
  app.service.get(task.id).state = 'completed'; app.service.get(task.id).last_event_id++;
  await app.harness.status(task.id);
  assert.equal(app.adopted[0].tab, reopened);
});

test('budget, stale and interrupted stop reasons remain distinct and only localized codes reach the strip', async () => {
  const app = fixture(), task = await app.start();
  const localized = { 'error.review_ai_budget': 'Budget reached', 'error.review_stale_generation': 'Document changed', 'error.review_ai_interrupted': 'Interrupted; resume explicitly' };
  app.window.i18n.t = key => localized[key] || key;
  for (const [state, code] of [['budget_exhausted', 'review_ai_budget'], ['needs_user_resume', 'review_stale_generation'], ['needs_user_resume', 'review_ai_interrupted']]) {
    Object.assign(app.service.get(task.id), { state, error_code: code, last_event_id: app.service.get(task.id).last_event_id + 1 });
    await app.harness.status(task.id);
    assert.equal(app.harness.list().find(item => item.id === task.id).error_code, code);
    assert.ok(app.document.getElementById('ai-task-status').children[0].children[0].textContent.includes(localized['error.' + code]));
  }
  Object.assign(app.service.get(task.id), { error_code: 'https://private.example/key', last_event_id: 100 });
  await app.harness.status(task.id); assert.equal(app.harness.list()[0].error_code, null);
});

test('completed history is bounded while every older resumable task remains accessible in details', async () => {
  const app = fixture();
  for (let i = 15; i >= 0; i--) app.service.set('old-' + i, { id: 'old-' + i, document_id: 'document-a', state: i < 5 ? 'paused' : 'completed', rounds: i, elapsed_ms: 0, last_event_id: i });
  await app.harness.init();
  const host = app.document.getElementById('ai-task-status');
  assert.equal(host.children.length, 5);
  const history = host.children.find(element => element.name === 'details'); assert.ok(history);
  assert.equal(history.children.length, 3, 'one summary plus the two older resumable jobs');
  assert.equal(app.harness.list().filter(task => task.state === 'paused').length, 5);
});


test('a paused task cannot resume while its document or a group refactor is reserved', async () => {
  const app = fixture(), task = await app.start();
  await app.harness.pause(task.id);
  assert.equal(await app.harness.reserveTabForRename(app.tabA), true);
  let calls = app.calls.filter(item => item.action === 'resume').length;
  await assert.rejects(app.harness.resume(task.id));
  assert.equal(app.calls.filter(item => item.action === 'resume').length, calls);
  app.harness.releaseTab(app.tabA);
  app.window.ReadMDRefactor = { busy: () => true };
  await assert.rejects(app.harness.resume(task.id));
  assert.equal(app.calls.filter(item => item.action === 'resume').length, calls);
  app.window.ReadMDRefactor = { busy: () => false };
  await app.harness.resume(task.id);
  assert.equal(app.calls.filter(item => item.action === 'resume').length, calls + 1);
});
