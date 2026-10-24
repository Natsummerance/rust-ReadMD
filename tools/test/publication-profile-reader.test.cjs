'use strict';
// Pure interaction regressions against the real reader and sparse profile
// helper. These do not certify native PDF/Word layout or product API replies.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),crypto=require('node:crypto');
const ROOT=path.resolve(__dirname,'../..'),plain=value=>JSON.parse(JSON.stringify(value));
const readerSource=fs.readFileSync(path.join(ROOT,'assets/js/reader/publication.js'),'utf8');
const profileSource=fs.readFileSync(path.join(ROOT,'assets/js/editor/publication-profile.js'),'utf8');
const EN=JSON.parse(fs.readFileSync(path.join(ROOT,'assets/i18n/en.json'),'utf8'));
class Element {
  constructor(tag){this.tagName=tag.toUpperCase();this.children=[];this.dataset={};this.style={};this.attributes={};this.className='';this.textContent='';this.classList={add:name=>{this.className+=' '+name;}};}
  append(...children){this.children.push(...children);}replaceChildren(...children){this.children=[...children];this.textContent='';}
  setAttribute(name,value){this.attributes[name]=String(value);}querySelectorAll(){return [];}
}
const deferred=()=>{let resolve;const promise=new Promise(done=>{resolve=done;});return{promise,resolve};};
const reply=value=>({ok:true,json:async()=>value});
function environment({source='# Ordinary\n\nText[^a].\n\n[^a]: Current note.\n',last={},sourceOptions={},profileReply,fetch,prepareFigures}={}) {
  const tab={id:'a',content:source,original:source},pane=new Element('article'),calls=[],figures=[],toasts=[];
  const state={activeTabId:'a',editing:true,file:'Z:/owned/source.md',dir:'Z:/owned',sourceName:'source.md',original:source,export:{options:plain(last)}};
  let raw=source,active=true;
  const doc=text=>({toString:()=>text}),view={state:{doc:doc(source)}};
  const context={window:{},state,cmView:view,AbortController,AbortSignal,Date,setTimeout,clearTimeout,crypto,
    document:{createElement:tag=>new Element(tag)},
    getActiveTab:()=>context.tab,getEditContent:()=>raw,
    i18n:{t:key=>EN[key]??key},showToast:value=>toasts.push(value),apiMessage:data=>EN['error.'+data?.error_code]||'Preparation failed',
    prepareExportFigures:async(content,options,signal)=>{figures.push({content,options:plain(options),signal});return prepareFigures?prepareFigures(content,options,signal):options;},
    apiFetch:async(route,init)=>{const body=JSON.parse(init.body);calls.push({route,body,signal:init.signal});if(fetch){const result=await fetch(route,body,init);if(result)return result;}
      if(route==='/api/publication/profile')return reply(profileReply??{ok:true,options:sourceOptions,diagnostics:[]});
      if(route==='/api/academic/document')return reply({ok:true,content:body.content,diagnostics:[],warnings:[],projection:{revision:'reader-r',citations:[]}});
      if(route==='/api/export/preview')return reply({ok:true,pages:1,firstPage:'AA==',sourceMap:{revision:'current',pages:[[600,800]],nodes:[],hits:[]}});
      if(route==='/api/task/cancel')return reply({ok:true});throw Error('Unexpected route '+route);
    }};
  context.tab=tab;context.window.ReadMDTask={loading(){}};
  vm.createContext(context);vm.runInContext(profileSource,context,{filename:'publication-profile.js'});vm.runInContext(readerSource,context,{filename:'publication.js'});
  const reader=context.window.ReadMDPublication;
  return {context,state,tab,pane,calls,figures,toasts,reader,view,
    render(){const requested=view.state.doc.toString();return reader.renderPaged(pane,{view,tab:'a',current:()=>active&&raw===requested&&context.cmView===view&&state.activeTabId==='a'});},
    setSource(text){raw=text;view.state.doc=doc(text);tab.content=text;},
    replaceDocument(){view.state.doc=doc(raw);},setCurrent(value){active=value;},
  };
}

test('profile-only block, flow, quoted, BOM/CRLF front matter and numbering import trigger reader preparation',async()=>{
  for(const source of [
    '---\nreadmd:\n  publication: {version: 1, future: 7}\n---\nVisible\n',
    '\uFEFF---\r\n{"readmd": {"publication": {"version": 1, "future": "007"}}}\r\n---\r\nVisible😀\r\n',
    "---\n'readmd': {publication: {version: 1, future: 7}}\n...\nVisible\n",
    'Visible😀\n\n<!--readmd-numbering-import:v1 []-->\n',
  ]){
    const e=environment({source});assert.equal(await e.reader.prepare(source,e.state.file),source);assert.equal(e.calls.length,1,source);assert.equal(e.calls[0].route,'/api/academic/document');
  }
});

test('ordinary content and a profile code example avoid extra reader requests',async()=>{
  for(const source of ['# Plain\n\nText.\n','```yaml\nreadmd: {publication: {version: 1}}\n```\n']){
    const e=environment({source});assert.equal(await e.reader.prepare(source,e.state.file),source);assert.equal(e.calls.length,0);
  }
});

test('reader exposes source diagnostics and localizes existing error codes without hiding the body',async()=>{
  const source='---\nreadmd: {publication: {version: 1, future: 7}}\n---\nVisible😀\n';
  const diagnostic={code:'publication_metadata_field',path:'source.readmd.publication.future',key:'source.readmd.publication.future',from:14,to:62,revision:'current'};
  const e=environment({source,fetch:async route=>route==='/api/academic/document'?reply({ok:true,content:source,diagnostics:[diagnostic],warnings:['publication_metadata_field: source.readmd.publication.future'],projection:{revision:'current',citations:[]}}):null});
  assert.equal(await e.reader.prepare(source,e.state.file),source);assert.deepEqual(plain(e.reader.diagnostics()),[diagnostic]);assert.deepEqual(e.toasts,[EN['error.publication_metadata_field']]);
});

test('paged preview merges Last below source before preparing figures and keeps the Typst default',async()=>{
  const source='---\nreadmd: {publication: {version: 1, footnotes: {start: 4, size: 12}}}\n---\nText[^a].\n\n[^a]: Note\n';
  const last={page:{columns:2},footnotes:{start:23,size:8,lineHeight:1.5},pdf:{engine:'xelatex'}},original=plain(last);
  const e=environment({source,last,sourceOptions:{footnotes:{start:4,size:12}}});await e.render();
  assert.deepEqual(e.calls.map(call=>call.route),['/api/publication/profile','/api/export/preview']);
  assert.deepEqual(e.calls[0].body,{content:source});assert.equal(e.figures.length,1);
  const options=e.calls[1].body.options;assert.equal(options.footnotes.start,4);assert.equal(options.footnotes.size,12);assert.equal(options.footnotes.lineHeight,1.5);assert.equal(options.page.columns,2);assert.equal(options.pdf.engine,'typst');
  assert.deepEqual(e.figures[0].options,options);assert.deepEqual(e.state.export.options,original);assert.deepEqual(last,original);
  assert.equal(e.calls[0].signal,e.figures[0].signal);assert.equal(e.calls[0].signal,e.calls[1].signal);
});

test('paged source kinds replace a remembered spacing union without saving inherited values',async()=>{
  const last={footnotes:{start:23,lineSpacing:{mode:'exact',pt:14}}};
  const sourceOptions={footnotes:{kinds:{footnote:{start:4,lineSpacing:{mode:'multiple',multiple:2}}},kindByLabel:{a:'footnote'}}};
  const e=environment({last,sourceOptions});await e.render();
  const options=e.calls.find(call=>call.route==='/api/export/preview').body.options;
  assert.deepEqual(options.footnotes.kinds.footnote.lineSpacing,{mode:'multiple',multiple:2});assert.equal(options.footnotes.kinds.footnote.start,4);assert(!Object.hasOwn(options.footnotes.kinds.footnote.lineSpacing,'pt'));assert.deepEqual(e.state.export.options,last);
});

test('no source profile preserves remembered defaults and does not introduce notes',async()=>{
  for(const last of [{page:{columns:2}},{footnotes:{start:23,size:8}}]){
    const e=environment({last});await e.render();const options=e.calls.find(call=>call.route==='/api/export/preview').body.options;
    if(last.footnotes)assert.deepEqual(options.footnotes,last.footnotes);else assert(!Object.hasOwn(options,'footnotes'));
    assert.deepEqual(e.state.export.options,last);
  }
});

test('profile errors and unknown settings block preparation before figures or the PDF request',async()=>{
  for(const profileReply of [
    {ok:false,error_code:'publication_note_policy_invalid'},
    {ok:true,options:{footnotes:{start:4}},diagnostics:[{code:'publication_metadata_field',path:'source.readmd.publication.future'}]},
  ]){
    const e=environment({profileReply});await e.render();assert.equal(e.figures.length,0);assert.deepEqual(e.calls.map(call=>call.route),['/api/publication/profile']);
    assert.equal(e.pane.children[0].role,'alert');assert.equal(e.pane.children[0].textContent,EN['error.'+(profileReply.error_code||profileReply.diagnostics[0].code)]);
  }
});

test('source, document identity, tab identity, view, epoch and cancellation changes discard delayed profile replies',async t=>{
  const cases={source:e=>e.setSource('Changed current source'),document:e=>e.replaceDocument(),tab:e=>{e.context.tab={...e.tab};},view:e=>{e.context.cmView={...e.view};},epoch:e=>e.setCurrent(false),cancel:e=>e.reader.stopPaged()};
  for(const [name,mutate]of Object.entries(cases))await t.test(name,async()=>{
    const waiting=deferred(),e=environment({fetch:async route=>route==='/api/publication/profile'?waiting.promise:null});
    const pending=e.render();assert.equal(e.calls[0].route,'/api/publication/profile');mutate(e);waiting.resolve(reply({ok:true,options:{footnotes:{start:4}},diagnostics:[]}));await pending;
    assert.equal(e.figures.length,0);assert(!e.calls.some(call=>call.route==='/api/export/preview'));assert(!e.pane.children.some(child=>child.role==='alert'));if(name==='cancel')assert.equal(e.calls[0].signal.aborted,true);
  });
});

test('a stale source after figure preparation cannot submit or publish a PDF preview',async()=>{
  const waiting=deferred(),e=environment({prepareFigures:async()=>waiting.promise});const pending=e.render();
  await new Promise(resolve=>setImmediate(resolve));assert.equal(e.figures.length,1);e.setSource('Changed after figures');waiting.resolve({});await pending;assert(!e.calls.some(call=>call.route==='/api/export/preview'));
});

test('a superseding preview aborts the old profile fetch and only current source options reach PDF',async()=>{
  const waiting=deferred();let first=true;
  const e=environment({fetch:async route=>{if(route!=='/api/publication/profile')return null;if(first){first=false;return waiting.promise;}return reply({ok:true,options:{footnotes:{start:7}},diagnostics:[]});}});
  const old=e.render();e.setSource('Current source for second request');await e.render();waiting.resolve(reply({ok:true,options:{footnotes:{start:99}},diagnostics:[]}));await old;
  assert.equal(e.calls[0].signal.aborted,true);const previews=e.calls.filter(call=>call.route==='/api/export/preview');assert.equal(previews.length,1);assert.equal(previews[0].body.content,'Current source for second request');assert.equal(previews[0].body.options.footnotes.start,7);assert.equal(e.figures.length,1);
});
