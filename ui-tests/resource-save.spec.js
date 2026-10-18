const { test, expect } = require('@playwright/test');
const fs = require('node:fs/promises');
const path = require('node:path');
const { waitForPage } = require('./page-condition.cjs');

test.beforeEach(async ({page}) => {
  await page.route('**/api/update/check', r => r.fulfill({contentType:'application/json',body:'{"ok":false}'}));
  await page.goto('/'); await waitForPage(page, () => window.__readmdSessionReady && window.ReadMDRecovery);
  await page.evaluate(() => window.__readmdSessionReady);
});

test('real staged image saves into shared resources, reopens, and leaves code examples untouched', async ({page}) => {
  const file = await page.evaluate(async () => {
    const file = await uploadFile(new File(['# Image note\n'], 'resource-save.md', {type:'text/markdown'}));
    await loadFile(file); await toggleEdit(); setEditorPref('aiCompletion',false); return file;
  });
  const directory = path.dirname(file), before = (await fs.readdir(directory)).sort();
  const staged = await page.evaluate(async () => {
    const response = await apiFetch('/api/image/save', {method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({stage:true,name:'diagram',format:'png',data:'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg=='})});
    if(!response.ok) throw Error('Image staging failed');
    const data = await response.json(); registerStagedImage(data.asset);
    cmView.dispatch({changes:{from:cmView.state.doc.length,insert:`\n![diagram](${data.rel})\n\n\`\`\`markdown\n![example](${data.rel})\n\`\`\`\n`}});
    return data;
  });
  expect((await fs.readdir(directory)).sort()).toEqual(before);
  expect(path.dirname(staged.path)).not.toBe(directory);
  expect(await page.evaluate(() => saveEdit())).toBe(true);
  const saved = await fs.readFile(file,'utf8');
  expect(saved).toMatch(/!\[diagram\]\(readmd-assets\//);
  expect(saved).toContain(`![example](${staged.rel})`);
  const images = await fs.readdir(path.join(directory,'readmd-assets'));
  expect(images.length).toBeGreaterThan(0);
  expect(await page.evaluate(() => saveEdit())).toBe(true);
  expect(await fs.readdir(path.join(directory,'readmd-assets'))).toEqual(images);
  await page.evaluate(() => cmUndo());
  expect(await page.evaluate(() => getEditContent())).toBe('# Image note\n');
  await page.evaluate(() => cmRedo());
  expect(await page.evaluate(() => getEditContent())).toBe(saved);
  await page.evaluate(async file => {await exitEdit(); await loadFile(file,{force:true,discardConfirmed:true});},file);
  await expect.poll(() => page.locator('#content img').evaluateAll(images => images.length===1 && images[0].complete && images[0].naturalWidth===1)).toBe(true);
});

test('paste cannot insert an asynchronously decoded image into another document', async ({page}) => {
  let release; const gate = new Promise(resolve => release = resolve); let pending = false;
  await page.route('**/api/image/save',async route => { pending=true;await gate;const response=await route.fetch();await route.fulfill({response}); });
  await page.evaluate(async () => {
    await renderVirtual('clipboard','First.md','','First',[]);await toggleEdit();
    const data=new DataTransfer();data.items.add(new File([Uint8Array.from(atob('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg=='),c=>c.charCodeAt(0))],'image.png',{type:'image/png'}));
    handleSmartExcelPaste(new ClipboardEvent('paste',{clipboardData:data,cancelable:true}));
  });
  await expect.poll(() => pending).toBe(true);
  await page.evaluate(async () => {await renderVirtual('clipboard','Second.md','','Second',[]);await toggleEdit();});
  const completed=page.waitForResponse('**/api/image/save'); release();await completed;
  await expect.poll(() => page.evaluate(() => getEditContent())).toBe('Second');
  expect(await page.evaluate(() => getActiveTab().webAssets)).toEqual([]);
});
