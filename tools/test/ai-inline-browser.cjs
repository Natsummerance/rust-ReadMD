'use strict';
// Production CSP/auth, real CodeMirror/ReviewStore/writer, owned headless profile.
// The loopback provider is a keyless protocol fixture and performs no inference.
const fs=require('node:fs'),path=require('node:path'),http=require('node:http'),assert=require('node:assert/strict');
const {ROOT,sha,pause,until,openOwned}=require('./owned-browser-host.cjs');
const {options}=require('./v006-table-clipboard-kernel.cjs');
const {startKernel}=require('./v006-refactor-host.cjs');
function barrier(){let resolve;const value={settled:false,promise:new Promise(done=>{resolve=done;})};value.resolve=()=>{value.settled=true;resolve();};return value;}
async function providerFixture(){
 const plans=new Map(),sockets=new Set(),requests=[];
 const plan=name=>{if(!plans.has(name))plans.set(name,{count:0,received:barrier(),closed:barrier(),release:barrier()});return plans.get(name);};
 const server=http.createServer(async(req,res)=>{try{
  assert.equal(req.headers.authorization,undefined);let raw='';for await(const chunk of req){raw+=chunk;if(raw.length>100000)throw Error('Fixture body budget');}
  const body=JSON.parse(raw),prompt=JSON.parse(body.messages.find(message=>message.role==='user').content),name=prompt.prefix.match(/READMD_INLINE_([a-z_]+)/)?.[1];assert(name);
  const current=plan(name);current.count++;requests.push({name,model:body.model});res.once('close',()=>current.closed.resolve());current.received.resolve();
  if(name==='automatic'){res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify({choices:[{message:{content:'test the evidence.'},finish_reason:'stop'}]}));return;}
  if(name==='revision'){await current.release.promise;if(res.destroyed)return;}
  res.writeHead(200,{'Content-Type':'text/event-stream'});const frame=value=>res.write('data: '+JSON.stringify(value)+'\n\n'),delta=value=>frame({choices:[{delta:{content:value}}]});
  if(name==='bad_utf'){res.write(Buffer.concat([Buffer.from('data: {"choices":[{"delta":{"content":"bad'),Buffer.from([0xc0,0xaf]),Buffer.from('text"}}]}\n\n')]));res.end('data: [DONE]\n\n');return;}
  delta(name==='normal'?'A中':name==='revision'?'late candidate':'visible partial');
  if(name==='eof'){res.end();return;}
  if(['partial','escape','ime'].includes(name))return;
  if(name==='normal'){await current.release.promise;if(res.destroyed)return;delta('文😀 candidate');}
  frame({choices:[{delta:{},finish_reason:'stop'}]});res.end('data: [DONE]\n\n');
 }catch{res.destroy();}});
 server.on('connection',socket=>{sockets.add(socket);socket.once('close',()=>sockets.delete(socket));});await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 return{base:'http://127.0.0.1:'+server.address().port,plan,requests,async close(){for(const value of plans.values())value.release.resolve();for(const socket of sockets)socket.destroy();await new Promise(resolve=>server.close(resolve));}};
}
async function main(){
 const args=options(process.argv.slice(2));assert(args.bin&&!args.base);const output=args.evidence;
 const report={driverSha256:sha(fs.readFileSync(__filename)),binarySha256:sha(fs.readFileSync(args.bin)),bootSha256:sha(fs.readFileSync(ROOT+'/assets/readmd.boot.js')),headless:true,nativeUiCertified:false,modelsRequested:false,productionAuthAndCsp:true,keymapInvokedInPage:true,ime:'synthetic DOM composition events, not OS IME',checks:[],passed:false};
 const originals=new Map(),saved=new Map();let provider,test,restarted,phase='start',restartScratch;
 try{
  provider=await providerFixture();
  test=await openOwned(output,async(api,paths)=>{
   await api('/api/ai/config',{providers:[{id:'custom:inline-browser',name:'Synthetic inline browser',category:'local',format:'openai',mode:'chat',base_url:provider.base+'/v1',models:['synthetic-fixed'],autocomplete:{mode:'current'}}],current:{provider_id:'custom:inline-browser',model:'synthetic-fixed'}});
   for(const name of ['revision','normal','partial','escape','ime','automatic','eof','bad_utf']){const text='READMD_INLINE_'+name+'\n\nSynthetic source 😀 for '+name+'.\n';originals.set(name,text);fs.writeFileSync(path.join(paths.documents,name+'.md'),text);}
  },{binary:args.bin,playwright:args.playwright||require.resolve('playwright',{paths:[path.resolve(ROOT,'../../rust-ReadMD/ui-tests'),ROOT+'/ui-tests']})});
  const {page}=test;
  assert.equal(await page.evaluate(()=>Boolean(window.ReadMDCompletionStream)),true,'A newly bundled completion candidate is required');
  async function enter(name){
   await page.evaluate(async file=>{setEditorPref('aiCompletion',false);await loadFile(file);await loadAiConfig();if(!state.editing)await toggleEdit();setPvLayout('none');await ReadMDReview.flush(getActiveTab());setEditorPref('aiCompletion',true);cmView.dispatch({selection:{anchor:cmView.state.doc.length}});cmView.focus();},path.join(test.documents,name+'.md'));
  }
  const ask=()=>page.evaluate(()=>ReadMDInlineCompletion.request(cmView));
  const ghost=()=>page.locator('.readmd-ai-ghost').textContent();
  async function visible(text){await until(async()=>await page.locator('.readmd-ai-ghost').count()===1&&await ghost()===text,'visible candidate '+text);}
  async function watchTransactions(){await page.evaluate(()=>{window.__inlineTransactions=[];cmView.dispatch({effects:ReadMDCodeMirror.StateEffect.appendConfig.of(ReadMDCodeMirror.EditorView.updateListener.of(update=>{for(const tr of update.transactions)if(tr.docChanged)window.__inlineTransactions.push({completion:tr.isUserEvent('input.complete'),text:tr.newDoc.toString()});}))});});}

  phase='revision-only stale candidate';await enter('revision');
  const before=await page.evaluate(async()=>{const tab=getActiveTab(),stamp=await ReadMDReview.capture(tab),at=cmView.state.doc.length;await ReadMDReview.propose([{from:at,to:at,insert:'Pending source.'}],stamp,tab);await ReadMDReview.flush(tab);const snap=ReadMDReview.snapshot(tab);window.__inlinePending=snap.changes.find(change=>change.decision==='pending').id;cmView.dispatch({selection:{anchor:cmView.state.doc.length}});cmView.focus();window.__inlineDoc=cmView.state.doc;ReadMDInlineCompletion.request(cmView);return{generation:snap.generation,review_revision:snap.review_revision,content:cmView.state.doc.toString()};});
  await until(()=>provider.plan('revision').received.settled,'held real completion');
  const revised=await page.evaluate(async()=>{const tab=getActiveTab();await ReadMDReview.resolve([window.__inlinePending],'accept',tab);await ReadMDReview.flush(tab);const snap=ReadMDReview.snapshot(tab);return{generation:snap.generation,review_revision:snap.review_revision,content:cmView.state.doc.toString(),sameDoc:cmView.state.doc===window.__inlineDoc};});
  assert(revised.review_revision>before.review_revision);assert(revised.sameDoc);assert.equal(revised.content,before.content);await until(()=>provider.plan('revision').closed.settled,'revision invalidation closes owned provider',2000);provider.plan('revision').release.resolve();await pause(100);
  assert.equal(await page.evaluate(()=>ReadMDInlineCompletion.accept(cmView)),false);assert.equal(await page.evaluate(()=>cmView.state.doc.toString()),before.content);assert.equal(await page.locator('.readmd-ai-ghost').count(),0);report.checks.push('a real revision-only acknowledgement keeps the same document object, closes the old completion and rejects its late candidate');

  phase='progressive ghost, one transaction, undo and save';await enter('normal');await watchTransactions();await ask();await visible('A中');assert.equal(await page.evaluate(()=>cmView.state.doc.toString()),originals.get('normal'));provider.plan('normal').release.resolve();await visible('A中文😀 candidate');
  await until(()=>page.locator('.readmd-ai-ghost').getAttribute('data-streaming').then(value=>value!== 'true'),'terminal ghost');
  const accepted=await page.evaluate(async()=>{const key=cmPriorityKeymap().find(binding=>binding.key==='Tab');const first=key.run(cmView),second=ReadMDInlineCompletion.accept(cmView);await ReadMDReview.flush(getActiveTab());return{first,second,content:cmView.state.doc.toString(),transactions:window.__inlineTransactions};});
  const completed=originals.get('normal')+'A中文😀 candidate';assert.equal(accepted.first,true);assert.equal(accepted.second,false);assert.equal(accepted.content,completed);assert.equal(accepted.transactions.filter(value=>value.completion).length,1);
  assert.equal(await page.evaluate(async()=>{ReadMDReview.history(false);await ReadMDReview.flush(getActiveTab());return cmView.state.doc.toString();}),originals.get('normal'));
  assert.equal(await page.evaluate(async()=>{ReadMDReview.history(true);await ReadMDReview.flush(getActiveTab());return cmView.state.doc.toString();}),completed);
  assert.equal(await page.evaluate(()=>saveEdit()),true);assert.equal(fs.readFileSync(path.join(test.documents,'normal.md'),'utf8'),completed);saved.set('normal',completed);report.checks.push('actual provider chunks stay outside the draft until the production Tab keymap inserts one transaction; durable undo/redo and save retain the exact source');

  phase='unaccepted partial save and partial Tab';await enter('partial');await ask();await visible('visible partial');assert.equal(await page.evaluate(()=>saveEdit()),true);assert.equal(fs.readFileSync(path.join(test.documents,'partial.md'),'utf8'),originals.get('partial'));await page.evaluate(()=>setEditorPref('aiCompletion',false));await until(()=>provider.plan('partial').closed.settled,'unaccepted save cancels provider',2000);
  await page.evaluate(()=>{setEditorPref('aiCompletion',true);cmView.focus();ReadMDInlineCompletion.request(cmView);});await visible('visible partial');await watchTransactions();
  const partial=await page.evaluate(async()=>{const first=cmPriorityKeymap().find(binding=>binding.key==='Tab').run(cmView),second=ReadMDInlineCompletion.accept(cmView);await ReadMDReview.flush(getActiveTab());return{first,second,content:cmView.state.doc.toString(),count:window.__inlineTransactions.filter(value=>value.completion).length};});
  assert(partial.first);assert.equal(partial.second,false);assert.equal(partial.count,1);assert.equal(partial.content,originals.get('partial')+'visible partial');
  await page.evaluate(async()=>{ReadMDReview.history(false);await ReadMDReview.flush(getActiveTab());});assert.equal(await page.evaluate(()=>saveEdit()),true);saved.set('partial',originals.get('partial'));report.checks.push('an unaccepted streaming ghost is absent from the real saved file; explicit partial Tab inserts once and its undo/save restore the original');

  phase='Escape keymap and composition lifecycle';for(const name of ['escape','ime']){
   await enter(name);await ask();await visible('visible partial');
   if(name==='escape')assert.equal(await page.evaluate(()=>cmPriorityKeymap().find(binding=>binding.key==='Escape').run(cmView)),true);
   else await page.evaluate(()=>document.querySelector('.cm-content').dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true,data:'中'})));
   await until(async()=>await page.locator('.readmd-ai-ghost').count()===0,'cleared '+name);await until(()=>provider.plan(name).closed.settled,'cancelled '+name+' provider',2000);assert.equal(await page.evaluate(()=>cmView.state.doc.toString()),originals.get(name));
   await page.evaluate(()=>setEditorPref('aiCompletion',false));if(name==='ime')await page.evaluate(()=>document.querySelector('.cm-content').dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:''})));
  }report.checks.push('production Escape and DOM composition-start clear ghost text, cancel only their owned request and preserve the draft');

  phase='automatic request after local persistence and type-through';await enter('automatic');await page.evaluate(()=>{const at=cmView.state.doc.length;cmView.dispatch({changes:{from:at,insert:' trigger'},selection:{anchor:at+8},userEvent:'input.type'});});await visible('test the evidence.');
  await page.evaluate(()=>{const at=cmView.state.doc.length;cmView.dispatch({changes:{from:at,insert:'test '},selection:{anchor:at+5},userEvent:'input.type'});});await visible('the evidence.');await pause(800);assert.equal(provider.plan('automatic').count,1);
  const typed=originals.get('automatic')+' triggertest ';assert.equal(await page.evaluate(()=>cmView.state.doc.toString()),typed);assert.equal(await page.evaluate(()=>saveEdit()),true);assert.equal(fs.readFileSync(path.join(test.documents,'automatic.md'),'utf8'),typed);saved.set('automatic',typed);report.checks.push('automatic completion survives the expected local write acknowledgement, and type-through carries its suffix without another provider request or saving the ghost');

  phase='upstream EOF and bad UTF-8';for(const name of ['eof','bad_utf']){
   await enter(name);await ask();await until(()=>page.evaluate(()=>document.getElementById('status-ai-completion').dataset.state==='editor.aiCompletionUnavailable'),'invalid stream rejected '+name);assert.equal(await page.locator('.readmd-ai-ghost').count(),0);assert.equal(await page.evaluate(()=>cmView.state.doc.toString()),originals.get(name));
  }report.checks.push('clean EOF without a provider terminator and malformed upstream UTF-8 retract all provisional candidates and preserve source');

  assert.deepEqual(test.errors,[]);assert.equal((await test.api('/api/ai/history?session=default')).messages.length,0);await page.evaluate(async()=>{setEditorPref('aiCompletion',false);await ReadMDReview.flushAll();await window.ReadMDSession.flush();});const profile=test.profile,documents=test.documents,firstPid=test.host.pid;await test.close();test=null;
  phase='new process recovery';restartScratch=path.join(output,'restart-scratch');fs.mkdirSync(restartScratch,{mode:0o700});Object.assign(process.env,{TMPDIR:restartScratch,TMP:restartScratch,TEMP:restartScratch,CFFIXED_USER_HOME:restartScratch});const requestCount=provider.requests.length;
  restarted=await startKernel(args.bin,profile,documents);assert.notEqual(restarted.pid,firstPid);
  for(const [name,expected] of saved){const file=path.join(documents,name+'.md'),response=await restarted.api('/api/file?p='+encodeURIComponent(file));assert.equal(response.status,200);assert.equal((await response.json()).content,expected);const review=await restarted.api('/api/review',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({action:'resume',path:file})});assert.equal(review.status,200);const receipt=await review.json();assert.equal(receipt.found,true);assert.equal(receipt.snapshot.content,expected);}
  assert.equal(provider.requests.length,requestCount);report.checks.push('a different real kernel process reopens the saved and durable source exactly; no unaccepted ghost or automatic provider request appears during recovery');report.passed=true;
 }catch(error){report.failure={phase,name:error.name,message:String(error.message).replaceAll(output,'<owned>').replaceAll(ROOT,'<repo>').slice(0,800)};if(test)await test.page.screenshot({path:path.join(output,'first-failure.png')}).catch(()=>{});process.exitCode=1;}
 finally{if(test)await test.close();if(restarted)await restarted.stop();if(restartScratch&&fs.existsSync(restartScratch)){assert(fs.lstatSync(restartScratch).isDirectory()&&!fs.lstatSync(restartScratch).isSymbolicLink());fs.rmSync(restartScratch,{recursive:true});}if(provider)await provider.close();report.scratchAbsent=['tmp','home','browser-profile','restart-scratch'].every(name=>!fs.existsSync(path.join(output,name)));fs.writeFileSync(path.join(output,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx',mode:0o600});console.log(JSON.stringify(report));}
}
if(require.main===module)main().catch(()=>{console.error('Owned inline browser fixture setup failed');process.exitCode=1;});
module.exports={main};
