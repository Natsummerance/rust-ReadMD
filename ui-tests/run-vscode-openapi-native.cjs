'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { spawn } = require('node:child_process');
const ROOT = path.resolve(__dirname, '..'), RECEIPT = '.readmd-openapi-build.json';
const sha = value => crypto.createHash('sha256').update(value).digest('hex'), digest = value => sha(JSON.stringify(value));
const within = (root, file) => { const relative = path.relative(root, file); return relative && relative !== '..' && !relative.startsWith('..' + path.sep) && !path.isAbsolute(relative); };
function fail(code) { throw Object.assign(Error(code), { code }); }
function ordinary(file, directory = false) {
  const stat = fs.lstatSync(file);
  if (stat.isSymbolicLink() || !(directory ? stat.isDirectory() : stat.isFile())) fail('extension_binding_link_or_type');
  return stat;
}
function tree(directory, base = directory) {
  ordinary(directory, true);
  return fs.readdirSync(directory).sort().flatMap(name => {
    const file = path.join(directory, name), stat = fs.lstatSync(file);
    if (stat.isSymbolicLink()) fail('extension_binding_link_or_type');
    if (stat.isDirectory()) return tree(file, base);
    ordinary(file); return [{ path: path.relative(base, file).split(path.sep).join('/'), bytes: stat.size, sha256: sha(fs.readFileSync(file)) }];
  });
}
function record(root, file) { const stat = ordinary(file); return { path: path.relative(root, file).split(path.sep).join('/'), bytes: stat.size, sha256: sha(fs.readFileSync(file)) }; }
function layout(root) {
  const extension = path.join(root, 'packages/vscode-extension');
  const manifest = JSON.parse(fs.readFileSync(path.join(extension, 'package.json'), 'utf8'));
  if (typeof manifest.main !== 'string' || path.isAbsolute(manifest.main)) fail('extension_manifest_main_invalid');
  const out = path.join(extension, 'out'), main = path.resolve(extension, manifest.main);
  if (!within(out, main) || path.extname(main) !== '.js' || /[\0\r\n]/.test(manifest.main)) fail('extension_manifest_main_invalid');
  return { extension, out, main, mainRelative: path.relative(root, main).split(path.sep).join('/') };
}
function inputs(root) {
  const { extension } = layout(root);
  const selected = ['package.json', 'package-lock.json', 'tsconfig.json'].map(name => path.join(extension, name));
  for (const name of ['src', 'l10n']) for (const item of tree(path.join(extension, name))) selected.push(path.join(extension, name, item.path));
  for (const name of fs.readdirSync(extension).filter(name => /^package\.nls(?:\.[^.]+)?\.json$/.test(name))) selected.push(path.join(extension, name));
  for (const name of ['run-vscode-openapi-native.cjs', 'vscode-openapi-native.cjs', 'vscode-openapi-driver.cjs']) selected.push(path.join(root, 'ui-tests', name));
  const fixtures = path.join(root, 'tests/fixtures/openapi/v007');
  for (const item of tree(fixtures)) selected.push(path.join(fixtures, item.path));
  return [...new Set(selected)].sort().map(file => record(root, file));
}
function toolchain(root, dependencies) {
  if (!dependencies || !path.isAbsolute(dependencies)) fail('extension_dependencies_explicit_path_required');
  ordinary(dependencies, true);
  const lock = JSON.parse(fs.readFileSync(path.join(root, 'packages/vscode-extension/package-lock.json'), 'utf8'));
  const installed = JSON.parse(fs.readFileSync(path.join(dependencies, '.package-lock.json'), 'utf8')), packages = [];
  for (const name of ['typescript', '@types/node', '@types/vscode', 'undici-types']) {
    const expected = lock.packages?.['node_modules/' + name], actual = installed.packages?.['node_modules/' + name];
    const directory = path.join(dependencies, name), metadata = JSON.parse(fs.readFileSync(path.join(directory, 'package.json'), 'utf8'));
    if (!expected || !actual || !expected.integrity || expected.integrity !== actual.integrity || expected.version !== actual.version || metadata.version !== expected.version) fail('extension_dependencies_lock_mismatch');
    packages.push({ name, version: metadata.version, integrity: expected.integrity, filesSha256: digest(tree(directory)) });
  }
  return { node: { version: process.version, sha256: sha(fs.readFileSync(process.execPath)) }, packages };
}
function outputSnapshot(out) { return tree(out).filter(item => item.path !== RECEIPT); }
function readReceipt(out) {
  const file = path.join(out, RECEIPT); if (ordinary(file).size > 2 * 1048576) fail('extension_binding_invalid');
  const receipt = JSON.parse(fs.readFileSync(file, 'utf8'));
  if (receipt.schema !== 1 || !Array.isArray(receipt.inputs) || !Array.isArray(receipt.outputs) || typeof receipt.dependencies !== 'string') fail('extension_binding_invalid');
  return receipt;
}
function verifyPrepared(root, dependencies) {
  try {
    const locations = layout(root);
    if (!fs.existsSync(locations.main)) return { ready: false, reason: 'extension_entrypoint_missing', main: locations.mainRelative };
    ordinary(locations.main);
    if (!fs.existsSync(path.join(locations.out, RECEIPT))) return { ready: false, reason: 'extension_binding_missing', main: locations.mainRelative };
    const receipt = readReceipt(locations.out);
    if (digest(receipt.inputs) !== receipt.inputsSha256 || digest(receipt.toolchain) !== receipt.toolchainSha256) fail('extension_binding_invalid');
    if (receipt.workspaceRoot !== fs.realpathSync(root) || receipt.main !== locations.mainRelative) fail('extension_manifest_main_changed');
    if (digest(inputs(root)) !== receipt.inputsSha256) fail('extension_sources_changed');
    if (digest(outputSnapshot(locations.out)) !== receipt.outputsSha256 || digest(receipt.outputs) !== receipt.outputsSha256) fail('extension_outputs_changed');
    if (digest(toolchain(root, dependencies || receipt.dependencies)) !== receipt.toolchainSha256) fail('extension_toolchain_changed');
    return { ready: true, reason: null, main: locations.mainRelative, mainSha256: sha(fs.readFileSync(locations.main)), inputsSha256: receipt.inputsSha256, outputsSha256: receipt.outputsSha256, toolchainSha256: receipt.toolchainSha256, nativeExtensionHostCertified: false };
  } catch (error) { return { ready: false, reason: error.code?.startsWith('extension_') ? error.code : 'extension_binding_invalid' }; }
}

// Explicit local preparation uses the locked compiler API. No npm, lifecycle
// script, dependency installation or GUI is invoked.
async function prepareExtension({ root = ROOT, dependencies = process.env.READMD_VSCODE_DEPENDENCIES } = {}) {
  const locations = layout(root), beforeInputs = inputs(root), beforeToolchain = toolchain(root, dependencies);
  const ts = require(path.join(dependencies, 'typescript/lib/typescript.js'));
  const config = ts.readConfigFile(path.join(locations.extension, 'tsconfig.json'), ts.sys.readFile);
  if (config.error || config.config.extends) fail('extension_config_invalid');
  const stage = fs.mkdtempSync(path.join(locations.extension, '.openapi-compile-')), identity = fs.lstatSync(stage);
  let retired;
  const removeStage = directory => { ordinary(directory, true); fs.rmSync(directory, { recursive: true }); };
  try {
    const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, locations.extension, { outDir: stage, incremental: false, noEmitOnError: true, typeRoots: [path.join(dependencies, '@types')], types: ['node', 'vscode'] });
    if (parsed.errors.length || !parsed.fileNames.length || parsed.fileNames.some(file => !within(path.join(locations.extension, 'src'), path.resolve(file)))) fail('extension_config_invalid');
    const compilerHost = ts.createCompilerHost(parsed.options);
    compilerHost.writeFile = (file, content) => { if (!within(stage, path.resolve(file))) fail('extension_emit_escape'); fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, content); };
    const program = ts.createProgram(parsed.fileNames, parsed.options, compilerHost), diagnostics = ts.getPreEmitDiagnostics(program), emitted = program.emit();
    if (diagnostics.length || emitted.emitSkipped || emitted.diagnostics.length) fail('extension_compile_failed');
    if (digest(inputs(root)) !== digest(beforeInputs) || digest(toolchain(root, dependencies)) !== digest(beforeToolchain)) fail('extension_sources_changed_during_compile');
    ordinary(path.join(stage, path.relative(locations.out, locations.main)));
    const outputs = outputSnapshot(stage);
    const receipt = { schema: 1, scope: 'OpenAPI Extension Host TypeScript preparation; no native runtime certification', workspaceRoot: fs.realpathSync(root), main: locations.mainRelative, inputs: beforeInputs, inputsSha256: digest(beforeInputs), outputs, outputsSha256: digest(outputs), dependencies: path.resolve(dependencies), toolchain: beforeToolchain, toolchainSha256: digest(beforeToolchain), preparedAt: new Date().toISOString() };
    fs.writeFileSync(path.join(stage, RECEIPT), JSON.stringify(receipt, null, 2) + '\n', { flag: 'wx' });
    if (fs.existsSync(locations.out)) {
      // Never sweep unknown output data. A previous receipt must account for
      // every old object before its stale source build may be replaced.
      const old = readReceipt(locations.out);
      if (old.workspaceRoot !== fs.realpathSync(root) || digest(outputSnapshot(locations.out)) !== old.outputsSha256 || digest(old.outputs) !== old.outputsSha256) fail('extension_output_unowned');
      retired = path.join(locations.extension, '.openapi-retired-' + crypto.randomUUID()); fs.renameSync(locations.out, retired);
    }
    try { fs.renameSync(stage, locations.out); } catch (error) { if (retired) { fs.renameSync(retired, locations.out); retired = undefined; } throw error; }
    if (retired) { removeStage(retired); retired = undefined; }
    const verified = verifyPrepared(root, dependencies); if (!verified.ready) fail(verified.reason);
    return verified;
  } finally {
    if (fs.existsSync(stage)) { const now = fs.lstatSync(stage); if (now.ino !== identity.ino || now.dev !== identity.dev) fail('extension_output_identity_changed'); removeStage(stage); }
  }
}

// Importing and --plan only read. They neither compile nor launch.
function buildLaunchPlan({ root = ROOT, out = path.join(root, '.cache/capability-upgrade/vscode-openapi-PLANNED'), environment = process.env } = {}) {
  const assets = path.join(root, 'assets'), extension = path.join(root, 'packages/vscode-extension');
  const extensionTests = path.join(root, 'ui-tests/vscode-openapi-native.cjs'), driver = path.join(root, 'ui-tests/vscode-openapi-driver.cjs');
  const env = { ...environment, READMD_VSCODE_OPENAPI_AUDIT: out, READMD_VSCODE_DEBUG_PORT: '29317', READMD_DATA_DIR: path.join(out, 'data'), READMD_ASSETS_DIR: assets, READMD_VSCODE_OPENAPI_FIXTURES: path.join(root, 'tests/fixtures/openapi/v007') };
  const args = ['--new-window', '--skip-welcome', '--skip-release-notes', '--disable-updates', '--disable-workspace-trust', '--locale=en-US', '--remote-debugging-port=' + env.READMD_VSCODE_DEBUG_PORT,
    '--user-data-dir=' + path.join(out, 'profile'), '--extensions-dir=' + path.join(out, 'extensions'), '--extensionDevelopmentPath=' + extension, '--extensionTestsPath=' + extensionTests, path.join(out, 'materials')];
  const checks = [
    { id: 'assets', file: assets, kind: 'directory' }, { id: 'extension', file: extension, kind: 'directory' },
    { id: 'extension-manifest', file: path.join(extension, 'package.json'), kind: 'file' }, { id: 'extension-tests', file: extensionTests, kind: 'file' },
    { id: 'graphical-driver', file: driver, kind: 'file' }, { id: 'openapi-fixtures', file: env.READMD_VSCODE_OPENAPI_FIXTURES, kind: 'directory' },
    { id: 'readmd-binary', file: environment.READMD_BIN, kind: 'file', external: true }, { id: 'vscode-binary', file: environment.READMD_VSCODE_BIN, kind: 'file', external: true },
  ].map(check => { let exists = false; try { const stat = check.file && fs.lstatSync(check.file); exists = !!stat && !stat.isSymbolicLink() && (check.kind === 'directory' ? stat.isDirectory() : stat.isFile()); } catch {} return { ...check, exists }; });
  const build = verifyPrepared(root, environment.READMD_VSCODE_DEPENDENCIES);
  return { root, out, env, args, driver, checks, build, ready: checks.every(check => check.exists) && build.ready };
}
function publicPlan(plan) {
  return { mode: 'launch-plan-only', launched: false, filesystemChanged: false, ready: plan.ready, build: plan.build,
    checks: plan.checks.map(check => ({ id: check.id, kind: check.kind, exists: check.exists, path: check.external ? (check.file ? '[external-binary]' : null) : path.relative(plan.root, check.file).split(path.sep).join('/') })),
    requiresGraphicalDriver: true, nativeExtensionHostCertified: false, acceptance: 'not_run' };
}
function stageFixtures(root, out) {
  const source = path.join(root, 'tests/fixtures/openapi/v007'), destination = path.join(out, 'materials/openapi'), snapshot = tree(source);
  for (const item of snapshot) { const file = path.join(destination, item.path); fs.mkdirSync(path.dirname(file), { recursive: true }); fs.copyFileSync(path.join(source, item.path), file, fs.constants.COPYFILE_EXCL); }
  if (digest(tree(destination)) !== digest(snapshot)) fail('extension_fixture_copy_changed');
  return { sourceManifest: snapshot, sourceManifestSha256: digest(snapshot), root: destination };
}
async function main(args = process.argv.slice(2), { root = ROOT, environment = process.env } = {}) {
  if (args.length === 1 && args[0] === '--plan') { const plan = buildLaunchPlan({ root, environment }); console.log(JSON.stringify(publicPlan(plan))); if (!plan.ready) process.exitCode = 2; return plan; }
  if (args.length === 1 && args[0] === '--prepare') { const build = await prepareExtension({ root, dependencies: environment.READMD_VSCODE_DEPENDENCIES }); console.log(JSON.stringify({ mode: 'prepare-only', prepared: true, launched: false, filesystemChanged: true, build, nativeExtensionHostCertified: false, acceptance: 'not_run' })); return build; }
  if (args.length) throw Error('Only --plan and --prepare are supported');
  let plan = buildLaunchPlan({ root, environment });
  if (!plan.ready) throw Error('Launch preflight failed: ' + (plan.build.reason || 'required_path_missing') + '; run explicit --prepare and --plan first');
  const cache = path.join(root, '.cache/capability-upgrade'); fs.mkdirSync(cache, { recursive: true });
  const out = fs.mkdtempSync(path.join(cache, 'vscode-openapi-'));
  for (const name of ['materials', 'data', 'profile', 'extensions']) fs.mkdirSync(path.join(out, name));
  const staged = stageFixtures(root, out);
  // Revalidate immediately before starting any process; never reuse a cached
  // ready flag from an earlier plan or perform an implicit compile.
  plan = buildLaunchPlan({ root, out, environment }); if (!plan.ready) throw Error('Launch preflight changed: ' + plan.build.reason);
  const launchBinding = { build: plan.build, fixturesSha256: staged.sourceManifestSha256, kernelSha256: sha(fs.readFileSync(environment.READMD_BIN)), vscodeSha256: sha(fs.readFileSync(environment.READMD_VSCODE_BIN)), nativeExtensionHostCertified: false };
  fs.writeFileSync(path.join(out, 'launch-binding.json'), JSON.stringify(launchBinding, null, 2) + '\n', { flag: 'wx' });
  fs.mkdirSync(path.join(out, 'profile/User'));
  fs.writeFileSync(path.join(out, 'profile/User/settings.json'), JSON.stringify({ 'git.enabled': false, 'chat.disableAIFeatures': true, 'telemetry.telemetryLevel': 'off', 'workbench.startupEditor': 'none', 'workbench.secondarySideBar.defaultVisibility': 'hidden', 'files.autoSave': 'off' }));
  const fd = fs.openSync(path.join(out, 'host.log'), 'w');
  const child = spawn(plan.env.READMD_VSCODE_BIN, plan.args, { env: plan.env, windowsHide: true, stdio: ['ignore', fd, fd] });
  const driver = spawn(process.execPath, [plan.driver], { env: plan.env, windowsHide: true, stdio: ['ignore', fd, fd] });
  const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
  try {
    const deadline = Date.now() + 120000;
    while (Date.now() < deadline) {
      if (fs.existsSync(path.join(out, 'report.json'))) {
        const after = buildLaunchPlan({ root, out, environment });
        if (!after.ready || digest(after.build) !== digest(launchBinding.build) || sha(fs.readFileSync(environment.READMD_BIN)) !== launchBinding.kernelSha256) fail('extension_launch_binding_changed');
        const report = JSON.parse(fs.readFileSync(path.join(out, 'report.json'), 'utf8'));
        console.log(JSON.stringify({ passed: report.passed, checks: report.checks.length, report: path.relative(root, path.join(out, 'report.json')), message: report.message })); process.exitCode = report.passed ? 0 : 1; return;
      }
      await delay(200);
    }
    throw Error('Native OpenAPI Extension Host timed out');
  } finally { driver.kill(); child.kill(); fs.closeSync(fd); }
}
module.exports = { ROOT, RECEIPT, buildLaunchPlan, publicPlan, prepareExtension, verifyPrepared, stageFixtures, main };
if (require.main === module) main().catch(error => { console.error(error.message); process.exitCode = 1; });
