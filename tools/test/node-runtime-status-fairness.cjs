'use strict';
// Genuine status probes and Run compete through the production service. OS
// observations inspect only this fixture's backend children and private files.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {options}=require('./v006-table-clipboard-kernel.cjs'),{startKernel}=require('./v006-refactor-host.cjs');
const {nodePack,installNodePack,api:request,settle,sha}=require('./node-runtime-fixture.cjs');
const {processes,sameWorker,ps}=require('./code-mcp-eof.cjs');
const {ownedNodeWorker}=require('./node-runtime-http-stdio.cjs');
const ROOT=path.resolve(__dirname,'../..'),pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const TIMER="setTimeout(()=>console.log('LATE'),60000)";
const PROBE="console.log(JSON.stringify({version:process.version,sum:[6,7].reduce((a,b)=>a*b,1),unicode:'中文😀',write:process.permission.has('fs.write')}))";
function size(directory){let n=0;try{for(const name of fs.readdirSync(directory)){const file=path.join(directory,name);try{const m=fs.lstatSync(file);n+=m.isDirectory()&&!m.isSymbolicLink()?size(file):m.size;}catch(e){if(e.code!=='ENOENT')throw e;}}}catch(e){if(e.code!=='ENOENT')throw e;}return n;}
function probeWorker(parent,tmp,proof,observe=()=>{}){
  const name=process.platform==='win32'?'node.exe':'node',entry=proof.manifest.files.find(f=>f.path==='bin/'+name);
  const scanAt=performance.now(),children=processes(parent),identityScanMs=performance.now()-scanAt;
  for(const candidate of children){
    if(!candidate.args?.includes('--permission --eval')||!candidate.args.endsWith(PROBE))continue;
    const match=candidate.args.match(/^(?:"([^"]+)"|(\S+))\s+--permission\s+--eval\s/),program=match?.[1]||match?.[2];if(!program)continue;
    const observation={pid:candidate.pid,birth:candidate.birth,argsSha256:sha(candidate.args),identityScanMs,program:path.relative(tmp,program)};
    observe({...observation,stage:'candidate_seen'});
    if(!candidate.birth){observe({...observation,stage:'birth_not_observable'});continue;}
    try{
      const m=fs.lstatSync(program),canonical=fs.realpathSync(program),prefix=fs.realpathSync(tmp)+path.sep;
      assert(m.isFile()&&!m.isSymbolicLink());assert(process.platform==='win32'?canonical.toLowerCase().startsWith(prefix.toLowerCase()):canonical.startsWith(prefix));
      assert.equal(m.size,entry.size);const hashAt=performance.now();observation.programSha256=sha(fs.readFileSync(program));observation.hashMs=performance.now()-hashAt;
      assert.equal(observation.programSha256,entry.sha256);assert(candidate.birth);
      const groupAt=performance.now();
      if(process.platform!=='win32'){
        const group=ps(candidate.pid,'pgid');
        if(!group){observe({...observation,stage:'group_not_observable',groupReadMs:performance.now()-groupAt});continue;}
        assert.equal(Number(group),candidate.pid);
      }
      observation.groupReadMs=performance.now()-groupAt;observe({...observation,stage:'private_probe_verified'});
      return{...candidate,program};
    }catch(e){observe({...observation,stage:e.code==='ENOENT'?'copy_retired_before_verification':'verification_failed',error:e.code||e.name});if(e.code!=='ENOENT')throw e;}
  }
  return null;
}
// Retry only read-only preparation, before a Run exists. A verified copy can
// finish while ps/hash/identity observations are in flight. Every lost window
// remains evidence, and exhausting this bound is an inconclusive failure.
async function observableProbe({find,live,pending,record,now=()=>performance.now(),sleep=pause}){
  const end=now()+6000;let candidates=0;
  while(now()<end&&candidates<6){
    const worker=find(),anchor=pending();
    if(!worker){record({stage:'no_verified_probe_observed'});await sleep(10);continue;}
    candidates++;const recheckAt=now(),alive=Boolean(live(worker)),elapsedMs=now()-recheckAt;
    const accepted=Boolean(anchor&&!anchor.settled&&alive&&now()<end);
    record({stage:'precondition_identity_recheck',candidate:candidates,pid:worker.pid,birth:worker.birth,argsSha256:sha(worker.args),statusIndex:anchor?.index??null,identityConfirmed:alive,identityRecheckMs:elapsedMs,anchorPending:Boolean(anchor&&!anchor.settled),preparationDeadlineReached:now()>=end,accepted});
    if(accepted)return{worker,anchor,candidates};
    await sleep(10);
  }
  const error=Error('No stable genuine probe window within the original 6s preparation bound; Run was not requested');
  error.code='probe_observation_window_unavailable';throw error;
}
function files(directory,proof){return proof.manifest.files.map(f=>{const p=path.join(directory,...f.path.split('/')),m=fs.lstatSync(p);assert(m.isFile()&&!m.isSymbolicLink());return{path:f.path,size:m.size,sha256:sha(fs.readFileSync(p))};});}
function noProbeCopies(directory){for(const name of fs.readdirSync(directory)){const file=path.join(directory,name),m=fs.lstatSync(file);assert(!m.isSymbolicLink(),'retain unexpected link in owned scratch');if(m.isDirectory())noProbeCopies(file);else assert(!['node','node.exe'].includes(name),'retain a probe copy whose retirement is not confirmed');}}
async function main(){
  const args=options(process.argv.slice(2));assert(args.bin&&!args.base);const proof=nodePack();
  const dirs=Object.fromEntries(['profile','documents','tmp'].map(n=>[n,path.join(args.evidence,n)]));for(const d of Object.values(dirs))fs.mkdirSync(d);
  Object.assign(process.env,{TMPDIR:dirs.tmp,TMP:dirs.tmp,TEMP:dirs.tmp});
  const sentinel=path.join(dirs.documents,'owned-sentinel.md'),source=Buffer.from('# Synthetic Node status fairness\r\n\r\nNo model or user file.\r\n');fs.writeFileSync(sentinel,source);
  const report={passed:false,platform:process.platform,arch:process.arch,modelsRequested:false,nativeUiCertified:false,binarySha256:sha(fs.readFileSync(args.bin)),bootSha256:sha(fs.readFileSync(path.join(ROOT,'assets/readmd.boot.js'))),driverSha256:sha(fs.readFileSync(__filename)),packSha256:proof.sha256,manifestSha256:proof.manifestSha256,timerSha256:sha(TIMER),probeSourceSha256:sha(PROBE),checks:[],statuses:[],probeObservations:[],probeSchedulingSignalsUsed:false,cleanupErrors:[]};
  const started=performance.now(),at=()=>Math.round((performance.now()-started)*1000)/1000,safe=s=>String(s).replaceAll(args.evidence,'<evidence>').replaceAll(ROOT,'<repo>');
  let host,task,running,timerWorker,observedProbe,statusLoop,stopStatus=false,phase='startup',installed=false,tmpBlocked=false;
  const mark=value=>{phase=value;console.log(JSON.stringify({phase,atMs:at()}));};
  const api=(route,body)=>request(host,route,body),pack=body=>api('/api/capabilities/packs',body);
  const directory=path.join(dirs.profile,'capabilities/managed-v2/node',proof.manifest.version);
  report.storage={peakObservedBytes:0,transientBudgetBytes:450*1048576,retainedBudgetBytes:250*1048576};const sample=()=>{report.storage.peakObservedBytes=Math.max(report.storage.peakObservedBytes,size(args.evidence));};const sampler=setInterval(sample,50);
  try{
    mark('real authenticated offline install');host=await startKernel(args.bin,dirs.profile,dirs.documents);report.hostPid=host.pid;report.installAttempted=true;report.installed=await installNodePack(host,proof);installed=true;
    const before=files(directory,proof);report.installedFiles=before;
    mark('continuous real status begins before Run');
    statusLoop=(async()=>{while(!stopStatus){const row={index:report.statuses.length,requestedAtMs:at(),settled:false};report.statuses.push(row);try{const result=await pack({action:'status',id:'node'});Object.assign(row,{settled:true,settledAtMs:at(),...result});}catch(e){Object.assign(row,{settled:true,settledAtMs:at(),error:safe(e.message)});throw e;}await pause(10);}})();statusLoop.catch(()=>{});
    report.precondition={mode:'bounded read-only natural-window observation',budgetMs:6000,maxVerifiedCandidates:6,outcome:'pending'};
    const pending=()=>report.statuses.find(row=>!row.settled),record=value=>report.probeObservations.push({atMs:at(),statusIndex:pending()?.index??null,...value});
    const selected=await observableProbe({find:()=>probeWorker(host.pid,dirs.tmp,proof,record),live:sameWorker,pending,record});
    observedProbe=selected.worker;const anchor=selected.anchor;
    Object.assign(report.precondition,{outcome:'stable_probe_observed',verifiedCandidates:selected.candidates,acceptedAtMs:at()});
    report.probeWorker={pid:observedProbe.pid,birth:observedProbe.birth,argsSha256:sha(observedProbe.args),program:path.relative(dirs.tmp,observedProbe.program),programSha256:proof.manifest.files.find(f=>f.path.startsWith('bin/')).sha256,observedAtMs:at()};
    report.anchorStatus=anchor.index;
    mark('timer must start during continuous real status');task=crypto.randomUUID();report.running={taskId:task,requestedAtMs:at(),settled:false};
    running=api('/api/code/run',{confirm:true,lang:'javascript',code:TIMER,task_id:task}).then(receipt=>{Object.assign(report.running,{settled:true,settledAtMs:at(),receipt});return receipt;},e=>{Object.assign(report.running,{settled:true,settledAtMs:at(),error:safe(e.message)});throw e;});running.catch(()=>{});
    const deadline=Date.now()+3000;
    while(Date.now()<deadline&&!report.running.settled){timerWorker=ownedNodeWorker(host.pid,dirs.tmp,TIMER);if(timerWorker)break;await pause(10);}
    report.timerObservedAtMs=at();report.timerStartedWithinThreeSeconds=!!timerWorker;report.anchorStillPendingAtTimerObservation=!anchor.settled;report.probeStillActiveAtTimerObservation=sameWorker(observedProbe);report.statusStillPolling=!stopStatus;report.pendingStatusesAtTimerObservation=report.statuses.filter(r=>!r.settled).length;
    assert(timerWorker,'continuous genuine status must not prevent the actual timer from starting within 3 seconds');
    report.timerWorker={pid:timerWorker.pid,birth:timerWorker.birth,scriptSha256:sha(fs.readFileSync(timerWorker.script)),argsSha256:sha(timerWorker.args),script:path.relative(dirs.tmp,timerWorker.script)};
    assert(report.anchorStillPendingAtTimerObservation&&report.probeStillActiveAtTimerObservation,'the actual timer must start while its independently identified status probe is still running');
    mark('status rechecks busy before returning while the timer stays active');
    const busyDeadline=Date.now()+3000;
    while(Date.now()<busyDeadline&&(!anchor.settled||!report.statuses[anchor.index+1]?.settled))await pause(10);
    assert.equal(report.running.settled,false,'the timer remains active through repeated real status responses');
    const next=report.statuses[anchor.index+1];assert(anchor.settled&&next?.settled,'original 3s busy observation window');
    assert.equal(anchor.body.busy,true,'the initial probe resamples the Run lease before returning');assert.equal(anchor.body.ready,false);
    assert.equal(next.body.busy,true,'subsequent status remains busy during the actual timer');assert.equal(next.body.ready,false);
    report.completedStatusWhileTimerActiveCount=report.statuses.filter(row=>row.settled).length;report.returnBusyHintResampled=true;assert(sameWorker(timerWorker));
    mark('in-use pack refuses real uninstall without changing resources');
    report.uninstallWhileRunning=await pack({action:'uninstall_start',id:'node'});assert.equal(report.uninstallWhileRunning.status,409);assert.equal(report.uninstallWhileRunning.body.error_code,'capability_install_busy');assert.deepEqual(files(directory,proof),before);
    mark('cancel and actual worker stop keep existing budgets');
    const cancelAt=performance.now();report.cancel=await api('/api/task/cancel',{id:task});assert.equal(report.cancel.body.state,'cancelling');const receipt=await running;report.cancelResponseMs=performance.now()-cancelAt;
    assert.equal(receipt.body.error_code,'execution_cancelled');assert.equal(receipt.body.stdout,'');assert.deepEqual(receipt.body.runtime,{id:'node',version:proof.manifest.version,manifest_sha256:proof.manifestSha256});assert(report.cancelResponseMs<200,'unchanged 200ms cancellation budget');
    const stopAt=performance.now();while(sameWorker(timerWorker)&&performance.now()-stopAt<2000)await pause(10);report.workerStopped=!sameWorker(timerWorker);report.observedStopMs=performance.now()-stopAt;assert(report.workerStopped&&report.observedStopMs<2000,'unchanged 2s stopping budget');
    stopStatus=true;await statusLoop;statusLoop=null;assert(report.statuses.every(r=>r.status===200&&!r.error));
    report.checks.push('actual timer starts under continuous actual probes; in-use uninstall refuses; fixed resources, 200ms cancel and 2s owned-worker stop');
    mark('failed real probe preparation cannot report ready or consume unknown data');
    const savedTmp=dirs.tmp+'-before-probe-failure';fs.renameSync(dirs.tmp,savedTmp);fs.writeFileSync(dirs.tmp,'Owned unknown temp obstruction\n',{flag:'wx'});tmpBlocked=true;
    report.failedProbe=await pack({action:'status',id:'node'});assert.equal(report.failedProbe.status,200);assert.equal(report.failedProbe.body.ready,false);assert.equal(fs.readFileSync(dirs.tmp,'utf8'),'Owned unknown temp obstruction\n');assert.deepEqual(files(directory,proof),before);
    fs.unlinkSync(dirs.tmp);fs.renameSync(savedTmp,dirs.tmp);tmpBlocked=false;
    report.recoveredProbe=await pack({action:'status',id:'node'});assert.equal(report.recoveredProbe.body.ready,true);assert.deepEqual(fs.readFileSync(sentinel),source);
    report.checks.push('valid installed bytes plus failed private probe preparation do not claim readiness; unknown obstruction survives; fresh real probe recovers');
    mark('real uninstall preserves owned source');report.uninstall=await pack({action:'uninstall_start',id:'node'});assert.equal(report.uninstall.status,200);report.removed=await settle(host,report.uninstall.body);assert.equal(report.removed.phase,'removed');installed=false;report.runtimeRemoved=true;assert.equal((await pack({action:'status',id:'node'})).body.ready,false);assert.deepEqual(fs.readFileSync(sentinel),source);report.passed=true;
  }catch(e){if(report.precondition?.outcome==='pending')report.precondition.outcome=e.code==='probe_observation_window_unavailable'?'inconclusive':'verification_failed';report.failure={phase,code:e.code||null,name:e.name,message:safe(e.message),stack:safe(e.stack||'').slice(0,6000)};process.exitCode=1;}
  finally{
    stopStatus=true;if(statusLoop)try{await statusLoop;}catch(e){report.cleanupErrors.push(safe(e.message));}
    if(running&&!report.running.settled&&host)try{report.cleanupCancel=await api('/api/task/cancel',{id:task});await running;}catch(e){report.cleanupErrors.push(safe(e.message));}
    if(timerWorker)try{const until=Date.now()+65000;while(sameWorker(timerWorker)&&Date.now()<until)await pause(100);assert(!sameWorker(timerWorker),'owned timer still active; retain scratch');}catch(e){report.cleanupErrors.push(safe(e.message));}
    if(observedProbe)try{report.cleanupProbeStopped=!sameWorker(observedProbe);assert(report.cleanupProbeStopped,'identified real probe remains active; retain scratch');}catch(e){report.cleanupErrors.push(safe(e.message));}
    if(tmpBlocked)try{assert.equal(fs.readFileSync(dirs.tmp,'utf8'),'Owned unknown temp obstruction\n');fs.unlinkSync(dirs.tmp);fs.renameSync(dirs.tmp+'-before-probe-failure',dirs.tmp);}catch(e){report.cleanupErrors.push(safe(e.message));}
    if(host&&(installed||report.installAttempted&&!report.runtimeRemoved))try{const start=await pack({action:'uninstall_start',id:'node'});assert.equal(start.status,200);report.cleanupUninstall=await settle(host,start.body);assert.equal(report.cleanupUninstall.phase,'removed');report.runtimeRemoved=true;}catch(e){report.cleanupErrors.push(safe(e.message));}
    if(host)try{await host.stop();}catch(e){report.cleanupErrors.push(safe(e.message));}
    if(!report.cleanupErrors.length){try{noProbeCopies(dirs.tmp);fs.rmSync(dirs.tmp,{recursive:true});report.ownedTmpRemoved=true;}catch(e){report.cleanupErrors.push(safe(e.message));}}
    report.runRequested=Boolean(report.running);report.statusObservationWindows=report.statuses.map(row=>({statusIndex:row.index,requestedAtMs:row.requestedAtMs,settledAtMs:row.settledAtMs,candidates:report.probeObservations.filter(o=>o.statusIndex===row.index&&o.stage==='candidate_seen').length,verified:report.probeObservations.filter(o=>o.statusIndex===row.index&&o.stage==='private_probe_verified').length,rechecks:report.probeObservations.filter(o=>o.statusIndex===row.index&&o.stage==='precondition_identity_recheck')}));
    clearInterval(sampler);sample();report.storage.retainedBytes=size(args.evidence);if(report.storage.peakObservedBytes>report.storage.transientBudgetBytes||report.storage.retainedBytes>report.storage.retainedBudgetBytes)report.cleanupErrors.push('owned storage budget exceeded');
    if(report.cleanupErrors.length){report.passed=false;process.exitCode=1;}fs.writeFileSync(path.join(args.evidence,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify(report));
  }
}
if(require.main===module)main().catch(e=>{console.error(e.message);process.exitCode=1;});
module.exports={main,probeWorker,observableProbe};
