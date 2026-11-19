'use strict';

// Preserve the existing CI entry point while exercising the same owned
// production shell at compact sizes.
if (process.argv.includes('--source-test')) {
  const { spawnSync } = require('node:child_process');
  const path = require('node:path');
  const run = spawnSync(process.execPath, ['--test', path.join(__dirname, 'shell-single-chrome.test.cjs')], { stdio: 'inherit' });
  process.exitCode = run.status ?? 1;
} else {
  process.argv.push('--case', 'compact');
  require('./shell-single-chrome-browser.cjs');
}
