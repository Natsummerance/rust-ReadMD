'use strict';
// A real Windows kernel and full production-CSP frontend, driven headlessly
// through installed Edge. Synthetic review injection here tests persistence,
// not a model; examples/review_ai_probe.rs separately requires a real provider.
const fs = require('node:fs'), path = require('node:path'), net = require('node:net');
const { spawn } = require('node:child_process');
const assert = require('node:assert/strict');
const { chromium } = require('playwright');
const root = path.resolve(__dirname, '..');
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
async function freePort() { const socket=net.createServer();await new Promise(resolve=>socket.listen(0,'127.0.0.1',resolve));const port=socket.address().port;await new Promise(resolve=>socket.close(resolve));return port; }
async function run() {
  assert.ok(process.env.READMD_BIN, 'Explicit prebuilt READMD_BIN is required');
  const base=path.resolve(process.env.READMD_REVIEW_OUTPUT || path.join(root,'.cache/capability-upgrade/v006-batch1-20261027'));
  assert.ok(base.startsWith(path.join(root,'.cache')+path.sep),'Evidence must stay in this owned cache');
  fs.mkdirSync(base,{recursive:true});const output=fs.mkdtempSync(path.join(base,'process-'));
  const profile=path.join(output,'profile'),documents=path.join(output,'documents');fs.mkdirSync(profile);fs.mkdirSync(documents);
  const file=path.join(documents,'Review-study.md'),second=path.join(documents,'Independent.md');
  const original='# Review study 😀\r\n\r\nRepeated sentence.\r\n\r\nRepeated sentence.\r\n\r\nC original.\r\n';
  fs.writeFileSync(file,Buffer.concat([Buffer.from([239,187,191]),Buffer.from(original)]));fs.writeFileSync(second,'# Independent\n\nUnchanged document.\n');
  const untouched=fs.readFileSync(file),otherBefore=fs.readFileSync(second),port=await freePort(),origin='http://127.0.0.1:'+port;
  let processHandle,browser,context,page;
  const evidence={platform:'windows-x64',surface:'installed msedge headless + actual Windows kernel',guiDriven:false,providerInjected:false,checks:[],pageErrors:[]};
  const record=(name,details={})=>{evidence.checks.push({name,...details});console.log(name+': PASS');};
  async function stop() { if(processHandle&&processHandle.exitCode===null){const child=processHandle;child.kill();await Promise.race([new Promise(resolve=>child.once('exit',resolve)),sleep(5000)]);}processHandle=null; }
  async function start() {
    processHandle=spawn(path.resolve(process.env.READMD_BIN),['--no-window','--port',String(port),'--data-dir',profile,'--workspace',documents,'--assets',path.join(root,'assets')],
      {cwd:root,windowsHide:true,stdio:'ignore',env:{...process.env,READMD_DATA_DIR:profile}});
    const deadline=Date.now()+30000;
    while(Date.now()<deadline){if(processHandle.exitCode!==null)throw Error('Owned kernel exited before readiness');try{if((await fetch(origin)).ok)return;}catch{}await sleep(50);}throw Error('Owned kernel startup deadline');
  }
  // Playwright waitForFunction internally uses eval, which the production
  // CSP forbids. Evaluate the supplied function directly; never bypass CSP.
  async function waitFor(predicate,arg){const deadline=Date.now()+30000;while(Date.now()<deadline){if(await page.evaluate(predicate,arg))return;await sleep(50);}throw Error('Production-CSP predicate deadline: '+String(predicate).slice(0,160));}
  async function ready(){await page.goto(origin);await page.evaluate(()=>window.__readmdSessionReady);await waitFor(()=>window.__readmdAppReady&&Boolean(window.ReadMDReview));}
  async function surface(storageState){
    context=await browser.newContext({viewport:{width:1160,height:820},locale:'zh-CN',bypassCSP:false,storageState});
    page=await context.newPage();page.on('pageerror',error=>evidence.pageErrors.push(error.message.slice(0,180)));
    await page.addInitScript(()=>localStorage.setItem('readmd_language','zh-CN'));await ready();
  }
  async function restart(){
    // Preserve only this isolated fixture origin's browser session in memory.
    // Close the old page before starting a new server so old timers/pagehide
    // cannot write through a fresh backend during crash recovery observation.
    const storageState=await context.storageState();await stop();await context.close();await start();await surface(storageState);
  }
  async function api(route,body){return page.evaluate(async({route,body})=>{const r=await apiFetch(route,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});return {status:r.status,data:await r.json()};},{route,body});}
  try {
    await start();browser=await chromium.launch({channel:'msedge',headless:true});
    await surface();
    await page.evaluate(file=>loadFile(file),file);
    assert.equal(await page.evaluate(()=>ReadMDReview.flushAll()),true);
    assert.equal(fs.existsSync(path.join(profile,'review.sqlite3')),false);assert.deepEqual(fs.readFileSync(file),untouched);
    await page.locator('#btn-edit').click();await waitFor(()=>state.editing&&window.cmView);
    await page.evaluate(()=>ReadMDReview.flush());assert.deepEqual(fs.readFileSync(file),untouched);
    const ids=await page.evaluate(async()=>{
      const tab=getActiveTab(),stamp=await ReadMDReview.capture(tab),source=cmView.state.doc.toString();
      const first=source.indexOf('Repeated sentence.'),second=source.indexOf('Repeated sentence.',first+1);
      const result=await ReadMDReview.propose([{from:first,to:first+18,insert:'AI improved paragraph.'},{from:second,to:second+18,insert:'AI improved paragraph.'}],stamp,tab);
      return result.changes.map(change=>change.id);
    });
    assert.equal(ids.length,2);assert.notEqual(ids[0],ids[1]);
    // Measure with real review widgets present; an empty list on another file
    // must not accidentally certify the 44px controls or small-window layout.
    for(const size of [{width:1024,height:680},{width:1160,height:820}]){
      await page.setViewportSize(size);await page.evaluate(()=>new Promise(requestAnimationFrame));
      const metrics=await page.evaluate(()=>({overflow:document.documentElement.scrollWidth>innerWidth+1,buttons:[...document.querySelectorAll('.review-action')].filter(el=>el.getClientRects().length).map(el=>({width:el.getBoundingClientRect().width,height:el.getBoundingClientRect().height}))}));
      assert.equal(metrics.overflow,false);assert.ok(metrics.buttons.length>=4);assert.ok(metrics.buttons.every(button=>button.width>=44&&button.height>=44));
      await page.screenshot({path:path.join(output,'pending-'+size.width+'x'+size.height+'.png')});
      evidence.checks.push({name:'pending-review-layout',...size,controls:metrics.buttons.length,minControlWidth:Math.min(...metrics.buttons.map(b=>b.width)),minControlHeight:Math.min(...metrics.buttons.map(b=>b.height))});
    }
    await page.evaluate(()=>{const source=cmView.state.doc.toString(),from=source.indexOf('C original.');cmView.dispatch({changes:{from,to:from+11,insert:'C manually edited.'}});});
    await page.evaluate(()=>ReadMDReview.flush());
    // The actual contextual control accepts A; acceptance never duplicates it.
    await page.locator('[data-review-action="accept"][data-review-change-id="'+ids[0]+'"]').click({force:true});
    await waitFor(id=>ReadMDReview.snapshot()?.changes.find(c=>c.id===id)?.decision==='accepted',ids[0]);
    const expected='# Review study 😀\n\nAI improved paragraph.\n\nAI improved paragraph.\n\nC manually edited.\n';
    assert.equal(await page.evaluate(()=>cmView.state.doc.toString()),expected);
    await page.keyboard.press('Control+s');await waitFor(()=>!getActiveTab().isDirty&&!ReadMDReview.busy());
    assert.deepEqual(fs.readFileSync(file),Buffer.concat([Buffer.from([239,187,191]),Buffer.from(expected.replace(/\n/g,'\r\n'))]));
    record('applied-A-B-manual-C-accept-A-CtrlS',{blocks:2,bom:true,crlf:true,pendingAfterSave:true});
    await page.evaluate(()=>ReadMDSession.flush());await restart();
    assert.equal(await page.evaluate(()=>getActiveTab()?.path),file);
    assert.equal(await page.locator('#status-review').isVisible(),true);
    await page.locator('#status-review').click();await waitFor(()=>state.editing&&window.cmView);
    evidence.phase='withdraw-B-after-restart';
    await page.locator('[data-review-action="withdraw"][data-review-change-id="'+ids[1]+'"]').click({force:true});
    const onlyB='# Review study 😀\n\nAI improved paragraph.\n\nRepeated sentence.\n\nC manually edited.\n';
    await waitFor(expected=>cmView.state.doc.toString()===expected,onlyB);
    assert.equal(await page.evaluate(()=>ReadMDReview.snapshot().changes[0].decision),'accepted');
    evidence.phase='undo-withdrawal';
    await page.keyboard.press('Control+z');await waitFor(expected=>cmView.state.doc.toString()===expected,expected);
    evidence.phase='redo-withdrawal';
    // A physical shifted letter has key="Z". Playwright's lower-case literal
    // synthesizes key="z" with Shift and invokes CM's unshifted fallback.
    await page.keyboard.press('Control+Shift+Z');await waitFor(expected=>cmView.state.doc.toString()===expected,onlyB);
    record('restart-pending-block-withdraw-undo-redo',{acceptedA:true,manualC:true});
    await page.keyboard.press('Control+s');await waitFor(()=>!getActiveTab().isDirty&&!ReadMDReview.busy());
    const savedState=await page.evaluate(()=>ReadMDReview.snapshot());
    const forbidden=await api('/api/save',{path:file,content:'Untracked overwrite',encoding:'utf-8'});assert.equal(forbidden.status,409);assert.equal(forbidden.data.error_code,'review_save_conflict');
    assert.equal((await api('/api/review',{action:'commit_save',document_id:savedState.document_id})).status,422);
    record('legacy-and-private-commit-bypass-rejected');
    // A durable but unsaved edit survives an abrupt process stop, even when
    // session metadata claims the document was clean before the last keystroke.
    await page.evaluate(()=>{const source=cmView.state.doc.toString();cmView.dispatch({changes:{from:source.length,insert:'\nCrash recovery draft.\n'}});});
    await page.evaluate(()=>ReadMDReview.flush());
    await restart();
    const recovered=onlyB+'\nCrash recovery draft.\n';
    await waitFor(expected=>getActiveTab()?.content.replace(/\r\n/g,'\n')===expected,recovered);
    assert.equal(await page.evaluate(()=>getActiveTab().isDirty),true);record('abrupt-kernel-restart-recovers-unsaved-review-body');
    if(!await page.evaluate(()=>state.editing))await page.locator('#btn-edit').click();
    await waitFor(()=>state.editing&&window.cmView);
    const discarded=await page.evaluate(()=>ReadMDRecovery.discard(getActiveTab()));assert.equal(discarded,true);
    assert.equal(await page.evaluate(()=>cmView.state.doc.toString()),onlyB);
    assert.equal(await page.evaluate(()=>ReadMDReview.snapshot().changes[0].decision),'accepted');record('discard-restores-saved-checkpoint-with-review-decisions');
    // Inject a response failure only at the editor transport. The independent
    // generic history really writes its newer draft and causal watermark, while
    // SQLite remains at the older acknowledged review body until restart.
    evidence.phase='unacknowledged-draft-recovery';
    let faulted=false;
    const fault=async route=>{const body=route.request().postDataJSON();if(!faulted&&body?.action==='edit'){faulted=true;await route.fulfill({status:503,contentType:'application/json',body:JSON.stringify({error:'owned fault injection',error_code:'review_persistence_failed'})});}else await route.continue();};
    await page.route('**/api/review',fault);
    await page.evaluate(()=>{const end=cmView.state.doc.length;cmView.dispatch({changes:{from:end,insert:'\nUnacknowledged hand draft.\n'}});});
    assert.equal(await page.evaluate(()=>ReadMDReview.flush()),false);assert.equal(faulted,true);
    await page.evaluate(async()=>{await ReadMDRecovery.flush();ReadMDSession.persist();});
    await restart();
    const unacknowledged=onlyB+'\nUnacknowledged hand draft.\n';
    await waitFor(expected=>getActiveTab()?.content.replace(/\r\n/g,'\n')===expected,unacknowledged);
    assert.equal(await page.evaluate(()=>ReadMDReview.flush()),true);
    assert.equal(await page.evaluate(()=>ReadMDReview.snapshot().content),unacknowledged);
    assert.equal(await page.evaluate(()=>ReadMDReview.snapshot().changes.find(c=>c.decision==='accepted')?.replacement),'AI improved paragraph.');
    assert.equal(await page.evaluate(()=>ReadMDRecovery.discard(getActiveTab())),true);
    record('newer-generic-draft-reconciles-after-transport-failure-and-real-restart',{faultInjected:true});
    evidence.phase='older-generic-draft-preserved';
    await page.evaluate(async()=>{const end=cmView.state.doc.length;cmView.dispatch({changes:{from:end,insert:'\nOlder generic checkpoint.\n'}});await ReadMDReview.flush();await ReadMDRecovery.flush();ReadMDSession.persist();});
    await page.evaluate(async()=>{const end=cmView.state.doc.length;cmView.dispatch({changes:{from:end,insert:'\nNewer acknowledged text.\n'}});await ReadMDReview.flush();ReadMDSession.persist();});
    await restart();
    const newer=onlyB+'\nOlder generic checkpoint.\n\nNewer acknowledged text.\n';
    await waitFor(expected=>getActiveTab()?.content.replace(/\r\n/g,'\n')===expected,newer);
    const history=await page.evaluate(async()=>{const response=await apiFetch('/api/documents/history');return response.json();});
    assert.ok(history.entries.some(entry=>entry.kind==='checkpoint'&&entry.reason==='review_recovery_conflict'));
    assert.equal(await page.evaluate(()=>ReadMDRecovery.discard(getActiveTab())),true);
    record('older-generic-body-checkpointed-before-durable-body-hydration');
    await page.evaluate(()=>exitEdit());await page.evaluate(second=>loadFile(second),second);
    const b=await api('/api/review',{action:'open',path:file});assert.equal(b.status,200);
    const withdrawn=await api('/api/review',{action:'resolve',document_id:b.data.document_id,generation:b.data.generation,review_revision:b.data.review_revision,call_id:'background-withdraw-a',ids:[ids[0]],decision:'withdraw'});
    assert.equal(withdrawn.status,200);assert.equal(await page.evaluate(()=>getActiveTab().path),second);assert.deepEqual(fs.readFileSync(second),otherBefore);
    record('background-original-document-receipt-does-not-touch-new-active-file');
    assert.deepEqual(evidence.pageErrors,[]);record('production-CSP-no-page-errors-and-small-window-layout');
    evidence.passed=true;
  }catch(error){evidence.passed=false;evidence.failure=error.message.slice(0,500);if(page)evidence.ownedFixtureState=await page.evaluate(()=>({content:window.cmView?.state.doc.toString(),snapshot:window.ReadMDReview?.snapshot(),focus:document.activeElement?.className})).catch(()=>null);process.exitCode=1;console.error(evidence.failure);}
  finally {if(page)await page.screenshot({path:path.join(output,'final.png')}).catch(()=>{});if(browser)await browser.close().catch(()=>{});await stop();fs.writeFileSync(path.join(output,'report.json'),JSON.stringify(evidence,null,2));console.log('review process report: '+path.relative(root,path.join(output,'report.json')));}
}
run().catch(error=>{console.error(error.message);process.exitCode=1;});
