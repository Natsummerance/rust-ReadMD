'use strict';
// Independent, deterministic ZIP fixtures. No product serializer or external tool.
const zlib=require('node:zlib'),assert=require('node:assert/strict');
const table=Uint32Array.from({length:256},(_,n)=>{for(let i=0;i<8;i++)n=(n>>>1)^((n&1)?0xedb88320:0);return n>>>0;});
function crc32(bytes){let crc=0xffffffff;for(const byte of bytes)crc=(crc>>>8)^table[(crc^byte)&255];return(crc^0xffffffff)>>>0;}
function zip(entries){
 const rows=Array.from(entries);assert(rows.length<=20000);const names=new Set(),local=[],central=[];let offset=0;
 for(const[name,value]of rows){assert(!names.has(name),'Duplicate fixture member');names.add(name);const n=Buffer.from(name),data=Buffer.isBuffer(value)?value:Buffer.from(value),compressed=zlib.deflateRawSync(data),crc=crc32(data);assert(n.length<=65535&&data.length<=64*1024*1024);
  const h=Buffer.alloc(30);h.writeUInt32LE(0x04034b50);h.writeUInt16LE(20,4);h.writeUInt16LE(0x800,6);h.writeUInt16LE(8,8);h.writeUInt16LE(33,12);h.writeUInt32LE(crc,14);h.writeUInt32LE(compressed.length,18);h.writeUInt32LE(data.length,22);h.writeUInt16LE(n.length,26);local.push(h,n,compressed);
  const c=Buffer.alloc(46);c.writeUInt32LE(0x02014b50);c.writeUInt16LE(20,4);c.writeUInt16LE(20,6);c.writeUInt16LE(0x800,8);c.writeUInt16LE(8,10);c.writeUInt16LE(33,14);c.writeUInt32LE(crc,16);c.writeUInt32LE(compressed.length,20);c.writeUInt32LE(data.length,24);c.writeUInt16LE(n.length,28);c.writeUInt32LE(offset,42);central.push(c,n);offset+=h.length+n.length+compressed.length;
 }
 const directory=Buffer.concat(central),end=Buffer.alloc(22);end.writeUInt32LE(0x06054b50);end.writeUInt16LE(rows.length,8);end.writeUInt16LE(rows.length,10);end.writeUInt32LE(directory.length,12);end.writeUInt32LE(offset,16);return Buffer.concat([...local,directory,end]);
}
module.exports={zip,crc32};
