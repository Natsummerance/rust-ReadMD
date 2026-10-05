// Stage and authenticate the complete desktop release before replacing downloads.
// Node built-ins only. This runs after offline compilation, never in the app.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';
export const PACKAGE_NAMES = ['ReadMD-linux-x86_64.deb','ReadMD-linux-x86_64.tar.gz','ReadMD-macos-arm64.dmg','ReadMD-macos-arm64.zip','ReadMD-macos-x64.dmg','ReadMD-macos-x64.zip','ReadMD-windows-x64.zip','ReadMDSetup-windows-x64.exe'];
export function parseChecksums(text) {
  const entries = new Map();
  for (const line of text.trim().split(/\r?\n/)) {
    const match = /^([a-f\d]{64})\s+\*?([\w.-]+)$/.exec(line);
    if (!match || !PACKAGE_NAMES.includes(match[2]) || entries.has(match[2])) throw Error('Invalid or unexpected checksum entry');
    entries.set(match[2],match[1]);
  }
  if (entries.size !== PACKAGE_NAMES.length) throw Error('Incomplete desktop checksum manifest');
  return entries;
}
function digest(file) { return crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex'); }
export async function publish({ directory, tag, commit, repo, token, notes, request }) {
  if (!/^Natsummerance\/rust-ReadMD$/i.test(repo) || !/^[a-f\d]{40}$/.test(commit) || !/^[vV]\d+\.\d+\.\d+$/.test(tag)) throw Error('Invalid release target');
  const names=[...PACKAGE_NAMES,'SHA256SUMS.txt'];
  const actual=fs.readdirSync(directory).filter(n=>fs.statSync(path.join(directory,n)).isFile()).sort();
  if (JSON.stringify(actual)!==JSON.stringify([...names].sort())) throw Error('Release directory must contain only the eight desktop packages and checksums');
  const sums=parseChecksums(fs.readFileSync(path.join(directory,'SHA256SUMS.txt'),'utf8'));
  const files=names.map(name=>({name,file:path.join(directory,name),sha:digest(path.join(directory,name)),size:fs.statSync(path.join(directory,name)).size}));
  for(const f of files) if(!f.size || (sums.has(f.name)&&sums.get(f.name)!==f.sha)) throw Error('Local package checksum mismatch: '+f.name);
  const call=request||async function(method,route,data,{file,missing=false}={}) {
    const url=route.startsWith('https://uploads.github.com/')?route:'https://api.github.com/'+route;
    if(!/^https:\/\/(api|uploads)\.github\.com\//.test(url)) throw Error('Invalid GitHub API endpoint');
    const response=await fetch(url,{method,headers:{Authorization:'Bearer '+token,Accept:'application/vnd.github+json','X-GitHub-Api-Version':'2022-11-28','Content-Type':file?'application/octet-stream':'application/json'},body:file?await fs.openAsBlob(file):(data===undefined?undefined:JSON.stringify(data)),signal:AbortSignal.timeout(file?600000:60000)});
    if(missing&&response.status===404)return null;
    if(!response.ok)throw Error('GitHub release request failed: '+method+' HTTP '+response.status);
    return response.status===204?null:response.json();
  };
  const base='repos/'+repo;
  const ref=await call('GET',base+'/git/ref/tags/'+tag);
  let object=ref.object;
  for(let i=0;object.type==='tag'&&i<5;i++)object=(await call('GET',base+'/git/tags/'+object.sha)).object;
  if(object.type!=='commit'||object.sha!==commit)throw Error('Release tag does not point to this verified build');
  let release=await call('GET',base+'/releases/tags/'+tag,undefined,{missing:true});
  if(!release)release=await call('POST',base+'/releases',{tag_name:tag,target_commitish:commit,name:'ReadMD '+tag,draft:true,prerelease:false,body:notes});
  const prefix='candidate-'+commit.slice(0,12)+'-';
  const assetRoute=id=>base+'/releases/assets/'+id;
  const assets=()=>call('GET',base+'/releases/'+release.id+'/assets?per_page=100');
  const authenticated=(a,f)=>a?.state==='uploaded'&&a.size===f.size&&a.digest==='sha256:'+f.sha;
  const staged=new Map();
  for(const file of files) {
    const stageName=prefix+file.name;
    let asset=(await assets()).find(a=>a.name===stageName);
    if(asset&&!authenticated(asset,file))throw Error('Existing candidate has different content: '+file.name);
    if(!asset)asset=await call('POST',release.upload_url.replace(/\{.*$/,'')+'?name='+encodeURIComponent(stageName),undefined,{file:file.file});
    if(!authenticated(asset,file))throw Error('Uploaded candidate failed SHA-256 verification: '+file.name);
    staged.set(file.name,asset);
    console.log('Candidate verified: '+file.name);
  }
  // Do not alter any public package until every candidate passed authentication.
  for(const file of files) {
    let old=(await assets()).find(a=>a.name===file.name);
    const next=staged.get(file.name);
    if(old)await call('PATCH',assetRoute(old.id),{name:'previous-'+old.id+'-'+file.name});
    try { await call('PATCH',assetRoute(next.id),{name:file.name}); }
    catch(error) { if(old)await call('PATCH',assetRoute(old.id),{name:file.name}); throw error; }
  }
  const final=await assets();
  for(const file of files)if(!authenticated(final.find(a=>a.name===file.name),file))throw Error('Final release checksum mismatch: '+file.name);
  await call('PATCH',base+'/releases/'+release.id,{name:'ReadMD '+tag,target_commitish:commit,body:notes,draft:false,prerelease:false,make_latest:'true'});
  // Old packages remain recoverable until the complete replacement is verified.
  for(const a of final)if(/^previous-\d+-/.test(a.name)&&names.some(n=>a.name.endsWith('-'+n)))await call('DELETE',assetRoute(a.id));
  console.log('Formal release verified: '+tag+' at '+commit);
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)) {
  const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
  const version=fs.readFileSync(path.join(root,'VERSION'),'utf8').trim();
  const at=process.argv.indexOf('--assets-dir');
  if(at<0||!process.argv[at+1])throw Error('--assets-dir is required');
  const tag=process.env.RELEASE_TAG||'V'+version;
  if(tag.replace(/^[vV]/,'')!==version)throw Error('Tag and application version differ');
  if(!process.env.GH_TOKEN)throw Error('GH_TOKEN is required');
  await publish({directory:path.resolve(process.argv[at+1]),tag,commit:process.env.EXPECTED_COMMIT,repo:process.env.GH_REPO,token:process.env.GH_TOKEN,notes:fs.readFileSync(path.join(root,'RELEASE_NOTES.md'),'utf8')});
}
