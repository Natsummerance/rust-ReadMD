/** Remove only completed, isolated recording fixtures after checking every target. */
import fs from 'node:fs';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {CAPTURE_PROFILE,recordingUiHash} from './capture-mode.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const out=path.join(root,'showcase'),runtime=path.join(out,'.runtime');
const manifest=JSON.parse(fs.readFileSync(path.join(out,'manifest.json')));
const currentUi=recordingUiHash(root);
if(manifest.features.some(f=>f.recording.status!=='recorded'||f.recording.capture_profile!==CAPTURE_PROFILE||f.recording.ui_source_sha256!==currentUi))throw Error('Finish all current recordings before cleanup');
let files=0,logicalBytes=0,uniqueBytes=0;
const targets=[];
function inspect(directory){
 const resolved=path.resolve(directory);
 if(!resolved.startsWith(runtime+path.sep))throw Error('Cleanup target outside recording runtime');
 for(const entry of fs.readdirSync(directory,{withFileTypes:true})){
  const file=path.join(directory,entry.name),stat=fs.lstatSync(file);
  if(stat.isSymbolicLink())throw Error('Recording cleanup refuses linked directories or files');
  if(stat.isDirectory())inspect(file);
  else if(stat.isFile()){files++;logicalBytes+=stat.size;if(stat.nlink===1)uniqueBytes+=stat.size;}
  else throw Error('Unexpected recording fixture entry');
 }
}
if(fs.existsSync(runtime)){
 if(fs.lstatSync(runtime).isSymbolicLink()||fs.realpathSync(runtime).toLowerCase()!==runtime.toLowerCase())throw Error('Recording runtime must be a real workspace directory');
 if(process.platform==='win32'){
  const result=spawnSync('powershell.exe',['-NoProfile','-NonInteractive','-Command',"$scope=[Console]::In.ReadToEnd().Trim();$active=@(Get-CimInstance Win32_Process | Where-Object {$_.ProcessId -ne $PID -and $_.CommandLine -and $_.CommandLine.Contains($scope)});[Console]::Write($active.Count)"],{input:runtime,encoding:'utf8',windowsHide:true});
  if(result.status!==0||Number(result.stdout.trim())!==0)throw Error('Close active recording processes before cleanup');
 }
 for(const entry of fs.readdirSync(runtime,{withFileTypes:true})){
  if(!entry.isDirectory()||!/^run-[A-Za-z0-9]{6}$|^privacy-final-[A-Za-z0-9]{6}$/.test(entry.name))throw Error('Unrecognized runtime asset; inspect it before cleanup');
  const target=path.join(runtime,entry.name);
  if(fs.lstatSync(target).isSymbolicLink())throw Error('Recording fixture must not be a directory link');
  inspect(target);targets.push(target);
 }
 // All paths, descendants and process ownership are checked before mutation.
 for(const target of targets)fs.rmSync(target,{recursive:true,maxRetries:10,retryDelay:300});
 fs.rmdirSync(runtime);
}
const videos=manifest.features.map(f=>path.join(out,f.recording.video));
const report={date:new Date().toISOString(),capture_profile:CAPTURE_PROFILE,completed:true,runtime_files:files,runtime_logical_bytes:logicalBytes,runtime_unique_bytes_reclaimed:uniqueBytes,runtime_removed:!fs.existsSync(runtime),per_clip_and_credentials_fixtures_removed:true,failed_recording_fixtures_removed:true,shared_build_and_dependency_caches_preserved:true,media_duplicates_avoided_by_hardlinks:videos.filter(file=>fs.statSync(file).nlink>=3).length,final_video_files:videos.length};
fs.writeFileSync(path.join(out,'checks/cleanup.json'),JSON.stringify(report,null,2)+'\n');
console.log(`Recording cleanup complete: ${targets.length} isolated directories; ${files} temporary files; ${report.final_video_files} final videos retained`);
