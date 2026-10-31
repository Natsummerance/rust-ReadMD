'use strict';
// Actual native image decode with strict token/CSP, editor preview and reopen.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),vm=require('node:vm');
const {prepare}=require('./macos-native-system-host.cjs'),root=path.resolve(__dirname,'..'),workspace=path.dirname(root),hash=f=>crypto.createHash('sha256').update(fs.readFileSync(f)).digest('hex');
async function run(app,out){
 assert(out.startsWith(workspace+path.sep));fs.mkdirSync(out,{mode:0o700});const docs=path.join(out,'documents'),profile=path.join(out,'profile'),tmp=path.join(out,'tmp'),home=path.join(out,'native-home');for(const p of[docs,profile,tmp,home])fs.mkdirSync(p,{mode:0o700});fs.mkdirSync(path.join(docs,'readmd-assets'));
 const image=path.join(docs,'readmd-assets/观测 曲线.svg'),file=path.join(docs,'中文 路径.md'),content='# Native resources\n\n![Shared figure](<readmd-assets/观测 曲线.svg>)\n';
 fs.writeFileSync(image,'<svg xmlns="http://www.w3.org/2000/svg" width="480" height="180"><rect width="480" height="180" fill="#6160ec"/><text x="30" y="100" fill="white" font-size="32">ReadMD</text></svg>');fs.writeFileSync(file,content);const originalHash=hash(image);
 fs.writeFileSync(path.join(profile,'.readmd-native-qa-owned'),'readmd-synthetic-native-qa-v1');fs.writeFileSync(path.join(profile,'settings.json'),JSON.stringify({lang:'en',closeToTray:false,autoCheckUpdate:false}));
 const script=String.raw`(async()=>{const checks=[],check=(ok,key)=>{if(!ok)throw Error(key);checks.push(key);},until=async fn=>{const end=Date.now()+25000;while(Date.now()<end){if(await fn())return;await new Promise(r=>setTimeout(r,40));}throw Error('resource_condition_timeout');},decoded=host=>{const i=host?.querySelector('img');return i?.naturalWidth===480&&i.complete&&i.src.startsWith('data:image/svg+xml')&&!i.hasAttribute('aria-busy');};try{
 await __readmdSessionReady;await loadFile(${JSON.stringify(file)});await until(()=>decoded($('content')));check(true,'native_chinese_resource_decoded');
 const raw='/raw?p='+encodeURIComponent(${JSON.stringify(image)});check((await fetch(raw)).status===403,'raw_without_token_still_forbidden');check((await apiFetch(raw)).ok,'authenticated_resource_bytes_available');
 check(!$('content').querySelector('img').src.includes(APP_TOKEN),'credential_absent_from_image_url');
 await toggleEdit();setPvLayout('right');await until(()=>decoded($('preview-pane')));check(true,'editor_preview_resource_decoded');check(!getActiveTab().isDirty,'preview_does_not_dirty_document');
 await toggleEdit();await closeTab(getActiveTab().id);await loadFile(${JSON.stringify(file)});await until(()=>decoded($('content')));check(true,'reopened_resource_decoded');
 const modules=await(await apiFetch('/api/modules')).json();check(modules.modules.ocr==='idle'&&modules.modules.transcribe==='idle','model_services_idle');
 window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:true,phase:'resources',checks}}));py.window_control('exit');
 }catch(e){window.ipc.postMessage(JSON.stringify({readmd_native_qa_result:{ok:false,phase:'resources',checks:[...checks,/^[a-z0-9_]{1,110}$/.test(e.message)?e.message:'resource_check_failed']}}));window.ipc.postMessage('readmd:quit');}})()`;
 new vm.Script(script);fs.writeFileSync(path.join(profile,'native-qa.js'),script,{mode:0o600});const host=prepare(app,out,{tmp,home}),instance=await host.launch({docs,profile,log:path.join(out,'native.log')}),report={sourceHost:'LaunchServices / Cocoa WKWebView',strictToken:true,productionCsp:true,modelsRequested:false,binarySha256:host.binarySha256,bootSha256:hash(path.join(root,'assets/readmd.boot.js')),harnessSha256:hash(__filename),passed:false};
 try{await instance.wait(90000);report.result=JSON.parse(fs.readFileSync(path.join(profile,'native-qa-result.json')));assert(report.result.ok,report.result.checks.join(','));assert.equal(hash(image),originalHash);assert.equal(fs.readFileSync(file,'utf8'),content);report.originalBytesUnchanged=true;report.passed=true;}finally{await instance.cleanup();for(const p of[tmp,home])fs.rmSync(p,{recursive:true,force:true});fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx',mode:0o600});}
 console.log(JSON.stringify({nativeResourcePassed:report.passed,checks:report.result.checks.length}));
}
if(require.main===module){assert.equal(process.argv.length,4);run(...process.argv.slice(2).map(p=>path.resolve(p))).catch(e=>{console.error(e.message);process.exitCode=1;});}
