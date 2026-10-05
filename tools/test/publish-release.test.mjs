import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import { PACKAGE_NAMES, parseChecksums, publish } from '../publish-release.mjs';
const hash=value=>crypto.createHash('sha256').update(value).digest('hex');
test('release manifest rejects missing, duplicate and foreign files',()=>{
  const valid=PACKAGE_NAMES.map(n=>hash(n)+'  '+n).join('\n');
  assert.equal(parseChecksums(valid).size,8);
  assert.throws(()=>parseChecksums(valid+'\n'+valid.split('\n')[0]));
  assert.throws(()=>parseChecksums(valid.split('\n').slice(1).join('\n')));
  assert.throws(()=>parseChecksums(valid.replace(PACKAGE_NAMES[0],'../private.mp4')));
});
function fixture(t,{badUpload=false,renameFailure=false}={}){
  const directory=fs.mkdtempSync(path.join(os.tmpdir(),'readmd-release-test-'));
  t.after(()=>fs.rmSync(directory,{recursive:true,force:true}));
  for(const n of PACKAGE_NAMES)fs.writeFileSync(path.join(directory,n),n);
  fs.writeFileSync(path.join(directory,'SHA256SUMS.txt'),PACKAGE_NAMES.map(n=>hash(n)+'  '+n).join('\n'));
  const commit='a'.repeat(40), original=[...PACKAGE_NAMES,'SHA256SUMS.txt'].map((name,i)=>({id:i+1,name,size:1,state:'uploaded',digest:'sha256:'+hash('old')}));
  const all=structuredClone(original), events=[];let next=20;
  const request=async(method,route,data,{file}={})=>{
    events.push({method,route,data});
    if(route.includes('/git/ref/'))return {object:{type:'commit',sha:commit}};
    if(route.includes('/releases/tags/'))return {id:4,upload_url:'https://uploads.github.com/repos/Natsummerance/rust-ReadMD/releases/4/assets{?name}'};
    if(method==='GET'&&route.includes('/assets?'))return structuredClone(all);
    if(file){const a={id:next++,name:new URL(route).searchParams.get('name'),size:fs.statSync(file).size,state:'uploaded',digest:'sha256:'+(badUpload?'f'.repeat(64):hash(fs.readFileSync(file)))};all.push(a);return structuredClone(a);}
    if(route.includes('/releases/assets/')){
      const id=Number(route.split('/').at(-1)),at=all.findIndex(a=>a.id===id);
      if(method==='DELETE'){all.splice(at,1);return null;}
      if(renameFailure&&id>=20&&data.name===PACKAGE_NAMES[0])throw Error('rename interrupted');
      Object.assign(all[at],data);return structuredClone(all[at]);
    }
    if(method==='PATCH'&&route.endsWith('/releases/4'))return {};
    throw Error('Unexpected request');
  };
  return {directory,commit,repo:'Natsummerance/rust-ReadMD',tag:'V0.0.4',notes:'Current release',request,all,events,original};
}
test('all uploads pass authentication before any public download changes',async t=>{
  const f=fixture(t);await publish(f);
  const firstRename=f.events.findIndex(x=>x.method==='PATCH');
  assert.equal(f.events.slice(0,firstRename).filter(x=>x.method==='POST').length,9);
  assert.deepEqual(f.all.map(a=>a.name).sort(),[...PACKAGE_NAMES,'SHA256SUMS.txt'].sort());
  assert.ok(f.all.every(a=>a.id>=20));
  assert.equal(f.events.filter(x=>x.method==='PATCH'&&x.data?.draft===false).length,1);
});
test('a bad candidate never renames or deletes the old release',async t=>{
  const f=fixture(t,{badUpload:true});await assert.rejects(publish(f),/SHA-256/);
  assert.deepEqual(f.all.filter(a=>a.id<20),f.original);
  assert.ok(f.events.every(x=>!['PATCH','DELETE'].includes(x.method)));
});
test('a failed replacement restores that public package name',async t=>{
  const f=fixture(t,{renameFailure:true});await assert.rejects(publish(f),/interrupted/);
  assert.equal(f.all.find(a=>a.id===1).name,PACKAGE_NAMES[0]);
  assert.ok(f.events.every(x=>x.method!=='DELETE'));
});
