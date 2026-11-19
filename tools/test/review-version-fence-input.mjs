// Sanitized, single-attempt input diagnostics. No URL, headers, error text or body
// is serialized. Only the exact file created by this instance may be removed.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
const names=new Set(['ReadMD-windows-x64.zip','ReadMD-linux-x86_64.tar.gz','ReadMD-macos-arm64.zip','ReadMD-macos-x64.zip']);
const phases=new Set(['request','response','download','archive_authentication','archive_authenticated','extract','binary_authentication','prepare_fixture','fence_behavior','complete']);
const hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
function identity(stat){return{dev:stat.dev,ino:stat.ino,uid:stat.uid};}
function same(stat,id){return stat.isFile()&&!stat.isSymbolicLink()&&stat.dev===id.dev&&stat.ino===id.ino&&stat.uid===id.uid;}
function lstat(file){try{return fs.lstatSync(file,{bigint:true});}catch(error){if(error.code==='ENOENT')return null;throw error;}}
function fingerprint(fd){const h=crypto.createHash('sha256'),buffer=Buffer.alloc(65536);let size=0;for(let n;(n=fs.readSync(fd,buffer,0,buffer.length,size));){h.update(buffer.subarray(0,n));size+=n;}return{size,sha256:h.digest('hex'),available:true};}
function exclusiveJson(file,value){const bytes=Buffer.from(JSON.stringify(value,null,2)+'\n'),fd=fs.openSync(file,'wx',0o600);try{let at=0;while(at<bytes.length)at+=fs.writeSync(fd,bytes,at,bytes.length-at);fs.fsyncSync(fd);}finally{fs.closeSync(fd);}return hash(bytes);}
export function createPreviousWriterInput({directory,name,expected,writerSourceCommit=null,writerRunId=null,consumerSourceCommit=null}){
 if(!path.isAbsolute(directory)||!names.has(name)||!Number.isSafeInteger(expected?.size)||expected.size<=0||!/^[a-f0-9]{64}$/.test(expected?.sha256||''))throw Error('Invalid previous input identity');
 let dir;try{dir=fs.lstatSync(directory);}catch{throw Error('Previous input directory must be ordinary');}if(!dir.isDirectory()||dir.isSymbolicLink())throw Error('Previous input directory must be ordinary');
 const archive=path.join(directory,name),diagnostic=path.join(directory,'input-diagnostics.json'),cleanupFile=path.join(directory,'input-cleanup.json');
 const state={schemaVersion:1,packageName:name,writerSourceCommit:/^[a-f0-9]{40}$/.test(writerSourceCommit||'')?writerSourceCommit:null,writerRunId:Number.isSafeInteger(writerRunId)&&writerRunId>0?writerRunId:null,consumerSourceCommit:/^[a-f0-9]{40}$/.test(consumerSourceCommit||'')?consumerSourceCommit:null,phase:'request',failureCode:null,failurePhase:null,httpStatus:null,expected:{size:expected.size,sha256:expected.sha256},actual:{size:0,sha256:hash(Buffer.alloc(0)),available:false,complete:false},archiveAuthenticated:false,exclusiveFileCreated:false};
 let created=null,started=false,finished=false;
 function failure(code){if(!state.failureCode){state.failureCode=code;state.failurePhase=state.phase;}}
 async function receive(response){
  if(started||finished){failure('repeated_receive');throw Error('Previous fixture download must run once');}started=true;state.phase='response';
  if(!Number.isInteger(response?.status)||response.status<0||response.status>599){failure('invalid_http_status');throw Error('Previous fixture HTTP status invalid');}
  state.httpStatus=response.status;
  if(!response.ok){failure('http_status');try{await response.body?.cancel();}catch{}throw Error('Previous fixture download HTTP '+response.status);}
  if(!response.body||typeof response.body.getReader!=='function'){failure('missing_body');throw Error('Previous fixture response body missing');}
  state.phase='download';let fd=null,reader=null,streamError=false;
  try{
   fd=fs.openSync(archive,'wx+',0o600);created=identity(fs.fstatSync(fd,{bigint:true}));state.exclusiveFileCreated=true;reader=response.body.getReader();
   for(;;){const next=await reader.read();if(next.done)break;if(!(next.value instanceof Uint8Array))throw Error('Invalid stream chunk');const bytes=Buffer.from(next.value);let at=0;while(at<bytes.length)at+=fs.writeSync(fd,bytes,at,bytes.length-at);}
   state.actual.complete=true;
  }catch{streamError=true;failure(created?'stream_failed':'archive_create_failed');}
  finally{
   if(!reader&&streamError)try{await response.body.cancel();}catch{}
   if(reader){if(streamError)try{await reader.cancel();}catch{}try{reader.releaseLock();}catch{failure('reader_release_failed');}}
   if(fd!==null){try{state.actual={...fingerprint(fd),complete:state.actual.complete};}catch{failure('fingerprint_failed');}finally{try{fs.closeSync(fd);}catch{failure('archive_close_failed');}}}
  }
  if(state.failureCode)throw Error('Previous fixture input '+state.failureCode);
  state.phase='archive_authentication';
  if(state.actual.size!==state.expected.size){failure('size_mismatch');throw Error('Previous package differs from certified published fixture');}
  if(state.actual.sha256!==state.expected.sha256){failure('hash_mismatch');throw Error('Previous package differs from certified published fixture');}
  state.archiveAuthenticated=true;state.phase='archive_authenticated';
 }
 function phase(value){if(!phases.has(value)||finished||state.failureCode)throw Error('Invalid previous input phase');state.phase=value;}
 function failed(){failure(state.phase+'_failed');}
 function finalize(){
  if(finished)throw Error('Previous fixture diagnostics already finalized');finished=true;
  // Persist actual size/hash before disposal. Failure to write this receipt
  // leaves the archive untouched rather than recreating the original evidence gap.
  let diagnosticSha256;try{diagnosticSha256=exclusiveJson(diagnostic,{...state,cleanup:{result:'pending',receipt:'input-cleanup.json'}});}catch{throw Error('Previous fixture diagnostics could not be exclusively written');}
  const cleanup={attempted:created!==null,result:'not_created',identityMatched:null,retainedUnknown:false};let cleanupError=false;
  try{
   const current=lstat(archive);
   if(current){if(!created||!same(current,created)){cleanup.result='blocked_identity_changed';cleanup.identityMatched=false;cleanup.retainedUnknown=true;cleanupError=true;}
    else{cleanup.identityMatched=true;fs.unlinkSync(archive);cleanup.result='removed_owned';}}
   else cleanup.result=created?'already_absent':'not_created';
  }catch{cleanup.result='cleanup_failed';cleanupError=true;}
  const result={...state,inputDiagnosticsSha256:diagnosticSha256,cleanup};try{exclusiveJson(cleanupFile,result);}catch{throw Error('Previous fixture cleanup receipt could not be exclusively written');}
  if(cleanupError)throw Error('Previous fixture input cleanup blocked; sanitized receipts retained');
  return result;
 }
 return{receive,phase,failed,finalize};
}
