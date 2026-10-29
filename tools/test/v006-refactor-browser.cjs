'use strict';
// Opt-in real kernel + headless Edge acceptance, with the production boot/CSP.
// No build/download/native dialog/CUA. Every document/profile is created under
// a fresh explicit evidence directory; runtime tokens are never recorded.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const net = require('node:net');
const { spawn } = require('node:child_process');
const assert = require('node:assert/strict');
const { ROOT } = require('./v006-table-clipboard-host.cjs');
const { options } = require('./v006-table-clipboard-kernel.cjs');
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const normalize = value => String(value || '').replace(/\\/g, '/').toLowerCase();
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(check, label, timeout = 30000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) { if (await check()) return; await delay(60); }
  throw Error('Deadline: ' + label);
}
function ownedEnvironment(profile) {
  // An allowlist avoids inheriting provider, proxy, credential, or user app
  // configuration variables into either the kernel or browser processes.
  const allowed = new Set(['systemroot', 'windir', 'comspec', 'pathext', 'path', 'programfiles', 'programfiles(x86)', 'programdata', 'allusersprofile']);
  const env = Object.fromEntries(Object.entries(process.env).filter(([name]) => allowed.has(name.toLowerCase())));
  const directories = { TEMP: path.join(profile, 'temp'), TMP: path.join(profile, 'temp'), APPDATA: path.join(profile, 'appdata'),
    LOCALAPPDATA: path.join(profile, 'localappdata'), USERPROFILE: path.join(profile, 'user') };
  for (const directory of new Set(Object.values(directories))) fs.mkdirSync(directory, { recursive: true });
  return { ...env, ...directories, READMD_DATA_DIR: profile, READMD_EXTRA_ROOTS: '', READMD_REQUIRE_TOKEN: '1' };
}
async function unusedPort() {
  const socket = net.createServer();
  await new Promise((resolve, reject) => { socket.once('error', reject); socket.listen(0, '127.0.0.1', resolve); });
  const port = socket.address().port; await new Promise(resolve => socket.close(resolve)); return port;
}
async function startOwnedKernel(binary, profile, documents, env) {
  const port = await unusedPort(), base = 'http://127.0.0.1:' + port;
  const child = spawn(binary, ['--no-window', '--port', String(port), '--require-token', '--data-dir', profile,
    '--workspace', documents, '--assets', path.join(ROOT, 'assets')], { cwd: ROOT, windowsHide: true, stdio: 'ignore', env });
  let spawnError; child.on('error', error => { spawnError = error; });
  const stop = async () => {
    if (spawnError || child.exitCode !== null || child.signalCode !== null) return;
    child.kill();
    await until(() => child.exitCode !== null || child.signalCode !== null, 'owned kernel stop', 10000);
  };
  try {
    const end = Date.now() + 30000;
    while (Date.now() < end) {
      if (spawnError || child.exitCode !== null || child.signalCode !== null) throw Error('Owned kernel exited before readiness');
      try {
        const response = await fetch(base, { signal: AbortSignal.timeout(1500) });
        const html = await response.text();
        if (response.ok && /name="readmd-app-token"\s+content="[^"]+"/.test(html)) return { base, pid: child.pid, stop };
      } catch (_) { /* Wait only for this owned process. */ }
      await delay(100);
    }
    throw Error('Owned kernel readiness deadline exceeded');
  } catch (error) { await stop(); throw error; }
}
async function main() {
  const args = options(process.argv.slice(2));
  assert(args.bin && !args.base, 'An explicit rebuilt prebuilt --bin is required; no compilation or download fallback exists');
  const documents = path.join(args.evidence, 'documents'), nested = path.join(documents, 'nested'), profile = path.join(args.evidence, 'profile');
  fs.mkdirSync(documents); fs.mkdirSync(nested); fs.mkdirSync(profile);
  const sourcePath = path.join(documents, 'original.md'), renamedPath = path.join(documents, 'renamed.md');
  const firstPath = path.join(documents, 'first.md'), secondPath = path.join(nested, 'second.md');
  const initial = new Map([
    [sourcePath, '# Synthetic target\n\n[Self](original.md#synthetic-target)\n\nTail.\n'],
    [firstPath, '# Saved reference one\n\n[Target](original.md#synthetic-target)\n'],
    [secondPath, '# CRLF reference\r\n\r\n[Target][destination]\r\n\r\n[destination]: ../original.md?draft=1#synthetic-target "Synthetic title"\r\n'],
  ]);
  for (const [file, body] of initial) fs.writeFileSync(file, body, { flag: 'wx' });
  const beforeHashes = new Map([...initial].map(([file]) => [file, sha(fs.readFileSync(file))]));
  const env = ownedEnvironment(profile);
  const report = { schemaVersion: 1, syntheticOnly: true, headless: true, browserChannel: 'msedge', bypassCSP: false,
    compiled: false, downloaded: false, nativeBridge: 'minimal synthetic chooser/bridge shim; production apiFetch authentication',
    binarySha256: sha(fs.readFileSync(args.bin)), bootSha256: sha(fs.readFileSync(path.join(ROOT, 'assets/readmd.boot.js'))),
    adapterSha256: sha(fs.readFileSync(path.join(ROOT, 'assets/js/features/document-refactor.js'))), checks: [], pageErrors: [], protocol: [], folderRequests: [], apiFailures: [], modalFit: [] };
  const { chromium } = require(args.playwright || require.resolve('playwright', { paths: [path.join(ROOT, 'ui-tests')] }));
  let kernel, browser, page, phase = 'startup';
  const safe = value => String(value || '').replaceAll(args.evidence, '[owned-evidence]').replaceAll(documents, '[owned-documents]').replaceAll(profile, '[owned-profile]').slice(0, 500);
  async function start() {
    kernel = await startOwnedKernel(args.bin, profile, documents, env); (report.kernelPids ||= []).push(kernel.pid);
    browser = await chromium.launch({ channel: 'msedge', headless: true, env });
    page = await browser.newPage({ bypassCSP: false, viewport: { width: 1160, height: 820 }, serviceWorkers: 'block' });
    page.on('pageerror', error => report.pageErrors.push(safe(error.message)));
    page.on('response', async response => {
      try {
        const pathname = new URL(response.url()).pathname;
        if (pathname.startsWith('/api/') && response.status() >= 400 && report.apiFailures.length < 30) report.apiFailures.push({ route: pathname, status: response.status() });
        if (pathname === '/api/list') {
          const data = await response.json();
          report.folderRequests.push({ status: response.status(), directory: typeof data.dir === 'string', files: Array.isArray(data.files) ? data.files.length : null, errorCode: data.error_code || null });
        }
        if (pathname !== '/api/rename/refactor') return;
        const body = response.request().postDataJSON(), data = await response.json();
        report.protocol.push({ action: body.action, status: response.status(), state: data.state || null,
          errorCode: data.error_code || null, files: Array.isArray(data.files) ? data.files.length : null });
      } catch (_) { /* Diagnostic capture never changes an application request. */ }
    });
    await page.route('**/*', route => new URL(route.request().url()).origin === kernel.base ? route.continue() : route.abort());
    await page.addInitScript(({ folder }) => {
      const headers = () => {
        const token = document.querySelector('meta[name="readmd-app-token"]')?.content || '';
        return { 'Content-Type': 'application/json', 'X-ReadMD-App-Token': token, 'X-ReadMD-Token': token };
      };
      const post = async (route, body) => (await fetch(route, { method: 'POST', headers: headers(), body: JSON.stringify(body) })).json();
      window.__syntheticFolderSelections = 0;
      window.pywebview = { api: {
        get_settings: async () => { const value = await (await fetch('/api/settings', { headers: headers() })).json(); return value?.settings && typeof value.ok === 'boolean' ? value.settings : value; },
        save_settings: value => post('/api/settings', value),
        choose_folder: async () => { window.__syntheticFolderSelections++; return folder; },
        rename_file: (file, stem, options) => post('/api/rename', { ...(options || {}), path: file, new_stem: stem }).catch(() => ({ ok: false, state: 'unknown' })),
        rename_status: operationId => post('/api/rename/status', { operation_id: operationId }).catch(() => ({ ok: false, state: 'unknown' })),
        rename_abandon: operationId => post('/api/rename/abandon', { operation_id: operationId }).catch(() => ({ ok: false, state: 'unknown' })),
        get_system_language: async () => 'en', get_autostart: async () => false,
        get_app_info: async () => ({}), start_modules: async () => true,
      } };
      localStorage.setItem('readmd_language', 'en');
    }, { folder: documents });
    const boot = page.waitForResponse(response => new URL(response.url()).pathname === '/assets/readmd.boot.js');
    await page.goto(kernel.base);
    assert.equal(sha(await (await boot).body()), report.bootSha256, 'The current production boot is actually served');
    const csp = await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content');
    assert.match(csp, /script-src/); assert.doesNotMatch(csp, /'unsafe-eval'/);
    await until(() => page.evaluate(() => window.__readmdAppReady && window.ReadMDRefactor && window.ReadMDSession && window.ReadMDReview), 'production application ready');
    await page.evaluate(() => window.__readmdSessionReady);
    assert.equal(await page.evaluate(() => window.READMD_ENGINE), 'rust');
  }
  async function stop() {
    if (browser) { await browser.close(); browser = null; page = null; }
    if (kernel) { await kernel.stop(); kernel = null; }
  }
  async function openRename() {
    const sourceId = await page.evaluate(at => state.tabs.find(tab => normalizePath(tab.path || '') === normalizePath(at))?.id, sourcePath);
    assert(sourceId, 'The original source tab exists');
    await page.evaluate(id => switchTab(id), sourceId);
    await page.locator('#doc-tabs-bar .tab-item.active .tab-title').first().dblclick();
    const input = page.locator('.tab-title-input'); await input.waitFor({ state: 'visible' });
    await input.fill('renamed'); await input.press('Enter');
    await page.locator('#document-refactor-modal').waitFor({ state: 'visible' });
  }
  async function tabStates() {
    return page.evaluate(() => state.tabs.map(tab => {
      const review = ReadMDReview.snapshot(tab);
      return { id: tab.id, name: tab.name, path: tab.path, original: tab.original, revision: tab.revision,
        dirty: tab.isDirty, pending: Boolean(tab._renamePending), recoveryKey: tab.recoveryKey,
        documentId: review?.document_id, reviewContent: review?.content, reviewRevision: review?.revision, diskRevision: review?.disk_revision };
    }));
  }
  async function inspectModalFit(width, height) {
    await page.setViewportSize({ width, height });
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    const geometry = await page.evaluate(() => {
      const rect = element => {
        const value = element.getBoundingClientRect();
        return { left: value.left, right: value.right, top: value.top, bottom: value.bottom, width: value.width, height: value.height };
      };
      const box = document.querySelector('#document-refactor-modal .document-refactor-box');
      const list = box.querySelector('.document-refactor-files'), actions = box.querySelector('.document-refactor-actions');
      return { box: rect(box), list: { ...rect(list), clientWidth: list.clientWidth, scrollWidth: list.scrollWidth,
        clientHeight: list.clientHeight, scrollHeight: list.scrollHeight }, actions: rect(actions),
        buttons: [...actions.querySelectorAll('button')].map(rect), horizontalOverflow: box.scrollWidth > box.clientWidth + 1 };
    });
    assert(geometry.box.left >= 8 && geometry.box.right <= width - 8 && geometry.box.top >= 8 && geometry.box.bottom <= height - 8,
      'The diff dialog fits inside the ' + width + '×' + height + ' viewport');
    assert.equal(geometry.horizontalOverflow, false); assert(geometry.list.clientHeight > 0);
    assert(geometry.list.scrollWidth <= geometry.list.clientWidth + 1, 'Long synthetic paths wrap without horizontal clipping');
    assert(geometry.actions.top >= geometry.list.bottom - 1, 'The action row does not overlap the scrolling file list');
    assert.equal(geometry.buttons.length, 2);
    for (const button of geometry.buttons) {
      assert(button.left >= geometry.box.left && button.right <= geometry.box.right && button.top >= geometry.box.top && button.bottom <= geometry.box.bottom,
        'Both actions stay visible inside the dialog');
      assert(button.width >= 71.5 && button.height >= 43.5, 'Actions retain their minimum target size');
    }
    await page.keyboard.press('Tab');
    assert.equal(await page.evaluate(() => Boolean(document.activeElement?.closest('#document-refactor-modal'))), true, 'Keyboard focus remains in the resized modal');
    report.modalFit.push({ viewport: { width, height }, ...geometry });
    await page.screenshot({ path: path.join(args.evidence, 'preview-' + width + 'x' + height + '.png') });
  }
  try {
    await start();
    phase = 'explicit workspace selection and three saved review identities';
    await page.locator('#btn-folder').click();
    await until(() => page.evaluate(folder => state.folder && normalizePath(state.folder) === normalizePath(folder), documents), 'explicit chosen workspace');
    assert.equal(await page.evaluate(() => window.__syntheticFolderSelections), 1);
    await page.evaluate(async paths => {
      for (const path of paths) { await loadFile(path); const tab = getActiveTab(); await ReadMDReview.ensure(tab, tab.original); }
      if (!await ReadMDSession.flush()) throw Error('Synthetic initial session did not persist');
    }, [sourcePath, firstPath, secondPath]);
    const beforeTabs = await tabStates(); assert.equal(beforeTabs.length, 3); assert(beforeTabs.every(tab => tab.documentId && !tab.dirty));
    assert((await page.locator('#file-list .tree-name').allTextContents()).includes('original.md'));
    report.checks.push('A chooser action selects the owned workspace; source and two saved references obtain real durable review identities');

    phase = 'real diff preview and cancellation';
    await openRename();
    assert.equal(await page.locator('#document-refactor-modal details').count(), 3);
    const diffText = await page.locator('#document-refactor-modal code').allTextContents();
    assert.deepEqual(diffText, ['original', 'renamed', 'original', 'renamed', 'original', 'renamed'], 'The UI displays the exact changed spans, excluding the unchanged .md suffix');
    const sourceSummary = await page.locator('#document-refactor-modal .document-refactor-source').textContent();
    assert(sourceSummary.includes('original.md')); assert(sourceSummary.includes('renamed.md'));
    assert.equal(await page.evaluate(() => Boolean(document.activeElement?.closest('#document-refactor-modal'))), true);
    await inspectModalFit(1160, 820); await inspectModalFit(1024, 680);
    await page.setViewportSize({ width: 1160, height: 820 });
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    await page.screenshot({ path: path.join(args.evidence, 'preview.png') });
    const referenceId = beforeTabs.find(tab => normalize(tab.path) === normalize(firstPath)).id;
    await page.evaluate(id => closeTab(id, true), referenceId);
    assert.equal(await page.evaluate(() => state.tabs.length), 3, 'A force-close cannot remove an affected tab behind the preview');
    await page.locator('#document-refactor-modal .document-refactor-actions button').first().click();
    await until(() => page.evaluate(() => !ReadMDRefactor.busy()), 'cancelled preview releases reservations');
    for (const [file, hash] of beforeHashes) assert.equal(sha(fs.readFileSync(file)), hash);
    assert.equal(fs.existsSync(renamedPath), false);
    assert((await tabStates()).every(tab => !tab.pending && !tab.dirty));
    const cancelledNames = await page.locator('#file-list .tree-name').allTextContents();
    assert(cancelledNames.includes('original.md')); assert.equal(cancelledNames.includes('renamed.md'), false);
    report.checks.push('The actual diff fits 1160×820 and 1024×680 with visible actions and trapped focus; Cancel preserves every byte/sidebar name and force-close is fenced');

    phase = 'confirmed group adoption';
    await openRename();
    await page.locator('#document-refactor-modal .document-refactor-actions button').last().click();
    await until(() => page.evaluate(() => !ReadMDRefactor.busy() && !ReadMDSession.busy() && state.tabs.some(tab => tab.name === 'renamed.md' && !tab._renamePending)), 'confirmed refactor and session settlement');
    assert.equal(fs.existsSync(sourcePath), false);
    const expected = new Map([...initial].map(([file, body]) => [file === sourcePath ? renamedPath : file, body.replaceAll('original.md', 'renamed.md')]));
    for (const [file, body] of expected) assert.equal(fs.readFileSync(file, 'utf8'), body);
    assert.equal(/(^|[^\r])\n/.test(fs.readFileSync(secondPath, 'utf8')), false, 'The CRLF reference retains every line ending');
    const afterTabs = await tabStates(); assert.equal(afterTabs.length, 3);
    for (const before of beforeTabs) {
      const after = afterTabs.find(tab => tab.id === before.id), actual = normalize(before.path) === normalize(sourcePath) ? renamedPath : [...expected.keys()].find(file => normalize(file) === normalize(before.path));
      assert(after); assert.equal(after.documentId, before.documentId); assert.equal(after.recoveryKey, before.recoveryKey);
      assert.equal(normalize(after.path), normalize(actual)); assert.equal(after.original, expected.get(actual));
      assert.equal(after.revision, sha(after.original)); assert.equal(after.reviewContent, after.original.replace(/\r\n?/g, '\n'));
      assert.equal(after.reviewRevision, sha(after.reviewContent)); assert.equal(after.diskRevision, sha(fs.readFileSync(actual)));
      assert.equal(after.dirty, false); assert.equal(after.pending, false);
    }
    const renamedNames = await page.locator('#file-list .tree-name').allTextContents();
    assert(renamedNames.includes('renamed.md'), 'The selected workspace sidebar shows the committed destination');
    assert.equal(renamedNames.includes('original.md'), false, 'The sidebar removes the obsolete source name after durable settlement');
    assert(renamedNames.includes('first.md') && renamedNames.includes('second.md'));
    await page.screenshot({ path: path.join(args.evidence, 'renamed-tabs.png') });
    report.checks.push('Confirm updates self-links, inline references, nested reference definitions and the Files sidebar; CRLF, tab/recovery identities and durable review IDs remain correct');

    phase = 'immediate reference edit save close and reopen';
    await page.evaluate(id => switchTab(id), referenceId);
    await page.locator('#btn-edit').click(); await until(() => page.evaluate(() => state.editing && Boolean(window.cmView)), 'reference editor mounts');
    const savedText = '\nSaved after the real refactor.\n';
    await page.evaluate(text => cmView.dispatch({ changes: { from: cmView.state.doc.length, insert: text } }), savedText);
    await page.evaluate(() => cmView.focus()); await page.keyboard.press('Control+z');
    await until(() => page.evaluate(() => !cmView.state.doc.toString().includes('Saved after the real refactor.')), 'reference undo');
    await page.keyboard.press('Control+y'); await until(() => page.evaluate(() => cmView.state.doc.toString().includes('Saved after the real refactor.')), 'reference redo');
    await page.locator('#edit-save').click();
    await until(() => page.evaluate(() => !getActiveTab().isDirty && !ReadMDReview.busy()), 'reference saved against adopted head');
    expected.set(firstPath, expected.get(firstPath) + savedText); assert.equal(fs.readFileSync(firstPath, 'utf8'), expected.get(firstPath));
    await page.locator('#edit-cancel').click(); await until(() => page.evaluate(() => !state.editing), 'reference reader restored');
    await page.locator('#doc-tabs-bar .tab-item.active .tab-close').first().click();
    await until(() => page.evaluate(() => state.tabs.length === 2), 'saved reference tab closes');
    await page.evaluate(async () => { if (!await ReadMDSession.flush()) throw Error('Synthetic close session did not persist'); });
    const closedSession = await page.evaluate(async () => (await (await apiFetch('/api/session')).json()).session);
    assert.equal(closedSession.tabs.length, 2); assert.equal(closedSession.tabs.some(tab => normalize(tab.path) === normalize(firstPath)), false);
    await page.evaluate(async at => { await loadFile(at); await ReadMDReview.ensure(getActiveTab(), getActiveTab().original); if (!await ReadMDSession.flush()) throw Error('Synthetic reopened session did not persist'); }, firstPath);
    const reopened = (await tabStates()).find(tab => normalize(tab.path) === normalize(firstPath));
    assert.equal(reopened.documentId, beforeTabs.find(tab => tab.id === referenceId).documentId); assert.equal(reopened.original, expected.get(firstPath));
    report.checks.push('A reference edits, undoes/redoes, saves, closes and reopens immediately after adoption using its same durable review identity');

    phase = 'fresh kernel and browser restart';
    await stop(); await start();
    await until(() => page.evaluate(() => state.tabs.length === 3 && state.tabs.every(tab => ReadMDReview.snapshot(tab)?.document_id && !tab._renamePending)), 'three restored review tabs');
    const restored = await tabStates();
    for (const before of beforeTabs) {
      const actual = normalize(before.path) === normalize(sourcePath) ? renamedPath : [...expected.keys()].find(file => normalize(file) === normalize(before.path));
      const tab = restored.find(tab => normalize(tab.path) === normalize(actual)); assert(tab);
      assert.equal(tab.documentId, before.documentId); assert.equal(tab.original, expected.get(actual)); assert.equal(tab.dirty, false); assert.equal(tab.pending, false);
    }
    assert.equal(fs.existsSync(sourcePath), false); assert.deepEqual(report.pageErrors, []);
    report.checks.push('A fresh owned kernel and headless Edge restore all three settled files, saved reference text and the same review identities');
    report.passed = true;
  } catch (error) {
    report.passed = false; report.failedStage = phase; report.failure = safe(error.message);
    if (page) {
      try {
        const diagnostic = await page.evaluate(() => ({ chosenFolderCalls: window.__syntheticFolderSelections || 0,
          hasNativeBridge: typeof hasPy !== 'undefined' && hasPy, selectedFolder: typeof state !== 'undefined' && Boolean(state.folder),
          refactorBusy: window.ReadMDRefactor?.busy?.() || false, sessionBusy: window.ReadMDSession?.busy?.() || false,
          dialogs: [...document.querySelectorAll('[role="dialog"]')].filter(element => !element.classList.contains('hidden')).map(element => element.id),
          messages: [...document.querySelectorAll('.toast')].map(element => element.textContent || '').slice(-4) }));
        diagnostic.messages = diagnostic.messages.map(safe); report.failureUi = diagnostic;
        await page.screenshot({ path: path.join(args.evidence, 'failure.png') });
      } catch (_) { /* Preserve the original failing assertion. */ }
    }
  } finally {
    try { await stop(); } catch (error) { report.cleanupFailure = safe(error.message); report.passed = false; }
    fs.writeFileSync(path.join(args.evidence, 'report.json'), JSON.stringify(report, null, 2));
  }
  console.log(JSON.stringify(report)); if (!report.passed) process.exitCode = 1;
}
if (require.main === module) main().catch(error => { console.error(String(error.message).slice(0, 300)); process.exitCode = 1; });
