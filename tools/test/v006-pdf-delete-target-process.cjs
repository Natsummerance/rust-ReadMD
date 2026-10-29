'use strict';
// Owned synthetic PDFs, real managed PDFium, and production-CSP Edge headless.
// The test never compiles, downloads, or opens a visible user window.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const assert = require('node:assert/strict');
const { ROOT } = require('./v006-table-clipboard-host.cjs');
const { options, startKernel } = require('./v006-table-clipboard-kernel.cjs');
const { resources } = require('./v006-pdf-image-target-process.cjs');
const { fixture } = require('../../ui-tests/pdf-fixture.cjs');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(check, label, timeout = 30000) { const end = Date.now() + timeout; while (Date.now() < end) { if (await check()) return; await delay(50); } throw Error('Deadline: ' + label); }

async function main() {
  const args = options(process.argv.slice(2)); assert(args.bin && !args.base, 'An explicit prebuilt --bin is required');
  const profile = path.join(args.evidence, 'profile'), documents = path.join(args.evidence, 'documents'); fs.mkdirSync(documents);
  const bytes = fixture(), files = Object.fromEntries(['A', 'B'].map(name => { const file = path.join(documents, name + '.pdf'); fs.writeFileSync(file, bytes, { flag: 'wx' }); return [name, file]; }));
  const viewport = process.env.READMD_PDF_TEST_COMPACT === '1' ? { width: 1024, height: 680 } : { width: 1160, height: 820 };
  const report = { schemaVersion: 1, syntheticOnly: true, headless: true, bypassCSP: false, compiled: false, downloaded: false, viewport,
    testSha256: sha(fs.readFileSync(__filename)),
    sourceSha256: sha(fs.readFileSync(path.join(ROOT, 'assets/js/features/pdf.js'))), bootSha256: sha(fs.readFileSync(path.join(ROOT, 'assets/readmd.boot.js'))),
    binarySha256: sha(fs.readFileSync(args.bin)), checks: [], pageErrors: [] };
  let browser, page, kernel, phase = 'managed resources';
  try {
    report.resources = resources(profile);
    delete process.env.READMD_PDF_WORKER; delete process.env.READMD_PDFIUM_LIB;
    const originalPath = process.env.PATH;
    try { process.env.PATH = path.join(process.env.SystemRoot, 'System32'); kernel = await startKernel(args.bin, profile, documents); }
    finally { process.env.PATH = originalPath; }
    const { chromium } = require(require.resolve('playwright', { paths: [path.join(ROOT, 'ui-tests')] }));
    browser = await chromium.launch({ channel: 'msedge', headless: true }); report.browserVersion = browser.version();
    const context = await browser.newContext({ bypassCSP: false, viewport }); page = await context.newPage();
    page.on('pageerror', error => report.pageErrors.push(error.message.slice(0, 600)));
    const requests = []; page.on('request', req => { if (new URL(req.url()).pathname === '/api/pdf/workspace' && req.method() === 'POST') requests.push(req.postDataJSON()); });
    await page.addInitScript(() => { localStorage.setItem('readmd_language', 'en'); window.__pdfCspViolations = []; window.addEventListener('securitypolicyviolation', event => window.__pdfCspViolations.push(event.effectiveDirective)); });
    const boot = page.waitForResponse(response => new URL(response.url()).pathname === '/assets/readmd.boot.js');
    await page.goto(kernel.base); report.servedBootSha256 = sha(await (await boot).body()); assert.equal(report.servedBootSha256, report.bootSha256);
    report.csp = await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content'); assert.match(report.csp, /script-src/); assert.doesNotMatch(report.csp, /'unsafe-eval'/);
    await until(() => page.evaluate(() => window.__readmdAppReady && window.ReadMDPdf), 'production app ready');
    const open = async file => {
      const response = page.waitForResponse(res => new URL(res.url()).pathname === '/api/pdf/workspace' && res.request().postDataJSON()?.action === 'open');
      assert.equal(await page.evaluate(p => ReadMDPdf.open(p), file), true); const result = await response; assert.equal(result.status(), 200); return result.json();
    };
    const beginDelete = async () => {
      if (!await page.locator('.pdf-more').evaluate(element => element.open)) await page.locator('.pdf-more > summary').click();
      await page.locator('#pdf-delete-page').click();
      await page.locator('#confirm-modal:not(.hidden)').waitFor();
    };
    const accept = async () => { await page.locator('#confirm-action').click(); await delay(500); };

    phase = 'deletion while a close request is in flight';
    await open(files.A);
    if (!await page.locator('.pdf-more').evaluate(element => element.open)) await page.locator('.pdf-more > summary').click();
    let releaseClose, markClose; const closeHeld = new Promise(resolve => { markClose = resolve; }), closeReleased = new Promise(resolve => { releaseClose = resolve; });
    const hold = async route => { if (route.request().postDataJSON()?.action === 'close') { markClose(); await closeReleased; } await route.continue().catch(() => {}); };
    await page.route('**/api/pdf/workspace', hold);
    let pendingOpen;
    try {
      pendingOpen = page.evaluate(file => ReadMDPdf.open(file), files.B); await closeHeld;
      const beforeClose = requests.filter(req => req.action === 'edit').length;
      await page.locator('#pdf-delete-page').click(); await delay(200);
      const confirmationVisible = await page.locator('#confirm-modal').isVisible();
      if (confirmationVisible) { await page.locator('#confirm-action').click(); await delay(400); }
      report.closing = { confirmationVisible, edits: requests.filter(req => req.action === 'edit').slice(beforeClose).map(req => ({ session: req.session, kind: req.operation?.kind, page: req.operation?.page })) };
    } finally { releaseClose(); await pendingOpen; await page.unroute('**/api/pdf/workspace', hold); }
    assert.equal(report.closing.confirmationVisible, false, 'A PDF closing over the network cannot open a new delete confirmation');
    assert.equal(report.closing.edits.length, 0, 'A PDF closing over the network cannot start a delete operation');
    report.checks.push('The actual delete button cannot begin a new edit while the PDF close request is in flight');

    phase = 'pending confirmation after opening another PDF';
    const a = await open(files.A); await beginDelete(); const b = await open(files.B);
    report.switch = { previousSession: a.session, currentSession: b.session, confirmationStillVisible: await page.locator('#confirm-modal').isVisible() };
    assert.notEqual(a.session, b.session);
    const before = requests.filter(req => req.action === 'edit').length;
    // Exercise the stale callback explicitly even when the newer PDF layer
    // intercepts pointer clicks. This is a controlled DOM dispatch, not a
    // claim that a user can click through that layer.
    await page.locator('#confirm-action').evaluate(element => element.click()); await delay(500);
    const edits = requests.filter(req => req.action === 'edit').slice(before);
    report.switch.edits = edits.map(req => ({ session: req.session, kind: req.operation?.kind, page: req.operation?.page }));
    report.switch.currentDirty = await page.evaluate(() => ReadMDPdf.dirty);
    assert.equal(edits.length, 0, 'A stale confirmation callback for PDF A must not delete a page from PDF B'); assert.equal(report.switch.currentDirty, false);
    assert.equal(report.switch.confirmationStillVisible, false, 'Closing PDF A must dismiss its pending confirmation');
    assert.deepEqual(fs.readFileSync(files.A), bytes); assert.deepEqual(fs.readFileSync(files.B), bytes);
    report.checks.push('Confirmation tied to an old document cannot mutate the newly opened document');

    phase = 'page navigation cancels an old confirmation';
    await beginDelete();
    // A controlled event exercises navigation while the dialog is pending;
    // the ordinary modal intentionally blocks physical pointer navigation.
    await page.locator('#pdf-next').evaluate(element => element.click());
    await until(async () => await page.locator('#pdf-page').inputValue() === '2' && await page.locator('#pdf-modal').getAttribute('aria-busy') === 'false', 'navigation completes');
    assert.equal(await page.locator('#confirm-modal').isVisible(), false);
    const beforeNavigation = requests.filter(req => req.action === 'edit').length;
    await page.locator('#confirm-action').evaluate(element => element.click()); await delay(300);
    assert.equal(requests.filter(req => req.action === 'edit').length, beforeNavigation);
    assert.equal(await page.evaluate(() => ReadMDPdf.dirty), false);
    report.checks.push('Starting page navigation cancels the old confirmation and its callback');

    phase = 'normal confirmation and undo';
    await beginDelete(); const response = page.waitForResponse(res => new URL(res.url()).pathname === '/api/pdf/workspace' && res.request().postDataJSON()?.operation?.kind === 'delete_page');
    await accept(); assert.equal((await (await response).json()).ok, true); await until(() => page.evaluate(() => ReadMDPdf.dirty), 'delete marks PDF dirty');
    assert.equal((await page.locator('#pdf-pages').textContent()).trim(), '/ 1'); await page.locator('#pdf-undo').click();
    await until(async () => (await page.locator('#pdf-pages').textContent()).trim() === '/ 2', 'undo restores page');
    assert.deepEqual(fs.readFileSync(files.B), bytes); report.checks.push('Valid current confirmation deletes one page and undo restores it without writing the file');

    phase = 'normal confirmation, cancel close, save on close, and fresh-process reopen';
    await beginDelete(); const secondDelete = page.waitForResponse(res => new URL(res.url()).pathname === '/api/pdf/workspace' && res.request().postDataJSON()?.operation?.kind === 'delete_page');
    await accept(); assert.equal((await (await secondDelete).json()).ok, true); await until(() => page.evaluate(() => ReadMDPdf.dirty), 'second delete marks PDF dirty');
    await page.locator('#pdf-close').click(); await page.locator('#close-confirm-modal:not(.hidden)').waitFor(); await page.locator('#close-confirm-cancel').click();
    await until(() => page.evaluate(() => ReadMDPdf.dirty), 'cancelled close preserves the draft'); assert.equal(await page.locator('#pdf-modal').isVisible(), true);
    let releaseSave, markSave, savePayload; const saveHeld = new Promise(resolve => { markSave = resolve; }), saveReleased = new Promise(resolve => { releaseSave = resolve; });
    const holdSave = async route => { const request = route.request().postDataJSON(); if (request?.action === 'save') { savePayload = request; markSave(); await saveReleased; } await route.continue().catch(() => {}); };
    await page.route('**/api/pdf/workspace', holdSave);
    const saved = page.waitForResponse(res => new URL(res.url()).pathname === '/api/pdf/workspace' && res.request().postDataJSON()?.action === 'save');
    try {
      await page.locator('#pdf-close').click(); await page.locator('#close-confirm-modal:not(.hidden)').waitFor(); await page.locator('#close-confirm-save').click();
      await saveHeld; await until(() => page.locator('#close-confirm-modal').isHidden(), 'save confirmation dismissed');
      const cancel = page.waitForRequest(req => new URL(req.url()).pathname === '/api/task/cancel' && req.method() === 'POST');
      await page.locator('#pdf-close').click(); const cancelRequest = await cancel;
      assert.equal(cancelRequest.postDataJSON().id, savePayload.task_id);
      report.checks.push('During save-on-close the real close button still sends cancellation for the active task');
    } finally { releaseSave(); await page.unroute('**/api/pdf/workspace', holdSave); }
    assert.equal((await (await saved).json()).ok, true); await until(() => page.locator('#pdf-modal').isHidden(), 'save and close completes');
    const savedBytes = fs.readFileSync(files.B); assert(!savedBytes.equals(bytes)); assert.deepEqual(fs.readFileSync(files.A), bytes);
    const reopened = await open(files.B); assert.equal(reopened.pages, 1); assert.deepEqual(fs.readFileSync(files.B), savedBytes);
    report.checks.push('Cancelling close preserves the draft; save-on-close commits the intended PDF and it reopens with one page');
    assert.deepEqual(report.pageErrors, []); report.cspViolations = await page.evaluate(() => window.__pdfCspViolations); assert.deepEqual(report.cspViolations, []); report.passed = true;
    await browser.close(); browser = null; await kernel.stop(); kernel = null;
    const fresh = path.join(args.evidence, 'profile-fresh'); resources(fresh);
    const freshPath = process.env.PATH;
    try { process.env.PATH = path.join(process.env.SystemRoot, 'System32'); kernel = await startKernel(args.bin, fresh, documents); }
    finally { process.env.PATH = freshPath; }
    const responseFresh = await kernel.api('/api/pdf/workspace', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ action: 'open', path: files.B, width: 1600 }) });
    const freshDoc = await responseFresh.json(); assert.equal(responseFresh.status, 200); assert.equal(freshDoc.pages, 1); assert.deepEqual(fs.readFileSync(files.B), savedBytes); assert.deepEqual(fs.readFileSync(files.A), bytes);
    report.checks.push('A fresh native kernel process reopens the saved PDF without changing the other PDF');
  } catch (error) { report.passed = false; report.failedStage = phase; report.failure = { name: error.name, message: error.message.slice(0, 1000) }; process.exitCode = 1; }
  finally { if (browser) await browser.close(); if (kernel) await kernel.stop(); fs.writeFileSync(path.join(args.evidence, 'report.json'), JSON.stringify(report, null, 2)); }
  console.log(JSON.stringify({ passed: report.passed, checks: report.checks.length, failedStage: report.failedStage, switch: report.switch, failure: report.failure }));
}
if (require.main === module) main().catch(error => { console.error(error.message); process.exitCode = 1; });
