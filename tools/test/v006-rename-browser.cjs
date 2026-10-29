'use strict';
// Real production boot/CSP and kernel in a headless browser, with a minimal
// native-bridge shim. Uses only owned synthetic files and never records tokens.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), assert = require('node:assert/strict');
const { ROOT } = require('./v006-table-clipboard-host.cjs');
const { options, startKernel } = require('./v006-table-clipboard-kernel.cjs');
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(check, label, timeout = 30000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) { if (await check()) return; await delay(60); }
  throw Error('Deadline: ' + label);
}
async function main() {
  const args = options(process.argv.slice(2)); assert(args.bin && !args.base, 'Explicit prebuilt binary required');
  const docs = path.join(args.evidence, 'documents'), profile = path.join(args.evidence, 'profile');
  fs.mkdirSync(docs); fs.mkdirSync(profile);
  const source = '# Synthetic rename\n\nOriginal paragraph.\n', oldPath = path.join(docs, 'original.md'), newPath = path.join(docs, 'renamed.md');
  fs.writeFileSync(oldPath, source, { flag: 'wx' });
  const report = { syntheticOnly: true, headless: true, bypassCSP: false, nativeBridge: 'minimal test shim',
    binarySha256: sha(fs.readFileSync(args.bin)), bootSha256: sha(fs.readFileSync(path.join(ROOT, 'assets/readmd.boot.js'))), checks: [], pageErrors: [] };
  let kernel, browser, page, phase = 'startup';
  const { chromium } = require(args.playwright || require.resolve('playwright', { paths: [path.join(ROOT, 'ui-tests')] }));
  async function start() {
    kernel = await startKernel(args.bin, profile, docs); (report.kernelPids ||= []).push(kernel.pid);
    browser = await chromium.launch({ channel: 'msedge', headless: true });
    page = await browser.newPage({ bypassCSP: false, viewport: { width: 1160, height: 820 } });
    page.on('pageerror', error => report.pageErrors.push(error.message.slice(0, 300)));
    await page.addInitScript(() => {
      const headers = () => ({ 'Content-Type': 'application/json', 'X-ReadMD-App-Token': document.querySelector('meta[name="readmd-app-token"]')?.content || '' });
      const post = async (route, body) => (await fetch(route, { method: 'POST', headers: headers(), body: JSON.stringify(body) })).json();
      window.pywebview = { api: {
        get_settings: async () => { const value = await (await fetch('/api/settings')).json(); return value?.settings && typeof value.ok === 'boolean' ? value.settings : value; },
        save_settings: value => post('/api/settings', value),
        rename_file: (file, stem, options) => post('/api/rename', { ...(options || {}), path: file, new_stem: stem }).catch(() => ({ ok: false, state: 'unknown' })),
        rename_status: id => post('/api/rename/status', { operation_id: id }).catch(() => ({ ok: false, state: 'unknown' })),
        rename_abandon: id => post('/api/rename/abandon', { operation_id: id }).catch(() => ({ ok: false, state: 'unknown' })),
        get_system_language: async () => 'en', get_autostart: async () => false,
        get_app_info: async () => ({}), start_modules: async () => true,
      } };
      localStorage.setItem('readmd_language', 'en');
    });
    const boot = page.waitForResponse(response => new URL(response.url()).pathname === '/assets/readmd.boot.js');
    await page.goto(kernel.base); assert.equal(sha(await (await boot).body()), report.bootSha256);
    const csp = await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content');
    assert.match(csp, /script-src/); assert.doesNotMatch(csp, /'unsafe-eval'/);
    await until(() => page.evaluate(() => window.__readmdAppReady && window.ReadMDReview && window.ReadMDSession), 'application ready');
    await page.evaluate(() => window.__readmdSessionReady);
  }
  async function stop() { if (browser) { await browser.close(); browser = null; } if (kernel) { await kernel.stop(); kernel = null; } }
  try {
    await start();
    phase = 'edit-state guard and durable review';
    await page.evaluate(file => loadFile(file), oldPath);
    await page.locator('#btn-edit').click();
    await until(() => page.evaluate(() => state.editing && Boolean(window.cmView)), 'editor mounted');
    await page.locator('.tab-item .tab-title').first().dblclick();
    assert.equal(await page.locator('.tab-title-input').count(), 0); assert.equal(fs.readFileSync(oldPath, 'utf8'), source);
    const reviewId = await page.evaluate(async () => {
      const tab = getActiveTab(), stamp = await ReadMDReview.capture(tab), at = stamp.content.indexOf('Original');
      await ReadMDReview.propose([{ from: at, to: at + 8, insert: 'Improved' }], stamp, tab);
      return ReadMDReview.snapshot(tab).document_id;
    });
    await page.locator('#edit-save').click();
    await until(() => page.evaluate(() => !getActiveTab().isDirty && !ReadMDReview.busy()), 'review edit saved');
    assert.equal(fs.readFileSync(oldPath, 'utf8'), source.replace('Original', 'Improved'));
    await page.locator('#edit-cancel').click();
    await until(() => page.evaluate(() => !state.editing), 'reader restored');
    report.checks.push('Editing blocks double-click rename; a real pending review edit saves before rename');

    phase = 'recovery failure and delayed autosave before rename';
    // Simulate a real recovery-store refusal, then let the ordinary 120 ms
    // session autosave run while that refusal is still in flight. The intent
    // must not be persisted or replayed after restart before rename is sent.
    await page.evaluate(() => {
      const original = window.ReadMDRecovery.flush;
      window.__restoreRecoveryFlush = () => { window.ReadMDRecovery.flush = original; };
      window.__recoveryFailureEntered = false;
      window.ReadMDRecovery.flush = async () => {
        window.__recoveryFailureEntered = true;
        await new Promise(resolve => setTimeout(resolve, 450));
        return false;
      };
    });
    await page.locator('.tab-item .tab-title').first().dblclick();
    const failedInput = page.locator('.tab-title-input'); await failedInput.waitFor({ state: 'visible' });
    await failedInput.fill('failed-target'); await failedInput.press('Enter');
    await until(() => page.evaluate(() => window.__recoveryFailureEntered), 'recovery failure entered');
    await page.evaluate(() => ReadMDSession.persist());
    await until(() => page.evaluate(() => !getActiveTab()?._renameInFlight), 'failed rename settled');
    await page.evaluate(() => window.__restoreRecoveryFlush());
    await delay(250);
    const session = await page.evaluate(async () => (await (await apiFetch('/api/session')).json()).session);
    assert.equal(session.tabs.length, 1);
    assert.equal(session.tabs[0].renamePending, null);
    assert.equal(await page.evaluate(() => getActiveTab()?._renamePending ?? null), null);
    assert.equal(fs.existsSync(path.join(docs, 'failed-target.md')), false);
    assert.equal(fs.readFileSync(oldPath, 'utf8'), source.replace('Original', 'Improved'));
    await stop(); await start();
    assert.equal(await page.evaluate(() => getActiveTab()?.name), 'original.md');
    await until(() => page.evaluate(() => ReadMDReview.snapshot()?.document_id), 'pending review restored after failed rename');
    assert.equal(await page.evaluate(() => ReadMDReview.snapshot().document_id), reviewId);
    assert.equal(await page.evaluate(() => ReadMDReview.snapshot().changes[0].decision), 'pending');
    assert.equal(fs.existsSync(path.join(docs, 'failed-target.md')), false);
    report.checks.push('Injected recovery refusal and concurrent session autosave do not persist or replay an unstarted rename; real process restart retains the pending review');

    phase = 'actual tab rename and post-rename save';
    await page.locator('.tab-item .tab-title').first().dblclick();
    const input = page.locator('.tab-title-input'); await input.waitFor({ state: 'visible' });
    await input.fill('renamed'); await input.press('Enter');
    await until(() => page.evaluate(() => getActiveTab()?.name === 'renamed.md' && !getActiveTab()?._renamePending), 'renamed tab');
    assert.equal(fs.existsSync(oldPath), false); assert.equal(fs.readFileSync(newPath, 'utf8'), source.replace('Original', 'Improved'));
    assert.equal(await page.evaluate(() => ReadMDReview.snapshot().document_id), reviewId);
    assert.equal(await page.evaluate(() => ReadMDReview.snapshot().changes[0].decision), 'pending');
    await page.locator('#btn-edit').click(); await until(() => page.evaluate(() => state.editing && Boolean(window.cmView)), 'editor reopened');
    await page.evaluate(() => cmView.dispatch({ changes: { from: cmView.state.doc.length, insert: '\nSaved after rename.\n' } }));
    await page.evaluate(() => cmView.focus()); await page.keyboard.press('Control+z');
    await until(() => page.evaluate(() => !cmView.state.doc.toString().includes('Saved after rename.')), 'undo after rename');
    await page.keyboard.press('Control+y');
    await until(() => page.evaluate(() => cmView.state.doc.toString().includes('Saved after rename.')), 'redo after rename');
    await page.locator('#edit-save').click(); await until(() => page.evaluate(() => !getActiveTab().isDirty && !ReadMDReview.busy()), 'saved renamed file');
    assert.match(fs.readFileSync(newPath, 'utf8'), /Saved after rename\./); assert.equal(fs.existsSync(oldPath), false);
    await page.locator('#edit-cancel').click(); await until(() => page.evaluate(() => !state.editing), 'reader after save');
    await page.evaluate(() => ReadMDSession.flush());
    report.checks.push('Tab rename, later edit, keyboard undo/redo and save preserve the real file and the same pending review identity');

    phase = 'actual process restart';
    await stop(); await start();
    assert.equal(await page.evaluate(() => getActiveTab()?.name), 'renamed.md');
    await until(() => page.evaluate(() => ReadMDReview.snapshot()?.document_id), 'review restored');
    assert.equal(await page.evaluate(() => ReadMDReview.snapshot().document_id), reviewId);
    assert.equal(await page.evaluate(() => ReadMDReview.snapshot().changes[0].decision), 'pending');
    assert.match(fs.readFileSync(newPath, 'utf8'), /Saved after rename\./);
    assert.deepEqual(report.pageErrors, []); report.checks.push('A fresh kernel and headless browser restore the renamed tab and review');
    report.passed = true;
  } catch (error) { report.passed = false; report.failedStage = phase; report.failure = error.message.slice(0, 500); }
  finally { await stop(); fs.writeFileSync(path.join(args.evidence, 'report.json'), JSON.stringify(report, null, 2)); }
  console.log(JSON.stringify(report)); if (!report.passed) process.exitCode = 1;
}
if (require.main === module) main().catch(error => { console.error(error.message); process.exitCode = 1; });
