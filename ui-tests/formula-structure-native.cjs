'use strict';
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),{execFileSync}=require('node:child_process');
const {chromium}=require('playwright'),{startNative}=require('./native-session.cjs');
const root=path.resolve(__dirname,'..'),work=path.join(root,'.cache/capability-upgrade/formula-native');fs.mkdirSync(work,{recursive:true});
const data=fs.mkdtempSync(path.join(work,'profile-')),docs=path.join(data,'documents');fs.mkdirSync(docs);
const file=path.join(docs,'Evidence.md'),original='# 证据与论证\n\n比较不同解释时，保留原始推导和每次修订。\n\n$\\frac{a}{b}$\n\n## 核对清单\n\n- [ ] 检查符号\n- [x] 保留来源\n';fs.writeFileSync(file,original);
const report={nativeCsp:true,syntheticOnly:true,checks:[],errors:[]};let native;
async function until(f){const end=Date.now()+30000;while(Date.now()<end){if(await f())return;await new Promise(r=>setTimeout(r,75));}throw Error('Native operation timed out');}
(async()=>{
 try{
  process.env.READMD_WORKSPACE=docs;native=await startNative(root,data,chromium,28854);const page=native.page;page.on('pageerror',e=>report.errors.push(e.message));
  await page.evaluate(async file=>{await i18n.setLanguage('en',false);await loadFile(file);await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');},file);
  await until(()=>page.locator('#preview-pane mjx-container').count());await page.locator('#preview-pane mjx-container').click();
  await until(()=>page.locator('.formula-inline-controls button').first().isEnabled());await page.locator('.formula-properties > summary').click();
  const p=page.locator('.formula-property-fields [data-math-container="fPr"]');await p.locator('summary').click();
  await p.locator('[data-math-property="fPr.type"] select').selectOption('skw');await until(()=>page.locator('.formula-inline-controls button').first().isEnabled());
  const cdp=await page.context().newCDPSession(page);for(const [width,height]of[[1024,680],[1160,820]]){await cdp.send('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false});assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));await page.screenshot({path:path.join(work,`formula-${width}.png`)});}await cdp.send('Emulation.clearDeviceMetricsOverride');await cdp.detach();
  assert.equal(fs.readFileSync(file,'utf8'),original);assert.equal(await page.evaluate(()=>saveEdit()),true);
  const changed=fs.readFileSync(file,'utf8');assert.ok(changed.includes(':math['));assert.ok(changed.includes('office.type(val: "skw")'));assert.ok(!changed.includes('<!--readmd:math:v1:'));assert.deepEqual(fs.readdirSync(docs),['Evidence.md']);
  await page.evaluate(async file=>{await exitEdit();await loadFile(file,{force:true,discardConfirmed:true});},file);await until(()=>page.locator('#content mjx-container').count());
  await until(()=>page.locator('#content [data-mml-node="mfrac"]').count());report.checks.push('real-file-atomic-save-reopen-rich-formula-no-sibling-files');
  const outputs={};for(const format of ['docx','pdf','epub']){const output=path.join(docs,'Evidence.'+format);const result=await page.evaluate(async({format,output,docs,content})=>{const r=await apiFetch('/api/export',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({format,content,baseDir:docs,suggestedName:'Evidence',options:{},out_path:output})});return {status:r.status,data:await r.json()};},{format,output,docs,content:changed});assert.equal(result.status,200);assert.equal(result.data.ok,true);assert.ok(fs.statSync(output).size>500);outputs[format]=output;}
  const nativeExports=execFileSync(process.env.READMD_QA_PYTHON||'python',['-c','import sys,zipfile,json,xml.etree.ElementTree as E,pymupdf; z=zipfile.ZipFile(sys.argv[1]); r=E.fromstring(z.read("word/document.xml")); ns={"m":"http://schemas.openxmlformats.org/officeDocument/2006/math"}; assert r.find(".//m:fPr/m:type",ns).attrib["{http://schemas.openxmlformats.org/officeDocument/2006/math}val"]=="skw"; p=pymupdf.open(sys.argv[2]); assert len(p)>0; assert len(p[0].get_drawings())>0; e=zipfile.ZipFile(sys.argv[3]); html="".join(e.read(n).decode() for n in e.namelist() if n.endswith(".xhtml")); assert "bevelled" in html; print(json.dumps({"editableOMML":True,"pdfPages":len(p),"pdfVectorPaths":len(p[0].get_drawings()),"epubMathML":True}))',outputs.docx,outputs.pdf,outputs.epub],{encoding:'utf8',windowsHide:true});report.exports=JSON.parse(nativeExports.trim());report.checks.push('native-DOCX-editable-OMML-PDF-vector-and-EPUB-MathML');
  const ps=[
   '$ErrorActionPreference="Stop"',
   '$word=New-Object -ComObject Word.Application',
   '$word.Visible=$false; $word.DisplayAlerts=0',
   'try {',
   ' $doc=$word.Documents.Open($env:READMD_WORD_INPUT,$false,$false)',
   ' if ($doc.OMaths.Count -ne 1) { throw "Expected one native equation" }',
   ' $range=$doc.OMaths.Item(1).Range; $range.Find.ClearFormatting()',
   ' if (-not $range.Find.Execute("a",$false,$false,$false,$false,$false,$true,0,$false,"q",1)) { throw "Word equation edit failed" }',
   ' $doc.SaveAs2($env:READMD_WORD_OUTPUT,16); $doc.Close(0)',
   ' $again=$word.Documents.Open($env:READMD_WORD_OUTPUT,$false,$true)',
   ' if ($again.OMaths.Count -ne 1 -or -not $again.OMaths.Item(1).Range.Text.Contains("q")) { throw "Reopened Word equation edit missing" }',
   ' $again.Close(0); Write-Output "WORD_ROUNDTRIP_OK"',
   '} finally { $word.Quit(); [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($word) }',
  ].join('\n')+'\n\n';
  const wordOut=path.join(docs,'Evidence-Word.docx');const word=execFileSync('powershell.exe',['-NoProfile','-NonInteractive','-EncodedCommand',Buffer.from(ps,'utf16le').toString('base64')],{encoding:'utf8',windowsHide:true,timeout:120000,env:{...process.env,READMD_WORD_INPUT:outputs.docx,READMD_WORD_OUTPUT:wordOut}});assert.ok(word.includes('WORD_ROUNDTRIP_OK'));
  report.checks.push('installed-Word-opens-native-OMML-edits-saves-and-reopens');
  const inspection=execFileSync(process.env.READMD_QA_PYTHON||'python',['-c','import sys,zipfile,json,xml.etree.ElementTree as E,pymupdf; z=zipfile.ZipFile(sys.argv[1]); r=E.fromstring(z.read("word/document.xml")); ns={"m":"http://schemas.openxmlformats.org/officeDocument/2006/math"}; assert r.find(".//m:fPr/m:type",ns).attrib["{http://schemas.openxmlformats.org/officeDocument/2006/math}val"]=="skw"; p=pymupdf.open(sys.argv[2]); assert len(p)>0; assert len(p[0].get_drawings())>0; print(json.dumps({"wordRichFraction":True,"pdfPages":len(p),"pdfVectorPaths":len(p[0].get_drawings())}))',wordOut,outputs.pdf],{encoding:'utf8',windowsHide:true});report.exports=JSON.parse(inspection.trim());
  assert.deepEqual(report.errors,[]);report.passed=true;
 }catch(error){report.passed=false;report.failure=error.message.slice(0,300);throw error;}
 finally{fs.writeFileSync(path.join(work,'report.json'),JSON.stringify(report,null,2));if(native){await native.browser.close().catch(()=>{});native.server.child.kill();}}
})().catch(e=>{console.error(e.message);process.exitCode=1;});
