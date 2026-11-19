// CI input preparation, not a build-time download. Run the actual previous
// V0.0.6 writer against an owned profile, followed by the freshly built kernel.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {Readable} from 'node:stream';
import {fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';
import {createPreviousWriterInput} from './review-version-fence-input.mjs';
if(!process.env.CI||!process.env.GITHUB_WORKSPACE)throw Error('Dedicated CI fixture required');
const repo=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
if(fs.realpathSync.native(repo)!==fs.realpathSync.native(process.env.GITHUB_WORKSPACE))throw Error('Unexpected CI workspace');
const mac=JSON.parse(fs.readFileSync(path.join(repo,'docs/reviews/readmd-v006-p0-macos-closeout-2026-11-03.json')));
// V0.0.6 was refreshed after P0. Pin its independently inspected and anonymously
// re-downloaded delivery receipt, not a moving release checksum or an old ZIP.
const published=JSON.parse(fs.readFileSync(path.join(repo,'tests/fixtures/review-version/previous-release-v006-5964ebd0-2026-11-05.json')));
if(published.schemaVersion!==1||published.version!=='0.0.6'||published.publicationStatus!=='complete'||published.runtimeSourceCommit!=='5964ebd0e2a4fecb676b2b8ee95573fa4b516fa6'||published.ciRunId!==41981825560||published.tagCommit!=='544defc32219e4de6b85e9d07d9edc230d6639f4'||published.anonymousDownloadsVerified!==11)throw Error('Unreviewed previous writer receipt');
const arch=process.arch==='arm64'?'arm64':'x64';
if(!['win32','linux','darwin'].includes(process.platform)||process.platform!=='darwin'&&process.arch!=='x64')throw Error('No certified previous writer for this target');
const name=process.platform==='win32'?'ReadMD-windows-x64.zip':process.platform==='linux'?'ReadMD-linux-x86_64.tar.gz':`ReadMD-macos-${arch}.zip`;
const proof=published.packages.find(p=>p.name===name),download=published.anonymousDownloads.find(p=>p.name===name);
if(!proof||!download?.passed||proof.sha256!==download.sha256||proof.size!==download.size||!/^[a-f0-9]{64}$/.test(proof.binarySha256||''))throw Error('Pinned previous package missing');
const own=path.join(repo,'.cache/p1-previous-writer'),archive=path.join(own,name),unpacked=path.join(own,'previous');
fs.mkdirSync(own,{recursive:true});if(fs.lstatSync(own).isSymbolicLink())throw Error('Fixture directory cannot be a symlink');
fs.mkdirSync(unpacked);
const input=createPreviousWriterInput({directory:own,name,expected:proof,writerSourceCommit:published.runtimeSourceCommit,writerRunId:published.ciRunId,consumerSourceCommit:process.env.GITHUB_SHA});
let consumerFailed=false;
try{
 const artifact=path.join(repo,'.cache/p1-previous-writer-artifact',name);
 if(!fs.lstatSync(artifact).isFile())throw Error('Expected a regular archived writer package');
 await input.receive(new Response(Readable.toWeb(fs.createReadStream(artifact))));
 const sha=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
 if(fs.statSync(archive).size!==proof.size||sha(archive)!==proof.sha256)throw Error('Previous package differs from certified published fixture');
 input.phase('extract');
 const tar=process.platform==='win32'?path.join(process.env.SystemRoot,'System32/tar.exe'):'/usr/bin/tar';
 execFileSync(tar,['-xf',archive,'-C',unpacked],{stdio:'pipe'});
 const old=path.join(unpacked,...(process.platform==='darwin'?['ReadMD.app','Contents','MacOS','ReadMD']:process.platform==='win32'?['ReadMD.exe']:['ReadMD']));
 input.phase('binary_authentication');
 const expected=proof.binarySha256;
 if(sha(old)!==expected)throw Error('Previous executable differs from certified published program');
 if(process.platform!=='win32')fs.chmodSync(old,0o755);
 input.phase('prepare_fixture');
 const current=process.env.READMD_BIN||path.join(repo,'rust/target/debug',process.platform==='win32'?'readmd.exe':'readmd');
 const schema=Number(fs.readFileSync(path.join(repo,'rust/readmd-kernel/src/review_store.rs'),'utf8').match(/SCHEMA_VERSION:\s*i64\s*=\s*(\d+)/)?.[1]);
 const oldSchema=mac.oldRealBinaryFormatFence.newSchema;if(!Number.isSafeInteger(schema)||schema<=oldSchema)throw Error('Expected a newer semantic storage version');
 fs.writeFileSync(path.join(own,'source-fixture.json'),JSON.stringify({name,sha256:proof.sha256,oldBinarySha256:expected,oldSourceCommit:published.runtimeSourceCommit,oldSchema,newSchema:schema,modelsRequested:false},null,2)+'\n',{flag:'wx'});
 input.phase('fence_behavior');
 execFileSync(process.execPath,[path.join(repo,'ui-tests/review-version-fence-http.cjs'),'--old-bin',old,'--new-bin',current,'--out',path.join(own,'result'),'--assets',path.join(repo,'assets'),'--old-schema',String(oldSchema),'--new-schema',String(schema)],{stdio:'inherit'});
 input.phase('complete');
}catch(error){
 consumerFailed=true;input.failed();throw error;
}finally{
 try{input.finalize();}catch(error){
  if(!consumerFailed)throw error;
  console.error('Previous fixture input diagnostics failed; original consumer failure preserved');
 }finally{
 // Known, authenticated fixture only; rm does not follow its internal symlinks.
 fs.rmSync(unpacked,{recursive:true});
 }
}
