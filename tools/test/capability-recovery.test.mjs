import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import crypto from 'node:crypto';
import path from 'node:path';
import { checkRecoveryAssets, readRecoveryCatalog, recoveryFingerprint, registerRecoveryLock } from '../lib/capability-recovery.mjs';
function owned(work) {
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'readmd-recovery-'));
  try { work(root); } finally { assert.ok(path.resolve(root).startsWith(path.resolve(os.tmpdir())+path.sep+'readmd-recovery-'));fs.rmSync(root,{recursive:true}); }
}
const lock=()=>({schema:1,id:'fonts',version:'2026.10.09',platform:'universal',review:{license:'test fixture'},files:['bin/test','data/test','NOTICE'].map(name=>({path:name,size:12,sha256:'1'.repeat(64)}))});
test('direct object fingerprints match serialized insertion order',()=>{
  const expected='{"2":"α","10":{"2":2,"12":1}}';
  assert.equal(recoveryFingerprint({'2':'α','10':{'2':2,'12':1}}),crypto.createHash('sha256').update(expected).digest('hex'));
});
test('a trusted v1 lock without modern review metadata retains its historical shape',()=>owned(root=>{
  const value=lock();value.id='pdfium';value.platform='windows-x64';delete value.review;
  const source=path.join(root,'old.json'),assets=path.join(root,'assets');fs.writeFileSync(source,JSON.stringify(value));
  const result=registerRecoveryLock(assets,source),saved=JSON.parse(fs.readFileSync(path.join(assets,'capabilities/history',result.fingerprint+'.lock.json')));
  assert.deepEqual(saved,value);assert.equal(Object.hasOwn(saved,'review'),false);assert.deepEqual(checkRecoveryAssets(assets),[]);
}));
test('explicit registration is immutable and idempotent, preserving Rust insertion order',()=>owned(root=>{
  const source=path.join(root,'source.json'), assets=path.join(root,'assets'), value=lock();fs.writeFileSync(source,JSON.stringify(value));
  const first=registerRecoveryLock(assets,source);assert.equal(first.changed,true);
  assert.deepEqual(registerRecoveryLock(assets,source),{fingerprint:first.fingerprint,changed:false});
  assert.deepEqual(checkRecoveryAssets(assets),[]);assert.equal(readRecoveryCatalog(assets).manifests.length,1);
  assert.notEqual(recoveryFingerprint(Object.fromEntries(Object.entries(value).reverse())),first.fingerprint);
}));
test('file enrollment preserves numeric-looking key order and rejects duplicate decoded keys',()=>owned(root=>{
  const source=path.join(root,'old.json'),assets=path.join(root,'assets');
  const original=JSON.stringify(lock()).replace('"review":{"license":"test fixture"}','"review":{"10":"α","2":"β","license":"test fixture"}');
  fs.writeFileSync(source,original);const result=registerRecoveryLock(assets,source);
  assert.equal(result.fingerprint,crypto.createHash('sha256').update(original).digest('hex'));
  assert.equal(fs.readFileSync(path.join(assets,'capabilities/history',result.fingerprint+'.lock.json'),'utf8'),original);
  assert.deepEqual(checkRecoveryAssets(assets),[]);
  fs.writeFileSync(source,original.replace('"schema":1','"schema":1,"\\u0073chema":1'));
  assert.throws(()=>registerRecoveryLock(assets,source),/Duplicate/);
}));
test('tampering with enrolled history fails audit and prevents further enrollment',()=>owned(root=>{
  const assets=path.join(root,'assets'),source=path.join(root,'source.json');fs.writeFileSync(source,JSON.stringify(lock()));const result=registerRecoveryLock(assets,source);
  const file=path.join(assets,'capabilities/history',result.fingerprint+'.lock.json');fs.writeFileSync(file,JSON.stringify({...lock(),version:'changed'}));
  assert.match(checkRecoveryAssets(assets).join(' '),/mismatch/);assert.throws(()=>registerRecoveryLock(assets,source),/mismatch/);
}));
test('unindexed files grant no authority and malformed/path descriptors fail',()=>owned(root=>{
  const assets=path.join(root,'assets'),caps=path.join(assets,'capabilities');fs.mkdirSync(path.join(caps,'history'),{recursive:true});fs.writeFileSync(path.join(caps,'history/arbitrary.json'),JSON.stringify(lock()));assert.deepEqual(readRecoveryCatalog(assets).manifests,[]);
  for(const entry of [{id:'fonts',platform:'universal',fingerprint:'../escape'},{id:'fonts',platform:'universal',fingerprint:'1'.repeat(64),path:'../escape'}]){
    fs.writeFileSync(path.join(caps,'recovery-locks.json'),JSON.stringify({schema:1,manifests:[entry]}));assert.ok(checkRecoveryAssets(assets).length);
  }
}));
test('duplicate and oversized catalogs are rejected before resource reads',()=>owned(root=>{
  const caps=path.join(root,'capabilities');fs.mkdirSync(caps);const entry={id:'fonts',platform:'universal',fingerprint:'1'.repeat(64)};
  for(const entries of [[entry,entry],Array(33).fill(entry)]){fs.writeFileSync(path.join(caps,'recovery-locks.json'),JSON.stringify({schema:1,manifests:entries}));assert.match(checkRecoveryAssets(root).join(' '),/catalog|duplicate|budget/);}
}));
test('bad inventories and traversal cannot enter the fixed application registry',()=>owned(root=>{
  const source=path.join(root,'source.json');for(const mutate of [v=>v.files[0].path='../private',v=>v.files[1].path=v.files[0].path,v=>v.id='untrusted',v=>v.version='../escape']){const value=lock();mutate(value);fs.writeFileSync(source,JSON.stringify(value));assert.throws(()=>registerRecoveryLock(path.join(root,'assets'),source));}assert.ok(!fs.existsSync(path.join(root,'assets')));
}));
test('number identity and Unicode cannot silently change the Rust manifest fingerprint',()=>owned(root=>{
  const source=path.join(root,'source.json');
  for(const number of ['1.0','1e3','-0','9007199254740993']) {
    fs.writeFileSync(source,JSON.stringify(lock()).replace('"size":12','"size":'+number));
    assert.throws(()=>registerRecoveryLock(path.join(root,'assets'),source),/integer/);
  }
  assert.throws(()=>recoveryFingerprint({review:'\ud800'}),/Unicode/);
}));
