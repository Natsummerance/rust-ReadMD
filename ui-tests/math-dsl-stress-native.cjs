'use strict';
// Opt-in native capacity check, not the eight-hour soak or full performance
// certification. Only generated documents and an isolated profile are used.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {chromium}=require('playwright'),{startNative}=require('./native-session.cjs');
const root=path.resolve(__dirname,'..'),data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/math-stress-native-'));
const docs=path.join(data,'documents');fs.mkdirSync(docs);
const file=path.join(docs,'Research-atlas.md'),sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const examples=i=>[
  `frac(α + ${i}, n + 1, style: stacked)`,
  `matrix([${i}, 0.84], [52, 0.76], align: [right, right], column_gap: 2em)`,
  `sum(x_i, from: i = 1, to: ${i+1}, limits: above_below)`,
  `root(3, superscript(x, 2) + ${i})`,
][i%4];
const source='# 研究图谱 · 一千条可复核推导\n\n'+Array.from({length:1000},(_,i)=>
  `## ${i+1} · 样本、估计与证据\n\n观察不是结论。记录样本与假设，比较不同解释，并保留可重复计算的过程。\n\n`+
  '```readmd-math v1\n'+`// 研究记录 ${i+1}\n`+examples(i)+'\n```\n\n').join('');
const report={platform:'windows-x64',nativeCsp:true,syntheticOnly:true,formulaCount:1000,sourceBytes:Buffer.byteLength(source),eightHourSoak:false,checks:[],passed:false};
let native,phase='startup';
async function until(fn){const end=Date.now()+90000;while(!await fn()){if(Date.now()>end)throw Error('Timed out: '+phase);await new Promise(r=>setTimeout(r,100));}}
(async()=>{try{
  fs.writeFileSync(file,source);process.env.READMD_WORKSPACE=docs;report.binarySha256=sha(fs.readFileSync(process.env.READMD_BIN));report.bootSha256=sha(fs.readFileSync(path.join(root,'assets/readmd.boot.js')));
  native=await startNative(root,data,chromium,28943);const page=native.page,errors=[];await page.evaluate(()=>window.__readmdSessionReady);
  page.on('pageerror',e=>errors.push(e.message));let active=0,maximum=0,requests=0;
  page.on('request',r=>{if(r.url().endsWith('/api/math/parse')){active++;requests++;maximum=Math.max(maximum,active);}});
  const finished=r=>{if(r.url().endsWith('/api/math/parse'))active--;};page.on('requestfinished',finished);page.on('requestfailed',finished);
  const checkpoint=name=>{report.checks.push({name,compilerRequests:requests,maximumCompilerConcurrency:maximum});};
  phase='thousand formula reading';const started=Date.now();await page.evaluate(file=>loadFile(file),file);
  report.pagination=await page.evaluate(()=>({enabled:state.pagination.enabled,mode:state.pagination.mode,pages:state.pagination.totalPages}));
  assert.equal(report.pagination.enabled,true);assert.ok(report.pagination.pages>1);
  await until(()=>page.locator('#content .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n>0));
  // The fixture exceeds the normal pagination line threshold. Exercise its
  // real full-book action rather than counting all pages inside page one.
  phase='full-book confirmation';await page.locator('#pg-mode-toggle').click();
  if(report.pagination.pages>20)await page.locator('#continuous-confirm').click();
  phase='thousand formula continuous reading';
  await until(()=>page.locator('#content .readmd-dsl-math').count().then(n=>n===1000));
  await until(()=>page.locator('#content .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n>0));
  report.firstFormulaMs=Date.now()-started;report.initialRendered=await page.locator('#content .readmd-dsl-math[data-readmd-ready="true"]').count();
  assert.ok(report.initialRendered<1000);assert.equal(sha(fs.readFileSync(file)),sha(source));
  for(const index of [500,999,0]){
    const formula=page.locator('#content .readmd-dsl-math').nth(index);await formula.scrollIntoViewIfNeeded();
    await until(()=>formula.getAttribute('data-readmd-ready').then(v=>v==='true'));assert.equal(await formula.locator('[data-mml-node="merror"]').count(),0);
  }
  assert.ok(maximum<=4);report.maximumBackgroundCompilerConcurrency=maximum;
  checkpoint('1000 real DSL expressions in a native file; first/middle/last viewport formulas render without touching disk');
  phase='edit under formula load';await page.evaluate(async()=>{await i18n.setLanguage('en',false);await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');});
  const first=page.locator('#preview-pane .readmd-dsl-math').first();await first.scrollIntoViewIfNeeded();await until(()=>first.getAttribute('data-readmd-ready').then(v=>v==='true'));
  await first.locator('[data-readmd-math-renderer]').click();await until(()=>page.locator('.formula-inline-controls button').first().isEnabled());
  const denominator=page.locator('.formula-inline').getByRole('button',{name:'Denominator',exact:true});await denominator.press('Enter');assert.equal(await page.locator('.formula-inline-input').inputValue(),'n + 1');await page.locator('.formula-inline-input').fill('n + 2');await page.locator('.formula-inline-input').press('Enter');
  await until(()=>page.locator('.formula-inline-controls button').first().isEnabled());await page.locator('.formula-inline-controls button').first().click();
  const changed=source.replace(examples(0),'frac(α + 0, n + 2, style: stacked)');assert.equal(await page.evaluate(()=>getEditContent()),changed);
  await page.keyboard.press('Control+z');assert.equal(await page.evaluate(()=>getEditContent()),source);await page.keyboard.press('Control+y');assert.equal(await page.evaluate(()=>getEditContent()),changed);
  phase='save and reopen';assert.equal(await page.evaluate(()=>saveEdit()),true);assert.equal(fs.readFileSync(file,'utf8'),changed);
  await page.evaluate(async file=>{await exitEdit();await loadFile(file,{force:true,discardConfirmed:true});},file);
  await until(()=>page.locator('#content .readmd-dsl-math').count().then(n=>n===1000));
  assert.deepEqual(fs.readdirSync(docs),['Research-atlas.md']);
  checkpoint('exact slot editing, one-step undo/redo, atomic save and reopen preserve all 1000 formulas and source comments');
  // The focused slot editor has one interactive parse in addition to the
  // bounded background renderer. Do not conflate it with renderer workers.
  report.compilerRequests=requests;report.maximumCompilerConcurrency=maximum;assert.ok(maximum<=5);assert.deepEqual(errors,[]);
  assert.equal(await page.locator('mjx-container mjx-container').count(),0);
  checkpoint('background parsing stays within 4 plus at most one focused editor request; no nested SVG or JavaScript errors');report.passed=true;
}catch(error){report.failedStage=phase;report.error=error.name;report.message=String(error.message).replaceAll(root,'<workspace>').replaceAll(data,'<test-profile>').slice(0,700);throw error;}
finally{if(native){await native.browser.close().catch(()=>{});const child=native.server.child;if(child.exitCode===null){const exited=new Promise(resolve=>child.once('exit',resolve));child.kill();await Promise.race([exited,new Promise(resolve=>setTimeout(resolve,5000))]);}}
  report.recordedAt=new Date().toISOString();fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,report:path.relative(root,path.join(data,'report.json')),failedStage:report.failedStage}));}
})().catch(()=>{console.error('Native math capacity check failed; evidence retained.');process.exitCode=1;});
