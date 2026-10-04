/** Verify real recordings, then exercise the published gallery in a browser. */
import fs from 'node:fs';
import path from 'node:path';
import http from 'node:http';
import crypto from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
import {CAPTURE_PROFILE,recordingUiHash} from './capture-mode.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const out=path.join(root,'showcase'),site=path.join(root,'website/dist');
const manifest=JSON.parse(fs.readFileSync(path.join(out,'manifest.json')));
const inventory=JSON.parse(fs.readFileSync(path.join(root,manifest.inventory.replace('../',''))));
const hash=p=>crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const require=createRequire(import.meta.url);
const {chromium,expect}=require(path.join(process.env.READMD_UI_NODE_MODULES||path.join(root,'ui-tests/node_modules'),'@playwright/test'));
const checks=[];
function assert(value,message){if(!value)throw Error(message);}
assert(manifest.inventory_sha256===hash(path.join(root,manifest.inventory.replace('../',''))),'Inventory hash is stale');
assert(JSON.stringify(manifest.features.map(f=>f.id))===JSON.stringify(inventory.features.map(f=>f.id)),'Feature coverage differs from inventory');
const currentUiHash=recordingUiHash(root);
assert(manifest.capture_profile===CAPTURE_PROFILE&&manifest.ui_source_sha256===currentUiHash,'Capture profile or UI snapshot is stale');
for(const f of manifest.features){
 const r=f.recording;assert(r.status==='recorded',f.id+' has no successful recording');
 assert(r.capture_profile===CAPTURE_PROFILE&&r.ui_source_sha256===currentUiHash,f.id+' must be rerecorded with the current immersive UI');
 assert(r.recorded_at&&r.overlays&&Object.values(r.overlays).every(value=>value===false),f.id+' contains recording overlays');
 assert(r.evidence.length&&r.steps.length,f.id+' has no proof or operation steps');
 assert(r.evidence.some(e=>e.assertion==='immersive_capture_presentation_restored'&&e.temporary_styles_removed===true),f.id+' did not restore notification presentation');
 const video=path.join(out,r.video);assert(hash(video)===r.sha256,f.id+' video hash mismatch');
 assert(r.privacy?.approved&&r.privacy.sha256===r.sha256,f.id+' privacy approval is missing or stale');
 assert(hash(path.join(out,r.poster))===r.privacy.poster_sha256,f.id+' poster privacy approval is stale');
 const probe=spawnSync('ffprobe',['-v','error','-show_entries','format=duration,size:stream=width,height,codec_name,pix_fmt','-of','json',video],{windowsHide:true,encoding:'utf8'});
 assert(probe.status===0,f.id+' cannot be decoded');const m=JSON.parse(probe.stdout),s=m.streams[0];
 assert(s.codec_name==='h264'&&['yuv420p','yuvj420p'].includes(s.pix_fmt)&&s.width>=1280,f.id+' video is not web compatible');
 assert(Math.abs(Number(m.format.duration)-r.duration)<.06&&Number(m.format.size)===r.bytes,f.id+' metadata mismatch');
 assert(fs.readFileSync(path.join(out,r.captions),'utf8').startsWith('WEBVTT\n'),f.id+' missing captions');
 for(const file of [r.video,r.poster,r.captions])assert(hash(path.join(out,file))===hash(path.join(site,'showcase',file)),f.id+' site asset is stale');
 checks.push({id:f.id,seconds:r.duration,bytes:r.bytes,sha256:r.sha256});
}
const mime={'.html':'text/html','.css':'text/css','.js':'text/javascript','.json':'application/json','.mp4':'video/mp4','.webp':'image/webp','.png':'image/png','.ico':'image/x-icon','.vtt':'text/vtt','.woff2':'font/woff2','.svg':'image/svg+xml'};
const server=http.createServer((req,res)=>{
 let file;try{const u=new URL(req.url,'http://localhost');file=path.resolve(site,'.'+decodeURIComponent(u.pathname));if(u.pathname.endsWith('/'))file=path.join(file,'index.html');}catch{res.writeHead(400).end();return;}
 if(!file.startsWith(site+path.sep)){res.writeHead(403).end();return;}
 if(!fs.existsSync(file)||!fs.statSync(file).isFile()){res.writeHead(404).end();return;}
 const size=fs.statSync(file).size,headers={'Content-Type':mime[path.extname(file)]||'application/octet-stream','Accept-Ranges':'bytes'};
 const range=req.headers.range?.match(/^bytes=(\d+)-(\d*)$/);
 if(range){const start=Number(range[1]),end=Math.min(Number(range[2]||size-1),size-1);res.writeHead(206,{...headers,'Content-Range':`bytes ${start}-${end}/${size}`,'Content-Length':end-start+1});fs.createReadStream(file,{start,end}).pipe(res);}
 else{res.writeHead(200,{...headers,'Content-Length':size});fs.createReadStream(file).pipe(res);}
});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const base='http://127.0.0.1:'+server.address().port;
const channel=process.env.READMD_VERIFY_BROWSER||(process.platform==='win32'?'msedge':null);
const browser=await chromium.launch(channel?{channel}:{});const layouts=[],playback=[];
try{
 for(const route of ['/','/zh-cn/','/zh-tw/','/ja/','/showcase/'])for(const viewport of [{width:1160,height:820},{width:1024,height:680},{width:390,height:844}])for(const theme of ['light','dark']){
  const page=await browser.newPage({viewport,colorScheme:theme});const errors=[];
  page.on('pageerror',e=>errors.push(e.message));page.on('response',r=>{if(r.url().startsWith(base)&&r.status()>=400)errors.push('HTTP '+r.status()+' '+new URL(r.url()).pathname);});
  await page.addInitScript(theme=>{localStorage.setItem('readmd.site.theme',theme);},theme);
  await page.goto(base+route);await page.locator(route==='/showcase/'?'.card':'.product-frame img').first().waitFor();
  const overflow=await page.evaluate(()=>({width:innerWidth,scroll:document.documentElement.scrollWidth,items:[...document.querySelectorAll('body *')].filter(e=>e.getBoundingClientRect().right>innerWidth+1&&getComputedStyle(e).position!=='fixed').slice(0,8).map(e=>({tag:e.tagName,class:e.className,right:e.getBoundingClientRect().right}))}));
  if(overflow.scroll>overflow.width+1){await page.screenshot({path:path.join(out,'checks/layout-failure.png')});throw Error(`Overflow ${route} ${viewport.width} ${theme}: ${JSON.stringify(overflow)}`);}
  assert((await page.locator('h1').count())===1,'Missing main heading '+route);
  const palette=await page.evaluate(()=>({background:getComputedStyle(document.body).backgroundColor,font:getComputedStyle(document.body).fontFamily}));
  assert(palette.background===(theme==='dark'?'rgb(0, 0, 0)':route==='/showcase/'?'rgb(245, 245, 247)':'rgb(255, 255, 255)'),`Unexpected original-style palette ${route} ${theme}: ${palette.background}`);
  assert(palette.font.includes('SF Pro'),`Original system font missing: ${route}`);
  if(route==='/showcase/'){
   await expect(page.locator('.card')).toHaveCount(inventory.features.length);
   await page.locator('#search').fill('F075');await expect(page.locator('.card')).toHaveCount(1);
   await page.locator('#search').fill('不存在的功能');await expect(page.locator('#empty')).toBeVisible();await page.locator('#search').fill('');
   await page.locator('.category').nth(1).click();await expect(page.locator('.category').nth(1)).toHaveAttribute('aria-pressed','true');await page.locator('.category').first().click();
   await page.waitForFunction(()=>[...document.querySelectorAll('.card img')].filter(e=>e.getBoundingClientRect().top<innerHeight).every(e=>e.complete&&e.naturalWidth>0));
   const previews=await page.locator('.card img').evaluateAll(images=>images.slice(0,2).map(img=>{const r=img.getBoundingClientRect();return{width:r.width,height:r.height,loaded:img.complete&&img.naturalWidth>0};}));
   assert(previews.every(p=>p.loaded&&Math.abs(p.height-p.width/1.6)<2),`Preview images have incorrect dimensions: ${route} ${viewport.width}`);
  }
  if(route==='/zh-cn/'&&theme==='light'||route==='/showcase/'&&theme==='dark')await page.screenshot({path:path.join(out,'checks',`${route==='/showcase/'?'gallery':'home'}-${viewport.width}-${theme}.png`),fullPage:false});
  assert(!errors.length,'Website errors '+route+': '+errors.join('; '));layouts.push({route,viewport,theme,passed:true});await page.close();
 }
 const direct=await browser.newPage({viewport:{width:1160,height:820}});
 await direct.goto(base+'/showcase/#F016');await expect(direct.locator('#player')).toBeVisible();
 await direct.waitForFunction(()=>document.querySelector('video').readyState>=2);
 await expect(direct.locator('#steps li')).toHaveCount(manifest.features.find(f=>f.id==='F016').recording.steps.length);
 await direct.close();
 const page=await browser.newPage({viewport:{width:1160,height:820}});await page.goto(base+'/showcase/');await page.locator('.card').first().waitFor();
 for(const f of manifest.features){
  await page.locator('#search').fill(f.id);await expect(page.locator('.card')).toHaveCount(1);await page.locator('.card').click();
  await expect(page.locator('#player')).toBeVisible();try{await page.waitForFunction(()=>document.querySelector('video').readyState>=2);}catch{throw Error(f.id+' video did not decode: '+await page.locator('video').evaluate(v=>JSON.stringify({error:v.error?.message,code:v.error?.code,ready:v.readyState,network:v.networkState,source:v.currentSrc,canPlay:v.canPlayType('video/mp4; codecs="avc1.64001f"')})));}
  await page.evaluate(()=>{const v=document.querySelector('video');v.pause();v.currentTime=Math.min(1,v.duration/2);});
  await page.waitForFunction(()=>!document.querySelector('video').seeking&&document.querySelector('video').videoWidth>=1280);
  try{await page.waitForFunction(()=>document.querySelector('track').readyState===2);}catch{throw Error(f.id+' captions did not load: '+await page.locator('track').evaluate(t=>JSON.stringify({state:t.readyState,src:t.src,mode:t.track.mode,cues:t.track.cues?.length})));}
  assert(await page.locator('track').evaluate(t=>t.track.mode==='hidden'),f.id+' captions must be off by default');
  playback.push({id:f.id,decoded:true,captions:true,captions_default:'hidden'});await page.keyboard.press('Escape');await expect(page.locator('#player')).toBeHidden();
  assert(await page.locator('video').evaluate(v=>!v.getAttribute('src')),f.id+' player retained a video after closing');
 }
 await page.close();
}finally{await browser.close();await new Promise(r=>server.close(r));}
const durations=checks.map(x=>x.seconds).sort((a,b)=>a-b);
const report={date:new Date().toISOString(),capture_profile:CAPTURE_PROFILE,ui_source_sha256:currentUiHash,notifications_restored:checks.length,inventory_sha256:manifest.inventory_sha256,features:checks.length,total_bytes:checks.reduce((n,c)=>n+c.bytes,0),median_seconds:durations[Math.floor(durations.length/2)],max_seconds:durations.at(-1),layouts,playback,direct_link:{id:'F016',loaded:true,steps:true},recordings:checks};
fs.writeFileSync(path.join(out,'checks/verification.json'),JSON.stringify(report,null,2)+'\n');
console.log(`Verified ${checks.length} recordings, ${layouts.length} layouts and ${playback.length} decoded videos with captions`);
