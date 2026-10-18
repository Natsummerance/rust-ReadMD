'use strict';
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const {chromium}=require('playwright'),{startNative}=require('./native-session.cjs');
const root=path.resolve(__dirname,'..'),directory=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/math-drafts-native-'));
const profile=path.join(directory,'profile'),docs=path.join(directory,'documents');fs.mkdirSync(docs);
const file=path.join(docs,'Evidence-math-workshop.md');
const source='# 证据与公式工作坊\n\n从原始观察到可检验结论，每一步都要保留依据。\n\n第一次：:math[frac(a, b)]\n\n第二次：:math[frac(a, b)]\n\n## 作者确认\n\n- [x] 保留原稿\n- [ ] 检查公式\n';
fs.writeFileSync(file,source);
const hash=p=>crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const report={platform:'windows-x64',formalCsp:true,syntheticOnly:true,checks:[],errors:[],passed:false,binarySha256:hash(process.env.READMD_BIN),bootSha256:hash(path.join(root,'assets/readmd.boot.js'))};
let native,phase='startup';
const pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function until(fn){const end=Date.now()+45000;while(!await fn()){if(Date.now()>end)throw Error('Timed out: '+phase);await pause(75);}}
async function launch(){
  process.env.READMD_WORKSPACE=docs;native=await startNative(root,profile,chromium,28947);
  native.page.on('pageerror',error=>report.errors.push(error.name));
  await native.page.evaluate(()=>Promise.all([window.__readmdSessionReady,window.__readmdPreferencesReady]));
  await native.page.evaluate(()=>i18n.setLanguage('zh-CN',false));return native.page;
}
async function quit(abrupt=false){
  const child=native.server.child;
  if(abrupt)child.kill();else assert.equal(await native.page.evaluate(()=>ReadMDRecovery.prepareClose()),true);
  await until(()=>Promise.resolve(child.exitCode!==null||child.signalCode!==null));await native.browser.close().catch(()=>{});native=null;
}
async function records(page){return page.evaluate(async file=>{const response=await(await apiFetch('/api/documents/history')).json();return response.entries.filter(e=>e.kind==='math_draft'&&e.path===file);},file);}
(async()=>{try{
  let page=await launch();phase='second identical formula, invalid input and native save';
  await page.evaluate(async file=>{await loadFile(file);await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');},file);
  await until(()=>page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===2));
  await page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"] [data-readmd-math-renderer]').nth(1).click();await until(()=>page.locator('.formula-inline-controls button').first().isEnabled());
  await page.locator('.formula-inline-input').fill('frac(');await page.locator('.formula-inline-input').press('Enter');
  assert.equal(await page.evaluate(()=>ReadMDInlineMath.flush()),true);
  assert.equal(await page.evaluate(()=>saveEdit()),true);assert.equal(fs.readFileSync(file,'utf8'),source);assert.equal((await records(page)).length,1);
  assert.deepEqual(fs.readdirSync(docs),['Evidence-math-workshop.md']);
  report.checks.push('native Ctrl+S saves applied body separately from the invalid second formula; source and directory unchanged');
  phase='normal process quit and restart';await quit();page=await launch();
  await until(()=>page.locator('.formula-inline-input').count().then(n=>n===1));assert.equal(await page.locator('.formula-inline-input').inputValue(),'frac(');
  assert.equal(await page.evaluate(()=>getEditContent()),source);
  report.checks.push('normal native process exit/restart restores the exact second formula and invalid slot without discarding it as ordinary cache');
  phase='confirmed recovery then abrupt process restart';
  await page.locator('.formula-inline-input').fill('sqrt(');await page.locator('.formula-inline-input').press('Enter');
  assert.equal(await page.evaluate(()=>ReadMDInlineMath.flush()),true);await page.evaluate(()=>ReadMDSession.flush());
  await quit(true);page=await launch();
  await until(()=>page.locator('.formula-inline-input').count().then(n=>n===1));assert.equal(await page.locator('.formula-inline-input').inputValue(),'sqrt(');
  assert.equal(fs.readFileSync(file,'utf8'),source);
  report.checks.push('confirmed persisted slot survives abrupt termination of this owned native process');
  await page.screenshot({path:path.join(directory,'restored-draft.png')});
  phase='corrected formula, cleanup and one undo';
  await page.locator('.formula-inline-input').fill('θ');await page.locator('#preview-pane h2').click();
  await until(()=>page.locator('.formula-inline').count().then(n=>n===0));
  const expected=source.replace('第二次：:math[frac(a, b)]','第二次：:math[frac(θ, b)]');
  assert.equal(await page.evaluate(()=>getEditContent()),expected);await until(()=>records(page).then(r=>r.length===0));
  await page.evaluate(()=>cmUndo());assert.equal(await page.evaluate(()=>getEditContent()),source);
  await page.evaluate(()=>cmRedo());assert.equal(await page.evaluate(()=>saveEdit()),true);assert.equal(fs.readFileSync(file,'utf8'),expected);
  report.checks.push('correction applies only the restored second formula; one undo/redo, real save and no resurrected draft');
  await quit();page=await launch();assert.equal(await page.evaluate(()=>state.original),expected);assert.equal((await records(page)).length,0);
  const historical=await page.evaluate(async file=>{
    const list=await(await apiFetch('/api/documents/history')).json(),versions=list.entries.filter(e=>e.kind==='version'&&e.path===file);
    return Promise.all(versions.map(async entry=>(await(await apiFetch('/api/documents/history',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({op:'read',id:entry.id})})).json()).content));
  },file);
  assert.ok(historical.includes(source));report.checks.push('after restart the centralized pre-save version retains the original body; persistent recovery is distinct from the in-memory undo stack');
  assert.deepEqual(fs.readdirSync(docs),['Evidence-math-workshop.md']);assert.deepEqual(report.errors,[]);report.passed=true;
}catch(error){report.failedStage=phase;report.error=error.name;report.message=String(error.message).replaceAll(root,'<workspace>').replaceAll(directory,'<owned-test>').slice(0,800);if(native)await native.page.screenshot({path:path.join(directory,'failure.png')}).catch(()=>{});process.exitCode=1;}
finally{if(native){await native.browser.close().catch(()=>{});native.server.child.kill();}report.recordedAt=new Date().toISOString();fs.writeFileSync(path.join(directory,'report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,failedStage:report.failedStage,report:path.relative(root,path.join(directory,'report.json'))}));}
})();
