import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import { PACKAGE_NAMES, parseChecksums, writeChecksums, releaseNotes, publish } from '../publish-release.mjs';
const version=fs.readFileSync(new URL('../../VERSION',import.meta.url),'utf8').trim();
const hash=value=>crypto.createHash('sha256').update(value).digest('hex');
test('release manifest rejects missing, duplicate and foreign files',()=>{
  const valid=PACKAGE_NAMES.map(n=>hash(n)+'  '+n).join('\n');
  assert.equal(parseChecksums(valid).size,PACKAGE_NAMES.length);
  assert.throws(()=>parseChecksums(valid+'\n'+valid.split('\n')[0]));
  assert.throws(()=>parseChecksums(valid.split('\n').slice(1).join('\n')));
  assert.throws(()=>parseChecksums(valid.replace(PACKAGE_NAMES[0],'../private.mp4')));
});
test('checksums are generated for exactly ten nonempty regular packages',t=>{
  const directory=fs.mkdtempSync(path.join(os.tmpdir(),'readmd-checksums-test-'));
  t.after(()=>fs.rmSync(directory,{recursive:true,force:true}));
  assert.equal(PACKAGE_NAMES.length,10);
  for(const name of PACKAGE_NAMES)fs.writeFileSync(path.join(directory,name),name);
  fs.writeFileSync(path.join(directory,'report.json'),'evidence');
  assert.throws(()=>writeChecksums(directory),/must contain only/);
  fs.rmSync(path.join(directory,'report.json'));
  fs.mkdirSync(path.join(directory,'evidence'));
  assert.throws(()=>writeChecksums(directory),/must contain only/);
  fs.rmSync(path.join(directory,'evidence'),{recursive:true});
  const first=path.join(directory,PACKAGE_NAMES[0]);
  fs.writeFileSync(first,'');
  assert.throws(()=>writeChecksums(directory),/Empty release package/);
  fs.writeFileSync(first,PACKAGE_NAMES[0]);
  const entries=writeChecksums(directory);
  for(const name of PACKAGE_NAMES)assert.equal(entries.get(name),hash(name));
  assert.throws(()=>writeChecksums(directory),/must contain only/,'a previous manifest is not silently replaced');
});
test('GitHub release notes contain only this version and commit-bound document links',()=>{
  const tag='V'+version,commit='a'.repeat(40),repo='Natsummerance/rust-ReadMD';
  const current='# ReadMD '+tag+'\n\nCurrent [review](docs/reviews/current.md) and [install](release/macos-install.md "Mac install").\n\n## Current details\n\n[Site](https://readmd.example/) and [anchor](#current-details).';
  const text='# ReadMD V9.9.9\n\nFuture\n\n---\n\n'+current+'\n\n---\n\n# ReadMD V0.0.1\n\nHistorical';
  const notes=releaseNotes(text,{tag,commit,repo});
  assert.equal(notes,current.replace('](docs/reviews/current.md)','](https://github.com/'+repo+'/blob/'+commit+'/docs/reviews/current.md)').replace('](release/macos-install.md "Mac install")','](https://github.com/'+repo+'/blob/'+commit+'/release/macos-install.md "Mac install")')+'\n');
  assert.doesNotMatch(notes,/Future|Historical|---/);
  assert.throws(()=>releaseNotes('# ReadMD V0.0.1\nOther',{tag,commit,repo}),/exactly one section/);
  assert.throws(()=>releaseNotes(current+'\n\n'+current,{tag,commit,repo}),/exactly one section/);
});
function fixture(t,{badUpload=false,renameFailure=false,missingTag=false,raceTag,wrongTag=false,newRelease=false,publishTimeout=false,uncertainDraft=false,draftRelease=false,draftCommit}={}){
  const directory=fs.mkdtempSync(path.join(os.tmpdir(),'readmd-release-test-'));
  t.after(()=>fs.rmSync(directory,{recursive:true,force:true}));
  for(const n of PACKAGE_NAMES)fs.writeFileSync(path.join(directory,n),n);
  fs.writeFileSync(path.join(directory,'SHA256SUMS.txt'),PACKAGE_NAMES.map(n=>hash(n)+'  '+n).join('\n'));
  const commit='a'.repeat(40), original=newRelease?[]:[...PACKAGE_NAMES,'SHA256SUMS.txt'].map((name,i)=>({id:i+1,name,size:1,state:'uploaded',digest:'sha256:'+hash('old')}));
  const all=structuredClone(original), events=[];let next=20;
  let tagObject=missingTag?null:{type:'commit',sha:wrongTag?'b'.repeat(40):commit};
  let release=newRelease?null:{id:4,draft:draftRelease,tag_name:'V'+version,target_commitish:draftCommit||commit,upload_url:'https://uploads.github.com/repos/Natsummerance/rust-ReadMD/releases/4/assets{?name}'};
  const request=async(method,route,data,{file}={})=>{
    events.push({method,route,data});
    if(route.includes('/git/ref/'))return tagObject?{object:structuredClone(tagObject)}:null;
    if(method==='POST'&&route.endsWith('/git/refs')) {
      tagObject={type:'commit',sha:raceTag==='different'?'b'.repeat(40):data.sha};
      if(raceTag)throw Error('GitHub release request failed: POST HTTP 422');
      return {object:structuredClone(tagObject)};
    }
    if(method==='DELETE'&&route.includes('/git/refs/tags/')){tagObject=null;return null;}
    if(route.includes('/releases/tags/'))return release?.draft?null:structuredClone(release);
    if(method==='GET'&&route.includes('/releases?per_page='))return release?[structuredClone(release)]:[];
    if(method==='POST'&&route.endsWith('/releases')) {
      release={...data,id:4,upload_url:'https://uploads.github.com/repos/Natsummerance/rust-ReadMD/releases/4/assets{?name}'};
      return structuredClone(release);
    }
    if(method==='GET'&&route.endsWith('/releases/4')) {
      if(uncertainDraft)throw Error('Release state unavailable');
      return structuredClone(release);
    }
    if(method==='GET'&&route.includes('/assets?'))return structuredClone(all);
    if(file){const a={id:next++,name:new URL(route).searchParams.get('name'),size:fs.statSync(file).size,state:'uploaded',digest:'sha256:'+(badUpload?'f'.repeat(64):hash(fs.readFileSync(file)))};all.push(a);return structuredClone(a);}
    if(route.includes('/releases/assets/')){
      const id=Number(route.split('/').at(-1)),at=all.findIndex(a=>a.id===id);
      if(method==='DELETE'){all.splice(at,1);return null;}
      if(renameFailure&&id>=20&&data.name===PACKAGE_NAMES[0])throw Error('rename interrupted');
      Object.assign(all[at],data);return structuredClone(all[at]);
    }
    if(method==='PATCH'&&route.endsWith('/releases/4')) {
      Object.assign(release,data);
      if(publishTimeout)throw Error('Publish response timed out after server update');
      return structuredClone(release);
    }
    throw Error('Unexpected request');
  };
  return {directory,commit,repo:'Natsummerance/rust-ReadMD',tag:'V'+version,notes:'# ReadMD V'+version+'\n\nCurrent [review](docs/reviews/current.md)\n\n---\n\n# ReadMD V0.0.1\n\nHistorical',request,all,events,original};
}
test('shared publication rejects another app version before local or remote access',async t=>{
  const f=fixture(t),[major,minor,patch]=version.split('.').map(Number),otherVersion=[major,minor,patch+1].join('.');
  for(const prefix of ['v','V'])await assert.rejects(publish({...f,directory:path.join(f.directory,'absent'),tag:prefix+otherVersion}),/Tag and application version differ/);
  assert.deepEqual(f.events,[]);
  assert.deepEqual(f.all,f.original);
});
test('the shared publisher accepts a lowercase current-version tag',async t=>{
  const f=fixture(t);await publish({...f,tag:'v'+version});
  assert.ok(f.events.some(x=>x.route.endsWith('/git/ref/tags/v'+version)));
  assert.equal(f.events.filter(x=>x.method==='PATCH'&&x.data?.draft===false).length,1);
});
test('all uploads pass authentication before any public download changes',async t=>{
  const f=fixture(t);await publish(f);
  const firstRename=f.events.findIndex(x=>x.method==='PATCH');
  assert.equal(f.events.slice(0,firstRename).filter(x=>x.method==='POST').length,PACKAGE_NAMES.length+1);
  assert.deepEqual(f.all.map(a=>a.name).sort(),[...PACKAGE_NAMES,'SHA256SUMS.txt'].sort());
  assert.ok(f.all.every(a=>a.id>=20));
  assert.equal(f.events.filter(x=>x.method==='PATCH'&&x.data?.draft===false).length,1);
  const body=f.events.find(x=>x.method==='PATCH'&&x.data?.draft===false).data.body;
  assert.doesNotMatch(body,/Historical|V0\.0\.1/);
  assert.ok(body.includes('https://github.com/'+f.repo+'/blob/'+f.commit+'/docs/reviews/current.md'));
});
test('extra evidence cannot be ignored when publishing package downloads',async t=>{
  const f=fixture(t);fs.mkdirSync(path.join(f.directory,'evidence'));
  await assert.rejects(publish(f),/must contain only/);
  assert.deepEqual(f.events,[]);
});
test('manual publication creates the exact lightweight version tag after local validation',async t=>{
  const f=fixture(t,{missingTag:true,newRelease:true});await publish({...f,createTag:true});
  const tags=f.events.filter(x=>x.method==='POST'&&x.route.endsWith('/git/refs'));
  assert.deepEqual(tags.map(x=>x.data),[{ref:'refs/tags/'+f.tag,sha:f.commit}]);
  assert.ok(f.events.every(x=>!(x.method==='PATCH'&&x.route.includes('/git/refs/'))));
  assert.ok(!f.events.some(x=>x.method==='DELETE'&&x.route.includes('/git/refs/')));
});
test('tag pushes cannot create a missing tag and invalid local packages create no tag',async t=>{
  const f=fixture(t,{missingTag:true,newRelease:true});
  await assert.rejects(publish(f),/must already exist/);
  assert.ok(f.events.every(x=>x.method==='GET'));
  f.events.length=0;
  fs.appendFileSync(path.join(f.directory,PACKAGE_NAMES[0]),'tampered');
  await assert.rejects(publish({...f,createTag:true}),/checksum mismatch/);
  assert.deepEqual(f.events,[]);
});
test('existing version tags are preserved and a different commit is rejected before mutation',async t=>{
  const same=fixture(t);await publish({...same,createTag:true});
  assert.ok(same.events.every(x=>!x.route.includes('/git/refs')));
  const other=fixture(t,{wrongTag:true});
  await assert.rejects(publish({...other,createTag:true}),/does not point to this verified build/);
  assert.ok(other.events.every(x=>x.method==='GET'));
});
test('a concurrently created tag is rechecked and never changed or removed',async t=>{
  const same=fixture(t,{missingTag:true,newRelease:true,raceTag:'same'});await publish({...same,createTag:true});
  assert.ok(!same.events.some(x=>['PATCH','DELETE'].includes(x.method)&&x.route.includes('/git/refs')));
  const other=fixture(t,{missingTag:true,newRelease:true,raceTag:'different'});
  await assert.rejects(publish({...other,createTag:true}),/does not point to this verified build/);
  assert.ok(!other.events.some(x=>x.route.includes('/releases')||x.method==='DELETE'));
});
test('a failed upload removes only its own new tag while its new release is confirmed private',async t=>{
  const f=fixture(t,{missingTag:true,newRelease:true,badUpload:true});
  await assert.rejects(publish({...f,createTag:true}),/SHA-256/);
  assert.equal(f.events.filter(x=>x.method==='DELETE'&&x.route.endsWith('/git/refs/tags/'+f.tag)).length,1);
  const existing=fixture(t,{newRelease:true,badUpload:true});
  await assert.rejects(publish({...existing,createTag:true}),/SHA-256/);
  assert.ok(!existing.events.some(x=>x.method==='DELETE'));
});
test('ambiguous release state or a publish timeout preserves the verified tag for retry',async t=>{
  const unclear=fixture(t,{missingTag:true,newRelease:true,badUpload:true,uncertainDraft:true});
  await assert.rejects(publish({...unclear,createTag:true}),/SHA-256/);
  assert.ok(!unclear.events.some(x=>x.method==='DELETE'));
  const timeout=fixture(t,{missingTag:true,newRelease:true,publishTimeout:true});
  await assert.rejects(publish({...timeout,createTag:true}),/Publish response timed out/);
  assert.ok(!timeout.events.some(x=>x.method==='DELETE'));
});
test('a retry discovers its matching private draft and refuses a draft for another source',async t=>{
  const own=fixture(t,{draftRelease:true});await publish({...own,createTag:true});
  assert.ok(own.events.some(x=>x.method==='GET'&&x.route.includes('/releases?per_page=')));
  assert.ok(!own.events.some(x=>x.method==='POST'&&x.route.endsWith('/releases')));
  const other=fixture(t,{draftRelease:true,draftCommit:'b'.repeat(40)});
  await assert.rejects(publish({...other,createTag:true}),/draft belongs to another source commit/);
  assert.ok(other.events.every(x=>x.method==='GET'));
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
