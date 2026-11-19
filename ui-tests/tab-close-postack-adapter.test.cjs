'use strict';
// Deterministic lifecycle ordering with the production close functions. The
// native runner separately proves actual WKWebView hit targets and durable CAS.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const tabs = fs.readFileSync(path.join(__dirname, '../assets/js/core/tabs.js'), 'utf8');
const lifecycle = fs.readFileSync(path.join(__dirname, '../assets/js/core/lifecycle.js'), 'utf8');
const shell = fs.readFileSync(path.join(__dirname, '../assets/js/shell/shell.js'), 'utf8');
const dragdrop = fs.readFileSync(path.join(__dirname, '../assets/js/core/dragdrop.js'), 'utf8');
const turn = () => new Promise(resolve => setImmediate(resolve));
function deferred() { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; }

function fixture({ count = 2, reviewGate } = {}) {
  const pages = Array.from({ length: count }, (_, index) => ({ id: `owned-${index}`, path: `/synthetic/${index}.md`, name: `${index}.md`, content: `Original ${index}`, original: `Original ${index}`, isDirty: false }));
  const state = { tabs: [...pages], activeTabId: pages.at(-1).id, editing: false };
  const rendered = deferred(), releaseRender = deferred(), closes = [], prompts = [];
  let held = false, reviewWaited = false, choice = 'cancel';
  const window = {
    i18n: { t: key => key },
    ReadMDSession: { restoring: () => false, busy: () => false, whenIdle: async () => {}, retryRecovery: async () => true,
      closeTabs: async (targets, remove) => { closes.push(targets.map(tab => tab.id)); remove(); return true; } },
    ReadMDReview: { onEditorUpdate() {}, snapshot: () => null, lf: value => value, flush: async () => {
      if (reviewGate && !reviewWaited) { reviewWaited = true; await reviewGate.promise; }
      return true;
    } },
    ReadMDRecovery: { prepareClose: async () => true, prepareInstall: async () => true },
  };
  const context = vm.createContext({ window, state, document: { activeElement: null }, Element: class {},
    getActiveTab: () => state.tabs.find(tab => tab.id === state.activeTabId), getEditContent: () => context.getActiveTab()?.content,
    promptDirtyClose: async name => { prompts.push(name); return choice; },
    saveTabBeforeClose: async () => assert.fail('No test authorizes a save'),
    exitEdit() { state.editing = false; }, resetHome() { state.activeTabId = null; }, syncStateFromActiveTab() {}, syncActiveTabDirty() {},
    renderTabsBar() {}, focusVisibleTab() {}, showToast() {},
    renderActiveTab: async () => { if (!held && state.tabs.length) { held = true; rendered.resolve(); await releaseRender.promise; } },
    normalizePath: value => value, confirmExitEdit: async () => true, closeOtherTabs: async () => true, closeAllTabs: async () => true,
    switchTab: async () => true, goHome: async () => true, newDocument: async () => {}, saveEdit: async () => true, commitDocumentSave: async () => true,
    toggleEdit: async () => { state.editing = true; }, renderVirtual: async () => {},
  });
  vm.runInContext(tabs.slice(tabs.indexOf('function requestTabClose('), tabs.indexOf('\nasync function closeOtherTabs(')), context);
  vm.runInContext(lifecycle, context); window.ReadMDLifecycle.install();
  return { context, window, state, pages, rendered, releaseRender, closes, prompts, set choice(value) { choice = value; } };
}

test('a next-tab close clicked after the ACK survives the previous render lifecycle', async () => {
  const f = fixture(), first = f.context.requestTabClose(f.pages[1].id);
  await f.rendered.promise;
  assert.equal(f.state.tabs.length, 1);
  assert.equal(f.window.ReadMDSession.busy(), false);
  assert.equal(f.window.ReadMDLifecycle.owner().tabCloseCommitted, true);
  const next = f.context.requestTabClose(f.pages[0].id);
  await turn();
  assert.equal(f.closes.length, 1, 'The new close must not overlap the old renderer');
  f.releaseRender.resolve(); await first; await next;
  assert.equal(f.state.tabs.length, 0);
  assert.equal(f.window.ReadMDLifecycle.busy(), false);
  assert.equal(f.closes.length, 2);
});

test('repeated clicks on the same queued page share one close operation', async () => {
  const f = fixture(), first = f.context.requestTabClose(f.pages[1].id);
  await f.rendered.promise;
  const next = f.context.requestTabClose(f.pages[0].id);
  assert.equal(f.context.requestTabClose(f.pages[0].id), next);
  f.releaseRender.resolve(); await Promise.all([first, next]);
  assert.equal(f.closes.length, 2);
});

test('a queued close retains its exact page and cannot close a replacement active page', async () => {
  const f = fixture(), first = f.context.requestTabClose(f.pages[1].id);
  await f.rendered.promise;
  const next = f.context.requestTabClose(f.pages[0].id);
  const replacement = { ...f.pages[0], id: 'owned-replacement' };
  f.state.tabs = [replacement]; f.state.activeTabId = replacement.id;
  f.releaseRender.resolve(); await first;
  assert.equal(await next, false);
  assert.equal(f.state.tabs[0], replacement);
  assert.equal(f.closes.length, 1);
});

test('edits made before a queued close still receive the current dirty-page decision', async () => {
  const f = fixture(), first = f.context.requestTabClose(f.pages[1].id);
  await f.rendered.promise;
  const next = f.context.requestTabClose(f.pages[0].id);
  f.pages[0].content = 'New unsaved input'; f.pages[0].isDirty = true;
  f.releaseRender.resolve(); await Promise.all([first, next]);
  assert.equal(f.prompts.length, 1);
  assert.equal(f.state.tabs[0].content, 'New unsaved input');
  assert.equal(f.closes.length, 1, 'Cancel preserves the latest dirty page');
});

test('an unacknowledged close does not queue a second confirmation or later close', async () => {
  const reviewGate = deferred(), f = fixture({ reviewGate });
  const first = f.context.requestTabClose(f.pages[1].id);
  assert.equal(await f.context.requestTabClose(f.pages[0].id), false);
  reviewGate.resolve(); await f.rendered.promise; f.releaseRender.resolve(); await first;
  assert.equal(f.closes.length, 1);
  assert.equal(f.state.tabs[0], f.pages[0]);
});

test('distinct requests queued behind an acknowledged close execute sequentially', async () => {
  const f = fixture({ count: 3 }), first = f.context.requestTabClose(f.pages[2].id);
  await f.rendered.promise;
  const next = f.context.requestTabClose(f.pages[0].id), last = f.context.requestTabClose(f.pages[1].id);
  f.releaseRender.resolve(); await Promise.all([first, next, last]);
  assert.equal(f.state.tabs.length, 0);
  assert.deepEqual(f.closes.map(ids => [...ids]), [[f.pages[2].id], [f.pages[0].id], [f.pages[1].id]]);
});

test('programmatic close retains the existing refusal while another lifecycle owns the page', async () => {
  const f = fixture(), first = f.context.requestTabClose(f.pages[1].id);
  await f.rendered.promise;
  assert.equal(await f.context.closeTab(f.pages[0].id), false);
  f.releaseRender.resolve(); await first;
  assert.equal(f.state.tabs[0], f.pages[0]);
  assert.equal(f.closes.length, 1);
});

test('the actual native menu close keeps its requested page through the post-ACK queue', async () => {
  const f = fixture(), first = f.context.requestTabClose(f.pages[1].id);
  await f.rendered.promise;
  vm.runInContext(shell.slice(shell.indexOf('let nativeMenuQueue ='), shell.indexOf('\nfunction initWindowChrome(')), f.context);
  const command = f.window.__readmdNativeMenu('close-document');
  await turn();
  const queued = f.context.requestTabClose.pending.get(f.pages[0]);
  assert(queued, 'The actual command must enter the same exact-page queue');
  const replacement = { ...f.pages[0], id: 'owned-newly-active' };
  f.state.tabs.push(replacement); f.state.activeTabId = replacement.id;
  f.releaseRender.resolve(); await Promise.all([first, command]);
  assert.equal(f.state.tabs[0], replacement);
  assert.equal(f.closes.at(-1)[0], f.pages[0].id);
});

test('the actual tab menu captures its target before closing the menu or waiting', async () => {
  const f = fixture(), first = f.context.requestTabClose(f.pages[1].id);
  await f.rendered.promise;
  let click;
  const button = { dataset: { action: 'close' }, addEventListener(event, callback) { if (event === 'click') click = callback; } };
  const menu = { dataset: { tabId: f.pages[0].id }, addEventListener() {}, querySelectorAll: () => [button] };
  f.context.$ = () => menu;
  f.context.document.addEventListener = () => {};
  f.context.closeTabContextMenu = () => { menu.dataset.tabId = 'another-menu-target'; };
  vm.runInContext(dragdrop.slice(dragdrop.indexOf('function bindTabContextMenuEvents(')), f.context);
  f.context.bindTabContextMenuEvents(); click();
  const queued = f.context.requestTabClose.pending.get(f.pages[0]);
  assert(queued, 'The menu callback must queue the target captured before dismissing it');
  f.releaseRender.resolve(); await Promise.all([first, queued]);
  assert.equal(f.state.tabs.length, 0);
  assert.equal(f.closes.at(-1)[0], f.pages[0].id);
});
