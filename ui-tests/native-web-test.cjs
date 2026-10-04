'use strict';
// Real system-WebView integration, using only an isolated local fixture.
// NODE_PATH may point to the project's already-installed test dependencies.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const http = require('node:http');
const { spawn } = require('node:child_process');
const { chromium } = require('playwright');
const root = path.resolve(__dirname, '..');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(fn, ms = 30000) {
  const end = Date.now() + ms;
  while (Date.now() < end) { const value = await fn(); if (value) return value; await delay(100); }
  throw new Error('Timed out waiting for native WebView fixture');
}
function debugPorts(dir, depth = 0) {
  if (depth > 5 || !fs.existsSync(dir)) return [];
  const ports = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) ports.push(...debugPorts(full, depth + 1));
    else if (entry.name === 'DevToolsActivePort') {
      const port = Number(fs.readFileSync(full, 'utf8').split('\n')[0]);
      if (port > 0) ports.push(port);
    }
  }
  return [...new Set(ports)];
}
async function closeOwnCaptureWindow(pid, main = false) {
  // WM_CLOSE exercises the same path as the capture window's title-bar ×.
  // Match only this fixture process and the auxiliary ReadMD window title.
  const script = `
Add-Type -TypeDefinition @'
using System; using System.Text; using System.Runtime.InteropServices;
public static class ReadMDCaptureFixture {
  delegate bool Callback(IntPtr window, IntPtr data);
  [DllImport("user32.dll")] static extern bool EnumWindows(Callback callback, IntPtr data);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr window, StringBuilder text, int limit);
  [DllImport("user32.dll")] static extern bool PostMessage(IntPtr window, uint message, IntPtr w, IntPtr l);
  public static bool Close(uint ownPid, bool main) {
    IntPtr found=IntPtr.Zero;
    EnumWindows((window,data)=>{ uint pid; GetWindowThreadProcessId(window,out pid);
      var title=new StringBuilder(512); GetWindowText(window,title,title.Capacity);
      var text=title.ToString();
      if(pid==ownPid && (main ? text.Contains("ReadMD") && !text.StartsWith("ReadMD \u00b7 ") : text.StartsWith("ReadMD \u00b7 "))) {found=window;return false;} return true;
    },IntPtr.Zero);
    return found!=IntPtr.Zero && PostMessage(found,0x0010,IntPtr.Zero,IntPtr.Zero);
  }
}
'@
$request=[Console]::In.ReadToEnd()|ConvertFrom-Json
if(![ReadMDCaptureFixture]::Close([uint32]$request.pid,[bool]$request.main)){exit 2}
`;
  await new Promise((resolve, reject) => {
    const helper = spawn('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script],
      { windowsHide: true, stdio: ['pipe', 'ignore', 'pipe'] });
    let error = ''; helper.stderr.on('data', chunk => { error += chunk; });
    helper.on('error', reject);
    helper.on('exit', code => code === 0 ? resolve() : reject(new Error(`Capture window close failed (${code}): ${error.slice(-500)}`)));
    helper.stdin.end(JSON.stringify({ pid, main }));
  });
}
(async () => {
  const data = fs.mkdtempSync(path.join(os.tmpdir(), 'readmd-native-web-test-'));
  const appPort = Number(process.env.READMD_NATIVE_TEST_PORT || 28646);
  const browsers = [];
  let child, fixture;
  try {
    fixture = http.createServer((req, res) => {
      res.setHeader('Content-Type', 'text/html; charset=utf-8');
      res.end('<!doctype html><title>ReadMD capture fixture</title><h1>Static fixture</h1><input value="fixture-secret"><script>document.cookie="fixture_session=ok; path=/";setTimeout(()=>{const p=document.createElement("p");p.id="dynamic";p.textContent="Rendered by JavaScript: "+document.cookie;document.body.appendChild(p)},300)</script>');
    });
    await new Promise(resolve => fixture.listen(0, '127.0.0.1', resolve));
    const url = `http://127.0.0.1:${fixture.address().port}/fixture`;
    child = spawn(process.env.READMD_BIN || path.join(root, 'target', 'debug', 'readmd.exe'),
      ['--port', String(appPort), '--data-dir', data, '--assets', path.join(root, 'assets'), '--workspace', data],
      { cwd: root, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'], env: { ...process.env,
        WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=0 --remote-debugging-address=127.0.0.1' } });
    let stderr = '';
    child.stderr.on('data', chunk => { stderr += chunk.toString(); });
    await until(async () => {
      if (child.exitCode !== null) throw new Error(`Native app exited (${child.exitCode}): ${stderr.slice(-1000)}`);
      return debugPorts(data).length;
    });
    const readerBrowser = await chromium.connectOverCDP(`http://127.0.0.1:${debugPorts(data)[0]}`);
    browsers.push(readerBrowser);
    const reader = await until(() => readerBrowser.contexts().flatMap(c => c.pages()).find(p => p.url().startsWith(`http://127.0.0.1:${appPort}`)));
    await reader.waitForFunction(() => !!window.pywebview?.api?.authorize_private_web);
    assert.deepEqual(await reader.evaluate(url => pywebview.api.render_web_page(url, 'denied', 5000, false), url),
      { ok: false, code: 'private_authorization_required' });
    const initialPorts = new Set(debugPorts(data));
    await reader.evaluate(url => { window.nativeTestAuthorization = pywebview.api.authorize_private_web(url, 'capture-test'); }, url);
    const auxPort = await until(() => debugPorts(data).find(p => !initialPorts.has(p)));
    const auxBrowser = await chromium.connectOverCDP(`http://127.0.0.1:${auxPort}`);
    browsers.push(auxBrowser);
    const aux = await until(() => auxBrowser.contexts().flatMap(c => c.pages()).find(p => p.url().startsWith(url)));
    await aux.locator('#dynamic').waitFor();
    assert.equal(await aux.evaluate(() => typeof window.pywebview), 'undefined');
    await aux.locator('#readmd-capture-bar button').click();
    const grant = await reader.evaluate(() => window.nativeTestAuthorization);
    assert.equal(grant.ok, true);
    assert.equal(typeof grant.grant, 'string');
    const captured = await reader.evaluate(({ url, grant }) => pywebview.api.render_web_page(url, 'capture-test', 12000, false, grant), { url, grant: grant.grant });
    assert.equal(captured.ok, true, JSON.stringify(captured));
    assert.match(captured.html, /Rendered by JavaScript: fixture_session=ok/);
    assert.doesNotMatch(captured.html, /fixture-secret|readmd-capture-bar|<script/);
    assert.equal(captured.engine, 'system-webview');
    const timedOut = await reader.evaluate(({ url, grant }) => pywebview.api.render_web_page(url, 'capture-test', 1000, true, grant), { url, grant: grant.grant });
    assert.deepEqual(timedOut, { ok: false, code: 'render_timeout' });
    await reader.evaluate(({ url, grant }) => { window.nativeTestCancel = pywebview.api.render_web_page(url, 'capture-test', 20000, true, grant); }, { url, grant: grant.grant });
    await delay(300);
    await reader.evaluate(() => pywebview.api.cancel_web_render('capture-test'));
    assert.deepEqual(await reader.evaluate(() => window.nativeTestCancel), { ok: false, code: 'cancelled' });
    await reader.evaluate(({ url, grant }) => { window.nativeTestClose = pywebview.api.render_web_page(url, 'capture-test', 20000, true, grant); }, { url, grant: grant.grant });
    await delay(500);
    await closeOwnCaptureWindow(child.pid);
    assert.deepEqual(await reader.evaluate(() => window.nativeTestClose), { ok: false, code: 'cancelled' });
    assert.equal(await reader.evaluate(() => typeof pywebview.api.get_app_info), 'function');
    await reader.evaluate(() => pywebview.api.revoke_private_web('capture-test'));
    assert.deepEqual(await reader.evaluate(({ url, grant }) => pywebview.api.render_web_page(url, 'capture-test', 1000, false, grant), { url, grant: grant.grant }),
      { ok: false, code: 'private_grant_invalid' });
    // This test exercises full exit; the separate window smoke covers close-to-tray.
    await reader.evaluate(() => { state.closeToTray = false; syncWindowPreferences(); });
    await reader.evaluate(async () => { await renderVirtual('clipboard', 'native-close.md', '', 'NATIVE RECOVERABLE DRAFT', []); await toggleEdit(); });
    await closeOwnCaptureWindow(child.pid, true);
    await reader.locator('#close-confirm-modal').waitFor();
    await reader.locator('#close-confirm-cancel').click();
    await until(() => reader.evaluate(() => !document.getElementById('close-confirm-modal').getClientRects().length));
    assert.equal(child.exitCode, null);
    assert.equal(await reader.evaluate(() => getEditContent()), 'NATIVE RECOVERABLE DRAFT');
    await closeOwnCaptureWindow(child.pid, true);
    await reader.locator('#close-confirm-modal').waitFor();
    await reader.locator('#close-confirm-discard').click();
    await until(() => child.exitCode !== null, 20000);
    const records = fs.readdirSync(path.join(data, 'document-history')).filter(name => name.endsWith('.json'))
      .map(name => JSON.parse(fs.readFileSync(path.join(data, 'document-history', name), 'utf8')));
    assert.ok(records.some(entry => entry.kind === 'discarded' && entry.name === 'native-close.md'));
    console.log('PASS: real JavaScript DOM, isolated bridge, explicit grant, cookies, form redaction, timeout, cancellation, native window close, revoked grant; reader WM_CLOSE cancels safely and retains discarded recovery before exit');
  } finally {
    // Disconnect CDP without closing another browser process.
    for (const browser of browsers) await browser.close().catch(() => {});
    if (child && child.exitCode === null) await new Promise(resolve => { child.once('exit', resolve); child.kill(); });
    if (fixture) await new Promise(resolve => fixture.close(resolve));
    await require('./cleanup-native-fixture.cjs')(data);
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
