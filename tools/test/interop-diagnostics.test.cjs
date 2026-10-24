'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const childProcess = require('node:child_process');
const { ROOT, parseArguments, publicValue, summarizeHistoricalAttempt } = require('./interop-diagnostics.cjs');

test('static diagnostics cannot opt into Office or GUI activation', () => {
  assert.deepEqual(parseArguments([]), { timeoutMs: 20000, cliVersions: false });
  assert.equal(parseArguments(['--cli-versions']).cliVersions, true);
  for (const args of [['--word'], ['--gui'], ['--timeout-ms', 'Infinity'], ['--timeout-ms'], ['--timeout-ms', '31000']]) assert.throws(() => parseArguments(args));
});

test('public diagnostic artifacts recursively remove executable paths and private absolute values', () => {
  const original = { applications: [{ id: 'calibre', executables: [{ fileName: 'ebook-convert.exe', _path: 'X:\\Private\\ebook-convert.exe', version: '9.16' }] }], nested: { arbitrary: 'X:\\Private\\name', unc: '\\\\server\\personal', _secret: 'hidden' } };
  const publicReport = publicValue(original);
  const text = JSON.stringify(publicReport);
  assert.equal(text.includes('Private'), false);
  assert.equal(text.includes('personal'), false);
  assert.equal(text.includes('hidden'), false);
  assert.equal(publicReport.applications[0].executables[0].fileName, 'ebook-convert.exe');
  assert.equal(original.applications[0].executables[0]._path, 'X:\\Private\\ebook-convert.exe');
});

test('historical timeout classification preserves uncertainty and actual ownership/artifact facts', () => {
  const cache = path.join(ROOT, '.cache/capability-upgrade/longtail-20261019/interop');
  fs.mkdirSync(cache, { recursive: true });
  const directory = fs.mkdtempSync(path.join(cache, 'unit-history-'));
  const names = ['report.json', 'word-result.json', 'word-owner.json', 'Word-roundtrip.docx'];
  try {
    fs.writeFileSync(path.join(directory, names[0]), JSON.stringify({ recordedAt: '2026-10-01T00:00:00Z', word: { failure: 'WORD_WORKER_TIMEOUT', timedOutAt: 'com-activation', elapsedMs: 45040, cleanup: { state: 'ownership_not_established' } } }));
    fs.writeFileSync(path.join(directory, names[1]), JSON.stringify({ stage: 'com-activation' }));
    const before = summarizeHistoricalAttempt(directory);
    assert.equal(before.classification, 'activation_did_not_return_before_worker_deadline');
    assert.equal(before.ownershipRecorded, false);
    assert.equal(before.artifactCount, 0);
    assert.equal(before.causeEstablished, false);
    assert.equal(before.historicalPostWordCount, null);
    fs.writeFileSync(path.join(directory, names[2]), '{}');
    fs.writeFileSync(path.join(directory, names[3]), 'synthetic');
    const after = summarizeHistoricalAttempt(directory);
    assert.equal(after.ownershipRecorded, true);
    assert.equal(after.artifactCount, 1);
    assert.equal(after.causeEstablished, false);
    fs.writeFileSync(path.join(directory, names[1]), JSON.stringify({ stage: 'save' }));
    assert.equal(summarizeHistoricalAttempt(directory).classification, 'historical_attempt_requires_review');
  } finally {
    for (const name of names) { const file = path.join(directory, name); if (fs.existsSync(file)) fs.unlinkSync(file); }
    fs.rmdirSync(directory);
  }
});

test('actual PowerShell preflight validator rejects wrong targets, redirects and incomplete evidence', { skip: process.platform !== 'win32' }, t => {
  const source = fs.readFileSync(path.join(ROOT, 'tools/test/interop-word-worker.ps1'), 'utf8');
  const start = source.indexOf('function Test-WordComPreflightSnapshot(');
  const end = source.indexOf('\nfunction Get-WordComPreflight', start);
  assert.ok(start >= 0 && end > start);
  const validateSource = source.slice(start, end);
  const entry = { progId: 'Word.Application', clsidRegistered: true, localServerRegistered: true, serverExecutableName: 'WINWORD.EXE', serverExecutableExists: true, treatAsRegistered: false, inprocServerRegistered: false };
  const snapshot = overrides => ({ processArchitectureBits: 64, entries: [entry], chainComplete: true, readError: false, ...overrides });
  const fixtures = [
    { value: snapshot({}), expected: null },
    { value: snapshot({ processArchitectureBits: 32, entries: [{ ...entry, serverExecutableName: 'wps.exe' }] }), expected: 'WORD_COM_TARGET_MISMATCH' },
    { value: snapshot({ entries: [{ ...entry, treatAsRegistered: true }] }), expected: 'WORD_COM_TARGET_MISMATCH' },
    { value: snapshot({ entries: [{ ...entry, inprocServerRegistered: true }] }), expected: 'WORD_COM_TARGET_MISMATCH' },
    { value: snapshot({ chainComplete: false }), expected: 'WORD_COM_REDIRECT_UNVERIFIED' },
    { value: snapshot({ entries: [{ ...entry, clsidRegistered: false }] }), expected: 'WORD_COM_REGISTRATION_UNVERIFIED' },
    { value: snapshot({ entries: [entry, { ...entry, progId: 'Word.Application.16', serverExecutableName: 'wps.exe' }] }), expected: 'WORD_COM_TARGET_MISMATCH' },
    { value: snapshot({ entries: [entry, { ...entry, progId: 'Word.Application.16' }] }), expected: null },
    { value: snapshot({ readError: true }), expected: 'WORD_COM_REGISTRATION_UNVERIFIED' }
  ];
  const encodedFixtures = Buffer.from(JSON.stringify(fixtures), 'utf8').toString('base64');
  const command = validateSource + '\n$fixtures = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String(\'' + encodedFixtures + '\')) | ConvertFrom-Json; $clock = [Diagnostics.Stopwatch]::StartNew(); $outputs = @($fixtures | ForEach-Object { Test-WordComPreflightSnapshot $_.value }); $clock.Stop(); @{ outputs = $outputs; validatorElapsedMs = $clock.ElapsedMilliseconds } | ConvertTo-Json -Depth 10 -Compress';
  const bootstrap = '$source = [Console]::In.ReadToEnd(); & ([ScriptBlock]::Create($source))';
  // Host startup has its own bounded budget; it is not the validator's execution time.
  const began = performance.now();
  const result = childProcess.spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-EncodedCommand', Buffer.from(bootstrap, 'utf16le').toString('base64')], { input: command, cwd: ROOT, windowsHide: true, timeout: 20000, encoding: 'utf8', maxBuffer: 256 * 1024 });
  assert.equal(result.status, 0, 'PowerShell host completed within 20s: ' + (result.error?.code || result.signal || result.stderr));
  const { outputs, validatorElapsedMs } = JSON.parse(result.stdout.trim());
  t.diagnostic(JSON.stringify({ hostElapsedMs: Math.round(performance.now() - began), validatorElapsedMs }));
  assert.ok(Number.isFinite(validatorElapsedMs) && validatorElapsedMs < 5000, 'pure snapshot validator completed within its unchanged 5s budget');
  assert.equal(outputs.length, fixtures.length);
  outputs.forEach((output, index) => {
    assert.equal(output.failure, fixtures[index].expected);
    assert.equal(output.accepted, fixtures[index].expected === null);
    assert.equal(output.activated, false);
    assert.equal(output.registrationScope, 'ClassesRoot/effective-current-process-view');
  });
});

test('VS Code plan checks real repository layout without importing the graphical driver', async () => {
  const launcherFile = path.join(ROOT, 'ui-tests/run-vscode-openapi-native.cjs');
  const originalSpawn = childProcess.spawn;
  let spawnCalls = 0;
  childProcess.spawn = () => { spawnCalls++; throw Error('GUI launch is forbidden in plan test'); };
  let launcher;
  try {
    delete require.cache[require.resolve(launcherFile)];
    launcher = require(launcherFile);
    const plan = launcher.buildLaunchPlan({ environment: { READMD_BIN: process.execPath, READMD_VSCODE_BIN: process.execPath } });
    assert.equal(launcher.ROOT, ROOT);
    assert.equal(plan.ready, true);
    assert.ok(plan.checks.every(check => check.exists));
    assert.ok(plan.args.includes('--extensionDevelopmentPath=' + path.join(ROOT, 'packages/vscode-extension')));
    assert.ok(plan.args.includes('--extensionTestsPath=' + path.join(ROOT, 'ui-tests/vscode-openapi-native.cjs')));
    assert.equal(plan.env.READMD_ASSETS_DIR, path.join(ROOT, 'assets'));
    const summary = launcher.publicPlan(plan);
    assert.equal(summary.launched, false);
    assert.equal(summary.acceptance, 'not_run');
    assert.equal(summary.requiresGraphicalDriver, true);
    assert.equal(JSON.stringify(summary).includes(process.execPath), false);
    const missing = launcher.buildLaunchPlan({ environment: {} });
    assert.equal(missing.ready, false);
    assert.deepEqual(missing.checks.filter(check => !check.exists).map(check => check.id), ['readmd-binary', 'vscode-binary']);
    assert.equal(spawnCalls, 0);
  } finally { childProcess.spawn = originalSpawn; delete require.cache[require.resolve(launcherFile)]; }
});

test('VS Code --plan performs no filesystem writes or application starts', async () => {
  const launcherFile = path.join(ROOT, 'ui-tests/run-vscode-openapi-native.cjs');
  const originalSpawn = childProcess.spawn;
  const originalMkdir = fs.mkdirSync;
  const originalMkdtemp = fs.mkdtempSync;
  const originalWrite = fs.writeFileSync;
  const originalLog = console.log;
  const originalExitCode = process.exitCode;
  let logged;
  try {
    childProcess.spawn = () => { throw Error('GUI launch is forbidden in plan test'); };
    fs.mkdirSync = fs.mkdtempSync = fs.writeFileSync = () => { throw Error('Plan cannot write files'); };
    console.log = value => { logged = JSON.parse(value); };
    delete require.cache[require.resolve(launcherFile)];
    await require(launcherFile).main(['--plan']);
    assert.equal(logged.mode, 'launch-plan-only');
    assert.equal(logged.filesystemChanged, false);
    assert.equal(logged.launched, false);
    assert.ok(logged.checks.filter(check => !check.id.endsWith('-binary')).every(check => check.exists));
  } finally {
    childProcess.spawn = originalSpawn; fs.mkdirSync = originalMkdir; fs.mkdtempSync = originalMkdtemp; fs.writeFileSync = originalWrite;
    console.log = originalLog; process.exitCode = originalExitCode; delete require.cache[require.resolve(launcherFile)];
  }
});
