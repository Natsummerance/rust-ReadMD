// Maintainer-only offline inventory. Classification never grants permission to
// redistribute: notices, matching sources and third-party terms need review.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const base=path.join(root,'.cache/capability-upgrade/tex-inputs');
const pin=JSON.parse(fs.readFileSync(path.join(root,'tools/tex-runtime-inputs.lock.json')));
const archive=path.join(base,pin.archive.name);
if(fs.statSync(archive).size!==pin.archive.size)throw Error('Pinned TeX input size differs');
const digest=crypto.createHash('sha256');
for await(const bytes of fs.createReadStream(archive))digest.update(bytes);
if(digest.digest('hex')!==pin.archive.sha256)throw Error('Pinned TeX input digest differs');
const inventory=JSON.parse(fs.readFileSync(path.join(base,'inventory.json')));
if(JSON.stringify(inventory.archive)!==JSON.stringify(pin.archive))throw Error('Run the locked offline unpack audit first');
const entry=inventory.files.find(f=>f.path==='tlpkg/texlive.tlpdb');
const input=fs.readFileSync(path.join(base,'TinyTeX/tlpkg/texlive.tlpdb'));
if(!entry||input.length!==entry.size||crypto.createHash('sha256').update(input).digest('hex')!==entry.sha256)throw Error('Package database differs from pinned input');
const names=new Set(),packages=[],licenseCounts={};
let sourceFileSections=0;
for(const block of input.toString('utf8').replaceAll('\r\n','\n').split(/\n\s*\n/)){
  if(!block.trim())continue;
  const value=key=>block.match(new RegExp('^'+key+' (.*)$','m'))?.[1];
  const name=value('name');
  if(!name||names.has(name))throw Error('Invalid or duplicate package record');
  names.add(name);
  if(name.startsWith('00texlive.'))continue;
  const license=value('catalogue-license')||'not-declared-in-package-record';
  licenseCounts[license]=(licenseCounts[license]||0)+1;
  const sourcesIncluded=/^srcfiles(?: |$)/m.test(block);
  if(sourcesIncluded)sourceFileSections++;
  packages.push({name,category:value('category'),revision:value('revision')||null,license,
    sourceArchive:value('srccontainerchecksum')?{size:Number(value('srccontainersize')),sha512:value('srccontainerchecksum')}:null,
    sourcesIncluded,ctan:value('catalogue-ctan')||null,
    dependencies:[...block.matchAll(/^depend (.+)$/gm)].map(m=>m[1]),
    classificationOnly:true});
}
const files=new Set(inventory.files.map(f=>f.path));
const report={schema:1,reviewed:false,approvedForRedistribution:false,
  distribution:'TinyTeX v2026.10',source:pin.source,input:pin.archive,
  databaseRecords:names.size,componentCount:packages.length,licenseCounts,sourceFileSections,
  expanded:{files:inventory.files.length,bytes:inventory.totalBytes},
  missingChineseResources:['texmf-dist/tex/latex/ctex/ctex.sty','texmf-dist/tex/xelatex/xecjk/xeCJK.sty'].filter(p=>!files.has(p)),
  scope:'Offline classification of the digest-pinned input. A source archive checksum or licence label is not a completed redistribution review. No application engine is enabled by this tool.',packages};
fs.writeFileSync(path.join(base,'license-inventory.json'),JSON.stringify(report,null,2)+'\n');
console.log(JSON.stringify({audited:true,offline:true,databaseRecords:names.size,components:packages.length,sourceFileSections,missingChineseResources:report.missingChineseResources.length,approvedForRedistribution:false}));
