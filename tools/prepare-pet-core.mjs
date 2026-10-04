// Dependency preparation only. Compilation and packaging never use the network.
// Proprietary Cubism Core is intentionally excluded from the source repository.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
const target=process.env.READMD_PET_CUBISM_CORE;
if(!target)throw Error('READMD_PET_CUBISM_CORE must name the prepared cache file');
const expected='25ae938cb4fe282ce189b357bcc97e603d1e1f7ec78bf04150d401c23cdc792f';
let data;
if(fs.existsSync(target))data=fs.readFileSync(target);
else{
 const response=await fetch('https://cubism.live2d.com/sdk-web/cubismcore/live2dcubismcore.min.js',{signal:AbortSignal.timeout(60000)});
 if(!response.ok)throw Error('Cubism dependency preparation failed: HTTP '+response.status);
 data=Buffer.from(await response.arrayBuffer());
}
if(crypto.createHash('sha256').update(data).digest('hex')!==expected)throw Error('Cubism Core digest mismatch');
fs.mkdirSync(path.dirname(target),{recursive:true});fs.writeFileSync(target,data);
console.log('Pinned Cubism Core dependency cache verified');
