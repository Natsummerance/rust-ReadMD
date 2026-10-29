'use strict';
// Real checked-in CM state/syntax/history and production editor functions.
// Geometry, modal DOM and HTTP save are controlled hosts, not browser evidence.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path');
const { ROOT, host, tableCells } = require('./v006-table-clipboard-host.cjs');
const SOURCE = '# Before\n\n| **Name** | Note | Tail |\n| :---- | :---: | ----: |\n| Alpha | left\\|right \\\\ end <br> next | 尾 |\n| Beta | [link](https://example.invalid/a) | 0002 |\n\nAfter.\n';

function fixture(source = SOURCE, marker = 'left\\|right', offset = 1) {
  const f = host({ source }), CM = f.CM;
  f.view.state = CM.EditorState.create({ doc: source, selection: { anchor: source.replace(/\r\n?/g, '\n').indexOf(marker) + offset },
    extensions: [CM.history(), CM.markdown({ base: CM.markdownLanguage }), CM.EditorState.allowMultipleSelections.of(true)] });
  f.align = alignment => f.context.cmAlignTableColumn(f.view, alignment);
  f.hit = () => f.context.cmTableAlignmentContext(f.view);
  return f;
}

test('selected column alignment changes only its delimiter token and preserves raw source and cell content', () => {
  const f = fixture(), beforeCells = tableCells(f.html());
  assert.equal(f.hit().column, 1); assert.equal(f.hit().alignment, 'center'); assert.equal(f.align('right'), true);
  assert.equal(f.text(), SOURCE.replace('| :---- | :---: | ----: |', '| :---- | ---: | ----: |'));
  assert.deepEqual(tableCells(f.html()), beforeCells); assert.equal(f.view.transactions.filter(transaction => transaction.docChanged).length, 1);
  assert.equal(f.view.transactions.at(-1).isUserEvent('input.table.align'), true); assert.equal(f.hit().alignment, 'right');
  f.undo(); assert.equal(f.text(), SOURCE); f.redo(); assert.equal(f.hit().alignment, 'right');
});

test('successive alignments are separate undo steps and a repeated alignment is a no-op', () => {
  const f = fixture(); f.align('right'); const right = f.text(); f.align('left');
  const count = f.view.transactions.length; assert.equal(f.align('left'), false); assert.equal(f.view.transactions.length, count);
  assert.equal(f.align('not-an-alignment'), false); f.undo(); assert.equal(f.text(), right);
  f.undo(); assert.equal(f.text(), SOURCE);
});

test('alignment is not merged with immediately preceding normal typing', () => {
  const f = fixture(), position = f.view.state.selection.main.head;
  f.view.dispatch({ changes: { from: position, insert: 'Q' }, selection: { anchor: position + 1 }, userEvent: 'input.type' });
  const typed = f.text(); f.align('right'); f.undo(); assert.equal(f.text(), typed); f.undo(); assert.equal(f.text(), SOURCE);
});

test('a selected formatted cell retains its exact selection after a preceding delimiter changes width', () => {
  const f = fixture(SOURCE, 'left\\|right', 0), start = f.text().indexOf('left\\|right'); f.setSelection(start, start + 'left\\|right'.length);
  const selected = f.view.state.sliceDoc(f.view.state.selection.main.from, f.view.state.selection.main.to); assert.equal(f.align('right'), true);
  assert.equal(f.view.state.sliceDoc(f.view.state.selection.main.from, f.view.state.selection.main.to), selected);
});

for (const [name, source, marker, column] of [
  ['no outer pipes', 'First | Second\n--- | ---\nValue | Target\n', 'Target', 1],
  ['empty cell', '| A | B |\n| --- | --- |\n| X |    |\n', '    |', 1],
  ['odd escaped pipe', '| A | B |\n| --- | --- |\n| left\\|right | Target |\n', 'Target', 1],
  ['even backslashes before a real delimiter', '| A | B |\n| --- | --- |\n| slash\\\\|Target |\n', 'Target', 1],
  ['Unicode and header formatting', '| **标题** | Note |\n| --- | --- |\n| 中文 | Target |\n', '**标题**', 0]
]) test('GFM cell boundaries: ' + name, () => {
  const f = fixture(source, marker); assert.equal(f.hit().column, column); assert.equal(f.align('center'), true);
  assert.equal(f.hit().alignment, 'center'); assert.deepEqual(tableCells(f.html()), tableCells(f.context.marked.parse(source, { gfm: true })));
});

for (const [name, source, marker] of [
  ['fenced code', '```md\n' + SOURCE + '```\n', 'left\\|right'],
  ['unclosed fence', '```md\n' + SOURCE, 'left\\|right'],
  ['indented code', SOURCE.split('\n').map(line => '    ' + line).join('\n'), 'left\\|right'],
  ['inline code in a real cell', SOURCE.replace('left\\|right', '`code`'), 'code'],
  ['HTML comment', '<!--\n' + SOURCE + '-->\n', 'left\\|right'],
  ['blockquote table', SOURCE.split('\n').map(line => '> ' + line).join('\n'), 'left\\|right'],
  ['plain pipe text', 'One | Two\nAlpha | Target\n', 'Target'],
  ['delimiter row', SOURCE, ':---:']
]) test('unsafe/non-cell context is inert: ' + name, () => {
  const f = fixture(source, marker), before = f.text(); assert.equal(f.hit(), null); assert.equal(f.align('right'), false); assert.equal(f.text(), before);
});

test('cross-cell selections and multiple selections are inert', () => {
  const f = fixture(), at = f.text().indexOf('left\\|right'); f.setSelection(at, f.text().indexOf('尾') + 1);
  assert.equal(f.hit(), null); assert.equal(f.align('right'), false); assert.equal(f.text(), SOURCE);
  f.view.dispatch({ selection: f.CM.EditorSelection.create([f.CM.EditorSelection.cursor(at), f.CM.EditorSelection.cursor(f.text().indexOf('Beta'))]) });
  assert.equal(f.hit(), null); assert.equal(f.align('right'), false);
});

test('read-only collaboration, read-only CM and an inactive view cannot be edited', () => {
  const f = fixture(); f.context.ReadMDCollaborationShare = { canEdit: () => false }; assert.equal(f.hit(), null); assert.equal(f.align('right'), false);
  delete f.context.ReadMDCollaborationShare;
  f.view.dispatch({ effects: f.CM.StateEffect.appendConfig.of(f.CM.EditorState.readOnly.of(true)) }); assert.equal(f.hit(), null); assert.equal(f.align('right'), false);
  const other = fixture(); assert.equal(f.context.cmAlignTableColumn(other.view, 'right'), false); assert.equal(other.text(), SOURCE);
});

test('bounded context rejects overlong lines, more than 100 columns and more than 1000 table rows', () => {
  for (const [source, marker] of [
    ['| A | B |\n| --- | --- |\n| ' + 'X'.repeat(65537) + ' | Target |\n', 'Target'],
    ['| ' + Array(101).fill('Head').join(' | ') + ' |\n| ' + Array(101).fill('---').join(' | ') + ' |\n| ' + Array(101).fill('Target').join(' | ') + ' |\n', 'Target'],
    ['| A | B |\n| --- | --- |\n' + Array(999).fill('| Target | Value |').join('\n') + '\n', 'Target']
  ]) { const f = fixture(source, marker), before = f.text(); assert.equal(f.hit(), null); assert.equal(f.align('right'), false); assert.equal(f.text(), before); }
});

function modalHost(f) {
  const nodes = new Map();
  function node(tag = 'div', id = '') {
    const classes = new Set(), value = { tag, id, children: [], dataset: {}, attributes: {}, listeners: {}, textContent: '',
      classList: { add: name => classes.add(name), remove: name => classes.delete(name), contains: name => classes.has(name), toggle(name, force) { if (force ?? !classes.has(name)) classes.add(name); else classes.delete(name); } },
      setAttribute(name, value) { this.attributes[name] = value; }, getAttribute(name) { return this.attributes[name] ?? null; },
      append(...items) { for (const item of items) { this.children.push(item); item.parentElement = this; } },
      before(item) { const siblings = this.parentElement.children; siblings.splice(siblings.indexOf(this), 0, item); item.parentElement = this.parentElement; },
      remove() { if (this.parentElement) this.parentElement.children = this.parentElement.children.filter(item => item !== this); nodes.delete(this.id); },
      addEventListener(name, callback) { this.listeners[name] = callback; }, focus() {}, querySelectorAll() { return this.children; }
    };
    Object.defineProperty(value, 'innerHTML', { set() { this.children = []; } });
    if (id) nodes.set(id, value); return value;
  }
  const modal = node('div', 'table-modal'), picker = node('div', 'table-grid-picker'), label = node('div', 'table-grid-label'), body = node(); body.append(picker, label); modal.append(body); modal.classList.add('hidden');
  f.context.$ = id => nodes.get(id) || [...body.children].find(item => item.id === id) || null;
  f.context.document.createElement = tag => node(tag); f.context.ReadMDModal = { close: element => element.classList.add('hidden') };
  return { modal, picker, body, label, controls: () => body.children.find(item => item.id === 'table-align-controls') };
}

test('existing table modal exposes localized pressed-state buttons and one real controller operation', () => {
  const f = fixture(), ui = modalHost(f); f.context.openTableModal(); const group = ui.controls(); assert(group);
  assert.equal(group.getAttribute('role'), 'group'); assert.equal(group.getAttribute('aria-label'), 'dialog.tableAlign');
  assert.equal(ui.picker.classList.contains('hidden'), true); assert.equal(group.children.length, 3);
  const center = group.children.find(button => button.dataset.tableAlign === 'center'), right = group.children.find(button => button.dataset.tableAlign === 'right');
  assert.equal(center.getAttribute('aria-pressed'), 'true'); assert.equal(right.textContent, 'dialog.tableAlignRight'); right.listeners.click();
  assert.equal(f.hit().alignment, 'right'); assert.equal(ui.modal.classList.contains('hidden'), true);
});

test('an old dialog cannot edit a changed draft, changed tab or newly read-only collaboration', () => {
  for (const change of [f => f.view.dispatch({ selection: { anchor: 0 } }), f => f.view.dispatch({ changes: { from: 0, insert: 'Changed draft\n' } }), f => { f.context.getActiveTab = () => ({ id: 'new-tab' }); }, f => { f.context.ReadMDCollaborationShare = { canEdit: () => false }; }]) {
    const f = fixture(), ui = modalHost(f); f.context.openTableModal(); const button = ui.controls().children.find(button => button.dataset.tableAlign === 'right');
    change(f); const before = f.text(); button.listeners.click(); assert.equal(f.text(), before); assert.equal(ui.modal.classList.contains('hidden'), true);
  }
});

test('non-document CM effects do not cancel an unchanged cell selection in the open dialog', () => {
  const f = fixture(), ui = modalHost(f); f.context.openTableModal(); const snapshot = f.view.state;
  f.view.dispatch({ effects: f.CM.StateEffect.appendConfig.of([]) });
  assert.notEqual(f.view.state, snapshot); assert.equal(f.view.state.doc, snapshot.doc); assert(f.view.state.selection.eq(snapshot.selection));
  ui.controls().children.find(button => button.dataset.tableAlign === 'right').listeners.click();
  assert.equal(f.hit().alignment, 'right'); assert.equal(ui.modal.classList.contains('hidden'), true);
});

test('existing alignment labels exist in every supported locale without adding new keys', () => {
  const directory = path.join(ROOT, 'assets/i18n'), names = fs.readdirSync(directory).filter(name => name.endsWith('.json') && name !== 'meta.json'); assert.equal(names.length, 46);
  for (const name of names) {
    const value = JSON.parse(fs.readFileSync(path.join(directory, name), 'utf8'));
    for (const key of ['dialog.tableAlign', 'dialog.tableAlignLeft', 'dialog.tableAlignCenter', 'dialog.tableAlignRight', 'formula.column']) assert.equal(typeof value[key], 'string', name + ': ' + key);
  }
});
