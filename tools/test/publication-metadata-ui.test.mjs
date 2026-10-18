import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const context = {window: {}};
vm.createContext(context);
vm.runInContext(fs.readFileSync(new URL('../../assets/js/editor/publication-metadata.js', import.meta.url), 'utf8'), context);
const changes = (source, edits) => JSON.parse(JSON.stringify(context.window.ReadMDPublicationMetadata.toEditorChanges(source, edits)));
const apply = (source, edits) => {let output = '', cursor = 0; for (const e of edits) {output += source.slice(cursor, e.from) + e.insert; cursor = e.to;} return output + source.slice(cursor);};

test('persisted CRLF ranges map to the same Unicode values in a single CM transaction', () => {
  const source = '\uFEFF---\r\ntitle: "中文🧭"\r\nauthors:\r\n  - name: "Ada, A."\r\n---\r\nBody 𝛂\r\n';
  const from = source.indexOf('"Ada, A."'), title = source.indexOf('"中文🧭"');
  const edits = [{from: title, to: title + '"中文🧭"'.length, insert: '"新标题"'}, {from, to: from + '"Ada, A."'.length, insert: '"Élodie, 甲"'}];
  const actual = changes(source, edits);
  assert.equal(apply(source.replace(/\r\n/g, '\n'), actual), apply(source, edits).replace(/\r\n/g, '\n'));
  assert.equal(actual.length, 2);
  assert.equal(source.replace(/\r\n/g, '\n').slice(actual[1].from, actual[1].to), '"Ada, A."');
});
test('inserted CRLF rows normalize inside CM without consuming body or comments', () => {
  const source = '---\r\nauthors: [] # retained\r\n---\r\nBody\r\n', from = source.indexOf('[]');
  const edits = [{from, to: from + 2, insert: '\r\n  - id: a1\r\n    name: Ada'}];
  assert.equal(apply(source.replace(/\r\n/g, '\n'), changes(source, edits)), apply(source, edits).replace(/\r\n/g, '\n'));
});
test('split CRLF, split surrogate pairs and overlapping ranges are rejected', () => {
  const source = '🧭\r\nabc';
  for (const edits of [[{from: 1, to: 2, insert: ''}], [{from: 3, to: 3, insert: ''}], [{from: 4, to: 6, insert: ''}, {from: 5, to: 7, insert: ''}], [{from: -1, to: 0, insert: ''}]]) assert.throws(() => changes(source, edits), /transaction_invalid_range/);
});
