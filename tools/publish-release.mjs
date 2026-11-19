// Stage and authenticate the complete desktop release before replacing downloads.
// Node built-ins only. This runs after offline compilation, never in the app.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const version = fs.readFileSync(path.join(root, 'VERSION'), 'utf8').trim();
export const PACKAGE_NAMES = ['ReadMD-linux-x86_64.deb','ReadMD-linux-x86_64.tar.gz','ReadMD-macos-arm64.dmg','ReadMD-macos-arm64.zip','ReadMD-macos-x64.dmg','ReadMD-macos-x64.zip','ReadMD-windows-x64.zip','ReadMDSetup-windows-x64.exe',`readmd-vscode-${version}.vsix`,`readmd-mcp-server-${version}.zip`];
export function parseChecksums(text) {
  const entries = new Map();
  for (const line of text.trim().split(/\r?\n/)) {
    const match = /^([a-f\d]{64})\s+\*?([\w.-]+)$/.exec(line);
    if (!match || !PACKAGE_NAMES.includes(match[2]) || entries.has(match[2])) throw Error('Invalid or unexpected checksum entry');
    entries.set(match[2],match[1]);
  }
  if (entries.size !== PACKAGE_NAMES.length) throw Error('Incomplete release checksum manifest');
  return entries;
}
function digest(file) { return crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex'); }
function requireFiles(directory, names) {
  const actual=fs.readdirSync(directory).sort();
  if (JSON.stringify(actual)!==JSON.stringify([...names].sort())
      || actual.some(name=>!fs.lstatSync(path.join(directory,name)).isFile()))
    throw Error('Release directory must contain only the desktop/integration packages'+(names.includes('SHA256SUMS.txt')?' and checksums':''));
}
export function writeChecksums(directory) {
  requireFiles(directory,PACKAGE_NAMES);
  const text=PACKAGE_NAMES.map(name=>{
    const file=path.join(directory,name);
    if(!fs.statSync(file).size)throw Error('Empty release package: '+name);
    return digest(file)+'  '+name;
  }).join('\n')+'\n';
  fs.writeFileSync(path.join(directory,'SHA256SUMS.txt'),text,{flag:'wx'});
  return parseChecksums(text);
}
export function releaseNotes(text,{tag,commit,repo}) {
  const headings=[...text.matchAll(/^#[ \t]+ReadMD[ \t]+([vV]\d+\.\d+\.\d+)[^\r\n]*$/gm)];
  const matches=headings.filter(heading=>heading[1].slice(1)===tag.slice(1));
  if(matches.length!==1)throw Error('Release notes must contain exactly one section for '+tag);
  const heading=matches[0],next=headings[headings.indexOf(heading)+1];
  const section=text.slice(heading.index,next?.index??text.length).trim().replace(/\n(?:---|\*\*\*)\s*$/,'').trimEnd();
  const base='https://github.com/'+repo+'/blob/'+commit+'/';
  return section.replace(/\]\(([^\s)]+)(\s+"[^"\n]*")?\)/g,(link,href,title='')=>{
    if(href.startsWith('#')||/^[a-z][a-z\d+.-]*:/i.test(href)||href.startsWith('//'))return link;
    const url=new URL(href.replace(/^\/+/,''),base).href;
    if(!url.startsWith(base))throw Error('Release note link escapes the repository');
    return ']('+url+title+')';
  })+'\n';
}
export async function publish({ directory, tag, commit, repo, token, notes, request, createTag=false }) {
  if (!/^Natsummerance\/rust-ReadMD$/i.test(repo) || !/^[a-f\d]{40}$/.test(commit) || !/^[vV]\d+\.\d+\.\d+$/.test(tag)) throw Error('Invalid release target');
  if (tag.slice(1)!==version) throw Error('Tag and application version differ');
  notes=releaseNotes(notes,{tag,commit,repo});
  const names=[...PACKAGE_NAMES,'SHA256SUMS.txt'];
  requireFiles(directory,names);
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
  const refRoute=base+'/git/ref/tags/'+tag;
  const verifyRef=async ref=>{
    let object=ref?.object;
    for(let i=0;object?.type==='tag'&&i<5;i++)object=(await call('GET',base+'/git/tags/'+object.sha)).object;
    if(object?.type!=='commit'||object.sha!==commit)throw Error('Release tag does not point to this verified build');
  };
  let ref=await call('GET',refRoute,undefined,{missing:createTag}),createdTag=false;
  if(!ref) {
    if(!createTag)throw Error('Release tag must already exist');
    try {
      await call('POST',base+'/git/refs',{ref:'refs/tags/'+tag,sha:commit});
      createdTag=true;
    } catch(error) {
      // Another publisher may have created it. Never update an existing ref.
      const raced=await call('GET',refRoute,undefined,{missing:true});
      if(!raced)throw error;
      await verifyRef(raced);
    }
    ref=await call('GET',refRoute);
  }
  await verifyRef(ref);
  let release,createdDraft=false,publicationAttempted=false;
  try {
  release=await call('GET',base+'/releases/tags/'+tag,undefined,{missing:true});
  if(!release) {
    // The tag endpoint only promises published releases. Reuse an owned
    // draft after a failed upload instead of creating another release.
    const matches=[];
    for(let page=1;page<=20;page++) {
      const releases=await call('GET',base+'/releases?per_page=100&page='+page);
      matches.push(...releases.filter(item=>item.tag_name===tag));
      if(releases.length<100)break;
      if(page===20)throw Error('Release listing exceeds the safe retry limit');
    }
    if(matches.length>1)throw Error('Multiple releases use this version tag');
    release=matches[0];
    if(release?.draft&&release.target_commitish!==commit)throw Error('Existing release draft belongs to another source commit');
  }
  if(!release) {
    release=await call('POST',base+'/releases',{tag_name:tag,target_commitish:commit,name:'ReadMD '+tag,draft:true,prerelease:false,body:notes});
    createdDraft=true;
  }
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
  publicationAttempted=true;
  await call('PATCH',base+'/releases/'+release.id,{name:'ReadMD '+tag,target_commitish:commit,body:notes,draft:false,prerelease:false,make_latest:'true'});
  // Old packages remain recoverable until the complete replacement is verified.
  for(const a of final)if(/^previous-\d+-/.test(a.name)&&names.some(n=>a.name.endsWith('-'+n)))await call('DELETE',assetRoute(a.id));
  console.log('Formal release verified: '+tag+' at '+commit);
  } catch(error) {
    // A failed upload can discard only this invocation's newly created tag,
    // while its own release is provably still private. After attempting to
    // publish, an API timeout may hide success: keep the tag for safe retry.
    if(createdTag&&createdDraft&&!publicationAttempted) {
      try {
        const draft=await call('GET',base+'/releases/'+release.id);
        const current=await call('GET',refRoute,undefined,{missing:true});
        if(draft.id===release.id&&draft.draft===true&&draft.tag_name===tag&&draft.target_commitish===commit
            &&current?.object?.type==='commit'&&current.object.sha===commit)
          await call('DELETE',base+'/git/refs/tags/'+tag);
      } catch { /* Uncertain remote state is preserved for a retry. */ }
    }
    throw error;
  }
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)) {
  const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
  const version=fs.readFileSync(path.join(root,'VERSION'),'utf8').trim();
  const at=process.argv.indexOf('--assets-dir');
  if(at<0||!process.argv[at+1])throw Error('--assets-dir is required');
  const directory=path.resolve(process.argv[at+1]);
  const tag=process.env.RELEASE_TAG||'V'+version;
  if(tag.replace(/^[vV]/,'')!==version)throw Error('Tag and application version differ');
  if(process.argv.includes('--checksums-only')) {
    writeChecksums(directory);
    console.log('Verified '+PACKAGE_NAMES.length+' release packages and wrote SHA256SUMS.txt');
  } else {
    if(!process.env.GH_TOKEN)throw Error('GH_TOKEN is required');
    await publish({directory,tag,commit:process.env.EXPECTED_COMMIT,repo:process.env.GH_REPO,token:process.env.GH_TOKEN,createTag:process.env.CREATE_RELEASE_TAG==='true',notes:fs.readFileSync(path.join(root,'RELEASE_NOTES.md'),'utf8')});
  }
}
