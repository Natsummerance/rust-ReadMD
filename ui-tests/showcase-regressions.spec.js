const { test, expect } = require('@playwright/test');
const fs = require('node:fs/promises');
test.beforeEach(async ({ page }) => {
  await page.route('**/api/update/check', r => r.fulfill({ status: 200, contentType: 'application/json', body: '{"ok":false}' }));
  await page.goto('/');
  await page.waitForFunction(() => typeof readerPrepare === 'function' && window.ReadMDRecovery);
});
async function open(page, name, content) {
  return page.evaluate(async ({name,content}) => {
    const path = await uploadFile(new File([content], name, {type:'text/markdown'}));
    await loadFile(path);return path;
  }, {name,content});
}
test('first reading render hides only closed YAML metadata and retains exact source', async ({page}) => {
  const source='---\r\ntitle: Reading metadata\r\nauthor: Demo\r\n---\r\n\r\n# Visible reading\r\n\r\nEvidence.\r\n';
  await open(page,'metadata.md',source);
  await expect(page.locator('#content')).toContainText('Visible reading');
  await expect(page.locator('#content')).not.toContainText('Reading metadata');
  expect(await page.evaluate(() => state.original)).toBe(source);
  const prepared=await page.evaluate(source => {const helper=window.ReadMDReader;window.ReadMDReader=null;try{return readerPrepare(source);}finally{window.ReadMDReader=helper;}},source);
  expect(prepared.split('\n').length).toBe(source.split('\n').length);
  expect(await page.evaluate(() => readerPrepare('---\nplain text\n---\n'))).toBe('---\nplain text\n---\n');
  expect(await page.evaluate(() => readerPrepare('---\ntitle: Unclosed\n'))).toBe('---\ntitle: Unclosed\n');
});
test('inline AI cannot apply a pending result and applying one result is one undo', async ({page}) => {
  const original='Facts and explanations should be recorded separately.';
  await open(page,'inline.md',original);await page.evaluate(() => toggleEdit());
  await page.evaluate(() => {ensureAiConfigured=async()=>({provider:'test',model:'test'});cmView.dispatch({selection:{anchor:0,head:cmView.state.doc.length}});openEditAiBar();});
  let release;const held=new Promise(r=>release=r);let request;
  await page.route('**/api/ai/chat',async r=>{request=r.request().postDataJSON();await held;await r.fulfill({status:200,contentType:'application/json',body:'{"ok":true,"content":"Separate observed facts from their explanations."}'});});
  await page.evaluate(() => {window.pendingInline=runEditAiAction('polish');});
  await expect(page.locator('#edit-ai-apply')).toBeDisabled();await expect(page.locator('#edit-ai-insert')).toBeDisabled();
  await expect.poll(() => !!request).toBe(true);expect(request.messages[0].content.trim()).not.toBe('');
  release();await page.evaluate(() => window.pendingInline);
  await expect(page.locator('#edit-ai-apply')).toBeEnabled();await page.locator('#edit-ai-apply').click();
  await expect.poll(() => page.evaluate(() => getEditContent())).toBe('Separate observed facts from their explanations.');
  await page.evaluate(() => cmUndo());expect(await page.evaluate(() => getEditContent())).toBe(original);
});
test('native rename bridge gets a stem and disk extension is retained exactly once', async ({page}) => {
  const source=await open(page,'original.md','# Retain this content\n');
  await page.evaluate(() => {hasPy=true;py={rename_file:async(path,new_stem)=>{window.renameStem=new_stem;return(await apiFetch('/api/rename',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({path,new_stem})})).json();}};});
  await page.keyboard.press('F2');await page.locator('.tab-title-input').fill('renamed');await page.locator('.tab-title-input').press('Enter');
  await expect.poll(() => page.evaluate(() => state.file)).toMatch(/renamed\.md$/);
  expect(await page.evaluate(() => window.renameStem)).toBe('renamed');
  const target=await page.evaluate(() => state.file);expect(await fs.readFile(target,'utf8')).toBe('# Retain this content\n');
  await expect(fs.access(source)).rejects.toThrow();await expect(fs.access(target+'.md')).rejects.toThrow();
  expect(await page.evaluate(() => state.sourceName)).toBe('renamed.md');
});
test('backlinks drawer indexes a fresh folder before querying outgoing links', async ({page}) => {
  const other=await open(page,'fresh-target.md','# Target\n');
  await open(page,'fresh-source.md','# Source\n\n[[fresh-target]]\n');
  await page.evaluate(() => {ReadMDGraph.toggleDrawer();});
  await page.locator('#backlinks-panel [data-tab="outgoing"]').click();
  const target=page.locator('#backlinks-content button').filter({hasText:'fresh-target'});
  await expect(target).toBeVisible();await target.click();
  await expect.poll(() => page.evaluate(() => state.file)).toBe(other);
});
test('printing an open export panel prints only the current reading surface', async ({page}) => {
  await open(page,'print-reading.md','# Printable body\n\nEvidence remains visible.\n');
  await page.locator('#btn-print').click();
  await expect(page.locator('#export-modal')).toBeVisible();
  await page.emulateMedia({media:'print'});
  await expect(page.locator('#export-modal')).toBeHidden();
  await expect(page.locator('#toolbar')).toBeHidden();
  await expect(page.locator('#content')).toBeVisible();
  await expect(page.locator('#content')).toContainText('Printable body');
  await page.emulateMedia({media:'screen'});
  await expect(page.locator('#export-modal')).toBeVisible();
});
test('opening export after deleting all editor text never falls back to the saved original', async ({page}) => {
  await open(page,'empty-export.md','# Saved original must not return\n');
  await page.evaluate(()=>toggleEdit());await page.locator('.cm-editor').waitFor();
  await page.evaluate(()=>cmView.dispatch({changes:{from:0,to:cmView.state.doc.length,insert:''}}));
  await page.locator('#btn-print').click();await expect(page.locator('#export-modal')).toBeVisible();
  expect(await page.evaluate(()=>currentExportContent())).toBe('');
  await expect(page.locator('#export-preview-mini-content')).not.toContainText('Saved original');
});
test('Windows code files keep line endings and cancel unchanged editing without a save prompt', async ({page}) => {
  const source='const minutes = [35, 28];\r\nconsole.log(minutes);\r\n';
  await open(page,'windows-code.js',source);
  await page.locator('#btn-code-edit').click();await page.locator('.cm-editor').waitFor();
  expect(await page.evaluate(()=>getEditContent())).toBe(source);
  expect(await page.evaluate(()=>hasUnsavedEditorChanges())).toBe(false);
  await page.locator('#edit-cancel').click();
  await expect(page.locator('#btn-code-ai-explain')).toBeVisible();
  await page.locator('#btn-code-edit').click();await page.locator('.cm-editor').waitFor();
  await page.evaluate(()=>cmView.dispatch({changes:{from:0,to:0,insert:'// edited\n'}}));
  expect(await page.evaluate(()=>getEditContent())).toBe('// edited\r\n'+source);
  await page.evaluate(()=>cmUndo());
  expect(await page.evaluate(()=>hasUnsavedEditorChanges())).toBe(false);
  await page.locator('#edit-cancel').click();
  await open(page,'ordinary.md','# Markdown\n');
  await expect(page.locator('#btn-code-ai-explain')).toHaveCount(0);
});
test('persisted code output uses editor offsets while retaining Windows file line endings', async ({page}) => {
  const source='# Results\r\n\r\n```javascript\r\nconsole.log(3);\r\n```\r\n\r\nKeep this final paragraph.\r\n';
  const file=await open(page,'windows-output.md',source);
  await page.evaluate(()=>toggleEdit());await page.locator('.cm-editor').waitFor();
  expect(await page.evaluate(()=>persistCodeChunkOutput({dataset:{sourceLine:3}},'3'))).toBe(true);
  const saved=await fs.readFile(file,'utf8');
  expect(saved).toContain('console.log(3);\r\n```');
  expect(saved).toContain('<!-- code_chunk_output -->\r\n\r\n3');
  expect(saved).toContain('Keep this final paragraph.\r\n');
  expect(saved.replace(/\r\n/g,'')).not.toContain('\n');
});
