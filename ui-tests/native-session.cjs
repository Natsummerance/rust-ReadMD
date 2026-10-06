'use strict';
// Native test sessions preserve the packaged application's CSP and use only
// the explicitly supplied binary/assets and an isolated profile.
const fs = require('node:fs');
const path = require('node:path');
const { spawn } = require('node:child_process');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
function debugPorts(directory, depth = 0) {
  if (depth > 10 || !fs.existsSync(directory)) return [];
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const file = path.join(directory, entry.name);
    if (entry.isDirectory()) return debugPorts(file, depth + 1);
    return entry.name === 'DevToolsActivePort'
      ? [Number(fs.readFileSync(file, 'utf8').split('\n')[0])].filter(Boolean) : [];
  });
}
async function startNative(root, data, chromium, port) {
  if (!process.env.READMD_BIN) throw Error('READMD_BIN is required for native tests');
  const env = { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=0 --remote-debugging-address=127.0.0.1' };
  delete env.READMD_DATA_DIR;
  const log = process.env.READMD_NATIVE_LOG_FILE ? fs.openSync(process.env.READMD_NATIVE_LOG_FILE, 'a') : null;
  const child = spawn(process.env.READMD_BIN, ['--port', String(port), '--data-dir', data,
    '--assets', process.env.READMD_ASSETS_DIR || path.join(root, 'assets'),
    // Keep implicit fixture indexing inside the owned profile, never scan all
    // of the user's Temp directory as a side effect of starting a native test.
    '--workspace', process.env.READMD_WORKSPACE || data],
    { cwd: root, windowsHide: true, stdio: log === null ? 'ignore' : ['ignore', log, log], env });
  if (log !== null) fs.closeSync(log);
  let launchError;
  child.on('error', error => { launchError = error; });
  let browser;
  try {
    const deadline = Date.now() + 45000;
    while (!browser && Date.now() < deadline) {
      if (launchError) throw launchError;
      if (child.exitCode !== null) throw Error('Native test app exited during startup');
      for (const candidate of debugPorts(data)) {
        try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${candidate}`, { timeout: 1000 }); break; } catch {}
      }
      if (!browser) await delay(150);
    }
    if (!browser) throw Error('Native debug port did not become ready');
    let page;
    while (Date.now() < deadline) {
      page = browser.contexts().flatMap(context => context.pages()).find(p => p.url().startsWith(`http://127.0.0.1:${port}`));
      if (page && await page.evaluate(() => Boolean(window.__readmdAppReady && window.pywebview?.api?.get_app_info)).catch(() => false)) {
        return { server: { child }, browser, page };
      }
      await delay(100);
    }
    throw Error('Native reader did not become ready');
  } catch (error) {
    if (browser) await browser.close().catch(() => {});
    if (child.exitCode === null) child.kill();
    throw error;
  }
}
module.exports = { startNative };
