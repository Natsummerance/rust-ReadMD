'use strict';
// Only a source-bound Rust projection can restore a numeric author font size.
// The Markdown sanitizer remains unchanged; arbitrary style strings are never
// interpreted. The same implementation runs in standalone HTML exports.
window.ReadMDAuthorText = (() => {
  const bindings = new WeakMap();
  const point = value => typeof value === 'number' && Number.isFinite(value) && value > 0 && value <= 4096;
  const blockSelector = 'p,h1,h2,h3,h4,h5,h6,td,th,li,figcaption,caption';
  function restore(node) {
    const saved = bindings.get(node); if (!saved) return;
    if (saved.value) node.style.setProperty('font-size', saved.value, saved.priority);
    else node.style.removeProperty('font-size');
    if (saved.code) {
      if (saved.codeValue) saved.code.style.setProperty('font-size', saved.codeValue, saved.codePriority);
      else saved.code.style.removeProperty('font-size');
    }
    delete node.dataset.readmdAuthorActive; bindings.delete(node);
  }
  function apply(container, model) {
    if (!container?.querySelectorAll) return;
    const valid = model && /^[a-f0-9]{32}$/.test(model.nonce || '') && Array.isArray(model.runs) && model.runs.length <= 32768;
    const rules = new Map();
    if (valid) for (const run of model.runs) {
      if (!run || typeof run.key !== 'string' || !run.key.startsWith(model.nonce + '-') ||
          !/^[a-f0-9]{32}-(?:0|[1-9]\d*)$/.test(run.key) ||
          !(run.size === null || point(run.size)) || !(run.base === null || point(run.base)) ||
          typeof run.complex !== 'boolean' || typeof run.blockBase !== 'boolean' || rules.has(run.key)) continue;
      rules.set(run.key, run);
    }
    const nodes = [...container.querySelectorAll('[data-readmd-author-run],[data-readmd-author-active]')];
    const bases=new Map();
    const counts = new Map();
    for (const node of nodes) counts.set(node.dataset.readmdAuthorRun, (counts.get(node.dataset.readmdAuthorRun) || 0) + 1);
    for (const node of nodes) {
      const run = rules.get(node.dataset.readmdAuthorRun);
      if (!run || counts.get(node.dataset.readmdAuthorRun) !== 1 || node.tagName !== 'SPAN') { restore(node); continue; }
      let value;
      if (run.size !== null) value = run.size + 'pt';
      else if (run.base !== null) value = run.base + 'pt';
      else if (run.blockBase) {
        const block = node.closest(blockSelector);
        // A nested caller may own just a text fragment. It cannot read styles
        // from a parent document outside its supplied rendering surface.
        if (!block || !container.contains(block)) { restore(node); continue; }
        if(!bases.has(block))bases.set(block,Number.parseFloat(getComputedStyle(block).fontSize));
        const px = bases.get(block);
        if (!Number.isFinite(px) || px <= 0 || px > 4096 * 96 / 72) { restore(node); continue; }
        value = px + 'px';
      }
      if (value === undefined) { restore(node); continue; }
      if (!bindings.has(node)) {
        const code=node.children.length===1&&node.firstElementChild?.tagName==='CODE'?node.firstElementChild:null;
        bindings.set(node, { value: node.style.getPropertyValue('font-size'), priority: node.style.getPropertyPriority('font-size'),
          code,codeValue:code?.style.getPropertyValue('font-size'),codePriority:code?.style.getPropertyPriority('font-size') });
      }
      node.style.setProperty('font-size', value); node.dataset.readmdAuthorActive = '';
      // The code element was produced by Rust, not the original HTML. Explicit
      // run sizes must not be shrunk by the reader's generic inline-code rule.
      if (node.children.length === 1 && node.firstElementChild?.tagName === 'CODE') node.firstElementChild.style.setProperty('font-size', 'inherit');
    }
  }
  return { apply };
})();
