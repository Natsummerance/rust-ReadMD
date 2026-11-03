import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import {PACKAGE_NAMES} from '../publish-release.mjs';
import {MAC_NAMES,PLATFORM_NAMES,mergedChecksums,mergedPlatformChecksums,refreshMacos,refreshPlatforms} from '../refresh-macos-release.mjs';
const sha=data=>crypto.createHash('sha256').update(data).digest('hex');
function fixture(t,{badUpload=false,failRenameAt=0,platforms=false}={}) {
  const directory=fs.mkdtempSync(path.join(os.tmpdir(),'readmd-mac-refresh-'));
  t.after(()=>fs.rmSync(directory,{recursive:true,force:true}));
  const sourceCommit='a'.repeat(40),tagCommit='b'.repeat(40),events=[],checkpoints=[];
  const before={id:4,tag_name:'V'+fs.readFileSync(new URL('../../VERSION',import.meta.url),'utf8').trim(),body:'Original release notes',draft:false,immutable:false,upload_url:'https://uploads.github.com/repos/Natsummerance/rust-ReadMD/releases/4/assets{?name}',assets:[...PACKAGE_NAMES,'SHA256SUMS.txt'].map((name,i)=>({id:i+1,name,state:'uploaded',size:3,digest:'sha256:'+sha('old')}))};
  const names=platforms?PLATFORM_NAMES:MAC_NAMES,bootSha256=sha('same kernel boot');
  const packages=names.map(name=>{const bytes=Buffer.from('verified '+name);fs.writeFileSync(path.join(directory,name),bytes);return {name,size:bytes.length,sha256:sha(bytes),signatureValid:true,coldStartupPassed:true,buildRunId:23,archiveInspected:true,sourceBootSha256:bootSha256,bootSha256,integrationFilesVerified:true};});
  fs.writeFileSync(path.join(directory,'SHA256SUMS.txt'),(platforms?mergedPlatformChecksums:mergedChecksums)(before,packages));
  const verification={sourceCommit,tagCommit,nativeWorkflow:{passed:true},signing:'ad-hoc',notarized:false,packages,ciRunId:23,bootSha256};
  const run={head_sha:sourceCommit,conclusion:'success',status:'completed',event:'workflow_dispatch',path:'.github/workflows/release.yml'};
  const jobs=['Build · Windows x64','Build · Linux x64','Kernel tests · windows-latest','Kernel tests · ubuntu-latest','VS Code and MCP quality','Frontend gates'].map((name,index)=>({id:100+index,name,conclusion:'success',steps:[{name:'P0 lifecycle against the actual Windows or Linux release binary',conclusion:'success'}]}));
  const all=structuredClone(before.assets),release={...before};let next=100,renames=0;
  const request=async(method,route,data,{file}={})=>{
    events.push({method,route,data});
    if(route.includes('/git/ref/'))return {object:{type:'commit',sha:tagCommit}};
    if(route.includes('/commits/'))return {sha:sourceCommit};
    if(route.endsWith('/actions/runs/23'))return structuredClone(run);
    if(route.endsWith('/actions/runs/23/jobs?filter=all&per_page=100'))return {total_count:jobs.length,jobs:structuredClone(jobs)};
    if(method==='GET'&&route.includes('/releases/tags/'))return structuredClone(release);
    if(method==='GET'&&route.endsWith('/assets?per_page=100'))return structuredClone(all);
    if(file){const asset={id:next++,name:new URL(route).searchParams.get('name'),state:'uploaded',size:fs.statSync(file).size,digest:'sha256:'+(badUpload?'f'.repeat(64):sha(fs.readFileSync(file)))};all.push(asset);return structuredClone(asset);}
    if(route.includes('/releases/assets/')) {
      const index=all.findIndex(a=>a.id===Number(route.split('/').at(-1)));
      if(method==='DELETE'){all.splice(index,1);return null;}
      if(method==='PATCH'){renames++;if(renames===failRenameAt)throw Error('controlled rename interruption');Object.assign(all[index],data);return structuredClone(all[index]);}
    }
    if(method==='PATCH'&&route.endsWith('/releases/4')){Object.assign(release,data);return structuredClone(release);}
    throw Error('Unexpected request');
  };
  return {before,directory,sourceCommit,tagCommit,notes:'Original release notes\nVerified package refresh',verification,request,checkpoint:j=>checkpoints.push(structuredClone(j)),all,events,checkpoints,release,run,jobs};
}
test('Mac replacement stages all five verified files, preserves the original tag and every other platform',async t=>{
  const f=fixture(t),result=await refreshMacos(f);
  assert.equal(result.status,'complete');
  const firstRename=f.events.findIndex(e=>e.method==='PATCH');
  assert.equal(f.events.slice(0,firstRename).filter(e=>e.method==='POST').length,5);
  assert.deepEqual(f.all.filter(a=>!MAC_NAMES.includes(a.name)&&a.name!=='SHA256SUMS.txt'),f.before.assets.filter(a=>!MAC_NAMES.includes(a.name)&&a.name!=='SHA256SUMS.txt'));
  assert.equal(f.all.length,f.before.assets.length);
  assert(f.events.every(e=>!e.route.includes('/git/')||e.method==='GET'));
  assert.deepEqual(f.checkpoints.map(j=>j.status).filter((s,i,a)=>i===0||s!==a[i-1]),['staging','replacing','verified','complete']);
  assert.equal(f.release.body,f.notes);
});
test('a release for another version cannot receive this version of the app',async t=>{
  const f=fixture(t);f.before.tag_name='V0.0.1';
  await assert.rejects(refreshMacos(f),/Invalid existing release target/);
  assert.equal(f.events.length,0);
});
test('a mid-batch failure restores all original public download identities and keeps recovery candidates',async t=>{
  const f=fixture(t,{failRenameAt:5});
  await assert.rejects(refreshMacos(f),/rolled_back.*interruption/);
  for(const old of f.before.assets)assert.deepEqual(f.all.find(a=>a.id===old.id),old);
  assert(f.events.every(e=>e.method!=='DELETE'));
  assert.equal(f.release.body,f.before.body);
  assert.equal(f.checkpoints.at(-1).status,'rolled_back');
});
test('bad upload digests leave public downloads and release notes untouched',async t=>{
  const f=fixture(t,{badUpload:true});await assert.rejects(refreshMacos(f),/SHA-256/);
  assert(f.events.every(e=>!['PATCH','DELETE'].includes(e.method)));
});
test('a changed remote asset aborts before upload or rename',async t=>{
  const f=fixture(t);f.all[0].digest='sha256:'+'c'.repeat(64);
  await assert.rejects(refreshMacos(f),/changed since inspection/);
  assert(f.events.every(e=>e.method==='GET'));
});
test('unverified local packages and foreign files cannot be uploaded',async t=>{
  const f=fixture(t);f.verification.packages[0].coldStartupPassed=false;
  await assert.rejects(refreshMacos(f),/verified artifact/);assert.equal(f.events.length,0);
  f.verification.packages[0].coldStartupPassed=true;fs.writeFileSync(path.join(f.directory,'private.txt'),'owned synthetic fixture');
  await assert.rejects(refreshMacos(f),/Only Mac/);assert.equal(f.events.length,0);
});
test('merged checksums cannot silently replace Windows or Linux bytes',async t=>{
  const f=fixture(t);const file=path.join(f.directory,'SHA256SUMS.txt');fs.writeFileSync(file,fs.readFileSync(file,'utf8').replace(sha('old'),'d'.repeat(64)));
  await assert.rejects(refreshMacos(f),/changed another platform/);assert.equal(f.events.length,0);
});
test('Windows/Linux/integrations stage seven files and preserve all four accepted Mac packages',async t=>{
  const f=fixture(t,{platforms:true}),result=await refreshPlatforms(f);
  assert.equal(result.status,'complete');
  const firstRename=f.events.findIndex(e=>e.method==='PATCH');
  assert.equal(f.events.slice(0,firstRename).filter(e=>e.method==='POST').length,7);
  for(const name of MAC_NAMES)assert.deepEqual(f.all.find(a=>a.name===name),f.before.assets.find(a=>a.name===name));
  assert.equal(f.all.length,f.before.assets.length);
  assert(f.events.every(e=>!e.route.includes('/git/')||e.method==='GET'));
});
test('platform refresh refuses a stale CI source before uploading',async t=>{
  const f=fixture(t,{platforms:true});f.run.head_sha='e'.repeat(40);
  await assert.rejects(refreshPlatforms(f),/workflow has not passed/);
  assert(f.events.every(e=>e.method==='GET'));
});
test('platform refresh requires the real release-binary lifecycle job and inspected integration archives',async t=>{
  const f=fixture(t,{platforms:true});f.jobs[0].steps[0].conclusion='skipped';
  await assert.rejects(refreshPlatforms(f),/lifecycle regression has not passed/);
  assert(f.events.every(e=>e.method==='GET'));
  f.events.length=0;f.jobs[0].steps[0].conclusion='success';f.verification.packages.find(p=>p.name.endsWith('.vsix')).integrationFilesVerified=false;
  await assert.rejects(refreshPlatforms(f),/verified artifact/);assert.equal(f.events.length,0);
});
test('platform checksum merging cannot change accepted Mac downloads',async t=>{
  const f=fixture(t,{platforms:true}),file=path.join(f.directory,'SHA256SUMS.txt');
  const lines=fs.readFileSync(file,'utf8').split('\n');const at=lines.findIndex(line=>line.endsWith('ReadMD-macos-arm64.zip'));lines[at]=lines[at].replace(/^[a-f\d]{64}/,'d'.repeat(64));fs.writeFileSync(file,lines.join('\n'));
  await assert.rejects(refreshPlatforms(f),/changed another platform/);assert.equal(f.events.length,0);
});
test('a platform rename interruption rolls back the entire batch without deleting old assets',async t=>{
  const f=fixture(t,{platforms:true,failRenameAt:9});
  await assert.rejects(refreshPlatforms(f),/rolled_back.*interruption/);
  for(const old of f.before.assets)assert.deepEqual(f.all.find(a=>a.id===old.id),old);
  assert(f.events.every(e=>e.method!=='DELETE'));
  assert.equal(f.checkpoints.at(-1).status,'rolled_back');
});
test('platform admission uses the latest job attempt and cannot hide a new failure behind an old success',async t=>{
  const f=fixture(t,{platforms:true});f.jobs.push({...f.jobs[0],id:99,conclusion:'failure'});
  assert.equal((await refreshPlatforms(f)).status,'complete');
  const g=fixture(t,{platforms:true});g.jobs.push({...g.jobs[0],id:200,conclusion:'failure'});
  await assert.rejects(refreshPlatforms(g),/Required platform job did not pass/);
  assert(g.events.every(e=>e.method==='GET'));
});
