'use strict';
// Actual production toolbar/dialog actions with the checked-in CSP and a real
// prebuilt kernel. Cursor placement is fixture setup; alignment is never called
// directly. No browser installation, compilation, native window or user profile.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), assert = require('node:assert/strict');
const { ROOT } = require('./v006-table-clipboard-host.cjs');
const { options, startKernel } = require('./v006-table-clipboard-kernel.cjs');
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const SOURCE = '# Synthetic table alignment\r\n\r\n| Kind | Note | Tail |\r\n| :---- | :---: | ----: |\r\n| **Alpha** | left\\|right \\\\ end <br> next | 尾 |\r\n| Beta | [link](https://example.invalid/a) | 0002 |\r\n\r\nOutside evidence.\r\n';
async function until(check, label, timeout = 30000) { const end = Date.now() + timeout; while (Date.now() < end) { if (await check()) return; await delay(60); } throw Error('Deadline: ' + label); }

async function run() {
  const args = options(process.argv.slice(2)); assert(args.bin && !args.base, 'An explicit prebuilt --bin is required');
  const documents = path.join(args.evidence, 'documents'); fs.mkdirSync(documents);
  const report = { schemaVersion: 1, syntheticOnly: true, headless: true, bypassCSP: false, compiled: false, downloaded: false,
    transport: 'Actual existing Insert/Table toolbar and dialog; cursor setup only uses CM dispatch; save/reopen uses real HTTP',
    binarySha256: sha(fs.readFileSync(args.bin)), bootSha256: sha(fs.readFileSync(path.join(ROOT, 'assets/readmd.boot.js'))),
    sourceSha256: sha(fs.readFileSync(path.join(ROOT, 'assets/js/editor/editor.js'))), checks: [], pageErrors: [] };
  let kernel, browser, page, phase = 'production CSP startup';
  try {
    kernel = await startKernel(args.bin, path.join(args.evidence, 'profile'), documents); report.kernelPids = [kernel.pid];
    const { chromium } = require(args.playwright || require.resolve('playwright', { paths: [path.join(ROOT, 'ui-tests')] }));
    browser = await chromium.launch({ channel: 'msedge', headless: true }); report.browserVersion = browser.version();
    page = await browser.newPage({ bypassCSP: false, viewport: { width: 1440, height: 1000 } });
    page.on('pageerror', error => report.pageErrors.push(error.message.slice(0, 600)));
    await page.addInitScript(() => { localStorage.setItem('readmd_language', 'en'); window.__tableAlignCsp = []; addEventListener('securitypolicyviolation', event => window.__tableAlignCsp.push(event.effectiveDirective)); });
    const boot = page.waitForResponse(response => new URL(response.url()).pathname === '/assets/readmd.boot.js');
    await page.goto(kernel.base); report.servedBootSha256 = sha(await (await boot).body()); assert.equal(report.servedBootSha256, report.bootSha256);
    report.csp = await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content'); assert.match(report.csp, /script-src/); assert.doesNotMatch(report.csp, /'unsafe-eval'/);
    await until(() => page.evaluate(() => window.__readmdAppReady && window.ReadMDReview && typeof loadFile === 'function'), 'production app ready');
    // Passive call/state observations help distinguish a stale-dialog refusal
    // from a transport or assertion error. They never choose an alignment.
    await page.evaluate(() => {
      window.__tableAlignCalls = [];
      if (typeof cmAlignTableColumn === 'function') {
        const original = cmAlignTableColumn;
        cmAlignTableColumn = function (view, value) { const result = original(view, value); window.__tableAlignCalls.push({ value, result }); return result; };
      }
      const open = openTableModal;
      openTableModal = function () { const result = open(); window.__tableAlignOpened = { state: cmView?.state, tab: getActiveTab() }; return result; };
    });
    const texts = await page.evaluate(() => Object.fromEntries(['left', 'center', 'right'].map(value => [value, i18n.t('dialog.tableAlign' + value[0].toUpperCase() + value.slice(1))])));
    const openEditor = async (name, source = SOURCE, marker = 'left\\|right') => {
      const file = path.join(documents, name + '.md'); fs.writeFileSync(file, source, { flag: 'wx' });
      await page.evaluate(async file => {
        if (state.editing) { if (!await ReadMDReview.flushAll()) throw Error('Synthetic review queue did not settle'); exitEdit(); }
        await loadFile(file, { force: true, discardConfirmed: true }); state.pvLayout = 'none';
      }, file);
      await page.locator('#btn-edit').click();
      await until(() => page.evaluate(() => state.editing && window.cmView), 'real editor mounted');
      await page.evaluate(marker => {
        const at = cmView.state.doc.toString().indexOf(marker); if (at < 0) throw Error('Synthetic cell not found');
        cmView.dispatch({ selection: { anchor: at + 1 } }); cmView.focus(); window.__tableAlignTransactions = [];
        cmView.dispatch({ effects: ReadMDCodeMirror.StateEffect.appendConfig.of(ReadMDCodeMirror.EditorView.updateListener.of(update => {
          for (const transaction of update.transactions) if (transaction.docChanged && transaction.isUserEvent('input.table.align')) window.__tableAlignTransactions.push(true);
        })) });
      }, marker); return file;
    };
    const openTable = async () => {
      const trigger = page.locator('[data-menu="md-insert-menu"]');
      if (await trigger.isVisible()) await trigger.click(); else await page.locator('#edit-overflow-trigger').click();
      await page.locator('#btn-insert-table').click(); await page.locator('#table-modal').waitFor({ state: 'visible' });
    };
    // Inspect CM's normalized draft separately from getEditContent(), which
    // deliberately restores the original file's CRLF for persistence.
    const text = () => page.evaluate(() => cmView.state.doc.toString());
    const align = value => page.locator('#table-modal').getByRole('button', { name: texts[value], exact: true });
    const toolbarAction = async id => {
      const button = page.locator(id);
      if (!await button.isVisible()) await page.locator('#edit-overflow-trigger').click();
      await button.click();
    };

    phase = 'existing table visual alignment is reachable';
    const file = await openEditor('aligned-wide'); await openTable();
    report.before = { alignmentButtons: await page.locator('#table-modal [data-table-align]').count(), newTableGridButtons: await page.locator('#table-grid-picker button').count(), sourceUnchanged: (await text()) === SOURCE.replace(/\r\n/g, '\n') };
    await page.screenshot({ path: path.join(args.evidence, 'table-dialog-before.png') });
    assert.equal(await align('right').count(), 1, 'Selecting a GFM cell then Insert/Table must expose its column alignment');
    assert.equal(await align('center').getAttribute('aria-pressed'), 'true');
    assert.equal(await align('left').getAttribute('aria-pressed'), 'false');
    report.dialogGuardBeforeClick = await page.evaluate(() => {
      const opened = window.__tableAlignOpened, hit = typeof cmTableAlignmentContext === 'function' && cmTableAlignmentContext(cmView);
      return { sameState: opened.state === cmView.state, sameDoc: opened.state.doc === cmView.state.doc, sameSelection: opened.state.selection.eq(cmView.state.selection),
        sameTab: opened.tab === getActiveTab(), context: hit ? { column: hit.column, alignment: hit.alignment } : null };
    });
    const expected = SOURCE.replace(/\r\n/g, '\n').replace('| :---- | :---: | ----: |', '| :---- | ---: | ----: |');
    await align('right').click(); await until(async () => await text() === expected, 'real column aligned right');
    assert.equal(await page.evaluate(() => window.__tableAlignTransactions.length), 1); assert.equal(fs.readFileSync(file, 'utf8'), SOURCE);
    await toolbarAction('#edit-undo'); await until(async () => await text() === SOURCE.replace(/\r\n/g, '\n'), 'one undo restores source');
    await toolbarAction('#edit-redo'); await until(async () => await text() === expected, 'one redo restores alignment');
    await openTable(); await align('left').click();
    await until(async () => await text() === expected.replace('| :---- | ---: | ----: |', '| :---- | :--- | ----: |'), 'second distinct alignment');
    await toolbarAction('#edit-undo'); await until(async () => await text() === expected, 'second action is an independent undo');
    report.checks.push('Existing toolbar changes only the selected delimiter cell; formatting/escaped pipes/backslashes/HTML, outside source and disk are preserved; each action is one undo');

    phase = 'explicit save and reader reopen';
    const save = page.waitForResponse(response => new URL(response.url()).pathname === '/api/save' && response.request().method() === 'POST');
    await page.locator('#edit-save').click(); assert.equal((await (await save).json()).ok, true);
    await until(() => page.evaluate(() => !getActiveTab().isDirty), 'save acknowledged');
    const persisted = expected.replace(/\n/g, '\r\n'); assert.equal(fs.readFileSync(file, 'utf8'), persisted); report.savedSha256 = sha(persisted);
    await page.locator('#edit-cancel').click();
    await page.evaluate(file => loadFile(file, { force: true, discardConfirmed: true }), file);
    assert.equal(await page.evaluate(() => state.original), persisted);
    const rendered = await page.locator('#content table tr').evaluateAll(rows => rows.map(row => { const cell = row.children[1]; return { text: cell.textContent, align: cell.getAttribute('align'), computed: getComputedStyle(cell).textAlign }; }));
    assert.equal(rendered.length, 3); assert(rendered.every(cell => cell.align === 'right' && cell.computed === 'right'));
    report.rendered = rendered; await page.screenshot({ path: path.join(args.evidence, 'aligned-reopened.png') });
    report.checks.push('Explicit toolbar Save preserves CRLF; actual loadFile/reader reopen shows computed right alignment and intact visible content');

    phase = 'narrow window and keyboard accessibility';
    await page.setViewportSize({ width: 360, height: 800 }); await openEditor('aligned-narrow'); await openTable();
    const right = align('right'); await right.focus();
    const bounds = await right.boundingBox(); assert(bounds && bounds.x >= 0 && bounds.x + bounds.width <= 360);
    const beforeEscape = await text(); await page.keyboard.press('Escape'); await until(() => page.locator('#table-modal').evaluate(element => element.classList.contains('hidden')), 'Escape closes dialog'); assert.equal(await text(), beforeEscape);
    await openTable(); await align('right').focus();
    for (let i = 0; i < 5; i++) { await page.keyboard.press('Tab'); assert(await page.evaluate(() => document.getElementById('table-modal').contains(document.activeElement))); }
    await align('right').focus(); await page.keyboard.press('Space'); await until(async () => await text() === expected, 'keyboard Space applies alignment');
    assert(await page.evaluate(() => !!document.activeElement.closest('#edit-cm .cm-content')));
    await toolbarAction('#edit-undo'); await until(async () => await text() === SOURCE.replace(/\r\n/g, '\n'), 'narrow operation undo');
    report.checks.push('At 360px, overflow toolbar reaches labelled alignment buttons; Tab stays in dialog, Escape is inert, Space applies and returns editor focus');

    phase = 'code block safety and ordinary insertion';
    const fenced = '```md\n' + SOURCE.replace(/\r\n/g, '\n') + '```\n'; await openEditor('fenced-table', fenced); await openTable();
    assert.equal(await page.locator('#table-modal [data-table-align]').count(), 0); assert.equal(await text(), fenced); assert.equal(await page.locator('#table-grid-picker button').count(), 100);
    await page.keyboard.press('Escape'); report.checks.push('A fenced table never exposes column editing; ordinary new-table picker remains available');

    report.cspViolations = await page.evaluate(() => window.__tableAlignCsp); assert.deepEqual(report.cspViolations, []); assert.deepEqual(report.pageErrors, []);
    phase = 'fresh PID source reopen'; await browser.close(); browser = null; page = null; await kernel.stop(); kernel = null;
    kernel = await startKernel(args.bin, path.join(args.evidence, 'profile-fresh'), documents); report.kernelPids.push(kernel.pid); assert.notEqual(report.kernelPids[0], kernel.pid);
    const fresh = await kernel.api('/api/file?p=' + encodeURIComponent(file)); assert.equal(fresh.status, 200); const data = await fresh.json(); assert.equal(data.original ?? data.content, persisted);
    report.checks.push('A fresh kernel PID and profile read the exact saved Markdown bytes'); report.passed = true;
  } catch (error) {
    report.passed = false; report.failedStage = phase; report.failure = { name: error.name, message: error.message.slice(0, 1400) }; process.exitCode = 1;
    if (page) await page.screenshot({ path: path.join(args.evidence, 'failure.png') }).catch(() => {});
  } finally {
    if (page) {
      report.cspViolations = await page.evaluate(() => window.__tableAlignCsp || []).catch(() => []);
      report.controllerCalls = await page.evaluate(() => window.__tableAlignCalls || []).catch(() => []);
    }
    if (browser) await browser.close(); if (kernel) await kernel.stop(); fs.writeFileSync(path.join(args.evidence, 'report.json'), JSON.stringify(report, null, 2));
  }
  console.log(JSON.stringify({ passed: report.passed, checks: report.checks.length, failedStage: report.failedStage, report: path.join(args.evidence, 'report.json'), failure: report.failure }));
}
if (require.main === module) run().catch(error => { console.error(error.message); process.exitCode = 1; });
