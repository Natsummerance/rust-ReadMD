import test from 'node:test';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {verifyContainer, gitBlobSha1, recheck} from './tex-corresponding-materials-audit.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const sha512 = bytes => crypto.createHash('sha512').update(bytes).digest('hex');
const original = Buffer.concat([Buffer.from('fd377a585a00', 'hex'), Buffer.from('fixed archive bytes')]);
const expected = {size:original.length, sha512:sha512(original)};

test('equal length cannot substitute for the locked SHA-512', () => {
  const changed = Buffer.from(original); changed[changed.length - 1] ^= 1;
  const result = verifyContainer(changed, expected);
  assert.equal(result.lengthMatches, true); assert.equal(result.xzHeader, true);
  assert.equal(result.digestMatches, false); assert.equal(result.exact, false);
});

test('a truncated archive and an HTTP 200 HTML body remain unusable', () => {
  assert.equal(verifyContainer(original.subarray(0, original.length - 1), expected).exact, false);
  const html = Buffer.from('<html>Access Denied</html>');
  assert.equal(verifyContainer(html, {size:html.length, sha512:sha512(html)}).exact, false);
});

test('source files use Git blob object identity, including its length prefix', () => {
  assert.equal(gitBlobSha1(Buffer.alloc(0)), 'e69de29bb2d1d6434b8b29ae775ad8c2e48c5391');
  assert.notEqual(gitBlobSha1(original), crypto.createHash('sha1').update(original).digest('hex'));
});

test('offline recheck rejects an exact-container claim forged in an audit JSON', () => {
  const lock = JSON.parse(fs.readFileSync(path.join(root, 'tools/tex-publication-inputs.lock.json')));
  const target = lock.components.find(c => c.name === 'pdfmanagement').containers.source;
  const dir = fs.mkdtempSync(path.join(root, '.cache/capability-upgrade/tex-materials-audit-negative-'));
  const file = path.join(dir, target.name), bytes = Buffer.alloc(target.size);
  original.subarray(0, 6).copy(bytes); fs.writeFileSync(file, bytes);
  const receipt = path.join(dir, 'forged-report.json');
  fs.writeFileSync(receipt, JSON.stringify({schema:1, d04Complete:false, redistributionApproved:false,
    frozenInputs:[], localInputs:[], attempts:[], sourceCandidates:[], materials:[{name:target.name, exactFile:path.relative(root, file)}]}));
  assert.throws(() => recheck(path.relative(root, receipt)), /pdfmanagement\.source\.tar\.xz/);
  // Keep this small negative fixture as inspectable evidence; never alter the
  // original containers or remove shared caches during verification.
});
