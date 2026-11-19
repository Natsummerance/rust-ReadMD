// Package the source-linked Office layout worker; no downloads or install.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {scanNativePaths} from './privacy-scan.mjs';
import {stageWindowsRuntime} from './stage-windows-runtime.mjs';

const hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
function run(program,args) {
  const child=spawnSync(program,args,{encoding:'utf8',timeout:15000,maxBuffer:8*1024*1024});
  if(child.status!==0)throw Error(path.basename(program)+' failed during Office runtime inspection');
  return child.stdout.trim();
}
function filesUnder(base) {
  const files=[];
  function visit(dir) {
    for(const item of fs.readdirSync(dir,{withFileTypes:true}).sort((a,b)=>a.name.localeCompare(b.name,'en'))) {
      const file=path.join(dir,item.name);
      if(item.isDirectory())visit(file);
      else if(item.isFile())files.push(file);
      else throw Error('Office runtime contains an alias or special file');
    }
  }
  visit(base);return files;
}
export function packageOfficeRuntime({root,source,binary,metadata,dependencies,identity,output,platform}) {
  const pinFile=path.join(root,'tools/office/third-party/source.lock.json');
  const pins=JSON.parse(fs.readFileSync(pinFile,'utf8'));
  if(identity.commit!==pins.commit||identity.pinSha256!==hash(fs.readFileSync(pinFile)))throw Error('Unreviewed Office source identity');
  const engine=fs.readFileSync(binary);
  if(engine.length<4096||engine.length>256*1024*1024)throw Error('Unexpected Office worker size');
  const failures=scanNativePaths(path.basename(binary),engine);
  if(failures.length)throw Error('Office worker privacy check failed: '+[...new Set(failures)].join('; '));
  let nativeLibraries=[];
  if(platform.startsWith('macos-')) {
    const arch=platform==='macos-arm64'?'arm64':'x86_64';
    if(process.platform!=='darwin'||run('/usr/bin/lipo',['-archs',binary])!==arch)throw Error('Office worker architecture mismatch');
    nativeLibraries=run('/usr/bin/otool',['-L',binary]).split('\n').slice(1).map(line=>line.trim().split(' (')[0]);
    if(nativeLibraries.some(lib=>!lib.startsWith('/usr/lib/')&&!lib.startsWith('/System/Library/')))throw Error('Office worker links an unbundled native library');
  } else if(platform==='linux-x64') {
    if(engine.subarray(0,6).toString('hex')!=='7f454c460201'||engine.readUInt16LE(18)!==62)throw Error('Office worker architecture mismatch');
    nativeLibraries=run('ldd',[binary]).split('\n').map(line=>line.trim());
    if(nativeLibraries.some(line=>line.includes('not found')))throw Error('Office worker has missing native libraries');
    nativeLibraries=nativeLibraries.map(line=>line.split(/\s+=>\s+|\s+\(/)[0]).filter(Boolean);
  } else if(platform==='windows-x64') {
    const pe=engine.readUInt32LE(0x3c);
    if(engine.subarray(0,2).toString()!=='MZ'||pe+6>engine.length||engine.subarray(pe,pe+4).toString('hex')!=='50450000'||engine.readUInt16LE(pe+4)!==0x8664)throw Error('Office worker architecture mismatch');
  } else throw Error('Unsupported managed Office platform');
  const expected='readmd-office-render office-layout 0.8.6 '+pins.commit;
  const versionExecuted=platform!=='macos-x64'||process.arch==='x64';
  if(versionExecuted?run(binary,['--version'])!==expected:!engine.includes(Buffer.from(expected)))throw Error('Office worker provenance mismatch');
  fs.mkdirSync(path.dirname(output),{recursive:true});fs.mkdirSync(output);
  const copy=(input,name)=>{
    if(!fs.lstatSync(input).isFile())throw Error('Office package input is not a regular file');
    const target=path.join(output,name);fs.mkdirSync(path.dirname(target),{recursive:true});fs.copyFileSync(input,target,fs.constants.COPYFILE_EXCL);
  };
  const executable='bin/readmd-office-render'+(platform==='windows-x64'?'.exe':'');
  copy(binary,executable);if(platform!=='windows-x64')fs.chmodSync(path.join(output,executable),0o755);
  if(platform==='windows-x64') {
    stageWindowsRuntime(path.join(output,'bin'));
    copy(path.join(root,'assets/runtimes/windows-x64/runtime.lock.json'),'licenses/windows-vc-runtime.lock.json');
  }
  // The upstream loader's first relative location is shared by all native
  // platforms: bin/../Resources/artborders. Carlito and icons are embedded.
  copy(path.join(source,'crates/ibis-office-app/data/artborders/OfficeOpenXML-WordprocessingMLArtBorders.zip'),'Resources/artborders/OfficeOpenXML-WordprocessingMLArtBorders.zip');
  for(const name of ['LICENSE','NOTICE','COMMERCIAL-LICENSE.md','docs/TRADEMARK.md','docs/THIRD-PARTY-NOTICES.md'])copy(path.join(source,name),'licenses/upstream/'+name);
  for(const file of filesUnder(source).filter(file=>/^(?:.*[-_])?(?:LICEN[SC]E|COPYING|NOTICE|UNLICENSE|OFL)(?:[._-].*)?$/i.test(path.basename(file)))) {
    const name=path.relative(source,file).split(path.sep).join('/');
    const dest='licenses/source/'+name;
    if(!fs.existsSync(path.join(output,dest)))copy(file,dest);
  }
  const used=new Set([...dependencies.matchAll(/^([^\s]+) v([^\s]+)/gm)].map(match=>match[1]+'@'+match[2]));
  const packages=metadata.packages.filter(pkg=>used.has(pkg.name+'@'+pkg.version));
  const upstreamNotices=['macos','linux'].map(name=>{
    const file=path.join(source,'crates/ibis-office-app',name,'license.md');
    return {file,path:'licenses/source/crates/ibis-office-app/'+name+'/license.md',text:fs.readFileSync(file,'utf8')};
  });
  const supplemental=JSON.parse(fs.readFileSync(path.join(root,'tools/office/third-party/notices.lock.json'),'utf8'));
  const notices=[];
  for(const pkg of packages) {
    const base=path.dirname(pkg.manifest_path);
    if(base===source||base.startsWith(source+path.sep)) {
      notices.push({name:pkg.name,version:pkg.version,license:pkg.license,source:'pinned-upstream-tree'});continue;
    }
    let files=fs.readdirSync(base).filter(name=>/^(LICEN[SC]E|COPYING|NOTICE|UNLICENSE|OFL)/i.test(name)&&fs.lstatSync(path.join(base,name)).isFile()).map(name=>path.join(base,name));
    const licenseFile=pkg.license_file?path.resolve(base,pkg.license_file):null;
    if(licenseFile&&fs.existsSync(licenseFile)&&!files.includes(licenseFile))files.push(licenseFile);
    if(!files.length) {
      const extra=supplemental.files.filter(item=>item.package===pkg.name&&item.version===pkg.version);
      if(extra.length) {
        const block=fs.readFileSync(path.join(source,'Cargo.lock'),'utf8').split('[[package]]').find(block=>block.startsWith('\nname = "'+pkg.name+'"\nversion = "'+pkg.version+'"\n'));
        const vcs=JSON.parse(fs.readFileSync(path.join(base,'.cargo_vcs_info.json'),'utf8'));
        const names=[];
        for(const item of extra) {
          if(!block?.includes('checksum = "'+item.crateSha256+'"')||vcs.git.sha1!==item.commit)throw Error('Dependency notice source differs from its pin');
          if(item.path.startsWith('/')||item.path.includes('\\')||item.path.split('/').some(part=>!part||part==='..'||part==='.'))throw Error('Unsafe dependency notice path');
          const input=path.join(root,'tools/office/third-party',item.path),bytes=fs.readFileSync(input);
          if(bytes.length!==item.size||hash(bytes)!==item.sha256)
            throw Error('Pinned dependency notice changed: '+item.path+' (expected '+item.size+' bytes, got '+bytes.length+'; preserve original checkout bytes)');
          const name='licenses/third-party/'+item.path;copy(input,name);names.push(name);
        }
        notices.push({name:pkg.name,version:pkg.version,license:pkg.license,notices:names,noticeSource:'pinned-dependency-repository',noticeUrls:extra.map(item=>item.url)});
        continue;
      }
      // Several published workspace crates omit standalone license files.
      // Retain the exact upstream distribution notices, with the exact package
      // and version listed, instead of manufacturing a generic license text.
      const official=upstreamNotices.filter(notice=>notice.text.includes('| '+pkg.name+' | '+pkg.version+' |'));
      if(!official.length)throw Error('Missing dependency license notice: '+pkg.name+' '+pkg.version);
      notices.push({name:pkg.name,version:pkg.version,license:pkg.license,notices:official.map(notice=>notice.path),noticeSource:'pinned-upstream-distribution'});
      continue;
    }
    const names=[];
    for(const file of files) {
      const name='licenses/crates/'+pkg.name+'-'+pkg.version+'/'+path.basename(file);
      if(!fs.existsSync(path.join(output,name)))copy(file,name);
      names.push(name);
    }
    notices.push({name:pkg.name,version:pkg.version,license:pkg.license,notices:names});
  }
  fs.writeFileSync(path.join(output,'source.json'),JSON.stringify({
    schema:1,repository:pins.repository,tag:pins.tag,commit:pins.commit,sourceVersion:pins.sourceVersion,
    sourceLockSha256:identity.pinSha256,cargoLockSha256:hash(fs.readFileSync(path.join(source,'Cargo.lock'))),
    adapters:identity.adapters,forks:pins.forks,sourceFiles:identity.files,
    build:'cargo build --offline --locked --release -p ibis-office-app --bin readmd-office-render --target '+({
      'macos-arm64':'aarch64-apple-darwin','macos-x64':'x86_64-apple-darwin','windows-x64':'x86_64-pc-windows-msvc','linux-x64':'x86_64-unknown-linux-gnu',
    })[platform],
    nativeLibraries,versionExecuted,packages:notices,
  },null,2)+'\n',{flag:'wx'});
  const files=filesUnder(output).map(file=>({path:path.relative(output,file).split(path.sep).join('/'),size:fs.statSync(file).size,sha256:hash(fs.readFileSync(file))}));
  const manifest={schema:1,engine:'office-layout',engine_version:'0.8.6',source_commit:pins.commit,source_version:pins.sourceVersion,platform,executable,sha256:hash(engine),files};
  fs.writeFileSync(path.join(output,'runtime.json'),JSON.stringify(manifest,null,2)+'\n',{flag:'wx'});
  return {platform,sourceCommit:pins.commit,executable,size:engine.length,sha256:manifest.sha256,files:files.length,packages:notices.length};
}
