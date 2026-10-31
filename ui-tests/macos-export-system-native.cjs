'use strict';
// Real system-launched reader, six live preview consumers and real snapshot
// artifacts. Explicit workspace outputs do not stand in for a save-panel test.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),vm=require('node:vm');
const {execFileSync,spawnSync}=require('node:child_process');
const {driver,content}=require('./macos-export-native.cjs'),{prepare}=require('./macos-native-system-host.cjs');
const root=path.resolve(__dirname,'..'),workspace=path.dirname(root),sha=b=>crypto.createHash('sha256').update(b).digest('hex');
function member(file,name){return execFileSync('/usr/bin/unzip',['-p',file,name],{encoding:'utf8',timeout:15000,maxBuffer:4*1024*1024});}
function text(xml){return xml.replace(/<[^>]*>/g,'').replace(/&(?:amp|lt|gt|quot|apos);/g,e=>({'&amp;':'&','&lt;':'<','&gt;':'>','&quot;':'"','&apos;':"'"})[e]);}
async function run(app,out){
 assert(out.startsWith(workspace+path.sep));fs.mkdirSync(out,{mode:0o700});const docs=path.join(out,'documents'),profile=path.join(out,'profile'),tmp=path.join(out,'tmp'),home=path.join(out,'native-home');for(const p of [docs,profile,tmp,home])fs.mkdirSync(p,{mode:0o700});
 const file=path.join(docs,'中文 导出.md'),outputs=Object.fromEntries(['pdf','docx','epub','html','tex','pptx'].map(fmt=>[fmt,path.join(docs,'Export.'+fmt)]));fs.writeFileSync(file,content,{flag:'wx'});fs.writeFileSync(path.join(profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1',{flag:'wx',mode:0o600});fs.writeFileSync(path.join(profile,'settings.json'),JSON.stringify({lang:'en',closeToTray:false,autoCheckUpdate:false}));
 const report={nativeHost:'LaunchServices / Cocoa WKWebView',systemRegistration:true,defaultAssociationsChanged:false,modelsRequested:false,productionCsp:true,strictTokenMode:true,chooserShim:false,explicitOwnedOutputs:true,harnessSha256:sha(fs.readFileSync(__filename)),driverSha256:sha(fs.readFileSync(require.resolve('./macos-export-native.cjs'))),bootSha256:sha(fs.readFileSync(path.join(root,'assets/readmd.boot.js'))),artifacts:[],passed:false};let ownedPack;const captureAck=path.join(docs,'capture.ack.md');
 try{
  if(process.env.READMD_QA_FONT_PACK){const bytes=fs.readFileSync(process.env.READMD_QA_FONT_PACK);assert.equal(sha(bytes),'037e01665e90fde0fc81adb8cda2093e5862c491794dcbe95544660479f07a69');ownedPack=path.join(docs,'fonts.readmd-pack');fs.writeFileSync(ownedPack,bytes,{flag:'wx'});fs.writeFileSync(path.join(docs,'figure.svg'),'<svg xmlns="http://www.w3.org/2000/svg" width="240" height="60"><text x="12" y="40" font-size="28">中文图像</text></svg>',{flag:'wx'});report.fontPackSha256=sha(bytes);}
  const script=driver(file,outputs,ownedPack,captureAck);new vm.Script(script);fs.writeFileSync(path.join(profile,'native-qa.js'),script,{mode:0o600});const host=prepare(app,out,{tmp,home});report.binarySha256=host.binarySha256;report.observerSha256=host.helperSha256;report.bundleIdentifier=host.bundleIdentifier;
  const instance=await host.launch({profile,docs,log:path.join(out,'native.log')});report.host=instance.receipt;
  try{const resultFile=path.join(profile,'native-qa-result.json'),waiting=instance.wait(180000);waiting.catch(()=>{});const end=Date.now()+180000;
   while(!fs.existsSync(resultFile)&&!instance.launcherStopped&&Date.now()<end)await new Promise(r=>setTimeout(r,50));
   const preview=JSON.parse(fs.readFileSync(resultFile));assert.equal(preview.ok,true,preview.checks.join(','));assert.equal(preview.phase,'capture_ready');
   const captureHelper=path.join(workspace,'.readmd-local/native-window-capture');
   if(fs.existsSync(captureHelper)){
    const probe=spawnSync(captureHelper,[String(instance.pid),String(preview.window_number)],{encoding:'utf8',timeout:10000});assert.equal(probe.status,0);report.captureProbe=JSON.parse(probe.stdout);assert(report.captureProbe.ownedWindowOnScreen);
    if(report.captureProbe.captureAllowed){const image=path.join(out,'native-pptx-preview.png'),captured=spawnSync('/usr/sbin/screencapture',['-x','-o','-l',String(preview.window_number),image],{timeout:10000});assert.equal(captured.status,0);report.previewScreenshotSha256=sha(fs.readFileSync(image));}
   }
   fs.renameSync(resultFile,path.join(out,'preview-capture-ready.json'));fs.writeFileSync(captureAck,'continue',{flag:'wx'});
   await waiting;report.result=JSON.parse(fs.readFileSync(resultFile));assert.equal(report.result.ok,true,report.result.phase+': '+report.result.checks.join(','));assert.equal(fs.readFileSync(file,'utf8'),content);
   for(const [format,file] of Object.entries(outputs)){
    const bytes=fs.readFileSync(file);assert(bytes.length>100);const artifact={format,bytes:bytes.length,sha256:sha(bytes)};
    if(format==='pdf')assert.equal(bytes.subarray(0,5).toString(),'%PDF-');
    else if(format==='docx'){const body=text(member(file,'word/document.xml'));assert(body.includes('Export contract')&&body.includes('Actual slide one.'));if(ownedPack)assert(body.includes('真实中文导出与恢复。'));}
    else if(format==='epub')assert.equal(member(file,'mimetype'),'application/epub+zip');
    else if(format==='html')assert(bytes.toString('utf8').includes('Actual slide one.'));
    else if(format==='tex')assert(bytes.toString('utf8').includes('\\documentclass'));
    else if(format==='pptx'){
     const names=execFileSync('/usr/bin/unzip',['-Z1',file],{encoding:'utf8'}).trim().split('\n'),slides=names.filter(n=>/^ppt\/slides\/slide\d+\.xml$/.test(n));assert.equal(slides.length,3);
     const xml=member(file,'ppt/presentation.xml'),size=xml.match(/<p:sldSz\b[^>]*\bcx="(\d+)"[^>]*\bcy="(\d+)"/);assert(size);assert.equal(Number(size[1])/Number(size[2]),16/9);
     const bodies=slides.map(n=>member(file,n));assert(bodies.every(s=>/<p:sp>/.test(s)&&/<a:t[ >]/.test(s)),'each slide needs actual editable text shapes');const body=bodies.map(text).join('\n');assert(body.includes('Export contract')&&body.includes('Actual slide one.')&&body.includes('Third slide'));if(ownedPack)assert(body.includes('真实中文导出与恢复。'));artifact.editableSlides=3;artifact.aspectRatio='16:9';
    }
    artifact.structuralCheck=true;report.artifacts.push(artifact);
   }
   report.passed=true;
  }finally{await instance.cleanup();}
 }finally{fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx',mode:0o600});if(ownedPack)fs.unlinkSync(ownedPack);if(fs.existsSync(captureAck))fs.unlinkSync(captureAck);for(const p of [tmp,home])fs.rmSync(p,{recursive:true,force:true});}
 console.log(JSON.stringify({nativeSystemExportsPassed:report.passed,checks:report.result.checks.length,formats:report.artifacts.length,chineseAndImage:Boolean(ownedPack)}));
}
if(require.main===module){const args=process.argv.slice(2);assert.equal(args.length,2);run(...args.map(p=>path.resolve(p))).catch(e=>{console.error(e.message);process.exitCode=1;});}
