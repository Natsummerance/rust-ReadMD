#!/usr/bin/env node
// Offline development asset generation. The distributed app uses the ICO;
// it never requires Node, a browser, system fonts or this build tool.
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {createRequire} from 'node:module';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const {chromium}=createRequire(path.join(root,'ui-tests/package.json'))('playwright');
// Usage: node tools/generate-document-icon.mjs input.svg [output-file-stem]
const input=process.argv[2]||path.join(root,'assets/markdown-file.svg');
const output=process.argv[3]||path.join(root,'assets/markdown-file');
const svg=fs.readFileSync(input,'utf8');fs.mkdirSync(path.dirname(output),{recursive:true});
const sizes=[16,20,24,32,40,48,64,96,128,256];
const browser=await chromium.launch({channel:process.env.READMD_ICON_BROWSER||'msedge',headless:true});
try{
  const page=await browser.newPage();
  const frames=[];
  for(const size of sizes){
    const rendered=await page.evaluate(async({svg,size})=>{
      const image=new Image();image.src='data:image/svg+xml;base64,'+btoa(unescape(encodeURIComponent(svg)));await image.decode();
      const canvas=document.createElement('canvas');canvas.width=size;canvas.height=size;const ctx=canvas.getContext('2d');ctx.drawImage(image,0,0,size,size);
      const rgba=ctx.getImageData(0,0,size,size).data;
      return {png:canvas.toDataURL('image/png').split(',')[1],chunks:Array.from({length:Math.ceil(rgba.length/0x8000)},(_,i)=>btoa(String.fromCharCode(...rgba.slice(i*0x8000,(i+1)*0x8000))))};
    },{svg,size});
    let data;
    if(size===256){data=Buffer.from(rendered.png,'base64');fs.writeFileSync(output+'-256.png',data);}
    else{
      const rgba=Buffer.concat(rendered.chunks.map(chunk=>Buffer.from(chunk,'base64'))),stride=Math.ceil(size/32)*4;
      data=Buffer.alloc(40+size*size*4+stride*size);data.writeUInt32LE(40,0);data.writeInt32LE(size,4);data.writeInt32LE(size*2,8);data.writeUInt16LE(1,12);data.writeUInt16LE(32,14);data.writeUInt32LE(size*size*4+stride*size,20);
      for(let y=0;y<size;y++)for(let x=0;x<size;x++){
        const input=(y*size+x)*4,output=40+((size-1-y)*size+x)*4;
        data[output]=rgba[input+2];data[output+1]=rgba[input+1];data[output+2]=rgba[input];data[output+3]=rgba[input+3];
        if(!rgba[input+3])data[40+size*size*4+(size-1-y)*stride+(x>>3)]|=0x80>>(x%8);
      }
    }
    frames.push({size,data});
  }
  const header=Buffer.alloc(6+16*frames.length);header.writeUInt16LE(1,2);header.writeUInt16LE(frames.length,4);let offset=header.length;
  frames.forEach(({size,data},i)=>{const p=6+i*16;header[p]=header[p+1]=size===256?0:size;header.writeUInt16LE(1,p+4);header.writeUInt16LE(32,p+6);header.writeUInt32LE(data.length,p+8);header.writeUInt32LE(offset,p+12);offset+=data.length;});
  fs.writeFileSync(output+'.ico',Buffer.concat([header,...frames.map(f=>f.data)]));
  console.log(JSON.stringify({asset:output+'.ico',sizes,bytes:offset,source:input}));
}finally{await browser.close();}
