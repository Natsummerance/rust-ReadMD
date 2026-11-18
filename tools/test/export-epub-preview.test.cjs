'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');

const source = fs.readFileSync(path.join(__dirname, '../../assets/js/features/export.js'), 'utf8');
const book = { ok: true, format: 'epub', html: '<article id="chapter">The EPUB preview</article>' };

class Element {
  constructor(className = '') {
    this.className = className;
    this.children = [];
    this.parentElement = null;
    this.attributes = {};
    this.dataset = {};
    this.listeners = {};
    this.classList = {
      add: (...names) => { this.className = [this.className, ...names].filter(Boolean).join(' '); },
      contains: name => this.className.split(' ').includes(name),
      remove: (...names) => { this.className = this.className.split(' ').filter(name => !names.includes(name)).join(' '); },
    };
  }

  append(...nodes) {
    for (const node of nodes) {
      this.children.push(node);
      node.parentElement = this;
    }
  }

  replaceChildren(...nodes) {
    for (const child of [...this.children]) child.remove();
    this.append(...nodes);
  }

  remove() {
    if (!this.parentElement) return;
    this.parentElement.children = this.parentElement.children.filter(child => child !== this);
    this.parentElement = null;
  }

  setAttribute(key, value) { this.attributes[key] = value; }
  addEventListener(name, listener) { this.listeners[name] = listener; }

  querySelectorAll(selector) {
    const classes = selector.split(',').map(part => part.trim())
      .filter(part => part.startsWith('.')).map(part => part.slice(1));
    const matches = child => classes.some(name => child.classList.contains(name));
    return this.children.flatMap(child => [
      ...(matches(child) ? [child] : []), ...child.querySelectorAll(selector),
    ]);
  }

  querySelector(selector) { return this.querySelectorAll(selector)[0] || null; }
}

function environment() {
  const mini = new Element(), modal = new Element('hidden');
  const wrapper = new Element('export-preview-paper-wrapper');
  modal.append(wrapper);
  const root = new Element();
  root.append(mini, modal);
  const ids = { 'export-preview-mini-content': mini, 'export-preview-modal': modal };
  const timers = new Map(), calls = [];
  let serial = 0;
  let previewTitle = 'Actual preview';
  const context = vm.createContext({
    $: id => ids[id] || null,
    _t: key => key === 'export.actualPreview' ? previewTitle : key,
    AbortController, TextEncoder,
    crypto: { randomUUID: () => String(++serial) },
    document: {
      querySelectorAll: selector => root.querySelectorAll(selector),
      createElement: tag => {
        const node = new Element();
        if (tag === 'iframe') node.contentDocument = { URL: 'about:blank' };
        return node;
      },
    },
    window: {
      addEventListener() {},
      READMD_ENGINE: 'rust',
      ReadMDReview: {
        capture: async () => ({ document_id: 'book', generation: 1, review_revision: 1, content: 'Book' }),
        lf: value => value,
      },
      ReadMDTask: {
        loading: host => { host.replaceChildren(); host.setAttribute('aria-busy', 'true'); },
      },
    },
    state: { dir: '' },
    getActiveTab: () => ({ id: 'book-tab' }),
    setTimeout: callback => { timers.set(++serial, callback); return serial; },
    clearTimeout: id => timers.delete(id),
    apiFetch: async (route, request) => {
      const body = JSON.parse(request.body);
      calls.push({ route, body });
      if (route === '/api/export/snapshot') {
        assert.equal(body.action, 'prepare');
        return { ok: true, json: async () => ({
          ok: true, snapshot_id: body.format, pending_count: 0, pending_ids: [],
          retained_bytes: 42, runtime_ready: true,
        }) };
      }
      assert.equal(route, '/api/export/preview');
      return { ok: true, json: async () => body.snapshot_id === 'epub'
        ? book : { ok: true, format: 'tex', mode: 'source', source: 'TeX body' } };
    },
  });
  vm.runInContext(source, context);
  const request = (format = 'epub') => context.requestNativeExportPreview(format, {}, 'Book', 'Book', 'Default');
  return {
    calls, request, mini, wrapper,
    showFull: () => { modal.classList.remove('hidden'); request(); },
    hideFull: () => modal.classList.add('hidden'),
    setTitle: value => { previewTitle = value; },
    async finishRequests() {
      for (const [id, callback] of timers) { timers.delete(id); await callback(); }
    },
  };
}

function finishLoading(frame) {
  frame.contentDocument = { URL: 'about:srcdoc', addEventListener() {} };
  frame.listeners.load();
}

test('opening and reopening an unchanged EPUB preserves its existing frames', async () => {
  const e = environment();
  e.request();
  await e.finishRequests();
  const miniFrame = e.mini.querySelector('.export-epub-frame');
  assert.equal(miniFrame.srcdoc, book.html);
  assert.equal(e.wrapper.querySelector('.export-epub-frame'), null);

  e.showFull();
  const fullFrame = e.wrapper.querySelector('.export-epub-frame');
  assert.equal(e.mini.querySelector('.export-epub-frame'), miniFrame, 'opening full preview keeps a pending miniature');
  assert.equal(fullFrame.srcdoc, book.html);
  assert.equal(fullFrame.attributes.sandbox, 'allow-same-origin');
  finishLoading(miniFrame);
  finishLoading(fullFrame);

  e.hideFull();
  e.setTitle('Aperçu');
  e.showFull();
  assert.equal(e.mini.querySelector('.export-epub-frame'), miniFrame);
  assert.equal(e.wrapper.querySelector('.export-epub-frame'), fullFrame);
  assert.equal(fullFrame.title, 'Aperçu');
  assert.equal(e.calls.length, 2, 'fullscreen changes do not prepare or render another snapshot');
});

test('switching formats removes old EPUB frames even when its snapshot result is reusable', async () => {
  const e = environment();
  e.showFull();
  await e.finishRequests();
  const firstMini = e.mini.querySelector('.export-epub-frame');
  const firstFull = e.wrapper.querySelector('.export-epub-frame');
  e.hideFull();
  e.request('tex');
  assert.equal(firstMini.parentElement, null);
  assert.equal(firstFull.parentElement, null);
  await e.finishRequests();

  e.showFull();
  await e.finishRequests();
  assert.notEqual(e.mini.querySelector('.export-epub-frame'), firstMini);
  assert.notEqual(e.wrapper.querySelector('.export-epub-frame'), firstFull);
  assert.equal(e.wrapper.querySelector('.export-epub-frame').srcdoc, book.html);
  assert.equal(e.calls.filter(call => call.route === '/api/export/preview' && call.body.snapshot_id === 'epub').length, 1);
});

test('a hidden EPUB frame that navigated away is restored on reopening', async () => {
  const e = environment();
  e.showFull();
  await e.finishRequests();
  const miniFrame = e.mini.querySelector('.export-epub-frame');
  const oldFull = e.wrapper.querySelector('.export-epub-frame');
  finishLoading(oldFull);
  oldFull.contentDocument = { URL: 'about:blank' };

  e.hideFull();
  e.showFull();
  assert.equal(e.mini.querySelector('.export-epub-frame'), miniFrame);
  assert.notEqual(e.wrapper.querySelector('.export-epub-frame'), oldFull);
  assert.equal(e.wrapper.querySelector('.export-epub-frame').srcdoc, book.html);
});
