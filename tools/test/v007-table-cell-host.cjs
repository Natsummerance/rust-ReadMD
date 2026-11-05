'use strict';
// Source-only CM/parser/VM/protocol controls. This is NOT a Rust service,
// model, browser, SQLite or cross-process persistence certificate.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const SOURCE=path.resolve(__dirname,'../..'),PRIVATE=!fs.existsSync(path.join(SOURCE,'assets/vendor/codemirror.bundle.js'));
const ROOT=PRIVATE?path.resolve(SOURCE,'../../../../../rust-ReadMD'):SOURCE,OWN=PRIVATE?path.dirname(SOURCE):path.join(ROOT,'ui-tests/test-results/table-cell-source-only');
const FIXTURE=path.join(SOURCE,'tests/fixtures/table-cell/v007');
const read=name=>fs.readFileSync(fs.existsSync(path.join(SOURCE,name))?path.join(SOURCE,name):path.join(ROOT,name),'utf8');
const expected=JSON.parse(fs.readFileSync(path.join(FIXTURE,'expectations.json'))),initialBytes=Buffer.from(expected.physical.initialBase64,'base64'),clone=value=>JSON.parse(JSON.stringify(value));
const lf=text=>String(text).replace(/^\uFEFF/,'').replace(/\r\n?/g,'\n'),sha=text=>crypto.createHash('sha256').update(text).digest('hex');
const apply=(text,edits)=>{for(const e of [...edits].reverse())text=text.slice(0,e.from)+e.insert+text.slice(e.to);return text;};
const deferred=()=>{let resolve;return{promise:new Promise(r=>{resolve=r;}),resolve:value=>resolve(value)};};
const evidence=process.env.READMD_TABLE_CELL_EVIDENCE||path.join(OWN,'evidence');
assert(path.isAbsolute(evidence)&&path.resolve(evidence).startsWith((PRIVATE?OWN:path.dirname(ROOT))+path.sep));fs.mkdirSync(evidence,{recursive:true,mode:0o700});
function service(initial=initialBytes.toString('utf8')){
 const dir=fs.mkdtempSync(path.join(evidence,'case-')),file=path.join(dir,'双表.md');fs.writeFileSync(file,initial,{flag:'wx',mode:0o600});
 let snapshot=null,nextId=0,hold=null,failure=null;const calls=[],receipts=new Map(),undo=[],redo=[];
 const diskRevision=()=>sha(fs.readFileSync(file));
 const response=(data,status=200)=>({ok:status===200,status,json:async()=>clone(data)});
 function advance(){snapshot.generation++;snapshot.review_revision++;snapshot.revision=sha(snapshot.content);snapshot.last_event_id++;}
 function map(at,edits,end=false){let delta=0;for(const e of edits){if(at<e.from)break;if(at>=e.to){delta+=e.insert.length-e.to+e.from;continue;}return e.from+delta+(end?e.insert.length:0);}return at+delta;}
 async function fetch(url,options={}){
  const body=options.body?JSON.parse(options.body):{};calls.push({url,body:clone(body)});
  if(hold&&hold.action===body.action){const gate=hold;hold=null;gate.entered.resolve();await gate.release.promise;}
  if(url==='/api/save'){
   if(failure?.action==='save'){failure=null;return response({ok:false,error:'controlled_save_failure'},503);}
   if(body.expected_revision!==diskRevision())return response({ok:false,error:'controlled_external_change'},409);
   if(body.review_session?.document_id!==snapshot.document_id||body.review_session.generation!==snapshot.generation||body.review_session.review_revision!==snapshot.review_revision)return response({ok:false,error:'controlled_review_cas'},409);
   const physical=body.encoding==='utf-8-sig'&&!body.content.startsWith('\ufeff')?'\ufeff'+body.content:body.content;
   fs.writeFileSync(file,physical);snapshot.disk_revision=diskRevision();snapshot.review_revision++;
   return response({ok:true,path:file,mtime:2,revision:diskRevision(),saved_content:body.content,review:clone(snapshot)});
  }
  assert.equal(url,'/api/review','No network/provider or unrelated API');
  if(body.action==='open'||body.action==='resume'){
   if(!snapshot)snapshot={document_id:'owned-document',path:file,content:lf(body.content??fs.readFileSync(file,'utf8')),generation:1,review_revision:0,revision:'',disk_revision:diskRevision(),encoding:'utf-8-sig',line_ending:'crlf',changes:[],last_event_id:0};
   snapshot.revision=sha(snapshot.content);
   if(snapshot.disk_revision!==diskRevision()){snapshot.recovery={reason:'external_change'};snapshot.status='needs_recovery';}
   return response(body.action==='resume'?{found:true,snapshot:clone(snapshot)}:snapshot);
  }
  if(body.action==='read')return response(snapshot);
  if(receipts.has(body.call_id))return response(receipts.get(body.call_id));
  if(body.document_id!==snapshot.document_id||body.generation!==snapshot.generation||body.review_revision!==snapshot.review_revision)return response({error_code:'review_stale_generation'},409);
  if(failure?.action===body.action&&!failure.afterCommit){failure=null;return response({error_code:'review_persistence_failed'},503);}
  const before=clone(snapshot);
  if(body.action==='edit'||body.action==='propose'){
   for(const change of snapshot.changes){change.from=map(change.from,body.edits);change.to=map(change.to,body.edits,true);}
   snapshot.content=apply(snapshot.content,body.edits);
   if(body.action==='propose')for(const e of body.edits)snapshot.changes.push({id:'pending-'+(++nextId),from:e.from,to:e.from+e.insert.length,original:before.content.slice(e.from,e.to),replacement:e.insert,origin:'ai',decision:'pending',application:'applied',version:1,taken_over:false});
   advance();undo.push({before,after:clone(snapshot),kind:body.action});redo.length=0;
  }else if(body.action==='undo'||body.action==='redo'){
   const source=body.action==='undo'?undo:redo,target=body.action==='undo'?redo:undo,entry=source.pop();assert(entry,'Controlled history entry exists');target.push(entry);
   const chosen=body.action==='undo'?entry.before:entry.after;snapshot.content=chosen.content;snapshot.changes=clone(chosen.changes);advance();
  }else throw Error('Unsupported controlled action '+body.action);
  receipts.set(body.call_id,clone(snapshot));
  if(failure?.action===body.action&&failure.afterCommit){failure=null;return response({error_code:'review_persistence_failed'},503);}
  return response(snapshot);
 }
 return{dir,file,calls,fetch,snapshot:()=>clone(snapshot),disk:()=>fs.readFileSync(file),revision:diskRevision,history:undo,
  fail(action,afterCommit=false){failure={action,afterCommit};},
  pause(action){const gate={action,entered:deferred(),release:deferred()};hold=gate;return gate;},
  external(text){fs.writeFileSync(file,text);},remoteHead(){snapshot.generation++;snapshot.review_revision++;snapshot.revision=sha(snapshot.content+'independent_revision');}};
}
function app(store,{source=store.disk().toString('utf8')}={}){
 const tab={id:'table-tab',path:store.file,name:'双表.md',original:source,content:lf(source),encoding:'utf-8-sig',revision:store.revision(),mtime:1,editGeneration:0,isDirty:false};
 const state={editing:true,original:source,file:tab.path,encoding:tab.encoding,tabs:[tab],activeTabId:tab.id},calls=store.calls,toasts=[],events=[];
 const listeners=new Map(),area={value:source,focus(){}},context={console,TextEncoder,Uint32Array,setTimeout,clearTimeout,setInterval,clearInterval,
  document:{documentElement:{style:{}},addEventListener(name,fn){const list=listeners.get(name)||[];list.push(fn);listeners.set(name,list);},getElementById:id=>id==='edit-area'?area:null,querySelectorAll:()=>[],createElement:()=>({setAttribute(){},addEventListener(){},appendChild(){}})},
  localStorage:{getItem:()=>null},state,getActiveTab:()=>state.tabs.find(t=>t.id===state.activeTabId),$:id=>id==='edit-area'?area:null,
  i18n:{t:(key,p)=>p?.count===undefined?key:`${key}:${p.count}`},crypto:{randomUUID:crypto.randomUUID},showToast:v=>toasts.push(v),
  ReadMDInlineCompletion:{accept:()=>false},ReadMDTask:{run:(_,fn)=>fn()},renderActiveTab:async()=>{},renderTabsBar(){},updateUnloadGuard(){},updateStatus(){},apiFetch:store.fetch,
  CustomEvent:class{constructor(type,options){this.type=type;Object.assign(this,options);}},addEventListener(){},dispatchEvent:event=>{events.push(event);return true;}};
 context.window=context;vm.createContext(context);
 for(const file of['assets/vendor/codemirror.bundle.js','assets/vendor/marked.min.js','assets/js/editor/md-transforms.js','assets/js/editor/editor.js','assets/js/editor/preview.js','assets/js/editor/review.js'])vm.runInContext(read(file),context,{filename:file});
 const CM=context.ReadMDCodeMirror,review=context.ReadMDReview;
 const view={transactions:[],focus(){},dispatch(spec){const tx=spec instanceof CM.Transaction?spec:view.state.update(spec);view.transactions.push(tx);view.state=tx.state;
   review.onEditorUpdate({view,transactions:[tx]});if(tx.docChanged){tab.editGeneration++;tab.content=view.state.doc.toString();tab.isDirty=lf(tab.original)!==tab.content;}
 }};
 view.state=CM.EditorState.create({doc:lf(source),selection:{anchor:Math.min(expected.target.from,lf(source).length)},extensions:[CM.history(),CM.markdown({base:CM.markdownLanguage}),CM.EditorState.allowMultipleSelections.of(true)]});
 context.schedulePreview=()=>{};
 context.__tableView=view;vm.runInContext('cmView=__tableView;window.cmView=cmView;',context);review.mount(view,tab,source);
 return{context,CM,review,view,tab,state,calls,toasts,events,store,text:()=>view.state.doc.toString(),
  ready:async()=>{await review.ensure(tab);assert.equal(await review.flush(tab),true);},
  select(anchor,head=anchor){view.dispatch({selection:{anchor,head}});},type(edit){view.dispatch({changes:edit,userEvent:'input.type'});},
  hit:()=>context.cmTableCellContext(view),capture:()=>review.captureManualRange(expected.target.from,expected.target.to,tab),apply:(stamp,insert='42😀')=>review.applyManualRange(insert,stamp,tab),
  async pending(){const from=expected.canonical.initial.length-'待审段落 ORIGINAL_AI_SCOPE。\n'.length+'待审段落 '.length;
   await review.propose([{from,to:from+'ORIGINAL_AI_SCOPE'.length,insert:'PENDING_AI_CANDIDATE'}],await review.capture(tab),tab);assert.equal(view.state.doc.toString(),expected.canonical.pending);return clone(review.snapshot(tab).changes[0]);},
  save:()=>context.saveEdit(),async history(redo=false){assert.equal(review.history(redo,tab),true);assert.equal(await review.flush(tab),true);}};
}
function modal(f){
 const nodes=[];let focus=null;
 function element(tag='div',id=''){
  const classes=new Set(),node={tag,id,children:[],style:{setProperty(key,value){this[key]=String(value);}},dataset:{},attributes:{},listeners:{},value:'',disabled:false,textContent:'',parentElement:null,
   classList:{add:(...names)=>names.forEach(n=>classes.add(n)),remove:(...names)=>names.forEach(n=>classes.delete(n)),contains:n=>classes.has(n)},
   setAttribute(k,v){this.attributes[k]=String(v);},getAttribute(k){return this.attributes[k]??null;},
   append(...items){for(const item of items){this.children.push(item);item.parentElement=this;}},appendChild(item){this.append(item);return item;},
   before(item){const list=this.parentElement.children;list.splice(list.indexOf(this),0,item);item.parentElement=this.parentElement;},
   remove(){if(this.parentElement)this.parentElement.children=this.parentElement.children.filter(n=>n!==this);this.parentElement=null;},
   addEventListener(k,fn){this.listeners[k]=fn;},focus(){focus=this;},querySelector(){return null;},querySelectorAll(selector){const all=[];function walk(node){for(const child of node.children){all.push(child);walk(child);}}walk(this);return selector==='button'?all.filter(n=>n.tag==='button'):this.children;},closest(selector){for(let n=this;n;n=n.parentElement)if(selector==='.hidden'&&n.classList.contains('hidden'))return n;return null;},click(){if(!this.disabled)return this.listeners.click?.();}};
  Object.defineProperty(node,'innerHTML',{set(){this.children=[];}});nodes.push(node);return node;
 }
 const root=element('div','table-modal'),body=element(),picker=element('div','table-grid-picker'),label=element('div','table-grid-label');root.append(body);body.append(picker,label);root.classList.add('hidden');
 const connected=node=>node===root||!!node.parentElement&&connected(node.parentElement),get=id=>nodes.find(n=>n.id===id&&connected(n))||null;
 f.context.$=get;f.context.document.getElementById=get;f.context.document.createElement=tag=>element(tag);f.context.ReadMDModal={close:node=>node.classList.add('hidden')};
 function escape(){
  // Exercise the actual shared Escape/closeTop code, with controlled layout
  // and stack only. It is not a browser/native keyboard certification.
  const source=read('assets/js/core/modal.js'),begin=source.indexOf('  function closeTop()'),end=source.indexOf('  function onFocusIn(',begin);assert(begin>=0&&end>begin);
  const event={key:'Escape',isComposing:false,keyCode:27,preventDefault(){},stopImmediatePropagation(){}};
  vm.runInNewContext(source.slice(begin,end)+'\nonKeyDown(event);',{stack:[{el:root}],guards:new Map(),isShown:el=>!el.classList.contains('hidden'),focusables:()=>[],document:f.context.document,event});
 }
 return{root,get,buttons:()=>nodes.filter(n=>n.dataset.tableRow!==undefined&&connected(n)),focus:()=>focus,escape};
}
module.exports={ROOT,OWN,SOURCE,FIXTURE,initialBytes,expected,service,app,modal,lf,sha,clone,apply};
