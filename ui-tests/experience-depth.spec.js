const {waitForPage}=require('./page-condition.cjs');
// Stateful acceptance: real filesystem revisions plus delayed/failing HTTP
// boundaries that immediate-success tests cannot exercise.
const { test, expect } = require('@playwright/test');
const fs = require('node:fs/promises');
const path = require('node:path');
const os = require('node:os');
const DOC = 'C:/depth/论证.docx';
const OTHER = 'C:/depth/证据.pdf';
const json = (route, body, status = 200) => route.fulfill({ status, json: body });
const gate = () => { let release; const promise = new Promise(resolve => release = resolve); return { promise, release }; };
async function app(page) {
  await page.goto('/');
  await waitForPage(page, () => window.ReadMDTask && typeof startAutoReload === 'function');
}
async function batch(page, items = [DOC]) {
  await page.route('**/api/modules', r => json(r, { modules: { convert: 'ready', ocr: 'ready' } }));
  await page.route('**/api/modules/load', r => json(r, { ok: true }));
  await page.route('**/api/convert/batch', r => json(r, { job: 'depth-job', total: items.length }));
  await app(page);
  await page.evaluate(paths => enqueueBatchFiles(paths, false), items);
}
async function exportPanel(page) {
  await app(page);
  await page.evaluate(async () => {
    await renderVirtual('clipboard', '论证.md', '', '# Evidence\n\nA precise argument.', []);
    await openExportModal(); state.export.fmt = 'html';
  });
}
test.beforeEach(async ({ page }) => {
  await page.route('**/api/update/check', r => json(r, { ok: false }));
  await page.addInitScript(() => localStorage.setItem('readmd_language', 'zh-CN'));
});

test('running rows are not counted as completed and terminal states never regress', async ({ page }) => {
  let finished = false;
  await page.route('**/api/convert/progress*', r => json(r, {
    running: !finished, finished, done: finished ? 2 : 0, total: 2,
    items: [{ src: DOC, status: finished ? 'ok' : 'running', out: 'C:/depth/论证.md' }, { src: OTHER, status: finished ? 'skipped' : 'queued', out: 'C:/depth/证据.md' }],
  }));
  await batch(page, [DOC, OTHER]);
  await expect(page.locator('.batch-item.running')).toHaveCount(1);
  expect(await page.evaluate(() => document.querySelector('.batch-item.running').dataset.done)).toBeUndefined();
  finished = true;
  await expect(page.locator('#convert-status')).toContainText('成功 1');
  await expect(page.locator('#convert-status')).toContainText('跳过 1');
  await page.evaluate(src => renderBatchProgress({ items: [{ src, status: 'running' }] }), DOC);
  await expect(page.locator('.batch-item.ok')).toHaveCount(1);
  await expect(page.locator('.batch-item.ok')).toHaveAttribute('tabindex', '0');
});

test('duplicate sources yield one conversion and one row, including Windows case aliases', async ({ page }) => {
  let submitted;
  await page.route('**/api/convert/progress*', r => json(r, { running: false, finished: true, items: [{ src: DOC, status: 'ok', out: 'C:/depth/论证.md' }] }));
  await page.route('**/api/modules', r => json(r, { modules: { convert: 'ready' } }));
  await page.route('**/api/convert/batch', r => { submitted = r.request().postDataJSON(); return json(r, { job: 'dedupe' }); });
  await app(page);
  await page.evaluate(paths => enqueueBatchFiles(paths, false), [DOC, DOC, 'c:\\depth\\论证.docx']);
  await expect(page.locator('#convert-status')).toContainText('成功 1');
  expect(submitted.paths).toEqual([DOC]);
  await expect(page.locator('.batch-item')).toHaveCount(1);
});

test('transient progress failure reconnects without inventing failed files or releasing the job', async ({ page }) => {
  let calls = 0;
  await page.route('**/api/convert/progress*', r => ++calls === 1 ? json(r, { error_code: 'unavailable' }, 503)
    : json(r, { running: false, finished: true, items: [{ src: DOC, status: 'ok', out: 'C:/depth/论证.md' }] }));
  await batch(page);
  await expect(page.locator('#convert-status')).toContainText('重新连接');
  await expect(page.locator('#convert-files')).toBeDisabled();
  await expect(page.locator('.batch-item.error')).toHaveCount(0);
  await expect(page.locator('#convert-status')).toContainText('成功 1');
  await expect(page.locator('#convert-files')).toBeEnabled();
});

test('slow progress is single-flight and hiding/reopening keeps the current workbench', async ({ page }) => {
  let calls = 0, inFlight = 0, peak = 0;
  await page.route('**/api/convert/progress*', async r => {
    calls++; peak = Math.max(peak, ++inFlight);
    await new Promise(resolve => setTimeout(resolve, 900));
    inFlight--;
    await json(r, { running: calls < 2, finished: calls >= 2, items: [{ src: DOC, status: calls < 2 ? 'running' : 'ok' }] });
  });
  await batch(page);
  await page.evaluate(() => { closeBatchModal(); openBatchModal(); });
  await expect(page.locator('.batch-item')).toHaveCount(1);
  await expect(page.locator('#convert-status')).toContainText('成功 1');
  expect(calls).toBe(2); expect(peak).toBe(1);
});

test('batch cancellation can retry a failed acknowledgement and waits for the final state', async ({ page }) => {
  let cancels = 0;
  await page.route('**/api/convert/cancel', r => ++cancels === 1 ? json(r, { ok: false }, 503) : json(r, { ok: true }));
  await page.route('**/api/convert/progress*', r => json(r, { running: cancels < 2, finished: cancels >= 2, items: [{ src: DOC, status: cancels >= 2 ? 'canceled' : 'running' }] }));
  await batch(page);
  await expect(page.locator('.batch-item.running')).toHaveCount(1);
  await page.locator('#batch-cancel').click();
  await expect(page.locator('#toast')).toContainText('再次点击取消');
  await expect(page.locator('#batch-cancel')).toBeEnabled();
  await page.locator('#batch-cancel').click();
  await expect(page.locator('#convert-status')).toContainText('已取消 1');
  await expect(page.locator('#convert-files')).toBeEnabled();
});

test('a final response missing file outcomes cannot report zero failures', async ({ page }) => {
  await page.route('**/api/convert/progress*', r => json(r, { running: false, finished: true, items: [{ src: DOC, status: 'running' }] }));
  await batch(page);
  await expect(page.locator('#convert-status')).toContainText('失败 1');
  await expect(page.locator('.batch-item.error')).toHaveAttribute('title', /未返回此文件/);
});

test('same-mtime external changes reload using content revision on the real filesystem', async ({ page }) => {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'readmd-depth-revision-'));
  try {
    const source = path.join(dir, 'argument.md');
    await fs.writeFile(source, '# Before\n'); const original = await fs.stat(source);
    await app(page); await page.evaluate(p => loadFile(p), source);
    await fs.writeFile(source, '# After!\n'); await fs.utimes(source, original.atime, original.mtime);
    await page.evaluate(() => { state.autoReload = true; startAutoReload(); });
    await expect(page.locator('#content')).toContainText('After!', { timeout: 10000 });
    expect(await fs.readFile(source, 'utf8')).toBe('# After!\n');
  } finally { await page.evaluate(() => stopAutoReload()); await fs.rm(dir, { recursive: true, force: true }); }
});

for (const stage of ['metadata', 'content']) {
  test(`late auto reload ${stage} response does not exit or replace a newly opened editor`, async ({ page }) => {
    const held = gate(); let waiting = false, changed = false;
    const source = 'C:/depth/reload.md';
    await page.route('**/api/file?p=*', async r => {
      const meta = new URL(r.request().url()).searchParams.has('meta');
      if (changed && ((stage === 'metadata' && meta) || (stage === 'content' && !meta))) { waiting = true; await held.promise; }
      await json(r, { path: source, dir: 'C:/depth', name: 'reload.md', content: changed ? '# External' : '# Original', original: changed ? '# External' : '# Original', mtime: changed ? 2 : 1, revision: changed ? 'new' : 'old', fixes: [], stats: {} });
    });
    await app(page); await page.evaluate(p => loadFile(p), source); changed = true;
    await page.evaluate(() => { state.autoReload = true; startAutoReload(); });
    await expect.poll(() => waiting).toBe(true);
    await page.evaluate(async () => { await toggleEdit(); cmView.dispatch({ changes: { from: 0, to: cmView.state.doc.length, insert: 'MY UNSAVED DRAFT' } }); });
    held.release();
    await page.waitForTimeout(250);
    expect(await page.evaluate(() => ({ editing: state.editing, text: getEditContent(), original: state.original }))).toEqual({ editing: true, text: 'MY UNSAVED DRAFT', original: '# Original' });
    await page.evaluate(() => stopAutoReload());
  });
}

test('export transport failures remain visible and the same panel can retry successfully', async ({ page }) => {
  let calls = 0;
  await page.route('**/api/export', r => ++calls === 1 ? r.abort('failed') : json(r, { ok: true, path: 'C:/depth/result.html' }));
  await exportPanel(page);
  await page.locator('#export-run').click();
  await expect(page.locator('#export-result')).toHaveClass(/err/);
  await expect(page.locator('#export-result')).not.toHaveText('');
  await expect(page.locator('#export-run')).toBeEnabled();
  await page.locator('#export-run').click();
  await expect(page.locator('#export-result')).toContainText('result.html');
  expect(calls).toBe(2);
});

test('export cancellation failure can retry, then reports the confirmed cancellation', async ({ page }) => {
  const held = gate(); let cancels = 0;
  await page.route('**/api/export', async r => { await held.promise; await json(r, { ok: false, error_code: 'cancelled' }); });
  await page.route('**/api/task/cancel', r => ++cancels === 1 ? json(r, { state: 'cancelling' }, 503) : json(r, { ok: true, state: 'cancelling' }));
  await exportPanel(page); await page.locator('#export-run').click();
  await page.locator('#export-cancel').click();
  await expect(page.locator('#export-result')).toContainText('再次点击取消');
  await expect(page.locator('#export-cancel')).toBeEnabled();
  await page.locator('#export-cancel').click();
  await expect(page.locator('#export-result')).toContainText('等待任务确认');
  await expect(page.locator('#export-run')).toBeDisabled();
  held.release();
  await expect(page.locator('#export-result')).toHaveText('已取消');
  await expect(page.locator('#export-run')).toBeEnabled();
});

test('cancelled chooser reports cancellation but an unavailable chooser reports an error', async ({ page }) => {
  let unavailable = false;
  await page.route('**/api/export', r => json(r, unavailable ? { ok: false, error_code: 'dialog_unavailable', canceled: true } : { ok: false, canceled: true }));
  await exportPanel(page); await page.locator('#export-run').click();
  await expect(page.locator('#export-result')).toHaveText('已取消');
  unavailable = true; await page.locator('#export-run').click();
  await expect(page.locator('#export-result')).toHaveClass(/err/);
});

test('export remembers the submitted preset even if settings change while waiting', async ({ page }) => {
  const held = gate();
  await page.route('**/api/export', async r => { await held.promise; await json(r, { ok: true, path: 'C:/depth/preset.html' }); });
  await exportPanel(page);
  await page.evaluate(() => {
    const select = $('exp-preset');
    for (const name of ['submitted-preset', 'newer-preset']) select.add(new Option(name, name));
    select.value = 'submitted-preset'; window.exportPromise = runExport(); select.value = 'newer-preset';
  });
  held.release(); await page.evaluate(() => window.exportPromise);
  expect(await page.evaluate(() => state.export.last.preset)).toBe('submitted-preset');
});

test('late cancel reply cannot overwrite a newer task status or disable its cancel button', async ({ page }) => {
  const cancelHeld = gate(), first = gate(), second = gate(); let exports = 0, waiting = false;
  await page.route('**/api/export', async r => { const n = ++exports; await (n === 1 ? first.promise : second.promise); await json(r, { ok: true, path: `C:/depth/result-${n}.html` }); });
  await page.route('**/api/task/cancel', async r => { waiting = true; await cancelHeld.promise; await json(r, { ok: true, state: 'cancelling' }); });
  await exportPanel(page); await page.locator('#export-run').click(); await page.locator('#export-cancel').click();
  await expect.poll(() => waiting).toBe(true); first.release();
  await expect(page.locator('#export-run')).toBeEnabled(); await page.locator('#export-run').click();
  cancelHeld.release(); await page.waitForTimeout(200);
  await expect(page.locator('#export-cancel')).toBeEnabled();
  await expect(page.locator('#export-result')).not.toContainText('等待任务确认');
  second.release(); await expect(page.locator('#export-result')).toContainText('result-2.html');
});

test('export remains successful when remembering settings fails and open errors do not escape', async ({ page }) => {
  const errors = []; page.on('pageerror', e => errors.push(e.message));
  await exportPanel(page);
  await page.evaluate(async () => {
    hasPy = true; py = { export_doc: async () => ({ ok: true, path: 'C:/depth/success.html' }), save_export_presets: async () => { throw new Error('disk unavailable'); }, open_path: async () => { throw new Error('moved'); }, reveal_path: async () => { throw new Error('moved'); } };
    await runExport();
  });
  await expect(page.locator('#export-result')).toHaveClass(/ok/);
  await expect(page.locator('#export-warns')).toContainText('未能记住');
  await page.locator('#export-open').click(); await page.locator('#export-reveal').click();
  await expect(page.locator('#export-open')).toBeEnabled(); expect(errors).toEqual([]);
});

const pluginList = (installing = false, enabled = true) => ({ ok: true, plugins: {
  easyocr: { id: 'easyocr', name_key: 'plugin.easyocr.name', desc_key: 'plugin.easyocr.desc', category: 'ocr', installed: !installing, enabled, installing },
} });

test('concurrent plugin refreshes share one request and clear the loading status', async ({ page }) => {
  const held = gate(); let calls = 0;
  await page.route('**/api/plugins/list', async r => { calls++; await held.promise; await json(r, pluginList()); });
  await app(page);
  await page.evaluate(() => { window.pluginOpen = openPluginModal(); window.pluginRefresh = Promise.all([refreshPluginList(), refreshPluginList(), refreshPluginList()]); });
  await expect.poll(() => calls).toBe(1); held.release();
  await page.evaluate(() => Promise.all([window.pluginOpen, window.pluginRefresh]));
  await expect(page.locator('#plugin-cards-grid .plugin-card')).toHaveCount(1);
  await expect(page.locator('#plugin-list-status')).toBeHidden(); expect(calls).toBe(1);
});

test('slow plugin installation progress stays single-flight and eventually refreshes', async ({ page }) => {
  let calls = 0, pending = 0, peak = 0;
  await page.route('**/api/plugins/list', async r => {
    const n = ++calls; peak = Math.max(peak, ++pending);
    if (n > 1) await new Promise(resolve => setTimeout(resolve, 1800));
    pending--; await json(r, pluginList(n < 3));
  });
  await app(page); await page.evaluate(() => openPluginModal());
  await expect.poll(() => calls, { timeout: 10000 }).toBeGreaterThanOrEqual(3);
  await expect(page.locator('#plugin-cards-grid input[data-action="toggle"]')).toBeVisible();
  expect(peak).toBe(1); expect(calls).toBe(3);
  await page.evaluate(() => closePluginModal());
});

test('a list requested before a toggle cannot roll its confirmed state back', async ({ page }) => {
  const stale = gate(); let calls = 0;
  await page.route('**/api/plugins/list', async r => {
    const n = ++calls; if (n === 2) await stale.promise;
    await json(r, pluginList(false, n < 3));
  });
  await page.route('**/api/plugins/toggle', r => json(r, { ok: true, enabled: false }));
  await app(page); await page.evaluate(() => openPluginModal());
  await page.evaluate(() => { window.stalePlugin = refreshPluginList(); });
  await expect.poll(() => calls).toBe(2);
  await page.locator('#plugin-cards-grid label.plugin-switch').click();
  await expect.poll(() => calls).toBe(3); stale.release();
  await page.evaluate(() => window.stalePlugin);
  await expect(page.locator('#plugin-cards-grid input[data-action="toggle"]')).not.toBeChecked();
});

async function previewPages(page) {
  await exportPanel(page);
  await page.evaluate(() => {
    cancelNativeExportPreview();
    const png = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aN1sAAAAASUVORK5CYII=';
    nativeExportPreview.data = { ok: true, format: 'pdf', key: 'page-navigation', pages: 3, firstPage: png, text: 'Preview' };
    nativeExportPreview.page = 1; nativeExportPreview.pageCache.set(1, png);
    $('export-preview-modal').classList.remove('hidden'); renderNativeExportPreview('Test', {});
  });
}

test('closing while export presets load prevents a late response from reopening settings', async ({ page }) => {
  const held = gate(); let requested = false;
  await page.route('**/api/export/presets', async route => { const response = await route.fetch(); requested = true; await held.promise; await route.fulfill({ response }); });
  await app(page);
  await page.evaluate(async () => { await renderVirtual('convert', '论证.md', '', '# Evidence', []); window.lateExportOpen = openExportModal(); });
  await expect.poll(() => requested).toBe(true);
  await page.evaluate(() => closeExportModal()); held.release(); await page.evaluate(() => window.lateExportOpen);
  await expect(page.locator('#export-modal')).toBeHidden();
  await page.evaluate(() => openExportModal()); await expect(page.locator('#export-modal')).toBeVisible();
});

test('preview page navigation sends one request and keeps the visible page until it is ready', async ({ page }) => {
  const held = gate(); let calls = 0;
  await page.route('**/api/export/preview', async r => { calls++; await held.promise; await json(r, { ok: true, png: 'next-page' }); });
  await previewPages(page);
  await page.evaluate(() => { $('export-preview-next-btn').click(); $('export-preview-next-btn').click(); $('export-preview-next-btn').click(); });
  await expect.poll(() => calls).toBe(1);
  await expect(page.locator('#export-preview-next-btn')).toBeDisabled();
  await expect(page.locator('#export-preview-pages-meta')).toContainText('1 / 3');
  held.release(); await expect(page.locator('#export-preview-pages-meta')).toContainText('2 / 3');
  await expect(page.locator('#export-preview-prev-btn')).toBeEnabled(); expect(calls).toBe(1);
});

test('preview page failure retains the old page and restores retry navigation', async ({ page }) => {
  let calls = 0;
  await page.route('**/api/export/preview', r => ++calls === 1 ? json(r, { ok: false }, 503) : json(r, { ok: true, png: 'next-page' }));
  await previewPages(page); await page.locator('#export-preview-next-btn').click();
  await expect(page.locator('#export-preview-pages-meta')).toContainText('1 / 3');
  await expect(page.locator('#export-preview-pages-meta')).toContainText('预览');
  await expect(page.locator('#export-preview-next-btn')).toBeEnabled();
  await page.locator('#export-preview-next-btn').click();
  await expect(page.locator('#export-preview-pages-meta')).toContainText('2 / 3'); expect(calls).toBe(2);
});
