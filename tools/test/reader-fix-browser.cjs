'use strict';
// Explicit new bundled kernel; real CodeMirror, Rust Harness/ReviewStore/save.
// The only provider is an owned keyless native-tool protocol fixture. The
// Save As cancellation adapter never invokes an OS picker or file writer.
const fs=require('node:fs'),path=require('node:path'),http=require('node:http'),assert=require('node:assert/strict');
const {ROOT,sha,pause,until,openOwned}=require('./owned-browser-host.cjs'),{options}=require('./v006-table-clipboard-kernel.cjs'),{startKernel}=require('./v006-refactor-host.cjs');
function barrier(){let resolve;const value={settled:false,promise:new Promise(done=>{resolve=done;})};value.resolve=()=>{value.settled=true;resolve();};return value;}
async function providerFixture(){
 const plans=new Map(),requests=[],sockets=new Set(),failures=[];let calls=0;
 const plan=name=>{if(!plans.has(name))plans.set(name,{held:barrier(),release:barrier(),closed:barrier(),blocked:name==='stop'});return plans.get(name);};
 const server=http.createServer(async(req,res)=>{try{
  assert.equal(req.headers.authorization,undefined);assert.equal(req.headers.origin,undefined);
  if(req.method==='GET'){res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify({data:[{id:'owned-reader-model'}]}));return;}
  assert(req.url.endsWith('/chat/completions'));let raw='';for await(const chunk of req){raw+=chunk;assert(raw.length<1000000);}
  const body=JSON.parse(raw);assert(body.tools?.length,'Repair must use native tools, never legacy chat');
  const messages=body.messages||[],decoded=messages.map(message=>{try{return{...message,parsed:JSON.parse(message.content)};}catch{return message;}});
  const initial=decoded.find(message=>message.role==='user'&&message.parsed?.document_id)?.parsed;assert(initial?.document_id);
  const toolCalls=messages.filter(message=>message.role==='assistant').flatMap(message=>message.tool_calls||[]),last=toolCalls.at(-1)?.function.name;
  const snapshot=decoded.filter(message=>message.role==='tool'&&typeof message.parsed?.content==='string').at(-1)?.parsed;
  const mode=snapshot?.content.match(/READMD_FIX_([a-z]+)/)?.[1]||'initial';requests.push({mode,document_id:initial.document_id,last:last||null});
  let name,args;
  if(!last){name='read_document';args={document_id:initial.document_id};}
  else if(last==='read_document'&&!toolCalls.some(call=>call.function.name==='propose_review')){
   assert(snapshot);const from=snapshot.content.indexOf('Original passage.');assert(from>=0);
   name='propose_review';args={document_id:initial.document_id,generation:snapshot.generation,review_revision:snapshot.review_revision,edits:[{from,to:from+'Original passage.'.length,original:'Original passage.',insert:'Revised passage.'}]};
   const current=plan(mode);if(current.blocked){res.once('close',()=>current.closed.resolve());current.held.resolve();await current.release.promise;if(res.destroyed)return;}
  }else if(last==='propose_review'){name='read_document';args={document_id:initial.document_id};}
  else if(last==='read_document'){name='inspect_review';args={document_id:initial.document_id};}
  else if(last==='inspect_review'){
   const inspected=decoded.filter(message=>message.role==='tool'&&Array.isArray(message.parsed?.changes)).at(-1)?.parsed;assert(inspected);name='task_complete';args={document_id:initial.document_id,generation:inspected.generation,review_revision:inspected.review_revision};
  }else throw Error('Unexpected native tool phase');
  const id='owned-reader-call-'+(++calls),encoded=JSON.stringify(args),split=Math.floor(encoded.length/2);
  if(body.stream===false){res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify({choices:[{message:{role:'assistant',tool_calls:[{id,type:'function',function:{name,arguments:encoded}}]},finish_reason:'tool_calls'}]}));return;}
  res.writeHead(200,{'Content-Type':'text/event-stream'});const frame=delta=>res.write('data: '+JSON.stringify({choices:[delta]})+'\n\n');
  frame({index:0,delta:{role:'assistant',tool_calls:[{index:0,id,type:'function',function:{name,arguments:encoded.slice(0,split)}}]}});await pause(20);if(res.destroyed)return;
  frame({index:0,delta:{tool_calls:[{index:0,function:{arguments:encoded.slice(split)}}]}});frame({index:0,delta:{},finish_reason:'tool_calls'});res.end('data: [DONE]\n\n');
 }catch(error){failures.push(error.message.slice(0,200));if(!res.destroyed){res.writeHead(500,{'Content-Type':'application/json'});res.end('{"error":"owned_fixture_failure"}');}}});
 server.on('connection',socket=>{sockets.add(socket);socket.once('close',()=>sockets.delete(socket));});await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 return{base:'http://127.0.0.1:'+server.address().port+'/v1',requests,failures,plan,release(name){const value=plan(name);value.blocked=false;value.release.resolve();},async close(){for(const value of plans.values())value.release.resolve();for(const socket of sockets)socket.destroy();await new Promise(resolve=>server.close(resolve));}};
}
async function main(){
 const args=options(process.argv.slice(2));assert(args.bin&&!args.base);const output=args.evidence;
 const sourceBinding=()=>Object.fromEntries(['assets/js/reader/fixes.js','assets/app.js','ui-tests/reader-fix-safety.test.cjs'].map(file=>[file,sha(fs.readFileSync(path.join(ROOT,file)))]));
 const report={passed:false,driverSha256:sha(fs.readFileSync(__filename)),binarySha256:sha(fs.readFileSync(args.bin)),bootSha256:sha(fs.readFileSync(ROOT+'/assets/readmd.boot.js')),sourceBefore:sourceBinding(),productionCspAndToken:true,realBackend:true,actualHeadlessBrowser:true,nativePickerCertified:false,modelsRequested:false,saveAs:'controlled chooser cancellation only; ordinary Save uses actual Rust writer',checks:[]};
 const originals=new Map(),encode=text=>Buffer.from('\ufeff'+text.replaceAll('\n','\r\n'));let provider,test,restarted,restartScratch,phase='start',normalTask;
 try{
  provider=await providerFixture();
  test=await openOwned(output,async(api,paths)=>{
   await api('/api/ai/config',{providers:[{id:'custom:reader-fix-browser',name:'Owned reader fix fixture',category:'local',format:'openai',mode:'chat',base_url:provider.base,models:['owned-reader-model']}],current:{provider_id:'custom:reader-fix-browser',model:'owned-reader-model'}});
   for(const name of ['reading','editing','aba','stop']){const text='# 标题😀\n\nREADMD_FIX_'+name+'\n\nOriginal passage.\n';originals.set(name,encode(text));fs.writeFileSync(path.join(paths.documents,name+'.md'),encode(text));}
   originals.set('copy',encode('#Title\n\n![Owned](owned.png)\n'));fs.writeFileSync(path.join(paths.documents,'copy.md'),originals.get('copy'));fs.writeFileSync(path.join(paths.documents,'copy.readmd.md'),'OWNED EXISTING SIDECAR');
   fs.writeFileSync(path.join(paths.documents,'owned.png'),Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jA1cAAAAASUVORK5CYII=','base64'));
  },{binary:args.bin,playwright:args.playwright||require.resolve('playwright',{paths:[ROOT+'/ui-tests',path.resolve(ROOT,'../../rust-ReadMD/ui-tests')]})});
  const {page}=test;
  assert.equal(await page.evaluate(()=>typeof handleSaveDocumentFix==='function'&&!runAiDocumentFix.toString().includes('/api/ai/chat')),true,'A newly bundled reader-fix candidate is required');
  await page.evaluate(()=>{setEditorPref('aiCompletion',false);window.__fixChooserCalls=[];py.save_as=async(content,suggested,assets,opts)=>{window.__fixChooserCalls.push({content,suggested,assets,review_session:opts.review_session,blocked_paths:opts.blocked_paths});return null;};});
  async function enter(name,editing=false){await page.evaluate(async({file,editing})=>{await loadFile(file);await loadAiConfig();if(editing&&!state.editing)await toggleEdit();if(state.editing)setPvLayout('none');}, {file:path.join(test.documents,name+'.md'),editing});}
  async function start(){return page.evaluate(async()=>{showFixModal();const tab=getActiveTab(),before=await ReadMDReview.capture(tab);const task=await handleAiDocumentFix();return{taskId:task?.id,tabId:tab.id,documentId:before.document_id,path:tab.path};});}
  async function complete(id){await until(()=>page.evaluate(async id=>{await ReadMDAiHarness.status(id);return ReadMDAiHarness.list().find(task=>task.id===id)?.state==='completed';},id),'actual native task completion',15000);await page.evaluate(()=>ReadMDReview.flush(getActiveTab()));}
  for(const name of ['reading','editing']){
   phase=name+' real pending repair';await enter(name,name==='editing');const owner=await start();assert(owner.taskId);await complete(owner.taskId);
   const state=await page.evaluate(()=>({tab:getActiveTab().id,path:getActiveTab().path,editing:state.editing,text:cmView.state.doc.toString(),snapshot:ReadMDReview.snapshot(getActiveTab()),tabs:state.tabs.length}));
   assert.equal(state.tab,owner.tabId);assert.equal(state.path,owner.path);assert(state.editing);assert(state.text.includes('Revised passage.'));assert(state.snapshot.changes.some(change=>change.origin==='ai'&&change.decision==='pending'));assert(fs.readFileSync(owner.path).equals(originals.get(name)));
   assert.equal(await page.evaluate(()=>saveEdit()),true);const expected=encode(originals.get(name).toString('utf8').replace(/^\ufeff/,'').replaceAll('\r\n','\n').replace('Original passage.','Revised passage.'));assert(fs.readFileSync(owner.path).equals(expected));
   const before=await page.evaluate(()=>cmView.state.doc.toString());const restored=await page.evaluate(async()=>{ReadMDReview.history(false);await ReadMDReview.flush(getActiveTab());return cmView.state.doc.toString();});assert(restored.includes('Original passage.'));
   assert.equal(await page.evaluate(async()=>{ReadMDReview.history(true);await ReadMDReview.flush(getActiveTab());return cmView.state.doc.toString();}),before);
   assert.equal(await page.evaluate(()=>ReadMDReview.snapshot(getActiveTab()).changes.some(change=>change.decision==='pending')),true);
   if(name==='reading')normalTask=owner;
   report.checks.push(name+': exact original tab, real native-tool pending proposal, no write before explicit Save, physical BOM/CRLF save and durable undo/redo');
  }
  phase='ABA during preparation';await enter('aba',true);const count=provider.requests.length;
  await page.evaluate(()=>{window.__fixOriginalConfigure=ensureAiConfigured;window.__fixConfigureGate=new Promise(resolve=>{window.__fixConfigureRelease=resolve;});ensureAiConfigured=async()=>{window.__fixConfigureEntered=true;await window.__fixConfigureGate;return window.__fixOriginalConfigure();};showFixModal();window.__fixPreparing=handleAiDocumentFix();});
  await until(()=>page.evaluate(()=>Boolean(window.__fixConfigureEntered)),'configuration barrier');
  await page.evaluate(async()=>{const at=cmView.state.doc.length;cmView.dispatch({changes:{from:at,insert:'Temporary.'},userEvent:'input.type'});await ReadMDReview.flush(getActiveTab());ReadMDReview.history(false);await ReadMDReview.flush(getActiveTab());window.__fixConfigureRelease();await window.__fixPreparing;ensureAiConfigured=window.__fixOriginalConfigure;});
  assert.equal(provider.requests.length,count);assert(fs.readFileSync(path.join(test.documents,'aba.md')).equals(originals.get('aba')));report.checks.push('real editor edit plus durable undo returns equal text but its newer generation rejects repair before any provider request');
  phase='clean reading copy cancellation';await enter('copy');
  const copy=await page.evaluate(async()=>{const source=getActiveTab(),initial=await ReadMDReview.capture(source);showFixModal();const result=await handleSaveDocumentFix();const draft=getActiveTab();return{result,sourceId:source.id,sourcePath:source.path,sourceStillOpen:state.tabs.includes(source),newId:draft.id,newPath:draft.path,dirty:draft.isDirty,content:draft.content,originalDocument:initial.document_id,newDocument:ReadMDReview.snapshot(draft)?.document_id,chooser:window.__fixChooserCalls.at(-1)};});
  assert.equal(copy.result,false);assert(copy.sourceStillOpen);assert.notEqual(copy.newId,copy.sourceId);assert.equal(copy.newPath,null);assert(copy.dirty);assert.notEqual(copy.newDocument,copy.originalDocument);assert(copy.content.includes('# Title'));assert(copy.chooser.blocked_paths.includes(copy.sourcePath));assert(fs.readFileSync(copy.sourcePath).equals(originals.get('copy')));assert.equal(fs.readFileSync(path.join(test.documents,'copy.readmd.md'),'utf8'),'OWNED EXISTING SIDECAR');assert(!fs.existsSync(path.join(test.documents,'readmd-assets')));
  report.checks.push('explicit reading-copy request creates a separate Review draft; controlled chooser cancellation retains it dirty with original blocked, original/sidecar bytes unchanged and no resource staging');
  phase='pending provenance cannot become a clean copy';await page.evaluate(async id=>{await switchTab(id);if(state.editing)exitEdit();showFixModal();window.__fixTabCount=state.tabs.length;window.__fixChooserCount=window.__fixChooserCalls.length;window.__fixLimited=await handleSaveDocumentFix();},normalTask.tabId);
  assert.equal(await page.evaluate(()=>window.__fixLimited===false&&state.tabs.length===window.__fixTabCount&&window.__fixChooserCalls.length===window.__fixChooserCount),true);report.checks.push('a source with durable AI review metadata cannot be copied as a provenance-free reading projection');
  phase='owned pause';await enter('stop');const stopped=await start();await until(()=>provider.plan('stop').held.settled,'real provider held before proposal');const at=performance.now();await page.evaluate(id=>ReadMDAiHarness.pause(id),stopped.taskId);await until(()=>provider.plan('stop').closed.settled,'pause closes only owned fixture',2000);report.pauseMs=performance.now()-at;assert(fs.readFileSync(path.join(test.documents,'stop.md')).equals(originals.get('stop')));
  const modules=await test.api('/api/modules');assert.equal(modules.modules.ocr,'idle');assert.equal(modules.modules.transcribe,'idle');
  report.kernelPid=test.host.pid;await test.close();test=null;
  phase='new process paused recovery';restartScratch=path.join(output,'restart-scratch');fs.mkdirSync(restartScratch);Object.assign(process.env,{TMPDIR:restartScratch,TMP:restartScratch,TEMP:restartScratch,CFFIXED_USER_HOME:restartScratch});
  restarted=await startKernel(args.bin,path.join(output,'profile'),path.join(output,'documents'));report.reopenedPid=restarted.pid;assert.notEqual(report.reopenedPid,report.kernelPid);
  report.recoveryRequests=[];
  const recoveryRequest=async(route,body,expectedStatus=200)=>{
   const url=new URL(route,'http://owned.invalid'),file=body?.path||url.searchParams.get('p');
   const summary={method:body?'POST':'GET',route:url.pathname,...(body?.action?{action:body.action}:{}),...(file?{fileName:path.basename(file),fileHash:sha(file).slice(0,16)}:{}),...(body?.task_id?{taskHash:sha(body.task_id).slice(0,16)}:{})};
   const r=await restarted.api(route,body?{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)}:{}),data=await r.json();
   const code=typeof data.error_code==='string'&&/^[a-z][a-z0-9_]{0,95}$/.test(data.error_code)?data.error_code:null;
   report.recoveryRequests.push({...summary,status:r.status,...(code?{error_code:code}:{})});
   assert.equal(r.status,expectedStatus,`${summary.method} ${summary.route} ${summary.action||''} returned ${r.status}${code?' '+code:''}`);return data;
  };
  const api=body=>recoveryRequest('/api/ai/tasks',body);
  const requestCount=provider.requests.length,listed=await api({action:'list'});assert(listed.tasks.some(task=>task.id===stopped.taskId&&['paused','needs_user_resume'].includes(task.state)));await api({action:'status',task_id:stopped.taskId});await pause(200);assert.equal(provider.requests.length,requestCount);
  const denied=await recoveryRequest('/api/review',{action:'resume',path:normalTask.path},403);assert.equal(denied.error_code,'review_not_authorized');
  // A fresh process has no file-open authority. Re-enter through the same
  // physical file API as the reader before requesting durable source text.
  await recoveryRequest('/api/file?p='+encodeURIComponent(normalTask.path));
  const savedReview=await recoveryRequest('/api/review',{action:'resume',path:normalTask.path});assert(savedReview.found);assert(savedReview.snapshot.changes.some(change=>change.decision==='pending'));assert(savedReview.snapshot.content.includes('Revised passage.'));
  await recoveryRequest('/api/file?p='+encodeURIComponent(stopped.path));assert.equal(provider.requests.length,requestCount);
  provider.release('stop');await api({action:'resume',task_id:stopped.taskId,call_id:'owned-explicit-resume'});await until(async()=>{const value=await api({action:'status',task_id:stopped.taskId});return value.task.state==='completed';},'explicit resumed task completes',15000);assert(provider.requests.length>requestCount);
  report.checks.push('a different kernel process restores saved pending review and paused task; list/status never infer, and only explicit Resume starts the owned fixture again');
  assert.equal(provider.failures.length,0);report.providerRequests=provider.requests;report.passed=true;
 }catch(error){report.failure={phase,name:error.name,message:String(error.message).replaceAll(output,'<owned>').replaceAll(ROOT,'<repo>').slice(0,800)};if(test)await test.page.screenshot({path:path.join(output,'first-failure.png')}).catch(()=>{});process.exitCode=1;}
 finally{if(test)await test.close();if(restarted)await restarted.stop();if(restartScratch&&fs.existsSync(restartScratch))fs.rmSync(restartScratch,{recursive:true});if(provider)await provider.close();report.sourceAfter=sourceBinding();report.sourceUnchanged=JSON.stringify(report.sourceBefore)===JSON.stringify(report.sourceAfter);report.binaryUnchanged=sha(fs.readFileSync(args.bin))===report.binarySha256;fs.writeFileSync(path.join(output,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx',mode:0o600});console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,failure:report.failure,sourceUnchanged:report.sourceUnchanged,binaryUnchanged:report.binaryUnchanged}));}
}
if(require.main===module)main().catch(()=>{console.error('Owned reader fix browser setup failed');process.exitCode=1;});
module.exports={main,providerFixture};
