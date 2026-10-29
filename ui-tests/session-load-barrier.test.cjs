'use strict';
// Exercise the actual reader functions with controlled, asynchronous session
// transitions. A dropped operating-system open or import is a user-visible loss.
const test = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const path = require('node:path');

const source = fs.readFileSync(path.join(__dirname, '../assets/js/reader/render.js'), 'utf8');
const fileCode = source.slice(source.indexOf('async function waitForSessionTransition()'), source.indexOf('async function openExternalFile('));
const virtualCode = source.slice(source.indexOf('async function renderVirtual('), source.indexOf('async function convertFile('));
const tabsSource = fs.readFileSync(path.join(__dirname, '../assets/js/core/tabs.js'), 'utf8');
const switchCode = tabsSource.slice(tabsSource.indexOf('async function switchTab('), tabsSource.indexOf('\nfunction promptDirtyClose('));
function deferred() { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; }
function fixture() {
  let busy = false, idle = deferred(), calls = 0, firstResponse = null;
  const session = { busy: () => busy, whenIdle: () => idle.promise };
  const state = { tabs: [], activeTabId: null, editing: false, mode: 'welcome', file: null,
    pagination: { enabled: false, totalPages: 1 } };
  const doc = { path: 'synthetic.md', name: 'synthetic.md', dir: '', content: '# Synthetic\n', revision: 'r', encoding: 'utf-8' };
  const window = { ReadMDSession: session, i18n: { t: key => key }, invalidateDocumentLoads() {} };
  const noop = () => {};
  const context = { window, state, document: { title: '' }, hasPy: false, Math, Date, Promise,
    normalizePath: value => value.toLowerCase(), findTabByPath: value => state.tabs.find(tab => tab.path === value),
    showToast: noop, setProgress: noop, hasUnsavedEditorChanges: () => false, exitEdit: noop,
    syncStateFromActiveTab: noop, prepareDocCitations: async () => {}, setFixes: noop, renderContent: async () => {},
    updateStatus: noop, renderTabsBar: noop, setFileTitle: noop, addRecent: noop, pushHistory: noop, saveLastFile: noop,
    clearAiOutput: noop, afterRender: noop, renderPage: noop, $: () => ({ disabled: false, scrollTop: 0 }),
    apiMessage: () => '', requestAnimationFrame: noop, console,
    documentLoadEpoch: 0,
    beginDocumentLoad() { return ++context.documentLoadEpoch; },
    isDocumentLoadCurrent(epoch) { return epoch === context.documentLoadEpoch; },
    async apiFetch(url) { calls++; if (firstResponse && calls === 1) await firstResponse.promise;
      const name = new URL(url, 'http://localhost').searchParams.get('p') || doc.path;
      return { ok: true, async json() { return { ...doc, path: name, name }; } }; },
  };
  vm.createContext(context); vm.runInContext(fileCode + '\n' + virtualCode, context);
  return { context, state, calls: () => calls, block() { busy = true; idle = deferred(); },
    release() { busy = false; idle.resolve(); }, delayFirstResponse() { firstResponse = deferred(); return firstResponse; } };
}

test('an OS file open waits for the session close and then actually opens', async () => {
  const f = fixture(); f.block(); const work = f.context.loadFile('synthetic.md');
  await Promise.resolve(); assert.equal(f.calls(), 0); assert.equal(f.state.tabs.length, 0);
  f.release(); await work;
  assert.equal(f.calls(), 1); assert.equal(f.state.tabs.length, 1); assert.equal(f.state.tabs[0].path, 'synthetic.md');
});

test('a delayed open response is retried after a close instead of mutating its snapshot', async () => {
  const f = fixture(), response = f.delayFirstResponse(); const work = f.context.loadFile('synthetic.md');
  await Promise.resolve(); assert.equal(f.calls(), 1);
  f.block(); response.resolve(); await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.state.tabs.length, 0); assert.equal(f.calls(), 1);
  f.release(); await work;
  assert.equal(f.calls(), 2); assert.equal(f.state.tabs.length, 1); assert.equal(f.state.tabs[0].path, 'synthetic.md');
});

test('a completed conversion waits until close and then adds its virtual document', async () => {
  const f = fixture(); f.block(); const work = f.context.renderVirtual('convert', 'synthetic.pdf', '', 'Converted content', [], {});
  await Promise.resolve(); assert.equal(f.state.tabs.length, 0);
  f.release(); await work;
  assert.equal(f.state.tabs.length, 1); assert.equal(f.state.tabs[0].content, 'Converted content');
  assert.equal(f.state.tabs[0].name, 'synthetic.md');
});

test('multiple OS opens queued during a close are all delivered in order', async () => {
  const f = fixture(); f.block();
  const first = f.context.loadFile('first.md'), second = f.context.loadFile('second.md');
  await Promise.resolve(); assert.equal(f.state.tabs.length, 0);
  f.release(); await Promise.all([first, second]);
  assert.deepEqual(f.state.tabs.map(tab => tab.path), ['first.md', 'second.md']);
});

for (const order of ['file-first', 'import-first']) {
  test('queued file open and import keep their ' + order + ' order', async () => {
    const f = fixture(); f.block();
    const file = () => f.context.loadFile('after.md');
    const imported = () => f.context.renderVirtual('convert', 'import.pdf', '', 'Converted content', [], {});
    const tasks = order === 'file-first' ? [file(), imported()] : [imported(), file()];
    f.release(); await Promise.all(tasks);
    assert.deepEqual(f.state.tabs.map(tab => tab.name), order === 'file-first' ? ['after.md', 'import.md'] : ['import.md', 'after.md']);
  });
}

test('a new open joins the queue while an earlier deferred response is loading', async () => {
  const f = fixture(), response = f.delayFirstResponse(); f.block();
  const first = f.context.loadFile('first.md'), second = f.context.loadFile('second.md');
  f.release(); await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.calls(), 1);
  const third = f.context.loadFile('third.md'); await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.calls(), 1, 'The later open must not cancel an earlier queued load');
  response.resolve(); await Promise.all([first, second, third]);
  assert.deepEqual(f.state.tabs.map(tab => tab.path), ['first.md', 'second.md', 'third.md']);
});

test('a new close starting immediately after another keeps the pending open waiting', async () => {
  const f = fixture(); f.block(); const work = f.context.loadFile('first.md');
  await Promise.resolve(); f.release(); f.block(); await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.calls(), 0); assert.equal(f.state.tabs.length, 0);
  f.release(); await work;
  assert.equal(f.calls(), 1); assert.equal(f.state.tabs[0].path, 'first.md');
});

test('a queued open of an externally changed inactive tab cannot wait behind itself', async () => {
  const f = fixture();
  const existing = { id: 'existing', path: 'first.md', content: 'Old synthetic content',
    original: 'Old synthetic content', externalChanged: true, isDirty: false };
  f.state.tabs.push({ id: 'active', path: 'active.md', content: 'Synthetic', original: 'Synthetic', isDirty: false }, existing);
  f.state.activeTabId = 'active';
  f.context.getActiveTab = () => f.state.tabs.find(tab => tab.id === f.state.activeTabId);
  f.context.captureReaderState = () => {};
  f.context.renderActiveTab = async () => {};
  f.context.window.invalidateDocumentLoads = () => ++f.context.documentLoadEpoch;
  // Use the production tab switch: its external-change refresh nests loadFile.
  vm.runInContext(switchCode, f.context);
  f.block();
  const first = f.context.loadFile('first.md'), following = f.context.loadFile('following.md');
  let completed = false;
  const work = Promise.all([first, following]).then(() => { completed = true; });
  f.release();
  // Every mocked I/O resolves immediately. A self-dependent promise queue stays
  // stuck across event-loop turns without requiring a wall-clock timeout.
  await new Promise(resolve => setImmediate(resolve));
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(completed, true, 'The nested external-change refresh must not enqueue behind its waiting parent');
  await work;
  assert.equal(f.state.tabs.find(tab => tab.id === existing.id), existing);
  assert.equal(existing.content, '# Synthetic\n');
  assert.equal(existing.externalChanged, false);
  assert.deepEqual(f.state.tabs.map(tab => tab.path), ['active.md', 'first.md', 'following.md']);
  assert.equal(f.state.tabs.find(tab => tab.id === f.state.activeTabId)?.path, 'following.md');
});
