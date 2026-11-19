// Explicit maintainer inputs only. No PATH discovery, engine execution,
// network, model loading, package manager or application installation.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import zlib from 'node:zlib';
import {Transform, Writable} from 'node:stream';
import {pipeline} from 'node:stream/promises';
import {isDeepStrictEqual} from 'node:util';
import {build, validateManifest} from '../build-stream-capability-pack.mjs';
import {validateCatalog} from './capability-publication.mjs';

export const mediaSpecs = Object.freeze({
  whisper: {version:'1.9.5', name:'whisper.cpp', features:['transcribe'], model:{
    path:'models/ggml-base-q5_1.bin', size:59707625,
    sha256:'422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898',
    source:'https://huggingface.co/ggerganov/whisper.cpp/tree/5359861c739e955e79d9a303bcbc70fb988958b1',
  }},
  'media-ffmpeg': {version:'9.0.2', name:'FFmpeg', features:['media_decode']},
});
const platforms = new Set(['macos-arm64','macos-x64','linux-arm64','linux-x64','windows-arm64','windows-x64']);
const MAX_JSON=1024*1024, MAX_FILE=256*1048576, BUFFER=65536;
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const digest = value => typeof value==='string' && /^[a-f0-9]{64}$/.test(value);
const text = value => typeof value==='string' && value.trim().length>0 && value.length<=4096;
const safePath = value => typeof value==='string' && value.length<=240 && value.split('/').every(p=>/^[A-Za-z0-9_.-]{1,128}$/.test(p)&&!['.','..'].includes(p));
const binaries = (id,platform) => (id==='whisper'?['whisper-cli']:['ffmpeg','ffprobe']).map(name=>'bin/'+name+(platform.startsWith('windows-')?'.exe':''));
function realDirectory(value) {
  const file=path.resolve(value), stat=fs.lstatSync(file);
  if(!stat.isDirectory()||stat.isSymbolicLink()||fs.realpathSync(file)!==file)throw Error('Real directory required');
  return file;
}
function realFile(file,maximum=MAX_FILE) {
  const stat=fs.lstatSync(file);
  if(!stat.isFile()||stat.isSymbolicLink()||!stat.size||stat.size>maximum||fs.realpathSync(file)!==file)throw Error('Regular, unredirected bounded input required');
  return stat;
}
function readJSON(file) {
  realFile(path.resolve(file),MAX_JSON);
  const bytes=fs.readFileSync(file), value=JSON.parse(bytes);
  return {bytes,value};
}
async function fileHash(file) {
  const hash=crypto.createHash('sha256');
  for await(const chunk of fs.createReadStream(file,{highWaterMark:BUFFER}))hash.update(chunk);
  return hash.digest('hex');
}
function sourceURL(value) {
  try {const u=new URL(value);return u.protocol==='https:'&&!u.username&&!u.password&&!u.hash&&!u.search;}catch{return false;}
}
export function validateMediaRecipe(recipe) {
  const spec=mediaSpecs[recipe?.id], review=recipe?.review;
  if(recipe?.schema!==1||!spec||recipe.version!==spec.version||!platforms.has(recipe.platform)||!review||Array.isArray(review)||Object.keys(recipe).some(k=>!['schema','id','version','platform','files','review'].includes(k)))throw Error('Invalid reviewed media input lock');
  if(!text(review.license)||!text(review.minimumPlatform)||review.portable!==true||typeof review.redistributionApproved!=='boolean'||typeof review.sourceDeliveryReviewed!=='boolean'||!Array.isArray(review.sources)||!review.sources.length||review.sources.length>16||review.sources.some(s=>!text(s.component)||!sourceURL(s.url)||!digest(s.sha256)||Object.keys(s).some(k=>!['component','url','sha256'].includes(k)))||!Array.isArray(review.buildConfiguration)||!review.buildConfiguration.length||review.buildConfiguration.length>64||!review.buildConfiguration.every(text)||Object.keys(review).some(k=>!['license','minimumPlatform','portable','redistributionApproved','sourceDeliveryReviewed','sources','buildConfiguration'].includes(k)))throw Error('Source, build configuration, license and redistribution review required');
  if(/(?:\/Users\/|\/home\/|[A-Za-z]:\\Users\\)/i.test(JSON.stringify(recipe)))throw Error('Private host paths must not enter a media input lock');
  const files=recipe.files;
  if(!Array.isArray(files)||!files.length||files.length>32)throw Error('Invalid media inventory');
  const projected={schema:2,id:recipe.id,version:recipe.version,platform:recipe.platform,files:files.map(f=>({path:f.path,size:f.size,sha256:f.sha256,executable:f.executable})),review:{}};
  validateManifest(projected,true);
  const expected=binaries(recipe.id,recipe.platform);
  for(const f of files) {
    if(!safePath(f.input)||!Number.isSafeInteger(f.size)||f.size<=0||typeof f.executable!=='boolean'||Object.keys(f).some(k=>!['path','input','size','sha256','executable'].includes(k)))throw Error('Invalid media input member');
    if(expected.includes(f.path)){if(!f.executable)throw Error('Media binary must be executable');}
    else if(f.executable||!(['LICENSE','NOTICE','SOURCE'].includes(f.path)||f.path.startsWith('licenses/')||(spec.model&&f.path===spec.model.path)))throw Error('Unsupported media payload or loader override');
  }
  for(const name of [...expected,'LICENSE','SOURCE',...(spec.model?['licenses/WHISPER-MODEL.txt']:[])])if(!files.some(f=>f.path===name))throw Error('Missing media binary, source provision or license notice: '+name);
  if(spec.model&&!files.some(f=>f.path===spec.model.path&&f.size===spec.model.size&&f.sha256===spec.model.sha256))throw Error('Whisper model differs from the application-pinned upstream object');
  return recipe;
}

// Inspect loader metadata, without launching the supplied binary. The current
// fixed runtime inventory has no room for non-system shared libraries.
export function inspectMediaExecutable(file,platform) {
  const fd=fs.openSync(file,'r');
  function read(size,offset){const b=Buffer.alloc(size);if(fs.readSync(fd,b,0,size,offset)!==size)throw Error('Truncated media executable');return b;}
  try {
    const h=read(64,0), arm=platform.endsWith('-arm64');
    if(platform.startsWith('macos-')) {
      if(h.readUInt32LE(0)!==0xfeedfacf||h.readUInt32LE(4)!==(arm?0x0100000c:0x01000007)||h.readUInt32LE(12)!==2)throw Error('Media Mach-O architecture differs from target');
      const count=h.readUInt32LE(16),length=h.readUInt32LE(20);
      if(count>4096||length>16*1048576)throw Error('Mach-O command budget');
      const commands=read(length,32);let offset=0;
      for(let i=0;i<count;i++) {
        if(offset+8>length)throw Error('Truncated Mach-O command');
        const cmd=commands.readUInt32LE(offset),size=commands.readUInt32LE(offset+4);
        if(size<8||offset+size>length)throw Error('Invalid Mach-O command');
        if(cmd===0x27)throw Error('Media binary contains a loader environment override');
        if([0xc,0x80000018,0x8000001f,0x80000023,0x20].includes(cmd)) {
          if(size<24)throw Error('Invalid Mach-O dependency');
          const start=commands.readUInt32LE(offset+8),end=commands.indexOf(0,offset+start);
          if(start<24||start>=size||end<0||end>=offset+size)throw Error('Invalid Mach-O dependency name');
          const name=commands.toString('utf8',offset+start,end);
          if(path.posix.normalize(name)!==name||(!name.startsWith('/usr/lib/')&&!name.startsWith('/System/Library/')))throw Error('Media binary requires a non-system shared library');
        }
        offset+=size;
      }
      if(offset!==length)throw Error('Mach-O command length mismatch');
    } else if(platform.startsWith('linux-')) {
      if(h.subarray(0,4).toString('hex')!=='7f454c46'||h[4]!==2||h[5]!==1||h.readUInt16LE(18)!==(arm?183:62))throw Error('Media ELF architecture differs from target');
      const offset=Number(h.readBigUInt64LE(32)),stride=h.readUInt16LE(54),count=h.readUInt16LE(56);
      if(!Number.isSafeInteger(offset)||offset>16*1048576||stride<56||stride>256||count>4096)throw Error('ELF program-header budget');
      for(let i=0;i<count;i++) {
        const p=read(56,offset+i*stride),kind=p.readUInt32LE(0);
        if(kind===3)throw Error('Linux media inputs must be static (no external interpreter)');
        if(kind===2) {
          const start=Number(p.readBigUInt64LE(8)),size=Number(p.readBigUInt64LE(32));
          if(!Number.isSafeInteger(start)||size>1048576)throw Error('ELF dynamic metadata budget');
          const table=read(size,start);
          for(let at=0;at+16<=size;at+=16){const tag=table.readBigUInt64LE(at);if(tag===0n)break;if(tag===1n)throw Error('Media binary requires a non-system shared library');}
        }
      }
    } else {
      const offset=h.readUInt32LE(60);
      if(h.readUInt16LE(0)!==0x5a4d||offset>1048576)throw Error('Invalid media PE header');
      const pe=read(24,offset);
      if(pe.readUInt32LE(0)!==0x4550||pe.readUInt16LE(4)!==(arm?0xaa64:0x8664))throw Error('Media PE architecture differs from target');
      const count=pe.readUInt16LE(6),optionalSize=pe.readUInt16LE(20),optional=read(optionalSize,offset+24);
      if(count>96||optionalSize<240||optional.readUInt16LE(0)!==0x20b)throw Error('Invalid media PE optional header');
      const sections=read(count*40,offset+24+optionalSize);
      function rva(value){for(let at=0;at<sections.length;at+=40){const start=sections.readUInt32LE(at+12),size=sections.readUInt32LE(at+16);if(value>=start&&value-start<size)return sections.readUInt32LE(at+20)+value-start;}throw Error('Invalid media PE import address');}
      const system=/^(?:api-ms-win-[a-z0-9-]+|ext-ms-win-[a-z0-9-]+|kernel32|kernelbase|ntdll|msvcrt|ucrtbase|advapi32|bcrypt|crypt32|ws2_32|user32|shell32|ole32|oleaut32|secur32|gdi32|winmm|version|shlwapi|comdlg32|psapi|setupapi|normaliz)\.dll$/i;
      for(const [index,stride,nameOffset] of [[1,20,12],[13,32,4]]) {
        const table=optional.readUInt32LE(112+index*8);if(!table)continue;
        let ended=false;
        for(let i=0;i<512;i++) {
          const entry=read(stride,rva(table+i*stride));if(entry.every(v=>v===0)){ended=true;break;}
          if(index===13&&entry.readUInt32LE(0)!==1)throw Error('Unsupported PE delay import');
          const name=read(256,rva(entry.readUInt32LE(nameOffset))),end=name.indexOf(0);
          if(end<1||!system.test(name.toString('ascii',0,end)))throw Error('Media binary requires a non-system shared library');
        }
        if(!ended)throw Error('Media PE import budget');
      }
    }
  } finally {fs.closeSync(fd);}
}

async function checkInputs(recipe,inputs) {
  for(const entry of recipe.files) {
    const file=path.join(inputs,...entry.input.split('/')),before=realFile(file);
    if(before.size!==entry.size||await fileHash(file)!==entry.sha256)throw Error('Media input digest mismatch: '+entry.input);
    if(entry.executable)inspectMediaExecutable(file,recipe.platform);
  }
}
export async function prepareMedia({lock,inputs,output}) {
  const {bytes:lockBytes,value:recipe}=readJSON(lock);validateMediaRecipe(recipe);inputs=realDirectory(inputs);
  await checkInputs(recipe,inputs);
  output=path.resolve(output);realDirectory(path.dirname(output));
  if(fs.existsSync(output))throw Error('Preparation output already exists; preserve immutable inputs and packs');
  if(output===inputs||output.startsWith(inputs+path.sep))throw Error('Preparation output must be outside the input directory');
  // All source hashes and loader metadata pass before output creation.
  fs.mkdirSync(output,{mode:0o700});const runtime=path.join(output,'runtime');fs.mkdirSync(runtime);
  for(const entry of recipe.files) {
    const source=path.join(inputs,...entry.input.split('/')),target=path.join(runtime,...entry.path.split('/'));
    const before=realFile(source);fs.mkdirSync(path.dirname(target),{recursive:true});
    const hash=crypto.createHash('sha256');let size=0;
    const check=new Transform({transform(chunk,_encoding,next){size+=chunk.length;hash.update(chunk);next(size>entry.size?Error('Media input grew during copying'):null,chunk);}});
    await pipeline(fs.createReadStream(source,{highWaterMark:BUFFER}),check,fs.createWriteStream(target,{flags:'wx',mode:entry.executable?0o700:0o600}));
    const after=realFile(source);
    if(size!==entry.size||hash.digest('hex')!==entry.sha256||before.ino!==after.ino||before.dev!==after.dev||before.size!==after.size)throw Error('Media input changed during preparation');
  }
  if(!fs.readFileSync(lock).equals(lockBytes))throw Error('Media input lock changed during preparation');
  const compression={node:process.versions.node,zlib:process.versions.zlib,gzipOS:255},inputLockSha256=sha(lockBytes);
  const manifest={schema:2,id:recipe.id,version:recipe.version,platform:recipe.platform,files:recipe.files.map(({path,size,sha256,executable})=>({path,size,sha256,executable})),review:{...recipe.review,inputLockSha256,compression}};
  const manifestFile=`${recipe.id}-${recipe.version}-${recipe.platform}.lock.json`,packFile=`readmd-${recipe.id}-${recipe.version}-${recipe.platform}.readmd-pack`;
  const manifestPath=path.join(output,manifestFile);fs.writeFileSync(manifestPath,JSON.stringify(manifest),{flag:'wx'});
  const result=await build({manifestPath,runtime,output:path.join(output,packFile),writeLock:true,gzipOS:255});
  fs.writeFileSync(path.join(output,'inputs.lock.json'),lockBytes,{flag:'wx'});
  const receipt={...result,version:recipe.version,platform:recipe.platform,inputLockSha256,manifestFile,packFile,manifestSha256:sha(Buffer.from(JSON.stringify(JSON.parse(fs.readFileSync(manifestPath))))),compression,nativeOperationVerified:false,redistributionApproved:recipe.review.redistributionApproved,sourceDeliveryReviewed:recipe.review.sourceDeliveryReviewed};
  fs.writeFileSync(path.join(output,'prepared.json'),JSON.stringify(receipt,null,2)+'\n',{flag:'wx'});
  return receipt;
}

async function verifyStream(file,manifest,receipt) {
  const before=realFile(file,2*1073741824);
  if(before.size!==receipt.bytes||await fileHash(file)!==receipt.sha256)throw Error('Prepared media package differs from receipt');
  const fd=fs.openSync(file,'r');let start;
  try {
    const head=Buffer.alloc(12);
    if(fs.readSync(fd,head,0,12,0)!==12||head.subarray(0,8).toString()!=='RMDPKG2\n')throw Error('Invalid media stream header');
    const length=head.readUInt32LE(8);if(!length||length>8*1048576)throw Error('Media stream manifest budget');
    const bytes=Buffer.alloc(length);
    if(fs.readSync(fd,bytes,0,length,12)!==length||!bytes.equals(Buffer.from(JSON.stringify(manifest))))throw Error('Media stream header differs from application manifest');
    start=12+length;
  } finally {fs.closeSync(fd);}
  for(const entry of manifest.files) {
    if(start+entry.compressedSize>before.size)throw Error('Truncated media stream payload');
    const hash=crypto.createHash('sha256');let size=0;
    const sink=new Writable({write(chunk,_encoding,next){size+=chunk.length;if(size>entry.size){next(Error('Media payload expansion budget'));return;}hash.update(chunk);next();}});
    await pipeline(fs.createReadStream(file,{start,end:start+entry.compressedSize-1,highWaterMark:BUFFER}),zlib.createGunzip(),sink);
    if(size!==entry.size||hash.digest('hex')!==entry.sha256)throw Error('Media payload differs from file lock');
    start+=entry.compressedSize;
  }
  const after=realFile(file,2*1073741824);
  if(start!==before.size||before.dev!==after.dev||before.ino!==after.ino||after.size!==before.size||await fileHash(file)!==receipt.sha256)throw Error('Media package changed or has trailing content');
}
export async function inspectPreparedMedia(directory,{lock}={}) {
  directory=realDirectory(directory);
  const {bytes:inputBytes,value:recipe}=readJSON(path.join(directory,'inputs.lock.json'));validateMediaRecipe(recipe);
  if(lock&&!fs.readFileSync(lock).equals(inputBytes))throw Error('Prepared media input lock differs from explicitly trusted lock');
  const {value:receipt}=readJSON(path.join(directory,'prepared.json'));
  const manifestFile=`${recipe.id}-${recipe.version}-${recipe.platform}.lock.json`,packFile=`readmd-${recipe.id}-${recipe.version}-${recipe.platform}.readmd-pack`;
  if(receipt.manifestFile!==manifestFile||receipt.packFile!==packFile||receipt.id!==recipe.id||receipt.version!==recipe.version||receipt.platform!==recipe.platform||receipt.inputLockSha256!==sha(inputBytes)||receipt.format!=='stream-v2')throw Error('Prepared media identity differs from input lock');
  const {value:manifest}=readJSON(path.join(directory,manifestFile));validateManifest(manifest);
  const compression=receipt.compression;
  if(!compression||!text(compression.node)||!text(compression.zlib)||compression.gzipOS!==255||Object.keys(compression).some(k=>!['node','zlib','gzipOS'].includes(k)))throw Error('Invalid media compression receipt');
  const expected={schema:2,id:recipe.id,version:recipe.version,platform:recipe.platform,files:recipe.files.map(({path,size,sha256,executable},i)=>({path,size,sha256,executable,compressedSize:manifest.files[i]?.compressedSize})),review:{...recipe.review,inputLockSha256:sha(inputBytes),compression}};
  if(!isDeepStrictEqual(manifest,expected)||receipt.manifestSha256!==sha(Buffer.from(JSON.stringify(manifest))))throw Error('Media manifest differs from reviewed inputs');
  const file=path.join(directory,packFile);await verifyStream(file,manifest,receipt);
  const spec=mediaSpecs[recipe.id],labels=recipe.id==='whisper'
    ?{labelKey:'plugin.whisper.name',descriptionKey:'plugin.whisper.desc'}
    :{name:spec.name,description:'WAV / MP3 / AAC / M4A / MP4 / FLAC / Ogg / WebM / MKV / WMA / MOV / AVI'};
  const entry={id:recipe.id,version:recipe.version,platform:recipe.platform,...labels,features:spec.features,delivery:'offline-only',format:'stream-v2',downloadBytes:receipt.bytes,installedBytes:manifest.files.reduce((n,f)=>n+f.size,0),sha256:receipt.sha256,manifestSha256:receipt.manifestSha256,license:recipe.review.license,minimumPlatform:recipe.review.minimumPlatform};
  if(entry.installedBytes!==receipt.installedBytes)throw Error('Media installed size differs from receipt');
  return {file,manifest,entry,recipe};
}

// Registration is local source editing, never installation or publication.
// The receipt must come from a reviewed native run on this exact pack.
export async function registerPreparedMedia({directory,lock,assets,nativeReceipt}) {
  if(!lock||!nativeReceipt)throw Error('Explicit trusted input lock and native receipt required for registration');
  const row=await inspectPreparedMedia(directory,{lock}),{recipe,entry,manifest}=row;
  if(!recipe.review.redistributionApproved||!recipe.review.sourceDeliveryReviewed)throw Error('Media redistribution and corresponding source delivery are not approved');
  const {value:native}=readJSON(nativeReceipt);
  if(native.schema!==1||native.id!==entry.id||native.version!==entry.version||native.platform!==entry.platform||native.sha256!==entry.sha256||native.manifestSha256!==entry.manifestSha256||native.install!==true||native.engineOperation!==true||native.uninstall!==true||native.synthetic!==false||!safePath(native.evidenceFile)||!digest(native.evidenceSha256))throw Error('Native media evidence must bind this exact platform, archive and manifest');
  const evidence=path.resolve(path.dirname(nativeReceipt),...native.evidenceFile.split('/'));
  realFile(evidence,8*1048576);
  if(await fileHash(evidence)!==native.evidenceSha256)throw Error('Native media evidence bytes differ from the reviewed receipt');
  assets=realDirectory(assets);const caps=realDirectory(path.join(assets,'capabilities')),catalogFile=path.join(caps,'catalog.json');
  const {bytes:before,value:catalog}=readJSON(catalogFile);validateCatalog(catalog);
  const old=[...catalog.packs,...(catalog.offlinePacks||[])].find(p=>p.id===entry.id&&p.platform===entry.platform);
  if(old&&!isDeepStrictEqual(old,entry))throw Error('Existing media catalog entry is immutable; review a new runtime version');
  const target=path.join(caps,`${entry.id}-${entry.version}-${entry.platform}.lock.json`);
  if(fs.existsSync(target)&&!isDeepStrictEqual(readJSON(target).value,manifest))throw Error('Existing media lock differs; old application authority is preserved');
  if(!old)catalog.offlinePacks=[...(catalog.offlinePacks||[]),entry];
  // Pending is global, so retain it until the platform matrix is complete.
  validateCatalog(catalog);
  if(!fs.readFileSync(catalogFile).equals(before))throw Error('Catalog changed during explicit media registration');
  const temporary=path.join(caps,`media-register-${crypto.randomUUID()}.partial`);
  try {
    fs.writeFileSync(temporary,JSON.stringify(catalog,null,2)+'\n',{flag:'wx'});
    if(!fs.existsSync(target))fs.writeFileSync(target,JSON.stringify(manifest,null,2)+'\n',{flag:'wx'});
    if(!fs.readFileSync(catalogFile).equals(before))throw Error('Catalog changed during explicit media registration');
    fs.renameSync(temporary,catalogFile);
  } finally {if(fs.existsSync(temporary))fs.unlinkSync(temporary);}
  return {registered:true,id:entry.id,platform:entry.platform,published:false,installed:false};
}
