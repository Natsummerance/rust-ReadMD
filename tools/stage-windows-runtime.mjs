// Offline app-local runtime staging. Never runs a redistributable installer or
// writes System32/registry. The same locked small DLLs serve core and workers.
import fs from 'node:fs';import path from 'node:path';import crypto from 'node:crypto';import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..'),source=path.join(root,'assets/runtimes/windows-x64');
export function stageWindowsRuntime(destination){
 const lock=JSON.parse(fs.readFileSync(path.join(source,'runtime.lock.json')));
 if(lock.schema!==1||lock.platform!=='windows-x64'||lock.files.length!==2)throw Error('Invalid bundled Windows runtime lock');
 let bytes=0;for(const item of lock.files){
  if(!['vcruntime140.dll','vcruntime140_1.dll'].includes(item.path))throw Error('Unexpected runtime file');
  const data=fs.readFileSync(path.join(source,item.path));
  if(data.length!==item.size||crypto.createHash('sha256').update(data).digest('hex')!==item.sha256)throw Error('Bundled Windows runtime digest mismatch');
  const pe=data.readUInt32LE(0x3c);if(data.toString('ascii',pe,pe+4)!=='PE\0\0'||data.readUInt16LE(pe+4)!==0x8664)throw Error('Bundled runtime is not x64');
  if(destination){fs.mkdirSync(destination,{recursive:true});fs.copyFileSync(path.join(source,item.path),path.join(destination,item.path));}bytes+=data.length;
 }return {files:lock.files.length,bytes,version:lock.version,offline:true};
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
 if(!process.argv[2])throw Error('Usage: node tools/stage-windows-runtime.mjs OUTPUT_DIR | --check');
 console.log(JSON.stringify(stageWindowsRuntime(process.argv[2]==='--check'?null:path.resolve(process.argv[2]))));
}
