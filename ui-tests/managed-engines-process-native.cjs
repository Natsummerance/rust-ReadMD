'use strict';
// Genuine installed engines, actual packaged stdio/API transports and physical
// owned-process fault injection. Never mock an engine, readiness or busy reply.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const {spawn}=require('node:child_process');
const root=path.resolve(__dirname,'..'),binary=path.resolve(process.env.READMD_BIN||'.cache/community-upgrade/build/debug/readmd.exe');
const assets=path.join(root,'assets'),qa=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/engine-mcp-native-'));
const docs=path.join(qa,'documents'),scratch=path.join(qa,'scratch'),segment='资料'.repeat(35),data=path.join(qa,segment,segment,segment,segment,'profile');fs.mkdirSync(docs);fs.mkdirSync(scratch);
const sha=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const env={...process.env,PATH:path.join(process.env.SystemRoot,'System32'),TMP:scratch,TEMP:scratch,TMPDIR:scratch};
delete env.READMD_REQUIRE_TOKEN;delete env.READMD_DATA_DIR;
const report={passed:false,platform:'windows-x64',actualTransports:['stdio-MCP','loopback-API'],physicalFaultInjection:true,checks:[],phase:'fixture',profileUtf16:data.length,binarySha256:sha(binary),testSha256:sha(__filename),observerSha256:sha(path.join(__dirname,'helpers/windows-worker-observer.ps1')),observerNativeSha256:sha(path.join(__dirname,'helpers/windows-worker-observer.cs'))};
let setup,observer,observerRescue='';const children=[];
async function stop(child){if(!child||child.exitCode!==null||child.signalCode!==null)return;const end=new Promise(resolve=>child.once('exit',resolve));child.kill();await Promise.race([end,delay(2000)]);}
function protocol(child,maximum=4*1024*1024){
  let serial=0,buffer='';const pending=new Map();let terminal;
  function fail(error){terminal=error;for(const p of pending.values()){clearTimeout(p.timer);p.reject(error);}pending.clear();}
  child.stdout.setEncoding('utf8');child.stdout.on('data',chunk=>{buffer+=chunk;if(buffer.length>maximum){fail(Error('Protocol output budget'));return;}for(;;){const at=buffer.indexOf('\n');if(at<0)break;const line=buffer.slice(0,at);buffer=buffer.slice(at+1);if(!line.trim())continue;let message;try{message=JSON.parse(line);}catch{fail(Error('Non-JSON protocol output'));return;}const p=pending.get(message.id);if(!p)continue;clearTimeout(p.timer);pending.delete(message.id);message.error?p.reject(Error(typeof message.error==='string'?message.error:JSON.stringify(message.error))):p.resolve(message.result);}});
  child.on('error',fail);child.on('exit',()=>fail(Error('Owned protocol process exited')));
  return {
    request(method,params,timeout=100000){const id=++serial;const promise=new Promise((resolve,reject)=>{if(terminal){reject(terminal);return;}const timer=setTimeout(()=>{pending.delete(id);reject(Error('Protocol request deadline '+method));},timeout);pending.set(id,{resolve,reject,timer});child.stdin.write(JSON.stringify({jsonrpc:'2.0',id,method,params})+'\n');});promise.catch(()=>{});return{id,promise};},
    notify(method,params){child.stdin.write(JSON.stringify({jsonrpc:'2.0',method,params})+'\n');},
    abandon(id){const p=pending.get(id);if(p){clearTimeout(p.timer);pending.delete(id);p.reject(Error('Client stopped waiting after cancellation notification'));}},
    observer(action,params,timeout=80000){const id=++serial;const promise=new Promise((resolve,reject)=>{if(terminal){reject(terminal);return;}const timer=setTimeout(()=>{pending.delete(id);reject(Error('Native observer deadline'));},timeout);pending.set(id,{resolve,reject,timer});child.stdin.write(JSON.stringify({id,action,params})+'\n');});promise.catch(()=>{});return promise;}
  };
}
async function host(){
  const log=fs.openSync(path.join(qa,'setup.log'),'wx');
  const child=spawn(binary,['--no-window','--port','-1','--data-dir',data,'--workspace',docs,'--assets',assets],{env,windowsHide:true,stdio:['ignore','pipe',log]});fs.closeSync(log);children.push(child);
  let buffer='';const url=await new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(Error('Owned setup startup deadline')),30000);child.on('error',error=>{clearTimeout(timer);reject(error);});child.on('exit',code=>{clearTimeout(timer);reject(Error('Owned setup exited '+code));});child.stdout.on('data',chunk=>{buffer+=chunk;const match=buffer.match(/ listening on (http:\/\/127\.0\.0\.1:\d+)/);if(match){clearTimeout(timer);resolve(match[1]);}if(buffer.length>32768)buffer=buffer.slice(-16384);});});
  return {child,async pack(action,id,location){const r=await fetch(url+'/api/capabilities/packs',{method:'POST',headers:{Origin:url,'Content-Type':'application/json'},body:JSON.stringify({action,id,...(location?{path:location}:{})}),signal:AbortSignal.timeout(90000)});return{status:r.status,data:await r.json()};}};
}
async function client(name){
  const log=fs.openSync(path.join(qa,name+'.log'),'wx');const child=spawn(binary,['--mcp','--data-dir',data,'--workspace',docs,'--assets',assets],{env,windowsHide:true,stdio:['pipe','pipe',log]});fs.closeSync(log);children.push(child);
  const channel=protocol(child);await channel.request('initialize',{protocolVersion:'2024-11-05',capabilities:{},clientInfo:{name:'readmd-genuine-engine-fault-qa',version:'1'}}).promise;
  channel.notify('notifications/initialized',{});const listed=await channel.request('tools/list',{}).promise;
  assert(listed.tools.some(t=>t.name==='readmd_ocr_to_markdown')&&listed.tools.some(t=>t.name==='readmd_export_document'));
  return{child,...channel};
}
const value=result=>result.structuredContent||JSON.parse(result.content[0].text);
function workspaces(){
  const directory=path.join(data,'worker-tasks-v1');if(!fs.existsSync(directory))return[];
  return fs.readdirSync(directory,{withFileTypes:true}).filter(d=>d.isDirectory()&&!d.isSymbolicLink()&&/^[a-f0-9]{32}$/.test(d.name)).map(d=>{
    const record=path.join(directory,d.name,'record.jsonl'),lines=fs.readFileSync(record,'utf8').split('\n');lines.pop();
    const events=lines.filter(Boolean).map(s=>JSON.parse(s)),header=events.shift();assert.equal(header.type,'Header');assert.equal(header.value.id,d.name);
    const task=header.value,plain=p=>p.replace(/^\\\\\?\\/,'');
    assert(path.resolve(plain(task.root)).startsWith(path.resolve(scratch)+path.sep),'Task root outside owned scratch');
    const files=events.filter(e=>e.type==='Created'&&e.kind==='File').map(e=>{assert(!e.path.includes('..')&&!path.isAbsolute(e.path));return path.join(task.root,e.path);});
    return{...task,record,files:[...new Set(files)].filter(f=>fs.existsSync(f))};
  });
}
async function until(action,message){const deadline=performance.now()+5000;while(performance.now()<deadline){if(action())return;await delay(20);}assert(action(),message);}
function exportArgs(name,content){return{markdown_content:content,output_path:path.join(docs,name+'.pdf'),output_format:'pdf',base_dir:docs,title:name,options:{pdf:{engine:'typst'}},confirm:true,overwrite:true};}
function task(client,kind,name,large){
  return client.request('tools/call',kind==='typst'?{name:'readmd_export_document',arguments:exportArgs(name,large)}:{name:'readmd_ocr_to_markdown',arguments:{file_path:path.join(docs,'Long-scan.png')}});
}
async function arm(client,kind){const program=path.join(data,'capabilities/managed-v2',kind,kind==='typst'?'0.15.1':'5.5.3-r1','bin',kind==='typst'?'typst.exe':'tesseract.exe');return observer.channel.observer('arm',{ownerPid:client.child.pid,ownerProgram:binary,program,sha256:sha(program),kind});}
async function busy(client,kind,name,large){const start=performance.now(),result=await task(client,kind,name,large).promise,ms=Math.round(performance.now()-start);fs.writeFileSync(path.join(qa,name+'-busy.json'),JSON.stringify({elapsedMs:ms,result},null,2));assert.equal(result.isError,true);assert.equal(value(result).error_code,kind+'_busy');assert(ms<200,'Busy request exceeded200ms');return ms;}
async function cancel(client,request,engine,name,expectedFree){
  const start=performance.now();client.notify('notifications/cancelled',{requestId:request.id,reason:'Owned fault-test cancellation'});
  await client.request('ping',{}).promise;const feedbackMs=Math.round(performance.now()-start);
  // This server suppresses replies to requests cancelled via notification.
  // Verify actual engine termination with its native process handle, instead
  // of waiting for a reply that the protocol intentionally does not send.
  const stopped=await observer.channel.observer('stopped',{key:engine.key,timeoutMs:2000});
  let slots;
  do{slots=await observer.channel.observer('freeSlots',{key:engine.key});if(slots.free>=expectedFree)break;await delay(5);}while(performance.now()-start<2000);
  const completeMs=Math.round(performance.now()-start);client.abandon(request.id);
  fs.writeFileSync(path.join(qa,name+'-cancel.json'),JSON.stringify({feedbackMs,completeMs,stopped,slots,expectedFree,notification:true,completionEvidence:'native process handle signalled and OS worker slot released'},null,2));
  assert(feedbackMs<=200);assert(completeMs<=2000);assert.equal(stopped.stopped,true);assert(slots.free>=expectedFree);
  return{feedbackMs,completeMs};
}
(async()=>{try{
  assert.equal(process.platform,'win32');assert(data.length>300);
  const observerChild=spawn(path.join(process.env.SystemRoot,'System32/WindowsPowerShell/v1.0/powershell.exe'),['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',path.join(__dirname,'helpers/windows-worker-observer.ps1'),'-QaRoot',qa,'-NodeProcessId',String(process.pid)],{windowsHide:true,stdio:['pipe','pipe','pipe']});children.push(observerChild);observerChild.stderr.on('data',b=>{observerRescue+=b.toString();});observer={child:observerChild,channel:protocol(observerChild)};
  const ready=await observer.channel.observer('ready',{echo:'资料'});assert.equal(ready.nodeAncestor,true);assert.equal(ready.echo,'资料');
  fs.copyFileSync(path.join(assets,'engines/ocr-probe.png'),path.join(docs,'Scan.png'));
  await require('./helpers/raster-fixtures.cjs')(path.join(docs,'Long-scan.png'),{width:3000,height:3500,source:fs.readFileSync(path.join(docs,'Scan.png')).toString('base64')});
  const originals={small:sha(path.join(docs,'Scan.png')),large:sha(path.join(docs,'Long-scan.png'))};
  report.phase='actual-offline-install';setup=await host();const packs=[];
  for(const[id,file]of [['fonts','readmd-fonts-2026.10.10-universal.readmd-pack'],['typst','readmd-typst-0.15.1-windows-x64.readmd-pack'],['ocr','readmd-ocr-5.5.3-r1-windows-x64.readmd-pack']]){const location=path.join(root,'.cache/capability-upgrade/locked-packs',file),r=await setup.pack('install',id,location);assert.equal(r.status,200);assert.equal(r.data.installed,true);packs.push({id,sha256:sha(location)});}
  report.packs=packs;report.checks.push({name:'genuine-fixed-pack-install-deep-owned-profile',passed:true});
  const seedClient=await client('seed');const seed=await seedClient.request('tools/call',{name:'readmd_export_document',arguments:exportArgs('Protected','# 原始研究记录\n\n保留证据，比较解释。\n\n```readmd-math v1\nfrac(α+x^2,sqrt(y))\n```\n')}).promise;assert(!seed.isError,JSON.stringify(seed));assert.equal(value(seed).ok,true);await stop(seedClient.child);const seedBytes=fs.readFileSync(path.join(docs,'Protected.pdf'));
  report.checks.push({name:'actual-MCP-seed-PDF-before-faults',passed:true});
  assert.equal(workspaces().length,0,'Successful probes and export left task workspaces');report.checks.push({name:'normal-install-probes-and-MCP-export-leave-no-task-directory',passed:true});
  const large='# 并发研究资料\n\n'+Array.from({length:1200},(_,i)=>'## 研究记录 '+i+'\n\n保留原始观察与研究数据，核对数字、引用和结论。\n\n$\\frac{α+x^2}{\\sqrt{y}}$\n\n| 观察 | 核对 |\n|---|---|\n| 保留原文 | 检查前提 |\n\n').join('');
  for(const kind of ['typst','ocr']){
    report.phase=kind+'-genuine-fault-matrix';const first=await client(kind+'-first'),second=await client(kind+'-second'),replacement=await client(kind+'-replacement');
    for(const name of ['First','Second','Replacement'])fs.writeFileSync(path.join(docs,kind+name+'.pdf'),seedBytes);
    const a=await arm(first,kind),firstRequest=task(first,kind,kind+'First',large),firstEngine=await observer.channel.observer('observe',{key:a.key});
    const purpose=kind==='typst'?'typst-export':'ocr-recognize',firstWorks=workspaces().filter(w=>w.purpose===purpose);assert.equal(firstWorks.length,1);const firstWork=firstWorks[0];assert(firstWork.files.length>0);
    const sentinel=path.join(firstWork.root,'qa-unknown-user-file.txt');fs.writeFileSync(sentinel,'Unknown user file must survive recovery',{flag:'wx'});
    const b=await arm(second,kind),secondRequest=task(second,kind,kind+'Second',large),secondEngine=await observer.channel.observer('observe',{key:b.key});
    const secondWorks=workspaces().filter(w=>w.purpose===purpose&&w.id!==firstWork.id);assert.equal(secondWorks.length,1);const secondWork=secondWorks[0];assert(secondWork.files.length>0);
    const liveRecovery=await client(kind+'-live-recovery');await stop(liveRecovery.child);
    assert(firstWork.files.every(f=>fs.existsSync(f))&&secondWork.files.every(f=>fs.existsSync(f)),'Live task files were reclaimed');
    report.checks.push({name:kind+'-restart-recovery-preserves-two-real-live-worker-leases',passed:true});
    fs.writeFileSync(path.join(qa,kind+'-observed.json'),JSON.stringify({firstEngine,secondEngine},null,2));
    const busyMs=await busy(replacement,kind,kind+'Third',large);report.checks.push({name:kind+'-two-real-MCP-owners-third-refused',passed:true,busyMs});
    const blocked=await setup.pack('uninstall',kind);fs.writeFileSync(path.join(qa,kind+'-uninstall-busy.json'),JSON.stringify(blocked,null,2));
    // The public pack API keeps engine-specific busy codes for Typst/TeX;
    // OCR uses the general resource-install code. Check its exact contract.
    assert.equal(blocked.status,409);assert.equal(blocked.data.error_code,kind==='typst'?'typst_busy':'capability_install_busy');report.checks.push({name:kind+'-active-real-engine-protects-resource-uninstall',passed:true});
    const crashStart=performance.now();await stop(first.child);const ended=await observer.channel.observer('stopped',{key:firstEngine.key,timeoutMs:2000}),crashMs=Math.round(performance.now()-crashStart);assert(ended.stopped&&crashMs<=2000);assert.equal((await observer.channel.observer('alive',{key:secondEngine.key})).alive,true);
    report.checks.push({name:kind+'-owner-abrupt-exit-stops-only-owned-real-engine',passed:true,crashMs});
    const crashRecovery=await client(kind+'-crash-recovery');
    await until(()=>firstWork.files.every(f=>!fs.existsSync(f)),'Known crashed-task files not reclaimed');
    assert.equal(fs.readFileSync(sentinel,'utf8'),'Unknown user file must survive recovery');assert(secondWork.files.every(f=>fs.existsSync(f)));
    fs.writeFileSync(path.join(qa,kind+'-workspace-recovery.json'),JSON.stringify({firstWork,secondWork,knownReclaimed:true,unknownPreserved:true,independentLiveFilesPreserved:true},null,2));
    report.checks.push({name:kind+'-actual-crashed-task-reclaims-known-files-preserves-unknown-and-live-sibling',passed:true,knownFiles:firstWork.files.length});
    await stop(crashRecovery.child);fs.unlinkSync(sentinel);const finishRecovery=await client(kind+'-finish-recovery');
    await until(()=>!fs.existsSync(firstWork.root),'Owned empty workspace not reclaimed on next restart');await stop(finishRecovery.child);
    report.checks.push({name:kind+'-recovery-resumes-idempotently-after-explicit-sentinel-removal',passed:true});
    const c=await arm(replacement,kind),replacementRequest=task(replacement,kind,kind+'Replacement',large),replacementEngine=await observer.channel.observer('observe',{key:c.key});
    const againMs=await busy(second,kind,kind+'Fourth',large);report.checks.push({name:kind+'-one-slot-recovered-independent-task-still-protected',passed:true,busyMs:againMs});
    const cancelSecond=await cancel(second,secondRequest,secondEngine,kind+'Second',1),cancelReplacement=await cancel(replacement,replacementRequest,replacementEngine,kind+'Replacement',2);
    report.checks.push({name:kind+'-actual-MCP-notification-cancels-engine',passed:true,results:[cancelSecond,cancelReplacement]});
    for(const name of ['First','Second','Replacement'])assert.equal(sha(path.join(docs,kind+name+'.pdf')),crypto.createHash('sha256').update(seedBytes).digest('hex'));
    if(kind==='typst'){const recovered=await replacement.request('tools/call',{name:'readmd_export_document',arguments:exportArgs('Recovered','# 恢复后的研究记录\n\n保留原始证据，任务恢复后仍可继续导出。\n\n```readmd-math v1\nfrac(α+x^2,sqrt(y))\n```\n')}).promise;assert(!recovered.isError,JSON.stringify(recovered));assert.equal(value(recovered).ok,true);}
    else{const recovered=await replacement.request('tools/call',{name:'readmd_ocr_to_markdown',arguments:{file_path:path.join(docs,'Scan.png')}}).promise;assert(!recovered.isError,JSON.stringify(recovered));assert.match(recovered.content[0].text,/READMD/);assert.match(recovered.content[0].text,/12345/);fs.writeFileSync(path.join(qa,'recovered-ocr.json'),JSON.stringify(recovered,null,2));}
    report.checks.push({name:kind+'-genuine-success-after-crash-and-cancellations-originals-protected',passed:true});
    await stop(second.child);await stop(replacement.child);
  }
  assert.equal(sha(path.join(docs,'Scan.png')),originals.small);assert.equal(sha(path.join(docs,'Long-scan.png')),originals.large);
  report.originals=originals;report.phase='normal-uninstall';
  for(const id of ['ocr','typst','fonts']){const r=await setup.pack('uninstall',id);assert.equal(r.status,200);assert.equal(r.data.ok,true);}
  report.checks.push({name:'normal-resource-uninstall-after-all-real-owners-finish',passed:true});
  await stop(setup.child);observer.child.stdin.end();await new Promise(resolve=>observer.child.once('exit',resolve));assert.equal(observerRescue,'','Observer needed rescue; job cleanup is not certified');
  // Forced client termination can interrupt leaf-marker deletion after the
  // actual task files have gone. Certify the promised next-start recovery.
  const finalRecovery=await client('final-recovery');await until(()=>workspaces().length===0,'Next-start recovery left owned workspace records');await stop(finalRecovery.child);
  report.checks.push({name:'next-start-recovery-finishes-engine-and-MCP-task-files-and-records',passed:true});
  report.scope='Genuine installed Typst compile and OCR recognition through packaged MCP stdio, shared loopback installation/uninstall, physical thread fault barriers,2 OS slots, abrupt owner termination, independent owner survival, replacement, cancellation, actual successful output, persistent task identity/OS leases and restart recovery of known files/junctions while preserving unknown files. Local Windows deep profile only; not creation-boundary crashes, disk-full, natural eight-hour stress, all transports/platforms/historical/Word/release certification.';
  report.passed=true;
}catch(error){report.error=String(error.message).replaceAll(qa,'<owned-profile>').replaceAll(root,'<workspace>');process.exitCode=1;
}finally{
  for(const child of children.filter(c=>(!observer||c!==observer.child)&&(!setup||c!==setup.child)).reverse())await stop(child);
  if(observer&&observer.child.exitCode===null){observer.child.stdin.end();await Promise.race([new Promise(resolve=>observer.child.once('exit',resolve)),delay(5000)]);await stop(observer.child);}
  report.observerRescue=observerRescue;
  if(setup&&setup.child.exitCode===null&&setup.child.signalCode===null){report.failureCleanup=[];for(const id of ['ocr','typst','fonts'])try{const r=await setup.pack('uninstall',id);report.failureCleanup.push({id,status:r.status,ok:r.data.ok===true});}catch{report.failureCleanup.push({id,failed:true});}}
  if(setup)await stop(setup.child);
  report.recordedAt=new Date().toISOString();fs.writeFileSync(path.join(qa,'report.json'),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,phase:report.phase,report:path.relative(root,path.join(qa,'report.json'))}));
}})();
