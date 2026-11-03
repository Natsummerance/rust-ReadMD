'use strict';
// Frontend ordering only. Rust still owns identity, revisions and every write.
window.ReadMDLifecycle = (() => {
  let active = null, depth = 0, installed = false, applyingSave = null;
  const waiters = new Set();
  const currentTab = () => typeof getActiveTab === 'function' ? getActiveTab() : null;
  const text = tab => state.editing && currentTab() === tab ? getEditContent() : tab?.content;
  const review = tab => {
    const head = window.ReadMDReview?.snapshot(tab);
    return head ? { document_id: head.document_id, generation: head.generation, review_revision: head.review_revision, revision: head.revision } : null;
  };
  function mark(tab) {
    return tab ? { tab, path: tab.path, revision: tab.revision, editGeneration: tab.editGeneration || 0, content: text(tab), review: review(tab) } : null;
  }
  function matches(stamp) {
    if (!stamp) return true;
    const now = mark(stamp.tab);
    return now.path === stamp.path && now.revision === stamp.revision && now.editGeneration === stamp.editGeneration &&
      now.content === stamp.content && JSON.stringify(now.review) === JSON.stringify(stamp.review);
  }
  function accept(tab, stamp, action) {
    if (!matches(stamp)) return false;
    if (active && tab) active.decisions.set(tab, { stamp, action });
    return true;
  }
  function guard(name, work) {
    return function (...args) {
      if (active && !depth || !active && window.ReadMDSession?.restoring?.()) return Promise.resolve(false);
      if (active) return work.apply(this, args);
      const owner = active = { name, target: currentTab(), decisions: new Map() };
      // A result completed during a close/save decision must not invalidate
      // the visible body behind that decision. Wait outside the review FIFO
      // so a nested save/flush never waits on the reserved code write.
      const releaseCode = window.ReadMDCodeRuns?.reserveWrites?.();
      let result;
      try { depth++; result = work.apply(this, args); }
      catch (error) { result = Promise.reject(error); }
      finally { depth--; }
      return Promise.resolve(result).finally(() => {
        if (active === owner) {
          active = null;
          for (const resolve of waiters) resolve();
          waiters.clear();
        }
        releaseCode?.();
      });
    };
  }
  function within(owner, action) {
    if (!owner || owner !== active) return Promise.resolve(false);
    try { depth++; return action(); } finally { depth--; }
  }
  const isOwner = owner => Boolean(owner && owner === active);
  async function whenIdle(owner) {
    while (active && !isOwner(owner)) await new Promise(resolve => waiters.add(resolve));
  }
  function saveReload(path) {
    const tab = currentTab(), decision = active?.decisions.get(tab);
    return Boolean(active && tab?.path && normalizePath(tab.path) === normalizePath(path) && decision?.action === 'save');
  }
  function reviewStamp(stamp) {
    return stamp?.review ? { ...stamp.review, tabId: stamp.tab.id, content: window.ReadMDReview.lf(stamp.content), editGeneration: stamp.editGeneration } : undefined;
  }
  function install() {
    if (installed) return;
    installed = true;
    const originalEditorUpdate = window.ReadMDReview.onEditorUpdate;
    window.ReadMDReview.onEditorUpdate = update => {
      const tab = currentTab();
      if (tab && update.docChanged && applyingSave !== tab) tab.inputGeneration = (tab.inputGeneration || 0) + 1;
      return originalEditorUpdate(update);
    };
    const originalCommit = commitDocumentSave, originalSave = saveEdit;
    commitDocumentSave = function (snapshot, ...args) {
      const previous = applyingSave;
      // The existing commit projects approved path/asset changes before its
      // first await. Those synchronous transactions are part of this save.
      applyingSave = snapshot.tab;
      try { return originalCommit.call(this, snapshot, ...args); }
      finally { applyingSave = previous; }
    };
    const guardedSave = guard('save', async (options = {}) => {
      const tab = currentTab(), inputGeneration = tab?.inputGeneration || 0;
      const saved = await originalSave({ ...options, exitAfterSave: false });
      if (!options.exitAfterSave) return saved;
      if (!saved || !tab || currentTab() !== tab || !state.editing || tab.isDirty || (tab.inputGeneration || 0) !== inputGeneration) return false;
      exitEdit();
      await renderActiveTab({ restoreScroll: true });
      return true;
    });
    saveEdit = function (options) {
      // An ordinary save already carries a document snapshot. Reading or
      // writing another page can continue while that snapshot is committed.
      if (!active && !options?.exitAfterSave) return originalSave(options);
      if (active && !depth) {
        const decision = active.decisions.get(currentTab());
        if (decision?.action !== 'save' || decision.consumed || !matches(decision.stamp)) return Promise.resolve(false);
        decision.consumed = true;
        return within(active, () => guardedSave(options));
      }
      return guardedSave(options);
    };
    // Keep the byte-pinned editor implementation behind this action boundary.
    confirmExitEdit = guard('exit-edit', confirmExitEdit);
    closeTab = guard('close-tab', closeTab);
    closeOtherTabs = guard('close-others', closeOtherTabs);
    closeAllTabs = guard('close-all', closeAllTabs);
    switchTab = guard('switch-tab', switchTab);
    goHome = guard('home', goHome);
    window.ReadMDSession.retryRecovery = guard('recover-page', window.ReadMDSession.retryRecovery);
    newDocument = guard('new-document', async () => {
      const owner = active;
      if (state.editing && !await within(owner, () => confirmExitEdit())) return false;
      await renderVirtual('', '', '', '', [], { _lifecycleOwner: owner });
      const created = currentTab();
      if (!created || !isOwner(owner)) return false;
      await toggleEdit();
      return currentTab() === created && state.editing;
    });
    if (window.ReadMDRecovery) {
      window.ReadMDRecovery.prepareClose = guard('quit', window.ReadMDRecovery.prepareClose);
      window.ReadMDRecovery.prepareInstall = guard('install', window.ReadMDRecovery.prepareInstall);
    }
  }
  return { install, guard, within, owner: () => active, busy: () => Boolean(active), isOwner, whenIdle, saveReload,
    mark, matches, accept, decision: tab => active?.decisions.get(tab), reviewStamp,
    applyingSave: tab => applyingSave === tab };
})();
