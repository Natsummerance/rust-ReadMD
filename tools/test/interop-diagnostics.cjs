'use strict';
// Static inspection never activates Office or a graphical reader.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const ROOT = path.resolve(__dirname, '../..');
const OUTPUT = path.join(ROOT, '.cache/capability-upgrade/longtail-20261019/interop');
const sha256 = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const readJson = file => JSON.parse(fs.readFileSync(file, 'utf8').replace(/^\uFEFF/, ''));
const writeJson = (file, value) => fs.writeFileSync(file, JSON.stringify(value, null, 2) + '\n', { flag: 'wx' });

function parseArguments(args) {
  const options = { timeoutMs: 20000, cliVersions: false };
  for (let index = 0; index < args.length; index++) {
    const argument = args[index];
    if (argument === '--help') options.help = true;
    else if (argument === '--cli-versions') options.cliVersions = true;
    else if (argument === '--timeout-ms') options.timeoutMs = Number(args[++index]);
    else throw Error('Unsupported diagnostics argument');
  }
  if (!Number.isSafeInteger(options.timeoutMs) || options.timeoutMs < 5000 || options.timeoutMs > 30000) throw Error('Diagnostic timeout must be 5000..30000 ms');
  return options;
}

function publicValue(value) {
  if (Array.isArray(value)) return value.map(publicValue);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).filter(([key]) => !key.startsWith('_')).map(([key, item]) => [key, publicValue(item)]));
  if (typeof value === 'string' && /(?:[a-z]:[\\/]|\\\\|\/home\/)/i.test(value)) return '[absolute-path-redacted]';
  return value;
}

function inspectPe(bytes) {
  if (bytes.length < 64 || bytes.readUInt16LE(0) !== 0x5a4d) return { verified: false, reason: 'not_pe' };
  const offset = bytes.readUInt32LE(0x3c);
  if (offset > bytes.length - 94 || bytes.readUInt32LE(offset) !== 0x00004550) return { verified: false, reason: 'invalid_pe_header' };
  const machine = bytes.readUInt16LE(offset + 4);
  const subsystem = bytes.readUInt16LE(offset + 24 + 68);
  return { verified: true, machine: '0x' + machine.toString(16), subsystem, console: subsystem === 3 };
}

function consoleEnvironment(directory) {
  const folders = Object.fromEntries(['config', 'home', 'roaming', 'local', 'temp', 'cache'].map(name => [name, path.join(directory, name)]));
  for (const folder of Object.values(folders)) fs.mkdirSync(folder, { recursive: true });
  return { ...process.env, HOME: folders.home, USERPROFILE: folders.home, APPDATA: folders.roaming, LOCALAPPDATA: folders.local, TEMP: folders.temp, TMP: folders.temp,
    CALIBRE_CONFIG_DIRECTORY: folders.config, CALIBRE_CACHE_DIRECTORY: folders.cache, CALIBRE_TEMP_DIR: folders.temp, CALIBRE_NO_NATIVE_FILEDIALOGS: '1', QT_QPA_PLATFORM: 'offscreen',
    HTTP_PROXY: 'http://127.0.0.1:9', HTTPS_PROXY: 'http://127.0.0.1:9', ALL_PROXY: 'http://127.0.0.1:9', NO_PROXY: '127.0.0.1,localhost' };
}

function summarizeHistoricalAttempt(directory) {
  const reportFile = path.join(directory, 'report.json');
  const resultFile = path.join(directory, 'word-result.json');
  const report = readJson(reportFile);
  const result = readJson(resultFile);
  const word = report.word || {};
  const ownerPresent = fs.existsSync(path.join(directory, 'word-owner.json'));
  const artifactsPresent = ['Word-roundtrip.docx', 'Word-layout.pdf'].filter(file => fs.existsSync(path.join(directory, file)));
  const classification = word.failure === 'WORD_WORKER_TIMEOUT' && word.timedOutAt === 'com-activation' && result.stage === 'com-activation'
    ? 'activation_did_not_return_before_worker_deadline' : 'historical_attempt_requires_review';
  let postWordCount = null;
  const postFile = path.join(directory, 'post-run-word-processes.json');
  if (fs.existsSync(postFile)) postWordCount = readJson(postFile).wordProcessCount ?? null;
  return {
    evidenceId: path.basename(directory), reportSha256: sha256(fs.readFileSync(reportFile)), resultSha256: sha256(fs.readFileSync(resultFile)),
    recordedAt: report.recordedAt, failure: word.failure, stage: result.stage, elapsedMs: word.elapsedMs,
    ownershipRecorded: ownerPresent, artifactCount: artifactsPresent.length, cleanupState: word.cleanup?.state,
    historicalPostWordCount: postWordCount, classification, causeEstablished: false, historicalWorkerArchitectureRecorded: false,
    boundary: 'The recorded stage precedes opening a document. It cannot identify a first-run dialog, add-in, modal block or COM server failure.'
  };
}

function inspectPowerShell(timeoutMs) {
  const inventorySource = fs.readFileSync(path.join(__dirname, 'interop-inventory.ps1'), 'utf8');
  const source = fs.readFileSync(path.join(__dirname, 'interop-diagnostics.ps1'), 'utf8');
  const command = '$interopInventory = & {\n' + inventorySource + '\n} -IncludePrivatePaths | ConvertFrom-Json; & {\n' + source + '\n} -Inventory $interopInventory -BudgetMs ' + Math.max(1000, timeoutMs - 1500);
  const started = Date.now();
  // Only a short bootstrap is encoded. Reading the complete script from stdin
  // avoids both Windows' command-line bound and -Command - multiline REPL rules.
  const bootstrap = '[Console]::InputEncoding = [Text.UTF8Encoding]::new($false); $source = [Console]::In.ReadToEnd(); & ([ScriptBlock]::Create($source))';
  const result = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-EncodedCommand', Buffer.from(bootstrap, 'utf16le').toString('base64')], {
    cwd: ROOT, input: command + '\n', windowsHide: true, encoding: 'utf8', timeout: timeoutMs, maxBuffer: 2 * 1024 * 1024
  });
  const execution = { elapsedMs: Date.now() - started, timeoutMs, launcherArchitecture: process.arch, timedOut: result.error?.code === 'ETIMEDOUT', exitCode: result.status, signal: result.signal, errorCode: result.error?.code || null,
    stdoutBytes: Buffer.byteLength(result.stdout || ''), stderrBytes: Buffer.byteLength(result.stderr || ''), stdoutSha256: sha256(result.stdout || ''), stderrSha256: sha256(result.stderr || '') };
  if (result.status !== 0) return { execution, inventory: null, failure: execution.timedOut ? 'STATIC_INVENTORY_TIMEOUT' : 'STATIC_INVENTORY_FAILED' };
  try { return { execution, inventory: JSON.parse(result.stdout.trim().replace(/^\uFEFF/, '')), failure: null }; }
  catch { return { execution, inventory: null, failure: 'STATIC_INVENTORY_INVALID_JSON' }; }
}

function inspectWordPreflight(executable, timeoutMs) {
  if (!executable || !fs.existsSync(executable)) return { state: 'shell_not_detected', launchedOffice: false };
  if (timeoutMs < 100) return { state: 'total_deadline_exhausted', launchedOffice: false };
  const source = fs.readFileSync(path.join(__dirname, 'interop-word-worker.ps1'), 'utf8');
  const bootstrap = '[Console]::InputEncoding = [Text.UTF8Encoding]::new($false); $source = [Console]::In.ReadToEnd(); & ([ScriptBlock]::Create($source))';
  const started = Date.now();
  const result = spawnSync(executable, ['-NoProfile', '-NonInteractive', '-EncodedCommand', Buffer.from(bootstrap, 'utf16le').toString('base64')], {
    input: '& {\n' + source + '\n} -PreflightOnly\n', cwd: ROOT, windowsHide: true, encoding: 'utf8', timeout: Math.min(5000, timeoutMs), maxBuffer: 256 * 1024
  });
  const execution = { elapsedMs: Date.now() - started, exitCode: result.status, timedOut: result.error?.code === 'ETIMEDOUT', errorCode: result.error?.code || null };
  try {
    const preflight = JSON.parse(result.stdout.trim().replace(/^\uFEFF/, ''));
    return { state: preflight.accepted ? 'preflight_accepted_without_activation' : 'preflight_rejected_without_activation', execution, preflight: publicValue(preflight), launchedOffice: false };
  } catch { return { state: 'preflight_incomplete', execution, launchedOffice: false }; }
}

function runConsoleVersions(inventory, directory, timeoutMs) {
  const calibre = inventory.applications.find(application => application.id === 'calibre');
  const executables = calibre?.executables || [];
  const results = [];
  const deadline = Date.now() + timeoutMs;
  for (const fileName of ['ebook-convert.exe', 'ebook-polish.exe']) {
    const executable = executables.find(item => item.fileName.toLowerCase() === fileName);
    if (!executable) { results.push({ executable: fileName, state: 'not_detected_in_bounded_scope', launched: false }); continue; }
    const remaining = deadline - Date.now();
    if (remaining < 100) { results.push({ executable: fileName, state: 'total_deadline_exhausted', launched: false }); continue; }
    if (fs.statSync(executable._path).size > 2 * 1024 * 1024) { results.push({ executable: fileName, state: 'console_launcher_size_outside_scope', launched: false }); continue; }
    const binary = fs.readFileSync(executable._path);
    const identity = { ...inspectPe(binary), sha256: sha256(binary), bytes: binary.length };
    if (!identity.verified || !identity.console) { results.push({ executable: fileName, state: 'console_binary_identity_unverified', launched: false, identity }); continue; }
    const started = Date.now();
    const result = spawnSync(executable._path, ['--version'], {
      cwd: directory, windowsHide: true, encoding: 'utf8', timeout: Math.min(5000, remaining), maxBuffer: 256 * 1024,
      env: consoleEnvironment(path.join(directory, 'console-isolation'))
    });
    // Keep only a version token; arbitrary stdout/stderr (possibly private paths) is never logged.
    const version = (String(result.stdout || '') + '\n' + String(result.stderr || '')).match(/\b\d+\.\d+(?:\.\d+){0,2}\b/)?.[0] || null;
    results.push({ executable: fileName, state: result.status === 0 && version ? 'version_query_completed' : 'version_query_unverified', launched: true,
      version, identity, exitCode: result.status, elapsedMs: Date.now() - started, timedOut: result.error?.code === 'ETIMEDOUT', errorCode: result.error?.code || null });
  }
  return results;
}

function main(args = process.argv.slice(2)) {
  const options = parseArguments(args);
  if (options.help) {
    console.log('node tools/test/interop-diagnostics.cjs [--timeout-ms 20000] [--cli-versions]\nDefault: bounded read-only registry/executable/process metadata and historical evidence inspection.\n--cli-versions allows only Calibre console version queries with an isolated config directory. No GUI or Office activation is supported.');
    return;
  }
  if (process.platform !== 'win32') throw Error('Static registry diagnostics require Windows');
  const started = Date.now();
  const deadline = started + options.timeoutMs;
  fs.mkdirSync(OUTPUT, { recursive: true });
  const directory = fs.mkdtempSync(path.join(OUTPUT, 'diagnostics-'));
  const inspected = inspectPowerShell(options.timeoutMs);
  const report = {
    schemaVersion: 1, recordedAt: new Date().toISOString(), mode: 'read-only-static-diagnostics', acceptance: 'unverified', productExecuted: false,
    baselineReference: { candidate: 'candidate-12', exeSha256: '5bc70acdb75570aeeb6305fcf736f8d3f245b85bf5a9aab295af10876bcc5b41', bootSha256: '48c81634b56fe056b360e5edbaba1aeee11674f8d4e4214f92895b306db79ba9', productExecutionBoundToBaseline: false },
    execution: inspected.execution, failure: inspected.failure,
    inventory: publicValue(inspected.inventory),
    historicalWordAttempts: ['interop-ULrFZp', 'interop-lc4eXY'].map(id => summarizeHistoricalAttempt(path.join(ROOT, '.cache/capability-upgrade', id))),
    safety: { officeActivationAttempted: false, graphicalApplicationsLaunched: false, existingProcessesReusedOrStopped: false, windowOrDocumentDataInspected: false, fullCommandsOrUserPathsLogged: false },
    limitations: [
      'Registration and executable versions establish static availability only.',
      'Current registry state cannot explain an earlier COM activation timeout.',
      'I01-I07 and I09 application workflows remain unverified by this diagnostic.',
      'No system or application configuration is modified; diagnostic artifacts are written only to an owned cache directory.'
    ],
    scripts: ['interop-diagnostics.cjs', 'interop-diagnostics.ps1', 'interop-inventory.ps1', 'interop-word-worker.ps1', 'interop-acceptance.cjs'].map(file => ({ file, sha256: sha256(fs.readFileSync(path.join(__dirname, file))) }))
  };
  const windows = process.env.SystemRoot;
  report.wordComPreflights = ['System32', 'SysWOW64'].map(scope => ({ shellLocation: scope,
    ...inspectWordPreflight(windows ? path.join(windows, scope, 'WindowsPowerShell/v1.0/powershell.exe') : null, Math.max(0, deadline - Date.now())) }));
  if (options.cliVersions && inspected.inventory) report.consoleVersions = runConsoleVersions(inspected.inventory, directory, Math.max(0, deadline - Date.now()));
  report.totalElapsedMs = Date.now() - started;
  report.totalDeadlineMs = options.timeoutMs;
  writeJson(path.join(directory, 'report.json'), report);
  console.log(JSON.stringify({ report: path.relative(ROOT, path.join(directory, 'report.json')), failure: report.failure,
    applications: report.inventory?.applications.map(application => ({ id: application.id, executableCount: application.executables.length, runningCount: application.running.length, nativeAcceptance: application.nativeAcceptance })),
    consoleVersions: report.consoleVersions, wordComPreflights: report.wordComPreflights.map(item => ({ shellLocation: item.shellLocation, state: item.state, processArchitectureBits: item.preflight?.processArchitectureBits, failure: item.preflight?.failure })),
    historicalWordAttempts: report.historicalWordAttempts.map(attempt => ({ evidenceId: attempt.evidenceId, stage: attempt.stage, elapsedMs: attempt.elapsedMs, causeEstablished: attempt.causeEstablished })) }));
  if (report.failure || report.inventory?.budgetExceeded) process.exitCode = 2;
  return { directory, report, privateInventory: inspected.inventory };
}

module.exports = { ROOT, parseArguments, publicValue, inspectPe, consoleEnvironment, summarizeHistoricalAttempt, inspectPowerShell, inspectWordPreflight, main };
if (require.main === module) {
  try { main(); } catch { console.error('INTEROP_DIAGNOSTIC_FAILED'); process.exitCode = 1; }
}
