'use strict';
// Owned headless profile and production CSP/auth. The bridge supplies settings
// bootstrap only, plus an opt-in owned fixture chooser. File, chat, review,
// capability installation, code and save services remain real.
// Scratch was freshly created here; rm never follows internal symbolic links.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const ROOT=path.resolve(__dirname,'../..');
const {startKernel}=require(ROOT+'/tools/test/v006-refactor-host.cjs');
const sha=value=>crypto.createHash('sha256').update(value).digest('hex'),pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function until(check,label,ms=10000){const end=Date.now()+ms;while(Date.now()<end){if(await check())return;await pause(15);}throw Error('Deadline: '+label);}
async function openOwned(output,configure,{binary,playwright,chooserFile}={}){
 if(!binary||!path.isAbsolute(binary))throw Error('Explicit prebuilt binary required');
 if(!fs.existsSync(output))fs.mkdirSync(output,{mode:0o700});
 if(fs.lstatSync(output).isSymbolicLink()||!fs.lstatSync(output).isDirectory()||fs.readdirSync(output).length)throw Error('Fresh owned browser evidence directory required');
 const paths=Object.fromEntries(['profile','documents','tmp','home','browser-profile'].map(name=>[name,path.join(output,name)]));for(const dir of Object.values(paths))fs.mkdirSync(dir,{mode:0o700});Object.assign(process.env,{TMPDIR:paths.tmp,TMP:paths.tmp,TEMP:paths.tmp});
 const scratch=['tmp','home','browser-profile'].map(name=>({path:paths[name],identity:fs.lstatSync(paths[name])}));
 function cleanup(){for(const item of scratch){const now=fs.lstatSync(item.path);if(!now.isDirectory()||now.isSymbolicLink()||now.ino!==item.identity.ino||now.dev!==item.identity.dev)throw Error('Owned browser scratch changed');fs.rmSync(item.path,{recursive:true});}}
 const host=await startKernel(binary,paths.profile,paths.documents),api=async(route,body)=>{const r=await host.api(route,body===undefined?{}:{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});assert.equal(r.status,200);return r.json();};let context;
 try{
  await configure?.(api,paths);const root=await host.api('/'),base=new URL(root.url).origin;
  let chooserAdapterCalls=0;
  const chosenFile=chooserFile===undefined?null:path.resolve(paths.documents,chooserFile);
  function chooseOwnedFile(){
   assert(chosenFile&&path.dirname(chosenFile)===paths.documents,'Chooser is restricted to a direct owned workspace fixture');
   const metadata=fs.lstatSync(chosenFile);assert(metadata.isFile()&&!metadata.isSymbolicLink());assert.equal(fs.realpathSync(chosenFile),chosenFile);
   chooserAdapterCalls++;return chosenFile;
  }
  if(chosenFile){assert.equal(typeof chooserFile,'string');assert(!path.isAbsolute(chooserFile));chooseOwnedFile();chooserAdapterCalls=0;}
  const {chromium}=require(playwright||require.resolve('playwright',{paths:[ROOT+'/ui-tests']}));
  const env=Object.fromEntries(Object.entries(process.env).filter(([name])=>/^(PATH|SYSTEMROOT|WINDIR|COMSPEC|PATHEXT|LANG|LC_ALL)$/i.test(name)));
  Object.assign(env,{TMPDIR:paths.tmp,CFFIXED_USER_HOME:paths.home,CHROME_LOG_FILE:path.join(output,'chrome.log')});
  const channel=process.env.READMD_CODE_BROWSER||'chrome';
  context=await chromium.launchPersistentContext(paths['browser-profile'],{channel:channel==='chromium'?undefined:channel,headless:true,env,viewport:{width:1160,height:820},bypassCSP:false,serviceWorkers:'block',args:['--disable-crash-reporter','--disable-breakpad','--disable-background-networking','--disable-component-update','--password-store=basic']});
  const page=context.pages()[0]||await context.newPage(),errors=[];page.setDefaultTimeout(10000);page.on('pageerror',error=>errors.push(String(error.message).replaceAll(output,'<owned>').slice(0,300)));
  await context.route('**/*',route=>new URL(route.request().url()).origin===base?route.continue():route.abort());
  if(chosenFile)await page.exposeBinding('__readmdOwnedChooseFile',source=>{assert.equal(source.page,page);return chooseOwnedFile();});
  await page.addInitScript(({chooserAdapter})=>{const headers=()=>{const token=document.querySelector('meta[name="readmd-app-token"]')?.content||'';return{'Content-Type':'application/json','X-ReadMD-App-Token':token,'X-ReadMD-Token':token};};window.pywebview={api:{get_settings:async()=>{const value=await(await fetch('/api/settings',{headers:headers()})).json();return value?.settings&&typeof value.ok==='boolean'?value.settings:value;},save_settings:async value=>(await fetch('/api/settings',{method:'POST',headers:headers(),body:JSON.stringify(value)})).json(),get_system_language:async()=>'zh-CN',get_autostart:async()=>false,get_app_info:async()=>({}),start_modules:async()=>true}};if(chooserAdapter)window.pywebview.api.choose_any_file=()=>window.__readmdOwnedChooseFile();},{chooserAdapter:Boolean(chosenFile)});
  const boot=page.waitForResponse(r=>new URL(r.url()).pathname==='/assets/readmd.boot.js');await page.goto(base);assert.equal(sha(await(await boot).body()),sha(fs.readFileSync(ROOT+'/assets/readmd.boot.js')));
  const csp=await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content');assert(csp.includes("script-src 'self'"));assert(!/script-src[^;]*'unsafe-(inline|eval)'/.test(csp));
  await until(()=>page.evaluate(()=>Boolean(window.__readmdAppReady&&window.ReadMDReview&&window.ReadMDChatStream)),'owned app ready');await page.evaluate(()=>window.__readmdSessionReady);
  return{...paths,api,host,page,context,errors,get chooserAdapterUsed(){return chooserAdapterCalls>0;},get chooserAdapterCalls(){return chooserAdapterCalls;},async close(){try{await context.close();}finally{await host.stop();cleanup();}}};
 }catch(error){if(context)await context.close();await host.stop();cleanup();throw error;}
}
module.exports={ROOT,sha,pause,until,openOwned};
