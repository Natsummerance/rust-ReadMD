'use strict';
// Opt-in production API checks against owned synthetic paths, with no builds
// or external connections. The parent directory is a fresh evidence folder.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { options } = require('./v006-table-clipboard-kernel.cjs');
const { startKernel } = require('./v006-rename-safe-http.cjs');
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
async function main() {
  const args = options(process.argv.slice(2));
  if (!args.bin || args.base) throw Error('Explicit prebuilt binary required');
  const docs = path.join(args.evidence, 'documents'), profile = path.join(args.evidence, 'profile');
  fs.mkdirSync(docs); fs.mkdirSync(profile);
  const uploadGuard = path.join(docs, 'upload-guard.md'), imageGuard = path.join(docs, 'image-guard.md'), exportGuard = path.join(docs, 'export-guard.md');
  fs.writeFileSync(uploadGuard, 'Upload sentinel\n', { flag: 'wx' }); fs.writeFileSync(imageGuard, 'Image sentinel\n', { flag: 'wx' });
  fs.writeFileSync(exportGuard, 'Export sentinel\n', { flag: 'wx' });
  const report = { syntheticOnly: true, tokenRequired: true, binarySha256: sha(fs.readFileSync(args.bin)), checks: [], failures: [] };
  let kernel;
  try {
    kernel = await startKernel(args.bin, profile, docs);
    const payload = Buffer.from('MALICIOUS SYNTHETIC BYTES\n');
    const attack = 'x/../../../documents/upload-guard.md';
    const upload = await kernel.api('/api/upload?ext=' + encodeURIComponent(attack), { method: 'POST', body: payload });
    const uploadIntact = fs.readFileSync(uploadGuard, 'utf8') === 'Upload sentinel\n';
    if (upload.status < 400 || !uploadIntact) report.failures.push({ case: 'upload_extension_path_escape', status: upload.status, sentinelIntact: uploadIntact });
    else report.checks.push('Upload extension traversal is rejected before modifying another file');
    const image = await kernel.api('/api/image/save', { method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ dir: docs, name: 'probe', format: 'x/../../../documents/image-guard.md', data: payload.toString('base64') }) });
    const imageIntact = fs.readFileSync(imageGuard, 'utf8') === 'Image sentinel\n';
    if (image.status < 400 || !imageIntact) report.failures.push({ case: 'image_extension_path_escape', status: image.status, sentinelIntact: imageIntact });
    else report.checks.push('Image extension traversal is rejected before modifying another file');
    const exported = await kernel.api('/api/export', { method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ format: 'html', content: '# Synthetic export\n', out_path: exportGuard, suggestedName: 'synthetic', options: {} }) });
    const exportBody = await exported.json(), exportIntact = fs.readFileSync(exportGuard, 'utf8') === 'Export sentinel\n';
    if ((exported.ok && exportBody.ok !== false) || !exportIntact) report.failures.push({ case: 'export_wrong_extension', status: exported.status, sentinelIntact: exportIntact });
    else report.checks.push('A non-Markdown export cannot overwrite a Markdown destination');
    const valid = await kernel.api('/api/upload?ext=md&name=fixture.md', { method: 'POST', body: Buffer.from('# Safe upload\n') });
    const uploaded = await valid.json();
    if (!valid.ok || typeof uploaded.path !== 'string' || fs.readFileSync(uploaded.path, 'utf8') !== '# Safe upload\n') report.failures.push({ case: 'ordinary_upload', status: valid.status });
    else report.checks.push('An ordinary Markdown upload still works in the isolated profile');
    const staged = await kernel.api('/api/image/save', { method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ stage: true, name: 'fixture', format: 'png', data: Buffer.from('synthetic-png').toString('base64') }) });
    const stagedBody = await staged.json();
    if (!staged.ok || stagedBody.ok !== true) report.failures.push({ case: 'ordinary_staged_image', status: staged.status });
    else report.checks.push('Ordinary image staging still works in the isolated profile');
    report.passed = report.failures.length === 0;
  } catch (error) { report.passed = false; report.failures.push({ case: 'harness', message: error.message.slice(0, 300) }); }
  finally { if (kernel) await kernel.stop(); fs.writeFileSync(path.join(args.evidence, 'report.json'), JSON.stringify(report, null, 2)); }
  console.log(JSON.stringify(report));
  if (!report.passed) process.exitCode = 1;
}
if (require.main === module) main().catch(error => { console.error(error.message); process.exitCode = 1; });
