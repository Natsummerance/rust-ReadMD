import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {test} from 'node:test';
import {execFileSync} from 'node:child_process';
import {validateOfficeArchive} from '../prepare-office-runtime.mjs';
import {officeDependencyNoticePlan} from '../build-office-runtime.mjs';

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

test('Windows auto-CRLF checkout preserves hash-bound Office manifests and license notices',t=>{
  const prefix='tools/office/third-party/';
  const notices=JSON.parse(fs.readFileSync(path.join(root,prefix,'notices.lock.json'),'utf8'));
  const cache=path.join(root,'.cache');fs.mkdirSync(cache,{recursive:true});
  const checkout=fs.mkdtempSync(path.join(cache,'office-notice-checkout-'));
  t.after(()=>fs.rmSync(checkout,{recursive:true,force:true}));
  const files=['source.lock.json','notices.lock.json',...notices.files.map(item=>item.path)].map(name=>prefix+name);
  const originals=new Map(files.map(file=>[file,fs.readFileSync(path.join(root,file))]));
  fs.copyFileSync(path.join(root,'.gitattributes'),path.join(checkout,'.gitattributes'));
  for(const [file,bytes] of originals){const target=path.join(checkout,file);fs.mkdirSync(path.dirname(target),{recursive:true});fs.writeFileSync(target,bytes);}
  const git=args=>execFileSync('git',args,{cwd:checkout,windowsHide:true,stdio:['ignore','pipe','pipe']});
  git(['init','--quiet','--template=']);
  git(['-c','core.autocrlf=false','-c','core.safecrlf=false','add','-f','--','.gitattributes',...files]);
  for(const file of files)fs.unlinkSync(path.join(checkout,file));
  git(['-c','core.autocrlf=true','-c','core.eol=crlf','checkout-index','--all','--force']);
  for(const [file,bytes] of originals)assert.deepEqual(fs.readFileSync(path.join(checkout,file)),bytes,file+' must survive a Windows-style checkout');
});

function noticeFixture(t) {
  const cache=path.join(root,'.cache');fs.mkdirSync(cache,{recursive:true});
  const fixture=fs.mkdtempSync(path.join(cache,'office-notice-fixture-'));
  t.after(()=>fs.rmSync(fixture,{recursive:true,force:true}));
  const source=path.join(fixture,'upstream'),thirdParty=path.join(fixture,'tools/office/third-party');
  fs.mkdirSync(thirdParty,{recursive:true});
  for(const platform of ['macos','linux']){const file=path.join(source,'crates/ibis-office-app',platform,'license.md');fs.mkdirSync(path.dirname(file),{recursive:true});fs.writeFileSync(file,'No inherited package rows.\n');}
  fs.writeFileSync(path.join(source,'Cargo.lock'),'');fs.writeFileSync(path.join(thirdParty,'notices.lock.json'),JSON.stringify({schema:1,files:[]}));
  const packages=['first','second'].map(name=>{const base=path.join(fixture,'registry',name);fs.mkdirSync(base,{recursive:true});fs.writeFileSync(path.join(base,'Cargo.toml'),'');return {name,version:'1.0.0',license:'MIT',manifest_path:path.join(base,'Cargo.toml')};});
  return {root:fixture,source,metadata:{packages},dependencies:'first v1.0.0\nsecond v1.0.0\n',thirdParty};
}

test('Office notice preflight reports every missing selected package before any copy',t=>{
  const fixture=noticeFixture(t);
  assert.throws(()=>officeDependencyNoticePlan(fixture),/Missing dependency license notices: first 1\.0\.0, second 1\.0\.0/);
  assert.throws(()=>officeDependencyNoticePlan({...fixture,metadata:{packages:[]}}),/Incomplete Office dependency metadata/);
});

test('supplemental notices require the original source commit, locked crate hash and exact text bytes',t=>{
  const fixture=noticeFixture(t),pkg=fixture.metadata.packages[0],commit='a'.repeat(40),crateSha256='b'.repeat(64),bytes=Buffer.from('Original upstream notice.\n');
  fs.writeFileSync(path.join(path.dirname(pkg.manifest_path),'.cargo_vcs_info.json'),JSON.stringify({git:{sha1:commit}}));
  fs.writeFileSync(path.join(fixture.source,'Cargo.lock'),'[[package]]\nname = "first"\nversion = "1.0.0"\nchecksum = "'+crateSha256+'"\n');
  const item={package:'first',version:'1.0.0',path:'notices/first/LICENSE-MIT',commit,crateSha256,size:bytes.length,sha256:hash(bytes),url:'https://example.invalid/first/LICENSE-MIT'};
  const notice=path.join(fixture.thirdParty,item.path);fs.mkdirSync(path.dirname(notice),{recursive:true});fs.writeFileSync(notice,bytes);
  const writePin=()=>fs.writeFileSync(path.join(fixture.thirdParty,'notices.lock.json'),JSON.stringify({schema:1,files:[item]}));writePin();
  const input={...fixture,dependencies:'first v1.0.0\n'};
  const plan=officeDependencyNoticePlan(input);assert.equal(plan.notices[0].noticeSource,'pinned-dependency-repository');assert.equal(plan.copies.length,1);
  fs.writeFileSync(notice,bytes.toString().replaceAll('\n','\r\n'));
  assert.throws(()=>officeDependencyNoticePlan(input),/Pinned dependency notice changed/);fs.writeFileSync(notice,bytes);
  item.commit='c'.repeat(40);writePin();assert.throws(()=>officeDependencyNoticePlan(input),/source differs from its pin/);
  item.commit=commit;item.crateSha256='d'.repeat(64);writePin();assert.throws(()=>officeDependencyNoticePlan(input),/source differs from its pin/);
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
