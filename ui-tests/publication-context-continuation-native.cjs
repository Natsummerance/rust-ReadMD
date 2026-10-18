'use strict';
// Explicit, once-only stages 11–14 continuation of the retained DyUpQm run.
// Product and original reports/artifacts remain frozen; original failures stay.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {spawnSync} = require('node:child_process');
const {root, fileHash, sha256} = require('../tools/test/publication-batch3-harness.cjs');
const {inspectArtifact, checkRichArtifact, checkSpans} = require('../tools/test/publication-batch3-checks.cjs');
const {verifySameCandidateOracle} = require('../tools/test/publication-context-oracle-evidence.cjs');
const profileName = '.cache/capability-upgrade/publication-context-native-DyUpQm';
const data = path.join(root, profileName), docs = path.join(data, 'documents'), outputs = path.join(data, 'outputs');
const originalReportFile = path.join(data, 'report.json'), completionFile = path.join(data, 'completion-report.json');
assert(!fs.existsSync(completionFile), 'The explicitly authorized continuation already has a report; do not retry it');
assert.equal(fileHash(originalReportFile), 'd2ee9a3d6a8eb26520f20dd8c6fabce6f639a5fa30e571683c671073ea5d7ea4');
const original = JSON.parse(fs.readFileSync(originalReportFile, 'utf8'));
const report = {passed: false, kind: 'explicit stages 11–14 continuation with composite provenance', noAutomaticRetries: true,
  platform: process.platform, nativeCsp: true, candidateSha256: fileHash(process.env.READMD_BIN), scriptSha256: fileHash(__filename), checks: [], outputs: [],
  limitations: ['Original theme and merged-space underline checker failures remain unchanged; this continuation is a QA contract correction, not a product fix.',
    'Independent Word application open/save and independent EPUB desktop-reader acceptance remain separate and unobtained.',
    'Only the 20 mandatory independent oracle cases pass. One optional citeproc-js cs:intext case remains unsupported.']};
report.qaDependencySha256 = Object.fromEntries(Object.keys(original.qaDependencySha256).map(name => [name, fileHash(path.join(root, name))]));
report.continuationHelperSha256 = fileHash(path.join(root, 'tools/test/publication-context-oracle-evidence.cjs'));
report.compositeProvenance = {originalReport: path.relative(root, originalReportFile), originalReportSha256: fileHash(originalReportFile),
  originalScriptSha256: original.scriptSha256, originalOverallPassed: original.passed, originalFailedStage: original.failedStage,
  inheritedStages: original.checks.slice(0, 10).map((check, index) => ({stage: index + 1, ...check})), actualStages: [11, 12, 13, 14],
  sourceHashesAtContinuationStart: original.inputs, qaOnlyCorrection: 'Underline checks use actual UNDERLINE letter glyph bounds and the original horizontal/vertical tolerances; rich PDF inspection explicitly enables --glyphs.'};
const originalFiles = ['report.json', 'Rich.docx.inspection.json', 'Rich.pdf.inspection.json', 'Rich.pdf.glyph-inspection.json', 'outputs/Rich.docx', 'outputs/Rich.pdf',
  'citation-1024-light.png', 'citation-1160-dark.png'];
const preserved = Object.fromEntries(originalFiles.map(name => [name, fileHash(path.join(data, name))]));
const snapshotInputs = () => Object.fromEntries(fs.readdirSync(docs).sort().map(name => [name, fileHash(path.join(docs, name))]));
const initial = {...original.inputs}, errors = [];
const source = fs.readFileSync(path.join(docs, 'Context.md'), 'utf8');
const richSource = fs.readFileSync(path.join(docs, 'Rich.md'), 'utf8'), noteSource = fs.readFileSync(path.join(docs, 'Author-notes.md'), 'utf8');
const nativeText = content => content.replace(/^\uFEFF/, '');
const options = {pdf: {engine: 'rust'}, typography: {font: 'Arial', size: 11}, cover: {enabled: false}, toc: {enabled: false}, footer: {pageNumbers: false}};
let native, page, stage = 'continuation preconditions';
const h = {root, data, docs, outputs, report,
  command(executable, args, timeout = 90000) {
    const result = spawnSync(executable, args, {encoding: 'utf8', windowsHide: true, timeout, maxBuffer: 32 * 1024 * 1024});
    assert.equal(result.status, 0, `${executable} failed: ${String(result.error || result.stderr).slice(0, 1200)}`); return result.stdout;
  },
};
function unchanged() { assert.deepEqual(snapshotInputs(), initial, 'Continuation changed an unauthorized source input'); }
async function until(check, timeout = 60000) {
  const deadline = Date.now() + timeout;
  while (!await check()) { assert(Date.now() < deadline, 'Timed out: ' + stage); await new Promise(resolve => setTimeout(resolve, 80)); }
}
async function api(route, body) {
  return page.evaluate(async ({route, body}) => {
    const response = await apiFetch(route, {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(body), signal: AbortSignal.timeout(120000)});
    return {status: response.status, data: await response.json()};
  }, {route, body});
}
async function start() {
  const {chromium} = require('playwright'), {startNative} = require('./native-session.cjs');
  process.env.READMD_WORKSPACE = docs;
  native = await startNative(root, data, chromium, 29126); page = native.page;
  page.on('pageerror', error => errors.push(error.message));
  await page.evaluate(() => window.__readmdSessionReady);
  const boot = fileHash(path.join(process.env.READMD_ASSETS_DIR || path.join(root, 'assets'), 'readmd.boot.js'));
  assert.equal(boot, process.env.READMD_CANDIDATE_BOOT_SHA256); assert.equal(boot, original.bootSha256);
  const served = await page.request.get(new URL('/assets/readmd.boot.js', page.url()).href);
  const servedHash = sha256(await served.body()); assert.equal(servedHash, boot);
  report.bootSha256 = boot; report.servedBootSha256 = servedHash;
  const csp = await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content');
  const scripts = csp.split(';').find(value => value.trim().startsWith('script-src'));
  assert(scripts?.includes("'self'") && !scripts.includes("'unsafe-inline'") && !scripts.includes("'unsafe-eval'"));
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
  assert(child.exitCode !== null || child.signalCode !== null, 'Owned continuation native process failed to stop');
}
async function step(number, name, work) {
  stage = name; console.log(JSON.stringify({stage: number, name}));
  try {await work(); report.checks.push({stage: number, name, passed: true});}
  catch (error) {report.checks.push({stage: number, name, passed: false}); throw error;}
}
function projection(value) {assert(value.projection && Array.isArray(value.projection.citations)); return value.projection;}
function recordOutput(file) {report.outputs.push({file: path.basename(file), sha256: fileHash(file), bytes: fs.statSync(file).size});}

async function run() {
  try {
    assert.equal(process.platform, 'win32'); assert.equal(report.candidateSha256, process.env.READMD_CANDIDATE_SHA256);
    assert.equal(report.candidateSha256, original.candidateSha256); assert.equal(original.checks.length, 11);
    assert.equal(original.passed, false); assert(original.checks.slice(0, 10).every(check => check.passed)); assert.equal(original.checks[10].passed, false);
    unchanged();
    for (const name of ['Continuation-Rich.docx', 'Continuation-Rich.pdf', 'Continuation-Rich.epub', 'Continuation-Author-notes.docx']) assert(!fs.existsSync(path.join(outputs, name)));
    // Recheck the retained original PDF without replacing either old inspection.
    checkRichArtifact(JSON.parse(fs.readFileSync(path.join(data, 'Rich.pdf.glyph-inspection.json'), 'utf8').replace(/^\uFEFF/, '')));
    report.originalPdfReadOnlyRecheck = {file: 'outputs/Rich.pdf', sha256: preserved['outputs/Rich.pdf'], inspection: 'Rich.pdf.glyph-inspection.json',
      inspectionSha256: preserved['Rich.pdf.glyph-inspection.json'], passed: true, productExecuted: false};
    await start();
    // Validate reuse before any dependent work, then record it as stage 14.
    verifySameCandidateOracle(report);
    await step(11, 'actual rich DOCX/PDF/EPUB, exact physical underline glyphs, author notes and EPUBCheck', async () => {
      const produced = [];
      for (const format of ['docx', 'pdf', 'epub']) {
        const file = path.join(outputs, 'Continuation-Rich.' + format);
        const result = await api('/api/export', {format, content: richSource, baseDir: docs, out_path: file, options});
        assert.equal(result.status, 200, JSON.stringify(result.data)); assert.equal(result.data.ok, true, JSON.stringify(result.data));
        checkRichArtifact(inspectArtifact(h, file, format === 'pdf' ? {glyphs: true, render: path.join(data, 'continuation-rich-pdf-pages')} : {}));
        recordOutput(file); produced.push(file);
      }
      const noteFile = path.join(outputs, 'Continuation-Author-notes.docx');
      const result = await api('/api/export', {format: 'docx', content: noteSource, baseDir: docs, out_path: noteFile, options});
      assert.equal(result.status, 200, JSON.stringify(result.data)); assert.equal(result.data.ok, true, JSON.stringify(result.data));
      const notes = inspectArtifact(h, noteFile); assert.equal(notes.nestedNoteReferences.length, 0);
      assert(notes.notes.some(note => note.text.includes('Citation inside the author note')));
      assert(notes.notes.filter(note => Number(note.id) > 0).length >= 4, 'Real note references, including narrative prose, were dropped');
      const narrative = notes.paragraphs.find(text => text.startsWith('Narrative '));
      assert(narrative?.includes('Example'), 'Note-style narrative prose lost the inline author');
      assert(!narrative.includes('10'), 'Note-style narrative locator leaked into the prose instead of its note');
      assert(notes.notes.some(note => note.text.includes('10')), 'Narrative locator did not reach a real note'); recordOutput(noteFile);
      const jar = process.env.READMD_EPUBCHECK_JAR; assert(jar && fs.existsSync(jar));
      const {checkEpub} = await import('../tools/check-epub.mjs');
      const validation = await checkEpub(produced.filter(file => file.endsWith('.epub')), {jar, output: path.join(data, 'continuation-epubcheck.json')});
      assert.equal(validation.passed, true); report.epubcheck = validation; unchanged();
    });
    await step(12, 'forged/replayed/duplicated source nonce markers cannot replace the new completed output', async () => {
      const file = path.join(outputs, 'Continuation-Rich.docx'), digest = fileHash(file);
      const prepared = await api('/api/academic/document', {content: richSource, baseDir: docs, options: {}, view: true});
      assert.equal(prepared.status, 200); const nonce = projection(prepared.data).nonce; assert.match(nonce, /^[a-f0-9]{32}$/);
      for (const marker of ['<readmd-csl data-slot="00000000000000000000000000000000 c0"/>',
        `<readmd-csl data-slot="${nonce} c0"/>`, `<readmd-csl data-slot="${nonce} c0"/><readmd-csl data-slot="${nonce} c0"/>`]) {
        const result = await api('/api/export', {format: 'docx', content: richSource + '\n' + marker + '\n', baseDir: docs, out_path: file, options});
        assert.equal(result.data.ok, false, 'A forged source marker was treated as an authorized typed slot');
        assert.match(result.data.error_code || '', /^academic_(?:slot|source|projection)/); assert.equal(fileHash(file), digest);
      }
      unchanged();
    });
    await step(13, 'native original citation edit, keyboard undo/redo, Save/reopen and real host restart', async () => {
      const file = path.join(docs, 'Context.md');
      await page.evaluate(async file => {await exitEdit(); await loadFile(file); await toggleEdit(); setEditorPref('aiCompletion', false);}, file);
      assert.equal(await page.evaluate(() => getEditContent()), nativeText(source));
      assert.equal(source.split('[-@study, p. 42]').length, 2);
      const edited = source.replace('[-@study, p. 42]', '[-@study, p. 44]');
      await page.evaluate(() => {const at = cmView.state.doc.toString().indexOf('[-@study, p. 42]') + '[-@study, p. '.length;
        cmView.dispatch({selection: {anchor: at, head: at + 2}}); cmView.focus();});
      await page.keyboard.type('44'); assert.equal(await page.evaluate(() => getEditContent()), nativeText(edited));
      await page.keyboard.press('Control+z'); assert.equal(await page.evaluate(() => getEditContent()), nativeText(source));
      await page.keyboard.press('Control+y'); assert.equal(await page.evaluate(() => getEditContent()), nativeText(edited));
      assert.equal(fs.readFileSync(file, 'utf8'), source);
      await page.locator('#edit-save').click(); await until(() => Promise.resolve(fs.readFileSync(file, 'utf8') === edited));
      initial['Context.md'] = fileHash(file); report.intentionalSourceEdit = {file: 'Context.md', beforeSha256: original.inputs['Context.md'], afterSha256: initial['Context.md'],
        change: 'One suppress-author locator 42 → 44 by native keyboard; BOM/CRLF and all other source bytes preserved.'};
      await page.evaluate(() => ReadMDSession.flush()); await stop(); await start();
      await page.evaluate(file => loadFile(file), file); assert.equal(await page.evaluate(() => state.original), nativeText(edited));
      const result = await api('/api/academic/document', {content: edited, baseDir: docs, options: {}, view: true});
      assert.equal(result.status, 200); checkSpans(edited, result.data);
      assert(projection(result.data).citations.some(citation => citation.source.items.some(item => item.cite.mode === 'suppress-author' && item.cite.locator === '44')));
      unchanged();
    });
    await step(14, 'reused independent same-candidate oracle verifies original fixtures and snapshot without repeating requests', async () => {
      const verified = verifySameCandidateOracle(report); report.oracle = verified.summary; report.oracleEvidence = verified.evidence;
      fs.writeFileSync(path.join(data, 'completion-reused-oracle-summary.json'), JSON.stringify(verified.summary, null, 2) + '\n', {flag: 'wx'});
    });
    assert.deepEqual(errors, [], 'Uncaught production page errors');
    report.passed = true;
  } catch (error) {
    report.failedStage = stage; report.error = String(error.stack || error.message).replaceAll(data, '<owned-profile>').replaceAll(root, '<workspace>').slice(0, 3600);
    if (page) await page.screenshot({path: path.join(data, 'completion-failure.png')}).catch(() => {});
    process.exitCode = 1;
  } finally {
    try {await stop();} catch (error) {report.passed = false; report.shutdownError = error.message; process.exitCode = 1;}
    try {for (const [name, digest] of Object.entries(preserved)) assert.equal(fileHash(path.join(data, name)), digest, 'Original evidence changed: ' + name);
      report.originalEvidenceUnchanged = preserved;
    } catch (error) {report.passed = false; report.preservationError = error.message; process.exitCode = 1;}
    report.sourceHashesAtContinuationEnd = snapshotInputs(); report.recordedAt = new Date().toISOString();
    report.compositeMandatoryGroups = {inheritedPassed: report.compositeProvenance.inheritedStages.filter(check => check.passed).length,
      actualContinuationPassed: report.checks.filter(check => check.passed).length, expectedTotal: 14, passed: report.passed};
    fs.writeFileSync(completionFile, JSON.stringify(report, null, 2) + '\n', {flag: 'wx'});
    console.log(JSON.stringify({passed: report.passed, checks: report.checks.length, report: path.relative(root, completionFile), failedStage: report.failedStage}));
  }
}
run();
