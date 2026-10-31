'use strict';
// Explicit OS integration: LaunchServices registers/launches a dedicated QA
// .app. This writes system-owned registration/cache state, so it is separate
// from tests confined to a workspace sandbox. It never changes a default app.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict'),{spawn,spawnSync}=require('node:child_process');
const {driver}=require('./macos-window-native.cjs');
const root=path.resolve(__dirname,'..'),workspace=path.dirname(root),sha=b=>crypto.createHash('sha256').update(b).digest('hex'),pause=ms=>new Promise(r=>setTimeout(r,ms));
function command(program,args,options={}){const r=spawnSync(program,args,{encoding:'utf8',timeout:30000,...options});assert.equal(r.status,0,path.basename(program)+' failed');return r.stdout;}
async function run(app,out){
 assert.equal(process.platform,'darwin');assert(app.startsWith(workspace+path.sep)&&out.startsWith(workspace+path.sep));assert(!fs.lstatSync(app).isSymbolicLink());
 const info=JSON.parse(command('/usr/bin/plutil',['-convert','json','-o','-',path.join(app,'Contents/Info.plist')])),id=info.CFBundleIdentifier;
 assert(id.startsWith('asia.readmd.nativeqa.'),'only the dedicated QA bundle may be launched');assert(!info.CFBundleDocumentTypes,'QA bundle cannot register document associations');
 command('/usr/bin/codesign',['--verify','--deep','--strict',app]);fs.mkdirSync(out,{mode:0o700});
 const docs=path.join(out,'documents'),profile=path.join(out,'profile'),tmp=path.join(out,'tmp'),home=path.join(out,'native-home');for(const d of [docs,profile,tmp,home])fs.mkdirSync(d,{mode:0o700});
 const file=path.join(docs,'系统启动与原生窗口验证.md'),ack=path.join(docs,'capture-ack.txt'),content='# Native window contract\n\nA synthetic document with reliable save and close.\n';fs.writeFileSync(file,content,{flag:'wx'});
 fs.writeFileSync(path.join(profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1',{mode:0o600});fs.writeFileSync(path.join(profile,'settings.json'),JSON.stringify({lang:'en',closeToTray:false,autoCheckUpdate:false}));fs.writeFileSync(path.join(profile,'native-qa.js'),driver(file,ack),{mode:0o600});
 // Swift serializes module paths. Mixing the Rust cache's historical ../
 // spelling with the canonical path made Clang load Foundation twice. Keep a
 // single canonical Swift QA cache, and never spill compiler scratch to /var.
 const cachePath=path.join(workspace,'.readmd-local/clang-module-cache/native-qa-swift');fs.mkdirSync(cachePath,{recursive:true});
 const moduleCache=fs.realpathSync(cachePath),compileEnv={...process.env,TMPDIR:tmp,CFFIXED_USER_HOME:home,CLANG_MODULE_CACHE_PATH:moduleCache,SWIFT_MODULE_CACHE_PATH:moduleCache};
 const helper=path.join(out,'owned-app-context'),compile=spawnSync('/usr/bin/xcrun',['swiftc','-module-cache-path',moduleCache,path.join(__dirname,'fixtures/macos-owned-app-context.swift'),'-o',helper],{encoding:'utf8',env:compileEnv,timeout:60000});
 fs.writeFileSync(path.join(out,'helper-build.log'),compile.stderr,{mode:0o600});assert.equal(compile.status,0);
 const owned=()=>JSON.parse(command(helper,[id,app])).ownedApps;
 assert.equal(owned().length,0,'a previous QA app must not be adopted');
 const env={READMD_NATIVE_QA:'1',READMD_DATA_DIR:profile,READMD_WORKSPACE:docs,READMD_EXTRA_ROOTS:'',READMD_ASSETS_DIR:'',TMPDIR:tmp,SQLITE_TMPDIR:tmp,CFFIXED_USER_HOME:home};
 const logfile=path.join(out,'native.log');fs.writeFileSync(logfile,'',{flag:'wx',mode:0o600});
 const argv=['-n','-W','--stdout',logfile,'--stderr',logfile,...Object.entries(env).flatMap(([k,v])=>['--env',k+'='+v]),app,'--args','--port','0','--require-token','--data-dir',profile,'--workspace',docs,'--assets',path.join(root,'assets')];
 const report={nativeHost:'LaunchServices / dedicated .app / Cocoa WKWebView / actual AppKit standard buttons',systemRegistration:true,defaultAssociationsChanged:false,modelsRequested:false,strictToken:true,productionCsp:true,binarySha256:sha(fs.readFileSync(path.join(app,'Contents/MacOS',info.CFBundleExecutable))),bootSha256:sha(fs.readFileSync(path.join(root,'assets/readmd.boot.js'))),harnessSha256:sha(fs.readFileSync(__filename)),passed:false};
 const launching=spawn('/usr/bin/open',argv,{cwd:out,stdio:'ignore'}),exited=new Promise(resolve=>{launching.once('exit',(code,signal)=>resolve({code,signal}));launching.once('error',()=>resolve({code:null}));});let actualPid=0;
 try{
  const resultFile=path.join(profile,'native-qa-result.json'),deadline=Date.now()+180000;
  while(!fs.existsSync(resultFile)&&Date.now()<deadline){const apps=owned();assert(apps.length<=1,'multiple QA instances');if(apps[0])actualPid=apps[0].pid;assert(launching.exitCode===null&&launching.signalCode===null,'system launcher exited early');await pause(200);}
  const first=JSON.parse(fs.readFileSync(resultFile));fs.renameSync(resultFile,path.join(out,first.phase+'.json'));report.first=first;assert.equal(first.ok,true,first.checks.join(','));assert.equal(first.phase,'capture_ready');
  const apps=owned();assert.equal(apps.length,1);actualPid=apps[0].pid;report.appContext=apps[0];assert.equal(apps[0].active,true,'the native reader must be active after restore and fullscreen');
  const commandPath=command('/bin/ps',['-p',String(actualPid),'-o','comm=']).trim();
  assert.equal(fs.realpathSync(commandPath),fs.realpathSync(path.join(app,'Contents/MacOS',info.CFBundleExecutable)),'PID must be the exact QA executable');
  report.ownedExecutableConfirmed=true;
  const captureHelper=path.join(workspace,'.readmd-local/native-window-capture');
  if(fs.existsSync(captureHelper)){
   const probe=JSON.parse(command(captureHelper,[String(actualPid),String(first.window_number)]));assert(probe.ownedWindowOnScreen,'capture must belong to the actual launched app');
   if(probe.captureAllowed){const image=path.join(out,'native-system-window.png');command('/usr/sbin/screencapture',['-x','-o','-l',String(first.window_number),image]);report.screenshotSha256=sha(fs.readFileSync(image));}
  }
  const quitStart=Date.now(),alive=()=>{try{process.kill(actualPid,0);return true;}catch(error){if(error.code==='ESRCH')return false;throw error;}};
  fs.writeFileSync(ack,'Owned native capture complete.');report.launcherOutcome=await Promise.race([exited,pause(20000).then(()=>{throw Error('system_app_quit_timeout');})]);assert.equal(report.launcherOutcome.code,0);
  report.launcherReturnedMs=Date.now()-quitStart;
  report.result=JSON.parse(fs.readFileSync(resultFile));assert.equal(report.result.ok,true,report.result.checks.join(','));
  // open -W and the LaunchServices registry can settle before/after the Unix
  // process. Observe the real PID as well; never replace a true quit with kill.
  while(alive()&&Date.now()-quitStart<2000)await pause(50);
  assert(!alive(),'the exact owned process must stop within two seconds');report.processGoneMs=Date.now()-quitStart;
  while(owned().length&&Date.now()-quitStart<5000)await pause(100);
  assert.equal(owned().length,0,'owned application registration must settle after true quit');report.registrationGoneMs=Date.now()-quitStart;
  assert.equal(fs.readFileSync(file,'utf8'),content+'\nOwned unsaved sentence.\n');
  // open -W reports the launcher outcome, not the app's Unix exit status.
  report.nativeProcessExitCode=null;report.nativeProcessGone=true;report.passed=true;
 }finally{
  if(actualPid&&owned().some(a=>a.pid===actualPid)){try{process.kill(actualPid,'SIGTERM');}catch{}await pause(500);}
  if(launching.exitCode===null&&launching.signalCode===null){launching.kill('SIGTERM');await exited;}
  fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n');for(const p of [tmp,home])fs.rmSync(p,{recursive:true,force:true});
 }
 console.log(JSON.stringify({nativeSystemWindowPassed:report.passed,checks:report.result?.checks.length,active:report.appContext?.active,screenshot:Boolean(report.screenshotSha256)}));
}
if(require.main===module){const args=process.argv.slice(2);assert.equal(args.length,2);run(...args.map(p=>path.resolve(p))).catch(e=>{console.error(e.message);process.exitCode=1;});}
