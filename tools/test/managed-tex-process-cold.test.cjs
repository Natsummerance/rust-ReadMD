'use strict';
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),os=require('node:os'),crypto=require('node:crypto');
const {argumentsFor,payload,readBackText,protectedFiles,unchanged,createTransportGuard,parseMcpLine,finishTransportObservation}=require('./managed-tex-process-cold.cjs');
const args=['--binary','readmd.exe','--data','.','--assets','assets','--output','new-output'];
test('explicit paths and one to three observations are required',()=>{
  assert.equal(argumentsFor(args).runs,3);assert.equal(argumentsFor([...args,'--runs','1']).runs,1);
  assert(path.isAbsolute(argumentsFor(args).binary));assert.equal(argumentsFor(['--help']).help,true);
  for(const extra of [['--runs','0'],['--runs','4'],['--runs','NaN'],['--runs','1.5'],['--other','x'],['--runs']])assert.throws(()=>argumentsFor([...args,...extra]));
  assert.throws(()=>argumentsFor(['--binary','readmd.exe']));
});
test('JSON export failures never become successes',()=>{
  assert.deepEqual(payload({structuredContent:{ok:true}}),{ok:true});
  assert.deepEqual(payload({content:[{type:'text',text:'{"ok":true}'}]}),{ok:true});
  for(const value of [{isError:true,structuredContent:{ok:true}},{isError:false,structuredContent:{ok:false,error_code:'latex_timeout'}},{structuredContent:{message:'not a success envelope'}},{content:[{type:'text',text:'not JSON'}]}])assert.throws(()=>payload(value));
});
test('plain Markdown readback preserves the unique marker and Chinese text',()=>{
  const text='# Document\n\nREADMDPROCESSCOLD01\n\n中 文 正 文\n';
  assert.equal(readBackText({content:[{type:'text',text}]},'READMDPROCESSCOLD01'),text);
  assert.throws(()=>readBackText({content:[{type:'text',text:'中文正文'}]},'READMDPROCESSCOLD01'));
  assert.throws(()=>readBackText({content:[{type:'text',text:'READMDPROCESSCOLD01'}]},'READMDPROCESSCOLD01'));
});
test('readback rejects a tool error that echoes all expected content',()=>{
  const content=[{type:'text',text:'READMDPROCESSCOLD01 中文正文'}];
  assert.throws(()=>readBackText({isError:true,content},'READMDPROCESSCOLD01'));
  assert.throws(()=>readBackText({isError:false,structuredContent:{ok:false},content},'READMDPROCESSCOLD01'));
});
test('old document, PDF and journal changes or removal are detected without traversing the runtime',()=>{
  const base=path.resolve('.cache/capability-upgrade/longtail-20261019/tex-cold');fs.mkdirSync(base,{recursive:true});
  const data=fs.mkdtempSync(path.join(base,'runner-protection-unit-'));
  const id=crypto.randomBytes(16).toString('hex'),profile=crypto.randomBytes(32).toString('hex');
  for(const name of ['documents','outputs','worker-tasks-v1/'+id,'capabilities/managed-v2/latex/runtime'])fs.mkdirSync(path.join(data,name),{recursive:true});
  for(const name of ['documents/Input.md','outputs/Old.pdf','Root.pdf','capabilities/managed-v2/latex/runtime/engine'])fs.writeFileSync(path.join(data,name),'unchanged');
  fs.writeFileSync(path.join(data,'worker-tasks-v1',id,'record.jsonl'),JSON.stringify({type:'Header',value:{schema:1,id,profile,root:path.join(fs.realpathSync.native(os.tmpdir()),'rmdw-'+profile.slice(0,12),id)}})+'\n');
  const before=protectedFiles(data);assert.equal(before.length,4);assert(!before.some(item=>item.path.startsWith('capabilities')));unchanged(data,before);
  for(const item of before){const file=path.join(data,item.path),original=fs.readFileSync(file);fs.writeFileSync(file,'corrupted');assert.throws(()=>unchanged(data,before));fs.writeFileSync(file,original);}
  const removed=path.join(data,before[0].path);fs.renameSync(removed,removed+'.retained');assert.throws(()=>unchanged(data,before));
});
test('retained short-workspace files are protected without adopting them or following arbitrary roots',()=>{
  const base=path.resolve('.cache/capability-upgrade/longtail-20261019/tex-cold');fs.mkdirSync(base,{recursive:true});
  const data=fs.mkdtempSync(path.join(base,'runner-workspace-unit-')),id=crypto.randomBytes(16).toString('hex'),profile=crypto.randomBytes(32).toString('hex');
  const root=path.join(fs.realpathSync.native(os.tmpdir()),'rmdw-'+profile.slice(0,12),id),journal=path.join(data,'worker-tasks-v1',id,'record.jsonl');
  fs.mkdirSync(path.dirname(journal),{recursive:true});fs.mkdirSync(root,{recursive:true});
  const old=path.join(root,'Unknown-kept.pdf');fs.writeFileSync(old,'keep this unknown PDF',{flag:'wx'});
  const header={type:'Header',value:{schema:1,id,profile,root}};fs.writeFileSync(journal,JSON.stringify(header)+'\n',{flag:'wx'});
  const before=protectedFiles(data);assert.equal(before.length,2);assert(before.some(item=>item.externalWorkspace&&item.path===old));unchanged(data,before);
  fs.writeFileSync(old,'different');assert.throws(()=>unchanged(data,before));fs.writeFileSync(old,'keep this unknown PDF');unchanged(data,before);
  header.value.root=data;fs.writeFileSync(journal,JSON.stringify(header)+'\n');assert.throws(()=>protectedFiles(data),/outside its declared local temporary scope/);
});
test('missing or malformed old record Headers refuse observation while ordinary JSONL stays byte-protected',()=>{
  const base=path.resolve('.cache/capability-upgrade/longtail-20261019/tex-cold');fs.mkdirSync(base,{recursive:true});
  const data=fs.mkdtempSync(path.join(base,'runner-header-unit-')),folder=path.join(data,'worker-tasks-v1',crypto.randomBytes(16).toString('hex'));fs.mkdirSync(folder,{recursive:true});
  fs.writeFileSync(path.join(folder,'ordinary.jsonl'),'unstructured old log\n');assert.equal(protectedFiles(data).length,1);
  const record=path.join(folder,'record.jsonl');
  for(const content of ['', 'invalid JSON\n', 'null\n', '{"type":"Other"}\n', '{"type":"Header","value":{}}\n']){
    fs.writeFileSync(record,content);assert.throws(()=>protectedFiles(data),/Header|header/);assert.equal(fs.readFileSync(record,'utf8'),content,'Refusal must leave the old record unchanged');
  }
});
test('fatal transport events after a resolved final reply still force the report to fail',async()=>{
  for(const failure of [
    guard=>parseMcpLine('late non-JSON output',guard),
    guard=>parseMcpLine('null',guard),
    guard=>guard.fail(Error('MCP stdout budget'),'stdout_budget'),
    guard=>guard.fail(Error('late stdin error'),'stdin_error'),
    guard=>guard.fail(Error('late stdout error'),'stdout_error'),
    guard=>guard.exit(7,null),
    guard=>guard.exit(null,'SIGTERM')
  ]){
    const pending=new Map();const guard=createTransportGuard(error=>{for(const call of pending.values())call.reject(error);pending.clear();});
    const reply=await Promise.resolve({jsonrpc:'2.0',id:4,result:{content:[{type:'text',text:'READMDPROCESSCOLD01 中文正文'}]}});
    readBackText(reply.result,'READMDPROCESSCOLD01');const report={passed:true};
    assert.equal(pending.size,0);failure(guard);guard.beginClose();guard.exit(0,null);
    finishTransportObservation(report,guard,true,true);assert.equal(report.passed,false);assert(report.transportFatal);assert.equal(report.processStoppedScope,'host-pid-only');
  }
});
test('normal requested stdin close is accepted and incomplete stdio closure cannot pass',()=>{
  const guard=createTransportGuard(()=>{}),report={passed:true};guard.beginClose();guard.exit(0,null);
  finishTransportObservation(report,guard,true,true);assert.equal(report.passed,true);assert.equal(report.transportFatal,null);assert.equal(report.descendantProcessesObserved,false);
  const notClosed=createTransportGuard(()=>{}),failed={passed:true};notClosed.beginClose();notClosed.exit(0,null);
  finishTransportObservation(failed,notClosed,true,false);assert.equal(failed.passed,false);assert.equal(failed.transportFatal.kind,'stdio_not_closed');
  const unexpected=createTransportGuard(()=>{}),early={passed:true};unexpected.exit(0,null);unexpected.beginClose();finishTransportObservation(early,unexpected,true,true);assert.equal(early.passed,false);
  const late=createTransportGuard(()=>{}),trailing={passed:true};late.beginClose();parseMcpLine('trailing invalid JSON',late);late.exit(0,null);finishTransportObservation(trailing,late,true,true);assert.equal(trailing.passed,false);
  const abnormal=createTransportGuard(()=>{}),badExit={passed:true};abnormal.beginClose();abnormal.exit(9,null);finishTransportObservation(badExit,abnormal,true,true);assert.equal(badExit.passed,false);
});
