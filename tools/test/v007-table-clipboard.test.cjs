'use strict';
// Controlled ClipboardEvents against the production editor/clipboard handlers.
// Native OS flavor transport is covered separately by the browser regression.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { service, app, modal } = require('./v007-table-cell-host.cjs');
const clipboard = require('../../assets/js/editor/table-clipboard.js');

const sourceTable = [
  '  | Key {width=120 height=44} | Note {width=160} | Tail |',
  '  | :--- | ---: | --- |',
  '  | **Alpha** {rowspan=2 colspan=2 align=right height=36} | [Beta](https://example.invalid) | O1 |',
  '  | `γ` {height=52} | left\\|right {valign=top} | O2 |',
  '  | Solo | Spare | O3 |',
].join('\n');
const destinationTable = [
  '  | Cover {width=200} | Column | Note |',
  '  | --- | :---: | ---: |',
  '  | R1 {height=30} | T1 | Keep1 |',
  '  | R2 {height=28} | T2 | Keep2 |',
  '  | R3 {height=27} | T3 | Keep3 |',
].join('\n');
const fixture = `# 😀 Clipboard\n\n${sourceTable}\n\n${destinationTable}\n\nAfter\n`;
const sourceAt = (s, token) => s.indexOf(token) + 1;
const buttonAt = (ui, row, column) => ui.buttons().find(button => +button.dataset.tableRow === row && +button.dataset.tableColumn === column);
const edits = f => f.calls.filter(call => call.body.action === 'edit');

function transfer(contents = {}) {
  const entries = new Map(Object.entries(contents));
  return { items: [], files: [], get types() { return [...entries.keys()]; },
    getData: type => entries.get(type) || '', setData: (type, value) => entries.set(type, value) };
}
function event(contents, button) {
  return { defaultPrevented: false, clipboardData: contents,
    target: button ? { closest: () => button } : null,
    preventDefault() { this.defaultPrevented = true; } };
}
async function until(check) {
  for (let i = 0; i < 100; i++) {
    if (check()) return;
    await new Promise(resolve => setImmediate(resolve));
  }
  throw Error('Clipboard action did not settle');
}
async function fixtureApp(source = fixture) {
  const f = app(service(Buffer.from(source)));
  await f.ready();
  return { f, ui: modal(f) };
}
function open(f, ui, at) {
  f.select(at);
  f.context.openTableModal();
  const controls = ui.get('table-cell-controls');
  assert.ok(controls && !ui.root.classList.contains('hidden'));
  return { grid: controls.children[0].children[0], status: controls.children.at(-1) };
}

test('raw, TSV, CSV and clipboard HTML expose one merged rectangle with the covered source slots', async () => {
  const { f, ui } = await fixtureApp();
  const { grid } = open(f, ui, sourceAt(fixture, 'Alpha'));
  const covered = buttonAt(ui, 1, 1);
  covered.click();
  const copy = event(transfer(), covered);
  grid.listeners.copy(copy);
  assert.equal(copy.defaultPrevented, true);
  const data = copy.clipboardData;
  assert.equal(data.getData('text/plain'), 'Alpha\tBeta\r\nγ\tleft|right');
  assert.equal(data.getData('text/tab-separated-values'), data.getData('text/plain'));
  assert.equal(data.getData('text/csv'), 'Alpha,Beta\r\nγ,left|right');
  const html = data.getData('text/html');
  assert.match(html, /<td rowspan="2" colspan="2" align="right" height="36">Alpha<br>Beta<br>γ<br>left\|right<\/td>/);
  assert.doesNotMatch(html, /<a\b|<script\b/);
  const complete = clipboard.read(data);
  assert.deepEqual(clipboard.read(transfer({ 'text/html': html, 'text/plain': data.getData('text/plain') })), complete);
  assert.deepEqual(clipboard.read(transfer({ [clipboard.mime]: '', 'text/html': html })), complete);
  assert.deepEqual(complete, { rich: true, columns: 2, rows: [
    ['**Alpha** {rowspan=2 colspan=2 align=right height=36}', '[Beta](https://example.invalid)'],
    ['`γ` {height=52}', 'left\\|right {valign=top}'],
  ] });
});

test('HTML-only ReadMD metadata round trips into another table in one sparse durable edit', async () => {
  const { f, ui } = await fixtureApp();
  const keepAt = fixture.indexOf('Keep1');
  await f.review.propose([{ from: keepAt, to: keepAt + 5, insert: 'Proposed across the range' }], await f.review.capture(f.tab), f.tab);
  const before = f.text(), outside = f.review.snapshot(f.tab).changes[0];
  let { grid } = open(f, ui, sourceAt(before, 'Solo'));
  const bottomRight = buttonAt(ui, 2, 1), topLeft = buttonAt(ui, 1, 0);
  bottomRight.click(); topLeft.click({ shiftKey: true });
  assert.equal(buttonAt(ui, 1, 1).dataset.tableInRange, '');
  assert.equal(buttonAt(ui, 1, 2).dataset.tableInRange, undefined);
  const copy = event(transfer(), topLeft);
  grid.listeners.copy(copy);
  assert.equal(copy.defaultPrevented, true);
  f.context.closeTableModal();
  ({ grid } = open(f, ui, sourceAt(before, 'R2 {height')));
  const destination = buttonAt(ui, 2, 0);
  destination.click();
  const paste = event(transfer({ 'text/html': copy.clipboardData.getData('text/html'), 'text/plain': copy.clipboardData.getData('text/plain') }), destination);
  grid.listeners.paste(paste);
  assert.equal(paste.defaultPrevented, true);
  await until(() => ui.root.classList.contains('hidden'));
  const after = before.replace('R2 {height=28} | T2', '**Alpha** {rowspan=2 colspan=2 align=right height=36} | [Beta](https://example.invalid)')
    .replace('R3 {height=27} | T3', '`γ` {height=52} | left\\|right {valign=top}');
  assert.equal(f.text(), after);
  assert.equal(f.view.transactions.filter(tx => tx.docChanged && tx.isUserEvent('input.paste.table')).length, 1);
  assert.equal(edits(f).length, 1);
  assert.equal(edits(f)[0].body.edits.length, 4);
  const change = f.review.snapshot(f.tab).changes[0];
  assert.equal(change.id, outside.id);
  assert.equal(change.replacement, outside.replacement);
  assert.equal(change.decision, 'pending');
  assert.equal(f.text().slice(change.from, change.to), change.replacement);
  await f.history(); assert.equal(f.text(), before);
  await f.history(true); assert.equal(f.text(), after);
});

test('TSV and explicitly typed CSV paste preserve merged target tracks, quoted delimiters, line breaks and empties', async () => {
  const cases = [
    { data: { 'text/plain': '"One\tTwo"\t"Line 1\r\nLine 2"\r\n"He said ""yes"""\t\r\n' }, first: 'One&#9;Two' },
    { data: { 'text/plain': 'Ignored fallback', 'text/csv': '\uFEFF"One,Two","Line 1\r\nLine 2"\r\n"He said ""yes""",\r\n' }, first: 'One,Two' },
  ];
  for (const { data, first } of cases) {
    const { f, ui } = await fixtureApp();
    const { grid } = open(f, ui, sourceAt(fixture, 'Alpha'));
    const target = buttonAt(ui, 1, 0);
    target.click();
    const paste = event(transfer(data), target);
    grid.listeners.paste(paste);
    assert.equal(paste.defaultPrevented, true);
    await until(() => ui.root.classList.contains('hidden'));
    assert.equal(f.text(), fixture.replace('**Alpha**', first).replace('[Beta](https://example.invalid)', 'Line 1<br>Line 2')
      .replace('`γ`', 'He said "yes"').replace('left\\|right {valign=top}', '{valign=top}'));
    assert.equal(edits(f).length, 1);
  }
});

test('partial copy, partial paste, shape mismatch and invalid source leave all text unchanged', async () => {
  const { f, ui } = await fixtureApp();
  const { grid, status } = open(f, ui, sourceAt(fixture, 'Alpha'));
  const anchor = buttonAt(ui, 1, 0), other = buttonAt(ui, 1, 1), edge = buttonAt(ui, 2, 2);
  anchor.click(); other.click({ shiftKey: true });
  const copy = event(transfer({ 'text/plain': 'Existing clipboard' }), other);
  grid.listeners.copy(copy);
  assert.equal(copy.clipboardData.getData('text/plain'), 'Existing clipboard');
  assert.equal(status.textContent, 'editor.tableCellInvalidRange');
  for (const [target, data] of [
    [other, { 'text/plain': '1\t2' }],
    [other, { 'text/plain': '1\t2\n3\t4' }],
    [edge, { 'text/plain': '1\t2\n3\t4' }],
    [anchor, { 'text/plain': '1\t2\nOnly one' }],
    [anchor, { 'text/html': '<table data-readmd-table="%GG"><tr><td>1</td></tr></table>', 'text/plain': '1\t2\n3\t4' }],
    [anchor, { [clipboard.mime]: JSON.stringify({ version: 1, header: false, firstColumn: true, rows: [['broken | cell', 'a'], ['b', 'c']] }) }],
  ]) {
    if (target !== other) target.click();
    const paste = event(transfer(data), target);
    grid.listeners.paste(paste);
    assert.equal(paste.defaultPrevented, true);
    assert.equal(status.textContent, 'editor.tableCellInvalidRange');
    assert.equal(f.text(), fixture);
    assert.equal(edits(f).length, 0);
  }
});

test('CodeMirror uses a cell caret as a table destination and refuses a raw range across table rows', async () => {
  const { f } = await fixtureApp();
  const { context, view } = f;
  const paste = event(transfer({ 'text/plain': 'one\ttwo\nthree\tfour' }));
  f.select(sourceAt(fixture, 'R1 {height'), sourceAt(fixture, 'R2 {height'));
  assert.equal(context.cmHandlePaste(paste, view), true);
  assert.equal(f.text(), fixture);
  assert.equal(f.toasts.at(-1), 'editor.tableCellInvalidRange');
  f.select(sourceAt(fixture, 'R1 {height'));
  assert.equal(context.cmHandlePaste(event(paste.clipboardData), view), true);
  await until(() => f.text() !== fixture);
  assert.equal(await f.review.flush(f.tab), true);
  assert.equal(f.text(), fixture.replace('R1 {height=30} | T1', 'one {height=30} | two').replace('R2 {height=28} | T2', 'three {height=28} | four'));
  const plain = event(transfer({ 'text/plain': 'comma, prose' }));
  assert.equal(context.cmHandlePaste(plain, view), false);
  assert.equal(plain.defaultPrevented, false);
});

test('unnamed table paste resumes the draft review before submitting its single edit', async () => {
  const draft = () => app(service(Buffer.from(fixture), { draft: true }));
  const paste = f => {
    f.select(sourceAt(fixture, 'R1 {height'));
    return f.context.cmHandlePaste(event(transfer({ 'text/plain': 'one\ttwo\nthree\tfour' })), f.view);
  };
  const f = draft();
  await f.ready();
  assert.equal(paste(f), true);
  await until(() => f.text() !== fixture);
  assert.equal(await f.review.flush(f.tab), true);
  assert.deepEqual(f.calls.filter(call => call.body.action === 'resume').map(call => call.body.path), ['draft:table-tab', 'draft:table-tab']);
  assert.equal(edits(f).length, 1);
  assert.equal(edits(f)[0].body.edits.length, 4);
  await f.history();
  assert.equal(f.text(), fixture);

  const stale = draft();
  await stale.ready();
  const gate = stale.store.pause('resume');
  assert.equal(paste(stale), true);
  await gate.entered.promise;
  stale.store.remoteHead(); gate.release.resolve();
  await until(() => stale.toasts.includes('review.conflict'));
  assert.equal(stale.text(), fixture);
  assert.equal(edits(stale).length, 0);
});

test('cancelled staged paste and an affected review do not change the source', async () => {
  const { f, ui } = await fixtureApp();
  const pasteData = transfer({ 'text/plain': 'A\tB\nC\tD' });
  let { grid } = open(f, ui, sourceAt(fixture, 'Alpha'));
  const gate = f.store.pause('resume');
  grid.listeners.paste(event(pasteData, buttonAt(ui, 1, 0)));
  await gate.entered.promise;
  ui.get('table-cell-cancel').click(); gate.release.resolve();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.text(), fixture); assert.equal(edits(f).length, 0);

  const inside = fixture.indexOf('Beta');
  await f.review.propose([{ from: inside, to: inside + 4, insert: 'Flagged' }], await f.review.capture(f.tab), f.tab);
  const before = f.text();
  const target = open(f, ui, sourceAt(before, 'Alpha'));
  grid = target.grid;
  grid.listeners.paste(event(pasteData, buttonAt(ui, 1, 0)));
  await until(() => target.status.textContent === 'review.conflict');
  assert.equal(f.text(), before); assert.equal(edits(f).length, 0);
});

test('global Paste leaves modal and field events native and still creates a document outside them', () => {
  const boot = fs.readFileSync(path.resolve(__dirname, '../../assets/app.js'), 'utf8');
  const from = boot.indexOf("  document.addEventListener('keydown', e => {", boot.indexOf('Global Keyboard Shortcuts'));
  const to = boot.indexOf('\n  });', from);
  assert.ok(from > 0 && to > from);
  let shortcuts, depth = 0, created = 0;
  vm.runInNewContext(boot.slice(from, to + '\n  });'.length), {
    document: { addEventListener(name, handler) { assert.equal(name, 'keydown'); shortcuts = handler; } },
    window: { ReadMDModal: { depth: () => depth } },
    createFromClipboard: () => created++,
  });
  for (const specimen of [
    { depth: 1, tag: 'BUTTON', prevented: false, expected: false, created: 0 },
    { depth: 0, tag: 'TEXTAREA', prevented: false, expected: false, created: 0 },
    { depth: 0, tag: 'BUTTON', prevented: true, expected: true, created: 0 },
    { depth: 0, tag: 'BUTTON', prevented: false, expected: true, created: 1 },
  ]) {
    depth = specimen.depth;
    const key = { metaKey: true, key: 'v', defaultPrevented: specimen.prevented, target: { tagName: specimen.tag },
      preventDefault() { this.defaultPrevented = true; } };
    shortcuts(key);
    assert.equal(key.defaultPrevented, specimen.expected);
    assert.equal(created, specimen.created);
  }
});
