const { test, expect } = require('@playwright/test');
test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await page.waitForFunction(() => window.__readmdAppReady);
  await page.evaluate(() => i18n.setLanguage('zh-CN', false));
});
test('repair report inspects the current unsaved draft without changing it', async ({ page }) => {
  await page.evaluate(async () => {
    await renderVirtual('clipboard', '检查演示.md', '', '# Original\n', []);
    await toggleEdit();
    cmView.dispatch({ changes: { from: 0, to: cmView.state.doc.length,
      insert: '# 新标题\n\n[需要检查](#missing)\n\n~~~md\n[代码示例](missing.md)\n~~~\n' } });
    showFixModal();
  });
  await expect(page.locator('.document-inspection-summary')).toHaveText('文档检查：1 项待检查');
  await expect(page.locator('.document-inspection-issue')).toHaveCount(1);
  await expect(page.locator('.document-inspection-issue')).toContainText('未找到目标标题');
  expect(await page.evaluate(() => getEditContent())).toContain('[需要检查](#missing)');
  expect(await page.evaluate(() => state.original)).toBe('# Original\n');
});
test('report handles service failure and language changes without stale or raw keys', async ({ page }) => {
  await page.route('**/api/document/analyze', route => route.fulfill({ status: 503, contentType: 'application/json', body: '{"ok":false}' }));
  await page.evaluate(async () => { await renderVirtual('clipboard', '检查演示.md', '', '# Title\n', []); showFixModal(); });
  await expect(page.locator('.document-inspection-summary')).toContainText('文档检查暂不可用');
  await page.unroute('**/api/document/analyze');
  await page.evaluate(async () => { document.getElementById('fix-modal').classList.add('hidden'); await i18n.setLanguage('en', false); showFixModal(); });
  await expect(page.locator('.document-inspection-summary')).toHaveText('Document inspection: 0 items to review');
});
test('workspace search rejects invalid limits rather than silently defaulting', async ({ page }) => {
  const result = await page.evaluate(async () => {
    const response = await apiFetch('/api/workspace/search', { method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ query: 'title:Plato', limit: -1 }) });
    return { status: response.status, body: await response.json() };
  });
  expect(result.status).toBe(400);
  expect(result.body.error_code).toBe('invalid_search_limit');
});
