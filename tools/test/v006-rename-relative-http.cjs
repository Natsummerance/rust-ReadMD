'use strict';
// Isolated regression: a path opened relative to the kernel working directory
// must refer to that same file when a rename request is made.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const assert = require('node:assert/strict');
const { options } = require('./v006-table-clipboard-kernel.cjs');
const { startKernel } = require('./v006-rename-safe-http.cjs');
const { ROOT } = require('./v006-table-clipboard-host.cjs');
async function main() {
  const args = options(process.argv.slice(2)); assert(args.bin && !args.base);
  const docs = path.join(args.evidence, 'documents'), profile = path.join(args.evidence, 'profile');
  fs.mkdirSync(docs); fs.mkdirSync(profile);
  const old = path.join(docs, 'relative.md'), target = path.join(docs, 'renamed.md');
  fs.writeFileSync(old, '# Synthetic relative document\n', { flag: 'wx' });
  const relative = path.relative(ROOT, old), report = { syntheticOnly: true, passed: false, checks: [] };
  let kernel;
  try {
    kernel = await startKernel(args.bin, profile, docs);
    report.binarySha256 = crypto.createHash('sha256').update(fs.readFileSync(args.bin)).digest('hex');
    const opened = await kernel.api('/api/file?p=' + encodeURIComponent(relative));
    assert.equal(opened.status, 200); const descriptor = await opened.json();
    const response = await kernel.api('/api/rename', { method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: relative, new_stem: 'renamed', operation_id: crypto.randomUUID(),
        expected_revision: descriptor.revision, expected_disk_revision: descriptor.disk_revision }) });
    const moved = await response.json(); assert.equal(moved.state, 'committed');
    assert.equal(fs.existsSync(old), false); assert.equal(fs.readFileSync(target, 'utf8'), '# Synthetic relative document\n');
    report.checks.push('Relative open and rename refer to the same authorized file');
    const outside = path.join(args.evidence, 'outside.md'), outsideTarget = path.join(args.evidence, 'outside-renamed.md');
    fs.writeFileSync(outside, '# Synthetic selected file\n', { flag: 'wx' });
    const outsideOpen = await kernel.api('/api/file?p=' + encodeURIComponent(outside)); assert.equal(outsideOpen.status, 200);
    const outsideDescriptor = await outsideOpen.json();
    const outsideResponse = await kernel.api('/api/rename', { method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: outside, new_stem: 'outside-renamed', operation_id: crypto.randomUUID(),
        expected_revision: outsideDescriptor.revision, expected_disk_revision: outsideDescriptor.disk_revision }) });
    const outsideMoved = await outsideResponse.json(); assert.equal(outsideMoved.state, 'committed');
    assert.equal(fs.existsSync(outside), false); assert.equal(fs.readFileSync(outsideTarget, 'utf8'), '# Synthetic selected file\n');
    report.checks.push('A user-opened file outside the workspace can be renamed in its own directory'); report.passed = true;
  } catch (error) { report.failure = { name: error.name, message: error.message.slice(0, 400) }; process.exitCode = 1; }
  finally { if (kernel) await kernel.stop(); fs.writeFileSync(path.join(args.evidence, 'report.json'), JSON.stringify(report, null, 2)); }
  console.log(JSON.stringify(report));
}
main().catch(error => { console.error(error.message); process.exitCode = 1; });
