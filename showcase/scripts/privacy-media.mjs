import fs from'node:fs';import path from'node:path';import crypto from'node:crypto';import{spawnSync,spawn}from'node:child_process';import{fileURLToPath}from'node:url';
const scripts=path.dirname(fileURLToPath(import.meta.url));
export const hash=p=>crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const run=(cmd,args)=>{const r=spawnSync(cmd,args,{windowsHide:true,encoding:'utf8'});if(r.status!==0)throw Error('Media privacy processing failed: '+r.stderr.slice(-500));return r.stdout;};
export async function inspectFrames(frames,work){
 if(process.platform!=='win32')throw Error('This pipeline requires the local Windows OCR privacy gate');
 const reports=await Promise.allSettled([0,1,2,3].map(async shard=>{
  const report=path.join(work,`ocr-${shard}.json`);
  const child=spawn('powershell.exe',['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',path.join(scripts,'privacy-ocr.ps1'),'-InputDirectory',frames,'-OutputFile',report,'-Shard',String(shard),'-Shards','4'],{windowsHide:true,stdio:['ignore','ignore','pipe']});let error='';child.stderr.on('data',b=>error+=b);
  await new Promise((resolve,reject)=>{child.once('error',reject);child.once('exit',c=>c?reject(Error('Local privacy OCR failed: '+error.slice(-400))):resolve());});
  return JSON.parse(fs.readFileSync(report,'utf8').replace(/^\uFEFF/,''));
 }));
 for(const r of reports)if(r.status==='rejected')throw r.reason;
 return{frames:reports.reduce((n,r)=>n+r.value.frames,0),findings:reports.flatMap(r=>r.value.findings)};
}
export function extractFrames(video,id,frames,poster){
 run('ffmpeg',['-hide_banner','-loglevel','error','-y','-i',video,'-vf','fps=4','-compression_level','1',path.join(frames,id+'-%04d.png')]);
 if(poster)run('ffmpeg',['-hide_banner','-loglevel','error','-y','-i',poster,'-frames:v','1',path.join(frames,id+'-poster.png')]);
}
export function redactPaths(video,findings,duration,work){
 const boxes=[];
 for(const f of findings){
  const words=f.rectangles;if(!words?.length)continue;
  const x=Math.max(0,Math.floor(Math.min(...words.map(r=>r.x))-4)),y=Math.max(0,Math.floor(Math.min(...words.map(r=>r.y))-3));
  const w=Math.ceil(Math.max(...words.map(r=>r.x+r.width))-x+4),h=Math.ceil(Math.max(...words.map(r=>r.y+r.height))-y+3);
  const number=Number(f.frame.match(/-(\d+)\.png$/)?.[1]),time=number?(number-1)/4:0;
  const start=number?Math.max(0,time-.5):0,end=number?Math.min(duration,time+.75):duration;
  const existing=boxes.find(b=>Math.abs(b.y-y)<4&&Math.abs(b.h-h)<4&&start<=b.end+.3&&end>=b.start-.3);
  if(existing){const right=Math.max(existing.x+existing.w,x+w);existing.x=Math.min(existing.x,x);existing.w=right-existing.x;existing.start=Math.min(existing.start,start);existing.end=Math.max(existing.end,end);}else boxes.push({x,y,w,h,start,end});
 }
 if(!boxes.length)return 0;
 const filter=boxes.map(b=>`drawbox=x=${b.x}:y=${b.y}:w=${b.w}:h=${b.h}:color=0xe8e8ed:t=fill:enable='between(t,${b.start},${b.end})'`).join(',');
 const filterFile=path.join(work,'redaction-filter.txt'),output=path.join(work,'redacted.mp4');fs.writeFileSync(filterFile,'[0:v]'+filter+'[safe]');
 run('ffmpeg',['-hide_banner','-loglevel','error','-y','-i',video,'-filter_complex_script',filterFile,'-map','[safe]','-c:v','libx264','-crf','24','-preset','fast','-pix_fmt','yuv420p','-movflags','+faststart','-an',output]);
 fs.copyFileSync(output,video);fs.unlinkSync(output);return boxes.length;
}
export function buildPoster(video,poster,duration){run('ffmpeg',['-hide_banner','-loglevel','error','-y','-i',video,'-ss',String(Math.min(duration*.65,duration-.5)),'-frames:v','1',poster]);}
export async function approveRecording(video,id,work,duration){
 const frames=path.join(work,'privacy-frames');fs.mkdirSync(frames);extractFrames(video,id,frames);
 const initial=await inspectFrames(frames,work);const regions=redactPaths(video,initial.findings,duration,work);
 if(regions){for(const n of fs.readdirSync(frames))fs.unlinkSync(path.join(frames,n));extractFrames(video,id,frames);const final=await inspectFrames(frames,work);if(final.findings.length)throw Error('Recording still contains a private path');}
 fs.rmSync(frames,{recursive:true,force:true});
 return{approved:true,method:'isolated-webview-capture-and-opaque-path-redaction',sampled_frames:initial.frames,redacted_regions:regions,sha256:hash(video)};
}
