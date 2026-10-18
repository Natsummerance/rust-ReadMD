const { test, expect } = require('@playwright/test');
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '../.cache/v005-community/session-documents');
let a, b, dir;
test.beforeEach(async ({ page }) => {
  fs.mkdirSync(root, { recursive: true });
  dir = fs.mkdtempSync(path.join(root, 'case-'));
  a = path.join(dir, '阅读.md'); b = path.join(dir, '研究.md');
  fs.writeFileSync(a, '# 阅读\n\n保留原稿。\n'); fs.writeFileSync(b, '# 研究\n\n比较证据。\n');
  await page.addInitScript(() => localStorage.setItem('readmd_language', 'zh-CN'));
  await page.goto('/'); await page.evaluate(() => window.__readmdSessionReady);
});
async function openBoth(page) {
  await page.evaluate(async ([a,b]) => { await loadFile(a); await loadFile(b); }, [a,b]);
}
async function restart(page) {
  await page.evaluate(async () => { await ReadMDSession.flush(); window.onbeforeunload = null; });
  await page.reload(); await page.evaluate(() => window.__readmdSessionReady);
}
const session = page => page.evaluate(() => ({ paths: state.tabs.map(t => t.path), active: state.file, mode: state.mode }));

test('restarts with the actual ordered open pages and active page', async ({ page }) => {
  await openBoth(page);
  await page.evaluate(async () => { reorderTabs(state.tabs[1].id, state.tabs[0].id, false); await switchTab(state.tabs[1].id); });
  const before = await session(page); await restart(page);
  expect(await session(page)).toEqual(before);
  await expect(page.locator('#content')).toContainText('保留原稿');
});

test('closing one page persists only the remaining page', async ({ page }) => {
  await openBoth(page); await page.evaluate(() => closeTab(state.tabs[0].id));
  await restart(page); expect((await session(page)).paths).toEqual([b]);
});

test('closing all pages opens home even with an obsolete last-file value', async ({ page }) => {
  await openBoth(page); await page.evaluate(() => closeAllTabs());
  await page.evaluate(a => localStorage.setItem('readmd-last', a), a);
  await restart(page);
  expect(await session(page)).toEqual({ paths: [], active: null, mode: 'welcome' });
  await expect(page.locator('#w-open')).toBeVisible();
});

test('visiting home preserves unclosed pages without reopening a closed file', async ({ page }) => {
  await openBoth(page); await page.evaluate(() => goHome()); await restart(page);
  expect(await session(page)).toEqual({ paths: [a,b], active: null, mode: 'welcome' });
  await page.locator('.tab-item').filter({ hasText: '研究.md' }).click();
  await expect(page.locator('#content')).toContainText('比较证据');
});

test('dirty file and a clean conversion preview survive without writing originals or sibling files', async ({ page }) => {
  await page.evaluate(async a => {
    await loadFile(a); await toggleEdit();
    cmView.dispatch({ changes: { from: cmView.state.doc.length, insert: '\n未保存的研究结论。' } });
    await renderVirtual('convert', '转换预览.md', state.dir, '# 转换预览\n\n待确认的内容。', []);
    await ReadMDSession.flush();
  }, a);
  const metadata = await page.evaluate(() => localStorage.getItem('readmd-session'));
  expect(metadata).not.toContain('未保存的研究结论');
  await restart(page);
  expect(await page.evaluate(() => state.tabs.map(t => ({ dirty: t.isDirty, text: t.content })))).toEqual([
    { dirty: true, text: '# 阅读\n\n保留原稿。\n\n未保存的研究结论。' },
    { dirty: false, text: '# 转换预览\n\n待确认的内容。' },
  ]);
  await page.evaluate(async () => { await switchTab(state.tabs[0].id); await toggleEdit(); });
  expect(await page.evaluate(() => getEditContent())).toContain('未保存的研究结论');
  expect(fs.readFileSync(a, 'utf8')).toBe('# 阅读\n\n保留原稿。\n');
  expect(fs.readdirSync(dir).sort()).toEqual(['研究.md', '阅读.md']);
});

test('missing files are skipped while remaining pages still open', async ({ page }) => {
  await openBoth(page); fs.unlinkSync(b); await restart(page);
  expect((await session(page)).paths).toEqual([a]);
  await expect(page.locator('#content')).toContainText('保留原稿');
});

test('an explicit file URL restores previous pages then activates the requested file', async ({ page }) => {
  await page.evaluate(a => loadFile(a), a); await page.evaluate(() => ReadMDSession.flush());
  await page.goto('/?file=' + encodeURIComponent(b)); await page.evaluate(() => window.__readmdSessionReady);
  expect(await session(page)).toEqual({ paths: [a,b], active: b, mode: 'file' });
});

test('a delayed restore never overwrites a newer user open', async ({ page }) => {
  await page.evaluate(a => loadFile(a), a); await page.evaluate(() => ReadMDSession.flush());
  let release, requested = false; const hold = new Promise(r => release = r);
  await page.route('**/api/file?*', async route => {
    if (new URL(route.request().url()).searchParams.get('p') === a) { requested = true; await hold; }
    await route.continue();
  });
  await page.reload(); await expect.poll(() => requested).toBe(true);
  await page.evaluate(b => loadFile(b), b); release();
  await page.evaluate(() => window.__readmdSessionReady);
  expect((await session(page)).paths).toEqual([b]);
  await expect(page.locator('#content')).toContainText('比较证据');
});

test('saving on app exit preserves open pages and cancellation retains edits', async ({ page }) => {
  await openBoth(page);
  await page.evaluate(async () => { await toggleEdit(); cmView.dispatch({ changes: { from: cmView.state.doc.length, insert: '\n已核对。' } }); window.exitResult = ReadMDRecovery.prepareClose(); });
  await page.locator('#close-confirm-cancel').click();
  expect(await page.evaluate(() => window.exitResult)).toBe(false);
  expect(await page.evaluate(() => getEditContent())).toContain('已核对');
  await page.evaluate(() => { window.exitResult = ReadMDRecovery.prepareClose(); });
  await page.locator('#close-confirm-save').click();
  expect(await page.evaluate(() => window.exitResult)).toBe(true);
  await restart(page);
  expect((await session(page)).paths).toEqual([a,b]);
  expect(fs.readFileSync(b, 'utf8')).toContain('已核对');
});

test('discard on app exit keeps a real page open and its draft recoverable', async ({ page }) => {
  await page.evaluate(async a => { await loadFile(a); await toggleEdit(); cmView.dispatch({ changes: { from: 0, to: cmView.state.doc.length, insert: '暂不采用的修改' } }); window.exitResult = ReadMDRecovery.prepareClose(); }, a);
  await page.locator('#close-confirm-discard').click();
  expect(await page.evaluate(() => window.exitResult)).toBe(true);
  await restart(page); expect((await session(page)).paths).toEqual([a]);
  await expect(page.locator('#content')).toContainText('保留原稿');
  const entries = await page.evaluate(async () => (await (await apiFetch('/api/documents/history')).json()).entries);
  expect(entries.some(e => e.kind === 'discarded' && e.path === a)).toBe(true);
});
