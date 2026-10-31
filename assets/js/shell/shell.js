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
  const t = key => window.i18n?.t(key) || key;
  const label = key => t(key).replace(/\s*[（(][^()（）]*[)）]\s*$/u, '');
  const preferences = { closeToTray: state.closeToTray !== false, labels: [t('window.show'), t('toolbar.open'), t('window.exit')], fileLabels: [t('menu.file'), t('menu.save'), t('menu.saveAs')], editLabels: ['shell.group.edit','editor.undo','editor.redo','editor.cut','editor.copy','editor.paste'].map(label) };
  if (py.native_titlebar === 'macos') preferences.menuLabels = Object.fromEntries(Object.entries({new:'menu.new',folder:'menu.openFolder',plugins:'cmd.plugins',pet:'cmd.pet',settings:'menu.settings',hide:'mac.hide',
    'close-document':'toolbar.close',view:'shell.group.view',zen:'menu.zenMode',fullscreen:'toolbar.fullScreen',minimize:'window.minimize','select-all':'mac.selectAll'}).map(([action,key])=>[action,label(key)]));
  py.window_control('preferences', preferences);
  $('btn-close-to-tray')?.setAttribute('aria-checked', String(state.closeToTray !== false));
  if ($('close-to-tray-status')) $('close-to-tray-status').textContent = t(state.closeToTray !== false ? 'app.enabled' : 'app.disabled');
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
    case 'close-document': if (state.activeTabId) await closeTab(state.activeTabId); break;
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
  for (const id of ['btn-open', 'btn-folder', 'btn-recent', 'btn-palette', 'btn-theme', 'btn-zen']) {
    if ($(id)) actions.appendChild($(id));
  }
  const ai = $('btn-ai');
  if (ai && $('btn-more')) ai.before($('btn-more'));
  const title = $('window-title');
  const native = !!(hasPy && py.custom_titlebar && py.window_control);
  const macNative = !!(hasPy && py.native_titlebar === 'macos' && py.window_control);
  const command = (action, options) => { if (native || macNative) py.window_control(action, options); };
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
  if (!native) return;
  document.body.classList.add('custom-titlebar');
  $('window-controls').classList.remove('hidden');
  $('btn-close-to-tray').classList.remove('hidden');
  $('window-minimize').onclick = () => command('minimize');
  $('window-maximize').onclick = () => command('maximize');
  $('window-close').onclick = () => command('close');
  $('btn-close-to-tray').onclick = async () => {
    const previous = state.closeToTray; state.closeToTray = previous === false;
    syncWindowPreferences();
    if (!await saveSettings()) { state.closeToTray = previous; syncWindowPreferences(); }
  };
  const drag = $('app-titlebar');
  const interactive = target => target.closest('button,a,input,select,textarea,#titlebar-actions,#window-controls');
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
    $('btn-close-to-tray').disabled = !status.trayAvailable;
  };
  syncWindowPreferences();
  window.addEventListener('readmd:language-changed', syncWindowPreferences);
}

function initMacWindowChrome(command) {
  document.body.classList.add('native-macos');
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
  for (const id of ['doc-tabs-bar', 'doc-tabs-secondary-bar']) {
    const bar = $(id); if (bar) new MutationObserver(schedule).observe(bar, { childList: true, subtree: true });
  }
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
  const interactive = target => target.closest('button,a,input,select,textarea,#titlebar-actions');
  let press = null;
  drag.addEventListener('pointerdown', event => { if (event.button === 0 && !interactive(event.target)) press = { x: event.screenX, y: event.screenY }; });
  drag.addEventListener('pointermove', event => {
    if (press && (event.buttons & 1) && Math.hypot(event.screenX - press.x, event.screenY - press.y) >= 4) { press = null; command('drag'); }
  });
  window.addEventListener('pointerup', () => { press = null; });
  drag.addEventListener('dblclick', event => { if (!interactive(event.target)) command('maximize'); });
  schedule();
}
