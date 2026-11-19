'use strict';
// Frontend protocol/DOM doubles. These checks run the production dispatch,
// Office viewer, tab recovery and readonly guards without a desktop or engine.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { randomUUID } = require('node:crypto');
const read = file => fs.readFileSync(path.join(__dirname, '..', file), 'utf8');
const sources = Object.fromEntries(Object.entries({ reader: 'assets/js/reader/render.js', office: 'assets/js/reader/office-preview.js',
  tabs: 'assets/js/core/tabs.js', history: 'assets/js/core/history.js', review: 'assets/js/editor/review.js',
  editor: 'assets/js/editor/preview.js', app: 'assets/app.js' }).map(([key, file]) => [key, read(file)]));
const segment = (source, start, end) => source.slice(source.indexOf(start), source.indexOf(end, source.indexOf(start)));
const clone = value => JSON.parse(JSON.stringify(value));
const DOCX = '/Synthetic/ReadMD/中文 原稿.docx', PPTX = '/Synthetic/ReadMD/演示.pptx';
const VIEW = '00000000-0000-4000-8000-000000000001', OTHER_VIEW = '00000000-0000-4000-8000-000000000002';
const SOURCE = 'a'.repeat(64), OTHER_SOURCE = 'b'.repeat(64);
function deferred() { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; }
class Element {
  constructor(tag = 'div') {
    this.tagName = tag.toUpperCase(); this.children = []; this.dataset = {}; this.attributes = {}; this.listeners = {}; this.className = ''; this.scrollTop = 0;
    this.style = { setProperty() {}, removeProperty() {} };
    this.classList = {
      contains: name => this.className.split(/\s+/).includes(name),
      add: (...names) => { this.className = [...new Set([...this.className.split(/\s+/).filter(Boolean), ...names])].join(' '); },
      remove: (...names) => { this.className = this.className.split(/\s+/).filter(name => !names.includes(name)).join(' '); },
      toggle: (name, force) => { const enabled = force ?? !this.classList.contains(name); this.classList[enabled ? 'add' : 'remove'](name); return enabled; },
    };
  }
  append(...children) { this.children.push(...children); }
  get options() { return this.children; }
  get value() { return this._value ?? ''; }
  set value(value) { this._value = String(value); }
  get innerHTML() { return this._html || ''; }
  set innerHTML(value) { this._html = value; this.children = []; }
  appendChild(child) { this.children.push(child); return child; }
  replaceChildren(...children) { this.children = children; }
  setAttribute(key, value) { this.attributes[key] = String(value); }
  getAttribute(key) { return this.attributes[key] ?? null; }
  removeAttribute(key) { delete this.attributes[key]; if (key === 'src') this.src = ''; }
  querySelectorAll() { return []; }
  addEventListener(key, callback) { this.listeners[key] = callback; }
  focus() {}
  reportValidity() { return false; }
}
function png(page = 0, { view = VIEW, source = SOURCE, pages = 2, type = 'image/png', omitPage = false } = {}) {
  const headers = { 'Content-Type': type, 'X-ReadMD-View': view, 'X-ReadMD-Pages': String(pages) };
  if (source !== null) headers['X-ReadMD-Source'] = source;
  if (!omitPage) headers['X-ReadMD-Page'] = String(page);
  return new Response(new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]), { headers });
}
function fixture() {
  const calls = [], toasts = [], local = new Map(), created = [], revoked = [], views = new Map();
  const state = { tabs: [], activeTabId: null, mode: 'welcome', editing: false, original: '', fixed: '', folder: '', scrollPos: {}, pagination: {} };
  const ids = ['content', 'status-left', 'status-right', 'status-summary', 'status-words', 'status-reading-time', 'status-format', 'status-encoding', 'statusbar',
    'btn-edit', 'btn-reload', 'btn-saveas', 'btn-document-copy', 'btn-print', 'btn-search', 'btn-presentation-menu', 'btn-run-all-chunks', 'btn-share', 'btn-fix',
    'btn-graph-menu', 'btn-backlinks-menu', 'btn-graph', 'btn-home', 'pagination-bar', 'pagination-live', 'status-pagination',
    'pg-first-btn', 'pg-prev-btn', 'pg-next-btn', 'pg-last-btn', 'pg-page-select', 'pg-total-label', 'pg-chapter-label',
    'pg-mode-toggle', 'pg-mode-icon-continuous', 'pg-mode-icon-paged'];
  const elements = new Map(ids.map(id => [id, new Element()]));
  const listeners = new Map();
  const listen = (key, work) => { if (!listeners.has(key)) listeners.set(key, []); listeners.get(key).push(work); };
  const window = { crypto: { randomUUID }, READMD_ENGINE: 'rust', addEventListener: listen,
    i18n: { t: (key, params) => key + (params ? ':' + JSON.stringify(params) : '') }, ReadMDRecovery: { schedule() {}, sessionDrafts: async () => new Map(), mathDrafts: async () => { throw Error('Office has no Markdown math drafts'); } } };
  window.ReadMDSession = { restoring: () => false, busy: () => false, persist() {}, closeTabs: async (_tabs, apply) => { apply(); return true; } };
  const context = vm.createContext({ window, state, crypto: window.crypto, console, Element, AbortController, Blob, Response, TextEncoder, TextDecoder,
    Map, Set, URL: { createObjectURL: blob => { const url = 'blob:synthetic-' + created.length; created.push({ url, blob }); return url; }, revokeObjectURL: url => revoked.push(url) },
    document: { title: '', body: new Element(), hidden: false, activeElement: null, readyState: 'loading', createElement: tag => new Element(tag),
      addEventListener: listen, querySelectorAll: () => [], getElementById: id => elements.get(id) || null },
    localStorage: { getItem: key => local.get(key) ?? null, setItem: (key, value) => local.set(key, value) }, setTimeout, clearTimeout, requestAnimationFrame: callback => callback(),
    $: id => elements.get(id) || null, hasPy: false, py: {}, settingsSaveQueue: Promise.resolve(), normalizePath: value => String(value || '').replace(/\\/g, '/').toLowerCase(),
    MD_RE: /\.(md|txt)$/i, IMG_RE: /\.(png|jpg)$/i, CONVERT_BINARY_RE: /\.(docx?|pptx?|pdf|xlsx?)$/i,
    apiFetch: async (url, init = {}) => {
      const body = init.body ? JSON.parse(init.body) : null; calls.push({ url, body, init });
      if (url === '/api/task/cancel' || body?.close) return Response.json({ ok: true });
      if (url !== '/api/office/preview') throw Error('Unexpected content API: ' + url);
      return server(body, init);
    },
    apiMessage: (data, fallback) => data.error_code ? 'error.' + data.error_code : fallback,
    showToast: text => toasts.push(text), setFileTitle() {}, setFixes: (_fixes, stats) => { state.stats = stats; },
    closeSearch() {}, closeMdPopups() {}, buildToc() {}, updateAiContextStrip() {}, updateActiveTocHeading() {},
    renderContentIncremental: async content => { calls.push({ markdownContinuous: content }); },
    readerPrepare: text => text, transformWikilinks: text => text, transformAcademicCallouts: text => text,
    protectMath: src => ({ src, saved: [] }), restoreMath: text => text, marked: { parse: text => text }, sanitizeRenderedHtml: text => text, postProcess() {},
    renderTabsBar: () => window.ReadMDSession.persist(), renderContent: async () => { throw Error('Office must not render Markdown'); },
    prepareDocCitations: async () => { throw Error('Office must not extract citations'); },
    addRecent: at => calls.push({ recent: at }), pushHistory: at => calls.push({ history: at }), saveLastFile: at => calls.push({ last: at }), clearAiOutput() {},
    exitEdit: () => { state.editing = false; }, hasUnsavedEditorChanges: () => true, getEditContent: () => '# Unsaved original draft',
    setProgress() {}, promptDirtyClose: async () => 'cancel', afterRender() {}, focusVisibleTab() {},
    convertFile: async at => calls.push({ convert: at }), convertOrOcr: async at => calls.push({ convert: at }),
    resetHome: () => { void window.ReadMDOffice.releaseView(); state.mode = 'welcome'; state.activeTabId = null; },
  });
  let server = async body => { const response = png(body.page); views.set(VIEW, true); return response; };
  const run = source => vm.runInContext(source, context);
  run(segment(sources.tabs, 'let tabRenderEpoch', 'function revealTabInTrack'));
  run(segment(sources.reader, 'let readerRenderEpoch', 'function isSanctionedRenderButton('));
  run(segment(sources.reader, 'async function waitForSessionTransition(', 'async function openExternalFile('));
  run(sources.office);
  run(sources.review); context.ReadMDReview = window.ReadMDReview;
  run(segment(sources.reader, 'function splitMdIntoPages(', 'async function processDocImports('));
  run(segment(sources.reader, 'function toggleZenMode(', '// Focus chrome and window fullscreen'));
  context.initPaginationEvents();
  run(segment(sources.history, 'function setUnavailableReason(', 'let goingHome ='));
  run(segment(sources.tabs, 'async function renderActiveTab(', 'async function activateTabForSave('));
  run(segment(sources.tabs, 'async function switchTab(', 'function promptDirtyClose('));
  run(segment(sources.tabs, 'async function renameTab(', 'function reorderTabs('));
  run(segment(sources.tabs, 'async function closeTab(', '/* Persist the open page set,'));
  run(segment(sources.reader, 'async function openDocumentPath(', 'async function loadFileDialog('));
  run(segment(sources.app, 'async function openInitialFile(', '/* ----------------------------------------------------------------------------------------------\n  应用启动引导序列'));
  run(segment(sources.editor, 'async function toggleEdit(', 'function exitEdit('));
  run(segment(sources.editor, 'async function saveAs(', 'async function saveAsSnapshot('));
  const tick = () => new Promise(done => setImmediate(done));
  const painted = async () => { for (let i = 0; i < 20; i++) { await tick(); if (elements.get('content').children[0]?.getAttribute('aria-busy') === 'false') return; } throw Error('Synthetic paint did not settle'); };
  return { context, window, state, elements, calls, toasts, local, created, revoked, run, tick, painted,
    click: id => elements.get(id).listeners.click({}),
    select: value => { const el = elements.get('pg-page-select'); el.value = value; el.listeners.change({ target: el }); },
    key: (key, modifiers = {}) => {
      const event = { key, altKey: true, ctrlKey: false, shiftKey: false, metaKey: false, preventDefault() { this.prevented = true; }, ...modifiers };
      listeners.get('keydown').at(-1)(event); return event;
    },
    setServer: work => { server = work; },
    restore: async descriptor => {
      local.set('readmd-session', JSON.stringify({ version: 1, tabs: [descriptor], active: descriptor.key, folder: '' }));
      run(sources.tabs.slice(sources.tabs.indexOf('(function () {\n  const storageKey')));
      await window.ReadMDSession.restore();
    } };
}
test('normal load/Open/OS uses one source tab with page images and readonly actions', async t => {
  const f = fixture(); t.after(() => f.window.ReadMDOffice.releaseView());
  assert.equal(await f.context.loadFile(DOCX), true); await f.painted();
  const tab = f.state.tabs[0], root = f.elements.get('content').children[0];
  assert.equal(tab.path, DOCX); assert.equal(tab.name, '中文 原稿.docx'); assert.equal(tab.mode, 'office-preview');
  assert.equal(f.state.original, null); assert.equal(f.state.encoding, '');
  assert.equal(root.children.length, 2); assert.equal(root.children[1].tagName, 'IMG'); assert.ok(root.children[1].src.startsWith('blob:'));
  for (const id of ['btn-edit', 'btn-saveas', 'btn-print', 'btn-document-copy', 'btn-search', 'btn-share']) assert.equal(f.elements.get(id).disabled, true, id);
  assert.equal(f.elements.get('btn-reload').disabled, false);
  await f.context.toggleEdit(); assert.equal(f.state.editing, false); assert.equal(await f.context.saveAs(), false);
  assert.equal(await f.window.ReadMDReview.capture(), null);
  await f.context.renameTab(tab.id, 'changed.docx'); assert.equal(tab.name, '中文 原稿.docx'); assert.equal(tab.path, DOCX);
  await f.context.openDocumentPath(DOCX); await f.painted(); assert.equal(f.state.tabs.length, 1); assert.equal(f.state.tabs[0], tab);
  await f.context.openInitialFile(PPTX); await f.painted(); assert.equal(f.state.tabs.length, 2); assert.equal(f.state.tabs[1].path, PPTX);
  assert.ok(!f.calls.some(call => call.convert || /\/api\/(?:file|convert|review|save)/.test(call.url || '')));
});
test('opening Office preserves the unrelated dirty Markdown editor', async t => {
  const f = fixture(); t.after(() => f.window.ReadMDOffice.releaseView());
  const previous = { id: 'markdown', mode: 'file', path: '/Synthetic/notes.md', content: '# Saved', original: '# Saved', isDirty: false };
  f.state.tabs = [previous]; f.state.activeTabId = previous.id; f.state.editing = true;
  await f.context.loadFile(DOCX); await f.painted();
  assert.equal(previous.content, '# Unsaved original draft'); assert.equal(previous.original, '# Saved'); assert.equal(previous.isDirty, true);
  assert.equal(f.state.tabs.length, 2); assert.equal(f.state.editing, false);
  assert.ok(!f.calls.some(call => /\/api\/(?:save|convert)/.test(call.url || '')));
});
test('a visible PDF workspace can veto Office activation through its existing close guard', async t => {
  const f = fixture(); t.after(() => f.window.ReadMDOffice.releaseView());
  const previous = { id: 'markdown', mode: 'file', path: '/Synthetic/notes.md', content: '# Saved', original: '# Saved', isDirty: false };
  f.state.tabs = [previous]; f.state.activeTabId = previous.id; f.state.mode = 'file';
  const modal = new Element(); modal.className = 'modal'; f.elements.set('pdf-modal', modal);
  let attempts = 0;
  f.window.ReadMDPdf = { prepareClose: async () => { attempts++; return false; } };
  assert.equal(await f.context.openInitialFile(DOCX), false);
  assert.equal(attempts, 1); assert.deepEqual(f.state.tabs, [previous]); assert.equal(f.state.activeTabId, previous.id); assert.equal(f.calls.length, 0);
  f.window.ReadMDPdf.prepareClose = async () => { attempts++; modal.classList.add('hidden'); return true; };
  assert.equal(await f.context.openInitialFile(DOCX), true); await f.painted();
  assert.equal(attempts, 2); assert.equal(f.state.tabs.length, 2); assert.equal(f.state.tabs[1].path, DOCX);
});
test('browser uploads retain the original Office filename, extension and copy badge', async t => {
  const f = fixture(); t.after(() => f.window.ReadMDOffice.releaseView());
  f.run(segment(read('assets/js/features/convert.js'), 'async function uploadFile(', 'function convertOrOcr('));
  const originalFetch = f.context.apiFetch, uploads = [];
  f.context.apiFetch = async (url, options) => {
    if (url.startsWith('/api/upload?')) { uploads.push({ url, options }); return Response.json({ path: DOCX }); }
    return originalFetch(url, options);
  };
  const file = { name: '中文 原稿.docx' }, uploaded = await f.context.uploadFile(file);
  await f.context.openDocumentPath(uploaded, { browserCopy: true }); await f.painted();
  const query = new URL(uploads[0].url, 'http://synthetic').searchParams;
  assert.equal(query.get('name'), file.name); assert.equal(query.get('ext'), '.docx'); assert.equal(uploads[0].options.body, file);
  assert.equal(f.state.tabs[0].path, DOCX); assert.equal(f.state.tabs[0].name, file.name); assert.equal(f.state.tabs[0].browserCopy, true);
  assert.ok(f.state.tabs[0].title.includes('app.browserCopy'));
});
test('folder listing requests the reader view and sidebar rows use normal document opening', async () => {
  const f = fixture(); f.run(read('assets/js/reader/folder.js')); f.context.renderFolderList = () => {};
  const requested = [], opened = [];
  f.context.apiFetch = async url => { requested.push(url); return Response.json({ dir: '/Synthetic/ReadMD', files: [DOCX, PPTX] }); };
  await f.context.listFolder('/Synthetic/ReadMD', { reveal: false });
  assert.equal(new URL(requested[0], 'http://synthetic').searchParams.get('view'), 'reader');
  f.context.openDocumentPath = at => opened.push(at);
  const container = new Element();
  f.context.renderTreeNodes(container, { doc: { name: 'word.docx', path: DOCX, type: 'file' },
    slides: { name: 'slides.pptx', path: PPTX, type: 'file' }, pdf: { name: 'source.pdf', path: '/Synthetic/source.pdf', type: 'file' } }, 0);
  for (const node of container.children) node.children[0].listeners.click({ stopPropagation() {} });
  assert.deepEqual(new Set(opened), new Set([DOCX, PPTX, '/Synthetic/source.pdf']));
});
test('open-file sidebar fallback shows the same browser-copy name as the document tab', async t => {
  const f = fixture(); t.after(() => f.window.ReadMDOffice.releaseView());
  await f.context.openDocumentPath(DOCX, { browserCopy: true }); await f.painted();
  f.elements.set('file-list', new Element()); f.state.folderFiles = [];
  f.run(read('assets/js/reader/folder.js')); f.context.syncFileSidebar(true);
  const row = f.elements.get('file-list').children[0].children[1].children[0].children[0];
  assert.equal(row.children[1].textContent, f.state.tabs[0].title);
  assert.ok(row.children[1].textContent.includes('app.browserCopy'));
});
test('pager reuses its view, reopens an expired view once, and revokes prior image bytes', async t => {
  const f = fixture(); t.after(() => f.window.ReadMDOffice.releaseView());
  await f.context.loadFile(DOCX); await f.painted();
  assert.equal(f.elements.get('pg-prev-btn').disabled, true); assert.equal(f.elements.get('pg-next-btn').disabled, false);
  let expired = true;
  f.setServer(async body => { if (body.view && expired) { expired = false; return Response.json({ error_code: 'expired' }, { status: 410 }); } return png(body.page, { view: OTHER_VIEW }); });
  f.click('pg-next-btn'); await f.painted();
  assert.equal(f.state.tabs[0].readerPage, 1); assert.equal(f.elements.get('pg-page-select').value, '1'); assert.equal(f.elements.get('pg-next-btn').disabled, true);
  assert.ok(f.calls.some(call => call.body?.view === VIEW && call.body?.page === 1));
  assert.equal(f.calls.filter(call => call.body?.path === DOCX).length, 2);
  assert.equal(f.calls.find(call => call.body?.path === DOCX && call.body?.page === 1).body.expected_source, SOURCE);
  assert.deepEqual(f.revoked, ['blob:synthetic-0']);
  assert.ok(!JSON.stringify(f.state.tabs).includes('blob:')); assert.ok(!JSON.stringify(f.state.tabs).includes(OTHER_VIEW));
});
test('a changed source never replaces the confirmed page, including after view expiry', async t => {
  for (const expired of [false, true]) {
    const f = fixture(); t.after(() => f.window.ReadMDOffice.releaseView());
    await f.context.loadFile(DOCX); await f.painted();
    const tab = f.state.tabs[0], image = f.elements.get('content').children[0].children[1], previousUrl = image.src;
    f.setServer(async body => expired && body.view ? Response.json({ error_code: 'expired' }, { status: 410 }) :
      png(body.page, { view: OTHER_VIEW, source: OTHER_SOURCE }));
    f.click('pg-next-btn'); await f.painted();
    assert.equal(tab.readerPage, 0); assert.equal(f.state.pagination.currentPage, 0); assert.equal(f.state.pagination.error, true);
    assert.equal(image.src, previousUrl); assert.equal(f.created.length, 1); assert.equal(f.revoked.length, 0);
    assert.ok(f.calls.some(call => call.body?.view === OTHER_VIEW && call.body?.close));
    if (expired) assert.equal(f.calls.find(call => call.body?.path === DOCX && call.body?.page === 1).body.expected_source, SOURCE);
    f.setServer(async body => png(body.page, { view: VIEW }));
    f.click('pg-next-btn'); await f.painted();
    assert.equal(tab.readerPage, 1); assert.equal(f.state.pagination.error, false);
    assert.deepEqual(f.revoked, [previousUrl]);
  }
});
test('Office uses the existing pagination controls, zero-based selection and keyboard directions', async t => {
  const f = fixture(); t.after(() => f.window.ReadMDOffice.releaseView());
  f.setServer(async body => png(body.page, { pages: 3 }));
  const bar = f.elements.get('pagination-bar'), select = f.elements.get('pg-page-select');
  await f.context.loadFile(DOCX); await f.painted();
  assert.equal(f.elements.get('pagination-bar'), bar); assert.equal(f.elements.get('pg-page-select'), select);
  assert.equal(f.elements.get('content').children[0].children.length, 2, 'Only status and the page image are inserted into the reader');
  assert.equal(bar.classList.contains('hidden'), false); assert.equal(bar.getAttribute('aria-busy'), 'false');
  assert.equal(f.state.pagination.kind, 'office'); assert.equal(f.state.pagination.enabled, true); assert.equal(f.state.pagination.currentPage, 0);
  assert.deepEqual(select.options.map(option => [option.value, option.textContent]), [['0', '1'], ['1', '2'], ['2', '3']]);
  assert.equal(select.value, '0'); assert.equal(f.elements.get('pg-total-label').textContent, '/ 3');
  assert.equal(f.elements.get('pg-mode-toggle').hidden, true); assert.equal(f.elements.get('pg-mode-toggle').disabled, true);
  assert.equal(f.elements.get('status-pagination').disabled, true);
  assert.ok(f.elements.get('pagination-live').textContent.includes('"current":1,"total":3'));
  f.click('pg-last-btn'); await f.painted(); assert.equal(f.state.pagination.currentPage, 2); assert.equal(select.value, '2');
  assert.equal(f.elements.get('pg-next-btn').disabled, true); assert.ok(f.elements.get('pagination-live').textContent.includes('"current":3'));
  f.click('pg-first-btn'); await f.painted(); assert.equal(f.state.pagination.currentPage, 0);
  f.select('1'); await f.painted(); assert.equal(f.state.tabs[0].readerPage, 1);
  assert.equal(f.key('ArrowRight').prevented, true); await f.painted(); assert.equal(f.state.pagination.currentPage, 2);
  assert.equal(f.key('Home').prevented, true); await f.painted(); assert.equal(f.state.pagination.currentPage, 0);
  const callsBeforeBoundary = f.calls.filter(call => call.body?.page !== undefined).length;
  f.key('ArrowLeft'); await f.painted(); assert.equal(f.state.pagination.currentPage, 0);
  assert.equal(f.calls.filter(call => call.body?.page !== undefined).length, callsBeforeBoundary);
  f.key('End'); await f.painted(); assert.equal(f.state.pagination.currentPage, 2);
  assert.equal(f.key('ArrowLeft', { ctrlKey: true }).prevented, undefined, 'The existing modifier gate remains intact');
  await f.context.togglePaginationMode(); assert.equal(f.state.pagination.mode, 'paged');
});
test('page failures preserve the displayed page, restore shared controls and allow retry', async t => {
  const f = fixture(); t.after(() => f.window.ReadMDOffice.releaseView());
  f.setServer(async () => Response.json({ error_code: 'office_preview_failed' }, { status: 503 }));
  await f.context.loadFile(DOCX); await f.painted();
  assert.equal(f.state.pagination.error, true); assert.equal(f.state.pagination.totalPages, 0);
  for (const id of ['pg-first-btn', 'pg-prev-btn', 'pg-next-btn', 'pg-last-btn', 'pg-page-select']) assert.equal(f.elements.get(id).disabled, true, id);
  f.setServer(async body => png(body.page, { pages: 3 }));
  await f.context.loadFile(DOCX, { force: true }); await f.painted();
  const image = f.elements.get('content').children[0].children[1], firstUrl = image.src, pending = deferred();
  f.setServer(async () => pending.promise);
  f.click('pg-next-btn');
  assert.equal(f.state.pagination.loading, true); assert.equal(f.elements.get('pagination-bar').getAttribute('aria-busy'), 'true');
  assert.equal(f.elements.get('pg-page-select').disabled, true); assert.equal(f.state.pagination.currentPage, 0);
  f.select('2'); assert.equal(f.elements.get('pg-page-select').value, '0', 'A pending page cannot replace the confirmed selection');
  pending.resolve(Response.json({ error_code: 'office_preview_failed' }, { status: 503 })); await f.painted();
  assert.equal(f.state.pagination.loading, false); assert.equal(f.state.pagination.currentPage, 0); assert.equal(f.state.tabs[0].readerPage, 0);
  assert.equal(image.src, firstUrl); assert.equal(f.elements.get('pg-page-select').value, '0'); assert.equal(f.elements.get('pg-next-btn').disabled, false);
  assert.equal(f.elements.get('pagination-live').textContent, 'editor.previewRenderFail');
  f.setServer(async body => png(body.page, { pages: 3 })); f.click('pg-next-btn'); await f.painted();
  assert.equal(f.state.pagination.currentPage, 1); assert.equal(f.state.pagination.error, false); assert.equal(f.elements.get('pg-page-select').value, '1');
  assert.ok(image.alt.includes('"current":2,"total":3'));
});
test('switching back to Markdown restores its reading mode and native Zen behavior', async t => {
  const f = fixture(); t.after(() => f.window.ReadMDOffice.releaseView());
  await f.context.loadFile(DOCX); await f.painted();
  const office = f.state.tabs[0], markdown = { id: 'markdown', mode: 'file', path: '/Synthetic/long.md', name: 'long.md',
    content: '# Markdown', original: '# Markdown', readerMode: 'continuous', readerPage: 1, isDirty: false };
  f.state.tabs.push(markdown); f.window.ReadMDReview.restore = async () => null;
  f.context.renderContent = async () => {
    f.context.beginReaderRender();
    Object.assign(f.state.pagination, { enabled: true, rawContent: '# Markdown', pages: [
      { pageIndex: 0, title: '', content: 'Markdown page 1' }, { pageIndex: 1, title: '', content: 'Markdown page 2' },
    ], allHeadings: [], totalPages: 2, currentPage: 0 });
    f.context.updatePaginationBar();
  };
  await f.context.switchTab(markdown.id); await f.tick();
  assert.equal(f.state.mode, 'file'); assert.equal(f.state.pagination.kind, undefined); assert.equal(f.state.pagination.mode, 'continuous');
  assert.equal(f.elements.get('pg-mode-toggle').hidden, false); assert.equal(f.elements.get('pg-mode-toggle').disabled, false);
  assert.equal(f.elements.get('status-pagination').disabled, false); assert.equal(f.elements.get('pg-mode-icon-paged').classList.contains('hidden'), false);
  assert.equal(f.elements.get('pg-page-select').disabled, true); assert.equal(f.elements.get('pg-page-select').options[0].textContent, '1. 1');
  await f.context.togglePaginationMode(); assert.equal(f.state.pagination.mode, 'paged'); assert.equal(f.elements.get('pg-page-select').disabled, false);
  f.key('End'); assert.equal(f.state.pagination.currentPage, 1);
  await f.context.togglePaginationMode(); assert.equal(f.state.pagination.mode, 'continuous'); assert.ok(f.calls.some(call => call.markdownContinuous === '# Markdown'));
  await f.context.switchTab(office.id); await f.painted();
  f.context.toggleZenMode(true); assert.equal(f.context.document.body.classList.contains('zen-mode'), true);
  f.key('ArrowRight'); await f.painted(); assert.equal(f.state.pagination.currentPage, 1);
  assert.equal(f.elements.get('pg-mode-toggle').hidden, true);
  await f.context.switchTab(markdown.id); await f.tick();
  assert.equal(f.context.document.body.classList.contains('zen-mode'), true); assert.equal(f.state.pagination.mode, 'continuous');
  assert.equal(f.elements.get('pg-mode-toggle').hidden, false); assert.equal(f.elements.get('pg-mode-toggle').disabled, false);
  f.context.toggleZenMode(false); assert.equal(f.context.document.body.classList.contains('zen-mode'), false);
  assert.equal(f.elements.get('pagination-bar').classList.contains('hidden'), false);
});
test('explicit close cancels loading and ignores late page responses without resurrecting a tab', async () => {
  const f = fixture(), response = deferred(), entered = deferred();
  f.setServer(async () => { entered.resolve(); return response.promise; });
  await f.context.loadFile(DOCX); await entered.promise;
  await f.context.closeTab(f.state.tabs[0].id);
  assert.equal(f.state.tabs.length, 0); assert.ok(f.calls.some(call => call.url === '/api/task/cancel'));
  response.resolve(png()); await f.tick(); await f.tick();
  assert.equal(f.state.tabs.length, 0); assert.equal(f.created.length, 0);
  assert.ok(f.calls.some(call => call.body?.view === VIEW && call.body?.close === true));
});
test('failed close persistence preserves the source tab, page bytes and server view', async t => {
  const f = fixture(); t.after(() => f.window.ReadMDOffice.releaseView());
  await f.context.loadFile(DOCX); await f.painted();
  const tab = f.state.tabs[0], image = f.elements.get('content').children[0].children[1], url = image.src;
  f.window.ReadMDSession.closeTabs = async () => false;
  await f.context.closeTab(tab.id);
  assert.equal(f.state.tabs[0], tab); assert.equal(image.src, url); assert.equal(f.revoked.length, 0);
  assert.ok(!f.calls.some(call => call.body?.close));
});
test('Office open waits for an in-flight session close before adding its source identity', async t => {
  const f = fixture(); t.after(() => f.window.ReadMDOffice.releaseView());
  let busy = true; const release = deferred();
  f.window.ReadMDSession.busy = () => busy; f.window.ReadMDSession.whenIdle = () => release.promise;
  const opening = f.context.loadFile(DOCX); await f.tick();
  assert.equal(f.state.tabs.length, 0); assert.equal(f.calls.length, 0);
  busy = false; release.resolve(); await opening; await f.painted();
  assert.equal(f.state.tabs[0].path, DOCX);
});
test('invalid page, source identity or image response stays in the source tab without extraction fallback', async t => {
  for (const response of [Response.json({ error_code: 'office_preview_failed' }, { status: 503 }), png(0, { omitPage: true }),
    png(0, { source: null }), png(0, { source: 'invalid' }), png(0, { type: 'text/html' })]) {
    const f = fixture(); t.after(() => f.window.ReadMDOffice.releaseView()); f.setServer(async () => response);
    await f.context.loadFile(DOCX); await f.painted();
    const root = f.elements.get('content').children[0];
    assert.equal(f.state.tabs[0].path, DOCX); assert.equal(root.children[1].hidden, true); assert.equal(root.children[0].hidden, false);
    assert.equal(f.created.length, 0); assert.ok(!f.calls.some(call => call.convert || call.url?.startsWith('/api/file')));
  }
});
test('session restores only Office source metadata, with no text read/review/citation recovery', async t => {
  const f = fixture(); t.after(() => f.window.ReadMDOffice.releaseView());
  await f.restore({ key: 'file:office', path: DOCX, name: '中文 原稿.docx', source: 'office', dirty: false, readerPage: 1 }); await f.painted();
  assert.equal(f.state.tabs.length, 1); assert.equal(f.state.tabs[0].mode, 'office-preview'); assert.equal(f.state.tabs[0].recoveryKey, 'file:office');
  assert.equal(f.state.tabs[0].readerPage, 1); assert.equal(f.state.original, null);
  const persisted = JSON.parse(f.local.get('readmd-session'));
  assert.equal(persisted.tabs[0].path, DOCX); assert.equal(persisted.tabs[0].readerPage, 1);
  assert.ok(!JSON.stringify(persisted).includes('blob:')); assert.ok(!JSON.stringify(persisted).includes(VIEW));
  assert.ok(!f.calls.some(call => /\/api\/(?:file|review|convert)/.test(call.url || '')));
});
test('dirty or rename-pending Office descriptors retain their recovery restriction without a binary text read', async () => {
  const pending = { operation_id: 'office-pending', old_path: DOCX, new_stem: 'renamed', expected_disk_revision: 'a'.repeat(64), expected_revision: 'b'.repeat(64) };
  for (const extra of [{ dirty: true }, { renamePending: pending }]) {
    const f = fixture(), descriptor = { key: 'file:protected-office', path: DOCX, name: '中文 原稿.docx', source: 'office', ...extra };
    await f.restore(descriptor);
    const tab = f.state.tabs[0];
    assert.deepEqual(clone(tab._restoreDescriptor), descriptor); assert.equal(tab.original, null);
    assert.equal(await f.context.closeTab(tab.id), false); assert.equal(f.state.tabs.length, 1);
    assert.ok(!f.calls.some(call => /\/api\/(?:office|file|review|convert)/.test(call.url || '')));
  }
});
