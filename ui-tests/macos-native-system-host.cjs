'use strict';
// Explicit normal OS integration for a dedicated, workspace-owned QA app.
// Callers require the user's system-write exception. Never register a default
// document handler, adopt an existing app, drive another app or request TCC.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const {spawn,spawnSync}=require('node:child_process');
const root=path.resolve(__dirname,'..'),workspace=path.dirname(root),pause=ms=>new Promise(r=>setTimeout(r,ms));
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
function command(program,args){const r=spawnSync(program,args,{encoding:'utf8',timeout:15000});assert.equal(r.status,0,path.basename(program)+' failed');return r.stdout;}
function within(p){assert(p.startsWith(workspace+path.sep),'only workspace-owned inputs');assert(!fs.lstatSync(p).isSymbolicLink());}
function exists(pid){try{process.kill(pid,0);return true;}catch(error){if(error.code==='ESRCH')return false;throw error;}}
function prepare(app,out,{tmp,home,petQa=false}){
 assert.equal(process.platform,'darwin');for(const p of [app,out,tmp,home])within(p);
 const info=JSON.parse(command('/usr/bin/plutil',['-convert','json','-o','-',path.join(app,'Contents/Info.plist')]));
 assert(info.CFBundleIdentifier.startsWith('asia.readmd.nativeqa.'));assert(!info.CFBundleDocumentTypes,'QA app must not declare file associations');
 command('/usr/bin/codesign',['--verify','--deep','--strict',app]);
 const binary=path.join(app,'Contents/MacOS',info.CFBundleExecutable);within(binary);
 const cachePath=path.join(workspace,'.readmd-local/clang-module-cache/native-qa-swift');fs.mkdirSync(cachePath,{recursive:true});const cache=fs.realpathSync(cachePath);
 const helper=path.join(out,'owned-app-context'),compile=spawnSync('/usr/bin/xcrun',['swiftc','-module-cache-path',cache,path.join(__dirname,'fixtures/macos-owned-app-context.swift'),'-o',helper],{encoding:'utf8',timeout:60000,env:{...process.env,TMPDIR:tmp,CFFIXED_USER_HOME:home,CLANG_MODULE_CACHE_PATH:cache,SWIFT_MODULE_CACHE_PATH:cache}});
 fs.writeFileSync(path.join(out,'helper-build.log'),compile.stderr||'',{flag:'wx',mode:0o600});assert.equal(compile.status,0,'owned app observer must compile');
 const owned=()=>JSON.parse(command(helper,[info.CFBundleIdentifier,app])).ownedApps;
 const exact=pid=>{if(!exists(pid))return false;const r=spawnSync('/bin/ps',['-p',String(pid),'-o','comm='],{encoding:'utf8',timeout:5000});if(r.status!==0)return false;try{return fs.realpathSync(r.stdout.trim())===fs.realpathSync(binary);}catch{return false;}};
 assert.equal(owned().length,0,'an existing instance must not be adopted');
 return {binary,binarySha256:sha(fs.readFileSync(binary)),helperSha256:sha(fs.readFileSync(helper)),bundleIdentifier:info.CFBundleIdentifier,
  async launch({profile,docs,log,openFile}){
   for(const p of [profile,docs])within(p);assert.equal(owned().length,0,'previous QA instance must be gone');
   if(openFile){within(openFile);assert(fs.lstatSync(openFile).isFile());}
   fs.writeFileSync(log,'',{flag:'wx',mode:0o600});
   const env={READMD_NATIVE_QA:'1',READMD_PET_NATIVE_QA:petQa?'1':'',READMD_DATA_DIR:profile,READMD_WORKSPACE:docs,READMD_EXTRA_ROOTS:'',READMD_ASSETS_DIR:'',TMPDIR:tmp,SQLITE_TMPDIR:tmp,CFFIXED_USER_HOME:home};
   const appArgs=['--port','0','--require-token','--data-dir',profile,'--workspace',docs,'--assets',path.join(root,'assets')];
   const child=spawn('/usr/bin/open',['-n','-W','--stdout',log,'--stderr',log,...Object.entries(env).flatMap(([k,v])=>['--env',k+'='+v]),...(openFile?['-a',app,openFile]:[app]),'--args',...appArgs],{cwd:out,stdio:'ignore'});
   const exited=new Promise(resolve=>{child.once('exit',(code,signal)=>resolve({code,signal}));child.once('error',()=>resolve({code:null,spawnError:true}));});
   let pid=0,naturallyStopped=false;const receipt={normalSystemLaunch:true,explicitOsDocumentOpen:Boolean(openFile),nativeProcessExitCode:null,forcedCleanup:false};
   const instance={receipt,get pid(){return pid;},get launcherStopped(){return child.exitCode!==null||child.signalCode!==null;},
    reopen(){
     const apps=owned();assert.equal(apps.length,1);assert.equal(apps[0].pid,pid);assert(exact(pid),'only the existing QA app may be reopened');
     // Supply the same isolated profile even if a real termination races the
     // OS request. No -n: a running application must receive the normal reopen.
     command('/usr/bin/open',[...Object.entries(env).flatMap(([k,v])=>['--env',k+'='+v]),'-a',app,'--args',...appArgs]);receipt.reopenedExistingPid=pid;
    },
    async wait(timeout=60000){
     const start=Date.now();let timer;try{receipt.launcherOutcome=await Promise.race([exited,new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('native_system_phase_timeout')),timeout);})]);}finally{clearTimeout(timer);}
     assert.equal(receipt.launcherOutcome.code,0,'system launcher outcome');const returned=Date.now();receipt.launcherWaitMs=returned-start;
     while(exists(pid)&&Date.now()-returned<2000)await pause(50);
     assert(!exists(pid),'the exact QA process must stop after true quit');receipt.processGoneAfterLauncherMs=Date.now()-returned;receipt.nativeProcessGone=true;
     while(owned().length&&Date.now()-returned<5000)await pause(100);
     assert.equal(owned().length,0,'system registry must settle after quit');receipt.registrySettledAfterLauncherMs=Date.now()-returned;naturallyStopped=true;return receipt;
    },
    async cleanup(){
     if(pid&&exists(pid)){assert(exact(pid),'refuse to stop an unrecognized process');assert(!naturallyStopped,'a stopped PID must never be reused');receipt.forcedCleanup=true;process.kill(pid,'SIGTERM');const until=Date.now()+2500;while(exists(pid)&&Date.now()<until)await pause(50);assert(!exists(pid),'owned process did not stop during cleanup');}
     if(child.exitCode===null&&child.signalCode===null){child.kill('SIGTERM');await exited;}
    }
   };
   try{const deadline=Date.now()+10000;while(Date.now()<deadline){const apps=owned();assert(apps.length<=1);if(apps.length){pid=apps[0].pid;assert(exact(pid),'system launch must identify the actual QA executable');receipt.initialAppContext=apps[0];return instance;}assert(!instance.launcherStopped,'system launcher returned before an owned app appeared');await pause(100);}throw Error('native_system_launch_timeout');}
   catch(error){await instance.cleanup();throw error;}
  }
 };
}
module.exports={prepare};
