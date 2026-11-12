'use strict';

const assert = require('node:assert/strict');
const normalize = text => text.replace(/\r\n?/g, '\n');
const canonicalBytes = text => Buffer.from(text.replaceAll('\n', '\r\n'));
// Derived from c5f8d80d after verifying its original full-file pins. These
// digests cover the portions outside the explicitly changed layout spans.
const editorBaseline = '067e5a8fda96e31577ec4832bc1f2505faac5fb2dc161aff03461431f452c08f';
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
    ["  cmView.dispatch({ changes: e.changes, selection: e.selection, scrollIntoView: true, userEvent: 'input.syntax' });\n  cmView.focus();\n}\n", '/* ============================================================\n   Editor Studio PRO: Code Chunk'],
    ['  const stats = T && T.textStats ? T.textStats(visibleText) : { words: 0, chars: visibleText.length, minutes: 0 };\n', '  const nf = n =>'],
    ['  const params = { words: nf(stats.words), chars: nf(stats.chars), min: nf(Math.max(stats.minutes, stats.words ? 1 : 0)), total: nf(stats.words) };\n', '  statsEl.textContent = selWords'],
  ];
  for (const span of spans) source = omitBetween(source, ...span);
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
  source = omitBetween(source, 'function setPvLayout(layout) {', 'function applyPvLayout(', false);
  source = omitBetween(source, "function applyPvLayout({ persist = true, updatePanels = true } = {}) {\n  const layout = state.pvLayout || 'none';\n", "  const mc = $('main-col');\n\n  const pw = $('preview-wrap');");
  source = omitBetween(source, 'window.alignEditorAndPreview = alignEditorAndPreview;\n', 'async function toggleEdit() {');
  return canonicalBytes(source);
}

module.exports = { editorOutsideLayout, previewOutsideLayout, editorBaseline, previewBaseline };
