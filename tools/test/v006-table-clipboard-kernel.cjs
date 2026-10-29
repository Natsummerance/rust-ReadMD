'use strict';
// Shared by the opt-in HTTP/browser acceptance scripts. It never builds a
// binary, opens a visible window, reads a user profile or logs an app token.
const fs = require('node:fs');
const path = require('node:path');
const net = require('node:net');
const { spawn } = require('node:child_process');
const { ROOT } = require('./v006-table-clipboard-host.cjs');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));

function options(argv) {
  const result = {};
  for (let i = 0; i < argv.length; i += 2) {
    if (!['--bin', '--evidence', '--base', '--playwright'].includes(argv[i]) || !argv[i + 1] || argv[i + 1].startsWith('--')) throw Error('Expected --bin, --evidence, --base or --playwright followed by a value');
    result[argv[i].slice(2)] = argv[i + 1];
  }
  if (!result.evidence || !result.bin && !result.base) throw Error('An explicit --evidence and either --bin or --base are required; no compilation fallback exists');
  result.evidence = path.resolve(result.evidence);
  if (fs.existsSync(result.evidence)) throw Error('Evidence directory must be new');
  if (result.bin) {
    result.bin = path.resolve(result.bin);
    if (!fs.statSync(result.bin).isFile()) throw Error('Prebuilt binary is missing');
  }
  if (result.base) {
    const url = new URL(result.base);
    if (url.protocol !== 'http:' || !['127.0.0.1', 'localhost', '[::1]'].includes(url.hostname) || url.username || url.password || url.search || url.hash) throw Error('--base must be a local HTTP server without credentials');
    result.base = url.origin;
  }
  fs.mkdirSync(result.evidence, { recursive: true });
  return result;
}

async function unusedPort() {
  const server = net.createServer();
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  const port = server.address().port;
  await new Promise(resolve => server.close(resolve));
  return port;
}

async function connect(base) {
  const response = await fetch(base, { signal: AbortSignal.timeout(3000) });
  if (!response.ok) throw Error('Local kernel root is unavailable');
  const html = await response.text();
  const token = html.match(/name="readmd-app-token"\s+content="([^"]+)"/)?.[1];
  if (!token) throw Error('Local kernel did not provide its app token');
  const api = (route, request = {}) => fetch(base + route, { ...request,
    headers: { ...request.headers, 'X-ReadMD-App-Token': token }, signal: AbortSignal.timeout(15000) });
  return { base, api, csp: response.headers.get('content-security-policy') || '' };
}

async function startKernel(binary, profile, workspace) {
  fs.mkdirSync(profile, { recursive: true });
  const port = await unusedPort();
  const child = spawn(binary, ['--no-window', '--port', String(port), '--data-dir', profile, '--workspace', workspace, '--assets', path.join(ROOT, 'assets')],
    { cwd: ROOT, windowsHide: true, stdio: 'ignore', env: { ...process.env, READMD_DATA_DIR: profile } });
  let spawnError;
  child.on('error', error => { spawnError = error; });
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
      if (spawnError || child.exitCode !== null || child.signalCode !== null) throw Error('Prebuilt kernel exited before becoming ready');
      try { return { ...await connect('http://127.0.0.1:' + port), pid: child.pid, stop }; }
      catch (_) { await delay(100); }
    }
    throw Error('Prebuilt kernel startup deadline exceeded');
  } catch (error) { await stop(); throw error; }
}

module.exports = { options, startKernel, connect };
