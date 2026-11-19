'use strict';
// Actual CPU-only recognition of the pinned upstream JFK fixture through an
// isolated, authenticated kernel. No user audio or local LLM is inspected.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {spawnSync}=require('node:child_process');
const {startKernel}=require('./v006-refactor-host.cjs');
const {api,settle,sha}=require('./node-runtime-fixture.cjs');
const {upload,pcm,tone}=require('./media-runtime-http.cjs');
const ROOT=path.resolve(__dirname,'../..'),FIXTURE_SHA256='59dfb9a4acb36fe2a2affc14bacbee2920ff435cb13cc314a08c13f66ba7860e';
function options(args){const values={},allowed=['--bin','--prepared','--lock','--ffmpeg-prepared','--ffmpeg-lock','--fixture','--evidence'];for(let at=0;at<args.length;at+=2){if(!allowed.includes(args[at])||!args[at+1]||args[at+1].startsWith('--')||values[args[at]])throw Error('Expected explicit kernel, Whisper/FFmpeg prepared directories and locks, upstream fixture and new evidence directory');values[args[at]]=path.resolve(args[at+1]);}if(allowed.some(key=>!values[key]))throw Error('All seven media acceptance inputs are required');if(fs.existsSync(values['--evidence']))throw Error('Preserve existing Whisper evidence');return values;}
async function main(args=process.argv.slice(2)){
  const values=options(args),{inspectPreparedMedia}=await import('../lib/media-runtime.mjs');
  const proof=await inspectPreparedMedia(values['--prepared'],{lock:values['--lock']}),decoder=await inspectPreparedMedia(values['--ffmpeg-prepared'],{lock:values['--ffmpeg-lock']});
  assert.equal(proof.entry.id,'whisper');assert.equal(proof.entry.platform,'macos-arm64');assert.equal(process.platform,'darwin');assert.equal(process.arch,'arm64');assert.equal(decoder.entry.id,'media-ffmpeg');
  const fixture=fs.readFileSync(values['--fixture']);assert.equal(sha(fixture),FIXTURE_SHA256,'Only the pinned official JFK sample is accepted');const durationMs=pcm(fixture).samples/16;assert(Number.isInteger(durationMs));
  const evidence=values['--evidence'];fs.mkdirSync(evidence,{mode:0o700});
  const assets=path.join(evidence,'assets'),profile=path.join(evidence,'profile'),documents=path.join(evidence,'documents'),tmp=path.join(evidence,'tmp'),empty=path.join(evidence,'empty-path');for(const dir of [profile,documents,tmp,empty])fs.mkdirSync(dir);
  fs.cpSync(path.join(ROOT,'assets'),assets,{recursive:true,errorOnExist:true,force:false});
  const catalogFile=path.join(assets,'capabilities','catalog.json'),catalog=JSON.parse(fs.readFileSync(catalogFile));
  for(const row of [decoder,proof]){catalog.offlinePacks=[...(catalog.offlinePacks||[]).filter(old=>!(old.id===row.entry.id&&old.platform===row.entry.platform)),row.entry];fs.writeFileSync(path.join(assets,'capabilities',row.entry.id+'-'+row.entry.version+'-'+row.entry.platform+'.lock.json'),JSON.stringify(row.manifest,null,2)+'\n');}fs.writeFileSync(catalogFile,JSON.stringify(catalog,null,2)+'\n');
  const binary=path.join(evidence,'readmd');fs.copyFileSync(values['--bin'],binary,fs.constants.COPYFILE_EXCL);fs.chmodSync(binary,0o700);const sample=path.join(documents,'官方短语音 JFK.wav');fs.writeFileSync(sample,fixture);
  const silence=path.join(documents,'250ms 静音.wav'),silent=tone(440,0.25);silent.fill(0,44);fs.writeFileSync(silence,silent);
  const report={schema:1,passed:false,id:proof.entry.id,version:proof.entry.version,platform:proof.entry.platform,packSha256:proof.entry.sha256,manifestSha256:proof.entry.manifestSha256,decoderPackSha256:decoder.entry.sha256,binarySha256:sha(fs.readFileSync(binary)),fixtureSha256:FIXTURE_SHA256,fixtureDurationMs:durationMs,engineSynthetic:false,localLlmModelsRequested:false,gpuRequested:false,sourceAssetsChanged:false,published:false,checks:[],whisperPeakRssKiB:0};
  let host,installed=new Set(),phase='startup',sampler;
  const original={TMPDIR:process.env.TMPDIR,TMP:process.env.TMP,TEMP:process.env.TEMP};Object.assign(process.env,{TMPDIR:tmp,TMP:tmp,TEMP:tmp});
  const mark=value=>{phase=value;console.log(JSON.stringify({phase}));},pack=body=>api(host,'/api/capabilities/packs',body),media=body=>api(host,'/api/transcribe',body);
  async function start(){const previous=process.env.PATH;process.env.PATH=empty;try{host=await startKernel(binary,profile,documents,{assets,requestTimeoutMs:120000});}finally{if(previous===undefined)delete process.env.PATH;else process.env.PATH=previous;}}
  function sampleMemory(){if(!host)return;try{const result=spawnSync('/bin/ps',['-axo','pid=,ppid=,rss=,comm='],{encoding:'utf8',timeout:1000,maxBuffer:2*1048576});const rows=result.stdout.split('\n').map(line=>line.match(/^\s*(\d+)\s+(\d+)\s+(\d+)\s+(.+)$/)).filter(Boolean).map(m=>({pid:Number(m[1]),parent:Number(m[2]),rss:Number(m[3]),name:path.basename(m[4])}));const owned=new Set([host.pid]);for(let at=0;at<4;at++)for(const row of rows)if(owned.has(row.parent))owned.add(row.pid);for(const row of rows)if(owned.has(row.pid)&&row.name==='whisper-cli')report.whisperPeakRssKiB=Math.max(report.whisperPeakRssKiB,row.rss);}catch{}}
  try{
    mark('managed decoder and missing recognizer');await start();sampler=setInterval(sampleMemory,100);
    report.decoderInstall=await upload(host,decoder);assert.equal(report.decoderInstall.phase,'ready',report.decoderInstall.error_code);installed.add(decoder.entry.id);
    assert.equal((await media({path:sample,operation:'transcribe',track:0,start_ms:0,end_ms:durationMs,language:'en'})).body.error_code,'speech_engine_unavailable');
    mark('managed Whisper upload and real model installation probe');report.installReceipt=await upload(host,proof);assert.equal(report.installReceipt.phase,'ready',report.installReceipt.error_code);installed.add(proof.entry.id);
    report.statusAfterInstall=(await pack({action:'status',id:proof.entry.id})).body;assert.equal(report.statusAfterInstall.ready,true);
    const inspected=await media({path:sample,operation:'inspect'});assert.equal(inspected.status,200,inspected.body.error_code);assert.equal(inspected.body.recognizer.engine,'whisper.cpp');assert.equal(inspected.body.recognizer.source,'managed-package');assert.equal(inspected.body.recognizer.available,true);report.recognizer=inspected.body.recognizer;
    mark('short silence and short real speech');
    const quiet=await media({path:silence,operation:'transcribe',track:0,start_ms:0,end_ms:250,language:'en'});assert.equal(quiet.body.error_code,'speech_no_result');report.shortSilence=quiet.body;
    const short=await media({path:sample,operation:'transcribe',track:0,start_ms:0,end_ms:3000,language:'en'});assert.equal(short.status,200,short.body.error_code);assert(short.body.content.toLowerCase().includes('americans'));for(const cue of short.body.cues)assert(cue.end_ms<=3000);report.shortSpeech=short.body;
    report.checks.push('A 250-ms silent selection returns the existing no-speech result; a real three-second speech selection transcribes within its source range.');
    mark('actual official speech recognition');const began=performance.now();const result=await media({path:sample,operation:'transcribe',source_revision:inspected.body.source_revision,track:0,start_ms:0,end_ms:durationMs,language:'en'});report.recognitionMs=performance.now()-began;assert.equal(result.status,200,result.body.error_code);assert.equal(result.body.engine,'whisper.cpp');assert.equal(result.body.language,'en');assert(result.body.cues.length>0);assert(result.body.content.toLowerCase().includes('country'));assert(result.body.vtt.startsWith('WEBVTT\n'));for(const cue of result.body.cues){assert(cue.start_ms>=0);assert(cue.end_ms<=durationMs);assert(cue.start_ms<cue.end_ms);}report.transcript=result.body;
    report.checks.push('Normal chunk installation loads the exact fixed ASR model in a CPU-only engine; the official speech fixture yields real text, source-clock cues and VTT with an empty host PATH.');
    mark('fresh process and durable model');await host.stop();host=null;await start();const fresh=await media({path:sample,operation:'inspect'});assert.equal(fresh.body.recognizer.engine,'whisper.cpp');assert.equal(fresh.body.recognizer.available,true);report.freshProcessRecognizer=fresh.body.recognizer;
    mark('normal uninstall and unknown note preservation');
    const directory=path.join(profile,'capabilities','managed-v2','whisper',proof.entry.version),note=path.join(directory,'user-note.txt');fs.writeFileSync(note,'owned QA note preserved');
    const removing=await pack({action:'uninstall_start',id:proof.entry.id});assert.equal(removing.status,200,removing.body.error_code);report.uninstallReceipt=await settle(host,removing.body);assert.equal(report.uninstallReceipt.phase,'removed',report.uninstallReceipt.error_code);installed.delete(proof.entry.id);assert.equal(fs.readFileSync(note,'utf8'),'owned QA note preserved');assert(!fs.existsSync(path.join(directory,'models','ggml-base-q5_1.bin')));assert.equal((await pack({action:'status',id:proof.entry.id})).body.ready,false);assert.equal((await media({path:sample,operation:'transcribe',track:0,start_ms:0,end_ms:durationMs,language:'en'})).body.error_code,'speech_engine_unavailable');
    report.checks.push('A fresh process discovers the durable managed recognizer; normal uninstall removes the pinned model/executable, preserves unknown notes, and disables recognition.');
    assert(report.whisperPeakRssKiB>0,'The real owned Whisper process was observed');assert(report.whisperPeakRssKiB<1024*1024,'Whisper test exceeded its one-GiB RSS budget');report.passed=true;
  }catch(error){report.failure={phase,name:error.name,message:String(error.message).replaceAll(evidence,'<evidence>'),stack:String(error.stack).replaceAll(evidence,'<evidence>').replaceAll(ROOT,'<repo>').slice(0,5000)};process.exitCode=1;}
  finally{
    clearInterval(sampler);report.cleanupErrors=[];
    for(const id of ['whisper','media-ffmpeg'])if(installed.has(id)&&host)try{const removing=await pack({action:'uninstall_start',id});const result=await settle(host,removing.body);assert.equal(result.phase,'removed',result.error_code);installed.delete(id);}catch(error){report.cleanupErrors.push(String(error.message));}
    if(host)try{await host.stop();}catch(error){report.cleanupErrors.push(String(error.message));}
    for(const [key,value] of Object.entries(original))if(value===undefined)delete process.env[key];else process.env[key]=value;
    if(report.cleanupErrors.length){report.passed=false;process.exitCode=1;}
    const bytes=Buffer.from(JSON.stringify(report,null,2)+'\n');fs.writeFileSync(path.join(evidence,'native-report.json'),bytes,{flag:'wx'});
    if(report.passed)fs.writeFileSync(path.join(evidence,'native-receipt.json'),JSON.stringify({schema:1,id:proof.entry.id,version:proof.entry.version,platform:proof.entry.platform,sha256:proof.entry.sha256,manifestSha256:proof.entry.manifestSha256,install:true,engineOperation:true,uninstall:true,synthetic:false,evidenceFile:'native-report.json',evidenceSha256:sha(bytes)},null,2)+'\n',{flag:'wx'});
    console.log(JSON.stringify({passed:report.passed,recognitionMs:report.recognitionMs,whisperPeakRssKiB:report.whisperPeakRssKiB,checks:report.checks,failure:report.failure,published:false}));
  }
  return report;
}
if(require.main===module)main().catch(error=>{console.error('Whisper acceptance failed: '+error.message);process.exitCode=1;});
module.exports={main,options};
