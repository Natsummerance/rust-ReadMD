import fs from 'node:fs';
import path from 'node:path';
async function connectPet(h,renderer='hermes-sprite'){
 let port;
 const matches=url=>url.includes('readmd-pet.localhost')&&new URL(url).searchParams.get('renderer')===renderer;
 await h.expect.poll(async()=>{for(const p of h.debugPorts()){try{const targets=await fetch(`http://127.0.0.1:${p}/json/list`).then(r=>r.json());if(targets.some(t=>matches(t.url))){port=p;return true;}}catch{}}return false;},{timeout:30000}).toBe(true);
 const browser=await h.chromium.connectOverCDP('http://127.0.0.1:'+port);
 h.petBrowsers ||= [];h.petBrowsers.push(browser);
 h.petPage=browser.contexts().flatMap(c=>c.pages()).find(p=>matches(p.url()));
 await h.petPage.waitForFunction(()=>!!window.__bongoPet?.opaqueRegions?.length,{timeout:30000});
 const s=(await h.api('/api/pets/status')).status;h.petPid=s.adapter.rust.pid;
 const b=h.nativeBounds;
 const current=(await h.native('bounds',null,false,{pid:h.petPid})).petBounds;
 // Use the real host command so the position survives subsequent snapshots.
 // An external SetWindowPos alone is overwritten when character state updates.
 await h.petPage.evaluate(({x,y,width,height})=>window.hermesDesktop.petOverlay.setBounds({x:x/devicePixelRatio,y:y/devicePixelRatio,width:width/devicePixelRatio,height:height/devicePixelRatio}),{x:b.x+b.width-520,y:b.y+220,width:current.width,height:current.height});
 await h.delay(500);
 h.petBounds=(await h.native('bounds',null,false,{pid:h.petPid})).petBounds;
}
export async function prepareDesktopPet(h){
 await h.open('notes/研究方法.md');
 await h.api('/api/pets/configure',{enabled:true,in_app:false,renderer:'hermes-sprite',character:'bongocat',scale:.30,always_on_top:true,lock_position:false,bubbles:h.feature.id==='F094',quiet:false});
 await connectPet(h);
}
async function point(h){
 await h.petPage.waitForFunction(()=>window.__bongoPet?.opaqueRegions?.some(r=>r.width>0&&r.height>0));
 h.petBounds=(await h.native('bounds',null,false,{pid:h.petPid})).petBounds;
 let p;
 await h.expect.poll(async()=>{p=await h.petPage.evaluate(()=>{
  const rects=window.__bongoPet.opaqueRegions;
  // Choose a pixel well inside the silhouette, not a thin animated edge.
  const cell=4,w=Math.ceil(innerWidth/cell),height=Math.ceil(innerHeight/cell),grid=new Uint8Array(w*height);
  for(const r of rects)for(let y=Math.max(0,Math.floor(r.y/cell));y<Math.min(height,Math.ceil((r.y+r.height)/cell));y++)for(let x=Math.max(0,Math.floor(r.x/cell));x<Math.min(w,Math.ceil((r.x+r.width)/cell));x++)grid[y*w+x]=1;
  // Native hit testing reserves bubble and menu rectangles for their controls.
  for(const el of document.querySelectorAll('[data-pet-interactive]')){
   if(!el.getClientRects().length||getComputedStyle(el).visibility==='hidden')continue;const r=el.getBoundingClientRect();
   for(let y=Math.max(0,Math.floor(r.top/cell));y<Math.min(height,Math.ceil(r.bottom/cell));y++)for(let x=Math.max(0,Math.floor(r.left/cell));x<Math.min(w,Math.ceil(r.right/cell));x++)grid[y*w+x]=0;
  }
  let best=null,bestRadius=-1;
  for(let y=1;y<height-1;y++)for(let x=1;x<w-1;x++){if(!grid[y*w+x])continue;let radius=0;
   for(let k=1;k<20;k++){let inside=true;for(let n=-k;n<=k;n++)if(x+n<0||x+n>=w||y-k<0||y+k>=height||!grid[(y-k)*w+x+n]||!grid[(y+k)*w+x+n]||x-k<0||x+k>=w||y+n<0||y+n>=height||!grid[(y+n)*w+x-k]||!grid[(y+n)*w+x+k]){inside=false;break;}if(!inside)break;radius=k;}
   if(radius>bestRadius){bestRadius=radius;best={x:(x+.5)*cell,y:(y+.5)*cell};}
  }
  return best?{...best,w:innerWidth,h:innerHeight}:null;
 });return !!p;},{timeout:15000}).toBe(true);
 const b=h.petBounds;return{x:Math.round(b.x+p.x*b.width/p.w),y:Math.round(b.y+p.y*b.height/p.h)};
}
async function mouse(h,extra){return h.native('mouse',null,false,{pid:h.petPid,...extra});}
async function cleanup(h){
 h.hidePetCapture=true;
 await h.api('/api/pets/configure',{enabled:false,in_app:false}).catch(()=>{});
 for(const b of h.petBrowsers||[])await b.close().catch(()=>{});
}
export const petRecipes={
 F093:{native:true,prepare:prepareDesktopPet,run:async h=>{try{
  const before=await h.petPage.evaluate(()=>window.__bongoPet.state.tapCounts.left);
  await h.step('BongoCat 跟随真实键盘和鼠标输入',()=>h.native('keys',null,false,{keys:'asdfasdf'}));
  await h.expect.poll(()=>h.petPage.evaluate(()=>window.__bongoPet.state.tapCounts.left)).toBeGreaterThan(before);
  let p=await point(h);const initial=h.petBounds;
  await h.step('从角色实体拖动原生窗口',()=>mouse(h,{...p,drag:true,toX:p.x-130,toY:p.y+70}));
  const moved=(await h.native('bounds',null,false,{pid:h.petPid})).petBounds;
  if(Math.abs(moved.x-initial.x)<60)throw Error('Native pet did not move');
  await h.step('透明边缘允许点击穿透',()=>mouse(h,{x:moved.x+2,y:moved.y+2}));
  await h.expect.poll(async()=> (await h.native('bounds',null,false,{pid:h.petPid})).transparent).toBe(true);
  await h.step('原有精灵保留自己的动画与点击互动',()=>h.api('/api/pets/configure',{renderer:'hermes-sprite',character:'mochi'}));
  await h.expect.poll(()=>h.petPage.evaluate(()=>window.__bongoPet.presentation)).toBe('mochi');
  p=await point(h);await mouse(h,{...p,click:true});await h.delay(700);
  await h.step('Live2D 使用模型动作与视线跟随',()=>h.api('/api/pets/configure',{renderer:'live2d',character:'arch-chan'}));
  await connectPet(h,'live2d');await h.petPage.waitForFunction(()=>!!window.__readmdLive2d);
  p=await point(h);await mouse(h,{x:p.x-100,y:p.y-60});await h.delay(500);await mouse(h,{...p,click:true});await h.delay(700);
  const model=await h.petPage.evaluate(()=>({ready:document.body.dataset.live2dReady,pose:window.__readmdLive2d.inputPose}));
  if(model.ready!=='true')throw Error('Live2D model never became ready');
  h.evidence.push({assertion:'actual_native_drag_raw_input_click_through_and_three_renderers',delta:{x:moved.x-initial.x,y:moved.y-initial.y},live2d:model});
 }finally{await cleanup(h);}}},
 F094:{native:true,prepare:prepareDesktopPet,run:async h=>{try{
  let p=await point(h);await h.step('右键角色，打开原生桌宠快捷菜单',()=>mouse(h,{...p,button:'right',click:true}));
  const menu=h.petPage.getByRole('menu');await h.expect(menu).toBeVisible();
  await h.step('进入角色子菜单，切换原创精灵',()=>menu.getByRole('menuitem',{name:/Characters|角色/}).click());
  const choice=await h.petPage.evaluate(()=>window.__bongoPet.state.petInfo.characters.find(c=>c.slug!=='bongocat'&&c.slug!=='arch-chan'));
  if(!choice)throw Error('Native character menu has no sprite character');
  const name=choice.name;
  await menu.getByRole('menuitemradio',{name,exact:true}).click();
  await h.expect.poll(()=>h.petPage.evaluate(()=>window.__bongoPet.presentation)).toBe(choice.slug);
  await h.expect.poll(()=>h.petPage.evaluate(()=>!!window.__bongoPet.spritePose && window.__bongoPet.opaqueRegions.length>0)).toBe(true);
  await h.delay(350);p=await point(h);
  const before=await h.petPage.evaluate(()=>{window.__showcasePetInteractions||=0;window.addEventListener('readmd-pet-interacted',()=>window.__showcasePetInteractions++,{once:true});return window.__showcasePetInteractions;});
  await h.step('点击角色，触发动作和互动反馈',()=>mouse(h,{...p,click:true}));
  await h.expect.poll(()=>h.petPage.evaluate(()=>window.__showcasePetInteractions),{timeout:5000}).toBeGreaterThan(before);
  p=await point(h);await mouse(h,{...p,button:'right',click:true});
  await h.step('从快捷菜单回到 ReadMD 设置',()=>menu.getByRole('menuitem',{name:/Settings|设置/}).click());
  await h.expect(h.page.locator('#pet-settings-modal')).toBeVisible();
  h.evidence.push({assertion:'actual_native_context_menu_character_click_and_settings'});
 }finally{await cleanup(h);}}},
 F095:{native:true,prepare:prepareDesktopPet,run:async h=>{try{
  const source=path.join(h.materials,'哲学阅读研究手册.md');const original=fs.readFileSync(source,'utf8');let explorer;
  await h.step('选择原创阅读手册（系统文件窗口不录入）',async()=>{const b=h.nativeBounds;explorer=await h.native('explorer-open',h.materials,false,{file:source,x:b.x+40,y:b.y+140,width:Math.round(b.width*.53),height:Math.round(b.height*.60)});});
  const p=await point(h);await h.step('把文件拖到独立桌宠，唤起 ReadMD',()=>mouse(h,{...explorer.source,drag:true,toX:p.x,toY:p.y}));
  await h.expect.poll(()=>h.page.evaluate(()=>state.file?.replaceAll('\\','/')),{timeout:15000}).toBe(source.replaceAll('\\','/'));
  await h.native('explorer-close',h.materials);await h.expect(h.page.locator('#content')).toContainText('论证与反例');
  if(fs.readFileSync(source,'utf8')!==original)throw Error('Drop changed its source file');
  h.evidence.push({assertion:'actual_explorer_ole_drop_full_path_opens_unchanged_original'});
 }finally{await h.native('explorer-close',h.materials).catch(()=>{});await cleanup(h);}}}
};
