import test from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const T = require('../../assets/js/editor/md-transforms.js');
const marked = require('../../assets/vendor/marked.min.js');
const fixture = require('../fixtures/toc-consistency.cjs');

function linksIn(markdown) {
  const links = [];
  marked.walkTokens(marked.lexer(markdown, { gfm: true }), token => {
    if (token.type === 'link') links.push(token);
  });
  return links;
}

test('TOC labels preserve literal Markdown punctuation and code text', () => {
  const source = '# Path \\ [label] \\*literal\\*\n\n# Code `&amp; <i> _x_`';
  assert.equal(T.tocMarkdown(source), String.raw`- [Path &#92; &#91;label&#93; \*literal\*](#path--label-literal)
- [Code \&amp\; \<i\> \_x\_](#code-amp-i-_x_)`);
});

test('Node TOC identities cover Setext, explicit and colliding IDs, entities and Unicode', () => {
  const source = fixture.source;
  const links = linksIn(T.tocMarkdown(source));
  assert.deepEqual(links.map(link => decodeURIComponent(link.href.slice(1))), fixture.headings.map(h => h.id));
  assert.equal(source, fixture.source);
  assert.equal(T.tocMarkdown('no headings\n\n```md\n# example\n```'), '');
  assert.match(T.tocMarkdown(source), /\]\(#Case%25%2F%E4%B8%AD%E6%96%87%5Bone%5D%28x%29\)/);
});

test('Node and browser use the same heading parser and Rust-compatible allocation order', () => {
  const H = require('../../assets/js/reader/heading-identity.js');
  assert.deepEqual(H.fromMarkdown(fixture.source).map(({id, text, level}) => ({id, text, level})), fixture.headings);
  const allocate = H.createIds();
  assert.deepEqual([
    allocate('A'), allocate('A-1'), allocate('A'),
    allocate('Other', 'a-1'), allocate('A'), allocate('!!!'), allocate('???'),
  ], ['a', 'a-1', 'a-2', 'a-1-1', 'a-3', 'section', 'section-1']);
  const seen = Object.create(null);
  assert.deepEqual(['A', 'A-1', 'A', 'A', '!!!', '???', '中文 α'].map(title => T.headingSlug(title, seen)),
    ['a', 'a-1', 'a-2', 'a-3', 'section', 'section-1', '中文-α']);
});

test('TOC parsing leaves CRLF source and front matter untouched', () => {
  const H = require('../../assets/js/reader/heading-identity.js');
  const source = '\uFEFF---\r\ntitle: Keep me\r\n---\r\n# Title\r\n\r\nUnderlined\r\n==========\r\n';
  const snapshot = Buffer.from(source);
  assert.deepEqual(H.fromMarkdown(source).map(({id, sourceLine}) => ({id, sourceLine})), [
    { id: 'title', sourceLine: 4 }, { id: 'underlined', sourceLine: 6 },
  ]);
  T.tocMarkdown(source);
  assert.deepEqual(Buffer.from(source), snapshot);
});

test('text entities decode once while code spans and incomplete references stay literal', () => {
  const H = require('../../assets/js/reader/heading-identity.js');
  const source = '# &amp;amp; &copy; &copy &#13; &#0; &#xD800;\n\n# `&amp;` \\&amp;';
  assert.deepEqual(H.fromMarkdown(source).map(h => h.text), [
    '&amp; © &copy \r \uFFFD \uFFFD', '&amp; &amp;',
  ]);
});
