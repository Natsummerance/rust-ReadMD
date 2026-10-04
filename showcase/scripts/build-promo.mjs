/** Keep promotional source footage and all explanations in separate files. */
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {CAPTURE_PROFILE,recordingUiHash} from './capture-mode.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const out=path.join(root,'showcase');
const manifest=JSON.parse(fs.readFileSync(path.join(out,'manifest.json')));
const currentUi=recordingUiHash(root);
const hash=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const clips=manifest.features.map(feature=>{
  const recording=feature.recording;
  if(recording.status!=='recorded'||recording.capture_profile!==CAPTURE_PROFILE||recording.ui_source_sha256!==currentUi)throw Error('Fresh immersive footage required: '+feature.id);
  if(hash(path.join(out,recording.video))!==recording.sha256||recording.privacy?.sha256!==recording.sha256||!recording.privacy.approved)throw Error('Unverified footage: '+feature.id);
  if(Object.values(recording.overlays).some(Boolean))throw Error('Recording overlays present: '+feature.id);
  const probe=spawnSync('ffprobe',['-v','error','-show_entries','stream=codec_type,width,height,avg_frame_rate','-of','json',path.join(out,recording.video)],{windowsHide:true,encoding:'utf8'});
  if(probe.status!==0)throw Error('Video metadata cannot be read: '+feature.id);
  const streams=JSON.parse(probe.stdout).streams;
  if(streams.length!==1||streams[0].codec_type!=='video')throw Error('Promotional source must contain video only: '+feature.id);
  const stream=streams[0];
  return {id:feature.id,title:feature.title,category:feature.section,video:recording.video,poster:recording.poster,optional_captions:recording.captions,seconds:recording.duration,dimensions:{width:stream.width,height:stream.height},fps:stream.avg_frame_rate,bytes:recording.bytes,sha256:recording.sha256,recorded_at:recording.recorded_at,steps:recording.steps,wait_edit:recording.editing||null};
});
fs.writeFileSync(path.join(out,'promo-clips.json'),JSON.stringify({schema:1,date:manifest.date,capture_profile:CAPTURE_PROFILE,ui_source_sha256:currentUi,burned_text:false,audio:false,captions_default:'off',clips},null,2)+'\n');
console.log(`Promotional handoff: ${clips.length} clean clips; no duplicated video files`);
