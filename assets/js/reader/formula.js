'use strict';
/* ============================================================
   ReadMD Reader - Mathematical Formulas & LaTeX Picker
   ============================================================ */

/* ---------------- 数学公式与 LaTeX 兼容自修复 ---------------- */

function repairLatex(latex) {
  if (!latex) return '';
  let t = latex.trim();

  // 1. HTML 实体还原
  t = t.replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#39;/g, "'");

  // 2. 常见 Unicode 数学符号自动转标准 LaTeX 指令
  const uMap = {
    '×': '\\times ', '÷': '\\div ', '±': '\\pm ', '∓': '\\mp ',
    '≤': '\\le ', '≥': '\\ge ', '≠': '\\ne ', '≈': '\\approx ',
    '≡': '\\equiv ', '∞': '\\infty ', '∑': '\\sum ', '∏': '\\prod ',
    '∫': '\\int ', '√': '\\sqrt', '∈': '\\in ', '∉': '\\notin ',
    '⊂': '\\subset ', '⊆': '\\subseteq ', '∪': '\\cup ', '∩': '\\cap ',
    '∀': '\\forall ', '∃': '\\exists ', '∇': '\\nabla ', '∂': '\\partial ',
    'α': '\\alpha ', 'β': '\\beta ', 'γ': '\\gamma ', 'δ': '\\delta ',
    'ε': '\\varepsilon ', 'θ': '\\theta ', 'λ': '\\lambda ', 'μ': '\\mu ',
    'π': '\\pi ', 'σ': '\\sigma ', 'τ': '\\tau ', 'φ': '\\varphi ',
    'ω': '\\omega ', 'Δ': '\\Delta ', 'Ω': '\\Omega '
  };
  for (const [u, r] of Object.entries(uMap)) {
    if (t.includes(u)) t = t.replaceAll(u, r);
  }

  // 3. 修复在 Markdown 解析中被破坏的 \\ 换行转义（例如在 cases / align / matrix 中）
  t = t.replace(/(\\\s*\n|\s*\\\\(?!\n))\s*/g, ' \\\\ \n');

  // 4. 自动配平未闭合的花括号 {}
  let openBraces = 0;
  let i = 0;
  while (i < t.length) {
    if (t[i] === '\\') { i += 2; continue; }
    if (t[i] === '{') openBraces++;
    else if (t[i] === '}') { if (openBraces > 0) openBraces--; }
    i++;
  }
  if (openBraces > 0) {
    t += '}'.repeat(openBraces);
  }

  return t;
}

function looksLikeInlineMath(body) {
  return /[\\^_{}]/.test(body) || (/[A-Za-z\u0391-\u03C9]/.test(body) && (!/\s/.test(body) || /[=+*/<>-]/.test(body))) || /^[\d\s.()+*/=<>-]+$/.test(body) && /[+*/=<>]/.test(body);
}

// Handles identify an immutable author source, never a client supplied mathPr.
// Prepared/paginated text carries only server-verified nonce/ordinal slots.
window.ReadMDMathContext=(()=>{
  const records=new Map(),scopes=new Map(),tickets=new Map();
  const MAX_BYTES=64*1024*1024,MAX_RECORDS=8;
  let retained=0;
  const abort=()=>new DOMException('Superseded formula context','AbortError');
  const errorMessage=error=>error.message==='transaction_conflict'?i18n.t('formula.stale'):apiMessage({error_code:error.message},'formula.invalid');
  function errorMarkup(raw,error){
    // Keep the author source readable without re-entering any math scanner.
    // Compressed native carriers are hidden; the document itself is untouched.
    const readable=String(raw||'').replace(/<!--readmd:math:[\s\S]*?(?:-->|$)/g,'');
    const literal=escapeHtml(readable).replace(/\$/g,'&#36;').replace(/\\/g,'&#92;').replace(/`/g,'&#96;');
    return'<p class="ai-err" data-readmd-math-context-error="true">'+escapeHtml(errorMessage(error))+'</p><pre class="tex2jax_ignore"><code>'+literal+'</code></pre>';
  }
  const alive=r=>records.get(r.nonce)===r&&scopes.get(r.scope)===r&&r.current();
  const selector=(r,o)=>({id:r.context.id,revision:r.context.revision,fingerprint:r.context.fingerprint,from:o.from,to:o.to});
  function key(site){const{record:r,occurrence:o}=site;return JSON.stringify([r.context.id,r.context.revision,r.context.fingerprint,o.from,o.to,o.source_revision,o.source,o.dialect,o.display,o.metadata]);}
  function discard(r){if(records.get(r.nonce)!==r)return;clearTimeout(r.timer);records.delete(r.nonce);retained-=r.charge;if(scopes.get(r.scope)===r)scopes.delete(r.scope);}
  function reserve(r){
    if(r.charge>MAX_BYTES)throw Error('math_budget');
    for(const old of [...records.values()])if(!alive(old))discard(old);
    while(records.size>=MAX_RECORDS||retained+r.charge>MAX_BYTES){const old=records.values().next().value;if(!old)throw Error('math_budget');discard(old);}
    records.set(r.nonce,r);retained+=r.charge;scopes.set(r.scope,r);
  }
  async function revision(source){const bytes=await crypto.subtle.digest('SHA-256',new TextEncoder().encode(source));return Array.from(new Uint8Array(bytes),b=>b.toString(16).padStart(2,'0')).join('');}
  function validate(value,raw,expected){
    const c=value.context;
    if(!c||typeof c.id!=='string'||!c.id||c.id.length>128||typeof c.nonce!=='string'||!/^[a-f0-9-]{36}$/.test(c.nonce)||c.revision!==expected||typeof c.fingerprint!=='string'||!c.fingerprint||c.fingerprint.length>128||typeof c.active!=='boolean'||!Number.isSafeInteger(c.ttl_ms)||c.ttl_ms<1000||c.ttl_ms>300000||!Array.isArray(c.occurrences)||c.occurrences.length>4096||typeof value.view_content!=='string')throw Error('math_invalid_source');
    let end=0;
    for(const[o,v]of c.occurrences.entries()){
      if(v.ordinal!==o||!Number.isSafeInteger(v.from)||!Number.isSafeInteger(v.to)||v.from<end||v.from>=v.to||v.to>raw.length||!Number.isSafeInteger(v.body_from)||!Number.isSafeInteger(v.body_to)||v.body_from<v.from||v.body_from>v.body_to||v.body_to>v.to||typeof v.source!=='string'||typeof v.source_revision!=='string'||!['latex','dsl'].includes(v.dialect)||typeof v.display!=='boolean'||typeof v.patchable!=='boolean'||v.metadata!==null&&typeof v.metadata!=='string')throw Error('math_invalid_source');
      end=v.to;
    }
    return c;
  }
  async function register(raw,view,current){
    const expected=await revision(raw);if(!current())throw abort();
    const response=await apiFetch('/api/math/context',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({content:raw,expected_revision:expected,view_content:view})});
    const value=await response.json();if(!current())throw abort();
    if(!response.ok||!value.ok)throw Error(value.error_code||'math_invalid_source');
    const context=validate(value,raw,expected);return{context,view:value.view_content};
  }
  function invalidate(r){
    dslMathCache.clear();
    for(const element of document.querySelectorAll('.readmd-model-math'))if(element.dataset.readmdContextSlot?.startsWith(r.nonce+':')){delete element.dataset.readmdReady;delete element.dataset.readmdReadyKey;}
  }
  function schedule(r){
    clearTimeout(r.timer);
    r.timer=setTimeout(()=>{if(!alive(r))return;renew(r,true).then(()=>{if(alive(r))renderDslMath(typeof r.scope==='object'?r.scope:document.getElementById(r.scope));}).catch(error=>{
      if(!alive(r)||error.name==='AbortError')return;
      // Retry is explicit after a network/validation failure; a failed renewal
      // never silently switches this document to standalone rendering.
      for(const element of document.querySelectorAll('.readmd-model-math'))if(element.dataset.readmdContextSlot?.startsWith(r.nonce+':')){
        const note=document.createElement('span');note.textContent=errorMessage(error);element.replaceChildren(note);element.dataset.readmdReady='error';
        const retry=document.createElement('button');retry.type='button';retry.textContent=i18n.t('audit.retryLoad');retry.onclick=()=>{retry.disabled=true;renew(r,true).then(()=>renderDslMath(element)).catch(next=>{if(alive(r)&&next.name!=='AbortError'){note.textContent=errorMessage(next);retry.disabled=false;}});};element.append(retry);
      }
    });},Math.max(1,r.expires-Date.now()));
  }
  async function renew(r,force=false){
    if(!alive(r))throw abort();if(!force&&Date.now()<r.expires)return r;
    if(r.pending)return r.pending;
    r.pending=(async()=>{
      const next=await register(r.raw,r.view,()=>alive(r));
      if(JSON.stringify(next.context.occurrences)!==JSON.stringify(r.context.occurrences)||next.context.fingerprint!==r.context.fingerprint||next.context.active!==r.context.active)throw Error('transaction_conflict');
      r.context=next.context;r.expires=Date.now()+Math.max(1,next.context.ttl_ms-1000);invalidate(r);schedule(r);return r;
    })();
    try{return await r.pending;}finally{r.pending=null;}
  }
  async function prepareSource(raw,view,options={}){
    raw=String(raw||'');view=String(view||'');const current=options.current||(()=>true),scope=options.scope||'content';
    if(!tickets.has(scope)&&tickets.size>=MAX_RECORDS)tickets.delete(tickets.keys().next().value);
    const ticket={};tickets.set(scope,ticket);const live=()=>tickets.get(scope)===ticket&&current();
    const old=scopes.get(scope);if(old)discard(old);
    // Ordinary documents cannot provide Word math defaults. Keep their
    // established MathJax/source-only path and avoid a full-body API upload.
    if(!/readmd-math|<!--readmd:math:/.test(raw)){if(!live())throw abort();return view;}
    const result=await register(raw,view,live);if(!live())throw abort();
    if(!result.context.active)return view;
    const charge=2*(raw.length+view.length+result.view.length+JSON.stringify(result.context.occurrences).length);
    const r={nonce:result.context.nonce,context:result.context,raw,view,scope,current:live,charge,expires:Date.now()+Math.max(1,result.context.ttl_ms-1000),pending:null,timer:null};
    reserve(r);schedule(r);return result.view;
  }
  function slot(element){
    const owner=element?.matches?.('.readmd-model-math')?element:element?.closest?.('.readmd-model-math');if(!owner)return null;
    const match=/^([a-f0-9-]{36}):(\d+)$/.exec(owner.dataset.readmdContextSlot||'');if(!match)return null;
    const r=records.get(match[1]),o=r?.context.occurrences[Number(match[2])];if(!r||!o||!alive(r)||o.ordinal!==Number(match[2]))return null;
    return{record:r,occurrence:o,owner};
  }
  function tokenHtml(nonce,ordinal){
    const r=records.get(nonce),o=r?.context.occurrences[ordinal];
    if(!r||!o||!alive(r))return'<span class="tex2jax_ignore">'+escapeHtml(i18n.t('formula.stale'))+'</span>';
    const tag=o.display?'div':'span';return'<'+tag+' class="readmd-model-math tex2jax_ignore" data-readmd-context-slot="'+nonce+':'+ordinal+'" data-readmd-mode="'+(o.display?'block':'inline')+'" data-readmd-math-renderer="pending"></'+tag+'>';
  }
  function locate(element,raw){
    const site=slot(element);if(!site||site.record.raw!==raw||!site.occurrence.patchable)return null;
    const o=site.occurrence;return{from:o.from,to:o.to,bodyFrom:o.body_from,bodyTo:o.body_to,tex:o.source,dialect:o.dialect,mode:o.display?'block':'inline',open:raw.slice(o.from,o.body_from),close:raw.slice(o.body_to,o.to),contextSite:site};
  }
  async function ensure(site){if(!site||!alive(site.record))throw abort();await renew(site.record);if(!alive(site.record))throw abort();return selector(site.record,site.occurrence);}
  function current(site,expected){return !!site&&alive(site.record)&&(!expected||key(site)===expected);}
  return{prepareSource,slot,tokenHtml,locate,ensure,current,key,errorMarkup,renew:site=>renew(site.record,true)};
})();

function mathFormulaSource(owner){const site=window.ReadMDMathContext.slot(owner);return site?site.occurrence.source:owner.dataset.readmdContextSlot?null:decodeURIComponent(owner.dataset.readmdDsl||'');}
function mathFormulaIdentity(owner){
  const site=window.ReadMDMathContext.slot(owner);
  if(owner.dataset.readmdContextSlot&&!site)return null;
  return site?window.ReadMDMathContext.key(site):JSON.stringify(['standalone',mathFormulaSource(owner),owner.dataset.readmdExtension||'']);
}
const mathCompileResults=new WeakMap();

function protectMath(src) {
  const saved = [];
  const save = m => {
    saved.push(m);
    return '\x01M' + (saved.length - 1) + '\x01';
  };
  src=src.replace(/<!--readmd:math-slot:v1:([a-f0-9-]{36}):(\d+)-->/g,(_,nonce,ordinal)=>save(ReadMDMathContext.tokenHtml(nonce,Number(ordinal))));
  const looksMath = looksLikeInlineMath;

  const envNames = 'cases|align\\*?|aligned|matrix|pmatrix|bmatrix|vmatrix|Vmatrix|equation\\*?|gather\\*?|array|split|smallmatrix';

  // DSL formulas are opaque to Markdown. The source is a data value, never
  // executable HTML, and the Rust compiler supplies their MathML rendering.
  for(const range of (window.ReadMDFormula?.ranges(src)||[]).filter(r=>r.dialect==='dsl').reverse()){
    const tag=range.mode==='block'?'div':'span';
    const unavailable=range.unsupported?' data-readmd-unsupported="true" data-readmd-ready="error" role="note"':'';
    const fallback=range.unsupported?'<small>'+escapeHtml(i18n.t('formula.unsupportedVersion'))+'</small><code>'+escapeHtml(range.tex)+'</code>':'';
    const metadata=/^\s*(<!--readmd:math:v2:[A-Za-z0-9_-]+:[A-Za-z0-9+/=]+-->)/.exec(src.slice(range.to));
    const carrier=metadata?' data-readmd-extension="'+encodeURIComponent(metadata[1]).replaceAll("'",'%27')+'"':'';
    const html='<'+tag+' class="readmd-dsl-math"'+unavailable+carrier+' data-readmd-dsl="'+encodeURIComponent(range.tex).replaceAll("'",'%27')+'" data-readmd-mode="'+range.mode+'">'+fallback+'</'+tag+'>';
    src=src.slice(0,range.from)+save(html)+src.slice(range.to);
  }

  // 0. 优先保护 ```math ... ``` 代码块
  src = src.replace(/```math\b[^\n]*\n([\s\S]+?)```/g, (m, b) => save('$$' + repairLatex(b) + '$$'));

  // 暂时屏蔽非 math 代码块与行内代码，防止代码（如 tikz、python、bash 等）中的 $、\begin 等被误当数学公式提取并损坏
  const codeBlocks = [];
  src = src.replace(/(```[\s\S]*?```|~~~[\s\S]*?~~~|`[^`\n]+`)/g, m => {
    codeBlocks.push(m);
    return '\x02C' + (codeBlocks.length - 1) + '\x02';
  });

  // 1. 优先保护标准多行块级 $$...$$
  src = src.replace(/\$\$([\s\S]+?)\$\$/g, (m, b) => save('$$' + repairLatex(b) + '$$'));

  // 2. 保护 \(...\) 与 \[...\]
  src = src.replace(/\\\(([\s\S]+?)\\\)/g, (m, b) => save('\\(' + repairLatex(b) + '\\)'));
  src = src.replace(/\\\[([\s\S]+?)\\\]/g, (m, b) => save('\\[' + repairLatex(b) + '\\]'));

  // 3. 保护包含 \begin{env} 的 $...$ 公式（可跨行但不可跨段落，优先于裸环境匹配，防止公式拆裂）
  const envInlineRegex = new RegExp('(^|[^\\\\$A-Za-z0-9])\\$((?:[^$\\n\\r]|\\r?\\n(?!\\r?\\n))*?\\\\begin\\{(?:' + envNames + ')\\}[\\s\\S]*?\\\\end\\{(?:' + envNames + ')\\}(?:[^$\\n\\r]|\\r?\\n(?!\\r?\\n))*?)\\$', 'g');
  src = src.replace(envInlineRegex, (m, pre, b) => pre + save('$' + repairLatex(b) + '$'));

  // 4. 保护标准行内 $...$ 公式
  src = src.replace(/(^|[^\\$A-Za-z0-9])\$([^$\n]+?)\$/g, (m, pre, b) => {
    if (looksMath(b)) {
      return pre + save('$' + repairLatex(b) + '$');
    }
    // MathJax scans raw delimiters again after Markdown. Mark rejected dollar
    // pairs as literal, otherwise ordinary currency is still typeset as math.
    return pre + save('<span class="tex2jax_ignore">$</span>') + b + save('<span class="tex2jax_ignore">$</span>');
  });

  // 5. 保护未包裹在 $ 或 $$ 里的独立裸 LaTeX 多行环境
  const envPattern = new RegExp('\\\\begin\\{(' + envNames + ')\\}([\\s\\S]*?)\\\\end\\{\\1\\}', 'g');
  src = src.replace(envPattern, (m, env, body) => save('$$\\begin{' + env + '}' + repairLatex(body) + '\\end{' + env + '}$$'));

  // 还原被屏蔽的非 math 代码块与行内代码
  if (codeBlocks.length > 0) {
    src = src.replace(/\x02C(\d+)\x02/g, (m, i) => (codeBlocks[+i] !== undefined ? codeBlocks[+i] : m));
  }

  return { src, saved };
}

function restoreMath(html, saved) {
  if (!html || !saved || saved.length === 0) return html;
  let prev = '';
  while (html !== prev && (/\x01M(\d+)\x01/.test(html) || /%01M(\d+)%01/.test(html))) {
    prev = html;
    html = html.replace(/\x01M(\d+)\x01/g, (m, i) => (saved[+i] !== undefined ? saved[+i] : m));
    html = html.replace(/%01M(\d+)%01/g, (m, i) => (saved[+i] !== undefined ? encodeURIComponent(saved[+i]) : m));
  }
  return html;
}

let mathObserver = null;
let mathReadyPromise = null;
let mathTypesetQueue = Promise.resolve();
const dslMathCache = new Map();
let mathFontEpoch=0;
for(const type of ['readmd:capability-installed','readmd:capability-removed'])window.addEventListener(type,event=>{
  if(event.detail?.id!=='fonts')return;
  mathFontEpoch++;dslMathCache.clear();
  window.ReadMDMathMetadata?.invalidateFonts();
  for(const id of ['content','preview-pane']){const root=document.getElementById(id);if(!root)continue;
    root.querySelectorAll('.readmd-dsl-math:not([data-readmd-unsupported]),.readmd-model-math').forEach(e=>delete e.dataset.readmdReady);
    renderDslMath(root);
  }
});
const filteredMathInputs = new WeakSet();
const dslMathRequests = new Map(), dslMathObservers = new WeakMap();
let dslMathWorkers = 0;
const dslMathWaiters = [];
function mathLayoutMetrics(owner,display){
  const font_size=Math.max(4,Math.min(512,(parseFloat(getComputedStyle(owner).fontSize)||16)*.75));
  if(!display)return{font_size};
  const style=getComputedStyle(owner),width=owner.clientWidth-(parseFloat(style.paddingLeft)||0)-(parseFloat(style.paddingRight)||0);
  // clientWidth excludes borders and the vertical scrollbar, but includes
  // padding. Keep the content box in untransformed CSS units, then convert
  // CSS px to physical points just as for font_size (1px = 0.75pt).
  return width>0?{font_size,layout_width:Math.max(1,Math.min(20000,Math.round(width*.75*2)/2))}:{font_size};
}
const mathLayoutRoots=new Map();
function observeMathLayout(body){
  if(!window.ResizeObserver)return;
  for(const[root,entry]of mathLayoutRoots)if(!root.isConnected){entry.observer.disconnect();entry.settingsObserver.disconnect();clearTimeout(entry.timer);mathLayoutRoots.delete(root);}
  const root=body.matches('.markdown-body')?body:body.querySelector('.markdown-body')||body.closest('.markdown-body,#content,#preview-pane')||body;if(mathLayoutRoots.has(root))return;
  const entry={key:JSON.stringify(mathLayoutMetrics(root,true)),timer:null,observer:null,settingsObserver:null};
  const refresh=()=>{
    if(!root.isConnected){entry.observer.disconnect();entry.settingsObserver.disconnect();clearTimeout(entry.timer);mathLayoutRoots.delete(root);return;}
    const key=JSON.stringify(mathLayoutMetrics(root,true));if(key===entry.key)return;entry.key=key;
    clearTimeout(entry.timer);entry.timer=setTimeout(()=>{
      if(!root.isConnected)return;
      root.querySelectorAll('.readmd-dsl-math[data-readmd-ready]:not([data-readmd-unsupported]),.readmd-model-math[data-readmd-ready]').forEach(e=>delete e.dataset.readmdReady);
      renderDslMath(root);window.ReadMDMathMetadata?.schedule();
    },80);
  };
  entry.observer=new ResizeObserver(refresh);entry.observer.observe(root);
  // A scroll pane can retain exactly the same box when its inherited font
  // changes. ResizeObserver alone cannot detect that settings transition.
  entry.settingsObserver=new MutationObserver(refresh);
  entry.settingsObserver.observe(document.body,{attributes:true,attributeFilter:['style','class']});
  if(root!==document.body)entry.settingsObserver.observe(root,{attributes:true,attributeFilter:['style','class']});
  mathLayoutRoots.set(root,entry);
}
async function compileDslMath(text, display, owner, retriedContext=false) {
  const contextSite=window.ReadMDMathContext.slot(owner);
  if(owner.dataset.readmdContextSlot&&!contextSite)throw new DOMException('Superseded formula context','AbortError');
  const document_context=contextSite?await ReadMDMathContext.ensure(contextSite):undefined;
  const identity=mathFormulaIdentity(owner),metrics=mathLayoutMetrics(owner,display),fontEpoch=mathFontEpoch;
  const dialect=contextSite?contextSite.occurrence.dialect:'dsl';
  const metadata=contextSite?(contextSite.occurrence.metadata||undefined):owner.dataset.readmdExtension?decodeURIComponent(owner.dataset.readmdExtension):undefined;
  if(!identity||mathFormulaSource(owner)!==text||contextSite&&contextSite.occurrence.display!==display)throw new DOMException('Superseded formula render','AbortError');
  const layoutKey=JSON.stringify(metrics),key=fontEpoch+'\0'+(display?'b':'i')+'\0'+layoutKey+'\0'+identity;
  const valid=element=>element?.isConnected&&mathFormulaSource(element)===text&&mathFormulaIdentity(element)===identity&&fontEpoch===mathFontEpoch&&JSON.stringify(mathLayoutMetrics(element,display))===layoutKey;
  if(dslMathCache.has(key)&&valid(owner))return dslMathCache.get(key);
  let entry=dslMathRequests.get(key);
  if(entry)entry.owners.add(owner);
  else{
    entry={owners:new Set([owner]),promise:null};dslMathRequests.set(key,entry);
    entry.promise=(async()=>{
      if(dslMathWorkers>=4)await new Promise(resolve=>dslMathWaiters.push(resolve));else dslMathWorkers++;
      try{
        if(![...entry.owners].some(valid))throw new DOMException('Superseded formula render','AbortError');
        const response=await apiFetch('/api/math/parse',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({source:text,dialect,metadata,display,document_context,...metrics})});
        const value=await response.json();
        if(![...entry.owners].some(valid))throw new DOMException('Superseded formula render','AbortError');
        if(!response.ok||!value.ok)throw Error(value.error_code||'math_invalid_source');
        if(document_context&&(!value.document_context||Object.keys(value.document_context).length!==5||Object.keys(document_context).some(k=>value.document_context[k]!==document_context[k])))throw Error('transaction_conflict');
        mathCompileResults.set(value,{identity,layoutKey,fontEpoch});
        if(dslMathCache.size>=32)dslMathCache.delete(dslMathCache.keys().next().value);
        dslMathCache.set(key,value);return value;
      }finally{const next=dslMathWaiters.shift();if(next)next();else dslMathWorkers--;}
    })();
  }
  try{return await entry.promise;}
  catch(error){
    if(!retriedContext&&contextSite&&error.message==='transaction_conflict'&&window.ReadMDMathContext.current(contextSite)){
      await ReadMDMathContext.renew(contextSite);return compileDslMath(text,display,owner,true);
    }
    throw error;
  }finally{if(dslMathRequests.get(key)===entry)dslMathRequests.delete(key);}
}
// Layout diagnostics are view state, never author text or HTML fragments.
const mathLayoutDiagnosticCodes=new Set([
  'math_equation_array_invalid_geometry','math_equation_array_literal_alignment',
  'math_equation_array_manual_alignment_scope','math_equation_array_marker_budget',
  'math_equation_array_mathml_distribution','math_equation_array_mixed_alignment',
  'math_equation_array_overflow','math_equation_array_paragraph_alignment_overflow',
  'math_equation_array_single_column_distribution','math_equation_array_width_unavailable',
]);
const mathDocumentWarningState=new WeakMap();
let mathWarningLanguageEpoch=0;
function mathLayoutDiagnosticMessage(issue){
  if(!issue||!mathLayoutDiagnosticCodes.has(issue.code))return i18n.t('formula.invalid');
  const params={};
  if(issue.code==='math_equation_array_overflow'||issue.code==='math_equation_array_paragraph_alignment_overflow'){
    for(const name of ['required_points','available_points']){
      const value=issue[name];
      if(typeof value!=='number'||!Number.isFinite(value)||value<0||value>1e12)return i18n.t('formula.invalid');
      try{params[name]=value.toLocaleString(i18n.currentLang,{maximumFractionDigits:3});}
      catch(_){params[name]=String(Math.round(value*1000)/1000);}
    }
  }
  return i18n.t('warn.'+issue.code,params);
}
function applyMathDocumentWarnings(node,value){
  const fonts=Array.isArray(value.font_warnings)?value.font_warnings:[];
  const layout=Array.isArray(value.layout_diagnostics)?value.layout_diagnostics:[];
  const sourceOnly=value.preview_supported===false;
  const extension=value.extension_preservation?.editable===false;
  const metadataStatus=value.metadata_status;
  const previous=mathDocumentWarningState.get(node);
  const baseTitle=previous?previous.baseTitle:node.title;
  // Restore only attributes and titles owned by this projection. A refresh
  // must neither retain stale diagnostics nor remove another SVG title.
  for(const entry of previous?.owners||[]){
    if(entry.owner.getAttribute('aria-description')===entry.text){
      if(entry.description===null)entry.owner.removeAttribute('aria-description');
      else entry.owner.setAttribute('aria-description',entry.description);
    }
    entry.title.remove();
  }
  const state={font_warnings:fonts,layout_diagnostics:layout,preview_supported:value.preview_supported,mathml_warns:value.mathml_warns,extension_preservation:value.extension_preservation,metadata_status:metadataStatus,baseTitle,owners:[]};
  mathDocumentWarningState.set(node,state);
  if(!fonts.length&&!layout.length&&!sourceOnly&&!extension&&!['stale','invalid'].includes(metadataStatus)){delete node.dataset.readmdMathWarnings;node.title=baseTitle;return;}
  node.dataset.readmdMathWarnings='true';
  const messages=fonts.map(w=>i18n.t('warn.'+(w.code==='glyph_missing'?'math_glyph_missing':w.code),w));
  if(sourceOnly){
    const message=apiMessage({error_code:'math_unsupported_extension'},'formula.invalid');messages.push(message);
    const unavailable=node.querySelector('[data-readmd-math-unavailable]');if(unavailable)unavailable.textContent=message;
    const source=node.querySelector('[data-readmd-math-source-entry]');if(source)source.textContent=i18n.t('formula.source');
  }
  if(extension)messages.push(i18n.t('formula.extensionPreserved'));
  if(metadataStatus==='stale'||metadataStatus==='invalid')messages.push(i18n.t('formula.metadata'+(metadataStatus==='stale'?'Stale':'Invalid')));
  const owners=layout.length?new Map([...node.querySelectorAll('[data-math-node]')].map(el=>[el.id,el])):new Map();
  const ownerMessages=new Map();
  for(const issue of layout){
    const text=mathLayoutDiagnosticMessage(issue);messages.push(text);
    const owner=typeof issue?.node_id==='string'?owners.get(issue.node_id):null;
    if(owner){
      if(!ownerMessages.has(owner))ownerMessages.set(owner,[]);
      ownerMessages.get(owner).push(text);
    }
  }
  for(const [owner,texts] of ownerMessages){
    const text=[...new Set(texts)].join('\n'),description=owner.getAttribute('aria-description');
    const title=document.createElementNS('http://www.w3.org/2000/svg','title');
    title.setAttribute('data-readmd-layout-warning-title','true');title.textContent=text;owner.append(title);
    owner.setAttribute('aria-description',text);state.owners.push({owner,text,description,title});
  }
  node.title=[...new Set(messages)].join('\n');
}
window.addEventListener('readmd:language-changed',()=>{
  const epoch=++mathWarningLanguageEpoch;
  const nodes=[...document.querySelectorAll('[data-readmd-math-warnings]')];let index=0;
  function refresh(){
    if(epoch!==mathWarningLanguageEpoch)return;
    for(let count=0;index<nodes.length&&count<16;index++,count++){
      const node=nodes[index],value=mathDocumentWarningState.get(node);
      if(value&&node.isConnected)applyMathDocumentWarnings(node,value);
    }
    if(index<nodes.length)requestAnimationFrame(refresh);
  }
  requestAnimationFrame(refresh);
});
async function renderMathDocument(value,display){
  if(value.preview_supported===false){
    const wrapper=document.createElement('span');wrapper.className='readmd-math-render';wrapper.dataset.readmdMathRenderer='source';wrapper.dataset.readmdPreviewSupported='false';wrapper.setAttribute('role','note');
    const message=document.createElement('span');message.dataset.readmdMathUnavailable='true';wrapper.append(message);
    if(value.document?.dialect==='dsl'&&typeof value.document.source==='string'){
      const source=document.createElement('code');source.textContent=value.document.source;source.hidden=true;
      const button=document.createElement('button');button.type='button';button.dataset.readmdMathSourceEntry='true';button.setAttribute('aria-expanded','false');
      button.onclick=()=>{source.hidden=!source.hidden;button.setAttribute('aria-expanded',String(!source.hidden));};
      wrapper.append(button,source);
    }
    applyMathDocumentWarnings(wrapper,value);return wrapper;
  }
  if(value.vector_preview){
    const parsed=new DOMParser().parseFromString(value.vector_preview,'image/svg+xml');
    if(parsed.querySelector('parsererror')||parsed.documentElement.localName!=='svg')throw Error('invalid');
    const wrapper=document.createElement('span');wrapper.className='readmd-math-render';wrapper.dataset.readmdMathRenderer='rust';
    if(display)wrapper.dataset.display='true';
    const alignment=parsed.documentElement.getAttribute('data-math-justification');
    if(display&&['left','right','center','centerGroup'].includes(alignment)){
      wrapper.style.display='block';wrapper.style.textAlign=alignment==='centerGroup'?'center':alignment;
    }
    wrapper.append(document.importNode(parsed.documentElement,true));
    // The visible paths are independent of screen-reader mathematics. Keep
    // original MathML available, and exclude it from MathJax's DOM finder.
    const math=new DOMParser().parseFromString(value.preview,'application/xml');
    if(!math.querySelector('parsererror')){
      const accessible=document.createElement('span');accessible.className='readmd-math-accessible';
      accessible.append(document.importNode(math.documentElement,true));wrapper.append(accessible);
    }
    applyMathDocumentWarnings(wrapper,value);return wrapper;
  }
  const node=await queueMathOperation(()=>MathJax.mathml2svgPromise(value.preview,{display}));
  applyMathDocumentWarnings(node,value);
  node.dataset.readmdMathRenderer='mathjax';return node;
}
async function renderDslMath(body) {
  if(!body)return;
  observeMathLayout(body);
  // Version diagnostics and source-context slots never become legacy DSL.
  const pending='.readmd-dsl-math:not([data-readmd-ready]):not([data-readmd-unsupported]),.readmd-model-math:not([data-readmd-ready])';
  const elements=[...(body.matches?.(pending)?[body]:[]),...body.querySelectorAll(pending)];
  const root=body.closest('#preview-pane, #content')||body;
  let lazy=dslMathObservers.get(root);
  if(lazy)for(const target of lazy.targets)if(!target.isConnected||!root.contains(target)){lazy.observer.unobserve(target);lazy.targets.delete(target);}
  if(elements.length>60&&window.IntersectionObserver){
    if(!lazy){
      lazy={targets:new Set(),observer:null};
      lazy.observer=new IntersectionObserver(entries=>{for(const entry of entries)if(entry.isIntersecting){lazy.observer.unobserve(entry.target);lazy.targets.delete(entry.target);renderDslMath(entry.target);}},{root,rootMargin:'350px 0px',threshold:0.01});
      dslMathObservers.set(root,lazy);
    }
    elements.slice(15).forEach(element=>{lazy.targets.add(element);lazy.observer.observe(element);});elements.length=15;
  }
  for(const element of elements){
    if(!element.isConnected||element.dataset.readmdBusy)continue;
    element.dataset.readmdBusy='true';element.setAttribute('aria-busy','true');
    let text='',retry=false,identity=null;const fontEpoch=mathFontEpoch;
    try{
      text=mathFormulaSource(element);identity=mathFormulaIdentity(element);
      if(text===null||!identity)throw new DOMException('Superseded formula context','AbortError');
      const display=element.dataset.readmdMode==='block',layoutKey=JSON.stringify(mathLayoutMetrics(element,display));
      const value=await compileDslMath(text,display,element);
      const proof=mathCompileResults.get(value)||{identity,layoutKey,fontEpoch};
      if(!element.isConnected)continue;
      const rendered=await renderMathDocument(value,display);
      // A shared job is publishable only for the same immutable document,
      // revision, defaults fingerprint, exact range, metadata and metrics.
      if(!element.isConnected)continue;
      if(proof.fontEpoch!==mathFontEpoch||proof.identity!==mathFormulaIdentity(element)||text!==mathFormulaSource(element)||proof.layoutKey!==JSON.stringify(mathLayoutMetrics(element,display))){retry=!!mathFormulaIdentity(element);continue;}
      if(rendered.querySelector('[data-mml-node="merror"],[data-mjx-error]'))throw Error('math_invalid_source');
      element.replaceChildren(rendered);element.dataset.readmdReady=value.preview_supported===false?'source':'true';element.dataset.readmdLayout=layoutKey;
      element.dataset.readmdReadyKey=JSON.stringify([proof.identity,proof.fontEpoch,proof.layoutKey]);
      if(value.preview_supported===false)element.dataset.readmdPreviewSupported='false';else delete element.dataset.readmdPreviewSupported;
    }catch(error){
      const now=mathFormulaIdentity(element);
      if(fontEpoch!==mathFontEpoch||now&&identity&&now!==identity)retry=true;
      else if(error.name!=='AbortError'&&element.isConnected&&now===identity){
        element.textContent=element.dataset.readmdContextSlot?apiMessage({error_code:error.message},'formula.invalid'):text;
        element.title=apiMessage({error_code:error.message},'formula.invalid');element.dataset.readmdReady='error';delete element.dataset.readmdReadyKey;delete element.dataset.readmdPreviewSupported;
      }
    }finally{delete element.dataset.readmdBusy;element.removeAttribute('aria-busy');if(element.isConnected&&mathFormulaIdentity(element)&&(retry||text!==mathFormulaSource(element)))requestAnimationFrame(()=>renderDslMath(element));}
  }
}
function queueMathOperation(operation) {
  const result = mathTypesetQueue.then(operation);
  mathTypesetQueue = result.catch(() => {});
  return result;
}
function queueMathTypeset(elements) {
  return queueMathOperation(() => {
    // MathML's DOM finder also discovers the assistive MathML inside results
    // produced by mathml2svgPromise. Unlike the TeX finder it does not skip
    // rendered containers. Never typeset that accessibility tree a second time:
    // it creates nested SVGs and makes normal formulas intercept each other.
    for (const input of MathJax.startup.document.inputJax) {
      if (input.name !== 'MathML' || filteredMathInputs.has(input)) continue;
      const find = input.findMath.bind(input);
      input.findMath = element => find(element).filter(item => !item.start.node.closest?.('mjx-container,[data-readmd-math-renderer]'));
      filteredMathInputs.add(input);
    }
    return MathJax.typesetPromise(elements);
  }).then(()=>{
    // A cold formula can have been typeset inside an ordinary-text baseline
    // span. Rebuild only those existing bindings, before measuring metadata.
    for(const root of elements)window.ReadMDPublication?.refreshBaseline?.(root);
    window.ReadMDMathMetadata?.schedule();
  }).catch(() => {});
}

// Incremental typesetting appends new items to MathJax's internal list. Match
// occurrences in the actual document order, including repeated expressions.
function renderedMathItemsWithin(pane) {
  if (!pane) return [];
  const items = window.MathJax?.startup?.document?.getMathItemsWithin?.([pane]);
  return Array.from(items || []).filter(item => pane.contains(item.typesetRoot)).sort((a, b) => {
    if (a.typesetRoot === b.typesetRoot) return 0;
    const position = a.typesetRoot.compareDocumentPosition(b.typesetRoot);
    return position & Node.DOCUMENT_POSITION_FOLLOWING ? -1 : position & Node.DOCUMENT_POSITION_PRECEDING ? 1 : 0;
  });
}

function renderMath(body) {
  if (!body) return;
  const html = body.innerHTML;
  if (!body.matches?.('.readmd-dsl-math,.readmd-model-math')&&!/readmd-dsl-math|readmd-model-math|\$\$|\\\(|\\\[|\$[^$\n]+\$|\\begin\{/.test(html)) return;

  function doTypeset() {
    if (!window.MathJax || !MathJax.typesetPromise) return;
    renderDslMath(body);

    if (mathObserver) {
      mathObserver.disconnect();
      mathObserver = null;
    }

    // 检测 DOM 中包含公式的段落/块级元素数量
    const hasMathText = el => {
      const txt = el.textContent || '';
      return txt.includes('$') || txt.includes('\\(') || txt.includes('\\[') || txt.includes('\\begin');
    };

    const mathBlocks = [];
    const directChildren = body.querySelectorAll('p, div, li, td, th, blockquote, .academic-callout');
    directChildren.forEach(el => {
      if (hasMathText(el) && !el.closest('pre, code, script, style')) {
        mathBlocks.push(el);
      }
    });

    // 如果公式块数量适中（<=60），直接全量极速排版
    if (mathBlocks.length <= 60 || !window.IntersectionObserver) {
      queueMathTypeset([body]);
      return;
    }

    // 超长文档（>60 个公式块）：视口按需懒渲染，彻底杜绝主线程卡死
    const scrollContainer = document.getElementById('content') || null;
    mathObserver = new IntersectionObserver((entries, obs) => {
      const visibleBatch = [];
      entries.forEach(entry => {
        if (entry.isIntersecting) {
          visibleBatch.push(entry.target);
          obs.unobserve(entry.target);
        }
      });
      if (visibleBatch.length > 0) {
        queueMathTypeset(visibleBatch);
      }
    }, {
      root: scrollContainer,
      rootMargin: '350px 0px 350px 0px',
      threshold: 0.01,
    });

    // 先立即渲染视口前 15 个块，确保即时开屏无延迟
    const immediateCount = Math.min(15, mathBlocks.length);
    const initialBatch = mathBlocks.slice(0, immediateCount);
    queueMathTypeset(initialBatch);

    // 其余块挂载到 Observer
    for (let i = immediateCount; i < mathBlocks.length; i++) {
      mathObserver.observe(mathBlocks[i]);
    }
  }

  if (window.MathJax) {
    if (typeof MathJax.typesetPromise === 'function') { try { doTypeset(); } catch (_) {} }
    else if (mathReadyPromise) mathReadyPromise.then(doTypeset).catch(() => {});
    return;
  }

  window.MathJax = {
    // html is already used by the formula tools; load the pinned local
    // component before TeX configuration, rather than a missing lazy URL.
    loader: { load: ['input/mml', '[tex]/html', '[tex]/boldsymbol', 'ui/safe'] },
    tex: {
      inlineMath: [['$', '$'], ['\\(', '\\)']],
      displayMath: [['$$', '$$'], ['\\[', '\\]']],
      packages: {'[-]': ['noundefined'], '[+]': ['cases', 'ams', 'color', 'html', 'boldsymbol']},
      formatError: (jax, err) => {
        return jax.formatError(err);
      }
    },
    options: {
      skipHtmlTags: ['script', 'noscript', 'style', 'textarea', 'pre', 'code'],
      safeOptions: {
        allow: { URLs: 'none', classes: 'safe', cssIDs: 'safe', styles: 'safe' },
        idPattern: /^(?:mjx-|rd-formula-token-)[-a-zA-Z0-9_.]+$/,
      },
    },
    startup: { typeset: false, input: ['tex','mml'] },
  };
  const s = document.createElement('script');
  s.src = '/assets/vendor/mathjax/tex-svg.js';
  mathReadyPromise = new Promise((resolve, reject) => {
    s.onload = () => Promise.resolve(MathJax.startup?.promise).then(resolve, reject);
    s.onerror = () => reject(Error('Local math renderer unavailable'));
  });
  mathReadyPromise.then(doTypeset).catch(() => {});
  document.head.appendChild(s);
}
