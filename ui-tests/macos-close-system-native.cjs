'use strict';
// Real WKWebView, strict authentication and durable files. The only startup
// gate delays the original session GET response; it supplies no fake data.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {prepare}=require('./macos-native-system-host.cjs');
const root=path.resolve(__dirname,'..'),workspace=path.dirname(root),sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const original='# 中文关闭回归\n\n原件必须保留。🙂\n',saved='\n保存后重新打开。🧭\n',draft='\n仅丢弃这一句。\n',incomingSaved='\n后台标签的保存目标。\n';
const encode=text=>Buffer.from('\ufeff'+text.replace(/\n/g,'\r\n'));
function driver(phase,files,{holdNeighborRender=false,lastEntry='button'}={}){return String.raw`(async()=>{
 const phase=${JSON.stringify(phase)},files=${JSON.stringify(files)},checks=[],trace=[],holdNeighborRender=${JSON.stringify(holdNeighborRender)},lastEntry=${JSON.stringify(lastEntry)};
 const renderGate={held:false,release:null};
 const views=new WeakMap();let viewSerial=0;
 const snapshot=()=>{
  const view=typeof cmView!=='undefined'?cmView:null;if(view&&!views.has(view))views.set(view,++viewSerial);
  const owner=window.ReadMDLifecycle?.owner?.();
  return{activeId:state.activeTabId,tabs:state.tabs.map(tab=>({id:tab.id,name:tab.name,dirty:tab.isDirty,renameInFlight:!!tab._renameInFlight})),editing:state.editing,
   view:view?views.get(view):null,viewText:view?.state?.doc?.toString(),sessionBusy:window.ReadMDSession?.busy?.(),
   lifecycle:owner?{name:owner.name,target:owner.target?.id,tabCloseCommitted:!!owner.tabCloseCommitted}:null,bodyInert:document.body.inert,
   rows:Array.from(document.querySelectorAll('#doc-tabs-bar .tab-item')).map(row=>({id:row.dataset.tabId,active:row.classList.contains('active'),disabled:row.querySelector('.tab-close')?.disabled}))};
 };
 const record=(event,detail={})=>{trace.push({at:performance.now(),event,...detail,state:snapshot()});};
 const canonical=value=>Array.isArray(value)?value.map(canonical):value&&typeof value==='object'?Object.fromEntries(Object.keys(value).sort().map(key=>[key,canonical(value[key])])):value;
 const originalClose=closeTab;closeTab=function(id,...args){record('close-call',{target:id,trigger:args[1]||'programmatic'});const pending=originalClose.call(this,id,...args);if(renderGate.held)renderGate.release?.();Promise.resolve(pending).then(result=>record('close-result',{target:id,result:result===undefined?'undefined':result}),error=>record('close-error',{target:id,message:String(error.message)}));return pending;};
 if(typeof requestTabClose==='function'){const originalRequest=requestTabClose;requestTabClose=function(id,trigger){record('ui-close-request',{target:id,trigger});const pending=originalRequest.call(this,id,trigger);if(renderGate.held)renderGate.release?.();return pending;};}
 const originalRender=renderActiveTab;renderActiveTab=function(...args){
  record('render-start');const pending=originalRender.apply(this,args);Promise.resolve(pending).then(()=>record('render-end'));
  if(holdNeighborRender&&phase==='four_tabs'&&state.tabs.length===1&&window.ReadMDLifecycle?.owner?.()?.name==='close-tab'&&!renderGate.release){
   let release;const held=new Promise(resolve=>release=resolve);renderGate.release=release;
   return Promise.resolve(pending).then(async value=>{renderGate.held=true;record('real-render-completion-held');await held;record('real-render-completion-released');return value;});
  }
  return pending;
 };
 const originalFetch=apiFetch;apiFetch=async function(url,options){
  let request;if(url==='/api/session'&&options?.method==='POST'){request=JSON.parse(options.body);record('session-request',{request:{call_id:request.call_id,expected:request.expected_session_version,active:request.session.active,tabs:request.session.tabs.map(tab=>({key:tab.key,name:tab.name,dirty:tab.dirty}))}});}
  const response=await originalFetch.apply(this,arguments);
  if(request)response.clone().json().then(result=>record('session-response',{call_id:request.call_id,status:response.status,error_code:result.error_code||null,ok:result.ok,persisted:result.persisted,version:result.session_version,receiptCallMatches:result.call_id===request.call_id,receiptSessionMatches:JSON.stringify(canonical(result.session))===JSON.stringify(canonical(request.session))})).catch(error=>record('session-response-error',{message:String(error.message)}));
  return response;
 };
 const saveTrace=async()=>{
  const file=await(await originalFetch('/api/file?p='+encodeURIComponent(files.trace))).json();
  const response=await originalFetch('/api/save',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({path:files.trace,content:JSON.stringify({phase,trace},null,2),encoding:'utf-8',expected_revision:file.revision})});
  if(!response.ok)throw Error('owned_trace_save_failed_'+response.status+'_'+((await response.json()).error_code||'unknown'));
 };
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
   check(text(savedBackground.content)===${JSON.stringify('# Incoming\n'+incomingSaved)},'background_save_commits_exact_target_body');
  }else{
   await __readmdSessionReady;
   if(phase==='reopen'){
    check(state.tabs.length===1&&getActiveTab()?.path===files.original,'new_process_restores_only_remaining_page');
    check(text(getActiveTab().content)===${JSON.stringify(original+saved)},'restart_preserves_saved_body');
    await toggleEdit();check(!hasUnsavedEditorChanges(),'reopened_editor_clean');
    await closeTab(getActiveTab().id);check(state.tabs.length===0&&!state.editing,'reopened_clean_tab_closes');
   }else if(phase==='four_tabs'){
    for(const file of files.four)await loadFile(file);
    check(state.tabs.length===4,'four_independent_tabs_open');
    const clickActive=()=>{
     const button=document.querySelector('#doc-tabs-bar .tab-item.active .tab-close'),row=button?.closest('.tab-item'),rect=button?.getBoundingClientRect();
     const hit=rect?document.elementFromPoint(rect.left+rect.width/2,rect.top+rect.height/2):null;
     record('click-close',{target:state.activeTabId,domTarget:row?.dataset.tabId,connected:button?.isConnected,disabled:button?.disabled,
      userReachable:!!button&&!button.disabled&&!button.closest('[inert]')&&!!hit&&(hit===button||button.contains(hit)),hit:hit?.id||hit?.className});
     if(!button)throw Error('active_close_button_missing');
     if(row.dataset.tabId!==state.activeTabId)throw Error('active_close_dom_target_mismatch');
     if(button.disabled||button.closest('[inert]')||!hit||hit!==button&&!button.contains(hit))throw Error('active_close_not_user_reachable');
     button.click();renderGate.release?.();
    };
    const closeActive=async count=>{
     clickActive();
     await until(()=>state.tabs.length===count);
     record('tab-count-reached',{count});
    };
    await closeActive(3);
    check(getActiveTab()?.path===files.four[2],'close_button_activates_expected_neighbor');
    await toggleEdit();
    cmView.dispatch({changes:{from:cmView.state.doc.length,insert:'临时修改'}});
    await choose(confirmExitEdit(),'close-confirm-cancel');
    check(state.editing&&getEditContent().endsWith('临时修改'),'cancel_preserves_third_tab');
    await choose(confirmExitEdit(),'close-confirm-discard');
    check(!state.editing&&!getActiveTab().isDirty,'discard_returns_to_clean_preview');
    await closeActive(2);
    await toggleEdit();
    cmView.dispatch({changes:{from:cmView.state.doc.length,insert:'保留修改'}});
    clickActive();
    await until(()=>!$('close-confirm-modal').classList.contains('hidden'));
    $('close-confirm-save').click();
    await until(()=>state.tabs.length===1);
    check(getActiveTab()?.path===files.four[0],'save_and_close_targets_second_tab');
    if(holdNeighborRender)await until(()=>renderGate.held);
    if(lastEntry==='command'){
     record('native-close-command',{target:state.activeTabId});
     const command=window.__readmdNativeMenu('close-document');await until(()=>state.tabs.length===0);await command;
    }else if(lastEntry==='tab_menu'){
     const row=document.querySelector('#doc-tabs-bar .tab-item.active'),target=state.activeTabId,rect=row.getBoundingClientRect();
     row.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,clientX:rect.left+12,clientY:rect.bottom}));
     const menu=$('tab-context-menu'),button=menu.querySelector('[data-action="close"]'),box=button.getBoundingClientRect(),hit=document.elementFromPoint(box.left+box.width/2,box.top+box.height/2);
     record('click-close-menu',{target,domTarget:menu.dataset.tabId,userReachable:!button.disabled&&!button.closest('[inert]')&&(hit===button||button.contains(hit))});
     if(menu.dataset.tabId!==target||button.disabled||button.closest('[inert]')||hit!==button&&!button.contains(hit))throw Error('close_menu_not_user_reachable');
     button.click();await until(()=>state.tabs.length===0);
    }else await closeActive(0);
    check(!state.editing&&!getActiveTab(),'all_four_closed');
   }else check(state.tabs.length===0&&!getActiveTab(),'closed_session_reopens_home');
  }
  check(await ReadMDSession.flush()===true,'final_session_persisted');
  await saveTrace();
  window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase,checks}}));
  await py.request_quit();
 }catch(error){record('failure',{message:String(error.message)});window.__closeStartupGate?.release();checks.push('failed_'+String(error.message).replace(/[^a-z0-9_]/g,'_').slice(0,90));try{await saveTrace();}catch(_){}window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:false,phase,checks}}));window.ipc.postMessage('readmd:quit');}
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
 const files={original:path.join(dirs.documents,'中文 原件.md'),incoming:path.join(dirs.documents,'Finder 新打开.md'),trace:path.join(dirs.documents,'owned-close-trace.md'),
  four:Array.from({length:4},(_,i)=>path.join(dirs.documents,'四页 '+(i+1)+'.md'))};
 for(const [i,file] of files.four.entries())fs.writeFileSync(file,encode('# 页面 '+(i+1)+'\n'),{flag:'wx'});
 fs.writeFileSync(files.original,encode(original));fs.writeFileSync(files.incoming,'# Incoming\n');
 fs.writeFileSync(files.trace,'{}');
 fs.writeFileSync(path.join(dirs.profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1');
 fs.writeFileSync(path.join(dirs.profile,'settings.json'),JSON.stringify({lang:'zh-CN',closeToTray:false,autoCheckUpdate:false,petEnabled:false,session:{version:1,tabs:[{key:'file:synthetic-original',path:files.original,name:'中文 原件.md',dirty:false,source:'file'}],active:'file:synthetic-original',folder:''}}));
 const holdNeighborRender=process.env.READMD_CLOSE_RENDER_BARRIER==='1';
 const lastEntry=process.env.READMD_CLOSE_LAST_ENTRY||'button';assert(['button','command','tab_menu'].includes(lastEntry));
 const report={nativeHost:'LaunchServices / Cocoa WKWebView',bootSha256,productionCsp:true,strictAuthentication:true,modelsRequested:false,realResponseDelayOnly:true,heldRealNeighborRenderCompletion:holdNeighborRender,lastEntry,phases:[],passed:false};
 try{
  const host=prepare(app,out,{tmp:dirs.tmp,home:dirs['native-home'],assets:dirs.assets});report.binarySha256=host.binarySha256;
  for(const phase of ['startup_and_discard','reopen','four_tabs','empty']){
   fs.writeFileSync(path.join(dirs.profile,'native-qa.js'),driver(phase,files,{holdNeighborRender,lastEntry}));
   const instance=await host.launch({profile:dirs.profile,docs:dirs.documents,log:path.join(out,phase+'.log')});
   try{
    // Only the first process holds the session response. Later cold starts
    // use exactly the shipping index and assets, without the passive gate.
    await instance.wait(60000);const resultFile=path.join(dirs.profile,'native-qa-result.json');const result=JSON.parse(fs.readFileSync(resultFile));fs.renameSync(resultFile,path.join(out,phase+'.json'));
    fs.copyFileSync(files.trace,path.join(out,phase+'.trace.json'));
    report.phases.push({phase,result,host:instance.receipt});assert.equal(result.ok,true,result.checks.join(','));
    assert.deepEqual(fs.readFileSync(files.original),encode(original+saved),'saved BOM and CRLF preserved');
    assert.equal(fs.readFileSync(files.incoming,'utf8'),'# Incoming\n'+incomingSaved);
    if(phase==='four_tabs'||phase==='empty')for(const [i,file] of files.four.entries()){
     assert.deepEqual(fs.readFileSync(file),encode('# 页面 '+(i+1)+'\n'+(i===1?'保留修改':'')),'four-tab disk outcome '+i);
    }
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
