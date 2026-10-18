'use strict';
// Windows production-CSP proof of sparse EPUB numbers, explicit zero, real
// reflow geometry and preset/language persistence. No save-dialog automation.
// --self-test/--list do not start ReadMD or claim native acceptance.
const fs=require('node:fs'),path=require('node:path'),net=require('node:net'),assert=require('node:assert/strict');
const {createHarness,fileHash,sha256,root,captureFailureScreenshot}=require('../tools/test/publication-batch3-harness.cjs');
const zipParts=require('./zip-parts.cjs');
const keys=['fontSize','lineHeight','marginV','marginH'];
const inherited={fontSize:10.5,lineHeight:1.55,marginV:5,marginH:8,language:'en-US'};
const explicit={fontSize:14.25,lineHeight:1.75,marginV:0,marginH:0,language:'en-GB'};
const plan={scope:'P12/P02 EPUB sparse numeric inheritance, explicit zero, clear, preset/format switching and restart; ReadMD reflow geometry only',stages:8,actualEpubFiles:6,automaticRetries:0,independentReadingSystems:'separate acceptance',systemSaveDialog:false};
const relative=file=>path.relative(root,file).replaceAll(path.sep,'/');
function sparse(epub){for(const key of keys)assert.equal(Object.hasOwn(epub||{},key),false,'Inherited EPUB '+key+' must stay absent');}
function near(actual,expected,tolerance=0.035){assert(Number.isFinite(actual));assert(Math.abs(actual-expected)<=tolerance,`${actual} differs from ${expected} by more than ${tolerance}`);}
function cssNumbers(css){const body=/\bbody\s*\{([^}]+)\}/u.exec(css)?.[1];assert(body,'Actual package has no body CSS');const get=expression=>{const match=expression.exec(body);assert(match,'Actual body CSS numeric declaration is missing');return match.slice(1).map(Number);};
  return{fontSize:get(/font-size:\s*([\d.]+)pt/u)[0],lineHeight:get(/line-height:\s*([\d.]+)/u)[0],marginV:get(/margin:\s*([\d.]+)%\s+([\d.]+)%/u)[0],marginH:get(/margin:\s*([\d.]+)%\s+([\d.]+)%/u)[1]};}
async function port(){const server=net.createServer();await new Promise((yes,no)=>{server.once('error',no);server.listen(0,'127.0.0.1',yes);});const number=server.address().port;await new Promise(yes=>server.close(yes));return number;}
function frozenSources(candidate){
  const manifest=path.resolve(root,candidate.sourceFreeze),snapshot=JSON.parse(fs.readFileSync(manifest,'utf8'));
  assert(Array.isArray(snapshot.sources)&&snapshot.sources.length>=19,'Full final-candidate source freeze is required');
  if(candidate.sourceFreezeSha256)assert.equal(fileHash(manifest),candidate.sourceFreezeSha256);
  const records=snapshot.sources.map(record=>{for(const name of [record.path,record.copy]){const absolute=path.resolve(root,name),rel=path.relative(root,absolute);assert(rel&&!rel.startsWith('..')&&!path.isAbsolute(rel));assert.equal(fileHash(absolute),record.sha256);}return record;});
  const digest=fileHash(manifest);return{manifest:relative(manifest),sha256:digest,sources:records,verify(){assert.equal(fileHash(manifest),digest);for(const record of records)for(const name of [record.path,record.copy])assert.equal(fileHash(path.resolve(root,name)),record.sha256);}};
}
async function run(){
  assert.equal(process.platform,'win32','Only Windows native acceptance is claimed');
  const candidateFile=process.env.READMD_P12_CANDIDATE_JSON;assert(candidateFile&&path.isAbsolute(candidateFile),'READMD_P12_CANDIDATE_JSON must identify root’s final candidate');
  const candidate=JSON.parse(fs.readFileSync(candidateFile,'utf8'));assert.equal(candidate.binarySha256,process.env.READMD_CANDIDATE_SHA256);assert.equal(candidate.bootSha256,process.env.READMD_CANDIDATE_BOOT_SHA256);
  const freeze=frozenSources(candidate),h=createHarness('epub-numeric-inherit-native',await port(),__filename);
  const paragraph='P12_BODY_BEGIN. A careful reading note keeps the question beside the evidence. It records what the author claims, how each example supports that claim, and which assumptions still need examination. The reader can return to this passage after checking the second chapter. A useful comparison needs enough connected prose to wrap across several real lines, so changes in type size and line spacing can be measured in the actual reading surface. Every sentence belongs to this synthetic book and remains the same across all six exported files. P12_BODY_END.';
  const source='\uFEFF'+('---\ntitle: Numeric inheritance book\nlanguage: en-US\n---\n# Opening\n\n'+paragraph+'\n\n## Evidence\n\nOriginal note marker.[^n]\n\n> A quotation remains unchanged.\n\n| Item | Value |\n| --- | ---: |\n| First | 12.5 |\n| Second | 20.0 |\n\n# Ending\n\nSECOND_CHAPTER remains unchanged. [Back to Opening](#opening).\n\n[^n]: P12_NOTE remains unchanged.\n').replaceAll('\n','\r\n');
  const input=path.join(h.docs,'Numeric-inheritance.md');fs.writeFileSync(input,source,{flag:'wx'});const inputs=h.snapshotInputs(),responses=path.join(h.data,'responses');fs.mkdirSync(responses);
  let page,requestId=0,primary,baselineSemantic,minimalRaw,zeroSaved,sparseSaved;
  Object.assign(h.report,{plan,scope:plan.scope,outputs:[],requests:[],previewProofs:[],screenshots:[],candidateFile:{path:relative(candidateFile),sha256:fileHash(candidateFile)},sourceFreeze:{manifest:freeze.manifest,sha256:freeze.sha256,sources:freeze.sources},initialInputs:inputs,systemSaveDialogVerified:false,independentReadingSystemsVerified:false});
  for(const file of ['ui-tests/epub-numeric-inherit-native.cjs','ui-tests/zip-parts.cjs','ui-tests/zip-fixtures.cjs','tools/test/export-epub-numeric-inherit.test.cjs','tools/epubcheck.lock.json'])h.report.qaDependencySha256[file]=fileHash(path.join(root,file));
  async function api(route,body,method='POST'){
    const result=await page.evaluate(async({route,body,method})=>{const response=await apiFetch(route,{method,...(method==='POST'?{headers:{'Content-Type':'application/json'},body:JSON.stringify(body)}:{}),signal:AbortSignal.timeout(120000)});return{status:response.status,data:await response.json()};},{route,body,method});
    const file=path.join(responses,String(++requestId).padStart(3,'0')+'.json');fs.writeFileSync(file,JSON.stringify({route,method,body,response:result},null,2)+'\n',{flag:'wx'});h.report.requests.push({path:relative(file),sha256:fileHash(file),route,status:result.status});return result;
  }
  function inventory(){
    const q=value=>"'"+value.replaceAll("'","''")+"'",ps=path.join(process.env.SystemRoot,'System32/WindowsPowerShell/v1.0/powershell.exe');
    const script="$ErrorActionPreference='Stop'\n$Scope="+q(h.nativeData)+"\n$Processes=@(Get-CimInstance Win32_Process|Where-Object {$_.CommandLine -and $_.CommandLine.IndexOf($Scope,[StringComparison]::OrdinalIgnoreCase)-ge 0 -and $_.Name -in @('readmd.exe','msedgewebview2.exe')}|ForEach-Object {@{pid=$_.ProcessId;name=$_.Name;created=$_.CreationDate.ToUniversalTime().ToString('o')}})\n@{processes=$Processes}|ConvertTo-Json -Depth 4 -Compress";
    return JSON.parse(h.command(ps,['-NoProfile','-NonInteractive','-Command',script]));
  }
  async function boot(){page=await h.start();await page.evaluate(()=>Promise.all([__readmdSessionReady,__readmdPreferencesReady]));assert.equal(h.report.servedBootSha256,candidate.bootSha256);
    const csp=fs.readFileSync(path.join(process.env.READMD_ASSETS_DIR||path.join(root,'assets'),'index.html'),'utf8').match(/<meta http-equiv="Content-Security-Policy" content="([^"]+)"/u)?.[1];assert(csp);assert.equal(await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content'),csp);}
  async function customize(){const details=page.locator('#export-box .export-customize');if(await details.count()&&!await details.evaluate(el=>el.open))await details.locator(':scope > summary').click();}
  async function select(name){
    // The complete picker belongs to “All presets”, alongside the preview.
    // “Customize layout” owns only the parameter sections in the main column.
    const list=page.locator('#export-box .export-preset-list'),picker=page.locator('#exp-preset');
    assert.equal(await list.count(),1);await list.locator(':scope > summary').waitFor({state:'visible'});
    if(!await list.evaluate(el=>el.open))await list.locator(':scope > summary').click();
    await picker.waitFor({state:'visible'});assert.equal(await picker.isEnabled(),true);
    await picker.selectOption(name);await h.until(()=>page.evaluate(name=>state.export.selectedPreset===name&&document.getElementById('exp-preset').value===name,name));
  }
  async function numericSection(){await customize();const section=page.locator('#export-opts .exp-sec').filter({has:page.locator('[data-k="epub.fontSize"]')});if(!await section.evaluate(el=>el.classList.contains('open')))await section.locator('.exp-sec-head').click();}
  async function metadataSection(){await customize();const section=page.locator('#export-opts .exp-sec').filter({has:page.locator('[data-k="epub.language"]')});if(!await section.evaluate(el=>el.classList.contains('open')))await section.locator('.exp-sec-head').click();}
  async function controls(){return page.evaluate(()=>({values:Object.fromEntries(['fontSize','lineHeight','marginV','marginH','language'].map(key=>[key,document.querySelector('#export-opts [data-k="epub.'+key+'"]').value])),options:collectExportOptions()}));}
  async function clear(){await numericSection();for(const key of keys)await page.locator('#export-opts [data-k="epub.'+key+'"]').fill('');await metadataSection();await page.locator('#export-opts [data-k="epub.language"]').selectOption('');sparse((await controls()).options.epub);}
  async function settled(full=false){
    await h.until(()=>page.evaluate(async full=>{
      const current=()=>{const preview=nativeExportPreview,signature=JSON.stringify({format:state.export.fmt,content:currentExportContent(),baseDir:state.dir||'',suggestedName:currentExportName(),options:collectExportOptions()});
        const frame=document.querySelector(full?'#export-preview-full-page iframe':'#export-preview-mini-content iframe'),doc=frame?.contentDocument;
        if(preview.signature!==signature||preview.taskId||!preview.data||preview.data.mode!=='epub-reflow'||frame?.srcdoc!==preview.data.html||doc?.readyState!=='complete'||doc.URL!=='about:srcdoc'||doc.fonts.status!=='loaded')return null;
        return{generation:preview.generation,data:preview.data,frame,doc};};
      const before=current();if(!before)return false;await new Promise(yes=>requestAnimationFrame(()=>requestAnimationFrame(yes)));const after=current();return Boolean(after&&after.generation===before.generation&&after.frame===before.frame&&after.doc===before.doc&&after.data===before.data);
    },full));
  }
  async function proof(name,wanted){
    await settled();await page.locator('#export-preview-card').click();await settled(true);
    const preview=await page.evaluate(()=>{const iframe=document.querySelector('#export-preview-full-page iframe'),doc=iframe.contentDocument,view=doc.defaultView,body=doc.body;
      const p=Array.from(doc.querySelectorAll('p')).find(el=>el.textContent.startsWith('P12_BODY_BEGIN.'));if(!p)throw Error('Actual preview first body paragraph is missing');
      const style=view.getComputedStyle(p),bodyStyle=view.getComputedStyle(body),rect=body.getBoundingClientRect(),range=doc.createRange();range.selectNodeContents(p);
      return{html:nativeExportPreview.data.html,signature:nativeExportPreview.signature,content:currentExportContent(),options:collectExportOptions(),language:doc.documentElement.lang,body:{fontSize:parseFloat(bodyStyle.fontSize),lineHeight:parseFloat(bodyStyle.lineHeight),marginTop:parseFloat(bodyStyle.marginTop),marginLeft:parseFloat(bodyStyle.marginLeft),marginRight:parseFloat(bodyStyle.marginRight),left:rect.left,width:rect.width},paragraph:{fontSize:parseFloat(style.fontSize),lineHeight:parseFloat(style.lineHeight),height:p.getBoundingClientRect().height,lineTops:[...new Set(Array.from(range.getClientRects()).filter(rect=>rect.width>0).map(rect=>rect.top))]},viewportWidth:doc.documentElement.clientWidth,horizontalOverflow:doc.documentElement.scrollWidth-doc.documentElement.clientWidth};});
    const px=wanted.fontSize*96/72,line=px*wanted.lineHeight;near(preview.body.fontSize,px);near(preview.paragraph.fontSize,px);near(preview.paragraph.lineHeight,line);assert(preview.paragraph.lineTops.length>=2,'Readable fixture must wrap into at least two actual lines');
    for(let i=1;i<preview.paragraph.lineTops.length;i++)near(preview.paragraph.lineTops[i]-preview.paragraph.lineTops[i-1],line);
    near(preview.paragraph.height,preview.paragraph.lineTops.length*line,0.15);near(preview.body.marginLeft,preview.viewportWidth*wanted.marginH/100,0.1);near(preview.body.marginRight,preview.viewportWidth*wanted.marginH/100,0.1);near(preview.body.marginTop,preview.viewportWidth*wanted.marginV/100,0.1);near(preview.body.left,preview.body.marginLeft,0.1);near(preview.body.width,preview.viewportWidth-preview.body.marginLeft-preview.body.marginRight,0.1);assert(preview.horizontalOverflow<=1);assert.equal(preview.language,wanted.language);
    const htmlFile=path.join(h.data,name+'.preview.actual.html');fs.writeFileSync(htmlFile,preview.html,{flag:'wx'});delete preview.html;preview.actualHtml={path:relative(htmlFile),sha256:fileHash(htmlFile)};
    const geometryFile=path.join(h.data,name+'.geometry.json');fs.writeFileSync(geometryFile,JSON.stringify(preview,null,2)+'\n',{flag:'wx'});h.report.previewProofs.push({name,path:relative(geometryFile),sha256:fileHash(geometryFile)});
    await page.locator('#export-preview-close').click();
    assert.equal(preview.content,source.slice(1));
    const file=path.join(h.outputs,name+'.epub'),response=await api('/api/export',{format:'epub',content:preview.content,baseDir:h.docs,out_path:file,options:preview.options,suggestedName:'Numeric inheritance book'});assert.equal(response.status,200);assert.equal(response.data.ok,true,JSON.stringify(response.data));
    const parts=zipParts(fs.readFileSync(file)),css=parts.get('OEBPS/style.css');assert(css);assert.deepEqual(cssNumbers(css),Object.fromEntries(keys.map(key=>[key,wanted[key]])));assert(fs.readFileSync(htmlFile,'utf8').includes(css.replaceAll('<','\\3c ')),'Visible preview must use this actual package stylesheet');
    const actual=await page.evaluate(parts=>{const parsed=Object.entries(parts).map(([name,xml])=>({name,doc:new DOMParser().parseFromString(xml,'application/xhtml+xml')}));if(parsed.some(item=>item.doc.querySelector('parsererror')))throw Error('Malformed actual EPUB XML');
      const opf=parsed.find(item=>item.name.endsWith('.opf')).doc,all=doc=>Array.from(doc.getElementsByTagName('*')),nav=parsed.find(item=>item.name.endsWith('/nav.xhtml')).doc,type=el=>el.getAttributeNS('http://www.idpf.org/2007/ops','type');
      const navigation=kind=>{const section=all(nav).find(el=>el.localName==='nav'&&type(el)===kind);if(!section)return null;return{heading:all(section).find(el=>el.localName===(kind==='toc'?'h1':'h2'))?.textContent.trim(),links:all(section).filter(el=>el.localName==='a').map(el=>({href:el.getAttribute('href'),label:el.textContent.trim(),...(kind==='landmarks'?{type:type(el)}:{})}))};};
      return{language:all(opf).filter(el=>el.namespaceURI==='http://purl.org/dc/elements/1.1/'&&el.localName==='language').map(el=>el.textContent),xhtml:parsed.filter(item=>item.name.endsWith('.xhtml')).map(item=>({name:item.name,lang:item.doc.documentElement.getAttribute('lang'),xmlLang:item.doc.documentElement.getAttributeNS('http://www.w3.org/XML/1998/namespace','lang')})),toc:navigation('toc'),landmarks:navigation('landmarks'),chapters:parsed.filter(item=>/\/chapter_\d+\.xhtml$/u.test(item.name)).map(item=>({name:item.name,text:item.doc.getElementsByTagNameNS('*','body')[0].textContent}))};},Object.fromEntries([...parts].filter(([name])=>name.endsWith('.xhtml')||name.endsWith('.opf'))));
    assert.deepEqual(actual.language,[wanted.language]);assert(actual.xhtml.length>=3);for(const doc of actual.xhtml){assert.equal(doc.lang,wanted.language);assert.equal(doc.xmlLang,wanted.language);}
    assert.deepEqual(actual.toc,{heading:'Numeric inheritance book',links:[{href:'chapter_1.xhtml#h-opening',label:'Opening'},{href:'chapter_1.xhtml#h-evidence',label:'Evidence'},{href:'chapter_2.xhtml#h-ending',label:'Ending'}]});
    assert.deepEqual(actual.landmarks,{heading:'Landmarks',links:[{href:'nav.xhtml#toc',label:'Contents',type:'toc'},{href:'chapter_1.xhtml#h-readmd-epub-bodymatter',label:'Start of content',type:'bodymatter'}]});
    assert(actual.chapters.some(chapter=>chapter.text.includes('P12_BODY_BEGIN')));assert(actual.chapters.some(chapter=>chapter.text.includes('SECOND_CHAPTER')));assert(actual.chapters.some(chapter=>chapter.text.includes('P12_NOTE')));
    const semantics=sha256(JSON.stringify(actual.chapters));if(baselineSemantic===undefined)baselineSemantic=semantics;else assert.equal(semantics,baselineSemantic,'Numeric changes altered actual chapter text');
    const inspection=path.join(h.data,name+'.package.json');fs.writeFileSync(inspection,JSON.stringify({...actual,numeric:cssNumbers(css),semanticSha256:semantics},null,2)+'\n',{flag:'wx'});
    h.report.outputs.push({name,path:relative(file),sha256:fileHash(file),bytes:fs.statSync(file).size,inspection:relative(inspection),inspectionSha256:fileHash(inspection),validated:true});h.unchanged(inputs);
  }
  async function save(name){await customize();await page.locator('#exp-save-preset').click();await page.locator('#exp-save-input').fill(name);await page.locator('#exp-save-ok').click();await h.until(()=>page.evaluate(name=>state.export.selectedPreset===name&&document.getElementById('exp-save-name').classList.contains('hidden'),name));const result=await api('/api/export/presets',undefined,'GET');assert.equal(result.data.last.preset,name);return result.data.custom[name];}
  await h.finish(async()=>{try{
    await h.step('fresh owned native profile uses final assets and default preset stays sparse',async()=>{
      assert.deepEqual(inventory().processes,[]);await boot();await page.setViewportSize({width:1160,height:820});await page.evaluate(async file=>{await i18n.setLanguage('en',false);await loadFile(file);await openExportModal();},input);await page.locator('#export-tab-epub').click();await select('minimal');
      const presets=await api('/api/export/presets',undefined,'GET');minimalRaw=presets.data.presets.minimal;sparse(minimalRaw.epub);assert.equal(minimalRaw.typography.size,10.5);assert.equal(minimalRaw.typography.lineHeight,1.55);
      const current=await controls();sparse(current.options.epub);for(const key of keys)assert.equal(current.values[key],'');assert.equal(current.values.language,'');await proof('01-default-inherits',inherited);
    });
    await h.step('legacy regional language and explicit zero create actual override geometry',async()=>{
      const preset=(await controls()).options;preset.epub={...(preset.epub||{}),fontSize:explicit.fontSize,lineHeight:explicit.lineHeight,marginV:0,marginH:0,language:explicit.language};const before=await api('/api/export/presets',undefined,'GET');assert.equal((await api('/api/export/presets',{custom:{...before.data.custom,'P12 legacy regional':preset}})).data.ok,true);
      await page.evaluate(async()=>{state.export.defaults=null;await loadExportPresets();renderExportPresetSelect();});await select('P12 legacy regional');const current=await controls();assert.equal(current.values.language,'en-GB');assert.equal(current.values.marginV,'0');assert.equal(current.values.fontSize,'14.25');await proof('02-explicit-zero',explicit);
      zeroSaved=await save('P12 saved zero and language');assert.deepEqual(zeroSaved.epub,preset.epub);
    });
    await h.step('clear restores source language and shared body typography in the real file',async()=>{await clear();await proof('03-cleared-inherits',inherited);});
    await h.step('preset and format switches do not leak old EPUB numbers and preserve sparse source preset',async()=>{
      await select('P12 saved zero and language');await page.locator('#export-tab-docx').click();await page.locator('#export-tab-epub').click();const zero=await controls();assert.equal(zero.options.epub.marginV,0);assert.equal(zero.values.language,'en-GB');await select('minimal');await proof('04-switched-minimal',inherited);
      const presets=await api('/api/export/presets',undefined,'GET');assert.deepEqual(presets.data.presets.minimal,minimalRaw);sparseSaved=await save('P12 saved sparse inherit');sparse(sparseSaved.epub);assert.equal(Object.hasOwn(sparseSaved.epub||{},'language'),false);
    });
    await h.step('invalid controls and API values fail without changing the old target or saved presets',async()=>{
      await numericSection();await page.locator('#export-opts [data-k="epub.marginH"]').fill('-1');assert.equal((await controls()).options.epub.marginH,-1);assert.equal(await page.evaluate(()=>runExport()),false);
      // Invalid numbers report inline/custom validity; Escape would close the
      // export modal itself, not dismiss a second error dialog.
      assert.equal(await page.locator('#export-modal').isVisible(),true);assert.equal(await page.locator('#export-opts [data-k="epub.marginH"]').getAttribute('aria-invalid'),'true');await page.locator('#export-opts [data-k="epub.marginH"]').fill('');
      const target=path.join(h.outputs,'protected-old.epub'),old=Buffer.from('KEEP ORIGINAL EPUB TARGET');fs.writeFileSync(target,old,{flag:'wx'});const before=(await api('/api/export/presets',undefined,'GET')).data;
      for(const epub of [{fontSize:0},{fontSize:'14'},{lineHeight:5.1},{lineHeight:false},{marginV:-1},{marginH:40.1},{marginH:'0'}])for(const route of ['/api/export','/api/export/preview','/api/export/presets']){
        const body=route.endsWith('/presets')?{custom:{'P12 invalid':{epub}}}:{format:'epub',content:source,baseDir:h.docs,out_path:target,options:{epub}};const result=await api(route,body);assert.equal(result.data.ok,false);assert.equal(result.data.error_code,'epub_invalid_options',JSON.stringify(result.data));assert.equal(result.data.key,undefined);assert.deepEqual(fs.readFileSync(target),old);
      }
      const after=(await api('/api/export/presets',undefined,'GET')).data;assert.deepEqual(after.custom,before.custom);assert.deepEqual(after.last,before.last);h.report.protectedOldTarget={path:relative(target),sha256:fileHash(target),preserved:true};
    });
    await h.step('real close restart and reselect preserve both saved preset states and regional language',async()=>{
      await page.evaluate(()=>{closeExportModal();ReadMDSession.flush();});h.report.beforeRestartProcesses=inventory();await h.stop();await h.until(()=>Promise.resolve(inventory().processes.length===0),15000);await boot();await page.setViewportSize({width:1160,height:820});await page.evaluate(async file=>{await loadFile(file);await openExportModal();},input);assert.equal(await page.locator('#exp-preset').inputValue(),'P12 saved sparse inherit');assert.equal(await page.locator('#export-tab-epub').getAttribute('aria-selected'),'true');
      const reopened=await api('/api/export/presets',undefined,'GET');assert.deepEqual(reopened.data.custom['P12 saved sparse inherit'],sparseSaved);assert.deepEqual(reopened.data.custom['P12 saved zero and language'],zeroSaved);sparse((await controls()).options.epub);await proof('05-reopened-sparse',inherited);await select('P12 saved zero and language');assert.equal((await controls()).values.language,'en-GB');await proof('06-reopened-zero',explicit);assert.equal(fs.readFileSync(input,'utf8'),source);
    });
    await h.step('visible blank and zero controls remain usable at two settled sizes and themes',async()=>{
      for(const [width,height,theme,preset]of [[1024,680,'light','P12 saved sparse inherit'],[1160,820,'dark','P12 saved zero and language']]){
        await page.locator('#export-close').click();await page.locator('#export-modal').waitFor({state:'hidden'});
        await page.setViewportSize({width,height});await h.until(()=>page.evaluate(()=>!document.documentElement.classList.contains('rm-theme-vt')));
        for(let i=0;i<4&&await page.locator('body').getAttribute('data-theme')!==theme;i++){await page.locator('#btn-theme').click();await h.until(()=>page.evaluate(()=>!document.documentElement.classList.contains('rm-theme-vt')));}assert.equal(await page.locator('body').getAttribute('data-theme'),theme);
        await page.locator('#btn-print').click();await page.locator('#export-modal').waitFor({state:'visible'});await select(preset);await numericSection();await settled();
        await h.until(()=>page.evaluate(()=>{const toast=document.getElementById('toast');return !toast||!toast.getClientRects().length||getComputedStyle(toast).visibility==='hidden';}));await page.locator('#export-opts [data-k="epub.fontSize"]').scrollIntoViewIfNeeded();assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth+1),false);
        const file=path.join(h.data,'numeric-'+width+'-'+theme+'.png');await page.screenshot({path:file});h.report.screenshots.push({path:relative(file),sha256:fileHash(file),width,height,theme,preset,visualReview:'pending actual inspection'});
      }
    });
    await h.step('pinned offline EPUBCheck validates every actual successful file with unchanged source and candidate',async()=>{
      assert.equal(h.report.outputs.length,plan.actualEpubFiles);const {checkEpub}=await import('../tools/check-epub.mjs');h.report.epubcheck=await checkEpub(h.report.outputs.map(output=>path.join(root,output.path)),{jar:process.env.READMD_EPUBCHECK_JAR,java:process.env.READMD_JAVA||'java',output:path.join(h.data,'epubcheck.json')});assert.equal(h.report.epubcheck.passed,true);assert.equal(h.report.epubcheck.files.length,plan.actualEpubFiles);h.unchanged(inputs);freeze.verify();
    });
  }catch(error){
    primary=error;
    // This work scope owns shutdown, so capture while the native page still
    // exists. The shared helper writes once and preserves earlier evidence.
    if(page)try{h.report.firstFailureScreenshot=await captureFailureScreenshot(page,root,h.data,undefined,'first-failure.png');}
    catch(captureError){h.report.firstFailureScreenshotError={name:captureError.name||'Error',code:captureError.code||null};}
    throw error;
  }finally{try{h.report.preStopProcesses=inventory();await h.stop();await h.until(()=>Promise.resolve(inventory().processes.length===0),15000);h.report.postStopProcesses=inventory();h.unchanged(inputs);freeze.verify();for(const [file,expected]of Object.entries(h.report.qaDependencySha256))assert.equal(fileHash(path.join(root,file)),expected);assert.equal(fileHash(candidateFile),h.report.candidateFile.sha256);h.report.hostStopped=true;h.report.sourceAndDependenciesUnchanged=true;}catch(error){h.report.postVerificationError=String(error);if(!primary)throw error;}}});
}
function selfTest(){assert.deepEqual(cssNumbers('body {font-size: 10.5pt;line-height: 1.55;margin: 0% 8%;}'),{fontSize:10.5,lineHeight:1.55,marginV:0,marginH:8});sparse({language:'en-US'});assert.throws(()=>sparse({marginV:0}));near(14.25*96/72,19);assert.equal(plan.actualEpubFiles,6);return{passed:true,checks:5,nativeExecuted:false};}
module.exports={run,plan,selfTest,cssNumbers};
if(require.main===module){if(process.argv.includes('--self-test'))console.log(JSON.stringify(selfTest(),null,2));else if(process.argv.includes('--list')||process.argv.includes('--help'))console.log(JSON.stringify({...plan,requiredEnvironment:['READMD_P12_CANDIDATE_JSON','READMD_BIN','READMD_CANDIDATE_SHA256','READMD_CANDIDATE_BOOT_SHA256','READMD_ASSETS_DIR','READMD_EPUBCHECK_JAR'],nativeExecuted:false},null,2));else run().catch(error=>{console.error(error.stack||error);process.exitCode=1;});}
