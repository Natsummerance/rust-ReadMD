import fs from 'node:fs';
import path from 'node:path';
import {spawn} from 'node:child_process';
const delay=ms=>new Promise(r=>setTimeout(r,ms));
export function debugPorts(dir,depth=0){
 if(depth>10||!fs.existsSync(dir))return [];
 const ports=[];
 for(const e of fs.readdirSync(dir,{withFileTypes:true})){
  const p=path.join(dir,e.name);
  if(e.isDirectory())ports.push(...debugPorts(p,depth+1));
  else if(e.name==='DevToolsActivePort'){const n=Number(fs.readFileSync(p,'utf8').split('\n')[0]);if(n>0)ports.push(n);}
 }
 return [...new Set(ports)];
}
export async function startNative(root,data,chromium,port){
 const child=spawn(process.env.READMD_BIN,['--port',String(port),'--data-dir',data,'--assets',process.env.READMD_ASSETS_DIR || path.join(root,'assets'),'--workspace',process.env.READMD_WORKSPACE || path.dirname(data)],{cwd:root,windowsHide:true,stdio:['ignore','pipe','pipe'],env:{...process.env,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:'--remote-debugging-port=0 --remote-debugging-address=127.0.0.1'}});
 child.stdout.resume();child.stderr.resume();
 const end=Date.now()+45000;
 while(!debugPorts(data).length){if(child.exitCode!==null)throw Error('Native demo app exited');if(Date.now()>end)throw Error('Native debug port not ready');await delay(150);}
 const browser=await chromium.connectOverCDP('http://127.0.0.1:'+debugPorts(data)[0]);
 let page;
 while(!(page=browser.contexts().flatMap(c=>c.pages()).find(p=>p.url().startsWith('http://127.0.0.1:'+port)))){if(Date.now()>end)throw Error('Native reader page not ready');await delay(150);}
 await page.waitForFunction(()=>!!window.pywebview?.api?.get_app_info);
 return {server:{child,stop(){}},browser,page};
}
