import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { inflateRawSync } from 'node:zlib';

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const sentinel = 'OWNED_NATIVE_PREPARATION_PRIVATE_SENTINEL';

// Inspect actual emitted ZIP members, rather than matching packager source.
function members(bytes) {
  const end = bytes.length - 22;
  assert.equal(bytes.readUInt32LE(end), 0x06054b50);
  const count = bytes.readUInt16LE(end + 10), result = new Map();
  let at = bytes.readUInt32LE(end + 16);
  for (let i = 0; i < count; i++) {
    assert.equal(bytes.readUInt32LE(at), 0x02014b50);
    const method = bytes.readUInt16LE(at + 10), size = bytes.readUInt32LE(at + 20);
    const nameLength = bytes.readUInt16LE(at + 28), extra = bytes.readUInt16LE(at + 30), comment = bytes.readUInt16LE(at + 32);
    const name = bytes.subarray(at + 46, at + 46 + nameLength).toString('utf8');
    const local = bytes.readUInt32LE(at + 42);
    assert.equal(bytes.readUInt32LE(local), 0x04034b50);
    const dataAt = local + 30 + bytes.readUInt16LE(local + 26) + bytes.readUInt16LE(local + 28);
    assert.equal(method, 8); assert(!result.has(name));
    result.set(name, inflateRawSync(bytes.subarray(dataAt, dataAt + size)));
    at += 46 + nameLength + extra + comment;
  }
  assert.equal(at, end);
  return result;
}

test('actual offline VSIX preserves compiled code but excludes local preparation receipts and source maps', () => {
  const area = fs.mkdtempSync(path.join(os.tmpdir(), 'readmd-vsix-privacy-')), identity = fs.lstatSync(area);
  const extension = path.join(area, 'packages/vscode-extension');
  const write = (file, text) => {
    const target = path.join(extension, file);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, text, { flag: 'wx' });
    return target;
  };
  try {
    const packager = write('scripts/package-vsix.mjs', fs.readFileSync(path.join(repo, 'packages/vscode-extension/scripts/package-vsix.mjs')));
    fs.mkdirSync(path.join(area, 'tools/lib'), { recursive: true });
    fs.copyFileSync(path.join(repo, 'tools/lib/zip.mjs'), path.join(area, 'tools/lib/zip.mjs'), fs.constants.COPYFILE_EXCL);
    write('package.json', JSON.stringify({ name: 'readmd-fixture', version: '0.0.6', publisher: 'owned-fixture',
      displayName: '%name%', description: '%description%', keywords: ['markdown'], categories: ['Other'],
      engines: { vscode: '^1.80.0' }, repository: { url: 'https://github.com/Natsummerance/rust-ReadMD' } }));
    write('package.nls.json', JSON.stringify({ name: 'ReadMD fixture', description: 'Synthetic offline package test' }));
    write('package.nls.zh-cn.json', '{}');
    for (const name of ['README.md', 'LICENSE', 'icon.png']) write(name, 'owned fixture');
    const runtime = Buffer.from('exports.activate = () => "compiled-public-runtime";\n');
    write('out/extension.js', runtime);
    write('out/webview.js', 'exports.publicWebview = true;\n');
    write('out/openapi-review-projection.js', 'exports.publicProjection = true;\n');
    const receiptText = JSON.stringify({ schema: 1, workspaceRoot: sentinel, dependencies: sentinel });
    const receipt = write('out/.readmd-openapi-build.json', receiptText);
    write('out/extension.js.map', JSON.stringify({ sources: [sentinel] }));
    for (const name of ['media/preview.js', 'media/diagram-frame.js', 'media/vendor/marked.min.js',
      'media/vendor/mermaid.min.js', 'media/vendor/reveal/theme/fonts/source-sans-pro/source-sans-pro.css']) write(name, '/* public fixture */');
    write('l10n/bundle.l10n.json', '{}'); write('snippets/markdown.json', '{}');
    const output = path.join(area, 'test.vsix');
    const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !/TOKEN|SECRET|CREDENTIAL|API_KEY/i.test(key)));
    execFileSync(process.execPath, [packager, output], { cwd: area, env, stdio: ['ignore', 'pipe', 'pipe'], timeout: 20000 });
    const actual = members(fs.readFileSync(output));
    assert.deepEqual(actual.get('extension/out/extension.js'), runtime);
    for (const name of ['extension/out/webview.js', 'extension/out/openapi-review-projection.js',
      'extension/l10n/bundle.l10n.json', 'extension/snippets/markdown.json', 'extension/media/preview.js']) assert(actual.has(name), name);
    assert.equal(actual.has('extension/out/.readmd-openapi-build.json'), false, 'native preparation receipt must stay local');
    assert.equal([...actual.keys()].some(name => name.endsWith('.map')), false);
    for (const data of actual.values()) assert.equal(data.includes(sentinel), false, 'preparation data cannot enter another member');
    assert.equal(fs.readFileSync(receipt, 'utf8'), receiptText, 'packaging must preserve local preparation evidence');
  } finally {
    const now = fs.lstatSync(area);
    assert(now.isDirectory() && !now.isSymbolicLink() && now.dev === identity.dev && now.ino === identity.ino);
    fs.rmSync(area, { recursive: true });
  }
});
