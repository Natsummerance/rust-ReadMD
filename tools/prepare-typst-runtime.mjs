// Explicit offline maintainer assembly. No download, installation, engine
// execution or publication; the application owns its real install/probe path.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {spawn} from 'node:child_process';
import {Transform} from 'node:stream';
import {pipeline} from 'node:stream/promises';
import {isDeepStrictEqual} from 'node:util';
import {fileURLToPath} from 'node:url';
import {build, validateManifest} from './build-stream-capability-pack.mjs';
import {validateCatalog} from './lib/capability-publication.mjs';

export const input = JSON.parse(fs.readFileSync(new URL('./typst-runtime-inputs.lock.json',import.meta.url)));
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
async function hash(file) {
  const digest=crypto.createHash('sha256');
  for await(const bytes of fs.createReadStream(file,{highWaterMark:65536}))digest.update(bytes);
  return digest.digest('hex');
}
function regular(file) {
  const stat=fs.lstatSync(file);
  if(!stat.isFile()||stat.isSymbolicLink()||fs.realpathSync(file)!==file)throw Error('Explicit regular input required');
  return stat;
}
async function extract(archive,entry,target) {
  // Only the fixed member's stdout is written. Archive paths cannot select a
  // destination, create links, overwrite files or invoke a package script.
  const child=spawn('/usr/bin/tar',['-xOf',archive,input.prefix+'/'+entry.member],{stdio:['ignore','pipe','pipe']});
  const closed=new Promise((resolve,reject)=>{child.once('error',reject);child.once('close',(code,signal)=>resolve({code,signal}));});
  closed.catch(()=>{});let size=0,diagnostics=0,timedOut=false;
  child.stderr.on('data',bytes=>{diagnostics+=bytes.length;});
  const timer=setTimeout(()=>{timedOut=true;child.kill();},30000);
  try {
    const bounded=new Transform({transform(bytes,_encoding,next){size+=bytes.length;next(size>entry.maximum?Error('Typst member budget'):null,bytes);}});
    await pipeline(child.stdout,bounded,fs.createWriteStream(target,{flags:'wx',mode:0o600}));
    const result=await closed;
    if(result.code!==0||result.signal||timedOut||!size||diagnostics>65536)throw Error('Locked Typst member extraction failed');
  }catch(error){child.kill();await closed.catch(()=>{});throw error;}
  finally{clearTimeout(timer);}
}
function inspectMachO(file) {
  const fd=fs.openSync(file,'r');
  const read=(size,offset)=>{const b=Buffer.alloc(size);if(fs.readSync(fd,b,0,size,offset)!==size)throw Error('Truncated Typst Mach-O');return b;};
  try {
    const head=read(32,0);
    if(head.readUInt32LE(0)!==0xfeedfacf||head.readUInt32LE(4)!==0x0100000c||head.readUInt32LE(12)!==2)throw Error('Typst must be the reviewed arm64 Mach-O executable');
    const count=head.readUInt32LE(16),length=head.readUInt32LE(20);
    if(count>4096||length>1048576)throw Error('Typst loader budget');
    const commands=read(length,32);let offset=0,minimum;
    for(let i=0;i<count;i++){
      if(offset+8>length)throw Error('Truncated Typst load command');
      const cmd=commands.readUInt32LE(offset),size=commands.readUInt32LE(offset+4);
      if(size<8||offset+size>length)throw Error('Invalid Typst load command');
      if(cmd===0x32){if(size<24||commands.readUInt32LE(offset+8)!==1)throw Error('Invalid Typst macOS build target');const v=commands.readUInt32LE(offset+12);minimum=`macOS ${v>>>16}.${(v>>>8)&255}.${v&255}`;}
      if(cmd===0x27)throw Error('Unexpected Typst loader environment');
      if([0xc,0x80000018,0x8000001f,0x80000023,0x20].includes(cmd)){
        const at=commands.readUInt32LE(offset+8),end=commands.indexOf(0,offset+at);
        if(size<24||at<24||at>=size||end<0||end>=offset+size)throw Error('Invalid Typst dependency');
        const name=commands.toString('utf8',offset+at,end);
        if(!name.startsWith('/usr/lib/')&&!name.startsWith('/System/Library/'))throw Error('Typst depends on an unpackaged library');
      }
      offset+=size;
    }
    if(offset!==length||!minimum)throw Error('Missing Typst platform requirement');
    return minimum;
  }finally{fs.closeSync(fd);}
}

export async function prepare({archive,output}) {
  archive=path.resolve(archive);const before=regular(archive);
  if(before.size!==input.archiveBytes||await hash(archive)!==input.archiveSha256)throw Error('Typst archive differs from official input lock');
  output=path.resolve(output);if(fs.existsSync(output))throw Error('Preparation output already exists');
  fs.mkdirSync(output,{recursive:true,mode:0o700});const runtime=path.join(output,'runtime');
  fs.mkdirSync(path.join(runtime,'bin'),{recursive:true,mode:0o700});
  try {
    for(const entry of input.files)await extract(archive,entry,path.join(runtime,entry.path));
    const after=regular(archive);
    if(after.dev!==before.dev||after.ino!==before.ino||after.size!==before.size||await hash(archive)!==input.archiveSha256)throw Error('Typst archive changed during extraction');
    const minimumPlatform=inspectMachO(path.join(runtime,'bin/typst'));
    if(!fs.readFileSync(path.join(runtime,'LICENSE'),'utf8').includes('Apache License')||!fs.readFileSync(path.join(runtime,'NOTICE'),'utf8').trim())throw Error('Original Typst license and notices required');
    fs.chmodSync(path.join(runtime,'bin/typst'),0o700);
    const files=[];
    for(const entry of input.files)files.push({path:entry.path,size:fs.statSync(path.join(runtime,entry.path)).size,sha256:await hash(path.join(runtime,entry.path)),executable:entry.executable});
    const compression={node:process.versions.node,zlib:process.versions.zlib,gzipOS:255};
    const manifest={schema:2,id:'typst',version:input.version,platform:input.platform,files,review:{source:input.source,archive:input.archive,archiveSha256:input.archiveSha256,license:input.license,minimumPlatform,unchangedOfficialBinary:true,execution:'Optional offline worker; generated quoted sources, owned root and bounded cancellable process; no package imports',compression}};
    const manifestFile=`typst-${input.version}-${input.platform}.lock.json`,manifestPath=path.join(output,manifestFile);
    fs.writeFileSync(manifestPath,JSON.stringify(manifest),{flag:'wx'});
    const packFile=`readmd-typst-${input.version}-${input.platform}.readmd-pack`;
    const result=await build({manifestPath,runtime,output:path.join(output,packFile),writeLock:true,gzipOS:255});
    const receipt={...result,version:input.version,platform:input.platform,archiveSha256:input.archiveSha256,manifestFile,packFile,manifestSha256:sha(fs.readFileSync(manifestPath)),minimumPlatform,compression,nativeOperationVerified:false};
    fs.writeFileSync(path.join(output,'prepared.json'),JSON.stringify(receipt,null,2)+'\n',{flag:'wx'});
    return receipt;
  }finally{fs.rmSync(runtime,{recursive:true,force:true});}
}

export async function register({prepared,assets}) {
  prepared=path.resolve(prepared);assets=path.resolve(assets);
  const receipt=JSON.parse(fs.readFileSync(path.join(prepared,'prepared.json')));
  const manifest=JSON.parse(fs.readFileSync(path.join(prepared,`typst-${input.version}-${input.platform}.lock.json`)));
  validateManifest(manifest);
  const pack=path.join(prepared,`readmd-typst-${input.version}-${input.platform}.readmd-pack`),stat=regular(pack);
  if(manifest.id!=='typst'||manifest.version!==input.version||manifest.platform!==input.platform||manifest.review.archiveSha256!==input.archiveSha256||receipt.archiveSha256!==input.archiveSha256||receipt.bytes!==stat.size||await hash(pack)!==receipt.sha256||sha(Buffer.from(JSON.stringify(manifest)))!==receipt.manifestSha256)throw Error('Prepared Typst bytes differ from receipt');
  const fd=fs.openSync(pack,'r');
  try {
    const prefix=Buffer.alloc(12);if(fs.readSync(fd,prefix,0,12,0)!==12||prefix.subarray(0,8).toString()!=='RMDPKG2\n')throw Error('Invalid Typst stream header');
    const size=prefix.readUInt32LE(8);if(size<1||size>8*1048576)throw Error('Typst stream manifest budget');
    const bytes=Buffer.alloc(size);if(fs.readSync(fd,bytes,0,size,12)!==size||!isDeepStrictEqual(JSON.parse(bytes),manifest))throw Error('Typst stream manifest differs from lock');
  }finally{fs.closeSync(fd);}
  const entry={id:'typst',version:input.version,platform:input.platform,labelKey:'capability.typst.name',descriptionKey:'capability.typst.purpose',features:['academic_pdf'],delivery:'offline-only',format:'stream-v2',downloadBytes:receipt.bytes,installedBytes:manifest.files.reduce((n,f)=>n+f.size,0),sha256:receipt.sha256,manifestSha256:receipt.manifestSha256,license:input.license,minimumPlatform:receipt.minimumPlatform};
  const caps=path.join(assets,'capabilities'),file=path.join(caps,'catalog.json');
  // Re-read at the actual commit, retaining other maintainers' platform entries.
  const before=fs.readFileSync(file),catalog=JSON.parse(before);validateCatalog(catalog);
  const old=[...catalog.packs,...(catalog.offlinePacks||[])].find(p=>p.id===entry.id&&p.platform===entry.platform);
  if(old&&!isDeepStrictEqual(old,entry))throw Error('Existing Typst catalog identity is immutable');
  if(!old)catalog.offlinePacks=[...(catalog.offlinePacks||[]),entry];
  validateCatalog(catalog);
  const target=path.join(caps,receipt.manifestFile);
  if(fs.existsSync(target)&&!isDeepStrictEqual(JSON.parse(fs.readFileSync(target)),manifest))throw Error('Existing Typst lock differs');
  const temporary=file+'.'+crypto.randomUUID()+'.partial';
  try {
    fs.writeFileSync(temporary,JSON.stringify(catalog,null,2)+'\n',{flag:'wx'});
    if(!fs.existsSync(target))fs.writeFileSync(target,JSON.stringify(manifest,null,2)+'\n',{flag:'wx'});
    if(!fs.readFileSync(file).equals(before))throw Error('Catalog changed during Typst registration; retry from the current catalog');
    fs.renameSync(temporary,file);
  }finally{if(fs.existsSync(temporary))fs.unlinkSync(temporary);}
  return {registered:true,id:entry.id,platform:entry.platform,published:false,installed:false};
}

if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  try {
    const [mode,...args]=process.argv.slice(2),values={};
    const names=mode==='build'?['--archive','--output']:mode==='register'?['--prepared','--assets']:[];
    for(let i=0;i<args.length;i+=2){if(!names.includes(args[i])||!args[i+1]||values[args[i]])throw Error('Invalid Typst preparation option');values[args[i]]=args[i+1];}
    if(names.length!==2||!names.every(name=>values[name]))throw Error('Use build --archive OFFICIAL.tar.xz --output NEW_DIR, or register --prepared DIR --assets ASSETS');
    console.log(JSON.stringify(mode==='build'?await prepare({archive:values['--archive'],output:values['--output']}):await register({prepared:values['--prepared'],assets:values['--assets']})));
  }catch(error){console.error(error.message);process.exitCode=1;}
}
