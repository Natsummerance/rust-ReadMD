import test from 'node:test';
import assert from 'node:assert/strict';
import { gzipSync } from 'node:zlib';
import { repository, sha256, validateCatalog, onlineReleaseCatalog, verifyLocalPack, verifyPublishedAsset } from '../lib/capability-publication.mjs';

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
test('offline Node metadata cannot grant download URLs or duplicate an online identity', () => {
  const f=fixture(),entry={id:'node',version:'24.22.2',platform:'macos-arm64',format:'stream-v2',delivery:'offline-only',downloadBytes:100,installedBytes:200,sha256:'a'.repeat(64),manifestSha256:'b'.repeat(64)};
  f.catalog.offlinePacks=[entry];assert.equal(validateCatalog(f.catalog),f.catalog);
  for(const change of [e=>{e.url='https://example.com/runtime';},e=>{e.manifestSha256='';},e=>{e.id='fonts';},e=>{e.format='json-v1';}]){const c=structuredClone(f.catalog);change(c.offlinePacks[0]);assert.throws(()=>validateCatalog(c));}
  f.catalog.offlinePacks.push({...entry});assert.throws(()=>validateCatalog(f.catalog));
});
test('bundled PDFium matches the backend platform, size, format and no-URL contract',()=>{
  const f=fixture(),bundled={id:'pdfium',version:'7881',platform:'macos-arm64',delivery:'bundled',downloadBytes:30687950,installedBytes:23011555,sha256:'a'.repeat(64)};
  f.catalog.packs.push(bundled);assert.equal(validateCatalog(f.catalog),f.catalog);
  for(const platform of ['windows-x64','linux-x64','linux-arm64','macos-x64','macos-arm64']){
    const catalog=structuredClone(f.catalog);catalog.packs[1].platform=platform;assert.doesNotThrow(()=>validateCatalog(catalog));
  }
  for(const alter of [p=>p.id='typst',p=>p.version='7882',p=>p.platform='windows-arm64',p=>p.url='https://example.com/pdfium',
    p=>p.format='stream-v2',p=>p.sha256='bad',p=>p.downloadBytes=0,p=>p.downloadBytes=128*1048576+1,p=>p.installedBytes=0,p=>p.delivery='unknown']){
    const catalog=structuredClone(f.catalog);alter(catalog.packs[1]);assert.throws(()=>validateCatalog(catalog));
  }
  const duplicate=structuredClone(f.catalog);duplicate.packs.push({...bundled});assert.throws(()=>validateCatalog(duplicate));
});
test('release publication excludes bundled and offline entries without changing the development catalog',()=>{
  const f=fixture();f.catalog.packs.push({id:'pdfium',version:'7881',platform:'macos-arm64',delivery:'bundled',downloadBytes:1,installedBytes:1,sha256:'a'.repeat(64)});
  f.catalog.offlinePacks=[{id:'typst',version:'0.15.1',platform:'macos-arm64',format:'stream-v2',delivery:'offline-only',downloadBytes:1,installedBytes:1,sha256:'b'.repeat(64),manifestSha256:'c'.repeat(64)}];
  const original=structuredClone(f.catalog),published=onlineReleaseCatalog(f.catalog);
  assert.deepEqual(published.packs,[f.entry]);assert(!Object.hasOwn(published,'offlinePacks'));assert.deepEqual(f.catalog,original);
  assert.doesNotThrow(()=>validateCatalog(published));
});
test('full-package tampering fails before publication', () => { const f = fixture(); f.bytes[0] ^= 1; assert.throws(() => verifyLocalPack(f.bytes, f.lock, f.entry)); });
test('new package hash cannot bless a changed payload under an old file lock', () => { const f = fixture(); f.pack.payload['bin/engine.exe'] = Buffer.from('modified').toString('base64'); f.bytes = Buffer.from(JSON.stringify(f.pack)); f.entry.sha256 = sha256(f.bytes); f.entry.downloadBytes = f.bytes.length; assert.throws(() => verifyLocalPack(f.bytes, f.lock, f.entry), /Payload differs/); });
test('extra payload and missing payload cannot be published', () => { for (const extra of [true, false]) { const f = fixture(); if (extra) f.pack.payload.extra = ''; else delete f.pack.payload['bin/engine.exe']; f.bytes = Buffer.from(JSON.stringify(f.pack)); Object.assign(f.entry, { sha256: sha256(f.bytes), downloadBytes: f.bytes.length }); assert.throws(() => verifyLocalPack(f.bytes, f.lock, f.entry)); } });
test('existing release mismatches require a new immutable version', () => { const f = fixture(); verifyPublishedAsset({ size: f.entry.downloadBytes, digest: 'sha256:' + f.entry.sha256 }, f.entry); assert.throws(() => verifyPublishedAsset({ size: f.entry.downloadBytes, digest: 'sha256:' + '0'.repeat(64) }, f.entry), /will not be replaced/); });
test('locked compressed payloads are decoded and bounded before checking', () => { const f = fixture(); f.pack.payload['bin/engine.exe'] = { compression: 'gzip', data: gzipSync(Buffer.from('portable engine fixture')).toString('base64') }; f.bytes = Buffer.from(JSON.stringify(f.pack)); Object.assign(f.entry, { sha256: sha256(f.bytes), downloadBytes: f.bytes.length }); assert.ok(verifyLocalPack(f.bytes, f.lock, f.entry)); f.lock.files[0].size = 1; f.bytes = Buffer.from(JSON.stringify(f.pack)); Object.assign(f.entry, { sha256: sha256(f.bytes), downloadBytes: f.bytes.length }); assert.throws(() => verifyLocalPack(f.bytes, f.lock, f.entry)); });
