'use strict';
// Real loopback HTTP + MCP stdio against an explicitly selected prebuilt kernel.
// Fixture files and profiles only. No compiler, model, browser, retries or GUI.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const net = require('node:net');
const assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const ROOT = path.resolve(__dirname, '../..');
const hash = value => crypto.createHash('sha256').update(value).digest('hex');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));

function plan() {
  return {
    scope: 'V6-31 offline OpenAPI local references through the shared Rust service',
    transports: ['real loopback HTTP', 'real MCP stdio'],
    checks: ['JSON/YAML relative references and source maps', 'HTTP/MCP projection equality',
      'one reversible content transaction', 'explicit guarded save and real file reopen',
      'dependency-only mutation rejects stale preview over both transports',
      'dependency update, idempotence, surrounding prose and manual edit protection',
      'missing dependency creation invalidates old receipt',
      'canonical containment and native junction/symlink escape rejection',
      'cycles and bounded source bytes', 'single-file compatibility'],
    nativeGuiCertified: false, extensionHostCertified: false, modelCalled: false,
    automaticRetries: 0, binaryRequired: 'READMD_BIN',
  };
}
function ownedEnvironment(profile, documents) {
  const env = { ...process.env, READMD_DATA_DIR: profile, READMD_WORKSPACE: documents,
    READMD_ASSETS_DIR: path.join(ROOT, 'assets'), READMD_EXTRA_ROOTS: '', READMD_REQUIRE_TOKEN: '1' };
  delete env.READMD_APP_TOKEN;
  return env;
}
async function freePort() {
  const server = net.createServer();
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  const port = server.address().port;
  await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
  return port;
}
function startMcp(binary, profile, documents) {
  const child = spawn(binary, ['--mcp', '--data-dir', profile, '--workspace', documents, '--assets', path.join(ROOT, 'assets')],
    { cwd: ROOT, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'], env: ownedEnvironment(profile, documents) });
  const closed = new Promise(resolve => child.once('close', (code, signal) => resolve({ code, signal })));
  const pending = new Map();
  let next = 0, buffer = '', failure;
  function rejectAll(error) {
    failure = error;
    for (const row of pending.values()) { clearTimeout(row.timer); row.reject(error); }
    pending.clear();
  }
  child.once('error', rejectAll);
  child.once('exit', () => { if (pending.size) rejectAll(Error('MCP exited with pending requests')); });
  child.stderr.resume();
  child.stdout.setEncoding('utf8');
  child.stdout.on('data', chunk => {
    try {
      buffer += chunk;
      assert(Buffer.byteLength(buffer) <= 20 * 1024 * 1024, 'MCP response byte limit');
      for (;;) {
        const at = buffer.indexOf('\n');
        if (at < 0) break;
        const line = buffer.slice(0, at); buffer = buffer.slice(at + 1);
        if (!line.trim()) continue;
        const message = JSON.parse(line);
        if (message.id === undefined) continue;
        const row = pending.get(message.id);
        assert(row, 'Unknown or duplicate MCP response ID');
        pending.delete(message.id); clearTimeout(row.timer);
        if (message.error) row.reject(Error('MCP JSON-RPC error'));
        else row.resolve(message.result);
      }
    } catch (error) { rejectAll(error); }
  });
  function request(method, params) {
    if (failure) return Promise.reject(failure);
    return new Promise((resolve, reject) => {
      const id = ++next;
      const timer = setTimeout(() => { pending.delete(id); reject(Error('MCP deadline')); }, 30000);
      pending.set(id, { resolve, reject, timer });
      child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n', error => {
        if (error) rejectAll(Error('MCP stdin failed'));
      });
    });
  }
  async function tool(name, args, expectedError) {
    const envelope = await request('tools/call', { name, arguments: args });
    assert(envelope && Array.isArray(envelope.content) && envelope.structuredContent, 'MCP result envelope');
    assert.deepEqual(JSON.parse(envelope.content[0].text), envelope.structuredContent, 'MCP text/structured parity');
    if (expectedError) {
      assert.equal(envelope.isError, true);
      assert.equal(envelope.structuredContent.error_code, expectedError);
    } else {
      assert.notEqual(envelope.isError, true, 'MCP tool unexpectedly failed');
      assert.equal(envelope.structuredContent.ok, true);
    }
    return envelope.structuredContent;
  }
  async function finish() {
    child.stdin.end();
    let timer;
    const result = await Promise.race([closed, new Promise((_, reject) => {
      timer = setTimeout(() => reject(Error('MCP EOF deadline')), 10000);
    })]).finally(() => clearTimeout(timer));
    assert.deepEqual(result, { code: 0, signal: null });
    assert.equal(buffer.trim(), ''); assert.equal(pending.size, 0);
    if (failure) throw failure;
  }
  return { child, closed, request, tool, finish };
}

async function run() {
  assert(process.env.READMD_BIN, 'Explicit prebuilt READMD_BIN is required');
  const binary = path.resolve(process.env.READMD_BIN);
  assert(fs.statSync(binary).isFile(), 'READMD_BIN must be an existing binary');
  const cache = path.join(ROOT, '.cache', 'capability-upgrade');
  const base = path.resolve(process.env.READMD_V006_OPENAPI_OUTPUT || path.join(cache, 'v006-batch3-openapi'));
  const relative = path.relative(cache, base);
  assert(relative && relative !== '..' && !relative.startsWith('..' + path.sep) && !path.isAbsolute(relative), 'Owned evidence root');
  fs.mkdirSync(base, { recursive: true });
  const output = fs.mkdtempSync(path.join(base, 'run-'));
  const documents = path.join(output, 'api');
  for (const dir of ['api', 'api/paths', 'api/defs', 'api/schemas', 'http-profile', 'mcp-profile', 'outside']) fs.mkdirSync(path.join(output, dir), { recursive: true });
  const source = path.join(documents, 'api.yaml'), target = path.join(documents, 'API-guide.md');
  const original = '# Author notes 😀\r\n\r\nPreserve the original guide.\r\n';
  const specification = "openapi: 3.1.1\ninfo: {title: Local reference API, version: '1'}\npaths:\n  /notes/{id}:\n    $ref: './paths/note.yaml'\ncomponents:\n  schemas:\n    Note: {$ref: './schemas/note.json#/$defs/Note'}\n";
  fs.writeFileSync(source, specification); fs.writeFileSync(target, original);
  fs.writeFileSync(path.join(documents, 'paths/note.yaml'), "get:\n  summary: Read original note\n  parameters: [{$ref: '../defs/parameters.yaml#/Id'}]\n  responses: {'200': {$ref: '../defs/responses.yaml#/Read'}}\npatch:\n  summary: Update original note\n  requestBody: {$ref: '../defs/request.yaml'}\n  responses: {'204': {description: Saved}}\n");
  fs.writeFileSync(path.join(documents, 'defs/parameters.yaml'), "Id: {name: id, in: path, required: true, schema: {type: string}}\n");
  fs.writeFileSync(path.join(documents, 'defs/responses.yaml'), "Read:\n  description: Returned note\n  content:\n    application/json:\n      schema: {$ref: '../schemas/note.json#/$defs/Note'}\n      example: {$ref: 'literal-do-not-read.yaml'}\n");
  fs.writeFileSync(path.join(documents, 'defs/request.yaml'), "required: true\ncontent: {application/json: {schema: {$ref: '../schemas/note.json#/$defs/Note'}}}\n");
  fs.writeFileSync(path.join(documents, 'schemas/note.json'), JSON.stringify({ $defs: { Note: { type: 'object', properties: { text: { $ref: '../defs/string.yaml' } } } } }));
  const stringFile = path.join(documents, 'defs/string.yaml');
  fs.writeFileSync(stringFile, 'type: string\nminLength: 2\n');
  const report = { ...plan(), passed: false, checks: [], startedAt: new Date().toISOString(),
    binarySha256: hash(fs.readFileSync(binary)), moduleSha256: hash(fs.readFileSync(path.join(ROOT, 'rust/readmd-kernel/src/openapi_document.rs'))),
    runnerSha256: hash(fs.readFileSync(__filename)), bootSha256: hash(fs.readFileSync(path.join(ROOT, 'assets/readmd.boot.js'))) };
  let phase = 'startup', server, mcp, serverClosed, token, origin;
  const record = name => report.checks.push({ name, passed: true });
  async function http(method, route, payload, expected = 200) {
    const response = await fetch(origin + route, { method, headers: {
      ...(payload === undefined ? {} : { 'content-type': 'application/json' }),
      ...(token ? { 'x-readmd-token': token, 'x-readmd-app-token': token } : {}),
    }, body: payload === undefined ? undefined : JSON.stringify(payload), signal: AbortSignal.timeout(30000) });
    const data = await response.json();
    assert.equal(response.status, expected, 'Unexpected HTTP status in ' + phase);
    if (expected === 200) assert.equal(data.ok, true, 'Unexpected HTTP tool failure in ' + phase);
    return data;
  }
  const args = content => ({ file_path: source, source_id: 'fixture/api.yaml', content, base_revision: hash(content), locale: 'en' });
  const openapi = (content, extra = {}, status = 200) => http('POST', '/api/developer/openapi', { ...args(content), ...extra }, status);
  const mcpApi = (content, extra = {}, error) => mcp.tool('readmd_openapi_document', { ...args(content), ...extra }, error);
  try {
    const port = await freePort(); origin = 'http://127.0.0.1:' + port;
    const profile = path.join(output, 'http-profile');
    server = spawn(binary, ['--no-window', '--port', String(port), '--host', '127.0.0.1', '--require-token', '--data-dir', profile,
      '--workspace', documents, '--assets', path.join(ROOT, 'assets')],
      { cwd: ROOT, windowsHide: true, stdio: ['ignore', 'ignore', 'ignore'], env: ownedEnvironment(profile, documents) });
    let launchError; server.once('error', error => { launchError = error; });
    serverClosed = new Promise(resolve => server.once('close', (code, signal) => resolve({ code, signal })));
    const deadline = Date.now() + 30000;
    while (!token) {
      if (launchError) throw launchError;
      assert.equal(server.exitCode, null, 'HTTP host exited during readiness');
      assert(Date.now() < deadline, 'HTTP host readiness deadline');
      try {
        const response = await fetch(origin + '/', { signal: AbortSignal.timeout(1500) });
        if (response.ok) token = /<meta name="readmd-app-token" content="([^"]+)"/.exec(await response.text())?.[1];
      } catch {}
      if (!token) await delay(100);
    }
    mcp = startMcp(binary, path.join(output, 'mcp-profile'), documents);
    const initialized = await mcp.request('initialize', { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'readmd-openapi-local-ref-qa', version: '1' } });
    assert(initialized.protocolVersion);
    mcp.child.stdin.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n');
    assert((await mcp.request('tools/list', {})).tools.some(tool => tool.name === 'readmd_openapi_document'));

    phase = 'mixed local references and maps';
    const first = await openapi(original), generated = first.result.content;
    assert(generated.startsWith(original)); assert(generated.includes('minLength'));
    assert(!generated.includes(output), 'No absolute fixture paths in projected document');
    assert.deepEqual(first.references.map(row => row.source).sort(), ['defs/parameters.yaml', 'defs/request.yaml', 'defs/responses.yaml', 'defs/string.yaml', 'paths/note.yaml', 'schemas/note.json']);
    assert.deepEqual(first.warnings, []);
    assert.equal(first.source_map.length, 2);
    for (const map of first.source_map) {
      assert(generated.slice(map.from).startsWith('## '));
      assert.equal(map.definition.source, 'paths/note.yaml');
      assert.equal(map.definition.pointer, '/' + map.method);
    }
    assert(generated.includes('literal-do-not-read.yaml'));
    assert.equal(fs.readFileSync(target, 'utf8'), original);
    record('JSON/YAML dependencies use their own directory; UTF-16 maps identify root calls and real external definitions');

    phase = 'HTTP and MCP parity';
    assert.deepEqual(await mcpApi(original), first);
    record('HTTP and real MCP stdio return the same projection, transaction, revisions and source maps');

    phase = 'single reversible transaction';
    const applied = await http('POST', '/api/documents/transaction', { content: original, transaction: first.transaction });
    assert.equal(applied.result.content, generated);
    const undone = await mcp.tool('readmd_content_transaction', { content: generated,
      transaction: { origin: 'openapi', base_revision: hash(generated), edits: applied.result.inverse } });
    assert.equal(undone.result.content, original);
    assert.equal(fs.readFileSync(target, 'utf8'), original);
    record('One shared content transaction applies and its returned inverse restores the complete original before any save');

    phase = 'guarded save and reopen';
    const read = await http('GET', '/api/file?p=' + encodeURIComponent(target));
    const saved = await http('POST', '/api/save', { path: target, content: generated, encoding: 'utf-8', expected_mtime: read.mtime, expected_revision: read.revision });
    assert.equal(saved.ok, true); assert.equal(fs.readFileSync(target, 'utf8'), generated);
    const reopened = await http('GET', '/api/file?p=' + encodeURIComponent(target));
    assert.equal(reopened.original || reopened.content, generated);
    assert.equal((await openapi(generated)).result.changed, false);
    record('The existing Rust writer saves explicitly; actual disk bytes reopen and regenerate idempotently');

    phase = 'dependency-only stale preview';
    fs.writeFileSync(stringFile, 'type: string\nminLength: 4\n');
    assert.equal(hash(fs.readFileSync(source)), hash(specification), 'Root source stayed unchanged');
    assert.equal((await openapi(original, { expected_source_revision: first.source_revision }, 409)).error_code, 'transaction_conflict');
    await mcpApi(original, { expected_source_revision: first.source_revision }, 'transaction_conflict');
    assert.equal(fs.readFileSync(target, 'utf8'), generated);
    record('A dependency-only edit rejects the old preview over HTTP and MCP without touching the target');

    phase = 'update, idempotence and manual protection';
    const notes = '\r\n\r\n## Author tail\r\nKeep the author commentary.\r\n';
    const revised = await openapi(generated + notes);
    assert.equal(revised.changes.length, 2);
    assert(revised.changes.every(change => change.kind === 'changed'));
    assert(revised.result.content.endsWith(notes));
    assert.notEqual(revised.source_revision, first.source_revision);
    assert.equal((await openapi(revised.result.content)).result.changed, false);
    const manual = generated.replace('Returned note', 'Manually revised note');
    assert.equal((await openapi(manual, {}, 422)).error_code, 'openapi_local_changes');
    assert.equal(fs.readFileSync(target, 'utf8'), generated);
    record('Dependency updates produce operation changes, preserve outside prose and refuse to overwrite local generated-region edits');

    phase = 'missing dependency appears';
    const missingSource = path.join(documents, 'missing.json');
    const responseSpec = reference => ({ openapi: '3.1.1', info: { title: 'Reference boundary', version: '1' }, paths: { '/x': { get: { responses: { 200: { $ref: reference } } } } } });
    fs.writeFileSync(missingSource, JSON.stringify(responseSpec('defs/optional.yaml')));
    const missingArgs = { file_path: missingSource, content: '', base_revision: hash(''), locale: 'en' };
    const missing = await http('POST', '/api/developer/openapi', missingArgs);
    assert(missing.warnings.some(row => row.code === 'missing_reference'));
    fs.writeFileSync(path.join(documents, 'defs/optional.yaml'), 'description: Newly created dependency\n');
    assert.equal((await http('POST', '/api/developer/openapi', { ...missingArgs, expected_source_revision: missing.source_revision }, 409)).error_code, 'transaction_conflict');
    record('A newly created missing dependency also invalidates the reviewed source receipt');

    phase = 'directory containment and native link';
    const outside = path.join(output, 'outside');
    fs.writeFileSync(path.join(outside, 'secret.yaml'), 'description: OUTSIDE_REFERENCE_CONTENT_MUST_STAY_PRIVATE\n');
    const link = path.join(documents, 'linked');
    fs.symlinkSync(outside, link, process.platform === 'win32' ? 'junction' : 'dir');
    const deniedFile = path.join(documents, 'denied.json');
    for (const reference of ['../outside/secret.yaml', '%2e%2e/outside/secret.yaml', 'linked/secret.yaml', 'https://example.invalid/schema.yaml', 'file:///outside.yaml', 'C:/outside.yaml']) {
      fs.writeFileSync(deniedFile, JSON.stringify(responseSpec(reference)));
      const result = await http('POST', '/api/developer/openapi', { ...missingArgs, file_path: deniedFile });
      assert(result.warnings.some(row => row.code === 'external_reference'));
      assert.deepEqual(result.references, []);
      assert(!JSON.stringify(result).includes('OUTSIDE_REFERENCE_CONTENT_MUST_STAY_PRIVATE'));
    }
    assert.equal(fs.readFileSync(path.join(outside, 'secret.yaml'), 'utf8'), 'description: OUTSIDE_REFERENCE_CONTENT_MUST_STAY_PRIVATE\n');
    record('Canonical containment rejects parent, encoded-parent, native junction/symlink, URL, file URI and drive escapes');

    phase = 'cycles and byte budget';
    fs.writeFileSync(path.join(documents, 'defs/cycle.yaml'), "A: {$ref: '#/B'}\nB: {$ref: '#/A'}\n");
    fs.writeFileSync(deniedFile, JSON.stringify(responseSpec('defs/cycle.yaml#/A')));
    const cycle = await http('POST', '/api/developer/openapi', { ...missingArgs, file_path: deniedFile });
    assert(cycle.warnings.some(row => row.code === 'recursive_reference'));
    assert(cycle.result.content.includes('defs/cycle.yaml#/A'));
    fs.writeFileSync(path.join(documents, 'defs/large.yaml'), 'x'.repeat(8 * 1024 * 1024 + 1));
    fs.writeFileSync(deniedFile, JSON.stringify(responseSpec('defs/large.yaml')));
    assert.equal((await http('POST', '/api/developer/openapi', { ...missingArgs, file_path: deniedFile }, 422)).error_code, 'openapi_budget');
    record('Cycles retain a qualified reference and real oversized dependency bytes return the hard budget error');

    phase = 'single-file compatibility';
    const standalone = JSON.stringify({ openapi: '3.0.4', info: { title: 'Single file', version: '1' }, paths: { '/x': { get: { responses: { 200: { $ref: '#/components/responses/Ok' } } } } }, components: { responses: { Ok: { description: 'Standalone response' } } } });
    const standaloneFile = path.join(documents, 'single.json'); fs.writeFileSync(standaloneFile, standalone);
    const sourceArgs = { source: standalone, source_name: 'single.json', source_id: 'fixture/single.json', content: '', base_revision: hash(''), locale: 'en' };
    const inline = await http('POST', '/api/developer/openapi', sourceArgs);
    const fromFile = await http('POST', '/api/developer/openapi', { ...sourceArgs, file_path: standaloneFile });
    assert.deepEqual(fromFile, inline); assert.equal(fromFile.source_revision, hash(standalone));
    assert.deepEqual(fromFile.references, []);
    record('OpenAPI 3.0 single-file projection, generated region and raw source revision remain byte-compatible');

    phase = 'MCP normal EOF'; await mcp.finish(); mcp = null;
    assert.equal(fs.readFileSync(target, 'utf8'), generated);
    assert.equal(hash(fs.readFileSync(source)), hash(specification));
    report.passed = true;
  } catch (error) {
    report.failedStage = phase;
    report.error = String(error.message).replaceAll(output, '<fixture>').replaceAll(ROOT, '<workspace>').slice(0, 1200);
    throw error;
  } finally {
    // Tokens remain in memory; no wire headers, user configuration or provider
    // state is retained. Headless HTTP shutdown is not a lifecycle certification.
    token = undefined;
    if (mcp && mcp.child.exitCode === null) mcp.child.kill();
    if (server && server.exitCode === null) server.kill();
    if (serverClosed) {
      let timer;
      report.httpProcessExit = await Promise.race([serverClosed, new Promise(resolve => { timer = setTimeout(() => resolve('owned host stop deadline'), 10000); })]).finally(() => clearTimeout(timer));
    }
    report.finishedAt = new Date().toISOString();
    const file = path.join(output, 'report.json');
    fs.writeFileSync(file, JSON.stringify(report, null, 2) + '\n');
    console.log(JSON.stringify({ passed: report.passed, checks: report.checks.length, failedStage: report.failedStage,
      report: path.relative(ROOT, file).replaceAll(path.sep, '/') }));
  }
}
if (require.main === module) {
  if (process.argv.includes('--plan')) console.log(JSON.stringify(plan(), null, 2));
  else run().catch(() => { console.error('OpenAPI local-reference validation failed; the first failing stage is retained.'); process.exitCode = 1; });
}
module.exports = { plan };
