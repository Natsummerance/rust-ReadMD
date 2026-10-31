'use strict';
// Real native panel readability and screenshots, including late locale loading.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),vm=require('node:vm'),{spawnSync}=require('node:child_process');
const {prepare}=require('./macos-native-system-host.cjs'),root=path.resolve(__dirname,'..'),workspace=path.dirname(root),sha=b=>crypto.createHash('sha256').update(b).digest('hex'),pause=ms=>new Promise(r=>setTimeout(r,ms));
const cases=[['zh-small-light',1024,680,'light','zh-CN',false,false],['zh-medium-dark',1160,820,'dark','zh-CN',true,false],['ar-small-dark',1024,680,'dark','ar',false,false],['de-medium-light',1160,820,'light','de',false,false],['en-installed',1024,680,'light','en',false,true]];
function driver(ack){return String.raw`(async()=>{
 const checks=[],until=async(fn,key)=>{const end=Date.now()+30000;while(Date.now()<end){py.window_control('status');if(await fn())return;await new Promise(r=>setTimeout(r,40));}throw Error(key||'layout_timeout');};
 const check=(value,key)=>{if(!value)throw Error(key);checks.push(key);};
 try{
  await __readmdSessionReady;py.window_control('qa_tray',{id:1,action:'plugins'});await until(()=>window.__readmdNativeTrayResult?.id===1&&document.querySelectorAll('.plugin-card').length===14);
  for(const [name,width,height,theme,lang,expanded,installed]of ${JSON.stringify(cases)}){
   py.window_control('qa_window_size',{width,height});await until(()=>Math.abs(innerWidth-width)<2&&Math.abs(innerHeight-height)<2);
   state.theme=theme;applySettings();await i18n.setLanguage(lang,false);await refreshPluginList();await window.refreshOfflineCapabilityPack();
   if(installed){const r=await apiFetch('/api/plugins/install',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({plugin_id:'charset_normalizer'})});check(r.ok,'native_profile_install_request');await until(async()=>{const r=await(await apiFetch('/api/plugins/list')).json();return r.plugins.charset_normalizer.installed;});await refreshPluginList();$('plugin-search').value='charset';$('plugin-search').dispatchEvent(new Event('input',{bubbles:true}));}
   const details=$('plugin-capability-pack').querySelector('details');check(!!details,'capability_details_available');if(details.open!==expanded)details.querySelector('summary').click();
   await until(()=>!$('plugin-modal').textContent.includes('plugin.'),'raw_translation_key_visible');await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
   const box=$('plugin-box').getBoundingClientRect(),grid=$('plugin-cards-grid'),head=grid.querySelector('.plugin-card-head').getBoundingClientRect(),card=grid.querySelector('.plugin-card').getBoundingClientRect();
   check(box.top>=0&&box.bottom<=innerHeight+1,'panel_fits_'+name.replaceAll('-','_'));check(head.width>box.width/2,'readable_text_column_'+lang.replaceAll('-','_').toLowerCase());check(card.bottom<=grid.getBoundingClientRect().bottom+1,'first_row_fully_visible_'+lang.replaceAll('-','_').toLowerCase());check(grid.scrollWidth<=grid.clientWidth+1,'no_horizontal_overflow_'+lang.replaceAll('-','_').toLowerCase());
   if(installed){const input=grid.querySelector('input[data-action="toggle"]');check(input?.checked&&input.closest('label').getBoundingClientRect().height>=44,'visible_native_switch_hit_target');check(!grid.textContent.includes(i18n.t('plugin.cacheReady')),'native_profile_does_not_claim_model_readiness');}
   check(expanded===details.open,'resource_details_disclosure');
   window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase:name.replaceAll('-','_'),checks:['actual_native_plugin_panel','translated_dynamic_content','readable_row_width','first_row_fully_visible','no_horizontal_overflow'],window_number:window.__readmdNativeWindowState.chrome.windowNumber}}));
   await until(async()=>{const r=await apiFetch('/api/file?p='+encodeURIComponent(${JSON.stringify(ack)}));return r.ok&&(await r.json()).content?.includes(name);});
  }
  const m=await(await apiFetch('/api/modules')).json();check(m.modules.ocr==='idle'&&m.modules.transcribe==='idle','model_services_remain_idle');window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase:'complete',checks}}));py.window_control('exit');
 }catch(error){window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:false,phase:'layout_failed',checks:[/^[a-z_0-9]{1,110}$/.test(error.message)?error.message:'layout_failed']}}));window.ipc.postMessage('readmd:quit');}
})()`;}
async function run(app,out){
 assert(out.startsWith(workspace+path.sep));fs.mkdirSync(out,{mode:0o700});const docs=path.join(out,'documents'),profile=path.join(out,'profile'),tmp=path.join(out,'tmp'),home=path.join(out,'native-home');for(const p of[docs,profile,tmp,home])fs.mkdirSync(p,{mode:0o700});
 fs.writeFileSync(path.join(profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1',{mode:0o600});fs.writeFileSync(path.join(profile,'settings.json'),JSON.stringify({lang:'zh-CN',closeToTray:false,autoCheckUpdate:false}));
 const ack=path.join(docs,'capture-ack.txt'),script=driver(ack);new vm.Script(script);fs.writeFileSync(path.join(profile,'native-qa.js'),script,{mode:0o600});
 const host=prepare(app,out,{tmp,home}),report={nativeHost:'LaunchServices / Cocoa WKWebView',modelsRequested:false,strictAuth:true,productionCsp:true,binarySha256:host.binarySha256,bootSha256:sha(fs.readFileSync(path.join(root,'assets/readmd.boot.js'))),harnessSha256:sha(fs.readFileSync(__filename)),cases:[],passed:false},instance=await host.launch({profile,docs,log:path.join(out,'native.log')}),resultFile=path.join(profile,'native-qa-result.json');
 try{for(const [name,width,height,theme,lang]of cases){let r;const end=Date.now()+45000;while(Date.now()<end){if(fs.existsSync(resultFile)){r=JSON.parse(fs.readFileSync(resultFile));assert(r.ok,r.checks.join(','));if(r.phase===name.replaceAll('-','_'))break;}assert(!instance.launcherStopped,'native app exited before capture');await pause(50);}assert.equal(r?.phase,name.replaceAll('-','_'));
   const probe=spawnSync(path.join(workspace,'.readmd-local/native-window-capture'),[String(instance.pid),String(r.window_number)],{encoding:'utf8',timeout:10000});assert.equal(probe.status,0);const owned=JSON.parse(probe.stdout);assert(owned.ownedWindowOnScreen&&owned.captureAllowed);const png=path.join(out,name+'.png'),capture=spawnSync('/usr/sbin/screencapture',['-x','-o','-l',String(r.window_number),png],{timeout:10000});assert.equal(capture.status,0);const b=fs.readFileSync(png);report.cases.push({name,width,height,theme,lang,screenshotSha256:sha(b),pixelWidth:b.readUInt32BE(16),pixelHeight:b.readUInt32BE(20)});fs.renameSync(resultFile,path.join(out,name+'.json'));fs.writeFileSync(ack,name);}
  await instance.wait();report.result=JSON.parse(fs.readFileSync(resultFile));assert.equal(report.result.ok,true,report.result.checks.join(','));assert.equal(report.result.phase,'complete');report.passed=true;
 }finally{await instance.cleanup();if(fs.existsSync(ack))fs.unlinkSync(ack);for(const p of[tmp,home])fs.rmSync(p,{recursive:true,force:true});fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx',mode:0o600});}
 console.log(JSON.stringify({nativePluginLayoutPassed:report.passed,cases:report.cases.length,checks:report.result.checks.length,modelsRequested:false}));
}
if(require.main===module){assert.equal(process.argv.length,4);run(...process.argv.slice(2).map(p=>path.resolve(p))).catch(e=>{console.error(e.message);process.exitCode=1;});}
