'use strict';
// Controller and DOM contract tests with synthetic API responses. These do not
// certify PDFium rendering or a native platform; existing process QA is separate.
const test=require('node:test'),assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../..'),source=file=>fs.readFileSync(path.join(root,'assets/js',file),'utf8');
const deferred=()=>{let resolve;const promise=new Promise(done=>{resolve=done;});return{resolve,promise};};
class Element {
  constructor(tag){this.tagName=tag.toUpperCase();this.children=[];this.dataset={};this.style={};this.attributes={};this.value='';this.hidden=false;this.disabled=false;this.inert=false;const classes=new Set();this.classList={add:value=>classes.add(value),remove:value=>classes.delete(value),contains:value=>classes.has(value),toggle:(value,on)=>on?classes.add(value):classes.delete(value)};}
  set className(value){this._className=value;for(const name of value.split(' '))this.classList.add(name);}
  get className(){return this._className||'';}
  append(...children){for(const child of children){child.parentNode=this;this.children.push(child);}}
  replaceChildren(...children){this.children=[];this.append(...children);}
  setAttribute(key,value){this.attributes[key]=String(value);}
  addEventListener(name,action){this.listeners??={};this.listeners[name]=action;}
  click(){}
  getBoundingClientRect(){return{left:0,top:0,width:600,height:800};}
  focus(){}
  decode(){return Promise.resolve();}
  closest(tag){let node=this;while(node&&node.tagName!==tag.toUpperCase())node=node.parentNode;return node;}
  querySelector(selector){if(selector[0]==='#')return this.find(node=>node.id===selector.slice(1));return null;}
  querySelectorAll(selector){const nodes=[];this.walk(node=>{if(selector==='.pdf-toolbar button,.pdf-toolbar input'){if(['BUTTON','INPUT'].includes(node.tagName))nodes.push(node);}else if(selector==='.pdf-link'&&node.classList.contains('pdf-link'))nodes.push(node);});return nodes;}
  walk(action){for(const node of this.children){action(node);node.walk(action);}}
  find(check){if(check(this))return this;for(const node of this.children){const found=node.find(check);if(found)return found;}return null;}
}
function harness({capability=true}={}) {
  const body=new Element('body'),calls=[],conversions=[],toasts=[],sessions=new Map(),created=[];let sequence=0,answer='cancel',saveFails=false,openGate=null,capabilityGate=null;
  const document={body,activeElement:null,createElement:tag=>{const node=new Element(tag);created.push(node);return node;},getElementById:id=>body.find(node=>node.id===id)};
  const window={i18n:{t:key=>key,currentLang:'en'},addEventListener(){},ReadMDTask:{loading(){}},ReadMDModal:{open:node=>node.classList.remove('hidden'),setGuard(){}},ReadMDCapabilities:{ensure:async()=>capabilityGate?capabilityGate.promise:capability}};
  const data=(id,file)=>({ok:true,session:id,path:file,revision:0,page:0,pages:2,width:600,height:800,rotation:0,image:'synthetic bitmap',can_edit:true,can_undo:false,can_redo:false,dirty:false,permissions:{pages:true,content:true,annotations:true,forms:true},coordinates:{origin:[0,0],x:[1,0],y:[0,1]},objects:[{kind:'text',id:1,text:'Original PDF text',bounds:{x:1,y:1,width:80,height:20}}],annotations:[],fonts:[],links:[]});
  const context=vm.createContext({window,document,addEventListener(){},crypto,AbortController,state:{modules:{}},updateModuleUi(){},updateUnloadGuard(){},showToast:value=>toasts.push(value),promptDirtyClose:async()=>answer,hasPy:false,convertFile:async file=>{conversions.push(file);return true;},apiFetch:async(url,options)=>{
    assert.equal(url,'/api/pdf/workspace');const payload=JSON.parse(options.body);calls.push(payload);let result;
    if(payload.action==='prepare_ink'){result={ok:true,width:120,height:80};}
    else if(payload.action==='open'){if(openGate)await openGate.promise;result=data('session-'+(++sequence),payload.path);sessions.set(result.session,result);}
    else {const current=sessions.get(payload.session);assert.ok(current,'Request must own a live session');
      if(payload.action==='close'){sessions.delete(payload.session);result={ok:true};}
      else if(payload.action==='save'){if(saveFails)return{ok:false,json:async()=>({ok:false,error_code:'pdf_save_failed'})};result={...current,dirty:false};sessions.set(payload.session,result);}
      else {result={...current,page:payload.page??current.page,dirty:payload.action==='edit'?true:current.dirty,revision:payload.action==='edit'?current.revision+1:current.revision};sessions.set(payload.session,result);}
    }
    return{ok:true,json:async()=>result};
  }});
  vm.runInContext(source('features/pdf.js'),context);
  return{window,context,calls,conversions,toasts,sessions,created,node:id=>document.getElementById('pdf-'+id),set answer(value){answer=value;},set saveFails(value){saveFails=value;},set openGate(value){openGate=value;},set capabilityGate(value){capabilityGate=value;}};
}
test('ordinary PDF reading hides mutations and keeps original page navigation available',async()=>{
  const h=harness();assert.equal(await h.window.ReadMDPdf.read('original.pdf'),true);
  assert.equal(h.node('title').textContent,'pdf.readTitle · original.pdf');assert.equal(h.node('status').textContent,'pdf.readHint');assert.equal(h.node('hits').children.length,0);
  for(const id of ['save','undo','redo','rotate','delete-page'])assert.equal(h.node(id).hidden,true);
  assert.equal(h.node('next').hidden,false);assert.equal(h.node('edit-original').hidden,false);assert.equal(h.node('extract-markdown').disabled,false);
  await h.node('rotate').onclick();assert.equal(h.calls.some(call=>call.action==='edit'),false,'Even a stale programmatic callback cannot edit in reading mode');
  await h.node('next').onclick();assert.equal(h.calls.at(-1).action,'render');assert.equal(h.calls.at(-1).page,1);assert.equal(h.window.ReadMDPdf.dirty,false);assert.equal(h.conversions.length,0);
});
test('editing the PDF requires an explicit action while the existing editor entry stays available',async()=>{
  const h=harness();await h.window.ReadMDPdf.read('original.pdf');await h.node('edit-original').onclick();
  assert.equal(h.node('title').textContent,'pdf.title · original.pdf');assert.equal(h.node('save').hidden,false);assert.equal(h.node('hits').children.length,1);
  await h.node('rotate').onclick();assert.equal(h.window.ReadMDPdf.dirty,true);
  h.answer='discard';assert.equal(await h.window.ReadMDPdf.open('other.pdf'),true);assert.equal(h.node('save').hidden,false,'Explicit Edit PDF keeps its original behavior');
});
test('conversion occurs only after explicit extraction and a successful PDF close',async()=>{
  const h=harness();await h.window.ReadMDPdf.read('original.pdf');assert.deepEqual(h.conversions,[]);
  await h.node('extract-markdown').onclick();assert.deepEqual(h.conversions,['original.pdf']);assert.equal(h.calls.at(-1).action,'close');assert.equal(h.sessions.size,0);assert.equal(h.node('modal').classList.contains('hidden'),true);
});
test('cancelled dirty close and failed save prevent extraction and preserve the edited PDF',async()=>{
  for(const action of ['cancel','save']){
    const h=harness();await h.window.ReadMDPdf.open('edited.pdf');await h.node('rotate').onclick();h.answer=action;h.saveFails=true;
    assert.equal(await h.node('extract-markdown').onclick(),false);assert.deepEqual(h.conversions,[]);assert.equal(h.window.ReadMDPdf.dirty,true);assert.equal(h.sessions.size,1);assert.equal(h.node('modal').classList.contains('hidden'),false);
  }
});
test('successful save precedes extraction of the saved source and leaves no session leak',async()=>{
  const h=harness();await h.window.ReadMDPdf.open('edited.pdf');await h.node('rotate').onclick();h.answer='save';await h.node('extract-markdown').onclick();
  assert.deepEqual(h.calls.slice(-2).map(call=>call.action),['save','close']);assert.deepEqual(h.conversions,['edited.pdf']);assert.equal(h.sessions.size,0);
});
test('overlapping opens run sequentially and close their own previous session',async()=>{
  const h=harness(),gate=deferred();h.openGate=gate;
  const first=h.window.ReadMDPdf.read('A.pdf'),second=h.window.ReadMDPdf.read('B.pdf');
  assert.equal(await h.window.ReadMDPdf.prepareClose(),false,'App exit cannot pass while open is pending');
  gate.resolve();await Promise.all([first,second]);
  assert.deepEqual(h.calls.map(call=>call.action),['open','close','open']);assert.equal(h.sessions.size,1);assert.equal([...h.sessions.values()][0].path,'B.pdf');
});
test('missing or cancelled PDF capability never starts OCR or conversion',async()=>{
  const h=harness({capability:false});assert.equal(await h.window.ReadMDPdf.read('original.pdf'),false);assert.deepEqual(h.calls,[]);assert.deepEqual(h.conversions,[]);
  delete h.window.ReadMDCapabilities;assert.equal(await h.window.ReadMDPdf.read('original.pdf'),false);assert.deepEqual(h.toasts,['pdf.readUnavailable']);
});
test('password retry keeps original reading mode and never reuses a password on another PDF',async()=>{
  const h=harness();await h.window.ReadMDPdf.read('protected.pdf');h.node('password').value='owned test password';await h.node('retry').onclick();
  assert.equal(h.calls.filter(call=>call.action==='open').at(-1).password,'owned test password');assert.equal(h.node('save').hidden,true);
  await h.window.ReadMDPdf.read('other.pdf');assert.equal(h.calls.filter(call=>call.action==='open').at(-1).password,'');assert.equal(h.node('password').value,'');
});
test('failed explicit extraction reports failure without claiming a Markdown result',async()=>{
  const h=harness();await h.window.ReadMDPdf.read('original.pdf');h.context.convertFile=async()=>false;
  assert.equal(await h.node('extract-markdown').onclick(),false);assert.equal(h.sessions.size,0);assert.equal(h.window.ReadMDPdf.dirty,false);
});
test('quit remains blocked during capability preparation and explicit extraction',async()=>{
  const h=harness(),cap=deferred();h.capabilityGate=cap;const opened=h.window.ReadMDPdf.read('original.pdf');
  assert.equal(await h.window.ReadMDPdf.prepareClose(),false);cap.resolve(true);await opened;
  const conversion=deferred();h.context.convertFile=async file=>{h.conversions.push(file);await conversion.promise;};
  const extracted=h.node('extract-markdown').onclick();while(!h.conversions.length)await Promise.resolve();
  assert.equal(await h.window.ReadMDPdf.prepareClose(),false);assert.equal(await h.window.ReadMDPdf.read('other.pdf'),false);
  conversion.resolve();await extracted;assert.equal(await h.window.ReadMDPdf.prepareClose(),true);
});

function routingHost() {
  const calls=[],input={value:'stale',multiple:true,files:[],click(){}},window={i18n:{t:key=>key},ReadMDPdf:{read:async file=>{calls.push(['read',file]);return true;}}};
  const context=vm.createContext({window,state:{editing:true},hasPy:true,py:{choose_file:async()=> 'native.PDF'},$:()=>input,CONVERT_BINARY_RE:/\.(pdf|docx)$/i,IMG_RE:/\.png$/i,
    syncActiveTabDirty:()=>calls.push(['sync-draft']),deferSessionOperation:action=>action(),waitForSessionTransition:async()=>true,
    loadFile:async(...args)=>calls.push(['load',...args]),convertOrOcr:async(...args)=>calls.push(['convert',...args]),uploadFile:async file=>'uploaded/'+file.name,showToast:text=>calls.push(['toast',text])});
  const render=source('reader/render.js'),from=render.indexOf('async function openDocumentPath(');assert(from>0);vm.runInContext(render.slice(from),context);
  return{context,window,calls,input};
}
test('native and browser Open use the original PDF lane and preserve the underlying editor draft',async()=>{
  const h=routingHost();await h.context.loadFileDialog();assert.deepEqual(h.calls,[['sync-draft'],['read','native.PDF']]);
  h.context.hasPy=false;h.input.files=[{name:'browser.pdf'}];await h.context.loadFileDialog();assert.equal(h.input.multiple,false);await h.input.onchange();
  assert.deepEqual(h.calls.slice(-2),[['sync-draft'],['read','uploaded/browser.pdf']]);assert.equal(h.calls.some(call=>['load','convert'].includes(call[0])),false);
  await h.context.openDocumentPath('report.docx');assert.deepEqual(h.calls.at(-1),['convert','report.docx','convert']);
});
test('PDF opens wait for session/lifecycle transitions and never fall back when the viewer is missing',async()=>{
  const h=routingHost(),gate=deferred();h.window.ReadMDLifecycle={whenIdle:()=>gate.promise};const pending=h.context.openDocumentPath('original.pdf');await Promise.resolve();assert.deepEqual(h.calls,[]);gate.resolve();await pending;
  delete h.window.ReadMDPdf;assert.equal(await h.context.openDocumentPath('other.pdf'),false);assert.deepEqual(h.calls.at(-1),['toast','pdf.readUnavailable']);assert.equal(h.calls.some(call=>call[0]==='convert'),false);
});
test('native and uploaded PDF drops stay outside implicit conversion and batch conversion',async()=>{
  const h=routingHost();h.context.loadFile=async(...args)=>h.calls.push(['load',...args]);h.context.enqueueBatchFiles=(...args)=>h.calls.push(['batch',...args]);
  vm.runInContext(source('core/dragdrop.js'),h.context);
  assert.equal(h.context.classifyDroppedEntry({name:'original.PDF'}),'pdf');
  await h.context.handleDroppedEntries([{name:'one.pdf',path:'native/one.pdf'},{name:'two.pdf',file:{name:'two.pdf'}}]);
  assert.deepEqual(h.calls.filter(call=>call[0]==='read'),[['read','native/one.pdf'],['read','uploaded/two.pdf']]);assert.equal(h.calls.some(call=>['convert','batch','load'].includes(call[0])),false);
});
test('PDF reading keys exist in all 46 locales without copied English sentences',()=>{
  const keys=['pdf.readTitle','pdf.readHint','pdf.editOriginalAction','pdf.extractMarkdown','pdf.readUnavailable','dialog.dropOriginalPdf','pdf.insertInk','pdf.placeInk','pdf.inkStroke'];
  const en=JSON.parse(fs.readFileSync(path.join(root,'assets/i18n/en.json'),'utf8')),files=fs.readdirSync(path.join(root,'assets/i18n')).filter(file=>file.endsWith('.json')&&file!=='meta.json');assert.equal(files.length,46);
  for(const file of files){const locale=JSON.parse(fs.readFileSync(path.join(root,'assets/i18n',file),'utf8'));for(const key of keys)assert.ok(typeof locale[key]==='string'&&locale[key].trim(),file+': '+key);if(file!=='en.json')for(const key of ['pdf.readHint','pdf.readUnavailable','dialog.dropOriginalPdf'])assert.notEqual(locale[key],en[key],file+': '+key);}
});
test('explicit extraction waits for the virtual document transition and requests only a preview',async()=>{
  const render=source('reader/render.js'),from=render.indexOf('async function convertFile('),to=render.indexOf('function showConvertWarns(',from);assert(from>0&&to>from);
  const transition=deferred(),calls=[],context=vm.createContext({window:{i18n:{t:key=>key}},ensureModule:async()=>true,busy(){},showToast(){},showConvertWarns(){},apiMessage:()=>'',
    apiFetch:async url=>{calls.push(url);return{ok:true,json:async()=>({name:'original.pdf',content:'# Extracted',dir:'',fixes:[]})};},renderVirtual:async(...args)=>{calls.push(args);return transition.promise;}});
  vm.runInContext(render.slice(from,to),context);let settled=false;const pending=context.convertFile('original.pdf').then(result=>{settled=true;return result;});
  while(calls.length<2)await Promise.resolve();assert.equal(settled,false);assert.match(calls[0],/preview=1/);assert.equal(calls[1][5].originPath,'original.pdf');transition.resolve(true);assert.equal(await pending,true);
});
test('owned drawing preparation is read-only; placement maps the current page into a native ink operation',async()=>{
  const h=harness();await h.window.ReadMDPdf.open('ink.pdf');await h.node('insert-ink').onclick();const input=h.created.at(-1);
  input.files=[{size:100,text:async()=>'<owned synthetic SVG>'}];await input.onchange();assert.equal(h.calls.at(-1).action,'prepare_ink');assert.equal(h.window.ReadMDPdf.dirty,false);assert.equal(h.node('stage').style.cursor,'crosshair');
  let stopped=false;h.node('stage').listeners.click({clientX:300,clientY:400,preventDefault(){},stopImmediatePropagation(){stopped=true;}});
  for(let i=0;i<20&&!h.window.ReadMDPdf.dirty;i++)await new Promise(setImmediate);
  const edit=h.calls.find(call=>call.action==='edit');assert(edit);assert.equal(edit.operation.kind,'ink');assert.equal(edit.operation.page,0);assert.equal(edit.operation.svg,'<owned synthetic SVG>');assert.equal(edit.operation.stroke,undefined,'The browser cannot authorize or compute the shared brush shape');
  const m=edit.operation.transform;assert.equal(Math.hypot(m[2],m[3]),Math.hypot(m[4],m[5]));assert.equal(stopped,true);assert.equal(h.window.ReadMDPdf.dirty,true);
});
test('an obsolete drawing file callback cannot arm ink on another page or PDF',async()=>{
  const h=harness(),read=deferred();await h.window.ReadMDPdf.open('A.pdf');await h.node('insert-ink').onclick();const input=h.created.at(-1);input.files=[{size:100,text:()=>read.promise}];const pending=input.onchange();
  await h.window.ReadMDPdf.open('B.pdf');read.resolve('<old drawing>');await pending;
  assert.equal(h.calls.some(call=>call.action==='prepare_ink'||call.action==='edit'),false);assert.equal(h.node('stage').style.cursor,'');assert.equal(h.window.ReadMDPdf.dirty,false);
});
test('reading mode cannot arm drawing insertion, even through a stale programmatic callback',async()=>{
  const h=harness();await h.window.ReadMDPdf.read('read.pdf');const before=h.created.length;await h.node('insert-ink').onclick();assert.equal(h.created.length,before);assert.equal(h.node('insert-ink').hidden,true);
});
