import fs from 'node:fs/promises';
import path from 'node:path';
import crypto from 'node:crypto';
import {fileURLToPath} from 'node:url';
const project=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const source=path.resolve(project,'../../assets');
const target=path.join(project,'entry/src/main/resources/rawfile/kernel-assets');
const files=[];
async function walk(dir,relative='') {
 for(const item of await fs.readdir(dir,{withFileTypes:true})) {
  if(item.name==='upstream'||item.isSymbolicLink())continue;
  const rel=relative?relative+'/'+item.name:item.name, full=path.join(dir,item.name);
  if(item.isDirectory())await walk(full,rel);else if(item.isFile())files.push(rel);
 }
}
await walk(source);files.sort();
const hash=crypto.createHash('sha256');
for(const file of files){hash.update(file);hash.update(await fs.readFile(path.join(source,file)));}
const manifest={version:hash.digest('hex'),files};
if(!process.argv.includes('--check')){
 await fs.mkdir(target,{recursive:true});
 for(const file of files){const dest=path.join(target,file);await fs.mkdir(path.dirname(dest),{recursive:true});await fs.copyFile(path.join(source,file),dest);}
 await fs.writeFile(path.join(project,'entry/src/main/resources/rawfile/kernel-manifest.json'),JSON.stringify(manifest));
}
console.log(JSON.stringify({files:files.length,version:manifest.version,written:!process.argv.includes('--check')}));
