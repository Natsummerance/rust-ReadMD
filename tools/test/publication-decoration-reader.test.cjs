'use strict';
// Controlled DOM ownership/restoration regressions. Rust tests certify the
// actual API payload; these tests do not claim native visual geometry.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const script=fs.readFileSync(path.join(__dirname,'../../assets/js/reader/publication.js'),'utf8');
const key='reader-decoration-current-revision';
const source='---\r\nbibliography: refs.bib\r\n---\r\n<!-- readmd-numbering:v1 {"headings":{"levels":[{}]}} -->\r\n\r\n# Original heading\r\n\r\n<!-- readmd:layout v1 {"paragraph":{"shading":{"fill":"DCE6F1"},"border":{"left":{"color":"4472C4","widthPt":2.6,"spacePt":4}}}} -->\r\n\r\n😀 中文 e\u0301 👩\u200d💻 [@sample] **bold** `code`.\r\n';
const plain=value=>JSON.parse(JSON.stringify(value));
const rule=()=>({'background-color':'#DCE6F1',...Object.fromEntries([
  ['top','#112233','0.25pt','0pt'],['left','#4472C4','2.6pt','4pt'],['bottom','#445566','12pt','31pt'],['right','#778899','1.25pt','7pt']
].flatMap(([side,color,width,gap])=>[[`border-${side}-color`,color],[`border-${side}-style`,'solid'],[`border-${side}-width`,width],[`padding-${side}`,gap]]))});

class Style {
  constructor(){this.values=new Map();}
  setProperty(property,value,priority=''){this.values.set(property,[String(value),priority]);}
  getPropertyValue(property){return this.values.get(property)?.[0]||'';}
  getPropertyPriority(property){return this.values.get(property)?.[1]||'';}
  removeProperty(property){this.values.delete(property);}
}
class Node {
  constructor(tag,text=''){this.nodeType=tag?1:3;this.tagName=tag?.toUpperCase();this.nodeValue=text;this.childNodes=[];this.parentNode=null;this.dataset={};this.style=new Style();this.className='';this.attributes={};this.listeners={};this.classList={contains:name=>this.className.split(/\s+/).includes(name)};}
  get children(){return this.childNodes.filter(node=>node.nodeType===1);}
  get textContent(){return this.nodeType===3?this.nodeValue:this.childNodes.map(node=>node.textContent).join('');}
  set textContent(value){for(const child of this.childNodes)child.parentNode=null;this.childNodes=[];if(String(value))this.append(new Node(null,String(value)));}
  get nextElementSibling(){const siblings=this.parentNode?.children||[];return siblings[siblings.indexOf(this)+1]||null;}
  get isConnected(){return this.tagName==='ROOT'||Boolean(this.parentNode?.isConnected);}
  append(...nodes){for(const node of nodes){node.remove();node.parentNode=this;this.childNodes.push(node);}}
  prepend(node){node.remove();node.parentNode=this;this.childNodes.unshift(node);}
  remove(){if(this.parentNode)this.parentNode.childNodes=this.parentNode.childNodes.filter(node=>node!==this);this.parentNode=null;}
  contains(node){for(;node;node=node.parentNode)if(node===this)return true;return false;}
  setAttribute(name,value){this.attributes[name]=String(value);}
  addEventListener(name,callback){this.listeners[name]=callback;}
  matches(selector){return selector.split(',').some(part=>{
    const tag=/^[a-z][\w-]*/i.exec(part);if(tag&&this.tagName!==tag[0].toUpperCase())return false;
    for(const match of part.matchAll(/\.([\w-]+)/g))if(!this.classList.contains(match[1]))return false;
    for(const match of part.matchAll(/\[([^=\]]+)(?:="([^"]*)")?\]/g)){
      const name=match[1],value=name.startsWith('data-')?this.dataset[name.slice(5).replace(/-([a-z])/g,(_,letter)=>letter.toUpperCase())]:this.attributes[name];
      if(value===undefined||match[2]!==undefined&&value!==match[2])return false;
    }
    return true;
  });}
  closest(selector){for(let node=this;node;node=node.parentNode)if(node.matches(selector))return node;return null;}
  querySelectorAll(selector){const found=[];const visit=node=>{for(const child of node.children){if(child.matches(selector))found.push(child);visit(child);}};visit(this);return found;}
  focus(){}
}
function fixture(content=source,layout={[key]:rule()},revision='current'){
  const from=content.indexOf('[@sample]'),span={from,to:from+9,source:'[@sample]'};
  return {ok:true,content:'Rust prepared display',layout,warnings:[],diagnostics:[],source_labels:[],
    source_numbering:{headings:[{id:'stable',level:1,numbering:{text:'1',suffix:' '}}]},
    source_citations:[{...span,revision}],projection:{revision,source_utf16:content.length,citations:[{source:{span},rich:[{type:'text',text:'(Sample, 2026)'}],rendered:true}]}};
}
function environment({editing=true,inline=false,fetch}={}){
  const root=new Node('root'),pane=new Node('article'),heading=new Node('h1'),target=new Node('p'),marker=new Node(inline?'span':'div');root.append(pane);heading.textContent='Original heading';pane.append(heading);
  marker.dataset.readmdLayout=key;
  if(inline){marker.dataset.readmdLayoutInline='true';pane.append(target);target.append(marker);}else pane.append(marker,target);
  const code=new Node('code'),citation=new Node('span'),math=new Node('mjx-container'),strong=new Node('strong');code.textContent='code';citation.className='readmd-citation';citation.textContent='(Sample, 2026)';math.textContent='generated formula';strong.textContent='bold';
  const content=inline?marker:target;content.append(new Node(null,'😀 中文 e\u0301 👩\u200d💻 '),citation,strong,code,math);
  target.style.setProperty('background-color','navy','important');target.style.setProperty('border-left-width','9pt','important');target.style.setProperty('padding-left','3px','important');target.style.setProperty('color','green','important');
  const originalStyles=[...target.style.values].map(([property,value])=>[property,[...value]]),toasts=[],selections=[],calls=[];
  const tab={id:'a',content:source,original:source,isDirty:false},state={editing,activeTabId:'a',file:'Z:/owned.md',original:source,fixed:source,pagination:{}};
  let raw=source,response=fixture(),now=0,fetcher=fetch;
  const document={createElement:tag=>new Node(tag)};
  const view={state:{doc:{toString:()=>raw.replace(/\r\n?/g,'\n')}},dispatch:change=>selections.push(change),focus(){}};
  const context={window:{},document,state,cmView:editing?view:null,editorOpenEpoch:0,getActiveTab:()=>context.tab,getEditContent:()=>raw,AbortSignal,Date:{now:()=>now},setTimeout,clearTimeout,
    showToast:value=>toasts.push(value),apiMessage:()=> 'Preparation failed',i18n:{t:key=>key},
    apiFetch:async(route,request)=>{const body=JSON.parse(request.body);calls.push(body);return fetcher?fetcher(body):{ok:true,json:async()=>response};}};
  context.tab=tab;context.toggleEdit=async()=>{state.editing=true;context.cmView=view;context.editorOpenEpoch++;};
  vm.runInNewContext(script,context,{filename:'publication.js'});
  const reader=context.window.ReadMDPublication;
  function citationData(data){const span=data.projection.citations[0].source.span;Object.assign(citation.dataset,{readmdCitationIndex:'0',readmdSourceFrom:String(span.from),readmdSourceTo:String(span.to),readmdSourceRevision:data.projection.revision});}
  citationData(response);
  return {context,reader,root,pane,heading,target,marker,content,code,citation,math,strong,tab,state,toasts,selections,calls,originalStyles,
    setSource(value){raw=value;state.original=value;state.fixed=value;context.tab.content=value;},setResponse(value){response=value;citationData(value);},setFetch(value){fetcher=value;},expire(){now+=2100;},
    prepare:()=>reader.prepare(raw,state.file),citationData};
}
function assertRestored(e){assert.deepEqual([...e.target.style.values],e.originalStyles);assert.equal(e.target.dataset.readmdDecoration,undefined);}
function assertEmpty(reader){assert.equal(reader.projection(),null);assert.equal(reader.revision(),'');assert.deepEqual(plain(reader.sourceCitations()),[]);assert.deepEqual(plain(reader.diagnostics()),[]);}

test('reader and editor preview consume every side without replacing special children or source mapping',async()=>{
  for(const editing of [false,true]){
    const e=environment({editing}),children=[...e.content.childNodes],text=e.target.textContent;
    assert.equal(await e.prepare(),'Rust prepared display');e.reader.applyLayout(e.pane);
    for(const [property,value]of Object.entries(rule()))assert.equal(e.target.style.getPropertyValue(property),value,property);
    assert.equal(e.target.style.getPropertyValue('color'),'green');assert.equal(e.target.style.getPropertyPriority('color'),'important');
    assert.equal(e.target.textContent,text);assert.deepEqual(e.content.childNodes,children);assert.equal(e.heading.textContent,'1 Original heading');assert.equal(e.heading.dataset.readmdGlobalId,'stable');
    assert.equal(e.citation.attributes.role,'button');assert.equal(e.citation.dataset.readmdSourceRevision,'current');assert.equal(await e.reader.locateCitation(e.citation),true);
    const from=source.indexOf('[@sample]'),anchor=source.slice(0,from).replace(/\r\n?/g,'\n').length;
    assert.deepEqual(plain(e.selections[0].selection),{anchor,head:anchor+9});assert.equal(e.tab.content,source);assert.equal(e.state.original,source);
    for(const child of [e.code,e.citation,e.math,e.strong])assert.equal(child.parentNode,e.content);
  }
});

test('an inline ordinary paragraph marker decorates its P box and retains its children',async()=>{
  const e=environment({inline:true}),children=[...e.marker.childNodes];await e.prepare();e.reader.applyLayout(e.pane);
  assert.equal(e.target.style.getPropertyValue('background-color'),'#DCE6F1');assert.equal(e.target.style.getPropertyValue('padding-bottom'),'31pt');assert.equal(e.marker.style.getPropertyValue('background-color'),'');
  assert.deepEqual(e.marker.childNodes,children);assert.equal(e.marker.parentNode,e.target);e.reader.clear();assertRestored(e);
});

test('sparse reset and clear restore exact prior CSS including priority after the marker is removed',async()=>{
  for(const clear of [false,true]){
    const e=environment();await e.prepare();e.reader.applyLayout(e.pane);assert.equal(e.marker.isConnected,false);
    if(clear)e.reader.clear();else{const reset=source.replace(/<!-- readmd:layout[^\r\n]+ -->\r\n\r\n/,'');e.setSource(reset);e.setResponse(fixture(reset,{},'reset'));await e.prepare();}
    e.reader.applyLayout(e.pane);assertRestored(e);
  }
});

test('changing a decoration retains the pre-projection CSS as its restoration baseline',async()=>{
  const e=environment();await e.prepare();e.reader.applyLayout(e.pane);
  const next=source+'Changed.\r\n',nextKey=key+'-changed',css={'background-color':'#FFFFFF','border-left-color':'#000000','border-left-style':'solid','border-left-width':'0.25pt','padding-left':'0pt'};
  e.setSource(next);e.setResponse(fixture(next,{[nextKey]:css},'changed'));e.marker.dataset.readmdLayout=nextKey;e.pane.childNodes.splice(1,0,e.marker);e.marker.parentNode=e.pane;
  await e.prepare();e.reader.applyLayout(e.pane);assert.equal(e.target.style.getPropertyValue('background-color'),'#FFFFFF');assert.equal(e.target.style.getPropertyValue('padding-left'),'0pt');assert.equal(e.target.style.getPropertyValue('border-bottom-width'),'');
  e.reader.applyLayout(e.pane);e.reader.clear();assertRestored(e);
});

test('malformed decoration CSS and nonordinary DOM targets never gain the paragraph allowance',async()=>{
  for(const bad of [
    {'background-color':'red'}, {'background-color':'#FFFFFF;position:fixed'},
    {'border-left-width':'12.01pt'}, {'border-left-width':'NaNpt'}, {'border-left-width':'2px'},
    {'padding-left':'32pt'}, {'padding-left':'1.5pt'}, {'border-left-style':'dashed'},
    {'border-left-color':'#GGGGGG'}, {'border-left-style':undefined}
  ]){
    const e=environment();e.setResponse(fixture(source,{[key]:{...rule(),...bad}}));await e.prepare();e.reader.applyLayout(e.pane);assertRestored(e);
  }
  for(const tag of ['H2','BLOCKQUOTE','SPAN']){const e=environment();e.target.tagName=tag;await e.prepare();e.reader.applyLayout(e.pane);assertRestored(e);}
  for(const tag of ['blockquote','li','td','pre']){const e=environment(),parent=new Node(tag);e.target.remove();e.pane.append(parent);parent.append(e.target);e.marker.remove();parent.childNodes.unshift(e.marker);e.marker.parentNode=parent;await e.prepare();e.reader.applyLayout(e.pane);assertRestored(e);}
  for(const name of ['rd-footnotes','readmd-bibliography']){const e=environment();e.pane.className=name;await e.prepare();e.reader.applyLayout(e.pane);assertRestored(e);}
});

test('a rebuilt surface releases detached paragraph bindings and keeps another active surface',async()=>{
  const e=environment();await e.prepare();e.reader.applyLayout(e.pane);e.target.remove();
  const other=new Node('article'),paragraph=new Node('p'),marker=new Node('div');marker.dataset.readmdLayout=key;e.root.append(other);other.append(marker,paragraph);e.reader.applyLayout(other);
  assertRestored(e);assert.equal(paragraph.style.getPropertyValue('background-color'),'#DCE6F1');
  e.reader.applyLayout(e.pane);assert.equal(paragraph.style.getPropertyValue('background-color'),'#DCE6F1');e.reader.clear();assert.equal(paragraph.style.getPropertyValue('background-color'),'');
});

test('failed same-source refresh drops the old cache, layout, numbering and citation authority',async()=>{
  const e=environment();await e.prepare();e.reader.applyLayout(e.pane);e.expire();
  e.setFetch(async()=>({ok:false,json:async()=>({ok:false,error_code:'publication_paragraph_decoration_unsupported'})}));
  assert.equal(await e.prepare(),source);assertEmpty(e.reader);assertRestored(e);assert.deepEqual(e.toasts,['Preparation failed']);
  e.reader.applyLayout(e.pane);assert.equal(e.heading.textContent,'Original heading');assert.equal(await e.reader.locateCitation(e.citation),false);
  e.marker.parentNode=e.pane;e.pane.childNodes.splice(1,0,e.marker);e.reader.applyLayout(e.pane);assertRestored(e);
});

test('plain-source fast fallback removes the preceding document projection without another request',async()=>{
  const e=environment();await e.prepare();e.reader.applyLayout(e.pane);const calls=e.calls.length;
  e.setSource('# Plain document\r\n\r\nOriginal text.\r\n');assert.equal(await e.prepare(),'# Plain document\r\n\r\nOriginal text.\r\n');assert.equal(e.calls.length,calls);assertEmpty(e.reader);assertRestored(e);
  e.reader.applyLayout(e.pane);assert.equal(e.heading.textContent,'Original heading');assert.equal(await e.reader.locateCitation(e.citation),false);
});

test('late success after a newer failure cannot resurrect an old document projection',async()=>{
  const queued=[],e=environment({fetch:body=>new Promise(resolve=>queued.push({body,resolve}))});
  const old=e.prepare(),next=source+'New document.\r\n';e.setSource(next);const current=e.prepare();
  queued[1].resolve({ok:false,json:async()=>({ok:false,error_code:'publication_local_style_invalid'})});assert.equal(await current,next);
  queued[0].resolve({ok:true,json:async()=>fixture(source)});assert.equal(await old,source);assertEmpty(e.reader);e.reader.applyLayout(e.pane);assertRestored(e);assert.equal(e.citation.attributes.role,undefined);
});

test('a superseded failure cannot clear a successful current projection',async()=>{
  const queued=[],e=environment({fetch:body=>new Promise(resolve=>queued.push({body,resolve}))});
  const old=e.prepare(),next=source+'New document.\r\n';e.setSource(next);const current=e.prepare();
  queued[1].resolve({ok:true,json:async()=>fixture(next,{[key]:rule()},'new')});await current;
  queued[0].resolve({ok:false,json:async()=>({ok:false})});await old;assert.equal(e.reader.revision(),'new');assert.deepEqual(e.toasts,[]);e.reader.applyLayout(e.pane);assert.equal(e.target.style.getPropertyValue('background-color'),'#DCE6F1');
});

test('responses after an unrequested edit, tab identity change or explicit clear leave no stale authority',async()=>{
  for(const invalidate of [e=>e.setSource(source+'Debounced edit.\r\n'),e=>{e.context.tab={...e.tab};},e=>{e.state.activeTabId='b';},e=>{e.context.cmView={};},e=>e.reader.clear()]){
    let release;const e=environment({fetch:()=>new Promise(resolve=>{release=resolve;})}),pending=e.prepare();invalidate(e);
    release({ok:true,json:async()=>fixture()});assert.equal(await pending,source);assertEmpty(e.reader);e.reader.applyLayout(e.pane);assertRestored(e);assert.deepEqual(e.toasts,[]);
  }
});
