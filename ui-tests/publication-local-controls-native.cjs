'use strict';
// Focused language / final chrome review of a previously verified native profile.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),{chromium}=require('playwright'),{startNative}=require('./native-session.cjs');
const root=path.resolve(__dirname,'..'),profile=path.resolve(process.env.READMD_LOCAL_PROFILE||'');assert.ok(profile.startsWith(path.join(root,'.cache','capability-upgrade','publication-local-native-')));
const hash=b=>crypto.createHash('sha256').update(b).digest('hex'),report={passed:false,platform:'windows-x64',checks:[],nativeCsp:true,synthetic:true};let native;
(async()=>{try{native=await startNative(root,profile,chromium,28989);const page=native.page;await page.evaluate(()=>window.__readmdSessionReady);await page.evaluate(async file=>{await loadFile(file);if(!state.editing)await toggleEdit();},path.join(profile,'documents','Local-layout.md'));
  for(const[width,height,locale]of[[1024,680,'zh-CN'],[1160,820,'de'],[1024,680,'ar']]){
    await page.setViewportSize({width,height});await page.evaluate(async locale=>{await i18n.setLanguage(locale,false);cmView.dispatch({selection:{anchor:getEditContent().lastIndexOf('SAME_TARGET')}});await ReadMDPublication.openStyle();},locale);
    await page.locator('#publication-style-modal summary').click();assert.equal(await page.evaluate(()=>{const e=document.querySelector('.publication-style-box'),r=document.querySelector('#publication-style-modal button[type=submit]').getBoundingClientRect();return e.scrollWidth<=e.clientWidth+2&&r.bottom<=innerHeight;}),true);
    if(locale==='de')assert.equal(await page.locator('[data-local="text.bold"]').locator('..').locator('span').textContent(),'Fett');
    assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),true);await page.screenshot({path:path.join(profile,`review-controls-${locale}.png`)});await page.keyboard.press('Escape');report.checks.push(locale+' controls / final toolbar / fixed action visibility');
  }
  report.passed=true;report.binarySha256=hash(fs.readFileSync(process.env.READMD_BIN));report.bootSha256=hash(fs.readFileSync(path.join(root,'assets/readmd.boot.js')));report.baseReportSha256=hash(fs.readFileSync(path.join(profile,'report.json')));
}finally{if(native){await native.browser.close().catch(()=>{});native.server.child.kill();}report.recordedAt=new Date().toISOString();fs.writeFileSync(path.join(profile,'controls-review.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,checks:report.checks.length}));}})().catch(e=>{process.exitCode=1;console.error(e.message);});
