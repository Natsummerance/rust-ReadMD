'use strict';
// Actual authenticated kernel and owned keyless sockets. No GUI, model,
// credential store, dependency install, build or private configuration.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),http=require('node:http'),assert=require('node:assert/strict');
const {options}=require('./v006-table-clipboard-kernel.cjs'),{startKernel}=require('./v006-refactor-host.cjs');
const {providerFixture,prefix,contextRevision}=require('./ai-completion-stream-fixture.cjs');
const ROOT=path.resolve(__dirname,'../..'),pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const sha=value=>crypto.createHash('sha256').update(value).digest('hex');
const sources=['rust/readmd-kernel/src/server.rs','rust/readmd-kernel/src/ai.rs','rust/readmd-kernel/src/ai_completion.rs','rust/readmd-kernel/src/review_transport.rs','rust/readmd-kernel/src/review_transport/network.rs','rust/readmd-kernel/src/server/chat_stream.rs'];
const binding=()=>Object.fromEntries(sources.map(file=>[file,sha(fs.readFileSync(path.join(ROOT,file)))]));
async function until(check,label,ms=2000){const end=Date.now()+ms;while(Date.now()<end){if(await check())return;await pause(10);}throw Error(label);}
async function bounded(promise,label,ms=5000){let timer;try{return await Promise.race([promise,new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error(label)),ms);})]);}finally{clearTimeout(timer);}}
async function collect(response,onEvent){
 if(!response.headers.get('content-type')?.includes('text/event-stream'))return{status:response.status,json:await response.json(),events:[]};
 const reader=response.body.getReader(),decoder=new TextDecoder('utf-8',{fatal:true}),events=[];let pending='',wire='',bytes=0;
 try{for(;;){const value=await reader.read();if(value.done){pending+=decoder.decode();break;}bytes+=value.value.length;assert(bytes<=1048576);const text=decoder.decode(value.value,{stream:true});wire+=text;pending+=text;
  for(;;){const boundary=pending.match(/\r?\n\r?\n/);if(!boundary)break;const data=pending.slice(0,boundary.index).split(/\r?\n/).filter(line=>line.startsWith('data:')).map(line=>line.slice(5).trimStart()).join('\n');pending=pending.slice(boundary.index+boundary[0].length);if(data){const event=JSON.parse(data);events.push(event);onEvent?.(event);}}
 }assert.equal(pending.trim(),'','downstream delimiter required');return{status:response.status,events,wire};}
 finally{await reader.cancel().catch(()=>{});reader.releaseLock();}
}
async function main(){
 const args=options(process.argv.slice(2));assert(args.bin&&!args.base);const out=args.evidence;
 const dirs=Object.fromEntries(['profile','documents','tmp','home'].map(name=>[name,path.join(out,name)]));for(const dir of Object.values(dirs))fs.mkdirSync(dir,{mode:0o700});
 Object.assign(process.env,{TMPDIR:dirs.tmp,TMP:dirs.tmp,TEMP:dirs.tmp,CFFIXED_USER_HOME:dirs.home,READMD_NATIVE_QA:'',READMD_PET_NATIVE_QA:'',READMD_EXTRA_ROOTS:'',READMD_ASSETS_DIR:''});
 const original=Buffer.from('\ufeff# Completion 原件😀\r\n\r\nUnaccepted ghosts must not be saved.\r\n'),file=path.join(dirs.documents,'补全 原件😀.md');fs.writeFileSync(file,original,{flag:'wx'});
 const report={passed:false,syntheticOnly:true,modelsRequested:false,guiStarted:false,productionAuth:false,binarySha256:sha(fs.readFileSync(args.bin)),driverSha256:sha(fs.readFileSync(__filename)),fixtureSha256:sha(fs.readFileSync(require.resolve('./ai-completion-stream-fixture.cjs'))),before:binding(),cases:[],kernelPids:[]};
 let host,provider,rawBase,rawToken;
 const test=async(name,run)=>{const row={name,passed:false};report.cases.push(row);try{await run(row);row.passed=true;}catch(error){row.failure={name:error.name,message:String(error.message).replaceAll(out,'<owned>').replaceAll(ROOT,'<repo>').slice(0,700)};}};
 try{
  provider=await providerFixture();host=await startKernel(args.bin,dirs.profile,dirs.documents);report.kernelPids.push(host.pid);
  const post=(route,body,sse=false)=>host.api(route,{method:'POST',headers:{'Content-Type':'application/json',...(sse?{Accept:'text/event-stream'}:{})},body:JSON.stringify(body)});
  const api=async(route,body)=>{const r=await post(route,body);assert.equal(r.status,200);return r.json();};
  const configure=async(extra={},currentModel='owned-fixed')=>{const record={id:'custom:owned-inline',name:'Owned inline protocol',category:'local',format:'openai',mode:'chat',base_url:provider.base+'/v1',models:['owned-fixed'],autocomplete:{mode:'current'},...extra};const r=await api('/api/ai/config',{providers:[record],current:{provider_id:record.id,model:currentModel}});assert(r.custom.some(p=>p.id===record.id));return r;};
  const request=(name,id,extra={})=>({prefix:prefix(name),suffix:'',task_id:id,context_revision:contextRevision(prefix(name)),...extra});
  const completion=async(name,id,extra={},sse=true)=>collect(await post('/api/ai/complete',request(name,id,extra),sse));
  const identity=(value,id,name)=>{for(const event of value.events){assert.equal(event.task_id,id);assert.equal(event.context_revision,contextRevision(prefix(name)));}assert(!value.wire?.includes('OWNED_PROVIDER_BODY_MUST_NOT_LEAK'));assert(!value.wire?.includes('OWNED_CATALOG_BODY_MUST_NOT_LEAK'));};
  const done=value=>value.events.find(event=>event.type==='done')||value.json;
  const success=(value,id,name,reason)=>{assert.equal(value.status,200);identity(value,id,name);const result=done(value);assert.equal(result.ok,true);assert.equal(result.finish_reason,reason);assert.equal(result.task_id,id);assert.equal(result.context_revision,contextRevision(prefix(name)));return result;};
  const unsuccessful=value=>{assert(!value.events.some(e=>e.type==='done'&&e.ok===true));assert(value.json?.ok===false||value.events.some(e=>e.type==='error'));};
  await configure();
  await test('formal CSP token and trailing slash authorization',async row=>{const root=await host.api('/'),html=await root.text();assert(html.includes("script-src 'self'"));assert(!/script-src[^;]*'unsafe-(inline|eval)'/.test(html));rawBase=new URL(root.url).origin;rawToken=html.match(/name="readmd-app-token"\s+content="([^"]+)"/)?.[1];assert(rawToken);
   const anonymous=await fetch(new URL('/api/ai/complete/',root.url),{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(request('anonymous','owned-anonymous'))});assert([401,403].includes(anonymous.status));row.anonymousStatus=anonymous.status;report.productionAuth=true;
   const r=await post('/api/ai/complete/',request('trailing_slash','owned-trailing'),true);success(await collect(r),'owned-trailing','trailing_slash','provider_finished');});
  await test('progressive real snapshots, terminal and cache identity',async row=>{let firstAt,wasPartial=false;const started=performance.now(),value=await collect(await post('/api/ai/complete',request('normal','owned-normal'),true),event=>{if(event.type==='suggestion'&&event.completion==='test the '){firstAt=performance.now()-started;wasPartial=true;}});
   const full=success(value,'owned-normal','normal','provider_finished');assert.equal(full.completion,'test the evidence.');assert(wasPartial);const cached=success(await completion('normal','owned-normal-cached'),'owned-normal-cached','normal','cached');assert.equal(cached.cached,true);assert.equal(provider.plan('normal').count,1);row.firstSnapshotMs=firstAt;row.upstreamRequests=1;});
  await test('ready unit stops the owned open provider early',async row=>{const at=performance.now(),result=success(await completion('ready_unit','owned-ready'),'owned-ready','ready_unit','ready_unit');assert(result.completion.endsWith('.'));assert(performance.now()-at<2000);await until(()=>provider.plan('ready_unit').rows.every(r=>r.closed),'ready_owned_socket_not_closed');row.elapsedMs=performance.now()-at;});
  await test('explicit normal reason permits EOF without DONE token',async()=>{success(await completion('finish_only','owned-finish-only'),'owned-finish-only','finish_only','provider_finished');});
  for(const name of ['clean_eof','delimiterless','bad_utf8','malformed','provider_error','length','raw_budget','transport_budget','json_length'])await test(name+' fails without a completion cache entry',async row=>{
   const before=provider.plan(name).count;for(let i=0;i<2;i++){const id='owned-'+name+'-'+i,value=await completion(name,id);row.last={status:value.status,events:value.events,json:value.json};unsuccessful(value);identity(value,id,name);}assert.equal(provider.plan(name).count-before,2);row.repeatedUpstreamRequests=2;});
  await test('same-model JSON compatibility retry is bounded and cached once',async()=>{const result=success(await completion('json_retry','owned-json-retry'),'owned-json-retry','json_retry','provider_finished');assert.equal(result.model,'owned-fixed');assert.equal(provider.plan('json_retry').count,2);assert(provider.plan('json_retry').rows.every(r=>r.model==='owned-fixed'));success(await completion('json_retry','owned-json-cached'),'owned-json-cached','json_retry','cached');assert.equal(provider.plan('json_retry').count,2);});
  for(const status of [403,429])await test('HTTP '+status+' preserves safe status without body',async()=>{const name='http_'+status,value=await completion(name,'owned-http-'+status);unsuccessful(value);identity(value,'owned-http-'+status,name);assert.equal(provider.plan(name).count,1);const failure=value.events.find(e=>e.type==='error')||value.json;assert.equal(failure.error_code,'ai_http_error');assert.equal(Number(failure.status),status);});
  await test('legacy JSON without task and independent fixed model remain compatible',async()=>{const noTask=request('json_no_task',undefined);delete noTask.task_id;const r=await post('/api/ai/complete',noTask);const value=await r.json();assert.equal(value.ok,true);assert.equal(value.context_revision,contextRevision(prefix('json_no_task')));assert.equal(provider.plan('json_no_task').count,1);
   await configure({autocomplete:{mode:'model',model:'owned-independent'}});const fixed=success(await completion('fixed_model','owned-fixed-model'),'owned-fixed-model','fixed_model','provider_finished');assert.equal(fixed.model,'owned-independent');assert.equal((await(await host.api('/api/ai/config')).json()).current.model,'owned-fixed');await configure();});
  await test('JSON completion preserves the same authenticated keep-alive connection',async row=>{
   const agent=new http.Agent({keepAlive:true,maxSockets:1});
   const call=(route,body)=>new Promise((resolve,reject)=>{const bytes=body?Buffer.from(JSON.stringify(body)):null;
    const req=http.request(new URL(route,rawBase),{agent,method:bytes?'POST':'GET',headers:{'X-ReadMD-App-Token':rawToken,'X-ReadMD-Token':rawToken,...(bytes?{'Content-Type':'application/json','Content-Length':bytes.length}:{})}},res=>{const port=res.socket.localPort;let text='';res.setEncoding('utf8');res.on('data',chunk=>{text+=chunk;});res.on('end',()=>{try{resolve({status:res.statusCode,port,reused:req.reusedSocket,value:JSON.parse(text)});}catch(error){reject(error);}});});req.on('error',reject);req.setTimeout(5000,()=>req.destroy(Error('keepalive_response_timeout')));req.end(bytes);});
   try{const first=await call('/api/ai/complete',request('json_keepalive','owned-json-keepalive'));assert.equal(first.status,200);assert.equal(first.value.ok,true);await pause(120);const next=await call('/api/modules');assert.equal(next.status,200);assert.equal(next.port,first.port);assert.equal(next.reused,true);row.sameConnectionReused=true;}
   finally{agent.destroy();}
  });
  await test('task and supplied context identity reject before any provider call',async()=>{const badHash=await completion('bad_hash','owned-bad-hash',{context_revision:'0'.repeat(64)});assert.equal(badHash.status,400);const badTask=await completion('bad_task','../outside');assert.equal(badTask.status,400);assert.equal(provider.plan('bad_hash').count,0);assert.equal(provider.plan('bad_task').count,0);});
  for(const kind of ['headers','stream'])await test('two cancelled '+kind+' requests retire sockets and both permits',async row=>{
   const names=['cancel_'+kind+'_a','cancel_'+kind+'_b'],jobs=[];const cancellationAt=performance.now();
   try{
    for(let i=0;i<2;i++){const promise=post('/api/ai/complete',request(names[i],'owned-cancel-'+kind+'-'+i),true).then(collect).catch(error=>({clientFailure:error.name}));jobs.push(promise);}
    await bounded(Promise.all(names.map(name=>provider.plan(name).received.promise)),'two_upstreams_not_received');row.acknowledgements=[];
    for(let i=0;i<2;i++){const at=performance.now(),receipt=await api('/api/task/cancel',{id:'owned-cancel-'+kind+'-'+i});const elapsedMs=performance.now()-at;row.acknowledgements.push({state:receipt.state,elapsedMs});assert(elapsedMs<200);assert.equal(receipt.state,'cancelling');}
    const at=performance.now();let closed=true;try{await until(()=>names.every(name=>provider.plan(name).rows.every(r=>r.closed)),'cancelled_owned_upstreams_not_closed');}catch(_){closed=false;}
    row.ownedStopMs=performance.now()-at;row.closedBeforeFixtureRelease=closed;
    const fresh=await completion('after_cancel_'+kind,'owned-after-cancel-'+kind);row.replacementStatus=fresh.status;assert(closed);assert.notEqual(fresh.status,429);success(fresh,'owned-after-cancel-'+kind,'after_cancel_'+kind,'provider_finished');
    const results=await bounded(Promise.all(jobs),'cancelled_downstreams_not_terminal');for(let i=0;i<results.length;i++){unsuccessful(results[i]);identity(results[i],'owned-cancel-'+kind+'-'+i,names[i]);}
   }finally{for(const name of names)provider.plan(name).release();await bounded(Promise.all(jobs),'owned_fixture_release_timeout').catch(()=>{});row.totalMs=performance.now()-cancellationAt;}
  });
  await test('downstream close cancels only its owned request',async()=>{
   const kept=await post('/api/ai/complete',request('cancel_stream_isolation','owned-isolation'),true),keptRead=collect(kept).catch(error=>({clientFailure:error.name}));
   try{const dropped=await post('/api/ai/complete',request('disconnect_only','owned-disconnect'),true);await dropped.body.cancel();await until(()=>provider.plan('disconnect_only').rows.every(r=>r.closed),'disconnected_owned_upstream_not_closed');assert(provider.plan('cancel_stream_isolation').rows.every(r=>!r.closed));}
   finally{await api('/api/task/cancel',{id:'owned-isolation'});provider.plan('cancel_stream_isolation').release();await bounded(keptRead,'isolation_cleanup_timeout');}
  });
  await test('JSON compatibility downstream close also retires its owned network',async()=>{
   const controller=new AbortController(),pending=fetch(rawBase+'/api/ai/complete',{method:'POST',headers:{'Content-Type':'application/json','X-ReadMD-App-Token':rawToken,'X-ReadMD-Token':rawToken},body:JSON.stringify(request('disconnect_json','owned-json-disconnect')),signal:controller.signal}).then(collect).catch(error=>({clientFailure:error.name}));
   try{await bounded(provider.plan('disconnect_json').received.promise,'json_upstream_not_received');controller.abort();await until(()=>provider.plan('disconnect_json').rows.every(row=>row.closed),'json_disconnected_owned_upstream_not_closed');const value=await bounded(pending,'json_abort_not_observed');assert.equal(value.clientFailure,'AbortError');}
   finally{controller.abort();provider.plan('disconnect_json').release();await bounded(pending,'json_disconnect_cleanup').catch(()=>{});}
  });
  await test('reused task id cancels the old context without stealing the replacement',async()=>{
   const old=await post('/api/ai/complete',request('reused_old','owned-reused'),true),oldRead=collect(old).catch(error=>({clientFailure:error.name}));
   try{const replacement=await completion('reused_new','owned-reused');success(replacement,'owned-reused','reused_new','provider_finished');
    await until(()=>provider.plan('reused_old').rows.every(r=>r.closed),'replaced_old_owned_upstream_not_closed');const oldResult=await bounded(oldRead,'old_reused_request_not_terminal');unsuccessful(oldResult);identity(oldResult,'owned-reused','reused_old');}
   finally{provider.plan('reused_old').release();await bounded(oldRead,'reused_cleanup_timeout').catch(()=>{});}});
  await test('cancelled catalog GET closes before a late result can dispatch inference',async row=>{
   provider.discovery('hold');await configure({id:'custom:owned-discovery-hold',models:[],autocomplete:{mode:'auto'}});const pending=post('/api/ai/complete',request('catalog_cancel','owned-catalog-cancel'),true).then(collect).catch(error=>({clientFailure:error.name}));
   try{await bounded(provider.plan('catalog').received.promise,'catalog_not_received');const count=provider.requests.length,at=performance.now();const cancelled=await api('/api/task/cancel',{id:'owned-catalog-cancel'});row.cancelMs=performance.now()-at;assert(row.cancelMs<200);assert.equal(cancelled.state,'cancelling');await until(()=>provider.catalogRequests.every(r=>r.closed),'catalog_owned_socket_not_closed');provider.plan('catalog').release();await bounded(pending,'cancelled_catalog_completion_not_terminal');await pause(100);assert.equal(provider.requests.length,count);assert.equal(provider.plan('catalog_cancel').count,0);row.lateResultDidNotStartCompletion=true;}
   finally{provider.plan('catalog').release();await bounded(pending,'catalog_cleanup').catch(()=>{});provider.discovery('normal');}
  });
  await test('bad catalog JSON falls back to saved current model without leaking its body',async()=>{provider.discovery('bad_json');await configure({id:'custom:owned-discovery-bad',models:[],autocomplete:{mode:'auto'}});const result=success(await completion('catalog_bad_json','owned-catalog-bad'),'owned-catalog-bad','catalog_bad_json','provider_finished');assert.equal(result.model,'owned-fixed');provider.discovery('normal');});
  await test('automatic catalog model stays separate from saved chat model',async()=>{await configure({id:'custom:owned-discovery-good',models:[],autocomplete:{mode:'auto'}});const result=success(await completion('catalog_good','owned-catalog-good'),'owned-catalog-good','catalog_good','provider_finished');assert.equal(result.model,'claude-haiku-owned');assert.equal((await(await host.api('/api/ai/config')).json()).current.model,'owned-fixed');});
  await test('FIM request keeps its real completion protocol',async()=>{await configure({mode:'completion'});const result=success(await completion('fim','owned-fim',{protocol:'qwen'}),'owned-fim','fim','provider_finished');assert.equal(result.completion,'test the evidence.');assert.equal(provider.plan('fim').rows[0].url,'/v1/completions');await configure();});
  await test('uncancelled fast attempt times out distinctly and cannot populate the cache',async row=>{
   // This is only a synthetic catalogue name filtered by the text policy;
   // no image/media model or engine is requested by this fixture.
   await configure({id:'custom:owned-timeout',models:['claude-haiku-owned'],autocomplete:{mode:'auto'}},'image-fast');
   try{const at=performance.now(),value=await completion('timeout_fast','owned-fast-timeout');row.elapsedMs=performance.now()-at;unsuccessful(value);identity(value,'owned-fast-timeout','timeout_fast');const failure=value.events.find(event=>event.type==='error')||value.json;assert.equal(failure.error_code,'ai_completion_timeout');assert(row.elapsedMs>=7500&&row.elapsedMs<11000);await until(()=>provider.plan('timeout_fast').rows.every(item=>item.closed),'timed_out_owned_socket_not_closed');
    const retry=success(await completion('timeout_fast','owned-timeout-retry',{model:'claude-haiku-owned'}),'owned-timeout-retry','timeout_fast','provider_finished');assert.equal(retry.cached,false);assert.equal(provider.plan('timeout_fast').count,2);row.distinctFromCancellation=true;row.expiredAttemptWasNotCached=true;}
   finally{provider.plan('timeout_fast').release();await configure();}
  });
  await test('new process has no completion cache or persisted original changes',async row=>{await host.stop();host=null;host=await startKernel(args.bin,dirs.profile,dirs.documents);report.kernelPids.push(host.pid);assert.notEqual(report.kernelPids[0],host.pid);const result=success(await completion('normal','owned-new-process'),'owned-new-process','normal','provider_finished');assert.equal(result.cached,false);assert.deepEqual(fs.readFileSync(file),original);const history=await(await host.api('/api/ai/history')).json();assert.deepEqual(history.sessions,[]);const modules=await(await host.api('/api/modules')).json();assert.equal(modules.modules.ocr,'idle');assert.equal(modules.modules.transcribe,'idle');row.rawChatHistoryNotCreated=true;});
  assert.deepEqual(provider.failures,[]);report.providerRequests=provider.requests;report.catalogRequests=provider.catalogRequests;report.passed=report.cases.every(row=>row.passed);
 }catch(error){report.setupFailure={name:error.name,message:String(error.message).replaceAll(out,'<owned>').slice(0,700)};}
 finally{if(provider)await provider.stop();if(host)await host.stop();for(const name of ['tmp','home'])fs.rmSync(dirs[name],{recursive:true});report.scratchCleaned=true;report.after=binding();report.rustSourcesUnchanged=JSON.stringify(report.before)===JSON.stringify(report.after);report.binaryUnchanged=report.binarySha256===sha(fs.readFileSync(args.bin));fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx',mode:0o600});console.log(JSON.stringify({passed:report.passed,cases:report.cases.length,failed:report.cases.filter(row=>!row.passed),setupFailure:report.setupFailure,rustSourcesUnchanged:report.rustSourcesUnchanged,binaryUnchanged:report.binaryUnchanged}));if(!report.passed)process.exitCode=1;}
}
if(require.main===module)main().catch(()=>{console.error('Owned completion HTTP acceptance failed; inspect its preserved report.');process.exitCode=1;});
module.exports={main,collect};
