'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const root = path.resolve(__dirname, '../..');
const source = fs.readFileSync(path.join(root, 'assets/js/features/pdf.js'), 'utf8');
const start = source.indexOf('  function cancelDeleteConfirmation()'), end = source.indexOf('  async function apply(){', start);
assert(start >= 0 && end > start);

function pdfHost() {
  const prompts = [], edits = [];
  const c = { session: { session: 'A', revision: 3, page: 0 }, working: false, closing: false, deleteConfirmation: null, AbortController,
    t: key => key, confirmAction: options => new Promise(resolve => prompts.push({ options, resolve })), edit: async operation => edits.push(operation) };
  vm.createContext(c); vm.runInContext(source.slice(start, end), c);
  return { c, prompts, edits };
}
for (const [label, change] of [
  ['a different document', c => { c.session = { session: 'B', revision: 3, page: 0 }; }],
  ['a changed page', c => { c.session.page = 1; }],
  ['a changed revision', c => { c.session.revision++; }],
  ['a running operation', c => { c.working = true; }],
  ['an in-flight close', c => { c.closing = true; }],
  ['a closed document', c => { c.session = null; }]
]) test('a delayed confirmation cannot delete from ' + label, async () => {
  const f = pdfHost(), pending = f.c.deletePage(); change(f.c); f.prompts[0].resolve(true); await pending; assert.equal(f.edits.length, 0);
});
test('explicit close cancellation remains effective even if the dialog resolves true later', async () => {
  const f = pdfHost(), pending = f.c.deletePage(); f.c.cancelDeleteConfirmation();
  assert.equal(f.prompts[0].options.signal.aborted, true); f.prompts[0].resolve(true); await pending; assert.equal(f.edits.length, 0);
});
test('a newer delete prompt supersedes the old one and only the new page is deleted', async () => {
  const f = pdfHost(), first = f.c.deletePage(); f.c.session.page = 1; const second = f.c.deletePage();
  assert.equal(f.prompts[0].options.signal.aborted, true); f.prompts[0].resolve(true); f.prompts[1].resolve(true);
  await Promise.all([first, second]); assert.deepEqual(JSON.parse(JSON.stringify(f.edits)), [{ kind: 'delete_page', page: 1 }]);
});
test('a cancelled or unchanged current confirmation has the expected outcome', async () => {
  const f = pdfHost(), cancelled = f.c.deletePage(); f.prompts[0].resolve(false); await cancelled; assert.equal(f.edits.length, 0);
  const accepted = f.c.deletePage(); f.prompts[1].resolve(true); await accepted;
  assert.deepEqual(JSON.parse(JSON.stringify(f.edits)), [{ kind: 'delete_page', page: 0 }]);
});

function closeHost({ dirty = false, answer = 'cancel', saveResult = true, closeFails = false } = {}) {
  const from = source.indexOf('  async function close(){'), to = source.indexOf('  async function open(path){', from);
  assert(from >= 0 && to > from);
  const permit = Symbol('private'), calls = [], pending = [];
  const nodes = new Map(); const $pdf = id => { if (!nodes.has(id)) nodes.set(id, { value: '' }); return nodes.get(id); };
  const c = { closing: false, working: false, task: null, session: { session: 'A', dirty }, openedPath: 'draft.pdf', modal: { classList: { add() {} } }, opener: null,
    closeSavePermit: permit, $pdf, t: key => key, cancelDeleteConfirmation() { calls.push('cancel-confirmation'); }, updateUnloadGuard() {},
    async cancelWorkingTask() { calls.push('cancel-task'); },
    promptDirtyClose: async () => answer, run: async (...args) => { calls.push({ run: args }); return saveResult; },
    request: async payload => { calls.push(payload); if (closeFails) throw Error('failed'); return new Promise(resolve => pending.push(resolve)); } };
  vm.createContext(c); vm.runInContext(source.slice(from, to), c); return { c, calls, pending, permit };
}
test('close is serialized before awaiting the backend and releases its guard after completion', async () => {
  const f = closeHost(), first = f.c.close(); await Promise.resolve(); assert.equal(f.c.closing, true);
  assert.equal(await f.c.close(), false); assert.equal(f.calls.filter(x => x.action === 'close').length, 1);
  f.pending[0]({}); assert.equal(await first, true); assert.equal(f.c.closing, false); assert.equal(f.c.session, null);
});
test('cancelled or failed save-on-close keeps the session and releases the closing guard', async () => {
  for (const opts of [{ dirty: true, answer: 'cancel' }, { dirty: true, answer: 'save', saveResult: false }, { closeFails: true }]) {
    const f = closeHost(opts); assert.equal(await f.c.close(), false); assert.equal(f.c.closing, false); assert.equal(f.c.session.session, 'A');
    if (opts.answer === 'save') assert.equal(f.calls.find(x => x.run).run[2], f.permit);
    assert.equal(f.calls.filter(x => x.action === 'close').length, opts.closeFails ? 1 : 0);
  }
});
test('save-on-close uses its private permit and then closes the original session', async () => {
  const f = closeHost({ dirty: true, answer: 'save' }), pending = f.c.close();
  while (!f.pending.length) await Promise.resolve();
  const call = f.calls.find(x => x.run).run; assert.equal(call[0], 'save'); assert.equal(call[2], f.permit); assert.equal(f.c.closing, true);
  f.pending[0]({}); assert.equal(await pending, true); assert.equal(f.c.closing, false);
});
test('a second close can cancel an ongoing save-on-close without starting another close', async () => {
  const f = closeHost(); f.c.closing = true; f.c.working = true; f.c.task = 'active-task';
  assert.equal(await f.c.close(), false); assert.equal(f.calls.filter(x => x === 'cancel-task').length, 1);
  assert.equal(f.calls.filter(x => x.action === 'close').length, 0);
});
test('repeated cancellation of the same task shares one in-flight request', async () => {
  const from = source.indexOf('  async function cancelWorkingTask(){'), to = source.indexOf('  async function close(){', from); assert(from >= 0 && to > from);
  const calls = [], pending = []; const c = { task: 'task-A', taskCancellation: null, apiFetch: (url, request) => { calls.push({ url, request }); return new Promise(resolve => pending.push(resolve)); } };
  vm.createContext(c); vm.runInContext(source.slice(from, to), c);
  const first = c.cancelWorkingTask(), second = c.cancelWorkingTask(); assert.equal(calls.length, 1);
  pending[0]({}); await Promise.all([first, second]); assert.equal(c.taskCancellation, null);
  c.task = 'task-B'; const third = c.cancelWorkingTask(); assert.equal(calls.length, 2); pending[1]({}); await third;
});

function dialogHost() {
  class Element extends EventTarget {
    constructor() { super(); this.isConnected = true; this.classes = new Set(['hidden']); this.focuses = 0;
      this.classList = { add: value => this.classes.add(value), remove: value => this.classes.delete(value) }; }
    focus() { this.focuses++; }
  }
  const nodes = new Map(), timers = []; for (const id of ['confirm-modal', 'confirm-action', 'confirm-cancel', 'confirm-title', 'confirm-message']) nodes.set(id, new Element());
  const c = { $: id => nodes.get(id), document: { activeElement: null }, HTMLElement: Element, setTimeout: f => timers.push(f) };
  vm.createContext(c); vm.runInContext(fs.readFileSync(path.join(root, 'assets/js/core/dialog.js'), 'utf8'), c);
  return { c, nodes, timers };
}
test('aborting a confirmation hides it, detaches its callbacks, and does not focus it later', async () => {
  const f = dialogHost(), controller = new AbortController(), pending = f.c.confirmAction({ title: 'Delete', signal: controller.signal });
  assert.equal(f.nodes.get('confirm-modal').classes.has('hidden'), false); controller.abort();
  assert.equal(await pending, false); assert.equal(f.nodes.get('confirm-modal').classes.has('hidden'), true);
  f.nodes.get('confirm-action').dispatchEvent(new Event('click')); f.timers.forEach(run => run()); assert.equal(f.nodes.get('confirm-cancel').focuses, 0);
});
test('an already-aborted request does not cancel an unrelated active confirmation', async () => {
  const f = dialogHost(), original = f.c.confirmAction({ title: 'First' }), controller = new AbortController(); controller.abort();
  assert.equal(await f.c.confirmAction({ title: 'Aborted', signal: controller.signal }), false);
  assert.equal(f.nodes.get('confirm-modal').classes.has('hidden'), false); f.nodes.get('confirm-action').dispatchEvent(new Event('click')); assert.equal(await original, true);
});
test('aborting a superseded request does not close the newer confirmation', async () => {
  const f = dialogHost(), controller = new AbortController(), old = f.c.confirmAction({ signal: controller.signal });
  const next = f.c.confirmAction({ title: 'Next' }); assert.equal(await old, false); controller.abort();
  assert.equal(f.nodes.get('confirm-modal').classes.has('hidden'), false); f.nodes.get('confirm-action').dispatchEvent(new Event('click')); assert.equal(await next, true);
});
