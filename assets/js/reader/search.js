'use strict';
/* ============================================================
   ReadMD Reader - In-Document Search & Highlighting
   ============================================================ */

let globalSearchState = { query: '', matches: [], globalIndex: 0 };
let editorFindState = { query: '', doc: null, matches: [], index: 0, capped: false };
let readerMarkGroups = [];
let activeMarkGroup = null;
const pageSearchIndexes = new WeakMap();
const searchHeadingSelector = 'h1,h2,h3,h4,h5,h6';
const searchBlockSelector = `${searchHeadingSelector},article,blockquote,div,p,li,td,th,pre,dt,dd,summary,figcaption`;
const searchRowBatchSize = 120;
let searchResultRows = [];
let searchResultFrame = 0;
let activeSearchRow = null;
let readerSearchObserver;
let readerSearchRefresh = 0;

function findSearchMatches(text, query, limit = Infinity) {
  const matches = [];
  if (!query) return { matches, capped: false };
  const re = new RegExp(query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'giu');
  let match;
  while ((match = re.exec(text)) && matches.length < limit) {
    matches.push({ from: match.index, to: match.index + match[0].length, text });
  }
  return { matches, capped: !!match };
}

function searchEditor(query, { jump = true, index } = {}) {
  const view = window.cmView;
  const fallback = $('edit-area');
  if (!state.editing || (!view && !fallback)) return false;
  const doc = view ? view.state.doc : fallback.value;
  if (editorFindState.doc !== doc || editorFindState.query !== query) {
    const source = doc.toString();
    const { matches, capped } = findSearchMatches(source, query, 10000);
    if (matches.length) {
      const headings = state.is_code ? [] : window.ReadMDHeadings.fromMarkdown(source);
      let line = 1, nextBreak = source.indexOf('\n'), nextHeading = 0, section = '';
      for (const match of matches) {
        while (nextBreak !== -1 && nextBreak < match.from) {
          line += 1;
          nextBreak = source.indexOf('\n', nextBreak + 1);
        }
        while (headings[nextHeading]?.sourceLine <= line) section = headings[nextHeading++].text;
        Object.assign(match, { line, section });
      }
    }
    const cursor = view ? view.state.selection.main.from : fallback.selectionStart;
    editorFindState = { query, doc, matches, index: Math.max(0, matches.findIndex(match => match.from >= cursor)), capped };
  }
  if (index !== undefined && editorFindState.matches.length) {
    editorFindState.index = (index + editorFindState.matches.length) % editorFindState.matches.length;
  }
  globalSearchState.query = query;
  state.lastQuery = query;
  const match = editorFindState.matches[editorFindState.index];
  if (jump && match) {
    if (view) view.dispatch({ selection: { anchor: match.from, head: match.to }, effects: window.ReadMDCodeMirror.EditorView.scrollIntoView(match.from, { y: 'center' }) });
    else {
      // Native textarea selection scrolls the draft to the match on focus.
      // Return focus to the search control so repeated Enter still works.
      const searchControl = document.activeElement?.closest('#side-search');
      const previousFocus = searchControl && document.activeElement;
      fallback.focus();
      fallback.setSelectionRange(match.from, match.to);
      previousFocus?.focus({ preventScroll: true });
    }
  }
  updateSearchCount({ reveal: jump });
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

function clearReaderSearchMarks() {
  const parents = new Set();
  for (const group of readerMarkGroups) {
    for (const mark of group) {
      if (!mark.isConnected) continue;
      parents.add(mark.parentNode);
      mark.replaceWith(...mark.childNodes);
    }
  }
  for (const parent of parents) parent.normalize();
  readerMarkGroups = [];
  activeMarkGroup = null;
  state.currentMarks = [];
  state.searchIndex = 0;
  if (parents.size) window.ReadMDReader?.flushPendingHighlight();
}

function readSearchText(body, retainNodes = false) {
  const walker = document.createTreeWalker(body, NodeFilter.SHOW_TEXT, {
    acceptNode: node => !node.nodeValue || node.parentElement?.closest('script,style,[data-rd-chrome]')
      ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT,
  });
  const chunks = [], segments = [];
  let offset = 0, section = '', lastHeading = null, node;
  while ((node = walker.nextNode())) {
    const parent = node.parentElement;
    const heading = parent?.closest(searchHeadingSelector);
    if (heading && heading !== lastHeading) {
      section = heading.dataset.readmdHeadingText ?? heading.textContent.trim();
      lastHeading = heading;
    }
    const text = node.nodeValue;
    segments.push({ end: offset + text.length, section, node: retainNodes ? node : null });
    chunks.push(text);
    offset += text.length;
  }
  return { text: chunks.join(''), segments };
}

function searchSegmentAt(segments, position) {
  let first = 0, end = segments.length;
  while (first < end) {
    const middle = Math.floor((first + end) / 2);
    if (segments[middle].end <= position) first = middle + 1;
    else end = middle;
  }
  return first;
}

function pageSearchIndex(page) {
  const reader = window.ReadMDReader;
  const cached = pageSearchIndexes.get(page);
  if (cached && cached.reader === reader) return cached;
  const transformed = transformAcademicCallouts(transformWikilinks(readerPrepare(page.content)));
  const protectedMath = protectMath(transformed);
  const html = marked.parse(protectedMath.src, { gfm: true, breaks: false });
  const template = document.createElement('template');
  template.innerHTML = sanitizeRenderedHtml(restoreMath(html, protectedMath.saved));
  window.ReadMDHeadings.applyDom(template.content);
  reader?.enhanceSearchProjection?.(template.content);
  const index = { reader, ...readSearchText(template.content) };
  pageSearchIndexes.set(page, index);
  return index;
}

function readerSearchMatches(index, query, pageIndex = null) {
  if (index.query === query) return index.matches;
  const { matches } = findSearchMatches(index.text, query);
  matches.forEach((match, matchIdxInPage) => Object.assign(match, {
    section: index.segments[searchSegmentAt(index.segments, match.from)].section,
    pageIndex, matchIdxInPage,
  }));
  Object.assign(index, { query, matches });
  return matches;
}

function surroundInlineSearchMatch(segments, first, last, match) {
  const start = segments[first].node, end = segments[last].node;
  const block = start.parentElement.closest(searchBlockSelector);
  if (!block || end.parentElement.closest(searchBlockSelector) !== block) return null;
  const from = match.from - (segments[first - 1]?.end || 0);
  const to = match.to - (segments[last - 1]?.end || 0);
  const edge = (node, sibling) => {
    while (node.parentNode !== block && !node[sibling]) node = node.parentNode;
    return node;
  };
  const range = document.createRange();
  if (from === 0) range.setStartBefore(edge(start, 'previousSibling'));
  else range.setStart(start, from);
  if (to === end.length) range.setEndAfter(edge(end, 'nextSibling'));
  else range.setEnd(end, to);
  if (range.toString() !== match.text.slice(match.from, match.to)) return null;
  const mark = document.createElement('mark');
  mark.className = 'hl';
  try {
    range.surroundContents(mark);
    return mark;
  } catch (error) {
    if (error.name !== 'InvalidStateError') throw error;
    return null;
  }
}

function highlightReaderSearchMatches(index, matches) {
  const { segments } = index;
  readerMarkGroups = new Array(matches.length);
  // Work from the end so splitting a text node leaves earlier offsets intact.
  // A partially selected inline element stays intact inside its own mark.
  for (let i = matches.length - 1; i >= 0; i -= 1) {
    const match = matches[i];
    const { from, to } = match;
    const first = searchSegmentAt(segments, from), last = searchSegmentAt(segments, to - 1);
    if (first !== last) {
      const mark = surroundInlineSearchMatch(segments, first, last, match);
      if (mark) {
        readerMarkGroups[i] = [mark];
        continue;
      }
    }
    const group = [];
    for (let pos = last; pos >= first; pos -= 1) {
      const { node, end } = segments[pos];
      const start = segments[pos - 1]?.end || 0;
      const left = Math.max(0, from - start);
      const length = Math.min(to, end) - start - left;
      const found = left ? node.splitText(left) : node;
      if (length < found.length) found.splitText(length);
      const mark = document.createElement('mark');
      mark.className = 'hl';
      found.replaceWith(mark);
      mark.append(found);
      group.unshift(mark);
    }
    readerMarkGroups[i] = group;
  }
  state.currentMarks = readerMarkGroups.map(group => group[0]);
}

function currentSearchPages() {
  const pagination = state.pagination;
  return pagination?.enabled && pagination.mode === 'paged' && pagination.pages?.length ? pagination.pages : null;
}

function currentReaderSearch() {
  return globalSearchState.tabId === state.activeTabId && globalSearchState.pages === currentSearchPages() &&
    globalSearchState.body === document.querySelector('#content .markdown-body');
}

function searchResultSnippet(match) {
  const { text, from, to } = match;
  const before = text.slice(Math.max(0, from - 28), from);
  const after = text.slice(to, to + 80);
  const lineBefore = before.slice(before.lastIndexOf('\n') + 1);
  const lineAfter = after.split('\n', 1)[0];
  const excerpt = part => part.replace(/\s+/g, ' ');
  return [
    (from > before.length && before === lineBefore ? '…' : '') + excerpt(lineBefore),
    excerpt(text.slice(from, Math.min(to, from + 110))) + (to - from > 110 ? '…' : ''),
    excerpt(lineAfter) + (to + after.length < text.length && after === lineAfter ? '…' : ''),
  ];
}

function locateReaderSearchMatch(matches, previous) {
  if (!previous) return -1;
  const candidates = matches.filter(match => match.pageIndex === previous.pageIndex);
  const exact = candidates.find(match => match.from === previous.from && match.text === previous.text);
  if (exact) return matches.indexOf(exact);
  const snippet = searchResultSnippet(previous);
  const nearby = candidates.filter(match => match.section === previous.section &&
    searchResultSnippet(match).every((part, index) => part === snippet[index]));
  if (nearby.length === 1) return matches.indexOf(nearby[0]);
  return -1;
}

function refreshReaderSearch(query, previous, revealed = false) {
  clearReaderSearchMarks();
  const body = document.querySelector('#content .markdown-body');
  const pages = currentSearchPages();
  let matches = [];
  if (body && query) {
    const currentPage = pages ? state.pagination.currentPage : null;
    const index = readSearchText(body, true);
    const local = readerSearchMatches(index, query, currentPage);
    if (pages) {
      // Cache only text and offsets; marks and elements belong to the live page.
      pageSearchIndexes.set(pages[currentPage], {
        text: index.text, segments: index.segments.map(({ end, section }) => ({ end, section })),
        reader: window.ReadMDReader, query, matches: local,
      });
      matches = pages.flatMap((page, pageIndex) => readerSearchMatches(pageSearchIndex(page), query, pageIndex));
    } else matches = local;
    highlightReaderSearchMatches(index, local);
  }
  const found = locateReaderSearchMatch(matches, previous);
  globalSearchState = { query, matches, globalIndex: Math.max(0, found), body, pages, tabId: state.activeTabId, revealed: revealed && found >= 0 };
  if (globalSearchState.revealed) selectReaderSearchMarks(matches[found]);
  state.lastQuery = query;
  readerSearchObserver?.takeRecords();
  updateSearchCount();
  return found;
}

function doSearch(query, index, { jump = true } = {}) {
  if ($('search-bar').classList.contains('hidden')) return;
  clearTimeout(readerSearchRefresh);
  if (searchEditor(query, { jump, index })) return;
  const current = globalSearchState;
  const sameDocument = current.tabId === state.activeTabId && current.pages === currentSearchPages() &&
    (current.pages || current.body === document.querySelector('#content .markdown-body'));
  const previous = query === current.query && sameDocument ? current.matches[current.globalIndex] : null;
  refreshReaderSearch(query, previous, current.revealed);
  if (jump) jumpToReaderSearchMatch(index ?? globalSearchState.globalIndex);
}

function selectReaderSearchMarks(match) {
  if (match.pageIndex !== null && match.pageIndex !== state.pagination.currentPage) return null;
  const group = readerMarkGroups[match.matchIdxInPage];
  if (!group?.length) return null;
  activeMarkGroup?.forEach(mark => mark.classList.remove('cur'));
  activeMarkGroup = group;
  state.searchIndex = match.matchIdxInPage;
  group.forEach(mark => mark.classList.add('cur'));
  return group[0];
}

function jumpToReaderSearchMatch(index) {
  let match = globalSearchState.matches[index];
  if (!match) return;
  globalSearchState.globalIndex = index;
  if (match.pageIndex !== null && match.pageIndex !== state.pagination.currentPage) {
    renderPage(match.pageIndex);
    index = refreshReaderSearch(globalSearchState.query, match);
    if (index < 0) return;
    match = globalSearchState.matches[index];
  }
  const mark = selectReaderSearchMarks(match);
  if (!mark) return;
  globalSearchState.revealed = true;
  let fold = mark.closest('details:not([open])');
  while (fold) {
    fold.open = true;
    fold = fold.parentElement?.closest('details:not([open])');
  }
  mark.scrollIntoView({ behavior: preferredScrollBehavior(), block: 'center' });
  updateSearchCount({ reveal: true });
}

function jumpToMark(dir) {
  const query = $('search-input').value;
  if (state.editing) {
    searchEditor(query, { jump: false });
    searchEditor(query, { index: editorFindState.index + dir });
    return;
  }
  if (query !== globalSearchState.query || !currentReaderSearch()) doSearch(query, undefined, { jump: false });
  const total = globalSearchState.matches.length;
  if (total) jumpToReaderSearchMatch((globalSearchState.globalIndex + dir + total) % total);
}

function updateSearchCount({ reveal = false } = {}) {
  const bar = $('search-bar');
  if (!bar) return;
  const editor = state.editing;
  const { matches, query } = editor ? editorFindState : globalSearchState;
  const index = editor ? editorFindState.index : globalSearchState.globalIndex;
  const total = matches.length;
  const page = !editor ? matches[index]?.pageIndex : null;
  const capped = editor && editorFindState.capped;
  $('search-prev').disabled = $('search-next').disabled = !total;
  $('search-replace-one').disabled = !editor || !total;
  $('search-replace-all').disabled = !editor || !total || capped;
  bar.classList.toggle('rd-search-empty', !!query && !total);
  const label = total ? `${index + 1}/${total}${capped ? '+' : ''}${page == null ? '' : ` (P.${page + 1})`}`
    : query ? (window.i18n?.t('search.noMatches') || '无匹配结果') : '';
  const count = $('search-count');
  if (count.textContent !== label) count.textContent = label;
  updateSearchResultList(matches, index, reveal);
}

function appendSearchResultRows(list, end) {
  const fragment = document.createDocumentFragment();
  for (let index = list.children.length; index < end; index += 1) {
    const match = searchResultRows[index];
    const item = document.createElement('li');
    const button = document.createElement('button');
    button.className = 'search-result';
    button.type = 'button';
    button.tabIndex = -1;
    button.dataset.searchResult = String(index);
    const context = document.createElement('span');
    context.className = 'search-result-context';
    context.dir = 'auto';
    const [before, matched, after] = searchResultSnippet(match);
    const highlight = document.createElement('mark');
    highlight.textContent = matched;
    context.append(before, highlight, after);
    button.append(context);
    const location = [];
    if (match.section) location.push(match.section);
    if (match.line) location.push(window.i18n?.t('graph.linePrefix', { line: match.line }) || String(match.line));
    if (match.pageIndex !== null && match.pageIndex !== undefined) {
      location.push(window.i18n?.t('pagination.pageInfo', { current: match.pageIndex + 1, total: state.pagination.totalPages }) || String(match.pageIndex + 1));
    }
    if (location.length) {
      const meta = document.createElement('span');
      meta.className = 'search-result-location';
      meta.dir = 'auto';
      meta.textContent = location.join(' · ');
      button.append(meta);
    }
    item.append(button);
    fragment.append(item);
  }
  list.append(fragment);
}

function fillSearchResultList(list) {
  const remaining = searchResultRows.length - list.children.length;
  if (remaining <= 0) return;
  searchResultFrame = requestAnimationFrame(() => {
    searchResultFrame = 0;
    appendSearchResultRows(list, Math.min(searchResultRows.length, list.children.length + searchRowBatchSize));
    fillSearchResultList(list);
  });
}

function updateSearchResultList(matches, index, reveal) {
  const list = $('search-results');
  if (!list) return;
  const hasRows = matches.length > 0 && !$('search-bar').classList.contains('hidden');
  let restoreResultFocus = false;
  if (matches !== searchResultRows) {
    restoreResultFocus = list.contains(document.activeElement);
    cancelAnimationFrame(searchResultFrame);
    searchResultFrame = 0;
    searchResultRows = matches;
    activeSearchRow = null;
    list.replaceChildren();
    list.scrollTop = 0;
  }
  list.hidden = !hasRows;
  if (!hasRows) {
    if (restoreResultFocus) $('search-input').focus({ preventScroll: true });
    return;
  }
  const end = Math.min(matches.length, Math.max(searchRowBatchSize, index + 1));
  if (list.children.length < end) appendSearchResultRows(list, end);
  if (!searchResultFrame) fillSearchResultList(list);
  const row = list.children[index]?.firstElementChild;
  if (row !== activeSearchRow) {
    if (activeSearchRow) {
      activeSearchRow.removeAttribute('aria-current');
      activeSearchRow.tabIndex = -1;
    }
    activeSearchRow = row;
    row?.setAttribute('aria-current', 'true');
    if (row) row.tabIndex = 0;
  }
  if (restoreResultFocus) row?.focus({ preventScroll: true });
  if (reveal || restoreResultFocus) row?.scrollIntoView({ block: 'nearest' });
}

function bindSearchResults() {
  const list = $('search-results');
  if (!list || readerSearchObserver) return;
  const input = $('search-input');
  input.addEventListener('input', () => {
    if (input.value !== globalSearchState.query) updateSearchResultList([], 0, false);
  });
  list.addEventListener('click', event => {
    const button = event.target.closest('button[data-search-result]');
    if (!button || input.value !== globalSearchState.query) return;
    const index = Number(button.dataset.searchResult);
    if (state.editing) {
      const doc = window.cmView?.state.doc ?? $('edit-area').value;
      if (doc === editorFindState.doc) searchEditor(globalSearchState.query, { index });
    } else if (currentReaderSearch()) {
      jumpToReaderSearchMatch(index);
    }
  });
  list.addEventListener('keydown', event => {
    if (event.altKey || event.ctrlKey || event.metaKey || event.isComposing) return;
    const index = Number(event.target.dataset.searchResult);
    if (!Number.isInteger(index)) return;
    let next;
    if (event.key === 'ArrowDown') next = index + 1;
    else if (event.key === 'ArrowUp') next = index - 1;
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = searchResultRows.length - 1;
    else return;
    event.preventDefault();
    next = Math.max(0, Math.min(next, searchResultRows.length - 1));
    if (list.children.length <= next) appendSearchResultRows(list, next + 1);
    list.children[next]?.firstElementChild?.focus();
  });
  input.addEventListener('keydown', event => {
    if (event.key !== 'ArrowDown' || event.isComposing || !activeSearchRow) return;
    event.preventDefault();
    activeSearchRow.focus();
  });
  window.addEventListener('readmd:language-changed', () => {
    if (!$('search-bar').classList.contains('hidden')) {
      searchResultRows = [];
      updateSearchCount();
    }
  });
  readerSearchObserver = new MutationObserver(records => {
    if (state.editing || !input.value || records.every(record =>
      (record.target.nodeType === Node.ELEMENT_NODE ? record.target : record.target.parentElement)?.closest('[data-rd-chrome]'))) return;
    clearTimeout(readerSearchRefresh);
    readerSearchRefresh = setTimeout(() => doSearch(input.value, undefined, { jump: false }), 24);
  });
}

function openSidebarSearch() {
  const bar = $('search-bar');
  if (bar.classList.contains('hidden')) {
    bar.classList.remove('hidden');
    if (state.editing) {
      const selected = window.cmView ? window.cmView.state.sliceDoc(window.cmView.state.selection.main.from, window.cmView.state.selection.main.to) : $('edit-area').value.slice($('edit-area').selectionStart, $('edit-area').selectionEnd);
      if (selected && selected.length <= 256 && !selected.includes('\n')) $('search-input').value = selected;
    }
  }
  bindSearchResults();
  readerSearchObserver?.observe($('content'), { childList: true, subtree: true, characterData: true });
  syncSearchMode();
}

function toggleSearch() {
  if ($('btn-search').disabled) {
    showToast(window.i18n?.t('toast.searchNeedsDocument') || '请打开文档');
    return;
  }
  showSide('search');
  $('search-input').focus();
  $('search-input').select();
}

function closeSearch({ restoreFocus = false, closeSidebar = true } = {}) {
  $('search-bar').classList.add('hidden');
  readerSearchObserver?.disconnect();
  clearTimeout(readerSearchRefresh);
  clearReaderSearchMarks();
  globalSearchState = { query: '', matches: [], globalIndex: 0 };
  editorFindState = { query: '', doc: null, matches: [], index: 0, capped: false };
  state.lastQuery = '';
  updateSearchCount();
  if (closeSidebar && $('side').dataset.view === 'search') closeSide();
  if (restoreFocus && state.editing && window.cmView) { window.cmView.focus(); return; }
  if (restoreFocus && state.editing) { $('edit-area').focus(); return; }
  if (restoreFocus) {
    const target = [$('btn-search'), $('btn-toc'), $('content')].find(node => node &&
      !node.disabled && !node.closest('[inert], [hidden], .hidden') &&
      node.getClientRects().length && getComputedStyle(node).visibility !== 'hidden');
    target?.focus({ preventScroll: true });
  }
}

function consumeInitialSearchJump() {
  const query = $('search-input').value;
  if (searchEditor(query, { jump: true })) return;
  if (!currentReaderSearch()) doSearch(query, undefined, { jump: false });
  jumpToReaderSearchMatch(globalSearchState.globalIndex);
}
