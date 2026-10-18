'use strict';
// Real self-hosted binary, durable WAL backup and restart. No credentials in logs.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {spawn,spawnSync}=require('node:child_process'),crypto=require('node:crypto');
const Y=require('../tools/collaboration-bundle/node_modules/yjs');
const root=path.resolve(__dirname,'..');
const binary=process.env.READMD_COLLAB_BIN||path.join(root,'.cache/community-upgrade/build/debug/readmd-collaboration.exe');
const base=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/selfhost-'));
const data=path.join(base,'original'),restored=path.join(base,'restored'),backup=path.join(base,'backup.sqlite');
const admin=crypto.randomBytes(32).toString('hex'),port=Number(process.env.READMD_COLLAB_PORT||28872),url='http://127.0.0.1:'+port;
let child,phase='start',stderr='';const checks=[];
async function stop(){if(!child)return;const current=child;child=null;if(current.exitCode===null){const done=new Promise(r=>current.once('exit',r));current.kill();await done;}}
async function start(profile){
  stderr='';child=spawn(binary,['--data-dir',profile,'--assets',path.join(root,'assets'),'--listen','127.0.0.1:'+port],{windowsHide:true,env:{...process.env,READMD_COLLAB_ADMIN_TOKEN:admin},stdio:['ignore','ignore','pipe']});
  child.stderr.on('data',b=>{stderr=(stderr+b).slice(-1024);});
  for(let i=0;i<100;i++){
    if(child.exitCode!==null)throw new Error('Service did not start');
    try{const response=await fetch(url+'/',{signal:AbortSignal.timeout(500)});if(response.ok)return;}catch{}
    await new Promise(r=>setTimeout(r,50));
  }throw new Error('Service startup timeout');
}
async function api(route,body,credential,extra={}){
  const r=await fetch(url+route,{method:'POST',headers:{'Content-Type':'application/json',Authorization:'Bearer '+(credential?.token||admin),...extra},body:JSON.stringify(body),signal:AbortSignal.timeout(5000)});
  return{status:r.status,data:await r.json()};
}
function cli(args,success=true){const result=spawnSync(binary,args,{windowsHide:true,encoding:'utf8',timeout:15000,env:{...process.env,READMD_COLLAB_ADMIN_TOKEN:''}});assert.equal(result.status===0,success,'CLI exit status');return result;}
(async()=>{
  const report={platform:process.platform,binarySha256:crypto.createHash('sha256').update(fs.readFileSync(binary)).digest('hex'),checks};
  try{
    await start(data);phase='authorization';assert.equal((await api('/sessions',{content:'Unauthorized'},{token:'invalid'})).status,403);
    const created=await api('/sessions',{title:'Synthetic research',content:'# Evidence\nOriginal 🦀\n',name:'Author'});assert.equal(created.status,200);const owner=created.data.credential,route='/sessions/'+owner.room+'/';
    assert.equal((await api(route+'snapshot',{},owner,{Origin:'https://unrelated.invalid'})).status,403);checks.push('admin and origin checks');
    phase='durable edit';const doc=new Y.Doc();const initial=await api(route+'sync',{vector:Buffer.from(Y.encodeStateVector(doc)).toString('base64')},owner);assert.equal(initial.status,200);Y.applyUpdate(doc,Buffer.from(initial.data.update,'base64'));
    const vector=Y.encodeStateVector(doc);doc.getText('content').insert(doc.getText('content').length,'Saved collaborative text\n');
    assert.equal((await api(route+'sync',{vector:Buffer.from(vector).toString('base64'),update:Buffer.from(Y.encodeStateAsUpdate(doc,vector)).toString('base64')},owner)).status,200);
    const expected=doc.getText('content').toString();assert.equal((await api(route+'snapshot',{},owner)).data.content,expected);
    assert.equal((await api(route+'review',{text:'Confirm the original source.'},owner)).status,200);
    const attachment=Buffer.from('An original synthetic attachment.');const upload=await fetch(url+route+'attachment',{method:'POST',headers:{Authorization:'Bearer '+owner.token,'Content-Type':'application/octet-stream','X-Asset-Name-Base64':Buffer.from('演示资料.txt').toString('base64')},body:attachment});assert.equal(upload.status,200);const asset=(await upload.json()).id;
    phase='live WAL backup';cli(['--data-dir',data,'--backup',backup]);const digest=crypto.createHash('sha256').update(fs.readFileSync(backup)).digest('hex');
    cli(['--data-dir',data,'--backup',backup],false);assert.equal(crypto.createHash('sha256').update(fs.readFileSync(backup)).digest('hex'),digest);checks.push('live WAL snapshot and overwrite refusal');
    await stop();phase='service restart';await start(data);assert.equal((await api(route+'snapshot',{},owner)).data.content,expected);checks.push('service restart retains acknowledged edit');await stop();
    phase='restore';cli(['--data-dir',restored,'--restore',backup]);cli(['--data-dir',restored,'--restore',backup],false);await start(restored);
    assert.equal((await api(route+'snapshot',{},owner)).data.content,expected);
    assert.ok(JSON.stringify((await api(route+'reviews',{},owner)).data).includes('Confirm the original source.'));
    const download=await fetch(url+route+'download',{method:'POST',headers:{Authorization:'Bearer '+owner.token,'Content-Type':'application/json'},body:JSON.stringify({id:asset})});assert.equal(download.status,200);assert.deepEqual(Buffer.from(await download.arrayBuffer()),attachment);checks.push('restore preserves document, credentials, reviews and attachments');
    for(const route of ['/api/file','/Cargo.toml','/collaboration.sqlite'])assert.equal((await fetch(url+route)).status,404);
    const css=await fetch(url+'/tokens.css');assert.equal(css.status,200);assert.ok((await css.text()).includes('--bg'));checks.push('asset allowlist and shared UI tokens');
    report.passed=true;
  }catch(error){report.passed=false;report.failedStage=phase;report.error=error.name;throw error;}
  finally{await stop();report.recordedAt=new Date().toISOString();fs.writeFileSync(path.join(base,'report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,checks:checks.length,report:path.relative(root,path.join(base,'report.json')),failedStage:report.failedStage}));}
})().catch(()=>{console.error('Self-host validation failed; stage recorded without credentials.');process.exitCode=1;});
