'use strict';
/* Durable review is owned by Rust. This adapter only orders editor operations
   and projects acknowledged ranges into the current, possibly newer, draft. */
window.ReadMDReview = (() => {
  const records = new WeakMap(), views = new WeakMap(), effects = new WeakMap(), restores = new WeakMap();
  const MAX_BYTES = 4 * 1024 * 1024;
  let serial = 0;
  const lf = text => String(text ?? '').replace(/^\uFEFF/, '').replace(/\r\n?/g, '\n');
  const activeTab = () => typeof getActiveTab === 'function' ? getActiveTab() : null;
  const activeView = () => window.cmView || null;
  const callId = () => 'review-' + (window.crypto?.randomUUID?.() || (Date.now().toString(36) + '-' + (++serial).toString(36)));
  const t = key => window.i18n?.t(key) || key;
  function failure(code, message) { const error = new Error(message || code); error.code = code; return error; }
  function message(error) {
    const code = error?.code || '';
    const key='error.'+code, localized=t(key);
    if (code && localized !== key) return localized;
    if (code === 'cancelled') return t('ai.stopped');
    return t(/budget|limit/.test(code) ? 'review.limit' : /conflict|stale|recovery/.test(code) ? 'review.conflict' : /progress|busy/.test(code) ? 'review.busy' : 'review.persistenceFailed');
  }
  function notifyError(error) { if (typeof showToast === 'function') showToast(message(error), 6000); }
  function source(tab) {
    if (tab === activeTab() && typeof state !== 'undefined' && state.editing) {
      const view = activeView();
      if (view && views.get(view)?.tab === tab) return view.state.doc.toString();
      const area = document.getElementById('edit-area');
      if (area && !view) return lf(area.value);
    }
    return lf(tab?.isDirty ? tab.content : (tab?.original ?? tab?.content ?? ''));
  }
  function persistedText(tab, text) {
    const original = tab.original || '';
    if (original.includes('\r\n') && !/(^|[^\r])\n/.test(original)) return text.replace(/\n/g, '\r\n');
    if (original.includes('\r') && !original.includes('\n')) return text.replace(/\n/g, '\r');
    return text;
  }
  function record(tab, initial) {
    if (!tab) return null;
    let rec = records.get(tab);
    if (!rec) {
      const text = lf(initial ?? source(tab));
      const identity = tab.path || tab.reviewPath || ('draft:' + tab.id);
      if (!tab.path) tab.reviewPath = identity;
      rec = { tab, path: identity, local: text, base: text, snapshot: null,
        queue: [], processing: null, opening: null, error: null, view: null, saving: false, renameEpoch: 0,
        recoveryDraft: typeof tab._recoveryContent === 'string' ? { content: tab._recoveryContent, review: tab._recoveryReview } : null,
        unsupported: new TextEncoder().encode(text).byteLength > MAX_BYTES };
      records.set(tab, rec);
    }
    return rec;
  }
  async function request(url, body) {
    const response = await apiFetch(url, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
    const data = await response.json().catch(() => ({}));
    if (!response.ok || data.ok === false || data.error) throw failure(data.error_code || data.code || 'review_persistence_failed', data.error || ('HTTP ' + response.status));
    const snapshot = data.snapshot || data.review || data;
    if (!snapshot.document_id || typeof snapshot.content !== 'string' || !Number.isInteger(snapshot.generation) || !Number.isInteger(snapshot.review_revision) || !Array.isArray(snapshot.changes)) throw failure('review_persistence_failed');
    if (lf(snapshot.content) !== snapshot.content) throw failure('review_persistence_failed');
    return snapshot;
  }
  function apply(text, edits) {
    let cursor = 0, result = '';
    for (const edit of edits) {
      if (!Number.isInteger(edit.from) || !Number.isInteger(edit.to) || edit.from < cursor || edit.to < edit.from || edit.to > text.length || typeof edit.insert !== 'string') throw failure('review_conflict');
      result += text.slice(cursor, edit.from) + edit.insert; cursor = edit.to;
    }
    return result + text.slice(cursor);
  }
  // A boundary diff never searches/rebinds source text. Expand surrogate
  // boundaries so the Rust UTF-16 validator receives complete characters.
  function diff(before, after) {
    if (before === after) return [];
    let from = 0, to = before.length, end = after.length;
    while (from < to && from < end && before.charCodeAt(from) === after.charCodeAt(from)) from++;
    while (to > from && end > from && before.charCodeAt(to - 1) === after.charCodeAt(end - 1)) { to--; end--; }
    if (from > 0 && /[\uDC00-\uDFFF]/.test(before[from] || after[from] || '')) from--;
    if (to < before.length && /[\uDC00-\uDFFF]/.test(before[to])) { to++; end++; }
    return [{ from, to, insert: after.slice(from, end) }];
  }
  function documentEdits(before, candidate) {
    before = lf(before); candidate = lf(candidate);
    const boundary = diff(before, candidate);
    if (!boundary.length) return [];
    const part = boundary[0], a = before.slice(part.from, part.to).match(/[^\n]*\n|[^\n]+$/g) || [], b = part.insert.match(/[^\n]*\n|[^\n]+$/g) || [];
    if ((a.length + 1) * (b.length + 1) > 262144) throw failure('review_budget');
    const width = b.length + 1, scores = new Uint32Array((a.length + 1) * width);
    for (let i = a.length - 1; i >= 0; i--) for (let j = b.length - 1; j >= 0; j--) scores[i * width + j] = a[i] === b[j] ? 1 + scores[(i + 1) * width + j + 1] : Math.max(scores[(i + 1) * width + j], scores[i * width + j + 1]);
    let i = 0, j = 0, position = part.from, pending = null;
    const edits = [], commit = () => { if (pending) { edits.push(pending); pending = null; } };
    while (i < a.length || j < b.length) {
      if (i < a.length && j < b.length && a[i] === b[j]) { commit(); position += a[i++].length; j++; continue; }
      if (!pending) pending = { from: position, to: position, insert: '' };
      if (j < b.length && (i === a.length || scores[i * width + j + 1] >= scores[(i + 1) * width + j])) pending.insert += b[j++];
      else { position += a[i++].length; pending.to = position; }
    }
    commit();
    if (edits.length > 4096 || apply(lf(before), edits) !== lf(candidate)) throw failure('review_conflict');
    return edits;
  }
  function mapPosition(position, edits, assoc = 1) {
    let delta = 0;
    for (const edit of edits) {
      if (position < edit.from) break;
      if (edit.from === edit.to && position === edit.from) return position + delta + (assoc > 0 ? edit.insert.length : 0);
      if (position >= edit.to) { delta += edit.insert.length - (edit.to - edit.from); continue; }
      return edit.from + delta + (position === edit.from ? 0 : assoc > 0 ? edit.insert.length : 0);
    }
    return position + delta;
  }
  function documentTransaction(rec, before, after, edits, origin) {
    if (before === after || typeof window.dispatchEvent !== 'function') return;
    // Consumers receive only immutable source coordinates. They cannot change
    // the FIFO queue or bind a selection to an equal string elsewhere.
    const changes = Object.freeze(edits.map(edit => Object.freeze({ from: edit.from, to: edit.to, insert: edit.insert })));
    const detail = Object.freeze({ tabId: rec.tab.id, before, after, edits: changes, origin,
      mapPosition: (position, assoc = 1) => mapPosition(position, changes, assoc) });
    window.dispatchEvent(new CustomEvent('readmd:document-transaction', { detail }));
  }
  function overlaps(a, b) {
    if (a.from === a.to) return b.from < a.from && a.from < b.to;
    if (b.from === b.to) return a.from < b.from && b.from < a.to;
    return Math.max(a.from, b.from) < Math.min(a.to, b.to);
  }
  function mappedEdits(edits, through, remote = false) {
    return edits.map(edit => {
      if (edit.from === edit.to) {
        const sameInsert = through.some(other => other.from === other.to && other.from === edit.from);
        const at = mapPosition(edit.from, through, remote && sameInsert ? -1 : 1);
        return { from: at, to: at, insert: edit.insert };
      }
      return { from: mapPosition(edit.from, through, 1), to: mapPosition(edit.to, through, -1), insert: edit.insert };
    });
  }
  function remoteEdits(before, snapshot, op) {
    if (before === snapshot.content) return [];
    if (op?.edits && apply(before, op.edits) === snapshot.content) return op.edits;
    if (op?.kind === 'resolve' && op.previous) {
      const edits = op.previous.changes.filter(change => op.ids.includes(change.id)).flatMap(change => {
        if (op.decision === 'withdraw' && change.application === 'applied' && change.decision !== 'withdrawn') return [{ from: change.from, to: change.to, insert: change.original }];
        if (op.decision === 'accept' && change.application === 'proposed') return [{ from: change.from, to: change.to, insert: change.replacement }];
        return [];
      }).sort((a, b) => a.from - b.from);
      if (apply(before, edits) === snapshot.content) return edits;
    }
    // New acknowledged proposals carry both original and resulting ranges.
    const oldIds = new Set(op?.previous?.changes.map(change => change.id) || []);
    const additions = snapshot.changes.filter(change => !oldIds.has(change.id) && change.application === 'applied' && change.decision !== 'withdrawn').sort((a, b) => a.from - b.from);
    let delta = 0;
    const edits = additions.map(change => {
      const from = change.from - delta, to = from + change.original.length;
      delta += change.replacement.length - change.original.length;
      return { from, to, insert: change.replacement };
    });
    try { if (edits.length && apply(before, edits) === snapshot.content) return edits; } catch (_) {}
    return diff(before, snapshot.content);
  }
  function refresh(rec) {
    if (!rec) return;
    if (activeTab() === rec.tab) statusContext(rec);
    const view = rec.view, CM = window.ReadMDCodeMirror;
    if (view && view === activeView() && activeTab() === rec.tab && effects.has(CM)) view.dispatch({ effects: effects.get(CM).of(null) });
    if (activeTab() === rec.tab && typeof state !== 'undefined' && state.editing) {
      let status = document.getElementById('review-persistence-status');
      if (rec.error && !status) {
        status = document.createElement('button'); status.id = 'review-persistence-status'; status.type = 'button';
        status.className = 'review-persistence-status'; status.setAttribute('role', 'status');
        status.addEventListener('click', () => { void retry(activeTab()); });
        document.getElementById('edit-wrap')?.appendChild(status);
      }
      if (status) {
        status.hidden = !rec.error;
        status.textContent = rec.error ? message(rec.error) : '';
        status.title = t('reader.refresh'); status.setAttribute('aria-label', status.textContent + ' · ' + t('reader.refresh'));
      }
    }
    if (typeof window.dispatchEvent === 'function') window.dispatchEvent(new CustomEvent('readmd:review-state', { detail: { tabId: rec.tab.id, snapshot: rec.snapshot, busy: Boolean(rec.processing || rec.saving), error: rec.error?.code || null } }));
  }
  function writeVisible(rec, before, edits, after) {
    if (!edits.length) return;
    const view = rec.view;
    if (view && view === activeView() && activeTab() === rec.tab && view.state.doc.toString() === before) {
      const CM = window.ReadMDCodeMirror;
      view.dispatch({ changes: edits, annotations: [CM.Transaction.userEvent.of('review.remote'), CM.Transaction.addToHistory.of(false)] });
    } else if (rec.tab === activeTab() && typeof state !== 'undefined' && state.editing && !activeView()) {
      const area = document.getElementById('edit-area');
      if (area && lf(area.value) === before) area.value = after;
    }
    rec.tab.content = persistedText(rec.tab, after); rec.tab.fixed = rec.tab.content;
    rec.tab.isDirty = rec.tab.content !== (rec.tab.original || '') || Boolean(rec.tab.unsavedCreation);
    if (rec.tab === activeTab() && typeof state !== 'undefined') {
      state.fixed = rec.tab.content;
      if (typeof syncActiveTabDirty === 'function' && state.editing) syncActiveTabDirty();
      if (typeof schedulePreview === 'function' && state.editing) schedulePreview();
    }
    if (typeof renderTabsBar === 'function') renderTabsBar();
    if (typeof updateUnloadGuard === 'function') updateUnloadGuard();
    documentTransaction(rec, before, after, edits, 'review');
  }
  function acknowledge(rec, snapshot) {
    rec.snapshot = snapshot; rec.path = snapshot.path || rec.path;
    rec.tab.reviewSession = { document_id: snapshot.document_id, generation: snapshot.generation, review_revision: snapshot.review_revision };
  }
  function receive(rec, snapshot, before, op = {}) {
    const next = remoteEdits(before, snapshot, op), oldLocal = rec.local;
    let projected = next;
    const rebased = [];
    let conflict = false;
    for (const pending of rec.queue) {
      // History commands carry no source coordinates. Keep their FIFO place
      // while an earlier edit is acknowledged; pump binds CAS when sent.
      if (pending.kind === 'undo' || pending.kind === 'redo') continue;
      if (pending.kind !== 'edit' || projected.some(a => pending.edits.some(b => overlaps(a, b)))) { conflict = true; break; }
      const previousEdits = pending.edits;
      const pendingBefore = apply(pending.before, projected);
      const pendingEdits = mappedEdits(previousEdits, projected);
      const pendingAfter = apply(pendingBefore, pendingEdits);
      projected = mappedEdits(projected, previousEdits, true);
      rebased.push({ pending, before: pendingBefore, edits: pendingEdits, after: pendingAfter });
    }
    acknowledge(rec, snapshot);
    if (conflict) {
      // Keep every keystroke. Sending this exact compensation as a manual edit
      // lets Rust mark touched proposals as conflict/taken_over and retain the
      // candidate and original for comparison; it never relocates by text.
      rec.queue = [{ kind: 'edit', before: snapshot.content, after: oldLocal,
        edits: diff(snapshot.content, oldLocal), origin: 'editor', call_id: callId() }];
      notifyError(failure('review_conflict'));
    } else {
      for (const item of rebased) Object.assign(item.pending, { before: item.before, edits: item.edits, after: item.after });
      const after = apply(oldLocal, projected);
      rec.local = after;
      writeVisible(rec, oldLocal, projected, after);
    }
    if (snapshot.recovery || snapshot.status === 'needs_recovery') {
      rec.error = failure('review_recovery_required'); notifyError(rec.error);
    }
    refresh(rec);
  }
  function sameWatermark(mark, snapshot) {
    return Boolean(mark && typeof mark.revision === 'string' && mark.revision &&
      mark.document_id === snapshot.document_id && mark.generation === snapshot.generation &&
      mark.review_revision === snapshot.review_revision && mark.revision === snapshot.revision);
  }
  async function hydrate(rec, snapshot, initial) {
    const tab = rec.tab, stored = rec.recoveryDraft?.content, mark = rec.recoveryDraft?.review;
    if (!rec.recoveryResolved && typeof stored === 'string' && lf(stored) !== snapshot.content) {
      const draft = lf(stored);
      tab._reviewRestorePending = true;
      // A clean session descriptor can lag a durable generic draft. Keep that
      // body visible too while deciding which acknowledged view it descends from.
      if (!rec.queue.length && rec.local === initial && rec.local !== draft) {
        const before = rec.local; rec.local = rec.base = draft;
        writeVisible(rec, before, diff(before, draft), draft);
      }
      tab.isDirty = true;
      if (sameWatermark(mark, snapshot) && rec.base === draft) {
        const edits = documentEdits(snapshot.content, draft);
        acknowledge(rec, snapshot);
        rec.queue.unshift({ kind: 'edit', before: snapshot.content, after: draft, edits, origin: 'editor', call_id: callId() });
        rec.recoveryResolved = true; tab._reviewRestorePending = false;
        refresh(rec); return snapshot;
      }
      // No clock establishes whether an old generic draft or SQLite is newer.
      // Preserve the generic body through the existing durable history API
      // before choosing SQLite, including typing made while a prior copy failed.
      const local = rec.local, preserve = window.ReadMDRecovery?.preserveReviewDraft;
      if (!preserve || !await preserve(tab, stored, mark)) throw failure('review_persistence_failed');
      if (local !== draft && !await preserve(tab, persistedText(tab, local), mark)) throw failure('review_persistence_failed');
      if (rec.local !== local) throw failure('review_conflict');
      rec.queue = []; rec.recoveryResolved = true; tab._reviewRestorePending = false;
      receive(rec, snapshot, local); notifyError(failure('review_conflict'));
      return snapshot;
    }
    rec.recoveryResolved = true; tab._reviewRestorePending = false;
    receive(rec, snapshot, initial); return snapshot;
  }
  async function open(rec) {
    if (rec.unsupported || rec.snapshot) return rec.snapshot;
    if (!rec.opening) {
      const initial = rec.base;
      rec.opening = request('/api/review', { action: 'open', path: rec.path, content: initial,
        encoding: rec.tab.encoding || 'utf-8' }).then(snapshot => hydrate(rec, snapshot, initial))
        .catch(error => { rec.error = error; refresh(rec); notifyError(error); return null; });
    }
    return rec.opening;
  }
  function cas(rec, id) { return { document_id: rec.snapshot.document_id, generation: rec.snapshot.generation, review_revision: rec.snapshot.review_revision, call_id: id || callId() }; }
  // _renameInFlight also spans preflight and outlives endRename. The review
  // reservation starts after flush settles; that reservation and durable
  // pending metadata are the fences for reads and queued writes here.
  const renameReserved = rec => Boolean(rec.renaming || rec.tab._renamePending);
  function pump(rec) {
    if (rec.processing || rec.saving || rec.adopting || renameReserved(rec) || rec.error || rec.unsupported) return rec.processing;
    rec.processing = (async () => {
      if (!await open(rec)) return;
      while (rec.queue.length && !rec.saving && !rec.adopting && !renameReserved(rec) && !rec.error) {
        const op = rec.queue.shift();
        const before = rec.snapshot.content;
        let cancelListener;
        try {
          if (op.signal?.aborted) throw failure('cancelled');
          if (op.kind === 'edit' && op.before !== before) throw failure('review_conflict');
          op.previous = rec.snapshot;
          if (!op.payload) {
            op.payload = { ...cas(rec, op.call_id), ...(op.body || {}), ...(op.kind === 'ai' ? {} : { action: op.kind }) };
            if (op.edits) Object.assign(op.payload, { edits: op.edits, origin: op.origin || 'editor' });
            if (op.kind === 'resolve') {
              Object.assign(op.payload, { ids: op.ids, decision: op.decision });
              if (op.resolution) op.payload.resolution = op.resolution;
            }
          }
          if (op.kind === 'ai' && op.signal) {
            cancelListener = () => { void cancelAi(op.payload.task_id, () => !op.finished); };
            op.signal.addEventListener('abort', cancelListener, { once: true });
          }
          const snapshot = await request(op.kind === 'ai' ? '/api/review/ai' : '/api/review', op.payload);
          if (op.kind === 'edit' && snapshot.content !== op.after) throw failure('review_conflict');
          receive(rec, snapshot, op.kind === 'edit' ? op.after : before, op);
          op.resolve?.(snapshot);
        } catch (error) {
          // Retain the same id and exact payload: an explicit retry can recover
          // a committed operation whose network acknowledgement was lost.
          const rejected = op.kind !== 'edit' && /conflict|stale|budget|invalid|no_history|nothing_to_|undo_unavailable|redo_unavailable|cancelled|ai_not_configured|ai_http_error|ai_network_error|review_ai_/.test(error.code || '');
          if (!rejected) { rec.queue.unshift(op); rec.error = error; }
          op.reject?.(error); delete op.resolve; delete op.reject;
          notifyError(error); refresh(rec);
        } finally {
          op.finished = true;
          if (cancelListener) op.signal.removeEventListener('abort', cancelListener);
        }
      }
    })().finally(() => { rec.processing = null; refresh(rec); });
    return rec.processing;
  }
  function enqueueEdit(rec, before, after, edits, origin) {
    if (before === after) return;
    if (rec.local !== before) { rec.error = failure('review_conflict'); notifyError(rec.error); return; }
    rec.local = after;
    documentTransaction(rec, before, after, edits, origin);
    if (!rec.unsupported) {
      const tail = rec.queue[rec.queue.length - 1]; let combined = false;
      if (rec.processing && origin === 'editor' && tail?.kind === 'edit' && tail.origin === 'editor' && !tail.payload && !tail.resolve && tail.after === before) {
        try {
          const composed = saveStageEdits(tail.before, [{ edits: tail.edits, content: tail.after }, { edits, content: after }], after);
          if (composed.length) { tail.edits = composed; tail.after = after; combined = true; }
        } catch (_) { /* A composition limit keeps these as ordered edits. */ }
      }
      if (!combined) rec.queue.push({ kind: 'edit', before, after, edits, origin, call_id: callId() });
      pump(rec);
    }
  }
  async function flush(tab = activeTab()) {
    // Closing/switching an unseen reader is not an editing operation. Explicit
    // capture/mutation/save create their own record before flushing instead.
    const rec = tab && records.get(tab); if (!rec || rec.unsupported) return true;
    if (renameReserved(rec) && (rec.queue.length || rec.processing)) return false;
    if (rec.adoption && !await rec.adoption) return false;
    // The textarea fallback has no CodeMirror transaction log.
    const current = source(tab);
    if (current !== rec.local && !rec.view) {
      if (renameReserved(rec)) return false;
      enqueueEdit(rec, rec.local, current, diff(rec.local, current), 'editor');
    }
    if (!await open(rec) || rec.error || rec.saving) return false;
    while (rec.queue.length || rec.processing) {
      if (renameReserved(rec)) return false;
      if (rec.adoption) { if (!await rec.adoption) return false; continue; }
      await (pump(rec) || rec.processing);
      if (rec.error || rec.saving) return false;
    }
    return rec.snapshot.content === rec.local;
  }
  async function flushAll() {
    const tabs = typeof state !== 'undefined' && Array.isArray(state.tabs) ? [...state.tabs] : [activeTab()].filter(Boolean);
    for (const tab of tabs) if (!await flush(tab)) return false;
    return true;
  }
  async function ensure(tab = activeTab(), initial) { const rec = record(tab, initial); return rec ? open(rec) : null; }
  async function restore(tab = activeTab()) {
    if (!tab) return null;
    const existing = records.get(tab);
    if (existing) { refresh(existing); return existing.snapshot || existing.opening; }
    if (!restores.has(tab)) {
      const path = tab.path || tab.reviewPath || ('draft:' + tab.id);
      const work = (async () => {
        const response = await apiFetch('/api/review', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ action: 'resume', path }) });
        const data = await response.json();
        if (!response.ok || data.ok === false || data.error) throw failure(data.error_code || data.code || 'review_persistence_failed');
        if (!data.found) { tab._reviewRestorePending = false; if (tab === activeTab()) statusContext(null); return null; }
        // Entering Edit can race this read. Its initial text and queued input
        // then belong to that record and must win over this hydration callback.
        const live = records.get(tab);
        if (live) return live.snapshot || live.opening || open(live);
        const snapshot = data.snapshot;
        if (!snapshot?.document_id || typeof snapshot.content !== 'string' || lf(snapshot.content) !== snapshot.content || !Number.isInteger(snapshot.generation) || !Number.isInteger(snapshot.review_revision) || !Array.isArray(snapshot.changes)) throw failure('review_persistence_failed');
        const rec = record(tab, tab.isDirty ? tab.content : (tab.original ?? tab.content));
        rec.opening = hydrate(rec, snapshot, rec.base).catch(error => { rec.error = error; refresh(rec); notifyError(error); return null; });
        if (!await rec.opening) return null;
        if (rec.queue.length) await pump(rec);
        return rec.snapshot;
      })().catch(error => { notifyError(error); return null; }).finally(() => { restores.delete(tab); });
      restores.set(tab, work);
    }
    return restores.get(tab);
  }
  function mount(view, tab, initial) {
    const rec = record(tab, initial); if (!rec) return;
    rec.view = view; if (view) views.set(view, rec);
    if (rec.snapshot && rec.local === lf(initial) && rec.snapshot.content === rec.local) refresh(rec);
    void open(rec).then(() => pump(rec));
  }
  function detach(view) { const rec = views.get(view); if (rec?.view === view) rec.view = null; }
  function onEditorUpdate(update) {
    const rec = views.get(update.view); if (!rec) return;
    for (const transaction of update.transactions) {
      if (!transaction.docChanged || transaction.isUserEvent('review.remote')) continue;
      const edits = [];
      transaction.changes.iterChanges((from, to, _fromB, _toB, inserted) => edits.push({ from, to, insert: inserted.toString() }));
      enqueueEdit(rec, transaction.startState.doc.toString(), transaction.newDoc.toString(), edits,
        transaction.isUserEvent('undo') ? 'undo' : transaction.isUserEvent('redo') ? 'redo' : 'editor');
    }
  }
  async function capture(tab = activeTab()) {
    if (tab?._renamePending || tab?._renameInFlight) return null;
    const rec = record(tab);
    if (!await flush(tab)) return null;
    if (!rec?.snapshot || rec.unsupported) return null;
    return { tabId: tab.id, document_id: rec.snapshot.document_id, generation: rec.snapshot.generation,
      review_revision: rec.snapshot.review_revision, content: rec.local, editGeneration: tab.editGeneration || 0 };
  }
  function isCurrent(rec, stamp) {
    return Boolean(stamp && stamp.tabId === rec.tab.id && stamp.document_id === rec.snapshot?.document_id &&
      stamp.generation === rec.snapshot.generation && stamp.review_revision === rec.snapshot.review_revision && stamp.content === rec.local && (rec.tab.editGeneration || 0) === stamp.editGeneration && !rec.queue.length);
  }
  async function mutate(kind, options = {}, tab = activeTab(), stamp) {
    if (tab?._renamePending || tab?._renameInFlight) throw failure('review_busy');
    const rec = record(tab);
    if (!await flush(tab)) throw rec?.error || failure('review_persistence_failed');
    if (!rec?.snapshot || rec.unsupported) throw failure('review_budget');
    if (stamp && !isCurrent(rec, stamp)) throw failure('review_stale_generation');
    if (options.signal?.aborted) throw failure('cancelled');
    return new Promise((resolve, reject) => { rec.queue.push({ kind, ...options, call_id: callId(), resolve, reject }); pump(rec); });
  }
  async function propose(edits, stamp, tab = activeTab()) {
    const normalized = edits.map(edit => ({ from: edit.from, to: edit.to, insert: lf(edit.insert) }));
    return mutate('propose', { edits: normalized, origin: 'ai' }, tab, stamp);
  }
  function resolve(ids, decision, tab = activeTab(), resolution) { return mutate('resolve', { ids: [...ids], decision, resolution }, tab); }
  async function discard(tab = activeTab()) {
    const rec = record(tab); if (!rec || rec.unsupported) return true;
    try {
      const snapshot = await mutate('discard', {}, tab);
      // New typing after the explicit discard request remains a live draft.
      // Callers must leave that editor open instead of clearing it on exit.
      return rec.local === snapshot.content;
    } catch (_) { return false; }
  }
  function history(redo = false) {
    const tab = activeTab(), rec = tab && records.get(tab);
    if (tab?._renamePending || tab?._renameInFlight) { notifyError(failure('review_busy')); return true; }
    if (!rec || rec.unsupported) return false;
    if (!rec.queue.length && !rec.processing && rec.snapshot && rec.snapshot[redo ? 'can_redo' : 'can_undo'] === false) return true;
    rec.queue.push({ kind: redo ? 'redo' : 'undo', call_id: callId(), reject: notifyError });
    pump(rec); return true;
  }
  // Export receipts and durable AI jobs arrive outside the editor FIFO. Read
  // the current authoritative head after any in-flight editor write, then
  // project it through unsent edits using the same scoped rebase as mutations.
  // A delayed receipt can never replace a newer server head or another tab.
  function deferAdoption(rec, documentId) {
    // Receipts only signal that the durable head should be read. Coalesce
    // arrivals during rename instead of losing a completed job's result or
    // putting a promise that awaits rename into the flush/rename barrier.
    rec.deferredAdoption = documentId;
    return false;
  }
  function resumeAdoption(rec) {
    if (renameReserved(rec) || rec.adoption || rec.error || !rec.deferredAdoption) return;
    const documentId = rec.deferredAdoption; rec.deferredAdoption = null;
    void adopt(null, rec.tab, documentId);
  }
  function adopt(snapshot, tab = activeTab(), documentId = snapshot?.document_id) {
    const rec = tab && records.get(tab);
    if (!rec?.snapshot || !documentId || documentId !== rec.snapshot.document_id) return Promise.resolve(false);
    if (renameReserved(rec)) return Promise.resolve(deferAdoption(rec, documentId));
    const renameEpoch = rec.renameEpoch;
    const previous = rec.adoption || Promise.resolve(true);
    const work = previous.catch(() => false).then(async () => {
      if (typeof state !== 'undefined' && Array.isArray(state.tabs) && !state.tabs.includes(tab)) return false;
      if (renameReserved(rec) || rec.renameEpoch !== renameEpoch) return deferAdoption(rec, documentId);
      rec.adopting = true;
      const current = source(tab);
      if (current !== rec.local && !rec.view) enqueueEdit(rec, rec.local, current, diff(rec.local, current), 'editor');
      await rec.processing;
      if (renameReserved(rec) || rec.renameEpoch !== renameEpoch) return deferAdoption(rec, documentId);
      // An ambiguous transport failure keeps its original idempotent payload
      // for Retry. Only a definitively rejected CAS is safe to rebase/reissue.
      if (rec.error && !/^review_(?:stale_generation|stale_revision|conflict)$/.test(rec.error.code || '')) return deferAdoption(rec, documentId);
      const latest = await request('/api/review', { action: 'read', document_id: documentId });
      if (renameReserved(rec) || rec.renameEpoch !== renameEpoch) return deferAdoption(rec, documentId);
      if (latest.document_id !== documentId || latest.generation < rec.snapshot.generation || latest.review_revision < rec.snapshot.review_revision) throw failure('review_stale_generation');
      for (const pending of rec.queue) if (pending.kind === 'edit' && pending.payload) {
        pending.call_id = callId(); delete pending.payload;
      }
      rec.error = null;
      receive(rec, latest, rec.snapshot.content, { previous: rec.snapshot });
      return !rec.error;
    }).catch(error => {
      deferAdoption(rec, documentId);
      if (renameReserved(rec) || rec.renameEpoch !== renameEpoch) return false;
      rec.error = error; notifyError(error); refresh(rec); return false;
    }).finally(() => {
      rec.adopting = false;
      if (rec.adoption === work) rec.adoption = null;
      resumeAdoption(rec);
      pump(rec);
    });
    rec.adoption = work;
    return work;
  }
  async function runAi(options, stamp, tab = activeTab()) {
    if (window.ReadMDAiHarness && window.READMD_ENGINE === 'rust') {
      const task = await window.ReadMDAiHarness.start(options, stamp, tab);
      return window.ReadMDAiHarness.wait(task.id);
    }
    // Provider configuration is resolved privately by Rust. The browser sends
    // opaque connection identities, never connection headers or credentials.
    const body = { request: options.request || '', provider: options.provider, credential_id: options.credential_id,
      model: options.model, stream: options.stream, skill_id: options.skill_id, ranges: options.ranges, task_id: options.task_id || callId() };
    return mutate('ai', { body, signal: options.signal }, tab, stamp);
  }
  async function cancelAi(taskId, stillRunning = () => true) {
    if (!taskId) return 'unknown';
    for (let attempt = 0; attempt < 5 && stillRunning(); attempt++) {
      let status = 'unknown';
      try {
        if (window.ReadMDTask?.requestCancel) status = await window.ReadMDTask.requestCancel(taskId);
        else {
          const response = await apiFetch('/api/task/cancel', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ id: taskId }) });
          const data = await response.json(); if (response.ok && data.ok !== false) status = data.state || 'unknown';
        }
      } catch (_) {}
      if (status !== 'unknown') return status;
      // A close can race registration. Retry only the cancellation request;
      // the original mutation keeps its id and remains awaiting its receipt.
      if (attempt < 4 && stillRunning()) await new Promise(done => setTimeout(done, 100 * (attempt + 1)));
    }
    return 'unknown';
  }
  async function prepareSave(tab = activeTab(), expected) {
    if (tab?._renamePending || tab?._renameInFlight) return false;
    const rec = record(tab);
    if (!await flush(tab)) return false;
    if (expected != null && lf(expected) !== rec?.local) return false;
    if (!rec?.snapshot || rec.unsupported) return null;
    rec.saving = true; refresh(rec);
    return cas(rec);
  }
  function saveStageEdits(before, stages, finalContent) {
    // Compose a piece table, retaining original source spans. Inserted text is
    // never rebound to an equal substring elsewhere in the original document.
    let pieces = [{ from: 0, to: before.length }], content = before;
    const length = piece => piece.insert === undefined ? piece.to - piece.from : piece.insert.length;
    const slice = (input, from, to) => {
      let offset = 0; const selected = [];
      for (const piece of input) {
        const size = length(piece), start = Math.max(from, offset), end = Math.min(to, offset + size);
        if (end > start) selected.push(piece.insert === undefined
          ? { from: piece.from + start - offset, to: piece.from + end - offset }
          : { insert: piece.insert.slice(start - offset, end - offset) });
        offset += size; if (offset >= to) break;
      }
      return selected;
    };
    for (const stage of stages) {
      if (!Array.isArray(stage.edits) || typeof stage.content !== 'string' || lf(stage.content) !== stage.content) throw failure('review_conflict');
      const expected = apply(content, stage.edits);
      if (expected !== stage.content) throw failure('review_conflict');
      let cursor = 0; const next = [];
      for (const edit of stage.edits) {
        next.push(...slice(pieces, cursor, edit.from));
        if (edit.insert) next.push({ insert: edit.insert });
        cursor = edit.to;
      }
      next.push(...slice(pieces, cursor, content.length)); pieces = next; content = expected;
    }
    if (content !== finalContent) throw failure('review_conflict');
    const composed = []; let cursor = 0, insert = '';
    for (const piece of pieces) {
      if (piece.insert !== undefined) { insert += piece.insert; continue; }
      if (piece.from !== cursor || insert) composed.push({ from: cursor, to: piece.from, insert });
      cursor = piece.to; insert = '';
    }
    if (cursor !== before.length || insert) composed.push({ from: cursor, to: before.length, insert });
    if (composed.length > 4096 || apply(before, composed) !== finalContent) throw failure('review_conflict');
    return composed;
  }
  function saved(tab, snapshot, before, edits, stages) {
    const rec = records.get(tab); if (!rec || !snapshot?.document_id) return;
    let normalized;
    if (Array.isArray(stages)) normalized = saveStageEdits(lf(before), stages, snapshot.content);
    else if (Array.isArray(edits) && edits.length) {
      let cursor = 0, removed = 0;
      const position = at => {
        while (cursor < at) { if (before[cursor] === '\r' && before[cursor + 1] === '\n') { removed++; cursor++; } cursor++; }
        return at - removed;
      };
      normalized = edits.map(edit => ({ from: position(edit.from), to: position(edit.to), insert: lf(edit.insert) }));
    }
    receive(rec, snapshot, lf(before), { previous: rec.snapshot, edits: normalized });
    rec.path = snapshot.path || tab.path || rec.path;
  }
  function finishSave(tab) { const rec = records.get(tab); if (rec) { rec.saving = false; pump(rec); refresh(rec); } }
  // A rename moves the durable identity, not the text. Lock the local queue
  // while its CAS is in flight and only adopt a matching server receipt.
  async function beginRename(tab) {
    if (!tab) return null;
    const restoring = restores.get(tab);
    if (restoring) await restoring;
    if (!await flush(tab)) return null;
    const rec = records.get(tab);
    if (rec?.unsupported || rec?.renaming || rec?.error || rec?.saving || rec?.adopting || rec?.adoption || rec?.processing || rec?.queue.length) return null;
    if (rec) { rec.renaming = true; rec.renameEpoch++; }
    return { review_session: rec?.snapshot ? cas(rec) : undefined };
  }
  function renamed(tab, snapshot) {
    if (!tab || !snapshot?.document_id || typeof snapshot.content !== 'string' ||
        !Number.isInteger(snapshot.generation) || !Number.isInteger(snapshot.review_revision) || !Array.isArray(snapshot.changes)) return false;
    const rec = record(tab, tab.original ?? tab.content);
    if (!rec || rec.unsupported || rec.error || rec.processing || rec.saving || rec.adopting || rec.adoption || rec.queue.length ||
        rec.local !== snapshot.content || (rec.snapshot && (rec.snapshot.document_id !== snapshot.document_id ||
        rec.snapshot.generation !== snapshot.generation || rec.snapshot.review_revision !== snapshot.review_revision))) return false;
    rec.renameEpoch++;
    acknowledge(rec, snapshot); rec.opening = Promise.resolve(snapshot); refresh(rec); return true;
  }
  // Refactors can change several saved bodies at once. Capture each local
  // head while reserved, then validate the entire group before publishing any
  // snapshot or editor transaction. Generic adopt() deliberately defers here.
  function refactorState(tab) {
    if (!tab) return null;
    const rec = records.get(tab);
    if (rec && (rec.unsupported || rec.error || rec.processing || rec.saving || rec.adopting || rec.adoption || rec.queue.length)) return null;
    return Object.freeze({ path: tab.path || '', revision: tab.revision || '', content: source(tab),
      editGeneration: tab.editGeneration || 0, recorded: Boolean(rec),
      document_id: rec?.snapshot?.document_id || null, generation: rec?.snapshot?.generation ?? null,
      review_revision: rec?.snapshot?.review_revision ?? null });
  }
  function refactorPath(value) {
    if (typeof normalizePath === 'function') return normalizePath(value);
    const path = String(value || '').replace(/\\/g, '/');
    return /^(?:[a-z]:\/|\/\/)/i.test(path) ? path.toLowerCase() : path;
  }
  function prepareRefactorGroup(entries) {
    if (!Array.isArray(entries) || !entries.length || entries.length > 100) return null;
    const seen = new Set(), prepared = [];
    try {
      for (const item of entries) {
        const tab = item?.tab, before = item?.before, snapshot = item?.snapshot;
        if (!tab || seen.has(tab) || !before || typeof before.content !== 'string' ||
            !snapshot?.document_id || typeof snapshot.path !== 'string' || !snapshot.path ||
            typeof snapshot.content !== 'string' || lf(snapshot.content) !== snapshot.content ||
            !Number.isSafeInteger(snapshot.generation) || snapshot.generation < 0 ||
            !Number.isSafeInteger(snapshot.review_revision) || snapshot.review_revision < 0 || !Array.isArray(snapshot.changes) ||
            snapshot.recovery || snapshot.status === 'needs_recovery' || tab.isDirty || tab.unsavedCreation || tab._reviewRestorePending ||
            (typeof state !== 'undefined' && Array.isArray(state.tabs) && !state.tabs.includes(tab))) return null;
        seen.add(tab);
        const rec = records.get(tab), current = source(tab), head = rec?.snapshot;
        if (rec && (rec.unsupported || rec.error || rec.processing || rec.saving || rec.adopting || rec.adoption || rec.queue.length)) return null;
        if (new TextEncoder().encode(snapshot.content).byteLength > MAX_BYTES) return null;
        const already = Boolean(head && head.document_id === snapshot.document_id && head.generation === snapshot.generation &&
          head.review_revision === snapshot.review_revision && head.content === snapshot.content && current === snapshot.content &&
          refactorPath(tab.path) === refactorPath(snapshot.path) && tab.revision === (item.file_revision ?? snapshot.revision));
        if (!already && (current !== before.content || (tab.editGeneration || 0) !== before.editGeneration ||
            refactorPath(tab.path) !== refactorPath(before.path) || (tab.revision || '') !== before.revision ||
            Boolean(rec) !== before.recorded || (head?.document_id || null) !== before.document_id ||
            (head?.generation ?? null) !== before.generation || (head?.review_revision ?? null) !== before.review_revision ||
            (before.document_id && snapshot.document_id !== before.document_id))) return null;
        if (rec?.view && rec.view === activeView() && activeTab() === tab && rec.view.state.doc.toString() !== current) return null;
        const edits = already ? [] : Array.isArray(item.edits) ? item.edits.map(edit => ({ from: edit.from, to: edit.to, insert: edit.insert })) : documentEdits(current, snapshot.content);
        if (apply(current, edits) !== snapshot.content) return null;
        const cached = !already && tab.editorState?.doc?.toString() === current && typeof tab.editorState.update === 'function'
          ? tab.editorState.update({ changes: edits, annotations: [window.ReadMDCodeMirror.Transaction.userEvent.of('review.remote'), window.ReadMDCodeMirror.Transaction.addToHistory.of(false)] }).state : null;
        prepared.push({ tab, rec, snapshot, before: current, edits, already, cached });
      }
    } catch (_) { return null; }
    return prepared;
  }
  function validateRefactorGroup(entries) { return Boolean(prepareRefactorGroup(entries)); }
  function applyRefactorGroup(entries, applyTabs) {
    if (applyTabs && (typeof applyTabs !== 'function' || applyTabs.constructor?.name === 'AsyncFunction')) return false;
    const prepared = prepareRefactorGroup(entries); if (!prepared) return false;
    // Publish every durable head before callbacks/events can observe the group.
    for (const item of prepared) {
      const rec = item.rec || record(item.tab, item.before); item.rec = rec;
      rec.renameEpoch++; rec.renaming = true; rec.local = rec.base = item.snapshot.content;
      acknowledge(rec, item.snapshot); rec.opening = Promise.resolve(item.snapshot);
      if (item.cached) item.tab.editorState = item.cached;
    }
    if (applyTabs) applyTabs();
    for (const item of prepared) {
      const { rec, tab, before, edits, snapshot } = item, view = rec.view;
      if (edits.length && view && view === activeView() && activeTab() === tab && view.state.doc.toString() === before) {
        const CM = window.ReadMDCodeMirror;
        view.dispatch({ changes: edits, annotations: [CM.Transaction.userEvent.of('review.remote'), CM.Transaction.addToHistory.of(false)] });
      } else if (edits.length && tab === activeTab() && typeof state !== 'undefined' && state.editing && !activeView()) {
        const area = document.getElementById('edit-area');
        if (area && lf(area.value) === before) {
          const start = area.selectionStart, end = area.selectionEnd, direction = area.selectionDirection;
          area.value = snapshot.content;
          if (typeof area.setSelectionRange === 'function') area.setSelectionRange(mapPosition(start, edits, 1), mapPosition(end, edits, -1), direction);
        }
      }
    }
    for (const item of prepared) {
      documentTransaction(item.rec, item.before, item.snapshot.content, item.edits, 'refactor');
      refresh(item.rec);
    }
    return true;
  }
  function endRename(tab) { const rec = tab && records.get(tab); if (rec) { rec.renaming = false; resumeAdoption(rec); pump(rec); refresh(rec); } }
  function retry(tab = activeTab()) {
    const rec = tab && records.get(tab); if (!rec) return Promise.resolve(true);
    rec.error = null;
    if (!rec.snapshot) rec.opening = null;
    resumeAdoption(rec);
    return flush(tab);
  }
  function projectedChanges(rec) {
    if (!rec?.snapshot) return [];
    return rec.snapshot.changes.filter(change => change.decision !== 'withdrawn').map(change => {
      let from = change.from, to = change.to, canCompare = change.decision === 'conflict' || Boolean(change.taken_over);
      let conflict = canCompare && change.decision !== 'accepted';
      for (const pending of rec.queue) {
        if (pending.kind !== 'edit') continue;
        if (pending.edits.some(edit => overlaps({ from, to }, edit))) { conflict = true; canCompare = true; }
        from = mapPosition(from, pending.edits, 1); to = mapPosition(to, pending.edits, -1);
      }
      return { ...change, from, to: Math.max(from, to), conflict, canCompare };
    });
  }
  function button(label, action, id, icon) {
    const el = document.createElement('button'); el.type = 'button'; el.className = 'review-action';
    el.title = t(label); el.setAttribute('aria-label', t(label));
    el.dataset.reviewAction = action; if (id) el.dataset.reviewChangeId = id;
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg'); svg.setAttribute('viewBox', '0 0 24 24'); svg.setAttribute('aria-hidden', 'true');
    const path = document.createElementNS('http://www.w3.org/2000/svg', 'path'); path.setAttribute('d', icon); svg.appendChild(path); el.appendChild(svg);
    return el;
  }
  function controls(rec, change) {
    const panel = document.createElement('span'); panel.className = 'review-block-actions'; panel.contentEditable = 'false';
    panel.dataset.reviewChangeId = change.id; panel.dataset.rdChrome = 'review'; panel.setAttribute('role', 'group');
    const contextLabel = t(change.conflict ? 'review.conflict' : change.decision === 'pending' ? 'review.pending' : 'review.accept');
    panel.setAttribute('aria-label', contextLabel);
    if (change.decision === 'pending' && !change.conflict) panel.appendChild(button('review.accept', 'accept', change.id, 'M5 12l4 4L19 6'));
    panel.appendChild(button('review.withdraw', 'withdraw', change.id, 'M9 5l-6 6 6 6M3 11h11a6 6 0 0 1 6 6'));
    const more = document.createElement('details'); more.className = 'review-more';
    const summary = document.createElement('summary'); summary.title = contextLabel; summary.setAttribute('aria-label', contextLabel); summary.textContent = '⋯'; more.appendChild(summary);
    const menu = document.createElement('span'); menu.className = 'review-context-menu';
    for (const [key, action] of [['review.previous', 'previous'], ['review.next', 'next'], ['review.acceptAll', 'accept-all'], ['review.withdrawAll', 'withdraw-all']]) {
      const el = document.createElement('button'); el.type = 'button'; el.textContent = t(key); el.dataset.reviewAction = action; menu.appendChild(el);
    }
    let keepCurrent;
    if (change.conflict) {
      keepCurrent = document.createElement('button'); keepCurrent.type = 'button'; keepCurrent.textContent = t('review.keepCurrent');
      keepCurrent.dataset.reviewAction = 'keep-current'; keepCurrent.dataset.reviewChangeId = change.id; menu.appendChild(keepCurrent);
    }
    if (change.canCompare) {
      const compare = document.createElement('div'); compare.className = 'review-comparison';
      for (const [key, text] of [['review.withdraw', change.original], ['review.aiEdit', change.replacement], ['review.conflict', rec.local.slice(change.from, change.to)]]) {
        const label = document.createElement('strong'); label.textContent = t(key);
        const value = document.createElement('pre'); value.textContent = text; compare.append(label, value);
      }
      menu.appendChild(compare);
    }
    more.appendChild(menu); panel.appendChild(more);
    panel.addEventListener('mousedown', event => { if (event.target.closest('button')) event.preventDefault(); });
    panel.addEventListener('click', event => {
      const target = event.target.closest('[data-review-action]'); if (!target) return;
      event.preventDefault(); event.stopPropagation();
      const action = target.dataset.reviewAction;
      if (action === 'next' || action === 'previous') { navigate(action === 'next' ? 1 : -1, rec.tab, change.id); return; }
      if (action === 'withdraw' && change.canCompare) { more.open = true; keepCurrent?.focus(); return; }
      const ids = action.endsWith('-all') ? projectedChanges(rec).filter(item => item.decision === 'pending' && !item.conflict).map(item => item.id) : [target.dataset.reviewChangeId];
      if (!ids.length) return;
      panel.setAttribute('aria-busy', 'true'); panel.querySelectorAll('button').forEach(el => { el.disabled = true; });
      void resolve(ids, action === 'keep-current' || action.startsWith('accept') ? 'accept' : 'withdraw', rec.tab, action === 'keep-current' ? 'keep_current' : undefined).catch(notifyError).finally(() => {
        panel.removeAttribute('aria-busy'); panel.querySelectorAll('button').forEach(el => { el.disabled = false; });
      });
    });
    return panel;
  }
  function navigate(direction, tab = activeTab(), changeId) {
    const rec = tab && records.get(tab), view = rec?.view; if (!view || view !== activeView() || tab !== activeTab()) return;
    const changes = projectedChanges(rec).filter(change => change.decision === 'pending' || change.conflict).sort((a, b) => a.from - b.from);
    if (!changes.length) return;
    let index = changeId ? changes.findIndex(change => change.id === changeId) : changes.findIndex(change => change.from >= view.state.selection.main.head);
    if (changeId) index += direction;
    else if (index < 0) index = 0;
    else if (direction < 0) index--;
    const change = changes[(index + changes.length) % changes.length];
    view.dispatch({ selection: { anchor: change.from, head: change.to }, scrollIntoView: true }); view.focus();
  }
  function statusContext(rec) {
    const footer = document.getElementById('statusbar'); if (!footer) return;
    let marker = document.getElementById('status-review');
    const changes = rec?.snapshot?.changes || [], pending = changes.filter(change => change.decision === 'pending').length;
    const conflict = Boolean(rec?.snapshot?.recovery || changes.some(change => change.decision !== 'accepted' && (change.decision === 'conflict' || change.taken_over)));
    if (!marker && (pending || conflict)) {
      marker = document.createElement('button'); marker.id = 'status-review'; marker.type = 'button'; marker.className = 'status-review';
      marker.addEventListener('click', async () => {
        const tab = activeTab(); if (!tab) return;
        if (typeof state !== 'undefined' && !state.editing && typeof toggleEdit === 'function') await toggleEdit();
        if (activeTab() === tab) navigate(1, tab);
      });
      footer.insertBefore(marker, document.getElementById('status-mods'));
    }
    if (marker) {
      marker.hidden = !(pending || conflict);
      marker.textContent = conflict ? t('review.conflict') : t('review.pending') + ' · ' + pending;
      marker.title = marker.textContent; marker.setAttribute('aria-label', marker.textContent);
      marker.dataset.reviewDocumentId = rec?.snapshot?.document_id || '';
    }
  }
  function extension(CM, tab) {
    if (!CM.ViewPlugin || !CM.Decoration || !CM.WidgetType || !tab) return [];
    const rec = record(tab), effect = effects.get(CM) || CM.StateEffect.define(); effects.set(CM, effect);
    class Actions extends CM.WidgetType {
      constructor(change) { super(); this.change = change; }
      eq(other) { return this.change.id === other.change.id && this.change.version === other.change.version && this.change.decision === other.change.decision && this.change.conflict === other.change.conflict && this.change.canCompare === other.change.canCompare && this.change.from === other.change.from && this.change.to === other.change.to; }
      toDOM() { return controls(rec, this.change); }
      ignoreEvent() { return true; }
    }
    function decorations(view) {
      const ranges = [], length = view.state.doc.length;
      for (const change of projectedChanges(rec)) {
        if (change.from < 0 || change.to > length) continue;
        const attrs = { 'data-review-change-id': change.id, 'data-review-decision': change.decision, title: t(change.conflict ? 'review.conflict' : change.decision === 'pending' ? 'review.pending' : 'review.accept') };
        if (change.to > change.from) ranges.push(CM.Decoration.mark({ class: 'cm-review-block' + (change.conflict ? ' is-conflict' : change.decision === 'accepted' ? ' is-accepted' : ''), attributes: attrs }).range(change.from, change.to));
        ranges.push(CM.Decoration.widget({ widget: new Actions(change), side: 1 }).range(change.to));
      }
      return CM.Decoration.set(ranges, true);
    }
    return [CM.ViewPlugin.fromClass(class {
      constructor(view) { this.decorations = decorations(view); }
      update(update) { if (update.docChanged || update.selectionSet || update.transactions.some(transaction => transaction.effects.some(item => item.is(effect)))) this.decorations = decorations(update.view); }
    }, { decorations: plugin => plugin.decorations }), CM.Prec.highest(CM.keymap.of([
      { key: 'Mod-z', run: () => history(false), preventDefault: true },
      { key: 'Mod-Shift-z', run: () => history(true), preventDefault: true },
      { key: 'Mod-y', run: () => history(true), preventDefault: true },
      { key: 'Alt-ArrowDown', run: () => { navigate(1, tab); return true; } },
      { key: 'Alt-ArrowUp', run: () => { navigate(-1, tab); return true; } },
    ]))];
  }
  document.addEventListener?.('readmd:document-loaded', () => statusContext(records.get(activeTab())));
  window.addEventListener?.('readmd:language-changed', () => statusContext(records.get(activeTab())));
  return { lf, documentEdits, ensure, restore, mount, detach, extension, onEditorUpdate, flush, flushAll, capture, propose, resolve, discard, history, runAi, cancelAi,
    prepareSave, saved, finishSave, beginRename, renamed, refactorState, validateRefactorGroup, applyRefactorGroup, endRename, retry, navigate, adopt, snapshot: tab => records.get(tab || activeTab())?.snapshot || null,
    busy: tab => { const rec = records.get(tab || activeTab()); return Boolean(rec && (rec.processing || rec.saving || rec.queue.length)); } };
})();
