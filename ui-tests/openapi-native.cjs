'use strict';
// Real Windows WebView2 and Rust service; fixture-only source/target documents.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const {chromium}=require('playwright'),{startNative}=require('./native-session.cjs');
const root=path.resolve(__dirname,'..'),data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/openapi-native-')),docs=path.join(data,'documents');fs.mkdirSync(docs);
const source=path.join(docs,'研究接口.yaml'),target=path.join(docs,'接口说明.md');
const specification=`openapi: 3.1.1
info: {title: 研究资料 API, version: '1.0'}
servers: [{url: 'https://example.test/v1'}]
security: [{bearer: []}]
paths:
  /notes/{id}:
    parameters: [{name: id, in: path, required: true, schema: {type: string}}]
    get:
      summary: 读取笔记
      operationId: readNote
      responses:
        200:
          description: 返回保留出处的研究笔记
          content:
            application/json:
              schema: {$ref: '#/components/schemas/Note'}
    patch:
      summary: 校订笔记
      requestBody:
        content:
          application/json:
            schema: {$ref: '#/components/schemas/Note'}
      responses: {204: {description: Saved}}
components:
  schemas:
    Note:
      type: object
      required: [text]
      properties:
        text: {type: string, description: 原始论证和可核对的来源}
        reviewed: {type: boolean, default: false}
  securitySchemes:
    bearer: {type: http, scheme: bearer}
`;
const original='# 接口校订手册\n\n😀 开放资料 · 保留来源与研究记录。\n\n| 阶段 | 操作 |\n| --- | --- |\n| 观察 | 核对证据 |\n| 修订 | 保留原稿 |\n\n## 手工说明\n\n此段应始终保留。\n';
fs.writeFileSync(source,specification);fs.writeFileSync(target,original);
const hash=text=>crypto.createHash('sha256').update(text).digest('hex'),checks=[],report={platform:'windows-x64',nativeCsp:true,synthetic:true,checks};let native,phase='startup';
const delay=ms=>new Promise(r=>setTimeout(r,ms));async function until(fn,ms=30000){const deadline=Date.now()+ms;while(Date.now()<deadline){if(await fn())return;await delay(50);}throw Error('OpenAPI native operation timed out');}
(async()=>{try{
  report.binarySha256=hash(fs.readFileSync(process.env.READMD_BIN));process.env.READMD_WORKSPACE=docs;native=await startNative(root,data,chromium,28898);const page=native.page;
  const api=payload=>page.evaluate(async payload=>{const response=await apiFetch('/api/developer/openapi',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(payload)});return{status:response.status,data:await response.json()};},payload);
  await page.evaluate(async target=>{await i18n.setLanguage('zh-CN',false);await loadFile(target);},target);
  phase='cancel without mutation';const tabs=await page.evaluate(()=>state.tabs.length);assert.equal(await page.evaluate(source=>ReadMDOpenAPI.generate(source),source),true);assert.ok((await page.locator('#openapi-review-after').innerText()).includes('请求参数'));assert.ok(!(await page.locator('#openapi-review-after').innerText()).includes('readmd:openapi:start'));await page.locator('#openapi-review-cancel').click();assert.equal(await page.evaluate(()=>state.tabs.length),tabs);assert.equal(await page.evaluate(()=>state.editing),false);assert.equal(fs.readFileSync(target,'utf8'),original);checks.push({name:'real contextual review is localized; cancellation creates no document, copy or dirty state',passed:true});
  phase='accept and undo';await page.evaluate(async()=>{await toggleEdit();setEditorPref('aiCompletion',false);});await page.evaluate(source=>ReadMDOpenAPI.generate(source),source);await page.locator('#openapi-review-apply').click();await until(()=>page.locator('#openapi-review-modal').isHidden());const generated=await page.evaluate(()=>getEditContent());assert.ok(generated.startsWith(original));assert.ok(generated.includes('readmd:openapi:start'));assert.ok(!generated.includes(data));assert.equal(fs.readFileSync(target,'utf8'),original);
  await page.keyboard.press('Control+z');assert.equal(await page.evaluate(()=>getEditContent()),original);await page.keyboard.press('Control+y');assert.equal(await page.evaluate(()=>getEditContent()),generated);checks.push({name:'acceptance applies one independently undoable Rust transaction without writing the disk original',passed:true});
  phase='save reopen and idempotence';assert.equal(await page.evaluate(()=>saveEdit()),true);await page.evaluate(async target=>{await exitEdit();await loadFile(target,{force:true});await toggleEdit();setEditorPref('aiCompletion',false);},target);assert.equal(await page.evaluate(source=>ReadMDOpenAPI.generate(source),source),true);assert.equal(await page.locator('#openapi-review-apply').isDisabled(),true);await page.locator('#openapi-review-cancel').click();assert.deepEqual(fs.readdirSync(docs).sort(),['研究接口.yaml','接口说明.md'].sort());checks.push({name:'explicit atomic save, real reopen and repeated generation preserve one region without sibling files',passed:true});
  phase='operation changes and outside prose';const notes='\n\n## 作者笔记\n外部段落应保留。\n';await page.evaluate(notes=>cmView.dispatch({changes:{from:cmView.state.doc.length,insert:notes},userEvent:'input.type'}),notes);const prior=await page.evaluate(()=>getEditContent());const revised=specification.replace('读取笔记','读取并核对笔记');fs.writeFileSync(source,revised);assert.equal(await page.evaluate(source=>ReadMDOpenAPI.generate(source),source),true);assert.ok((await page.locator('#openapi-review-changes').innerText()).includes('更新'));await page.locator('#openapi-review-apply').click();await until(()=>page.locator('#openapi-review-modal').isHidden());assert.ok((await page.evaluate(()=>getEditContent())).endsWith(notes));await page.keyboard.press('Control+z');assert.equal(await page.evaluate(()=>getEditContent()),prior);checks.push({name:'operation-aware update preserves surrounding author notes and one undo restores the previous generated region',passed:true});
  phase='stale source and document';assert.equal(await page.evaluate(source=>ReadMDOpenAPI.generate(source),source),true);fs.writeFileSync(source,revised.replace("version: '1.0'","version: '1.1'"));assert.equal(await page.evaluate(()=>ReadMDOpenAPI.accept()),false);assert.equal(await page.evaluate(()=>getEditContent()),prior);assert.ok((await page.locator('#openapi-review-status').innerText()).includes(await page.evaluate(()=>i18n.t('error.transaction_conflict'))));await page.locator('#openapi-review-cancel').click();fs.writeFileSync(source,revised);
  assert.equal(await page.evaluate(source=>ReadMDOpenAPI.generate(source),source),true);await page.evaluate(()=>cmView.dispatch({changes:{from:cmView.state.doc.length,insert:'\n新的本地修改'},userEvent:'input.type'}));const changed=await page.evaluate(()=>getEditContent());assert.equal(await page.evaluate(()=>ReadMDOpenAPI.accept()),false);assert.equal(await page.evaluate(()=>getEditContent()),changed);await page.locator('#openapi-review-cancel').click();checks.push({name:'source-file changes and concurrent document changes both reject the reviewed proposal without mutation',passed:true});
  phase='manual generation protection';await page.evaluate(text=>cmView.dispatch({changes:{from:0,to:cmView.state.doc.length,insert:text}}),prior.replace('读取笔记','手工修改'));const manual=await page.evaluate(()=>getEditContent());assert.equal(await page.evaluate(source=>ReadMDOpenAPI.generate(source),source),false);assert.equal(await page.evaluate(()=>getEditContent()),manual);checks.push({name:'manual edits inside the generated region cannot be silently overwritten',passed:true});
  phase='native layouts';await page.evaluate(text=>cmView.dispatch({changes:{from:0,to:cmView.state.doc.length,insert:text}}),prior);assert.equal(await page.evaluate(source=>ReadMDOpenAPI.generate(source),source),true);const cdp=await page.context().newCDPSession(page);
  for(const[width,height]of[[1024,680],[1160,820]]){await cdp.send('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false});for(const language of['zh-CN','en','ar']){await page.evaluate(language=>i18n.setLanguage(language,false),language);const button=await page.locator('#openapi-review-apply').boundingBox();assert.ok(button&&button.height>=44&&button.y+button.height<=height+1);assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));}await page.evaluate(()=>i18n.setLanguage('zh-CN',false));await page.screenshot({path:path.join(data,'review-'+width+'.png')});}await cdp.detach();await page.locator('#openapi-review-cancel').click();checks.push({name:'1024 and 1160 native review layout, translated controls and RTL switch keep 44px actions accessible',passed:true});
  phase='same filenames and bounded concurrency';const other=path.join(docs,'other');fs.mkdirSync(other);const second=path.join(other,'研究接口.yaml');fs.writeFileSync(second,specification);let result=await api({file_path:second,content:generated,base_revision:hash(generated),locale:'zh-CN'});assert.equal(result.status,200);assert.equal((result.data.result.content.match(/<!-- readmd:openapi:start /g)||[]).length,2);
  const many=JSON.stringify({openapi:'3.1.1',info:{title:'Load fixture',version:'1'},paths:Object.fromEntries(Array.from({length:600},(_,i)=>['/notes/'+i,{get:{summary:'Evidence review '+i,responses:{200:{description:'A reviewed research note with original references'}}}}]))});
  const began=Date.now(),load=await Promise.all(Array.from({length:8},()=>api({source:many,source_name:'load.json',content:'',base_revision:hash(''),locale:'zh-CN'})));assert.ok(load.some(r=>r.status===200));assert.ok(load.every(r=>r.status===200||r.status===429));report.load={requests:load.length,completed:load.filter(r=>r.status===200).length,busy:load.filter(r=>r.status===429).length,elapsedMs:Date.now()-began};assert.equal(fs.readFileSync(target,'utf8'),generated);checks.push({name:'same basename sources remain distinct; concurrent 600-operation documents are admitted or rejected within the bounded service',passed:true});
  report.passed=true;
}catch(error){report.passed=false;report.failedStage=phase;report.message=String(error.message).replaceAll(data,'<profile>').replaceAll(root,'<workspace>').slice(0,1000);if(native)await native.page.screenshot({path:path.join(data,'failure.png')}).catch(()=>{});throw error;
}finally{if(native){await native.browser.close().catch(()=>{});native.server.child.kill();}report.recordedAt=new Date().toISOString();fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,checks:checks.length,report:path.relative(root,path.join(data,'report.json')),failedStage:report.failedStage}));}
})().catch(()=>{console.error('OpenAPI native validation failed; stage recorded.');process.exitCode=1;});
