// Verify the official archive before extracting the native library. Rust
// compilation and capability assembly continue to run offline.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const [destination, platform, archiveFlag, localArchive] = process.argv.slice(2);
const pins = {
  'linux-x64': ['linux-x64', 3644759, '1470e21b8b4a3b4ad7f85684e2da11d94f3b69a86d81dee11b9b6709d927ac1d', 'lib/libpdfium.so'],
  'linux-arm64': ['linux-arm64', 3588127, 'ee7f7b7d5468958336a818c1cd580bdd20972846b7377b13f9a923d92d1d4674', 'lib/libpdfium.so'],
  'macos-arm64': ['mac-arm64', 3533019, '52e94ca5aa8847934330daf3f8150c190682c5ca93831468794f8b90d4392e40', 'lib/libpdfium.dylib'],
  'macos-x64': ['mac-x64', 3588813, '6dedf83990e0e3d6b7c93c9e7589c5a126b0ae14b7464d76120cff7a26afb18b', 'lib/libpdfium.dylib'],
  'windows-x64': ['win-x64', 3733154, '73cc0de638ac2095e7445bf56a38200a5b7c7ca0e9f4ba144598f2457377ac08', 'bin/pdfium.dll'],
};
assert(destination && Object.hasOwn(pins, platform)
  && (process.argv.length === 4 || (process.argv.length === 6 && archiveFlag === '--archive' && localArchive)),
'Usage: node tools/prepare-pdfium.mjs NEW_DIR PLATFORM [--archive PINNED_ARCHIVE]');
const [releasePlatform, size, expected, library] = pins[platform];
let bytes;
if (localArchive) {
  const stat = fs.lstatSync(localArchive);
  assert(stat.isFile() && stat.size === size, 'Local PDFium archive differs from its pin');
  bytes = fs.readFileSync(localArchive);
} else {
  const url = 'https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F7881/pdfium-' + releasePlatform + '.tgz';
  const response = await fetch(url, {signal: AbortSignal.timeout(120000)});
  assert(response.ok, 'PDFium release download failed');
  const chunks = [];
  let received = 0;
  for await (const chunk of response.body) {
    received += chunk.length;
    assert(received <= size, 'PDFium archive exceeded its pinned size');
    chunks.push(chunk);
  }
  bytes = Buffer.concat(chunks);
}
assert.equal(bytes.length, size, 'PDFium archive length differs from its pin');
assert.equal(createHash('sha256').update(bytes).digest('hex'), expected, 'PDFium archive digest differs from its pin');
const folder = path.resolve(destination);
fs.mkdirSync(path.dirname(folder), {recursive: true});
fs.mkdirSync(folder);
try {
  const archive = path.join(folder, 'input.tgz');
  fs.writeFileSync(archive, bytes, {flag: 'wx'});
  execFileSync('tar', ['-xzf', archive, '-C', folder], {stdio: 'inherit'});
  fs.unlinkSync(archive);
  assert(fs.lstatSync(path.join(folder, library)).isFile(), 'Pinned PDFium library missing');
} catch (error) {
  fs.rmSync(folder, {recursive: true});
  throw error;
}
