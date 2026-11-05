'use strict';
const fs=require('node:fs'),path=require('node:path'),{chromium}=require('playwright');
const out=process.env.READMD_VSCODE_OPENAPI_AUDIT,delay=ms=>new Promise(r=>setTimeout(r,ms));
(async()=>{let browser;try{
 const deadline=Date.now()+90000;
 while(!browser&&Date.now()<deadline){try{browser=await chromium.connectOverCDP('http://127.0.0.1:'+process.env.READMD_VSCODE_DEBUG_PORT);}catch{await delay(200);}}
 if(!browser)throw Error('Native VS Code debugging endpoint did not start');
 const page=browser.contexts().flatMap(c=>c.pages()).find(p=>p.url().includes('workbench'));if(!page)throw Error('Native workbench missing');
 async function waitFile(name){const file=path.join(out,name);while(!fs.existsSync(file)){if(Date.now()>=deadline)throw Error('Native projection handshake deadline: '+name);let s;try{s=JSON.parse(fs.readFileSync(path.join(out,'state.json'),'utf8'));}catch{}if(s?.phase==='failed')throw Error('Extension Host failed during '+name);await delay(40);}return fs.readFileSync(file,'utf8');}
 const renderFrame=()=>page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
 for(const phase of ['cancel','accept','metadata-only','source-conflict','document-conflict']){
  let expectedChanges;
  while(Date.now()<deadline){let state;try{state=JSON.parse(fs.readFileSync(path.join(out,'state.json'),'utf8'));}catch{}if(state?.phase===phase){expectedChanges=state.expectedChanges;break;}if(state?.phase==='failed')throw Error('Extension Host failed before '+phase);await delay(80);}
  if(expectedChanges!==({cancel:4,accept:4,'metadata-only':0,'source-conflict':3,'document-conflict':3})[phase])throw Error('Fixed complex-fixture change count missing for '+phase);
  const toast=page.locator('.notification-toast').filter({hasText:'Review '+expectedChanges+' API changes.'}).last();
  await toast.waitFor({state:'visible',timeout:30000});
  await page.screenshot({path:path.join(out,phase+'.png')});fs.writeFileSync(path.join(out,phase+'.ready'),'');
  if(['cancel','accept','metadata-only'].includes(phase)){
   const summary=JSON.parse(await waitFile(phase+'.summary.checked'));if(summary.phase!==phase||summary.raw!==false||!summary.targetUnchanged||!summary.readonlyProviderPair||!summary.reviewId)throw Error('Owned Summary proof missing');
   await toast.getByRole('button',{name:'View raw text',exact:true}).click();await toast.getByRole('button',{name:'Show readable diff',exact:true}).waitFor({state:'visible',timeout:5000});
   fs.writeFileSync(path.join(out,phase+'.raw.ready'),JSON.stringify({phase,reviewId:summary.reviewId}));
   const raw=JSON.parse(await waitFile(phase+'.raw.checked'));if(raw.phase!==phase||raw.reviewId!==summary.reviewId||raw.raw!==true||!raw.targetUnchanged)throw Error('Owned Raw proof missing');
   await renderFrame();await page.screenshot({path:path.join(out,phase+'.raw.png')});
   if(phase!=='accept'){
    await toast.getByRole('button',{name:'Show readable diff',exact:true}).click();await toast.getByRole('button',{name:'View raw text',exact:true}).waitFor({state:'visible',timeout:5000});
    fs.writeFileSync(path.join(out,phase+'.summary-again.ready'),JSON.stringify({phase,reviewId:summary.reviewId}));
    const again=JSON.parse(await waitFile(phase+'.summary-again.checked'));if(again.phase!==phase||again.reviewId!==summary.reviewId||again.raw!==false||again.leftSha256!==summary.leftSha256||again.rightSha256!==summary.rightSha256)throw Error('Exact Summary restoration missing');
    await renderFrame();await page.screenshot({path:path.join(out,phase+'.summary-again.png')});
   }
  }
  await waitFile(phase+'.go');
  await toast.getByRole('button',{name:['cancel','metadata-only'].includes(phase)?'Cancel':'Accept changes',exact:true}).click();fs.writeFileSync(path.join(out,phase+'.clicked'),'');
 }
}finally{if(browser)await browser.close();}})().catch(error=>{fs.writeFileSync(path.join(out,'driver-error.txt'),error.message);console.error(error.message);process.exitCode=1;});
