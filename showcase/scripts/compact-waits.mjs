/** Shorten only uninterrupted network/model waits in real recordings. */
import fs from 'node:fs';import path from 'node:path';import crypto from 'node:crypto';import{spawnSync}from'node:child_process';import{fileURLToPath}from'node:url';
const out=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..'),manifestPath=path.join(out,'manifest.json');
const manifest=JSON.parse(fs.readFileSync(manifestPath));
const run=(cmd,args)=>{const r=spawnSync(cmd,args,{windowsHide:true,encoding:'utf8'});if(r.status!==0)throw Error(r.stderr.slice(-900));return r.stdout;};
const hash=p=>crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const time=s=>{const n=Math.round(s*1000);return `${String(Math.floor(n/3600000)).padStart(2,'0')}:${String(Math.floor(n/60000)%60).padStart(2,'0')}:${String(Math.floor(n/1000)%60).padStart(2,'0')}.${String(n%1000).padStart(3,'0')}`;};
for(const f of manifest.features){
 const r=f.recording;if(r.editing||r.status!=='recorded'||r.duration<=12)continue;
 const video=path.join(out,r.video);if(hash(video)!==r.sha256)throw Error(f.id+' recording changed');
 const intervals=(r.waits||[]).filter(w=>w.end-w.start>7).map(w=>({start:w.start+.5,end:w.end-1,target:2,reason:'explicit_operation_wait'}));
 // A hidden OS chooser, clipboard permission or network wait can leave the
 // captured app unchanged. Shorten only objectively unchanged visual ranges.
 const detection=spawnSync('ffmpeg',['-hide_banner','-i',video,'-vf','freezedetect=n=-45dB:d=3','-an','-f','null','-'],{windowsHide:true,encoding:'utf8'});
 if(detection.status!==0)throw Error('Static-frame detection failed: '+f.id);
 let frozenStart=null;
 const appendFrozen=end=>{if(frozenStart!==null&&end-frozenStart>7){const w={start:frozenStart+.5,end:end-.5,target:1.4,reason:'unchanged_app_frames'};if(!intervals.some(other=>w.start<other.end&&w.end>other.start))intervals.push(w);}frozenStart=null;};
 for(const match of detection.stderr.matchAll(/lavfi\.freezedetect\.freeze_(start|end):\s*([\d.]+)/g)) {
  if(match[1]==='start')frozenStart=Number(match[2]);else appendFrozen(Number(match[2]));
 }
 if(frozenStart!==null)appendFrozen(r.duration);
 intervals.sort((a,b)=>a.start-b.start);
 if(!intervals.length)continue;
 for(const w of intervals)w.speed=(w.end-w.start)/w.target;
 const edited=path.join(out,'.runtime',f.id+'-compact.mp4');
 const map=t=>t-intervals.reduce((n,w)=>n+Math.max(0,Math.min(t,w.end)-w.start)*(1-1/w.speed),0);
 const pts='T'+intervals.map(w=>`-if(lt(T,${w.start}),0,if(lt(T,${w.end}),(T-${w.start})*${1-1/w.speed},${(w.end-w.start)*(1-1/w.speed)}))`).join('');
 const filter=`setpts='(${pts})/TB',fps=20,tpad=stop_mode=clone:stop_duration=1.4`;
 run('ffmpeg',['-hide_banner','-loglevel','error','-y','-i',video,'-vf',filter,'-c:v','libx264','-preset','fast','-crf','18','-pix_fmt','yuv420p','-movflags','+faststart','-an',edited]);
 const m=JSON.parse(run('ffprobe',['-v','error','-show_entries','format=duration,size','-of','json',edited]));
 r.editing={kind:'accelerated_wait',intervals,realtime_seconds:r.duration,original_sha256:r.sha256};
 r.duration=Number(m.format.duration);r.bytes=Number(m.format.size);r.steps=r.steps.map(s=>({...s,at:map(s.at)}));
 for(const w of intervals)r.steps.push({at:map(w.start),text:`等待过程已加速；本次真实操作总用时 ${r.editing.realtime_seconds.toFixed(1)} 秒`});r.steps.sort((a,b)=>a.at-b.at);
 r.sha256=hash(edited);fs.copyFileSync(edited,video);fs.unlinkSync(edited);
 run('ffmpeg',['-hide_banner','-loglevel','error','-y','-i',video,'-ss',String(r.duration-1.7),'-frames:v','1',path.join(out,r.poster)]);
 fs.writeFileSync(path.join(out,r.captions),'WEBVTT\n\n'+r.steps.map((s,i)=>`${time(s.at)} --> ${time(r.steps[i+1]?.at||r.duration)}\n${s.text}\n`).join('\n'));
 console.log(`${f.id}: ${r.editing.realtime_seconds.toFixed(1)}s → ${r.duration.toFixed(1)}s; wait edit documented in sidecars`);
}
fs.writeFileSync(manifestPath,JSON.stringify(manifest,null,2)+'\n');
