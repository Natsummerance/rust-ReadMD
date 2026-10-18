'use strict';
// Small, independent interaction tests; these do not certify native rendering
// or citation output. Rust remains the only citation HTML serializer.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const script = fs.readFileSync(path.join(__dirname, '../../assets/js/reader/publication.js'), 'utf8');
const source = '---\r\nbibliography: refs.json\r\n---\r\n😀 中文 [@sample]\r\n\r\nAgain [@sample].\r\n';
const normalized = source.replace(/\r\n?/g, '\n');
const first = source.indexOf('[@sample]'), second = source.indexOf('[@sample]', first + 1);

function fixture(content = source, revision = 'qa-source-v1') {
  const spans = [...content.matchAll(/\[@sample\]/g)].map(match => ({ from: match.index, to: match.index + 9, source: '[@sample]' }));
  return {
    ok: true, content: 'legacy compatibility projection', reader_content: 'Rust safe reader projection',
    source_citations: spans.map(span => ({ ...span, revision })),
    projection: { revision, source_utf16: content.length, citations: spans.map(span => ({ source: { span, items: [{cite: {key: 'sample', mode: 'normal'}}], context: {in_note: false, note_number: null, note_label: null} }, rich: [{type: 'text', text: '(Sample, 2026)'}], rendered: true })) },
  };
}
function element(index, data) {
  const span = data.projection.citations[index].source.span;
  return {
    dataset: { readmdCitationIndex: String(index), readmdSourceFrom: String(span.from), readmdSourceTo: String(span.to), readmdSourceRevision: data.projection.revision },
    isConnected: true, textContent: '(Sample, 2026)', attributes: {},
    setAttribute(key, value) { this.attributes[key] = value; },
    closest() { return this; },
  };
}
function environment({ reading = false, response = fixture(), fetch, original = source, display = original } = {}) {
  const tab = { id: 'a', content: display, original, isDirty: false }, selections = [], toasts = [], events = {}, nodes = [element(0, response), element(1, response)];
  const pane = {
    querySelectorAll(selector) { return selector.startsWith('.readmd-citation') ? nodes : []; },
    contains(node) { return nodes.includes(node); },
    addEventListener(name, callback) { events[name] = callback; },
  };
  const makeView = () => ({ state: { doc: { toString: () => original.replace(/\r\n?/g, '\n') } }, dispatch(change) { selections.push(change); }, focus() { this.focused = true; } });
  const state = { activeTabId: 'a', file: 'Z:/paper.md', editing: !reading, original, fixed: display };
  const context = {
    window: {}, state, cmView: reading ? null : makeView(), editorOpenEpoch: 0,
    getActiveTab: () => context.tab, getEditContent: () => context.raw,
    apiFetch: fetch || (async () => ({ ok: true, json: async () => response })),
    i18n: { t: () => 'Edit source' }, showToast(value) { toasts.push(value); }, apiMessage: () => 'Preparation failed',
    AbortSignal, Date, setTimeout, clearTimeout,
  };
  context.tab = tab; context.raw = original;
  context.toggleEdit = async () => { state.editing = true; context.cmView = makeView(); context.editorOpenEpoch++; };
  vm.runInNewContext(script, context, { filename: 'publication.js' });
  const publication = context.window.ReadMDPublication;
  return { context, publication, selections, toasts, nodes, pane, events, makeView, async prepare() { const result = await publication.prepare(display, state.file); publication.applyLayout(pane); return result; } };
}
const selectionFor = at => ({ anchor: source.slice(0, at).replace(/\r\n?/g, '\n').length, head: source.slice(0, at + 9).replace(/\r\n?/g, '\n').length });
const plain = value => JSON.parse(JSON.stringify(value));

test('consume server reader_content and retain the independent source model', async () => {
  const env = environment();
  assert.equal(await env.prepare(), 'Rust safe reader projection');
  assert.equal(env.publication.revision(), 'qa-source-v1');
  assert.deepEqual(plain(env.publication.sourceCitations()), fixture().source_citations);
  assert.deepEqual(plain(env.publication.projection()), fixture().projection);
  // The real editor uses a global lexical cmView, not window.cmView.
  assert.equal(env.context.window.cmView, undefined);
});
test('select the second identical citation at its exact CRLF/emoji UTF-16 range', async () => {
  const env = environment(); await env.prepare();
  assert.equal(await env.publication.locateCitation(env.nodes[1]), true);
  assert.deepEqual(plain(env.selections), [{ selection: selectionFor(second), scrollIntoView: true }]);
  assert.equal(env.context.cmView.focused, true);
  assert.equal(env.context.raw, source);
});
test('reject stale source, tab, tab identity, editor view, detached node and cleared cache', async t => {
  const cases = {
    source: e => { e.context.raw += 'changed'; },
    tab: e => { e.context.state.activeTabId = 'b'; },
    identity: e => { e.context.tab = { ...e.context.tab }; },
    view: e => { e.context.cmView = e.makeView(); },
    detached: e => { e.nodes[1].isConnected = false; },
    cleared: e => { e.publication.clear(); },
  };
  for (const [name, mutate] of Object.entries(cases)) await t.test(name, async () => {
    const env = environment(); await env.prepare(); mutate(env);
    assert.equal(await env.publication.locateCitation(env.nodes[1]), false);
    assert.equal(env.selections.length, 0);
  });
});
test('reject replayed revision, wrong occurrence span and legacy/projection disagreement', async t => {
  const cases = {
    revision: e => { e.nodes[1].dataset.readmdSourceRevision = 'old'; },
    occurrence: e => { e.nodes[1].dataset.readmdSourceFrom = String(first); },
    index: e => { e.nodes[1].dataset.readmdCitationIndex = '01'; },
    legacy: e => { e.publication.sourceCitations()[1].from = first; },
    text: e => { e.publication.projection().citations[1].source.span.source = '[@other!]'; },
  };
  for (const [name, mutate] of Object.entries(cases)) await t.test(name, async () => {
    const env = environment(); await env.prepare(); mutate(env);
    assert.equal(await env.publication.locateCitation(env.nodes[1]), false);
    assert.equal(env.selections.length, 0);
  });
});
test('keyboard activation is accessible and modified clicks keep their link behavior', async () => {
  const env = environment(); await env.prepare();
  assert.equal(env.nodes[1].tabIndex, 0); assert.equal(env.nodes[1].attributes.role, 'button');
  const event = { target: env.nodes[1], key: 'Enter', preventDefault() { this.prevented = true; }, stopPropagation() {} };
  env.events.keydown(event);
  assert.equal(event.prevented, true); assert.equal(env.selections.length, 1);
  const modified = { ...event, button: 0, ctrlKey: true, prevented: false };
  env.events.click(modified);
  assert.equal(modified.prevented, false); assert.equal(env.selections.length, 1);
});
test('reading activation enters editing and selects the exact source occurrence', async () => {
  const env = environment({ reading: true }); await env.prepare();
  assert.equal(await env.publication.locateCitation(env.nodes[1]), true);
  assert.equal(env.context.state.editing, true);
  assert.deepEqual(plain(env.selections[0].selection), selectionFor(second));
});

test('reader table repair and CRLF normalization retain the second original occurrence and Rust display', async () => {
  const original = source.replace('😀 中文', '| Column |\r\n|---|\r\n| value |\r\n\r\n😀 中文');
  const display = original.replace(/\r\n/g, '\n').replace('|---|', '| --- |');
  const rendered = fixture(display, 'fixed'), authoritative = fixture(original, 'original'), requests = [];
  const env = environment({reading: true, original, display, response: rendered, fetch: async (_, init) => {
    const body = JSON.parse(init.body); requests.push(body.content);
    return {ok: true, json: async () => body.content === display ? rendered : authoritative};
  }});
  assert.equal(await env.prepare(), 'Rust safe reader projection');
  assert.deepEqual(requests, [display, original]);
  assert.equal(env.nodes[1].dataset.readmdSourceFrom, String(authoritative.projection.citations[1].source.span.from));
  assert.equal(env.nodes[1].dataset.readmdSourceRevision, 'original');
  await env.prepare(); assert.equal(requests.length, 2); // A cache hit retains its binding identity.
  env.publication.applyLayout(env.pane); // Rebinding must retain the original mapping.
  assert.equal(await env.publication.locateCitation(env.nodes[1]), true);
  const at = authoritative.projection.citations[1].source.span.from;
  assert.deepEqual(plain(env.selections[0].selection), {anchor: original.slice(0, at).replace(/\r\n?/g, '\n').length, head: original.slice(0, at + 9).replace(/\r\n?/g, '\n').length});
  assert.equal(env.context.raw, original); assert.equal(env.context.tab.content, display);
});

test('reader rejects an original changed after a repaired projection was bound', async () => {
  const display = normalized, rendered = fixture(display, 'fixed'), authoritative = fixture(source, 'original');
  const env = environment({reading: true, original: source, display, response: rendered, fetch: async (_, init) => ({ok: true, json: async () => JSON.parse(init.body).content === display ? rendered : authoritative})});
  await env.prepare(); env.context.state.original += 'changed';
  assert.equal(await env.publication.locateCitation(env.nodes[1]), false);
  assert.equal(env.context.state.editing, false); assert.equal(env.selections.length, 0);
});

test('editing a dirty draft validates the exact preview source rather than its saved original', async () => {
  const draft = source + 'Unsaved draft.\r\n', response = fixture(draft, 'draft');
  const env = environment({response});
  env.context.raw = draft; env.context.tab.isDirty = true; env.context.tab.content = draft.replace(/\r\n/g, '\n');
  env.context.cmView.state.doc.toString = () => draft.replace(/\r\n/g, '\n');
  await env.publication.prepare(draft, env.context.state.file); env.publication.applyLayout(env.pane);
  assert.equal(await env.publication.locateCitation(env.nodes[1]), true);
  assert.deepEqual(plain(env.selections[0].selection), selectionFor(second));
});

test('reader refuses repaired/original typed disagreements and source changes during preparation', async t => {
  for (const kind of ['count', 'item', 'rich', 'context', 'order', 'original-changed']) await t.test(kind, async () => {
    const display = normalized, rendered = fixture(display, 'fixed'), authoritative = fixture(source, 'original');
    if (kind === 'count') authoritative.projection.citations.pop();
    if (kind === 'item') authoritative.projection.citations[1].source.items[0].cite.key = 'other';
    if (kind === 'rich') authoritative.projection.citations[1].rich[0].text = 'Other';
    if (kind === 'context') authoritative.projection.citations[1].source.context.in_note = true;
    if (kind === 'order') {authoritative.projection.citations.reverse(); authoritative.source_citations.reverse();}
    const env = environment({reading: true, original: source, display, response: rendered, fetch: async (_, init) => ({ok: true, json: async () => JSON.parse(init.body).content === display ? rendered : authoritative})});
    const pending = env.publication.prepare(display, env.context.state.file);
    if (kind === 'original-changed') env.context.state.original += 'new original';
    await pending; env.publication.applyLayout(env.pane);
    assert.equal(env.nodes[1].attributes.role, undefined);
    assert.equal(await env.publication.locateCitation(env.nodes[1]), false); assert.equal(env.selections.length, 0);
  });
});
test('reading activation refuses a fixed projection different from editable original', async () => {
  const env = environment({ reading: true }); await env.prepare(); env.context.state.original += 'author source';
  assert.equal(await env.publication.locateCitation(env.nodes[1]), false);
  assert.equal(env.context.state.editing, false); assert.equal(env.selections.length, 0);
});
test('reading activation rejects a tab switch or a new editor opening during await', async t => {
  for (const kind of ['tab', 'opening']) await t.test(kind, async () => {
    const env = environment({ reading: true }); await env.prepare();
    let release; const opened = new Promise(resolve => { release = resolve; });
    env.context.toggleEdit = async () => { env.context.state.editing = true; env.context.cmView = env.makeView(); env.context.editorOpenEpoch++; await opened; };
    const navigating = env.publication.locateCitation(env.nodes[1]);
    if (kind === 'tab') env.context.state.activeTabId = 'b'; else env.context.editorOpenEpoch++;
    release(); assert.equal(await navigating, false); assert.equal(env.selections.length, 0);
  });
});
test('late responses and cleared in-flight responses cannot replace current projection', async () => {
  const queued = [];
  const env = environment({ fetch: async () => new Promise(resolve => queued.push(resolve)) });
  const a = env.publication.prepare(source, 'Z:/paper.md');
  const next = source + 'new source'; env.context.raw = next; env.context.tab.content = next;
  const b = env.publication.prepare(next, 'Z:/paper.md');
  queued[1]({ ok: true, json: async () => fixture(next, 'new') }); await b;
  queued[0]({ ok: true, json: async () => fixture(source, 'old') }); await a;
  assert.equal(env.publication.revision(), 'new');
  const c = env.publication.prepare(next + ' again', 'Z:/paper.md'); env.publication.clear();
  queued[2]({ ok: true, json: async () => fixture(next + ' again', 'cleared') }); await c;
  assert.equal(env.publication.projection(), null);
});
test('returning to a cached source supersedes a request for a different source', async () => {
  const queued = [];
  const env = environment({ fetch: async () => new Promise(resolve => queued.push(resolve)) });
  const a = env.publication.prepare(source, 'Z:/paper.md');
  queued[0]({ ok: true, json: async () => fixture(source, 'a-before') }); await a;
  const next = source + 'other'; env.context.raw = next; env.context.tab.content = next;
  const b = env.publication.prepare(next, 'Z:/paper.md');
  env.context.raw = source; env.context.tab.content = source;
  const back = env.publication.prepare(source, 'Z:/paper.md'); assert.equal(queued.length, 3);
  queued[2]({ ok: true, json: async () => fixture(source, 'a-after') }); await back;
  queued[1]({ ok: true, json: async () => fixture(next, 'b-late') }); await b;
  assert.equal(env.publication.revision(), 'a-after');
});
test('a concurrent normalized completion request cannot steal the committed preview identity', async () => {
  const queued = [];
  const env = environment({ fetch: async () => new Promise(resolve => queued.push(resolve)) });
  const preview = env.publication.prepare(source, 'Z:/paper.md');
  const completion = env.publication.prepare(normalized, 'Z:/paper.md');
  queued[1]({ ok: true, json: async () => fixture(normalized, 'completion-lf') }); await completion;
  queued[0]({ ok: true, json: async () => fixture(source) }); await preview;
  env.publication.applyLayout(env.pane);
  assert.equal(await env.publication.locateCitation(env.nodes[1]), true);
  assert.deepEqual(plain(env.selections[0].selection), selectionFor(second));
});
test('bibliography restores typed numeric spacing including eleven after sanitization', () => {
  const env = environment(), entry = {style: {}}, section = {dataset: {cslLineSpacing: '11', cslEntrySpacing: '0'}, style: {}, querySelectorAll: () => [entry]};
  env.publication.applyLayout({querySelectorAll: selector => selector === '.readmd-bibliography' ? [section] : []});
  assert.deepEqual(section.style, {lineHeight: '11'}); assert.deepEqual(entry.style, {marginBottom: '0em'});
});

test('bibliography entry gaps multiply the actual line-spacing and omit the trailing gap', () => {
  for (const [line, entry, gap] of [[1, 0, 0], [1, 2, 2], [3, 0, 0], [3, 2, 6]]) {
    const env = environment(), nodes = [{style: {}}, {style: {}}];
    const section = {dataset: {cslLineSpacing: String(line), cslEntrySpacing: String(entry)}, style: {}, querySelectorAll: () => nodes};
    env.publication.applyLayout({querySelectorAll: selector => selector === '.readmd-bibliography' ? [section] : []});
    assert.equal(section.style.lineHeight, String(line));
    assert.equal(nodes[0].style.marginBottom, gap + 'em');
    assert.equal(nodes[1].style.marginBottom, '0em');
  }
});
test('bibliography never interprets style strings, invalid numbers or extreme spacing budgets', () => {
  const env = environment();
  for (const value of ['-1', '1.5', '11;color:red', 'Infinity', '4294967295']) {
    const entry = {style: {}}, section = {dataset: {cslLineSpacing: value, cslEntrySpacing: value}, style: {}, querySelectorAll: () => [entry]};
    env.publication.applyLayout({querySelectorAll: selector => selector === '.readmd-bibliography' ? [section] : []});
    assert.deepEqual(section.style, {}); assert.deepEqual(entry.style, {});
  }
});
test('a reply before the next debounced request cannot show diagnostics for an edited source', async () => {
  let release;
  const env = environment({fetch: async () => new Promise(resolve => {release = resolve;})});
  const preparing = env.publication.prepare(source, 'Z:/paper.md');
  env.context.raw += 'changed before another request';
  release({ok: true, json: async () => ({...fixture(), warnings: ['academic_missing_reference: old-key']})});
  await preparing; assert.deepEqual(env.toasts, []);
});
