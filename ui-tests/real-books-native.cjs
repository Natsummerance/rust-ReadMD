'use strict';
// Opt-in user-authorized books. Originals are read-only; only isolated copies
// are converted/annotated. Reports contain no book text, titles or source paths.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const {chromium}=require('playwright'),{startNative}=require('./native-session.cjs');
const root=path.resolve(__dirname,'..'),data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/real-books-')),docs=path.join(data,'documents');fs.mkdirSync(docs);
const digest=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const inputs=[['epub',process.env.READMD_TEST_EPUB],['pdf',process.env.READMD_TEST_PDF]].map(([format,file])=>{assert.ok(file,format+' source required');assert.equal(path.extname(file).toLowerCase(),'.'+format);return {format,source:path.resolve(file),copy:path.join(docs,'Research-book.'+format),sha256:digest(file),bytes:fs.statSync(file).size};});
const report={platform:'windows-x64',nativeCsp:true,synthetic:false,privateInputs:true,originalsModified:false,checks:[],passed:false};let native,phase='startup';
async function until(check){const deadline=Date.now()+90000;while(!await check()){if(Date.now()>deadline)throw Error('Timed out: '+phase);await new Promise(r=>setTimeout(r,100));}}
(async()=>{try{
 for(const input of inputs)fs.copyFileSync(input.source,input.copy);
 process.env.READMD_WORKSPACE=docs;report.binarySha256=digest(process.env.READMD_BIN);
 native=await startNative(root,data,chromium,28923);const page=native.page;await page.evaluate(()=>window.__readmdSessionReady);
 const request=(route,body)=>page.evaluate(async({route,body})=>{const r=await apiFetch(route,{signal:AbortSignal.timeout(120000),...(body===undefined?{}:{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)})});return {status:r.status,data:await r.json()};},{route,body});
 phase='EPUB conversion preview';assert.equal(await page.evaluate(()=>ensureModule('convert')),true);
 const started=Date.now(),epub=inputs[0],converted=await request('/api/convert?preview=1&p='+encodeURIComponent(epub.copy));
 assert.equal(converted.status,200);assert.ok(converted.data.content.length>10000);assert.equal(converted.data.saved,false);
 assert.deepEqual(fs.readdirSync(docs).sort(),['Research-book.epub','Research-book.pdf']);
 await page.evaluate(d=>renderVirtual('convert','Research-book.md',d.dir,d.content,d.fixes,{assets:d.assets}),converted.data);
 await until(()=>page.locator('#content h1,#content h2,#content p').count());
 assert.equal(await page.evaluate(()=>hasUnsavedEditorChanges()),false);
 report.epub={bytes:epub.bytes,sourceSha256:epub.sha256,characters:converted.data.content.length,headings:(converted.data.content.match(/^#{1,6}\s/gm)||[]).length,warningCount:converted.data.warns?.length||0,previewMs:Date.now()-started};
 report.checks.push('real EPUB converted and displayed in native WebView; preview creates no adjacent files');
 phase='explicit Markdown save and native editing';const output=path.join(docs,'Research-book.md');
 assert.ok(converted.data.assets.length>0,'the real book must exercise embedded image staging');
 for(const asset of converted.data.assets){assert.ok(fs.existsSync(asset.path));assert.ok(!path.resolve(asset.path).startsWith(path.resolve(docs)+path.sep));}
 const saved=await request('/api/convert?p='+encodeURIComponent(epub.copy)+'&on_exists=rename');assert.equal(saved.status,200);assert.equal(saved.data.saved,true);assert.equal(saved.data.out,output);
 assert.ok(saved.data.content.includes('readmd-assets/'));assert.ok(!saved.data.content.includes('/raw?p='));
 assert.ok(fs.readdirSync(path.join(docs,'readmd-assets')).length>0);
 await page.evaluate(async file=>{await loadFile(file);await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('none');cmView.dispatch({changes:{from:0,insert:'# Local review copy\n\n'}});},output);
 const changed=await page.evaluate(()=>getEditContent());assert.equal(await page.evaluate(()=>saveEdit()),true);assert.equal(fs.readFileSync(output,'utf8'),changed);
 await page.evaluate(async file=>{await exitEdit();await loadFile(file,{force:true,discardConfirmed:true});},output);
 await until(()=>page.locator('#content h1').first().textContent().then(t=>t.includes('Local review copy')));
 await until(()=>page.locator('#content img').evaluateAll(images=>images.length>0&&images.every(image=>image.complete&&image.naturalWidth>0)));
 report.checks.push('explicit Markdown creation, native editing/atomic save and reopen preserve the complete converted book');
 phase='PDF capability install';const installed=await request('/api/capabilities/packs',{action:'install',path:process.env.READMD_PDF_PACK});assert.equal(installed.status,200);assert.equal(installed.data.installed,true);
 const pdf=payload=>request('/api/pdf/workspace',payload),pdfStart=Date.now(),opened=await pdf({action:'open',path:inputs[1].copy,width:1000});assert.equal(opened.status,200);let state=opened.data;assert.ok(state.pages>10);
 const pages=[...new Set([0,Math.floor(state.pages/2),state.pages-1])],renders={};
 for(const page of pages){const rendered=await pdf({action:'render',session:state.session,revision:state.revision,page,width:1000});assert.equal(rendered.status,200);assert.ok(rendered.data.image.length>1000);renders[page]=crypto.createHash('sha256').update(rendered.data.image).digest('hex');}
 const pageCount=state.pages;assert.ok(state.permissions.annotations);
 phase='real PDF annotation and save';const edited=await pdf({action:'edit',session:state.session,revision:state.revision,width:1000,operation:{kind:'note',page:0,x:40,y:40,text:'Local review copy'}});assert.equal(edited.status,200);state=edited.data;assert.ok(state.dirty);assert.equal(digest(inputs[1].copy),inputs[1].sha256);
 const committed=await pdf({action:'save',session:state.session,revision:state.revision});assert.equal(committed.status,200);await pdf({action:'close',session:state.session});
 const reopened=await pdf({action:'open',path:inputs[1].copy,width:1000});assert.equal(reopened.status,200);state=reopened.data;assert.equal(state.pages,pageCount);assert.ok(state.annotations.some(a=>a.text==='Local review copy'));
 for(const page of pages.filter(p=>p!==0)){const rendered=await pdf({action:'render',session:state.session,revision:state.revision,page,width:1000});assert.equal(rendered.status,200);assert.equal(crypto.createHash('sha256').update(rendered.data.image).digest('hex'),renders[page]);}
 await pdf({action:'close',session:state.session});
 assert.equal(await page.evaluate(file=>ReadMDPdf.open(file),inputs[1].copy),true);await until(()=>page.locator('#pdf-image').evaluate(img=>img.naturalWidth>0));await page.locator('#pdf-close').click();
 report.pdf={bytes:inputs[1].bytes,sourceSha256:inputs[1].sha256,pages:pageCount,sampledPages:pages,unmodifiedPagesPixelIdentical:true,elapsedMs:Date.now()-pdfStart};
 assert.deepEqual(fs.readdirSync(docs).sort(),['Research-book.epub','Research-book.md','Research-book.pdf','readmd-assets']);
 report.checks.push('real multipage PDF renders first/middle/last pages, saves/reopens an annotation, and preserves unmodified sampled pages pixel-for-pixel');report.passed=true;
}catch(error){report.failedStage=phase;report.error=error.name;report.message=String(error.message).replaceAll(root,'<workspace>');for(const input of inputs)report.message=report.message.replaceAll(input.source,'<private-source>');report.message=report.message.slice(0,900);process.exitCode=1;}
finally{if(native){await native.browser.close().catch(()=>{});native.server.child.kill();}report.originalsModified=inputs.some(input=>digest(input.source)!==input.sha256);if(report.originalsModified){report.passed=false;process.exitCode=1;}report.recordedAt=new Date().toISOString();fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,report:path.relative(root,path.join(data,'report.json')),failedStage:report.failedStage}));}})().catch(()=>{console.error('Real book validation failed; no private paths printed.');process.exitCode=1;});
