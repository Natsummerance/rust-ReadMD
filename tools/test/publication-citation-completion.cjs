'use strict';
// Run the shipped editor context/apply functions against its real CodeMirror
// parser and history. No server, native process or fabricated bibliography.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const root = path.resolve(__dirname, '../..');
const source = fs.readFileSync(path.join(root, 'assets/js/editor/editor.js'), 'utf8');
const document = {readyState: 'loading', documentElement: {style: {}}, addEventListener() {}, createElement() { return {style: {}, setAttribute() {}}; }};
const sandbox = {document, navigator: {userAgent: 'ReadMD offline QA', platform: 'Win32'}, console, setTimeout, clearTimeout};
sandbox.window = {document, navigator: sandbox.navigator, addEventListener() {}};
vm.createContext(sandbox);
vm.runInContext(fs.readFileSync(path.join(root, 'assets/vendor/codemirror.bundle.js'), 'utf8'), sandbox, {filename: 'codemirror.bundle.js'});
vm.runInContext(fs.readFileSync(path.join(root, 'assets/js/editor/formula-studio.js'), 'utf8'), sandbox, {filename: 'formula-studio.js'});
const contextFunction = source.slice(source.indexOf('function cmKnowledgeContext('), source.indexOf('\nfunction cmMathContext('));
const codeFunction = source.slice(source.indexOf('const CM_CODE_NODE_RE ='), source.indexOf('\nfunction cmDispatchEdit('));
const applyBody = /apply\(view, completion, from, to\) \{([\s\S]*?)\n          \},/.exec(source.slice(source.indexOf('const options = rows.map')));
assert(applyBody, 'Editor completion apply callback changed; review the extraction');
vm.runInContext(codeFunction + '\n' + contextFunction + '\nfunction applyCompletion(view,completion,from,to){' + applyBody[1] + '\n}', sandbox);
const CM = sandbox.window.ReadMDCodeMirror;
function context(input) {
  const pos = input.indexOf('|'); assert(pos >= 0);
  const text = input.slice(0, pos) + input.slice(pos + 1);
  const state = CM.EditorState.create({doc: text, extensions: [CM.markdown(), CM.history()]});
  return {text, state, pos, semantic: sandbox.cmKnowledgeContext({state, pos})};
}
let assertions = 0;
for (const [input, query, close] of [
  ['[see @stu|', 'stu', ']'], ['[-@stu|', 'stu', ']'],
  ['😀 @stu|', 'stu', ''], [':cite[author-only @stu|', 'stu', ']'],
  ['[@first; cf. @stu|', 'stu', ']'], ['[见 @证据|', '证据', ']'],
  ['[@{https://example.test/bib?name=wo|', '{https://example.test/bib?name=wo', '}]'],
  ['@{title with spaces|', '{title with spaces', '}'],
]) {
  const result = context(input); assert.equal(result.semantic?.kind, 'citation', input);
  assert.equal(result.semantic.query, query, input); assert.equal(result.semantic.close, close, input);
  assert.equal(result.text.slice(result.semantic.from, result.pos), query.replace(/^\{/, ''), input); assertions++;
}
for (const input of [
  'user@example|', 'user+work@example|', 'https://example.test/@stu|',
  'https://example.test/?name=@stu|', 'mailto:@stu|', '\\@stu|',
  '`@stu|`', '```md\n@stu|\n```', '~~~md\n@stu|',
  '$x+@stu|$', '$x + @stu|', '$$\nx+@stu|', '\\(@stu|\\)',
  '\\[x+@stu|', ':math[row(@stu|)]', ':math[row(@stu|',
  '[label @stu|](https://example.test)', ':cite[unknown @stu|',
  '---\nexample: @stu|\n---\nText',
]) { assert.equal(context(input).semantic, null, input); assertions++; }
for (const [input, key, expected] of [
  ['[see @stu|]', 'study', '[see @study]'], ['[-@stu|]', 'study', '[-@study]'],
  ['@stu|', 'study', '@study'], [':cite[author-only @stu|]', 'study', ':cite[author-only @study]'],
  ['[@{stu|}]', 'study', '[@{study}]'], ['[@{stu|}', 'study', '[@{study}]'],
  ['[@{stu|]', 'study', '[@{study}]'], ['[@{stu|', 'study', '[@{study}]'],
  ['@{stu|}', 'study', '@{study}'], ['[[rea|]', 'reading', '[[reading]]'],
  ['[^sc|]', 'scope', '[^scope]'], ['[x](not|)', 'notes.md', '[x](notes.md)'],
]) {
  const result = context(input); assert(result.semantic, input + ' ' + CM.syntaxTree(result.state).toString());
  let dispatches = 0;
  const view = {state: result.state, composing: false, dispatch(spec) { dispatches++; this.state = spec.state || this.state.update(spec).state; }};
  view.dispatch = view.dispatch.bind(view);
  Object.assign(sandbox, {state: {activeTabId: 'qa'}, tabId: 'qa', cmView: view, semantic: result.semantic, row: {insert: key}});
  sandbox.applyCompletion(view, {}, result.semantic.from, result.pos);
  assert.equal(dispatches, 1, input); assert.equal(view.state.doc.toString(), expected, input);
  assert(CM.undo(view), 'Completion must be undoable: ' + input);
  assert.equal(view.state.doc.toString(), result.text, input);
  assert(CM.redo(view), 'Completion must be redoable: ' + input);
  assert.equal(view.state.doc.toString(), expected, input); assertions++;
}
console.log(JSON.stringify({ok: true, assertions, parser: 'shipped CodeMirror', history: 'single transaction, undo and redo'}, null, 2));
