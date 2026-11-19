'use strict';
// Read-only evidence for this fixture, never a service authorization mechanism.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {execFileSync}=require('node:child_process');
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const MAX_JOURNAL=4*1024*1024,UUID=/^[a-f0-9]{32}$/;
function failure(code,message){const error=Error(message);error.code=code;return error;}
function before(deadline,now,label){const at=now();if(!Number.isFinite(at)||at>=deadline)throw failure('status_observation_deadline',label+' exceeded its absolute monotonic deadline');return at;}
function json(bytes){return JSON.parse(bytes.toString('utf8'),(key,value,context)=>{
  if(typeof value!=='number')return value;
  assert(context&&/^(0|[1-9][0-9]*)$/.test(context.source),'journal integers need lossless JSON source');
  const number=BigInt(context.source);assert(number<=0xffffffffffffffffn);return number;
});}
function keys(value,names){assert(value&&typeof value==='object'&&!Array.isArray(value));assert.deepEqual(Object.keys(value).sort(),[...names].sort());}
function identity(metadata){return {volume:metadata.dev,index:metadata.ino,created:metadata.birthtimeNs};}
function validIdentity(value){keys(value,['volume','index','created']);for(const n of Object.values(value))assert(typeof n==='bigint'&&n>=0n&&n<=0xffffffffffffffffn);assert(value.created>0n);return value;}
function matches(actual,expected){return actual.volume===expected.volume&&actual.index===expected.index&&actual.created===expected.created;}
function encoded(value){return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,typeof v==='bigint'?String(v):v]));}
function metadata(file){const m=fs.fstatSync(file,{bigint:true});assert(m.isFile()&&!m.isSymbolicLink());return m;}
function content(m){return {dev:m.dev,ino:m.ino,birthtimeNs:m.birthtimeNs,size:m.size,mtimeNs:m.mtimeNs,mode:m.mode,uid:m.uid,gid:m.gid};}
function stable(m){return {...content(m),ctimeNs:m.ctimeNs,nlink:m.nlink};}
function directory(file,privateDirectory=false){
  const m=fs.lstatSync(file,{bigint:true});assert(m.isDirectory()&&!m.isSymbolicLink());
  assert.equal(fs.realpathSync(file),file,'owned directory was redirected');
  if(process.getuid)assert.equal(m.uid,BigInt(process.getuid()));
  if(privateDirectory)assert.equal(m.mode&0o077n,0n,'task directory must be private');
  return identity(m);
}
function nofollow(file){
  assert(path.isAbsolute(file));let parent=path.parse(file).root;
  for(const part of path.relative(parent,path.dirname(file)).split(path.sep).filter(Boolean)){
    parent=path.join(parent,part);const m=fs.lstatSync(parent);assert(m.isDirectory()&&!m.isSymbolicLink(),'redirected file ancestor');
  }
  const named=fs.lstatSync(file,{bigint:true});assert(named.isFile()&&!named.isSymbolicLink());
  const fd=fs.openSync(file,fs.constants.O_RDONLY|fs.constants.O_NOFOLLOW|fs.constants.O_NONBLOCK);
  try{assert(matches(identity(metadata(fd)),identity(named)),'opened a different object');assert.equal(fs.realpathSync(file),file);return fd;}catch(error){fs.closeSync(fd);throw error;}
}
function readJournal(fd){
  const m=metadata(fd);assert(m.size<=BigInt(MAX_JOURNAL),'journal exceeds the fixed bound');
  const bytes=Buffer.alloc(Number(m.size));let at=0;
  while(at<bytes.length){const n=fs.readSync(fd,bytes,at,bytes.length-at,at);assert(n>0,'short journal read');at+=n;}
  const end=bytes.lastIndexOf(10);return end<0?Buffer.alloc(0):bytes.subarray(0,end+1);
}
function journal(prefix){
  const events=prefix.toString('utf8').split('\n').filter(Boolean).map(line=>json(Buffer.from(line)));
  if(!events.length)return null;
  keys(events[0],['type','value']);assert.equal(events[0].type,'Header');const header=events[0].value;
  keys(header,['schema','id','profile','root','identity','purpose','reserved_bytes']);assert.equal(header.schema,1n);validIdentity(header.identity);
  assert(UUID.test(header.id)&&/^[a-f0-9]{64}$/.test(header.profile));assert(typeof header.root==='string'&&path.isAbsolute(header.root));
  assert(typeof header.reserved_bytes==='bigint'&&header.reserved_bytes>0n);
  let created,finished=false,retiring;
  for(const event of events.slice(1)){
    assert(!retiring,'event after retirement intent');
    if(event.type==='Created'){
      keys(event,['type','path','identity','kind','target']);assert(!created&&!finished);assert.equal(event.path,'node');assert.equal(event.kind,'File');assert.equal(event.target,null);created=validIdentity(event.identity);
    }else if(event.type==='Finished'){
      keys(event,['type']);assert(created&&!finished);finished=true;
    }else if(event.type==='Retiring'){
      keys(event,['type','value']);assert(finished);keys(event.value,['directory','journal','lease']);for(const value of Object.values(event.value))validIdentity(value);retiring=event.value;
    }else throw failure('status_journal_invalid','unexpected node-probe journal event');
  }
  return {header,created,finished,retiring};
}
function parseSnapshots(text){
  return text.trim().split('\n').filter(Boolean).map(line=>{
    const m=line.match(/^\s*(\d+)\s+(\d+)\s+(\d+)\s+(\S+)\s+(\S+\s+\S+\s+\d+\s+\d{2}:\d{2}:\d{2}\s+\d{4})\s+(.+)$/);
    assert(m,'incomplete owned process snapshot');return {pid:Number(m[1]),ppid:Number(m[2]),pgid:Number(m[3]),state:m[4],birth:m[5],args:m[6]};
  });
}
function snapshots(parent,onlyPid){
  assert(Number.isSafeInteger(parent)&&parent>0);
  const ids=onlyPid?[onlyPid]:execFileSync('/bin/ps',['-axo','pid=,ppid='],{encoding:'utf8',timeout:3000,maxBuffer:1024*1024})
    .trim().split('\n').map(line=>line.trim().split(/\s+/).map(Number)).filter(([,ppid])=>ppid===parent).map(([pid])=>pid);
  assert(ids.length<=32,'unexpected owned child count');if(!ids.length)return [];
  try{return parseSnapshots(execFileSync('/bin/ps',['-ww','-p',ids.join(','),'-o','pid=,ppid=,pgid=,stat=,lstart=,args='],{encoding:'utf8',timeout:3000,maxBuffer:1024*1024})).filter(row=>row.ppid===parent);}catch(error){if(error.status===1)return [];throw error;}
}
function live(row,parent){return Boolean(row&&Number.isSafeInteger(row.pid)&&row.pid>0&&row.ppid===parent&&row.pgid===row.pid&&typeof row.birth==='string'&&row.birth.length&&typeof row.state==='string'&&row.state.length&&!/[ZX]/.test(row.state)&&typeof row.args==='string'&&row.args.length);}
function sameLive(worker,parent,read=snapshots){return Boolean(worker&&read(parent,worker.pid).some(row=>live(row,parent)&&row.pid===worker.pid&&row.birth===worker.birth&&row.args===worker.args));}
function sampleStorage(root,observer,{afterDirectoryRead=()=>{},afterScan=()=>{}}={}){
  const pinned=observer?.selected?.created;let pathnameBytes=0,scannedPinnedBytes=0;
  function scan(dir){
    let names;try{names=fs.readdirSync(dir);}catch(error){if(error.code==='ENOENT')return;throw error;}
    afterDirectoryRead(dir);
    for(const name of names){const file=path.join(dir,name);let m;try{m=fs.lstatSync(file,{bigint:true});}catch(error){if(error.code==='ENOENT')continue;throw error;}
      if(m.isDirectory()&&!m.isSymbolicLink())scan(file);
      else{pathnameBytes+=Number(m.size);if(pinned&&m.isFile()&&matches(identity(m),pinned))scannedPinnedBytes+=Number(m.size);}
    }
  }
  scan(root);afterScan();
  // Scan first, fstat last. An unlink/rename can make the scan miss a still
  // open object, or leave its old name counted. Only add the missing bytes;
  // never subtract an object already seen in the scan.
  const current=observer?.pinnedStorage();const fdBytes=current?.bytes||0;
  const supplementBytes=Math.max(fdBytes-scannedPinnedBytes,0);
  return {pathnameBytes,scannedPinnedBytes,linkedFdBytes:current?.links>0?fdBytes:0,unlinkedOpenBytes:current?.links===0?fdBytes:0,uncertainCloseReservationBytes:current?.uncertain?fdBytes:0,supplementBytes,totalBytes:pathnameBytes+supplementBytes,sampledHighWaterOnly:true,physicalAllocationOrAllTimePeakCertified:false};
}
class StatusProbeObserver {
  constructor({profile,tmp,hostPid,expected,probeSource,record=()=>{},now=()=>performance.now(),readSnapshots=snapshots}){
    assert(['darwin','linux'].includes(process.platform),'status-first OS observer is POSIX only');
    this.profile=fs.realpathSync(profile);this.tmp=fs.realpathSync(tmp);this.profileIdentity=directory(this.profile);this.tmpIdentity=directory(this.tmp);
    this.profileHash=sha(Buffer.from(this.profile));this.workParent=path.join(this.tmp,'rmdw-'+this.profileHash.slice(0,12));this.journals=path.join(this.profile,'worker-tasks-v1');
    this.hostPid=hostPid;this.expected=expected;this.probeSource=probeSource;this.record=record;this.now=now;this.readSnapshots=readSnapshots;this.seen=new Set();
    assert(Number.isSafeInteger(expected.size)&&expected.size>0&&/^[a-f0-9]{64}$/.test(expected.sha256));this.baseline=new Set(this.ids());
  }
  ids(){try{directory(this.journals);const entries=fs.readdirSync(this.journals).filter(n=>UUID.test(n));assert(entries.length<=128,'owned journal scan exceeds task bound');return entries;}catch(error){if(error.code==='ENOENT')return [];throw error;}}
  capture(anchor){
    assert(anchor&&!anchor.settled);assert(!this.selected,'anchor cannot be replaced');
    assert(matches(directory(this.profile),this.profileIdentity)&&matches(directory(this.tmp),this.tmpIdentity));
    const candidates=[];
    for(const id of this.ids().filter(id=>!this.baseline.has(id))){
      let fd;
      try{
        const dir=path.join(this.journals,id);directory(dir,true);fd=nofollow(path.join(dir,'record.jsonl'));
        const prefix=readJournal(fd),state=journal(prefix);if(!state||state.header.purpose!=='node-probe'||!state.created||state.finished)continue;
        assert.equal(state.header.id,id);assert.equal(state.header.profile,this.profileHash);assert.equal(state.header.root,path.join(this.workParent,id));
        directory(this.workParent,true);assert(matches(directory(state.header.root,true),state.header.identity),'task root identity mismatch');
        this.seen.add(id);assert(this.seen.size<=6,'original six-candidate bound');candidates.push({id,dir,prefix,state});
      }catch(error){if(error.code!=='ENOENT')throw error;this.record({stage:'owned_copy_retired_before_barrier',taskId:id});}
      finally{if(fd!==undefined)fs.closeSync(fd);}
    }
    assert(candidates.length<=1,'ambiguous new node-probe task');if(!candidates.length)return null;
    const candidate=candidates[0];let journalFd,binaryFd;
    try{
      journalFd=nofollow(path.join(candidate.dir,'record.jsonl'));const prefix=readJournal(journalFd);assert(prefix.subarray(0,candidate.prefix.length).equals(candidate.prefix));
      const state=journal(prefix);assert.deepEqual(state.header,candidate.state.header);assert(matches(state.created,candidate.state.created));assert(!state.finished);const program=path.join(state.header.root,'node');binaryFd=nofollow(program);const m=metadata(binaryFd);
      assert(matches(identity(m),state.created),'Created inode mismatch');assert.equal(m.nlink,1n);assert(m.size<=BigInt(this.expected.size));if(process.getuid)assert.equal(m.uid,BigInt(process.getuid()));
      const ownerFd=nofollow(path.join(candidate.dir,'owner.lock'));let ownerIdentity;try{assert.equal(metadata(ownerFd).size,0n);ownerIdentity=identity(metadata(ownerFd));}finally{fs.closeSync(ownerFd);}
      this.selected={...candidate,prefix,state,program,journalFd,binaryFd,journalIdentity:identity(metadata(journalFd)),journalDirectory:directory(candidate.dir,true),ownerIdentity,created:state.created,anchor};
      const proof={taskId:candidate.id,profileSha256:this.profileHash,root:path.relative(this.tmp,state.header.root),created:encoded(state.created),statusIndex:anchor.index,atMonotonicMs:this.now(),copyPreparationOnly:true};
      this.record({stage:'owned_header_created_barrier',...proof});return proof;
    }catch(error){this.selected=undefined;if(binaryFd!==undefined)fs.closeSync(binaryFd);if(journalFd!==undefined)fs.closeSync(journalFd);if(error.code==='ENOENT'){this.record({stage:'owned_copy_retired_before_barrier',taskId:candidate.id});return null;}throw error;}
  }
  journalState(){
    const selected=this.selected;assert(selected&&selected.journalFd!==undefined);assert(matches(identity(metadata(selected.journalFd)),selected.journalIdentity));
    const prefix=readJournal(selected.journalFd);assert(prefix.length>=selected.prefix.length&&prefix.subarray(0,selected.prefix.length).equals(selected.prefix),'journal is not append-only');
    selected.prefix=Buffer.from(prefix);const state=journal(prefix);assert.deepEqual(state.header,selected.state.header);assert(matches(state.created,selected.created));
    if(state.retiring){assert(matches(state.retiring.directory,selected.journalDirectory));assert(matches(state.retiring.journal,selected.journalIdentity));assert(matches(state.retiring.lease,selected.ownerIdentity));}
    return state;
  }
  observeLive(workers=this.readSnapshots(this.hostPid)){
    assert(this.selected);if(this.worker)return this.worker;
    const selected=this.selected;
    for(const row of workers){
      if(row.args!==selected.program+' --permission --eval '+this.probeSource&&row.args!=='"'+selected.program+'" --permission --eval '+this.probeSource)continue;
      assert(live(row,this.hostPid),'probe identity/state/group mismatch');
      this.journalState();const m=metadata(selected.binaryFd),named=fs.lstatSync(selected.program,{bigint:true});
      assert(named.isFile()&&!named.isSymbolicLink());assert.equal(fs.realpathSync(selected.program),selected.program);
      assert(matches(identity(m),selected.created)&&matches(identity(named),selected.created));assert.equal(m.size,BigInt(this.expected.size));assert.equal(m.mode&0o777n,0o700n);assert.equal(m.nlink,1n);
      selected.liveMetadata=content(m);this.worker={...row,program:selected.program};
      this.witness={pid:row.pid,ppid:row.ppid,pgid:row.pgid,state:row.state,birth:row.birth,argsSha256:sha(row.args),taskId:selected.id,created:encoded(selected.created),atMonotonicMs:this.now(),program:path.relative(this.tmp,selected.program),metadata:encoded(stable(m))};
      this.record({stage:'actual_owned_probe_witness',...this.witness});return this.worker;
    }
    return null;
  }
  retirementReady(){
    assert(this.selected);if(!this.worker)return false;const state=this.journalState(),m=metadata(this.selected.binaryFd);
    assert(matches(identity(m),this.selected.created));assert.deepEqual(content(m),this.selected.liveMetadata,'content metadata changed after live witness');
    // Finished is neither probe-success nor per-file unlink evidence. The fd
    // itself must be unlinked, and the complete identity-bound intent exists.
    return Boolean(state.finished&&state.retiring&&m.nlink===0n);
  }
  async hashRetired(deadline,{afterChunk=()=>{},yieldNow=()=>new Promise(resolve=>setImmediate(resolve))}={}){
    try {
    assert(this.retirementReady(),'no stable actual unlink evidence');before(deadline,this.now,'probe retirement observation');
    const selected=this.selected,start=metadata(selected.binaryFd);assert.equal(start.nlink,0n);const digest=crypto.createHash('sha256'),buffer=Buffer.alloc(Math.min(this.expected.size,1024*1024));let offset=0;
    const startedAt=this.now();
    while(offset<this.expected.size){
      const n=fs.readSync(selected.binaryFd,buffer,0,Math.min(buffer.length,this.expected.size-offset),offset);before(deadline,this.now,'independent probe read');assert(n>0,'short probe read');digest.update(buffer.subarray(0,n));offset+=n;
      afterChunk(offset);await yieldNow();before(deadline,this.now,'independent probe hash');
    }
    const actual=digest.digest('hex'),end=metadata(selected.binaryFd);assert.deepEqual(stable(end),stable(start),'probe metadata changed during independent full hash');assert.equal(actual,this.expected.sha256,'independent private-copy digest');
    this.journalState();const completedAt=before(deadline,this.now,'independent probe proof');
    this.hashProof={taskId:selected.id,bytes:offset,sha256:actual,startedAtMonotonicMs:startedAt,completedAtMonotonicMs:completedAt,metadataBefore:encoded(stable(start)),metadataAfter:encoded(stable(end)),binaryFdActuallyUnlinked:true,journalRetirementIntentValidated:true,finishedIsProbeSuccess:false,hiddenLiveToRetirementMutationExcluded:false,stableDigestIntervalCertified:true};
    this.record({stage:'independent_private_probe_hash',...this.hashProof});before(deadline,this.now,'reported hash proof');return this.hashProof;
    } finally { this.close(); }
  }
  pinnedBytes(){return this.selected?.binaryFd!==undefined||this.closeUncertain?this.expected.size:0;}
  pinnedStorage(){
    if(this.closeUncertain)return {bytes:this.expected.size,links:null,uncertain:true};
    if(this.selected?.binaryFd===undefined)return null;
    const m=metadata(this.selected.binaryFd);assert(matches(identity(m),this.selected.created));return {bytes:Number(m.size),links:Number(m.nlink),uncertain:false};
  }
  close(){let first;for(const key of ['binaryFd','journalFd']){const fd=this.selected?.[key];if(fd!==undefined){this.selected[key]=undefined;try{fs.closeSync(fd);}catch(error){this.closeUncertain=true;first??=error;}}}if(first)throw first;if(this.closeUncertain)throw failure('status_observer_close_uncertain','observer descriptor close could not be certified');}
}
async function prepareStatusFirst({observer,pending,now=()=>performance.now(),sleep=pause,record=()=>{},deadline=now()+6000}){
  try {
  while(now()<deadline){const anchor=pending();if(anchor){const proof=observer.capture(anchor);before(deadline,now,'status-work barrier');if(proof){assert(!anchor.settled,'anchor finished before the unique Run');return {anchor,proof,deadline};}}else before(deadline,now,'status-work wait');await sleep(10);}
  record({stage:'status_work_window_unavailable'});throw failure('status_work_window_unavailable','No unique owned Header+Created within the original6s preparation bound; Run was not requested');
  } catch(error) { observer.close?.(); throw error; }
}
async function observeRunProgress({observer,findTimer,finished,readWorkers=()=>undefined,now=()=>performance.now(),sleep=pause,deadline}){
  try {
  let timer,timerObservedAt;
  while(now()<deadline&&!finished()){
    // Use one snapshot of the host's children for both witnesses. Starting a
    // second set of ps processes between samples can miss the short probe.
    const workers=(!observer.worker||!timer)?readWorkers():undefined;
    if(!observer.worker){observer.observeLive(workers);before(deadline,now,'actual probe witness');}
    if(!timer){const found=findTimer(workers);const at=before(deadline,now,'actual timer witness');if(found){timer=found;timerObservedAt=at;}}
    if(observer.worker&&!observer.hashProof){const ready=observer.retirementReady();before(deadline,now,'probe retirement witness');if(ready){await observer.hashRetired(deadline);before(deadline,now,'complete probe hash witness');}}
    if(timer&&observer.worker&&observer.hashProof){before(deadline,now,'complete Run proof');return {timer,timerObservedAt,busyDeadline:timerObservedAt+3000,probe:observer.worker,hash:observer.hashProof};}
    await sleep(10);
  }
  throw failure('status_run_proof_unavailable','Actual timer, same-anchor live probe and full independent hash must all be observed within Run+3s');
  } catch(error) { observer.close?.(); throw error; }
}
async function waitBusy({anchor,next,finished,timerLive,deadline,now=()=>performance.now(),sleep=pause}){
  while(now()<deadline){const following=next();before(deadline,now,'busy receipt observation');if(anchor.settled&&following?.settled){
    assert(!finished(),'the timer remains active through repeated real status responses');
    for(const row of [anchor,following]){assert.equal(row.status,200);assert.equal(row.body.busy,true,'immutable status anchor and next must be busy');assert.equal(row.body.ready,false);}
    assert(timerLive(),'owned timer remains live');before(deadline,now,'busy timer witness');return following;
  }await sleep(10);}
  throw failure('status_busy_window_unavailable','Original additional3s busy observation window expired');
}
module.exports={StatusProbeObserver,prepareStatusFirst,observeRunProgress,waitBusy,before,journal,json,parseSnapshots,snapshots,sameLive,sampleStorage};
