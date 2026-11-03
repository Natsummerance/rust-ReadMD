'use strict';
// Actual stdio shutdown on all three platforms. The only external worker is
// our fixed Node timer, with verified script bytes and parent/birth identity.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), assert = require('node:assert/strict');
const { spawn, spawnSync, execFileSync } = require('node:child_process'), { options } = require('./v006-table-clipboard-kernel.cjs');
const ROOT = path.resolve(__dirname, '../..'), pause = ms => new Promise(resolve => setTimeout(resolve, ms));
const sha = data => crypto.createHash('sha256').update(data).digest('hex');
const TIMER = 'setTimeout(() => console.log("LATE"), 60000);';
const SQL = 'WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<1000000000) SELECT sum(x) FROM n;';
function ps(pid, field) { const r = spawnSync('/bin/ps', ['-ww', '-p', String(pid), '-o', field + '='], { encoding: 'utf8', timeout: 3000 }); return r.status === 0 ? r.stdout.trim() : null; }
function windowsProcesses(filter) {
  const script = `Get-CimInstance Win32_Process -Filter '${filter}' | Select-Object ProcessId,ParentProcessId,CreationDate,CommandLine | ConvertTo-Json -Compress`;
  const raw = execFileSync('powershell.exe', ['-NoLogo', '-NoProfile', '-NonInteractive', '-Command', script], { encoding: 'utf8', timeout: 10000 }).trim();
  const value = raw ? JSON.parse(raw) : null; return value ? Array.isArray(value) ? value : [value] : [];
}
function processes(parent) {
  if (process.platform === 'win32') return windowsProcesses('ParentProcessId = ' + parent).map(v => ({ pid: v.ProcessId, birth: String(v.CreationDate), args: v.CommandLine }));
  return execFileSync('/bin/ps', ['-axo', 'pid=,ppid='], { encoding: 'utf8' }).trim().split('\n').map(line => line.trim().split(/\s+/).map(Number)).filter(([, p]) => p === parent).map(([pid]) => ({ pid, birth: ps(pid, 'lstart'), args: ps(pid, 'args') }));
}
function sameWorker(worker) {
  if (!worker) return false;
  if (process.platform === 'win32') { const value = windowsProcesses('ProcessId = ' + worker.pid)[0]; return value && String(value.CreationDate) === worker.birth && value.CommandLine === worker.args; }
  return ps(worker.pid, 'lstart') === worker.birth && ps(worker.pid, 'args') === worker.args;
}
async function fixture(args, kind) {
  const root = path.join(args.evidence, kind); fs.mkdirSync(root);
  const dirs = Object.fromEntries(['profile', 'documents', 'tmp', 'home'].map(name => [name, path.join(root, name)])); for (const dir of Object.values(dirs)) fs.mkdirSync(dir);
  const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !/TOKEN|SECRET|CREDENTIAL|API_KEY|PROXY/i.test(key)));
  Object.assign(env, { READMD_DATA_DIR: dirs.profile, TMPDIR: dirs.tmp, TMP: dirs.tmp, TEMP: dirs.tmp, CFFIXED_USER_HOME: dirs.home });
  const child = spawn(args.bin, ['--mcp', '--data-dir', dirs.profile, '--workspace', dirs.documents, '--assets', path.join(ROOT, 'assets')], { cwd: ROOT, env, stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
  const closed = new Promise(resolve => child.once('close', (code, signal) => resolve({ code, signal })));
  let initialize, init = new Promise(resolve => { initialize = resolve; }), buffer = '', outcome, outcomeCount = 0, worker, protocolError, stderrBytes = 0, forced = false;
  child.on('error', () => { protocolError = 'spawn_failed'; }); child.stderr.on('data', bytes => { stderrBytes += bytes.length; }); child.stdout.setEncoding('utf8');
  child.stdin.on('error', () => { protocolError = 'stdin_closed'; });
  child.stdout.on('data', data => {
    try {
      buffer += data; if (buffer.length > 1024 * 1024) throw Error('output_budget');
      for (;;) { const end = buffer.indexOf('\n'); if (end < 0) break; const line = buffer.slice(0, end); buffer = buffer.slice(end + 1); if (!line.trim()) continue;
        const message = JSON.parse(line); if (message.id === 1) initialize(message); if (message.id === 2) { outcomeCount++; outcome = message.result?.structuredContent; } }
    } catch (_) { protocolError = 'invalid_protocol_output'; child.stdin.end(); }
  });
  const send = message => child.stdin.write(JSON.stringify({ jsonrpc: '2.0', ...message }) + '\n');
  const row = { kind, hostPid: child.pid, passed: false, modelsRequested: false, workerObserved: false };
  const fixtureStarted = Date.now();
  const watchdog = setTimeout(() => { if (child.exitCode === null && child.signalCode === null) { forced = true; child.kill(); } }, 20000);
  try {
    send({ id: 1, method: 'initialize', params: { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'owned-code-eof', version: '1' } } });
    let timer; await Promise.race([init, new Promise((_, reject) => { timer = setTimeout(() => reject(Error('initialize_deadline')), 5000); })]).finally(() => clearTimeout(timer));
    send({ method: 'notifications/initialized', params: {} });
    send({ id: 2, method: 'tools/call', params: { name: 'readmd_run_code_chunk', arguments: { confirm: true, language: kind === 'sql' ? 'sql' : 'javascript', code: kind === 'sql' ? SQL : TIMER } } });
    if (kind === 'node') {
      const end = Date.now() + 6000;
      while (!worker && Date.now() < end && !outcome) {
        for (const candidate of processes(child.pid)) {
          const match = candidate.args?.match(/(?:"([^"\n]+main\.js)"|([^\s"\n]+main\.js))\s*$/), script = match?.[1] || match?.[2];
          if (!script) continue;
          const canonical = fs.realpathSync(script), prefix = fs.realpathSync(dirs.tmp) + path.sep;
          const within = process.platform === 'win32' ? canonical.toLowerCase().startsWith(prefix.toLowerCase()) : canonical.startsWith(prefix);
          if (!within || !fs.lstatSync(script).isFile() || fs.lstatSync(script).isSymbolicLink() || sha(fs.readFileSync(script)) !== sha(TIMER) || !candidate.birth) throw Error('owned_worker_identity_mismatch');
          if (process.platform !== 'win32') assert.equal(Number(ps(candidate.pid, 'pgid')), candidate.pid);
          worker = { ...candidate, script }; row.workerObserved = true; row.workerPid = worker.pid; break;
        }
        if (!worker) await pause(20);
      }
      assert(worker, 'owned_node_worker_started');
    } else await pause(100);
    const at = performance.now(); child.stdin.end(); const exit = await closed; row.eofToExitMs = performance.now() - at;
    assert.equal(protocolError, undefined); assert.equal(forced, false); assert.deepEqual(exit, { code: 0, signal: null }); assert(row.eofToExitMs < 2000, 'EOF_stop_budget');
    assert.equal(outcomeCount, 1); assert.equal(outcome?.ok, false); assert.equal(outcome?.error_code, 'execution_cancelled');
    // The existing MCP error envelope omits console channels. HTTP retains
    // an empty stdout; either representation must contain no late output.
    assert(!Object.hasOwn(outcome, 'stdout') || outcome.stdout === '');
    row.workerStopped = !sameWorker(worker); assert(row.workerStopped, 'owned_worker_stopped');
    row.temporaryScriptRemoved = !worker || !fs.existsSync(worker.script); assert(row.temporaryScriptRemoved, 'temporary_script_reclaimed'); row.passed = true;
  } catch (error) { row.failure = String(error.message).replaceAll(args.evidence, '<evidence>').replaceAll(ROOT, '<repo>').slice(0, 800); }
  finally {
    if (child.exitCode === null && child.signalCode === null) { child.stdin.end(); await closed; }
    clearTimeout(watchdog); row.forcedCleanup = forced; row.ownHostStopped = child.exitCode !== null || child.signalCode !== null; row.stderrBytes = stderrBytes;
    if (worker) {
      // A failing timer fixture has no side effects and naturally ends after
      // 60s. Never taskkill a queried PID that could have been reused.
      try {
        while (!row.passed && sameWorker(worker) && Date.now() - fixtureStarted < 65000) await pause(100);
        row.cleanupWorkerStopped = !sameWorker(worker);
      } catch (_) { row.cleanupWorkerStopped = false; }
    }
    // Retain unknown objects after failure. Successful RAII cleanup leaves the
    // controlled script absent; only these owned transient directories go.
    if (row.passed) for (const name of ['tmp', 'home']) fs.rmSync(dirs[name], { recursive: true, force: true });
    fs.writeFileSync(path.join(root, 'report.json'), JSON.stringify(row, null, 2) + '\n', { flag: 'wx' });
  }
  return row;
}
async function main() {
  const args = options(process.argv.slice(2)); assert(args.bin && !args.base);
  const report = { passed: false, platform: process.platform, modelsRequested: false, nativeUiCertified: false, managedRuntimeCertified: false,
    hostNodeTestFixture: true, binarySha256: sha(fs.readFileSync(args.bin)), checks: [] };
  try {
    for (const kind of ['sql', 'node']) report.checks.push(await fixture(args, kind));
    report.passed = report.checks.length === 2 && report.checks.every(check => check.passed && !check.forcedCleanup && check.workerStopped && check.ownHostStopped);
  } finally {
    fs.writeFileSync(path.join(args.evidence, 'report.json'), JSON.stringify(report, null, 2) + '\n', { flag: 'wx' }); console.log(JSON.stringify({ passed: report.passed, checks: report.checks })); if (!report.passed) process.exitCode = 1;
  }
}
if (require.main === module) main().catch(() => { console.error('Code MCP EOF acceptance failed; inspect the owned report.'); process.exitCode = 1; });
