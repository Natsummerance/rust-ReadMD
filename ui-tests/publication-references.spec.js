const {test,expect}=require('@playwright/test');
const {waitForPage}=require('./page-condition.cjs');
const fs=require('node:fs'),path=require('node:path'),os=require('node:os');
const zipParts=require('./zip-parts.cjs');
test.use({bypassCSP:false});
let folder,file;
const source='---\ntitle: Evidence workshop\nlang: zh-CN\n---\n# 从观察到结论\n\n参见 :ref[eq:second]、:ref[fig:route] 与 :ref[tbl:evidence]。\n\n```readmd-math v1\n# Keep this author comment\nfrac(a, b)\n```\n\n:label[eq:first] 初始假设\n\n```readmd-math v1\nfrac(a, b)\n```\n\n:label[eq:second] **更新后的假设**\n\n![证据路线](route.svg)\n\n:label[fig:route] 阅读与检验\n\n| 观察 | 证据 |\n|---|---|\n| 独立复核 | 保留来源 |\n\n:label[tbl:evidence] 复核记录\n\n代码示例 `:ref[eq:first]` 保持不变。\n';
test.beforeEach(async({page})=>{
  folder=fs.mkdtempSync(path.join(os.tmpdir(),'readmd-publication-reference-'));file=path.join(folder,'Workshop.md');
  fs.writeFileSync(file,source);fs.writeFileSync(path.join(folder,'route.svg'),'<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="400"><rect width="1200" height="400" fill="#eef0ff"/><text x="70" y="200" font-size="64">Observe → Compare → Revise</text></svg>');
  await page.goto('/');await waitForPage(page,()=>window.__readmdAppReady);
  await page.evaluate(async file=>{await i18n.setLanguage('zh-CN',false);await loadFile(file);},file);
  await expect(page.locator('#content .publication-caption')).toHaveCount(4);
});
test.afterEach(()=>fs.rmSync(folder,{recursive:true,force:true}));
async function edit(page){await page.evaluate(async()=>{await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');});await expect(page.locator('#preview-pane .publication-caption')).toHaveCount(4);}
test('stable object references render, jump within their own pane and never change author source',async({page})=>{
  await expect(page.locator('#content a[href="#eq:second"]')).toHaveText('公式 2');
  await expect(page.locator('#content a[href="#fig:route"]')).toHaveText('图 1');
  await page.locator('#content a[href="#eq:second"]').click();
  expect(await page.locator('#content [id="eq:second"]').evaluate(el=>el===document.activeElement)).toBe(true);
  await edit(page);await page.locator('#preview-pane a[href="#tbl:evidence"]').click();
  expect(await page.locator('#preview-pane [id="tbl:evidence"]').evaluate(el=>el===document.activeElement)).toBe(true);
  await expect(page.locator('#preview-pane code').filter({hasText:':ref[eq:first]'})).toHaveCount(1);
  expect(await page.evaluate(()=>getEditContent())).toBe(source);expect(fs.readFileSync(file,'utf8')).toBe(source);
  expect(fs.readdirSync(folder).sort()).toEqual(['Workshop.md','route.svg']);
});
test('numbering projection cannot redirect an edit to the first identical formula',async({page})=>{
  await edit(page);await expect(page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"]')).toHaveCount(2);
  await page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"] [data-readmd-math-renderer]').nth(1).click();await expect(page.locator('.formula-inline-controls button').first()).toBeEnabled();
  await page.locator('.formula-inline').getByRole('button',{name:'分子',exact:true}).click();
  await page.locator('.formula-inline-input').fill('θ + 1');await page.locator('.formula-inline-input').press('Enter');await expect(page.locator('.formula-inline-controls button').first()).toBeEnabled();await page.locator('.formula-inline-controls button').first().click();
  const expected=source.replace('```readmd-math v1\nfrac(a, b)\n```','```readmd-math v1\nfrac(θ + 1, b)\n```');expect(await page.evaluate(()=>getEditContent())).toBe(expected);
  await expect(page.locator('#preview-pane a[href="#eq:second"]')).toHaveText('公式 2');await page.evaluate(()=>cmUndo());expect(await page.evaluate(()=>getEditContent())).toBe(source);expect(fs.readFileSync(file,'utf8')).toBe(source);
});
test('unsaved object changes renumber immediately, complete stable ids and save without projection metadata',async({page})=>{
  await edit(page);
  const first='```readmd-math v1\n# Keep this author comment\nfrac(a, b)\n```\n\n:label[eq:first] 初始假设\n\n';
  await page.evaluate(first=>{const from=getEditContent().indexOf(first);cmView.dispatch({changes:{from,to:from+first.length,insert:''},userEvent:'input'});},first);
  await expect(page.locator('#preview-pane a[href="#eq:second"]')).toHaveText('公式 1');
  await page.evaluate(()=>cmUndo());await expect(page.locator('#preview-pane a[href="#eq:second"]')).toHaveText('公式 2');
  await page.evaluate(()=>{const pos=cmView.state.doc.length;cmView.dispatch({changes:{from:pos,insert:'\n参见 :ref[eq:s]'},selection:{anchor:pos+'\n参见 :ref[eq:s'.length}});cmView.focus();});
  await page.keyboard.press('Control+Space');await expect(page.locator('.cm-tooltip-autocomplete')).toContainText('eq:second');
  await page.keyboard.press('Tab');expect(await page.evaluate(()=>getEditContent())).toContain(':ref[eq:second]');
  expect(await page.evaluate(()=>getEditContent())).not.toContain(':ref[eq:second]]');
  const expected=await page.evaluate(()=>getEditContent());await page.evaluate(()=>saveEdit());await expect.poll(()=>fs.readFileSync(file,'utf8')).toBe(expected);
  expect(expected).not.toContain('readmd:caption');await page.evaluate(async file=>{if(state.editing)await toggleEdit();await loadFile(file);},file);
  await expect(page.locator('#content a[href="#eq:second"]')).toHaveCount(2);expect(fs.readdirSync(folder).sort()).toEqual(['Workshop.md','route.svg']);
});
test('duplicate and missing references remain readable and report accurate UTF-16 locations',async({page})=>{
  const invalid='😀\n\n$$x$$\n\n:label[eq:same]\n\n$$y$$\n\n:label[eq:same]\n\n:ref[eq:same] :ref[eq:missing]\n';
  const result=await page.evaluate(async content=>(await(await apiFetch('/api/academic/document',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({content})})).json()),invalid);
  expect(result.source_labels).toEqual([]);expect(result.content).toBe(invalid);expect(result.diagnostics).toHaveLength(4);
  for(const d of result.diagnostics)expect(invalid.slice(d.from,d.to)).toContain(d.key);
  await page.evaluate(async invalid=>renderVirtual('clipboard','重复编号.md','',invalid,[]),invalid);
  await expect(page.locator('#content')).toContainText(':ref[eq:missing]');await expect(page.locator('#content a[href="#eq:same"]')).toHaveCount(0);
  await expect(page.locator('#toast')).toContainText('eq:same');await expect(page.locator('#toast')).not.toContainText('academic_duplicate_label');
});
test('real exports retain editable Word fields, body indent, PDF output and EPUB chapter destinations',async({page})=>{
  test.setTimeout(120000);
  expect(process.env.READMD_FONTS_PACK,'Use the pinned offline publication font pack').toBeTruthy();
  const fonts=await page.evaluate(async pack=>(await(await apiFetch('/api/capabilities/packs',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({action:'install',id:'fonts',path:pack})})).json()),process.env.READMD_FONTS_PACK);
  expect(fonts.ok,JSON.stringify(fonts)).toBe(true);
  const content=source.replace('# 从观察到结论','# 从观察到结论\n\n本研究先记录证据。').replace('![证据路线]','# 检验与复核\n\n![证据路线]');
  for(const format of ['docx','pdf','epub']){
    const output=path.join(folder,'Workshop.'+format);
    const result=await page.evaluate(async args=>(await(await apiFetch(args.format==='epub'?'/api/export/epub':'/api/export',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(args)})).json()),{format,content,baseDir:folder,out_path:output,confirm:true,options:{typography:{firstLineIndent:8},epub:{splitLevel:'h1'}}});
    expect(result.ok,JSON.stringify(result)).toBe(true);expect(fs.statSync(output).size).toBeGreaterThan(800);
    expect(result.warn_items?.filter(w=>w.code==='image_unsupported')||[]).toEqual([]);
  }
  const word=zipParts(fs.readFileSync(path.join(folder,'Workshop.docx'))),xml=word.get('word/document.xml');
  expect(xml).toContain(' SEQ ReadMDEquation ');expect(xml).toContain(' SEQ ReadMDTable ');expect(xml).toContain(' SEQ ReadMDFigure ');expect(xml).toContain(' REF ');expect(xml).toContain('w:dirty="true"');expect(xml).toContain('<m:f>');
  expect(word.get('word/styles.xml')).toContain('w:firstLine="454"');
  expect(word.get('word/settings.xml')).toContain('<w:updateFields w:val="true"/>');
  const media=[...zipParts(fs.readFileSync(path.join(folder,'Workshop.docx')),null)].find(([name])=>name.startsWith('word/media/')&&name.endsWith('.png'))?.[1];
  expect(media).toBeTruthy();expect(media.readUInt32BE(16)).toBe(2400);expect(media.readUInt32BE(20)).toBe(800);
  const pixels=await page.evaluate(async data=>{const image=new Image();image.src='data:image/png;base64,'+data;await image.decode();const canvas=document.createElement('canvas');canvas.width=image.width;canvas.height=image.height;const ctx=canvas.getContext('2d');ctx.drawImage(image,0,0);return [...ctx.getImageData(1,1,1,1).data];},media.toString('base64'));
  expect(pixels).toEqual([238,240,255,255]);
  for(const paragraph of xml.match(/<w:p[ >][\s\S]*?<\/w:p>/g)||[])if(paragraph.includes('<m:oMath')||paragraph.includes('<w:drawing>'))expect(paragraph).not.toContain('w:val="BodyText"');
  const epub=zipParts(fs.readFileSync(path.join(folder,'Workshop.epub'))),chapters=[...epub].filter(([name])=>name.endsWith('.xhtml')).map(([,body])=>body);
  const body=chapters.join('');expect(body).toContain('<math');expect(body).toContain('<code>:ref[eq:first]</code>');expect(body.match(/:ref\[/g)).toHaveLength(1);expect(body).not.toContain(':label[');expect(body).toMatch(/chapter_2.xhtml#/);
  expect(fs.readFileSync(file,'utf8')).toBe(source);
});
test('note citation styles create actual footnotes alongside authored notes',async({page})=>{
  fs.writeFileSync(path.join(folder,'references.bib'),'@book{kant,title={Critique of Pure Reason},author={Kant, Immanuel},year={1781}}');
  const content='---\nbibliography: references.bib\ncsl: chicago-notes-bibliography\nlang: en-US\n---\n# Notes\n\nAuthor note[^original], followed by a citation [@kant, p. 42] and a repeated citation [@kant, p. 43].\n\n[^original]: Keep this author note.\n';
  const prepared=await page.evaluate(async args=>(await(await apiFetch('/api/academic/document',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(args)})).json()),{content,baseDir:folder});
  expect(prepared.style_class).toBe('note');expect(prepared.content).toContain('[^readmd-citation-1]');expect(prepared.content).toContain('[^original]: Keep this author note.');
  const output=path.join(folder,'Notes.docx');const result=await page.evaluate(async args=>(await(await apiFetch('/api/export',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(args)})).json()),{format:'docx',content,baseDir:folder,out_path:output});expect(result.ok).toBe(true);
  const word=zipParts(fs.readFileSync(output));expect(word.get('word/document.xml').match(/<w:footnoteReference /g)).toHaveLength(3);expect(word.get('word/footnotes.xml')).toContain('Keep this author note.');expect(word.get('word/footnotes.xml')).toContain('Kant');expect(word.get('word/footnotes.xml')).toContain('42');expect(word.get('word/footnotes.xml')).toContain('43');
});
test('offline SVG export retains transparency and writes no processed sibling image',async({page})=>{
  fs.writeFileSync(path.join(folder,'transparent.svg'),'<svg xmlns="http://www.w3.org/2000/svg" width="800" height="400"><rect x="200" width="400" height="400" fill="#336699" fill-opacity="0.5"/></svg>');
  const content='![Transparent evidence](transparent.svg)\n\n:label[fig:transparent] Transparent overlay';
  for(const format of ['docx','pdf']){const result=await page.evaluate(async args=>(await(await apiFetch('/api/export',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(args)})).json()),{format,content,baseDir:folder,out_path:path.join(folder,'Alpha.'+format)});expect(result.ok).toBe(true);expect(result.warn_items||[]).toEqual([]);}
  const media=[...zipParts(fs.readFileSync(path.join(folder,'Alpha.docx')),null)].find(([n])=>n.startsWith('word/media/'))[1];
  const pixels=await page.evaluate(async data=>{const image=new Image();image.src='data:image/png;base64,'+data;await image.decode();const c=document.createElement('canvas');c.width=image.width;c.height=image.height;const ctx=c.getContext('2d');ctx.drawImage(image,0,0);return {width:image.width,height:image.height,clear:[...ctx.getImageData(1,1,1,1).data],center:[...ctx.getImageData(1200,600,1,1).data]};},media.toString('base64'));
  expect(pixels.width).toBe(2400);expect(pixels.height).toBe(1200);expect(pixels.clear).toEqual([0,0,0,0]);expect(pixels.center[3]).toBe(128);expect(Math.abs(pixels.center[0]-51)).toBeLessThanOrEqual(1);expect(pixels.center[2]).toBe(153);
  expect(fs.readFileSync(path.join(folder,'Alpha.pdf')).toString('latin1')).toContain('/SMask');
  expect(fs.readdirSync(folder).sort()).toEqual(['Alpha.docx','Alpha.pdf','Workshop.md','route.svg','transparent.svg']);
});
test('unsupported math versions remain diagnostic after font refresh and layout changes',async({page})=>{
  const content='```readmd-math v2\nfrac(unknown_version_only, b)\n```\n\n```readmd-math v1\nfrac(a, b)\n```\n';
  const sources=[];page.on('request',r=>{if(r.url().endsWith('/api/math/parse'))sources.push(r.postDataJSON()?.source);});
  fs.writeFileSync(file,content);await page.evaluate(file=>loadFile(file,{force:true,discardConfirmed:true}),file);
  await page.evaluate(async()=>{await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');});
  const unsupported=page.locator('#preview-pane [data-readmd-unsupported]');
  const valid=page.locator('#preview-pane .readmd-dsl-math:not([data-readmd-unsupported])');
  await expect(unsupported).toContainText('暂不支持此公式语法版本');await expect(valid).toHaveAttribute('data-readmd-ready','true');
  await page.evaluate(()=>window.dispatchEvent(new CustomEvent('readmd:capability-installed',{detail:{id:'fonts'}})));
  await expect(valid).toHaveAttribute('data-readmd-ready','true');
  await expect(unsupported).toHaveAttribute('data-readmd-ready','error');
  const previous=await valid.getAttribute('data-readmd-layout');
  await page.evaluate(()=>{$('preview-pane').style.fontSize='23px';});
  await expect.poll(()=>valid.getAttribute('data-readmd-layout')).not.toBe(previous);
  await expect(unsupported).toContainText('暂不支持此公式语法版本');
  await expect(unsupported.locator('[data-readmd-math-renderer]')).toHaveCount(0);
  expect(sources.some(s=>s?.includes('unknown_version_only'))).toBe(false);
  expect(await page.evaluate(()=>getEditContent())).toBe(content);
});

test('unsupported math versions stay readable and saveable while exports preserve existing output',async({page})=>{
  const content='😀\n\n```readmd-math v2\nfrac(a, b)\n```\n\n```readmd-math v1\nfrac(a, b)\n```\n\n````markdown\n```readmd-math v2\nexample\n```\n````\n';
  fs.writeFileSync(file,content);await page.evaluate(file=>loadFile(file,{force:true,discardConfirmed:true}),file);
  await expect(page.locator('#content [data-readmd-unsupported]')).toContainText('暂不支持此公式语法版本');
  await expect(page.locator('#content .readmd-dsl-math[data-readmd-ready="true"]')).toHaveCount(1);
  const prepared=await page.evaluate(async content=>(await(await apiFetch('/api/academic/document',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({content})})).json()),content);
  expect(prepared.content).toBe(content);expect(prepared.diagnostics).toHaveLength(1);expect(prepared.diagnostics[0].from).toBe(4);
  await page.evaluate(async()=>{await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');});
  await expect(page.locator('#preview-pane [data-readmd-unsupported]')).toContainText('暂不支持此公式语法版本');
  await expect(page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"]')).toHaveCount(1);
  await page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"]:not([data-readmd-unsupported]) [data-readmd-math-renderer]').click();await expect(page.locator('.formula-inline-controls button').first()).toBeEnabled();
  await page.locator('.formula-inline').getByRole('button',{name:'分子',exact:true}).click();await page.locator('.formula-inline-input').fill('θ');await page.locator('.formula-inline-input').press('Enter');await expect(page.locator('.formula-inline-controls button').first()).toBeEnabled();await page.locator('.formula-inline-controls button').first().click();
  const expected=content.replace('```readmd-math v1\nfrac(a, b)','```readmd-math v1\nfrac(θ, b)');expect(await page.evaluate(()=>getEditContent())).toBe(expected);
  expect(await page.evaluate(()=>saveEdit())).toBe(true);expect(fs.readFileSync(file,'utf8')).toBe(expected);
  for(const format of ['docx','pdf','epub']){
    const output=path.join(folder,'Keep.'+format);fs.writeFileSync(output,'existing output');
    const result=await page.evaluate(async args=>(await(await apiFetch(args.format==='epub'?'/api/export/epub':'/api/export',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(args)})).json()),{format,content:expected,baseDir:folder,out_path:output,confirm:true,overwrite:true});
    expect(result.ok).toBe(false);expect(result.error_code||result.error).toContain('math_unsupported_version');expect(fs.readFileSync(output,'utf8')).toBe('existing output');
  }
});
