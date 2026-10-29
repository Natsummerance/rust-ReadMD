'use strict';
// Opt-in acceptance against an explicit prebuilt kernel and owned synthetic
// files only. Never builds, downloads, reads user files, or records tokens.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), net = require('node:net');
const { spawn } = require('node:child_process');
const assert = require('node:assert/strict');
const { options } = require('./v006-table-clipboard-kernel.cjs');
const { ROOT } = require('./v006-table-clipboard-host.cjs');
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const callId = () => crypto.randomUUID();
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function startKernel(binary, profile, workspace) {
  const socket = net.createServer();
  await new Promise((resolve, reject) => { socket.once('error', reject); socket.listen(0, '127.0.0.1', resolve); });
  const port = socket.address().port;
  await new Promise(resolve => socket.close(resolve));
  const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !/TOKEN|SECRET|CREDENTIAL|HERMES|PROXY|API_KEY/i.test(key)));
  Object.assign(env, { READMD_DATA_DIR: profile, READMD_EXTRA_ROOTS: '', READMD_REQUIRE_TOKEN: '1' });
  const child = spawn(binary, ['--no-window', '--port', String(port), '--require-token', '--data-dir', profile,
    '--workspace', workspace, '--assets', path.join(ROOT, 'assets')], { cwd: ROOT, windowsHide: true, stdio: 'ignore', env });
  let spawnError; child.on('error', error => { spawnError = error; });
  const base = `http://127.0.0.1:${port}`;
  const stop = async () => {
    if (child.exitCode !== null || child.signalCode !== null || spawnError) return;
    child.kill();
    const until = Date.now() + 10000;
    while (child.exitCode === null && child.signalCode === null && Date.now() < until) await delay(50);
    if (child.exitCode === null && child.signalCode === null) throw Error('Owned kernel did not stop');
  };
  try {
    const until = Date.now() + 30000;
    while (Date.now() < until) {
      if (spawnError || child.exitCode !== null || child.signalCode !== null) throw Error('Owned kernel exited before readiness');
      try {
        const page = await fetch(base, { signal: AbortSignal.timeout(1500) });
        const token = (await page.text()).match(/name="readmd-app-token"\s+content="([^"]+)"/)?.[1];
        if (page.ok && token) return { pid: child.pid, stop, api: (route, request = {}) => fetch(base + route, { ...request,
          headers: { ...request.headers, 'X-ReadMD-App-Token': token, 'X-ReadMD-Token': token }, signal: AbortSignal.timeout(15000) }) };
      } catch (_) { /* The owned server is still starting. */ }
      await delay(100);
    }
    throw Error('Owned kernel startup deadline exceeded');
  } catch (error) { await stop(); throw error; }
}

module.exports = { startKernel };
