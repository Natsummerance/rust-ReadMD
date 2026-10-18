'use strict';
// Explicit API/output integration diagnostic. This does not replace the
// separate full metadata-native suite or certify its UI/save/reopen workflows.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {createHarness, fileHash} = require('../tools/test/publication-batch3-harness.cjs');
const {paperMetadata, bookMetadata, sourceFor} = require('../tools/test/publication-batch3-fixtures.cjs');
const {runMetadataFieldMatrix, contentLabelsPath} = require('../tools/test/publication-metadata-field-matrix.cjs');
const h = createHarness('publication-metadata-output-native', 29126, __filename);
h.report.scope = 'integration-diagnostic-output-matrix-before-final-candidate';
h.report.fullMetadataNativeCertified = false;
h.report.limitations.push('API/export-only diagnostic: UI editing, undo, Save, reopen and preset restart are not exercised or certified here.');
const helper = path.join(h.root, 'tools/test/publication-metadata-field-matrix.cjs');
h.report.qaDependencySha256['tools/test/publication-metadata-field-matrix.cjs'] = fileHash(helper);
fs.copyFileSync(__filename, path.join(h.data, 'qa-script.cjs')); fs.copyFileSync(helper, path.join(h.data, 'qa-metadata-field-matrix.cjs'));
process.env.READMD_NATIVE_LOG_FILE ||= path.join(h.data, 'native-host.log');
h.report.contentLabelsSha256 = fileHash(contentLabelsPath);
for (const [kind, metadata] of [['paper', paperMetadata], ['book', bookMetadata]]) fs.writeFileSync(path.join(h.docs, kind + '.md'), sourceFor(metadata, '# Output diagnostic input\n\nSynthetic source stays unchanged.\n', {bibliography: false}));
const expectedInputs = h.snapshotInputs(); h.report.inputs = expectedInputs;
const baseOptions = {pdf: {engine: 'rust'}, typography: {font: 'Arial', size: 11}, cover: {enabled: false}, toc: {enabled: false}, footer: {pageNumbers: false}};
h.finish(async () => {
  assert.match(process.env.READMD_CANDIDATE_BOOT_SHA256 || '', /^[a-f0-9]{64}$/i);
  const expectedBoot = process.env.READMD_CANDIDATE_BOOT_SHA256.toLowerCase();
  assert.equal(fileHash(path.join(process.env.READMD_ASSETS_DIR || path.join(h.root, 'assets'), 'readmd.boot.js')), expectedBoot);
  h.report.binarySha256 = h.report.candidateSha256; h.report.expectedBootSha256 = expectedBoot;
  const page = await h.start(); assert.equal(h.report.bootSha256, expectedBoot);
  await h.step('production CSP and existing offline fonts/PDFium packs', async () => {
    const csp = await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content');
    const scripts = csp.split(';').find(value => value.trim().startsWith('script-src'));
    assert(scripts?.includes("'self'") && !scripts.includes("'unsafe-inline'") && !scripts.includes("'unsafe-eval'"));
    await h.installPacks(); h.unchanged(expectedInputs);
  });
  let matrix, matrixError, validatorError;
  try {
    await h.step('35 independent A/B cases and two baselines: every format attempted once', async () => {
      matrix = await runMetadataFieldMatrix(h, baseOptions, expectedInputs, {collectAll: true});
      assert.equal(matrix.passed, true, JSON.stringify({counts: matrix.counts, firstFailure: matrix.failures[0]}));
    });
  } catch (error) {matrixError = error;}
  try {
    await h.step('all actually produced EPUB files pass locked offline EPUBCheck', async () => {
      const epubs = fs.readdirSync(h.outputs).filter(name => name.endsWith('.epub')).map(name => path.join(h.outputs, name));
      assert(epubs.length > 0, 'No actual EPUB was produced; validator cannot run');
      const {checkEpub} = await import('../tools/check-epub.mjs');
      const jar = process.env.READMD_EPUBCHECK_JAR || path.join(h.root, '.cache/community-upgrade/qa-tools/epubcheck-v5.4.0/epubcheck-5.4.0/epubcheck.jar');
      const validation = await checkEpub(epubs, {jar, output: path.join(h.data, 'epubcheck.json')});
      h.report.epubcheck = validation;
      assert.equal(validation.passed, true, JSON.stringify(validation.files.filter(file => !file.passed)));
      assert.equal(epubs.length, 37, 'An A/B EPUB file is missing');
      h.unchanged(expectedInputs);
    });
  } catch (error) {validatorError = error;}
  await h.step('aggregate output diagnostic retains every failure and certifies no UI workflow', async () => {
    assert.equal(h.report.fullMetadataNativeCertified, false);
    if (matrixError || validatorError) throw new AggregateError([matrixError, validatorError].filter(Boolean), 'Output diagnostic failed; inspect metadata-field-matrix.json and epubcheck.json for every recorded failure.');
    assert.equal(matrix.counts.artifacts, 111); assert.equal(matrix.counts.validated, 111); h.unchanged(expectedInputs);
  });
});
