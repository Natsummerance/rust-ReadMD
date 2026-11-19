'use strict';
// Explicit staged managed-pack acceptance. All PDFs, profiles and writes are owned.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto'),net=require('node:net');
const {spawn,spawnSync}=require('node:child_process');
const {connect}=require('./v006-table-clipboard-kernel.cjs');
const root=path.resolve(__dirname,'../..'),sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function until(check){const end=Date.now()+15000;while(Date.now()<end){if(await check())return;await delay(50);}throw Error('Owned PDF acceptance deadline');}
async function main(){
  const args={};for(let i=2;i<process.argv.length;i+=2){assert(['--kernel','--assets','--python','--output','--playwright'].includes(process.argv[i])&&process.argv[i+1]);args[process.argv[i].slice(2)]=path.resolve(process.argv[i+1]);}
  for(const name of ['kernel','assets','python','output'])assert(args[name],name+' required');
  assert(!fs.existsSync(args.output));fs.mkdirSync(args.output,{recursive:true});
  const docs=path.join(args.output,'documents'),profile=path.join(args.output,'profile');fs.mkdirSync(profile);
  const python=(...options)=>{const result=spawnSync(args.python,[path.join(__dirname,'pdf-choice-fixture.py'),...options],{encoding:'utf8',timeout:30000});assert.equal(result.status,0,result.stderr);return JSON.parse(result.stdout);};
  python('create',docs);
  const source=path.join(docs,'multi.pdf'),original=fs.readFileSync(source);
  const pack=path.join(args.assets,'capabilities/bundled/ReadMD-PDFium-macos-arm64.readmd-pack');
  const lock=JSON.parse(fs.readFileSync(path.join(args.assets,'capabilities/pdfium-7881-macos-arm64.lock.json')));
  const report={platform:process.platform+'-'+process.arch,syntheticOnly:true,nativeGuiCertified:false,managedPack:true,
    kernelSha256:sha(fs.readFileSync(args.kernel)),packSha256:sha(fs.readFileSync(pack)),
    workerSha256:lock.files.find(file=>file.path==='bin/readmd-pdf-worker').sha256,
    librarySha256:lock.files.find(file=>file.path==='bin/libpdfium.dylib').sha256,
    choiceSourceSha256:sha(fs.readFileSync(path.join(root,'rust/readmd-kernel/src/pdf_choice.rs'))),
    frontendSha256:sha(fs.readFileSync(path.join(args.assets,'js/features/pdf.js'))),
    bootSha256:sha(fs.readFileSync(path.join(args.assets,'readmd.boot.js'))),runnerSha256:sha(fs.readFileSync(__filename)),checks:[]};
  const record=name=>report.checks.push({name,passed:true});
  const portServer=net.createServer();await new Promise(resolve=>portServer.listen(0,'127.0.0.1',resolve));const port=portServer.address().port;await new Promise(resolve=>portServer.close(resolve));
  const env={...process.env,READMD_DATA_DIR:profile,READMD_WORKSPACE:docs,READMD_ASSETS_DIR:args.assets,READMD_EXTRA_ROOTS:''};
  delete env.READMD_PDF_WORKER;delete env.READMD_PDFIUM_LIB;delete env.READMD_APP_TOKEN;
  let host,browser,context,phase='startup';
  const child=spawn(args.kernel,['--no-window','--host','127.0.0.1','--port',String(port),'--data-dir',profile,'--workspace',docs,'--assets',args.assets],{cwd:root,env,stdio:'ignore'});
  const exited=new Promise(resolve=>child.once('close',(code,signal)=>resolve({code,signal})));
  try{
    await until(async()=>{assert.equal(child.exitCode,null);try{host=await connect('http://127.0.0.1:'+port);return true;}catch{return false;}});
    const post=async(route,payload,status=200)=>{const response=await host.api(route,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(payload)}),result=await response.json();assert.equal(response.status,status,phase+': '+JSON.stringify(result));if(status===200)assert.equal(result.ok,true);return result;};
    phase='staged managed install';
    await post('/api/capabilities/packs',{action:'install',id:'pdfium',path:pack});
    await post('/api/pdf/workspace',{action:'probe'});
    record('Fresh owned profile installs the staged, hash-pinned Mac PDFium pack and probes its actual worker');
    const pdf=(payload,status=200)=>post('/api/pdf/workspace',payload,status);
    let state=await pdf({action:'open',path:source,width:600});
    const operation=(action,extra={},status=200)=>pdf({action,session:state.session,revision:state.revision,page:0,width:600,...extra},status);
    const selected=data=>data.annotations.find(annotation=>annotation.kind==='choice');
    assert.equal(selected(state).text,'Standard delivery');
    assert.deepEqual(selected(state).options.map(option=>option.label),['Standard delivery','Express delivery','Express delivery']);
    const before=state.image,untouched=(await operation('render',{page:2})).image;
    phase='index selection and multi-widget appearance';
    state=await operation('edit',{operation:{kind:'field',page:0,annotation:selected(state).id,option:2}});
    assert.equal(selected(state).options.find(option=>option.selected).index,2);assert.equal(selected(state).text,'Express delivery');assert.notEqual(state.image,before);
    const other=await operation('render',{page:1});assert.equal(selected(other).text,'Express delivery');assert.equal(selected(other).options.find(option=>option.selected).index,2);
    assert.equal((await operation('render',{page:2})).image,untouched);assert.deepEqual(fs.readFileSync(source),original);
    record('Option index distinguishes duplicate display labels; both linked pages update while the third page and source file stay unchanged');
    phase='undo redo and invalid input';
    state=await operation('undo');assert.equal(state.image,before);assert.equal(selected(state).text,'Standard delivery');
    state=await operation('redo');assert.equal(selected(state).options.find(option=>option.selected).index,2);
    const revision=state.revision;assert.equal((await operation('edit',{operation:{kind:'field',page:0,annotation:selected(state).id,option:999}},422)).error_code,'pdf_invalid_operation');
    assert.equal((await operation('render')).revision,revision);assert.deepEqual(fs.readFileSync(source),original);
    record('Actual workspace Undo/Redo replays the choice as one operation; an invalid option preserves revision, draft and original bytes');
    phase='save and canonical reopen';
    const saved=await operation('save');state={...state,...saved};assert(saved.recovery);assert.deepEqual(fs.readFileSync(path.join(profile,'pdf-history',saved.recovery+'.pdf')),original);
    report.structure=python('inspect',source,'EXP2');
    fs.copyFileSync(source,path.join(args.output,'selected.pdf'));
    await operation('close');state=await pdf({action:'open',path:source,width:600});assert.equal(selected(state).options.find(option=>option.selected).index,2);
    assert.equal(selected(await operation('render',{page:1})).text,'Express delivery');
    fs.writeFileSync(path.join(args.output,'selected.bmp'),Buffer.from(state.image,'base64'));
    record('Explicit save retains exact original recovery; independent pypdf confirms canonical export EXP2, both parent-linked widgets and regenerated AP, then PDFium reopens interactively');
    await operation('close');
    phase='read-only and unsupported form boundaries';
    for(const [file,code] of [['readonly.pdf','pdf_read_only'],['editable.pdf','pdf_unsupported_object'],['list.pdf','pdf_unsupported_object'],['ambiguous.pdf','pdf_unsupported_object']]){
      const filePath=path.join(docs,file),bytes=fs.readFileSync(filePath);state=await pdf({action:'open',path:filePath,width:600});
      if(file==='readonly.pdf')assert.equal(selected(state).readonly,true);else if(file!=='ambiguous.pdf')assert(state.annotations.every(annotation=>annotation.kind!=='choice'));
      assert.equal((await operation('edit',{operation:{kind:'field',page:0,annotation:0,option:2}},422)).error_code,code);
      assert.deepEqual(fs.readFileSync(filePath),bytes);await operation('close');
    }
    record('Inherited read-only fields, editable combo text, list boxes and unrelated same-name widgets without a shared Parent reject writes without changing originals');
    phase='actual PDF panel';
    const {chromium}=require(args.playwright||require.resolve('playwright',{paths:[path.join(root,'ui-tests')]}));
    browser=await chromium.launch({channel:'chrome',headless:true,args:['--disable-background-networking','--disable-component-update','--disable-breakpad']});
    context=await browser.newContext({viewport:{width:1180,height:880},serviceWorkers:'block'});
    await context.route('**/*',route=>new URL(route.request().url()).origin===host.base?route.continue():route.abort());
    const page=await context.newPage();page.setDefaultTimeout(15000);
    const servedBoot=page.waitForResponse(response=>new URL(response.url()).pathname==='/assets/readmd.boot.js');
    await page.goto(host.base);assert.equal(sha(await(await servedBoot).body()),report.bootSha256);await page.waitForFunction(()=>window.__readmdAppReady&&window.ReadMDPdf);
    assert.equal(await page.evaluate(file=>ReadMDPdf.open(file),source),true);
    await page.getByRole('button',{name:'Express delivery',exact:true}).click();
    assert.equal(await page.locator('#pdf-choice').inputValue(),'2');assert(await page.locator('#pdf-text').isHidden());
    await page.locator('#pdf-choice').selectOption('0');await page.locator('#pdf-apply').click();
    await page.getByRole('button',{name:'Standard delivery',exact:true}).waitFor();
    await page.locator('#pdf-undo').click();await page.getByRole('button',{name:'Express delivery',exact:true}).waitFor();
    await page.locator('#pdf-redo').click();await page.getByRole('button',{name:'Standard delivery',exact:true}).waitFor();
    await page.getByRole('button',{name:'Standard delivery',exact:true}).click();
    await page.screenshot({path:path.join(args.output,'choice-panel.png')});
    await page.locator('#pdf-save').click();await until(()=>page.locator('#pdf-save').isDisabled());
    report.afterPanelSave=python('inspect',source,'STD');
    await page.locator('#pdf-close').click();
    record('Real staged PDF panel presents source-owned labels in a select, submits option indices, and completes Apply, Undo, Redo and explicit Save against the managed service');
    assert.equal(sha(fs.readFileSync(pack)),report.packSha256);report.passed=true;
  }catch(error){report.passed=false;report.phase=phase;report.error=String(error.message).replaceAll(args.output,'<owned>');throw error;}
  finally{await context?.close();await browser?.close();if(child.exitCode===null&&child.signalCode===null)child.kill();report.hostExit=await Promise.race([exited,delay(10000).then(()=>({deadline:true}))]);fs.writeFileSync(path.join(args.output,'report.json'),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,phase:report.phase,report:path.join(args.output,'report.json')}));}
}
main().catch(error=>{console.error(String(error.message).slice(0,800));process.exitCode=1;});
