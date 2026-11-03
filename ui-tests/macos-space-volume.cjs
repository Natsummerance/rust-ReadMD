// Native small-volume integration: own every file and unmount in finally.
// No UI automation, no real user profile, no model calls.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const {spawnSync} = require('node:child_process');
const crypto = require('node:crypto');
const args = process.argv.slice(2);
const option = name => { const i = args.indexOf(name); if (i < 0 || !args[i + 1]) throw Error('Missing ' + name); return path.resolve(args[i + 1]); };
if (process.platform !== 'darwin') throw Error('Native macOS required');
const out = option('--out'), binary = option('--kernel-test'), assets = option('--assets'), pack = option('--pack');
if (fs.existsSync(out)) throw Error('Refuse reusing evidence directory');
fs.mkdirSync(out, {recursive: true, mode: 0o700});
const image = path.join(out, 'owned-small-volume.dmg'), mount = path.join(out, 'mount'), report = path.join(out, 'native-space.json');
fs.mkdirSync(mount, {mode: 0o700});
const sha = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const run = (program, argv, env = process.env, limit = 120000) => {
  const r = spawnSync(program, argv, {env, encoding: 'utf8', timeout: limit, maxBuffer: 1024 * 1024});
  if (r.error || r.status !== 0) throw Error(path.basename(program) + ' failed: ' + String(r.stderr || r.stdout || r.error).replaceAll(out, '<evidence>'));
  return r;
};
let mounted = false;
const started = Date.now();
try {
  run('/usr/bin/hdiutil', ['create', '-size', '256m', '-fs', 'HFS+', '-type', 'UDIF', '-nospotlight', '-volname', 'ReadMD P0 Owned', image]);
  run('/usr/bin/hdiutil', ['attach', '-nobrowse', '-noautoopen', '-mountpoint', mount, image]);
  mounted = true;
  fs.writeFileSync(path.join(mount, '.readmd-owned-small-volume-v1'), 'ReadMD P0 capacity evidence only\n', {flag: 'wx'});
  const env = {...process.env, READMD_SPACE_OWNED_VOLUME: mount, READMD_SPACE_INSTALL_ASSETS: assets,
    READMD_SPACE_INSTALL_PACK: pack, READMD_SPACE_REPORT_PATH: report};
  delete env.READMD_SPACE_REOPEN_ONLY;
  const testArgs = ['--exact', 'capability_packs::space_macos_tests::native_small_volume_preflight_enospc_cancel_and_reopen', '--ignored', '--nocapture', '--test-threads=1'];
  const first = run(binary, testArgs, env);
  fs.writeFileSync(path.join(out, 'first-process.log'), first.stdout + first.stderr);
  const second = run(binary, testArgs, {...env, READMD_SPACE_REOPEN_ONLY: '1'});
  fs.writeFileSync(path.join(out, 'reopen-process.log'), second.stdout + second.stderr);
  const value = JSON.parse(fs.readFileSync(report, 'utf8'));
  if (!value.passed || !value.native_enospc || !value.new_process_reopened || !value.pending_review_retained) throw Error('Missing consumer evidence');
  value.kernel_test_sha256 = sha(binary); value.offline_pack_sha256 = sha(pack);
  value.duration_ms = Date.now() - started;
  fs.writeFileSync(report, JSON.stringify(value, null, 2) + '\n');
} finally {
  if (mounted) { run('/usr/bin/hdiutil', ['detach', mount]); mounted = false; }
  // Only these exact two objects were created by this fresh invocation. Do not
  // recursively clean mount contents or follow an unexpected mount/symlink.
  if (fs.existsSync(image)) {
    const s = fs.lstatSync(image); if (!s.isFile() || s.isSymbolicLink()) throw Error('Image identity changed; preserve for inspection');
    fs.unlinkSync(image);
  }
  if (!fs.lstatSync(mount).isDirectory() || fs.lstatSync(mount).isSymbolicLink() || fs.readdirSync(mount).length) throw Error('Mount cleanup requires inspection');
  fs.rmdirSync(mount);
}
console.log(JSON.stringify({passed: true, native_enospc: true, new_process_reopened: true, temporary_image_removed: true}));
