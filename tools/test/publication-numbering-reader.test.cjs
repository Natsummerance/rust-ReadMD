'use strict';
// Execute the real reader/export host wiring with an owned small DOM. These
// contracts do not certify WebView geometry, Word or real product API replies.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),crypto=require('node:crypto');
const ROOT=path.resolve(__dirname,'../..'),plain=value=>JSON.parse(JSON.stringify(value));
const EN=JSON.parse(fs.readFileSync(path.join(ROOT,'assets/i18n/en.json'),'utf8'));
const dataKey=name=>name.slice(5).replace(/-([a-z])/g,(_,letter)=>letter.toUpperCase());
class Node {
  constructor(tag=null,text=''){this.nodeType=tag?1:3;this.tagName=tag?.toUpperCase();this.nodeName=this.tagName||'#text';this.nodeValue=text;this.childNodes=[];this.parentNode=null;this.dataset={};this.attributes={};this.className='';this.listeners={};this.value='';this.open=false;this.style={setProperty(){},getPropertyValue(){return'';},getPropertyPriority(){return'';},removeProperty(){}};this.options=[];this.classList={contains:name=>this.className.split(/\s+/).includes(name),add:name=>{if(!this.classList.contains(name))this.className+=' '+name;},remove:name=>{this.className=this.className.split(/\s+/).filter(x=>x!==name).join(' ');},toggle:(name,force)=>{const yes=force??!this.classList.contains(name);yes?this.classList.add(name):this.classList.remove(name);return yes;}};}
  get children(){return this.childNodes.filter(node=>node.nodeType===1);}get childElementCount(){return this.children.length;}get parentElement(){return this.parentNode;}get isConnected(){return this.tagName==='ROOT'||Boolean(this.parentNode?.isConnected);}
  get textContent(){return this.nodeType===3?this.nodeValue:this.childNodes.map(node=>node.textContent).join('');}set textContent(value){this.replaceChildren();if(String(value))this.append(new Node(null,String(value)));}
  append(...nodes){for(const node of nodes){node.remove();node.parentNode=this;this.childNodes.push(node);}}appendChild(node){this.append(node);return node;}prepend(node){node.remove();node.parentNode=this;this.childNodes.unshift(node);}remove(){if(this.parentNode)this.parentNode.childNodes=this.parentNode.childNodes.filter(node=>node!==this);this.parentNode=null;}replaceChildren(...nodes){for(const node of this.childNodes)node.parentNode=null;this.childNodes=[];this.append(...nodes);}
  setAttribute(name,value){this.attributes[name]=String(value);if(name.startsWith('data-'))this.dataset[dataKey(name)]=String(value);if(name==='id')this.id=String(value);}getAttribute(name){return this.attributes[name]??null;}addEventListener(name,fn){(this.listeners[name]||=[]).push(fn);}setCustomValidity(value){this.validationMessage=value;}reportValidity(){return !this.validationMessage;}focus(){this.focused=true;}scrollIntoView(){this.scrolled=true;}add(option){this.options.push(option);}
  matches(selector){
    const simple=(node,part)=>{const tag=/^[a-z][\w-]*/iu.exec(part);if(tag&&node.tagName!==tag[0].toUpperCase())return false;for(const match of part.matchAll(/([#.])([\w-]+)/gu))if(match[1]==='#'?node.id!==match[2]:!node.classList.contains(match[2]))return false;for(const attr of part.matchAll(/\[([^=\]]+)(?:="([^"]*)")?\]/gu)){const value=attr[1].startsWith('data-')?node.dataset[dataKey(attr[1])]:node.attributes[attr[1]];if(value===undefined||attr[2]!==undefined&&String(value)!==attr[2])return false;}return !part.replace(/^[a-z][\w-]*/iu,'').replace(/[#.][\w-]+/gu,'').replace(/\[[^\]]+\]/gu,'');};
    return selector.split(',').some(raw=>{const steps=raw.trim().split(/\s+/u);let node=this;if(!simple(node,steps.pop()))return false;while(steps.length){const part=steps.pop();for(node=node.parentNode;node&&!simple(node,part);node=node.parentNode){}if(!node)return false;}return true;});
  }
  closest(selector){for(let node=this;node;node=node.parentNode)if(node.matches(selector))return node;return null;}contains(target){for(let node=target;node;node=node.parentNode)if(node===this)return true;return false;}
  querySelectorAll(selector){const output=[];const visit=node=>{for(const child of node.children){if(child.matches(selector))output.push(child);visit(child);}};visit(this);return output;}querySelector(selector){return this.querySelectorAll(selector)[0]||null;}
  set innerHTML(html){this.replaceChildren();let select=null;for(const match of html.matchAll(/<(input|select|option)\b([^>]*)>/gu)){const node=new Node(match[1]);for(const attr of match[2].matchAll(/([\w-]+)="([^"]*)"/gu)){node.setAttribute(attr[1],attr[2]);if(['type','value','min','max','step'].includes(attr[1]))node[attr[1]]=attr[2];}if(match[1]==='option'&&select)select.append(node);else{this.append(node);if(match[1]==='select')select=node;}}}
}
const config={headings:{levels:[{},{}]}};
const rule=(text,numbers)=>({text,numbers,suffix:' ',rules:[]});
const body='# First **bold** {#first}\n\n> # Nested\n\n# Second {#second}\n';
const source='<!-- readmd-numbering:v1 {"headings":{"levels":[{},{}]}} -->\n\n'+body;
function fixture(overrides={}){return{ok:true,content:body,reader_content:body,diagnostics:[],warnings:[],source_labels:[],source_numbering:{config,headings:[{ordinal:0,id:'first',level:1,numbering:rule('1',[1])},{ordinal:1,id:'second',level:1,numbering:rule('2',[2])}]},...overrides};}
function env({content=source,response=fixture(),fetch,editing=false}={}){
  const root=new Node('root'),host=new Node('div'),pane=new Node('article');root.append(host,pane);const calls=[],modals=[],mounts=[],toasts=[];let raw=content;const tab={id:'a',content,original:content};
  const state={activeTabId:'a',editing,file:'Z:/Owned.md',sourceName:'Owned.md',original:content,fixed:content,mode:'file',export:{fmt:'docx',defaults:{},options:{},catalog:[]},pagination:{}};
  const document={body:root,createElement:tag=>new Node(tag),createTextNode:text=>new Node(null,text),getElementById:id=>root.querySelector('#'+id),querySelectorAll:selector=>selector==='#export-opts [data-k]'?host.querySelectorAll('[data-k]'):root.querySelectorAll(selector),querySelector:selector=>root.querySelector(selector),createTreeWalker(node){const texts=[];const walk=parent=>{for(const child of parent.childNodes){if(child.nodeType===3)texts.push(child);else walk(child);}};walk(node);let index=0;return{nextNode:()=>texts[index++]||null};}};
  const view={state:{doc:{toString:()=>raw.replace(/\r\n?/g,'\n')},selection:{main:{head:0}}},dispatch(){},focus(){}};
  const i18n={t:(key,params)=>key.startsWith('warn.')?(EN[key]??key):key+(params?.level?' '+params.level:'')};
  const context={window:{document,i18n,addEventListener(){}},document,state,cmView:view,i18n,AbortController,AbortSignal,Date,setTimeout,clearTimeout,console,NodeFilter:{SHOW_TEXT:4},Option:class{constructor(text,value){this.text=text;this.value=value;}},
    getActiveTab:()=>context.tab,showToast:message=>toasts.push(message),apiMessage:()=>'',ReadMDModal:{open:modal=>modals.push(modal),close(){}},$:id=>document.getElementById(id),cancelNativeExportPreview(){},
    apiFetch:async(route,request)=>{const requestBody=JSON.parse(request.body);calls.push({route,body:requestBody});if(fetch)return fetch(route,requestBody);let data;if(route==='/api/publication/blocks')data={ok:true,document:{revision:'r0',diagnostics:[],nodes:[{id:'block-first',from:0,to:raw.length,kind:'paragraph',editable:true,title:'Owned paragraph',style:{}}]}};else if(route==='/api/publication/numbering')data={ok:true,revision:'r0',config,plan:response.source_numbering};else data=response;return{ok:true,json:async()=>data};}};
  context.tab=tab;vm.createContext(context);
  vm.runInContext(fs.readFileSync(path.join(ROOT,'assets/vendor/marked.min.js'),'utf8'),context);context.window.marked=context.marked;
  for(const file of ['assets/js/editor/preview.js','assets/js/reader/heading-identity.js','assets/js/editor/publication-metadata.js','assets/js/editor/publication-numbering.js','assets/js/reader/publication.js','assets/js/features/export.js'])vm.runInContext(fs.readFileSync(path.join(ROOT,file),'utf8'),context,{filename:file});
  context.ReadMDHeadings=context.window.ReadMDHeadings;context.ReadMDPublication=context.window.ReadMDPublication;
  const actual=context.window.ReadMDPublicationNumbering;context.window.ReadMDPublicationNumbering={mount(target,options){const controller=actual.mount(target,options);mounts.push({host:target,options,controller});return controller;}};
  context.toggleEdit=async()=>{state.editing=true;};
  return{context,state,tab,root,host,pane,calls,modals,mounts,toasts,reader:context.ReadMDPublication,view,
    setSource(value){raw=value;state.original=value;state.fixed=value;context.tab.content=value;},
    setEditorText(value){raw=value;},
    heading(text,id,parent=pane,level=1){const node=new Node('h'+level);node.textContent=text;if(id){node.dataset.readmdHeadingText=text;node.dataset.readmdExplicitId=id;}parent.append(node);return node;},
    async prepare(){return context.ReadMDPublication.prepare(state.editing?context.getEditContent():raw,state.file);},ids(){context.ReadMDHeadings.applyDom(pane);}};
}

function pagedReader(e,pages,rawContent){
  const content=e.host;content.id='content';const toc=new Node('div');toc.id='toc-list';e.root.append(toc);
  Object.defineProperty(content,'innerHTML',{set(html){
    this.replaceChildren();const article=new Node('article');article.className='markdown-body rd-article';this.append(article);
    for(const match of html.matchAll(/<h([1-6])\b([^>]*)>([\s\S]*?)<\/h\1>/gu)){const heading=new Node('h'+match[1]);heading.textContent=match[3].replace(/<[^>]+>/gu,'');for(const attr of match[2].matchAll(/([\w-]+)="([^"]*)"/gu))heading.setAttribute(attr[1],attr[2]);article.append(heading);}
  }});
  const c=e.context;c.document.createDocumentFragment=()=>new Node('fragment');c.CSS={escape:String};c.MutationObserver=class{observe(){}};
  c.requestAnimationFrame=callback=>callback();c.setTimeout=()=>0;c.preferredScrollBehavior=()=> 'auto';
  c.readerPrepare=value=>value;c.transformAcademicCallouts=value=>value;c.transformWikilinks=value=>value;c.protectMath=value=>({src:value,saved:[]});c.restoreMath=value=>value;c.sanitizeRenderedHtml=value=>value;
  for(const name of ['updatePaginationBar','updateStatus','readerEnhance','fixLinks','fixImages','processBibCitations','renderMath','renderAllCodeChunks','renderAllDiagrams'])c[name]=()=>{};
  c.ensureHeadingIds=body=>c.ReadMDHeadings.applyDom(body);
  const render=fs.readFileSync(path.join(ROOT,'assets/js/reader/render.js'),'utf8');
  vm.runInContext(fs.readFileSync(path.join(ROOT,'assets/js/reader/toc.js'),'utf8'),c,{filename:'assets/js/reader/toc.js'});c.updateActiveTocHeading=()=>{};
  vm.runInContext(render.slice(render.indexOf('function renderPage('),render.indexOf('\nlet paginationEventsBound')),c,{filename:'renderPage.real.js'});
  vm.runInContext(render.slice(render.indexOf('function postProcess('),render.indexOf('\nfunction renderAllCodeChunks(')),c,{filename:'postProcess.real.js'});
  e.state.pagination={enabled:true,mode:'paged',pages:pages.map((content,pageIndex)=>({content,pageIndex})),rawContent,currentPage:0,totalPages:pages.length};
  return {content,toc,render:(index,target=null)=>c.renderPage(index,target,false),headings:()=>content.querySelectorAll('h1,h2,h3,h4,h5,h6')};
}

test('reader numbers only true body headings without rewriting title nodes or IDs',async()=>{
  const e=env(),first=e.heading('First bold','first'),second=e.heading('Second','second'),strong=new Node('strong');strong.textContent='bold';first.replaceChildren(new Node(null,'First '),strong);
  for(const [tag,className]of [['blockquote',''],['li',''],['section','rd-footnotes'],['div','readmd-bibliography']]){const parent=new Node(tag);parent.className=className;e.pane.append(parent);e.heading('Nested example',null,parent);}
  await e.prepare();e.reader.applyLayout(e.pane);e.ids();assert.equal(first.textContent,'1 First bold');assert.equal(second.textContent,'2 Second');assert.equal(first.dataset.readmdHeadingText,'First bold');assert.equal(first.id,'first');assert.equal(second.id,'second');assert.equal(strong.parentNode,first);assert.equal(e.pane.querySelectorAll('[data-readmd-heading-number]').length,2);
  e.reader.applyLayout(e.pane);e.ids();assert.equal(first.textContent,'1 First bold');assert.equal(e.pane.querySelectorAll('[data-readmd-heading-number]').length,2);
});

test('clearing configuration removes old number spans from retained heading DOM',async()=>{
  const e=env(),first=e.heading('First bold','first');e.heading('Second','second');await e.prepare();e.reader.applyLayout(e.pane);assert.equal(first.textContent,'1 First bold');e.setSource(body);await e.prepare();e.reader.applyLayout(e.pane);e.ids();assert.equal(first.textContent,'First bold');assert.equal(first.id,'first');assert.equal(e.pane.querySelectorAll('[data-readmd-heading-number]').length,0);
});

test('a reader page uses the full source plan and preserves source IDs in its outline',async()=>{
  const content='<!-- readmd-numbering:v1 {"headings":{"levels":[{}]}} -->\n\n# See :ref[eq:value]\n\n# Other\n',projected='# See Equation 2.1\n\n# Other\n';
  const response=fixture({content:projected,reader_content:projected,source_numbering:{config,headings:[{ordinal:0,id:'see-refeqvalue',level:1,numbering:rule('1',[1])},{ordinal:1,id:'other',level:1,numbering:rule('2',[2])}]}});
  const e=env({content,response}),heading=e.heading('See Equation 2.1');heading.dataset.readmdGlobalId='see-equation-21';heading.id='see-equation-21';e.state.pagination={rawContent:projected,allHeadings:[{id:'see-equation-21',level:1},{id:'other',level:1}]};await e.prepare();e.reader.applyLayout(e.pane);e.ids();assert.equal(heading.textContent,'1 See Equation 2.1');assert.equal(heading.id,'see-refeqvalue');assert.equal(e.state.pagination.allHeadings[0].id,'see-refeqvalue');
  e.pane.replaceChildren();const other=e.heading('Other');other.dataset.readmdGlobalId='other';e.reader.applyLayout(e.pane);e.ids();assert.equal(other.textContent,'2 Other');assert.equal(other.id,'other');
});

test('the real first paged render builds IDs before numbering ordinary headings with no prior outline',async()=>{
  const content='<!-- readmd-numbering:v1 {"headings":{"levels":[{}]}} -->\n\n# First\n\n# Second\n\n# Third\n',projected='# First\n\n# Second\n\n# Third\n';
  const response=fixture({content:projected,reader_content:projected,source_numbering:{config,headings:['first','second','third'].map((id,index)=>({ordinal:index,id,level:1,numbering:rule(String(index+1),[index+1])}))}}),e=env({content,response});await e.prepare();
  const paged=pagedReader(e,['# First\n\n# Second\n','# Third\n'],projected);assert.equal(e.state.pagination.allHeadings,undefined);paged.render(0);
  assert.deepEqual(paged.headings().map(node=>[node.id,node.textContent]),[['first','1 First'],['second','2 Second']]);assert.equal(paged.content.querySelectorAll('[data-readmd-heading-number]').length,2);
  paged.render(1);assert.equal(paged.headings()[0].textContent,'3 Third');paged.render(0);assert.equal(paged.headings()[0].textContent,'1 First');assert.equal(paged.content.querySelectorAll('[data-readmd-heading-number]').length,2);
});

test('real paged TOC links and arrival targets keep source IDs for projected reference headings',async()=>{
  const content='<!-- readmd-numbering:v1 {"headings":{"levels":[{}]}} -->\n\n# See :ref[eq:x]\n\n# Other\n',projected='# See Equation 1\n\n# Other\n';
  const response=fixture({content:projected,reader_content:projected,source_numbering:{config,headings:[{ordinal:0,id:'see-refeqx',level:1,numbering:rule('1',[1])},{ordinal:1,id:'other',level:1,numbering:rule('2',[2])}]}}),e=env({content,response});await e.prepare();const paged=pagedReader(e,['# See Equation 1\n','# Other\n'],projected);paged.render(0);
  const link=paged.toc.querySelector('[data-heading-id]');assert.equal(link.dataset.headingId,'see-refeqx');assert.equal(link.getAttribute('href'),'#see-refeqx');assert.equal(paged.headings()[0].id,link.dataset.headingId);
  paged.render(1);const group=paged.toc.querySelector('details[data-page-idx="1"]');group.open=true;for(const listener of group.listeners.toggle||[])listener();assert(paged.toc.querySelectorAll('[data-heading-id]').some(node=>node.dataset.headingId==='other'));
  for(const listener of paged.toc.listeners.click||[])listener({target:link,preventDefault(){}});
  assert.equal(paged.headings()[0].id,'see-refeqx');assert.equal(paged.headings()[0].textContent,'1 See Equation 1');assert.equal(paged.headings()[0].scrolled,true);assert.equal(paged.headings()[0].focused,true);assert.equal(link.dataset.headingId,'see-refeqx');
});

test('repeated paged layouts cannot remap a resolved ID that equals another projected heading ID',async()=>{
  const content='<!-- readmd-numbering:v1 {"headings":{"levels":[{}]}} -->\n\n# See :ref[eq:x]\n\n# See refeqx\n',projected='# See Equation 1\n\n# See refeqx\n';
  const response=fixture({content:projected,reader_content:projected,source_numbering:{config,headings:[{ordinal:0,id:'see-refeqx',level:1,numbering:rule('1',[1])},{ordinal:1,id:'see-refeqx-1',level:1,numbering:rule('2',[2])}]}}),e=env({content,response});await e.prepare();const paged=pagedReader(e,['# See Equation 1\n','# See refeqx\n'],projected);
  for(const page of [0,1,0,1,0]){paged.render(page);assert.deepEqual(plain(e.state.pagination.allHeadings.map(row=>row.id)),['see-refeqx','see-refeqx-1']);assert.equal(paged.headings()[0].id,page?'see-refeqx-1':'see-refeqx');}
  assert.equal(paged.toc.querySelector('[data-heading-id]').dataset.headingId,'see-refeqx');
});

test('tab identity, source changes and cleared generations cannot reuse a numbering plan',async()=>{
  for(const invalidate of [e=>{e.context.tab={...e.tab};},e=>{e.state.activeTabId='b';},e=>e.setSource(source+'New'),e=>e.reader.clear()]){const e=env(),heading=e.heading('First bold','first');e.heading('Second','second');await e.prepare();e.reader.applyLayout(e.pane);invalidate(e);e.reader.applyLayout(e.pane);assert.equal(heading.textContent,'First bold');assert.equal(e.pane.querySelectorAll('[data-readmd-heading-number]').length,0);}
});

test('an LF completion projection cannot erase a current CRLF editor numbering plan',async()=>{
  for(const order of ['preview-first','completion-first']){
    const raw=source.replace(/\n/g,'\r\n'),e=env({content:raw,editing:true}),heading=e.heading('First bold','first');e.heading('Second','second');assert.equal(e.context.getEditContent(),raw);assert.equal(e.view.state.doc.toString(),source);
    const completion=()=>e.reader.labels(e.view.state.doc.toString(),e.state.file);if(order==='preview-first'){await e.prepare();await completion();}else{await completion();await e.prepare();}
    assert(e.calls.some(call=>call.body.content===raw));assert(e.calls.some(call=>call.body.content===source));e.reader.applyLayout(e.pane);assert.equal(heading.textContent,'1 First bold');
    e.setEditorText(source+'Edited\n');assert.equal(e.context.getEditContent(),raw+'Edited\r\n');e.reader.applyLayout(e.pane);assert.equal(heading.textContent,'First bold');await e.prepare();e.reader.applyLayout(e.pane);assert.equal(heading.textContent,'1 First bold');
  }
});

test('numbering-only and citation-resources-only source reaches the shared preparation service',async()=>{
  for(const content of [source,'---\ncitation-resources: [refs.json]\n---\n# Heading\n']){const e=env({content});await e.prepare();assert.equal(e.calls.length,1);assert.equal(e.calls[0].route,'/api/academic/document');assert.equal(e.calls[0].body.view,true);}
});

test('a returned resource capsule without YAML reaches CSL projection and retains authored source',async()=>{
  const text='[{"id":"owned","type":"book","title":"Owned","author":[{"family":"Owned"}],"issued":{"date-parts":[[2026]]}}]',resources={version:1,bibliography:['refs.json'],files:[{name:'refs.json',kind:'csl-json',sha256:crypto.createHash('sha256').update(text).digest('hex'),text}],style:'apa',locale:'en-US',locale_xml:null,manual_bibliography:false};
  const capsule='<!--readmd:citation-resources:v1\n'+JSON.stringify(resources)+'\n-->\n\nReturned [@owned].\n',projected='Returned <span class="readmd-citation">(Owned, 2026)</span>.\n',e=env({content:capsule,response:fixture({content:projected,reader_content:projected,source_numbering:null})});assert.equal(await e.prepare(),projected);assert.equal(e.calls.length,1);assert.equal(e.calls[0].body.content,capsule);assert.equal(e.tab.content,capsule);assert.equal(e.state.original,capsule);
});

test('an import-only return reaches conflict diagnostics and code examples stay backend-owned',async()=>{
  const diagnostic={code:'docx_citation_conflict',reason:'citation_render_modified',visible_content_preserved:true},annotation='<!--readmd:citation-import:v1\n'+JSON.stringify({version:1,diagnostics:[diagnostic]})+'\n-->\n';
  const e=env({content:annotation,response:fixture({content:'',reader_content:'',source_numbering:null,warnings:['docx_citation_conflict'],diagnostics:[diagnostic]})});await e.prepare();assert.equal(e.calls.length,1);assert.deepEqual(e.toasts,[EN['warn.docx_citation_conflict']]);assert.equal(e.tab.content,annotation);
  const literal='```md\n'+annotation+'```\n',safe=env({content:literal,response:fixture({content:literal,reader_content:literal,source_numbering:null})});assert.equal(await safe.prepare(),literal);assert.equal(safe.calls.length,1);assert.deepEqual(safe.toasts,[]);assert.equal(safe.calls[0].body.content,literal);
});

test('the existing local-format modal mounts numbering using the real lexical editor view',async()=>{
  const e=env({editing:true});assert.equal(e.context.window.cmView,undefined);await e.reader.openStyle();assert.equal(e.modals.length,1);assert.equal(e.mounts.length,1);assert.equal(e.mounts[0].options.view,e.view);assert.equal(e.mounts[0].controller.root.open,false);assert.equal(e.calls.filter(call=>call.route==='/api/publication/numbering').length,0);assert.equal(e.mounts[0].options.isCurrent(),true);
});

test('the existing export heading group mounts the same component and activates editing on apply',async()=>{
  const e=env();e.host.id='export-opts';e.context.getExportSections=()=>[{title:'Headings',fmts:['docx'],headingRows:true}];e.context.applyExportOptionsToDom=()=>{};e.context.updateExportLivePreview=()=>{};e.context.renderExportSections();assert.equal(e.mounts.length,1);const mounted=e.mounts[0];assert.equal(mounted.controller.root.open,false);assert.equal(e.state.editing,false);const binding=await mounted.options.ensureEditor();assert.equal(e.state.editing,true);assert.equal(binding.view,e.view);assert.equal(binding.current(),true);assert.equal(e.calls.length,0);
});

test('numbering controls never enter export preset collection or its generic events',async()=>{
  const e=env();e.host.id='export-opts';e.context.getExportSections=()=>[{title:'Headings',fmts:['docx'],headingRows:true}];e.context.applyExportOptionsToDom=()=>{};e.context.updateExportLivePreview=()=>{};e.context.renderExportSections();await e.mounts[0].controller.load();const original={future:{retained:true},footnotes:{continuationNotice:'Kept while hidden',futurePolicy:'retained'}};e.state.export.options=plain(original);const collected=plain(e.context.collectExportOptions());assert.deepEqual(collected.future,original.future);assert.deepEqual(collected.footnotes,original.footnotes);assert.equal(collected.numbering,undefined);for(const input of e.mounts[0].controller.controls.values())assert.equal(input.dataset.k,undefined);
});

test('new note controls follow actual engine support and hidden values remain in presets',()=>{
  const e=env(),fields=e.context.getExportSections().flatMap(section=>section.fields||[]),keys=['restart','separator','continuationSeparator','continuationNotice','longNote'];
  for(const key of keys)assert(fields.some(field=>field.k==='footnotes.'+key));
  for(const fmt of ['docx','epub','pdf','html','tex'])for(const engine of ['rust','typst','xelatex']){e.state.export.fmt=fmt;e.state.export.options={pdf:{engine}};for(const key of keys){const field=fields.find(field=>field.k==='footnotes.'+key),supported=['continuationSeparator','continuationNotice'].includes(key)?fmt==='docx':fmt==='docx'||fmt==='epub'||fmt==='pdf'&&engine==='typst';assert.equal(e.context.expFieldApplicable(field,['pdf','docx','html','epub'],fmt),supported,fmt+'/'+engine+'/'+key);}}
});

test('both real rendering paths apply source numbering before heading identity allocation',()=>{
  for(const file of ['assets/js/reader/render.js','assets/js/editor/preview.js']){const text=fs.readFileSync(path.join(ROOT,file),'utf8'),apply=text.lastIndexOf('ReadMDPublication.applyLayout('),identity=text.indexOf('ensureHeadingIds(',apply);assert(apply>=0&&identity>apply);}
  const text=fs.readFileSync(path.join(ROOT,'assets/js/features/export.js'),'utf8');assert(text.includes("el.closest('[data-publication-numbering]')"));
});
