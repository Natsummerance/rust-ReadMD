// Offline review of one digest-locked upstream input, not a runtime installer.
// Re-running audits the prepared tree instead of overwriting it.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { inflateRawSync, crc32 } from 'node:zlib';
import { fileURLToPath } from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const base = path.join(root, '.cache/capability-upgrade/tex-inputs');
const pin = JSON.parse(fs.readFileSync(path.join(root, 'tools/tex-runtime-inputs.lock.json'))).archive;
const archive = fs.readFileSync(path.join(base, pin.name));
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
if (archive.length !== pin.size || sha(archive) !== pin.sha256) throw Error('Unreviewed TeX input');
const target = path.join(base, 'TinyTeX');
const audit = fs.existsSync(target);
const staging = audit ? target : fs.mkdtempSync(path.join(base, 'unpack-'));
if (fs.lstatSync(staging).isSymbolicLink()) throw Error('Real preparation directory required');
let eocd = -1;
for (let i = archive.length - 22; i >= Math.max(0, archive.length - 65557); i--) {
  if (archive.readUInt32LE(i) === 0x06054b50 && i + 22 + archive.readUInt16LE(i + 20) === archive.length) { eocd = i; break; }
}
if (eocd < 0 || archive.readUInt16LE(eocd + 4) || archive.readUInt16LE(eocd + 6)) throw Error('Unsupported ZIP directory');
const count = archive.readUInt16LE(eocd + 10);
const directoryBytes = archive.readUInt32LE(eocd + 12);
const directory = archive.readUInt32LE(eocd + 16);
let at = directory, total = 0;
const names = new Set(), files = [];
if (count > 30000 || count !== archive.readUInt16LE(eocd + 8) || at + directoryBytes !== eocd) throw Error('Unsupported archive budget');
for (let i = 0; i < count; i++) {
  if (at + 46 > eocd || archive.readUInt32LE(at) !== 0x02014b50) throw Error('Invalid ZIP directory');
  const flags = archive.readUInt16LE(at + 8), method = archive.readUInt16LE(at + 10), crc = archive.readUInt32LE(at + 16);
  const compressed = archive.readUInt32LE(at + 20), size = archive.readUInt32LE(at + 24);
  const nameLength = archive.readUInt16LE(at + 28), extra = archive.readUInt16LE(at + 30), comment = archive.readUInt16LE(at + 32);
  const external = archive.readUInt32LE(at + 38), offset = archive.readUInt32LE(at + 42);
  const name = archive.subarray(at + 46, at + 46 + nameLength).toString('utf8');
  at += 46 + nameLength + extra + comment;
  if (at > eocd || flags & 1 || ![0, 8].includes(method) || ((external >>> 16) & 0xf000) === 0xa000 || size > 128 * 1048576 || name.includes('\\') || !name.startsWith('TinyTeX/') || name.split('/').some(p => p === '.' || p === '..') || /[:\x00-\x1f\x7f]/.test(name) || names.has(name.toLowerCase())) throw Error('Unsafe or unsupported archive entry');
  names.add(name.toLowerCase()); total += size;
  if (total > 1536 * 1048576) throw Error('Expanded archive budget');
  const relative = name.slice('TinyTeX/'.length);
  if (!relative || name.endsWith('/')) continue;
  const destination = path.resolve(staging, relative);
  if (!destination.startsWith(staging + path.sep) || offset + 30 > directory || archive.readUInt32LE(offset) !== 0x04034b50) throw Error('Invalid local entry');
  const localLength = archive.readUInt16LE(offset + 26);
  const start = offset + 30 + localLength + archive.readUInt16LE(offset + 28);
  if (archive.subarray(offset + 30, offset + 30 + localLength).toString('utf8') !== name || start + compressed > directory || archive.readUInt16LE(offset + 8) !== method) throw Error('ZIP entries disagree');
  const input = archive.subarray(start, start + compressed);
  const bytes = method === 8 ? inflateRawSync(input, { maxOutputLength: Math.max(1, size + 1) }) : input;
  if (bytes.length !== size || crc32(bytes) !== crc) throw Error('Entry integrity failed');
  const digest = sha(bytes);
  if (audit) {
    const real = fs.realpathSync(destination);
    if (!real.startsWith(fs.realpathSync(staging) + path.sep) || fs.statSync(destination).size !== size || sha(fs.readFileSync(destination)) !== digest) throw Error('Prepared runtime differs from locked input');
  } else {
    fs.mkdirSync(path.dirname(destination), { recursive: true });
    fs.writeFileSync(destination, bytes, { flag: 'wx' });
  }
  files.push({ path: relative, size, sha256: digest });
}
if (at !== eocd || files.length < 100) throw Error('Incomplete distribution');
if (!audit) fs.renameSync(staging, target);
fs.writeFileSync(path.join(base, 'inventory.json'), JSON.stringify({ archive: pin, files, totalBytes: total }, null, 2) + '\n');
console.log(JSON.stringify({ prepared: true, auditedExistingTree: audit, offline: true, files: files.length, bytes: total, published: false }));
