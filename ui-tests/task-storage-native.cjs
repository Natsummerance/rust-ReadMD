'use strict';
// Actual Rust workspace leases, packaged MCP, atomic target publication and
// restart recovery. The helper owns fixtures; it never supplies an MCP reply.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {spawn}=require('node:child_process');
const root=path.resolve(__dirname,'..'),binary=path.resolve(process.env.READMD_BIN||'.cache/community-upgrade/build/debug/readmd.exe');
const helper=path.resolve(process.env.READMD_WORKSPACE_TEST_BIN||'');
assert(process.env.READMD_WORKSPACE_TEST_BIN,'Explicit compiled Rust helper required');
const qa=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/task-storage-native-'));
const data=path.join(qa,'profile'),docs=path.join(qa,'documents'),scratch=path.join(qa,'scratch');
for(const p of [data,docs,scratch])fs.mkdirSync(p);
const sha=p=>crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const env={...process.env,PATH:path.join(process.env.SystemRoot,'System32'),TMP:scratch,TEMP:scratch,TMPDIR:scratch};
for(const key of ['READMD_REQUIRE_TOKEN','READMD_DATA_DIR','READMD_TASK_CREATION_CHILD','READMD_TASK_LEASE_CHILD'])delete env[key];
const report={passed:false,platform:process.platform,binarySha256:sha(binary),helperSha256:sha(helper),testSha256:sha(__filename),checks:[],phase:'fixtures',scope:'Actual cross-process transient admission, pinned baseline and next-start recovery; numeric reservations, not hard OS quotas or actual disk-full.'};
const children=[],delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
function check(name,detail={}){report.checks.push({name,passed:true,...detail});}
async function stop(child){if(!child||child.exitCode!==null)return;const exited=new Promise(r=>child.once('exit',r));child.kill();await Promise.race([exited,delay(2000)]);assert(child.exitCode!==null||child.signalCode!==null,'Owned child did not exit');}
function launch(program,args,name,extra={}){
 const log=fs.openSync(path.join(qa,name+'.stderr.log'),'wx');
 const child=spawn(program,args,{cwd:root,env:{...env,...extra},windowsHide:true,stdio:['pipe','pipe',log]});fs.closeSync(log);children.push(child);return child;
}
function lines(child,name){
 let text='',terminal;const found=[],waiters=[];
 const log=fs.createWriteStream(path.join(qa,name+'.stdout.log'),{flags:'wx'});
 child.stdout.setEncoding('utf8');child.stdout.on('data',chunk=>{log.write(chunk);text+=chunk;assert(text.length<4*1048576,'Protocol budget');
  for(;;){const at=text.indexOf('\n');if(at<0)break;const line=text.slice(0,at).trim();text=text.slice(at+1);found.push(line);for(const w of [...waiters]){const value=w.select(line);if(value!==undefined){clearTimeout(w.timer);waiters.splice(waiters.indexOf(w),1);w.resolve(value);}}}
 });
 child.on('error',fail);child.on('exit',()=>{fail(Error('Owned protocol child exited'));log.end();});
 function fail(error){terminal=error;for(const w of waiters.splice(0)){clearTimeout(w.timer);w.reject(error);}}
 return select=>{for(const line of found){const value=select(line);if(value!==undefined)return Promise.resolve(value);}return new Promise((resolve,reject)=>{if(terminal){reject(terminal);return;}const w={select,resolve,reject,timer:setTimeout(()=>{waiters.splice(waiters.indexOf(w),1);reject(Error('Protocol deadline'));},30000)};waiters.push(w);});};
}
async function mcp(name){
 const child=launch(binary,['--mcp','--data-dir',data,'--workspace',docs,'--assets',path.join(root,'assets')],name),next=lines(child,name);let id=0;
 async function call(method,params){const requestId=++id;child.stdin.write(JSON.stringify({jsonrpc:'2.0',id:requestId,method,params})+'\n');return next(line=>{let v;try{v=JSON.parse(line);}catch{return;}if(v.id!==requestId)return;if(v.error)throw Error(JSON.stringify(v.error));return v.result;});}
 await call('initialize',{protocolVersion:'2024-11-05',capabilities:{},clientInfo:{name:'owned-task-storage-QA',version:'1'}});
 child.stdin.write(JSON.stringify({jsonrpc:'2.0',method:'notifications/initialized',params:{}})+'\n');await call('ping',{});
 return{child,call};
}
const unpack=v=>v.structuredContent||JSON.parse(v.content[0].text);
const plain=p=>p.replace(/^\\\\\?\\/,'');
function taskDirs(){const dir=path.join(data,'worker-tasks-v1');return fs.existsSync(dir)?fs.readdirSync(dir).filter(n=>/^[0-9a-f]{32}$/.test(n)):[];}
async function until(action,message){const deadline=performance.now()+5000;while(performance.now()<deadline){if(action())return;await delay(20);}assert(action(),message);}
const source='# 研究与证据\n\n保留原始观察，比较解释，再修订结论。\n\n| 项目 | 结果 |\n|---|---|\n| 工作区 | 原子保存 |\n\n$\\frac{α+x^2}{\\sqrt{y}}$\n';
async function exportFile(client,format,file){return client.call('tools/call',{name:'readmd_export_document',arguments:{output_format:format,output_path:path.join(docs,file),base_dir:docs,markdown_content:source,title:'研究与证据',confirm:true,overwrite:true}});}
(async()=>{let owner;try{
 assert.equal(process.platform,'win32');
 for(const format of ['html','docx'])fs.writeFileSync(path.join(docs,'Protected.'+format),Buffer.from('Previous document — must remain unchanged','utf8'));
 const originals=Object.fromEntries(['html','docx'].map(f=>[f,sha(path.join(docs,'Protected.'+f))]));
 owner=launch(helper,['--exact','task_workspace::tests::admission_child','--nocapture'],'owner',{READMD_TASK_ADMISSION_CHILD:data});const ready=lines(owner,'owner');
 const tasks=await ready(line=>line.startsWith('ADMISSION_READY ')?JSON.parse(line.slice(16)):undefined);
 assert.equal(tasks.length,8);assert.equal(taskDirs().length,8);
 for(const task of tasks){assert(path.resolve(plain(task.root)).startsWith(scratch+path.sep));assert(fs.existsSync(path.join(task.root,'baseline.txt')));}
 check('eight-real-workspaces-reserve-2GiB',{tasks:tasks.length});
 const full=await mcp('full-client');await until(()=>taskDirs().length===8,'Live task records changed');
 for(const format of ['html','docx']){
  const result=await exportFile(full,format,'Protected.'+format);fs.writeFileSync(path.join(qa,format+'-refusal.json'),JSON.stringify(result,null,2));
  assert.equal(result.isError,true);assert.equal(unpack(result).error_code,'write_failed');assert.equal(sha(path.join(docs,'Protected.'+format)),originals[format]);assert.equal(taskDirs().length,8);
  check('actual-MCP-'+format+'-refuses-before-target-change');
 }
 owner.stdin.write('release-one\n');await ready(line=>line==='ADMISSION_RELEASED'?true:undefined);
 await until(()=>taskDirs().length===7,'Released reservation not reclaimed');
 const released=await exportFile(full,'docx','Replacement.docx');assert(!released.isError,JSON.stringify(released));assert.equal(unpack(released).ok,true);assert.equal(taskDirs().length,7);
 check('one-release-allows-actual-DOCX-export-with-seven-live-tasks');
 await stop(full.child);await stop(owner);owner=null;
 const recovered=await mcp('restart-client');await until(()=>taskDirs().length===1,'Next startup did not reclaim unpinned crash work');
 const pinned=tasks[0];assert.equal(taskDirs()[0],pinned.id);assert.equal(fs.readFileSync(path.join(pinned.root,'baseline.txt'),'utf8'),'Protected recovery baseline');
 for(const task of tasks.slice(1))assert(!fs.existsSync(task.root));
 check('next-actual-startup-reclaims-only-stopped-unpinned-tasks',{removed:6,preservedPinned:1});
 for(const format of ['html','docx']){
  const result=await exportFile(recovered,format,'Recovered.'+format);assert(!result.isError,JSON.stringify(result));assert.equal(unpack(result).ok,true);assert.equal(sha(path.join(docs,'Protected.'+format)),originals[format]);
  check('actual-MCP-'+format+'-succeeds-after-recovery');
 }
 await stop(recovered.child);
 const release=launch(helper,['--exact','task_workspace::tests::release_child','--nocapture'],'release',{READMD_TASK_RELEASE_CHILD:data,READMD_TASK_RELEASE_ID:pinned.id});
 const releaseLines=lines(release,'release');await releaseLines(line=>line==='REFERENCE_RELEASED'?true:undefined);await stop(release);
 const final=await mcp('final-client');await until(()=>taskDirs().length===0,'Released pinned workspace was not reclaimed');assert(!fs.existsSync(pinned.root));await stop(final.child);
 check('revision-guarded-internal-release-then-real-startup-reclaims-baseline');
 report.phase='parallel-client-contention';
 const burst=await Promise.all(Array.from({length:4},(_,index)=>mcp('burst-'+index)));
 const concurrent=await Promise.all(burst.map((client,index)=>exportFile(client,'html','Concurrent-'+index+'.html')));
 fs.writeFileSync(path.join(qa,'concurrent-exports.json'),JSON.stringify(concurrent,null,2));
 assert(concurrent.every(result=>!result.isError&&unpack(result).ok===true),'Short allocation-lock contention rejected a valid concurrent export');
 await until(()=>taskDirs().length===0,'Concurrent exports left task records');
 for(const client of burst)await stop(client.child);
 check('four-real-MCP-clients-export-concurrently-without-spurious-refusal');
 report.phase='independent-output-inspection';
 report.outputs=await require('./helpers/inspect-task-outputs.cjs')(docs);fs.writeFileSync(path.join(qa,'independent-outputs.json'),JSON.stringify(report.outputs,null,2));
 check('independent-DOCX-XML-ZIP-OMML-and-HTML-content-inspection');
 assert.equal(fs.readdirSync(scratch).flatMap(name=>fs.readdirSync(path.join(scratch,name))).length,0);
 check('owned-scratch-has-no-task-children');report.passed=true;report.phase='complete';
}catch(error){report.error=error.stack;process.exitCode=1;}finally{for(const child of children.reverse())await stop(child).catch(()=>{});fs.writeFileSync(path.join(qa,'report.json'),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify({qa:path.basename(qa),passed:report.passed,checks:report.checks.length,phase:report.phase,error:report.error}));}})();
