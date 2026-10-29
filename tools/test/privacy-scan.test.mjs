import assert from 'node:assert/strict';
import test from 'node:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { scanBuffer, scanNativePaths } from '../privacy-scan.mjs';

const scan = (label, text, opts) => scanBuffer(label, Buffer.from(text, 'utf8'), opts);
const fakeKey = 'sk-' + 'A'.repeat(24);

test('flags key-shaped strings and developer paths', () => {
  assert.match(scan('src/a.js', `const k = "${fakeKey}"`)[0], /plaintext API key/);
  assert.match(scan('src/a.js', 'root = "D:/Users/someone/x"')[0], /local absolute path/);
  assert.match(scan('src/a.js', 'p = "/home/dev/Projects/x"')[0], /local absolute path/);
});

test('retired provider markers, except the audited attribution files', () => {
  const marker = 'cc' + '-switch';
  assert.match(scan('src/a.js', `x ${marker}`)[0], /retired provider marker/);
  assert.deepEqual(scan('assets/providers/provider-catalog.json', `x ${marker}`), []);
});

test('sensitive document names fail before any skip rule', () => {
  assert.match(scan('tests/北京交通大学软件学院毕业实习文档.docx', '')[0], /sensitive/);
});

test('vendored, test, binary and dist content is skipped as before', () => {
  for (const label of ['assets/vendor/x.js', 'assets/upstream/a/b.md', 'tests/x.js', 'ui-tests/y.js', 'a/b.png', 'x.zip']) {
    assert.deepEqual(scan(label, fakeKey), [], label);
  }
  assert.deepEqual(scan('dist/app.js', 'C:/Users/me/x'), []);
  assert.deepEqual(scan('app.js', 'C:/Users/me/x', { sourceTree: false }), []);
});

// Synthetic roots assembled here are not a developer's real workspace.
const developerRoot = ['Q:', 'Users', 'native-fixture', 'workspace', 'main.rs'].join('/');
const unixRoot = ['', 'home', 'native-fixture', 'Projects', 'main.rs'].join('/');
function pe() {
  const header = Buffer.alloc(128); header.write('MZ'); header.writeUInt32LE(64, 0x3c); header.write('PE\0\0', 64); return header;
}
const elf = () => Buffer.concat([Buffer.from([0x7f, 0x45, 0x4c, 0x46, 2, 1, 1, 0]), Buffer.alloc(56)]);
const macho = () => Buffer.from([0xcf, 0xfa, 0xed, 0xfe, 0, 0, 0, 0]);
const native = (header, text, encoding = 'ascii', alignment = 0) => {
  let payload = Buffer.from(text, encoding === 'utf16be' ? 'utf16le' : encoding);
  if (encoding === 'utf16be') payload.swap16();
  return Buffer.concat([header, Buffer.alloc(alignment), payload, Buffer.alloc(2)]);
};

test('PE, extensionless ELF and Mach-O identify embedded ASCII developer paths', () => {
  for (const [label, header] of [['worker.exe', pe()], ['ReadMD', elf()], ['Contents/MacOS/ReadMD', macho()], ['ReadMD-universal', Buffer.from([0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 1])]]) {
    for (const root of [developerRoot, developerRoot.replaceAll('/', '\\'), unixRoot]) {
      const failures = scanNativePaths(label, native(header, root));
      assert.equal(failures.length, 1, label); assert.match(failures[0], /absolute path in native binary/);
      assert.ok(!failures[0].includes(root) && !failures[0].includes('native-fixture'), 'findings must not disclose matched strings');
    }
  }
});

test('UTF-16 LE and BE paths are detected at both byte alignments', () => {
  for (const encoding of ['utf16le', 'utf16be']) for (const alignment of [0, 1]) {
    assert.equal(scanNativePaths('worker.dll', native(pe(), developerRoot, encoding, alignment)).length, 1, `${encoding}/${alignment}`);
    assert.equal(scanNativePaths('ReadMD', native(elf(), unixRoot, encoding, alignment)).length, 1, `${encoding}/${alignment}`);
  }
});

test('explicit synthetic CI roots cover both separators, case and wide encodings', () => {
  const buildRoots = ['D:\\a', '/home/runner/.cargo', '/home/runner/.rustup'];
  for (const value of ['d:/A/source/main.rs', 'D:\\a\\source\\main.rs', '\\\\?\\D:\\a\\source\\main.rs', '/HOME/RUNNER/.CARGO/registry/src/lib.rs', '/home/runner/.rustup/toolchains/lib.rs']) {
    for (const encoding of ['ascii', 'utf16le', 'utf16be']) for (const alignment of [0, 1]) {
      const failures = scanNativePaths('worker.exe', native(pe(), value, encoding, alignment), { buildRoots });
      assert.equal(failures.length, 1); assert.ok(!failures[0].includes(value));
    }
  }
  assert.equal(scanBuffer('Contents/MacOS/ReadMD', native(macho(), 'D:/a/source/main.rs'), { sourceTree: false, buildRoots }).length, 1);
  assert.equal(scanNativePaths('ReadMD', native(elf(), 'D:/a/src/main.rs'), { buildRoots: ['D:/a/child/..//'] }).length, 1);
});

test('build-root boundaries and invalid roots cannot turn similar text into a finding', () => {
  const buildRoots = ['D:\\a', '/home/runner/.cargo'];
  for (const value of ['D:/another/src.rs', 'prefixD:/a/src.rs', '/home/runner/.cargo2/src.rs', '/home/runner/.cargo-old/src.rs', 'prefix/home/runner/.cargo/src.rs', 'src/main.rs']) {
    assert.deepEqual(scanNativePaths('ReadMD', native(elf(), value), { buildRoots }), []);
  }
  const invalid = [null, 0, '', '/', 'D:/', '/a', 'relative', 'D:relative', 'D:/a\ninvalid', ' D:/a'];
  assert.deepEqual(scanNativePaths('ReadMD', native(elf(), 'D:/a/src.rs /home/runner/.cargo/src.rs'), { buildRoots: invalid }), []);
  assert.deepEqual(scanNativePaths('ReadMD', native(elf(), 'D:/a/src.rs'), { buildRoots: [] }), []);
});

test('native path checks precede directory, extension, dist and external-artifact skips', () => {
  for (const label of ['assets/engines/resvg/worker.exe', 'assets/vendor/worker.dll', 'tests/fixture.node', 'dist/ReadMD', 'Contents/MacOS/ReadMD', 'Contents/Frameworks/Fixture.framework/Fixture']) {
    for (const sourceTree of [true, false]) assert.equal(scanBuffer(label, native(elf(), developerRoot), { sourceTree }).length, 1, label);
  }
  for (const label of ['worker.so.2', 'worker.dylib', 'worker.pyd', 'worker.lib', 'worker.rlib', 'worker.o', 'worker.obj']) {
    assert.equal(scanNativePaths(label, Buffer.from(developerRoot)).length, 1, label);
  }
});

test('clean native artifacts pass and PNG or ordinary license text is not decoded as native', () => {
  for (const [label, header] of [['worker.exe', pe()], ['ReadMD', elf()], ['Contents/MacOS/ReadMD', macho()]]) {
    assert.deepEqual(scanBuffer(label, native(header, 'src/main.rs; /rustc/compiler; offline worker'), { sourceTree: false }), []);
  }
  const png = Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), Buffer.from(developerRoot), Buffer.from(unixRoot, 'utf16le')]);
  assert.deepEqual(scanNativePaths('poster.png', png), []); assert.deepEqual(scanBuffer('poster.png', png), []);
  assert.deepEqual(scanNativePaths('LICENSE', Buffer.from(developerRoot)), []);
  assert.deepEqual(scanBuffer('assets/vendor/LICENSE', Buffer.from(developerRoot)), []);
});

function temporary(t) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'readmd-native-privacy-'));
  t.after(() => {
    assert.equal(path.dirname(path.resolve(directory)), path.resolve(os.tmpdir()));
    assert.match(path.basename(directory), /^readmd-native-privacy-/);
    fs.rmSync(directory, { recursive: true, force: true });
  });
  return directory;
}

test('external artifact CLI rejects native paths without printing their content', t => {
  const directory = temporary(t), binary = path.join(directory, 'ReadMD');
  fs.writeFileSync(binary, native(elf(), developerRoot, 'utf16le', 1));
  const result = spawnSync(process.execPath, [fileURLToPath(new URL('../privacy-scan.mjs', import.meta.url)), directory], { encoding: 'utf8' });
  assert.equal(result.status, 1); assert.match(result.stderr, /absolute path in native binary ReadMD/);
  assert.ok(!result.stderr.includes(developerRoot) && !result.stderr.includes('native-fixture'));
});

test('external CLI derives a configured build root without disclosing it', t => {
  const directory = temporary(t), binary = path.join(directory, 'ReadMD'), buildRoot = 'D:/a';
  fs.writeFileSync(binary, native(elf(), buildRoot + '/source/main.rs', 'utf16be', 1));
  const result = spawnSync(process.execPath, [fileURLToPath(new URL('../privacy-scan.mjs', import.meta.url)), directory],
    { encoding: 'utf8', env: { ...process.env, CARGO_HOME: buildRoot } });
  assert.equal(result.status, 1); assert.match(result.stderr, /absolute path in native binary ReadMD/);
  assert.ok(!result.stderr.includes(buildRoot));
});

test('SVG packager rejects an embedded path before creating or copying output', t => {
  const directory = temporary(t), binary = path.join(directory, 'worker.exe'), output = path.join(directory, 'output');
  fs.writeFileSync(binary, native(pe(), developerRoot, 'utf16be', 1));
  const result = spawnSync(process.execPath, [fileURLToPath(new URL('../build-svg-renderer.mjs', import.meta.url)),
    path.join(directory, 'source'), binary, path.join(directory, 'metadata.json'), path.join(directory, 'dependencies.txt'), output], { encoding: 'utf8' });
  assert.equal(result.status, 1); assert.match(result.stderr, /SVG worker privacy check failed/);
  assert.ok(!result.stderr.includes(developerRoot) && !result.stderr.includes('native-fixture'));
  assert.equal(fs.existsSync(output), false);
});

test('bundled Mac SVG workers pass privacy checks on hosted runners as well as locally', () => {
  for (const platform of ['macos-arm64', 'macos-x64']) {
    const bytes = fs.readFileSync(new URL('../../assets/engines/resvg/' + platform + '/resvg', import.meta.url));
    assert.deepEqual(scanNativePaths(platform, bytes), []);
    assert.deepEqual(scanNativePaths(platform, bytes, { buildRoots: ['/Users/runner', '/home/runner'] }), []);
  }
});
