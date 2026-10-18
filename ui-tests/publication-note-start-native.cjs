'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict'),net=require('node:net');
const {chromium}=require('playwright'),{startNative}=require('./native-session.cjs'),zipParts=require('./zip-parts.cjs');
const root=path.resolve(__dirname,'..'),data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/publication-note-start-native-')),docs=path.join(data,'documents');fs.mkdirSync(docs);
const hash=f=>crypto.createHash('sha256').update(fs.readFileSync(f)).digest('hex');
const report={passed:false,platform:'windows-x64',formalCsp:true,synthetic:true,checks:[],pageErrors:[],outputs:[]};
const source='# Evidence report\n\nFirst claim[^shared], repeated[^shared]. Another claim[^second].\n\n# Results\n\nThe same evidence[^shared] still has the same identity.\n\n| Measure | Result |\n| --- | --- |\n| Consistency | Verified |\n\n[^shared]: Shared evidence ALPHA. A note links [back to results](#results).\n\n    A second paragraph with an editable formula :math[frac(a, b)].\n\n[^second]: Other evidence BETA.\n';
const file=path.join(docs,'Note-numbering.md');fs.writeFileSync(file,source);
let host,page,stage='startup';
async function port(){const s=net.createServer();await new Promise(r=>s.listen(0,'127.0.0.1',r));const p=s.address().port;await new Promise(r=>s.close(r));return p;}
async function until(fn){const end=Date.now()+90000;while(!await fn()){assert(Date.now()<end,'Timed out: '+stage);await new Promise(r=>setTimeout(r,75));}}
async function stop(){if(!host)return;await host.browser.close().catch(()=>{});const c=host.server.child;if(c.exitCode===null){const done=new Promise(r=>c.once('exit',r));c.kill();await Promise.race([done,new Promise(r=>setTimeout(r,2000))]);}}
const api=(route,body)=>page.evaluate(async({route,body})=>{const r=await apiFetch(route,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});return{status:r.status,data:await r.json()};},{route,body});
async function output(format,position,options){const out=path.join(docs,'Notes-'+position+'.'+format),r=await api('/api/export',{format,content:source,baseDir:docs,out_path:out,options});assert.equal(r.status,200,JSON.stringify(r.data));assert.equal(r.data.ok,true,JSON.stringify(r.data));assert(fs.statSync(out).size>1000);report.outputs.push({format,position,path:path.relative(data,out),sha256:hash(out)});return out;}
async function pdfProof(file,position){
 const opened=(await api('/api/pdf/workspace',{action:'open',path:file,width:1000})).data;assert.equal(opened.ok,true,JSON.stringify(opened));
 const pages=[];try{for(let at=0;at<opened.pages;at++){const r=at===0?opened:(await api('/api/pdf/workspace',{action:'render',session:opened.session,revision:opened.revision,page:at,width:1000})).data;assert.equal(r.ok,true);pages.push({objects:r.objects,links:r.links});fs.writeFileSync(path.join(data,position+'-page-'+at+'.json'),JSON.stringify(pages.at(-1),null,2));if(at===0)fs.writeFileSync(path.join(data,position+'-page-0.bmp'),Buffer.from(r.image,'base64'));}}finally{await api('/api/pdf/workspace',{action:'close',session:opened.session});}
 const text=pages.flatMap(p=>p.objects.filter(o=>o.kind==='text').map(o=>o.text)).join('\n');
 assert(text.includes('ALPHA')&&text.includes('BETA'));assert(text.includes('AA')&&text.includes('AB'),'PDF must contain actual offset note labels');
 const noteSpacing=pages.flatMap((page,index)=>{
  const body=page.objects.find(o=>o.kind==='text'&&o.text.startsWith('Shared evidence ALPHA'));
  if(!body)return[];
  const marker=page.objects.find(o=>o.kind==='text'&&o.text.trim()==='AA'&&o.bounds.x<body.bounds.x&&Math.abs(o.bounds.y-body.bounds.y)<20);
  assert(marker,'Real note marker and body must be locatable');
  assert(marker.bounds.x+marker.bounds.width<=body.bounds.x+1,'Long note labels cannot overlap note body');
  return[{page:index,marker:marker.bounds,body:body.bounds}];
 });assert(noteSpacing.length>0);
 const internal=pages.flatMap(p=>p.links).filter(l=>l.target.kind==='internal');assert(internal.length>=5);assert(internal.every(l=>l.target.page>=0&&l.target.page<pages.length));
 return{pages:pages.length,internalLinks:internal.length,text,noteSpacing};
}
(async()=>{try{
 assert(process.env.READMD_BIN&&process.env.READMD_FONTS_PACK&&process.env.READMD_TYPST_PACK&&process.env.READMD_PDF_PACK);
 process.env.READMD_WORKSPACE=docs;report.binarySha256=hash(process.env.READMD_BIN);report.bootSha256=hash(path.join(root,'assets/readmd.boot.js'));report.scriptSha256=hash(__filename);
 host=await startNative(root,data,chromium,await port());page=host.page;page.on('pageerror',e=>report.pageErrors.push(e.message));await page.evaluate(()=>Promise.all([__readmdSessionReady,__readmdPreferencesReady]));
 for(const[id,key]of [['fonts','READMD_FONTS_PACK'],['typst','READMD_TYPST_PACK'],['pdfium','READMD_PDF_PACK']])assert.equal((await api('/api/capabilities/packs',{action:'install',id,path:process.env[key]})).data.ok,true);
 stage='native preset controls';await page.setViewportSize({width:1160,height:820});await page.evaluate(async file=>{await i18n.setLanguage('zh-CN',false);await loadFile(file);await openExportModal();},file);
 await page.locator('#export-tab-epub').click();await page.locator('#export-box .export-customize > summary').click();
 const group=page.locator('.exp-sec').filter({has:page.locator('[data-k="footnotes.start"]')});await group.locator('.exp-sec-head').click();
 const start=page.locator('[data-k="footnotes.start"]');assert.equal(await start.inputValue(),'1');await start.fill('27');await start.dispatchEvent('change');
 await page.locator('[data-k="footnotes.numberFormat"]').selectOption('upperLetter');await page.locator('[data-k="footnotes.position"]').selectOption('document');await page.locator('[data-k="footnotes.title"]').fill('研究注释');
 assert.equal(await page.evaluate(()=>collectExportOptions().footnotes.start),27);
 await page.locator('#exp-save-preset').click();await page.locator('#exp-save-input').fill('Offset note study');await page.locator('#exp-save-ok').click();await until(()=>page.locator('#exp-save-name').isHidden());
 report.checks.push('The existing native export panel exposes a localized starting number, accepts 27 with alphabetic labels and saves it in a user preset without changing the document.');
 const base=await page.evaluate(()=>collectExportOptions());report.pdf={};
 for(const position of ['page','document']){
  const options={...base,pdf:{engine:'typst'},typography:{...base.typography,font:'Arial',size:11},footnotes:{...base.footnotes,position,start:27,numberFormat:'upperLetter'},cover:{enabled:false},toc:{enabled:false},footer:{pageNumbers:false},epub:{...base.epub,splitLevel:'h1'}};
  stage=position+' actual DOCX';const word=await output('docx',position,options),parts=zipParts(fs.readFileSync(word)),kind=position==='page'?'footnote':'endnote',settings=parts.get('word/settings.xml');
  const native=await page.evaluate(({settings,body,notes,kind})=>{const parse=s=>new DOMParser().parseFromString(s,'application/xml'),w='http://schemas.openxmlformats.org/wordprocessingml/2006/main',s=parse(settings),b=parse(body),n=parse(notes);return{valid:[s,b,n].every(d=>!d.querySelector('parsererror')),start:s.getElementsByTagNameNS(w,'numStart')[0]?.getAttributeNS(w,'val'),format:s.getElementsByTagNameNS(w,'numFmt')[0]?.getAttributeNS(w,'val'),references:[...b.getElementsByTagNameNS(w,kind+'Reference')].map(e=>e.getAttributeNS(w,'id')),definitions:[...n.getElementsByTagNameNS(w,kind)].filter(e=>!e.getAttributeNS(w,'type')).map(e=>e.getAttributeNS(w,'id'))};},{settings,body:parts.get('word/document.xml'),notes:parts.get('word/'+kind+'s.xml'),kind});
  assert.equal(native.valid,true);assert.equal(native.start,'27');assert.equal(native.format,'upperLetter');assert.deepEqual(native.references,['1','1','2','1']);assert.deepEqual(native.definitions,['1','2']);fs.writeFileSync(path.join(data,position+'-settings.xml'),settings);
  report.checks.push('Real '+kind+' DOCX uses w:numStart=27 and native upperLetter numbering; three shared references retain ID 1 and only two definitions are emitted.');
  stage=position+' actual EPUB';const epub=await output('epub',position,options),entries=[...zipParts(fs.readFileSync(epub))].filter(([n])=>/chapter_\d+\.xhtml$/.test(n));
  const labels=await page.evaluate(entries=>{const docs=entries.map(([file,s])=>[file.split('/').pop(),new DOMParser().parseFromString(s,'application/xhtml+xml')]),ids=new Map(docs.map(([f,d])=>[f,new Set([...d.querySelectorAll('[id]')].map(e=>e.id))]));return{valid:docs.every(([,d])=>!d.querySelector('parsererror')),marks:docs.flatMap(([,d])=>[...d.querySelectorAll('.noteref')].map(e=>e.textContent)),backs:docs.flatMap(([,d])=>[...d.querySelectorAll('.fn-back')].map(e=>e.textContent)),links:docs.flatMap(([f,d])=>[...d.querySelectorAll('a[href]')].filter(e=>e.getAttribute('href').includes('#')).map(e=>{const[to,id]=e.getAttribute('href').split('#');return ids.get(to||f)?.has(id)===true;}))};},entries);
  assert.equal(labels.valid,true);assert.deepEqual(labels.marks,['AA','AA','AB','AA']);assert(labels.backs.includes('AA.')&&labels.backs.includes('AB.'));assert(labels.links.length>=8&&labels.links.every(Boolean));report[position+'Epub']=labels;
  const {checkEpub}=await import('../tools/check-epub.mjs');assert.equal((await checkEpub([epub],{jar:process.env.READMD_EPUBCHECK_JAR,output:path.join(data,position+'-epubcheck.json')})).passed,true);
  report.checks.push('Real '+kind+' EPUB displays AA/AB while every forward/back target resolves; pinned offline EPUBCheck passes.');
  stage=position+' actual paginated PDF';const pdf=await output('pdf',position,options);report.pdf[position]=await pdfProof(pdf,position);
  report.checks.push('Real Typst '+kind+' PDF reopens in managed PDFium with actual AA/AB labels, complete evidence and valid internal links.');
 }
 stage='real PDF loading and hidden controls';await page.locator('#export-close').click();await page.setViewportSize({width:1024,height:680});
 const pdfFile=path.join(data,report.outputs.find(o=>o.format==='pdf'&&o.position==='document').path);
 const ui=await page.evaluate(async file=>{
  const observations=[],capture=()=>{const l=document.getElementById('pdf-loading'),s=document.getElementById('pdf-stage');if(l&&s)observations.push({loadingHidden:l.hidden,loadingDisplay:getComputedStyle(l).display,stageHidden:s.hidden,stageDisplay:getComputedStyle(s).display});};
  const observer=new MutationObserver(capture);observer.observe(document.body,{subtree:true,childList:true,attributes:true,attributeFilter:['hidden','class']});
  const ok=await ReadMDPdf.open(file);await new Promise(requestAnimationFrame);await new Promise(requestAnimationFrame);capture();observer.disconnect();
  return{ok,observations,hidden:[...document.querySelectorAll('#pdf-modal [hidden]')].map(e=>({id:e.id,display:getComputedStyle(e).display})),height:document.getElementById('pdf-image').naturalHeight};
 },pdfFile);
 assert.equal(ui.ok,true);assert(ui.height>0);assert(ui.observations.some(o=>!o.loadingHidden&&o.loadingDisplay!=='none'&&o.stageHidden&&o.stageDisplay==='none'));assert(ui.hidden.every(e=>e.display==='none'));report.pdfUi=ui;await page.screenshot({path:path.join(data,'pdf-native-1024.png')});await page.locator('#pdf-close').click();
 report.checks.push('Actual PDF UI shows loading while stale content is hidden; after rendering, hidden loader/context controls remain invisible without CSS !important at 1024×680.');
 stage='persisted preset and restart';assert.equal(fs.readFileSync(file,'utf8'),source);await page.evaluate(()=>ReadMDSession.flush());await stop();host=await startNative(root,data,chromium,await port());page=host.page;await page.evaluate(()=>Promise.all([__readmdSessionReady,__readmdPreferencesReady]));await page.evaluate(async f=>{await loadFile(f);await openExportModal();},file);
 assert.equal(await page.locator('#exp-preset').inputValue(),'Offset note study');assert.equal(await page.evaluate(()=>collectExportOptions().footnotes.start),27);assert.equal(await page.evaluate(()=>collectExportOptions().footnotes.numberFormat),'upperLetter');assert.equal(await page.evaluate(()=>collectExportOptions().footnotes.position),'document');
 assert.equal(fs.readFileSync(file,'utf8'),source);assert(!fs.existsSync(path.join(docs,'readmd-assets')));assert.deepEqual(report.pageErrors,[]);
 report.checks.push('A real process restart reloads the chosen preset and all note parameters; opening and exporting leave the original Markdown and its directory clean.');report.passed=true;
 }catch(e){report.failedStage=stage;report.error=String(e.stack||e.message).replaceAll(root,'<workspace>').slice(0,1900);if(page)await page.screenshot({path:path.join(data,'failure.png')}).catch(()=>{});process.exitCode=1;}
 finally{await stop();report.recordedAt=new Date().toISOString();report.scope='Native starting-number increment; actual Word application editing, independent EPUB reader, chapter/page restart policies and complete B3 remain unverified.';fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,report:path.relative(root,path.join(data,'report.json')),failedStage:report.failedStage}));}
})();
