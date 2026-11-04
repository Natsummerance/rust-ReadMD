'use strict';
// Owned protocol bytes only: no inference, credential, CORS or browser bridge.
const http=require('node:http'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const prefix=name=>'OWNED_INLINE_'+name+' synthetic writing context 中文😀 ';
const contextRevision=(before,after='')=>crypto.createHash('sha256').update(Buffer.byteLength(before)+':'+before+after).digest('hex');
function barrier(){let resolve;const value={settled:false,promise:new Promise(done=>{resolve=done;})};value.resolve=()=>{value.settled=true;resolve();};return value;}
async function providerFixture(){
 const states=new Map(),requests=[],catalogRequests=[],failures=[],sockets=new Set(),timers=new Set();let catalogMode='normal';
 const plan=name=>{if(!states.has(name)){const state={name,count:0,received:barrier(),closed:barrier(),rows:[],pending:new Set()};state.release=()=>{for(const complete of state.pending)complete();state.pending.clear();};states.set(name,state);}return states.get(name);};
 const server=http.createServer(async(req,res)=>{try{
  for(const header of ['authorization','origin','sec-fetch-site','x-readmd-app-token','x-readmd-token'])assert.equal(req.headers[header],undefined);
  if(req.method==='GET'){
   assert(['/v1/models','/models','/api/tags'].includes(req.url));const state=plan('catalog'),row={url:req.url,mode:catalogMode,closed:false};catalogRequests.push(row);state.received.resolve();res.once('close',()=>{row.closed=true;state.closed.resolve();});
   const finish=()=>{if(!res.destroyed){res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify({data:[{id:'claude-haiku-owned'},{id:'owned-fixed'}]}));}};
   if(catalogMode==='hold'){state.pending.add(finish);return;}
   if(catalogMode==='bad_json'){res.writeHead(200,{'Content-Type':'application/json'});res.end('{broken OWNED_CATALOG_BODY_MUST_NOT_LEAK');return;}
   finish();return;
  }
  assert.equal(req.method,'POST');assert(['/v1/chat/completions','/v1/completions'].includes(req.url));let text='';for await(const chunk of req){text+=chunk;assert(Buffer.byteLength(text)<=1048576);}
  const body=JSON.parse(text);assert.equal(body.max_tokens,128);let context;
  if(body.messages){const user=body.messages.find(message=>message.role==='user');context=JSON.parse(user.content).prefix;}else context=body.prompt;
  const name=context.match(/OWNED_INLINE_([a-z0-9_]+)/)?.[1];assert(name);const state=plan(name);state.count++;
  const row={name,model:body.model,stream:body.stream===true,url:req.url,closed:false,receivedAt:Date.now(),writes:[]};state.rows.push(row);requests.push(row);state.received.resolve();res.once('close',()=>{row.closed=true;row.closedAt=Date.now();state.closed.resolve();});
  const json=(value='test the evidence.')=>{if(!res.destroyed){res.writeHead(200,{'Content-Type':'Application/JSON; charset=utf-8'});res.end(JSON.stringify({choices:[{message:{content:value},finish_reason:'stop'}]}));}};
  if(name.startsWith('http_')){res.writeHead(Number(name.slice(5)),{'Content-Type':'application/json'});res.end('{"error":{"message":"OWNED_PROVIDER_BODY_MUST_NOT_LEAK"}}');return;}
  if(name==='json_retry'&&body.stream){res.writeHead(400,{'Content-Type':'application/json'});res.end('{"error":"OWNED_PROVIDER_BODY_MUST_NOT_LEAK"}');return;}
  if(['instant_json','json_retry','json_no_task','fixed_model'].includes(name)){json();return;}
  if(name==='json_length'){res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify({choices:[{message:{content:'partial'},finish_reason:'length'}]}));return;}
  if(name==='auto_retry'&&body.model==='claude-haiku-owned'){res.writeHead(503);res.end();return;}
  const start=()=>{res.writeHead(200,{'Content-Type':'text/event-stream; charset=utf-8'});res.flushHeaders();};
  const frame=value=>{if(!res.destroyed)res.write('data: '+JSON.stringify(value)+'\r\n\r\n');};
  const delta=value=>{row.writes.push({atMs:Date.now()-row.receivedAt,characters:[...value].length});frame({choices:[req.url.endsWith('/completions')&&!req.url.endsWith('/chat/completions')?{text:value}:{delta:{content:value}}]});};
  const end=()=>{frame({choices:[{finish_reason:'stop',delta:{}}]});if(!res.destroyed)res.end('data: [DONE]\r\n\r\n');};
  if(name==='timeout_fast'&&state.count>1){json();return;}
  if(name==='held_headers'||name.startsWith('cancel_headers')||name==='timeout_fast'){state.pending.add(()=>{if(!res.destroyed){start();delta('test the evidence.');end();}});return;}
  start();
  if(name==='bad_utf8'){res.write(Buffer.concat([Buffer.from('data: {"choices":[{"delta":{"content":"bad'),Buffer.from([0xc0,0xaf]),Buffer.from('text"}}]}\n\n')]));res.end('data: [DONE]\n\n');return;}
  if(name==='transport_budget'){res.end(':'+('X'.repeat(70000))+'\n\n');return;}
  if(name==='raw_budget'){delta('X'.repeat(16001));end();return;}
  if(name==='provider_error'){delta('part');frame({error:{message:'OWNED_PROVIDER_BODY_MUST_NOT_LEAK'}});res.end();return;}
  if(name==='malformed'){delta('part');res.end('data: {broken}\n\n');return;}
  if(name==='clean_eof'){delta('part');res.end();return;}
  if(name==='delimiterless'){delta('part');res.end('data: [DONE]');return;}
  if(name==='length'){delta('part');frame({choices:[{finish_reason:'length',delta:{}}]});res.end();return;}
  if(name==='ready_unit'){delta('This complete writing unit contains enough detail to end here. More text');state.pending.add(end);return;}
  if(name==='held_partial'||name.startsWith('cancel_stream')||name.startsWith('disconnect')||name.startsWith('reused_old')){delta('test the ');state.pending.add(()=>{delta('evidence.');end();});return;}
  if(name==='finish_only'){delta('完整短片段');frame({choices:[{finish_reason:'stop',delta:{}}]});res.end();return;}
  delta('test the ');
  const complete=()=>{delta('evidence.');end();};
  if(name==='normal_held'){state.pending.add(complete);return;}
  const timer=setTimeout(()=>{timers.delete(timer);if(!res.destroyed)complete();},120);timers.add(timer);res.once('close',()=>{clearTimeout(timer);timers.delete(timer);});
 }catch(_){failures.push('fixture_protocol_rejected');res.destroy();}});
 server.on('connection',socket=>{sockets.add(socket);socket.once('close',()=>sockets.delete(socket));});
 await new Promise((resolve,reject)=>{server.once('error',reject);server.listen(0,'127.0.0.1',resolve);});
 return{base:'http://127.0.0.1:'+server.address().port,plan,requests,catalogRequests,failures,discovery(mode){assert(['normal','hold','bad_json'].includes(mode));catalogMode=mode;},
  async stop(){for(const timer of timers)clearTimeout(timer);for(const state of states.values())state.release();for(const socket of sockets)socket.destroy();await new Promise(resolve=>server.close(resolve));}};
}
module.exports={providerFixture,prefix,contextRevision,barrier};
