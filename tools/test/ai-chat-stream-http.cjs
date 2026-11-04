'use strict';
// Production authentication/CSP and actual local sockets, storage and reopen.
// Protocol fixtures are explicit; this does not certify a model or native UI.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),vm=require('node:vm'),assert=require('node:assert/strict');
const {startKernel}=require('./v006-refactor-host.cjs'),{options}=require('./v006-table-clipboard-kernel.cjs'),{providerFixture,barrier}=require('./ai-chat-provider-fixture.cjs');
const ROOT=path.resolve(__dirname,'../..'),sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const window={};vm.runInNewContext(fs.readFileSync(path.join(ROOT,'assets/js/features/ai-chat-stream.js'),'utf8'),{window,TextDecoder,DOMException});const {consume}=window.ReadMDChatStream;
async function bounded(promise,label,ms=5000){let timer;try{return await Promise.race([promise,new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('Deadline: '+label)),ms);})]);}finally{clearTimeout(timer);}}
async function main(){
 const args=options(process.argv.slice(2));assert(args.bin&&!args.base);
 const profile=path.join(args.evidence,'profile'),docs=path.join(args.evidence,'documents'),tmp=path.join(args.evidence,'tmp');for(const directory of [profile,docs,tmp])fs.mkdirSync(directory);
 Object.assign(process.env,{TMPDIR:tmp,TMP:tmp,TEMP:tmp});const file=path.join(docs,'聊天 原件😀.md'),original=Buffer.from('\ufeff# Keep original\r\n\r\nBody.\r\n');fs.writeFileSync(file,original);
 const report={passed:false,modelsRequested:false,protocolFixture:true,nativeUiCertified:false,strictAuth:true,productionCsp:true,binarySha256:sha(fs.readFileSync(args.bin)),bootSha256:sha(fs.readFileSync(path.join(ROOT,'assets/readmd.boot.js'))),checks:[],processes:[]};
 let host,provider,base,token,phase='start';const histories=[];
 async function start(){host=await startKernel(args.bin,profile,docs);report.processes.push(host.pid);const root=await host.api('/'),html=await root.text();base=root.url;token=html.match(/name="readmd-app-token"\s+content="([^"]+)"/)?.[1];assert(token);const csp=html.match(/<meta http-equiv="Content-Security-Policy" content="([^"]+)"/)?.[1]||'';assert(csp.includes("script-src 'self'"));assert(!/script-src[^;]*'unsafe-(eval|inline)'/.test(csp));}
 const api=async(route,body)=>{const r=await host.api(route,body===undefined?{}:{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});assert.equal(r.status,200);return r.json();};
 const history=async session=>(await api('/api/ai/history?session='+encodeURIComponent(session))).messages;
 function body(name,extra={}){return{base_url:provider.base+'/v1',api_key:'owned-non-secret-fixture',model:'owned-fixture',mode:'chat',stream:true,session:'owned-'+name,task_id:crypto.randomUUID(),messages:[{role:'user',content:'READMD_CASE_'+name}],...extra};}
 function post(value,headers={}){return fetch(new URL('/api/ai/chat',base),{method:'POST',headers:{'Content-Type':'application/json',Accept:'text/event-stream','X-ReadMD-App-Token':token,'X-ReadMD-Token':token,...headers},body:JSON.stringify(value),signal:AbortSignal.timeout(15000)});}
 async function answer(name,extra={}){const value=body(name,extra),r=await post(value);assert.equal(r.status,200);const deltas=[],usages=[];await consume(r,{taskId:value.task_id,onDelta:delta=>deltas.push(delta),onUsage:usage=>usages.push(usage)});return{value,text:deltas.join(''),deltas,usages};}
 try{
  provider=await providerFixture();await start();
  phase='auth and shape';for(const alias of ['','/','//']){const r=await fetch(new URL('/api/ai/chat'+alias,base),{method:'POST',headers:{'Content-Type':'application/json',Accept:'text/event-stream'},body:JSON.stringify(body('normal'))});assert([401,403].includes(r.status));}assert.equal(provider.requests.length,0);
  const invalid=await post([]);assert.equal(invalid.status,400);assert((invalid.headers.get('content-type')||'').includes('application/json'));assert.equal(provider.requests.length,0);report.checks.push('production CSP and real token; every trailing-slash alias rejects anonymous access before inference; malformed shape makes no provider request');
  phase='real delayed stream';const value=body('normal'),r=await post(value),first=barrier(),deltas=[],usages=[];let complete=false;
  const reading=consume(r,{taskId:value.task_id,onDelta:delta=>{deltas.push(delta);first.resolve();},onUsage:usage=>usages.push(usage)}).then(()=>{complete=true;});
  await bounded(first.promise,'first actual delta');assert.equal(deltas.join(''),'A中');assert.equal(complete,false);assert.equal((await history(value.session)).length,0);provider.plan('normal').release.resolve();await reading;
  assert.equal(deltas.join(''),'A中文😀 B');assert.equal(usages.length,1);assert.equal(usages[0].total_tokens,4);assert.equal(provider.plan('normal').count,1);
  const normal=await history(value.session);assert.deepEqual(normal.map(x=>x.role),['user','assistant']);assert.equal(normal[1].content,'A中文😀 B');histories.push([value.session,'A中文😀 B']);
  report.checks.push('a genuine upstream delta arrives before the fixture is allowed to send the rest; no early history commit, one provider request, exact Unicode and one final usage');
  phase='one-request JSON fallback';const json=await answer('json',{stream:false});assert.equal(json.text,'JSON reply');assert.equal(json.deltas.length,1);assert.equal(provider.plan('json').count,1);assert.equal(json.usages[0].total_tokens,5);
  const legacyBody=body('legacy',{stream:false}),legacy=await post(legacyBody,{Accept:'application/json'});assert((legacy.headers.get('content-type')||'').includes('application/json'));assert.equal((await legacy.json()).content,'JSON reply');assert.equal(provider.plan('legacy').count,1);
  report.checks.push('nonstream providers emit one genuine delta without a repeated request; the original JSON endpoint remains compatible');
  for(const name of ['truncated','malformed','bad_utf8','length']){
   phase=name;const value=body(name),r=await post(value),partial=[],usages=[];await assert.rejects(consume(r,{taskId:value.task_id,onDelta:delta=>partial.push(delta),onUsage:usage=>usages.push(usage)}),error=>error.code===(name==='length'?'review_ai_budget':'ai_invalid_response'));assert.equal((await history(value.session)).length,0);assert.equal(usages.length,0);if(['truncated','length'].includes(name))assert.equal(partial.join(''),'partial');
  }
  report.checks.push('bare upstream EOF, malformed frames, invalid UTF-8 and token exhaustion retain partial text without completion, usage or history writes');
  for(const status of [400,403,429]){
   phase='HTTP '+status;const value=body('error_'+status),r=await post(value),text=r.clone().text();await assert.rejects(consume(r,{taskId:value.task_id}),error=>error.code==='ai_http_error'&&error.status===status);const wire=await text;assert(!wire.includes('DO_NOT_ECHO_FIXTURE_SECRET'));assert(!wire.includes('owned-non-secret-fixture'));assert(!wire.includes(provider.base));assert.equal((await history(value.session)).length,0);
  }
  report.checks.push('HTTP 400/403/429 remain distinguishable without echoing provider error bodies, connection or authorization; no successful history');
  report.cancellations=[];
  for(const name of ['cancel_headers','cancel_stream']){
   phase=name;const value=body(name),r=await post(value),partial=[],reading=consume(r,{taskId:value.task_id,onDelta:delta=>partial.push(delta)});const rejected=assert.rejects(reading,error=>error.name==='AbortError');
   await bounded(provider.plan(name).received.promise,'actual provider request');if(name==='cancel_stream')for(const end=Date.now()+3000;!partial.length;){assert(Date.now()<end);await new Promise(resolve=>setTimeout(resolve,5));}
   const at=performance.now(),receipt=await api('/api/task/cancel',{id:value.task_id}),responseMs=performance.now()-at;assert.equal(receipt.state,'cancelling');assert(responseMs<200);await rejected;await bounded(provider.plan(name).closed.promise,'owned upstream close',2000);const stopMs=performance.now()-at;assert(stopMs<2000);assert.equal((await history(value.session)).length,0);assert.equal((await api('/api/task/cancel',{id:value.task_id})).state,'unknown');report.cancellations.push({phase:name,responseMs,ownedUpstreamCloseMs:stopMs});
   assert.equal((await answer('after_cancel',{stream:false,session:'owned-next-'+name})).text,'JSON reply');
  }
  report.checks.push('real cancellation before upstream headers and mid-stream responds below 200ms, closes the owned connection below two seconds, preserves history and permits the next request');
  phase='downstream disconnect';const disconnected=body('disconnect'),response=await post(disconnected),control=new AbortController();const lost=consume(response,{taskId:disconnected.task_id,signal:control.signal,onDelta:()=>control.abort()});await assert.rejects(lost,error=>error.name==='AbortError');await bounded(provider.plan('disconnect').closed.promise,'peer disconnect closes upstream',2000);assert.equal((await history(disconnected.session)).length,0);
  report.checks.push('closing the downstream reader without calling the cancellation route stops the owned upstream request and does not persist a partial answer');
  phase='incognito';const privateReply=await answer('private',{stream:false,incognito:true});assert.equal(privateReply.text,'JSON reply');assert.equal((await history(privateReply.value.session)).length,0);assert.deepEqual(fs.readFileSync(file),original);
  const privateJson=body('private',{stream:false,incognito:true,session:'owned-private-json'}),privateJsonResponse=await post(privateJson,{Accept:'application/json'});assert((privateJsonResponse.headers.get('content-type')||'').includes('application/json'));assert.equal((await privateJsonResponse.json()).content,'JSON reply');assert.equal((await history(privateJson.session)).length,0);
  report.checks.push('incognito replies in both SSE and JSON are usable without raw chat-history persistence; Markdown bytes remain unchanged');
  phase='new process';await host.stop();host=null;await start();for(const [session,text]of histories){const rows=await history(session);assert.equal(rows.length,2);assert.equal(rows[1].content,text);}for(const name of ['truncated','cancel_headers','cancel_stream','disconnect','private'])assert.equal((await history('owned-'+name)).length,0);assert.deepEqual(fs.readFileSync(file),original);
  report.checks.push('a second real process reopens the committed full turn only; failed, cancelled and incognito sessions and BOM/CRLF original remain intact');report.providerRequests=provider.requests.length;report.passed=true;
 }catch(error){report.failure={phase,name:error.name,code:/^[a-z0-9_]+$/.test(error.code||'')?error.code:null,message:String(error.message).replaceAll(args.evidence,'<evidence>').replaceAll(ROOT,'<repo>').replaceAll(token||'UNSET_TOKEN','<token>').slice(0,500)};process.exitCode=1;}
 finally{if(host)await host.stop();if(provider)await provider.stop();fs.writeFileSync(path.join(args.evidence,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify(report));}
}
if(require.main===module)main().catch(()=>{console.error('Owned chat stream acceptance failed; inspect its report');process.exitCode=1;});
module.exports={main};
