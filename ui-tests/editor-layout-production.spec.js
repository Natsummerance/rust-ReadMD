const { test, expect } = require('@playwright/test');
const path = require('node:path');
for (const [width, height, lang] of [[1160,820,'zh-CN'],[1024,680,'en'],[1024,680,'de'],[1024,680,'fr'],[1024,680,'ja'],[1024,680,'ar']]) {
  test(`editor commands and footer remain usable at ${width}×${height} in ${lang}`, async ({ page }) => {
    await page.setViewportSize({ width, height });
    await page.addInitScript(code => localStorage.setItem('readmd_language', code), lang);
    await page.goto('/');
    await page.waitForFunction(() => window.__readmdAppReady && window.ReadMDRecovery);
    await page.evaluate(async () => {
      await renderVirtual('clipboard', 'Layout-demo.md', '', '# Reading\n\nOne two three four five\n\n你好世界\n', []);
      state.pvLayout = 'none'; await toggleEdit();
    });
    await expect(page.locator('#edit-doc-stats')).toBeVisible();
    const toolbar = await page.locator('#edit-bar').boundingBox();
    expect(toolbar.height).toBeLessThanOrEqual(56);
    const footer = await page.locator('#edit-doc-stats').boundingBox();
    expect(footer.y).toBeGreaterThan(toolbar.y + toolbar.height);
    expect(footer.y + footer.height).toBeLessThanOrEqual(height);
    const controls = page.locator('#edit-bar button:visible');
    for (let n = 0; n < await controls.count(); n++) {
      const control = controls.nth(n), bounds = await control.boundingBox();
      expect(bounds.x).toBeGreaterThanOrEqual(0); expect(bounds.x + bounds.width).toBeLessThanOrEqual(width);
      expect(bounds.height).toBeGreaterThanOrEqual(44);
      expect(await control.evaluate(el => { const r = el.getBoundingClientRect(); const hit = document.elementFromPoint(r.x+r.width/2, r.y+r.height/2); return hit === el || el.contains(hit); })).toBe(true);
    }
    await page.locator('[data-menu="md-insert-menu"]').click();
    await page.locator('#btn-insert-table').click();
    await expect(page.locator('#table-modal')).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(page.locator('#table-modal')).toBeHidden();
    await page.evaluate(async () => {
      cmView.dispatch({ changes: { from: cmView.state.doc.length, insert: '\nLocal edit\n' } });
      await ReadMDRecovery.flush();
    });
    await expect(page.locator('#document-recovery-status')).toHaveText(await page.evaluate(() => i18n.t('storage.draftKept')));
    await page.evaluate(async () => i18n.setLanguage(i18n.currentLang === 'en' ? 'zh-CN' : 'en', false));
    await expect(page.locator('#document-recovery-status')).toHaveText(await page.evaluate(() => i18n.t('storage.draftKept')));
    await expect(page.locator('#pv-trigger')).toContainText(await page.evaluate(() => i18n.t('editor.previewNone')));
    await expect(page.locator('#btn-app-exit')).toHaveCount(0);
    if (process.env.READMD_AUDIT_SCREENSHOTS) await page.screenshot({ path: path.join(process.env.READMD_AUDIT_SCREENSHOTS, `editor-${width}-${lang}.png`) });
  });
}

test('one row adapts to window size and keeps original overflow commands usable', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('readmd_language', 'zh-CN'));
  await page.goto('/'); await page.waitForFunction(() => window.__readmdAppReady);
  await page.evaluate(async () => {
    await renderVirtual('clipboard','Toolbar-check.md','','# 标题\n\n内容',[]); await toggleEdit();
  });
  const moved = [];
  for (const width of [390,640,760,1024,1160,1920]) {
    await page.setViewportSize({ width, height:820 });
    await expect.poll(() => page.locator('#edit-bar').evaluate(el => el.scrollWidth <= el.clientWidth + 1)).toBe(true);
    const toolbar = await page.locator('#edit-bar').boundingBox();
    expect(toolbar.height).toBe(56);
    for (const id of ['edit-save','edit-cancel','pv-trigger']) {
      const button = page.locator('#' + id), r = await button.boundingBox();
      await expect(button).toBeVisible(); expect(r.x).toBeGreaterThanOrEqual(0); expect(r.x+r.width).toBeLessThanOrEqual(width);
      expect(r.y).toBeGreaterThanOrEqual(toolbar.y); expect(r.y+r.height).toBeLessThanOrEqual(toolbar.y+toolbar.height);
    }
    moved.push(await page.locator('#edit-overflow-menu .editor-overflow-group').count());
  }
  expect(moved[0]).toBeGreaterThan(moved[3]); expect(moved.at(-1)).toBe(0);
  await page.setViewportSize({ width:390, height:820 });
  await page.locator('#edit-overflow-trigger').click();
  await page.locator('#formula-open').click(); await expect(page.locator('#formula-modal')).toBeVisible();
  await page.keyboard.press('Escape');
  await page.locator('#edit-overflow-trigger').click();
  await page.locator('#edit-view-lines').click();
  await expect(page.locator('#edit-view-lines')).toHaveAttribute('aria-checked','true');
  await page.keyboard.press('Escape');
  await page.locator('#edit-overflow-trigger').click();
  await page.locator('#btn-insert-table').click(); await expect(page.locator('#table-modal')).toBeVisible();
});
