'use strict';
// Pure controller boundaries; the process script separately exercises actual
// filechooser, PNG decoder, PDFium worker, save and reopen under production CSP.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const source = fs.readFileSync(path.resolve(__dirname, '../../assets/js/features/pdf.js'), 'utf8');
const slice = (from, to) => { const start = source.indexOf(from), end = source.indexOf(to, start); assert(start >= 0 && end > start); return source.slice(start, end); };
const controller = slice('  function select(item){', '  function setLoading(value){') + slice('  async function replaceImage(){', '  async function close(){');

function host() {
  const nodes = new Map(), inputs = [], pending = [], calls = [], bitmaps = [];
  let canvasAllocations = 0;
  const node = id => { if (!nodes.has(id)) nodes.set(id, { value: '', hidden: false, focus() {}, closest() { return this; } }); return nodes.get(id); };
  const context = { session: { session: 'A', revision: 3, page: 0, can_edit: true }, selected: { kind: 'image', id: 2 }, working: false, closing: false, imageSelectionEpoch: 0,
    t: key => key, $pdf: node, Uint8ClampedArray, btoa: text => Buffer.from(text, 'binary').toString('base64'),
    document: { createElement(tag) {
      if (tag === 'input') { const input = { files: [{ type: 'image/png' }], click() { this.clicked = true; } }; inputs.push(input); return input; }
      assert.equal(tag, 'canvas'); canvasAllocations++;
      return { getContext: () => ({ drawImage() {}, getImageData: () => ({ data: new Uint8ClampedArray(16) }) }) };
    } },
    createImageBitmap: () => new Promise((resolve, reject) => pending.push({ resolve, reject })),
    edit: async operation => { calls.push({ operation, session: context.session.session, revision: context.session.revision }); }
  };
  vm.createContext(context); vm.runInContext(controller, context);
  const bitmap = (width = 2, height = 2) => { const value = { width, height, closed: 0, close() { this.closed++; } }; bitmaps.push(value); return value; };
  const picker = async () => { await context.replaceImage(); return inputs.at(-1); };
  const choose = async () => { const input = await picker(); const work = input.onchange(); return { input, work }; };
  return { context, calls, inputs, pending, nodes, bitmaps, bitmap, picker, choose, canvasAllocations: () => canvasAllocations };
}

test('file selection from an obsolete PDF context is discarded before decoding', async () => {
  const f = host(), input = await f.picker(); f.context.session = { session: 'B', revision: 0, page: 0 };
  const work = input.onchange(); await Promise.resolve();
  if (f.pending.length) f.pending[0].resolve(f.bitmap());
  await work; assert.equal(f.pending.length, 0); assert.equal(f.calls.length, 0); assert.equal(f.canvasAllocations(), 0);
});

for (const [label, change] of [
  ['different document', c => { c.session = { session: 'B', revision: 0, page: 0 }; }],
  ['changed revision on the same session object', c => { c.session.revision++; }],
  ['changed page on the same session object', c => { c.session.page = 1; }],
  ['different selection', c => { c.selected = { kind: 'image', id: 9 }; }],
  ['in-flight operation', c => { c.working = true; }],
  ['an in-flight close', c => { c.closing = true; }],
  ['closed workspace', c => { c.session = null; c.selected = null; }]
]) test('pending decode is cancelled for ' + label, async () => {
  const f = host(), { work } = await f.choose(), image = f.bitmap();
  change(f.context); f.pending[0].resolve(image); await work;
  assert.equal(f.calls.length, 0); assert.equal(f.canvasAllocations(), 0); assert.equal(image.closed, 1);
});

test('deselecting and reselecting the same object never revives an older selection request', async () => {
  const f = host(), target = f.context.selected, { work } = await f.choose(), image = f.bitmap();
  f.context.select(null); f.context.select(target); f.pending[0].resolve(image); await work;
  assert.equal(f.calls.length, 0); assert.equal(image.closed, 1);
});

test('opening a newer picker invalidates the older decode even before the newer file is selected', async () => {
  const f = host(), first = await f.choose(), image = f.bitmap();
  await f.picker(); f.pending[0].resolve(image); await first.work;
  assert.equal(f.calls.length, 0); assert.equal(image.closed, 1);
  const next = f.inputs.at(-1).onchange(), nextImage = f.bitmap(); f.pending[1].resolve(nextImage); await next;
  assert.equal(f.calls.length, 1); assert.equal(nextImage.closed, 1);
});

test('late decoding failure does not replace the new document status', async () => {
  const f = host(), { work } = await f.choose();
  f.context.session = { session: 'B', revision: 0, page: 0 }; f.context.$pdf('status').textContent = 'B remains ready';
  f.pending[0].reject(Error('unsupported image')); await work;
  assert.equal(f.context.$pdf('status').textContent, 'B remains ready'); assert.equal(f.calls.length, 0);
});

test('obsolete decoding failure cannot clear the new session task or overwrite its status', async () => {
  const f = host(), { work } = await f.choose();
  f.context.session = { session: 'B', revision: 5, page: 1 }; f.context.working = true; f.context.task = 'B-active-task';
  f.context.$pdf('status').textContent = 'B is processing'; f.pending[0].reject(Error('bad PNG')); await work;
  assert.equal(f.context.$pdf('status').textContent, 'B is processing'); assert.equal(f.context.working, true); assert.equal(f.context.task, 'B-active-task'); assert.equal(f.calls.length, 0);
});

test('an older failure in the same session is silent while the newer picker still succeeds', async () => {
  const f = host(), first = await f.choose(); await f.picker();
  f.context.$pdf('status').textContent = 'Current selection'; f.pending[0].reject(Error('old bad PNG')); await first.work;
  assert.equal(f.context.$pdf('status').textContent, 'Current selection'); assert.equal(f.calls.length, 0);
  const work = f.inputs.at(-1).onchange(), image = f.bitmap(); f.pending[1].resolve(image); await work;
  assert.equal(f.calls.length, 1); assert.equal(image.closed, 1);
});

test('a current image produces exactly one edit and releases its bitmap', async () => {
  const f = host(), { work } = await f.choose(), image = f.bitmap(); f.pending[0].resolve(image); await work;
  assert.equal(f.calls.length, 1); assert.deepEqual(JSON.parse(JSON.stringify(f.calls[0])), { session: 'A', revision: 3, operation: { kind: 'image', page: 0, object: 2, width: 2, height: 2, rgba: Buffer.alloc(16).toString('base64') } });
  assert.equal(image.closed, 1); assert.equal(f.canvasAllocations(), 1);
});

test('oversized current image keeps the existing limit and releases its bitmap without editing', async () => {
  const f = host(), { work } = await f.choose(), image = f.bitmap(3000, 2000); f.pending[0].resolve(image); await work;
  assert.equal(f.calls.length, 0); assert.equal(f.context.$pdf('status').textContent, 'error.pdf_limit'); assert.equal(image.closed, 1);
});

test('cancelling the file picker sends no edit', async () => {
  const f = host(), input = await f.picker(); input.files = []; await input.onchange(); assert.equal(f.pending.length, 0); assert.equal(f.calls.length, 0);
});
