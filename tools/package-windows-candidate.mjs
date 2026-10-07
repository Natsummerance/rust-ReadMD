// Offline, reviewable portable candidate. No installer or registry mutation.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {zip} from './lib/zip.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const binary=path.resolve(process.argv[2]||'');
const output=path.resolve(process.argv[3]||'dist/candidate');
const pet=path.resolve(process.argv[4]||'dist/ReadMD-Pet-Rust.zip');
const version=fs.readFileSync(path.join(root,'VERSION'),'utf8').trim();
if(!binary||!fs.statSync(binary,{throwIfNoEntry:false})?.isFile())throw Error('Provide a built Windows binary');
if(!execFileSync(binary,['--version'],{encoding:'utf8',windowsHide:true}).includes(version))throw Error('Binary version differs from VERSION');
if(!fs.statSync(pet,{throwIfNoEntry:false})?.isFile())throw Error('Provide the verified desktop pet archive');
const entries=[];
const add=(name,file)=>entries.push({name,data:fs.readFileSync(file)});
function walk(directory,prefix){
 for(const entry of fs.readdirSync(directory,{withFileTypes:true}).sort((a,b)=>a.name.localeCompare(b.name,'en'))){
  const file=path.join(directory,entry.name),name=prefix+'/'+entry.name;
  if(entry.isSymbolicLink())throw Error('Candidate assets must not contain symlinks');
  if(entry.isDirectory())walk(file,name);else if(entry.isFile())add(name,file);
 }
}
add('ReadMD.exe',binary);add('ReadMD-Pet-Rust.zip',pet);
for(const name of ['LICENSE','README.md','RELEASE_NOTES.md'])add(name,path.join(root,name));
walk(path.join(root,'assets'),'assets');
fs.mkdirSync(output,{recursive:true});
const name='ReadMD-windows-x64.zip',file=path.join(output,name);
const bytes=zip(entries);fs.writeFileSync(file,bytes);
const sha=crypto.createHash('sha256').update(bytes).digest('hex');
fs.writeFileSync(path.join(output,'candidate.json'),JSON.stringify({version,channel:'candidate',platform:'windows-x64',
 file:name,sha256:sha,files:entries.length,bytes:bytes.length,source_commit:execFileSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'}).trim()},null,2)+'\n');
fs.writeFileSync(path.join(output,'SHA256SUMS.txt'),sha+'  '+name+'\n');
console.log('Prepared '+version+' portable candidate: '+entries.length+' files; '+bytes.length+' bytes.');
