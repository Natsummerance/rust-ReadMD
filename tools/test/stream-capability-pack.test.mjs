import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import crypto from 'node:crypto';
import zlib from 'node:zlib';
import {build,validateManifest} from '../build-stream-capability-pack.mjs';
function fixture(t){
  const parent=fs.realpathSync(os.tmpdir()),root=fs.mkdtempSync(path.join(parent,'readmd-stream-builder-'));
  t.after(()=>{assert.equal(path.dirname(fs.realpathSync(root)),parent);fs.rmSync(root,{recursive:true});});
  const runtime=path.join(root,'runtime');fs.mkdirSync(runtime);
  const bytes=Buffer.from('ReadMD synthetic offline fixture.\n'.repeat(3000));fs.writeFileSync(path.join(runtime,'sample.txt'),bytes);
  const manifest={schema:2,id:'fixture',version:'1',platform:'universal',files:[{path:'sample.txt',size:bytes.length,sha256:crypto.createHash('sha256').update(bytes).digest('hex')}],review:{license:'Synthetic test'}};
  const manifestPath=path.join(root,'fixture.lock.json');fs.writeFileSync(manifestPath,JSON.stringify(manifest));
  return {root,runtime,manifestPath,manifest,bytes,output:path.join(root,'fixture.readmd-pack')};
}
test('offline two-pass builder locks gzip lengths and reproduces the same archive',async t=>{
  const f=fixture(t);const r=await build({...f,writeLock:true});assert.equal(r.published,false);assert.equal(r.enabledInCatalog,false);assert.equal(r.installedBytes,f.bytes.length);
  const bytes=fs.readFileSync(f.output);assert.equal(bytes.subarray(0,8).toString(),'RMDPKG2\n');
  const length=bytes.readUInt32LE(8),manifest=JSON.parse(bytes.subarray(12,12+length));assert.deepEqual(manifest,JSON.parse(fs.readFileSync(f.manifestPath)));
  assert.equal(bytes.length-12-length,manifest.files[0].compressedSize);assert.deepEqual(zlib.gunzipSync(bytes.subarray(12+length)),f.bytes);
  const again=await build({...f,output:path.join(f.root,'again.readmd-pack')});assert.equal(again.sha256,r.sha256);assert.equal(again.bytes,r.bytes);
});
test('changed runtime and missing compression review produce no archive',async t=>{
  const f=fixture(t);await assert.rejects(build(f),/compression lock/);assert.equal(fs.existsSync(f.output),false);
  fs.writeFileSync(path.join(f.runtime,'sample.txt'),Buffer.alloc(f.bytes.length));await assert.rejects(build({...f,writeLock:true}),/digest differs/);assert.equal(fs.existsSync(f.output),false);
});
test('existing output is immutable and retained',async t=>{
  const f=fixture(t);fs.writeFileSync(f.output,'existing release');await assert.rejects(build({...f,writeLock:true}),/already exists/);assert.equal(fs.readFileSync(f.output,'utf8'),'existing release');
});
test('unsafe paths, duplicate names and file-parent conflicts are refused',async t=>{
  const f=fixture(t);
  for(const name of ['../outside','a:b','CON.txt','CON .txt','a/COM1 .exe','a"b','/outside','a\\b','end.']){const m=structuredClone(f.manifest);m.files[0].path=name;fs.writeFileSync(f.manifestPath,JSON.stringify(m));await assert.rejects(build({...f,writeLock:true}),/Invalid v2 file entry/);}
  for(const name of ['SAMPLE.TXT','sample.txt/child']){const m=structuredClone(f.manifest);m.files.push({...m.files[0],path:name});fs.writeFileSync(f.manifestPath,JSON.stringify(m));await assert.rejects(build({...f,writeLock:true}),/Invalid v2 file entry|archive parent/);}
  assert.equal(fs.existsSync(f.output),false);
});
test('complete TeX-sized inventory keeps distinct file and header bounds',()=>{
  const manifest={schema:2,id:'fixture',version:'1',platform:'universal',files:Array.from({length:40000},(_,i)=>({path:`texmf/${i}.sty`,size:0,compressedSize:20,sha256:'0'.repeat(64)})),review:{license:'Synthetic metadata only'}};
  assert.doesNotThrow(()=>validateManifest(manifest));
  manifest.files.push({...manifest.files[0],path:'extra.sty'});
  assert.throws(()=>validateManifest(manifest),/Invalid v2 manifest/);
  manifest.files.pop();
  manifest.files.forEach((file,i)=>{file.path='x'.repeat(120)+`/${i}.sty`;});
  assert.throws(()=>validateManifest(manifest),/Manifest budget/);
});
test('concurrent sizing preserves file order and reproducible compressed bytes',async t=>{
  const f=fixture(t);
  f.manifest.files=Array.from({length:8},(_,i)=>{
    const bytes=Buffer.from(('ordered fixture '+i+'\n').repeat(1+i*513)),name=`part-${i}.txt`;
    fs.writeFileSync(path.join(f.runtime,name),bytes);
    return {path:name,size:bytes.length,sha256:crypto.createHash('sha256').update(bytes).digest('hex')};
  });
  fs.writeFileSync(f.manifestPath,JSON.stringify(f.manifest));
  const progress=[],first=await build({...f,writeLock:true,onProgress:p=>progress.push(p)});
  const bytes=fs.readFileSync(f.output),headerSize=bytes.readUInt32LE(8),manifest=JSON.parse(bytes.subarray(12,12+headerSize));
  let offset=12+headerSize;
  for(const entry of manifest.files){assert.deepEqual(zlib.gunzipSync(bytes.subarray(offset,offset+entry.compressedSize)),fs.readFileSync(path.join(f.runtime,entry.path)));offset+=entry.compressedSize;}
  assert.equal(offset,bytes.length);
  assert.deepEqual(progress,[{phase:'sizing',files:8,total:8},{phase:'writing',files:8,total:8}]);
  const second=await build({...f,output:path.join(f.root,'second.readmd-pack')});
  assert.equal(second.sha256,first.sha256);
});
test('a concurrent input failure drains workers before rejecting without publishing',async t=>{
  const f=fixture(t);
  f.manifest.files=Array.from({length:8},(_,i)=>{
    const bytes=Buffer.alloc(128*1024,i),name=`part-${i}.txt`;
    if(i!==2)fs.writeFileSync(path.join(f.runtime,name),bytes);
    return {path:name,size:bytes.length,sha256:crypto.createHash('sha256').update(bytes).digest('hex')};
  });
  fs.writeFileSync(f.manifestPath,JSON.stringify(f.manifest));
  await assert.rejects(build({...f,writeLock:true}),/ENOENT/);
  assert.equal(fs.existsSync(f.output),false);
  assert.deepEqual(fs.readdirSync(f.root).sort(),['fixture.lock.json','runtime']);
  assert.deepEqual(JSON.parse(fs.readFileSync(f.manifestPath)),f.manifest);
});
