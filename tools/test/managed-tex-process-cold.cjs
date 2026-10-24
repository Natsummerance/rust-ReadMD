'use strict';
// Explicit process-start benchmark for the real stdio service. This does not
// flush the OS file cache and can never certify the D06 OS-cold requirement.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict'),os=require('node:os');
const {spawn}=require('node:child_process');
const hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const fileHash=file=>hash(fs.readFileSync(file));
const elapsed=start=>Number(process.hrtime.bigint()-start)/1e6;
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const BUDGET=120000;

function argumentsFor(argv){
  const result={runs:3};
  for(let i=0;i<argv.length;i++){
    if(argv[i]==='--help')return {help:true};
    const name=argv[i];assert(['--binary','--data','--assets','--output','--runs'].includes(name),'Unknown argument: '+name);
    assert(i+1<argv.length,'Missing argument: '+name);result[name.slice(2)]=argv[++i];
  }
  for(const name of ['binary','data','assets','output']){assert(result[name],'Required --'+name);result[name]=path.resolve(result[name]);}
  result.runs=Number(result.runs);assert(Number.isSafeInteger(result.runs)&&result.runs>=1&&result.runs<=3,'--runs must be 1..3');
  return result;
}
function protectedFiles(data){
  const result=[];
  function visit(folder,journalsOnly=false){
    if(!fs.existsSync(folder))return;
    for(const entry of fs.readdirSync(folder,{withFileTypes:true})){
      const file=path.join(folder,entry.name);assert(!entry.isSymbolicLink(),'Protected input contains a reparse point');
      if(entry.isDirectory())visit(file,journalsOnly);
      else if(entry.isFile()&&(!journalsOnly||entry.name.endsWith('.jsonl'))){
        result.push({path:path.relative(data,file),bytes:fs.statSync(file).size,sha256:fileHash(file)});
      }
    }
  }
  for(const folder of ['documents','outputs'])visit(path.join(data,folder));
  visit(path.join(data,'worker-tasks-v1'),true);
  for(const entry of fs.readdirSync(data,{withFileTypes:true}))if(entry.isFile()&&entry.name.toLowerCase().endsWith('.pdf')){
    const file=path.join(data,entry.name);result.push({path:entry.name,bytes:fs.statSync(file).size,sha256:fileHash(file)});
  }
  // A retained old task may point to a short workspace outside the profile.
  // Preserve its unknown files as well as its journal. This is read-only
  // protection, never a claim that the benchmark owns or may recover it.
  const normal=file=>path.toNamespacedPath(path.resolve(file)).toLowerCase();
  for(const journal of [...result].filter(item=>item.path.endsWith(path.sep+'record.jsonl'))){
    const file=path.join(data,journal.path);assert(journal.bytes<=4*1024*1024,'Old journal exceeds observer budget');
    let header;try{header=JSON.parse(fs.readFileSync(file,'utf8').split(/\r?\n/,1)[0]);}catch{throw Error('Old record.jsonl has an unreadable Header: '+journal.path);}
    assert(header?.type==='Header','Old record.jsonl is missing its first Header: '+journal.path);
    const entry=header.value;
    assert(entry?.schema===1&&/^[a-f0-9]{32}$/.test(entry.id)&&/^[a-f0-9]{64}$/.test(entry.profile)&&typeof entry.root==='string'&&entry.root.length>0,'Invalid old workspace header');
    assert.equal(path.basename(path.dirname(file)),entry.id,'Old journal directory and task disagree');
    const expected=path.join(fs.realpathSync.native(os.tmpdir()),'rmdw-'+entry.profile.slice(0,12),entry.id);
    assert.equal(normal(entry.root),normal(expected),'Old workspace is outside its declared local temporary scope');
    if(!fs.existsSync(expected))continue;
    assert.equal(normal(fs.realpathSync.native(expected)),normal(expected),'Old workspace passes through a reparse point');
    function visitExternal(folder){
      assert(!fs.lstatSync(folder).isSymbolicLink(),'Old workspace contains a reparse point');
      for(const child of fs.readdirSync(folder,{withFileTypes:true})){
        assert(!child.isSymbolicLink(),'Old workspace contains a reparse point');const target=path.join(folder,child.name);
        if(child.isDirectory())visitExternal(target);
        else if(child.isFile())result.push({path:target,externalWorkspace:true,bytes:fs.statSync(target).size,sha256:fileHash(target)});
      }
    }
    visitExternal(expected);
  }
  return result.sort((a,b)=>a.path.localeCompare(b.path));
}
function unchanged(data,before){
  for(const item of before){const file=item.externalWorkspace?item.path:path.join(data,item.path);assert(fs.existsSync(file),'Old protected input removed: '+item.path);assert.equal(fileHash(file),item.sha256,'Old protected input changed: '+item.path);}
}
function payload(result){
  const value=result.structuredContent||JSON.parse(result.content.find(item=>item.type==='text').text);
  assert(!result.isError,JSON.stringify(value));assert.equal(value.ok,true,JSON.stringify(value));return value;
}
function readBackText(result,marker){
  assert(!result.isError,JSON.stringify(result.structuredContent||result.content));
  assert.notEqual(result.structuredContent?.ok,false,JSON.stringify(result.structuredContent));
  // This tool returns a successful plain Markdown TextContent, rather than a
  // JSON envelope. A real error remains a failure even if it echoes the marker.
  const text=result.content.filter(item=>item.type==='text').map(item=>item.text).join('\n');
  assert(text.includes(marker),'Fresh PDF marker is missing from real stdio readback');
  assert(text.replace(/\s/g,'').includes('中文正文'),'Chinese prose is missing from real stdio readback');
  return text;
}
function createTransportGuard(rejectPending){
  let fatal=null,closing=false;
  const fail=(error,kind='transport_error')=>{
    if(!fatal)fatal={kind,message:String(error.message||error)};
    rejectPending(Error(fatal.message));
  };
  return {
    fail,beginClose:()=>{closing=true;},get fatal(){return fatal;},
    exit:(code,signal)=>{
      if(!closing||code!==0||signal!==null)fail(Error('MCP host exit code='+code+' signal='+signal+' closing='+closing),'host_exit');
    }
  };
}
function parseMcpLine(line,transport){
  let message;try{message=JSON.parse(line);}catch{transport.fail(Error('Invalid MCP JSON'),'invalid_json');return null;}
  if(!message||typeof message!=='object'||Array.isArray(message)||message.jsonrpc!=='2.0'){
    transport.fail(Error('Invalid MCP JSON-RPC envelope'),'invalid_envelope');return null;
  }
  return message;
}
function finishTransportObservation(report,transport,hostStopped,stdioClosed){
  if(!hostStopped)transport.fail(Error('Owned MCP host PID did not stop'),'host_not_stopped');
  if(!stdioClosed)transport.fail(Error('Owned MCP stdio did not close'),'stdio_not_closed');
  report.processStopped=hostStopped;report.processStoppedScope='host-pid-only';report.hostStdioClosed=stdioClosed;
  report.descendantProcessesObserved=false;report.cancellationCertified=false;
  report.jobObjectContainmentScope='Product source provides JobObject containment; this runner does not independently observe descendants.';
  report.transportFatal=transport.fatal;
  if(transport.fatal){report.passed=false;report.error??=transport.fatal.message;}
}
async function run(options,index){
  const out=path.join(options.output,'run-'+String(index).padStart(2,'0'));fs.mkdirSync(out);
  const marker='READMDPROCESSCOLD'+String(index).padStart(2,'0');
  const content='# 受管理 TeX 性能检查\n\n'+marker+'\n\n中文正文与公式 $x^2+1$。\n\n| 项目 | 数量 |\n|---|---:|\n| 样本 | 12 |\n';
  const input=path.join(out,'Input.md'),pdf=path.join(out,'Output.pdf');fs.writeFileSync(input,content,{flag:'wx'});
  const before=protectedFiles(options.data);fs.writeFileSync(path.join(out,'protected-before.json'),JSON.stringify(before,null,2)+'\n',{flag:'wx'});
  const report={index,processCold:true,osCacheState:'uncontrolled',coldPerformanceCertified:false,deadlineMs:BUDGET,
    readyProbeBeforeTiming:false,installationBeforeTiming:false,runtimeCopied:false,system32OnlyPath:true,
    inputSha256:hash(Buffer.from(content)),passed:false,startedUtc:new Date().toISOString(),osUptimeSeconds:os.uptime()};
  let child,next=1,buffer='',timer,stopped=false,stdioClosed=false,closeObserved;
  const pending=new Map(),started=process.hrtime.bigint();
  const stderr=fs.openSync(path.join(out,'stderr.log'),'wx'),stdout=fs.openSync(path.join(out,'stdout.jsonl'),'wx');
  function rejectAll(error){for(const call of pending.values())call.reject(error);pending.clear();}
  const transport=createTransportGuard(rejectAll);
  function request(method,params){
    if(transport.fatal)return Promise.reject(Error(transport.fatal.message));
    return new Promise((resolve,reject)=>{
      const id=next++;pending.set(id,{resolve,reject});
      try{child.stdin.write(JSON.stringify({jsonrpc:'2.0',id,method,params})+'\n',error=>{if(error)transport.fail(error,'stdin_error');});}
      catch(error){transport.fail(error,'stdin_error');}
    });
  }
  try{
    child=spawn(options.binary,['--mcp','--data-dir',options.data,'--workspace',out,'--assets',options.assets],{
      windowsHide:true,stdio:['pipe','pipe',stderr],env:{...process.env,PATH:path.join(process.env.WINDIR,'System32')}
    });
    report.pid=child.pid;
    closeObserved=new Promise(resolve=>child.once('close',()=>{
      stdioClosed=true;
      if(buffer.trim())transport.fail(Error('MCP stdout ended with an unterminated JSON line'),'incomplete_json');
      resolve();
    }));
    timer=setTimeout(()=>{transport.fail(Error('Entire process-start workflow exceeded 120000ms'),'deadline');child.kill();},BUDGET);
    child.on('error',error=>transport.fail(error,'process_error'));
    child.on('exit',(code,signal)=>{stopped=true;report.hostExit={code,signal};transport.exit(code,signal);});
    child.stdin.on('error',error=>transport.fail(error,'stdin_error'));child.stdout.on('error',error=>transport.fail(error,'stdout_error'));child.stdout.setEncoding('utf8');
    child.stdout.on('data',chunk=>{
      try{fs.writeSync(stdout,chunk);}catch(error){transport.fail(error,'stdout_log_error');child.kill();return;}
      buffer+=chunk;
      if(Buffer.byteLength(buffer)>8*1024*1024){transport.fail(Error('MCP stdout budget'),'stdout_budget');child.kill();return;}
      for(;;){
        const at=buffer.indexOf('\n');if(at<0)break;
        const line=buffer.slice(0,at);buffer=buffer.slice(at+1);if(!line.trim())continue;
        const message=parseMcpLine(line,transport);if(!message){child.kill();return;}
        const call=pending.get(message.id);if(!call)continue;pending.delete(message.id);
        message.error?call.reject(Error(JSON.stringify(message.error))):call.resolve(message.result);
      }
    });
    await request('initialize',{protocolVersion:'2024-11-05',capabilities:{},clientInfo:{name:'readmd-process-cold-performance-qa',version:'1'}});
    child.stdin.write(JSON.stringify({jsonrpc:'2.0',method:'notifications/initialized'})+'\n');
    const tools=await request('tools/list',{});assert(tools.tools.some(tool=>tool.name==='readmd_export_document'));
    report.beforeFirstExportMs=elapsed(started);
    const exported=payload(await request('tools/call',{name:'readmd_export_document',arguments:{
      markdown_content:content,output_path:pdf,output_format:'pdf',base_dir:out,confirm:true,
      options:{pdf:{engine:'xelatex'},tex:{docClass:'article',useCtex:true},cover:{enabled:false},toc:{enabled:false},frontMatter:{mode:'none'}}
    }}));
    const bytes=fs.readFileSync(pdf);assert(bytes.subarray(0,5).equals(Buffer.from('%PDF-')));assert(bytes.subarray(-1024).includes(Buffer.from('%%EOF')));
    report.pdfReadyMs=elapsed(started);report.pdf={bytes:bytes.length,sha256:hash(bytes)};report.exported=exported;
    const text=readBackText(await request('tools/call',{name:'readmd_convert_to_markdown',arguments:{file_path:pdf}}),marker);
    fs.writeFileSync(path.join(out,'Readback.md'),text,{flag:'wx'});report.readback={sha256:hash(Buffer.from(text)),markerPreserved:true,chineseProsePreserved:true};
    report.elapsedMs=elapsed(started);report.withinBudget=report.elapsedMs<BUDGET;assert(report.withinBudget,'Entire process-start workflow exceeded 120000ms');
    clearTimeout(timer);report.passed=true;
  }catch(error){report.error=String(error.message);report.elapsedMs=elapsed(started);report.withinBudget=report.elapsedMs<BUDGET;}
  finally{
    clearTimeout(timer);transport.beginClose();rejectAll(Error('Owned process closing'));
    if(child){
      try{child.stdin.end();}catch(error){transport.fail(error,'stdin_error');}
      if(!stdioClosed){await Promise.race([closeObserved,delay(2000)]);if(!stdioClosed){transport.fail(Error('MCP stdio did not close after stdin.end'),'close_timeout');child.kill();await Promise.race([closeObserved,delay(3000)]);}}
    }
    // Child 'exit' can precede trailing stdout data. Finalize only after
    // 'close', and keep every fatal error latched even with no pending request.
    finishTransportObservation(report,transport,stopped,stdioClosed);report.stopElapsedMs=elapsed(started);
    if(!child||stdioClosed){fs.closeSync(stderr);fs.closeSync(stdout);report.logDescriptorsClosed=true;}
    else {
      // Never close a log FD while a late stdout data callback can still run.
      // This observation already failed; a later close only releases its FDs.
      report.logDescriptorsClosed=false;child.once('close',()=>{fs.closeSync(stderr);fs.closeSync(stdout);});
    }
    report.oldInputsAndJournalsUnchangedScope='Enumerated pre-existing files: continued existence and content hashes only.';
    report.directoryIdentityObserved=false;report.newFilesCompared=false;
    try{unchanged(options.data,before);report.oldInputsAndJournalsUnchanged=true;}catch(error){report.passed=false;report.protectionError=String(error.message);}
    fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});
  }
  console.log(JSON.stringify({index,passed:report.passed,pdfReadyMs:report.pdfReadyMs,elapsedMs:report.elapsedMs,error:report.error}));return report;
}
async function main(){
  const options=argumentsFor(process.argv.slice(2));if(options.help){console.log('node tools/test/managed-tex-process-cold.cjs --binary EXE --data EXISTING_PROFILE --assets QA_ASSETS --output NEW_DIRECTORY [--runs 1..3]');return;}
  assert.equal(process.platform,'win32','This benchmark describes the Windows managed service');
  const lock=path.join(options.assets,'capabilities/latex-2026.10-r1-windows-x64.lock.json');
  const manifest=JSON.parse(fs.readFileSync(lock));assert.equal(manifest.id,'latex');assert.equal(manifest.review.redistributionApproved,false,'This runner currently targets the existing unpublished QA runtime');
  assert(fs.existsSync(path.join(options.data,'capabilities/managed-v2/latex/2026.10-r1')),'Managed runtime missing');
  fs.mkdirSync(options.output);
  const summary={schemaVersion:1,scope:'New process to first managed XeLaTeX PDF and real stdio readback, including startup and unchanged 120s deadline; host shutdown measured separately',
    options,qaOnlyRuntime:true,redistributionApproved:false,osCacheState:'uncontrolled',coldPerformanceCertified:false,D06Accepted:false,
    nodeVersion:process.version,nodeExecutable:process.execPath,platform:process.platform,architecture:process.arch,startedNewHostForEveryRun:true,bootLoadedByStdio:false,
    binarySha256:fileHash(options.binary),bootSha256:fileHash(path.join(options.assets,'readmd.boot.js')),manifestSha256:fileHash(lock),runnerSha256:fileHash(__filename),runs:[]};
  for(let index=1;index<=options.runs;index++){
    assert.equal(fileHash(options.binary),summary.binarySha256,'Candidate binary changed between observations');
    assert.equal(fileHash(lock),summary.manifestSha256,'QA resource lock changed between observations');
    assert.equal(fileHash(path.join(options.assets,'readmd.boot.js')),summary.bootSha256,'Recorded QA asset identity changed between observations');
    const report=await run(options,index);summary.runs.push(report);fs.writeFileSync(path.join(options.output,'progress.json'),JSON.stringify(summary,null,2)+'\n');
    if(!report.passed)break;
  }
  assert.equal(fileHash(options.binary),summary.binarySha256,'Candidate binary changed during observations');
  assert.equal(fileHash(lock),summary.manifestSha256,'QA resource lock changed during observations');
  summary.processColdBudgetSatisfied=summary.runs.length===options.runs&&summary.runs.every(report=>report.passed&&report.withinBudget);
  fs.writeFileSync(path.join(options.output,'summary.json'),JSON.stringify(summary,null,2)+'\n',{flag:'wx'});
  if(!summary.processColdBudgetSatisfied)process.exitCode=1;
}
if(require.main===module)main().catch(error=>{console.error(error.stack);process.exitCode=1;});
module.exports={argumentsFor,payload,readBackText,protectedFiles,unchanged,createTransportGuard,parseMcpLine,finishTransportObservation};
