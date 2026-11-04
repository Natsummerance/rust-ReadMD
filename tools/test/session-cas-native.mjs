// Genuine two-process CAS. A failed invocation is never retried into a pass.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {execFileSync, spawnSync} from 'node:child_process';

const exact = 'settings_persistence::tests::session_two_processes_with_one_expected_version_have_exactly_one_winner';
const controls = ['legacy-shared-delete', 'held-two-process', 'denied-delete', 'post-publish-sync'];
const marker = 'Owned Windows CAS controls passed: ' + controls.join(', ');
const inputs = ['rust/readmd-kernel/src/settings_persistence.rs', 'rust/readmd-kernel/src/settings_persistence_tests.rs',
  'rust/readmd-kernel/src/capability_packs/legacy.rs', 'rust/Cargo.toml', 'rust/readmd-kernel/Cargo.toml', 'rust/Cargo.lock', 'tools/test/session-cas-native.mjs'];
const hash = value => crypto.createHash('sha256').update(value).digest('hex');
function binding(repo) {
  const git = args => execFileSync('git', args, {cwd:repo, encoding:'utf8', maxBuffer:2 * 1024 * 1024}).trim();
  const status = git(['status', '--porcelain=v1', '--untracked-files=all']);
  return {head:git(['rev-parse', 'HEAD']), tree:git(['rev-parse', 'HEAD^{tree}']), clean:status === '',
    statusSha256:hash(status), sourceHashes:Object.fromEntries(inputs.map(file => [file, hash(fs.readFileSync(path.join(repo, file)))]))};
}
function tail(file) {
  const fd = fs.openSync(file, 'r');
  try { const size = fs.fstatSync(fd).size, bytes = Buffer.alloc(Math.min(size, 256 * 1024));
    fs.readSync(fd, bytes, 0, bytes.length, size - bytes.length); return bytes.toString('utf8');
  } finally { fs.closeSync(fd); }
}
function retain(out, stdout, stderr, detail) {
  fs.renameSync(stdout, path.join(out, 'first-failure.stdout.log'));
  fs.renameSync(stderr, path.join(out, 'first-failure.stderr.log'));
  const target = fs.openSync(path.join(out, 'first-failure.log'), 'wx');
  const streams = {};
  try { for (const name of ['stdout', 'stderr']) {
    const file = path.join(out, 'first-failure.' + name + '.log'), source = fs.openSync(file, 'r');
    const digest = crypto.createHash('sha256'), chunk = Buffer.alloc(64 * 1024); let bytes = 0, count;
    try { while ((count = fs.readSync(source, chunk, 0, chunk.length, null)) > 0) {
      digest.update(chunk.subarray(0, count)); let written = 0;
      while (written < count) written += fs.writeSync(target, chunk, written, count - written);
      bytes += count;
    } } finally { fs.closeSync(source); }
    streams[name] = {bytes, sha256:digest.digest('hex')};
  } } finally { fs.closeSync(target); }
  fs.writeFileSync(path.join(out, 'first-failure.json'), JSON.stringify({...detail, streams, capture:'file descriptors; no maxBuffer truncation'}, null, 2) + '\n', {flag:'wx'});
}

if (process.argv.length !== 2 || !process.env.CI || !process.env.GITHUB_WORKSPACE || !/^[a-f0-9]{40}$/.test(process.env.GITHUB_SHA || '')) throw Error('Exact CI checkout required');
const repo = fs.realpathSync(process.cwd());
if (fs.realpathSync(process.env.GITHUB_WORKSPACE) !== repo) throw Error('CI workspace mismatch');
const cache = path.join(repo, '.cache'), out = path.join(cache, 'session-cas-native');
if (fs.existsSync(cache) && (fs.lstatSync(cache).isSymbolicLink() || fs.realpathSync(cache) !== cache)) throw Error('Owned cache required');
if (fs.existsSync(out)) throw Error('CAS evidence already exists');
fs.mkdirSync(out, {recursive:true});
const repetitions = process.platform === 'win32' ? 128 : 16;
const report = {passed:false, platform:process.platform, sourceCommit:process.env.GITHUB_SHA, modelsRequested:false,
  expectedRepeatedPasses:repetitions, passedInvocations:0, exactTest:exact,
  windowsControls:process.platform === 'win32' ? controls : [], windowsControlsInvocations:0};
let currentStdout, currentStderr;
try {
  report.sourceBefore = binding(repo);
  if (!report.sourceBefore.clean || report.sourceBefore.head !== report.sourceCommit) throw Error('ci_source_not_clean_or_exact');
  for (let i = 0; i < repetitions; i++) {
    currentStdout = path.join(out, 'current.stdout.log'); currentStderr = path.join(out, 'current.stderr.log');
    const stdoutFd = fs.openSync(currentStdout, 'wx'), stderrFd = fs.openSync(currentStderr, 'wx'); let result;
    try { result = spawnSync('cargo', ['test', '--offline', '--locked', '--manifest-path', 'rust/Cargo.toml', '-p', 'readmd-kernel', '--lib', exact,
      '--', '--exact', '--test-threads=1', '--nocapture'], {cwd:repo, stdio:['ignore', stdoutFd, stderrFd], windowsHide:true});
    } catch (error) { result = {status:null, error}; }
    finally { fs.closeSync(stdoutFd); fs.closeSync(stderrFd); }
    const output = tail(currentStdout), exactPass = /^test result: ok\. 1 passed; 0 failed;/m.test(output);
    const controlMarkers = Object.fromEntries(controls.map(name => [name, output.includes(marker) && output.includes(name)]));
    const allControls = process.platform !== 'win32' || controls.every(name => controlMarkers[name]);
    const after = binding(repo);
    const unchanged = after.clean && after.head === report.sourceCommit && JSON.stringify(after) === JSON.stringify(report.sourceBefore);
    if (result.status !== 0 || result.error || result.signal || !exactPass || !allControls || !unchanged) {
      report.failedInvocation = i + 1;
      retain(out, currentStdout, currentStderr, {invocation:i + 1, status:result.status, signal:result.signal || null,
        spawnErrorCode:result.error?.code || null, exactPass, controlMarkers:process.platform === 'win32' ? controlMarkers : null, sourceUnchanged:unchanged});
      currentStdout = currentStderr = null;
      throw Error('actual_session_cas_invocation_failed');
    }
    fs.unlinkSync(currentStdout); fs.unlinkSync(currentStderr); currentStdout = currentStderr = null;
    report.passedInvocations++;
    if (process.platform === 'win32') report.windowsControlsInvocations++;
    console.log('Actual two-process session CAS passed: ' + (i + 1) + '/' + repetitions);
  }
  report.passed = true;
} catch (error) {
  report.failureCode = /^[a-z0-9_]+$/.test(error.message || '') ? error.message : 'session_cas_driver_failed';
  if (!fs.existsSync(path.join(out, 'first-failure.json'))) {
    if (!currentStdout || !fs.existsSync(currentStdout)) { currentStdout = path.join(out, 'current.stdout.log'); fs.writeFileSync(currentStdout, Buffer.alloc(0), {flag:'wx'}); }
    if (!currentStderr || !fs.existsSync(currentStderr)) { currentStderr = path.join(out, 'current.stderr.log'); fs.writeFileSync(currentStderr, Buffer.alloc(0), {flag:'wx'}); }
    retain(out, currentStdout, currentStderr, {stage:'driver', code:report.failureCode});
  }
  process.exitCode = 1;
} finally {
  try { report.sourceAfter = binding(repo); report.sourceUnchanged = JSON.stringify(report.sourceBefore) === JSON.stringify(report.sourceAfter); }
  catch { report.sourceUnchanged = false; }
  if (!report.sourceUnchanged) { report.passed = false; process.exitCode = 1; }
  fs.writeFileSync(path.join(out, 'report.json'), JSON.stringify(report, null, 2) + '\n', {flag:'wx'});
  console.log(JSON.stringify(report));
}
