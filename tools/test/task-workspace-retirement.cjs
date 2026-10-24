'use strict';
// Runs only owned, headless CLI fixtures. Every attempt gets a fresh evidence
// directory; failures and fixture files are deliberately retained.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),os=require('node:os');
const assert=require('node:assert/strict');
const {spawn}=require('node:child_process');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const hash=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const REPO=path.resolve(__dirname,'../..');
const STAGES=['IntentSynced','WitnessLinked','LeaseRemoved','JournalRemoved','DirectoryRemoved'];
const normal=file=>path.toNamespacedPath(path.resolve(file)).toLowerCase();
function verificationCases(){
  return STAGES.map(stage=>({label:stage,stage,kind:'normal'})).concat([
    {label:'unknown-journal-leaf',stage:'WitnessLinked',kind:'unknown'},
    {label:'replacement-journal-directory',stage:'JournalRemoved',kind:'replacement'},
    {label:'foreign-witness',stage:'IntentSynced',kind:'foreign'},
    {label:'replacement-owner-lock',stage:'WitnessLinked',kind:'leaf-replacement',leaf:'owner.lock'},
    {label:'replacement-record-jsonl',stage:'WitnessLinked',kind:'leaf-replacement',leaf:'record.jsonl'},
  ]);
}

function within(base,target){
  assert(path.isAbsolute(base)&&path.isAbsolute(target),'Scope checks require absolute paths');
  // Rust canonical paths use the extended Windows namespace. Compare both
  // spellings in that namespace before applying the same strict containment
  // rule; a spelling difference must neither reject owned paths nor admit peers.
  const scoped=value=>process.platform==='win32'?path.toNamespacedPath(path.resolve(value)):path.resolve(value);
  const relative=path.relative(scoped(base),scoped(target));
  assert(relative&&!relative.startsWith('..'+path.sep)&&relative!=='..'&&!path.isAbsolute(relative),'Path escapes owned scope');
  return target;
}
function argumentsFor(argv){
  const result={mode:'verify'};
  for(let i=0;i<argv.length;i++){
    if(argv[i]==='--help')return {help:true};
    if(argv[i]==='--plan'){result.plan=true;continue;}
    assert(['--mode','--binary','--app','--candidate','--output'].includes(argv[i]),'Unknown argument '+argv[i]);
    assert(i+1<argv.length,'Missing argument '+argv[i]);result[argv[i].slice(2)]=argv[++i];
  }
  assert(['baseline','verify'].includes(result.mode),'Unknown mode');
  if(result.plan)assert.equal(result.mode,'verify','--plan describes native verification only');
  for(const name of ['binary','output']){if(!result.plan)assert(result[name],'Required --'+name);if(result[name])result[name]=path.resolve(result[name]);}
  for(const name of ['app','candidate'])if(result[name])result[name]=path.resolve(result[name]);
  if(result.mode==='baseline')for(const name of ['app','candidate'])assert(result[name],'Required baseline --'+name);
  return result;
}
function start(binary,args,env,folder,label,report){
  const out=fs.openSync(path.join(folder,label+'.stdout.log'),'wx');
  const err=fs.openSync(path.join(folder,label+'.stderr.log'),'wx');
  const child=spawn(binary,args,{cwd:REPO,env:{...process.env,...env},windowsHide:true,stdio:['pipe','pipe','pipe']});
  const entry={label,binary,args,fixtureEnvironment:env,directory:folder,pid:child.pid??null,startedAt:new Date().toISOString(),stdout:path.join(folder,label+'.stdout.log'),stderr:path.join(folder,label+'.stderr.log'),exitCode:null,signal:null};
  report.processes.push(entry);
  let stdout='',stderr='',failure=null,done=false;
  child.stdout.on('data',bytes=>{fs.writeSync(out,bytes);stdout+=bytes.toString('utf8');});
  child.stderr.on('data',bytes=>{fs.writeSync(err,bytes);stderr+=bytes.toString('utf8');});
  child.on('error',error=>{failure=error;entry.error=String(error);});
  const closed=new Promise(resolve=>child.on('close',(code,signal)=>{
    done=true;entry.exitCode=code;entry.signal=signal;entry.closedAt=new Date().toISOString();fs.closeSync(out);fs.closeSync(err);resolve();
  }));
  async function line(prefix,ms=15000){
    const deadline=Date.now()+ms;
    for(;;){
      if(failure)throw failure;
      const found=stdout.split(/\r?\n/).find(value=>value.startsWith(prefix));
      if(found)return found.slice(prefix.length);
      assert(!done,'Fixture exited before '+prefix+': '+stdout+' '+stderr);
      assert(Date.now()<deadline,'Fixture readiness timeout '+prefix);
      await delay(10);
    }
  }
  async function finish(kill=false){
    if(kill&&!done){entry.terminationRequested=true;assert(child.kill(),'Owned fixture termination was rejected');}
    if(!kill&&!done)child.stdin.end();
    async function waitClose(ms){let timer;try{await Promise.race([closed,new Promise(resolve=>{timer=setTimeout(resolve,ms);})]);}finally{clearTimeout(timer);}}
    await waitClose(10000);
    if(!done){child.kill();await waitClose(5000);assert.fail('Owned fixture did not close');}
    if(failure)throw failure;
    if(!kill){assert.equal(entry.exitCode,0,'Fixture exit');assert.equal(entry.signal,null,'Fixture signal');}
    return entry;
  }
  return {child,line,finish,closed,get done(){return done;},get stdout(){return stdout;}};
}
function fileIdentity(file){
  const stat=fs.statSync(file,{bigint:true});
  return {dev:String(stat.dev),ino:String(stat.ino),created:String(stat.birthtimeNs),bytes:String(stat.size),sha256:hash(file)};
}
function assertUnchanged(file,before){assert.deepEqual(fileIdentity(file),before,'Protected file changed: '+file);}
function removeOwnedFile(base,file,before){within(base,file);assertUnchanged(file,before);fs.unlinkSync(file);}
function assertReplacementIdentity(original,replacement){
  assert.equal(replacement.bytes,original.bytes,'Replacement content length differs from the original');
  assert.equal(replacement.sha256,original.sha256,'Replacement bytes differ from the original');
  const identity=value=>({dev:value.dev,ino:value.ino,created:value.created});
  assert.notDeepEqual(identity(replacement),identity(original),'Replacement reused the original file identity');
}
function directoryIdentity(folder){
  const stat=fs.lstatSync(folder,{bigint:true});assert(stat.isDirectory()&&!stat.isSymbolicLink(),'Fixture directory was redirected');
  return {dev:String(stat.dev),ino:String(stat.ino),created:String(stat.birthtimeNs)};
}
function inspectReady(data,ready,stage){
  assert.equal(ready.stage,stage);assert(/^[a-f0-9]{32}$/.test(ready.id));
  const journals=within(data,path.join(data,'worker-tasks-v1'));
  assert.equal(normal(ready.journal),normal(within(journals,path.join(journals,ready.id))));
  assert.equal(normal(ready.witness),normal(within(journals,path.join(journals,'retiring-'+ready.id+'.jsonl'))));
  const original=path.join(ready.journal,'record.jsonl');
  const wal=fs.existsSync(original)?original:ready.witness;
  const events=fs.readFileSync(wal,'utf8').trimEnd().split('\n').map(line=>JSON.parse(line));
  assert.equal(events[0].type,'Header');const header=events[0].value;
  assert.equal(header.id,ready.id);assert(/^[a-f0-9]{64}$/.test(header.profile));
  assert.equal(normal(ready.root),normal(path.join(fs.realpathSync.native(os.tmpdir()),'rmdw-'+header.profile.slice(0,12),ready.id)));
  assert.equal(normal(header.root),normal(ready.root));
  assert(events.some(event=>event.type==='Retiring'),'Complete retirement intent is missing');
  assert(!fs.existsSync(ready.root),'Retirement must follow work-root cleanup');
  assert.equal(fs.existsSync(ready.journal),stage!=='DirectoryRemoved');
  assert.equal(fs.existsSync(original),['IntentSynced','WitnessLinked','LeaseRemoved'].includes(stage));
  assert.equal(fs.existsSync(path.join(ready.journal,'owner.lock')),['IntentSynced','WitnessLinked'].includes(stage));
  assert.equal(fs.existsSync(ready.witness),stage!=='IntentSynced');
  if(fs.existsSync(original)&&fs.existsSync(ready.witness)){
    const a=fileIdentity(original),b=fileIdentity(ready.witness);
    assert.equal(a.ino,b.ino,'Witness is not a hard link to the validated WAL');assert.equal(a.dev,b.dev);assert.equal(a.sha256,b.sha256);
  }
  return {events,walSha256:hash(wal)};
}
async function recoverFixture(options,data,folder,label,report){
  const process=start(options.binary,['--exact','task_workspace::tests::retirement_recover_child','--nocapture'],{READMD_TASK_RETIREMENT_RECOVER:data},folder,label,report);
  try{const result=JSON.parse(await process.line('RETIREMENT_RECOVERED '));await process.finish();return result;}
  finally{if(!process.done)await process.finish(true);}
}
function assertRecovered(value,id){
  assert.deepEqual(value.before,[id],'Directory and witness were double-counted');
  assert.equal(value.reservedBefore,256*1024*1024,'Retirement reservation was lost or counted twice');
  assert.equal(value.report.visited,1);assert.equal(value.report.removed,1);
  for(const key of ['running','retained','preserved','failed'])assert.equal(value.report[key],0,'Unexpected recovery result '+key);
  assert.deepEqual(value.after,[]);
}
function freezeWorkingSources(output){
  const result=[];
  for(const name of ['rust/readmd-kernel/src/task_workspace.rs','rust/readmd-kernel/src/task_workspace/registry.rs','tools/test/task-workspace-retirement.cjs','tools/test/task-workspace-retirement.test.cjs']){
    const file=path.join(REPO,name);assert(fs.existsSync(file),'Missing verification source '+name);
    const copy=within(output,path.join(output,'source',name));fs.mkdirSync(path.dirname(copy),{recursive:true});fs.copyFileSync(file,copy,fs.constants.COPYFILE_EXCL);
    result.push({path:name,sha256:hash(file),copy:path.relative(output,copy)});
  }
  return result;
}
async function verify(options,report){
  assert.equal(process.platform,'win32','Native retirement crash acceptance is Windows only');
  report.binding.sources=freezeWorkingSources(options.output);report.cases=[];
  const cases=verificationCases();
  for(const item of cases){
    const folder=within(options.output,path.join(options.output,item.label));fs.mkdirSync(folder);
    const data=within(folder,path.join(folder,'profile'));fs.mkdirSync(data);
    const protectedFile=within(data,path.join(data,'protected-old-target.txt'));
    fs.writeFileSync(protectedFile,'Synthetic pre-existing target must remain byte-for-byte and identity stable.\n',{flag:'wx'});
    const protectedBefore=fileIdentity(protectedFile);
    const record={...item,data,protectedFile,protectedBefore};report.cases.push(record);
    const owner=start(options.binary,['--exact','task_workspace::tests::retirement_child','--nocapture'],{READMD_TASK_RETIREMENT_CHILD:data,READMD_TASK_RETIREMENT_STAGE:item.stage},folder,'owner',report);
    try{
      const ready=JSON.parse(await owner.line('RETIREMENT_READY '));record.ready=ready;
      record.beforeCrash=inspectReady(data,ready,item.stage);
      const exit=await owner.finish(true);assert(exit.terminationRequested,'Fixture did not receive a real termination request');
      assert(exit.exitCode!==0||exit.signal!==null,'Crash fixture exited successfully');record.crashObserved=true;
      assertUnchanged(protectedFile,protectedBefore);
      if(item.kind==='normal'){
        record.recovery=await recoverFixture(options,data,folder,'restart',report);assertRecovered(record.recovery,ready.id);
      }else if(item.kind==='leaf-replacement'){
        assert(['owner.lock','record.jsonl'].includes(item.leaf),'Unexpected replacement fixture leaf');
        const leaf=within(data,path.join(ready.journal,item.leaf));
        // Keep the original object outside the journal directory. An extra
        // unknown child must not accidentally supply the preservation result.
        const saved=within(data,path.join(data,'original-'+ready.id+'-'+item.leaf));
        const original=fileIdentity(leaf),bytes=fs.readFileSync(leaf),witnessBefore=fileIdentity(ready.witness);
        assert(!fs.existsSync(saved),'Preserved original already exists');
        fs.renameSync(leaf,saved);assertUnchanged(saved,original);
        fs.writeFileSync(leaf,bytes,{flag:'wx'});
        const replacement=fileIdentity(leaf);assertReplacementIdentity(original,replacement);
        record.identityReplacement={leaf,saved,original,replacement,witnessBefore};
        record.protectedRecovery=await recoverFixture(options,data,folder,'restart-preserve',report);
        assert.deepEqual(record.protectedRecovery.before,[ready.id]);assert.equal(record.protectedRecovery.reservedBefore,256*1024*1024);
        assert.equal(record.protectedRecovery.report.visited,1);assert.equal(record.protectedRecovery.report.preserved,1);
        for(const key of ['removed','running','retained','failed'])assert.equal(record.protectedRecovery.report[key],0,'Replacement recovery result '+key);
        assert.deepEqual(record.protectedRecovery.after,[ready.id]);
        assertUnchanged(leaf,replacement);assertUnchanged(saved,original);assertUnchanged(ready.witness,witnessBefore);assertUnchanged(protectedFile,protectedBefore);
        // Move the exact original identity back, then use another independent
        // restart to test convergence. No failed operation is automatically retried.
        removeOwnedFile(data,leaf,replacement);within(data,saved);fs.renameSync(saved,leaf);assertUnchanged(leaf,original);
        assert(!fs.existsSync(saved));
        record.originalIdentityRestored=true;
        record.restoredRecovery=await recoverFixture(options,data,folder,'restart-after-fixture-restore',report);assertRecovered(record.restoredRecovery,ready.id);
      }else{
        let sentinel,moved,originalDirectory,replacementDirectory;
        if(item.kind==='unknown')sentinel=within(data,path.join(ready.journal,'unknown.txt'));
        if(item.kind==='replacement'){
          moved=within(data,path.join(data,'worker-tasks-v1','original-'+ready.id));
          originalDirectory=directoryIdentity(ready.journal);
          within(data,ready.journal);fs.renameSync(ready.journal,moved);fs.mkdirSync(ready.journal);
          replacementDirectory=directoryIdentity(ready.journal);
          sentinel=within(data,path.join(ready.journal,'unknown.txt'));
        }
        if(item.kind==='foreign')sentinel=within(data,ready.witness);
        fs.writeFileSync(sentinel,'Synthetic foreign content: the reaper has no ownership of this object.\n',{flag:'wx'});
        const sentinelBefore=fileIdentity(sentinel);record.sentinel={file:sentinel,identity:sentinelBefore};
        record.protectedRecovery=await recoverFixture(options,data,folder,'restart-preserve',report);
        assert.deepEqual(record.protectedRecovery.before,[ready.id]);assert.equal(record.protectedRecovery.reservedBefore,256*1024*1024);
        assert.equal(record.protectedRecovery.report.visited,1);assert.equal(record.protectedRecovery.report.removed,0);
        assert.equal(record.protectedRecovery.report.preserved,1);assert.equal(record.protectedRecovery.report.failed,0);
        assertUnchanged(sentinel,sentinelBefore);assertUnchanged(protectedFile,protectedBefore);
        if(item.kind==='unknown'){
          assert(fs.existsSync(path.join(ready.journal,'record.jsonl')),'Unknown leaf lost its original recovery WAL');
          assert.equal(hash(ready.witness),record.beforeCrash.walSha256,'Unknown leaf lost its external recovery WAL');
        }else if(item.kind==='replacement'){
          assert.equal(hash(ready.witness),record.beforeCrash.walSha256,'Replacement directory lost the recovery witness');
          assert.deepEqual(fs.readdirSync(moved),[],'Original empty directory was unexpectedly changed');
        }else assert.equal(hash(path.join(ready.journal,'record.jsonl')),record.beforeCrash.walSha256,'Foreign witness caused original WAL mutation');
        // Restore only the explicitly created fixture, after checking its exact
        // identity and bytes. This is a separate convergence phase, not a retry.
        removeOwnedFile(data,sentinel,sentinelBefore);
        if(moved){
          within(data,ready.journal);assert.deepEqual(directoryIdentity(ready.journal),replacementDirectory);assert.deepEqual(fs.readdirSync(ready.journal),[]);fs.rmdirSync(ready.journal);
          within(data,moved);assert.deepEqual(directoryIdentity(moved),originalDirectory);fs.renameSync(moved,ready.journal);assert.deepEqual(directoryIdentity(ready.journal),originalDirectory);
        }
        record.restoredRecovery=await recoverFixture(options,data,folder,'restart-after-fixture-restore',report);assertRecovered(record.restoredRecovery,ready.id);
      }
      assert(!fs.existsSync(ready.journal));assert(!fs.existsSync(ready.witness));assertUnchanged(protectedFile,protectedBefore);
      record.ok=true;
    }finally{if(!owner.done)await owner.finish(true);}
  }
  for(const source of report.binding.sources)assert.equal(hash(path.join(REPO,source.path)),source.sha256,'Source changed during native verification');
  assert.equal(hash(options.binary),report.binding.binarySha256,'Test binary changed during verification');
}
function snapshotSources(candidate,output){
  const record=JSON.parse(fs.readFileSync(candidate,'utf8'));
  const freeze=path.resolve(REPO,record.sourceFreeze);
  assert.equal(hash(freeze),record.sourceFreezeSha256,'Candidate source manifest changed');
  const manifest=JSON.parse(fs.readFileSync(freeze,'utf8'));
  const sources=[];
  for(const name of ['rust/readmd-kernel/src/task_workspace.rs','rust/readmd-kernel/src/task_workspace/registry.rs']){
    const entry=manifest.sources.find(item=>item.path===name);assert(entry,'Frozen task source missing');
    const file=path.resolve(REPO,entry.copy);assert.equal(hash(file),entry.sha256,'Frozen task source changed');
    const copy=within(output,path.join(output,name));fs.mkdirSync(path.dirname(copy),{recursive:true});fs.copyFileSync(file,copy,fs.constants.COPYFILE_EXCL);
    sources.push({path:name,original:file,sha256:entry.sha256});
  }
  return {record,freeze,freezeSha256:hash(freeze),sources};
}
async function baseline(options,report){
  const binding=snapshotSources(options.candidate,options.output);report.binding={...report.binding,...binding};
  assert.equal(hash(options.app),binding.record.binarySha256,'Baseline app is not the frozen candidate');
  const data=within(options.output,path.join(options.output,'profile'));fs.mkdirSync(data);
  const control=start(options.binary,['--exact','task_workspace::tests::admission_child','--nocapture'],{READMD_TASK_ADMISSION_CHILD:data},options.output,'baseline-owner',report);
  try{
    const tasks=JSON.parse(await control.line('ADMISSION_READY '));assert.equal(tasks.length,8);
    const task=tasks[7];assert(/^[a-f0-9]{32}$/.test(task.id));
    const journal=within(data,path.join(data,'worker-tasks-v1',task.id));
    const unknown=within(journal,path.join(journal,'unknown.txt'));
    const original=Buffer.from('Synthetic unknown journal leaf; baseline must preserve it.\n');
    fs.writeFileSync(unknown,original,{flag:'wx'});
    report.orphan={id:task.id,root:task.root,journal,unknown,sha256:hash(unknown)};
    control.child.stdin.write('release-one\n');await control.line('ADMISSION_RELEASED');
    assert(!fs.existsSync(task.root),'Baseline did not remove known work root');
    assert(!fs.existsSync(path.join(journal,'record.jsonl')),'Baseline unexpectedly kept the recovery WAL');
    assert.deepEqual(fs.readFileSync(unknown),original);
    report.originalFailureObserved=true;
    await control.finish();
    const release=start(options.binary,['--exact','task_workspace::tests::release_child','--nocapture'],{READMD_TASK_RELEASE_CHILD:data,READMD_TASK_RELEASE_ID:tasks[0].id},options.output,'baseline-release',report);
    await release.finish();
    const app=start(options.app,['--mcp','--data-dir',data],{},options.output,'baseline-restart',report);
    try{
      app.child.stdin.write(JSON.stringify({jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'task-retirement-baseline',version:'1'}}})+'\n');
      const deadline=Date.now()+15000;
      while(!app.stdout.split(/\r?\n/).some(line=>{try{return JSON.parse(line).id===1;}catch{return false;}})){
        assert(!app.done&&Date.now()<deadline,'Baseline MCP initialization failed');await delay(10);
      }
      // Observing the ordinary unpinned task disappear proves startup recovery
      // ran; an arbitrary delay cannot establish that the orphan was visited.
      const ordinary=within(data,path.join(data,'worker-tasks-v1',tasks[0].id));
      while(fs.existsSync(ordinary)){assert(!app.done&&Date.now()<deadline,'Startup recovery did not run');await delay(10);}
      assert(fs.existsSync(journal),'Baseline orphan disappeared without recovery proof');
      assert(!fs.existsSync(path.join(journal,'record.jsonl')));
      assert.deepEqual(fs.readFileSync(unknown),original);
      report.afterRestart={journalEntries:fs.readdirSync(journal),ordinaryTaskRecovered:true,unknownSha256:hash(unknown),orphanStillHasNoWal:true};
      await app.finish();
    }finally{if(!app.done)await app.finish(true);}
  }finally{if(!control.done)await control.finish(true);}
}
async function main(argv){
  const options=argumentsFor(argv);
  if(options.help){console.log('node task-workspace-retirement.cjs --mode baseline --binary <old test.exe> --app <old readmd.exe> --candidate <candidate.json> --output <new directory>\nnode task-workspace-retirement.cjs --binary <new test.exe> --output <new directory>\nnode task-workspace-retirement.cjs --plan');return;}
  if(options.plan){const cases=verificationCases();console.log(JSON.stringify({schema:1,mode:'verify',nativeVerificationRun:false,computerUse:false,automaticRetries:0,caseCount:cases.length,cases}));return;}
  assert(!fs.existsSync(options.output),'Evidence output already exists; preserve the first attempt');fs.mkdirSync(options.output,{recursive:true});
  const report={schema:1,mode:options.mode,startedAt:new Date().toISOString(),computerUse:false,automaticRetries:0,processes:[],binding:{binary:options.binary,binarySha256:hash(options.binary),app:options.app,appSha256:options.app?hash(options.app):null,qaSha256:hash(__filename)}};
  try{
    if(options.mode==='baseline')await baseline(options,report);
    else await verify(options,report);
    report.ok=true;
  }catch(error){report.ok=false;report.failure=String(error.stack||error);process.exitCode=1;}
  report.finishedAt=new Date().toISOString();fs.writeFileSync(path.join(options.output,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});
  console.log(JSON.stringify({ok:report.ok,originalFailureObserved:report.originalFailureObserved??false,output:options.output,processes:report.processes.map(item=>({label:item.label,exitCode:item.exitCode,signal:item.signal})),failure:report.failure}));
}
module.exports={within,argumentsFor,assertRecovered,assertReplacementIdentity,verificationCases,STAGES};
if(require.main===module)main(process.argv.slice(2)).catch(error=>{console.error(error);process.exitCode=1;});
