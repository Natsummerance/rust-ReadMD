import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import crypto from 'node:crypto';
import {signingOptions,signTarget,notarizeTarget} from '../macos-sign.mjs';

test('free ad-hoc signing cannot silently claim Developer ID distribution',()=>{
  assert.deepEqual(signingOptions({}),{mode:'development',identity:'-',keychain:null});
  for(const env of [
    {READMD_MACOS_SIGNING_MODE:'distribution'},
    {READMD_MACOS_SIGNING_MODE:'distribution',READMD_MACOS_SIGNING_IDENTITY:'-'},
    {READMD_MACOS_SIGNING_MODE:'anything'},
  ])assert.throws(()=>signingOptions(env));
});

test('real macOS bundle sealing detects missing and changed resources',{skip:process.platform!=='darwin'},()=>{
  const temp=fs.mkdtempSync(path.join(os.tmpdir(),'readmd-macos-sign-'));
  try {
    const app=path.join(temp,'Fixture.app'),contents=path.join(app,'Contents');
    fs.mkdirSync(path.join(contents,'MacOS'),{recursive:true});
    fs.mkdirSync(path.join(contents,'Resources'));
    fs.mkdirSync(path.join(contents,'Helpers'));
    fs.copyFileSync('/usr/bin/true',path.join(contents,'MacOS','Fixture'));
    const engine=path.join(contents,'Resources/assets/engines/resvg/macos-arm64');
    fs.mkdirSync(engine,{recursive:true});
    const worker=path.join(engine,'resvg');fs.copyFileSync('/usr/bin/true',worker);
    const hash=()=>crypto.createHash('sha256').update(fs.readFileSync(worker)).digest('hex');
    fs.writeFileSync(path.join(engine,'lock.json'),JSON.stringify({id:'resvg',version:'0.48.1',sha256:hash(),size:fs.statSync(worker).size}));
    fs.writeFileSync(path.join(contents,'Info.plist'),'<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleExecutable</key><string>Fixture</string><key>CFBundleIdentifier</key><string>test.readmd.signing</string><key>CFBundlePackageType</key><string>APPL</string></dict></plist>');
    const resource=path.join(contents,'Resources','document.md');
    fs.writeFileSync(resource,'# Original\n');
    const verify=()=>spawnSync('/usr/bin/codesign',['--verify','--deep','--strict',app],{stdio:'pipe'}).status;
    assert.notEqual(verify(),0,'a signed executable copied into a new bundle is insufficient');
    const report=signTarget(app,{env:{}});
    assert.equal(report.binaries,2);assert.equal(report.bundles,1);
    assert.equal(report.mode,'development');assert.equal(report.notarized,false);
    const lock=JSON.parse(fs.readFileSync(path.join(engine,'lock.json')));
    assert.equal(lock.sha256,hash());assert.equal(lock.size,fs.statSync(worker).size);
    assert.equal(verify(),0);
    fs.writeFileSync(resource,'# Tampered\n');
    assert.notEqual(verify(),0,'modified resources must fail the real signature check');
    signTarget(app,{env:{}});assert.equal(verify(),0);
    fs.writeFileSync(path.join(engine,'lock.json'),JSON.stringify({...lock,sha256:'0'.repeat(64)}));
    assert.throws(()=>signTarget(app,{env:{}}),/integrity before signing/);
    fs.writeFileSync(path.join(engine,'lock.json'),JSON.stringify({...lock,sha256:hash(),size:fs.statSync(worker).size}));
    assert.throws(()=>notarizeTarget(app,{env:{}}),/Development signatures/);
    const alias=path.join(contents,'Resources','outside');
    fs.symlinkSync(os.tmpdir(),alias);
    assert.throws(()=>signTarget(app,{env:{}}),/escaping symlink/);
  } finally {fs.rmSync(temp,{recursive:true});}
});
