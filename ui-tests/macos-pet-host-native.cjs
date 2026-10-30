'use strict';
// Actual native companion process, AppKit input/drag, Quartz hit testing and
// real renderer. Standalone-host evidence; kernel persistence is a separate run.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict'),{spawn,spawnSync}=require('node:child_process');
const root=path.resolve(__dirname,'..'),workspace=path.dirname(root),sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function run(binary,fixture,out){
 assert.equal(process.platform,'darwin');assert(out.startsWith(workspace+path.sep));assert(fs.existsSync(path.join(fixture,'fixture.json')));fs.mkdirSync(out,{mode:0o700});
 const profile=path.join(out,'profile'),temp=path.join(out,'tmp'),home=path.join(out,'native-home');for(const p of [profile,temp,home])fs.mkdirSync(p,{mode:0o700});
 fs.writeFileSync(path.join(profile,'.readmd-pet-native-qa-owned'),'readmd-synthetic-pet-native-qa-v1',{flag:'wx',mode:0o600});
 const bridge=path.join(profile,'pet-state.json');let generation=0;
 const initial={x:800,y:240,width:320,height:420};let snapshot={format_version:1,visible:true,renderer:'hermes-sprite',character:'mochi',bounds:initial,info:{character:'mochi',opacity:1,sound:false,always_on_top:true,lock_position:false}};
 function publish(update={}){snapshot={...snapshot,...update,generation:++generation};const p=bridge+'.tmp';fs.writeFileSync(p,JSON.stringify(snapshot));fs.renameSync(p,bridge);}
 publish();
 const policy='(version 1)(allow default)(deny file-write* (require-all (require-not (subpath '+JSON.stringify(workspace)+')) (require-not (subpath '+JSON.stringify('/System/Volumes/Data'+workspace)+')) (require-not (literal "/dev/null")) (require-not (literal "/dev/tty"))))';
 const parent=spawn(process.execPath,['-e','setInterval(()=>{},10000)'],{stdio:'ignore'}),fd=fs.openSync(path.join(out,'native-host.log'),'wx');
 const child=spawn('/usr/bin/sandbox-exec',['-p',policy,binary],{cwd:fixture,env:{...process.env,READMD_PET_BRIDGE_FILE:bridge,READMD_PET_RENDERER_ROOT:path.join(fixture,'renderer'),READMD_PET_RUNTIME_DIR:fixture,READMD_DATA_DIR:profile,READMD_PET_RENDERER:'hermes-sprite',READMD_PARENT_PID:String(parent.pid),READMD_PET_NATIVE_QA:'1',TMPDIR:temp,CFFIXED_USER_HOME:home},stdio:['ignore',fd,fd]});fs.closeSync(fd);
 const exited=new Promise(resolve=>{child.once('exit',(code,signal)=>resolve({code,signal}));child.once('error',()=>resolve({code:null,error:'spawn_failed'}));});
 let sequence=0;const checks=[],observations=[];
 const report={platform:'macos-'+process.arch,nativeHost:'AppKit / WKWebView',strictWorkspaceWriteSandbox:true,modelsRequested:false,controller:'standalone host, not a kernel persistence certificate',binarySha256:sha(fs.readFileSync(binary)),fixtureSha256:sha(fs.readFileSync(path.join(fixture,'fixture.json'))),harnessSha256:sha(fs.readFileSync(__filename)),checks,observations,passed:false};
 const until=async fn=>{const end=Date.now()+25000;while(Date.now()<end){if(child.exitCode!==null||child.signalCode!==null)throw Error('native_pet_exited_early');if(await fn())return;await sleep(25);}throw Error('native_pet_condition_timeout');};
 async function qa(action='status'){
  const id=++sequence,p=path.join(profile,'native-pet-qa-command.json'),tmp=p+'.tmp';fs.writeFileSync(tmp,JSON.stringify({sequence:id,pid:child.pid,action}));fs.renameSync(tmp,p);
  let value;await until(()=>{try{value=JSON.parse(fs.readFileSync(path.join(profile,'native-pet-qa-result.json')));return value.sequence===id;}catch{return false;}});assert.equal(value.ok,true);return value;
 }
 const check=(ok,name)=>{assert(ok,name);checks.push(name);};
 const capture=(status,name)=>{
  const p=spawnSync(path.join(workspace,'.readmd-local/native-window-capture'),[String(child.pid),String(status.window_number)],{encoding:'utf8',timeout:10000});check(p.status===0,'capture_window_belongs_to_owned_native_pet');
  const r=spawnSync('/usr/sbin/screencapture',['-x','-o','-l',String(status.window_number),path.join(out,name)],{timeout:10000});check(r.status===0,'owned_native_pet_window_captured');
 };
 try{
  let status=await qa();observations.push({phase:'first_ready',...status});
  check(status.renderer_ready&&status.interaction_generation>0&&status.pet_regions>0,'real_renderer_painted_and_published_silhouette');
  if(!status.nonactivating||status.level!==3){observations.push({phase:'diagnose_immediate',...await qa('diagnose_native_style')});await sleep(200);observations.push({phase:'diagnose_settled',...await qa()});}
  check(status.visible&&!status.opaque&&!status.shadow&&status.nonactivating&&status.level===3,'actual_transparent_floating_nonactivating_native_window');
  check(status.frame.width===320&&status.frame.height===420,'actual_native_window_size_matches_snapshot');capture(status,'native-pet-ready.png');
  const base={...snapshot.bounds};
  check(status.pointer&&status.hit_point,'native_pointer_and_real_silhouette_available');
  publish({bounds:{...base,x:status.pointer.x-status.hit_point.x,y:status.pointer.y-status.hit_point.y}});
  await until(async()=>{status=await qa();return !status.ignores_mouse;});check(true,'quartz_pointer_over_actual_pet_enables_native_mouse_delivery');
  publish({bounds:{...base,x:status.pointer.x-8,y:status.pointer.y-8}});
  await until(async()=>{status=await qa();return status.ignores_mouse;});check(true,'transparent_margin_releases_native_mouse_delivery');
  publish({bounds:base});await sleep(120);status=await qa();const first={...status.frame};
  status=await qa('click');await sleep(150);
  const commands=()=>fs.existsSync(bridge+'.commands')?fs.readdirSync(bridge+'.commands').filter(n=>n.endsWith('.json')).map(n=>JSON.parse(fs.readFileSync(path.join(bridge+'.commands',n)))):[];
  const interactions=()=>commands().filter(v=>v.command?.type==='interact').length;
  await until(()=>interactions()>0);check(interactions()===1,'one_native_click_produces_one_interaction');
  check(true,'owned_appkit_mouse_click_reaches_real_renderer_and_durable_command');capture(status,'native-pet-interaction.png');
  status=await qa('drag');observations.push({phase:'dragged',...status});
  check(Math.abs(status.frame.x-first.x-48)<1&&Math.abs(status.frame.y-first.y+36)<1,'owned_appkit_drag_moves_actual_native_window');
  await until(()=>commands().some(v=>v.command?.type==='bounds'||v.type==='bounds'));
  check(true,'native_drag_publishes_durable_bounds');
  const moved={...status.bounds};publish({bounds:base});await sleep(120);status=await qa();
  check(Math.abs(status.frame.x-first.x-48)<1,'stale_snapshot_does_not_rewind_completed_native_drag');
  publish({bounds:moved,info:{...snapshot.info,lock_position:true}});await sleep(120);status=await qa();check(status.locked,'native_lock_position_applied');
  const locked={...status.frame};status=await qa('drag');check(Math.abs(status.frame.x-locked.x)<1&&Math.abs(status.frame.y-locked.y)<1,'locked_native_pet_cannot_be_dragged');
  check(interactions()===1,'moved_gestures_do_not_become_accidental_pet_interactions');
  await qa('context-menu');await until(()=>commands().some(v=>v.command?.type==='open-menu'));
  check(commands().filter(v=>v.command?.type==='open-menu').length===1,'native_right_click_publishes_one_normal_durable_menu_request');
  publish({visible:false});await until(async()=>!(await qa()).visible);check(true,'normal_snapshot_hides_real_native_window');
  publish({visible:true});await until(async()=>Boolean((await qa()).visible));check(true,'normal_snapshot_wakes_real_native_window');
  const at=Date.now();parent.kill('SIGTERM');const outcome=await Promise.race([exited,sleep(2500).then(()=>({error:'parent_exit_timeout'}))]);
  check(outcome.code===0&&Date.now()-at<2000,'native_host_stops_with_owned_parent_under_two_seconds');report.exit=outcome;report.passed=true;
 }finally{
  if(child.exitCode===null&&child.signalCode===null){child.kill('SIGTERM');await exited;}
  if(parent.exitCode===null&&parent.signalCode===null)parent.kill('SIGTERM');
  fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n');for(const p of [temp,home])fs.rmSync(p,{recursive:true,force:true});
 }
 console.log(JSON.stringify({nativePetHostPassed:report.passed,checks:checks.length,inputPermissions:observations[0]?.input_permissions}));
}
if(require.main===module){const args=process.argv.slice(2);assert.equal(args.length,3,'absolute host binary, owned renderer fixture, new evidence directory required');run(...args.map(p=>path.resolve(p))).catch(e=>{console.error(e.message);process.exitCode=1;});}
