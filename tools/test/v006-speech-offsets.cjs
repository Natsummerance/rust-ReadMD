'use strict';
// Physical PCM frame windows and retained native result audio, compared with
// real System.Speech and the existing service. Synthetic audio only.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const qa = require('./v006-media-duration.cjs');

function wave(bytes) {
  assert.equal(bytes.toString('ascii', 0, 4), 'RIFF'); assert.equal(bytes.toString('ascii', 8, 12), 'WAVE');
  let fmt, data;
  for (let at = 12; at + 8 <= bytes.length;) {
    const size = bytes.readUInt32LE(at + 4), end = at + 8 + size; assert(end <= bytes.length, 'complete native audio chunk');
    const id = bytes.toString('ascii', at, at + 4);
    if (id === 'fmt ') fmt = bytes.subarray(at + 8, end);
    if (id === 'data') data = bytes.subarray(at + 8, end);
    at = end + size % 2;
  }
  assert(fmt && fmt.length >= 16 && data);
  assert.equal(fmt.readUInt16LE(0), 1); assert.equal(fmt.readUInt16LE(2), 1); assert.equal(fmt.readUInt32LE(4), 16000);
  assert.equal(fmt.readUInt16LE(12), 2); assert.equal(fmt.readUInt16LE(14), 16); assert.equal(data.length % 2, 0);
  return { fmt: fmt.subarray(0, 16), data, frames: data.length / 2 };
}
function writeWave(file, fmt, data) {
  const header = Buffer.alloc(44); header.write('RIFF'); header.writeUInt32LE(36 + data.length, 4); header.write('WAVEfmt ', 8);
  header.writeUInt32LE(16, 16); fmt.copy(header, 20); header.write('data', 36); header.writeUInt32LE(data.length, 40);
  fs.writeFileSync(file, Buffer.concat([header, data]));
}
function activeFrames(data) {
  let first, last;
  for (let frame = 0; frame < data.length / 2; frame++) if (Math.abs(data.readInt16LE(frame * 2)) > 16) {
    first ??= frame; last = frame;
  }
  assert(first !== undefined && last > first, 'synthetic speech has real non-silent PCM frames');
  return { first, last };
}
async function fixture(run) {
  const first = path.join(run.output, 'first.wav'), second = path.join(run.output, 'second.wav');
  const script = String.raw`
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Speech
$request=[Console]::In.ReadToEnd()|ConvertFrom-Json
foreach($item in $request.items) {
  $speaker=New-Object System.Speech.Synthesis.SpeechSynthesizer
  $voice=$speaker.GetInstalledVoices()|Where-Object {$_.VoiceInfo.Culture.TwoLetterISOLanguageName -eq 'en'}|Select-Object -First 1
  if(!$voice){throw 'English synthetic voice unavailable'}
  $speaker.SelectVoice($voice.VoiceInfo.Name)
  $format=New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(16000,[System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen,[System.Speech.AudioFormat.AudioChannel]::Mono)
  $speaker.SetOutputToWaveFile([string]$item.path,$format)
  $speaker.Speak([string]$item.text)
  $speaker.Dispose()
}
`;
  await qa.processBytes('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script], JSON.stringify({ items: [
    { path: first, text: 'This is a reading document.' }, { path: second, text: 'The application converts files to markdown.' }] }));
  const a = wave(fs.readFileSync(first)), b = wave(fs.readFileSync(second)); assert.deepEqual(a.fmt, b.fmt);
  const lead = 2 * 16000, gap = 5 * 16000, tail = 8 * 16000;
  const data = Buffer.concat([Buffer.alloc(lead * 2), a.data, Buffer.alloc(gap * 2), b.data, Buffer.alloc(tail * 2)]);
  const source = path.join(run.documents, 'Frame-reference.wav'); writeWave(source, a.fmt, data);
  const aActive = activeFrames(a.data), bActive = activeFrames(b.data), bFrom = lead + a.frames + gap;
  const windows = [{ from: lead, to: lead + a.frames, activeFrom: lead + aActive.first, activeTo: lead + aActive.last },
    { from: bFrom, to: bFrom + b.frames, activeFrom: bFrom + bActive.first, activeTo: bFrom + bActive.last }];
  const normalized = path.join(run.output, 'normalized.wav');
  await qa.processBytes('ffmpeg', ['-nostdin', '-v', 'error', '-y', '-i', source, '-vn', '-ac', '1', '-ar', '16000', '-c:a', 'pcm_s16le', normalized]);
  const decoded = wave(fs.readFileSync(normalized)); assert.deepEqual(decoded.data, data, 'WAV normalization preserves every PCM frame in this fixture');
  run.report.frameOracle = { sampleRate: 16000, totalFrames: data.length / 2, windows, leadFrames: lead, gapFrames: gap, tailFrames: tail,
    sourcePcmSha256: qa.hash(data), normalizedPcmSha256: qa.hash(decoded.data), sourceSha256: qa.hash(fs.readFileSync(source)) };
  return { source, normalized, data, windows, seconds: data.length / 32000 };
}

const PROBE = String.raw`
$ErrorActionPreference='Stop'
[Console]::InputEncoding=New-Object System.Text.UTF8Encoding($false)
[Console]::OutputEncoding=New-Object System.Text.UTF8Encoding($false)
Add-Type -AssemblyName System.Speech
$request=[Console]::In.ReadToEnd()|ConvertFrom-Json
$engine=[System.Speech.Recognition.SpeechRecognitionEngine]::InstalledRecognizers()|Where-Object {$_.Culture.TwoLetterISOLanguageName -eq 'en'}|Select-Object -First 1
if(!$engine){throw 'English recognition unavailable'}
function Result-Row($result,$recognizer,$prefix,$index) {
  $audio=$result.Audio
  $name=$prefix+'-'+$index+'.wav'
  $stream=[IO.File]::Create([IO.Path]::Combine([string]$request.clips,$name))
  try{$audio.WriteToWaveStream($stream)}finally{$stream.Dispose()}
  $inputPosition=$null;$recognizerPosition=$null
  try{$inputPosition=$recognizer.AudioPosition.TotalSeconds;$recognizerPosition=$recognizer.RecognizerAudioPosition.TotalSeconds}catch{}
  @{text=$result.Text;position=$audio.AudioPosition.TotalSeconds;duration=$audio.Duration.TotalSeconds;clip=$name;
    inputPosition=$inputPosition;recognizerPosition=$recognizerPosition;
    words=@($result.Words|ForEach-Object {@{text=$_.Text;position=$_.AudioPosition.TotalSeconds;duration=$_.AudioDuration.TotalSeconds}})}
}
function Recognize-File($inputPath,$mode,$prefix) {
  $recognizer=New-Object System.Speech.Recognition.SpeechRecognitionEngine($engine.Id)
  $recognizer.LoadGrammar((New-Object System.Speech.Recognition.DictationGrammar))
  $recognizer.SetInputToWaveFile([string]$inputPath)
  $rows=New-Object 'System.Collections.Generic.List[object]'
  try {
    if($mode -eq 'sync') {
      for($i=0;$i -lt 16;$i++) {
        try{$result=$recognizer.Recognize()}
        catch{if($rows.Count -gt 0 -and $_.Exception.InnerException -is [System.InvalidOperationException]){break};throw}
        if(!$result){break}
        $rows.Add((Result-Row $result $recognizer $prefix $rows.Count))
      }
    } else {
      $recognized='readmd-recognized-'+[Guid]::NewGuid().ToString('N')
      $completed='readmd-completed-'+[Guid]::NewGuid().ToString('N')
      Register-ObjectEvent -InputObject $recognizer -EventName SpeechRecognized -SourceIdentifier $recognized|Out-Null
      Register-ObjectEvent -InputObject $recognizer -EventName RecognizeCompleted -SourceIdentifier $completed|Out-Null
      try {
        $recognizer.RecognizeAsync([System.Speech.Recognition.RecognizeMode]::Multiple)
        $deadline=[DateTime]::UtcNow.AddSeconds(30)
        while($true) {
          if([DateTime]::UtcNow -gt $deadline){throw 'native async deadline'}
          $event=Wait-Event -Timeout 1
          if(!$event){continue}
          try{
            if($event.SourceIdentifier -eq $recognized){$rows.Add((Result-Row $event.SourceEventArgs.Result $recognizer $prefix $rows.Count))}
            elseif($event.SourceIdentifier -eq $completed){if($event.SourceEventArgs.Error){throw 'native async error'};break}
          }finally{Remove-Event -EventIdentifier $event.EventIdentifier}
        }
      } finally {
        Unregister-Event -SourceIdentifier $recognized -ErrorAction SilentlyContinue
        Unregister-Event -SourceIdentifier $completed -ErrorAction SilentlyContinue
      }
    }
    @{mode=$mode;prefix=$prefix;rows=@($rows.ToArray())}
  } finally {$recognizer.Dispose()}
}
$results=@()
foreach($item in $request.inputs){foreach($mode in @('sync','multiple')){$results+=Recognize-File $item.path $mode ($item.name+'-'+$mode)}}
@{engine='system-speech';results=$results}|ConvertTo-Json -Depth 10 -Compress
`;

function documentSegments(content) {
  return [...content.matchAll(/^\*\*\[(\d{2}:\d{2}(?::\d{2})?)\]\*\*\s+(.+)$/gm)].map(match => ({ timestamp: match[1],
    seconds: match[1].split(':').map(Number).reduce((total, value) => total * 60 + value, 0), text: match[2] }));
}

async function main() {
  const sourceOnly = process.argv.includes('--source-only');
  const before = process.env.READMD_V006_MEDIA_OUTPUT;
  process.env.READMD_V006_MEDIA_OUTPUT = process.env.READMD_V006_POSITION_OUTPUT || path.join(qa.ROOT, '.cache/capability-upgrade/v006-speech-offsets');
  const run = qa.ownedRun(sourceOnly ? 'source' : 'offsets');
  if (before === undefined) delete process.env.READMD_V006_MEDIA_OUTPUT; else process.env.READMD_V006_MEDIA_OUTPUT = before;
  run.report.scope = 'Investigate physical phrase offsets; no selection UI or AI';
  run.report.compiledKernelExercised = false;
  run.report.sourceOnly = sourceOnly;
  let host, directHost, phase = 'independent PCM fixture';
  try {
    const media = await fixture(run); qa.record(run, 'Independent PCM windows and bit-identical FFmpeg normalization');
    phase = 'native synchronous and continuous result audio';
    const clips = path.join(run.output, 'native-clips'); fs.mkdirSync(clips);
    const bytes = await qa.processBytes('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', PROBE], JSON.stringify({ clips,
      inputs: [{ name: 'original', path: media.source }, { name: 'normalized', path: media.normalized }] }), 90000);
    const native = JSON.parse(bytes.toString('utf8').trim());
    for (const result of native.results) for (const row of result.rows) {
      const clip = wave(fs.readFileSync(path.join(clips, row.clip)));
      const offset = media.data.indexOf(clip.data);
      row.clipFrames = clip.frames; row.matchOffsetFrames = offset >= 0 && offset % 2 === 0 ? offset / 2 : null;
      row.clipUnique = offset >= 0 && media.data.indexOf(clip.data, offset + 1) < 0;
      row.clipPcmSha256 = qa.hash(clip.data);
      row.positionMatchesPhysicalFrames = row.matchOffsetFrames !== null && Math.abs(row.position * 16000 - row.matchOffsetFrames) <= 1;
    }
    run.report.native = native;
    assert.equal(native.results.length, 4); assert(native.results.every(result => result.rows.length >= 2), 'Each real native mode recognizes both separated phrases');
    assert(native.results.every(result => result.rows.every(row => row.matchOffsetFrames !== null && row.clipUnique)), 'Retained native audio matches a unique original PCM frame range');
    assert(native.results.filter(result => result.mode === 'multiple').every(result => result.rows.every(row => row.positionMatchesPhysicalFrames)),
      'Continuous native result positions equal independently matched physical source frames');
    qa.record(run, 'Native recognized audio has an independent, unique original PCM frame offset');
    phase = 'literal production script offsets';
    const source = fs.readFileSync(path.join(qa.ROOT, 'rust/readmd-kernel/src/speech.rs'), 'utf8');
    const script = /const SCRIPT: &str = r#"\r?\n([\s\S]*?)\r?\n"#;/.exec(source)?.[1]; assert(script, 'Actual production script literal');
    const sourceBytes = await qa.processBytes('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script],
      JSON.stringify({ path: media.source, language: 'en' }), 45000);
    const sourceResult = JSON.parse(sourceBytes.toString('utf8').trim());
    assert.equal(sourceResult.ok, true); assert.equal(sourceResult.engine, 'system-speech'); assert.equal(sourceResult.segments.length, 2);
    const reference = native.results.find(result => result.prefix === 'original-multiple').rows;
    for (let index = 0; index < 2; index++) {
      assert.equal(sourceResult.segments[index].text, reference[index].text, 'Production script preserves real recognized text');
      assert(Math.abs(sourceResult.segments[index].start * 16000 - reference[index].matchOffsetFrames) <= 1600,
        'Production script start matches independent native audio within 100ms');
    }
    run.report.productionScript = { segments: sourceResult.segments, textSha256: qa.hash(sourceResult.segments.map(row => row.text).join('\n')) };
    qa.record(run, 'The literal production PowerShell script preserves real text and source-frame offsets');
    if (sourceOnly) { run.report.passed = true; return; }
    phase = 'existing service offsets';
    host = await qa.startHost(run);
    run.report.compiledKernelExercised = true;
    const converted = await host.api('POST', '/api/transcribe', { path: media.source, language: 'en' });
    assert.equal(converted.status, 200); qa.recognized(converted.data.content);
    const withDecoder = documentSegments(converted.data.content);
    const directProfile = path.join(run.output, 'no-ffmpeg-profile'); fs.mkdirSync(directProfile);
    const systemOnly = [path.join(process.env.SystemRoot, 'System32'), path.join(process.env.SystemRoot, 'System32/WindowsPowerShell/v1.0')].join(path.delimiter);
    directHost = await qa.startHost({ ...run, profile: directProfile }, { PATH: systemOnly });
    const direct = await directHost.api('POST', '/api/transcribe', { path: media.source, language: 'en' });
    assert.equal(direct.status, 200); qa.recognized(direct.data.content);
    const withoutDecoder = documentSegments(direct.data.content);
    run.report.existingService = { withDecoder, withoutDecoder, duration: qa.duration(converted.data.content),
      contentWithoutDurationSha256: qa.hash(converted.data.content.replace(/^duration:[^\n]*\n\n?/m, '')),
      textSha256: qa.hash(withDecoder.map(row => row.text).join('\n')) };
    assert.equal(qa.duration(converted.data.content), qa.timestamp(media.seconds), 'Trailing silence duration repair remains intact');
    assert.equal(withDecoder.length, 2); assert.equal(withoutDecoder.length, 2);
    assert.deepEqual(withDecoder.map(row => row.text), reference.map(row => row.text));
    assert.deepEqual(withoutDecoder.map(row => row.text), reference.map(row => row.text));
    for (const segments of [withDecoder, withoutDecoder]) for (let index = 0; index < 2; index++) {
      const lower = Math.floor(media.windows[index].activeFrom / 16000) - 1;
      const upper = Math.ceil(media.windows[index].activeTo / 16000);
      assert(segments[index].seconds >= lower && segments[index].seconds <= upper,
        'Phrase ' + index + ' timestamp ' + segments[index].timestamp + ' must fall in its independently known PCM window [' + lower + ',' + upper + ']');
    }
    qa.record(run, 'Existing transcription reports each separated phrase in its physical source window with and without FFmpeg');
    run.report.passed = true;
  } catch (error) { qa.failure(run, phase, error); }
  finally {
    for (const [name, owned] of [['directHostExit', directHost], ['ownedHostExit', host]]) if (owned) {
      try { run.report[name] = await owned.stop(); } catch (error) { qa.failure(run, 'owned host cleanup', error); run.report.passed = false; }
    }
    qa.writeReport(run);
  }
}
module.exports = { wave, fixture, PROBE, documentSegments };
if (require.main === module) main().catch(() => { console.error('Speech offset validation could not start'); process.exitCode = 1; });
