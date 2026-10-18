'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const {spawnSync} = require('node:child_process');
const root = path.resolve(__dirname, '../..');
const sha256 = value => crypto.createHash('sha256').update(value).digest('hex');
const fileHash = file => sha256(fs.readFileSync(file));
function failureScreenshotPath(workspace, data, privateDirectory) {
  if (privateDirectory === undefined) return path.join(data, 'failure.png');
  const privateRoot = path.join(workspace, '.cache', 'private');
  assert(typeof privateDirectory === 'string' && path.isAbsolute(privateDirectory), 'Private failure screenshots require an explicit absolute directory');
  const directory = path.resolve(privateDirectory), rel = path.relative(privateRoot, directory);
  assert(rel && !rel.startsWith('..') && !path.isAbsolute(rel), 'Private failure screenshot escaped private QA evidence');
  for (let current = directory; ; current = path.dirname(current)) {
    const stat = fs.lstatSync(current);
    assert(stat.isDirectory() && !stat.isSymbolicLink(), 'Failure screenshot directory has a reparse ancestor');
    if (path.relative(workspace, current) === '') break;
    assert(path.dirname(current) !== current, 'Failure screenshot directory escaped the workspace');
  }
  return path.join(directory, 'failure.png');
}
async function captureFailureScreenshot(page, workspace, data, privateDirectory, name = 'failure.png') {
  // Resolve policy before capturing; an invalid private destination must never
  // fall back to a public file. wx preserves any earlier failure evidence.
  assert(['failure.png', 'first-failure.png'].includes(name), 'Failure screenshot filename is not allowed');
  const file = path.join(path.dirname(failureScreenshotPath(workspace, data, privateDirectory)), name);
  const bytes = await page.screenshot();
  assert(Buffer.isBuffer(bytes), 'Failure screenshot did not return bytes');
  fs.writeFileSync(file, bytes, {flag: 'wx'});
  return {path: path.relative(workspace, file).replaceAll(path.sep, '/'), sha256: sha256(bytes), bytes: bytes.length, privateDerived: privateDirectory !== undefined};
}
async function stopAfterInventory(inventory, stop) {
  let inventoryError, stopError;
  try { await inventory(); } catch (error) { inventoryError = error; }
  try { await stop(); } catch (error) { stopError = error; }
  return {inventoryError, stopError};
}
function createHarness(name, port, script, nativeDataDirectory, {privateFailureScreenshotDirectory} = {}) {
  assert(process.env.READMD_BIN, 'READMD_BIN must identify root’s final prebuilt candidate');
  assert.match(process.env.READMD_CANDIDATE_SHA256 || '', /^[a-f0-9]{64}$/i, 'READMD_CANDIDATE_SHA256 is required; QA never selects its own candidate');
  assert.equal(fileHash(process.env.READMD_BIN), process.env.READMD_CANDIDATE_SHA256.toLowerCase(), 'Candidate byte hash differs from root’s supplied hash');
  const parent = path.join(root, '.cache', 'capability-upgrade'); fs.mkdirSync(parent, {recursive: true});
  const data = fs.mkdtempSync(path.join(parent, name + '-'));
  const nativeData = nativeDataDirectory === undefined ? data : path.resolve(nativeDataDirectory);
  if (nativeDataDirectory !== undefined) assert(path.isAbsolute(nativeDataDirectory) && fs.lstatSync(nativeData).isDirectory() && !fs.lstatSync(nativeData).isSymbolicLink(), 'An explicit native data directory must already be an absolute owned directory');
  const docs = path.join(data, 'documents'), outputs = path.join(data, 'outputs');
  fs.mkdirSync(docs); fs.mkdirSync(outputs);
  const report = {passed: false, platform: process.platform, nativeCsp: true, synthetic: true, noAutomaticRetries: true, candidateSha256: fileHash(process.env.READMD_BIN), scriptSha256: fileHash(script), checks: [], limitations: ['Independent Word and EPUB desktop applications are separate acceptance items; package/XML/EPUBCheck checks do not certify those applications.']};
  report.nativeDataDirectory = path.relative(root, nativeData).replaceAll(path.sep, '/');
  report.nativeDataFresh = nativeDataDirectory === undefined;
  report.failureScreenshotPolicy = {private: privateFailureScreenshotDirectory !== undefined,
    directory: path.relative(root, path.dirname(failureScreenshotPath(root, data, privateFailureScreenshotDirectory))).replaceAll(path.sep, '/')};
  report.qaDependencySha256 = Object.fromEntries(['tools/test/publication-batch3-fixtures.cjs', 'tools/test/publication-batch3-harness.cjs', 'tools/test/publication-batch3-checks.cjs', 'tools/test/publication-citation-oracle.cjs', 'tools/test/inspect-publication-batch3.py', 'ui-tests/native-session.cjs', 'tools/check-epub.mjs'].map(name => [name, fileHash(path.join(root, name))]));
  let native, page, stage = 'startup';
  const errors = [];
  const api = async (route, body) => {
    assert(page, 'Native page is unavailable');
    return page.evaluate(async ({route, body}) => {
      const response = await apiFetch(route, {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(body), signal: AbortSignal.timeout(120000)});
      return {status: response.status, data: await response.json()};
    }, {route, body});
  };
  async function start() {
    const {chromium} = require('playwright');
    const {startNative} = require('../../ui-tests/native-session.cjs');
    process.env.READMD_WORKSPACE = docs;
    native = await startNative(root, nativeData, chromium, port); page = native.page;
    page.on('pageerror', error => errors.push(error.message));
    await page.evaluate(() => window.__readmdSessionReady);
    const assets = process.env.READMD_ASSETS_DIR || path.join(root, 'assets');
    const boot = fileHash(path.join(assets, 'readmd.boot.js'));
    if (report.bootSha256) assert.equal(boot, report.bootSha256, 'Assets changed during the same candidate QA run');
    report.bootSha256 = boot;
    const served = await page.request.get(new URL('/assets/readmd.boot.js', page.url()).href);
    report.servedBootSha256 = sha256(await served.body());
    assert.equal(report.servedBootSha256, boot, 'Native host served a different boot bundle');
    return page;
  }
  async function stop() {
    if (!native) return;
    const current = native; native = undefined; page = undefined;
    await current.browser.close().catch(() => {});
    const child = current.server.child;
    if (child.exitCode === null && child.signalCode === null) {
      const exited = new Promise(resolve => child.once('exit', resolve)); child.kill();
      await Promise.race([exited, new Promise(resolve => setTimeout(resolve, 3000))]);
    }
    assert(child.exitCode !== null || child.signalCode !== null, 'Owned native process failed to stop');
  }
  async function step(label, callback) {
    stage = label;
    try { const detail = await callback(); report.checks.push({name: label, passed: true, ...(detail === undefined ? {} : {detail})}); return detail; }
    catch (error) { report.checks.push({name: label, passed: false}); throw error; }
  }
  async function until(check, timeout = 60000) {
    const deadline = Date.now() + timeout;
    while (!await check()) { assert(Date.now() < deadline, 'Timed out: ' + stage); await new Promise(resolve => setTimeout(resolve, 80)); }
  }
  function command(executable, args, timeout = 90000) {
    const result = spawnSync(executable, args, {encoding: 'utf8', windowsHide: true, timeout, maxBuffer: 32 * 1024 * 1024});
    assert.equal(result.status, 0, `${executable} failed: ${String(result.error || result.stderr).slice(0, 1200)}`);
    return result.stdout;
  }
  async function installPacks() {
    for (const [id, variable] of [['fonts', 'READMD_FONTS_PACK'], ['pdfium', 'READMD_PDFIUM_PACK']]) {
      const pack = process.env[variable] || (id === 'pdfium' ? process.env.READMD_PDF_PACK : undefined);
      assert(pack && fs.existsSync(pack), `${variable} must identify the existing locked offline pack`);
      const result = await api('/api/capabilities/packs', {action: 'install', id, path: path.resolve(pack)});
      assert.equal(result.data.ok, true, JSON.stringify(result.data));
      report[id + 'PackSha256'] = fileHash(pack);
    }
  }
  function snapshotInputs() { return Object.fromEntries(fs.readdirSync(docs).sort().map(name => [name, fileHash(path.join(docs, name))])); }
  function unchanged(expected) { assert.deepEqual(snapshotInputs(), expected, 'Read-only publication/export polluted or changed source inputs'); }
  async function finish(work) {
    try { await work(); assert.deepEqual(errors, [], 'Uncaught production page errors'); report.passed = true; }
    catch (error) {
      report.failedStage = stage;
      report.error = String(error.stack || error.message).replaceAll(data, '<owned-profile>').replaceAll(root, '<workspace>').slice(0, 3200);
      if (page) {
        try { report.failureScreenshot = await captureFailureScreenshot(page, root, data, privateFailureScreenshotDirectory); }
        catch (captureError) { report.failureScreenshotError = {name: captureError.name || 'Error', code: captureError.code || null}; }
      }
      process.exitCode = 1;
    } finally {
      try { await stop(); } catch (error) { report.passed = false; report.shutdownError = error.message; process.exitCode = 1; }
      report.recordedAt = new Date().toISOString();
      fs.writeFileSync(path.join(data, 'report.json'), JSON.stringify(report, null, 2) + '\n');
      console.log(JSON.stringify({passed: report.passed, checks: report.checks.length, report: path.relative(root, path.join(data, 'report.json')), failedStage: report.failedStage}));
    }
  }
  return {root, data, nativeData, docs, outputs, report, get page() {return page;}, start, stop, api, step, until, command, installPacks, snapshotInputs, unchanged, finish};
}
module.exports = {createHarness, sha256, fileHash, root, failureScreenshotPath, captureFailureScreenshot, stopAfterInventory};
