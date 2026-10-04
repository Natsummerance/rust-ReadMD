'use strict';
// Opt-in live metadata check. Never downloads a release package or changes an installation.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { startUiServer } = require('./ui-server.cjs');
const proxyKeys = ['HTTPS_PROXY', 'HTTP_PROXY', 'ALL_PROXY', 'https_proxy', 'http_proxy', 'all_proxy'];
(async () => {
  const results = [];
  for (const [index, scenario] of ['current-network', 'unreachable-proxy'].entries()) {
    const original = Object.fromEntries(proxyKeys.map(key => [key, process.env[key]]));
    let server;
    try {
      if (index) for (const key of proxyKeys) process.env[key] = 'http://127.0.0.1:1';
      server = await startUiServer(28666 + index, { stdio: 'ignore', timeoutMs: 30000 });
      const started = Date.now();
      const response = await fetch(`http://127.0.0.1:${server.port}/api/update/check`, { signal: AbortSignal.timeout(55000) });
      const payload = await response.json();
      const elapsedMs = Date.now() - started;
      assert.equal(response.status, 200);
      assert.equal(typeof payload.ok, 'boolean');
      assert.ok(elapsedMs < 55000, 'network failures have a bounded total duration');
      if (index) { assert.equal(payload.ok, false); assert.ok(payload.error_code); }
      results.push({ scenario, elapsedMs, ok: payload.ok, hasUpdate: payload.has_update ?? null,
        latestVersion: payload.latest_version ?? null, errorCode: payload.error_code ?? null });
    } finally {
      for (const key of proxyKeys) { if (original[key] === undefined) delete process.env[key]; else process.env[key] = original[key]; }
      if (server) {
        const exited = new Promise(resolve => server.child.once('exit', resolve));
        server.child.kill(); await exited; await server.stop();
      }
    }
  }
  if (process.env.READMD_NETWORK_REPORT) fs.writeFileSync(process.env.READMD_NETWORK_REPORT, JSON.stringify({ date:'2026-10-04', results }, null, 2) + '\n');
  console.log(JSON.stringify(results));
})().catch(error => { console.error(error.message); process.exitCode = 1; });
