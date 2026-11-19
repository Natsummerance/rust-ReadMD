'use strict';
// Real old/new executables, strict HTTP auth and one owned synthetic profile.
// No GUI, no account configuration, no model requests, no token output.
const fs=require('node:fs'),path=require('node:path'),net=require('node:net'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {spawn}=require('node:child_process');const {DatabaseSync}=require('node:sqlite');
const args=process.argv.slice(2),arg=n=>{const i=args.indexOf(n);assert(i>=0&&args[i+1]);return path.resolve(args[i+1]);};
const old=arg('--old-bin'),current=arg('--new-bin'),out=arg('--out'),assets=arg('--assets');
const schema=(name,fallback)=>{const at=args.indexOf(name);const value=at<0?fallback:Number(args[at+1]);assert(Number.isSafeInteger(value)&&value>0);return value;};
const oldSchema=schema('--old-schema',5),newSchema=schema('--new-schema',6);assert(newSchema>oldSchema);
const repo=path.resolve(__dirname,'..'),workspace=path.dirname(repo),pause=ms=>new Promise(r=>setTimeout(r,ms)),sha=b=>crypto.createHash('sha256').update(b).digest('hex');
assert(out.startsWith(workspace+path.sep));fs.mkdirSync(out,{mode:0o700});
const data=path.join(out,'profile'),docs=path.join(out,'documents'),tmp=path.join(out,'tmp'),home=path.join(out,'home');
for(const dir of [data,docs,tmp,home])fs.mkdirSync(dir,{mode:0o700});
const source=path.join(docs,'旧版到新版 中文😀.md'),original=Buffer.from('\ufeffOriginal prose.\r\n');fs.writeFileSync(source,original,{flag:'wx'});
const version=()=>{const db=new DatabaseSync(path.join(data,'review.sqlite3'),{readOnly:true});try{return db.prepare('PRAGMA user_version').get().user_version;}finally{db.close();}};
async function start(binary){
 const sock=net.createServer();await new Promise(r=>sock.listen(0,'127.0.0.1',r));const port=sock.address().port;await new Promise(r=>sock.close(r));
 const env=Object.fromEntries(Object.entries(process.env).filter(([k])=>!/TOKEN|SECRET|CREDENTIAL|API_KEY/i.test(k)));
 Object.assign(env,{READMD_NATIVE_QA:'',READMD_PET_NATIVE_QA:'',READMD_DATA_DIR:data,READMD_WORKSPACE:docs,READMD_EXTRA_ROOTS:'',CFFIXED_USER_HOME:home,TMPDIR:tmp,SQLITE_TMPDIR:tmp});
 const child=spawn(binary,['--no-window','--port',String(port),'--require-token','--data-dir',data,'--workspace',docs,'--assets',assets],{cwd:repo,env,stdio:'ignore'});
 let spawnError;child.on('error',e=>{spawnError=e;});const base='http://127.0.0.1:'+port;
 const stop=async()=>{if(child.exitCode===null&&child.signalCode===null){child.kill('SIGTERM');const end=Date.now()+5000;while(child.exitCode===null&&child.signalCode===null&&Date.now()<end)await pause(20);assert(child.exitCode!==null||child.signalCode!==null,'owned headless process did not stop');}};
 try{const end=Date.now()+20000;while(Date.now()<end){assert(!spawnError&&child.exitCode===null&&child.signalCode===null,'owned backend stopped before readiness');
  try{const r=await fetch(base,{signal:AbortSignal.timeout(1000)}),html=await r.text(),token=html.match(/name="readmd-app-token"\s+content="([^"]+)"/)?.[1];
   if(r.ok&&token)return {stop,api:async(route,body)=>{const r=await fetch(base+route,{method:body===undefined?'GET':'POST',headers:{'X-ReadMD-App-Token':token,'X-ReadMD-Token':token,'Content-Type':'application/json'},...(body===undefined?{}:{body:JSON.stringify(body)}),signal:AbortSignal.timeout(10000)});return {status:r.status,body:await r.json()};}};
  }catch{}await pause(50);
 }throw Error('backend readiness timeout');}catch(e){await stop();throw e;}
}
const checks=[],report={oldBinarySha256:sha(fs.readFileSync(old)),newBinarySha256:sha(fs.readFileSync(current)),oldSchema,newSchema,productionCsp:true,strictAuth:true,modelsRequested:false,nativeHostEvidence:false,controlledHeadlessTermination:true,checks,passed:false};
let host;
const openFile=async()=>{const r=await host.api('/api/file?p='+encodeURIComponent(source));assert.equal(r.status,200);};
const cas=s=>({document_id:s.document_id,generation:s.generation,review_revision:s.review_revision,call_id:crypto.randomUUID()});
(async()=>{try{
 host=await start(old);await openFile();let r=await host.api('/api/review',{action:'open',path:source});assert.equal(r.status,200);const first=r.body;
 r=await host.api('/api/review',{action:'propose',...cas(first),origin:'ai',edits:[{from:0,to:8,insert:'Updated'}]});assert.equal(r.status,200);const pending=r.body;
 assert.equal(version(),oldSchema);checks.push('actual_released_writer_created_version_'+oldSchema+'_pending_review');await host.stop();host=null;
 host=await start(current);await openFile();r=await host.api('/api/review',{action:'read',document_id:pending.document_id});assert.equal(r.status,200);assert.equal(version(),newSchema);
 assert.equal(r.body.content,pending.content);assert.equal(r.body.changes[0].original,'Original');assert.equal(r.body.changes[0].decision,'pending');
 checks.push('actual_new_writer_migrates_without_losing_identity_original_or_decision');await host.stop();host=null;
 const before=sha(fs.readFileSync(path.join(data,'review.sqlite3')));
 host=await start(old);r=await host.api('/api/review',{action:'read',document_id:pending.document_id});assert.equal(r.body.error_code,'review_unsupported_schema');assert(r.status>=400);
 checks.push('actual_released_reader_refuses_new_semantic_format');
 const fileRead=await host.api('/api/file?p='+encodeURIComponent(source));
 assert(fileRead.status===200||fileRead.body.error_code==='review_unsupported_schema');
 for(const review of [undefined,cas(pending)]){
  r=await host.api('/api/save',{path:source,content:'must never be written',encoding:'utf-8-sig',...(review?{review_session:review}:{})});
  assert(r.status>=400);assert.equal(r.body.error_code,'review_unsupported_schema');assert.deepEqual(fs.readFileSync(source),original);
 }
 await host.stop();host=null;assert.equal(sha(fs.readFileSync(path.join(data,'review.sqlite3'))),before);assert.equal(version(),newSchema);
 checks.push('old_review_and_legacy_saves_refuse_without_mutating_document_or_database');
 host=await start(current);await openFile();r=await host.api('/api/review',{action:'read',document_id:pending.document_id});assert.equal(r.status,200);assert.equal(r.body.content,pending.content);
 r=await host.api('/api/save',{path:source,content:r.body.content.replace(/\n/g,'\r\n'),encoding:'utf-8-sig',review_session:cas(r.body)});assert.equal(r.status,200);assert.equal(r.body.ok,true);assert.equal(r.body.review.changes[0].decision,'pending');
 assert.deepEqual(fs.readFileSync(source),Buffer.from('\ufeffUpdated prose.\r\n'));checks.push('new_writer_reopens_and_commits_visible_pending_body_after_old_writer_rejection');report.passed=true;
 }finally{if(host)await host.stop();fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx',mode:0o600});}
 console.log(JSON.stringify({reviewVersionFencePassed:true,checks:checks.length}));
})().catch(error=>{
 const failure={name:error.name,code:error.code||null,message:String(error.message).replaceAll(workspace,'<workspace>').slice(0,1200)};
 fs.writeFileSync(path.join(out,'first-failure.json'),JSON.stringify(failure,null,2)+'\n',{flag:'wx',mode:0o600});
 console.error('Review version fence failed: '+failure.message);process.exitCode=1;
});
