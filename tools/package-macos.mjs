// Native packaging of locked build outputs. No downloads or account setup.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {zip} from './lib/zip.mjs';
import {signTarget,signingOptions,notarizeTarget} from './macos-sign.mjs';
import {scanBuffer} from './privacy-scan.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
function command(program,args,options={}) {
  const result=spawnSync(program,args,{...options,maxBuffer:64*1024*1024});
  if(result.error||result.status!==0)throw Error(path.basename(program)+' failed while packaging (exit '+result.status+', signal '+(result.signal||'none')+'; inspect the owned inputs locally)');
  return result.stdout;
}
function archOf(binary){return command('/usr/bin/lipo',['-archs',binary],{encoding:'utf8'}).trim();}
function preparePet(input,destination,arch,temporary,env) {
  const member=name=>command('/usr/bin/unzip',['-p',input,name]);
  const names=command('/usr/bin/unzip',['-Z1',input],{encoding:'utf8'}).trim().split('\n');
  const manifest=JSON.parse(member('runtime-manifest.json'));
  if(manifest.runtime!=='readmd-pet-rust'||manifest.platform!=='macos'||manifest.arch!==(arch==='arm64'?'aarch64':'x86_64'))throw Error('Pet runtime platform mismatch');
  if(!Array.isArray(manifest.artifacts)||manifest.artifacts.length>2000||names.length!==manifest.artifacts.length+1||new Set(names).size!==names.length)throw Error('Pet package inventory mismatch');
  const expected=new Set(['runtime-manifest.json']),entries=[];
  let executable=0;
  for(const artifact of manifest.artifacts) {
    const name=artifact.path;
    if(typeof name!=='string'||/[\\:\0\r\n*?\[\]]/.test(name)||name.startsWith('/')||name.split('/').some(p=>!p||p==='.'||p==='..')||expected.has(name)||!names.includes(name))throw Error('Unsafe pet package path');
    expected.add(name);
    let data=member(name);
    if(data.length!==artifact.size||hash(data)!==artifact.sha256)throw Error('Pet input integrity mismatch');
    if(scanBuffer(name,data,{sourceTree:false}).length)throw Error('Pet input privacy check failed');
    if(artifact.role==='executable') {
      if(name!=='readmd-pet-rust')throw Error('Unexpected pet executable');
      executable++;
      const binary=path.join(temporary,name);
      fs.writeFileSync(binary,data,{flag:'wx',mode:0o755});
      if(archOf(binary)!==(arch==='arm64'?'arm64':'x86_64'))throw Error('Pet executable architecture mismatch');
      signTarget(binary,{env});data=fs.readFileSync(binary);
      artifact.size=data.length;artifact.sha256=hash(data);fs.unlinkSync(binary);
    }
    entries.push({name,data,mode:artifact.role==='executable'||name.endsWith('.sh')?0o755:0o644});
  }
  if(executable!==1||expected.size!==names.length)throw Error('Pet executable inventory mismatch');
  entries.push({name:'runtime-manifest.json',data:JSON.stringify(manifest,null,2)+'\n'});
  fs.writeFileSync(destination,zip(entries),{flag:'wx'});
  return {sourceArchiveSha256:hash(fs.readFileSync(input)),archiveSha256:hash(fs.readFileSync(destination)),artifacts:manifest.artifacts.length,signedExecutables:executable};
}
export function packageMacos({binary,pet,output,arch,format='all',env=process.env}) {
  if(process.platform!=='darwin')throw Error('A native Mac is required');
  if(!['arm64','x64'].includes(arch))throw Error('Expected arm64 or x64');
  if(!['all','zip'].includes(format))throw Error('Expected all or zip package format');
  for(const file of [binary,pet])if(!file||!fs.lstatSync(file).isFile())throw Error('Expected a regular local input file');
  if(archOf(binary)!==(arch==='arm64'?'arm64':'x86_64'))throw Error('Main executable architecture mismatch');
  const version=fs.readFileSync(path.join(root,'VERSION'),'utf8').trim();
  if(!/^\d+\.\d+\.\d+$/.test(version))throw Error('Invalid version');
  const mode=signingOptions(env).mode;
  output=path.resolve(output);fs.mkdirSync(output); // Never replace an installation.
  const stage=path.join(output,'image'),app=path.join(stage,'ReadMD.app'),contents=path.join(app,'Contents');
  const temporary=fs.mkdtempSync(path.join(output,'.package-'));
  let completed=false;
  try {
    fs.mkdirSync(path.join(contents,'MacOS'),{recursive:true});fs.mkdirSync(path.join(contents,'Resources'));
    const executable=path.join(contents,'MacOS/ReadMD');
    fs.copyFileSync(binary,executable);fs.chmodSync(executable,0o755);
    fs.cpSync(path.join(root,'assets'),path.join(contents,'Resources/assets'),{recursive:true});
    const icon=['ReadMD.icns','readmd.icns'].map(n=>path.join(root,'assets',n)).find(n=>fs.existsSync(n));
    if(icon)fs.copyFileSync(icon,path.join(contents,'Resources/readmd.icns'));
    const petName='ReadMD-Pet-Rust-macos-'+(arch==='arm64'?'aarch64':'x86_64')+'.zip';
    const petReport=preparePet(pet,path.join(contents,'Resources',petName),arch,temporary,env);
    fs.copyFileSync(path.join(root,'release/macos-install.md'),path.join(contents,'Resources/MACOS-INSTALL.md'));
    fs.writeFileSync(path.join(contents,'Info.plist'),`<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>ReadMD</string>
<key>CFBundleDisplayName</key><string>ReadMD</string>
<key>CFBundleIdentifier</key><string>asia.readmd.desktop</string>
<key>CFBundleVersion</key><string>${version}</string>
<key>CFBundleShortVersionString</key><string>${version}</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleExecutable</key><string>ReadMD</string>
<key>CFBundleIconFile</key><string>readmd.icns</string>
<key>LSMinimumSystemVersion</key><string>${arch==='arm64'?'11.0':'10.15'}</string>
<key>NSHighResolutionCapable</key><true/>
<key>CFBundleDocumentTypes</key><array><dict>
<key>CFBundleTypeName</key><string>Markdown</string>
<key>CFBundleTypeRole</key><string>Editor</string>
<key>LSHandlerRank</key><string>Alternate</string>
<key>LSItemContentTypes</key><array><string>net.daringfireball.markdown</string></array>
<key>CFBundleTypeExtensions</key><array><string>md</string><string>markdown</string><string>mdown</string><string>mkd</string></array>
</dict></array>
<key>UTImportedTypeDeclarations</key><array><dict>
<key>UTTypeIdentifier</key><string>net.daringfireball.markdown</string>
<key>UTTypeDescription</key><string>Markdown</string>
<key>UTTypeConformsTo</key><array><string>public.plain-text</string></array>
<key>UTTypeTagSpecification</key><dict>
<key>public.filename-extension</key><array><string>md</string><string>markdown</string><string>mdown</string><string>mkd</string></array>
<key>public.mime-type</key><string>text/markdown</string>
</dict></dict></array>
</dict></plist>
`);
    const signing=signTarget(app,{env});
    if(mode==='distribution')Object.assign(signing,notarizeTarget(app,{env}));
    command(process.execPath,[path.join(root,'tools/privacy-scan.mjs'),app]);
    fs.copyFileSync(path.join(root,'release/macos-install.md'),path.join(stage,'安装说明 INSTALL.md'));
    fs.symlinkSync('/Applications',path.join(stage,'Applications'));
    const zipFile=path.join(output,'ReadMD-macos-'+arch+'.zip'),dmg=path.join(output,'ReadMD-macos-'+arch+'.dmg');
    command('/usr/bin/ditto',['-c','-k','--sequesterRsrc','--keepParent',app,zipFile]);
    const files=[zipFile];
    if(format==='all'){
      command('/usr/bin/hdiutil',['create','-volname','ReadMD','-srcfolder',stage,'-format','UDZO',dmg]);
      signTarget(dmg,{env});if(mode==='distribution')notarizeTarget(dmg,{env});files.push(dmg);
    }
    const report={version,arch,formats:format==='zip'?['zip']:['zip','dmg'],signing,sourceBinarySha256:hash(fs.readFileSync(binary)),binarySha256:hash(fs.readFileSync(executable)),bootSha256:hash(fs.readFileSync(path.join(contents,'Resources/assets/readmd.boot.js'))),pet:petReport,files:files.map(file=>({name:path.basename(file),size:fs.statSync(file).size,sha256:hash(fs.readFileSync(file))}))};
    fs.writeFileSync(path.join(output,'package.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});
    completed=true;return report;
  } finally {
    fs.rmSync(temporary,{recursive:true});
    if(!completed)fs.writeFileSync(path.join(output,'INCOMPLETE'),'Packaging failed; do not distribute.\n',{flag:'wx'});
  }
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)) {
  const args=process.argv.slice(2),value=name=>args[args.indexOf(name)+1];
  const keys=args.filter((_,i)=>i%2===0),allowed=['--binary','--pet','--output','--arch','--format'];
  if(![8,10].includes(args.length)||new Set(keys).size!==keys.length||keys.some(k=>!allowed.includes(k))||!allowed.slice(0,4).every(n=>keys.includes(n)))throw Error('Usage: package-macos.mjs --binary FILE --pet ZIP --output NEW_DIRECTORY --arch arm64|x64 [--format all|zip]');
  console.log(JSON.stringify(packageMacos({binary:value('--binary'),pet:value('--pet'),output:value('--output'),arch:value('--arch'),format:keys.includes('--format')?value('--format'):'all'})));
}
