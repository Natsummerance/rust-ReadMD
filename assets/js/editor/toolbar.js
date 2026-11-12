'use strict';

function closeMdPopups({ restoreFocus = false } = {}) {
  const bar = $('edit-bar');
  const trigger = bar.querySelector('[data-menu][aria-expanded="true"]');
  bar.querySelectorAll('.md-menu:not(.hidden)').forEach(menu => menu.classList.add('hidden'));
  trigger?.setAttribute('aria-expanded', 'false');
  if (restoreFocus) trigger?.focus({ preventScroll: true });
}

function positionEditorMenu(trigger, menu) {
  const gap = 8;
  const anchor = trigger.getBoundingClientRect();
  const column = $('document-column').getBoundingClientRect();
  const viewport = document.documentElement;
  const left = Math.max(gap, column.left + gap);
  const right = Math.min(viewport.clientWidth - gap, column.right - gap);
  const top = anchor.bottom + gap;
  menu.style.inset = 'auto';
  menu.style.setProperty('--editor-menu-width', Math.max(0, right - left) + 'px');
  menu.style.setProperty('--editor-menu-height', Math.max(0, viewport.clientHeight - top - gap) + 'px');
  const width = menu.getBoundingClientRect().width;
  const rtl = getComputedStyle(trigger).direction === 'rtl';
  const alignRight = (trigger.id === 'edit-view-trigger') !== rtl;
  const wanted = alignRight ? anchor.right - width : anchor.left;
  menu.style.left = Math.max(left, Math.min(wanted, right - width)) + 'px';
  menu.style.top = top + 'px';
}

function editorMenuItems(menu) {
  return [...menu.querySelectorAll('button:not(:disabled), input:not(:disabled)')]
    .filter(control => control.getClientRects().length);
}

function editMenuArrowNav(event) {
  if (event.key === 'Escape') {
    event.preventDefault();
    event.stopPropagation();
    closeMdPopups({ restoreFocus: true });
    return;
  }
  const items = editorMenuItems(event.currentTarget);
  if (!items.length) return;
  const index = items.indexOf(document.activeElement);
  let next;
  switch (event.key) {
    case 'ArrowDown': next = items[(index + 1) % items.length]; break;
    case 'ArrowUp': next = items[(index - 1 + items.length) % items.length]; break;
    case 'Home': next = items[0]; break;
    case 'End': next = items.at(-1); break;
    default: return;
  }
  event.preventDefault();
  next.focus();
}

// Reuse each command and its listeners when placing it in the overflow menu.
function bindEditorToolbarOverflow() {
  const bar = $('edit-bar'), menu = $('edit-overflow-menu'), more = $('edit-overflow-wrap'), trigger = $('edit-overflow-trigger');
  if (!bar || !menu || !more || !trigger || bar.dataset.overflowBound) return;
  bar.dataset.overflowBound = '1';
  menu.addEventListener('click', event => { if (event.target.closest('button')) closeMdPopups(); });
  const candidates = [
    ['[data-menu="md-text-menu"]', 'editor.text'],
    ['#formula-open'],
    ['[data-menu="md-structure-menu"]', 'editor.structure'],
    ['[data-menu="md-insert-menu"]', 'editor.insert'],
    ['#edit-redo'], ['#edit-undo'], ['.md-fmt-group [data-md="link"]'],
    ['.md-fmt-group [data-md="italic"]']
  ].map(([selector, heading]) => {
    const button = bar.querySelector(selector);
    if (!button) return null;
    const popup = heading ? $(button.dataset.menu) : null;
    const original = popup ? button.closest('.md-menu-wrap') : button;
    const anchor = document.createComment('editor toolbar position');
    original.before(anchor);
    const section = document.createElement('div');
    section.className = 'editor-overflow-group';
    const nodes = popup ? [...popup.childNodes] : [button];
    if (popup) {
      const caption = document.createElement('span');
      caption.className = 'editor-overflow-heading';
      caption.dataset.i18n = heading;
      caption.textContent = window.i18n ? i18n.t(heading) : button.textContent;
      section.append(caption);
    } else if (button.classList.contains('icon')) {
      const label = document.createElement('span');
      label.className = 'editor-overflow-label';
      label.dataset.i18n = button.dataset.i18nAria;
      label.textContent = button.getAttribute('aria-label');
      button.append(label);
    }
    return { button, popup, original, anchor, section, nodes, moved:false };
  }).filter(Boolean);
  const restore = candidate => {
    if (!candidate.moved) return;
    if (candidate.popup) {
      candidate.popup.append(...candidate.nodes);
      candidate.original.classList.remove('editor-overflowed');
    } else candidate.anchor.after(candidate.original);
    candidate.section.remove(); candidate.moved = false;
  };
  const move = candidate => {
    if (candidate.popup) candidate.original.classList.add('editor-overflowed');
    candidate.section.append(...candidate.nodes);
    menu.append(candidate.section); candidate.moved = true;
  };
  const required = () => {
    const style = getComputedStyle(bar);
    const children = [...bar.children].filter(el => getComputedStyle(el).display !== 'none');
    return children.reduce((sum, el) => sum + el.getBoundingClientRect().width, 0)
      + Math.max(0, children.length - 1) * (parseFloat(style.columnGap) || 0)
      + (parseFloat(style.paddingLeft) || 0) + (parseFloat(style.paddingRight) || 0);
  };
  let frame = 0;
  const schedule = () => {
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      if (!bar.getBoundingClientRect().width) return;
      const focused = document.activeElement;
      closeMdPopups(); candidates.forEach(restore);
      more.classList.add('hidden');
      const width = bar.clientWidth;
      if (required() > width + .5) {
        more.classList.remove('hidden');
        for (const candidate of candidates) {
          move(candidate);
          if (required() <= width + .5) break;
        }
      }
      if (focused && focused !== document.activeElement && focused.getClientRects().length) focused.focus({ preventScroll:true });
    });
  };
  new ResizeObserver(schedule).observe(bar);
  window.addEventListener('readmd:language-changed', schedule);
  document.fonts?.ready.then(schedule);
  schedule();
}

function bindEditBarExtras() {
  const bar = $('edit-bar');
  if (bar.dataset.controlsBound) return;
  bar.dataset.controlsBound = 'true';
  bar.querySelectorAll('[data-menu]').forEach(trigger => {
    const menu = $(trigger.dataset.menu);
    trigger.addEventListener('click', event => {
      event.stopPropagation();
      const show = menu.classList.contains('hidden');
      closeMdPopups();
      if (!show) return;
      menu.classList.remove('hidden');
      trigger.setAttribute('aria-expanded', 'true');
      positionEditorMenu(trigger, menu);
      if (event.detail === 0) editorMenuItems(menu)[0]?.focus();
    });
    menu.addEventListener('keydown', editMenuArrowNav);
  });
  window.addEventListener('resize', () => {
    const trigger = bar.querySelector('[data-menu][aria-expanded="true"]');
    if (trigger) positionEditorMenu(trigger, $(trigger.dataset.menu));
  });
  const options = [
    ['edit-view-lines', 'lineNumbers'],
    ['edit-view-focus', 'focus'],
    ['edit-view-typewriter', 'typewriter'],
    ['edit-view-ai-completion', 'aiCompletion'],
  ];
  for (const [id, key] of options) {
    $(id).addEventListener('click', () => setEditorPref(key));
  }
  const slashButton = $('edit-slash-btn');
  slashButton.addEventListener('mousedown', event => event.preventDefault());
  slashButton.addEventListener('click', () => {
    closeMdPopups();
    if (cmView) openSlashAtCursor(cmView);
  });
  applyEditorViewClasses();
  bindEditorToolbarOverflow();
}
