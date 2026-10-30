'use strict';
// Real Cocoa/WKWebView, real NSOpenPanel/NSSavePanel, Vision child, strict auth
// and production CSP. Only owned synthetic files; no chooser/auth substitute.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict'),vm=require('node:vm'),{spawn}=require('node:child_process');
const root=path.resolve(__dirname,'..'),allowed=path.dirname(root),sha=b=>crypto.createHash('sha256').update(b).digest('hex');
function driver(phase,files,lines){return String.raw`(async()=>{
 const checks=[],phase=${JSON.stringify(phase)},files=${JSON.stringify(files)},lines=${JSON.stringify(lines)};
 const corrected='# 校订结果\n\n'+lines.join('\n')+'\n';
 const check=(ok,name)=>{if(!ok)throw Error(name);checks.push(name);};
 const sleep=ms=>new Promise(r=>setTimeout(r,ms));
 const until=async fn=>{const end=Date.now()+55000;while(Date.now()<end){if(await fn())return;await sleep(30);}throw Error('native_ocr_condition_timeout');};
 let id=0;
 const panel=async(choice,p)=>{
  const mine=++id;py.window_control('qa_native_dialog',{choice,path:p,id:mine});
  await until(()=>window.__readmdNativeDialogResult?.id===mine);return window.__readmdNativeDialogResult;
 };
 const finishPanel=async(choice,p)=>{
  await until(async()=>{const r=await panel('status',p);return r.available&&r.visible;});
  check(true,'real_appkit_panel_visible');
  if(choice==='accept'){
   await panel('configure',p);
   await until(async()=>Boolean((await panel('status',p)).configured));
  }
  await panel(choice);
 };
 const post=async(route,body)=>{const r=await apiFetch(route,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});return {r,d:await r.json()};};
 try{
  await window.__readmdSessionReady;
  check(window.__readmdAppReady&&hasPy&&document.body.classList.contains('native-macos'),'actual_mac_reader_ready');
  if(phase==='recognize_save'){
   const status=await post('/api/capabilities/packs',{action:'status',id:'ocr'});
   check(status.r.ok&&status.d.ready&&status.d.native&&status.d.installed===false,'real_native_probe_without_managed_install_claim');
   const choosing=chooseFile('ocr');await finishPanel('accept',files.image);await choosing;
   const tab=getActiveTab();
   check(tab?.source==='ocr'&&tab.isVirtual&&!tab.path,'native_open_panel_to_real_ocr_draft');
   check(lines.slice(0,3).every(s=>tab.content.includes(s)),'three_chinese_lines_recognized_exactly');
   check(tab.content.includes('ReadMD')&&tab.content.includes('2026'),'mixed_script_recognized');
   check(!tab.isDirty&&!tab.unsavedCreation,'preview_is_clean_and_unsaved');
   check(document.getElementById('content').textContent.includes(lines[0]),'recognized_content_has_native_layout');
   window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase:'preview_ready',checks}}));
   await until(async()=>{const r=await apiFetch('/api/file?p='+encodeURIComponent(files.ack));if(!r.ok)return false;return (await r.json()).content==='continue';});
   await toggleEdit();await until(()=>state.editing&&!!cmView);
   cmView.dispatch({changes:{from:0,to:cmView.state.doc.length,insert:corrected}});
   check(getEditContent()===corrected&&hasUnsavedEditorChanges(),'explicit_user_correction_in_working_draft');
   const saving=saveEdit();await finishPanel('accept',files.saved);
   check(await saving===true,'actual_native_save_panel_and_backend_commit');
   check(getActiveTab().path===files.saved&&!hasUnsavedEditorChanges(),'save_ack_retargets_corrected_document');
   cmView.dispatch({changes:{from:cmView.state.doc.length,insert:'\n尚未保存的校订。\n'}});
   const dirty=getEditContent(),cancelSave=saveAs();await finishPanel('cancel');
   check(await cancelSave===false,'native_save_panel_cancel_is_not_success');
   check(getActiveTab().path===files.saved&&getEditContent()===dirty&&hasUnsavedEditorChanges(),'save_cancel_preserves_dirty_body_and_identity');
   cmHistoryCommand('undo')(cmView);
   check(getEditContent()===corrected&&!hasUnsavedEditorChanges(),'one_undo_restores_saved_correction');
   const count=state.tabs.length;
   check(await ocrFile(files.blank)===false&&state.tabs.length===count,'empty_recognition_creates_no_document');
   check(await ocrFile(files.invalid)===false&&state.tabs.length===count,'corrupt_input_creates_no_document');
   const task='mac-ocr-cancel-owned',request=apiFetch('/api/ocr?p='+encodeURIComponent(files.image)+'&save=1&task_id='+task);
   let cancelled=false;
   await until(async()=>{const {d}=await post('/api/task/cancel',{id:task});cancelled=d.state==='cancelling';return cancelled;});
   const at=performance.now(),response=await request,result=await response.json();
   check(cancelled&&result.error_code==='cancelled'&&!response.ok,'running_native_worker_cancelled_before_publication');
   check(performance.now()-at<200,'cancel_ack_to_worker_completion_under_200ms');
   check(getEditContent()===corrected&&!hasUnsavedEditorChanges(),'failed_and_cancelled_ocr_preserve_current_document');
   check(await confirmExitEdit()===true,'corrected_editor_exits_cleanly');
  }else{
   await until(()=>getActiveTab()?.path===files.saved);
   check(getActiveTab().content===corrected,'corrected_ocr_result_restored_from_disk');
   await toggleEdit();await until(()=>state.editing&&!!cmView);
   check(getEditContent()===corrected&&!hasUnsavedEditorChanges(),'reopened_ocr_result_is_clean_and_editable');
  }
  check(await ReadMDSession.flush()!==false,'session_persisted');
  window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase,checks}}));
  await py.request_quit();
 }catch(error){
  const label=String(error.message||'native_ocr_failed');checks.push(/^[a-z_]{1,127}$/.test(label)?label:'native_ocr_failed');
  window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:false,phase,checks}}));window.ipc.postMessage('readmd:quit');
 }
})()`;}
async function run(binary,out,fixture,expectedFile){
 assert.equal(process.platform,'darwin');for(const p of [binary,out,fixture,expectedFile])assert(path.isAbsolute(p));
 assert(out.startsWith(allowed+path.sep));assert(fixture.startsWith(allowed+path.sep));
 const input=fs.readFileSync(fixture),lines=JSON.parse(fs.readFileSync(expectedFile,'utf8')).lines;
 assert(Array.isArray(lines)&&lines.length===4&&lines.every(s=>typeof s==='string'));
 fs.mkdirSync(out,{mode:0o700});const docs=path.join(out,'documents'),profile=path.join(out,'profile'),temp=path.join(out,'tmp'),home=path.join(out,'native-home');
 for(const p of [docs,profile,temp,home])fs.mkdirSync(p,{mode:0o700});
 const files={image:path.join(docs,'中文 扫描.png'),saved:path.join(docs,'校订结果.md'),blank:path.join(docs,'空白.png'),invalid:path.join(docs,'损坏.png'),ack:path.join(docs,'preview.ack.md')};
 fs.writeFileSync(files.image,input,{flag:'wx'});fs.writeFileSync(files.invalid,'invalid image',{flag:'wx'});
 // A fixed 1x1 transparent PNG is an independently empty input.
 fs.writeFileSync(files.blank,Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR42mP8/x8AAwMCAO+/e9sAAAAASUVORK5CYII=','base64'),{flag:'wx'});
 fs.writeFileSync(path.join(profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1',{flag:'wx',mode:0o600});
 fs.writeFileSync(path.join(profile,'settings.json'),JSON.stringify({lang:'zh-CN',closeToTray:false,autoCheckUpdate:false}));
 const expected='# 校订结果\n\n'+lines.join('\n')+'\n',report={platform:process.platform+'-'+process.arch,nativeHost:'Cocoa WKWebView',productionCsp:true,strictTokenMode:true,chooserShim:false,binarySha256:sha(fs.readFileSync(binary)),bootSha256:sha(fs.readFileSync(path.join(root,'assets/readmd.boot.js'))),sourceSha256:sha(input),harnessSha256:sha(fs.readFileSync(__filename)),phases:[],passed:false};
 const policy='(version 1)(allow default)(deny file-write* (require-all (require-not (subpath '+JSON.stringify(allowed)+')) (require-not (subpath '+JSON.stringify('/System/Volumes/Data'+allowed)+')) (require-not (literal "/dev/null")) (require-not (literal "/dev/tty"))))';
 try{
  for(const phase of ['recognize_save','reopen']){
   const script=driver(phase,files,lines);new vm.Script(script);fs.writeFileSync(path.join(profile,'native-qa.js'),script,{mode:0o600});
   const fd=fs.openSync(path.join(out,phase+'.log'),'wx'),resultFile=path.join(profile,'native-qa-result.json');
   const child=spawn('/usr/bin/sandbox-exec',['-p',policy,binary,'--port','0','--require-token','--data-dir',profile,'--workspace',docs,'--assets',path.join(root,'assets')],{cwd:out,env:{...process.env,READMD_NATIVE_QA:'1',READMD_STARTUP_PROBE:'1',READMD_DATA_DIR:profile,READMD_WORKSPACE:docs,READMD_EXTRA_ROOTS:'',TMPDIR:temp,SQLITE_TMPDIR:temp,CFFIXED_USER_HOME:home},stdio:['ignore',fd,fd]});fs.closeSync(fd);
   const exited=new Promise(resolve=>{child.once('exit',(code,signal)=>resolve({code,signal}));child.once('error',()=>resolve({code:null,error:'spawn_failed'}));});
   const timer=setTimeout(()=>child.kill('SIGTERM'),120000);
   let preview=false;try{
    while(child.exitCode===null&&child.signalCode===null){
     if(fs.existsSync(resultFile)){const r=JSON.parse(fs.readFileSync(resultFile));if(r.phase==='preview_ready'){
      assert.deepEqual(fs.readdirSync(docs).sort(),['中文 扫描.png','损坏.png','空白.png'].sort());
      assert.equal(sha(fs.readFileSync(files.image)),report.sourceSha256);preview=true;
      fs.renameSync(resultFile,path.join(out,'preview-ready.json'));fs.writeFileSync(files.ack,'continue',{flag:'wx'});
     }}await new Promise(r=>setTimeout(r,25));
    }
   }finally{clearTimeout(timer);if(child.exitCode===null&&child.signalCode===null)child.kill('SIGTERM');}
   const outcome=await exited,result=fs.existsSync(resultFile)?JSON.parse(fs.readFileSync(resultFile)):{ok:false,phase,checks:['native_report_missing']};
   if(fs.existsSync(resultFile))fs.renameSync(resultFile,path.join(out,phase+'.json'));
   report.phases.push({...result,...outcome});assert.equal(outcome.code,0);assert.equal(result.ok,true,result.checks.join(','));
   if(phase==='recognize_save')assert(preview,'native preview checkpoint observed');
   assert.equal(fs.readFileSync(files.saved,'utf8'),expected);assert.equal(sha(fs.readFileSync(files.image)),report.sourceSha256);
   assert(!fs.existsSync(path.join(docs,'中文 扫描.md')));assert(!fs.existsSync(path.join(docs,'空白.md')));assert(!fs.existsSync(path.join(docs,'损坏.md')));
  }
  report.passed=true;
 }finally{
  fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n');
  if(fs.existsSync(files.ack))fs.unlinkSync(files.ack);
  for(const p of [temp,home])fs.rmSync(p,{recursive:true,force:true});
 }
 console.log(JSON.stringify({nativeOcrPassed:report.passed,phases:report.phases.length,checks:report.phases.reduce((n,x)=>n+x.checks.length,0)}));
}
if(require.main===module){const args=process.argv.slice(2);assert.equal(args.length,4,'binary, new evidence directory, owned PNG, expected JSON required');run(...args.map(p=>path.resolve(p))).catch(e=>{console.error(e.message);process.exitCode=1;});}
