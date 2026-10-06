'use strict';
/* Shell chrome: palette / sheet keys, theme cross-fade, welcome wiring. */
(function () {
  const byId = id => document.getElementById(id);
  const reduceMotion = () => !!(window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches);

  function withThemeTransition(apply) {
    const root = document.documentElement;
    if (typeof document.startViewTransition !== 'function' || reduceMotion() || document.hidden) { apply(); return; }
    root.classList.add('rm-theme-vt');
    let vt;
    try { vt = document.startViewTransition(() => { apply(); }); }
    catch (e) { root.classList.remove('rm-theme-vt'); apply(); return; }
    const done = () => root.classList.remove('rm-theme-vt');
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
  if (!hasPy || !py.custom_titlebar || !py.window_control) return;
  const t = key => window.i18n?.t(key) || key;
  py.window_control('preferences', { closeToTray: state.closeToTray !== false, labels: [t('window.show'), t('toolbar.open'), t('window.exit')] });
  $('btn-close-to-tray')?.setAttribute('aria-checked', String(state.closeToTray !== false));
  if ($('close-to-tray-status')) $('close-to-tray-status').textContent = t(state.closeToTray !== false ? 'app.enabled' : 'app.disabled');
}
function initWindowChrome() {
  const actions = $('titlebar-actions');
  if (!actions || actions.dataset.initialized) return;
  actions.dataset.initialized = 'true';
  for (const id of ['btn-open', 'btn-folder', 'btn-recent', 'btn-palette', 'btn-theme', 'btn-more', 'more-menu']) {
    if ($(id)) actions.appendChild($(id));
  }
  const title = $('window-title');
  const native = !!(hasPy && py.custom_titlebar && py.window_control);
  const command = (action, options) => { if (native) py.window_control(action, options); };
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
  const drag = $('window-drag-region');
  let press = null;
  drag.addEventListener('pointerdown', e => { if (e.button === 0) press = { x: e.screenX, y: e.screenY }; });
  drag.addEventListener('pointermove', e => {
    if (press && (e.buttons & 1) && Math.hypot(e.screenX - press.x, e.screenY - press.y) >= 4) { press = null; command('drag'); }
  });
  window.addEventListener('pointerup', () => { press = null; });
  drag.addEventListener('dblclick', () => command('maximize'));
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
