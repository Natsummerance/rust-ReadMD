'use strict';
// Source-bound pure SQL, actual HTTP/MCP processes, saved BOM/CRLF and durable
// review history. No model, host interpreter, GUI or simulated file service.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), assert = require('node:assert/strict');
const { options } = require('./v006-table-clipboard-kernel.cjs'), { startKernel } = require('./v006-refactor-host.cjs');
const { stdio } = require('./code-lifecycle-http-stdio.cjs');
const ROOT = path.resolve(__dirname, '../..'), pause = ms => new Promise(resolve => setTimeout(resolve, ms));
const code = '```sql cmd output=true\nSELECT 42;\n```\n';
const SOURCE = '# Batch 😀\n\n' + code + '\nSame.\n\n' + code;
const OUT = '42\n--\n42', each = code + '\n<!-- code_chunk_output -->\n\n' + OUT + '\n\n<!-- /code_chunk_output -->\n';
const EXPECTED = '# Batch 😀\n\n' + each + '\nPending.\n\n' + each;
const LONG = '```sql cmd output=true\nWITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<1000000000) SELECT sum(x) FROM n;\n```\n\n' + code;
const encoded = text => Buffer.from('\ufeff' + text.replaceAll('\n', '\r\n'));
const cas = (s, action, extra = {}) => ({ action, document_id: s.document_id, generation: s.generation, review_revision: s.review_revision, call_id: crypto.randomUUID(), ...extra });
async function main() {
  const args = options(process.argv.slice(2)); assert(args.bin && !args.base);
  const profile = path.join(args.evidence, 'profile'), docs = path.join(args.evidence, 'documents'); fs.mkdirSync(profile); fs.mkdirSync(docs);
  const fixtures = { '批量 中文😀.md': SOURCE, 'other.md': SOURCE, 'cancel.md': LONG,
    'failure.md': '```sql cmd output=true\nINVALID SQL;\n```\n\n' + code,
    'partial.md': code + '\n```sql cmd output=true\nINVALID SQL;\n```\n',
    'mixed.md': '```sql cmd output=true\nINVALID SQL;\n```\n\n```js cmd\nconsole.log(999);\n```\n',
    'generated.md': code + '\n<!-- code_chunk_output -->\n\n```sql cmd output=true\nSELECT 999;\n```\n\n<!-- /code_chunk_output -->\n' };
  for (const [name, text] of Object.entries(fixtures)) fs.writeFileSync(path.join(docs, name), encoded(text));
  const file = path.join(docs, '批量 中文😀.md'), report = { passed: false, pureInMemorySqlOnly: true, strictAuth: true, productionCsp: true,
    modelsRequested: false, nativeUiCertified: false, binarySha256: crypto.createHash('sha256').update(fs.readFileSync(args.bin)).digest('hex'), checks: [], processes: [] };
  let host, mcp;
  const api = async (route, body) => { const response = await host.api(route, body === undefined ? {} : { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }); return { status: response.status, body: await response.json() }; };
  const review = async body => { const result = await api('/api/review', body); assert.equal(result.status, 200, result.body.error_code); return result.body; };
  const open = async name => { const path_ = path.join(docs, name); await api('/api/file?p=' + encodeURIComponent(path_)); return review({ action: 'open', path: path_ }); };
  const prepare = async head => (await review(cas(head, 'code_batch_target'))).code_batch;
  async function connect() {
    mcp = stdio(args.bin, profile, docs); report.processes.push(mcp.child.pid);
    await mcp.request('initialize', { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'batch-acceptance', version: '1' } }).result;
    mcp.notify('notifications/initialized', {});
  }
  const tool = async args_ => { const value = await mcp.request('tools/call', { name: 'readmd_review_document', arguments: { ...args_, confirm: true } }).result; assert.notEqual(value.isError, true, value.structuredContent?.error_code); return value.structuredContent; };
  try {
    host = await startKernel(args.bin, profile, docs); report.processes.push(host.pid);
    const root = await host.api('/'), html = await root.text(), csp = html.match(/<meta http-equiv="Content-Security-Policy" content="([^"]+)"/)?.[1] || '';
    assert(csp.includes("script-src 'self'")); assert(!/script-src[^;]*'unsafe-(inline|eval)'/.test(csp));
    const denied = await fetch(new URL('/api/review', root.url), { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ action: 'code_batch_run', confirm: true }) }); assert([401, 403].includes(denied.status));
    const first = await open('批量 中文😀.md'), before = await review(cas(first, 'propose', { origin: 'ai', edits: [{ from: SOURCE.indexOf('Same.'), to: SOURCE.indexOf('Same.') + 5, insert: 'Pending.' }] }));
    const batch = await prepare(before); assert.equal(batch.items.length, 2);
    assert.equal((await api('/api/review', cas(before, 'code_batch_run', { batch }))).body.error_code, 'confirmation_required');
    const run = await review(cas(before, 'code_batch_run', { batch, confirm: true })); assert.equal(run.ok, true); assert.equal(run.source_written, false); assert.deepEqual(run.results.map(r => r.stdout), [OUT, OUT]);
    const insert = cas(before, 'code_batch_output', { batch, outputs: run.results.map(r => r.stdout) }), result = await review(insert);
    assert.equal(result.content, EXPECTED); assert.equal(result.generation, before.generation + 1); assert.equal(result.changes[0].original, 'Same.'); assert.equal(result.changes[0].decision, 'pending');
    assert.deepEqual(await review(insert), result); assert.equal((await api('/api/review', { ...insert, outputs: ['other', OUT] })).body.error_code, 'review_call_conflict');
    const other = await open('other.md'); assert.equal((await api('/api/review', cas(other, 'code_batch_output', { batch, outputs: [OUT, OUT] }))).body.error_code, 'review_conflict');
    assert.deepEqual(fs.readFileSync(file), encoded(SOURCE));
    const saved = await api('/api/save', { path: file, content: EXPECTED.replaceAll('\n', '\r\n'), encoding: 'utf-8-sig', review_session: cas(result, 'save') }); assert.equal(saved.status, 200, saved.body.error_code); assert.deepEqual(fs.readFileSync(file), encoded(EXPECTED));
    report.checks.push('authenticated duplicate SQL batch commits exactly once, preserves pending originals and saves exact BOM/CRLF');
    for (const name of ['mixed.md', 'failure.md', 'partial.md', 'generated.md']) {
      const head = await open(name), batch_ = await prepare(head), execution = (await api('/api/review', cas(head, 'code_batch_run', { batch: batch_, confirm: true }))).body;
      if (name === 'mixed.md') assert.equal(execution.error_code, 'capability_pack_unavailable');
      if (name === 'failure.md') { assert.equal(execution.error_code, 'execution_failed'); assert.equal(execution.failed_index, 0); assert.deepEqual(execution.results, []); }
      if (name === 'partial.md') { assert.equal(execution.error_code, 'execution_failed'); assert.equal(execution.failed_index, 1); assert.equal(execution.results[0].stdout, OUT); }
      if (name === 'generated.md') { assert.equal(batch_.items.length, 1); assert.equal(execution.results.length, 1); assert.equal(execution.results[0].stdout, OUT); }
      assert.equal((await review({ action: 'read', document_id: head.document_id })).generation, head.generation);
      assert.deepEqual(fs.readFileSync(path.join(docs, name)), encoded(fixtures[name]));
    }
    report.checks.push('mixed runtime preflight, failure stop and generated-code exclusion do not mutate sources');
    const cancelHead = await open('cancel.md'), cancelBatch = await prepare(cancelHead), task = crypto.randomUUID();
    const running = api('/api/review', cas(cancelHead, 'code_batch_run', { batch: cancelBatch, confirm: true, task_id: task })); await pause(100);
    assert.equal((await api('/api/task/cancel', { id: task })).body.state, 'cancelling'); const at = performance.now(), stopped = await running; report.cancelResponseMs = performance.now() - at;
    assert.equal(stopped.body.error_code, 'execution_cancelled'); assert.equal(stopped.body.failed_index, 0); assert.deepEqual(stopped.body.results, []); assert(report.cancelResponseMs < 200);
    assert.equal((await api('/api/task/cancel', { id: task })).body.state, 'unknown');
    assert.equal((await review({ action: 'read', document_id: cancelHead.document_id })).generation, cancelHead.generation); assert.deepEqual(fs.readFileSync(path.join(docs, 'cancel.md')), encoded(LONG));
    report.checks.push('actual concurrent HTTP cancellation stops the first SQL and admits no later block or partial output');
    await host.stop(); host = null; await connect();
    let head = await tool({ action: 'open', path: file }); assert.equal(head.content, EXPECTED);
    let undone = await tool(cas(head, 'undo')); assert.equal(undone.content, before.content); assert.equal(undone.changes[0].decision, 'pending'); assert.deepEqual(await tool(insert), result);
    assert.equal((await tool({ action: 'read', document_id: head.document_id })).content, before.content);
    head = await tool(cas(undone, 'redo')); assert.equal(head.content, EXPECTED);
    const nextBatch = (await tool(cas(head, 'code_batch_target'))).code_batch, next = await tool(cas(head, 'code_batch_run', { batch: nextBatch })); assert.equal(next.results.length, 2);
    const identical = await tool(cas(head, 'code_batch_output', { batch: nextBatch, outputs: next.results.map(r => r.stdout) })); assert.equal(identical.code_output_unchanged, true); assert.equal(identical.generation, head.generation);
    const cancelledHead = await tool({ action: 'open', path: path.join(docs, 'cancel.md') }), cancellationBatch = (await tool(cas(cancelledHead, 'code_batch_target'))).code_batch;
    const mcpRun = mcp.request('tools/call', { name: 'readmd_review_document', arguments: { ...cas(cancelledHead, 'code_batch_run', { batch: cancellationBatch }), confirm: true } }); await pause(100);
    const mcpAt = performance.now(); mcp.cancel(mcpRun); await mcpRun.result; await mcp.stop(); report.mcpCancelAndEofMs = performance.now() - mcpAt; assert(report.mcpCancelAndEofMs < 2000); mcp = null;
    report.checks.push('actual MCP process restores one undo/redo, old receipts and no-op outputs; external cancellation and normal EOF stop a batch');
    host = await startKernel(args.bin, profile, docs); report.processes.push(host.pid); head = await open('批量 中文😀.md');
    assert.equal(head.content, EXPECTED); assert.equal(head.changes[0].original, 'Same.'); assert.equal(head.changes[0].decision, 'pending');
    undone = await review(cas(head, 'undo')); assert.equal(undone.content, before.content); head = await review(cas(undone, 'redo')); assert.equal(head.content, EXPECTED);
    for (const [name, content] of Object.entries(fixtures)) assert.deepEqual(fs.readFileSync(path.join(docs, name)), encoded(name === '批量 中文😀.md' ? EXPECTED : content));
    assert.equal(new Set(report.processes).size, 3); report.checks.push('third real process reopens the exact saved result and independent history; unrelated files stay byte-identical');
    report.passed = true;
  } catch (error) { report.failure = { name: error.name, message: String(error.message).replaceAll(args.evidence, '<evidence>').replaceAll(ROOT, '<repo>').slice(0, 1000) }; process.exitCode = 1; }
  finally {
    if (host) await host.stop(); if (mcp?.child.exitCode === null) { mcp.child.kill(); await mcp.close; }
    fs.writeFileSync(path.join(args.evidence, 'report.json'), JSON.stringify(report, null, 2) + '\n', { flag: 'wx' });
    console.log(JSON.stringify({ passed: report.passed, checks: report.checks.length, failure: report.failure, cancelResponseMs: report.cancelResponseMs, mcpCancelAndEofMs: report.mcpCancelAndEofMs }));
  }
}
if (require.main === module) main().catch(() => { console.error('Code batch HTTP/MCP acceptance failed; inspect the owned report.'); process.exitCode = 1; });
