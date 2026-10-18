'use strict';
// Real browser rasterization. Canvas never reads personal files or captures UI.
const fs=require('node:fs'),assert=require('node:assert/strict');
module.exports=async function rasterFixture(destination,options,page){
 let browser;
 try{
  if(!page){const {chromium}=require('playwright');browser=await chromium.launch({channel:'msedge',headless:true});page=await browser.newPage();}
  const png=await page.evaluate(async options=>{
   const canvas=document.createElement('canvas');canvas.width=options.width;canvas.height=options.height;const context=canvas.getContext('2d');context.fillStyle='white';context.fillRect(0,0,canvas.width,canvas.height);
   if(options.source){const image=new Image();image.src='data:image/png;base64,'+options.source;await image.decode();for(let y=0;y<3400;y+=image.height+30)for(let x=0;x<2800;x+=image.width+30)context.drawImage(image,x,y);}
   else{context.fillStyle='black';context.font='80px Arial';context.fillText(options.label,80,180);}
   return canvas.toDataURL('image/png').split(',')[1];
  },options);
  const bytes=Buffer.from(png,'base64');assert.equal(bytes.subarray(0,8).toString('hex'),'89504e470d0a1a0a');assert.equal(bytes.readUInt32BE(16),options.width);assert.equal(bytes.readUInt32BE(20),options.height);fs.writeFileSync(destination,bytes);
 }finally{if(browser)await browser.close();}
};
