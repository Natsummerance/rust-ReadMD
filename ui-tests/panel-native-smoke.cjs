'use strict';
// Opt-in Windows/WebView verification. Use a private fixture profile and
// synthetic content; never open a system file picker or copy real documents.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const { chromium } = require('playwright');
const { startNative } = require('./native-session.cjs');
async function until(predicate, timeout = 10000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await predicate()) return;
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  throw Error('Native state did not settle within the time limit');
}
(async () => {
  if (!process.env.READMD_PANEL_NATIVE_DATA || !process.env.READMD_PANEL_NATIVE_REPORT) throw Error('Provide isolated fixture and report paths');
  const data = path.resolve(process.env.READMD_PANEL_NATIVE_DATA), root = path.resolve(__dirname, '..');
  fs.mkdirSync(data, { recursive:true });
  const document = path.join(data, 'panel-native-fixture.md');
  const content = '# 界面验收\n\n这是一份合成文档，用于检查排版、交互与保存。\n';
  fs.writeFileSync(document, content);
  const started = Date.now();
  const session = await startNative(root, data, chromium, Number(process.env.READMD_PANEL_NATIVE_PORT || 28798));
  const page = session.page, errors = [], checks = [];
  page.on('pageerror', error => errors.push(error.message));
  const check = async (name, action) => { await action(); checks.push({ name, passed:true }); };
  try {
    await page.emulateMedia({ reducedMotion:'reduce' });
    await page.evaluate(async file => { await i18n.setLanguage('zh-CN', false); await loadFile(file); state.pvLayout = 'none'; await toggleEdit(); }, document);
    await check('cancel-table-without-editing', async () => {
      await page.locator('[data-menu="md-insert-menu"]').click(); await page.locator('#btn-insert-table').click();
      assert.equal(await page.evaluate(() => hasUnsavedEditorChanges()), false);
      await page.keyboard.press('Escape');
      assert.equal(await page.evaluate(() => cmView.state.doc.toString()), content);
    });
    await check('insert-once-save-and-undo', async () => {
      await page.evaluate(() => openCodeChunkModal()); await page.locator('#code-chunk-lang').selectOption('javascript');
      await page.locator('#code-chunk-code').fill('console.log("NATIVE_ONE_INSERT");');
      await page.locator('#code-chunk-insert').click();
      assert.equal((await page.evaluate(() => getEditContent())).split('NATIVE_ONE_INSERT').length - 1, 1);
      assert.equal(await page.evaluate(() => !!document.activeElement.closest('#edit-cm')), true, 'Insertion should return focus to the editor');
      await page.keyboard.press('Control+z'); assert.equal(await page.evaluate(() => getEditContent()), content);
      assert.equal(fs.readFileSync(document, 'utf8'), content);
    });
    await check('all-46-languages-keep-draft-and-one-row', async () => {
      await page.evaluate(() => cmView.dispatch({ changes:{ from:cmView.state.doc.length, insert:'\n保留编辑草稿\n' } }));
      const draft = await page.evaluate(() => getEditContent());
      const languages = await page.evaluate(() => Object.keys(i18n.meta));
      assert.equal(languages.length, 46);
      for (const lang of languages) {
        await page.evaluate(lang => i18n.setLanguage(lang, false), lang);
        assert.equal(await page.evaluate(() => getEditContent()), draft);
        const bounds = await page.locator('#edit-bar').boundingBox(); assert.ok(bounds.height <= 56.5);
      }
      await page.evaluate(() => i18n.setLanguage('zh-CN', false));
    });
    await check('real-image-save-insert-and-undo', async () => {
      const before = await page.evaluate(() => getEditContent());
      await page.evaluate(() => {
        openImgModal(); const canvas = document.createElement('canvas'); canvas.width = 80; canvas.height = 50;
        canvas.getContext('2d').fillRect(0, 0, 80, 50); loadImgSrc(canvas.toDataURL());
      });
      await until(() => page.evaluate(() => !document.getElementById('img-insert').disabled));
      await page.locator('#img-insert').click();
      await until(() => page.evaluate(() => document.getElementById('img-modal').classList.contains('hidden')));
      assert.match(await page.evaluate(() => getEditContent()), /!\[.*?\]\(.*?\.png\)/);
      assert.equal(await page.evaluate(() => !!document.activeElement.closest('#edit-cm')), true);
      await page.keyboard.press('Control+z'); assert.equal(await page.evaluate(() => getEditContent()), before);
      assert.equal(fs.readFileSync(document, 'utf8'), content);
    });
    await check('real-lan-share-start-copy-state-stop', async () => {
      await page.evaluate(() => openShareModal()); await page.locator('#share-start').click();
      await until(() => page.evaluate(() => !document.getElementById('share-copy').classList.contains('hidden')));
      assert.equal(await page.locator('#share-qr img').count(), 1);
      await page.locator('#share-stop').click(); await until(() => page.evaluate(() => !document.getElementById('share-start').disabled));
      assert.equal(await page.locator('#share-copy').isVisible(), false);
      await page.locator('#share-close').click();
    });
    await check('three-themes-advanced-conversion-and-image-empty-state', async () => {
      for (const theme of ['light','dark','sepia']) {
        await page.evaluate(theme => { state.theme = theme; applySettings(); openConvertModal(); document.querySelector('#convert-modal details').open = true; }, theme);
        assert.equal(await page.evaluate(() => document.body.dataset.theme), theme);
        await page.locator('#convert-overwrite').check(); await page.locator('#convert-overwrite').uncheck();
        await page.keyboard.press('Escape');
      }
      await page.evaluate(() => { state.theme = 'light'; applySettings(); openImgModal(); });
      assert.equal(await page.locator('#img-rot-l').isDisabled(), true);
      await page.locator('#img-close').click();
    });
    await check('native-screenshot-and-preserved-file', async () => {
      await page.evaluate(() => openCodeChunkModal());
      await page.screenshot({ path: path.join(path.dirname(process.env.READMD_PANEL_NATIVE_REPORT), 'panel-native.png') });
      assert.equal(fs.readFileSync(document, 'utf8'), content); assert.deepEqual(errors, []);
    });
    const result = { passed:true, realNativeCsp:true, elapsedMs:Date.now()-started, languages:46, themes:3, scenarios:checks, pageErrors:errors.length, syntheticFilePreserved:true };
    fs.writeFileSync(process.env.READMD_PANEL_NATIVE_REPORT, JSON.stringify(result, null, 2)); console.log(JSON.stringify(result));
  } finally {
    await page.evaluate(() => stopShare()).catch(() => {});
    await session.browser.close(); if (session.server.child.exitCode === null) session.server.child.kill();
  }
})().catch(error => { console.error(error.message); process.exitCode = 1; });
