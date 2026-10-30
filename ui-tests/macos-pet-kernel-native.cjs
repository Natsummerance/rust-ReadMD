'use strict';
// Real reader settings -> Rust supervisor -> verified native pet -> durable
// bounds -> kernel settings -> process restart. No AI models or runtime shim.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict'),vm=require('node:vm'),{spawn,spawnSync}=require('node:child_process');
const root=path.resolve(__dirname,'..'),workspace=path.dirname(root),sha=b=>crypto.createHash('sha256').update(b).digest('hex'),pause=ms=>new Promise(r=>setTimeout(r,ms));
function driver(phase,file,ack){return String.raw`(async()=>{
 const phase=${JSON.stringify(phase)},checks=[],ack=${JSON.stringify(ack)};
 const progress=p=>window.ipc.postMessage(JSON.stringify({readmd_native_qa_progress:{ok:true,phase:p,checks}}));
 const check=(ok,name)=>{if(!ok)throw Error(name);checks.push(name);};
 const pause=ms=>new Promise(r=>setTimeout(r,ms));
 const until=async fn=>{const end=Date.now()+35000;while(Date.now()<end){py.window_control('status');if(await fn())return;await pause(60);}throw Error('native_pet_kernel_timeout');};
 const waitAck=async value=>until(async()=>{const r=await apiFetch('/api/file?p='+encodeURIComponent(ack));return r.ok&&(await r.json()).content===value;});
 try{
  await __readmdSessionReady;check(hasPy&&document.body.classList.contains('native-macos'),'actual_native_reader');
  await loadFile(${JSON.stringify(file)});
  if(phase==='enable'){
   await openPetSettings();await until(()=>activePetSettingsStatus?.adapter?.available);
   $('pet-enabled').checked=true;$('pet-runtime').value='desktop';$('pet-renderer').value='hermes-sprite';
   currentActivePetSlug='mochi';$('pet-lock-position').checked=false;$('pet-sound').checked=false;
   const result=await savePetSettings();check(result?.ok===true,'real_pet_settings_configure_succeeded');closePetSettings();
  }
  let status=await fetchPetRuntimeStatus();
  check(status.enabled&&status.in_app===false&&status.adapter?.rust?.running,'real_kernel_supervises_healthy_native_pet');
  check(status.active_slug==='mochi','normal_settings_select_real_pet_character');
  if(phase==='enable'){
   state.closeToTray=true;syncWindowPreferences();py.window_control('close');
   await until(()=>window.__readmdNativeWindowState?.visible===false);
   progress('pet_context_ready');await waitAck('context_sent');
   await until(async()=>{await pollPetControls();return window.__readmdNativeWindowState?.visible===true&&$('more-menu').classList.contains('open');});
   check(true,'native_pet_context_request_wakes_hidden_reader_and_opens_real_menu');closeMoreMenu();
  }
  progress(phase==='enable'?'pet_drag_ready':'pet_restored_ready');await waitAck('native_checked_'+phase);
  if(phase==='enable'){
   const locking=await requestConfigurePet({lock_position:true});check(locking.ok===true,'normal_backend_applies_position_lock');
   progress('pet_lock_ready');await waitAck('lock_checked');
   const disabling=await requestConfigurePet({enabled:false,in_app:false});check(disabling.ok===true,'normal_settings_disable_native_pet');
   progress('pet_disabled');await waitAck('disabled_checked');
   const enabling=await requestConfigurePet({enabled:true,in_app:false,lock_position:false});check(enabling.ok===true,'normal_settings_reenable_native_pet');
   progress('pet_reenabled');await waitAck('reenabled_checked');
  }else{
   state.closeToTray=true;syncWindowPreferences();py.window_control('close');
   await until(()=>{py.window_control('status');return window.__readmdNativeWindowState?.visible===false;});
   progress('reader_hidden_pet_active');await waitAck('hidden_checked');
   py.window_control('show');
  }
  const r=await apiFetch('/api/modules'),m=await r.json();check(m.modules.ocr==='idle'&&m.modules.transcribe==='idle','recognition_models_stay_unloaded');
  check(await ReadMDSession.flush()!==false,'reader_session_persisted');
  window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase,checks}}));
  py.window_control('qa_application',{action:'terminate',id:1});
 }catch(error){checks.push('failed_'+(/^[a-z_]{1,110}$/.test(error.message)?error.message:'native_pet_kernel'));window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:false,phase,checks}}));window.ipc.postMessage('readmd:quit');}
})()`;}
async function run(binary,petBinary,fixture,out){
 assert.equal(process.platform,'darwin');assert(out.startsWith(workspace+path.sep));fs.mkdirSync(out,{mode:0o700});
 const docs=path.join(out,'documents'),profile=path.join(out,'profile'),temp=path.join(out,'tmp'),home=path.join(out,'native-home'),plugins=path.join(profile,'plugins');for(const p of [docs,profile,temp,home,plugins])fs.mkdirSync(p,{mode:0o700});
 const file=path.join(docs,'桌宠保存恢复专用.md'),ack=path.join(docs,'native-ack.md');fs.writeFileSync(file,'# 原生桌宠\n\n这是专用验证材料，不含私人文档。\n');
 fs.writeFileSync(path.join(profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1',{mode:0o600});fs.writeFileSync(path.join(plugins,'.readmd-pet-native-qa-owned'),'readmd-synthetic-pet-native-qa-v1',{mode:0o600});
 fs.writeFileSync(path.join(profile,'settings.json'),JSON.stringify({lang:'zh-CN',autoCheckUpdate:false,closeToTray:true,pet_enabled:false,pet_in_app:true,pet_bounds:{x:740,y:260,width:320,height:420}}));
 const stage=path.join(plugins,'pet/readmd-rust-host');fs.mkdirSync(stage,{recursive:true});
 for(const dir of ['renderer','models','vendor'])fs.cpSync(path.join(fixture,dir),path.join(stage,dir),{recursive:true,dereference:false,errorOnExist:true});
 fs.copyFileSync(petBinary,path.join(stage,'readmd-pet-rust'));fs.chmodSync(path.join(stage,'readmd-pet-rust'),0o755);
 const signed=spawnSync('/usr/bin/codesign',['--force','--sign','-','--timestamp=none',path.join(stage,'readmd-pet-rust')],{encoding:'utf8',timeout:30000});assert.equal(signed.status,0,'owned debug companion ad-hoc signing');
 const artifacts=[];function walk(d){for(const e of fs.readdirSync(d,{withFileTypes:true}).sort((a,b)=>a.name.localeCompare(b.name))){assert(!e.isSymbolicLink());const p=path.join(d,e.name);if(e.isDirectory())walk(p);else{const b=fs.readFileSync(p),rel=path.relative(stage,p).split(path.sep).join('/');artifacts.push({path:rel,sha256:sha(b),size:b.length,role:rel==='readmd-pet-rust'?'executable':rel.startsWith('renderer/')?'renderer':rel.startsWith('models/')?'model':'asset'});}}}walk(stage);
 const version=fs.readFileSync(path.join(root,'packages/readmd-pet-rust/Cargo.toml'),'utf8').match(/^version = "([^"]+)"/m)[1];
 const manifest={runtime:'readmd-pet-rust',version,protocol_version:1,protocol:1,platform:'macos',arch:process.arch==='arm64'?'aarch64':'x86_64',artifacts};
 fs.writeFileSync(path.join(stage,'runtime-manifest.json'),JSON.stringify(manifest,null,2)+'\n');
 const report={nativeHost:'Cocoa reader / Rust supervisor / actual NSPanel / WKWebView',setup:'verified preinstalled debug runtime with original renderer; packaged cold start separate',strictToken:true,productionCsp:true,modelsRequested:false,binarySha256:sha(fs.readFileSync(binary)),petBinarySha256:sha(fs.readFileSync(path.join(stage,'readmd-pet-rust'))),manifestSha256:sha(fs.readFileSync(path.join(stage,'runtime-manifest.json'))),bootSha256:sha(fs.readFileSync(path.join(root,'assets/readmd.boot.js'))),harnessSha256:sha(fs.readFileSync(__filename)),checks:[],phases:[],passed:false};
 const check=(ok,name)=>{assert(ok,name);report.checks.push(name);},json=p=>{try{return JSON.parse(fs.readFileSync(p));}catch{return null;}},alive=pid=>{try{process.kill(pid,0);return true;}catch{return false;}};
 const healthFile=path.join(plugins,'pet/hermes-overlay-state.json.rust.health.json'),settingsFile=path.join(profile,'settings.json'),progressFile=path.join(profile,'native-qa-progress.json');
 let petPid=0,sequence=0,expectedBounds;
 const policy='(version 1)(allow default)(deny file-write* (require-all (require-not (subpath '+JSON.stringify(workspace)+')) (require-not (subpath '+JSON.stringify('/System/Volumes/Data'+workspace)+')) (require-not (literal "/dev/null")) (require-not (literal "/dev/tty"))))';
 try{for(const phase of ['enable','reopen']){
  const script=driver(phase,file,ack);new vm.Script(script);fs.writeFileSync(path.join(profile,'native-qa.js'),script,{mode:0o600});
  if(fs.existsSync(progressFile))fs.unlinkSync(progressFile);
  const fd=fs.openSync(path.join(out,phase+'.log'),'wx'),child=spawn('/usr/bin/sandbox-exec',['-p',policy,binary,'--port','0','--require-token','--data-dir',profile,'--workspace',docs,'--assets',path.join(root,'assets')],{cwd:out,env:{...process.env,READMD_NATIVE_QA:'1',READMD_PET_NATIVE_QA:'1',READMD_STARTUP_PROBE:'1',READMD_DATA_DIR:profile,READMD_WORKSPACE:docs,READMD_EXTRA_ROOTS:'',TMPDIR:temp,SQLITE_TMPDIR:temp,CFFIXED_USER_HOME:home},stdio:['ignore',fd,fd]});fs.closeSync(fd);
  const exited=new Promise(resolve=>{child.once('exit',(code,signal)=>resolve({code,signal}));child.once('error',()=>resolve({code:null,error:'spawn_failed'}));});
  const until=async fn=>{const end=Date.now()+35000;while(Date.now()<end){if(await fn())return;assert(child.exitCode===null&&child.signalCode===null,'reader_exited_early');await pause(40);}throw Error('native_kernel_condition_timeout');};
  const progress=label=>until(()=>json(progressFile)?.phase===label);
  const qa=async(action='status')=>{const id=++sequence,p=path.join(plugins,'native-pet-qa-command.json');fs.writeFileSync(p+'.tmp',JSON.stringify({sequence:id,pid:petPid,action}));fs.renameSync(p+'.tmp',p);let v;await until(()=>{v=json(path.join(plugins,'native-pet-qa-result.json'));return v?.sequence===id;});assert.equal(v.ok,true);return v;};
  const acknowledge=text=>fs.writeFileSync(ack,text);
  try{
   if(phase==='enable'){
    await progress('pet_context_ready');const contextHealth=json(healthFile);assert(contextHealth?.state==='ready');petPid=contextHealth.pid;
    await qa('context-menu');acknowledge('context_sent');
   }
   await progress(phase==='enable'?'pet_drag_ready':'pet_restored_ready');let health=json(healthFile);check(health?.state==='ready'&&health.engine==='rust'&&alive(health.pid),'actual_supervised_native_pet_ready_'+phase);petPid=health.pid;
   let status=await qa();check(status.renderer_ready&&status.visible&&status.nonactivating&&status.level===3,'actual_native_panel_and_renderer_ready_'+phase);
   if(phase==='enable'){
    const first=status.bounds;status=await qa('drag');expectedBounds=status.bounds;
    check(Math.abs(expectedBounds.x-first.x-48)<1&&Math.abs(expectedBounds.y-first.y-36)<1,'actual_native_drag_moves_owned_companion');
    await until(()=>{const b=json(settingsFile)?.pet_bounds;return b&&Math.abs(b.x-expectedBounds.x)<1&&Math.abs(b.y-expectedBounds.y)<1;});check(true,'durable_native_bounds_consumed_and_saved_by_kernel');
   }else check(Math.abs(status.bounds.x-expectedBounds.x)<1&&Math.abs(status.bounds.y-expectedBounds.y)<1,'native_position_restored_after_main_process_restart');
   acknowledge('native_checked_'+phase);
   if(phase==='enable'){
    await progress('pet_lock_ready');await until(async()=>Boolean((await qa()).locked));const locked=(await qa()).bounds,statusAfter=await qa('drag');check(statusAfter.bounds.x===locked.x&&statusAfter.bounds.y===locked.y,'kernel_position_lock_prevents_native_drag');acknowledge('lock_checked');
    await progress('pet_disabled');await until(()=>!alive(petPid));check(true,'normal_kernel_disable_reaps_owned_native_companion');acknowledge('disabled_checked');
    await progress('pet_reenabled');health=json(healthFile);check(health?.state==='ready'&&health.pid!==petPid&&alive(health.pid),'normal_kernel_reenable_starts_fresh_healthy_companion');petPid=health.pid;status=await qa();check(Math.abs(status.bounds.x-expectedBounds.x)<1&&Math.abs(status.bounds.y-expectedBounds.y)<1,'native_position_retained_after_disable_reenable');acknowledge('reenabled_checked');
   }else{await progress('reader_hidden_pet_active');status=await qa();check(status.visible&&alive(petPid),'native_companion_continues_when_reader_is_hidden');acknowledge('hidden_checked');}
   const outcome=await Promise.race([exited,pause(20000).then(()=>{throw Error('native_reader_quit_timeout');})]);check(outcome.code===0,'normal_native_reader_quit_'+phase);
   const at=Date.now();while(alive(petPid)&&Date.now()-at<2000)await pause(20);check(!alive(petPid),'native_companion_stopped_on_true_reader_quit_'+phase);
   const resultFile=path.join(profile,'native-qa-result.json'),result=json(resultFile);assert.equal(result?.ok,true,result?.checks?.join(','));fs.renameSync(resultFile,path.join(out,phase+'.json'));report.phases.push({...result,...outcome});
  }finally{if(child.exitCode===null&&child.signalCode===null){child.kill('SIGTERM');await exited;}if(petPid&&alive(petPid))await pause(1000);}
 }report.passed=true;}finally{fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n');if(fs.existsSync(ack))fs.unlinkSync(ack);for(const p of [temp,home])fs.rmSync(p,{recursive:true,force:true});}
 console.log(JSON.stringify({nativePetKernelPassed:report.passed,nativeChecks:report.checks.length,uiChecks:report.phases.reduce((n,p)=>n+p.checks.length,0),phases:report.phases.length}));
}
if(require.main===module){const args=process.argv.slice(2);assert.equal(args.length,4);run(...args.map(p=>path.resolve(p))).catch(e=>{console.error(e.message);process.exitCode=1;});}
