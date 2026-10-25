'use strict';
// A fixed-text interaction illustration. No model, network, storage or timers.
(() => {
  const active = new Map();
  function bind(root) {
    const editor = root.querySelector('[data-ai-editor]');
    const suggestion = root.querySelector('[data-ai-suggestion]');
    const inserted = root.querySelector('[data-ai-inserted]');
    const status = root.querySelector('[data-ai-status]');
    const buttons = Object.fromEntries(['accept', 'dismiss', 'undo', 'reset'].map(action => [action, root.querySelector('[data-ai-action="' + action + '"]')]));
    if (!editor || !suggestion || !inserted || !status || Object.values(buttons).some(button => !button)) return () => {};
    let state = 'pending';
    const listeners = [];
    const listen = (target, type, handler) => {
      target.addEventListener(type, handler);
      listeners.push(() => target.removeEventListener(type, handler));
    };
    const render = next => {
      state = next;
      root.dataset.aiState = state;
      suggestion.hidden = state !== 'pending';
      inserted.textContent = state === 'accepted' ? suggestion.textContent : '';
      inserted.hidden = state !== 'accepted';
      buttons.accept.disabled = buttons.dismiss.disabled = state !== 'pending';
      buttons.undo.disabled = state !== 'accepted';
      buttons.reset.disabled = false;
      status.textContent = status.dataset[state];
    };
    const accept = () => { if (state === 'pending') render('accepted'); };
    const dismiss = () => { if (state === 'pending') render('dismissed'); };
    const undo = () => { if (state === 'accepted') render('restored'); };
    for (const [action, perform] of Object.entries({ accept, dismiss, undo, reset: () => render('pending') })) {
      listen(buttons[action], 'click', () => { perform(); editor.focus({ preventScroll: true }); });
    }
    listen(editor, 'keydown', event => {
      if (event.target !== editor || event.isComposing || event.altKey) return;
      if (event.key === 'Tab' && !event.shiftKey && !event.ctrlKey && !event.metaKey && state === 'pending') {
        event.preventDefault(); accept();
      } else if (event.key === 'Escape' && !event.ctrlKey && !event.metaKey && state === 'pending') {
        event.preventDefault(); dismiss();
      } else if (event.key.toLowerCase() === 'z' && (event.ctrlKey || event.metaKey) && !event.shiftKey && state === 'accepted') {
        event.preventDefault(); undo();
      }
    });
    render('pending');
    root.dataset.aiReady = 'true';
    return () => { for (const remove of listeners) remove(); root.dataset.aiReady = 'false'; };
  }
  function mount() {
    for (const root of document.querySelectorAll('[data-ai-illustration="offline"]')) {
      if (!active.has(root)) active.set(root, bind(root));
    }
  }
  function show(event) { if (event.persisted) mount(); }
  function hide(event) {
    for (const remove of active.values()) remove();
    active.clear();
    if (!event.persisted) {
      window.removeEventListener('pageshow', show);
      window.removeEventListener('pagehide', hide);
    }
  }
  window.addEventListener('pageshow', show);
  window.addEventListener('pagehide', hide);
  mount();
})();
