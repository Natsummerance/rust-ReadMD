'use strict';
/* ============================================================
   ReadMD Core - Multi-Tab Management & Safety Dialog
   ============================================================ */

/* ---------------- 多标签页系统 (Multi-Tab Management) ---------------- */

let tabRenderEpoch = 0;

function getActiveTab() {
  return state.tabs.find(t => t.id === state.activeTabId) || null;
}

function findTabByPath(filePath) {
  if (!filePath) return null;
  const targetKey = normalizePath(filePath);
  return state.tabs.find(t => t.path && normalizePath(t.path) === targetKey) || null;
}

function syncStateFromActiveTab() {
  const tab = getActiveTab();
  if (!tab) return;
  state.mode = tab.mode || 'file';
  state.source = tab.source || 'file';
  state.browserCopy = tab.browserCopy || false;
  state.sourceName = tab.title || tab.name || '';
  state.file = tab.path;
  state.dir = tab.dir || '';
  state.mtime = tab.mtime || 0;
  state.revision = tab.revision || '';
  state.size = tab.size || 0;
  state.encoding = tab.encoding || 'utf-8';
  state.fixed = tab.content || '';
  state.original = tab.original ?? tab.content ?? '';
  state.fixes = tab.fixes || [];
  state.stats = tab.stats || {};
  state.webAssets = tab.webAssets || [];
  state.is_code = !!tab.is_code;
  state.code_lang = tab.code_lang || '';
  state.ext = tab.ext || '';
  if (tab.readerMode) state.pagination.mode = tab.readerMode;
}

function captureReaderState(tab) {
  if (!tab) return;
  const pagination = state.pagination || {};
  tab.scrollPos = $('content')?.scrollTop || 0;
  const scrollKey = normalizePath(tab.title || tab.name || state.file || '');
  if (scrollKey) state.scrollPos[scrollKey] = tab.scrollPos;
  tab.readerMode = pagination.enabled ? pagination.mode : 'paged';
  tab.readerPage = pagination.enabled && pagination.mode === 'paged'
    ? pagination.currentPage
    : 0;
  tab.continuousScroll = pagination.enabled && pagination.mode === 'continuous'
    ? ($('content')?.scrollTop || 0)
    : (tab.scrollPos || 0);
}

function renderTabsBar() {
  window.ReadMDSession?.persist();
  const bar = $('doc-tabs-bar');
  const secBar = $('doc-tabs-secondary-bar');
  const dropdown = $('doc-tabs-dropdown');
  const overflowWrap = $('doc-tabs-overflow-wrap');
  const btnHome = $('btn-home');
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;

  if (state.tabs.length === 0) {
    if (bar) bar.innerHTML = '';
    if (secBar) { secBar.innerHTML = ''; secBar.classList.add('hidden'); }
    if (overflowWrap) overflowWrap.classList.add('hidden');
    if (btnHome) btnHome.classList.add('hidden');
    if (window.ReadMDGraph && typeof window.ReadMDGraph.updateVisibility === 'function') {
      window.ReadMDGraph.updateVisibility(false);
    }
    return;
  }

  if (btnHome) {
    if (state.mode === 'welcome') {
      btnHome.classList.add('hidden');
    } else {
      btnHome.classList.remove('hidden');
    }
  }

  if (window.ReadMDGraph && typeof window.ReadMDGraph.updateVisibility === 'function') {
    if (state.mode === 'welcome') {
      window.ReadMDGraph.updateVisibility(false);
    } else {
      window.ReadMDGraph.updateVisibility();
    }
  }

  const createTabEl = (tab) => {
    const el = document.createElement('div');
    el.className = 'tab-item' + (tab.id === state.activeTabId ? ' active' : '');
    el.dataset.tabId = tab.id;
    el.setAttribute('role', 'tab');
    el.setAttribute('aria-selected', tab.id === state.activeTabId ? 'true' : 'false');
    el.setAttribute('aria-controls', 'content');
    el.setAttribute('aria-keyshortcuts', 'Alt+Left Arrow Alt+Right Arrow Delete Backspace');
    el.tabIndex = tab.id === state.activeTabId ? 0 : -1;
    el.draggable = true;

    el.title = `${tab.path || tab.title || tab.name} (${_t('tabs.closeTab')}: Delete)`;
    if (tab.isDirty) {
      const dot = document.createElement('span');
      const reason = tab.externalChanged
        ? (_t('toast.saveConflict') || '文件已被外部修改')
        : (_t('tabs.dirty') || '未保存');
      dot.className = tab.externalChanged ? 'tab-dirty tab-external-changed' : 'tab-dirty';
      dot.title = reason;
      dot.setAttribute('aria-hidden', 'true');
      el.setAttribute('aria-description', reason);
      el.appendChild(dot);
    } else if (tab.externalChanged) {
      const dot = document.createElement('span');
      dot.className = 'tab-dirty tab-external-changed';
      dot.title = _t('toast.saveConflict') || '文件已被外部修改';
      dot.setAttribute('aria-hidden', 'true');
      el.setAttribute('aria-description', dot.title);
      el.appendChild(dot);
    }

    const titleSpan = document.createElement('span');
    titleSpan.className = 'tab-title';
    titleSpan.textContent = tab.title || tab.name || (_t('tabs.untitled') || '未命名');
    el.appendChild(titleSpan);

    const closeBtn = document.createElement('button');
    closeBtn.className = 'tab-close';
    closeBtn.innerHTML = '&times;';
    closeBtn.title = _t('tabs.closeTab') || '关闭标签';
    closeBtn.tabIndex = -1;
    closeBtn.setAttribute('aria-hidden', 'true');
    closeBtn.addEventListener('click', e => {
      e.stopPropagation();
      closeTab(tab.id);
    });
    el.appendChild(closeBtn);

    el.addEventListener('click', () => switchTab(tab.id));
    el.addEventListener('keydown', e => {
      if (e.altKey && (e.key === 'ArrowLeft' || e.key === 'ArrowRight')) {
        e.preventDefault();
        const index = state.tabs.findIndex(item => item.id === tab.id);
        const targetIndex = e.key === 'ArrowLeft' ? index - 1 : index + 1;
        if (targetIndex >= 0 && targetIndex < state.tabs.length) {
          reorderTabs(tab.id, state.tabs[targetIndex].id, e.key === 'ArrowRight');
          renderTabsBar();
          focusVisibleTab(tab.id);
        }
        e.stopPropagation();
        return;
      }
      if (['ArrowRight', 'ArrowLeft', 'Home', 'End'].includes(e.key)) {
        e.preventDefault();
        moveTabFocus(tab.id, e.key);
        return;
      }
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        switchTab(tab.id);
        return;
      }
      if (e.key === 'Delete' || e.key === 'Backspace') {
        e.preventDefault();
        closeTab(tab.id);
      }
      if (e.key === 'ContextMenu' || (e.key === 'F10' && e.shiftKey)) {
        e.preventDefault();
        const rect = el.getBoundingClientRect();
        openTabContextMenu({ clientX: rect.left + 12, clientY: rect.bottom + 4 }, tab.id);
      }
    });

    el.addEventListener('dblclick', e => {
      e.stopPropagation();

      startTabInlineRename(tab, titleSpan, el);
    });

    el.addEventListener('contextmenu', e => {
      e.preventDefault();
      openTabContextMenu(e, tab.id);
    });

    el.draggable = true;
    el.addEventListener('dragstart', e => {
      state.isDraggingTab = true;
      e.dataTransfer.setData('application/x-readmd-tab', tab.id);
      e.dataTransfer.setData('text/plain', tab.id);
      e.dataTransfer.effectAllowed = 'move';
      el.classList.add('tab-dragging');
    });
    el.addEventListener('dragover', e => {
      e.preventDefault();
      e.stopPropagation();
      e.dataTransfer.dropEffect = 'move';
      const rect = el.getBoundingClientRect();
      const mid = rect.left + rect.width / 2;
      const isRight = e.clientX > mid;
      el.classList.toggle('tab-drag-over-left', !isRight);
      el.classList.toggle('tab-drag-over-right', isRight);
    });
    el.addEventListener('dragleave', () => {
      el.classList.remove('tab-drag-over-left', 'tab-drag-over-right');
    });
    el.addEventListener('drop', e => {
      e.preventDefault();
      e.stopPropagation();
      el.classList.remove('tab-drag-over-left', 'tab-drag-over-right');
      const srcId = e.dataTransfer.getData('application/x-readmd-tab') || e.dataTransfer.getData('text/plain');
      if (srcId && srcId !== tab.id) {
        const rect = el.getBoundingClientRect();
        const isRight = e.clientX > (rect.left + rect.width / 2);
        reorderTabs(srcId, tab.id, isRight);
      }
    });
    el.addEventListener('dragend', () => {
      state.isDraggingTab = false;
      el.classList.remove('tab-dragging', 'tab-drag-over-left', 'tab-drag-over-right');
      document.querySelectorAll('.tab-item').forEach(t => t.classList.remove('tab-drag-over-left', 'tab-drag-over-right'));
    });

    return el;
  };


  if (bar) {
    bar.innerHTML = '';
    state.tabs.forEach(tab => bar.appendChild(createTabEl(tab)));
  }

  if (dropdown) {
    dropdown.innerHTML = '';
    state.tabs.forEach(tab => {
      const item = document.createElement('button');
      item.setAttribute('role', 'menuitem');
      item.tabIndex = -1;
      item.className = 'doc-tabs-dropdown-item' + (tab.id === state.activeTabId ? ' active' : '');
      item.type = 'button';
      item.setAttribute('role', 'menuitem');
      item.tabIndex = -1;
      const label = document.createElement('span');
      label.textContent = (tab.title || tab.name) + (tab.isDirty ? ' •' : '');
      const meta = document.createElement('small');
      meta.textContent = tab.path || (tab.isVirtual ? (_t('tabs.virtual') || '虚拟') : '');
      item.append(label, meta);
      item.addEventListener('click', () => {
        switchTab(tab.id);
        dropdown.classList.add('hidden');
      });
      dropdown.appendChild(item);
    });

  }

  if (bar && overflowWrap) {
    const syncOverflow = () => {
      if (!bar.isConnected || !overflowWrap.isConnected) return;
      overflowWrap.classList.toggle('hidden', !(bar.scrollWidth > bar.clientWidth + 6));
    };
    syncOverflow();
    requestAnimationFrame(syncOverflow);
  }

  const revealActiveTab = (container) => {
    const active = container?.querySelector('.tab-item.active');
    if (!active) return;
    const containerRect = container.getBoundingClientRect();
    const activeRect = active.getBoundingClientRect();
    const left = activeRect.left - containerRect.left + container.scrollLeft;
    const right = left + active.offsetWidth;
    if (left < container.scrollLeft + 8) {
      container.scrollLeft = Math.max(0, left - 8);
    } else if (right > container.scrollLeft + container.clientWidth - 8) {
      container.scrollLeft = right - container.clientWidth + 8;
    }
  };
  revealActiveTab(bar);
  revealActiveTab(secBar);

  if (secBar) {
    if (window.innerWidth < 650 && state.tabs.length > 0) {
      secBar.innerHTML = '';
      state.tabs.forEach(tab => secBar.appendChild(createTabEl(tab)));
      secBar.classList.remove('hidden');
      if (bar && bar.parentElement) bar.parentElement.classList.add('hidden');
    } else {
      secBar.classList.add('hidden');
      if (bar && bar.parentElement) bar.parentElement.classList.remove('hidden');
    }
  }
}

function focusVisibleTab(tabId) {
  requestAnimationFrame(() => {
    const matches = Array.from(document.querySelectorAll(`[data-tab-id="${CSS.escape(tabId)}"]`));
    const next = matches.find(item => item.offsetParent !== null) || matches[0];
    if (next && document.activeElement !== next) next.focus({ preventScroll: true });
  });
}

function moveTabFocus(activeTabId, key) {
  const index = state.tabs.findIndex(tab => tab.id === activeTabId);
  if (index < 0) return;
  const last = state.tabs.length - 1;
  let target = index;
  if (key === 'ArrowRight') target = index === last ? 0 : index + 1;
  else if (key === 'ArrowLeft') target = index === 0 ? last : index - 1;
  else if (key === 'Home') target = 0;
  else if (key === 'End') target = last;
  if (target === index) return;
  const targetId = state.tabs[target].id;
  switchTab(targetId).then(() => focusVisibleTab(targetId));
}

function startTabInlineRename(tab, titleSpan, tabEl) {
  if (tab?._renamePending) { void recoverPendingRename(tab); return; }
  if (renameBlocked(tab)) return;
  if (tabEl.querySelector('.tab-title-input') || tabEl.querySelector('.tab-rename-wrap')) return;
  const currentTitle = tab.title || tab.name || '';
  const dot = currentTitle.lastIndexOf('.');
  const stem = dot > 0 ? currentTitle.slice(0, dot) : currentTitle;
  const ext = dot > 0 ? currentTitle.slice(dot) : '';

  const tabsTrack = tabEl.parentElement;
  if (tabsTrack) tabsTrack.classList.add('tab-renaming-mode');
  tabEl.classList.add('tab-renaming-active');

  titleSpan.classList.add('hidden');
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  const wrap = document.createElement('div');
  wrap.className = 'tab-rename-wrap';
  const input = document.createElement('input');
  input.type = 'text';
  input.className = 'tab-title-input';
  input.spellcheck = false;
  input.autocomplete = 'off';
  input.setAttribute('aria-label', _t('tabs.rename') || '重命名文件');
  const extension = document.createElement('span');
  extension.className = 'tab-rename-ext';
  extension.textContent = ext;
  wrap.append(input, extension);
  input.value = stem;
  tabEl.insertBefore(wrap, titleSpan);
  input.focus();
  input.select();

  let committed = false;
  const cleanup = () => {
    wrap.remove();
    titleSpan.classList.remove('hidden');
    tabEl.classList.remove('tab-renaming-active');
    if (tabsTrack) tabsTrack.classList.remove('tab-renaming-mode');
  };

  const commit = async () => {
    if (committed) return;
    committed = true;
    const newStem = input.value.trim();
    cleanup();
    if (newStem && newStem !== stem) {
      const newFullName = newStem + ext;
      await renameTab(tab.id, newFullName);
    }
  };

  input.addEventListener('keydown', e => {
    if (e.key === 'Enter') { e.preventDefault(); commit(); }
    else if (e.key === 'Escape') { e.preventDefault(); committed = true; cleanup(); }
  });
  input.addEventListener('blur', () => { setTimeout(() => { if (!committed) commit(); }, 120); });
}


function renameBlocked(tab) {
  if (!tab) return true;
  const t = key => window.i18n?.t(key) || key;
  if (window.ReadMDSession?.busy?.() || tab._renameInFlight || tab._reviewRestorePending) { showToast(t('review.busy')); return true; }
  if (tab.mode === 'file' && (tab.isDirty || (state.editing && state.activeTabId === tab.id))) {
    showToast(t('toast.renameBlockedEdit')); return true;
  }
  return false;
}

function renameFailureMessage(result) {
  const t = key => window.i18n?.t(key) || key;
  const code = result?.error_code || result?.code || '';
  if (/target_exists/.test(code)) return t('convert.existsTitle');
  if (/not_found|operation_missing/.test(code)) return t('error.file_not_found');
  if (/not_authorized/.test(code)) return t('error.review_not_authorized');
  if (/unsaved/.test(code)) return t('toast.renameBlockedEdit');
  if (/busy|in_progress/.test(code)) return t('error.review_save_in_progress');
  if (/identity_changed|revision|conflict/.test(code)) return t('error.review_save_conflict');
  return t('toast.unknownError');
}

function renameRecoveryMessage(result) {
  const t = (key, params) => window.i18n?.t(key, params) || key;
  const target = result?.target_path || result?.path || '';
  const moved = result?.moved === true && target ? t('toast.renamedTo', { name: target.split(/[\\/]/).pop() }) + ' · ' : '';
  return moved + t('error.review_needs_recovery');
}

async function finishRenameReceipt(tab, receipt) {
  const pending = tab?._renamePending;
  if (!pending || !receipt || receipt.operation_id !== pending.operation_id) return false;
  if (!['committed', 'cancelled'].includes(receipt.state) || !receipt.path || !receipt.review ||
      normalizePath(receipt.review.path || '') !== normalizePath(receipt.path) ||
      (pending.review_session && receipt.review.document_id !== pending.review_session.document_id)) return false;
  // A receipt by itself never grants access. Reopen the actual destination and
  // refuse to rebind a tab if another actor changed the saved document.
  const response = await apiFetch('/api/file?p=' + encodeURIComponent(receipt.path));
  if (!response.ok) return false;
  const file = await response.json();
  if (file.revision !== pending.expected_revision || file.disk_revision !== pending.expected_disk_revision || tab.isDirty || tab._reviewRestorePending) return false;
  if (tab.isVirtual && tab._recoveryContent === undefined) {
    tab.original = file.original ?? file.content; tab.content = tab.fixed = file.content;
  }
  if (!window.ReadMDReview?.renamed(tab, receipt.review)) return false;
  const oldPath = tab.path || pending.old_path;
  Object.assign(tab, { mode: 'file', source: 'file', isVirtual: false, path: file.path, dir: file.dir,
    name: file.name, title: file.name, revision: file.revision, mtime: file.mtime || 0, encoding: file.encoding || tab.encoding });
  if (tab.originPath && normalizePath(tab.originPath) === normalizePath(oldPath)) tab.originPath = file.path;
  if (state.activeTabId === tab.id) {
    state.file = file.path; state.sourceName = file.name; state.revision = file.revision; state.mtime = file.mtime || 0;
    document.title = file.name + ' - ReadMD'; setFileTitle(file.name, true, file.path);
  }
  delete tab._renamePending;
  // Do not permit another edit until the resolved path is durably recorded.
  // Otherwise a crash after a later save would restore an obsolete receipt.
  if (!window.ReadMDSession || !await window.ReadMDSession.flush()) {
    tab._renamePending = pending; window.ReadMDSession?.persist(); return false;
  }
  window.ReadMDReview.endRename(tab); window.ReadMDAiHarness?.releaseTab(tab);
  renderTabsBar();
  return true;
}

async function finishAbandonedRename(tab, receipt) {
  const pending = tab?._renamePending;
  if (!pending || receipt?.state !== 'abandoned' || receipt.operation_id !== pending.operation_id) return false;
  const response = await apiFetch('/api/file?p=' + encodeURIComponent(pending.old_path));
  if (!response.ok) return false;
  const file = await response.json();
  if (file.revision !== pending.expected_revision || file.disk_revision !== pending.expected_disk_revision || tab.isDirty || tab._reviewRestorePending) return false;
  if (tab.isVirtual && tab._recoveryContent === undefined) {
    tab.original = file.original ?? file.content; tab.content = tab.fixed = file.content;
  }
  Object.assign(tab, { mode: 'file', source: 'file', isVirtual: false, path: file.path, dir: file.dir,
    name: file.name, title: file.name, revision: file.revision, mtime: file.mtime || 0, encoding: file.encoding || tab.encoding });
  if (state.activeTabId === tab.id) {
    state.file = file.path; state.sourceName = file.name; state.revision = file.revision; state.mtime = file.mtime || 0;
    document.title = file.name + ' - ReadMD'; setFileTitle(file.name, true, file.path);
  }
  delete tab._renamePending;
  if (!window.ReadMDSession || !await window.ReadMDSession.flush()) {
    tab._renamePending = pending; window.ReadMDSession?.persist(); return false;
  }
  window.ReadMDReview?.endRename(tab); window.ReadMDAiHarness?.releaseTab(tab);
  renderTabsBar();
  return true;
}

async function settleUnstartedRename(pending, receipt) {
  if ((receipt?.state === 'not_started' || receipt?.state === 'not_found') && py.rename_abandon)
    return await py.rename_abandon(pending.operation_id);
  return receipt;
}

async function recoverPendingRename(tab) {
  const pending = tab?._renamePending;
  if (!pending || tab._renameInFlight || !hasPy || !py.rename_status) return false;
  tab._renameInFlight = true;
  try {
    if (window.ReadMDSession?.uncertainRename?.(pending.operation_id) && !await window.ReadMDSession.flush()) {
      showToast(window.i18n?.t('review.persistenceFailed') || 'review.persistenceFailed'); return false;
    }
    let receipt = await py.rename_status(pending.operation_id);
    if (receipt?.state === 'not_found') {
      // If the request never reached the server, retry the very same intent
      // only after the normal file-open flow re-establishes authorization.
      const response = await apiFetch('/api/file?p=' + encodeURIComponent(pending.old_path));
      const file = response.ok ? await response.json() : null;
      if (file?.revision === pending.expected_revision && file?.disk_revision === pending.expected_disk_revision) {
        receipt = await py.rename_file(pending.old_path, pending.new_stem, pending);
      }
    }
    receipt = await settleUnstartedRename(pending, receipt);
    if (await finishAbandonedRename(tab, receipt)) return true;
    if (await finishRenameReceipt(tab, receipt)) return true;
    showToast(renameRecoveryMessage(receipt));
    return false;
  } catch (_) {
    showToast(window.i18n?.t('review.busy') || 'review.busy'); return false;
  } finally { delete tab._renameInFlight; }
}

async function renameTab(tabId, newTitle) {
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  const tab = state.tabs.find(t => t.id === tabId);
  if (!tab || !newTitle || newTitle === (tab.title || tab.name)) return;
  if (tab.mode === 'file' && tab.path) {
    if (!hasPy || !py.rename_file) { showToast(_t('review.persistenceFailed')); return; }
    if (tab._renamePending) {
      if (tab._renamePending.refactor) await window.ReadMDRefactor?.recover(tab);
      else await recoverPendingRename(tab);
      return;
    }
    if (renameBlocked(tab)) return;
    if (state.folder && /\.(md|markdown|mdown)$/i.test(tab.path) && window.ReadMDRefactor) { await window.ReadMDRefactor.rename(tab, newTitle); return; }
    tab._renameInFlight = true;
    busy(true);
    let reserved = false, reviewBegun = false, sent = false;
    try {
      const ext = (tab.path.match(/\.[^\\/.]+$/) || [''])[0];
      const stem = ext && newTitle.toLowerCase().endsWith(ext.toLowerCase()) ? newTitle.slice(0, -ext.length) : newTitle;
      const oldPath = tab.path;
      if (window.ReadMDAiHarness && !(reserved = await window.ReadMDAiHarness.reserveTabForRename(tab))) throw new Error('busy');
      const review = await window.ReadMDReview?.beginRename(tab);
      if (!review) throw new Error('busy');
      reviewBegun = true;
      const response = await apiFetch('/api/file?meta=1&p=' + encodeURIComponent(oldPath));
      const file = response.ok ? await response.json() : null;
      if (!file || file.revision !== tab.revision || !/^[0-9a-f]{64}$/.test(file.disk_revision || '') ||
          tab.path !== oldPath || tab.isDirty || (state.editing && state.activeTabId === tab.id)) throw new Error('conflict');
      const pending = { operation_id: 'rename-' + crypto.randomUUID(), old_path: oldPath, new_stem: stem,
        expected_revision: file.revision, expected_disk_revision: file.disk_revision, ...(review.review_session ? { review_session: review.review_session } : {}) };
      if (!window.ReadMDSession?.prepareRename || !await window.ReadMDSession.prepareRename(tab, pending)) throw new Error('persistence');
      sent = true;
      let receipt = await py.rename_file(oldPath, stem, pending);
      if (receipt?.state === 'not_started') {
        const failure = receipt;
        receipt = await settleUnstartedRename(pending, receipt);
        if (await finishAbandonedRename(tab, receipt)) { showToast(_t('toast.renameFailed') + renameFailureMessage(failure)); return; }
      }
      if (!['committed', 'cancelled'].includes(receipt?.state) && py.rename_status) receipt = await py.rename_status(pending.operation_id);
      if (await finishRenameReceipt(tab, receipt)) {
        if (receipt.state === 'committed') showToast(_t('toast.renamedTo', { name: tab.name }));
        else showToast(_t('toast.renameFailed') + renameFailureMessage(receipt));
      } else showToast(renameRecoveryMessage(receipt));
    } catch (e) {
      if (!sent && !window.ReadMDSession?.uncertainRename?.(tab._renamePending?.operation_id)) delete tab._renamePending;
      showToast(_t(e.message === 'busy' ? 'review.busy' : e.message === 'persistence' ? 'review.persistenceFailed' : 'review.conflict'));
    } finally {
      if (!tab._renamePending) {
        if (reviewBegun) window.ReadMDReview?.endRename(tab);
        if (reserved) window.ReadMDAiHarness?.releaseTab(tab);
        window.ReadMDSession?.persist();
      }
      delete tab._renameInFlight;
      busy(false);
    }
  } else {
    tab.title = newTitle;
    tab.name = newTitle;

    if (state.activeTabId === tab.id) {
      state.sourceName = newTitle;
      document.title = newTitle + ' - ReadMD';
    }
  }
  renderTabsBar();
}

function reorderTabs(srcId, destId, insertAfter = false) {
  if (window.ReadMDSession?.busy?.()) return;
  const srcIdx = state.tabs.findIndex(t => t.id === srcId);
  if (srcIdx < 0) return;
  const [removed] = state.tabs.splice(srcIdx, 1);
  let destIdx = state.tabs.findIndex(t => t.id === destId);
  if (destIdx < 0) {
    state.tabs.push(removed);
  } else {
    if (insertAfter) destIdx++;
    state.tabs.splice(destIdx, 0, removed);
  }
  renderTabsBar();
}


function openTabContextMenu(e, tabId) {
  const menu = $('tab-context-menu');
  if (!menu) return;
  menu.dataset.tabId = tabId;
  const tabIndex = state.tabs.findIndex(tab => tab.id === tabId);
  menu.querySelector('[data-action="move-left"]').disabled = tabIndex <= 0;
  menu.querySelector('[data-action="move-right"]').disabled = tabIndex < 0 || tabIndex >= state.tabs.length - 1;
  menu.style.left = Math.min(window.innerWidth - 180, e.clientX) + 'px';
  menu.style.top = Math.min(window.innerHeight - 200, e.clientY) + 'px';
  menu.classList.remove('hidden');
  menu.dataset.returnFocus = tabId;
  setTimeout(() => menu.querySelector('button:not([disabled])')?.focus(), 20);
}

function closeTabContextMenu({ restoreFocus = false } = {}) {
  const menu = $('tab-context-menu');
  if (!menu || menu.classList.contains('hidden')) return;
  menu.classList.add('hidden');
  if (!restoreFocus) return;
  const tabId = menu.dataset.returnFocus;
  if (tabId) focusVisibleTab(tabId);
}

async function renderActiveTab({ restoreScroll = false } = {}) {
  const nextTab = getActiveTab();
  if (!nextTab) return;
  await window.ReadMDReview?.restore(nextTab);
  if (getActiveTab() !== nextTab) return;
  syncStateFromActiveTab();
  const scrollKey = normalizePath(nextTab.title || nextTab.name || nextTab.path || '');
  if (scrollKey && typeof nextTab.scrollPos === 'number') {
    state.scrollPos[scrollKey] = nextTab.scrollPos;
  }
  const renderEpoch = ++tabRenderEpoch;
  setFixes(nextTab.fixes || [], nextTab.stats || {});
  await renderContent(nextTab.content, nextTab.title || nextTab.name);
  if (renderEpoch !== tabRenderEpoch) return;
  document.title = (nextTab.title || nextTab.name) + ' - ReadMD';
  setFileTitle(nextTab.title || nextTab.name, !nextTab.isVirtual && hasPy, nextTab.path);
  if (restoreScroll && nextTab.scrollPos) {
    requestAnimationFrame(() => { $('content').scrollTop = nextTab.scrollPos; });
  }
  updateStatus();
  renderTabsBar();
  afterRender();
}

function syncActiveTabDirty() {
  if (!state.editing) return;
  const tab = getActiveTab();
  if (!tab) return;
  const dirty = hasUnsavedEditorChanges();
  {
    tab.content = getEditContent();
    tab.fixed = tab.content;
  }
  if (tab.isDirty !== dirty) {
    tab.isDirty = dirty;
    renderTabsBar();
  }
  window.ReadMDRecovery?.schedule();
}

async function activateTabForSave(tabId) {
  if (window.ReadMDSession?.busy?.()) return false;
  if (state.activeTabId !== tabId) {
    const previousTab = getActiveTab();
    if (previousTab && window.ReadMDReview && !await window.ReadMDReview.flush(previousTab)) return false;
    if (previousTab && state.editing) {
      if (hasUnsavedEditorChanges()) {
        previousTab.content = getEditContent();
        previousTab.fixed = previousTab.content;
        previousTab.isDirty = true;
      }
    }
    exitEdit();
    state.activeTabId = tabId;
    syncStateFromActiveTab();
    renderActiveTab();
  }
  if (!state.editing) await toggleEdit();
  const nextTab = getActiveTab();
  const draft = nextTab ? nextTab.content : '';
  if (typeof cmView !== 'undefined' && cmView) {
    cmView.dispatch({ changes: { from: 0, to: cmView.state.doc.length, insert: draft } });
  } else if ($('edit-area')) {
    $('edit-area').value = draft;
  }
}

async function switchTab(tabId, { _sessionQueued = false } = {}) {
  if (window.ReadMDSession?.busy?.()) return;
  if (state.activeTabId === tabId) return;
  if(window.ReadMDInlineMath&&!await ReadMDInlineMath.leave())return;
  window.invalidateDocumentLoads?.();
  const prevTab = getActiveTab();
  if (prevTab && window.ReadMDReview && !await window.ReadMDReview.flush(prevTab)) return;
  if (prevTab) {
    if (state.editing) {
      if (hasUnsavedEditorChanges()) {
        prevTab.isDirty = true;
        const action = await promptDirtyClose(prevTab.title || prevTab.name || 'document');
        if (action === 'cancel') return;
        if (action === 'save') {
          await saveEdit({ exitAfterSave: true });
          if (state.editing) return;
        } else {
          if (window.ReadMDRecovery && !await window.ReadMDRecovery.discard(prevTab)) return;
          await window.ReadMDCollaborationShare?.detach(prevTab,true);
          exitEdit();
          prevTab.content = prevTab.original;
          prevTab.fixed = prevTab.original;
          prevTab.isDirty = false;
        }
      }
      else {
        exitEdit();
      }
    }
    captureReaderState(prevTab);
  }
  exitEdit();
  state.activeTabId = tabId;
  syncStateFromActiveTab();
  renderTabsBar();
  const nextTabState = getActiveTab();
  renderTabsBar();
  if (nextTabState?.externalChanged && nextTabState.path && !nextTabState.isDirty) {
    nextTabState.externalChanged = false;
    await loadFile(nextTabState.path, { force: true, _sessionQueued });
    return;
  }
  const preferredPage = Number(nextTabState?.readerPage || 0);
  const switchedTabId = tabId;
  const rendered = renderActiveTab({ restoreScroll: true });
  Promise.resolve(rendered).then(() => {
    if (state.activeTabId !== switchedTabId) return;
    if (state.pagination.enabled && state.pagination.mode === 'paged' && preferredPage >= 0) {
      renderPage(preferredPage, null, true);
    } else if (state.pagination.enabled && state.pagination.mode === 'continuous' && nextTabState.continuousScroll) {
      requestAnimationFrame(() => { $('content').scrollTop = nextTabState.continuousScroll; });
    }
  });
}

function promptDirtyClose(tabName) {
  const _t = (k, p) => {
    if (window.i18n && window.i18n.t) {
      const v = window.i18n.t(k, p);
      if (v && v !== k) return v;
    }
    return null;
  };
  return new Promise(resolve => {
    const modal = $('close-confirm-modal');
    if (!modal) {
      resolve('cancel');
      return;
    }
    const titleEl = $('close-confirm-title');
    if (titleEl) titleEl.textContent = _t('dialog.unsavedTitle') || '是否保存对文档的修改？';
    const descEl = $('close-confirm-desc');
    if (descEl) descEl.textContent = _t('dialog.unsavedMsgDesc', { name: tabName || (_t('tabs.untitled') || '文档') }) || `「${tabName || '文档'}」已被修改，如果直接关闭，未保存的内容将会丢失。`;
    modal.classList.remove('hidden');

    const onKeyDown = (e) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        cleanUp('cancel');
      }
    };

    const onBackdropClick = (e) => {
      if (e.target === modal) {
        cleanUp('cancel');
      }
    };

    const cleanUp = (action) => {
      modal.classList.add('hidden');
      $('close-confirm-save').onclick = null;
      $('close-confirm-discard').onclick = null;
      $('close-confirm-cancel').onclick = null;
      modal.removeEventListener('click', onBackdropClick);
      document.removeEventListener('keydown', onKeyDown);
      resolve(action);
    };

    $('close-confirm-save').onclick = () => cleanUp('save');
    $('close-confirm-discard').onclick = () => cleanUp('discard');
    $('close-confirm-cancel').onclick = () => cleanUp('cancel');
    modal.addEventListener('click', onBackdropClick);
    document.addEventListener('keydown', onKeyDown);

    // 聚焦主要行动按钮
    const cancelBtn = $('close-confirm-cancel');
    if (cancelBtn) setTimeout(() => cancelBtn.focus(), 30);
  });
}

/**
 * 通用多选一对话框。choices: [{id, label, kind?: 'accent'|'danger'}]；
 * Esc / 点击遮罩返回 'cancel'。焦点困在对话框内，关闭后还给触发元素。
 */
function askChoice(title, desc, choices) {
  return new Promise(resolve => {
    const modal = $('choice-modal');
    const actions = $('choice-actions');
    if (!modal || !actions) { resolve('cancel'); return; }
    const opener = document.activeElement;
    $('choice-title').textContent = title || '';
    $('choice-desc').textContent = desc || '';
    actions.innerHTML = '';
    const buttons = choices.map(c => {
      const b = document.createElement('button');
      b.type = 'button';
      b.className = 'tb-btn' + (c.kind ? ' ' + c.kind : '');
      b.textContent = c.label;
      b.onclick = () => done(c.id);
      actions.appendChild(b);
      return b;
    });
    const onKey = e => {
      if (e.key === 'Escape') { e.preventDefault(); done('cancel'); return; }
      if (e.key === 'Tab' && buttons.length) {
        const i = buttons.indexOf(document.activeElement);
        const next = e.shiftKey ? (i <= 0 ? buttons.length - 1 : i - 1) : (i + 1) % buttons.length;
        e.preventDefault();
        buttons[next].focus();
      }
    };
    const onBackdrop = e => { if (e.target === modal) done('cancel'); };
    function done(id) {
      modal.classList.add('hidden');
      modal.removeEventListener('click', onBackdrop);
      document.removeEventListener('keydown', onKey, true);
      actions.innerHTML = '';
      if (opener && typeof opener.focus === 'function') opener.focus();
      resolve(id);
    }
    modal.addEventListener('click', onBackdrop);
    document.addEventListener('keydown', onKey, true);
    modal.classList.remove('hidden');
    setTimeout(() => (buttons[0] || modal).focus(), 30);
  });
}


async function closeTab(tabId, force = false) {
  if (window.ReadMDSession?.busy?.()) return;
  const tab = state.tabs.find(t => t.id === tabId);
  if (!tab) return;
  if (tab._renamePending || tab._renameInFlight) { showToast(window.i18n.t('review.busy')); return; }
  if(state.activeTabId===tabId&&window.ReadMDInlineMath&&!await ReadMDInlineMath.leave())return;
  if (window.ReadMDReview && !await window.ReadMDReview.flush(tab)) return;
  if (state.editing && state.activeTabId === tabId) syncActiveTabDirty();
  let discardShared=false;
  if (tab.isDirty && !force) {
    const action = await promptDirtyClose(tab.title || tab.name);
    if (action === 'cancel') return;
    discardShared=action==='discard';
    if (action === 'save') {
      await activateTabForSave(tabId);
      const saved = await saveEdit({ exitAfterSave: true });
      if (!saved || (getActiveTab()?.id === tabId && state.editing)) return;
    }
  }
  const idx = state.tabs.findIndex(t => t.id === tabId);
  if (idx < 0) return;
  const accepted = new Map([[tab, state.editing && getActiveTab() === tab ? getEditContent() : tab.content]]);
  if (window.ReadMDAiHarness && !await window.ReadMDAiHarness.pauseTab(tab)) return;
  try {
  const focusedTabId = document.activeElement instanceof Element ? document.activeElement.dataset.tabId : null;
  let closedIndex = -1, wasActive = false;
  const remove = () => {
    closedIndex = state.tabs.indexOf(tab);
    if (closedIndex < 0) return;
    state.tabs.splice(closedIndex, 1);
    wasActive = state.activeTabId === tabId;
    if (wasActive) {
      exitEdit();
      const next = state.tabs[Math.min(closedIndex, state.tabs.length - 1)];
      state.activeTabId = next?.id || null;
      if (next) syncStateFromActiveTab();
      else resetHome();
    }
  };
  if (window.ReadMDSession?.closeTabs) {
    if (!await window.ReadMDSession.closeTabs([tab], remove, accepted)) return;
  } else remove();
  // The confirmed session no longer references this page. Until that ACK,
  // its only draft and visible source must survive a failed close.
  if (discardShared && window.ReadMDRecovery) await window.ReadMDRecovery.discard(tab);
  await window.ReadMDCollaborationShare?.detach(tab,discardShared);
  if (wasActive) {
    await renderActiveTab({ restoreScroll: true });
    focusVisibleTab(state.activeTabId);
  }
  renderTabsBar();
  if (focusedTabId === tabId) {
    const fallbackTab = state.tabs[Math.min(idx, state.tabs.length - 1)];
    focusVisibleTab(fallbackTab ? fallbackTab.id : state.activeTabId);
  }
  } finally { window.ReadMDAiHarness?.releaseTab(tab); }
}

async function closeOtherTabs(keepTabId) {
  if (window.ReadMDSession?.busy?.()) return;
  const keepTab = state.tabs.find(t => t.id === keepTabId);
  if (!keepTab) return;
  if (state.tabs.some(tab => tab !== keepTab && (tab._renamePending || tab._renameInFlight))) { showToast(window.i18n.t('review.busy')); return; }
  if(window.ReadMDInlineMath&&!await ReadMDInlineMath.leave())return;
  if (window.ReadMDReview && !await window.ReadMDReview.flushAll()) return;
  if (state.editing) syncActiveTabDirty();
  const candidates = [...state.tabs], accepted = new Map(), discardedShared=new Set();
  const text = tab => state.editing && state.activeTabId === tab.id ? getEditContent() : tab.content;
  for (const t of candidates) {
    if (t.id !== keepTabId) {
      if (t.isDirty) {
        const action = await promptDirtyClose(t.title || t.name);
        if (action === 'cancel') return;
        if (action === 'discard') discardedShared.add(t);
      if (action === 'save') {
        await activateTabForSave(t.id);
        const saved = await saveEdit({ exitAfterSave: true });
        if (!saved || state.tabs.some(item => item.id === t.id && item.isDirty)) return;
        }
      }
      accepted.set(t, text(t));
    }
  }
  if (state.tabs.length !== candidates.length || candidates.some(tab => !state.tabs.includes(tab)) || [...accepted].some(([tab, content]) => text(tab) !== content)) return;
  const pausedTabs = [];
  try {
  for (const tab of candidates) if (tab !== keepTab && window.ReadMDAiHarness) {
    if (!await window.ReadMDAiHarness.pauseTab(tab)) return;
    pausedTabs.push(tab);
  }
  const remove = () => {
    state.tabs = [keepTab];
    window.invalidateDocumentLoads?.();
    if (state.activeTabId !== keepTabId) exitEdit();
    state.activeTabId = keepTabId;
    syncStateFromActiveTab();
  };
  if (window.ReadMDSession?.closeTabs) {
    if (!await window.ReadMDSession.closeTabs(candidates.filter(tab => tab !== keepTab), remove, accepted)) return;
  } else remove();
  for (const tab of discardedShared) if (window.ReadMDRecovery) await window.ReadMDRecovery.discard(tab);
  for (const tab of candidates) if (tab !== keepTab) await window.ReadMDCollaborationShare?.detach(tab,discardedShared.has(tab));
  renderTabsBar();
  } finally { for (const tab of pausedTabs) window.ReadMDAiHarness?.releaseTab(tab); }
}

async function closeAllTabs() {
  if (window.ReadMDSession?.busy?.()) return;
  if (state.tabs.some(tab => tab._renamePending || tab._renameInFlight)) { showToast(window.i18n.t('review.busy')); return; }
  if(window.ReadMDInlineMath&&!await ReadMDInlineMath.leave())return;
  if (window.ReadMDReview && !await window.ReadMDReview.flushAll()) return;
  if (state.editing) syncActiveTabDirty();
  const candidates = [...state.tabs], accepted = new Map(), discardedShared=new Set();
  const text = tab => state.editing && state.activeTabId === tab.id ? getEditContent() : tab.content;
  for (const t of candidates) {
    if (t.isDirty) {
      const action = await promptDirtyClose(t.title || t.name);
      if (action === 'cancel') return;
      if (action === 'discard') discardedShared.add(t);
      if (action === 'save') {
        await activateTabForSave(t.id);
        const saved = await saveEdit({ exitAfterSave: true });
        if (!saved || (getActiveTab()?.isDirty || state.editing)) return;
      }
    }
    accepted.set(t, text(t));
  }
  if (state.tabs.length !== candidates.length || candidates.some(tab => !state.tabs.includes(tab)) || [...accepted].some(([tab, content]) => text(tab) !== content)) return;
  const pausedTabs = [];
  try {
  for (const tab of candidates) if (window.ReadMDAiHarness) {
    if (!await window.ReadMDAiHarness.pauseTab(tab)) return;
    pausedTabs.push(tab);
  }
  const remove = () => { exitEdit(); state.tabs = []; state.activeTabId = null; resetHome(); };
  if (window.ReadMDSession?.closeTabs) {
    if (!await window.ReadMDSession.closeTabs(candidates, remove, accepted)) return;
  } else remove();
  for (const tab of discardedShared) if (window.ReadMDRecovery) await window.ReadMDRecovery.discard(tab);
  for (const tab of candidates) await window.ReadMDCollaborationShare?.detach(tab,discardedShared.has(tab));
  renderTabsBar();
  } finally { for (const tab of pausedTabs) window.ReadMDAiHarness?.releaseTab(tab); }
}

/* Persist the open page set, never the recent-file list. Document bodies stay
   in the existing bounded recovery store, outside the user's folders. */
(function () {
  const storageKey = 'readmd-session';
  let ready = false, timer, pending, lastSaved = '', notified = false;
  let sessionVersion = null, eligible = false, conflicted = false, unconfirmed = null;
  let fence = 0, closing = false;
  const confirmedRenames = new Set();
  function rememberRenames(value) {
    for (const tab of Array.isArray(value?.tabs) ? value.tabs : []) if (tab?.renamePending?.operation_id) confirmedRenames.add(tab.renamePending.operation_id);
  }
  const idleWaiters = new Set();
  async function whenIdle() {
    if (closing) await new Promise(resolve => idleWaiters.add(resolve));
    await window.ReadMDRefactor?.whenIdle?.();
  }
  const nativeCas = () => hasPy && window.READMD_ENGINE === 'rust';
  const version = value => value && typeof value.epoch === 'string' && /^[a-f0-9-]{36}$/i.test(value.epoch) &&
    typeof value.generation === 'string' && /^(0|[1-9][0-9]{0,31})$/.test(value.generation)
    ? { epoch: value.epoch, generation: value.generation } : null;
  const canonical = value => Array.isArray(value) ? value.map(canonical) : value && typeof value === 'object'
    ? Object.fromEntries(Object.keys(value).sort().map(key => [key, canonical(value[key])])) : value;
  function failed() {
    if (!notified) { notified = true; showToast(window.i18n.t('storage.recoveryFailed', { error: window.i18n.t('review.persistenceFailed') })); }
    return false;
  }
  function bounded(value) {
    if (!value || value.version !== 1 || !Array.isArray(value.tabs) || value.tabs.length > 100) return null;
    const seen = new Set(), tabs = [];
    const text = (s, limit = 32768) => typeof s === 'string' && s.length <= limit ? s : '';
    const number = n => Number.isFinite(n) ? Math.max(0, Math.min(n, 1e9)) : 0;
    for (const item of value.tabs) {
      if (!item || typeof item !== 'object') continue;
      const key = text(item.key), path = text(item.path), name = text(item.name, 512);
      if (!key || seen.has(path ? normalizePath(path) : key)) continue;
      seen.add(path ? normalizePath(path) : key);
      const rawRename = item.renamePending;
      let renamePending = null;
      if (rawRename && typeof rawRename === 'object' && /^[a-zA-Z0-9_:-]{1,128}$/.test(rawRename.operation_id || '') &&
          /^[0-9a-f]{64}$/.test(rawRename.expected_disk_revision || '') && /^[0-9a-f]{64}$/.test(rawRename.expected_revision || '') &&
          text(rawRename.old_path) && text(rawRename.new_stem, 512) && !/[\\/]/.test(rawRename.new_stem)) {
        if (rawRename.refactor === true && (!/^[0-9a-f]{64}$/.test(rawRename.plan_hash || '') || !text(rawRename.workspace_root))) return null;
        const review = rawRename.review_session;
        renamePending = { operation_id: rawRename.operation_id, old_path: rawRename.old_path, new_stem: rawRename.new_stem,
          expected_disk_revision: rawRename.expected_disk_revision, expected_revision: rawRename.expected_revision };
        if (rawRename.refactor === true) Object.assign(renamePending, { refactor: true, plan_hash: rawRename.plan_hash, workspace_root: rawRename.workspace_root });
        if (review && typeof review.document_id === 'string' && review.document_id.length <= 180 &&
            Number.isSafeInteger(review.generation) && review.generation >= 0 && Number.isSafeInteger(review.review_revision) && review.review_revision >= 0 &&
            typeof review.call_id === 'string' && review.call_id.length <= 128) {
          renamePending.review_session = { document_id: review.document_id, generation: review.generation,
            review_revision: review.review_revision, call_id: review.call_id };
        }
      }
      tabs.push({ key, path, name, dir: text(item.dir), source: text(item.source, 64), originPath: text(item.originPath), renamePending,
        reviewPath: /^draft:[A-Za-z0-9_.-]{1,180}$/.test(item.reviewPath || '') ? item.reviewPath : '',
        revision: text(item.revision, 256), dirty: item.dirty === true, unsavedCreation: item.unsavedCreation === true,
        readerMode: item.readerMode === 'continuous' ? 'continuous' : 'paged',
        readerPage: number(item.readerPage), scrollPos: number(item.scrollPos) });
    }
    return { version: 1, tabs, active: text(value.active), folder: text(value.folder) };
  }
  function snapshot(excluded = new Set(), activeKey) {
    // Recovery belongs to a tab/document identity, not its current pathname.
    // A newly created file at an old renamed path must never share its drafts.
    const tabs = state.tabs.filter(tab => !excluded.has(tab)).map(tab => ({ key: tab.recoveryKey ||= ((tab.path || tab._renamePending?.old_path ? 'file:' : 'draft:') + tab.id), path: tab.path || tab._renamePending?.old_path || '',
      name: tab.name || tab.title, dir: tab.dir, source: tab.source, originPath: tab.originPath, revision: tab.revision,
      reviewPath: tab.reviewPath || '',
      dirty: !!tab.isDirty, unsavedCreation: !!tab.unsavedCreation, readerMode: tab.readerMode,
      readerPage: tab.readerPage, scrollPos: tab.scrollPos, renamePending: tab._renamePending || null }));
    return bounded({ version: 1, tabs, active: activeKey ?? getActiveTab()?.recoveryKey ?? '', folder: state.folder || '' });
  }
  function persist() {
    if (!ready || closing) return;
    const value = snapshot();
    if (!value) return;
    const encoded = JSON.stringify(value);
    try { localStorage.setItem(storageKey, encoded); } catch (_) { /* Native settings remain authoritative. */ }
    if (nativeCas() && (!eligible || conflicted)) return;
    if (encoded === lastSaved) { pending = null; clearTimeout(timer); return; }
    pending = { value, encoded, fence };
    clearTimeout(timer);
    timer = setTimeout(write, 120);
  }
  async function sendCas(request) {
    const response = await apiFetch('/api/session', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(request) });
    const result = await response.json();
    const acknowledged = version(result?.session_version), expected = version(request.expected_session_version);
    // A durable receipt certifies this exact request, not merely some valid
    // session/version returned with HTTP 200. Decimal strings stay exact even
    // beyond Number.MAX_SAFE_INTEGER; JSON object ordering is immaterial.
    const bound = result?.protocol === 'readmd-session-cas-v1' && result.call_id === request.call_id &&
      acknowledged && expected && acknowledged.epoch === expected.epoch &&
      BigInt(acknowledged.generation) === BigInt(expected.generation) + 1n &&
      JSON.stringify(canonical(result.session)) === JSON.stringify(canonical(request.session));
    if (!response.ok || result?.ok !== true || result.persisted !== true || !bound) {
      if (response.status === 409) conflicted = true;
      throw new Error('session_save_failed');
    }
    sessionVersion = acknowledged;
    rememberRenames(request.session);
    lastSaved = JSON.stringify(request.session);
    unconfirmed = null;
    return true;
  }
  async function save(next, force) {
    if (nativeCas()) {
      if (!eligible || conflicted || !sessionVersion) return failed();
      // An uncertain acknowledgement must settle its exact request first.
      // A new strict CAS afterwards proves this head is still current; an old
      // idempotent receipt alone is not a lease over another process's writes.
      if (unconfirmed) await sendCas(unconfirmed);
      if (!force && next.encoded === lastSaved) return true;
      unconfirmed = { session: next.value, expected_session_version: { ...sessionVersion }, call_id: 'session-' + crypto.randomUUID() };
      return sendCas(unconfirmed);
    }
    if (hasPy) {
      const result = await py.save_settings({ session: next.value });
      if (result === false || result?.ok === false) throw new Error('session_save_failed');
    } else localStorage.setItem(storageKey, next.encoded);
    lastSaved = next.encoded;
    return true;
  }
  function write(force = false, explicit) {
    clearTimeout(timer);
    const next = explicit ? { value: explicit, encoded: JSON.stringify(explicit), fence } : pending;
    pending = null;
    if (!next) return settingsSaveQueue;
    settingsSaveQueue = settingsSaveQueue.catch(() => false).then(async () => {
      if (next.fence !== fence) return false;
      try {
        return await save(next, force);
      } catch (error) {
        return failed();
      }
    });
    return settingsSaveQueue;
  }
  async function recoveryBarrier(allowClose = false) {
    if (!ready) await window.__readmdSessionReady;
    if (window.ReadMDReview && !await window.ReadMDReview.flushAll()) return false;
    if (await window.ReadMDRecovery?.flush() === false) return false;
    captureReaderState(getActiveTab());
    return !closing || allowClose;
  }
  async function flush(allowClose = false) {
    if (!await recoveryBarrier(allowClose)) return false;
    const value = snapshot();
    if (!value) return false;
    try { localStorage.setItem(storageKey, JSON.stringify(value)); } catch (_) {}
    return (await write(true, value)) === true;
  }
  async function prepareRename(tab, intent) {
    // Do not expose an intent to snapshot()/autosave while recovery is still
    // pending. Otherwise a successful timer can publish it even when this
    // barrier later refuses the rename before its bridge request is sent.
    if (!await recoveryBarrier()) return false;
    if (closing || !state.tabs.includes(tab) || tab.path !== intent.old_path || tab._renamePending || tab.isDirty ||
        (state.editing && state.activeTabId === tab.id)) return false;
    tab._renamePending = intent;
    const value = snapshot();
    try { localStorage.setItem(storageKey, JSON.stringify(value)); } catch (_) {}
    // No await separates publishing the local intent and queueing its CAS.
    return (await write(true, value)) === true;
  }
  async function closeTabs(tabs, apply, accepted) {
    if (closing || window.ReadMDRefactor?.busy?.() || tabs.some(tab => !state.tabs.includes(tab) || tab._renamePending || tab._renameInFlight)) return false;
    closing = true; fence++; clearTimeout(timer); pending = null;
    const reserved = [], flags = new Map();
    const body = document.body, inert = body?.inert;
    try {
      if (!await flush(true)) return false;
      const owners = [...state.tabs];
      for (const tab of owners) { flags.set(tab, tab._renameInFlight); tab._renameInFlight = true; }
      if (body) body.inert = true;
      // Existing review reservations defer completed AI receipts as well as
      // writes. A large/unseen reader without a review snapshot needs no CAS.
      for (const tab of owners) if (window.ReadMDReview?.snapshot(tab)) {
        if (!await window.ReadMDReview.beginRename(tab)) return false;
        reserved.push(tab);
      }
      if (tabs.some(tab => !state.tabs.includes(tab) || tab._renamePending)) return false;
      if (accepted && [...accepted].some(([tab, text]) =>
        (state.editing && getActiveTab() === tab ? getEditContent() : tab.content) !== text)) return false;
      if (await window.ReadMDRecovery?.flush() === false) return false;
      const excluded = new Set(tabs), remaining = state.tabs.filter(tab => !excluded.has(tab));
      const active = getActiveTab();
      const next = excluded.has(active) ? remaining[Math.min(state.tabs.indexOf(active), remaining.length - 1)] : active;
      const value = snapshot(excluded, next?.recoveryKey || '');
      const stamp = () => JSON.stringify({ session: snapshot(), load: documentLoadEpoch,
        text: state.tabs.map(tab => [tab.id, tab.editGeneration || 0, state.editing && getActiveTab() === tab ? getEditContent() : tab.content]) });
      const before = stamp();
      if (!value || !await write(true, value)) return false;
      if (stamp() !== before) {
        // An external open or programmatic edit can outlive the UI fence.
        // Retain the actual current pages and drafts, and compensate only
        // against our acknowledged version, never a newly fetched version.
        await window.ReadMDRecovery?.flush();
        await write(true, snapshot());
        return failed();
      }
      apply(); // No await between the verified acknowledgement and UI removal.
      fence++;
      try { localStorage.setItem(storageKey, JSON.stringify(snapshot())); } catch (_) {}
      return true;
    } catch (_) { return failed(); }
    finally {
      for (const tab of reserved) window.ReadMDReview.endRename(tab);
      for (const [tab, previous] of flags) {
        if (previous === undefined) delete tab._renameInFlight;
        else tab._renameInFlight = previous;
      }
      if (body) body.inert = inert;
      closing = false;
      for (const resolve of idleWaiters) resolve();
      idleWaiters.clear();
      persist();
    }
  }
  async function restore(initialFile) {
    const epoch = documentLoadEpoch;
    const untouched = () => epoch === documentLoadEpoch && state.tabs.length === 0 && state.mode === 'welcome';
    let nativeRead = false, applied = false, faithful = false;
    try {
      let stored;
      if (nativeCas()) {
        try {
          const response = await apiFetch('/api/session');
          const result = await response.json();
          const current = version(result?.session_version);
          if (response.ok && result?.ok === true && result.protocol === 'readmd-session-cas-v1' && current && Object.hasOwn(result, 'session')) {
            stored = result.session; sessionVersion = current; nativeRead = true;
            rememberRenames(result.session);
          }
        } catch (_) { /* The mirror can restore a view, never native write eligibility. */ }
      } else if (hasPy) {
        try { const settings = await py.get_settings(); if (settings && typeof settings === 'object' && !Array.isArray(settings)) { stored = settings.session; nativeRead = true; } }
        catch (_) { /* The same-origin mirror can recover a transient settings failure. */ }
      }
      if (!nativeRead) {
        try { stored = JSON.parse(localStorage.getItem(storageKey) || 'null'); } catch (_) {}
      }
      const session = bounded(stored);
      faithful = stored == null || Boolean(session && session.tabs.length === stored.tabs.length &&
        ['active', 'folder'].every(key => stored[key] === undefined || typeof stored[key] === 'string' && stored[key].length <= 32768) &&
        new Set(session.tabs.map(tab => tab.key)).size === session.tabs.length &&
        session.tabs.every((tab, index) => {
          const raw = stored.tabs[index];
          return ['key', 'path', 'name', 'dir', 'source', 'originPath', 'revision', 'reviewPath'].every(key => raw[key] === undefined ||
            typeof raw[key] === 'string' && raw[key].length <= (key === 'name' ? 512 : key === 'source' ? 64 : key === 'revision' ? 256 : 32768)) &&
            ['dirty', 'unsavedCreation'].every(key => raw[key] === undefined || typeof raw[key] === 'boolean') &&
            (!raw.renamePending || JSON.stringify(canonical(tab.renamePending)) === JSON.stringify(canonical(raw.renamePending)));
        }));
      if (!untouched()) {
        // A genuinely empty native snapshot grants no older descriptors to
        // preserve. A user-opened page must not overwrite an interrupted restore.
        applied = stored == null;
        return;
      }
      if (!session?.tabs.length) applied = faithful;
      if (session?.tabs.length) {
        let drafts = new Map();
        let recoveryComplete = true;
        try { drafts = await window.ReadMDRecovery.sessionDrafts(session.tabs.map(tab => tab.key)); }
        catch (_) { recoveryComplete = !session.tabs.some(tab => tab.dirty || tab.unsavedCreation || tab.renamePending); }
        if (!untouched()) return;
        const restored = [];
        // Reconcile each group before ANY member is opened. Parallel source /
        // reference reads could otherwise drop a descriptor while rollback owns it.
        const refactors = new Map();
        for (const descriptor of session.tabs.filter(item => item.renamePending?.refactor)) {
          let result;
          do {
            result = await window.ReadMDRefactor?.restoreDescriptor(descriptor);
            if (result?.settled) break;
            const retry = typeof askChoice === 'function' && await askChoice(window.i18n.t('cmd.rename'), window.i18n.t('error.review_needs_recovery'), [
              { id: 'retry', label: window.i18n.t('reader.refresh') }, { id: 'cancel', label: window.i18n.t('common.cancel') }]);
            if (retry !== 'retry') { eligible = false; window.ReadMDRefactor?.suspendRestore?.(); return; }
          } while (untouched());
          if (!untouched() || !result?.settled) { eligible = false; window.ReadMDRefactor?.suspendRestore?.(); return; }
          refactors.set(descriptor.key, result);
        }
        // Limit concurrent disk reads; a missing document must not block the rest.
        for (let offset = 0; offset < session.tabs.length; offset += 4) {
          const group = await Promise.all(session.tabs.slice(offset, offset + 4).map(async descriptor => {
            let file, receipt, pending = descriptor.renamePending;
            const open = async path => {
              try { const response = await apiFetch('/api/file?p=' + encodeURIComponent(path)); return response.ok ? await response.json() : null; }
              catch (_) { return null; }
            };
            if (pending?.refactor) {
              const result = refactors.get(descriptor.key);
              if (result?.settled) { file = result.file; receipt = result.receipt; pending = null; }
            } else if (pending && hasPy && py.rename_status) {
              try {
                receipt = await py.rename_status(pending.operation_id);
                if (receipt?.state === 'not_found') {
                  const oldFile = await open(pending.old_path);
                  if (oldFile?.revision === pending.expected_revision && oldFile?.disk_revision === pending.expected_disk_revision)
                    receipt = await py.rename_file(pending.old_path, pending.new_stem, pending);
                }
                receipt = await settleUnstartedRename(pending, receipt);
                if (receipt?.state === 'abandoned' && receipt.operation_id === pending.operation_id) {
                  const reopened = await open(pending.old_path);
                  if (reopened?.revision === pending.expected_revision && reopened?.disk_revision === pending.expected_disk_revision) { file = reopened; pending = null; }
                }
                if (pending && receipt?.operation_id === pending.operation_id && ['committed', 'cancelled'].includes(receipt.state)) {
                  const reopened = await open(receipt.path);
                  const review = receipt.review;
                  const sameReview = review?.document_id && (!pending.review_session || review.document_id === pending.review_session.document_id) &&
                    normalizePath(review.path || '') === normalizePath(receipt.path || '') &&
                    window.ReadMDReview?.lf(review.content) === window.ReadMDReview?.lf(reopened?.original ?? reopened?.content);
                  if (reopened?.revision === pending.expected_revision && reopened?.disk_revision === pending.expected_disk_revision && sameReview) { file = reopened; pending = null; }
                }
              } catch (_) { /* Keep the pending receipt and any saved draft. */ }
            } else if (descriptor.path) {
              file = await open(descriptor.path);
            }
            const draft = drafts.get(descriptor.key);
            if ((descriptor.dirty || descriptor.unsavedCreation) && !draft) recoveryComplete = false;
            if (!file && !draft && !pending) return null;
            const dirty = descriptor.dirty && !!draft;
            const content = (!file || dirty) ? (draft?.content ?? receipt?.review?.content ?? '') : file.content;
            return { id: 'tab_' + crypto.randomUUID(), mode: file ? 'file' : 'virtual', source: file ? 'file' : (descriptor.source || 'recovery'),
              path: file?.path || null, dir: file?.dir || draft?.entry.context?.dir || descriptor.dir,
              name: file?.name || descriptor.name || 'document.md', title: file?.name || descriptor.name || 'document.md',
              content, fixed: content, original: file?.original ?? file?.content ?? content,
              revision: dirty ? descriptor.revision : (file?.revision || ''), mtime: file?.mtime || 0,
              size: file?.size || 0, encoding: file?.encoding || 'utf-8', fixes: file?.fixes || [], stats: file?.stats || {},
              is_code: file?.is_code || false, code_lang: file?.code_lang || '', ext: file?.ext || '',
              isVirtual: !file, isDirty: dirty, unsavedCreation: !file && descriptor.unsavedCreation,
              originPath: descriptor.originPath || descriptor.path || null, recoveryKey: descriptor.key,
              reviewPath: descriptor.reviewPath || (/^draft:[A-Za-z0-9_.-]{1,180}$/.test(descriptor.key) ? descriptor.key : ''),
              _recoveryContent: draft?.content, _recoveryReview: draft?.entry.context?.review,
              _reviewRestorePending: Boolean(window.ReadMDReview && draft),
              _renamePending: pending || undefined,
              _renameSettlement: descriptor.renamePending && !pending ? descriptor.renamePending : undefined,
              _renameInFlight: Boolean(descriptor.renamePending && !pending),
              webAssets: Array.isArray(draft?.entry.context?.assets) ? draft.entry.context.assets : [],
              externalChanged: dirty && !!file && descriptor.revision !== file.revision,
              readerMode: descriptor.readerMode, readerPage: descriptor.readerPage, scrollPos: descriptor.scrollPos,
              continuousScroll: descriptor.scrollPos };
          }));
          if (!untouched()) return;
          restored.push(...group.filter(Boolean));
        }
        state.tabs = restored;
        applied = recoveryComplete;
        if (window.ReadMDReview) for (const tab of restored) if (!tab._renamePending) await window.ReadMDReview.restore(tab);
        if (epoch !== documentLoadEpoch) return;
        state.folder = session.folder;
        const active = restored.find(tab => tab.recoveryKey === session.active) || (session.active ? restored.at(-1) : null);
        state.activeTabId = active?.id || null;
        if (active) {
          syncStateFromActiveTab();
          await prepareDocCitations(active.path || active.originPath || '', active.content);
          if (state.activeTabId !== active.id || epoch !== documentLoadEpoch) return;
          await renderActiveTab({ restoreScroll: true });
          if (state.activeTabId !== active.id || epoch !== documentLoadEpoch) return;
          if (state.pagination.enabled && state.pagination.mode === 'paged') renderPage(active.readerPage || 0, null, true);
          if (active.isDirty || (await window.ReadMDRecovery.mathDrafts(active)).length) await toggleEdit();
        } else resetHome();
      }
      // An explicit OS/file URL open becomes active after restoring older pages.
      if (initialFile && epoch === documentLoadEpoch) {
        if (window.ReadMDRefactor?.busy?.()) {
          void window.ReadMDRefactor.whenIdle().then(() => openInitialFile(initialFile)).catch(() => failed());
        } else await openInitialFile(initialFile);
      }
    } finally {
      ready = true;
      eligible = !nativeCas() || (nativeRead && applied && faithful);
      const settled = state.tabs.filter(tab => tab._renameSettlement);
      if (settled.length) {
        persist();
        const saved = await write(true, snapshot());
        for (const tab of settled) {
          if (saved === false) tab._renamePending = tab._renameSettlement;
          delete tab._renameSettlement; delete tab._renameInFlight;
        }
        if (saved === false) persist();
        window.ReadMDRefactor?.finishRestore?.(saved !== false);
      }
      renderTabsBar();
    }
  }
  window.ReadMDSession = { persist, flush, prepareRename, restore, closeTabs, whenIdle, busy: () => closing || Boolean(window.ReadMDRefactor?.busy?.()),
    uncertainRename: id => Boolean(id && (confirmedRenames.has(id) ||
      // A replay conflict cannot prove the original request was rejected: its
      // ACK may have been lost before another window adopted this intent.
      unconfirmed?.session?.tabs.some(tab => tab.renamePending?.operation_id === id))) };
  window.addEventListener('pagehide', () => { captureReaderState(getActiveTab()); persist(); write(); });
  document.addEventListener('visibilitychange', () => { if (document.hidden) { captureReaderState(getActiveTab()); persist(); write(); } });
})();
