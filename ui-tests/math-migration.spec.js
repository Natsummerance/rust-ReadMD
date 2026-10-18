'use strict';
const {test,expect}=require('@playwright/test');
const fs=require('node:fs'),path=require('node:path');
const {waitForPage}=require('./page-condition.cjs');
const zipParts=require('./zip-parts.cjs');
const root=path.resolve(__dirname,'..');let folder,file;
const xml='<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:f><m:fPr><m:type m:val="skw"/></m:fPr><m:num><m:r><m:t>a</m:t></m:r></m:num><m:den><m:r><m:t>b</m:t></m:r></m:den></m:f></m:oMath>';
async function api(page,url,body){return page.evaluate(async({url,body})=>{const r=await apiFetch(url,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});return {status:r.status,...await r.json()};},{url,body});}
async function metadata(page){const r=await api(page,'/api/math/serialize',{source:xml,dialect:'omml',target:'metadata',latex:'\\frac{a}{b}',id:'legacy-equation'});expect(r.status).toBe(200);return r.output;}
async function edit(page,source){fs.writeFileSync(file,source);await page.evaluate(async file=>{await loadFile(file,{force:true,discardConfirmed:true});await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');},file);}
test.beforeEach(async({page})=>{
  folder=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/math-migration-ui-'));file=path.join(folder,'Study.md');
  await page.route('**/api/update/check',r=>r.fulfill({json:{ok:false}}));await page.goto('/');await waitForPage(page,()=>window.__readmdSessionReady&&window.ReadMDFormula&&window.ReadMDRecovery);
  await page.evaluate(()=>i18n.setLanguage('en',false));
});
test('opening is read only; explicit save migrates standard properties with one keyboard undo',async({page})=>{
  const source='# 😀 Research\r\n\r\n## Estimate\r\n\r\nFirst $\\frac{a}{b}$'+await metadata(page)+'.\r\n\r\nLast $\\frac{a}{b}$'+await metadata(page)+'.\r\n';await edit(page,source);
  expect(fs.readFileSync(file,'utf8')).toBe(source);expect(await page.evaluate(()=>getActiveTab().isDirty)).toBe(false);
  const planned=await api(page,'/api/math/migrate',{content:source});expect(planned.ok).toBe(true);expect(planned.migration.edits).toHaveLength(2);expect(fs.readFileSync(file,'utf8')).toBe(source);
  expect(await page.evaluate(()=>saveEdit())).toBe(true);const saved=fs.readFileSync(file,'utf8');
  expect(saved).toContain(':math[');expect(saved).toContain('office.type(val: "skw")');expect(saved).not.toContain('readmd:math:v1');expect(saved.endsWith('\r\n')).toBe(true);
  expect(await page.evaluate(()=>getEditContent())).toBe(saved);expect(await page.evaluate(()=>hasUnsavedEditorChanges())).toBe(false);
  await page.evaluate(()=>cmView.focus());await page.keyboard.press('Control+z');expect(await page.evaluate(()=>getEditContent())).toBe(source);
  await page.keyboard.press('Control+y');expect(await page.evaluate(()=>getEditContent())).toBe(saved);
  await page.evaluate(async file=>{await exitEdit();await loadFile(file,{force:true,discardConfirmed:true});},file);
  await expect(page.locator('#content .readmd-dsl-math[data-readmd-ready="true"]')).toHaveCount(2);
  expect(fs.readdirSync(folder)).toEqual(['Study.md']);
});
test('stale metadata detaches only after protected recovery is saved and opens as a separate copy',async({page})=>{
  const source='$\\frac{z}{b}$'+await metadata(page);await edit(page,source);
  expect(await page.evaluate(()=>saveEdit())).toBe(true);expect(fs.readFileSync(file,'utf8')).toBe('$\\frac{z}{b}$');
  const entries=await page.evaluate(async()=> (await(await apiFetch('/api/documents/history')).json()).entries);
  const record=entries.find(e=>e.kind==='math_recovery'&&e.path.endsWith('Study.md'));expect(record).toBeTruthy();expect(record.context.reason).toBe('math_metadata_stale');
  const copy=await api(page,'/api/documents/history',{op:'read',id:record.id});expect(copy.content).toContain('skw');expect(copy.content).not.toContain('readmd:math:v1');
  await page.locator('#btn-more').click();await page.locator('.more-group').filter({has:page.locator('#btn-document-history')}).locator('.more-group-header').click();await page.locator('#btn-document-history').click();await page.locator('[data-history-id="'+record.id+'"]').getByRole('button',{name:'Open as copy',exact:true}).click();
  await expect(page.locator('#document-history-modal')).toHaveClass(/hidden/);expect(await page.evaluate(()=>getEditContent())).toBe(copy.content);
  expect(await page.evaluate(()=>getActiveTab().path)).toBeFalsy();expect(fs.readFileSync(file,'utf8')).toBe('$\\frac{z}{b}$');
  expect(fs.readdirSync(folder)).toEqual(['Study.md']);
});
test('typing during save keeps the newer working content and the earlier disk revision',async({page})=>{
  const source='$\\frac{a}{b}$'+await metadata(page);await edit(page,source);
  let release,started=false;const gate=new Promise(r=>release=r);await page.route('**/api/save',async route=>{started=true;await gate;await route.continue();});
  try{
    await page.evaluate(()=>{window.__migrationSave=saveEdit();});await expect.poll(()=>started).toBe(true);
    await page.evaluate(()=>cmView.dispatch({changes:{from:cmView.state.doc.length,insert:'\nNew author text'}}));release();expect(await page.evaluate(()=>window.__migrationSave)).toBe(true);
    expect(await page.evaluate(()=>getEditContent())).toBe(source+'\nNew author text');expect(await page.evaluate(()=>hasUnsavedEditorChanges())).toBe(true);
    expect(fs.readFileSync(file,'utf8')).not.toContain('New author text');expect(fs.readFileSync(file,'utf8')).toContain(':math[');
    expect(await page.evaluate(()=>saveEdit())).toBe(true);expect(fs.readFileSync(file,'utf8')).toContain('New author text');
    expect(await page.evaluate(()=>hasUnsavedEditorChanges())).toBe(false);
  }finally{release();}
});
test('saving again after undo acknowledges migrated disk content and keeps duplicate recovery anchors',async({page})=>{
  const comment=await metadata(page),source='# Repeated estimates\r\n\r\n$\\frac{z}{b}$'+comment+'\r\n\r\n$\\frac{z}{b}$'+comment+'\r\n';await edit(page,source);
  expect(await page.evaluate(()=>saveEdit())).toBe(true);const saved=fs.readFileSync(file,'utf8'),mtime=fs.statSync(file).mtimeMs;
  const entries=await page.evaluate(async file=>(await(await apiFetch('/api/documents/history')).json()).entries.filter(e=>e.kind==='math_recovery'&&e.path===file),file);
  expect(entries).toHaveLength(2);expect(entries[0].context.from).not.toBe(entries[1].context.from);
  await page.evaluate(()=>cmView.focus());await page.keyboard.press('Control+z');expect(await page.evaluate(()=>getEditContent())).toBe(source);
  expect(await page.evaluate(()=>saveEdit())).toBe(true);expect(await page.evaluate(()=>getEditContent())).toBe(saved);expect(await page.evaluate(()=>hasUnsavedEditorChanges())).toBe(false);expect(fs.statSync(file).mtimeMs).toBe(mtime);
  const again=await page.evaluate(async file=>(await(await apiFetch('/api/documents/history')).json()).entries.filter(e=>e.kind==='math_recovery'&&e.path===file),file);
  expect(again.map(e=>e.id).sort()).toEqual(entries.map(e=>e.id).sort());expect(fs.readdirSync(folder)).toEqual(['Study.md']);
});
test('an external file revision is never overwritten by migration',async({page})=>{
  const source='$\\frac{a}{b}$'+await metadata(page);await edit(page,source);fs.writeFileSync(file,'External author revision');
  const pending=page.evaluate(()=>saveEdit());await expect(page.locator('#save-conflict-modal')).not.toHaveClass(/hidden/);await page.locator('#save-conflict-cancel').click();expect(await pending).toBe(false);
  expect(fs.readFileSync(file,'utf8')).toBe('External author revision');expect(await page.evaluate(()=>getEditContent())).toBe(source);expect(fs.readdirSync(folder)).toEqual(['Study.md']);
});
test('staged image and formula migration share an exact save acknowledgment and undo',async({page})=>{
  const source='# 😀 Figure\r\n\r\n## Original\r\n\r\n$\\frac{a}{b}$'+await metadata(page)+'\r\n';await edit(page,source);
  const staged=await api(page,'/api/image/save',{stage:true,name:'chart',format:'png',data:'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg=='});expect(staged.ok).toBe(true);
  await page.evaluate(staged=>{registerStagedImage(staged.asset);cmView.dispatch({changes:{from:cmView.state.doc.length,insert:'\n![chart]('+staged.rel+')\n'},userEvent:'input.paste'});},staged);
  expect(fs.readdirSync(folder)).toEqual(['Study.md']);expect(await page.evaluate(()=>saveEdit())).toBe(true);
  const saved=fs.readFileSync(file,'utf8');expect(saved).toContain('readmd-assets/');expect(saved).toContain(':math[');expect(saved).not.toContain('readmd:math:v1');expect(await page.evaluate(()=>getEditContent())).toBe(saved);
  await page.evaluate(()=>cmView.focus());await page.keyboard.press('Control+z');expect(await page.evaluate(()=>getEditContent())).toBe(source);
  await page.keyboard.press('Control+y');expect(await page.evaluate(()=>getEditContent())).toBe(saved);
});
test('nested quote and list formulas render, edit in place and export as math',async({page})=>{
  const source='# Research\n\n> ```readmd-math v1\n> frac(a, b, style: skewed)\n> ```\n\n- ```readmd-math v1\n  matrix([a, b], [c, d])\n  ```\n\n````markdown\n> ```readmd-math v1\n> example\n> ```\n````\n';await edit(page,source);
  await expect(page.locator('#preview-pane .readmd-dsl-math[data-readmd-ready="true"]')).toHaveCount(2);
  await expect(page.locator('#preview-pane blockquote .readmd-dsl-math')).toHaveCount(1);await expect(page.locator('#preview-pane li .readmd-dsl-math')).toHaveCount(1);
  await page.locator('#preview-pane blockquote mjx-container').click();await expect(page.locator('.formula-inline-controls button').first()).toBeEnabled();
  await page.locator('.formula-inline').getByRole('button',{name:'Numerator',exact:true}).click();await page.locator('.formula-inline-input').fill('θ + 1');await page.locator('.formula-inline-input').press('Enter');await expect(page.locator('.formula-inline-controls button').first()).toBeEnabled();await page.locator('.formula-inline-controls button').first().click();
  const changed=source.replace('> frac(a, b, style: skewed)','> frac(θ + 1, b, style: skewed)');expect(await page.evaluate(()=>getEditContent())).toBe(changed);expect(await page.evaluate(()=>saveEdit())).toBe(true);
  const output=path.join(folder,'Research.docx');expect((await api(page,'/api/export',{format:'docx',content:changed,out_path:output,baseDir:folder})).ok).toBe(true);
  const doc=zipParts(fs.readFileSync(output)).get('word/document.xml');expect(doc).toContain('<m:f>');expect(doc).toContain('<m:m>');expect(doc).toContain('θ');expect(doc).toContain('example');
});
test('invalid nested math is diagnosed before any existing output can be replaced',async({page})=>{
  const content='> ```readmd-math v1\n> frac(\n> ```\n';const output=path.join(folder,'Existing.docx');fs.writeFileSync(output,'KEEP');
  expect((await api(page,'/api/export',{format:'docx',content,out_path:output,overwrite:true})).ok).not.toBe(true);expect(fs.readFileSync(output,'utf8')).toBe('KEEP');
});
for(const [container,prefix,selector]of[['quote','> ','blockquote'],['list','- ','li']])test('advanced LaTeX properties become readable DSL inside a '+container,async({page})=>{
  const source='# Review\n\n'+prefix+'$$\\frac{a}{b}$$\n\nKeep the author notes.\n';await edit(page,source);
  await page.locator('#preview-pane mjx-container').click();await expect(page.locator('.formula-inline-controls button').first()).toBeEnabled();
  await page.locator('.formula-properties > summary').click();await page.locator('[data-math-container="fPr"] > summary').click();await page.locator('[data-math-property="fPr.type"] select').selectOption('skw');await expect(page.locator('.formula-inline-controls button').first()).toBeEnabled();await page.locator('.formula-inline-controls button').first().click();
  const changed=await page.evaluate(()=>getEditContent());expect(changed).toContain(prefix+'```readmd-math v1');expect(changed).toContain('office.type(val: "skw")');expect(changed).toContain('Keep the author notes.');expect(changed).not.toContain('readmd:math:v1:');
  await expect(page.locator('#preview-pane '+selector+' .readmd-dsl-math[data-readmd-ready="true"]')).toHaveCount(1);
  await page.keyboard.press('Control+z');expect(await page.evaluate(()=>getEditContent())).toBe(source);await page.keyboard.press('Control+y');expect(await page.evaluate(()=>getEditContent())).toBe(changed);
  expect(await page.evaluate(()=>saveEdit())).toBe(true);expect(fs.readFileSync(file,'utf8')).toBe(changed);
});
