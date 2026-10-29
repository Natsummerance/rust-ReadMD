'use strict';
// Real stdio, isolated profile and real files; no GUI, model or protocol mock.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {spawn}=require('node:child_process');
const root=path.resolve(__dirname,'../..');
async function run(){
  assert(process.env.READMD_BIN,'Explicit READMD_BIN required');
  const base=path.join(root,'.cache/capability-upgrade/v006-batch1-20261027');fs.mkdirSync(base,{recursive:true});
  const output=fs.mkdtempSync(path.join(base,'stdio-')),profile=path.join(output,'profile'),workspace=path.join(output,'workspace');fs.mkdirSync(profile);fs.mkdirSync(workspace);
  const file=path.join(workspace,'Review.md'),original=Buffer.from('\uFEFFA: same.\r\n\r\nB: same.\r\n\r\nC: original.\r\n');fs.writeFileSync(file,original);
  const report={passed:false,transport:'real MCP stdio',processes:[],checks:[],modelCalled:false};let client;
  function start(){
    const child=spawn(path.resolve(process.env.READMD_BIN),['--mcp','--data-dir',profile,'--workspace',workspace,'--assets',path.join(root,'assets')],{cwd:root,windowsHide:true,stdio:['pipe','pipe','pipe'],env:{...process.env,READMD_DATA_DIR:profile}});
    report.processes.push(child.pid);let next=1,buffer='',wireFailure;const pending=new Map();
    const close=new Promise(resolve=>child.once('close',(code,signal)=>resolve({code,signal})));
    function rejectAll(error){wireFailure=error;for(const p of pending.values()){clearTimeout(p.timer);p.reject(error);}pending.clear();}
    child.once('error',rejectAll);child.stdout.setEncoding('utf8');child.stderr.resume();
    child.stdout.on('data',chunk=>{try{buffer+=chunk;assert(buffer.length<=8*1024*1024,'Bounded stdio response');for(;;){const at=buffer.indexOf('\n');if(at<0)break;const line=buffer.slice(0,at);buffer=buffer.slice(at+1);if(!line.trim())continue;const message=JSON.parse(line);if(message.id===undefined)continue;const p=pending.get(message.id);assert(p,'Unknown or duplicate response ID');pending.delete(message.id);clearTimeout(p.timer);message.error?p.reject(Error('MCP protocol error')):p.resolve(message.result);}}catch(error){rejectAll(error);}});
    child.once('exit',()=>{if(pending.size)rejectAll(Error('MCP exited with pending requests'));});
    function request(method,params){if(wireFailure)return Promise.reject(wireFailure);return new Promise((resolve,reject)=>{const id=next++,timer=setTimeout(()=>{pending.delete(id);reject(Error('MCP request deadline'));},15000);pending.set(id,{resolve,reject,timer});child.stdin.write(JSON.stringify({jsonrpc:'2.0',id,method,params})+'\n');});}
    async function tool(args){const result=await request('tools/call',{name:'readmd_review_document',arguments:{...args,confirm:true}});assert.notEqual(result.isError,true,JSON.stringify(result.structuredContent));const value=result.structuredContent;assert.deepEqual(JSON.parse(result.content[0].text),value);return value;}
    async function finish(){child.stdin.end();let timer;const result=await Promise.race([close,new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('Normal MCP EOF deadline')),5000);})]).finally(()=>clearTimeout(timer));assert.deepEqual(result,{code:0,signal:null});assert.equal(buffer.trim(),'');assert.equal(pending.size,0);if(wireFailure)throw wireFailure;}
    return {child,request,tool,finish,close};
  }
  const cas=(s,action,call_id,extra={})=>({action,document_id:s.document_id,generation:s.generation,review_revision:s.review_revision,call_id,...extra});
  async function handshake(){await client.request('initialize',{protocolVersion:'2024-11-05',capabilities:{},clientInfo:{name:'review-contract-qa',version:'1'}});client.child.stdin.write(JSON.stringify({jsonrpc:'2.0',method:'notifications/initialized'})+'\n');const result=await client.request('tools/list',{});for(const name of ['readmd_review_document','readmd_review_ai'])assert(result.tools.some(t=>t.name===name));}
  try{
    client=start();await handshake();let s=await client.tool({action:'open',path:file});assert.equal(s.content,'A: same.\n\nB: same.\n\nC: original.\n');
    const proposal=cas(s,'propose','actual-stdio-proposal',{edits:[{from:3,to:8,insert:'reviewed.'},{from:13,to:18,insert:'reviewed.'}],origin:'ai'});
    s=await client.tool(proposal);assert.deepEqual(await client.tool(proposal),s);const [a,b]=s.changes.map(c=>c.id);assert.notEqual(a,b);report.checks.push('native discovery, exact duplicate ranges and idempotent receipt');
    const c=s.content.indexOf('original.');s=await client.tool(cas(s,'edit','actual-hand-C',{edits:[{from:c,to:c+9,insert:'manual.'}],origin:'editor'}));
    s=await client.tool(cas(s,'resolve','actual-accept-A',{ids:[a],decision:'accept'}));assert.deepEqual(fs.readFileSync(file),original);await client.finish();
    client=start();await handshake();s=await client.tool({action:'open',path:file});assert.equal(s.changes.find(c=>c.id===a).decision,'accepted');assert.equal(s.changes.find(c=>c.id===b).decision,'pending');report.checks.push('unsaved body and independent decisions survive real process EOF and restart');
    s=await client.tool(cas(s,'resolve','actual-withdraw-B',{ids:[b],decision:'withdraw'}));assert.equal(s.content,'A: reviewed.\n\nB: same.\n\nC: manual.\n');
    s=await client.tool(cas(s,'undo','actual-undo'));assert.equal(s.content,'A: reviewed.\n\nB: reviewed.\n\nC: manual.\n');
    s=await client.tool(cas(s,'redo','actual-redo'));assert.equal(s.content,'A: reviewed.\n\nB: same.\n\nC: manual.\n');report.checks.push('reopen, single withdrawal and durable undo/redo keep A and hand C');await client.finish();
    client=start();await handshake();s=await client.tool({action:'open',path:file});assert.equal(s.content,'A: reviewed.\n\nB: same.\n\nC: manual.\n');assert.deepEqual(fs.readFileSync(file),original);await client.finish();
    assert.equal(new Set(report.processes).size,3);report.checks.push('third distinct process restores exact review state without disk-save side effects');report.passed=true;
  }catch(error){report.failure=error.message.slice(0,500);process.exitCode=1;}
  finally{if(client?.child.exitCode===null){client.child.kill();await client.close;}fs.writeFileSync(path.join(output,'report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,report:path.relative(root,path.join(output,'report.json')),failure:report.failure}));}
}
run().catch(error=>{console.error(error.message);process.exitCode=1;});
