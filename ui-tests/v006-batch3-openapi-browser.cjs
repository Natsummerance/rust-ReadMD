'use strict';
// Real kernel, production CSP and an installed headless browser; all documents
// are synthetic fixtures. This is not a native window or Extension Host test.
const fs=require('node:fs'),path=require('node:path'),net=require('node:net');
const crypto=require('node:crypto'),assert=require('node:assert/strict'),{spawn}=require('node:child_process');
const {chromium}=require('playwright');
const root=path.resolve(__dirname,'..'),base=path.resolve(process.env.READMD_BATCH3_OUTPUT||path.join(root,'.cache/capability-upgrade/v006-batch3-20261027'));
assert.ok(base.startsWith(path.join(root,'.cache')+path.sep));assert.ok(process.env.READMD_BIN);
fs.mkdirSync(base,{recursive:true});const output=fs.mkdtempSync(path.join(base,'openapi-browser-'));
const docs=path.join(output,'documents'),profile=path.join(output,'profile');fs.mkdirSync(docs);fs.mkdirSync(profile);
const api=path.join(docs,'api.yaml'),response=path.join(docs,'response.yaml'),schema=path.join(docs,'schema.json'),file=path.join(docs,'guide.md');
const original='# API guide\n\nAuthor notes remain intact.\n';
fs.writeFileSync(file,original);
fs.writeFileSync(api,"openapi: 3.1.1\ninfo: {title: Reports, version: '1'}\npaths:\n  /reports:\n    get:\n      summary: Read report\n      responses: {'200': {$ref: './response.yaml#/Report'}}\n");
fs.writeFileSync(response,"Report:\n  description: Original report\n  content:\n    application/json:\n      schema: {$ref: './schema.json#/$defs/Report'}\n");
fs.writeFileSync(schema,JSON.stringify({$defs:{Report:{type:'object',properties:{title:{type:'string',minLength:2}}}}}));
const sha=value=>crypto.createHash('sha256').update(value).digest('hex'),sleep=ms=>new Promise(r=>setTimeout(r,ms));
const report={passed:false,synthetic:true,surface:'headless Edge and actual local kernel',nativeWindow:false,extensionHost:false,checks:[],pageErrors:[]};
async function port(){const s=net.createServer();await new Promise(r=>s.listen(0,'127.0.0.1',r));const p=s.address().port;await new Promise(r=>s.close(r));return p;}
async function main(){let child,browser,page,phase='startup';
 try{
  const p=await port(),origin='http://127.0.0.1:'+p;
  child=spawn(path.resolve(process.env.READMD_BIN),['--no-window','--port',String(p),'--data-dir',profile,'--workspace',docs,'--assets',path.join(root,'assets')],{cwd:root,windowsHide:true,stdio:'ignore',env:{...process.env,READMD_DATA_DIR:profile}});
  let deadline=Date.now()+30000;for(;;){if(child.exitCode!==null)throw Error('kernel startup failed');try{if((await fetch(origin)).ok)break;}catch(_){}if(Date.now()>deadline)throw Error('kernel deadline');await sleep(50);}
  browser=await chromium.launch({channel:'msedge',headless:true});const context=await browser.newContext({viewport:{width:1160,height:820},bypassCSP:false});page=await context.newPage();page.on('pageerror',e=>report.pageErrors.push(e.name));await page.goto(origin);
  async function until(predicate,arg){const end=Date.now()+30000;while(!await page.evaluate(predicate,arg)){if(Date.now()>end)throw Error('frontend deadline');await sleep(50);}}
  await until(()=>window.__readmdAppReady&&Boolean(window.ReadMDOpenAPI));
  phase='open-preview';await page.evaluate(async file=>{await loadFile(file);await toggleEdit();},file);await until(()=>state.editing&&Boolean(window.cmView));
  assert.equal(await page.evaluate(api=>ReadMDOpenAPI.generate(api),api),true);
  assert.match(await page.locator('#openapi-review-after').innerText(),/Original report/);
  assert.match(await page.locator('#openapi-review-after').innerText(),/minLength/);
  assert.equal(fs.readFileSync(file,'utf8'),original);report.checks.push('Local dependency content appears in the real review without changing the file');
  phase='apply-undo-save';await page.locator('#openapi-review-apply').click();await until(()=>document.querySelector('#openapi-review-modal').classList.contains('hidden'));
  const generated=await page.evaluate(()=>cmView.state.doc.toString());assert.ok(generated.startsWith(original));
  await page.keyboard.press('Control+z');await until(text=>cmView.state.doc.toString()===text,original);
  await page.keyboard.press('Control+Shift+Z');await until(text=>cmView.state.doc.toString()===text,generated);
  assert.equal(await page.evaluate(()=>saveEdit()),true);assert.equal(fs.readFileSync(file,'utf8'),generated);
  await page.evaluate(file=>{exitEdit();return loadFile(file,{force:true});},file);assert.equal(await page.evaluate(()=>getActiveTab().content),generated);
  report.checks.push('Accept, one undo/redo, explicit save and actual reopen preserve the generated region');
  phase='stale-dependency';await page.evaluate(()=>toggleEdit());await until(()=>state.editing&&Boolean(window.cmView));
  assert.equal(await page.evaluate(api=>ReadMDOpenAPI.generate(api),api),true);
  fs.writeFileSync(schema,JSON.stringify({$defs:{Report:{type:'object',properties:{title:{type:'string',minLength:8}}}}}));
  for(const language of ['en','zh-CN','ar']){
    await page.evaluate(language=>i18n.setLanguage(language,false),language);
    assert.equal(await page.evaluate(()=>ReadMDOpenAPI.accept()),false);
    assert.equal(await page.locator('#openapi-review-status').innerText(),await page.evaluate(()=>i18n.t('error.review_stale_generation')));
    assert.equal(await page.evaluate(()=>cmView.state.doc.toString()),generated);assert.equal(fs.readFileSync(file,'utf8'),generated);
  }
  const locales=path.join(root,'assets/i18n'),english=JSON.parse(fs.readFileSync(path.join(locales,'en.json'),'utf8'));
  const dictionaries=fs.readdirSync(locales).filter(name=>name.endsWith('.json')&&name!=='meta.json');assert.equal(dictionaries.length,46);
  for(const name of dictionaries){const value=JSON.parse(fs.readFileSync(path.join(locales,name),'utf8'))['error.review_stale_generation'];assert.ok(value&&value!=='error.review_stale_generation');if(name!=='en.json')assert.notEqual(value,english['error.review_stale_generation'],name);}
  await page.locator('#openapi-review-cancel').click();
  report.checks.push('A dependency-only change rejects the reviewed proposal and preserves editor and disk');
  assert.deepEqual(report.pageErrors,[]);report.binarySha256=sha(fs.readFileSync(process.env.READMD_BIN));report.bootSha256=sha(fs.readFileSync(path.join(root,'assets/readmd.boot.js')));report.passed=true;
 }catch(e){report.failedStage=phase;report.error=String(e.message).replaceAll(root,'<workspace>').slice(0,500);process.exitCode=1;}
 finally{if(browser)await browser.close().catch(()=>{});if(child&&child.exitCode===null){const done=new Promise(r=>child.once('exit',r));child.kill();await Promise.race([done,sleep(3000)]);}fs.writeFileSync(path.join(output,'report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,stage:report.failedStage,report:path.relative(root,path.join(output,'report.json'))}));}
}
main().catch(()=>{console.error('OpenAPI browser verification failed');process.exitCode=1;});
