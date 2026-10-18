'use strict';
/* Recovery drafts and save checkpoints are separate from the original file. */
(function () {
  let queue = Promise.resolve(), timer, initialized = false, lastFailure = 0, listEpoch = 0, flushing = false, flushPromise, closing = false, lastFeedback, recoveryCount = 0;
  const t = (key, params) => window.i18n ? window.i18n.t(key, params) : key;
  const keyFor = tab => tab.recoveryKey ||= (tab.path || 'draft:' + tab.id);
  const capture = tab => ({ tab, key: keyFor(tab), path: tab.path || '', name: tab.name || tab.title || 'document.md',
    context: { dir: tab.dir || '', assets: tab.webAssets || [] },
    content: state.activeTabId === tab.id && state.editing ? getEditContent() : (tab.content ?? '') });
  async function request(body) {
    const response = await apiFetch('/api/documents/history', body ? {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body),
    } : {});
    const data = await response.json();
    if (!response.ok || data.ok !== true) throw new Error(data.error === 'recovery_document_too_large' ?
      (body?.kind==='math_draft'?t('storage.mathPolicy',{count:32,size:'512 KiB'}):t('storage.tooLarge')) :
      data.error==='invalid_recovery_document'&&body?.kind==='math_draft'?t('formula.invalid'):(data.error || 'HTTP ' + response.status));
    return data;
  }
  function enqueue(body) {
    const work = queue.then(() => request(body));
    queue = work.catch(() => {});
    return work;
  }
  function feedback(key, params = {}, failed = false) {
    lastFeedback = { key, params, failed };
    const el = $('document-recovery-status');
    if (el) { el.textContent = t(key, params); el.title = el.textContent; el.classList.toggle('err', failed); }
  }
  async function record(snapshot, kind, reason) {
    try {
      const result = await enqueue({ op: 'record', key: snapshot.key, path: snapshot.path, name: snapshot.name,
        kind, reason, content: snapshot.content, context: snapshot.context });
      if (kind === 'draft' && state.activeTabId === snapshot.tab.id) feedback('storage.draftKept');
      return result.entry;
    } catch (error) {
      feedback('storage.recoveryFailed', { error: error.message }, true);
      if (Date.now() - lastFailure > 30000) { lastFailure = Date.now(); showToast(t('storage.recoveryFailed', { error: error.message })); }
      return null;
    }
  }
  function flush() {
    if (flushPromise) return flushPromise;
    flushPromise = flushDrafts().finally(() => { flushPromise = null; });
    return flushPromise;
  }
  async function flushDrafts() {
    flushing = true;
    try {
    clearTimeout(timer);
    if (window.ReadMDInlineMath && !await ReadMDInlineMath.flush()) return false;
    if (state.editing) syncActiveTabDirty();
    for (const tab of state.tabs.filter(tab => tab.path && !tab.isDirty && tab._recoveryContent !== undefined)) {
      try { await enqueue({ op: 'clear_draft', key: keyFor(tab), content: tab._recoveryContent }); tab._recoveryContent = undefined; }
      catch (_) { /* Retry during maintenance. */ }
    }
    const pending = state.tabs.filter(tab => (tab.isDirty || !tab.path) && !tab._recoveryDiscarding);
    for (const tab of pending) {
      const snapshot = capture(tab);
      if (tab._recoveryContent === snapshot.content) continue;
      if (await record(snapshot, 'draft', 'edit')) {
        tab._recoveryContent = snapshot.content;
        if (capture(tab).content !== snapshot.content) setTimeout(schedule, 0);
      }
    }
    } finally { flushing = false; window.ReadMDSession?.persist(); }
  }
  function schedule() { if (flushing) return; clearTimeout(timer); timer = setTimeout(flush, 1500); }
  async function checkpoint(reason = 'ai', tab = getActiveTab()) {
    if (!tab) return null;
    return record(capture(tab), 'checkpoint', reason);
  }
  async function saved(snapshot, tab) {
    // The queued clear follows earlier autosaves. A newer draft is then queued again.
    try { await enqueue({ op: 'clear_draft', key: snapshot.recoveryKey || keyFor(tab), content: snapshot.content, before: snapshot.started }); }
    catch (e) { feedback('storage.recoveryFailed', { error: e.message }, true); }
    tab._recoveryContent = undefined;
    if (tab.isDirty) schedule(); else if (getActiveTab() === tab) feedback('storage.saved');
  }
  async function discard(tab) {
    if (!tab?.isDirty) return true;
    tab._recoveryDiscarding = true;
    const snapshot = capture(tab);
    const entry = await record(snapshot, 'discarded', 'discard');
    if (!entry) { tab._recoveryDiscarding = false; return false; }
    if (capture(tab).content !== snapshot.content) { tab._recoveryDiscarding = false; showToast(t('storage.savedEarlier')); return false; }
    try { await enqueue({ op: 'clear_draft', key: keyFor(tab) }); } catch (_) { /* stored discarded copy remains */ }
    tab._recoveryDiscarding = false;
    return true;
  }
  async function createCopy() {
    const origin = getActiveTab(); if (!origin) return;
    const snap = capture(origin);
    const base = snap.name.replace(/\.[^.]+$/, '');
    let name = base + '-' + t('storage.copySuffix') + '.md', n = 2;
    while (state.tabs.some(tab => tab.name === name)) name = base + '-' + t('storage.copySuffix') + '-' + n++ + '.md';
    await renderVirtual('copy', name, origin.dir || '', snap.content, [], { originPath: origin.path, assets: origin.webAssets || [] });
    await toggleEdit();
    showToast(t('storage.copyDraft'));
  }
  function ensureModal() {
    if ($('document-history-modal')) return;
    const modal = document.createElement('div'); modal.id = 'document-history-modal';
    modal.className = 'modal-overlay hidden'; modal.setAttribute('role', 'dialog'); modal.setAttribute('aria-modal', 'true');
    modal.setAttribute('aria-labelledby', 'document-history-title');
    modal.innerHTML = '<div class="modal-box document-history-box"><div class="modal-header"><h2 id="document-history-title"></h2><button id="document-history-close" type="button" class="tb-btn" data-modal-close>×</button></div><p id="document-history-policy" class="hint"></p><div id="document-history-status" role="status" aria-live="polite"></div><div id="document-history-list" class="document-history-list"></div><div class="modal-actions"><button id="document-history-refresh" class="tb-btn" type="button"></button></div></div>';
    document.body.appendChild(modal);
    $('document-history-close').onclick = () => modal.classList.add('hidden');
    $('document-history-refresh').onclick = refresh;
    modal.addEventListener('click', event => { if (event.target === modal) modal.classList.add('hidden'); });
  }
  async function restore(id, apply) {
    const expectedTab = getActiveTab();
    const expectedView = cmView, previous = expectedTab ? capture(expectedTab).content : '';
    const data = await enqueue({ op: 'read', id });
    if (data.entry.kind === 'math_draft') {
      const source = ReadMDInlineMath.recoveryText(JSON.parse(data.content));
      await renderVirtual('recovery', data.entry.name, data.entry.context?.dir || '', source, [], { originPath:data.entry.path });
      await toggleEdit();
      $('document-history-modal').classList.add('hidden');showToast(t('storage.restoredDraft'));return;
    }
    const baseDir = data.entry.context?.dir || (data.entry.path ? data.entry.path.replace(/[\\/][^\\/]*$/, '') : '');
    const extras = { originPath: data.entry.path, assets: Array.isArray(data.entry.context?.assets) ? data.entry.context.assets : [] };
    const sameOrigin = expectedTab?.path && data.entry.path && normalizePath(expectedTab.path) === normalizePath(data.entry.path);
    if (apply && expectedTab === getActiveTab() && sameOrigin && state.editing && cmView) {
      if (!await checkpoint('restore', expectedTab)) return;
      if (getActiveTab() === expectedTab && state.editing && cmView === expectedView && getEditContent() === previous) {
        cmView.dispatch({ changes: { from: 0, to: cmView.state.doc.length, insert: data.content },
        annotations: window.ReadMDCodeMirror.Transaction.userEvent.of('history.restore') });
        syncActiveTabDirty(); schedule();
      } else {
        await renderVirtual('recovery', data.entry.name, baseDir, data.content, [], extras);
        await toggleEdit();
      }
    } else {
      await renderVirtual('recovery', data.entry.name, baseDir, data.content, [], extras);
      await toggleEdit();
    }
    $('document-history-modal').classList.add('hidden');
    showToast(t('storage.restoredDraft'));
  }
  async function refresh() {
    const epoch = ++listEpoch;
    $('document-history-status').textContent = t('storage.loading');
    $('document-history-refresh').disabled = true;
    try {
      await flush();
      const data = await request();
      if (epoch !== listEpoch) return;
      const list = $('document-history-list'); list.replaceChildren();
      for (const entry of data.entries) {
        const row = document.createElement('article'); row.className = 'document-history-row';
        row.dataset.historyId = entry.id;
        const title = document.createElement('strong'); title.textContent = entry.name;
        const meta = document.createElement('p'); meta.className = 'hint';
        meta.textContent = (['math_draft','math_recovery'].includes(entry.kind) ? t('editor.formula') + ' · ' + t('storage.draftKept') : t('storage.kind.' + entry.kind)) + ' · ' + new Date(entry.created).toLocaleString() + ' · ' + Math.ceil(entry.bytes / 1024) + ' KB';
        const path = document.createElement('p'); path.className = 'hint'; path.textContent = entry.path || t('storage.unsaved');
        row.append(title, meta, path);
        const actions = document.createElement('div'); actions.className = 'document-history-actions';
        const button = (label, action) => {
          const b = document.createElement('button'); b.type = 'button'; b.className = 'tb-btn'; b.textContent = t(label);
          b.onclick = async () => { b.disabled = true; try { await action(); } catch (e) { $('document-history-status').textContent = e.message; } finally { b.disabled = false; } };
          actions.appendChild(b);
        };
        button('storage.openCopy', () => restore(entry.id, false));
        const tab = getActiveTab();
        if (!['math_draft','math_recovery'].includes(entry.kind) && state.editing && tab?.path && entry.path && normalizePath(tab.path) === normalizePath(entry.path)) button('storage.restoreEditor', () => restore(entry.id, true));
        button('storage.delete', async () => {
          if (!await confirmAction({ title: t('storage.delete'), message: t('storage.deleteConfirm'), confirmText: t('storage.delete'), cancelText: t('common.cancel') })) return;
          await enqueue({ op: 'delete', id: entry.id }); await refresh();
        });
        row.appendChild(actions); list.appendChild(row);
      }
      $('document-history-status').textContent = data.entries.length ? t('storage.count', { count: data.entries.length }) : t('storage.empty');
    } catch (e) { if (epoch === listEpoch) $('document-history-status').textContent = t('storage.recoveryFailed', { error: e.message }); }
    finally { if (epoch === listEpoch) $('document-history-refresh').disabled = false; }
  }
  async function open() {
    ensureModal();
    $('document-history-title').textContent = t('storage.history');
    $('document-history-policy').textContent = t('storage.policy')+' '+t('storage.mathPolicy',{count:32,size:'512 KiB'});
    $('document-history-close').setAttribute('aria-label', t('common.close'));
    $('document-history-refresh').textContent = t('storage.refresh');
    $('document-history-modal').classList.remove('hidden'); await refresh();
  }
  function init() {
    if (initialized) return; initialized = true;
    $('btn-document-history').addEventListener('click', open);
    $('btn-document-copy').addEventListener('click', createCopy);
    if (!window.__STARTUP_PROBE__) setTimeout(async () => {
      try {
        const data = await request();
        recoveryCount = data.entries.filter(entry => ['draft','discarded','math_draft','math_recovery'].includes(entry.kind)).length;
        if (recoveryCount) $('btn-document-history').querySelector('em').textContent = t('storage.recoveryAvailable', { count: recoveryCount });
      } catch (_) { /* Opening recovery provides an explicit retry with feedback. */ }
    }, 3000);
    document.addEventListener('visibilitychange', () => { if (document.hidden) flush(); });
    window.addEventListener('pagehide', () => flush());
    // Idle maintenance also covers background dirty tabs. Never writes the original file.
    setInterval(flush, 10000);
  }
  async function prepareClose() {
    if (closing) return false;
    closing = true;
    try {
      if (window.ReadMDInlineMath && !await ReadMDInlineMath.leave()) return false;
      if (state.editing) syncActiveTabDirty();
      if (await flush() === false) return false;
      if (window.ReadMDPdf && !await window.ReadMDPdf.prepareClose()) return false;
      if (window.ReadMDCollaborationShare && !await window.ReadMDCollaborationShare.prepareClose()) return false;
      if (!await confirmExitChanges()) return false;
      await window.ReadMDSession?.flush();
      await queue;
      window.ipc?.postMessage('readmd:quit');
      return true;
    } finally { closing = false; }
  }
  async function prepareInstall() {
    if (window.ReadMDInlineMath && !await ReadMDInlineMath.leave()) return false;
    if (state.editing) syncActiveTabDirty();
    if (await flush() === false) return false;
    if (window.ReadMDPdf && !await window.ReadMDPdf.prepareClose()) return false;
    if (window.ReadMDCollaborationShare && !await window.ReadMDCollaborationShare.prepareClose()) return false;
    if (!await confirmExitChanges()) return false;
    await window.ReadMDSession?.flush();
    await queue;
    return true;
  }
  // App exit is not the same operation as explicitly closing every page.
  async function confirmExitChanges() {
    const candidates = [...state.tabs], discarded = [], accepted = new Map();
    const text = tab => state.editing && state.activeTabId === tab.id ? getEditContent() : tab.content;
    for (const tab of candidates) {
      if (tab.isDirty) {
        const action = await promptDirtyClose(tab.title || tab.name);
        if (action === 'cancel') return false;
        if (action === 'save') {
          await activateTabForSave(tab.id);
          if (!await saveEdit({ exitAfterSave: true }) || tab.isDirty) return false;
        } else {
          if (!await discard(tab)) return false;
          discarded.push(tab);
        }
      }
      accepted.set(tab, text(tab));
    }
    if (state.tabs.length !== candidates.length || candidates.some(tab => !state.tabs.includes(tab) || text(tab) !== accepted.get(tab))) return false;
    for (const tab of discarded) {
      await window.ReadMDCollaborationShare?.detach(tab,true);
      if (!tab.path) state.tabs.splice(state.tabs.indexOf(tab), 1);
      else { tab.content = tab.fixed = tab.original; tab.isDirty = false; tab._recoveryContent = undefined; tab.editorState = null; }
    }
    if (discarded.length) {
      exitEdit();
      if (!getActiveTab()) state.activeTabId = state.tabs.at(-1)?.id || null;
      if (getActiveTab()) { syncStateFromActiveTab(); await renderActiveTab(); }
      else resetHome();
    }
    return true;
  }
  async function sessionDrafts(keys) {
    const wanted = new Set(keys), result = new Map();
    const { entries } = await request();
    for (const entry of entries.filter(e => e.kind === 'draft' && wanted.has(e.doc_key)).sort((a, b) => b.created - a.created)) {
      if (!result.has(entry.doc_key)) {
        const data = await enqueue({ op: 'read', id: entry.id });
        result.set(entry.doc_key, data);
      }
    }
    return result;
  }
  async function mathDrafts(tab) {
    if (tab.mathDrafts) return tab.mathDrafts;
    const key=keyFor(tab),data=await request();
    const drafts=[];
    for (const entry of data.entries.filter(e=>e.kind==='math_draft'&&e.doc_key===key)) {
      const row=await enqueue({op:'read',id:entry.id});drafts.push({entry:row.entry,payload:JSON.parse(row.content)});
    }
    return tab.mathDrafts ||= drafts;
  }
  async function saveMathDraft(tab,payload) {
    try {
      await mathDrafts(tab);
      const result=await enqueue({op:'record',key:keyFor(tab),path:tab.path||'',name:tab.name||tab.title||'document.md',
        kind:'math_draft',reason:'formula',content:JSON.stringify(payload),context:{dir:tab.dir||''}});
      tab.mathDrafts=tab.mathDrafts.filter(d=>d.entry.context.math_slot!==result.entry.context.math_slot);
      tab.mathDrafts.unshift({entry:result.entry,payload});if(getActiveTab()===tab)feedback('storage.draftKept');return true;
    }catch(error){feedback('storage.recoveryFailed',{error:error.message},true);return false;}
  }
  async function removeMathDraft(tab,slot) {
    try {
      for (const row of [...await mathDrafts(tab)]) if(row.entry.context.math_slot===slot){
        await enqueue({op:'delete',id:row.entry.id});tab.mathDrafts=tab.mathDrafts.filter(d=>d.entry.id!==row.entry.id);
      }
      return true;
    }catch(error){feedback('storage.recoveryFailed',{error:error.message},true);return false;}
  }
  window.ReadMDRecovery = { init, schedule, flush, checkpoint, saved, discard, open, createCopy, prepareClose, prepareInstall, sessionDrafts,
    mathDrafts, saveMathDraft, removeMathDraft, mathKept:()=>feedback('storage.draftKept') };
  window.addEventListener('readmd:language-changed', () => {
    if (lastFeedback) feedback(lastFeedback.key, lastFeedback.params, lastFeedback.failed);
    if (recoveryCount) $('btn-document-history').querySelector('em').textContent = t('storage.recoveryAvailable', { count: recoveryCount });
    if ($('document-history-modal') && !$('document-history-modal').classList.contains('hidden')) open();
  });
})();
