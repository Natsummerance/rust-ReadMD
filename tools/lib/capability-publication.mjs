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
    const name = `readmd-${p.id}-${p.version}-${p.platform}.readmd-pack`;
    if (p.url !== `https://github.com/${repository}/releases/download/${catalog.release}/${name}`) throw Error('Capability URL differs from owned immutable release');
  }
  return catalog;
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
