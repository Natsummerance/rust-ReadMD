'use strict';
// Actual Cocoa standard buttons, real WKWebView, production auth/CSP. No chooser,
// window or fullscreen substitute. Native-only actions are debug feature gated.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict'),vm=require('node:vm');
const {spawn,spawnSync}=require('node:child_process');
const root=path.resolve(__dirname,'..'),workspace=path.dirname(root),sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const source='# Native window contract\n\nA synthetic document with reliable save and close.\n';
function driver(file,ack){return String.raw`(async()=>{
 const checks=[];let phase='window_chrome';
 const check=(ok,name)=>{if(!ok)throw Error(name);checks.push(name);window.ipc.postMessage(JSON.stringify({readmd_native_qa_progress:{ok:true,phase,checks}}));};
 const until=async fn=>{const end=Date.now()+45000;while(Date.now()<end){py.window_control('status');if(await fn())return;await new Promise(r=>setTimeout(r,60));}throw Error('native_window_timeout');};
 const host=()=>window.__readmdNativeWindowState;
 let clickSequence=0;
 const click=async button=>{const id=++clickSequence;py.window_control('qa_native_button',{button,id});await until(()=>window.__readmdNativeButtonResult?.id===id);check(window.__readmdNativeButtonResult.performed,'native_'+button+'_button_invoked');};
 try{
  await window.__readmdSessionReady;await loadFile(${JSON.stringify(file)});
  await until(()=>host()?.chrome?.buttons?.length===3);
  check(py.native_titlebar==='macos'&&document.body.classList.contains('native-macos'),'native_macos_bridge');
  const chrome=host().chrome;
  check(chrome.systemControls&&chrome.fullSizeContent&&chrome.titleHidden&&chrome.transparentTitlebar,'actual_cocoa_titlebar_style');
  check(getComputedStyle($('window-controls')).display==='none'&&!document.querySelector('.window-resize-edge'),'no_duplicate_windows_controls');
  const geometry=()=>{
   const c=host().chrome,header=$('app-titlebar').getBoundingClientRect(),logo=$('titlebar-home').getBoundingClientRect();
   const right=Math.max(...c.buttons.map(b=>b.x+b.width));
   return header.top===0&&Math.abs(header.height-48)<1&&c.buttons.every(b=>b.width>8&&b.height>8&&b.x>=0&&b.y>=0&&b.y+b.height<=48)
    &&c.leadingInset>=right+10&&logo.left>=right+10&&header.right<=innerWidth+1;
  };
  for(const[width,height]of [[1024,680],[1160,820]]){
   py.window_control('qa_window_size',{width,height});await until(()=>Math.abs(innerWidth-width)<2&&Math.abs(innerHeight-height)<2&&geometry());
   check(true,'native_header_'+width);
  }
  const largest={width:Math.min(1920,screen.availWidth-24),height:Math.min(1080,screen.availHeight-24)};
  py.window_control('qa_window_size',largest);await until(()=>Math.abs(innerWidth-largest.width)<2&&Math.abs(innerHeight-largest.height)<2&&geometry());check(true,'native_header_largest_available_screen');
  py.window_control('qa_window_size',{width:1160,height:820});await until(()=>innerWidth===1160&&geometry());
  const logo=$('titlebar-home').getBoundingClientRect(),name=$('window-drag-region').getBoundingClientRect(),actions=$('titlebar-actions').getBoundingClientRect();
  check(logo.right>innerWidth-20&&name.right<=logo.left&&logo.left-name.right<=12&&actions.right<=name.left,'mac_top_row_mirrored_and_right_aligned');
  check($('btn-open').getBoundingClientRect().left>$('btn-folder').getBoundingClientRect().left&&$('btn-folder').getBoundingClientRect().left>$('btn-zen').getBoundingClientRect().left,'mac_global_actions_reverse_in_top_row');
  await i18n.setLanguage('ar',false);await until(geometry);check(true,'rtl_respects_native_left_controls');await i18n.setLanguage('en',false);
  state.theme='dark';applySettings();await until(()=>document.body.dataset.theme==='dark');check(geometry(),'dark_header_fits');
  state.theme='light';applySettings();await until(()=>document.body.dataset.theme==='light');check(geometry(),'light_header_fits');
  const contentTop=$('content').getBoundingClientRect().top,headerHeight=$('app-titlebar').getBoundingClientRect().height,toolbarHeight=$('toolbar').getBoundingClientRect().height;
  toggleZenMode(true);await new Promise(r=>setTimeout(r,280));
  check(contentTop-$('content').getBoundingClientRect().top>=headerHeight+toolbarHeight-2,'zen_releases_header_layout_height');
  toggleZenMode(false);await until(()=>!document.body.classList.contains('zen-mode')&&host().chrome.buttons.every(b=>!b.hidden));
  await click('minimize');await until(()=>host()?.minimized===true);check(true,'actual_native_yellow_minimizes');
  // Restoration must reactivate the reader as well as de-miniaturize it.
  // An inactive restored window left WKWebView hidden and green unresponsive.
  py.window_control('show');await until(()=>host()?.minimized===false&&host()?.visible===true&&host()?.focused===true&&document.visibilityState==='visible');
  check(true,'native_window_restores_and_reactivates_reader');
  await click('fullscreen');await until(()=>host()?.fullscreen===true&&host().chrome.nativeFullscreen&&window.__readmdNativeFullscreen===true);
  check(document.body.classList.contains('zen-mode'),'actual_native_green_enters_fullscreen_and_zen');
  toggleZenMode(false);py.window_control('status');await new Promise(r=>setTimeout(r,180));
  check(host().fullscreen&&!document.body.classList.contains('zen-mode'),'fullscreen_allows_independent_zen_exit');
  // AppKit marks the style/fullscreen intent at the start of its Space
  // animation. A second click there can be ignored even on an enabled button.
  // Wait for the real full-screen viewport to settle before the next action.
  let stableSince=0,lastGeometry='';
  await until(()=>{
   const c=host().chrome,key=[innerWidth,innerHeight,c.contentWidth,c.contentHeight].join(',');
   if(key!==lastGeometry){lastGeometry=key;stableSince=Date.now();}
   return c.nativeFullscreen&&innerWidth>=screen.availWidth-2&&innerHeight>=screen.availHeight-2
    &&Date.now()-stableSince>=1200&&c.buttons.find(b=>b.kind==='fullscreen')?.enabled;
  });
  await click('fullscreen');await until(()=>!host()?.fullscreen&&!host().chrome.nativeFullscreen&&!window.__readmdNativeFullscreen);
  check(!document.body.classList.contains('zen-mode'),'actual_native_green_exit_clears_zen');
  await until(geometry);check(true,'native_buttons_return_to_header_after_fullscreen');
  await toggleEdit();await until(()=>state.editing&&!!cmView);
  cmView.dispatch({changes:{from:cmView.state.doc.length,insert:'\nOwned unsaved sentence.\n'}});
  await until(()=>host().chrome.documentEdited===true);check(true,'native_unsaved_indicator_tracks_real_edit');
  await click('close');await until(()=>!$('close-confirm-modal').classList.contains('hidden'));
  $('close-confirm-cancel').click();await until(()=>$('close-confirm-modal').classList.contains('hidden'));
  check(getEditContent().includes('Owned unsaved sentence.')&&getActiveTab().isDirty,'actual_native_red_cancel_preserves_document');
  check(await saveEdit()===true,'native_save_after_close_cancel');await until(()=>host().chrome.documentEdited===false);check(true,'native_unsaved_indicator_clears_after_save');
  await toggleEdit();await until(()=>!state.editing);
  window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase:'capture_ready',checks:['owned_native_window_ready'],window_number:host().chrome.windowNumber}}));
  await until(async()=>{const response=await apiFetch('/api/file?p='+encodeURIComponent(${JSON.stringify(ack)}));return response.ok;});
  check(await ReadMDSession.flush()!==false,'native_window_session_acknowledged');
  window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase,checks}}));await py.request_quit();
 }catch(error){checks.push(/^[a-z0-9_]{1,127}$/.test(error.message)?error.message:'native_window_failed');checks.push('native_error_'+String(error.name).toLowerCase().replace(/[^a-z0-9_]/g,'').slice(0,64));for(const[name,value]of [['host_fullscreen',host()?.fullscreen],['cocoa_fullscreen',host()?.chrome?.nativeFullscreen],['page_fullscreen',window.__readmdNativeFullscreen],['host_minimized',host()?.minimized],['host_visible',host()?.visible],['page_visible',document.visibilityState==='visible']])checks.push('observed_'+name+'_'+String(!!value));window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:false,phase,checks}}));window.ipc.postMessage('readmd:quit');}
})();`;}
async function run(binary,out){
 assert.equal(process.platform,'darwin');assert(path.isAbsolute(binary)&&fs.statSync(binary).isFile());assert(out.startsWith(workspace+path.sep));fs.mkdirSync(out);
 const docs=path.join(out,'documents'),profile=path.join(out,'profile'),temp=path.join(out,'tmp'),home=path.join(out,'native-home');for(const dir of [docs,profile,temp,home])fs.mkdirSync(dir);
 const file=path.join(docs,'窗口适配与中文文件名_保存和恢复_2026-10-30.md'),ack=path.join(docs,'capture-ack.txt');fs.writeFileSync(file,source);fs.writeFileSync(path.join(profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1');fs.writeFileSync(path.join(profile,'settings.json'),JSON.stringify({lang:'en',closeToTray:false,autoCheckUpdate:false}));
 const script=driver(file,ack);new vm.Script(script);fs.writeFileSync(path.join(profile,'native-qa.js'),script);
 const report={host:'Cocoa WKWebView / actual AppKit standard buttons',productionCsp:true,strictAuth:true,chooserOrWindowShim:false,binarySha256:sha(fs.readFileSync(binary)),bootSha256:sha(fs.readFileSync(path.join(root,'assets/readmd.boot.js'))),driverSha256:sha(fs.readFileSync(__filename)),passed:false};
 const policy='(version 1)(allow default)(deny file-write* (require-all (require-not (subpath '+JSON.stringify(workspace)+')) (require-not (subpath '+JSON.stringify('/System/Volumes/Data'+workspace)+')) (require-not (literal "/dev/null")) (require-not (literal "/dev/tty"))))';
 const fd=fs.openSync(path.join(out,'native.log'),'wx'),child=spawn('/usr/bin/sandbox-exec',['-p',policy,binary,'--port','0','--require-token','--data-dir',profile,'--workspace',docs,'--assets',path.join(root,'assets')],{cwd:out,env:{...process.env,READMD_NATIVE_QA:'1',READMD_DATA_DIR:profile,READMD_WORKSPACE:docs,READMD_ASSETS_DIR:'',READMD_EXTRA_ROOTS:'',TMPDIR:temp,SQLITE_TMPDIR:temp,CFFIXED_USER_HOME:home},stdio:['ignore',fd,fd]});fs.closeSync(fd);
 const result=path.join(profile,'native-qa-result.json');let timer;const exit=new Promise(resolve=>{child.on('error',()=>resolve({code:null,spawnError:true}));child.on('exit',(code,signal)=>resolve({code,signal}));});
 fs.writeFileSync(path.join(out,'owned-process.json'),JSON.stringify({pid:child.pid,binarySha256:report.binarySha256}));
 try{
  timer=setTimeout(()=>child.kill('SIGTERM'),180000);
  const deadline=Date.now()+140000;while(!fs.existsSync(result)&&child.exitCode===null&&child.signalCode===null&&Date.now()<deadline)await new Promise(r=>setTimeout(r,50));
  const first=JSON.parse(fs.readFileSync(result,'utf8'));fs.renameSync(result,path.join(out,first.phase+'.json'));assert.equal(first.ok,true,first.checks.join(','));assert.equal(first.phase,'capture_ready');
  const helper=path.join(workspace,'.readmd-local/native-window-capture');
  if(fs.existsSync(helper)){
   const queried=spawnSync(helper,[String(child.pid),String(first.window_number)],{encoding:'utf8',timeout:10000});if(queried.status===0)report.captureProbe=JSON.parse(queried.stdout);
   if(report.captureProbe?.captureAllowed&&report.captureProbe?.ownedWindowOnScreen){
    const image=path.join(out,'native-window.png');const capture=spawnSync('/usr/sbin/screencapture',['-x','-o','-l',String(first.window_number),image],{timeout:10000,encoding:'utf8'});
    report.screenshot={captured:capture.status===0};if(report.screenshot.captured)report.screenshot.sha256=sha(fs.readFileSync(image));
   }
  }
  fs.writeFileSync(ack,'Owned native capture complete.');
  report.outcome=await exit;report.result=JSON.parse(fs.readFileSync(result,'utf8'));assert.equal(report.outcome.code,0);assert.equal(report.result.ok,true,report.result.checks.join(','));assert.equal(fs.readFileSync(file,'utf8'),source+'\nOwned unsaved sentence.\n');report.passed=true;
 }finally{clearTimeout(timer);if(child.exitCode===null&&child.signalCode===null){child.kill('SIGTERM');await exit;}fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n');for(const dir of [temp,home])fs.rmSync(dir,{recursive:true,force:true});}
 console.log(JSON.stringify({nativeWindowPassed:report.passed,checks:report.result.checks.length,screenshot:report.screenshot?.captured||false}));
}
if(require.main===module){const[binary,out]=process.argv.slice(2);if(!binary||!out)throw Error('Usage: macos-window-native.cjs <native-qa binary> <new owned evidence directory>');run(path.resolve(binary),path.resolve(out)).catch(error=>{console.error(error.message);process.exitCode=1;});}
module.exports={driver};
