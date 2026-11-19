// Import the pinned Office layout source, then build the linked native worker.
// Network preparation and the locked/offline compilation are separate steps.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import crypto from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {nativeBuildEnvironment} from './native-build-env.mjs';
import {officeDependencyNoticePlan,packageOfficeRuntime} from './build-office-runtime.mjs';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const pinFile=path.join(root,'tools/office/third-party/source.lock.json');
const pins=JSON.parse(fs.readFileSync(pinFile,'utf8'));
const directory=path.join(root,'.cache/readmd-office-source');
const source=path.join(directory,'upstream');
const marker='readmd-office-source-v1\n';
const platforms={
  'macos-arm64':'aarch64-apple-darwin',
  'macos-x64':'x86_64-apple-darwin',
  'windows-x64':'x86_64-pc-windows-msvc',
  'linux-x64':'x86_64-unknown-linux-gnu',
};
const hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const regular=file=>fs.lstatSync(file).isFile();
function run(program,args,{cwd=root,env=process.env,capture=true}={}) {
  const child=spawnSync(program,args,{cwd,env,encoding:'utf8',stdio:capture?'pipe':'inherit',timeout:1800000,maxBuffer:32*1024*1024});
  if(child.status!==0)throw Error(path.basename(program)+' failed: '+String(child.stderr||child.error?.message||'').replaceAll(root,'<workspace>').replaceAll(os.homedir(),'<build-home>').slice(-10000));
  return child.stdout||'';
}
function ownedPath(value) {
  const full=path.resolve(value);
  if(!full.startsWith(root+path.sep))throw Error('Office build paths must stay inside this workspace');
  for(let at=full;at!==path.dirname(at);at=path.dirname(at))
    if(fs.existsSync(at)&&fs.lstatSync(at).isSymbolicLink())throw Error('Office build paths cannot traverse symlinks');
  return full;
}
function walk(base) {
  const files=[];
  const visit=dir=>{
    for(const entry of fs.readdirSync(dir,{withFileTypes:true}).sort((a,b)=>a.name.localeCompare(b.name,'en'))) {
      const file=path.join(dir,entry.name);
      if(entry.isDirectory())visit(file);
      else if(entry.isFile())files.push({path:path.relative(base,file).split(path.sep).join('/'),size:fs.statSync(file).size,sha256:hash(fs.readFileSync(file))});
      else throw Error('Imported Office source contains a non-regular file');
    }
  };
  visit(base);return files;
}
function verifyIdentity({checkAdapters=true}={}) {
  const receipt=JSON.parse(fs.readFileSync(path.join(directory,'source-identity.json'),'utf8'));
  if(receipt.commit!==pins.commit||receipt.pinSha256!==hash(fs.readFileSync(pinFile))
    ||JSON.stringify(walk(source))!==JSON.stringify(receipt.files))throw Error('Prepared Office source changed');
  if(checkAdapters)for(const file of ['worker-main.rs','source-bridge.rs'])
    if(receipt.adapters[file]!==hash(fs.readFileSync(path.join(root,'tools/office',file))))throw Error('Office adapter changed; run prepare before rebuilding');
  return receipt;
}
function refreshAdapters(receipt) {
  const adapters={};
  for(const [name,destination] of [['worker-main.rs','src/readmd_worker.rs'],['source-bridge.rs','src/readmd_embed.rs']]) {
    const bytes=fs.readFileSync(path.join(root,'tools/office',name));
    const target=path.join(source,'crates/ibis-office-app',destination);
    adapters[name]=hash(bytes);
    if(hash(fs.readFileSync(target))!==adapters[name])fs.writeFileSync(target,bytes);
  }
  receipt.adapters=adapters;receipt.files=walk(source);
  fs.writeFileSync(path.join(directory,'source-identity.json'),JSON.stringify(receipt,null,2)+'\n');
  return receipt;
}
export function validateOfficeArchive(names,types,prefix) {
  if(!names.length||names.length>20000||names.length!==types.length
    ||new Set(names).size!==names.length
    ||names.some(name=>!name.startsWith(prefix)||name.includes('\\')||name.split('/').some(part=>part==='..'||part==='.'))
    ||types.some(line=>!/^[d-]/.test(line)))throw Error('Unsafe Office fork archive');
}
async function importSource(localSource,archiveDirectory) {
  let checkout;
  if(localSource)checkout=ownedPath(localSource);
  else {
    checkout=path.join(directory,'checkout');
    if(!fs.existsSync(checkout)) {
      run('git',['clone','-c','core.autocrlf=false','--filter=blob:none','--no-checkout','--depth','1','--branch',pins.tag,pins.repository,checkout]);
      run('git',['-C',checkout,'sparse-checkout','set','--no-cone','/crates/','/tools/ibis-bench/','/Cargo.toml','/Cargo.lock','/.gitmodules','/version.txt','/LICENSE','/NOTICE','/COMMERCIAL-LICENSE.md','/docs/TRADEMARK.md','/docs/THIRD-PARTY-NOTICES.md']);
      run('git',['-C',checkout,'checkout','--detach',pins.commit]);
    }
  }
  if(run('git',['-C',checkout,'rev-parse','HEAD']).trim()!==pins.commit)throw Error('Office source commit differs from its pin');
  // Validate both the git blob identity and the independent SHA-256 pins.
  // Copy only the build closure; no repository, large benchmark corpus or app.
  const closure=name=>Object.hasOwn(pins.files,name)||name==='COMMERCIAL-LICENSE.md'||name.startsWith('crates/')||name.startsWith('tools/ibis-bench/');
  const entries=run('git',['-C',checkout,'ls-tree','-rz',pins.commit]).split('\0').filter(Boolean);
  for(const fork of pins.forks)
    if(!entries.includes('160000 commit '+fork.commit+'\tforks/'+fork.repository))throw Error('Office fork pin differs from the main source gitlink');
  fs.mkdirSync(source);
  for(const entry of entries) {
    const match=/^(\d+) (\w+) ([a-f0-9]{40})\t(.+)$/.exec(entry);
    if(!match||!closure(match[4]))continue;
    const [,mode,type,oid,name]=match;
    if(type!=='blob'||!['100644','100755'].includes(mode)||name.split('/').some(part=>part==='..'||part==='.'))throw Error('Unsupported Office source entry');
    const file=path.join(checkout,name);
    if(!regular(file))throw Error('Office source file is not regular: '+name);
    const bytes=fs.readFileSync(file);
    const gitHash=crypto.createHash('sha1').update('blob '+bytes.length+'\0').update(bytes).digest('hex');
    if(gitHash!==oid||(pins.files[name]&&hash(bytes)!==pins.files[name]))throw Error('Office source file differs from its pin: '+name);
    const dest=path.join(source,name);fs.mkdirSync(path.dirname(dest),{recursive:true});fs.writeFileSync(dest,bytes,{flag:'wx',mode:mode==='100755'?0o755:0o644});
  }
  for(const name of Object.keys(pins.files))if(!fs.existsSync(path.join(source,name)))throw Error('Required Office source file missing: '+name);
  fs.mkdirSync(path.join(source,'forks'));
  fs.mkdirSync(path.join(directory,'archives'),{recursive:true});
  for(const fork of pins.forks) {
    const name=fork.repository+'-'+fork.commit+'.tar.gz';
    const archive=path.join(directory,'archives',name);
    if(!fs.existsSync(archive)) {
      let bytes;
      if(archiveDirectory)bytes=fs.readFileSync(path.join(ownedPath(archiveDirectory),name));
      else {
        const response=await fetch('https://codeload.github.com/ibisjp/'+fork.repository+'/tar.gz/'+fork.commit,{signal:AbortSignal.timeout(120000)});
        if(!response.ok)throw Error('Office fork download HTTP '+response.status);
        const chunks=[];let count=0;
        for await(const chunk of response.body){count+=chunk.length;if(count>fork.size)throw Error('Office fork archive exceeded its pin');chunks.push(chunk);}
        bytes=Buffer.concat(chunks);
      }
      if(bytes.length!==fork.size||hash(bytes)!==fork.sha256)throw Error('Office fork archive differs from its pin');
      fs.writeFileSync(archive,bytes,{flag:'wx'});
    }
    const bytes=fs.readFileSync(archive);
    if(bytes.length!==fork.size||hash(bytes)!==fork.sha256)throw Error('Office fork archive changed');
    const prefix=fork.repository+'-'+fork.commit+'/';
    // GNU tar from Git Bash interprets Windows drive letters in -f as a
    // remote archive. Run from the archive directory with a relative name.
    const archiveOptions={cwd:path.dirname(archive)};
    validateOfficeArchive(run('tar',['-tzf',name],archiveOptions).trim().split('\n'),run('tar',['-tvzf',name],archiveOptions).trim().split('\n'),prefix);
    const dest=path.join(source,'forks',fork.repository);fs.mkdirSync(dest);
    const relativeArchive=path.relative(dest,archive).split(path.sep).join('/');
    run('tar',['-xzf',relativeArchive,'--strip-components=1'],{cwd:dest});
  }
  const crate=path.join(source,'crates/ibis-office-app');
  fs.appendFileSync(path.join(crate,'Cargo.toml'),'\n[[bin]]\nname = "readmd-office-render"\npath = "src/readmd_worker.rs"\n');
  fs.appendFileSync(path.join(crate,'src/lib.rs'),'\n// ReadMD narrow headless source integration.\n#[cfg(headless_export)]\npub mod readmd_embed;\n');
  fs.copyFileSync(path.join(root,'tools/office/source-bridge.rs'),path.join(crate,'src/readmd_embed.rs'));
  fs.copyFileSync(path.join(root,'tools/office/worker-main.rs'),path.join(crate,'src/readmd_worker.rs'));
  const adapters=Object.fromEntries(['worker-main.rs','source-bridge.rs'].map(name=>[name,hash(fs.readFileSync(path.join(root,'tools/office',name)))]));
  const receipt={schema:1,commit:pins.commit,pinSha256:hash(fs.readFileSync(pinFile)),adapters,files:walk(source)};
  fs.writeFileSync(path.join(directory,'source-identity.json'),JSON.stringify(receipt,null,2)+'\n',{flag:'wx'});
  return receipt;
}
function seedCargoCache(previous,cargoHome) {
  if(!previous||!fs.existsSync(path.join(previous,'registry')))return;
  const packages=[...fs.readFileSync(path.join(source,'Cargo.lock'),'utf8').matchAll(/\[\[package\]\]\nname = "([^"]+)"\nversion = "([^"]+)"\nsource = "registry[^"]+"\nchecksum = "([^"]+)"/g)];
  const copy=(from,to)=>{if(fs.existsSync(from)&&regular(from)&&!fs.existsSync(to)){fs.mkdirSync(path.dirname(to),{recursive:true});fs.copyFileSync(from,to);}};
  const registries=path.join(previous,'registry/cache');
  if(!fs.existsSync(registries))return;
  for(const registry of fs.readdirSync(registries,{withFileTypes:true}).filter(e=>e.isDirectory()).map(e=>e.name)) {
    // A fresh private Cargo home uses crates.io, even if the shared cache uses
    // an alternate registry. Never write through a link into another project.
    if(!registry.startsWith('index.crates.io-'))continue;
    copy(path.join(previous,'registry/index',registry,'config.json'),path.join(cargoHome,'registry/index',registry,'config.json'));
    for(const [,name,version,checksum] of packages) {
      const archive=path.join(registries,registry,name+'-'+version+'.crate');
      if(fs.existsSync(archive)&&hash(fs.readFileSync(archive))===checksum)copy(archive,path.join(cargoHome,'registry/cache',registry,path.basename(archive)));
      const key=name.length===1?'1/'+name:name.length===2?'2/'+name:name.length===3?'3/'+name[0]+'/'+name:name.slice(0,2)+'/'+name.slice(2,4)+'/'+name;
      copy(path.join(previous,'registry/index',registry,'.cache',key),path.join(cargoHome,'registry/index',registry,'.cache',key));
    }
  }
}
async function main() {
  const args=process.argv.slice(2),action=args.shift(),platform=args.shift();
  if(!['prepare-source','prepare','build','clean'].includes(action)||!Object.hasOwn(platforms,platform)||args.length%2)throw Error('Usage: prepare-office-runtime.mjs prepare-source|prepare|build|clean PLATFORM [--source WORKSPACE_CHECKOUT] [--archives WORKSPACE_DIR] [--output WORKSPACE_DIR]');
  const options={};
  for(let i=0;i<args.length;i+=2){if(!['--source','--archives','--output'].includes(args[i])||options[args[i]]||!args[i+1])throw Error('Invalid Office build option');options[args[i]]=args[i+1];}
  ownedPath(directory);
  if(fs.existsSync(directory)&&fs.readFileSync(path.join(directory,'.readmd-owned'),'utf8')!==marker)throw Error('Office build directory is not owned');
  if(action==='clean'){if(fs.existsSync(directory))fs.rmSync(directory,{recursive:true});return;}
  if(!fs.existsSync(directory)){fs.mkdirSync(directory,{recursive:true});fs.writeFileSync(path.join(directory,'.readmd-owned'),marker,{flag:'wx'});}
  if(!fs.existsSync(path.join(directory,'source-identity.json'))) {
    if(fs.existsSync(source))throw Error('Interrupted Office import; clean only the owned preparation and retry');
    if(action==='build')throw Error('Prepare the pinned Office source and dependencies before building offline');
    await importSource(options['--source'],options['--archives']);
  }
  let identity=verifyIdentity({checkAdapters:action==='build'});
  if(action!=='build')identity=refreshAdapters(identity);
  if(action==='prepare-source'){console.log(JSON.stringify({sourceImported:true,commit:pins.commit,files:identity.files.length}));return;}
  const cargoHome=path.join(directory,'cargo');
  const sharedMacTarget=path.resolve(root,'../../cargo-target');
  const targetDirectory=path.resolve(process.env.CARGO_TARGET_DIR||(process.platform==='darwin'&&fs.existsSync(sharedMacTarget)?fs.realpathSync(sharedMacTarget):path.join(root,'rust/target')));
  const env=nativeBuildEnvironment({...process.env,CARGO_HOME:cargoHome,CARGO_TARGET_DIR:targetDirectory,CARGO_INCREMENTAL:'0',CARGO_BUILD_JOBS:'2'},root);
  const target=platforms[platform],manifest=path.join(source,'Cargo.toml');
  if(action==='prepare') {
    seedCargoCache(process.env.CARGO_HOME||path.join(os.homedir(),'.cargo'),cargoHome);
    run('cargo',['fetch','--locked','--manifest-path',manifest,'--target',target],{cwd:source,env,capture:false});
    console.log('Pinned Office layout source imported; locked '+platform+' dependencies prepared');
  } else {
    if((platform.startsWith('macos-')&&process.platform!=='darwin')||(platform==='linux-x64'&&(process.platform!=='linux'||process.arch!=='x64'))||(platform==='windows-x64'&&(process.platform!=='win32'||process.arch!=='x64')))throw Error('Build the Office worker on its native desktop platform');
    const common=['--offline','--locked','--manifest-path',manifest];
    const metadata=JSON.parse(run('cargo',['metadata',...common,'--format-version','1','--filter-platform',target],{cwd:source,env}));
    const dependencies=run('cargo',['tree',...common,'-p','ibis-office-app','--target',target,'--edges','normal,build','--prefix','none'],{cwd:source,env});
    // Check the target's complete source notices before the expensive offline build.
    officeDependencyNoticePlan({root,source,metadata,dependencies});
    run('cargo',['build',...common,'--release','-p','ibis-office-app','--bin','readmd-office-render','--target',target,'--jobs','2'],{cwd:source,env,capture:false});
    const binary=path.join(targetDirectory,target,'release','readmd-office-render'+(platform==='windows-x64'?'.exe':''));
    const output=ownedPath(options['--output']||path.join(root,'build/office-runtime',platform));
    console.log(JSON.stringify(packageOfficeRuntime({root,source,binary,metadata,dependencies,identity,output,platform})));
  }
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url))await main();
