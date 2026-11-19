'use strict';
// Explicit Mac acceptance through the normal authenticated chunk installer.
// A short-lived owned kernel probes the exact pack and exports synthetic MD.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const {startKernel}=require('./v006-refactor-host.cjs');
const {api,settle,sha}=require('./node-runtime-fixture.cjs');

const source=`---
title: Offline Academic Publishing
author: Alex Example
lang: en-US
bibliography: Academic-Typst-Mac.bib
nocite: '@method'
---

# A reproducible research note

## Abstract

This synthetic document demonstrates local academic typesetting on Apple silicon. Its equation, table, citations, and footnote are exported directly from Markdown with the managed Typst engine.

## Method

The analysis follows a small, inspectable example [@study, pp. 12-14]. Three illustrative observations are summarized below.

| Condition | Observation | Weight |
| :--- | ---: | ---: |
| Baseline | 12 | 1 |
| Comparison | 15 | 1 |
| Follow-up | 18 | 1 |

The mean observation is given by

$$Q = \\frac{x_1 + x_2 + x_3}{3} = 15.$$

## Interpretation

All values are synthetic and serve as a formatting check.[^data] The additional methods reference is included with \\u0060nocite\\u0060 and has no artificial body citation.

[^data]: This example is an export acceptance fixture, not empirical research.
`;
const bibliography='@book{study,title={A synthetic research study},author={Example, Alex},year={2024},publisher={Example Academic}}\n@book{method,title={Extended methods reading},author={Sample, Morgan},year={2023},publisher={Example Academic}}\n';

async function main(){
  assert.equal(process.platform,'darwin');assert.equal(process.arch,'arm64');
  const [binary,assets,prepared,profile,workspace,evidence,mode='export']=process.argv.slice(2).map((v,i)=>i<6?path.resolve(v):v);
  assert(binary&&assets&&prepared&&profile&&workspace&&evidence,'BINARY ASSETS PREPARED PROFILE WORKSPACE EVIDENCE [install-only]');
  assert(['export','install-only','cjk-export'].includes(mode));
  for(const dir of [profile,workspace,evidence])fs.mkdirSync(dir,{recursive:true});
  const receipt=JSON.parse(fs.readFileSync(path.join(prepared,'prepared.json'))),pack=path.join(prepared,receipt.packFile),bytes=fs.readFileSync(pack);
  assert.equal(bytes.length,receipt.bytes);assert.equal(sha(bytes),receipt.sha256);
  const binarySha256=sha(fs.readFileSync(binary));
  let host;
  try{
    host=await startKernel(binary,profile,workspace,{assets,requestTimeoutMs:150000});
    const catalog=await api(host,'/api/capabilities/packs',{action:'catalog'});assert.equal(catalog.status,200,catalog.body.error_code);
    const entry=catalog.body.packs.find(p=>p.id==='typst');assert(entry&&entry.available);assert.equal(entry.sha256,receipt.sha256);
    async function install(id,payload,format){
    const existing=await api(host,'/api/capabilities/packs',{action:'status',id});
    if(existing.body.ready!==true){
    const start=await api(host,'/api/capabilities/packs',{action:'upload_start',id,format,total:payload.length,request_key:crypto.randomUUID()});
    assert.equal(start.status,200,JSON.stringify(start.body));
    let offset=start.body.received;
    while(offset<payload.length){
      const chunk=payload.subarray(offset,offset+start.body.chunk_bytes);
      const response=await host.api('/api/capabilities/upload/chunk?job='+encodeURIComponent(start.body.job)+'&offset='+offset,{method:'POST',headers:{'Content-Type':'application/octet-stream','X-ReadMD-Upload-Token':start.body.upload_token,'X-ReadMD-Chunk-SHA256':sha(chunk)},body:chunk});
      const ack=await response.json();assert.equal(response.status,200,JSON.stringify(ack));assert.equal(ack.received,offset+chunk.length);offset=ack.received;
    }
    const finish=await api(host,'/api/capabilities/packs',{action:'upload_finish',job:start.body.job,upload_token:start.body.upload_token});
    assert.equal(finish.status,200,JSON.stringify(finish.body));const final=await settle(host,finish.body);assert.equal(final.phase,'ready',JSON.stringify(final));
    }
    const ready=await api(host,'/api/capabilities/packs',{action:'status',id});assert.equal(ready.status,200);assert.equal(ready.body.installed,true);assert.equal(ready.body.ready,true);
    }
    await install('typst',bytes,'stream-v2');
    let fonts;
    if(mode==='cjk-export'){
      assert(path.isAbsolute(process.env.READMD_TEST_FONTS_PACK||''),'Explicit locked font package required');
      const fontBytes=fs.readFileSync(process.env.READMD_TEST_FONTS_PACK),fontEntry=catalog.body.packs.find(p=>p.id==='fonts');
      assert(fontEntry?.available);assert.equal(fontBytes.length,fontEntry.downloadBytes);assert.equal(sha(fontBytes),fontEntry.sha256);
      await install('fonts',fontBytes,'json-v1');fonts={sha256:fontEntry.sha256,ready:true};
    }
    let output;
    let sourceFile;
    if(mode!=='install-only'){
      let markdown=source.replaceAll('\\u0060','`');
      if(mode==='cjk-export')for(const [before,after] of [
        ['lang: en-US','lang: zh-CN'],['title: Offline Academic Publishing','title: 本地学术出版样张'],
        ['# A reproducible research note','# 本地科研排版样张'],['## Abstract','## 摘要'],['## Method','## 方法'],['## Interpretation','## 说明'],
        ['This synthetic document demonstrates local academic typesetting on Apple silicon. Its equation, table, citations, and footnote are exported directly from Markdown with the managed Typst engine.','本样张用于验证苹果芯片上的本地学术排版。正文、公式、表格、引文与脚注均从 Markdown 直接导出，使用经过安装探测的 Typst 引擎。'],
        ['The analysis follows a small, inspectable example [@study, pp. 12-14]. Three illustrative observations are summarized below.','下文引用一份合成研究示例 [@study, pp. 12-14]，三组演示数值列于表中。'],
        ['The mean observation is given by','平均值由下式计算：'],
        ['All values are synthetic and serve as a formatting check.[^data] The additional methods reference is included with `nocite` and has no artificial body citation.','全部数值仅用于排版验收。[^data] 方法类文献通过 `nocite` 加入延伸阅读，没有在正文中插入虚假引用。'],
        ['This example is an export acceptance fixture, not empirical research.','本样张属于导出验收材料，不代表真实研究结果。'],
      ])markdown=markdown.replace(before,after);
      const stem=mode==='cjk-export'?'Academic-Typst-Mac-CJK':'Academic-Typst-Mac';
      sourceFile=path.join(workspace,stem+'.md');
      fs.writeFileSync(sourceFile,markdown);fs.writeFileSync(path.join(workspace,'Academic-Typst-Mac.bib'),bibliography);
      output=path.join(workspace,stem+'.pdf');
      const exported=await api(host,'/api/export',{format:'pdf',content:markdown,baseDir:workspace,out_path:output,options:{pdf:{engine:'typst'},cover:{enabled:false},toc:{enabled:false},frontMatter:{mode:'none'}}});
      assert.equal(exported.status,200,JSON.stringify(exported.body));assert.equal(exported.body.ok,true,JSON.stringify(exported.body));
      const pdf=fs.readFileSync(output);assert.equal(pdf.subarray(0,5).toString(),'%PDF-');assert(pdf.length>1000);
    }
    const mediaPreserved={};
    if(process.env.READMD_TEST_PRESERVE_MEDIA==='1')for(const id of ['whisper','media-ffmpeg']){
      const status=await api(host,'/api/capabilities/packs',{action:'status',id});assert.equal(status.status,200);assert.equal(status.body.ready,true);mediaPreserved[id]=true;
    }
    const proof={schema:1,id:'typst',version:receipt.version,platform:receipt.platform,sha256:receipt.sha256,manifestSha256:receipt.manifestSha256,binarySha256,install:true,engineProbe:true,engineOperation:!!output,sourceSha256:sourceFile?sha(fs.readFileSync(sourceFile)):null,pdfSha256:output?sha(fs.readFileSync(output)):null,pdfBytes:output?fs.statSync(output).size:null,fonts,mediaPreserved,syntheticMaterials:true,published:false};
    fs.writeFileSync(path.join(evidence,'native-receipt.json'),JSON.stringify(proof,null,2)+'\n');console.log(JSON.stringify(proof));
  }finally{await host?.stop();}
}
main().catch(error=>{console.error(error.stack||error.message);process.exitCode=1;});
