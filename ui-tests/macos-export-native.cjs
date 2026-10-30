'use strict';
// Real Cocoa/WKWebView, production bridge/CSP/auth. Explicit synthetic output
// paths exercise the real snapshot writer; no native chooser or auth is mocked.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict'),vm=require('node:vm');
const {spawn}=require('node:child_process');
const root=path.resolve(__dirname,'..'),allowed=path.dirname(root),sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const content='# Export contract\n\nActual slide one.\n\n---\n\n## Second slide\n\nThe original sentence must survive.\n\n---\n\n## Third slide\n\nA complete final page.\n'+(process.env.READMD_QA_FONT_PACK?'\n真实中文导出与恢复。\n\n![中文图像](figure.svg)\n':'');
function driver(file,outputs,fontPack){return `(async()=>{
 const checks=[];let phase='startup';const file=${JSON.stringify(file)},outputs=${JSON.stringify(outputs)};
 const check=(ok,name)=>{if(!ok)throw Error(name);checks.push(name);};
 const until=async fn=>{const end=Date.now()+40000;while(Date.now()<end){if(await fn())return;await new Promise(r=>setTimeout(r,40));}throw Error('condition_timeout');};
 try{
  await window.__readmdSessionReady;check(window.__readmdAppReady&&hasPy&&!!window.ipc,'cocoa_bridge_ready');
  await loadFile(file);check(getActiveTab()?.path===file,'chinese_path_opened');
  ${fontPack?`const installed=await ReadMDCapabilities.request({action:'install',path:${JSON.stringify(fontPack)}});check(installed.ok===true,'locked_offline_fonts_installed');`:''}
  check(!getActiveTab().isDirty,'opened_document_clean');await openExportModal();
  for(const fmt of ['pdf','docx','epub','html','tex','pptx']){
   phase=fmt;document.querySelector('#export-box .exp-fmt[data-fmt="'+fmt+'"]').click();
   await until(()=>{const host=$('export-preview-mini-content');if(host?.getAttribute('aria-busy')==='true')return false;
    if(fmt==='pdf'||fmt==='docx'){const img=host?.querySelector('img');return nativeExportPreview.data?.format===fmt&&img?.complete&&img.naturalWidth>100;}
    if(fmt==='epub')return nativeExportPreview.data?.format==='epub'&&host?.querySelector('iframe')?.contentDocument?.body?.textContent.includes('Export contract');
    if(fmt==='tex')return nativeExportPreview.data?.mode==='source'&&host?.querySelector('code')?.textContent.includes('\\\\documentclass');
    if(fmt==='pptx')return host?.querySelector('iframe')?.dataset.ready==='true';
    return nativeExportPreview.data?.format==='html'&&host?.dataset.ready==='true'&&host.textContent.includes('Actual slide one.')
     &&[...host.querySelectorAll('img')].every(image=>image.complete&&image.naturalWidth>0);
   });check(true,fmt+'_preview_ready');
   if(fmt==='pptx'){
    const doc=$('export-preview-mini-content').querySelector('iframe').contentDocument;
    check(doc.querySelectorAll('.readmd-slide').length===3&&nativeExportPreview.data.width===960&&nativeExportPreview.data.height===540,'three_actual_widescreen_slides');
   }
    const frozen=await prepareReviewExport(fmt,effectiveExportOptions(collectExportOptions()),currentExportName(),getActiveTab(),false);
    await bindSnapshotRuntime(frozen);await prepareSnapshotFigures(frozen);
    const receipt=await exportSnapshotRequest('commit',{snapshot_id:frozen.snapshot_id,call_id:frozen.commitCallId,out_path:outputs[fmt],overwrite:false,accept_pending:false});
    check(receipt.ok===true,fmt+'_snapshot_committed');
  }
  closeExportModal();await loadFile(file);check(getActiveTab().content===${JSON.stringify(content)},'source_reopened_unchanged');
  check(await ReadMDSession.flush()!==false,'session_acknowledged');
  window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase,checks}}));await py.request_quit();
 }catch(error){
  checks.push(/^[a-z_]{1,127}$/.test(error.message)?error.message:'native_export_failed');
  checks.push(window.READMD_ENGINE==='rust'?'engine_rust':'engine_other');
  checks.push(nativeExportPreview.data?'preview_data_present':'preview_data_missing');
  const host=$('export-preview-mini-content'),image=host?.querySelector('img');
  checks.push(image?(image.naturalWidth?'image_has_pixels':'image_without_pixels'):'image_missing');
  if(nativeExportPreview.missingCapability?.id)checks.push('missing_'+nativeExportPreview.missingCapability.id);
  try{const response=await apiFetch('/api/export/preview',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({format:'pdf',content:'Native PDF diagnostic.',options:{},baseDir:state.dir})});const data=await response.json();checks.push(data.ok?'direct_preview_ok':'direct_preview_failed');if(data.error_code&&/^[a-z_]+$/.test(data.error_code))checks.push(data.error_code);checks.push(data.firstPage?'direct_page_pixels':'direct_page_missing');}catch(_) {checks.push('diagnostic_request_failed');}
  window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:false,phase,checks}}));window.ipc.postMessage('readmd:quit');}
})();`;}
async function run(binary,out){
 assert.equal(process.platform,'darwin');assert(path.isAbsolute(binary)&&fs.statSync(binary).isFile());assert(out.startsWith(allowed+path.sep));fs.mkdirSync(out,{mode:0o700});
 const docs=path.join(out,'documents'),profile=path.join(out,'profile'),temp=path.join(out,'tmp'),nativeHome=path.join(out,'native-home');for(const dir of [docs,profile,temp,nativeHome])fs.mkdirSync(dir,{mode:0o700});
 const file=path.join(docs,'中文 导出.md');fs.writeFileSync(file,content,{flag:'wx'});const outputs=Object.fromEntries(['pdf','docx','epub','html','tex','pptx'].map(fmt=>[fmt,path.join(docs,'Export.'+fmt)]));
 if(process.env.READMD_QA_FONT_PACK)fs.writeFileSync(path.join(docs,'figure.svg'),'<svg xmlns="http://www.w3.org/2000/svg" width="240" height="60"><text x="12" y="40" font-size="28">中文图像</text></svg>',{flag:'wx'});
 fs.writeFileSync(path.join(profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1',{mode:0o600});fs.writeFileSync(path.join(profile,'settings.json'),JSON.stringify({lang:'en',closeToTray:false,autoCheckUpdate:false}));
 let ownedPack;if(process.env.READMD_QA_FONT_PACK){const bytes=fs.readFileSync(process.env.READMD_QA_FONT_PACK);assert.equal(sha(bytes),'037e01665e90fde0fc81adb8cda2093e5862c491794dcbe95544660479f07a69');ownedPack=path.join(docs,'fonts.readmd-pack');fs.writeFileSync(ownedPack,bytes,{flag:'wx'});}
 const script=driver(file,outputs,ownedPack);new vm.Script(script);fs.writeFileSync(path.join(profile,'native-qa.js'),script,{mode:0o600});
 const report={nativeHost:'Cocoa WKWebView',productionCsp:true,strictTokenMode:true,authOrChooserShim:false,explicitOwnedOutputs:true,binarySha256:sha(fs.readFileSync(binary)),bootSha256:sha(fs.readFileSync(path.join(root,'assets/readmd.boot.js'))),driverSha256:sha(fs.readFileSync(__filename)),passed:false};
 const policy='(version 1)(allow default)(deny file-write* (require-all (require-not (subpath '+JSON.stringify(allowed)+')) (require-not (subpath '+JSON.stringify('/System/Volumes/Data'+allowed)+')) (require-not (literal "/dev/null")) (require-not (literal "/dev/tty"))))';
 const fd=fs.openSync(path.join(out,'native.log'),'wx');const inBundle=binary.includes('.app/Contents/MacOS/');
 const child=spawn('/usr/bin/sandbox-exec',['-p',policy,binary,'--port','0','--require-token','--data-dir',profile,'--workspace',docs,...(inBundle?[]:['--assets',path.join(root,'assets')])],{cwd:out,env:{...process.env,READMD_NATIVE_QA:'1',READMD_DATA_DIR:profile,READMD_WORKSPACE:docs,READMD_EXTRA_ROOTS:'',READMD_ASSETS_DIR:'',TMPDIR:temp,SQLITE_TMPDIR:temp,CFFIXED_USER_HOME:nativeHome},stdio:['ignore',fd,fd]});fs.closeSync(fd);
 let timer;
 try{
  const outcome=await new Promise((resolve,reject)=>{timer=setTimeout(()=>{child.kill('SIGTERM');reject(Error('native_export_deadline'));},180000);child.on('error',reject);child.on('exit',(code,signal)=>resolve({code,signal}));});
  report.outcome=outcome;const result=JSON.parse(fs.readFileSync(path.join(profile,'native-qa-result.json'),'utf8'));report.result=result;
  assert.equal(outcome.code,0);assert.equal(result.ok,true,result.phase+': '+result.checks.join(','));assert.equal(fs.readFileSync(file,'utf8'),content);
  report.artifacts=Object.entries(outputs).map(([format,file])=>{const bytes=fs.readFileSync(file);assert(bytes.length>100);if(format==='pptx')assert.equal(bytes.subarray(0,4).toString('hex'),'504b0304');return {format,bytes:bytes.length,sha256:sha(bytes)};});report.passed=true;
 }finally{clearTimeout(timer);if(child.exitCode===null&&child.signalCode===null)child.kill('SIGTERM');fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n');for(const dir of [temp,nativeHome])fs.rmSync(dir,{recursive:true,force:true});if(ownedPack)fs.unlinkSync(ownedPack);}
 console.log(JSON.stringify({nativeExportsPassed:report.passed,checks:report.result.checks.length,formats:report.artifacts.length}));
}
if(require.main===module){const [binary,out]=process.argv.slice(2);if(!binary||!out)throw Error('Usage: macos-export-native.cjs <explicit debug native-qa binary> <new evidence directory>');run(path.resolve(binary),path.resolve(out)).catch(error=>{console.error(error.message);process.exitCode=1;});}
module.exports={driver};
