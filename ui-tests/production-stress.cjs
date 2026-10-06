'use strict';
// Opt-in, bounded stress of the actual installed kernel. Uses only generated
// documents in an explicit isolated directory, and checks real disk contents.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { spawn, spawnSync } = require('node:child_process');
const assert = require('node:assert/strict');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const percentile = (values, p) => [...values].sort((a,b) => a-b)[Math.floor((values.length-1)*p)];
async function until(fn, timeout = 60000) { const end = Date.now()+timeout; while(Date.now()<end) { const value=await fn(); if(value)return value; await delay(100); } throw Error('Stress operation timed out'); }
async function parallel(count, concurrency, action) {
  let next = 0; const timings=[];
  await Promise.all(Array.from({length:concurrency},async()=>{
    while(next<count){const index=next++,start=Date.now();await action(index);timings.push(Date.now()-start);}
  }));
  return { count, concurrency, p50Ms:percentile(timings,.5), p95Ms:percentile(timings,.95), maxMs:Math.max(...timings) };
}
function memory(pid) {
  const result=spawnSync('powershell.exe',['-NoProfile','-NonInteractive','-Command',`$childProc=Get-Process -Id ${pid}; [PSCustomObject]@{rssBytes=$childProc.WorkingSet64;peakBytes=$childProc.PeakWorkingSet64;cpuSeconds=$childProc.CPU}|ConvertTo-Json -Compress`],{windowsHide:true,encoding:'utf8'});
  return JSON.parse(result.stdout);
}
(async()=>{
  for(const key of ['READMD_BIN','READMD_ASSETS_DIR','READMD_STRESS_DATA','READMD_STRESS_REPORT']) if(!process.env[key])throw Error(`${key} is required`);
  const data=path.resolve(process.env.READMD_STRESS_DATA), fixtures=path.join(data,'documents');
  fs.mkdirSync(fixtures,{recursive:true});
  const marker='READMD_STRESS_MARKER';
  const files=Array.from({length:64},(_,i)=>{
    const file=path.join(fixtures,`parallel-${i}.md`);fs.writeFileSync(file,`# ${marker}\n\n保存中文与 emoji 🌿\n`);return file;
  });
  const conversions=[];
  const formats={html:`<h1>${marker}</h1><p>Readable body</p>`,txt:`${marker}\n\nReadable body`,csv:`Title,Value\n${marker},42`,json:JSON.stringify({title:marker,value:42}),rs:`fn main() { println!("${marker}"); }`,js:`console.log('${marker}');`,rtf:`{\\rtf1\\ansi ${marker} readable body}`,tex:`\\documentclass{article}\\begin{document}\\section{${marker}}Readable body\\end{document}`};
  for(let i=0;i<200;i++){
    const ext=Object.keys(formats)[i%8], file=path.join(fixtures,`batch-${i}.${ext}`);
    fs.writeFileSync(file,formats[ext]);conversions.push(file);
  }
  const binary=process.env.READMD_BIN,port=Number(process.env.READMD_STRESS_PORT||28784);
  const env={...process.env};delete env.READMD_DATA_DIR;
  const child=spawn(binary,['--no-window','--port',String(port),'--data-dir',data,'--assets',process.env.READMD_ASSETS_DIR,'--workspace',fixtures],{windowsHide:true,stdio:'ignore',env});
  const origin=`http://127.0.0.1:${port}`;let token;
  const report={binarySha256:crypto.createHash('sha256').update(fs.readFileSync(binary)).digest('hex'),scenarios:[]};
  const request=async(route,body)=>{
    const response=await fetch(origin+route,{...(body===undefined?{}:{method:'POST',headers:{'Content-Type':'application/json','X-ReadMD-App-Token':token},body:JSON.stringify(body)}),signal:AbortSignal.timeout(20000)});
    return {status:response.status,data:await response.json()};
  };
  try {
    await until(async()=>{
      if(child.exitCode!==null)throw Error('Installed kernel exited');
      try {const html=await(await fetch(origin)).text();token=html.match(/name="readmd-app-token" content="([^"]+)"/)?.[1];return token;}catch{return false;}
    });
    report.before=memory(child.pid);
    for(let cycle=0;cycle<3;cycle++){
      report.scenarios.push({name:`read-cycle-${cycle+1}`,...await parallel(400,8,async i=>{
        const response=await request('/api/file?p='+encodeURIComponent(files[i%files.length]));
        assert.equal(response.status,200);assert.ok(response.data.content.includes(marker));
      })});
    }
    report.scenarios.push({name:'64-concurrent-authorized-saves-and-disk-readback',...await parallel(files.length,8,async i=>{
      const file=files[i], before=(await request('/api/file?p='+encodeURIComponent(file))).data;
      const content=`# ${marker}\n\n真实并发保存 ${i} 🌿\n`;
      const saved=await request('/api/save',{path:file,content,expected_revision:before.revision,encoding:'utf-8'});
      assert.equal(saved.status,200);assert.equal(saved.data.ok,true);assert.equal(fs.readFileSync(file,'utf8'),content);
    })});
    const shared=(await request('/api/file?p='+encodeURIComponent(files[0]))).data;
    const races=await Promise.all(Array.from({length:8},(_,i)=>request('/api/save',{path:files[0],content:`# ${marker}\nWriter ${i}`,expected_revision:shared.revision})));
    assert.equal(races.filter(r=>r.status===200&&r.data.ok).length,1);assert.equal(races.filter(r=>r.status===409&&r.data.conflict).length,7);
    report.scenarios.push({name:'same-document-save-conflict',writers:8,committed:1,conflicts:7});
    const start=Date.now(), launch=await request('/api/convert/batch',{confirm:true,paths:[...conversions,...conversions.slice(0,10)],overwrite:false});
    assert.equal(launch.status,200);assert.equal(launch.data.total,200);
    const progress=await until(async()=>{const value=(await request('/api/convert/progress?job='+launch.data.job)).data;return value.finished&&value;},120000);
    assert.equal(progress.items.filter(item=>item.status==='ok').length,200);
    for(const item of progress.items)assert.ok(fs.readFileSync(item.out,'utf8').includes(marker));
    report.scenarios.push({name:'200-file-eight-format-batch',uniqueInputs:200,duplicateInputs:10,completed:200,elapsedMs:Date.now()-start});
    const damaged=path.join(fixtures,'damaged.docx');fs.writeFileSync(damaged,'not a ZIP');
    const refusal=await request('/api/convert?p='+encodeURIComponent(damaged));
    assert.ok(refusal.status>=400);assert.equal(refusal.data.ok,false);assert.equal(fs.existsSync(damaged.replace('.docx','.md')),false);
    report.scenarios.push({name:'damaged-document-is-not-false-success',status:refusal.status});
    report.after=memory(child.pid);
    assert.ok(report.after.rssBytes-report.before.rssBytes<256*1024*1024,'retained kernel memory exceeds 256 MiB budget');
    report.passed=true;
  } catch(error) {report.passed=false;report.error=error.message.slice(0,500);process.exitCode=1;}
  finally { child.kill();await until(()=>child.exitCode!==null,10000).catch(()=>{});fs.writeFileSync(process.env.READMD_STRESS_REPORT,JSON.stringify(report,null,2)); }
  console.log(JSON.stringify(report,null,2));
})().catch(error=>{console.error(error.message);process.exitCode=1;});
