'use strict';
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawn } = require('node:child_process');
module.exports = async function cleanupNativeFixture(directory) {
  const resolved = path.resolve(directory);
  if (!resolved.startsWith(path.resolve(os.tmpdir()) + path.sep) || !/^readmd-(window-smoke|native-web-test)-/.test(path.basename(resolved))) {
    throw new Error('Native fixture cleanup target is outside its own temporary directory');
  }
  // WebView2 Crashpad can outlive a closed fixture. Match only its unique user-data path.
  if (process.platform === 'win32') await new Promise((resolve, reject) => {
    const child = spawn('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command',
      "$fixture = [Console]::In.ReadToEnd().Trim(); Get-CimInstance Win32_Process -Filter \"Name='msedgewebview2.exe'\" | Where-Object { $_.CommandLine -and $_.CommandLine.Contains($fixture) } | ForEach-Object { Stop-Process -Id $_.ProcessId -ErrorAction SilentlyContinue }"],
      { windowsHide:true, stdio:['pipe','ignore','ignore'] });
    child.once('error', reject); child.once('exit', code => code === 0 ? resolve() : reject(new Error('Own fixture WebView cleanup failed')));
    child.stdin.end(resolved);
  });
  await fs.promises.rm(resolved, { recursive:true, force:true, maxRetries:10, retryDelay:300 });
};
