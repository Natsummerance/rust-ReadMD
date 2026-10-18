'use strict';
// Continue only the oracle group after the old-candidate evidence binding
// stopped an otherwise successful native run. No product requests are retried.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {createHarness, fileHash} = require('./publication-batch3-harness.cjs');
const {runCitationOracle} = require('./publication-citation-oracle.cjs');
const h = createHarness('publication-context-oracle-completion', 29127, __filename);
const priorName = process.argv[2], priorDigest = process.argv[3];
assert(priorName && !path.isAbsolute(priorName), 'Relative original report path required');
const priorFile = path.resolve(h.root, priorName), relative = path.relative(h.root, priorFile);
assert(!relative.startsWith('..') && !path.isAbsolute(relative), 'Owned original report required');
assert.equal(fileHash(priorFile), priorDigest, 'Original execution changed');
const prior = JSON.parse(fs.readFileSync(priorFile, 'utf8'));
assert.equal(prior.passed, false); assert.equal(prior.checks.length, 14);
assert.equal(prior.noAutomaticRetries, true); assert.equal(prior.nativeCsp, true);
assert.equal(prior.candidateSha256, h.report.candidateSha256);
assert(prior.checks.slice(0, 13).every(check => check.passed));
assert.equal(prior.checks[13].passed, false);
assert.equal(prior.failedStage, 'reused independent same-candidate oracle verifies original fixture hashes without repeating requests');
assert(prior.error.includes('5d39faf5b275d58106ffb87353857a01b4fb92e48ff2f3a6cb6ca066ed8df1b1'));
assert.deepEqual(prior.qaDependencySha256, h.report.qaDependencySha256, 'Oracle implementation or fixture adapter changed');
h.report.compositeProvenance = {originalReport: priorName.replaceAll('\\', '/'), originalReportSha256: priorDigest,
  inheritedStages: prior.checks.slice(0, 13), reason: 'QA evidence reuse was fixed to run the original independent cases once against the new candidate. The first thirteen successful groups and the original failed evidence-binding check remain unchanged.'};
h.report.compositeMandatoryGroups = {inheritedPassed: 13, actualContinuationPassed: 1, expectedTotal: 14, passed: false};
h.report.singleRunFullSuitePassed = false;
h.finish(async () => {
  const page = await h.start();
  for (const key of ['bootSha256', 'servedBootSha256']) assert.equal(prior[key], h.report[key]);
  const csp = await page.locator('meta[http-equiv="Content-Security-Policy"]').getAttribute('content');
  const scripts = csp.split(';').find(value => value.trim().startsWith('script-src'));
  assert(scripts?.includes("'self'") && !scripts.includes("'unsafe-eval'") && !scripts.includes("'unsafe-inline'"));
  await h.step('fixed independent upstream RESULT oracle over this native candidate, once per fixture', async () => {
    h.report.oracle = await runCitationOracle(h.api, {summaryDirectory: h.data});
    assert.equal(h.report.oracle.mandatoryFixtures, 20);
    assert.equal(h.report.oracle.optionalCompatibility.length, 1);
    assert.equal(h.report.oracle.mandatoryFailures, 0, 'Mandatory original RESULT mismatches retained; no fixture was retried');
    h.report.oracleEvidence = {kind: 'actual current-candidate requests after QA evidence binding failure', requestsRepeated: false, originalFixtureHashesRevalidated: true};
  });
  assert.equal(fileHash(priorFile), priorDigest);
  h.report.compositeMandatoryGroups.passed = true;
});
