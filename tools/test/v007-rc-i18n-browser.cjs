'use strict';
// Opt-in, real kernel + production CSP + owned Windows-1252 file. No builds, models,
// downloads or native dialogs. The settings bridge is the existing QA adapter.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { openOwned, sha, until } = require('./owned-browser-host.cjs');

const root = path.resolve(__dirname, '../..');
const localeDirectory = path.join(root, 'assets/i18n');
const meta = JSON.parse(fs.readFileSync(path.join(localeDirectory, 'meta.json'), 'utf8'));
const locales = Object.keys(meta).sort();
const output = path.resolve(process.env.READMD_I18N_REPORT || path.join(root, 'build/v007-rc-i18n/browser'));
const binary = process.env.READMD_BIN;

async function run() {
  assert(binary && path.isAbsolute(binary), 'An explicit current READMD_BIN is required');
  assert(output.startsWith(root + path.sep), 'Evidence and temporary files must stay in this workspace');
  const report = {
    passed: false, native: false, productionCsp: true, externalRequestsAllowed: false,
    binarySha256: sha(fs.readFileSync(binary)), bootSha256: sha(fs.readFileSync(path.join(root, 'assets/readmd.boot.js'))),
    dictionarySha256: Object.fromEntries(locales.map(code => [code, sha(fs.readFileSync(path.join(localeDirectory, `${code}.json`)))])),
    locales: [], acceptedSave: null, errors: [],
    scope: 'Owned Windows-1252 file, actual cancel/write APIs and rendered language labels; headless Chromium only. No native speaker, screen reader, physical high-DPI or other-platform certification.',
  };
  let owned;
  // GBK byte input is detected as GB18030, which can also represent emoji.
  // This lone é byte selects the actual CP1252 path and cannot encode 🧪.
  const originalBytes = Buffer.from('# Café\n', 'latin1');
  try {
    owned = await openOwned(output, async (_api, paths) => {
      fs.writeFileSync(path.join(paths.documents, 'encoding-original.md'), originalBytes);
    }, { binary, playwright: process.env.READMD_PLAYWRIGHT });
    const { page } = owned;
    report.pageErrorStacks = [];
    page.on('pageerror', error => report.pageErrorStacks.push(String(error.stack || error)
      .replaceAll(root, '<workspace>').replace(/http:\/\/127\.0\.0\.1:\d+/g, '<owned-origin>')));
    const original = path.join(owned.documents, 'encoding-original.md');
    await page.evaluate(async file => { await loadFile(file); await toggleEdit(); }, original);
    await until(() => page.evaluate(() => Boolean(state.editing && cmView)), 'owned editor ready');
    const sourceEncoding = await page.evaluate(() => getActiveTab().encoding);
    assert.equal(sourceEncoding, 'cp1252', 'The source must actually use a limited legacy encoding');
    await page.evaluate(() => cmView.dispatch({ changes: { from: cmView.state.doc.length, insert: '\nKeep 🧪\n' } }));
    const edited = await page.evaluate(() => getEditContent());
    assert(edited.includes('🧪'));
    report.sourceEncoding = sourceEncoding;
    report.originalSha256 = sha(originalBytes);

    for (const code of locales) {
      const dictionary = JSON.parse(fs.readFileSync(path.join(localeDirectory, `${code}.json`), 'utf8'));
      assert.equal(await page.evaluate(code => i18n.setLanguage(code, false), code), true, code);
      const actual = await page.evaluate(() => ({
        language: document.documentElement.lang, direction: document.documentElement.dir,
        native: document.querySelector('#lang-current-label').textContent,
        save: document.querySelector('#edit-save').textContent,
        text: getEditContent(), dirty: getActiveTab().isDirty,
      }));
      assert.equal(actual.language, code);
      assert.equal(actual.direction, meta[code].dir);
      assert.equal(actual.native, meta[code].native);
      assert.equal(actual.save, dictionary['editor.save']);
      assert.equal(actual.text, edited);
      assert.equal(actual.dirty, true);

      await page.locator('#tab-conversion').click();
      for (const [selector, key] of [
        ['#btn-convert span', 'menu.convert'], ['#btn-web span', 'menu.web'],
        ['#btn-ocr span', 'menu.ocr'], ['#btn-pdf-edit span', 'pdf.title'],
      ]) {
        assert(await page.locator(selector).isVisible(), `${code}: visible ${key}`);
        assert.equal(await page.locator(selector).innerText(), dictionary[key], `${code}: ${key}`);
      }
      await page.locator('#tab-settings').click();
      for (const [selector, key] of [
        ['#btn-lang span', 'menu.lang'], ['#btn-plugin-menu span', 'menu.plugins'],
      ]) {
        assert(await page.locator(selector).isVisible(), `${code}: visible ${key}`);
        assert.equal(await page.locator(selector).innerText(), dictionary[key], `${code}: ${key}`);
      }

      await page.locator('#tab-files').click();
      await page.locator('#btn-document-history').click();
      await page.locator('#document-history-close').waitFor({ state: 'visible' });
      assert.equal(await page.locator('#document-history-close').getAttribute('aria-label'), dictionary['common.close'], `${code}: recovery close name`);
      await page.locator('#document-history-close').click();

      await page.evaluate(() => {
        window.__rcI18nSaveResult = undefined;
        window.__rcI18nSaveWork = saveEdit().then(value => { window.__rcI18nSaveResult = value; return value; });
      });
      await page.locator('#confirm-modal').waitFor({ state: 'visible' });
      const confirmation = await page.evaluate(() => ({
        title: document.querySelector('#confirm-title').textContent,
        message: document.querySelector('#confirm-message').textContent,
        action: document.querySelector('#confirm-action').textContent,
        cancel: document.querySelector('#confirm-cancel').textContent,
        direction: getComputedStyle(document.querySelector('#confirm-modal')).direction,
      }));
      report.inProgress = { code, confirmation };
      assert.equal(confirmation.title, dictionary['dialog.encodingTitle'], code);
      assert.equal(confirmation.action, dictionary['dialog.encodingUseUtf8'], code);
      assert.equal(confirmation.cancel, dictionary['common.cancel'], code);
      assert.equal(confirmation.direction, meta[code].dir, code);
      assert(confirmation.message.includes(sourceEncoding), `${code}: original encoding in dialog`);
      assert(!confirmation.message.includes('{char}'), `${code}: no dependency on an optional character response`);
      assert(!/dialog\.encoding/.test(JSON.stringify(confirmation)), `${code}: no raw encoding key`);
      if (['ar', 'he', 'ug', 'en', 'zh-CN'].includes(code)) {
        await page.setViewportSize({ width: 1024, height: 680 });
        await page.screenshot({ path: path.join(output, `encoding-${code}.png`), animations: 'disabled' });
        const geometry = await page.locator('#confirm-modal .close-confirm-box').evaluate(node => {
          const box = node.getBoundingClientRect();
          return { left: box.left, right: box.right, top: box.top, bottom: box.bottom, viewportWidth: innerWidth, viewportHeight: innerHeight, clientWidth: node.clientWidth, scrollWidth: node.scrollWidth };
        });
        assert(geometry.left >= 0 && geometry.right <= geometry.viewportWidth && geometry.top >= 0 && geometry.bottom <= geometry.viewportHeight, `${code}: confirmation fits narrow viewport`);
        assert(geometry.scrollWidth <= geometry.clientWidth + 1, `${code}: confirmation text has no horizontal overflow`);
        confirmation.geometry = geometry;
        await page.setViewportSize({ width: 1160, height: 820 });
      }
      await page.locator('#confirm-cancel').click();
      assert.equal(await page.evaluate(() => window.__rcI18nSaveWork), false, `${code}: cancel result`);
      assert.deepEqual(fs.readFileSync(original), originalBytes, `${code}: cancel preserves original bytes`);
      assert.deepEqual(await page.evaluate(() => ({ text: getEditContent(), dirty: getActiveTab().isDirty, encoding: getActiveTab().encoding })), { text: edited, dirty: true, encoding: sourceEncoding }, `${code}: cancel preserves editor`);
      report.locales.push({ code, dir: actual.direction, menuLabels: true, recoveryCloseName: true, encodingCancel: true, originalBytesPreserved: true, confirmation });
      console.log(`i18n browser: ${code} passed`);
    }

    await page.evaluate(async () => {
      await i18n.setLanguage('en', false);
      window.__rcI18nSaveWork = saveEdit();
    });
    await page.locator('#confirm-modal').waitFor({ state: 'visible' });
    await page.locator('#confirm-action').click();
    assert.equal(await page.evaluate(() => window.__rcI18nSaveWork), true);
    assert.equal(fs.readFileSync(original, 'utf8'), edited);
    assert.equal(await page.evaluate(() => getActiveTab().encoding), 'utf-8');
    assert.equal(await page.evaluate(() => getActiveTab().isDirty), false);
    report.acceptedSave = { encoding: 'utf-8', bytesSha256: sha(fs.readFileSync(original)), exactContent: true, dirty: false };
    delete report.inProgress;
    assert.deepEqual(owned.errors, [], 'No page errors');
    report.passed = true;
  } catch (error) {
    report.errors.push(String(error.stack || error).replaceAll(root, '<workspace>'));
    throw error;
  } finally {
    if (owned) {
      report.pageErrors = owned.errors;
      await owned.close();
      for (const directory of [owned.profile, owned.documents]) fs.rmSync(directory, { recursive: true, force: true });
    }
    fs.mkdirSync(output, { recursive: true });
    fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2) + '\n');
  }
  console.log(`i18n browser: passed ${report.locales.length} locales and accepted UTF-8 save`);
}

run().catch(error => { console.error(error.stack || error); process.exitCode = 1; });
