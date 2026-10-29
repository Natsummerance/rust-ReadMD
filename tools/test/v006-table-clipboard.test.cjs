'use strict';
// node --test tools/test/v006-table-clipboard.test.cjs
const test = require('node:test');
const assert = require('node:assert/strict');
const { host, tableCells } = require('./v006-table-clipboard-host.cjs');

test('TSV quoted tabs, doubled quotes, CRLF multiline cells and edge empty cells survive the real paste route', () => {
  const f = host();
  const text = '\tName\tNote\t\r\n001\t"two\tparts"\t"line 1\r\nline 2"\t\r\n002\t"He said ""yes"""\t"\\| <b> &amp; **x**"\t\r\n\t\t\t\r\n';
  const pasted = f.paste(text);
  assert.equal(pasted.handled, true);
  assert.deepEqual(tableCells(f.html()), [
    ['', 'Name', 'Note', ''], ['001', 'two\tparts', 'line 1\nline 2', ''],
    ['002', 'He said "yes"', '\\| <b> &amp; **x**', ''], ['', '', '', '']
  ]);
  assert.deepEqual(f.toasts, ['toast.tableConverted:4']);
});

test('TSV preserves boundary spaces, ragged cells and genuine trailing empty records without inventing one for final CRLF', () => {
  for (const eol of ['\r\n', '\r', '\n']) {
    const f = host();
    assert.equal(f.paste(['  Head ', 'Edge '].join('\t') + eol + ' value \t' + eol + eol).handled, true);
    assert.deepEqual(tableCells(f.html()), [['  Head ', 'Edge '], [' value ', ''], ['', '']]);
  }
  const f = host();
  assert.equal(f.paste('H1\tH2\nA\tB\tC\nX\n').handled, true);
  assert.deepEqual(tableCells(f.html()), [['H1', 'H2', ''], ['A', 'B', 'C'], ['X', '', '']]);
});

test('explicit text/csv handles commas, BOM, quoted newlines and empty final cells', () => {
  const f = host();
  assert.equal(f.paste('Plain fallback', { 'text/csv': '\uFEFFName,Note,Extra\r\n"One, two","He said ""yes""\r\nnext",\r\n' }).handled, true);
  assert.deepEqual(tableCells(f.html()), [['Name', 'Note', 'Extra'], ['One, two', 'He said "yes"\nnext', '']]);
  const tsv = host();
  assert.equal(tsv.paste('', { 'text/tab-separated-values': 'A\tB\n1\t2\n' }).handled, true);
  assert.deepEqual(tableCells(tsv.html()), [['A', 'B'], ['1', '2']]);
});

test('ambiguous comma prose and malformed or non-table data retain ordinary paste', () => {
  const cases = [
    ['Hello, world\nGood, day'], ['A,B\n1,2'], ['A\tB'], ['One\nTwo'], [''],
    ['A\tB\n"broken\tvalue'], ['A\tB\n"quoted"extra\tvalue'],
    ['', { 'text/csv': 'A,B\n1' }], ['', { 'text/csv': 'A,B\n1,2,3' }],
    ['', { 'text/csv': 'A,B\n"broken,2' }], ['', { 'text/csv': 'A,B\nno"quote,2' }]
  ];
  for (const args of cases) {
    const f = host({ source: 'Owned source' });
    const result = f.paste(...args);
    assert.equal(result.handled, false, JSON.stringify(args));
    assert.equal(result.event.defaultPrevented, false);
    assert.equal(f.text(), 'Owned source');
    assert.equal(f.view.transactions.length, 0);
  }
});

test('office cell text stays literal through marked and existing Tab / Enter table editing', () => {
  const f = host();
  const literal = ' \\| **bold** _word_ ~~gone~~ `code` [link](https://example.invalid) <img src=x> &amp; $x$ =SUM(A1:A2) 🧪 ';
  assert.equal(f.paste('Value\tOther\n"' + literal + '"\tEnd\n').handled, true);
  const expected = [['Value', 'Other'], [literal, 'End']];
  assert.deepEqual(tableCells(f.html()), expected);
  assert.doesNotMatch(f.html(), /<(?:img|strong|em|del|code|a)\b/);
  f.setSelection(f.text().indexOf('Value'));
  assert.equal(f.key('Tab'), true);
  assert.deepEqual(tableCells(f.html()), expected);
  f.setSelection(f.view.state.selection.main.head);
  assert.equal(f.key('Enter'), true);
  assert.deepEqual(tableCells(f.html()), [expected[0], ['', ''], expected[1]]);
});

test('code destinations, multiple selections and read-only collaboration do not consume table paste', () => {
  for (const source of ['```csv\nHERE\n```', '    HERE', 'Use `HERE` here', '<script>\nHERE\n</script>']) {
    const at = source.indexOf('HERE') + 1;
    const f = host({ source, selection: { anchor: at } });
    assert.equal(f.paste('A\tB\n1\t2').handled, false, source);
    assert.equal(f.text(), source);
  }
  const multi = host({ source: 'a\nb' });
  multi.view.dispatch({ selection: multi.CM.EditorSelection.create([multi.CM.EditorSelection.cursor(1), multi.CM.EditorSelection.cursor(3)]) });
  assert.equal(multi.paste('A\tB\n1\t2').handled, false);
  assert.equal(multi.text(), 'a\nb');
  const readonly = host({ source: 'Owned' });
  readonly.context.ReadMDCollaborationShare = { canEdit: () => false };
  assert.equal(readonly.paste('A\tB\n1\t2').handled, false);
  assert.equal(readonly.text(), 'Owned');
});

test('selection replacement is one input.paste transaction and one undo preserves previous typing', () => {
  const f = host({ source: 'Intro\n\nreplace\n\nAfter', selection: { anchor: 7, head: 14 } });
  f.view.dispatch({ changes: { from: 0, insert: 'Typed ' }, userEvent: 'input.type' });
  const before = f.text(), beforeSelection = f.view.state.selection.main;
  const count = f.view.transactions.length;
  assert.equal(f.paste('A\tB\n1\t2').handled, true);
  assert.equal(f.view.transactions.length, count + 1);
  assert.equal(f.view.transactions.at(-1).isUserEvent('input.paste'), true);
  assert.match(f.text(), /^Typed Intro/);
  assert.match(f.text(), /After$/);
  const after = f.text();
  f.undo();
  assert.equal(f.text(), before);
  assert.equal(f.view.state.selection.main.from, beforeSelection.from);
  assert.equal(f.view.state.selection.main.to, beforeSelection.to);
  f.redo();
  assert.equal(f.text(), after);
});

test('normal saveEdit includes converted source, retains CRLF, and keeps paste history after save acknowledgement', async () => {
  const original = '# Owned\r\n\r\n';
  const f = host({ source: original });
  assert.equal(f.paste('A\tB\r\n"multi\r\nline"\t\\|\r\n').handled, true);
  const beforeSave = f.context.getEditContent();
  assert.equal(/(^|[^\r])\n/.test(beforeSave), false);
  assert.equal(await f.save(), true);
  assert.equal(f.calls.length, 1);
  assert.equal(f.calls[0].route, '/api/save');
  assert.equal(f.calls[0].body.content, beforeSave);
  assert.equal(f.tab.original, beforeSave);
  assert.equal(f.tab.isDirty, false);
  const reopened = host({ source: f.tab.original });
  assert.deepEqual(tableCells(reopened.html()), [['A', 'B'], ['multi\nline', '\\|']]);
  f.undo();
  assert.equal(f.context.getEditContent(), original);
  f.redo();
  assert.equal(f.context.getEditContent(), beforeSave);
});

test('textarea fallback receives the real delegated paste event and failed insertion leaves native paste available', () => {
  const f = host({ source: '' });
  f.fallback();
  let insertions = 0;
  f.context.document.execCommand = (command, _, value) => {
    assert.equal(command, 'insertText'); insertions++; f.area.value = value; return true;
  };
  const event = { target: f.area, clipboardData: { items: [], files: [], getData: type => type === 'text/plain' ? 'A\tB\n1\t2' : '' }, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; } };
  for (const listener of f.listeners.get('paste') || []) listener(event);
  assert.equal(insertions, 1);
  assert.equal(event.defaultPrevented, true);
  assert.deepEqual(tableCells(f.context.marked.parse(f.area.value, { gfm: true })), [['A', 'B'], ['1', '2']]);
  f.context.document.execCommand = () => false;
  const refused = { ...event, defaultPrevented: false };
  for (const listener of f.listeners.get('paste') || []) listener(refused);
  assert.equal(refused.defaultPrevented, false);
});

const budgetCases = [
  ['ASCII input above 1 MiB', () => 'A\tB\n' + 'x'.repeat(1024 * 1024) + '\ty'],
  ['UTF-8 bytes above 1 MiB with UTF-16 length below the limit', () => 'A\tB\n' + '界'.repeat(350000) + '\ty'],
  ['1001 logical rows', () => Array(1001).fill('A\tB').join('\n')],
  ['101 columns', () => Array(101).fill('H').join('\t') + '\n' + Array(101).fill('X').join('\t')],
  ['20100 cells after padding short rows', () => Array(100).fill('H').join('\t') + '\n' + Array(200).fill('X').join('\n')]
];
for (const [name, fixture] of budgetCases) {
  test('conversion budget leaves native paste available for ' + name, () => {
    const text = fixture();
    const f = host({ source: 'Budget source' });
    const result = f.paste(text);
    assert.equal(result.handled, false, `UTF-16 length ${text.length}`);
    assert.equal(result.event.defaultPrevented, false);
    assert.equal(f.text(), 'Budget source');
    assert.equal(f.view.transactions.length, 0);
    assert.equal(f.toasts.length, 0);
  });
}

test('conversion accepts exactly 1000 rows / 20000 cells, 100 columns and 1 MiB of input', () => {
  const exact = host();
  assert.equal(exact.paste(Array(1000).fill(Array(20).fill('x').join('\t')).join('\n')).handled, true);
  assert.equal(tableCells(exact.html()).length, 1000);
  const columns = host();
  assert.equal(columns.paste(Array(100).fill('H').join('\t') + '\n' + Array(100).fill('X').join('\t')).handled, true);
  const bytes = host();
  assert.equal(bytes.paste('A\tB\n' + 'x'.repeat(1024 * 1024 - 6) + '\ty').handled, true);
});

test('image clipboard takes precedence over table budgets and stale async images cannot change a different active document', async () => {
  for (const stale of [false, true]) {
    const pending = [], staged = [], inserted = [];
    const f = host({ source: 'Image target', apiFetch: async () => ({ ok: true, json: async () => ({ ok: true, rel: 'stage/image.png', asset: { path: 'stage/image.png' } }) }) });
    f.context.FileReader = class {
      readAsDataURL() { this.result = 'data:image/png;base64,SYNTHETIC'; pending.push(() => this.onload()); }
    };
    f.context.registerStagedImage = (asset, tab) => staged.push({ asset, tab });
    f.context.cmInsertImage = value => inserted.push(value);
    const event = { clipboardData: { items: [{ type: 'image/png', getAsFile: () => ({ type: 'image/png' }) }], files: [], getData: () => 'A\tB\n' + 'x'.repeat(1024 * 1024) },
      defaultPrevented: false, preventDefault() { this.defaultPrevented = true; } };
    assert.equal(f.context.cmHandlePaste(event, f.view), true);
    assert.equal(pending.length, 1);
    if (stale) f.context.getActiveTab = () => ({ id: 'different-document' });
    await pending[0]();
    assert.equal(f.calls[0].route, '/api/image/save');
    assert.equal(f.calls[0].body.stage, true);
    assert.equal(staged.length, stale ? 0 : 1);
    assert.equal(inserted.length, stale ? 0 : 1);
    assert.equal(f.text(), 'Image target');
  }
});
