'use strict';
// Genuine status probes and Run compete through the production service. OS
// observations inspect only this fixture's backend children and private files.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {options}=require('./v006-table-clipboard-kernel.cjs'),{startKernel}=require('./v006-refactor-host.cjs');
const {nodePack,installNodePack,api:request,settle,sha}=require('./node-runtime-fixture.cjs');
const {sameWorker}=require('./code-mcp-eof.cjs');
const {StatusProbeObserver,prepareStatusFirst,observeRunProgress,waitBusy,before:assertBeforeDeadline,sameLive,sampleStorage,snapshots}=require('./node-runtime-status-observer.cjs');
const {ownedNodeWorker}=require('./node-runtime-http-stdio.cjs');
const ROOT=path.resolve(__dirname,'../..'),pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const TIMER="setTimeout(()=>console.log('LATE'),60000)";
const PROBE="console.log(JSON.stringify({version:process.version,sum:[6,7].reduce((a,b)=>a*b,1),unicode:'中文😀',write:process.permission.has('fs.write')}))";
function files(directory,proof){return proof.manifest.files.map(f=>{const p=path.join(directory,...f.path.split('/')),m=fs.lstatSync(p);assert(m.isFile()&&!m.isSymbolicLink());return{path:f.path,size:m.size,sha256:sha(fs.readFileSync(p))};});}
function noProbeCopies(directory){for(const name of fs.readdirSync(directory)){const file=path.join(directory,name),m=fs.lstatSync(file);assert(!m.isSymbolicLink(),'retain unexpected link in owned scratch');if(m.isDirectory())noProbeCopies(file);else assert(!['node','node.exe'].includes(name),'retain a probe copy whose retirement is not confirmed');}}
async function main(){
  const args=options(process.argv.slice(2));assert(args.bin&&!args.base);const proof=nodePack();
  const dirs=Object.fromEntries(['profile','documents','tmp'].map(n=>[n,path.join(args.evidence,n)]));for(const d of Object.values(dirs))fs.mkdirSync(d);
  Object.assign(process.env,{TMPDIR:dirs.tmp,TMP:dirs.tmp,TEMP:dirs.tmp});
  const sentinel=path.join(dirs.documents,'owned-sentinel.md'),source=Buffer.from('# Synthetic Node status fairness\r\n\r\nNo model or user file.\r\n');fs.writeFileSync(sentinel,source);
  const report={passed:false,platform:process.platform,arch:process.arch,modelsRequested:false,nativeUiCertified:false,binarySha256:sha(fs.readFileSync(args.bin)),bootSha256:sha(fs.readFileSync(path.join(ROOT,'assets/readmd.boot.js'))),driverSha256:sha(fs.readFileSync(__filename)),packSha256:proof.sha256,manifestSha256:proof.manifestSha256,timerSha256:sha(TIMER),probeSourceSha256:sha(PROBE),observationProtocol:'status-first-lease-progress-v2',observerSha256:sha(fs.readFileSync(path.join(__dirname,'node-runtime-status-observer.cjs'))),validationOnlyLockCoverage:'separate actual-operate validation test required; this HTTP receipt does not certify every validation phase',osProcessCoLiveness:{outcome:'not_certified',mandatory:false,reason:'No atomic or bracketed two-process witness is claimed by this contract'},checks:[],statuses:[],probeObservations:[],probeSchedulingSignalsUsed:false,cleanupErrors:[]};
  const started=performance.now(),at=()=>Math.round((performance.now()-started)*1000)/1000,safe=s=>String(s).replaceAll(args.evidence,'<evidence>').replaceAll(ROOT,'<repo>');
  let host,task,running,timerWorker,observedProbe,observer,statusLoop,stopStatus=false,phase='startup',installed=false,tmpBlocked=false;
  const mark=value=>{phase=value;console.log(JSON.stringify({phase,atMs:at()}));};
  const api=(route,body)=>request(host,route,body),pack=body=>api('/api/capabilities/packs',body);
  const directory=path.join(dirs.profile,'capabilities/managed-v2/node',proof.manifest.version);
  report.storage={peakObservedBytes:0,transientBudgetBytes:450*1048576,retainedBudgetBytes:250*1048576};const sample=()=>{try{const value=sampleStorage(args.evidence,observer);report.storage.lastSample=value;for(const [name,bytes] of Object.entries({peakPathnameBytes:value.pathnameBytes,peakLinkedFdBytes:value.linkedFdBytes,peakUnlinkedOpenBytes:value.unlinkedOpenBytes,peakFdSupplementBytes:value.supplementBytes}))report.storage[name]=Math.max(report.storage[name]||0,bytes);report.storage.peakObservedBytes=Math.max(report.storage.peakObservedBytes,value.totalBytes);}catch(e){if(!report.storage.observationError){report.storage.observationError=safe(e.message);report.cleanupErrors.push('storage observation failed: '+safe(e.message));}}};const sampler=setInterval(sample,50);
  try{
    mark('real authenticated offline install');host=await startKernel(args.bin,dirs.profile,dirs.documents);report.hostPid=host.pid;report.installAttempted=true;report.installed=await installNodePack(host,proof);installed=true;
    const before=files(directory,proof);report.installedFiles=before;
    const now=()=>performance.now(),preparationStarted=now(),preparationDeadline=preparationStarted+6000;
    report.deadlines={clock:'performance.now()',preparation:{startedAtMonotonicMs:preparationStarted,deadlineMonotonicMs:preparationDeadline,budgetMs:6000}};
    const pending=()=>report.statuses.find(row=>!row.settled),record=value=>{report.probeObservations.push({atMs:at(),statusIndex:observer?.selected?.anchor?.index??pending()?.index??null,...value});sample();};
    observer=new StatusProbeObserver({profile:dirs.profile,tmp:dirs.tmp,hostPid:host.pid,expected:proof.manifest.files.find(f=>f.path==='bin/node'),probeSource:PROBE,record,now});
    mark('continuous real status begins before Run');
    statusLoop=(async()=>{while(!stopStatus){const row={index:report.statuses.length,requestedAtMs:at(),requestedAtMonotonicMs:now(),settled:false};report.statuses.push(row);try{const result=await pack({action:'status',id:'node'});Object.assign(row,{settled:true,settledAtMs:at(),settledAtMonotonicMs:now(),...result});}catch(e){Object.assign(row,{settled:true,settledAtMs:at(),settledAtMonotonicMs:now(),error:safe(e.message)});throw e;}await pause(10);}})();statusLoop.catch(()=>{});
    report.precondition={mode:'same owned status Header+Created before the unique Run; copy preparation only',budgetMs:6000,maxVerifiedCandidates:6,outcome:'pending'};
    const selected=await prepareStatusFirst({observer,pending,record,now,deadline:preparationDeadline});
    const anchor=selected.anchor;report.anchorStatus=anchor.index;report.statusWorkBarrier=selected.proof;
    Object.assign(report.precondition,{outcome:'owned_copy_preparation_observed',observedAtMs:at(),taskId:selected.proof.taskId});
    mark('timer must start while the status-first operation can admit Run');task=crypto.randomUUID();
    assertBeforeDeadline(preparationDeadline,now,'status-first handoff');assert(!anchor.settled,'immutable anchor remains pending before the unique Run');assert(!report.running);
    const requestedAt=now(),timerDeadline=requestedAt+3000;
    report.running={taskId:task,requestedAtMs:at(),requestedAtMonotonicMs:requestedAt,settled:false};
    report.deadlines.timerProbeHash={startedAtMonotonicMs:requestedAt,deadlineMonotonicMs:timerDeadline,budgetMs:3000};
    assertBeforeDeadline(preparationDeadline,now,'unique Run dispatch');
    report.runRequestDispatched=true;
    running=api('/api/code/run',{confirm:true,lang:'javascript',code:TIMER,task_id:task}).then(receipt=>{Object.assign(report.running,{settled:true,settledAtMs:at(),receipt});return receipt;},e=>{Object.assign(report.running,{settled:true,settledAtMs:at(),error:safe(e.message)});throw e;});running.catch(()=>{});
    const progress=await observeRunProgress({observer,now,deadline:timerDeadline,finished:()=>report.running.settled,readWorkers:()=>snapshots(host.pid),findTimer:workers=>{
      const found=ownedNodeWorker(host.pid,dirs.tmp,TIMER,workers);
      if(found){timerWorker=found;report.anchorStillPendingAtTimerObservation=!anchor.settled;report.probeStillActiveAtTimerObservation=null;report.pendingStatusesAtTimerObservation=report.statuses.filter(row=>!row.settled).length;}
      return found;
    }});
    timerWorker=progress.timer;observedProbe=progress.probe;report.timerObservedAtMs=progress.timerObservedAt-started;
    report.timerObservedAtMonotonicMs=progress.timerObservedAt;report.timerStartedWithinThreeSeconds=progress.timerObservedAt<timerDeadline;
    report.probeWorker=observer.witness;report.independentProbeHash=progress.hash;report.statusStillPolling=!stopStatus;
    report.timerWorker={pid:timerWorker.pid,birth:timerWorker.birth,scriptSha256:sha(fs.readFileSync(timerWorker.script)),argsSha256:sha(timerWorker.args),script:path.relative(dirs.tmp,timerWorker.script)};
    assertBeforeDeadline(timerDeadline,now,'timer script proof');
    report.deadlines.additionalBusy={startedAtMonotonicMs:progress.timerObservedAt,deadlineMonotonicMs:progress.busyDeadline,budgetMs:3000};
    mark('immutable anchor rechecks busy before returning; next status remains busy');
    await waitBusy({anchor,next:()=>report.statuses[anchor.index+1],finished:()=>report.running.settled,timerLive:()=>sameWorker(timerWorker),deadline:progress.busyDeadline,now});
    report.completedStatusWhileTimerActiveCount=report.statuses.filter(row=>row.settled).length;report.returnBusyHintResampled=true;
    report.runLeaseProgress={sourceBoundIsolatedInference:true,uniqueRun:true,anchorStatus:anchor.index,anchorTaskId:selected.proof.taskId,anchorBusy:true,anchorReady:false,notAuthorization:true,osProcessCoLivenessCertified:false};
    mark('in-use pack refuses real uninstall without changing resources');
    report.uninstallWhileRunning=await pack({action:'uninstall_start',id:'node'});assert.equal(report.uninstallWhileRunning.status,409);assert.equal(report.uninstallWhileRunning.body.error_code,'capability_install_busy');assert.deepEqual(files(directory,proof),before);
    mark('cancel and actual worker stop keep existing budgets');
    const cancelAt=performance.now();report.cancel=await api('/api/task/cancel',{id:task});assert.equal(report.cancel.body.state,'cancelling');const receipt=await running;report.cancelResponseMs=performance.now()-cancelAt;
    assert.equal(receipt.body.error_code,'execution_cancelled');assert.equal(receipt.body.stdout,'');assert.deepEqual(receipt.body.runtime,{id:'node',version:proof.manifest.version,manifest_sha256:proof.manifestSha256});assert(report.cancelResponseMs<200,'unchanged 200ms cancellation budget');
    const stopAt=performance.now();while(sameWorker(timerWorker)&&performance.now()-stopAt<2000)await pause(10);report.workerStopped=!sameWorker(timerWorker);report.observedStopMs=performance.now()-stopAt;assert(report.workerStopped&&report.observedStopMs<2000,'unchanged 2s stopping budget');
    stopStatus=true;await statusLoop;statusLoop=null;assert(report.statuses.every(r=>r.status===200&&!r.error));
    report.checks.push('status-first real probe/independent same-inode hash and immutable busy resampling admit actual timer; in-use uninstall refuses; fixed resources, 200ms cancel and 2s owned-worker stop');
    mark('failed real probe preparation cannot report ready or consume unknown data');
    const savedTmp=dirs.tmp+'-before-probe-failure';fs.renameSync(dirs.tmp,savedTmp);fs.writeFileSync(dirs.tmp,'Owned unknown temp obstruction\n',{flag:'wx'});tmpBlocked=true;
    report.failedProbe=await pack({action:'status',id:'node'});assert.equal(report.failedProbe.status,200);assert.equal(report.failedProbe.body.ready,false);assert.equal(fs.readFileSync(dirs.tmp,'utf8'),'Owned unknown temp obstruction\n');assert.deepEqual(files(directory,proof),before);
    fs.unlinkSync(dirs.tmp);fs.renameSync(savedTmp,dirs.tmp);tmpBlocked=false;
    report.recoveredProbe=await pack({action:'status',id:'node'});assert.equal(report.recoveredProbe.body.ready,true);assert.deepEqual(fs.readFileSync(sentinel),source);
    report.checks.push('valid installed bytes plus failed private probe preparation do not claim readiness; unknown obstruction survives; fresh real probe recovers');
    mark('real uninstall preserves owned source');report.uninstall=await pack({action:'uninstall_start',id:'node'});assert.equal(report.uninstall.status,200);report.removed=await settle(host,report.uninstall.body);assert.equal(report.removed.phase,'removed');installed=false;report.runtimeRemoved=true;assert.equal((await pack({action:'status',id:'node'})).body.ready,false);assert.deepEqual(fs.readFileSync(sentinel),source);report.passed=true;
  }catch(e){if(report.precondition?.outcome==='pending')report.precondition.outcome=e.code==='status_work_window_unavailable'?'inconclusive':'verification_failed';report.failure={phase,code:e.code||null,name:e.name,message:safe(e.message),stack:safe(e.stack||'').slice(0,6000)};process.exitCode=1;}
  finally{
    stopStatus=true;observedProbe??=observer?.worker;if(observer)try{sample();observer.close();report.observerDescriptorsClosed=observer.pinnedBytes()===0;}catch(e){report.cleanupErrors.push(safe(e.message));}
    if(statusLoop)try{await statusLoop;}catch(e){report.cleanupErrors.push(safe(e.message));}
    if(running&&host&&(!report.running.settled||Object.hasOwn(report.running,'error')))try{report.cleanupCancel=await api('/api/task/cancel',{id:task});await running;}catch(e){report.cleanupErrors.push(safe(e.message));}
    if(timerWorker)try{const until=Date.now()+65000;while(sameWorker(timerWorker)&&Date.now()<until)await pause(100);assert(!sameWorker(timerWorker),'owned timer still active; retain scratch');}catch(e){report.cleanupErrors.push(safe(e.message));}
    if(observedProbe)try{report.cleanupProbeStopped=!sameLive(observedProbe,host.pid);assert(report.cleanupProbeStopped,'identified real probe remains active; retain scratch');}catch(e){report.cleanupErrors.push(safe(e.message));}
    if(tmpBlocked)try{assert.equal(fs.readFileSync(dirs.tmp,'utf8'),'Owned unknown temp obstruction\n');fs.unlinkSync(dirs.tmp);fs.renameSync(dirs.tmp+'-before-probe-failure',dirs.tmp);}catch(e){report.cleanupErrors.push(safe(e.message));}
    if(host&&(installed||report.installAttempted&&!report.runtimeRemoved))try{const start=await pack({action:'uninstall_start',id:'node'});assert.equal(start.status,200);report.cleanupUninstall=await settle(host,start.body);assert.equal(report.cleanupUninstall.phase,'removed');report.runtimeRemoved=true;}catch(e){report.cleanupErrors.push(safe(e.message));}
    if(host)try{await host.stop();}catch(e){report.cleanupErrors.push(safe(e.message));}
    if(!report.cleanupErrors.length){try{noProbeCopies(dirs.tmp);fs.rmSync(dirs.tmp,{recursive:true});report.ownedTmpRemoved=true;}catch(e){report.cleanupErrors.push(safe(e.message));}}
    report.runRequested=report.runRequestDispatched===true;report.statusObservationWindows=report.statuses.map(row=>({statusIndex:row.index,requestedAtMs:row.requestedAtMs,settledAtMs:row.settledAtMs,workBarriers:report.probeObservations.filter(o=>o.statusIndex===row.index&&o.stage==='owned_header_created_barrier').length,liveWitnesses:report.probeObservations.filter(o=>o.statusIndex===row.index&&o.stage==='actual_owned_probe_witness').length,fullHashes:report.probeObservations.filter(o=>o.statusIndex===row.index&&o.stage==='independent_private_probe_hash').length}));
    clearInterval(sampler);sample();const retained=sampleStorage(args.evidence,observer);report.storage.retainedPathnameBytes=retained.pathnameBytes;report.storage.retainedFdSupplementBytes=retained.supplementBytes;report.storage.retainedBytes=retained.totalBytes;if(report.storage.peakObservedBytes>report.storage.transientBudgetBytes||report.storage.retainedBytes>report.storage.retainedBudgetBytes)report.cleanupErrors.push('owned storage budget exceeded');
    if(report.cleanupErrors.length){report.passed=false;process.exitCode=1;}fs.writeFileSync(path.join(args.evidence,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify(report));
  }
}
if(require.main===module)main().catch(e=>{console.error(e.message);process.exitCode=1;});
module.exports={main,PROBE,TIMER,StatusProbeObserver,prepareStatusFirst,observeRunProgress,waitBusy};
