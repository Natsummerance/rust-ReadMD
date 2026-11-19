'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const {StatusProbeObserver,prepareStatusFirst,observeRunProgress,waitBusy,journal,json,parseSnapshots,snapshots,sameLive,sampleStorage}=require('./node-runtime-status-observer.cjs');
const PROBE="console.log(JSON.stringify({version:process.version,sum:[6,7].reduce((a,b)=>a*b,1),unicode:'中文😀',write:process.permission.has('fs.write')}))";
const ROOT=path.resolve(__dirname,'../..'),SCRATCH=process.env.READMD_TEST_STATUS_OBSERVER_TMP||path.join(ROOT,'.cache/node-status-observer-tests');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex'),id=()=>crypto.randomUUID().replaceAll('-','');
const serial=value=>JSON.stringify(value,(_,v)=>typeof v==='bigint'?JSON.rawJSON(String(v)):v)+'\n';
const identity=file=>{const m=fs.statSync(file,{bigint:true});return {volume:m.dev,index:m.ino,created:m.birthtimeNs};};
const posix=['darwin','linux'].includes(process.platform);
function fixture(t,{baseline=false,payload=Buffer.from('Owned synthetic executable bytes\n')}={}){
  fs.mkdirSync(SCRATCH,{recursive:true,mode:0o700});const root=fs.mkdtempSync(path.join(SCRATCH,'owned-'));
  const profile=path.join(root,'profile'),tmp=path.join(root,'tmp');for(const d of [profile,tmp])fs.mkdirSync(d,{mode:0o700});
  const journals=path.join(profile,'worker-tasks-v1');fs.mkdirSync(journals,{mode:0o700});
  const profileHash=sha(Buffer.from(fs.realpathSync(profile))),workParent=path.join(tmp,'rmdw-'+profileHash.slice(0,12));fs.mkdirSync(workParent,{mode:0o700});
  let rows=[],observer;const f={root,profile,tmp,journals,workParent,payload,records:[],tasks:[],setRows:v=>{rows=v;}};
  f.task=({purpose='node-probe',bytes=payload,mode=0o700}={})=>{
    const uuid=id(),dir=path.join(journals,uuid),work=path.join(workParent,uuid);fs.mkdirSync(dir,{mode:0o700});fs.mkdirSync(work,{mode:0o700});
    const program=path.join(work,'node'),record=path.join(dir,'record.jsonl'),owner=path.join(dir,'owner.lock');fs.writeFileSync(program,bytes,{mode});fs.chmodSync(program,mode);fs.writeFileSync(owner,'',{mode:0o600});
    const header={type:'Header',value:{schema:1,id:uuid,profile:profileHash,root:work,identity:identity(work),purpose,reserved_bytes:256*1024*1024}};
    const created={type:'Created',path:'node',identity:identity(program),kind:'File',target:null};
    fs.writeFileSync(record,serial(header)+serial(created),{mode:0o600});
    const task={uuid,dir,work,program,record,owner,header,created,append:event=>fs.appendFileSync(record,serial(event))};
    task.row={pid:4201,ppid:3141,pgid:4201,state:'S',birth:'Thu Nov  5 03:10:20 2026',args:program+' --permission --eval '+PROBE};
    task.intent=()=>({type:'Retiring',value:{directory:identity(dir),journal:identity(record),lease:identity(owner)}});
    task.retire=()=>{task.append({type:'Finished'});const intent=task.intent();fs.unlinkSync(program);fs.rmdirSync(work);task.append(intent);};
    f.tasks.push(task);return task;
  };
  if(baseline)f.task();
  observer=new StatusProbeObserver({profile,tmp,hostPid:3141,expected:{size:payload.length,sha256:sha(payload)},probeSource:PROBE,record:r=>f.records.push(r),readSnapshots:()=>rows});f.observer=observer;
  t.after(()=>{observer.close();fs.rmSync(root,{recursive:true});});return f;
}
function selected(t,options){const f=fixture(t,options),task=f.task();f.anchor={index:0,settled:false};assert(f.observer.capture(f.anchor));f.setRows([task.row]);assert(f.observer.observeLive());return {...f,task};}
function closed(fd){assert.throws(()=>fs.fstatSync(fd),{code:'EBADF'});}

test('fixed probe is the unchanged permission/eval JSON, not a version-only command',()=>{
  const driver=require('./node-runtime-status-fairness.cjs');assert.equal(driver.PROBE,PROBE);assert.equal(sha(PROBE),'6c7502c8e2f3771ebe33c45fdcede07ee60c9a341f272ce32419744d84bec83b');
});
test('journal preserves u64 values above the Number precision limit',()=>{assert.equal(json(Buffer.from('{"n":1793823812345678901}')).n,1793823812345678901n);assert.throws(()=>json(Buffer.from('{"n":1.5}')));});
test('owned process row requires all fields and preserves full eval argv',()=>{
  const row=parseSnapshots(' 4201 3141 4201 S Thu Nov  5 03:10:20 2026 /owned/node --permission --eval '+PROBE)[0];assert.equal(row.ppid,3141);assert.equal(row.pgid,4201);assert.equal(row.birth,'Thu Nov  5 03:10:20 2026');assert.equal(row.args,'/owned/node --permission --eval '+PROBE);assert.throws(()=>parseSnapshots('4201 3141 incomplete'));
});
test('actual ps reads only this test process and supplies parent/birth/group/state/argv',{skip:!posix},()=>{
  const rows=snapshots(process.ppid,process.pid);assert.equal(rows.length,1);const row=rows[0];assert.equal(row.pid,process.pid);assert.equal(row.ppid,process.ppid);assert(row.birth&&row.args&&row.state&&row.pgid>0);
});
test('same live identity rejects changed parent, group, birth, argv and zombie',()=>{
  const worker={pid:42,ppid:10,pgid:42,birth:'owned birth',args:'fixed',state:'S'};assert(sameLive(worker,10,()=>[worker]));
  for(const patch of [{ppid:11},{pgid:41},{birth:'new birth'},{birth:''},{args:'other'},{state:'Z'},{state:undefined}])assert(!sameLive(worker,10,()=>[{...worker,...patch}]));
});
test('Header+Created can prove copy preparation while bytes are incomplete; never execution',{skip:!posix},t=>{
  const f=fixture(t),task=f.task({bytes:Buffer.alloc(0),mode:0o600});const barrier=f.observer.capture({index:0,settled:false});assert.equal(barrier.copyPreparationOnly,true);assert.equal(f.observer.worker,undefined);assert.equal(f.observer.pinnedBytes(),f.payload.length);f.setRows([task.row]);assert.throws(()=>f.observer.observeLive());
});
test('complete owned identity then actual unlink permits an exact stable full digest',{skip:!posix},async t=>{
  const f=selected(t),binary=f.observer.selected.binaryFd,record=f.observer.selected.journalFd;assert(!f.observer.retirementReady());f.task.retire();assert(f.observer.retirementReady());
  const proof=await f.observer.hashRetired(performance.now()+3000);assert.equal(proof.bytes,f.payload.length);assert.equal(proof.sha256,sha(f.payload));assert.equal(proof.binaryFdActuallyUnlinked,true);assert.equal(proof.stableDigestIntervalCertified,true);assert.equal(proof.finishedIsProbeSuccess,false);assert.equal(proof.hiddenLiveToRetirementMutationExcluded,false);assert.equal(f.observer.pinnedBytes(),0);closed(binary);closed(record);
});
test('retired original journal remains observable through its pinned inode',{skip:!posix},async t=>{
  const f=selected(t);f.task.retire();const witness=path.join(f.journals,'owned-wal');fs.linkSync(f.task.record,witness);fs.unlinkSync(f.task.record);fs.unlinkSync(witness);
  assert(f.observer.retirementReady());assert.equal((await f.observer.hashRetired(performance.now()+3000)).sha256,sha(f.payload));
});
test('independent digest reads the complete file across the one-MiB chunk boundary',{skip:!posix},async t=>{
  const payload=Buffer.concat([Buffer.alloc(1024*1024,0x31),Buffer.from('independent final chunk')]),f=selected(t,{payload}),offsets=[];f.task.retire();
  const proof=await f.observer.hashRetired(performance.now()+3000,{afterChunk:offset=>offsets.push(offset)});assert.deepEqual(offsets,[1024*1024,payload.length]);assert.equal(proof.bytes,payload.length);assert.equal(proof.sha256,sha(payload));
});
test('corruption beyond the first digest chunk cannot use a valid prefix hash',{skip:!posix},async t=>{
  const payload=Buffer.concat([Buffer.alloc(1024*1024,0x31),Buffer.from('known final chunk')]),f=fixture(t,{payload}),changed=Buffer.from(payload);changed[changed.length-1]^=1;
  const task=f.task({bytes:changed});f.observer.capture({index:0,settled:false});f.setRows([task.row]);f.observer.observeLive();task.retire();await assert.rejects(f.observer.hashRetired(performance.now()+3000),/digest/);
});
test('Finished alone and Finished+Retiring alone do not prove per-file unlink',{skip:!posix},t=>{
  const f=selected(t);f.task.append({type:'Finished'});assert(!f.observer.retirementReady());f.task.append(f.task.intent());assert(!f.observer.retirementReady());assert.equal(fs.statSync(f.task.program).nlink,1);
});
test('existing baseline tasks cannot become a new status anchor',{skip:!posix},t=>{const f=fixture(t,{baseline:true});assert.equal(f.observer.capture({index:0,settled:false}),null);});
test('an incomplete journal line is not a Created event',{skip:!posix},t=>{
  const f=fixture(t),task=f.task();fs.writeFileSync(task.record,serial(task.header)+serial(task.created).trimEnd());assert.equal(f.observer.capture({index:0,settled:false}),null);fs.appendFileSync(task.record,'\n');assert(f.observer.capture({index:0,settled:false}));
});
test('two valid new tasks are ambiguous, not a choice of convenient later anchor',{skip:!posix},t=>{const f=fixture(t);f.task();f.task();assert.throws(()=>f.observer.capture({index:0,settled:false}),/ambiguous/);assert.equal(f.observer.pinnedBytes(),0);});
for(const field of ['profile','root','id'])test('wrong Header '+field+' fails ownership before Run',{skip:!posix},t=>{
  const f=fixture(t),task=f.task();task.header.value[field]=field==='profile'?'f'.repeat(64):field==='id'?id():path.join(f.root,'unowned');fs.writeFileSync(task.record,serial(task.header)+serial(task.created));assert.throws(()=>f.observer.capture({index:0,settled:false}));assert.equal(f.observer.pinnedBytes(),0);
});
test('same pathname with a replacement inode cannot match Created',{skip:!posix},t=>{
  const f=fixture(t),task=f.task(),held=fs.openSync(task.program,'r');t.after(()=>fs.closeSync(held));fs.unlinkSync(task.program);fs.writeFileSync(task.program,f.payload,{mode:0o700});assert.throws(()=>f.observer.capture({index:0,settled:false}),/Created inode/);assert.equal(f.observer.pinnedBytes(),0);
});
test('a symbolic executable never becomes a pinned program',{skip:!posix},t=>{
  const f=fixture(t),task=f.task(),unknown=path.join(f.root,'unknown');fs.writeFileSync(unknown,'PRESERVE');fs.unlinkSync(task.program);fs.symlinkSync(unknown,task.program);assert.throws(()=>f.observer.capture({index:0,settled:false}));assert.equal(fs.readFileSync(unknown,'utf8'),'PRESERVE');
});
test('a redirected task-root ancestor fails before opening its executable',{skip:!posix},t=>{
  const f=fixture(t),task=f.task(),moved=path.join(f.root,'owned-moved-task');fs.renameSync(task.work,moved);fs.symlinkSync(moved,task.work);assert.throws(()=>f.observer.capture({index:0,settled:false}));assert.equal(fs.readFileSync(path.join(moved,'node')).compare(f.payload),0);
});
for(const [label,patch] of [['parent',{ppid:999}],['group',{pgid:999}],['zombie',{state:'Z'}]])test('wrong live '+label+' cannot upgrade preparation',{skip:!posix},t=>{
  const f=fixture(t),task=f.task();f.observer.capture({index:0,settled:false});f.setRows([{...task.row,...patch}]);assert.throws(()=>f.observer.observeLive());assert.equal(f.observer.worker,undefined);
});
test('changed eval argv is not a genuine fixed probe',{skip:!posix},t=>{const f=fixture(t),task=f.task();f.observer.capture({index:0,settled:false});f.setRows([{...task.row,args:task.row.args+';changed'}]);assert.equal(f.observer.observeLive(),null);});
test('the anchor cannot be replaced after it is selected',{skip:!posix},t=>{const f=selected(t);assert.throws(()=>f.observer.capture({index:1,settled:false}),/cannot be replaced/);});
test('rewriting an already-read journal prefix fails',{skip:!posix},t=>{const f=selected(t);fs.writeFileSync(f.task.record,serial({...f.task.header,value:{...f.task.header.value,purpose:'changed'}})+serial(f.task.created));assert.throws(()=>f.observer.journalState(),/append-only/);});
test('Retiring must bind the actual pinned journal and lease identities',{skip:!posix},t=>{
  const f=selected(t);f.task.append({type:'Finished'});const intent=f.task.intent();intent.value.lease.index+=1n;fs.unlinkSync(f.task.program);f.task.append(intent);assert.throws(()=>f.observer.retirementReady());
});
test('wrong independent bytes fail and close both descriptors',{skip:!posix},async t=>{
  const f=selected(t),binary=f.observer.selected.binaryFd,record=f.observer.selected.journalFd;f.observer.expected.sha256='0'.repeat(64);f.task.retire();await assert.rejects(f.observer.hashRetired(performance.now()+3000),/digest/);closed(binary);closed(record);
});
test('ctime-only mutation during a full digest is rejected with no exception',{skip:!posix},async t=>{
  const f=selected(t),writer=fs.openSync(f.task.program,'r+');t.after(()=>fs.closeSync(writer));f.task.retire();
  await assert.rejects(f.observer.hashRetired(performance.now()+3000,{afterChunk(){fs.fchmodSync(writer,0o600);fs.fchmodSync(writer,0o700);}}),/metadata changed/);assert.equal(f.observer.pinnedBytes(),0);
});
test('modified then restored bytes during hash do not certify immutability',{skip:!posix},async t=>{
  const f=selected(t),writer=fs.openSync(f.task.program,'r+');t.after(()=>fs.closeSync(writer));f.task.retire();
  await assert.rejects(f.observer.hashRetired(performance.now()+3000,{afterChunk(){fs.writeSync(writer,Buffer.from('X'),0,1,0);fs.writeSync(writer,f.payload.subarray(0,1),0,1,0);}}),/metadata changed/);assert.equal(f.observer.pinnedBytes(),0);
});
test('a full hash finishing at3101ms fails and closes its pinned descriptors',{skip:!posix},async t=>{
  const f=selected(t);f.task.retire();let clock=0;f.observer.now=()=>clock;const binary=f.observer.selected.binaryFd,record=f.observer.selected.journalFd;
  await assert.rejects(f.observer.hashRetired(3000,{afterChunk(){clock=3101;}}),{code:'status_observation_deadline'});closed(binary);closed(record);
});
test('close on an abandoned preparation is idempotent and releases reservation',{skip:!posix},t=>{const f=selected(t),binary=f.observer.selected.binaryFd,record=f.observer.selected.journalFd;f.observer.close();f.observer.close();closed(binary);closed(record);assert.equal(f.observer.pinnedBytes(),0);});
test('storage counts a linked pinned inode once, not another expected-size copy',{skip:!posix},t=>{
  const f=selected(t),value=sampleStorage(f.root,f.observer);assert.equal(value.linkedFdBytes,f.payload.length);assert.equal(value.scannedPinnedBytes,f.payload.length);assert.equal(value.supplementBytes,0);assert.equal(value.totalBytes,value.pathnameBytes);
});
test('storage supplements an actually unlinked open inode absent from paths',{skip:!posix},t=>{
  const f=selected(t);f.task.retire();const value=sampleStorage(f.root,f.observer);assert.equal(value.unlinkedOpenBytes,f.payload.length);assert.equal(value.scannedPinnedBytes,0);assert.equal(value.supplementBytes,f.payload.length);assert.equal(value.totalBytes,value.pathnameBytes+f.payload.length);
});
test('scan then actual unlink never subtracts or double-counts its seen inode',{skip:!posix},t=>{
  const f=selected(t),value=sampleStorage(f.root,f.observer,{afterScan:()=>f.task.retire()});assert.equal(value.scannedPinnedBytes,f.payload.length);assert.equal(value.unlinkedOpenBytes,f.payload.length);assert.equal(value.supplementBytes,0);assert.equal(value.totalBytes,value.pathnameBytes);
});
test('scan-missed quarantine rename still supplements a linked original inode',{skip:!posix},t=>{
  const f=selected(t);let moved=false;
  const value=sampleStorage(f.root,f.observer,{afterDirectoryRead(dir){if(dir===f.task.work&&!moved){moved=true;const quarantine=path.join(f.workParent,'owned-quarantine');fs.mkdirSync(quarantine,{mode:0o700});fs.renameSync(f.task.program,path.join(quarantine,'node'));}}});
  assert(moved);assert.equal(value.scannedPinnedBytes,0);assert.equal(value.linkedFdBytes,f.payload.length);assert.equal(value.supplementBytes,f.payload.length);assert.equal(value.totalBytes,value.pathnameBytes+f.payload.length);assert.equal(value.physicalAllocationOrAllTimePeakCertified,false);
});
test('an uncertain close cannot certify closed descriptors or zero retained bytes',{skip:!posix},t=>{
  const f=selected(t),binary=f.observer.selected.binaryFd,record=f.observer.selected.journalFd,original=fs.closeSync;
  try{fs.closeSync=fd=>{original(fd);if(fd===binary)throw Error('synthetic close uncertainty after real close');};assert.throws(()=>f.observer.close(),/uncertainty/);}finally{fs.closeSync=original;}
  assert.equal(f.observer.pinnedBytes(),f.payload.length);assert.throws(()=>f.observer.close(),{code:'status_observer_close_uncertain'});closed(binary);closed(record);
  // The test independently confirmed both real closes before allowing its
  // own exact scratch cleanup. Production never clears an uncertain close.
  f.observer.closeUncertain=false;
});

function fakeObserver(){return {worker:null,hashProof:null,close(){this.closed=true;},observeLive(){this.worker={pid:42};},retirementReady(){return true;},async hashRetired(){this.hashProof={sha256:'synthetic-only'};}};}
test('timer observer returning at3101ms cannot pass even with a valid worker',async()=>{
  let clock=0;const observer=fakeObserver();await assert.rejects(observeRunProgress({observer,now:()=>clock,deadline:3000,finished:()=>false,findTimer(){clock=3101;return {pid:43};}}),{code:'status_observation_deadline'});
});
test('probe identity observer returning at3101ms cannot pass',async()=>{
  let clock=0;const observer=fakeObserver();observer.observeLive=function(){clock=3101;this.worker={pid:42};};await assert.rejects(observeRunProgress({observer,now:()=>clock,deadline:3000,finished:()=>false,findTimer:()=>({pid:43})}),{code:'status_observation_deadline'});
});
test('hash proof returning at3101ms cannot pass',async()=>{
  let clock=0;const observer=fakeObserver();observer.hashRetired=async function(){clock=3101;this.hashProof={};};await assert.rejects(observeRunProgress({observer,now:()=>clock,deadline:3000,finished:()=>false,findTimer:()=>({pid:43})}),{code:'status_observation_deadline'});
});
test('work barrier crossing6000ms cannot authorize Run',async()=>{
  let clock=0;const observer={capture(){clock=6001;return {taskId:'synthetic'};}};await assert.rejects(prepareStatusFirst({observer,pending:()=>({index:0,settled:false}),now:()=>clock,deadline:6000}),{code:'status_observation_deadline'});
});
test('a real pinned barrier crossing the preparation deadline closes both fds',{skip:!posix},async t=>{
  const f=fixture(t);f.task();const original=f.observer.capture.bind(f.observer);let clock=0,binary,record;
  f.observer.capture=anchor=>{const proof=original(anchor);binary=f.observer.selected.binaryFd;record=f.observer.selected.journalFd;clock=6001;return proof;};
  await assert.rejects(prepareStatusFirst({observer:f.observer,pending:()=>({index:0,settled:false}),now:()=>clock,deadline:6000}),{code:'status_observation_deadline'});closed(binary);closed(record);
});
test('a Run proof failure also closes an actual pinned observer',{skip:!posix},async t=>{
  const f=selected(t),binary=f.observer.selected.binaryFd,record=f.observer.selected.journalFd;let clock=0;
  await assert.rejects(observeRunProgress({observer:f.observer,now:()=>clock,deadline:3000,finished:()=>false,findTimer(){clock=3101;return {pid:43};}}),{code:'status_observation_deadline'});closed(binary);closed(record);
});
test('HTTP pending alone never supplies a status-work barrier',async()=>{
  let clock=0;await assert.rejects(prepareStatusFirst({observer:{capture:()=>null},pending:()=>({settled:false}),now:()=>clock,sleep:async()=>{clock+=1000;},deadline:6000}),{code:'status_work_window_unavailable'});
});
test('timer and additional busy windows have separate fixed monotonic origins',async()=>{
  let clock=0;const observer=fakeObserver();observer.hashRetired=async function(){clock=2500;this.hashProof={};};
  const progress=await observeRunProgress({observer,now:()=>clock,deadline:3000,finished:()=>false,findTimer(){clock=500;return {pid:43};}});assert.equal(progress.timerObservedAt,500);assert.equal(progress.busyDeadline,3500);
  const anchor={settled:true,status:200,body:{busy:true,ready:false}},next={...anchor};clock=3400;assert.equal(await waitBusy({anchor,next:()=>next,finished:()=>false,timerLive:()=>true,deadline:progress.busyDeadline,now:()=>clock}),next);
});
test('late blocking receipt or timer recheck cannot pass the busy deadline',async()=>{
  const anchor={settled:true,status:200,body:{busy:true,ready:false}};
  for(const kind of ['receipt','timer']){let clock=0;await assert.rejects(waitBusy({anchor,next(){if(kind==='receipt')clock=3101;return anchor;},finished:()=>false,timerLive(){clock=3101;return true;},deadline:3000,now:()=>clock}),{code:'status_observation_deadline'});}
});
test('a later busy status cannot rescue a nonbusy original anchor',async()=>{
  const anchor={settled:true,status:200,body:{busy:false,ready:true}},next={settled:true,status:200,body:{busy:true,ready:false}};await assert.rejects(waitBusy({anchor,next:()=>next,finished:()=>false,timerLive:()=>true,deadline:3000,now:()=>0}),/immutable status anchor/);
});
