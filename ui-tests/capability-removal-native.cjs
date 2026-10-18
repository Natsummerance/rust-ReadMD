'use strict';
// Actual formal-CSP Windows UI, genuine QA-only TeX pack, cancellation/rollback.
// No fake engine, API mocks, production profile edits or source package writes.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),net=require('node:net');
const {chromium}=require('playwright'),{startNative}=require('./native-session.cjs');
const root=path.resolve(__dirname,'..'),data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/removal-native-')),docs=path.join(data,'documents');fs.mkdirSync(docs);
const hash=p=>crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const report={passed:false,platform:'windows-x64',nativeCsp:true,qaOnly:true,redistributionApproved:false,checks:[],timings:{}};
let native,page,stage='input',lastJob=null;const phases=[];let cancelSent=0;
const delay=ms=>new Promise(r=>setTimeout(r,ms));
async function until(f,ms=180000){const end=Date.now()+ms;while(!await f()){assert(Date.now()<end,'Timed out: '+stage);await delay(30);}}
async function stop(){if(!native)return;await native.browser.close().catch(()=>{});const c=native.server.child;if(c.exitCode===null){const exited=new Promise(r=>c.once('exit',r));c.kill();await Promise.race([exited,delay(3000)]);}native=null;}
async function port(){const s=net.createServer();await new Promise(r=>s.listen(0,'127.0.0.1',r));const p=s.address().port;await new Promise(r=>s.close(r));return p;}
async function start(){const previous=process.env.PATH;process.env.PATH=path.join(process.env.WINDIR,'System32');try{native=await startNative(root,data,chromium,await port());}finally{process.env.PATH=previous;}page=native.page;await page.evaluate(()=>window.__readmdSessionReady);page.on('pageerror',e=>report.pageErrors.push(e.message));
 page.on('request',r=>{if(r.url().includes('/api/capabilities/packs')){try{if(r.postDataJSON()?.action==='cancel_download')cancelSent=Date.now();}catch{}}});
 page.on('response',async r=>{if(!r.url().includes('/api/capabilities/packs'))return;try{const d=await r.json();if(d.operation==='uninstall'&&d.job){lastJob=d;phases.push({phase:d.phase,done:d.files_done,total:d.files_total,at:Date.now()});if(cancelSent&&['cancelling','cancelled'].includes(d.phase)&&report.timings.cancelAckMs===undefined)report.timings.cancelAckMs=Date.now()-cancelSent;}}catch{}});
}
const api=(route,body)=>page.evaluate(async({route,body})=>{const r=await apiFetch(route,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});return {status:r.status,data:await r.json()};},{route,body});
const pack=body=>api('/api/capabilities/packs',body);
async function openPlugins(){await page.evaluate(async()=>{await i18n.setLanguage('zh-CN',false);window.openPluginModal();await window.refreshOfflineCapabilityPack();});await page.locator('#plugin-modal').waitFor({state:'visible'});}
async function beginRemoval(){lastJob=null;await page.locator('#plugin-capability-uninstall-latex').click();await page.locator('#confirm-modal').waitFor({state:'visible'});await page.locator('#confirm-action').click();}
function cloneAssets(source,destination){fs.mkdirSync(destination,{recursive:true});for(const e of fs.readdirSync(source,{withFileTypes:true})){const from=path.join(source,e.name),to=path.join(destination,e.name);assert(!e.isSymbolicLink(),'Unexpected asset link');if(e.isDirectory())cloneAssets(from,to);else if(e.isFile())fs.linkSync(from,to);}}
(async()=>{try{
 assert(process.env.READMD_BIN&&process.env.READMD_QA_TEX_FIXTURE,'Explicit current binary and unapproved QA fixture required');
 report.pageErrors=[];report.binarySha256=hash(process.env.READMD_BIN);report.scriptSha256=hash(__filename);report.bootSha256=hash(path.join(root,'assets/readmd.boot.js'));
 const fixture=path.resolve(process.env.READMD_QA_TEX_FIXTURE),receipt=JSON.parse(fs.readFileSync(path.join(fixture,'receipt.json')));assert.equal(receipt.qaOnly,true);assert.equal(receipt.redistributionApproved,false);
 const assets=path.join(data,'assets');cloneAssets(path.join(root,'assets'),assets);
 // Only the QA engine lock/catalog differ from current assets. Replace the
 // new hardlink itself before copying; never write through it into the repo.
 for(const name of ['catalog.json','latex-2026.10-r1-windows-x64.lock.json']){const destination=path.join(assets,'capabilities',name);if(fs.existsSync(destination))fs.unlinkSync(destination);fs.copyFileSync(path.join(receipt.assets,'capabilities',name),destination);}
 const lock=JSON.parse(fs.readFileSync(path.join(assets,'capabilities/latex-2026.10-r1-windows-x64.lock.json')));assert.equal(lock.review.redistributionApproved,false);
 const uploads=path.join(data,'uploads');fs.mkdirSync(uploads);const input=path.join(uploads,'QA.readmd-pack');fs.linkSync(receipt.pack,input);report.packSha256=hash(input);report.fileCount=lock.files.length;assert.equal(lock.files.length,34822);
 process.env.READMD_ASSETS_DIR=assets;process.env.READMD_WORKSPACE=docs;process.env.READMD_NATIVE_LOG_FILE=path.join(data,'native.log');
 stage='native install';console.log(JSON.stringify({stage,data:path.relative(root,data)}));await start();await page.setViewportSize({width:1160,height:820});
 let started=Date.now(),r=await pack({action:'install',id:'latex',path:input});report.timings.installMs=Date.now()-started;assert.equal(r.status,200,JSON.stringify(r.data));assert.equal(r.data.installed,true);
 const runtime=path.join(data,'capabilities/managed-v2/latex/2026.10-r1'),unknown=path.join(runtime,'Personal-note.txt');fs.writeFileSync(unknown,'Synthetic user note, preserve exact original location');const original=hash(unknown);
 report.checks.push('Current formal-CSP native host actually installs the pinned 34,822-file offline QA engine with System32-only PATH.');
 stage='confirmation cancel';await openPlugins();await page.locator('#plugin-capability-uninstall-latex').click();await page.locator('#confirm-cancel').click();assert.equal((await pack({action:'status',id:'latex'})).data.installed,true);report.checks.push('Cancelling the real confirmation leaves the installed engine intact.');
 stage='actual removal cancellation';await beginRemoval();await until(()=>lastJob?.phase==='removing'&&lastJob.files_done>0,60000);const firstJob=lastJob.job;
 const active=await pack({action:'uninstall_start',id:'latex'});assert.equal(active.data.job,firstJob);const incompatible=await pack({action:'import',id:'latex',path:input});assert.equal(incompatible.status,409);
 const cancel=page.locator('#plugin-capability-cancel-latex');assert.equal(await cancel.isEnabled(),true);const size=await cancel.boundingBox();assert(size.width>=44&&size.height>=44);
 await page.screenshot({path:path.join(data,'removing-1160.png')});await cancel.click();await until(()=>lastJob?.phase==='cancelled',180000);assert(report.timings.cancelAckMs<=200,'Cancellation feedback exceeded 200ms');
 await page.locator('.plugin-capability-operation').waitFor({state:'detached',timeout:180000});r=await pack({action:'status',id:'latex'});assert.equal(r.data.ready,true,JSON.stringify(r.data));assert.equal(hash(unknown),original);
 report.checks.push('Real plugin cancel after files moved acknowledges within 200ms, rolls back, and restores actual ready TeX; duplicate removal joins the same job and installation cannot join it.');
 stage='real output after rollback';r=await api('/api/export',{format:'pdf',content:'# After cancelled removal\n\n$E=mc^2$\n',baseDir:docs,out_path:path.join(docs,'After-cancel.pdf'),options:{pdf:{engine:'pdflatex'},cover:{enabled:false},toc:{enabled:false}}});assert.equal(r.data.ok,true,JSON.stringify(r.data));assert(fs.statSync(path.join(docs,'After-cancel.pdf')).size>1000);report.checks.push('The restored genuine engine creates an actual PDF after cancellation.');
 stage='normal removal progress and commit';await page.setViewportSize({width:1024,height:680});started=Date.now();await beginRemoval();await until(()=>lastJob?.phase==='committing',180000);
 assert.equal(await page.locator('#plugin-capability-cancel-latex').isDisabled(),true);const late=await pack({action:'cancel_download',job:lastJob.job});assert.equal(late.status,409);await page.screenshot({path:path.join(data,'committing-1024.png')});
 await until(()=>lastJob?.phase==='removed',180000);report.timings.normalUninstallMs=Date.now()-started;report.result=lastJob.result;assert.equal(lastJob.result.installed,false);assert.equal(lastJob.result.cleanup_pending,false);assert.equal(lastJob.result.unknown_files_preserved,true);assert.equal(hash(unknown),original);
 for(const f of lock.files)assert(!fs.existsSync(path.join(runtime,f.path)),'Locked file remains: '+f.path);
 report.checks.push('Real UI progress advances to committed removal, disables cancellation at the atomic boundary, removes all known files and keeps unknown user bytes at the exact source path.');
 await page.locator('.plugin-capability-operation').waitFor({state:'detached',timeout:180000});await page.screenshot({path:path.join(data,'completed-1024.png')});
 stage='native restart';await stop();await start();r=await pack({action:'status',id:'latex'});assert.equal(r.data.installed,false);assert.equal(hash(unknown),original);assert.deepEqual(report.pageErrors,[]);report.checks.push('A real host restart does not resurrect the removed engine; unknown source files persist and native UI has no page errors.');report.passed=true;
}catch(error){report.failedStage=stage;report.error=String(error.message).replaceAll(data,'<owned-qa>').replaceAll(root,'<workspace>').slice(0,1800);if(page)await page.screenshot({path:path.join(data,'failure.png')}).catch(()=>{});process.exitCode=1;}
finally{await stop();report.recordedAt=new Date().toISOString();report.phases=phases;report.scope='Current actual Windows CSP UI and genuine QA-only pinned TeX: install, confirmation, mid-removal cancellation, rollback output, duplicate/isolation, progress/commit, unknown-file protection and restart. Not cold-performance, formal redistribution, full disk/history/platform/B3 certification.';fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,report:path.relative(root,path.join(data,'report.json')),failedStage:report.failedStage}));}
})();
