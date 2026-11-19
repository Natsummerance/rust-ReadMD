'use strict';
const fs=require('node:fs'),path=require('node:path'),{chromium}=require('playwright');
const out=process.env.READMD_VSCODE_OPENAPI_AUDIT,delay=ms=>new Promise(r=>setTimeout(r,ms));
// Serialized by Playwright. Read-only DOM/animation sampling only; no text,
// scrolling, focus, style changes, animation manipulation or notification close.
async function waitForOwnedToastFrame({toast,target,maxMs,stableFrames=4,tolerance=0.1}){
 const started=performance.now(),seen=new Map(),cancelled=new Set();let frames=0,stable=0,last,lastConditions,signature='',lastReason='not_sampled';
 if(!Number.isFinite(maxMs)||maxMs<=0||maxMs>2500)return{ok:false,reason:'capture_budget_invalid'};
 return new Promise(resolve=>{
  let done=false,timer=0,raf=0;
  const finish=(ok,reason)=>{if(done)return;done=true;clearTimeout(timer);if(raf)cancelAnimationFrame(raf);resolve({ok,reason,elapsedMs:performance.now()-started,frames,stableFrames:stable,requiredStableFrames:stableFrames,toleranceCssPx:tolerance,animationSettled:lastConditions?.animationsSettled===true,observedAnimations:seen.size,cancelledAnimations:cancelled.size,...(last?{geometry:last}:{}),...(lastConditions?{conditions:lastConditions}:{})});};
  const rect=e=>{const r=e.getBoundingClientRect();const v={x:r.x,y:r.y,width:r.width,height:r.height,left:r.left,top:r.top,right:r.right,bottom:r.bottom};if(!Object.values(v).every(Number.isFinite))throw Error('geometry_unknown');return v;};
  const inside=(r,v,positive)=>r.width>=(positive?Number.EPSILON:0)&&r.height>=(positive?Number.EPSILON:0)&&r.left>=v.left&&r.top>=v.top&&r.right<=v.left+v.width&&r.bottom<=v.top+v.height;
  const equal=(a,b)=>a&&b&&Object.keys(a).every(k=>typeof a[k]==='object'?equal(a[k],b[k]):Math.abs(a[k]-b[k])<=tolerance);
  function sample(){
   if(done)return;
   try{
    frames++;if(performance.now()-started>=maxMs)return finish(false,lastReason==='not_sampled'?'capture_deadline':lastReason);
    if(!toast?.isConnected||!target?.isConnected||!toast.contains(target)||target.closest('.notification-toast')!==toast)return finish(false,'owned_toast_identity_changed');
    const ancestors=[];let container=null,node=toast.parentElement;
    while(node){if(ancestors.length>=64)return finish(false,'ancestor_budget');ancestors.push(node);if(node.matches('.notifications-toasts'))container=node;if(node===document.documentElement)break;node=node.parentElement;}
    if(!container)return finish(false,'notification_container_missing');
    if(ancestors.at(-1)!==document.documentElement)return finish(false,'ancestor_chain_unknown');
    const animations=new Set();
    // Never collect ancestor subtrees: sibling notifications and editor caret
    // animations are unrelated to this pinned toast.
    for(const [element,subtree]of [[toast,true],...ancestors.map(e=>[e,false])]){
     if(typeof element.getAnimations!=='function')return finish(false,'web_animations_unavailable');
     for(const animation of element.getAnimations({subtree})){animations.add(animation);if(animations.size>64)return finish(false,'animation_budget');}
    }
    let pending=false;const states=[];
    for(const animation of animations){
     if(!seen.has(animation)){if(seen.size>=64)return finish(false,'animation_budget');seen.set(animation,seen.size+1);}
     const state=animation.playState;if(!['idle','running','paused','finished'].includes(state)||typeof animation.pending!=='boolean')return finish(false,'animation_state_unknown');
     if(state==='idle'&&!animation.pending){cancelled.add(animation);states.push(seen.get(animation)+':idle');continue;}
     const end=animation.effect?.getComputedTiming?.().endTime;
     if(!Number.isFinite(end)||end<0)return finish(false,'animation_timing_unknown_or_nonfinite');
     pending=pending||animation.pending===true||state==='running'||state==='paused';states.push(seen.get(animation)+':'+state+':'+end+':'+Boolean(animation.pending));
    }
    for(const animation of seen.keys())if(animation.playState==='idle')cancelled.add(animation);
    const nextSignature=states.sort().join('|');
    const visual=window.visualViewport,viewport={left:visual?.offsetLeft??0,top:visual?.offsetTop??0,width:visual?.width??window.innerWidth,height:visual?.height??window.innerHeight};
    if(!Object.values(viewport).every(Number.isFinite)||viewport.width<=0||viewport.height<=0)return finish(false,'viewport_unknown');
    const geometry={toast:rect(toast),target:rect(target),container:rect(container),viewport};
    const styles=[toast,target,...ancestors].map(e=>getComputedStyle(e));
    const visible=styles.every(s=>s.display!=='none'&&s.visibility!=='hidden'&&s.visibility!=='collapse'&&Number.isFinite(Number(s.opacity))&&Number(s.opacity)>=0.99);
    const r=geometry.target,dx=r.width/4,dy=r.height/4,points=[[(r.left+r.right)/2,(r.top+r.bottom)/2],[r.left+dx,r.top+dy],[r.right-dx,r.top+dy],[r.left+dx,r.bottom-dy],[r.right-dx,r.bottom-dy]];
    const hit=points.every(([x,y])=>{const element=document.elementFromPoint(x,y);return element===target||target.contains(element);});
    const inViewport=inside(geometry.toast,viewport,true)&&inside(r,viewport,true)&&inside(geometry.container,viewport,false),enabled=target.disabled!==true&&target.getAttribute('aria-disabled')!=='true';
    const ready=!pending&&visible&&inViewport&&enabled&&hit;
    lastConditions={animationsSettled:!pending,visible,toastAndTargetWithinViewport:inViewport,targetEnabled:enabled,hitTargetClear:hit,hitSamples:5,ancestorCount:ancestors.length,currentToastSubtree:true,ancestorSubtrees:false};
    stable=ready?(nextSignature===signature&&equal(geometry,last)?stable+1:1):0;
    lastReason=pending?'animation_not_settled':!visible?'toast_not_visible':!inViewport?'geometry_outside_viewport':!enabled?'action_disabled':!hit?'action_occluded':'geometry_not_stable';
    last=geometry;signature=nextSignature;
    if(stable>=stableFrames)return finish(true,'finite_animations_and_owned_hit_geometry_stable');
    raf=requestAnimationFrame(sample);
   }catch{return finish(false,'capture_observation_failed');}
  }
  timer=setTimeout(()=>finish(false,lastReason==='not_sampled'?'capture_deadline':lastReason),maxMs);
  raf=requestAnimationFrame(sample);
 });
}
(async()=>{let browser;try{
 const deadline=Date.now()+90000;
 while(!browser&&Date.now()<deadline){try{browser=await chromium.connectOverCDP('http://127.0.0.1:'+process.env.READMD_VSCODE_DEBUG_PORT);}catch{await delay(200);}}
 if(!browser)throw Error('Native VS Code debugging endpoint did not start');
 const page=browser.contexts().flatMap(c=>c.pages()).find(p=>p.url().includes('workbench'));if(!page)throw Error('Native workbench missing');
 async function waitFile(name){const file=path.join(out,name);while(!fs.existsSync(file)){if(Date.now()>=deadline)throw Error('Native projection handshake deadline: '+name);let s;try{s=JSON.parse(fs.readFileSync(path.join(out,'state.json'),'utf8'));}catch{}if(s?.phase==='failed')throw Error('Extension Host failed during '+name);await delay(40);}return fs.readFileSync(file,'utf8');}
 const renderFrame=()=>page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
 async function beforePng(phase,suffix,toast,action){
  const file=path.join(out,phase+suffix+'.frame-proof.json'),until=Math.min(deadline,Date.now()+2500);let toastElement,targetElement,proof;
  const bounded=async promise=>{let timer;try{return await Promise.race([promise,new Promise((_,reject)=>{timer=setTimeout(()=>reject(Object.assign(Error('capture_host_deadline'),{code:'capture_host_deadline'})),Math.max(0,until-Date.now()));})]);}finally{clearTimeout(timer);}};
  const current=()=>{const state=JSON.parse(fs.readFileSync(path.join(out,'state.json'),'utf8'));if(state.phase!==phase)throw Error('capture_phase_changed');};
  try{current();if(until<=Date.now())throw Error('capture_deadline');toastElement=await toast.elementHandle({timeout:Math.max(1,until-Date.now())});targetElement=await toast.getByRole('button',{name:action,exact:true}).elementHandle({timeout:Math.max(1,until-Date.now())});
   if(!toastElement||!targetElement)throw Error('capture_target_missing');
   proof=await bounded(page.evaluate(waitForOwnedToastFrame,{toast:toastElement,target:targetElement,maxMs:until-Date.now()}));current();
   if(proof.ok){if(Date.now()>=until)proof={...proof,ok:false,reason:'capture_deadline'};else{const same=await bounded(toast.evaluate((element,pinned)=>element===pinned,toastElement,{timeout:Math.max(1,until-Date.now())}));if(!same)proof={...proof,ok:false,reason:'current_toast_replaced'};}}
  }catch(error){proof={...(proof||{}),ok:false,reason:error.code==='capture_host_deadline'?'capture_host_deadline':'capture_preflight_failed',errorName:error.name};}
  finally{try{await bounded(Promise.allSettled([toastElement?.dispose(),targetElement?.dispose()]));}catch{proof={...(proof||{}),ok:false,reason:'capture_handle_cleanup_deadline'};}}
  fs.writeFileSync(file,JSON.stringify({schema:'readmd-owned-toast-frame-proof-v1',phase,frame:phase+suffix+'.png',action,perCaptureLimitMs:2500,proof,containsDocumentText:false,pixelOrNativeAcceptanceCertified:false},null,2)+'\n',{flag:'wx',mode:0o600});
  if(!proof?.ok)throw Error('Native frame readiness failed: '+proof?.reason);
 }
 for(const phase of ['cancel','accept','metadata-only','source-conflict','document-conflict']){
  let expectedChanges;
  while(Date.now()<deadline){let state;try{state=JSON.parse(fs.readFileSync(path.join(out,'state.json'),'utf8'));}catch{}if(state?.phase===phase){expectedChanges=state.expectedChanges;break;}if(state?.phase==='failed')throw Error('Extension Host failed before '+phase);await delay(80);}
  if(expectedChanges!==({cancel:4,accept:4,'metadata-only':0,'source-conflict':3,'document-conflict':3})[phase])throw Error('Fixed complex-fixture change count missing for '+phase);
  const toast=page.locator('.notification-toast').filter({hasText:'Review '+expectedChanges+' API changes.'}).last();
  await toast.waitFor({state:'visible',timeout:30000});
  await beforePng(phase,'',toast,['cancel','accept','metadata-only'].includes(phase)?'View raw text':'Accept changes');
  await page.screenshot({path:path.join(out,phase+'.png')});fs.writeFileSync(path.join(out,phase+'.ready'),'');
  if(['cancel','accept','metadata-only'].includes(phase)){
   const summary=JSON.parse(await waitFile(phase+'.summary.checked'));if(summary.phase!==phase||summary.raw!==false||!summary.targetUnchanged||!summary.readonlyProviderPair||!summary.reviewId)throw Error('Owned Summary proof missing');
   await toast.getByRole('button',{name:'View raw text',exact:true}).click();await toast.getByRole('button',{name:'Show readable diff',exact:true}).waitFor({state:'visible',timeout:5000});
   fs.writeFileSync(path.join(out,phase+'.raw.ready'),JSON.stringify({phase,reviewId:summary.reviewId}));
   const raw=JSON.parse(await waitFile(phase+'.raw.checked'));if(raw.phase!==phase||raw.reviewId!==summary.reviewId||raw.raw!==true||!raw.targetUnchanged)throw Error('Owned Raw proof missing');
   await renderFrame();await beforePng(phase,'.raw',toast,phase==='accept'?'Accept changes':'Show readable diff');await page.screenshot({path:path.join(out,phase+'.raw.png')});
   if(phase!=='accept'){
    await toast.getByRole('button',{name:'Show readable diff',exact:true}).click();await toast.getByRole('button',{name:'View raw text',exact:true}).waitFor({state:'visible',timeout:5000});
    fs.writeFileSync(path.join(out,phase+'.summary-again.ready'),JSON.stringify({phase,reviewId:summary.reviewId}));
    const again=JSON.parse(await waitFile(phase+'.summary-again.checked'));if(again.phase!==phase||again.reviewId!==summary.reviewId||again.raw!==false||again.leftSha256!==summary.leftSha256||again.rightSha256!==summary.rightSha256)throw Error('Exact Summary restoration missing');
    await renderFrame();await beforePng(phase,'.summary-again',toast,'Cancel');await page.screenshot({path:path.join(out,phase+'.summary-again.png')});
   }
  }
  await waitFile(phase+'.go');
  await toast.getByRole('button',{name:['cancel','metadata-only'].includes(phase)?'Cancel':'Accept changes',exact:true}).click();fs.writeFileSync(path.join(out,phase+'.clicked'),'');
 }
}finally{if(browser)await browser.close();}})().catch(error=>{fs.writeFileSync(path.join(out,'driver-error.txt'),error.message);console.error(error.message);process.exitCode=1;});
