'use strict';
// node tools/test/v006-table-clipboard-browser.cjs --bin .cache/community-upgrade/build/debug/readmd.exe --evidence .cache/v006-table-clipboard/browser-1
// Playwright must already be installed. Production CSP remains active. TSV and
// PNG use Chromium's headless clipboard + Control+V and must emit trusted paste;
// CSV uses a separately labelled DOM paste event because Chrome does not expose
// native text/csv through ClipboardItem. No parser/transform is called directly.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const { ROOT } = require('./v006-table-clipboard-host.cjs');
const { options, startKernel, connect } = require('./v006-table-clipboard-kernel.cjs');
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));

async function poll(page, predicate, arg, timeout = 30000) {
  const until = Date.now() + timeout;
  while (Date.now() < until) {
    if (await page.evaluate(predicate, arg)) return;
    await delay(75);
  }
  throw Error('Browser condition did not become true with production CSP enabled');
}

async function run() {
  const args = options(process.argv.slice(2));
  const playwrightPath = args.playwright || require.resolve('playwright', { paths: [path.join(ROOT, 'ui-tests')] });
  const { chromium } = require(playwrightPath);
  const documents = path.join(args.evidence, 'documents');
  fs.mkdirSync(documents);
  const report = { schemaVersion: 1, syntheticOnly: true, headless: true, compiled: false, bypassCSP: false,
    userConfigurationRead: false, tokensLogged: false, pasteTransport: 'Chromium headless clipboard -> Control+V -> trusted contentDOM paste',
    csvTransport: 'DataTransfer text/csv -> synthetic contentDOM ClipboardEvent (explicitly not native OS CSV)',
    sourceSha256: sha(fs.readFileSync(path.join(ROOT, 'assets/js/editor/editor.js'))),
    bootSha256: sha(fs.readFileSync(path.join(ROOT, 'assets/readmd.boot.js'))), cases: [], pageErrors: [] };
  let kernel, browser, page;
  try {
    kernel = args.bin ? await startKernel(args.bin, path.join(args.evidence, 'profile'), documents) : { ...await connect(args.base), stop: async () => {} };
    report.kernelPid = kernel.pid;
    if (args.bin) report.binarySha256 = sha(fs.readFileSync(args.bin));
    // Use the repository's established installed Chromium channel. No optional
    // Playwright browser download, visible window or existing profile is used.
    browser = await chromium.launch({ headless: true, channel: 'msedge' });
    report.browser = { channel: 'msedge', version: browser.version() };
    const context = await browser.newContext({ permissions: ['clipboard-read', 'clipboard-write'], bypassCSP: false, viewport: { width: 1440, height: 900 } });
    page = await context.newPage();
    page.setDefaultTimeout(30000);
    page.on('pageerror', error => report.pageErrors.push({ name: error.name, message: error.message.slice(0, 600) }));
    await page.addInitScript(() => {
      localStorage.setItem('readmd_language', 'en');
      localStorage.setItem('readmd-editor-prefs', JSON.stringify({ lineNumbers: false, focus: false, typewriter: false, aiCompletion: false }));
      window.__tableCspViolations = [];
      window.addEventListener('securitypolicyviolation', event => window.__tableCspViolations.push(event.effectiveDirective));
    });
    const bootResponse = page.waitForResponse(response => new URL(response.url()).pathname === '/assets/readmd.boot.js');
    const response = await page.goto(kernel.base, { waitUntil: 'domcontentloaded' });
    assert.equal(response.status(), 200);
    const loadedBoot = await bootResponse;
    assert.equal(loadedBoot.status(), 200);
    report.servedBootSha256 = sha(await loadedBoot.body());
    assert.equal(report.servedBootSha256, report.bootSha256);
    const headers = await response.allHeaders();
    const metaPolicy = await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content');
    report.csp = headers['content-security-policy'] || metaPolicy || '';
    report.cspDelivery = headers['content-security-policy'] ? 'HTTP header' : 'production index.html meta';
    assert.match(report.csp, /script-src/);
    assert.doesNotMatch(report.csp, /'unsafe-eval'/);
    await poll(page, () => window.__readmdAppReady && window.ReadMDRecovery && typeof loadFile === 'function');
    report.loadedClipboardParser = await page.evaluate(() => typeof clipboardTableRows === 'function');
    report.nativeCsvClipboardTypeSupported = await page.evaluate(() => typeof ClipboardItem.supports === 'function' ? ClipboardItem.supports('text/csv') : null);

    const openEditor = async (name, source) => {
      const file = path.join(documents, name + '.md');
      fs.writeFileSync(file, source, { flag: 'wx' });
      await page.evaluate(async file => {
        if (state.editing) exitEdit();
        await loadFile(file, { force: true, discardConfirmed: true });
        state.pvLayout = 'none';
      }, file);
      await page.locator('#btn-edit').click();
      await poll(page, () => state.editing && window.cmView && document.querySelector('#edit-cm .cm-content'));
      await page.evaluate(() => {
        window.__tablePasteEvents = [];
        cmView.contentDOM.addEventListener('paste', event => {
          const record = { trusted: event.isTrusted, types: Array.from(event.clipboardData?.types || []) };
          window.__tablePasteEvents.push(record);
        }, true);
        window.__tableDocumentTransactions = [];
        cmView.dispatch({ effects: ReadMDCodeMirror.StateEffect.appendConfig.of(ReadMDCodeMirror.EditorView.updateListener.of(update => {
          for (const transaction of update.transactions) if (transaction.docChanged) window.__tableDocumentTransactions.push({ paste: transaction.isUserEvent('input.paste'), remote: transaction.isUserEvent('review.remote') });
        })) });
        cmView.dispatch({ selection: { anchor: cmView.state.doc.length } });
        cmView.focus();
      });
      return file;
    };
    const nativeTextPaste = async text => {
      await page.evaluate(text => navigator.clipboard.writeText(text), text);
      await page.locator('#edit-cm .cm-content').focus();
      await page.keyboard.press('Control+v');
      await poll(page, () => window.__tablePasteEvents.length > 0);
      const event = await page.evaluate(() => window.__tablePasteEvents.at(-1));
      assert.equal(event.trusted, true);
      return event;
    };
    const renderedSourceCells = () => page.evaluate(() => {
      const root = document.createElement('div');
      root.innerHTML = marked.parse(getEditContent(), { gfm: true });
      return [...root.querySelectorAll('table tr')].map(row => [...row.querySelectorAll('th,td')].map(cell => {
        cell.querySelectorAll('br').forEach(br => br.replaceWith(document.createTextNode('\n')));
        return cell.textContent;
      }));
    });
    const readerCells = () => page.evaluate(() => [...document.querySelectorAll('#content table tr')].map(row => [...row.querySelectorAll('th,td')].map(cell => {
      const copy = cell.cloneNode(true);
      copy.querySelectorAll('br').forEach(br => br.replaceWith(document.createTextNode('\n')));
      return copy.textContent;
    })));
    const reopenFile = async (file, source) => {
      const reopenResponse = page.waitForResponse(response => {
        const url = new URL(response.url());
        return url.pathname === '/api/file' && url.searchParams.get('p') === file && response.request().method() === 'GET';
      });
      await page.evaluate(async file => {
        if (state.editing) exitEdit();
        await loadFile(file, { force: true, discardConfirmed: true });
      }, file);
      const reopened = await reopenResponse;
      assert.equal(reopened.status(), 200);
      const payload = await reopened.json();
      assert.equal(payload.original ?? payload.content, source);
      assert.equal(await page.evaluate(() => state.original), source);
    };
    const saveAndReopen = async (file, source) => {
      const savedResponse = page.waitForResponse(response => response.url().endsWith('/api/save') && response.request().method() === 'POST');
      await page.locator('#edit-save').click();
      const saved = await savedResponse;
      assert.equal(saved.status(), 200);
      assert.equal((await saved.json()).ok, true);
      await poll(page, () => !getActiveTab().isDirty && getActiveTab().original === getEditContent());
      assert.equal(fs.readFileSync(file, 'utf8'), source);
      await reopenFile(file, source);
    };

    const original = '# Owned clipboard browser\r\n\r\n';
    const file = await openEditor('native-tsv', original);
    const tsv = '\tName\tNote\t\r\n001\t"two\tparts"\t"line 1\r\nline 2"\t\r\n002\t"He said ""yes"""\t"\\| <b> &amp; **x**"\t\r\n\t\t\t\r\n';
    const expected = [['', 'Name', 'Note', ''], ['001', 'two\tparts', 'line 1\nline 2', ''], ['002', 'He said "yes"', '\\| <b> &amp; **x**', ''], ['', '', '', '']];
    const tsvEvent = await nativeTextPaste(tsv);
    report.nativeTsvPaste = tsvEvent;
    assert.deepEqual(await renderedSourceCells(), expected);
    assert.equal(await page.evaluate(() => window.__tableDocumentTransactions.filter(transaction => transaction.paste).length), 1);
    const savedSource = await page.evaluate(() => getEditContent());
    assert.equal(/(^|[^\r])\n/.test(savedSource), false);
    await page.locator('#edit-undo').click();
    await poll(page, source => getEditContent() === source, original);
    assert.equal(await page.evaluate(() => getEditContent()), original);
    await page.locator('#edit-redo').click();
    await poll(page, source => getEditContent() === source, savedSource);
    assert.equal(await page.evaluate(() => getEditContent()), savedSource);
    await saveAndReopen(file, savedSource);
    assert.deepEqual(await readerCells(), expected);
    await page.screenshot({ path: path.join(args.evidence, 'native-tsv-reopened.png') });
    report.cases.push({ name: 'native TSV', nativePasteTrusted: tsvEvent.trusted, undoRedoViaToolbar: true, savedViaToolbar: true, reopenedViaLoadFile: true,
      savedSha256: sha(savedSource), cells: 16, crlf: true });

    const csvFile = await openEditor('explicit-csv', original);
    await page.evaluate(() => {
      const data = new DataTransfer();
      data.setData('text/plain', 'plain fallback');
      data.setData('text/csv', '\uFEFFName,Note,Extra\r\n"One, two","He said ""yes""\r\nnext",\r\n');
      cmView.contentDOM.dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }));
    });
    const csvExpected = [['Name', 'Note', 'Extra'], ['One, two', 'He said "yes"\nnext', '']];
    assert.deepEqual(await renderedSourceCells(), csvExpected);
    assert.equal(await page.evaluate(() => window.__tablePasteEvents.at(-1).trusted), false);
    const csvSource = await page.evaluate(() => getEditContent());
    await saveAndReopen(csvFile, csvSource);
    assert.deepEqual(await readerCells(), csvExpected);
    report.cases.push({ name: 'explicit CSV DOM event', nativePasteTrusted: false, savedViaToolbar: true, reopenedViaLoadFile: true, savedSha256: sha(csvSource) });

    const proseOriginal = '# Ordinary paste\n\n';
    await openEditor('comma-prose', proseOriginal);
    const prose = 'Hello, world\nGood, day';
    await nativeTextPaste(prose);
    assert.equal(await page.evaluate(() => getEditContent()), proseOriginal + prose);
    report.cases.push({ name: 'ordinary comma prose', nativePasteTrusted: true, rawTextRetained: true });

    await openEditor('code-destination', '```text\n\n```\n');
    await page.evaluate(() => { cmView.dispatch({ selection: { anchor: 8 } }); cmView.focus(); });
    await nativeTextPaste('A\tB\n1\t2');
    assert.equal(await page.evaluate(() => getEditContent()), '```text\nA\tB\n1\t2\n```\n');
    report.cases.push({ name: 'code destination', nativePasteTrusted: true, rawTextRetained: true });

    const imageFile = await openEditor('image-priority', '# Synthetic image\n\n');
    await page.evaluate(async () => {
      const canvas = document.createElement('canvas'); canvas.width = canvas.height = 1;
      const paint = canvas.getContext('2d'); paint.fillStyle = '#4370c0'; paint.fillRect(0, 0, 1, 1);
      const png = await new Promise(resolve => canvas.toBlob(resolve, 'image/png'));
      await navigator.clipboard.write([new ClipboardItem({ 'image/png': png, 'text/plain': new Blob(['A\tB\n1\t2'], { type: 'text/plain' }) })]);
      cmView.focus();
    });
    await page.keyboard.press('Control+v');
    await poll(page, () => /!\[.*\]\(/.test(getEditContent()) && getActiveTab().webAssets?.length === 1);
    assert.equal(await page.evaluate(() => window.__tablePasteEvents.at(-1).trusted), true);
    assert.equal(await page.evaluate(() => getEditContent().includes('| --- |')), false);
    const imageSave = page.waitForResponse(response => response.url().endsWith('/api/save') && response.request().method() === 'POST');
    await page.locator('#edit-save').click();
    assert.equal((await (await imageSave).json()).ok, true);
    await poll(page, () => !getActiveTab().isDirty);
    const imageSource = fs.readFileSync(imageFile, 'utf8');
    assert.match(imageSource, /readmd-assets\//);
    await reopenFile(imageFile, imageSource);
    await poll(page, () => { const image = document.querySelector('#content img'); return image?.complete && image.naturalWidth === 1; });
    report.cases.push({ name: 'native image precedes table text', nativePasteTrusted: true, actualPngSave: true, reopenedImageWidth: 1 });

    report.cspViolations = await page.evaluate(() => window.__tableCspViolations);
    assert.deepEqual(report.cspViolations, []);
    assert.deepEqual(report.pageErrors, []);
    report.passed = true;
  } catch (error) {
    report.passed = false;
    report.failure = { name: error.name, message: error.message.slice(0, 1600) };
    process.exitCode = 1;
  } finally {
    if (page) report.cspViolations = await page.evaluate(() => window.__tableCspViolations || []).catch(() => []);
    if (browser) await browser.close();
    if (kernel) await kernel.stop();
    fs.writeFileSync(path.join(args.evidence, 'report.json'), JSON.stringify(report, null, 2));
  }
  console.log(JSON.stringify({ passed: report.passed, cases: report.cases.length, loadedClipboardParser: report.loadedClipboardParser,
    report: path.join(args.evidence, 'report.json'), ...(report.failure ? { failure: report.failure } : {}) }));
}
if (require.main === module) run().catch(error => { console.error(error.message); process.exitCode = 1; });
