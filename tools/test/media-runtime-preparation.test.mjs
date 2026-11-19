import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import {mediaSpecs,validateMediaRecipe,inspectMediaExecutable,prepareMedia,inspectPreparedMedia,registerPreparedMedia} from '../lib/media-runtime.mjs';
import {main} from '../prepare-media-runtime.mjs';
import {prepareCatalog} from '../prepare-capability-catalog.mjs';

const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
function fixture(t,{approved=false}={}) {
  const root=fs.mkdtempSync(path.join(fs.realpathSync(os.tmpdir()),'readmd-media-'));
  t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  const inputs=path.join(root,'inputs');fs.mkdirSync(inputs);
  // Header-shaped synthetic bytes only. Never executed or native evidence.
  const binary=Buffer.alloc(256);binary.writeUInt32LE(0xfeedfacf,0);binary.writeUInt32LE(0x0100000c,4);binary.writeUInt32LE(2,12);
  const data={ffmpeg:binary,ffprobe:binary,LICENSE:Buffer.from('Synthetic QA license.\n'),SOURCE:Buffer.from('Synthetic fixture, no upstream engine.\n')};
  for(const [name,bytes] of Object.entries(data))fs.writeFileSync(path.join(inputs,name),bytes);
  const recipe={schema:1,id:'media-ffmpeg',version:mediaSpecs['media-ffmpeg'].version,platform:'macos-arm64',files:Object.entries(data).map(([name,bytes])=>({path:name.startsWith('ff')?'bin/'+name:name,input:name,size:bytes.length,sha256:sha(bytes),executable:name.startsWith('ff')})),review:{license:'Synthetic QA only',minimumPlatform:'Synthetic macOS target',portable:true,redistributionApproved:approved,sourceDeliveryReviewed:approved,sources:[{component:'Synthetic fixture',url:'https://example.invalid/synthetic-source.tar.gz',sha256:'1'.repeat(64)}],buildConfiguration:['Synthetic header fixture; no compiler or engine invocation']}};
  const lock=path.join(root,'reviewed.lock.json');fs.writeFileSync(lock,JSON.stringify(recipe));
  return {root,inputs,recipe,lock,output:path.join(root,'prepared'),data};
}
test('media versions and model pins agree with the installation runtime',()=>{
  const rust=fs.readFileSync(new URL('../../rust/readmd-kernel/src/whisper_cpp.rs',import.meta.url),'utf8');
  for(const value of [mediaSpecs.whisper.version,mediaSpecs['media-ffmpeg'].version,mediaSpecs.whisper.model.path,mediaSpecs.whisper.model.sha256])assert.ok(rust.includes('"'+value+'"'));
  assert.equal(mediaSpecs.whisper.model.size,59707625);
});
test('offline assembly produces a verified v2 pack without registration or installation',async t=>{
  const f=fixture(t),receipt=await prepareMedia(f);
  assert.equal(receipt.published,false);assert.equal(receipt.enabledInCatalog,false);assert.equal(receipt.nativeOperationVerified,false);assert.equal(receipt.redistributionApproved,false);
  const row=await inspectPreparedMedia(f.output,{lock:f.lock});
  assert.equal(row.entry.delivery,'offline-only');assert.equal(row.entry.sha256,receipt.sha256);assert.equal(row.manifest.files.length,4);assert.equal(row.entry.url,undefined);
  assert.deepEqual(fs.readdirSync(f.inputs).sort(),Object.keys(f.data).sort());
  const second=await prepareMedia({...f,output:path.join(f.root,'again')});assert.equal(second.sha256,receipt.sha256);
  await assert.rejects(prepareMedia(f),/already exists/);
  await assert.rejects(registerPreparedMedia({directory:f.output,assets:f.root}),/Explicit trusted/);
});
test('changed input is preserved and rejected before output creation',async t=>{
  const f=fixture(t);fs.writeFileSync(path.join(f.inputs,'ffmpeg'),'foreign bytes');
  await assert.rejects(prepareMedia(f),/input digest mismatch/);assert.equal(fs.existsSync(f.output),false);assert.equal(fs.readFileSync(path.join(f.inputs,'ffmpeg'),'utf8'),'foreign bytes');
});
test('input paths cannot escape or redirect through a symlink',async t=>{
  const f=fixture(t);
  for(const input of ['../foreign','/absolute','a\\b']){const r=structuredClone(f.recipe);r.files[0].input=input;assert.throws(()=>validateMediaRecipe(r),/input member/);}
  fs.unlinkSync(path.join(f.inputs,'ffmpeg'));fs.symlinkSync(path.join(f.inputs,'ffprobe'),path.join(f.inputs,'ffmpeg'));
  await assert.rejects(prepareMedia(f),/unredirected/);assert.equal(fs.existsSync(f.output),false);
});
test('inventory requires notices and source provision and excludes loaders and model substitutions',t=>{
  const f=fixture(t);
  for(const change of [r=>r.files.splice(2,1),r=>r.files.splice(3,1),r=>r.files.push({...r.files[0],path:'bin/loader.dylib'}),r=>{r.review.portable=false;},r=>{r.review.sources[0].url='https://user:password@example.invalid/source';},r=>{r.review.buildConfiguration=['build /Users/example/private'];}]){const r=structuredClone(f.recipe);change(r);assert.throws(()=>validateMediaRecipe(r));}
  const r=structuredClone(f.recipe);r.id='whisper';r.version=mediaSpecs.whisper.version;r.files=r.files.filter(f=>f.path!=='bin/ffprobe');r.files[0].path='bin/whisper-cli';r.files.push({path:'licenses/WHISPER-MODEL.txt',input:'LICENSE',size:1,sha256:'0'.repeat(64),executable:false});r.files.push({path:mediaSpecs.whisper.model.path,size:mediaSpecs.whisper.model.size,input:'model.bin',sha256:'0'.repeat(64),executable:false});
  assert.throws(()=>validateMediaRecipe(r),/application-pinned/);
});
test('Homebrew-style Mach-O dependencies are refused before assembly',async t=>{
  const f=fixture(t);
  for(const name of ['/opt/homebrew/lib/libavcodec.dylib','/usr/lib/../../opt/homebrew/lib/libavcodec.dylib']){
  const commandSize=24+Buffer.byteLength(name)+1,b=Buffer.alloc(32+commandSize);
  b.writeUInt32LE(0xfeedfacf,0);b.writeUInt32LE(0x0100000c,4);b.writeUInt32LE(2,12);b.writeUInt32LE(1,16);b.writeUInt32LE(commandSize,20);b.writeUInt32LE(0xc,32);b.writeUInt32LE(commandSize,36);b.writeUInt32LE(24,40);b.write(name,56);
  fs.writeFileSync(path.join(f.inputs,'ffmpeg'),b);f.recipe.files[0].size=b.length;f.recipe.files[0].sha256=sha(b);fs.writeFileSync(f.lock,JSON.stringify(f.recipe));
  assert.throws(()=>inspectMediaExecutable(path.join(f.inputs,'ffmpeg'),'macos-x64'),/architecture/);
  await assert.rejects(prepareMedia(f),/non-system shared library/);assert.equal(fs.existsSync(f.output),false);
  }
});
test('a new receipt hash cannot bless changed payloads, manifests or trusted inputs',async t=>{
  const f=fixture(t),receipt=await prepareMedia(f),pack=path.join(f.output,receipt.packFile),original=fs.readFileSync(pack);
  const changed=Buffer.from(original);changed[changed.length-9]^=1;fs.writeFileSync(pack,changed);
  await assert.rejects(inspectPreparedMedia(f.output,{lock:f.lock}),/differs from receipt/);
  const receiptFile=path.join(f.output,'prepared.json');fs.writeFileSync(receiptFile,JSON.stringify({...receipt,sha256:sha(changed)}));
  await assert.rejects(inspectPreparedMedia(f.output,{lock:f.lock}));
  fs.writeFileSync(pack,original);fs.writeFileSync(receiptFile,JSON.stringify(receipt));
  const manifestFile=path.join(f.output,receipt.manifestFile),manifest=JSON.parse(fs.readFileSync(manifestFile));manifest.files[0].sha256='0'.repeat(64);fs.writeFileSync(manifestFile,JSON.stringify(manifest));
  await assert.rejects(inspectPreparedMedia(f.output,{lock:f.lock}),/differs from reviewed inputs/);
  fs.writeFileSync(f.lock,JSON.stringify({...f.recipe,version:'different'}));
  await assert.rejects(inspectPreparedMedia(f.output,{lock:f.lock}),/explicitly trusted lock/);
});
test('registration requires exact native evidence and preserves catalog authority on failure',async t=>{
  const f=fixture(t,{approved:true}),receipt=await prepareMedia(f),assets=path.join(f.root,'assets'),caps=path.join(assets,'capabilities');fs.mkdirSync(caps,{recursive:true});
  const online={id:'fonts',version:'1',platform:'universal',sha256:'a'.repeat(64),downloadBytes:1,installedBytes:1,url:'https://github.com/Natsummerance/tool-ReadMD/releases/download/packs-qa/readmd-fonts-1-universal.readmd-pack'};
  const node={id:'node',version:'24.22.2',platform:'macos-arm64',delivery:'offline-only',format:'stream-v2',sha256:'b'.repeat(64),manifestSha256:'c'.repeat(64),downloadBytes:1,installedBytes:1};
  const catalog={schema:1,repository:'Natsummerance/tool-ReadMD',release:'packs-qa',packs:[online],offlinePacks:[node],pending:[{id:'speech',reason:'not-packaged'}]};
  const catalogFile=path.join(caps,'catalog.json');fs.writeFileSync(catalogFile,JSON.stringify(catalog));const before=fs.readFileSync(catalogFile);
  const nativeReceipt=path.join(f.root,'native.json'),native={schema:1,id:receipt.id,version:receipt.version,platform:'windows-x64',sha256:receipt.sha256,manifestSha256:receipt.manifestSha256,install:true,engineOperation:true,uninstall:true,synthetic:false,evidenceSha256:'d'.repeat(64)};fs.writeFileSync(nativeReceipt,JSON.stringify(native));
  await assert.rejects(registerPreparedMedia({directory:f.output,lock:f.lock,assets,nativeReceipt}),/exact platform/);assert.deepEqual(fs.readFileSync(catalogFile),before);
  // Metadata validation with synthetic inputs is not native engine evidence.
  native.platform=receipt.platform;native.synthetic=true;fs.writeFileSync(nativeReceipt,JSON.stringify(native));
  await assert.rejects(registerPreparedMedia({directory:f.output,lock:f.lock,assets,nativeReceipt}),/exact platform/);assert.deepEqual(fs.readFileSync(catalogFile),before);
});
test('local registration keeps other platform locks immutable and requires evidence bytes',async t=>{
  // Exercise the source-edit transaction in temporary assets only. The receipt
  // is a synthetic maintainer claim, not a certification of an engine run.
  const f=fixture(t,{approved:true}),receipt=await prepareMedia(f),assets=path.join(f.root,'assets'),caps=path.join(assets,'capabilities');fs.mkdirSync(caps,{recursive:true});
  const online={id:'fonts',version:'1',platform:'universal',sha256:'a'.repeat(64),downloadBytes:1,installedBytes:1,url:'https://github.com/Natsummerance/tool-ReadMD/releases/download/packs-qa/readmd-fonts-1-universal.readmd-pack'};
  const offline={id:'node',version:'24.22.2',platform:'macos-arm64',delivery:'offline-only',format:'stream-v2',sha256:'b'.repeat(64),manifestSha256:'c'.repeat(64),downloadBytes:1,installedBytes:1};
  const catalogFile=path.join(caps,'catalog.json');fs.writeFileSync(catalogFile,JSON.stringify({schema:1,repository:'Natsummerance/tool-ReadMD',release:'packs-qa',packs:[online],offlinePacks:[offline],pending:[{id:'speech'}]}));
  const evidence=Buffer.from('Synthetic transaction test; no native engine operation.');fs.writeFileSync(path.join(f.root,'evidence.txt'),evidence);
  const nativeReceipt=path.join(f.root,'native.json'),native={schema:1,id:receipt.id,version:receipt.version,platform:receipt.platform,sha256:receipt.sha256,manifestSha256:receipt.manifestSha256,install:true,engineOperation:true,uninstall:true,synthetic:false,evidenceFile:'evidence.txt',evidenceSha256:'0'.repeat(64)};fs.writeFileSync(nativeReceipt,JSON.stringify(native));
  await assert.rejects(registerPreparedMedia({directory:f.output,lock:f.lock,assets,nativeReceipt}),/evidence bytes/);
  native.evidenceSha256=sha(evidence);fs.writeFileSync(nativeReceipt,JSON.stringify(native));
  assert.equal((await registerPreparedMedia({directory:f.output,lock:f.lock,assets,nativeReceipt})).registered,true);
  const catalog=JSON.parse(fs.readFileSync(catalogFile));assert.deepEqual(catalog.packs,[online]);assert.deepEqual(catalog.offlinePacks[0],offline);assert.equal(catalog.offlinePacks[1].delivery,'offline-only');assert.deepEqual(catalog.pending,[{id:'speech'}]);
  await registerPreparedMedia({directory:f.output,lock:f.lock,assets,nativeReceipt});
  const manifestFile=path.join(caps,receipt.manifestFile),bytes=fs.readFileSync(manifestFile),tampered=JSON.parse(bytes);tampered.files[0].sha256='0'.repeat(64);fs.writeFileSync(manifestFile,JSON.stringify(tampered));
  const before=fs.readFileSync(catalogFile);await assert.rejects(registerPreparedMedia({directory:f.output,lock:f.lock,assets,nativeReceipt}),/old application authority/);assert.deepEqual(fs.readFileSync(catalogFile),before);
});
test('catalog regeneration preserves offline media, Node entries and pending platform gaps',t=>{
  const f=fixture(t),caps=path.join(f.root,'capabilities');fs.mkdirSync(caps);const inputs=[];
  for(const id of ['pdfium','typst','ocr','fonts']){
    const bytes=Buffer.from('synthetic '+id),manifest={schema:1,id,version:'1',platform:'universal',files:[{path:'sample.txt',size:bytes.length,sha256:sha(bytes)}]},pack=path.join(f.root,id+'.readmd-pack');
    fs.writeFileSync(path.join(caps,`${id}-1-universal.lock.json`),JSON.stringify(manifest));fs.writeFileSync(pack,JSON.stringify({manifest,payload:{'sample.txt':bytes.toString('base64')}}));inputs.push(pack);
  }
  const baseline=prepareCatalog({inputs,release:'packs-qa',capabilities:caps}),offline=['node','media-ffmpeg'].map(id=>({id,version:'1',platform:'macos-arm64',format:'stream-v2',delivery:'offline-only',sha256:'a'.repeat(64),manifestSha256:'b'.repeat(64),downloadBytes:1,installedBytes:1}));
  baseline.offlinePacks=offline;baseline.pending.push({id:'test',reason:'retain platform gaps'});fs.writeFileSync(path.join(caps,'catalog.json'),JSON.stringify(baseline));
  const updated=prepareCatalog({inputs,capabilities:caps});assert.deepEqual(updated.offlinePacks,offline);assert.deepEqual(updated.pending,baseline.pending);assert.equal(updated.release,'packs-qa');
});
test('CLI has no download, install or implicit registration mode',async()=>{
  for(const args of [[],['download','--inputs','somewhere'],['build','--lock','x','--inputs','x','--output','x','--download','true'],['register','--prepared','x']])await assert.rejects(main(args));
});
