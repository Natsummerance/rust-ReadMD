import test from 'node:test';
import assert from 'node:assert/strict';
import {nativeBuildFlags} from '../native-build-env.mjs';

test('Windows native flags preserve spaces and both compiler path spellings',()=>{
  const roots={platform:'win32',home:'C:\\Users\\sample',workspace:'D:\\Build folder\\readmd',cargoHome:'C:\\Users\\sample\\.cargo',rustupHome:'C:\\Users\\sample\\.rustup'};
  const flags=nativeBuildFlags({...roots,existing:['-C','target-feature=+crt-static']});
  assert(flags.includes('--remap-path-prefix=D:\\Build folder\\readmd=/readmd/source'));
  assert(flags.includes('--remap-path-prefix=D:/Build folder/readmd=/readmd/source'));
  assert.deepEqual(flags.slice(0,2),['-C','target-feature=+crt-static']);
  assert(flags.indexOf('--remap-path-prefix=C:\\Users\\sample=/readmd/home')<flags.indexOf('--remap-path-prefix=C:\\Users\\sample\\.cargo=/readmd/cargo'));
  assert.equal(flags.join('\x1f').split('\x1f').length,flags.length);
});

test('Unix roots remain separate, deterministic native arguments',()=>{
  const roots={platform:'linux',home:'/home/build',workspace:'/work/readmd',cargoHome:'/home/build/.cargo',rustupHome:'/home/build/.rustup'};
  const a=nativeBuildFlags(roots);assert.equal(a.length,4);assert.deepEqual(a,nativeBuildFlags(roots));assert(a.includes('--remap-path-prefix=/work/readmd=/readmd/source'));
});

test('repeated option markers in preexisting encoded flags keep their argument meaning',()=>{
  const existing=['-C','opt-level=2','-C','debuginfo=0'];
  const flags=nativeBuildFlags({platform:'linux',home:'/home/build',workspace:'/work/readmd',cargoHome:'/home/build/.cargo',rustupHome:'/home/build/.rustup',existing});
  assert.deepEqual(flags.slice(0,existing.length),existing);
  assert.equal(flags.filter(value=>value==='-C').length,2);
});

test('relative roots and environment-file injection are rejected',()=>{
  const roots={platform:'linux',home:'/home/build',workspace:'/work/readmd',cargoHome:'/home/build/.cargo',rustupHome:'/home/build/.rustup'};
  for(const workspace of ['relative','/tmp/a\nOTHER=value','/tmp/a\x1f-bad','/tmp/a\0bad'])assert.throws(()=>nativeBuildFlags({...roots,workspace}),/absolute paths/);
  assert.throws(()=>nativeBuildFlags({...roots,existing:['-C\nBAD=value']}),/Invalid native build flags/);
});
