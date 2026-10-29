'use strict';
// Explicit opt-in real-kernel acceptance. Owned synthetic documents only;
// private provider configuration and inherited credential variables are unused.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const assert = require('node:assert/strict');
const { startKernel } = require('./v006-refactor-host.cjs');
const { options } = require('./v006-table-clipboard-kernel.cjs');
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const id = () => 'refactor-' + crypto.randomUUID();
const lf = text => text.replace(/^\uFEFF/, '').replace(/\r\n?/g, '\n');
async function main() {
  const args = options(process.argv.slice(2)); assert(args.bin && !args.base, 'Explicit prebuilt kernel required');
  const workspace = path.join(args.evidence, 'documents'), profile = path.join(args.evidence, 'profile');
  fs.mkdirSync(workspace); fs.mkdirSync(profile);
  const report = { schemaVersion: 1, syntheticOnly: true, binarySha256: sha(fs.readFileSync(args.bin)), checks: [] };
  let kernel, second, phase = 'startup';
  const file = name => path.join(workspace, name);
  const cas = snapshot => ({ document_id: snapshot.document_id, generation: snapshot.generation,
    review_revision: snapshot.review_revision, call_id: crypto.randomUUID() });
  async function api(route, body, owner = kernel) {
    const response = await owner.api(route, body === undefined ? {} : { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
    return { status: response.status, body: await response.json() };
  }
  const open = p => api('/api/file?p=' + encodeURIComponent(p));
  const refactor = body => api('/api/rename/refactor', body);
  const before = name => fs.readFileSync(file(name));
  try {
    kernel = await startKernel(args.bin, profile, workspace);
    report.processes = [kernel.pid];
    const original = '# Original\r\n[Self](chapter.md#details)\r\n';
    const index = '# Contents\n[Chapter](chapter.md?print=1#details)\n\n`[untouched](chapter.md)`\n[web](https://example.test/chapter.md)\n';
    const referenceText = '# Notes\r\n[Chapter][c]\r\n\r\n[c]: ../chapter.md "Preserved title"\r\n';
    fs.mkdirSync(file('notes')); fs.writeFileSync(file('chapter.md'), original);
    fs.writeFileSync(file('index.md'), index);
    const wide = Buffer.concat([Buffer.from([255,254]), Buffer.from(referenceText, 'utf16le')]);
    fs.writeFileSync(file('notes/reference.md'), wide);
    const opened = await open(file('chapter.md')); assert.equal(opened.status, 200);
    const sourceReview = await api('/api/review', { action: 'open', path: file('chapter.md') }); assert.equal(sourceReview.status, 200);
    second = await startKernel(args.bin, profile, workspace); report.processes.push(second.pid);
    assert.equal((await api('/api/file?p=' + encodeURIComponent(file('chapter.md')), undefined, second)).status, 200);
    const request = { action: 'preview', operation_id: id(), path: file('chapter.md'), new_stem: 'renamed', workspace_root: workspace };
    phase = 'read-only exact preview';
    const preview = await refactor(request); assert.equal(preview.status, 200); assert.equal(preview.body.ok, true);
    assert.equal(preview.body.files.length, 3); assert.equal(preview.body.diagnostics.length, 0);
    assert.equal(before('chapter.md').toString(), original); assert.equal(before('notes/reference.md').equals(wide), true);
    assert.equal(fs.readdirSync(workspace).some(name => name.startsWith('.readmd-refactor')), false);
    const commit = { ...request, action: 'commit', plan_hash: preview.body.plan_hash, confirmed: true };
    phase = 'stale scan plan blocks new reference';
    fs.writeFileSync(file('introduced.md'), '[New](chapter.md)\n');
    assert.equal((await refactor(commit)).status, 409); assert.equal(before('chapter.md').toString(), original);
    fs.unlinkSync(file('introduced.md'));
    report.checks.push('Preview is read-only and adding a new referencing document invalidates confirmation');
    phase = 'commit all references and source';
    const done = await refactor(commit); assert.equal(done.status, 200); assert.equal(done.body.state, 'committed');
    assert.equal(fs.existsSync(file('chapter.md')), false);
    assert.equal(before('renamed.md').toString(), original.replace('chapter.md#', 'renamed.md#'));
    assert.equal(before('index.md').toString(), index.replace('[Chapter](chapter.md?', '[Chapter](renamed.md?'));
    const expectedWide = Buffer.concat([Buffer.from([255,254]), Buffer.from(referenceText.replace('../chapter.md', '../renamed.md'), 'utf16le')]);
    assert.equal(before('notes/reference.md').equals(expectedWide), true);
    const sourceRow = done.body.files.find(row => row.old_path === opened.body.path);
    assert.equal(sourceRow.review.document_id, sourceReview.body.document_id);
    assert.equal(done.body.cleanup_pending, undefined);
    assert.equal(fs.readdirSync(workspace).some(name => name.startsWith('.readmd-refactor')), false);
    report.checks.push('Source self-reference, inline links, reference definitions, UTF16 BOM and CRLF update exactly; code and URLs stay unchanged');
    phase = 'same operation retry and different-intent rejection';
    assert.deepEqual((await refactor(commit)).body, done.body);
    assert.equal((await refactor({ ...commit, new_stem: 'different' })).status, 409);
    const staleSecond = await api('/api/save', { path: file('chapter.md'), content: original, encoding: 'utf-8' }, second);
    assert(staleSecond.status >= 400 || staleSecond.body.ok === false); assert.equal(fs.existsSync(file('chapter.md')), false);
    report.checks.push('Durable idempotence, intent conflict and stale second-process source authorization are enforced');
    phase = 'immediate review edit/save on renamed file';
    const content = sourceRow.review.content + '\nSaved after refactor.\n';
    const edited = await api('/api/review', { action: 'edit', ...cas(sourceRow.review), origin: 'editor',
      edits: [{ from: sourceRow.review.content.length, to: sourceRow.review.content.length, insert: '\nSaved after refactor.\n' }] });
    assert.equal(edited.status, 200);
    const currentFile = await open(file('renamed.md')); assert.equal(currentFile.status, 200); assert.equal(currentFile.body.identity, sourceRow.identity);
    const saved = await api('/api/save', { path: file('renamed.md'), content, encoding: 'utf-8',
      expected_revision: currentFile.body.revision, review_session: cas(edited.body) });
    assert.equal(saved.status, 200); assert.equal(lf(before('renamed.md').toString()), content);
    report.checks.push('The same review document can edit and save immediately after path and inode changes');
    phase = 'restart retains paths and review state';
    await second.stop(); second = null; await kernel.stop();
    kernel = await startKernel(args.bin, profile, workspace); report.processes.push(kernel.pid);
    const status = await refactor({ action: 'status', operation_id: request.operation_id, plan_hash: commit.plan_hash });
    assert.equal(status.body.state, 'committed');
    assert.equal((await open(file('renamed.md'))).status, 200);
    const reopenedReview = await api('/api/review', { action: 'resume', path: file('renamed.md') });
    assert.equal(reopenedReview.status,200); assert.equal(reopenedReview.body.found,true);
    assert.equal(reopenedReview.body.snapshot.document_id, sourceReview.body.document_id); assert.equal(reopenedReview.body.snapshot.content, content);
    report.checks.push('A new real kernel process retains the terminal receipt and saved review identity');
    phase = 'durable abandon and late request';
    const unused = id(), planned = { action: 'preview', operation_id: unused, path: file('renamed.md'), new_stem: 'later', workspace_root: workspace };
    const plan2 = await refactor(planned); assert.equal(plan2.status, 200);
    const abandoned = await refactor({ action: 'abandon', operation_id: unused, plan_hash: plan2.body.plan_hash }); assert.equal(abandoned.body.state, 'abandoned');
    const late = await refactor({ ...planned, action: 'commit', confirmed: true, plan_hash: plan2.body.plan_hash });
    assert.equal(late.body.state, 'abandoned'); assert.equal(fs.existsSync(file('later.md')), false);
    report.checks.push('A durable abandon tombstone prevents a delayed commit from beginning');
    report.status = 'passed';
  } catch (error) {
    report.status = 'failed'; report.phase = phase;
    report.failure = String(error.message || error).replaceAll(args.evidence, '<evidence>').replaceAll(workspace, '<documents>').slice(0, 1800);
    throw error;
  } finally {
    await second?.stop(); await kernel?.stop();
    fs.writeFileSync(path.join(args.evidence, 'report.json'), JSON.stringify(report, null, 2) + '\n');
  }
  console.log(JSON.stringify({ status: report.status, checks: report.checks.length, binarySha256: report.binarySha256 }));
}
if (require.main === module) main().catch(() => { console.error('Refactor acceptance failed; inspect the sanitized owned report.'); process.exitCode = 1; });
module.exports = { main };
