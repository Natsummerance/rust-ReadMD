'use strict';
// Controlled request scheduling only: no HTTP server, managed runtime or auth substitute.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const root = path.resolve(__dirname, '../..');
const pdf = fs.readFileSync(path.join(root, 'assets/js/features/pdf.js'), 'utf8');
const core = fs.readFileSync(path.join(root, 'assets/js/core/capabilities.js'), 'utf8');
const convert = fs.readFileSync(path.join(root, 'assets/js/features/convert.js'), 'utf8');
const deferred = () => { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; };
const installed = { ok: true, id: 'node', ready: true, installed: true, present: true, busy: false };
const absent = { ...installed, ready: false, installed: false, present: false };

class Element {
  constructor(tag) {
    this.tagName = tag.toUpperCase(); this.children = []; this.parentNode = null;
    this.dataset = {}; this.attributes = new Map(); this.disabled = false; this._text = '';
    const names = new Set();
    this.classList = { add: name => names.add(name), remove: name => names.delete(name), contains: name => names.has(name), toggle: (name, on) => on ? names.add(name) : names.delete(name) };
  }
  get isConnected() { return this.tagName === 'BODY' || Boolean(this.parentNode?.isConnected); }
  get textContent() { return this._text + this.children.map(child => child.textContent).join(''); }
  set textContent(text) { this.replaceChildren(); this._text = String(text); }
  append(...children) { for (const child of children) { child.parentNode = this; this.children.push(child); } }
  replaceChildren(...children) { for (const child of this.children) child.parentNode = null; this.children = []; this._text = ''; this.append(...children); }
  remove() { if (this.parentNode) this.parentNode.children = this.parentNode.children.filter(child => child !== this); this.parentNode = null; }
  setAttribute(name, value) { this.attributes.set(name, String(value)); }
  getAttribute(name) { return this.attributes.get(name) ?? null; }
  removeAttribute(name) { this.attributes.delete(name); }
  addEventListener() {}
  click() {}
  querySelectorAll(selector) {
    const matches = child => child.tagName === 'BUTTON' && (selector === 'button' || selector === 'button[data-capability-action]' && child.dataset.capabilityAction !== undefined);
    return this.children.flatMap(child => [...(matches(child) ? [child] : []), ...child.querySelectorAll(selector)]);
  }
}

function harness({ status = async () => installed, confirm = async () => true, terminal } = {}) {
  const body = new Element('body'), row = new Element('div'), offline = new Element('button');
  row.id = 'plugin-capability-pack'; offline.id = 'plugin-offline-pack'; body.append(row, offline);
  const find = (node, id) => node.id === id ? node : node.children.map(child => find(child, id)).find(Boolean);
  const requests = [], toasts = [], events = [], created = []; let confirmationCount = 0;
  const window = { i18n: { currentLang: 'en', t: key => 'T:' + key }, addEventListener() {}, dispatchEvent: event => events.push(event.type) };
  const context = vm.createContext({
    window, document: { body, getElementById: id => find(body, id), createElement: tag => { const element = new Element(tag); created.push(element); return element; } },
    addEventListener() {}, Intl, setTimeout, clearTimeout, state: { modules: {} }, updateModuleUi() {}, hasPy: false,
    showToast: text => toasts.push(text), confirmAction: () => { confirmationCount++; return confirm(); },
    CustomEvent: class { constructor(type, options) { this.type = type; this.detail = options?.detail; } },
    apiFetch: async (_url, options) => {
      const request = JSON.parse(options.body); requests.push(request);
      let data;
      if (request.action === 'catalog') data = { ok: true, packs: [{ id: 'node', available: true, name: 'Node.js', descriptionKey: 'node.about', downloadBytes: 1, installedBytes: 1 }] };
      else if (request.action === 'status') data = await status();
      else if (request.action === 'uninstall_start') data = { ok: true, job: 'owned-vm-uninstall', phase: 'removing', files_done: 0, files_total: 2 };
      else if (request.action === 'download_status') { assert(terminal, 'Explicit terminal fixture required'); data = await terminal.promise; }
      else throw Error('Unexpected request: ' + request.action);
      return { ok: true, status: 200, json: async () => data };
    },
  });
  vm.runInContext(core, context, { filename: 'assets/js/core/capabilities.js' });
  vm.runInContext(pdf, context, { filename: 'assets/js/features/pdf.js' });
  return { context, window, row, requests, toasts, events, created, find: id => find(body, id), get confirmationCount() { return confirmationCount; } };
}

async function until(check) {
  const end = Date.now() + 2000;
  while (!check()) { assert(Date.now() < end, 'Controlled request did not reach expected stage'); await new Promise(setImmediate); }
}
const count = (fixture, action) => fixture.requests.filter(request => request.action === action).length;

test('concurrent capability refresh shares the exact Promise and sends one request sequence', async () => {
  const status = deferred(), h = harness({ status: () => status.promise });
  const first = h.window.refreshOfflineCapabilityPack(), second = h.window.refreshOfflineCapabilityPack();
  status.resolve(installed); await Promise.all([first, second]);
  assert.equal(first, second);
  assert.equal(count(h, 'catalog'), 1); assert.equal(count(h, 'status'), 1);
  assert.equal(h.row.getAttribute('aria-busy'), 'false');
});

test('backend busy is visible and disables uninstall without treating installed as actionable', async () => {
  const h = harness({ status: async () => ({ ...installed, ready: false, busy: true, busy_code: 'capability_install_busy' }) });
  await h.window.refreshOfflineCapabilityPack();
  const remove = h.find('plugin-capability-uninstall-node'); assert(remove);
  assert.equal(remove.disabled, true); assert.match(h.row.textContent, /T:error\.capability_install_busy/);
  await remove.onclick(); assert.equal(h.confirmationCount, 0); assert.equal(count(h, 'uninstall_start'), 0);
});

test('refresh disables existing actions immediately and a stale callback cannot start uninstall', async () => {
  let next = null; const h = harness({ status: () => next ? next.promise : Promise.resolve(installed) });
  await h.window.refreshOfflineCapabilityPack(); const remove = h.find('plugin-capability-uninstall-node');
  next = deferred(); const refreshing = h.window.refreshOfflineCapabilityPack();
  assert.equal(remove.disabled, true); assert.equal(h.row.getAttribute('aria-busy'), 'true');
  // Programmatic invocation deliberately bypasses browser disabled-button behavior.
  await remove.onclick(); assert.equal(h.confirmationCount, 0); assert.equal(count(h, 'uninstall_start'), 0);
  next.resolve(installed); await refreshing;
});

test('confirmation and uninstall defer shared refresh until the production core job terminal receipt', async () => {
  const confirmation = deferred(), terminal = deferred(); let current = installed;
  const h = harness({ status: async () => current, confirm: () => confirmation.promise, terminal });
  await h.window.refreshOfflineCapabilityPack(); const remove = h.find('plugin-capability-uninstall-node');
  const removing = remove.onclick(), duplicate = remove.onclick();
  const first = h.window.refreshOfflineCapabilityPack(), second = h.window.refreshOfflineCapabilityPack();
  let refreshed = false; first.then(() => { refreshed = true; });
  assert.equal(h.confirmationCount, 1); assert.equal(first, second); await duplicate;
  assert.equal(count(h, 'catalog'), 1); assert.equal(count(h, 'status'), 1); assert.equal(count(h, 'uninstall_start'), 0);
  confirmation.resolve(true); await until(() => count(h, 'download_status') === 1);
  assert.equal(refreshed, false); assert.equal(count(h, 'uninstall_start'), 1);
  assert.equal(count(h, 'status'), 1); assert.equal(remove.isConnected, true);
  current = absent; terminal.resolve({ ok: true, job: 'owned-vm-uninstall', phase: 'removed', result: { installed: false, cleanup_pending: false } });
  await Promise.all([removing, first, second]);
  assert.equal(refreshed, true); assert.equal(count(h, 'catalog'), 2); assert.equal(count(h, 'status'), 2);
  assert.equal(h.find('plugin-capability-uninstall-node'), undefined); assert(h.find('plugin-capability-install-node'));
  assert.equal(h.row.getAttribute('aria-busy'), 'false'); assert.deepEqual(h.events, ['readmd:capability-removed']);
  await remove.onclick(); assert.equal(h.confirmationCount, 1); assert.equal(count(h, 'uninstall_start'), 1);
});

test('failed removal refreshes only after its terminal error and never replays the mutation or shows success', async () => {
  const terminal = deferred(), h = harness({ terminal });
  await h.window.refreshOfflineCapabilityPack(); const removing = h.find('plugin-capability-uninstall-node').onclick();
  await until(() => count(h, 'download_status') === 1);
  const refreshing = h.window.refreshOfflineCapabilityPack(); assert.equal(count(h, 'status'), 1);
  terminal.resolve({ ok: true, job: 'owned-vm-uninstall', phase: 'error', error_code: 'capability_install_failed' });
  await Promise.all([removing, refreshing]);
  assert.equal(count(h, 'uninstall_start'), 1); assert.equal(count(h, 'status'), 2);
  assert.deepEqual(h.events, []); assert(h.find('plugin-capability-uninstall-node'));
  assert(h.toasts.some(text => text === 'T:error.capability_install_failed')); assert(h.toasts.every(text => !text.includes('uninstallSuccess')));
});

test('cancelling confirmation releases a queued refresh without sending uninstall_start', async () => {
  const confirmation = deferred(), h = harness({ confirm: () => confirmation.promise });
  await h.window.refreshOfflineCapabilityPack(); const removing = h.find('plugin-capability-uninstall-node').onclick();
  const refreshing = h.window.refreshOfflineCapabilityPack();
  confirmation.resolve(false); await Promise.all([removing, refreshing]);
  assert.equal(count(h, 'uninstall_start'), 0); assert.equal(count(h, 'status'), 2);
  assert.equal(h.row.getAttribute('aria-busy'), 'false'); assert.deepEqual(h.toasts, []);
});

test('an installer mutation also holds queued refresh and cannot be started twice', async () => {
  let current = absent; const install = deferred(), h = harness({ status: async () => current }); let starts = 0;
  h.window.ReadMDCapabilities.ensure = async () => { starts++; await install.promise; return true; };
  await h.window.refreshOfflineCapabilityPack(); const button = h.find('plugin-capability-install-node');
  const installing = button.onclick(), duplicate = button.onclick(), refreshing = h.window.refreshOfflineCapabilityPack();
  assert.equal(starts, 1); assert.equal(count(h, 'status'), 1); await duplicate;
  current = installed; install.resolve(); await Promise.all([installing, refreshing]);
  assert.equal(count(h, 'status'), 2); assert.equal(h.row.getAttribute('aria-busy'), 'false');
  assert(h.find('plugin-capability-uninstall-node'));
});

test('file selection waits for an existing refresh to settle and its obsolete result cannot repaint the operation', async () => {
  let next = null, uploadStarted = false;
  const upload = deferred(), h = harness({ status: () => next ? next.promise : Promise.resolve(installed) });
  h.window.ReadMDCapabilities.upload = async () => { uploadStarted = true; await upload.promise; return { id: 'node' }; };
  await h.window.refreshOfflineCapabilityPack(); const old = h.find('plugin-capability-uninstall-node');
  await h.window.installOfflineCapabilityPack(); const input = h.created.find(element => element.tagName === 'INPUT'); assert(input);
  next = deferred(); const refreshingBeforeSelection = h.window.refreshOfflineCapabilityPack();
  input.files = [{}]; const installing = input.onchange(), after = h.window.refreshOfflineCapabilityPack();
  assert.equal(uploadStarted, false); assert.equal(count(h, 'status'), 1);
  next.resolve(installed); await refreshingBeforeSelection; await until(() => uploadStarted);
  assert.equal(old.isConnected, true, 'The pre-operation response must not repaint the operation');
  assert.equal(h.row.getAttribute('aria-busy'), 'true'); assert.equal(old.disabled, true);
  next = null; upload.resolve(); await Promise.all([installing, after]);
  assert.equal(count(h, 'status'), 3); assert.equal(old.isConnected, false);
  assert.equal(h.row.getAttribute('aria-busy'), 'false');
});

test('opening the plugin modal awaits capability refresh as well as the normal plugin list', async () => {
  const capability = deferred(), plugins = deferred(), preferences = deferred(); let opened = false, settled = false, probes = 0;
  const context = vm.createContext({
    window: { __readmdPreferencesReady: preferences.promise, refreshOfflineCapabilityPack: () => { probes++; return capability.promise; } },
    $: () => ({ classList: { remove: () => { opened = true; } } }), _t: key => key,
    setPluginListStatus() {}, refreshPluginList: () => plugins.promise,
  });
  const begin = convert.indexOf('async function openPluginModal()'), end = convert.indexOf('function closePluginModal()', begin);
  assert(begin >= 0 && end > begin); vm.runInContext(convert.slice(begin, end), context);
  const opening = context.openPluginModal(); opening.then(() => { settled = true; });
  await Promise.resolve(); assert.equal(opened, false);
  preferences.resolve(); await until(() => probes === 1); plugins.resolve();
  for (let turn = 0; turn < 10; turn++) await Promise.resolve();
  const settledBeforeCapability = settled; capability.resolve(); await opening;
  assert.equal(settledBeforeCapability, false); assert.equal(opened, true);
});
