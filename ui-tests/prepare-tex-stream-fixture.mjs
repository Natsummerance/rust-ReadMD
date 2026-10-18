// Explicit offline capacity fixture. The unreviewed TeX input never enters the
// application catalog, installer or publication tools through this script.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {build} from '../tools/build-stream-capability-pack.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const base=path.join(root,'.cache/capability-upgrade/tex-inputs');
const args=process.argv.slice(2);
if(args.length&&!(args.length===2&&args[0]==='--output-name'&&/^[a-z0-9][a-z0-9-]{1,63}$/.test(args[1])))throw Error('Usage: node ui-tests/prepare-tex-stream-fixture.mjs [--output-name NAME]');
const inventory=path.join(base,'publication-runtime.json');
const raw=fs.readFileSync(inventory),receipt=JSON.parse(raw);
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
if(receipt.kind!=='publication-runtime'||receipt.redistributionApproved!==false||receipt.lockSha256!==sha(fs.readFileSync(path.join(root,'tools/tex-publication-inputs.lock.json')))||receipt.files.length!==34822)throw Error('Prepared pinned input required');
const directory=path.join(base,args[1]||'stream-review-fixture');
fs.mkdirSync(directory); // exclusive: never replace an older fixture/report
const manifestPath=path.join(directory,'fixture.lock.json');
fs.writeFileSync(manifestPath,JSON.stringify({schema:2,id:'tex-review-fixture',version:'2026.10',platform:'windows-x64',files:receipt.files,review:{redistributionApproved:false,scope:'Explicit native archive QA only; no application installation or publication.',runtimeInventorySha256:sha(raw)}}),{flag:'wx'});
const result=await build({manifestPath,runtime:path.join(base,'publication-runtime'),output:path.join(directory,'fixture.readmd-pack'),writeLock:true,onProgress:progress=>console.log(JSON.stringify(progress))});
const report={...result,redistributionApproved:false,applicationCatalogChanged:false,inventorySha256:sha(raw),manifestSha256:sha(fs.readFileSync(manifestPath))};
fs.writeFileSync(path.join(directory,'preparation.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});
console.log(JSON.stringify(report));
