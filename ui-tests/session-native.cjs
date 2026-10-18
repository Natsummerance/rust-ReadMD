'use strict';
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const { chromium } = require('playwright'), { startNative } = require('./native-session.cjs');
const root = path.resolve(__dirname, '..'), work = path.join(root, '.cache/v005-community');
const delay = ms => new Promise(r => setTimeout(r, ms));
async function until(fn) { const end=Date.now()+30000; while(Date.now()<end) { if(await fn())return; await delay(80); } throw Error('Native session did not settle'); }
(async () => {
  const data = fs.mkdtempSync(path.join(work, 'session-native-')), docs = path.join(data, 'documents');
  fs.mkdirSync(path.join(docs, 'notes'), { recursive:true });
  const a = path.join(docs, '哲学阅读研究手册.md'), b = path.join(docs, '认识与经验.md');
  for(const file of ['哲学阅读研究手册.md','references.bib','reading-map.png','argument-dashboard.png']) fs.copyFileSync(path.join(root,'showcase/materials',file),path.join(docs,file));
  fs.copyFileSync(path.join(root,'showcase/materials/notes/认识与经验.md'),b);
  process.env.READMD_WORKSPACE=docs;
  let native, capture;
  const report={nativeCsp:true,synthetic:true,scenarios:[],errors:[],profile:path.relative(root,data).replaceAll('\\','/')};
  const check=(name,details={})=>{report.scenarios.push({name,passed:true,...details});console.log(name+' passed');};
  async function launch() {
    native=await startNative(root,data,chromium,28864);
    native.page.on('pageerror',e=>report.errors.push(e.message));
    await until(()=>native.page.evaluate(()=>Boolean(window.ReadMDSession)));
    await native.page.evaluate(()=>window.__readmdSessionReady);
    await native.page.evaluate(async()=>{await i18n.setLanguage('zh-CN',false);state.theme='light';applySettings();});
    if(capture)await capture.attach(native.page);
    return native.page;
  }
  async function quit() {
    if(capture)await capture.pause();
    const child=native.server.child;
    await native.page.evaluate(()=>ReadMDRecovery.prepareClose()).catch(()=>{});
    await until(()=>Promise.resolve(child.exitCode!==null));
    await native.browser.close().catch(()=>{}); native=null;
  }
  try {
    let page=await launch();
    await page.evaluate(async({a,b})=>{await loadFile(a);await loadFile(b);await ReadMDSession.flush();},{a,b});
    const before=await page.evaluate(()=>({names:state.tabs.map(t=>t.name),active:getActiveTab().name}));
    if(process.env.READMD_RECORD_SESSION==='1') {
      capture=await require(path.join(root,'showcase/updates/v0.0.5/session-workspace/record.cjs'))();
      await capture.attach(page);capture.mark('两篇未关闭的研究文档');await delay(1500);
    }
    await quit(); page=await launch();
    assert.deepEqual(await page.evaluate(()=>({names:state.tabs.map(t=>t.name),active:getActiveTab().name})),before);
    check('real-process-restart-restores-ordered-pages-and-active-document',{pages:2});
    if(capture){capture.mark('重新启动，恢复原有两页和当前页');await delay(1700);}
    await page.locator('.tab-item').filter({hasText:'哲学阅读研究手册.md'}).click();
    await until(()=>page.locator('#content').innerText().then(s=>s.includes('26.87%')));
    if(capture){capture.mark('切回保留的研究手册');await delay(1500);}
    await page.locator('#titlebar-home').click();
    assert.equal(await page.evaluate(()=>state.tabs.length),2);
    if(capture){capture.mark('点击 LOGO 回主页，未关闭页面仍保留');await delay(900);}
    await page.locator('.tab-item').filter({hasText:'哲学阅读研究手册.md'}).click();
    await page.evaluate(()=>closeAllTabs()); await page.evaluate(()=>ReadMDSession.flush());
    assert.equal(await page.evaluate(()=>state.mode),'welcome');
    if(capture){capture.mark('关闭全部页面，显示主页');await delay(1200);}
    await quit();page=await launch();
    assert.deepEqual(await page.evaluate(()=>({tabs:state.tabs.length,mode:state.mode,file:state.file})),{tabs:0,mode:'welcome',file:null});
    check('real-process-restart-after-close-all-shows-home');
    if(capture){capture.mark('再次启动，保持主页');await delay(1400);await capture.finish();capture=null;}
    // Also exercise the persisted native session/draft store across a process
    // boundary, not just same-origin browser reloads.
    await page.evaluate(async a=>{await loadFile(a);await toggleEdit();setPvLayout('none');cmView.dispatch({changes:{from:cmView.state.doc.length,insert:'\n原生重启后恢复的草稿。'}});await ReadMDSession.flush();},a);
    // Simulate abrupt termination only of this test's owned process.
    native.server.child.kill();await native.browser.close().catch(()=>{});native=null;await delay(500);
    page=await launch();
    assert.ok((await page.evaluate(()=>getEditContent())).includes('原生重启后恢复的草稿'));
    assert.equal(fs.readFileSync(a,'utf8'),fs.readFileSync(path.join(root,'showcase/materials/哲学阅读研究手册.md'),'utf8'));
    check('native-abrupt-restart-recovers-draft-without-touching-original');
    await page.evaluate(()=>{exitEdit();getActiveTab().isDirty=false;});
    await page.evaluate(()=>closeAllTabs());
    assert.equal(report.errors.length,0,JSON.stringify(report.errors));report.passed=true;
  } catch(error) { report.passed=false;report.failure=error.message;throw error; }
  finally {
    if(capture)await capture.pause().catch(()=>{});
    if(native){await native.browser.close().catch(()=>{});native.server.child.kill();}
    fs.writeFileSync(path.join(work,'session-native-report.json'),JSON.stringify(report,null,2));
  }
})().catch(e=>{console.error(e.stack);process.exitCode=1;});
