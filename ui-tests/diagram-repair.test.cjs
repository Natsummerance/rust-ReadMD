'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { randomUUID } = require('node:crypto');
const source = fs.readFileSync(path.join(__dirname, '../assets/js/features/diagram-repair.js'), 'utf8');
const diagnostics = require('../assets/js/reader/diagram-diagnostics.js');
const tick = () => new Promise(resolve => setImmediate(resolve));
async function until(predicate) { for (let n = 0; n < 80; n++) { if (predicate()) return; await tick(); } throw Error('Unit operation did not settle'); }
function barrier() { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; }
class Element {
  constructor(tag) { this.tagName = tag.toUpperCase(); this.nodeType = 1; this.children = []; this.listeners = new Map(); this.dataset = {}; this.attributes = {}; this.style = {}; this.className = ''; this.value = ''; this.disabled = false; this.hidden = false; }
  append(...nodes) { for (const node of nodes) { node.remove(); this.children.push(node); node.parentNode = this; } }
  replaceChildren(...nodes) { for (const node of this.children) node.parentNode = null; this.children = []; this._html = ''; this._text = ''; this.append(...nodes); }
  remove() { if (this.parentNode) this.parentNode.children = this.parentNode.children.filter(node => node !== this); this.parentNode = null; }
  get isConnected() { return this.isBody || Boolean(this.parentNode?.isConnected); }
  get classList() { const node = this; return { contains: name => node.className.split(/\s+/).includes(name), add(name) { if (!this.contains(name)) node.className += ' ' + name; }, remove(name) { node.className = node.className.split(/\s+/).filter(value => value !== name).join(' '); } }; }
  matches(selector) { return selector[0] === '.' ? this.classList.contains(selector.slice(1)) : selector[0] === '#' ? this.id === selector.slice(1) : this.tagName.toLowerCase() === selector; }
  querySelectorAll(selector) { return this.children.flatMap(node => [...(node.matches(selector) ? [node] : []), ...node.querySelectorAll(selector)]); }
  querySelector(selector) { return this.querySelectorAll(selector)[0] || null; }
  setAttribute(name, value) { this.attributes[name] = String(value); }
  removeAttribute(name) { delete this.attributes[name]; }
  addEventListener(type, listener) { if (!this.listeners.has(type)) this.listeners.set(type, []); this.listeners.get(type).push(listener); }
  emit(type) { for (const listener of this.listeners.get(type) || []) listener({ target: this }); }
  set textContent(value) { this.replaceChildren(); this._text = String(value); }
  get textContent() { return this._text || this.children.map(node => node.textContent).join(''); }
  set innerHTML(value) { this.replaceChildren(); this._html = value; }
  get innerHTML() { return this._html || ''; }
  focus() {}
  select() {}
}
function fixture(options = {}) {
  const original = options.original || 'bad';
  const fence = '```' + (options.engine || 'mermaid') + '\n' + original + '\n```\n';
  const prefix = '# Private surrounding prose 😀\n\n' + fence + '\n';
  const content = prefix + fence + '\nPrivate ending';
  const from = prefix.length, to = from + fence.length, sourceLine = prefix.split('\n').length;
  const owner = { id: 'owner', path: '/private/Private.md', revision: 'disk-v1', content, original: content, editGeneration: 0 };
  const other = { id: 'other', path: '/private/Other.md', content, original: content };
  const state = { tabs: [owner, other], activeTabId: owner.id, editing: false, pagination: { enabled: false, mode: 'paged', currentPage: 0 }, ai: { messages: [{ role: 'user', content: 'PRIVATE CHAT' }] } };
  const head = { document_id: 'doc-owned', generation: 1, review_revision: 0, content, changes: [] };
  const calls = [], proposals = [], copied = [], cancelled = [], timers = new Map(); let timer = 0, configured = 0;
  const body = new Element('body'); body.isBody = true;
  const card = new Element('div'); card.className = 'diagram-card';
  card.dataset = { diagramEngine: options.engine || 'mermaid', diagramCode: encodeURIComponent(original), ...(options.noLine ? {} : { sourceLine: String(sourceLine) }) }; body.append(card);
  const document = { body, activeElement: null, createElement: tag => new Element(tag), getElementById: id => body.querySelector('#' + id), execCommand: () => options.copyFailure !== true };
  const events = new Map();
  const window = { state, crypto: { randomUUID }, i18n: { t: (key, params) => params ? key + JSON.stringify(params) : key },
    ReadMDDiagramDiagnostics: diagnostics,
    ReadMDDiagramTools: { async writeClipboard(text) { if (options.copyFailure) return false; copied.push(text); return true; } },
    ReadMDModal: { open: node => node.classList.remove('hidden'), close: node => node.classList.add('hidden') },
    addEventListener(type, callback) { events.set(type, callback); } };
  const review = {
    snapshot: tab => tab === owner ? head : null,
    async codeTarget(line, code, tab) {
      calls.push({ kind: 'target', line, code, tab });
      if (options.bindingFailure) throw Error('Unavailable');
      assert.equal(tab, owner); assert.equal(line, sourceLine); assert.equal(code, original);
      return { tab, stamp: { tabId: owner.id, document_id: head.document_id, generation: head.generation, review_revision: head.review_revision, content: head.content, editGeneration: owner.editGeneration },
        target: { from, to, source_line: sourceLine, code, node_id: 'a'.repeat(64) } };
    },
    async propose(edits, stamp, tab) {
      assert.equal(tab, owner); assert.equal(stamp.document_id, head.document_id); assert.equal(stamp.generation, head.generation); assert.equal(stamp.content, head.content);
      proposals.push({ edits, stamp, tab });
      if (options.proposalFailure) throw Error('Stale');
      for (const edit of [...edits].reverse()) owner.content = owner.content.slice(0, edit.from) + edit.insert + owner.content.slice(edit.to);
      head.content = owner.content; head.generation++; return head;
    },
    async cancelAi(taskId) { cancelled.push(taskId); return 'cancelled'; },
  };
  window.ReadMDReview = review;
  const context = vm.createContext({ window, document, state, navigator: { clipboard: { async writeText(text) { if (options.copyFailure) throw Error('No clipboard'); copied.push(text); } } },
    AbortController, DOMException, setInterval(callback) { const id = ++timer; timers.set(id, callback); return id; }, clearInterval: id => timers.delete(id),
    getActiveTab: () => state.tabs.find(tab => tab.id === state.activeTabId), getCurrentDocumentText: () => state.tabs.find(tab => tab.id === state.activeTabId)?.content || '',
    showToast: message => calls.push({ kind: 'toast', message }),
    async ensureAiConfigured() { configured++; return { provider: 'existing-connection', credential_id: 'opaque-credential', model: 'chosen-model', headers: { Authorization: 'PRIVATE HEADER' }, base_url: 'https://private.provider' }; },
    async apiFetch(url, request) {
      const body = JSON.parse(request.body); calls.push({ kind: 'http', url, body });
      if (options.api) { const result = await options.api(url, request, body, calls); if (result) return result; }
      if (url === '/api/ai/chat') return json({ ok: true, text: 'good' });
      if (url === '/api/diagram/render') return json({ ok: true, svg: '<svg><text>offline</text></svg>' });
      throw Error('Unexpected URL ' + url);
    },
    async renderLocalDiagram(engine, code, target) {
      calls.push({ kind: 'render', engine, code });
      if (options.render) { const value = await options.render(engine, code, target); if (value !== undefined) return value; }
      if (!code.startsWith('good')) throw Object.assign(Error('Invalid diagram'), { code: 'diagram_invalid_input' });
      return '<svg><text>validated</text></svg>';
    },
    diagramOutputMarkup: markup => markup,
  });
  vm.runInContext(source, context);
  const app = window.ReadMDDiagramRepair;
  return { app, window, document, context, state, owner, other, card, head, calls, proposals, copied, cancelled, from, to, prefix, fence, configured: () => configured,
    node: id => document.getElementById('diagram-repair-' + id),
    open: initialCandidate => app.open(card, { code: 'diagram_invalid_input', message: 'line 1: expected arrow /Users/private/secret\nBearer abcdefgh', line: 1 }, initialCandidate),
    async click(id) { const node = this.node(id); assert.ok(node); if (!node.disabled) node.emit('click'); await tick(); },
    async settle() { for (let n = 0; n < 6; n++) await tick(); },
    watch() { for (const callback of [...timers.values()]) callback(); },
  };
}
const json = (body, ok = true) => ({ ok, status: ok ? 200 : 500, headers: new Headers({ 'content-type': 'application/json' }), json: async () => body });

test('opening and previewing a local candidate never consults AI; a validated proposal targets the second equal fence', async t => {
  const app = fixture(); t.after(() => app.app.close()); app.open('good locally repaired'); await app.settle();
  assert.equal(app.configured(), 0); assert.equal(app.calls.filter(call => call.kind === 'http').length, 0);
  assert.equal(app.node('apply').disabled, false); assert.match(app.node('preview').children[0].innerHTML, /<svg>/);
  await app.click('apply'); await app.settle();
  assert.equal(app.proposals.length, 1); assert.equal(app.proposals[0].edits[0].from, app.from); assert.equal(app.proposals[0].edits[0].to, app.to);
  assert.equal(app.owner.content.slice(0, app.from), app.prefix); assert.equal(app.owner.content.includes('good locally repaired'), true);
  assert.equal(app.calls.at(-1).message, 'fixes.savePending');
});

test('AI sends only this diagram and redacted diagnosis through the selected opaque connection; output gets offline validation', async t => {
  const app = fixture({ api: url => url === '/api/ai/chat' ? json({ text: '```mermaid\ngood AI candidate\n```' }) : undefined });
  t.after(() => app.app.close()); app.open(); await app.settle(); assert.equal(app.configured(), 0);
  await app.click('generate'); await until(() => !app.node('apply').disabled);
  const request = app.calls.find(call => call.url === '/api/ai/chat').body;
  assert.equal(request.incognito, true); assert.equal(request.provider, 'existing-connection'); assert.equal(request.credential_id, 'opaque-credential'); assert.equal(request.model, 'chosen-model');
  assert.equal(request.messages.length, 2); assert.equal(request.session, undefined); assert.equal(request.skill_id, undefined); assert.equal(request.headers, undefined);
  assert.deepEqual(JSON.parse(request.messages[1].content), { engine: 'mermaid', source: 'bad', diagnostic: { code: 'diagram_invalid_input', message: 'line 1: expected arrow [path]', line: 1 } });
  assert.doesNotMatch(JSON.stringify(request), /PRIVATE CHAT|Private surrounding|Private\.md|PRIVATE HEADER|private\.provider/);
  assert.equal(app.node('candidate').value, 'good AI candidate'); assert.equal(app.proposals.length, 0);
});

test('editing a validated candidate revokes permission immediately and a failed preview cannot apply', async t => {
  const app = fixture(); t.after(() => app.app.close()); app.open('good candidate'); await app.settle();
  assert.equal(app.node('apply').disabled, false);
  app.node('candidate').value = 'bad again'; app.node('candidate').emit('input');
  assert.equal(app.node('apply').disabled, true); await app.click('apply'); assert.equal(app.proposals.length, 0);
  await app.click('preview-btn'); await app.settle(); assert.equal(app.node('apply').disabled, true); assert.equal(app.node('status').dataset.state, 'error');
});

test('late renderer completion after candidate edits cannot validate another candidate', async t => {
  const rendering = barrier();
  const app = fixture({ render: async (_engine, code) => { if (code === 'good old') { await rendering.promise; return '<svg>old</svg>'; } } });
  t.after(() => app.app.close()); app.open('good old'); await app.settle();
  app.node('candidate').value = 'good edited'; app.node('candidate').emit('input'); rendering.resolve(); await app.settle();
  assert.equal(app.node('apply').disabled, true); assert.equal(app.node('preview').children.length, 0);
  await app.click('preview-btn'); await until(() => !app.node('apply').disabled);
});

for (const change of ['manual edit and undo', 'switch tab', 'switch page', 'replace diagram identity', 'close document']) test(change + ' leaves only a copyable candidate', async t => {
  const app = fixture(); t.after(() => app.app.close()); app.open('good candidate'); await app.settle();
  assert.equal(app.node('apply').disabled, false);
  if (change === 'manual edit and undo') app.owner.editGeneration += 2;
  if (change === 'switch tab') app.state.activeTabId = app.other.id;
  if (change === 'switch page') app.state.pagination.currentPage++;
  if (change === 'replace diagram identity') app.card.dataset.sourceLine = '1';
  if (change === 'close document') app.state.tabs = [app.other];
  app.watch(); assert.equal(app.node('apply').disabled, true); assert.equal(app.node('status').textContent, 'reader.diagramAiCopyOnly');
  await app.click('copy'); assert.deepEqual(app.copied, ['good candidate']); assert.equal(app.proposals.length, 0);
});

test('unknown source location keeps AI and offline preview usable, with no writeback fallback', async t => {
  const app = fixture({ noLine: true }); t.after(() => app.app.close()); app.open('good candidate'); await app.settle(); app.watch();
  assert.equal(app.node('generate').disabled, false); assert.equal(app.node('apply').disabled, true);
  await app.click('generate'); await until(() => app.node('candidate').value === 'good'); await app.settle();
  assert.equal(app.node('apply').disabled, true); assert.equal(app.configured(), 1); assert.equal(app.proposals.length, 0);
});

test('failure, explicit cancellation and retry use independent tasks and cannot auto-apply', async t => {
  let attempt = 0;
  const app = fixture({ api: async (url, request) => {
    if (url !== '/api/ai/chat') return;
    attempt++;
    if (attempt === 1) return json({ error_code: 'ai_network_error' }, false);
    if (attempt === 2) return new Promise((_resolve, reject) => request.signal.addEventListener('abort', () => reject(new DOMException('Cancelled', 'AbortError')), { once: true }));
    return json({ text: 'good after retry' });
  } });
  t.after(() => app.app.close()); app.open(); await app.settle();
  await app.click('generate'); await until(() => app.node('status').dataset.state === 'error'); assert.equal(app.node('generate').disabled, false);
  await app.click('generate'); await until(() => attempt === 2); await app.click('stop'); await until(() => !app.node('generate').disabled);
  assert.equal(app.node('status').textContent, 'ai.stopped'); assert.equal(app.cancelled.length, 1); assert.equal(app.node('apply').disabled, true);
  await app.click('generate'); await until(() => !app.node('apply').disabled);
  const ids = app.calls.filter(call => call.url === '/api/ai/chat').map(call => call.body.task_id); assert.equal(new Set(ids).size, 3); assert.equal(app.proposals.length, 0);
});

test('server-rendered candidates stay offline and remote-only results cannot enable apply', async t => {
  const app = fixture({ engine: 'plantuml', original: '@startuml\n@enduml', api: url => url === '/api/diagram/render' ? json({ ok: true, requires_network: true, svg: '<svg>online</svg>' }) : undefined });
  t.after(() => app.app.close()); app.open('@startuml\nAlice -> Bob: hello\n@enduml'); await app.settle();
  const renders = app.calls.filter(call => call.url === '/api/diagram/render'); assert.equal(renders.length, 2); assert.ok(renders.every(call => call.body.allow_remote === false));
  assert.equal(app.node('apply').disabled, true); assert.equal(app.node('status').dataset.state, 'error'); assert.equal(app.configured(), 0);
});

test('proposal rejection retains the edited candidate for explicit copy', async t => {
  const app = fixture({ proposalFailure: true }); t.after(() => app.app.close()); app.open('good candidate'); await app.settle(); await app.click('apply'); await app.settle();
  assert.equal(app.node('apply').disabled, true); assert.equal(app.node('candidate').value, 'good candidate'); assert.equal(app.owner.content, app.head.content);
  await app.click('copy'); assert.deepEqual(app.copied, ['good candidate']);
});

test('copy failure is reported accurately and does not claim success', async t => {
  const app = fixture({ copyFailure: true }); t.after(() => app.app.close()); app.open('good candidate'); await app.settle(); await app.click('copy');
  assert.equal(app.node('status').textContent, 'toast.copyFailed');
});
