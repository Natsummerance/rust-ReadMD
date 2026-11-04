'use strict';
// Actual managed installation, execution, durable document history and fresh
// HTTP/MCP processes. No model, host runtime fallback or user documents.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {startKernel}=require('./v006-refactor-host.cjs'),{stdio}=require('./code-lifecycle-http-stdio.cjs'),{options}=require('./v006-table-clipboard-kernel.cjs');
const {nodePack,installNodePack,api:request,settle,sha}=require('./node-runtime-fixture.cjs');
const ROOT=path.resolve(__dirname,'../..'),pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const SOURCE='# 😀\n\n```javascript cmd output=true\nconsole.log(6*7);\n```\n\nSame.\n\n```javascript cmd output=true\nconsole.log(6*7);\n```\n\nTail.\n';
const EXPECTED='# 😀\n\n```javascript cmd output=true\nconsole.log(6*7);\n```\n\nPending.\n\n```javascript cmd output=true\nconsole.log(6*7);\n```\n\n<!-- code_chunk_output -->\n\n42\n\n<!-- /code_chunk_output -->\n\nTail.\n';
const encoded=text=>Buffer.from('\ufeff'+text.replaceAll('\n','\r\n'));
const cas=(s,action,extra={})=>({action,document_id:s.document_id,generation:s.generation,review_revision:s.review_revision,call_id:crypto.randomUUID(),...extra});
async function main(){
  const args=options(process.argv.slice(2));assert(args.bin&&!args.base);const proof=nodePack();
  const profile=path.join(args.evidence,'profile'),docs=path.join(args.evidence,'documents'),tmp=path.join(args.evidence,'tmp'),poison=path.join(args.evidence,'poison-path');for(const dir of [profile,docs,tmp,poison])fs.mkdirSync(dir);
  Object.assign(process.env,{TMPDIR:tmp,TMP:tmp,TEMP:tmp});
  function assertNoProbeCopies(directory=tmp){
    for(const item of fs.readdirSync(directory,{withFileTypes:true})){
      const file=path.join(directory,item.name);assert(!item.isSymbolicLink(),'unexpected link in owned temporary fixture');
      if(item.isDirectory())assertNoProbeCopies(file);
      else assert(!['node','node.exe'].includes(item.name),'a completed probe retained its executable copy');
    }
  }
  fs.writeFileSync(path.join(poison,process.platform==='win32'?'node.cmd':'node'),process.platform==='win32'?'@echo UNMANAGED_NODE_SHOULD_NOT_RUN\r\n':"#!/bin/sh\nprintf UNMANAGED_NODE_SHOULD_NOT_RUN\n",{mode:0o700});
  const file=path.join(docs,'受管代码 中文😀.md');fs.writeFileSync(file,encoded(SOURCE));
  const report={passed:false,platform:process.platform,modelsRequested:false,strictAuth:true,productionCsp:true,nativeUiCertified:false,binarySha256:sha(fs.readFileSync(args.bin)),bootSha256:sha(fs.readFileSync(path.join(ROOT,'assets/readmd.boot.js'))),packSha256:proof.sha256,manifestSha256:proof.manifestSha256,checks:[],processes:[]};
  let host,mcp;
  const api=(route,body)=>request(host,route,body),pack=body=>api('/api/capabilities/packs',body),run=(code,extra={})=>api('/api/code/run',{confirm:true,lang:'javascript',code,...extra});
  async function start(){const previous=process.env.PATH;process.env.PATH=poison;try{host=await startKernel(args.bin,profile,docs);}finally{if(previous===undefined)delete process.env.PATH;else process.env.PATH=previous;}report.processes.push(host.pid);}
  async function review(body){const r=await api('/api/review',body);assert.equal(r.status,200,r.body.error_code);return r.body;}
  try{
    await start();const page=await host.api('/'),html=await page.text(),csp=html.match(/<meta http-equiv="Content-Security-Policy" content="([^"]+)"/)?.[1]||'';
    assert(csp.includes("script-src 'self'"));assert(!/script-src[^;]*'unsafe-(inline|eval)'/.test(csp));
    const anonymous=await fetch(new URL('/api/code/run',page.url),{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({confirm:true,lang:'js',code:'console.log(42)'})});assert([401,403].includes(anonymous.status));
    assert.equal((await api('/api/code/run',{lang:'js',code:'console.log(42)'})).status,400);
    const missing=await run('console.log(6*7)');assert.equal(missing.body.error_code,'capability_pack_unavailable');assert.equal(missing.body.stdout,'');
    const entry=(await pack({action:'catalog'})).body.packs.find(p=>p.id==='node');assert.equal(entry.delivery,'offline-only');assert.equal(entry.url,undefined);assert.equal((await pack({action:'download',id:'node'})).body.error_code,'capability_pack_unavailable');
    report.checks.push('formal CSP/auth/confirmation; empty profile never uses poisoned PATH; offline catalog cannot trigger a download');
    // Corruption is sent to the actual importer, not simulated by a fake lock.
    const damaged=Buffer.from(fs.readFileSync(proof.file));damaged[damaged.length-5]^=0x40;
    const badResponse=await host.api('/api/capabilities/upload?id=node',{method:'POST',headers:{'Content-Type':'application/octet-stream'},body:damaged}),bad=await badResponse.json();assert.notEqual(badResponse.status,200);assert.equal(bad.ok,false);assert.equal((await run('console.log(42)')).body.error_code,'capability_pack_unavailable');assert.deepEqual(fs.readFileSync(file),encoded(SOURCE));
    report.checks.push('actual damaged streamed pack refuses installation and preserves document bytes');
    report.installed=await installNodePack(host,proof);assert.equal((await pack({action:'status',id:'node'})).body.ready,true);assertNoProbeCopies();
    const good=await run('console.log(6*7)');assert.equal(good.body.ok,true);assert.equal(good.body.stdout,'42');assert.deepEqual(good.body.runtime,{id:'node',version:'24.22.2',manifest_sha256:proof.manifestSha256});
    const unicode=await run("console.log('中文😀')");assert.equal(unicode.body.stdout,'中文😀');
    const restricted=await run("const f=require('no'+'de:fs');try{f.writeFileSync('forbidden.tmp','x');console.log('WRITE_ALLOWED')}catch(e){console.log(e.code==='ERR_ACCESS_DENIED'?'WRITE_BLOCKED':'OTHER_ERROR')}");assert.equal(restricted.body.ok,true);assert.equal(restricted.body.stdout,'WRITE_BLOCKED');
    assertNoProbeCopies();report.completedProbeCopiesRetained=0;
    report.checks.push('authenticated chunk import runs the genuine version/Unicode probe; pinned Node works with poisoned PATH and denies ungranted file writes');
    const task=crypto.randomUUID(),running=run("setTimeout(()=>console.log('LATE'),60000)",{task_id:task});let busy;
    for(const end=Date.now()+3000;Date.now()<end;){busy=(await pack({action:'status',id:'node'})).body;if(busy.busy)break;await pause(10);}assert.equal(busy.busy,true);
    assert.equal((await pack({action:'uninstall_start',id:'node'})).status,409);assert.equal((await pack({action:'uninstall_start',id:'node'})).body.error_code,'capability_install_busy');
    const cancelAt=performance.now();assert.equal((await api('/api/task/cancel',{id:task})).body.state,'cancelling');const cancelled=await running;report.cancelResponseMs=performance.now()-cancelAt;assert.equal(cancelled.body.error_code,'execution_cancelled');assert.equal(cancelled.body.stdout,'');assert(report.cancelResponseMs<200);
    const timeoutAt=performance.now(),timed=await run("setTimeout(()=>console.log('LATE'),60000)",{timeout:1});report.timeoutMs=performance.now()-timeoutAt;assert.equal(timed.body.error_code,'execution_timeout');assert(report.timeoutMs<3000);
    report.checks.push('live lease rejects uninstall; actual cancellation and timeout stop without late successful output');
    await api('/api/file?p='+encodeURIComponent(file));const first=await review({action:'open',path:file});
    const pending=await review(cas(first,'propose',{origin:'ai',edits:[{from:SOURCE.indexOf('Same.'),to:SOURCE.indexOf('Same.')+5,insert:'Pending.'}]}));
    const target=await review(cas(pending,'code_target',{source_line:9,code:'console.log(6*7);'}));const output=(await run(target.code_target.code)).body;assert.equal(output.stdout,'42');
    const inserted=await review(cas(pending,'code_output',{target:target.code_target,output:output.stdout}));assert.equal(inserted.content,EXPECTED);assert.equal(inserted.changes[0].original,'Same.');assert.equal(inserted.changes[0].decision,'pending');
    const saved=await api('/api/save',{path:file,content:EXPECTED.replaceAll('\n','\r\n'),encoding:'utf-8-sig',review_session:cas(inserted,'save')});assert.equal(saved.status,200,saved.body.error_code);assert.deepEqual(fs.readFileSync(file),encoded(EXPECTED));
    await host.stop();host=null;
    mcp=stdio(args.bin,profile,docs);report.processes.push(mcp.child.pid);await mcp.request('initialize',{protocolVersion:'2024-11-05',capabilities:{},clientInfo:{name:'managed-node-acceptance',version:'1'}}).result;mcp.notify('notifications/initialized',{});
    const result=await mcp.request('tools/call',{name:'readmd_run_code_chunk',arguments:{confirm:true,language:'javascript',code:'console.log(6*7)'}}).result;assert.equal(result.structuredContent.ok,true);assert.equal(result.structuredContent.runtime.manifest_sha256,proof.manifestSha256);
    async function tool(body){const r=await mcp.request('tools/call',{name:'readmd_review_document',arguments:{...body,confirm:true}}).result;assert.notEqual(r.isError,true,r.structuredContent?.error_code);return r.structuredContent;}
    const head=await tool({action:'open',path:file}),undone=await tool(cas(head,'undo'));assert.equal(undone.content,pending.content);assert.equal(undone.changes[0].original,'Same.');const redone=await tool(cas(undone,'redo'));assert.equal(redone.content,EXPECTED);assert.deepEqual(fs.readFileSync(file),encoded(EXPECTED));await mcp.stop();mcp=null;
    report.checks.push('real JavaScript writes only the second duplicate block; BOM/CRLF save, a fresh MCP process, one undo/redo and pending originals survive');
    await start();const directory=path.join(profile,'capabilities/managed-v2/node/24.22.2'),note=path.join(directory,'user-note.txt');fs.writeFileSync(note,'preserve this unknown user object');
    const removing=await pack({action:'uninstall_start',id:'node'});assert.equal(removing.status,200);const removed=await settle(host,removing.body);assert.equal(removed.phase,'removed',removed.error_code);assert.equal(fs.readFileSync(note,'utf8'),'preserve this unknown user object');assert(!fs.existsSync(path.join(directory,'bin',process.platform==='win32'?'node.exe':'node')));
    assert.equal((await pack({action:'status',id:'node'})).body.ready,false);assert.equal((await run('console.log(42)')).body.error_code,'capability_pack_unavailable');assert.deepEqual(fs.readFileSync(file),encoded(EXPECTED));
    // A new upload can be cancelled before execution; its partial bytes never
    // resurrect the removed runtime or consume another user's files.
    const startUpload=await pack({action:'upload_start',id:'node',format:'stream-v2',total:proof.size,request_key:crypto.randomUUID()});assert.equal(startUpload.status,200);
    const cancelledUpload=await pack({action:'cancel_download',job:startUpload.body.job});assert.equal(cancelledUpload.body.phase,'cancelled');assert.equal((await run('console.log(42)')).body.error_code,'capability_pack_unavailable');
    report.checks.push('third real process uninstalls only pinned objects, preserves the unknown note and saved document; cancelled upload cannot silently reinstall');
    await host.stop();host=null;assertNoProbeCopies();report.passed=true;
  }catch(error){report.failure={name:error.name,message:String(error.message).replaceAll(args.evidence,'<evidence>').replaceAll(ROOT,'<repo>').slice(0,1000)};process.exitCode=1;}
  finally{if(host)await host.stop();if(mcp)await mcp.stop();fs.writeFileSync(path.join(args.evidence,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify(report));}
}
if(require.main===module)main().catch(()=>{console.error('Managed Node acceptance failed; inspect the owned evidence');process.exitCode=1;});
module.exports={main};
