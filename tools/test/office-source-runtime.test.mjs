import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {test} from 'node:test';
import {execFileSync} from 'node:child_process';
import {validateOfficeArchive} from '../prepare-office-runtime.mjs';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const pins=JSON.parse(fs.readFileSync(path.join(root,'tools/office/third-party/source.lock.json'),'utf8'));
const hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');

test('supplemental Office notices retain the exact locked repository bytes',()=>{
  const notices=JSON.parse(fs.readFileSync(path.join(root,'tools/office/third-party/notices.lock.json'),'utf8'));
  assert(notices.files.length>0);
  for(const item of notices.files) {
    const file=path.join(root,'tools/office/third-party',item.path);
    assert.ok(fs.lstatSync(file).isFile(),item.path);
    const bytes=fs.readFileSync(file);
    assert.equal(bytes.length,item.size,item.path);
    assert.equal(hash(bytes),item.sha256,item.path);
  }
});

test('Windows auto-CRLF checkout preserves hash-bound Office manifests and license notices',()=>{
  const prefix='tools/office/third-party/';
  const notices=JSON.parse(fs.readFileSync(path.join(root,prefix,'notices.lock.json'),'utf8'));
  for(const name of ['source.lock.json','notices.lock.json',...notices.files.map(item=>item.path)]) {
    const file=prefix+name;
    const blob=execFileSync('git',['show','HEAD:'+file],{cwd:root,windowsHide:true});
    const checkout=execFileSync('git',['-c','core.autocrlf=true','-c','core.eol=crlf','cat-file','--filters','HEAD:'+file],{cwd:root,windowsHide:true});
    assert.deepEqual(checkout,blob,file+' must survive Windows checkout without newline conversion');
    assert.deepEqual(fs.readFileSync(path.join(root,file)),blob,file+' must retain its repository bytes');
  }
});

test('Office source extraction refuses traversal, aliases and duplicate members',()=>{
  const prefix='gpui-test-pin/';
  assert.doesNotThrow(()=>validateOfficeArchive([prefix,prefix+'src/lib.rs'],['drwxr-xr-x x','-rw-r--r-- x'],prefix));
  for(const [names,types] of [
    [[prefix+'../outside'],['-rw-r--r-- x']],
    [[prefix+'src\\escape'],['-rw-r--r-- x']],
    [[prefix+'link'],['lrwxr-xr-x x -> ../../outside']],
    [[prefix+'hardlink'],['hrw-r--r-- x']],
    [[prefix+'same',prefix+'same'],['-rw-r--r-- x','-rw-r--r-- x']],
    [['/absolute'],['-rw-r--r-- x']],
  ])assert.throws(()=>validateOfficeArchive(names,types,prefix),/Unsafe/);
});

test('prepared upstream layout and PDF algorithms retain their pinned bytes',t=>{
  const source=path.join(root,'.cache/readmd-office-source/upstream');
  if(!fs.existsSync(source)){t.skip('source preparation has not run');return;}
  for(const name of ['Cargo.lock','crates/ibis-office-app/src/export.rs','crates/ibis-office-app/src/pdf_vector.rs','crates/ibis-office-app/src/render/web_image.rs'])
    assert.equal(hash(fs.readFileSync(path.join(source,name))),pins.files[name],name);
});

test('built Office runtime binds every packaged byte and preserves upstream notices',t=>{
  const runtime=process.env.READMD_TEST_OFFICE_RUNTIME;
  if(!runtime){t.skip('set READMD_TEST_OFFICE_RUNTIME to inspect a built runtime');return;}
  const manifest=JSON.parse(fs.readFileSync(path.join(runtime,'runtime.json'),'utf8'));
  assert.equal(manifest.engine,'office-layout');
  assert.equal(manifest.source_commit,pins.commit);
  assert.equal(hash(fs.readFileSync(path.join(runtime,manifest.executable))),manifest.sha256);
  for(const item of manifest.files) {
    const file=path.join(runtime,item.path);
    assert.ok(fs.lstatSync(file).isFile());
    const bytes=fs.readFileSync(file);assert.equal(bytes.length,item.size,item.path);assert.equal(hash(bytes),item.sha256,item.path);
  }
  for(const file of ['LICENSE','NOTICE','docs/TRADEMARK.md','docs/THIRD-PARTY-NOTICES.md'])
    assert.equal(hash(fs.readFileSync(path.join(runtime,'licenses/upstream',file))),pins.files[file],file);
  assert.ok(manifest.files.some(item=>item.path==='Resources/artborders/OfficeOpenXML-WordprocessingMLArtBorders.zip'));
});
