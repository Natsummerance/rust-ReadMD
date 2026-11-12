const {waitForPage}=require('./page-condition.cjs');
// Part C UI quality: modal focus management, task feedback and cancellation,
// narrow-screen layout and a click smoke over the visible toolbar.
const { test, expect } = require('@playwright/test');

test.beforeEach(async ({ page }) => {
  await page.route('**/api/update/check', route => route.fulfill({
    status: 200, contentType: 'application/json', body: JSON.stringify({ ok: false }),
  }));
  await page.addInitScript(() => localStorage.setItem('readmd_language', 'zh-CN'));
});

async function openDoc(page, body = '# Title\n\nSome text.\n') {
  await page.goto('/');
  await waitForPage(page, () => typeof renderVirtual === 'function' && window.ReadMDModal);
  await page.evaluate(async md => { await window.__readmdSessionReady;await renderVirtual('clipboard', 'ui-'+crypto.randomUUID()+'.md', '', md, []); }, body);
}

const MODALS = [
  ['export-modal', () => openExportModal()],
  ['convert-modal', () => openConvertModal()],
  ['plugin-modal', () => openPluginModal()],
  ['tpl-modal', () => openTplModal()],
];

for (const [id, open] of MODALS) {
  test(`${id}: focus stays inside, Esc closes and focus returns`, async ({ page }) => {
    await openDoc(page);
    const opener = page.locator('#tab-settings');
    await opener.focus();
    await expect(opener).toBeFocused();
    await page.evaluate(open);
    const modal = page.locator('#' + id);
    await expect(modal).toBeVisible();
    await expect.poll(() => page.evaluate(m => document.getElementById(m).contains(document.activeElement), id)).toBe(true);
    for (let i = 0; i < 25; i++) {
      await page.keyboard.press(i % 5 === 4 ? 'Shift+Tab' : 'Tab');
      expect(await page.evaluate(m => document.getElementById(m).contains(document.activeElement), id)).toBe(true);
    }
    await page.keyboard.press('Escape');
    await expect(modal).toBeHidden();
    expect(await page.evaluate(() => document.querySelectorAll('[data-rm-inert]').length)).toBe(0);
    await expect(opener).toBeFocused();
  });
}

test('sidebar tool panels stay below style editing and return focus after saving', async ({ page }) => {
  await page.setViewportSize({ width: 1024, height: 680 });
  await openDoc(page);
  await page.evaluate(() => { document.body.dataset.theme = 'sepia'; });
  for (const view of ['conversion', 'interaction', 'settings']) {
    await page.locator('#tab-' + view).click();
    await expect(page.locator('#side-' + view)).toBeVisible();
    await expect(page.locator('#side .side-view:visible')).toHaveCount(1);
  }
  const opener = page.locator('#btn-style-custom');
  const modal = page.locator('#style-custom-modal');
  await opener.click();
  await expect(modal).toBeVisible();
  const css = modal.locator('#style-custom-css');
  const head = modal.locator('#style-custom-head');
  await expect(css).toBeEnabled();
  const before = await page.evaluate(async () => {
    const response = await apiFetch('/api/style/get');
    return (await response.json()).data;
  });
  try {
    const layers = await page.evaluate(() => {
      const side = document.getElementById('side');
      const modal = document.getElementById('style-custom-modal');
      const panel = modal.querySelector('.modal-dialog').getBoundingClientRect();
      const sidebar = side.getBoundingClientRect();
      const x = Math.max(panel.left, sidebar.left) + 2;
      const y = Math.max(panel.top, sidebar.top) + 18;
      return {
        hasOverlap: x < Math.min(panel.right, sidebar.right) && y < Math.min(panel.bottom, sidebar.bottom),
        onTop: modal.contains(document.elementFromPoint(x, y)),
        zOrder: Number(getComputedStyle(side).zIndex) < Number(getComputedStyle(modal).zIndex),
        inert: document.getElementById('layout').inert,
      };
    });
    expect(layers).toEqual({ hasOverlap: true, onTop: true, zOrder: true, inert: true });
    await modal.locator('#btn-preset-indent').click();
    await expect(css).toHaveValue(/text-indent/);
    const next = { css: '.markdown-body h1 { color: #3355aa; }', head: '<meta name="color-scheme" content="light dark">' };
    await css.fill(next.css);
    await head.locator('..').locator('summary').click();
    await head.fill(next.head);
    const prompt = modal.locator('#style-ai-prompt');
    await prompt.locator('../..').locator('summary').click();
    await prompt.fill('加粗');
    await prompt.press('Shift+Enter');
    await prompt.press('a');
    await expect(prompt).toHaveValue('加粗\na');
    await modal.locator('#style-modal-save').click();
    await expect(modal).toBeHidden();
    await expect(opener).toBeFocused();
    await opener.click();
    await expect(css).toBeEnabled();
    await expect(css).toHaveValue(next.css);
    await expect(head).toHaveValue(next.head);
    await page.keyboard.press('Escape');
    await expect(opener).toBeFocused();
  } finally {
    await page.evaluate(async body => {
      await apiFetch('/api/style/save', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
    }, before);
  }
});

test('Esc during IME composition does not close a modal', async ({ page }) => {
  await openDoc(page);
  await page.evaluate(() => openExportModal());
  await page.evaluate(() => {
    document.activeElement.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', isComposing: true, bubbles: true }));
  });
  await expect(page.locator('#export-modal')).toBeVisible();
});

// UI protocol fixture only; real cancellation/persistence is exercised by the
// Rust and export-harness MCP acceptance tests. Preparation remains real here.
test('export is single-flight and can be cancelled', async ({ page }) => {
  let exports = 0;
  let startedTask = null;
  let cancelled = null;
  await page.route('**/api/export/snapshot', async route => {
    if(route.request().postDataJSON()?.action!=='commit')return route.continue();
    exports += 1;startedTask=route.request().postDataJSON().task_id;
    await new Promise(r => setTimeout(r, 800));
    await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ ok: false, error_code: 'cancelled' }) });
  });
  await page.route('**/api/task/cancel', async route => {
    cancelled = JSON.parse(route.request().postData() || '{}');
    await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ ok: true, state: 'cancelling' }) });
  });
  await openDoc(page);
  await page.evaluate(async () => { await openExportModal(); state.export.fmt = 'html'; });
  const run = page.locator('#export-run');
  await run.click();
  await run.click({ force: true });
  await run.click({ force: true });
  await expect(run).toBeDisabled();
  const cancel = page.locator('#export-cancel');
  await expect(cancel).toBeVisible();
  await cancel.click();
  await expect(page.locator('#export-result')).toHaveText(/已取消|Cancelled/);
  await expect(cancel).toBeHidden();
  await expect(run).toBeEnabled();
  expect(exports).toBe(1);
  expect(startedTask).toBeTruthy();
  expect(cancelled && cancelled.id).toBe(startedTask);
});

test('no horizontal page overflow at 360px', async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 740 });
  await openDoc(page, '# Narrow\n\n| a | b | c |\n|---|---|---|\n| 1 | 2 | 3 |\n');
  const overflow = async () => page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
  expect(await overflow()).toBeLessThanOrEqual(0);
  for (const [id, open] of MODALS) {
    await page.evaluate(open);
    await expect(page.locator('#' + id)).toBeVisible();
    expect(await overflow(), id).toBeLessThanOrEqual(0);
    await page.keyboard.press('Escape');
    await expect(page.locator('#' + id)).toBeHidden();
  }
});

test('every visible toolbar button has an accessible name and clicks without errors', async ({ page }) => {
  const errors = [];
  page.on('pageerror', e => errors.push(String(e)));
  await openDoc(page);
  // Controls that leave the page, open OS dialogs or quit are not clicked.
  const SKIP = /^(btn-open|btn-open-folder|btn-home|btn-close|btn-quit|btn-pet)$/;
  const ids = await page.evaluate(() => [...document.querySelectorAll('.toolbar button[id], #toolbar button[id]')]
    .filter(b => b.offsetParent !== null && !b.disabled)
    .map(b => b.id));
  expect(ids.length).toBeGreaterThan(0);
  for (const id of ids) {
    const btn = page.locator('#' + id);
    const name = await btn.evaluate(b => (b.getAttribute('aria-label') || b.title || b.textContent || '').trim());
    expect(name, `#${id} needs an accessible name`).not.toBe('');
    if (SKIP.test(id)) continue;
    await btn.click({ timeout: 2000 }).catch(() => {});
    await page.waitForTimeout(150);
    for (let i = 0; i < 4 && (await page.evaluate(() => window.ReadMDModal.depth())); i++) await page.keyboard.press('Escape');
    await page.evaluate(() => { for (let i = 0; i < 8 && window.ReadMDModal.depth(); i++) window.ReadMDModal.closeTop(); });
  }
  expect(errors).toEqual([]);
});
