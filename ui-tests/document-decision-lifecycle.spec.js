const {test,expect}=require('@playwright/test');
const fs=require('node:fs/promises'),os=require('node:os'),path=require('node:path');
const {waitForPage}=require('./page-condition.cjs');
let owned,a,b;
const original='\ufeff# 固定原件\r\n\r\n中文正文。🙂\r\n';
const tail='\r\n等待确认的修改。\r\n';
test.beforeEach(async({page})=>{
 owned=await fs.mkdtemp(path.join(process.env.READMD_WORKSPACE||os.tmpdir(),'lifecycle-'));
 a=path.join(owned,'中文 原件.md');b=path.join(owned,'另一个目标.md');
 await fs.writeFile(a,original);await fs.writeFile(b,'# Another document\n');
 await page.addInitScript(()=>localStorage.setItem('readmd_language','en'));
 await page.goto('/');await waitForPage(page,()=>!!window.__readmdSessionReady);await page.evaluate(()=>window.__readmdSessionReady);
 await page.evaluate(async at=>{await loadFile(at);await toggleEdit();cmView.dispatch({changes:{from:cmView.state.doc.length,insert:'\n等待确认的修改。\n'}});await ReadMDReview.flush();},a);
});
test.afterEach(async()=>{if(owned)await fs.rm(owned,{recursive:true,force:true});});

test('an OS open waits until the dirty editor discard and detach finish',async({page})=>{
 let entered,release;const waiting=new Promise(r=>entered=r),held=new Promise(r=>release=r);
 await page.route('**/api/documents/history',async route=>{
  const data=route.request().postDataJSON();
  if(data?.op==='record'&&data.kind==='discarded'){entered();await held;}
  await route.continue();
 });
 await page.evaluate(()=>{window.lifecycleExit=confirmExitEdit();});await page.locator('#close-confirm-discard').click();await waiting;
 await page.evaluate(at=>{
  const fetch=window.fetch;window.lifecycleReads=0;window.fetch=function(...args){if(String(args[0]).includes('/api/file?'))window.lifecycleReads++;return fetch.apply(this,args);};
  window.lifecycleOpen=loadFile(at);
 },b);
 await page.evaluate(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))));
 try{
  expect(await page.evaluate(()=>window.lifecycleReads)).toBe(0);
  expect(await page.evaluate(()=>({path:getActiveTab().path,editing:state.editing}))).toEqual({path:a,editing:true});
 }finally{release();}
 expect(await page.evaluate(()=>window.lifecycleExit)).toBe(true);await page.evaluate(()=>window.lifecycleOpen);
 expect(await page.evaluate(()=>getActiveTab().path)).toBe(b);expect(await fs.readFile(a,'utf8')).toBe(original);
 const discarded=await page.evaluate(async()=>{const list=await(await apiFetch('/api/documents/history')).json(),entry=list.entries.find(x=>x.kind==='discarded');return(await(await apiFetch('/api/documents/history',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({op:'read',id:entry.id})})).json()).content;});
 expect(discarded).toBe(original.replace(/^\ufeff/,'')+tail);
});

test('editing then undoing to identical text invalidates an older discard decision',async({page})=>{
 const before=await page.evaluate(()=>({content:getEditContent(),generation:getActiveTab().editGeneration}));
 await page.evaluate(()=>{window.lifecycleExit=confirmExitEdit();});await expect(page.locator('#close-confirm-modal')).toBeVisible();
 await page.evaluate(async()=>{
  const at=cmView.state.doc.length;cmView.dispatch({changes:{from:at,insert:'临时输入'}});cmView.dispatch({changes:{from:at,to:cmView.state.doc.length,insert:''}});await ReadMDReview.flush();
 });
 expect(await page.evaluate(()=>getEditContent())).toBe(before.content);
 expect(await page.evaluate(()=>getActiveTab().editGeneration)).toBeGreaterThan(before.generation);
 await page.locator('#close-confirm-discard').click();
 expect(await page.evaluate(()=>window.lifecycleExit)).toBe(false);
 expect(await page.evaluate(()=>({editing:state.editing,content:getEditContent()}))).toEqual({editing:true,content:before.content});
 expect(await fs.readFile(a,'utf8')).toBe(original);
});

test('typing while discard cleanup waits keeps the editor and a newer durable draft',async({page})=>{
 let entered,release,key;const waiting=new Promise(r=>entered=r),held=new Promise(r=>release=r);
 await page.route('**/api/documents/history',async route=>{
  const data=route.request().postDataJSON();
  if(data?.op==='clear_draft'){key=data.key;entered();await held;}
  await route.continue();
 });
 await page.evaluate(()=>{window.lifecycleExit=confirmExitEdit();});await page.locator('#close-confirm-discard').click();await waiting;
 const newer=await page.evaluate(async({key,at})=>{
  cmView.dispatch({changes:{from:cmView.state.doc.length,insert:'\n这段新输入不能丢失。\n'}});await ReadMDReview.flush();
  const content=getEditContent();const result=await apiFetch('/api/documents/history',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({op:'record',kind:'draft',reason:'edit',key,path:at,name:'中文 原件.md',content,context:{}})});
  if(!result.ok)throw Error('Draft write failed');return content;
 },{key,at:a});
 release();expect(await page.evaluate(()=>window.lifecycleExit)).toBe(false);
 expect(await page.evaluate(()=>({editing:state.editing,content:getEditContent()}))).toEqual({editing:true,content:newer});
 const durable=await page.evaluate(async key=>{const list=await(await apiFetch('/api/documents/history')).json(),entry=list.entries.find(x=>x.kind==='draft'&&x.doc_key===key);if(!entry)return null;return(await(await apiFetch('/api/documents/history',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({op:'read',id:entry.id})})).json()).content;},key);
 expect(durable).toBe(newer);expect(await fs.readFile(a,'utf8')).toBe(original);
});

test('home cancellation and a later discard both settle without dropping the page',async({page})=>{
 await page.evaluate(()=>{window.lifecycleHome=goHome();});await page.locator('#close-confirm-cancel').click();
 expect(await page.evaluate(()=>window.lifecycleHome)).toBe(false);expect(await page.evaluate(()=>state.editing)).toBe(true);
 await page.evaluate(()=>{window.lifecycleHome=goHome();});await page.locator('#close-confirm-discard').click();
 expect(await page.evaluate(()=>window.lifecycleHome)).toBe(true);expect(await page.evaluate(()=>({mode:state.mode,tabs:state.tabs.length}))).toEqual({mode:'welcome',tabs:1});
 expect(await fs.readFile(a,'utf8')).toBe(original);
});

test('new document cancellation and discard target only the intended editor',async({page})=>{
 await page.evaluate(()=>{window.lifecycleNew=newDocument();});await page.locator('#close-confirm-cancel').click();await page.evaluate(()=>window.lifecycleNew);
 expect(await page.evaluate(()=>({path:getActiveTab().path,tabs:state.tabs.length}))).toEqual({path:a,tabs:1});
 await page.evaluate(()=>{window.lifecycleNew=newDocument();});await page.locator('#close-confirm-discard').click();
 expect(await page.evaluate(()=>window.lifecycleNew)).toBe(true);
 expect(await page.evaluate(()=>({virtual:getActiveTab().isVirtual,editing:state.editing,tabs:state.tabs.length}))).toEqual({virtual:true,editing:true,tabs:2});
 expect(await fs.readFile(a,'utf8')).toBe(original);
});

test('save-and-exit refuses an old request after edit and undo to identical text',async({page})=>{
 let entered,release;const waiting=new Promise(r=>entered=r),held=new Promise(r=>release=r);
 await page.route('**/api/save',async route=>{entered();await held;await route.continue();});
 const before=await page.evaluate(()=>getEditContent());
 await page.evaluate(()=>{window.lifecycleSave=saveEdit({exitAfterSave:true});});await waiting;
 await page.evaluate(()=>{const at=cmView.state.doc.length;cmView.dispatch({changes:{from:at,insert:'临时输入'}});cmView.dispatch({changes:{from:at,to:cmView.state.doc.length,insert:''}});});
 release();expect(await page.evaluate(()=>window.lifecycleSave)).toBe(false);
 expect(await page.evaluate(()=>({editing:state.editing,content:getEditContent()}))).toEqual({editing:true,content:before});
 expect(await fs.readFile(a,'utf8')).toBe(original+tail);
});

test('a new file cannot inherit the previous document selection or editor state',async({page})=>{
 const matching=await page.evaluate(()=>cmView.state.doc.toString());await fs.writeFile(b,matching);
 await page.evaluate(()=>cmView.dispatch({selection:{anchor:2,head:7}}));
 await page.evaluate(async at=>{await loadFile(at);await toggleEdit();},b);
 expect(await page.evaluate(()=>getActiveTab().path)).toBe(b);
 expect(await page.evaluate(()=>({empty:cmView.state.selection.main.empty,head:cmView.state.selection.main.head}))).toEqual({empty:true,head:0});
 const earlier=await page.evaluate(at=>state.tabs.find(tab=>tab.path===at).id,a);
 await page.evaluate(async id=>{await switchTab(id);await toggleEdit();},earlier);
 expect(await page.evaluate(()=>({anchor:cmView.state.selection.main.anchor,head:cmView.state.selection.main.head}))).toEqual({anchor:2,head:7});
 expect(await fs.readFile(a,'utf8')).toBe(original);expect(await fs.readFile(b,'utf8')).toBe(matching);
});
