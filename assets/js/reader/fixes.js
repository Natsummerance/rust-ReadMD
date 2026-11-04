'use strict';
/* ============================================================
   ReadMD Reader - Document Fixes Modal
   ============================================================ */

/* ---------------- 修正详情 ---------------- */

let documentInspectionGeneration = 0;
let documentFixOwner = null;
function showFixModal() {
  documentFixOwner = captureDocumentFixOwner();
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  const list = $('fix-list');
  if (list) list.innerHTML = '';
  const fixes = state.fixes || [];
  const fixCount = $('fix-count');
  if (fixCount) {
    fixCount.textContent = fixes.length ? (_t('fixes.countTotal', { count: fixes.length }) || ('（共 ' + fixes.length + ' 处）')) : '';
  }
  if (!fixes.length) {
    const li = document.createElement('li');
    li.className = 'empty';
    li.textContent = _t('fixes.noFixes') || '本篇文档未发现需要修正的内容';
    if (list) list.appendChild(li);
  } else {
    fixes.forEach(f => {
      const li = document.createElement('li');
      li.textContent = f;
      if (list) list.appendChild(li);
    });
  }
  const modal = $('fix-modal');
  if (modal) modal.classList.remove('hidden');
  void inspectCurrentDocument(++documentInspectionGeneration);
}

async function inspectCurrentDocument(generation) {
  const text = state.editing ? getEditContent() : (state.original ?? state.fixed ?? '');
  const tabId = state.activeTabId;
  const file = state.file && !state.virtualSource ? state.file : '';
  const list = $('fix-list');
  const status = document.createElement('li');
  status.className = 'document-inspection-summary';
  status.textContent = i18n.t('inspection.loading');
  list?.appendChild(status);
  try {
    const response = await apiFetch('/api/document/analyze', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ content: text, ...(file && /^(?:[A-Za-z]:[\\/]|\/)/.test(file) ? { file_path: file } : {}) })
    });
    const result = await response.json();
    if (generation !== documentInspectionGeneration || tabId !== state.activeTabId || $('fix-modal').classList.contains('hidden')) return;
    if (text !== (state.editing ? getEditContent() : (state.original ?? state.fixed ?? ''))) {
      status.textContent = i18n.t('inspection.changed'); return;
    }
    if (!response.ok || !result.ok) throw new Error('inspection_failed');
    status.textContent = i18n.t('inspection.summary', { count: result.diagnostics.length });
    for (const issue of result.diagnostics) {
      const row = document.createElement('li');
      row.className = 'document-inspection-issue';
      const key = 'inspection.' + issue.code;
      row.textContent = i18n.t('inspection.atLine', { line: issue.position.line }) + ' · ' + i18n.t(key) + ' · ' + issue.target;
      list?.appendChild(row);
    }
    if (result.truncated) {
      const row = document.createElement('li'); row.textContent = i18n.t('inspection.limited'); list?.appendChild(row);
    }
  } catch (_) {
    if (generation === documentInspectionGeneration && tabId === state.activeTabId && status.isConnected) status.textContent = i18n.t('inspection.failed');
  }
}

function documentFixText(key, fallback = 'review.persistenceFailed') {
  const value = window.i18n?.t(key);
  const alternative = window.i18n?.t(fallback);
  return value && value !== key ? value : alternative && alternative !== fallback ? alternative : '';
}
function documentFixFailure(code) { const error = new Error(code); error.code = code; return error; }
function documentFixSource(tab) {
  const text = tab === getActiveTab() && state.editing ? getEditContent() : tab?.isDirty ? tab.content : (tab?.original ?? tab?.content ?? '');
  return window.ReadMDReview?.lf ? window.ReadMDReview.lf(text) : String(text ?? '').replace(/^\uFEFF/, '').replace(/\r\n?/g, '\n');
}
function captureDocumentFixOwner() {
  const tab = typeof getActiveTab === 'function' ? getActiveTab() : null;
  return tab ? { tab, path: tab.path, generation: tab.editGeneration || 0, source: documentFixSource(tab), view: state.editing ? window.cmView : null } : null;
}
function ownsDocumentFix(owner, active = true) {
  return Boolean(owner && documentFixOwner === owner && state.tabs.includes(owner.tab) && owner.tab.path === owner.path &&
    (owner.tab.editGeneration || 0) === owner.generation && documentFixSource(owner.tab) === owner.source &&
    (!active || getActiveTab() === owner.tab && (!owner.view || window.cmView === owner.view)));
}
function sameDocumentFixStamp(left, right) {
  return Boolean(left && right && left.document_id === right.document_id && left.generation === right.generation &&
    left.review_revision === right.review_revision && left.content === right.content && left.editGeneration === right.editGeneration);
}
function notifyDocumentFix(error) {
  const code = error?.code || '';
  showToast(documentFixText('error.' + code, code === 'cancelled' ? 'ai.stopped' : /stale|conflict/.test(code) ? 'review.conflict' : 'review.persistenceFailed'));
}
function showDocumentFixTask(owner, task) {
  // A fast legitimate proposal may already have advanced the draft. This is
  // status presentation only; keep its original tab identity, not old text.
  if (!owner || documentFixOwner !== owner || !state.tabs.includes(owner.tab) || owner.tab.path !== owner.path || getActiveTab() !== owner.tab) return;
  $('fix-modal')?.classList.add('hidden');
  if ($('ai-panel')?.classList.contains('hidden') && typeof toggleAiPanel === 'function') toggleAiPanel();
  window.ReadMDAiHarness.render();
  const key = ['queued', 'running'].includes(task.state) ? 'ai.taskRunning' : task.state === 'completed' ? 'ai.taskCompleted' : 'ai.taskPaused';
  showToast(documentFixText(key));
}
async function handleAiDocumentFix() {
  return window.ReadMDTask.run('document-ai-fix', runAiDocumentFix, { trigger: 'fix-ai-btn' });
}
async function runAiDocumentFix() {
  const owner = documentFixOwner || (documentFixOwner = captureDocumentFixOwner());
  try {
    if (!ownsDocumentFix(owner)) throw documentFixFailure('review_stale_generation');
    if (window.READMD_ENGINE !== 'rust' || !window.ReadMDReview || !window.ReadMDAiHarness) throw documentFixFailure('review_persistence_failed');
    if (!owner.source.trim()) { showToast(documentFixText('fixes.noDocContent')); return false; }
    const stamp = await window.ReadMDReview.capture(owner.tab);
    if (!stamp || !ownsDocumentFix(owner)) throw documentFixFailure('review_stale_generation');
    if (!await window.ReadMDAiHarness.init()) throw documentFixFailure('ai_task_failed');
    if (!ownsDocumentFix(owner)) throw documentFixFailure('review_stale_generation');
    const existing = window.ReadMDAiHarness.list().find(task => task.document_id === stamp.document_id && ['queued', 'running'].includes(task.state));
    if (existing) { showDocumentFixTask(owner, existing); return existing; }
    // Reading enters the existing editor on the same document. Its source is
    // the Review draft/original, not the reader's corrected display projection.
    if (!state.editing) await toggleEdit();
    if (!state.editing || !ownsDocumentFix(owner)) throw documentFixFailure('review_stale_generation');
    const view = window.cmView;
    const connection = typeof ensureAiConfigured === 'function' ? await ensureAiConfigured() : null;
    if (!connection) return false;
    if (!ownsDocumentFix(owner) || window.cmView !== view || !state.editing) throw documentFixFailure('review_stale_generation');
    if ($('fix-modal')?.classList.contains('hidden')) throw documentFixFailure('cancelled');
    // start performs a second capture, including editGeneration and both Rust
    // revisions. An edit followed by undo cannot validate an old equal string.
    const task = await window.ReadMDAiHarness.start({ request: documentFixText('fixes.aiFix'), skill_id: 'readmd-format-fix',
      provider: connection.provider, credential_id: connection.credential_id, model: connection.model,
      task_id: window.ReadMDTask.newTaskId('document-fix'), ranges: [{ from: 0, to: stamp.content.length }] }, stamp, owner.tab);
    showDocumentFixTask(owner, task);
    return task;
  } catch (error) { notifyDocumentFix(error); return false; }
}
async function handleSaveDocumentFix() {
  return window.ReadMDTask.run('document-fix-save', runSaveDocumentFix, { trigger: 'fix-save' });
}
async function runSaveDocumentFix() {
  const owner = documentFixOwner || (documentFixOwner = captureDocumentFixOwner());
  const editing = Boolean(state.editing), projection = editing ? null : state.fixed;
  let copy = null;
  try {
    if (!ownsDocumentFix(owner) || !window.ReadMDReview) throw documentFixFailure('review_stale_generation');
    const stamp = await window.ReadMDReview.capture(owner.tab);
    if (!stamp || !ownsDocumentFix(owner) || Boolean(state.editing) !== editing) throw documentFixFailure('review_stale_generation');
    if (editing) {
      // saveAs captures the actual editor/Review draft, resources and target.
      // It preserves pending decisions; a save is not an accept operation.
      $('fix-modal')?.classList.add('hidden');
      if (window.ReadMDReview.snapshot(owner.tab)?.changes?.some(change => change.decision === 'pending')) showToast(documentFixText('fixes.savePending', 'review.pending'));
      return await saveAs();
    }
    if (!window.ReadMDAiHarness || !await window.ReadMDAiHarness.init()) {
      showToast(documentFixText('fixes.copyLimited', 'storage.copyDraft')); return false;
    }
    if (!ownsDocumentFix(owner) || state.editing || state.fixed !== projection) throw documentFixFailure('review_stale_generation');
    const snapshot = window.ReadMDReview.snapshot(owner.tab);
    const activeTask = window.ReadMDAiHarness?.list().some(task => task.document_id === stamp.document_id && ['queued', 'running'].includes(task.state));
    // A display projection has no authority to erase review provenance. Only
    // a clean, history-free source can become an independent corrected copy.
    if (owner.tab.isDirty || !snapshot || !Array.isArray(snapshot.changes) || snapshot.changes.length || snapshot.can_undo !== false || snapshot.can_redo !== false || snapshot.recovery || snapshot.save_intent || snapshot.rename || activeTask) {
      showToast(documentFixText('fixes.copyLimited', 'storage.copyDraft')); return false;
    }
    if (typeof projection !== 'string' || !projection.trim()) return false;
    const tabs = new Set(state.tabs), name = (owner.tab.name || state.sourceName || 'document.md').replace(/\.(?:md|markdown|mkd)$/i, '') + ' ' + documentFixText('storage.copySuffix') + '.md';
    $('fix-modal')?.classList.add('hidden');
    // No resource is copied here. The normal Save As writer owns staging and
    // publication only after the explicit picker/transaction boundary.
    await renderVirtual('fix', name, owner.tab.dir || state.dir || '', projection, [], {
      originPath: owner.path || owner.tab.originPath || null, assets: (owner.tab.webAssets || []).map(asset => ({ ...asset })) });
    const added = state.tabs.filter(tab => !tabs.has(tab) && tab.source === 'fix' && tab.content === projection);
    copy = added.length === 1 ? added[0] : null;
    if (!copy || !ownsDocumentFix(owner, false) || getActiveTab() !== copy || copy.path || (copy.editGeneration || 0)) throw documentFixFailure('review_stale_generation');
    const latest = await window.ReadMDReview.capture(owner.tab);
    if (!sameDocumentFixStamp(stamp, latest) || !ownsDocumentFix(owner, false) || getActiveTab() !== copy || copy.path || copy.content !== projection || (copy.editGeneration || 0)) throw documentFixFailure('review_stale_generation');
    // The original tab remains open and is therefore already in the normal
    // Save As blocked_paths list. Never compute or overwrite a .readmd sidecar.
    const saved = await saveAs();
    if (!saved || !copy.path) { showToast(documentFixText('fixes.copyKept', 'storage.draftKept')); return false; }
    return true;
  } catch (error) {
    // A cancelled picker, failed save or later user edit owns this dirty copy.
    // Do not remove a tab or resources as a rollback shortcut.
    if (copy) showToast(documentFixText('fixes.copyKept', 'storage.draftKept'));
    else notifyDocumentFix(error);
    return false;
  }
}

document.addEventListener('DOMContentLoaded', () => {
  $('fix-ai-btn')?.addEventListener('click', handleAiDocumentFix);
});
