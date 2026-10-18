'use strict';
// Independent physical-coordinate counterexamples; no product output is golden.
const test = require('node:test'), assert = require('node:assert/strict');
const {checkRichArtifact} = require('./publication-batch3-checks.cjs');

function evidence() {
  const letters = [...'UNDERLINE'].map((character, index) => ({unicode: character.codePointAt(0), glyphId: index + 1,
    origin: [103 + index * 7, 100], bbox: [103 + index * 7, 92, 110 + index * 7, 103]}));
  return {kind: 'pdf', text: 'Synthetic Evidence UNDERLINE SUP SUB', pages: [{
    spans: [{text: ' UNDERLINE ', font: 'Helvetica', bbox: [100, 89, 169, 104], origin: [100, 100]},
      {text: 'Synthetic Evidence', font: 'Helvetica-Oblique', bbox: [10, 89, 95, 104], origin: [10, 100]},
      {text: 'SUP', font: 'Helvetica', bbox: [180, 84, 197, 95], origin: [180, 93]},
      {text: 'SUB', font: 'Helvetica', bbox: [200, 96, 217, 107], origin: [200, 105]}],
    glyphTraces: [{font: 'Helvetica', size: 11, chars: letters}], drawings: [{bbox: [103, 101, 166, 101], width: 0.6}],
  }]};
}

test('physical underline covers the exact letters although parser span includes plain spaces', () => {
  assert.doesNotThrow(() => checkRichArtifact(evidence()));
});
test('missing physical underline fails despite matching text and font styles', () => {
  const result = evidence(); result.pages[0].drawings = [];
  assert.throws(() => checkRichArtifact(result), /no physical line/);
});
test('a short physical line cannot stand in for the full target word', () => {
  const result = evidence(); result.pages[0].drawings[0].bbox[2] = 140;
  assert.throws(() => checkRichArtifact(result), /no physical line/);
});
test('a line shifted horizontally beyond the original two-point tolerance fails', () => {
  const result = evidence(); result.pages[0].drawings[0].bbox = [108, 101, 171, 101];
  assert.throws(() => checkRichArtifact(result), /no physical line/);
});
test('a line above the baseline or below the original vertical budget fails', () => {
  for (const y of [99, 107]) {
    const result = evidence(); result.pages[0].drawings[0].bbox = [103, y, 166, y];
    assert.throws(() => checkRichArtifact(result), /no physical line/);
  }
});
test('zero-width strokes and absent actual glyph evidence cannot certify an underline', () => {
  const result = evidence(); result.pages[0].drawings[0].width = 0;
  assert.throws(() => checkRichArtifact(result), /no physical line/);
  const missing = evidence(); delete missing.pages[0].glyphTraces;
  assert.throws(() => checkRichArtifact(missing), /requires --glyphs/);
});
