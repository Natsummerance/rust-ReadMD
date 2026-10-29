'use strict';
// Two real standalone processes, one owned synthetic profile. No Cargo fallback,
// GUI, credentials in reports, or claim of multiple writable server support.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const { spawn, spawnSync } = require('node:child_process');
const Y = require('../tools/collaboration-bundle/node_modules/yjs');
const root = path.resolve(__dirname, '..');
const binary = process.env.READMD_COLLAB_BIN || path.join(root, '.cache/community-upgrade/build/debug/readmd-collaboration.exe');
const base = fs.mkdtempSync(path.join(root, '.cache/capability-upgrade/collaboration-owner-'));
const profile = path.join(base, 'service');
const admin = crypto.randomBytes(32).toString('hex');
const env = { ...process.env, READMD_COLLAB_ADMIN_TOKEN: admin };
const bytes = value => Buffer.from(value).toString('base64');
const wait = ms => new Promise(resolve => setTimeout(resolve, ms));
const report = { scope: 'real standalone loopback HTTP and CLI processes; one writable Service per database; no TLS proxy, native multi-user or long-duration certification', platform: process.platform, checks: [], passed: false };
const children = [];
let phase = 'startup';
function ended(child) { return child.exitCode !== null || child.signalCode !== null; }
function check(name, condition, details = {}) {
  report.checks.push({ name, passed: Boolean(condition), ...details });
  assert(condition, name);
}
function launch(data) {
  const child = spawn(binary, ['--data-dir', data, '--assets', path.join(root, 'assets'), '--listen', '127.0.0.1:0'], { cwd: root, windowsHide: true, env, stdio: ['ignore', 'pipe', 'pipe'] });
  const instance = { child, port: null, failed: false, busy: false, storage: false };
  let pending = '';
  child.on('error', () => { instance.failed = true; });
  child.stdout.on('data', chunk => {
    pending = (pending + chunk).slice(-512);
    const match = pending.match(/listening on port (\d+)/);
    if (match) instance.port = Number(match[1]);
  });
  child.stderr.on('data', chunk => {
    const value = String(chunk);
    if (value.includes('collaboration_busy')) instance.busy = true;
    if (value.includes('collaboration_storage')) instance.storage = true;
  });
  children.push(instance);
  return instance;
}
async function start(data) {
  const instance = launch(data), deadline = Date.now() + 10000;
  while (!instance.port) {
    if (instance.failed || ended(instance.child)) throw Error('Owned service startup');
    if (Date.now() > deadline) throw Error('Owned service startup deadline');
    await wait(25);
  }
  instance.url = 'http://127.0.0.1:' + instance.port;
  return instance;
}
async function refused(data, code) {
  const instance = launch(data), deadline = Date.now() + 5000;
  while (!ended(instance.child) && !instance.failed) {
    if (Date.now() > deadline) throw Error('Duplicate service was not refused');
    await wait(25);
  }
  check('duplicate owner refuses startup: ' + code, !instance.failed && instance.child.exitCode !== 0 && instance[code]);
}
async function stop(instance) {
  if (ended(instance.child) || instance.failed) return;
  await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(Error('Owned service shutdown deadline')), 5000);
    instance.child.once('exit', () => { clearTimeout(timer); resolve(); });
    instance.child.kill();
  });
}
async function api(instance, operation, body, credential) {
  const response = await fetch(instance.url + (credential ? '/sessions/' + credential.room + '/' + operation : '/sessions'), {
    method: 'POST', headers: { 'Content-Type': 'application/json', Authorization: 'Bearer ' + (credential?.token || admin) }, body: JSON.stringify(body), signal: AbortSignal.timeout(5000),
  });
  return { status: response.status, data: await response.json() };
}
async function download(instance, credential, asset) {
  const response = await fetch(instance.url + '/sessions/' + credential.room + '/download', { method: 'POST', headers: { 'Content-Type': 'application/json', Authorization: 'Bearer ' + credential.token }, body: JSON.stringify({ id: asset }), signal: AbortSignal.timeout(5000) });
  return { status: response.status, bytes: Buffer.from(await response.arrayBuffer()) };
}
function cli(data, operation, file, expected) {
  const result = spawnSync(binary, ['--data-dir', data, operation, file], { cwd: root, windowsHide: true, env, encoding: 'utf8', timeout: 15000 });
  check(operation + ' CLI exit', (result.status === 0) === expected, { actualExit: result.status });
  return result;
}
(async () => {
  try {
    report.binarySha256 = crypto.createHash('sha256').update(fs.readFileSync(binary)).digest('hex');
    const first = await start(profile);
    const created = await api(first, 'create', { title: 'Synthetic owner evidence', content: 'Original 🦀\n', name: 'Owner' });
    assert.equal(created.status, 200);
    const owner = created.data.credential, doc = new Y.Doc();
    const initial = await api(first, 'sync', { vector: bytes(Y.encodeStateVector(doc)) }, owner);
    assert.equal(initial.status, 200);
    Y.applyUpdate(doc, Buffer.from(initial.data.update, 'base64'));
    const vector = Y.encodeStateVector(doc);
    doc.getText('content').insert(doc.getText('content').length, '[acknowledged before crash]\n');
    const update = Y.encodeStateAsUpdate(doc, vector), expected = doc.getText('content').toString();
    check('real CRDT delta is durably acknowledged', (await api(first, 'sync', { vector: bytes(vector), update: bytes(update) }, owner)).status === 200);
    const review = await api(first, 'review', { text: 'Retain the synthetic source.' }, owner);
    check('review is acknowledged', review.status === 200);
    const material = Buffer.from('Synthetic evidence attachment.');
    const upload = await fetch(first.url + '/sessions/' + owner.room + '/attachment', { method: 'POST', headers: { Authorization: 'Bearer ' + owner.token, 'Content-Type': 'text/plain' }, body: material, signal: AbortSignal.timeout(5000) });
    assert.equal(upload.status, 200);
    const asset = (await upload.json()).id;
    const invited = await api(first, 'invite', { role: 'editor', name: 'Synthetic peer' }, owner);
    assert.equal(invited.status, 200);
    const editor = invited.data.credential;
    assert.equal((await api(first, 'permission', { member: editor.member, role: null }, owner)).status, 200);
    phase = 'duplicate process rejection';
    await refused(profile, 'busy');
    const alias = path.join(base, 'profile-alias');
    fs.symlinkSync(profile, alias, process.platform === 'win32' ? 'junction' : 'dir');
    await refused(alias, 'busy');
    check('rejected duplicate leaves the original owner active', (await api(first, 'snapshot', {}, owner)).data.content === expected);
    phase = 'online read-only CLI backup';
    const backup = path.join(base, 'online.sqlite');
    cli(profile, '--backup', backup, true);
    const digest = crypto.createHash('sha256').update(fs.readFileSync(backup)).digest('hex');
    cli(profile, '--backup', backup, false);
    check('failed backup preserves the existing target', crypto.createHash('sha256').update(fs.readFileSync(backup)).digest('hex') === digest);
    check('read-only backup leaves the writer available', (await api(first, 'snapshot', {}, owner)).data.content === expected);
    phase = 'process termination and lease release';
    await stop(first);
    const marker = path.join(profile, 'collaboration.sqlite.owner.lock');
    check('crash leaves a stable empty lock name', fs.statSync(marker).isFile() && fs.statSync(marker).size === 0);
    const markerIdentity = fs.statSync(marker).ino;
    const replacement = await start(profile);
    check('replacement owner retains the acknowledged delta', (await api(replacement, 'snapshot', {}, owner)).data.content === expected);
    check('replacement owner retains revocation', (await api(replacement, 'snapshot', {}, editor)).status === 403);
    const savedAttachment = await download(replacement, owner, asset);
    check('replacement owner retains attachment bytes', savedAttachment.status === 200 && savedAttachment.bytes.equals(material));
    await stop(replacement);
    check('replacement does not delete or replace the lock object', fs.statSync(marker).ino === markerIdentity);
    phase = 'restore into a new profile';
    const restoredProfile = path.join(base, 'restored');
    cli(restoredProfile, '--restore', backup, true);
    cli(restoredProfile, '--restore', backup, false);
    const restored = await start(restoredProfile);
    check('live WAL backup restores exact acknowledged content', (await api(restored, 'snapshot', {}, owner)).data.content === expected);
    check('live backup restores revoked credentials as denied', (await api(restored, 'snapshot', {}, editor)).status === 403);
    check('live backup restores review records', (await api(restored, 'reviews', {}, owner)).data.reviews.some(item => item.id === review.data.id && item.body.text === 'Retain the synthetic source.'));
    const restoredAttachment = await download(restored, owner, asset);
    check('live backup restores exact attachment bytes', restoredAttachment.status === 200 && restoredAttachment.bytes.equals(material));
    phase = 'invalid lock object preserves target';
    const blockedProfile = path.join(base, 'blocked');
    fs.mkdirSync(blockedProfile);
    const blockedMarker = path.join(blockedProfile, 'collaboration.sqlite.owner.lock');
    fs.writeFileSync(blockedMarker, 'Keep this unrelated file.');
    await refused(blockedProfile, 'storage');
    cli(blockedProfile, '--restore', backup, false);
    check('lock failure preserves the marker and absent database', fs.readFileSync(blockedMarker, 'utf8') === 'Keep this unrelated file.' && !fs.existsSync(path.join(blockedProfile, 'collaboration.sqlite')));
    doc.destroy();
    report.passed = true;
  } catch (error) {
    report.failedStage = phase;
    report.error = error.name;
  } finally {
    for (const instance of children) {
      try { await stop(instance); } catch { report.passed = false; report.shutdownFailed = true; }
    }
    report.recordedAt = new Date().toISOString();
    fs.writeFileSync(path.join(base, 'report.json'), JSON.stringify(report, null, 2) + '\n');
    console.log(JSON.stringify({ passed: report.passed, checks: report.checks.length, failedStage: report.failedStage, report: path.relative(root, path.join(base, 'report.json')) }));
    if (!report.passed) process.exitCode = 1;
  }
})();
