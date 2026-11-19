// Inspect shipped bytes without installing or launching foreign executables.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {PLATFORM_NAMES} from './refresh-macos-release.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const run=(program,args,options={})=>execFileSync(program,args,{maxBuffer:256*1024*1024,...options});
export function inspectPlatformPackages({directory,sourceCommit,buildRunId,scratch}) {
  if(!/^[a-f\d]{40}$/.test(sourceCommit)||!Number.isSafeInteger(buildRunId))throw Error('Bound source and build run required');
  const original=file=>run('git',['show',sourceCommit+':'+file],{cwd:root});
  const assets=[
    'assets/index.html','assets/readmd.boot.js','assets/js/core/lifecycle.js',
    'assets/css/shell.css','assets/css/dialogs.css','assets/css/reader.css',
    'assets/providers/provider-catalog.json','assets/i18n/zh-CN.json',
  ];
  const sourceBytes=new Map(assets.map(file=>[file,original(file)]));
  const expected=new Map(assets.map(file=>[file,sha(sourceBytes.get(file))]));
  const packages=PLATFORM_NAMES.map(name=>{
    const file=path.join(directory,name),info=fs.lstatSync(file);
    if(!info.isFile()||info.isSymbolicLink()||!info.size)throw Error('Expected regular candidate archive');
    return {name,size:info.size,sha256:sha(fs.readFileSync(file)),buildRunId,archiveInspected:false};
  });
  fs.mkdirSync(scratch,{recursive:true});
  if(fs.lstatSync(scratch).isSymbolicLink())throw Error('Scratch cannot be a symlink');
  const temporary=fs.mkdtempSync(path.join(scratch,'platform-inspection-'));
  const checks=[],binaryDigests={};
  function desktop(name,read,binary,kind) {
    const assetSha256={},sourceByteVariants={};
    for(const file of assets){
      const bytes=read(file),source=sourceBytes.get(file);assetSha256[file]=sha(bytes);
      if(bytes.equals(source))sourceByteVariants[file]='exact Git bytes';
      // Git's Windows checkout converts unpinned text to CRLF. This is an
      // exact two-byte-variant oracle, not whitespace normalization: any other
      // byte change fails, and the shipped hash remains independently bound.
      else if(kind==='pe'&&!source.includes(Buffer.from('\r\n'))&&bytes.equals(Buffer.from(source.toString('utf8').replaceAll('\n','\r\n'))))sourceByteVariants[file]='exact Windows CRLF checkout';
      else throw Error('Packaged source asset mismatch: '+name+' / '+file);
    }
    const bytes=read(binary);
    if(kind==='pe'){
      const offset=bytes.length>=64?bytes.readUInt32LE(0x3c):0;
      if(bytes.toString('ascii',0,2)!=='MZ'||offset+6>bytes.length||bytes.readUInt32LE(offset)!==0x4550||bytes.readUInt16LE(offset+4)!==0x8664)throw Error('Expected Windows x64 PE executable');
    }else if(bytes.length<20||bytes.toString('hex',0,4)!=='7f454c46'||bytes[4]!==2||bytes[5]!==1||bytes.readUInt16LE(18)!==62)throw Error('Expected Linux x86_64 ELF executable');
    binaryDigests[name]=sha(bytes);
    Object.assign(packages.find(p=>p.name===name),{archiveInspected:true,sourceBootSha256:expected.get('assets/readmd.boot.js'),bootSha256:assetSha256['assets/readmd.boot.js'],assetSha256,sourceByteVariants,binarySha256:sha(bytes)});
    checks.push({name,sourceAssetsMatch:true,sourceByteVariants,executableArchitecture:kind==='pe'?'PE x86_64':'ELF x86_64',nativeHostCertified:false});
  }
  try {
    const win='ReadMD-windows-x64.zip',winFile=path.join(directory,win);
    desktop(win,file=>run('unzip',['-p',winFile,file]),'ReadMD.exe','pe');
    const installer='ReadMDSetup-windows-x64.exe',installerFile=path.join(directory,installer),seven=process.env.READMD_7Z_PATH||'7z';
    const entries=run(seven,['l','-slt','-ba',installerFile],{encoding:'utf8'}).split(/\r?\n/).filter(line=>line.startsWith('Path = ')).map(line=>line.slice(7));
    const installerRead=file=>{
      const matches=entries.filter(entry=>entry.replaceAll('\\','/').replace(/^\$INSTDIR\//,'')===file);
      if(matches.length!==1)throw Error('Installer member is missing or ambiguous: '+file);
      return run(seven,['x','-so','-spd',installerFile,matches[0]]);
    };
    desktop(installer,installerRead,'ReadMD.exe','pe');
    if(binaryDigests[win]!==binaryDigests[installer])throw Error('Installer executable differs from portable ZIP');
    if(JSON.stringify(packages.find(p=>p.name===win).assetSha256)!==JSON.stringify(packages.find(p=>p.name===installer).assetSha256))throw Error('Installer assets differ from portable ZIP');
    const tar='ReadMD-linux-x86_64.tar.gz';
    desktop(tar,file=>run('tar',['-xOf',path.join(directory,tar),'./'+(file==='ReadMD'?'ReadMD':file)]),'ReadMD','elf');
    const deb='ReadMD-linux-x86_64.deb',debFile=path.join(directory,deb);
    const members=run('ar',['-t',debFile],{encoding:'utf8'}).trim().split(/\r?\n/),data=members.filter(name=>/^data\.tar\.(xz|gz|zst)$/.test(name));
    if(data.length!==1)throw Error('Expected one Debian payload');
    const dataFile=path.join(temporary,data[0]);fs.writeFileSync(dataFile,run('ar',['-p',debFile,data[0]]),{flag:'wx'});
    desktop(deb,file=>run('tar',['-xOf',dataFile,file==='ReadMD'?'./usr/bin/readmd':'./usr/share/readmd/'+file]),'ReadMD','elf');
    if(binaryDigests[tar]!==binaryDigests[deb])throw Error('Debian executable differs from portable tarball');
    const vscode=packages.find(p=>p.name.endsWith('.vsix')),vscodeFile=path.join(directory,vscode.name);
    const pkg=JSON.parse(run('unzip',['-p',vscodeFile,'extension/package.json'],{encoding:'utf8'}));
    if(pkg.version!=='0.0.6'||pkg.repository?.url!==JSON.parse(original('packages/vscode-extension/package.json')).repository.url)throw Error('VSIX version or repository differs');
    for(const name of ['extension/out/extension.js','extension/out/webview.js','extension/media/preview.js','extension/media/diagram-frame.js'])if(!run('unzip',['-p',vscodeFile,name]).length)throw Error('Compiled extension member missing');
    Object.assign(vscode,{archiveInspected:true,integrationFilesVerified:true});
    const vscodeMembers=run('unzip',['-Z1',vscodeFile],{encoding:'utf8'}).trim().split(/\r?\n/);
    checks.push({name:vscode.name,version:pkg.version,compiledIntegrationPresent:true,containsDesktopKernel:vscodeMembers.some(name=>name.startsWith('extension/core/bin/'))});
    const mcp=packages.find(p=>p.name.startsWith('readmd-mcp-server-')),mcpFile=path.join(directory,mcp.name);
    const mcpNames=run('unzip',['-Z1',mcpFile],{encoding:'utf8'}).trim().split(/\r?\n/).sort();
    if(JSON.stringify(mcpNames)!==JSON.stringify(['LICENSE','README.md','mcp_config_templates.json']))throw Error('Unexpected MCP kit members');
    for(const name of mcpNames){const source=name==='LICENSE'?name:'packages/mcp-server/'+name;if(sha(run('unzip',['-p',mcpFile,name]))!==sha(original(source)))throw Error('MCP documentation/configuration differs from source');}
    Object.assign(mcp,{archiveInspected:true,integrationFilesVerified:true});
    checks.push({name:mcp.name,sourceFilesMatch:true,kit:'documentation and configuration; kernel supplied by desktop/server executable'});
    return {sourceCommit,ciRunId:buildRunId,bootSha256:expected.get('assets/readmd.boot.js'),packages,checks,installerMatchesPortable:true,debianMatchesTarball:true,foreignExecutablesLaunched:false};
  } finally {fs.rmSync(temporary,{recursive:true});}
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  const [directory,sourceCommit,buildRunId,output]=process.argv.slice(2);
  const proof=inspectPlatformPackages({directory:path.resolve(directory),sourceCommit,buildRunId:Number(buildRunId),scratch:path.dirname(path.resolve(output))});
  fs.writeFileSync(output,JSON.stringify(proof,null,2)+'\n',{flag:'wx'});
  console.log(JSON.stringify({packages:proof.packages.length,sourceCommit,bootSha256:proof.bootSha256,foreignExecutablesLaunched:false}));
}
