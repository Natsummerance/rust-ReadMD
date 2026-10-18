const { test, expect } = require('@playwright/test');
test.beforeEach(async ({ page }) => {
  await page.goto('/'); await page.evaluate(() => window.__readmdSessionReady);
  await page.evaluate(async () => {
    await renderVirtual('convert', '任务核对.md', '', '# 任务核对\n\n- [ ] 检查来源\n- [x] 核对证据\n\n[[关联笔记]]', []);
    await loadReaderAssets(); await renderActiveTab();
  });
});
const geometry = el => {
  const s = getComputedStyle(el);
  return { width: s.width, height: s.height, radius: s.borderRadius, border: s.borderWidth,
    color: s.backgroundColor, image: s.backgroundImage };
};
test('static and dynamically generated checkboxes use the AI control style in every theme', async ({ page }) => {
  await page.evaluate(async () => {
    openExportModal();
    await ReadMDGraph.open(); ReadMDGraph.close();
  });
  expect(await page.locator('#export-modal input[type=checkbox]').count()).toBeGreaterThan(4);
  expect(await page.locator('#content input[type=checkbox]').count()).toBe(2);
  for (const theme of ['light', 'dark', 'sepia']) {
    await page.evaluate(theme => { state.theme = theme; applySettings(); }, theme);
    const reference = await page.locator('#ai-incognito').evaluate(geometry);
    const mismatches = await page.locator('input[type=checkbox]:not(.plugin-switch-input):not(.plugin-switch input):not(.apple-switch input)').evaluateAll((elements, reference) => elements.flatMap(el => {
      const s = getComputedStyle(el);
      return ['width','height','borderRadius','borderWidth'].some((k,i) => s[k] !== [reference.width,reference.height,reference.radius,reference.border][i]) ? [{ id: el.id, width: s.width, height: s.height, radius: s.borderRadius }] : [];
    }), reference);
    expect(reference.width).toBe('14px'); expect(reference.radius).toBe('4px'); expect(mismatches).toEqual([]);
    const checked = page.locator('#content input:checked');
    await page.locator('#ai-incognito').evaluate(el => { el.checked = true; });
    await expect.poll(async () => JSON.stringify(await checked.evaluate(geometry)) === JSON.stringify(await page.locator('#ai-incognito').evaluate(geometry))).toBe(true);
    const tick = await page.locator('#ai-incognito').evaluate(el => ({ image:getComputedStyle(el).backgroundImage, foreground:getComputedStyle(el).getPropertyValue('--accent-fg').trim() }));
    expect(tick.image).toContain(theme === 'dark' ? 'rgb(10, 16, 48)' : 'rgb(255, 255, 255)');
  }
});

test('AI label, keyboard focus, mixed state and disabled state still work; switches retain their track', async ({ page }) => {
  await page.evaluate(() => { $('export-modal').classList.add('hidden'); toggleAiPanel(); });
  const box = page.locator('#ai-incognito');
  await box.focus(); await page.keyboard.press('Space'); await expect(box).toBeChecked();
  expect(await box.evaluate(el => getComputedStyle(el).outlineStyle)).toBe('solid');
  expect((await box.locator('..').boundingBox()).height).toBeGreaterThanOrEqual(44);
  await box.locator('..').click(); await expect(box).not.toBeChecked();
  await box.evaluate(el => { el.indeterminate = true; });
  expect((await box.evaluate(geometry)).image).toContain('linear-gradient');
  await box.evaluate(el => { el.disabled = true; });
  await expect(box).toBeDisabled();
  expect(await box.evaluate(el => getComputedStyle(el).opacity)).toBe('0.5');
  const switchGeometry = await page.locator('#pet-enabled').evaluate(el => ({ input: getComputedStyle(el).position, track: getComputedStyle(el.nextElementSibling).width }));
  expect(switchGeometry.input).toBe('absolute'); expect(parseFloat(switchGeometry.track)).toBeGreaterThan(14);
});

test('editor task widgets and view menu indicators use the same glyph and task edits remain undoable', async ({ page }) => {
  await page.evaluate(async () => { await toggleEdit(); setPvLayout('none'); });
  const task = page.locator('.cm-task-box:not(.is-checked)').first();
  await expect(task).toBeVisible();
  const before = await page.evaluate(() => getEditContent());
  await task.click(); expect(await page.evaluate(() => getEditContent())).toContain('- [x] 检查来源');
  const reference = await page.locator('#ai-incognito').evaluate(el => { el.checked = true; const s = getComputedStyle(el); return { width:s.width,height:s.height,image:s.backgroundImage,radius:s.borderRadius }; });
  const widget = await page.locator('.cm-task-box.is-checked').first().evaluate(el => { const s=getComputedStyle(el);return {width:s.width,height:s.height,image:s.backgroundImage,radius:s.borderRadius}; });
  expect(widget).toEqual(reference);
  await page.evaluate(() => cmUndo()); expect(await page.evaluate(() => getEditContent())).toBe(before);
  await page.evaluate(() => setEditorPref('focus', true));
  const indicators = await page.locator('.md-check').evaluateAll(els => els.map(el => ({ width:getComputedStyle(el).width,radius:getComputedStyle(el).borderRadius })));
  expect(indicators.length).toBeGreaterThan(2); expect(indicators.every(s => s.width==='14px' && s.radius==='4px')).toBe(true);
});
