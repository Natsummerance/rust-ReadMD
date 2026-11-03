'use strict';
// Production UI/CSP + actual HTTP/files, in an owned headless browser profile.
// This does not certify Cocoa, a native chooser, managed runtimes or models.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), assert = require('node:assert/strict');
const { options } = require('./v006-table-clipboard-kernel.cjs');
const { startKernel } = require('./v006-refactor-host.cjs');
const ROOT = path.resolve(__dirname, '../..'), sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
async function bounded(promise, label, ms = 10000) { let timer; try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(Error('Deadline: ' + label)), ms); })]); } finally { clearTimeout(timer); } }
async function until(check, label, ms = 15000) { const end = Date.now() + ms; while (Date.now() < end) { if (await check()) return; await pause(25); } throw Error('Deadline: ' + label); }
async function main() {
  const args = options(process.argv.slice(2)); assert(args.bin && !args.base);
  const profile = path.join(args.evidence, 'profile'), docs = path.join(args.evidence, 'documents');
  for (const dir of [profile, docs, path.join(args.evidence, 'browser-home'), path.join(args.evidence, 'tmp')]) fs.mkdirSync(dir);
  const sql = 'SELECT 42 AS result;', single = '# Code 😀\n\n```sql cmd output=true\n' + sql + '\n```\n\nTail.\n';
  const duplicate = '# Code 😀\n\n```sql cmd output=true\n' + sql + '\n```\n\nSame.\n\n```sql cmd output=true\n' + sql + '\n```\n\nTail.\n';
  const longSql = 'WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000000) SELECT sum(x) FROM n;';
  const fixtures = { '重复代码 中文😀.md': duplicate, 'async-A.md': single, 'async-B.md': single, 'stale.md': single, 'quit-cancel.md': single, 'cancel.md': single.replace(sql, longSql) };
  for (const [name, text] of Object.entries(fixtures)) fs.writeFileSync(path.join(docs, name), '\ufeff' + text.replaceAll('\n', '\r\n'));
  const report = { passed: false, headless: true, nativeUiCertified: false, nativeBridge: 'settings bootstrap only; real authenticated Rust session/save/code routes', productionCsp: true, strictAuth: true, modelInference: false, checks: [], pageErrors: [], kernelPids: [], binarySha256: sha(fs.readFileSync(args.bin)), bootSha256: sha(fs.readFileSync(path.join(ROOT, 'assets/readmd.boot.js'))) };
  const { chromium } = require(args.playwright || require.resolve('playwright', { paths: [path.join(ROOT, 'ui-tests')] }));
  let host, browser, page, phase = 'startup';
  function mark(value) { phase = value; console.log(JSON.stringify({ phase })); }
  const watchdog = setTimeout(() => {
    fs.writeFileSync(path.join(args.evidence, 'deadline.json'), JSON.stringify({ phase, deadlineMs: 90000 }) + '\n', { flag: 'wx' });
    void browser?.close();
  }, 90000);
  async function start() {
    host = await startKernel(args.bin, profile, docs); report.kernelPids.push(host.pid);
    const root = await host.api('/'), base = new URL(root.url).origin;
    const safeEnv = Object.fromEntries(Object.entries(process.env).filter(([key]) => /^(PATH|SYSTEMROOT|WINDIR|COMSPEC|PATHEXT|LANG|LC_ALL)$/i.test(key)));
    Object.assign(safeEnv, { TMPDIR: path.join(args.evidence, 'tmp'), CFFIXED_USER_HOME: path.join(args.evidence, 'browser-home'), CHROME_LOG_FILE: path.join(args.evidence, 'chrome.log') });
    const channel = process.env.READMD_CODE_BROWSER || 'chrome';
    browser = await chromium.launch({ channel: channel === 'chromium' ? undefined : channel, headless: true, env: safeEnv, args: ['--disable-crash-reporter', '--disable-breakpad'] });
    page = await browser.newPage({ viewport: { width: 1160, height: 820 }, bypassCSP: false, serviceWorkers: 'block' });
    page.setDefaultTimeout(10000);
    page.on('pageerror', error => report.pageErrors.push(error.message.replaceAll(args.evidence, '<evidence>').slice(0, 500)));
    await page.route('**/*', route => new URL(route.request().url()).origin === base ? route.continue() : route.abort());
    // The desktop session adapter is selected by the host bridge. Supply only
    // its bootstrap envelope; session CAS, file/save and code work remain real
    // authenticated requests. This is not native host/chooser evidence.
    await page.addInitScript(() => {
      const headers = () => { const token = document.querySelector('meta[name="readmd-app-token"]')?.content || ''; return { 'Content-Type': 'application/json', 'X-ReadMD-App-Token': token, 'X-ReadMD-Token': token }; };
      window.pywebview = { api: {
        get_settings: async () => { const value = await (await fetch('/api/settings', { headers: headers() })).json(); return value?.settings && typeof value.ok === 'boolean' ? value.settings : value; },
        save_settings: async value => (await fetch('/api/settings', { method: 'POST', headers: headers(), body: JSON.stringify(value) })).json(),
        get_system_language: async () => 'zh-CN', get_autostart: async () => false, get_app_info: async () => ({}), start_modules: async () => true,
      } };
    });
    const boot = page.waitForResponse(r => new URL(r.url()).pathname === '/assets/readmd.boot.js'); await page.goto(base);
    assert.equal(sha(await (await boot).body()), report.bootSha256);
    const csp = await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content'); assert(csp.includes("script-src 'self'")); assert(!/script-src[^;]*'unsafe-(inline|eval)'/.test(csp));
    await until(() => page.evaluate(() => Boolean(window.__readmdAppReady && window.ReadMDReview?.codeTarget && window.ReadMDCodeRuns)), 'application ready');
    await page.evaluate(() => window.__readmdSessionReady);
    assert.equal(await page.evaluate(() => hasPy && window.READMD_ENGINE === 'rust'), true);
  }
  async function edit(name) {
    await page.evaluate(async file => { await loadFile(file); if (!state.editing) await toggleEdit(); setPvLayout('right'); await renderPreview(); }, path.join(docs, name));
    await until(() => page.locator('#preview-pane .code-chunk-run-btn').count().then(n => n > 0), 'actual runnable preview');
  }
  const text = () => page.evaluate(() => cmView.state.doc.toString());
  const idle = () => until(() => page.evaluate(() => window.ReadMDCodeRuns.busy() === 0), 'code job terminal');
  function delayedCode() {
    let entered, release; const ready = new Promise(resolve => { entered = resolve; }), gate = new Promise(resolve => { release = resolve; });
    const handler = async route => { const response = await route.fetch(); entered(); await gate; await route.fulfill({ response }); };
    return { ready, release, handler };
  }
  try {
    await start(); mark('real run second duplicate and save'); await edit('重复代码 中文😀.md');
    await page.locator('#preview-pane .code-chunk-run-btn').nth(1).click(); await idle();
    const expected = '# Code 😀\n\n```sql cmd output=true\n' + sql + '\n```\n\nSame.\n\n```sql cmd output=true\n' + sql + '\n```\n\n<!-- code_chunk_output -->\n\nresult\n------\n42\n\n<!-- /code_chunk_output -->\n\nTail.\n';
    assert.equal(await text(), expected); assert.equal(fs.readFileSync(path.join(docs, '重复代码 中文😀.md'), 'utf8'), '\ufeff' + expected.replaceAll('\n', '\r\n'));
    await page.locator('.cm-content').click(); await page.keyboard.press(process.platform === 'darwin' ? 'Meta+z' : 'Control+z');
    await until(async () => await text() === duplicate, 'single actual undo');
    await page.keyboard.press(process.platform === 'darwin' ? 'Meta+Shift+z' : 'Control+Shift+z'); await until(async () => await text() === expected, 'actual redo');
    await page.evaluate(() => saveEdit());
    report.checks.push('actual second duplicate run, BOM/CRLF save and keyboard single undo/redo');
    mark('switch while real HTTP result waits'); await edit('async-A.md'); const delayed = delayedCode();
    await page.route('**/api/code/run', delayed.handler); await page.locator('#preview-pane .code-chunk-run-btn').click(); await delayed.ready;
    await edit('async-B.md'); delayed.release(); await idle(); await page.unroute('**/api/code/run', delayed.handler);
    assert.equal(await text(), single); assert.equal(fs.readFileSync(path.join(docs, 'async-B.md'), 'utf8'), '\ufeff' + single.replaceAll('\n', '\r\n'));
    const target = await page.evaluate(() => { const tab = state.tabs.find(t => t.path.endsWith('async-A.md')); return { content: tab.content, dirty: tab.isDirty }; });
    assert(target.content.includes('<!-- code_chunk_output -->')); assert.equal(target.dirty, true);
    assert.equal(fs.readFileSync(path.join(docs, 'async-A.md'), 'utf8'), '\ufeff' + single.replaceAll('\n', '\r\n'));
    report.checks.push('delayed real output updates only its original inactive durable draft; current document and both original files protected');
    mark('edit then undo rejects old result'); await edit('stale.md'); const stale = delayedCode();
    await page.route('**/api/code/run', stale.handler); await page.locator('#preview-pane .code-chunk-run-btn').click(); await stale.ready;
    await page.evaluate(async () => { cmView.dispatch({ changes: { from: 0, to: 0, insert: 'User edit\n' } }); await ReadMDReview.flush(getActiveTab()); ReadMDReview.history(false); await ReadMDReview.flush(getActiveTab()); });
    assert.equal(await text(), single); stale.release(); await idle(); await page.unroute('**/api/code/run', stale.handler);
    assert.equal(await text(), single); assert(!fs.readFileSync(path.join(docs, 'stale.md'), 'utf8').includes('code_chunk_output'));
    report.checks.push('edit then undo keeps equal text but invalidates the old generation');
    mark('cancel quit while a generated write waits'); await edit('quit-cancel.md'); const quitHeld = delayedCode();
    await page.route('**/api/code/run', quitHeld.handler); await page.locator('#preview-pane .code-chunk-run-btn').click(); await quitHeld.ready;
    await page.evaluate(() => { window.__codeQuitAttempt = ReadMDRecovery.prepareClose(); });
    await page.locator('#close-confirm-modal').waitFor({ state: 'visible' }); quitHeld.release();
    assert.equal(await text(), single, 'quit confirmation must hold generated writes outside the review queue');
    await page.locator('#close-confirm-cancel').click(); assert.equal(await bounded(page.evaluate(() => window.__codeQuitAttempt), 'cancelled quit ready'), false); await idle();
    await page.unroute('**/api/code/run', quitHeld.handler);
    assert(String(await text()).includes('<!-- code_chunk_output -->'));
    assert(fs.readFileSync(path.join(docs, 'quit-cancel.md'), 'utf8').includes('<!-- code_chunk_output -->'));
    report.checks.push('cancelling quit releases the write reservation and the original task completes without blocking close/save');
    mark('real running cancel and close'); await edit('cancel.md');
    const running = page.waitForRequest(r => new URL(r.url()).pathname === '/api/code/run'); await page.locator('#preview-pane .code-chunk-run-btn').click(); await running;
    await page.locator('#preview-pane .code-chunk-run-btn').click(); await idle(); assert(!String(await text()).includes('code_chunk_output'));
    const started = page.waitForRequest(r => new URL(r.url()).pathname === '/api/code/run'); await page.locator('#preview-pane .code-chunk-run-btn').click(); await started;
    const closed = await page.evaluate(async () => { const id = getActiveTab().id; await closeTab(id); return !state.tabs.some(tab => tab.id === id); }); assert.equal(closed, true); await idle();
    assert.equal(fs.readFileSync(path.join(docs, 'cancel.md'), 'utf8'), '\ufeff' + fixtures['cancel.md'].replaceAll('\n', '\r\n'));
    report.checks.push('actual SQL cancel and acknowledged tab close leave no result and preserve the file');
    mark('new process restoration'); await page.evaluate(async () => { await ReadMDReview.flushAll(); await ReadMDRecovery.flush(); await ReadMDSession.flush(); });
    await browser.close(); browser = null; await host.stop(); host = null; await start();
    // Inactive pages are deliberately hydrated when opened. Activate the real
    // restored page before asserting its body, rather than its descriptor.
    const recovered = await page.evaluate(async () => {
      const tab = state.tabs.find(t => t.path.endsWith('async-A.md')); if (!tab) return null;
      await switchTab(tab.id); if (!state.editing) await toggleEdit();
      return { content: cmView.state.doc.toString(), dirty: tab.isDirty };
    });
    assert(recovered?.content.includes('<!-- code_chunk_output -->')); assert.equal(recovered.dirty, true);
    assert.equal(fs.readFileSync(path.join(docs, 'async-B.md'), 'utf8'), '\ufeff' + single.replaceAll('\n', '\r\n'));
    report.checks.push('distinct kernel process restores the inactive result draft and protects the other file');
    mark('save recovered draft before changing the quit fixture');
    assert.equal(await bounded(page.evaluate(() => saveEdit()), 'save recovered draft'), true);
    await bounded(page.evaluate(async () => { const tab = state.tabs.find(t => t.path.endsWith('async-B.md')); await switchTab(tab.id); }), 'switch after explicit save');
    await edit('cancel.md');
    const quitText = fixtures['cancel.md'] + '\nUser edit saved during quit.\n';
    await page.evaluate(async suffix => { cmView.dispatch({ changes: { from: cmView.state.doc.length, insert: suffix } }); await ReadMDReview.flush(getActiveTab()); }, '\nUser edit saved during quit.\n');
    mark('confirmed quit stops code before leaving');
    const quitRunning = page.waitForRequest(r => new URL(r.url()).pathname === '/api/code/run'); await page.locator('#preview-pane .code-chunk-run-btn').click(); await quitRunning;
    await page.evaluate(() => { window.__codeFinalQuit = ReadMDRecovery.prepareClose(); });
    await page.locator('#close-confirm-modal').waitFor({ state: 'visible' }); await page.locator('#close-confirm-save').click();
    assert.equal(await bounded(page.evaluate(() => window.__codeFinalQuit), 'confirmed quit ready'), true); await idle();
    assert.equal(fs.readFileSync(path.join(docs, 'cancel.md'), 'utf8'), '\ufeff' + quitText.replaceAll('\n', '\r\n'));
    assert(fs.readFileSync(path.join(docs, 'async-A.md'), 'utf8').includes('<!-- code_chunk_output -->'));
    report.checks.push('confirmed desktop quit preparation saves the chosen draft and stops the running code before reporting ready');
    assert.deepEqual(report.pageErrors, []); report.passed = true;
  } catch (error) { report.phase = phase; report.failure = { name: error.name, message: String(error.message).replaceAll(args.evidence, '<evidence>').replaceAll(ROOT, '<repo>').slice(0, 1200) }; process.exitCode = 1;
    if (page) report.ui = await page.evaluate(() => ({ modalVisible: Boolean(document.getElementById('close-confirm-modal')?.getClientRects().length && !document.getElementById('close-confirm-modal')?.classList.contains('hidden')),
      modalTitle: document.getElementById('close-confirm-title')?.textContent, jobs: window.ReadMDCodeRuns?.busy(), lifecycleBusy: window.ReadMDLifecycle?.busy(),
      tabs: state.tabs.map(tab => ({ name: tab.name, active: tab.id === state.activeTabId, dirty: tab.isDirty, reviewBusy: window.ReadMDReview?.busy(tab), generation: window.ReadMDReview?.snapshot(tab)?.generation })) })).catch(() => ({ unavailable: true }));
    if (page) await page.screenshot({ path: path.join(args.evidence, 'first-failure.png') }).catch(() => {});
  } finally {
    clearTimeout(watchdog); if (browser) await browser.close(); if (host) await host.stop();
    for (const dir of ['tmp', 'browser-home']) fs.rmSync(path.join(args.evidence, dir), { recursive: true, force: true });
    fs.writeFileSync(path.join(args.evidence, 'report.json'), JSON.stringify(report, null, 2) + '\n', { flag: 'wx' });
    console.log(JSON.stringify({ passed: report.passed, checks: report.checks.length, phase: report.phase, failure: report.failure }));
  }
}
if (require.main === module) main().catch(() => { console.error('Code output browser acceptance failed; inspect its owned report.'); process.exitCode = 1; });
