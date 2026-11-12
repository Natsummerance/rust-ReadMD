'use strict';
// Opt-in real-product presentation check. Only owned synthetic files, real
// Rust document analysis and the settings-only browser bootstrap are used.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const { ROOT, sha, until, openOwned } = require('../tools/test/owned-browser-host.cjs');
const { options } = require('../tools/test/v006-table-clipboard-kernel.cjs');

const files = ['assets/index.html', 'assets/css/shell-deferred.css', 'assets/readmd.boot.js',
  'assets/js/reader/fixes.js', 'assets/app.js', 'assets/js/core/modal.js', 'assets/css/panels.css'];
const binding = () => Object.fromEntries(files.map(file => [file, sha(fs.readFileSync(path.join(ROOT, file)))]));
async function settle(page) {
  await page.evaluate(async () => Promise.all(document.getAnimations()
    .filter(animation => Number.isFinite(animation.effect?.getTiming().iterations))
    .map(animation => animation.finished.catch(() => {}))));
}
async function main() {
  const args = options(process.argv.slice(2));
  assert(args.bin && !args.base, 'Explicit prebuilt kernel required');
  const playwright = args.playwright || require.resolve('playwright', { paths: [ROOT + '/ui-tests'] });
  const { expect } = require('@playwright/test');
  const report = { passed: false, driverSha256: sha(fs.readFileSync(__filename)),
    binarySha256: sha(fs.readFileSync(args.bin)), sourceBefore: binding(), checks: [], cases: [],
    realCspAndToken: true, realDocumentAnalysis: true, actualHeadlessBrowser: true,
    modelsRequested: false, nativeCertified: false, saveAndReviewBusinessCertified: false };
  let test, phase = 'bootstrap';
  const forbiddenRequests = [];
  try {
    test = await openOwned(args.evidence, async (_api, paths) => {
      fs.writeFileSync(path.join(paths.profile, 'settings.json'), JSON.stringify({ lang: 'zh-CN', autoCheckUpdate: false }));
      const long = '#Synthetic diagnostic document\n\n' + Array.from({ length: 42 }, (_, index) =>
        `[Synthetic missing target ${index}](missing-${index}-${'long-target-'.repeat(12)}.md)`).join('\n\n') + '\n';
      fs.writeFileSync(path.join(paths.documents, 'long.md'), long);
      fs.writeFileSync(path.join(paths.documents, 'empty.md'), '# Synthetic clean document\n\nA clean paragraph.\n');
    }, { binary: args.bin, playwright });
    const { page } = test;
    page.on('request', request => {
      const route = new URL(request.url()).pathname;
      const body = request.postDataJSON?.();
      if (request.method() === 'POST' && (/^\/api\/ai\/(chat|completion)$/.test(route) ||
        route === '/api/ai/tasks' && ['start', 'resume'].includes(body?.action))) forbiddenRequests.push(route);
    });
    await page.evaluate(async () => { setEditorPref('aiCompletion', false); await ReadMDShellLoader.load(); });
    async function openFix() {
      if (await page.locator('#btn-more').getAttribute('aria-selected') !== 'true') await page.locator('#btn-more').click();
      const group = page.locator('.more-group').filter({ has: page.locator('#btn-fix') });
      if (!await group.evaluate(node => node.classList.contains('open'))) await group.locator('.more-group-header').click();
      await page.locator('#btn-fix').click();
    }
    const cases = [
      { name: 'zh-long-light-1024', file: 'long', lang: 'zh-CN', theme: 'light', width: 1024, height: 680 },
      { name: 'zh-long-dark-1160', file: 'long', lang: 'zh-CN', theme: 'dark', width: 1160, height: 820 },
      { name: 'ar-long-light-1024', file: 'long', lang: 'ar', theme: 'light', width: 1024, height: 680 },
      { name: 'ar-empty-dark-1024', file: 'empty', lang: 'ar', theme: 'dark', width: 1024, height: 680 },
      { name: 'zh-empty-light-1024', file: 'empty', lang: 'zh-CN', theme: 'light', width: 1024, height: 680 },
    ];
    for (const item of cases) {
      phase = item.name;
      await page.setViewportSize({ width: item.width, height: item.height });
      await page.evaluate(async ({ file, lang, theme }) => {
        await i18n.setLanguage(lang, false);
        state.theme = theme; applySettings();
        await loadFile(file);
      }, { file: path.join(test.documents, item.file + '.md'), lang: item.lang, theme: item.theme });
      await until(() => page.locator('#toast').evaluate(node => node.classList.contains('hidden')), 'original file-open toast expiry');
      const analyzed = page.waitForResponse(response => new URL(response.url()).pathname === '/api/document/analyze').catch(error => error);
      await openFix();
      const response = await analyzed;
      if (response instanceof Error) throw response;
      assert.equal(response.status(), 200);
      const analysis = await response.json();
      assert.equal(analysis.ok, true);
      assert.equal(analysis.diagnostics.length, item.file === 'long' ? 42 : 0, 'Independent real-analysis fixture count');
      await until(() => page.locator('#fix-list .document-inspection-issue').count()
        .then(count => count === analysis.diagnostics.length), 'real diagnostic rows');
      await settle(page);
      const metrics = await page.evaluate(() => {
        const rect = element => { const r = element.getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height, right: r.right, bottom: r.bottom }; };
        const box = document.getElementById('fix-box'), list = document.getElementById('fix-list');
        const note = box.querySelector('.fix-note'), controls = ['fix-ai-btn', 'fix-close', 'fix-save'];
        const buttons = Object.fromEntries(controls.map(id => { const node = document.getElementById(id), css = getComputedStyle(node); return [id,
          { ...rect(node), background: css.backgroundColor, color: css.color, name: node.getAttribute('aria-label') || node.textContent.trim() }]; }));
        const css = getComputedStyle(box), save = getComputedStyle(document.getElementById('fix-save'));
        return { box: rect(box), note: rect(note), list: { ...rect(list), clientWidth: list.clientWidth, scrollWidth: list.scrollWidth, clientHeight: list.clientHeight, scrollHeight: list.scrollHeight },
          buttons, radius: css.borderTopLeftRadius, padding: css.paddingTop,
          noteBeforeList: Boolean(note.compareDocumentPosition(list) & Node.DOCUMENT_POSITION_FOLLOWING),
          savePrimary: document.getElementById('fix-save').classList.contains('primary'),
          aiPrimary: document.getElementById('fix-ai-btn').classList.contains('primary'),
          dialogDescription: document.getElementById('fix-modal').getAttribute('aria-describedby'),
          direction: getComputedStyle(box).direction, theme: document.body.dataset.theme,
          bodyOverflow: document.documentElement.scrollWidth > innerWidth + 1,
          listFirstBorder: list.firstElementChild ? getComputedStyle(list.firstElementChild).borderInlineStartWidth : null,
          saveBackground: save.backgroundColor,
          focused: document.activeElement.id };
      });
      report.cases.push({ ...item, diagnosticCount: analysis.diagnostics.length, metrics });
      assert(metrics.noteBeforeList && metrics.note.bottom <= metrics.list.y + 1, 'Reading-scope explanation must precede the scrolling diagnostics');
      assert(metrics.savePrimary && !metrics.aiPrimary, 'Save As is primary; AI is secondary');
      assert.equal(metrics.radius, '16px');
      assert.equal(metrics.padding, '24px');
      assert(metrics.box.width <= 560.01 && metrics.box.x >= 15 && metrics.box.right <= item.width - 15);
      assert(metrics.box.y >= 15 && metrics.box.bottom <= item.height - 15);
      assert(!metrics.bodyOverflow && metrics.list.scrollWidth <= metrics.list.clientWidth + 1, 'No horizontal overflow');
      assert.equal(metrics.listFirstBorder, '0px', 'No framed diagnostic cards');
      for (const button of Object.values(metrics.buttons)) {
        assert(button.width >= 44 && button.height >= 44, 'Every real hit target is at least 44px');
        assert(button.x >= metrics.box.x && button.right <= metrics.box.right + 1 && button.bottom <= metrics.box.bottom + 1);
      }
      assert.equal(metrics.direction, item.lang === 'ar' ? 'rtl' : 'ltr');
      assert.equal(metrics.theme, item.theme);
      const dictionary = JSON.parse(fs.readFileSync(path.join(ROOT, 'assets/i18n', item.lang + '.json')));
      for (const [id, key] of [['fix-ai-btn', 'fixes.aiFix'], ['fix-save', 'app.saveAs'], ['fix-close', 'toolbar.close']]) {
        await expect(page.locator('#' + id)).toHaveAccessibleName(dictionary[key]);
      }
      await expect(page.locator('#fix-ai-btn')).toHaveAccessibleDescription(dictionary['review.pending']);
      await expect(page.locator('#fix-modal')).toHaveAccessibleName(dictionary['dialog.fixTitle']);
      await expect(page.locator('#fix-modal')).toHaveAccessibleDescription(dictionary['fixes.note']);
      if (item.file === 'long') {
        assert(metrics.list.scrollHeight > metrics.list.clientHeight + 100, 'Long diagnostics must scroll inside the list');
        const last = page.locator('#fix-list li').last();
        await last.scrollIntoViewIfNeeded();
        const footerVisible = await page.locator('#fix-save').isVisible();
        assert(footerVisible);
        assert(await page.evaluate(() => { const b = $('fix-save').getBoundingClientRect(), l = $('fix-list').getBoundingClientRect(); return b.y >= l.bottom; }), 'Footer stays outside list scroll');
      } else assert(await page.locator('#fix-list .empty').isVisible(), 'Clean-document empty state remains visible');
      await page.locator('#fix-close').focus();
      await page.keyboard.press('Tab');
      assert(await page.locator('#fix-modal').evaluate(node => node.contains(document.activeElement)), 'Tab stays in original modal trap');
      await page.keyboard.press('Shift+Tab');
      assert(await page.locator('#fix-modal').evaluate(node => node.contains(document.activeElement)), 'Reverse Tab stays in original modal trap');
      await page.keyboard.press('Escape');
      await until(() => page.locator('#fix-modal').evaluate(node => node.classList.contains('hidden')), 'original Escape close');
      const menuReturn = await page.evaluate(() => ({ active: document.activeElement.id || document.activeElement.tagName,
        openerVisible: $('btn-fix').getClientRects().length > 0,
        moreSelected: $('btn-more').getAttribute('aria-selected') === 'true',
        remainingInert: document.querySelectorAll('[data-rm-inert]').length }));
      report.cases.at(-1).menuReturn = menuReturn;
      assert.equal(menuReturn.remainingInert, 0, 'Original modal manager releases the background');
      report.checks.push(item.name + ': real analysis, hierarchy, scroll, hit targets, locale, theme, ARIA and original modal keyboard close');
    }
    phase = 'backdrop and close button';
    await openFix();
    await settle(page);
    await page.locator('#fix-close').click();
    await until(() => page.locator('#fix-modal').evaluate(node => node.classList.contains('hidden')), 'original close button');
    await openFix();
    await settle(page);
    await page.mouse.click(16, 16);
    await until(() => page.locator('#fix-modal').evaluate(node => node.classList.contains('hidden')), 'original backdrop close');
    phase = 'visible opener focus return';
    await page.locator('#btn-more').focus();
    await page.evaluate(() => showFixModal());
    await settle(page);
    await page.keyboard.press('Escape');
    await expect(page.locator('#btn-more')).toBeFocused();
    assert.deepEqual(forbiddenRequests, []);
    assert.deepEqual(test.errors, []);
    report.checks.push('Original close and backdrop handlers; zero model/task-start calls and zero page errors');
    report.modalServiceControlPassed = true;
    report.modalServiceControl = 'Visible #btn-more focused then real showFixModal() called; original Escape handler returned focus. This is a separate modal-service control, not the real menu flow.';
    report.menuFocusReturnPassed = report.cases.every(item => item.menuReturn.active === 'btn-fix' && item.menuReturn.openerVisible && item.menuReturn.moreSelected);
    assert.equal(report.menuFocusReturnPassed, true, 'Every real repair flow must restore focus to its visible sidebar action');
    report.visualScopePassed = true;
    report.menuFocusBoundary = 'The More sidebar stays open behind the repair dialog. Closing the dialog restores focus to the action that opened it.';
    report.passed = true;
  } catch (error) {
    report.failure = { phase, message: error.message, stack: error.stack };
    process.exitCode = 1;
  } finally {
    if (test) { report.pageErrors = test.errors; await test.close(); }
    report.sourceAfter = binding();
    report.sourceUnchanged = JSON.stringify(report.sourceBefore) === JSON.stringify(report.sourceAfter);
    report.forbiddenRequests = forbiddenRequests;
    report.ownedScratchRemoved = ['tmp', 'home', 'browser-profile'].every(name => !fs.existsSync(path.join(args.evidence, name)));
    fs.writeFileSync(path.join(args.evidence, 'report.json'), JSON.stringify(report, null, 2) + '\n');
    console.log(JSON.stringify({ passed: report.passed, failure: report.failure?.message, cases: report.cases.length,
      checks: report.checks.length, sourceUnchanged: report.sourceUnchanged, ownedScratchRemoved: report.ownedScratchRemoved }));
  }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
