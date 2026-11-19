'use strict';
// Test-only input selection. It does not override runtime discovery or write
// managed engine files: the production authenticated chunk installer does that.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const ROOT=path.resolve(__dirname,'../..'),pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
function nodePack(){
  const file=process.env.READMD_TEST_NODE_PACK;if(!file||!path.isAbsolute(file))throw Error('An explicit READMD_TEST_NODE_PACK is required');
  const metadata=fs.lstatSync(file);assert(metadata.isFile()&&!metadata.isSymbolicLink());
  const fd=fs.openSync(file,'r'),prefix=Buffer.alloc(12);try{assert.equal(fs.readSync(fd,prefix,0,12,0),12);assert.equal(prefix.subarray(0,8).toString(),'RMDPKG2\n');const length=prefix.readUInt32LE(8);assert(length>0&&length<=8*1048576);const bytes=Buffer.alloc(length);assert.equal(fs.readSync(fd,bytes,0,length,12),length);const manifest=JSON.parse(bytes);assert.equal(manifest.id,'node');
    const lock=JSON.parse(fs.readFileSync(path.join(ROOT,'assets/capabilities',`node-${manifest.version}-${manifest.platform}.lock.json`)));
    assert.deepEqual(manifest,lock);const catalog=JSON.parse(fs.readFileSync(path.join(ROOT,'assets/capabilities/catalog.json'))),entry=[...catalog.packs,...(catalog.offlinePacks||[])].find(p=>p.id==='node'&&p.platform===manifest.platform);
    assert(entry);assert.equal(entry.downloadBytes,metadata.size);assert.equal(entry.manifestSha256,sha(Buffer.from(JSON.stringify(lock))));
    // This is a bounded test input (<128 MiB), never an application loader.
    assert.equal(sha(fs.readFileSync(file)),entry.sha256);
    return {file,manifest,entry,size:metadata.size,sha256:entry.sha256,manifestSha256:entry.manifestSha256};
  }finally{fs.closeSync(fd);}
}
async function api(host,route,body){const response=await host.api(route,body===undefined?{}:{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});return{status:response.status,body:await response.json()};}
async function settle(host,job){
  const end=Date.now()+60000;let value=job;
  while(!['ready','removed','error','cancelled'].includes(value.phase)&&Date.now()<end){await pause(25);const next=await api(host,'/api/capabilities/packs',{action:'download_status',job:value.job});assert.equal(next.status,200,next.body.error_code);value=next.body;}
  assert(['ready','removed','error','cancelled'].includes(value.phase),'bounded capability operation');return value;
}
async function installNodePack(host,proof=nodePack()){
  const catalog=await api(host,'/api/capabilities/packs',{action:'catalog'}),entry=catalog.body.packs.find(p=>p.id==='node');
  assert.equal(catalog.status,200);assert.equal(entry?.platform,proof.manifest.platform);assert.equal(entry.sha256,proof.sha256);assert.equal(entry.available,true);
  const start=await api(host,'/api/capabilities/packs',{action:'upload_start',id:'node',format:'stream-v2',total:proof.size,request_key:crypto.randomUUID()});
  assert.equal(start.status,200,start.body.error_code);let current=start.body;const fd=fs.openSync(proof.file,'r');
  try{const buffer=Buffer.alloc(current.chunk_bytes);let offset=current.received;while(offset<proof.size){const count=fs.readSync(fd,buffer,0,Math.min(buffer.length,proof.size-offset),offset);assert(count>0);const bytes=buffer.subarray(0,count);
    const response=await host.api('/api/capabilities/upload/chunk?job='+encodeURIComponent(current.job)+'&offset='+offset,{method:'POST',headers:{'Content-Type':'application/octet-stream','X-ReadMD-Upload-Token':start.body.upload_token,'X-ReadMD-Chunk-SHA256':sha(bytes)},body:bytes}),ack=await response.json();assert.equal(response.status,200,ack.error_code);assert.equal(ack.received,offset+count);offset=ack.received;
  }}finally{fs.closeSync(fd);}
  const finish=await api(host,'/api/capabilities/packs',{action:'upload_finish',job:start.body.job,upload_token:start.body.upload_token});assert.equal(finish.status,200,finish.body.error_code);
  const result=await settle(host,finish.body);assert.equal(result.phase,'ready',result.error_code);return{sha256:proof.sha256,version:proof.manifest.version,platform:proof.manifest.platform,manifestSha256:proof.manifestSha256};
}
module.exports={nodePack,installNodePack,api,settle,sha};
