'use strict';

const assert = require('node:assert/strict');
const { createHash } = require('node:crypto');
const normalize = text => text.replace(/\r\n?/g, '\n');
const canonicalBytes = text => Buffer.from(text.replaceAll('\n', '\r\n'));
// Derived from c5f8d80d after verifying its original full-file pins. These
// digests cover the portions outside the explicitly changed layout spans.
const editorBaseline = 'f724a59dfa093918b4ac8f00d234f50ab5f51ad95621afea5bc39037fb81da6f';
const previewBaseline = 'deb39e48780735d19718221e0ef18b933c130092e3833ba282318f5987c5b160';

function position(text, marker) {
  const at = text.indexOf(marker);
  assert(at >= 0 && text.lastIndexOf(marker) === at, `Expected one source boundary ${marker}`);
  return at;
}

function omitBetween(text, start, end, keepStart = true) {
  const from = position(text, start) + (keepStart ? start.length : 0);
  const to = position(text, end);
  assert(to >= from, 'Source boundaries out of order');
  return text.slice(0, from) + text.slice(to);
}

function projectPinnedChange(text, start, end, approvedDigest, original) {
  const from = position(text, start), to = position(text, end);
  assert(to > from);
  assert.equal(createHash('sha256').update(text.slice(from, to)).digest('hex'), approvedDigest, start);
  return text.slice(0, from) + original + text.slice(to);
}

// The approved UI work moved the toolbar, changed selection gestures and
// added sidebar statistics. Keep the source around those exact spans pinned.
function editorOutsideLayout(text) {
  let source = normalize(text);
  const spans = [
    ['let cmCompartments = null;\n', 'let cmReusedState = false;\n'],
    ['if (u.selectionSet && !u.docChanged) scheduleDocStatistics();\n', '        if (u.geometryChanged && !slash.open)'],
    ['  syncSearchMode();\n  applyEditorViewClasses();\n', "  cmView.scrollDOM.addEventListener('scroll', () => {"],
    ['  observeEditorLayout(cmView);\n  return true;\n\n}\n', '/* vendor 包只导出 historyKeymap'],
    ['function hideCmSelectionToolbar() {', '/* Selection toolbar buttons', false],
    ["function updateCmSelectionToolbar() {\n  const toolbar = $('cm-selection-toolbar');\n  if (!toolbar) return;\n", '  const text = cmView.state.sliceDoc(sel.from, sel.to).trim();'],
    ['function destroyEditor() {\n\n', '  if (cmLayoutCleanup) { cmLayoutCleanup(); cmLayoutCleanup = null; }\n'],
    ['/* Typewriter mode: the cursor line stays at the vertical centre. */\n', 'function applyEditorViewClasses() {'],
    ["  cmView.dispatch({ changes: e.changes, selection: e.selection, scrollIntoView: true, userEvent: 'input.syntax' });\n  cmView.focus();\n}\n", '/* ============================================================\n   Editor Studio PRO: Code Chunk'],
    ['  const stats = T && T.textStats ? T.textStats(visibleText) : { words: 0, chars: visibleText.length, minutes: 0 };\n', '  const nf = n =>'],
    ['  const params = { words: nf(stats.words), chars: nf(stats.chars), min: nf(Math.max(stats.minutes, stats.words ? 1 : 0)), total: nf(stats.words) };\n', '  statsEl.textContent = selWords'],
  ];
  for (const span of spans) source = omitBetween(source, ...span);
  // The already shipped merge/keyboard work (a52dc432, 662bbfa7) added two
  // changes outside the literal-cell markers. Pin those exact changes too,
  // then restore their pre-merge text before checking the original digest.
  source = projectPinnedChange(source, 'function clipboardTableCell(text) {', '/* 智能 Excel / CSV 粘贴转 Markdown 表格 */',
    '4ef10d45e79cb1dcf8e0420a8a2db99a05b7de6aec1d86022cf20c9f9421a307', [
      'function clipboardTableCell(text) {',
      "  // Office cells are text, not Markdown/HTML. Entities also avoid GFM's",
      '  // backslash/pipe parity trap and survive subsequent table Tab/Enter edits.',
      "  return text.replace(/[&<>\\t\\\\|`*_[\\]~$:@]/g, ch => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[ch] || '&#' + ch.charCodeAt(0) + ';'))",
      "    .replace(/\\n/g, '<br>')",
      "    .replace(/^ +| +$/g, spaces => '&#32;'.repeat(spaces.length));",
      '}', '', '',
    ].join('\n'));
  source = projectPinnedChange(source, 'function cmAttributedTableRows(', '/* Enter at the end of an unclosed opening fence',
    'aed39c2d95ea7ef2a441a61fa8a20c9ca196fd3656150e0062979f596b8c2563', `/* Tab / Shift+Tab inside a GFM table: realign pipes and jump between cells. */
function cmTableTab(view, dir) {
  const T = window.ReadMDTransforms;
  const hit = cmSingleCursorLine(view);
  if (!T || !hit || cmInCode(view.state, hit.range.head)) return false;
  return cmDispatchEdit(view, T.tableNavEdit(view.state.doc.toString(), hit.range.head, dir), 'input.table');
}

/* Enter inside a table adds a row; Enter on an empty last row leaves it. */
function cmTableEnter(view) {
  const T = window.ReadMDTransforms;
  const hit = cmSingleCursorLine(view);
  if (!T || !hit || !hit.range.empty || cmInCode(view.state, hit.range.head)) return false;
  return cmDispatchEdit(view, T.tableEnterEdit(view.state.doc.toString(), hit.range.head), 'input.table');
}

`);
  const moved = '\n/* ============================================================\n   Edit bar: view menu';
  if (source.includes(moved)) {
    const from = position(source, moved);
    assert(source.indexOf('\nfunction bindEditBarExtras(', from) > from, 'Original toolbar section');
    source = source.slice(0, from);
  }
  return canonicalBytes(source);
}

function previewOutsideLayout(text) {
  let source = normalize(text);
  const mathStart = '  // READMD_MATH_CONTEXT_BEGIN', mathEnd = '  // READMD_MATH_CONTEXT_END\n';
  const from = position(source, mathStart), to = position(source, mathEnd);
  assert(to > from, 'Math context markers out of order');
  source = source.slice(0, from) + source.slice(to + mathEnd.length);
  const publication = '  if(window.ReadMDPublication)ReadMDPublication.applyLayout(pane);\n';
  const tableSizing = '  window.ReadMDTableView?.observe(pane);\n';
  position(source, publication + tableSizing);
  source = source.replace(publication + tableSizing, publication);
  source = omitBetween(source, 'function setPvLayout(layout) {', 'function applyPvLayout(', false);
  source = omitBetween(source, "function applyPvLayout({ persist = true, updatePanels = true } = {}) {\n  const layout = state.pvLayout || 'none';\n", "  const mc = $('main-col');\n\n  const pw = $('preview-wrap');");
  source = omitBetween(source, 'window.alignEditorAndPreview = alignEditorAndPreview;\n', 'async function toggleEdit() {');
  return canonicalBytes(source);
}

module.exports = { editorOutsideLayout, previewOutsideLayout, editorBaseline, previewBaseline };
