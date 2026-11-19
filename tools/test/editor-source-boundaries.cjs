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
  // 362494b6 added a cursor/revision-bound clipboard image destination.
  // Verify the complete new helper and image-only branch before projecting
  // their exact pre-change text. Do not fold this into the table-cell span.
  source = projectPinnedChange(source,
    '// Freeze the destination before any file picker, image decode or clipboard I/O.\n',
    '/* 智能 Excel / CSV 粘贴转 Markdown 表格 */',
    'a47d7d2fe210705c785e0746c0740d304ae86809a113989d053ac4c3669fe3bf', '');
  const imageStart = '  if (imageFile) {\n';
  const imageEnd = '  if (!state.editing || window.ReadMDCollaborationShare?.canEdit() === false) return;\n';
  let originalPaste = source.slice(position(source, imageStart), position(source, imageEnd));
  const imageEdits = [
    ["    const target = imageEditTarget();\n    if (!target) return;\n    e.preventDefault();\n    const format = { 'image/png':'png', 'image/jpeg':'jpg', 'image/gif':'gif', 'image/webp':'webp',\n      'image/bmp':'bmp', 'image/tiff':'tiff', 'image/svg+xml':'svg', 'image/jp2':'jp2' }[imageFile.type.toLowerCase()];\n    if (!format) { showToast(window.i18n.t('error.unsupported_format')); return; }\n",
      '    e.preventDefault();\n    const pasteTab = getActiveTab(), pasteView = cmView, pasteContent = getEditContent();\n    const pasteTargetCurrent = () => getActiveTab() === pasteTab && cmView === pasteView && state.editing && getEditContent() === pasteContent;\n'],
    ['            format,\n', "            format: 'png',\n"],
    ['        if (resp.ok && d && d.ok && target.current()) {\n', '        if (resp.ok && d && d.ok && pasteTargetCurrent()) {\n'],
    ['          registerStagedImage(d.asset, target.tab);\n', '          registerStagedImage(d.asset, pasteTab);\n'],
    ['          cmInsertImage(insertRel, target);\n', '          cmInsertImage(insertRel);\n'],
    ['        } else if (target.current()) {\n', '        } else if (pasteTargetCurrent()) {\n'],
  ];
  for (const [current, original] of imageEdits) {
    position(originalPaste, current);
    originalPaste = originalPaste.replace(current, original);
  }
  assert.equal(createHash('sha256').update(originalPaste).digest('hex'),
    '8f4a7dc80a42d6c6c55f9ae52ff0b28083a4052b995d78806619d37fbd01c139');
  source = projectPinnedChange(source, imageStart, imageEnd,
    '598b8aed5a99da6472988c3e00c86802a5c374b9df7ef60b74026124125fb1ee', originalPaste);
  // 362494b6/e5efbea4 route only these four new commands to their controllers.
  const syntaxSwitch = "function cmInsertSyntax(kind) {\n  if (!cmView) return;\n  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;\n  switch (kind) {\n";
  const creationCommands = "    case 'ink': return window.ReadMDInk.open();\n    case 'chapter-select': return ReadMDManuscript.select();\n    case 'chapter-up': return ReadMDManuscript.move('up');\n    case 'chapter-down': return ReadMDManuscript.move('down');\n";
  position(source, syntaxSwitch + creationCommands);
  source = source.replace(syntaxSwitch + creationCommands, syntaxSwitch);
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
  // 179380db explicitly rejects Office source tabs from Markdown Save As.
  // The protected pipeline underneath both guards retains its original pin.
  source = projectPinnedChange(source, 'async function saveAs(contentOverride = null) {\n',
    '  if(state.editing&&window.ReadMDInlineMath){\n',
    'f7c17671425f75b3c7586c914f88f7dbbd9e227fe4f920b32a868e135825cae2',
    'async function saveAs(contentOverride = null) {\n');
  source = projectPinnedChange(source, 'async function saveAsSnapshot(snapshot, options = {}) {\n',
    '  if (!snapshot.reviewPrepared && window.ReadMDReview) {\n',
    'abbce4411f52d115a84a316e34043c6a3543939b69926620ddf91e7b33611fb1',
    'async function saveAsSnapshot(snapshot, options = {}) {\n');
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

// ecf7f9e2 added batch output settings and export actions, efb3a7ad appended
// extraction receipts, and 3c51fc55 added the native PDF choice control.
// Only those exact committed additions are projected out. Preserve the raw
// bytes of the original CSS, including its existing mixed line endings.
function panelsBeforeDeliveryChanges(bytes) {
  let source = bytes.toString('utf8');
  const spans = [
    ['.convert-output-settings {', '#convert-box .panel-options-body { grid-template-columns: repeat(2, minmax(0, 1fr)); align-items: center; }', '0eff0551fa9acc09c673f61b943e4ac0d57580f63e84a9b645d67d690f2025ce'],
    ['.batch-output-name {', '.batch-state { white-space: normal; overflow-wrap: anywhere; }', '596f791cf360bab46aacfac49edddf3bbe91aae56c89b78b9fefa68d2ea5914b'],
    ['.pdf-context>select{', '.pdf-context-actions{display:flex;align-items:center;gap:8px;flex-wrap:wrap}', '772533bd8f66a286437c11f64207ec51ea6cec97dbcc5273032f80a032f6c2b9'],
  ];
  for (const [start, end, digest] of spans) {
    const from = position(source, start), to = position(source, end);
    assert(to > from);
    assert.equal(createHash('sha256').update(normalize(source.slice(from, to))).digest('hex'), digest, start);
    source = source.slice(0, from) + source.slice(to);
  }
  const refactorEnd = '/* READMD_DOCUMENT_REFACTOR_END */';
  const at = position(source, refactorEnd) + refactorEnd.length;
  const ending = /^(?:\r\n|\n)/.exec(source.slice(at));
  assert(ending, 'Document refactor suffix newline');
  const stop = at + ending[0].length;
  assert.equal(createHash('sha256').update(normalize(source.slice(stop))).digest('hex'),
    '5b2945f0006a3a0867750115c9f3593f8b8097b6140889240c5ffcdd76b891bb', 'Export and extraction receipt suffix');
  return Buffer.from(source.slice(0, stop));
}

module.exports = { editorOutsideLayout, previewOutsideLayout, panelsBeforeDeliveryChanges, editorBaseline, previewBaseline };
