/** Review every published clip and poster; fail closed before rebuilding the site. */
import fs from'node:fs';import path from'node:path';import{fileURLToPath}from'node:url';import{spawnSync}from'node:child_process';
import{recipes}from'./recipes.mjs';import{inspectFrames,extractFrames,redactPaths,buildPoster,hash}from'./privacy-media.mjs';
const out=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..'),manifestPath=path.join(out,'manifest.json');
const m=JSON.parse(fs.readFileSync(manifestPath));
const requested=new Set(process.argv.slice(2));
for(const id of requested)if(!m.features.some(f=>f.id===id))throw Error('Unknown recording '+id);
const selected=m.features.filter(f=>!requested.size||requested.has(f.id));
const previous=requested.size?JSON.parse(fs.readFileSync(path.join(out,'checks/privacy.json'))):null;
// Unchanged clips reuse the existing OCR approval only when both actual media
// hashes still match it; any other changed clip must be explicitly reviewed.
for(const f of m.features){
 const r=f.recording;
 if(r.status!=='recorded')throw Error('Unrecorded '+f.id);
 if(recipes[f.id]?.native&&r.capture_surface!=='isolated-native-webviews')throw Error('Unsafe native capture '+f.id);
 if(hash(path.join(out,r.video))!==r.sha256)throw Error('Changed recording '+f.id);
 if(requested.size&&!requested.has(f.id)){
  const approved=previous.recordings.find(a=>a.id===f.id);
  if(!approved?.approved||approved.sha256!==r.sha256||approved.poster_sha256!==hash(path.join(out,r.poster)))throw Error('Unreviewed media change '+f.id);
 }
}
fs.mkdirSync(path.join(out,'.runtime'),{recursive:true});
const work=fs.mkdtempSync(path.join(out,'.runtime/privacy-final-')),frames=path.join(work,'frames');fs.mkdirSync(frames);
const run=(cmd,args)=>{const r=spawnSync(cmd,args,{windowsHide:true,encoding:'utf8'});if(r.status)throw Error(r.stderr.slice(-500));return r.stdout;};
const time=s=>{const n=Math.round(s*1000);return `${String(Math.floor(n/3600000)).padStart(2,'0')}:${String(Math.floor(n/60000)%60).padStart(2,'0')}:${String(Math.floor(n/1000)%60).padStart(2,'0')}.${String(n%1000).padStart(3,'0')}`;};
for(const f of selected){const r=f.recording;extractFrames(path.join(out,r.video),f.id,frames,path.join(out,r.poster));}
console.log('Inspecting '+selected.length+' clips and posters with four local OCR workers');
const initial=await inspectFrames(frames,work);const changes=[];
for(const f of selected){const r=f.recording,found=initial.findings.filter(i=>i.frame.startsWith(f.id+'-'));const caseWork=path.join(work,f.id);fs.mkdirSync(caseWork);const video=path.join(out,r.video);const oldHash=r.sha256;
 const regions=redactPaths(video,found,r.duration,caseWork);
 if(regions){const meta=JSON.parse(run('ffprobe',['-v','error','-show_entries','format=duration,size','-of','json',video]));r.duration=Number(meta.format.duration);r.bytes=Number(meta.format.size);r.sha256=hash(video);buildPoster(video,path.join(out,r.poster),r.duration);r.privacy_source_sha256=oldHash;changes.push(f);console.log(f.id+': hid '+regions+' private-path regions');}
 r.capture_surface||='isolated-browser';
 r.privacy={approved:true,method:'isolated-webview-capture-and-opaque-path-redaction',sampled_frames:fs.readdirSync(frames).filter(n=>n.startsWith(f.id+'-')).length,redacted_regions:regions+(r.privacy?.redacted_regions||0),sha256:r.sha256,poster_sha256:hash(path.join(out,r.poster))};
 fs.writeFileSync(path.join(out,r.captions),'WEBVTT\n\n'+r.steps.map((s,i)=>`${time(s.at)} --> ${time(r.steps[i+1]?.at||r.duration)}\n${s.text}\n`).join('\n'));
}
let after=0;if(changes.length){const verify=path.join(work,'verify');fs.mkdirSync(verify);for(const f of changes)extractFrames(path.join(out,f.recording.video),f.id,verify,path.join(out,f.recording.poster));const result=await inspectFrames(verify,work);after=result.frames;if(result.findings.length)throw Error('Private paths remain after redaction in '+[...new Set(result.findings.map(f=>f.frame.slice(0,4)))].join(','));}
fs.writeFileSync(manifestPath,JSON.stringify(m,null,2)+'\n');
fs.writeFileSync(path.join(out,'checks/privacy.json'),JSON.stringify({date:new Date().toISOString(),clips:m.features.length,posters:m.features.length,frames_inspected:m.features.reduce((n,f)=>n+f.recording.privacy.sampled_frames,0),reviewed_this_run:selected.map(f=>f.id),redacted_clips:m.features.filter(f=>f.recording.privacy.redacted_regions>0||f.recording.privacy_source_sha256).map(f=>f.id),redacted_this_review:changes.map(f=>f.id),verification_frames:after+(previous?.verification_frames||0),private_path_matches_remaining:0,native_capture:'own WebView surfaces only; no desktop, Explorer or OS file-picker pixels',private_text_persisted:false,recordings:m.features.map(f=>({id:f.id,...f.recording.privacy}))},null,2)+'\n');
console.log(`Privacy review passed: ${m.features.length} approved clips and posters; ${selected.length} reviewed this run, ${initial.frames+after} inspected frames; ${changes.length} clips redacted`);
