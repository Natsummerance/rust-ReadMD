'use strict';
// Real native CSL regression, including the explicitly implemented cs:intext
// extension. Upstream originals and failure details remain in private storage.
const fs = require('node:fs');
const net = require('node:net');
const assert = require('node:assert/strict');
const {createHarness, fileHash} = require('./publication-batch3-harness.cjs');
const {runCitationOracle} = require('./publication-citation-oracle.cjs');
const {freezeSources} = require('../../ui-tests/publication-structure-native.cjs');

async function run() {
  assert.equal(process.platform, 'win32');
  const candidateFile = process.env.READMD_STRUCTURE_CANDIDATE_JSON;
  assert(candidateFile, 'Explicit frozen candidate required');
  const candidate = JSON.parse(fs.readFileSync(candidateFile, 'utf8'));
  assert.equal(candidate.binarySha256, process.env.READMD_CANDIDATE_SHA256);
  const freeze = freezeSources(candidate);
  const socket = net.createServer();
  await new Promise((resolve, reject) => { socket.once('error', reject); socket.listen(0, '127.0.0.1', resolve); });
  const port = socket.address().port;
  await new Promise(resolve => socket.close(resolve));
  const h = createHarness('publication-citation-intext-native', port, __filename);
  h.report.candidateFileSha256 = fileHash(candidateFile);
  h.report.sourceFreezeSha256 = candidate.sourceFreezeSha256;
  await h.finish(async () => {
    try {
      await h.start();
      assert.equal(h.report.servedBootSha256, candidate.bootSha256);
      await h.step('original 20 required citations and explicit optional intext author layout', async () => {
        h.report.oracle = await runCitationOracle(h.api, {summaryDirectory: h.data, runName: 'intext-regression'});
        assert.equal(h.report.oracle.mandatoryFixtures, 20);
        assert.equal(h.report.oracle.optionalCompatibility.length, 1);
        assert.equal(h.report.oracle.mandatoryFailures, 0, 'Original mandatory CSL expectations failed; private details retained');
        assert.equal(h.report.oracle.optionalCompatibility[0].passed, true, 'Implemented cs:intext differs from original expectation; private details retained');
        assert.equal(h.report.oracle.passed, true);
      });
    } finally {
      await h.stop();
      freeze.verify();
    }
  });
}
module.exports = {run};
if (require.main === module) run().catch(error => { console.error(error.stack || error.message); process.exitCode = 1; });
