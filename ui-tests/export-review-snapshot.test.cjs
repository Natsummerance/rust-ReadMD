'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { randomUUID } = require('node:crypto');
const reviewSource = fs.readFileSync(path.join(__dirname, '../assets/js/editor/review.js'), 'utf8');
const exportSource = fs.readFileSync(path.join(__dirname, '../assets/js/features/export.js'), 'utf8');
const clone = value => JSON.parse(JSON.stringify(value));
function barrier() { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; }

// Protocol double: verifies the adapter's ordering/consent and editor scope.
// Filesystem freezing, output verification and SQLite atomicity have Rust tests.
async function fixture(settings = {}) {
  const calls = [], choices = [], toasts = [], nodes = new Map(), frozen = new Map(); let committedReceipt = null;
  const installed = new Set(settings.installedCapabilities || []), required = settings.requiredCapabilities || [];
  const snapshot = { document_id: 'document-a', path: 'A.md', generation: 1, review_revision: 1, revision: 'rev-1', content: 'AI A / AI B', changes: [
    { id: 'A', from: 0, to: 4, original: 'A', replacement: 'AI A', decision: 'pending', application: 'applied', version: 1 },
    { id: 'B', from: 7, to: 11, original: 'B', replacement: 'AI B', decision: 'pending', application: 'applied', version: 1 },
  ] };
  if (settings.cjk) { snapshot.content = '中文 ' + snapshot.content; snapshot.changes.forEach(change => { change.from += 3; change.to += 3; }); }
  if (settings.oversized) { snapshot.content = 'a'.repeat(4 * 1024 * 1024 + 1); snapshot.changes = []; }
  const tab = { id: 'tab-a', path: 'A.md', name: 'A.md', original: snapshot.content, content: snapshot.content, isDirty: false, editGeneration: 0 };
  const state = { tabs: [tab], activeTabId: tab.id, editing: true, mode: 'file', file: tab.path, dir: '', original: tab.original,
    export: { fmt: settings.format || 'html', options: clone(settings.options || {}), defaults: {} } };
  function node() { return { value: '', textContent: '', className: '', title: '', dataset: {}, hidden: false, setAttribute() {}, addEventListener() {}, appendChild() {},
    classList: { add() {}, remove() {}, toggle() {}, contains: () => false } }; }
  for (const name of ['edit-area', 'export-result', 'export-open', 'export-reveal', 'export-opts', 'export-modal', 'exp-preset']) nodes.set(name, node());
  nodes.get('edit-area').value = snapshot.content;
  const listeners = new Map();
  const document = { getElementById: name => nodes.get(name) || null, querySelectorAll: () => [], createElement: node, addEventListener() {} };
  const window = { READMD_ENGINE: 'rust', crypto: { randomUUID }, i18n: { t: (key, params) => key + (params ? JSON.stringify(params) : '') },
    addEventListener(type, fn) { if (!listeners.has(type)) listeners.set(type, []); listeners.get(type).push(fn); },
    dispatchEvent(event) { for (const listener of listeners.get(event.type) || []) listener(event); } };
  const response = (data, ok = true) => ({ ok, status: ok ? 200 : 409, json: async () => clone(data) });
  async function fetch(url, opts) {
    const body = JSON.parse(opts.body); calls.push({ url, body: clone(body) });
    if (url === '/api/export/preview') {
      assert.ok(frozen.has(body.snapshot_id));
      assert.equal(frozen.get(body.snapshot_id).runtime_ready, true);
      return response({ ok: true, format: 'epub', warnings: [] });
    }
    if (url === '/api/review') {
      if (body.action === 'resume') return settings.denyResume ? { ok: false, status: 403, json: async () => ({ ok: false, error_code: 'review_not_authorized' }) } : response({ ok: true, found: Boolean(settings.resumeFound) });
      if (body.action === 'open' || body.action === 'read') return response(snapshot);
      assert.equal(body.document_id, snapshot.document_id);
      if (settings.holdEdit && body.action === 'edit') { settings.holdEdit.entered.resolve(); await settings.holdEdit.release.promise; settings.holdEdit = null; }
      if (body.generation !== snapshot.generation || body.review_revision !== snapshot.review_revision) return response({ ok: false, error_code: 'review_stale_generation' }, false);
      if (body.action === 'edit' || body.action === 'propose') {
        const before = snapshot.content;
        for (const edit of [...body.edits].reverse()) snapshot.content = snapshot.content.slice(0, edit.from) + edit.insert + snapshot.content.slice(edit.to);
        if (body.action === 'propose') for (const edit of body.edits) snapshot.changes.push({ id: 'C', from: edit.from, to: edit.from + edit.insert.length,
          original: before.slice(edit.from, edit.to), replacement: edit.insert, decision: 'pending', application: 'applied', version: 1 });
        snapshot.generation++; snapshot.review_revision++; snapshot.revision = 'rev-' + snapshot.generation;
        return response(snapshot);
      }
      throw new Error('Unexpected review action: ' + body.action);
    }
    assert.equal(url, '/api/export/snapshot');
    if (body.action === 'prepare') {
      assert.equal(body.doc_id, snapshot.document_id); assert.equal(body.generation, snapshot.generation); assert.equal(body.review_revision, snapshot.review_revision);
      const pending = snapshot.changes.filter(item => item.decision === 'pending');
      const prepared = { ok: true, snapshot_id: 'snapshot-' + (frozen.size + 1), format: body.format, pending_count: pending.length, pending_ids: pending.map(item => item.id),
        runtime_ready: required.every(id => installed.has(id)),
        content: snapshot.content, options: body.options, versions: pending.map(item => [item.id, item.version]) };
      frozen.set(prepared.snapshot_id, clone(prepared));
      return response(prepared);
    }
    if (body.action === 'runtime') {
      const prepared = frozen.get(body.snapshot_id); assert.ok(prepared);
      const missing = [...required, ...(prepared.figures_require_fonts ? ['fonts'] : [])].find(id => !installed.has(id));
      if (missing) return response({ ok: false, error_code: { typst: 'typst_engine_unavailable', latex: 'latex_engine_unavailable', fonts: 'publication_fonts_unavailable' }[missing] }, false);
      prepared.runtime_ready = true; return response({ ok: true, snapshot_id: body.snapshot_id, runtime_ready: true });
    }
    if (body.action === 'discard') return response({ ok: true });
    if (body.action === 'figures') {
      const prepared = frozen.get(body.snapshot_id); assert.equal(prepared.runtime_ready, true);
      if (settings.figuresRequireFonts) { prepared.figures_require_fonts = true; prepared.runtime_ready = installed.has('fonts'); }
      return response({ ok: true });
    }
    if (body.action === 'reconcile') return response({ ok: true, state: committedReceipt ? 'committed' : 'commit_intent', receipt: committedReceipt });
    assert.equal(body.action, 'commit');
    if (settings.holdCommit) { settings.holdCommit.entered.resolve(); await settings.holdCommit.release.promise; }
    if (settings.failure) return response({ ok: false, error_code: settings.failure, error: 'secret provider URL or local traceback' }, false);
    if (settings.uncertainCommit) throw new TypeError('secret dropped network connection');
    const prepared = frozen.get(body.snapshot_id); assert.ok(prepared);
    assert.equal(prepared.runtime_ready, true);
    if (prepared.pending_count) assert.equal(body.accept_pending, true);
    for (const [changeId, version] of prepared.versions) {
      const change = snapshot.changes.find(item => item.id === changeId);
      if (change?.version === version && change.decision === 'pending') { change.decision = 'accepted'; change.version++; }
    }
    snapshot.review_revision++;
    committedReceipt = clone({ ok: true, path: 'out.' + state.export.fmt, size: prepared.content.length, warns: [], review: snapshot, review_conflicts: [], finalization_pending: false });
    if (settings.loseCommitAck) throw new TypeError('secret dropped network connection');
    return response(committedReceipt);
  }
  const context = vm.createContext({ window, document, state, TextEncoder, Uint32Array, AbortController, crypto: { randomUUID }, setTimeout, clearTimeout,
    CustomEvent: class { constructor(type, value) { this.type = type; Object.assign(this, value); } },
    $: name => nodes.get(name) || null, apiFetch: fetch, getActiveTab: () => state.tabs.find(item => item.id === state.activeTabId),
    getEditContent: () => nodes.get('edit-area').value, i18n: window.i18n, showToast: value => toasts.push(value), busy() {}, renderTabsBar() {}, updateUnloadGuard() {},
    warnMessages: () => [], apiMessage: (data, fallback) => data.error_code || fallback, hasPy: true,
    py: { export_doc(format, payload) {
      if (!settings.oversized) throw new Error('Legacy export bypass');
      calls.push({ url: 'native-legacy', body: { format, ...clone(payload) } }); return { ok: true, path: 'large.' + format, warns: [] };
    }, export_epub(content, _out, _meta, _options, _base, provenance) {
      if (!settings.oversized) throw new Error('Legacy EPUB bypass');
      calls.push({ url: 'native-legacy', body: { format: 'epub', content, ...clone(provenance) } }); return { ok: true, path: 'large.epub', warns: [] };
    } },
    askChoice: async (title, description, buttons) => { choices.push({ title, description, buttons }); return typeof settings.choice === 'function' ? settings.choice() : settings.choice || 'export'; },
    switchTab: async id => { state.activeTabId = id; }, toggleEdit: async () => { state.editing = true; },
  });
  vm.runInContext(reviewSource, context);
  vm.runInContext(exportSource, context);
  const actualPrepareFigures = context.prepareExportFigures;
  const figureInputs = [];
  context.prepareExportFigures = async (content, options) => {
    calls.push({ url: 'figures', body: { content } }); figureInputs.push(content);
    if (settings.holdFigures) { settings.holdFigures.entered.resolve(); await settings.holdFigures.release.promise; }
    return { ...options, figures: { rendered: [{ key: 'fixed-fence', svg: settings.figureSvg || '<svg/>' }] } };
  };
  context.renderExportWarns = () => {};
  window.ReadMDCapabilities = context.ReadMDCapabilities = {
    ensure: async id => {
      if (settings.trackCapabilities) calls.push({ url: 'capability', body: { id } });
      if (settings.denyCapability === id) return false;
      installed.add(id); return true;
    },
    ensurePublication: async (content, options) => {
      if (settings.trackCapabilities) calls.push({ url: 'publication', body: clone({ content, options }) });
      if (settings.holdPublication) { settings.holdPublication.entered.resolve(); await settings.holdPublication.release.promise; }
      if (options.pdf?.engine && !['rust', 'typst'].includes(options.pdf.engine) || !/[\u2e80-\u9fff\uac00-\ud7af\uf900-\ufaff]/u.test(content + JSON.stringify(options))) return true;
      if (settings.denyCapability === 'fonts') return false;
      installed.add('fonts'); return true;
    },
  };
  if (!settings.oversized) { await window.ReadMDReview.ensure(tab); assert.equal(await window.ReadMDReview.flush(tab), true); }
  return { context, state, tab, snapshot, nodes, calls, choices, toasts, figureInputs, actualPrepareFigures, frozen, review: window.ReadMDReview,
    run: () => context.runExport(),
    type(text) { nodes.get('edit-area').value = text; tab.content = text; tab.editGeneration++; },
  };
}

for (const format of ['html', 'pdf', 'docx', 'epub', 'tex']) test(format + ': consent is bound to the prepared snapshot and Rust never uses the legacy bridge', async () => {
  const app = await fixture({ format });
  await app.run();
  assert.deepEqual(app.snapshot.changes.map(item => item.decision), ['accepted', 'accepted']);
  assert.equal(app.choices.length, 1);
  assert.deepEqual(clone(app.choices[0].buttons.map(item => [item.id, item.label])), [
    ['export', 'export.acceptAllAndExport'], ['review', 'export.viewChanges'], ['cancel', 'dialog.cancel'],
  ]);
  const route = app.calls.filter(call => call.url === '/api/export/snapshot');
  assert.deepEqual(route.map(call => call.body.action), ['prepare', 'runtime', 'figures', 'runtime', 'commit']);
  assert.equal(route.at(-1).body.snapshot_id, 'snapshot-1');
});

for (const choice of ['cancel', 'review']) test(choice + ': opening review or cancelling exports changes no review decision', async () => {
  const app = await fixture({ choice }); await app.run();
  assert.deepEqual(app.snapshot.changes.map(item => item.decision), ['pending', 'pending']);
  assert.deepEqual(app.calls.filter(call => call.url === '/api/export/snapshot').map(call => call.body.action), ['prepare', 'discard']);
  assert.equal(app.figureInputs.length, 0);
});

for (const failure of ['cancelled', 'export_write_failed']) test(failure + ': an unsuccessful commit leaves every pending change intact and scrubs transport detail', async () => {
  const app = await fixture({ failure }); await app.run();
  assert.deepEqual(app.snapshot.changes.map(item => item.decision), ['pending', 'pending']);
  assert.ok(!app.toasts.some(text => text.includes('secret')));
  assert.equal(app.calls.some(call => call.url === '/api/export/snapshot' && call.body.action === 'discard'), false, 'a commit intent is never discarded');
});

test('preparation freezes before figure work; double activation creates one export; later C stays pending', async () => {
  const gate = { entered: barrier(), release: barrier() }, app = await fixture({ holdFigures: gate });
  const first = app.run(), second = app.run(); assert.equal(first, second);
  await gate.entered.promise;
  const stamp = await app.review.capture(app.tab);
  await app.review.propose([{ from: stamp.content.length, to: stamp.content.length, insert: ' / AI C' }], stamp, app.tab);
  gate.release.resolve(); await first;
  assert.equal(app.figureInputs[0], 'AI A / AI B');
  assert.deepEqual(app.snapshot.changes.map(item => item.decision), ['accepted', 'accepted', 'pending']);
  assert.equal(app.snapshot.content, 'AI A / AI B / AI C');
  assert.ok(app.calls.findIndex(call => call.body.action === 'prepare') < app.calls.findIndex(call => call.url === 'figures'));
  assert.equal(app.calls.filter(call => call.body.action === 'commit').length, 1);
});

test('a delayed export receipt reads the current head and keeps newer typing and another active document', async () => {
  const gate = { entered: barrier(), release: barrier() }, app = await fixture({ holdCommit: gate });
  const exported = app.run(); await gate.entered.promise;
  app.type('AI A / AI B + hand input'); assert.equal(await app.review.flush(app.tab), true);
  const other = { id: 'tab-other', path: 'Other.md', content: 'Other typed text', original: 'Other typed text' };
  app.state.tabs.push(other); app.state.activeTabId = other.id; app.nodes.get('edit-area').value = other.content;
  gate.release.resolve(); await exported;
  assert.equal(app.tab.content, 'AI A / AI B + hand input');
  assert.equal(other.content, 'Other typed text'); assert.equal(app.nodes.get('edit-area').value, 'Other typed text');
});

test('an external AI head rebases a definitively stale queued edit instead of erasing hand input', async () => {
  const gate = { entered: barrier(), release: barrier() }, app = await fixture({ holdEdit: gate });
  app.type('AI A / AI B + hand'); const flushing = app.review.flush(app.tab); await gate.entered.promise;
  app.snapshot.content = 'NEW A / AI B'; app.snapshot.generation++; app.snapshot.review_revision++;
  app.snapshot.changes[0].replacement = 'NEW A'; app.snapshot.changes[0].to = 5;
  const adopted = app.review.adopt(clone(app.snapshot), app.tab);
  gate.release.resolve(); await flushing; assert.equal(await adopted, true);
  assert.equal(await app.review.flush(app.tab), true);
  assert.equal(app.snapshot.content, 'NEW A / AI B + hand');
  assert.equal(app.nodes.get('edit-area').value, 'NEW A / AI B + hand');
  const edits = app.calls.filter(call => call.body.action === 'edit');
  assert.equal(edits.length, 2); assert.equal(edits[1].body.generation, 2);
  assert.notEqual(edits[0].body.call_id, edits[1].body.call_id);
});

test('presentation export requires explicit review and never accepts changes through its unsupported route', async () => {
  const app = await fixture({ format: 'presentation', choice: 'review' }); await app.run();
  assert.equal(app.calls.some(call => call.url.startsWith('/api/export')), false);
  assert.deepEqual(app.snapshot.changes.map(item => item.decision), ['pending', 'pending']);
});

test('native preview and the final export reuse the same frozen snapshot and bind figures once', async () => {
  const app = await fixture({ format: 'epub' }); let generatePreview;
  app.context.setTimeout = callback => { generatePreview = callback; return 1; };
  app.context.renderNativeExportPreview = () => {};
  const modal = { classList: { contains: () => true } }; app.nodes.set('export-preview-modal', modal);
  const options = app.context.collectExportOptions();
  app.context.requestNativeExportPreview('epub', options, app.snapshot.content, 'A', '');
  await generatePreview(); await app.run();
  const preview = app.calls.find(call => call.url === '/api/export/preview');
  const commit = app.calls.find(call => call.body.action === 'commit');
  assert.ok(preview); assert.equal(preview.body.snapshot_id, commit.body.snapshot_id);
  assert.equal(app.calls.filter(call => call.body.action === 'prepare').length, 1);
  assert.equal(app.calls.filter(call => call.body.action === 'figures').length, 1);
});

test('a lost commit acknowledgement reconciles its durable receipt without invoking another export', async () => {
  const app = await fixture({ loseCommitAck: true }); await app.run();
  assert.deepEqual(app.calls.filter(call => call.url === '/api/export/snapshot').map(call => call.body.action), ['prepare', 'runtime', 'figures', 'runtime', 'commit', 'reconcile']);
  assert.deepEqual(app.snapshot.changes.map(item => item.decision), ['accepted', 'accepted']);
  assert.equal(app.nodes.get('export-result').className, 'export-result ok');
});

test('an unresolved commit intent never reports export success or discards the prepared consent', async () => {
  const app = await fixture({ uncertainCommit: true }); await app.run();
  assert.deepEqual(app.snapshot.changes.map(item => item.decision), ['pending', 'pending']);
  assert.equal(app.nodes.get('export-result').className, 'export-result err');
  assert.equal(app.calls.some(call => call.body.action === 'discard'), false);
  assert.ok(!app.toasts.some(text => text.includes('secret')));
});

for (const format of ['html', 'epub']) test('oversized untracked ' + format + ' uses legacy export only after a read confirms no review exists', async () => {
  const app = await fixture({ oversized: true, format }); await app.run();
  const resume = app.calls.find(call => call.body.action === 'resume'), exported = app.calls.find(call => call.url === 'native-legacy');
  assert.ok(resume); assert.equal(resume.body.path, 'A.md'); assert.ok(exported); assert.equal(exported.body.source_path, 'A.md');
  assert.equal(exported.body.source_identity.tab_id, app.tab.id);
  assert.equal(app.calls.some(call => call.body.action === 'open' || call.body.action === 'prepare'), false, 'no review or snapshot database creation');
});

for (const mode of ['denyResume', 'resumeFound']) test('oversized source cannot bypass review protection when ' + mode, async () => {
  const app = await fixture({ oversized: true, [mode]: true }); await app.run();
  assert.equal(app.calls.some(call => call.url === 'native-legacy'), false);
  assert.equal(app.calls.some(call => call.body.action === 'open' || call.body.action === 'prepare'), false);
});

for (const engine of ['plantuml', 'puml']) test(engine + ' is rendered to a frozen SVG before commit without enabling remote rendering', async () => {
  const app = await fixture(), requests = [], svg = '<svg xmlns="http://www.w3.org/2000/svg"><text>A</text></svg>';
  app.context.apiFetch = async (_url, options) => {
    const body = JSON.parse(options.body); requests.push(body);
    return { ok: true, json: async () => body.action === 'fences' ? { ok: true, fences: [{ engine, key: 'plant-' + engine, code: 'A -> B' }] } : { ok: true, svg } };
  };
  const svgNode = { setAttribute() {}, querySelectorAll: () => [] };
  app.context.document.createElement = () => ({ style: {}, remove() {}, querySelector: name => name === 'svg' ? svgNode : null });
  app.context.document.body = { append() {} };
  app.context.Element = class {};
  app.context.XMLSerializer = class { serializeToString() { return svg; } };
  app.context.diagramOutputMarkup = value => value;
  app.context.renderLocalDiagram = () => { throw new Error('PlantUML must use the local API renderer'); };
  const options = await app.actualPrepareFigures('```' + engine + '\nA -> B\n```', {});
  assert.equal(requests[1].action, 'render'); assert.equal(requests[1].engine, engine); assert.equal(requests[1].allow_remote, false);
  assert.deepEqual(clone(options.figures.rendered), [{ key: 'plant-' + engine, svg }]);
});

test('missing PDF engine and fonts install after prepare, then bind before figures without refreshing source or accepting newer C', async () => {
  const gate = { entered: barrier(), release: barrier() };
  const app = await fixture({ format: 'pdf', cjk: true, options: { pdf: { engine: 'typst' } }, requiredCapabilities: ['typst', 'fonts'], trackCapabilities: true, holdPublication: gate });
  const exporting = app.run(); await gate.entered.promise;
  const prepared = [...app.frozen.values()][0]; assert.equal(prepared.runtime_ready, false); assert.equal(prepared.content, '中文 AI A / AI B');
  assert.deepEqual(app.snapshot.changes.map(change => change.decision), ['pending', 'pending']);
  const stamp = await app.review.capture(app.tab);
  await app.review.propose([{ from: stamp.content.length, to: stamp.content.length, insert: ' / AI C' }], stamp, app.tab);
  gate.release.resolve(); await exporting;
  const order = app.calls.filter(call => call.url !== '/api/review').map(call => call.url === 'capability' ? 'ensure:' + call.body.id : call.url === 'publication' ? 'ensure:publication' : call.url === 'figures' ? 'render:figures' : call.body.action);
  assert.deepEqual(order, ['prepare', 'ensure:typst', 'ensure:publication', 'runtime', 'render:figures', 'figures', 'ensure:publication', 'runtime', 'commit']);
  assert.equal(app.figureInputs[0], '中文 AI A / AI B');
  assert.deepEqual(app.snapshot.changes.map(change => change.decision), ['accepted', 'accepted', 'pending']);
  assert.equal(app.calls.filter(call => call.body.action === 'prepare').length, 1);
  assert.equal(app.calls.find(call => call.body.action === 'runtime').body.snapshot_id, prepared.snapshot_id);
});

test('DOCX publication fonts use the frozen projection-safe PDF engine instead of inherited TeX settings', async () => {
  const app = await fixture({ format: 'docx', cjk: true, options: { pdf: { engine: 'xelatex' } }, requiredCapabilities: ['fonts'], trackCapabilities: true });
  await app.run();
  const publication = app.calls.find(call => call.url === 'publication'); assert.ok(publication);
  assert.equal(publication.body.options.pdf.engine, 'rust'); assert.equal(app.state.export.options.pdf.engine, 'xelatex');
  assert.equal(app.calls.some(call => call.url === 'capability'), false, 'irrelevant TeX settings do not install a TeX engine for DOCX');
  const order = app.calls.filter(call => call.url !== '/api/review').map(call => call.url === 'publication' ? 'ensure:publication' : call.url === 'figures' ? 'render:figures' : call.body.action);
  assert.deepEqual(order, ['prepare', 'ensure:publication', 'runtime', 'render:figures', 'figures', 'ensure:publication', 'runtime', 'commit']);
});

for (const denied of ['typst', 'fonts']) test('declining required ' + denied + ' installation discards only the prepared snapshot and preserves review decisions', async () => {
  const app = await fixture({ format: 'pdf', cjk: true, options: { pdf: { engine: 'typst' } }, requiredCapabilities: ['typst', 'fonts'], denyCapability: denied, trackCapabilities: true });
  await app.run();
  assert.deepEqual(app.snapshot.changes.map(change => change.decision), ['pending', 'pending']);
  assert.deepEqual(app.calls.filter(call => call.url === '/api/export/snapshot').map(call => call.body.action), ['prepare', 'discard']);
  assert.equal(app.figureInputs.length, 0);
});

for (const entry of [
  { format: 'pdf', engine: 'typst', required: ['typst', 'fonts'], cjk: true },
  { format: 'pdf', engine: 'xelatex', required: ['latex'] },
  { format: 'docx', engine: 'pdflatex', required: ['fonts'], cjk: true },
]) test(entry.format + '/' + entry.engine + ' preview keeps the install card and reuses its frozen snapshot after missing runtime installation', async () => {
  const app = await fixture({ format: entry.format, cjk: entry.cjk, options: { pdf: { engine: entry.engine } }, requiredCapabilities: entry.required, trackCapabilities: true });
  let nextTurn;
  app.context.setTimeout = callback => { nextTurn = callback; return 1; }; app.context.clearTimeout = () => {};
  app.context.renderNativeExportPreview = () => {};
  app.context._t = app.context.window.i18n.t;
  const element = () => ({ children: [], classList: { add() {}, remove() {} }, remove() {}, setAttribute() {}, replaceChildren() { this.children = []; }, append(...children) { this.children.push(...children); } });
  const capability = element(); app.nodes.set('export-preview-capability', capability);
  app.nodes.set('export-preview-card', element());
  app.nodes.set('export-preview-modal', { classList: { contains: () => true }, querySelector: () => null });
  app.context.document.createElement = element;
  const options = app.context.collectExportOptions();
  app.context.updateExportLivePreview = () => app.context.requestNativeExportPreview(entry.format, options, app.snapshot.content, 'A', '');
  app.context.updateExportLivePreview(); await nextTurn();
  assert.equal(app.calls.some(call => ['capability', 'publication', 'figures'].includes(call.url)), false, 'opening a preview does not prompt for installation or begin rendering');
  for (const id of entry.required) {
    assert.equal(capability.children.length, 2, 'existing explanatory text and install button');
    const install = capability.children[1]; assert.equal(install.textContent, 'capability.installContinue');
    await install.onclick({ stopPropagation() {} });
    assert.equal(app.calls.filter(call => call.url === 'capability').at(-1).body.id, id);
    await nextTurn();
  }
  const preview = app.calls.find(call => call.url === '/api/export/preview'); assert.ok(preview);
  await app.run();
  assert.equal(app.calls.filter(call => call.body.action === 'prepare').length, 1);
  assert.equal(app.calls.filter(call => call.body.action === 'figures').length, 1);
  assert.equal(app.calls.find(call => call.body.action === 'commit').body.snapshot_id, preview.body.snapshot_id);
  assert.ok(app.calls.filter(call => call.body.action === 'runtime').every(call => call.body.snapshot_id === preview.body.snapshot_id));
});

test('CJK labels rendered from ASCII source install fonts before the second runtime bind for PDF and DOCX without refreshing the snapshot', async () => {
  const svg = '<svg xmlns="http://www.w3.org/2000/svg"><text>中文轴标</text></svg>';
  for (const format of ['pdf', 'docx']) {
    const gate = { entered: barrier(), release: barrier() };
    const app = await fixture({ format, options: { pdf: { engine: format === 'docx' ? 'xelatex' : 'rust' } }, figuresRequireFonts: true, figureSvg: svg, trackCapabilities: true, holdFigures: gate });
    const exporting = app.run(); await gate.entered.promise;
    const prepared = [...app.frozen.values()][0]; assert.equal(prepared.runtime_ready, true); assert.equal(prepared.content, 'AI A / AI B');
    const stamp = await app.review.capture(app.tab); await app.review.propose([{ from: stamp.content.length, to: stamp.content.length, insert: ' / AI C' }], stamp, app.tab);
    gate.release.resolve(); await exporting;
    const publications = app.calls.filter(call => call.url === 'publication'); assert.equal(publications.length, 2);
    assert.equal(publications[0].body.options.figures, undefined); assert.equal(publications[1].body.options.figures.rendered[0].svg, svg);
    assert.ok(publications.every(call => call.body.content === 'AI A / AI B' && call.body.options.pdf.engine === 'rust'));
    const order = app.calls.filter(call => call.url !== '/api/review').map(call => call.url === 'publication' ? 'publication' : call.url === 'figures' ? 'render' : call.body.action);
    assert.deepEqual(order, ['prepare', 'publication', 'runtime', 'render', 'figures', 'publication', 'runtime', 'commit']);
    assert.deepEqual(app.snapshot.changes.map(change => change.decision), ['accepted', 'accepted', 'pending']);
    assert.equal(app.calls.filter(call => call.body.action === 'prepare').length, 1); assert.equal(app.calls.filter(call => call.body.action === 'figures').length, 1);
    assert.ok(app.calls.filter(call => ['runtime', 'commit'].includes(call.body.action)).every(call => call.body.snapshot_id === prepared.snapshot_id));
  }
});

test('CJK SVG labels discovered after preview rendering show the existing font install card and retain that snapshot and its rendered figures', async () => {
  const app = await fixture({ format: 'docx', options: { pdf: { engine: 'xelatex' } }, figuresRequireFonts: true, figureSvg: '<svg><text>中文轴标</text></svg>', trackCapabilities: true }); let nextTurn;
  app.context.setTimeout = callback => { nextTurn = callback; return 1; }; app.context.clearTimeout = () => {};
  app.context.renderNativeExportPreview = () => {}; app.context._t = app.context.window.i18n.t;
  const element = () => ({ children: [], classList: { add() {}, remove() {} }, remove() {}, setAttribute() {}, replaceChildren() { this.children = []; }, append(...children) { this.children.push(...children); } });
  const capability = element(); app.nodes.set('export-preview-capability', capability); app.nodes.set('export-preview-card', element());
  app.nodes.set('export-preview-modal', { classList: { contains: () => true }, querySelector: () => null }); app.context.document.createElement = element;
  const options = app.context.collectExportOptions(); app.context.updateExportLivePreview = () => app.context.requestNativeExportPreview('docx', options, app.snapshot.content, 'A', '');
  app.context.updateExportLivePreview(); await nextTurn();
  assert.equal(app.calls.some(call => ['capability', 'publication', '/api/export/preview'].includes(call.url)), false, 'preview does not prompt or publish with missing fonts');
  assert.equal(capability.children[0].textContent, 'error.publication_fonts_unavailable'); assert.equal(capability.children[1].textContent, 'capability.installContinue');
  assert.deepEqual(app.calls.filter(call => call.url === '/api/export/snapshot').map(call => call.body.action), ['prepare', 'runtime', 'figures', 'runtime']);
  assert.deepEqual(app.snapshot.changes.map(change => change.decision), ['pending', 'pending']);
  await capability.children[1].onclick({ stopPropagation() {} }); await nextTurn();
  const preview = app.calls.find(call => call.url === '/api/export/preview'); assert.ok(preview); await app.run();
  assert.equal(app.calls.filter(call => call.body.action === 'prepare').length, 1); assert.equal(app.calls.filter(call => call.body.action === 'figures').length, 1); assert.equal(app.figureInputs.length, 1);
  assert.ok(app.calls.filter(call => ['runtime', 'commit'].includes(call.body.action)).every(call => call.body.snapshot_id === preview.body.snapshot_id));
  assert.equal(app.calls.find(call => call.url === 'capability').body.id, 'fonts');
});
