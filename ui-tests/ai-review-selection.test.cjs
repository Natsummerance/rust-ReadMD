'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const selection = require('../assets/js/features/ai-selection.js');

// Small deterministic DOM fixture: no browser, parser replacement or new
// dependency. Source offsets in assertions are independent fixed expectations.
class Element {
  constructor(tag, attrs = {}, ...children) {
    this.nodeType = 1; this.tagName = tag.toUpperCase(); this.attrs = attrs;
    this.id = attrs.id || ''; this.dataset = { ...(attrs['data-source-line'] ? { sourceLine: String(attrs['data-source-line']) } : {}) };
    this.childNodes = children.map(child => typeof child === 'string' ? new Text(child) : child);
    for (const child of this.childNodes) child.parentElement = this;
  }
  get children() { return this.childNodes.filter(child => child.nodeType === 1); }
  matches(selectors) {
    return selectors.split(',').some(selector => {
      selector = selector.trim();
      if (selector[0] === '.') return (this.attrs.class || '').split(/\s+/).includes(selector.slice(1));
      const attr = /^(\w+)?\[([^=\]]+)(?:="([^"]*)")?\]$/.exec(selector);
      if (attr) return (!attr[1] || this.tagName === attr[1].toUpperCase()) && Object.hasOwn(this.attrs, attr[2]) && (attr[3] === undefined || this.attrs[attr[2]] === attr[3]);
      return this.tagName === selector.toUpperCase();
    });
  }
  closest(selector) { for (let node = this; node; node = node.parentElement) if (node.matches(selector)) return node; return null; }
  contains(target) { return target === this || this.childNodes.some(child => child === target || child.nodeType === 1 && child.contains(target)); }
  querySelector(selector) { for (const child of this.children) { if (child.matches(selector)) return child; const nested = child.querySelector(selector); if (nested) return nested; } return null; }
}
class Text { constructor(text) { this.nodeType = 3; this.nodeValue = text; this.parentElement = null; } }
const e = (tag, attrs, ...children) => new Element(tag, attrs, ...children);
const root = (...children) => e('div', { id: 'content' }, e('article', { class: 'markdown-body' }, ...children));
const range = (node, start, end, endNode = node) => ({ startContainer: node, startOffset: start, endContainer: endNode, endOffset: end });
const bounds = value => value && ({ from: value.from, to: value.to, text: value.text, displayText: value.displayText, exact: value.exact });

test('second repeated heading maps bold text and link label without changing delimiters or URL', () => {
  const source = '# **Same** and [Same](url)\n\n# **Same** and [Same](url)\n';
  const first = e('h1', { 'data-source-line': 1 }, e('strong', {}, 'Same'), ' and ', e('a', { href: 'url' }, 'Same'));
  const bold = e('strong', {}, 'Same'), link = e('a', { href: 'url' }, 'Same');
  const second = e('h1', { 'data-source-line': 3 }, bold, ' and ', link, e('a', { class: 'rd-anchor', 'data-rd-chrome': '' }, '#'));
  const body = root(first, second);
  assert.deepEqual(bounds(selection.mapRange(source, body, range(link.childNodes[0], 0, 4))), { from: 44, to: 48, text: 'Same', displayText: 'Same', exact: true });
  const mapped = selection.mapRange(source, body, range(bold.childNodes[0], 1, 3));
  assert.equal(mapped.from, 33); assert.equal(mapped.to, 35);
  assert.equal(source.slice(0, mapped.from) + 'NEW' + source.slice(mapped.to), '# **Same** and [Same](url)\n\n# **SNEWe** and [Same](url)\n');
  assert.equal(selection.mapRange(source, body, range(second.children[2].childNodes[0], 0, 1)), null, 'heading chrome is not a source selection');
});

test('list item ordinal distinguishes identical text when the renderer has no source-line attribute', () => {
  const source = '- Same **word**\n- Same **word**\n';
  const secondWord = e('strong', {}, 'word');
  const list = e('ul', {}, e('li', {}, 'Same ', e('strong', {}, 'word')), e('li', {}, 'Same ', secondWord));
  const body = root(list);
  assert.deepEqual(bounds(selection.mapRange(source, body, range(secondWord.childNodes[0], 0, 4))), { from: 25, to: 29, text: 'word', displayText: 'word', exact: true });
  assert.equal(selection.mapRange(source, body, range(secondWord.childNodes[0], 0, 4), { partial: true }), null, 'an unanchored paged fragment cannot claim the first identical list');
});

test('nested ordered list inside blockquote retains quote markers, item markers and link destination', () => {
  const source = '> 1. Same [word](url)\n> 2. Same [word](url)\n';
  const secondWord = e('a', { href: 'url' }, 'word');
  const quote = e('blockquote', { 'data-source-line': 1 }, e('ol', {},
    e('li', {}, 'Same ', e('a', { href: 'url' }, 'word')), e('li', {}, 'Same ', secondWord)));
  const mapped = selection.mapRange(source, root(quote), range(secondWord.childNodes[0], 0, 4));
  assert.deepEqual(bounds(mapped), { from: 33, to: 37, text: 'word', displayText: 'word', exact: true });
  assert.equal(source.slice(0, mapped.from) + 'changed' + source.slice(mapped.to), '> 1. Same [word](url)\n> 2. Same [changed](url)\n');
});

test('escape, entity and emoji ranges include complete source units and reject partial projections', () => {
  const source = 'Text \\* &amp; &#x1F600; &NotEqualTilde;.\r\n';
  assert.deepEqual(bounds(selection.mapText(source, [0], 5, 6)), { from: 5, to: 7, text: '\\*', displayText: '*', exact: true });
  assert.deepEqual(bounds(selection.mapText(source, [0], 7, 8)), { from: 8, to: 13, text: '&amp;', displayText: '&', exact: true });
  assert.deepEqual(bounds(selection.mapText(source, [0], 9, 11)), { from: 14, to: 23, text: '&#x1F600;', displayText: '😀', exact: true });
  assert.equal(selection.mapText(source, [0], 10, 11), null);
  assert.equal(selection.mapText(source, [0], 12, 13), null);
  assert.deepEqual(bounds(selection.mapText(source, [0], 12, 14)), { from: 24, to: 39, text: '&NotEqualTilde;', displayText: '≂̸', exact: true });
});

test('BOM and CRLF normalize to the same UTF-16 offsets used by the editor and kernel', () => {
  const source = '\uFEFF**😀**\r\n\r\n**😀**\r\n';
  assert.deepEqual(bounds(selection.mapText(source, [1], 0, 2)), { from: 10, to: 12, text: '😀', displayText: '😀', exact: true });
  assert.equal(selection.mapText(source, [1], 0, 1), null);
});

test('metadata omission, reference definitions and explicit heading IDs retain exact source offsets', () => {
  const metadata = '---\ntitle: Test\n---\n\n# Heading\n\n- Same\n- Same\n';
  assert.deepEqual(bounds(selection.mapText(metadata, [1, 1], 0, 4)), { from: 41, to: 45, text: 'Same', displayText: 'Same', exact: true });
  const reference = '[id]: https://example.test\n\n[same][id]\n';
  assert.deepEqual(bounds(selection.mapText(reference, [0], 0, 4)), { from: 29, to: 33, text: 'same', displayText: 'same', exact: true });
  const heading = '## Same **word** {#identity}\n';
  assert.deepEqual(bounds(selection.mapText(heading, [0], 5, 9)), { from: 10, to: 14, text: 'word', displayText: 'word', exact: true });
});

test('selection ending outside a partial bold span cannot leave dangling delimiters', () => {
  const source = '**bold** plain';
  assert.equal(selection.mapText(source, [0], 2, 8), null);
  assert.deepEqual(bounds(selection.mapText(source, [0], 0, 10)), { from: 0, to: 14, text: '**bold** plain', displayText: 'bold plain', exact: true });
});

test('formula, code, table, raw HTML and transformed DOM projections remain explicitly read-only', () => {
  for (const source of ['$x + y$', '`same`', 'Safe <span>same</span>']) assert.equal(selection.mapText(source, [0], 0, 4), null);
  assert.equal(selection.mapText('| A |\n| --- |\n| same |\n', [0], 0, 1), null);
  const body = root(e('p', { 'data-source-line': 1 }, 'Changed projection'));
  assert.equal(selection.mapRange('Original source', body, range(body.children[0].children[0].childNodes[0], 0, 7)), null);
});

test('DOM element boundary offsets and skipped chrome use document text rather than hidden controls', () => {
  const source = 'A **bold** end';
  const bold = e('strong', {}, 'bold'), paragraph = e('p', { 'data-source-line': 1 }, 'A ', bold, ' end', e('button', { 'data-rd-chrome': '' }, 'control'));
  const body = root(paragraph);
  assert.deepEqual(bounds(selection.mapRange(source, body, range(paragraph, 1, 2))), { from: 4, to: 8, text: 'bold', displayText: 'bold', exact: true });
  assert.equal(selection.mapRange(source, body, range(paragraph.children[1].childNodes[0], 0, 7)), null);
});
