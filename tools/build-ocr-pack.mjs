#!/usr/bin/env node
// Inert, reviewed engine/model payload. No download or dependency resolution.
import fs from 'node:fs';import path from 'node:path';import crypto from 'node:crypto';import zlib from 'node:zlib';
const args=process.argv.slice(2),value=key=>args[args.indexOf(key)+1];
if(!args.includes('--runtime')||!args.includes('--output'))throw Error('Usage: --runtime DIR --output FILE [--write-lock]');
const runtime=path.resolve(value('--runtime')),output=path.resolve(value('--output')),sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const inputs=JSON.parse(fs.readFileSync('tools/ocr-runtime-inputs.lock.json'));
const files=[],payload={};
for(const [input,target]of [['tesseract.exe','bin/tesseract.exe'],...['eng','chi_sim','chi_tra'].map(n=>[n+'.traineddata','tessdata/'+n+'.traineddata']),...['TESSERACT-LICENSE','LEPTONICA-LICENSE','MODELS-LICENSE'].map(n=>[n,'licenses/'+n]),['NOTICE','NOTICE']]){
 const b=fs.readFileSync(path.join(runtime,input));const model=inputs.items.find(i=>i.file===input);if(model&&(b.length!==model.bytes||sha(b)!==model.sha256))throw Error('Model differs from locked upstream source');
 files.push({path:target,size:b.length,sha256:sha(b)});payload[target]={compression:'gzip',data:zlib.gzipSync(b,{level:9}).toString('base64')};
}
// The static build must not silently reintroduce a VC redistributable or codec
// DLL dependency. Its only import is the Windows platform kernel API.
const b=fs.readFileSync(path.join(runtime,'tesseract.exe')),pe=b.readUInt32LE(0x3c),count=b.readUInt16LE(pe+6),opt=pe+24,size=b.readUInt16LE(pe+20);if(b.readUInt16LE(opt)!==0x20b)throw Error('Expected Windows x64 PE');
function offset(r){for(let i=0;i<count;i++){const s=opt+size+40*i,start=b.readUInt32LE(s+12);if(r>=start&&r<start+Math.max(b.readUInt32LE(s+8),b.readUInt32LE(s+16)))return b.readUInt32LE(s+20)+r-start;}throw Error('Invalid PE section');}
const imports=[];for(let i=offset(b.readUInt32LE(opt+112+8));b.readUInt32LE(i+12);i+=20){const start=offset(b.readUInt32LE(i+12));imports.push(b.subarray(start,b.indexOf(0,start)).toString());}if(imports.length!==1||imports[0].toLowerCase()!=='kernel32.dll')throw Error('Unreviewed runtime dependency: '+imports.join(','));
const manifest={schema:1,id:'ocr',version:'5.5.3-r1',platform:'windows-x64',files,review:{license:'Tesseract and models Apache-2.0; Leptonica BSD-2-Clause; static MSVC runtime',source:'https://github.com/tesseract-ocr/tesseract/tree/'+inputs.items.find(i=>i.id==='tesseract').commit,inputs:inputs.items.filter(i=>i.id!=='cmake'),build:'tools/build-ocr-runtime.mjs; all codec/network/archive/GUI/training/legacy features disabled; portable CPU dispatch',execution:'Rust decodes bounded input to local PNM; static worker reads only application-locked models; bounded output, deadline and cancellation',systemImports:imports}};
const lock='assets/capabilities/ocr-5.5.3-r1-windows-x64.lock.json';
if(args.includes('--write-lock'))fs.writeFileSync(lock,JSON.stringify(manifest,null,2)+'\n');else if(JSON.stringify(JSON.parse(fs.readFileSync(lock)))!==JSON.stringify(manifest))throw Error('Runtime differs from application lock');
const bytes=Buffer.from(JSON.stringify({manifest,payload}));if(bytes.length>128*1024*1024)throw Error('Package too large');fs.mkdirSync(path.dirname(output),{recursive:true});fs.writeFileSync(output,bytes);console.log(JSON.stringify({id:manifest.id,bytes:bytes.length,installedBytes:files.reduce((n,f)=>n+f.size,0),sha256:sha(bytes),imports}));
