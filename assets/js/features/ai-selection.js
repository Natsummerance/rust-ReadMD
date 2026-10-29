'use strict';
/* Exact ordinary-prose selection projection. Token locations come from the
   parser's remaining-input cursor; equal strings never identify source. */
(function (root, factory) {
  if (typeof module !== 'undefined' && module.exports) module.exports = factory(
    () => require('../../vendor/marked.min.js'), null, require('../../vendor/html5-character-references.json'),
    () => require('../reader/heading-identity.js'));
  else root.ReadMDAiSelection = factory(() => root.marked, root.document, null, () => root.ReadMDHeadings);
})(typeof window === 'undefined' ? null : window, (getMarked, document, entities, getHeadings) => {
  const normalize = source => String(source ?? '').replace(/^\uFEFF/, '').replace(/\r\n?/g, '\n');
  const CHROME = '[data-rd-chrome],button,input,textarea,select,mjx-assistive-mml,.rd-anchor,.review-block-actions';
  const UNSAFE = 'pre,code,table,math,mjx-container,mjx-assistive-mml,svg,.diagram-card,.code-chunk-card,.rd-footnotes,.rd-callout,.academic-callout,[data-readmd-footnote]';
  const LEAF = 'p,h1,h2,h3,h4,h5,h6,li';
  const numeric = { 128:8364,130:8218,131:402,132:8222,133:8230,134:8224,135:8225,136:710,137:8240,138:352,139:8249,140:338,142:381,145:8216,146:8217,147:8220,148:8221,149:8226,150:8211,151:8212,152:732,153:8482,154:353,155:8250,156:339,158:382,159:376 };
  let decoder, cached;
  const origin = (view, at) => view.positions ? view.positions[at] : view.offset + at;
  const sub = (view, from, to) => ({ text: view.text.slice(from, to), offset: view.positions ? 0 : view.offset + from, positions: view.positions?.slice(from, to) });
  const span = (view, from, to) => ({ from: origin(view, from), to: origin(view, to - 1) + 1 });
  function decode(reference) {
    if (document) {
      decoder ||= document.createElement('textarea'); decoder.innerHTML = reference; return decoder.textContent;
    }
    const key = reference.slice(1);
    if (key[0] !== '#') return entities[key] ?? reference;
    const hex = /^#[xX]/.test(key); let code = parseInt(key.slice(hex ? 2 : 1), hex ? 16 : 10);
    if (!code || code > 0x10ffff || code >= 0xd800 && code <= 0xdfff) code = 0xfffd;
    return String.fromCodePoint(numeric[code] || code);
  }
  function inline(view, tokens) {
    const result = { text: '', spans: [], containers: [] };
    const append = (text, range) => { result.text += text; for (let i = 0; i < text.length; i++) result.spans.push(range); };
    function literal(mapped) {
      for (let at = 0; at < mapped.text.length;) {
        const reference = mapped.text[at] === '&' ? /^&(?:#\d{1,7}|#[xX][\da-fA-F]{1,6}|\w+);/.exec(mapped.text.slice(at))?.[0] : null;
        const raw = reference || String.fromCodePoint(mapped.text.codePointAt(at));
        append(reference ? decode(reference) : raw, span(mapped, at, at + raw.length)); at += raw.length;
      }
    }
    function walk(mapped, parts) {
      let cursor = 0;
      for (const token of parts || []) {
        const raw = token.raw || '';
        if (!raw || mapped.text.slice(cursor, cursor + raw.length) !== raw) return false;
        const part = sub(mapped, cursor, cursor + raw.length); cursor += raw.length;
        if (token.type === 'text') {
          if (token.tokens) { if (!walk(part, token.tokens)) return false; }
          else literal(part);
        } else if (token.type === 'escape') append(token.text, span(part, 0, raw.length));
        else if (token.type === 'br') append('\n', span(part, 0, raw.length));
        else if (['strong', 'em', 'del', 'link'].includes(token.type)) {
          let from, to;
          if (token.type === 'link') {
            if (raw[0] === '[') {
              let depth = 1; from = 1;
              for (let i = 1; i < raw.length; i++) {
                if (raw[i] === '\\') { i++; continue; }
                if (raw[i] === '[') depth++;
                if (raw[i] === ']' && --depth === 0) { to = i; break; }
              }
            } else if (raw[0] === '<' && raw.endsWith('>')) { from = 1; to = raw.length - 1; }
            else if (raw === token.text) { from = 0; to = raw.length; }
          } else {
            const size = token.type === 'em' ? 1 : 2, mark = raw[0].repeat(size);
            if (raw.startsWith(mark) && raw.endsWith(mark)) { from = size; to = raw.length - size; }
          }
          if (!Number.isInteger(from) || !Number.isInteger(to) || to < from || raw.slice(from, to) !== token.text) return false;
          const outFrom = result.text.length;
          if (!walk(sub(part, from, to), token.tokens)) return false;
          result.containers.push({ ...span(part, 0, raw.length), outFrom, outTo: result.text.length });
        } else return false; // Code, images, HTML and extensions are read-only.
      }
      return cursor === mapped.text.length;
    }
    return walk(view, tokens) ? result : null;
  }
  function stripLines(view, strip, expected) {
    let text = '', positions = [], cursor = 0, row = 0;
    for (const line of view.text.match(/[^\n]*\n|[^\n]+$/g) || []) {
      const count = strip(line, row++); if (!Number.isInteger(count) || count < 0 || count > line.length) return null;
      for (let i = count; i < line.length; i++) { text += line[i]; positions.push(origin(view, cursor + i)); }
      cursor += line.length;
    }
    if (!text.startsWith(expected) || !/^\n*$/.test(text.slice(expected.length))) return null;
    return { text: expected, positions: positions.slice(0, expected.length), offset: 0 };
  }
  function lexer(view, links, breaks) {
    const marked = getMarked(); if (!marked?.Tokenizer || !marked?.Lexer) return null;
    const tokenizer = new marked.Tokenizer();
    for (const name of ['space','code','fences','heading','hr','blockquote','list','html','def','table','lheading','paragraph','text']) {
      const parse = tokenizer[name];
      if (typeof parse === 'function') tokenizer[name] = function (remaining, ...args) {
        const token = parse.call(this, remaining, ...args);
        if (token) token.readmdOffset = view.text.length - remaining.length;
        return token;
      };
    }
    const parser = new marked.Lexer({ gfm: true, breaks, tokenizer });
    if (links) parser.tokens.links = { ...links };
    return { parser, tokens: parser.lex(view.text) };
  }
  function blocks(source, options = {}) {
    source = normalize(source); const breaks = Boolean(options.breaks);
    if (cached?.source === source && cached.breaks === breaks) return cached;
    const lineStarts = [0]; for (let i = 0; i < source.length; i++) if (source[i] === '\n') lineStarts.push(i + 1);
    function lineOf(at) { let low = 0, high = lineStarts.length; while (low + 1 < high) { const middle = (low + high) >> 1; if (lineStarts[middle] <= at) low = middle; else high = middle; } return low + 1; }
    // Mirror the reader's line-preserving metadata omission, retaining original
    // offsets by replacing metadata characters with spaces rather than deleting.
    const prepared = source.replace(/^---\n([\s\S]*?)\n---(?:\n|$)/, (raw, fields) => /^[ \t]*[A-Za-z_][\w-]*[ \t]*:/m.test(fields) ? raw.replace(/[^\n]/g, ' ') : raw);
    function parse(view, inheritedLinks) {
      const lexed = lexer(view, inheritedLinks, breaks); if (!lexed) return [];
      const { parser, tokens } = lexed, tree = [];
      for (const token of tokens) {
        if (token.type === 'space' || token.type === 'def') continue;
        const at = token.readmdOffset, raw = token.raw || '';
        if (!Number.isInteger(at) || at < 0 || view.text.slice(at, at + raw.length) !== raw) { tree.push({ tag: null }); continue; }
        const mapped = sub(view, at, at + raw.length), location = span(mapped, 0, raw.length);
        const node = { ...location, line: lineOf(location.from), tag: null, inline: null, children: [] };
        if (token.type === 'paragraph' || token.type === 'heading' || token.type === 'text') {
          node.tag = token.type === 'heading' ? 'H' + token.depth : 'P';
          let from = 0, text = token.text || '';
          if (token.type === 'heading') {
            const opening = /^ {0,3}#{1,6}(?:[ \t]+|$)/.exec(raw); from = opening?.[0].length || 0;
            const headings = getHeadings?.(); if (headings?.attributes) text = headings.attributes(text).text;
          }
          if (raw.slice(from, from + text.length) === text && !/(^|[^\\])\$|\\[([]|<!--readmd:math:/.test(text)) {
            const contents = sub(mapped, from, from + text.length);
            node.inline = inline(contents, parser.inlineTokens(text));
          }
        } else if (token.type === 'blockquote') {
          node.tag = 'BLOCKQUOTE';
          const content = stripLines(mapped, line => /^ {0,3}>[ \t]?/.exec(line)?.[0].length || 0, token.text);
          if (content) node.children = parse(content, tokens.links);
        } else if (token.type === 'list') {
          node.tag = token.ordered ? 'OL' : 'UL'; let cursor = 0;
          for (const item of token.items) {
            const itemRaw = item.raw || '', itemView = sub(mapped, cursor, cursor + itemRaw.length); cursor += itemRaw.length;
            const child = { tag: 'LI', inline: null, children: [], task: Boolean(item.task) };
            if (itemView.text === itemRaw) {
              const marker = /^ {0,3}(?:[*+-]|\d{1,9}[.)])[ ]+/.exec(itemRaw), indent = marker?.[0].length;
              if (indent && !itemRaw.includes('\t')) {
                const content = stripLines(itemView, (line, row) => row === 0 ? indent + (item.task ? /^\[[ xX]\][ ]+/.exec(line.slice(indent))?.[0].length || 0 : 0) : Math.min(indent, /^ */.exec(line)[0].length), item.text);
                if (content) { child.children = parse(content, tokens.links); if (child.children.length === 1 && child.children[0].tag === 'P') child.inline = child.children[0].inline; }
              }
            }
            node.children.push(child);
          }
        } else if (token.type === 'code') node.tag = 'PRE';
        else if (token.type === 'table') node.tag = 'TABLE';
        else if (token.type === 'hr') node.tag = 'HR';
        tree.push(node);
      }
      return tree;
    }
    cached = { source, breaks, tree: parse({ text: prepared, offset: 0, positions: null }) };
    return cached;
  }
  function mapInline(source, projection, from, to) {
    if (!projection || !Number.isInteger(from) || !Number.isInteger(to) || from < 0 || from >= to || to > projection.text.length) return null;
    const first = projection.spans[from], last = projection.spans[to - 1]; if (!first || !last) return null;
    if (from && projection.spans[from - 1]?.to > first.from || to < projection.spans.length && projection.spans[to]?.from < last.to) return null;
    let start = first.from, end = last.to;
    for (const container of projection.containers) {
      if (to <= container.outFrom || from >= container.outTo || from >= container.outFrom && to <= container.outTo) continue;
      if (from <= container.outFrom && to >= container.outTo) { start = Math.min(start, container.from); end = Math.max(end, container.to); }
      else return null;
    }
    // Removed container prefixes between text lines cannot be silently eaten by
    // an inline replacement. Keep those cross-line projections read-only.
    if (/\n[ \t]*(?:>|[*+-] |\d+[.)] )/.test(source.slice(start, end))) return null;
    return { from: start, to: end, text: source.slice(start, end), displayText: projection.text.slice(from, to), exact: true };
  }
  function mapText(source, path, from, to, options = {}) {
    const indexed = blocks(source, options); let node = { children: indexed.tree };
    for (const index of path) node = node?.children?.[index];
    return mapInline(indexed.source, node?.inline, from, to);
  }
  function element(node) { return node?.nodeType === 1 ? node : node?.parentElement; }
  function visibleChildren(node) { return Array.from(node?.children || []).filter(child => !child.matches(CHROME)); }
  function topNode(indexed, root, top, partial) {
    const line = Number(top.dataset?.sourceLine);
    if (Number.isInteger(line) && line > 0) return indexed.tree.find(node => node.line === line && node.tag === top.tagName) || null;
    const siblings = visibleChildren(root), target = siblings.indexOf(top); if (target < 0) return null;
    for (let i = target - 1; i >= 0; i--) {
      const anchorLine = Number(siblings[i].dataset?.sourceLine); if (!anchorLine) continue;
      const anchor = indexed.tree.findIndex(node => node.line === anchorLine && node.tag === siblings[i].tagName); if (anchor < 0) return null;
      for (let k = i + 1; k <= target; k++) if (indexed.tree[anchor + k - i]?.tag !== siblings[k].tagName) return null;
      return indexed.tree[anchor + target - i];
    }
    if (partial || siblings.length !== indexed.tree.length || siblings.some((child, i) => child.tagName !== indexed.tree[i].tag)) return null;
    return indexed.tree[target];
  }
  function leafProjection(node, dom, target) {
    if (!node || node.tag !== dom.tagName) return null;
    if (dom === target) return node.inline;
    const children = visibleChildren(dom), index = children.findIndex(child => child === target || child.contains(target));
    if (index < 0 || children.length !== node.children.length || children.some((child, i) => child.tagName !== node.children[i]?.tag)) return null;
    return leafProjection(node.children[index], children[index], target);
  }
  function domText(root) {
    let text = ''; const positions = new WeakMap();
    function visit(node) {
      const start = text.length;
      if (node.nodeType === 3) { text += node.nodeValue || ''; positions.set(node, { start, end: text.length }); return; }
      if (node.nodeType !== 1 || node.matches(CHROME)) return;
      if (node.tagName === 'BR') { text += '\n'; positions.set(node, { start, end: text.length, boundaries: [start] }); return; }
      const boundaries = [start]; for (const child of node.childNodes || []) { visit(child); boundaries.push(text.length); }
      positions.set(node, { start, end: text.length, boundaries });
    }
    visit(root);
    return { text, at(node, offset) { const entry = positions.get(node); if (!entry || !Number.isInteger(offset) || offset < 0) return null; return node.nodeType === 3 ? offset <= (node.nodeValue || '').length ? entry.start + offset : null : entry.boundaries?.[offset] ?? null; } };
  }
  function mapRange(source, root, range, options = {}) {
    try {
      const start = element(range.startContainer), end = element(range.endContainer);
      if (!start || !end || !root.contains(start) || !root.contains(end) || start.closest(CHROME + ',' + UNSAFE) || end.closest(CHROME + ',' + UNSAFE)) return null;
      const leaf = start.closest(LEAF); if (!leaf || !leaf.contains(range.endContainer) || leaf.querySelector(UNSAFE)) return null;
      const container = leaf.closest('.markdown-body') || root; if (!root.contains(container)) return null;
      let top = leaf; while (top.parentElement && top.parentElement !== container) top = top.parentElement;
      if (top.parentElement !== container) return null;
      const indexed = blocks(source, options), node = topNode(indexed, container, top, Boolean(options.partial));
      let projection = leafProjection(node, top, leaf); if (!projection) return null;
      const dom = domText(leaf);
      // Marked inserts one display space after a task checkbox. It has no
      // editable source counterpart, while the task's actual text still maps.
      if (leaf.querySelector('input[type="checkbox"]') && dom.text === ' ' + projection.text) projection = {
        text: ' ' + projection.text, spans: [null, ...projection.spans], containers: projection.containers.map(item => ({ ...item, outFrom: item.outFrom + 1, outTo: item.outTo + 1 })) };
      if (dom.text !== projection.text) return null;
      const from = dom.at(range.startContainer, range.startOffset), to = dom.at(range.endContainer, range.endOffset);
      return mapInline(indexed.source, projection, from, to);
    } catch (_) { return null; }
  }
  function readonlyTarget(target, source) {
    return { ...target, source, isSelection: true, exact: false, from: -1, to: -1, conflict: true };
  }
  function exactTarget(target, source) {
    const boundary = at => at === 0 || at === source.length || !(/[\uD800-\uDBFF]/.test(source[at - 1]) && /[\uDC00-\uDFFF]/.test(source[at]));
    return target.exact !== false && target.source === source && Number.isInteger(target.from) && Number.isInteger(target.to) &&
      target.from >= 0 && target.to > target.from && target.to <= source.length && boundary(target.from) && boundary(target.to) &&
      target.text === source.slice(target.from, target.to);
  }
  function currentTarget(target, source) {
    if (!target.isSelection) return { ...target, source, text: source, from: 0, to: source.length };
    return exactTarget(target, source) ? { ...target, exact: true } : readonlyTarget(target, source);
  }
  function remapTarget(target, transaction) {
    if (!target || target.tabId !== transaction?.tabId) return target;
    const { before, after, edits, mapPosition } = transaction;
    if (typeof after !== 'string') return readonlyTarget(target, target.source);
    if (!target.isSelection) return currentTarget(target, after);
    if (typeof before !== 'string' || !exactTarget(target, before) || !Array.isArray(edits) || typeof mapPosition !== 'function') return readonlyTarget(target, after);
    let cursor = 0, content = '';
    for (const edit of edits) {
      if (!Number.isInteger(edit.from) || !Number.isInteger(edit.to) || edit.from < cursor || edit.to < edit.from || edit.to > before.length || typeof edit.insert !== 'string') return readonlyTarget(target, after);
      // A replacement crossing a selection edge cannot prove which part of
      // its inserted text belongs to that selection. Never widen its grant.
      if (edit.from < target.from && edit.to > target.from || edit.from < target.to && edit.to > target.to) return readonlyTarget(target, after);
      content += before.slice(cursor, edit.from) + edit.insert; cursor = edit.to;
    }
    if (content + before.slice(cursor) !== after) return readonlyTarget(target, after);
    const from = mapPosition(target.from, 1), to = mapPosition(target.to, -1);
    const mapped = { ...target, source: after, from, to, text: after.slice(from, to), displayText: after.slice(from, to), exact: true, conflict: false };
    return exactTarget(mapped, after) ? mapped : readonlyTarget(target, after);
  }
  return { blocks, mapText, mapRange, currentTarget, remapTarget };
});
