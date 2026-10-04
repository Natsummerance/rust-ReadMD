const { test, expect } = require('@playwright/test');
const fs = require('node:fs');
const path = require('node:path');
const json = (route, body, status = 200) => route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));

test.beforeEach(async ({ page }) => {
  await page.route('**/api/update/check', route => json(route, { ok: false }));
  await page.addInitScript(() => localStorage.setItem('readmd_language', 'en'));
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.goto('/');
  await page.waitForFunction(() => typeof runEditAiAction === 'function' && window.ReadMDTask);
});

async function edit(page, text = '# Original\n\nBody\n') {
  await page.evaluate(async text => { await renderVirtual('clipboard', 'audit.md', '', text, []); await toggleEdit(); }, text);
  await expect(page.locator('.cm-editor')).toBeVisible();
}
async function replace(page, text) {
  await page.evaluate(text => cmView.dispatch({ changes: { from: 0, to: cmView.state.doc.length, insert: text } }), text);
}
const doc = page => page.evaluate(() => cmView.state.doc.toString());
async function mockAi(page) {
  await page.evaluate(() => { ensureAiConfigured = async () => ({ provider: 'fixture', model: 'fixture', credential_id: 'cred:testfixture' }); });
}

test('F006 returning home protects a dirty editor on cancel and discards only after confirmation', async ({ page }) => {
  await edit(page); await replace(page, 'UNSAVED DRAFT');
  await page.locator('#btn-home').click();
  await expect(page.locator('#close-confirm-modal')).toBeVisible();
  await page.locator('#close-confirm-modal').getByRole('button', { name: /cancel/i }).click();
  expect(await doc(page)).toBe('UNSAVED DRAFT');
  expect(await page.evaluate(() => state.mode)).toBe('virtual');
  await page.locator('#btn-home').click();
  await page.locator('#close-confirm-modal').getByRole('button', { name: /discard|don.t save/i }).click();
  await expect.poll(() => page.evaluate(() => state.mode)).toBe('welcome');
  expect(await page.evaluate(() => state.tabs[0].content)).toContain('# Original');
});

test('F006 save before home writes the actual file before leaving the editor', async ({ page }) => {
  const source = await page.evaluate(async () => {
    const path = await uploadFile(new File(['# Saved\n'], 'home-save.md', { type: 'text/markdown' }));
    await loadFile(path); await toggleEdit(); return path;
  });
  await replace(page, '# Saved\n\nPersisted before home.');
  await page.locator('#btn-home').click();
  await page.locator('#close-confirm-save').click();
  await expect.poll(() => page.evaluate(() => state.mode)).toBe('welcome');
  expect(fs.readFileSync(source, 'utf8')).toBe('# Saved\n\nPersisted before home.');
});

test('F082 AI repair preserves newer edits and creates a dirty copy instead of overwriting another document', async ({ page }) => {
  let requests = 0, release;
  const held = new Promise(resolve => { release = resolve; });
  await page.route('**/api/ai/chat', async route => { requests++; await held; await json(route, { ok: true, content: '# Repaired\n' }); });
  await edit(page); await mockAi(page);
  const origin = await page.evaluate(() => { void handleAiDocumentFix(); void handleAiDocumentFix(); return state.activeTabId; });
  await expect.poll(() => requests).toBe(1);
  await replace(page, 'NEWER EDIT');
  await page.evaluate(() => renderVirtual('clipboard', 'other.md', '', 'OTHER DOCUMENT', []));
  release();
  await expect.poll(() => page.evaluate(() => state.tabs.length)).toBe(3);
  const tabs = await page.evaluate(() => state.tabs.map(t => ({ id: t.id, content: t.content, isDirty: t.isDirty, name: t.name })));
  expect(tabs.find(t => t.id === origin).content).toBe('NEWER EDIT');
  expect(tabs.find(t => t.name === 'other.md').content).toBe('OTHER DOCUMENT');
  expect(tabs.at(-1)).toMatchObject({ content: '# Repaired', isDirty: true });
});

test('F082 repair in reading mode keeps the source intact and in an unchanged editor remains undoable', async ({ page }) => {
  await page.route('**/api/ai/chat', route => json(route, { ok: true, content: '# Repaired' }));
  await page.evaluate(() => renderVirtual('clipboard', 'read.md', '', '# Source', []));
  await mockAi(page); await page.evaluate(() => handleAiDocumentFix());
  expect(await page.evaluate(() => state.tabs[0].original)).toBe('# Source');
  expect(await page.evaluate(() => state.tabs[1].isDirty)).toBe(true);
  await page.evaluate(() => toggleEdit()); await replace(page, '# Before');
  await page.evaluate(() => handleAiDocumentFix());
  expect(await doc(page)).toBe('# Repaired');
  await page.evaluate(() => window.ReadMDCodeMirror.undo(cmView));
  expect(await doc(page)).toBe('# Before');
});

test('F045 closed inline AI ignores a late result and a changed tab cannot receive its old selection', async ({ page }) => {
  await edit(page, 'unique selection'); await mockAi(page);
  await page.route('**/api/ai/chat', async route => { await pause(120); await json(route, { ok: true, content: 'AI REPLY' }).catch(() => {}); });
  await page.evaluate(() => { cmView.dispatch({ selection: { anchor: 0, head: cmView.state.doc.length } }); openEditAiBar(); void runEditAiAction('polish'); closeEditAiBar(); openEditAiBar(); });
  await pause(220);
  expect(await page.evaluate(() => editAiCurrentResult)).toBe('');
  await expect(page.locator('#edit-ai-submit')).toBeEnabled();
  await page.evaluate(() => runEditAiAction('polish'));
  const result = await page.evaluate(() => ({ snapshot: editAiSnapshot, result: editAiCurrentResult }));
  await page.evaluate(async result => {
    await renderVirtual('clipboard', 'same-text.md', '', 'unique selection', []); await toggleEdit();
    editAiSnapshot = result.snapshot; editAiCurrentResult = result.result; applyEditAiResult();
  }, result);
  await expect.poll(() => page.evaluate(() => state.tabs.length)).toBe(3);
  expect(await page.evaluate(() => state.tabs[1].content)).toBe('unique selection');
  expect(await page.evaluate(() => state.tabs[2].content)).toBe('AI REPLY');
});

test('F036 metadata reads existing values, escapes quotes and keeps unknown/nested fields', async ({ page }) => {
  const text = '---\ntitle: "Old title"\nauthor: \'O\'\'Brien\'\nbibliography: refs.bib\ncustom:\n  labels: [one, two]\npresentation:\n    theme: moon\n    transition: fade\n    controls: false\n---\n\n# Body\n';
  await edit(page, text); await page.evaluate(() => openFrontmatterModal());
  await expect(page.locator('#fm-input-title')).toHaveValue('Old title');
  await expect(page.locator('#fm-input-author')).toHaveValue("O'Brien");
  await expect(page.locator('#fm-select-theme')).toHaveValue('moon');
  const title = 'A "quoted" title \\ example';
  await page.locator('#fm-input-title').fill(title);
  await page.locator('#fm-select-theme').selectOption('white');
  await page.locator('#fm-modal-insert').click();
  const value = await doc(page);
  expect(value).toContain('title: ' + JSON.stringify(title));
  expect(value).toContain("author: 'O''Brien'");
  expect(value).toContain('bibliography: refs.bib\ncustom:\n  labels: [one, two]');
  expect(value).toContain('controls: false');
  expect(value).toContain('theme: "white"');
  expect(value.endsWith('# Body\n')).toBe(true);
  await page.evaluate(() => openFrontmatterModal());
  await expect(page.locator('#fm-input-title')).toHaveValue(title);
});

test('F036 flow presentation metadata preserves nested options and an ordinary rule is not a YAML header', async ({ page }) => {
  await edit(page, '---\ntitle: Old\npresentation: {theme: moon, transition: fade, controls: false, nested: {a: 1, b: 2}}\n---\nBody');
  await page.evaluate(() => openFrontmatterModal());
  await page.locator('#fm-select-transition').selectOption('none'); await page.locator('#fm-modal-insert').click();
  expect(await doc(page)).toContain('nested: {a: 1, b: 2}');
  expect(await doc(page)).toContain('controls: false');
  await replace(page, '---not a header\nBody\n----\nTail');
  await page.evaluate(() => openFrontmatterModal()); await page.locator('#fm-modal-insert').click();
  expect(await doc(page)).toContain('---not a header\nBody\n----\nTail');
});

test('F033–F035 insertion panels reject empty/invalid input and preserve embedded fences and drafts', async ({ page }) => {
  await edit(page, 'Body'); await page.evaluate(() => openCodeChunkModal());
  await page.locator('#code-chunk-code').fill(''); await page.locator('#code-chunk-insert').click();
  await expect(page.locator('#code-chunk-modal')).toBeVisible(); expect(await doc(page)).toBe('Body');
  await page.locator('#code-chunk-code').fill('console.log("```")'); await page.locator('#code-chunk-insert').click();
  expect(await doc(page)).toContain('````');
  await page.evaluate(() => openDiagramModal()); await page.locator('#diagram-code').fill('digraph { A -> B }');
  await page.evaluate(() => { closeDiagramModal(); openDiagramModal(); });
  await expect(page.locator('#diagram-code')).toHaveValue('digraph { A -> B }');
  await page.evaluate(() => { closeDiagramModal(); openDocImportModal(); });
  for (const path of ['', 'bad"path.md']) {
    await page.locator('#doc-import-path').fill(path); await page.locator('#doc-import-insert').click();
    await expect(page.locator('#doc-import-modal')).toBeVisible();
  }
  await page.locator('#doc-import-path').fill('./child.md'); await page.locator('#doc-import-mode').selectOption('code');
  await page.locator('#doc-import-lines').fill('20-10'); await page.locator('#doc-import-insert').click();
  await expect(page.locator('#doc-import-modal')).toBeVisible();
  await page.locator('#doc-import-lines').fill('10 - 20'); await page.locator('#doc-import-insert').click();
  expect(await doc(page)).toContain('@import "./child.md" {mode="code" lines="10-20"}');
});

test('F013 a failed folder read preserves the tree and an older response cannot replace a newer folder', async ({ page }) => {
  await page.route('**/api/list?*', async route => {
    const p = new URL(route.request().url()).searchParams.get('p');
    if (p === 'bad') return json(route, { ok: false, error: 'Read denied' }, 403);
    await pause(p === 'old' ? 150 : 10); return json(route, { dir: p, files: [p + '/a.md'] });
  });
  await page.evaluate(() => listFolder('good')); await page.evaluate(() => listFolder('bad'));
  expect(await page.evaluate(() => state.folder)).toBe('good');
  await page.evaluate(() => Promise.all([listFolder('old'), listFolder('new')]));
  expect(await page.evaluate(() => state.folder)).toBe('new');
  await expect(page.locator('#file-list')).toContainText('new');
});

test('F077 share failure is visible, retry works and duplicate actions send one request', async ({ page }) => {
  let statusFails = true, running = false, starts = 0;
  await page.route('**/api/share/status', route => statusFails ? json(route, { ok: false, error: 'Status unavailable' }, 500) : json(route, { ok: true, running }));
  await page.route('**/api/share/start', async route => { starts++; await pause(100); return json(route, { ok: false, error_code: 'start_failed' }); });
  await page.route('**/api/share/stop', route => json(route, { ok: false, error: 'Stop denied' }, 500));
  await page.evaluate(() => openShareModal());
  await expect(page.locator('#share-qr')).toContainText('Status unavailable'); await expect(page.locator('#share-start')).toBeDisabled();
  statusFails = false; await page.locator('#share-refresh').click(); await expect(page.locator('#share-start')).toBeEnabled();
  const successToasts = await page.evaluate(async () => {
    const events = []; const original = showToast; showToast = text => events.push(text);
    try { await Promise.all([startShare(), startShare()]); await stopShare(); } finally { showToast = original; }
    return events;
  });
  expect(starts).toBe(1);
  expect(successToasts.some(text => /started|stopped|开启|关闭/.test(text))).toBe(false);
  expect(successToasts.join(' ')).toContain('start_failed'); expect(successToasts.join(' ')).toContain('Stop denied');
});

test('F077 the real Rust LAN service starts, rejects anonymous requests, serves its URL and stops', async ({ page, request }) => {
  const original = '# Shared heading\n\nReal **formatted** content.';
  const source = await page.evaluate(async original => { const path = await uploadFile(new File([original], 'lan-audit.md', { type: 'text/markdown' })); await loadFile(path); return path; }, original);
  await page.evaluate(() => openShareModal()); await page.locator('#share-start').click();
  const status = await page.evaluate(async () => (await apiFetch('/api/share/status')).json());
  expect(status.running).toBe(true);
  try {
    const url = new URL(status.url); url.hostname = '127.0.0.1'; url.searchParams.set('token', status.token);
    const response = await request.get(url.href); expect(response.ok()).toBe(true); expect(await response.text()).toContain('<html');
    expect(await response.text()).toContain('<a href=');
    const documentUrl = new URL(url.href); documentUrl.pathname = '/' + encodeURIComponent(path.basename(source));
    const rendered = await request.get(documentUrl.href);
    expect(await rendered.text()).toContain('<h1>Shared heading</h1>'); expect(await rendered.text()).toContain('<strong>formatted</strong>');
    documentUrl.searchParams.set('raw', '1');
    const raw = await request.get(documentUrl.href); expect(await raw.text()).toBe(original);
    const head = await request.head(documentUrl.href); expect(Number(head.headers()['content-length'])).toBe(Buffer.byteLength(original));
    const anonymous = await request.get(url.origin + '/'); expect(anonymous.status()).toBe(403);
    await expect(page.locator('#share-qr img')).toBeVisible();
    expect(await page.evaluate(() => $('share-url').textContent.includes('?token='))).toBe(true);
  } finally { await page.evaluate(() => stopShare()); }
  await expect.poll(() => page.evaluate(async () => (await apiFetch('/api/share/status')).json().then(d => d.running))).toBe(false);
});

test('F083 style loading locks inputs, retries failures and a save never closes over newer edits', async ({ page }) => {
  let fails = true, saves = 0;
  await page.route('**/api/style/get', async route => { await pause(100); return json(route, fails ? { ok: false, error: 'Storage unavailable' } : { ok: true, data: { css: '/* saved */', head: '' } }, fails ? 500 : 200); });
  await page.route('**/api/style/save', async route => { saves++; await pause(180); return json(route, { ok: true }); });
  await page.evaluate(() => { void openStyleModal(); }); await expect(page.locator('#style-custom-css')).toBeDisabled();
  await expect(page.locator('#style-load-status')).toContainText('Storage unavailable');
  fails = false; await page.locator('#style-load-retry').click();
  await expect(page.locator('#style-custom-css')).toBeEnabled(); await expect(page.locator('#style-custom-css')).toHaveValue('/* saved */');
  await page.locator('#style-custom-css').fill('/* request */');
  await page.evaluate(() => { void saveStyleModal(); void saveStyleModal(); });
  await expect.poll(() => saves).toBe(1); await page.locator('#style-custom-css').fill('/* newer */');
  await expect(page.locator('#style-modal-save')).toBeEnabled(); await expect(page.locator('#style-custom-modal')).toBeVisible();
  await expect(page.locator('#style-custom-css')).toHaveValue('/* newer */');
});

test('F016 corrupted settings cannot overwrite document state or inject invalid layout values', async ({ page }) => {
  await page.evaluate(async () => {
    localStorage.setItem('readmd-settings', JSON.stringify({ theme: 'invalid', fontSize: -500, lineWidth: 'auto', pvSplitX: 10000, readingWidth: 'broken', file: 'evil', tabs: [], ai: null, pvLayout: 'none' }));
    await renderVirtual('clipboard', 'kept.md', '', 'KEPT', []); await loadSettings();
  });
  expect(await page.evaluate(() => ({ count: state.tabs.length, file: state.file, fontSize: state.fontSize, split: state.pvSplitX, layout: state.pvLayout, ai: !!state.ai }))).toEqual({ count: 1, file: null, fontSize: 70, split: 70, layout: 'none', ai: true });
  expect(await page.evaluate(() => document.body.dataset.theme)).not.toBe('invalid');
});

test('F005 native chooser and recent-clear failures show feedback without unhandled rejections', async ({ page }) => {
  const errors = []; page.on('pageerror', error => errors.push(error.message));
  const messages = await page.evaluate(async () => {
    const previous = { hasPy, py, showToast }; const messages = [];
    hasPy = true; py = { choose_file: async () => { throw new Error('Dialog unavailable'); }, clear_recent: async () => false };
    showToast = text => messages.push(text);
    try { await loadFileDialog(); await clearRecent(); } finally { hasPy = previous.hasPy; py = previous.py; showToast = previous.showToast; }
    return messages;
  });
  expect(messages.join(' ')).toContain('Dialog unavailable'); expect(messages.join(' ')).toContain('Could not clear'); expect(errors).toEqual([]);
});

test('F021 an empty original stays empty and restoring an empty draft clears dirty without saving it', async ({ page }) => {
  await edit(page, ''); await replace(page, 'Draft');
  await page.evaluate(() => { exitEdit(); syncStateFromActiveTab(); });
  expect(await page.evaluate(() => state.original)).toBe('');
  await page.evaluate(() => toggleEdit());
  // State sync must not convert an unsaved draft into a clean original.
  expect(await page.evaluate(() => hasUnsavedEditorChanges())).toBe(true);
  await replace(page, '');
  expect(await page.evaluate(() => getActiveTab().isDirty)).toBe(false);
  expect(await page.evaluate(() => getActiveTab().isVirtual)).toBe(true);
  expect(await page.evaluate(() => hasUnsavedEditorChanges())).toBe(false);
  await page.evaluate(() => openTableModal());
  const cell = page.locator('.table-grid-cell[data-row="2"][data-col="3"]');
  await cell.focus(); await page.keyboard.press('ArrowRight'); await page.keyboard.press('Enter');
  const table = await doc(page); expect(table.split('\n').filter(line => line.startsWith('|'))[0].split('|')).toHaveLength(6);
});

test('F085 update polling catches failures, never overlaps and reports a failed cancellation', async ({ page }) => {
  const errors = []; page.on('pageerror', error => errors.push(error.message));
  let concurrent = 0, peak = 0, polls = 0;
  await page.route('**/api/update/download', route => json(route, { ok: true }));
  await page.route('**/api/update/status', async route => {
    concurrent++; peak = Math.max(peak, concurrent); polls++;
    await pause(500); concurrent--; await json(route, { ok: false, error: 'Status offline' }, 503);
  });
  await page.route('**/api/update/cancel', route => json(route, { ok: false, error: 'Cancel denied' }, 500));
  await page.evaluate(() => {
    updateInfo = { flavor: 'win_installer', asset: { name: 'fixture.exe', download_url: 'https://example.invalid/fixture.exe', expected_sha: 'a'.repeat(64) } };
    openUpdateModal(); return startUpdateDownload();
  });
  await expect.poll(() => polls).toBeGreaterThanOrEqual(3);
  await expect(page.locator('#update-progress-text')).toContainText('Status offline');
  expect(peak).toBe(1);
  await page.locator('#btn-update-cancel').click(); await expect(page.locator('#toast')).toContainText('Cancel denied');
  await page.evaluate(() => { clearInterval(updateTimer); updateTimer = null; ++updateJobEpoch; isUpdating = false; });
  expect(errors).toEqual([]);
});

test('F085 a verified update with a failed installer can retry installation without downloading again', async ({ page }) => {
  let downloads = 0, applies = 0;
  await page.route('**/api/update/download', route => { downloads++; return json(route, { ok: true }); });
  await page.route('**/api/update/status', route => json(route, { status: 'ready', target_file: 'C:/fixture/verified.exe' }));
  await page.route('**/api/update/apply', route => { applies++; return json(route, { ok: false, error: 'Installer launch denied' }, 500); });
  await page.evaluate(() => {
    updateInfo = { flavor: 'win_installer', asset: { name: 'fixture.exe', download_url: 'https://example.invalid/fixture.exe', expected_sha: 'a'.repeat(64) } };
    openUpdateModal(); return startUpdateDownload();
  });
  await expect(page.locator('#update-progress-text')).toContainText('Installer launch denied');
  await expect(page.locator('#btn-update-start')).toHaveText('Retry installation');
  await page.locator('#btn-update-start').click(); await expect.poll(() => applies).toBe(2);
  expect(downloads).toBe(1); await expect(page.locator('#btn-update-start')).toBeEnabled();
});

test('F021 changing documents during editor loading cannot mount the old editor over the new document', async ({ page }) => {
  let requested = false, release;
  const held = new Promise(resolve => { release = resolve; });
  await page.route('**/codemirror.bundle.js*', async route => { requested = true; await held; await route.continue(); });
  await page.evaluate(async () => { await renderVirtual('clipboard', 'first.md', '', 'FIRST', []); void toggleEdit(); });
  await expect.poll(() => requested).toBe(true);
  await page.evaluate(() => renderVirtual('clipboard', 'second.md', '', 'SECOND', []));
  release(); await page.waitForFunction(() => window.ReadMDCodeMirror);
  expect(await page.evaluate(() => state.editing)).toBe(false);
  expect(await page.evaluate(() => state.tabs[0].content)).toBe('FIRST');
  await expect(page.locator('#content')).toContainText('SECOND');
  await expect(page.locator('.cm-editor')).toHaveCount(0);
});

test('F077 unsaved documents share their actual content through an isolated snapshot and can restart', async ({ page, request }) => {
  await edit(page, '# Unsaved\n\nSnapshot body.'); await replace(page, '# Unsaved\n\nLatest draft.');
  await page.evaluate(() => startShare());
  const first = await page.evaluate(async () => (await apiFetch('/api/share/status')).json());
  const url = new URL(first.url); url.hostname = '127.0.0.1'; url.pathname = '/shared-document.md'; url.searchParams.set('token', first.token);
  try {
    expect(await (await request.get(url.href)).text()).toContain('Latest draft.');
    url.searchParams.set('raw', '1'); expect(await (await request.get(url.href)).text()).toBe('# Unsaved\n\nLatest draft.');
    expect(await page.evaluate(() => getActiveTab().isDirty)).toBe(true);
    await page.evaluate(() => stopShare());
    await page.evaluate(() => startShare());
    const second = await page.evaluate(async () => (await apiFetch('/api/share/status')).json());
    expect(second.running).toBe(true);
    // A stopped session must not keep serving when a new one resets its state.
    await expect.poll(async () => { try { await request.get(url.href, { timeout: 1000 }); return false; } catch { return true; } }).toBe(true);
  } finally { await page.evaluate(() => stopShare()); }
});

test('F082 textarea fallback repairs the current draft into a safe copy and retains its source', async ({ page }) => {
  await page.route('**/codemirror.bundle.js*', route => route.fulfill({ status: 503, body: 'Unavailable' }));
  let sent;
  await page.route('**/api/ai/chat', route => { sent = route.request().postDataJSON(); return json(route, { ok: true, content: '# Repaired fallback' }); });
  await page.evaluate(async () => { await renderVirtual('clipboard', 'fallback.md', '', '# Original', []); await toggleEdit(); });
  await expect(page.locator('#edit-area')).toBeVisible(); await page.locator('#edit-area').fill('# Current fallback draft');
  await mockAi(page); await page.evaluate(() => handleAiDocumentFix());
  expect(sent.skill_variables.document).toBe('# Current fallback draft');
  expect(await page.evaluate(() => state.tabs[0].content)).toBe('# Current fallback draft');
  expect(await page.evaluate(() => state.tabs[1].content)).toBe('# Repaired fallback');
  expect(await page.evaluate(() => state.tabs[1].isDirty)).toBe(true);
});
