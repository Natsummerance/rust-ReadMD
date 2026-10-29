'use strict';
// Production-function DOM controls only; no browser geometry/native acceptance.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../..');
const read=name=>fs.readFileSync(path.join(root,name),'utf8');
const readerSource=read('assets/js/reader/formula.js'),inlineSource=read('assets/js/editor/formula-inline.js'),studioSource=read('assets/js/editor/formula-studio.js');
const english=JSON.parse(read('assets/i18n/en.json'));
const t=key=>english[key]??key;
const unsupported=t('error.math_unsupported_extension');
function apiMessage(value,fallback){return t('error.'+value.error_code)!=='error.'+value.error_code?t('error.'+value.error_code):t(fallback);}
const dataName=name=>name.replace(/-([a-z])/g,(_,c)=>c.toUpperCase());
class Element{
  constructor(tag,document){this.tagName=tag.toUpperCase();this.localName=tag;this.document=document;this.children=[];this.dataset={};this.attributes={};this.style={};this.className='';this.title='';this.value='';this.hidden=false;this.disabled=false;this.isConnected=true;this.listeners={};this._text='';
    this.classList={add:()=>{},remove:()=>{},toggle:()=>{}};}
  set textContent(value){this._text=String(value);this.children=[];}get textContent(){return this._text+this.children.map(c=>c.textContent).join('');}
  set innerHTML(_){throw Error('Source must not be interpreted as HTML');}
  append(...nodes){for(const node of nodes){node.parentElement=this;this.children.push(node);}}
  replaceChildren(...nodes){this._text='';this.children=[];this.append(...nodes);}
  setAttribute(name,value){this.attributes[name]=String(value);if(name.startsWith('data-'))this.dataset[dataName(name.slice(5))]=String(value);}
  getAttribute(name){return Object.hasOwn(this.attributes,name)?this.attributes[name]:null;}
  removeAttribute(name){delete this.attributes[name];if(name.startsWith('data-'))delete this.dataset[dataName(name.slice(5))];}
  addEventListener(name,handler){(this.listeners[name]||=[]).push(handler);}
  remove(){if(this.parentElement)this.parentElement.children=this.parentElement.children.filter(n=>n!==this);}
  contains(node){return node===this||this.children.some(child=>child.contains(node));}
  closest(){return null;}matches(){return false;}
  focus(){this.document.activeElement=this;}select(){}setSelectionRange(from,to){this.selectionStart=from;this.selectionEnd=to;}
  querySelectorAll(selector){const result=[];const matches=(element,part)=>{
    if(part.startsWith('.'))return element.className.split(' ').includes(part.slice(1));
    const data=/^\[data-([\w-]+)(?:="([^"]*)")?\]$/.exec(part);
    if(data)return Object.hasOwn(element.dataset,dataName(data[1]))&&(data[2]===undefined||element.dataset[dataName(data[1])]===data[2]);
    const attribute=/^\[([\w-]+)(?:="([^"]*)")?\]$/.exec(part);
    if(attribute)return element.getAttribute(attribute[1])!==null&&(attribute[2]===undefined||element.getAttribute(attribute[1])===attribute[2]);
    return element.localName===part;
  };const parts=selector.split(',').map(s=>s.trim());function walk(node){for(const child of node.children){if(parts.some(part=>matches(child,part)))result.push(child);walk(child);}}walk(this);return result;}
  querySelector(selector){return this.querySelectorAll(selector)[0]||null;}
}
function dom(){const elements=new Map(),document={readyState:'loading',activeElement:null,addEventListener(){},querySelectorAll(){return[];},createElement(tag){return new Element(tag,document);},createElementNS(_ns,tag){return new Element(tag,document);},getElementById(id){if(!elements.has(id))elements.set(id,new Element('span',document));return elements.get(id);}};return{document,elements};}
function environment(extra={}){const context={...dom(),console,Promise,Map,Set,WeakMap,WeakSet,TextEncoder,AbortController,DOMException,i18n:{t},apiMessage,addEventListener(){},requestAnimationFrame(){},...extra};context.window=context;vm.createContext(context);return context;}
function reader(){const calls={mathjax:[],domParser:0};const context=environment({MathJax:{mathml2svgPromise:async(...args)=>{calls.mathjax.push(args);return context.document.createElement('span');}},DOMParser:class{constructor(){calls.domParser++;throw Error('No XML parse for source-only');}}});vm.runInContext(readerSource,context);return{context,calls};}
const source='row(word.pPr(word.ind(left: "-1")), office.oMathPara(office.oMath(x)))';
function unavailable({dialect='dsl',text=source}={}){return{ok:true,preview_supported:false,preview:'',vector_preview:null,mathml_warns:['math_unsupported_extension'],font_warnings:[],layout_diagnostics:[],document:{dialect,source:text,revision:'owned-revision',root:{},nodes:[],diagnostics:[{code:'math_unsupported_extension',from:0,to:text.length}]}};}
function extract(text,start,end){const from=text.indexOf(start),to=text.indexOf(end,from);assert(from>=0&&to>from);return text.slice(from,to);}

test('source-only preview bypasses MathJax/XML, uses localized text and exposes only readable DSL',async()=>{
  const {context,calls}=reader(),value=unavailable({text:'row(x, "<script>untrusted</script>")'});value.preview='<malformed';value.vector_preview='<svg onclick="untrusted"/>';
  const before=JSON.stringify(value),node=await context.renderMathDocument(value,true);
  assert.equal(calls.mathjax.length,0);assert.equal(calls.domParser,0);assert.equal(JSON.stringify(value),before);
  assert.equal(node.dataset.readmdMathRenderer,'source');assert.equal(node.dataset.readmdPreviewSupported,'false');assert.equal(node.getAttribute('role'),'note');
  assert.equal(node.querySelector('[data-readmd-math-unavailable]').textContent,unsupported);assert.equal(node.title,unsupported);
  assert.equal(node.querySelectorAll('svg,[data-math-span],[data-math-hit]').length,0);
  const code=node.querySelector('code'),entry=node.querySelector('[data-readmd-math-source-entry]');assert.equal(code.textContent,value.document.source);assert.equal(code.hidden,true);assert.equal(entry.textContent,t('formula.source'));
  entry.onclick();assert.equal(code.hidden,false);assert.equal(entry.getAttribute('aria-expanded'),'true');
});
test('reader never dumps native XML or compressed metadata in a source-only note',async()=>{
  const {context,calls}=reader();for(const dialect of ['omml','mathml','latex']){
    const node=await context.renderMathDocument(unavailable({dialect,text:'<w:pPr/> <!--readmd:math:v2:private:compressed-->'}),false);
    assert.equal(node.querySelector('code'),null);assert.equal(node.querySelector('[data-readmd-math-source-entry]'),null);assert.equal(node.textContent,unsupported);
  }assert.equal(calls.mathjax.length,0);assert.equal(calls.domParser,0);
});
test('supported legacy previews still use MathJax and are not reclassified as source-only',async()=>{
  const {context,calls}=reader();const node=await context.renderMathDocument({preview:'<math><mi>x</mi></math>',font_warnings:[],layout_diagnostics:[]},false);
  assert.equal(calls.mathjax.length,1);assert.equal(calls.mathjax[0][0],'<math><mi>x</mi></math>');assert.equal(node.dataset.readmdMathRenderer,'mathjax');assert.equal(node.querySelector('[data-readmd-math-unavailable]'),null);
});
test('reader records unavailable preview as source state and keeps occurrence identity',async()=>{
  const {context,calls}=reader(),body=context.document.createElement('div'),owner=context.document.createElement('span');owner.dataset.readmdDsl=encodeURIComponent(source);owner.dataset.readmdMode='block';body.append(owner);
  body.querySelectorAll=()=>[owner];context.observeMathLayout=()=>{};context.mathLayoutMetrics=()=>({font_size:12,layout_width:100});
  const requests=[];context.compileDslMath=async(text,display,element)=>{requests.push({text,display,element});return unavailable();};
  await context.renderDslMath(body);assert.equal(owner.dataset.readmdReady,'source');assert.equal(owner.dataset.readmdPreviewSupported,'false');assert.equal(requests[0].element,owner);assert.equal(requests[0].text,source);assert.equal(calls.mathjax.length,0);assert.equal(decodeURIComponent(owner.dataset.readmdDsl),source);
  delete owner.dataset.readmdReady;context.compileDslMath=async()=>({preview:'<math/>',font_warnings:[],layout_diagnostics:[]});await context.renderDslMath(body);
  assert.equal(owner.dataset.readmdReady,'true');assert.equal(Object.hasOwn(owner.dataset,'readmdPreviewSupported'),false);assert.equal(calls.mathjax.length,1);
});
test('inline request retains source-only fields and syntax diagnostics on the rich model',async()=>{
  const value=unavailable(),context=environment({mathLayoutMetrics:()=>({font_size:12}),apiFetch:async()=>({ok:true,json:async()=>value})});
  vm.runInContext(extract(inlineSource,'  async function request(', '  function locate('),context);
  const model=await context.request('/api/math/parse',{source,dialect:'dsl',display:true});assert.equal(model.previewSupported,false);assert.deepEqual(Array.from(model.mathmlWarns),['math_unsupported_extension']);assert.equal(model.diagnostics[0].code,'math_unsupported_extension');assert.equal(model.source,source);
  context.apiFetch=async()=>({ok:false,json:async()=>({ok:false,error_code:'math_invalid_property'})});await assert.rejects(context.request('/api/math/parse',{source,dialect:'dsl'}),/math_invalid_property/);
});
test('inline source-only rendering clears stale hotspots while keeping valid Apply and source entry',async()=>{
  const {context:renderer}=reader(),calls=[];const context=environment({current:()=>true,epoch:7,tr:t,openSource:d=>calls.push(d),ReadMDMathProperties:{flatten(){throw Error('No visual source mapping without a preview');}},renderMathDocument:async value=>{calls.push(value);return renderer.renderMathDocument(value,true);}});
  vm.runInContext(extract(inlineSource,'  async function renderRich(', '  async function render(d,'),context);
  const d={range:{mode:'block'},rich:{...unavailable().document,preview:'',previewSupported:false,mathmlWarns:['math_unsupported_extension']},canvas:context.document.createElement('span'),input:context.document.createElement('input'),message:context.document.createElement('span'),apply:context.document.createElement('button'),spans:[{node:'stale'}],selected:0,valid:false};
  await context.renderRich(d,7,0);assert.equal(calls[0].preview_supported,false);assert.deepEqual(Array.from(calls[0].mathml_warns),['math_unsupported_extension']);assert.equal(calls[0].document,d.rich);
  assert.equal(d.valid,true);assert.equal(d.apply.disabled,false);assert.equal(d.input.disabled,true);assert.equal(d.input.hidden,true);assert.equal(d.selected,null);assert.equal(d.spans.length,0);assert.equal(d.message.textContent,unsupported);
  assert.equal(d.canvas.querySelectorAll('[data-math-span],[data-math-hit]').length,0);const entry=d.canvas.querySelector('[data-readmd-math-source-entry]');assert.equal(entry.getAttribute('aria-haspopup'),'dialog');entry.onclick();assert.equal(calls[1],d);
  d.rich.extensionMetadata='opaque-carrier';const previous=calls.length;await context.renderRich(d,7,0);
  const preserved=d.canvas.querySelector('[data-readmd-math-source-entry]');assert.equal(preserved.getAttribute('aria-haspopup'),null);assert.equal(preserved.getAttribute('aria-expanded'),'false');preserved.onclick();assert.equal(d.canvas.querySelector('code').hidden,false);assert.equal(calls.length,previous+1);assert.equal(d.valid,true);
});
function handoff({dialect='dsl',metadata,serialize={output:source},persist=true,onRequest,onPersist}={}){
  const calls=[],doc='😀 first-source second-source last-source',from=doc.indexOf('second-source'),range={from,to:from+'second-source'.length,tex:'second-source',mode:'block',dialect:'dsl'},d={range,doc,view:{},tab:'owned-tab',rich:{source,dialect,extensionMetadata:metadata,identity:'owned-equation'},message:{textContent:''}};let live=true;
  const context=environment({current:()=>live,persist:async value=>{calls.push({persist:value});if(onPersist)onPersist(()=>live=false);return persist;},request:async(route,body)=>{calls.push({route,body});if(onRequest)onRequest(()=>live=false);return serialize;},cancel:focus=>calls.push({cancel:focus}),ReadMDFormula:{open:async(...args)=>calls.push({open:args})}});
  vm.runInContext(extract(inlineSource,'  async function openSource(', '  async function renderRich('),context);return{context,d,calls,run:()=>context.openSource(d)};
}
test('source handoff carries an uncommitted draft with its original range, tab and full source binding',async()=>{
  const f=handoff();await f.run();assert.equal(f.calls.length,3);assert.equal(f.calls[0].persist,f.d);assert.equal(f.calls[1].cancel,false);
  const [mode,range,initial]=f.calls[2].open;assert.equal(mode,'block');assert.equal(range,f.d.range);assert.equal(initial.source,source);assert.equal(initial.view,f.d.view);assert.equal(initial.tab,f.d.tab);assert.equal(initial.doc,f.d.doc);assert.equal(initial.from,f.d.range.from);assert.equal(initial.to,f.d.range.to);assert.equal(initial.original,f.d.doc.slice(f.d.range.from,f.d.range.to));assert.equal(f.d.sourceOpening,false);
  const native=handoff({dialect:'omml'});await native.run();assert.equal(native.calls[0].route,'/api/math/serialize');assert.equal(native.calls[0].body.target,'dsl');assert.equal(native.calls.at(-1).open[2].dialect,'dsl');assert.equal(native.calls.at(-1).open[1],native.d.range);
});
test('stale, unpersisted or opaque-metadata handoffs preserve the inline draft and write nothing',async()=>{
  for(const options of [{onRequest:stop=>stop(),dialect:'omml'},{onPersist:stop=>stop()},{persist:false},{metadata:'<!--readmd:math:v2:opaque-->'},{dialect:'omml',serialize:{output:source,metadata:'opaque'}}]){
    const f=handoff(options),original=f.d.rich;await f.run();assert.equal(f.calls.some(row=>Object.hasOwn(row,'open')||Object.hasOwn(row,'cancel')),false);assert.equal(f.d.rich,original);assert.equal(f.d.sourceOpening,false);if(options.metadata||options.serialize?.metadata)assert.equal(f.d.message.textContent,unsupported);
  }
});
function studio({response=unavailable(),fetch:pending,doc='😀 first :math[x] second :math[x] last :math[x]',mathjax=false}={}){
  let text=doc;const changes=[],requests=[],calls={mathjax:0,loader:0};const context=environment({state:{editing:true},getActiveTab:()=>({id:'owned-tab'}),closeMdPopups(){},ReadMDModal:{close(){}},renderMath(){calls.loader++;},queueMathOperation:fn=>fn(),fetch:async()=>({ok:true,json:async()=>({schema:1,symbols:[],templates:[]})}),apiFetch:async(route,options)=>{requests.push({route,body:JSON.parse(options.body)});return pending?pending():{ok:response.ok,json:async()=>response};}});
  const view={state:{doc:{toString:()=>text},selection:{main:{from:0,to:0}},sliceDoc:(from,to)=>text.slice(from,to)},dispatch(change){changes.push(change);text=text.slice(0,change.changes.from)+change.changes.insert+text.slice(change.changes.to);},focus(){}};context.cmView=view;
  context.renderMathDocument=(value,display)=>{assert.equal(value.preview_supported,false);const note=context.document.createElement('span');note.textContent=unsupported;note.dataset.readmdMathRenderer='source';return note;};
  if(mathjax)context.MathJax={tex2svgPromise:async()=>{calls.mathjax++;return context.document.createElement('span');},mathml2svgPromise:async()=>{calls.mathjax++;return context.document.createElement('span');}};
  const marker='  return { open, close, picker, apply, insert, ranges, write, asDsl, metadataRanges, withoutMetadata, tokens, annotate, load };';assert.equal(studioSource.split(marker).length,2);
  vm.runInContext(studioSource.replace(marker,'  window.__studio={renderDraft,setDraft:value=>draft=value,getDraft:()=>draft};\n'+marker),context);
  context.document.getElementById('formula-mode').value='inline';context.document.getElementById('formula-search').value='';
  return{context,view,changes,requests,calls,getText:()=>text,setText:value=>text=value};
}
test('studio source-only response stays editable and applies only the bound repeated occurrence',async()=>{
  const f=studio(),ranges=f.context.ReadMDFormula.ranges(f.getText()),range=ranges[1],before=f.getText();
  const initial={view:f.view,tab:'owned-tab',doc:before,from:range.from,to:range.to,original:before.slice(range.from,range.to),source,dialect:'dsl'};
  assert.equal(await f.context.ReadMDFormula.open('inline',range,initial),true);
  const draft=f.context.__studio.getDraft();assert.equal(draft.tex,source);assert.equal(draft.valid,true);assert.equal(draft.tokens.length,0);assert.equal(draft.selected,null);assert.equal(draft.previewSupported,false);
  assert.equal(f.context.document.getElementById('formula-source').value,source);assert.equal(f.context.document.getElementById('formula-source-details').open,true);assert.equal(f.context.document.getElementById('formula-error').textContent,unsupported);assert.equal(f.context.document.getElementById('formula-apply').disabled,false);assert.equal(f.context.document.getElementById('formula-token-row').hidden,true);
  assert.equal(f.calls.mathjax,0);assert.equal(f.calls.loader,0);assert.equal(f.getText(),before);
  f.context.ReadMDFormula.apply();assert.equal(f.changes.length,1);assert.equal(f.changes[0].changes.from,range.from);assert.equal(f.changes[0].changes.to,range.to);assert.equal(f.getText(),before.slice(0,range.from)+':math['+source+']'+before.slice(range.to));
});
test('inline Apply accepts capability diagnostics and changes only the exact source occurrence',async()=>{
  const f=studio({doc:'\uFEFF😀 :math[x]\r\n:math[x]\r\n:math[x]'}),before=f.getText(),range=f.context.ReadMDFormula.ranges(before)[1],events=[];
  const d={range,doc:before,view:f.view,tab:'owned-tab',tex:source,rich:{...unavailable().document,previewSupported:false},valid:true,selected:null,composing:false};
  const context=environment({cmView:f.view,getActiveTab:()=>({id:'owned-tab'}),current:()=>true,replace:async()=>assert.equal(d.selected,null),cancel:focus=>events.push(['cancel',focus]),drop:async value=>events.push(['drop',value]),ReadMDFormula:f.context.ReadMDFormula,request(){throw Error('DSL source Apply needs no preview or lossy serialize');},error(){throw Error('Capability diagnostic must not become a source syntax error');}});
  vm.runInContext(extract(inlineSource,'  async function commitDraft(', '  async function open(container,'),context);
  assert.equal(await context.commitDraft(d,true),true);assert.equal(d.applied,true);assert.equal(f.changes.length,1);assert.equal(f.changes[0].changes.from,range.from);assert.equal(f.changes[0].changes.to,range.to);assert.equal(f.getText(),before.slice(0,range.from)+':math['+source+']'+before.slice(range.to));assert.equal(events[0][0],'cancel');assert.equal(events[1][1],d);
});
test('studio keeps supported rendering and unavailable preview states distinct',async()=>{
  const f=studio({response:{ok:true,preview_supported:true,preview:'<math/>',document:{nodes:[]}},mathjax:true}),range=f.context.ReadMDFormula.ranges(f.getText())[1];
  await f.context.ReadMDFormula.open('inline',range);assert.equal(f.context.__studio.getDraft().valid,true);assert.equal(f.calls.mathjax,1);assert.equal(f.calls.loader,0);
  f.context.apiFetch=async()=>({ok:true,json:async()=>unavailable()});await f.context.__studio.renderDraft();assert.equal(f.context.__studio.getDraft().valid,true);assert.equal(f.context.__studio.getDraft().previewSupported,false);assert.equal(f.calls.mathjax,1);assert.equal(f.context.document.getElementById('formula-token-row').hidden,true);
});
test('studio rejects stale initial binding and source-only cancellation never writes the document',async()=>{
  for(const field of ['view','tab','doc','from','to','original','dialect']){
    const f=studio(),range=f.context.ReadMDFormula.ranges(f.getText())[1],initial={view:f.view,tab:'owned-tab',doc:f.getText(),from:range.from,to:range.to,original:f.getText().slice(range.from,range.to),source,dialect:'dsl'};
    initial[field]=field==='view'?{}:field==='from'||field==='to'?initial[field]+1:'different';assert.equal(await f.context.ReadMDFormula.open('inline',range,initial),false,field);assert.equal(f.requests.length,0);assert.equal(f.changes.length,0);assert.equal(f.context.__studio.getDraft(),null);
  }
  const f=studio(),range=f.context.ReadMDFormula.ranges(f.getText())[1],before=f.getText();await f.context.ReadMDFormula.open('inline',range);f.context.ReadMDFormula.close();assert.equal(f.getText(),before);assert.equal(f.changes.length,0);
});
test('studio syntax rejection stays distinct from capability diagnostics and prevents Apply',async()=>{
  const f=studio({response:{ok:false,error_code:'math_invalid_property'}}),range=f.context.ReadMDFormula.ranges(f.getText())[1],before=f.getText();await f.context.ReadMDFormula.open('inline',range);
  assert.equal(f.context.__studio.getDraft().valid,false);assert.equal(f.context.document.getElementById('formula-apply').disabled,true);assert.equal(f.context.document.getElementById('formula-error').textContent,t('formula.invalid'));assert.equal(f.calls.mathjax,0);assert.equal(f.calls.loader,0);f.context.ReadMDFormula.apply();assert.equal(f.getText(),before);assert.equal(f.changes.length,0);
});
test('studio does not treat a late source-only response as the replacement draft',async()=>{
  let resolve;const pending=new Promise(done=>resolve=done),f=studio({fetch:()=>pending}),range=f.context.ReadMDFormula.ranges(f.getText())[1];const task=f.context.ReadMDFormula.open('inline',range);while(!f.requests.length)await Promise.resolve();
  const replacement={tex:'replacement',valid:false};f.context.__studio.setDraft(replacement);resolve({ok:true,json:async()=>unavailable()});await task;
  assert.equal(f.context.__studio.getDraft(),replacement);assert.equal(replacement.valid,false);assert.equal(f.context.document.getElementById('formula-preview').children.length,0);assert.equal(f.calls.mathjax,0);assert.equal(f.changes.length,0);
});
test('approved review integration keeps frontend bytes fixed outside the formula context hook',()=>{
  // V006 batch 1 changes review/editor/save integration; batch 3 also fixes
  // clipboard and column alignment tables. Advance only the reviewed editor
  // baseline after real CSP editing, undo, save and reopen checks. Other guards stay unchanged.
  // The two advanced baselines use canonical CRLF, independent of checkout
  // EOL. The preview baseline includes the rename/recovery fence validated by
  // the separate real-browser edit, undo/redo, save and restart regression.
  // Leave the three pre-existing exact-byte protection pins untouched.
  const protectedFiles=[
    {path:'assets/js/editor/editor.js',sha256:'4eeb5bf5a071299820c692d46ca3ef01cff171be46f9bf6b37b41a5189a61fdf'},
    {path:'assets/js/editor/preview.js',sha256:'c417cfc69977d7961d5f5745b25991fbf9636d68a9d512fcfdea158ddce34635'},
    {path:'assets/js/core/settings.js',sha256:'5a8939d0f721b0740565a1725a974fc3b6d46c6815e8b79166f40e757c725aae'},
    {path:'assets/css/panels.css',sha256:'bea3c2046704118937afc0eeddb13b68584ace5c79216cb5611b157ef619d146'},
    {path:'tests/frontend/toolbar.test.mjs',sha256:'218d6e854ea8368d3bc888515977297fd27fe4c65b61accea9b12141da08f0e2'},
  ];assert.equal(protectedFiles.length,5);
  for(const row of protectedFiles){
    let bytes=fs.readFileSync(path.join(root,row.path));
    if(row.path==='assets/js/editor/preview.js'){
      const text=bytes.toString('utf8'),begin='  // READMD_MATH_CONTEXT_BEGIN',end='  // READMD_MATH_CONTEXT_END';
      if(text.includes(begin)){
        assert.equal(text.split(begin).length,2);assert.equal(text.split(end).length,2);
        bytes=Buffer.from(text.slice(0,text.indexOf(begin))+text.slice(text.indexOf(end)+end.length).replace(/^\r?\n/,''));
      }
    }
    if(row.path==='assets/css/panels.css'){
      let text=bytes.toString('utf8');
      const refactor='\n/* READMD_DOCUMENT_REFACTOR_BEGIN */',refactorAt=text.indexOf(refactor);
      if(refactorAt>=0){
        assert.equal(text.split(refactor).length,2);const suffix=text.slice(refactorAt);
        assert.ok(suffix.endsWith('/* READMD_DOCUMENT_REFACTOR_END */\n'));
        for(const match of suffix.replace(/\/\*[\s\S]*?\*\//g,'').matchAll(/([^{}]+)\{/g))assert.match(match[1].trim(),/^\.document-refactor-/);
        text=text.slice(0,refactorAt);
      }
      const tasks='\n/* READMD_AI_TASK_STATUS_BEGIN */',taskAt=text.indexOf(tasks);
      if(taskAt>=0){assert.equal(text.split(tasks).length,2);assert.ok(text.endsWith('/* READMD_AI_TASK_STATUS_END */\n'));text=text.slice(0,taskAt);}
      const begin='/* READMD_CONTEXT_SOURCE_BEGIN */',end='/* READMD_CONTEXT_SOURCE_END */\n';
      if(text.includes(begin)){
        assert.equal(text.split(begin).length,2);assert.equal(text.split(end).length,2);assert(text.endsWith(end));
        bytes=Buffer.from(text.slice(0,text.indexOf(begin)));
      }
    }
    if(row.path==='assets/js/editor/editor.js'||row.path==='assets/js/editor/preview.js')bytes=Buffer.from(bytes.toString('utf8').replace(/\r\n?/g,'\n').replace(/\n/g,'\r\n'));
    assert.equal(crypto.createHash('sha256').update(bytes).digest('hex'),row.sha256,row.path);
  }
});
