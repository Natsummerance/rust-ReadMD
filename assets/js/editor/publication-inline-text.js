'use strict';
// An optional section in the existing local-format dialog, not another toolbar.
window.ReadMDPublicationInlineText = (() => {
  const failures = ['author_inline_text_invalid','author_inline_text_unsupported','author_inline_text_target','author_inline_text_budget','transaction_conflict','transaction_invalid_range'];
  function mount(parent, options) {
    const { source, view, signal, isCurrent, onApplied, t } = options;
    const selection = view.state.selection.main;
    const details = document.createElement('details'), summary = document.createElement('summary');
    summary.textContent = t('pdf.selectedText'); details.append(summary); parent.append(details);
    const content = document.createElement('div'); content.className = 'publication-style-fields'; details.append(content);
    const error = document.createElement('p'); error.className = 'publication-style-error'; error.role = 'alert'; details.append(error);
    let loaded = false, disposed = false, target = null, revision = '', busy = false;
    const controls = [], dirty = new Set();
    const live = () => !disposed && !signal?.aborted && isCurrent();
    function showFailure(data) {
      const code = failures.includes(data?.error_code) ? data.error_code : 'author_inline_text_invalid';
      error.textContent = t('error.' + (code.startsWith('transaction_') ? 'author_inline_text_target' : code));
    }
    async function request(body) {
      const response = await apiFetch('/api/publication/inline-text', { method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify(body), signal });
      const data = await response.json();
      if (!response.ok || data.ok !== true) throw Object.assign(new Error('inline-text'), { data });
      return data;
    }
    const actions = document.createElement('div'); actions.className = 'publication-style-actions'; details.append(actions);
    const reset = document.createElement('button'); reset.type = 'button'; reset.className = 'tb-btn'; reset.textContent = t('publication.local.reset'); reset.disabled = true; actions.append(reset);
    const apply = document.createElement('button'); apply.type = 'button'; apply.className = 'tb-btn'; apply.textContent = t('publication.local.apply'); apply.disabled = true; actions.append(apply);
    function state() { reset.disabled = apply.disabled = busy || !target || !live(); }
    function axis(key, label, value) {
      const wrapper = document.createElement('label'), caption = document.createElement('span'); caption.textContent = t(label); wrapper.append(caption);
      const mode = document.createElement('select'); mode.dataset.inlineAxis = key;
      for (const [code,labelKey] of [['','pm.inherit'],['points','export.fontSize'],['base','publication.inline.blockBase']]) {
        const option = document.createElement('option'); option.value = code; option.textContent = t(labelKey); mode.append(option);
      }
      const input = document.createElement('input'); input.type = 'number'; input.min = '0.5'; input.max = '4096'; input.step = '0.5'; input.setAttribute('aria-label', t(label)); input.dataset.inlinePoints = key;
      mode.value = value === 'base' ? 'base' : typeof value === 'number' ? 'points' : '';
      input.value = String(typeof value === 'number' ? value : 12); input.disabled = mode.value !== 'points';
      mode.onchange = () => { input.disabled = mode.value !== 'points'; dirty.add(key); };
      input.oninput = () => dirty.add(key);
      wrapper.append(mode,input); content.append(wrapper); controls.push({key,mode,input});
    }
    async function commit(action, patch) {
      if (busy) return;
      if (!live() || !target) { showFailure({error_code:'author_inline_text_target'}); return; }
      busy = true; error.textContent = ''; state();
      try {
        const data = await request({action,content:source,base_revision:revision,from:target.from,to:target.to,
          ...(action === 'clear' ? {fields:['size','sizeCs','cs','rtl']} : {patch})});
        if (!live()) { showFailure({error_code:'author_inline_text_target'}); return; }
        if (!data.result || !data.transaction || !Array.isArray(data.transaction.edits)) throw new Error('inline-text-result');
        if (data.result.changed) view.dispatch({changes:data.transaction.edits,userEvent:'input.publication'});
        onApplied();
      } catch (failure) { if (!signal?.aborted && !disposed) showFailure(failure.data); }
      finally { busy = false; state(); }
    }
    apply.onclick = () => {
      const patch = {};
      for (const {key,mode,input} of controls) if (dirty.has(key)) {
        if (mode.value === 'points' && !input.reportValidity()) return;
        patch[key] = mode.value === 'base' ? 'base' : mode.value === 'points' ? Number(input.value) : null;
      }
      if (Object.keys(patch).length) commit('apply', patch);
    };
    reset.onclick = () => commit('clear');
    details.ontoggle = async () => {
      if (!details.open || loaded || disposed) return; loaded = true;
      content.textContent=t('common.loading');content.setAttribute('aria-busy','true');
      try {
        const data = await request({action:'get',content:source});
        content.textContent='';
        if (!live()) { showFailure({error_code:'author_inline_text_target'}); return; }
        revision = data.revision;
        const spans = (data.nodes || []).filter(node => node.kind === 'span' && node.contentFrom <= selection.from && selection.to <= node.contentTo)
          .sort((a,b) => a.to-a.from-(b.to-b.from));
        const owner = spans[0];
        target = selection.from !== selection.to ? {from:selection.from,to:selection.to} : owner ? {from:owner.contentFrom,to:owner.contentTo} : null;
        if (!target || typeof revision !== 'string') { showFailure({error_code:'author_inline_text_target'}); return; }
        axis('size','export.fontSize',owner?.style?.size);
        axis('sizeCs','publication.inline.complexSize',owner?.style?.sizeCs);
        state();
      } catch (failure) { loaded=false;content.textContent='';if (!signal?.aborted && !disposed) showFailure(failure.data); }
      finally{content.setAttribute('aria-busy','false');}
    };
    return {destroy(){disposed = true;details.remove();}};
  }
  return { mount };
})();
