'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { createHash } = require('node:crypto');
const { openAPIReviewProjection, metadataViewText } = require('../out/openapi-review-projection');
const hash = value => createHash('sha256').update(value).digest('hex');
const ID = '1'.repeat(64), summary = '4 operations, 2 source files';
const OPS = Object.fromEntries(['/items/get', '/items/post', '/other/post', '/search/get'].map(p => [p, '2'.repeat(64)]));
function region(body, { id = ID, eol = '\n', metadata = {}, encoded } = {}) {
  const value = { schema: 1, id, source: 'api.yaml', source_sha256: '3'.repeat(64), body_sha256: hash(body), operations: OPS,
    references: [{ source: 'defs/a.yaml', revision: '4'.repeat(64) }, { source: 'defs/b.yaml', revision: '5'.repeat(64) }], ...metadata };
  const payload = encoded ?? Buffer.from(JSON.stringify(value)).toString('base64url');
  return ('<!-- readmd:openapi:start ' + payload + ' -->\n' + body + '<!-- readmd:openapi:end ' + id + ' -->').replace(/\n/g, eol);
}
function generated(before, insert, from = before.length, to = from) {
  return { after: before.slice(0, from) + insert + before.slice(to), transaction: { base_revision: hash(before), origin: 'openapi', edits: [{ from, to, insert }] } };
}
function compact(text, span) { return metadataViewText(text, span, summary); }
function freeze(value) { if (value && typeof value === 'object') { Object.freeze(value); for (const child of Object.values(value)) freeze(child); } return value; }

test('append compacts only the backend carrier; full raw strings and frozen transaction stay exact', () => {
  const before = '# API guide 😀\n\nAuthor notes.\n', body = '## GET /items\n\nVISIBLE_RESPONSE\n';
  const result = freeze(generated(before, '\n\n' + region(body))), snapshot = JSON.stringify(result);
  const projection = openAPIReviewProjection(before, result.after, result.transaction);
  assert(projection); assert.equal(projection.before, undefined);
  assert.equal(compact(before, projection.before), before);
  assert.equal(compact(result.after, projection.after), before + '\n\n<!-- 4 operations, 2 source files -->\n' + body + '<!-- readmd:openapi:end ' + ID + ' -->');
  assert.equal(projection.after.operations, 4); assert.equal(projection.after.references, 2);
  assert(Object.isFrozen(projection) && Object.isFrozen(projection.after));
  assert.equal(JSON.stringify(result), snapshot);
});

test('an update projects both exact sides and preserves every other byte', () => {
  const old = region('## GET /items\nOLD\n'), next = region('## GET /items\nNEW\n');
  const prefix = '# Manual\n\n', suffix = '\n\nAuthor tail\n', before = prefix + old + suffix;
  const result = generated(before, next, prefix.length, prefix.length + old.length);
  const projection = openAPIReviewProjection(before, result.after, result.transaction);
  assert(projection?.before);
  assert.equal(compact(before, projection.before), prefix + '<!-- 4 operations, 2 source files -->\n## GET /items\nOLD\n<!-- readmd:openapi:end ' + ID + ' -->' + suffix);
  assert.equal(compact(result.after, projection.after), prefix + '<!-- 4 operations, 2 source files -->\n## GET /items\nNEW\n<!-- readmd:openapi:end ' + ID + ' -->' + suffix);
});

test('identical carriers in fenced examples/repeated prose and other API regions are never adopted', () => {
  const old = region('Body OLD\n'), next = region('Body NEW\n'), foreign = region('Other region\n', { id: 'a'.repeat(64) });
  const prefix = '```markdown\n' + old + '\n```\n\n' + foreign + '\n\nRepeated heading\n';
  const before = prefix + old + '\nTail', result = generated(before, next, prefix.length, prefix.length + old.length);
  const projection = openAPIReviewProjection(before, result.after, result.transaction);
  assert(projection?.before);
  assert.equal(compact(before, projection.before).slice(0, prefix.length), prefix);
  assert.equal(compact(result.after, projection.after).slice(0, prefix.length), prefix);
  assert.equal(before.slice(projection.before.from, projection.before.to), old.split('\n')[0]);
  assert.equal(projection.before.from, prefix.length, 'not the first equal carrier in the document');
});

test('UTF16 prefix delta4 and CRLF preserve raw coordinates and line endings', () => {
  const prefix = '😀Z\n'; assert.equal(prefix.length, 4);
  const old = region('OLD\n'), before = prefix + old, result = generated(before, region('NEW\n'), 4, 4 + old.length);
  const projection = openAPIReviewProjection(before, result.after, result.transaction);
  assert.equal(projection.before.from, 4); assert.equal(projection.after.from, 4);
  const crlfBefore = '# 中文😀\r\n', crlf = generated(crlfBefore, '\r\n\r\n' + region('BODY\n', { eol: '\r\n' }));
  const view = openAPIReviewProjection(crlfBefore, crlf.after, crlf.transaction);
  assert(view); assert.equal(compact(crlf.after, view.after), crlfBefore + '\r\n\r\n<!-- 4 operations, 2 source files -->\r\nBODY\r\n<!-- readmd:openapi:end ' + ID + ' -->');
});

test('unknown transaction shapes and mismatched applications retain raw display', () => {
  const before = '😀\n', fixture = generated(before, '\n\n' + region('BODY\n'));
  for (const change of [
    tx => { tx.base_revision = '0'.repeat(64); }, tx => { tx.origin = 'other'; },
    tx => { tx.edits = []; }, tx => { tx.edits.push({ from: before.length, to: before.length, insert: '' }); },
    tx => { tx.edits[0].from = 1; }, tx => { tx.edits[0].from = -1; },
    tx => { tx.edits[0].to = 999; }, tx => { tx.edits[0].from = 1.5; },
    tx => { tx.edits[0].from = NaN; }, tx => { tx.extra = true; },
  ]) {
    const tx = structuredClone(fixture.transaction); change(tx);
    assert.equal(openAPIReviewProjection(before, fixture.after, tx), undefined);
  }
  assert.equal(openAPIReviewProjection(before, fixture.after + 'unexpected', fixture.transaction), undefined);
  const mid = generated('abc\n', region('BODY\n'), 1, 1);
  assert.equal(openAPIReviewProjection('abc\n', mid.after, mid.transaction), undefined);
});

test('unknown or corrupt metadata is never hidden', () => {
  for (const options of [
    { metadata: { schema: 2 } }, { metadata: { unknown: true } }, { metadata: { body_sha256: '0'.repeat(64) } },
    { metadata: { operations: [] } }, { metadata: { id: 'bad' } }, { metadata: { references: [{}] } },
    { encoded: '!' }, { encoded: 'A' }, { encoded: Buffer.from([0xff]).toString('base64url') },
    { encoded: Buffer.from('{bad json}').toString('base64url') }, { encoded: 'x'.repeat(1024 * 1024 + 1) },
  ]) {
    const result = generated('', region('BODY\n', options));
    assert.equal(openAPIReviewProjection('', result.after, result.transaction), undefined);
  }
  const old = region('OLD\n'), result = generated(old, region('NEW\n', { id: 'a'.repeat(64) }), 0, old.length);
  assert.equal(openAPIReviewProjection(old, result.after, result.transaction), undefined);
});

test('an invalid summary cannot create structure or discard raw content', () => {
  const value = generated('', region('BODY\n')), p = openAPIReviewProjection('', value.after, value.transaction);
  for (const label of ['', 'two\nlines', 'two\rlines', 'comment --> escaped', 'x'.repeat(513)]) assert.equal(metadataViewText(value.after, p.after, label), value.after);
});

test('valid metadata looking identical but outside the replacement keeps its exact hash', () => {
  const outside = region('Outside\n'), current = region('Current\n'), before = outside + '\n\n' + current;
  const from = outside.length + 2, result = generated(before, region('Changed\n'), from, before.length);
  const p = openAPIReviewProjection(before, result.after, result.transaction);
  assert.equal(hash(compact(result.after, p.after).slice(0, outside.length)), hash(outside));
});
