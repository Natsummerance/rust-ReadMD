const { test, expect } = require('@playwright/test');
const json = (route, body) => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });

// Exercise every static dialog, including controls revealed by details and
// panes which used to be clipped by a non-scrolling modal body.
for (const size of [{ width: 1160, height: 820 }, { width: 1024, height: 680 }]) {
  for (const locale of (process.env.READMD_PANEL_TEST_LOCALES || 'zh-CN,en,zh-TW').split(',')) {
    for (const theme of (process.env.READMD_PANEL_TEST_THEMES || 'light').split(',')) {
    test(`F097 all 29 dialogs remain reachable at ${size.width}x${size.height} in ${locale}, ${theme}`, async ({ page }, testInfo) => {
      test.setTimeout(120000);
      const errors = []; page.on('pageerror', error => errors.push(error.message));
      await page.route('**/api/update/check', route => json(route, { ok: false }));
      await page.addInitScript(locale => localStorage.setItem('readmd_language', locale), locale);
      await page.setViewportSize(size); await page.emulateMedia({ reducedMotion: 'reduce' });
      await page.goto('/'); await page.waitForFunction(() => typeof openPetSettings === 'function');
      await page.evaluate(theme => { state.theme = theme; applySettings(); }, theme);
      await page.evaluate(async () => {
        const path = await uploadFile(new File(['# Layout\n\nBody'], 'layout.md', { type: 'text/markdown' }));
        await loadFile(path); await toggleEdit();
        await loadAiConfig();
        await ReadMDRecovery.open(); document.getElementById('document-history-modal').classList.add('hidden');
      });
      const ids = await page.evaluate(() => [...document.querySelectorAll('[role="dialog"][id$="-modal"]')].map(el => el.id));
      expect(ids).toHaveLength(29);
      const report = [];
      for (const id of ids) {
        await page.evaluate(async id => {
          document.querySelectorAll('[role="dialog"]').forEach(el => window.ReadMDModal.close(el));
          const openers = {
            'document-history-modal': () => ReadMDRecovery.open(),
            'pet-settings-modal': openPetSettings, 'ai-history-modal': () => openAiModal('ai-history-modal'),
            'history-modal': openHistoryModal, 'img-modal': openImgModal, 'formula-modal': openFormulaModal,
            'tpl-modal': openTplModal, 'share-modal': openShareModal, 'url-modal': openWebDialog,
            'fix-modal': showFixModal, 'export-modal': openExportModal, 'convert-modal': openConvertModal,
            'plugin-modal': openPluginModal, 'style-custom-modal': openStyleModal, 'table-modal': openTableModal,
            'code-chunk-modal': openCodeChunkModal, 'diagram-modal': openDiagramModal, 'doc-import-modal': openDocImportModal,
            'frontmatter-modal': openFrontmatterModal,
          };
          if (openers[id]) await openers[id]();
          else window.ReadMDModal.open(id);
          const modal = document.getElementById(id);
          modal.querySelectorAll('details').forEach(el => { el.open = true; });
          modal.querySelectorAll('input[type="text"], textarea').forEach(el => { if (el.disabled) return; el.value ||= 'A long sample value for the layout verification'; });
        }, id);
        await expect(page.locator('#' + id)).toBeVisible();
        await page.waitForTimeout(80);
        const check = await page.evaluate(id => {
          const modal = document.getElementById(id); const surface = modal.firstElementChild;
          const bounds = surface.getBoundingClientRect();
          const failures = [];
          if (bounds.left < -1 || bounds.top < -1 || bounds.right > innerWidth + 1 || bounds.bottom > innerHeight + 1) failures.push('surface outside viewport');
          const controls = [...modal.querySelectorAll('button, summary, input:not([type="hidden"]):not([type="checkbox"]):not([type="radio"]), select, textarea')].filter(el => el.getClientRects().length && getComputedStyle(el).visibility !== 'hidden' && !el.closest('.hidden'));
          for (const el of controls) {
            if (el.disabled) continue;
            el.scrollIntoView({ block: 'nearest', inline: 'nearest' });
            const rect = el.getBoundingClientRect();
            const key = el.id || el.className || el.tagName;
            if (rect.height < 43.5 && el.tagName !== 'TEXTAREA') failures.push(key + ': target below 44px');
            const x = rect.left + Math.min(rect.width / 2, 20), y = rect.top + rect.height / 2;
            const hit = document.elementFromPoint(x, y);
            if (!hit || (!el.contains(hit) && !hit.contains(el))) failures.push(key + ': unreachable/covered');
          }
          return { id, controls: controls.length, failures: [...new Set(failures)] };
        }, id);
        report.push(check);
        // Capture problematic screens for review without serializing input values.
        if (check.failures.length) await page.screenshot({ path: testInfo.outputPath(id + '.png') });
      }
      await testInfo.attach('panel-geometry.json', { body: Buffer.from(JSON.stringify({ size, locale, theme, report }, null, 2)), contentType: 'application/json' });
      expect(report.filter(row => row.failures.length)).toEqual([]);
      expect(errors).toEqual([]);
    });
    }
  }
}
