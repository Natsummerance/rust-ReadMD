import test from 'node:test';
import assert from 'node:assert/strict';
import { gzipSync } from 'node:zlib';
import { repository, sha256, validateCatalog, verifyLocalPack, verifyPublishedAsset } from '../lib/capability-publication.mjs';

function fixture() {
  const file = Buffer.from('portable engine fixture');
  const lock = { schema: 1, id: 'ocr', version: '1', platform: 'windows-x64', files: [{ path: 'bin/engine.exe', size: file.length, sha256: sha256(file) }] };
  const pack = { manifest: lock, payload: { 'bin/engine.exe': file.toString('base64') } };
  const bytes = Buffer.from(JSON.stringify(pack));
  const entry = { id: lock.id, version: lock.version, platform: lock.platform, downloadBytes: bytes.length, installedBytes: file.length, sha256: sha256(bytes), url: `https://github.com/${repository}/releases/download/packs-1/readmd-ocr-1-windows-x64.readmd-pack` };
  return { lock, pack, bytes, entry, catalog: { schema: 1, repository, release: 'packs-1', packs: [entry] } };
}
test('valid catalog and every payload are verified locally', () => { const f = fixture(); validateCatalog(f.catalog); assert.ok(verifyLocalPack(f.bytes, f.lock, f.entry)); });
test('arbitrary repository, URL and duplicate identities are refused', () => {
  for (const alter of [f => { f.catalog.repository = 'other/tool'; }, f => { f.entry.url = 'https://example.com/engine'; }, f => { f.catalog.packs.push({ ...f.entry }); }, f => { f.entry.version = '../engine'; }]) { const f = fixture(); alter(f); assert.throws(() => validateCatalog(f.catalog)); }
});
test('full-package tampering fails before publication', () => { const f = fixture(); f.bytes[0] ^= 1; assert.throws(() => verifyLocalPack(f.bytes, f.lock, f.entry)); });
test('new package hash cannot bless a changed payload under an old file lock', () => { const f = fixture(); f.pack.payload['bin/engine.exe'] = Buffer.from('modified').toString('base64'); f.bytes = Buffer.from(JSON.stringify(f.pack)); f.entry.sha256 = sha256(f.bytes); f.entry.downloadBytes = f.bytes.length; assert.throws(() => verifyLocalPack(f.bytes, f.lock, f.entry), /Payload differs/); });
test('extra payload and missing payload cannot be published', () => { for (const extra of [true, false]) { const f = fixture(); if (extra) f.pack.payload.extra = ''; else delete f.pack.payload['bin/engine.exe']; f.bytes = Buffer.from(JSON.stringify(f.pack)); Object.assign(f.entry, { sha256: sha256(f.bytes), downloadBytes: f.bytes.length }); assert.throws(() => verifyLocalPack(f.bytes, f.lock, f.entry)); } });
test('existing release mismatches require a new immutable version', () => { const f = fixture(); verifyPublishedAsset({ size: f.entry.downloadBytes, digest: 'sha256:' + f.entry.sha256 }, f.entry); assert.throws(() => verifyPublishedAsset({ size: f.entry.downloadBytes, digest: 'sha256:' + '0'.repeat(64) }, f.entry), /will not be replaced/); });
test('locked compressed payloads are decoded and bounded before checking', () => { const f = fixture(); f.pack.payload['bin/engine.exe'] = { compression: 'gzip', data: gzipSync(Buffer.from('portable engine fixture')).toString('base64') }; f.bytes = Buffer.from(JSON.stringify(f.pack)); Object.assign(f.entry, { sha256: sha256(f.bytes), downloadBytes: f.bytes.length }); assert.ok(verifyLocalPack(f.bytes, f.lock, f.entry)); f.lock.files[0].size = 1; f.bytes = Buffer.from(JSON.stringify(f.pack)); Object.assign(f.entry, { sha256: sha256(f.bytes), downloadBytes: f.bytes.length }); assert.throws(() => verifyLocalPack(f.bytes, f.lock, f.entry)); });
