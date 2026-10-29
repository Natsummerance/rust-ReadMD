'use strict';
// Bounded controls for production source-context lifecycle; no browser/engine.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../..');
const read=p=>fs.readFileSync(path.join(root,p),'utf8');
const formula=read('assets/js/reader/formula.js');
const source='$$x$$\r\n\r\n```readmd-math v1\r\nrow(office.mathPr(office.dispDef(val: "0")), office.oMathPara(office.oMath(x)))\r\n```\r\n\r\n$$x$$';
const sha=s=>crypto.createHash('sha256').update(s).digest('hex');
const tick=()=>new Promise(setImmediate);
function descriptor(raw,ordinal,from,to,tex,dialect='latex',display=true){return{ordinal,from,to,body_from:from+2,body_to:to-2,source:tex,dialect,display,patchable:true,metadata:null,source_revision:sha(tex)};}
function response(raw,view=raw,serial=1){
  const last=raw.lastIndexOf('$$x$$'),start=raw.indexOf('```'),end=raw.lastIndexOf('```')+3;
  const occurrences=[descriptor(raw,0,0,5,'x'),descriptor(raw,1,start,end,raw.slice(start+24,end-3).trim(),'dsl'),descriptor(raw,2,last,last+5,'x')];
  const context={id:'context-'+serial,nonce:'00000000-0000-4000-8000-'+String(serial).padStart(12,'0'),revision:sha(raw),fingerprint:'policy-'+sha(raw),active:true,ttl_ms:300000,occurrences};
  let output=view;
  // Only the fake API has these offsets. Frontend must consume its tokens,
  // never infer ownership from repeated expression text.
  if(view===raw){output='';let cursor=0;for(const o of occurrences){output+=raw.slice(cursor,o.from)+'<!--readmd:math-slot:v1:'+context.nonce+':'+o.ordinal+'-->'+raw.slice(o.from,o.to).replace(/[^\r\n]/g,'');cursor=o.to;}output+=raw.slice(cursor);}
  return{ok:true,context,view_content:output};
}
function environment(fetch){
  let now=1000;const timers=[],nodes=[],calls=[];
  const document={body:{},querySelectorAll:()=>nodes,getElementById(){return null;},addEventListener(){}};
  const context={console,Promise,Map,Set,WeakMap,WeakSet,TextEncoder,Uint8Array,AbortController,DOMException,crypto:crypto.webcrypto,
    Date:class extends Date{static now(){return now;}},setTimeout(fn,ms){const timer={fn,ms};timers.push(timer);return timer;},clearTimeout(timer){if(timer)timer.cancelled=true;},requestAnimationFrame(){},
    document,i18n:{t:k=>k},apiMessage:(v,f)=>v.error_code||f,escapeHtml:s=>String(s).replace(/[&<>"']/g,c=>'&#'+c.charCodeAt(0)+';'),addEventListener(){},
    ReadMDFormula:{ranges(){return[];}},apiFetch:async(route,options)=>{const body=JSON.parse(options.body);calls.push({route,body});return fetch(route,body,calls.length);}};
  context.window=context;vm.createContext(context);vm.runInContext(formula,context);
  context.mathLayoutMetrics=()=>({font_size:12,layout_width:400});
  const owner=(nonce,ordinal=0)=>({dataset:{readmdContextSlot:nonce+':'+ordinal,readmdMode:'block'},isConnected:true,clientWidth:500,
    matches:s=>s.includes('readmd-model-math'),closest(){return this;},setAttribute(){},removeAttribute(){},querySelector(){return null;},querySelectorAll(){return[];},replaceChildren(value){this.rendered=value;}});
  return{context,calls,timers,nodes,owner,setTime:value=>now=value};
}
const ok=value=>({ok:true,status:200,json:async()=>value});
async function prepared(extra={}){let serial=0;const f=environment(async(route,body)=>route==='/api/math/context'?ok(response(body.content,body.view_content,++serial)):ok({ok:true,document:{source:body.source},document_context:body.document_context,preview:'<math/>'}));let current=true;const scope={};const options={scope,current:()=>current,...extra};const view=await f.context.ReadMDMathContext.prepareSource(source,source,options);const c=f.calls[0];const nonce=/v1:([^:]+):0-->/.exec(view)[1];return{...f,view,nonce,scope,options,setCurrent:value=>current=value,registration:c};}

test('registers full author source once and consumes duplicate ordinal tokens independently',async()=>{
  const f=await prepared();assert.equal(f.registration.route,'/api/math/context');assert.equal(f.registration.body.content,source);assert.equal(f.registration.body.expected_revision,sha(source));assert.equal(f.registration.body.view_content,source);
  const protectedMath=f.context.protectMath(f.view);assert.equal(protectedMath.saved.length,3);assert.match(protectedMath.saved[0],/readmd-model-math/);assert.match(protectedMath.saved[0],/data-readmd-context-slot="[^" ]+:0"/);assert.match(protectedMath.saved[2],/data-readmd-context-slot="[^" ]+:2"/);
  assert.doesNotMatch(protectedMath.saved[0],/readmd-dsl-math/);assert.match(protectedMath.saved[0],/tex2jax_ignore/);assert.equal(f.calls.length,1);
  const a=f.context.ReadMDMathContext.locate(f.owner(f.nonce,0),source),b=f.context.ReadMDMathContext.locate(f.owner(f.nonce,2),source);assert.equal(a.tex,b.tex);assert.notEqual(a.from,b.from);assert.equal(a.dialect,'latex');
});
test('a forged nonce or stale document owner cannot create a compilable model slot',async()=>{
  const f=await prepared();const unknown=f.context.protectMath('<!--readmd:math-slot:v1:00000000-0000-4000-8000-999999999999:0-->');assert.doesNotMatch(unknown.saved.join(''),/data-readmd-context-slot=/);
  f.setCurrent(false);assert.equal(f.context.ReadMDMathContext.locate(f.owner(f.nonce),source),null);await assert.rejects(f.context.compileDslMath('x',true,f.owner(f.nonce)),e=>e.name==='AbortError');assert.equal(f.calls.length,1);
});
test('late registration is discarded after a tab/source owner change',async()=>{
  let resolve,current=true;const pending=new Promise(done=>resolve=done),f=environment(()=>pending);
  const task=f.context.ReadMDMathContext.prepareSource(source,source,{scope:{},current:()=>current});while(!f.calls.length)await tick();current=false;resolve(ok(response(source)));
  await assert.rejects(task,e=>e.name==='AbortError');assert.equal(f.context.ReadMDMathContext.locate(f.owner(response(source).context.nonce),source),null);
});
test('same-scope later render wins even when the first registration completes last',async()=>{
  const pending=[];const f=environment(()=>new Promise(done=>pending.push(done))),scope={};
  const first=f.context.ReadMDMathContext.prepareSource(source,source,{scope,current:()=>true});while(pending.length<1)await tick();
  const second=f.context.ReadMDMathContext.prepareSource(source,source,{scope,current:()=>true});while(pending.length<2)await tick();
  pending[1](ok(response(source,source,2)));await second;pending[0](ok(response(source,source,1)));await assert.rejects(first,e=>e.name==='AbortError');
  assert.equal(f.context.ReadMDMathContext.locate(f.owner(response(source,source,1).context.nonce),source),null);
});
test('context render cache separates equal TeX by source range and carries exact selectors',async()=>{
  const f=await prepared();await f.context.compileDslMath('x',true,f.owner(f.nonce,0));await f.context.compileDslMath('x',true,f.owner(f.nonce,2));
  const parses=f.calls.filter(c=>c.route==='/api/math/parse');assert.equal(parses.length,2);assert.notEqual(parses[0].body.document_context.from,parses[1].body.document_context.from);assert.equal(parses[0].body.document_context.revision,sha(source));assert.equal(parses[0].body.dialect,'latex');
  await f.context.compileDslMath('x',true,f.owner(f.nonce,0));assert.equal(f.calls.filter(c=>c.route==='/api/math/parse').length,2);
});
test('an expired context re-registers complete source and invalidates its former ready key',async()=>{
  const f=await prepared(),owner=f.owner(f.nonce);await f.context.compileDslMath('x',true,owner);const before=f.calls.at(-1).body.document_context.id;
  f.setTime(400000);await f.context.compileDslMath('x',true,owner);assert.equal(f.calls.filter(c=>c.route==='/api/math/context').length,2);assert.notEqual(f.calls.at(-1).body.document_context.id,before);
});
test('a server eviction retries once with a freshly registered source, never standalone',async()=>{
  let serial=0,parse=0;const f=environment(async(route,body)=>route==='/api/math/context'?ok(response(body.content,body.view_content,++serial)):++parse===1?{ok:false,status:409,json:async()=>({ok:false,error_code:'transaction_conflict'})}:ok({ok:true,document_context:body.document_context,preview:'<math/>'}));
  const view=await f.context.ReadMDMathContext.prepareSource(source,source,{scope:{},current:()=>true}),nonce=/v1:([^:]+):0-->/.exec(view)[1];await f.context.compileDslMath('x',true,f.owner(nonce));
  assert.equal(serial,2);assert.equal(parse,2);for(const row of f.calls.filter(c=>c.route==='/api/math/parse'))assert.ok(row.body.document_context);
});
test('a response that completed after source invalidation is never cached',async()=>{
  let finish,parseCount=0;const f=environment(async(route,body)=>route==='/api/math/context'?ok(response(source)):new Promise(done=>{parseCount++;finish=()=>done(ok({ok:true,document_context:body.document_context,preview:'<math/>'}));}));let current=true;
  const view=await f.context.ReadMDMathContext.prepareSource(source,source,{scope:{},current:()=>current}),nonce=/v1:([^:]+):0-->/.exec(view)[1],owner=f.owner(nonce);
  const task=f.context.compileDslMath('x',true,owner);while(!finish)await tick();current=false;finish();await assert.rejects(task,e=>e.name==='AbortError');assert.equal(vm.runInContext('dslMathCache.size',f.context),0);assert.equal(parseCount,1);
});
test('source identity remains exact for inline lookup and non-contiguous bodies stay read only',async()=>{
  const f=await prepared();assert.equal(f.context.ReadMDMathContext.locate(f.owner(f.nonce),source+' '),null);
  const g=environment(async()=>{const r=response(source);r.context.occurrences[1].patchable=false;return ok(r);});const view=await g.context.ReadMDMathContext.prepareSource(source,source,{scope:{},current:()=>true}),nonce=/v1:([^:]+):0-->/.exec(view)[1];
  assert.equal(g.context.ReadMDMathContext.locate(g.owner(nonce,1),source),null);assert.ok(g.context.ReadMDMathContext.slot(g.owner(nonce,1)));
});
test('projection proof failures are explicit and cannot degrade to standalone formula requests',async()=>{
  const f=environment(async()=>({ok:false,status:422,json:async()=>({ok:false,error_code:'math_unsupported_extension'})}));await assert.rejects(f.context.ReadMDMathContext.prepareSource(source,source+'\n$x$',{scope:{},current:()=>true}),/math_unsupported_extension/);assert.equal(f.calls.length,1);
});
test('a context failure retains readable source while hiding carriers and blocking all math scanners',()=>{
  const f=environment(()=>{}),raw='Visible prose\n\n$$x$$\n\n```readmd-math v1\nx\n```\n<!--readmd:math:v2:private:compressed-->\n\\(y\\)';
  const html=f.context.ReadMDMathContext.errorMarkup(raw,Error('math_unsupported_extension'));assert.match(html,/Visible prose/);assert.match(html,/<pre class="tex2jax_ignore"><code>/);assert.doesNotMatch(html,/compressed|readmd:math:v2|\$|\\|`/);
  const protectedMath=f.context.protectMath(html);assert.equal(protectedMath.saved.length,0);assert.match(protectedMath.src,/&#36;&#36;x&#36;&#36;/);
});
test('delayed DOM conversion cannot mark an obsolete document ready',async()=>{
  const f=await prepared(),owner=f.owner(f.nonce);let finish,converting=false;
  f.context.observeMathLayout=()=>{};f.context.renderMathDocument=()=>new Promise(done=>{converting=true;finish=()=>done({querySelector(){return null;}});});
  const body={matches(){return false;},querySelectorAll(){return[owner];},closest(){return null;},contains(){return true;}};
  const task=f.context.renderDslMath(body);while(!converting)await tick();f.setCurrent(false);finish();await task;
  assert.equal(owner.dataset.readmdReady,undefined);assert.equal(owner.dataset.readmdReadyKey,undefined);assert.equal(owner.rendered,undefined);
});
test('ready identity includes document policy/range and is replaced after automatic renewal',async()=>{
  const f=await prepared(),owner=f.owner(f.nonce);f.nodes.push(owner);f.context.observeMathLayout=()=>{};f.context.renderMathDocument=async()=>({querySelector(){return null;}});
  const body={matches(){return false;},querySelectorAll(){return[owner];},closest(){return null;},contains(){return true;}};
  await f.context.renderDslMath(body);const ready=owner.dataset.readmdReadyKey;assert.equal(owner.dataset.readmdReady,'true');assert.match(ready,/policy-/);assert.match(ready,/context-1/);
  f.setTime(400000);await f.context.renderDslMath(body);assert.notEqual(owner.dataset.readmdReadyKey,ready);assert.match(owner.dataset.readmdReadyKey,/context-2/);
});
test('queued stale jobs are discarded and model compilation retains the four-worker cap',async()=>{
  let inflight=0,maximum=0;const waiting=[],f=environment(async(route,body)=>{
    if(route==='/api/math/context')return ok(response(source));
    maximum=Math.max(maximum,++inflight);return new Promise(done=>waiting.push(()=>{inflight--;done(ok({ok:true,document_context:body.document_context,preview:'<math/>'}));}));
  });let current=true;const view=await f.context.ReadMDMathContext.prepareSource(source,source,{scope:{},current:()=>current}),nonce=/v1:([^:]+):0-->/.exec(view)[1];
  const tasks=[];for(let i=0;i<5;i++){const owner=f.owner(nonce);owner.metric=i;tasks.push(f.context.compileDslMath('x',true,owner).catch(e=>e));}
  f.context.mathLayoutMetrics=owner=>({font_size:12,layout_width:400+(owner.metric||0)});
  while(waiting.length<4)await tick();assert.equal(maximum,4);current=false;while(waiting.length)waiting.shift()();
  const results=await Promise.all(tasks);assert.ok(results.every(e=>e.name==='AbortError'));assert.equal(f.calls.filter(c=>c.route==='/api/math/parse').length,4);assert.equal(vm.runInContext('dslMathCache.size',f.context),0);
});
test('inline preserves a bound selector and rejects a response that drops it',async()=>{
  const inline=read('assets/js/editor/formula-inline.js'),start=inline.indexOf('  async function request('),end=inline.indexOf('  async function contextExpressionModel(',start),calls=[];
  const c={id:'owned',revision:'revision',fingerprint:'fingerprint',from:1,to:6};
  const f=environment(async(route,body)=>{calls.push(body);return ok({ok:true,document:{source:'x'},document_context:c});});
  vm.runInContext(inline.slice(start,end),f.context);const model=await f.context.request('/api/math/parse',{source:'x',document_context:c});assert.equal(model.documentContext.id,c.id);assert.deepEqual(calls[0].document_context,c);
  f.context.apiFetch=async()=>ok({ok:true,document:{source:'x'}});await assert.rejects(f.context.request('/api/math/parse',{source:'x',document_context:c}),/transaction_conflict/);
});
function htmlTextControl(tag){
  let value='';return{localName:tag,attributes:{},hidden:false,disabled:false,setAttribute(k,v){this.attributes[k]=v;},get value(){return value;},set value(v){value=tag==='input'?String(v).replace(/[\r\n]/g,''):String(v).replace(/\r\n?/g,'\n');}};
}
function inlineFixture({intercept,tamper,body='x^2',normalizeSemantic=false}={}){
  const semantic=text=>(normalizeSemantic?text.replace(/\r\n?/g,'\n'):text).trim();
  const raw='😀\r\n\r\n$$x^2$$\r\n\r\n$$'+body+'$$\r\n\r\n```readmd-math v1\r\nrow(office.mathPr(office.dispDef(val: false)), office.oMathPara(office.oMath(x)))\r\n```',first=raw.indexOf('$$'),second=raw.indexOf('$$',first+7),fence=raw.indexOf('```'),fenceEnd=raw.lastIndexOf('```')+3;
  const inventory=[descriptor(raw,0,first,first+7,'x^2'),descriptor(raw,1,second,second+body.length+4,semantic(body)),{...descriptor(raw,2,fence,fenceEnd,raw.slice(fence+18,fenceEnd-5),'dsl'),body_from:fence+18,body_to:fenceEnd-5}];
  const selected=inventory[1],calls=[],changes=[],nodes=[],originalContext={id:'original',revision:sha(raw),fingerprint:sha('original-policy'),active:true,occurrences:inventory};let liveText=raw,serial=0;
  const node=()=>({isConnected:true,dataset:{},children:[],value:'',hidden:false,disabled:false,attributes:{},setAttribute(k,v){this.attributes[k]=v;},getAttribute(k){return this.attributes[k]??null;},removeAttribute(k){delete this.attributes[k];},querySelector(){return null;},querySelectorAll(){throw Error('Ordinary source editor must not attach XML/glyph hotspots');},replaceChildren(...v){this.children=v;},replaceWith(v){this.isConnected=false;v.isConnected=true;},focus(){},select(){}});
  const model=(text,context)=>({source:text,dialect:'latex',revision:sha(text),nodes:[{id:'s0',kind:'symbol',from:0,to:1,tex:text[0],slots:[]},{id:'s1',kind:'superscript',from:1,to:3,tex:'^2',slots:[{role:'e',from:2,to:3}]}],root:{id:'0',name:'#root',content:[{type:'element',value:{id:'0.0',name:'m:oMath',content:[{type:'element',value:{id:'0.0.0',name:'m:sSup',content:[]}}]}}]},diagnostics:[],documentContext:context,preview:'<math/>',vectorPreview:'native-rust-'+sha(text)});
  const view={state:{doc:{toString:()=>liveText}},dispatch(change){changes.push(change);liveText=liveText.slice(0,change.changes.from)+change.changes.insert+liveText.slice(change.changes.to);},focus(){}};
  const context={console,Promise,Map,Set,WeakMap,WeakSet,TextEncoder,Uint8Array,AbortController,DOMException,crypto:crypto.webcrypto,structuredClone,
    state:{editing:true},cmView:view,getActiveTab:()=>({id:'owned-tab'}),i18n:{t:k=>k},apiMessage:v=>v.error_code,
    document:{readyState:'loading',addEventListener(){},getElementById(){return null;}},setTimeout(){return 1;},clearTimeout(){},setInterval(){return 1;},clearInterval(){},
    mathLayoutMetrics:()=>({font_size:12,layout_width:300}),ReadMDRecovery:{saveMathDraft:async()=>true,removeMathDraft:async()=>true},
    ReadMDMathProperties:{flatten(){throw Error('No s0 to XML0.0 mapping exists');},mount(){throw Error('No ordinary advanced properties');}},
    renderMathDocument:async value=>{assert.ok(value.vector_preview.startsWith('native-rust-'));nodes.push(value);return{querySelector(){return null;},vector:value.vector_preview};},
    apiFetch:async(route,options)=>{
      const body=JSON.parse(options.body),call={route,body,signal:options.signal};calls.push(call);
      let value;
      if(route==='/api/math/context'){
        const prefix=raw.slice(0,selected.body_from),suffix=raw.slice(selected.body_to);assert.ok(body.content.startsWith(prefix));assert.ok(body.content.endsWith(suffix));assert.equal(body.expected_revision,sha(body.content));
        const text=body.content.slice(prefix.length,body.content.length-suffix.length),delta=text.length-(selected.body_to-selected.body_from);
        const occurrences=inventory.map((o,index)=>{const shift=index>1?delta:0;return{...o,from:o.from+shift,to:o.to+(index===1?delta:shift),body_from:o.body_from+shift,body_to:index===1?o.body_from+text.length:o.body_to+shift,source:index===1?semantic(text):o.source,source_revision:index===1?sha(semantic(text)):o.source_revision};});
        value={ok:true,context:{id:'candidate-'+(++serial),revision:sha(body.content),fingerprint:sha(body.content),active:true,occurrences}};
        if(tamper)tamper(value);
      }else{assert.equal(route,'/api/math/parse','Ordinary source editing must never issue an XML patch');assert.equal(body.dialect,'latex');assert.ok(body.document_context);const document=model(body.source,body.document_context);delete document.documentContext;value={ok:true,document,document_context:body.document_context,preview:'<math/>',vector_preview:'native-rust-'+sha(body.source)};}
      return intercept?intercept(call,value):ok(value);
    }};
  context.window=context;vm.createContext(context);
  const production=read('assets/js/editor/formula-inline.js'),marker='  return { open,commit,cancel,locate,annotated,active:()=>!!draft,flush,leave,restore,recoveryText,suspend:()=>cancel(false) };';assert.equal(production.split(marker).length,2);
  vm.runInContext(production.replace(marker,'  window.__inlineContext={contextExpressionModel,replaceContextExpression,render,history,setDraft:value=>draft=value,getDraft:()=>draft};\n'+marker),context);
  const binding={id:originalContext.id,revision:originalContext.revision,fingerprint:originalContext.fingerprint,from:selected.from,to:selected.to};
  const d={doc:raw,view,tab:'owned-tab',tabRef:{id:'owned-tab'},range:{from:selected.from,to:selected.to,bodyFrom:selected.body_from,bodyTo:selected.body_to,open:'$$',close:'$$',tex:semantic(body),dialect:'latex',mode:'block',contextSite:{record:{raw,context:originalContext},occurrence:selected}},contextOrdinary:true,rich:model(semantic(body),binding),tex:semantic(body),richDirty:false,undo:[],redo:[],spans:[],selected:null,host:node(),original:node(),input:htmlTextControl('textarea'),apply:node(),message:node(),canvas:node(),valid:true};d.input.value=semantic(body);context.__inlineContext.setDraft(d);
  return{context,d,calls,changes,nodes,raw,selected,model,getText:()=>liveText,setText:value=>liveText=value,edit:async text=>{d.input.value=text;await context.__inlineContext.replaceContextExpression(d);}};
}
test('ordinary inline whole-source edits use exact candidate ranges with real s0 and XML0.0 identities',async()=>{
  const f=inlineFixture();assert.equal(f.d.rich.nodes[0].id,'s0');assert.equal(f.d.rich.root.content[0].value.id,'0.0');
  await f.edit('\\frac{a}{b}');assert.equal(f.d.valid,true);assert.equal(f.d.rich.source,'\\frac{a}{b}');assert.equal(f.d.input.value,'\\frac{a}{b}');assert.equal(f.d.undo.length,1);assert.equal(f.getText(),f.raw);assert.equal(f.changes.length,0);
  assert.equal(f.calls[0].body.content,f.raw.slice(0,f.selected.body_from)+'\\frac{a}{b}'+f.raw.slice(f.selected.body_to));assert.deepEqual(f.calls.map(c=>c.route),['/api/math/context','/api/math/parse']);assert.equal(f.calls[1].body.document_context.revision,sha(f.calls[0].body.content));
  await f.context.__inlineContext.render(f.d,0);assert.equal(f.d.input.attributes['aria-label'],'formula.source');assert.equal(f.d.spans.length,0);assert.equal(f.d.selected,null);
});
test('two ordinary edits and undo/redo always preserve the original document prefix and suffix',async()=>{
  const f=inlineFixture();await f.edit('z^2');await f.edit('q^3');assert.equal(f.d.undo.length,2);
  for(const call of f.calls.filter(c=>c.route==='/api/math/context'))assert.equal(call.body.content,f.raw.slice(0,f.selected.body_from)+(call===f.calls[0]?'z^2':'q^3')+f.raw.slice(f.selected.body_to));
  await f.context.__inlineContext.history(false);assert.equal(f.d.rich.source,'z^2');assert.equal(f.d.input.value,'z^2');await f.context.__inlineContext.history(true);assert.equal(f.d.rich.source,'q^3');assert.equal(f.getText(),f.raw);
  f.context.ReadMDInlineMath.cancel();assert.equal(f.changes.length,0);assert.equal(f.getText(),f.raw);
});
test('ordinary source commit writes only its original range and retains whitespace the user entered',async()=>{
  const f=inlineFixture();await f.edit(' z^2 ');assert.equal(f.d.rich.source,'z^2');assert.equal(f.d.tex,' z^2 ');assert.equal(await f.context.ReadMDInlineMath.commit(),true);
  assert.equal(f.changes.length,1);assert.equal(f.changes[0].changes.from,f.selected.from);assert.equal(f.changes[0].changes.to,f.selected.to);assert.equal(f.getText(),f.raw.slice(0,f.selected.body_from)+' z^2 '+f.raw.slice(f.selected.body_to));
});
test('a changed sibling occurrence rejects the ordinary candidate while keeping its input editable',async()=>{
  const f=inlineFixture({tamper:value=>{value.context.occurrences[0].source='wrong';}}),before=f.d.rich;await f.edit('z^2');assert.equal(f.d.rich,before);assert.equal(f.d.input.value,'z^2');assert.equal(f.d.input.disabled,false);assert.equal(f.d.valid,false);assert.equal(f.changes.length,0);assert.equal(f.calls.length,1);
});
test('late ordinary candidate responses lose to a newer source edit and cancellation writes nothing',async()=>{
  let finish;const f=inlineFixture({intercept:(call,value)=>call.route==='/api/math/context'&&call.body.content.includes('$$z^2$$')?new Promise(done=>finish=()=>done(ok(value))):ok(value)});
  const old=f.edit('z^2');while(!finish)await tick();await f.edit('q^3');finish();await old;assert.equal(f.d.rich.source,'q^3');assert.equal(f.d.tex,'q^3');assert.equal(f.d.undo.length,1);
  f.context.ReadMDInlineMath.cancel();assert.equal(f.changes.length,0);assert.equal(f.getText(),f.raw);
});
test('ordinary async preview and commit reject a changed document revision',async()=>{
  let finish;const f=inlineFixture({intercept:(call,value)=>call.route==='/api/math/parse'?new Promise(done=>finish=()=>done(ok(value))):ok(value)});
  const pending=f.edit('z^2');while(!finish)await tick();f.setText('newer source');finish();await pending;assert.equal(f.d.rich.source,'x^2');assert.equal(await f.context.ReadMDInlineMath.commit(),false);assert.equal(f.changes.length,0);assert.equal(f.getText(),'newer source');
});
test('ordinary control is a textarea with native newline value semantics and Enter remains multiline',async()=>{
  const production=read('assets/js/editor/formula-inline.js'),from=production.indexOf('    const input = document.createElement(contextOrdinary?'),to=production.indexOf('    const apply =',from);assert.ok(from>0&&to>from);
  const context={document:{createElement:htmlTextControl}};vm.createContext(context);vm.runInContext('function create(contextOrdinary){'+production.slice(from,to)+';return input;}',context);
  const ordinary=context.create(true),legacy=context.create(false);ordinary.value='a\r\nb\rc\nd';legacy.value=ordinary.value;assert.equal(ordinary.localName,'textarea');assert.equal(legacy.localName,'input');assert.equal(ordinary.value,'a\nb\nc\nd');assert.equal(legacy.value,'abcd');assert.equal(ordinary.rows,3);
  const keyFrom=production.indexOf("    host.addEventListener('keydown',async e => {"),keyTo=production.indexOf('    apply.onclick=',keyFrom);let handler,prevented=0,committed=0;
  Object.assign(context,{host:{addEventListener:(_type,fn)=>handler=fn},d:{contextOrdinary:true},input:ordinary,commit:async()=>committed++});vm.runInContext(production.slice(keyFrom,keyTo),context);
  const event=key=>({key,target:ordinary,preventDefault(){prevented++;},stopPropagation(){}});await handler(event('Enter'));await handler(event('Tab'));assert.equal(prevented,0);await handler({...event('Enter'),ctrlKey:true});assert.equal(committed,1);assert.equal(prevented,1);
});
test('CRLF multiline source opening and Apply without input preserve every original byte',async()=>{
  const f=inlineFixture({body:' \r\n\\frac{x}{y}\r\n+z^2\r\n '});await f.context.__inlineContext.render(f.d,0);assert.equal(f.d.input.value,'\\frac{x}{y}\n+z^2');assert.equal(f.d.contextInputBaseline,f.d.input.value);assert.equal(f.d.richDirty,false);
  assert.equal(await f.context.ReadMDInlineMath.commit(),true);assert.equal(f.calls.length,0);assert.equal(f.changes.length,0);assert.equal(f.getText(),f.raw);
});
test('multiline edits keep CRLF body bytes while semantic source comes from the registered occurrence',async()=>{
  const f=inlineFixture({body:'x\r\n+y',normalizeSemantic:true});await f.context.__inlineContext.render(f.d,0);await f.edit(' a\n+y ');assert.equal(f.d.valid,true);assert.equal(f.d.tex,' a\r\n+y ');assert.equal(f.d.rich.source,'a\n+y');assert.equal(f.d.input.value,' a\n+y ');
  const one=f.calls[0].body.content;assert.equal(one,f.raw.slice(0,f.selected.body_from)+' a\r\n+y '+f.raw.slice(f.selected.body_to));assert.equal(f.calls[1].body.source,'a\n+y');
  await f.edit(' b\n+y ');assert.equal(f.d.tex,' b\r\n+y ');await f.context.__inlineContext.history(false);assert.equal(f.d.tex,' a\r\n+y ');assert.equal(f.d.input.value,' a\n+y ');await f.context.__inlineContext.history(true);assert.equal(f.d.tex,' b\r\n+y ');
  assert.equal(await f.context.ReadMDInlineMath.commit(),true);assert.equal(f.getText(),f.raw.slice(0,f.selected.body_from)+' b\r\n+y '+f.raw.slice(f.selected.body_to));
});
test('late multiline candidate response cannot overwrite a newer textarea value or cancel result',async()=>{
  let finish;const f=inlineFixture({body:'x\r\n+y',normalizeSemantic:true,intercept:(call,value)=>call.route==='/api/math/context'&&call.body.content.includes('a\r\n+y')?new Promise(done=>finish=()=>done(ok(value))):ok(value)});
  await f.context.__inlineContext.render(f.d,0);const old=f.edit('a\n+y');while(!finish)await tick();await f.edit('b\n+y');finish();await old;assert.equal(f.d.tex,'b\r\n+y');assert.equal(f.d.input.value,'b\n+y');assert.equal(f.d.valid,true);
  f.context.ReadMDInlineMath.cancel();assert.equal(f.changes.length,0);assert.equal(f.getText(),f.raw);
});
test('textarea CSS is one appended bounded suffix and leaves the original panels bytes intact',()=>{
  let css=fs.readFileSync(path.join(root,'assets/css/panels.css'));
  // V006 batch2 adds an independently bounded AI-task panel suffix. Keep the
  // complete original prefix and the exact math-only suffix guard unchanged.
  // The authorized rename preview is independently bounded and cannot alter
  // a formula selector or the protected prefix.
  const refactor=Buffer.from('\n/* READMD_DOCUMENT_REFACTOR_BEGIN */'),refactorAt=css.indexOf(refactor);
  if(refactorAt>=0){
    assert.equal(css.indexOf(refactor,refactorAt+1),-1);
    const suffix=css.subarray(refactorAt).toString();assert.ok(suffix.endsWith('/* READMD_DOCUMENT_REFACTOR_END */\n'));
    for(const match of suffix.replace(/\/\*[\s\S]*?\*\//g,'').matchAll(/([^{}]+)\{/g))assert.match(match[1].trim(),/^\.document-refactor-/);
    css=css.subarray(0,refactorAt);
  }
  const tasks=Buffer.from('\n/* READMD_AI_TASK_STATUS_BEGIN */'),taskAt=css.indexOf(tasks);
  if(taskAt>=0){assert.equal(css.indexOf(tasks,taskAt+1),-1);assert.ok(css.toString().endsWith('/* READMD_AI_TASK_STATUS_END */\n'));css=css.subarray(0,taskAt);}
  const begin=Buffer.from('/* READMD_CONTEXT_SOURCE_BEGIN */'),end='/* READMD_CONTEXT_SOURCE_END */\n',at=css.indexOf(begin);assert.ok(at>0);assert.equal(css.indexOf(begin,at+1),-1);assert.equal(sha(css.subarray(0,at)),'bea3c2046704118937afc0eeddb13b68584ace5c79216cb5611b157ef619d146');
  const suffix=css.subarray(at).toString();assert.ok(suffix.endsWith(end));assert.equal(suffix.split(end).length,2);assert.match(suffix,/min-height:44px;resize:vertical/);assert.match(suffix,/textarea\.formula-inline-input:focus-visible\{outline:2px solid var\(--accent\);outline-offset:1px\}/);assert.equal(suffix.split('\n').length,5);
});
test('complete-source hooks run before splitting and preserve all protected preview bytes outside their bracket',()=>{
  const reader=read('assets/js/reader/render.js'),preview=read('assets/js/editor/preview.js');
  const start=reader.indexOf('async function renderContent(content, name)'),hook=reader.indexOf('ReadMDMathContext.prepareSource',start),split=reader.indexOf('const linesCount',start);assert.ok(start>=0&&hook>start&&hook<split);
  const begin='  // READMD_MATH_CONTEXT_BEGIN',end='  // READMD_MATH_CONTEXT_END';assert.equal(preview.split(begin).length,2);assert.equal(preview.split(end).length,2);
  // Review/save and the tested V006 rename/recovery fence are authorized;
  // the mathematical context bracket remains independently checked.
  const source=preview.slice(0,preview.indexOf(begin))+preview.slice(preview.indexOf(end)+end.length).replace(/^\r?\n/,'');
  const original=Buffer.from(source.replace(/\r\n?/g,'\n').replace(/\n/g,'\r\n'));assert.equal(crypto.createHash('sha256').update(original).digest('hex'),'c417cfc69977d7961d5f5745b25991fbf9636d68a9d512fcfdea158ddce34635');
  const render=preview.indexOf('async function renderPreview()');assert.ok(preview.indexOf('ReadMDMathContext.prepareSource',render)<preview.indexOf('readerPrepare(src)',render));
});
