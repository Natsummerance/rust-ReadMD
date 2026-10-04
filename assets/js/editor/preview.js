'use strict';
/* ============================================================
   ReadMD Editor - Live Split Preview & Scroll Sync
   ============================================================ */

/* ---------------- 编辑实时预览（左/右/下/上 + 滚动同步） ---------------- */

let pvTimer = null;
let pvLast = '';
let pvEditorEl = null;
let pvRenderEpoch = 0;
let editorOpenEpoch = 0;

function hasUnsavedEditorChanges() {
  if (!state.editing) return false;
  return getEditContent() !== (state.original || '') || Boolean(getActiveTab()?.isVirtual && getActiveTab()?.unsavedCreation);
}

function syncSavedTab(path, content) {
  const tab = typeof findTabByPath === 'function' ? findTabByPath(path) : null;
  if (!tab) return;
  tab.content = content;
  tab.original = content;
  tab.fixed = content;
  tab.fixes = [];
  tab.isDirty = false;
  tab.externalChanged = false;
}

function applySavedMtime(result) {
  if (result && typeof result.mtime === 'number') {
    state.mtime = result.mtime;
    const tab = typeof getActiveTab === 'function' ? getActiveTab() : null;
    if (tab) tab.mtime = result.mtime;
  }
}

async function renderSavedDocument(content) {
  state.original = content;
  state.fixed = content;
  state.fixes = [];
  state.stats = {};
  if (typeof setFixes === 'function') setFixes([], {});
  if (typeof renderContent === 'function') {
    const contentEl = document.getElementById('content');
    const page = state.pagination?.enabled && state.pagination.mode === 'paged' ? state.pagination.currentPage : 0;
    const scroll = contentEl?.scrollTop || 0;
    await renderContent(content, state.sourceName || (state.file ? state.file.split(/[\\/]/).pop() : 'document'));
    if (page > 0) await renderPage(page, null, true);
    requestAnimationFrame(() => {
      if (contentEl) contentEl.scrollTop = scroll;
    });
  }
}

function getEditContent() {
  const text = cmView ? cmView.state.doc.toString() : ($('edit-area') && $('edit-area').value || '');
  const original = state.original ?? '';
  // Both editors normalize line endings; opening a file is not a change.
  if (text === original.replace(/\r\n?/g, '\n')) return original;
  if (original.includes('\r\n') && !/(^|[^\r])\n/.test(original)) return text.replace(/\r?\n/g, '\r\n');
  if (original.includes('\r') && !original.includes('\n')) return text.replace(/\r?\n/g, '\r');
  return text;
}

function setPvLayout(layout) {
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  if (['none', 'left', 'right', 'bottom', 'top'].indexOf(layout) < 0) layout = 'none';
  if (layout === 'none' && typeof switchEditAiToChatPanel === 'function') {
    switchEditAiToChatPanel();
  }
  state.pvLayout = layout;
  document.querySelectorAll('.pv-btn').forEach(b => b.classList.toggle('active', b.dataset.pv === layout));
  const names = {
    none: _t('editor.previewNone') || '无',
    left: _t('editor.previewLeft') || '左',
    right: _t('editor.previewRight') || '右',
    bottom: _t('editor.previewBottom') || '下',
    top: _t('editor.previewTop') || '上'
  };
  const narrow = window.innerWidth < 600 && (layout === 'left' || layout === 'right');
  const previewLabel = _t('editor.preview') || '预览';
  const trigger = $('pv-trigger');
  if (trigger) {
    // Short visible label; the full state (incl. narrow-screen note) is in title/aria-label.
    const full = narrow
      ? previewLabel + '：' + names[layout] + '（' + (_t('editor.narrowScreenBottom') || '窄屏置底') + '）'
      : previewLabel + '：' + names[layout];
    trigger.innerHTML = '<svg class="tb-ic" viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M12 4v16"/></svg><span class="pv-trigger-label"></span><svg class="md-caret" viewBox="0 0 24 24" aria-hidden="true"><path d="m7 10 5 5 5-5"/></svg>';
    trigger.querySelector('.pv-trigger-label').textContent = full;
    trigger.title = full;
    trigger.setAttribute('aria-label', full);
    trigger.classList.toggle('is-on', layout !== 'none');
  }
  const mc = $('main-col');

  const pw = $('preview-wrap');
  if (!mc || !pw) return;
  mc.classList.remove('pv-left', 'pv-right', 'pv-bottom', 'pv-top');
  mc.classList.remove('pv-auto-hidden');
  if (state.editing && layout !== 'none') {
    // 预览界面和AI对话界面不能同时出现：若开启预览，自动收起 AI 对话面板
    const aiPanel = $('ai-panel');
    if (aiPanel && !aiPanel.classList.contains('hidden')) {
      aiPanel.classList.add('hidden');
    }
    mc.classList.add('pv-' + layout);
    const bounds = mc.getBoundingClientRect();
    const horizontal = layout === 'left' || layout === 'right';
    // Decide on window width: main-col width is circular here (it shrinks when the
    // preview pane is already flexed), while the specs pin the boundary at 720/760.
    const constrained = horizontal ? window.innerWidth < 740 : bounds.height < 520;
    mc.classList.toggle('pv-auto-hidden', constrained);
    pw.classList.toggle('hidden', constrained);
    $('pv-splitter').classList.toggle('hidden', constrained);
    if (!constrained) {
      applyPvSplit();
      schedulePreview();
    }
  } else {
    pw.classList.add('hidden');
    $('pv-splitter').classList.add('hidden');
  }
  // AI 写入时临时收起预览（_pvLayoutBeforeAi），不能覆盖用户记住的布局。
  if (!state._pvLayoutBeforeAi) saveSettings();
}

/* 每个文档记住上次的光标与滚动位置，再次进入编辑时恢复。 */
function editMemoryKey() {
  const id = state.file || state.sourceName || '';
  return id ? 'readmd_edit_pos:' + id : '';
}

function rememberEditPosition() {
  const key = editMemoryKey();
  if (!key || !cmView) return;
  try {
    localStorage.setItem(key, JSON.stringify({ a: cmView.state.selection.main.head, t: cmView.scrollDOM.scrollTop }));
  } catch (e) { /* storage full or disabled */ }
}

/* 新建空白文档并直接进入编辑（欢迎页按钮、Ctrl+N、命令面板共用）。 */
async function newDocument() {
  if (state.editing && !await confirmExitEdit()) return;
  await renderVirtual('', '', '', '', []);
  await toggleEdit();
}

function restoreEditPosition() {
  const key = editMemoryKey();
  if (!key || !cmView) return;
  try {
    const m = JSON.parse(localStorage.getItem(key) || 'null');
    if (!m) return;
    const anchor = Math.max(0, Math.min(Number(m.a) || 0, cmView.state.doc.length));
    cmView.dispatch({ selection: { anchor } });
    requestAnimationFrame(() => { if (cmView) cmView.scrollDOM.scrollTop = Number(m.t) || 0; });
  } catch (e) { /* ignore corrupt entry */ }
}

function applyPvSplit() {
  const pw = $('preview-wrap'); if (!pw) return;
  const horizontal = state.pvLayout === 'left' || state.pvLayout === 'right';
  const key = horizontal ? 'pvSplitX' : 'pvSplitY';
  const raw = Number(state[key]);
  const pct = Math.max(25, Math.min(70, Number.isFinite(raw) ? raw : (horizontal ? 50 : 46)));
  state[key] = pct;
  pw.style.flexBasis = pct + '%';
  const splitter = $('pv-splitter');
  if (splitter) {
    splitter.setAttribute('aria-valuemin', '25');
    splitter.setAttribute('aria-valuemax', '70');
    splitter.setAttribute('aria-valuenow', String(Math.round(pct)));
    splitter.setAttribute('aria-valuetext', `${Math.round(pct)}%`);
    splitter.setAttribute('aria-orientation', horizontal ? 'vertical' : 'horizontal');
  }
}

function bindPvSplitter() {
  const bar = $('pv-splitter'); const mc = $('main-col'); if (!bar || !mc) return;
  const update = e => {
    const r = mc.getBoundingClientRect(); let pct;
    if (state.pvLayout === 'left') pct = (e.clientX - r.left) / r.width * 100;
    else if (state.pvLayout === 'right') pct = (r.right - e.clientX) / r.width * 100;
    else if (state.pvLayout === 'top') pct = (e.clientY - r.top) / r.height * 100;
    else pct = (r.bottom - e.clientY) / r.height * 100;
    pct = Math.max(25, Math.min(70, pct));
    if (state.pvLayout === 'left' || state.pvLayout === 'right') state.pvSplitX = pct; else state.pvSplitY = pct;
    applyPvSplit();
  };
  bar.addEventListener('pointerdown', e => { bar.setPointerCapture(e.pointerId); update(e); });
  bar.addEventListener('pointermove', e => { if (bar.hasPointerCapture(e.pointerId)) update(e); });
  bar.addEventListener('pointerup', e => { if (bar.hasPointerCapture(e.pointerId)) bar.releasePointerCapture(e.pointerId); saveSettings(); });
  bar.addEventListener('keydown', e => { if (!['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(e.key)) return; e.preventDefault(); const delta = (e.key === 'ArrowRight' || e.key === 'ArrowDown') ? 2 : -2; if (state.pvLayout === 'left' || state.pvLayout === 'right') state.pvSplitX = Math.max(25, Math.min(70, state.pvSplitX + delta)); else state.pvSplitY = Math.max(25, Math.min(70, state.pvSplitY + delta)); applyPvSplit(); saveSettings(); });
  window.addEventListener('resize', () => requestAnimationFrame(() => setPvLayout(state.pvLayout)));
}

/* Which pane the user is driving: scroll events from the other pane are
   echoes of our own programmatic scrolling and must not bounce back. */
let pvScrollDriver = null;
let pvScrollDriverTimer = null;
let pvSyncFrame = 0;
function pvClaimScroll(who) {
  pvScrollDriver = who;
  if (pvScrollDriverTimer) clearTimeout(pvScrollDriverTimer);
  pvScrollDriverTimer = setTimeout(() => { pvScrollDriver = null; }, 160);
}
// Kept for callers that read these flags.
let isSyncingFromEditor = false;
let isSyncingFromPreview = false;

/* Preview refresh: the first keystroke after a pause renders on the next
   frame (instant feel), a burst of typing is coalesced, and the delay grows
   with document size so huge files stay responsive. */
let pvLastRenderAt = 0;
function schedulePreview() {
  if (pvTimer) clearTimeout(pvTimer);
  if (state.liveUpdate === false) return; // Save-only mode
  if (state.pvLayout === 'none' || !state.editing) return;
  const size = getEditContent().length;
  const settle = size >= 100000 ? 600 : size >= 30000 ? 320 : 140;
  const idle = Date.now() - pvLastRenderAt > settle * 2;
  pvTimer = setTimeout(renderPreview, idle ? 16 : settle);
}

/* Replace only the top-level blocks whose HTML changed, so MathJax output,
   diagrams and images elsewhere in the pane are not re-rendered. */
function pvPatchPane(pane, html) {
  const tpl = document.createElement('template');
  tpl.innerHTML = html;
  const next = Array.from(tpl.content.childNodes).filter(n => n.nodeType === 1 || (n.nodeType === 3 && n.textContent.trim()));
  const prev = Array.from(pane.childNodes);
  if (!prev.length || !pane.__pvSource || Math.abs(prev.length - next.length) > 400) {
    pane.innerHTML = '';
    next.forEach((n, i) => { if (n.nodeType === 1) n.__pvSource = n.outerHTML; pane.appendChild(n); });
    pane.__pvSource = true;
    return next.filter(n => n.nodeType === 1);
  }
  const key = n => n.nodeType === 1 ? (n.__pvSource || n.outerHTML).replace(/\sdata-source-line="\d+"/, '') : n.textContent;
  const prevKeys = prev.map(key);
  const nextKeys = next.map(n => n.nodeType === 1 ? n.outerHTML.replace(/\sdata-source-line="\d+"/, '') : n.textContent);
  let head = 0;
  while (head < prev.length && head < next.length && prevKeys[head] === nextKeys[head]) head++;
  let tail = 0;
  while (tail < prev.length - head && tail < next.length - head && prevKeys[prev.length - 1 - tail] === nextKeys[next.length - 1 - tail]) tail++;
  // Unchanged blocks keep their node, only their source line is refreshed.
  const syncLine = (oldNode, newNode) => {
    if (oldNode.nodeType !== 1) return;
    const l = newNode.getAttribute('data-source-line');
    if (l) oldNode.setAttribute('data-source-line', l);
  };
  for (let i = 0; i < head; i++) syncLine(prev[i], next[i]);
  for (let i = 0; i < tail; i++) syncLine(prev[prev.length - 1 - i], next[next.length - 1 - i]);
  const anchor = tail ? prev[prev.length - tail] : null;
  for (let i = head; i < prev.length - tail; i++) prev[i].remove();
  const fresh = [];
  for (let i = head; i < next.length - tail; i++) {
    const n = next[i];
    if (n.nodeType === 1) { n.__pvSource = n.outerHTML; fresh.push(n); }
    pane.insertBefore(n, anchor);
  }
  return fresh;
}

async function renderPreview() {
  pvTimer = null;
  const renderEpoch = ++pvRenderEpoch;
  const pane = $('preview-pane');
  if (!pane || state.pvLayout === 'none' || !state.editing) return;
  let src = getEditContent();
  if (src === pvLast) return;
  pvLast = src;

  // 预处理 @import
  if (window.processDocImports) {
    src = await window.processDocImports(src, state.file || '');
    if (renderEpoch !== pvRenderEpoch) return;
  }

  let html;
  try {
    const transformed = window.transformAcademicCallouts ? transformAcademicCallouts(src) : src;
    const prot = protectMath(transformed);
    if (window.parseMarkdownWithSourceMap) {
      html = restoreMath(parseMarkdownWithSourceMap(prot.src), prot.saved);
    } else {
      html = restoreMath(marked.parse(prot.src, { gfm: true, breaks: !!(state && state.breakOnSingleNewline) }), prot.saved);
    }
  } catch (e) {
    const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
    html = '<p class="ai-err">' + (_t('editor.previewRenderFail') || '预览渲染失败') + '</p>';
  }
  if (renderEpoch !== pvRenderEpoch) return;
  pvLastRenderAt = Date.now();
  const safe = window.sanitizeRenderedHtml ? window.sanitizeRenderedHtml(html) : html;
  const fresh = pvPatchPane(pane, safe);
  if (!fresh.length) return;
  // Post-process only the blocks that changed: fixLinks binds click handlers,
  // so it sees just the fresh links while heading lookup spans the whole pane.
  const freshLinks = fresh.flatMap(n => (n.tagName === 'A' ? [n] : Array.from(n.querySelectorAll('a'))));
  fixLinks({ querySelectorAll: sel => (sel === 'a' ? freshLinks : pane.querySelectorAll(sel)) });
  fixImages(pane);
  fresh.forEach(n => renderMath(n));
  if (window.renderAllCodeChunks) renderAllCodeChunks(pane);
  if (window.renderAllDiagrams) renderAllDiagrams(pane);
  if (state.pvSync) pvSyncFromEditor();
}

function getEditorVisibleLine() {
  const pos = pvEditorTopPosition();
  return pos ? Math.max(1, Math.floor(pos.line)) : 1;
}

/* Fractional source line at the top edge of the editor viewport. */
function pvEditorTopPosition() {
  if (cmView && cmView.lineBlockAtHeight) {
    try {
      const top = cmView.scrollDOM.scrollTop;
      // Heights inside lineBlockAtHeight are relative to the document top.
      const docTop = cmView.documentTop - cmView.scrollDOM.getBoundingClientRect().top + top;
      const y = Math.max(0, top - docTop);
      const block = cmView.lineBlockAtHeight(y);
      const line = cmView.state.doc.lineAt(block.from).number;
      const frac = block.height > 0 ? Math.min(1, Math.max(0, (y - block.top) / block.height)) : 0;
      return { line: line + frac, max: cmView.scrollDOM.scrollHeight - cmView.scrollDOM.clientHeight, top };
    } catch (e) {
      return null;
    }
  }
  const ta = $('edit-area');
  if (ta) {
    const totalLines = ta.value.split('\n').length;
    const pct = ta.scrollTop / Math.max(1, ta.scrollHeight - ta.clientHeight);
    return { line: 1 + pct * (totalLines - 1), max: ta.scrollHeight - ta.clientHeight, top: ta.scrollTop };
  }
  return null;
}

/* [{line, top}] for every preview element carrying a source line, sorted. */
function pvAnchors(wrap, pane) {
  const base = wrap.getBoundingClientRect().top - wrap.scrollTop;
  const out = [];
  pane.querySelectorAll('[data-source-line]').forEach(el => {
    const line = parseInt(el.dataset.sourceLine, 10);
    if (!line || !el.getClientRects().length) return;
    const top = el.getBoundingClientRect().top - base;
    if (out.length && (line <= out[out.length - 1].line || top < out[out.length - 1].top)) return;
    out.push({ line, top });
  });
  return out;
}

function pvSyncFromEditor() {
  if (!state.pvSync || state.pvLayout === 'none' || pvScrollDriver === 'preview') return;
  if (pvSyncFrame) return;
  pvSyncFrame = requestAnimationFrame(() => {
    pvSyncFrame = 0;
    pvClaimScroll('editor');
    const dst = $('preview-wrap');
    const pane = $('preview-pane');
    const pos = pvEditorTopPosition();
    if (!dst || !pane || !pos) return;
    const maxDst = dst.scrollHeight - dst.clientHeight;
    if (maxDst <= 0) return;
    if (pos.top <= 1) { dst.scrollTop = 0; return; }
    if (pos.max > 0 && pos.top >= pos.max - 1) { dst.scrollTop = maxDst; return; }
    const anchors = pvAnchors(dst, pane);
    if (!anchors.length) {
      if (pos.max > 0) dst.scrollTop = (pos.top / pos.max) * maxDst;
      return;
    }
    let i = 0;
    while (i + 1 < anchors.length && anchors[i + 1].line <= pos.line) i++;
    const a = anchors[i];
    const b = anchors[i + 1];
    let y;
    if (pos.line < a.line) y = a.top * (pos.line - 1) / Math.max(1, a.line - 1);
    else if (b) y = a.top + (b.top - a.top) * (pos.line - a.line) / Math.max(1e-6, b.line - a.line);
    else {
      const lines = cmView ? cmView.state.doc.lines : a.line + 1;
      y = a.top + (dst.scrollHeight - a.top) * (pos.line - a.line) / Math.max(1, lines + 1 - a.line);
    }
    dst.scrollTop = Math.max(0, Math.min(maxDst, y - 12));
  });
}

function pvSyncFromPreview() {
  if (!state.pvSync || state.pvLayout === 'none' || pvScrollDriver === 'editor') return;
  if (pvSyncFrame) return;
  pvSyncFrame = requestAnimationFrame(() => {
    pvSyncFrame = 0;
    pvClaimScroll('preview');
    const src = $('preview-wrap');
    const pane = $('preview-pane');
    if (!src || !pane) return;
    const maxSrc = src.scrollHeight - src.clientHeight;
    const y = src.scrollTop + 12;
    const scroller = cmView ? cmView.scrollDOM : $('edit-area');
    if (!scroller) return;
    const maxDst = scroller.scrollHeight - scroller.clientHeight;
    if (src.scrollTop <= 1) { scroller.scrollTop = 0; return; }
    if (maxSrc > 0 && src.scrollTop >= maxSrc - 1) { scroller.scrollTop = maxDst; return; }
    const anchors = pvAnchors(src, pane);
    if (!anchors.length || !cmView) {
      if (maxSrc > 0) scroller.scrollTop = (src.scrollTop / maxSrc) * maxDst;
      return;
    }
    let i = 0;
    while (i + 1 < anchors.length && anchors[i + 1].top <= y) i++;
    const a = anchors[i], b = anchors[i + 1];
    let line;
    if (y < a.top) line = 1 + (a.line - 1) * (y / Math.max(1, a.top));
    else if (b) line = a.line + (b.line - a.line) * (y - a.top) / Math.max(1, b.top - a.top);
    else line = a.line + (y - a.top) / Math.max(1, src.scrollHeight - a.top) * (cmView.state.doc.lines + 1 - a.line);
    const doc = cmView.state.doc;
    const n = Math.min(doc.lines, Math.max(1, Math.floor(line)));
    try {
      const block = cmView.lineBlockAt(doc.line(n).from);
      const docTop = cmView.documentTop - scroller.getBoundingClientRect().top + scroller.scrollTop;
      const target = docTop + block.top + block.height * Math.min(1, line - n);
      scroller.scrollTop = Math.max(0, Math.min(maxDst, target));
    } catch (e) { /* ignore */ }
  });
}

function alignEditorAndPreview() {
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  pvSyncFromEditor();
  showToast(_t('editor.previewAligned'), 1200);
}
window.alignEditorAndPreview = alignEditorAndPreview;

function applyPvUi() {
  document.querySelectorAll('.pv-btn').forEach(b => b.classList.toggle('active', b.dataset.pv === state.pvLayout));
  const sync = $('pv-sync');
  if (sync) sync.checked = !!state.pvSync;
  setPvLayout(state.pvLayout);
}

async function toggleEdit() {
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  if (state.editing) {
    if (!await confirmExitEdit()) return;
    applyPvUi();
    return;
  }
  // 没有打开文档时不能编辑（新建文档请用 Ctrl+N / 欢迎页“新建”）；空文件可以编辑。
  if (state.original == null || (state.mode === 'welcome' && !state.file)) { showToast(_t('toast.noEditableContent') || '没有可编辑的内容'); return; }
  const epoch = ++editorOpenEpoch;
  const tabId = state.activeTabId;
  const activeTab = typeof getActiveTab === 'function' ? getActiveTab() : null;
  const editorContent = activeTab?.isDirty ? (activeTab.content ?? state.original ?? '') : (state.original ?? '');
  // The fallback is also the draft snapshot while CodeMirror is loading.
  $('edit-area').value = editorContent;
  $('edit-bar').classList.remove('hidden');
  $('content').classList.add('hidden');
  state.editing = true;
  $('main-col')?.classList.add('is-editing');
  setEditBtn(_t('editor.editing') || '编辑中');
  pvLast = '';
  try {
    await loadCodeMirror();
  } catch (e) { /* 退回 textarea */ }
  if (epoch !== editorOpenEpoch || !state.editing || state.activeTabId !== tabId) return;
  let cmMounted = false;
  if (window.ReadMDCodeMirror) {
    $('edit-area').classList.add('hidden');
    $('edit-wrap').classList.remove('hidden');
    // 旧版或损坏的 CodeMirror 包会让 createEditor 抛错：退回 textarea，避免卡在空白编辑页。
    try { createEditor(editorContent); cmMounted = !!cmView; } catch (e) {
      console.error(e);
      try { destroyEditor(); } catch (_) { /* ignore */ }
    }
  }
  if (cmMounted) {
    pvEditorEl = cmView.scrollDOM;
    pvEditorEl.addEventListener('scroll', pvSyncFromEditor);
    restoreEditPosition();
    cmView.focus();
  } else {
    $('edit-wrap').classList.add('hidden');
    $('edit-area').classList.remove('hidden');
    $('edit-area').value = editorContent;
    pvEditorEl = $('edit-area');
    pvEditorEl.addEventListener('scroll', pvSyncFromEditor);
    $('edit-area').focus();
  }
  applyPvUi();
}

async function confirmExitEdit() {
  if (!hasUnsavedEditorChanges()) {
    exitEdit();
    return true;
  }
  const action = await promptDirtyClose(state.sourceName || state.file || 'document');
  if (action === 'cancel') return false;
  if (action === 'save') {
    await saveEdit({ exitAfterSave: true });
    return !state.editing;
  }
  const activeTab = typeof getActiveTab === 'function' ? getActiveTab() : null;
  if (window.ReadMDRecovery && !await window.ReadMDRecovery.discard(activeTab)) return false;
  if (activeTab) {
    activeTab.content = state.original;
    activeTab.fixed = state.original;
    activeTab.isDirty = false;
    if (typeof renderTabsBar === 'function') renderTabsBar();
  }
  exitEdit();
  return true;
}

function exitEdit() {
  ++editorOpenEpoch;
  if (state.editing) rememberEditPosition();
  if (typeof switchEditAiToChatPanel === 'function') switchEditAiToChatPanel();
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  if (pvTimer) { clearTimeout(pvTimer); pvTimer = null; }
  if (pvEditorEl) {
    pvEditorEl.removeEventListener('scroll', pvSyncFromEditor);
    pvEditorEl = null;
  }
  const pw = $('preview-wrap');
  if (pw) pw.classList.add('hidden');
  const mc = $('main-col');
  if (mc) mc.classList.remove('pv-left', 'pv-right', 'pv-bottom', 'pv-top', 'pv-auto-hidden', 'is-editing');
  pvLast = '';
  if (!state.editing) {
    $('edit-bar').classList.add('hidden');
    $('edit-area').classList.add('hidden');
    $('edit-wrap').classList.add('hidden');
    $('content').classList.remove('hidden');
    setEditBtn(_t('toolbar.edit') || '编辑');
    return;
  }
  const tab = getActiveTab();
  if (tab && cmView) { tab.editorState = cmView.state; tab.editorCompartments = cmCompartments; }
  destroyEditor();
  $('edit-bar').classList.add('hidden');
  $('edit-area').classList.add('hidden');
  $('edit-wrap').classList.add('hidden');
  $('content').classList.remove('hidden');
  state.editing = false;
  setEditBtn(_t('toolbar.edit') || '编辑');
  if (typeof updateUnloadGuard === 'function') updateUnloadGuard();
}

function documentSaveSnapshot(contentOverride = null) {
  const tab = getActiveTab();
  return { tab, id: tab?.id, started: Date.now(), path: tab?.path || state.file, content: contentOverride ?? (state.editing ? getEditContent() : (tab?.content ?? state.fixed ?? state.original ?? '')),
    original: tab?.original ?? state.original ?? '', name: tab?.name || state.sourceName || 'document.md',
    encoding: tab?.encoding || state.encoding || 'utf-8', mtime: tab?.mtime || state.mtime || null,
    revision: tab?.revision || '', assets: tab?.webAssets || state.webAssets || [], recoveryKey: tab?.recoveryKey || tab?.path || ('draft:' + tab?.id) };
}

async function commitDocumentSave(snapshot, path, result, options = {}) {
  const tab = snapshot.tab;
  if (!tab || !state.tabs.includes(tab)) return true;
  const active = getActiveTab() === tab;
  let draft = active && state.editing ? getEditContent() : (tab.content ?? snapshot.content);
  const unchanged = draft === snapshot.content;
  const savedContent = typeof result.saved_content === 'string' ? result.saved_content : snapshot.content;
  if (unchanged && savedContent !== draft) {
    draft = savedContent;
    if (active && state.editing) {
      if (cmView) cmView.dispatch({ changes: { from: 0, to: cmView.state.doc.length, insert: draft }, annotations: window.ReadMDCodeMirror.Transaction.userEvent.of('save.assets') });
      else $('edit-area').value = draft;
    }
  }
  tab.path = path; tab.dir = String(path).replace(/[\\/][^\\/]*$/, '');
  tab.mode = 'file'; tab.isVirtual = false;
  tab.name = String(path).split(/[\\/]/).pop(); tab.title = tab.browserCopy && !options.retarget ? tab.title : tab.name;
  if (options.retarget) tab.browserCopy = false;
  tab.original = savedContent; tab.content = draft; tab.fixed = draft; tab.fixes = [];
  if (Array.isArray(result.saved_assets)) tab.webAssets = unchanged ? result.saved_assets : [...(result.source_assets || snapshot.assets), ...result.saved_assets];
  tab.isDirty = !unchanged; tab.mtime = result.mtime || 0; tab.revision = result.revision || ''; tab.encoding = snapshot.encoding;
  if (active) {
    state.file = path; state.dir = tab.dir; state.mode = 'file'; state.browserCopy = tab.browserCopy;
    state.sourceName = tab.name; state.original = savedContent; state.fixed = draft;
    state.mtime = tab.mtime; state.revision = tab.revision; state.encoding = tab.encoding;
    state.webAssets = tab.webAssets || [];
    if (options.retarget) { document.title = tab.name + ' - ReadMD'; setFileTitle(tab.name, true, path); addRecent(path); }
    if (options.exitAfterSave && unchanged) { exitEdit(); await renderActiveTab({ restoreScroll: true }); }
    else await renderActiveTab({ restoreScroll: true });
  }
  renderTabsBar(); updateUnloadGuard(); updateStatus();
  await window.ReadMDRecovery?.saved(snapshot, tab);
  showToast(window.i18n.t(unchanged ? 'storage.saved' : 'storage.savedEarlier'));
  return !options.exitAfterSave || (active && unchanged);
}

async function saveEdit(options = {}) {
  if (!state.editing) return false;
  return window.ReadMDTask.run('document-save', () => saveEditOnce(options), { trigger: ['edit-save', 'btn-saveas'] });
}

async function saveEditOnce(options = {}) {
  const snapshot = documentSaveSnapshot();
  if (!snapshot.path) return saveAsSnapshot(snapshot, options);
  const write = async () => {
    // Use one acknowledgement shape for native and browser editing. The API is the Rust writer in both.
    const response = await apiFetch('/api/save', { method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: snapshot.path, content: snapshot.content, encoding: snapshot.encoding,
        expected_mtime: snapshot.mtime, expected_revision: snapshot.revision || null }) });
    const result = await response.json();
    if (!response.ok && !result.conflict && result.error_code !== 'encoding_unrepresentable') throw new Error(result.error || 'HTTP ' + response.status);
    return result;
  };
  try {
    let result = await write();
    if (result.error_code === 'encoding_unrepresentable') {
      if (!await confirmAction({ title: window.i18n.t('dialog.encodingTitle'), message: window.i18n.t('dialog.encodingMessage', { encoding: snapshot.encoding, char: result.char || '' }),
        confirmText: window.i18n.t('dialog.encodingUseUtf8'), cancelText: window.i18n.t('common.cancel') })) return false;
      snapshot.encoding = 'utf-8'; result = await write();
    }
    if (result.ok === true) return commitDocumentSave(snapshot, snapshot.path, result, options);
    if (result.conflict) {
      if (getActiveTab() !== snapshot.tab) { showToast(window.i18n.t('storage.conflictBackground')); return false; }
      const action = await promptSaveConflict();
      if (action === 'save-as') return saveAsSnapshot(snapshot, options);
      if (action === 'reload' && getActiveTab() === snapshot.tab) {
        // Preserve the draft before an explicit destructive reload.
        const view = cmView, latest = getEditContent();
        if (!await window.ReadMDRecovery?.checkpoint('conflict_reload', snapshot.tab)) return false;
        if (getActiveTab() !== snapshot.tab || !state.editing || cmView !== view || getEditContent() !== latest) return false;
        exitEdit(); await loadFile(snapshot.path, { force: true, discardConfirmed: true }); return !snapshot.tab.isDirty;
      }
      return false;
    }
    throw new Error(result.error || window.i18n.t('audit.invalidResponse'));
  } catch (error) { showToast(window.i18n.t('toast.saveFailed') + error.message); return false; }
}

function promptSaveConflict() {
  return new Promise(resolve => {
    const modal = $('save-conflict-modal');
    if (!modal) {
      resolve('cancel');
      return;
    }
    let opener = document.activeElement;
    if (!(opener instanceof HTMLElement) || !opener.isConnected || opener === document.body) {
      opener = $('edit-save');
    }
    modal.classList.remove('hidden');
    const reload = $('save-conflict-reload');
    const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
    reload.textContent = `${_t('toolbar.reload') || 'Reload'} (${_t('dialog.dontSave') || 'Do not save'})`;
    const cancel = $('save-conflict-cancel');
    setTimeout(() => cancel?.focus(), 20);

    const finish = action => {
      modal.classList.add('hidden');
      $('save-conflict-save-as').onclick = null;
      $('save-conflict-reload').onclick = null;
      $('save-conflict-cancel').onclick = null;
      modal.removeEventListener('click', onBackdrop);
      document.removeEventListener('keydown', onKey);
      resolve(action);
      setTimeout(() => {
        if (opener instanceof HTMLElement && opener.isConnected) opener.focus({ preventScroll: true });
      }, 60);
    };
    const onBackdrop = event => { if (event.target === modal) finish('cancel'); };
    const onKey = event => {
      if (event.key === 'Escape') { event.preventDefault(); finish('cancel'); }
    };
    $('save-conflict-save-as').onclick = () => finish('save-as');
    $('save-conflict-reload').onclick = () => finish('reload');
    cancel.onclick = () => finish('cancel');
    modal.addEventListener('click', onBackdrop);
    document.addEventListener('keydown', onKey);
  });
}

async function saveAs(contentOverride = null) {
  // DOM event listeners pass a MouseEvent, which is not document content.
  const snapshot = documentSaveSnapshot(typeof contentOverride === 'string' ? contentOverride : null);
  return window.ReadMDTask.run('document-save', () => saveAsSnapshot(snapshot, { retarget: true }), { trigger: ['edit-save', 'btn-saveas'] });
}

async function saveAsSnapshot(snapshot, options = {}) {
  const suggested = String(snapshot.name || 'document.md').replace(/[\\/]/g, '_');
  if (!hasPy) {
    const blob = new Blob([snapshot.content], { type: 'text/markdown;charset=utf-8' });
    const a = document.createElement('a'); a.href = URL.createObjectURL(blob); a.download = suggested;
    a.click(); setTimeout(() => URL.revokeObjectURL(a.href), 3000);
    await window.ReadMDRecovery?.flush();
    showToast(window.i18n.t('storage.downloadDraft'));
    // A download request is not proof that a file was written. Keep the dirty document and its recovery copy.
    return !options.exitAfterSave;
  }
  try {
    const outcome = await py.save_as(snapshot.content, suggested, snapshot.assets, { result: true,
      dir: snapshot.tab?.dir || '', base_dir: snapshot.tab?.dir || '', encoding: snapshot.path ? snapshot.encoding : 'utf-8',
      blocked_paths: state.tabs.filter(tab => tab !== snapshot.tab && tab.path).map(tab => tab.path) });
    if (!outcome || outcome.canceled) return false;
    const result = typeof outcome === 'string' ? { ok: true, path: outcome } : outcome;
    if (result.ok !== true || !result.path) throw new Error(result.error || window.i18n.t('audit.invalidResponse'));
    if (!result.mtime) {
      const response = await apiFetch('/api/file?p=' + encodeURIComponent(result.path) + '&meta=1');
      if (response.ok) Object.assign(result, await response.json(), { path: result.path });
    }
    if (!snapshot.path) snapshot.encoding = 'utf-8';
    const committed = await commitDocumentSave(snapshot, result.path, result, { ...options, retarget: true });
    if (result.warns?.length) showToast(result.warns.join('\n'), 6000);
    return committed;
  } catch (error) {
    if (error.details?.error_code === 'encoding_unrepresentable' && snapshot.encoding !== 'utf-8') {
      if (await confirmAction({ title: window.i18n.t('dialog.encodingTitle'), message: window.i18n.t('dialog.encodingMessage', { encoding: snapshot.encoding, char: error.details.char || '' }),
        confirmText: window.i18n.t('dialog.encodingUseUtf8'), cancelText: window.i18n.t('common.cancel') })) {
        snapshot.encoding = 'utf-8'; return saveAsSnapshot(snapshot, options);
      }
      return false;
    }
    const message = error.message === 'target_open_in_another_tab' ? window.i18n.t('storage.targetOpen') : error.message;
    showToast(window.i18n.t('toast.saveFailed') + message); return false;
  }
}

// Older callers use this name. AI copies now use the same explicit Save As workflow as any unsaved document.
async function autoSaveAiCopyTab(tab, options = {}) {
  if (getActiveTab() !== tab) return false;
  return saveAsSnapshot(documentSaveSnapshot(options.content ?? null), options);
}
window.autoSaveAiCopyTab = autoSaveAiCopyTab;
