'use strict';
// Owned real Unix HTTP process and timer. No native GUI, model or user profile.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), assert = require('node:assert/strict');
const { execFileSync, spawnSync } = require('node:child_process');
const { startKernel } = require('./v006-refactor-host.cjs');
const { options } = require('./v006-table-clipboard-kernel.cjs');
const ROOT = path.resolve(__dirname, '../..');
async function main() {
const args = options(process.argv.slice(2)); assert(args.bin && !args.base);
if (!['darwin','linux'].includes(process.platform)) throw Error('Unix normal-signal acceptance only');
const binary = args.bin, output = args.evidence;
const profile = path.join(output, 'profile'), docs = path.join(output, 'documents'), tmp = path.join(output, 'tmp'), home = path.join(output, 'native-home');
for (const dir of [profile, docs, tmp, home]) fs.mkdirSync(dir, { mode: 0o700 });
const code = 'setTimeout(() => console.log("LATE"), 60000);', hash = value => crypto.createHash('sha256').update(value).digest('hex');
process.env.TMPDIR = tmp; process.env.CFFIXED_USER_HOME = home;
const report = { passed: false, managedRuntimeCertified: false, hostNodeTestFixture: true, nativeUiCertified: false, input: 'one owned Node timer; normal SIGTERM of its dedicated headless kernel', binarySha256: hash(fs.readFileSync(binary)), modelsRequested: false, hostPid: null, workerPid: null, workerStoppedWithinTwoSeconds: false, cleanup: false };
let host, worker, pending;
const pause = ms => new Promise(r => setTimeout(r, ms));
function ps(pid, field) { const r = spawnSync('/bin/ps', ['-ww', '-p', String(pid), '-o', field + '='], { encoding: 'utf8', timeout: 3000 }); return r.status === 0 ? r.stdout.trim() : null; }
function sameWorker() { return worker && ps(worker.pid, 'lstart') === worker.birth && ps(worker.pid, 'args') === worker.args; }
try {
  host = await startKernel(binary, profile, docs); report.hostPid = host.pid;
  pending = host.api('/api/code/run', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ confirm: true, lang: 'javascript', code, timeout: 10, task_id: 'owned-host-exit-probe' }) }).then(r => r.json()).catch(() => ({ transportClosed: true }));
  const end = Date.now() + 3000;
  while (!worker && Date.now() < end) {
    const children = execFileSync('/bin/ps', ['-axo', 'pid=,ppid='], { encoding: 'utf8' }).trim().split('\n').map(line => line.trim().split(/\s+/).map(Number)).filter(([, parent]) => parent === host.pid).map(([pid]) => pid);
    for (const pid of children) {
      const args = ps(pid, 'args'), script = args?.match(/(\/[^\s]+\/main\.js)\s*$/)?.[1];
      if (!script?.startsWith(tmp + '/') || !script.includes('/readmd-script-')) continue;
      if (Number(ps(pid, 'pgid')) !== pid || !fs.lstatSync(script).isFile() || fs.lstatSync(script).isSymbolicLink() || hash(fs.readFileSync(script)) !== hash(code)) throw Error('Controlled worker identity mismatch');
      worker = { pid, args, birth: ps(pid, 'lstart'), script }; report.workerPid = pid; break;
    }
    if (!worker) await pause(20);
  }
  if (!worker) throw Error('Owned timer worker did not start');
  await host.stop(); assert.equal(host.exitCode, 0, 'normal host exit status'); host = null;
  const deadline = Date.now() + 2000; while (sameWorker() && Date.now() < deadline) await pause(25);
  report.workerStoppedWithinTwoSeconds = !sameWorker();
  if (!report.workerStoppedWithinTwoSeconds) throw Error('Owned code worker survived normal host exit');
  assert.notEqual((await pending).ok, true, 'exit must not return a successful late result'); report.passed = true;
} catch (error) { report.failure = String(error.message).replaceAll(output, '<evidence>').replaceAll(ROOT, '<repo>'); process.exitCode = 1; }
finally {
  if (host) await host.stop();
  if (sameWorker()) {
    if (Number(ps(worker.pid, 'pgid')) !== worker.pid) throw Error('Owned group changed; refuse cleanup');
    process.kill(-worker.pid, 'SIGKILL');
    const end = Date.now() + 2000; while (sameWorker() && Date.now() < end) await pause(20);
    report.cleanup = !sameWorker();
  } else report.cleanup = true;
  if (pending) { const outcome = await pending; report.requestOutcome = { ok: outcome.ok, errorCode: outcome.error_code, exitCode: outcome.exit_code, transportClosed: outcome.transportClosed }; }
  if (worker && report.cleanup) {
    const dir = path.dirname(worker.script);
    if (fs.existsSync(dir) && fs.readdirSync(dir).length === 1 && fs.readdirSync(dir)[0] === 'main.js' && !fs.lstatSync(worker.script).isSymbolicLink() && hash(fs.readFileSync(worker.script)) === hash(code)) fs.rmSync(dir, { recursive: true });
  }
  fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
  console.log(JSON.stringify(report));
}

}
if (require.main === module) main().catch(() => { console.error('Owned Unix exit acceptance failed; inspect its receipt.'); process.exitCode = 1; });
