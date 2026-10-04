/** Read-only compatibility checks; private books never enter published assets. */
import fs from 'node:fs';import path from 'node:path';import crypto from 'node:crypto';import {createRequire}from'node:module';import{fileURLToPath}from'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const require=createRequire(import.meta.url);const{chromium}=require(path.join(process.env.READMD_UI_NODE_MODULES||path.join(root,'ui-tests/node_modules'),'@playwright/test'));
const browser=await chromium.launch();
const results=[];
try{
 const page=await browser.newPage();await page.goto('http://127.0.0.1:'+Number(process.env.READMD_DEMO_PORT||28731));await page.waitForFunction(()=>typeof apiFetch==='function');
 if(process.argv.includes('--diagram')){
  const r=await page.evaluate(async()=>{const r=await apiFetch('/api/diagram/render',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({engine:'plantuml',allow_remote:true,code:'@startuml\nstart\n:记录观察;\n:检查前提;\n:修订结论;\nstop\n@enduml'})});const d=await r.json();return{status:r.status,ok:d.ok,error_code:d.error_code,svg_characters:d.svg?.length,contains_label:d.svg?.includes('检查前提')};});
  results.push({test:'real_remote_plantuml',...r});
 }
 for(const source of process.argv.slice(2).filter(x=>!x.startsWith('--'))){
  const original=fs.readFileSync(source),sha=crypto.createHash('sha256').update(original).digest('hex');
  // Use an existing isolated recording workspace, never the library directory.
  const workspace=process.env.READMD_COMPAT_WORKSPACE;
  if(!workspace)throw Error('Isolated workspace is not available');
  const owned=path.resolve(workspace);if(!owned.startsWith(path.join(root,'showcase','.runtime')+path.sep))throw Error('Book check is outside demonstration workspace');
  const dir=path.join(owned,'private-compatibility');fs.mkdirSync(dir,{recursive:true});const copy=path.join(dir,'philosophy'+path.extname(source));fs.writeFileSync(copy,original);
  const r=await page.evaluate(async p=>{const r=await apiFetch('/api/convert?preview=1&p='+encodeURIComponent(p));const d=await r.json();return{status:r.status,ok:r.ok&&!d.error_code&&d.content?.length>0,characters:d.content?.length,error_code:d.error_code,engine:d.engine};},copy);
  if(crypto.createHash('sha256').update(fs.readFileSync(source)).digest('hex')!==sha)throw Error('Book original changed');
  results.push({test:'private_philosophy_book',format:path.extname(source),bytes:original.length,original_unchanged:true,...r});
 }
}finally{await browser.close();}
console.log(JSON.stringify(results,null,2));
if(results.some(r=>!r.ok))process.exitCode=1;
