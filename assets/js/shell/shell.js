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
  const native = !!(hasPy && py.custom_titlebar && py.window_control);
  const macNative = !!(hasPy && py.native_titlebar === 'macos' && py.window_control);
  const command = (action, options) => { if (native || macNative) py.window_control(action, options); };
  const updateTitle = () => {
    title.textContent = document.title.replace(/\s*[-–·]\s*ReadMD$/, '') || 'ReadMD';
    title.title = title.textContent;
    syncChromeDocumentDisclosure();
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

// This layer projects the existing document/tab services into the shell. It
// never clones a tab, installs a second close handler or changes editor state.
function syncChromeDocumentDisclosure() {
  const button = $('titlebar-documents'), side = $('side'), section = $('side-documents');
  if (!button || !side) return;
  const tabs = state.tabs || [];
  const legacy = $('file-title');
  const available = tabs.length > 0 || !!(legacy && !legacy.classList.contains('hidden') && legacy.textContent);
  button.disabled = !available;
  button.setAttribute('aria-expanded', String(!side.classList.contains('hidden')));
  $('btn-toc')?.setAttribute('aria-expanded', String(!side.classList.contains('hidden')));
  const label = window.i18n?.t('tabs.openDocs') || 'tabs.openDocs';
  const name = $('window-title')?.textContent || document.title;
  button.setAttribute('aria-label', `${label}: ${name}${tabs.length > 1 ? ` (${tabs.length})` : ''}`);
  button.title = `${name}\n${label}`;
  if (section) section.hidden = !available;
  const icon = $('titlebar-document-icon'); if (icon) icon.hidden = !available;
  const count = $('titlebar-tab-count');
  if (count) { count.textContent = String(tabs.length); count.classList.toggle('hidden', tabs.length < 2); }
  const active = tabs.find(tab => tab.id === state.activeTabId);
  $('titlebar-dirty')?.classList.toggle('hidden', !active?.isDirty && !active?.externalChanged);
  if (active?.isDirty || active?.externalChanged) button.setAttribute('aria-description', window.i18n?.t(active.externalChanged ? 'toast.saveConflict' : 'tabs.dirty') || '');
  else button.removeAttribute('aria-description');
}

function initSingleChromeNavigation() {
  const side = $('side'), disclosure = $('titlebar-documents'), toolbar = $('toolbar');
  if (!side || !disclosure || disclosure.dataset.bound) return;
  disclosure.dataset.bound = 'true';
  let opener = $('btn-toc'), lastFocus = document.activeElement;
  document.addEventListener('focusin', event => { lastFocus = event.target; });
  disclosure.addEventListener('click', () => {
    opener = disclosure;
    side.classList.remove('hidden');
    syncChromeDocumentDisclosure();
    if (state.tabs?.length) focusVisibleTab(state.activeTabId || state.tabs[0].id);
    else $('file-title')?.focus();
  });
  $('btn-toc')?.setAttribute('aria-controls', 'side');
  $('btn-toc')?.addEventListener('pointerdown', () => { opener = $('btn-toc'); });
  new MutationObserver(() => {
    syncChromeDocumentDisclosure();
    if (side.classList.contains('hidden') && (side.contains(document.activeElement) || document.activeElement === document.body && side.contains(lastFocus))) {
      const target = opener && !opener.disabled ? opener : $('btn-toc');
      target?.focus({ preventScroll: true });
    }
  }).observe(side, { attributes: true, attributeFilter: ['class'] });
  for (const id of ['doc-tabs-bar', 'doc-tabs-secondary-bar']) {
    const bar = $(id);
    if (bar) new MutationObserver(syncChromeDocumentDisclosure).observe(bar, { childList: true, subtree: true, attributes: true, attributeFilter: ['aria-selected'] });
  }
  if ($('file-title')) new MutationObserver(syncChromeDocumentDisclosure).observe($('file-title'), { childList: true, attributes: true, attributeFilter: ['class'] });
  side.addEventListener('keydown', event => {
    if (event.isComposing || event.keyCode === 229 || event.key !== 'Escape' || window.ReadMDModal?.top()) return;
    if (event.defaultPrevented) {
      event.stopPropagation();
      if (event.target.matches('.tab-title-input')) event.composedPath().find(node => node instanceof HTMLElement && node.matches('.tab-item'))?.focus({ preventScroll: true });
      return;
    }
    if (event.target.closest('input,textarea,select,[contenteditable="true"]')) return;
    event.preventDefault(); event.stopPropagation();
    side.classList.add('hidden');
  });
  // These are the same menu buttons and business listeners. Close their menu
  // and expose a visible opener before the existing action opens its dialog.
  const relayMenuAction = event => {
    const action = event.target.closest('[data-shell-menu-action],.more-item');
    if (!action || event.type === 'contextmenu' && action.id !== 'btn-zen') return;
    closeMoreMenu();
    $('btn-more')?.focus({ preventScroll: true });
  };
  for (const type of ['click','contextmenu']) $('more-menu')?.addEventListener(type, relayMenuAction, true);
  // Compact chrome relocates the original controls, including their business
  // listeners. Keep its header out of the desktop menu's keyboard item list.
  const compactGroup = $('shell-compact-group'), compactActions = $('shell-compact-actions');
  if (compactGroup && compactActions) {
    const compact = window.matchMedia('(max-width: 640px)');
    const groupAnchor = document.createComment('compact menu position');
    compactGroup.before(groupAnchor);
    const placements = [
      ['btn-open', 'toolbar.open'], ['btn-palette', 'editor.command'],
      ['btn-search', 'toolbar.search'], ['btn-ai', 'toolbar.ai'], ['btn-graph', 'graph.title'],
    ].map(([id, key]) => {
      const button = $(id);
      if (!button) return null;
      const anchor = document.createComment(id + ' desktop position');
      button.before(anchor);
      const wrapper = document.createElement('div'), label = document.createElement('span');
      wrapper.className = 'shell-menu-action shell-compact-action';
      wrapper.dataset.shellCompact = id;
      label.className = 'shell-menu-label';
      label.dataset.i18n = key;
      label.setAttribute('aria-hidden', 'true');
      wrapper.append(label);
      compactActions.append(wrapper);
      return { button, anchor, wrapper, label, key, role: button.getAttribute('role'),
        menuItem: button.classList.contains('more-item'), menuAction: button.hasAttribute('data-shell-menu-action') };
    }).filter(Boolean);
    const modalOpeners = new WeakMap();
    let pendingFocus = null;
    window.addEventListener('readmd:modal-opened', () => {
      const modal = window.ReadMDModal?.top();
      if (!modal) return;
      const origin = placements.find(({ button }) => button === document.activeElement || button.contains(document.activeElement));
      if (origin) modalOpeners.set(modal, origin.button);
      else modalOpeners.delete(modal);
    });
    const watchMovedOpener = modal => {
      if (pendingFocus) {
        pendingFocus.observer.disconnect();
        if (pendingFocus.frame !== null) cancelAnimationFrame(pendingFocus.frame);
      }
      const pending = { observer: null, frame: null };
      pending.observer = new MutationObserver(() => {
        if (pending.frame !== null) return;
        pending.frame = requestAnimationFrame(() => {
          pending.frame = null;
          if (pendingFocus !== pending || modal.isConnected && !modal.classList.contains('hidden') && getComputedStyle(modal).display !== 'none') return;
          pending.observer.disconnect(); pendingFocus = null;
          // The original manager/caller owns close and any explicit editor
          // focus transfer. Repair only an unusable result after it finishes.
          if (window.ReadMDModal?.top()) return;
          const active = document.activeElement;
          const usable = active && active !== document.body && active.isConnected && !active.disabled &&
            !active.closest('[inert],.hidden,[hidden]') && active.getClientRects().length && getComputedStyle(active).visibility !== 'hidden';
          if (!usable) $('btn-more')?.focus({ preventScroll: true });
        });
      });
      pendingFocus = pending;
      pending.observer.observe(modal, { attributes: true, attributeFilter: ['class'] });
    };
    const labels = () => {
      for (const { button, label, key } of placements) {
        const value = window.i18n?.t(key);
        const text = value && value !== key ? value : button.getAttribute('aria-label') || button.title || '';
        label.textContent = window.ReadMDKeys?.isMac ? text.replace(/\bCtrl\b/g, 'Cmd') : text;
      }
      const heading = compactGroup.querySelector('[data-i18n="shell.group.general"]');
      const value = window.i18n?.t('shell.group.general');
      if (heading && value && value !== 'shell.group.general') heading.textContent = value;
    };
    const visibility = () => {
      for (const { button, wrapper, menuItem } of placements) {
        const hidden = button.hidden || button.classList.contains('hidden') || button.getAttribute('aria-hidden') === 'true';
        if (wrapper.hidden !== hidden) wrapper.hidden = hidden;
        // The original menu's arrow-key selector uses .more-item, not computed
        // visibility. Exclude a service-hidden graph without changing its state.
        const item = menuItem || compact.matches && !hidden;
        if (button.classList.contains('more-item') !== item) button.classList.toggle('more-item', item);
        if (hidden && wrapper.contains(document.activeElement)) $('btn-more')?.focus({ preventScroll: true });
      }
    };
    const project = () => {
      const active = document.activeElement;
      const modal = window.ReadMDModal?.top(), modalOpener = modal && modalOpeners.get(modal);
      const openerParent = modalOpener?.parentNode;
      const movedFocus = placements.find(({ button }) => button === active || button.contains(active));
      const menuFocused = $('more-menu')?.contains(active);
      const menuOpen = $('more-menu')?.classList.contains('open');
      if (menuOpen) closeMoreMenu();
      if (compact.matches) {
        groupAnchor.after(compactGroup);
        compactGroup.hidden = false;
        for (const { button, wrapper } of placements) {
          wrapper.prepend(button);
          button.setAttribute('role', 'menuitem');
          button.setAttribute('data-shell-menu-action', '');
        }
      } else {
        for (const { button, anchor, role, menuAction } of placements) {
          anchor.after(button);
          if (role === null) button.removeAttribute('role');
          else button.setAttribute('role', role);
          if (!menuAction) button.removeAttribute('data-shell-menu-action');
        }
        compactGroup.hidden = true;
        compactGroup.remove();
      }
      labels(); visibility();
      if (modalOpener && modalOpener.parentNode !== openerParent) watchMovedOpener(modal);
      if (!compact.matches && movedFocus && !movedFocus.button.disabled && !movedFocus.button.classList.contains('hidden')) movedFocus.button.focus({ preventScroll: true });
      else if (movedFocus || menuFocused) $('btn-more')?.focus({ preventScroll: true });
    };
    for (const { button } of placements) new MutationObserver(visibility).observe(button, { attributes: true, attributeFilter: ['class', 'hidden', 'aria-hidden'] });
    compact.addEventListener('change', project);
    window.addEventListener('readmd:language-changed', labels);
    project();
  }
  const shortcuts = () => {
    const mac = !!window.ReadMDKeys?.isMac;
    for (const el of [...toolbar.querySelectorAll('.tb-btn[title],.tb-btn[aria-label]'), ...side.querySelectorAll('#side-file-actions .tb-btn[title], #side-close-btn[title]')]) {
      for (const attribute of ['title','aria-label']) {
        const old = el.getAttribute(attribute);
        const next = mac && old ? old.replace(/\bCtrl\b/g, 'Cmd') : old;
        if (old !== next) el.setAttribute(attribute, next);
      }
    }
    const command = $('btn-palette'), hint = $('shell-command-hint');
    if (command && hint) {
      const label = command.getAttribute('aria-label') || window.i18n?.t('editor.command') || '';
      const title = `${label} (${mac ? 'Cmd' : 'Ctrl'}+K / ${mac ? 'Cmd' : 'Ctrl'}+Shift+P)`;
      if (command.hasAttribute('title') && command.title !== title) command.title = title;
      if (hint.textContent !== title) hint.textContent = title;
    }
  };
  if (toolbar) new MutationObserver(shortcuts).observe(toolbar, { subtree: true, attributes: true, attributeFilter: ['title', 'aria-label'] });
  window.addEventListener('readmd:language-changed', () => { shortcuts(); syncChromeDocumentDisclosure(); });
  shortcuts(); syncChromeDocumentDisclosure();
}

function initMacWindowChrome(command) {
  document.body.classList.add('native-macos');
  // Reuse the packaged Mac brand, including its optical inset and rounded tile.
  // The original mark remains the default for browsers and other platforms.
  for (const mark of document.querySelectorAll('.titlebar-mark,.welcome-mark')) {
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
