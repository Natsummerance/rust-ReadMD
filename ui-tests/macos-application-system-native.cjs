'use strict';
// Reuse the actual Cocoa menu/delegate driver in a normal system-launched app.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),vm=require('node:vm');
const {driver,original,addition}=require('./macos-application-native.cjs'),{prepare}=require('./macos-native-system-host.cjs');
const root=path.resolve(__dirname,'..'),workspace=path.dirname(root),sha=b=>crypto.createHash('sha256').update(b).digest('hex');
async function run(app,out){
 assert(out.startsWith(workspace+path.sep));fs.mkdirSync(out,{mode:0o700});const docs=path.join(out,'documents'),profile=path.join(out,'profile'),tmp=path.join(out,'tmp'),home=path.join(out,'native-home');for(const p of [docs,profile,tmp,home])fs.mkdirSync(p,{mode:0o700});
 const file=path.join(docs,'中文 原生生命周期.md');fs.writeFileSync(file,original,{flag:'wx'});fs.writeFileSync(path.join(profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1',{flag:'wx',mode:0o600});fs.writeFileSync(path.join(profile,'settings.json'),JSON.stringify({lang:'en',closeToTray:true,autoCheckUpdate:false}));
 const report={nativeHost:'LaunchServices / Cocoa WKWebView / NSMenu key equivalents / NSApplication delegate',modelsRequested:false,productionCsp:true,strictAuth:true,bootSha256:sha(fs.readFileSync(path.join(root,'assets/readmd.boot.js'))),harnessSha256:sha(fs.readFileSync(__filename)),driverSha256:sha(fs.readFileSync(require.resolve('./macos-application-native.cjs'))),phases:[],passed:false};
 try{const host=prepare(app,out,{tmp,home});report.binarySha256=host.binarySha256;
  for(const phase of ['lifecycle','reopen']){const script=driver(phase,file);new vm.Script(script);fs.writeFileSync(path.join(profile,'native-qa.js'),script,{mode:0o600});const instance=await host.launch({profile,docs,log:path.join(out,phase+'.log')}),entry={phase,host:instance.receipt,passed:false};report.phases.push(entry);
   try{await instance.wait(90000);const r=JSON.parse(fs.readFileSync(path.join(profile,'native-qa-result.json')));fs.renameSync(path.join(profile,'native-qa-result.json'),path.join(out,phase+'.json'));entry.result=r;assert.equal(r.ok,true,r.checks.join(','));assert.equal(fs.readFileSync(file,'utf8'),original+addition);entry.passed=true;}finally{await instance.cleanup();}}
  report.passed=true;
 }finally{fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx',mode:0o600});for(const p of [tmp,home])fs.rmSync(p,{recursive:true,force:true});}
 console.log(JSON.stringify({nativeSystemApplicationPassed:report.passed,checks:report.phases.reduce((n,p)=>n+(p.result?.checks.length||0),0),phases:report.phases.length}));
}
if(require.main===module){assert.equal(process.argv.length,4);run(...process.argv.slice(2).map(p=>path.resolve(p))).catch(e=>{console.error(e.message);process.exitCode=1;});}
