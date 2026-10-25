import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';

const read = name => JSON.parse(fs.readFileSync(new URL(name, import.meta.url), 'utf8'));
const spec = read('../i18n-v005-release-closeout.json');
const locales = Object.keys(read('../../assets/i18n/meta.json')).sort();
const errorKey = 'error.publication_paragraph_decoration_unsupported';
const warningKey = 'warn.math_revision_not_preserved';
const placeholders = value => [...value.matchAll(/\{([\w.-]+)\}/g)].map(match => match[1]).sort();

// These checks establish complete, applied translations and safe interpolation.
// They do not certify native-speaker semantic review of any language.
test('V005 release messages explicitly cover and populate all 46 locales', () => {
  assert.equal(locales.length, 46);
  assert.deepEqual(Object.keys(spec).sort(), [errorKey, warningKey].sort());
  for (const key of [errorKey, warningKey]) {
    assert.deepEqual(Object.keys(spec[key]).sort(), locales, key);
    for (const locale of locales) {
      const value = spec[key][locale];
      assert.equal(typeof value, 'string', `${locale}: ${key}`);
      assert.equal(value, value.trim(), `${locale}: surrounding whitespace`);
      assert(value.length > 0, `${locale}: empty ${key}`);
      assert.equal(read(`../../assets/i18n/${locale}.json`)[key], value, `${locale}: unapplied ${key}`);
      if (locale !== 'en') assert.notEqual(value, spec[key].en, `${locale}: copied English ${key}`);
    }
  }
});

test('V005 messages retain the Word name and exact interpolation contracts', () => {
  for (const key of [errorKey, warningKey]) {
    assert.deepEqual(placeholders(spec[key].en), [], `${key}: no runtime arguments`);
    for (const locale of locales) {
      const value = spec[key][locale];
      assert.deepEqual(placeholders(value), placeholders(spec[key].en), `${locale}: placeholders in ${key}`);
      assert(!/[\u0000-\u001f\ufffd]/u.test(value), `${locale}: invalid text in ${key}`);
    }
  }
  for (const locale of locales) assert.match(spec[warningKey][locale], /Word/u, `${locale}: Word name`);
});

test('English contracts and separate Chinese regional translations are present', () => {
  assert.equal(spec[errorKey].en, 'This output or view does not support paragraph shading or borders. Remove this decoration or choose a supported output.');
  assert.equal(spec[warningKey].en, 'Word formatting revision history in some equations could not be retained in this export. The source document is unchanged.');
  assert.equal(spec[warningKey]['zh-CN'], '部分公式的 Word 格式修订记录未保留在此导出文件中，源文档保持不变。');
  for (const key of [errorKey, warningKey]) {
    assert.notEqual(spec[key]['zh-HK'], spec[key]['zh-TW'], `${key}: explicit regional wording`);
    for (const locale of ['zh-CN', 'zh-HK', 'zh-TW']) assert.match(spec[key][locale], /[\u3400-\u9fff]/u);
  }
});
