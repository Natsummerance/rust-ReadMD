'use strict';
// Actual authenticated HTTP and MCP processes. No GUI, models or user files.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const assert = require('node:assert/strict'), { spawn } = require('node:child_process');
const { startKernel } = require('./v006-refactor-host.cjs');
const { options } = require('./v006-table-clipboard-kernel.cjs');
const ROOT = path.resolve(__dirname, '../..');
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
const LONG_SQL = 'WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000000) SELECT sum(x) FROM n;';

function stdio(binary, profile, workspace) {
  const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !/TOKEN|SECRET|CREDENTIAL|API_KEY|PROXY/i.test(key)));
  env.READMD_DATA_DIR = profile;
  const child = spawn(binary, ['--mcp', '--data-dir', profile, '--workspace', workspace, '--assets', path.join(ROOT, 'assets')], { cwd: ROOT, env, stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
  let serial = 0, buffer = '', failure, diagnostic = '';
  const pending = new Map(), close = new Promise(resolve => child.once('close', (code, signal) => resolve({ code, signal })));
  const rejectAll = error => { failure = error; for (const p of pending.values()) { clearTimeout(p.timer); p.reject(error); } pending.clear(); };
  child.once('error', rejectAll); child.stderr.on('data', bytes => { diagnostic = (diagnostic + bytes.toString('utf8')).slice(-8192); }); child.stdout.setEncoding('utf8');
  child.once('exit', () => { if (pending.size) rejectAll(Error('Owned MCP exited with pending requests')); });
  child.stdout.on('data', chunk => {
    try {
      buffer += chunk; assert(buffer.length < 1024 * 1024, 'Bounded response');
      for (;;) {
        const end = buffer.indexOf('\n'); if (end < 0) break;
        const line = buffer.slice(0, end); buffer = buffer.slice(end + 1); if (!line.trim()) continue;
        const response = JSON.parse(line); if (response.id === undefined) continue;
        const p = pending.get(response.id); assert(p, 'Unknown response'); pending.delete(response.id); clearTimeout(p.timer);
        response.error ? p.reject(Error('MCP protocol failure')) : p.resolve(response.result);
      }
    } catch (error) { rejectAll(error); }
  });
  function request(method, params) {
    if (failure) throw failure;
    const id = ++serial;
    const result = new Promise((resolve, reject) => { const timer = setTimeout(() => { pending.delete(id); reject(Error('MCP deadline')); }, 15000); pending.set(id, { resolve, reject, timer }); });
    child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n');
    return { id, result };
  }
  function notify(method, params) { child.stdin.write(JSON.stringify({ jsonrpc: '2.0', method, params }) + '\n'); }
  function cancel(request_) {
    // MCP cancellation deliberately has no response for the original ID.
    // Worker completion is verified by normal EOF waiting for all workers.
    const p = pending.get(request_.id); assert(p); clearTimeout(p.timer); pending.delete(request_.id); p.resolve(undefined);
    notify('notifications/cancelled', { requestId: request_.id });
  }
  async function stop() {
    child.stdin.end(); let timer;
    const value = await Promise.race([close, new Promise((_, reject) => { timer = setTimeout(() => reject(Error('MCP EOF deadline')), 5000); })]).finally(() => clearTimeout(timer));
    assert.deepEqual(value, { code: 0, signal: null }); assert.equal(pending.size, 0); assert.equal(buffer.trim(), ''); if (failure) throw failure;
  }
  return { child, request, notify, cancel, stop, close, diagnostic: () => diagnostic };
}

async function main() {
  const args = options(process.argv.slice(2)); assert(args.bin && !args.base, 'Prebuilt kernel required');
  const profile = path.join(args.evidence, 'profile'), workspace = path.join(args.evidence, 'documents');
  fs.mkdirSync(profile); fs.mkdirSync(workspace);
  const file = path.join(workspace, '代码与审阅 中文😀.md'), original = Buffer.from('\ufeff# Code\r\n\r\nKeep original.\r\n'); fs.writeFileSync(file, original);
  const report = { passed: false, binarySha256: crypto.createHash('sha256').update(fs.readFileSync(args.bin)).digest('hex'), productionCsp: true, strictAuth: true, modelsRequested: false, nativeUiCertified: false, checks: [], processes: [] };
  let host, mcp;
  const api = async (route, body) => { const response = await host.api(route, body === undefined ? {} : { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }); return { status: response.status, body: await response.json() }; };
  const cas = s => ({ document_id: s.document_id, generation: s.generation, review_revision: s.review_revision, call_id: crypto.randomUUID() });
  try {
    host = await startKernel(args.bin, profile, workspace); report.processes.push(host.pid);
    const page = await host.api('/');
    const csp = (await page.text()).match(/<meta http-equiv="Content-Security-Policy" content="([^"]+)"/)?.[1] || '';
    assert(csp.includes("script-src 'self' 'wasm-unsafe-eval'")); assert(!/script-src[^;]*'unsafe-(inline|eval)'/.test(csp));
    const anonymous = await fetch(new URL('/api/code/run', page.url), { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ confirm: true, lang: 'sql', code: 'SELECT 42;' }) });
    assert([401, 403].includes(anonymous.status));
    assert.equal((await api('/api/code/run', { lang: 'sql', code: 'SELECT 42;' })).status, 400);
    await api('/api/file?p=' + encodeURIComponent(file)); const first = (await api('/api/review', { action: 'open', path: file })).body;
    const pending = (await api('/api/review', { action: 'propose', ...cas(first), origin: 'ai', edits: [{ from: first.content.length, to: first.content.length, insert: 'Pending review.\n' }] })).body;
    const { call_id: _call, durable: _receipt, ...pendingDocument } = pending;
    report.checks.push('actual production CSP, authentication and explicit run confirmation');
    const task = crypto.randomUUID(), running = api('/api/code/run', { confirm: true, lang: 'sql', code: LONG_SQL, task_id: task });
    const until = Date.now() + 2000; let state;
    do { state = (await api('/api/task/cancel', { id: task })).body.state; if (state !== 'cancelling') await pause(5); } while (state !== 'cancelling' && Date.now() < until);
    assert.equal(state, 'cancelling'); const cancelledAt = performance.now(), stopped = await running;
    report.httpCancelResponseMs = performance.now() - cancelledAt;
    assert.equal(stopped.status, 200); assert.equal(stopped.body.error_code, 'execution_cancelled'); assert.equal(stopped.body.stdout, ''); assert(report.httpCancelResponseMs < 200);
    assert.equal((await api('/api/task/cancel', { id: task })).body.state, 'unknown');
    report.checks.push('actual concurrent HTTP cancellation stops SQL without successful or late output');
    const timed = await api('/api/code/run', { confirm: true, lang: 'sql', code: LONG_SQL, timeout: 1 });
    assert.equal(timed.body.error_code, 'execution_timeout');
    const deniedFile = path.join(workspace, 'not-authorized.sqlite');
    const denied = await api('/api/code/run', { confirm: true, lang: 'sql', code: `ATTACH DATABASE '${deniedFile.replaceAll("'", "''")}' AS outside;` });
    assert.equal(denied.body.error_code, 'path_access_not_allowed'); assert(!fs.existsSync(deniedFile));
    const success = await api('/api/code/run', { confirm: true, lang: 'sql', code: 'SELECT 6*7 AS answer;' }); assert.equal(success.body.ok, true); assert(success.body.stdout.includes('42'));
    assert.deepEqual(fs.readFileSync(file), original); assert.deepEqual((await api('/api/review', { action: 'read', document_id: pending.document_id })).body, pendingDocument);
    report.checks.push('timeout, forbidden SQL file access and successful next run preserve disk bytes and pending review');
    await host.stop(); host = null;
    mcp = stdio(args.bin, profile, workspace); report.processes.push(mcp.child.pid);
    report.phase = 'MCP initialize';
    await mcp.request('initialize', { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'code-lifecycle-acceptance', version: '1' } }).result; mcp.notify('notifications/initialized', {});
    const tool = arguments_ => mcp.request('tools/call', { name: 'readmd_run_code_chunk', arguments: arguments_ });
    report.phase = 'MCP run and cancel';
    const job = tool({ code: LONG_SQL, language: 'sql', confirm: true }); await pause(75);
    const notificationAt = performance.now(); mcp.cancel(job); await job.result;
    report.phase = 'MCP next successful SQL';
    const next = await tool({ code: 'SELECT 42;', language: 'sql', confirm: true }).result; assert.equal(next.structuredContent.ok, true); assert(next.structuredContent.stdout.includes('42'));
    await mcp.stop(); report.mcpWorkerStopAndEofMs = performance.now() - notificationAt; assert(report.mcpWorkerStopAndEofMs < 2000); mcp = null;
    report.checks.push('actual MCP cancellation suppresses the cancelled ID, the next call succeeds and normal EOF waits for worker termination');
    host = await startKernel(args.bin, profile, workspace); report.processes.push(host.pid);
    await api('/api/file?p=' + encodeURIComponent(file)); const reopened = (await api('/api/review', { action: 'resume', path: file })).body.snapshot;
    assert.equal(reopened.content, pending.content); assert.equal(reopened.changes[0].decision, 'pending'); assert.deepEqual(fs.readFileSync(file), original);
    await host.stop(); host = null; report.checks.push('third real process reopens unchanged original and the exact pending review'); report.passed = true;
  } catch (error) { report.failure = { name: error.name, message: String(error.message).replaceAll(args.evidence, '<evidence>').replaceAll(ROOT, '<repo>').slice(0, 800) }; process.exitCode = 1; }
  finally {
    if (host) await host.stop();
    if (mcp) {
      report.mcpExitBeforeCleanup = mcp.child.exitCode;
      if (report.failure && mcp.diagnostic()) fs.writeFileSync(path.join(args.evidence, 'mcp-first-failure.log'), mcp.diagnostic().replaceAll(args.evidence, '<evidence>').replaceAll(ROOT, '<repo>'), { flag: 'wx' });
      if (mcp.child.exitCode === null) { mcp.child.kill(); await mcp.close; }
    }
    fs.writeFileSync(path.join(args.evidence, 'report.json'), JSON.stringify(report, null, 2) + '\n', { flag: 'wx' });
    console.log(JSON.stringify({ passed: report.passed, checks: report.checks.length, httpCancelResponseMs: report.httpCancelResponseMs, mcpWorkerStopAndEofMs: report.mcpWorkerStopAndEofMs, failure: report.failure }));
  }
}
if (require.main === module) main().catch(() => { console.error('Code lifecycle acceptance failed; inspect its owned report.'); process.exitCode = 1; });
module.exports = { stdio };
