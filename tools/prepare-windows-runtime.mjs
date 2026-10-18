// Extract the exact, previously reviewed Microsoft runtime offline. No MSI,
// redistributable installer, dependency download, registry or system write.
import fs from 'node:fs';import path from 'node:path';import crypto from 'node:crypto';import {spawnSync} from 'node:child_process';import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..'),[archive,output]=process.argv.slice(2);
if(process.platform!=='win32'||!archive||!output)throw Error('Usage on Windows: node tools/prepare-windows-runtime.mjs LOCAL_ARCHIVE OUTPUT_DIR');
const lock=JSON.parse(fs.readFileSync(path.join(root,'assets/runtimes/windows-x64/runtime.lock.json'))),bytes=fs.readFileSync(path.resolve(archive)),sha=b=>crypto.createHash('sha256').update(b).digest('hex');
if(bytes.length!==lock.source.bytes||sha(bytes)!==lock.source.sha256)throw Error('Archive differs from the reviewed Microsoft signature/digest');
const cache=path.join(root,'.cache/capability-upgrade');fs.mkdirSync(cache,{recursive:true});const work=fs.mkdtempSync(path.join(cache,'runtime-extract-'));
function expand(cab,dir){fs.mkdirSync(dir,{recursive:true});const r=spawnSync(path.join(process.env.SystemRoot,'System32/expand.exe'),['-F:*',cab,dir],{encoding:'utf8',windowsHide:true});if(r.status!==0)throw Error('Offline cabinet extraction failed');}
let count=0;for(let at=0;(at=bytes.indexOf('MSCF',at))>=0;at+=4){const size=bytes.readUInt32LE(at+8);if(size<40||at+size>bytes.length||bytes[at+24]!==3||bytes[at+25]!==1)continue;const file=path.join(work,`container-${count}.cab`);fs.writeFileSync(file,bytes.subarray(at,at+size));expand(file,path.join(work,`container-${count++}`));}
const results=new Map();for(let i=0;i<count;i++)for(const name of fs.readdirSync(path.join(work,`container-${i}`))){const file=path.join(work,`container-${i}`,name),data=fs.readFileSync(file);if(data.toString('ascii',0,4)!=='MSCF')continue;const dir=path.join(work,`payload-${i}-${name}`);expand(file,dir);for(const item of lock.files){const extracted=path.join(dir,item.path+'_amd64');if(!fs.existsSync(extracted))continue;const dll=fs.readFileSync(extracted);if(dll.length!==item.size||sha(dll)!==item.sha256)throw Error('Extracted DLL differs from its application lock');results.set(item.path,dll);}}
if(results.size!==lock.files.length)throw Error('Required x64 runtime DLL missing');fs.mkdirSync(path.resolve(output),{recursive:true});for(const[name,dll]of results)fs.writeFileSync(path.join(path.resolve(output),name),dll);
// Keep the explicit extraction workspace for review and classified cleanup.
console.log(JSON.stringify({files:results.size,bytes:lock.files.reduce((n,f)=>n+f.size,0),offline:true,installerExecuted:false}));
