import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const read = file => fs.readFileSync(path.join(root, file), 'utf8');
const meta = JSON.parse(read('assets/i18n/meta.json'));
const locales = Object.keys(meta).sort();
const dictionaries = Object.fromEntries(locales.map(code => [code, JSON.parse(read(`assets/i18n/${code}.json`))]));
const keys = Object.keys(dictionaries.en).sort();
const placeholders = value => [...value.matchAll(/\{([\w.-]+)\}/g)].map(match => match[1]).sort();

function engine() {
  const context = vm.createContext({ window: {}, document: { addEventListener() {} } });
  vm.runInContext(read('assets/js/core/i18n.js'), context);
  return context.window.i18n;
}

function* interfaceFiles(directory = 'assets/js') {
  for (const entry of fs.readdirSync(path.join(root, directory), { withFileTypes: true })) {
    if (entry.name === 'vendor') continue;
    const file = `${directory}/${entry.name}`;
    if (entry.isDirectory()) yield* interfaceFiles(file);
    else if (entry.name.endsWith('.js')) yield file;
  }
}

// These checks certify dictionary/interpolation contracts. They deliberately do
// not equate script detection or English-copy counts with native semantic review.
test('all 46 packaged locale names and directions match the runtime catalog', () => {
  const i18n = engine();
  assert.equal(locales.length, 46);
  assert.deepEqual(JSON.parse(JSON.stringify(i18n.meta)), meta);
  assert.deepEqual(fs.readdirSync(path.join(root, 'assets/i18n')).filter(file => file.endsWith('.json') && file !== 'meta.json').sort(), locales.map(code => `${code}.json`).sort());
  assert.deepEqual(locales.filter(code => meta[code].dir === 'rtl'), ['ar', 'he', 'ug']);
  for (const code of locales) {
    assert.equal(typeof meta[code].native, 'string', code);
    assert(meta[code].native.trim(), `${code}: missing language name`);
    assert.equal(i18n.matchLanguage(code), code);
  }
  assert.equal(i18n.matchLanguage('ar-EG'), 'ar');
  assert.equal(i18n.matchLanguage('he-IL'), 'he');
  assert.equal(i18n.matchLanguage('ug-CN'), 'ug');
  assert.equal(i18n.matchLanguage('zh-Hant-HK'), 'zh-HK');
});

test('all shipped dictionary values are populated labels with identical placeholder contracts', () => {
  for (const [code, dictionary] of Object.entries(dictionaries)) {
    assert.deepEqual(Object.keys(dictionary).sort(), keys, code);
    for (const key of keys) {
      const value = dictionary[key];
      assert.equal(typeof value, 'string', `${code}: ${key}`);
      assert(value.trim(), `${code}: empty ${key}`);
      assert.notEqual(value, key, `${code}: raw key ${key}`);
      assert(!/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f\ufffd]/u.test(value), `${code}: invalid text ${key}`);
      assert.deepEqual(placeholders(value), placeholders(dictionaries.en[key]), `${code}: placeholders in ${key}`);
    }
  }
});

test('literal interface calls and accessible HTML labels have dictionary entries', () => {
  const missing = [];
  for (const file of [...interfaceFiles(), 'assets/app.js', 'assets/index.html']) {
    const text = read(file);
    // A complete literal argument excludes dynamically concatenated prefixes.
    // Bare words in feature-local wrappers are not global dictionary keys.
    const calls = /\b(?:i18n(?:\?\.|\.)t|_t|t)\(\s*(['"])([\w-]+(?:\.[\w-]+)+)\1\s*(?=[,)])/g;
    const attributes = /\bdata-i18n(?:-(?:aria|html|placeholder|title|label))?=["']([\w.-]+)["']/g;
    for (const [pattern, group] of [[calls, 2], [attributes, 1]]) {
      for (const match of text.matchAll(pattern)) {
        if (!Object.hasOwn(dictionaries.en, match[group])) {
          missing.push(`${file}:${text.slice(0, match.index).split('\n').length}: ${match[group]}`);
        }
      }
    }
  }
  assert.deepEqual(missing, []);
});

test('encoding recovery names its target without relying on an optional offending-character response', () => {
  const i18n = engine();
  for (const code of locales) {
    i18n.dict = dictionaries[code];
    i18n.fallbackDict = dictionaries.en;
    const message = i18n.t('dialog.encodingMessage', { encoding: '$& GBK {encoding}' });
    assert.notEqual(i18n.t('dialog.encodingTitle'), 'dialog.encodingTitle', code);
    assert.match(i18n.t('dialog.encodingUseUtf8'), /UTF-8/u, code);
    assert(message.includes('$& GBK {encoding}'), `${code}: literal original encoding`);
    assert.deepEqual(placeholders(dictionaries[code]['dialog.encodingMessage']), ['encoding'], `${code}: char is optional in the API response`);
    assert.notEqual(message, 'dialog.encodingMessage', code);
  }
});

test('new workflow labels and menu names resolve in every selected language', () => {
  const i18n = engine();
  const workflowKeys = [
    'batch.defaultOutput', 'batch.chooseOutput', 'batch.retryIncomplete',
    'pdf.readTitle', 'pdf.readHint', 'pdf.readUnavailable', 'pdf.extractMarkdown',
    'collaboration.resolution.compare', 'collaboration.resolution.draftKept',
    'collaboration.resolution.denied', 'convert.fidelity.title',
    'convert.fidelity.status.not_checked', 'ink.title', 'pdf.insertInk',
    'mediaTranscribe.start', 'mediaTranscribe.sourceChanged', 'common.loading', 'common.close',
  ];
  const menuKeys = keys.filter(key => /^(?:menu\.|pet\.menu\.)/.test(key));
  for (const code of locales) {
    i18n.dict = dictionaries[code];
    i18n.fallbackDict = dictionaries.en;
    for (const key of [...workflowKeys, ...menuKeys]) {
      assert.equal(i18n.t(key), dictionaries[code][key], `${code}: ${key}`);
      assert.notEqual(i18n.t(key), key, `${code}: raw ${key}`);
    }
  }
});
