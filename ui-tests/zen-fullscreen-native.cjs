'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),net=require('node:net'),assert=require('node:assert/strict'),{execFileSync}=require('node:child_process');
const {chromium,expect}=require('@playwright/test'),{startNative}=require('./native-session.cjs');
const root=path.resolve(__dirname,'..'),dir=fs.mkdtempSync(path.join(root,'.cache/zen-fullscreen-20261017/native-'));
const sha=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
async function main(){
  const report={passed:false,checks:[],binarySha256:sha(process.env.READMD_BIN),bootSha256:sha(path.join(root,'assets/readmd.boot.js')),panelsSha256:sha(path.join(root,'assets/css/panels.css')),nativeCsp:true};let session;
  const server=net.createServer();await new Promise(ok=>server.listen(0,'127.0.0.1',ok));const port=server.address().port;await new Promise(ok=>server.close(ok));
  async function check(name,fn){await fn();report.checks.push({name,passed:true});}
  try{
    session=await startNative(root,dir,chromium,port);const page=session.page;
    await page.evaluate(()=>Promise.all([__readmdSessionReady,__readmdPreferencesReady]));
    await page.evaluate(()=>{
      window.__zenTestEvents=[];
      const record=event=>{
        window.__zenTestEvents.push({at:Math.round(performance.now()),event:event.type,target:event.target?.id||event.target?.tagName,key:event.key,button:event.button,full:!!window.__readmdNativeFullscreen,zen:document.body.classList.contains('zen-mode')});
        if(window.__zenTestEvents.length>60)window.__zenTestEvents.shift();
      };
      for(const type of ['pointerdown','pointerup','click','contextmenu','keydown'])window.addEventListener(type,record,true);
      new MutationObserver(()=>record({type:'body-class',target:document.body})).observe(document.body,{attributes:true,attributeFilter:['class']});
    });
    const csp=fs.readFileSync(path.join(root,'assets/index.html'),'utf8').match(/<meta http-equiv="Content-Security-Policy" content="([^"]+)"/)[1];assert.equal(await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content'),csp);
    const input=path.join(dir,'Focus.md'),body='# Focus\r\n\r\nThe original document stays unchanged.\r\n';fs.writeFileSync(input,body);await page.evaluate(file=>loadFile(file),input);
    const geometry=()=>JSON.parse(execFileSync(process.env.READMD_QA_POWERSHELL||'pwsh.exe',['-NoProfile','-NonInteractive','-File',path.join(root,'tools/test/native-window-geometry.ps1'),'-OwnerProcessId',String(session.server.child.pid)],{encoding:'utf8',windowsHide:true}));
    const initial=geometry();report.initialGeometry=initial;
    async function mode(full,zen){
      await expect.poll(()=>page.evaluate(()=>({full:!!window.__readmdNativeFullscreen,zen:document.body.classList.contains('zen-mode')}))).toEqual({full,zen});
      // Wait for the actual native surface, not only the page's state mirror.
      await expect.poll(()=>geometry().window).toEqual(full?initial.monitor:initial.window);
      // CSS viewport pixels round at fractional Windows DPI; native bounds
      // above remain an exact comparison in physical monitor pixels.
      if(full)await expect.poll(()=>page.evaluate(rect=>Math.abs(innerWidth*devicePixelRatio-(rect.right-rect.left))<=2&&Math.abs(innerHeight*devicePixelRatio-(rect.bottom-rect.top))<=2,initial.monitor)).toBe(true);
    }
    async function reveal(){await page.mouse.move(600,160);await page.mouse.move(200,3);await expect.poll(()=>page.locator('#btn-zen').evaluate(e=>{const r=e.getBoundingClientRect();return r.height>=44&&r.top>=0;})).toBe(true);}
    await check('left click enters only Zen; Escape restores ordinary reading',async()=>{
      await page.locator('#btn-zen').click();await mode(false,true);assert.deepEqual(geometry().window,initial.window);await page.keyboard.press('Escape');await mode(false,false);
    });
    await check('right click enters real monitor fullscreen with Zen',async()=>{
      await page.locator('#btn-zen').click({button:'right'});await mode(true,true);await expect.poll(()=>{const g=geometry();return JSON.stringify(g.window)===JSON.stringify(g.monitor);}).toBe(true);report.fullscreenGeometry=geometry();
    });
    await check('left click restores chrome without leaving real fullscreen; right click exits both',async()=>{
      await reveal();await page.locator('#btn-zen').click();await mode(true,false);assert.deepEqual(geometry().window,report.fullscreenGeometry.window);
      await page.screenshot({path:path.join(dir,'pure-fullscreen.png')});
      await page.locator('#btn-zen').click({button:'right'});await mode(false,false);await expect.poll(()=>geometry().window).toEqual(initial.window);
    });
    await check('right click also exits Zen and fullscreen together while Zen remains enabled',async()=>{
      await page.locator('#btn-zen').click({button:'right'});await mode(true,true);await reveal();await page.locator('#btn-zen').click({button:'right'});await mode(false,false);
    });
    await check('F11 toggles fullscreen and ignores repeat; Escape exits both',async()=>{
      await page.keyboard.down('F11');await mode(true,true);await page.keyboard.down('F11');await mode(true,true);await page.keyboard.up('F11');await page.keyboard.press('Escape');await mode(false,false);
      await page.keyboard.press('F11');await mode(true,true);await page.keyboard.press('F11');await mode(false,false);
    });
    await check('Escape exits pure fullscreen without closing the editor or changing content',async()=>{
      await page.evaluate(()=>toggleEdit());const before=await page.evaluate(()=>getEditContent());await page.keyboard.press('F11');await mode(true,true);await reveal();await page.locator('#btn-zen').click();await mode(true,false);await page.keyboard.press('Escape');await mode(false,false);assert.equal(await page.evaluate(()=>state.editing),true);assert.equal(await page.evaluate(()=>getEditContent()),before);await expect(page.locator('#close-confirm-modal')).toBeHidden();
    });
    await check('fullscreen Escape preserves an open export dialog and original file',async()=>{
      await page.keyboard.press('F11');await mode(true,true);await page.evaluate(()=>openExportModal());await page.keyboard.press('Escape');await mode(false,false);await expect(page.locator('#export-modal')).toBeVisible();await page.evaluate(()=>closeExportModal());assert.equal(fs.readFileSync(input,'utf8'),body);await expect.poll(()=>geometry().window).toEqual(initial.window);
    });
    report.passed=true;
  }catch(error){report.error=String(error.stack||error);if(session){report.events=await session.page.evaluate(()=>window.__zenTestEvents).catch(()=>[]);await session.page.screenshot({path:path.join(dir,'failure.png')}).catch(()=>{});}throw error;}finally{
    if(session){await session.page.evaluate(()=>window.pywebview.api.request_quit()).catch(()=>{});await Promise.race([new Promise(ok=>session.server.child.exitCode!==null?ok():session.server.child.once('exit',ok)),new Promise((_,bad)=>setTimeout(()=>bad(Error('Owned host did not exit')),10000))]).catch(error=>{session.server.child.kill();report.shutdownError=String(error);report.passed=false;});report.hostStopped=session.server.child.exitCode!==null;await session.browser.close().catch(()=>{});}
    fs.writeFileSync(path.join(dir,'report.json'),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,report:path.relative(root,path.join(dir,'report.json'))}));
  }
}
main().catch(error=>{console.error(error.stack||error);process.exitCode=1;});
