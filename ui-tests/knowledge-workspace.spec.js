const {waitForPage}=require('./page-condition.cjs');
// Real Rust filesystem endpoints and the actual editor/reader, no fabricated
// suggestion results. Delayed-response tests use the real response too.
const { test, expect } = require('@playwright/test');
const fs = require('node:fs');
const path = require('node:path');
const docs = path.resolve(__dirname, '../.cache/v005-community/documents');
const source = path.join(docs, '研究总览.md');
const draft = '---\nbibliography: references.bib\n---\n# 研究总览\n\n## 本篇结论\n\n[[认识#证据与判断|阅读证据]] · [[#本篇结论]]\n\n[^scope]: 明确范围。\n\n';
test.beforeAll(() => {
  fs.mkdirSync(path.join(docs, 'notes'), { recursive: true });
  fs.mkdirSync(path.join(docs, 'other'), { recursive: true });
  fs.mkdirSync(path.join(docs, 'images'), { recursive: true });
  fs.writeFileSync(source, draft);
  fs.writeFileSync(path.join(docs, 'notes/认识.md'), '# 认识\n\n## 证据与判断\n\n把观察和解释分开。\n\n## 证据与判断\n\n修订后的结论。');
  fs.writeFileSync(path.join(docs, 'notes/同名.md'), '# 同名');
  fs.writeFileSync(path.join(docs, 'other/同名.md'), '# 同名');
  fs.writeFileSync(path.join(docs, 'references.bib'), '@book{evidence2026,title={Evidence and reasoning},author={Reading Group},year={2026}}');
  fs.writeFileSync(path.join(docs, 'images/reading map.svg'), '<svg xmlns="http://www.w3.org/2000/svg" width="100" height="60"><rect width="100" height="60" fill="#7799cc"/></svg>');
});
test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('readmd_language', 'zh-CN'));
  await page.goto('/'); await waitForPage(page, () => window.__readmdAppReady);
  await page.evaluate(async ({ source, docs }) => { await loadFile(source); state.folder = docs; }, { source, docs });
});
async function edit(page, text = '') {
  await page.evaluate(async () => {
    await toggleEdit(); setPvLayout('none'); setEditorPref('aiCompletion', false);
    cmView.dispatch({ selection: { anchor: cmView.state.doc.length } }); cmView.focus();
  });
  if (text) await page.keyboard.type(text, { delay: 35 });
}
test('wiki suggestions resolve nested real files and Tab accepts one undoable edit', async ({ page }) => {
  await edit(page, '[[认识');
  await expect(page.locator('.cm-tooltip-autocomplete')).toContainText('notes/认识.md');
  const before = await page.evaluate(() => getEditContent());
  await page.keyboard.press('Tab');
  expect(await page.evaluate(() => getEditContent())).toContain('[[notes/认识]]');
  await page.keyboard.press('Control+z');
  expect(await page.evaluate(() => getEditContent())).toBe(before);
});
test('cross-document heading completion preserves delimiters and Enter accepts before list handlers', async ({ page }) => {
  await edit(page, '[[认识#证据');
  await expect(page.locator('.cm-tooltip-autocomplete')).toContainText('证据与判断');
  await page.keyboard.press('Enter');
  expect(await page.evaluate(() => getEditContent())).toContain('[[认识#证据与判断]]');
  expect(await page.evaluate(() => getEditContent())).not.toContain(']]]]');
});
test('current heading suggestions use the unsaved draft and skip code blocks', async ({ page }) => {
  await edit(page, '\n## 新的结论\n\n~~~md\n# 伪标题\n~~~\n\n[[#');
  await expect(page.locator('.cm-tooltip-autocomplete')).toContainText('新的结论');
  await expect(page.locator('.cm-tooltip-autocomplete')).not.toContainText('伪标题');
});
test('footnotes and bibliography keys come from the current source', async ({ page }) => {
  await edit(page, '[^sc');
  await expect(page.locator('.cm-tooltip-autocomplete')).toContainText('scope');
  await page.keyboard.press('Tab');
  expect(await page.evaluate(() => getEditContent())).toContain('[^scope]');
  await page.keyboard.type('\n\n[@ev', { delay: 35 });
  await expect(page.locator('.cm-tooltip-autocomplete')).toContainText('evidence2026');
  await page.keyboard.press('Tab');
  expect(await page.evaluate(() => getEditContent())).toContain('[@evidence2026]');
});
test('image paths escape spaces and remote URLs never request local suggestions', async ({ page }) => {
  await edit(page, '![路线](images/reading');
  await expect(page.locator('.cm-tooltip-autocomplete')).toContainText('reading map.svg');
  await page.keyboard.press('Tab');
  expect(await page.evaluate(() => getEditContent())).toContain('![路线](images/reading%20map%2Esvg)');
  let requests = 0;
  await page.route('**/api/workspace/suggest', async r => { requests++; await r.continue(); });
  await page.keyboard.type('\n\n[官网](https://example.test', { delay: 20 });
  await page.waitForTimeout(350);
  expect(requests).toBe(0);
});
test('cross-directory reader links open the verified file and focus its exact heading', async ({ page }) => {
  await page.locator('.wikilink').filter({ hasText: '阅读证据' }).click();
  await expect.poll(() => page.evaluate(() => state.file.replaceAll('\\', '/'))).toBe(path.join(docs, 'notes/认识.md').replaceAll('\\', '/'));
  await expect(page.locator('.heading-target-highlight')).toHaveText('证据与判断');
});
test('current-document wiki heading works and ambiguous notes leave the current file intact', async ({ page }) => {
  await page.locator('.wikilink').filter({ hasText: '#本篇结论' }).click();
  await expect(page.locator('.heading-target-highlight')).toHaveText('本篇结论');
  await page.evaluate(() => navigateWikilink('同名', []));
  await expect(page.locator('#toast')).toContainText('有多篇同名笔记');
  expect(await page.evaluate(() => state.file.replaceAll('\\', '/'))).toBe(source.replaceAll('\\', '/'));
});
test('wiki transform protects fences of both kinds, longer closers, inline runs and escaped text', async ({ page }) => {
  const result = await page.evaluate(() => {
    const cases = [
      '~~~md\n[[hidden]]\n~~~~\n[[visible]]',
      '````md\n```\n[[hidden]]\n`````\n[[visible]]',
      '`` [[hidden]] `example` ``\n[[visible]]',
      '```md\n[[hidden]]',
      '\\[[escaped]] [[visible]]',
    ];
    return cases.map(s => transformWikilinks(s));
  });
  expect(result.slice(0, 3).every(s => s.includes('[[hidden]]') && (s.match(/class="wikilink"/g) || []).length === 1)).toBe(true);
  expect(result[3]).toBe('```md\n[[hidden]]');
  expect(result[4]).toContain('\\[[escaped]]');
});
test('plain prose and fenced code stay quiet while manual Markdown snippets remain available', async ({ page }) => {
  let requests = 0;
  await page.route('**/api/workspace/suggest', async r => { requests++; await r.continue(); });
  await edit(page, 'Ordinary writing.\n\n```markdown\n[[认识');
  await page.waitForTimeout(450); expect(requests).toBe(0);
  await expect(page.locator('.cm-tooltip-autocomplete')).toHaveCount(0);
  await page.keyboard.type('\n```\n\n'); await page.keyboard.press('Control+Space');
  await expect(page.locator('.cm-tooltip-autocomplete')).toBeVisible();
});
test('late real suggestion responses never insert into a different tab', async ({ page }) => {
  let release; const delayed = new Promise(resolve => release = resolve);
  await page.route('**/api/workspace/suggest', async route => { const response = await route.fetch(); await delayed; await route.fulfill({ response }).catch(() => {}); });
  await edit(page, '[[认识'); await page.waitForTimeout(250);
  await page.evaluate(async () => { await renderVirtual('clipboard', '另一个文档.md', '', '# Another\n\n', []); if (!state.editing) await toggleEdit(); });
  release(); await page.waitForTimeout(350);
  await expect(page.locator('.cm-tooltip-autocomplete')).toHaveCount(0);
  expect(await page.evaluate(() => getEditContent())).not.toContain('notes/认识');
});
