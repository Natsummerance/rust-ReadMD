const { test, expect } = require('@playwright/test');
const fs = require('node:fs');
const path = require('node:path');

test('editing search uses the current draft; replace all is undoable and never changes the original', async ({ page }) => {
  await page.goto('/'); await page.waitForFunction(() => window.__readmdAppReady);
  const original = '# Search acceptance\n\nOriginal stays intact.\n';
  await page.evaluate(async original => {
    const uploaded = await uploadFile(new File([original], 'search-acceptance.md', { type: 'text/markdown' }));
    await loadFile(uploaded); state.pvLayout = 'none'; await toggleEdit();
    cmView.dispatch({ changes: { from: cmView.state.doc.length, insert: '\nDraftNeedle draftneedle DRAFTNEEDLE [literal].\n' } });
  }, original);
  await page.locator('#btn-search').click();
  await page.locator('#search-input').fill('draftneedle');
  await expect(page.locator('#search-count')).toHaveText('1/3');
  await page.locator('#search-input').press('Enter');
  expect(await page.evaluate(() => cmView.state.sliceDoc(cmView.state.selection.main.from, cmView.state.selection.main.to))).toBe('DraftNeedle');
  await page.locator('#search-next').click();
  expect(await page.evaluate(() => cmView.state.sliceDoc(cmView.state.selection.main.from, cmView.state.selection.main.to))).toBe('draftneedle');
  await page.locator('#search-replace-input').fill('Replaced');
  const before = await page.evaluate(() => cmView.state.doc.toString());
  await page.locator('#search-replace-all').click();
  expect(await page.evaluate(() => cmView.state.doc.toString())).toBe(before.replace(/draftneedle/gi, 'Replaced'));
  await page.keyboard.press('Control+z');
  expect(await page.evaluate(() => cmView.state.doc.toString())).toBe(before);
  expect(await page.evaluate(() => state.original)).toBe(original);
  await page.locator('#search-input').fill('[literal]');
  await expect(page.locator('#search-count')).toHaveText('1/1');
  await page.locator('#search-input').fill('missing');
  await expect(page.locator('#search-replace-all')).toBeDisabled();
  await page.keyboard.press('Escape');
  await expect(page.locator('#search-bar')).toBeHidden();
});

test('search and replace fits the narrow window without wrapping the editing toolbar', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 680 });
  await page.goto('/'); await page.waitForFunction(() => window.__readmdAppReady);
  await page.evaluate(async () => { await newDocument(); if (!state.editing) await toggleEdit(); toggleSearch(); });
  for (const id of ['search-input','search-close','search-replace-input','search-replace-one','search-replace-all']) {
    const box = await page.locator('#' + id).boundingBox();
    expect(box, id + ' must be visible').not.toBeNull();
    expect(box.x).toBeGreaterThanOrEqual(0); expect(box.x + box.width).toBeLessThanOrEqual(390);
  }
  fs.mkdirSync(path.resolve(__dirname, '../.cache/community-upgrade/screens'), { recursive: true });
  await page.screenshot({ path: path.resolve(__dirname, '../.cache/community-upgrade/screens/editor-search-390.png') });
});

test('textarea fallback searches the draft and replaces literally with a single undo', async ({ page }) => {
  await page.route('**/codemirror.bundle.js*', route => route.fulfill({status:503,body:'Unavailable'}));
  await page.goto('/'); await page.waitForFunction(() => window.__readmdAppReady);
  await page.evaluate(async () => { await renderVirtual('clipboard', 'fallback-search.md', '', '# Original', []); await toggleEdit(); });
  await page.locator('#edit-area').fill('Needle one needle two [literal]');
  await page.locator('#btn-search').click(); await page.locator('#search-input').fill('needle');
  await expect(page.locator('#search-count')).toHaveText('1/2');
  await page.locator('#search-next').click();
  expect(await page.evaluate(() => $('edit-area').value.slice($('edit-area').selectionStart,$('edit-area').selectionEnd))).toBe('needle');
  await page.locator('#search-replace-input').fill('$1 literal'); await page.locator('#search-replace-all').click();
  await expect(page.locator('#edit-area')).toHaveValue('$1 literal one $1 literal two [literal]');
  await page.locator('#edit-area').focus(); await page.keyboard.press('Control+z');
  await expect(page.locator('#edit-area')).toHaveValue('Needle one needle two [literal]');
  expect(await page.evaluate(() => state.original)).toBe('# Original');
});
