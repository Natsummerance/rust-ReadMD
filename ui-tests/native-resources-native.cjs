'use strict';
// Native resource creation uses Windows APIs in an explicitly compiled helper.
// Recovery is performed by the actual application's next MCP startup.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {spawn,spawnSync}=require('node:child_process');
const root=path.resolve(__dirname,'..'),binary=path.resolve(process.env.READMD_BIN||'.cache/community-upgrade/build/debug/readmd.exe');
assert(process.env.READMD_WORKSPACE_TEST_BIN,'Explicit current compiled Rust helper required');
const helper=path.resolve(process.env.READMD_WORKSPACE_TEST_BIN),sha=p=>crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const qa=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/native-resources-native-'));
const data=path.join(qa,'profile'),docs=path.join(qa,'documents'),scratch=path.join(qa,'scratch');
for(const p of [data,docs,scratch])fs.mkdirSync(p);
const env={...process.env,PATH:path.join(process.env.SystemRoot,'System32'),TEMP:scratch,TMP:scratch,TMPDIR:scratch};
for(const key of Object.keys(env))if(key.startsWith('READMD_NATIVE_RESOURCE_')||['READMD_REQUIRE_TOKEN','READMD_DATA_DIR'].includes(key))delete env[key];
const report={passed:false,platform:process.platform,binarySha256:sha(binary),helperSha256:sha(helper),testSha256:sha(__filename),checks:[],phase:'fixtures',scope:'Actual Windows AppContainer/profile and DOS-device resources after successful ownership acknowledgement; independent live owner, real MCP next-start recovery and unknown record preservation. Not TeX generated files, pre-acknowledgement crashes or other platforms.'};
const children=[],delay=ms=>new Promise(r=>setTimeout(r,ms));
const taskDirs=()=>fs.existsSync(path.join(data,'worker-tasks-v1'))?fs.readdirSync(path.join(data,'worker-tasks-v1')).filter(n=>/^[0-9a-f]{32}$/.test(n)):[];
function check(name,detail={}){report.checks.push({name,passed:true,...detail});}
function launch(program,args,name,extra={}){
 const log=fs.openSync(path.join(qa,name+'.stderr.log'),'wx');
 const child=spawn(program,args,{cwd:root,env:{...env,...extra},windowsHide:true,stdio:['pipe','pipe',log]});fs.closeSync(log);children.push(child);return child;
}
async function stop(child){if(!child||child.exitCode!==null||child.signalCode!==null)return;const done=new Promise(r=>child.once('exit',r));child.kill();await Promise.race([done,delay(2000)]);assert(child.exitCode!==null||child.signalCode!==null,'Owned child exit deadline');}
async function until(fn,message){const until=performance.now()+10000;while(performance.now()<until){if(fn())return;await delay(20);}assert(fn(),message);}
async function owner(name){
 const base=path.join(qa,name);fs.mkdirSync(base);for(const dir of ['runtime','work'])fs.mkdirSync(path.join(base,dir));
 fs.writeFileSync(path.join(base,'runtime','original.marker'),'Original runtime — preserve me');
 const child=launch(helper,['--exact','worker_sandbox::platform::tests::native_resource_child','--nocapture'],name,{READMD_NATIVE_RESOURCE_CHILD:base,READMD_NATIVE_RESOURCE_PROFILE:data});
 const file=path.join(base,'ready.json');let receipt;
 await until(()=>{assert(child.exitCode===null&&child.signalCode===null,'Native owner exited before readiness');try{receipt=JSON.parse(fs.readFileSync(file));return true;}catch{return false;}},'Native owner readiness deadline');
 return{child,file,receipt,base};
}
function inspect(owner,absent){
 const run=spawnSync(helper,['--exact','worker_sandbox::platform::tests::native_resource_inspector_child','--nocapture'],{cwd:root,env:{...env,READMD_NATIVE_RESOURCE_INSPECT:owner.file,READMD_NATIVE_RESOURCE_EXPECT:absent?'absent':'present'},windowsHide:true,encoding:'utf8',timeout:10000});
 fs.writeFileSync(path.join(qa,owner.receipt.id+'-'+(absent?'absent':'present')+'-'+crypto.randomUUID()+'.log'),String(run.stdout||'')+String(run.stderr||''),{flag:'wx'});
 assert.equal(run.status,0,run.error?.message||run.stdout+run.stderr);assert(run.stdout.includes('NATIVE_RESOURCES_CHECKED '+(absent?'absent':'present')));
}
async function client(name){
 const child=launch(binary,['--mcp','--data-dir',data,'--workspace',docs,'--assets',path.join(root,'assets')],name);
 const log=fs.createWriteStream(path.join(qa,name+'.stdout.log'),{flags:'wx'});let text='',id=0,terminal;const pending=new Map();
 function fail(error){terminal=error;for(const p of pending.values()){clearTimeout(p.timer);p.reject(error);}pending.clear();}
 child.stdout.setEncoding('utf8');child.stdout.on('data',chunk=>{log.write(chunk);text+=chunk;if(text.length>1048576){fail(Error('MCP response limit'));return;}for(;;){const at=text.indexOf('\n');if(at<0)break;const line=text.slice(0,at);text=text.slice(at+1);let v;try{v=JSON.parse(line);}catch{continue;}const p=pending.get(v.id);if(p){clearTimeout(p.timer);pending.delete(v.id);v.error?p.reject(Error(JSON.stringify(v.error))):p.resolve(v.result);}}});
 child.on('error',fail);child.on('exit',()=>{fail(Error('MCP child exited'));log.end();});
 const call=(method,params)=>new Promise((resolve,reject)=>{if(terminal){reject(terminal);return;}const key=++id;pending.set(key,{resolve,reject,timer:setTimeout(()=>{pending.delete(key);reject(Error('MCP response deadline'));},10000)});child.stdin.write(JSON.stringify({jsonrpc:'2.0',id:key,method,params})+'\n');});
 await call('initialize',{protocolVersion:'2024-11-05',capabilities:{},clientInfo:{name:'native-resource-recovery-QA',version:'1'}});await call('ping',{});return{child,call};
}
(async()=>{try{
 assert.equal(process.platform,'win32');const first=await owner('first'),second=await owner('second');inspect(first,false);inspect(second,false);
 for(const o of [first,second]){const events=fs.readFileSync(path.join(data,'worker-tasks-v1',o.receipt.id,'record.jsonl'),'utf8').trim().split('\n').map(JSON.parse);assert.equal(events.filter(v=>v.type==='NativeResource').length,3);o.resourceView=path.join(events[0].value.root,'tex-view','resources');assert.equal(fs.readFileSync(path.join(o.resourceView,'original.marker'),'utf8'),'Original runtime — preserve me');}
 check('two-real-owners-have-three-durable-native-resources-each');
 check('ordinary-short-resource-views-read-original-runtime-without-copying');
 const unknown=crypto.randomUUID().replaceAll('-',''),unknownDir=path.join(data,'worker-tasks-v1',unknown);fs.mkdirSync(unknownDir);fs.writeFileSync(path.join(unknownDir,'record.jsonl'),'Unknown record — preserve me');fs.writeFileSync(path.join(unknownDir,'owner.lock'),'Unknown lease — preserve me');
 const unknownSha=sha(path.join(unknownDir,'record.jsonl')),unknownLeaseSha=sha(path.join(unknownDir,'owner.lock'));
 const live=await client('live-start');assert.equal(taskDirs().length,3);inspect(first,false);inspect(second,false);await stop(live.child);check('actual-MCP-startup-preserves-both-live-owners');
 report.phase='first-owner-crash';await stop(first.child);inspect(first,false);
 const restart=await client('restart');await until(()=>!taskDirs().includes(first.receipt.id),'Real startup did not reclaim stopped owner');inspect(first,true);inspect(second,false);check('actual-next-start-reclaims-crashed-profile-and-two-device-views');
 assert(taskDirs().includes(second.receipt.id));check('other-independent-owner-remains-usable');await stop(restart.child);
 await stop(second.child);const final=await client('final-start');await until(()=>taskDirs().length===1,'Second stopped owner not reclaimed');inspect(second,true);await stop(final.child);check('second-owner-reclaims-on-later-actual-start');
 assert.deepEqual(taskDirs(),[unknown]);assert.equal(sha(path.join(unknownDir,'record.jsonl')),unknownSha);assert.equal(sha(path.join(unknownDir,'owner.lock')),unknownLeaseSha);check('unknown-record-and-nonempty-lock-never-truncated');
 for(const o of [first,second]){assert(!fs.existsSync(o.resourceView));assert.equal(fs.readFileSync(path.join(o.base,'runtime','original.marker'),'utf8'),'Original runtime — preserve me');}check('recovering-short-resource-junctions-preserves-original-targets');
 const scratchChildren=fs.readdirSync(scratch).flatMap(n=>fs.readdirSync(path.join(scratch,n)));assert.equal(scratchChildren.length,0);check('known-native-resource-task-roots-reclaimed');report.passed=true;report.phase='complete';
 }catch(error){report.error=error.stack;process.exitCode=1;}finally{for(const child of children.reverse())await stop(child).catch(()=>{});fs.writeFileSync(path.join(qa,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify({qa:path.basename(qa),passed:report.passed,checks:report.checks.length,phase:report.phase,error:report.error}));}})();
