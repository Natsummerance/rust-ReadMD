'use strict';
// A real SQL result arrives while the user's dirty-close decision is pending.
// Only network delivery is delayed; auth, CSP, execution, files and history
// remain production Rust services. This is headless, not native chooser QA.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {options}=require('./v006-table-clipboard-kernel.cjs'),{startKernel}=require('./v006-refactor-host.cjs');
const ROOT=path.resolve(__dirname,'../..'),pause=ms=>new Promise(resolve=>setTimeout(resolve,ms)),sha=data=>crypto.createHash('sha256').update(data).digest('hex');
const code='```sql cmd output=true\nSELECT 42;\n```\n';
const source=mode=>'# Close 😀\n\nKeep.\n\n'+code+(mode==='batch'?'\n'+code:'');
const encoded=text=>Buffer.from('\ufeff'+text.replaceAll('\n','\r\n'));
async function until(fn,label){const end=Date.now()+15000;while(Date.now()<end){if(await fn())return;await pause(20);}throw Error('Deadline: '+label);}
async function main(){
 const args=options(process.argv.slice(2));assert(args.bin&&!args.base);
 for(const name of ['profile','documents','tmp','browser-home'])fs.mkdirSync(path.join(args.evidence,name));
 const profile=path.join(args.evidence,'profile'),docs=path.join(args.evidence,'documents');
 const report={passed:false,headless:true,nativeUiCertified:false,productionCsp:true,strictAuth:true,modelsRequested:false,binarySha256:sha(fs.readFileSync(args.bin)),bootSha256:sha(fs.readFileSync(path.join(ROOT,'assets/readmd.boot.js'))),checks:[],observations:[],pageErrors:[],kernelPids:[]};
 const {chromium}=require(args.playwright||require.resolve('playwright',{paths:[path.join(ROOT,'ui-tests')]}));
 let host,browser,page,phase='startup';
 const watchdog=setTimeout(()=>{void browser?.close();},120000);
 async function start(){
  host=await startKernel(args.bin,profile,docs);report.kernelPids.push(host.pid);const root=await host.api('/'),base=new URL(root.url).origin;
  const env=Object.fromEntries(Object.entries(process.env).filter(([key])=>/^(PATH|SYSTEMROOT|WINDIR|COMSPEC|PATHEXT|LANG|LC_ALL)$/i.test(key)));
  Object.assign(env,{TMPDIR:path.join(args.evidence,'tmp'),CFFIXED_USER_HOME:path.join(args.evidence,'browser-home'),CHROME_LOG_FILE:path.join(args.evidence,'chrome.log')});
  const channel=process.env.READMD_CODE_BROWSER||'chrome';browser=await chromium.launch({channel:channel==='chromium'?undefined:channel,headless:true,env,args:['--disable-crash-reporter','--disable-breakpad']});
  page=await browser.newPage({viewport:{width:1160,height:820},bypassCSP:false,serviceWorkers:'block'});page.setDefaultTimeout(10000);
  page.on('pageerror',error=>report.pageErrors.push(error.message.replaceAll(args.evidence,'<evidence>').slice(0,500)));
  await page.route('**/*',route=>new URL(route.request().url()).origin===base?route.continue():route.abort());
  await page.addInitScript(()=>{
   const headers=()=>{const token=document.querySelector('meta[name="readmd-app-token"]')?.content||'';return{'Content-Type':'application/json','X-ReadMD-App-Token':token,'X-ReadMD-Token':token};};
   window.pywebview={api:{get_settings:async()=>{const v=await(await fetch('/api/settings',{headers:headers()})).json();return v?.settings&&typeof v.ok==='boolean'?v.settings:v;},save_settings:async v=>(await fetch('/api/settings',{method:'POST',headers:headers(),body:JSON.stringify(v)})).json(),get_system_language:async()=>'zh-CN',get_autostart:async()=>false,get_app_info:async()=>({}),start_modules:async()=>true}};
  });
  const boot=page.waitForResponse(r=>new URL(r.url()).pathname==='/assets/readmd.boot.js');await page.goto(base);assert.equal(sha(await(await boot).body()),report.bootSha256);
  const csp=await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content');assert(csp.includes("script-src 'self'"));assert(!/script-src[^;]*'unsafe-(inline|eval)'/.test(csp));
  await until(()=>page.evaluate(()=>Boolean(window.__readmdAppReady&&window.ReadMDCodeRuns)),'application');await page.evaluate(()=>window.__readmdSessionReady);
 }
 async function stop(){if(browser){await browser.close();browser=null;}if(host){await host.stop();host=null;}}
 try{
  await start();
  for(const mode of ['single','batch'])for(const choice of ['discard','cancel','save']){
   phase=mode+'_'+choice;console.log(JSON.stringify({phase}));const name=phase+'.md',file=path.join(docs,name),original=source(mode),manual=original.replace('Keep.','Unsaved.');fs.writeFileSync(file,encoded(original));
   await page.evaluate(async file=>{await loadFile(file);if(!state.editing)await toggleEdit();setPvLayout('right');await renderPreview();const body=cmView.state.doc.toString(),at=body.indexOf('Keep.');cmView.dispatch({changes:{from:at,to:at+5,insert:'Unsaved.'}});if(!await ReadMDReview.flush(getActiveTab()))throw Error('manual_flush');},file);
   let entered,release,delivered;const ready=new Promise(r=>{entered=r;}),gate=new Promise(r=>{release=r;}),delivery=new Promise(r=>{delivered=r;});
   const routePattern=mode==='single'?'**/api/code/run':'**/api/review';
   const handler=async route=>{if(mode==='batch'&&route.request().postDataJSON()?.action!=='code_batch_run')return route.continue();const response=await route.fetch();const result=await response.json();assert.equal(result.ok,true);entered();await gate;await route.fulfill({response});delivered();};
   await page.route(routePattern,handler);
   if(mode==='single')await page.locator('#preview-pane .code-chunk-run-btn').first().click();else await page.evaluate(()=>document.getElementById('btn-run-all-chunks').click());
   await ready;
   const closing=page.evaluate(()=>closeTab(getActiveTab().id));closing.catch(()=>{});
   await until(()=>page.locator('#close-confirm-modal').isVisible(),'dirty-close modal');release();await delivery;await pause(200);
   const during=await page.evaluate(()=>cmView.state.doc.toString());
   assert.deepEqual(fs.readFileSync(file),encoded(original),'no save before the choice');
   await page.locator('#close-confirm-'+choice).click();await closing;await until(()=>page.evaluate(()=>ReadMDCodeRuns.busy()===0),'code terminal');await page.unroute(routePattern,handler);
   const exists=await page.evaluate(name=>state.tabs.some(t=>t.name===name),name);
   report.observations.push({mode,choice,decisionBodyStable:during===manual,tabStillOpen:exists});
   assert.equal(during,manual,'completed code must not mutate the pending close decision');
   if(choice==='cancel'){
    assert.equal(exists,true);const current=await page.evaluate(()=>cmView.state.doc.toString());assert(current.includes('Unsaved.')&&current.includes('<!-- code_chunk_output -->'));
    assert.deepEqual(fs.readFileSync(file),encoded(current));
    assert.equal(await page.evaluate(()=>ReadMDReview.history(false)),true);await until(()=>page.evaluate(()=>!ReadMDReview.busy(getActiveTab())),'one undo');
    assert.equal(await page.evaluate(()=>cmView.state.doc.toString()),manual);assert.equal(await page.evaluate(()=>saveEdit()),true);await page.evaluate(()=>closeTab(getActiveTab().id));
   }else{assert.equal(exists,false,'the first explicit choice must close');assert.deepEqual(fs.readFileSync(file),encoded(choice==='save'?manual:original));}
   report.checks.push({mode,choice,passed:true});
  }
  assert.equal(await page.evaluate(()=>ReadMDSession.flush()),true);await stop();await start();assert.equal(await page.evaluate(()=>state.tabs.length),0);assert.equal(await page.evaluate(()=>ReadMDCodeRuns.busy()),0);
  assert.equal(report.pageErrors.length,0);report.checks.push({newProcessExplicitClosesAndNoCodeResume:true});report.passed=true;
 }catch(error){report.failure={phase,message:String(error.message).replaceAll(args.evidence,'<evidence>').replaceAll(ROOT,'<repo>').slice(0,1600)};if(page&&!page.isClosed())await page.screenshot({path:path.join(args.evidence,'first-failure.png')}).catch(()=>{});process.exitCode=1;}
 finally{clearTimeout(watchdog);await stop();for(const name of ['tmp','browser-home'])fs.rmSync(path.join(args.evidence,name),{recursive:true,force:true});fs.writeFileSync(path.join(args.evidence,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,failure:report.failure}));}
}
if(require.main===module)main().catch(()=>{console.error('Code close-boundary acceptance failed; inspect the owned report.');process.exitCode=1;});
