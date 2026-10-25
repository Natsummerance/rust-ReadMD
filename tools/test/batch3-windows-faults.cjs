'use strict';
// Windows fault step2: cross-transport owners (MCP + HTTP) for typst/ocr.
// --plan and --case are detached planning only: no process, no file write.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {spawn,spawnSync}=require('node:child_process');
const {performance}=require('node:perf_hooks');
const {StdioClient,verifyBinding,systemEnvironment,assertToolCallSchema}=require('./batch3-semantics-stdio.cjs');
const {payload}=require('./managed-tex-process-cold.cjs');
const {HttpHost,freePort}=require('./batch3-math-section-stdio.cjs');
const {requestHttp}=require('./batch3-windows-resource-wait-stdio.cjs');
const ROOT=path.resolve(__dirname,'../..'),UPGRADE=path.join(ROOT,'.cache/capability-upgrade');
const SELF='tools/test/batch3-windows-faults.cjs',ORACLE='tools/test/inspect-windows-faults.py',PDF_ORACLE='tools/test/inspect-windows-resource-wait.py';
const FILES=[SELF,SELF.replace('.cjs','.test.cjs'),ORACLE,ORACLE.replace('.py','.test.py'),
 'tools/test/batch3-semantics-stdio.cjs','tools/test/batch3-math-section-stdio.cjs','tools/test/inspect-math-section-roundtrip.py',
 'tools/test/batch3-windows-resource-wait-stdio.cjs','tools/test/inspect-windows-resource-wait.py',
 'rust/readmd-kernel/src/capability_process.rs','rust/readmd-kernel/src/capability_packs/space.rs',
 'ui-tests/helpers/windows-worker-observer.cs','ui-tests/helpers/windows-worker-observer.ps1'];
// Observers are frozen with the candidate and their exact bytes are also
// recorded in the run report for standalone inspection.
const OBSERVER_FILES=['ui-tests/helpers/windows-worker-observer.cs','ui-tests/helpers/windows-worker-observer.ps1'];
const PROBE_PNG='assets/engines/ocr-probe.png',PROBE_PNG_SHA256='b978c564d7f48e172d6bf3fa52f18cb81938f69348dbf531165ca68ca6150b29';
const PACKS=[{id:'fonts',name:'readmd-fonts-2026.10.10-universal.readmd-pack',sha256:'037e01665e90fde0fc81adb8cda2093e5862c491794dcbe95544660479f07a69',bytes:39944763},
 {id:'typst',name:'readmd-typst-0.15.1-windows-x64.readmd-pack',sha256:'dc808794d6f8ef734156201db0bd4a18ce1b79fc03d03cd370ec4d8df21b37e3',bytes:31364775},
 {id:'ocr',name:'readmd-ocr-5.5.3-r1-windows-x64.readmd-pack',sha256:'2380fe54795283a24086b5147e06840050ec36b136bcc27c984c222793b1a26c',bytes:9873674}];
// Fixed crash directions; deliberately not caller-configurable.
const CRASH_DIRECTION={typst:'mcp',ocr:'http'};
const PHASES=[{label:'typst-normal',engine:'typst',ending:'normal',crashTransport:null,leases:['typst','fonts'],survivor:'http'},
 {label:'typst-owner-crash',engine:'typst',ending:'crash',crashTransport:'mcp',leases:['typst','fonts'],survivor:'http'},
 {label:'ocr-normal',engine:'ocr',ending:'normal',crashTransport:null,leases:['ocr'],survivor:'http'},
 {label:'ocr-owner-crash',engine:'ocr',ending:'crash',crashTransport:'http',leases:['ocr'],survivor:'mcp'}];
const LIMITS={totalMs:480000,phaseMs:90000,prepareMs:90000,cleanupMs:30000,barrierMs:20000,busyMs:200,cancelAckMs:200,pingMs:200,retirementMs:2000,stoppedMs:2000,suspendMaxMs:60000,clientTimeoutMs:120000,hostStopMs:25000,
 installedBytes:128*1024*1024,scopeBytes:256*1024*1024,smallEvidenceBytes:32*1024*1024,files:4096,responseBytes:8*1024*1024};
const sha=value=>crypto.createHash('sha256').update(value).digest('hex'),hash=file=>sha(fs.readFileSync(file));
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
function inside(base,file){const actual=path.resolve(file),rel=path.relative(base,actual);assert(rel&&rel!=='..'&&!rel.startsWith('..'+path.sep)&&!path.isAbsolute(rel),'Outside owned fault-lane scope');return actual;}
function argumentsFor(argv,env=process.env){const out={candidate:env.READMD_MCP_CANDIDATE,python:env.READMD_TEST_PYTHON};for(let n=0;n<argv.length;n++){const key=argv[n];if(key==='--plan'){out.plan=true;continue;}if(key==='--case'){assert(n+1<argv.length);out.case=argv[++n];continue;}assert(['--candidate','--python','--output'].includes(key),'Unknown '+key);assert(n+1<argv.length);out[key.slice(2)]=argv[++n];}for(const key of ['candidate','python','output'])if(out[key])out[key]=path.resolve(out[key]);if(out.output){inside(UPGRADE,out.output);assert.equal(path.dirname(out.output),UPGRADE,'Owned directory must be a direct child of .cache/capability-upgrade');assert(/^batch3-faults-native-[A-Za-z0-9-]+$/.test(path.basename(out.output)),'Owned directory must be batch3-faults-native-...');}if(out.case)assert(PHASES.some(row=>row.label===out.case),'Unknown case '+out.case+' (expected one of '+PHASES.map(row=>row.label).join(', ')+')');if(!out.plan&&!out.case)for(const key of ['candidate','python','output'])assert(out[key],'Required --'+key);return out;}
function transportsFor(entry){const phase4=entry.label==='ocr-owner-crash';return {twoOwners:['mcp','http'],thirdRequest:['mcp','http'],newOwner:'mcp',siblingClose:phase4?'mcp':'http',phase4HasSurvivingHttpOwner:false};}
function planFor(){return {schemaVersion:1,scope:'Windows fault step2: cross-transport owners, four serial typst/ocr phases',noProcessesStarted:true,noFilesWritten:true,
 crashDirection:{...CRASH_DIRECTION,configurable:false},phases:structuredClone(PHASES).map(entry=>({...entry,transports:transportsFor(entry)})),
 transportAssignments:{ownerA:'mcp stdio (readmd.exe --mcp, multi-instance)',ownerB:'http host (Node->Python broker->host, one per phase)',thirdRequestPerTransport:['mcp','http'],newOwner:'mcp stdio only (http cannot start while its host lives or instance.json remains)',siblingClosePhases1To3:'http POST /api/task/cancel',phase4Close:'mcp notifications/cancelled (no surviving HTTP owner)'},
 limits:{...LIMITS},packs:structuredClone(PACKS),installedBytesLimit:LIMITS.installedBytes,installedBytesMeasuredAtInstall:true,
 lockedPacksDirectory:'.cache/capability-upgrade/locked-packs',lockedFiles:[...FILES],
 observer:{helpers:['ui-tests/helpers/windows-worker-observer.cs','ui-tests/helpers/windows-worker-observer.ps1'],qaRootPrefix:'batch3-faults-native-',actions:['ready','arm','observe','alive','freeSlots','stopped','jobAccounting','lease','resume'],armBeforeFirstEngineTask:true,jobAccounting:'cumulative per-owner private Job, positively calibrated'},
 recovery:{newOwnerOccupiesFreedSlot:true,mcpCancellation:'notifications/cancelled with independent ping',httpCancellation:'POST /api/task/cancel cancelling ack then final cancelled',typstPdfCheck:'tools/test/inspect-windows-resource-wait.py pdf (marker/glyph/link)',ocrTextCheck:'tools/test/inspect-windows-faults.py ocr (READMD and 12345)'},
 observations:['Per-phase busy latency through both transports with unchanged owner Job totals','Killed owner worker tree stops within 2s while the sibling worker and its Shared engine leases survive','Lease probes per engine: typst phases hold typst.lock and fonts.lock, ocr phases hold ocr.lock only','New MCP owner occupies the freed slot, is cancelled explicitly, then really succeeds after recovery','Phase 4 HTTP host killed directly: broker crash report recorded as-is, instance.json residue recorded, never deleted','Three packages uninstalled by a direct-spawn cleanup host over a dead instance.json'],
 limitations:['This lane does not certify steps 1,3,4,5 of the Windows fault plan, ENOSPC, eight-hour runs, other platforms or GUI.',
  'The third HTTP export request passes through key_with_data_checked before capacity is checked; a >200ms first failure is kept, not retried away.',
  'Phase 4 records the crash shape and residue truthfully; the broker non-zero exit is not a harness failure.',
  'No instance.json is ever deleted to obtain a pass.']};}
function caseFor(label){const entry=PHASES.find(row=>row.label===label);assert(entry,'Unknown case '+label+' (expected one of '+PHASES.map(row=>row.label).join(', ')+')');return {schemaVersion:1,noProcessesStarted:true,noFilesWritten:true,label,phase:{...structuredClone(entry),transports:transportsFor(entry)},crashDirection:CRASH_DIRECTION[entry.engine],limits:{...LIMITS},packs:structuredClone(PACKS),lockedFiles:[...FILES]};}
function leaseResourcesFor(engine){return engine==='typst'?['typst','fonts']:['ocr'];}
function validateLease(value,resource){assert.equal(value.resource,resource);assert.equal(value.held,true);assert.equal(value.exclusiveWinError,32,'Lease probe must be a sharing violation');assert.equal(value.identityBytes,0);assert(value.identity&&value.identity.bytes===0&&value.identity.index&&value.identity.created,'Lease identity must be the empty stable lock file');return true;}
function validateObserve(value,expectedSha){assert.equal(value.compileOrRecognition,true);assert.equal(value.physicalSuspension,true);assert.equal(value.genuineEngineSha256,expectedSha,'Observed engine bytes changed');assert(Number.isInteger(value.threads)&&value.threads>0);return true;}
function validateAccountingUnchanged(before,after){assert.equal(after.totalProcesses,before.totalProcesses,'Owner started an engine or version subprocess inside the busy window');assert.equal(after.ownerAlive,true);return true;}
function validateAccountingGrowth(before,after){assert(after.totalProcesses>=before.totalProcesses+1,'Cumulative Job accounting was not positively calibrated by a real engine subprocess');assert.equal(after.ownerAlive,true);return true;}
function validateMcpBusy(result,engine,elapsedMs){assert.equal(result.isError,true);const value=result.structuredContent||JSON.parse(result.content[0].text);assert.equal(value.error_code,engine+'_busy');assert(Number.isFinite(elapsedMs)&&elapsedMs>=0&&elapsedMs<=LIMITS.busyMs,'MCP busy request exceeded 200ms');return true;}
function validateHttpBusy(response,engine){assert.equal(response.body.error_code,engine+'_busy');assert.equal(response.status,engine==='ocr'?409:200);assert(Number.isFinite(response.elapsedMs)&&response.elapsedMs>=0&&response.elapsedMs<=LIMITS.busyMs,'HTTP busy request exceeded 200ms');return true;}
function validateCancelAck(response){assert.equal(response.status,200);assert.deepEqual(response.body,{ok:true,state:'cancelling'});assert(Number.isFinite(response.elapsedMs)&&response.elapsedMs>=0&&response.elapsedMs<=LIMITS.cancelAckMs,'HTTP cancellation acknowledgement exceeded 200ms');return true;}
function validateCancelledFinal(response,engine){assert.equal(response.status,engine==='ocr'?422:200,'Cancelled final must anchor to the mapped status');assert.equal(response.body.ok,false);assert.equal(response.body.error_code,'cancelled');return true;}
function validateCrashReport(report){assert.equal(report.passed,false);assert(Number.isInteger(report.hostExitCode)&&report.hostExitCode!==0,'Crashed host exit code must be non-zero');assert.equal(report.instanceRemoved,false);assert(typeof report.error==='string'&&report.error.includes('Owned HTTP host exited before shutdown'),'Broker did not report the expected crash shape');return true;}
function validateResume(value){assert.equal(value.processHandleKept,true,'Resume must keep the pinned worker handle');assert(Number.isInteger(value.resumed)&&value.resumed>=1,'Resume must release the threads this observer suspended');return true;}
function validateStopped(value){assert.equal(value.stopped,true);assert(Number.isFinite(value.waitMs)&&value.waitMs>=0&&value.waitMs<=LIMITS.stoppedMs,'Worker did not stop within 2s');return true;}
class CancellableClient extends StdioClient{
 begin(name,args,id,timeout=LIMITS.clientTimeoutMs){assert(!this.pending.has(id));const promise=new Promise((resolve,reject)=>{const timer=setTimeout(()=>this.transport.fail(Error('MCP request total deadline'),'request_deadline'),timeout);this.pending.set(id,{resolve,reject,timer});this.send({jsonrpc:'2.0',id,method:'tools/call',params:{name,arguments:args}});});promise.catch(()=>{});return promise;}
 cancel(id){const item=this.pending.get(id);assert(item,'No in-flight MCP request');this.pending.delete(id);clearTimeout(item.timer);this.send({jsonrpc:'2.0',method:'notifications/cancelled',params:{requestId:id,reason:'Owned Windows fault-lane cancellation'}});const error=Error('Client waiter cancelled; no original server result permitted');error.name='AbortError';item.reject(error);}
}
function sourceFor(label){const marker='FAULTLANE'+label.replaceAll('-','').toUpperCase();return {marker,content:'# '+marker+'\r\n\r\n'+marker+' visible cross-owner recovery proof.\r\n\r\n[FAULTLANELINK](https://example.invalid/resource-wait)\r\n\r\n---\r\n\r\n$\\frac{a+1}{b+2}$\r\n'};}
function longSource(label){const marker='FAULTLANEBARRIER'+label.replaceAll('-','').toUpperCase();const body=Array.from({length:1200},(_,i)=>'## 故障研究 '+i+'\r\n\r\n保留原始观察与研究数据，核对数字、引用和结论。\r\n\r\n$\\frac{α+x^2}{\\sqrt{y}}$\r\n\r\n| 观察 | 核对 |\r\n|---|---|\r\n| 保留原文 | 检查前提 |\r\n\r\n').join('');return {marker,content:'# '+marker+'\r\n\r\n'+body+'\r\n\r\n[链接](https://example.invalid/resource-wait)\r\n'};}
function optionsFor(){return {pdf:{engine:'typst'},typography:{font:'Arial',size:12,align:'left',firstLineIndent:0},cover:{enabled:false},toc:{enabled:false},header:{text:''},footer:{text:'',pageNumbers:false}};}
function exportArgs(content,target,documents){return {markdown_content:content,output_path:target,output_format:'pdf',base_dir:documents,options:optionsFor(),overwrite:true,confirm:true};}
function httpBody(content,target,documents,id){return {format:'pdf',content,baseDir:documents,suggestedName:id,options:optionsFor(),task_id:id,out_path:target};}
async function until(action,label,ms=8000){const end=performance.now()+ms;while(performance.now()<end){const value=await action();if(value)return value;await delay(20);}throw Error('State observation deadline: '+label);}
function seal(base){const rows=[];let total=0;const walk=(dir,rel)=>{for(const name of fs.readdirSync(dir).sort()){if(['profile','documents','scratch'].includes(name)&&fs.statSync(path.join(dir,name)).isDirectory())continue; // engine install, materials and scratch are budgets, not evidence
  const file=path.join(dir,name),info=fs.lstatSync(file);assert(!info.isSymbolicLink());if(info.isDirectory())walk(file,rel?rel+'/'+name:name);else{assert(info.isFile());assert(info.size<=LIMITS.smallEvidenceBytes,'Single small evidence file exceeded 32MiB: '+name);total+=info.size;assert(rows.length<LIMITS.files&&total<=LIMITS.scopeBytes,'Evidence budget');rows.push({path:(rel?rel+'/':'')+name,sha256:hash(file),bytes:info.size});}}};
 walk(base,'');
 return {rows,totalBytes:total};}
function bindingFor(options){require('./freeze-longtail-candidate.cjs').verify(options.candidate);const candidate=JSON.parse(fs.readFileSync(options.candidate)),sourceFreeze=inside(ROOT,path.resolve(ROOT,candidate.sourceFreeze));assert.equal(hash(sourceFreeze),candidate.sourceFreezeSha256);const manifest=JSON.parse(fs.readFileSync(sourceFreeze)),boundFiles={};for(const name of FILES){const row=manifest.sources.find(row=>row.path===name);assert(row,'Candidate did not freeze '+name);boundFiles[name]={file:inside(ROOT,path.join(ROOT,name)),copy:inside(ROOT,path.join(ROOT,row.copy)),sha256:row.sha256};}const value={candidate:options.candidate,candidateSha256:hash(options.candidate),binary:candidate.binary,binarySha256:candidate.binarySha256,assets:path.dirname(candidate.boot),boot:candidate.boot,bootSha256:candidate.bootSha256,sourceFreeze,sourceFreezeSha256:candidate.sourceFreezeSha256,boundFiles};verifyBinding(value);return value;}
// The external QA Python launcher cannot start with an overlong TEMP. Keep
// its scratch separate while the actual product and engines retain long paths.
function oracleEnvironment(engineEnv, temporary){
 assert(typeof temporary==='string'&&path.isAbsolute(temporary));
 return {...engineEnv,TMP:temporary,TEMP:temporary,TMPDIR:temporary};
}
module.exports={argumentsFor,planFor,caseFor,PHASES,PACKS,FILES,OBSERVER_FILES,LIMITS,CRASH_DIRECTION,PROBE_PNG,PROBE_PNG_SHA256,transportsFor,leaseResourcesFor,validateLease,validateObserve,validateAccountingUnchanged,validateAccountingGrowth,validateMcpBusy,validateHttpBusy,validateCancelAck,validateCancelledFinal,validateCrashReport,validateResume,validateStopped,sourceFor,longSource,optionsFor,exportArgs,httpBody,CancellableClient,inside,requestHttp,sha,until,oracleEnvironment};

class PsObserver{
 constructor(spawnOptions){this.seq=0;this.pending=new Map();this.buffer='';this.rescue='';this.failure=null;
  this.child=spawn(path.join(process.env.SystemRoot,'System32/WindowsPowerShell/v1.0/powershell.exe'),['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',path.join(ROOT,'ui-tests/helpers/windows-worker-observer.ps1'),'-QaRoot',spawnOptions.evidence,'-NodeProcessId',String(process.pid),'-QaRootPrefix','batch3-faults-native-'],{windowsHide:true,stdio:['pipe','pipe','pipe'],env:spawnOptions.env});
  this.closed=new Promise(resolve=>this.child.once('close',(code,signal)=>{this.exit={code,signal};resolve();}));
  this.child.on('error',error=>{this.failure=error;for(const waiter of this.pending.values())waiter.reject(error);this.pending.clear();});
  this.child.stderr.on('data',chunk=>{this.rescue+=chunk.toString();});
  this.child.stdout.on('data',chunk=>{try{this.buffer+=chunk.toString();for(;;){const at=this.buffer.indexOf('\n');if(at<0)break;const line=this.buffer.slice(0,at);this.buffer=this.buffer.slice(at+1);if(!line.trim())continue;const message=JSON.parse(line);const waiter=this.pending.get(message.id);assert(waiter,'Uncorrelated observer receipt');this.pending.delete(message.id);clearTimeout(waiter.timer);message.error?waiter.reject(Error(message.error)):waiter.resolve(message.result);}}catch(error){this.failure=error;for(const waiter of this.pending.values())waiter.reject(error);this.pending.clear();}});}
 request(action,params={},timeout=80000){assert(!this.exit,'Observer already exited');if(this.failure)throw this.failure;const id=++this.seq;return new Promise((resolve,reject)=>{const timer=setTimeout(()=>{this.pending.delete(id);reject(Error('Observer deadline '+action));},timeout);this.pending.set(id,{resolve,reject,timer});this.child.stdin.write(JSON.stringify({id,action,params})+'\n');});}
 async finish(){this.child.stdin.end();await Promise.race([this.closed,delay(10000).then(()=>{throw Error('Observer EOF deadline');})]);assert.equal(this.exit.code,0,'Observer exited with '+this.exit.code);assert.equal(this.rescue.trim(),'','Observer needed rescue');assert(!this.buffer.trim(),'Observer stdout ended mid-frame');}
 async abort(){if(!this.exit){this.child.kill();await Promise.race([this.closed,delay(3000)]);}}
}

async function main(){
 const options=argumentsFor(process.argv.slice(2));
 if(options.plan){console.log(JSON.stringify(planFor(),null,2));return;}
 if(options.case){console.log(JSON.stringify(caseFor(options.case),null,2));return;}
 assert.equal(process.platform,'win32','Windows-only fault lane');
 const binding=bindingFor(options);assert(fs.statSync(options.python).isFile());
 const packs=PACKS.map(pack=>{const file=path.join(UPGRADE,'locked-packs',pack.name);assert.equal(fs.statSync(file).size,pack.bytes);assert.equal(hash(file),pack.sha256);return {...pack,path:file};});
 fs.mkdirSync(options.output,{recursive:false});const evidence=options.output;
 const segment='资料'.repeat(35),run=path.join(evidence,segment,segment,segment,segment);
 const profile=path.join(run,'profile'),documents=path.join(run,'documents'),scratch=path.join(run,'scratch'),outputs=path.join(evidence,'outputs');
 assert(profile.length>300,'Owned profile must stay a long UTF-16 path');
 for(const name of [profile,documents,scratch,outputs])fs.mkdirSync(name,{recursive:true});
 for(const name of ['requests','responses','inspection','oracle-temp'])fs.mkdirSync(path.join(evidence,name),{recursive:true});
 const engineEnv={...systemEnvironment(profile,documents),TMP:scratch,TEMP:scratch,TMPDIR:scratch};
 const oracleEnv=oracleEnvironment(engineEnv,path.join(evidence,'oracle-temp'));
 const retain=(name,value)=>{const file=inside(evidence,path.join(evidence,name));fs.writeFileSync(file,Buffer.isBuffer(value)||typeof value==='string'?value:JSON.stringify(value,null,2)+'\n',{flag:'wx'});return {path:name.replaceAll(path.sep,'/'),sha256:hash(file),bytes:fs.statSync(file).size};};
 const report={schemaVersion:1,scope:planFor().scope,passed:false,startedUtc:new Date().toISOString(),candidateBinding:binding,python:{path:options.python,sha256:hash(options.python)},packs,phases:[],sessions:[],checks:[],calls:[],http:[],limitations:planFor().limitations};
 let host=null,observer=null;const clients=new Set();const cleanupFns={};
 let stage='prepare',watchdog=false;const started=performance.now();
 const timer=setTimeout(()=>{watchdog=true;for(const client of clients)client.forceStop();if(host){try{host.child.stdin.end('shutdown\n');}catch{}}if(observer){try{observer.child.kill();}catch{}}},LIMITS.totalMs);
 try{
  const serialCounter={n:0};const nextSerial=()=>++serialCounter.n;
  const engineProgram=engine=>path.join(profile,'capabilities/managed-v2',engine,engine==='typst'?'0.15.1':'5.5.3-r1','bin',engine==='typst'?'typst.exe':'tesseract.exe');
  const snapshotBytes=file=>({sha256:hash(file),bytes:fs.statSync(file).size});
  const protectTarget=file=>{fs.writeFileSync(file,'OWNED OLD FAULT-LANE TARGET\r\n'+binding.binarySha256+'\r\n',{flag:'wx'});return snapshotBytes(file);};
  const assertUnchanged=(file,before)=>{const after=snapshotBytes(file);assert.deepEqual(after,before,'Old target bytes changed');return after;};
  const partLeaks=()=>fs.readdirSync(outputs).filter(name=>name.endsWith('.readmd-part'));
  const taskJournals=()=>{const directory=path.join(profile,'worker-tasks-v1');if(!fs.existsSync(directory))return[];return fs.readdirSync(directory).filter(name=>name.length===32||name.startsWith('retiring-')).map(name=>({id:name}));};
  const oracle=(label,args,script=ORACLE)=>{const result=spawnSync(options.python,['-I','-B',path.join(ROOT,script),...args],{windowsHide:true,timeout:60000,maxBuffer:32*1024*1024,env:oracleEnv});retain('inspection/'+label+'.stdout.json',result.stdout||Buffer.alloc(0));retain('inspection/'+label+'.stderr.log',result.stderr||Buffer.alloc(0));assert(!result.error,result.error?.message);assert.equal(result.status,0,'Independent oracle failed: '+label);return JSON.parse(result.stdout);};
  const httpCall=async(origin,method,route,body,timeout=10000)=>{verifyBinding(binding);const index=nextSerial(),request=retain('requests/'+index+'-http.json',{method,route,body}),value=await requestHttp(origin,method,route,body,timeout),response=retain('responses/'+index+'-http.json',{status:value.status,headers:value.headers,body:value.body,elapsedMs:value.elapsedMs});report.http.push({index,method,route,request,response,elapsedMs:value.elapsedMs});verifyBinding(binding);return value;};
  const mcpCall=async(client,name,args,{id,timeout=30000}={})=>{verifyBinding(binding);assertToolCallSchema({name,arguments:args},client.tools);const index=nextSerial(),request=retain('requests/'+index+'-mcp.json',{jsonrpc:'2.0',id:id??client.next+1,method:'tools/call',params:{name,arguments:args}}),t0=performance.now();const value=id===undefined?await client.tool(name,args,timeout):await client.begin(name,args,id,timeout);retain('responses/'+index+'-mcp.json',{isError:value?.isError??false,structuredContent:value?.structuredContent??null,text:value?.content?String(value.content[0].text).slice(0,262144):null});report.calls.push({index,name,pid:client.info.pid,startedMs:t0,finishedMs:performance.now(),elapsedMs:performance.now()-t0,request});verifyBinding(binding);return value;};
  const startMcp=async wire=>{fs.mkdirSync(path.join(run,wire),{recursive:true});const client=new CancellableClient({binary:binding.binary,assets:binding.assets,profile,documents,out:path.join(run,wire),env:{TMP:scratch,TEMP:scratch,TMPDIR:scratch}});clients.add(client);await client.initialize(true);for(const name of ['readmd_export_document','readmd_ocr_to_markdown'])assert(client.tools.some(row=>row.name===name));retain('inspection/'+wire+'-tools.json',client.tools);return client;};
  const stopMcp=async client=>{const record={passed:true};await client.stop(record);assert(record.passed&&record.session.hostExit.code===0&&!record.session.forcedStop,'MCP owner did not close normally');report.sessions.push({transport:'mcp',pid:record.session.pid,hostExitCode:record.session.hostExit.code,forcedStop:record.session.forcedStop});clients.delete(client);};
  const startHttp=async wire=>{fs.mkdirSync(path.join(run,wire),{recursive:true});host=new HttpHost({python:options.python,binary:binding.binary,assets:binding.assets,profile,documents,out:path.join(run,wire),port:await freePort()});await host.ready();const created=await until(async()=>{const raw=fs.readFileSync(path.join(run,wire,'broker.stdout.jsonl'),'utf8'),lines=raw.split('\n').slice(0,-1).filter(Boolean).map(line=>JSON.parse(line)),found=lines.filter(row=>row.event==='host-created');assert(found.length<=1);return found[0];},'actual HTTP host PID receipt');host.actualPid=created.pid;return host;};
  const stopHttp=async()=>{const value=await host.stop();report.sessions.push({transport:'http',pid:value.pid,hostExitCode:value.hostExitCode,instanceRemoved:value.instanceRemoved,forcedStop:value.forcedStop,passed:value.passed});host=null;return value;};
  // The finally block lives outside this try scope; hand it the two cleanup
  // helpers explicitly instead of re-declaring them at function scope.
  cleanupFns.stopMcp=stopMcp;cleanupFns.stopHttp=stopHttp;
  // Phase-4 crash finishing reuses HttpHost's process objects but never its
  // stop() assertions: a killed host cannot satisfy passed/hostExitCode=0.
  const crashFinishHttp=async()=>{const current=host;host=null;const killedAt=performance.now();process.kill(current.actualPid);await current.closed;const reportFile=path.join(current.options.out,'broker-report.json');const crash=await until(async()=>{if(!fs.existsSync(reportFile))return null;try{return JSON.parse(fs.readFileSync(reportFile,'utf8'));}catch{return null;}},'broker crash report receipt',LIMITS.hostStopMs);validateCrashReport(crash);for(const fd of Object.values(current.logs))fs.closeSync(fd);const instanceJson=path.join(profile,'instance.json');return {crashReport:crash,brokerExitCode:current.exit?.code??null,instanceJsonPresent:fs.existsSync(instanceJson),instanceJsonDeletedByHarness:false,killMs:performance.now()-killedAt};};
  const startDirectHost=async wire=>{const log=fs.openSync(path.join(run,wire+'.log'),'wx');const child=spawn(binding.binary,['--no-window','--port','-1','--data-dir',profile,'--workspace',documents,'--assets',binding.assets],{windowsHide:true,stdio:['ignore','pipe',log],env:engineEnv});fs.closeSync(log);const closed=new Promise(resolve=>child.once('close',resolve));let buffer='';child.stdout.on('data',chunk=>{buffer+=chunk.toString();});const origin=await until(async()=>{if(child.exitCode!==null)throw Error('Direct cleanup host exited early');const match=buffer.match(/ listening on (http:\/\/127\.0\.0\.1:\d+)/);return match?match[1]:null;},'direct cleanup host startup',30000);report.sessions.push({transport:'direct-spawn-host',pid:child.pid,purpose:wire});return {child,origin,closed};};

  stage='install pinned packages through a real HTTP owner';const prepareStart=performance.now();
  await startHttp('wire-setup-http');
  for(const pack of packs){const value=await httpCall(host.origin,'POST','/api/capabilities/packs',{action:'install',id:pack.id,path:pack.path},120000);assert.equal(value.status,200);assert.equal(value.body.ok,true,'Offline private package install failed');assert.equal(value.body.id,pack.id);assert.equal(value.body.installed,true);}
  // Inventory the installed bytes for real and compare against the hard cap.
  const inventory=(()=>{const base=path.join(profile,'capabilities/managed-v2');const rows=[];let total=0;(function walk(dir){for(const name of fs.readdirSync(dir).sort()){const file=path.join(dir,name),info=fs.lstatSync(file);assert(!info.isSymbolicLink(),'Symlink inside managed profile');if(info.isDirectory()){if(dir===base&&name==='.locks')continue;walk(file);}else{total+=info.size;rows.push({path:path.relative(base,file).replaceAll(path.sep,'/'),bytes:info.size});}}})(base);return {files:rows.length,bytes:total,rows};})();
  assert(inventory.bytes<=LIMITS.installedBytes,'Installed packages exceed 128MiB');retain('inspection/installed-inventory.json',inventory);report.inventory=inventory;
  stage='derive OCR material and prove one real recognition';const tile=path.join(documents,'fault-ocr-tile.png');
  const tileSummary=oracle('ocr-tile-derivation',['tile','--source',path.join(ROOT,PROBE_PNG),'--output',tile]);
  assert.equal(tileSummary.sourceSha256,PROBE_PNG_SHA256);assert.equal(tileSummary.width,3000);assert.equal(tileSummary.height,3500);report.ocrMaterial=tileSummary;
  const firstOcr=await httpCall(host.origin,'GET','/api/ocr?p='+encodeURIComponent(tile)+'&task_id=setup-first-recognition',undefined,LIMITS.clientTimeoutMs);
  assert.equal(firstOcr.status,200);assert.equal(firstOcr.body.empty??false,false);retain('inspection/first-recognition.md',firstOcr.body.content);const firstOcrProof=oracle('first-recognition-text',['ocr','--input',path.join(evidence,'inspection/first-recognition.md')]);assert.equal(firstOcrProof.passed,true);report.firstRecognition={status:firstOcr.status,tokens:firstOcrProof.tokens};
  await stopHttp();report.checks.push({name:'Three pinned packages installed (measured '+inventory.bytes+'B <= 128MiB), OCR material derived and really recognized once'});
  assert(performance.now()-prepareStart<=LIMITS.prepareMs,'Preparation exceeded 90s');report.prepareMs=performance.now()-prepareStart;
  const protectedPdf=path.join(outputs,'protected.pdf'),protectedBefore=protectTarget(protectedPdf);
  const sentinel=path.join(documents,'unknown-sentinel.txt');fs.writeFileSync(sentinel,'Do not remove this unrelated owned sentinel.\r\n',{flag:'wx'});const sentinelBefore=snapshotBytes(sentinel);const tileBefore=snapshotBytes(tile);
  stage='observer';observer=new PsObserver({evidence,env:engineEnv});assert.equal((await observer.request('ready',{echo:'故障'})).nodeAncestor,true);
 report.observer={helpers:OBSERVER_FILES.map(name=>({path:name,sha256:hash(path.join(ROOT,name)),bytes:fs.statSync(path.join(ROOT,name)).size})),qaRootPrefix:'batch3-faults-native-'};

  for(let index=0;index<PHASES.length;index++){
   const entry=PHASES[index],engine=entry.engine,typst=engine==='typst',program=engineProgram(engine),programSha=hash(program);
   const phaseStart=performance.now();
   const record={label:entry.label,engine,index,ending:entry.ending,crashTransport:entry.crashTransport,leases:{}};report.phases.push(record);
   stage=entry.label+' owners';
   const A=await startMcp('wire-'+entry.label+'-mcp');
   const aArm=await observer.request('arm',{ownerPid:A.info.pid,ownerProgram:binding.binary,program,sha256:programSha,kind:engine});
   const B=await startHttp('wire-'+entry.label+'-http');
   const bArm=await observer.request('arm',{ownerPid:B.actualPid,ownerProgram:binding.binary,program,sha256:programSha,kind:engine,ownerParentPid:B.child.pid});
   record.owners={mcp:{pid:A.info.pid},http:{pid:B.actualPid,brokerPid:B.child.pid}};
   const aBase=await observer.request('jobAccounting',{key:aArm.key}),bBase=await observer.request('jobAccounting',{key:bArm.key});
   assert(aBase.totalProcesses===1&&aBase.activeProcesses===1);assert(bBase.totalProcesses===1&&bBase.activeProcesses===1);
   const aTarget=path.join(outputs,entry.label+'-a.pdf'),bTarget=path.join(outputs,entry.label+'-b.pdf');
   const bBefore=protectTarget(bTarget);const aBefore=entry.ending==='crash'?protectTarget(aTarget):null;
   stage=entry.label+' two real workers';
   const aId='fault-'+entry.label+'-a',bId='fault-'+entry.label+'-b';
   const aTask=typst?A.begin('readmd_export_document',exportArgs(longSource(entry.label+'-a').content,aTarget,documents),aId):A.begin('readmd_ocr_to_markdown',{file_path:tile},aId);
   const aObserveStart=performance.now();const aObserved=await observer.request('observe',{key:aArm.key});record.observeMs={mcp:performance.now()-aObserveStart};assert(record.observeMs.mcp<=LIMITS.barrierMs,'MCP observe barrier exceeded 20s');validateObserve(aObserved,programSha);
   const bPending=typst?httpCall(B.origin,'POST','/api/export',httpBody(longSource(entry.label+'-b').content,bTarget,documents,bId),LIMITS.clientTimeoutMs):httpCall(B.origin,'GET','/api/ocr?p='+encodeURIComponent(tile)+'&task_id='+encodeURIComponent(bId),undefined,LIMITS.clientTimeoutMs);bPending.catch(()=>{});
   const bObserveStart=performance.now();const bObserved=await observer.request('observe',{key:bArm.key});record.observeMs.http=performance.now()-bObserveStart;assert(record.observeMs.http<=LIMITS.barrierMs,'HTTP observe barrier exceeded 20s');validateObserve(bObserved,programSha);
   const aEngine=await observer.request('jobAccounting',{key:aArm.key}),bEngine=await observer.request('jobAccounting',{key:bArm.key});
   validateAccountingGrowth(aBase,aEngine);validateAccountingGrowth(bBase,bEngine);
   record.accounting={mcp:{armed:aBase,engine:aEngine},http:{armed:bBase,engine:bEngine}};
   stage=entry.label+' third request refused by both transports';
   const thirdBusy={};
   {const index=nextSerial();retain('requests/'+index+'-mcp.json',{jsonrpc:'2.0',method:'tools/call',params:{name:typst?'readmd_export_document':'readmd_ocr_to_markdown',arguments:typst?exportArgs(longSource(entry.label+'-third').content,path.join(outputs,entry.label+'-third.pdf'),documents):{file_path:tile}}});const t0=performance.now();const result=await A.tool(typst?'readmd_export_document':'readmd_ocr_to_markdown',typst?exportArgs(longSource(entry.label+'-third').content,path.join(outputs,entry.label+'-third.pdf'),documents):{file_path:tile},20000);thirdBusy.mcpMs=performance.now()-t0;validateMcpBusy(result,engine,thirdBusy.mcpMs);retain('responses/'+index+'-mcp.json',{isError:true,structuredContent:result.structuredContent??null,text:result.content?String(result.content[0].text).slice(0,4096):null,busyMs:thirdBusy.mcpMs});}
   {const t0=performance.now();const response=typst?await httpCall(B.origin,'POST','/api/export',httpBody(longSource(entry.label+'-third-http').content,path.join(outputs,entry.label+'-third-http.pdf'),documents,'fault-'+entry.label+'-third-http'),20000):await httpCall(B.origin,'GET','/api/ocr?p='+encodeURIComponent(tile)+'&task_id=fault-'+entry.label+'-third-http',undefined,20000);thirdBusy.httpMs=performance.now()-t0;validateHttpBusy(response,engine);}
   const aBusy=await observer.request('jobAccounting',{key:aArm.key}),bBusy=await observer.request('jobAccounting',{key:bArm.key});
   validateAccountingUnchanged(aEngine,aBusy);validateAccountingUnchanged(bEngine,bBusy);
   record.thirdRequest={mcpBusyMs:thirdBusy.mcpMs,httpBusyMs:thirdBusy.httpMs,accountingUnchanged:{mcp:aBusy,http:bBusy}};
   stage=entry.label+' ending owner';
   if(entry.ending==='normal'){
    const t0=performance.now();const resumed=await observer.request('resume',{key:aArm.key});record.resume={...resumed,elapsedMs:performance.now()-t0};validateResume(resumed);assert(record.resume.elapsedMs<=LIMITS.suspendMaxMs,'Resume exceeded the single-suspension bound');
    const completed=await aTask;record.ownerCompletion={isError:completed.isError===true};
    if(typst){const value=payload(completed);assert.equal(path.resolve(value.output_path),aTarget);record.ownerCompletion.output=snapshotBytes(aTarget);}
    else{const text=completed.content[0].text;retain('inspection/'+entry.label+'-a-ocr.md',text);record.ownerCompletion.tokens=oracle(entry.label+'-a-ocr-text',['ocr','--input',path.join(evidence,'inspection/'+entry.label+'-a-ocr.md')]).tokens;}
    const stopped=await observer.request('stopped',{key:aArm.key,timeoutMs:LIMITS.stoppedMs});record.ownerCompletion.workerStopped=stopped;validateStopped(stopped);
    await stopMcp(A);
   }else if(entry.crashTransport==='mcp'){
    const t0=performance.now();A.forceStop();clients.delete(A);const stopped=await observer.request('stopped',{key:aArm.key,timeoutMs:LIMITS.stoppedMs});record.crashMs=performance.now()-t0;validateStopped(stopped);record.crashedOwner={transport:'mcp',pid:A.info.pid,stopped};
    await assert.rejects(aTask,error=>error instanceof Error,'Crashed MCP owner request did not fail');
   }else{
    record.httpCrash=await crashFinishHttp();
    const stopped=await observer.request('stopped',{key:bArm.key,timeoutMs:LIMITS.stoppedMs});record.crashStoppedMs=stopped.waitMs;validateStopped(stopped);record.crashedOwner={transport:'http',pid:B.actualPid,stopped};
   }
   stage=entry.label+' sibling survival and engine leases';
   const survivorKey=entry.survivor==='http'?bArm.key:aArm.key;record.survivor={transport:entry.survivor};
   assert.equal((await observer.request('alive',{key:survivorKey})).alive,true,'Sibling owner worker did not survive');
   for(const resource of leaseResourcesFor(engine)){const lease=await observer.request('lease',{key:survivorKey,resource});validateLease(lease,resource);record.leases[resource]=lease;}
   record.leaseResources=leaseResourcesFor(engine);
   if(entry.ending==='crash')assertUnchanged(aTarget,aBefore);
   assertUnchanged(bTarget,bBefore);assert.deepEqual(partLeaks(),[],'Staged part leaked into outputs');
   stage=entry.label+' new MCP owner';
   const D=await startMcp('wire-'+entry.label+'-new');
   const dArm=await observer.request('arm',{ownerPid:D.info.pid,ownerProgram:binding.binary,program,sha256:programSha,kind:engine});
   const dBase=await observer.request('jobAccounting',{key:dArm.key});
   record.newOwner={pid:D.info.pid,slotsAfterRecovery:await until(async()=>{const value=await observer.request('freeSlots',{key:dArm.key});return value.free>=1?value:null;},'freed slot recovery',LIMITS.retirementMs)};
   const dCancelTarget=path.join(outputs,entry.label+'-d-cancel.pdf');const dBefore=typst?protectTarget(dCancelTarget):null;
   const dId='fault-'+entry.label+'-d-cancel';
   const dTask=typst?D.begin('readmd_export_document',exportArgs(longSource(entry.label+'-d').content,dCancelTarget,documents),dId):D.begin('readmd_ocr_to_markdown',{file_path:tile},dId);
   const dObserved=await observer.request('observe',{key:dArm.key});validateObserve(dObserved,programSha);record.newOwner.observed=dObserved;
   // While the new owner really holds the freed slot only the frozen survivor
   // keeps the other one: zero free slots prove real occupancy.
   record.newOwner.slotsDuringNewTask=await observer.request('freeSlots',{key:dArm.key});
   assert.equal(record.newOwner.slotsDuringNewTask.free,0,'New owner did not really occupy the freed engine slot');
   const cancelStart=performance.now();D.cancel(dId);const pingStart=performance.now();assert.deepEqual(await D.request('ping',{},1000),{});record.newOwner.pingMs=performance.now()-pingStart;assert(record.newOwner.pingMs<=LIMITS.pingMs,'MCP ping exceeded 200ms');
   const dStopped=await observer.request('stopped',{key:dArm.key,timeoutMs:LIMITS.stoppedMs});validateStopped(dStopped);
   const dSlots=await until(async()=>{const value=await observer.request('freeSlots',{key:dArm.key});return value.free>=1?value:null;},'cancelled slot retirement',LIMITS.retirementMs);
   await assert.rejects(dTask,{name:'AbortError'});record.newOwner.cancellation={pingMs:record.newOwner.pingMs,retiredMs:performance.now()-cancelStart,stopped:dStopped,slots:dSlots,originalReplySuppressed:true};
   if(typst)assertUnchanged(dCancelTarget,dBefore);
   const retryOutput=path.join(outputs,entry.label+'-d-retry.pdf');
   if(typst){const material=sourceFor(entry.label+'-retry');const value=payload(await mcpCall(D,'readmd_export_document',exportArgs(material.content,retryOutput,documents),{timeout:90000}));assert.equal(path.resolve(value.output_path),retryOutput);const proof=oracle(entry.label+'-retry-pdf',['pdf','--input',retryOutput,'--marker',material.marker],PDF_ORACLE);assert.equal(proof.passed,true);record.newOwner.retry={pdf:snapshotBytes(retryOutput),proof};}
   else{const result=await mcpCall(D,'readmd_ocr_to_markdown',{file_path:tile},{timeout:90000});const text=result.content[0].text;retain('inspection/'+entry.label+'-retry-ocr.md',text);record.newOwner.retry={tokens:oracle(entry.label+'-retry-ocr-text',['ocr','--input',path.join(evidence,'inspection/'+entry.label+'-retry-ocr.md')]).tokens};}
   const dAfter=await observer.request('jobAccounting',{key:dArm.key});validateAccountingGrowth(dBase,dAfter);record.newOwner.accounting={armed:dBase,after:dAfter};
   await stopMcp(D);
   stage=entry.label+' close survivor';
   if(entry.survivor==='http'){
    const ackStart=performance.now();const ack=await httpCall(B.origin,'POST','/api/task/cancel',{id:bId},10000);validateCancelAck(ack);
    const final=await bPending;const finalMs=performance.now()-ackStart;validateCancelledFinal(final,engine);assert(finalMs<=LIMITS.retirementMs,'Cancelled final did not arrive within 2s of the ack');
    const survivorStopped=await observer.request('stopped',{key:bArm.key,timeoutMs:LIMITS.stoppedMs});validateStopped(survivorStopped);
    const survivorSlots=await until(async()=>{const value=await observer.request('freeSlots',{key:bArm.key});return value.free>=2?value:null;},'survivor slot retirement',LIMITS.retirementMs);
    assert.deepEqual(partLeaks(),[],'Cancelled export leaked a staged part');
    const stopRecord=await stopHttp();
    record.survivorClose={transport:'http',ackMs:ack.elapsedMs,finalMs,body:{status:final.status,error_code:final.body.error_code,canceled:final.body.canceled},stopped:survivorStopped,slots:survivorSlots,hostStop:{passed:stopRecord.passed,hostExitCode:stopRecord.hostExitCode,instanceRemoved:stopRecord.instanceRemoved}};
   }else{
    const cancelStart=performance.now();A.cancel(aId);const pingStart=performance.now();assert.deepEqual(await A.request('ping',{},1000),{});const pingMs=performance.now()-pingStart;assert(pingMs<=LIMITS.pingMs,'MCP ping exceeded 200ms');
    const survivorStopped=await observer.request('stopped',{key:aArm.key,timeoutMs:LIMITS.stoppedMs});validateStopped(survivorStopped);
    await until(async()=>{const value=await observer.request('freeSlots',{key:aArm.key});return value.free>=2?value:null;},'survivor slot retirement',LIMITS.retirementMs);
    await assert.rejects(aTask,{name:'AbortError'});
    record.survivorClose={transport:'mcp',pingMs,retiredMs:performance.now()-cancelStart,stopped:survivorStopped};
    await stopMcp(A);
   }
   record.phaseMs=performance.now()-phaseStart;assert(record.phaseMs<=LIMITS.phaseMs,'Phase '+entry.label+' exceeded 90s');
   record.journalsAtPhaseEnd=taskJournals().length;
  }

  stage='final cleanup';const cleanupStart=performance.now();
  report.cleanup={instanceJsonResidueBeforeCleanup:fs.existsSync(path.join(profile,'instance.json'))};
  const cleanupHost=await startDirectHost('wire-cleanup-host');
  const uninstalls=[];for(const id of ['ocr','typst','fonts']){const value=await httpCall(cleanupHost.origin,'POST','/api/capabilities/packs',{action:'uninstall',id},60000);assert.equal(value.status,200);assert.equal(value.body.ok,true,'Cleanup uninstall failed for '+id);uninstalls.push({id,ok:true});}
  report.cleanup.uninstalls=uninstalls;
  let journals=taskJournals();
  if(journals.length){const finalRecovery=await startMcp('wire-final-recovery');await until(()=>Promise.resolve(taskJournals().length===0),'next-start journal recovery',8000).catch(()=>null);journals=taskJournals();await stopMcp(finalRecovery);}
  assert(journals.length===0,'Owned task journals did not retire after the promised next-start recovery');
  report.cleanup.instanceJsonResidueAfterCleanupHost=fs.existsSync(path.join(profile,'instance.json'));
  const cleanupKill=performance.now();cleanupHost.child.kill();await Promise.race([cleanupHost.closed,delay(5000)]);report.cleanup.hostKilledMs=performance.now()-cleanupKill;
  report.cleanup.instanceJsonFinalResidue=fs.existsSync(path.join(profile,'instance.json'));
  report.cleanup.instanceJsonDeletedByHarness=false;report.cleanup.journalsRemaining=journals.map(row=>row.id);
  assert(performance.now()-cleanupStart<=LIMITS.cleanupMs,'Cleanup exceeded 30s');
  await observer.finish();observer=null;
  assertUnchanged(protectedPdf,protectedBefore);assertUnchanged(sentinel,sentinelBefore);assertUnchanged(tile,tileBefore);
  assert(!watchdog,'Total watchdog fired');verifyBinding(binding);for(const pack of packs)assert.equal(hash(pack.path),pack.sha256);
  const scope=(()=>{let total=0;for(const dir of [scratch,documents]){(function walk(d){for(const name of fs.readdirSync(d)){const file=path.join(d,name),info=fs.lstatSync(file);if(info.isDirectory())walk(file);else total+=info.size;}})(dir);}return total;})()+seal(evidence).totalBytes;
  assert(scope<=LIMITS.scopeBytes,'scratch+documents+evidence exceeded 256MiB');report.scopeBytes=scope;
  report.elapsedMs=performance.now()-started;assert(report.elapsedMs<=LIMITS.totalMs,'Matrix exceeded 8 minutes');
  report.passed=true;
 }catch(error){report.failedStage=stage;report.firstFailure=String(error.stack||error);process.exitCode=1;}
 finally{clearTimeout(timer);
  for(const client of [...clients])try{await cleanupFns.stopMcp(client);}catch(error){report.passed=false;report.mcpShutdownFailure??=String(error.stack||error);process.exitCode=1;}
  if(host)try{await cleanupFns.stopHttp();}catch(error){report.passed=false;report.httpShutdownFailure??=String(error.stack||error);process.exitCode=1;}
  if(observer)try{await observer.abort();report.observerAborted=true;}catch(error){report.observerShutdownFailure=String(error);}
  report.watchdogFired=watchdog;report.finishedUtc=new Date().toISOString();report.elapsedMs=performance.now()-started;
  try{report.evidenceArtifacts=seal(evidence).rows;}catch(error){report.sealFailure=String(error);report.passed=false;process.exitCode=1;}
  try{retain('report.json',report);}catch{}
  try{console.log(JSON.stringify({passed:report.passed,phases:report.phases.length,report:path.join(evidence,'report.json'),sha256:hash(path.join(evidence,'report.json')),failedStage:report.failedStage}));}catch{}
 }
}
if(require.main===module)main().catch(error=>{console.error(error.stack);process.exitCode=1;});
