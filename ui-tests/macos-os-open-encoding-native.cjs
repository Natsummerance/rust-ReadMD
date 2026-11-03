'use strict';
// Actual LaunchServices file deliveries and the production native editor.
// Inputs cover newline/encoding boundaries; every original byte is an oracle.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict'),vm=require('node:vm');
const {spawnSync}=require('node:child_process'),{prepare}=require('./macos-native-system-host.cjs');
const root=path.resolve(__dirname,'..'),workspace=path.dirname(root),sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const body='# 原生编码\n\n中文与 emoji 🙂。\n';
const inputs=[['恢复原页 LF.md',Buffer.from(body)],['Finder 中文 😀 BOM.md',Buffer.from('\ufeff'+body.replace(/\n/g,'\r\n'))],
 ['Finder 100% # CR.md',Buffer.from(body.replace(/\n/g,'\r'))],['Finder UTF16 LE.md',Buffer.from('\ufeff'+body.replace(/\n/g,'\r\n'),'utf16le')],
 ['Finder UTF16 BE.MD',Buffer.from('\ufeff'+body,'utf16le').swap16()],['Finder 空正文.md',Buffer.alloc(0)]];
function driver(phase,files){return String.raw`(async()=>{
 const phase=${JSON.stringify(phase)},files=${JSON.stringify(files)},checks=[];
 const check=(ok,name)=>{if(!ok)throw Error(name);checks.push(name);};
 const until=async fn=>{const end=Date.now()+45000;while(Date.now()<end){if(await fn())return;await new Promise(r=>setTimeout(r,40));}throw Error('native_os_open_condition_timeout');};
 try{
  await __readmdSessionReady;check(hasPy&&document.body.classList.contains('native-macos'),'actual_native_host');
  if(phase==='cold_os_open'){
   await until(()=>state.tabs.length===files.length&&!ReadMDSession.busy());
   check(state.tabs.every((tab,i)=>tab.path===files[i].path),'restored_and_os_delivered_identities_preserved_in_order');
   check(getActiveTab()?.path===files.at(-1).path,'last_explicit_os_target_active');
   check(!state.tabs.some(t=>t.isDirty||t.isVirtual),'native_open_never_creates_dirty_copies');
  }else if(phase==='reopen_encode'){
   check(state.tabs.length===files.length&&state.tabs.every((t,i)=>t.path===files[i].path),'all_six_identities_survive_actual_restart');
   for(let i=0;i<files.length;i++){
    const tab=state.tabs.find(t=>t.path===files[i].path);await switchTab(tab.id);if(!state.editing)await toggleEdit();
    check(getEditContent().replace(/\r\n?/g,'\n')===files[i].text&&!hasUnsavedEditorChanges(),'encoding_'+i+'_opens_edit_clean');
    const from=cmView.state.doc.length;cmView.dispatch({changes:{from,insert:'\n原生追加。\n'}});
    check(await saveEdit()===true&&!hasUnsavedEditorChanges(),'encoding_'+i+'_visible_body_saved');
    check(ReadMDReview.history(false)===true,'encoding_'+i+'_undo_accepted');
    await until(()=>!ReadMDReview.busy(tab)&&getEditContent().replace(/\r\n?/g,'\n')===files[i].text);
    check(await saveEdit()===true&&!hasUnsavedEditorChanges(),'encoding_'+i+'_undo_saved');
    check(await confirmExitEdit()===true&&!state.editing,'encoding_'+i+'_clean_exit_without_prompt');
   }
   await closeAllTabs();check(state.tabs.length===0,'all_clean_os_pages_close_after_session_ack');
  }else check(state.tabs.length===0&&!getActiveTab(),'empty_session_reopens_home');
  check(await ReadMDSession.flush()===true,'actual_session_acknowledged');
  const m=await(await apiFetch('/api/modules')).json();check(m.modules.ocr==='idle'&&m.modules.transcribe==='idle','model_services_deferred');
  window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase,checks}}));await py.request_quit();
 }catch(error){checks.push('failed_'+(/^[a-z0-9_]{1,100}$/.test(error.message)?error.message:'native_os_open_operation'));window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:false,phase,checks}}));window.ipc.postMessage('readmd:quit');}
})();`;}
async function run(app,out){
 assert(out.startsWith(workspace+path.sep));fs.mkdirSync(out,{mode:0o700});const dirs=Object.fromEntries(['documents','profile','tmp','native-home'].map(n=>[n,path.join(out,n)]));for(const d of Object.values(dirs))fs.mkdirSync(d,{mode:0o700});
 const files=inputs.map(([name,bytes],i)=>({path:path.join(dirs.documents,name),text:i===5?'':body}));inputs.forEach(([,bytes],i)=>fs.writeFileSync(files[i].path,bytes,{flag:'wx'}));
 fs.writeFileSync(path.join(dirs.profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1',{flag:'wx'});
 fs.writeFileSync(path.join(dirs.profile,'settings.json'),JSON.stringify({lang:'zh-CN',closeToTray:false,autoCheckUpdate:false,petEnabled:false,session:{version:1,tabs:[{key:'file:restore-encoding',path:files[0].path,name:inputs[0][0],dirty:false,source:'file'}],active:'file:restore-encoding',folder:''}}));
 const report={nativeHost:'LaunchServices / actual OS file URLs / Cocoa WKWebView',productionCsp:true,strictAuth:true,modelsRequested:false,bootSha256:sha(fs.readFileSync(path.join(root,'assets/readmd.boot.js'))),harnessSha256:sha(fs.readFileSync(__filename)),phases:[],passed:false};
 try{const host=prepare(app,out,{tmp:dirs.tmp,home:dirs['native-home']});report.binarySha256=host.binarySha256;
  for(const phase of ['cold_os_open','reopen_encode','empty']){
   const script=driver(phase,files);new vm.Script(script);fs.writeFileSync(path.join(dirs.profile,'native-qa.js'),script);
   const instance=await host.launch({profile:dirs.profile,docs:dirs.documents,log:path.join(out,phase+'.log'),...(phase==='cold_os_open'?{openFile:files[1].path}:{})});const entry={phase,host:instance.receipt,passed:false};report.phases.push(entry);
   try{
    if(phase==='cold_os_open'){
     const env={READMD_NATIVE_QA:'1',READMD_DATA_DIR:dirs.profile,READMD_WORKSPACE:dirs.documents,READMD_EXTRA_ROOTS:'',CFFIXED_USER_HOME:dirs['native-home'],TMPDIR:dirs.tmp,SQLITE_TMPDIR:dirs.tmp};
     const args=[...Object.entries(env).flatMap(([k,v])=>['--env',k+'='+v]),'-a',app,...files.slice(2).map(f=>f.path),'--args','--port','0','--require-token','--data-dir',dirs.profile,'--workspace',dirs.documents,'--assets',path.join(root,'assets')];
     assert.equal(spawnSync('/usr/bin/open',args,{timeout:15000,stdio:'ignore'}).status,0);
    }
    await instance.wait(120000);const f=path.join(dirs.profile,'native-qa-result.json');entry.result=JSON.parse(fs.readFileSync(f));fs.renameSync(f,path.join(out,phase+'.json'));assert.equal(entry.result.ok,true,entry.result.checks.join(','));
    for(let i=0;i<files.length;i++)assert.deepEqual(fs.readFileSync(files[i].path),inputs[i][1],'encoding '+i+' exact original bytes');entry.passed=true;
   }finally{await instance.cleanup();}
  }report.passed=true;
 }finally{for(const n of ['tmp','native-home'])fs.rmSync(dirs[n],{recursive:true,force:true});fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});}
 console.log(JSON.stringify({nativeOsOpenEncodingPassed:report.passed,phases:report.phases.length,checks:report.phases.reduce((n,p)=>n+(p.result?.checks.length||0),0)}));
}
if(require.main===module){const [app,out]=process.argv.slice(2);run(path.resolve(app),path.resolve(out)).catch(e=>{console.error(e.message);process.exitCode=1;});}
module.exports={driver};
