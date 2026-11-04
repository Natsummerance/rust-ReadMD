'use strict';
// Local compiler and read-only plans only. This suite never starts VS Code.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), cp = require('node:child_process');
const ROOT = path.resolve(__dirname, '..'), launcherFile = path.join(__dirname, 'run-vscode-openapi-native.cjs');
const launcher = require(launcherFile), sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const dependencies = process.env.READMD_VSCODE_DEPENDENCIES || path.join(ROOT, 'packages/vscode-extension/node_modules');
const output = path.resolve(process.env.READMD_OPENAPI_PREFLIGHT_OUTPUT || path.join(ROOT, '.cache/capability-upgrade/openapi-preflight-unit'));
assert(output.startsWith(ROOT + path.sep) || output.startsWith(path.resolve(ROOT, '..') + path.sep), 'Owned workspace output required');
fs.mkdirSync(output, { recursive: true });
const environment = { READMD_BIN: process.execPath, READMD_VSCODE_BIN: process.execPath, READMD_VSCODE_DEPENDENCIES: dependencies };
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
function fixture() {
  const root = fs.mkdtempSync(path.join(output, 'case-')), extension = path.join(root, 'packages/vscode-extension');
  for (const name of ['assets', 'ui-tests', 'packages/vscode-extension/src', 'packages/vscode-extension/l10n']) fs.mkdirSync(path.join(root, name), { recursive: true });
  fs.writeFileSync(path.join(extension, 'package.json'), JSON.stringify({ name: 'owned-preflight-fixture', main: './out/extension.js' }));
  fs.copyFileSync(path.join(ROOT, 'packages/vscode-extension/package-lock.json'), path.join(extension, 'package-lock.json'));
  fs.writeFileSync(path.join(extension, 'tsconfig.json'), JSON.stringify({ compilerOptions: { module: 'commonjs', target: 'ES2022', rootDir: 'src', outDir: 'out', strict: true, sourceMap: true } }));
  fs.writeFileSync(path.join(extension, 'src/extension.ts'), 'export const activate = (): number => 7;\n');
  fs.writeFileSync(path.join(extension, 'l10n/bundle.l10n.json'), '{}\n');
  for (const name of ['run-vscode-openapi-native.cjs', 'vscode-openapi-native.cjs', 'vscode-openapi-driver.cjs']) fs.copyFileSync(path.join(ROOT, 'ui-tests', name), path.join(root, 'ui-tests', name));
  fs.cpSync(path.join(ROOT, 'tests/fixtures/openapi/v007'), path.join(root, 'tests/fixtures/openapi/v007'), { recursive: true, errorOnExist: true });
  return { root, extension, out: path.join(extension, 'out'), source: path.join(extension, 'src/extension.ts'), main: path.join(extension, 'out/extension.js') };
}
function ownedTest(name, body) {
  test(name, async t => {
    const area = fixture(), identity = fs.lstatSync(area.root);
    try { await body(t, area); }
    catch (error) {
      const file = path.join(output, name.replace(/[^a-z0-9]+/gi, '-').slice(0, 90) + '-first-failure.json');
      if (!fs.existsSync(file)) fs.writeFileSync(file, JSON.stringify({ name, error: String(error.message), source: fs.readFileSync(area.source, 'utf8') }, null, 2));
      throw error;
    } finally {
      const now = fs.lstatSync(area.root); assert(now.isDirectory() && !now.isSymbolicLink() && now.ino === identity.ino && now.dev === identity.dev);
      fs.rmSync(area.root, { recursive: true });
    }
  });
}
const prepare = area => launcher.prepareExtension({ root: area.root, dependencies });
const plan = area => launcher.buildLaunchPlan({ root: area.root, environment });
ownedTest('missing main and unbound outputs fail closed without writes or GUI starts', async (_t, area) => {
  const missing = plan(area); assert.equal(missing.ready, false); assert.equal(missing.build.reason, 'extension_entrypoint_missing');
  let spawns = 0; const originalSpawn = cp.spawn, originalWrite = fs.writeFileSync, originalMkdir = fs.mkdirSync, originalTemp = fs.mkdtempSync, originalLog = console.log, originalExit = process.exitCode;
  try {
    cp.spawn = () => { spawns++; throw Error('GUI forbidden'); };
    fs.writeFileSync = fs.mkdirSync = fs.mkdtempSync = () => { throw Error('Plan cannot write'); };
    console.log = () => {};
    await launcher.main(['--plan'], { root: area.root, environment });
    await assert.rejects(launcher.main([], { root: area.root, environment }), /extension_entrypoint_missing/);
    assert.equal(spawns, 0);
  } finally { cp.spawn = originalSpawn; fs.writeFileSync = originalWrite; fs.mkdirSync = originalMkdir; fs.mkdtempSync = originalTemp; console.log = originalLog; process.exitCode = originalExit; }
  fs.mkdirSync(area.out); fs.writeFileSync(area.main, 'exports.activate=()=>7;\n');
  assert.equal(plan(area).build.reason, 'extension_binding_missing');
  await assert.rejects(prepare(area)); assert.equal(fs.readFileSync(area.main, 'utf8'), 'exports.activate=()=>7;\n');
});
ownedTest('explicit fresh locked TypeScript compile binds main source and emitted bytes', async (_t, area) => {
  const lock = path.join(dependencies, '.package-lock.json'), compiler = path.join(dependencies, 'typescript/lib/typescript.js');
  const dependencyBefore = { lock: sha(fs.readFileSync(lock)), compiler: sha(fs.readFileSync(compiler)) };
  const prepared = await prepare(area); assert.equal(prepared.ready, true); assert.equal(plan(area).ready, true);
  const receipt = JSON.parse(fs.readFileSync(path.join(area.out, launcher.RECEIPT), 'utf8'));
  assert.equal(receipt.main, 'packages/vscode-extension/out/extension.js'); assert(receipt.inputs.some(row => row.path.endsWith('src/extension.ts'))); assert(receipt.outputs.some(row => row.path === 'extension.js'));
  assert.equal(receipt.toolchain.packages.find(row => row.name === 'typescript').version, '5.9.3');
  assert.equal(sha(fs.readFileSync(lock)), dependencyBefore.lock); assert.equal(sha(fs.readFileSync(compiler)), dependencyBefore.compiler);
  assert.equal(launcher.publicPlan(plan(area)).nativeExtensionHostCertified, false); assert.equal(launcher.publicPlan(plan(area)).acceptance, 'not_run');
  const receiptFile = path.join(area.out, launcher.RECEIPT); receipt.inputs[0].sha256 = '0'.repeat(64); fs.writeFileSync(receiptFile, JSON.stringify(receipt));
  assert.equal(plan(area).ready, false); assert.equal(plan(area).build.reason, 'extension_binding_invalid');
});
ownedTest('source edits reject a prior ready plan and explicit reprepare binds the new source', async (_t, area) => {
  const first = await prepare(area); fs.appendFileSync(area.source, 'export const revision = 2;\n');
  assert.equal(plan(area).ready, false); assert.equal(plan(area).build.reason, 'extension_sources_changed');
  await assert.rejects(launcher.main([], { root: area.root, environment }), /extension_sources_changed/);
  const next = await prepare(area); assert.equal(next.ready, true); assert.notEqual(next.inputsSha256, first.inputsSha256); assert.notEqual(next.outputsSha256, first.outputsSha256);
});
ownedTest('output edits unknown objects and manifest main changes cannot reuse a binding', async (_t, area) => {
  await prepare(area); const original = fs.readFileSync(area.main);
  fs.appendFileSync(area.main, '\n// changed after preparation\n'); assert.equal(plan(area).build.reason, 'extension_outputs_changed');
  await assert.rejects(prepare(area)); assert(fs.readFileSync(area.main, 'utf8').includes('changed after preparation'));
  fs.writeFileSync(area.main, original); fs.writeFileSync(path.join(area.out, 'unknown-note.txt'), 'keep unknown output');
  assert.equal(plan(area).build.reason, 'extension_outputs_changed'); await assert.rejects(prepare(area)); assert.equal(fs.readFileSync(path.join(area.out, 'unknown-note.txt'), 'utf8'), 'keep unknown output');
  fs.unlinkSync(path.join(area.out, 'unknown-note.txt'));
  const manifestFile = path.join(area.extension, 'package.json'), manifest = JSON.parse(fs.readFileSync(manifestFile));
  manifest.main = './out/other.js'; fs.writeFileSync(manifestFile, JSON.stringify(manifest)); fs.copyFileSync(area.main, path.join(area.out, 'other.js'));
  assert.equal(plan(area).ready, false); assert.equal(plan(area).build.reason, 'extension_manifest_main_changed');
});
ownedTest('compile errors emit no ready output and leave no owned compile directory', async (_t, area) => {
  fs.writeFileSync(area.source, 'export const broken: number = "invalid";\n');
  await assert.rejects(prepare(area), /extension_compile_failed/); assert.equal(fs.existsSync(area.out), false);
  assert.deepEqual(fs.readdirSync(area.extension).filter(name => name.startsWith('.openapi-')), []);
});
ownedTest('a source changed during a real separate compiler process is refused', async (_t, area) => {
  const code = 'require(process.argv[1]).prepareExtension({root:process.argv[2],dependencies:process.argv[3]}).then(()=>process.exit(0)).catch(e=>{process.stdout.write(e.code||e.message);process.exit(3)});';
  const child = cp.spawn(process.execPath, ['-e', code, launcherFile, area.root, dependencies], { cwd: area.root, stdio: ['ignore', 'pipe', 'pipe'], env: { PATH: process.env.PATH, TMPDIR: area.root, TMP: area.root, TEMP: area.root } });
  let stdout = '', stderr = ''; child.stdout.on('data', bytes => { stdout += bytes; }); child.stderr.on('data', bytes => { stderr += bytes; });
  const closed = new Promise(resolve => child.once('close', (code, signal) => resolve({ code, signal }))); let changed = false;
  try {
    const end = Date.now() + 15000;
    while (Date.now() < end && child.exitCode === null) { if (fs.readdirSync(area.extension).some(name => name.startsWith('.openapi-compile-'))) { fs.appendFileSync(area.source, 'export const raced = 3;\n'); changed = true; break; } await pause(5); }
    assert.equal(changed, true, 'Reached actual source-snapshot to compiler boundary');
    const result = await closed; assert.deepEqual(result, { code: 3, signal: null }, stderr); assert.equal(stdout, 'extension_sources_changed_during_compile');
    assert.equal(fs.existsSync(area.out), false); assert.deepEqual(fs.readdirSync(area.extension).filter(name => name.startsWith('.openapi-')), []);
  } finally { if (child.exitCode === null) { child.kill(); await closed; } }
});
ownedTest('locked dependency mismatch refuses preparation before producing output', async (_t, area) => {
  const fake = path.join(area.root, 'mismatched-dependencies'); fs.mkdirSync(path.join(fake, 'typescript'), { recursive: true });
  fs.copyFileSync(path.join(dependencies, '.package-lock.json'), path.join(fake, '.package-lock.json'));
  fs.writeFileSync(path.join(fake, 'typescript/package.json'), JSON.stringify({ version: '0.0.0' }));
  await assert.rejects(launcher.prepareExtension({ root: area.root, dependencies: fake }), /extension_dependencies_lock_mismatch/); assert.equal(fs.existsSync(area.out), false);
});
ownedTest('staged complex fixture bytes are bound and source inputs remain unchanged', async (_t, area) => {
  const prepared = await prepare(area), audit = path.join(area.root, 'audit'); fs.mkdirSync(audit);
  const staged = launcher.stageFixtures(area.root, audit); assert.equal(staged.sourceManifest.length, 14);
  assert(staged.sourceManifest.some(row => row.path === 'bundle/schemas/complex.json'));
  fs.appendFileSync(path.join(staged.root, 'bundle/defs/scalar.yaml'), '# change owned staged dependency\n');
  assert.equal(plan(area).ready, true); assert.equal(plan(area).build.inputsSha256, prepared.inputsSha256);
  fs.appendFileSync(path.join(area.root, 'tests/fixtures/openapi/v007/bundle/defs/scalar.yaml'), '# source fixture changed\n');
  assert.equal(plan(area).ready, false); assert.equal(plan(area).build.reason, 'extension_sources_changed');
});
