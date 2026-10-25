const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const { chromium } = require('playwright'), { start } = require('./lib/serve-static.cjs');

(async () => {
  const local = await start(path.resolve(__dirname, '../dist'));
  const report = { passed: false, cases: [], errors: [] };
  let browser;
  try {
    browser = await chromium.launch({ headless: true, ...(process.env.READMD_AI_SITE_CHANNEL ? { channel: process.env.READMD_AI_SITE_CHANNEL } : {}) });
    for (const prefix of ['', 'zh-cn/', 'zh-tw/', 'ja/']) for (const scheme of ['light', 'dark']) {
      const page = await browser.newPage({ viewport: { width: 1280, height: 800 }, colorScheme: scheme });
      await page.route('https://api.github.com/**', route => route.abort());
      page.on('pageerror', error => report.errors.push(error.message));
      page.on('response', response => { if (response.url().startsWith(local.url) && response.status() >= 400) report.errors.push(response.status() + ' ' + new URL(response.url()).pathname); });
      page.on('request', request => { if (/\.(?:mp4|webm|mov)(?:[?#]|$)/i.test(request.url())) report.errors.push('Unexpected recording request: ' + request.url()); });
      await page.goto(local.url + '/' + prefix + 'ai-autocomplete/');
      const demo = page.locator('[data-ai-illustration="offline"]');
      await page.waitForFunction(() => document.querySelector('[data-ai-illustration]')?.dataset.aiReady === 'true');
      assert.equal(await page.locator('video,source,iframe').count(), 0);
      assert.ok((await demo.locator('[data-ai-disclosure]').textContent()).trim());
      assert.ok((await demo.locator('[data-ai-context]').textContent()).trim());
      const editor = demo.locator('[data-ai-editor]'), draft = await demo.locator('[data-ai-draft]').textContent();
      const suggestion = await demo.locator('[data-ai-suggestion]').textContent();
      const state = () => demo.getAttribute('data-ai-state');
      await editor.focus(); await editor.press('Tab');
      assert.equal(await state(), 'accepted');
      assert.equal(await demo.locator('[data-ai-inserted]').textContent(), suggestion);
      assert.equal(await demo.locator('[data-ai-suggestion]').isVisible(), false);
      assert.equal(await demo.locator('[data-ai-draft]').textContent(), draft);
      await editor.press('Tab');
      assert.equal(await editor.evaluate(node => document.activeElement === node), false, 'Tab must leave after accepting');
      await demo.locator('[data-ai-action="undo"]').click();
      assert.equal(await state(), 'restored');
      assert.equal(await demo.locator('[data-ai-inserted]').textContent(), '');
      assert.equal(await demo.locator('[data-ai-draft]').textContent(), draft);
      assert.equal(await demo.locator('[data-ai-action="undo"]').isDisabled(), true);
      await demo.locator('[data-ai-action="reset"]').click(); await editor.press('Escape');
      assert.equal(await state(), 'dismissed');
      assert.equal(await demo.locator('[data-ai-inserted]').textContent(), '');
      await demo.locator('[data-ai-action="reset"]').click();
      await page.emulateMedia({ reducedMotion: 'reduce' }); await page.setViewportSize({ width: 390, height: 844 });
      await editor.press('Tab'); await editor.press(process.platform === 'darwin' ? 'Meta+z' : 'Control+z');
      assert.equal(await state(), 'restored', 'reduced motion keeps controls usable');
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
      report.cases.push({ prefix, scheme, accepted: true, dismissed: true, oneUndo: true, tabLeaves: true, reducedMotion: true, mobileOverflow: false });
      await page.close();
    }
    for (const prefix of ['', 'zh-cn/', 'zh-tw/', 'ja/']) {
      const page = await browser.newPage({ viewport: { width: 390, height: 844 }, javaScriptEnabled: false });
      await page.goto(local.url + '/' + prefix + 'ai-autocomplete/');
      assert.ok(await page.locator('[data-ai-disclosure]').isVisible());
      assert.ok(await page.locator('[data-ai-suggestion]').isVisible());
      assert.ok(await page.locator('noscript').isVisible());
      assert.equal(await page.locator('[data-ai-action="accept"]').isDisabled(), true);
      report.cases.push({ prefix, noJavaScriptReadable: true }); await page.close();
    }
    assert.deepEqual(report.errors, []); report.passed = true;
    console.log(JSON.stringify(report));
    if (process.env.READMD_AI_SITE_REPORT) fs.writeFileSync(process.env.READMD_AI_SITE_REPORT, JSON.stringify(report, null, 2));
  } finally {
    await browser?.close(); await new Promise(resolve => local.server.close(resolve));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
