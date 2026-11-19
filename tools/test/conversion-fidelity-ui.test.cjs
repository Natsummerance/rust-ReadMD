'use strict';
// Exercise the real read-only report renderer and owning receipt handlers.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ROOT=path.resolve(__dirname,'../..'),source=file=>fs.readFileSync(path.join(ROOT,file),'utf8');
const report={schema:1,scope:'structural',inspection:'partial',renderedComparison:false,findings:[{code:'native_math',count:2,status:'preserved',severity:'info'},{code:'charts',count:1,status:'review',severity:'warning'},{code:'inspection_incomplete',count:1,status:'not_checked',severity:'warning'}]};
class Element {
  constructor(tag='div'){this.tagName=tag.toUpperCase();this.children=[];this.dataset={};this.attributes={};this.className='';this.ownText='';this.listeners={};this.classList={add:n=>{if(!this.className.split(' ').includes(n))this.className+=' '+n;},remove:n=>{this.className=this.className.split(' ').filter(v=>v!==n).join(' ');},contains:n=>this.className.split(' ').includes(n),toggle:(n,on)=>{if(on)this.classList.add(n);else this.classList.remove(n);}};}
  set textContent(value){this.ownText=String(value);this.children=[];} get textContent(){return this.ownText+this.children.map(n=>n.textContent).join('');}
  set innerHTML(value){assert.equal(value,'','report content must remain plain text');this.children=[];} get innerHTML(){return '';}
  append(...nodes){for(const node of nodes)this.appendChild(node);} appendChild(node){this.children.push(node);node.parentNode=this;return node;}
  replaceChildren(...nodes){this.children=[];this.ownText='';this.append(...nodes);}
  setAttribute(key,value){this.attributes[key]=String(value);} removeAttribute(key){delete this.attributes[key];if(key==='tabindex')delete this.tabIndex;}
  addEventListener(key,fn){this.listeners[key]=fn;}
  matches(selector){if(selector.startsWith('.'))return this.className.split(' ').includes(selector.slice(1));if(selector.startsWith('#'))return this.id===selector.slice(1);return this.tagName===selector.toUpperCase();}
  querySelectorAll(selector){return this.children.flatMap(node=>[...(node.matches(selector)?[node]:[]),...node.querySelectorAll(selector)]);} querySelector(selector){return this.querySelectorAll(selector)[0]||null;}
  remove(){this.parentNode.children=this.parentNode.children.filter(node=>node!==this);} replaceWith(node){const i=this.parentNode.children.indexOf(this);this.parentNode.children[i]=node;node.parentNode=this.parentNode;}
}
function environment(locale='en'){
  const strings=JSON.parse(source('assets/i18n/'+locale+'.json')),nodes=new Map(),body=new Element('body'),opened=[];
  const t=(key,args={})=>String(strings[key]??key).replace(/\{(\w+)\}/g,(all,name)=>args[name]??all);
  const context={window:{i18n:{t},ReadMDModal:{open:node=>opened.push(node),close:()=>{}}},document:{body,createElement:tag=>new Element(tag)},setTimeout,clearTimeout,AbortController,TextEncoder,crypto:require('node:crypto').webcrypto,console,hasPy:false,py:{},state:{},loadFile:async()=>{}};
  context.$=id=>{if(!nodes.has(id)){const node=new Element();node.id=id;nodes.set(id,node);}return nodes.get(id);};
  context.document.getElementById=context.$;context.apiMessage=()=>strings['error.conversion_failed'];
  vm.createContext(context);const text=source('assets/js/features/convert.js');vm.runInContext(text.slice(text.indexOf('const _t ='),text.indexOf('function currentSpeechLanguage')),context);
  return{context,strings,opened,nodes,body};
}
test('all 46 locales supply the same 20 report labels and distinct extraction states',()=>{
  const locales=Object.keys(JSON.parse(source('assets/i18n/meta.json'))),base=JSON.parse(source('assets/i18n/en.json'));
  const keys=Object.keys(base).filter(key=>key.startsWith('convert.fidelity.'));assert.equal(locales.length,46);assert.equal(keys.length,20);
  for(const locale of locales){const strings=JSON.parse(source('assets/i18n/'+locale+'.json'));for(const key of keys)assert(typeof strings[key]==='string'&&strings[key].trim()&&strings[key]!==key,locale+': '+key);assert.notEqual(strings['convert.fidelity.status.preserved'],strings['convert.fidelity.status.not_preserved']);assert.notEqual(strings['convert.fidelity.status.extracted'],strings['convert.fidelity.status.not_checked']);}
});
test('the report preserves typed findings, remains read-only and ignores unknown raw details',()=>{
  for(const locale of ['zh-CN','en','ar']){
    const e=environment(locale),api=e.context.window.ReadMDConversionReport;assert.equal(api.show(undefined,'old.docx'),false);assert.equal(api.show({...report,schema:2},'future.docx'),false);assert.equal(e.opened.length,0);
    const data={...report,findings:[...report.findings,{code:'<img src=x onerror=alert(1)>',status:'preserved',count:-1,message:'DO_NOT_ECHO_SOURCE_TEXT',path:'/private/source'}]};
    const before=JSON.stringify(data);assert.equal(api.show(data,'/owned/原件.docx'),true);assert.equal(JSON.stringify(data),before);
    const text=e.opened[0].textContent;assert(text.includes(e.strings['convert.fidelity.title']));assert(text.includes(e.strings['convert.fidelity.status.preserved']));assert(text.includes(e.strings['convert.fidelity.code.native_math']));assert(text.includes(e.strings['convert.fidelity.status.not_checked']));assert(!text.includes('DO_NOT_ECHO_SOURCE_TEXT')&&!text.includes('<img')&&!text.includes('/private'));
    assert.equal(e.opened[0].querySelector('#conversion-fidelity-source').textContent,'原件.docx');
  }
});
test('batch reports survive selected-job reopen and do not hijack result opening',async()=>{
  const e=environment(),context=e.context,calls=[];context.apiFetch=async url=>{calls.push(url);return{ok:true,status:200,json:async()=>url.includes('history=1')?{batches:[]}:{job:'b-owned',attempt:1,finished:true,running:false,done:1,total:1,output_dir:'/owned/results',items:[{src:'/owned/book.docx',status:'ok',planned_out:'/owned/results/book.md',out:'/owned/results/book.md',fidelity_report:report}]}};};
  vm.runInContext(source('assets/js/features/batch.js'),context);await context.restoreBatchWorkbench('b-owned');
  assert(calls.some(url=>url==='/api/convert/progress?job=b-owned'));
  const row=context.$('convert-list').children[0],details=row.querySelector('.conversion-fidelity');assert(details);assert(details.textContent.includes(e.strings['convert.fidelity.code.charts']));assert.equal(row.attributes.role,'group');assert.equal(row.querySelector('.batch-name').attributes.role,'button');assert.equal(row.dataset.out,'/owned/results/book.md');
  let stopped=0;details.listeners.click({stopPropagation:()=>stopped++});details.listeners.keydown({stopPropagation:()=>stopped++});assert.equal(stopped,2);
  context.renderBatchProgress({items:[{src:'/owned/book.docx',status:'running'}],running:false});assert.equal(row.querySelectorAll('.conversion-fidelity').length,1);assert.equal(row.dataset.result,'ok');
  const old=context.makeBatchRow('/owned/old.docx');context.window.ReadMDConversionReport.attach(old,undefined);assert.equal(old.querySelector('.conversion-fidelity'),null);
});
test('single and PDF extraction show successful, empty and preview receipts without changing failure outcomes',async()=>{
  const text=source('assets/js/reader/render.js'),from=text.indexOf('async function convertFile('),to=text.indexOf('function showConvertWarns(',from);
  for(const kind of ['preview','saved','empty','failure','legacy']){
    const shown=[],opened=[],toasts=[],data={name:'source.pdf',content:kind==='empty'?'':'# Extracted',dir:'/owned',fixes:[],...(kind==='legacy'?{}:{fidelity_report:report}),...(kind==='saved'?{saved:true,out:'/owned/result.md'}:{})};
    const context=vm.createContext({window:{i18n:{t:key=>key},ReadMDConversionReport:{show:value=>{if(value)shown.push(value);}}},ensureModule:async()=>true,busy(){},apiFetch:async()=>({ok:kind!=='failure',status:kind==='failure'?500:200,json:async()=>data}),apiMessage:()=>'',showToast:value=>toasts.push(value),showConvertWarns(){},renderVirtual:async(...args)=>{opened.push(args);return true;},loadFile:async path=>opened.push(path)});
    vm.runInContext(text.slice(from,to),context);assert.equal(await context.convertFile('/owned/source.pdf'),!['failure','empty'].includes(kind));assert.equal(shown.length,['failure','legacy'].includes(kind)?0:1);assert.equal(opened.length,['failure','empty'].includes(kind)?0:1);
  }
});
