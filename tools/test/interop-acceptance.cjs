'use strict';
// External Office/readers are acceptance targets, never ReadMD runtime dependencies.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const ROOT = path.resolve(__dirname, '../..');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const readJson = file => JSON.parse(fs.readFileSync(file, 'utf8').replace(/^\uFEFF/, ''));
const writeJson = (file, value) => fs.writeFileSync(file, JSON.stringify(value, null, 2) + '\n', { flag: 'wx' });
const WORKFLOWS = {
  word: ['I01 Word equations', 'I02 Word publication', 'I03 Word return import'],
  wps: ['I04 WPS edit/save/reopen'],
  libreoffice: ['I05 Writer edit/save/reopen'],
  thorium: ['I06 Thorium import/reflow/reopen'],
  calibre: ['I07 independent ebook engine']
};
function classifyApplication(application, errors = []) {
  const running = application.running || [];
  const executables = application.executables || [];
  const registrations = application.registrations || [];
  let availability = 'not_detected';
  if (executables.length) availability = 'installed_unverified';
  else if (registrations.length) availability = 'registration_only';
  else if (errors.length) availability = 'inspection_incomplete';
  if (running.length) availability = 'user_instance_preserved';
  return {
    ...application, availability, nativeAcceptance: 'not_run',
    requirements: (WORKFLOWS[application.id] || []).map(id => ({ id, state: 'unverified' })),
    reason: running.length ? 'Existing application processes were not opened, closed, or reused.'
      : executables.length ? 'Executable/version detected; no document workflow has been verified.'
      : registrations.length ? 'Registration alone does not establish executable availability.'
      : 'Not found in the bounded discovery scope; this is not proof that the application is absent.'
  };
}
function parseArguments(args) {
  const options = { word: false };
  for (let index = 0; index < args.length; index++) {
    const key = args[index];
    if (key === '--word') options.word = true;
    else if (key === '--help') options.help = true;
    else if (['--docx', '--epub', '--timeout-ms'].includes(key)) {
      if (!args[index + 1] || args[index + 1].startsWith('--')) throw Error('Missing value for ' + key);
      options[key.slice(2)] = args[++index];
    } else throw Error('Unknown argument: ' + key);
  }
  if (options.word && !options.docx) throw Error('--word requires an explicitly selected --docx QA fixture');
  options.timeout = Number(options['timeout-ms'] || 45000);
  if (!Number.isInteger(options.timeout) || options.timeout < 5000 || options.timeout > 120000) throw Error('Timeout must be between 5000 and 120000 ms');
  return options;
}
function validateOwnership(owner) {
  return !!owner && Number.isSafeInteger(owner.processId) && owner.processId > 0
    && owner.processName === 'WINWORD' && /^[0-9]{16,20}$/.test(owner.startTicks || '');
}
function ps(file, args = [], timeout = 30000) {
  const switches = new Set(['-Workspace', '-ProcessId', '-StartTicks']);
  const invocation = args.map((value, index) => {
    if (index % 2 === 0) { if (!switches.has(value)) throw Error('Unexpected PowerShell parameter'); return value; }
    return "'" + value.replaceAll("'", "''") + "'";
  }).join(' ');
  const command = '& {\n' + fs.readFileSync(file, 'utf8') + '\n} ' + invocation;
  // Keep the actual script out of Windows' command-line length limit. stdin is
  // interpreted by the same PowerShell engine; no execution policy is changed.
  const bootstrap = '[Console]::InputEncoding = [Text.UTF8Encoding]::new($false); $source = [Console]::In.ReadToEnd(); & ([ScriptBlock]::Create($source))';
  const encoded = Buffer.from(bootstrap, 'utf16le').toString('base64');
  return spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-EncodedCommand', encoded], {
    cwd: ROOT, input: command + '\n', encoding: 'utf8', windowsHide: true, timeout, maxBuffer: 2 * 1024 * 1024
  });
}
function copyFixture(input, extension, directory) {
  const resolved = path.resolve(input);
  if (path.extname(resolved).toLowerCase() !== '.' + extension) throw Error('Expected a .' + extension + ' fixture');
  const info = fs.statSync(resolved);
  if (!info.isFile() || info.size > 128 * 1024 * 1024) throw Error('QA fixture must be a regular file at most 128 MiB');
  const bytes = fs.readFileSync(resolved);
  if (!bytes.subarray(0, 4).equals(Buffer.from([0x50, 0x4b, 0x03, 0x04]))) throw Error('Fixture is not a ZIP document');
  const name = 'source.' + extension;
  fs.writeFileSync(path.join(directory, name), bytes, { flag: 'wx' });
  return { file: name, bytes: bytes.length, sha256: sha(bytes), sourceUnmodified: true };
}
function cleanupOwnedWord(directory) {
  const file = path.join(directory, 'word-owner.json');
  if (!fs.existsSync(file)) return { state: 'ownership_not_established', action: 'No application process was terminated.' };
  let owner;
  try { owner = readJson(file); } catch { return { state: 'invalid_ownership_record', action: 'No application process was terminated.' }; }
  if (!validateOwnership(owner)) return { state: 'invalid_ownership_record', action: 'No application process was terminated.' };
  // A PID alone is unsafe: the cleanup helper also checks process name and exact start ticks.
  const result = ps(path.join(__dirname, 'interop-word-cleanup.ps1'), [
    '-ProcessId', String(owner.processId), '-StartTicks', owner.startTicks
  ], 5000);
  if (result.status !== 0) return { state: 'cleanup_unverified', action: 'No broad process cleanup attempted.' };
  try { return JSON.parse(result.stdout.trim()); } catch { return { state: 'cleanup_report_invalid' }; }
}
function runWord(directory, application, timeout) {
  if (application.availability !== 'installed_unverified') return {
    passed: false, acceptance: 'unverified', stage: 'preflight',
    failure: application.availability === 'user_instance_preserved' ? 'WORD_USER_INSTANCE_PRESERVED' : 'WORD_EXECUTABLE_NOT_VERIFIED'
  };
  const started = Date.now();
  const result = ps(path.join(__dirname, 'interop-word-worker.ps1'), ['-Workspace', directory], timeout);
  const resultFile = path.join(directory, 'word-result.json');
  let word = { passed: false, acceptance: 'unverified', stage: 'worker-start' };
  if (fs.existsSync(resultFile)) {
    try { word = readJson(resultFile); } catch { word.failure = 'WORD_RESULT_INCOMPLETE'; }
  }
  if (result.error?.code === 'ETIMEDOUT') {
    word = { ...word, passed: false, acceptance: 'unverified', failure: 'WORD_WORKER_TIMEOUT', timedOutAt: word.stage };
  } else if (result.status !== 0 && !word.failure) {
    word = { ...word, passed: false, acceptance: 'unverified', failure: 'WORD_WORKER_FAILED' };
  }
  word.elapsedMs = Date.now() - started;
  word.cleanup = cleanupOwnedWord(directory);
  word.artifacts = ['Word-roundtrip.docx', 'Word-layout.pdf'].filter(name => fs.existsSync(path.join(directory, name))).map(name => {
    const bytes = fs.readFileSync(path.join(directory, name));
    return { file: name, bytes: bytes.length, sha256: sha(bytes) };
  });
  return word;
}
function main(args = process.argv.slice(2)) {
  const options = parseArguments(args);
  if (options.help) {
    console.log('node tools/test/interop-acceptance.cjs [--docx QA.docx] [--epub QA.epub] [--word] [--timeout-ms 45000]\nDefault: read-only inventory. --word explicitly starts one bounded, owned Word probe; never retries automatically.\nReports and fixture copies are written to a new .cache/capability-upgrade/interop-* directory.');
    return;
  }
  if (process.platform !== 'win32') throw Error('This native inventory adapter currently supports Windows only');
  const cache = path.join(ROOT, '.cache/capability-upgrade');
  fs.mkdirSync(cache, { recursive: true });
  const directory = fs.mkdtempSync(path.join(cache, 'interop-'));
  writeJson(path.join(directory, 'interop-owner.json'), { schemaVersion: 1, purpose: 'isolated-external-interop-qa', createdAt: new Date().toISOString() });
  const fixtures = [];
  if (options.docx) fixtures.push(copyFixture(options.docx, 'docx', directory));
  if (options.epub) fixtures.push(copyFixture(options.epub, 'epub', directory));
  const inventoryResult = ps(path.join(__dirname, 'interop-inventory.ps1'));
  if (inventoryResult.status !== 0) {
    writeJson(path.join(directory, 'report.json'), { schemaVersion: 1, acceptance: 'unverified', failure: inventoryResult.error?.code === 'ETIMEDOUT' ? 'INVENTORY_TIMEOUT' : 'INVENTORY_FAILED' });
    throw Error('Inventory did not complete; report retained under ' + path.relative(ROOT, directory));
  }
  const inventory = JSON.parse(inventoryResult.stdout.trim().replace(/^\uFEFF/, ''));
  writeJson(path.join(directory, 'inventory.json'), inventory);
  const report = {
    schemaVersion: 1, recordedAt: new Date().toISOString(), productRequiresOffice: false,
    acceptance: 'unverified', mode: options.word ? 'word-partial-roundtrip' : 'read-only-inventory',
    detectionScope: inventory.detectionScope, errors: inventory.errors, fixtures,
    applications: inventory.applications.map(application => classifyApplication(application, inventory.errors)),
    limitations: [
      'Installation, XML validation and EPUBCheck never count as native workflow acceptance.',
      'A Word probe covers one editable formula and structural counts, not all 19 OMML classes or visual fidelity.',
      'Native reader import, navigation, reflow, themes and restart require a separate observed workflow.',
      'No application is installed, no user document/window is inspected, and no current process is reused.',
      'Word activation timeouts without proven process ownership are reported; unidentified processes are not killed.'
    ],
    scripts: ['interop-acceptance.cjs', 'interop-inventory.ps1', 'interop-word-worker.ps1', 'interop-word-cleanup.ps1'].map(file => ({ file, sha256: sha(fs.readFileSync(path.join(__dirname, file))) }))
  };
  if (options.word) {
    const application = report.applications.find(application => application.id === 'word');
    report.word = runWord(directory, application, options.timeout);
    application.nativeAcceptance = report.word.passed ? 'partial_native_roundtrip' : 'attempt_unverified';
  }
  for (const fixture of fixtures) {
    if (sha(fs.readFileSync(path.join(directory, fixture.file))) !== fixture.sha256) throw Error('Input fixture copy was unexpectedly modified');
  }
  writeJson(path.join(directory, 'report.json'), report);
  console.log(JSON.stringify({
    report: path.relative(ROOT, path.join(directory, 'report.json')), mode: report.mode,
    applications: report.applications.map(application => ({ id: application.id, availability: application.availability, nativeAcceptance: application.nativeAcceptance })),
    word: report.word ? { passed: report.word.passed, acceptance: report.word.acceptance, failure: report.word.failure } : undefined
  }));
  if (options.word && !report.word?.passed) process.exitCode = 2;
  return report;
}
module.exports = { classifyApplication, parseArguments, validateOwnership, copyFixture, main };
if (require.main === module) {
  try { main(); } catch (error) { console.error(error.message); process.exitCode = 1; }
}
