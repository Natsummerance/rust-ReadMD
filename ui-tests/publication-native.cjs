'use strict';
// Real Windows WebView2, original CSP, real files and installed TeX engines.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const {chromium}=require('playwright'),{startNative}=require('./native-session.cjs');
const root=path.resolve(__dirname,'..'),data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/publication-native-'));
const docs=path.join(data,'documents');fs.mkdirSync(docs);let native,phase='startup';
const checks=[],report={platform:'windows-x64',nativeCsp:true,synthetic:true,checks};
const delay=ms=>new Promise(r=>setTimeout(r,ms));
async function until(check,ms=60000){const deadline=Date.now()+ms;while(Date.now()<deadline){if(await check())return;await delay(80);}throw Error('Native publication operation timed out');}
(async()=>{
  const file=path.join(docs,'研究.md');const original='---\ntitle: 证据与推理\nbibliography: references.bib\ncsl: apa\nlang: zh-CN\n---\n# 从观察到结论\n\n先核对原始论证 [@kant, p. 42]，再比较解释。\n\n$$E=mc^2$$\n\n| 观察 | 判断 |\n|---|---|\n| 证据 | 保留出处 |\n';
  fs.writeFileSync(file,original);fs.writeFileSync(path.join(docs,'references.bib'),'@book{kant,title={Critique of Pure Reason},author={Kant, Immanuel},year={1781}}');
  try{
    report.binarySha256=crypto.createHash('sha256').update(fs.readFileSync(process.env.READMD_BIN)).digest('hex');process.env.READMD_WORKSPACE=docs;
    native=await startNative(root,data,chromium,28874);const page=native.page;
    await page.evaluate(async file=>{await i18n.setLanguage('zh-CN',false);await loadFile(file);},file);phase='native CSL reader';
    await until(async()=> (await page.locator('#content').innerText()).includes('1781')&&await page.locator('#content mjx-container').count()===1);
    assert.ok((await page.locator('#content').innerText()).includes('参考文献'));assert.equal(await page.locator('#content mjx-container').count(),1);
    await page.evaluate(async()=>{await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');});await until(async()=> (await page.locator('#preview-pane').innerText()).includes('1781'));
    assert.equal(await page.evaluate(()=>getEditContent()),original);checks.push({name:'reader and editor preview use real CSL',passed:true});
    phase='engine controls';await page.evaluate(()=>openExportModal());await until(()=>page.locator('#export-opts [data-k="pdf.engine"]').count());
    await page.locator('.export-customize > summary').click();
    await page.locator('#export-opts .exp-sec').filter({has:page.locator('[data-k="pdf.engine"]')}).locator('.exp-sec-head').click();
    await page.locator('#export-opts [data-k="pdf.engine"]').selectOption('xelatex');
    assert.equal(await page.locator('#export-opts [data-k="page.marginTop"]').count(),0);assert.equal(await page.locator('#export-opts [data-k="tex.margin"]').count(),1);
    await until(()=>page.evaluate(()=>nativeExportPreview.data?.pages>0),120000);assert.equal(await page.evaluate(()=>nativeExportPreview.data.mode),'artifact');
    assert.ok((await page.evaluate(()=>nativeExportPreview.data.text)).includes('1781'));
    checks.push({name:'native XeLaTeX preview and honest engine-specific controls',passed:true});
    phase='preview export identity';const options=await page.evaluate(()=>collectExportOptions()),out=path.join(docs,'研究.pdf');
    const expected=Buffer.from(await page.evaluate(()=>nativeExportPreview.data.pdf),'base64');
    const result=await page.evaluate(async args=>{const r=await apiFetch('/api/export',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(args)});return r.json();},{format:'pdf',content:original,baseDir:docs,suggestedName:await page.evaluate(()=>currentExportName()),options,out_path:out});assert.equal(result.ok,true);assert.deepEqual(fs.readFileSync(out),expected);
    checks.push({name:'export is byte-identical to actual preview artifact',passed:true});await page.evaluate(()=>closeExportModal());
    phase='atomic save and reopen';const edited=original+'\n校订：保留来源与原稿。\n';await page.evaluate(text=>cmView.dispatch({changes:{from:0,to:cmView.state.doc.length,insert:text}}),edited);assert.equal(await page.evaluate(()=>saveEdit()),true);assert.equal(fs.readFileSync(file,'utf8'),edited);await page.evaluate(()=>exitEdit());await page.evaluate(file=>loadFile(file),file);await until(async()=> (await page.locator('#content').innerText()).includes('校订'));assert.equal(fs.readFileSync(file,'utf8'),edited);
    assert.deepEqual(fs.readdirSync(docs).sort(),['references.bib','研究.md','研究.pdf'].sort());checks.push({name:'save and reopen without sibling build products',passed:true});
    phase='compact layout';const cdp=await page.context().newCDPSession(page);
    for(const[width,height]of[[1024,680],[1160,820]]){
      await cdp.send('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false});await page.evaluate(()=>openExportModal());await until(()=>page.locator('#export-opts [data-k="pdf.engine"]').count());
      assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));const button=await page.locator('#export-run').boundingBox();assert.ok(button&&button.y+button.height<=height+1);await page.screenshot({path:path.join(data,'publication-'+width+'.png')});await page.evaluate(()=>closeExportModal());
    }await cdp.detach();checks.push({name:'1024 and 1160 native export layout',passed:true});report.passed=true;
  }catch(error){report.passed=false;report.failedStage=phase;report.error=error.name;report.message=String(error.message).replaceAll(data,'<test-profile>').replaceAll(root,'<workspace>').slice(0,1000);if(native)await native.page.screenshot({path:path.join(data,'failure.png')}).catch(()=>{});throw error;}
  finally{if(native){await native.browser.close().catch(()=>{});native.server.child.kill();}report.recordedAt=new Date().toISOString();fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,checks:checks.length,report:path.relative(root,path.join(data,'report.json')),failedStage:report.failedStage}));}
})().catch(()=>{console.error('Native publication validation failed; stage recorded.');process.exitCode=1;});
