const { test, expect } = require('@playwright/test');
const fs = require('node:fs');

function storedZip(files) {
  const local = [], central = []; let offset = 0;
  for (const [name, text] of files) {
    const nameBytes = Buffer.from(name), data = Buffer.from(text);
    let crc = 0xffffffff;
    for (const byte of data) { crc ^= byte; for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0); }
    crc = (crc ^ 0xffffffff) >>> 0;
    const header = Buffer.alloc(30);
    header.writeUInt32LE(0x04034b50); header.writeUInt16LE(20, 4); header.writeUInt32LE(crc, 14);
    header.writeUInt32LE(data.length, 18); header.writeUInt32LE(data.length, 22); header.writeUInt16LE(nameBytes.length, 26);
    local.push(header, nameBytes, data);
    const record = Buffer.alloc(46);
    record.writeUInt32LE(0x02014b50); record.writeUInt16LE(20, 4); record.writeUInt16LE(20, 6);
    record.writeUInt32LE(crc, 16); record.writeUInt32LE(data.length, 20); record.writeUInt32LE(data.length, 24);
    record.writeUInt16LE(nameBytes.length, 28); record.writeUInt32LE(offset, 42);
    central.push(record, nameBytes); offset += header.length + nameBytes.length + data.length;
  }
  const directory = Buffer.concat(central), end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50); end.writeUInt16LE(files.length, 8); end.writeUInt16LE(files.length, 10);
  end.writeUInt32LE(directory.length, 12); end.writeUInt32LE(offset, 16);
  return Buffer.concat([...local, directory, end]);
}
const json = (route, body) => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });
test.beforeEach(async ({ page }) => {
  await page.route('**/api/update/check', route => json(route, { ok: false }));
  await page.addInitScript(() => localStorage.setItem('readmd_language', 'en'));
  await page.goto('/');
  await page.waitForFunction(() => typeof enqueueBatchFiles === 'function' && typeof openExportModal === 'function');
});

test('code header editing opens the editor and preserves the code', async ({ page }) => {
  const code = 'fn main() { println!("ReadMD"); }\n';
  await page.evaluate(async code => {
    await renderVirtual('clipboard', 'sample.rs', '', code, []);
    state.is_code = true; state.code_lang = 'rust';
    await renderContent(code, 'sample.rs');
  }, code);
  await page.locator('#btn-code-edit').click();
  await expect(page.locator('.cm-editor')).toBeVisible();
  expect(await page.evaluate(() => window.cmView.state.doc.toString())).toBe(code);
  await page.evaluate(() => window.ReadMDPalette.open());
  const commands = await page.evaluate(() => window.ReadMDActions.list().filter(a => /^(export|view)\.presentation$/.test(a.id)).map(a => ({ id: a.id, run: String(a.run) })));
  expect(commands.map(a => a.id).sort()).toEqual(['export.presentation', 'view.presentation']);
  expect(commands.find(a => a.id === 'export.presentation').run).toContain('openExportAs');
});

test('a running batch survives closing and rejects another enqueue', async ({ page }) => {
  let launches = 0, done = false;
  const source = 'C:/fixture/kept.html';
  await page.route('**/api/modules', route => json(route, { modules: { convert: 'ready', ocr: 'ready' } }));
  await page.route('**/api/convert/batch', route => { launches++; return json(route, { job: 'kept-job', total: 1 }); });
  await page.route('**/api/convert/progress*', route => json(route, {
    running: !done, finished: done, done: done ? 1 : 0, total: 1,
    items: [{ src: source, status: done ? 'ok' : 'queued', out: done ? 'C:/fixture/kept.md' : undefined }],
  }));
  await page.evaluate(source => enqueueBatchFiles([source], false), source);
  await expect.poll(() => launches).toBe(1);
  await page.evaluate(() => closeBatchModal());
  await page.evaluate(() => openConvertModal());
  await expect(page.locator('#convert-list .batch-item')).toHaveCount(1);
  await expect(page.locator('#convert-speech-language')).toBeDisabled();
  await page.evaluate(() => enqueueBatchFiles(['C:/fixture/other.html'], false));
  expect(launches).toBe(1);
  done = true;
  await expect(page.locator('#convert-list .batch-item')).toHaveClass(/ok/);
  await expect(page.locator('#convert-speech-language')).toBeEnabled();
});

test('a real native extension installs, executes, persists toggles and uninstalls', async ({ page }, testInfo) => {
  const source = testInfo.outputPath('keywords.html');
  fs.writeFileSync(source, '<h1>Keywords</h1><p>Reading reading reading document document.</p>');
  await page.evaluate(() => openPluginModal());
  const card = page.locator('.plugin-card[data-plugin-id="jieba"]');
  await card.locator('[data-action="install"]').click();
  await page.locator('#confirm-action').click();
  await expect(card.locator('[data-action="toggle"]')).toBeChecked({ timeout: 15000 });
  const result = await page.evaluate(async text => {
    const uploaded = await uploadFile(new File([text], 'keywords.html', { type: 'text/html' }));
    return (await apiFetch('/api/convert?p=' + encodeURIComponent(uploaded))).json();
  }, fs.readFileSync(source, 'utf8'));
  expect(result.content).toContain('关键词');
  await card.locator('.plugin-switch').click();
  await expect(card.locator('[data-action="toggle"]')).not.toBeChecked();
  await expect(card.locator('[data-action="toggle"]')).toBeEnabled();
  await page.reload();
  await page.waitForFunction(() => typeof openPluginModal === 'function');
  await page.evaluate(() => openPluginModal());
  await expect(card.locator('[data-action="toggle"]')).not.toBeChecked();
  await card.locator('[data-action="uninstall"]').click();
  await page.locator('#confirm-action').click();
  await expect(card.locator('[data-action="install"]')).toBeVisible();
  const list = await page.evaluate(async () => (await apiFetch('/api/plugins/list')).json());
  expect(list.plugins.jieba.installed).toBe(false);
});

test('PDF preview shows a real page and export reuses precisely those bytes', async ({ page }, testInfo) => {
  const content = '# Exact preview\n\nThis PDF is the export artifact.\n';
  await page.evaluate(content => renderVirtual('clipboard', 'exact.md', '', content, []), content);
  await page.evaluate(() => openExportModal());
  await expect(page.locator('#export-preview-mini-content .export-native-page-image')).toBeVisible({ timeout: 15000 });
  const artifact = await page.evaluate(() => ({ data: nativeExportPreview.data, payload: JSON.parse(nativeExportPreview.signature) }));
  expect(artifact.data.mode).toBe('artifact');
  expect(Buffer.from(artifact.data.pdf, 'base64').subarray(0, 5).toString()).toBe('%PDF-');
  const out = testInfo.outputPath('exact.pdf');
  const result = await page.evaluate(async ({ payload, out }) => (await apiFetch('/api/export', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ ...payload, out_path: out }),
  })).json(), { payload: artifact.payload, out });
  expect(result.ok).toBe(true);
  expect(fs.readFileSync(out).equals(Buffer.from(artifact.data.pdf, 'base64'))).toBe(true);
  await page.locator('#export-preview-card').click();
  await expect(page.locator('#export-preview-full-page .export-native-page-image')).toBeVisible();
});

test('an emptied editor exports its empty content instead of the original', async ({ page }) => {
  await page.evaluate(() => renderVirtual('clipboard', 'empty.md', '', '# Original content', []));
  await page.evaluate(() => toggleEdit());
  await page.waitForFunction(() => !!window.cmView);
  await page.evaluate(() => window.cmView.dispatch({ changes: { from: 0, to: window.cmView.state.doc.length, insert: '' } }));
  expect(await page.evaluate(() => currentExportContent())).toBe('');
});

test('selecting an archive extracts and converts the real child documents', async ({ page }, testInfo) => {
  const archive = testInfo.outputPath('documents.zip');
  fs.writeFileSync(archive, storedZip([['first.html', '<h1>First archive document</h1><p>Actual content.</p>'], ['nested/second.csv', 'Name,Value\nReadMD,2\n']]));
  await page.evaluate(() => openConvertModal());
  await page.locator('#convert-files').click();
  await page.locator('#file-input').setInputFiles(archive);
  const rows = page.locator('#convert-list .batch-item');
  await expect(rows).toHaveCount(2);
  await expect(page.locator('#convert-list .batch-item.ok')).toHaveCount(2, { timeout: 15000 });
  const outputs = await rows.evaluateAll(items => items.map(item => item.dataset.out));
  const contents = outputs.map(file => fs.readFileSync(file, 'utf8')).join('\n');
  expect(contents).toContain('First archive document');
  expect(contents).toContain('ReadMD');
});

test('rich clipboard creation keeps headings and emphasis without Turndown', async ({ page }) => {
  await page.evaluate(async () => {
    hasPy = true;
    py = { read_clipboard: async () => ({ html: '<h2>Clipboard chapter</h2><p>A <strong>rich</strong> paragraph.</p><script>untrusted()</script>', text: 'Clipboard chapter A rich paragraph.' }) };
    await createFromClipboard();
  });
  await expect(page.locator('#content h2')).toHaveText('Clipboard chapter');
  await expect(page.locator('#content strong')).toHaveText('rich');
  expect(await page.evaluate(() => state.original)).not.toContain('untrusted');
});

test('WSD, D2 and ASCII diagram fences render real offline SVG', async ({ page }) => {
  await page.route('https://**/*', route => route.abort());
  await page.evaluate(() => renderVirtual('clipboard', 'native-diagrams.md', '',
    '```wsd\nAlice->Bob: Hello\nBob-->Alice: Reply\n```\n\n```d2\na: Reader\nb: Writer\na -> b: Convert\n```\n\n```ditaa\n+-----+\n| Box |---> Output\n+-----+\n```', []));
  await expect(page.locator('.diagram-preview svg')).toHaveCount(3);
  await expect(page.locator('.diagram-preview .diagram-fallback')).toHaveCount(0);
});
