'use strict';
// Pure DOM/editor transaction contracts. No GUI, product API or Office is run.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ROOT=path.resolve(__dirname,'../..'),plain=value=>JSON.parse(JSON.stringify(value));
class Element {
  constructor(tag){this.tagName=tag.toUpperCase();this.children=[];this.parentElement=null;this.dataset={};this.attributes={};this.style={};this.listeners={};this.value='';this.checked=false;this.disabled=false;this.textContent='';this.open=false;}
  append(...nodes){for(const node of nodes){if(node.parentElement)node.parentElement.children=node.parentElement.children.filter(child=>child!==node);this.children.push(node);node.parentElement=this;}}
  replaceChildren(...nodes){for(const node of this.children)node.parentElement=null;this.children=[];this.append(...nodes);}
  setAttribute(name,value){this.attributes[name]=String(value);}addEventListener(name,fn){(this.listeners[name]||=[]).push(fn);}
  emit(name){for(const fn of this.listeners[name]||[])fn({target:this,preventDefault(){}});}
  get isConnected(){return this.tagName==='ROOT'||Boolean(this.parentElement?.isConnected);}
}
function replace(source,edits){let out='',at=0;for(const edit of edits){out+=source.slice(at,edit.from)+edit.insert;at=edit.to;}return out+source.slice(at);}
function deferred(){let resolve;const promise=new Promise(done=>{resolve=done;});return{promise,resolve};}
function environment({source='# Title\n',config=null,respond,ensureEditor=true}={}){
  const root=new Element('root'),host=new Element('div');root.append(host);let sourceValue=source,text=source.replace(/\r\n?/g,'\n'),current=true,language='one';const calls=[],dispatches=[],history=[],events={};
  const view={state:{doc:{toString:()=>text}},dispatch(spec){dispatches.push(spec);history.push(text);text=replace(text,spec.changes);sourceValue=text;},focus(){}};
  const get={ok:true,revision:'r0',config:plain(config),plan:null};
  const binding={view,source,current:()=>current};
  const context={window:{addEventListener:(name,fn)=>{(events[name]||=[]).push(fn);}},document:{createElement:tag=>new Element(tag)},AbortController,console,
    apiFetch:async(route,request)=>{const body=JSON.parse(request.body);calls.push({route,request,body});const result=respond?await respond(body,request):body.action==='get'?get:{ok:true,transaction:{base_revision:'r0',origin:'editor',edits:[]},result:{content:body.content,revision:'r0',changed:false,inverse:[]},config:body.config,plan:null};return{ok:result.ok,json:async()=>result};}};
  context.window.i18n={t:(key,params)=>language+':'+key+(params?.level?' '+params.level:'')};
  context.window.ReadMDCodeMirror={Transaction:{userEvent:{of:value=>({userEvent:value})}}};vm.createContext(context);
  for(const file of ['assets/js/editor/publication-metadata.js','assets/js/editor/publication-numbering.js'])vm.runInContext(fs.readFileSync(path.join(ROOT,file),'utf8'),context,{filename:file});
  const options={source,getSource:()=>sourceValue,isCurrent:()=>current,onApplied(){},...(ensureEditor?{ensureEditor:async()=>binding}:{})};
  const controller=context.window.ReadMDPublicationNumbering.mount(host,options);
  return{context,api:context.window.ReadMDPublicationNumbering,controller,host,calls,dispatches,view,binding,get,options,
    edit(path,value){const control=controller.controls.get(path);assert(control,'Missing control '+path);if(control.type==='checkbox')control.checked=value;else control.value=String(value);control.emit(control.tagName==='SELECT'||control.type==='checkbox'?'change':'input');return control;},
    setSource(value){sourceValue=value;text=value.replace(/\r\n?/g,'\n');},setCurrent(value){current=value;},getText:()=>text,
    undo(){assert(history.length);text=history.pop();sourceValue=text;},language(value){language=value;for(const fn of events['readmd:language-changed']||[])fn();}};
}
const numbered={headings:{levels:[{format:'decimal',start:1,restartAfter:0}],separator:'.',suffix:'space'},captions:{}};

test('native per-level text, suffix, legal-format and zero starts survive unrelated UI edits',async()=>{
  const config={headings:{levels:[{format:'decimal',start:0,text:'Part %1)',suffix:'tab'},{format:'upperLetter',start:28,text:'[%2/%1/%2] %x',suffix:'nothing',isLegal:true}]},captions:{table:{start:0,sequence:'Native_Table'}}},e=environment({config});
  await e.controller.load();assert.deepEqual(plain(e.controller.read()),config);e.edit('captions.table.captionSeparator',': ');
  const result=plain(e.controller.read());assert.equal(result.headings.levels[0].start,0);assert.equal(result.headings.levels[0].text,'Part %1)');assert.equal(result.headings.levels[1].suffix,'nothing');assert.equal(result.headings.levels[1].isLegal,true);assert.equal(result.captions.table.sequence,'Native_Table');
  e.edit('levels.0.customText',false);assert.equal(Object.hasOwn(e.controller.read().headings.levels[0],'text'),false);
  e.edit('levels.1.customSuffix',false);assert.equal(Object.hasOwn(e.controller.read().headings.levels[1],'suffix'),false);
  e.edit('captions.table.sequence','Table \\r 4');assert.throws(()=>e.controller.read(),/publication_numbering_invalid/);
});

test('one edited native control preserves untouched omission, explicit null and explicit false',async()=>{
  const config={headings:{levels:[{start:0,text:null,isLegal:false},{format:'upperLetter',start:28,text:'[%2]',suffix:'tab'}]},captions:{table:{prefix:null,start:4,sequence:'表'}}},e=environment({config});
  await e.controller.load();e.edit('levels.0.start','2');
  assert.deepEqual(plain(e.controller.read()),{...config,headings:{levels:[{start:2,text:null,isLegal:false},config.headings.levels[1]]}});
  e.edit('captions.table.captionSeparator',': ');assert.equal(e.controller.read().captions.table.prefix,null);assert.equal(e.controller.read().captions.table.sequence,'表');
});

test('mount is collapsed, lazy, and never materializes default source settings',async()=>{
  const e=environment();assert.equal(e.controller.root.open,false);assert.equal(e.calls.length,0);assert.equal(e.dispatches.length,0);assert.equal(await e.controller.load(),true);assert.equal(e.calls.length,1);assert.equal(e.calls[0].body.action,'get');assert.equal(e.controller.read(),null);assert.equal(e.dispatches.length,0);assert.equal(await e.controller.apply(),true);assert.equal(e.calls[1].body.config,null);assert.equal(e.dispatches.length,0);
});

test('all six heading levels and three caption kinds are actual accessible controls',async()=>{
  const e=environment();await e.controller.load();for(let level=0;level<6;level++)for(const key of ['format','start','restartAfter'])assert(e.controller.controls.get(`levels.${level}.${key}`).attributes['aria-labelledby']);
  for(const kind of ['equation','figure','table'])for(const key of ['scope','chapterLevel','format','start','customPrefix','prefix','separator','labelSeparator','captionSeparator'])assert(e.controller.controls.has(`captions.${kind}.${key}`));
  assert.equal(e.controller.root.children[1].children.at(-1).children[0].type,'button');assert.equal(e.controller.root.children[1].children.at(-1).children[1].type,'button');for(const control of e.controller.controls.values())assert.equal(control.dataset.k,undefined);
});

test('opening existing rules preserves omitted defaults and source configuration exactly',async()=>{
  const config={headings:{levels:[{}]},captions:{equation:{scope:'document',prefix:''}}},e=environment({config});await e.controller.load();assert.deepEqual(plain(e.controller.read()),config);assert.equal(e.controller.controls.get('captions.equation.customPrefix').checked,true);assert.equal(e.controller.controls.get('captions.equation.prefix').value,'');assert.equal(e.controller.controls.get('levels.1.start').disabled,true);
});

test('chapter scope requires a configured real heading level and retains invalid drafts',async()=>{
  const e=environment();await e.controller.load();e.edit('captions.equation.scope','chapter');assert.throws(()=>e.controller.read(),/publication_numbering_invalid/);assert.equal(await e.controller.apply(),false);assert.equal(e.calls.length,1);assert.equal(e.controller.controls.get('captions.equation.scope').value,'chapter');e.edit('depth','2');e.edit('captions.equation.chapterLevel','2');assert.equal(e.controller.read().captions.equation.chapterLevel,2);e.edit('levels.1.start','10001');assert.equal(await e.controller.apply(),false);assert.equal(e.controller.controls.get('levels.1.start').value,'10001');assert.equal(e.calls.length,1);
});

test('native restart choices and literal separators are validated without clamp or fallback',async()=>{
  const e=environment();await e.controller.load();e.edit('depth','6');e.edit('levels.5.format','upperRoman');e.edit('levels.5.start','27');e.edit('levels.5.restartAfter','0');e.edit('separator','-');let config=e.controller.read();assert.equal(config.headings.levels[5].start,27);assert.equal(config.headings.levels[5].restartAfter,0);assert.equal(config.headings.levels[5].format,'upperRoman');e.edit('separator','%2');assert.throws(()=>e.controller.read(),/publication_numbering_invalid/);assert.equal(e.controller.controls.get('separator').value,'%2');
});

test('custom empty caption label differs from the source-language default label',async()=>{
  const e=environment();await e.controller.load();e.edit('captions.figure.scope','document');assert.equal(Object.hasOwn(e.controller.read().captions.figure,'prefix'),false);e.edit('captions.figure.customPrefix',true);e.edit('captions.figure.prefix','');e.edit('captions.figure.labelSeparator','');assert.equal(e.controller.read().captions.figure.prefix,'');assert.equal(e.controller.read().captions.figure.labelSeparator,'');e.edit('captions.figure.customPrefix',false);assert.equal(Object.hasOwn(e.controller.read().captions.figure,'prefix'),false);
});

test('a source configuration change applies as one exact CM transaction and one inverse step',async()=>{
  const source='\uFEFF---\r\ntitle: 😀\r\n---\r\n# Title\r\n',from=source.indexOf('# Title'),insert='<!-- readmd-numbering:v1 {"headings":{"levels":[{}]}} -->\r\n\r\n';
  const e=environment({source,respond:body=>body.action==='get'?{ok:true,revision:'r0',config:null,plan:null}:{ok:true,transaction:{base_revision:'r0',origin:'editor',edits:[{from,to:from,insert}]},result:{content:replace(source,[{from,to:from,insert}]),revision:'r1',changed:true,inverse:[]},config:body.config,plan:null}});
  await e.controller.load();e.edit('depth',1);assert.equal(await e.controller.apply(),true);assert.equal(e.dispatches.length,1);assert.equal(e.dispatches[0].annotations.userEvent,'input.publication.numbering');assert.equal(e.getText(),replace(source,[{from,to:from,insert}]).replace(/\r\n/g,'\n'));e.undo();assert.equal(e.getText(),source.replace(/\r\n/g,'\n'));
});

test('clear sends config null without changing local export options',async()=>{
  const e=environment({config:numbered});await e.controller.load();assert.equal(await e.controller.clear(),true);assert.equal(e.calls.at(-1).body.config,null);assert.equal(Object.hasOwn(e.context,'state'),false);assert.equal(e.dispatches.length,0);
});

test('a source change during get prevents controls from attaching to newer content',async()=>{
  const wait=deferred(),e=environment({respond:()=>wait.promise});const pending=e.controller.load();e.setSource('# Newer\n');wait.resolve({ok:true,revision:'old',config:numbered,plan:null});assert.equal(await pending,false);assert.equal(e.controller.controls.size,0);assert.equal(e.dispatches.length,0);
});

test('tab or editor changes during apply reject the pending callback',async()=>{
  const wait=deferred(),e=environment({config:numbered,respond:body=>body.action==='get'?{ok:true,revision:'r0',config:numbered,plan:null}:wait.promise});await e.controller.load();const pending=e.controller.apply();await new Promise(resolve=>setImmediate(resolve));assert.equal(e.calls.length,2);e.setCurrent(false);wait.resolve({ok:true,transaction:{base_revision:'r0',edits:[{from:0,to:0,insert:'OLD'}]},result:{content:'OLD# Title\n',revision:'r1',changed:true},config:numbered});assert.equal(await pending,false);assert.equal(e.dispatches.length,0);
});

test('two rapid apply actions issue one request and cannot duplicate source edits',async()=>{
  const wait=deferred(),e=environment({respond:body=>body.action==='get'?{ok:true,revision:'r0',config:null,plan:null}:wait.promise});await e.controller.load();const first=e.controller.apply(),second=e.controller.apply();assert.equal(await second,false);await new Promise(resolve=>setImmediate(resolve));assert.equal(e.calls.length,2);wait.resolve({ok:true,transaction:{base_revision:'r0',edits:[]},result:{content:'# Title\n',revision:'r0',changed:false},config:null});assert.equal(await first,true);assert.equal(e.dispatches.length,0);
});

test('missing editor binding is actionable and never sends an apply request',async()=>{
  const e=environment({ensureEditor:false});await e.controller.load();e.edit('depth',1);assert.equal(await e.controller.apply(),false);assert.equal(e.calls.length,1);assert.equal(e.dispatches.length,0);
});

test('wrong base revision, split UTF16 and divergent returned content never reach CM',()=>{
  const e=environment(),source='😀\r\nBody';for(const data of [
    {transaction:{base_revision:'wrong',edits:[]},result:{content:source,revision:'r0',changed:false}},
    {transaction:{base_revision:'r0',edits:[{from:1,to:1,insert:'X'}]},result:{content:source,revision:'r1',changed:true}},
    {transaction:{base_revision:'r0',edits:[{from:0,to:0,insert:'X'}]},result:{content:'different',revision:'r1',changed:true}},
    {transaction:{base_revision:'r0',edits:[]},result:{content:source,revision:'r0',changed:true}}
  ])assert.throws(()=>e.api.editorChanges(source,data,'r0'));assert.equal(e.dispatches.length,0);
});

test('language refresh translates labels while preserving an invalid typed draft',async()=>{
  const e=environment();await e.controller.load();e.edit('depth',1);e.edit('levels.0.start','99999');const before=e.controller.root.children[0].textContent;e.language('two');assert.notEqual(e.controller.root.children[0].textContent,before);assert.equal(e.controller.controls.get('levels.0.start').value,'99999');assert.throws(()=>e.controller.read());assert.equal(e.calls.length,1);
});

test('destroy aborts its owned request and never permits a late edit',async()=>{
  const wait=deferred(),e=environment({respond:()=>wait.promise});const pending=e.controller.load();e.controller.destroy();assert.equal(e.calls[0].request.signal.aborted,true);wait.resolve({ok:true,revision:'r0',config:numbered,plan:null});assert.equal(await pending,false);assert.equal(await e.controller.apply(),false);assert.equal(e.dispatches.length,0);
});

test('numbering labels have a fixed 30-key catalog and no hardcoded English fallback',()=>{
  const e=environment();assert.equal(e.api.keys.length,30);const source=fs.readFileSync(path.join(ROOT,'assets/js/editor/publication-numbering.js'),'utf8');assert(!/english\s*=|fallbackDict|\|\|\s*['"](?:Numbering|Heading|Caption)/u.test(source));
});
