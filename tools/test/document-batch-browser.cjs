'use strict';
// Production browser assets and authenticated HTTP. The test bridge supplies
// synthetic chooser results; it never opens a system picker or the user's app.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { createHash } = require('node:crypto');
const { options } = require('./v006-table-clipboard-kernel.cjs');
const { startKernel } = require('./v006-refactor-host.cjs');

async function main() {
  const args = options(process.argv.slice(2));
  assert(args.bin && !args.base, 'Specify a prebuilt kernel for both launches');
  const { chromium } = require(args.playwright || 'playwright');
  const profile = path.join(args.evidence, 'profile');
  const input = path.join(args.evidence, 'input');
  const destination = path.join(args.evidence, 'chosen-destination');
  for (const directory of [profile, input, destination]) fs.mkdirSync(directory);
  const first = path.join(input, '报告.txt');
  const second = path.join(input, '报告.csv');
  const empty = path.join(input, '待补充.txt');
  const later = path.join(input, 'later.txt');
  const sibling = path.join(input, '报告.md');
  fs.writeFileSync(first, '# Fixture title\n\nCompleted first output.\n');
  fs.writeFileSync(second, 'name,total\nalpha,3\nbeta,4\n');
  fs.writeFileSync(empty, '');
  fs.writeFileSync(later, 'Second batch independent of the first.');
  fs.writeFileSync(sibling, 'User-owned file beside the source.');
  const bytes = name => fs.readFileSync(name, 'utf8');
  const report = { passed: false, kernelPids: [], pageErrors: [], steps: [] };
  const digest = file => createHash('sha256').update(fs.readFileSync(file)).digest('hex');
  const root = path.resolve(__dirname, '../..');
  report.binSha256 = digest(args.bin);
  report.bootSha256 = digest(path.join(root, 'assets', 'readmd.boot.js'));
  let host, browser, page;

  async function launch() {
    host = await startKernel(args.bin, profile, input);
    report.kernelPids.push(host.pid);
    const base = new URL((await host.api('/')).url).origin;
    browser = await chromium.launch({ channel: 'chrome', headless: true, args: ['--disable-breakpad'] });
    page = await browser.newPage({ viewport: { width: 1200, height: 900 }, bypassCSP: false, serviceWorkers: 'block' });
    page.on('pageerror', error => report.pageErrors.push(error.message.slice(0, 200)));
    await page.route('**/*', route => new URL(route.request().url()).origin === base ? route.continue() : route.abort());
    await page.addInitScript(({ first, second, empty, destination }) => {
      window.testBatchSelection = [first, second, empty, first];
      window.pywebview = { api: {
        get_system_language: async () => 'en',
        get_app_info: async () => ({}),
        start_modules: async () => true,
        choose_many_files: async () => window.testBatchSelection,
        choose_folder: async () => destination,
      } };
      localStorage.setItem('readmd_language', 'en');
    }, { first, second, empty, destination });
    const boot = page.waitForResponse(response => new URL(response.url()).pathname === '/assets/readmd.boot.js');
    await page.goto(base);
    assert.equal(createHash('sha256').update(await (await boot).body()).digest('hex'), report.bootSha256);
    await page.waitForFunction(() => window.__readmdAppReady && typeof restoreBatchWorkbench === 'function');
    await page.evaluate(() => window.__readmdSessionReady);
    await page.locator('#tab-conversion').click();
    await page.locator('#btn-convert').click();
    await page.locator('#convert-modal').waitFor({ state: 'visible' });
    await page.waitForFunction(() => !isBatchRunning());
  }

  async function stop() {
    if (browser) { await browser.close(); browser = null; }
    if (host) { await host.stop(); host = null; }
  }

  async function latest() {
    const response = await host.api('/api/convert/progress?latest=1');
    assert.equal(response.status, 200);
    return response.json();
  }

  try {
    await launch();
    await page.locator('#convert-output-choose').click();
    await page.waitForFunction(dir => document.getElementById('convert-output-parent').textContent === dir, destination);
    await page.locator('#convert-files').click();
    await page.locator('.batch-item.error').waitFor();
    await page.locator('#batch-retry').waitFor();
    const result = await latest();
    assert.equal(result.total, 3);
    assert.deepEqual(result.items.map(item => item.status), ['ok', 'ok', 'error']);
    assert.equal(path.dirname(result.output_dir), fs.realpathSync(destination));
    assert.deepEqual(await page.locator('.batch-output-name').allTextContents(), [
      '001 - 报告.md', '002 - 报告.md', '003 - 待补充.md',
    ]);
    let openedDirectory;
    await page.route('**/api/system/open-path', route => {
      openedDirectory = route.request().postDataJSON().path;
      return route.fulfill({ json: { ok: true } });
    });
    const opening = page.waitForResponse(response => new URL(response.url()).pathname === '/api/system/open-path');
    await page.locator('#convert-open-dir').click();
    await opening;
    assert.equal(openedDirectory, result.output_dir);
    await page.unroute('**/api/system/open-path');
    const output = result.items[0].out;
    assert(bytes(output).includes('Completed first output.'));
    assert.equal(bytes(sibling), 'User-owned file beside the source.');
    fs.writeFileSync(output, '# Manually edited first result\n');
    report.steps.push('Selected folder, stable output names, alias deduplication, failure row and result folder');

    await page.evaluate(file => { window.testBatchSelection = [file]; }, later);
    await page.locator('#convert-output-reset').click();
    await page.locator('#convert-files').click();
    await page.locator('.batch-output-name', { hasText: '001 - later.md' }).waitFor();
    await page.waitForFunction(() => !isBatchRunning());
    const secondBatch = await latest();
    assert.notEqual(result.job, secondBatch.job);
    assert(secondBatch.output_dir.startsWith(fs.realpathSync(profile) + path.sep));
    await stop();
    await launch();
    assert.equal(await page.locator('.batch-item.ok').count(), 1);
    await page.locator('#batch-history').selectOption(result.job);
    await page.locator('#batch-retry').waitFor();
    assert.equal(await page.locator('.batch-item.ok').count(), 2);
    fs.writeFileSync(empty, 'Repaired source selected for retry.');
    await page.locator('#batch-retry').click();
    await page.waitForFunction(() => document.querySelectorAll('.batch-item.ok').length === 3 && !isBatchRunning());
    assert.equal(bytes(output), '# Manually edited first result\n');
    assert(bytes(path.join(result.output_dir, '003 - 待补充.md')).includes('Repaired source'));
    const row = page.locator('.batch-item.ok').first();
    await row.focus();
    await page.keyboard.press('Enter');
    await page.waitForFunction(() => document.getElementById('content').textContent.includes('Manually edited first result'));
    assert.equal(bytes(sibling), 'User-owned file beside the source.');
    assert.equal(fs.readdirSync(input).length, 5);
    assert.notEqual(report.kernelPids[0], report.kernelPids[1]);
    assert.deepEqual(report.pageErrors, []);
    report.steps.push('New process restores latest results and finds earlier batch, retry preserves completed bytes and opens result by keyboard');
    report.passed = true;
  } finally {
    await stop();
    fs.writeFileSync(path.join(args.evidence, 'report.json'), JSON.stringify(report, null, 2) + '\n');
  }
}

main().catch(error => { console.error(error); process.exitCode = 1; });
