'use strict';
// Original tiny examples only; no upstream fixture or RESULT is redistributed.
const test = require('node:test');
const assert = require('node:assert/strict');
const {canonicalHTML, adapt} = require('./publication-citation-oracle.cjs');

test('fixed format classes preserve exact HTML formatting semantics', () => {
  const examples = [
    ['italic', 'font-style:italic', '<em>X</em>'], ['oblique', 'font-style:oblique', '<span style="font-style:oblique">X</span>'],
    ['bold', 'font-weight:700', '<strong>X</strong>'], ['light', 'font-weight:300', '<span style="font-weight:300">X</span>'],
    ['smallcaps', 'font-variant:small-caps', '<span style="font-variant:small-caps">X</span>'],
    ['underline', 'text-decoration:underline', '<u>X</u>'],
    ['sup', 'vertical-align:super;font-size:smaller', '<sup>X</sup>'], ['sub', 'vertical-align:sub;font-size:smaller', '<sub>X</sub>'],
  ];
  for (const [name, style, expected] of examples) {
    assert.deepEqual(canonicalHTML(`<span class="readmd-csl-${name}" style="${style}">X</span>`), canonicalHTML(expected));
    assert.deepEqual(canonicalHTML(`<span class="readmd-csl-${name}">X</span>`), canonicalHTML(expected));
  }
});
test('unknown classes and class/style contradictions fail instead of being ignored', () => {
  assert.throws(() => canonicalHTML('<span class="unreviewed">X</span>'), /Unreviewed oracle rich class/);
  assert.throws(() => canonicalHTML('<span class="readmd-csl-italic" style="font-style:normal">X</span>'), /disagrees with style/);
  assert.throws(() => canonicalHTML('<span class="readmd-csl-sub readmd-csl-sup">X</span>'), /disagrees with style/);
});
test('canonical equivalence preserves punctuation and all citation text whitespace', () => {
  assert.notDeepEqual(canonicalHTML('Name, 2026.'), canonicalHTML('Name 2026.'));
  assert.notDeepEqual(canonicalHTML('Name, 2026.'), canonicalHTML('Name,  2026.'));
  assert.notDeepEqual(canonicalHTML('Name, 2026.'), canonicalHTML('Name, 2026'));
});
test('every HTML character and attribute must be consumed before comparison', () => {
  assert.throws(() => canonicalHTML('X<'), /Unconsumed independent oracle HTML text/);
  assert.throws(() => canonicalHTML('<span onclick=x>X</span>'), /Unconsumed independent oracle HTML attribute/);
  assert.throws(() => canonicalHTML('<span class="readmd-csl-italic" class="readmd-csl-bold">X</span>'), /Duplicate independent oracle HTML attribute/);
  assert.throws(() => canonicalHTML('<em>X</em unexpected>'), /closing-tag content/);
  assert.throws(() => canonicalHTML('<br onclick=x>'), /Unconsumed independent oracle HTML attribute/);
  assert.deepEqual(canonicalHTML('X<br/>Y'), canonicalHTML('X<br>Y'));
});
test('a CSS declaration cannot silently discard an extra colon or missing value', () => {
  assert.throws(() => canonicalHTML('<span style="font-style:italic:unexpected">X</span>'), /Unreviewed oracle CSS declaration/);
  assert.throws(() => canonicalHTML('<span style="font-style:">X</span>'), /Unreviewed oracle CSS declaration/);
});
function synthetic(styleClass) {
  return {INPUT: '[{"id":"synthetic"}]', CSL: `<style class="${styleClass}"/>`, MODE: 'citation', RESULT: '>>[0] Example, 2026',
    CITATIONS: JSON.stringify([[{citationID: 'one', properties: {noteIndex: 0}, citationItems: [{id: 'synthetic'}]}, [], []]])};
}
test('in-text CITATIONS protocol zero explicitly maps to no note without changing expected text', () => {
  const result = adapt(synthetic('in-text'));
  assert.deepEqual(result.request.contexts, [{note_number: null, in_note: false, note_label: null}]);
  assert.equal(result.expected, 'Example, 2026'); assert.match(result.adapter, /noteIndex 0 means no note/);
});
test('an unreviewed zero note index in a note style is an explicit adapter failure', () => {
  assert.throws(() => adapt(synthetic('note')), /separate semantic review/);
});
test('an explicit original locale is a separate request field and cannot rewrite style or RESULT', () => {
  const blocks = synthetic('in-text'), style = blocks.CSL, originalResult = blocks.RESULT;
  const localeXml = '<locale xml:lang="en-US" version="1.0"><terms><term name="translator" form="short">original</term></terms></locale>';
  const result = adapt(blocks, {localeXml});
  assert.equal(result.request.locale_xml, localeXml); assert.equal(result.request.csl, style);
  assert.equal(blocks.CSL, style); assert.equal(blocks.RESULT, originalResult);
  assert.equal(result.expected, 'Example, 2026'); assert.match(result.adapter, /original runtime locale XML/);
  assert.equal(adapt(blocks).request.locale_xml, undefined);
});
