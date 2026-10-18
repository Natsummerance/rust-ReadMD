'use strict';
/* Formula drafts live outside the document until one guarded editor transaction.
 * Source ranges are UTF-16 offsets, matching CodeMirror and DOM selection.
 * MathJax cssId is generated from our token indices, never from user HTML. */
window.ReadMDFormula = (() => {
  let catalog, loading, draft = null, renderEpoch = 0, category = 'structures', pageSize = 72;
  const templatePreviews = new Map();
  const tr = (key, args) => window.i18n ? i18n.t(key, args) : key;
  const byId = id => document.getElementById(id);
  const special = new Set(['left', 'right', 'middle', 'begin', 'end', 'text', 'textrm', 'textbf', 'operatorname', 'color', 'textcolor', 'href', 'class', 'cssId', 'style', 'unicode', 'label', 'tag']);
  const structural = new Set(['frac', 'dfrac', 'tfrac', 'binom', 'sqrt', 'hat', 'widehat', 'vec', 'bar', 'overline', 'underline', 'dot', 'ddot', 'tilde', 'widetilde', 'boxed', 'overbrace', 'underbrace', 'overset', 'underset', 'phantom', 'hphantom', 'vphantom', 'mathrm', 'mathbf', 'mathbb', 'mathcal', 'mathsf', 'mathfrak', 'mathit', 'boldsymbol']);
  function load() {
    if (catalog) return Promise.resolve(catalog);
    if (!loading) loading = fetch('/assets/math-catalog.json').then(r => { if (!r.ok) throw Error('catalog'); return r.json(); }).then(c => {
      if (c.schema !== 1 || !Array.isArray(c.symbols) || !Array.isArray(c.templates)) throw Error('catalog schema');
      catalog = c; return c;
    }).catch(e => { loading = null; throw e; });
    return loading;
  }
  // Bounded, fence/inline-code aware scanner. Never locates a formula by its text:
  // identical occurrences remain distinct, and code examples remain untouched.
  function ranges(src) {
    const out = []; let i = 0, fence = null;
    const frontMatter = /^\uFEFF?---[ \t]*\r?\n[\s\S]*?\r?\n(?:---|\.\.\.)[ \t]*(?:\r?\n|$)/.exec(src);
    if (frontMatter) i = frontMatter[0].length;
    while (i < src.length) {
      if (i === 0 || src[i - 1] === '\n') {
        const m = /^((?:(?: {0,3}>[ \t]?)+[ \t]*| {0,3})(?:(?:[-+*]|\d+[.)])[ \t]+)?)(`{3,}|~{3,})[^\n]*/.exec(src.slice(i));
        if (m) {
          const info=m[0].slice(m[1].length+m[2].length).trim();
          if (!fence && info.split(/\s+/)[0]==='readmd-math') {
            const bodyFrom=i+m[0].length+(src[i+m[0].length]==='\n'?1:0);
            const prefix=m[1].replace(/[^>]/g,' '),from=i+m[1].length;
            const closing=new RegExp('^'+(prefix||' {0,3}')+m[2][0]+'{'+m[2].length+',}[ \\t]*\\r?$','gm');closing.lastIndex=bodyFrom;
            const end=closing.exec(src),bodyTo=end?src.slice(0,end.index).replace(/[\r\n]+$/,'').length:src.length,to=end?end.index+end[0].length:src.length;
            const tex=src.slice(bodyFrom,bodyTo).split('\n').map(line=>prefix&&line.startsWith(prefix)?line.slice(prefix.length):line).join('\n');
            out.push({from,to,bodyFrom,bodyTo,open:src.slice(from,bodyFrom),close:src.slice(bodyTo,to),tex,rawBody:src.slice(bodyFrom,bodyTo),prefix,mode:'block',dialect:'dsl',unsupported:info!=='readmd-math v1'});i=to;continue;
          }
          if (!fence) fence = [m[2][0], m[2].length]; else if (m[2][0] === fence[0] && m[2].length >= fence[1]) fence = null; i += m[0].length; continue;
        }
      }
      if (fence) { i++; continue; }
      if (src.startsWith('<!--', i)) { const end = src.indexOf('-->', i + 4); i = end < 0 ? src.length : end + 3; continue; }
      if (src[i] === '<') {
        const raw = /^<(script|noscript|style|textarea|pre|code|annotation(?:-xml)?|kbd)\b[^>]*>/i.exec(src.slice(i));
        if (raw) { const closing = new RegExp('</' + raw[1] + '\\s*>', 'ig'); closing.lastIndex = i + raw[0].length; const end = closing.exec(src); i = end ? closing.lastIndex : src.length; continue; }
        const tag = /^<\/?[A-Za-z][^\n>]*>/.exec(src.slice(i)); if (tag) { i += tag[0].length; continue; }
      }
      if (src[i] === '`') { const m = /^`+/.exec(src.slice(i))[0]; const end = src.indexOf(m, i + m.length); i = end < 0 ? i + m.length : end + m.length; continue; }
      if(src.startsWith(':math[',i)){
        let end=i+6,depth=1,quoted=false,escaped=false;
        for(;end<src.length;end++){
          const c=src[end];if(escaped){escaped=false;continue;}if(c==='\\'){escaped=true;continue;}if(c==='"'){quoted=!quoted;continue;}
          if(!quoted){if(c==='\n')break;if(c==='[')depth++;if(c===']'&&!--depth)break;}
        }
        if(depth===0){out.push({from:i,to:end+1,bodyFrom:i+6,bodyTo:end,open:':math[',close:']',tex:src.slice(i+6,end),mode:'inline',dialect:'dsl'});i=end+1;continue;}
      }
      let open = '', close = '';
      if (src.startsWith('\\(', i)) { open = '\\('; close = '\\)'; }
      else if (src.startsWith('\\[', i)) { open = '\\['; close = '\\]'; }
      else if (src[i] === '$') { open = src[i + 1] === '$' ? '$$' : '$'; close = open; }
      if (!open) { i += src[i] === '\\' ? Math.min(2, src.length - i) : 1; continue; }
      let end = i + open.length;
      while (end < src.length) {
        if (src.startsWith(close, end)) break;
        if (close === '$' && src[end] === '\n') { end = src.length; break; }
        end += src[end] === '\\' ? Math.min(2, src.length - end) : 1;
      }
      if (end < src.length) {
        const body = src.slice(i + open.length, end);
        // Preserve the reader's currency distinction.
        if (open !== '$' || (typeof looksLikeInlineMath === 'function' ? looksLikeInlineMath(body) : /[\\^_{}+*=]/.test(body) || /^[A-Za-z\u0391-\u03c9]+$/.test(body))) out.push({ from: i, to: end + close.length, bodyFrom: i + open.length, bodyTo: end, open, close, tex: body, mode: open === '$$' || open === '\\[' ? 'block' : 'inline' });
        i = end + close.length;
      } else i += open.length;
      if (out.length >= 4000) break;
    }
    return out;
  }
  function write(range,source){
    if(source===range.tex&&typeof range.rawBody==='string')return range.open+range.rawBody+range.close;
    const previous=(range.rawBody||'').split('\n');
    const body=range.prefix?source.split('\n').map((line,i)=>(previous[i]!==undefined&&!previous[i].startsWith(range.prefix)?'':range.prefix)+line).join('\n'):source;
    return range.open+body+range.close;
  }
  function asDsl(range,source,document,to=range.to){
    if(range.mode!=='block')return ':math['+source+']';
    const eol=document.includes('\r\n')?'\r\n':'\n',prefix=document.slice(0,range.from).split('\n').pop();
    const container=/^(?: {0,3}> ?)*[ \t]*(?:(?:[-+*]|[0-9]+[.)])[ \t]+)?$/.test(prefix);
    if(!container&&prefix.includes('>'))throw Error('math_migration_loss');
    const continuation=container?prefix.replace(/[^>]/g,' '):'';
    let insert='```readmd-math v1'+eol+source+eol+'```';
    if(continuation)insert=insert.split(eol).join(eol+continuation);
    if(!container)insert=eol+eol+insert;
    if(document.slice(to)&&!/^\r?\n/.test(document.slice(to)))insert+=eol+eol;
    return insert;
  }
  function metadataRanges(src) {
    if (!src.includes('<!--readmd:math:')) return [];
    const out = [];
    for (const formula of ranges(src)) {
      const match = /^\s*(<!--readmd:math:v(?:1:[A-Za-z0-9+/=]+|2:[A-Za-z0-9_-]+:[A-Za-z0-9+/=]+)-->)/.exec(src.slice(formula.to));
      if (match) { const from = formula.to + match[0].length - match[1].length; out.push({ from, to: from + match[1].length }); }
    }
    return out;
  }
  function withoutMetadata(src) {
    let out = '', from = 0;
    for (const range of metadataRanges(src)) { out += src.slice(from, range.from); from = range.to; }
    return out + src.slice(from);
  }
  function tokens(src) {
    const result = []; let i = 0, skipDelimiter = false;
    while (i < src.length && result.length < 2000) {
      const from = i; let end = i + 1;
      if (src[i] === '\\') {
        const m = /^\\([A-Za-z]+|.)/.exec(src.slice(i));
        if (!m) { i++; continue; }
        end = i + m[0].length; const name = m[1];
        if (skipDelimiter) { skipDelimiter = false; i = end; continue; }
        if (special.has(name)) {
          if (['left', 'right', 'middle'].includes(name)) { skipDelimiter = true; i = end; continue; }
          // Treat ordinary text as one selectable unit; never inject macros into it.
          let j = end; while (/\s/.test(src[j] || '') && j < src.length) j++;
          if (src[j] === '{') { let depth = 1; j++; while (j < src.length && depth) { if (src[j] === '\\') j += 2; else { if (src[j] === '{') depth++; if (src[j] === '}') depth--; j++; } }
            if (['text', 'textrm', 'textbf', 'operatorname'].includes(name)) result.push({ from, to: j, tex: src.slice(from, j) }); i = j; continue; }
          i = end; continue;
        }
        if (structural.has(name) || name === '\\' || name === ',' || name === ';' || name === '!' || name === 'quad' || name === 'qquad') { i = end; continue; }
      } else {
        const ch = String.fromCodePoint(src.codePointAt(i)); end = i + ch.length;
        if (skipDelimiter && !/\s/.test(ch)) { skipDelimiter = false; i = end; continue; }
        if (/[{}^_&\[\]\s]/.test(ch)) { i = end; continue; }
        if (/[0-9]/.test(ch)) { const m = /^[0-9]+(?:\.[0-9]+)?/.exec(src.slice(i)); end = i + m[0].length; }
      }
      result.push({ from, to: end, tex: src.slice(from, end) }); i = end;
    }
    return result;
  }
  function annotate(src, list, prefix) {
    let out = '', last = 0;
    for (let i = 0; i < list.length; i++) {
      const t = list[i]; out += src.slice(last, t.from) + '{\\cssId{' + prefix + i + '}{' + t.tex + '}}'; last = t.to;
    }
    return out + src.slice(last);
  }
  function select(index) {
    if (!draft || !draft.tokens[index]) return;
    draft.selected = index;
    const token = draft.tokens[index]; const input = byId('formula-token');
    input.disabled = false; input.value = token.tex;
    byId('formula-token-row').hidden = false;
    byId('formula-source').setSelectionRange(token.from, token.to);
    byId('formula-preview').querySelectorAll('[id^="' + draft.prefix + '"]').forEach(el => el.classList.toggle('is-selected', el.id === draft.prefix + index));
    input.focus(); input.select();
  }
  async function renderDraft() {
    if (!draft) return;
    const current = draft, epoch = ++renderEpoch, target = byId('formula-preview');
    current.tokens = current.dialect==='dsl'?[]:tokens(current.tex); current.selected = null;
    byId('formula-source').value = current.tex;
    byId('formula-token-row').hidden = true;
    byId('formula-apply').disabled = true;
    byId('formula-error').textContent = '';
    if (current.tex.length > 16000 || current.tokens.length >= 2000) {
      current.valid = false; target.textContent = ''; byId('formula-error').textContent = tr('formula.tooLarge'); return;
    }
    target.textContent = tr('formula.loading');
    try {
      // Use the existing offline MathJax loader, then render from source rather
      // than HTML. A stale promise never changes another draft or a closed modal.
      if (!window.MathJax?.tex2svgPromise) { target.textContent = '$$x$$'; renderMath(target); }
      if (window.MathJax?.startup?.promise) await MathJax.startup.promise;
      const deadline = Date.now() + 12000;
      while (!window.MathJax?.tex2svgPromise && Date.now() < deadline) await new Promise(r => setTimeout(r, 40));
      if (draft !== current || epoch !== renderEpoch) return;
      if (!window.MathJax?.tex2svgPromise) throw Error('renderer unavailable');
      if(!current.tex.trim()){target.replaceChildren();renderTemplatePreviews();return;}
      const prefix = 'rd-formula-token-' + epoch + '-' + Math.random().toString(36).slice(2, 10) + '-';
      current.prefix = prefix;
      let node;
      if(current.dialect==='dsl'){
        const response=await apiFetch('/api/math/parse',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({source:current.tex,dialect:'dsl',display:byId('formula-mode').value==='block'})});
        const data=await response.json();if(!response.ok||!data.ok)throw Error('invalid');
        if(draft!==current||epoch!==renderEpoch)return;
        current.tokens=data.document.nodes.filter(n=>['r','num','den','e','deg','sub','sup','lim','fName'].includes(n.kind)).sort((a,b)=>a.from-b.from||a.to-b.to).filter((n,i,all)=>!all.slice(0,i).some(p=>p.from===n.from&&p.to===n.to));
        node=await queueMathOperation(()=>MathJax.mathml2svgPromise(data.preview,{display:byId('formula-mode').value==='block'}));
        current.tokens.forEach((token,index)=>{const el=node.querySelector('[id="rd-formula-token-omml-'+token.id.replaceAll('.','-')+'"]');if(el)el.id=prefix+index;});
      }else{
        const annotated = annotate(current.tex, current.tokens, prefix);
        node = await queueMathOperation(() => MathJax.tex2svgPromise(annotated, { display: byId('formula-mode').value === 'block' }));
      }
      if (draft !== current || epoch !== renderEpoch) return;
      target.replaceChildren(node);
      const svg = node.querySelector('svg');
      if (svg) { svg.removeAttribute('aria-hidden'); svg.setAttribute('role', 'group'); svg.setAttribute('aria-label', tr('formula.visualPreview')); }
      node.querySelectorAll('mjx-assistive-mml').forEach(el => el.setAttribute('aria-hidden', 'true'));
      node.querySelectorAll('a').forEach(el => { el.removeAttribute('href'); el.removeAttribute('xlink:href'); });
      const invalid = !!node.querySelector('[data-mml-node="merror"], [data-mjx-error]');
      if (invalid) throw Error('invalid formula');
      current.valid = !!current.tex.trim(); byId('formula-apply').disabled = !current.valid;
      target.querySelectorAll('[id^="' + prefix + '"]').forEach(el => {
        const index = Number(el.id.slice(prefix.length));
        el.setAttribute('role', 'button'); el.setAttribute('tabindex', '0');
        const raw=current.tokens[index]?.tex||'';let label=raw;
        if(current.dialect==='dsl'){try{const value=JSON.parse(raw);if(typeof value==='string')label=value;}catch(_){}}
        el.setAttribute('aria-label', tr('formula.editToken', { token: label }));
        el.addEventListener('click', e => { e.stopPropagation(); select(index); });
        el.addEventListener('keydown', e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); select(index); } });
        // SVG glyphs have holes and unpainted centres. A local transparent
        // hitbox makes the actual symbol bounds clickable without guessing.
        const box = el.getBBox();
        const rect = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
        for (const [key, value] of Object.entries({ x: box.x - 20, y: box.y - 20, width: box.width + 40, height: box.height + 40 })) rect.setAttribute(key, String(value));
        rect.setAttribute('fill', 'transparent'); rect.setAttribute('pointer-events', 'all');
        el.insertBefore(rect, el.firstChild);
      });
      renderTemplatePreviews();
    } catch (_) {
      if (draft !== current || epoch !== renderEpoch) return;
      current.valid = false; byId('formula-apply').disabled = true;
      byId('formula-error').textContent = tr('formula.invalid');
      renderTemplatePreviews();
    }
  }
  async function replaceSelection(tex, sourceInput=true) {
    if (!draft) return;
    const current=draft;
    const token = draft.selected === null ? null : draft.tokens[draft.selected];
    const source = byId('formula-source');
    const from = token ? token.from : source.selectionStart;
    const to = token ? token.to : source.selectionEnd;
    if(current.dialect==='dsl'&&!sourceInput){
      try{
        const response=await apiFetch('/api/math/serialize',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({source:tex,dialect:'latex',target:'dsl',display:false})});
        const result=await response.json();if(!response.ok||!result.ok)throw Error('invalid');
        if(draft!==current)return;tex=result.output;
        // A catalogue item replaces a slot, not the whole Office root.
        const wrapped=/^office\.oMath\(([\s\S]*)\)$/.exec(tex);if(wrapped)tex=wrapped[1];
      }catch(_){if(draft===current)byId('formula-error').textContent=tr('formula.invalid');return;}
    }
    // Empty, unselected drafts accept a template directly. Existing content is
    // replaced only when the user selected it, never just by opening the picker.
    draft.tex = draft.tex.slice(0, from) + tex + draft.tex.slice(to);
    renderDraft(); source.setSelectionRange(from + tex.length, from + tex.length);
  }
  function picker() {
    if (!catalog || !draft) return;
    const catBox = byId('formula-cats'); catBox.replaceChildren();
    for (const cat of ['structures', 'functions', 'greek', 'operators', 'relations', 'arrows', 'letters']) {
      const b = document.createElement('button'); b.type = 'button'; b.textContent = tr('formula.category.' + cat);
      b.classList.toggle('active', category === cat); b.setAttribute('aria-pressed', String(category === cat));
      b.onclick = () => { category = cat; pageSize = 72; picker(); }; catBox.append(b);
    }
    const q = byId('formula-search').value.trim().toLowerCase();
    const all = [...catalog.templates, ...catalog.symbols];
    const rows = all.filter(item => q ? [item.tex, item.symbol, item.id, ...(item.aliases || []), item.label ? tr(item.label, item.labelArgs) : ''].join(' ').toLowerCase().includes(q) : item.category === category);
    const list = byId('formula-list'); list.replaceChildren();
    for (const item of rows.slice(0, pageSize)) {
      const b = document.createElement('button'); b.type = 'button'; b.className = 'formula-item'; b.dataset.mathId = item.id;
      const label = document.createElement('span'), small = document.createElement('small');
      label.textContent = item.label ? tr(item.label, item.labelArgs) : item.symbol; small.textContent = item.tex + (item.symbol ? ' · ' + item.id : '');
      b.append(label, small); b.setAttribute('aria-label', label.textContent + ' ' + item.tex); b.title = item.tex;
      if (item.label) b.dataset.templateTex = item.tex;
      b.onclick = () => replaceSelection(item.tex,false); list.append(b);
    }
    byId('formula-more').hidden = rows.length <= pageSize;
    byId('formula-count').textContent = tr('formula.results', { shown: Math.min(rows.length, pageSize), total: rows.length });
    renderTemplatePreviews();
  }
  async function renderTemplatePreviews() {
    if (!draft || !window.MathJax?.tex2svgPromise) return;
    const current = draft;
    for (const button of byId('formula-list').querySelectorAll('[data-template-tex]')) {
      if (!button.isConnected || button.dataset.previewPending) continue;
      button.dataset.previewPending = 'true';
      try {
        const tex = button.dataset.templateTex;
        let cached = templatePreviews.get(tex);
        if (!cached) {
          cached = await queueMathOperation(() => MathJax.tex2svgPromise(tex, { display: false }));
          if (cached.querySelector('[data-mml-node="merror"]')) continue;
          templatePreviews.set(tex, cached);
        }
        if (draft !== current || !button.isConnected) return;
        const preview = document.createElement('span'); preview.className = 'formula-template-preview'; preview.setAttribute('aria-hidden', 'true');
        const node = cached.cloneNode(true); node.removeAttribute('tabindex');
        // Cached glyph definitions are self-contained; unique IDs avoid DOM
        // collisions when the same catalogue is reopened or filtered.
        const suffix = '-thumb-' + Math.random().toString(36).slice(2, 10);
        node.querySelectorAll('[id]').forEach(el => { const old = el.id; el.id += suffix;
          node.querySelectorAll('[href], [*|href]').forEach(use => {
            for (const attr of Array.from(use.attributes)) if ((attr.localName === 'href') && attr.value === '#' + old) attr.value += suffix;
          });
        });
        preview.append(node); button.querySelector('small').hidden = true; button.append(preview);
      } catch (_) { /* Keep the localized label and source spelling available. */ }
    }
  }
  async function open(mode, range) {
    if (!state.editing || !cmView) return;
    closeMdPopups();
    const view = cmView, doc = view.state.doc.toString(), sel = view.state.selection.main;
    const found = range || ranges(doc).find(r => sel.from >= r.from && sel.to <= r.to
      && (sel.from !== sel.to || sel.from < r.to));
    const selected = doc.slice(sel.from, sel.to);
    draft = { view, tab: getActiveTab()?.id, doc, from: found?.from ?? sel.from, to: found?.to ?? sel.to,
      tex: found ? found.tex : selected, range: found, dialect:found?.dialect||(!found&&!selected.includes('\\')?'dsl':'latex'),tokens: [], selected: null, valid: false };
    renderEpoch++;
    byId('formula-preview').textContent = tr('formula.loading');
    byId('formula-token-row').hidden = true;
    byId('formula-error').textContent = '';
    byId('formula-apply').disabled = true;
    byId('formula-source').value = draft.tex;
    byId('formula-mode').value = found?.mode || mode || 'inline';
    byId('formula-search').value = ''; pageSize = 72;
    byId('formula-modal').classList.remove('hidden');
    byId('formula-source-details').open = false;
    const current = draft;
    try { await load(); if (draft !== current) return; picker(); }
    catch (_) { if (draft === current) byId('formula-error').textContent = tr('formula.catalogError'); return; }
    if (draft !== current) return;
    await renderDraft(); byId('formula-search').focus();
  }
  function close() { draft = null; renderEpoch++; window.ReadMDModal.close('formula-modal'); if (cmView) cmView.focus(); }
  async function pasteExternalFormula(event) {
    if (!draft) return;
    const source = byId('formula-source'), text = event.clipboardData?.getData('text/plain') || '';
    // Import only a complete formula, never reinterpret a fragment of the
    // user's DSL or legacy TeX. Failed/stale imports keep the pasted text.
    if (source.value && (source.selectionStart !== 0 || source.selectionEnd !== source.value.length)) return;
    const input = text.trim();
    const dialect = /^<(?:[A-Za-z_][\w.-]*:)?math(?:\s|>)/.test(input) ? 'mathml'
      : /^\\(?:[dt]?frac|binom|sqrt|begin|text|math(?:bf|rm|it|cal|sf|bb)|operatorname)\s*\{/.test(input) && draft.dialect === 'dsl' ? 'latex'
      : /[■√∑∏∫∬∭∮⁄∕¦▒〖〗]|\\(?:[bBpPvV]?matrix|eqarray|rect|overbar|underbar|overbrace|underbrace)\b|\\sqrt\s*\(|\)\s*\\(?:vec|hat|bar|dot|ddot|tilde)\b/.test(input) ? 'unicodemath' : null;
    if (!dialect) return;
    event.preventDefault();
    const current = draft, epoch = ++renderEpoch;
    source.value = text; current.tex = text; current.valid = false;
    byId('formula-apply').disabled = true;
    byId('formula-preview').textContent = tr('formula.loading');
    byId('formula-error').textContent = '';
    try {
      const response = await apiFetch('/api/math/serialize', {method:'POST', headers:{'Content-Type':'application/json'},
        body:JSON.stringify({source:input, dialect, target:'dsl', display:byId('formula-mode').value === 'block'})});
      const result = await response.json();
      if (draft !== current || renderEpoch !== epoch || source.value !== text) return;
      if (!response.ok || !result.ok) throw Error('invalid imported formula');
      current.dialect = 'dsl'; current.tex = result.output; current.selected = null;
      source.value = result.output; source.setSelectionRange(source.value.length, source.value.length);
      await renderDraft();
    } catch (_) {
      if (draft !== current || renderEpoch !== epoch || source.value !== text) return;
      byId('formula-preview').replaceChildren();
      byId('formula-error').textContent = tr('formula.invalid');
    }
  }
  function apply() {
    if (!draft?.valid) return;
    if (cmView !== draft.view || getActiveTab()?.id !== draft.tab || cmView.state.doc.toString() !== draft.doc) { byId('formula-error').textContent = tr('formula.stale'); return; }
    const mode = byId('formula-mode').value;
    const original = draft.range;
    const insert = original && draft.dialect === 'dsl' && original.dialect !== 'dsl'
      ? asDsl({...original,mode},draft.tex,draft.doc)
      : original && mode === original.mode ? write(original,draft.tex)
      : draft.dialect==='dsl' ? (mode==='block'?'\n```readmd-math v1\n'+draft.tex+'\n```\n':':math['+draft.tex+']')
      : mode === 'block' ? '\n$$\n' + draft.tex + '\n$$\n' : '$' + draft.tex + '$';
    const view = draft.view, from = draft.from, to = draft.to;
    // Close before dispatch: status/render callbacks cannot reuse the old draft.
    close();
    if (view.state.sliceDoc(from, to) === insert) return;
    view.dispatch({ changes: { from, to, insert }, selection: { anchor: from + insert.length }, userEvent: 'input.formula' });
    view.focus();
  }
  function bind() {
    byId('formula-apply').onclick = apply;
    byId('formula-more').onclick = () => { pageSize += 72; picker(); };
    byId('formula-mode').addEventListener('change', renderDraft);
    let timer;
    byId('formula-source').addEventListener('input', () => { if (!draft) return; draft.tex = byId('formula-source').value; draft.valid = false; byId('formula-apply').disabled = true; clearTimeout(timer); timer = setTimeout(renderDraft, 100); });
    byId('formula-source').addEventListener('paste', event => { clearTimeout(timer); void pasteExternalFormula(event); });
    byId('formula-token').addEventListener('keydown', e => { if (e.key === 'Enter') { e.preventDefault(); replaceSelection(e.target.value); } });
    byId('formula-token-replace').onclick = () => replaceSelection(byId('formula-token').value);
    // Reuse the live preview: the selected DOM math item identifies its exact
    // source occurrence. Matching source order disambiguates repeated equations.
    byId('preview-pane').addEventListener('dblclick', e => {
      const container = e.target.closest('mjx-container'); if (!container || !state.editing || !cmView) return;
      if(container.closest('.readmd-dsl-math')){const range=ReadMDInlineMath.locate(container);if(range)open(range.mode,range);return;}
      const items = renderedMathItemsWithin(byId('preview-pane'));
      if (!items.length) return;
      const candidates = ranges(cmView.state.doc.toString()).filter(range=>range.dialect!=='dsl');
      let cursor = 0, range;
      for (const item of items) {
        const index = candidates.findIndex((r, n) => n >= cursor && repairLatex(r.tex).trim() === item.math.trim());
        if (index < 0) continue;
        cursor = index + 1;
        if (item.typesetRoot === container) { range = candidates[index]; break; }
      }
      if (range) open(range.mode, range);
    });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', bind); else bind();
  async function insert(tex) { await open('inline'); if (!draft) return; draft.dialect='latex';draft.tex = tex; await renderDraft(); }
  return { open, close, picker, apply, insert, ranges, write, asDsl, metadataRanges, withoutMetadata, tokens, annotate, load };
})();
