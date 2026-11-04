'use strict';
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),crypto=require('node:crypto');
const source=name=>fs.readFileSync(path.join(__dirname,'../assets/js/features/',name),'utf8');
const consumerSource=source('ai-inline-stream.js'),inlineSource=source('ai-inline.js');
const encoder=new TextEncoder(),taskId='owned-inline',prefix='中文😀 synthetic prefix ',suffix=' suffix';
const contextRevision=crypto.createHash('sha256').update(Buffer.byteLength(prefix)+':'+prefix+suffix).digest('hex');
const tick=()=>new Promise(resolve=>setImmediate(resolve));
function barrier(){let resolve;const promise=new Promise(done=>{resolve=done;});return{promise,resolve};}
function trackedController(){
 const control=new AbortController(),listeners=new Set(),add=control.signal.addEventListener.bind(control.signal),remove=control.signal.removeEventListener.bind(control.signal);
 control.signal.addEventListener=(type,listener,options)=>{if(type==='abort')listeners.add(listener);return add(type,listener,options);};
 control.signal.removeEventListener=(type,listener,options)=>{if(type==='abort')listeners.delete(listener);return remove(type,listener,options);};control.listeners=listeners;return control;
}
function response(chunks){let cancelled=false;const body=new ReadableStream({start(controller){for(const chunk of chunks)controller.enqueue(typeof chunk==='string'?encoder.encode(chunk):chunk);controller.close();},cancel(){cancelled=true;}});return{ok:true,status:200,headers:new Headers({'content-type':'text/event-stream; charset=utf-8'}),body,cancelled:()=>cancelled};}
function frame(value,identity={task_id:taskId,context_revision:contextRevision}){return 'data: '+JSON.stringify({...identity,...value})+'\r\n\r\n';}
const suggestion=text=>frame({type:'suggestion',completion:text,model:'synthetic-fast'}),done=(text='final',reason='provider_finished')=>frame({type:'done',ok:true,completion:text,finish_reason:reason,model:'synthetic-fast'});
const window={crypto:crypto.webcrypto};vm.runInNewContext(consumerSource,{window,TextDecoder,TextEncoder,DOMException,Uint8Array});
const protocol=window.ReadMDCompletionStream,options={taskId,contextRevision};

test('revision hashes the UTF-8 prefix byte length and exact context',async()=>{
 assert.equal(await protocol.revision(prefix,suffix),contextRevision);
 assert.notEqual(await protocol.revision('abc','def'),await protocol.revision('abcd','ef'));
});
test('real UTF-8/CRLF fragments and multiline SSE yield cumulative snapshots, including retraction',async()=>{
 const payload=suggestion('中文😀')+suggestion('')+'data: {"type":"suggestion",\r\ndata: "completion":"替换", "task_id":"'+taskId+'","context_revision":"'+contextRevision+'"}\r\n\r\n'+done('最终');
 const bytes=encoder.encode(payload),snapshots=[],control=trackedController(),r=response([...bytes].map(byte=>Uint8Array.of(byte)));
 const terminal=await protocol.consume(r,{...options,signal:control.signal,onSuggestion:value=>snapshots.push(value.completion)});
 assert.deepEqual(snapshots,['中文😀','','替换']);assert.equal(terminal.completion,'最终');assert.equal(r.body.locked,false);assert.equal(control.listeners.size,0);
});
for(const reason of ['provider_finished','ready_unit','cached'])test('terminal completion preserves legitimate finish reason '+reason,async()=>{
 const terminal=await protocol.consume(response([done('accepted',reason)]),options);assert.equal(terminal.finish_reason,reason);
});
test('a complete bare-CR SSE frame is valid at EOF',async()=>{const terminal=await protocol.consume(response([done().replace(/\r\n/g,'\r')]),options);assert.equal(terminal.ok,true);});
test('EOF and an undelimited done are incomplete and do not become terminal results',async()=>{
 for(const ending of ['',done().trim(),done().trim()+'\n']){
  const r=response([suggestion('provisional')+ending]),snapshots=[];
  await assert.rejects(protocol.consume(r,{...options,onSuggestion:value=>snapshots.push(value.completion)}),error=>error.code==='incomplete_completion_stream');
  assert.deepEqual(snapshots,['provisional']);assert.equal(r.body.locked,false);
 }
});
test('bad UTF-8, malformed JSON, task/context mismatch, unknown frames and invalid terminal reasons fail strictly',async()=>{
 const bodies=[Buffer.concat([Buffer.from('data: {"type":"done","ok":true,"completion":"bad'),Buffer.from([0xc0,0xaf]),Buffer.from('"}\n\n')]),
  'data: {broken}\n\n',frame({type:'suggestion',completion:'wrong',task_id:'other'}),frame({type:'suggestion',completion:'wrong',context_revision:'b'.repeat(64)}),
  frame({type:'unknown'}),done('bad','unknown'),frame({type:'done',ok:false,completion:'bad',finish_reason:'provider_finished'}),suggestion('x'.repeat(1001))];
 for(const body of bodies){const r=response([body]);await assert.rejects(protocol.consume(r,options));assert.equal(r.body.locked,false);}
});
test('abort within one network chunk rejects before a later snapshot and releases the reader and listener',async()=>{
 const control=trackedController(),snapshots=[],r=response([suggestion('first')+suggestion('late')+done()]);
 await assert.rejects(protocol.consume(r,{...options,signal:control.signal,onSuggestion:value=>{snapshots.push(value.completion);control.abort();}}),error=>error.name==='AbortError');
 assert.deepEqual(snapshots,['first']);assert.equal(r.body.locked,false);assert.equal(control.listeners.size,0);
});
test('abort releases a blocked reader; a terminal frame ignores later frames in the same chunk',async()=>{
 const control=trackedController();let closed=false;const body=new ReadableStream({cancel(){closed=true;}});
 const reading=protocol.consume({headers:new Headers({'content-type':'text/event-stream'}),body},{...options,signal:control.signal});control.abort();await assert.rejects(reading,error=>error.name==='AbortError');
 assert(closed);assert.equal(body.locked,false);assert.equal(control.listeners.size,0);
 const snapshots=[];await protocol.consume(response([suggestion('first')+done()+suggestion('late')]),{...options,onSuggestion:value=>snapshots.push(value.completion)});assert.deepEqual(snapshots,['first']);
});
test('JSON fallback keeps legacy results but validates identities and finish reasons when present',()=>{
 assert.equal(protocol.result({ok:true,completion:'legacy'},options).completion,'legacy');
 for(const value of [{ok:true,completion:'wrong',task_id:'other'},{ok:true,completion:'wrong',context_revision:'b'.repeat(64)},{ok:true,completion:1},{ok:true,completion:'wrong',finish_reason:'unknown'}])assert.throws(()=>protocol.result(value,options));
});

// Production Controller and consumer, deterministic CodeMirror/protocol doubles.
// These tests are not evidence for Rust, real IME, saving or native UI.
function documentValue(value){return{length:value.length,toString:()=>value,sliceString:(a,b)=>value.slice(a,b),lineAt:pos=>({text:value.slice(0,pos).split('\n').at(-1)})};}
function fixture(settings={}){
 let Controller,handlers,controller,visible=null;const calls=[],controllers=[],transactions=[],cancellations=[],events=new Map(),status={dataset:{},title:''};
 const tab={id:'tab-a',reviewSession:{document_id:'doc-a',generation:1,review_revision:1}},other={id:'tab-b',reviewSession:{document_id:'doc-b',generation:1,review_revision:1}};
 const state={editing:true,activeTabId:tab.id,tabs:[tab,other],ai:{config:{current:{provider_id:'synthetic-provider',model:'synthetic-chat'}},providers:[{id:'synthetic-provider',autocomplete:{mode:'model',model:'synthetic-fast'}}]}};
 const view={hasFocus:true,composing:false,state:{doc:documentValue('Synthetic prose with sufficient context '),selection:{main:{head:40,empty:true},ranges:[{}]}},dom:{querySelector:()=>null},scrollDOM:{scrollTop:0},requestMeasure(){},
  dispatch(spec){const before=view.state.doc.toString(),changes=spec.changes;const docChanged=Boolean(changes),selectionSet=Boolean(spec.selection);
   if(spec.effects)visible=spec.effects.value?.text||null;
   if(changes){transactions.push(spec);const{from,to=from,insert}=changes;view.state.doc=documentValue(before.slice(0,from)+insert+before.slice(to));}
   if(spec.selection)view.state.selection.main={head:spec.selection.anchor,empty:true};
   controller?.update({docChanged,selectionSet,focusChanged:false,transactions:[{isUserEvent:name=>(spec.userEvent||'').startsWith(name)}],changes:{iterChanges:callback=>{if(changes)callback(changes.from,changes.to??changes.from,0,0,{toString:()=>changes.insert});}}});
  }};view.state.selection.main.head=view.state.doc.length;
 const CM={StateEffect:{define(){const token={of:value=>({value,is:other=>other===token})};return token;}},StateField:{define:value=>value},WidgetType:class{},Decoration:{none:[],set:value=>value,widget:()=>({range:pos=>pos})},
  EditorView:{decorations:{from:()=>[]},domEventHandlers:value=>{handlers=value;return value;}},ViewPlugin:{fromClass:value=>{Controller=value;return value;}},completionStatus:()=>null};
 const review={snapshot:owner=>owner.reviewSession,flush:async owner=>settings.flush?settings.flush(owner,view,controller):true,cancelAi:async(id,running)=>{cancellations.push({id,running:running()});return 'cancelling';}};
 const window={crypto:crypto.webcrypto,ReadMDReview:review,i18n:{t:key=>key},addEventListener:(name,callback)=>events.set(name,callback),matchMedia:()=>({matches:true})};
 const context=vm.createContext({window,document:{hidden:false,getElementById:()=>status,addEventListener(){}},state,editorPrefs:{aiCompletion:true},cmView:view,TextEncoder,TextDecoder,DOMException,Uint8Array,Intl,Date,setTimeout,clearTimeout,queueMicrotask,
  AbortController:class{constructor(){const control=trackedController();controllers.push(control);return control;}},getActiveTab:()=>state.tabs.find(value=>value.id===state.activeTabId),
  apiFetch:async(url,request)=>{const body=JSON.parse(request.body);calls.push({url,body});if(settings.reply)return settings.reply(body,request);return{ok:true,headers:new Headers({'content-type':'application/json'}),json:async()=>({ok:true,completion:'test the evidence.'})};}});
 vm.runInContext(consumerSource,context);vm.runInContext(inlineSource,context);const inline=vm.runInContext('ReadMDInlineCompletion',context);inline.extension(CM);controller=new Controller(view);
 return{context,window,inline,controller,view,tab,other,state,calls,cancellations,controllers,transactions,handlers,status,visible:()=>visible,
  run:()=>controller.request(controller.snapshot(),controller.generation),identity:body=>({task_id:body.task_id,context_revision:body.context_revision}),
  async waitVisible(){for(let n=0;n<100;n++){if(visible)return;await tick();}throw Error('No candidate');}};
}

test('the real revision-only failure is rejected while pending, even with the same editor document object',async()=>{
 const entered=barrier(),release=barrier(),app=fixture({reply:async()=>{entered.resolve();await release.promise;return{ok:true,headers:new Headers({'content-type':'application/json'}),json:async()=>({ok:true,completion:'late candidate'})};}});
 const before=app.view.state.doc,request=app.run();await entered.promise;app.tab.reviewSession={...app.tab.reviewSession,review_revision:2};app.view.dispatch({effects:{value:null}});release.resolve();await request;await tick();
 assert.equal(app.view.state.doc,before);assert.equal(app.visible(),null);assert.equal(app.inline.accept(app.view),false);assert.equal(app.transactions.length,0);assert.equal(app.cancellations.length,1);assert(app.controllers.every(control=>control.listeners.size===0));
});
test('a displayed candidate is cleared by durable generation changes and cannot be accepted',async()=>{
 const app=fixture();await app.run();assert(app.visible());app.tab.reviewSession={...app.tab.reviewSession,generation:2};app.view.dispatch({effects:{value:null}});await tick();
 assert.equal(app.visible(),null);assert.equal(app.inline.accept(app.view),false);assert.equal(app.transactions.length,0);
});
test('Tab accepts a visible streaming partial exactly once and cancels its owned request',async()=>{
 let closed=false;const app=fixture({reply:async body=>({ok:true,headers:new Headers({'content-type':'text/event-stream'}),body:new ReadableStream({start(controller){controller.enqueue(encoder.encode(frame({type:'suggestion',completion:'visible partial'},app.identity(body))));},cancel(){closed=true;}})})});
 const before=app.view.state.doc.toString(),request=app.run();await app.waitVisible();assert.equal(app.inline.accept(app.view),true);assert.equal(app.inline.accept(app.view),false);await request;
 assert.equal(app.view.state.doc.toString(),before+'visible partial');assert.equal(app.transactions.length,1);assert.equal(app.transactions[0].userEvent,'input.complete');assert(closed);assert.equal(app.cancellations.length,1);assert(app.controllers.every(control=>control.listeners.size===0));
});
test('Esc and IME clear candidates and leave the original draft unchanged',async()=>{
 for(const action of ['escape','ime']){
  const app=fixture();await app.run();const before=app.view.state.doc.toString();
  if(action==='escape')assert.equal(app.inline.dismiss(app.view),true);else{app.view.composing=true;app.handlers.compositionstart({},app.view);assert.equal(app.controller.snapshot(),null);}
  assert.equal(app.visible(),null);assert.equal(app.inline.accept(app.view),false);assert.equal(app.view.state.doc.toString(),before);assert.equal(app.transactions.length,0);
 }
});
test('type-through retains just the visible suffix without a second request or document transaction',async()=>{
 const app=fixture();await app.run();const at=app.view.state.doc.length;app.view.dispatch({changes:{from:at,insert:'test '},selection:{anchor:at+5},userEvent:'input.type'});await tick();
 assert.equal(app.visible(),'the evidence.');assert.equal(app.calls.length,1);assert.equal(app.transactions.length,1);assert.equal(app.transactions[0].userEvent,'input.type');
 app.inline.dismiss(app.view);
});
test('automatic preparation waits for the expected local edit acknowledgement before capturing its revision',async()=>{
 let flushes=0;const app=fixture({flush:async(owner,view)=>{flushes++;owner.reviewSession={...owner.reviewSession,generation:2};view.dispatch({effects:{value:null}});return true;}});
 app.inline.request(app.view);await app.waitVisible();assert.equal(flushes,1);assert.equal(app.calls.length,1);assert.equal(app.controller.suggestion.revision,JSON.stringify(['doc-a',2,1]));app.inline.dismiss(app.view);
});
test('type-through survives its own durable edit acknowledgement and still needs only one model request',async()=>{
 const app=fixture({flush:async(owner,view)=>{owner.reviewSession={...owner.reviewSession,generation:owner.reviewSession.generation+1};view.dispatch({effects:{value:null}});return true;}});
 await app.run();const at=app.view.state.doc.length;app.view.dispatch({changes:{from:at,insert:'test '},selection:{anchor:at+5},userEvent:'input.type'});await tick();
 assert.equal(app.visible(),'the evidence.');assert.equal(app.calls.length,1);assert.equal(app.controller.suggestion.revision,JSON.stringify(['doc-a',2,1]));app.inline.dismiss(app.view);
});
test('switching tabs and changing then restoring text discard an old request',async()=>{
 for(const change of ['tab','undo']){
  const entered=barrier(),release=barrier(),app=fixture({reply:async()=>{entered.resolve();await release.promise;return{ok:true,headers:new Headers({'content-type':'application/json'}),json:async()=>({ok:true,completion:'old answer'})};}});
  const request=app.run();await entered.promise;
  if(change==='tab')app.state.activeTabId=app.other.id;else{const text=app.view.state.doc.toString();app.view.state.doc=documentValue(text);app.controller.update({docChanged:true,selectionSet:false,focusChanged:false,transactions:[],changes:{iterChanges(){}}});}
  release.resolve();await request;assert.equal(app.visible(),null);assert.equal(app.transactions.length,0);
 }
});
test('a delayed context hash cannot issue a request after cancellation',async()=>{
 const app=fixture(),entered=barrier(),release=barrier(),real=app.window.ReadMDCompletionStream;
 app.window.ReadMDCompletionStream={...real,revision:async(...args)=>{entered.resolve();await release.promise;return real.revision(...args);}};
 const request=app.run();await entered.promise;app.controller.clear();release.resolve();await request;
 assert.equal(app.calls.length,0);assert.equal(app.cancellations.length,0);assert(app.controllers.every(control=>control.listeners.size===0));
});
test('late JSON failure and cleanup cannot clear a newer request owner',async()=>{
 const entered=barrier(),release=barrier(),nextEntered=barrier(),nextRelease=barrier();let count=0;
 const app=fixture({reply:async()=>{if(++count===1){entered.resolve();await release.promise;throw Error('old failure');}nextEntered.resolve();await nextRelease.promise;return{ok:true,headers:new Headers({'content-type':'application/json'}),json:async()=>({ok:true,completion:'new candidate'})};}});
 const old=app.run();await entered.promise;app.controller.clear();const next=app.run();await nextEntered.promise;const owner=app.controller.pending;release.resolve();await old;assert.equal(app.controller.pending,owner);nextRelease.resolve();await next;assert.equal(app.visible(),'new candidate');assert(app.controllers.every(control=>control.listeners.size===0));
});
test('a provisional stream error retracts its ghost and never becomes an accepted or cached answer',async()=>{
 let count=0;const app=fixture({reply:async body=>{count++;return response([frame({type:'suggestion',completion:'provisional'},app.identity(body))]);}});
 await app.run();assert.equal(app.visible(),null);assert.equal(app.inline.accept(app.view),false);assert.equal(app.transactions.length,0);await app.run();assert.equal(count,2);assert.equal(app.status.dataset.state,'editor.aiCompletionUnavailable');
});
test('registration-race cancellation retries only cancellation and never replays the original completion',async()=>{
 const app=fixture({reply:async body=>({ok:true,headers:new Headers({'content-type':'text/event-stream'}),body:new ReadableStream({start(controller){controller.enqueue(encoder.encode(frame({type:'suggestion',completion:'partial'},app.identity(body))));}})})});
 const reviewSource=fs.readFileSync(path.join(__dirname,'../assets/js/editor/review.js'),'utf8'),cancelSource=reviewSource.slice(reviewSource.indexOf('  async function cancelAi('),reviewSource.indexOf('  async function prepareSave('));
 const ids=[];app.window.ReadMDTask={requestCancel:async id=>{ids.push(id);return ids.length===1?'unknown':'cancelling';}};vm.runInContext(cancelSource,app.context);app.window.ReadMDReview.cancelAi=app.context.cancelAi;
 const request=app.run();await app.waitVisible();app.inline.dismiss(app.view);await request;
 assert.equal(ids.length,2);assert.equal(new Set(ids).size,1);assert.equal(app.calls.length,1);assert(app.controllers.every(control=>control.listeners.size===0));
});
