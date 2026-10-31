// Deterministic, offline rasterization of the existing ReadMD vector mark.
// No models, fonts, network resources or global icon-cache writes.
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
if(process.platform!=='darwin')throw Error('Native Mac asset generation requires macOS');
const stage=path.resolve(process.argv[2]||'');
if(!process.argv[2]||!stage.startsWith(path.dirname(root)+path.sep))throw Error('A new workspace staging directory is required');
fs.mkdirSync(stage);const iconset=path.join(stage,'ReadMD.iconset');fs.mkdirSync(iconset);
const cachePath=path.join(path.dirname(root),'.readmd-local/clang-module-cache/native-qa-swift');fs.mkdirSync(cachePath,{recursive:true});
const cache=fs.realpathSync(cachePath);
const command=(program,args)=>{const r=spawnSync(program,args,{encoding:'utf8',env:{...process.env,CLANG_MODULE_CACHE_PATH:cache,SWIFT_MODULE_CACHE_PATH:cache}});if(r.status!==0)throw Error(path.basename(program)+' failed: '+r.stderr);};
const helper=path.join(stage,'render');
try {
  command('/usr/bin/xcrun',['swiftc','-module-cache-path',cache,path.join(root,'tools/macos-icon-render.swift'),'-o',helper]);
  for(const [name,size] of [['ReadMD-macos',1024],['ReadMD-status-template',72]])command(helper,[path.join(root,'assets',name+'.svg'),String(size),path.join(stage,name+'.png')]);
  for(const size of [16,32,128,256,512])for(const scale of [1,2]) {
    command(helper,[path.join(root,'assets/ReadMD-macos.svg'),String(size*scale),path.join(iconset,`icon_${size}x${size}${scale===2?'@2x':''}.png`)]);
  }
  // ICNS is a length-prefixed container of PNG representations. Write it
  // directly so iconutil cannot create temporary files outside the workspace.
  const specs=[['ic07',128,1],['ic08',256,1],['ic09',512,1],['ic10',512,2],['ic11',16,2],['ic12',32,2],['ic13',128,2],['ic14',256,2]];
  const chunks=specs.map(([type,size,scale])=>{const bytes=fs.readFileSync(path.join(iconset,`icon_${size}x${size}${scale===2?'@2x':''}.png`));const header=Buffer.alloc(8);header.write(type);header.writeUInt32BE(bytes.length+8,4);return Buffer.concat([header,bytes]);});
  const toc=Buffer.alloc(8+chunks.length*8);toc.write('TOC ');toc.writeUInt32BE(toc.length,4);chunks.forEach((chunk,i)=>chunk.copy(toc,8+i*8,0,8));
  const header=Buffer.alloc(8);header.write('icns');header.writeUInt32BE(8+toc.length+chunks.reduce((n,b)=>n+b.length,0),4);
  const icns=path.join(stage,'ReadMD-macos.icns');fs.writeFileSync(icns,Buffer.concat([header,toc,...chunks]),{flag:'wx'});
  // Independently decode the finished container using the native consumer.
  command(helper,[icns,'128',path.join(stage,'decoded-icns.png')]);
  for(const name of ['ReadMD-macos.png','ReadMD-status-template.png','ReadMD-macos.icns'])fs.copyFileSync(path.join(stage,name),path.join(root,'assets',name));
  console.log(JSON.stringify({sizes:[16,32,64,128,256,512,1024],templateSize:72}));
} finally {fs.rmSync(stage,{recursive:true});}
