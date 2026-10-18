'use strict';
// Pure view-state controls. No server, editor transaction, browser or build is
// started here; formal CSP and physical geometry still require native tests.
const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const root=path.resolve(__dirname,'../..');
const read=p=>fs.readFileSync(path.join(root,p),'utf8');
const reader=read('assets/js/reader/formula.js');
const editor=read('assets/js/editor/formula-inline.js');
const apiCodes=read('rust/readmd-kernel/src/api_codes.rs');
const codes=[...apiCodes.matchAll(/pub const WARN_MATH_EQARR_[A-Z_]+: &str = "([a-z_]+)";/g)].map(m=>m[1]);
const localeNames=fs.readdirSync(path.join(root,'assets/i18n')).filter(f=>f.endsWith('.json')&&f!=='meta.json').map(f=>f.slice(0,-5));
const dictionaries=Object.fromEntries(localeNames.map(name=>[name,JSON.parse(read('assets/i18n/'+name+'.json'))]));
const overflow={code:'math_equation_array_overflow',node_id:'original-run',required_points:123.4567,available_points:40.25};

class Node {
  constructor(name='span'){this.localName=name;this.children=[];this.attrs=new Map();this.dataset={};this.title='';this.textContent='';this.isConnected=true;this.style={};}
  get id(){return this.getAttribute('id')||'';}
  set id(value){this.setAttribute('id',value);}
  setAttribute(name,value){this.attrs.set(name,String(value));}
  getAttribute(name){return this.attrs.get(name)??null;}
  removeAttribute(name){this.attrs.delete(name);}
  append(...children){for(const child of children){child.parent=this;this.children.push(child);}}
  remove(){if(this.parent)this.parent.children=this.parent.children.filter(child=>child!==this);this.parent=null;}
  querySelectorAll(selector){
    assert.match(selector,/^\[[a-z-]+\]$/,'Source data must never become a selector');
    const attr=selector.slice(1,-1),result=[];
    for(const child of this.children){if(child.attrs.has(attr))result.push(child);result.push(...child.querySelectorAll(selector));}
    return result;
  }
  querySelector(selector){return this.querySelectorAll(selector)[0]||null;}
  clone(){const clone=new Node(this.localName);clone.attrs=new Map(this.attrs);clone.dataset={...this.dataset};clone.title=this.title;clone.textContent=this.textContent;for(const child of this.children)clone.append(child.clone());return clone;}
}
function environment(){
  const nodes=[],events=new Map(),frames=[],calls=[];
  const document={
    createElement:name=>new Node(name),createElementNS:(_ns,name)=>new Node(name),importNode:node=>node.clone(),
    querySelectorAll:selector=>{assert.equal(selector,'[data-readmd-math-warnings]');return nodes.filter(n=>n.dataset.readmdMathWarnings);},
    getElementById:id=>({id}),
  };
  const i18n={currentLang:'en',t(key,params={}){
    calls.push({key,params});const value=dictionaries[this.currentLang][key];assert.equal(typeof value,'string','Missing translation '+key);
    return value.replace(/\{([\w.-]+)\}/g,(_all,name)=>String(params[name]??'{'+name+'}'));
  }};
  class DOMParser {
    parseFromString(source,type){
      if(source==='bad-svg')return {querySelector:()=>null,documentElement:new Node('html')};
      assert.ok(source==='svg-ok'||source==='math-ok');
      const element=new Node(type==='image/svg+xml'?'svg':'math');
      if(source==='svg-ok'){const owner=new Node('g');owner.id=overflow.node_id;owner.setAttribute('data-math-node','r');element.append(owner);}
      return {querySelector:()=>null,documentElement:element};
    }
  }
  const context=vm.createContext({document,i18n,DOMParser,window:{addEventListener:(name,fn)=>events.set(name,fn)},
    requestAnimationFrame:fn=>frames.push(fn),queueMathOperation:fn=>fn(),MathJax:{mathml2svgPromise:async()=>new Node('mjx-container')}});
  const start=reader.indexOf('// Layout diagnostics are view state');
  const end=reader.indexOf('async function renderDslMath(',start);
  assert.ok(start>=0&&end>start);
  vm.runInContext(reader.slice(start,end)+'\nglobalThis.controls={message:mathLayoutDiagnosticMessage,apply:applyMathDocumentWarnings,render:renderMathDocument};',context);
  function flush(){let count=0;while(frames.length){assert.ok(count++<100,'Unbounded locale refresh');frames.shift()();}}
  return {nodes,events,frames,calls,i18n,document,flush,...context.controls};
}
function projectedNode(id=overflow.node_id){
  const node=new Node(),owner=new Node('g'),foreignTitle=new Node('title');
  node.title='Original accessible explanation';owner.id=id;owner.setAttribute('data-math-node','r');owner.setAttribute('aria-description','Original run description');
  foreignTitle.textContent='Foreign glyph title';owner.append(foreignTitle);node.append(owner);return {node,owner,foreignTitle};
}

test('actual layout metrics convert the padded client content box from CSS px to points',()=>{
  const start=reader.indexOf('function mathLayoutMetrics(owner,display){'),end=reader.indexOf('const mathLayoutRoots=',start);
  assert.ok(start>=0&&end>start);
  const context=vm.createContext({getComputedStyle:owner=>owner.computed});
  vm.runInContext(reader.slice(start,end)+'\nglobalThis.measure=mathLayoutMetrics;',context);
  // clientWidth includes 12px padding on each side, excludes a border and
  // scrollbar. A transformed bounding rect must not change layout capacity.
  const owner={clientWidth:344,computed:{fontSize:'16px',paddingLeft:'12px',paddingRight:'12px'},
    getBoundingClientRect(){throw Error('Transformed border box is not the content capacity');}};
  const plain=value=>JSON.parse(JSON.stringify(value));
  assert.deepEqual(plain(context.measure(owner,true)),{font_size:12,layout_width:240});
  assert.deepEqual(plain(context.measure(owner,false)),{font_size:12});
  owner.clientWidth=24;assert.deepEqual(plain(context.measure(owner,true)),{font_size:12});
  owner.clientWidth=24.8;assert.equal(context.measure(owner,true).layout_width,1);
  owner.clientWidth=344.4;assert.equal(context.measure(owner,true).layout_width,240.5);
  owner.clientWidth=50000;assert.equal(context.measure(owner,true).layout_width,20000);
  owner.computed.fontSize='1024px';assert.equal(context.measure(owner,true).font_size,512);
  owner.computed.fontSize='1px';assert.equal(context.measure(owner,true).font_size,4);
});

test('all 46 locales distinguish the ten diagnostics with exact numeric placeholders',()=>{
  assert.equal(codes.length,10);assert.equal(new Set(codes).size,10);assert.equal(localeNames.length,46);
  const backend=['math_equation_array.rs','math_layout.rs','omml.rs'].map(file=>read('rust/readmd-kernel/src/'+file)).join('\n');
  for(const code of codes)assert.ok(backend.includes('"'+code+'"'),'Unreachable diagnostic '+code);
  const placeholders=value=>[...value.matchAll(/\{([\w.-]+)\}/g)].map(m=>m[1]).sort();
  for(const locale of localeNames)for(const code of codes){
    const key='warn.'+code,value=dictionaries[locale][key];assert.equal(typeof value,'string',locale+' '+key);assert.ok(value.trim());
    assert.deepEqual(placeholders(value),/_(?:paragraph_alignment_)?overflow$/.test(code)?['available_points','required_points']:[],locale+' '+key);
    if(locale!=='en')assert.notEqual(value,dictionaries.en[key],locale+' copied English '+key);
  }
});

test('unknown or malformed codes never become a raw-code or source-HTML tooltip',()=>{
  const env=environment();
  for(const issue of [null,{}, {code:'math_equation_array_overflow_extra'},{code:'<img src=x onerror=secret>'}]){
    assert.equal(env.message(issue),dictionaries.en['formula.invalid']);
  }
  assert.ok(env.calls.every(call=>call.key==='formula.invalid'));
});

test('overflow numbers localize while invalid numbers and arbitrary parameters are rejected',()=>{
  const env=environment();
  for(const locale of ['en','zh-CN','ar']){
    env.i18n.currentLang=locale;const text=env.message({...overflow,source:'<secret>',required_points:123.4567});
    assert.ok(text.includes((123.4567).toLocaleString(locale,{maximumFractionDigits:3})));assert.ok(!text.includes('<secret>'));
  }
  for(const value of ['123',NaN,Infinity,-1,1e12+1,null,{}]){
    assert.equal(env.message({...overflow,required_points:value}),dictionaries.ar['formula.invalid']);
    assert.equal(env.message({...overflow,available_points:value}),dictionaries.ar['formula.invalid']);
  }
  assert.notEqual(env.message({...overflow,required_points:0,available_points:0}),dictionaries.ar['formula.invalid']);
});

test('source-bound messages aggregate safely and keep existing font diagnostics and SVG titles',()=>{
  const env=environment(),{node,owner,foreignTitle}=projectedNode('id\"][secret]');
  const input={font_warnings:[{code:'glyph_missing',font:'locked',character:'x'}],layout_diagnostics:[
    {...overflow,node_id:owner.id},{code:'math_equation_array_mathml_distribution',node_id:owner.id},
  ]};
  const original=JSON.stringify(input);env.apply(node,input);
  assert.equal(JSON.stringify(input),original);assert.equal(owner.children[0],foreignTitle);
  assert.equal(owner.querySelectorAll('[data-readmd-layout-warning-title]').length,1);
  assert.ok(node.title.includes(dictionaries.en['warn.math_equation_array_mathml_distribution']));
  assert.ok(owner.getAttribute('aria-description').includes('\n'));assert.equal(node.dataset.readmdMathWarnings,'true');
  assert.ok(env.calls.some(call=>call.key==='warn.math_glyph_missing'));
  assert.equal(owner.querySelector('[data-readmd-layout-warning-title]').textContent,owner.getAttribute('aria-description'));
});

test('clearing or updating projection diagnostics restores only owned view state',()=>{
  const env=environment(),{node,owner,foreignTitle}=projectedNode();
  env.apply(node,{layout_diagnostics:[overflow]});env.apply(node,{layout_diagnostics:[{code:'math_equation_array_marker_budget'}]});
  assert.equal(owner.getAttribute('aria-description'),'Original run description');assert.equal(owner.children.length,1);
  assert.equal(owner.children[0],foreignTitle);assert.ok(!node.title.includes('123.457'));
  env.apply(node,{layout_diagnostics:[overflow]});owner.setAttribute('aria-description','Updated by another accessible layer');
  env.apply(node,{});assert.equal(node.title,'Original accessible explanation');assert.equal(node.dataset.readmdMathWarnings,undefined);
  assert.equal(owner.getAttribute('aria-description'),'Updated by another accessible layer');assert.equal(owner.children.length,1);
});

test('language changes refresh connected projections in bounded batches and discard stale work',()=>{
  const env=environment();for(let index=0;index<35;index++){const {node}=projectedNode();env.apply(node,{layout_diagnostics:[overflow]});env.nodes.push(node);}
  env.nodes[34].isConnected=false;
  const english=env.nodes[0].title;
  env.i18n.currentLang='zh-CN';const chinese=env.message(overflow);env.events.get('readmd:language-changed')();assert.equal(env.frames.length,1);
  env.frames.shift()();assert.equal(env.nodes[0].title,chinese);assert.equal(env.nodes[16].title,english);assert.equal(env.frames.length,1);
  env.i18n.currentLang='ar';env.events.get('readmd:language-changed')();env.flush();
  for(const node of env.nodes.slice(0,34))assert.equal(node.title,env.message(overflow));
  assert.ok(env.nodes[34].title.startsWith('The equation array'));assert.equal(env.frames.length,0);
});

test('both actual render branches retain diagnostics without accepting a non-SVG vector root',async()=>{
  const env=environment();
  const value={vector_preview:'svg-ok',preview:'math-ok',layout_diagnostics:[overflow]};
  const vector=await env.render(value,true);assert.equal(vector.dataset.readmdMathRenderer,'rust');assert.equal(vector.title,env.message(overflow));
  assert.equal(vector.querySelector('[data-readmd-layout-warning-title]').textContent,env.message(overflow));
  const fallback=await env.render({...value,vector_preview:null},false);assert.equal(fallback.dataset.readmdMathRenderer,'mathjax');assert.equal(fallback.title,vector.title);
  await assert.rejects(env.render({...value,vector_preview:'bad-svg'},false),/invalid/);
});

test('the actual editor request cache keeps typed diagnostics separate from author source',async()=>{
  const start=editor.indexOf('  async function request(path, body, signal) {'),end=editor.indexOf('  function locate(',start);
  assert.ok(start>=0&&end>start);let sent;const source='<m:eqArr maxDist="1">author source</m:eqArr>';
  const response={ok:true,document:{source},preview:'math-ok',vector_preview:'svg-ok',layout_diagnostics:[overflow],font_warnings:[]};
  const context=vm.createContext({document:{getElementById:()=>({})},mathLayoutMetrics:()=>({font_size:24,layout_width:80}),
    apiFetch:async(url,options)=>{sent={url,options};return {ok:true,json:async()=>response};}});
  vm.runInContext(editor.slice(start,end)+'\nglobalThis.requestControl=request;',context);
  const input={source,dialect:'omml',display:true},original=JSON.stringify(input);
  const result=await context.requestControl('/api/math/parse',input);
  assert.equal(JSON.stringify(input),original);assert.equal(result.source,source);assert.equal(result.layoutDiagnostics,response.layout_diagnostics);
  assert.deepEqual(JSON.parse(sent.options.body),{...input,font_size:24,layout_width:80});
  response.ok=false;response.error_code='math_invalid_source';await assert.rejects(context.requestControl('/api/math/patch',input),/math_invalid_source/);
});
