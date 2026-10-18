'use strict';
// Real formal-CSP WebView2, genuine engines, owned legacy/private trees only.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const {chromium}=require('playwright'),{startNative}=require('./native-session.cjs');
const root=path.resolve(__dirname,'..'),data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/engine-legacy-native-中文-')),docs=path.join(data,'documents');fs.mkdirSync(docs);
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
const engines=[{id:'typst',version:'0.15.1',pack:process.env.READMD_TYPST_PACK},{id:'ocr',version:'5.5.3-r1',pack:process.env.READMD_OCR_PACK}];
const source='# 迁移后的研究记录\n\n保留原始证据，比较解释。这个结果来自独立的受管理引擎。\n\n```readmd-math v1\nfrac(α + x^2, sqrt(y))\n```\n\n| 记录 | 检查 |\n|---|---|\n| 原文 | 保留 |\n| 推论 | 校核 |\n';
const report={passed:false,nativeCsp:true,systemPathRestricted:true,platform:process.platform,checks:[]};let native,page,phase='prepare';
async function stop(){if(!native)return;const current=native;native=null;const child=current.server.child;if(child.exitCode===null&&child.signalCode===null){const exited=new Promise(r=>child.once('exit',r));child.kill();await Promise.race([exited,new Promise((_,reject)=>setTimeout(()=>reject(Error('Owned native host did not exit')),2000))]);}await current.browser.close().catch(()=>{});}
async function start(){const previous=process.env.PATH;process.env.PATH=path.join(process.env.SystemRoot,'System32');try{native=await startNative(root,data,chromium,28969);}finally{process.env.PATH=previous;}page=native.page;await page.evaluate(()=>window.__readmdSessionReady);}
const api=async(route,body)=>page.evaluate(async({route,body})=>{const response=await apiFetch(route,body?{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)}:{});return {status:response.status,data:await response.json()};},{route,body});
const pack=async(action,id,location)=>{const r=await api('/api/capabilities/packs',{action,id,...(location?{path:location}:{})});assert.equal(r.status,200,JSON.stringify(r.data));return r.data;};
function owned(file){const relative=path.relative(data,path.resolve(file));assert.ok(relative&&!relative.startsWith('..')&&!path.isAbsolute(relative));return file;}
async function minimum(name){
  const out=path.join(docs,name+'.pdf');const exported=await api('/api/export',{format:'pdf',content:source,baseDir:docs,out_path:out,options:{pdf:{engine:'typst'},footer:{pageNumbers:false}}});assert.equal(exported.data.ok,true,JSON.stringify(exported.data));
  const recognized=await api('/api/ocr?p='+encodeURIComponent(path.join(docs,'Scan.png'))+'&details=1');assert.equal(recognized.status,200,JSON.stringify(recognized.data));assert.equal(recognized.data.engine,'readmd-tesseract');assert.match(recognized.data.content,/READMD LEGACY 12345/);
  fs.writeFileSync(path.join(docs,name+'-ocr.json'),JSON.stringify(recognized.data,null,2));return out;
}
(async()=>{try{
  assert.ok(process.env.READMD_BIN&&process.env.READMD_FONTS_PACK&&engines.every(e=>e.pack),'Explicit offline assets required');
  report.binarySha256=hash(fs.readFileSync(process.env.READMD_BIN));report.scriptSha256=hash(fs.readFileSync(__filename));process.env.READMD_WORKSPACE=docs;
  fs.writeFileSync(path.join(docs,'Research.md'),source);
  phase='genuine engine installation in Unicode profile';await start();await require('./helpers/raster-fixtures.cjs')(path.join(docs,'Scan.png'),{width:1900,height:500,label:'READMD LEGACY 12345'},page);report.scanSha256=hash(fs.readFileSync(path.join(docs,'Scan.png')));assert.equal((await pack('install','fonts',process.env.READMD_FONTS_PACK)).ok,true);
  for(const e of engines){assert.equal((await pack('install',e.id,e.pack)).ok,true);e.lock=JSON.parse(fs.readFileSync(path.join(root,`assets/capabilities/${e.id}-${e.version}-windows-x64.lock.json`),'utf8'));e.private=owned(path.join(data,`capabilities/managed-v2/${e.id}/${e.version}`));e.legacy=owned(path.join(data,`capabilities/${e.id}/${e.version}`));}
  await stop();
  for(const e of engines){assert.ok(fs.lstatSync(e.private).isDirectory()&&!fs.lstatSync(e.private).isSymbolicLink());assert.ok(!fs.existsSync(e.legacy));fs.mkdirSync(path.dirname(e.legacy),{recursive:true});fs.renameSync(owned(e.private),owned(e.legacy));fs.writeFileSync(path.join(e.legacy,'user-note.txt'),'Preserve legacy engine extra');}
  report.checks.push('Genuine verified Typst/OCR are seeded into owned legacy trees only after the real host exits; fonts remain private.');
  phase='real legacy migration';await start();await minimum('Migrated');
  for(const e of engines){assert.equal((await pack('status',e.id)).ready,true);assert.equal(fs.readFileSync(path.join(e.legacy,'user-note.txt'),'utf8'),'Preserve legacy engine extra');assert.ok(!fs.existsSync(path.join(e.private,'user-note.txt')));for(const f of e.lock.files){assert.equal(hash(fs.readFileSync(path.join(e.private,f.path))),f.sha256);assert.equal(hash(fs.readFileSync(path.join(e.legacy,f.path))),f.sha256);}}
  report.checks.push('Actual native Typst PDF and managed Tesseract recognition work after startup/first-use migration, with all locked bytes verified and unknown originals retained.');
  phase='independent private copies';for(const e of engines){const f=e.lock.files[0],old=path.join(e.legacy,f.path),original=fs.readFileSync(old);fs.writeFileSync(old,'Owned legacy bytes changed');assert.equal(hash(fs.readFileSync(path.join(e.private,f.path))),f.sha256);await minimum('After-'+e.id+'-legacy-write');fs.writeFileSync(old,original);}
  report.checks.push('Writing each old engine copy cannot change the private copy or break either genuine operation; no hardlink dependency.');
  phase='persistent uninstall';for(const e of engines)assert.equal((await pack('uninstall',e.id)).ok,true);await stop();await start();
  for(const e of engines){assert.equal((await pack('status',e.id)).installed,false);assert.ok(!fs.existsSync(e.private));assert.equal(hash(fs.readFileSync(path.join(e.legacy,e.lock.files[0].path))),e.lock.files[0].sha256);assert.equal((await pack('install',e.id,e.pack)).ok,true);}
  await minimum('Explicit-reinstall');report.checks.push('After uninstall and a real host restart, valid old resources do not auto-resurrect; explicit reinstall and both actual operations still work.');
  phase='normal cleanup';for(const e of engines)assert.equal((await pack('uninstall',e.id)).ok,true);assert.equal((await pack('uninstall','fonts')).ok,true);report.passed=true;
}catch(error){report.failedStage=phase;report.error=String(error.message).replaceAll(data,'<owned-qa>').replaceAll(root,'<workspace>').slice(0,1600);process.exitCode=1;}
finally{try{await stop();}catch(error){report.cleanupError=String(error.message);report.passed=false;process.exitCode=1;}report.recordedAt=new Date().toISOString();report.scope='Real Windows current host and two genuine engines on owned legacy trees. Normal startup/first use, independent copies, persistent opt-out and reinstall; not old-client engine owner crashes, all transports, cold/long stress or release certification.';fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,report:path.relative(root,path.join(data,'report.json')),failedStage:report.failedStage}));}})();
