'use strict';
// Real Windows WebView/CSP and disk transactions. Isolated profile only; the
// optional media recorder captures this page, never the desktop or a picker.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const assert = require('node:assert/strict'), { chromium } = require('playwright');
const { startNative } = require('./native-session.cjs');
const root = path.resolve(__dirname, '..');
const hash = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const report = { realNativeCsp: true, isolatedProfile: true, scenarios: [], pageErrors: [] };
async function until(fn, timeout = 20000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) { if (await fn()) return; await delay(100); }
  throw Error('Native scenario did not settle');
}
(async () => {
  for (const key of ['READMD_BIN', 'READMD_DEPTH_DATA', 'READMD_DEPTH_REPORT']) assert.ok(process.env[key], `${key} required`);
  const data = path.resolve(process.env.READMD_DEPTH_DATA), docs = path.join(data, 'documents');
  fs.mkdirSync(docs, { recursive: true });
  const source = path.join(docs, '论证笔记.md');
  const original = '# 证据与推理\n\n> 哲学阅读札记 · 观察、解释与检验\n\n好的论证，从可靠的证据开始。\n\n## 三个步骤\n\n| 观察 | 解释 | 检验 |\n| --- | --- | --- |\n| 保留原始材料 | 比较不同观点 | 主动寻找反例 |\n\n[[研究方法]]\n';
  fs.writeFileSync(source, original); fs.writeFileSync(path.join(docs, '研究方法.md'), '# 研究方法\n\n[[论证笔记]]\n');
  const copies = ['README.md', 'docs/reviews/readmd-ui-function-inventory-2026-10-02.md'].map((relative, i) => {
    const source = path.join(root, relative), copy = path.join(docs, `真实文档-${i + 1}.md`);
    fs.copyFileSync(source, copy); return { source, copy, before: hash(source) };
  });
  let native;
  const scenario = async (name, action) => {
    const start = Date.now();
    try { const details = await action(); report.scenarios.push({ name, passed: true, elapsedMs: Date.now() - start, ...details }); }
    catch (error) { report.scenarios.push({ name, passed: false, elapsedMs: Date.now() - start, error: error.message.slice(0, 300) }); }
    fs.writeFileSync(process.env.READMD_DEPTH_REPORT, JSON.stringify(report, null, 2));
    console.log(JSON.stringify(report.scenarios.at(-1)));
  };
  try {
    native = await startNative(root, data, chromium, Number(process.env.READMD_DEPTH_PORT || 28831));
    const page = native.page;
    page.on('pageerror', e => report.pageErrors.push(e.message.slice(0, 150)));
    await page.evaluate(async source => { await i18n.setLanguage('zh-CN', false); await loadFile(source); }, source);
    await scenario('real-native-picker-failure-feedback-six-entry-points', async () => {
      await page.route('**/api/dialog/**', r => r.fulfill({ json: { ok: true, canceled: true, error_code: 'dialog_unavailable' } }));
      try {
        const messages = await page.evaluate(async () => {
          const messages = [], previous = window.showToast; window.showToast = text => messages.push(text);
          try {
            await py.choose_file(); await py.choose_folder(''); await py.choose_any_file();
            await py.choose_many_files(); await py.choose_skill_source('folder'); await py.choose_pet_plugin();
          } finally { window.showToast = previous; }
          return messages;
        });
        assert.equal(messages.length, 6); assert.ok(messages.every(m => m && !m.includes('error.dialog_unavailable')));
        return { entryPoints: 6, injectedDialogFailure: true, osChooserOpened: false };
      } finally { await page.unroute('**/api/dialog/**'); }
    });
    await scenario('native-save-undo-copy-and-recovery-disk-roundtrip', async () => {
      await page.evaluate(async () => { await toggleEdit(); cmView.dispatch({ changes: { from: cmView.state.doc.length, insert: '\n已保存的研究记录。\n' } }); });
      assert.equal(await page.evaluate(() => saveEdit()), true); assert.ok(fs.readFileSync(source, 'utf8').includes('已保存'));
      await page.evaluate(() => cmUndo()); assert.equal(await page.evaluate(() => getEditContent()), original);
      assert.equal(await page.evaluate(() => saveEdit()), true); assert.equal(fs.readFileSync(source, 'utf8'), original);
      await page.evaluate(() => ReadMDRecovery.createCopy()); assert.equal(await page.evaluate(() => state.file), null);
      await page.evaluate(() => ReadMDRecovery.flush());
      const entries = await page.evaluate(async () => (await (await apiFetch('/api/documents/history')).json()).entries);
      assert.ok(entries.some(e => e.kind === 'version')); assert.ok(!fs.existsSync(source + '.bak'));
      await page.evaluate(() => exitEdit()); return { originalRestored: true, undoRetained: true, centralVersions: true };
    });
    await scenario('native-real-files-open-and-compact-three-language-editor', async () => {
      for (const copy of copies) {
        await page.evaluate(p => loadFile(p), copy.copy);
        assert.ok(await page.evaluate(() => state.original.length > 1000));
      }
      await page.evaluate(async source => { await loadFile(source); await toggleEdit(); }, source);
      for (const width of [1024, 1160]) {
        await native.page.context().newCDPSession(page).then(async cdp => { await cdp.send('Emulation.setDeviceMetricsOverride', { width, height: 680, deviceScaleFactor: 1, mobile: false }); await cdp.detach(); });
        for (const lang of ['zh-CN', 'zh-TW', 'en']) {
          await page.evaluate(lang => i18n.setLanguage(lang, false), lang);
          const bounds = await page.locator('#edit-bar').boundingBox(); assert.ok(bounds.height <= 56.5);
          assert.equal(await page.evaluate(() => getEditContent()), original);
        }
      }
      await page.evaluate(async () => { await i18n.setLanguage('zh-CN', false); exitEdit(); });
      return { realDocumentCopies: copies.length, widths: [1024, 1160], languages: 3 };
    });
    await scenario('native-same-mtime-content-refresh-and-no-edit-interruption', async () => {
      await page.evaluate(p => loadFile(p), source); const stat = fs.statSync(source);
      const external = original + '\n外部新增的证据。\n'; fs.writeFileSync(source, external); fs.utimesSync(source, stat.atime, stat.mtime);
      await page.evaluate(() => { state.autoReload = true; startAutoReload(); });
      await until(() => page.evaluate(() => state.original.includes('外部新增的证据')));
      await page.evaluate(async () => { await toggleEdit(); cmView.dispatch({ changes: { from: cmView.state.doc.length, insert: '\n本地未保存的草稿。\n' } }); });
      fs.writeFileSync(source, original + '\n第二次外部修改。\n');
      await delay(2800);
      assert.ok(await page.evaluate(() => state.editing && getEditContent().includes('本地未保存的草稿')));
      await page.evaluate(() => { stopAutoReload(); exitEdit(); }); fs.writeFileSync(source, original);
      return { sameMtimeDetected: true, dirtyDraftPreserved: true };
    });
    const formats = {
      txt: '哲学阅读记录\n\n证据先于结论。', html: `<article><h1>证据，先于结论。</h1>
      <blockquote>READMD READING GROUP · 哲学阅读工作坊</blockquote>
      <p>读到一个令人信服的观点时，先问它依赖什么证据，再判断解释是否足够。让阅读从收藏走向理解。</p>
      <h2>一份可复核的阅读记录</h2>
      <table><thead><tr><th>阶段</th><th>关键问题</th><th>留下的材料</th></tr></thead><tbody>
      <tr><td>观察</td><td>作者实际说了什么？</td><td>原文、页码与上下文</td></tr>
      <tr><td>解释</td><td>还有别的解释吗？</td><td>不同观点与必要前提</td></tr>
      <tr><td>检验</td><td>什么会推翻结论？</td><td>反例、边界与待查问题</td></tr></tbody></table>
      <h2>把观点变成行动</h2><ul><li>区分事实、解释和价值判断。</li><li>保留原始材料，再进行整理和改写。</li><li>用自己的语言总结，同时留下不确定之处。</li></ul>
      <p><strong>今天的问题：</strong>一项观察能够支持多强的结论？</p>
      <hr><p>原创演示材料 · 从 HTML 转换为 Markdown，结构与语义完整保留。</p></article>`,
      csv: '阶段,行动\n观察,保留证据\n解释,比较观点', json: JSON.stringify({ title: '哲学阅读记录', evidence: 42 }),
      rtf: '{\\rtf1\\ansi Evidence precedes conclusions.}', tex: '\\documentclass{article}\\begin{document}\\section{Evidence}Compare explanations.\\end{document}',
      rs: 'fn main() { println!("Evidence precedes conclusions."); }', js: 'console.log("Compare explanations.");',
    };
    const inputs = Array.from({ length: 32 }, (_, i) => {
      const ext = Object.keys(formats)[i % 8], file = path.join(docs, `阅读材料-${String(i + 1).padStart(2, '0')}.${ext}`); fs.writeFileSync(file, formats[ext]); return file;
    });
    await scenario('native-32-file-eight-format-batch-counts-skip-and-keyboard-open', async () => {
      await page.evaluate(paths => enqueueBatchFiles([...paths, ...paths.slice(0, 4)], false), inputs);
      await until(() => page.evaluate(() => !isBatchRunning()), 60000);
      let counts = await page.evaluate(() => ({ ...batchCount })); assert.equal(counts.ok, 32); assert.equal(counts.error, 0);
      assert.equal(await page.locator('.batch-item').count(), 32);
      for (const input of inputs) { const out = input.replace(/\.[^.]+$/, '.md'); assert.ok(fs.statSync(out).size > 0); }
      const row = page.locator('.batch-item.ok').first(); await row.focus(); await page.keyboard.press('Enter');
      await until(() => page.evaluate(() => /阅读材料-01\.md$/.test(state.file || '')));
      await page.evaluate(paths => enqueueBatchFiles(paths, false), inputs);
      await until(() => page.evaluate(() => !isBatchRunning()), 60000);
      counts = await page.evaluate(() => ({ ...batchCount })); assert.equal(counts.skipped, 32); assert.equal(counts.ok, 0);
      await page.evaluate(() => closeBatchModal()); return { formats: 8, inputs: 32, duplicatesExcluded: 4, existingOutputsSkipped: 32 };
    });
    await scenario('native-offline-plugin-install-toggle-reopen-uninstall', async () => {
      const installed = await page.evaluate(async () => (await (await apiFetch('/api/plugins/install', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ plugin_id: 'jieba' }) })).json())); assert.equal(installed.ok, true);
      await until(() => page.evaluate(async () => (await (await apiFetch('/api/plugins/list')).json()).plugins.jieba.installed));
      await page.evaluate(() => openPluginModal());
      const toggle = page.locator('[data-plugin-id="jieba"] input[data-action="toggle"]');
      const before = await toggle.isChecked(); await page.locator('[data-plugin-id="jieba"] .plugin-switch').click();
      await until(() => toggle.isEnabled()); assert.equal(await toggle.isChecked(), !before);
      await page.evaluate(async () => { closePluginModal(); await openPluginModal(); }); assert.equal(await toggle.isChecked(), !before);
      await page.evaluate(() => closePluginModal());
      const removed = await page.evaluate(async () => (await (await apiFetch('/api/plugins/uninstall', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ plugin_id: 'jieba' }) })).json())); assert.equal(removed.ok, true);
      return { nativeOfflineInstaller: true, persistedToggle: true, removed: true };
    });
    await scenario('native-pdf-preview-equals-export-artifact', async () => {
      const target = path.join(docs, '论证笔记.pdf');
      const result = await page.evaluate(async ({ content, dir, target }) => {
        const payload = { format: 'pdf', content, baseDir: dir, suggestedName: '论证笔记', options: {} };
        const preview = await (await apiFetch('/api/export/preview', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload) })).json();
        const exported = await (await apiFetch('/api/export', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ ...payload, task_id: 'native-depth-export', out_path: target }) })).json();
        return { preview, exported };
      }, { content: original, dir: docs, target });
      assert.equal(result.preview.ok, true); assert.equal(result.exported.ok, true);
      const previewBytes = Buffer.from(result.preview.pdf, 'base64'); assert.ok(previewBytes.length > 1000);
      assert.ok(fs.readFileSync(target).equals(previewBytes)); assert.ok(!fs.existsSync(target + '.readmd-part'));
      return { byteIdentical: true, pdfBytes: previewBytes.length, stagingClean: true };
    });
    await scenario('native-graph-and-authenticated-lan-share', async () => {
      await page.evaluate(async ({ source, docs }) => { await loadFile(source); await ReadMDGraph.open(docs); }, { source, docs });
      assert.equal(await page.locator('#graph-modal').isVisible(), true); await page.evaluate(() => ReadMDGraph.close());
      await page.evaluate(() => openShareModal()); await page.locator('#share-start').click();
      await until(() => page.locator('#share-copy').isVisible());
      await until(async () => (await page.locator('#share-qr img').count()) === 1);
      await page.locator('#share-stop').click(); await until(() => page.locator('#share-start').isEnabled());
      assert.equal(await page.locator('#share-copy').isVisible(), false); await page.locator('#share-close').click();
      return { indexedGraph: true, nativeQr: true, shareStopped: true };
    });
    report.originalsUnchanged = copies.every(copy => hash(copy.source) === copy.before);
    report.passed = report.originalsUnchanged && report.pageErrors.length === 0 && report.scenarios.every(s => s.passed);
    if (report.passed && process.env.READMD_DEPTH_MEDIA) {
      const record = require(path.resolve(process.env.READMD_DEPTH_MEDIA));
      await record({ page, docs, source, inputs, original, formats }); report.mediaRecorded = true;
    }
  } finally {
    if (native) { await native.page.evaluate(() => { stopAutoReload(); return stopShare(); }).catch(() => {}); await native.browser.close().catch(() => {}); if (native.server.child.exitCode === null) native.server.child.kill(); }
    fs.writeFileSync(process.env.READMD_DEPTH_REPORT, JSON.stringify(report, null, 2));
  }
  assert.equal(report.passed, true, 'Native experience acceptance failed');
})().catch(error => { console.error(error.message); process.exitCode = 1; });
