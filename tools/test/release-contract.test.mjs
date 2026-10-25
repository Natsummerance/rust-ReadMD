// Release and packaging contracts (ported from the retired pytest suite).
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const read = rel => fs.readFileSync(path.join(root, rel), 'utf8');
const exists = rel => fs.existsSync(path.join(root, rel));

test('platform matrix distinguishes native evidence from HarmonyOS source integration', () => {
  const matrix = JSON.parse(read('release/platform-matrix.json'));
  const ids = new Set(matrix.formal_support.map(item => item.id));
  for (const id of ['windows-x64', 'macos-x64', 'macos-arm64', 'linux-x64', 'linux-arm64']) {
    assert.ok(ids.has(id), id);
  }
  assert.equal(matrix.release, read('VERSION').trim());
  assert.ok(matrix.experimental_support.some(p=>p.id==='harmonyos-arm64' && p.status==='source-integrated-sdk-device-unverified' && p.artifacts.length===0));
  assert.ok(matrix.formal_support.every(p=>p.status));
  assert.equal(matrix.evidence_policy.release_blocked_until_complete, false);
  assert.equal(matrix.evidence_policy.full_capability_certification_blocked_until_complete, true);
  assert.deepEqual(matrix.evidence_policy.scoped_release, {
    version: '0.0.5',
    scope: 'docs/design/v005-release-scope-2026-10-25.md',
    candidate_evidence_required: true,
    native_evidence_not_inferred_from_build: true,
    deferred_version: '0.0.6',
    targets: ['windows-x64', 'linux-x64', 'macos-x64', 'macos-arm64'],
  });
  assert.equal(matrix.evidence_policy.scoped_release.version, matrix.release);
  assert.ok(exists(matrix.evidence_policy.scoped_release.scope));
  assert.ok(matrix.formal_support.every(item => item.native_backend));
  assert.equal(matrix.feature_matrix.no_implicit_empty_cells, true);
  assert.equal(matrix.feature_matrix.default_status, 'pending-native-evidence');
  for (const f of ['open', 'edit', 'save', 'preview', 'convert', 'export', 'ocr', 'ai', 'skills', 'vsix', 'mcp']) {
    assert.ok(matrix.features.includes(f), f);
  }
});

test('release workflow ships no HAP source archive and needs no Python', () => {
  const workflow = read('.github/workflows/release.yml');
  assert.ok(!workflow.includes('ReadMD-harmonyos'));
  assert.ok(!/setup-python|readmd_mcp_server\.py|requirements-common\.txt/.test(workflow));
  assert.ok(!read('scripts/linux/build_linux.sh').includes('.hap'));
  assert.ok(!read('release/release_notes.md').includes('.hap'));
  for (const name of ['README.md', 'README.en.md', 'README.ja.md', 'README.zh-TW.md']) {
    assert.ok(!read(name).includes('releases/latest/download/ReadMD-harmonyos'), name);
  }
});

test('release workflow builds every desktop target from the Rust workspace', () => {
  const workflow = read('.github/workflows/release.yml');
  assert.match(workflow, /tags:\s*\[\s*'v\*',\s*'V\*'\s*\]/);
  for (const os of ['windows', 'ubuntu', 'macos']) assert.match(workflow, new RegExp(os + '-'), os);
  assert.match(workflow, /cargo build/);
  assert.match(workflow, /node tools\/publish-release\.mjs --assets-dir release-assets/);
  assert.match(read('tools/publish-release.mjs'), /readFileSync\(path.join\(root,'RELEASE_NOTES.md'\)/);
  assert.ok(exists('RELEASE_NOTES.md'));
});

test('native release packages remap build roots and pass privacy validation before archiving',()=>{
  const workflow=read('.github/workflows/release.yml');
  const build=workflow.slice(workflow.indexOf('\n  build:'),workflow.indexOf('\n  release:'));
  const remap=build.indexOf('node tools/native-build-env.mjs --github-env');
  assert(remap>=0&&remap<build.indexOf('Build release binary with Cargo'));
  assert(build.indexOf('node tools/privacy-scan.mjs dist/pet')>build.indexOf('-- pet-package --platform'));
  assert(build.indexOf('node tools/privacy-scan.mjs dist/pet')<build.indexOf('Package Windows Artifacts'));
  for(const [gate,archive] of [
    ['node tools/privacy-scan.mjs "$pkgDir"','Compress-Archive -Path'],
    ['node tools/privacy-scan.mjs "$PKG_DIR"','tar -czvf'],
    ['node tools/privacy-scan.mjs "$APP_DIR"','zip -r "${{ matrix.platform.asset_name }}.zip"'],
  ])assert(build.indexOf(gate)>=0&&build.indexOf(gate)<build.indexOf(archive),gate);
  const sources=read('tools/test/freeze-longtail-candidate.cjs');
  assert(sources.includes('tools/native-build-env.mjs'));
  assert(sources.includes('tools/build-svg-renderer.mjs'));
});

test('HarmonyOS scaffold stays structurally buildable', () => {
  const h = 'packages/harmonyos-app/';
  for (const rel of [
    'build-profile.json5', 'hvigorfile.ts', 'oh-package.json5', 'entry/build-profile.json5',
    'entry/hvigorfile.ts', 'entry/oh-package.json5', 'entry/src/main/module.json5',
    'entry/src/main/resources/base/profile/main_pages.json',
  ]) assert.ok(exists(h + rel), rel);
  const res = h + 'entry/src/main/resources/base/';
  const names = new Set(JSON.parse(read(res + 'element/string.json')).string.map(i => i.name));
  for (const n of ['module_desc', 'EntryAbility_desc', 'EntryAbility_label']) assert.ok(names.has(n), n);
  assert.ok(JSON.parse(read(res + 'element/color.json')).color.some(i => i.name === 'start_window_background'));
  assert.ok(exists(res + 'media/icon.png'));
  assert.ok(exists(h + 'AppScope/resources/base/media/app_icon.png'));
  const pkg = JSON.parse(read(h + 'package.json'));
  assert.equal(pkg.scripts['sync:web'], 'node scripts/sync-web-assets.mjs');
  const script = read(h + 'scripts/sync-web-assets.mjs');
  assert.ok(script.includes("../../assets"));
  assert.ok(script.includes('kernel-manifest.json'));
  assert.ok(exists(h+'entry/src/main/cpp/readmd_bridge.cpp'));
  assert.ok(exists('rust/readmd-kernel/src/mobile_ffi.rs'));
  assert.ok(script.includes('resources/rawfile'));
  assert.ok(!exists(h + 'entry/src/main/resources/rawfile'));
});

test('formal release waits for shared quality checks and every build', () => {
  const workflow = read('.github/workflows/release.yml');
  const quality = read('.github/workflows/repo-quality.yml');
  assert.match(quality, /^  workflow_call:\s*$/m);
  assert.match(workflow, /^  quality:[\s\S]*?uses: \.\/\.github\/workflows\/repo-quality\.yml/m);
  const publishing = workflow.slice(workflow.indexOf('\n  release:'));
  assert.match(publishing, /needs:\s*\[build,\s*quality\]/);
  assert.ok(!/always\(\)|continue-on-error:\s*true/.test(publishing));
  for (const stage of ['Test kernel and xtask', 'Frontend gates', 'UI quality (Playwright)']) assert.ok(quality.includes(stage), stage);
  assert.match(quality, /VS Code and MCP quality/);
  assert.match(quality, /node tools\/package-integrations.mjs dist\/integrations/);
  assert.match(quality, /name: ReadMD-integrations/);
  assert.match(quality, /packages\/mcp-server\/test\/stdio.mjs/);
});

test('manual release runs build candidates by default and publish only with explicit consent', () => {
  const workflow = read('.github/workflows/release.yml');
  const dispatch = workflow.slice(workflow.indexOf('\n  workflow_dispatch:'), workflow.indexOf('\npermissions:'));
  const input = /^      publish_release:\r?\n((?:        [^\r\n]*\r?\n?)+)/m.exec(dispatch)?.[1];
  assert.ok(input, 'publish_release input is required');
  assert.match(input, /^        type: boolean\s*$/m);
  assert.match(input, /^        default: false\s*$/m);
  assert.match(input, /^        required: false\s*$/m);
  const quality = workflow.slice(workflow.indexOf('\n  quality:'), workflow.indexOf('\n  build:'));
  const build = workflow.slice(workflow.indexOf('\n  build:'), workflow.indexOf('\n  release:'));
  const publishing = workflow.slice(workflow.indexOf('\n  release:'));
  assert.ok(!/^    if:/m.test(build), 'all build matrix entries run for manual candidates');
  const condition = section => /^    if:\s*(.+)$/m.exec(section)?.[1].trim();
  const evaluate = (expression, event, ref, requested) => runInNewContext(expression, {
    github: { event_name: event, ref },
    inputs: requested === undefined ? {} : { publish_release: requested },
    startsWith: (value, prefix) => value.startsWith(prefix),
  }, { timeout: 100 });
  for (const [event, ref, requested, expected] of [
    ['workflow_dispatch', 'refs/heads/main', undefined, false],
    ['workflow_dispatch', 'refs/heads/main', false, false],
    ['workflow_dispatch', 'refs/heads/main', true, true],
    ['workflow_dispatch', 'refs/tags/V0.0.5', false, false],
    ['workflow_dispatch', 'refs/tags/V0.0.5', true, true],
    ['push', 'refs/tags/v0.0.5', undefined, true],
    ['push', 'refs/tags/V0.0.5', false, true],
    ['push', 'refs/heads/main', true, false],
    ['pull_request', 'refs/tags/V0.0.5', true, false],
  ]) assert.equal(evaluate(condition(publishing), event, ref, requested), expected, `${event} ${ref} publish=${requested}`);
  for (const ref of ['refs/heads/main', 'refs/tags/V0.0.5']) {
    assert.equal(evaluate(condition(quality), 'workflow_dispatch', ref, false), true, 'manual candidates keep quality checks');
  }
});

test('candidate runs retain every existing desktop artifact and integrations', () => {
  const workflow = read('.github/workflows/release.yml');
  const build = workflow.slice(workflow.indexOf('\n  build:'), workflow.indexOf('\n  release:'));
  assert.deepEqual([...build.matchAll(/^            target:\s*(\S+)\s*$/gm)].map(match => match[1]).sort(), [
    'aarch64-apple-darwin', 'x86_64-apple-darwin', 'x86_64-pc-windows-msvc', 'x86_64-unknown-linux-gnu',
  ]);
  const upload = build.slice(build.indexOf('      - name: Upload build artifact'));
  assert.match(upload, /uses: actions\/upload-artifact@v4/);
  assert.match(upload, /name: \$\{\{ matrix\.platform\.asset_name \}\}/);
  assert.match(upload, /if-no-files-found: error/);
  for (const glob of ['dist/*.zip', 'dist/*.tar.gz', 'dist/*.deb', 'dist/*.dmg', 'dist/*Setup*.exe']) {
    assert.ok(upload.includes(glob), glob);
  }
  const quality = read('.github/workflows/repo-quality.yml');
  const integrations = quality.slice(quality.indexOf('      - name: Upload integration packages'), quality.indexOf('\n  ui:'));
  assert.match(integrations, /uses: actions\/upload-artifact@v4/);
  assert.match(integrations, /name: ReadMD-integrations/);
  assert.match(integrations, /if-no-files-found: error/);
  for (const glob of ['dist/integrations/*.vsix', 'dist/integrations/*.zip']) assert.ok(integrations.includes(glob), glob);
  const publishing = workflow.slice(workflow.indexOf('\n  release:'));
  assert.match(publishing, /uses: actions\/download-artifact@v4\s+with:\s+path: release-assets\s+merge-multiple: true/);
});

test('release packages require the current pet runtime and aligned versions', () => {
  const workflow=read('.github/workflows/release.yml');
  assert.match(workflow,/cargo build --offline --locked/);
  assert.match(workflow,/pet-package --platform/);
  assert.match(workflow,/Copy-Item "dist\\pet\\ReadMD-Pet-Rust.zip".*-ErrorAction Stop/);
  assert.match(workflow,/ReadMD-Pet-Rust-linux-x86_64.zip.*usr\/share\/readmd/);
  assert.match(workflow,/ReadMD-Pet-Rust-macos-\*\.zip.*Contents\/Resources/);
  assert.match(read('ReadMD.nsi'),/File "dist\\ReadMD-windows-x64\\ReadMD-Pet-Rust.zip"/);
  const version=read('VERSION').trim();
  assert.match(read('rust/Cargo.toml'),new RegExp('version = "'+version.replaceAll('.','\\.')+'"'));
  assert.match(read('rust/readmd-kernel/src/updater.rs'),/GITHUB_REPO: &str = "Natsummerance\/rust-ReadMD"/);
  const pin=JSON.parse(read('packages/readmd-pet-rust/runtime-assets/manifest.json'));
  assert.equal(pin.files.length,4);
  for(const file of pin.files)assert.ok(exists('packages/readmd-pet-rust/runtime-assets/'+file.path));
  assert.ok(!exists('packages/readmd-pet-rust/runtime-assets/vendor/live2dcubismcore.min.js'));
});
