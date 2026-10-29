// Refresh only an existing release's Mac downloads. Keep the original tag and
// other platforms; upload/authenticate the whole candidate before any rename.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {PACKAGE_NAMES,parseChecksums} from './publish-release.mjs';
export const MAC_NAMES=['ReadMD-macos-arm64.dmg','ReadMD-macos-arm64.zip','ReadMD-macos-x64.dmg','ReadMD-macos-x64.zip'];
const hash=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const same=(a,b)=>a?.id===b?.id&&a.name===b.name&&a.size===b.size&&a.digest===b.digest&&a.state==='uploaded';
const authentic=(a,f)=>a?.state==='uploaded'&&a.size===f.size&&a.digest==='sha256:'+f.sha256;
export function mergedChecksums(before,replacements) {
  const entries=new Map(before.assets.filter(a=>PACKAGE_NAMES.includes(a.name)).map(a=>[a.name,a.digest?.replace(/^sha256:/,'')]));
  if(entries.size!==PACKAGE_NAMES.length||[...entries.values()].some(s=>!/^[a-f\d]{64}$/.test(s)))throw Error('Existing release has incomplete digests');
  for(const file of replacements){if(!MAC_NAMES.includes(file.name))throw Error('Only Mac downloads may change');entries.set(file.name,file.sha256);}
  const text=PACKAGE_NAMES.map(n=>entries.get(n)+'  '+n).join('\n')+'\n';parseChecksums(text);return text;
}
export async function refreshMacos({before,directory,sourceCommit,tagCommit,notes,verification,request,checkpoint=()=>{}}) {
  const repo='Natsummerance/rust-ReadMD',base='repos/'+repo;
  if(!/^[a-f\d]{40}$/.test(sourceCommit)||!/^[a-f\d]{40}$/.test(tagCommit)||!/^V0\.0\.5$/.test(before.tag_name)||before.draft||before.immutable)throw Error('Invalid existing release target');
  if(verification?.sourceCommit!==sourceCommit||verification.tagCommit!==tagCommit||verification.nativeWorkflow?.passed!==true||verification.signing!=='ad-hoc'||verification.notarized!==false)throw Error('Bound native verification is required');
  const names=[...MAC_NAMES,'SHA256SUMS.txt'];
  if(JSON.stringify(fs.readdirSync(directory).sort())!==JSON.stringify([...names].sort()))throw Error('Only Mac downloads and checksums may be uploaded');
  const files=names.map(name=>{const file=path.join(directory,name);if(!fs.lstatSync(file).isFile())throw Error('Expected regular release files');return {name,file,size:fs.statSync(file).size,sha256:hash(file)};});
  if(files.some(f=>!f.size))throw Error('Empty release file');
  for(const file of files.filter(f=>MAC_NAMES.includes(f.name))) {
    const proof=verification.packages?.find(p=>p.name===file.name);
    if(!proof||proof.size!==file.size||proof.sha256!==file.sha256||proof.signatureValid!==true||proof.coldStartupPassed!==true)throw Error('Package differs from verified artifact');
  }
  if(fs.readFileSync(path.join(directory,'SHA256SUMS.txt'),'utf8')!==mergedChecksums(before,files.filter(f=>MAC_NAMES.includes(f.name))))throw Error('Merged checksums changed another platform');
  const call=request,assetRoute=id=>base+'/releases/assets/'+id,assets=()=>call('GET',base+'/releases/'+before.id+'/assets?per_page=100');
  async function checkTag(){let object=(await call('GET',base+'/git/ref/tags/'+before.tag_name)).object;for(let i=0;object.type==='tag'&&i<5;i++)object=(await call('GET',base+'/git/tags/'+object.sha)).object;if(object.type!=='commit'||object.sha!==tagCommit)throw Error('Original tag changed');}
  const release=await call('GET',base+'/releases/tags/'+before.tag_name);
  if(release.id!==before.id||release.body!==before.body||release.draft||release.immutable)throw Error('Release metadata changed since inspection');
  await checkTag();
  if((await call('GET',base+'/commits/'+sourceCommit)).sha!==sourceCommit)throw Error('Hotfix source must be pushed before publishing binaries');
  const original=before.assets.filter(a=>[...PACKAGE_NAMES,'SHA256SUMS.txt'].includes(a.name));
  if(original.length!==PACKAGE_NAMES.length+1)throw Error('Original asset inventory is incomplete');
  const unchanged=original.filter(a=>!names.includes(a.name));
  function checkOriginal(current){for(const old of original)if(!same(current.find(a=>a.name===old.name),old))throw Error('Release downloads changed since inspection');}
  checkOriginal(await assets());
  const prefix='macos-refresh-'+sourceCommit.slice(0,12)+'-',staged=new Map(),journal={releaseId:before.id,tag:before.tag_name,tagCommit,sourceCommit,status:'staging',replacements:[]};
  await checkpoint(journal);
  for(const file of files) {
    const candidateName=prefix+file.name;
    let asset=(await assets()).find(a=>a.name===candidateName);
    if(asset&&!authentic(asset,file))throw Error('Existing candidate has different bytes');
    if(!asset)asset=await call('POST',release.upload_url.replace(/\{.*$/,'')+'?name='+encodeURIComponent(candidateName),undefined,{file:file.file});
    if(!authentic(asset,file))throw Error('Candidate upload failed SHA-256 authentication');
    const old=original.find(a=>a.name===file.name);
    staged.set(file.name,asset);journal.replacements.push({name:file.name,oldId:old.id,nextId:asset.id,sha256:file.sha256});await checkpoint(journal);
  }
  checkOriginal(await assets());await checkTag();journal.status='replacing';await checkpoint(journal);
  const moved=[];
  try {
    for(const file of files) {
      const old=original.find(a=>a.name===file.name),next=staged.get(file.name);
      moved.push({old,next}); // Include an ambiguously acknowledged rename in recovery.
      await call('PATCH',assetRoute(old.id),{name:prefix+'previous-'+old.id+'-'+file.name});
      await call('PATCH',assetRoute(next.id),{name:file.name});await checkpoint(journal);
    }
    const final=await assets();
    for(const file of files)if(!authentic(final.find(a=>a.name===file.name),file))throw Error('Final Mac download verification failed');
    for(const old of unchanged)if(!same(final.find(a=>a.name===old.name),old))throw Error('Another platform changed during replacement');
    await checkTag();
    await call('PATCH',base+'/releases/'+before.id,{body:notes});
    if((await call('GET',base+'/releases/tags/'+before.tag_name)).body!==notes)throw Error('Release notes verification failed');
    journal.status='verified';await checkpoint(journal);
  } catch(error) {
    let recovered=true;
    for(const {old,next} of moved.reverse()) {
      try {
        const current=await assets();
        const currentNext=current.find(a=>a.id===next.id),currentOld=current.find(a=>a.id===old.id);
        if(!currentNext||!currentOld)throw Error('Recovery asset missing');
        if(currentNext.name===old.name)await call('PATCH',assetRoute(next.id),{name:prefix+old.name});
        else if(currentNext.name!==prefix+old.name)throw Error('Candidate changed outside refresh');
        if(currentOld.name!==old.name){if(currentOld.name!==prefix+'previous-'+old.id+'-'+old.name)throw Error('Original asset changed outside refresh');await call('PATCH',assetRoute(old.id),{name:old.name});}
      }catch{recovered=false;}
    }
    try{checkOriginal(await assets());const current=await call('GET',base+'/releases/tags/'+before.tag_name);if(current.body===notes)await call('PATCH',base+'/releases/'+before.id,{body:before.body});else if(current.body!==before.body)recovered=false;}catch{recovered=false;}
    journal.status=recovered?'rolled_back':'recovery_pending';await checkpoint(journal);
    throw Error('Mac refresh failed; '+journal.status+'. '+error.message);
  }
  // Destructive cleanup follows a durable verified receipt, never staging.
  journal.retainedBackups=[];
  for(const old of original.filter(a=>names.includes(a.name))) {
    try {const current=(await assets()).find(a=>a.id===old.id);if(current?.name!==prefix+'previous-'+old.id+'-'+old.name||current.digest!==old.digest)throw Error('Backup changed');await call('DELETE',assetRoute(old.id));}
    catch{journal.retainedBackups.push(old.id);}
  }
  journal.status=journal.retainedBackups.length?'verified_with_retained_backups':'complete';await checkpoint(journal);return journal;
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)) {
  const args=process.argv.slice(2),option=name=>args[args.indexOf(name)+1];
  const required=['--before','--assets-dir','--verification','--notes','--journal'];
  if(args.length!==required.length*2||!required.every(n=>args.includes(n))||!process.env.GH_TOKEN)throw Error('Expected --before --assets-dir --verification --notes --journal and GH_TOKEN');
  const verification=JSON.parse(fs.readFileSync(option('--verification'))),before=JSON.parse(fs.readFileSync(option('--before'))),journalFile=path.resolve(option('--journal'));
  const checkpoint=journal=>{const tmp=journalFile+'.tmp';const fd=fs.openSync(tmp,'wx',0o600);try{fs.writeFileSync(fd,JSON.stringify(journal,null,2)+'\n');fs.fsyncSync(fd);}finally{fs.closeSync(fd);}fs.renameSync(tmp,journalFile);};
  const request=async(method,route,data,{file}={})=>{
    const url=route.startsWith('https://uploads.github.com/')?route:'https://api.github.com/'+route;
    if(!/^https:\/\/(api|uploads)\.github\.com\//.test(url))throw Error('Unexpected API endpoint');
    const response=await fetch(url,{method,headers:{Authorization:'Bearer '+process.env.GH_TOKEN,Accept:'application/vnd.github+json','X-GitHub-Api-Version':'2022-11-28','Content-Type':file?'application/octet-stream':'application/json'},body:file?await fs.openAsBlob(file):data===undefined?undefined:JSON.stringify(data),signal:AbortSignal.timeout(file?600000:60000)});
    if(!response.ok)throw Error('GitHub '+method+' HTTP '+response.status);
    return response.status===204?null:response.json();
  };
  const result=await refreshMacos({before,directory:path.resolve(option('--assets-dir')),sourceCommit:verification.sourceCommit,tagCommit:verification.tagCommit,notes:fs.readFileSync(option('--notes'),'utf8'),verification,request,checkpoint});
  console.log(JSON.stringify({status:result.status,tag:result.tag,sourceCommit:result.sourceCommit,retainedBackups:result.retainedBackups.length}));
}
