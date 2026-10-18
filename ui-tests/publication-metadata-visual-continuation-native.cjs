'use strict';
// One explicitly authorized continuation of the final visual stage. Existing
// exports are verified, never regenerated; the original failed report remains.
const fs = require('node:fs'), path = require('node:path'), net = require('node:net'), assert = require('node:assert/strict');
const {fileHash, sha256, root} = require('../tools/test/publication-batch3-harness.cjs');
const {paperMetadata, bookMetadata, sourceFor, paperBody, bookBody, references} = require('../tools/test/publication-batch3-fixtures.cjs');
const {startNative} = require('./native-session.cjs');
const data = path.resolve(process.env.READMD_METADATA_CONTINUATION_PROFILE || 'missing-owned-profile');
assert.equal(path.dirname(data), path.join(root, '.cache/capability-upgrade'));
assert.match(path.basename(data), /^publication-metadata-native-[A-Za-z0-9]+$/);
const reportPath = path.join(data, 'completion-cspfixed-report.json');
assert(!fs.existsSync(reportPath), 'This continuation has already been attempted; its report must not be overwritten');
const stageName = 'light/dark reference sizes and Arabic structured-property controls';
const report = {passed: false, composite: true, singleRunFullSuitePassed: false,
  scope: 'same-candidate 15 prior successful + 1 corrected-QA continuation',
  noAutomaticRetries: true, nativeCsp: true, profile: path.basename(data), scriptSha256: fileHash(__filename),
  checks: [], screenshots: [], restart: {}, limitations: ['Independent Word and EPUB desktop applications remain unverified.']};
let native, page, originals, savedInputs, outputs, savedPaper;
const docs = path.join(data, 'documents'), port = 29123, errors = [];
const snapshot = directory => Object.fromEntries(fs.readdirSync(directory).sort().map(name => [name, fileHash(path.join(directory, name))]));
async function until(check) {
  const deadline = Date.now() + 30000;
  while (!await check()) {
    assert(Date.now() < deadline, 'Timed out waiting for the real native theme transition');
    await new Promise(resolve => setTimeout(resolve, 80));
  }
}
function verifyUnchanged() {
  assert.deepEqual(snapshot(docs), savedInputs, 'Visual continuation changed a source document');
  assert.deepEqual(snapshot(path.join(data, 'outputs')), outputs, 'Visual continuation changed an accepted output');
  for (const [name, digest] of Object.entries(originals)) assert.equal(fileHash(path.join(data, name)), digest, 'Prior evidence changed: ' + name);
}
async function listening() {
  return new Promise(resolve => {
    const socket = net.createConnection({host: '127.0.0.1', port});
    const finish = value => {socket.destroy(); resolve(value);};
    socket.once('connect', () => finish(true)); socket.once('error', () => finish(false));
    socket.setTimeout(1000, () => finish(false));
  });
}
async function stopOwned() {
  if (!native) return;
  const current = native; native = undefined;
  await current.browser.close().catch(error => {report.restart.browserCloseError = error.message;});
  const child = current.server.child;
  if (child.exitCode === null && child.signalCode === null) {
    const exited = new Promise(resolve => child.once('exit', resolve)); child.kill();
    await Promise.race([exited, new Promise(resolve => setTimeout(resolve, 3000))]);
  }
  assert(child.exitCode !== null || child.signalCode !== null, 'Owned continuation process failed to stop');
  report.restart.stoppedAt = new Date().toISOString();
  report.restart.exitCode = child.exitCode; report.restart.signalCode = child.signalCode;
  report.restart.portClosedAfterStop = !await listening();
  assert.equal(report.restart.portClosedAfterStop, true);
}
async function run() {
  try {
    const original = path.join(data, 'report.json');
    assert.match(process.env.READMD_METADATA_PRIOR_REPORT_SHA256 || '', /^[a-f0-9]{64}$/i);
    assert.equal(fileHash(original), process.env.READMD_METADATA_PRIOR_REPORT_SHA256.toLowerCase());
    const prior = JSON.parse(fs.readFileSync(original, 'utf8'));
    assert.equal(prior.passed, false); assert.equal(prior.checks.length, 16);
    assert(prior.checks.slice(0, 15).every(check => check.passed));
    assert.equal(prior.checks[15].name, stageName); assert.equal(prior.checks[15].passed, false);
    assert.equal(prior.shutdownError, undefined);
    assert.equal(fileHash(path.join(data, 'qa-script.cjs')), prior.scriptSha256);
    assert.equal(prior.candidateSha256, process.env.READMD_CANDIDATE_SHA256);
    assert.equal(fileHash(process.env.READMD_BIN), prior.candidateSha256);
    assert.equal(prior.bootSha256, process.env.READMD_CANDIDATE_BOOT_SHA256);
    assert.equal(fileHash(path.join(process.env.READMD_ASSETS_DIR || path.join(root, 'assets'), 'readmd.boot.js')), prior.bootSha256);
    report.binarySha256 = prior.candidateSha256; report.bootSha256 = prior.bootSha256;
    report.priorExecution = {report: 'report.json', sha256: fileHash(original), scriptSha256: prior.scriptSha256,
      passed: false, passedCount: 15, failed: 1, failedStage: prior.failedStage, failureScreenshot: 'failure.png'};
    const priorContinuation = JSON.parse(fs.readFileSync(path.join(data, 'completion-report.json'), 'utf8'));
    assert.equal(priorContinuation.passed, false);
    assert.equal(fileHash(path.join(data, 'qa-visual-continuation-script.cjs')), priorContinuation.scriptSha256);
    report.priorContinuation = {report: 'completion-report.json', sha256: fileHash(path.join(data, 'completion-report.json')),
      script: 'qa-visual-continuation-script.cjs', scriptSha256: priorContinuation.scriptSha256,
      passed: false, failureScreenshot: 'continuation-failure.png', reason: 'QA waitForFunction used eval blocked by the preserved production CSP'};
    originals = Object.fromEntries(['report.json', 'metadata-field-matrix.json', 'epubcheck.json', 'qa-script.cjs',
      'qa-metadata-field-matrix.cjs', 'missing-glyph-rejections.json', 'failure.png', 'metadata-1024-en-light.png',
      'completion-report.json', 'qa-visual-continuation-script.cjs', 'continuation-failure.png',
      'continuation-runner.log', 'continuation-invocation.json', 'continuation-native-host.log']
      .map(name => [name, fileHash(path.join(data, name))]));
    const paper = sourceFor(paperMetadata, paperBody, {bom: true, crlf: true});
    const originalSources = {'Paper.md': paper, 'Book.md': sourceFor(bookMetadata, bookBody), 'references.json': JSON.stringify(references)};
    assert.deepEqual(prior.inputs, Object.fromEntries(Object.entries(originalSources).map(([name, text]) => [name, sha256(text)])));
    savedPaper = paper.replace('name: "Nguyen, Linh"', 'name: "Nguyen, Linh revised"');
    savedInputs = Object.fromEntries(Object.entries({...originalSources, 'Paper.md': savedPaper}).map(([name, text]) => [name, sha256(text)]));
    assert.deepEqual(snapshot(docs), savedInputs);
    report.originalInputHashes = prior.inputs; report.savedInputHashes = savedInputs;
    outputs = Object.fromEntries(prior.outputs.map(output => {assert.equal(output.validated, true); return [output.file, output.sha256];}));
    assert.equal(Object.keys(outputs).length, 121); assert.deepEqual(snapshot(path.join(data, 'outputs')), outputs);
    const matrix = JSON.parse(fs.readFileSync(path.join(data, 'metadata-field-matrix.json'), 'utf8'));
    assert.equal(matrix.passed, true); assert.equal(matrix.passedCaseCount, 35);
    assert.equal(matrix.counts.validated, 111); assert.equal(matrix.counts.failed, 0);
    assert.equal(prior.fieldMatrix.sha256, originals['metadata-field-matrix.json']);
    for (const baseline of Object.values(matrix.baselines)) assert.equal(fileHash(path.join(data, baseline.input)), baseline.inputSha256);
    const epub = JSON.parse(fs.readFileSync(path.join(data, 'epubcheck.json'), 'utf8'));
    assert.equal(epub.passed, true); assert.equal(epub.files.length, 40);
    for (const file of epub.files) {assert.equal(file.passed, true); assert.deepEqual(file.counts, {fatal: 0, error: 0, warning: 0}); assert.equal(outputs[file.name], file.sha256);}
    report.fieldMatrix = {path: 'metadata-field-matrix.json', sha256: originals['metadata-field-matrix.json'], cases: 35, outputs: 111, passed: true};
    report.epubcheck = {path: 'epubcheck.json', sha256: originals['epubcheck.json'], count: 40, passed: true};
    report.checks = prior.checks.slice(0, 15).map((check, index) => ({name: check.name, passed: true, execution: 'prior-successful-stage',
      evidence: {report: 'report.json', sha256: originals['report.json'], scriptSha256: prior.scriptSha256, checkIndex: index + 1}}));
    fs.copyFileSync(__filename, path.join(data, 'qa-visual-cspfixed-continuation-script.cjs'));
    report.restart.priorHostPortClosed = !await listening(); assert.equal(report.restart.priorHostPortClosed, true);
    process.env.READMD_WORKSPACE = docs;
    process.env.READMD_NATIVE_LOG_FILE = path.join(data, 'continuation2-native-host.log');
    const {chromium} = require('playwright');
    report.restart.startedAt = new Date().toISOString();
    native = await startNative(root, data, chromium, port); page = native.page;
    report.restart.ownedChildPid = native.server.child.pid;
    page.on('pageerror', error => errors.push(error.message));
    await page.evaluate(() => window.__readmdSessionReady);
    const served = await page.request.get(new URL('/assets/readmd.boot.js', page.url()).href);
    report.servedBootSha256 = sha256(await served.body()); assert.equal(report.servedBootSha256, report.bootSha256);
    const csp = await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content');
    const scripts = csp.split(';').find(value => value.trim().startsWith('script-src'));
    assert(scripts.includes("'self'") && !scripts.includes("'unsafe-inline'") && !scripts.includes("'unsafe-eval'"));
    const expectedText = savedPaper.replace(/^\uFEFF/, '');
    await page.evaluate(async ({file, expected}) => {
      if (state.editing) {if (getEditContent() !== expected) throw Error('Restored editor differs from the accepted saved source'); await exitEdit();}
      await loadFile(file);
    }, {file: path.join(docs, 'Paper.md'), expected: expectedText});
    assert.equal(await page.evaluate(() => state.original), expectedText);
    await page.evaluate(() => toggleEdit());
    for (const [width, height, language, theme] of [[1024, 680, 'en', 'light'], [1160, 820, 'de', 'dark'], [1024, 680, 'ar', 'dark']]) {
      await page.setViewportSize({width, height});
      const transitions = [];
      for (let count = 0; count < 4 && await page.locator('body').getAttribute('data-theme') !== theme; count++) {
        const before = await page.evaluate(() => state.theme);
        await page.locator('#btn-theme').click();
        await until(() => page.evaluate(choice => state.theme !== choice, before));
        await until(() => page.evaluate(() => !document.documentElement.classList.contains('rm-theme-vt')));
        transitions.push({before, after: await page.evaluate(() => state.theme)});
      }
      assert.equal(await page.locator('body').getAttribute('data-theme'), theme);
      await page.evaluate(language => i18n.setLanguage(language, false), language);
      await page.evaluate(() => openFrontmatterModal());
      await page.locator('#frontmatter-modal').waitFor({state: 'visible'});
      const author = page.locator('#publication-metadata-editor [data-pm-field="authors"] [data-pm-row]').first();
      await author.waitFor({state: 'attached'});
      assert.equal(await author.locator('[data-pm-key="name"]').inputValue(), 'Nguyen, Linh revised');
      assert.equal(await page.locator('#frontmatter-modal .modal-dialog').evaluate(element => element.scrollWidth > element.clientWidth + 2), false);
      assert.equal(await page.locator('#fm-modal-insert').evaluate(element => {const box = element.getBoundingClientRect(); return box.top >= 0 && box.bottom <= innerHeight;}), true);
      const name = `continuation2-metadata-${width}-${language}-${theme}.png`;
      await page.screenshot({path: path.join(data, name)});
      report.screenshots.push({file: name, sha256: fileHash(path.join(data, name)), width, height, language, theme, transitions});
      await page.locator('#fm-modal-cancel').click();
    }
    assert.equal(await page.evaluate(() => getEditContent()), expectedText); verifyUnchanged();
    assert.deepEqual(errors, []);
    report.checks.push({name: stageName, passed: true, execution: 'corrected-QA-continuation',
      evidence: {script: 'qa-visual-cspfixed-continuation-script.cjs', sha256: report.scriptSha256, screenshots: report.screenshots, sourceInputHashes: savedInputs}});
    report.passed = true;
  } catch (error) {
    report.error = String(error.stack || error).replaceAll(data, '<owned-profile>').replaceAll(root, '<workspace>');
    if (page) await page.screenshot({path: path.join(data, 'continuation2-failure.png')}).catch(() => {});
    if (!report.checks.some(check => check.execution === 'corrected-QA-continuation')) report.checks.push({name: stageName, passed: false, execution: 'corrected-QA-continuation'});
    process.exitCode = 1;
  } finally {
    try {await stopOwned(); if (originals && savedInputs && outputs) verifyUnchanged();}
    catch (error) {report.passed = false; report.shutdownOrIntegrityError = error.message; process.exitCode = 1;}
    try {report.originalEvidenceUnchanged = Boolean(originals) && Object.entries(originals).every(([name, digest]) => fileHash(path.join(data, name)) === digest);}
    catch (error) {report.originalEvidenceUnchanged = false; report.passed = false; report.integrityError = error.message; process.exitCode = 1;}
    report.passedCount = report.checks.filter(check => check.passed).length;
    report.failed = report.checks.filter(check => !check.passed).length;
    report.skipped = 16 - report.checks.length; report.flaky = 0;
    report.recordedAt = new Date().toISOString();
    fs.writeFileSync(reportPath, JSON.stringify(report, null, 2) + '\n');
    console.log(JSON.stringify({passed: report.passed, scope: report.scope, passedCount: report.passedCount, failed: report.failed, report: reportPath}));
  }
}
run().catch(error => {console.error(error); process.exitCode = 1;});
