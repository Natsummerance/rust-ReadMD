const {test, expect} = require('@playwright/test');
const {waitForPage} = require('./page-condition.cjs');

for (const [width,height] of [[390,740],[1024,680],[1160,820],[1440,900]]) {
  test('titlebar keeps one tab track and reachable controls at '+width+'×'+height, async ({page}) => {
    await page.setViewportSize({width,height}); await page.goto('/');
    await waitForPage(page, () => window.__readmdAppReady);
    const name = '研究记录与证据分析_'.repeat(12)+'修订稿.md';
    await page.evaluate(async name => {
      await window.__readmdSessionReady;
      await renderVirtual('clipboard',name,'','# Evidence\n\nDocument title layout.',[]);
    }, name);
    const tab = page.locator('#doc-tabs-bar .tab-item.active');
    await expect(tab).toHaveAttribute('title', name);
    await expect(tab.locator('.tab-title')).toHaveText(name);
    await expect(page.locator('#app-titlebar img')).toHaveCount(0);
    const geometry = await page.evaluate(() => {
      const rect = id => $(id).getBoundingClientRect();
      return { edit:rect('btn-edit').right, tabs:rect('doc-tabs-bar').left, tabsRight:rect('doc-tabs-container').right,
        ai:rect('btn-ai').left, overflow:document.documentElement.scrollWidth-innerWidth,
        shortened:$('doc-tabs-bar').querySelector('.tab-title').scrollWidth > $('doc-tabs-bar').querySelector('.tab-title').clientWidth };
    });
    expect(geometry.edit).toBeLessThanOrEqual(geometry.tabs);
    expect(geometry.tabsRight).toBeLessThanOrEqual(geometry.ai);
    expect(geometry.overflow).toBeLessThanOrEqual(1);
    expect(geometry.shortened).toBe(true);
    await page.locator('#btn-home').click();
    await expect(page.locator('#welcome')).toBeVisible();
    await expect(page.locator('#doc-tabs-bar .tab-title').filter({hasText:name})).toBeVisible();
  });
}

for (const [width,height] of [[1024,680],[1160,820]]) {
  test('reading settings open from the rail at '+width+'×'+height, async ({page}) => {
    await page.setViewportSize({width,height}); await page.goto('/');
    await waitForPage(page, () => window.__readmdAppReady);
    await page.evaluate(async () => {
      await window.__readmdSessionReady;
      await renderVirtual('convert','阅读设置.md','','# 阅读设置\n\n检查字体、布局和专注阅读。',[]);
    });
    const font = page.locator('#side-font-slot #rd-prefs-btn');
    await expect(font).toBeVisible();
    const before = await page.evaluate(() => state.fontSize);
    await font.click();
    const panel = page.locator('#rd-prefs'); await expect(panel).toBeVisible();
    const box = await panel.boundingBox();
    expect(box.x).toBeGreaterThanOrEqual(0); expect(box.x+box.width).toBeLessThanOrEqual(width);
    expect(box.y).toBeGreaterThanOrEqual(0); expect(box.y+box.height).toBeLessThanOrEqual(height);
    await page.locator('.rd-size-btn--inc').click();
    expect(await page.evaluate(() => state.fontSize)).toBeGreaterThan(before);
    await page.keyboard.press('Escape'); await expect(panel).toBeHidden(); await expect(font).toBeFocused();
    await page.locator('#btn-edit').click(); await expect(font).toBeHidden();
    await page.locator('#edit-cancel').click(); await expect(font).toBeVisible();
    await font.click(); await page.locator('#btn-home').click(); await expect(panel).toBeHidden();
    await page.locator('#doc-tabs-bar .tab-item').first().click(); await expect(font).toBeVisible();
  });
}
