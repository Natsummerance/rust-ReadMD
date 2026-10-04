const { test, expect } = require('@playwright/test');

test.beforeEach(async ({ page }) => {
  await page.route('**/api/update/check', r => r.fulfill({ contentType:'application/json', body:'{"ok":false,"error_code":"update_network_error"}' }));
  await page.addInitScript(() => localStorage.setItem('readmd_language', 'en'));
  await page.goto('/');
  await page.waitForFunction(() => window.ReadMDRecovery && document.getElementById('btn-open').closest('#app-titlebar'));
});
async function document(page, content) {
  await page.evaluate(async text => { const path = await uploadFile(new File([text], 'window-fixture.md')); await loadFile(path); await toggleEdit(); }, content);
}
async function beginClose(page) { await page.evaluate(() => { window.closingTest = closeTab(state.activeTabId); }); }

for (const ending of ['\n', '\r\n', '\r']) test(`unchanged ${JSON.stringify(ending)} file closes without a dialog`, async ({page}) => {
  await document(page, '# Read only' + ending + 'This is the saved text.' + ending);
  await beginClose(page);
  await expect.poll(() => page.evaluate(() => state.tabs.length)).toBe(0);
  await expect(page.locator('#close-confirm-modal')).toBeHidden();
  expect(await page.evaluate(() => state.editing)).toBe(false);
});
test('undo to the saved content clears the dirty marker and closes without a dialog', async ({page}) => {
  await document(page, '# Original\r\n');
  await page.evaluate(() => cmView.dispatch({changes:{from:0,to:cmView.state.doc.length,insert:'# Changed\n'}}));
  expect(await page.evaluate(() => getActiveTab().isDirty)).toBe(true);
  await page.evaluate(() => cmUndo());
  expect(await page.evaluate(() => hasUnsavedEditorChanges())).toBe(false);
  await page.locator('.tab-close').click();
  await expect.poll(() => page.evaluate(() => state.tabs.length)).toBe(0);
  await expect(page.locator('#close-confirm-modal')).toBeHidden();
});
test('a real modification prompts once; cancellation retains it and discard is recoverable', async ({page}) => {
  await document(page, '# Original\n');
  await page.evaluate(() => {
    window.prompts = 0; const original = promptDirtyClose;
    promptDirtyClose = name => { window.prompts++; return original(name); };
    cmView.dispatch({changes:{from:0,to:cmView.state.doc.length,insert:'# Modified draft\n'}});
  });
  await beginClose(page); await expect(page.locator('#close-confirm-modal')).toBeVisible();
  await page.locator('#close-confirm-cancel').click();
  expect(await page.evaluate(() => getEditContent())).toBe('# Modified draft\n');
  await beginClose(page); await page.locator('#close-confirm-discard').click();
  await expect.poll(() => page.evaluate(() => state.tabs.length)).toBe(0);
  expect(await page.evaluate(() => window.prompts)).toBe(2);
  expect(await page.evaluate(async () => (await (await apiFetch('/api/documents/history')).json()).entries.some(e=>e.kind==='discarded'))).toBe(true);
});
for (const source of ['convert','ocr','url']) test(`${source} preview is silent until its text changes`, async ({page}) => {
  await page.evaluate(async source => { await renderVirtual(source, 'Preview.md', '', '# Imported preview\n', []); await toggleEdit(); }, source);
  expect(await page.evaluate(() => hasUnsavedEditorChanges())).toBe(false);
  await beginClose(page); await expect.poll(() => page.evaluate(() => state.tabs.length)).toBe(0);
  await page.evaluate(async source => { await renderVirtual(source, 'Preview.md', '', '# Imported preview\n', []); await toggleEdit(); cmView.dispatch({changes:{from:0,insert:'Edited\n'}}); }, source);
  await beginClose(page); await expect(page.locator('#close-confirm-modal')).toBeVisible(); await page.locator('#close-confirm-cancel').click();
});
test('an AI copy still asks to save its newly created content', async ({page}) => {
  await page.evaluate(async () => { await renderVirtual('ai', 'AI-copy.md', '', '# AI result\n', []); });
  await beginClose(page); await expect(page.locator('#close-confirm-modal')).toBeVisible(); await page.locator('#close-confirm-cancel').click();
});
test('global actions move above tabs and browser mode hides native window controls', async ({page}) => {
  for (const id of ['btn-open','btn-folder','btn-recent','btn-palette','btn-theme','btn-more']) await expect(page.locator('#app-titlebar #' + id)).toBeVisible();
  for (const id of ['btn-toc','btn-search','btn-edit','btn-print','btn-ai']) expect(await page.locator('#toolbar #' + id).count()).toBe(1);
  await expect(page.locator('#window-controls')).toBeHidden();
  await page.locator('#btn-more').click(); await expect(page.locator('#more-menu')).toBeVisible();
  await page.keyboard.press('Escape'); await expect(page.locator('#more-menu')).toBeHidden();
});
test('update checks share a request and recover on reconnect', async ({page}) => {
  await page.evaluate(() => clearTimeout(updateRetryTimer));
  let requests=0,release; const held=new Promise(r=>release=r);
  await page.route('**/api/update/check',async r=>{requests++;await held;await r.fulfill({contentType:'application/json',body:'{"ok":false,"error_code":"update_network_error"}'});});
  await page.evaluate(() => { window.pendingChecks = Promise.all([checkUpdate(true),checkUpdate(false)]); });
  await expect.poll(() => requests).toBe(1); await expect(page.locator('#btn-check-update')).toBeDisabled();
  release(); await page.evaluate(() => window.pendingChecks);
  await expect(page.locator('#toast')).toContainText('retry');
  await page.route('**/api/update/check',async r=>{requests++;await r.fulfill({contentType:'application/json',body:'{"ok":true,"has_update":false,"current_version":"2.4.0"}'});});
  await page.evaluate(() => window.dispatchEvent(new Event('online')));
  await expect.poll(() => requests).toBe(2);
  expect(await page.evaluate(() => updateCheckFailures)).toBe(0);
});
test('installation cancellation never calls the backend or loses the dirty document', async ({page}) => {
  await document(page, 'Original'); await page.evaluate(() => cmView.dispatch({changes:{from:0,insert:'Modified '}}));
  let calls=0; await page.route('**/api/update/apply', r=>{calls++;return r.fulfill({body:'{"ok":false}'});});
  await page.evaluate(() => { updateReadyFile='fixture.exe';updateInfo={flavor:'win_installer'}; window.applying=applyReadyUpdate(); });
  await expect(page.locator('#close-confirm-modal')).toBeVisible(); await page.locator('#close-confirm-cancel').click();
  await page.evaluate(() => window.applying); expect(calls).toBe(0); expect(await page.evaluate(()=>getEditContent())).toBe('Modified Original');
});
for (const size of [{width:1160,height:820},{width:1024,height:680},{width:720,height:480}]) test(`chrome and plugin panel fit ${size.width}×${size.height}`, async ({page}) => {
  await page.setViewportSize(size);
  await page.evaluate(() => {
    hasPy=true;py={custom_titlebar:true,window_control:()=>{},save_settings:async()=>true,request_quit:()=>{}};
    delete document.getElementById('titlebar-actions').dataset.initialized;initWindowChrome();
    __readmdWindowState({maximized:false,fullscreen:false,trayAvailable:true});
  });
  await page.locator('#window-controls').waitFor();
  const boxes=await page.locator('#app-titlebar button:not(.hidden)').evaluateAll(elements=>elements.filter(e=>e.getClientRects().length).map(e=>{const r=e.getBoundingClientRect();return {x:r.x,y:r.y,right:r.right,bottom:r.bottom,w:r.width,h:r.height};}));
  for(const r of boxes){expect(r.x).toBeGreaterThanOrEqual(0);expect(r.right).toBeLessThanOrEqual(size.width);expect(r.w).toBeGreaterThanOrEqual(44);expect(r.h).toBeGreaterThanOrEqual(44);}
  await page.evaluate(() => openPluginModal()); await expect(page.locator('#plugin-cards-grid .plugin-card').first()).toBeVisible();
  await page.locator('#plugin-search').fill('not-a-real-plugin'); await expect(page.locator('#plugin-list-status')).toContainText('No matching');
  await page.locator('#plugin-search').fill(''); await page.locator('#plugin-installed-filter').click();await expect(page.locator('#plugin-installed-filter')).toHaveAttribute('aria-pressed','true');
  await expect(page.locator('#plugin-cards-grid button[data-action="install"]')).toHaveCount(0);
  const box=await page.locator('#plugin-box').boundingBox();expect(box.y).toBeGreaterThanOrEqual(0);expect(box.y+box.height).toBeLessThanOrEqual(size.height);
  expect(await page.locator('#plugin-box').evaluate(e=>e.scrollWidth<=e.clientWidth)).toBe(true);
  await page.screenshot({path:`test-results/window-${size.width}.png`});
});
