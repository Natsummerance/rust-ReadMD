'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const {visibleText, hasAll} = require('./publication-batch3-checks.cjs');

const expected = '不能因预览或导出而改写源文。';
test('PDF physical Han line wraps retain every original character', () => {
  hasAll(visibleText('不能因预览或导出而改\n写源文。', 'pdf'), [expected], 'PDF');
  hasAll(visibleText('不能因预览或导出而改\r\n写源文。', 'pdf'), [expected], 'PDF');
  assert.equal(visibleText('甲\n乙\n丙', 'pdf'), '甲乙丙');
});
test('missing or reordered Han characters still fail the visible-content check', () => {
  for (const text of ['不能因预览或导出而\n写源文。', '不能因预览或导出而写\n改源文。']) {
    assert.throws(() => hasAll(visibleText(text, 'pdf'), [expected], 'PDF'), /dropped expected text/);
  }
});
test('literal Han spaces and Latin word spaces cannot be swallowed', () => {
  assert.throws(() => hasAll(visibleText('不能因预览或导出而改 写源文。', 'pdf'), [expected], 'PDF'), /dropped expected text/);
  assert.throws(() => hasAll(visibleText('Evidencefirst', 'pdf'), ['Evidence first'], 'PDF'), /dropped expected text/);
  hasAll(visibleText('Evidence\nfirst', 'pdf'), ['Evidence first'], 'PDF');
});
test('astral character loss is not hidden by PDF line-wrap normalization', () => {
  assert.throws(() => hasAll(visibleText('Unicode 懑', 'pdf'), ['Unicode 😀'], 'PDF'), /dropped expected text/);
  assert.throws(() => hasAll(visibleText('Unicode ', 'pdf'), ['Unicode 😀'], 'PDF'), /dropped expected text/);
});
test('non-PDF visible content retains its original whitespace boundary contract', () => {
  assert.equal(visibleText('甲\n乙', 'docx'), '甲 乙');
  assert.equal(visibleText('甲\r\n乙', 'epub'), '甲 乙');
});
