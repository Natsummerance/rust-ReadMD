const { test, expect } = require('@playwright/test');
const json = (route, value, status = 200) => route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(value) });
const initial = '# Panel acceptance\n\nKeep this draft.\n';

test.beforeEach(async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.addInitScript(() => localStorage.setItem('readmd_language', 'zh-CN'));
  await page.goto('/'); await page.waitForFunction(() => window.__readmdAppReady);
  await page.evaluate(async content => {
    const path = await uploadFile(new File([content], 'panel-acceptance.md', { type: 'text/markdown' }));
    await loadFile(path); state.pvLayout = 'none'; await toggleEdit();
  }, initial);
});

for (const [tool, opener, field, action, value] of [
  ['code', 'openCodeChunkModal', 'code-chunk-code', 'code-chunk-insert', 'console.log("one-insertion-73");'],
  ['diagram', 'openDiagramModal', 'diagram-code', 'diagram-insert', 'digraph G { one_insertion_73 -> Done; }'],
  ['reference', 'openDocImportModal', 'doc-import-path', 'doc-import-insert', 'one-insertion-73.md'],
]) {
  test(`${tool}: one click inserts once and one undo restores the draft`, async ({ page }) => {
    const before = await page.evaluate(() => cmView.state.doc.toString());
    await page.evaluate(opener => window[opener](), opener);
    await page.locator('#' + field).fill(value);
    await page.locator('#' + action).click();
    const inserted = await page.evaluate(() => cmView.state.doc.toString());
    expect(inserted.split(value).length - 1).toBe(1);
    await page.keyboard.press('Control+z');
    expect(await page.evaluate(() => cmView.state.doc.toString())).toBe(before);
  });
}

for (const [opener, field, selector, target] of [
  ['openCodeChunkModal', 'code-chunk-code', 'code-chunk-lang', 'javascript'],
  ['openDiagramModal', 'diagram-code', 'diagram-type', 'graphviz'],
]) {
  test(`${field}: changing type preserves custom input`, async ({ page }) => {
    await page.evaluate(opener => window[opener](), opener);
    const draft = 'Custom content that must survive a type change.';
    await page.locator('#' + field).fill(draft);
    await page.locator('#' + selector).selectOption(target);
    await expect(page.locator('#' + field)).toHaveValue(draft);
    if (selector === 'code-chunk-lang') await expect(page.locator('#code-chunk-opt-plot')).toBeDisabled();
  });
}

for (const size of [{ width: 1024, height: 680 }, { width: 390, height: 680 }]) {
  test(`form headers and actions stay visible at ${size.width}px`, async ({ page }) => {
    await page.setViewportSize(size);
    for (const [id, opener, action] of [
      ['code-chunk', 'openCodeChunkModal', 'code-chunk-insert'],
      ['diagram', 'openDiagramModal', 'diagram-insert'],
      ['doc-import', 'openDocImportModal', 'doc-import-insert'],
      ['frontmatter', 'openFrontmatterModal', 'fm-modal-insert'],
    ]) {
      await page.evaluate(opener => window[opener](), opener);
      const modal = page.locator('#' + id + '-modal');
      await modal.locator('details').evaluateAll(els => els.forEach(el => { el.open = true; }));
      const before = await page.locator('#' + action).boundingBox();
      await modal.locator('.panel-scroll-body').evaluate(el => { el.scrollTop = el.scrollHeight; });
      const after = await page.locator('#' + action).boundingBox();
      expect(after.y).toBeCloseTo(before.y, 1);
      expect(after.y + after.height).toBeLessThanOrEqual(size.height);
      await expect(modal.locator('.modal-header')).toBeInViewport();
      await page.keyboard.press('Escape'); await expect(modal).toBeHidden();
    }
  });
}

test('table selector shows all ten columns and inserts the chosen table', async ({ page }) => {
  await page.setViewportSize({ width: 1024, height: 680 });
  await page.evaluate(() => openTableModal());
  const left = await page.locator('.table-grid-cell[data-row="1"][data-col="1"]').boundingBox();
  const right = await page.locator('.table-grid-cell[data-row="1"][data-col="10"]').boundingBox();
  const surface = await page.locator('#table-modal .modal-dialog').boundingBox();
  expect(left.x).toBeGreaterThan(surface.x); expect(right.x + right.width).toBeLessThan(surface.x + surface.width);
  await page.locator('.table-grid-cell[data-row="2"][data-col="4"]').click();
  const doc = await page.evaluate(() => cmView.state.doc.toString());
  const rows = doc.split('\n').filter(line => line.startsWith('| '));
  expect(rows).toHaveLength(4); expect(rows[0].split('|')).toHaveLength(6);
});

test('search navigation reflects whether the current query has results', async ({ page }) => {
  await page.evaluate(() => { exitEdit(); renderContent(state.original, 'Search'); });
  await page.locator('#btn-search').click();
  await page.locator('#search-input').fill('not-found-in-this-document');
  await expect(page.locator('#search-next')).toBeDisabled(); await expect(page.locator('#search-prev')).toBeDisabled();
  await page.locator('#search-input').fill('draft');
  await expect(page.locator('#search-next')).toBeEnabled(); await expect(page.locator('#search-prev')).toBeEnabled();
});

test('every insertion dialog can be cancelled without changing the document', async ({ page }) => {
  await page.setViewportSize({ width: 1160, height: 820 });
  for (const [button, modal] of [
    ['btn-insert-table','table-modal'], ['btn-insert-code-chunk','code-chunk-modal'],
    ['btn-insert-diagram','diagram-modal'], ['btn-insert-doc-import','doc-import-modal'],
    ['btn-insert-frontmatter','frontmatter-modal'],
  ]) {
    const before = await page.evaluate(() => cmView.state.doc.toString());
    await page.locator('[data-menu="md-insert-menu"]').click();
    await page.locator('#' + button).click(); await expect(page.locator('#' + modal)).toBeVisible();
    expect(await page.evaluate(() => cmView.state.doc.toString())).toBe(before);
    await page.keyboard.press('Escape'); await expect(page.locator('#' + modal)).toBeHidden();
    expect(await page.evaluate(() => hasUnsavedEditorChanges())).toBe(false);
  }
});

test('image editor has clear empty controls and discards late loads after closing', async ({ page }) => {
  await page.evaluate(() => openImgModal());
  await expect(page.locator('#img-insert')).toBeDisabled();
  await expect(page.locator('#img-rot-l')).toBeDisabled();
  await page.evaluate(() => {
    const canvas = document.createElement('canvas'); canvas.width = 80; canvas.height = 50;
    canvas.getContext('2d').fillRect(0, 0, 80, 50); loadImgSrc(canvas.toDataURL());
  });
  await expect(page.locator('#img-insert')).toBeEnabled();
  await expect(page.locator('#img-rot-l')).toBeEnabled();
  await page.locator('#img-close').click(); await page.evaluate(() => openImgModal());
  await expect(page.locator('#img-insert')).toBeDisabled();
  await expect(page.locator('#img-out-w')).toHaveValue('');
  expect(await page.evaluate(() => { const c = document.getElementById('img-canvas'); return c.getContext('2d').getImageData(0,0,1,1).data[3]; })).toBe(0);
  let complete;
  const pending = new Promise(resolve => { complete = resolve; });
  await page.route('**/delayed-image.svg', async route => { await pending; await route.fulfill({ contentType:'image/svg+xml', body:'<svg xmlns="http://www.w3.org/2000/svg" width="80" height="50"><rect width="80" height="50"/></svg>' }); });
  const request = page.waitForRequest('**/delayed-image.svg');
  await page.evaluate(() => loadImgSrc('/delayed-image.svg'));
  await request;
  await page.locator('#img-close').click(); complete();
  await page.waitForTimeout(100);
  expect(await page.evaluate(() => imgState.img === null)).toBe(true);
});

test('image save is single flight, preserves editing on failure and supports one-step undo', async ({ page }) => {
  let calls = 0, release;
  const pending = new Promise(resolve => { release = resolve; });
  await page.route('**/api/image/save', async route => {
    if (++calls === 1) { await pending; await json(route, { ok:false, error:'Synthetic save failure' }, 500); }
    else await json(route, { ok:true, rel:'assets/acceptance-image.png' });
  });
  const before = await page.evaluate(() => cmView.state.doc.toString());
  await page.evaluate(() => {
    openImgModal(); const canvas = document.createElement('canvas'); canvas.width = 80; canvas.height = 50;
    canvas.getContext('2d').fillRect(0, 0, 80, 50); loadImgSrc(canvas.toDataURL());
  });
  await expect(page.locator('#img-insert')).toBeEnabled(); await page.locator('#img-insert').click();
  await expect.poll(() => calls).toBe(1);
  await expect(page.locator('#img-close')).toBeDisabled(); await expect(page.locator('#img-file')).toBeDisabled();
  await page.keyboard.press('Escape');
  await page.evaluate(() => { closeImgModal(); openImgModal(); void exportAndInsertImg(); });
  await expect(page.locator('#img-modal')).toBeVisible(); expect(calls).toBe(1);
  release(); await expect(page.locator('#img-insert')).toBeEnabled();
  expect(await page.evaluate(() => cmView.state.doc.toString())).toBe(before);
  await expect(page.locator('#img-rot-l')).toBeEnabled(); await expect(page.locator('#img-close')).toBeEnabled();
  await page.locator('#img-insert').click(); await expect(page.locator('#img-modal')).toBeHidden();
  expect(calls).toBe(2); expect((await page.evaluate(() => cmView.state.doc.toString())).split('acceptance-image.png').length - 1).toBe(1);
  await page.keyboard.press('Control+z'); expect(await page.evaluate(() => cmView.state.doc.toString())).toBe(before);
  await page.evaluate(() => openImgModal()); await expect(page.locator('#img-insert')).toBeDisabled();
});

test('image completion cannot overwrite a document changed during the save', async ({ page }) => {
  let release, started = false;
  const pending = new Promise(resolve => { release = resolve; });
  await page.route('**/api/image/save', async route => { started = true; await pending; await json(route, {ok:true, rel:'assets/stale-image.png'}); });
  await page.evaluate(() => {
    openImgModal(); const canvas = document.createElement('canvas'); canvas.width = 80; canvas.height = 50;
    loadImgSrc(canvas.toDataURL());
  });
  await expect(page.locator('#img-insert')).toBeEnabled(); await page.locator('#img-insert').click();
  await expect.poll(() => started).toBe(true);
  await page.evaluate(() => cmView.dispatch({ changes:{ from:cmView.state.doc.length, insert:'\nExternally updated draft.' } }));
  const changed = await page.evaluate(() => cmView.state.doc.toString());
  release(); await expect(page.locator('#img-insert')).toBeEnabled();
  expect(await page.evaluate(() => cmView.state.doc.toString())).toBe(changed);
  await expect(page.locator('#toast')).toHaveText(await page.evaluate(() => i18n.t('toast.imgExportFail')));
  await page.locator('#img-close').click();
});

test('batch OCR waits for each result and reports partial failure accurately', async ({ page }) => {
  let active = 0, peak = 0, calls = 0;
  await page.route('**/api/ocr?*', async route => {
    const index = ++calls; peak = Math.max(peak, ++active);
    await new Promise(resolve => setTimeout(resolve, 100)); --active;
    await json(route, index === 2 ? { empty:true, content:'' } : { name:'Recognized-' + index, dir:'', content:'# Recognized-' + index, fixes:[] });
  });
  const before = await page.evaluate(() => state.tabs.length);
  const chooser = page.waitForEvent('filechooser');
  await page.evaluate(() => chooseFile('ocr'));
  const selection = await chooser;
  await selection.setFiles([1,2,3].map(n => ({ name:'scan-' + n + '.png', mimeType:'image/png', buffer:Buffer.from('demo OCR transport fixture') })));
  const summary = await page.evaluate(() => i18n.t('batch.summary', { ok:2, skipped:0, failed:1 }));
  await expect(page.locator('#toast')).toHaveText(summary);
  expect(calls).toBe(3); expect(peak).toBe(1);
  expect(await page.evaluate(() => state.tabs.length)).toBe(before + 2);
  await expect(page.locator('#btn-ocr')).toBeEnabled();
});

test('web extraction gives inline URL feedback and owns cancellation until completion', async ({ page }) => {
  let requests = 0, cancels = 0, release;
  const completion = new Promise(resolve => { release = resolve; });
  await page.route('**/api/web/extract', async route => { requests++; await completion; await json(route, { ok:true, content:'# Extracted', meta:{ title:'Extracted' } }); });
  await page.route('**/api/web/cancel', async route => { cancels++; await json(route, {ok:true}); });
  await page.evaluate(() => openWebDialog());
  await page.locator('#url-input').fill('http://'); await page.locator('#url-go').click();
  await expect(page.locator('#url-status')).toBeVisible(); await expect(page.locator('#url-input')).toHaveAttribute('aria-invalid','true');
  expect(requests).toBe(0);
  await page.locator('#url-input').fill('https://example.com/article'); await page.locator('#url-go').click();
  await expect.poll(() => requests).toBe(1);
  await page.keyboard.press('Escape');
  await expect(page.locator('#url-modal')).toBeVisible();
  await expect(page.locator('#url-cancel')).toBeDisabled();
  await page.evaluate(() => openWebDialog());
  await expect(page.locator('#url-go')).toBeDisabled(); await expect(page.locator('#url-progress')).toBeVisible();
  release(); await expect(page.locator('#url-go')).toBeEnabled();
  expect(cancels).toBe(1);
  await expect(page.locator('#url-status')).toHaveText(await page.evaluate(() => i18n.t('web.cancelledStatus')));
  await page.locator('#url-close').click(); await expect(page.locator('#url-modal')).toBeHidden();
});

test('share copy uses the authenticated link and failed clipboard fallback is honest', async ({ page }) => {
  await page.route('**/api/share/status', route => json(route, { running:true, url:'http://127.0.0.1:9999/read', token:'fixture-only-token' }));
  await page.evaluate(() => {
    window.copiedShareValue = null;
    Object.defineProperty(navigator.clipboard, 'writeText', { configurable:true, writable:true, value:async text => { window.copiedShareValue = text; } });
  });
  await page.evaluate(() => openShareModal());
  await page.locator('#share-copy').click();
  expect(await page.evaluate(() => window.copiedShareValue)).toBe('http://127.0.0.1:9999/read?token=fixture-only-token');
  await page.evaluate(() => {
    navigator.clipboard.writeText = async () => { throw new Error('denied'); };
    document.execCommand = () => false;
  });
  await page.locator('#share-copy').click();
  await expect(page.locator('#toast')).toHaveText(await page.evaluate(() => i18n.t('toast.copyFailed')));
  await expect(page.locator('#share-copy')).toBeFocused();
});
