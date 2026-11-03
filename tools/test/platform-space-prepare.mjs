// Prepare reviewed input and compile once. No model, GUI, VM or user profile.
// This helper is for an ephemeral CI host, not a developer's actual disks.
import fs from 'node:fs';import path from 'node:path';import crypto from 'node:crypto';import {spawnSync} from 'node:child_process';import {fileURLToPath} from 'node:url';
import {verifyLocalPack} from '../lib/capability-publication.mjs';
if(!process.env.CI||!process.env.GITHUB_WORKSPACE)throw Error('Dedicated CI required');
const repo=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');if(fs.realpathSync(repo)!==fs.realpathSync(process.env.GITHUB_WORKSPACE))throw Error('Unexpected CI workspace');
const out=path.join(repo,'.cache/p1-platform-space');fs.mkdirSync(out,{recursive:true});if(fs.lstatSync(out).isSymbolicLink())throw Error('Owned preparation cannot be redirected');
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex'),catalog=JSON.parse(fs.readFileSync(path.join(repo,'assets/capabilities/catalog.json'))),entry=catalog.packs.find(p=>p.id==='fonts');
if(!entry||entry.downloadBytes>64*1024*1024||!/^https:\/\/github\.com\/Natsummerance\/tool-ReadMD\/releases\/download\//.test(entry.url))throw Error('Reviewed font input required');
const archive=path.join(out,'locked-fonts.readmd-pack');
if(process.argv.includes('--clean-input')){
 if(fs.existsSync(archive)){
  const stat=fs.lstatSync(archive);if(!stat.isFile()||stat.isSymbolicLink()||stat.size!==entry.downloadBytes||sha(fs.readFileSync(archive))!==entry.sha256)throw Error('Preserve changed input');
  fs.unlinkSync(archive);
 }
 console.log(JSON.stringify({ownedFontInputRemoved:true}));process.exit(0);
}
const response=await fetch(entry.url,{signal:AbortSignal.timeout(180000)});if(!response.ok)throw Error('Font input preparation HTTP '+response.status);
const chunks=[];let size=0;for await(const chunk of response.body){size+=chunk.length;if(size>entry.downloadBytes)throw Error('Font input exceeds reviewed budget');chunks.push(chunk);}const bytes=Buffer.concat(chunks);
if(bytes.length!==entry.downloadBytes||sha(bytes)!==entry.sha256)throw Error('Font input does not match reviewed catalog');
const lock=JSON.parse(fs.readFileSync(path.join(repo,`assets/capabilities/fonts-${entry.version}-${entry.platform}.lock.json`)));verifyLocalPack(bytes,lock,entry);fs.writeFileSync(archive,bytes,{flag:'wx',mode:0o600});
const safeEnv=Object.fromEntries(Object.entries(process.env).filter(([key])=>!/TOKEN|SECRET|CREDENTIAL|API_KEY/i.test(key)));
const build=spawnSync('cargo',['test','--offline','--locked','--manifest-path',path.join(repo,'rust/Cargo.toml'),'-p','readmd-kernel','--no-default-features','--lib','--no-run','--message-format=json'],{cwd:repo,env:{...safeEnv,CARGO_INCREMENTAL:'0',CARGO_BUILD_JOBS:'2'},encoding:'utf8',timeout:600000,maxBuffer:16*1024*1024});
fs.writeFileSync(path.join(out,'test-build.log'),build.stderr||'',{flag:'wx'});if(build.error||build.status!==0)throw Error('Offline test consumer build failed; see owned log');
const outputs=build.stdout.split('\n').filter(Boolean).map(line=>JSON.parse(line)).filter(v=>v.reason==='compiler-artifact'&&v.target.name==='readmd_kernel'&&v.profile.test&&v.executable);
if(outputs.length!==1)throw Error('Expected one real test consumer');const binary=fs.realpathSync(outputs[0].executable);if(!binary.startsWith(repo+path.sep))throw Error('Test consumer outside owned CI workspace');
const git=spawnSync('git',['rev-parse','HEAD'],{cwd:repo,encoding:'utf8'});if(git.status!==0)throw Error('Missing source identity');
const prepared={sourceCommit:git.stdout.trim(),binary:path.relative(repo,binary),kernelTestSha256:sha(fs.readFileSync(binary)),pack:path.relative(repo,archive),offlinePackSha256:entry.sha256,assets:'assets',genuineLockedFonts:true,modelsRequested:false,vmInstalled:false};
fs.writeFileSync(path.join(out,'prepared.json'),JSON.stringify(prepared,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify({sourceCommit:prepared.sourceCommit,genuineLockedFonts:true,kernelTestSha256:prepared.kernelTestSha256,modelsRequested:false}));
