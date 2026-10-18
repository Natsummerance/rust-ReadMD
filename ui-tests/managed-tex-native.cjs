'use strict';
// Production WebView2/CSP and services with an unpublished, isolated QA lock.
// The runtime is real; redistribution approval is explicitly NOT implied.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const playwright=require('playwright'),{chromium}=playwright,{startNative}=require('./native-session.cjs');
const root=path.resolve(__dirname,'..'),fixture=path.resolve(process.env.READMD_TEX_QA_FIXTURE||'');
const receipt=JSON.parse(fs.readFileSync(path.join(fixture,'receipt.json')));
assert(receipt.qaOnly&&receipt.redistributionApproved===false&&!receipt.published);
const data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/tex-managed-native-'));
const docs=path.join(data,'documents');fs.mkdirSync(docs);
const hash=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const report={passed:false,platform:'windows-x64',nativeCsp:true,system32OnlyChildPath:true,qaOnlyRuntime:true,
  redistributionApproved:false,formalPackAcceptance:false,checks:[],timings:[],exports:{}};
const original='---\ntitle: 观察、证据与判断\nbibliography: references.bib\ncsl: apa\nlang: zh-CN\n---\n# 从观察到结论\n\n> 每项判断都需要能够回到原始材料。\n\n先核对研究方法 [@kant, p. 42]，然后比较解释。\n\n## 数据与不确定性\n\n| 分组 | 数量 | 平均得分 |\n|---|---:|---:|\n| 实验组 | 48 | 0.84 |\n| 对照组 | 52 | 0.76 |\n\n式中 $E=mc^2$。另一项估计 :math[frac(σ^2, n)]。\n\n## 校订与复核\n\n公式、正文和参考文献必须使用同一份内容。保存不能在输入目录留下编译中间文件。\n';
const file=path.join(docs,'Research.md');fs.writeFileSync(file,original);
fs.writeFileSync(path.join(docs,'references.bib'),'@book{kant,title={Critique of Pure Reason},author={Kant, Immanuel},year={1781},publisher={Cambridge University Press}}');
let native,phase='startup',shuttingDown=false;const delay=ms=>new Promise(r=>setTimeout(r,ms));
async function until(check,ms=120000){const end=Date.now()+ms;while(!await check()){if(Date.now()>end)throw Error('Timed out at '+phase);await delay(80);}}
function mark(name){report.checks.push({name,passed:true});console.log(JSON.stringify({phase,passed:true}));}
(async()=>{try{
  report.binarySha256=hash(process.env.READMD_BIN);report.bootSha256=hash(path.join(root,'assets/readmd.boot.js'));
  report.packSha256=hash(receipt.pack);assert.equal(report.packSha256,receipt.archiveSha256);
  process.env.READMD_ASSETS_DIR=receipt.assets;process.env.READMD_WORKSPACE=docs;
  process.env.READMD_NATIVE_LOG_FILE=path.join(data,'native.log');
  const oldPath=process.env.PATH;try{process.env.PATH=path.join(process.env.WINDIR,'System32');native=await startNative(root,data,chromium,28938);}finally{process.env.PATH=oldPath;}
  native.server.child.on('exit',(code,signal)=>{report.hostExit={code,signal,expected:shuttingDown,phase};});
  const page=native.page,errors=[];page.on('pageerror',e=>errors.push(e.message));
  const previewFailures=[];
  page.on('response',async response=>{
    if(response.url().endsWith('/api/export/preview')) {
      const body=await response.json().catch(()=>null);
      if(body&&body.ok===false)previewFailures.push({status:response.status(),code:body.error_code,details:body.details});
    }
  });
  report.previewFailures=previewFailures;
  // The 373 MB chooser uses real bounded HTTP chunks. Do not retain their
  // bodies in the automation/debugging channel (as in the chunk regression).
  const debugClient=playwright._toImpl(page).delegate._mainFrameSession._client;
  await debugClient.send('Network.enable',{maxPostDataSize:1,maxTotalBufferSize:1048576,maxResourceBufferSize:262144});
  debugClient.on('Inspector.detached',event=>{report.debugDetach=event.reason||'unknown';});
  report.boundedDebugNetwork=true;
  await page.evaluate(async()=>{await Promise.all([window.__readmdSessionReady,window.__readmdPreferencesReady]);await i18n.setLanguage('zh-CN',false);});
  assert.equal(crypto.createHash('sha256').update(await(await page.request.get(new URL('/assets/readmd.boot.js',page.url()).href)).body()).digest('hex'),report.bootSha256);
  const api=(route,body)=>page.evaluate(async({route,body})=>{const r=await apiFetch(route,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});return {status:r.status,data:await r.json()};},{route,body});
  phase='catalog preflight before extraction';
  const catalogFile=path.join(receipt.assets,'capabilities/catalog.json'),catalogBytes=fs.readFileSync(catalogFile);
  assert.equal(fs.statSync(catalogFile).nlink,1,'QA catalog must not share the application hard link');
  const incomplete=JSON.parse(catalogBytes);incomplete.packs=incomplete.packs.filter(p=>p.id!=='latex');
  try{
    fs.writeFileSync(catalogFile,JSON.stringify(incomplete));
    const started=Date.now(),denied=await api('/api/capabilities/packs',{action:'install',id:'latex',path:receipt.pack});
    const elapsed=Date.now()-started;report.timings.push({name:'missingCatalogPreflight',ms:elapsed});
    assert.equal(denied.data.error_code,'capability_pack_unavailable');
    // This includes JSON parsing/validation of a 6 MB, 34,822-file manifest.
    // The invariant is rejection before extraction, not a sub-second UI claim.
    assert(elapsed<10000,'Missing catalog preflight exceeded 10 seconds: '+elapsed);
    const folder=path.join(data,'capabilities/managed-v2/latex');assert(!fs.existsSync(folder)||!fs.readdirSync(folder).some(name=>name.startsWith('stream-install-')));
  }finally{fs.writeFileSync(catalogFile,catalogBytes);}
  mark('Missing catalog rejects a real 373 MB archive before extraction and leaves no staged runtime.');
  phase='missing runtime and contextual entry';
  const target=path.join(docs,'Existing.pdf');fs.writeFileSync(target,'Synthetic prior output');
  const absent=await api('/api/export',{format:'pdf',content:original,baseDir:docs,out_path:target,suggestedName:'Research',options:{pdf:{engine:'xelatex'}}});
  assert.equal(absent.data.error_code,'latex_engine_unavailable');assert.equal(fs.readFileSync(target,'utf8'),'Synthetic prior output');
  await page.evaluate(async file=>{await loadFile(file);openExportModal();},file);
  await until(()=>page.locator('#export-opts [data-k="pdf.engine"]').count());
  await page.evaluate(()=>{state.export.options.pdf={engine:'xelatex'};renderExportSections();});
  await until(()=>page.locator('#export-preview-capability button').count());
  assert.equal(await page.evaluate(()=>nativeExportPreview.missingCapability.id),'latex');
  assert.match(await page.locator('#export-preview-capability').innerText(),/TeX/);
  mark('Missing managed TeX preserves prior PDF and exposes install from the preview, with no host fallback.');
  phase='contextual cancellation';
  // Exercise the actual browser chooser in this same-host WebView. Only the
  // host picker bridge is temporarily disabled; no product API is intercepted.
  await page.evaluate(()=>{window.__texQaBridge=window.pywebview;window.pywebview=null;hasPy=false;});
  report.browserFallbackChooser=true;
  await page.locator('#export-preview-capability button').click();
  await page.locator('#capability-install-modal').waitFor({state:'visible'});
  assert.match(await page.locator('.capability-install-summary').innerText(),/356/);
  assert.match(await page.locator('.capability-install-summary').innerText(),/702/);
  await page.locator('#capability-install-modal button').first().click();
  await page.locator('#capability-install-modal').waitFor({state:'detached'});
  assert.equal(fs.readFileSync(file,'utf8'),original);
  assert.equal(await page.evaluate(()=>nativeExportPreview.missingCapability.id),'latex');
  mark('The real contextual installer explains download/disk size and cancellation preserves the document and missing-engine state.');
  phase='real stream installation and engine probes';const installStart=Date.now();
  await page.locator('#export-preview-capability button').click();
  await page.locator('#capability-install-modal').waitFor({state:'visible'});
  const chooser=page.waitForEvent('filechooser');
  await page.getByRole('button',{name:'导入离线包',exact:true}).click();
  const implementation=playwright._toImpl(native.browser);
  assert.equal(typeof implementation._isCollocatedWithServer,'boolean');implementation._isCollocatedWithServer=true;
  await(await chooser).setFiles(receipt.pack,{timeout:120000});
  await page.locator('#capability-install-modal').waitFor({state:'detached',timeout:600000});
  await page.evaluate(()=>{window.pywebview=window.__texQaBridge;hasPy=true;delete window.__texQaBridge;});
  report.timings.push({name:'installValidateAndProbe',ms:Date.now()-installStart});
  assert.equal((await api('/api/capabilities/packs',{action:'status',id:'latex'})).data.installed,true);
  mark('All 34,822 files stream-install, validate, and execute both real minimum engine operations before ready.');
  phase='native Chinese preview';const started=Date.now();
  await until(()=>page.evaluate(()=>nativeExportPreview.data?.pages>0));
  const preview=await page.evaluate(()=>nativeExportPreview.data);
  assert.equal(preview.mode,'artifact');assert.equal(preview.previewEngine,'xelatex');
  assert.match(preview.text,/从观察到结论/);assert.match(preview.text,/1781/);assert.match(preview.text,/42/);
  assert.ok(!preview.text.includes('readmd:math:'));
  report.timings.push({name:'nativeChinesePreview',ms:Date.now()-started});
  mark('Actual XeLaTeX preview includes Chinese prose, DSL formulas, tables and CSL references.');
  phase='preview export identity';const output=path.join(docs,'Research.pdf'),options=await page.evaluate(()=>collectExportOptions());
  const exported=await api('/api/export',{format:'pdf',content:original,baseDir:docs,out_path:output,suggestedName:await page.evaluate(()=>currentExportName()),options});
  assert.equal(exported.data.ok,true,JSON.stringify(exported.data));
  assert.deepEqual(fs.readFileSync(output),Buffer.from(preview.pdf,'base64'));
  report.exports.chinese={path:path.relative(data,output),sha256:hash(output),pages:preview.pages};
  mark('Export reuses the exact preview artifact and commits a real PDF atomically.');
  phase='second managed engine';
  const latin=await api('/api/export/preview',{format:'pdf',content:'# Offline publication\n\nReadMD portable output. A formula $x^2$.\n',
    suggestedName:'Offline-publication',options:{pdf:{engine:'pdflatex'},tex:{docClass:'article',useCtex:false}}});
  assert.equal(latin.data.ok,true,JSON.stringify(latin.data));assert.equal(latin.data.previewEngine,'pdflatex');
  assert.match(latin.data.text,/ReadMD portable output/);const latinFile=path.join(data,'Latin.pdf');
  fs.writeFileSync(latinFile,Buffer.from(latin.data.pdf,'base64'));report.exports.latin={path:'Latin.pdf',sha256:hash(latinFile),pages:latin.data.pages};
  mark('pdfLaTeX executes through the same managed service and returns searchable vector publication output.');
  phase='real MCP managed export';
  const mcpOutput=path.join(data,'MCP.pdf');
  report.mcp=await require('./managed-tex-mcp.cjs')(process.env.READMD_BIN,data,receipt.assets,docs,mcpOutput);
  assert.ok(fs.statSync(mcpOutput).size>1000);
  report.exports.mcp={path:'MCP.pdf',sha256:hash(mcpOutput),pages:1};
  mark('A real stdio MCP client initializes, discovers the tool and exports through the same installed TeX service without host PATH.');
  phase='compact layouts';
  const cdp=await page.context().newCDPSession(page);
  for(const[width,height]of[[1024,680],[1160,820]]){
    await cdp.send('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false});
    assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));
    const button=await page.locator('#export-run').boundingBox();assert(button&&button.y+button.height<=height+1);
    await page.screenshot({path:path.join(data,'tex-managed-'+width+'.png')});
  }await cdp.detach();await page.evaluate(()=>closeExportModal());
  mark('Real preview/export controls fit 1024×680 and 1160×820 under the production CSP.');
  phase='save reopen and clean input folder';
  await page.evaluate(async()=>{await toggleEdit();setEditorPref('aiCompletion',false);});
  const edited=original+'\n校订完成：保留来源。\n';await page.evaluate(text=>cmView.dispatch({changes:{from:0,to:cmView.state.doc.length,insert:text}}),edited);
  assert.equal(await page.evaluate(()=>saveEdit()),true);assert.equal(fs.readFileSync(file,'utf8'),edited);
  await page.evaluate(async file=>{await exitEdit();await loadFile(file);},file);
  await until(async()=>(await page.locator('#content').innerText()).includes('校订完成'));
  assert.deepEqual(fs.readdirSync(docs).sort(),['Existing.pdf','Research.md','Research.pdf','references.bib'].sort());
  mark('Saving/reopening works and leaves no TeX intermediates, copied fonts or engine folders beside the input.');
  phase='uninstall and invalidate preview';
  const removed=await api('/api/capabilities/packs',{action:'uninstall',id:'latex'});assert.equal(removed.data.ok,true,JSON.stringify(removed.data));
  const again=await api('/api/export/preview',{format:'pdf',content:original,baseDir:docs,suggestedName:'Research',options});
  assert.equal(again.data.error_code,'latex_engine_unavailable');assert.equal(hash(output),report.exports.chinese.sha256);
  assert.equal((await api('/api/capabilities/packs',{action:'status',id:'latex'})).data.installed,false);
  assert.deepEqual(errors,[]);mark('Uninstall removes only managed runtime files, invalidates the previous artifact identity and preserves exported documents.');
  report.passed=true;
}catch(error){report.failedStage=phase;report.error=error.name;report.message=String(error.message).replaceAll(data,'<qa-profile>').replaceAll(root,'<workspace>').slice(0,2000);if(native){report.hostAtFailure={exitCode:native.server.child.exitCode,signalCode:native.server.child.signalCode,connected:native.browser.isConnected()};await native.page.screenshot({path:path.join(data,'failure.png')}).catch(()=>{});}throw error;}
finally{shuttingDown=true;if(native){const stopped=new Promise(resolve=>native.server.child.once('exit',resolve));await native.browser.close().catch(()=>{});if(native.server.child.exitCode===null){native.server.child.kill();await Promise.race([stopped,delay(2000)]);}}report.recordedAt=new Date().toISOString();fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,failedStage:report.failedStage,profile:path.relative(root,data)}));}
})().catch(()=>{process.exitCode=1;});
