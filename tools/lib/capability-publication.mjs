import crypto from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';
import { gunzipSync } from 'node:zlib';

export const repository = 'Natsummerance/tool-ReadMD';
export const sha256 = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const safe = value => typeof value === 'string' && /^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(value);
const digest = value => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);

export function validateCatalog(catalog) {
  if (catalog?.schema !== 1 || catalog.repository !== repository || !safe(catalog.release) || !Array.isArray(catalog.packs) || !catalog.packs.length) throw Error('Invalid owned capability catalog');
  const identities = new Set();
  for (const p of catalog.packs) {
    if (!['pdfium', 'typst', 'ocr', 'fonts'].includes(p.id) || !safe(p.version) || !safe(p.platform) || !digest(p.sha256) || !Number.isSafeInteger(p.downloadBytes) || p.downloadBytes <= 0 || p.downloadBytes > 128 * 1048576 || !Number.isSafeInteger(p.installedBytes) || p.installedBytes <= 0) throw Error('Invalid capability identity or size');
    const identity = p.id + ':' + p.platform;
    if (identities.has(identity)) throw Error('Duplicate capability/platform');
    identities.add(identity);
    if (p.delivery === 'bundled') {
      if(p.id!=='pdfium'||p.version!=='7881'||!['windows-x64','linux-x64','linux-arm64','macos-x64','macos-arm64'].includes(p.platform)
          ||(p.format!==undefined&&p.format!=='json-v1')||p.url!==undefined)throw Error('Invalid bundled PDFium identity or source');
      continue;
    }
    if(p.delivery!==undefined)throw Error('Unsupported online capability delivery');
    const name = `readmd-${p.id}-${p.version}-${p.platform}.readmd-pack`;
    if (p.url !== `https://github.com/${repository}/releases/download/${catalog.release}/${name}`) throw Error('Capability URL differs from owned immutable release');
  }
  if (catalog.offlinePacks !== undefined) {
    if (!Array.isArray(catalog.offlinePacks)) throw Error('Invalid offline capability catalog');
    for (const p of catalog.offlinePacks) {
      const identity=p.id+':'+p.platform;
      if(!['node','whisper','media-ffmpeg','typst'].includes(p.id)||p.delivery!=='offline-only'||p.format!=='stream-v2'||p.url!==undefined||!safe(p.version)||!safe(p.platform)||!digest(p.sha256)||!digest(p.manifestSha256)||!Number.isSafeInteger(p.downloadBytes)||p.downloadBytes<=0||p.downloadBytes>2*1073741824||!Number.isSafeInteger(p.installedBytes)||p.installedBytes<=0||identities.has(identity))throw Error('Invalid offline capability identity or size');
      identities.add(identity);
    }
  }
  return catalog;
}

// A development candidate may carry a bundled PDFium and reviewed offline
// additions. Neither belongs in an online release's catalog or upload list.
export function onlineReleaseCatalog(catalog) {
  validateCatalog(catalog);
  const published={...catalog,packs:catalog.packs.filter(entry=>entry.delivery!=='bundled')};
  delete published.offlinePacks;
  if(!published.packs.length)throw Error('No online release packs');
  return published;
}

export function verifyLocalPack(bytes, lock, entry) {
  if (bytes.length !== entry.downloadBytes || sha256(bytes) !== entry.sha256) throw Error('Local package differs from catalog');
  let pack;
  try { pack = JSON.parse(bytes); } catch { throw Error('Invalid local package'); }
  if (!isDeepStrictEqual(pack.manifest, lock) || lock.id !== entry.id || lock.version !== entry.version || lock.platform !== entry.platform || !Array.isArray(lock.files) || !pack.payload || Array.isArray(pack.payload)) throw Error('Package manifest differs from application lock');
  const names = new Set();
  let installed = 0;
  for (const file of lock.files) {
    if (typeof file.path !== 'string' || !/^[A-Za-z0-9._/-]+$/.test(file.path) || file.path.split('/').some(s => !s || s === '.' || s === '..') || names.has(file.path) || !Number.isSafeInteger(file.size) || file.size < 0 || !digest(file.sha256)) throw Error('Invalid package file lock');
    names.add(file.path);
    const payload = pack.payload[file.path];
    let data;
    if (typeof payload === 'string') data = Buffer.from(payload, 'base64');
    else if (payload?.compression === 'gzip' && typeof payload.data === 'string') data = gunzipSync(Buffer.from(payload.data, 'base64'), { maxOutputLength: Math.max(1, file.size + 1) });
    else throw Error('Missing or unsupported locked payload');
    if (data.length !== file.size || sha256(data) !== file.sha256) throw Error('Payload differs from file lock');
    installed += file.size;
  }
  if (Object.keys(pack.payload).length !== names.size || installed !== entry.installedBytes) throw Error('Unexpected payload or installed size');
  return true;
}

export function verifyPublishedAsset(asset, entry) {
  if (asset.size !== entry.downloadBytes || asset.digest !== 'sha256:' + entry.sha256) throw Error('Published asset differs; immutable package will not be replaced');
}
