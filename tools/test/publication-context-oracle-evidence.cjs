'use strict';
// Reuse the authorized, once-only native oracle. Never execute an API request.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {root, fileHash, sha256} = require('./publication-batch3-harness.cjs');
const {loadPrivateFixtures, adapt, loadHistoricalLocale} = require('./publication-citation-oracle.cjs');
const reportPath = '.cache/capability-upgrade/publication-citation-oracle-kxSPCv/report.json';
const snapshotPath = '.cache/capability-upgrade/publication-context-engines-final-scripts-20261013T1651382558357Z';
const checkerChange = {
  before: 'fb4ee938a46498084a0aab65048d41d93b7e86e5514e00f45b8f829dc75f7c6f',
  after: '71677079b181eb47b640e41bc500b11fc825ee9ef065bd604d1bcd97bbe890f6',
  reason: 'QA-only physical underline checks use the exact target glyph bounds instead of a display span containing adjacent plain spaces. Oracle code, fixtures, adapter, canonical comparison and requests are unchanged.',
};

function verifySameCandidateOracle(current) {
  const reportFile = path.join(root, reportPath), summaryFile = path.join(path.dirname(reportFile), 'oracle-summary.json');
  const snapshotDirectory = path.join(root, snapshotPath), manifestFile = path.join(snapshotDirectory, 'manifest.json');
  assert.equal(fileHash(reportFile), 'c3cd45a25b249e784bca88c9144a8873b0b5020afd00cc387ab1d691f8c6a8b1');
  assert.equal(fileHash(summaryFile), '30d2eae2ccbee90eaaa0e8d7dd0d417a6533eaeb03da43c5074c3481ecdb9616');
  assert.equal(fileHash(manifestFile), '65c86d6579b8dc61c785198f4cffa2494e6f2a5c0235f0bf0ad5e665ef19b268');
  const prior = JSON.parse(fs.readFileSync(reportFile, 'utf8')), summary = JSON.parse(fs.readFileSync(summaryFile, 'utf8'));
  const manifest = JSON.parse(fs.readFileSync(manifestFile, 'utf8').replace(/^\uFEFF/, ''));
  for (const key of ['candidateSha256', 'bootSha256', 'servedBootSha256']) assert.equal(prior[key], current[key]);
  assert.equal(prior.candidateSha256, manifest.candidateSha256); assert.equal(prior.bootSha256, manifest.bootSha256);
  assert.equal(prior.passed, true); assert.equal(prior.platform, 'win32'); assert.equal(prior.nativeCsp, true); assert.equal(prior.noAutomaticRetries, true);
  assert.equal(prior.scriptSha256, manifest.scripts['tools/test/publication-citation-oracle.cjs']);
  assert.deepEqual(prior.oracle, summary);
  assert.deepEqual(Object.keys(prior.qaDependencySha256).sort(), Object.keys(current.qaDependencySha256).sort());
  const differences = [];
  for (const [name, digest] of Object.entries(prior.qaDependencySha256)) {
    assert.equal(manifest.scripts[name], digest, 'Standalone oracle dependency does not match its fixed source snapshot: ' + name);
    assert.equal(fileHash(path.join(snapshotDirectory, name.replaceAll('/', '__'))), digest, 'Original oracle dependency snapshot changed: ' + name);
    const present = current.qaDependencySha256[name];
    assert.equal(fileHash(path.join(root, name)), present);
    if (present === digest) continue;
    assert.equal(name, 'tools/test/publication-batch3-checks.cjs', 'An unreviewed oracle dependency changed');
    assert.equal(digest, checkerChange.before); assert.equal(present, checkerChange.after);
    differences.push({path: name, ...checkerChange});
  }
  assert.equal(differences.length, 1, 'Only the explicitly approved physical-coordinate checker correction is expected');
  const fixtures = loadPrivateFixtures(); assert.equal(fixtures.length, 21); assert.equal(summary.results.length, fixtures.length);
  for (const [index, fixture] of fixtures.entries()) {
    const p = fixture.provenance, result = summary.results[index];
    for (const key of ['repository', 'commit', 'path', 'licenseStatus']) assert.equal(result[key], p[key]);
    assert.equal(result.fixtureSha256, p.sha256); assert.equal(result.resultSha256, p.resultSha256);
    assert.equal(result.styleSha256, sha256(fixture.blocks.CSL)); assert.equal(result.requestStyleSha256, result.styleSha256);
    const historic = p.repository === 'Juris-M/citeproc-js' && p.path === 'fixtures/local/discretionary_SuppressAuthor.txt' ? loadHistoricalLocale() : null;
    const request = adapt(fixture.blocks, historic ? {localeXml: historic.xml} : {}).request;
    assert.equal(result.requestSha256, sha256(JSON.stringify(request)), 'Current adapter differs from the once-only original request');
    if (historic) assert.deepEqual(result.locale, historic.provenance);
    const optional = p.repository === 'Juris-M/citeproc-js' && p.path === 'fixtures/local/discretionary_AuthorDateAuthorOnlyThenSuppressAuthor.txt';
    assert.equal(result.mandatory, !optional); assert.equal(result.passed, !optional);
  }
  assert.equal(summary.fixtures, 21); assert.equal(summary.mandatoryFixtures, 20); assert.equal(summary.mandatoryFailures, 0); assert.equal(summary.mandatoryPassed, true);
  assert.equal(summary.passed, false); assert.equal(summary.failures, 1); assert.equal(summary.optionalCompatibility.length, 1); assert.equal(summary.optionalUnsupported.length, 1);
  assert.equal(summary.optionalCompatibility[0].passed, false); assert.equal(summary.noAutomaticRetries, true);
  return {summary, evidence: {kind: 'reused independent same-candidate standalone diagnostic', report: reportPath, reportSha256: fileHash(reportFile),
    summary: path.relative(root, summaryFile), summarySha256: fileHash(summaryFile), originalSourceSnapshot: snapshotPath,
    originalSourceSnapshotManifestSha256: fileHash(manifestFile), qaOnlyDependencyDifferences: differences, requestsRepeated: false, originalFixtureHashesRevalidated: true}};
}
module.exports = {verifySameCandidateOracle};
