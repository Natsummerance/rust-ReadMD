'use strict';
const fs = require('node:fs');
const path = require('node:path');
const { spawn } = require('node:child_process');
const ROOT = path.resolve(__dirname, '..');

// Importing this module and --plan only construct/check paths. They cannot launch
// the Extension Host or its graphical Playwright driver.
function buildLaunchPlan({ root = ROOT, out = path.join(root, '.cache/capability-upgrade/vscode-openapi-PLANNED'), environment = process.env } = {}) {
  const assets = path.join(root, 'assets');
  const extension = path.join(root, 'packages/vscode-extension');
  const extensionTests = path.join(root, 'ui-tests/vscode-openapi-native.cjs');
  const driver = path.join(root, 'ui-tests/vscode-openapi-driver.cjs');
  const env = { ...environment, READMD_VSCODE_OPENAPI_AUDIT: out, READMD_VSCODE_DEBUG_PORT: '29317', READMD_DATA_DIR: path.join(out, 'data'), READMD_ASSETS_DIR: assets };
  const args = ['--new-window', '--skip-welcome', '--skip-release-notes', '--disable-updates', '--disable-workspace-trust', '--locale=en-US', '--remote-debugging-port=' + env.READMD_VSCODE_DEBUG_PORT,
    '--user-data-dir=' + path.join(out, 'profile'), '--extensions-dir=' + path.join(out, 'extensions'), '--extensionDevelopmentPath=' + extension, '--extensionTestsPath=' + extensionTests, path.join(out, 'materials')];
  const checks = [
    { id: 'assets', file: assets, kind: 'directory' },
    { id: 'extension', file: extension, kind: 'directory' },
    { id: 'extension-manifest', file: path.join(extension, 'package.json'), kind: 'file' },
    { id: 'extension-tests', file: extensionTests, kind: 'file' },
    { id: 'graphical-driver', file: driver, kind: 'file' },
    { id: 'readmd-binary', file: environment.READMD_BIN, kind: 'file', external: true },
    { id: 'vscode-binary', file: environment.READMD_VSCODE_BIN, kind: 'file', external: true }
  ].map(check => {
    let exists = false;
    try { const stat = check.file && fs.statSync(check.file); exists = !!stat && (check.kind === 'directory' ? stat.isDirectory() : stat.isFile()); } catch { }
    return { ...check, exists };
  });
  return { root, out, env, args, driver, checks, ready: checks.every(check => check.exists) };
}

function publicPlan(plan) {
  return {
    mode: 'launch-plan-only', launched: false, filesystemChanged: false, ready: plan.ready,
    checks: plan.checks.map(check => ({ id: check.id, kind: check.kind, exists: check.exists,
      path: check.external ? (check.file ? '[external-binary]' : null) : path.relative(plan.root, check.file).replaceAll('\\', '/') })),
    requiresGraphicalDriver: true,
    acceptance: 'not_run'
  };
}

async function main(args = process.argv.slice(2)) {
  if (args.length === 1 && args[0] === '--plan') {
    const plan = buildLaunchPlan();
    console.log(JSON.stringify(publicPlan(plan)));
    if (!plan.ready) process.exitCode = 2;
    return plan;
  }
  if (args.length) throw Error('Only --plan is supported');
  const initialPlan = buildLaunchPlan();
  if (!initialPlan.ready) throw Error('Launch paths unavailable; use --plan for a read-only check');
  const cache = path.join(ROOT, '.cache/capability-upgrade');
  fs.mkdirSync(cache, { recursive: true });
  const out = fs.mkdtempSync(path.join(cache, 'vscode-openapi-'));
  const plan = buildLaunchPlan({ out });
  for (const name of ['materials', 'data', 'profile', 'extensions']) fs.mkdirSync(path.join(out, name));
  fs.mkdirSync(path.join(out, 'profile', 'User'));
  fs.writeFileSync(path.join(out, 'profile', 'User', 'settings.json'), JSON.stringify({ 'git.enabled': false, 'chat.disableAIFeatures': true, 'telemetry.telemetryLevel': 'off', 'workbench.startupEditor': 'none', 'workbench.secondarySideBar.defaultVisibility': 'hidden', 'files.autoSave': 'off' }));
  const fd = fs.openSync(path.join(out, 'host.log'), 'w');
  const child = spawn(plan.env.READMD_VSCODE_BIN, plan.args, { env: plan.env, windowsHide: true, stdio: ['ignore', fd, fd] });
  const driver = spawn(process.execPath, [plan.driver], { env: plan.env, windowsHide: true, stdio: ['ignore', fd, fd] });
  const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
  try {
    const deadline = Date.now() + 120000;
    while (Date.now() < deadline) {
      if (fs.existsSync(path.join(out, 'report.json'))) {
        const report = JSON.parse(fs.readFileSync(path.join(out, 'report.json'), 'utf8'));
        console.log(JSON.stringify({ passed: report.passed, checks: report.checks.length, report: path.relative(ROOT, path.join(out, 'report.json')), message: report.message }));
        process.exitCode = report.passed ? 0 : 1;
        return;
      }
      await delay(200);
    }
    throw Error('Native OpenAPI Extension Host timed out');
  } finally { driver.kill(); child.kill(); fs.closeSync(fd); }
}

module.exports = { ROOT, buildLaunchPlan, publicPlan, main };
if (require.main === module) {
  main().catch(error => { console.error(error.message); process.exitCode = 1; });
}
