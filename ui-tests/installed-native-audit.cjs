'use strict';
// Opt-in: test an installed Windows payload, real copied documents and the
// actual WebView/native bridge. The corpus manifest is private local input;
// reports contain aliases and measurements, never source text or credentials.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const assert = require('node:assert/strict');
const { chromium } = require('playwright');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const required = key => { if (!process.env[key]) throw Error(`${key} is required`); return process.env[key]; };
const digest = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
async function until(fn, timeout = 30000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) { const value = await fn(); if (value) return value; await delay(100); }
  throw Error('Scenario did not complete within its time limit');
}
(async () => {
  const binary = required('READMD_BIN'), assets = required('READMD_ASSETS_DIR');
  const reportFile = required('READMD_PRODUCTION_REPORT');
  const corpus = JSON.parse(fs.readFileSync(required('READMD_PRODUCTION_CORPUS'), 'utf8'));
  const data = required('READMD_PRODUCTION_DATA');
  const port = Number(process.env.READMD_PRODUCTION_PORT || 28780);
  fs.mkdirSync(data, { recursive: true });
  // Only opaque handles are copied. The OS/encrypted vault stays in place.
  if (process.env.READMD_PRODUCTION_AI_CONFIG) {
    const config = JSON.parse(fs.readFileSync(process.env.READMD_PRODUCTION_AI_CONFIG, 'utf8'));
    if (process.env.READMD_PRODUCTION_AI_BASE_URL) {
      const provider = config.providers.find(p => p.id === config.current.provider_id);
      provider.base_url = process.env.READMD_PRODUCTION_AI_BASE_URL;
    }
    fs.writeFileSync(path.join(data, 'ai.json'), JSON.stringify(config));
  }
  const results = [], pageErrors = [], failedAssets = [];
  let child, browser, page;
  const saveReport = () => fs.writeFileSync(reportFile, JSON.stringify({
    binarySha256: digest(binary), scenarios: results, pageErrors, failedAssets,
    originalsUnchanged: corpus.every(entry => digest(entry.source) === entry.sha256),
  }, null, 2));
  const api = (route, body, timeout = 120000) => page.evaluate(async ({ route, body, timeout }) => {
    const response = await apiFetch(route, { ...(body === undefined ? {} : { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }), signal: AbortSignal.timeout(timeout) });
    return { status: response.status, data: await response.json() };
  }, { route, body, timeout });
  async function scenario(name, action) {
    const start = Date.now();
    try { results.push({ name, passed: true, elapsedMs: Date.now() - start, ...(await action()), elapsedMs: Date.now() - start }); }
    catch (error) { results.push({ name, passed: false, elapsedMs: Date.now() - start, error: error.message.slice(0, 250) }); }
    saveReport();
    console.log(JSON.stringify(results.at(-1)));
  }
  async function launch() {
    const previousWorkspace = process.env.READMD_WORKSPACE;
    process.env.READMD_WORKSPACE = path.dirname(corpus[0].copy);
    let session;
    try { session = await require('./native-session.cjs').startNative(path.resolve(__dirname, '..'), data, chromium, port); }
    finally {
      if (previousWorkspace === undefined) delete process.env.READMD_WORKSPACE;
      else process.env.READMD_WORKSPACE = previousWorkspace;
    }
    child = session.server.child; browser = session.browser; page = session.page;
    page.on('pageerror', error => pageErrors.push(error.message.slice(0, 180)));
    page.on('response', response => { if (response.status() >= 400 && new URL(response.url()).pathname.startsWith('/assets/')) failedAssets.push({ path: new URL(response.url()).pathname, status: response.status() }); });
    await until(() => page.evaluate(() => Boolean(window.__readmdAppReady && window.pywebview?.api?.get_app_info)), 20000);
  }
  try {
    await launch();
    await scenario('native-installed-bootstrap-and-bridge', async () => {
      const info = await page.evaluate(() => py.get_app_info());
      assert.ok(info);
      assert.equal(await page.evaluate(() => typeof py.request_quit), 'function');
      assert.equal(failedAssets.length, 0);
      return { ready: true };
    });
    await scenario('real-large-markdown-native-reading', async () => {
      const entry = corpus.find(e => e.suffix === '.md');
      await page.evaluate(file => loadFile(file), entry.copy);
      await until(() => page.evaluate(() => Boolean(state.original && document.querySelector('#content .markdown-body'))));
      assert.ok(await page.evaluate(() => state.original.length > 100000));
      await page.locator('#btn-theme').click();
      await page.locator('#btn-zen').click(); await page.keyboard.press('Escape');
      return { inputBytes: entry.bytes, renderedDomNodes: await page.evaluate(() => document.querySelectorAll('*').length) };
    });
    await scenario('real-document-native-save-undo-copy-and-recovery', async () => {
      const entry = corpus.filter(e => e.suffix === '.md').at(-1);
      await page.evaluate(file => loadFile(file), entry.copy);
      const original = fs.readFileSync(entry.copy, 'utf8');
      await page.locator('#btn-edit').click();
      await until(() => page.evaluate(() => Boolean(window.cmView)));
      assert.equal(await page.evaluate(() => hasUnsavedEditorChanges()), false);
      await page.evaluate(() => cmView.dispatch({ changes: { from: cmView.state.doc.length, insert: '\nREADMD_PRODUCTION_SAVE\n' } }));
      await page.locator('#edit-save').click();
      await until(() => fs.readFileSync(entry.copy, 'utf8').includes('READMD_PRODUCTION_SAVE'));
      await page.evaluate(() => cmUndo());
      assert.equal(await page.evaluate(() => hasUnsavedEditorChanges()), true);
      assert.equal(await page.evaluate(() => saveEdit()), true);
      assert.equal(fs.readFileSync(entry.copy, 'utf8'), original);
      const history = (await api('/api/documents/history')).data.entries;
      assert.ok(history.some(e => e.kind === 'version'));
      await page.evaluate(() => ReadMDRecovery.createCopy());
      assert.equal(await page.evaluate(() => getActiveTab().path || ''), '');
      await page.evaluate(() => ReadMDRecovery.flush());
      assert.ok((await api('/api/documents/history')).data.entries.some(e => e.kind === 'draft'));
      return { savedAndReadBack: true, undoPreserved: true, sourceRestored: true };
    });
    await scenario('saved-antigravity-discovery-and-real-small-chat', async () => {
      const discovery = await api('/api/ai/models', {});
      assert.equal(discovery.status, 200); assert.equal(discovery.data.source, 'provider');
      const models = discovery.data.models; assert.ok(models.length);
      const model = process.env.READMD_PRODUCTION_AI_MODEL || models.find(id => /gemini-3\.8.*flash/i.test(id)) || models.find(id => /gemini.*flash/i.test(id)) || models[0];
      assert.ok(models.includes(model), 'The chosen model must be advertised by the actual provider');
      const reply = await api('/api/ai/chat', { model, stream: false, max_tokens: 32, messages: [{ role: 'user', content: 'Reply with READMD_PRODUCTION_OK only.' }], session: 'production-synthetic' }, 60000);
      assert.equal(reply.status, 200); assert.equal(reply.data.ok, true);
      assert.ok(reply.data.content?.includes('READMD_PRODUCTION_OK'), 'A retirement notice or empty reply is not a successful model response');
      return { modelCount: models.length, actualReplyReceived: true, privateDocumentSent: false };
    });
    for (const entry of corpus.filter(e => ['.epub', '.docx', '.pptx', '.xlsx'].includes(e.suffix))) await scenario(`convert-${entry.id}-${entry.id.startsWith('generated') ? 'fixture' : 'real'}-${entry.suffix.slice(1)}`, async () => {
      const result = await api('/api/convert?p=' + encodeURIComponent(entry.copy) + '&on_exists=overwrite');
      if (entry.expectedRefusal) {
        assert.ok(result.status >= 400); assert.equal(result.data.ok, false);
        return { inputBytes: entry.bytes, invalidContainerRefused: true };
      }
      assert.equal(result.status, 200); assert.equal(result.data.saved, true); assert.ok(result.data.content.length > (entry.suffix === '.epub' ? 1000 : 0));
      assert.equal(fs.readFileSync(result.data.out, 'utf8'), result.data.content);
      return { inputBytes: entry.bytes, outputCharacters: result.data.content.length };
    });
    await scenario('real-large-converted-epub-native-paginated-reading', async () => {
      const entry = corpus.find(e => e.suffix === '.epub');
      const file = entry.copy.replace(/\.epub$/i, '.md');
      const start = Date.now(); await page.evaluate(file => loadFile(file), file);
      await until(() => page.evaluate(() => state.original.length > 1000000 && Boolean(document.querySelector('#content .markdown-body'))), 60000);
      const nodes = await page.evaluate(() => document.querySelectorAll('*').length);
      assert.ok(nodes < 20000, 'Large reading must not expand the entire book into the live DOM');
      return { markdownBytes: fs.statSync(file).size, openMs: Date.now() - start, renderedDomNodes: nodes };
    });
    await scenario('real-image-offline-ocr-and-empty-result-contract', async () => {
      const entry = corpus.find(e => e.suffix === '.png');
      const result = await api('/api/ocr?p=' + encodeURIComponent(entry.copy));
      assert.equal(result.status, 200); assert.equal(typeof result.data.content, 'string');
      if (result.data.empty) assert.equal(result.data.note_code, 'ocr_no_text');
      else assert.ok(result.data.content.trim());
      return { inputBytes: entry.bytes, outputCharacters: result.data.content.length, noTextRecognized: Boolean(result.data.empty) };
    });
    for (const extension of ['.wav', '.mp3']) await scenario(`real-${extension.slice(1)}-offline-transcription`, async () => {
      const entry = corpus.find(e => e.suffix === extension);
      let file = entry.copy;
      if (extension === '.mp3') {
        file = path.join(data, 'bounded-real-audio.mp3');
        const trim = spawnSync('ffmpeg', ['-nostdin', '-hide_banner', '-loglevel', 'error', '-y', '-i', entry.copy, '-t', '20', '-c:a', 'libmp3lame', file], { windowsHide: true, encoding: 'utf8' });
        assert.equal(trim.status, 0, 'The existing decoder must create the bounded real-audio sample');
      }
      const result = await api('/api/transcribe', { path: file, language: 'en' }, 120000);
      if (result.data.ok) {
        assert.equal(result.status, 200); assert.ok(result.data.content.trim());
        return { inputBytes: entry.bytes, boundedSeconds: extension === '.mp3' ? 20 : null, recognizedCharacters: result.data.content.length };
      }
      // Speech-free media can legitimately produce no text. Do not promote a
      // unavailable engine or malformed input to a passing transcription.
      const noSpeech = result.status === 422 && /no speech|未识别|no_segments|empty|没有识别/i.test(result.data.error_detail || '');
      assert.ok(noSpeech, `Actual transcription failed: ${result.data.error_code || result.status}`);
      return { inputBytes: entry.bytes, noSpeechRecognized: true };
    });
    await scenario('real-zip-extraction-and-contained-output', async () => {
      const entry = corpus.find(e => e.suffix === '.zip');
      const result = await api('/api/batch/extract-zip', { confirm: true, path: entry.copy });
      assert.equal(result.status, 200); assert.equal(result.data.ok, true);
      assert.ok(result.data.paths.length);
      for (const file of result.data.paths) {
        const destination = typeof file === 'string' ? file : file.path;
        assert.ok(fs.existsSync(destination));
        assert.ok(path.relative(path.join(data, 'temp_zip'), destination).split(path.sep)[0] !== '..');
      }
      return { extractedFiles: result.data.paths.length, contained: true };
    });
    await scenario('all-six-real-export-formats-and-reimport', async () => {
      const content = '# READMD_EXPORT_MARKER\n\nReadable production artifact.\n\n|Item|Value|\n|---|---|\n|Reading|42|\n';
      const outputs = [];
      for (const [format, extension] of [['pdf','pdf'],['docx','docx'],['epub','epub'],['html','html'],['tex','tex'],['presentation','html']]) {
        const file = path.join(data, `artifact-${format}.${extension}`);
        const result = await api('/api/export', { format, content, out_path: file, options: {}, suggestedName: 'Production artifact' });
        assert.equal(result.status, 200, `${format} export returned ${result.data.error_code || result.status}`); assert.equal(result.data.ok, true); assert.ok(fs.statSync(file).size > 40);
        if (format !== 'presentation') {
          const converted = await api('/api/convert?p=' + encodeURIComponent(file) + '&preview=1');
          assert.equal(converted.status, 200); assert.ok(converted.data.content.includes('READMD_EXPORT_MARKER'));
        }
        outputs.push({ format, bytes: fs.statSync(file).size });
      }
      return { outputs };
    });
    await scenario('all-fourteen-plugin-install-disable-reenable-uninstall', async () => {
      const ids = Object.keys((await api('/api/plugins/list')).data.plugins);
      for (const plugin_id of ids) {
        assert.equal((await api('/api/plugins/install', { plugin_id })).data.ok, true);
        await until(async () => (await api('/api/plugins/list')).data.plugins[plugin_id].installed);
        assert.equal((await api('/api/plugins/toggle', { plugin_id, enabled: false })).data.ok, true);
        assert.equal((await api('/api/plugins/list')).data.plugins[plugin_id].enabled, false);
        assert.equal((await api('/api/plugins/toggle', { plugin_id, enabled: true })).data.ok, true);
        assert.equal((await api('/api/plugins/list')).data.plugins[plugin_id].enabled, true);
        assert.equal((await api('/api/plugins/uninstall', { plugin_id })).data.ok, true);
        assert.equal((await api('/api/plugins/list')).data.plugins[plugin_id].installed, false);
      }
      return { pluginCount: ids.length };
    });
    await scenario('real-scanned-pdf-cancellation-and-no-output-commit', async () => {
      const entry = corpus.filter(e => e.suffix === '.pdf').at(-1);
      const output = entry.copy.replace(/\.pdf$/i, '.md');
      assert.equal(fs.existsSync(output), false);
      const job = (await api('/api/convert/batch', { confirm: true, paths: [entry.copy], overwrite: false })).data.job;
      await delay(1500); const start = Date.now();
      assert.equal((await api('/api/convert/cancel', { job })).data.ok, true);
      const finished = await until(async () => { const progress = (await api('/api/convert/progress?job=' + job)).data; return progress.finished && progress; }, 10000);
      assert.equal(finished.items[0].status, 'canceled'); assert.equal(fs.existsSync(output), false);
      return { inputBytes: entry.bytes, cancellationMs: Date.now() - start, outputCommitted: false };
    });
    await scenario('desktop-runtime-install-start-interact-and-shutdown', async () => {
      assert.equal((await api('/api/pets/runtime/install', { confirm: true })).data.ok, true);
      const configured = await api('/api/pets/configure', { enabled: true, in_app: false, renderer: 'hermes-sprite', character: 'bongocat', scale: .22 });
      assert.equal(configured.data.ok, true);
      const statusEnvelope = (await api('/api/pets/status')).data;
      const status = statusEnvelope.status || statusEnvelope;
      assert.equal(status.adapter.rust.running, true); assert.equal(status.adapter.rust.health.state, 'ready');
      assert.equal((await api('/api/pets/interact', { action: 'pet' })).data.ok, true);
      assert.equal((await api('/api/pets/configure', { enabled: false })).data.ok, true);
      const stopped = (await api('/api/pets/status')).data;
      assert.equal((stopped.status || stopped).adapter.rust.running, false);
      return { nativeRuntimeReady: true };
    });
    await scenario('native-default-association-status-and-live-update-metadata', async () => {
      const assoc = await api('/api/system/assoc', { op: 'status' });
      assert.equal(assoc.status, 200); assert.equal(assoc.data.extensions.length, 4);
      const update = await api('/api/update/check', undefined, 55000);
      assert.equal(update.status, 200); assert.equal(update.data.ok, true);
      return { associationStatusRead: true, updateMetadataFetched: true };
    });
  } finally {
    if (page && !page.isClosed()) await api('/api/pets/configure', { enabled: false }).catch(() => {});
    if (browser) await browser.close().catch(() => {});
    if (child && child.exitCode === null) { child.kill(); await until(() => child.exitCode !== null).catch(() => {}); }
    saveReport();
  }
  if (results.some(result => !result.passed) || pageErrors.length || failedAssets.length) process.exitCode = 1;
})().catch(error => { console.error(error.message); process.exitCode = 1; });
