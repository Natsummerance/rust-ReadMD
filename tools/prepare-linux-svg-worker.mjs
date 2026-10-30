// Explicit network preparation, followed by a separate locked/offline build.
// Upstream prebuilt binaries contain build paths, so build the pinned source.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import crypto from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {nativeBuildEnvironment} from './native-build-env.mjs';

export function validateResvgInventory(names,types){
 const prefix='resvg-0.48.1/';
 const documentedReadme=/^l.* resvg-0\.48\.1\/crates\/resvg\/README\.md -> \.\.\/\.\.\/README\.md$/;
 if(!names.length||names.length!==types.length||names.length>20000||names.some(name=>!name.startsWith(prefix)||name.split('/').includes('..')||name.includes('\\'))||types.some(line=>!/^[-d]/.test(line)&&!documentedReadme.test(line)))throw Error('Unsafe SVG source inventory');
}
async function main(){
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const action=process.argv[2];
if(!['prepare','build','clean'].includes(action)||process.argv.length!==3)throw Error('Expected prepare, build or clean');
if(process.platform!=='linux'||process.arch!=='x64')throw Error('The SVG Linux worker must be built and exercised on Linux x64');
const directory=path.join(root,'.cache/readmd-linux-svg-worker'),marker=path.join(directory,'.readmd-owned');
const ownership='readmd-linux-svg-worker-v1\n',version='0.48.1',source=path.join(directory,'resvg-'+version),manifest=path.join(source,'Cargo.toml');
const sourceSha256='13ed5a2bae7a01156288ecae5bf944cf7d1c572742c19fc68027947a4d87294c';
const archive=path.join(directory,'resvg-'+version+'.tar.xz');
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
function ordinaryParents(file){
 for(let at=file;at!==path.dirname(at);at=path.dirname(at))if(fs.existsSync(at)&&fs.lstatSync(at).isSymbolicLink())throw Error('SVG build paths cannot traverse symlinks');
}
ordinaryParents(directory);
if(fs.existsSync(directory)&&fs.readFileSync(marker,'utf8')!==ownership)throw Error('SVG build directory is not owned');
if(action==='clean'){
 if(fs.existsSync(directory))fs.rmSync(directory,{recursive:true});
 console.log('Owned SVG source preparation removed');
 process.exit(0);
}
if(!fs.existsSync(directory)){fs.mkdirSync(directory,{recursive:true});fs.writeFileSync(marker,ownership,{flag:'wx'});}
const env=nativeBuildEnvironment({...process.env,CARGO_INCREMENTAL:'0',CARGO_TARGET_DIR:path.resolve(process.env.CARGO_TARGET_DIR||path.join(root,'rust/target'))},root);
function run(program,args){
 const child=spawnSync(program,args,{cwd:root,env,encoding:'utf8',timeout:600000,maxBuffer:32*1024*1024});
 if(child.status!==0){
  const detail=String(child.stderr||child.error?.code||'').replaceAll(root,'<workspace>').replaceAll(os.homedir(),'<build-home>').slice(-5000);
  throw Error(path.basename(program)+' failed: '+detail);
 }
 return child.stdout;
}
if(action==='prepare'){
 if(!fs.existsSync(archive)){
  const response=await fetch('https://github.com/linebender/resvg/releases/download/v'+version+'/resvg-'+version+'.tar.xz',{signal:AbortSignal.timeout(60000)});
  if(!response.ok)throw Error('SVG source download HTTP '+response.status);
  const chunks=[];let count=0;
  for await(const chunk of response.body){count+=chunk.length;if(count>16*1024*1024)throw Error('SVG source archive exceeds its budget');chunks.push(chunk);}
  const bytes=Buffer.concat(chunks);if(sha(bytes)!==sourceSha256)throw Error('SVG source archive digest mismatch');fs.writeFileSync(archive,bytes,{flag:'wx'});
 }
 if(sha(fs.readFileSync(archive))!==sourceSha256)throw Error('SVG source archive changed');
 if(!fs.existsSync(source)){
  const names=run('tar',['-tJf',archive]).trim().split('\n');
  const types=run('tar',['-tvJf',archive]).trim().split('\n');
  validateResvgInventory(names,types);
  // The sole documented README link is unnecessary for building. Copy the
  // verified root README as an ordinary file instead of materializing a link.
  run('tar',['-xJf',archive,'-C',directory,'--exclude=resvg-'+version+'/crates/resvg/README.md','--no-same-owner','--no-same-permissions','--keep-old-files']);
  fs.copyFileSync(path.join(source,'README.md'),path.join(source,'crates/resvg/README.md'),fs.constants.COPYFILE_EXCL);
 }
 const identity={sourceSha256,manifest:sha(fs.readFileSync(manifest)),cargoLock:sha(fs.readFileSync(path.join(source,'Cargo.lock')))};
 const receipt=path.join(directory,'source-identity.json');
 if(fs.existsSync(receipt)&&fs.readFileSync(receipt,'utf8')!==JSON.stringify(identity))throw Error('Prepared SVG source changed');
 fs.writeFileSync(receipt,JSON.stringify(identity));
 run('cargo',['fetch','--locked','--manifest-path',manifest]);
 console.log('resvg '+version+' source verified; locked dependencies prepared');
}else{
 const identity=JSON.parse(fs.readFileSync(path.join(directory,'source-identity.json'),'utf8'));
 if(identity.sourceSha256!==sourceSha256||identity.manifest!==sha(fs.readFileSync(manifest))||identity.cargoLock!==sha(fs.readFileSync(path.join(source,'Cargo.lock'))))throw Error('Prepared SVG source changed');
 const common=['--offline','--locked','--manifest-path',manifest];
 const target='x86_64-unknown-linux-gnu';
 run('cargo',['build',...common,'--profile','production','-p','resvg','--bin','resvg','--target',target]);
 const metadata=path.join(directory,'metadata.json'),dependencies=path.join(directory,'dependencies.txt');
 fs.writeFileSync(metadata,run('cargo',['metadata',...common,'--format-version','1','--filter-platform',target]));
 fs.writeFileSync(dependencies,run('cargo',['tree',...common,'-p','resvg','--target',target,'--edges','normal,build','--prefix','none']));
 run(process.execPath,[path.join(root,'tools/build-svg-renderer.mjs'),source,path.join(env.CARGO_TARGET_DIR,target,'production/resvg'),metadata,dependencies,path.join(root,'assets/engines/resvg/linux-x64'),'linux-x64']);
 console.log('Linux x64 SVG worker built offline, inspected and packaged with license notices');
}
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url))await main();
