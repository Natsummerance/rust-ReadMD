'use strict';
// Office pages use the actual PDF rendering from the original document. Only
// the visible page and its temporary server view live here; session/recovery
// stores retain the original source identity, never PDF or image bytes.
window.ReadMDOffice = (() => {
  let visible = null;
  const isPath = path => typeof path === 'string' && /\.(docx|pptx)$/i.test(path);
  const isTab = tab => tab?.mode === 'office-preview';
  const t = (key, params) => window.i18n ? window.i18n.t(key, params) : key;
  const failure = message => Object.assign(new Error(message), { officePreviewMessage: true });
  const uuid = value => /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(value || '');
  const post = (body, options = {}) => apiFetch('/api/office/preview', {
    method: 'POST', headers: { 'Content-Type': 'application/json' },
    cache: 'no-store', body: JSON.stringify(body), ...options,
  });
  async function closeView(view) {
    if (uuid(view)) try { await post({ view, close: true }); } catch (_) { /* The bounded server view also expires. */ }
  }
  function releaseView() {
    const target = visible;
    if (!target) return Promise.resolve();
    visible = null;
    target.controller.abort();
    target.image.removeAttribute('src');
    if (target.url) URL.revokeObjectURL(target.url);
    $('content')?.classList.remove('is-office-preview');
    if (state.pagination.kind === 'office') {
      delete state.pagination.kind;
      delete state.pagination.loading;
      delete state.pagination.error;
      Object.assign(state.pagination, { enabled: false, pages: [], totalPages: 0, currentPage: 0 });
      updatePaginationBar();
    }
    const cancel = target.task ? apiFetch('/api/task/cancel', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id: target.task }),
    }).catch(() => {}) : Promise.resolve();
    return Promise.allSettled([cancel, closeView(target.view)]);
  }
  function createTab(descriptor) {
    const path = descriptor.path;
    const name = path.split(/[\\/]/).pop() || descriptor.name;
    const browserCopy = descriptor.browserCopy === true;
    return { id: descriptor.id || 'tab_' + crypto.randomUUID(), mode: 'office-preview', source: 'office',
      path, originPath: path, dir: path.replace(/[\\/][^\\/]*$/, ''), name,
      title: browserCopy ? `${name} (${t('app.browserCopy')})` : name,
      browserCopy, content: '', original: null, fixed: '', fixes: [], stats: null,
      isDirty: false, isVirtual: false, size: 0, encoding: '', revision: '', mtime: 0,
      ext: '.' + path.split('.').pop().toLowerCase(), webAssets: [],
      recoveryKey: descriptor.recoveryKey || descriptor.key,
      readerMode: 'paged', readerPage: descriptor.readerPage || 0, scrollPos: descriptor.scrollPos || 0 };
  }
  const current = target => visible === target && !target.controller.signal.aborted &&
    isReaderRenderCurrent(target.render) && state.tabs.includes(target.tab) && getActiveTab() === target.tab;
  function updateControls(target) {
    target.root.setAttribute('aria-busy', String(target.loading));
    if (target.pages) target.image.alt = (target.tab.title || target.tab.name) + ' · ' + t('pagination.pageInfo', { current: target.page + 1, total: target.pages });
    Object.assign(state.pagination, { loading: target.loading, error: !!target.error,
      currentPage: target.page, totalPages: target.pages });
    updatePaginationBar();
  }
  async function loadPage(target, page) {
    if (!current(target) || target.loading || !Number.isSafeInteger(page) || page < 0) return false;
    target.loading = true;
    target.error = false;
    target.image.hidden = true;
    target.status.hidden = false;
    target.status.dataset.i18n = 'audit.loading';
    target.status.textContent = t('audit.loading');
    updateControls(target);
    const task = target.task = crypto.randomUUID();
    try {
      const fromSource = () => ({ path: target.tab.path, page, task_id: task,
        ...(target.source ? { expected_source: target.source } : {}) });
      let response = await post(target.view ? { view: target.view, page, task_id: task } :
        fromSource(), { signal: target.controller.signal });
      // An expired in-memory rendering is safe to reopen from its source once.
      if (response.status === 410 && target.view && current(target)) {
        void closeView(target.view); target.view = null;
        response = await post(fromSource(), { signal: target.controller.signal });
      }
      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw failure(apiMessage(data, 'editor.previewRenderFail'));
      }
      const view = response.headers.get('X-ReadMD-View');
      const source = response.headers.get('X-ReadMD-Source');
      const pagesHeader = response.headers.get('X-ReadMD-Pages'), pageHeader = response.headers.get('X-ReadMD-Page');
      const pages = Number(pagesHeader), actualPage = Number(pageHeader);
      if (!uuid(view) || !/^[0-9a-f]{64}$/.test(source || '') || (target.source && target.source !== source) || !/^\d+$/.test(pagesHeader || '') || !/^\d+$/.test(pageHeader || '') || !Number.isSafeInteger(pages) || pages < 1 ||
          !Number.isSafeInteger(actualPage) || actualPage !== page || actualPage >= pages ||
          !/^image\/png(?:\s*;|$)/i.test(response.headers.get('Content-Type') || '')) {
        if (uuid(view) && view !== target.view) void closeView(view);
        throw failure(t('editor.previewRenderFail'));
      }
      if (!current(target)) { void closeView(view); return false; }
      target.view = view; target.source = source;
      const blob = await response.blob();
      if (!current(target)) { void closeView(view); return false; }
      if (!blob.size) throw failure(t('editor.previewRenderFail'));
      const url = URL.createObjectURL(blob), previousUrl = target.url;
      target.url = url; target.page = actualPage; target.pages = pages;
      if (state.pagination.pages.length !== pages)
        state.pagination.pages = Array.from({ length: pages }, (_, pageIndex) => ({ pageIndex, title: '' }));
      target.tab.readerPage = actualPage;
      target.image.src = url;
      target.image.hidden = false;
      target.status.hidden = true;
      $('content').scrollTop = 0;
      if (previousUrl) URL.revokeObjectURL(previousUrl);
      window.ReadMDSession?.persist();
      return true;
    } catch (error) {
      if (current(target)) {
        target.error = true;
        delete target.status.dataset.i18n;
        target.status.textContent = error.officePreviewMessage ? error.message : t('editor.previewRenderFail');
        target.status.hidden = false;
      }
      return false;
    } finally {
      if (target.task === task) target.task = null;
      target.loading = false;
      if (current(target)) updateControls(target);
    }
  }
  function goToPage(page) {
    const target = visible;
    if (!target || !current(target)) return false;
    if (!Number.isSafeInteger(page) || target.loading) { updateControls(target); return false; }
    page = Math.max(0, Math.min(page, Math.max(0, target.pages - 1)));
    if (page === target.page && target.url && !target.error) { updateControls(target); return true; }
    target.restoreScroll = null;
    return loadPage(target, page);
  }
  async function render(tab) {
    const render = beginReaderRender(), pane = $('content');
    if (!pane || !state.tabs.includes(tab) || getActiveTab() !== tab) return false;
    Object.assign(state.pagination, { kind: 'office', enabled: true, mode: 'paged', rawContent: null, searchText: null,
      pages: [], allHeadings: [], totalPages: 0, currentPage: 0, loading: false, error: false });
    state.headings = [];
    closeSearch(); closeMdPopups();
    window.ReadMDGraph?.close?.();
    $('backlinks-panel')?.classList.add('hidden');
    const root = document.createElement('section'); root.className = 'office-preview'; root.setAttribute('aria-label', tab.title || tab.name);
    const status = document.createElement('p'); status.className = 'office-preview-status'; status.setAttribute('role', 'status');
    const image = document.createElement('img'); image.className = 'office-preview-page'; image.alt = tab.title || tab.name; image.draggable = false; image.hidden = true;
    root.append(status, image); pane.replaceChildren(root); pane.classList.add('is-office-preview'); pane.classList.remove('hidden');
    const target = visible = { tab, render, root, status, image,
      controller: new AbortController(), view: null, task: null, url: null, page: 0, pages: 0, loading: false,
      restoreScroll: tab.scrollPos || 0, preferredPage: Number(tab.readerPage || 0) };
    render.signal.addEventListener('abort', () => { if (visible === target) void releaseView(); }, { once: true });
    image.onerror = () => { if (current(target)) { target.error = true; status.dataset.i18n = 'editor.previewRenderFail'; status.textContent = t('editor.previewRenderFail'); status.hidden = false; updateControls(target); } };
    image.onload = () => {
      const restoredPage = target.preferredPage > 0 && target.preferredPage < target.pages ? target.preferredPage : 0;
      if (current(target) && target.restoreScroll !== null && target.page === restoredPage) {
        pane.scrollTop = target.restoreScroll; target.restoreScroll = null;
      }
    };
    buildToc();
    const preferredPage = Number(tab.readerPage || 0);
    if (!await loadPage(target, 0)) return false;
    if (preferredPage > 0 && preferredPage < target.pages && current(target)) await loadPage(target, preferredPage);
    return current(target);
  }
  async function open(path, options = {}) {
    if (!isPath(path)) return false;
    const ownsLifecycle = () => window.ReadMDLifecycle?.isOwner(options._lifecycleOwner);
    const blocked = () => window.ReadMDSession?.busy?.() || (window.ReadMDLifecycle?.busy?.() && !ownsLifecycle());
    const defer = () => deferSessionOperation(async () => {
      if (!options._sessionRestoring && window.ReadMDSession?.restoring?.()) await window.__readmdSessionReady;
      await window.ReadMDLifecycle?.whenIdle(options._lifecycleOwner);
      if (!await waitForSessionTransition()) return false;
      return open(path, { ...options, _sessionQueued: true });
    }, options._sessionQueued);
    if ((!options._sessionRestoring && window.ReadMDSession?.restoring?.()) || blocked() || (!options._sessionQueued && !ownsLifecycle())) return defer();
    if (window.ReadMDInlineMath && !await ReadMDInlineMath.leave()) return false;
    if (blocked()) return defer();
    const pdfModal = $('pdf-modal');
    if (pdfModal && !pdfModal.classList.contains('hidden')) {
      if (!window.ReadMDPdf?.prepareClose || !await window.ReadMDPdf.prepareClose()) return false;
      if (blocked()) return defer();
    }
    let tab = findTabByPath(path);
    if (tab?._restoreDescriptor || tab?._renamePending || tab?._renameInFlight || tab?.isDirty) { showToast(t('error.review_needs_recovery')); return false; }
    if (tab && !isTab(tab) && tab.mode !== 'unavailable') { showToast(t('toast.noEditableContent')); return false; }
    if (tab?.mode === 'unavailable') {
      Object.assign(tab, createTab(tab)); delete tab._restoreUnavailable;
    }
    if (tab && state.activeTabId !== tab.id) {
      await switchTab(tab.id, { _sessionQueued: options._sessionQueued });
      return getActiveTab() === tab;
    }
    const previous = getActiveTab();
    if (previous && window.ReadMDReview && !await window.ReadMDReview.flush(previous)) return false;
    if (getActiveTab() !== previous) return false;
    if (blocked()) return defer();
    beginDocumentLoad();
    if (state.editing) syncActiveTabDirty();
    if (previous) captureReaderState(previous);
    exitEdit();
    if (tab) { Object.assign(tab, createTab({ ...tab, browserCopy: options.browserCopy ?? tab.browserCopy })); delete tab._restoreUnavailable; }
    else { tab = createTab({ path, browserCopy: options.browserCopy }); state.tabs.push(tab); }
    state.activeTabId = tab.id;
    syncStateFromActiveTab();
    addRecent(path); pushHistory(path); saveLastFile(path); clearAiOutput();
    await renderActiveTab({ restoreScroll: true });
    return state.tabs.includes(tab);
  }
  window.addEventListener('pagehide', () => { void releaseView(); });
  window.addEventListener('readmd:language-changed', () => { if (visible && current(visible)) updateControls(visible); });
  return { isPath, isTab, createTab, open, render, goToPage, releaseView,
    prepareClose: async () => { await releaseView(); return true; } };
})();
