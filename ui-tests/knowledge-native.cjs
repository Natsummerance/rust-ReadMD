'use strict';
// Real Windows WebView/CSP, real Rust endpoints, original synthetic workbook.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {chromium}=require('playwright'),{startNative}=require('./native-session.cjs');
const root=path.resolve(__dirname,'..'),work=path.join(root,'.cache/v005-community');
const delay=ms=>new Promise(r=>setTimeout(r,ms));
async function until(fn,timeout=30000,label='Native operation'){const end=Date.now()+timeout;while(Date.now()<end){if(await fn())return;await delay(60);}throw Error(label+' did not settle');}
(async()=>{
 const data=fs.mkdtempSync(path.join(work,'native-')),docs=path.join(data,'documents');fs.mkdirSync(docs,{recursive:true});
 fs.cpSync(path.join(root,'showcase/materials'),docs,{recursive:true});
 process.env.READMD_WORKSPACE=docs;
 const source=path.join(docs,'哲学阅读研究手册.md');
 const report={nativeCsp:true,synthetic:true,originalUnchanged:false,scenarios:[],errors:[]};
 let native;
 const record=async(name,fn)=>{const start=Date.now();const details=await fn();report.scenarios.push({name,passed:true,ms:Date.now()-start,...details});console.log(name+' passed');};
 try{
  native=await startNative(root,data,chromium,28863);const page=native.page;
  page.on('pageerror',e=>report.errors.push(e.message));
  await page.evaluate(async({source,docs})=>{await i18n.setLanguage('zh-CN',false);await loadFile(source);state.folder=docs;state.theme='light';applySettings();},{source,docs});
  await record('rich-reader-content-and-local-assets',async()=>{
   // Theme application can schedule a second native render after loadFile.
   await until(()=>page.evaluate(()=>$('content')?.innerText.includes('26.87%')),30000,'rich document content');
   const actual=await page.evaluate(()=>({chars:state.original.length,tables:document.querySelectorAll('#content table').length,wiki:document.querySelectorAll('.wikilink').length,text:$('content').innerText}));
   assert.ok(actual.chars>4000);assert.ok(actual.tables>=4);assert.ok(actual.wiki>=16);assert.ok(actual.text.includes('26.87%'));
   // Reader figures are lazy-loaded. Exercise each real image in view rather
   // than requiring off-screen assets to defeat the application's lazy load.
   const figures=page.locator('#content img');
   for(let i=0;i<await figures.count();i++){
    await figures.nth(i).scrollIntoViewIfNeeded();
    await until(()=>figures.nth(i).evaluate(img=>img.complete&&img.naturalWidth>=3840),30000,'4K figure '+i);
   }
   await until(()=>page.evaluate(()=>!!document.querySelector('#content mjx-container svg')&&!!document.querySelector('#content .diagram-preview svg')),30000,'math and Mermaid SVG');
   const rendered=await page.evaluate(()=>({formulas:document.querySelectorAll('#content mjx-container svg').length,mathErrors:document.querySelectorAll('#content [data-mml-node="merror"]').length,diagrams:document.querySelectorAll('#content .diagram-preview svg').length,citations:document.querySelectorAll('#content .bib-cite-badge').length,footnotes:document.querySelectorAll('#content .rd-footnotes li').length}));
   assert.equal(rendered.mathErrors,0);assert.ok(rendered.citations>=2);
   if(rendered.footnotes<2)fs.writeFileSync(path.join(work,'native-footnote-diagnostic.json'),JSON.stringify(await page.evaluate(()=>({readerReady:!!window.ReadMDReader,footnoteSections:Array.from(document.querySelectorAll('[class*="footnote"],.rd-footnotes')).map(e=>e.outerHTML),definitions:Array.from(document.querySelectorAll('#content p')).filter(e=>e.textContent.includes('[^')).map(e=>e.outerHTML)})),null,2));
   assert.ok(rendered.footnotes>=2,JSON.stringify(rendered));
   return{characters:actual.chars,tables:actual.tables,wikilinks:actual.wiki,images4K:true,...rendered};
  });
  await record('all-workbook-local-links-and-headings-resolve',async()=>{
   const files=[source,...fs.readdirSync(path.join(docs,'notes')).filter(n=>n.endsWith('.md')).map(n=>path.join(docs,'notes',n))];
   const counts={files:files.length,links:0,wiki:0};
   for(const file of files){const content=fs.readFileSync(file,'utf8');const checked=await page.evaluate(async({file,content,docs})=>(await(await apiFetch('/api/document/analyze',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({file_path:file,workspace_root:docs,content})})).json()),{file,content,docs});
    assert.ok(Array.isArray(checked.links),JSON.stringify(checked));
    const problems=checked.diagnostics.filter(d=>['missing_file','missing_anchor','outside_workspace','ambiguous_link'].includes(d.code));
    assert.deepEqual(problems,[],path.basename(file)+': '+JSON.stringify(problems));
    for(const link of checked.links.filter(l=>l.kind==='wiki')){const found=await page.evaluate(async({file,docs,target})=>{const r=await apiFetch('/api/workspace/resolve',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({workspace_root:docs,file_path:file,target})});return{status:r.status,body:await r.json()};},{file,docs,target:link.target});assert.equal(found.status,200,link.target+': '+JSON.stringify(found.body));counts.wiki++;}
    counts.links+=checked.links.length;
   }return counts;
  });
  await record('native-semantic-completion-tab-undo-and-source-preservation',async()=>{
   await page.evaluate(async()=>{await toggleEdit();setPvLayout('none');setEditorPref('aiCompletion',false);cmView.dispatch({selection:{anchor:cmView.state.doc.length}});cmView.focus();});
   await page.keyboard.type('\n\n[[认识与经验#观察',{delay:55});await until(()=>page.locator('.cm-tooltip-autocomplete').isVisible());
   const box=await page.locator('.cm-tooltip-autocomplete').boundingBox();
   const viewport=await page.evaluate(()=>({width:innerWidth,height:innerHeight}));
   assert.ok(box.x>=0&&box.y>=0&&box.x+box.width<=viewport.width+1&&box.y+box.height<=viewport.height+1);
   const before=await page.evaluate(()=>getEditContent());await page.keyboard.press('Tab');
   assert.ok((await page.evaluate(()=>getEditContent())).endsWith('[[认识与经验#观察笔记]]'));
   await page.keyboard.press('Control+z');assert.equal(await page.evaluate(()=>getEditContent()),before);
   assert.equal(fs.readFileSync(source,'utf8'),fs.readFileSync(path.join(root,'showcase/materials/哲学阅读研究手册.md'),'utf8'));
   await page.evaluate(()=>exitEdit());
   await page.evaluate(source=>loadFile(source,{force:true,discardConfirmed:true}),source);
   return{tabAccept:true,undo:true,sourceUntouched:true};
  });
  await record('native-cross-directory-heading-and-compact-layouts',async()=>{
   await page.evaluate(source=>loadFile(source),source);
   await page.locator('.wikilink').filter({hasText:'观察记录'}).first().click();
   await until(()=>page.evaluate(()=>/认识与经验\.md$/.test(state.file||'')&&!!document.querySelector('.heading-target-highlight')));
   assert.equal(await page.locator('.heading-target-highlight').textContent(),'观察笔记');
   await page.evaluate(()=>toggleEdit());
   const cdp=await page.context().newCDPSession(page);
   for(const [width,height]of[[1024,680],[1160,820]]){
    await cdp.send('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false});
    for(const language of['zh-CN','zh-TW','en']){
     await page.evaluate(l=>i18n.setLanguage(l,false),language);
     assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));
     const rows=await page.evaluate(()=>Array.from(document.querySelectorAll('#edit-toolbar > *')).filter(e=>e.getBoundingClientRect().width&&getComputedStyle(e).display!=='none').map(e=>Math.round(e.getBoundingClientRect().top)));
     // The existing toolbar's own responsive grouping decides which controls
     // to fold; native outer viewport must remain free of horizontal overflow.
     await page.screenshot({path:path.join(work,`native-${width}-${language}.png`)});
    }
   }
   await cdp.detach();await page.evaluate(()=>exitEdit());return{sizes:2,languages:3};
  });
  await record('bounded-suggestion-load-and-no-input-directory-pollution',async()=>{
   const before=fs.readdirSync(docs).sort();
   const timing=await page.evaluate(async docs=>{let finished=0,errors=0;const start=performance.now();await Promise.all(Array.from({length:8},async()=>{for(let n=0;n<20;n++){const r=await apiFetch('/api/workspace/suggest',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({workspace_root:docs,kind:'wiki',query:'',limit:40})});if(!r.ok)errors++;else finished++;}}));return{requests:finished,errors,ms:Math.round(performance.now()-start)};},docs);
   assert.equal(timing.requests,160);assert.equal(timing.errors,0);assert.deepEqual(fs.readdirSync(docs).sort(),before);return timing;
  });
  await record('rich-workbook-six-format-real-conversion',async()=>{
   const inputs=['证据与论证.pdf','哲学阅读工作坊.epub','证据与论证.html','研究数据.csv','研究数据.json','研究摘要.tex'].map(name=>{
    const dir=path.join(docs,'conversion',path.extname(name).slice(1));fs.mkdirSync(dir,{recursive:true});
    const input=path.join(dir,name);fs.copyFileSync(path.join(docs,name),input);return input;
   });
   await page.evaluate(paths=>enqueueBatchFiles(paths,false),inputs);
   await until(()=>page.evaluate(()=>!isBatchRunning()),120000);
   const counts=await page.evaluate(()=>({...batchCount}));assert.equal(counts.ok,6,JSON.stringify(counts));assert.equal(counts.error,0);
   const converted=[];
   for(const input of inputs){const output=input.replace(/\.[^.]+$/,'.md'),text=fs.readFileSync(output,'utf8');assert.ok(text.includes('认识与经验'),path.extname(input));assert.ok(text.includes('开放问题'),path.extname(input));converted.push({format:path.extname(input),characters:text.length});}
   await page.evaluate(()=>closeBatchModal());return{formats:converted,originalsPreserved:inputs.every(f=>fs.existsSync(f))};
  });
  if(process.env.READMD_RECORD_COMMUNITY==='1'){
   const capture=require(path.join(root,'showcase/updates/v0.0.5/community-workspace/record.cjs'));
   await capture({page,docs,source,until});
  }
  report.originalUnchanged=fs.readFileSync(source,'utf8')===fs.readFileSync(path.join(root,'showcase/materials/哲学阅读研究手册.md'),'utf8');
  assert.equal(report.errors.length,0,JSON.stringify(report.errors));report.passed=true;
 }catch(e){report.passed=false;report.failure=e.message;if(native?.page)report.diagnostic=await native.page.evaluate(()=>({file:state.file,content:$('content')?.innerText.slice(0,2200),math:document.querySelectorAll('mjx-container').length,diagrams:Array.from(document.querySelectorAll('.diagram-preview')).map(el=>el.innerText),footnotes:document.querySelectorAll('.rd-footnotes li').length})).catch(()=>null);throw e;}
 finally{fs.writeFileSync(path.join(work,'native-report.json'),JSON.stringify(report,null,2));if(native){await native.browser.close().catch(()=>{});native.server.child.kill();}}
})().catch(e=>{console.error(e.message);process.exitCode=1;});
