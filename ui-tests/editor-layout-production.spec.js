const {waitForPage}=require('./page-condition.cjs');
const { test, expect } = require('@playwright/test');
const path = require('node:path');
for (const [width, height, lang] of [[1160,820,'zh-CN'],[1024,680,'en'],[1024,680,'de'],[1024,680,'fr'],[1024,680,'ja'],[1024,680,'ar']]) {
  test(`editor commands and sidebar data remain usable at ${width}×${height} in ${lang}`, async ({ page }) => {
    await page.setViewportSize({ width, height });
    await page.addInitScript(code => localStorage.setItem('readmd_language', code), lang);
    await page.goto('/');
    await waitForPage(page, () => window.__readmdAppReady && window.ReadMDRecovery);
    await page.evaluate(async () => {
      await renderVirtual('clipboard', 'Layout-demo.md', '', '# Reading\n\nOne two three four five\n\n你好世界\n', []);
      state.pvLayout = 'none'; await toggleEdit();
    });
    await page.locator('#btn-toc').click();
    await expect(page.locator('#status-summary')).toBeVisible();
    const toolbar = await page.locator('#edit-bar').boundingBox();
    expect(toolbar.height).toBeLessThanOrEqual(56);
    const side = await page.locator('#side').boundingBox();
    const titlebar = await page.locator('#app-titlebar').boundingBox();
    expect(side.y).toBeCloseTo(titlebar.y + titlebar.height, 0);
    expect(toolbar.x >= side.x + side.width - 1 || toolbar.x + toolbar.width <= side.x + 1).toBe(true);
    const footer = await page.locator('#status-summary').boundingBox();
    expect(footer.y).toBeGreaterThan(toolbar.y + toolbar.height);
    await expect(page.locator('#status-summary')).toHaveCSS('white-space', 'nowrap');
    await expect(page.locator('#statusbar')).not.toContainText('●');
    expect(footer.y + footer.height).toBeLessThanOrEqual(height);
    const controls = page.locator('#edit-bar button:visible');
    for (let n = 0; n < await controls.count(); n++) {
      const control = controls.nth(n), bounds = await control.boundingBox();
      expect(bounds.x).toBeGreaterThanOrEqual(0); expect(bounds.x + bounds.width).toBeLessThanOrEqual(width);
      expect(bounds.height).toBeGreaterThanOrEqual(44);
      expect(await control.evaluate(el => { const r = el.getBoundingClientRect(); const hit = document.elementFromPoint(r.x+r.width/2, r.y+r.height/2); return hit === el || el.contains(hit); })).toBe(true);
    }
    const view = page.locator('#edit-view-trigger');
    await view.click();
    const menu = page.locator('#edit-view-menu');
    await expect(menu).toBeVisible();
    const geometry = await menu.evaluate(element => {
      const column = document.getElementById('document-column').getBoundingClientRect();
      const frame = element.getBoundingClientRect();
      const buttons = Array.from(element.querySelectorAll('.pv-btn'), button => {
        const rect = button.getBoundingClientRect();
        const text = document.createRange();
        text.selectNodeContents(button);
        const glyph = text.getBoundingClientRect();
        return { x: rect.x, y: rect.y, width: rect.width, height: rect.height,
          centerError: Math.abs(rect.left + rect.width / 2 - glyph.left - glyph.width / 2) };
      });
      const ticks = Array.from(element.querySelectorAll('.view-option'), row => {
        const mark = row.firstElementChild.getBoundingClientRect();
        return mark.left + mark.width / 2;
      });
      return { left: frame.left, right: frame.right, bottom: frame.bottom, columnLeft: column.left,
        columnRight: column.right, fits: element.scrollWidth <= element.clientWidth + 1, buttons, ticks };
    });
    expect(geometry.left).toBeGreaterThanOrEqual(geometry.columnLeft);
    expect(geometry.right).toBeLessThanOrEqual(geometry.columnRight);
    expect(geometry.bottom).toBeLessThanOrEqual(height);
    expect(geometry.fits).toBe(true);
    expect(geometry.buttons).toHaveLength(5);
    for (const button of geometry.buttons) {
      expect(button.width).toBeCloseTo(geometry.buttons[0].width, 0);
      expect(button.height).toBeGreaterThanOrEqual(44);
      expect(button.centerError).toBeLessThan(1);
    }
    expect(Math.max(...geometry.ticks) - Math.min(...geometry.ticks)).toBeLessThan(1);
    await page.keyboard.press('Escape');
    await expect(menu).toBeHidden();
    await expect(view).toBeFocused();
    await expect(page.locator('#btn-details')).toHaveCount(0);

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
    await expect(page.locator('[data-pv="none"]')).toHaveAttribute('aria-pressed','true');
    await expect(page.locator('#btn-app-exit')).toHaveCount(0);
    if (process.env.READMD_AUDIT_SCREENSHOTS) await page.screenshot({ path: path.join(process.env.READMD_AUDIT_SCREENSHOTS, `editor-${width}-${lang}.png`) });
  });
}

test('one row adapts to window size and keeps original overflow commands usable', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('readmd_language', 'zh-CN'));
  await page.goto('/'); await waitForPage(page, () => window.__readmdAppReady);
  await page.evaluate(async () => {
    await renderVirtual('clipboard','Toolbar-check.md','','# 标题\n\n内容',[]); await toggleEdit();
  });
  const moved = [];
  for (const width of [390,640,760,1024,1160,1920]) {
    await page.setViewportSize({ width, height:820 });
    await expect.poll(() => page.locator('#edit-bar').evaluate(el => el.scrollWidth <= el.clientWidth + 1)).toBe(true);
    const toolbar = await page.locator('#edit-bar').boundingBox();
    expect(toolbar.height).toBeLessThanOrEqual(56);
    for (const id of ['edit-view-trigger','edit-save','edit-cancel']) {
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
  await page.setViewportSize({ width:390, height:360 });
  await page.locator('#edit-view-trigger').click();
  const previewMenu = page.locator('#edit-view-menu');
  const previewBounds = await previewMenu.boundingBox();
  expect(previewBounds.x).toBeGreaterThanOrEqual(0);
  expect(previewBounds.x + previewBounds.width).toBeLessThanOrEqual(390);
  expect(previewBounds.y + previewBounds.height).toBeLessThanOrEqual(360);
  expect(await previewMenu.evaluate(el => el.scrollHeight > el.clientHeight)).toBe(true);
  await page.locator('#edit-view-lines').click();
  await expect(page.locator('#edit-view-lines')).toHaveAttribute('aria-checked','true');
  await page.keyboard.press('Escape');
  await page.locator('#edit-overflow-trigger').click();
  await page.locator('#btn-insert-table').click(); await expect(page.locator('#table-modal')).toBeVisible();
});
