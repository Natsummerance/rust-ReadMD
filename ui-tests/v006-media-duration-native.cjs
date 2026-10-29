'use strict';
// The existing upload/open -> convert preview -> editor save flow in installed
// headless Edge, against a real prebuilt kernel and the unmodified product CSP.
// This is not native-window, range-selection or managed-engine certification.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const { chromium } = require('playwright');
const qa = require('../tools/test/v006-media-duration.cjs');

async function main() {
  const run = qa.ownedRun('browser'); let host, browser, page, phase = 'synthetic fixture';
  run.report.surface = 'headless Edge, production CSP and real installed Windows speech recognition';
  run.report.nativeWindow = false; run.report.pageErrors = []; run.report.requests = []; run.report.externalRequestCount = 0;
  try {
    const media = await qa.fixture(run); host = await qa.startHost(run);
    browser = await chromium.launch({ channel: 'msedge', headless: true });
    const context = await browser.newContext({ viewport: { width: 1160, height: 820 }, bypassCSP: false,
      extraHTTPHeaders: host.browserHeaders });
    await context.route('**/*', route => {
      const url = new URL(route.request().url());
      if (['http:', 'https:'].includes(url.protocol) && url.origin !== host.origin) {
        run.report.externalRequestCount++; return route.abort();
      }
      return route.continue();
    });
    page = await context.newPage(); page.on('pageerror', error => run.report.pageErrors.push(error.name));
    page.on('response', response => {
      const route = new URL(response.url()).pathname;
      if (['/api/upload', '/api/convert'].includes(route)) run.report.requests.push({ route, status: response.status() });
    });
    async function until(predicate, argument, timeout = 30000) {
      const deadline = Date.now() + timeout;
      while (!await page.evaluate(predicate, argument)) {
        assert(!run.report.requests.some(request => request.status >= 400), 'Existing upload/conversion request failed');
        assert(Date.now() < deadline, 'Frontend state deadline in ' + phase);
        await qa.delay(100);
      }
    }
    await page.goto(host.origin);
    await until(() => window.__readmdAppReady && window.i18n && typeof convertFile === 'function');
    const expectedCsp = fs.readFileSync(path.join(qa.ROOT, 'assets/index.html'), 'utf8').match(/<meta http-equiv="Content-Security-Policy" content="([^"]+)"/u)?.[1];
    assert(expectedCsp); assert.equal(await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content'), expectedCsp);
    assert(!/(?:^|;)\s*script-src[^;]*'unsafe-(?:inline|eval)'/u.test(expectedCsp));
    await page.evaluate(async () => { await i18n.setLanguage('en', false); localStorage.setItem('readmd.transcribe.language', 'en');
      if (typeof setEditorPref === 'function') setEditorPref('aiCompletion', false); });
    run.report.browserVersion = browser.version(); run.report.productionCspUnchanged = true;
    qa.record(run, 'The packaged frontend boots under its production CSP in installed headless Edge');
    phase = 'visible open control and actual transcription';
    // Trigger the existing visible Open control, then supply only our synthetic
    // fixture to its ordinary file input. No patched API or fake result.
    await page.locator('#btn-open').click();
    assert.equal(await page.evaluate(() => typeof document.querySelector('#file-input').onchange === 'function'), true, 'Browser Open action installs the real file input handler');
    await page.locator('#file-input').setInputFiles(media.source);
    await until(() => Boolean(getActiveTab()?.source === 'convert' && getActiveTab()?.content), undefined, 90000);
    const tab = await page.evaluate(() => { const tab = getActiveTab(); return { content: tab.content, path: tab.path,
      source: tab.source, isVirtual: tab.isVirtual, originPath: tab.originPath }; });
    qa.recognized(tab.content, false); assert.equal(tab.path, null); assert.equal(tab.isVirtual, true);
    run.report.observedDuration = qa.duration(tab.content);
    assert.equal(run.report.observedDuration, qa.timestamp(media.seconds), 'The existing media UI must show the full duration including trailing silence');
    assert(!fs.existsSync(path.join(run.documents, 'Synthetic-speech.md')));
    qa.record(run, 'A visible Open action uploads and transcribes real PCM into an unsaved preview with complete media duration');
    phase = 'editor save and real reopen';
    await page.locator('#btn-edit').click();
    await until(() => state.editing && Boolean(window.cmView));
    assert.equal(qa.duration(await page.evaluate(() => cmView.state.doc.toString())), qa.timestamp(media.seconds));
    assert((await page.locator('.cm-content').innerText()).includes(qa.timestamp(media.seconds)), 'Correct duration is visible in the existing editor');
    const downloadEvent = page.waitForEvent('download');
    await page.locator('#edit-save').click();
    const download = await downloadEvent;
    const saved = path.join(run.documents, 'Saved-transcript.md'); await download.saveAs(saved);
    assert.equal(qa.duration(fs.readFileSync(saved, 'utf8')), qa.timestamp(media.seconds));
    await page.evaluate(async file => { exitEdit(); await loadFile(file, { force: true }); }, saved);
    await until(file => getActiveTab()?.path === file, saved);
    const reopened = await page.evaluate(() => getActiveTab().content);
    assert.equal(qa.duration(reopened), qa.timestamp(media.seconds)); qa.recognized(reopened, false);
    assert.equal(qa.hash(fs.readFileSync(media.source)), media.sourceSha256);
    qa.record(run, 'The existing editor Save action downloads the real transcript and opening the saved file preserves duration and recognized content');
    assert.deepEqual(run.report.pageErrors, []);
    assert.equal(run.report.externalRequestCount, 0, 'The media flow requests no external browser resource');
    run.report.passed = true;
  } catch (error) {
    qa.failure(run, phase, error);
    if (page) run.report.frontendFailureState = await page.evaluate(() => ({ nativeBridge: hasPy,
      fileInputHandlerInstalled: typeof document.querySelector('#file-input').onchange === 'function',
      tabCount: state.tabs.length, toast: document.querySelector('#toast')?.textContent?.slice(0, 200) })).catch(() => null);
  }
  finally {
    if (browser) await browser.close().catch(() => {});
    if (host) { try { run.report.ownedHostExit = await host.stop(); } catch (error) { qa.failure(run, 'host cleanup', error); run.report.passed = false; } }
    qa.writeReport(run);
  }
}
main().catch(() => { console.error('Media browser validation could not start'); process.exitCode = 1; });
