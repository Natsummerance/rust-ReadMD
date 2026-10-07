import test from 'node:test';
import assert from 'node:assert/strict';
import { inflateRawSync } from 'node:zlib';
import { zip } from '../lib/zip.mjs';
test('ZIP payload, standard CRC, UTF-8 paths and repeatability',()=>{
  const entries=[{name:'资料/测试.md',data:'123456789'}];
  const bytes=zip(entries);
  assert.equal(bytes.readUInt32LE(0),0x04034b50);
  assert.equal(bytes.readUInt32LE(14),0xcbf43926);
  const length=bytes.readUInt16LE(26), packed=bytes.readUInt32LE(18);
  assert.equal(bytes.subarray(30,30+length).toString(),'资料/测试.md');
  assert.equal(inflateRawSync(bytes.subarray(30+length,30+length+packed)).toString(),'123456789');
  assert.deepEqual(zip(entries),bytes);
  assert.equal(bytes.readUInt32LE(bytes.length-22),0x06054b50);
});
test('archives reject traversal, absolute paths and duplicate members',()=>{
  for(const name of ['../private','a/../private','/absolute','C:/private','a\\private','a//b']) assert.throws(()=>zip([{name,data:''}]));
  assert.throws(()=>zip([{name:'a',data:'1'},{name:'a',data:'2'}]));
});
