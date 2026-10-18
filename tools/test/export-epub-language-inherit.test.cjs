'use strict';
// Real export functions with a small select/transport host. This does not
// certify browser layout, native UI, source parsing or generated EPUB bytes.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const crypto = require('node:crypto');
const root = path.resolve(process.env.READMD_QA_REPO_ROOT || path.join(__dirname, '../..'));
const draft = process.env.READMD_EPUB_LANGUAGE_DRAFT;
const load = name => fs.readFileSync(path.join(name === 'assets/js/features/export.js' && draft ? draft : root, name), 'utf8');
const plain = value => JSON.parse(JSON.stringify(value));
const source = '---\ntitle: Synthetic language check\nlanguage: en-US\n---\n# Opening\n\nSource stays unchanged.\n';

class Option {
  constructor(text, value = text) { this.text = String(text); this.value = String(value); }
}
class Element {
  constructor(tag = 'div') {
    this.tagName = tag.toUpperCase(); this.dataset = {}; this.attributes = {}; this.children = [];
    this.className = ''; this.textContent = ''; this._html = ''; this.value = ''; this.isConnected = true;
    this.classList = {
      add: name => { if (!this.classList.contains(name)) this.className += ' ' + name; },
      remove: name => { this.className = this.className.split(/\s+/).filter(part => part !== name).join(' '); },
      contains: name => this.className.split(/\s+/).includes(name)
    };
  }
  setAttribute(name, value) {
    this.attributes[name] = String(value);
    if (name.startsWith('data-')) this.dataset[name.slice(5).replace(/-([a-z])/g, (_, c) => c.toUpperCase())] = String(value);
  }
  appendChild(child) { this.children.push(child); return child; }
  addEventListener() {}
  get innerHTML() { return this._html; }
  set innerHTML(html) {
    this._html = html; this.children = [];
    const select = /<select\b[^>]*data-k="([^"]+)"[^>]*>([\s\S]*?)<\/select>/.exec(html);
    if (select) {
      const control = new Select(select[1]);
      for (const match of select[2].matchAll(/<option value="([^"]*)">([\s\S]*?)<\/option>/g)) control.add(new Option(match[2], match[1]));
      this.children.push(control);
    }
  }
  querySelector(selector) { return selector === '[data-k]' ? this.children.find(child => child.dataset.k) || null : null; }
}
class Select extends Element {
  constructor(key) { super('select'); this.type = 'select-one'; this.dataset.k = key; this.options = []; this._value = ''; }
  add(option) { this.options.push(option); if (this.options.length === 1) this._value = option.value; }
  get value() { return this._value; }
  set value(value) { this._value = this.options?.some(option => option.value === String(value)) ? String(value) : ''; }
}
function environment({ options = {}, defaults = {}, bridge = true, locale = 'en' } = {}) {
  const ids = new Map(), controls = [], requests = [], bridgeCalls = [], saved = [], timers = [];
  for (const id of ['export-opts', 'export-result', 'export-warns', 'exp-preset', 'export-preview-modal']) ids.set(id, new Element());
  ids.get('exp-preset').value = 'minimal'; ids.get('export-preview-modal').className = 'hidden';
  const dictionary = JSON.parse(fs.readFileSync(path.join(root, 'assets/i18n', locale + '.json'), 'utf8'));
  const i18n = { t: key => dictionary[key] || key };
  const document = {
    createElement: tag => new Element(tag), getElementById: id => ids.get(id) || null,
    querySelectorAll: selector => selector === '#export-opts [data-k]' ? controls : [], querySelector: () => null
  };
  const context = {
    window: { i18n, addEventListener() {} }, i18n, document, Option, AbortController, crypto,
    state: { mode: 'file', editing: false, original: source, file: 'synthetic-language.md', dir: '', export: {
      fmt: 'epub', defaults: plain(defaults), options: plain(options), catalog: [], custom: {}, presets: {}
    } },
    $: id => ids.get(id) || null, hasPy: bridge,
    py: {
      export_epub: async (...args) => { bridgeCalls.push(plain(args)); return { ok: true, path: '' }; },
      save_export_presets: async payload => { saved.push(plain(payload)); return true; }
    },
    apiFetch: async (route, request) => {
      requests.push({ route, body: JSON.parse(request.body) });
      return { ok: true, json: async () => ({ ok: true, path: '', format: 'epub', html: '<html></html>' }) };
    },
    setTimeout: fn => { timers.push(fn); return timers.length; }, clearTimeout() {},
    busy() {}, showToast() {}, warnMessages: () => [], console
  };
  vm.createContext(context);
  vm.runInContext(load('assets/js/editor/publication-metadata.js'), context, { filename: 'publication-metadata.js' });
  vm.runInContext(load('assets/js/features/export.js'), context, { filename: 'export.js' });
  const field = () => context.getExportSections().flatMap(section => section.fields || []).find(item => item.k === 'epub.language');
  const control = context.expFieldEl(field()).querySelector('[data-k]'); controls.push(control);
  return { context, control, controls, field, ids, requests, bridgeCalls, saved,
    apply: () => context.applyExportOptionsToDom(), collect: () => plain(context.collectExportOptions()),
    async flushTimers() { while (timers.length) await timers.shift()(); }
  };
}
function lacksLanguage(value) { assert.equal(Object.hasOwn(value || {}, 'language'), false); }
function sourceUnchanged(e) { assert.equal(e.context.state.original, source); }
function successfulExport(e) { assert.equal(e.ids.get('export-result').className, 'export-result ok'); sourceUnchanged(e); }

test('all shipped interface languages label the initial empty choice with pm.inherit', () => {
  const locales = Object.keys(JSON.parse(fs.readFileSync(path.join(root, 'assets/i18n/meta.json'), 'utf8')));
  assert.equal(locales.length, 46);
  for (const locale of locales) {
    const e = environment({ locale }), dictionary = JSON.parse(fs.readFileSync(path.join(root, 'assets/i18n', locale + '.json'), 'utf8'));
    assert.equal(e.field().default, '');
    assert.equal(e.control.options[0].value, '');
    assert.equal(e.control.options[0].text, dictionary['pm.inherit']);
    assert(dictionary['pm.inherit']);
    assert.equal(e.control.dataset.default, JSON.stringify(''));
  }
});
test('opening old/default options collects no implicit EPUB language and preserves source/options', () => {
  for (const options of [{}, { epub: { fontSize: 12, splitLevel: 'h1' } }]) {
    const e = environment({ options }); e.apply();
    assert.equal(e.control.value, ''); lacksLanguage(e.collect().epub);
    assert.deepEqual(plain(e.context.state.export.options), options); sourceUnchanged(e);
  }
});
test('explicit English/Chinese and stored regional tags survive apply/collect without duplicate options', () => {
  for (const language of ['en', 'zh-CN', 'zh-TW', 'en-US']) {
    const e = environment({ options: { epub: { language } } }); e.apply(); e.apply();
    assert.equal(e.control.value, language); assert.equal(e.collect().epub.language, language);
    assert.equal(e.control.options.filter(option => option.value === language).length, 1); sourceUnchanged(e);
  }
});
test('switching an explicit preset to one missing language resets the same control to inherit', () => {
  const e = environment({ options: { epub: { language: 'zh-CN' } } }); e.apply();
  assert.equal(e.control.value, 'zh-CN');
  e.context.state.export.options = { epub: { fontSize: 14 } }; e.apply();
  assert.equal(e.control.value, ''); lacksLanguage(e.collect().epub); assert.equal(e.collect().epub.fontSize, 14);
});
test('choosing inherit removes a merged legacy override while retaining common metadata and style', () => {
  const e = environment({ defaults: { epub: { language: 'zh-CN', fontSize: 12 } }, options: {
    epub: { language: 'en', marginV: 3 }, meta: { language: 'en-US', title: 'Common title', clear: ['subject'] }
  } });
  e.apply(); e.control.value = ''; const result = e.collect();
  lacksLanguage(result.epub); assert.equal(result.epub.fontSize, 12); assert.equal(result.epub.marginV, 3);
  assert.deepEqual(result.meta, { language: 'en-US', title: 'Common title', clear: ['subject'] });
  assert.equal(e.context.state.export.options.epub.language, 'en');
});
test('a language control absent from another format leaves the stored EPUB override intact', () => {
  const e = environment({ options: { epub: { language: 'zh-CN' } } });
  e.controls.length = 0; e.context.state.export.fmt = 'pdf'; assert.equal(e.collect().epub.language, 'zh-CN');
});
test('the actual native-preview request leaves automatic language absent and source bytes intact', async () => {
  const e = environment({ options: { epub: { fontSize: 12 }, meta: { language: 'en-US' } } }); e.apply();
  e.context.requestNativeExportPreview('epub', e.context.collectExportOptions(), e.context.currentExportContent(), 'Synthetic', 'minimal');
  await e.flushTimers(); assert.equal(e.requests.length, 1);
  const request = e.requests[0]; assert.equal(request.route, '/api/export/preview'); assert.equal(request.body.content, source);
  lacksLanguage(request.body.options.epub); assert.deepEqual(request.body.options.meta, { language: 'en-US' }); sourceUnchanged(e);
});
test('the actual legacy bridge export preserves common metadata and never manufactures automatic language', async () => {
  const e = environment({ options: { epub: { fontSize: 12 }, meta: { language: 'en-US', clear: ['subject'] } } }); e.apply();
  await e.context.runExportOnce(); successfulExport(e); assert.equal(e.bridgeCalls.length, 1); assert.equal(e.requests.length, 0);
  const [content, output, legacy, options] = e.bridgeCalls[0];
  assert.equal(content, source); assert.equal(output, ''); lacksLanguage(legacy); lacksLanguage(options.epub);
  assert.equal(options.epub.fontSize, 12); assert.deepEqual(options.meta, { language: 'en-US', clear: ['subject'] });
  lacksLanguage(e.context.state.export.last.options.epub); assert.equal(e.saved.length, 1);
});
test('the actual bridge keeps explicitly chosen English/Chinese overrides and common metadata', async () => {
  for (const language of ['en', 'zh-CN']) {
    const e = environment({ options: { epub: { fontSize: 12 }, meta: { language: 'en-US' } } }); e.apply(); e.control.value = language;
    await e.context.runExportOnce(); successfulExport(e);
    assert.equal(e.bridgeCalls[0][2].language, language); assert.equal(e.bridgeCalls[0][3].epub.language, language);
    assert.equal(e.bridgeCalls[0][3].meta.language, 'en-US');
  }
});
test('the actual HTTP fallback sends inherit/explicit choices with the same legacy payload semantics', async () => {
  for (const language of ['', 'en', 'zh-CN']) {
    const e = environment({ bridge: false, options: { epub: { fontSize: 12 }, meta: { language: 'en-US' } } }); e.apply(); e.control.value = language;
    await e.context.runExportOnce(); successfulExport(e); assert.equal(e.requests.length, 1); assert.equal(e.bridgeCalls.length, 0);
    const { route, body } = e.requests[0]; assert.equal(route, '/api/export/epub'); assert.equal(body.confirm, true); assert.equal(body.content, source);
    assert.deepEqual(body.options.meta, { language: 'en-US' });
    for (const layer of [body.meta, body.epub, body.options.epub]) {
      if (language) assert.equal(layer.language, language); else lacksLanguage(layer);
    }
  }
});
test('meta-only old callers retain their explicit language without receiving a Chinese fallback', async () => {
  const e = environment({ options: { meta: { language: 'en-US', title: 'Explicit metadata' } } }); e.apply();
  await e.context.runExportOnce(); successfulExport(e);
  assert.equal(e.bridgeCalls[0][2].language, 'en-US'); assert.equal(e.bridgeCalls[0][3].meta.language, 'en-US');
});
