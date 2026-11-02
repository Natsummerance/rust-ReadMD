'use strict';
// Real WKWebView, strict authentication and durable files. The only startup
// gate delays the original session GET response; it supplies no fake data.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {prepare}=require('./macos-native-system-host.cjs');
const root=path.resolve(__dirname,'..'),workspace=path.dirname(root),sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const original='# 中文关闭回归\n\n原件必须保留。🙂\n',saved='\n保存后重新打开。🧭\n',draft='\n仅丢弃这一句。\n',incomingSaved='\n后台标签的保存目标。\n';
const encode=text=>Buffer.from('\ufeff'+text.replace(/\n/g,'\r\n'));
function driver(phase,files){return String.raw`(async()=>{
 const phase=${JSON.stringify(phase)},files=${JSON.stringify(files)},checks=[];
 const check=(ok,name)=>{if(!ok)throw Error(name);checks.push(name);};
 const until=async fn=>{const end=Date.now()+15000;while(Date.now()<end){if(await fn())return;await new Promise(r=>setTimeout(r,30));}throw Error('native_condition_timeout');};
 const text=value=>String(value||'').replace(/^\uFEFF/,'').replace(/\r\n?/g,'\n');
 const choose=async(pending,id)=>{await until(()=>!$('close-confirm-modal').classList.contains('hidden'));$(id).click();return pending;};
 try{
  check(hasPy&&document.body.classList.contains('native-macos'),'real_native_mac_bridge');
  if(phase==='startup_and_discard'){
   await until(()=>window.__closeStartupGate?.waiting);
   const opening=loadFile(files.incoming);
   await new Promise(r=>setTimeout(r,150));
   const deferred=state.tabs.length===0;
   window.__closeStartupGate.release();await __readmdSessionReady;await opening;
   check(deferred,'finder_open_waited_for_authoritative_restore');
   check(state.tabs.length===2&&state.tabs.some(t=>t.path===files.original)&&getActiveTab()?.path===files.incoming,'old_and_incoming_pages_preserved');
   check(await ReadMDSession.flush()===true,'session_still_write_eligible');
   const incoming=getActiveTab();document.querySelector('.tab-item.active .tab-close').click();await until(()=>!state.tabs.includes(incoming));
   check(getActiveTab()?.path===files.original,'real_tab_close_activates_previous_page');
   await toggleEdit();check(!hasUnsavedEditorChanges(),'unchanged_edit_starts_clean');
   cmView.dispatch({changes:{from:cmView.state.doc.length,insert:${JSON.stringify(draft)}}});
   const dirty=getEditContent(),firstExit=confirmExitEdit();
   await until(()=>!$('close-confirm-modal').classList.contains('hidden'));
   check(await confirmExitEdit()===false,'repeated_exit_does_not_replace_pending_decision');
   check(!$('close-confirm-modal').classList.contains('hidden'),'first_confirmation_still_visible');
   $('close-confirm-cancel').click();check(await firstExit===false,'cancel_refuses_editor_exit');
   check(state.editing&&getEditContent()===dirty,'cancel_preserves_exact_visible_draft');
   check(await choose(confirmExitEdit(),'close-confirm-discard')===true,'discard_exit_acknowledged');
   check(!state.editing&&!getActiveTab().isDirty&&text(getActiveTab().content)===${JSON.stringify(original)},'discard_restores_saved_preview');
   const history=await(await apiFetch('/api/documents/history')).json();
   const discarded=history.entries.find(e=>e.kind==='discarded');check(!!discarded,'discarded_text_retained_in_history');
   const recovery=await(await apiFetch('/api/documents/history',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({op:'read',id:discarded.id})})).json();
   check(text(recovery.content)===${JSON.stringify(original+draft)},'recoverable_discard_contains_exact_draft');
   await toggleEdit();cmView.dispatch({changes:{from:cmView.state.doc.length,insert:${JSON.stringify(draft)}}});
   const closing=closeTab(getActiveTab().id);await choose(closing,'close-confirm-discard');
   check(state.tabs.length===0&&!state.editing,'dirty_discard_closes_last_tab_and_editor');
   await loadFile(files.original);await toggleEdit();cmView.dispatch({changes:{from:cmView.state.doc.length,insert:${JSON.stringify(saved)}}});
   check(await saveEdit()===true,'real_save_committed');
   const owner=getActiveTab();await closeTab(owner.id);check(!state.tabs.includes(owner),'saved_editor_closes_without_prompt');
   await loadFile(files.original);check(text(getActiveTab().content)===${JSON.stringify(original+saved)},'saved_body_reopens_exactly');
   const kept=getActiveTab();await loadFile(files.incoming);await toggleEdit();
   cmView.dispatch({changes:{from:cmView.state.doc.length,insert:${JSON.stringify(incomingSaved)}}});
   check(await ReadMDReview.flush()===true,'background_draft_review_persisted');
   syncActiveTabDirty();const background=getActiveTab();exitEdit();await switchTab(kept.id);
   check(background.isDirty&&getActiveTab()===kept,'dirty_background_target_is_distinct');
   await choose(closeTab(background.id),'close-confirm-save');
   check(state.tabs.length===1&&getActiveTab()===kept&&!state.tabs.includes(background),'background_save_closes_only_its_target');
   const savedBackground=await(await apiFetch('/api/file?p='+encodeURIComponent(files.incoming))).json();
   check(text(savedBackground.original)===${JSON.stringify('# Incoming\n'+incomingSaved)},'background_save_commits_exact_target_body');
  }else{
   await __readmdSessionReady;
   if(phase==='reopen'){
    check(state.tabs.length===1&&getActiveTab()?.path===files.original,'new_process_restores_only_remaining_page');
    check(text(getActiveTab().content)===${JSON.stringify(original+saved)},'restart_preserves_saved_body');
    await toggleEdit();check(!hasUnsavedEditorChanges(),'reopened_editor_clean');
    await closeTab(getActiveTab().id);check(state.tabs.length===0&&!state.editing,'reopened_clean_tab_closes');
   }else check(state.tabs.length===0&&!getActiveTab(),'closed_session_reopens_home');
  }
  check(await ReadMDSession.flush()===true,'final_session_persisted');
  window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase,checks}}));
  await py.request_quit();
 }catch(error){window.__closeStartupGate?.release();checks.push('failed_'+String(error.message).replace(/[^a-z0-9_]/g,'_').slice(0,90));window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:false,phase,checks}}));window.ipc.postMessage('readmd:quit');}
})();`;}
async function run(app,out,baselineBoot){
 assert(out.startsWith(workspace+path.sep));fs.mkdirSync(out,{mode:0o700});
 const dirs=Object.fromEntries(['profile','documents','tmp','native-home','assets'].map(n=>{const p=path.join(out,n);fs.mkdirSync(p);return[n,p];}));
 fs.cpSync(path.join(root,'assets'),dirs.assets,{recursive:true});
 if(baselineBoot)fs.copyFileSync(baselineBoot,path.join(dirs.assets,'readmd.boot.js'));
 const bootSha256=sha(fs.readFileSync(path.join(dirs.assets,'readmd.boot.js')));
 const index=path.join(dirs.assets,'index.html');let html=fs.readFileSync(index,'utf8');assert(html.includes('<head>'));
 fs.writeFileSync(index,html.replace('<head>','<head><script src="/close-startup-gate.js"></script>'));
 fs.writeFileSync(path.join(dirs.assets,'close-startup-gate.js'),`(()=>{const original=window.fetch;let release;const held=new Promise(r=>release=r);const gate=window.__closeStartupGate={waiting:false,release};window.fetch=function(...args){const p=original.apply(this,args);try{if(new URL(typeof args[0]==='string'?args[0]:args[0].url,location.href).pathname==='/api/session'&&(!args[1]?.method||args[1].method==='GET')){gate.waiting=true;return p.then(async response=>{await held;return response;});}}catch(_){}return p;};})();`);
 const files={original:path.join(dirs.documents,'中文 原件.md'),incoming:path.join(dirs.documents,'Finder 新打开.md')};
 fs.writeFileSync(files.original,encode(original));fs.writeFileSync(files.incoming,'# Incoming\n');
 fs.writeFileSync(path.join(dirs.profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1');
 fs.writeFileSync(path.join(dirs.profile,'settings.json'),JSON.stringify({lang:'zh-CN',closeToTray:false,autoCheckUpdate:false,petEnabled:false,session:{version:1,tabs:[{key:'file:synthetic-original',path:files.original,name:'中文 原件.md',dirty:false,source:'file'}],active:'file:synthetic-original',folder:''}}));
 const report={nativeHost:'LaunchServices / Cocoa WKWebView',bootSha256,productionCsp:true,strictAuthentication:true,modelsRequested:false,realResponseDelayOnly:true,phases:[],passed:false};
 try{
  const host=prepare(app,out,{tmp:dirs.tmp,home:dirs['native-home'],assets:dirs.assets});report.binarySha256=host.binarySha256;
  for(const phase of ['startup_and_discard','reopen','empty']){
   fs.writeFileSync(path.join(dirs.profile,'native-qa.js'),driver(phase,files));
   const instance=await host.launch({profile:dirs.profile,docs:dirs.documents,log:path.join(out,phase+'.log')});
   try{
    // Only the first process holds the session response. Later cold starts
    // use exactly the shipping index and assets, without the passive gate.
    await instance.wait(60000);const resultFile=path.join(dirs.profile,'native-qa-result.json');const result=JSON.parse(fs.readFileSync(resultFile));fs.renameSync(resultFile,path.join(out,phase+'.json'));
    report.phases.push({phase,result,host:instance.receipt});assert.equal(result.ok,true,result.checks.join(','));
    assert.deepEqual(fs.readFileSync(files.original),encode(original+saved),'saved BOM and CRLF preserved');
    assert.equal(fs.readFileSync(files.incoming,'utf8'),'# Incoming\n'+incomingSaved);
   }finally{await instance.cleanup();}
   fs.writeFileSync(index,html);
  }
  report.passed=true;
 }finally{
  fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx',mode:0o600});
  for(const n of ['tmp','native-home','assets'])fs.rmSync(dirs[n],{recursive:true,force:true});
 }
 console.log(JSON.stringify({nativeClosePassed:true,phases:report.phases.length,checks:report.phases.reduce((n,p)=>n+p.result.checks.length,0),bootSha256}));
}
if(require.main===module){const[app,out,boot]=process.argv.slice(2);run(path.resolve(app),path.resolve(out),boot&&path.resolve(boot)).catch(e=>{console.error(e.message);process.exitCode=1;});}
module.exports={driver,encode,original,saved};
