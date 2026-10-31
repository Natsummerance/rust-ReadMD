'use strict';
// Same fixed inputs, production frontend and byte oracles as the owned native
// workflow; launches each independent process through normal LaunchServices.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict'),vm=require('node:vm');
const {driver,encode,before,inserted,external}=require('./macos-workflow-native.cjs');
const {prepare}=require('./macos-native-system-host.cjs');
const root=path.resolve(__dirname,'..'),workspace=path.dirname(root),pause=ms=>new Promise(r=>setTimeout(r,ms)),sha=b=>crypto.createHash('sha256').update(b).digest('hex');
async function run(app,out){
 assert(out.startsWith(workspace+path.sep));fs.mkdirSync(out,{mode:0o700});const docs=path.join(out,'documents'),profile=path.join(out,'profile'),tmp=path.join(out,'tmp'),home=path.join(out,'native-home');for(const d of [docs,profile,tmp,home])fs.mkdirSync(d,{mode:0o700});
 const file=path.join(docs,'中文 路径.md');fs.writeFileSync(file,encode(before),{flag:'wx'});fs.writeFileSync(path.join(profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1',{flag:'wx',mode:0o600});fs.writeFileSync(path.join(profile,'settings.json'),JSON.stringify({lang:'en',closeToTray:false,autoCheckUpdate:false}));
 const report={nativeHost:'LaunchServices / Cocoa WKWebView',systemRegistration:true,defaultAssociationsChanged:false,modelsRequested:false,productionCsp:true,strictTokenMode:true,chooserOrAuthShim:false,controlledCleanupAfterDirtyQuitCancel:true,harnessSha256:sha(fs.readFileSync(__filename)),driverSha256:sha(fs.readFileSync(require.resolve('./macos-workflow-native.cjs'))),bootSha256:sha(fs.readFileSync(path.join(root,'assets/readmd.boot.js'))),phases:[],passed:false};
 try{const host=prepare(app,out,{tmp,home});report.binarySha256=host.binarySha256;report.bundleIdentifier=host.bundleIdentifier;report.observerSha256=host.helperSha256;
  for(const phase of ['edit_save','restore_close','empty_home','blocked_rename','external_conflict']){
   if(phase==='blocked_rename')fs.writeFileSync(path.join(docs,'links.md'),'[说明](<中文 路径.md>)\r\n',{flag:'wx'});
   const script=driver(phase,file);new vm.Script(script);fs.writeFileSync(path.join(profile,'native-qa.js'),script,{mode:0o600});const resultFile=path.join(profile,'native-qa-result.json'),entry={phase,passed:false};report.phases.push(entry);
   const instance=await host.launch({profile,docs,log:path.join(out,phase+'.log')});entry.host=instance.receipt;
   try{
    if(phase==='external_conflict'){
     const end=Date.now()+30000;while(!fs.existsSync(resultFile)&&!instance.launcherStopped&&Date.now()<end)await pause(30);
     const ready=JSON.parse(fs.readFileSync(resultFile));assert(ready.ok&&ready.phase==='external_ready');fs.renameSync(resultFile,path.join(out,'external-ready.json'));fs.writeFileSync(file,encode(external));
    }
    await instance.wait();const result=JSON.parse(fs.readFileSync(resultFile));fs.renameSync(resultFile,path.join(out,phase+'.json'));entry.result=result;assert.equal(result.phase,phase);assert.equal(result.ok,true,result.checks.join(','));
    assert.deepEqual(fs.readFileSync(file),encode(phase==='external_conflict'?external:before+inserted),'exact BOM/CRLF and external bytes');
    assert.deepEqual(fs.readdirSync(docs).sort(),(['blocked_rename','external_conflict'].includes(phase)?['links.md','中文 路径.md']:['中文 路径.md']).sort(),'no extra document or resource directory');
    if(['blocked_rename','external_conflict'].includes(phase))assert.equal(fs.readFileSync(path.join(docs,'links.md'),'utf8'),'[说明](<中文 路径.md>)\r\n');entry.passed=true;
   }finally{await instance.cleanup();}
  }report.passed=true;
 }finally{fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx',mode:0o600});for(const d of [tmp,home])fs.rmSync(d,{recursive:true,force:true});}
 console.log(JSON.stringify({nativeSystemWorkflowPassed:report.passed,phases:report.phases.length,checks:report.phases.reduce((n,p)=>n+p.result.checks.length,0)}));
}
if(require.main===module){const args=process.argv.slice(2);assert.equal(args.length,2);run(...args.map(p=>path.resolve(p))).catch(e=>{console.error(e.message);process.exitCode=1;});}
