'use strict';
// A production release companion (no pet QA feature) installed by the actual
// Cocoa reader. Exercise an empty profile and a genuine prior release archive.
// The reader's opt-in driver only invokes existing UI and waits for receipts.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict'),vm=require('node:vm'),{spawn,spawnSync}=require('node:child_process');
const root=path.resolve(__dirname,'..'),workspace=path.dirname(root),sha=b=>crypto.createHash('sha256').update(b).digest('hex'),pause=ms=>new Promise(r=>setTimeout(r,ms));
const json=p=>JSON.parse(fs.readFileSync(p)),alive=pid=>{try{process.kill(pid,0);return true;}catch{return false;}};
function command(program,args){const r=spawnSync(program,args,{encoding:'utf8',timeout:30000});assert.equal(r.status,0,path.basename(program)+' failed');return r.stdout;}
function verifyTree(tree,manifest){
 const found=[];function walk(dir){for(const e of fs.readdirSync(dir,{withFileTypes:true})){assert(!e.isSymbolicLink(),'runtime cannot contain aliases');const p=path.join(dir,e.name);if(e.isDirectory())walk(p);else{assert(e.isFile());found.push(path.relative(tree,p).split(path.sep).join('/'));}}}walk(tree);
 assert.deepEqual(found.sort(),['runtime-manifest.json',...manifest.artifacts.map(a=>a.path)].sort());
 for(const a of manifest.artifacts){assert(!a.path.includes('..')&&!path.isAbsolute(a.path));const b=fs.readFileSync(path.join(tree,a.path));assert.equal(b.length,a.size);assert.equal(sha(b),a.sha256);}
 command('/usr/bin/codesign',['--verify','--strict',path.join(tree,'readmd-pet-rust')]);
}
function driver(phase,doc,ack,scenario){return String.raw`(async()=>{
 const checks=[],phase=${JSON.stringify(phase)},ack=${JSON.stringify(ack)},character=${JSON.stringify(scenario==='bongo'?'bongocat':scenario==='live2d'?'arch-chan':'mochi')},renderer=${JSON.stringify(scenario==='live2d'?'live2d':'hermes-sprite')},pause=ms=>new Promise(r=>setTimeout(r,ms));
 const check=(ok,name)=>{if(!ok)throw Error(name);checks.push(name);};
 const until=async fn=>{const end=Date.now()+45000;while(Date.now()<end){if(await fn())return;await pause(60);}throw Error('packaged_pet_timeout');};
 try{
  await __readmdSessionReady;check(hasPy&&document.body.classList.contains('native-macos'),'actual_cocoa_reader');
  if(phase==='install'){
   await loadFile(${JSON.stringify(doc)});await openPetSettings();
   $('pet-enabled').checked=true;$('pet-runtime').value='desktop';$('pet-renderer').value=renderer;currentActivePetSlug=character;$('pet-sound').checked=false;
   const enabled=await savePetSettings();if(!enabled?.ok){const code=enabled?.code||enabled?.error_code;throw Error('release_install_'+(/^[a-z_]{1,70}$/.test(code)?code:'failed'));}
   check(true,'normal_settings_install_and_enable_release_companion');closePetSettings();
  }else check(getActiveTab()?.path===${JSON.stringify(doc)},'native_session_restores_same_document');
  await until(async()=>{const s=await fetchPetRuntimeStatus();return s.enabled&&!s.in_app&&s.adapter?.rust?.running&&s.adapter.rust.health?.state==='ready';});
  check(true,'real_release_companion_rendered_and_supervised');
  const actual=await fetchPetRuntimeStatus();check(actual.active_slug===character&&actual.adapter.rust.health.renderer===renderer,'selected_character_and_real_renderer_match');
  const m=await (await apiFetch('/api/modules')).json();check(m.modules.ocr==='idle'&&m.modules.transcribe==='idle','recognition_models_remain_unloaded');
  window.ipc.postMessage(JSON.stringify({readmd_native_qa_progress:{ok:true,phase:'release_pet_ready',checks}}));
  await until(async()=>{const r=await apiFetch('/api/file?p='+encodeURIComponent(ack));return r.ok&&(await r.json()).content==='checked';});
  check(await ReadMDSession.flush()!==false,'session_persisted_before_true_quit');
  window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase,checks}}));py.window_control('qa_application',{action:'terminate',id:1});
 }catch(error){checks.push('failed_'+(/^[a-z_]{1,110}$/.test(error.message)?error.message:'packaged_pet'));window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:false,phase,checks}}));window.ipc.postMessage('readmd:quit');}
})()`;}
async function run(binary,archive,previous,out){
 assert.equal(process.platform,'darwin');assert(out.startsWith(workspace+path.sep));fs.mkdirSync(out,{mode:0o700});
 const resources=path.join(out,'resources');fs.mkdirSync(resources);fs.cpSync(path.join(root,'assets'),path.join(resources,'assets'),{recursive:true,dereference:false});
 const archiveName='ReadMD-Pet-Rust-macos-'+(process.arch==='arm64'?'aarch64':'x86_64')+'.zip';fs.copyFileSync(archive,path.join(resources,archiveName));
 const manifest=JSON.parse(command('/usr/bin/unzip',['-p',archive,'runtime-manifest.json'])),old=JSON.parse(command('/usr/bin/unzip',['-p',previous,'runtime-manifest.json']));
 assert.equal(manifest.version,'1.0.1');assert.equal(old.version,'1.0.0');assert.equal(manifest.platform,'macos');
 const report={nativeHost:'Cocoa reader / production release NSPanel companion / WKWebView',petQaCompiled:false,strictToken:true,productionCsp:true,modelsRequested:false,binarySha256:sha(fs.readFileSync(binary)),bootSha256:sha(fs.readFileSync(path.join(root,'assets/readmd.boot.js'))),archiveSha256:sha(fs.readFileSync(archive)),previousArchiveSha256:sha(fs.readFileSync(previous)),harnessSha256:sha(fs.readFileSync(__filename)),checks:[],scenarios:[],passed:false};
 const check=(ok,name)=>{assert(ok,name);report.checks.push(name);};
 const policy='(version 1)(allow default)(deny file-write* (require-all (require-not (subpath '+JSON.stringify(workspace)+')) (require-not (subpath '+JSON.stringify('/System/Volumes/Data'+workspace)+')) (require-not (literal "/dev/null")) (require-not (literal "/dev/tty"))))';
 try{for(const scenario of ['cold','upgrade','bongo','live2d']){
  const base=path.join(out,scenario),profile=path.join(base,'profile'),docs=path.join(base,'documents'),tmp=path.join(base,'tmp'),home=path.join(base,'native-home');for(const p of [base,profile,docs,tmp,home])fs.mkdirSync(p,{mode:0o700});
  const plugin=path.join(profile,'plugins/pet'),target=path.join(plugin,'readmd-rust-host'),ack=path.join(docs,'checked.md'),doc=path.join(docs,'桌宠安装与升级.md');fs.mkdirSync(plugin,{recursive:true});
  const original='# 原生桌宠安装与升级\n\n这是专用验证文档。🐾\n';fs.writeFileSync(doc,original);fs.writeFileSync(path.join(plugin,'custom-owned-note.txt'),'用户数据必须保留。');
  fs.writeFileSync(path.join(profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1',{mode:0o600});
  fs.writeFileSync(path.join(profile,'settings.json'),JSON.stringify({lang:'zh-CN',autoCheckUpdate:false,closeToTray:false,pet_enabled:false,pet_in_app:true,pet_bounds:{x:740,y:260,width:320,height:420}}));
  if(scenario==='upgrade'){fs.mkdirSync(target);command('/usr/bin/unzip',['-q',previous,'-d',target]);verifyTree(target,old);check(true,'genuine_v005_runtime_verified_before_upgrade');}else check(!fs.existsSync(target),'cold_profile_has_no_runtime');
  const scenarioReport={scenario,phases:[]};report.scenarios.push(scenarioReport);
  try{for(const phase of ['install','reopen']){
   if(fs.existsSync(ack))fs.unlinkSync(ack);const progressFile=path.join(profile,'native-qa-progress.json');if(fs.existsSync(progressFile))fs.unlinkSync(progressFile);
   const script=driver(phase,doc,ack,scenario);new vm.Script(script);fs.writeFileSync(path.join(profile,'native-qa.js'),script,{mode:0o600});
   const fd=fs.openSync(path.join(base,phase+'.log'),'wx'),env={...process.env,READMD_NATIVE_QA:'1',READMD_STARTUP_PROBE:'1',READMD_DATA_DIR:profile,READMD_WORKSPACE:docs,READMD_EXTRA_ROOTS:'',TMPDIR:tmp,SQLITE_TMPDIR:tmp,CFFIXED_USER_HOME:home};delete env.READMD_PET_NATIVE_QA;
   const child=spawn('/usr/bin/sandbox-exec',['-p',policy,binary,'--port','0','--require-token','--data-dir',profile,'--workspace',docs,'--assets',path.join(resources,'assets')],{cwd:base,env,stdio:['ignore',fd,fd]});fs.closeSync(fd);
   const exited=new Promise(resolve=>{child.once('exit',(code,signal)=>resolve({code,signal}));child.once('error',()=>resolve({code:null}));});let pid=0;
   try{
    const end=Date.now()+60000;while(Date.now()<end){if(fs.existsSync(progressFile)&&json(progressFile).phase==='release_pet_ready')break;assert(child.exitCode===null&&child.signalCode===null,'reader_exited_early');await pause(40);}
    assert.equal(json(progressFile).phase,'release_pet_ready');const health=json(path.join(plugin,'hermes-overlay-state.json.rust.health.json'));pid=health.pid;
    check(health.state==='ready'&&alive(pid),'actual_release_companion_ready_'+scenario+'_'+phase);
    assert.deepEqual(json(path.join(target,'runtime-manifest.json')),manifest);verifyTree(target,manifest);check(true,'installed_release_bytes_and_signature_verified_'+scenario+'_'+phase);
    const probe=JSON.parse(command(path.join(workspace,'.readmd-local/native-window-capture'),[String(pid)]));check(probe.ownedWindows?.length===1,'release_companion_has_own_visible_native_window_'+scenario+'_'+phase);
    if(phase==='install'&&probe.captureAllowed){const capture=path.join(base,'release-pet.png');command('/usr/sbin/screencapture',['-x','-o','-l',String(probe.ownedWindows[0].id),capture]);scenarioReport.screenshotSha256=sha(fs.readFileSync(capture));}
    if(scenario==='upgrade'){
     const backups=fs.readdirSync(plugin).filter(n=>n.startsWith('readmd-runtime-upgrade-'));assert.equal(backups.length,1);const b=path.join(plugin,backups[0]);assert.equal(json(path.join(b,'upgrade-intent.json')).state,'published_previous_retained');assert.deepEqual(json(path.join(b,'previous/runtime-manifest.json')),old);verifyTree(path.join(b,'previous'),old);check(true,'genuine_previous_runtime_retained_for_recovery_'+phase);
    }
    assert.equal(fs.readFileSync(path.join(plugin,'custom-owned-note.txt'),'utf8'),'用户数据必须保留。');assert.equal(fs.readFileSync(doc,'utf8'),original);check(true,'document_and_custom_data_preserved_'+scenario+'_'+phase);
    fs.writeFileSync(ack,'checked');const outcome=await Promise.race([exited,pause(25000).then(()=>{throw Error('native_quit_timeout');})]);assert.equal(outcome.code,0);
    const stop=Date.now();while(alive(pid)&&Date.now()-stop<2000)await pause(20);check(!alive(pid),'release_companion_stops_after_true_quit_'+scenario+'_'+phase);
    const resultFile=path.join(profile,'native-qa-result.json'),result=json(resultFile);assert.equal(result.ok,true,result.checks.join(','));fs.renameSync(resultFile,path.join(base,phase+'.json'));scenarioReport.phases.push(result);
   }finally{if(child.exitCode===null&&child.signalCode===null){child.kill('SIGTERM');await exited;}}
  }}finally{if(fs.existsSync(ack))fs.unlinkSync(ack);for(const p of [tmp,home])fs.rmSync(p,{recursive:true,force:true});}
 }report.passed=true;}finally{fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n');fs.rmSync(resources,{recursive:true,force:true});}
 console.log(JSON.stringify({nativePetReleasePackagePassed:report.passed,checks:report.checks.length,uiChecks:report.scenarios.reduce((n,s)=>n+s.phases.reduce((v,p)=>v+p.checks.length,0),0),scenarios:report.scenarios.length}));
}
if(require.main===module){const args=process.argv.slice(2);assert.equal(args.length,4);run(...args.map(p=>path.resolve(p))).catch(e=>{console.error(e.message);process.exitCode=1;});}
