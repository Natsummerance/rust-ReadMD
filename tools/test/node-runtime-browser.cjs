'use strict';
// Owned headless UI, production CSP/auth/session and genuine managed Node.
// The optional chooser returns only a verified synthetic workspace file;
// this is not Cocoa, PowerBox, a native picker or a packaged-app certificate.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const { ROOT, sha, pause, until, openOwned } = require('./owned-browser-host.cjs');
const { options } = require('./v006-table-clipboard-kernel.cjs');
const { nodePack } = require('./node-runtime-fixture.cjs');
const CODE = 'console.log(6*7);';
const SINGLE = '# Same 😀\n\n```javascript cmd output=true\n' + CODE + '\n```\n\nTail.\n';
const DUPLICATE = '# Code 😀\n\n```javascript cmd output=true\n' + CODE + '\n```\n\nSame.\n\n```javascript cmd output=true\n' + CODE + '\n```\n\nTail.\n';
const EXPECTED = '# Code 😀\n\n```javascript cmd output=true\n' + CODE + '\n```\n\nSame.\n\n```javascript cmd output=true\n' + CODE + '\n```\n\n<!-- code_chunk_output -->\n\n42\n\n<!-- /code_chunk_output -->\n\nTail.\n';
const encoded = text => Buffer.from('\ufeff' + text.replaceAll('\n', '\r\n'));
const barrier = () => { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; };
async function bounded(promise, label, ms = 15000) {
  let timer;
  try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(Error('Deadline: ' + label)), ms); })]); }
  finally { clearTimeout(timer); }
}

// Measure only this run's freshly created evidence tree and never follow a
// browser link. Concurrent native cleanup can remove entries between reads.
function ownedSize(directory) {
  let logical = 0, allocated = 0;
  try {
    for (const name of fs.readdirSync(directory)) {
      const file = path.join(directory, name);
      try {
        const stat = fs.lstatSync(file);
        if (stat.isDirectory() && !stat.isSymbolicLink()) {
          const nested = ownedSize(file); logical += nested.logical; allocated += nested.allocated;
        } else { logical += stat.size; allocated += (stat.blocks ?? Math.ceil(stat.size / 512)) * 512; }
      } catch (error) { if (error.code !== 'ENOENT') throw error; }
    }
  } catch (error) { if (error.code !== 'ENOENT') throw error; }
  return { logical, allocated };
}

async function main() {
  const args = options(process.argv.slice(2)); assert(args.bin && !args.base);
  const proof = nodePack(), output = args.evidence, chooserFile = 'owned-node.readmd-pack';
  const fixtures = { '重复代码 中文😀.md': DUPLICATE, 'same-A.md': SINGLE, 'same-B.md': SINGLE };
  const report = {
    passed: false, headless: true, nativeUiCertified: false, chooserAdapterUsed: false,
    artifactScope: 'explicit prebuilt kernel and current served boot; final source certification requires packaging and source-binding receipts',
    nativeBridge: 'settings/session bootstrap and owned fixture chooser only; real authenticated capability import/probe/run/review/save/uninstall',
    modelsRequested: false, productionCsp: true, strictAuth: true, platform: process.platform,
    driverSha256: sha(fs.readFileSync(__filename)), helperSha256: sha(fs.readFileSync(require.resolve('./owned-browser-host.cjs'))),
    binarySha256: sha(fs.readFileSync(args.bin)), bootSha256: sha(fs.readFileSync(path.join(ROOT, 'assets/readmd.boot.js'))),
    pack: { sha256: proof.sha256, manifestSha256: proof.manifestSha256, bytes: proof.size, version: proof.manifest.version, platform: proof.manifest.platform },
    inputDocuments: Object.entries(fixtures).map(([name, text]) => ({ name, sha256: sha(encoded(text)), bytes: encoded(text).length })),
    checks: [], capabilityRequests: [], reviewRequests: [], codeRequests: [], codeReceipts: [], historyKeys: [],
    storage: { transientBudgetBytes: 450 * 1048576, retainedBudgetBytes: 250 * 1048576, sampleIntervalMs: 50, peakObservedLogicalBytes: 0, peakObservedAllocatedBytes: 0 },
  };
  let test, phase = 'start', releaseImport, copiedFile, copiedIdentity, sampler, deadline;
  const responseReads = [];
  function mark(next) { phase = next; console.log(JSON.stringify({ phase })); }
  function sample() {
    const size = ownedSize(output);
    report.storage.peakObservedLogicalBytes = Math.max(report.storage.peakObservedLogicalBytes, size.logical);
    report.storage.peakObservedAllocatedBytes = Math.max(report.storage.peakObservedAllocatedBytes, size.allocated);
    return size;
  }
  function removeCopiedPack() {
    if (!copiedFile || !fs.existsSync(copiedFile)) return;
    const now = fs.lstatSync(copiedFile);
    assert(now.isFile() && !now.isSymbolicLink() && now.ino === copiedIdentity.ino && now.dev === copiedIdentity.dev, 'Owned chooser input identity changed');
    assert.equal(sha(fs.readFileSync(copiedFile)), proof.sha256);
    fs.unlinkSync(copiedFile); report.chooserInputRemoved = true;
  }
  try {
    sampler = setInterval(sample, 50);
    deadline = setTimeout(() => {
      fs.writeFileSync(path.join(output, 'deadline.json'), JSON.stringify({ phase, deadlineMs: 110000 }) + '\n', { flag: 'wx' });
      releaseImport?.(); void test?.context.close();
    }, 110000);
    test = await openOwned(output, async (_api, paths) => {
      for (const [name, text] of Object.entries(fixtures)) fs.writeFileSync(path.join(paths.documents, name), encoded(text), { flag: 'wx' });
      copiedFile = path.join(paths.documents, chooserFile);
      fs.copyFileSync(proof.file, copiedFile, fs.constants.COPYFILE_EXCL);
      copiedIdentity = fs.lstatSync(copiedFile);
      assert.equal(sha(fs.readFileSync(copiedFile)), proof.sha256);
      report.chooserInput = { name: chooserFile, sha256: proof.sha256, bytes: copiedIdentity.size };
    }, { binary: args.bin, playwright: args.playwright, chooserFile });
    const { page, api } = test;
    page.on('request', request => {
      const route = new URL(request.url()).pathname;
      if (request.method() !== 'POST') return;
      if (!['/api/capabilities/packs', '/api/review', '/api/code/run'].includes(route)) return;
      const body = request.postDataJSON();
      if (route === '/api/capabilities/packs') report.capabilityRequests.push({ action: body.action, id: body.id });
      else if (route === '/api/review') report.reviewRequests.push({ action: body.action, documentId: body.document_id, generation: body.generation, revision: body.review_revision, sourceLine: body.source_line });
      else report.codeRequests.push({ lang: body.lang, code: body.code, confirm: body.confirm });
    });
    page.on('response', response => {
      if (new URL(response.url()).pathname !== '/api/code/run') return;
      responseReads.push(response.json().then(body => report.codeReceipts.push({ ok: body.ok, stdout: body.stdout, runtime: body.runtime, errorCode: body.error_code })));
    });
    await page.evaluate(() => {
      window.__nodeHistoryKeys = [];
      document.addEventListener('keydown', event => {
        if (!(event.metaKey || event.ctrlKey) || !['z', 'Z', 'y', 'Y'].includes(event.key)) return;
        setTimeout(() => window.__nodeHistoryKeys.push({ key: event.key, code: event.code, meta: event.metaKey, ctrl: event.ctrlKey, shift: event.shiftKey, prevented: event.defaultPrevented, editorTarget: Boolean(event.target?.closest?.('.cm-editor')) }), 0);
      }, true);
    });
    await until(() => page.evaluate(() => Boolean(window.ReadMDReview?.codeTarget && window.ReadMDCodeRuns)), 'managed Node browser ready');
    assert.equal(await page.evaluate(() => hasPy && window.READMD_ENGINE === 'rust'), true);
    const root = await test.host.api('/'), anonymous = await fetch(new URL('/api/capabilities/packs', root.url), { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ action: 'status', id: 'node' }) });
    assert([401, 403].includes(anonymous.status)); report.anonymousCapabilityStatus = anonymous.status;
    const status = () => api('/api/capabilities/packs', { action: 'status', id: 'node' });
    const docText = () => page.evaluate(() => cmView.state.doc.toString());
    const original = name => assert.deepEqual(fs.readFileSync(path.join(test.documents, name)), encoded(fixtures[name]));
    const idle = () => until(() => page.evaluate(() => window.ReadMDCodeRuns.busy() === 0), 'real code job terminal');
    async function edit(name) {
      await page.evaluate(async file => { await loadFile(file); if (!state.editing) await toggleEdit(); setPvLayout('right'); await renderPreview(); }, path.join(test.documents, name));
      await until(() => page.locator('#preview-pane .code-chunk-run-btn').count().then(count => count > 0), 'actual JavaScript preview');
      assert.equal(await docText(), fixtures[name]);
    }
    const modal = () => page.locator('#capability-install-modal');
    const modalCancel = () => page.locator('#capability-install-modal .capability-install-actions button').nth(0);
    const modalOffline = () => page.locator('#capability-install-modal .capability-install-actions button').nth(1);

    mark('empty profile contextual install and cancel');
    assert.equal((await status()).ready, false);
    const catalog = await api('/api/capabilities/packs', { action: 'catalog' }), entry = catalog.packs.find(item => item.id === 'node');
    assert.equal(entry.delivery, 'offline-only'); assert.equal(entry.sha256, proof.sha256); assert.equal(entry.url, undefined);
    await edit('重复代码 中文😀.md');
    await page.locator('#preview-pane .code-chunk-run-btn').nth(1).click(); await modal().waitFor({ state: 'visible' });
    assert((await page.locator('#capability-install-title').innerText()).includes('Node'));
    const online = page.locator('#capability-install-modal .capability-install-actions button').nth(2);
    assert.equal(await online.isVisible(), false); assert.equal(await online.getAttribute('hidden'), '');
    assert.equal(await modalOffline().isVisible(), true);
    await page.screenshot({ path: path.join(output, 'empty-offline-install.png') });
    await modalCancel().click(); await modal().waitFor({ state: 'detached' }); await idle();
    assert.equal(await docText(), DUPLICATE); original('重复代码 中文😀.md'); assert.equal(report.codeRequests.length, 0);
    assert.equal(test.chooserAdapterUsed, false);
    report.checks.push('empty profile JavaScript Run opens the existing Node installer, hides online install for offline-only delivery; cancel issues no execution and preserves exact document bytes');

    mark('switch same code document during genuine import wait');
    await edit('same-A.md'); await page.locator('#preview-pane .code-chunk-run-btn').click(); await modal().waitFor({ state: 'visible' });
    const entered = barrier(), release = barrier(); releaseImport = release.resolve;
    const heldImport = async route => {
      const body = route.request().method() === 'POST' ? route.request().postDataJSON() : null;
      if (body?.action !== 'import' || body.id !== 'node') { await route.continue(); return; }
      const response = await route.fetch();
      assert.equal(response.status(), 200);
      const actual = await response.json(); assert(actual.ok && actual.job);
      report.importStart = { phase: actual.phase, job: actual.job, status: response.status(), responseSha256: sha(await response.body()), responseUnmodified: true };
      entered.resolve(); await release.promise; await route.fulfill({ response });
    };
    await page.route('**/api/capabilities/packs', heldImport);
    await modalOffline().click(); await bounded(entered.promise, 'actual import-start response');
    await edit('same-B.md');
    const whileHeld = await page.evaluate(() => ({ activeName: getActiveTab().name, codeJobs: ReadMDCodeRuns.busy(), cardOwnerName: document.querySelector('#preview-pane .code-chunk-card')?._readmdCodeOwner?.name }));
    assert.equal(whileHeld.activeName, 'same-B.md'); assert.equal(whileHeld.cardOwnerName, 'same-B.md'); assert.equal(whileHeld.codeJobs, 0);
    report.importWaitSwitch = whileHeld; release.resolve();
    await modal().waitFor({ state: 'detached', timeout: 60000 }); await idle(); await page.unroute('**/api/capabilities/packs', heldImport); releaseImport = null;
    assert.equal((await status()).ready, true); await pause(150);
    assert.equal(report.codeRequests.length, 0); assert.equal(await docText(), SINGLE);
    const protectedTabs = await page.evaluate(() => state.tabs.filter(tab => /same-[AB]\.md$/.test(tab.name)).map(tab => ({ name: tab.name, content: tab.content, dirty: tab.isDirty })));
    assert.equal(protectedTabs.length, 2); for (const tab of protectedTabs) { assert.equal(tab.content.replaceAll('\r\n', '\n'), SINGLE); assert.equal(tab.dirty, false); }
    report.protectedTabsAfterImport = protectedTabs.map(tab => ({ name: tab.name, contentSha256: sha(tab.content), dirty: tab.dirty }));
    original('same-A.md'); original('same-B.md'); original('重复代码 中文😀.md');
    assert.equal(test.chooserAdapterCalls, 1); removeCopiedPack(); sample();
    report.checks.push('owned chooser supplies a verified pack to the genuine authenticated import and probe; switching to identical code while the unmodified import receipt waits makes the old Run exit without executing or changing either document');

    mark('actual second duplicate run, BOM CRLF save and physical undo redo');
    await edit('重复代码 中文😀.md');
    await page.locator('#preview-pane .code-chunk-run-btn').nth(1).click();
    await until(async () => report.codeRequests.length === 1 && await docText() === EXPECTED, 'real Node second-block result'); await idle();
    assert.deepEqual(fs.readFileSync(path.join(test.documents, '重复代码 中文😀.md')), encoded(EXPECTED));
    assert.equal(report.codeRequests[0].confirm, true); assert.equal(report.codeRequests[0].code.trim(), CODE);
    const target = report.reviewRequests.filter(item => item.action === 'code_target'); assert.equal(target.length, 1); assert.equal(target[0].sourceLine, 9);
    const historyStart = report.reviewRequests.length, mod = process.platform === 'darwin' ? 'Meta' : 'Control';
    await page.locator('.cm-content').click(); await page.keyboard.press(mod + '+KeyZ');
    await until(async () => await docText() === DUPLICATE, 'one actual keyboard undo');
    await page.keyboard.press(mod + '+Shift+KeyZ'); await until(async () => await docText() === EXPECTED, 'one actual keyboard redo');
    assert.deepEqual(report.reviewRequests.slice(historyStart).filter(item => ['undo', 'redo'].includes(item.action)).map(item => item.action), ['undo', 'redo']);
    report.historyKeys = await page.evaluate(() => window.__nodeHistoryKeys);
    assert.equal(report.historyKeys.length, 2); assert(report.historyKeys.every(key => key.editorTarget && key.prevented));
    assert.equal(await page.evaluate(() => saveEdit()), true);
    assert.deepEqual(fs.readFileSync(path.join(test.documents, '重复代码 中文😀.md')), encoded(EXPECTED)); original('same-A.md'); original('same-B.md');
    await Promise.all(responseReads);
    assert.equal(report.codeReceipts.length, 1); assert.equal(report.codeReceipts[0].ok, true); assert.equal(report.codeReceipts[0].stdout, '42');
    assert.deepEqual(report.codeReceipts[0].runtime, { id: 'node', version: proof.manifest.version, manifest_sha256: proof.manifestSha256 });
    report.savedDocument = { name: '重复代码 中文😀.md', sha256: sha(encoded(EXPECTED)), bytes: encoded(EXPECTED).length, bom: 'UTF-8', lineEndings: 'CRLF', outputBlocks: 1, sourceLine: target[0].sourceLine };
    report.checks.push('genuine pinned Node returns 42 and writes one output after the second matching source fence only; a real editor keyboard undo and redo each form one durable history action, then save preserves BOM and CRLF');

    mark('1024 by 680 plugin Node row and actual uninstall');
    await page.setViewportSize({ width: 1024, height: 680 }); await page.evaluate(async () => { await openPluginModal(); await refreshOfflineCapabilityPack(); });
    const uninstall = page.locator('#plugin-capability-uninstall-node'); await uninstall.waitFor({ state: 'visible' });
    const row = uninstall.locator('..'); await row.locator('summary').click(); await row.scrollIntoViewIfNeeded();
    report.pluginLayout = await row.evaluate(element => {
      const rect = value => { const r = value.getBoundingClientRect(); return { left: r.left, right: r.right, width: r.width }; };
      const box = document.getElementById('plugin-box'), overlay = document.getElementById('plugin-modal');
      return { viewport: { width: innerWidth, height: innerHeight }, row: { ...rect(element), scrollWidth: element.scrollWidth, clientWidth: element.clientWidth }, box: { ...rect(box), scrollWidth: box.scrollWidth, clientWidth: box.clientWidth }, overlay: { scrollWidth: overlay.scrollWidth, clientWidth: overlay.clientWidth }, document: { scrollWidth: document.documentElement.scrollWidth, clientWidth: document.documentElement.clientWidth }, children: [...element.children].map(child => ({ tag: child.tagName, ...rect(child) })) };
    });
    for (const key of ['row', 'box', 'overlay', 'document']) assert(report.pluginLayout[key].scrollWidth <= report.pluginLayout[key].clientWidth + 1, 'No horizontal overflow: ' + key);
    assert(report.pluginLayout.row.left >= report.pluginLayout.box.left - 1 && report.pluginLayout.row.right <= report.pluginLayout.box.right + 1);
    for (const child of report.pluginLayout.children) assert(child.left >= report.pluginLayout.row.left - 1 && child.right <= report.pluginLayout.row.right + 1, 'Node row child stays within row');
    await page.screenshot({ path: path.join(output, 'node-plugin-1024x680.png') });
    await uninstall.click(); await page.locator('#confirm-modal').waitFor({ state: 'visible' }); await page.locator('#confirm-action').click();
    await until(async () => !(await status()).ready && await page.locator('#plugin-capability-install-node').count() === 1, 'real Node removal and refreshed row', 60000);
    assert.equal(await page.locator('#plugin-capability-uninstall-node').count(), 0);
    report.removedStatus = await status(); assert.equal(report.removedStatus.ready, false);
    assert(report.capabilityRequests.some(item => item.action === 'uninstall_start' && item.id === 'node'));
    assert.equal(fs.existsSync(path.join(test.profile, 'capabilities/managed-v2/node', proof.manifest.version, 'bin', process.platform === 'win32' ? 'node.exe' : 'node')), false);
    await page.locator('#plugin-close').click();
    assert.equal(await docText(), EXPECTED); assert.deepEqual(fs.readFileSync(path.join(test.documents, '重复代码 中文😀.md')), encoded(EXPECTED)); original('same-A.md'); original('same-B.md');
    await page.locator('#preview-pane .code-chunk-run-btn').nth(1).click(); await modal().waitFor({ state: 'visible' });
    await modalCancel().click(); await modal().waitFor({ state: 'detached' }); await idle(); assert.equal(report.codeRequests.length, 1);
    report.checks.push('at 1024×680 the expanded Node row and all controls fit without horizontal overflow; real confirmation and uninstall remove the runtime, preserve all documents and make the next Run request installation again');
    assert.deepEqual(test.errors, []); report.passed = true;
  } catch (error) {
    report.failure = { phase, name: error.name, message: String(error.message).replaceAll(output, '<owned>').replaceAll(ROOT, '<repo>').slice(0, 1200) }; process.exitCode = 1;
    if (test) {
      report.ui = await test.page.evaluate(() => ({ activeName: getActiveTab()?.name, editing: state.editing, jobs: window.ReadMDCodeRuns?.busy(), installer: document.getElementById('capability-install-status')?.textContent, modal: Boolean(document.getElementById('capability-install-modal')), tabs: state.tabs.map(tab => ({ name: tab.name, dirty: tab.isDirty, generation: tab.inputGeneration })) })).catch(() => ({ unavailable: true }));
      await test.page.screenshot({ path: path.join(output, 'first-failure.png') }).catch(() => {});
    }
  } finally {
    clearTimeout(deadline); releaseImport?.();
    if (test) { report.chooserAdapterUsed = test.chooserAdapterUsed; report.chooserAdapterCalls = test.chooserAdapterCalls; report.pageErrors = test.errors; await test.close(); report.ownedBrowserScratchRemoved = true; }
    removeCopiedPack(); clearInterval(sampler); const retained = sample();
    report.storage.retainedLogicalBytes = retained.logical; report.storage.retainedAllocatedBytes = retained.allocated;
    assert(report.storage.peakObservedLogicalBytes <= report.storage.transientBudgetBytes, 'Observed owned transient evidence exceeds budget');
    assert(retained.logical <= report.storage.retainedBudgetBytes, 'Retained owned evidence exceeds budget');
    assert.equal(sha(fs.readFileSync(proof.file)), proof.sha256, 'Original Node pack changed');
    report.originalPackUnchanged = true;
    fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
    console.log(JSON.stringify({ passed: report.passed, checks: report.checks.length, failure: report.failure, chooserAdapterUsed: report.chooserAdapterUsed, binarySha256: report.binarySha256, bootSha256: report.bootSha256, storage: report.storage }));
  }
}
if (require.main === module) main().catch(error => { console.error('Managed Node browser acceptance failed: ' + String(error.message).slice(0, 160)); process.exitCode = 1; });
module.exports = { main };
