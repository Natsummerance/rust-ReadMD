'use strict';
// Production frontend adapters in a VM with synthetic services/DOM only.
// These tests establish protocol ordering and state preservation; they make no
// native filesystem, durability, browser/CSP, or GUI acceptance claim.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { createHash, webcrypto } = require('node:crypto');
const ROOT = path.join(__dirname, '..');
const reviewSource = fs.readFileSync(path.join(ROOT, 'assets/js/editor/review.js'), 'utf8');
const adapterSource = fs.readFileSync(path.join(ROOT, 'assets/js/features/document-refactor.js'), 'utf8');
const OLD = 'C:/Synthetic/Refactor/Original.md', NEW = 'C:/Synthetic/Refactor/Renamed.md', REF = 'C:/Synthetic/Refactor/Links.md';
const WORKSPACE = 'C:/Synthetic/Refactor';
const BODY = '# Synthetic target\n\nKeep the review checkpoint.\n';
const LINKS = '😀 [first](Original.md#heading) and [second](Original.md?x=1)\r\n';
const SENTINEL = 'SYNTHETIC_PRIVATE_SENTINEL_NOT_A_CREDENTIAL';
const clone = value => value === undefined ? undefined : JSON.parse(JSON.stringify(value));
const lf = value => String(value || '').replace(/^\uFEFF/, '').replace(/\r\n?/g, '\n');
const sha = value => createHash('sha256').update(value).digest('hex');
const normalize = value => String(value || '').replace(/\\/g, '/').toLowerCase();
const scenario = (name, run) => test('document refactor adapter: ' + name, { timeout: 6000 }, run);
function deferred() { let resolve; const promise = new Promise(yes => { resolve = yes; }); return { promise, resolve }; }
function diskBytes(body, encoding = 'utf-8') { return encoding === 'utf-16le' ? Buffer.concat([Buffer.from([255, 254]), Buffer.from(body, 'utf16le')]) : Buffer.from(body); }
function file(at, body, encoding = 'utf-8') {
  return { path: at, dir: WORKSPACE, name: at.split('/').pop(), content: body, original: body,
    identity: 'synthetic-identity-' + sha(at + body), revision: sha(body), disk_revision: sha(diskBytes(body, encoding)), encoding,
    mtime: 17, size: diskBytes(body, encoding).length, fixes: [], stats: {} };
}
function snapshot(at, text, documentId, encoding = 'utf-8') {
  return { document_id: documentId, path: at, content: lf(text), revision: sha(lf(text)), disk_revision: sha(diskBytes(text, encoding)),
    generation: 3, review_revision: 7, last_event_id: 9, can_undo: true, can_redo: false,
    checkpoint: { generation: 2, review_revision: 4 }, changes: [] };
}
function destinationEdits(before) {
  const result = [], expression = /Original\.md/g;
  let match;
  while ((match = expression.exec(lf(before)))) result.push({ from: match.index, to: match.index + match[0].length, insert: 'Renamed.md' });
  return result;
}
function apply(text, edits) {
  let position = 0, next = '';
  for (const edit of edits) { next += text.slice(position, edit.from) + edit.insert; position = edit.to; }
  return next + text.slice(position);
}
class Element {
  constructor(tag) { this.tagName = tag; this.children = []; this.attributes = {}; this.dataset = {}; this.listeners = new Map(); this.textContent = ''; this.className = ''; this.isConnected = true; }
  append(...nodes) { for (const node of nodes) this.appendChild(node); }
  appendChild(node) { this.children.push(node); node.parentElement = this; return node; }
  setAttribute(name, value) { this.attributes[name] = String(value); }
  addEventListener(name, callback) { this.listeners.set(name, callback); }
  remove() { this.isConnected = false; if (this.parentElement) this.parentElement.children = this.parentElement.children.filter(child => child !== this); }
  click() { this.listeners.get('click')?.({ target: this }); }
}
function descendants(element) { return [element, ...element.children.flatMap(descendants)]; }
async function app(options = {}) {
  const referenceEncoding = options.utf16 ? 'utf-16le' : 'utf-8';
  const files = new Map([[OLD, file(OLD, BODY)], [REF, file(REF, LINKS, referenceEncoding)]]);
  const heads = new Map([[OLD, snapshot(OLD, BODY, 'synthetic-source')], [REF, snapshot(REF, LINKS, 'synthetic-reference', referenceEncoding)]]);
  if (options.noReferences) { files.delete(REF); heads.delete(REF); }
  const timeline = [], calls = [], toasts = [], events = [], modals = [], releases = [], diskBefore = clone([...files]);
  const source = { id: 'source', mode: 'file', source: 'file', path: OLD, dir: WORKSPACE, name: 'Original.md', title: 'Original.md',
    content: BODY, fixed: BODY, original: BODY, revision: sha(lf(BODY)), isDirty: false, editGeneration: 0, recoveryKey: 'file:source', originPath: OLD };
  const reference = { id: 'reference', mode: 'file', source: 'file', path: REF, dir: WORKSPACE, name: 'Links.md', title: 'Links.md',
    content: LINKS, fixed: LINKS, original: LINKS, revision: sha(LINKS), isDirty: false, editGeneration: 0, recoveryKey: 'file:reference' };
  const state = { tabs: options.noReferences ? [source] : [source, reference], activeTabId: source.id, editing: false, folder: WORKSPACE, folderFiles: [OLD, REF],
    file: OLD, revision: source.revision, original: BODY, fixed: BODY, ai: { privateCredential: SENTINEL } };
  const body = new Element('body'), listeners = new Map();
  const document = { body, title: '', addEventListener() {}, createElement: tag => new Element(tag),
    getElementById: id => descendants(body).find(element => element.id === id) || null };
  let serial = 0, previews = 0, receipt = null, pendingSaved = null, flushes = 0;
  const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => clone(data) });
  function makePlan(operationId) {
    return { ok: true, operation_id: operationId, plan_hash: sha('synthetic-plan-' + previews), source_path: OLD, target_path: NEW, diagnostics: [],
      files: [...files].map(([at, value]) => {
        const after = at === REF ? value.original.replace(/Original\.md/g, 'Renamed.md') : value.original;
        return { source_path: at, target_path: at === OLD ? NEW : at, before: value.original, after,
          encoding: value.encoding, before_disk_revision: value.disk_revision, after_disk_revision: file(at, after, value.encoding).disk_revision,
          edits: destinationEdits(value.original), review_head: clone(heads.get(at)) };
      }) };
  }
  function commit(plan) {
    const rows = [];
    for (const planned of plan.files) {
      const head = heads.get(planned.source_path), next = file(planned.target_path, planned.after, planned.encoding);
      const changed = lf(planned.before) !== lf(planned.after);
      const review = { ...clone(head), path: next.path, content: lf(next.original), revision: sha(lf(next.original)), disk_revision: next.disk_revision,
        generation: head.generation + (changed ? 1 : 0), review_revision: head.review_revision + (changed ? 1 : 0) };
      files.delete(planned.source_path); files.set(next.path, next); heads.delete(planned.source_path); heads.set(next.path, review);
      rows.push({ old_path: planned.source_path, path: next.path, identity: next.identity, before_disk_revision: planned.before_disk_revision, disk_revision: next.disk_revision, review });
    }
    receipt = { ok: true, state: 'committed', operation_id: plan.operation_id, plan_hash: plan.plan_hash, path: NEW, target_path: NEW, files: rows };
    timeline.push('disk:committed');
    return clone(receipt);
  }
  let lastPlan;
  async function apiFetch(url, request) {
    if (url === '/api/rename/refactor') {
      const value = JSON.parse(request.body); calls.push(clone(value)); timeline.push(value.action);
      if (value.action === 'preview') {
        previews++;
        const preflightCode = typeof options.previewError === 'function' ? options.previewError(previews) : options.previewError;
        if (preflightCode) return response({ ok: false, error_code: preflightCode, error: 'SYNTHETIC_BACKEND_MESSAGE_MUST_NOT_RENDER' }, 409);
        if (options.previewFailure?.(previews)) return response({ ok: false, operation_id: value.operation_id, error_code: 'rename_revision_conflict' }, 409);
        lastPlan = makePlan(value.operation_id); options.plan?.(lastPlan, previews);
        return response(lastPlan);
      }
      if (value.action === 'commit') {
        assert.equal(value.confirmed, true); assert.equal(value.plan_hash, lastPlan.plan_hash);
        assert.equal(pendingSaved.operation_id, value.operation_id, 'The exact intent is acknowledged before commit');
        if (options.commitWait) { options.commitWait.entered.resolve(); await options.commitWait.release.promise; }
        if (options.commitError) return response({ ok: false, error_code: options.commitError, error: 'SYNTHETIC_BACKEND_MESSAGE_MUST_NOT_RENDER' }, 409);
        if (options.notStarted) return response({ ok: false, state: 'not_started', operation_id: value.operation_id, plan_hash: value.plan_hash }, 409);
        if (options.rolledBack) {
          receipt = { ok: false, state: 'rolled_back', operation_id: value.operation_id, plan_hash: value.plan_hash, path: OLD, target_path: NEW,
            ...(options.rollbackCode ? { error_code: options.rollbackCode } : {}), files: lastPlan.files.map(planned => {
              const file = files.get(planned.source_path);
              return { old_path: file.path, path: file.path, identity: file.identity, before_disk_revision: file.disk_revision,
                disk_revision: file.disk_revision, review: clone(heads.get(file.path)) };
            }) };
          timeline.push('disk:rolled_back');
        } else commit(lastPlan);
        options.afterCommit?.({ source, reference, state, receipt, files });
        if (options.lostCommit) throw new Error('Synthetic dropped commit acknowledgment');
        return response(receipt);
      }
      if (value.action === 'status') {
        if (options.statusUnknown) return response({ state: 'recovery_pending', operation_id: value.operation_id, plan_hash: value.plan_hash });
        return response(receipt || { state: 'not_started', operation_id: value.operation_id, plan_hash: value.plan_hash });
      }
      if (value.action === 'abandon') {
        if (options.abandonFailure) throw new Error('Synthetic dropped tombstone acknowledgment');
        return response({ ok: false, state: 'abandoned', operation_id: value.operation_id, plan_hash: value.plan_hash }, 409);
      }
      throw Error('Unexpected refactor action');
    }
    if (url.startsWith('/api/file?')) {
      const query = new URL(url, 'https://synthetic.invalid').searchParams, at = query.get('p');
      timeline.push((query.get('meta') === '1' ? 'metadata:' : 'open:') + at);
      let result = clone(files.get(at));
      if (query.get('meta') !== '1') result = options.reopen?.(result, at) ?? result;
      return response(result || { ok: false }, result ? 200 : 404);
    }
    if (url === '/api/review') {
      const value = JSON.parse(request.body), head = [...heads.values()].find(head => head.path === value.path || head.document_id === value.document_id);
      if (value.action === 'open') return response(head);
      if (value.action === 'read') return response(head);
      if (value.action === 'resume') return response({ ok: true, found: Boolean(head), snapshot: head });
      throw Error('A refactor must not send a local edit/history operation');
    }
    throw Error('Unexpected URL ' + url);
  }
  const window = { crypto: { subtle: webcrypto.subtle, randomUUID: () => '00000000-0000-4000-8000-' + String(++serial).padStart(12, '0') },
    i18n: { t: (name, params) => options.messages?.[name] || name + (params ? ':' + JSON.stringify(params) : '') },
    addEventListener(name, callback) { listeners.set(name, callback); },
    dispatchEvent(event) {
      events.push(event);
      if (event.type === 'readmd:document-transaction') {
        timeline.push('transaction:' + event.detail.tabId);
        options.event?.(event, { source, reference, review: window.ReadMDReview });
      }
    },
    ReadMDAiHarness: { async reserveTabForRename(tab) { timeline.push('ai:reserve:' + tab.id); return options.aiFailure !== tab.id; }, releaseTab(tab) { releases.push('ai:' + tab.id); timeline.push('ai:release:' + tab.id); } },
    ReadMDSession: {
      async prepareRename(tab, pending) {
        timeline.push('session:prepare'); tab._renamePending = clone(pending);
        assert(!JSON.stringify(pending).includes(SENTINEL)); assert(!JSON.stringify(pending).includes(BODY)); assert(!JSON.stringify(pending).includes(LINKS));
        if (options.prepareFailure) return false;
        pendingSaved = clone(pending); return true;
      },
      async flush() {
        flushes++; timeline.push('session:settle');
        if (options.flushFailure) return false;
        if (source._renamePending && options.uncertainPrepare) { pendingSaved = clone(source._renamePending); return true; }
        assert.equal(source._renamePending, undefined);
        return true;
      },
      uncertainRename: () => options.uncertainPrepare === true, persist() { timeline.push('session:mirror'); },
    },
    ReadMDModal: { setGuard() {}, close() {}, open(modal) {
      modals.push(modal); timeline.push('confirm:open');
      const buttons = descendants(modal).filter(element => element.tagName === 'button');
      if (options.modal) options.modal({ modal, cancel: buttons[0], confirm: buttons[1], source, reference, state });
      else queueMicrotask(() => buttons[options.cancel ? 0 : 1].click());
    } },
  };
  const context = vm.createContext({ window, document, state, console, TextEncoder, Map, Set, WeakMap, WeakSet, URL, Object, Number, JSON, Promise, String, Array, Uint32Array,
    crypto: window.crypto, CustomEvent: class { constructor(type, init) { this.type = type; this.detail = init?.detail; } },
    getActiveTab: () => state.tabs.find(tab => tab.id === state.activeTabId), normalizePath: normalize,
    showToast: text => toasts.push(text), apiFetch,
    syncStateFromActiveTab() { const tab = state.tabs.find(tab => tab.id === state.activeTabId); Object.assign(state, { file: tab.path, revision: tab.revision, original: tab.original, fixed: tab.content }); },
    setFileTitle() {}, renderTabsBar() { timeline.push('tabs:render'); }, renderActiveTab: async () => { timeline.push('reader:render'); },
    renderFolderList() { timeline.push('folder:render'); },
  });
  vm.runInContext(reviewSource, context, { filename: 'review.js' });
  vm.runInContext(adapterSource, context, { filename: 'document-refactor.js' });
  const review = window.ReadMDReview, adapter = window.ReadMDRefactor;
  if (options.ensure !== false) { await review.ensure(source, BODY); if (!options.noReferences) await review.ensure(reference, LINKS); }
  return { source, reference, state, window, context, review, adapter, files, heads, timeline, calls, toasts, events, modals, releases, diskBefore,
    options, commit, get receipt() { return receipt; }, get pendingSaved() { return pendingSaved; }, get flushes() { return flushes; } };
}

scenario('unsupported platform stops after read-only preview without confirmation or durable intent', async () => {
  const a = await app({ plan(value) { value.commit_supported = false; } });
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), false);
  assert.deepEqual(a.calls.map(call => call.action), ['preview']);
  assert.equal(a.modals.length, 0);
  assert.equal(a.pendingSaved, null);
  assert(!a.timeline.some(step => /session:prepare|ai:reserve/.test(step)));
  assert.equal(a.source.path, OLD);
  assert.equal(a.source._renamePending, undefined);
  assert.equal(a.adapter.busy(), false);
  assert.equal(a.files.get(OLD).original, BODY);
  assert.equal(a.files.get(REF).original, LINKS);
  assert.deepEqual(a.toasts, ['rename.groupUnavailable']);
});

scenario('confirms the exact second preview and adopts all verified files after intent persistence', async () => {
  const a = await app({ event(_event, { source, reference, review }) {
    assert.equal(source.path, NEW); assert.match(reference.original, /Renamed\.md/);
    assert.equal(review.snapshot(reference).content, lf(reference.original), 'Events observe every acknowledged group head');
  } });
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), true);
  assert.deepEqual(a.calls.map(call => call.action), ['preview', 'preview', 'commit']);
  assert.equal(a.calls[0].workspace_root, WORKSPACE);
  assert(a.timeline.indexOf('session:prepare') < a.timeline.indexOf('commit'));
  assert(a.timeline.indexOf('open:' + REF) < a.timeline.indexOf('transaction:reference'));
  assert(a.timeline.indexOf('session:settle') < a.timeline.indexOf('ai:release:source'));
  assert.equal(a.source.recoveryKey, 'file:source'); assert.equal(a.reference.recoveryKey, 'file:reference');
  assert.equal(a.review.snapshot(a.source).document_id, 'synthetic-source'); assert.equal(a.review.snapshot(a.reference).document_id, 'synthetic-reference');
  assert.deepEqual(clone(a.review.snapshot(a.reference).checkpoint), { generation: 2, review_revision: 4 });
  assert.equal(a.reference.original.includes('\r\n'), true); assert.equal(a.reference.isDirty, false); assert.equal(a.state.file, NEW);
  assert.equal(a.adapter.busy(), false); assert.equal(a.source._renamePending, undefined);
  assert.deepEqual(clone(a.state.folderFiles), [NEW, REF]);
  assert(a.timeline.indexOf('session:settle') < a.timeline.indexOf('folder:render'));
  const code = descendants(a.modals[0]).filter(element => element.tagName === 'code');
  assert.deepEqual(code.map(element => element.textContent), ['Original.md', 'Renamed.md', 'Original.md', 'Renamed.md']);
  assert.equal(descendants(a.modals[0]).some(element => 'innerHTML' in element), false);
});

for (const flag of ['isDirty', 'unsavedCreation', '_reviewRestorePending', 'externalChanged']) scenario(flag + ' in an affected background tab preserves every body and prevents commit', async () => {
  const a = await app(); a.reference[flag] = true;
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), false);
  assert.equal(a.calls.some(call => call.action === 'commit'), false); assert.deepEqual(clone([...a.files]), a.diskBefore);
  assert.equal(a.reference.content, LINKS); assert.equal(a.adapter.busy(), false); assert.equal(a.source._renameInFlight, undefined);
});

scenario('an affected active editor is preserved and no implicit save or exit occurs', async () => {
  const a = await app(); a.state.activeTabId = a.reference.id; a.state.editing = true;
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), false);
  assert.equal(a.state.editing, true); assert.equal(a.reference.content, LINKS); assert.equal(a.calls.some(call => call.action === 'commit'), false);
});

scenario('missing selected workspace never falls back to the opened file parent', async () => {
  const a = await app(); a.state.folder = null;
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), false); assert.equal(a.calls.length, 0);
});

scenario('failed preview releases its source reservation and does not publish an intent', async () => {
  const a = await app({ previewFailure: count => count === 1 });
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), false);
  assert.equal(a.adapter.busy(), false); assert.equal(a.pendingSaved, null); assert.equal(a.source._renameInFlight, undefined);
});

scenario('a changed affected roster after reservation rejects the whole second preview', async () => {
  const a = await app({ plan(plan, count) { if (count === 2) plan.files.pop(); } });
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), false);
  assert.equal(a.pendingSaved, null); assert.equal(a.releases.length, 2); assert.equal(a.adapter.busy(), false);
});

scenario('a stale review head in the confirmed preview never reaches commit', async () => {
  const a = await app({ plan(plan, count) { if (count === 2) plan.files[1].review_head.generation++; } });
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), false); assert.equal(a.modals.length, 0); assert.equal(a.pendingSaved, null);
});

scenario('Cancel releases all reservations without persisting an intent or changing disk', async () => {
  const a = await app({ cancel: true });
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), false);
  assert.equal(a.pendingSaved, null); assert.deepEqual(clone([...a.files]), a.diskBefore); assert.equal(a.releases.length, 2); assert.equal(a.adapter.busy(), false);
});

scenario('an AI reservation refusal releases earlier owners without stopping jobs', async () => {
  const a = await app({ aiFailure: 'reference' });
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), false);
  assert.deepEqual(a.releases, ['ai:source']); assert.equal(a.pendingSaved, null); assert.equal(a.adapter.busy(), false);
});

scenario('lost committed acknowledgment reconciles status without sending another commit', async () => {
  const a = await app({ lostCommit: true });
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), true);
  assert.equal(a.calls.filter(call => call.action === 'commit').length, 1); assert.equal(a.calls.filter(call => call.action === 'status').length, 1);
  assert.equal(a.source.path, NEW); assert.equal(a.adapter.busy(), false);
});

scenario('unknown outcome retains every reservation and the same operation for explicit recovery', async () => {
  const a = await app({ lostCommit: true, statusUnknown: true });
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), false);
  const pending = clone(a.source._renamePending);
  assert.equal(a.adapter.busy(), true); assert.equal(a.releases.length, 0); assert.equal(a.source.path, OLD); assert.equal(a.reference.content, LINKS);
  a.options.statusUnknown = false;
  assert.equal(await a.adapter.recover(a.source), true);
  assert.equal(a.calls.filter(call => call.action === 'commit').length, 1); assert.equal(pending.operation_id, a.receipt.operation_id); assert.equal(a.source.path, NEW);
});

scenario('failed settlement CAS retains adopted tabs and recovery is idempotent', async () => {
  const a = await app({ flushFailure: true });
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), false);
  const id = a.source._renamePending.operation_id, transactions = a.events.filter(event => event.type === 'readmd:document-transaction').length;
  assert.equal(a.source.path, NEW); assert.match(a.reference.content, /Renamed\.md/); assert.equal(a.adapter.busy(), true); assert.equal(a.releases.length, 0);
  a.options.flushFailure = false;
  assert.equal(await a.adapter.recover(a.source), true);
  assert.equal(a.events.filter(event => event.type === 'readmd:document-transaction').length, transactions);
  assert.equal(a.receipt.operation_id, id); assert.equal(a.calls.filter(call => call.action === 'commit').length, 1); assert.equal(a.adapter.busy(), false);
});

scenario('raw-byte mismatch in the last reopened file prevents every tab adoption', async () => {
  const a = await app({ reopen(value, at) { if (at === REF) value.disk_revision = sha('Synthetic different raw bytes'); return value; } });
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), false);
  assert.equal(a.source.path, OLD); assert.equal(a.reference.original, LINKS); assert.equal(a.review.snapshot(a.source).path, OLD);
  assert.equal(a.events.some(event => event.type === 'readmd:document-transaction'), false); assert.equal(a.adapter.busy(), true);
  assert.equal(a.files.has(NEW), true, 'A terminal committed operation is not silently rolled back');
});

scenario('foreign reference document identity rejects the complete local group', async () => {
  const a = await app({ afterCommit({ receipt }) { receipt.files[1].review.document_id = 'synthetic-foreign'; } });
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), false);
  assert.equal(a.source.path, OLD); assert.equal(a.reference.original, LINKS); assert.equal(a.adapter.busy(), true);
});

scenario('a newer local draft arriving during commit is retained without partial adoption', async () => {
  const entered = deferred(), release = deferred(), a = await app({ commitWait: { entered, release } });
  const work = a.adapter.rename(a.source, 'Renamed.md'); await entered.promise;
  a.reference.content = a.reference.fixed = LINKS + 'Synthetic newer local draft\n'; a.reference.isDirty = true; a.reference.editGeneration++;
  release.resolve(); assert.equal(await work, false);
  assert.equal(a.source.path, OLD); assert.match(a.reference.content, /newer local draft/); assert.equal(a.review.snapshot(a.source).path, OLD); assert.equal(a.adapter.busy(), true);
});

scenario('an unstarted outcome requires a durable abandonment acknowledgment', async () => {
  const a = await app({ notStarted: true, abandonFailure: true });
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), false);
  assert.equal(a.adapter.busy(), true); assert(a.source._renamePending); assert.equal(a.releases.length, 0);
  a.options.abandonFailure = false;
  assert.equal(await a.adapter.recover(a.source), true);
  assert.equal(a.calls.filter(call => call.action === 'commit').length, 1); assert.equal(a.calls.filter(call => call.action === 'abandon').length, 2);
  assert.equal(a.source.path, OLD); assert.deepEqual(clone([...a.files]), a.diskBefore); assert.equal(a.adapter.busy(), false);
});

scenario('busy/whenIdle fences span confirmation and uncertain settlement', async () => {
  let confirm;
  const opened = deferred(), a = await app({ modal(value) { confirm = value.confirm; opened.resolve(); }, flushFailure: true });
  const work = a.adapter.rename(a.source, 'Renamed.md'); await opened.promise;
  let resumed = false; const idle = a.adapter.whenIdle().then(() => { resumed = true; });
  await Promise.resolve(); assert.equal(a.adapter.busy(), true); assert.equal(resumed, false);
  confirm.click(); assert.equal(await work, false); await Promise.resolve(); assert.equal(resumed, false);
  a.options.flushFailure = false; assert.equal(await a.adapter.recover(a.source), true); await idle; assert.equal(resumed, true);
});

scenario('cold descriptor recovery verifies every file and awaits the parent session CAS before release', async () => {
  const warm = await app({ flushFailure: true }); await warm.adapter.rename(warm.source, 'Renamed.md');
  const a = await app({ ensure: false });
  a.files.clear(); for (const [at, value] of warm.files) a.files.set(at, clone(value));
  const pending = clone(warm.source._renamePending), descriptor = { key: 'file:source', path: OLD, revision: pending.expected_revision, renamePending: pending };
  // The synthetic cold status uses the same terminal receipt via a new VM API.
  const originalFetch = a.context.apiFetch;
  a.context.apiFetch = async (url, request) => url === '/api/rename/refactor' ? { ok: true, json: async () => clone(warm.receipt) } : originalFetch(url, request);
  const result = await a.adapter.restoreDescriptor(descriptor);
  assert.equal(result.settled, true); assert.equal(result.descriptor.path, NEW); assert.equal(result.descriptor.renamePending, null);
  assert.equal(descriptor.renamePending.operation_id, pending.operation_id); assert.equal(a.adapter.busy(), true);
  a.adapter.finishRestore(false); assert.equal(a.adapter.busy(), true);
  a.adapter.finishRestore(true); assert.equal(a.adapter.busy(), false);
});

scenario('group validation prevents an earlier valid snapshot from being applied before a later invalid one', async () => {
  const a = await app(), beforeA = a.review.refactorState(a.source), beforeB = a.review.refactorState(a.reference);
  const first = { ...clone(a.review.snapshot(a.source)), path: NEW };
  const second = { ...clone(a.review.snapshot(a.reference)), content: 'Different\n', revision: sha('Different\n'), generation: 4, review_revision: 8, document_id: 'foreign' };
  let applied = false;
  assert.equal(a.review.applyRefactorGroup([{ tab: a.source, before: beforeA, snapshot: first, edits: [] }, { tab: a.reference, before: beforeB, snapshot: second }], () => { applied = true; }), false);
  assert.equal(applied, false); assert.equal(a.review.snapshot(a.source).path, OLD); assert.equal(a.reference.original, LINKS);
  assert.equal(a.review.renamed(a.reference, second), false, 'Identity-only rename still rejects changed content');
});

scenario('UTF-16 BOM bytes, CRLF decoded revisions and LF review revisions remain distinct through recovery', async () => {
  const a = await app({ utf16: true, flushFailure: true });
  const originalDisk = a.files.get(REF).disk_revision;
  assert.notEqual(originalDisk, a.reference.revision); assert.notEqual(a.reference.revision, a.review.snapshot(a.reference).revision);
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), false);
  assert.equal(a.reference.encoding, 'utf-16le'); assert(a.reference.original.includes('\r\n'));
  assert.equal(a.reference.revision, sha(a.reference.original));
  assert.equal(a.files.get(REF).disk_revision, sha(diskBytes(a.reference.original, 'utf-16le')));
  assert.equal(a.review.snapshot(a.reference).revision, sha(lf(a.reference.original)));
  a.options.flushFailure = false; assert.equal(await a.adapter.recover(a.source), true); assert.equal(a.adapter.busy(), false);
});

scenario('a selected-workspace rename with no reference edits still settles the source identity', async () => {
  const a = await app({ noReferences: true });
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), true);
  assert.equal(a.source.path, NEW); assert.equal(a.source.original, BODY); assert.equal(a.review.snapshot(a.source).document_id, 'synthetic-source');
  assert.equal(a.events.some(event => event.type === 'readmd:document-transaction'), false);
});

scenario('a cached editor state is mapped with exact edits and a non-history annotation', async () => {
  const a = await app(), mapped = { doc: { toString: () => lf(LINKS).replace(/Original\.md/g, 'Renamed.md') }, historyToken: 'synthetic-existing-history' };
  const updates = [];
  a.window.ReadMDCodeMirror = { Transaction: { userEvent: { of: value => ({ userEvent: value }) }, addToHistory: { of: value => ({ addToHistory: value }) } } };
  a.reference.editorState = { doc: { toString: () => lf(LINKS) }, historyToken: 'synthetic-existing-history', update(spec) { updates.push(clone(spec)); return { state: mapped }; } };
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), true);
  assert.equal(a.reference.editorState, mapped); assert.equal(a.reference.editorState.historyToken, 'synthetic-existing-history');
  assert(updates.length >= 1); assert.deepEqual(updates[0].changes, destinationEdits(LINKS));
  assert.deepEqual(updates[0].annotations, [{ userEvent: 'review.remote' }, { addToHistory: false }]);
});

scenario('an uncertain intent persistence is reconciled before abandoning the unsent operation', async () => {
  const a = await app({ prepareFailure: true, uncertainPrepare: true });
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), false);
  assert.equal(a.adapter.busy(), true); assert(a.source._renamePending); assert.equal(a.calls.some(call => call.action === 'commit'), false);
  assert.equal(await a.adapter.recover(a.source), true);
  assert(a.timeline.lastIndexOf('session:settle') > a.timeline.indexOf('abandon'));
  assert.equal(a.calls.some(call => call.action === 'commit'), false); assert.equal(a.source.path, OLD); assert.equal(a.adapter.busy(), false);
});

scenario('a cold unstarted virtual source is rebound only after acknowledged abandonment and authorized reopen', async () => {
  const a = await app({ ensure: false, noReferences: true });
  a.source._renamePending = { refactor: true, operation_id: 'refactor-synthetic-cold', plan_hash: sha('synthetic-cold-plan'), old_path: OLD,
    new_stem: 'Renamed', workspace_root: WORKSPACE, expected_revision: a.source.revision, expected_disk_revision: a.files.get(OLD).disk_revision };
  a.source.mode = 'virtual'; a.source.isVirtual = true; a.source.path = null; a.source.original = a.source.content = a.source.fixed = '';
  assert.equal(await a.adapter.recover(a.source), true);
  assert.equal(a.source.mode, 'file'); assert.equal(a.source.path, OLD); assert.equal(a.source.original, BODY); assert.equal(a.source._renamePending, undefined);
  assert.deepEqual(a.calls.map(call => call.action), ['status', 'abandon']);
});

scenario('cold recovery rejects a source receipt from another durable review document', async () => {
  const warm = await app({ flushFailure: true }); await warm.adapter.rename(warm.source, 'Renamed.md');
  const a = await app({ ensure: false }), receipt = clone(warm.receipt); receipt.files[0].review.document_id = 'synthetic-foreign-source';
  const pending = clone(warm.source._renamePending), item = { key: 'file:source', path: OLD, revision: pending.expected_revision, renamePending: pending };
  a.context.apiFetch = async () => ({ ok: true, json: async () => receipt });
  const result = await a.adapter.restoreDescriptor(item);
  assert.equal(result.settled, false); assert.equal(result.descriptor, item); assert.equal(a.adapter.busy(), true);
});

scenario('replacement with identical decoded and raw bytes still fails the receipt identity check', async () => {
  const a = await app({ reopen(value, at) { if (at === REF) value.identity = 'synthetic-replaced-object-with-equal-bytes'; return value; } });
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), false);
  assert.equal(a.source.path, OLD); assert.equal(a.reference.original, LINKS); assert.equal(a.adapter.busy(), true);
  assert.equal(a.events.some(event => event.type === 'readmd:document-transaction'), false);
});

scenario('suspending a cancelled cold restore cannot release live owners or an outstanding status read', async () => {
  const a = await app({ ensure: false }), status = deferred();
  const pending = { refactor: true, operation_id: 'refactor-synthetic-unresolved', plan_hash: sha('synthetic-unresolved'), old_path: OLD,
    new_stem: 'Renamed', workspace_root: WORKSPACE, expected_revision: a.source.revision, expected_disk_revision: a.files.get(OLD).disk_revision };
  a.context.apiFetch = async () => { await status.promise; return { ok: true, json: async () => ({ state: 'recovery_pending', operation_id: pending.operation_id, plan_hash: pending.plan_hash }) }; };
  const work = a.adapter.restoreDescriptor({ key: 'file:source', path: OLD, renamePending: pending });
  assert.equal(a.adapter.suspendRestore(), false); assert.equal(a.adapter.busy(), true);
  status.resolve(); assert.equal((await work).settled, false);
  assert.equal(a.adapter.suspendRestore(), true); assert.equal(a.adapter.busy(), false);
  const warm = await app({ flushFailure: true }); await warm.adapter.rename(warm.source, 'Renamed.md');
  assert.equal(warm.adapter.suspendRestore(), false); assert.equal(warm.adapter.busy(), true);
});

for (const stage of [1, 2]) scenario('standard preflight ApiError without an operation ID at preview ' + stage + ' is a localized rejection, not recovery', async () => {
  const a = await app({ previewError: count => count === stage ? 'rename_revision_conflict' : null });
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), false);
  assert.equal(a.source._renamePending, undefined); assert.equal(a.source._renameInFlight, undefined); assert.equal(a.adapter.busy(), false);
  assert.equal(a.pendingSaved, null); assert.deepEqual(clone([...a.files]), a.diskBefore);
  assert.deepEqual(a.toasts, ['review.conflict']);
  assert.equal(a.calls.some(call => ['commit', 'status', 'abandon'].includes(call.action)), false);
  assert.equal(a.toasts.some(text => /recovery|SYNTHETIC_BACKEND_MESSAGE/.test(text)), false);
});

for (const code of ['document_operation_unsupported', 'rename_read_only', 'rename_metadata_unsupported', 'document_operation_access_denied']) scenario(code + ' uses the translated safety message and remains retryable without pending state', async () => {
  const french = JSON.parse(fs.readFileSync(path.join(ROOT, 'assets/i18n/fr.json'), 'utf8'))['rename.safeUnsupported'];
  const a = await app({ previewError: code, messages: { 'rename.safeUnsupported': french } });
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), false); assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), false);
  assert.deepEqual(a.toasts, [french, french]); assert.equal(a.adapter.busy(), false); assert.equal(a.source._renamePending, undefined);
  assert.deepEqual(clone([...a.files]), a.diskBefore); assert.equal(a.calls.every(call => call.action === 'preview'), true);
});

scenario('a commit ApiError without a receipt retains its localized reason through exact status and durable abandonment', async () => {
  const french = JSON.parse(fs.readFileSync(path.join(ROOT, 'assets/i18n/fr.json'), 'utf8'))['rename.safeUnsupported'];
  const a = await app({ commitError: 'document_operation_unsupported', messages: { 'rename.safeUnsupported': french } });
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), false);
  assert.deepEqual(a.calls.map(call => call.action), ['preview', 'preview', 'commit', 'status', 'abandon']);
  assert.deepEqual(a.toasts, [french]); assert.equal(a.source._renamePending, undefined); assert.equal(a.adapter.busy(), false);
  assert.deepEqual(clone([...a.files]), a.diskBefore); assert.equal(a.flushes, 1);
});

for (const code of [null, 'review_save_conflict', 'document_operation_unsupported']) scenario('rolled-back settlement reports ' + (code || 'a conflict') + ' and never claims rename success', async () => {
  const messages = JSON.parse(fs.readFileSync(path.join(ROOT, 'assets/i18n/fr.json'), 'utf8'));
  const a = await app({ rolledBack: true, rollbackCode: code, messages });
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), false);
  assert.equal(a.source.path, OLD); assert.equal(a.reference.original, LINKS); assert.equal(a.source._renamePending, undefined); assert.equal(a.adapter.busy(), false);
  assert.deepEqual(clone([...a.files]), a.diskBefore);
  assert.deepEqual(clone(a.state.folderFiles), [OLD, REF]);
  const expected = code === 'document_operation_unsupported' ? messages['rename.safeUnsupported'] : code ? messages['error.' + code] : messages['review.conflict'];
  assert.deepEqual(a.toasts, [expected]); assert.equal(a.toasts.some(text => text.startsWith('toast.renamedTo')), false);
});

scenario('a rolled-back outcome recovered after a lost acknowledgment reports failure after durable local settlement', async () => {
  const a = await app({ rolledBack: true, rollbackCode: 'document_operation_unsupported', lostCommit: true, statusUnknown: true });
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), false); assert.equal(a.adapter.busy(), true);
  a.options.statusUnknown = false; assert.equal(await a.adapter.recover(a.source), true);
  assert.equal(a.toasts.at(-1), 'rename.safeUnsupported'); assert.equal(a.source.path, OLD); assert.equal(a.adapter.busy(), false);
  assert.equal(a.toasts.some(text => text.startsWith('toast.renamedTo')), false);
});

scenario('a rejected commit code cannot release an unknown operation or claim its original is safe before reconciliation', async () => {
  const a = await app({ commitError: 'document_operation_unsupported', statusUnknown: true });
  assert.equal(await a.adapter.rename(a.source, 'Renamed.md'), false);
  assert.equal(a.adapter.busy(), true); assert(a.source._renamePending); assert.deepEqual(a.toasts, ['error.review_needs_recovery']);
  a.options.statusUnknown = false; assert.equal(await a.adapter.recover(a.source), true);
  assert.equal(a.toasts.at(-1), 'rename.safeUnsupported'); assert.equal(a.adapter.busy(), false);
});

scenario('cold rollback notification waits for the parent final session acknowledgment', async () => {
  const warm = await app({ rolledBack: true, rollbackCode: 'document_operation_unsupported', lostCommit: true, statusUnknown: true });
  await warm.adapter.rename(warm.source, 'Renamed.md');
  const a = await app({ ensure: false }), pending = clone(warm.source._renamePending), originalFetch = a.context.apiFetch;
  a.context.apiFetch = async (url, request) => url === '/api/rename/refactor' ? { ok: true, json: async () => clone(warm.receipt) } : originalFetch(url, request);
  const result = await a.adapter.restoreDescriptor({ key: 'file:source', path: OLD, renamePending: pending });
  assert.equal(result.settled, true); assert.deepEqual(a.toasts, []);
  a.adapter.finishRestore(false); assert.deepEqual(a.toasts, []);
  a.adapter.finishRestore(true); assert.deepEqual(a.toasts, ['rename.safeUnsupported']); assert.equal(a.adapter.busy(), false);
});

scenario('the safety message is explicitly translated and installed in all 46 locales', () => {
  const translations = JSON.parse(fs.readFileSync(path.join(ROOT, 'tools/i18n-v006-rename.json'), 'utf8'))['rename.safeUnsupported'];
  const locales = Object.keys(JSON.parse(fs.readFileSync(path.join(ROOT, 'assets/i18n/meta.json'), 'utf8')));
  assert.equal(locales.length, 46); assert.deepEqual(Object.keys(translations).sort(), locales.sort());
  for (const locale of locales) {
    const actual = JSON.parse(fs.readFileSync(path.join(ROOT, 'assets/i18n', locale + '.json'), 'utf8'))['rename.safeUnsupported'];
    assert.equal(actual, translations[locale]); assert.equal(typeof actual, 'string'); assert(actual.length > 20);
    if (locale !== 'en') assert.notEqual(actual, translations.en, locale + ' must not copy the English fallback');
  }
});
