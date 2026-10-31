'use strict';
// Genuine LaunchServices file URL delivery, without changing default handlers
// or passing the document through CLI arguments / loadFile in the QA driver.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),vm=require('node:vm');
const {prepare}=require('./macos-native-system-host.cjs');
const root=path.resolve(__dirname,'..'),workspace=path.dirname(root),sha=b=>crypto.createHash('sha256').update(b).digest('hex'),pause=ms=>new Promise(r=>setTimeout(r,ms));
const before='# 系统打开固定输入\n\n中文路径与 emoji 🧭。\n',added='\n从真正的系统文件事件保存。\n',encode=s=>Buffer.concat([Buffer.from([239,187,191]),Buffer.from(s.replace(/\n/g,'\r\n'))]);
function driver(phase,file){return String.raw`(async()=>{
 const phase=${JSON.stringify(phase)},file=${JSON.stringify(file)},checks=[];
 const check=(ok,name)=>{if(!ok)throw Error(name);checks.push(name);};
 const until=async fn=>{const end=Date.now()+30000;while(Date.now()<end){py.window_control('status');if(await fn())return;await new Promise(r=>setTimeout(r,50));}throw Error('os_open_condition_timeout');};
 try{
  await __readmdSessionReady;await until(()=>getActiveTab()?.path===file&&!document.hidden);
  check(getActiveTab().content.replace(/\r\n/g,'\n')===${JSON.stringify(phase==='open'?before:before+added)},phase==='open'?'actual_os_file_url_opened_chinese_document':'os_open_session_restored_from_disk');
  check(!getActiveTab().isDirty,'opened_document_clean');check(!state.folder,'file_open_does_not_select_parent_workspace');check(state.tabs.length===1,'single_document_without_duplicate_tab');
  await toggleEdit();await until(()=>state.editing&&!!cmView);check(!hasUnsavedEditorChanges(),'entering_edit_creates_no_dirty_copy');
  if(phase==='open'){
   cmView.dispatch({changes:{from:cmView.state.doc.length,insert:${JSON.stringify(added)}}});check(await saveEdit()===true,'os_open_document_saved');check(!hasUnsavedEditorChanges(),'saved_document_clean');await toggleEdit();
   py.window_control('qa_native_button',{button:'minimize',id:1});await until(()=>window.__readmdNativeWindowState?.minimized===true);check(true,'actual_native_window_minimized');
   window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase:'reopen_ready',checks:['native_reader_minimized_for_os_reopen']}}));
   await until(()=>window.__readmdNativeWindowState?.visible&&window.__readmdNativeWindowState?.focused&&!window.__readmdNativeWindowState?.minimized&&!document.hidden);check(true,'actual_os_reopen_restores_and_activates_reader');
   check(getActiveTab()?.path===file&&getActiveTab().content.replace(/\r\n/g,'\n')===${JSON.stringify(before+added)},'os_reopen_preserves_document_and_text');
  }else{check(getEditContent().replace(/\r\n/g,'\n')===${JSON.stringify(before+added)},'restored_document_editable');await toggleEdit();}
  const modules=await(await apiFetch('/api/modules')).json();check(modules.deferred_modules.includes('ocr')&&modules.deferred_modules.includes('transcribe')&&modules.modules.ocr==='idle'&&modules.modules.transcribe==='idle','model_services_remain_deferred');
  check(await ReadMDSession.flush()!==false,'os_open_session_acknowledged');window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase,checks}}));await py.request_quit();
 }catch(error){checks.push(/^[a-z_]{1,127}$/.test(error.message)?error.message:'os_open_failed');window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:false,phase,checks}}));window.ipc.postMessage('readmd:quit');}
})();`;}
async function run(app,out){
 assert(out.startsWith(workspace+path.sep));fs.mkdirSync(out,{mode:0o700});const profile=path.join(out,'profile'),docs=path.join(out,'empty-workspace'),external=path.join(out,'outside-workspace'),tmp=path.join(out,'tmp'),home=path.join(out,'native-home');for(const d of [profile,docs,external,tmp,home])fs.mkdirSync(d,{mode:0o700});
 const file=path.join(external,'中文 系统打开 🧭.md');fs.writeFileSync(file,encode(before),{flag:'wx'});fs.writeFileSync(path.join(profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1',{flag:'wx',mode:0o600});fs.writeFileSync(path.join(profile,'settings.json'),JSON.stringify({lang:'zh-CN',closeToTray:false,autoCheckUpdate:false}));
 const report={nativeHost:'LaunchServices / Cocoa WKWebView',defaultAssociationsChanged:false,defaultAssociationTested:false,explicitOsFileOpen:true,documentPassedInCli:false,documentOutsideAuthorizedWorkspace:true,modelsRequested:false,productionCsp:true,strictAuth:true,harnessSha256:sha(fs.readFileSync(__filename)),hostSha256:sha(fs.readFileSync(require.resolve('./macos-native-system-host.cjs'))),bootSha256:sha(fs.readFileSync(path.join(root,'assets/readmd.boot.js'))),phases:[],passed:false};
 try{const host=prepare(app,out,{tmp,home});report.binarySha256=host.binarySha256;for(const phase of ['open','restore']){
  const script=driver(phase,file);new vm.Script(script);fs.writeFileSync(path.join(profile,'native-qa.js'),script,{mode:0o600});const resultFile=path.join(profile,'native-qa-result.json'),instance=await host.launch({profile,docs,log:path.join(out,phase+'.log'),...(phase==='open'?{openFile:file}:{})}),entry={phase,host:instance.receipt,passed:false};report.phases.push(entry);
  try{const waiting=instance.wait(70000);waiting.catch(()=>{});if(phase==='open'){
    const end=Date.now()+35000;while(!fs.existsSync(resultFile)&&!instance.launcherStopped&&Date.now()<end)await pause(50);const ready=JSON.parse(fs.readFileSync(resultFile));assert.equal(ready.ok,true,ready.checks.join(','));assert.equal(ready.phase,'reopen_ready');fs.renameSync(resultFile,path.join(out,'reopen-ready.json'));instance.reopen();
   }
   await waiting;const result=JSON.parse(fs.readFileSync(resultFile));entry.result=result;fs.renameSync(resultFile,path.join(out,phase+'.json'));assert(result.ok,result.checks.join(','));assert.equal(result.phase,phase);assert.deepEqual(fs.readFileSync(file),encode(before+added));assert.deepEqual(fs.readdirSync(external),['中文 系统打开 🧭.md']);assert.deepEqual(fs.readdirSync(docs),[]);entry.passed=true;
  }finally{await instance.cleanup();}
 }report.passed=true;
 }finally{fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx',mode:0o600});for(const d of [tmp,home])fs.rmSync(d,{recursive:true,force:true});}
 console.log(JSON.stringify({nativeOsOpenPassed:report.passed,phases:report.phases.length,checks:report.phases.reduce((n,p)=>n+p.result.checks.length,0),defaultAssociationChanged:false}));
}
if(require.main===module){const args=process.argv.slice(2);assert.equal(args.length,2);run(...args.map(p=>path.resolve(p))).catch(e=>{console.error(e.message);process.exitCode=1;});}
