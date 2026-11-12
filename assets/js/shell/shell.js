'use strict';
/* Shell chrome: palette / sheet keys, theme cross-fade, welcome wiring. */
(function () {
  const byId = id => document.getElementById(id);
  const reduceMotion = () => !!(window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches);
  let themeTransition = null;
  window.addEventListener('readmd:modal-opened', () => themeTransition?.skipTransition());

  function withThemeTransition(apply) {
    const root = document.documentElement;
    if (typeof document.startViewTransition !== 'function' || reduceMotion() || document.hidden || window.ReadMDModal?.depth()) { apply(); return; }
    themeTransition?.skipTransition();
    root.classList.add('rm-theme-vt');
    let vt;
    try { vt = document.startViewTransition(() => { apply(); }); }
    catch (e) { root.classList.remove('rm-theme-vt'); apply(); return; }
    themeTransition = vt;
    const done = () => { if (themeTransition === vt) { themeTransition = null; root.classList.remove('rm-theme-vt'); } };
    vt.finished.then(done, done);
  }
  function wrapToggleTheme() {
    const orig = window.toggleTheme;
    if (typeof orig !== 'function' || orig.__rmShell) return;
    const wrapped = function () { withThemeTransition(() => orig.apply(this, arguments)); };
    wrapped.__rmShell = true;
    window.toggleTheme = wrapped;
  }

  const isEditable = el => !!el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.tagName === 'SELECT' || el.isContentEditable || !!(el.closest && el.closest('.cm-editor, [contenteditable="true"]')));
  const inCodeMirror = el => !!(el && el.closest && el.closest('.cm-editor'));
  const shellLayer = () => {
    const top = window.ReadMDModal ? window.ReadMDModal.top() : null;
    return !top || top.id === 'command-palette-modal' || top.id === 'shortcuts-modal';
  };

  function onGlobalKey(e) {
    if (e.defaultPrevented || e.isComposing || e.keyCode === 229) return;
    const mod = e.ctrlKey || e.metaKey;
    const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
    const target = e.target;
    if (mod && !e.shiftKey && !e.altKey && key === 'k' && !inCodeMirror(target)) {
      if (!shellLayer()) return;
      e.preventDefault(); e.stopPropagation();
      window.ReadMDPalette && window.ReadMDPalette.toggle();
      return;
    }
    if (mod && e.shiftKey && !e.altKey && key === 'p') {
      if (!shellLayer()) return;
      e.preventDefault(); e.stopPropagation();
      window.ReadMDPalette && window.ReadMDPalette.toggle();
      return;
    }
    if ((mod && !e.altKey && (key === '/' || e.code === 'Slash')) || (!mod && !e.altKey && e.key === '?' && !isEditable(target))) {
      if (mod && inCodeMirror(target)) return;
      if (!shellLayer()) return;
      e.preventDefault(); e.stopPropagation();
      window.ReadMDShortcuts.toggle();
      return;
    }
  }

  function fillKbd(root = document) {
    const A = window.ReadMDKeys;
    if (!A) return;
    root.querySelectorAll('[data-kbd]').forEach(el => {
      if (el.dataset.kbdDone === el.dataset.kbd) return;
      el.innerHTML = A.kbdHtml(el.dataset.kbd);
      el.dataset.kbdDone = el.dataset.kbd;
    });
  }

  function bindChrome() {
    initWindowChrome();
    fillKbd();
    const contentEl = byId('content');
    if (contentEl) new MutationObserver(() => { if (byId('welcome')) fillKbd(contentEl); }).observe(contentEl, { childList: true });
    const btn = byId('btn-palette');
    if (btn) btn.addEventListener('click', () => window.ReadMDPalette && window.ReadMDPalette.open());
    const content = byId('content');
    if (content) content.addEventListener('click', e => {
      const el = e.target.closest && e.target.closest('#welcome [data-action]');
      if (!el) return;
      if (el.dataset.action === 'palette') window.ReadMDPalette && window.ReadMDPalette.open();
      else if (el.dataset.action === 'shortcuts') window.ReadMDShortcuts && window.ReadMDShortcuts.open();
    });
  }

  window.addEventListener('keydown', onGlobalKey, true);
  wrapToggleTheme();
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', bindChrome);
  else bindChrome();

  window.ReadMDShell = { withThemeTransition };
})();

function syncWindowPreferences() {
  if (!hasPy || (!py.custom_titlebar && py.native_titlebar !== 'macos') || !py.window_control) return;
  const translated = key => {
    const value = window.i18n?.t(key);
    return typeof value === 'string' && value !== key ? value.trim() : '';
  };
  const preferences = { closeToTray: state.closeToTray !== false };
  if (py.native_titlebar === 'macos') {
    const keys = {
      show: 'window.show', open: 'menu.open', exit: 'window.exit',
      file: 'menu.file', new: 'menu.new', folder: 'menu.openFolder', save: 'menu.save',
      'save-as': 'menu.saveAs', 'close-document': 'toolbar.close',
      edit: 'toolbar.edit', undo: 'editor.undo', redo: 'editor.redo',
      cut: 'editor.cut', copy: 'editor.copy', paste: 'editor.paste', 'select-all': 'mac.selectAll',
      view: 'shell.group.view', zen: 'menu.zenMode', fullscreen: 'toolbar.fullScreen',
      minimize: 'window.minimize', plugins: 'cmd.plugins', pet: 'cmd.pet', settings: 'menu.settings', hide: 'mac.hide',
    };
    const labels = {};
    for (const [action, key] of Object.entries(keys)) {
      let text = translated(key);
      if (['edit', 'undo', 'redo', 'fullscreen'].includes(action)) text = text.replace(/\s*[（(](?:[^()（）]*\+[^()（）]+|F11)[)）]$/iu, '');
      if (text) labels[action] = text;
    }
    preferences.menuLabels = labels;
  } else {
    const labels = ['window.show', 'toolbar.open', 'window.exit'].map(translated);
    if (labels.every(Boolean)) preferences.labels = labels;
  }
  py.window_control('preferences', preferences);
  $('btn-close-to-tray')?.setAttribute('aria-checked', String(preferences.closeToTray));
  const status = $('close-to-tray-status');
  const statusLabel = translated(preferences.closeToTray ? 'app.enabled' : 'app.disabled');
  if (status && statusLabel) status.textContent = statusLabel;
}

// Native menus enter the existing UI workflows. Modal guards, revision checks,
// workspace permissions and save/recovery remain owned by those workflows.
let nativeMenuQueue = Promise.resolve();
window.__readmdNativeMenu = action => {
  // AppKit acknowledges dispatch before an asynchronous editor transition
  // finishes. Preserve command order so New followed by Close is not lost
  // against the document service's busy guard.
  const work = nativeMenuQueue.then(() => runNativeMenu(action));
  nativeMenuQueue = work.catch(() => false);
  return work;
};
async function runNativeMenu(action) {
  await window.__readmdSessionReady;
  if (typeof cmView !== 'undefined' && cmView?.composing) return false;
  if (window.ReadMDModal?.top()) {
    if (action === 'close-document') return window.ReadMDModal.closeTop();
    return false;
  }
  switch (action) {
    case 'new': await newDocument(); break;
    case 'folder': await openFolder(); break;
    case 'plugins': await openPluginModal(); break;
    case 'pet': await openPetSettings(); break;
    case 'settings': await window.ReadMDPalette?.open('settings'); break;
    case 'close-document': if (state.activeTabId) await closeTab(state.activeTabId, false, 'command'); break;
    case 'zen': toggleZenMode(); break;
    case 'fullscreen': await window.ReadMDViewMode?.toggleFullscreen(); break;
    default: return false;
  }
  return true;
}
function initWindowChrome() {
  const actions = $('titlebar-actions');
  if (!actions || actions.dataset.initialized) return;
  actions.dataset.initialized = 'true';
  initSingleChromeNavigation();
  const title = $('window-title');
  const macNative = !!(hasPy && py.native_titlebar === 'macos' && py.window_control);
  const custom = !macNative && !!(hasPy && py.custom_titlebar && py.window_control);
  const command = (action, options) => { if (custom || macNative) py.window_control(action, options); };
  const updateTitle = () => {
    title.textContent = document.title.replace(/\s*[-–·]\s*ReadMD$/, '') || 'ReadMD';
    title.title = title.textContent;
    command('title', { title: document.title });
  };
  new MutationObserver(updateTitle).observe(document.querySelector('title'), { childList: true, subtree: true, characterData: true });
  updateTitle();
  // Keyboard access to global actions also reveals the document actions in zen mode.
  actions.addEventListener('focusin', () => {
    document.body.classList.remove('zen-toolbar-suppressed');
    $('toolbar')?.classList.add('zen-toolbar-revealed');
  });
  actions.addEventListener('focusout', () => setTimeout(() => {
    if (!actions.contains(document.activeElement)) $('toolbar')?.classList.remove('zen-toolbar-revealed');
  }, 0));
  if (macNative) initMacWindowChrome(command);
  if (custom || macNative) {
    syncWindowPreferences();
    window.addEventListener('readmd:language-changed', syncWindowPreferences);
  }
  if (!custom) return;
  document.body.classList.add('custom-titlebar');
  $('window-controls').classList.remove('hidden');
  const trayToggle = $('btn-close-to-tray');
  $('window-minimize').onclick = () => command('minimize');
  $('window-maximize').onclick = () => command('maximize');
  $('window-close').onclick = () => command('close');
  trayToggle.onclick = async () => {
    const previous = state.closeToTray; state.closeToTray = previous === false;
    syncWindowPreferences();
    if (!await saveSettings()) { state.closeToTray = previous; syncWindowPreferences(); }
  };
  const drag = $('app-titlebar');
  const interactive = target => target.closest('button,a,input,select,textarea,.tab-item,#doc-tabs-dropdown,#window-controls');
  let press = null;
  drag.addEventListener('pointerdown', e => { if (e.button === 0 && !interactive(e.target)) press = { x: e.screenX, y: e.screenY }; });
  drag.addEventListener('pointermove', e => {
    if (press && (e.buttons & 1) && Math.hypot(e.screenX - press.x, e.screenY - press.y) >= 4) { press = null; command('drag'); }
  });
  window.addEventListener('pointerup', () => { press = null; });
  drag.addEventListener('dblclick', e => { if (!interactive(e.target)) command('maximize'); });
  for (const edge of ['n', 'ne', 'e', 'se', 's', 'sw', 'w', 'nw']) {
    const handle = document.createElement('div'); handle.className = 'window-resize-edge edge-' + edge;
    handle.setAttribute('aria-hidden', 'true');
    handle.onpointerdown = e => { if (e.button === 0) { e.preventDefault(); command('resize', { edge }); } };
    document.body.appendChild(handle);
  }
  window.__readmdWindowState = status => {
    document.body.classList.toggle('window-maximized', !!status.maximized);
    document.body.classList.toggle('window-fullscreen', !!status.fullscreen);
    const button = $('window-maximize');
    const label = window.i18n?.t(status.maximized ? 'window.restore' : 'window.maximize');
    button.setAttribute('aria-label', label || 'Maximize'); button.title = label || '';
    button.setAttribute('aria-pressed', String(!!status.maximized));
    button.innerHTML = status.maximized ? '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M6 3h7v7M6 3v3h4v4h3"/><rect x="3" y="6" width="7" height="7"/></svg>' : '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="3.5" y="3.5" width="9" height="9"/></svg>';
    $('window-close').title = window.i18n?.t(status.trayAvailable && state.closeToTray !== false ? 'window.hide' : 'window.close') || '';
    trayToggle.classList.toggle('hidden', !status.trayAvailable);
    trayToggle.disabled = !status.trayAvailable;
  };
}

// The rail stays in place. Its views use the original document services and
// controls, so changing panel does not rebind a command or change the document.
function syncSidebar() {
  const side = $('side');
  const open = !side.classList.contains('hidden');
  const view = side.dataset.view;
  $('btn-toc').setAttribute('aria-expanded', String(open));
  for (const button of $('side-tabs').querySelectorAll('[data-side-view]')) {
    const current = button.dataset.sideView === view;
    button.setAttribute('aria-selected', String(open && current));
    button.tabIndex = current && !button.disabled ? 0 : -1;
  }
  if (!$('side-tabs').querySelector('[tabindex="0"]')) $('tab-files').tabIndex = 0;
}

function showSide(view = $('side').dataset.view, { loadFiles = true } = {}) {
  const side = $('side');
  const next = $('side-' + view);
  if (!next?.classList.contains('side-view') || view === 'search' && $('btn-search').disabled) view = 'files';
  if (side.dataset.view === 'search' && view !== 'search') closeSearch({ closeSidebar: false });
  side.dataset.view = view;
  for (const panel of side.querySelectorAll('.side-view')) panel.hidden = panel.id !== 'side-' + view;
  side.classList.remove('hidden');
  syncSidebar();
  if (view === 'files') {
    renderFolderList();
    if (loadFiles && state.folder && !state.folderFiles.length) void listFolder(state.folder, { reveal: false });
  } else if (view === 'toc') {
    buildToc();
  } else if (view === 'recent') {
    refreshRecent();
  } else if (view === 'search') {
    openSidebarSearch();
  }
}

function closeSide({ restoreFocus = false } = {}) {
  const side = $('side');
  if (side.classList.contains('hidden')) return;
  const focused = side.contains(document.activeElement);
  if (!$('search-bar').classList.contains('hidden')) closeSearch({ closeSidebar: false });
  side.classList.add('hidden');
  syncSidebar();
  if (restoreFocus || focused) $('btn-toc').focus({ preventScroll: true });
}

function toggleSide(view) {
  const side = $('side');
  if (!side.classList.contains('hidden') && (!view || view === side.dataset.view)) closeSide();
  else showSide(view);
}

function initSingleChromeNavigation() {
  const side = $('side'), rail = $('side-tabs');
  if (!side || !rail || rail.dataset.bound) return;
  rail.dataset.bound = 'true';
  $('btn-toc').addEventListener('click', () => toggleSide());
  $('doc-tabs-new').addEventListener('click', () => newDocument());
  const buttons = [...rail.querySelectorAll('[data-side-view]')];
  for (const button of buttons) button.addEventListener('click', () => {
    const view = button.dataset.sideView;
    if (view === 'search' && (side.classList.contains('hidden') || side.dataset.view !== view)) toggleSearch();
    else toggleSide(view);
  });
  rail.addEventListener('keydown', event => {
    if (event.isComposing || !['ArrowUp','ArrowDown','Home','End'].includes(event.key)) return;
    const items = buttons.filter(button => !button.disabled);
    const from = items.indexOf(document.activeElement);
    if (from < 0) return;
    const next = event.key === 'Home' ? items[0] : event.key === 'End' ? items.at(-1) :
      items[(from + (event.key === 'ArrowDown' ? 1 : items.length - 1)) % items.length];
    event.preventDefault();
    showSide(next.dataset.sideView);
    next.focus();
  });
  // Some document lifecycle paths hide the panel directly. Keep the rail
  // state correct and clear search marks when those paths close it.
  new MutationObserver(() => {
    if (side.classList.contains('hidden') && !$('search-bar').classList.contains('hidden')) closeSearch({ closeSidebar: false });
    syncSidebar();
  }).observe(side, { attributes: true, attributeFilter: ['class'] });
  const bar = $('doc-tabs-bar');
  new ResizeObserver(() => syncTopTabOverflow({ reveal: true })).observe($('doc-tabs-container'));
  bar.addEventListener('wheel', event => {
    if (event.deltaX || !event.deltaY || event.ctrlKey || bar.scrollWidth <= bar.clientWidth) return;
    event.preventDefault();
    bar.scrollLeft += event.deltaY;
  }, { passive: false });
  syncSidebar();
}

function initMacWindowChrome(command) {
  document.body.classList.add('native-macos');
  // Reuse the packaged Mac brand, including its optical inset and rounded tile.
  // The original mark remains the default for browsers and other platforms.
  for (const mark of document.querySelectorAll('.welcome-mark')) {
    mark.src = '/assets/ReadMD-macos.svg';
  }
  // Cocoa draws, localizes and operates the traffic lights. The web page has
  // no second set of caption buttons or resize hit areas on this platform.
  $('window-controls')?.classList.add('hidden');
  let scheduled = false, previous = '';
  const visible = () => {
    if (!document.body.classList.contains('zen-mode')) return true;
    const focused = document.activeElement?.closest?.('#app-titlebar,#toolbar');
    return !!focused || !!$('app-titlebar')?.matches(':hover') || !!$('toolbar')?.matches(':hover')
      || !!$('toolbar')?.classList.contains('zen-toolbar-revealed')
      || !document.body.classList.contains('zen-toolbar-suppressed') && !!document.querySelector('.zen-hover-trigger:hover');
  };
  const update = () => {
    scheduled = false;
    const payload = { visible: visible(), theme: document.body.dataset.theme === 'dark' ? 'dark' : 'light',
      edited: !!state.tabs?.some(tab => tab.isDirty) || !!window.ReadMDPdf?.dirty };
    const key = JSON.stringify(payload);
    if (key !== previous) { previous = key; command('chrome', payload); }
  };
  const schedule = () => { if (!scheduled) { scheduled = true; requestAnimationFrame(update); } };
  new MutationObserver(schedule).observe(document.body, { attributes: true, attributeFilter: ['class', 'data-theme'] });
  new MutationObserver(schedule).observe($('doc-tabs-bar'), { childList: true, subtree: true });
  for (const target of [$('app-titlebar'), $('toolbar'), document.querySelector('.zen-hover-trigger')]) {
    if (!target) continue;
    for (const event of ['pointerenter', 'pointerleave', 'focusin', 'focusout']) target.addEventListener(event, schedule);
  }
  window.addEventListener('readmd:document-transaction', schedule);
  window.__readmdWindowState = status => {
    window.__readmdNativeWindowState = status;
    document.body.classList.toggle('window-maximized', !!status.maximized);
    document.body.classList.toggle('window-fullscreen', !!status.fullscreen);
    document.body.classList.toggle('window-inactive', status.focused === false);
    const inset = status.chrome?.leadingInset;
    if (Number.isFinite(inset) && inset >= 76 && inset <= 128) document.body.style.setProperty('--native-controls-leading', inset + 'px');
  };
  const drag = $('app-titlebar');
  const interactive = target => target.closest('button,a,input,select,textarea,.tab-item,#doc-tabs-dropdown,#window-controls');
  let press = null;
  drag.addEventListener('pointerdown', event => { if (event.button === 0 && !interactive(event.target)) press = { x: event.screenX, y: event.screenY }; });
  drag.addEventListener('pointermove', event => {
    if (press && (event.buttons & 1) && Math.hypot(event.screenX - press.x, event.screenY - press.y) >= 4) { press = null; command('drag'); }
  });
  window.addEventListener('pointerup', () => { press = null; });
  drag.addEventListener('dblclick', event => { if (!interactive(event.target)) command('maximize'); });
  schedule();
}
