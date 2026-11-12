// Fetch the published, pinned archive separately from the offline Rust build.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const [destination, platform] = process.argv.slice(2);
const pins = {
  'linux-x64': [3644759, '1470e21b8b4a3b4ad7f85684e2da11d94f3b69a86d81dee11b9b6709d927ac1d'],
  'linux-arm64': [3588127, 'ee7f7b7d5468958336a818c1cd580bdd20972846b7377b13f9a923d92d1d4674'],
};
assert(destination && Object.hasOwn(pins, platform), 'Expected a new destination and a Linux platform');
const [size, expected] = pins[platform];
const url = 'https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F7881/pdfium-' + platform + '.tgz';
const response = await fetch(url, {signal: AbortSignal.timeout(90000)});
assert(response.ok, 'PDFium release download failed');
const chunks = [];
let received = 0;
for await (const chunk of response.body) {
  received += chunk.length;
  assert(received <= size, 'PDFium archive exceeded its pinned size');
  chunks.push(chunk);
}
assert.equal(received, size);
const bytes = Buffer.concat(chunks);
assert.equal(createHash('sha256').update(bytes).digest('hex'), expected);
const folder = path.resolve(destination);
fs.mkdirSync(path.dirname(folder), {recursive:true});
fs.mkdirSync(folder);
try {
  const archive = path.join(folder, 'input.tgz');
  fs.writeFileSync(archive, bytes, {flag:'wx'});
  execFileSync('tar', ['-xzf', archive, '-C', folder], {stdio:'inherit'});
  fs.unlinkSync(archive);
  assert(fs.lstatSync(path.join(folder, 'lib/libpdfium.so')).isFile());
} catch (error) {
  fs.rmSync(folder, {recursive:true});
  throw error;
}
