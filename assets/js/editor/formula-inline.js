'use strict';
// A local draft in the actual document preview. No document mutation occurs
// until commit; source positions, not rendered character text, identify edits.
window.ReadMDInlineMath = (() => {
  let draft = null, epoch = 0, clickTimer;
  let persistence = Promise.resolve(true);
  let retryPersistence = null;
  function enqueuePersistence(operation) {
    persistence=persistence.then(operation).catch(()=>false).then(ok=>{retryPersistence=ok?null:operation;return ok;});
    return persistence;
  }
  const tr = (key, args) => i18n.t(key, args);
  const contextDisplaySource=value=>String(value).replace(/\r\n?/g,'\n');
  const contextInputChanged=d=>d.input.value!==(d.contextInputBaseline??contextDisplaySource(d.tex));
  function contextAuthorSource(d,value){
    const o=d.range.contextSite.occurrence,body=d.doc.slice(o.body_from,o.body_to);
    // Only a real edit chooses an EOL convention. A no-op keeps every original
    // byte, including mixed EOLs and whitespace outside the displayed source.
    const eol=/\r\n|\r|\n/.exec(body)?.[0]||/\r\n|\r|\n/.exec(d.doc)?.[0]||'\n';
    return contextDisplaySource(value).replace(/\n/g,eol);
  }
  function spanInput(d,s){
    if(s.kind==='text')return s.raw;
    if(!d.rich)return d.tex.slice(s.from,s.to);
    if(d.range.dialect==='dsl')return s.sourceKind==='operator'?d.rich.source.slice(s.from,s.to):(d.rich.visualDsl?.[s.node]??d.rich.source.slice(s.from,s.to));
    return s.kind==='r'?s.raw:(d.rich.projections[s.node]??'');
  }
  async function revision(source) {
    const bytes=await crypto.subtle.digest('SHA-256',new TextEncoder().encode(source));
    return Array.from(new Uint8Array(bytes),b=>b.toString(16).padStart(2,'0')).join('');
  }
  function persist(d = draft) {
    if(!d||d.closed||d.applied)return persistence;
    clearTimeout(d.persistTimer);
    const s=d.spans[d.selected],input=d.input.value;
    const changed=d.tex!==d.range.tex||d.richDirty||(s&&input!==spanInput(d,s))||(d.contextOrdinary&&contextInputChanged(d));
    if(!changed)return persistence;
    const payload={schema:1,from:d.range.from,to:d.range.to,original:d.doc.slice(d.range.from,d.range.to),dialect:d.range.dialect||'latex',
      tex:d.tex,model_source:d.rich?.source||'',model_dialect:d.rich?.dialect||'',model_metadata:d.rich?.extensionMetadata||'',model_identity:d.rich?.identity||'',rich_dirty:!!d.richDirty,
      selected:s?{node:s.node||'',from:s.from,to:s.to,role:s.role,kind:s.kind,text_from:s.textFrom,text_to:s.textTo}: {},input,
      input_from:d.input.selectionStart||0,input_to:d.input.selectionEnd||0};
    const signature=JSON.stringify(payload);
    if(d.persistQueued===signature)return persistence;
    d.persistQueued=signature;
    const work=enqueuePersistence(async()=>{
      payload.document_revision=await (d.sourceRevision ||= revision(d.doc));
      const ok=await ReadMDRecovery.saveMathDraft(d.tabRef,payload);
      if(!ok&&d.persistQueued===signature)d.persistQueued=null;
      return ok;
    });
    return work;
  }
  function scheduleDraft(d) {clearTimeout(d.persistTimer);d.persistTimer=setTimeout(()=>persist(d),250);}
  async function drop(d) {
    return enqueuePersistence(async()=>{
      const hash=await(d.sourceRevision ||= revision(d.doc));
      return ReadMDRecovery.removeMathDraft(d.tabRef,hash+':'+d.range.from+':'+d.range.to);
    });
  }
  async function flush(){
    const before=persistence,result=await persist();
    if(!result&&before===persistence&&retryPersistence)return enqueuePersistence(retryPersistence);
    return result;
  }
  async function leave(){if(!draft)return flush();if(await commit({focus:false}))return true;return persist();}
  function recoveryText(p) {
    let source=p.tex;
    if(p.selected&&Number.isInteger(p.selected.from)&&Number.isInteger(p.selected.to))source=source.slice(0,p.selected.from)+p.input+source.slice(p.selected.to);
    const fence='`'.repeat(Math.max(3,...(source.match(/`+/g)||[]).map(s=>s.length+1)));
    const inputFence='`'.repeat(Math.max(3,...(p.input.match(/`+/g)||[]).map(s=>s.length+1)));
    return '# '+tr('editor.formula')+'\n\n'+tr('formula.source')+'\n\n'+fence+(p.dialect==='dsl'?'readmd-math v1':'latex')+'\n'+source+'\n'+fence+'\n\n'+tr('formula.selectedToken')+'\n\n'+inputFence+'text\n'+p.input+'\n'+inputFence+'\n';
  }
  const ns = 'http://www.w3.org/2000/svg';
  async function request(path, body, signal) {
    if(path==='/api/math/parse'||path==='/api/math/patch')body={...body,...mathLayoutMetrics(document.getElementById('preview-pane'),body.display)};
    const r = await apiFetch(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body), signal });
    const data = await r.json();
    if (!r.ok || !data.ok) throw Error(data.error_code || 'math_invalid_source');
    if(body.document_context){
      const c=data.document_context;
      if(!c||Object.keys(c).length!==5||!['id','revision','fingerprint'].every(k=>typeof c[k]==='string'&&c[k])||!['from','to'].every(k=>Number.isSafeInteger(c[k])&&c[k]>=0)||c.from>=c.to)throw Error('transaction_conflict');
    }
    const doc=data.document;
    if(doc){doc.preview=data.preview;doc.previewSupported=data.preview_supported;doc.mathmlWarns=data.mathml_warns;doc.vectorPreview=data.vector_preview;doc.projection=data.latex;doc.projections=data.projections;doc.visualDsl=data.visual_dsl;doc.fontWarnings=data.font_warnings;doc.layoutDiagnostics=data.layout_diagnostics;doc.metadataStatus=data.metadata_status;doc.identity=data.identity;doc.extensionMetadata=data.metadata;doc.extensionPreservation=data.extension_preservation;doc.documentContext=data.document_context;}
    return doc || data;
  }
  async function contextExpressionModel(d,text,signal,live){
    const site=d.range.contextSite,original=site?.occurrence,inventory=site?.record.context.occurrences;
    if(!d.contextOrdinary||!original||site.record.raw!==d.doc||!original.patchable||original.dialect!=='latex'||original.metadata||!text.trim())throw Error('transaction_conflict');
    const candidate=d.doc.slice(0,original.body_from)+text+d.doc.slice(original.body_to);
    const candidateRevision=await revision(candidate);if(!live())throw new DOMException('Superseded formula draft','AbortError');
    const registered=await request('/api/math/context',{content:candidate,expected_revision:candidateRevision},signal);if(!live())throw new DOMException('Superseded formula draft','AbortError');
    const context=registered.context,delta=text.length-(original.body_to-original.body_from);
    if(!context||context.revision!==candidateRevision||typeof context.id!=='string'||typeof context.fingerprint!=='string'||!Array.isArray(context.occurrences)||context.occurrences.length!==inventory.length)throw Error('transaction_conflict');
    for(const [index,before]of inventory.entries()){
      const after=context.occurrences[index],selected=index===original.ordinal,shift=index>original.ordinal?delta:0;
      if(!after||after.ordinal!==index||after.dialect!==before.dialect||after.display!==before.display||after.metadata!==before.metadata||after.patchable!==before.patchable
        ||after.from!==before.from+shift||after.to!==before.to+(selected?delta:shift)||after.body_from!==before.body_from+shift||after.body_to!==(selected?before.body_from+text.length:before.body_to+shift)
        ||typeof after.source!=='string'||!selected&&(after.source!==before.source||after.source_revision!==before.source_revision))throw Error('transaction_conflict');
    }
    const occurrence=context.occurrences[original.ordinal],document_context={id:context.id,revision:context.revision,fingerprint:context.fingerprint,from:occurrence.from,to:occurrence.to};
    // Markdown decides the semantic source (including its newline rules).
    // The registered full-source revision and exact body span prove its owner.
    const sourceRevision=await revision(occurrence.source);if(!live())throw new DOMException('Superseded formula draft','AbortError');
    if(occurrence.source_revision!==sourceRevision)throw Error('transaction_conflict');
    const model=await request('/api/math/parse',{source:occurrence.source,dialect:'latex',display:original.display,document_context},signal);if(!live())throw new DOMException('Superseded formula draft','AbortError');
    if(model.source!==occurrence.source||model.dialect!=='latex'||model.revision!==sourceRevision||Object.keys(document_context).some(k=>model.documentContext?.[k]!==document_context[k]))throw Error('transaction_conflict');
    return model;
  }
  function locate(container) {
    const pane = document.getElementById('preview-pane');
    if(container.closest('.readmd-model-math'))return cmView?window.ReadMDMathContext?.locate(container,cmView.state.doc.toString()):null;
    const dsl=container.closest('.readmd-dsl-math');
    if(dsl&&cmView){
      const index=Array.from(pane.querySelectorAll('.readmd-dsl-math:not([data-readmd-unsupported])')).indexOf(dsl);
      const range=ReadMDFormula.ranges(cmView.state.doc.toString()).filter(r=>r.dialect==='dsl'&&!r.unsupported)[index];
      return range&&range.tex===decodeURIComponent(dsl.dataset.readmdDsl)?range:null;
    }
    container=container.closest('mjx-container')||container;
    const items = renderedMathItemsWithin(pane);
    if (!items.length || !cmView) return null;
    const candidates = ReadMDFormula.ranges(cmView.state.doc.toString()).filter(range=>range.dialect!=='dsl');
    let cursor = 0;
    for (const item of items) {
      const index = candidates.findIndex((r, n) => n >= cursor && repairLatex(r.tex).trim() === item.math.trim());
      if (index < 0) continue;
      cursor = index + 1;
      if (item.typesetRoot === container) return candidates[index];
    }
    return null;
  }
  function current(d) {
    return draft === d && state.editing && cmView === d.view && getActiveTab()?.id === d.tab && d.view.state.doc.toString() === d.doc && d.host.isConnected;
  }
  function cancel(focus = true) {
    const d = draft; if (!d) return;
    if(focus){d.closed=true;drop(d);}else persist(d);
    d.closed=true;clearTimeout(d.persistTimer);
    draft = null; epoch++; d.abort?.abort(); clearInterval(d.watch); clearTimeout(d.typingTimer);
    if (d.host.isConnected) d.host.replaceWith(d.original);
    if (focus && cmView === d.view) d.view.focus();
  }
  // The laminar source spans are annotated by opening/closing events. This
  // preserves nested fractions, superscripts and empty slots without flattening.
  function annotated(tex, spans, prefix) {
    const starts = new Map(), ends = new Map();
    spans.forEach((s, i) => {
      const row = { ...s, i };
      if (!starts.has(s.from)) starts.set(s.from, []);
      if (!ends.has(s.to)) ends.set(s.to, []);
      starts.get(s.from).push(row); ends.get(s.to).push(row);
    });
    let out = '';
    for (let p = 0; p <= tex.length; p++) {
      for (const s of (ends.get(p) || []).filter(s => s.from !== s.to).sort((a, b) => b.from - a.from || b.i - a.i)) out += '}}';
      for (const s of (starts.get(p) || []).sort((a, b) => b.to - a.to || a.i - b.i)) {
        out += '{\\cssId{' + prefix + s.i + '}{';
        if (s.from === s.to) out += '\\square}}';
      }
      if (p < tex.length) out += tex[p];
    }
    return out;
  }
  function select(index) {
    const d = draft; if (!d || !d.spans[index]) return;
    d.selected = index;
    const s = d.spans[index]; d.input.value = spanInput(d,s);
    d.input.disabled = false; d.input.setAttribute('aria-label', label(s));
    d.canvas.querySelectorAll('[data-math-span]').forEach(el => el.classList.toggle('is-selected', Number(el.dataset.mathSpan) === index));
    d.input.focus(); d.input.select();
  }
  async function choose(index){
    const d=draft;if(!d||d.committing||!d.spans[index])return;
    if(index===d.selected){d.input.focus();return;}
    const target={...d.spans[index]},edited=d.spans[d.selected],before=d.tex,rich=d.rich;
    await replace();if(!current(d)||!d.valid)return;
    let next=-1;
    if(rich){
      let id=target.node;
      if(edited?.kind==='r'&&rich!==d.rich){
        // Replacing a run with several objects shifts later sibling paths.
        // Resolve the originally clicked node in the new revision, rather
        // than reusing a glyph index after a structural edit.
        const parent=edited.node.slice(0,edited.node.lastIndexOf('.'));
        const children=root=>ReadMDMathProperties.flatten(root).find(n=>n.id===parent)?.content.filter(p=>p.type==='element').length||0;
        const delta=children(d.rich.root)-children(rich.root),prefix=parent+'.';
        if(id.startsWith(prefix)){
          const path=id.slice(prefix.length).split('.'),at=Number(path[0]),changed=Number(edited.node.slice(parent.length+1));
          if(at>changed){path[0]=String(at+delta);id=prefix+path.join('.');}
        }
      }
      let textFrom=target.textFrom;
      if(target.kind==='text'&&edited?.kind==='r'&&target.run===edited.node&&rich!==d.rich){d.message.textContent=tr('formula.stale');return;}
      if(target.kind==='text'&&edited?.kind==='text'&&target.node===edited.node&&rich!==d.rich&&textFrom>=edited.textTo){
        const currentText=ReadMDMathProperties.flatten(d.rich.root).find(n=>n.id===id),oldText=ReadMDMathProperties.flatten(rich.root).find(n=>n.id===id);
        const length=n=>n?.content.filter(p=>p.type==='text').map(p=>p.value).join('').length||0;
        textFrom+=length(currentText)-length(oldText);
      }
      next=d.spans.findIndex(s=>s.node===id&&(target.kind!=='text'||s.kind==='text'&&s.textFrom===textFrom));
    }else{
      const delta=d.tex.length-before.length;
      const from=target.from+(edited&&target.from>=edited.to?delta:0);
      const to=target.to+(edited&&target.to>=edited.to?delta:0);
      next=d.spans.findIndex(s=>s.from===from&&s.to===to&&s.role===target.role);
    }
    if(next>=0)select(next);
  }
  function label(s) {
    return s.role ? tr('formula.slot.' + s.role) : tr('formula.editToken', { token: s.tex });
  }
  function error(d, key) {
    d.message.textContent = tr(key); d.apply.disabled = true; d.valid = false;
    // A failed local draft must remain editable so the author can correct it.
    if (current(d) && d.selected !== null) d.input.disabled = false;
    scheduleDraft(d);
  }
  function snapshot(d){return {tex:d.tex,rich:d.rich?structuredClone(d.rich):null,richDirty:d.richDirty};}
  async function activateRich(d){
    if(d.rich)return;
    d.rich=d.model;await render(d,0);
  }
  async function richPatch(d,patch){
    if(!current(d)||!d.rich)return;
    if(d.contextOrdinary)throw Error('math_unsupported_extension');
    if(d.patching)throw Error('math_busy');
    d.patching=true;
    const base=d.rich;
    d.valid=false;d.apply.disabled=true;d.input.disabled=true;
    d.host.querySelectorAll('.formula-properties input,.formula-properties select,.formula-properties button').forEach(el=>el.disabled=true);
    const before=snapshot(d);
    try{
      if(d.range.contextSite&&!d.rich.documentContext)throw Error('transaction_conflict');
      const result=await request('/api/math/patch',{source:d.rich.source,dialect:d.rich.dialect,expected_revision:d.rich.revision,metadata:d.rich.extensionMetadata,id:d.rich.identity||d.identity,display:d.range.mode==='block',document_context:d.rich.documentContext,patch});
      if(!current(d)||d.rich!==base)return;
      d.undo.push(before);d.redo.length=0;d.rich=result;d.richDirty=true;d.tex=d.range.dialect==='dsl'||d.contextOrdinary?result.source:result.projection;
      await render(d,d.selected??0);
    }finally{
      d.patching=false;
      if(current(d))d.host.querySelectorAll('.formula-properties input,.formula-properties select,.formula-properties button').forEach(el=>el.disabled=false);
      scheduleDraft(d);
    }
  }
  async function openSource(d){
    if(!current(d)||d.sourceOpening)return;
    d.sourceOpening=true;
    try{
      // The existing studio does not edit opaque carriers. Retain this draft
      // rather than hand it a partial source that could discard extensions.
      const base=d.rich;if(!base||base.extensionMetadata)throw Error('math_unsupported_extension');
      if(d.range.contextSite)throw Error('math_unsupported_extension');
      let source=base.source;
      if(base.dialect!=='dsl'){
        const result=await request('/api/math/serialize',{source:base.source,dialect:base.dialect,target:'dsl',display:d.range.mode==='block',id:base.identity||d.identity,document_context:base.documentContext});
        if(result.metadata)throw Error('math_unsupported_extension');source=result.output;
      }
      if(!current(d)||d.rich!==base)return;
      if(!await persist(d))return;
      if(!current(d)||d.rich!==base)return;
      const initial={source,dialect:'dsl',view:d.view,tab:d.tab,doc:d.doc,from:d.range.from,to:d.range.to,original:d.doc.slice(d.range.from,d.range.to)};
      cancel(false);await ReadMDFormula.open(d.range.mode,d.range,initial);
    }catch(error){if(current(d))d.message.textContent=apiMessage({error_code:error.message},'formula.invalid');}
    finally{d.sourceOpening=false;}
  }
  async function renderRich(d,serial,selectAfter){
    const base=d.rich;
    const node=await renderMathDocument({preview:d.rich.preview,preview_supported:d.rich.previewSupported,mathml_warns:d.rich.mathmlWarns,document:d.rich,vector_preview:d.rich.vectorPreview,font_warnings:d.rich.fontWarnings,layout_diagnostics:d.rich.layoutDiagnostics,extension_preservation:d.rich.extensionPreservation,metadata_status:d.rich.metadataStatus},d.range.mode==='block');
    if(!current(d)||serial!==epoch||base!==d.rich)return;
    if(node.querySelector('[data-mml-node="merror"],[data-mjx-error]'))throw Error('invalid');
    if(d.contextOrdinary){
      d.canvas.replaceChildren(node);d.spans=[];d.selected=null;d.input.hidden=false;d.input.disabled=false;d.input.value=d.tex;d.contextInputBaseline=d.input.value;d.input.setAttribute('aria-label',tr('formula.source'));
      d.valid=true;d.apply.disabled=false;return;
    }
    d.canvas.replaceChildren(node);d.input.hidden=d.rich.previewSupported===false;
    if(d.rich.previewSupported===false){
      d.spans=[];d.selected=null;d.input.value='';d.input.disabled=true;
      d.message.textContent=apiMessage({error_code:'math_unsupported_extension'},'formula.invalid');
      if(!d.rich.extensionMetadata){
        let source=node.querySelector('[data-readmd-math-source-entry]');
        if(!source){source=document.createElement('button');source.type='button';source.dataset.readmdMathSourceEntry='true';source.textContent=tr('formula.source');node.append(source);}
        source.removeAttribute('aria-expanded');source.setAttribute('aria-haspopup','dialog');source.onclick=()=>openSource(d);
      }
      d.valid=true;d.apply.disabled=false;return;
    }
    const svg=node.querySelector('svg');if(svg){svg.removeAttribute('aria-hidden');svg.setAttribute('role','group');svg.setAttribute('aria-label',tr('formula.visualPreview'));}
    node.querySelectorAll('mjx-assistive-mml,.readmd-math-accessible').forEach(el=>el.setAttribute('aria-hidden','true'));
    // Source regions share SVG coordinates, but their DOM hierarchy must
    // reflect containment. A numerator's characters are its descendants, not
    // unrelated overlays that prevent clicking or focusing the whole slot.
    const regions=[...node.querySelectorAll('[data-math-selection-regions] > [data-math-node]')],byId=new Map(regions.map(el=>[el.id,el]));
    for(const el of regions){let id=el.id;while(id.includes('-')){id=id.slice(0,id.lastIndexOf('-'));const parent=byId.get(id);if(parent){parent.append(el);break;}}}
    const all=ReadMDMathProperties.flatten(d.rich.root),roles=new Set(['num','den','e','deg','sub','sup','lim','fName']),sourceNodes=new Map(d.rich.nodes.map(n=>[n.id,n]));
    d.spans=[];
    for(const n of all){const type=ReadMDMathProperties.local(n);if(!ReadMDMathProperties.isMath(n)||(type!=='r'&&!roles.has(type)))continue;
      const el=node.querySelector('[id="rd-formula-token-omml-'+n.id.replaceAll('.','-')+'"]');if(!el)continue;
      const text=n=>n.content.map(p=>p.type==='text'?p.value:p.type==='element'?text(p.value):'').join('');
      const sourceNode=d.range.dialect==='dsl'?sourceNodes.get(n.id):null;
      if(d.range.dialect==='dsl'&&!sourceNode)continue;
      const index=d.spans.length,s={node:n.id,role:type==='r'?null:(['sub','sup','fName'].includes(type)?'e':type),tex:d.rich.projections[n.id]??'',raw:text(n),kind:type,sourceKind:sourceNode?.kind,from:sourceNode?.from,to:sourceNode?.to};d.spans.push(s);
      el.dataset.mathSpan=index;el.setAttribute('role','button');el.setAttribute('tabindex','0');el.setAttribute('aria-label',label(s));
      // A source run can occupy several wrapped lines. Use its individual
      // fragments so one large union rectangle cannot cover other symbols.
      const fragments=[...el.children].filter(n=>n.localName==='rect'&&!n.dataset.mathHit);
      const boxes=fragments.length?fragments.map(n=>n.getBBox()):[el.getBBox()];
      for(const box of boxes){const hit=document.createElementNS(ns,'rect');for(const[k,v]of Object.entries({x:box.x-35,y:box.y-35,width:Math.max(100,box.width+70),height:Math.max(100,box.height+70)}))hit.setAttribute(k,v);
        hit.setAttribute('fill','transparent');hit.setAttribute('pointer-events','all');hit.setAttribute('data-math-hit','true');el.prepend(hit);
      }
      el.onclick=e=>{e.stopPropagation();choose(index);};el.onkeydown=e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();choose(index);}};
      if(type==='r'&&typeof Intl.Segmenter==='function'){
        const texts=n.content.filter(p=>p.type==='element'&&ReadMDMathProperties.isMath(p.value)&&ReadMDMathProperties.local(p.value)==='t').map(p=>p.value);
        // Nonstandard multi-t runs retain their whole-run editor until there
        // is an unambiguous per-text-node mapping. Never guess the first t.
        if(texts.length!==1)continue;
        const raw=text(texts[0]),fragments=[...el.querySelectorAll('[data-math-node="run-text"]')];
        const graphemes=[...new Intl.Segmenter(undefined,{granularity:'grapheme'}).segment(raw)];
        if(graphemes.length<2||!fragments.length)continue;
        const byStart=new Map(fragments.map(e=>[Number(e.dataset.mathTextFrom),e]));
        for(const {segment,index:from} of graphemes){
          const to=from+segment.length,parts=[];
          for(let at=from;at<to;){const part=byStart.get(at);if(part&&Number(part.dataset.mathTextTo)<=to)parts.push(part);at+=raw.codePointAt(at)>0xffff?2:1;}
          if(!parts.length)continue;
          const group=document.createElementNS(ns,'g'),span={node:texts[0].id,kind:'text',role:null,raw:segment,tex:segment,textFrom:from,textTo:to,run:n.id};
          const at=d.spans.length;d.spans.push(span);group.dataset.mathSpan=at;group.dataset.mathTextFrom=from;group.dataset.mathTextTo=to;
          group.setAttribute('role','button');group.setAttribute('tabindex','0');group.setAttribute('aria-label',label(span));
          for(const part of parts){
            for(const rect of [...part.querySelectorAll('rect')]){const hit=rect.cloneNode(false);hit.dataset.mathHit='true';hit.setAttribute('pointer-events','all');group.append(hit);}
            part.remove();
          }
          // Keep a combining sequence or astral character as one input unit.
          // Logical UTF-16 offsets come from Rust; font glyph order is unused.
          el.append(group);group.onclick=e=>{e.stopPropagation();choose(at);};group.onkeydown=e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();e.stopPropagation();choose(at);}};
        }
      }
    }
    d.valid=true;d.apply.disabled=false;
    if(selectAfter!==undefined&&d.spans.length)select(Math.min(selectAfter,d.spans.length-1));
  }
  async function render(d, selectAfter) {
    const serial = ++epoch;
    d.valid = false; d.apply.disabled = true; d.input.disabled = true; d.message.textContent = d.notice ? tr(d.notice) : '';
    d.abort?.abort(); d.abort = new AbortController();
    try {
      if(d.rich){await renderRich(d,serial,selectAfter);return;}
      const model = await request('/api/math/parse', { source: d.tex, dialect: 'latex', display: d.range.mode === 'block' }, d.abort.signal);
      if (!current(d) || serial !== epoch) return;
      d.model = model;
      if(model.previewSupported===false){d.rich=model;await renderRich(d,serial,selectAfter);return;}
      const slots = model.nodes.flatMap(n => n.slots.filter(s => s.role !== 'name').map(s => ({ ...s, node: n.id, tex: d.tex.slice(s.from, s.to) })));
      const leaves = ReadMDFormula.tokens(d.tex).map(s => ({ ...s, role: null }));
      // Exact coincident spans prefer the structural slot. Crossing annotations
      // (e.g. text macros containing prose) are deliberately not instrumented.
      d.spans = [...slots, ...leaves].sort((a,b) => a.from-b.from || b.to-a.to).filter((s,i,all) =>
        !all.slice(0,i).some(a => a.from === s.from && a.to === s.to || a.from < s.from && s.from < a.to && a.to < s.to));
      const prefix = 'rd-formula-token-inline-' + serial + '-';
      const node = await queueMathOperation(() => MathJax.tex2svgPromise(annotated(d.tex,d.spans,prefix), { display: d.range.mode === 'block' }));
      if (!current(d) || serial !== epoch) return;
      if (!d.tex.trim() || node.querySelector('[data-mml-node="merror"], [data-mjx-error]')) throw Error('invalid');
      d.canvas.replaceChildren(node);
      const svg=node.querySelector('svg');
      if(svg){svg.removeAttribute('aria-hidden');svg.setAttribute('role','group');svg.setAttribute('aria-label',tr('formula.visualPreview'));}
      node.querySelectorAll('a').forEach(a => { a.removeAttribute('href'); a.removeAttribute('xlink:href'); });
      node.querySelectorAll('mjx-assistive-mml').forEach(el => el.setAttribute('aria-hidden','true'));
      node.querySelectorAll('[id^="' + prefix + '"]').forEach(el => {
        const index = Number(el.id.slice(prefix.length)), s = d.spans[index];
        el.dataset.mathSpan = index; el.setAttribute('role','button'); el.setAttribute('tabindex','0'); el.setAttribute('aria-label',label(s));
        const box = el.getBBox(), hit = document.createElementNS(ns,'rect');
        hit.setAttribute('data-math-hit','true');
        for (const [k,v] of Object.entries({ x:box.x-35, y:box.y-35, width:Math.max(100,box.width+70), height:Math.max(100,box.height+70) })) hit.setAttribute(k,v);
        hit.setAttribute('fill','transparent'); hit.setAttribute('pointer-events','all'); el.prepend(hit);
        el.addEventListener('click',e => { e.stopPropagation(); choose(index); });
        el.addEventListener('keydown',e => {
          if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); choose(index); }
        });
      });
      d.valid = true; d.apply.disabled = false;
      if (selectAfter !== undefined) select(Math.min(selectAfter,d.spans.length-1));
    } catch (e) { if (current(d) && serial === epoch && e.name !== 'AbortError') error(d,'formula.invalid'); }
  }
  async function replace(next = false) {
    const d = draft; if (!d || d.composing) return;
    if(d.contextOrdinary){d.pending=replaceContextExpression(d);await d.pending;return;}
    if(d.selected===null)return;
    if (d.replacing || d.input.disabled) {
      try{await(d.replacing||d.pending);}catch(_){if(current(d))error(d,'formula.invalid');}
      return;
    }
    clearTimeout(d.typingTimer);
    const s = d.spans[d.selected], index = d.selected;
    const value = d.input.value;
    if(d.rich){
      if(s.kind==='text'){
        if(value!==s.raw){try{d.pending=richPatch(d,{operation:'replace_text_range',node_id:s.node,from:s.textFrom,to:s.textTo,text:value});await d.pending;}catch(_){if(current(d))error(d,'formula.invalid');}}
        else if(next)select((index+1)%d.spans.length);
        return;
      }
      if(d.range.dialect==='dsl'){
        if(value!==spanInput(d,s)){
          const target=ReadMDMathProperties.flatten(d.rich.root).find(n=>n.id===s.node);
          const textNode=s.kind==='r'?target?.content.find(p=>p.type==='element'&&ReadMDMathProperties.local(p.value)==='t')?.value:null;
          const op=s.sourceKind==='operator'||d.rich.visualDsl?.[s.node]===undefined?{operation:'replace_source',node_id:s.node,text:value}:
            textNode&&Array.from(value).length===1?{operation:'replace_text',node_id:textNode.id,text:value}:
            {operation:'replace_dsl_content',node_id:s.node,source:value};
          try{d.pending=richPatch(d,op);await d.pending;}
          catch(_){if(current(d))error(d,'formula.invalid');}
        }else if(next)select((index+1)%d.spans.length);
        return;
      }
      const existing=s.kind==='r'?s.raw:(d.rich.projections[s.node]??'');
      if(value!==existing){
        const base=d.rich;
        d.input.disabled=true;d.apply.disabled=true;d.valid=false;
        d.host.querySelectorAll('.formula-properties input,.formula-properties select,.formula-properties button').forEach(el=>el.disabled=true);
        const task=(async()=>{
          const target=ReadMDMathProperties.flatten(base.root).find(n=>n.id===s.node);
          const textNode=target?.content.find(p=>p.type==='element'&&ReadMDMathProperties.local(p.value)==='t')?.value;
          const catalog=textNode?await ReadMDFormula.load():null;
          if(!current(d)||d.rich!==base)return;
          const symbol=catalog?.symbols.find(s=>s.tex===value)?.symbol;
          const op=textNode&&(!/[\\^_{}+*=\/]/.test(value)||symbol)?{operation:'replace_text',node_id:textNode.id,text:symbol||value}:{operation:'replace_content',node_id:s.node,latex:value};
          await richPatch(d,op);
        })();
        // Include catalog loading in the pending operation. Enter, autosave and
        // Apply must await the same change instead of submitting it twice.
        d.replacing=task;d.pending=task;
        try{await task;}catch(_){if(current(d))error(d,'formula.invalid');}
        finally{
          if(d.replacing===task)d.replacing=null;
          if(current(d))d.host.querySelectorAll('.formula-properties input,.formula-properties select,.formula-properties button').forEach(el=>el.disabled=false);
        }
      }else if(next)select((index+1)%d.spans.length);
      return;
    }
    if (value !== d.tex.slice(s.from,s.to)) {
      const previous={state:snapshot(d),model:d.model,spans:d.spans,selected:d.selected,redo:d.redo.slice()};
      d.undo.push(previous.state); d.redo.length = 0;
      d.tex = d.tex.slice(0,s.from) + value + d.tex.slice(s.to);
      d.pending = render(d,next ? (index+1)%d.spans.length : index);
      const serial=epoch;
      await d.pending;
      if(current(d)&&epoch===serial&&!d.valid){
        // Keep the last valid source spans behind an invalid input. Correcting
        // that input still replaces the original slot, not offsets in bad TeX.
        d.tex=previous.state.tex;d.model=previous.model;d.spans=previous.spans;d.selected=previous.selected;
        d.undo.pop();d.redo=previous.redo;
      }
    } else if (next) select((index+1)%d.spans.length);
  }
  async function replaceContextExpression(d){
    if(!contextInputChanged(d)){await render(d);return;}
    const displayed=d.input.value,text=contextAuthorSource(d,displayed),base=d.rich,serial=++epoch;
    d.abort?.abort();d.abort=new AbortController();d.valid=false;d.apply.disabled=true;d.input.disabled=false;
    const live=()=>current(d)&&epoch===serial&&d.rich===base&&d.input.value===displayed;
    try{
      const model=await contextExpressionModel(d,text,d.abort.signal,live);if(!live())return;
      const node=await renderMathDocument({preview:model.preview,preview_supported:model.previewSupported,mathml_warns:model.mathmlWarns,document:model,vector_preview:model.vectorPreview,font_warnings:model.fontWarnings,layout_diagnostics:model.layoutDiagnostics,extension_preservation:model.extensionPreservation,metadata_status:model.metadataStatus},d.range.mode==='block');
      if(!live())return;if(node.querySelector('[data-mml-node="merror"],[data-mjx-error]'))throw Error('math_invalid_source');
      if(text!==d.tex){d.undo.push(snapshot(d));d.redo.length=0;d.richDirty=true;}
      d.rich=model;d.tex=text;d.contextInputBaseline=displayed;d.canvas.replaceChildren(node);d.spans=[];d.selected=null;d.message.textContent='';d.valid=true;d.apply.disabled=false;
    }catch(e){if(live()&&e.name!=='AbortError'){d.message.textContent=apiMessage({error_code:e.message},'formula.invalid');d.valid=false;d.apply.disabled=true;d.input.disabled=false;}}
    finally{if(current(d)&&epoch===serial)scheduleDraft(d);}
  }
  async function history(redo) {
    const d = draft; if (!d) return;
    if(d.patching||d.committing)return;
    if(d.contextOrdinary){d.abort?.abort();epoch++;}
    const source = redo ? d.redo : d.undo, target = redo ? d.undo : d.redo;
    if (!source.length) return;
    target.push(snapshot(d));const previous=source.pop();d.tex=previous.tex;d.rich=previous.rich;d.richDirty=previous.richDirty;await render(d,0);await d.properties?.refresh();
  }
  async function commit(options = {}) {
    const d = draft; if (!d) return true;
    if(d.committing)return d.committing;
    const task=commitDraft(d, options.focus !== false);d.committing=task;
    try{return await task;}finally{if(d.committing===task)d.committing=null;}
  }
  async function commitDraft(d, focus) {
    if (!current(d)) { error(d,'formula.stale'); return false; }
    // A rejected *previous* slot edit must not block a freshly corrected input.
    // replace() validates the current input again before any body mutation.
    try{await d.pending;}catch(_){}
    await replace();
    if (!current(d) || !d.valid || d.composing) return false;
    let insert = d.range.open + d.tex + d.range.close,to=d.range.to;
    if(d.contextOrdinary){
      if(d.rich.dialect!=='latex'||!d.rich.documentContext){error(d,'formula.stale');return false;}
      insert=d.richDirty?d.range.open+d.tex+d.range.close:d.doc.slice(d.range.from,d.range.to);
    }else if(d.range.dialect==='dsl'){
      insert=ReadMDFormula.write(d.range,d.rich.source);
      if(d.rich.extensionMetadata){
        // Replace this occurrence's body and carrier in one editor change.
        // A stale carrier stays in the source until guarded save records it.
        const gap=d.metadata?d.metadata.slice(0,d.metadata.indexOf('<!--')):(d.range.mode==='block'?'\n':'');
        insert+=gap+d.rich.extensionMetadata;if(d.metadata)to+=d.metadata.length;
      }
    }else if(d.richDirty){
      d.apply.disabled=true;d.input.disabled=true;
      d.host.querySelectorAll('.formula-properties input,.formula-properties select,.formula-properties button').forEach(el=>el.disabled=true);
      try{
        const body={source:d.rich.source,dialect:d.rich.dialect,display:d.range.mode==='block',metadata:d.rich.extensionMetadata,id:d.rich.identity||d.identity,document_context:d.rich.documentContext};
        try{
          const result=await request('/api/math/serialize',{...body,target:'dsl'});
          if(!current(d))return false;
          insert=ReadMDFormula.asDsl(d.range,result.output,d.doc,to+(d.metadata?.length||0));
          if(result.metadata)insert+=(d.range.mode==='block'?'\n':'')+result.metadata;
        }catch(e){
          if(!['math_unsupported_extension','math_invalid_property','math_migration_loss'].includes(e.message))throw e;
          // Imported nonstandard extensions retain their existing lossless
          // adapter until a bound readable representation is available.
          const result=await request('/api/math/serialize',{...body,target:'metadata',latex:d.tex,id:d.rich.identity||d.identity});
          if(!current(d))return false;
          insert+=(d.range.mode==='block'?'\n':'')+result.output;
        }
        if(d.metadata)to+=d.metadata.length;
      }catch(e){
        if(current(d))d.message.textContent=apiMessage({error_code:e.message},'formula.invalid');
        return false;
      }finally{
        if(current(d)){d.apply.disabled=!d.valid;d.input.disabled=false;await d.properties?.refresh();}
      }
    }
    const changed = d.doc.slice(d.range.from,to) !== insert;
    d.applied=true;cancel(false);
    if (changed) d.view.dispatch({ changes:{ from:d.range.from,to,insert }, userEvent:'input.formula' });
    // Applying is a body transaction. Immediate keyboard undo must target it,
    // even while recovery cleanup waits on disk. Late cleanup must not steal
    // focus from the next document or control the user chooses.
    if (focus && cmView===d.view && getActiveTab()?.id===d.tab) d.view.focus();
    await drop(d);
    return true;
  }
  async function open(container, recovered = null, character = null) {
    const model=container.closest('.readmd-model-math');
    if(model)container=model;
    else if(!container.closest('.readmd-dsl-math'))container=container.closest('mjx-container')||container;
    if (!state.editing || !cmView || !window.MathJax?.tex2svgPromise&&container.dataset.readmdPreviewSupported!=='false') return;
    if (draft && !await leave()) return;
    if (!container.isConnected) return;
    const range = locate(container); if (!range) return;
    cancel(false);
    const contextOrdinary=!!range.contextSite&&range.dialect==='latex'&&!range.contextSite.occurrence.metadata;
    const host = document.createElement(range.mode === 'block' ? 'div' : 'span'); host.className = 'formula-inline'; host.contentEditable = 'false'; host.setAttribute('role','group'); host.setAttribute('aria-label',tr('formula.visualPreview'));
    const canvas = document.createElement('span'); canvas.className='formula-inline-canvas';
    const controls = document.createElement('span'); controls.className='formula-inline-controls';
    const input = document.createElement(contextOrdinary?'textarea':'input');if(contextOrdinary){input.rows=3;input.wrap='off';}else input.type='text';input.autocomplete='off'; input.spellcheck=false; input.className='formula-inline-input'; input.disabled=true;
    const apply = document.createElement('button'); apply.type='button'; apply.textContent=tr('formula.apply'); apply.disabled=true;
    const close = document.createElement('button'); close.type='button'; close.textContent=tr('common.cancel');
    const message = document.createElement('span'); message.className='formula-inline-message'; message.setAttribute('role','status');
    controls.append(input,apply,close); host.append(canvas,controls,message);
    const doc=cmView.state.doc.toString(),metadata=/^\s*(<!--readmd:math:v(?:1:[A-Za-z0-9+/=]+|2:[A-Za-z0-9_-]+:[A-Za-z0-9+/=]+)-->)/.exec(doc.slice(range.to));
    const tabRef=getActiveTab();
    const d = { range, original:container,host,canvas,input,apply,message,view:cmView,tab:tabRef?.id,tabRef,doc,tex:range.tex,spans:[],selected:null,undo:[],redo:[],valid:false,composing:false,richDirty:false,contextOrdinary,identity:'eq-'+crypto.randomUUID(),metadata:metadata?.[0] };
    if(d.contextOrdinary){input.id='readmd-inline-source-'+d.identity;input.setAttribute('aria-label',tr('formula.source'));const label=document.createElement('label');label.htmlFor=input.id;label.textContent=tr('formula.source');controls.prepend(label);}
    container.replaceWith(host); draft=d;
    host.addEventListener('focusout', e => {
      // Internal slot/property navigation is still the same local draft.
      // A null target also occurs while rendering disables a focused control.
      if (e.relatedTarget && !host.contains(e.relatedTarget) && !d.composing) {
        commit({ focus:false });
      }
    });
    input.addEventListener('compositionstart',()=>d.composing=true);
    input.addEventListener('compositionend',()=>d.composing=false);
    input.addEventListener('input',()=>{
      clearTimeout(d.typingTimer);
      scheduleDraft(d);
      if(d.composing)return;
      d.typingTimer=setTimeout(async()=>{
        const start=input.selectionStart,end=input.selectionEnd,selected=d.selected;
        await replace();
        if(current(d)&&!input.disabled&&d.selected===selected)input.setSelectionRange(start,end);
      },220);
    });
    // Keep the input focused so pressing Apply cannot lose its click to a blur
    // handler or an intermediate render disabling the button.
    apply.addEventListener('pointerdown',e=>e.preventDefault());
    close.addEventListener('pointerdown',e=>e.preventDefault());
    host.addEventListener('keydown',async e => {
      if(e.isComposing || d.composing) return;
      if(e.key==='Escape') { e.preventDefault();e.stopPropagation();cancel(); }
      else if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='z') { e.preventDefault();e.stopPropagation();await history(e.shiftKey); }
      else if(e.key==='Tab' && e.target===input&&!d.contextOrdinary) { e.preventDefault(); await replace(); if(current(d)&&d.spans.length)select((d.selected+(e.shiftKey?-1:1)+d.spans.length)%d.spans.length); }
      else if(e.key==='Enter' && e.target===input) { if(d.contextOrdinary&&!e.ctrlKey&&!e.metaKey)return;e.preventDefault();e.stopPropagation();if(e.ctrlKey||e.metaKey)await commit();else await replace(); }
    });
    apply.onclick=commit;close.onclick=()=>cancel();
    d.watch=setInterval(()=>{ if(!current(d))cancel(false); },200);
    if(!recovered){
      const hash=await(d.sourceRevision ||= revision(doc));
      const rows=await ReadMDRecovery.mathDrafts(tabRef);
      if(!current(d))return;
      recovered=rows.find(row=>row.payload.document_revision===hash&&row.payload.from===range.from&&row.payload.to===range.to&&row.payload.original===doc.slice(range.from,range.to))?.payload;
    }
    if(recovered){
      d.tex=recovered.tex;d.richDirty=recovered.rich_dirty;
      if(range.contextSite){error(d,'formula.stale');return;}
      if(recovered.model_source){try{d.rich=await request('/api/math/parse',{source:recovered.model_source,dialect:recovered.model_dialect,metadata:recovered.model_metadata||undefined,id:recovered.model_identity||undefined,display:range.mode==='block'});}
        catch(_){if(current(d))error(d,'formula.invalid');return;}}
    }
    if(!d.rich&&range.contextSite){
      try{
        const site=range.contextSite,document_context=await ReadMDMathContext.ensure(site),o=site.occurrence;
        if(!current(d))return;
        d.rich=await request('/api/math/parse',{source:o.source,dialect:o.dialect,metadata:o.metadata||undefined,display:o.display,document_context});
        if(!current(d))return;
      }catch(e){if(current(d))error(d,'formula.invalid');return;}
    }
    if(!d.rich&&range.dialect==='dsl'){
      try{d.rich=await request('/api/math/parse',{source:range.tex,dialect:'dsl',metadata:metadata?.[1],display:range.mode==='block'});
        if(!current(d))return;if(d.rich.metadataStatus==='current')d.identity=d.rich.identity;
        else if(d.rich.metadataStatus==='stale'||d.rich.metadataStatus==='invalid')d.notice='formula.metadata'+(d.rich.metadataStatus==='stale'?'Stale':'Invalid');}
      catch(_){if(current(d)){error(d,'formula.invalid');return;}}
    }else if(!d.rich&&metadata){
      try{const model=await request('/api/math/parse',{source:range.tex,metadata:metadata[1],dialect:'latex',display:range.mode==='block'});
        if(!current(d))return;if(model.metadataStatus==='current'){d.rich=model;d.identity=model.identity;}
        else if(model.metadataStatus==='stale'||model.metadataStatus==='invalid')d.notice='formula.metadata'+(model.metadataStatus==='stale'?'Stale':'Invalid');
      }catch(_){if(current(d))d.notice='formula.metadataInvalid';}
    }
    await render(d,0);
    if(!recovered&&character&&current(d)){
      const index=d.spans.findIndex(s=>s.kind==='text'&&s.run===character.run&&s.textFrom<=character.from&&s.textTo>character.from);
      if(index>=0)select(index);
    }
    if(recovered&&current(d)){
      const index=d.spans.findIndex(s=>recovered.selected.node?s.node===recovered.selected.node&&(recovered.selected.kind!=='text'||s.kind==='text'&&s.textFrom===recovered.selected.text_from&&s.textTo===recovered.selected.text_to):s.from===recovered.selected.from&&s.to===recovered.selected.to);
      if(index>=0){select(index);input.value=recovered.input;input.setSelectionRange(recovered.input_from||0,recovered.input_to||0);await replace();}
      if(current(d)&&d.valid)d.message.textContent=(d.rich?.previewSupported===false?apiMessage({error_code:'math_unsupported_extension'},'formula.invalid')+'\n':'')+tr('storage.draftKept');
    }
    if(current(d)&&!d.contextOrdinary)d.properties=await ReadMDMathProperties.mount(d,()=>activateRich(d),p=>{
      d.pending=richPatch(d,p);return d.pending;
    });
  }
  async function restore() {
    const tab=getActiveTab(),view=cmView;if(!tab||!view||!state.editing)return;
    const rows=await ReadMDRecovery.mathDrafts(tab);if(!rows.length||draft)return;
    const doc=view.state.doc.toString(),hash=await revision(doc);
    const saved=rows.find(row=>row.payload.document_revision===hash&&doc.slice(row.payload.from,row.payload.to)===row.payload.original)?.payload;
    if(!saved){if(getActiveTab()===tab)showToast(tr('formula.stale'));return;}
    if(getActiveTab()!==tab||cmView!==view||view.state.doc.toString()!==doc)return;
    if(state.pvLayout==='none')setPvLayout('right');await renderPreview();
    const deadline=Date.now()+6000;
    while(Date.now()<deadline&&getActiveTab()===tab&&cmView===view&&state.editing&&view.state.doc.toString()===doc&&!draft){
      const containers=$('preview-pane').querySelectorAll('mjx-container,[data-readmd-math-renderer]');
      for(const container of containers){const r=locate(container);if(r?.from===saved.from&&r.to===saved.to){await open(container,saved);return;}}
      await new Promise(resolve=>setTimeout(resolve,50));
    }
  }
  function bind() {
    const pane=document.getElementById('preview-pane');
    pane.addEventListener('click',e=>{
      if(e.target.closest('.formula-inline'))return;
      const container=e.target.closest('mjx-container,[data-readmd-math-renderer]'); if(!container)return;
      const painted=e.target.closest('[data-math-owners]');
      const owner=painted?.getAttribute('data-math-owners').split(' ').find(id=>/-text-\d+$/.test(id));
      let part=owner&&container.querySelector('[id="'+owner+'"]');
      // A glyph's bounding box can contain unpainted space. Hit-test the
      // source boxes too, so clicking inside a character does not require
      // landing on its outline. Regions already use the root SVG coordinates.
      if(!part&&e.clientX!==0&&e.clientY!==0){
        const svg=container.querySelector('svg'),matrix=svg?.getScreenCTM();
        if(matrix){const point=svg.createSVGPoint();point.x=e.clientX;point.y=e.clientY;const local=point.matrixTransform(matrix.inverse());
          part=[...svg.querySelectorAll('[data-math-node="run-text"]')].find(group=>[...group.children].some(rect=>{const b=rect.getBBox();return local.x>=b.x&&local.x<=b.x+b.width&&local.y>=b.y&&local.y<=b.y+b.height;}));
        }
      }
      const character=part?{run:part.id.replace(/-text-\d+$/,'').replace('rd-formula-token-omml-','').replaceAll('-','.'),from:Number(part.dataset.mathTextFrom)}:null;
      // Resizing can replace the painted SVG during the double-click delay.
      // Bind the gesture to the exact source revision/range, then use that
      // formula's current renderer. A changed document must discard it.
      const view=cmView,source=view?.state.doc.toString(),tab=getActiveTab()?.id,range=locate(container);
      clearTimeout(clickTimer);clickTimer=setTimeout(()=>{
        if(!range||cmView!==view||getActiveTab()?.id!==tab||view.state.doc.toString()!==source)return;
        const same=element=>{const at=locate(element);return at?.from===range.from&&at.to===range.to&&at.tex===range.tex&&at.dialect===range.dialect;};
        const target=container.isConnected&&same(container)?container:
          [...pane.querySelectorAll('mjx-container,[data-readmd-math-renderer]')].find(same);
        if(target)open(target,null,character);
      },250);
    });
    pane.addEventListener('dblclick',e=>{
      // LaTeX has a separate double-click studio. Rust-rendered DSL uses
      // this editor for both click speeds; cancelling here made a rapid
      // cancel-and-reopen gesture silently do nothing.
      if(!e.target.closest('[data-readmd-math-renderer]'))clearTimeout(clickTimer);
    });
    document.addEventListener('pointerdown', e => {
      const d = draft;
      if (d && !d.host.contains(e.target) && !d.composing && current(d)) {
        // Flush the last keystroke, await rendering and apply one body change.
        // Invalid drafts remain open; never steal focus back from the target.
        commit({ focus:false });
      }
    }, true);
    document.getElementById('formula-open').addEventListener('click',()=>cancel(false));
  }
  if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',bind);else bind();
  return { open,commit,cancel,locate,annotated,active:()=>!!draft,flush,leave,restore,recoveryText,suspend:()=>cancel(false) };
})();
