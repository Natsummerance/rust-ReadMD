'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const path = require('node:path'), Module = require('node:module'), { createHash } = require('node:crypto');
const hash = value => createHash('sha256').update(value).digest('hex');
const ID = '1'.repeat(64), sourceRevision = 'a'.repeat(64);
function output(before, { fallback = false, replace, body = '## GET /items\n\nVISIBLE_ENDPOINT\n', revision = sourceRevision } = {}) {
  const metadata = { schema: 1, id: ID, source: 'api.yaml', source_sha256: revision, body_sha256: hash(body),
    operations: { '/paths/~1items/get': 'b'.repeat(64) }, references: [{ source: 'defs/item.yaml', revision: 'c'.repeat(64) }] };
  const insert = (replace ? '' : '\n\n') + '<!-- readmd:openapi:start ' + Buffer.from(JSON.stringify(metadata)).toString('base64url') + ' -->\n' + body + '<!-- readmd:openapi:end ' + ID + ' -->';
  const from = replace ? replace.from : before.length, to = replace ? replace.to : before.length;
  const transaction = { base_revision: hash(before), origin: 'openapi', edits: [{ from, to, insert }] };
  if (fallback) transaction.edits.push({ from: before.length, to: before.length, insert: '' });
  return { result: { content: before.slice(0, from) + insert + before.slice(to), changed: true }, source_revision: revision, changes: [{ pointer: '/paths/~1items/get', kind: 'added' }], transaction };
}
function fixture({ decisions = ['cancel'], language = 'en', fallback = false, closeDuringTarget = false, initialText, generationOptions = {} } = {}) {
  const commands = new Map(), listeners = [], calls = [], diffs = [], emitted = [], errors = [], edits = [], subscriptions = [];
  let provider, currentText = initialText ?? '# API guide 😀\n\nManual prose.\n', version = 1, onRecheck, diffError = false;
  const original = currentText;
  const uri = text => ({ scheme: text.split(':')[0], fsPath: '/owned/api.yaml', toString: () => text });
  const doc = { uri: uri('file:/owned/API-guide.md'), languageId: 'markdown', isClosed: false,
    get version() { return version; }, getText: () => currentText, positionAt: offset => ({ offset }) };
  const generated = output(original, { fallback, ...generationOptions });
  const deep = value => { if (value && typeof value === 'object') { Object.freeze(value); for (const child of Object.values(value)) deep(child); } return value; };
  deep(generated);
  const token = () => ({ dispose() {} });
  const editor = { document: doc, viewColumn: 1, edit: async (callback, options) => {
    const replacements = []; callback({ replace: (range, insert) => replacements.push({ from: range.start.offset, to: range.end.offset, insert }) });
    edits.push({ replacements, options });
    for (const change of [...replacements].reverse()) currentText = currentText.slice(0, change.from) + change.insert + currentText.slice(change.to);
    version++; return true;
  } };
  const close = value => listeners.forEach(listener => listener({ uri: value }));
  const stub = {
    env: { language }, Uri: { parse: uri }, Range: class { constructor(start, end) { Object.assign(this, { start, end }); } },
    EventEmitter: class {
      constructor() { this.listeners = new Set(); this.event = listener => { this.listeners.add(listener); return { dispose: () => this.listeners.delete(listener) }; }; }
      fire(value) { for (const listener of this.listeners) listener(value); }
      dispose() { this.listeners.clear(); }
    },
    workspace: { isTrusted: true, registerTextDocumentContentProvider: (scheme, value) => {
      assert.equal(scheme, 'readmd-openapi-review'); provider = value;
      value.onDidChange(event => emitted.push({ uri: event.toString(), text: provider.provideTextDocumentContent(event) })); return token();
    }, onDidCloseTextDocument: listener => { listeners.push(listener); return token(); } },
    commands: { registerCommand: (name, callback) => { commands.set(name, callback); return token(); }, executeCommand: async (name, ...args) => {
      assert.equal(name, 'vscode.diff');
      if (diffError) throw Error('synthetic diff failed');
      const [left, right] = args; assert.equal(left.scheme, 'readmd-openapi-review'); assert.equal(right.scheme, 'readmd-openapi-review');
      assert.notEqual(left.toString(), right.toString());
      diffs.push({ left, right, before: provider.provideTextDocumentContent(left), after: provider.provideTextDocumentContent(right) });
      stub.window.activeTextEditor = { document: { uri: right, languageId: 'markdown' } };
    } },
    window: { activeTextEditor: editor, showOpenDialog: async () => [], showWarningMessage: async message => { errors.push(message); },
      showErrorMessage: async message => { errors.push(message); }, showInformationMessage: async (_message, ...choices) => {
        const input = decisions.shift();
        if (typeof input === 'function') return input({ choices, close, diff: diffs.at(-1), stub, mutate: value => { currentText = value; version++; } });
        if (input === 'apply') return choices.find(c => c === 'Accept changes' || c === '接受修改');
        if (input === 'raw') return choices.find(c => c === 'View raw text' || c === '查看完整原文');
        if (input === 'summary') return choices.find(c => c === 'Show readable diff' || c === '显示易读差异');
        return choices.find(c => c === 'Cancel' || c === '取消');
      }, showTextDocument: async document => {
        assert.equal(document, doc); stub.window.activeTextEditor = editor;
        if (closeDuringTarget) { const pair = diffs.at(-1); close(pair.left); close(pair.right); }
        return editor;
      } },
  };
  const bridge = { callMcpTool: async (name, args) => {
    assert.equal(name, 'readmd_openapi_document'); calls.push(args);
    if (args.expected_source_revision && onRecheck) await onRecheck();
    return generated;
  } };
  const entry = path.resolve(__dirname, '../out/openapi.js'), localization = path.resolve(__dirname, '../out/localization.js');
  const load = Module._load;
  Module._load = function (name, ...args) { return name === 'vscode' ? stub : load.call(this, name, ...args); };
  try { delete require.cache[entry]; delete require.cache[localization]; require(entry).registerOpenAPI({ subscriptions }, bridge); }
  finally { Module._load = load; }
  return { stub, original, doc, generated, calls, diffs, emitted, errors, edits, close,
    text: () => currentText, provider: () => provider,
    run: async () => { stub.window.activeTextEditor = editor; await commands.get('readmd.openApiDocument')(uri('file:/owned/api.yaml')); },
    recheck: fn => { onRecheck = fn; }, diffError: flag => { diffError = flag; },
    dispose: () => subscriptions.forEach(item => item.dispose()) };
}

test('both native diff documents are readonly projections and Cancel makes no edit', async () => {
  const f = fixture(); await f.run();
  assert.equal(f.diffs.length, 1); const pair = f.diffs[0];
  assert.equal(pair.before, f.original); assert(pair.after.includes('VISIBLE_ENDPOINT'));
  assert(pair.after.includes('Generated API record · Operations: 1 · Referenced files: 1'));
  assert(!pair.after.includes('<!-- readmd:openapi:start '));
  assert.equal(f.text(), f.original); assert.equal(f.edits.length, 0); assert.equal(f.calls.length, 1);
  f.close(pair.left); assert.equal(f.provider().provideTextDocumentContent(pair.right), pair.after);
  f.close(pair.right); assert.equal(f.provider().provideTextDocumentContent(pair.right), ''); f.dispose();
});

test('raw and summary toggles update only this pair and preserve the exact raw transaction on Accept', async () => {
  const f = fixture({ decisions: ['raw', 'summary', 'apply'], closeDuringTarget: true });
  const rawSnapshot = JSON.stringify(f.generated); await f.run();
  assert.equal(f.emitted.length, 4); const pair = f.diffs[0];
  assert.deepEqual(f.emitted.map(e => e.uri), [pair.left, pair.right, pair.left, pair.right].map(u => u.toString()));
  assert.equal(f.emitted[0].text, f.original); assert.equal(f.emitted[1].text, f.generated.result.content);
  assert.equal(f.emitted[3].text, pair.after); assert.equal(f.text(), f.generated.result.content);
  assert.equal(f.edits.length, 1); assert.deepEqual(f.edits[0].replacements, f.generated.transaction.edits);
  assert.deepEqual(f.edits[0].options, { undoStopBefore: true, undoStopAfter: true });
  assert.equal(f.calls[1].expected_source_revision, sourceRevision); assert.equal(JSON.stringify(f.generated), rawSnapshot);
  assert.deepEqual(f.errors, []); f.dispose();
});

test('unknown transaction shape remains raw but Accept still applies the original complete transaction', async () => {
  const f = fixture({ fallback: true, decisions: ['apply'] }); await f.run();
  assert.equal(f.diffs[0].after, f.generated.result.content); assert.equal(f.text(), f.generated.result.content);
  assert.deepEqual(f.edits[0].replacements, f.generated.transaction.edits); assert.deepEqual(f.emitted, []); f.dispose();
});

test('an author edit before Accept still refuses the transaction', async () => {
  const f = fixture({ decisions: [({ choices, mutate }) => { mutate('New author prose\n'); return choices[0]; }] });
  await f.run(); assert.equal(f.text(), 'New author prose\n'); assert.equal(f.edits.length, 0);
  assert.equal(f.calls.length, 1); assert(f.errors[0].includes('changed')); f.dispose();
});

test('an update displays both compact sides, while Accept keeps every raw carrier byte', async () => {
  const notes = '# API guide 😀\n\nManual prose.\n', first = output(notes);
  const f = fixture({ initialText: first.result.content, generationOptions: { replace: { from: notes.length + 2, to: first.result.content.length }, body: '## GET /items\n\nVISIBLE_ENDPOINT_NEW\n' }, decisions: ['raw', 'apply'] });
  await f.run(); const pair = f.diffs[0];
  assert(pair.before.includes('Previous API record · Operations: 1'));
  assert(pair.after.includes('Updated API record · Operations: 1'));
  assert(!pair.before.includes('<!-- readmd:openapi:start ')); assert(!pair.after.includes('<!-- readmd:openapi:start '));
  assert.equal(f.emitted[0].text, first.result.content); assert.equal(f.emitted[1].text, f.generated.result.content);
  assert.equal(f.text(), f.generated.result.content); assert.deepEqual(f.edits[0].replacements, f.generated.transaction.edits); f.dispose();
});

test('edit then undo to equal text still rejects the old choice by document version', async () => {
  const original = '# API guide 😀\n\nManual prose.\n';
  const f = fixture({ decisions: [({ choices, mutate }) => { mutate('Temporary author edit'); mutate(original); return choices[0]; }] });
  await f.run(); assert.equal(f.text(), original); assert.equal(f.edits.length, 0); assert.equal(f.calls.length, 1); assert(f.errors[0].includes('changed')); f.dispose();
});

test('metadata-only changes remain visible even when body and counts are identical', async () => {
  const notes = '# API guide 😀\n\nManual prose.\n', first = output(notes);
  const f = fixture({ initialText: first.result.content, generationOptions: { replace: { from: notes.length + 2, to: first.result.content.length }, revision: 'd'.repeat(64) } });
  await f.run(); const pair = f.diffs[0];
  assert.notEqual(f.generated.result.content, first.result.content, 'independent raw metadata mutation');
  assert.notEqual(pair.before, pair.after, 'readable diff must disclose the metadata-only update');
  assert.equal(f.edits.length, 0); f.dispose();
});

test('a source revision recheck failure still refuses Apply after raw display', async () => {
  const f = fixture({ decisions: ['raw', 'apply'] }); f.recheck(async () => { throw Error('transaction_conflict'); });
  await f.run(); assert.equal(f.text(), f.original); assert.equal(f.edits.length, 0); assert.equal(f.calls.length, 2);
  assert.equal(f.calls[1].expected_source_revision, sourceRevision); assert(f.errors[0].includes('changed')); f.dispose();
});

test('a late choice after an owned view closes cannot toggle or apply; foreign closes are ignored', async () => {
  const closed = fixture({ decisions: [({ choices, close, diff }) => { close(diff.left); return choices[0]; }] });
  await closed.run(); assert.equal(closed.edits.length, 0); assert.equal(closed.calls.length, 1); assert.equal(closed.emitted.length, 0); closed.dispose();
  const foreign = fixture({ decisions: [({ choices, close }) => { close({ toString: () => 'readmd-openapi-review:/foreign.md?other' }); return choices.at(-1); }, 'cancel'] });
  await foreign.run(); assert.equal(foreign.emitted.length, 2); assert.equal(foreign.edits.length, 0); foreign.dispose();
});

test('closing an owned view during the source recheck refuses Apply before acquiring the target editor', async () => {
  const f = fixture({ decisions: ['apply'] }); f.recheck(async () => f.close(f.diffs[0].right));
  await f.run(); assert.equal(f.edits.length, 0); assert.equal(f.text(), f.original); assert(f.errors[0].includes('changed')); f.dispose();
});

test('URI pairs preserve all16 logical review slots, then release only their own records', async () => {
  const f = fixture();
  for (let i = 0; i < 16; i++) await f.run();
  assert.equal(f.diffs.length, 16);
  await f.run(); assert.equal(f.diffs.length, 16); assert(f.errors.at(-1).includes('limit'));
  f.close(f.diffs[0].left); f.close(f.diffs[0].right); await f.run(); assert.equal(f.diffs.length, 17);
  const last = f.diffs.at(-1); f.dispose(); assert.equal(f.provider().provideTextDocumentContent(last.right), '');
});

test('a failed native diff does not consume a hidden review slot', async () => {
  const f = fixture(); f.diffError(true); for (let i = 0; i < 17; i++) await f.run();
  f.diffError(false); await f.run(); assert.equal(f.diffs.length, 1); assert.equal(f.edits.length, 0); f.dispose();
});

test('the simplified Chinese presentation labels are translated, not English copies', async () => {
  const f = fixture({ language: 'zh-cn', decisions: ['raw', 'summary', 'cancel'] }); await f.run();
  assert(f.diffs[0].after.includes('接口生成记录 · 接口数：1 · 引用文件数：1'));
  assert.equal(f.emitted.length, 4); assert.equal(f.emitted[1].text, f.generated.result.content); f.dispose();
});
