#!/usr/bin/env node
// This command only assembles already reviewed local resources. No download,
// dependency install, generated scripts or online package resolution.
import fs from 'node:fs';import path from 'node:path';import crypto from 'node:crypto';import zlib from 'node:zlib';
const args=process.argv.slice(2),value=key=>args[args.indexOf(key)+1];
if(!args.includes('--runtime')||!args.includes('--archive')||!args.includes('--output'))throw Error('Usage: node tools/build-typst-pack.mjs --runtime DIR --archive ZIP --output FILE [--write-lock]');
const root=process.cwd(),runtime=path.resolve(value('--runtime')),archive=path.resolve(value('--archive')),output=path.resolve(value('--output'));
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const archiveSha256='19ce3551153c2fe7ee9fa2f95208310c8f4d3209fedb699e0333faf8913f6736';
if(sha(fs.readFileSync(archive))!==archiveSha256)throw Error('Official release archive digest mismatch');
const payload={},files=[];
for(const [input,name]of [['typst.exe','bin/typst.exe'],['LICENSE','LICENSE'],['NOTICE','NOTICE']]){
  const bytes=fs.readFileSync(path.join(runtime,input));files.push({path:name,size:bytes.length,sha256:sha(bytes)});
  payload[name]={compression:'gzip',data:zlib.gzipSync(bytes,{level:9}).toString('base64')};
}
const manifest={schema:1,id:'typst',version:'0.15.1',platform:'windows-x64',files,review:{license:'Apache-2.0; bundled dependency and font notices retained in NOTICE',source:'https://github.com/typst/typst/releases/tag/v0.15.1',archive:'https://github.com/typst/typst/releases/download/v0.15.1/typst-x86_64-pc-windows-msvc.zip',archiveSha256,delivery:'Optional offline worker; not linked or embedded in the main executable',execution:'Generated quoted sources only, owned root, bounded/cancellable worker, no package imports'}};
const lock=path.join(root,'assets/capabilities/typst-0.15.1-windows-x64.lock.json');
if(args.includes('--write-lock'))fs.writeFileSync(lock,JSON.stringify(manifest,null,2)+'\n');
else if(JSON.stringify(JSON.parse(fs.readFileSync(lock)))!==JSON.stringify(manifest))throw Error('Runtime does not match the application lock');
const pack=Buffer.from(JSON.stringify({manifest,payload}));if(pack.length>32*1024*1024)throw Error('Offline pack exceeds browser upload budget');
fs.mkdirSync(path.dirname(output),{recursive:true});fs.writeFileSync(output,pack);
console.log(JSON.stringify({version:manifest.version,platform:manifest.platform,bytes:pack.length,sha256:sha(pack),runtimeBytes:files.reduce((n,f)=>n+f.size,0),offline:true}));
