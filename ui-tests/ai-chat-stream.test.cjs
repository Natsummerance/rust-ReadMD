'use strict';
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const window={};vm.runInNewContext(fs.readFileSync(path.join(__dirname,'../assets/js/features/ai-chat-stream.js'),'utf8'),{window,TextDecoder,DOMException});
const {consume}=window.ReadMDChatStream,taskId='owned-chat',encode=new TextEncoder();
const frame=value=>'data: '+JSON.stringify({task_id:taskId,...value})+'\r\n\r\n',start=frame({type:'start'}),delta=text=>frame({type:'delta',delta:text}),done=frame({type:'done',ok:true});
function response(chunks){let cancelled=0;const body=new ReadableStream({start(controller){for(const value of chunks)controller.enqueue(typeof value==='string'?encode.encode(value):value);controller.close();},cancel(){cancelled++;}});return{ok:true,status:200,headers:new Headers({'content-type':'text/event-stream; charset=utf-8'}),body,cancelled:()=>cancelled};}
test('fragmented UTF-8/CRLF and multiline SSE preserve actual deltas, final usage counts once',async()=>{
 const text=start+': ping\r\n'+delta('中文😀')+frame({type:'usage',usage:{total_tokens:2}})+frame({type:'usage',usage:{total_tokens:7}})+'data: {"type":"delta",\r\ndata: "task_id":"owned-chat","delta":"尾"}\r\n\r\n'+done;
 const bytes=encode.encode(text),deltas=[],usages=[];const r=await consume(response([...bytes].map(value=>Uint8Array.of(value))),{taskId,onDelta:value=>deltas.push(value),onUsage:value=>usages.push(value)});
 assert.equal(deltas.join(''),'中文😀尾');assert.equal(r.done,true);assert.equal(usages.length,1);assert.equal(usages[0].total_tokens,7);
});
test('bare EOF and a terminal frame without its delimiter remain incomplete; partial text is retained',async()=>{
 for(const ending of ['',done.trim()]){const deltas=[],usages=[];await assert.rejects(consume(response([start+delta('partial')+frame({type:'usage',usage:{total_tokens:9}})+ending]),{taskId,onDelta:value=>deltas.push(value),onUsage:value=>usages.push(value)}),error=>error.code==='ai_invalid_response');assert.equal(deltas.join(''),'partial');assert.equal(usages.length,0);}
});
test('invalid UTF-8, malformed JSON, wrong task and unknown frames do not become success',async()=>{
 for(const body of [Buffer.concat([Buffer.from(start),Buffer.from([0xc0,0xaf])]),start+'data: {broken}\n\n',start+frame({type:'delta',task_id:'other',delta:'wrong'}),start+frame({type:'unknown'})])await assert.rejects(consume(response([body]),{taskId}),error=>error.code==='ai_invalid_response');
});
test('error frames retain earlier deltas and do not commit provisional usage',async()=>{
 const deltas=[],usages=[];await assert.rejects(consume(response([start+delta('kept')+frame({type:'usage',usage:{total_tokens:3}})+frame({type:'error',error_code:'ai_network_error'})]),{taskId,onDelta:value=>deltas.push(value),onUsage:value=>usages.push(value)}),error=>error.code==='ai_network_error');assert.equal(deltas.join(''),'kept');assert.equal(usages.length,0);
});
test('abort releases a blocked reader and reports stopped instead of completed',async()=>{
 const control=new AbortController();let stopped=false;const stream=new ReadableStream({start(controller){controller.enqueue(encode.encode(start+delta('first')));},cancel(){stopped=true;}});
 const result=consume({headers:new Headers({'content-type':'text/event-stream'}),body:stream},{taskId,signal:control.signal,onDelta:()=>control.abort()});await assert.rejects(result,error=>error.name==='AbortError');assert.equal(stopped,true);
});
test('text after done cannot append another answer or count usage twice',async()=>{const deltas=[];await consume(response([start+delta('first')+done+delta('late')]),{taskId,onDelta:value=>deltas.push(value)});assert.equal(deltas.join(''),'first');});

test('abort during a delta stops later frames in the same chunk and releases the reader lock', async () => {
 const control = new AbortController(), deltas = [], usages = [];
 const r = response([start + delta('first') + delta('must not append') + frame({type:'usage',usage:{total_tokens:9}}) + done]);
 await assert.rejects(consume(r, {taskId,signal:control.signal,onDelta(value){deltas.push(value);control.abort();},onUsage:value=>usages.push(value)}), error => error.name === 'AbortError');
 assert.deepEqual(deltas, ['first']); assert.deepEqual(usages, []); assert.equal(r.body.locked, false);
});

test('completion and failure release both the reader lock and AbortSignal subscription', async () => {
 for (const complete of [true, false]) {
  const control = trackedController(), r = response([start + delta('answer') + (complete ? done : '')]);
  const result = consume(r, {taskId,signal:control.signal});
  if (complete) await result; else await assert.rejects(result, error => error.code === 'ai_invalid_response');
  assert.equal(r.body.locked, false); assert.equal(control.listeners.size, 0);
 }
});

// Deterministic frontend lifecycle tests exercise the production entry points.
// The protocol doubles below are unit fixtures, not real Rust/browser evidence.
const aiSource = fs.readFileSync(path.join(__dirname, '../assets/js/features/ai.js'), 'utf8');
const tick = () => new Promise(resolve => setImmediate(resolve));
function barrier() { let resolve; const promise = new Promise(done => {resolve = done;}); return {promise,resolve}; }
function trackedController() {
 const control = new AbortController(), listeners = new Set();
 const add = control.signal.addEventListener.bind(control.signal), remove = control.signal.removeEventListener.bind(control.signal);
 control.signal.addEventListener = (type, listener, options) => {if(type === 'abort') listeners.add(listener);return add(type,listener,options);};
 control.signal.removeEventListener = (type, listener, options) => {if(type === 'abort') listeners.delete(listener);return remove(type,listener,options);};
 control.listeners = listeners; return control;
}
class Element {
 constructor(tag = 'div') {this.tagName = tag;this.children = [];this.listeners = new Map();this.style = {};this.dataset = {};this.value = '';this.className = '';this.disabled = false;this.hidden = false;this.scrollHeight = 10;this.clientHeight = 10;this.scrollTop = 0;}
 append(...nodes) {for(const node of nodes)this.appendChild(node);}
 appendChild(node) {this.children.push(node);node.parent = this;return node;}
 prepend(node) {this.children.unshift(node);node.parent = this;}
 replaceChildren(...nodes) {this.children = [];this.append(...nodes);}
 remove() {if(this.parent)this.parent.children = this.parent.children.filter(node => node !== this);this.parent = null;}
 set innerHTML(value) {this.html = value;this.children = [];if(value === '<strong></strong><small></small>')this.append(new Element('strong'),new Element('small'));}
 get innerHTML() {return this.html || '';}
 get classList() {const node = this;return {contains:name=>node.className.split(/\s+/).includes(name),add(name){if(!this.contains(name))node.className += ' ' + name;},remove(name){node.className = node.className.split(/\s+/).filter(value=>value !== name).join(' ');},toggle(name,enabled){const next = enabled === undefined ? !this.contains(name) : enabled;if(next)this.add(name);else this.remove(name);return next;}};}
 matches(selector) {return selector[0] === '.' ? this.classList.contains(selector.slice(1)) : this.tagName === selector;}
 querySelectorAll(selector) {if(selector.startsWith(':scope > '))return this.children.filter(node=>node.matches(selector.slice(9)));return this.children.flatMap(node=>[...(node.matches(selector)?[node]:[]),...node.querySelectorAll(selector)]);}
 querySelector(selector) {return this.querySelectorAll(selector)[0] || null;}
 insertAdjacentHTML() {}
 addEventListener(type, listener) {this.listeners.set(type, listener);}
 setAttribute(name, value) {this[name] = value;}
 focus() {}
}
function chatFixture(options = {}) {
 const provider = {id:'owned-unit',name:'Owned unit',models:['unit-model'],category:'local'};
 const tabA = {id:'tab-a',path:'A.md',name:'A.md',original:'Document A',content:'Document A',editGeneration:0};
 const tabB = {id:'tab-b',path:'B.md',name:'B.md',original:'Document B',content:'Document B',editGeneration:0};
 const state = {tabs:[tabA,tabB],activeTabId:tabA.id,mode:'file',file:'A.md',sourceName:'A.md',editing:false,
  ai:{providers:[provider],messages:[],sessions:[],sessionId:null,sessionEpoch:0,sessUsage:{prompt_tokens:0,completion_tokens:0,total_tokens:0}}};
 const nodes = new Map(), calls = [], toasts = [], copied = [], savedCopies = [], controllers = [], cancelled = [];
 for (const id of ['ai-output','ai-prompt','ai-provider','ai-model','ai-mode','ai-base-url','ai-stream','ai-incognito','ai-endpoint-mode','ai-session','ai-history-list','ai-usage','ai-run','ai-stop','ai-clear-ctx','ai-copy','ai-apply','ai-saveas']) nodes.set(id,new Element());
 nodes.get('ai-prompt').value = 'Question A';nodes.get('ai-provider').value = provider.id;nodes.get('ai-model').value = 'unit-model';nodes.get('ai-stream').checked = true;
 const document = {readyState:'loading',documentElement:{lang:'zh-CN'},addEventListener(){},createElement:tag=>new Element(tag),createElementNS:(_ns,tag)=>new Element(tag)};
 const review = {capture:async tab=>({content:tab.original,editGeneration:tab.editGeneration,generation:1}),cancelAi:async id=>{cancelled.push(id);return 'cancelled';}};
 const window = {i18n:{t:key=>key},addEventListener(){},ReadMDReview:review,
  ReadMDTask:{newTaskId:()=>taskId,run:(_name,callback)=>callback()}};
 const context = vm.createContext({window,document,state,ReadMDReview:review,DOMException,TextDecoder,crypto:require('node:crypto').webcrypto,
  AbortController:class {constructor(){const control = trackedController();controllers.push(control);return control;}},setTimeout,clearTimeout,
  $:id=>nodes.get(id) || null,_t:key=>key,getActiveTab:()=>state.tabs.find(tab=>tab.id === state.activeTabId),
  confirmAction:async()=>true,
  showToast:value=>toasts.push(value),renderSafeMarkdown:value=>value,renderMath(){},
  navigator:{clipboard:{writeText:async value=>copied.push(value)}},
  renderVirtual:async(_type,_name,_dir,value)=>savedCopies.push(value),saveAs:async()=>true,
  async apiFetch(url, request = {}) {
   const body = request.body ? JSON.parse(request.body) : null;calls.push({url,body});
   if(options.api){const result = await options.api(url,request,body);if(result !== undefined)return result;}
   if(url === '/api/ai/chat')return response([start+delta('Complete reply')+frame({type:'usage',usage:{prompt_tokens:2,completion_tokens:3,total_tokens:5}})+done]);
   if(body?.action === 'save')return json({ok:true,session:{...body.session,id:body.session.id || 'saved-session'}});
   if(body?.action === 'delete' || body?.action === 'clear')return json({ok:true});
   if(url === '/api/ai/history')return json({ok:true,sessions:options.sessions || []});
   if(url.startsWith('/api/ai/history?id='))return json({ok:true,session:session(decodeURIComponent(url.split('=')[1]))});
   throw Error('Unexpected unit URL: ' + url);
  }});
 vm.runInContext(fs.readFileSync(path.join(__dirname,'../assets/js/features/ai-chat-stream.js'),'utf8'),context);
 vm.runInContext(aiSource,context);
 Object.assign(context,{ensureAiConfigured:async()=>true,saveAiSelection:options.saveSelection || (async()=>true),
  currentAiProvider:()=>provider,currentAiTemplate:()=>null,onAiProviderChange(){},syncAiKey(){},updateAiContextStrip(){}});
 window.confirm = () => true;
 return {context,state,nodes,calls,toasts,copied,savedCopies,controllers,cancelled,tabA,tabB,
  run:()=>context.runAi('ask'),retry:()=>nodes.get('ai-output').querySelector('.ai-retry-btn'),
  async idle(){for(let n=0;n<30;n++){if(!state.ai.busy && !state.ai.preparing)return;await tick();}throw Error('Unit request did not finish');}};
}
const json = value => ({ok:true,status:200,json:async()=>value});
const session = id => ({id,title:id,provider:'owned-unit',model:'unit-model',messages:[{role:'user',content:id+' question'},{role:'assistant',content:id+' answer'}],usage:{prompt_tokens:0,completion_tokens:0,total_tokens:0}});

test('clearing a preparing request allows a new send and the old config receipt cannot send or release the new owner', async () => {
 const entered = barrier(), release = barrier(), chatEntered = barrier(), chatRelease = barrier();let saves = 0;
 const app = chatFixture({saveSelection:async()=>{if(++saves === 1){entered.resolve();await release.promise;}return true;},api:async url=>{if(url === '/api/ai/chat'){chatEntered.resolve();await chatRelease.promise;}}});
 const old = app.run();await entered.promise;app.context.clearAiContext();app.nodes.get('ai-prompt').value = 'New question';const next = app.run();await chatEntered.promise;
 const owner = app.state.ai.aborter;release.resolve();await old;
 assert.equal(app.state.ai.aborter,owner);assert.equal(app.state.ai.busy,true);assert.equal(app.state.ai.preparing,true);
 chatRelease.resolve();await next;assert.equal(app.calls.filter(call=>call.url === '/api/ai/chat').length,1);assert.ok(app.state.ai.messages.length,JSON.stringify(app.toasts));assert.match(app.state.ai.messages[0].content,/New question$/);assert.equal(app.state.ai.preparing,false);
});

test('a document switch during configuration preparation never sends the old document into the new one', async () => {
 const entered = barrier(), release = barrier();const app = chatFixture({saveSelection:async()=>{entered.resolve();await release.promise;return true;}});
 const request = app.run();await entered.promise;app.state.activeTabId = app.tabB.id;release.resolve();await request;
 assert.equal(app.calls.length,0);assert.equal(app.state.ai.preparing,false);assert.equal(app.state.ai.raw,undefined);assert.equal(app.cancelled.length,0);
});

test('late history loads cannot replace a newer selection, a cleared conversation or a new send', async () => {
 for(const operation of ['switch','clear','send']) {
  const entered = barrier(), release = barrier();const app = chatFixture({api:async url=>{if(url === '/api/ai/history?id=old'){entered.resolve();await release.promise;return json({session:session('old')});}}});
  const old = app.context.onAiSessionChange('old');await entered.promise;
  if(operation === 'switch')await app.context.onAiSessionChange('new');else if(operation === 'clear')app.context.clearAiContext();else await app.run();
  const expected = app.state.ai.sessionId, raw = app.state.ai.raw;release.resolve();await old;
  assert.equal(app.state.ai.sessionId,expected);assert.equal(app.state.ai.raw,raw);assert.equal(app.state.ai.loadingSessionId,null);
 }
});

test('manual save receipts are tied to their original conversation and the most recent save ticket', async () => {
 for(const operation of ['clear','save']) {
  const entered = barrier(), release = barrier();let saves = 0;
  const app = chatFixture({api:async(_url,_request,body)=>{if(body?.action === 'save' && ++saves === 1){entered.resolve();await release.promise;return json({ok:true,session:{id:'late-old-save'}});}}});
  app.state.ai.messages = session('old').messages;const old = app.context.saveCurrentSession(false);await entered.promise;
  if(operation === 'clear'){app.context.clearAiContext();app.state.ai.sessionId = 'new-conversation';}else {assert.equal(await app.context.saveCurrentSession(true),true);}
  const expected = app.state.ai.sessionId;release.resolve();assert.equal(await old,false);assert.equal(app.state.ai.sessionId,expected);assert(!app.toasts.includes('toast.sessionSaved'));
 }
});

test('clearing history invalidates old list/load receipts and uses the real session renderer', async () => {
 const listEntered = barrier(), listRelease = barrier(), loadEntered = barrier(), loadRelease = barrier();let lists = 0;
 const app = chatFixture({api:async(url)=>{if(url === '/api/ai/history' && ++lists === 1){listEntered.resolve();await listRelease.promise;return json({sessions:[session('late-old')]});}if(url === '/api/ai/history?id=late-old'){loadEntered.resolve();await loadRelease.promise;return json({session:session('late-old')});}}});
 app.state.ai.sessions = [session('old')];app.state.ai.sessionId = 'old';app.state.ai.messages = session('old').messages;
 const oldList = app.context.loadAiSessions();await listEntered.promise;const oldLoad = app.context.onAiSessionChange('late-old');await loadEntered.promise;
 await app.context.clearAiSessions();listRelease.resolve();loadRelease.resolve();await Promise.all([oldList,oldLoad]);
 assert.equal(app.state.ai.sessionId,null);assert.equal(app.state.ai.sessions.length,0);assert.equal(app.state.ai.messages[0].content,'old question');assert.equal(app.cancelled.length,0);assert(app.toasts.includes('toast.sessionsCleared'));
});

test('deleting a pending history selection prevents its delayed load without discarding the current conversation', async () => {
 const entered = barrier(), release = barrier();const app = chatFixture({api:async url=>{if(url === '/api/ai/history?id=delete-me'){entered.resolve();await release.promise;return json({session:session('delete-me')});}}});
 app.state.ai.sessionId = 'current';app.state.ai.messages = session('current').messages;
 const load = app.context.onAiSessionChange('delete-me');await entered.promise;await app.context.deleteAiSessionById('delete-me');release.resolve();await load;
 assert.equal(app.state.ai.sessionId,'current');assert.equal(app.state.ai.messages[0].content,'current question');
});

test('a failed partial answer remains copyable/exportable; retry replaces its turn and counts only final usage', async () => {
 let attempts = 0;const app = chatFixture({api:async url=>{if(url === '/api/ai/chat' && ++attempts === 1)return response([start+delta('Partial answer')+frame({type:'usage',usage:{total_tokens:99}})]);}});
 app.state.ai.sessionId = 'prior-session';app.state.ai.messages = session('prior').messages;app.context.renderAiHistory();
 await app.run();assert.equal(app.state.ai.messages.at(-1).incomplete,true,JSON.stringify(app.toasts));assert.equal(app.state.ai.sessUsage.total_tokens,0);assert.equal(app.nodes.get('ai-copy').disabled,false);
 await app.context.copyAi();assert.deepEqual(app.copied,['Partial answer']);await app.context.saveAiAs();assert.deepEqual(app.savedCopies,['Partial answer']);
 const markdown = await app.context.selectedConversationMarkdown();assert.match(markdown,/Partial answer/);assert.match(markdown,/ai\.stoppedSuffix/);assert.equal(app.calls.filter(call=>call.body?.action === 'save').length,0);
 app.retry().onclick();await app.idle();const requests = app.calls.filter(call=>call.url === '/api/ai/chat');
 assert.equal(requests.length,2);assert.deepEqual(requests[1].body.messages,requests[0].body.messages);assert.equal(app.nodes.get('ai-output').querySelectorAll(':scope > .ai-msg').filter(node=>node.classList.contains('user')).length,2);
 assert.equal(app.state.ai.messages.length,4);assert.equal(app.state.ai.messages.at(-1).content,'Complete reply');assert.equal(app.state.ai.sessUsage.total_tokens,5);assert(app.controllers.every(control=>control.listeners.size === 0));
});

test('retry refuses to reuse an old answer after its document or question changes', async () => {
 for(const change of ['document','prompt']) {
  const app = chatFixture({api:async url=>url === '/api/ai/chat' ? response([start+delta('Partial')]) : undefined});await app.run();
  if(change === 'document'){app.state.activeTabId = app.tabB.id;}else app.nodes.get('ai-prompt').value = 'Different question';
  app.retry().onclick();await tick();assert.equal(app.calls.filter(call=>call.url === '/api/ai/chat').length,1);assert(app.toasts.includes('review.conflict'));
 }
});

test('stopping a blocked partial stream keeps incomplete content, enables copy/save and releases its subscriptions', async () => {
 const entered = barrier();let closed = false;
 const app = chatFixture({api:async url=>{if(url !== '/api/ai/chat')return;const body = new ReadableStream({start(controller){controller.enqueue(encode.encode(start+delta('Stopped partial')));entered.resolve();},cancel(){closed = true;}});return {ok:true,status:200,headers:new Headers({'content-type':'text/event-stream'}),body};}});
 const request = app.run();await entered.promise;for(let n=0;n<30 && app.state.ai.raw !== 'Stopped partial';n++)await tick();assert.equal(app.state.ai.raw,'Stopped partial',JSON.stringify(app.toasts));app.state.ai.aborter.abort();await request;
 assert.equal(closed,true);assert.equal(app.state.ai.messages.at(-1).incomplete,true);assert.equal(app.nodes.get('ai-copy').disabled,false);assert.equal(app.nodes.get('ai-saveas').disabled,false);
 await app.context.copyAi();await app.context.saveAiAs();assert.deepEqual(app.copied,['Stopped partial']);assert.deepEqual(app.savedCopies,['Stopped partial']);assert.equal(app.calls.filter(call=>call.body?.action === 'save').length,0);assert(app.controllers.every(control=>control.listeners.size === 0));
});
