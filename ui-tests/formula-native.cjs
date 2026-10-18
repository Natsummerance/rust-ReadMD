'use strict';
// Real Windows WebView, packaged CSP and Rust export/save routes. Fixtures are
// owned copies; no user documents, credentials or external services are used.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {execFileSync}=require('node:child_process');
const {chromium}=require('playwright'),{startNative}=require('./native-session.cjs');
const root=path.resolve(__dirname,'..'),work=path.join(root,'.cache/math-ux');
const delay=ms=>new Promise(r=>setTimeout(r,ms));
async function until(fn,ms=30000){const end=Date.now()+ms;while(Date.now()<end){if(await fn())return;await delay(60);}throw Error('Native formula operation timed out');}
(async()=>{
 const data=fs.mkdtempSync(path.join(work,'native-')),docs=path.join(data,'documents'),out=path.join(data,'exports');
 fs.mkdirSync(docs);fs.mkdirSync(out);
 const source=path.join(docs,'概率与证据.md');
 const equation='\\sigma^{2}=\\frac{1}{n}\\sum_{i=1}^{n}(x_i-\\bar{x})^2';
 const text=`# 从观察到估计\n\n研究笔记 · 统计推断与证据判断\n\n## 一份估计，必须说明适用范围\n\n我们用有限样本认识总体。公式不仅需要正确计算，也应清楚说明符号、假设和适用条件。以下是供校订的方差草稿：\n\n$$\n${equation}\n$$\n\n> 校订目标：将总体方差记号改为样本方差，并使用 n − 1 的自由度。\n\n| 观察值 | 3 | 5 | 8 | 13 |\n| --- | --- | --- | --- | --- |\n| 偏离均值 | −4.25 | −2.25 | 0.75 | 5.75 |\n\n## 怎样解释修订\n\n- 平均值使用同一组样本估计。\n- 一个约束消耗一个自由度，因此分母应使用 n − 1。\n- 修改公式应能一次撤销，确认保存前保留原稿。\n\n## 审阅记录\n\n1. 区分总体参数与样本统计量。\n2. 核对每一个上下标、分子和分母。\n3. 在 Word 与 PDF 中复核公式和正文。\n\n我们的论证从观察开始，经由解释、比较和修订，最后形成可复核的结论。任何估计都应伴随不确定性说明，不把统计相关误认为因果。\n`;
 fs.writeFileSync(source,text);
 const report={nativeCsp:true,synthetic:true,scenarios:[],errors:[]};let native;
 async function record(name,fn){const start=Date.now(),details=await fn();report.scenarios.push({name,passed:true,ms:Date.now()-start,...details});console.log(name+' passed');}
 try{
  process.env.READMD_WORKSPACE=docs;
  native=await startNative(root,data,chromium,28881);const page=native.page;
  page.on('pageerror',e=>report.errors.push(e.message));
  await page.evaluate(async source=>{await i18n.setLanguage('zh-CN',false);await loadFile(source);state.theme='light';applySettings();},source);
  await record('native-offline-reader-and-exact-preview-source',async()=>{
   await until(()=>page.locator('#content mjx-container > svg').count());
   assert.equal(await page.locator('#content [data-mml-node="merror"]').count(),0);
   await page.evaluate(async()=>{await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');});
   await until(()=>page.locator('#preview-pane mjx-container').count());
   await page.locator('#preview-pane mjx-container').first().dblclick();
   await until(()=>page.locator('#formula-apply').isEnabled());
   assert.equal(await page.locator('#formula-source').inputValue(),'\n'+equation+'\n');
   return{offlineComponents:true,exactSource:true};
  });
  await record('native-symbol-pointer-edit-single-undo-and-source-protection',async()=>{
   await page.getByRole('button',{name:'编辑符号 \\sigma',exact:true}).click();
   await page.locator('#formula-token').fill('s');await page.locator('#formula-token-replace').click();
   await until(()=>page.locator('#formula-apply').isEnabled());
   await page.getByRole('button',{name:'编辑符号 n',exact:true}).first().click();
   await page.locator('#formula-token').fill('n-1');await page.locator('#formula-token').press('Enter');
   await until(()=>page.locator('#formula-apply').isEnabled());
   await page.screenshot({path:path.join(work,'formula-native-light.png')});
   assert.equal(fs.readFileSync(source,'utf8'),text);
   await page.locator('#formula-apply').click();
   const changed=await page.evaluate(()=>getEditContent());
   assert.ok(changed.includes('s^{2}=\\frac{1}{n-1}'));assert.equal(fs.readFileSync(source,'utf8'),text);
   await page.keyboard.press('Control+z');assert.equal(await page.evaluate(()=>getEditContent()),text);
   await page.keyboard.press('Control+y');assert.equal(await page.evaluate(()=>getEditContent()),changed);
   assert.equal(await page.evaluate(()=>saveEdit()),true);assert.equal(fs.readFileSync(source,'utf8'),changed);
   const names=fs.readdirSync(docs);assert.deepEqual(names,['概率与证据.md']);
   return{pointer:true,undo:true,redo:true,explicitSave:true,noSiblingBackups:true};
  });
  await record('native-primary-languages-compact-light-and-dark',async()=>{
   const cdp=await page.context().newCDPSession(page);
   try{for(const [width,height]of[[1024,680],[1160,820]]){
    await cdp.send('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false});
    for(const language of['zh-CN','zh-TW','en','ja']){
     await page.evaluate(async l=>{await i18n.setLanguage(l,false);const src=getEditContent(),at=src.indexOf('s^{2}');cmView.dispatch({selection:{anchor:at}});await ReadMDFormula.open();},language);
     await until(()=>page.locator('#formula-apply').isEnabled());
     const b=await page.locator('#formula-apply').boundingBox();assert.ok(b.y+b.height<=height+1);
     assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));
     assert.doesNotMatch(await page.locator('#formula-modal').innerText(),/formula\.(?:template|category|apply)/);
     await page.keyboard.press('Escape');
    }
   }
   await page.evaluate(async()=>{await i18n.setLanguage('zh-CN',false);state.theme='dark';applySettings();await ReadMDFormula.open();});
   await until(()=>page.locator('#formula-apply').isEnabled());
   await page.screenshot({path:path.join(work,'formula-native-dark.png')});await page.keyboard.press('Escape');
   }finally{await cdp.send('Emulation.clearDeviceMetricsOverride');await cdp.detach();}
   return{sizes:2,languages:4,themes:2};
  });
  await record('native-docx-pdf-epub-export-and-no-source-rewrite',async()=>{
   const before=fs.readFileSync(source,'utf8'),exports=[];
   for(const format of['docx','pdf','epub']){
    const output=path.join(out,'formula-proof.'+format);
    const result=await page.evaluate(async({format,output,docs})=>{
     const content=getEditContent()+'\n\n## 符号兼容样例\n\n$$ a\\cdot b \\quad \\therefore \\quad \\nexists x \\quad a\\leftrightharpoons b $$\n';
     const r=await apiFetch('/api/export',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({format,content,baseDir:docs,suggestedName:'公式校订',options:{},out_path:output,task_id:'formula-proof-'+format})});return{status:r.status,result:await r.json()};
    },{format,output,docs});
    assert.equal(result.status,200);assert.equal(result.result.ok,true,JSON.stringify(result.result));assert.ok(fs.statSync(output).size>1000);
    exports.push({format,bytes:fs.statSync(output).size,warnings:result.result.warns||[]});
   }
   const python=process.env.READMD_QA_PYTHON;
   if(!python)throw Error('READMD_QA_PYTHON must name the existing QA Python runtime');
   const inspectionOutput=execFileSync(python,['-c',[
    'import sys,zipfile,json,xml.etree.ElementTree as E,fitz',
    'from pathlib import Path',
    'p=Path(sys.argv[1]);ns={"m":"http://schemas.openxmlformats.org/officeDocument/2006/math"}',
    'z=zipfile.ZipFile(p/"formula-proof.docx");tree=E.fromstring(z.read("word/document.xml"))',
    'counts={k:len(tree.findall(".//m:"+k,ns)) for k in ["oMath","f","nary","acc","sSup"]}',
    'assert all(counts.values()),counts',
    'assert "n-1" in "".join(tree.itertext())',
    'assert all(g in "".join(tree.itertext()) for g in ["⋅","∴","∄","⇋"])',
    'pdf=fitz.open(p/"formula-proof.pdf");assert len(pdf)>0',
    'assert "从观察到估计" in "".join(pg.get_text() for pg in pdf)',
    'epub=zipfile.ZipFile(p/"formula-proof.epub");assert epub.read("mimetype")==b"application/epub+zip"',
    'html="".join(epub.read(n).decode("utf-8") for n in epub.namelist() if n.endswith((".html",".xhtml")))',
    'assert "math" in html and "n-1" in html',
    'print("READMD_QA_JSON="+json.dumps({"docxEditableMath":counts,"pdfPages":len(pdf),"epubMath":True}))',
   ].join('\n'),out],{encoding:'utf8',windowsHide:true});
   const inspection=JSON.parse(inspectionOutput.split(/\r?\n/).find(l=>l.startsWith('READMD_QA_JSON=')).slice('READMD_QA_JSON='.length));
   assert.equal(fs.readFileSync(source,'utf8'),before);return{exports,...inspection,sourceUnchanged:true};
  });
  if(process.env.READMD_RECORD_FORMULA==='1'){
   await page.evaluate(async source=>{state.theme='light';applySettings();await i18n.setLanguage('zh-CN',false);await exitEdit();await loadFile(source,{force:true,discardConfirmed:true});await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');},source);
   const recorder=require(path.join(root,'showcase/updates/v0.0.5/formula-workspace/record.cjs'));
   await recorder({page,docs,source,text,equation,until});
  }
  assert.deepEqual(report.errors,[]);report.passed=true;
 }catch(e){report.passed=false;report.failure=e.message;throw e;}
 finally{fs.writeFileSync(path.join(work,'native-formula-report.json'),JSON.stringify(report,null,2));if(native){await native.browser.close().catch(()=>{});native.server.child.kill();}}
})().catch(e=>{console.error(e.message);process.exitCode=1;});
