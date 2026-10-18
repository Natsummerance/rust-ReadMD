'use strict';
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const {chromium}=require('playwright'),{startNative}=require('./native-session.cjs'),zipParts=require('./zip-parts.cjs');
const root=path.resolve(__dirname,'..'),data=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/math-dsl-native-')),docs=path.join(data,'documents');fs.mkdirSync(docs);
const file=path.join(docs,'Research-workshop.md'),hash=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const report={platform:'windows-x64',nativeCsp:true,synthetic:true,viewportEmulation:true,wordRoundTrip:'unverified',checks:[],passed:false};let native,phase='startup';
async function until(check){const limit=Date.now()+45000;while(!await check()){if(Date.now()>limit)throw Error('Timed out: '+phase);await new Promise(r=>setTimeout(r,75));}}
const source='# 从观察到论证\n\n## 研究记录 · 可复核的统计推理\n\n> [!NOTE]\n> 计算不能替代判断。先明确样本、假设与证据，再讨论结论的适用范围。\n\n| 研究组 | 样本数 | 平均得分 | 标准差 |\n|---|---:|---:|---:|\n| 观察组 A | 48 | 0.84 | 0.11 |\n| 对照组 B | 52 | 0.76 | 0.14 |\n\n## 一、检查估计量\n\n第一次计算 :math[frac(α, β, style: stacked)]。第二次校订 :math[frac(α, β, style: stacked)]。重复公式应独立编辑。\n\n```readmd-math v1\n// 样本均值：保留研究者的说明\nfrac(sum(x_i, from: i = 1, to: n, limits: above_below), n)\n```\n\n## 二、比较矩阵与精度\n\n```readmd-math v1\nmatrix([48, 0.84], [52, 0.76], align: [right, right], column_gap: 2em)\n```\n\n误差传播 :math[root(2, superscript(σ, 2))]，定义域 :math[α <= β]。\n\n## 三、作者确认\n\n- [x] 保留原始观察\n- [x] 明确公式的修改范围\n- [ ] 检查 Word、PDF 与电子书输出\n\n```markdown\n:math[frac(a, b)]\n```\n';
(async()=>{try{
  process.env.READMD_WORKSPACE=docs;fs.writeFileSync(file,source);report.binarySha256=hash(process.env.READMD_BIN);
  report.assets=Object.fromEntries(['assets/readmd.boot.js','assets/css/panels.css','assets/vendor/reveal/dist/readmd-boot.js'].map(f=>[f,hash(path.join(root,f))]));
  native=await startNative(root,data,chromium,28933);const page=native.page,errors=[];await page.evaluate(()=>Promise.all([window.__readmdSessionReady,window.__readmdPreferencesReady]));page.on('pageerror',e=>errors.push(e.message));
  const api=(route,body)=>page.evaluate(async({route,body})=>{const response=await apiFetch(route,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});return {status:response.status,data:await response.json()};},{route,body});
  phase='native reading and editing';await page.evaluate(async file=>{await i18n.setLanguage('en',false);await loadFile(file);await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');},file);
  await until(async()=>await page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"]').count()===6);
  assert.equal(await page.locator('#preview-pane [data-mml-node="merror"]').count(),0);
  // Chromium can defer offscreen layout; inspect the actual visible example,
  // rather than interpreting innerText from a skipped subtree as lost source.
  await page.locator('#preview-pane code').scrollIntoViewIfNeeded();await until(async()=>await page.locator('#preview-pane code').innerText()===':math[frac(a, b)]');
  assert.equal(await page.locator('#preview-pane code').innerText(),':math[frac(a, b)]');
  phase='exact repeated slot';await page.locator('#preview-pane .readmd-dsl-math [data-readmd-math-renderer]').nth(1).click();await until(()=>page.locator('.formula-inline-controls button').first().isEnabled());
  await page.locator('.formula-inline').getByRole('button',{name:'Denominator',exact:true}).click();await page.locator('.formula-inline-input').fill('θ + 1');await page.locator('.formula-inline-input').press('Enter');await until(()=>page.locator('.formula-inline-controls button').first().isEnabled());
  assert.equal(await page.evaluate(()=>getEditContent()),source);assert.equal(fs.readFileSync(file,'utf8'),source);
  await page.locator('.formula-inline-controls button').first().click();const changed=source.replace('第二次校订 :math[frac(α, β, style: stacked)]','第二次校订 :math[frac(α, θ + 1, style: stacked)]');
  assert.equal(await page.evaluate(()=>getEditContent()),changed);assert.equal(await page.evaluate(()=>cmView.hasFocus),true);await page.keyboard.press('Control+z');assert.equal(await page.evaluate(()=>getEditContent()),source);await page.keyboard.press('Control+y');assert.equal(await page.evaluate(()=>getEditContent()),changed);
  report.checks.push('native repeated DSL formulas edit by exact UTF-16 slot identity and undo/redo as one body change');
  phase='readable advanced property';await until(()=>page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===6));
  await page.evaluate(()=>mathTypesetQueue);assert.equal(await page.locator('#preview-pane mjx-container mjx-container').count(),0);assert.equal(await page.locator('#preview-pane .readmd-dsl-math [data-readmd-math-renderer]').count(),6);
  await page.locator('#preview-pane .readmd-dsl-math [data-readmd-math-renderer]').nth(1).click();await until(()=>page.locator('.formula-inline-controls button').first().isEnabled());await page.locator('.formula-properties > summary').click();await page.locator('[data-math-container="fPr"] > summary').click();await page.locator('[data-math-property="fPr.type"] select').selectOption('skw');await until(()=>page.locator('.formula-inline-controls button').first().isEnabled());await page.locator('.formula-inline-controls button').first().click();
  const reviewed=changed.replace('frac(α, θ + 1, style: stacked)','frac(α, θ + 1, style: skewed)');assert.equal(await page.evaluate(()=>getEditContent()),reviewed);assert.ok(!reviewed.includes('<!--readmd:math:'));
  phase='actual save and reopen';assert.equal(await page.evaluate(()=>saveEdit()),true);assert.equal(fs.readFileSync(file,'utf8'),reviewed);assert.deepEqual(fs.readdirSync(docs),['Research-workshop.md']);
  await page.evaluate(async file=>{await exitEdit();await loadFile(file,{force:true,discardConfirmed:true});},file);await until(()=>page.locator('#content .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===6));assert.equal(await page.evaluate(()=>hasUnsavedEditorChanges()),false);
  report.checks.push('advanced Office property stays readable; atomic native save/reopen preserves DSL, comments and clean source directory');
  await page.locator('#content .readmd-dsl-math').first().scrollIntoViewIfNeeded();await page.screenshot({path:path.join(data,'formula-reading.png')});
  phase='responsive native chrome';const cdp=await page.context().newCDPSession(page);
  for(const[width,height,language]of[[1024,680,'zh-CN'],[1160,820,'ar']]){
    await cdp.send('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false});await page.evaluate(l=>i18n.setLanguage(l,false),language);
    assert.equal(await page.evaluate(()=>i18n.currentLang),language);assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));await page.screenshot({path:path.join(data,'reading-'+width+'.png')});
    await page.locator('#rd-prefs-btn').click();await until(()=>page.locator('#rd-prefs').isVisible());await page.locator('#rd-prefs').evaluate(async e=>{await Promise.all(e.getAnimations({subtree:true}).map(a=>a.finished.catch(()=>{})));});
    const rect=await page.locator('#rd-prefs').boundingBox();assert.ok(rect.x>=0&&rect.x+rect.width<=width+1);await page.screenshot({path:path.join(data,'preferences-'+width+'.png')});await page.keyboard.press('Escape');
  }
  await cdp.send('Emulation.clearDeviceMetricsOverride');await cdp.detach();await page.evaluate(()=>i18n.setLanguage('en',false));
  report.checks.push('1024×680 Chinese and 1160×820 RTL native reading/preferences have no horizontal overflow');
  phase='required CJK fonts and existing-output protection';assert.ok(process.env.READMD_FONTS_PACK,'Explicit locked CJK font pack required');
  const protectedOutput=path.join(docs,'Research-workshop.pdf'),oldOutput=Buffer.from('Previous completed PDF');fs.writeFileSync(protectedOutput,oldOutput);
  const unavailable=await api('/api/export',{format:'pdf',content:reviewed,baseDir:docs,options:{pdf:{engine:'rust'}},out_path:protectedOutput});
  assert.equal(unavailable.data.ok,false,JSON.stringify(unavailable.data));assert.equal(unavailable.data.error_code,'publication_fonts_unavailable');assert.deepEqual(fs.readFileSync(protectedOutput),oldOutput);
  const fonts=await api('/api/capabilities/packs',{action:'install',id:'fonts',path:process.env.READMD_FONTS_PACK});assert.equal(fonts.data.ok,true,JSON.stringify(fonts.data));
  report.checks.push('A fresh profile reports the actual CJK font requirement and preserves an existing PDF; installing the locked offline font pack enables the same output.');
  phase='real OfficeMath exports';const outputs={};for(const format of ['docx','epub','pdf']){
    const output=path.join(docs,'Research-workshop.'+format),result=await api('/api/export',{format,content:reviewed,baseDir:docs,options:{pdf:{engine:'rust'}},out_path:output});assert.equal(result.status,200,JSON.stringify(result.data));assert.equal(result.data.ok,true,JSON.stringify(result.data));assert.ok(fs.statSync(output).size>1000);outputs[format]=output;
  }
  const word=zipParts(fs.readFileSync(outputs.docx)).get('word/document.xml');assert.ok(word.includes('<m:f>'));assert.ok(word.includes('m:val="skw"'));assert.ok(word.includes('<m:m>'));assert.ok(word.includes('m:val="right"'));assert.ok(!word.includes('readmd:math:v1:'));
  const epub=[...zipParts(fs.readFileSync(outputs.epub))].filter(([n])=>n.endsWith('.xhtml')).map(([,v])=>v).join('');assert.ok(epub.includes('<math'));assert.ok(epub.includes('bevelled="true"'));assert.ok(epub.includes('columnspacing="2em"'));assert.ok(epub.includes('columnalign="right right"'));
  report.checks.push('actual DOCX contains editable native OMML; EPUB contains MathML with skewed fractions, matrix alignment and exact gap units');
  phase='actual exported PDF display';const installed=await api('/api/capabilities/packs',{action:'install',path:process.env.READMD_PDF_PACK});assert.equal(installed.status,200);assert.equal(installed.data.installed,true);assert.equal(await page.evaluate(file=>ReadMDPdf.open(file),outputs.pdf),true);await until(()=>page.locator('#pdf-image').evaluate(i=>i.complete&&i.naturalHeight>0));await page.screenshot({path:path.join(data,'exported-pdf.png')});await page.locator('#pdf-viewport').evaluate(async e=>{e.scrollTop=650;await new Promise(requestAnimationFrame);await new Promise(requestAnimationFrame);});assert.ok(await page.locator('#pdf-viewport').evaluate(e=>e.scrollTop>0));await page.screenshot({path:path.join(data,'exported-pdf-math.png')});await page.locator('#pdf-close').click();
  assert.equal(fs.readFileSync(file,'utf8'),reviewed);report.checks.push('actual vector PDF reopens through installed PDFium in Windows WebView2; source remains unchanged');
  phase='common structural slot edits';
  const structures=path.join(docs,'Common-structures.md');
  for(const [name,expression,label,value,expected] of [
    ['root','root(3, x)','Root index','4','root(4, x)'],
    ['matrix','matrix([a, b], [c, d])','Edit symbol b','β','matrix([a, β], [c, d])'],
    ['superscript','superscript(x, 2)','Edit symbol 2','3','superscript(x, 3)'],
    ['subscript','subscript(x, i)','Edit symbol i','j','subscript(x, j)'],
    ['sum','sum(x, from: i = 1, to: n)','Edit symbol n','m','sum(x, from: i = 1, to: m)']
  ]){
    phase='native '+name+' slot';const original='# Editable '+name+'\n\n```readmd-math v1\n'+expression+'\n```\n\nKeep the author paragraph.\n';
    await page.evaluate(async()=>{if(state.editing)await exitEdit();});fs.writeFileSync(structures,original);
    await page.evaluate(async file=>{await loadFile(file,{force:true,discardConfirmed:true});await toggleEdit();setPvLayout('right');},structures);
    await until(()=>page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===1));
    await page.locator('#preview-pane [data-readmd-math-renderer]').click();await until(()=>page.locator('.formula-inline-controls button').first().isEnabled());
    await page.locator('.formula-inline-canvas').getByRole('button',{name:label,exact:true}).click();
    await page.locator('.formula-inline-input').fill(value);await page.locator('.formula-inline-input').press('Enter');await until(()=>page.locator('.formula-inline-controls button').first().isEnabled());
    assert.equal(await page.evaluate(()=>getEditContent()),original);await page.locator('.formula-inline-controls button').first().click();
    const changed=original.replace(expression,expected);assert.equal(await page.evaluate(()=>getEditContent()),changed);
    await page.keyboard.press('Control+z');assert.equal(await page.evaluate(()=>getEditContent()),original);await page.keyboard.press('Control+y');
    assert.equal(await page.evaluate(()=>saveEdit()),true);assert.equal(fs.readFileSync(structures,'utf8'),changed);
    await page.evaluate(async file=>{await exitEdit();await loadFile(file,{force:true,discardConfirmed:true});},structures);
    await until(()=>page.locator('#content .readmd-dsl-math[data-readmd-ready="true"]').count().then(n=>n===1));
    report.checks.push('Actual '+name+' slot selection, Greek/text replacement, one undo/redo, atomic save and reopen preserve the surrounding author source.');
  }
  assert.deepEqual(errors,[]);report.passed=true;
}catch(error){report.failedStage=phase;report.error=error.name;report.message=String(error.message).replaceAll(root,'<workspace>').replaceAll(data,'<test-profile>').slice(0,700);if(native)await native.page.screenshot({path:path.join(data,'failure.png')}).catch(()=>{});throw error;}
finally{if(native){await native.browser.close().catch(()=>{});native.server.child.kill();}report.recordedAt=new Date().toISOString();fs.writeFileSync(path.join(data,'report.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,report:path.relative(root,path.join(data,'report.json')),failedStage:report.failedStage}));}
})().catch(()=>{console.error('Native DSL failed; evidence retained.');process.exitCode=1;});
