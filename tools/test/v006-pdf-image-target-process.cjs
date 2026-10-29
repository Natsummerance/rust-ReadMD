'use strict';
// Real managed PDFium + production-CSP Edge headless. Never compiles or
// downloads; fresh profiles hardlink exact locked resources in an owned cache.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const assert = require('node:assert/strict');
const { ROOT } = require('./v006-table-clipboard-host.cjs');
const { options, startKernel } = require('./v006-table-clipboard-kernel.cjs');
const { fixture } = require('../../ui-tests/pdf-fixture.cjs');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(check, label, timeout = 30000) { const end = Date.now() + timeout; while (Date.now() < end) { if (await check()) return; await delay(50); } throw Error('Deadline: ' + label); }

function resources(profile) {
  const pack = path.join(ROOT, '.cache/capability-upgrade/locked-packs/readmd-pdfium-7881-windows-x64.readmd-pack');
  const lock = JSON.parse(fs.readFileSync(path.join(ROOT, 'assets/capabilities/pdfium-7881-windows-x64.lock.json'), 'utf8'));
  const owned = path.join(ROOT, '.cache/v006-pdf-image-target/locked-resources');
  const target = path.join(profile, 'capabilities/managed-v2/pdfium/7881');
  let archive, materializedBytes = 0, hardlinkedBytes = 0;
  for (const expected of lock.files) {
    const file = path.resolve(owned, expected.path), linked = path.resolve(target, expected.path);
    assert(file.startsWith(path.resolve(owned) + path.sep)); assert(linked.startsWith(path.resolve(target) + path.sep));
    if (!fs.existsSync(file)) {
      archive ||= JSON.parse(fs.readFileSync(pack, 'utf8'));
      assert.deepEqual(archive.manifest, lock);
      const bytes = Buffer.from(archive.payload[expected.path], 'base64');
      assert.equal(bytes.length, expected.size); assert.equal(sha(bytes), expected.sha256);
      fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, bytes, { flag: 'wx' }); materializedBytes += bytes.length;
    }
    const bytes = fs.readFileSync(file); assert.equal(bytes.length, expected.size); assert.equal(sha(bytes), expected.sha256);
    fs.mkdirSync(path.dirname(linked), { recursive: true }); fs.linkSync(file, linked); hardlinkedBytes += bytes.length;
  }
  return { files: lock.files.length, hardlinkedBytes, materializedBytes, workerSha256: lock.files.find(file => file.path === 'bin/readmd-pdf-worker.exe').sha256,
    librarySha256: lock.files.find(file => file.path === 'bin/pdfium.dll').sha256 };
}

function unchangedOutside(before, after, rect) {
  const a = Buffer.from(before, 'base64'), b = Buffer.from(after, 'base64'); assert.equal(a.length, b.length);
  const width = a.readInt32LE(18), height = -a.readInt32LE(22); assert.equal(width, b.readInt32LE(18));
  let changed = 0;
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    const at = 54 + 4 * (y * width + x);
    if (!a.subarray(at, at + 4).equals(b.subarray(at, at + 4))) {
      changed++; assert(x >= rect.x - 4 && x <= rect.x + rect.width + 4 && y >= rect.y - 4 && y <= rect.y + rect.height + 4, 'Unmodified PDF pixels changed');
    }
  }
  assert(changed > 0); return changed;
}

async function run() {
  const args = options(process.argv.slice(2)); assert(args.bin && !args.base, 'An explicit prebuilt --bin is required');
  const profile = path.join(args.evidence, 'profile'), documents = path.join(args.evidence, 'documents'); fs.mkdirSync(documents);
  const files = Object.fromEntries(['A', 'B'].map(name => { const file = path.join(documents, name + '.pdf'); fs.writeFileSync(file, fixture(), { flag: 'wx' }); return [name, file]; }));
  const report = { schemaVersion: 1, syntheticOnly: true, headless: true, bypassCSP: false, compiled: false, downloaded: false,
    sourceSha256: sha(fs.readFileSync(path.join(ROOT, 'assets/js/features/pdf.js'))), bootSha256: sha(fs.readFileSync(path.join(ROOT, 'assets/readmd.boot.js'))),
    binarySha256: sha(fs.readFileSync(args.bin)), checks: [], pageErrors: [] };
  let kernel, browser, page, phase = 'managed resources';
  try {
    report.resources = resources(profile);
    delete process.env.READMD_PDF_WORKER; delete process.env.READMD_PDFIUM_LIB;
    const originalPath = process.env.PATH;
    try { process.env.PATH = path.join(process.env.SystemRoot, 'System32'); kernel = await startKernel(args.bin, profile, documents); }
    finally { process.env.PATH = originalPath; }
    report.kernelPids = [kernel.pid];
    const { chromium } = require(require.resolve('playwright', { paths: [path.join(ROOT, 'ui-tests')] }));
    browser = await chromium.launch({ channel: 'msedge', headless: true }); report.browserVersion = browser.version();
    const context = await browser.newContext({ bypassCSP: false, viewport: { width: 1440, height: 1000 } }); page = await context.newPage();
    page.on('pageerror', error => report.pageErrors.push(error.message.slice(0, 600)));
    const requests = [];
    page.on('request', request => { if (new URL(request.url()).pathname === '/api/pdf/workspace' && request.method() === 'POST') requests.push(request.postDataJSON()); });
    await page.addInitScript(() => { localStorage.setItem('readmd_language', 'en'); window.__pdfCspViolations = []; window.addEventListener('securitypolicyviolation', event => window.__pdfCspViolations.push(event.effectiveDirective)); });
    const boot = page.waitForResponse(response => new URL(response.url()).pathname === '/assets/readmd.boot.js');
    await page.goto(kernel.base); report.servedBootSha256 = sha(await (await boot).body()); assert.equal(report.servedBootSha256, report.bootSha256);
    report.csp = await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content'); assert.match(report.csp, /script-src/); assert.doesNotMatch(report.csp, /'unsafe-eval'/);
    await until(() => page.evaluate(() => window.__readmdAppReady && window.ReadMDPdf), 'production app ready');
    const pdf = async payload => { const response = await kernel.api('/api/pdf/workspace', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload) }); return { status: response.status, data: await response.json() }; };
    const probe = await pdf({ action: 'probe' }); assert.equal(probe.status, 200); assert.equal(probe.data.version, '7881');
    const fonts = await (await kernel.api('/api/capabilities/packs', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ action: 'status', id: 'fonts' }) })).json();
    report.fontPackInstalled = fonts.installed; assert.equal(fonts.installed, false);
    const png = path.join(documents, 'replacement.png');
    const data = await page.evaluate(() => { const canvas = document.createElement('canvas'); canvas.width = canvas.height = 2; const paint = canvas.getContext('2d'); paint.fillStyle = '#14beaa'; paint.fillRect(0, 0, 2, 2); return canvas.toDataURL('image/png').split(',')[1]; });
    fs.writeFileSync(png, Buffer.from(data, 'base64'), { flag: 'wx' });
    const newerPng = path.join(documents, 'newer-replacement.png'), badPng = path.join(documents, 'invalid.png');
    const newerData = await page.evaluate(() => { const canvas = document.createElement('canvas'); canvas.width = canvas.height = 2; const paint = canvas.getContext('2d'); paint.fillStyle = '#a52148'; paint.fillRect(0, 0, 2, 2); return canvas.toDataURL('image/png').split(',')[1]; });
    fs.writeFileSync(newerPng, Buffer.from(newerData, 'base64'), { flag: 'wx' }); fs.writeFileSync(badPng, 'Synthetic invalid PNG', { flag: 'wx' });
    const original = fs.readFileSync(files.A), open = async file => {
      const response = page.waitForResponse(response => new URL(response.url()).pathname === '/api/pdf/workspace' && response.request().postDataJSON()?.action === 'open');
      assert.equal(await page.evaluate(file => ReadMDPdf.open(file), file), true);
      const result = await response; assert.equal(result.status(), 200); return result.json();
    };
    const selectImage = async () => { const name = await page.evaluate(() => i18n.t('pdf.image')); await page.locator('#pdf-hits').getByRole('button', { name, exact: true }).click(); };
    let gateSerial = 0;
    const beginSelection = async (file = png) => {
      const id = ++gateSerial;
      await page.evaluate(id => {
        window.__pdfNativeBitmap ||= window.createImageBitmap; window.__pdfGates ||= {};
        const gate = window.__pdfGates[id] = { waiting: false, done: false };
        window.createImageBitmap = async (...args) => {
          let error; try { gate.bitmap = await window.__pdfNativeBitmap(...args); } catch (failure) { error = failure; }
          gate.failed = !!error; gate.waiting = true; await new Promise(resolve => { gate.release = resolve; });
          if (error) throw error; return gate.bitmap;
        };
      }, id);
      const choosing = page.waitForEvent('filechooser'); await page.locator('#pdf-replace-image').click(); const chooser = await choosing;
      const input = await chooser.element();
      // FileChooser handles originate in Playwright's isolated utility world.
      // Adopt the node into the page world so the wrapper observes the same
      // native decoder gates as production's onchange callback.
      await page.evaluate(({ element, id }) => {
        const callback = element.onchange;
        element.onchange = async function (...args) { try { return await callback.apply(this, args); } finally { window.__pdfGates[id].done = true; } };
      }, { element: input, id });
      await chooser.setFiles(file);
      await until(() => page.evaluate(id => window.__pdfGates[id].waiting, id), 'real PNG decoder suspended');
      return id;
    };
    const release = async id => {
      await page.evaluate(id => window.__pdfGates[id].release(), id);
      await until(() => page.evaluate(id => window.__pdfGates[id].done, id), 'pending file callback completed');
      const result = await page.evaluate(id => { const gate = window.__pdfGates[id]; return { nativeDecodeFailed: gate.failed, bitmapClosed: gate.bitmap ? gate.bitmap.width === 0 : null }; }, id);
      if (!result.nativeDecodeFailed) assert.equal(result.bitmapClosed, true);
      await page.evaluate(() => { window.createImageBitmap = window.__pdfNativeBitmap; });
      return result;
    };

    phase = 'cross-document image selection';
    const a = await open(files.A); await selectImage(); const crossGate = await beginSelection(); const b = await open(files.B);
    assert.equal(a.objects.find(item => item.kind === 'image').id, b.objects.find(item => item.kind === 'image').id);
    const before = requests.filter(request => request.action === 'edit').length;
    await release(crossGate);
    report.crossDocument = { previousSession: a.session, currentSession: b.session, imageEditsAfterSwitch: requests.slice().filter(request => request.action === 'edit').length - before,
      currentDirty: await page.evaluate(() => ReadMDPdf.dirty), sourceAUnchanged: fs.readFileSync(files.A).equals(original), sourceBUnchanged: fs.readFileSync(files.B).equals(original) };
    assert.equal(report.crossDocument.imageEditsAfterSwitch, 0, 'Old image selection sent an edit to the current PDF');
    assert.equal(report.crossDocument.currentDirty, false);
    const bAfter = await pdf({ action: 'render', session: b.session, revision: b.revision, page: 0, width: 1600 }); assert.equal(bAfter.status, 200); assert.equal(bAfter.data.image, b.image);
    report.checks.push('An old real filechooser/PNG decode cannot edit the next PDF or dirty its draft');

    phase = 'obsolete native decode failure during a new session task';
    await open(files.A); await selectImage(); const failedGate = await beginSelection(badPng), next = await open(files.B);
    let finishRender, renderArrived;
    const renderReleased = new Promise(resolve => { finishRender = resolve; }), renderStarted = new Promise(resolve => { renderArrived = resolve; });
    const holdRender = async route => {
      const payload = route.request().postDataJSON();
      if (payload.action === 'render' && payload.session === next.session) { renderArrived(); await renderReleased; }
      await route.continue().catch(() => {});
    };
    await page.route('**/api/pdf/workspace', holdRender);
    try {
      await page.locator('#pdf-next').click(); await renderStarted;
      const status = await page.locator('#pdf-status').textContent(); assert.equal(await page.locator('#pdf-modal').getAttribute('aria-busy'), 'true');
      const edits = requests.filter(request => request.action === 'edit').length, failure = await release(failedGate);
      assert.equal(failure.nativeDecodeFailed, true); assert.equal(await page.locator('#pdf-status').textContent(), status);
      assert.equal(await page.locator('#pdf-modal').getAttribute('aria-busy'), 'true'); assert.equal(await page.locator('#pdf-stage').evaluate(element => element.hidden), true);
      assert.equal(requests.filter(request => request.action === 'edit').length, edits);
      report.checks.push('An obsolete real PNG decoding failure cannot overwrite the next PDF status or clear its genuinely pending render task');
    } finally { finishRender(); await page.unroute('**/api/pdf/workspace', holdRender); }
    await until(async () => await page.locator('#pdf-page').inputValue() === '2' && await page.locator('#pdf-modal').getAttribute('aria-busy') === 'false', 'new session render completes');

    for (const [label, firstFile] of [['older success', png], ['older native failure', badPng]]) {
      phase = 'same-session consecutive pickers: ' + label;
      await open(files.A); await selectImage(); const first = await beginSelection(firstFile), second = await beginSelection(newerPng);
      const edits = requests.filter(request => request.action === 'edit').length, status = await page.locator('#pdf-status').textContent();
      const obsolete = await release(first); assert.equal(obsolete.nativeDecodeFailed, firstFile === badPng);
      assert.equal(requests.filter(request => request.action === 'edit').length, edits); assert.equal(await page.evaluate(() => ReadMDPdf.dirty), false);
      assert.equal(await page.locator('#pdf-status').textContent(), status); await release(second);
      assert.equal(requests.filter(request => request.action === 'edit').length, edits + 1); assert.equal(await page.evaluate(() => ReadMDPdf.dirty), true);
      await page.locator('#pdf-undo').click(); await until(() => page.evaluate(() => !ReadMDPdf.dirty), 'consecutive selection undone');
      assert.deepEqual(fs.readFileSync(files.A), original); report.checks.push('Same-session ' + label + ' is permanently superseded; only the newer real picker produces one undoable image operation');
    }

    phase = 'deselection and reselection';
    await open(files.A); await selectImage(); const deselected = await beginSelection(), editCount = requests.filter(request => request.action === 'edit').length;
    await page.locator('#pdf-deselect').click(); await selectImage(); await release(deselected);
    assert.equal(requests.filter(request => request.action === 'edit').length, editCount); assert.equal(await page.evaluate(() => ReadMDPdf.dirty), false);
    report.checks.push('Deselect/reselect of the same real object does not revive an obsolete file selection');

    phase = 'normal image edit';
    const normal = await open(files.A), target = normal.objects.find(item => item.kind === 'image');
    const secondBefore = await pdf({ action: 'render', session: normal.session, revision: normal.revision, page: 1, width: 1600 }); assert.equal(secondBefore.status, 200);
    await selectImage(); const normalGate = await beginSelection(); await release(normalGate); assert.equal(await page.evaluate(() => ReadMDPdf.dirty), true);
    const imageEdit = requests.filter(request => request.action === 'edit' && request.operation?.kind === 'image').at(-1); assert.equal(imageEdit.session, normal.session);
    const edited = await pdf({ action: 'render', session: normal.session, revision: normal.revision + 1, page: 0, width: 1600 }); assert.equal(edited.status, 200);
    report.changedImagePixels = unchangedOutside(normal.image, edited.data.image, target.bounds);
    assert.deepEqual(edited.data.objects.filter(item => item.id !== target.id), normal.objects.filter(item => item.id !== target.id)); assert.deepEqual(edited.data.annotations, normal.annotations); assert.deepEqual(fs.readFileSync(files.A), original);
    await page.locator('#pdf-undo').click(); await until(() => page.evaluate(() => !ReadMDPdf.dirty), 'image undo');
    const undone = await pdf({ action: 'render', session: normal.session, revision: normal.revision + 2, page: 0, width: 1600 }); assert.equal(undone.data.image, normal.image);
    await page.locator('#pdf-redo').click(); await until(() => page.evaluate(() => ReadMDPdf.dirty), 'image redo');
    const saveResponse = page.waitForResponse(response => new URL(response.url()).pathname === '/api/pdf/workspace' && response.request().postDataJSON()?.action === 'save');
    await page.locator('#pdf-save').click(); assert.equal((await (await saveResponse).json()).ok, true); await until(() => page.evaluate(() => !ReadMDPdf.dirty), 'image saved');
    const saved = fs.readFileSync(files.A); assert(!saved.equals(original)); report.savedSha256 = sha(saved);
    const reopened = await open(files.A); assert.equal(reopened.image, edited.data.image); assert.deepEqual(reopened.annotations, normal.annotations);
    const secondAfter = await pdf({ action: 'render', session: reopened.session, revision: reopened.revision, page: 1, width: 1600 }); assert.equal(secondAfter.data.image, secondBefore.data.image);
    await page.screenshot({ path: path.join(args.evidence, 'image-reopened.png') }); report.checks.push('Normal image selection is one operation, undo/redo works, native PDF saves/reopens, unchanged pixels/objects/forms/second page remain intact');
    report.cspViolations = await page.evaluate(() => window.__pdfCspViolations); assert.deepEqual(report.cspViolations, []); assert.deepEqual(report.pageErrors, []);

    phase = 'different PID reopen using the same locked worker';
    await browser.close(); browser = null; page = null; await kernel.stop(); kernel = null;
    const freshProfile = path.join(args.evidence, 'profile-fresh'); report.freshResources = resources(freshProfile);
    const freshPath = process.env.PATH;
    try { process.env.PATH = path.join(process.env.SystemRoot, 'System32'); kernel = await startKernel(args.bin, freshProfile, documents); }
    finally { process.env.PATH = freshPath; }
    report.kernelPids.push(kernel.pid); assert.notEqual(report.kernelPids[0], kernel.pid);
    const fresh = await pdf({ action: 'open', path: files.A, width: 1600 }); assert.equal(fresh.status, 200); assert.equal(fresh.data.image, edited.data.image); assert.deepEqual(fresh.data.annotations, normal.annotations);
    const freshSecond = await pdf({ action: 'render', session: fresh.data.session, revision: fresh.data.revision, page: 1, width: 1600 }); assert.equal(freshSecond.status, 200); assert.equal(freshSecond.data.image, secondBefore.data.image);
    assert.deepEqual(fs.readFileSync(files.A), saved); assert.deepEqual(fs.readFileSync(files.B), original); assert.equal((await pdf({ action: 'close', session: fresh.data.session })).status, 200);
    report.checks.push('A fresh native kernel PID/profile reopens the saved PDF with identical edited image and untouched page/form content'); report.passed = true;
  } catch (error) { report.passed = false; report.failedStage = phase; report.failure = { name: error.name, message: error.message.slice(0, 1400) }; process.exitCode = 1; }
  finally { if (page) report.cspViolations = await page.evaluate(() => window.__pdfCspViolations || []).catch(() => []); if (browser) await browser.close(); if (kernel) await kernel.stop(); fs.writeFileSync(path.join(args.evidence, 'report.json'), JSON.stringify(report, null, 2)); }
  console.log(JSON.stringify({ passed: report.passed, checks: report.checks.length, failedStage: report.failedStage, crossDocument: report.crossDocument, report: path.join(args.evidence, 'report.json'), failure: report.failure }));
}
module.exports = { resources };
if (require.main === module) run().catch(error => { console.error(error.message); process.exitCode = 1; });
