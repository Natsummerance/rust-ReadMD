'use strict';
// Real MCP stdio, three actual processes, isolated profile and owned files.
// Does not call a model, launch a GUI, or inject protocol/provider responses.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const { spawn } = require('node:child_process');

const root = path.resolve(__dirname, '../..');
const sha256 = bytes => crypto.createHash('sha256').update(bytes).digest('hex');

async function run() {
  assert(process.env.READMD_BIN, 'Explicit prebuilt READMD_BIN required');
  const executable = path.resolve(process.env.READMD_BIN);
  assert(fs.statSync(executable).isFile(), 'READMD_BIN must be an existing binary');
  const cache = path.join(root, '.cache');
  const base = path.resolve(process.env.READMD_BATCH2_OUTPUT || path.join(cache, 'capability-upgrade/v006-batch2-20261027'));
  const relative = path.relative(cache, base);
  assert(relative && relative !== '..' && !relative.startsWith('..' + path.sep) && !path.isAbsolute(relative), 'Evidence must stay in the owned repository cache');
  fs.mkdirSync(base, { recursive: true });
  const output = fs.mkdtempSync(path.join(base, 'export-stdio-'));
  const profile = path.join(output, 'profile');
  const workspace = path.join(output, 'workspace');
  fs.mkdirSync(profile); fs.mkdirSync(workspace);
  const file = path.join(workspace, 'Source.md');
  const published = path.join(workspace, 'Frozen.html');
  const protectedFile = path.join(workspace, 'Protected.html');
  const original = Buffer.from('\uFEFF# Durable export 😀\r\n\r\nA: same.\r\n\r\nB: same.\r\n\r\nC: original.\r\n');
  const protectedBytes = Buffer.from('USER OWNED TARGET: DO NOT REPLACE\n');
  fs.writeFileSync(file, original); fs.writeFileSync(protectedFile, protectedBytes);
  const report = { passed: false, transport: 'real MCP stdio', processes: [], checks: [], modelCalled: false,
    source_sha256: sha256(original), profile: path.relative(root, profile) };
  let client;
  let phase = 'startup';
  const record = (name, details = {}) => report.checks.push({ name, ...details });

  function start() {
    const child = spawn(executable, ['--mcp', '--data-dir', profile, '--workspace', workspace, '--assets', path.join(root, 'assets')], {
      cwd: root, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'], env: { ...process.env, READMD_DATA_DIR: profile },
    });
    report.processes.push(child.pid);
    const close = new Promise(resolve => child.once('close', (code, signal) => resolve({ code, signal })));
    let next = 1, buffer = '', wireFailure;
    const pending = new Map();
    function rejectAll(error) {
      wireFailure = error;
      for (const entry of pending.values()) { clearTimeout(entry.timer); entry.reject(error); }
      pending.clear();
    }
    child.once('error', rejectAll);
    child.stderr.resume();
    child.stdout.setEncoding('utf8');
    child.stdout.on('data', chunk => {
      try {
        buffer += chunk;
        assert(Buffer.byteLength(buffer) <= 8 * 1024 * 1024, 'Bounded MCP response');
        for (;;) {
          const at = buffer.indexOf('\n');
          if (at < 0) break;
          const line = buffer.slice(0, at); buffer = buffer.slice(at + 1);
          if (!line.trim()) continue;
          const message = JSON.parse(line);
          if (message.id === undefined) continue;
          const entry = pending.get(message.id);
          assert(entry, 'Unknown or duplicate MCP response ID');
          pending.delete(message.id); clearTimeout(entry.timer);
          if (message.error) entry.reject(Error('MCP JSON-RPC error'));
          else entry.resolve(message.result);
        }
      } catch (error) { rejectAll(error); }
    });
    child.once('exit', () => { if (pending.size) rejectAll(Error('MCP exited with pending requests')); });
    function request(method, params) {
      if (wireFailure) return Promise.reject(wireFailure);
      return new Promise((resolve, reject) => {
        const id = next++;
        const timer = setTimeout(() => { pending.delete(id); reject(Error('MCP request deadline')); }, 45000);
        pending.set(id, { resolve, reject, timer });
        child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n', error => {
          if (error) rejectAll(Error('MCP stdin write failed'));
        });
      });
    }
    async function tool(name, args) {
      const result = await request('tools/call', { name, arguments: args });
      assert(result && Array.isArray(result.content), 'MCP content envelope');
      assert(result.structuredContent && typeof result.structuredContent === 'object', 'MCP structured result');
      assert.deepEqual(JSON.parse(result.content[0].text), result.structuredContent, 'Text and structured MCP answers agree');
      return result;
    }
    async function ok(name, args) {
      const result = await tool(name, args);
      assert.notEqual(result.isError, true, JSON.stringify(result.structuredContent));
      assert.notEqual(result.structuredContent.ok, false, JSON.stringify(result.structuredContent));
      return result.structuredContent;
    }
    async function denied(name, args, expected) {
      const result = await tool(name, args);
      assert.equal(result.isError, true, 'Rejected operation must be a real MCP tool error');
      assert.equal(result.structuredContent.error_code, expected);
      return result.structuredContent;
    }
    async function finish() {
      child.stdin.end();
      let timer;
      const result = await Promise.race([close, new Promise((_, reject) => { timer = setTimeout(() => reject(Error('Normal MCP EOF deadline')), 10000); })]).finally(() => clearTimeout(timer));
      assert.deepEqual(result, { code: 0, signal: null }, 'Normal MCP EOF closes the actual process');
      assert.equal(buffer.trim(), ''); assert.equal(pending.size, 0);
      if (wireFailure) throw wireFailure;
    }
    return { child, request, tool, ok, denied, finish, close };
  }
  async function handshake() {
    const initialized = await client.request('initialize', { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'export-harness-contract-qa', version: '2' } });
    assert(initialized.protocolVersion, 'Real MCP initialize result');
    client.child.stdin.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n');
    const catalog = await client.request('tools/list', {});
    for (const name of ['readmd_review_document', 'readmd_ai_task', 'readmd_export_snapshot']) assert(catalog.tools.some(tool => tool.name === name), 'Discover ' + name);
    const registry = await client.ok('readmd_ai_task', { action: 'tools', confirm: true });
    assert.deepEqual(registry.tools.map(tool => tool.name).sort(), ['inspect_review', 'propose_review', 'read_document', 'task_complete']);
    for (const tool of registry.tools) {
      assert.equal(tool.inputSchema.type, 'object');
      assert.equal(tool.inputSchema.additionalProperties, false);
      assert(['read', 'review_mutation', 'task_completion'].includes(tool.effect));
    }
    const tasks = await client.ok('readmd_ai_task', { action: 'list', confirm: true });
    assert.deepEqual(tasks.tasks, [], 'Listing never starts a model task');
    assert(!fs.existsSync(path.join(profile, 'ai-tasks.sqlite3')), 'Read-only task discovery stays lazy');
  }
  const review = args => client.ok('readmd_review_document', { ...args, confirm: true });
  const snapshot = args => client.ok('readmd_export_snapshot', { ...args, confirm: true });
  const cas = (state, action, call_id, extra = {}) => ({ action, document_id: state.document_id, generation: state.generation, review_revision: state.review_revision, call_id, ...extra });
  const decision = (state, changeId) => state.changes.find(change => change.id === changeId)?.decision;
  const unchangedSource = () => assert.deepEqual(fs.readFileSync(file), original, 'Original BOM/CRLF source bytes never change');
  try {
    client = start(); await handshake();
    record('discover both tools and exactly four shared native tools without creating tasks');
    phase = 'prepare-frozen-A-B';
    let current = await review({ action: 'open', path: file });
    assert.equal(current.content, '# Durable export 😀\n\nA: same.\n\nB: same.\n\nC: original.\n');
    const first = current.content.indexOf('same.');
    const second = current.content.indexOf('same.', first + 1);
    current = await review(cas(current, 'propose', 'stdio-propose-A-B', { origin: 'ai', edits: [
      { from: first, to: first + 5, insert: 'A frozen accepted sentence.' },
      { from: second, to: second + 5, insert: 'B frozen accepted sentence.' },
    ] }));
    const [a, b] = current.changes.map(change => change.id);
    assert(a && b && a !== b);
    const prepared = await snapshot({ action: 'prepare', document_id: current.document_id, generation: current.generation,
      review_revision: current.review_revision, call_id: 'stdio-freeze-A-B', format: 'html', suggestedName: 'Reviewed.md', options: {} });
    assert(!Object.prototype.hasOwnProperty.call(prepared, 'content'), 'MCP prepare does not echo private frozen source/resources');
    assert(!Object.prototype.hasOwnProperty.call(prepared, 'options'), 'MCP prepare does not echo frozen embedded assets/fonts');
    assert.equal(prepared.state, 'prepared'); assert.equal(prepared.pending_count, 2);
    assert.deepEqual([...prepared.pending_ids].sort(), [a, b].sort());
    const commit = { action: 'commit', snapshot_id: prepared.snapshot_id, out_path: published, overwrite: false,
      accept_pending: true, call_id: 'stdio-publish-A-B', task_id: 'stdio-export-task' };

    phase = 'confirmation-and-overwrite-guards';
    await client.denied('readmd_export_snapshot', commit, 'confirmation_required');
    await client.denied('readmd_export_snapshot', { ...commit, confirm: false }, 'confirmation_required');
    assert(!fs.existsSync(published));
    await client.denied('readmd_export_snapshot', { ...commit, accept_pending: false, confirm: true }, 'export_snapshot_consent_required');
    assert(!fs.existsSync(published));
    await client.denied('readmd_export_snapshot', { ...commit, out_path: protectedFile, confirm: true }, 'output_exists');
    assert.deepEqual(fs.readFileSync(protectedFile), protectedBytes); unchangedSource();
    record('missing/false confirmation, missing review consent and overwrite false block output and preserve source');

    phase = 'later-C-and-frozen-commit';
    const cFrom = current.content.indexOf('original.');
    current = await review(cas(current, 'propose', 'stdio-propose-later-C', { origin: 'ai', edits: [{ from: cFrom, to: cFrom + 9, insert: 'C LATER PENDING MUST NOT EXPORT.' }] }));
    const c = current.changes.find(change => change.id !== a && change.id !== b).id;
    const committed = await snapshot(commit);
    assert.equal(committed.snapshot_state, 'finished');
    assert.equal(decision(committed.review, a), 'accepted'); assert.equal(decision(committed.review, b), 'accepted'); assert.equal(decision(committed.review, c), 'pending');
    assert.deepEqual([...committed.review_receipt.accepted_ids].sort(), [a, b].sort());
    assert.equal(committed.review_receipt.accepted_count, 2);
    const publishedBytes = fs.readFileSync(published);
    const html = publishedBytes.toString('utf8');
    assert(html.includes('A frozen accepted sentence.')); assert(html.includes('B frozen accepted sentence.'));
    assert(html.includes('C: original.')); assert(!html.includes('C LATER PENDING MUST NOT EXPORT.'));
    unchangedSource();
    report.artifact_sha256 = sha256(publishedBytes);
    record('actual frozen HTML contains only original A/B snapshot; later C remains pending', { accepted_count: 2, later_pending: 1, bytes: publishedBytes.length });
    const replay = await snapshot(commit);
    assert.equal(replay.review.review_revision, committed.review.review_revision);
    assert.deepEqual(fs.readFileSync(published), publishedBytes);
    record('same commit safely replays through MCP with overwrite false without another acceptance');
    await client.finish();

    phase = 'second-process-restore-and-withdraw';
    client = start(); await handshake();
    current = await review({ action: 'open', path: file });
    assert.equal(decision(current, a), 'accepted'); assert.equal(decision(current, b), 'accepted'); assert.equal(decision(current, c), 'pending');
    assert(current.content.includes('C LATER PENDING MUST NOT EXPORT.'));
    const restored = await snapshot({ action: 'status', snapshot_id: prepared.snapshot_id });
    assert.deepEqual([...restored.receipt.accepted_ids].sort(), [a, b].sort());
    current = await review(cas(current, 'resolve', 'stdio-withdraw-exported-B', { ids: [b], decision: 'withdraw' }));
    assert.equal(decision(current, a), 'accepted'); assert.equal(decision(current, b), 'withdrawn'); assert.equal(decision(current, c), 'pending');
    assert(current.content.includes('B: same.')); assert(current.content.includes('A frozen accepted sentence.'));
    const afterWithdrawReplay = await snapshot(commit);
    assert.equal(decision(afterWithdrawReplay.review, b), 'withdrawn');
    assert.equal(afterWithdrawReplay.review.review_revision, current.review_revision);
    assert.deepEqual(fs.readFileSync(published), publishedBytes); unchangedSource();
    record('real EOF and second PID restore decisions; exported B remains independently withdrawable and replay cannot reaccept it');

    phase = 'changed-output-replay-must-reverify';
    const changedBytes = Buffer.from('CHANGED USER OUTPUT: NEVER OVERWRITE DURING REPLAY\n');
    fs.writeFileSync(published, changedBytes);
    await client.denied('readmd_export_snapshot', { ...commit, confirm: true }, 'export_snapshot_output_unverified');
    await client.denied('readmd_export_snapshot', { action: 'reconcile', snapshot_id: prepared.snapshot_id, confirm: true }, 'export_snapshot_output_unverified');
    assert.deepEqual(fs.readFileSync(published), changedBytes);
    const afterChangedReplay = await review({ action: 'read', document_id: current.document_id });
    assert.equal(afterChangedReplay.review_revision, current.review_revision);
    assert.equal(decision(afterChangedReplay, a), 'accepted'); assert.equal(decision(afterChangedReplay, b), 'withdrawn'); assert.equal(decision(afterChangedReplay, c), 'pending');
    unchangedSource(); record('replay and reconcile reverify changed output, report failure, never overwrite or change review decisions');
    await client.finish();

    phase = 'third-process-final-state';
    client = start(); await handshake();
    const final = await review({ action: 'open', path: file });
    assert.equal(final.content, current.content); assert.equal(final.review_revision, current.review_revision);
    assert.equal(decision(final, a), 'accepted'); assert.equal(decision(final, b), 'withdrawn'); assert.equal(decision(final, c), 'pending');
    assert.deepEqual(fs.readFileSync(published), changedBytes); assert.deepEqual(fs.readFileSync(protectedFile), protectedBytes); unchangedSource();
    await client.finish();
    assert.equal(new Set(report.processes).size, 3);
    assert(report.processes.every(pid => Number.isInteger(pid) && pid > 0));
    record('third distinct PID restores exact decisions and source bytes; no model task or GUI used');
    report.passed = true;
  } catch (error) {
    report.failure = { phase, message: String(error.message).slice(0, 700) };
    process.exitCode = 1;
  } finally {
    if (client?.child.exitCode === null) { client.child.kill(); await client.close; }
    const reportPath = path.join(output, 'report.json');
    fs.writeFileSync(reportPath, JSON.stringify(report, null, 2));
    console.log(JSON.stringify({ passed: report.passed, checks: report.checks.length, report: path.relative(root, reportPath), failure: report.failure }));
  }
}
run().catch(error => { console.error(String(error.message).slice(0, 500)); process.exitCode = 1; });
