'use strict';
// Real scans, actual native CSP, owned offline engine/models and PDFium.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),{spawnSync}=require('node:child_process');
const {chromium}=require('playwright'),{startNative}=require('./native-session.cjs');
const root=path.resolve(__dirname,'..'),data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/portable-ocr-native-')),docs=path.join(data,'documents');fs.mkdirSync(docs);
const pack=path.resolve(process.env.READMD_OCR_PACK||'.cache/capability-upgrade/readmd-ocr-5.5.3-r1-windows-x64.readmd-pack'),pdfPack=path.resolve(process.env.READMD_PDF_PACK||'.cache/capability-upgrade/packages/readmd-pdfium-7881-windows-x64.readmd-pack');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex'),report={platform:'windows-x64',nativeCsp:true,systemPathRestricted:true,checks:[],passed:false};let native,phase='fixtures';
const python=String.raw`from PIL import Image,ImageDraw,ImageFont
import pymupdf as pdf,sys,pathlib
d=pathlib.Path(sys.argv[1]);im=Image.new('RGB',(1900,650),'white');g=ImageDraw.Draw(im)
en=ImageFont.truetype('C:/Windows/Fonts/arial.ttf',66);zh=ImageFont.truetype('C:/Windows/Fonts/msyh.ttc',66)
g.text((60,60),'READMD OFFLINE 12345',font=en,fill='black')
g.text((60,210),'阅读与研究：保留原始证据',font=zh,fill='black')
g.text((60,360),'OBSERVATIONS RETAIN ORIGINAL EVIDENCE',font=ImageFont.truetype('C:/Windows/Fonts/arial.ttf',57),fill='black')
for ext in ['png','jpg','bmp','gif','tif','webp']:im.save(d/('Scan.'+ext))
book=pdf.open()
for i in range(2):
 p=book.new_page(width=595,height=842);p.insert_image(pdf.Rect(24,80,571,268),filename=str(d/'Scan.png'))
book.save(d/'Scans.pdf')
large=Image.new('RGB',(3000,3500),'white');q=ImageDraw.Draw(large)
for i in range(28):q.text((50,40+i*120),'READMD EVIDENCE '+str(i)+' OBSERVATION AND REASONING',font=en,fill='black')
large.save(d/'Long-scan.png')
`;
async function stop(){if(!native)return;const child=native.server.child;const end=child.exitCode!==null||child.signalCode!==null?Promise.resolve():new Promise(r=>child.once('exit',r));child.kill();await Promise.race([end,new Promise(r=>setTimeout(r,2000))]);await native.browser.close().catch(()=>{});}
(async()=>{try{
 const generated=spawnSync('python',['-c',python,docs],{windowsHide:true,encoding:'utf8'});assert.equal(generated.status,0,generated.stderr);report.binarySha256=sha(fs.readFileSync(process.env.READMD_BIN));report.bootSha256=sha(fs.readFileSync(path.join(root,'assets/readmd.boot.js')));report.packSha256=sha(fs.readFileSync(pack));
 const originals=Object.fromEntries(fs.readdirSync(docs).map(n=>[n,sha(fs.readFileSync(path.join(docs,n)))]));
 const savedPath=process.env.PATH;process.env.PATH=process.env.SystemRoot+'/System32;'+process.env.SystemRoot;try{native=await startNative(root,data,chromium,28979);}finally{process.env.PATH=savedPath;}
 const page=native.page;await page.evaluate(()=>i18n.setLanguage('zh-CN',false));
 const api=(route,body)=>page.evaluate(async({route,body})=>{const r=await apiFetch(route,{...(body?{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)}:{})});return {status:r.status,data:await r.json()};},{route,body});
 const pick=async file=>{await page.evaluate(()=>{hasPy=false;window.pywebview=null;});const choose=page.waitForEvent('filechooser');await page.getByRole('button',{name:'导入离线包',exact:true}).click();await(await choose).setFiles(file,{timeout:120000});};
 phase='contextual installation';await page.evaluate(file=>{window.__ocr=ocrFile(file);},path.join(docs,'Scan.png'));await page.locator('#capability-install-modal').waitFor({state:'visible'});assert.match(await page.locator('#capability-install-modal').innerText(),/9.4 MiB/);
 // Visibility can become true at opacity zero. Preserve the real transition,
 // but capture the finished dialog rather than its translucent first frame.
 await page.locator('#capability-install-modal').evaluate(async element=>{await Promise.all(element.getAnimations({subtree:true}).filter(animation=>animation.effect.getComputedTiming().iterations!==Infinity).map(animation=>animation.finished.catch(()=>{})));});
 await page.screenshot({path:path.join(data,'ocr-install-prompt.png')});await pick(pack);await page.locator('#capability-install-modal').waitFor({state:'detached',timeout:60000});assert.equal(await page.evaluate(()=>window.__ocr),true);
 assert.match(await page.locator('#content').innerText(),/READMD OFFLINE 12345/);report.checks.push('First OCR use offers exact engine/model size, imports the real offline pack and resumes recognition in the original native document.');
 phase='six image formats';let recognized;
 for(const ext of ['png','jpg','bmp','gif','tif','webp']){const r=await api('/api/ocr?p='+encodeURIComponent(path.join(docs,'Scan.'+ext))+'&details=1');assert.equal(r.status,200,JSON.stringify(r.data));assert.equal(r.data.engine,'readmd-tesseract');assert.match(r.data.content,/READMD OFFLINE 12345/);assert.ok((r.data.content.match(/[\u4e00-\u9fff]/g)||[]).length>=10);recognized=r.data.content;}
 report.checks.push('PNG/JPEG/BMP/GIF/TIFF/WebP perform real Chinese/English recognition with the managed engine, not Windows OCR or a system program.');
 phase='PDF dependency and resume';await page.evaluate(file=>{window.__ocrPdf=ocrFile(file);},path.join(docs,'Scans.pdf'));await page.locator('#capability-install-modal').waitFor({state:'visible',timeout:60000});assert.match(await page.locator('#capability-install-title').innerText(),/PDF/);await pick(pdfPack);await page.locator('#capability-install-modal').waitFor({state:'detached',timeout:60000});assert.equal(await page.evaluate(()=>window.__ocrPdf),true);assert.match(await page.locator('#content').innerText(),/第 2 页/);report.checks.push('Scanned PDF requests the missing PDF raster engine in context, resumes both actual pages and keeps OCR independent of the OS PDF renderer.');
 phase='save and reopen';const target=path.join(docs,'Recognized.md');fs.writeFileSync(target,'');await page.evaluate(file=>loadFile(file),target);const saved=await api('/api/save',{path:target,content:recognized});assert.equal(saved.status,200,JSON.stringify(saved.data));await page.evaluate(file=>loadFile(file),target);assert.match(await page.locator('#content').innerText(),/READMD OFFLINE 12345/);report.checks.push('Recognized Markdown is atomically saved through the real service and reopened with its source image reference.');
 phase='malformed image';const invalid=path.join(docs,'Invalid.png');fs.writeFileSync(invalid,'not a valid image');const failure=await api('/api/ocr?p='+encodeURIComponent(invalid));assert.notEqual(failure.status,200);assert.equal(failure.data.error_code,'ocr_failed');report.checks.push('Invalid images produce an actual failure instead of successful empty recognition.');
 phase='cancellation';await page.evaluate(file=>{window.__longOcr=apiFetch('/api/ocr?p='+encodeURIComponent(file)+'&task_id=portable-ocr-cancel').then(async r=>({status:r.status,data:await r.json()}));},path.join(docs,'Long-scan.png'));
 const cancelStart=Date.now();let cancel;for(let i=0;i<40;i++){cancel=await api('/api/task/cancel',{id:'portable-ocr-cancel'});if(cancel.data.state==='cancelling')break;await new Promise(r=>setTimeout(r,25));}assert.equal(cancel.data.state,'cancelling');report.cancellationAckMs=Date.now()-cancelStart;const stopped=await page.evaluate(()=>window.__longOcr);assert.equal(stopped.data.error_code,'cancelled');report.cancellationCompleteMs=Date.now()-cancelStart;report.checks.push('Real OCR cancellation returns a cancelled result and leaves no converted output or input changes.');
 phase='corrupt model and repair';const model=path.join(data,'capabilities/managed-v2/ocr/5.5.3-r1/tessdata/chi_sim.traineddata');fs.appendFileSync(model,'corrupt');assert.equal((await api('/api/capabilities/packs',{action:'status',id:'ocr'})).data.installed,false);assert.equal((await api('/api/capabilities/packs',{action:'install',path:pack})).data.repaired,true);report.checks.push('Model corruption invalidates readiness; reinstall validates and repairs the exact model with a real recognition probe.');
 phase='restart';await stop();native=await startNative(root,data,chromium,28979);const restored=await native.page.evaluate(async file=>{const r=await apiFetch('/api/ocr?p='+encodeURIComponent(file)+'&details=1');return r.json();},path.join(docs,'Scan.png'));assert.equal(restored.engine,'readmd-tesseract');assert.match(restored.content,/READMD OFFLINE 12345/);report.checks.push('Installed models survive a native restart and the request uses its isolated application profile.');
 for(const[name,digest]of Object.entries(originals))assert.equal(sha(fs.readFileSync(path.join(docs,name))),digest);assert.ok(!fs.existsSync(path.join(docs,'readmd-assets')));report.passed=true;
}catch(error){report.error=String(error.message).replaceAll(data,'<profile>').replaceAll(root,'<workspace>');report.failedStage=phase;process.exitCode=1;if(native)await native.page.screenshot({path:path.join(data,'failure.png')}).catch(()=>{});
}finally{await stop();report.recordedAt=new Date().toISOString();fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,report:path.relative(root,path.join(data,'report.json')),failedStage:report.failedStage}));}})();
