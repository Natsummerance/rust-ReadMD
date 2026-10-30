'use strict';
// Opt-in integration in the actual Cocoa WKWebView. Requires an explicitly
// prebuilt debug native-qa binary, a NEW evidence directory and synthetic files.
// Uses the production frontend/bridge/CSP; no browser, UI automation or token shim.
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),crypto=require('node:crypto');
const {spawn}=require('node:child_process');
const assert=require('node:assert/strict');
const vm=require('node:vm');
const root=path.resolve(__dirname,'..'),allowed=path.dirname(root);
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const before='# Mac 原生验收\n\n重复段落。🙂\n重复段落。🙂\n';
const inserted='\n保存成功：中文路径与 emoji 🧭。\n';
const external='# External revision\n\n外部程序的新正文。\n';
const unsaved='\n尚未保存的本人编辑。\n';
const encode=s=>Buffer.concat([Buffer.from([0xef,0xbb,0xbf]),Buffer.from(s.replace(/\n/g,'\r\n'))]);
function driver(phase,file) {
  return String.raw`(async()=>{
    const checks=[],phase=${JSON.stringify(phase)},file=${JSON.stringify(file)};
    function check(ok,name){if(!ok)throw Error(name);checks.push(name);}
    async function until(fn){const end=Date.now()+25000;while(Date.now()<end){if(await fn())return;await new Promise(r=>setTimeout(r,40));}throw Error('condition_timeout');}
    try{
      await window.__readmdSessionReady;
      check(window.__readmdAppReady===true&&hasPy&&!!py.save_file,'native_bridge_ready');
      if(phase==='edit_save'){
        await loadFile(file);
        check(getActiveTab()?.path===file,'chinese_path_opened');
        check(getActiveTab().content.replace(/\r\n/g,'\n')===${JSON.stringify(before)},'initial_text_exact');
        const count=state.tabs.length;
        await toggleEdit();await until(()=>state.editing&&!!cmView);
        await until(()=>cmView.dom.getBoundingClientRect().height>100);
        check(!document.hidden,'native_document_visible');
        check(cmView.dom.getBoundingClientRect().width>100,'editor_has_real_layout');
        const line=cmView.dom.querySelector('.cm-line'),lineBox=line.getBoundingClientRect(),scrollBox=cmView.scrollDOM.getBoundingClientRect();
        check(line.textContent.includes('Mac 原生验收')&&lineBox.top>=scrollBox.top-2&&lineBox.bottom<=scrollBox.bottom+2,'source_visible_without_scrolling');
        check(!hasUnsavedEditorChanges()&&!getActiveTab().isDirty,'unchanged_editor_clean');
        check(state.tabs.length===count,'edit_creates_no_copy');
        cmView.dispatch({changes:{from:cmView.state.doc.length,insert:${JSON.stringify(inserted)}}});
        check(hasUnsavedEditorChanges(),'edit_is_dirty');
        cmHistoryCommand('undo')(cmView);
        check(getEditContent().replace(/\r\n/g,'\n')===${JSON.stringify(before)},'undo_restores_original');
        cmHistoryCommand('redo')(cmView);
        check(getEditContent().replace(/\r\n/g,'\n')===${JSON.stringify(before+inserted)},'redo_restores_edit');
        check(await saveEdit()===true,'native_save_receipt');
        check(!getActiveTab().isDirty,'saved_editor_clean');
        const tab=getActiveTab();
        check(true,window.ReadMDSession?.busy?.()?'session_busy_before_close':'session_idle_before_close');
        check(true,'begin_clean_close');await closeTab(tab.id);
        check(!state.tabs.some(t=>t.id===tab.id),'clean_tab_closed');
        check(document.getElementById('close-confirm-modal')?.classList.contains('hidden')===true,'clean_close_has_no_prompt');
        await loadFile(file);
        check(getActiveTab().content.replace(/\r\n/g,'\n')===${JSON.stringify(before+inserted)},'saved_file_reopened');
      }else if(phase==='restore_close'){
        check(state.tabs.length===1&&getActiveTab()?.path===file,'session_restored');
        check(getActiveTab().content.replace(/\r\n/g,'\n')===${JSON.stringify(before+inserted)},'restart_text_exact');
        await toggleEdit();await until(()=>state.editing&&!!cmView);
        check(!hasUnsavedEditorChanges()&&!getActiveTab().isDirty,'restored_editor_clean');
        await closeTab(getActiveTab().id);
        check(state.tabs.length===0,'last_tab_closed');
      }else if(phase==='empty_home'){
        check(state.tabs.length===0&&!getActiveTab(),'empty_session_home');
      }else if(phase==='blocked_rename'){
        await loadFile(file);
        check(await listFolder(${JSON.stringify(path.dirname(file))})===true,'explicit_workspace_listed');
        const tab=getActiveTab();
        check(await window.ReadMDRefactor.rename(tab,'改名候选.md')===false,'group_rename_refused');
        check(tab.path===file&&!tab._renamePending&&!tab._renameInFlight,'no_rename_intent_or_path_change');
        check(!window.ReadMDRefactor.busy(),'rename_barrier_released');
        check(document.getElementById('toast').textContent.includes(window.i18n.t('rename.groupUnavailable')),'platform_reason_visible');
      }else if(phase==='external_conflict'){
        await loadFile(file);await toggleEdit();await until(()=>state.editing&&!!cmView);
        cmView.dispatch({changes:{from:cmView.state.doc.length,insert:${JSON.stringify(unsaved)}}});
        check(hasUnsavedEditorChanges(),'conflict_fixture_dirty');
        window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase:'external_ready',checks:['editor_ready_for_external_write']}}));
        await until(async()=>{const response=await apiFetch('/api/file?p='+encodeURIComponent(file));const value=await response.json();return value.original?.replace(/\r\n/g,'\n')===${JSON.stringify(external)};});
        const saving=saveEdit();
        await until(()=>!document.getElementById('save-conflict-modal').classList.contains('hidden'));
        check(true,'real_save_conflict_dialog');
        const conflictBox=document.getElementById('save-conflict-modal').firstElementChild.getBoundingClientRect();
        check(conflictBox.width>0&&conflictBox.height>0&&conflictBox.left>=0&&conflictBox.top>=0&&conflictBox.right<=innerWidth&&conflictBox.bottom<=innerHeight,'save_conflict_dialog_fits_window');
        document.getElementById('save-conflict-cancel').click();
        check(await saving===false,'external_revision_rejects_save');
        const draft=${JSON.stringify(before+inserted+unsaved)};
        check(getEditContent().replace(/\r\n/g,'\n')===draft&&hasUnsavedEditorChanges(),'failed_save_keeps_user_text');
        const quit=py.request_quit();
        await until(()=>{const modal=document.getElementById('close-confirm-modal');return modal&&!modal.classList.contains('hidden');});
        document.getElementById('close-confirm-cancel').click();
        check(await quit===false,'quit_cancelled');
        check(getEditContent().replace(/\r\n/g,'\n')===draft&&hasUnsavedEditorChanges(),'cancel_keeps_dirty_editor');
        check(await ReadMDRecovery.flush()!==false,'dirty_draft_checkpointed');
      }
      check(await window.ReadMDSession.flush()!==false,'session_acknowledged');
      window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase,checks}}));
      if(phase==='external_conflict')window.ipc.postMessage('readmd:quit'); // Owned synthetic cleanup after proving Cancel.
      else if(phase==='edit_save')window.ipc.postMessage(JSON.stringify({readmd_window:'close'}));
      else await py.request_quit();
    }catch(error){
      const name=String(error.message||'native_workflow_failed');
      checks.push(/^[a-z_]{1,127}$/.test(name)?name:'native_workflow_failed');
      checks.push('native_error_'+String(error.name||'Error').toLowerCase().replace(/[^a-z0-9_]/g,'').slice(0,64));
      window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:false,phase,checks}}));
      window.ipc.postMessage('readmd:quit');
    }
  })();`;
}
async function run(binary,out) {
  assert.equal(process.platform,'darwin','native Mac required');
  assert(path.isAbsolute(binary)&&fs.statSync(binary).isFile(),'explicit prebuilt binary required');
  assert(out.startsWith(allowed+path.sep),'evidence directory must stay in the workspace');
  fs.mkdirSync(out,{mode:0o700});
  const workspace=path.join(out,'documents'),profile=path.join(out,'profile'),temp=path.join(out,'tmp'),nativeHome=path.join(out,'native-home');
  for(const dir of [workspace,profile,temp,nativeHome])fs.mkdirSync(dir,{mode:0o700});
  const file=path.join(workspace,'中文 路径.md');fs.writeFileSync(file,encode(before),{flag:'wx'});
  fs.writeFileSync(path.join(profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1',{flag:'wx',mode:0o600});
  fs.writeFileSync(path.join(profile,'settings.json'),JSON.stringify({lang:'en',closeToTray:false}));
  const report={platform:'macos-'+process.arch,kernelRelease:os.release(),binarySha256:sha(fs.readFileSync(binary)),bootSha256:sha(fs.readFileSync(path.join(root,'assets/readmd.boot.js'))),harnessSha256:sha(fs.readFileSync(__filename)),nativeHost:'Cocoa WKWebView',productionCsp:true,strictTokenMode:true,chooserOrAuthShim:false,driver:'debug-only native-qa; direct production UI function calls',workspaceWriteSandbox:true,controlledCleanupAfterDirtyQuitCancel:true,phases:[],passed:false};
  const policy='(version 1)(allow default)(deny file-write* (require-all (require-not (subpath '+JSON.stringify(allowed)+')) (require-not (subpath '+JSON.stringify('/System/Volumes/Data'+allowed)+')) (require-not (literal "/dev/null")) (require-not (literal "/dev/tty"))))';
  try {
    for(const phase of ['edit_save','restore_close','empty_home','blocked_rename','external_conflict']) {
      if(phase==='blocked_rename')fs.writeFileSync(path.join(workspace,'links.md'),'[说明](<中文 路径.md>)\r\n',{flag:'wx'});
      const script=driver(phase,file);
      new vm.Script(script,{filename:'native-qa.js'});
      fs.writeFileSync(path.join(profile,'native-qa.js'),script,{mode:0o600});
      const fd=fs.openSync(path.join(out,phase+'.log'),'wx');
      // An app bundle must locate its own assets. A loose debug binary uses
      // the repository assets explicitly. Neither path uses a fallback browser.
      const inBundle=binary.includes('.app/Contents/MacOS/');
      const child=spawn('/usr/bin/sandbox-exec',['-p',policy,binary,'--port','0','--require-token','--data-dir',profile,'--workspace',workspace,...(inBundle?[]:['--assets',path.join(root,'assets')])],{cwd:out,env:{...process.env,READMD_NATIVE_QA:'1',READMD_STARTUP_PROBE:'1',READMD_DATA_DIR:profile,READMD_WORKSPACE:workspace,READMD_EXTRA_ROOTS:'',READMD_ASSETS_DIR:'',TMPDIR:temp,SQLITE_TMPDIR:temp,CFFIXED_USER_HOME:nativeHome},stdio:['ignore',fd,fd]});
      fs.closeSync(fd);
      const timeout=setTimeout(()=>child.kill('SIGTERM'),60000);
      const resultPath=path.join(profile,'native-qa-result.json');
      const exited=new Promise(resolve=>{child.once('error',()=>resolve({code:null,signal:null,spawnError:true}));child.once('exit',(code,signal)=>resolve({code,signal}));});
      let outcome;
      try {
        if(phase==='external_conflict'){
          const deadline=Date.now()+30000;
          while(!fs.existsSync(resultPath)&&child.exitCode===null&&child.signalCode===null&&Date.now()<deadline)await new Promise(resolve=>setTimeout(resolve,20));
          assert.equal(JSON.parse(fs.readFileSync(resultPath,'utf8')).phase,'external_ready');
          fs.renameSync(resultPath,path.join(out,'external-ready.json'));
          fs.writeFileSync(file,encode(external));
        }
        outcome=await exited;
      } finally {
        clearTimeout(timeout);
        if(child.exitCode===null&&child.signalCode===null){child.kill('SIGTERM');await exited;}
      }
      const result=fs.existsSync(resultPath)?JSON.parse(fs.readFileSync(resultPath,'utf8')):{ok:false,phase,checks:['native_report_missing']};
      if(fs.existsSync(resultPath))fs.renameSync(resultPath,path.join(out,phase+'.json'));
      report.phases.push({...result,...outcome});
      assert.equal(outcome.code,0,'owned native process must exit normally');
      assert.equal(result.ok,true,'native workflow failed: '+result.checks.join(','));
      assert.deepEqual(fs.readFileSync(file),encode(phase==='external_conflict'?external:before+inserted),'exact bytes preserve saves and external revisions');
      assert.deepEqual(fs.readdirSync(workspace).sort(),(['blocked_rename','external_conflict'].includes(phase)?['links.md','中文 路径.md']:['中文 路径.md']).sort(),'no extra document or resource directory');
      if(['blocked_rename','external_conflict'].includes(phase))assert.equal(fs.readFileSync(path.join(workspace,'links.md'),'utf8'),'[说明](<中文 路径.md>)\r\n');
    }
    report.passed=true;
  } finally {
    fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n');
    // Retain the documents/profile for independent evidence; these are the
    // only rebuildable temporary caches and are never followed through links.
    for(const dir of [temp,nativeHome])fs.rmSync(dir,{recursive:true,force:true});
  }
  console.log(JSON.stringify({nativeWorkflowPassed:report.passed,phases:report.phases.length,checks:report.phases.reduce((n,p)=>n+p.checks.length,0)}));
}
if(require.main===module){
  const args=process.argv.slice(2),binary=args[0],out=args[1];
  if(!binary||!out)throw Error('Usage: macos-workflow-native.cjs <absolute debug native-qa binary> <new absolute evidence directory>');
  run(path.resolve(binary),path.resolve(out)).catch(error=>{console.error(error.message);process.exitCode=1;});
}
