'use strict';
// Actual native hosts: an owned stale CDP advertisement must not hide the new
// instance or disconnect a different live host. No mocked transport or UI.
const fs=require('node:fs'),path=require('node:path'),net=require('node:net'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const {chromium}=require('playwright'),{startNative}=require('./native-session.cjs');
const root=path.resolve(__dirname,'..'),base=path.join(root,'.cache/community-upgrade/verification/2026-10-12-docx-note-import');
const decoyProfile=path.join(root,'.cache/capability-upgrade/docx-note-import-5950VR'),targetProfile=path.join(root,'.cache/capability-upgrade/docx-note-import-Gei9Bc');
const advertisementDir=path.join(targetProfile,'000-owned-stale-debug'),advertisement=path.join(advertisementDir,'DevToolsActivePort');
const sha=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const report={passed:false,synthetic:true,formalCsp:true,platform:'windows-x64',automaticRetries:0,binarySha256:sha(process.env.READMD_BIN),bootSha256:sha(path.join(root,'assets/readmd.boot.js')),testSha256:sha(__filename),harnessSha256:sha(path.join(__dirname,'native-session.cjs')),checks:[]};
let decoy,target;
const freePort=()=>new Promise((resolve,reject)=>{const server=net.createServer();server.on('error',reject);server.listen(0,'127.0.0.1',()=>{const port=server.address().port;server.close(()=>resolve(port));});});
function ports(dir){return fs.readdirSync(dir,{withFileTypes:true}).flatMap(entry=>{if(entry.isSymbolicLink())return[];const file=path.join(dir,entry.name);return entry.isDirectory()?ports(file):entry.name==='DevToolsActivePort'?[Number(fs.readFileSync(file,'utf8').split('\n')[0])]:[];});}
async function stop(host){if(!host)return;await host.browser.close().catch(()=>{});const child=host.server.child;const stopped=()=>child.exitCode!==null||child.signalCode!==null;if(!stopped()){const done=new Promise(resolve=>child.once('exit',resolve));child.kill();await Promise.race([done,new Promise(resolve=>setTimeout(resolve,2000))]);}assert(stopped(),'Owned host must terminate before restart');}
(async()=>{try{
 assert(!fs.existsSync(advertisementDir),'Owned stale advertisement must be new');
 process.env.READMD_WORKSPACE=path.join(decoyProfile,'documents');decoy=await startNative(root,decoyProfile,chromium,await freePort());
 await decoy.page.evaluate(()=>window.__readmdSessionReady);
 const decoyOrigin=new URL(decoy.page.url()).origin;
 const debugPorts=ports(decoyProfile);assert.equal(debugPorts.length,1,'One real owned debug endpoint required');
 fs.mkdirSync(advertisementDir);fs.writeFileSync(advertisement,String(debugPorts[0])+'\n');
 process.env.READMD_WORKSPACE=path.join(targetProfile,'documents');
 for(let index=0;index<3;index++){
  const port=await freePort();const started=Date.now();target=await startNative(root,targetProfile,chromium,port);
  await target.page.evaluate(()=>window.__readmdSessionReady);
  assert.equal(new URL(target.page.url()).origin,'http://127.0.0.1:'+port);
  assert.notEqual(new URL(target.page.url()).origin,decoyOrigin);
  assert.equal(await decoy.page.evaluate(()=>Boolean(window.__readmdAppReady)),true,'Unused CDP connection must not close the other native host');
  report.checks.push({operation:index===0?'start with earlier live stale endpoint':'immediate same-profile restart',seconds:Math.round((Date.now()-started)/10)/100,correctOwnedInstance:true,otherOwnedHostIntact:true});
  await stop(target);target=null;
 }
 report.passed=true;
}catch(error){report.error=String(error.stack||error).replaceAll(root,'<workspace>');process.exitCode=1;}
finally{
 await stop(target).catch(error=>{report.passed=false;report.stopError=error.message;process.exitCode=1;});
 await stop(decoy).catch(error=>{report.passed=false;report.stopError=error.message;process.exitCode=1;});
 // Remove only this task's tiny, validated advertisement; retain QA profiles.
 if(path.dirname(advertisement)===advertisementDir&&advertisementDir.startsWith(targetProfile+path.sep)&&fs.existsSync(advertisement)){fs.unlinkSync(advertisement);fs.rmdirSync(advertisementDir);}
 report.recordedAt=new Date().toISOString();report.scope='Native QA harness instance discovery and restart; not production startup performance or complete B3.';
 fs.writeFileSync(path.join(base,'native-session-restart-report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,report:path.relative(root,path.join(base,'native-session-restart-report.json'))}));
}})();
