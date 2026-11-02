const {test,expect}=require('@playwright/test');
const fs=require('node:fs/promises'),os=require('node:os'),path=require('node:path');
const {waitForPage}=require('./page-condition.cjs');
let owned,stable,missing;
test.beforeEach(async({page})=>{
 owned=await fs.mkdtemp(path.join(os.tmpdir(),'readmd-session-page-'));stable=path.join(owned,'已保存.md');missing=path.join(owned,'暂时不可读.md');
 await fs.writeFile(stable,'\ufeff# Stable original\r\n\r\n中文原件。🙂\r\n');
 const session={version:1,tabs:[{key:'file:stable',path:stable,name:path.basename(stable),source:'file',dirty:false},{key:'file:missing',path:missing,name:path.basename(missing),source:'file',dirty:false}],active:'file:missing',folder:''};
 await page.addInitScript(value=>{localStorage.setItem('readmd_language','en');if(!localStorage.getItem('readmd-session'))localStorage.setItem('readmd-session',JSON.stringify(value));},session);
 await page.goto('/');await waitForPage(page,()=>!!window.__readmdSessionReady);await page.evaluate(()=>window.__readmdSessionReady);
 await expect(page.locator('.restore-unavailable')).toBeVisible();
});
test.afterEach(async()=>{if(owned)await fs.rm(owned,{recursive:true,force:true});});

test('unavailable restored file keeps its page and blocks empty edit, save, export and AI capture',async({page})=>{
 expect(await page.evaluate(()=>state.tabs.length)).toBe(2);
 for(const id of ['btn-edit','btn-saveas','btn-print'])await expect(page.locator('#'+id)).toBeDisabled();
 const result=await page.evaluate(async()=>{await toggleEdit();return{editing:state.editing,saved:await saveAs(),capture:await ReadMDReview.capture(),original:getActiveTab().original};});
 expect(result).toEqual({editing:false,saved:false,capture:null,original:null});
 expect(await fs.readFile(stable,'utf8')).toBe('\ufeff# Stable original\r\n\r\n中文原件。🙂\r\n');
 await page.evaluate(()=>ReadMDSession.flush());await page.reload();await page.evaluate(()=>window.__readmdSessionReady);
 await expect(page.locator('.restore-unavailable')).toBeVisible();expect(await page.evaluate(()=>state.tabs.length)).toBe(2);
});

test('retry reads actual bytes into the same page and survives another reload',async({page})=>{
 const id=await page.evaluate(()=>getActiveTab().id);
 await fs.writeFile(missing,'# Returned file\n\n真实恢复的正文。\n',{flag:'wx'});
 await page.locator('[data-restore-retry]').click();await expect(page.locator('.restore-unavailable')).toHaveCount(0);
 await expect(page.locator('#content')).toContainText('真实恢复的正文。');
 expect(await page.evaluate(()=>({id:getActiveTab().id,dirty:getActiveTab().isDirty,mode:state.mode}))).toEqual({id,dirty:false,mode:'file'});
 await page.evaluate(()=>ReadMDSession.flush());await page.reload();await page.evaluate(()=>window.__readmdSessionReady);
 await expect(page.locator('#content')).toContainText('真实恢复的正文。');expect(await page.evaluate(()=>state.tabs.length)).toBe(2);
});

test('explicitly closing an unavailable page keeps only the other saved page after reload',async({page})=>{
 await page.locator('.restore-unavailable button').last().click();
 await expect.poll(()=>page.evaluate(()=>state.tabs.length)).toBe(1);
 await expect(page.locator('#content')).toContainText('Stable original');
 await page.reload();await page.evaluate(()=>window.__readmdSessionReady);
 expect(await page.evaluate(()=>state.tabs.length)).toBe(1);await expect(page.locator('.restore-unavailable')).toHaveCount(0);
 expect(await fs.readFile(stable,'utf8')).toBe('\ufeff# Stable original\r\n\r\n中文原件。🙂\r\n');
});

test('a retry that finishes after explicit close cannot resurrect its page',async({page})=>{
 await fs.writeFile(missing,'# Late response\n',{flag:'wx'});
 let release,entered;const held=new Promise(r=>release=r),requested=new Promise(r=>entered=r);
 await page.route('**/api/file?*',async route=>{if(new URL(route.request().url()).searchParams.get('p')===missing){entered();await held;}await route.continue();});
 await page.locator('[data-restore-retry]').click();await requested;
 await page.locator('.restore-unavailable button').last().click();await expect.poll(()=>page.evaluate(()=>state.tabs.length)).toBe(1);
 release();await page.unrouteAll({behavior:'wait'});
 await expect(page.locator('#content')).toContainText('Stable original');expect(await page.evaluate(()=>state.tabs.length)).toBe(1);
});

test('save-and-close of a dirty background page writes only that file',async({page})=>{
 await page.locator('.restore-unavailable button').last().click();
 await fs.writeFile(missing,'# Background original\n',{flag:'wx'});
 const owner=await page.evaluate(async at=>{
  await loadFile(at);await toggleEdit();cmView.dispatch({changes:{from:cmView.state.doc.length,insert:'\nBackground saved text.\n'}});
  if(!await ReadMDReview.flush())throw Error('Review flush failed');
  syncActiveTabDirty();const tab=getActiveTab();exitEdit();
  await switchTab(state.tabs.find(other=>other!==tab).id);return tab.id;
 },missing);
 const closing=page.evaluate(id=>closeTab(id),owner);
 await page.locator('#close-confirm-save').click();await closing;
 expect(await page.evaluate(()=>state.tabs.length)).toBe(1);
 await expect(page.locator('#content')).toContainText('Stable original');
 expect(await fs.readFile(stable,'utf8')).toBe('\ufeff# Stable original\r\n\r\n中文原件。🙂\r\n');
 expect(await fs.readFile(missing,'utf8')).toBe('# Background original\n\nBackground saved text.\n');
 await page.reload();await page.evaluate(()=>window.__readmdSessionReady);
 expect(await page.evaluate(()=>state.tabs.length)).toBe(1);
});
