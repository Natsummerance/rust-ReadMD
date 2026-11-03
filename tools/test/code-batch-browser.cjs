'use strict';
// Production boot/CSP, real Rust execution/history/session/files. The bridge
// selects only the desktop session adapter; this is headless, not Cocoa QA.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), assert = require('node:assert/strict');
const { options } = require('./v006-table-clipboard-kernel.cjs'), { startKernel } = require('./v006-refactor-host.cjs');
const ROOT = path.resolve(__dirname, '../..'), pause = ms => new Promise(resolve => setTimeout(resolve, ms));
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const code = '```sql cmd output=true\nSELECT 42;\n```\n', simple = '# Batch 😀\n\n' + code + '\nSame.\n\n' + code;
const generated = code + '\n<!-- code_chunk_output -->\n\n```sql cmd output=true\nSELECT 999;\n```\n\n<!-- /code_chunk_output -->\n';
const SOURCE = '# Batch 😀\n\n' + generated + '\nSame.\n\n' + code;
const each = code + '\n<!-- code_chunk_output -->\n\n42\n--\n42\n\n<!-- /code_chunk_output -->\n';
const EXPECTED = '# Batch 😀\n\n' + each + '\nPending.\n\n' + each;
const SIMPLE_EXPECTED = '# Batch 😀\n\n' + each + '\nSame.\n\n' + each;
const LONG = '```sql cmd output=true\nWITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<1000000000) SELECT sum(x) FROM n;\n```\n\n' + code;
const encoded = text => Buffer.from('\ufeff' + text.replaceAll('\n', '\r\n'));
async function until(check, label) { const end = Date.now() + 15000; while (Date.now() < end) { if (await check()) return; await pause(25); } throw Error('Deadline: ' + label); }
async function main() {
  const args = options(process.argv.slice(2)); assert(args.bin && !args.base);
  const profile = path.join(args.evidence, 'profile'), docs = path.join(args.evidence, 'documents');
  for (const name of ['profile', 'documents', 'tmp', 'browser-home']) fs.mkdirSync(path.join(args.evidence, name));
  const fixtures = { '批量 中文😀.md': SOURCE, 'async-A.md': simple, 'async-B.md': simple, 'stale.md': simple, 'cancel.md': LONG, 'close.md': LONG,
    'partial.md': code + '\n```sql cmd output=true\nINVALID SQL;\n```\n',
    'mixed.md': '```sql cmd output=true\nINVALID SQL;\n```\n\n```js cmd\nconsole.log(999);\n```\n' };
  for (const [name, source] of Object.entries(fixtures)) fs.writeFileSync(path.join(docs, name), encoded(source));
  const report = { passed: false, headless: true, nativeUiCertified: false, bridge: 'settings bootstrap only', productionCsp: true, strictAuth: true,
    modelsRequested: false, binarySha256: sha(fs.readFileSync(args.bin)), bootSha256: sha(fs.readFileSync(path.join(ROOT, 'assets/readmd.boot.js'))),
    checks: [], pageErrors: [], historyRequests: [], kernelPids: [], batchRunRequests: 0, singleRunRequests: 0 };
  const { chromium } = require(args.playwright || require.resolve('playwright', { paths: [path.join(ROOT, 'ui-tests')] }));
  let host, browser, page, phase = 'startup';
  const watchdog = setTimeout(() => { fs.writeFileSync(path.join(args.evidence, 'deadline.json'), JSON.stringify({ phase, deadlineMs: 120000 }) + '\n', { flag: 'wx' }); void browser?.close(); }, 120000);
  const mark = text => { phase = text; console.log(JSON.stringify({ phase })); };
  async function start() {
    host = await startKernel(args.bin, profile, docs); report.kernelPids.push(host.pid);
    const root = await host.api('/'), base = new URL(root.url).origin;
    const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => /^(PATH|SYSTEMROOT|WINDIR|COMSPEC|PATHEXT|LANG|LC_ALL)$/i.test(key)));
    Object.assign(env, { TMPDIR: path.join(args.evidence, 'tmp'), CFFIXED_USER_HOME: path.join(args.evidence, 'browser-home'), CHROME_LOG_FILE: path.join(args.evidence, 'chrome.log') });
    const channel = process.env.READMD_CODE_BROWSER || 'chrome'; browser = await chromium.launch({ channel: channel === 'chromium' ? undefined : channel, headless: true, env, args: ['--disable-crash-reporter', '--disable-breakpad'] });
    page = await browser.newPage({ viewport: { width: 1160, height: 820 }, bypassCSP: false, serviceWorkers: 'block' }); page.setDefaultTimeout(10000);
    page.on('pageerror', e => report.pageErrors.push(e.message.replaceAll(args.evidence, '<evidence>').slice(0, 500)));
    page.on('request', request => {
      const name = new URL(request.url()).pathname;
      if (name === '/api/code/run') report.singleRunRequests++;
      if (name === '/api/review') {
        const body = request.postDataJSON(); if (body?.action === 'code_batch_run') report.batchRunRequests++;
        if (['undo', 'redo'].includes(body?.action)) report.historyRequests.push(body.action);
      }
    });
    await page.route('**/*', route => new URL(route.request().url()).origin === base ? route.continue() : route.abort());
    await page.addInitScript(() => {
      const headers = () => { const token = document.querySelector('meta[name="readmd-app-token"]')?.content || ''; return { 'Content-Type': 'application/json', 'X-ReadMD-App-Token': token, 'X-ReadMD-Token': token }; };
      window.pywebview = { api: {
        get_settings: async () => { const data = await (await fetch('/api/settings', { headers: headers() })).json(); return data?.settings && typeof data.ok === 'boolean' ? data.settings : data; },
        save_settings: async value => (await fetch('/api/settings', { method: 'POST', headers: headers(), body: JSON.stringify(value) })).json(),
        get_system_language: async () => 'zh-CN', get_autostart: async () => false, get_app_info: async () => ({}), start_modules: async () => true,
      } };
    });
    const boot = page.waitForResponse(r => new URL(r.url()).pathname === '/assets/readmd.boot.js'); await page.goto(base); assert.equal(sha(await (await boot).body()), report.bootSha256);
    const csp = await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content'); assert(csp.includes("script-src 'self'")); assert(!/script-src[^;]*'unsafe-(inline|eval)'/.test(csp));
    await until(() => page.evaluate(() => Boolean(window.__readmdAppReady && window.ReadMDReview?.codeBatchTarget && window.ReadMDCodeRuns)), 'application ready'); await page.evaluate(() => window.__readmdSessionReady);
  }
  const edit = async name => { await page.evaluate(async file => { await loadFile(file); if (!state.editing) await toggleEdit(); setPvLayout('right'); await renderPreview(); }, path.join(docs, name)); await until(() => page.locator('#preview-pane .code-chunk-card').count().then(n => n > 0), 'runnable editor'); };
  const text = () => page.evaluate(() => cmView.state.doc.toString());
  async function history(redo) {
    const before = report.historyRequests.length; await page.locator('.cm-content').focus();
    await page.keyboard.press((process.platform === 'darwin' ? 'Meta+' : 'Control+') + (redo ? 'Shift+KeyZ' : 'KeyZ'));
    await until(() => page.evaluate(() => !ReadMDReview.busy(getActiveTab())), 'history ACK'); assert.deepEqual(report.historyRequests.slice(before), [redo ? 'redo' : 'undo']);
  }
  async function stop() { if (browser) { await browser.close(); browser = null; } if (host) { await host.stop(); host = null; } }
  function heldBatch() {
    let ready, release; const entered = new Promise(resolve => { ready = resolve; }), gate = new Promise(resolve => { release = resolve; });
    const handler = async route => { if (route.request().postDataJSON()?.action !== 'code_batch_run') return route.continue(); const response = await route.fetch(); ready(); await gate; await route.fulfill({ response }); };
    return { entered, release, handler };
  }
  try {
    await start(); mark('all blocks once, actual history and save'); await edit('批量 中文😀.md');
    await page.evaluate(async () => { const tab = getActiveTab(), stamp = await ReadMDReview.capture(tab), at = cmView.state.doc.toString().indexOf('Same.'); await ReadMDReview.propose([{ from: at, to: at + 5, insert: 'Pending.' }], stamp, tab); await renderPreview(); });
    const response = page.waitForResponse(r => new URL(r.url()).pathname === '/api/review' && r.request().postDataJSON()?.action === 'code_batch_run');
    await page.evaluate(() => document.getElementById('btn-run-all-chunks').click());
    await until(() => page.evaluate(() => ReadMDCodeRuns.busy() === 0), 'batch terminal');
    assert.equal((await (await response).json()).results.length, 2); assert.equal(await text(), EXPECTED); assert.equal(report.singleRunRequests, 0); assert.equal(report.batchRunRequests, 1);
    assert.deepEqual(fs.readFileSync(path.join(docs, '批量 中文😀.md')), encoded(EXPECTED)); assert.equal(await page.locator('#btn-run-all-chunks em').count(), 1);
    await history(false); assert.equal(await text(), SOURCE.replace('Same.', 'Pending.')); await history(true); assert.equal(await text(), EXPECTED);
    assert.equal(await page.evaluate(() => saveEdit()), true);
    assert.equal(await page.evaluate(() => ReadMDReview.snapshot(getActiveTab()).changes[0].original), 'Same.');
    report.checks.push('actual Run All menu uses one batch, excludes generated and hidden cards, saves both outputs and one physical-key undo/redo');
    mark('late batch remains on original page'); await edit('async-A.md');
    const held = heldBatch(); await page.route('**/api/review', held.handler); const work = page.evaluate(() => runAllCodeChunks()); await held.entered;
    await edit('async-B.md'); held.release(); assert.equal(await work, true); await page.unroute('**/api/review', held.handler);
    assert.equal(await text(), simple); assert.deepEqual(fs.readFileSync(path.join(docs, 'async-A.md')), encoded(simple)); assert.deepEqual(fs.readFileSync(path.join(docs, 'async-B.md')), encoded(simple));
    assert.equal(await page.evaluate(() => state.tabs.find(t => t.name === 'async-A.md').content), SIMPLE_EXPECTED.replaceAll('\n', '\r\n'));
    assert.equal(await page.evaluate(() => ReadMDSession.flush()), true); await stop(); await start();
    await page.evaluate(async name => { const tab = state.tabs.find(t => t.name === name); if (!tab) throw Error('missing original tab'); await switchTab(tab.id); if (!state.editing) await toggleEdit(); }, 'async-A.md');
    assert.equal(await text(), SIMPLE_EXPECTED); assert.equal(await page.evaluate(() => ReadMDCodeRuns.busy()), 0); await history(false); assert.equal(await text(), simple); assert.equal(await page.evaluate(() => saveEdit()), true);
    report.checks.push('new real kernel restores only the original inactive draft, its single undo and no automatically resumed execution');
    mark('hand edit then undo rejects delayed batch'); await edit('stale.md');
    const stale = heldBatch(); await page.route('**/api/review', stale.handler); const staleWork = page.evaluate(() => runAllCodeChunks()); await stale.entered;
    await page.evaluate(async () => { cmView.dispatch({ changes: { from: cmView.state.doc.length, insert: 'Manual.\n' } }); if (!await ReadMDReview.flush(getActiveTab())) throw Error('edit not durable'); });
    await history(false); assert.equal(await text(), simple); stale.release(); assert.equal(await staleWork, false); await page.unroute('**/api/review', stale.handler);
    assert.equal(await text(), simple); assert.deepEqual(fs.readFileSync(path.join(docs, 'stale.md')), encoded(simple));
    report.checks.push('hand edit then undo keeps the same body but invalidates delayed batch generation without partial insertion');
    mark('cancel and close stop whole batch');
    for (const name of ['cancel.md', 'close.md']) {
      await edit(name); const request = page.waitForRequest(r => new URL(r.url()).pathname === '/api/review' && r.postDataJSON()?.action === 'code_batch_run');
      const pending = page.evaluate(() => runAllCodeChunks()); await request; await pause(100);
      if (name === 'cancel.md') assert.equal(await page.evaluate(() => runAllCodeChunks()), false);
      else await page.evaluate(() => closeTab(getActiveTab().id));
      assert.equal(await pending, false); await until(() => page.evaluate(() => ReadMDCodeRuns.busy() === 0), 'cancelled batch terminal');
      assert.deepEqual(fs.readFileSync(path.join(docs, name)), encoded(LONG));
      if (name === 'close.md') assert.equal(await page.evaluate(() => state.tabs.some(t => t.name === 'close.md')), false);
    }
    report.checks.push('second Run All click and acknowledged tab close cancel the original batch with no partial save');
    mark('unsupported and partially executed batches do not write sources');
    for (const name of ['mixed.md', 'partial.md']) {
      await edit(name); assert.equal(await page.evaluate(() => runAllCodeChunks()), false);
      assert.equal(await text(), fixtures[name]); assert.deepEqual(fs.readFileSync(path.join(docs, name)), encoded(fixtures[name]));
      assert.equal(await page.evaluate(() => ReadMDReview.flush(getActiveTab())), true);
    }
    assert.equal(report.singleRunRequests, 0); assert.equal(report.kernelPids.length, 2); assert.equal(report.pageErrors.length, 0);
    report.checks.push('unmanaged mixed runtimes fail before execution and leave source, menu structure and subsequent UI usable'); report.passed = true;
  } catch (error) {
    report.failure = { phase, name: error.name, message: String(error.message).replaceAll(args.evidence, '<evidence>').replaceAll(ROOT, '<repo>').slice(0, 1000) };
    if (page && !page.isClosed()) await page.screenshot({ path: path.join(args.evidence, 'first-failure.png') }).catch(() => {}); process.exitCode = 1;
  } finally {
    clearTimeout(watchdog); await stop(); for (const name of ['tmp', 'browser-home']) fs.rmSync(path.join(args.evidence, name), { recursive: true, force: true });
    fs.writeFileSync(path.join(args.evidence, 'report.json'), JSON.stringify(report, null, 2) + '\n', { flag: 'wx' }); console.log(JSON.stringify({ passed: report.passed, checks: report.checks.length, failure: report.failure }));
  }
}
if (require.main === module) main().catch(() => { console.error('Code batch browser acceptance failed; inspect the owned report.'); process.exitCode = 1; });
