'use strict';
// Owned HTTP protocol fixture; no inference or credentials from the host.
const http=require('node:http'),assert=require('node:assert/strict');
function barrier(){let resolve;const promise=new Promise(done=>{resolve=done;});return{promise,resolve};}
async function providerFixture(){
 const plans=new Map(),requests=[],sockets=new Set();
 function plan(name){if(!plans.has(name))plans.set(name,{name,received:barrier(),closed:barrier(),release:barrier(),count:0});return plans.get(name);}
 const server=http.createServer(async(req,res)=>{
  try{
   let body='';for await(const chunk of req){body+=chunk;if(body.length>1048576)throw Error('Fixture request budget');}
   const payload=JSON.parse(body),text=(payload.messages||[]).filter(x=>x.role==='user').map(x=>String(x.content||'')).join('\n'),name=text.match(/READMD_CASE_([a-z0-9_-]+)/)?.[1];
   assert(name);assert.equal(req.url,'/v1/chat/completions');assert.equal(req.headers.authorization,'Bearer owned-non-secret-fixture');
   const state=plan(name);state.count++;requests.push({name,stream:payload.stream===true});res.once('close',()=>state.closed.resolve());state.received.resolve();
   if(name.startsWith('error_')){res.writeHead(Number(name.slice(6)),{'Content-Type':'application/json'});res.end(JSON.stringify({error:{message:'DO_NOT_ECHO_FIXTURE_SECRET'}}));return;}
   if(['json','legacy','private','after_cancel'].includes(name)){res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify({choices:[{message:{content:'JSON reply'},finish_reason:'stop'}],usage:{prompt_tokens:2,completion_tokens:3,total_tokens:5}}));return;}
   if(name==='cancel_headers')return;
   res.writeHead(200,{'Content-Type':'text/event-stream; charset=utf-8','Cache-Control':'no-store'});res.flushHeaders();
   const frame=value=>res.write('data: '+JSON.stringify(value)+'\n\n'),delta=text=>frame({choices:[{delta:{content:text}}]});
   if(name==='bad_utf8'){res.end(Buffer.from([0xc0,0xaf]));return;}
   if(name==='malformed'){res.end('data: {broken}\n\n');return;}
   delta(name==='normal'?'A中':'partial');
   if(['cancel_stream','disconnect'].includes(name))return;
   if(name==='truncated'){res.end();return;}
   if(name==='length'){frame({choices:[{finish_reason:'length',delta:{}}]});res.end();return;}
   assert.equal(name,'normal');await state.release.promise;if(res.destroyed)return;
   delta('文😀 B');frame({choices:[],usage:{prompt_tokens:1,completion_tokens:2,total_tokens:3}});
   frame({choices:[],usage:{prompt_tokens:1,completion_tokens:3,total_tokens:4}});
   frame({choices:[{finish_reason:'stop',delta:{}}]});res.end('data: [DONE]\n\n');
  }catch{if(!res.headersSent)res.writeHead(500);res.end();}
 });
 server.on('connection',socket=>{sockets.add(socket);socket.once('close',()=>sockets.delete(socket));});
 await new Promise((resolve,reject)=>{server.once('error',reject);server.listen(0,'127.0.0.1',resolve);});
 return{base:'http://127.0.0.1:'+server.address().port,plan,requests,async stop(){for(const state of plans.values())state.release.resolve();for(const socket of sockets)socket.destroy();await new Promise(resolve=>server.close(resolve));}};
}
module.exports={providerFixture,barrier};
