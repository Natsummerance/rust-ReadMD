const {waitForPage}=require('./page-condition.cjs');
const { test, expect } = require('@playwright/test');
const fs = require('node:fs');
const path = require('node:path');
const locales = fs.readdirSync(path.join(__dirname, '../assets/i18n')).filter(f => f.endsWith('.json') && f !== 'meta.json').map(f => f.slice(0, -5));

test('all shipped languages refresh editor state without losing the current document', async ({ page }) => {
  test.setTimeout(90000);
  await page.goto('/');
  await waitForPage(page, () => window.__readmdAppReady && window.ReadMDRecovery);
  await page.evaluate(async () => {
    await renderVirtual('clipboard', 'Language-check.md', '', '# Public language fixture\n\nOne two three\n', []);
    await toggleEdit(); setPvLayout('right');
    cmView.dispatch({ changes:{ from:cmView.state.doc.length, insert:'\n修改保留\n' } });
    await ReadMDRecovery.flush();
  });
  const content = await page.evaluate(() => getEditContent());
  for (const locale of locales) {
    await page.evaluate(async locale => i18n.setLanguage(locale, false), locale);
    await expect(page.locator('#edit-save')).toHaveText(await page.evaluate(() => i18n.t('editor.save')));
    await expect(page.locator('[data-pv="right"]')).toHaveAttribute('title', await page.evaluate(() => i18n.t('editor.previewRight')));
    await expect(page.locator('[data-pv="right"]')).toHaveAttribute('aria-pressed', 'true');
    await expect(page.locator('#btn-toc')).toHaveAttribute('aria-label', await page.evaluate(() => i18n.t('shell.sidebar')));
    await expect(page.locator('#document-recovery-status')).toHaveText(await page.evaluate(() => i18n.t('storage.draftKept')));
    expect(await page.evaluate(() => ({ text:getEditContent(), layout:state.pvLayout, language:i18n.currentLang }))).toEqual({ text:content, layout:'right', language:locale });
  }
  await page.evaluate(async () => i18n.setLanguage('zh-CN', false));
  await expect(page.locator('[data-pv="right"]')).toHaveAttribute('title', /预览/);
  await expect(page.locator('[data-pv="right"]')).not.toHaveAttribute('title', /Preview|Close Preview/);
});
