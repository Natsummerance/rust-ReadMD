'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { classifyApplication, parseArguments, validateOwnership, copyFixture } = require('./interop-acceptance.cjs');
const app = overrides => ({ id: 'word', name: 'Microsoft Word', executables: [], registrations: [], running: [], ...overrides });

test('installation cannot become a native acceptance pass', () => {
  const result = classifyApplication(app({ executables: [{ fileName: 'WINWORD.EXE', version: '16.0' }], nativeAcceptance: 'passed' }));
  assert.equal(result.availability, 'installed_unverified');
  assert.equal(result.nativeAcceptance, 'not_run');
  assert.ok(result.requirements.length === 3 && result.requirements.every(value => value.state === 'unverified'));
});
test('existing application processes take precedence and remain outside automation', () => {
  const result = classifyApplication(app({ executables: [{}], running: [{ id: 123 }] }));
  assert.equal(result.availability, 'user_instance_preserved');
  assert.equal(result.nativeAcceptance, 'not_run');
});
test('registration, incomplete discovery, and no match are distinct evidence', () => {
  assert.equal(classifyApplication(app({ registrations: [{}] })).availability, 'registration_only');
  assert.equal(classifyApplication(app(), [{ operation: 'registry-root' }]).availability, 'inspection_incomplete');
  assert.equal(classifyApplication(app()).availability, 'not_detected');
});
test('Word cannot be started implicitly or without an explicit QA fixture', () => {
  assert.equal(parseArguments([]).word, false);
  assert.throws(() => parseArguments(['--word']), /requires/);
  assert.throws(() => parseArguments(['--word', '--docx']), /Missing/);
  assert.throws(() => parseArguments(['--force']), /Unknown/);
  assert.equal(parseArguments(['--word', '--docx', 'QA.docx']).word, true);
  assert.throws(() => parseArguments(['--timeout-ms', '0']), /Timeout/);
  assert.throws(() => parseArguments(['--timeout-ms', 'Infinity']), /Timeout/);
});
test('cleanup requires exact process identity, never merely a reused PID', () => {
  assert.equal(validateOwnership({ processId: 123, processName: 'WINWORD', startTicks: '639000000000000000' }), true);
  for (const owner of [
    null, { processId: 123 }, { processId: 0, processName: 'WINWORD', startTicks: '639000000000000000' },
    { processId: 123, processName: 'wps', startTicks: '639000000000000000' },
    { processId: 123, processName: 'WINWORD', startTicks: '639000000000000000; Stop-Process' }
  ]) assert.equal(validateOwnership(owner), false);
});
test('QA fixture handoff copies bytes, preserves source, and refuses overwrite', () => {
  const base = path.resolve(__dirname, '../../.cache/capability-upgrade');
  fs.mkdirSync(base, { recursive: true });
  const directory = fs.mkdtempSync(path.join(base, 'interop-unit-'));
  const input = path.join(directory, 'fixture.docx');
  const bytes = Buffer.from([0x50, 0x4b, 3, 4, 5, 6]);
  fs.writeFileSync(input, bytes, { flag: 'wx' });
  const copied = copyFixture(input, 'docx', directory);
  assert.deepEqual(fs.readFileSync(input), bytes);
  assert.deepEqual(fs.readFileSync(path.join(directory, copied.file)), bytes);
  assert.equal(copied.sourceUnmodified, true);
  assert.match(copied.sha256, /^[a-f0-9]{64}$/);
  assert.throws(() => copyFixture(input, 'docx', directory), /EEXIST/);
  assert.throws(() => copyFixture(input, 'epub', directory), /Expected/);
  const bad = path.join(directory, 'invalid.epub');
  fs.writeFileSync(bad, 'not a ZIP');
  assert.throws(() => copyFixture(bad, 'epub', directory), /ZIP/);
  // Only three specifically created files are removed; no recursive deletion.
  for (const name of ['fixture.docx', 'source.docx', 'invalid.epub']) fs.unlinkSync(path.join(directory, name));
  fs.rmdirSync(directory);
});
