'use strict';
/* ============================================================
   ReadMD Reader - In-Document Search & Highlighting
   ============================================================ */

/* ---------------- 搜索 ---------------- */

let globalSearchState = {
  query: '',
  matches: [],       // [{ pageIndex, matchIdxInPage }]
  globalIndex: 0,
};
let enterAdvancePending = false;
let editorFindState = { query: '', doc: null, matches: [], index: 0, capped: false };

function searchEditor(query, { jump = true, index } = {}) {
  const view = window.cmView;
  const fallback = $('edit-area');
  if (!state.editing || (!view && !fallback)) return false;
  const text = view ? view.state.doc : fallback.value;
  if (editorFindState.doc !== text || editorFindState.query !== query) {
    const matches = [];
    if (query) {
      const re = new RegExp(query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'giu');
      const source = text.toString();
      let match;
      while ((match = re.exec(source)) && matches.length < 10000) matches.push({ from: match.index, to: match.index + match[0].length });
      editorFindState.capped = !!match;
    } else editorFindState.capped = false;
    const cursor = view ? view.state.selection.main.from : fallback.selectionStart;
    editorFindState = { query, doc: text, matches, index: Math.max(0, matches.findIndex(match => match.from >= cursor)), capped: editorFindState.capped };
  }
  if (index !== undefined && editorFindState.matches.length) editorFindState.index = (index + editorFindState.matches.length) % editorFindState.matches.length;
  globalSearchState.query = query;
  state.lastQuery = query;
  const match = editorFindState.matches[editorFindState.index];
  if (jump && match) {
    if (view) view.dispatch({ selection: { anchor: match.from, head: match.to }, effects: window.ReadMDCodeMirror.EditorView.scrollIntoView(match.from, { y: 'center' }) });
    else { fallback.focus(); fallback.setSelectionRange(match.from, match.to); }
  }
  updateSearchCount();
  return true;
}

function replaceEditorMatch(all = false) {
  if (!state.editing) return;
  const query = $('search-input').value;
  searchEditor(query, { jump: false });
  const found = editorFindState;
  if (!found.matches.length) return;
  if (all && found.capped) {
    showToast(window.i18n.t('search.refineBeforeReplace'));
    return;
  }
  const replacement = $('search-replace-input').value;
  const matches = all ? found.matches : [found.matches[found.index]];
  const view = window.cmView;
  if (view) view.dispatch({ changes: matches.map(match => ({ from: match.from, to: match.to, insert: replacement })), userEvent: 'input.replace' });
  else {
    const fallback = $('edit-area');
    const first = matches[0], last = matches[matches.length - 1];
    const source = fallback.value;
    let inserted = '', offset = first.from;
    for (const match of matches) { inserted += source.slice(offset, match.from) + replacement; offset = match.to; }
    inserted += source.slice(offset, last.to);
    fallback.focus(); fallback.setSelectionRange(first.from, last.to);
    // Native editing command preserves the browser's undo history in fallback mode.
    if (!document.execCommand('insertText', false, inserted)) {
      fallback.setRangeText(inserted, first.from, last.to, 'end');
    }
    fallback.dispatchEvent(new Event('input', { bubbles: true }));
  }
  searchEditor(query, { jump: false });
  // One replacement operation is one undo step, including Replace All.
  if (view) view.focus();
}

function syncSearchMode() {
  const bar = $('search-bar');
  if (!bar) return;
  if (!state.editing) editorFindState = { query: '', doc: null, matches: [], index: 0, capped: false };
  bar.classList.toggle('is-editor-search', !!state.editing);
  const replacement = $('search-replace-controls');
  if (replacement) replacement.hidden = !state.editing;
  if (!bar.classList.contains('hidden')) doSearch($('search-input').value, undefined, { jump: false });
}

function clearMarks() {
  state.currentMarks.forEach(m => {
    const p = m.parentNode;
    if (!p) return;
    p.replaceChild(document.createTextNode(m.textContent), m);
    p.normalize();
  });
  state.currentMarks = [];
  state.searchIndex = 0;
  globalSearchState = { query: '', matches: [], globalIndex: 0 };
  updateSearchCount();
  if (window.ReadMDReader) window.ReadMDReader.flushPendingHighlight();
}

function pageSearchText(page) {
  if (typeof page.searchText === 'string') return page.searchText;
  const transformed = transformAcademicCallouts(page.content);
  const prot = protectMath(transformed);
  const html = marked.parse(prot.src, { gfm: true, breaks: false });
  const probe = document.createElement('div');
  probe.innerHTML = restoreMath(html, prot.saved);
  // Mirror the reader's DOM: callout markers such as "[!NOTE]" become
  // non-searchable chrome labels, so they must not count as matches here.
  page.searchText = (probe.textContent || '')
    .replace(/\[![A-Za-z][\w-]*\][+-]?/g, '')
    .toLowerCase();
  return page.searchText;
}

function highlightTextMatches(body, query) {
  const walker = document.createTreeWalker(body, NodeFilter.SHOW_TEXT, {
    acceptNode: node => {
      const parent = node.parentNode;
      if (!parent || parent.nodeName === 'SCRIPT' || parent.nodeName === 'STYLE') return NodeFilter.FILTER_REJECT;
      if (parent.nodeName === 'MARK' && parent.classList.contains('hl')) return NodeFilter.FILTER_REJECT;
      // Reader chrome (code headers, line numbers, reading time) is not document text.
      if (parent.closest && parent.closest('[data-rd-chrome]')) return NodeFilter.FILTER_REJECT;
      return NodeFilter.FILTER_ACCEPT;
    },
  });
  const nodes = [];
  let node;
  while ((node = walker.nextNode())) nodes.push(node);
  const segments = nodes.map(textNode => textNode.textContent);
  const haystack = segments.join('').toLowerCase();
  const needle = query.toLowerCase();
  if (!needle) return;

  const positionAt = offset => {
    for (let index = 0; index < nodes.length; index += 1) {
      const length = segments[index].length;
      if (offset <= length) return { node: nodes[index], start: offset };
      offset -= length;
    }
    return null;
  };

  const ranges = [];
  let position = haystack.indexOf(needle);
  while (position !== -1) {
    const start = positionAt(position);
    const end = positionAt(position + needle.length);
    if (!start || !end) break;
    const range = document.createRange();
    range.setStart(start.node, start.start);
    range.setEnd(end.node, end.start);
    ranges.push(range);
    position = haystack.indexOf(needle, position + needle.length);
  }

  for (let index = ranges.length - 1; index >= 0; index -= 1) {
    const range = ranges[index];
    const mark = document.createElement('mark');
    mark.className = 'hl';
    mark.appendChild(range.extractContents());
    range.insertNode(mark);
    state.currentMarks.unshift(mark);
  }
}

function highlightCurrentPageForSearch(query) {
  const body = document.querySelector('#content .markdown-body');
  if (!body) return false;
  state.currentMarks = [];
  state.searchIndex = 0;
  highlightTextMatches(body, query);
  return true;
}

function focusPagedSearchMatch(match) {
  if (!match || match.pageIndex === state.pagination.currentPage) {
    if (match) jumpToLocalMark(match.matchIdxInPage);
    return;
  }

  // Page replacement removes the previous DOM marks synchronously.  Do not
  // queue the re-highlight behind a frame: low-end devices can starve frames
  // while laying out a long page, which would leave the search result lost.
  renderPage(match.pageIndex);
  if (!highlightCurrentPageForSearch(globalSearchState.query)) return;
  updateSearchCount();
  enterAdvancePending = true;
  jumpToLocalMark(match.matchIdxInPage);
}

function doSearch(q, jumpToIdx, { jump = true } = {}) {
  if (searchEditor(q, { jump, index: jumpToIdx })) return;
  clearMarks();
  state.lastQuery = q;
  if (!q) {
    enterAdvancePending = false;
    updateSearchCount();
    return;
  }

  // 1. 分页模式下：在内存中预先对所有页进行关键词索引
  const isPaged = state.pagination && state.pagination.enabled && state.pagination.mode === 'paged' && state.pagination.pages && state.pagination.pages.length;
  if (isPaged) {
    const ql = q.toLowerCase();
    if (!Array.isArray(state.pagination.searchText) || state.pagination.searchText.length !== state.pagination.pages.length) {
      state.pagination.searchText = state.pagination.pages.map(pageSearchText);
    }
    const allMatches = [];
    state.pagination.pages.forEach((pg, pIdx) => {
      const text = state.pagination.searchText[pIdx];
      let pos = 0;
      let countInPage = 0;
      while ((pos = text.indexOf(ql, pos)) !== -1) {
        allMatches.push({ pageIndex: pIdx, matchIdxInPage: countInPage });
        countInPage++;
        pos += ql.length;
      }
    });
    globalSearchState = {
      query: q,
      matches: allMatches,
      globalIndex: typeof jumpToIdx === 'number' ? jumpToIdx : 0,
    };
  }

  // 2. 在当前页 DOM 中生成实际的高亮 mark 标签
  if (!isPaged) {
    globalSearchState = { query: q, matches: [], globalIndex: 0 };
  }
  if (!highlightCurrentPageForSearch(q)) return;

  updateSearchCount();
  enterAdvancePending = true;

  if (isPaged && globalSearchState.matches.length > 0) {
    const curMatch = globalSearchState.matches[globalSearchState.globalIndex];
    if (!jump) return;
    if (curMatch && curMatch.pageIndex !== state.pagination.currentPage) {
      focusPagedSearchMatch(curMatch);
      return;
    }
    if (curMatch && curMatch.pageIndex === state.pagination.currentPage) {
      jumpToLocalMark(curMatch.matchIdxInPage);
    }
  } else if (jump && state.currentMarks.length > 0) {
    jumpToLocalMark(0);
  }
}

function focusCurrentSearchMatch() {
  const isPaged = state.pagination?.enabled && state.pagination.mode === 'paged' && globalSearchState.matches.length > 0;
  if (isPaged) {
    const match = globalSearchState.matches[globalSearchState.globalIndex];
    if (!match) return;
    focusPagedSearchMatch(match);
    return;
  }
  jumpToLocalMark(0);
}

function jumpToLocalMark(idx) {
  if (!state.currentMarks.length) return;
  state.searchIndex = Math.max(0, Math.min(idx, state.currentMarks.length - 1));
  state.currentMarks.forEach((m, i) => m.classList.toggle('cur', i === state.searchIndex));
  revealSearchMark(state.currentMarks[state.searchIndex]);
}

function jumpToMark(dir) {
  if (state.editing) {
    searchEditor($('search-input').value, { jump: false });
    searchEditor($('search-input').value, { index: editorFindState.index + dir });
    return;
  }
  const isPaged = state.pagination && state.pagination.enabled && state.pagination.mode === 'paged' && globalSearchState.matches.length > 0;

  if (isPaged) {
    const total = globalSearchState.matches.length;
    const nextGlobal = (globalSearchState.globalIndex + dir + total) % total;
    globalSearchState.globalIndex = nextGlobal;
    const targetMatch = globalSearchState.matches[nextGlobal];

    if (targetMatch.pageIndex !== state.pagination.currentPage) {
      focusPagedSearchMatch(targetMatch);
    } else {
      updateSearchCount();
      jumpToLocalMark(targetMatch.matchIdxInPage);
    }
    return;
  }

  // 常规/连续模式单页跳转
  if (!state.currentMarks.length) return;
  state.searchIndex = (state.searchIndex + dir + state.currentMarks.length) % state.currentMarks.length;
  state.currentMarks.forEach((m, i) => m.classList.toggle('cur', i === state.searchIndex));
  revealSearchMark(state.currentMarks[state.searchIndex]);
  updateSearchCount();
}

function revealSearchMark(mark) {
  if (!mark) return;
  mark.scrollIntoView({ behavior: preferredScrollBehavior(), block: 'center' });
}

function updateSearchCount() {
  if (state.editing) {
    const found = editorFindState;
    const hasHits = !!found.matches.length;
    $('search-prev').disabled = $('search-next').disabled = !hasHits;
    $('search-replace-one').disabled = !hasHits;
    $('search-replace-all').disabled = !hasHits || found.capped;
    $('search-bar').classList.toggle('rd-search-empty', !!found.query && !hasHits);
    $('search-count').textContent = hasHits ? `${found.index + 1}/${found.matches.length}${found.capped ? '+' : ''}` : found.query ? window.i18n.t('search.noMatches') : '';
    return;
  }
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  const bar = $('search-bar');
  if (bar) {
    const hasHits = state.currentMarks.length > 0 || globalSearchState.matches.length > 0;
    bar.classList.toggle('rd-search-empty', !!state.lastQuery && !hasHits);
    $('search-prev').disabled = !hasHits;
    $('search-next').disabled = !hasHits;
  }
  const isPaged = state.pagination && state.pagination.enabled && state.pagination.mode === 'paged' && globalSearchState.matches.length > 0;

  if (isPaged) {
    const total = globalSearchState.matches.length;
    const cur = globalSearchState.globalIndex + 1;
    const pIdx = globalSearchState.matches[globalSearchState.globalIndex]?.pageIndex;
    const pageNum = (typeof pIdx === 'number' ? pIdx + 1 : state.pagination.currentPage + 1);
    $('search-count').textContent = total ? `${cur}/${total} (P.${pageNum})` : (state.lastQuery ? (_t('search.noMatches') || '无结果') : '');
    return;
  }

  const total = state.currentMarks.length;
  $('search-count').textContent = total ? ((state.searchIndex % total) + 1) + '/' + total : (state.lastQuery ? (_t('search.noMatches') || '无结果') : '');
}

function toggleSearch() {
  const _t = (k, p) => window.i18n ? window.i18n.t(k, p) : k;
  if (state.mode === 'welcome' || (!state.editing && !state.file && state.original == null)) {
    showToast(_t('toast.searchNeedsDocument') || '请先打开文档，再按 Ctrl+F 搜索');
    return;
  }
  const bar = $('search-bar');
  syncSearchMode();
  if (bar.classList.contains('hidden')) {
    bar.classList.remove('hidden');
    if (state.editing) {
      const selected = window.cmView ? window.cmView.state.sliceDoc(window.cmView.state.selection.main.from, window.cmView.state.selection.main.to) : $('edit-area').value.slice($('edit-area').selectionStart, $('edit-area').selectionEnd);
      if (selected && selected.length <= 256 && !selected.includes('\n')) $('search-input').value = selected;
      searchEditor($('search-input').value, { jump: false });
    }
    $('search-input').focus();
    $('search-input').select();
  } else {
    closeSearch({ restoreFocus: true });
  }
}

function closeSearch({ restoreFocus = false } = {}) {
  $('search-bar').classList.add('hidden');
  clearMarks();
  editorFindState = { query: '', doc: null, matches: [], index: 0, capped: false };
  if (restoreFocus && state.editing && window.cmView) { window.cmView.focus(); return; }
  if (restoreFocus && state.editing) { $('edit-area').focus(); return; }
  if (restoreFocus && $('btn-search')) $('btn-search').focus({ preventScroll: true });
}

function consumeInitialSearchJump() {
  if (searchEditor($('search-input').value, { jump: true })) return;
  focusCurrentSearchMatch();
}
