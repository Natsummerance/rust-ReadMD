'use strict';
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),net=require('node:net');
const checks=require('../tools/test/publication-navigation-checks.cjs'),repo=path.resolve(__dirname,'..');
const fixture=path.join(repo,'rust/readmd-kernel/src/fixtures/publication-navigation-book.md'),expectation=path.join(repo,'rust/readmd-kernel/src/fixtures/publication-navigation-book.expected.json');
const hash=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const plan={scope:'P13 actual EPUB navigation and ReadMD reflow reader only',stages:8,actualEpubFiles:5,automaticRetries:0,warmNativeData:true,independentDesktopReader:'pending separate; emitted handoff is not certification',productInvocations:0};
async function port(){const server=net.createServer();await new Promise((yes,no)=>{server.once('error',no);server.listen(0,'127.0.0.1',yes);});const p=server.address().port;await new Promise(yes=>server.close(yes));return p;}
async function run(){
  assert.equal(process.platform,'win32');const candidateFile=process.env.READMD_P13_CANDIDATE_JSON;
  assert(candidateFile&&path.isAbsolute(candidateFile));const candidate=JSON.parse(fs.readFileSync(candidateFile,'utf8'));
  assert.equal(candidate.binarySha256,process.env.READMD_CANDIDATE_SHA256);assert.equal(candidate.bootSha256,process.env.READMD_CANDIDATE_BOOT_SHA256);
  const {createHarness,stopAfterInventory}=require('../tools/test/publication-batch3-harness.cjs'),{freezeSources}=require('./publication-structure-native.cjs'),helpers=require('./publication-engines-native.cjs');
  const {inspectArtifact}=require('../tools/test/publication-batch3-checks.cjs'),python=process.env.READMD_QA_PYTHON;
  assert(python&&path.isAbsolute(python));assert.equal(hash(python),process.env.READMD_QA_PYTHON_SHA256);
  const zipParts=require('./zip-parts.cjs'),{waitCapabilityReady}=require('../tools/test/publication-capability-readiness.cjs');
  const profile=helpers.resolveEngineReuseProfile(process.env.READMD_ENGINE_REUSE_PROFILE);assert(profile,'Explicit fixed n2 managed data required');
  const freeze=freezeSources(candidate),h=createHarness('publication-navigation-native',await port(),__filename,profile),relative=file=>path.relative(repo,file).replaceAll(path.sep,'/');
  // The frozen package fixture is intentionally short. This dedicated runtime
  // variant adds readable chapter content so an actual return jump can scroll.
  const readingNotes="**Reading note 01.** A useful reading journal begins with a question rather than a conclusion. Write down what you hope to understand, where the account comes from, and which parts deserve a second look. This small habit gives the chapter a purpose and makes later navigation more than a search for a remembered phrase.\n\n**Reading note 02.** Read the opening paragraph slowly enough to notice its promises. An author may offer a description, an argument, a set of instructions, or a recollection. Naming that purpose helps you choose a fair standard of evidence. A practical guide should support an action; a personal account may instead explain a particular point of view.\n\n**Reading note 03.** Keep the source of a quotation beside the quotation itself. A sentence copied without its surrounding question can seem more certain than the original passage. When you return to a reference page, read the preceding paragraph as well as the highlighted line, and check whether the author was making a claim or describing an unresolved possibility.\n\n**Reading note 04.** Separate an observation from the explanation attached to it. Two readers can agree about what happened and still disagree about its cause. A short note that records both parts makes the disagreement easier to revisit. It also leaves room for new evidence without requiring you to discard every useful detail from an earlier account.\n\n**Reading note 05.** Examples help a reader test an abstract idea, but one example rarely settles the question. Look for a second case with a different setting, a different scale, or a different outcome. Ask which features the explanation can predict and which features it only describes after the event. Record the limits beside the apparent success.\n\n**Reading note 06.** When a chapter introduces a technical word, translate it into an ordinary sentence before using it in your own notes. If the translation loses an essential distinction, add that distinction explicitly. A clear definition should help you recognize a new instance of the idea, not merely repeat the wording of the original definition.\n\n**Reading note 07.** A diagram and a paragraph often answer different questions. The diagram can show an arrangement or a sequence at a glance; the paragraph can explain a condition, an exception, or a reason. Move between them deliberately. If their accounts seem inconsistent, note the specific label or sentence that requires clarification rather than guessing at an intended meaning.\n\n**Reading note 08.** Numbers need a frame of reference. A count may refer to people, events, documents, or repeated measurements of the same object. Before comparing two totals, check what each one includes and which period it covers. Preserve those boundaries in the journal so that a later reader can understand the comparison without reconstructing the whole chapter.\n\n**Reading note 09.** Treat a cross-reference as an invitation to inspect the surrounding discussion. The destination might supply a definition, an earlier example, or a qualification that changes how the current sentence should be read. After visiting it, return to the original passage and ask what you now understand differently. That return is part of the reading task.\n\n**Reading note 10.** A disagreement becomes more useful when its premises are visible. Describe the point on which two accounts diverge, then list the evidence each account would need to change. This approach keeps the journal focused on a question that can be examined. It also helps distinguish a factual dispute from a difference in preference or emphasis.\n\n**Reading note 11.** Reading in a second language can make familiar words appear more precise than they are. Keep the original term when a translation is uncertain, and explain the surrounding context in your own language. A later revision can improve the translation while preserving the evidence that led you to choose it in the first place.\n\n**Reading note 12.** A practical instruction should be read together with its starting conditions. A method that works for a small collection may need a different order of work for a large one. Write down the assumed tools, available time, and expected result. These details make it possible to adapt the method responsibly rather than following isolated steps mechanically.\n\n**Reading note 13.** After finishing a difficult section, pause before writing a summary. Try to state its central claim from memory, then compare that statement with the text. The difference reveals what you understood, what you overlooked, and what the author left ambiguous. A concise correction at this point is often more valuable than another page of copied notes.\n\n**Reading note 14.** Preserve a trail for unresolved questions. A question can be worth keeping even when the chapter provides no answer and the journal cannot yet suggest one. Give it a short explanation and a reference to the passage that raised it. When related evidence appears later, the connection will be available without pretending that the earlier uncertainty never existed.\n\n**Reading note 15.** Share a reading note by giving another reader enough context to disagree with it. Include the relevant passage, your interpretation, and the reason you found it persuasive. Avoid presenting a private abbreviation as a shared definition. A useful discussion starts when both readers can identify the same question and inspect the same source material.\n\n**Reading note 16.** The final review should leave the account easier to navigate. Remove accidental repetition, clarify references that no longer point to the intended passage, and preserve qualifications that still matter. Then follow one reference back to the opening discussion. A successful return shows that the notes form a connected reading record rather than a collection of disconnected excerpts.",insertion='[Return to the preface](#readmd-epub-front).';
  const base=fs.readFileSync(fixture,'utf8').replaceAll('\r','');assert.equal(base.split(insertion).length-1,1);
  const expanded=base.replace(insertion,readingNotes+'\n\n'+insertion),original='\uFEFF'+expanded.replaceAll('\n','\r\n'),expected=JSON.parse(fs.readFileSync(expectation,'utf8'));
  const notesFile=path.join(h.data,'navigation-scroll-reading-notes.md');fs.writeFileSync(notesFile,readingNotes+'\n',{flag:'wx'});
  h.report.runtimeSourceExpansion={scope:'Readable dedicated scroll fixture derived at runtime; frozen Markdown unchanged',fixtureSha256:hash(fixture),insertionBefore:insertion,paragraphs:16,path:relative(notesFile),sha256:hash(notesFile)};
  const input=path.join(h.docs,'Navigation-book.md');fs.writeFileSync(input,original,{flag:'wx'});fs.copyFileSync(expectation,path.join(h.docs,'Navigation-book.expected.json'));
  const initial=h.snapshotInputs(),current={...initial};let page,reuse,requestId=0,primary,known=[];
  Object.assign(h.report,{scope:plan.scope,plan,outputs:[],requests:[],screenshots:[],capabilityStatus:[],candidateFile:{path:relative(candidateFile),sha256:hash(candidateFile)},sourceFreeze:{path:freeze.file,sha256:freeze.sha256,sources:freeze.sources},initialInputHashes:initial,externalReadingSystem:'unverified',qaInterpreter:{executable:'<qa-python>',sha256:hash(python),productDependency:false},fixtureSha256:hash(fixture),expectedSha256:hash(expectation)});
  const dependencies=['ui-tests/publication-navigation-native.cjs','tools/test/publication-navigation-checks.cjs','ui-tests/zip-parts.cjs','ui-tests/zip-fixtures.cjs','ui-tests/publication-engines-native.cjs','ui-tests/publication-structure-native.cjs','tools/test/publication-capability-readiness.cjs'];
  for(const file of dependencies){h.report.qaDependencySha256[file]=hash(path.join(repo,file));fs.copyFileSync(path.join(repo,file),path.join(h.data,'qa-'+path.basename(file)));}
  const responses=path.join(h.data,'responses');fs.mkdirSync(responses);
  function inventory(){
    const q=s=>"'"+s.replaceAll("'","''")+"'",powershell=path.join(process.env.SystemRoot,'System32/WindowsPowerShell/v1.0/powershell.exe');
    const remembered=known.length?known.map(p=>'($_.ProcessId -eq '+p.pid+' -and $_.CreationDate.ToUniversalTime().ToString(\'o\') -eq '+q(p.created)+')').join(' -or '):'$false';
    const code=["$ErrorActionPreference='Stop'","$Scope="+q(profile),"$All=@(Get-CimInstance Win32_Process)","$Ids=@($All|Where-Object {($_.CommandLine -and $_.CommandLine.IndexOf($Scope,[StringComparison]::OrdinalIgnoreCase)-ge 0 -and $_.Name -in @('readmd.exe','msedgewebview2.exe')) -or ("+remembered+")}|ForEach-Object {$_.ProcessId})","for($i=0;$i -lt 16;$i++){$Next=@($All|Where-Object {$Ids -contains $_.ParentProcessId -and $Ids -notcontains $_.ProcessId}|ForEach-Object {$_.ProcessId});if(!$Next.Count){break};$Ids+=$Next}","@{recordedAt=[DateTime]::UtcNow.ToString('o');processes=@($All|Where-Object {$Ids -contains $_.ProcessId}|ForEach-Object {@{pid=$_.ProcessId;created=$_.CreationDate.ToUniversalTime().ToString('o');name=$_.Name}})}|ConvertTo-Json -Depth 4 -Compress"].join('\n');
    return JSON.parse(h.command(powershell,['-NoProfile','-NonInteractive','-Command',code]));
  }
  async function api(route,body,method='POST',budget=120000){
    const file=path.join(responses,String(++requestId).padStart(3,'0')+'.json'),start=Date.now();let response;
    try{response=await page.evaluate(async({route,body,method,budget})=>{const r=await apiFetch(route,{method,...(method==='POST'?{headers:{'Content-Type':'application/json'},body:JSON.stringify(body)}:{}),signal:AbortSignal.timeout(budget)});return {status:r.status,data:await r.json()};},{route,body,method,budget});}
    catch(error){const elapsedMs=Date.now()-start;fs.writeFileSync(file,JSON.stringify({route,body,method,budgetMs:budget,elapsedMs,error:String(error)})+'\n',{flag:'wx'});h.report.requests.push({path:relative(file),sha256:hash(file),elapsedMs,status:null});throw error;}
    const record={route,body,method,budgetMs:budget,elapsedMs:Date.now()-start,response};fs.writeFileSync(file,JSON.stringify(record,null,2)+'\n',{flag:'wx'});h.report.requests.push({path:relative(file),sha256:hash(file),elapsedMs:record.elapsedMs,status:response.status});return response;
  }
  async function boot(){page=await h.start();await page.evaluate(()=>Promise.all([__readmdSessionReady,__readmdPreferencesReady]));assert.equal(h.report.servedBootSha256,candidate.bootSha256);
    const csp=fs.readFileSync(path.join(process.env.READMD_ASSETS_DIR,'index.html'),'utf8').match(/<meta http-equiv="Content-Security-Policy" content="([^"]+)"/u)?.[1];assert(csp);assert.equal(await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content'),csp);h.report.productionCspUnchanged=true;}
  async function settledReader(){
    await h.until(()=>page.evaluate(()=>{
      const visible=element=>Boolean(element&&!element.hidden&&getComputedStyle(element).display!=='none'&&getComputedStyle(element).visibility!=='hidden'&&element.getClientRects().length);
      return !visible(document.getElementById('toast'))&&!visible(document.getElementById('busy'))&&(state.busyCount||0)===0&&
        !document.querySelector('#render-progress')&&!Array.from(document.querySelectorAll('[aria-busy="true"],#content .readmd-loading,#content .diagram-loading')).some(visible);
    }));
    await page.evaluate(async()=>{await document.fonts.ready;await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));});
  }
  async function settledEpubPreview(scope){
    // Observe from the host: a srcdoc document can expose anchors before its
    // load commits. Never await fonts.ready inside that replaceable context.
    await h.until(()=>page.evaluate(async scope=>{
      const current=()=>{
        const preview=nativeExportPreview,mini=document.getElementById('export-preview-mini-content');
        const modal=document.getElementById('export-preview-modal'),fullOpen=!modal.classList.contains('hidden');
        const wrapper=modal.querySelector('.export-preview-paper-wrapper');
        const signature=JSON.stringify({format:state.export.fmt,content:currentExportContent(),baseDir:state.dir||'',suggestedName:currentExportName(),options:collectExportOptions()});
        if(state.export.fmt!=='epub'||state.export.selectedPreset!=='__custom__'||document.getElementById('exp-preset').value!=='__custom__'||document.querySelector('#export-opts select[data-k="epub.language"]')?.value!=='en'||preview.signature!==signature)return null;
        if(!preview.data){
          if(mini.getAttribute('aria-busy')!=='true'||mini.querySelector('iframe')||(fullOpen&&(wrapper.getAttribute('aria-busy')!=='true'||wrapper.querySelector('iframe'))))throw Error('The current EPUB request must visibly replace old preview content with loading');
          return null;
        }
        if(preview.taskId||preview.pagePending||preview.data.format!=='epub'||preview.data.mode!=='epub-reflow'||mini.getAttribute('aria-busy')!=='false')return null;
        if(scope==='full'&&(!fullOpen||wrapper.getAttribute('aria-busy')!=='false'))return null;
        const frame=document.querySelector(scope==='full'?'#export-preview-full-page iframe':'#export-preview-mini-content iframe'),doc=frame?.contentDocument;
        if(!frame?.isConnected||frame.srcdoc!==preview.data.html||doc?.URL!=='about:srcdoc'||doc.readyState!=='complete'||doc.fonts.status!=='loaded'||doc.querySelectorAll('[role="doc-pagebreak"]').length!==4)return null;
        return {generation:preview.generation,data:preview.data,frame,doc};
      };
      const before=current();if(!before)return false;
      await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
      const after=current();return Boolean(after&&after.generation===before.generation&&after.data===before.data&&after.frame===before.frame&&after.doc===before.doc);
    },scope));
  }
  async function summary(file){
    const parts=Object.fromEntries([...zipParts(fs.readFileSync(file))].filter(([name])=>name.endsWith('.xhtml')||name.endsWith('.opf')));
    return page.evaluate(parts=>{
      const parser=new DOMParser(),documents={},parseErrors=[];for(const[name,xml]of Object.entries(parts)){const doc=parser.parseFromString(xml,'application/xhtml+xml');if(doc.getElementsByTagName('parsererror').length)parseErrors.push(name);documents[name.replace(/^OEBPS\//u,'')]=doc;}
      if(!documents['nav.xhtml']||!documents['content.opf'])parseErrors.push('Missing required package/nav document');if(parseErrors.length)return {parseErrors};
      const all=doc=>Array.from(doc.getElementsByTagName('*')),type=e=>e.getAttributeNS('http://www.idpf.org/2007/ops','type')||e.getAttribute('epub:type'),nav=documents['nav.xhtml'];
      const navs=all(nav).filter(e=>e.localName==='nav'),getNav=kind=>navs.find(e=>type(e)===kind),links=node=>node?all(node).filter(e=>e.localName==='a').map(e=>({href:e.getAttribute('href'),label:e.textContent,...(type(e)?{type:type(e)}:{})})):[];
      const ids=[],anchors=[],internalLinks=[];let bodyText='',chapterCount=0;
      for(const[file,doc]of Object.entries(documents)){if(!file.endsWith('.xhtml'))continue;const nodes=all(doc);
        if(/^chapter_\d+\.xhtml$/u.test(file)){chapterCount++;bodyText+=doc.getElementsByTagNameNS('*','body')[0]?.textContent||'';}
        for(const e of nodes){if(e.hasAttribute('id'))ids.push({file,id:e.getAttribute('id')});if(e.hasAttribute('data-readmd-epub-anchor'))anchors.push({file,id:e.getAttribute('id'),owner:e.getAttribute('data-readmd-epub-anchor'),type:type(e),role:e.getAttribute('role'),label:e.getAttribute('aria-label'),text:e.textContent});
          const href=e.localName==='a'?e.getAttribute('href'):null;if(href&&(href.startsWith('#')||/^[^:/?#]+\.xhtml(?:#.*)?$/u.test(href)))internalLinks.push({file,href});}
      }
      const opf=documents['content.opf'],nodes=all(opf),metadata=nodes.find(e=>e.localName==='metadata'),meta=all(metadata),dc=name=>meta.filter(e=>e.namespaceURI==='http://purl.org/dc/elements/1.1/'&&e.localName===name);
      const source=dc('source').find(e=>e.getAttribute('id')==='readmd-pagination-source'),relations=meta.filter(e=>e.getAttribute('property')==='source-of'&&e.getAttribute('refines')==='#readmd-pagination-source'&&e.textContent==='pagination');
      const custom=meta.find(e=>e.getAttribute('property')==='readmd:metadata');
      return {parseErrors,chapterCount,pages:links(getNav('page-list')),landmarks:links(getNav('landmarks')),tocCount:navs.filter(e=>type(e)==='toc').length,pageListCount:navs.filter(e=>type(e)==='page-list').length,landmarksCount:navs.filter(e=>type(e)==='landmarks').length,
        sourceOfPagination:source?.textContent||null,sourceOfRelations:relations.length,title:dc('title')[0]?.textContent,creators:dc('creator').map(e=>e.textContent),rights:dc('rights')[0]?.textContent,customMetadata:custom?JSON.parse(custom.textContent):null,
        ids,anchors,internalLinks,bodyText,files:Object.keys(documents),sourceDirectiveLeaked:Object.values(parts).some(xml=>xml.includes('<!-- readmd:epub'))};
    },parts);
  }
  async function exported(name,source,options,wanted=expected,overrides={}){
    const file=path.join(h.outputs,name+'.epub'),record={path:relative(file),validated:false};h.report.outputs.push(record);
    const response=await api('/api/export',{format:'epub',content:source,baseDir:h.docs,out_path:file,suggestedName:'Navigation book',options});assert.equal(response.status,200);assert.equal(response.data.ok,true,JSON.stringify(response.data));
    record.sha256=hash(file);record.bytes=fs.statSync(file).size;
    const command=h.command;h.command=(_executable,args,...rest)=>command(python,args,...rest);try{inspectArtifact(h,file);}finally{h.command=command;}
    record.artifactInspection=relative(path.join(h.data,path.basename(file)+'.inspection.json'));record.artifactInspectionSha256=hash(path.join(repo,record.artifactInspection));
    const actual=await summary(file),inspection=path.join(h.data,name+'.navigation.json');
    fs.writeFileSync(inspection,JSON.stringify(actual,null,2)+'\n',{flag:'wx'});record.inspection=relative(inspection);record.inspectionSha256=hash(inspection);record.proof=checks.checkNavigation(actual,wanted,overrides);record.validated=true;h.unchanged(current);return file;
  }
  await h.finish(async()=>{try{
    await h.step('fixed owned warm profile with immutable proofs, fresh assets and real native CSP',async()=>{reuse=helpers.prepareEngineProfileReuse(h,profile,path.resolve(process.env.READMD_ASSETS_DIR||path.join(repo,'assets')));h.report.preStartProcesses=inventory();assert.deepEqual(h.report.preStartProcesses.processes,[]);await boot();
      await waitCapabilityReady({id:'fonts',budget:120000,request:remaining=>api('/api/capabilities/packs',{action:'status',id:'fonts'},'POST',remaining),observe:value=>h.report.capabilityStatus.push(value)});h.report.engineProfileReuse.actualProductReadinessReexecuted=true;h.report.engineProfileReuse.currentRunReadinessCapabilities=['fonts'];});
    let originalBook;
    await h.step('actual auto, explicit cover, disabled and single-spine EPUB navigation packages',async()=>{
      originalBook=await exported('Navigation-auto',original,expected.options);
      await exported('Navigation-explicit-cover',original,{...expected.options,cover:{enabled:true},epub:{splitLevel:'h1',navigation:{landmarks:[{type:'cover',target:'@cover'},{type:'bodymatter',target:'main'}]}}},expected,{landmarks:[{href:'cover.xhtml',label:'Cover',type:'cover'},{href:'chapter_2.xhtml#h-readmd-epub-main',label:'Start of content',type:'bodymatter'}]});
      await exported('Navigation-disabled',original,{...expected.options,epub:{splitLevel:'h1',navigation:{landmarks:false,pageList:false}}},expected,{pages:false,landmarks:[]});
      const single={...expected,pages:[{href:'chapter_1.xhtml#h-readmd-epub-front',label:'i'},{href:'chapter_1.xhtml#h-readmd-epub-main',label:'1'},{href:'chapter_1.xhtml#h-readmd-epub-middle',label:'2'},{href:'chapter_1.xhtml#h-readmd-epub-last',label:'3'}],landmarks:[{href:'nav.xhtml#toc',label:'Contents',type:'toc'},{href:'chapter_1.xhtml#h-readmd-epub-front',label:'Preface',type:'preface'},{href:'chapter_1.xhtml#h-readmd-epub-main',label:'Start of content',type:'bodymatter'},{href:'chapter_1.xhtml#h-readmd-epub-last',label:'Index',type:'index'}]};
      await exported('Navigation-single',original,{...expected.options,epub:{splitLevel:'none'}},single,{chapterCount:1});
    });
    await h.step('real native EPUB reflow preview retains semantic page anchors and cross-chapter jump',async()=>{
      await page.setViewportSize({width:1024,height:680});
      await page.evaluate(async file=>{await i18n.setLanguage('en',false);await loadFile(file);},input);assert.equal(await page.evaluate(()=>state.original),original.slice(1));
      await page.evaluate(()=>openExportModal());await page.locator('#export-tab-epub').click();
      const picker=page.locator('#exp-preset'),details=picker.locator('xpath=ancestor::details[1]');if(!await details.evaluate(element=>element.open))await details.locator(':scope > summary').click();await picker.selectOption('minimal');
      // Publication language is a separate output control. This UI offers en,
      // not the fixture's regional en-US tag; keep the English oracle fixed.
      const language=page.locator('#export-opts select[data-k="epub.language"]');assert.equal(await language.count(),1);
      const customize=page.locator('#export-modal details.export-customize');assert.equal(await customize.count(),1);const wasOpen=await customize.evaluate(element=>element.open);
      if(!wasOpen)await customize.locator(':scope > summary').click();
      const section=language.locator('xpath=ancestor::div[contains(concat(" ",normalize-space(@class)," ")," exp-sec ")][1]');assert.equal(await section.count(),1);
      if(!await section.evaluate(element=>element.classList.contains('open')))await section.locator(':scope > button.exp-sec-head').click();
      assert.equal(await language.isVisible(),true);await language.selectOption('en');
      if(!wasOpen)await customize.locator(':scope > summary').click();
      assert.equal(await language.inputValue(),'en');assert.equal(await picker.inputValue(),'__custom__');
      h.report.limitations.push('Historical candidate evidence records an untouched EPUB selector overriding source en-US with zh-CN. This scoped run explicitly selects English (en); current-candidate default language inheritance requires separate validation.');
      await settledEpubPreview('mini');
      await page.locator('#export-preview-card').click();await settledEpubPreview('full');const reader=page.frameLocator('#export-preview-full-page iframe');assert.equal(await reader.locator('[role="doc-pagebreak"]').count(),4);assert.equal(await reader.locator('html').getAttribute('lang'),'en');
      const targetId='epub-chapter_1-h-readmd-epub-front',href='#'+targetId;
      const target=reader.locator('[id="'+targetId+'"]');assert.equal(await target.count(),1);
      assert.equal(await target.getAttribute('role'),'doc-pagebreak');assert.equal(await target.getAttribute('aria-label'),'i');assert.equal(await target.getAttribute('data-readmd-epub-anchor'),'source');
      const htmlFile=path.join(h.data,'navigation-preview.actual.html');fs.writeFileSync(htmlFile,await reader.locator('html').evaluate(element=>element.outerHTML),{flag:'wx'});
      h.report.previewJump={actualHtml:{path:relative(htmlFile),sha256:hash(htmlFile)},passed:false};
      // A body return link, a hidden preface landmark and a hidden reference-
      // page link may all address this one valid target. Audit each meaning;
      // never choose the first match from a global href query.
      const bindings=await reader.locator('a[href="'+href+'"]').evaluateAll(links=>links.map(link=>{
        const nav=link.closest('nav'),type=element=>element?.getAttributeNS('http://www.idpf.org/2007/ops','type')||element?.getAttribute('epub:type')||null;
        return {article:link.closest('article')?.id||null,navType:type(nav),linkType:type(link),label:link.textContent.trim(),href:link.getAttribute('href')};
      }));
      assert.deepEqual(bindings.filter(link=>link.navType==='landmarks'),[{article:'epub-nav-',navType:'landmarks',linkType:'preface',label:'Preface',href}]);
      assert.deepEqual(bindings.filter(link=>link.navType==='page-list'),[{article:'epub-nav-',navType:'page-list',linkType:null,label:'i',href}]);
      assert.deepEqual(bindings.filter(link=>link.navType===null),[{article:'epub-chapter_2-',navType:null,linkType:null,label:'Return to the preface',href}]);
      const jump=reader.locator('article[id="epub-chapter_2-"]').getByRole('link',{name:'Return to the preface',exact:true});assert.equal(await jump.count(),1);assert.equal(await jump.getAttribute('href'),href);
      function coordinates(element){
        const doc=element.ownerDocument,view=doc.defaultView,rect=element.getBoundingClientRect(),scrollY=view.scrollY,scrollHeight=doc.scrollingElement.scrollHeight;
        const maxScrollY=Math.max(0,scrollHeight-view.innerHeight),documentTop=rect.top+scrollY;
        return {targetId:element.id,targetArticle:element.closest('article')?.id||null,targetTop:rect.top,targetDocumentTop:documentTop,scrollY,maxScrollY,viewportHeight:view.innerHeight,documentUrl:doc.URL,
          expectedScrollY:Math.max(0,Math.min(maxScrollY,documentTop)-8)};
      }
      // Set an ordinary scroll position where the real source link is fully
      // visible and as far as possible from the computed destination. No
      // content, styles, anchors or product handlers are changed for this test.
      const geometry=await target.evaluate(coordinates),source=await jump.evaluate(link=>{const view=link.ownerDocument.defaultView,rect=link.getBoundingClientRect();return {top:rect.top+view.scrollY,bottom:rect.bottom+view.scrollY};});
      const minimum=Math.max(0,Math.ceil(source.bottom-geometry.viewportHeight+1)),maximum=Math.min(geometry.maxScrollY,Math.floor(source.top-1));assert(maximum>=minimum,'The original return link must fit in the actual preview viewport');
      const start=Math.abs(minimum-geometry.expectedScrollY)>=Math.abs(maximum-geometry.expectedScrollY)?minimum:maximum;
      await target.evaluate((element,top)=>element.ownerDocument.defaultView.scrollTo({top,behavior:'instant'}),start);
      const before=await target.evaluate(coordinates),tolerancePx=1.5;assert(Math.abs(before.scrollY-before.expectedScrollY)>2,'The dedicated readable fixture must exercise a real scroll jump');
      assert.equal(before.targetArticle,'epub-chapter_1-');assert.equal(before.documentUrl,'about:srcdoc');
      await jump.click();let after;
      await h.until(async()=>{after=await target.evaluate(coordinates);return Math.abs(after.scrollY-before.expectedScrollY)<=tolerancePx&&after.targetTop>=-tolerancePx&&after.targetTop<after.viewportHeight;});
      assert.equal(after.documentUrl,before.documentUrl);assert.equal(after.targetId,targetId);assert.equal(after.targetArticle,'epub-chapter_1-');
      assert(Math.abs(after.scrollY-before.scrollY)>2);assert(Math.abs(after.targetDocumentTop-before.targetDocumentTop)<=tolerancePx);assert(Math.abs(after.targetTop-(before.targetDocumentTop-before.expectedScrollY))<=tolerancePx);
      const preview=await api('/api/export/preview',{format:'epub',content:original,baseDir:h.docs,options:expected.options});assert.equal(preview.data.mode,'epub-reflow');
      const documents=['nav.xhtml',...Array.from({length:expected.chapters},(_,index)=>'chapter_'+(index+1)+'.xhtml')];assert.deepEqual(preview.data.documents,documents);assert(!preview.data.html.includes('<!-- readmd:epub'));
      const proofFile=path.join(h.data,'navigation-preview-jump.json'),proof={passed:true,sourceLanguage:'en-US',explicitOutputLanguage:'en',defaultLanguageInheritanceVerified:false,sourceArticle:'epub-chapter_2-',sourceLabel:'Return to the preface',href,targetId,bindings,documents,before,after,tolerancePx,actualHtml:h.report.previewJump.actualHtml};
      fs.writeFileSync(proofFile,JSON.stringify(proof,null,2)+'\n',{flag:'wx'});Object.assign(h.report.previewJump,{passed:true,path:relative(proofFile),sha256:hash(proofFile),sourceArticle:proof.sourceArticle,targetId,realScrollChanged:true});
      await page.locator('#export-preview-close').click();await page.evaluate(()=>closeExportModal());h.unchanged(current);
    });
    await h.step('native reader reference sizes retain text and unclipped source with settled themes',async()=>{
      for(const[width,height,theme]of [[1024,680,'light'],[1160,820,'dark']]){
        await page.setViewportSize({width,height});await h.until(()=>page.evaluate(()=>!document.documentElement.classList.contains('rm-theme-vt')));
        for(let i=0;i<4&&await page.locator('body').getAttribute('data-theme')!==theme;i++){const before=await page.locator('#btn-theme').getAttribute('data-theme-icon');await page.locator('#btn-theme').click();await h.until(()=>page.evaluate(before=>document.getElementById('btn-theme').dataset.themeIcon!==before&&!document.documentElement.classList.contains('rm-theme-vt'),before));}
        assert.equal(await page.locator('body').getAttribute('data-theme'),theme);assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth+1),false);
        assert.equal(await page.evaluate(()=>state.original),original.slice(1));await settledReader();const png=path.join(h.data,'navigation-'+width+'-'+theme+'.png');await page.screenshot({path:png});h.report.screenshots.push({path:relative(png),sha256:hash(png),width,height,theme,settledAfterRealToastAndLoading:true,visualReview:'pending human inspection'});
      }
    });
    await h.step('invalid navigation and unsupported pagination vocabulary preserve old targets and preview cache',async()=>{
      const cases=[{content:original.replace('"id":"main"','"id":"front"'),options:expected.options,code:'publication_navigation_reference'},{content:original.replace('readmd:epub v1','readmd:epub v2'),options:expected.options,code:'publication_navigation_invalid'},{content:original,options:{epub:{navigation:{landmarks:[{type:'index',target:'missing'}]}}},code:'publication_navigation_reference'},{content:original,options:{epub:{navigation:{pageBreakSource:'unsupported'}}},code:'publication_navigation_invalid'}];
      for(const[i,item]of cases.entries()){const target=path.join(h.outputs,'protected-'+i+'.epub'),before=Buffer.from('KEEP ORIGINAL EPUB '+i);fs.writeFileSync(target,before,{flag:'wx'});
        for(const route of ['/api/export','/api/export/preview']){const r=await api(route,{format:'epub',content:item.content,baseDir:h.docs,out_path:target,options:item.options});assert.equal(r.data.ok,false);assert.equal(r.data.error_code,item.code,JSON.stringify(r.data));assert.equal(r.data.key,undefined);assert.deepEqual(fs.readFileSync(target),before);}
      }h.unchanged(current);
    });
    await h.step('actual source edit undo save restart and sparse navigation preset retain BOM CRLF metadata',async()=>{
      await page.evaluate(async file=>{await exitEdit();await loadFile(file);await toggleEdit();},input);
      await page.evaluate(()=>{const at=cmView.state.doc.toString().indexOf('MAIN_BEGIN');if(at<0)throw Error('Original edit token missing');cmView.dispatch({selection:{anchor:at,head:at+10}});cmView.focus();});await page.keyboard.type('MAIN_EDITED');
      const edited=original.replace('MAIN_BEGIN','MAIN_EDITED');assert.equal(await page.evaluate(()=>getEditContent()),edited.slice(1));await page.keyboard.press('Control+z');assert.equal(await page.evaluate(()=>getEditContent()),original.slice(1));await page.keyboard.press('Control+y');
      assert.equal(fs.readFileSync(input,'utf8'),original);await page.locator('#edit-save').click();await h.until(()=>Promise.resolve(fs.readFileSync(input,'utf8')===edited));current['Navigation-book.md']=hash(input);
      await h.until(()=>page.evaluate(saved=>!ReadMDTask.isRunning('document-save')&&!getActiveTab()?.isDirty&&state.original===saved&&getEditContent()===saved,edited.slice(1)));
      const presets=await api('/api/export/presets',undefined,'GET'),settings={...expected.options,epub:{splitLevel:'h1',navigation:{landmarks:'auto',pageList:'auto'}}};const custom={...(presets.data.custom||{}),'P13 navigation':settings};assert.equal((await api('/api/export/presets',{custom})).data.ok,true);
      await page.evaluate(()=>ReadMDSession.flush());known.push(...inventory().processes);await h.stop();assert.deepEqual(inventory().processes,[]);await boot();await page.evaluate(file=>loadFile(file),input);assert.equal(await page.evaluate(()=>state.original),edited.slice(1));
      const reopened=await api('/api/export/presets',undefined,'GET');assert.deepEqual(reopened.data.custom['P13 navigation'].epub.navigation,settings.epub.navigation);
      await exported('Navigation-reopened',edited,reopened.data.custom['P13 navigation'],{...expected,bodyTokens:['FRONT_SENTINEL','MAIN_EDITED','MAIN_END','INDEX_SENTINEL']});h.report.intentionalSourceEdit={beforeSha256:initial['Navigation-book.md'],afterSha256:hash(input),onlyReplacement:['MAIN_BEGIN','MAIN_EDITED'],bomAndCrlfPreserved:true};
    });
    await h.step('independent pinned EPUBCheck validates every actual file; desktop reader handoff is explicit',async()=>{
      const {checkEpub}=await import('../tools/check-epub.mjs'),validation=await checkEpub(h.report.outputs.map(output=>path.join(repo,output.path)),{jar:process.env.READMD_EPUBCHECK_JAR,output:path.join(h.data,'epubcheck.json'),java:process.env.READMD_JAVA||'java'});h.report.epubcheck=validation;assert.equal(validation.files.length,plan.actualEpubFiles);assert.equal(validation.passed,true);
      h.report.independentReaderHandoff={path:relative(originalBook),sha256:hash(originalBook),expectedNavigation:expected,readerCertification:'pending separate real desktop reading-system operation'};
      fs.writeFileSync(path.join(h.data,'independent-reader-handoff.json'),JSON.stringify(h.report.independentReaderHandoff,null,2)+'\n',{flag:'wx'});
    });
    await h.step('actual file count source product QA and old owned proofs remain bound',()=>{assert.equal(h.report.outputs.length,plan.actualEpubFiles);assert(h.report.outputs.every(output=>output.validated&&hash(path.join(repo,output.path))===output.sha256));h.unchanged(current);freeze.verify();h.report.finalInputHashes=h.snapshotInputs();});
  }catch(error){primary=error;throw error;}finally{try{
    const stopped=await stopAfterInventory(()=>{h.report.preStopProcesses=inventory();known.push(...h.report.preStopProcesses.processes);},()=>h.stop());if(stopped.inventoryError||stopped.stopError)throw stopped.inventoryError||stopped.stopError;
    h.report.postStopProcesses=inventory();assert.deepEqual(h.report.postStopProcesses.processes,[]);reuse?.assertPreserved();freeze.verify();h.unchanged(current);
    for(const[file,expected]of Object.entries(h.report.qaDependencySha256))assert.equal(hash(path.join(repo,file)),expected);assert.equal(hash(candidateFile),h.report.candidateFile.sha256);assert.equal(hash(python),h.report.qaInterpreter.sha256);h.report.hostStopped=true;h.report.sourceAndDependenciesUnchanged=true;
  }catch(error){h.report.postVerificationError=String(error);if(!primary)throw error;}}});
}
function selfTest(){assert.equal(plan.actualEpubFiles,5);assert.equal(JSON.parse(fs.readFileSync(expectation,'utf8')).pages.length,4);return checks.selfTest();}
module.exports={run,plan,selfTest};
if(require.main===module){if(process.argv.includes('--self-test'))console.log(JSON.stringify(selfTest(),null,2));else if(process.argv.includes('--list')||process.argv.includes('--help'))console.log(JSON.stringify({...plan,requiredEnvironment:['READMD_P13_CANDIDATE_JSON','READMD_BIN','READMD_CANDIDATE_SHA256','READMD_CANDIDATE_BOOT_SHA256','READMD_ASSETS_DIR','READMD_ENGINE_REUSE_PROFILE','READMD_EPUBCHECK_JAR','READMD_QA_PYTHON','READMD_QA_PYTHON_SHA256'],nativeExecuted:false},null,2));else run().catch(error=>{console.error(error.stack||error);process.exitCode=1;});}
