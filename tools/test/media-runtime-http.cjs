'use strict';
// Opt-in real media engine acceptance. Uses explicit prepared inputs, an
// isolated application asset copy/profile, and generated test audio only.
// Production upload, install probes, inspection, preview and uninstall routes
// perform all managed installation work. No host PATH or model is requested.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {spawnSync}=require('node:child_process');
const {startKernel}=require('./v006-refactor-host.cjs');
const {api,settle,sha}=require('./node-runtime-fixture.cjs');
const ROOT=path.resolve(__dirname,'../..');

function options(args) {
  const values={};
  for(let at=0;at<args.length;at+=2) {
    if(!['--bin','--prepared','--lock','--evidence'].includes(args[at])||!args[at+1]||args[at+1].startsWith('--')||values[args[at]])throw Error('Expected --bin KERNEL --prepared DIRECTORY --lock INPUT_LOCK --evidence NEW_DIRECTORY');
    values[args[at]]=path.resolve(args[at+1]);
  }
  if(Object.keys(values).length!==4)throw Error('An explicit kernel, prepared pack, trusted input lock and new evidence directory are required');
  if(fs.existsSync(values['--evidence']))throw Error('Preserve existing media evidence; choose a new directory');
  return values;
}
function tone(frequency,duration=2) {
  const samples=16000*duration,bytes=Buffer.alloc(44+samples*2);
  bytes.write('RIFF');bytes.writeUInt32LE(bytes.length-8,4);bytes.write('WAVEfmt ',8);bytes.writeUInt32LE(16,16);bytes.writeUInt16LE(1,20);bytes.writeUInt16LE(1,22);
  bytes.writeUInt32LE(16000,24);bytes.writeUInt32LE(32000,28);bytes.writeUInt16LE(2,32);bytes.writeUInt16LE(16,34);bytes.write('data',36);bytes.writeUInt32LE(samples*2,40);
  for(let at=0;at<samples;at++)bytes.writeInt16LE(Math.round(Math.sin(2*Math.PI*frequency*at/16000)*12000),44+at*2);
  return bytes;
}
function pcm(bytes) {
  assert.equal(bytes.subarray(0,4).toString(),'RIFF');assert.equal(bytes.subarray(8,12).toString(),'WAVE');
  let format,data;
  for(let at=12;at+8<=bytes.length;) {
    const size=bytes.readUInt32LE(at+4);assert(at+8+size<=bytes.length);
    if(bytes.toString('ascii',at,at+4)==='fmt ')format=bytes.subarray(at+8,at+8+size);
    if(bytes.toString('ascii',at,at+4)==='data')data=bytes.subarray(at+8,at+8+size);
    at+=8+size+(size%2);
  }
  assert(format&&data);assert.equal(format.readUInt16LE(0),1);assert.equal(format.readUInt16LE(2),1);assert.equal(format.readUInt32LE(4),16000);assert.equal(format.readUInt16LE(14),16);
  assert.equal(data.length%2,0);let crossings=0,nonzero=false;
  for(let at=2;at<data.length;at+=2){const sample=data.readInt16LE(at);if(sample)nonzero=true;if(data.readInt16LE(at-2)<=0&&sample>0)crossings++;}
  return {samples:data.length/2,nonzero,frequency:crossings/(data.length/2/16000)};
}
async function upload(host,proof,{damage=false}={}) {
  const start=await api(host,'/api/capabilities/packs',{action:'upload_start',id:proof.entry.id,format:'stream-v2',total:proof.entry.downloadBytes,request_key:crypto.randomUUID()});
  assert.equal(start.status,200,start.body.error_code);const fd=fs.openSync(proof.file,'r'),buffer=Buffer.alloc(start.body.chunk_bytes);let offset=0;
  try {
    while(offset<proof.entry.downloadBytes) {
      const count=fs.readSync(fd,buffer,0,Math.min(buffer.length,proof.entry.downloadBytes-offset),offset);assert(count>0);
      const bytes=Buffer.from(buffer.subarray(0,count));if(damage&&offset+count===proof.entry.downloadBytes)bytes[bytes.length-1]^=0x40;
      const response=await host.api('/api/capabilities/upload/chunk?job='+encodeURIComponent(start.body.job)+'&offset='+offset,{method:'POST',headers:{'Content-Type':'application/octet-stream','X-ReadMD-Upload-Token':start.body.upload_token,'X-ReadMD-Chunk-SHA256':sha(bytes)},body:bytes});
      const next=await response.json();assert.equal(response.status,200,next.error_code);assert.equal(next.received,offset+count);offset=next.received;
    }
  } finally {fs.closeSync(fd);}
  const finish=await api(host,'/api/capabilities/packs',{action:'upload_finish',job:start.body.job,upload_token:start.body.upload_token});
  if(finish.status!==200)return {status:finish.status,...finish.body};
  return settle(host,finish.body);
}
async function main(args=process.argv.slice(2)) {
  const values=options(args),evidence=values['--evidence'];
  const {inspectPreparedMedia}=await import('../lib/media-runtime.mjs');
  const proof=await inspectPreparedMedia(values['--prepared'],{lock:values['--lock']});
  assert.equal(proof.entry.id,'media-ffmpeg');assert.equal(proof.entry.platform,process.platform==='darwin'&&process.arch==='arm64'?'macos-arm64':'unsupported');
  fs.mkdirSync(evidence,{mode:0o700});
  const report={schema:1,passed:false,platform:proof.entry.platform,id:proof.entry.id,version:proof.entry.version,packSha256:proof.entry.sha256,manifestSha256:proof.entry.manifestSha256,modelsRequested:false,engineSynthetic:false,audioFixtureSynthetic:true,published:false,sourceAssetsChanged:false,checks:[]};
  const assets=path.join(evidence,'assets'),profile=path.join(evidence,'profile'),documents=path.join(evidence,'documents'),tmp=path.join(evidence,'tmp'),poison=path.join(evidence,'empty-path');
  for(const directory of [profile,documents,tmp,poison])fs.mkdirSync(directory);
  fs.cpSync(path.join(ROOT,'assets'),assets,{recursive:true,errorOnExist:true,force:false});
  // QA authority is confined to this copied asset directory; no application
  // registration/redistribution approval is inferred from fixture staging.
  const catalogFile=path.join(assets,'capabilities','catalog.json'),catalog=JSON.parse(fs.readFileSync(catalogFile));
  catalog.offlinePacks=[...(catalog.offlinePacks||[]).filter(row=>!(row.id===proof.entry.id&&row.platform===proof.entry.platform)),proof.entry];
  fs.writeFileSync(catalogFile,JSON.stringify(catalog,null,2)+'\n');
  fs.writeFileSync(path.join(assets,'capabilities',proof.entry.id+'-'+proof.entry.version+'-'+proof.entry.platform+'.lock.json'),JSON.stringify(proof.manifest,null,2)+'\n');
  const binary=path.join(evidence,'readmd');fs.copyFileSync(values['--bin'],binary,fs.constants.COPYFILE_EXCL);fs.chmodSync(binary,0o700);report.binarySha256=sha(fs.readFileSync(binary));
  const one=path.join(documents,'440 中文.wav'),two=path.join(documents,'880 中文.wav'),movie=path.join(documents,'双音轨.m4a');
  fs.writeFileSync(one,tone(440));fs.writeFileSync(two,tone(880));
  let host,installed=false,phase='start',oldTemp={TMPDIR:process.env.TMPDIR,TMP:process.env.TMP,TEMP:process.env.TEMP};
  Object.assign(process.env,{TMPDIR:tmp,TMP:tmp,TEMP:tmp});
  const mark=value=>{phase=value;console.log(JSON.stringify({phase}));};
  async function start() {const previous=process.env.PATH;process.env.PATH=poison;try{host=await startKernel(binary,profile,documents,{assets,requestTimeoutMs:45000});}finally{if(previous===undefined)delete process.env.PATH;else process.env.PATH=previous;}}
  const pack=body=>api(host,'/api/capabilities/packs',body),media=body=>api(host,'/api/transcribe',body);
  async function preview(file,revision,track,from,to,label) {
    const response=await host.api('/api/transcribe',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({path:file,operation:'preview',source_revision:revision,track,start_ms:from,end_ms:to})});
    assert.equal(response.status,200,await (response.status===200?Promise.resolve(''):response.text()));assert(response.headers.get('content-type').startsWith('audio/wav'));
    const bytes=Buffer.from(await response.arrayBuffer()),result=pcm(bytes);assert.equal(result.samples,(to-from)*16);assert.equal(result.nonzero,true);fs.writeFileSync(path.join(evidence,label+'.wav'),bytes,{flag:'wx'});return result;
  }
  try {
    mark('empty profile and damaged archive');await start();
    const unavailable=await media({path:one,operation:'inspect'});assert.equal(unavailable.body.error_code,'ffmpeg_unavailable');assert.equal((await pack({action:'status',id:proof.entry.id})).body.ready,false);
    const entry=(await pack({action:'catalog'})).body.packs.find(row=>row.id===proof.entry.id);assert.equal(entry.delivery,'offline-only');assert.equal(entry.url,undefined);assert.equal((await pack({action:'download',id:proof.entry.id})).body.error_code,'capability_pack_unavailable');
    report.corruptImport=await upload(host,proof,{damage:true});assert(report.corruptImport.status!==200||report.corruptImport.phase==='error');assert.equal((await pack({action:'status',id:proof.entry.id})).body.ready,false);
    report.checks.push('Empty profile and corrupted upload cannot use host PATH or trigger an online download.');
    mark('production chunk upload and native installation probes');report.installReceipt=await upload(host,proof);assert.equal(report.installReceipt.phase,'ready',report.installReceipt.error_code);installed=true;
    report.statusAfterInstall=(await pack({action:'status',id:proof.entry.id})).body;assert.equal(report.statusAfterInstall.ready,true);
    const inspected=await media({path:one,operation:'inspect'});assert.equal(inspected.status,200,inspected.body.error_code);assert.equal(inspected.body.decoder.managed,true);assert.equal(inspected.body.decoder.source,'managed-package');assert.equal(inspected.body.media.tracks[0].codec,'pcm_s16le');
    report.waveInspect=inspected.body;report.wavePreview=await preview(one,inspected.body.source_revision,0,250,750,'wave-preview');assert(Math.abs(report.wavePreview.frequency-440)<5);
    report.checks.push('Real install probe generates, probes and decodes PCM. Authenticated media inspection and a half-second preview use the installed managed decoder.');
    mark('AAC encoding and second-track selection');
    const managed=path.join(profile,'capabilities','managed-v2',proof.entry.id,proof.entry.version),ffmpeg=path.join(managed,'bin','ffmpeg');
    const encoded=spawnSync(ffmpeg,['-nostdin','-v','error','-y','-i',one,'-i',two,'-map','0:a:0','-map','1:a:0','-threads','1','-c:a','aac','-b:a','48k','-metadata:s:a:0','language=eng','-metadata:s:a:1','language=zho','-f','mp4',movie],{cwd:tmp,env:{},encoding:'utf8',timeout:30000,maxBuffer:262144});
    assert.equal(encoded.status,0,encoded.stderr);report.encodedBytes=fs.statSync(movie).size;
    const compressed=await media({path:movie,operation:'inspect'});assert.equal(compressed.status,200,compressed.body.error_code);assert.equal(compressed.body.decoder.managed,true);assert.equal(compressed.body.media.tracks.length,2);assert.equal(compressed.body.media.tracks[1].codec,'aac');
    report.aacInspect=compressed.body;report.aacPreview=await preview(movie,compressed.body.source_revision,compressed.body.media.tracks[1].index,500,1000,'aac-second-track-preview');assert(Math.abs(report.aacPreview.frequency-880)<25);
    const original=fs.readFileSync(movie);fs.appendFileSync(movie,Buffer.from([0]));const stale=await media({path:movie,operation:'preview',source_revision:compressed.body.source_revision,track:0,start_ms:0,end_ms:250});assert.equal(stale.status,409);assert.equal(stale.body.error_code,'media_source_changed');fs.writeFileSync(movie,original);
    report.checks.push('Real AAC/MP4 transcode, two audio tracks, selected second-track PCM frequency/duration, and stale-source refusal passed.');
    mark('fresh process and normal uninstall');await host.stop();host=null;await start();
    const fresh=await media({path:movie,operation:'inspect'});assert.equal(fresh.status,200,fresh.body.error_code);assert.equal(fresh.body.decoder.managed,true);report.freshProcessInspect=fresh.body;
    const note=path.join(managed,'user-note.txt');fs.writeFileSync(note,'owned QA note preserved');const removing=await pack({action:'uninstall_start',id:proof.entry.id});assert.equal(removing.status,200,removing.body.error_code);report.uninstallReceipt=await settle(host,removing.body);assert.equal(report.uninstallReceipt.phase,'removed',report.uninstallReceipt.error_code);installed=false;
    assert.equal(fs.readFileSync(note,'utf8'),'owned QA note preserved');assert.equal(fs.existsSync(ffmpeg),false);assert.equal((await pack({action:'status',id:proof.entry.id})).body.ready,false);assert.equal((await media({path:movie,operation:'inspect'})).body.error_code,'ffmpeg_unavailable');
    report.checks.push('Fresh process uses the durable managed installation; normal uninstall removes pinned binaries, preserves an unknown note and disables media decoding.');
    report.passed=true;
  } catch(error) {report.failure={phase,name:error.name,message:String(error.message).replaceAll(evidence,'<evidence>'),stack:String(error.stack).replaceAll(evidence,'<evidence>').replaceAll(ROOT,'<repo>').slice(0,5000)};process.exitCode=1;}
  finally {
    report.cleanupErrors=[];
    if(installed&&host)try{const removing=await pack({action:'uninstall_start',id:proof.entry.id});report.cleanupUninstall=await settle(host,removing.body);assert.equal(report.cleanupUninstall.phase,'removed');}catch(error){report.cleanupErrors.push(String(error.message));}
    if(host)try{await host.stop();}catch(error){report.cleanupErrors.push(String(error.message));}
    for(const [key,value] of Object.entries(oldTemp))if(value===undefined)delete process.env[key];else process.env[key]=value;
    if(report.cleanupErrors.length){report.passed=false;process.exitCode=1;}
    const bytes=Buffer.from(JSON.stringify(report,null,2)+'\n');fs.writeFileSync(path.join(evidence,'native-report.json'),bytes,{flag:'wx'});
    if(report.passed)fs.writeFileSync(path.join(evidence,'native-receipt.json'),JSON.stringify({schema:1,id:proof.entry.id,version:proof.entry.version,platform:proof.entry.platform,sha256:proof.entry.sha256,manifestSha256:proof.entry.manifestSha256,install:true,engineOperation:true,uninstall:true,synthetic:false,evidenceFile:'native-report.json',evidenceSha256:sha(bytes)},null,2)+'\n',{flag:'wx'});
    console.log(JSON.stringify({passed:report.passed,checks:report.checks,failure:report.failure,modelsRequested:false,published:false}));
  }
  return report;
}
if(require.main===module)main().catch(error=>{console.error('Media acceptance failed: '+error.message);process.exitCode=1;});
module.exports={main,options,tone,pcm,upload};
