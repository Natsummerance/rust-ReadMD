'use strict';
/* Generated diagram chrome only. Source stays on the card; opening, copying,
   and closing this panel never edits the Markdown or starts a renderer. */
(function () {
  if (window.ReadMDDiagramTools) return;
  const copyTimers = new WeakMap();
  const preparedFigures = new WeakSet();
  const text = key => window.i18n ? window.i18n.t(key) : key;

  function neutralLevel(value) {
    const colour = String(value || '').trim().toLowerCase();
    if (colour === 'black') return 0;
    if (colour === 'white') return 255;
    const hex = /^#([\da-f]{3}|[\da-f]{6})$/.exec(colour);
    const rgb = hex
      ? (hex[1].length === 3 ? [...hex[1]].map(c => parseInt(c + c, 16)) : [0, 2, 4].map(i => parseInt(hex[1].slice(i, i + 2), 16)))
      : /^rgb\(\s*(\d+)\s*[, ]\s*(\d+)\s*[, ]\s*(\d+)\s*\)$/.exec(colour)?.slice(1).map(Number);
    return rgb && rgb[0] === rgb[1] && rgb[1] === rgb[2] ? rgb[0] : null;
  }

  function prepareFigure(card) {
    if (card.dataset.diagramStyle !== 'research') return;
    const svg = card.querySelector('.diagram-preview > svg');
    if (!svg || preparedFigures.has(svg)) return;
    preparedFigures.add(svg);
    // Keep publication attributes intact. Only screen CSS reads these
    // annotations; print/export retain the original paper and authored hues.
    svg.querySelectorAll('[fill], [stroke]').forEach(node => {
      for (const property of ['fill', 'stroke']) {
        const level = neutralLevel(node.getAttribute(property));
        if (level === null) continue;
        node.setAttribute('data-diagram-neutral-' + property, level === 255 && property === 'fill' ? 'paper' : 'ink');
        node.style.setProperty('--diagram-neutral-' + property, `rgb(${255 - level}, ${255 - level}, ${255 - level})`);
      }
    });
  }

  function prepareAddedFigures(records) {
    const cards = new Set();
    for (const record of records) {
      const owner = record.target.closest?.('.diagram-card');
      if (owner) cards.add(owner);
      for (const node of record.addedNodes) {
        if (node.nodeType !== 1) continue;
        if (node.matches('.diagram-card')) cards.add(node);
        node.querySelectorAll('.diagram-card').forEach(card => cards.add(card));
      }
    }
    cards.forEach(prepareFigure);
  }

  document.querySelectorAll('.diagram-card').forEach(prepareFigure);
  new MutationObserver(prepareAddedFigures).observe(document, { subtree: true, childList: true });

  async function writeClipboard(value) {
    try {
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(value);
        return true;
      }
    } catch (_) { /* WebViews may require the selection-based fallback. */ }
    const active = document.activeElement;
    const selection = window.getSelection();
    const ranges = [];
    if (selection) {
      for (let index = 0; index < selection.rangeCount; index += 1) {
        ranges.push(selection.getRangeAt(index).cloneRange());
      }
    }
    const field = document.createElement('textarea');
    field.value = value;
    field.setAttribute('readonly', '');
    field.setAttribute('aria-hidden', 'true');
    field.style.cssText = 'position:fixed;inset-inline-start:0;top:0;width:1px;height:1px;opacity:0;';
    document.body.appendChild(field);
    try {
      field.select();
      return document.execCommand('copy');
    } catch (_) {
      return false;
    } finally {
      field.remove();
      active?.focus({ preventScroll: true });
      if (selection && ranges.length) {
        selection.removeAllRanges();
        ranges.forEach(range => selection.addRange(range));
      }
    }
  }

  function setCopyState(button, copied) {
    button.classList.toggle('is-copied', copied);
    const label = button.querySelector('.diagram-copy-label');
    if (label) label.textContent = text(copied ? 'reader.codeCopied' : 'reader.codeCopy');
    button.setAttribute('aria-label', text(copied ? 'reader.codeCopied' : 'reader.codeCopyTitle'));
    button.title = text(copied ? 'reader.codeCopied' : 'reader.codeCopyTitle');
  }

  async function copy(button) {
    const card = button.closest('.diagram-card');
    const tools = button.closest('[data-diagram-tools]');
    if (!card || !tools || !card.contains(tools) || button.disabled) return;
    let source;
    try { source = decodeURIComponent(card.dataset.diagramCode || ''); }
    catch (_) { source = tools.querySelector('.diagram-src-wrap code')?.textContent || ''; }
    button.disabled = true;
    let copied = false;
    try { copied = await writeClipboard(source); }
    catch (_) { /* A detached selection or unavailable clipboard can fail. */ }
    finally { button.disabled = false; }
    if (!button.isConnected) return;
    const status = tools.querySelector('.diagram-copy-status');
    if (status) {
      status.setAttribute('role', 'status');
      status.setAttribute('aria-live', 'polite');
      status.textContent = text(copied ? 'reader.codeCopied' : 'toast.copyFailed');
    }
    if (!copied) {
      if (typeof window.showToast === 'function') window.showToast(text('toast.copyFailed'));
      return;
    }
    setCopyState(button, true);
    clearTimeout(copyTimers.get(button));
    copyTimers.set(button, setTimeout(() => {
      if (button.isConnected) {
        setCopyState(button, false);
        if (status) status.textContent = '';
      }
      copyTimers.delete(button);
    }, 1600));
  }

  document.addEventListener('click', event => {
    const button = event.target.closest?.('.diagram-copy-btn');
    if (!button || !button.closest('.diagram-card [data-diagram-tools]')) return;
    event.preventDefault();
    void copy(button);
  });

  document.addEventListener('keydown', event => {
    if (event.key !== 'Escape' || event.defaultPrevented) return;
    const tools = event.target.closest?.('details[data-diagram-tools][open]');
    if (!tools || !tools.closest('.diagram-card')) return;
    event.preventDefault();
    event.stopPropagation();
    tools.open = false;
    tools.querySelector('summary')?.focus({ preventScroll: true });
  });

  window.ReadMDDiagramTools = Object.freeze({ copy, writeClipboard, prepareFigure });
})();
