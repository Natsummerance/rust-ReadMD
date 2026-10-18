// Local QA only. Keeps redistributionApproved=false and never writes assets/.
// Reuses previously locked compressed bytes; no download or engine execution.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {pipeline} from 'node:stream/promises';
const root=process.cwd(),base=path.join(root,'.cache/capability-upgrade');
const input=path.join(base,'tex-inputs/stream-review-fixture-concurrent');
const expected=JSON.parse(fs.readFileSync(path.join(input,'fixture.lock.json')));
assert.equal(expected.schema,2);assert.equal(expected.id,'tex-review-fixture');
assert.equal(expected.review.redistributionApproved,false);
assert.equal(expected.files.length,34822);
const archive=path.join(input,'fixture.readmd-pack');
const fd=fs.openSync(archive,'r'),prefix=Buffer.alloc(12);fs.readSync(fd,prefix);fs.closeSync(fd);
assert.equal(prefix.subarray(0,8).toString(),'RMDPKG2\n');
const oldLength=prefix.readUInt32LE(8);
const header=Buffer.alloc(oldLength),inputFd=fs.openSync(archive,'r');fs.readSync(inputFd,header,0,oldLength,12);fs.closeSync(inputFd);
assert.deepEqual(JSON.parse(header),expected);
const out=fs.mkdtempSync(path.join(base,'tex-managed-fixture-'));
const assets=path.join(out,'assets');fs.mkdirSync(assets);
function linkTree(source,target){
  fs.mkdirSync(target,{recursive:true});
  for(const entry of fs.readdirSync(source,{withFileTypes:true})){
    assert(!entry.isSymbolicLink());const from=path.join(source,entry.name),to=path.join(target,entry.name);
    if(entry.isDirectory())linkTree(from,to);else{assert(entry.isFile());fs.linkSync(from,to);}
  }
}
linkTree(path.join(root,'assets'),assets);
const manifest=structuredClone(expected);manifest.id='latex';manifest.version='2026.10-r1';
manifest.review.qaOnly=true;
const bytes=Buffer.from(JSON.stringify(manifest)),size=Buffer.alloc(4);size.writeUInt32LE(bytes.length);
const pack=path.join(out,'latex-qa-only.readmd-pack');
fs.writeFileSync(pack,Buffer.concat([prefix.subarray(0,8),size,bytes]),{flag:'wx'});
await pipeline(fs.createReadStream(archive,{start:12+oldLength,highWaterMark:65536}),fs.createWriteStream(pack,{flags:'a',highWaterMark:65536}));
fs.writeFileSync(path.join(assets,'capabilities/latex-2026.10-r1-windows-x64.lock.json'),bytes,{flag:'wx'});
const sha=async file=>{const digest=crypto.createHash('sha256');for await(const chunk of fs.createReadStream(file,{highWaterMark:65536}))digest.update(chunk);return digest.digest('hex');};
const receipt={qaOnly:true,published:false,redistributionApproved:false,applicationAssetsChanged:false,assets,pack,
  manifestSha256:await sha(path.join(assets,'capabilities/latex-2026.10-r1-windows-x64.lock.json')),
  archiveSha256:await sha(pack),bytes:fs.statSync(pack).size,files:manifest.files.length,
  installedBytes:manifest.files.reduce((n,f)=>n+f.size,0)};
// Real v2 installation requires an archive-level catalog identity as well as
// the file lock. This entry exists only in this isolated QA asset directory.
const catalogFile=path.join(assets,'capabilities/catalog.json');
const before=fs.readFileSync(path.join(root,'assets/capabilities/catalog.json'));
const catalog=JSON.parse(before);
assert(!catalog.packs.some(p=>p.id==='latex'));
catalog.pending=catalog.pending.filter(p=>p.id!=='latex');
catalog.packs.push({id:'latex',version:'2026.10-r1',platform:'windows-x64',format:'stream-v2',
  labelKey:'capability.latex.name',descriptionKey:'capability.latex.purpose',features:['xelatex','pdflatex'],
  downloadBytes:receipt.bytes,installedBytes:receipt.installedBytes,sha256:receipt.archiveSha256,
  manifestSha256:receipt.manifestSha256,url:'https://github.com/Natsummerance/tool-ReadMD/releases/download/qa-only-never-published/latex-qa-only.readmd-pack',
  license:'UNPUBLISHED LOCAL QA ONLY — redistribution review incomplete',qaOnly:true});
fs.unlinkSync(catalogFile);fs.writeFileSync(catalogFile,JSON.stringify(catalog,null,2)+'\n',{flag:'wx'});
assert.deepEqual(fs.readFileSync(path.join(root,'assets/capabilities/catalog.json')),before);
receipt.catalogSha256=await sha(catalogFile);
fs.writeFileSync(path.join(out,'receipt.json'),JSON.stringify(receipt,null,2)+'\n',{flag:'wx'});
console.log(JSON.stringify({fixture:path.relative(root,out),...receipt}));
