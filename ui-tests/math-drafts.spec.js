const {test,expect}=require('@playwright/test');
const fs=require('node:fs'),path=require('node:path');
const {waitForPage}=require('./page-condition.cjs');
const root=path.resolve(__dirname,'..'),work=fs.mkdtempSync(path.join(root,'.cache/capability-upgrade/math-drafts-ui-'));
test.use({bypassCSP:false});
test.beforeEach(async({page})=>{
  await page.addInitScript(()=>localStorage.setItem('readmd_language','en'));
  await page.goto('/');await waitForPage(page,()=>window.__readmdAppReady);
  await page.evaluate(()=>Promise.all([window.__readmdSessionReady,window.__readmdPreferencesReady]));
});
async function edit(page,source){
  const file=path.join(work,Math.random().toString(36).slice(2)+'.md');fs.writeFileSync(file,source);
  await page.evaluate(async file=>{await loadFile(file);await toggleEdit();setEditorPref('aiCompletion',false);setPvLayout('right');},file);
  await expect(page.locator('#preview-pane mjx-container').first()).toBeVisible();return file;
}
async function invalid(page,index=0){
  await page.locator('#preview-pane mjx-container').nth(index).click();
  await expect(page.locator('.formula-inline-controls button').first()).toBeEnabled();
  await page.locator('.formula-inline-input').fill('frac(');await page.locator('.formula-inline-input').press('Enter');
  await expect(page.locator('.formula-inline-message')).not.toHaveText('');
  expect(await page.evaluate(()=>ReadMDInlineMath.flush())).toBe(true);
}
async function records(page,file){
  return page.evaluate(async file=>{const r=await(await apiFetch('/api/documents/history')).json();return r.entries.filter(e=>e.path===file&&e.kind==='math_draft');},file);
}
test('B2-01 a second identical formula restores its exact pending slot across tabs and corrects with one undo',async({page})=>{
  const source='First :math[frac(a, b)].\n\nSecond :math[frac(a, b)].\n\nKeep this paragraph.';
  const file=await edit(page,source);await invalid(page,1);
  const tab=await page.evaluate(()=>state.activeTabId);
  await page.evaluate(async()=>{await renderVirtual('clipboard','Other.md','','Unrelated document.',[]);});
  await page.evaluate(async tab=>{await switchTab(tab);await toggleEdit();},tab);
  await expect(page.locator('.formula-inline-input')).toHaveValue('frac(');
  expect(await page.evaluate(()=>ReadMDInlineMath.locate($('preview-pane').querySelector('mjx-container'))?.from)).toBe(source.indexOf(':math['));
  await page.locator('.formula-inline-input').fill('θ');await page.locator('#preview-pane p').last().click();
  await expect(page.locator('.formula-inline')).toHaveCount(0);
  expect(await page.evaluate(()=>getEditContent())).toBe(source.replace('Second :math[frac(a, b)]','Second :math[frac(θ, b)]'));
  await expect.poll(async()=> (await records(page,file)).length).toBe(0);
  await page.evaluate(()=>cmUndo());expect(await page.evaluate(()=>getEditContent())).toBe(source);expect(fs.readFileSync(file,'utf8')).toBe(source);
});
test('B2-02 saving the body keeps invalid formula input independently and reload restores it',async({page})=>{
  const source=':math[frac(a, b)]\n\nSaved body.';const file=await edit(page,source);await invalid(page);
  expect(await page.evaluate(()=>saveEdit())).toBe(true);expect(fs.readFileSync(file,'utf8')).toBe(source);
  expect((await records(page,file)).length).toBe(1);
  await page.evaluate(()=>ReadMDSession.flush());await page.reload();await waitForPage(page,()=>window.__readmdAppReady);
  await page.evaluate(()=>window.__readmdSessionReady);
  await expect(page.locator('.formula-inline-input')).toHaveValue('frac(');
  expect(await page.evaluate(()=>getEditContent())).toBe(source);
});
test('B2-05 external changes cannot receive an old formula draft; recovery opens readable source',async({page})=>{
  const source=':math[frac(a, b)]\n\nOriginal.';const file=await edit(page,source);await invalid(page);
  const changed='New external paragraph.\n\n'+source;fs.writeFileSync(file,changed);
  await page.evaluate(async file=>{await loadFile(file,{force:true,discardConfirmed:true});await toggleEdit();},file);
  await expect(page.locator('.formula-inline')).toHaveCount(0);expect(await page.evaluate(()=>getEditContent())).toBe(changed);
  await page.evaluate(()=>ReadMDRecovery.open());
  const entry=(await records(page,file))[0];
  await page.locator('[data-history-id="'+entry.id+'"]').getByRole('button',{name:'Open as copy'}).click();
  const copy=await page.evaluate(()=>getEditContent());expect(copy).toContain('frac(');expect(copy).not.toContain('document_revision');expect(copy).not.toContain('model_source');
  expect(fs.readFileSync(file,'utf8')).toBe(changed);
});
test('B2-06 Escape discards only this pending formula and it does not return on reload',async({page})=>{
  const file=await edit(page,':math[frac(a, b)]');await invalid(page);
  await page.locator('.formula-inline-input').press('Escape');expect(await page.evaluate(()=>ReadMDInlineMath.flush())).toBe(true);
  expect((await records(page,file)).length).toBe(0);await page.evaluate(()=>ReadMDSession.flush());await page.reload();await waitForPage(page,()=>window.__readmdAppReady);await page.evaluate(()=>window.__readmdSessionReady);
  await expect(page.locator('.formula-inline')).toHaveCount(0);
});
test('B2-07 failed recovery persistence retains input and prevents navigation until storage succeeds',async({page})=>{
  await edit(page,':math[frac(a, b)]');
  const other=await page.evaluate(async()=>{const current=state.activeTabId;await renderVirtual('clipboard','Target.md','','Other.',[]);const other=state.activeTabId;await switchTab(current);await toggleEdit();setPvLayout('right');return other;});
  await page.locator('#preview-pane mjx-container').first().click();await expect(page.locator('.formula-inline-controls button').first()).toBeEnabled();
  await page.route('**/api/documents/history',route=>{const body=route.request().postDataJSON();return body?.kind==='math_draft'?route.fulfill({status:503,json:{ok:false,error:'recovery_unavailable'}}):route.continue();});
  await page.locator('.formula-inline-input').fill('frac(');await page.locator('.formula-inline-input').press('Enter');
  expect(await page.evaluate(()=>ReadMDInlineMath.flush())).toBe(false);const original=await page.evaluate(()=>state.activeTabId);
  await page.evaluate(other=>switchTab(other),other);expect(await page.evaluate(()=>state.activeTabId)).toBe(original);
  await expect(page.locator('.formula-inline-input')).toHaveValue('frac(');
  await page.unroute('**/api/documents/history');expect(await page.evaluate(()=>ReadMDInlineMath.flush())).toBe(true);
  await page.evaluate(other=>switchTab(other),other);expect(await page.evaluate(()=>state.activeTabId)).toBe(other);
});
test('B2-08 an in-flight draft write cannot resurrect input after Escape',async({page})=>{
  const file=await edit(page,':math[frac(a, b)]');
  await page.locator('#preview-pane mjx-container').click();await expect(page.locator('.formula-inline-controls button').first()).toBeEnabled();
  let release,started;const gate=new Promise(resolve=>release=resolve),waiting=new Promise(resolve=>started=resolve);
  await page.route('**/api/documents/history',async route=>{if(route.request().postDataJSON()?.kind==='math_draft'){started();await gate;}await route.continue();});
  await page.locator('.formula-inline-input').fill('frac(');await page.evaluate(()=>{window.__mathWrite=ReadMDInlineMath.flush();});await waiting;
  await page.locator('.formula-inline-input').press('Escape');release();await page.evaluate(async()=>{await window.__mathWrite;await ReadMDInlineMath.flush();});
  expect((await records(page,file)).length).toBe(0);
});
test('advanced draft properties survive suspension together with an invalid slot',async({page})=>{
  await edit(page,'```readmd-math v1\nfrac(a, b)\n```');
  await page.locator('#preview-pane mjx-container').click();await expect(page.locator('.formula-inline-controls button').first()).toBeEnabled();
  await page.locator('.formula-properties > summary').click();await page.locator('[data-math-container="fPr"] > summary').click();
  await page.locator('[data-math-property="fPr.type"] select').selectOption('skw');
  await expect(page.locator('.formula-inline-controls button').first()).toBeEnabled();
  await page.locator('.formula-inline-input').fill('frac(');await page.locator('.formula-inline-input').press('Enter');expect(await page.evaluate(()=>ReadMDInlineMath.flush())).toBe(true);
  await page.evaluate(async()=>{exitEdit();await toggleEdit();});await expect(page.locator('.formula-inline-input')).toHaveValue('frac(');
  await page.locator('.formula-properties > summary').click();await page.locator('[data-math-container="fPr"] > summary').click();
  await expect(page.locator('[data-math-property="fPr.type"] select')).toHaveValue('skw');
});
