'use strict';
// Own Cocoa menus, real plugin profiles and conversion files. No native
// chooser confirmation, network service, model or global input is simulated.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),vm=require('node:vm');
const {prepare}=require('./macos-native-system-host.cjs');
const root=path.resolve(__dirname,'..'),workspace=path.dirname(root),sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const original='# 原生菜单与插件\n\n保护当前正文和保存结果。\n',addition='\n菜单保存的可见正文。🙂\n',input='<html><body><p>A real readable conversion document.</p></body></html>\n';
function driver(phase,file,textFile,converted){return String.raw`(async()=>{
 const phase=${JSON.stringify(phase)},file=${JSON.stringify(file)},checks=[];let id=0;
 const pause=ms=>new Promise(r=>setTimeout(r,ms)),report=()=>window.ipc.postMessage(JSON.stringify({readmd_native_qa_progress:{ok:true,phase,checks}}));
 const check=(ok,key)=>{if(!ok)throw Error(key);checks.push(key);report();};
 const until=async(fn,key)=>{const end=Date.now()+20000;while(Date.now()<end){py.window_control('status');if(await fn())return;await pause(50);}throw Error(key||'menu_wait_timeout');};
 const settled=async test=>{let key='',since=0;await until(()=>{const next=[innerWidth,innerHeight,window.__readmdNativeWindowState?.chrome?.nativeFullscreen].join(',');if(next!==key){key=next;since=Date.now();}return test()&&Date.now()-since>=1200;},'native_space_animation_not_settled');};
 const visible=id=>!!$(id)&&!$(id).classList.contains('hidden');
 const menu=async action=>{const mine=++id;py.window_control('qa_tray',{id:mine,action});await until(()=>window.__readmdNativeTrayResult?.id===mine);const r=window.__readmdNativeTrayResult;check(r.performed,'native_'+action.replaceAll('-','_')+'_dispatched');return r.menu;};
 const json=async(route,body)=>{const r=await apiFetch(route,body?{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)}:undefined);const data=await r.json();if(!r.ok||data.ok===false)throw Error('backend_'+r.status+'_'+(data.error_code||'failed').replace(/[^a-z_0-9]/g,''));return data;};
 const card=()=>document.querySelector('[data-plugin-id="charset_normalizer"]');
 const toggle=()=>card()?.querySelector('input[data-action="toggle"]');
 const idle=async()=>{const r=await(await apiFetch('/api/modules')).json();check(r.modules.ocr==='idle'&&r.modules.transcribe==='idle','model_modules_remain_idle');};
 try{
  await __readmdSessionReady;check(hasPy&&document.body.classList.contains('native-macos'),'actual_native_bridge');
  await loadFile(file);await until(()=>getActiveTab()?.path===file);check(getActiveTab().content===${JSON.stringify(original+addition)},'same_saved_body_loaded');
  const native=await menu('show');check(native.template===true&&native.width===18&&native.height===18,'actual_monochrome_template_image');
  check(['new','folder','plugins','pet','settings','close-document','zen','fullscreen','minimize','hide','select-all','save','save-as','undo','redo'].every(k=>native.commands.includes(k)),'native_command_inventory');
  if(phase==='install_menu'){
   await menu('plugins');await until(()=>visible('plugin-modal')&&document.querySelectorAll('.plugin-card').length===14);
   check(true,'native_menu_opens_real_plugin_panel');
   const listing=await json('/api/plugins/list');check(listing.plugins.easyocr.native.validation==='on_demand','ocr_validation_is_on_demand');
   for(const id of ['whisper','faster_whisper']){const p=listing.plugins[id],c=document.querySelector('[data-plugin-id="'+id+'"]');check(!p.enabled&&p.native.platform_supported===false&&!c.querySelector('[data-action="install"],[data-action="toggle"]'),'unsupported_'+id+'_has_no_fake_activation');}
   await idle();const box=$('plugin-box').getBoundingClientRect(),grid=$('plugin-cards-grid');
   check(box.top>=0&&box.bottom<=innerHeight+1&&grid.clientHeight>0&&grid.scrollHeight>=grid.clientHeight,'plugin_sheet_fits_native_viewport');
   const active=state.activeTabId;await menu('close-document');await until(()=>!visible('plugin-modal'));check(state.activeTabId===active,'native_close_dismisses_panel_without_closing_document');
   await menu('settings');await until(()=>visible('command-palette-modal'));check($('cmdp-input').value==='settings','settings_menu_uses_existing_command_palette');await menu('close-document');await until(()=>!visible('command-palette-modal'));
   await menu('pet');await until(()=>visible('pet-settings-modal'));check(true,'pet_menu_opens_existing_settings_without_starting_pet');await menu('close-document');await until(()=>!visible('pet-settings-modal'));
   await menu('zen');await until(()=>document.body.classList.contains('zen-mode'));await menu('zen');await until(()=>!document.body.classList.contains('zen-mode'));
   const width=innerWidth,height=innerHeight;
   await menu('fullscreen');await settled(()=>window.__readmdNativeFullscreen===true&&innerWidth>=screen.availWidth-2&&innerHeight>=screen.availHeight-2);check(document.body.classList.contains('zen-mode'),'native_menu_fullscreen_enables_zen');
   await menu('zen');await until(()=>!document.body.classList.contains('zen-mode'));check(window.__readmdNativeFullscreen===true,'zen_can_exit_inside_fullscreen');await menu('fullscreen');await settled(()=>window.__readmdNativeFullscreen===false&&Math.abs(innerWidth-width)<=2&&Math.abs(innerHeight-height)<=2);
   await menu('minimize');await until(()=>window.__readmdNativeWindowState?.minimized===true);await menu('show');await until(()=>window.__readmdNativeWindowState?.minimized===false);
   await menu('hide');await pause(300);await menu('show');await until(()=>document.visibilityState==='visible');check(true,'native_show_recovers_application_hide');
   await menu('new');await until(()=>state.activeTabId!==active);check(!getActiveTab().path,'native_new_uses_new_document_service');await menu('close-document');await until(()=>state.activeTabId===active);
   await toggleEdit();await until(()=>state.editing&&!!cmView);cmView.dispatch({changes:{from:cmView.state.doc.length,insert:'\n临时未保存更改。'}});
   await menu('close-document');await until(()=>visible('close-confirm-modal'));$('close-confirm-cancel').click();await until(()=>!visible('close-confirm-modal'));check(hasUnsavedEditorChanges(),'native_document_close_cancel_preserves_user_edit');
   cmView.dispatch({changes:{from:0,to:cmView.state.doc.length,insert:${JSON.stringify(original+addition)}}});await menu('save');await until(()=>!hasUnsavedEditorChanges());check(getEditContent()===${JSON.stringify(original+addition)},'native_save_uses_current_visible_revision');
   await menu('plugins');await until(()=>visible('plugin-modal')&&card()?.querySelector('[data-action="install"]'));card().querySelector('[data-action="install"]').click();await until(()=>visible('confirm-modal'));$('confirm-action').click();await until(()=>toggle()&&!toggle().disabled);
   check(toggle().checked,'real_profile_install_enabled');toggle().click();await until(()=>toggle()&&!toggle().disabled&&!toggle().checked);check(!(await json('/api/plugins/list')).plugins.charset_normalizer.enabled,'disable_persisted_by_real_backend');await idle();
   await menu('close-document');await until(()=>!visible('plugin-modal'));
  }else{
   const listing=await json('/api/plugins/list');check(listing.plugins.charset_normalizer.installed&&!listing.plugins.charset_normalizer.enabled,'installed_disabled_profile_survives_new_process');
   await menu('plugins');await until(()=>visible('plugin-modal')&&toggle()&&!toggle().disabled);check(!toggle().checked,'reopened_panel_matches_persisted_disabled_state');toggle().click();await until(()=>toggle()&&!toggle().disabled&&toggle().checked);
   const converted=await json('/api/convert?p='+encodeURIComponent(${JSON.stringify(textFile)})+'&on_exists=overwrite');check(converted.content?.includes('A real readable conversion document.'),'real_plugin_conversion_returns_body');
   check((await json('/api/plugins/list')).plugins.charset_normalizer.runtime.ok===true,'conversion_records_actual_plugin_execution');
   card().querySelector('[data-action="uninstall"]').click();await until(()=>visible('confirm-modal'));$('confirm-cancel').click();await until(()=>!visible('confirm-modal'));check((await json('/api/plugins/list')).plugins.charset_normalizer.installed,'cancel_uninstall_preserves_profile');
   card().querySelector('[data-action="uninstall"]').click();await until(()=>visible('confirm-modal'));$('confirm-action').click();await until(async()=>!(await json('/api/plugins/list')).plugins.charset_normalizer.installed);check(true,'actual_profile_uninstall_committed');await menu('close-document');await until(()=>!visible('plugin-modal'));
   await loadFile(${JSON.stringify(converted)});await until(()=>getActiveTab()?.path===${JSON.stringify(converted)});check(getActiveTab().content.includes('A real readable conversion document.'),'converted_file_reopens_from_disk');await menu('close-document');await until(()=>getActiveTab()?.path===file);await idle();
  }
  check(await ReadMDSession.flush()!==false,'session_persisted');window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase,checks}}));py.window_control('exit');
 }catch(error){checks.push('failed_'+(/^[a-z_0-9]{1,110}$/.test(error.message)?error.message:'menu_workflow'));window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:false,phase,checks}}));window.ipc.postMessage('readmd:quit');}
})()`;}
async function run(app,out){
 assert(out.startsWith(workspace+path.sep));fs.mkdirSync(out,{mode:0o700});const docs=path.join(out,'documents'),profile=path.join(out,'profile'),tmp=path.join(out,'tmp'),home=path.join(out,'native-home');for(const p of [docs,profile,tmp,home])fs.mkdirSync(p,{mode:0o700});
 const file=path.join(docs,'中文 菜单.md'),text=path.join(docs,'真实 插件转换.html'),converted=path.join(docs,'真实 插件转换.md');fs.writeFileSync(file,original+addition,{flag:'wx'});fs.writeFileSync(text,input,{flag:'wx'});fs.writeFileSync(path.join(profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1',{mode:0o600});fs.writeFileSync(path.join(profile,'settings.json'),JSON.stringify({lang:'en',closeToTray:false,autoCheckUpdate:false,petEnabled:false}));
 const report={nativeHost:'LaunchServices / Cocoa WKWebView / actual NSMenu and NSStatusItem',modelsRequested:false,strictAuth:true,productionCsp:true,bootSha256:sha(fs.readFileSync(path.join(root,'assets/readmd.boot.js'))),harnessSha256:sha(fs.readFileSync(__filename)),phases:[],passed:false};
 try{const host=prepare(app,out,{tmp,home});report.binarySha256=host.binarySha256;
  for(const phase of ['install_menu','restart_plugin']){const script=driver(phase,file,text,converted);new vm.Script(script);fs.writeFileSync(path.join(profile,'native-qa.js'),script,{mode:0o600});const instance=await host.launch({profile,docs,log:path.join(out,phase+'.log')}),entry={phase,host:instance.receipt,passed:false};report.phases.push(entry);
   try{await instance.wait(180000);const r=JSON.parse(fs.readFileSync(path.join(profile,'native-qa-result.json')));fs.renameSync(path.join(profile,'native-qa-result.json'),path.join(out,phase+'.json'));entry.result=r;assert.equal(r.ok,true,r.checks.join(','));assert.equal(fs.readFileSync(file,'utf8'),original+addition);assert.equal(fs.readFileSync(text,'utf8'),input);
    const native=path.join(profile,'plugins/native/charset_normalizer.json'),manifest=JSON.parse(fs.readFileSync(path.join(profile,'plugins/plugins.json')));if(phase==='install_menu'){assert(fs.lstatSync(native).isFile());assert.equal(manifest.charset_normalizer.enabled,false);}else{assert(!fs.existsSync(native));assert.equal(manifest.charset_normalizer.uninstalled,true);assert(fs.readFileSync(converted,'utf8').includes('A real readable conversion document.'));}entry.passed=true;
   }finally{await instance.cleanup();}}
  report.passed=true;
 }finally{fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx',mode:0o600});for(const p of [tmp,home])fs.rmSync(p,{recursive:true,force:true});}
 console.log(JSON.stringify({nativeSystemMenusPassed:report.passed,checks:report.phases.reduce((n,p)=>n+(p.result?.checks.length||0),0),phases:report.phases.length}));
}
if(require.main===module){assert.equal(process.argv.length,4);run(...process.argv.slice(2).map(p=>path.resolve(p))).catch(e=>{console.error(e.message);process.exitCode=1;});}
module.exports={driver};
