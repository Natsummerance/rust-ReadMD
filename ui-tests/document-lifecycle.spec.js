const { test, expect } = require('@playwright/test');
const fs = require('node:fs/promises');
const path = require('node:path');
const os = require('node:os');
test.beforeEach(async ({ page }) => {
  await page.route('**/api/update/check', route => route.fulfill({ status: 200, contentType: 'application/json', body: '{"ok":false}' }));
  await page.addInitScript(() => localStorage.setItem('readmd_language', 'en'));
  await page.goto('/');
  await page.waitForFunction(() => window.ReadMDRecovery && typeof saveAs === 'function');
});
async function file(page, text = '# Original\n') {
  return page.evaluate(async text => {
    const path = await uploadFile(new File([text], 'lifecycle.md', { type: 'text/markdown' }));
    await loadFile(path); await toggleEdit(); return path;
  }, text);
}
async function change(page, text) { await page.evaluate(text => cmView.dispatch({ changes: { from: 0, to: cmView.state.doc.length, insert: text } }), text); }
const current = page => page.evaluate(() => getEditContent());
const history = page => page.evaluate(async () => (await (await apiFetch('/api/documents/history')).json()).entries);

test('saving preserves undo and a central version without sibling backups', async ({ page }) => {
  const source = await file(page); await change(page, '# Saved\n');
  expect(await page.evaluate(() => saveEdit())).toBe(true);
  expect(await fs.readFile(source, 'utf8')).toBe('# Saved\n');
  expect((await history(page)).some(e => e.kind === 'version' && e.path === source)).toBe(true);
  await expect(fs.access(source + '.bak')).rejects.toThrow();
  await page.evaluate(() => cmUndo()); expect(await current(page)).toBe('# Original\n');
  expect(await page.evaluate(() => getActiveTab().isDirty)).toBe(true);
});
test('a late save acknowledges only submitted text and retains newer input', async ({ page }) => {
  const source = await file(page); await change(page, 'SUBMITTED');
  let release; const held = new Promise(resolve => release = resolve);
  await page.route('**/api/save', async route => { await held; const response = await route.fetch(); await route.fulfill({ response }); });
  await page.evaluate(() => { window.pendingSave = saveEdit({ exitAfterSave: true }); });
  await expect(page.locator('#edit-save')).toBeDisabled();
  await change(page, 'NEWER DRAFT'); release();
  expect(await page.evaluate(() => window.pendingSave)).toBe(false);
  expect(await fs.readFile(source, 'utf8')).toBe('SUBMITTED');
  expect(await current(page)).toBe('NEWER DRAFT');
  expect(await page.evaluate(() => ({ editing: state.editing, dirty: getActiveTab().isDirty, original: state.original }))).toEqual({ editing: true, dirty: true, original: 'SUBMITTED' });
});
test('a save response cannot change a different active document', async ({ page }) => {
  const source = await file(page); await change(page, 'SOURCE SAVED');
  const id = await page.evaluate(() => state.activeTabId);
  let release; const held = new Promise(resolve => release = resolve);
  await page.route('**/api/save', async route => { await held; const response = await route.fetch(); await route.fulfill({ response }); });
  await page.evaluate(() => { window.pendingSave = saveEdit(); });
  await expect(page.locator('#edit-save')).toBeDisabled();
  await page.evaluate(async () => { await renderVirtual('clipboard', 'other.md', '', 'OTHER DRAFT', []); await toggleEdit(); });
  release(); expect(await page.evaluate(() => window.pendingSave)).toBe(true);
  expect(await current(page)).toBe('OTHER DRAFT');
  expect(await page.evaluate(id => state.tabs.find(tab => tab.id === id).original, id)).toBe('SOURCE SAVED');
  expect(await fs.readFile(source, 'utf8')).toBe('SOURCE SAVED');
});
test('Save As and editing copy use the current draft without silently creating files', async ({ page }) => {
  const source = await file(page); await change(page, 'CURRENT DRAFT');
  await page.evaluate(async () => { hasPy = true; py = { save_as: async (...args) => { window.saveAsArgs = args; return null; } }; await saveAs(); });
  expect(await page.evaluate(() => window.saveAsArgs[0])).toBe('CURRENT DRAFT');
  expect(await fs.readFile(source, 'utf8')).toBe('# Original\n');
  await page.evaluate(() => ReadMDRecovery.createCopy());
  expect(await current(page)).toBe('CURRENT DRAFT');
  expect(await page.evaluate(() => ({ path: state.file, source: getActiveTab().source, dirty: getActiveTab().isDirty }))).toEqual({ path: null, source: 'copy', dirty: true });
});
test('AI copy saving always uses a chooser even if its origin has a directory', async ({ page }) => {
  const source = await file(page);
  await page.evaluate(async source => {
    await renderVirtual('ai', 'AI-note.md', source.replace(/[\\/][^\\/]*$/, ''), 'AI CONTENT', [], { originPath: source }); await toggleEdit();
    hasPy = true; py = { save_as: async content => { window.chosenAiContent = content; return null; }, save_file: () => { throw new Error('silent sibling save'); } };
  }, source);
  expect(await page.evaluate(() => saveEdit())).toBe(false);
  expect(await page.evaluate(() => window.chosenAiContent)).toBe('AI CONTENT');
  expect(await page.evaluate(() => state.file)).toBe(null);
});
test('recovery draft survives reload and opens safely without rewriting its source', async ({ page }) => {
  const source = await file(page); await change(page, 'RECOVER ME');
  await page.evaluate(() => ReadMDRecovery.flush());
  const entry = (await history(page)).find(e => e.path === source && e.kind === 'draft'); expect(entry).toBeTruthy();
  await page.evaluate(() => { window.onbeforeunload = null; }); await page.reload();
  await page.waitForFunction(() => window.ReadMDRecovery);
  await page.evaluate(() => ReadMDRecovery.open());
  const row = page.locator('[data-history-id="' + entry.id + '"]');
  await row.getByRole('button', { name: 'Open as copy', exact: true }).click();
  await expect.poll(() => current(page)).toBe('RECOVER ME');
  expect(await page.evaluate(() => state.file)).toBe(null);
  expect(await fs.readFile(source, 'utf8')).toBe('# Original\n');
});
test('restoring a saved version is one undoable edit and does not write until Save', async ({ page }) => {
  const source = await file(page); await change(page, 'AFTER SAVE'); await page.evaluate(() => saveEdit());
  await page.evaluate(() => ReadMDRecovery.open());
  await page.locator('.document-history-row').filter({ hasText: source }).filter({ hasText: 'Previous saved version' }).getByRole('button', { name: 'Apply to current editor' }).click();
  await expect.poll(() => current(page)).toBe('# Original\n'); expect(await fs.readFile(source, 'utf8')).toBe('AFTER SAVE');
  await page.evaluate(() => cmUndo()); expect(await current(page)).toBe('AFTER SAVE');
});
test('closing with discard retains a recoverable draft and native-close cancellation preserves the editor', async ({ page }) => {
  const source = await file(page); await change(page, 'DISCARDED DRAFT');
  await page.evaluate(() => { window.closeResult = ReadMDRecovery.prepareClose(); });
  await expect(page.locator('#close-confirm-modal')).toBeVisible();
  await page.locator('#close-confirm-cancel').click(); expect(await page.evaluate(() => window.closeResult)).toBe(false);
  expect(await current(page)).toBe('DISCARDED DRAFT');
  await page.evaluate(() => { window.closeResult = confirmExitEdit(); });
  await page.locator('#close-confirm-discard').click(); expect(await page.evaluate(() => window.closeResult)).toBe(true);
  expect((await history(page)).some(e => e.kind === 'discarded' && e.path === source)).toBe(true);
  expect(await fs.readFile(source, 'utf8')).toBe('# Original\n');
});
test('undo history survives switching away and reopening an edited tab', async ({ page }) => {
  await file(page); const id = await page.evaluate(() => state.activeTabId); await change(page, 'CHANGED');
  await page.evaluate(async () => { await renderVirtual('clipboard', 'other.md', '', 'Other', []); });
  await page.evaluate(async id => { await switchTab(id); await toggleEdit(); }, id);
  await page.evaluate(() => cmUndo()); expect(await current(page)).toBe('# Original\n');
});
test('single conversion preview writes nothing, explicit overwrite keeps the earlier Markdown', async ({ page, request }) => {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'readmd-convert-lifecycle-'));
  try {
    const source = path.join(dir, 'report.txt'), target = path.join(dir, 'report.md');
    await fs.writeFile(source, 'Converted text');
    const root = '/api/convert?p=' + encodeURIComponent(source);
    const preview = await request.get(root + '&preview=1'); expect(preview.ok()).toBe(true);
    expect((await preview.json()).saved).toBe(false); await expect(fs.access(target)).rejects.toThrow();
    await fs.writeFile(target, 'PREVIOUS OUTPUT');
    const overwritten = await request.get(root + '&on_exists=overwrite'); expect((await overwritten.json()).saved).toBe(true);
    const entry = (await history(page)).find(e => e.path === target && e.kind === 'version'); expect(entry).toBeTruthy();
    const old = await page.evaluate(async id => (await (await apiFetch('/api/documents/history', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ op: 'read', id }) })).json()).content, entry.id);
    expect(old).toBe('PREVIOUS OUTPUT'); expect((await fs.readdir(dir)).sort()).toEqual(['report.md', 'report.txt']);
  } finally { await fs.rm(dir, { recursive: true, force: true }); }
});
test('recovery panel stays reachable in both target windows', async ({ page }) => {
  await file(page); await change(page, 'Draft');
  for (const size of [{ width: 1160, height: 820 }, { width: 1024, height: 680 }]) {
    await page.setViewportSize(size); await page.evaluate(() => ReadMDRecovery.open());
    const rect = await page.locator('.document-history-box').boundingBox(); expect(rect.x).toBeGreaterThanOrEqual(0); expect(rect.y).toBeGreaterThanOrEqual(0);
    expect(rect.x + rect.width).toBeLessThanOrEqual(size.width); expect(rect.y + rect.height).toBeLessThanOrEqual(size.height);
    const refresh = await page.locator('#document-history-refresh').boundingBox(); expect(refresh.height).toBeGreaterThanOrEqual(44);
    await page.locator('#document-history-close').click();
  }
});

test('Save As retargets to acknowledged asset paths and keeps the operation undoable', async ({ page }) => {
  await file(page); await change(page, '![image](temporary/image.png)');
  await page.evaluate(async () => {
    hasPy = true; py = { save_as: async () => ({ ok: true, path: 'C:/chosen/new.md', mtime: 12, revision: 'revision', saved_content: '![image](new.assets/image.png)' }) };
    await saveAs();
  });
  expect(await current(page)).toBe('![image](new.assets/image.png)');
  expect(await page.evaluate(() => ({ original: state.original, path: state.file, dirty: getActiveTab().isDirty }))).toEqual({ original: '![image](new.assets/image.png)', path: 'C:/chosen/new.md', dirty: false });
  await page.evaluate(() => cmUndo()); expect(await current(page)).toBe('![image](temporary/image.png)');
  expect(await page.evaluate(() => getActiveTab().isDirty)).toBe(true);
});

test('a delayed history restore cannot replace input added during its checkpoint', async ({ page }) => {
  const source = await file(page); const id = await page.evaluate(() => state.activeTabId);
  await change(page, 'SAVED'); await page.evaluate(() => saveEdit()); await page.evaluate(() => ReadMDRecovery.open());
  let release; const held = new Promise(resolve => release = resolve);
  await page.route('**/api/documents/history', async route => {
    if (route.request().method() === 'POST' && route.request().postDataJSON().reason === 'restore') await held;
    const response = await route.fetch(); await route.fulfill({ response });
  });
  await page.locator('.document-history-row').filter({ hasText: source }).filter({ hasText: 'Previous saved version' }).getByRole('button', { name: 'Apply to current editor' }).click();
  await change(page, 'NEWER INPUT'); release();
  await page.waitForFunction(() => getActiveTab().source === 'recovery');
  expect(await current(page)).toBe('# Original\n');
  expect(await page.evaluate(id => state.tabs.find(tab => tab.id === id).content, id)).toBe('NEWER INPUT');
  expect(await fs.readFile(source, 'utf8')).toBe('SAVED');
});

test('Ctrl Shift S invokes Save As and sends the actual current draft', async ({ page }) => {
  await file(page); await change(page, 'SHORTCUT DRAFT');
  await page.evaluate(() => { hasPy = true; py = { save_as: async content => { window.shortcutContent = content; return null; } }; });
  await page.keyboard.press('Control+Shift+S');
  await page.waitForFunction(() => window.shortcutContent === 'SHORTCUT DRAFT');
  expect(await page.evaluate(() => getActiveTab().isDirty)).toBe(true);
});

test('failed recovery blocks discard and preserves the open editor', async ({ page }) => {
  await file(page); await change(page, 'KEEP ME');
  await page.route('**/api/documents/history', route => route.fulfill({ status: 500, contentType: 'application/json', body: '{"ok":false,"error":"storage unavailable"}' }));
  await page.evaluate(() => { window.exitResult = confirmExitEdit(); });
  await page.locator('#close-confirm-discard').click();
  expect(await page.evaluate(() => window.exitResult)).toBe(false);
  expect(await current(page)).toBe('KEEP ME'); expect(await page.evaluate(() => state.editing)).toBe(true);
});

test('external change refuses overwrite and confirmed reload retains the discarded draft centrally', async ({ page }) => {
  const source = await file(page); await change(page, 'MY DRAFT'); await fs.writeFile(source, '# EXTERNAL\n');
  await page.evaluate(() => { window.conflictSave = saveEdit(); });
  await expect(page.locator('#save-conflict-modal')).toBeVisible();
  await page.locator('#save-conflict-reload').click();
  expect(await page.evaluate(() => window.conflictSave)).toBe(true);
  expect(await fs.readFile(source, 'utf8')).toBe('# EXTERNAL\n');
  expect(await page.evaluate(() => getActiveTab().content)).toBe('# EXTERNAL\n');
  const checkpoint = (await history(page)).find(e => e.path === source && e.reason === 'conflict_reload'); expect(checkpoint).toBeTruthy();
  expect(await page.evaluate(async id => (await (await apiFetch('/api/documents/history', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ op: 'read', id }) })).json()).content, checkpoint.id)).toBe('MY DRAFT');
});

test('editing copies and recovered virtual documents retain resource references and their base directory', async ({ page }) => {
  await page.evaluate(async () => {
    await renderVirtual('url', 'clip.md', 'C:/clips', '![image](photo.png)', [], { assets: [{ path: 'C:/clips/photo.png', name: 'photo.png' }] });
    await ReadMDRecovery.createCopy(); await ReadMDRecovery.flush();
  });
  const entry = (await history(page)).find(e => e.name === 'clip-copy.md' && e.kind === 'draft');
  expect(entry.context.dir).toBe('C:/clips'); expect(entry.context.assets).toHaveLength(1);
  await page.evaluate(() => ReadMDRecovery.open());
  await page.locator('[data-history-id="' + entry.id + '"]').getByRole('button', { name: 'Open as copy', exact: true }).click();
  await page.waitForFunction(() => getActiveTab().source === 'recovery');
  expect(await page.evaluate(() => ({ dir: state.dir, assets: getActiveTab().webAssets }))).toEqual({ dir: 'C:/clips', assets: [{ path: 'C:/clips/photo.png', name: 'photo.png' }] });
});

test('saving an AI reply uses a recoverable copy and blocks every other open file as its destination', async ({ page }) => {
  const source = await file(page); await change(page, 'SOURCE DRAFT'); const id = await page.evaluate(() => state.activeTabId);
  await page.evaluate(async () => {
    state.ai.raw = 'AI REPLY'; hasPy = true;
    py = { save_as: async (...args) => { window.aiExportArgs = args; return null; } };
    await saveAiAs();
  });
  expect(await page.evaluate(() => window.aiExportArgs[0])).toBe('AI REPLY');
  expect(await page.evaluate(() => window.aiExportArgs[3].blocked_paths)).toContain(source);
  expect(await page.evaluate(() => ({ source: getActiveTab().source, dirty: getActiveTab().isDirty, path: state.file }))).toEqual({ source: 'ai', dirty: true, path: null });
  expect(await page.evaluate(id => state.tabs.find(tab => tab.id === id).content, id)).toBe('SOURCE DRAFT');
  expect(await fs.readFile(source, 'utf8')).toBe('# Original\n');
});
