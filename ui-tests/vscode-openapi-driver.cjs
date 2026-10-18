'use strict';
const fs=require('node:fs'),path=require('node:path'),{chromium}=require('playwright');
const out=process.env.READMD_VSCODE_OPENAPI_AUDIT,delay=ms=>new Promise(r=>setTimeout(r,ms));
(async()=>{let browser;try{
 const deadline=Date.now()+90000;
 while(!browser&&Date.now()<deadline){try{browser=await chromium.connectOverCDP('http://127.0.0.1:'+process.env.READMD_VSCODE_DEBUG_PORT);}catch{await delay(200);}}
 if(!browser)throw Error('Native VS Code debugging endpoint did not start');
 const page=browser.contexts().flatMap(c=>c.pages()).find(p=>p.url().includes('workbench'));if(!page)throw Error('Native workbench missing');
 for(const phase of ['cancel','accept','source-conflict','document-conflict']){
  while(Date.now()<deadline){let state;try{state=JSON.parse(fs.readFileSync(path.join(out,'state.json'),'utf8'));}catch{}if(state?.phase===phase)break;if(state?.phase==='failed')throw Error('Extension Host failed before '+phase);await delay(80);}
  const toast=page.locator('.notification-toast').filter({hasText:'Review 1 API changes.'}).last();
  await toast.waitFor({state:'visible',timeout:30000});
  await page.screenshot({path:path.join(out,phase+'.png')});fs.writeFileSync(path.join(out,phase+'.ready'),'');
  while(!fs.existsSync(path.join(out,phase+'.go'))&&Date.now()<deadline)await delay(50);
  await toast.getByRole('button',{name:phase==='cancel'?'Cancel':'Accept changes',exact:true}).click();fs.writeFileSync(path.join(out,phase+'.clicked'),'');
 }
}finally{if(browser)await browser.close();}})().catch(error=>{fs.writeFileSync(path.join(out,'driver-error.txt'),error.message);console.error(error.message);process.exitCode=1;});
