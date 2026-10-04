'use strict';
// Native titlebar, real Win32 tray callbacks and resident activation; isolated fixtures only.
const assert=require('node:assert/strict'),fs=require('node:fs'),os=require('node:os'),path=require('node:path');
const {spawn}=require('node:child_process');const {chromium}=require('playwright');
const root=path.resolve(__dirname,'..'),delay=ms=>new Promise(r=>setTimeout(r,ms));
async function until(fn,ms=30000){const end=Date.now()+ms;while(Date.now()<end){const value=await fn();if(value)return value;await delay(100);}throw Error('Native fixture timed out');}
function ps(file,request){return new Promise((resolve,reject)=>{const process=spawn('powershell.exe',['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',file],{windowsHide:true,stdio:['pipe','pipe','pipe']});let output='',error='';process.stdout.on('data',b=>output+=b);process.stderr.on('data',b=>error+=b);process.on('error',reject);process.on('exit',code=>code?reject(Error(error.slice(-700))):resolve(JSON.parse(output.trim())));process.stdin.end(JSON.stringify(request));});}
(async()=>{
 const data=fs.mkdtempSync(path.join(os.tmpdir(),'readmd-window-smoke-')),port=28656;let child,browser;
 const control=(action)=>ps(path.join(__dirname,'window-native-control.ps1'),{pid:child.pid,action});
 try {
  fs.writeFileSync(path.join(data,'settings.json'),JSON.stringify({lang:'en',closeToTray:true}));
  fs.writeFileSync(path.join(data,'update-result.json'),JSON.stringify({ok:false,error_code:'update_replace_failed'}));
  const {startNative}=await import('../showcase/scripts/native-session.mjs');
  const session=await startNative(root,data,chromium,port);child=session.server.child;browser=session.browser;const page=session.page;
  await page.waitForFunction(()=>document.body.classList.contains('custom-titlebar')&&window.ReadMDRecovery);
  await page.waitForFunction(()=>!document.getElementById('btn-close-to-tray').disabled);
  assert.equal((await page.evaluate(()=>py.get_app_info())).last_update_error,'update_replace_failed');
  await until(()=>page.locator('#toast').textContent().then(text=>/previous version|原版本/.test(text)));
  assert.equal(fs.existsSync(path.join(data,'update-result.json')),false);
  const initial = await control('state');
  assert.ok(initial.clientTop <= 8 * initial.dpi / 96, 'the actual client area contains no native caption');
  // OS maximize and restore, including icon/accessible-name synchronization.
  await page.locator('#window-maximize').click();await until(async()=>(await control('state')).maximized);
  await page.waitForFunction(()=>document.getElementById('window-maximize').getAttribute('aria-pressed')==='true');
  await page.locator('#window-maximize').click();await until(async()=>!(await control('state')).maximized);
  await page.locator('#window-minimize').click();await until(async()=>(await control('state')).minimized);
  await control('tray-show');await until(async()=>{const s=await control('state');return s.visible&&!s.minimized;});
  // Real physical dragging stays confined to this fixture's titlebar.
  let before=await control('state');let bounds=await page.locator('#window-drag-region').boundingBox();const ratio=before.dpi/96;
  const x=Math.round(before.x+(bounds.x+40)*ratio),y=Math.round(before.y+(bounds.y+24)*ratio);
  await ps(path.join(root,'showcase/scripts/native-control.ps1'),{pid:child.pid,action:'focus'});
  await ps(path.join(root,'showcase/scripts/native-control.ps1'),{pid:child.pid,action:'mouse',x,y,drag:true,toX:x+80,toY:y+40});
  let after=await control('state');assert.ok(Math.abs(after.x-before.x)>=40,'native titlebar drag moves the actual window');
  await page.evaluate(async()=>{await renderVirtual('clipboard','Resident-demo.md','','# Kept draft\n',[]);await toggleEdit();});
  await page.locator('#window-close').click();await until(async()=>!(await control('state')).visible);
  assert.equal(child.exitCode,null);assert.equal(await page.locator('#close-confirm-modal').isVisible(),false);
  await control('tray-show');await until(async()=>(await control('state')).visible);assert.equal(await page.evaluate(()=>getEditContent()),'# Kept draft\n');
  // Create a control descriptor for this custom-port fixture; never touches a real user's instance.
  fs.writeFileSync(path.join(data,'instance.json'),JSON.stringify({port,token:'fixture-resident-token',pid:child.pid}));
  await control('close');await until(async()=>!(await control('state')).visible);
  const second=spawn(process.env.READMD_BIN,['--data-dir',data,'--assets',path.join(root,'assets')],{windowsHide:true,stdio:'ignore'});
  await until(()=>second.exitCode!==null);assert.equal(second.exitCode,0);await until(async()=>(await control('state')).visible);
  const file=path.join(data,'Opened-from-shell.md');fs.writeFileSync(file,'# Shell handoff\n');
  await page.evaluate(()=>{syncActiveTabDirty();exitEdit();});
  // Resident activation must not create a second process/window.
  const fileLaunch=spawn(process.env.READMD_BIN,[file,'--data-dir',data,'--assets',path.join(root,'assets')],{windowsHide:true,stdio:'ignore'});
  await until(()=>fileLaunch.exitCode!==null);assert.equal(fileLaunch.exitCode,0);
  await until(()=>page.evaluate(()=>state.tabs.some(t=>t.name==='Opened-from-shell.md')));
  const associations=await page.evaluate(async()=>(await(await apiFetch('/api/system/assoc',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({op:'status'})})).json()));
  assert.equal(associations.extensions.length,4);assert.equal(typeof associations.all_default,'boolean');
  // Quit is distinct from hide, and cancellation preserves every dirty tab.
  await page.evaluate(()=>{void py.request_quit();});await page.locator('#close-confirm-modal').waitFor();await page.locator('#close-confirm-cancel').click();assert.equal(child.exitCode,null);
  await page.evaluate(()=>{void py.request_quit();});await page.locator('#close-confirm-modal').waitFor();await page.locator('#close-confirm-discard').click();
  await until(()=>child.exitCode!==null,20000);assert.equal(child.exitCode,0);
  await browser.close();browser=null;
  // Disabling residence makes an unmodified editor close immediately, without a dialog.
  const cleanData=path.join(data,'clean-session');fs.mkdirSync(cleanData);
  fs.writeFileSync(path.join(cleanData,'settings.json'),JSON.stringify({lang:'en',closeToTray:false}));
  const clean=await startNative(root,cleanData,chromium,port);child=clean.server.child;browser=clean.browser;
  await clean.page.waitForFunction(()=>window.__readmdAppReady && window.ReadMDRecovery);
  await clean.page.evaluate(async file=>{await loadFile(file);await toggleEdit();state.closeToTray=false;syncWindowPreferences();},file);
  assert.equal(await clean.page.evaluate(()=>hasUnsavedEditorChanges()),false);
  await clean.page.locator('#window-close').click();await until(()=>child.exitCode!==null,20000);assert.equal(child.exitCode,0);
  console.log('PASS: custom native caption, maximize/restore/minimize, physical drag, tray hide/show, hot launch, shell file handoff, read-only association status, dirty quit cancellation and recovery.');
 }finally{
  if(browser)await browser.close().catch(()=>{});if(child&&child.exitCode===null){child.kill();await until(()=>child.exitCode!==null).catch(()=>{});}
  await require('./cleanup-native-fixture.cjs')(data);
 }
})().catch(e=>{console.error(e);process.exitCode=1;});
