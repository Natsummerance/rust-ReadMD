/** Inventory-driven genuine recordings. No response mocks or simulated results. */
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { createRequire } from 'node:module';
import { spawnSync, spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { recipes } from './recipes.mjs';
import {startNative,debugPorts} from './native-session.mjs';
import {protectCapture} from './privacy-capture.mjs';
import {approveRecording} from './privacy-media.mjs';
import {CAPTURE_PROFILE,uiSourceHash,installImmersiveCapture,restoreCapturePresentation} from './capture-mode.mjs';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const out = path.join(root, 'showcase');
const require = createRequire(import.meta.url);
const deps = process.env.READMD_UI_NODE_MODULES || path.join(root,'ui-tests/node_modules');
const { chromium, expect } = require(path.join(deps,'@playwright/test'));
const { startUiServer } = require(path.join(root,'ui-tests/ui-server.cjs'));
const inventoryPath = path.join(root,'docs/reviews/ui-function-inventory-2026-10-02/inventory.json');
const inventory = JSON.parse(fs.readFileSync(inventoryPath,'utf8'));
const hash = p => crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const run = (cmd,args) => { const r=spawnSync(cmd,args,{windowsHide:true,encoding:'utf8'});if(r.status!==0)throw new Error(`${cmd} failed: ${(r.stderr||'').slice(-1200)}`);return r.stdout; };
const delay = ms => new Promise(r=>setTimeout(r,ms));
const wanted = new Set(process.argv.slice(2).filter(x=>/^F\d{3}$/.test(x)));
const force = process.argv.includes('--force');
const nativeMode=process.argv.includes('--native');
const demoPort=Number(process.env.READMD_DEMO_PORT||28731);
for(const dir of ['videos','posters','captions','checks','.runtime'])fs.mkdirSync(path.join(out,dir),{recursive:true});
const manifestPath=process.env.READMD_DEMO_MANIFEST||path.join(out,'manifest.json');
const previous=fs.existsSync(manifestPath)?JSON.parse(fs.readFileSync(manifestPath,'utf8')):{features:[]};
const sourceHash=uiSourceHash(root);
const manifest={schema:1,date:new Date().toLocaleDateString('en-CA',{timeZone:'Asia/Shanghai'}),capture_profile:CAPTURE_PROFILE,ui_source_sha256:sourceHash,inventory:'../docs/reviews/ui-function-inventory-2026-10-02/inventory.json',inventory_sha256:hash(inventoryPath),viewport:{width:1280,height:800},features:inventory.features.map(f=>({...f,recording:previous.features.find(p=>p.id===f.id)?.recording||{status:'pending'}}))};
const persist=()=>fs.writeFileSync(manifestPath,JSON.stringify(manifest,null,2)+'\n');persist();
const runtime=fs.mkdtempSync(path.join(out,'.runtime/run-'));
function removeOwnedRuntime(target) {
 const resolved=path.resolve(target),scope=path.resolve(runtime);
 if((resolved!==scope&&!resolved.startsWith(scope+path.sep))||!scope.startsWith(path.resolve(out,'.runtime')+path.sep))throw Error('Cleanup target is outside this recording run');
 if(fs.existsSync(resolved)&&fs.lstatSync(resolved).isSymbolicLink())throw Error('Cleanup target must be a real recording directory');
 fs.rmSync(resolved,{recursive:true,force:true,maxRetries:10,retryDelay:300});
}
async function stopOwnWebViewProcesses(directory) {
 if(process.platform!=='win32')return;
 await new Promise((resolve,reject)=>{
  const child=spawn('powershell.exe',['-NoProfile','-NonInteractive','-Command',"$fixture=[Console]::In.ReadToEnd().Trim();Get-CimInstance Win32_Process -Filter \"Name='msedgewebview2.exe'\" | Where-Object {$_.CommandLine -and $_.CommandLine.Contains($fixture)} | ForEach-Object {Stop-Process -Id $_.ProcessId -ErrorAction SilentlyContinue}"],{windowsHide:true,stdio:['pipe','ignore','ignore']});
  child.once('error',reject);child.once('exit',code=>code===0?resolve():reject(Error('Own recording WebView cleanup failed')));child.stdin.end(path.resolve(directory));
 });
}
const data=path.join(runtime,'data');fs.mkdirSync(data);
// The kernel derives the pet installation directory from the assets parent.
// Real directories with hardlinked assets keep all writable demo state isolated.
function linkTree(source,target){fs.mkdirSync(target,{recursive:true});for(const e of fs.readdirSync(source,{withFileTypes:true})){if(e.name.includes('WebView2')||e.name==='node_modules')continue;const s=path.join(source,e.name),t=path.join(target,e.name);if(e.isDirectory())linkTree(s,t);else if(e.isFile()){try{fs.linkSync(s,t);}catch{fs.copyFileSync(s,t);}}}}
const demoApp=path.join(runtime,'app');linkTree(path.join(root,'assets'),path.join(demoApp,'assets'));
const kit=path.join(root,'plugins/pet/readmd-rust-host');if(fs.existsSync(kit))linkTree(kit,path.join(demoApp,'plugins/pet/readmd-rust-host'));
process.env.READMD_ASSETS_DIR=path.join(demoApp,'assets');
const workspace=path.join(runtime,'workspace');fs.mkdirSync(workspace);
process.env.READMD_WORKSPACE=workspace;
const materials=path.join(workspace,'materials');fs.cpSync(path.join(out,'materials'),materials,{recursive:true});
process.env.READMD_DATA_DIR=data;
// Explicit optional local credential source. Nothing from it is logged or published.
if(process.env.READMD_DEMO_AI_DIR){for(const n of ['ai.json','credentials.vault','encryption.key']){const p=path.join(process.env.READMD_DEMO_AI_DIR,n);if(fs.existsSync(p))fs.copyFileSync(p,path.join(data,n));}}
process.env.READMD_BIN ||= 'Z:/readmd-target/main/release/readmd.exe';
const nativeSession=nativeMode?await startNative(root,data,chromium,demoPort):null;
const server=nativeSession?.server||await startUiServer(demoPort,{stdio:'pipe'});
const browser=nativeSession?.browser||await chromium.launch({headless:true});
const managerPath=process.env.READMD_ANTIGRAVITY_CONFIG;
let managerProvider=null;
if(managerPath){
 const proxy=JSON.parse(fs.readFileSync(managerPath,'utf8')).proxy;
 if(!proxy?.enabled||!proxy.api_key)throw new Error('Antigravity Manager proxy is not configured');
 managerProvider={id:'custom:showcase-antigravity',name:'Antigravity Manager',custom:true,base_url:`http://127.0.0.1:${proxy.port}/v1`,api_key:proxy.api_key,models:['gemini-3.8-flash-high'],mode:'chat',format:'openai',endpoint_mode:'prefix'};
}
let failed=0;
try {
 for(const feature of manifest.features){
  if(wanted.size&&!wanted.has(feature.id))continue;
  if(feature.recording.status==='recorded'&&feature.recording.capture_profile===CAPTURE_PROFILE&&feature.recording.ui_source_sha256===sourceHash&&!force)continue;
  const recipe=recipes[feature.id];if(!recipe){continue;}
  if(nativeMode&&!recipe.native&&!wanted.has(feature.id))continue;
  const lastGood=feature.recording.status==='recorded'?feature.recording:null;
  if(recipe.native&&!nativeMode)continue;
  console.log(`${feature.id} preparing ${feature.title}`);
  const work=path.join(workspace,feature.id);fs.mkdirSync(work);
  const caseMaterials=path.join(work,'materials');fs.cpSync(path.join(out,'materials'),caseMaterials,{recursive:true});
  const context=nativeMode?nativeSession.page.context():await browser.newContext({viewport:manifest.viewport,locale:'zh-CN',acceptDownloads:true,permissions:['clipboard-read','clipboard-write']});
  const page=nativeMode?nativeSession.page:await context.newPage();page.setDefaultTimeout(12000);
  await page.addInitScript(()=>{localStorage.setItem('readmd_language','zh-CN');localStorage.setItem('readmd-settings',JSON.stringify({theme:'light'}));});
  const responses=[];
  page.on('response',response=>{const u=new URL(response.url());if(u.pathname.startsWith('/api/'))responses.push({route:u.pathname,status:response.status()});});
  const steps=[],waits=[];let recording=false,frames=[],captureError=null,captureLoop,navigationRetryStarted=0;
  const evidence=[];
  const demoCurrent=managerProvider?{provider_id:managerProvider.id,model:managerProvider.models[0]}:process.env.READMD_DEMO_AI_DIR?JSON.parse(fs.readFileSync(path.join(process.env.READMD_DEMO_AI_DIR,'ai.json'),'utf8')).current:null;
  const h={page,context,expect,root,materials:caseMaterials,data,work,feature,delay,evidence,demoCurrent,debugPorts:()=>debugPorts(runtime),chromium,nativeMode,demoApp,
   native(action,pathValue,folder=false,extra={}){const pending=new Promise((resolve,reject)=>{const child=spawn('powershell.exe',['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-STA','-File',path.join(out,'scripts/native-control.ps1')],{windowsHide:true,stdio:['pipe','pipe','pipe']});let err='',output='';child.stdout.on('data',b=>output+=b);child.stderr.on('data',b=>err+=b);child.once('exit',c=>{if(c!==0)return reject(new Error(err.slice(-800)));try{const result=JSON.parse(output.trim());if(result.bounds)h.nativeBounds=result.bounds;resolve(result);}catch(e){reject(e);}});child.stdin.end(JSON.stringify({pid:server.child.pid,action,path:pathValue,folder,...extra}));});pending.catch(()=>{});return pending;},
   async api(url,body){return page.evaluate(async({url,body})=>{const r=await apiFetch(url,body?{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)}:{});const d=await r.json();if(!r.ok||d.ok===false)throw new Error(d.error_code||d.error||'HTTP '+r.status);return d;},{url,body});},
   async step(text,action){steps.push({at:recording?(Date.now()-h.started)/1000:0,text});await action();await delay(450);},
   async wait(action){const start=recording?(Date.now()-h.started)/1000:null;await action();if(start!==null)waits.push({start,end:(Date.now()-h.started)/1000});},
   async click(selector){const l=page.locator(selector).filter({visible:true}).first();await l.scrollIntoViewIfNeeded();const b=await l.boundingBox();if(!b)throw new Error('Unreachable '+selector);await page.mouse.move(b.x+b.width/2,b.y+b.height/2,{steps:5});await delay(120);await l.click();await delay(350);},
   async reveal(selector){const details=page.locator('details').filter({has:page.locator(selector)});for(let i=0;i<await details.count();i++){if(!await details.nth(i).evaluate(d=>d.open)){await details.nth(i).locator(':scope > summary').click();await delay(150);}}},
   async open(name='哲学阅读研究手册.md'){await page.evaluate(p=>loadFile(p),path.join(caseMaterials,name));await expect(page.locator('#content')).toBeVisible();},
   async edit(name){await h.open(name);await h.click('#btn-edit');await expect(page.locator('.cm-editor')).toBeVisible();},
   async more(id){if(!await page.locator('#'+id).isVisible())await h.click('#btn-more');if(!await page.locator('#'+id).isVisible()){const header=page.locator('.more-group').filter({has:page.locator('#'+id)}).locator('.more-group-header');await header.click();await delay(200);}await h.click('#'+id);},
   async insertMenu(id){await h.click('[data-menu="md-insert-menu"]');await h.click('#'+id);},
   async text(content){await page.locator('.cm-content').click();await page.keyboard.press('Control+End');await page.keyboard.insertText(content);},
   async assertText(text){await expect.poll(()=>page.evaluate(()=>getEditContent())).toContain(text);evidence.push({assertion:'editor_contains',text});},
   async begin(){
    // Explanations stay in sidecars. Captured pixels contain only the app.
    await expect(page.locator('#busy')).toBeHidden({timeout:30000});
    await expect(page.locator('#toast')).toBeHidden({timeout:7000});
    await installImmersiveCapture(page);
    const captureState=await page.evaluate(()=>({toastHidden:getComputedStyle(document.getElementById('toast')).visibility==='hidden',overlayAbsent:!document.getElementById('showcase-caption')&&!document.getElementById('showcase-cursor')}));
    if(!captureState.toastHidden||!captureState.overlayAbsent)throw Error('Immersive capture presentation did not apply');
    h.started=Date.now();recording=true;
    // Never capture the desktop. CDP captures only our actual WebView surface,
    // even when an Explorer, file picker or unrelated application is in front.
    captureLoop=(async()=>{while(recording){const at=Date.now();const p=path.join(work,`frame-${String(frames.length).padStart(5,'0')}.jpg`);try{
     const target=h.capturePage&&!h.capturePage.isClosed()?h.capturePage:page;
     await installImmersiveCapture(target);await protectCapture(target);await target.screenshot({path:p,type:'jpeg',quality:95,timeout:8000});
     let pet=null;
     if(h.petPage&&!h.petPage.isClosed()&&!h.hidePetCapture){
      const petPage=h.petPage,petPath=p.replace('.jpg','-pet.png');
      const [mainMetrics,petMetrics]=await Promise.all([page.evaluate(()=>({x:screenX,y:screenY,w:innerWidth,h:innerHeight})),petPage.evaluate(()=>({x:screenX,y:screenY,w:innerWidth,h:innerHeight}))]);
      await protectCapture(petPage);await petPage.screenshot({path:petPath,omitBackground:true,timeout:8000});
      pet={path:petPath,x:petMetrics.x-mainMetrics.x,y:petMetrics.y-mainMetrics.y,width:petMetrics.w,height:petMetrics.h,mainWidth:mainMetrics.w};
     }
     frames.push({p,at,pet});navigationRetryStarted=0;
    }catch(e){if(!page.isClosed()&&/Execution context was destroyed|Cannot find context with specified id|Inspected target navigated or closed/.test(e.message)){
      navigationRetryStarted ||= Date.now();
      if(Date.now()-navigationRetryStarted<10000){await delay(100);continue;}
     }
     if(!page.isClosed()&&/Target page, context or browser has been closed/.test(e.message)){h.capturePage=null;continue;}captureError=e;break;}await delay(Math.max(0,100-(Date.now()-at)));}})();await delay(450);
   }
  };
  try {
   await page.goto('http://127.0.0.1:'+demoPort+'/');await page.waitForFunction(()=>typeof loadFile==='function'&&window.ReadMDRecovery);
   if(nativeMode){await h.native('dismiss-network');await h.native('resize');}
   if(managerProvider)await page.evaluate(async({provider,current})=>{const r=await apiFetch('/api/ai/config',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({providers:[provider],current})});if(!r.ok)throw Error('Could not configure isolated demo connection');},{provider:managerProvider,current:demoCurrent});
   await recipe.prepare?.(h);await h.begin();await recipe.run(h);await delay(800);recording=false;await captureLoop;
   if(captureError)throw captureError;
   if(!evidence.length)throw new Error('Recipe must verify a genuine result');
   const captureEnded=Date.now(),duration=(captureEnded-h.started)/1000;
   const video=path.join(work,'result.mp4');
   const poster=path.join(work,'result.webp'),captions=path.join(work,'result.vtt');
   {
    const mainDimensions=new Map();
    for(const f of frames.filter(f=>f.pet)){
     if(!mainDimensions.has(f.pet.mainWidth))mainDimensions.set(f.pet.mainWidth,JSON.parse(run('ffprobe',['-v','error','-show_entries','stream=width,height','-of','json',f.p])).streams[0]);
     const dimensions=mainDimensions.get(f.pet.mainWidth);
     const scale=dimensions.width/f.pet.mainWidth;
     const merged=f.p.replace('.jpg','-merged.jpg');
     run('ffmpeg',['-hide_banner','-loglevel','error','-y','-i',f.p,'-i',f.pet.path,'-filter_complex',`[1:v]scale=${Math.round(f.pet.width*scale)}:${Math.round(f.pet.height*scale)}[pet];[0:v][pet]overlay=x=${Math.round(f.pet.x*scale)}:y=${Math.round(f.pet.y*scale)}:format=auto`,'-frames:v','1','-q:v','1',merged]);
     f.p=merged;
    }
    const concat=frames.map((f,i)=>`file '${f.p.replaceAll('\\','/')}'\nduration ${Math.max(.04,((frames[i+1]?.at||captureEnded)-f.at)/1000).toFixed(3)}`).join('\n')+'\n'+`file '${frames.at(-1).p.replaceAll('\\','/')}'\n`;
    fs.writeFileSync(path.join(work,'frames.txt'),concat);
    run('ffmpeg',['-hide_banner','-loglevel','error','-y','-safe','0','-f','concat','-i',path.join(work,'frames.txt'),'-vf',nativeMode?'scale=1300:850:flags=lanczos,fps=20':'fps=20','-c:v','libx264','-preset','fast','-crf','18','-pix_fmt','yuv420p','-movflags','+faststart','-an',video]);
   }
   const privacy=await approveRecording(video,feature.id,work,duration);
   run('ffmpeg',['-hide_banner','-loglevel','error','-y','-i',video,'-ss',String(Math.min(recipe.captureUi?.posterAt??duration*.65,duration-1)),'-frames:v','1',poster]);
   privacy.poster_sha256=hash(poster);
   const meta=JSON.parse(run('ffprobe',['-v','error','-show_entries','format=duration,size:stream=width,height,codec_name','-of','json',video]));
   const time=s=>{const n=Math.round(s*1000);return `${String(Math.floor(n/3600000)).padStart(2,'0')}:${String(Math.floor(n/60000)%60).padStart(2,'0')}:${String(Math.floor(n/1000)%60).padStart(2,'0')}.${String(n%1000).padStart(3,'0')}`;};
   fs.writeFileSync(captions,'WEBVTT\n\n'+steps.map((s,i)=>`${time(s.at)} --> ${time(steps[i+1]?.at||duration)}\n${s.text}\n`).join('\n'));
   for(const [source,dest]of [[video,`videos/${feature.id}.mp4`],[poster,`posters/${feature.id}.webp`],[captions,`captions/${feature.id}.vtt`]])fs.copyFileSync(source,path.join(out,dest));
   await restoreCapturePresentation(h.capturePage);await restoreCapturePresentation(h.petPage);await restoreCapturePresentation(page);
   if(await page.locator('#showcase-capture-style,#showcase-privacy-layer').count())throw Error('Recording presentation was not restored');
   evidence.push({assertion:'immersive_capture_presentation_restored',temporary_styles_removed:true});
   feature.recording={status:'recorded',capture_profile:CAPTURE_PROFILE,ui_source_sha256:sourceHash,recorded_at:new Date().toISOString(),overlays:{burned_captions:false,feature_number:false,synthetic_cursor:false,toasts:false,system_windows:false},video:`videos/${feature.id}.mp4`,poster:`posters/${feature.id}.webp`,captions:`captions/${feature.id}.vtt`,duration:Number(meta.format.duration),bytes:Number(meta.format.size),sha256:hash(video),steps,waits,evidence,api:responses,capture_surface:nativeMode?'isolated-native-webviews':'isolated-browser',privacy};persist();
   console.log(`${feature.id} recorded ${feature.recording.duration.toFixed(1)}s ${feature.title}`);
   await restoreCapturePresentation(page);if(!nativeMode)await context.close();removeOwnedRuntime(work);
  }catch(error){recording=false;await captureLoop?.catch(()=>{});failed++;const canonical=lastGood&&path.join(out,lastGood.video);feature.recording=canonical&&fs.existsSync(canonical)&&hash(canonical)===lastGood.sha256?lastGood:{status:'failed'};persist();await page.screenshot({path:path.join(work,'failure.png')}).catch(()=>{});const diagnostic=await page.locator('.batch-item.error .batch-state,#export-result,#ai-conn-status').evaluateAll(nodes=>nodes.map(n=>({text:n.textContent,title:n.title}))).catch(()=>[]);fs.writeFileSync(path.join(work,'failure.json'),JSON.stringify({id:feature.id,error:error.message,diagnostic,api:responses},null,2));console.error(`${feature.id} failed: ${error.message.slice(0,320)}`);await restoreCapturePresentation(page).catch(()=>{});if(nativeMode)await h.native('close-dialogs').catch(()=>{});else await context.close();}
 }
}finally{await browser.close();await new Promise(resolve=>{if(server.child.exitCode!==null)return resolve();server.child.once('exit',resolve);server.child.kill();});await server.stop();if(nativeMode)await stopOwnWebViewProcesses(runtime);}
console.log(`Current immersive UI coverage ${manifest.features.filter(x=>x.recording.status==='recorded'&&x.recording.capture_profile===CAPTURE_PROFILE&&x.recording.ui_source_sha256===sourceHash).length}/${manifest.features.length}; ${failed} failures this run`);
if(!failed)removeOwnedRuntime(runtime);
process.exitCode=failed?1:0;

