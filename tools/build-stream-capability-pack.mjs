// Offline maintainer builder. No engine discovery, download or package script.
// v2 is not enabled by creating an archive: catalog review and probing remain
// the responsibility of the common capability service/publication workflow.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import zlib from 'node:zlib';
import {Transform,Writable} from 'node:stream';
import {pipeline} from 'node:stream/promises';
import {fileURLToPath} from 'node:url';
const BUFFER=65536,MAX_MANIFEST=8*1048576,MAX_FILES=40000,MAX_FILE=256*1048576,MAX_DOWNLOAD=2*1073741824,MAX_EXPANDED=4*1073741824;
const token=v=>typeof v==='string'&&/^[A-Za-z0-9_.-]{1,128}$/.test(v)&&!['.','..'].includes(v);
function safeName(name){return typeof name==='string'&&name.length<=240&&name.length>0&&/^[\x20-\x7e]+$/.test(name)&&name.split('/').every(p=>p&&p.length<=128&&!['.','..'].includes(p)&&!/[ .]$|[<>:"\\|?*]/.test(p)&&!/^(con|prn|aux|nul|conin\$|conout\$|clock\$|com[1-9]|lpt[1-9])$/i.test(p.split('.')[0].trimEnd()));}
export function validateManifest(manifest,allowUnprepared=false){
  if(Buffer.byteLength(JSON.stringify(manifest))>MAX_MANIFEST)throw Error('Manifest budget');
  if(manifest.schema!==2||!token(manifest.id)||!token(manifest.version)||!token(manifest.platform)||!manifest.review||Array.isArray(manifest.review)||typeof manifest.review!=='object'||!Array.isArray(manifest.files)||!manifest.files.length||manifest.files.length>MAX_FILES||Object.keys(manifest).some(k=>!['schema','id','version','platform','files','review'].includes(k)))throw Error('Invalid v2 manifest');
  const names=new Set();let expanded=0,compressed=MAX_MANIFEST+12;
  for(const entry of manifest.files){
    if(!safeName(entry.path)||names.has(entry.path.toLowerCase())||!Number.isSafeInteger(entry.size)||entry.size<0||entry.size>MAX_FILE||!/^[a-f0-9]{64}$/.test(entry.sha256)||Object.keys(entry).some(k=>!['path','size','compressedSize','sha256','executable'].includes(k))||('executable'in entry&&typeof entry.executable!=='boolean'))throw Error('Invalid v2 file entry');
    if(!allowUnprepared&&(!Number.isSafeInteger(entry.compressedSize)||entry.compressedSize<20||entry.compressedSize>MAX_FILE+1048576))throw Error('Unprepared compression lock');
    names.add(entry.path.toLowerCase());expanded+=entry.size;compressed+=entry.compressedSize||0;
  }
  for(const name of names)for(let at=name.indexOf('/');at!==-1;at=name.indexOf('/',at+1))if(names.has(name.slice(0,at)))throw Error('File is also an archive parent');
  if(expanded>MAX_EXPANDED||(!allowUnprepared&&compressed>MAX_DOWNLOAD))throw Error('Pack exceeds streaming budget');
}
async function writeAll(file,bytes){let at=0;while(at<bytes.length){const {bytesWritten}=await file.write(bytes,at,bytes.length-at);if(!bytesWritten)throw Error('Short package write');at+=bytesWritten;}}
async function compress(runtime,entry,output){
  const file=path.resolve(runtime,...entry.path.split('/'));
  const [real,metadata]=await Promise.all([fs.promises.realpath(file),fs.promises.lstat(file)]),relative=path.relative(runtime,real);
  if(!relative||relative==='..'||relative.startsWith('..'+path.sep)||path.isAbsolute(relative)||metadata.isSymbolicLink()||!metadata.isFile()||metadata.size!==entry.size)throw Error('Runtime entry differs from lock');
  const digest=crypto.createHash('sha256');let size=0,compressed=0;
  const check=new Transform({highWaterMark:BUFFER,transform(chunk,_encoding,next){size+=chunk.length;if(size>entry.size){next(Error('Runtime grew during compression'));return;}digest.update(chunk);next(null,chunk);}});
  const sink=new Writable({highWaterMark:BUFFER,write(chunk,_encoding,next){compressed+=chunk.length;if(compressed>MAX_FILE+1048576){next(Error('Compressed entry budget'));return;}if(output)writeAll(output,chunk).then(()=>next(),next);else next();}});
  await pipeline(fs.createReadStream(file,{highWaterMark:BUFFER}),check,zlib.createGzip({level:9,chunkSize:BUFFER}),sink);
  if(size!==entry.size||digest.digest('hex')!==entry.sha256)throw Error('Runtime digest differs from reviewed lock');
  return compressed;
}
export async function build({manifestPath,runtime,output,writeLock=false,onProgress=()=>{}}){
  const metadata=fs.statSync(manifestPath);if(metadata.size>MAX_MANIFEST)throw Error('Manifest budget');
  const expected=JSON.parse(fs.readFileSync(manifestPath));validateManifest(expected,writeLock);
  runtime=fs.realpathSync(runtime);output=path.resolve(output);
  if(fs.existsSync(output))throw Error('Output already exists; immutable archives cannot be replaced');
  fs.mkdirSync(path.dirname(output),{recursive:true});
  const manifest=structuredClone(expected);
  // Only the read-only sizing pass runs concurrently. Four bounded pipelines
  // retain fixed buffers; output remains ordered and is never interleaved.
  let next=0,completed=0,failed;
  await Promise.all(Array.from({length:Math.min(4,manifest.files.length)},async()=>{
    while(!failed&&next<manifest.files.length){
      const entry=manifest.files[next++];
      try{const compressed=await compress(runtime,entry);if(!writeLock&&entry.compressedSize!==compressed)throw Error('Compressed bytes differ from lock');entry.compressedSize=compressed;completed++;if(completed%2000===0||completed===manifest.files.length)onProgress({phase:'sizing',files:completed,total:manifest.files.length});}
      catch(error){failed??=error;}
    }
  }));
  if(failed)throw failed;
  validateManifest(manifest,false);
  const header=Buffer.from(JSON.stringify(manifest));if(header.length>MAX_MANIFEST)throw Error('Manifest budget');
  const temporary=output+'.'+crypto.randomUUID()+'.partial';let file;
  try{
    file=await fs.promises.open(temporary,'wx');const length=Buffer.alloc(4);length.writeUInt32LE(header.length);await writeAll(file,Buffer.from('RMDPKG2\n'));await writeAll(file,length);await writeAll(file,header);
    let written=0;
    for(const entry of manifest.files){const compressed=await compress(runtime,entry,file);if(compressed!==entry.compressedSize)throw Error('Runtime changed between compression passes');written++;if(written%2000===0||written===manifest.files.length)onProgress({phase:'writing',files:written,total:manifest.files.length});}
    await file.sync();await file.close();file=null;
    // Exclusive same-filesystem publication; never rename over an old asset.
    await fs.promises.link(temporary,output);
    if(writeLock){const lockTemporary=manifestPath+'.'+crypto.randomUUID()+'.partial';try{fs.writeFileSync(lockTemporary,header,{flag:'wx'});fs.renameSync(lockTemporary,manifestPath);}finally{if(fs.existsSync(lockTemporary))fs.unlinkSync(lockTemporary);}}
    const digest=crypto.createHash('sha256');for await(const bytes of fs.createReadStream(output,{highWaterMark:BUFFER}))digest.update(bytes);
    return {prepared:true,offline:true,format:'stream-v2',id:manifest.id,files:manifest.files.length,bytes:fs.statSync(output).size,installedBytes:manifest.files.reduce((n,f)=>n+f.size,0),sha256:digest.digest('hex'),published:false,enabledInCatalog:false};
  }finally{if(file)await file.close();if(fs.existsSync(temporary))fs.unlinkSync(temporary);}
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  try{const args=process.argv.slice(2),writeLock=args.includes('--write-lock'),positional=args.filter(a=>a!=='--write-lock');if(positional.length!==3)throw Error('Usage: node tools/build-stream-capability-pack.mjs MANIFEST RUNTIME OUTPUT [--write-lock]');console.log(JSON.stringify(await build({manifestPath:positional[0],runtime:positional[1],output:positional[2],writeLock})));}catch(error){console.error(error.message);process.exitCode=1;}
}
