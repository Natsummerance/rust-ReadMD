'use strict';
// Existing visible media workflow with its default formatting in an owned
// test profile. No profile switch, engine install or inferred audio offsets.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const { chromium } = require('playwright');
const qa = require('../tools/test/v006-media-duration.cjs');
const offsets = require('../tools/test/v006-speech-offsets.cjs');

function timestamps(content) {
  return [...content.matchAll(/\[(\d{2}:\d{2}(?::\d{2})?)\]/g)].map(match => ({ timestamp: match[1],
    seconds: match[1].split(':').map(Number).reduce((total, value) => total * 60 + value, 0) }));
}
function inWindows(rows, windows) {
  assert.equal(rows.length, 2, 'Both separated recognized phrases have visible timestamps');
  for (let index = 0; index < 2; index++) {
    const lower = Math.floor(windows[index].activeFrom / 16000) - 1, upper = Math.ceil(windows[index].activeTo / 16000);
    assert(rows[index].seconds >= lower && rows[index].seconds <= upper,
      'Visible phrase ' + index + ' timestamp ' + rows[index].timestamp + ' must fall in physical PCM window [' + lower + ',' + upper + ']');
  }
}
async function main() {
  process.env.READMD_V006_MEDIA_OUTPUT = process.env.READMD_V006_POSITION_OUTPUT || path.join(qa.ROOT, '.cache/capability-upgrade/v006-speech-offsets');
  const run = qa.ownedRun('browser'); let host, browser, page, phase = 'independent PCM fixture';
  Object.assign(run.report, { surface: 'headless Edge, production CSP and real Windows recognition', nativeWindow: false,
    pageErrors: [], externalRequestCount: 0, requests: [] });
  try {
    const media = await offsets.fixture(run); host = await qa.startHost(run);
    phase = 'production CSP and default formatting';
    browser = await chromium.launch({ channel: 'msedge', headless: true });
    const context = await browser.newContext({ viewport: { width: 1160, height: 820 }, bypassCSP: false, extraHTTPHeaders: host.browserHeaders });
    await context.route('**/*', route => {
      const url = new URL(route.request().url());
      if (['http:', 'https:'].includes(url.protocol) && url.origin !== host.origin) { run.report.externalRequestCount++; return route.abort(); }
      return route.continue();
    });
    page = await context.newPage(); page.on('pageerror', error => run.report.pageErrors.push(error.name));
    page.on('response', response => { const route = new URL(response.url()).pathname;
      if (['/api/upload', '/api/convert'].includes(route)) run.report.requests.push({ route, status: response.status() }); });
    async function until(predicate, argument, timeout = 30000) {
      const deadline = Date.now() + timeout;
      while (!await page.evaluate(predicate, argument)) {
        assert(!run.report.requests.some(request => request.status >= 400), 'Existing upload/conversion request failed');
        assert(Date.now() < deadline, 'Frontend deadline in ' + phase); await qa.delay(100);
      }
    }
    await page.goto(host.origin); await until(() => window.__readmdAppReady && window.i18n && typeof convertFile === 'function');
    const expectedCsp = fs.readFileSync(path.join(qa.ROOT, 'assets/index.html'), 'utf8').match(/<meta http-equiv="Content-Security-Policy" content="([^"]+)"/u)?.[1];
    assert(expectedCsp); assert.equal(await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content'), expectedCsp);
    assert(!/(?:^|;)\s*script-src[^;]*'unsafe-(?:inline|eval)'/u.test(expectedCsp));
    await page.evaluate(async () => { await i18n.setLanguage('en', false); localStorage.setItem('readmd.transcribe.language', 'en');
      if (typeof setEditorPref === 'function') setEditorPref('aiCompletion', false); });
    run.report.productionCspUnchanged = true; run.report.browserVersion = browser.version();
    qa.record(run, 'Existing default conversion formatting and production-CSP frontend');
    phase = 'visible Open and physical timestamp windows';
    await page.locator('#btn-open').click(); await page.locator('#file-input').setInputFiles(media.source);
    await until(() => Boolean(getActiveTab()?.source === 'convert' && getActiveTab()?.content), undefined, 90000);
    const preview = await page.evaluate(() => getActiveTab().content); qa.recognized(preview, false);
    run.report.visibleTimestamps = timestamps(preview); inWindows(run.report.visibleTimestamps, media.windows);
    assert.equal(qa.duration(preview), qa.timestamp(media.seconds));
    const assertTimestampEmphasis = async label => {
      await page.waitForFunction(rows => rows.every(row => document.querySelector('#content')?.textContent?.includes('[' + row.timestamp + ']')),
        run.report.visibleTimestamps, { timeout: 30000 });
      const actual = await page.locator('#content strong').allTextContents();
      for (const row of run.report.visibleTimestamps)
        assert(actual.includes('[' + row.timestamp + ']'), label + ': each timestamp renders as emphasis, not literal markup');
    };
    await assertTimestampEmphasis('conversion preview');
    qa.record(run, 'The real transcription preview renders each timestamp as emphasis without visible markup');
    await page.locator('#btn-edit').click(); await until(() => state.editing && Boolean(window.cmView));
    inWindows(timestamps(await page.evaluate(() => cmView.state.doc.toString())), media.windows);
    const visible = await page.locator('.cm-content').innerText();
    for (const row of run.report.visibleTimestamps) assert(visible.includes(row.timestamp), 'Each correct timestamp is visible in the existing editor');
    qa.record(run, 'Visible Open, actual transcription and editor show both phrases in their independently known PCM windows');
    phase = 'visible Save and physical file reopen';
    const pendingDownload = page.waitForEvent('download'); await page.locator('#edit-save').click();
    const download = await pendingDownload, saved = path.join(run.documents, 'Saved-offsets.md'); await download.saveAs(saved);
    inWindows(timestamps(fs.readFileSync(saved, 'utf8')), media.windows);
    await page.evaluate(async file => { exitEdit(); await loadFile(file, { force: true }); }, saved);
    await until(file => getActiveTab()?.path === file, saved);
    const reopened = await page.evaluate(() => getActiveTab().content);
    inWindows(timestamps(reopened), media.windows); assert.equal(qa.duration(reopened), qa.timestamp(media.seconds));
    await assertTimestampEmphasis('saved and reopened transcript');
    qa.record(run, 'The saved transcript renders each timestamp as emphasis after reopening');
    assert.equal(qa.hash(fs.readFileSync(media.source)), run.report.frameOracle.sourceSha256);
    qa.record(run, 'Visible Save downloads the real transcript; saved-file reopen preserves offsets, full duration and original media bytes');
    assert.deepEqual(run.report.pageErrors, []); assert.equal(run.report.externalRequestCount, 0);
    run.report.passed = true;
  } catch (error) { qa.failure(run, phase, error); }
  finally {
    if (browser) await browser.close().catch(() => {});
    if (host) { try { run.report.ownedHostExit = await host.stop(); } catch (error) { qa.failure(run, 'owned host cleanup', error); run.report.passed = false; } }
    qa.writeReport(run);
  }
}
main().catch(() => { console.error('Speech offset browser validation could not start'); process.exitCode = 1; });
