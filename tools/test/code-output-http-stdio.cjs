'use strict';
// Actual authenticated HTTP, normal MCP EOF/restart, and owned UTF-8 BOM/CRLF
// files. No protocol mock, chooser, user data, external runtime or model.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), assert = require('node:assert/strict');
const { startKernel } = require('./v006-refactor-host.cjs');
const { stdio } = require('./code-lifecycle-http-stdio.cjs');
const { options } = require('./v006-table-clipboard-kernel.cjs');
const ROOT = path.resolve(__dirname, '../..');
const SOURCE = '# 😀\n\n```sql cmd output=true\nSELECT 42;\n```\n\nSame.\n\n```sql cmd output=true\nSELECT 42;\n```\n\nTail.\n';
const OUTPUT = '42\n--\n42';
const EXPECTED = '# 😀\n\n```sql cmd output=true\nSELECT 42;\n```\n\nPending.\n\n```sql cmd output=true\nSELECT 42;\n```\n\n<!-- code_chunk_output -->\n\n' + OUTPUT + '\n\n<!-- /code_chunk_output -->\n\nTail.\n';
const encoded = text => Buffer.from('\ufeff' + text.replaceAll('\n', '\r\n'));
const cas = (s, action, extra = {}) => ({ action, document_id: s.document_id, generation: s.generation, review_revision: s.review_revision, call_id: crypto.randomUUID(), ...extra });
async function main() {
  const args = options(process.argv.slice(2)); assert(args.bin && !args.base);
  const profile = path.join(args.evidence, 'profile'), docs = path.join(args.evidence, 'documents');
  fs.mkdirSync(profile); fs.mkdirSync(docs);
  const file = path.join(docs, '重复代码 中文😀.md'), other = path.join(docs, '其他.md');
  fs.writeFileSync(file, encoded(SOURCE)); fs.writeFileSync(other, encoded(SOURCE));
  const report = { passed: false, strictAuth: true, productionCsp: true, modelsRequested: false, nativeUiCertified: false,
    binarySha256: crypto.createHash('sha256').update(fs.readFileSync(args.bin)).digest('hex'), checks: [], processes: [] };
  let host, mcp;
  async function api(route, body) {
    const response = await host.api(route, body === undefined ? {} : { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
    return { status: response.status, body: await response.json() };
  }
  async function review(body) { const r = await api('/api/review', body); assert.equal(r.status, 200, r.body.error_code); return r.body; }
  async function connect() {
    mcp = stdio(args.bin, profile, docs); report.processes.push(mcp.child.pid);
    await mcp.request('initialize', { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'code-output-acceptance', version: '1' } }).result;
    mcp.notify('notifications/initialized', {});
  }
  async function tool(body) {
    const result = await mcp.request('tools/call', { name: 'readmd_review_document', arguments: { ...body, confirm: true } }).result;
    assert.notEqual(result.isError, true, result.structuredContent?.error_code);
    assert.deepEqual(JSON.parse(result.content[0].text), result.structuredContent); return result.structuredContent;
  }
  try {
    host = await startKernel(args.bin, profile, docs); report.processes.push(host.pid);
    const page = await host.api('/'), html = await page.text();
    const csp = html.match(/<meta http-equiv="Content-Security-Policy" content="([^"]+)"/)?.[1] || '';
    assert(csp.includes("script-src 'self'")); assert(!/script-src[^;]*'unsafe-(inline|eval)'/.test(csp));
    const denied = await fetch(new URL('/api/review', page.url), { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ action: 'code_output' }) });
    assert([401, 403].includes(denied.status));
    await api('/api/file?p=' + encodeURIComponent(file)); await api('/api/file?p=' + encodeURIComponent(other));
    const first = await review({ action: 'open', path: file });
    const before = await review(cas(first, 'propose', { origin: 'ai', edits: [{ from: SOURCE.indexOf('Same.'), to: SOURCE.indexOf('Same.') + 5, insert: 'Pending.' }] }));
    const prepared = await review(cas(before, 'code_target', { source_line: 9, code: 'SELECT 42;' }));
    assert.equal(prepared.code_target.from, '# 😀\n\n```sql cmd output=true\nSELECT 42;\n```\n\nPending.\n\n'.length);
    const run = await api('/api/code/run', { confirm: true, lang: 'sql', code: prepared.code_target.code });
    assert.equal(run.body.ok, true); assert.equal(run.body.stdout, OUTPUT);
    const insert = cas(before, 'code_output', { target: prepared.code_target, output: run.body.stdout });
    const result = await review(insert); assert.equal(result.content, EXPECTED); assert.equal(result.changes[0].decision, 'pending');
    assert.equal(result.changes[0].original, 'Same.');
    assert.deepEqual(await review(insert), result);
    assert.equal((await api('/api/review', { ...insert, output: 'different result' })).body.error_code, 'review_call_conflict');
    const second = await review({ action: 'open', path: other });
    assert.equal((await api('/api/review', cas(second, 'code_output', { target: prepared.code_target, output: OUTPUT }))).body.error_code, 'review_conflict');
    assert.deepEqual(fs.readFileSync(file), encoded(SOURCE)); assert.deepEqual(fs.readFileSync(other), encoded(SOURCE));
    report.checks.push('actual SQL binds only the second duplicate fence; idempotency, another document and pending originals stay protected');
    const saved = await api('/api/save', { path: file, content: result.content.replaceAll('\n', '\r\n'), encoding: 'utf-8-sig', review_session: cas(result, 'save') });
    assert.equal(saved.status, 200, saved.body.error_code); assert.deepEqual(fs.readFileSync(file), encoded(EXPECTED));
    await host.stop(); host = null;
    await connect();
    let head = await tool({ action: 'open', path: file }); assert.equal(head.content, EXPECTED); assert.equal(head.changes[0].decision, 'pending');
    const undone = await tool(cas(head, 'undo')); assert.equal(undone.content, before.content); assert.equal(undone.changes[0].original, 'Same.');
    head = await tool(cas(undone, 'redo')); assert.equal(head.content, EXPECTED);
    assert.deepEqual(await tool(insert), result, 'old receipt must report its original outcome without replaying the insertion');
    const current = await tool({ action: 'read', document_id: head.document_id }); assert.equal(current.generation, head.generation);
    const target = (await tool(cas(head, 'code_target', { source_line: 9, code: 'SELECT 42;' }))).code_target;
    const unchanged = await tool(cas(head, 'code_output', { target, output: OUTPUT }));
    assert.equal(unchanged.code_output_unchanged, true); assert.equal(unchanged.generation, head.generation);
    report.checks.push('new actual MCP process restores the saved body, one undo/redo and exact old receipt; identical output adds no undo');
    const length = head.content.length;
    const edited = await tool(cas(head, 'edit', { origin: 'editor', edits: [{ from: length, to: length, insert: 'Manual.\n' }] }));
    const same = await tool(cas(edited, 'undo')); assert.equal(same.content, EXPECTED);
    const stale = await mcp.request('tools/call', { name: 'readmd_review_document', arguments: { ...cas(head, 'code_output', { target, output: 'STALE' }), confirm: true } }).result;
    assert.equal(stale.isError, true); assert.equal(stale.structuredContent.error_code, 'review_stale_generation');
    await mcp.stop(); mcp = null;
    await connect(); head = await tool({ action: 'open', path: file }); assert.equal(head.content, EXPECTED);
    assert.equal(head.changes[0].decision, 'pending'); assert.equal(head.changes[0].original, 'Same.');
    assert.deepEqual(fs.readFileSync(file), encoded(EXPECTED)); assert.deepEqual(fs.readFileSync(other), encoded(SOURCE));
    await mcp.stop(); mcp = null;
    assert.equal(new Set(report.processes).size, 3); report.checks.push('edit then undo invalidates the old target, and third process preserves BOM/CRLF, pending original and the unrelated file');
    report.passed = true;
  } catch (error) {
    report.failure = { name: error.name, message: String(error.message).replaceAll(args.evidence, '<evidence>').replaceAll(ROOT, '<repo>').slice(0, 1000) }; process.exitCode = 1;
  } finally {
    if (host) await host.stop();
    if (mcp?.child.exitCode === null) { mcp.child.kill(); await mcp.close; }
    fs.writeFileSync(path.join(args.evidence, 'report.json'), JSON.stringify(report, null, 2) + '\n', { flag: 'wx' });
    console.log(JSON.stringify({ passed: report.passed, checks: report.checks.length, failure: report.failure }));
  }
}
if (require.main === module) main().catch(() => { console.error('Code output HTTP/MCP acceptance failed; inspect its owned report.'); process.exitCode = 1; });
