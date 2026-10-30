// Package the independently built, reviewed offline SVG worker. No downloads.
import fs from 'node:fs';import path from 'node:path';import crypto from 'node:crypto';import {spawnSync} from 'node:child_process';
import {scanNativePaths} from './privacy-scan.mjs';
const [source,binary,metadata,dependencies,output,requestedPlatform='windows-x64']=process.argv.slice(2);
if(!output)throw Error('Usage: node tools/build-svg-renderer.mjs SOURCE_DIR RESVG_BINARY METADATA_JSON DEPENDENCIES_TXT OUTPUT_DIR [windows-x64|macos-arm64|macos-x64|linux-x64]');
const hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const platform=requestedPlatform;
const mac={
  'macos-arm64':{arch:'arm64',target:'aarch64-apple-darwin'},
  'macos-x64':{arch:'x86_64',target:'x86_64-apple-darwin'},
}[platform];
const linux=platform==='linux-x64';
if(platform!=='windows-x64'&&!mac&&!linux)throw Error('Unsupported worker platform');
const name=platform==='windows-x64'?'resvg.exe':'resvg';
const engine=fs.readFileSync(binary);if(engine.length>8*1024*1024)throw Error('Unexpected worker size');
const privacyFailures=[...scanNativePaths(name,engine),...scanNativePaths(name,engine,{buildRoots:['/Users/runner','/home/runner']})];if(privacyFailures.length)throw Error('SVG worker privacy check failed: '+[...new Set(privacyFailures)].join('; '));
if(mac||linux){
  if(mac&&process.platform!=='darwin')throw Error('Mac worker preparation requires native architecture inspection');
  if(linux&&(process.platform!=='linux'||process.arch!=='x64'))throw Error('Linux worker preparation requires the native x64 host');
  if(linux&&(engine.length<64||engine.subarray(0,6).toString('hex')!=='7f454c460201'||engine.readUInt16LE(18)!==62))throw Error('Wrong Linux worker architecture');
  if(mac){
  const arch=spawnSync('/usr/bin/lipo',['-archs',binary],{encoding:'utf8'});
  if(arch.status!==0||arch.stdout.trim()!==mac.arch)throw Error('Wrong Mac worker architecture');
  }
  const version=spawnSync(binary,['--version'],{encoding:'utf8',timeout:10000});
  if(version.status!==0||version.stdout.trim()!=='0.48.1')throw Error('Wrong Mac worker version');
}
const archive=path.join(source,'..','resvg-0.48.1.tar.xz');
const sourceSha256='13ed5a2bae7a01156288ecae5bf944cf7d1c572742c19fc68027947a4d87294c';
if(hash(fs.readFileSync(archive))!==sourceSha256)throw Error('Unreviewed SVG archive');
if(!fs.readFileSync(path.join(source,'crates/resvg/Cargo.toml'),'utf8').includes('version = "0.48.1"'))throw Error('Unreviewed SVG version');
fs.mkdirSync(path.join(output,'licenses'),{recursive:true});fs.writeFileSync(path.join(output,name),engine,{mode:platform==='windows-x64'?0o644:0o755});
for(const name of ['LICENSE-MIT','LICENSE-APACHE','AUTHORS'])fs.copyFileSync(path.join(source,name),path.join(output,'licenses','resvg-'+name+'.txt'));
const used=new Set([...fs.readFileSync(dependencies,'utf8').matchAll(/^([^\s]+) v([^\s]+)/gm)].map(m=>m[1]+'@'+m[2]));
const packages=JSON.parse(fs.readFileSync(metadata,'utf8')).packages.filter(p=>used.has(p.name+'@'+p.version));
const licenses=[];
for(const p of packages){
  const directory=path.dirname(p.manifest_path),files=fs.readdirSync(directory).filter(n=>/^(LICEN[SC]E|COPYING|NOTICE|UNLICENSE|OFL)/i.test(n)&&fs.statSync(path.join(directory,n)).isFile());
  if(!files.length&&p.name!=='resvg'&&p.name!=='usvg')throw Error('Missing license notice for '+p.name);
  for(const name of files)fs.copyFileSync(path.join(directory,name),path.join(output,'licenses',p.name+'-'+p.version+'-'+name.replace(/[^a-z0-9_.-]/gi,'_')+'.txt'));
  licenses.push({name:p.name,version:p.version,license:p.license,notices:files});
}
const lock={schema:1,id:'resvg',version:'0.48.1',platform,size:engine.length,sha256:hash(engine),source:'https://github.com/linebender/resvg/releases/tag/v0.48.1',sourceSha256,license:'Apache-2.0 OR MIT',build:'cargo build --offline --locked --release -p resvg --bin resvg',packages:licenses};
if(mac||linux){lock.build='cargo build --offline --locked --profile production -p resvg --bin resvg --target '+(mac?mac.target:'x86_64-unknown-linux-gnu');lock.cargoLockSha256=hash(fs.readFileSync(path.join(source,'Cargo.lock')));lock.pathRemapping=true;}
fs.writeFileSync(path.join(output,'lock.json'),JSON.stringify(lock,null,2)+'\n');console.log(JSON.stringify({platform,size:engine.length,sha256:lock.sha256,licenses:licenses.length}));
