const { test, expect } = require('@playwright/test');
const { waitForPage } = require('./page-condition.cjs');

const code = JSON.stringify({
  data: { values: [{ group: 'A', value: 3 }, { group: 'B', value: 5 }] },
  mark: 'bar',
  encoding: { x: { field: 'group', type: 'nominal' }, y: { field: 'value', type: 'quantitative' } },
}, null, 2);
const fenced = source => ['```vega-lite', source, '```'].join('\n');

async function show(page, source) {
  await page.goto('/');
  await waitForPage(page, () => window.__readmdAppReady);
  await page.evaluate(content => renderContent(content, 'charts.md'), source);
  return page.locator('#content .diagram-card');
}

test('a reader figure shows only its plot until the source control is opened', async ({ page }) => {
  const figure = await show(page, fenced(code));
  await expect(figure.locator('svg.marks .mark-rect.role-mark > path')).toHaveCount(2);
  const tools = figure.locator('details[data-diagram-tools]');
  await expect(figure).toHaveCSS('border-top-width', '0px');
  await expect(figure).toHaveCSS('box-shadow', 'none');
  await expect(tools.locator('pre')).toBeHidden();
  await expect(tools.locator('.diagram-copy-btn')).toBeHidden();
  await tools.locator('summary').focus();
  await page.keyboard.press('Enter');
  await expect(tools.locator('pre code')).toHaveText(code);
  await expect(tools.locator('.diagram-copy-btn')).toBeVisible();
  await page.emulateMedia({ media: 'print' });
  await expect(tools).toBeHidden();
  await expect(figure.locator('svg.marks')).toBeVisible();
});

test('malformed JSON offers a local preview without changing this fence or a sibling', async ({ page }) => {
  const broken = code.replace(/}\s*$/, ',\n}');
  const cards = await show(page, [fenced(broken), fenced(code)].join('\n\n'));
  const first = cards.nth(0);
  await expect(cards.nth(1).locator('svg.marks')).toBeVisible();
  await expect.poll(() => first.evaluate(card => !!card._diagramDiagnostic?.fix)).toBe(true);
  const diagnosis = await first.evaluate(card => ({ line: card._diagramDiagnostic.line, original: decodeURIComponent(card.dataset.diagramCode) }));
  expect(diagnosis.line).toBeGreaterThan(1);
  expect(diagnosis.original).toBe(broken);
  const previewLabel = await page.evaluate(() => i18n.t('reader.diagramLocalFix'));
  await first.getByRole('button', { name: previewLabel }).click();
  await expect(first.locator('.diagram-preview svg.marks')).toBeVisible();
  expect(await first.evaluate(card => decodeURIComponent(card.dataset.diagramCode))).toBe(broken);
  await expect(cards.nth(1).locator('svg.marks .mark-rect.role-mark > path')).toHaveCount(2);
});
