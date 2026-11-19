import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import {createPreviousWriterInput} from './review-version-fence-input.mjs';
const hash=value=>crypto.createHash('sha256').update(value).digest('hex'),name='ReadMD-windows-x64.zip',bytes=Buffer.from('owned public fixture bytes');
function fixture(t,{body=bytes,size=body.length,sha256=hash(body)}={}){
 const directory=fs.mkdtempSync(path.join(os.tmpdir(),'previous-writer-input-')),identity=fs.lstatSync(directory);
 t.after(()=>{const current=fs.lstatSync(directory);assert.equal(current.dev,identity.dev);assert.equal(current.ino,identity.ino);assert(!current.isSymbolicLink());fs.rmSync(directory,{recursive:true});});
 const input=createPreviousWriterInput({directory,name,expected:{size,sha256},writerSourceCommit:'e'.repeat(40),writerRunId:41905970855,consumerSourceCommit:'7'.repeat(40)});
 return{directory,input,archive:path.join(directory,name),diagnostic:()=>JSON.parse(fs.readFileSync(path.join(directory,'input-diagnostics.json'))),cleanup:()=>JSON.parse(fs.readFileSync(path.join(directory,'input-cleanup.json')))};
}
function sanitized(value){const text=JSON.stringify(value);assert(!text.includes('PRIVATE_BODY'));assert(!text.includes('secret-token'));assert(!/\/Users\/|\/Applications\/|[A-Za-z]:[\\/]|localhost|127\.0\.0\.1|authorization|bearer\s|responseBody|errorMessage|stack/i.test(text));}
test('matching public bytes alone may enter the downstream phase and both receipts prove owned cleanup',async t=>{
 const f=fixture(t);let downstream=0;await f.input.receive(new Response(bytes,{status:200}));f.input.phase('fence_behavior');downstream++;f.input.phase('complete');const result=f.input.finalize();
 assert.equal(downstream,1);assert.equal(result.phase,'complete');assert.equal(result.httpStatus,200);assert.equal(result.archiveAuthenticated,true);assert.deepEqual(result.actual,{size:bytes.length,sha256:hash(bytes),available:true,complete:true});assert.equal(result.cleanup.result,'removed_owned');assert.equal(result.cleanup.identityMatched,true);assert(!fs.existsSync(f.archive));assert.equal(f.diagnostic().cleanup.result,'pending');assert.equal(result.inputDiagnosticsSha256,hash(fs.readFileSync(path.join(f.directory,'input-diagnostics.json'))));assert.deepEqual(f.cleanup(),result);assert(fs.statSync(path.join(f.directory,'input-diagnostics.json')).size<4096);assert(fs.statSync(path.join(f.directory,'input-cleanup.json')).size<4096);sanitized(result);
});
test('size mismatch remains a hard rejection with actual bytes/hash and removes only its owned archive',async t=>{
 const f=fixture(t,{size:bytes.length+1});let downstream=0;try{await f.input.receive(new Response(bytes));downstream++;}catch{f.input.failed();}const result=f.input.finalize();
 assert.equal(downstream,0);assert.equal(result.failureCode,'size_mismatch');assert.equal(result.failurePhase,'archive_authentication');assert.equal(result.expected.size,bytes.length+1);assert.equal(result.actual.size,bytes.length);assert.equal(result.actual.sha256,hash(bytes));assert.equal(result.actual.complete,true);assert.equal(result.archiveAuthenticated,false);assert.equal(result.cleanup.result,'removed_owned');assert(!fs.existsSync(f.archive));sanitized(result);
});
test('same-size wrong SHA cannot reach extraction and its digest survives disposal',async t=>{
 const f=fixture(t,{sha256:'0'.repeat(64)});let downstream=0;try{await f.input.receive(new Response(bytes));downstream++;}catch{f.input.failed();}const result=f.input.finalize();
 assert.equal(downstream,0);assert.equal(result.failureCode,'hash_mismatch');assert.equal(result.actual.size,result.expected.size);assert.equal(result.actual.sha256,hash(bytes));assert.equal(result.expected.sha256,'0'.repeat(64));assert.equal(result.cleanup.result,'removed_owned');assert(!fs.existsSync(f.archive));sanitized(result);
});
test('abrupt short stream records the actual prefix and releases the reader, without serializing its private exception',async t=>{
 const prefix=Buffer.from('prefix'),f=fixture(t,{body:Buffer.from('prefix and rest')});let pulls=0,downstream=0;const body=new ReadableStream({pull(controller){if(pulls++===0)controller.enqueue(prefix);else controller.error(Error(['PRIVATE_BODY','secret-token',path.join(f.directory,'not-public'),'http://'+['127','0','0','1'].join('.')+'/?sentinel=secret-token'].join(' ')));}},{highWaterMark:0});
 await assert.rejects(async()=>{await f.input.receive(new Response(body));downstream++;},/stream_failed/);assert.equal(body.locked,false);const result=f.input.finalize();
 assert.equal(downstream,0);assert.equal(result.failureCode,'stream_failed');assert.equal(result.failurePhase,'download');assert.equal(result.actual.size,prefix.length);assert.equal(result.actual.sha256,hash(prefix));assert.equal(result.actual.complete,false);assert.equal(result.archiveAuthenticated,false);assert.equal(result.httpStatus,200);assert.equal(result.cleanup.result,'removed_owned');assert(!fs.existsSync(f.archive));sanitized(result);sanitized(f.diagnostic());
});
test('HTTP rejection cancels the body without reading it or creating an archive',async t=>{
 const f=fixture(t);let cancelled=0,read=0;await assert.rejects(f.input.receive({status:503,ok:false,body:{cancel:async()=>{cancelled++;},getReader(){read++;throw Error('PRIVATE_BODY');}}}),/HTTP 503/);const result=f.input.finalize();
 assert.equal(read,0);assert.equal(cancelled,1);assert.equal(result.httpStatus,503);assert.equal(result.failureCode,'http_status');assert.equal(result.actual.available,false);assert.equal(result.actual.complete,false);assert.equal(result.cleanup.result,'not_created');assert(!fs.existsSync(f.archive));sanitized(result);
});
test('a request error before response still records phase and no observed body',t=>{
 const f=fixture(t);f.input.failed();const result=f.input.finalize();assert.equal(result.phase,'request');assert.equal(result.failureCode,'request_failed');assert.equal(result.httpStatus,null);assert.equal(result.actual.available,false);assert.equal(result.cleanup.result,'not_created');sanitized(result);
});
test('a replaced archive object is preserved and marked blocked, never hashed as the owned input',async t=>{
 const f=fixture(t);await f.input.receive(new Response(bytes));fs.renameSync(f.archive,path.join(f.directory,'owned-renamed.zip'));fs.writeFileSync(f.archive,'PRIVATE_BODY replacement',{flag:'wx'});
 assert.throws(()=>f.input.finalize(),/cleanup blocked/);const result=f.cleanup();assert.equal(result.actual.sha256,hash(bytes));assert.equal(result.cleanup.result,'blocked_identity_changed');assert.equal(result.cleanup.retainedUnknown,true);assert.equal(result.cleanup.identityMatched,false);assert.equal(fs.readFileSync(f.archive,'utf8'),'PRIVATE_BODY replacement');sanitized(result);
});
test('a preexisting unknown file is neither opened nor removed when wx creation fails',async t=>{
 const f=fixture(t);fs.writeFileSync(f.archive,'PRIVATE_BODY unknown',{flag:'wx'});const response=new Response(bytes);await assert.rejects(f.input.receive(response),/archive_create_failed/);assert.equal(response.body.locked,false);assert.throws(()=>f.input.finalize(),/cleanup blocked/);const result=f.cleanup();assert.equal(result.exclusiveFileCreated,false);assert.equal(result.actual.available,false);assert.equal(result.cleanup.result,'blocked_identity_changed');assert.equal(fs.readFileSync(f.archive,'utf8'),'PRIVATE_BODY unknown');sanitized(result);
});
test('no archive is removed unless the sanitized pre-cleanup receipt can be exclusively written',async t=>{
 const f=fixture(t);await f.input.receive(new Response(bytes));fs.writeFileSync(path.join(f.directory,'input-diagnostics.json'),'known prior receipt',{flag:'wx'});assert.throws(()=>f.input.finalize(),/diagnostics could not be exclusively written/);assert.equal(fs.readFileSync(path.join(f.directory,'input-diagnostics.json'),'utf8'),'known prior receipt');assert.equal(hash(fs.readFileSync(f.archive)),hash(bytes));assert(!fs.existsSync(path.join(f.directory,'input-cleanup.json')));
});
test('single-attempt context cannot conceal a failure by receiving or finalizing again',async t=>{
 const f=fixture(t,{sha256:'0'.repeat(64)});await assert.rejects(f.input.receive(new Response(bytes)),/certified published fixture/);await assert.rejects(f.input.receive(new Response(bytes)),/must run once/);const result=f.input.finalize();assert.equal(result.failureCode,'hash_mismatch');assert.throws(()=>f.input.finalize(),/already finalized/);assert.equal(result.archiveAuthenticated,false);
});
