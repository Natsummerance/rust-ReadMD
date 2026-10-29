'use strict';
// Opt-in check for external byte-level changes to a renamed document. Only
// synthetic files and explicitly supplied kernels are used.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const assert = require('node:assert/strict');
const { options } = require('./v006-table-clipboard-kernel.cjs');
const { startKernel } = require('./v006-rename-safe-http.cjs');

async function main() {
  const args = options(process.argv.slice(2)); assert(args.bin && !args.base);
  const profile = path.join(args.evidence, 'profile'), docs = path.join(args.evidence, 'documents');
  fs.mkdirSync(profile); fs.mkdirSync(docs);
  const source = path.join(docs, 'source.md'), target = path.join(docs, 'renamed.md');
  const text = '# Synthetic byte change\n'; fs.writeFileSync(source, text, { flag: 'wx' });
  // Use an exactly representable timestamp, so restoring it on Windows does
  // not lose a fractional millisecond and invalidate the test precondition.
  const timestamp = new Date('2020-01-01T00:00:00.000Z'); fs.utimesSync(source, timestamp, timestamp);
  const report = { syntheticOnly: true, tokenRequired: true, binarySha256: crypto.createHash('sha256').update(fs.readFileSync(args.bin)).digest('hex'), passed: false };
  let kernel, phase = 'startup';
  async function api(route, body) {
    const request = body === undefined ? {} : { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) };
    const response = await kernel.api(route, request); return { status: response.status, value: await response.json() };
  }
  try {
    kernel = await startKernel(args.bin, profile, docs);
    const opened = await api('/api/file?p=' + encodeURIComponent(source)); assert.equal(opened.status, 200);
    const renamed = await api('/api/rename', { path: source, new_stem: 'renamed', operation_id: 'rename-' + crypto.randomUUID(),
      expected_revision: opened.value.revision, expected_disk_revision: opened.value.disk_revision });
    assert.equal(renamed.value.state, 'committed');
    const prior = fs.statSync(target), external = Buffer.concat([Buffer.from([0xef, 0xbb, 0xbf]), Buffer.from(text)]);
    fs.writeFileSync(target, external); fs.utimesSync(target, timestamp, timestamp);
    report.sameMtimeAtMillisecondPrecision = Math.trunc(fs.statSync(target).mtimeMs) === Math.trunc(prior.mtimeMs);
    assert(report.sameMtimeAtMillisecondPrecision, 'Synthetic timestamp precondition unavailable');
    phase = 'external bytes changed but decoded text and mtime remained the same';
    const saved = await api('/api/save', { path: target, content: text + 'User draft\n', encoding: 'utf-8',
      expected_revision: opened.value.revision, expected_mtime: Math.trunc(prior.mtimeMs) / 1000 });
    report.saveStatus = saved.status; report.saveErrorCode = saved.value.error_code || null;
    report.externalBytesPreserved = fs.readFileSync(target).equals(external);
    assert.equal(saved.status, 409, 'Saving must detect an external change to the encoded bytes');
    assert.equal(report.externalBytesPreserved, true);
    report.passed = true;
  } catch (error) {
    report.failedStage = phase; report.failure = { name: error.name, message: error.message.slice(0, 300) }; process.exitCode = 1;
  } finally {
    if (kernel) await kernel.stop(); fs.writeFileSync(path.join(args.evidence, 'report.json'), JSON.stringify(report, null, 2));
  }
  console.log(JSON.stringify(report));
}
if (require.main === module) main().catch(error => { console.error(error.message); process.exitCode = 1; });
