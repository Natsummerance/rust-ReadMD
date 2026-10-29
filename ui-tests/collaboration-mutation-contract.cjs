'use strict';
// Real offline HTTP service checks, with synthetic data and in-memory secrets.
// READMD_COLLAB_BIN selects the standalone server; READMD_BIN selects a headless
// desktop kernel and also checks its shared session API. Never invokes Cargo.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const Y = require('../tools/collaboration-bundle/node_modules/yjs');
const root = path.resolve(__dirname, '..');
const standalone = Boolean(process.env.READMD_COLLAB_BIN);
const binary = process.env.READMD_COLLAB_BIN || process.env.READMD_BIN || path.join(root, '.cache/community-upgrade/build/debug/readmd.exe');
const base = fs.mkdtempSync(path.join(root, '.cache/capability-upgrade/collaboration-contract-'));
const profile = path.join(base, 'service');
const admin = crypto.randomBytes(32).toString('hex');
const appToken = crypto.randomBytes(32).toString('hex');
const bytes = value => Buffer.from(value).toString('base64');
const wait = ms => new Promise(resolve => setTimeout(resolve, ms));
const report = {
  platform: process.platform,
  scope: 'real loopback HTTP requests and independent Yjs documents; no native windows, human participants, TLS proxy or long-duration certification',
  transport: standalone ? 'standalone' : 'headless desktop and dedicated LAN listener',
  checks: [], failures: [], passed: false,
};
let child, desktopUrl, sessionUrl, phase = 'startup';
function check(name, passed, details = {}) {
  const item = { name, passed: Boolean(passed), ...details };
  report.checks.push(item);
  if (!passed) report.failures.push(item);
}
async function stop() {
  if (!child) return;
  const current = child;
  child = null;
  if (current.exitCode !== null) return;
  const closed = new Promise(resolve => current.once('exit', resolve));
  current.kill();
  await Promise.race([closed, wait(5000).then(() => {
    if (current.exitCode === null) throw Error('Owned service did not stop');
  })]);
}
async function start(owner) {
  const args = standalone
    ? ['--data-dir', profile, '--assets', path.join(root, 'assets'), '--listen', '127.0.0.1:0']
    : ['--no-window', '--port', '-1', '--data-dir', profile, '--workspace', profile, '--assets', path.join(root, 'assets'), '--require-token'];
  const env = { ...process.env, READMD_DATA_DIR: profile, READMD_REQUIRE_TOKEN: '1', READMD_APP_TOKEN: appToken, READMD_COLLAB_ADMIN_TOKEN: admin };
  child = spawn(binary, args, { cwd: root, windowsHide: true, env, stdio: ['ignore', 'pipe', 'ignore'] });
  let port, launchFailed = false, pending = '';
  child.on('error', () => { launchFailed = true; });
  child.stdout.on('data', chunk => {
    pending = (pending + chunk).slice(-8192);
    const match = pending.match(standalone ? /listening on port (\d+)/ : /listening on http:\/\/127\.0\.0\.1:(\d+)/);
    if (match) port = Number(match[1]);
  });
  const deadline = Date.now() + 15000;
  while (!port) {
    if (launchFailed || child.exitCode !== null) throw Error('Owned service did not start');
    if (Date.now() >= deadline) throw Error('Owned service startup timed out');
    await wait(25);
  }
  if (standalone) sessionUrl = 'http://127.0.0.1:' + port;
  else {
    desktopUrl = 'http://127.0.0.1:' + port;
    if (owner) {
      const resumed = await desktop('resume', { room: owner.room }, owner);
      assert.equal(resumed.status, 200, 'Resume an owned synthetic room');
      sessionUrl = 'http://127.0.0.1:' + resumed.data.port;
    }
  }
}
async function json(url, body, headers) {
  const response = await fetch(url, { method: 'POST', headers: { 'Content-Type': 'application/json', ...headers }, body: JSON.stringify(body), signal: AbortSignal.timeout(5000) });
  return { status: response.status, data: await response.json() };
}
async function desktop(operation, body, credential) {
  return json(desktopUrl + (['create', 'resume'].includes(operation) ? '/api/collaboration/' + operation : '/api/collaboration/session'), {
    ...body, ...(credential && operation !== 'create' ? { room: credential.room, ...(operation !== 'resume' ? { operation } : {}) } : {}),
  }, { 'X-ReadMD-Token': appToken, ...(credential ? { 'X-ReadMD-Collaboration-Token': credential.token } : {}) });
}
async function lan(operation, body, credential) {
  return json(sessionUrl + '/sessions/' + credential.room + '/' + operation, body, { Authorization: 'Bearer ' + credential.token });
}
async function create() {
  const body = { title: 'Synthetic contract', content: 'Original 🦀\n', name: 'Owner' };
  const created = standalone ? await json(sessionUrl + '/sessions', body, { Authorization: 'Bearer ' + admin }) : await desktop('create', body);
  assert.equal(created.status, 200, 'Create an isolated synthetic room');
  if (!standalone) sessionUrl = 'http://127.0.0.1:' + created.data.port;
  return created.data.credential;
}
async function invite(api, owner, role = 'editor') {
  const result = await api('invite', { role, name: 'Synthetic peer' }, owner);
  assert.equal(result.status, 200, 'Invite a synthetic peer');
  return result.data.credential;
}
async function upload(credential, material = Buffer.from('Synthetic attachment.')) {
  const response = await fetch(sessionUrl + '/sessions/' + credential.room + '/attachment', {
    method: 'POST', headers: { Authorization: 'Bearer ' + credential.token, 'Content-Type': 'text/plain', 'X-Asset-Name-Base64': bytes(Buffer.from('合成资料.txt')) }, body: material, signal: AbortSignal.timeout(5000),
  });
  return { status: response.status, data: await response.json() };
}
async function download(credential, asset) {
  const response = await fetch(sessionUrl + '/sessions/' + credential.room + '/download', {
    method: 'POST', headers: { Authorization: 'Bearer ' + credential.token, 'Content-Type': 'application/json' }, body: JSON.stringify({ id: asset }), signal: AbortSignal.timeout(5000),
  });
  return { status: response.status, bytes: Buffer.from(await response.arrayBuffer()) };
}
async function initial(api, credential) {
  const doc = new Y.Doc();
  const response = await api('sync', { vector: bytes(Y.encodeStateVector(doc)) }, credential);
  assert.equal(response.status, 200, 'Read the initial CRDT state');
  Y.applyUpdate(doc, Buffer.from(response.data.update, 'base64'));
  return doc;
}
async function contract(label, api, owner) {
  phase = label + ': malformed permission';
  const victim = await invite(api, owner);
  const missingRole = await api('permission', { member: victim.member }, owner);
  check(label + ': missing role is rejected', missingRole.status === 422 && missingRole.data.error_code === 'collaboration_invalid', { expectedStatus: 422, actualStatus: missingRole.status });
  const access = await api('snapshot', {}, victim);
  check(label + ': rejected permission preserves access', access.status === 200 && access.data.role === 'editor', { expectedStatus: 200, actualStatus: access.status });
  for (const role of [false, [], {}, 'invalid']) {
    const result = await api('permission', { member: victim.member, role }, owner);
    check(label + ': invalid role is rejected (' + typeof role + ')', result.status === 422, { expectedStatus: 422, actualStatus: result.status });
  }
  phase = label + ': malformed delta then retry';
  const editor = await invite(api, owner);
  const doc = await initial(api, editor), original = doc.getText('content').toString();
  const vector = Y.encodeStateVector(doc);
  doc.getText('content').insert(doc.getText('content').length, '[' + label + ' offline retry]\n');
  const update = Y.encodeStateAsUpdate(doc, vector);
  for (const [kind, invalid] of [['null', null], ['boolean', false], ['number', 7], ['array', []], ['object', { data: bytes(update) }]]) {
    const response = await api('sync', { vector: bytes(vector), update: invalid, presence: { cursor: 0 } }, editor);
    check(label + ': ' + kind + ' delta receives no success ACK', response.status === 422 && response.data.error_code === 'collaboration_invalid', { expectedStatus: 422, actualStatus: response.status, acknowledged: response.data.synced === true });
    const current = await api('snapshot', {}, owner);
    check(label + ': ' + kind + ' delta leaves content intact', current.status === 200 && current.data.content === original);
  }
  const polled = await api('sync', { vector: bytes(vector) }, editor);
  check(label + ': omitted delta remains a read-only poll', polled.status === 200 && polled.data.synced === true);
  for (let delivery = 0; delivery < 2; delivery++) {
    const response = await api('sync', { vector: bytes(vector), update: bytes(update) }, editor);
    check(label + ': real retry delivery ' + delivery + ' is acknowledged', response.status === 200 && response.data.synced === true);
  }
  const expected = doc.getText('content').toString();
  check(label + ': retry and duplicate delivery preserve exact content', (await api('snapshot', {}, owner)).data.content === expected);
  doc.destroy();
  phase = label + ': explicit revocation';
  const revoked = await api('permission', { member: editor.member, role: null }, owner);
  check(label + ': explicit null still revokes', revoked.status === 200 && (await api('snapshot', {}, editor)).status === 403);
  check(label + ': delayed delta after revocation is denied', (await api('sync', { vector: bytes(vector), update: bytes(update) }, editor)).status === 403);
  return { revoked: editor, expected };
}
(async () => {
  let owner, finalExpected, revoked;
  try {
    report.binarySha256 = crypto.createHash('sha256').update(fs.readFileSync(binary)).digest('hex');
    await start();
    owner = await create();
    const adapters = standalone ? [['LAN', lan]] : [['desktop', desktop], ['LAN', lan]];
    for (const [label, api] of adapters) ({ expected: finalExpected, revoked } = await contract(label, api, owner));
    phase = 'attachment authorization and delayed writes';
    const uploaded = await upload(owner);
    assert.equal(uploaded.status, 200, 'Upload a synthetic attachment');
    const asset = uploaded.data.id;
    const material = Buffer.from('Synthetic attachment.');
    const peer = await invite(lan, owner), doc = await initial(lan, peer), vector = Y.encodeStateVector(doc);
    doc.getText('content').insert(0, 'Unsent after downgrade.\n');
    const update = bytes(Y.encodeStateAsUpdate(doc, vector));
    for (const role of ['reviewer', 'reader']) {
      assert.equal((await lan('permission', { member: peer.member, role }, owner)).status, 200);
      check(role + ': delayed CRDT write is denied', (await lan('sync', { vector: bytes(vector), update }, peer)).status === 403);
      check(role + ': attachment upload is denied before deduplication', (await upload(peer, material)).status === 403);
      const received = await download(peer, asset);
      check(role + ': authorized attachment download retains exact bytes', received.status === 200 && received.bytes.equals(material));
      check(role + ': attachment list remains readable', (await lan('attachments', {}, peer)).status === 200);
    }
    assert.equal((await lan('permission', { member: peer.member, role: null }, owner)).status, 200);
    for (const operation of ['snapshot', 'attachments', 'reviews']) check('revoked: ' + operation + ' is denied', (await lan(operation, {}, peer)).status === 403);
    check('revoked: attachment upload is denied before deduplication', (await upload(peer, material)).status === 403);
    check('revoked: known attachment download is denied', (await download(peer, asset)).status === 403);
    check('revoked: delayed CRDT write is denied', (await lan('sync', { vector: bytes(vector), update }, peer)).status === 403);
    check('revoked: delayed review is denied', (await lan('review', { text: 'Late synthetic comment' }, peer)).status === 403);
    check('denied writes preserve the canonical content', (await lan('snapshot', {}, owner)).data.content === finalExpected);
    doc.destroy();
    const foreign = await create();
    check('another room cannot download a known attachment ID', (await download(foreign, asset)).status === 404);
    phase = 'restart durability';
    await stop();
    await start(owner);
    check('restart retains acknowledged retry content', (await lan('snapshot', {}, owner)).data.content === finalExpected);
    check('restart retains explicit revocation', (await lan('snapshot', {}, revoked)).status === 403);
    check('restart denies revoked attachment download', (await download(peer, asset)).status === 403);
    const restored = await download(owner, asset);
    check('restart retains authorized attachment bytes', restored.status === 200 && restored.bytes.equals(material));
    report.passed = report.failures.length === 0;
  } catch (error) {
    report.failedStage = phase;
    report.error = error.name;
    // Do not persist exception messages, URLs, tokens or local profile config.
  } finally {
    try { await stop(); } catch { report.failedStage ||= 'owned service shutdown'; report.passed = false; }
    report.recordedAt = new Date().toISOString();
    fs.writeFileSync(path.join(base, 'report.json'), JSON.stringify(report, null, 2) + '\n');
    console.log(JSON.stringify({ passed: report.passed, checks: report.checks.length, failures: report.failures.length, failedStage: report.failedStage, report: path.relative(root, path.join(base, 'report.json')) }));
    if (!report.passed) process.exitCode = 1;
  }
})();
