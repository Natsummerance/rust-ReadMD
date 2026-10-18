// Offline maintainer authority only. Runtime/user-directory manifests are
// never auto-enrolled. The kernel enables serde_json preserve_order: legacy
// fingerprints include object insertion order. Never sort/rewrite old locks.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { validateManifest } from '../build-stream-capability-pack.mjs';
const digest = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const ids = new Set(['fonts', 'pdfium', 'typst', 'ocr', 'latex']);
const platforms = new Set(['universal', 'windows-x64', 'windows-arm64', 'macos-x64', 'macos-arm64', 'linux-x64', 'linux-arm64']);
const snapshots = new WeakMap();
function canonical(value, depth=0) {
  if (depth>128) throw Error('Recovery nesting limit');
  if (typeof value === 'number' && (!Number.isSafeInteger(value) || Object.is(value, -0))) throw Error('Recovery lock numbers must be exact integers');
  if (typeof value === 'string' && Buffer.from(value).toString('utf8') !== value) throw Error('Invalid recovery Unicode');
  if (Array.isArray(value)) value.forEach(v=>canonical(v,depth+1));
  else if (value && typeof value === 'object') Object.keys(value).forEach(key => { canonical(key,depth+1); canonical(value[key],depth+1); });
  return value;
}
function compact(source) {
  let i=0;
  const space=()=>{while(/\s/.test(source[i]||'')&&i<source.length)i++;};
  function string(){const start=i++;while(i<source.length){if(source[i]==='\\')i+=2;else if(source[i++]==='"')return JSON.parse(source.slice(start,i));}throw Error('Incomplete recovery string');}
  function read(depth=0){
    if(depth>128)throw Error('Recovery nesting limit');space();
    if(source[i]==='"')return JSON.stringify(string());
    if(source[i]==='{'){
      i++;space();const seen=new Set(),fields=[];
      if(source[i]!=='}')while(true){space();const key=string();if(seen.has(key))throw Error('Duplicate recovery key');seen.add(key);space();if(source[i++]!==':')throw Error('Invalid recovery object');fields.push(JSON.stringify(key)+':'+read(depth+1));space();if(source[i]!==',')break;i++;}
      if(source[i++]!=='}')throw Error('Incomplete recovery object');return '{'+fields.join(',')+'}';
    }
    if(source[i]==='['){i++;space();const values=[];if(source[i]!==']')while(true){values.push(read(depth+1));space();if(source[i]!==',')break;i++;}if(source[i++]!==']')throw Error('Incomplete recovery array');return '['+values.join(',')+']';}
    const match=source.slice(i).match(/^(?:true|false|null|-?(?:0|[1-9]\d*))/);if(!match)throw Error('Invalid recovery token');i+=match[0].length;return match[0];
  }
  const result=read();space();if(i!==source.length)throw Error('Trailing recovery content');return result;
}
// File snapshots preserve numeric-looking key order too, which JSON.parse /
// JSON.stringify alone cannot retain. Direct objects follow JS serialization.
export const recoveryFingerprint = value => digest(Buffer.from(snapshots.get(value)?.compact ?? JSON.stringify(canonical(value))));
function regular(file, limit) {
  const stat = fs.lstatSync(file);
  if (stat.isSymbolicLink() || !stat.isFile() || stat.size === 0 || stat.size > limit) throw Error('Invalid recovery asset');
  const source=fs.readFileSync(file,'utf8');
  // Capability-lock numeric fields are integers. Do not silently change a
  // Rust float token (even 1.0 or -0) into a JS integer and a different hash.
  for (const token of source.match(/"(?:\\[\s\S]|[^"\\])*"|-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?/g) || []) {
    if (!token.startsWith('"') && (!/^-?(?:0|[1-9]\d*)$/.test(token) || !Number.isSafeInteger(Number(token)) || token==='-0')) throw Error('Recovery lock numbers must use canonical integer tokens');
  }
  const value=JSON.parse(source), encoded=compact(source);canonical(value);
  if(value&&typeof value==='object')snapshots.set(value,{source,compact:encoded});
  return value;
}
function directory(dir) { const stat = fs.lstatSync(dir); if (!stat.isDirectory() || stat.isSymbolicLink()) throw Error('Redirected recovery directory'); }
function manifest(value) {
  if (!ids.has(value.id) || !platforms.has(value.platform) || typeof value.version !== 'string' || value.version.length > 64) throw Error('Invalid recovery identity');
  if (value.schema === 2) validateManifest(value);
  else if (value.schema === 1) {
    if (!Array.isArray(value.files) || value.files.length < 3 || value.files.length > 64 || value.files.some(f => f.size > 128 * 1048576)) throw Error('Invalid legacy recovery inventory');
    const projected = structuredClone(value); projected.schema = 2;
    // The original PDFium v1 lock predates review metadata. Validation here
    // grants recovery ownership only; preserve its exact historical shape.
    if (!Object.hasOwn(projected,'review')) projected.review={recoveryOnly:true};
    projected.files.forEach(f => { f.compressedSize = 20; });
    validateManifest(projected);
  } else throw Error('Invalid recovery schema');
}
function entries(catalog) {
  if (catalog.schema !== 1 || Object.keys(catalog).some(k => !['schema', 'manifests'].includes(k)) || !Array.isArray(catalog.manifests) || catalog.manifests.length > 32) throw Error('Invalid recovery catalog');
  const seen = new Set(), counts = new Map();
  for (const e of catalog.manifests) {
    if (Object.keys(e).some(k => !['id', 'platform', 'fingerprint'].includes(k)) || !ids.has(e.id) || !platforms.has(e.platform) || !/^[a-f0-9]{64}$/.test(e.fingerprint)) throw Error('Invalid recovery descriptor');
    const key = `${e.id}/${e.platform}`, identity = `${key}/${e.fingerprint}`;
    if (seen.has(identity) || (counts.get(key) || 0) >= 8) throw Error('Recovery duplicate or budget');
    seen.add(identity); counts.set(key, (counts.get(key) || 0) + 1);
  }
}
export function readRecoveryCatalog(assets) {
  const caps = path.join(assets, 'capabilities'), file = path.join(caps, 'recovery-locks.json');
  if (!fs.existsSync(file)) return { schema: 1, manifests: [] };
  directory(caps); const catalog = regular(file, 65536); entries(catalog);
  if (catalog.manifests.length) directory(path.join(caps, 'history'));
  for (const e of catalog.manifests) {
    const value = regular(path.join(caps, 'history', `${e.fingerprint}.lock.json`), 8 * 1048576);
    manifest(value);
    if (value.id !== e.id || value.platform !== e.platform || recoveryFingerprint(value) !== e.fingerprint) throw Error('Recovery lock identity or fingerprint mismatch');
  }
  return catalog;
}
export function checkRecoveryAssets(assets) { try { readRecoveryCatalog(assets); return []; } catch (error) { return [`capability recovery: ${error.message}`]; } }
export function registerRecoveryLock(assets, trustedSource) {
  const value = regular(trustedSource, 8 * 1048576); manifest(value);
  const catalog = readRecoveryCatalog(assets), fingerprint = recoveryFingerprint(value);
  const entry = { id: value.id, platform: value.platform, fingerprint };
  if (catalog.manifests.some(e => e.id === entry.id && e.platform === entry.platform && e.fingerprint === fingerprint)) return { fingerprint, changed: false };
  catalog.manifests.push(entry); entries(catalog);
  const caps = path.join(assets, 'capabilities'); fs.mkdirSync(caps, { recursive: true }); directory(caps);
  const history = path.join(caps, 'history'); fs.mkdirSync(history, { recursive: true }); directory(history);
  const target = path.join(history, `${fingerprint}.lock.json`);
  if (fs.existsSync(target)) { if (recoveryFingerprint(regular(target, 8 * 1048576)) !== fingerprint) throw Error('Existing immutable recovery file differs'); }
  else { const fd = fs.openSync(target, 'wx'); try { fs.writeFileSync(fd, snapshots.get(value).source); fs.fsyncSync(fd); } finally { fs.closeSync(fd); } }
  const temporary = path.join(caps, `recovery-write-${crypto.randomUUID()}.json`);
  const fd = fs.openSync(temporary, 'wx'); try { fs.writeFileSync(fd, JSON.stringify(catalog, null, 2) + '\n'); fs.fsyncSync(fd); } finally { fs.closeSync(fd); }
  fs.renameSync(temporary, path.join(caps, 'recovery-locks.json'));
  return { fingerprint, changed: true };
}
