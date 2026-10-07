import { deflateRawSync } from 'node:zlib';
const table = Array.from({length:256}, (_,i) => { for(let j=0;j<8;j++) i=(i>>>1)^((i&1)?0xedb88320:0); return i>>>0; });
const crc32 = bytes => { let crc=0xffffffff; for(const byte of bytes) crc=table[(crc^byte)&255]^(crc>>>8); return (crc^0xffffffff)>>>0; };
/** Deterministic ZIP32 with UTF-8 names, deflate and a fixed 1980 timestamp. */
export function zip(entries) {
  if (entries.length > 65535) throw Error('Too many ZIP entries');
  const names = new Set(), records = [], directory = [];
  let offset = 0;
  for (const entry of [...entries].sort((a,b) => a.name.localeCompare(b.name,'en'))) {
    if (!entry.name || entry.name.startsWith('/') || /[\\:\0]/.test(entry.name) || entry.name.split('/').some(part => !part || part === '..' || part === '.') || names.has(entry.name)) throw Error('Invalid or duplicate ZIP path');
    names.add(entry.name);
    const name = Buffer.from(entry.name), data = Buffer.from(entry.data), packed = deflateRawSync(data,{level:9}), crc = crc32(data);
    if (name.length > 65535 || data.length > 0xffffffff || packed.length > 0xffffffff || offset > 0xffffffff) throw Error('ZIP32 limit exceeded');
    const local = Buffer.alloc(30); local.writeUInt32LE(0x04034b50); local.writeUInt16LE(20,4); local.writeUInt16LE(0x800,6); local.writeUInt16LE(8,8); local.writeUInt16LE(33,12);
    local.writeUInt32LE(crc,14); local.writeUInt32LE(packed.length,18); local.writeUInt32LE(data.length,22); local.writeUInt16LE(name.length,26);
    records.push(local,name,packed);
    const central = Buffer.alloc(46); central.writeUInt32LE(0x02014b50); central.writeUInt16LE((3<<8)|20,4); central.writeUInt16LE(20,6); central.writeUInt16LE(0x800,8); central.writeUInt16LE(8,10); central.writeUInt16LE(33,14);
    central.writeUInt32LE(((0o100000 | (entry.mode ?? 0o644)) << 16) >>> 0, 38);
    central.writeUInt32LE(crc,16); central.writeUInt32LE(packed.length,20); central.writeUInt32LE(data.length,24); central.writeUInt16LE(name.length,28); central.writeUInt32LE(offset,42);
    directory.push(central,name); offset += local.length + name.length + packed.length;
  }
  const central = Buffer.concat(directory), end = Buffer.alloc(22); end.writeUInt32LE(0x06054b50); end.writeUInt16LE(entries.length,8); end.writeUInt16LE(entries.length,10); end.writeUInt32LE(central.length,12); end.writeUInt32LE(offset,16);
  return Buffer.concat([...records,central,end]);
}
