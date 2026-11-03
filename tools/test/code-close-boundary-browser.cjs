'use strict';
// Real V006 services/files, with delayed delivery of an actual SQL response.
// Observing the existing persistence promise does not replace it or its result.
// Browser evidence; no native window/chooser or managed runtime certification.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {options}=require('./v006-table-clipboard-kernel.cjs'),{startKernel}=require('./v006-refactor-host.cjs');
const ROOT=path.resolve(__dirname,'../..'),pause=ms=>new Promise(r=>setTimeout(r,ms)),sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const original='# Close 😀\n\nKeep.\n\n```sql cmd output=true\nSELECT 42;\n```\n',manual=original.replace('Keep.','Unsaved.');
const encoded=s=>Buffer.from('\ufeff'+s.replaceAll('\n','\r\n'));
async function until(fn,label){const end=Date.now()+15000;while(Date.now()<end){if(await fn())return;await pause(20);}throw Error('Deadline: '+label);}
async function bounded(promise,label){let timer;try{return await Promise.race([promise,new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('Deadline: '+label)),15000);})]);}finally{clearTimeout(timer);}}
async function main(){
 const args=options(process.argv.slice(2));assert(args.bin&&!args.base);
 for(const dir of ['profile','documents','tmp','browser-home'])fs.mkdirSync(path.join(args.evidence,dir));
 const profile=path.join(args.evidence,'profile'),docs=path.join(args.evidence,'documents');
 const report={passed:false,productionCsp:true,strictAuth:true,headless:true,nativeUiCertified:false,modelsRequested:false,binarySha256:sha(fs.readFileSync(args.bin)),bootSha256:sha(fs.readFileSync(path.join(ROOT,'assets/readmd.boot.js'))),checks:[],observations:[],pageErrors:[],kernelPids:[]};
 const {chromium}=require(args.playwright||require.resolve('playwright',{paths:[path.join(ROOT,'ui-tests')]}));
 let host,browser,page,phase='startup';const watchdog=setTimeout(()=>{process.exitCode=1;fs.writeFileSync(path.join(args.evidence,'deadline.json'),JSON.stringify({phase,acceptanceIncomplete:true})+'\n',{flag:'wx'});void browser?.close();},120000);
 async function start(){
  host=await startKernel(args.bin,profile,docs);report.kernelPids.push(host.pid);const root=await host.api('/'),base=new URL(root.url).origin;
  const env=Object.fromEntries(Object.entries(process.env).filter(([key])=>/^(PATH|SYSTEMROOT|WINDIR|COMSPEC|PATHEXT|LANG|LC_ALL)$/i.test(key)));
  Object.assign(env,{TMPDIR:path.join(args.evidence,'tmp'),CFFIXED_USER_HOME:path.join(args.evidence,'browser-home'),CHROME_LOG_FILE:path.join(args.evidence,'chrome.log')});
  const channel=process.env.READMD_CODE_BROWSER||'chrome';browser=await chromium.launch({channel:channel==='chromium'?undefined:channel,headless:true,env,args:['--disable-crash-reporter','--disable-breakpad']});
  page=await browser.newPage({viewport:{width:1160,height:820},bypassCSP:false,serviceWorkers:'block'});page.setDefaultTimeout(10000);
  page.on('pageerror',e=>report.pageErrors.push(e.message.replaceAll(args.evidence,'<evidence>').slice(0,500)));
  await page.route('**/*',route=>new URL(route.request().url()).origin===base?route.continue():route.abort());
  await page.addInitScript(()=>{
   const headers=()=>{const token=document.querySelector('meta[name="readmd-app-token"]')?.content||'';return{'Content-Type':'application/json','X-ReadMD-App-Token':token,'X-ReadMD-Token':token};};
   window.pywebview={api:{get_settings:async()=>{const v=await(await fetch('/api/settings',{headers:headers()})).json();return v?.settings&&typeof v.ok==='boolean'?v.settings:v;},save_settings:async v=>(await fetch('/api/settings',{method:'POST',headers:headers(),body:JSON.stringify(v)})).json(),get_system_language:async()=>'zh-CN',get_autostart:async()=>false,get_app_info:async()=>({}),start_modules:async()=>true}};
  });
  const boot=page.waitForResponse(r=>new URL(r.url()).pathname==='/assets/readmd.boot.js');await page.goto(base);assert.equal(sha(await(await boot).body()),report.bootSha256);
  const csp=await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content');assert(csp.includes("script-src 'self'"));assert(!/script-src[^;]*'unsafe-(inline|eval)'/.test(csp));
  await until(()=>page.evaluate(()=>Boolean(window.__readmdAppReady&&window.ReadMDLifecycle)),'application');await page.evaluate(()=>window.__readmdSessionReady);
  await page.evaluate(()=>{window.__codeObserved={pending:0,results:[]};const original=window.persistCodeChunkOutput;window.persistCodeChunkOutput=async function(...args){__codeObserved.pending++;try{const result=await original.apply(this,args);__codeObserved.results.push(result);return result;}finally{__codeObserved.pending--;}};});
 }
 async function stop(){if(browser){await browser.close();browser=null;}if(host){await host.stop();host=null;}}
 const body=()=>page.evaluate(()=>cmView.state.doc.toString());
 async function edit(name,unsaved=false){const file=path.join(docs,name);if(!fs.existsSync(file))fs.writeFileSync(file,encoded(original));await page.evaluate(async({file,unsaved})=>{await loadFile(file);if(!state.editing)await toggleEdit();setPvLayout('right');await renderPreview();if(unsaved){const at=cmView.state.doc.toString().indexOf('Keep.');cmView.dispatch({changes:{from:at,to:at+5,insert:'Unsaved.'}});}if(!await ReadMDReview.flush(getActiveTab()))throw Error('manual_flush');}, {file,unsaved});return file;}
 async function held(){
  let entered,release,delivered;const ready=new Promise(r=>{entered=r;}),gate=new Promise(r=>{release=r;}),delivery=new Promise(r=>{delivered=r;});
  const handler=async route=>{const response=await route.fetch();assert.equal((await response.json()).ok,true);entered();await gate;await route.fulfill({response});delivered();};
  await page.route('**/api/code/run',handler);const count=await page.evaluate(()=>__codeObserved.results.length);await page.locator('#preview-pane .code-chunk-run-btn').click();await bounded(ready,'actual SQL response');
  return{release,delivery,async finish(){release();await delivery;await until(()=>page.evaluate(count=>__codeObserved.pending===0&&__codeObserved.results.length===count+1,count),'persistence terminal');await page.unroute('**/api/code/run',handler);}};
 }
 async function closeDiscard(){const closing=page.evaluate(()=>closeTab(getActiveTab().id));closing.catch(()=>{});await until(async()=>await page.locator('#close-confirm-modal').isVisible()||await page.evaluate(()=>state.tabs.length===0),'close');if(await page.locator('#close-confirm-modal').isVisible())await page.locator('#close-confirm-discard').click();await closing;}
 try{
  await start();
  for(const action of ['tab','edit'])for(const choice of ['discard','cancel','save']){
   phase=action+'_'+choice;console.log(JSON.stringify({phase}));const name=phase+'.md',file=await edit(name,true),result=await held();
   const closing=page.evaluate(action=>action==='tab'?closeTab(getActiveTab().id):confirmExitEdit(),action);closing.catch(()=>{});
   await until(()=>page.locator('#close-confirm-modal').isVisible(),'dirty decision');result.release();await result.delivery;await pause(150);
   const during=await body();assert.deepEqual(fs.readFileSync(file),encoded(original),'no save before choice');
   await page.locator('#close-confirm-'+choice).click();await bounded(closing,'decision completion');await result.finish();
   const after=await page.evaluate(name=>({open:state.tabs.some(t=>t.name===name),editing:state.editing,result:__codeObserved.results.at(-1)}),name);
   report.observations.push({action,choice,decisionBodyStable:during===manual,...after});assert.equal(during,manual,'result cannot mutate the pending decision');
   if(choice==='cancel'){
    assert(after.open&&after.editing&&after.result);const current=await body();assert(current.includes('Unsaved.')&&current.includes('<!-- code_chunk_output -->'));assert.deepEqual(fs.readFileSync(file),encoded(current));
    if(action==='tab'){const again=await held();await again.finish();assert.equal(await body(),current,'identical rerun is a source no-op');assert.equal(await page.evaluate(()=>__codeObserved.results.at(-1)),true);assert.deepEqual(fs.readFileSync(file),encoded(current));}
    assert.equal(await page.evaluate(()=>ReadMDReview.history(false)),true);await until(async()=>await body()===manual,'one undo');assert.equal(await page.evaluate(()=>ReadMDReview.history(true)),true);await until(async()=>await body()===current,'one redo');assert.equal(await page.evaluate(()=>saveEdit()),true);await page.evaluate(()=>closeTab(getActiveTab().id));
   }else{
    assert.equal(after.result,false,'closed/exited editor rejects late source writes');assert.equal(after.open,action==='edit');assert.equal(after.editing,false);assert.deepEqual(fs.readFileSync(file),encoded(choice==='save'?manual:original));if(after.open)await page.evaluate(()=>closeTab(getActiveTab().id));
   }
   report.checks.push({action,choice,passed:true});
  }
  phase='cross_document';const b=await edit('other.md'),a=await edit('owner.md'),switchResult=await held();await edit('other.md');await switchResult.finish();assert.equal(await body(),original);assert.deepEqual(fs.readFileSync(a),encoded(original));assert.deepEqual(fs.readFileSync(b),encoded(original));assert.equal(await page.evaluate(()=>__codeObserved.results.at(-1)),false);await page.evaluate(()=>closeAllTabs());report.checks.push({crossDocument:true});
  phase='edit_and_undo';const stale=await edit('stale.md'),staleResult=await held();await page.evaluate(async()=>{cmView.dispatch({changes:{from:0,insert:'Temporary\n'}});if(!await ReadMDReview.flush(getActiveTab()))throw Error('flush');if(!ReadMDReview.history(false))throw Error('undo');});await until(async()=>await body()===original,'actual undo to identical text');await staleResult.finish();assert.equal(await body(),original);assert.deepEqual(fs.readFileSync(stale),encoded(original));assert.equal(await page.evaluate(()=>__codeObserved.results.at(-1)),false);await closeDiscard();report.checks.push({sameTextNewGenerationRefused:true});
  phase='external_save_conflict';const conflict=await edit('external.md'),conflictResult=await held(),external=original.replace('Keep.','External revision.');fs.writeFileSync(conflict,encoded(external));conflictResult.release();await conflictResult.delivery;await page.locator('#save-conflict-modal').waitFor({state:'visible'});await page.locator('#save-conflict-cancel').click();await conflictResult.finish();assert.equal(await page.evaluate(()=>__codeObserved.results.at(-1)),false);assert.deepEqual(fs.readFileSync(conflict),encoded(external));assert((await body()).includes('<!-- code_chunk_output -->'),'failed save preserves the visible draft');await closeDiscard();report.checks.push({saveFailureNeverReportedAsSuccess:true,externalBytesPreserved:true});
  assert.equal(await page.evaluate(()=>ReadMDSession.flush()),true);await stop();await start();assert.equal(await page.evaluate(()=>state.tabs.length),0);assert.equal(await page.evaluate(()=>__codeObserved.pending),0);assert.equal(report.pageErrors.length,0);report.checks.push({newProcessExplicitClosesPersist:true});report.passed=true;
 }catch(error){report.failure={phase,message:String(error.message).replaceAll(args.evidence,'<evidence>').replaceAll(ROOT,'<repo>').slice(0,1600)};if(page&&!page.isClosed()){report.atFailure=await page.evaluate(()=>({editing:state.editing,lifecycleBusy:ReadMDLifecycle.busy(),reviewBusy:ReadMDReview.busy(getActiveTab()),observed:__codeObserved,cardOwners:[...document.querySelectorAll('#preview-pane .code-chunk-card')].map(c=>({sameOwner:c._readmdCodeOwner===getActiveTab(),bound:Boolean(c._bound),disabled:c.querySelector('button')?.disabled,status:c.querySelector('.code-chunk-status')?.className}))})).catch(()=>null);await page.screenshot({path:path.join(args.evidence,'first-failure.png')}).catch(()=>{});}process.exitCode=1;}
 finally{clearTimeout(watchdog);await stop();for(const name of ['tmp','browser-home'])fs.rmSync(path.join(args.evidence,name),{recursive:true,force:true});fs.writeFileSync(path.join(args.evidence,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,failure:report.failure}));}
}
if(require.main===module)main().catch(()=>{console.error('Code decision acceptance failed; inspect its owned receipt.');process.exitCode=1;});
