'use strict';
// Independent inspection of owned DOCX/EPUB fixtures, without product parsers.
const assert=require('node:assert/strict'),zlib=require('node:zlib'),{crc32}=require('./zip-fixtures.cjs');
module.exports=function zipParts(bytes,encoding='utf8'){
  assert.ok(bytes.length>=22&&bytes.length<=64*1024*1024,'bounded test archive');
  let end=bytes.length-22;const first=Math.max(0,bytes.length-65557);
  while(end>=first&&(bytes.readUInt32LE(end)!==0x06054b50||end+22+bytes.readUInt16LE(end+20)!==bytes.length))end--;
  assert.ok(end>=first,'ZIP end directory');assert.equal(bytes.readUInt16LE(end+4),0,'single disk');assert.equal(bytes.readUInt16LE(end+6),0);
  let at=bytes.readUInt32LE(end+16);const count=bytes.readUInt16LE(end+10),directoryEnd=at+bytes.readUInt32LE(end+12),parts=new Map();assert.equal(bytes.readUInt16LE(end+8),count);assert.ok(count<=20000);assert.equal(directoryEnd,end,'directory size');
  for(let i=0;i<count;i++){
    assert.ok(at+46<=directoryEnd,'central header bounds');assert.equal(bytes.readUInt32LE(at),0x02014b50);
    const flags=bytes.readUInt16LE(at+8),method=bytes.readUInt16LE(at+10),crc=bytes.readUInt32LE(at+16),size=bytes.readUInt32LE(at+20),length=bytes.readUInt32LE(at+24),n=bytes.readUInt16LE(at+28),extra=bytes.readUInt16LE(at+30),comment=bytes.readUInt16LE(at+32),local=bytes.readUInt32LE(at+42);
    assert.ok(at+46+n+extra+comment<=directoryEnd);assert.equal(bytes.readUInt16LE(at+34),0);assert.equal(flags&1,0,'no encrypted test member');assert.ok(method===0||method===8);assert.ok(length<=64*1024*1024);
    const nameBytes=bytes.subarray(at+46,at+46+n),name=nameBytes.toString();assert.ok(!parts.has(name),'unique archive member');assert.ok(local+30<=end);assert.equal(bytes.readUInt32LE(local),0x04034b50);assert.equal(bytes.readUInt16LE(local+6),flags);assert.equal(bytes.readUInt16LE(local+8),method);
    const localName=bytes.readUInt16LE(local+26),start=local+30+localName+bytes.readUInt16LE(local+28);assert.ok(start+size<=bytes.readUInt32LE(end+16),'member before central directory');assert.deepEqual(bytes.subarray(local+30,local+30+localName),nameBytes,'same local member name');
    if(!(flags&8)){assert.equal(bytes.readUInt32LE(local+14),crc);assert.equal(bytes.readUInt32LE(local+18),size);assert.equal(bytes.readUInt32LE(local+22),length);}
    const content=bytes.subarray(start,start+size),decoded=method===8?zlib.inflateRawSync(content,{maxOutputLength:64*1024*1024}):content;assert.equal(decoded.length,length,'uncompressed member size');assert.equal(crc32(decoded),crc,'member CRC32');parts.set(name,encoding===null?decoded:decoded.toString(encoding));at+=46+n+extra+comment;
  }assert.equal(at,directoryEnd,'exact directory consumption');return parts;
};
