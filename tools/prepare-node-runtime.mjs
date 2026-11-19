// Explicit maintainer preparation only. Ordinary builds and the application
// never call this tool. Building is offline; downloading requires its own mode.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {spawn} from 'node:child_process';
import {Transform} from 'node:stream';
import {pipeline} from 'node:stream/promises';
import {fileURLToPath} from 'node:url';
import {build} from './build-stream-capability-pack.mjs';

export const inputsLock = JSON.parse(fs.readFileSync(new URL('./node-runtime-inputs.lock.json', import.meta.url)));
const MAX_INPUT = 128 * 1048576, MAX_BINARY = 256 * 1048576;
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
export function select(platform) {
  const item = inputsLock.items.find(item => item.platform === platform);
  if (!item) throw Error('Unreviewed Node platform');
  return item;
}
async function hash(file) {
  const digest = crypto.createHash('sha256');
  for await (const bytes of fs.createReadStream(file, {highWaterMark:65536})) digest.update(bytes);
  return digest.digest('hex');
}
function directory(value) {
  const absolute = path.resolve(value), metadata = fs.lstatSync(absolute);
  if (!metadata.isDirectory() || metadata.isSymbolicLink() || fs.realpathSync(absolute) !== absolute) throw Error('Real preparation directory required');
  return absolute;
}
export async function downloadInputs({inputs, platforms}) {
  inputs = directory(inputs); const results = [];
  if (platforms.length > 1) {
    let next = 0, failed;
    await Promise.all(Array.from({length:Math.min(3,platforms.length)},async()=>{
      while (!failed && next < platforms.length) {
        const index = next++;
        try {results[index]=(await downloadInputs({inputs,platforms:[platforms[index]]}))[0];}
        catch(error){failed??=error;}
      }
    }));
    if(failed)throw failed;
    return results;
  }
  for (const platform of platforms) {
    const item = select(platform), target = path.join(inputs,item.file);
    if (fs.existsSync(target)) {
      const metadata=fs.lstatSync(target);
      if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size>MAX_INPUT || await hash(target)!==item.sha256) throw Error('Existing Node input differs from lock');
      results.push({platform,sha256:item.sha256,reused:true}); continue;
    }
    const url='https://nodejs.org/download/release/v'+inputsLock.version+'/'+item.file;
    const response=await fetch(url,{signal:AbortSignal.timeout(900000)});
    if (!response.ok || new URL(response.url).hostname!=='nodejs.org') throw Error('Official Node input download failed');
    const temporary=target+'.'+crypto.randomUUID()+'.partial';
    let bytes=0; const digest=crypto.createHash('sha256');
    const check=new Transform({transform(chunk,_encoding,next){bytes+=chunk.length;if(bytes>MAX_INPUT)return next(Error('Node input budget'));digest.update(chunk);next(null,chunk);}});
    try {
      await pipeline(response.body,check,fs.createWriteStream(temporary,{flags:'wx',mode:0o600}));
      if (!bytes || digest.digest('hex')!==item.sha256) throw Error('Node input digest mismatch');
      fs.linkSync(temporary,target); results.push({platform,sha256:item.sha256,bytes,reused:false});
    } finally {if(fs.existsSync(temporary))fs.unlinkSync(temporary);}
  }
  return results;
}
async function extract(archive,member,target,maximum) {
  // Only stdout is extracted. Even an archive entry cannot select a filesystem
  // destination, create a link, run a script, or overwrite an existing object.
  const executable=process.platform==='win32'?path.join(process.env.SystemRoot||'C:\\Windows','System32','tar.exe'):'/usr/bin/tar';
  const child=spawn(executable,['-xOf',archive,member],{stdio:['ignore','pipe','pipe'],windowsHide:true});
  const closed=new Promise((resolve,reject)=>{child.once('error',reject);child.once('close',(code,signal)=>resolve({code,signal}));});
  // Attach before awaiting pipelines: a missing native archive reader must not
  // leave an unhandled rejection or a partially accepted runtime.
  closed.catch(()=>{}); let diagnostics=0,bytes=0,timedOut=false;
  child.stderr.on('data',chunk=>{diagnostics+=chunk.length;});
  const timer=setTimeout(()=>{timedOut=true;child.kill();},30000);
  try {
    const check=new Transform({transform(chunk,_encoding,next){bytes+=chunk.length;next(bytes>maximum?Error('Node member budget'):null,chunk);}});
    await pipeline(child.stdout,check,fs.createWriteStream(target,{flags:'wx',mode:0o600}));
    const exit=await closed;if(timedOut||exit.code!==0||exit.signal||!bytes||diagnostics>65536)throw Error('Locked Node member extraction failed');
  } catch(error) {child.kill();await closed.catch(()=>{});throw error;}
  finally {clearTimeout(timer);}
}
function inspectExecutable(file,platform){
  const fd=fs.openSync(file,'r');try{
    const head=Buffer.alloc(64);if(fs.readSync(fd,head,0,64,0)!==64)throw Error('Truncated Node executable');
    const arm=platform.endsWith('-arm64');
    if(platform.startsWith('macos-')){
      if(head.readUInt32LE(0)!==0xfeedfacf||head.readUInt32LE(4)!==(arm?0x0100000c:0x01000007))throw Error('Node Mach-O architecture differs from input target');
    }else if(platform.startsWith('linux-')){
      if(head.subarray(0,4).toString('hex')!=='7f454c46'||head[4]!==2||head[5]!==1||head.readUInt16LE(18)!==(arm?183:62))throw Error('Node ELF architecture differs from input target');
    }else{
      const offset=head.readUInt32LE(60);if(head.readUInt16LE(0)!==0x5a4d||offset>1048576)throw Error('Invalid Node PE header');
      const pe=Buffer.alloc(24);if(fs.readSync(fd,pe,0,24,offset)!==24||pe.readUInt32LE(0)!==0x4550||pe.readUInt16LE(4)!==(arm?0xaa64:0x8664))throw Error('Node PE architecture differs from input target');
    }
  }finally{fs.closeSync(fd);}
}
export async function prepare({inputs,output,platform}) {
  inputs=directory(inputs);const item=select(platform),archive=path.join(inputs,item.file),metadata=fs.lstatSync(archive);
  if(!metadata.isFile()||metadata.isSymbolicLink()||metadata.size>MAX_INPUT||await hash(archive)!==item.sha256)throw Error('Node input digest mismatch');
  // Host Node distributions can link a different zlib and emit OS-specific
  // gzip bytes. Use the pinned official Node for immutable pack preparation.
  if(process.versions.node!==inputsLock.preparation.node||process.versions.zlib!==inputsLock.preparation.zlib)throw Error('Build with the reviewed official Node '+inputsLock.version+' executable and its bundled zlib');
  output=path.resolve(output);if(fs.existsSync(output))throw Error('Preparation output already exists');
  fs.mkdirSync(output,{mode:0o700});const runtime=path.join(output,'runtime');fs.mkdirSync(runtime);fs.mkdirSync(path.join(runtime,'bin'));fs.mkdirSync(path.join(runtime,'licenses'));
  const binary=platform.startsWith('windows-')?'bin/node.exe':'bin/node';
  await extract(archive,item.prefix+'/'+item.binary,path.join(runtime,binary),MAX_BINARY);
  await extract(archive,item.prefix+'/LICENSE',path.join(runtime,'licenses/NODE-LICENSE.txt'),512*1024);
  inspectExecutable(path.join(runtime,binary),platform);
  const after=fs.lstatSync(archive);if(!after.isFile()||after.isSymbolicLink()||after.dev!==metadata.dev||after.ino!==metadata.ino||after.size!==metadata.size||await hash(archive)!==item.sha256)throw Error('Node input changed during extraction');
  const license=fs.readFileSync(path.join(runtime,'licenses/NODE-LICENSE.txt'),'utf8');
  if(!license.includes('Node.js contributors')||!license.includes('externally maintained libraries')||!license.includes('V8'))throw Error('Complete upstream Node notices required');
  if(process.platform!=='win32')fs.chmodSync(path.join(runtime,binary),0o700);
  fs.writeFileSync(path.join(runtime,'NOTICE'),`ReadMD optional Node.js runtime\nNode.js ${inputsLock.version}; unmodified official executable and complete upstream LICENSE.\nSource: https://nodejs.org/dist/v${inputsLock.version}/${item.file}\nInput SHA-256: ${item.sha256}\nPlatform requirement: ${item.minimum}\nNo npm, package installation scripts or global modules. No system-wide installation.\nNode Permission Model is an additional restriction, not an OS sandbox for malicious code.\n`,{flag:'wx'});
  const files=[];for(const name of [binary,'licenses/NODE-LICENSE.txt','NOTICE'])files.push({path:name,size:fs.statSync(path.join(runtime,name)).size,sha256:await hash(path.join(runtime,name)),executable:name===binary});
  const compression={node:process.versions.node,zlib:process.versions.zlib,gzipOS:255};
  const manifest={schema:2,id:'node',version:inputsLock.version,platform,files,review:{source:inputsLock.source,inputSha256:item.sha256,minimumPlatform:item.minimum,license:'MIT and bundled third-party notices',unchangedOfficialBinary:true,npmIncluded:false,compression}};
  const manifestPath=path.join(output,`node-${inputsLock.version}-${platform}.lock.json`);fs.writeFileSync(manifestPath,JSON.stringify(manifest),{flag:'wx'});
  const pack=path.join(output,`readmd-node-${inputsLock.version}-${platform}.readmd-pack`),result=await build({manifestPath,runtime,output:pack,writeLock:true,gzipOS:255});
  const receipt={...result,platform,inputSha256:item.sha256,binarySha256:files[0].sha256,licenseSha256:files[1].sha256,manifestFile:path.basename(manifestPath),packFile:path.basename(pack),minimumPlatform:item.minimum,compression,nativeOperationVerified:false};
  fs.writeFileSync(path.join(output,'prepared.json'),JSON.stringify(receipt,null,2)+'\n',{flag:'wx'});return receipt;
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  try {
    const [mode,...args]=process.argv.slice(2),values={};for(let i=0;i<args.length;i+=2){if(!['--inputs','--output','--platform'].includes(args[i])||!args[i+1]||values[args[i]])throw Error('Invalid Node preparation option');values[args[i]]=args[i+1];}
    if(!values['--inputs'])throw Error('Explicit input directory required');
    if(mode==='download'){const platforms=values['--platform']?[values['--platform']]:inputsLock.items.map(item=>item.platform);console.log(JSON.stringify(await downloadInputs({inputs:values['--inputs'],platforms})));}
    else if(mode==='build'&&values['--output']&&values['--platform'])console.log(JSON.stringify(await prepare({inputs:values['--inputs'],output:values['--output'],platform:values['--platform']})));
    else throw Error('Use download --inputs DIR [--platform PLATFORM], or build --inputs DIR --output NEW_DIR --platform PLATFORM');
  }catch(error){console.error(error.message);process.exitCode=1;}
}
