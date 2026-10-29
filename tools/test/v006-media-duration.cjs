'use strict';
// Synthetic speech only; explicit prebuilt host. No microphone, playback, model
// downloads, compiler, private profile, provider or desktop automation.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const net = require('node:net');
const assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const ROOT = path.resolve(__dirname, '../..');
const hash = value => crypto.createHash('sha256').update(value).digest('hex');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));

async function processBytes(executable, args, input = '', timeout = 45000, max = 12 * 1024 * 1024) {
  const child = spawn(executable, args, { cwd: ROOT, windowsHide: true, stdio: ['pipe', 'pipe', 'ignore'] });
  let output = [], bytes = 0, exceeded = false;
  child.stdout.on('data', chunk => {
    bytes += chunk.length;
    if (bytes > max) { exceeded = true; child.kill(); }
    else output.push(chunk);
  });
  child.stdin.on('error', () => {});
  child.stdin.end(input);
  let timer;
  const result = await new Promise((resolve, reject) => {
    timer = setTimeout(() => { child.kill(); reject(Error('fixture process deadline')); }, timeout);
    child.once('error', () => reject(Error('fixture dependency unavailable')));
    child.once('close', code => resolve(code));
  }).finally(() => clearTimeout(timer));
  assert(!exceeded, 'fixture output byte limit');
  assert.equal(result, 0, 'fixture process failed');
  return Buffer.concat(output);
}

function ownedRun(prefix) {
  assert.equal(process.platform, 'win32', 'This acceptance requires a real Windows speech engine');
  assert(process.env.READMD_BIN, 'Explicit prebuilt READMD_BIN is required');
  const binary = path.resolve(process.env.READMD_BIN);
  assert(fs.statSync(binary).isFile(), 'READMD_BIN must be an existing binary');
  const cache = path.join(ROOT, '.cache', 'capability-upgrade');
  const base = path.resolve(process.env.READMD_V006_MEDIA_OUTPUT || path.join(cache, 'v006-media-duration'));
  const relative = path.relative(cache, base);
  assert(relative && !relative.startsWith('..' + path.sep) && relative !== '..' && !path.isAbsolute(relative), 'Owned evidence root');
  fs.mkdirSync(base, { recursive: true });
  const output = fs.mkdtempSync(path.join(base, prefix + '-'));
  const documents = path.join(output, 'documents'), profile = path.join(output, 'profile');
  fs.mkdirSync(documents); fs.mkdirSync(profile);
  const report = { passed: false, synthetic: true, installedEngine: 'Windows System.Speech', managedSpeechPack: false,
    automaticRetries: 0, checks: [], startedAt: new Date().toISOString(),
    binarySha256: hash(fs.readFileSync(binary)),
    speechSourceSha256: hash(fs.readFileSync(path.join(ROOT, 'rust/readmd-kernel/src/speech.rs'))),
    bootSha256: hash(fs.readFileSync(path.join(ROOT, 'assets/readmd.boot.js'))) };
  return { output, documents, profile, binary, report };
}

// Independent fixture oracle: inspect all RIFF chunks and count complete PCM
// frames. This never uses the product's duration or phrase timestamps.
function pcmWave(bytes) {
  assert.equal(bytes.toString('ascii', 0, 4), 'RIFF');
  assert.equal(bytes.toString('ascii', 8, 12), 'WAVE');
  assert.equal(bytes.readUInt32LE(4) + 8, bytes.length);
  let fmt, data;
  for (let at = 12; at < bytes.length;) {
    assert(at + 8 <= bytes.length, 'fixture chunk header');
    const size = bytes.readUInt32LE(at + 4), end = at + 8 + size;
    assert(end <= bytes.length, 'fixture chunk payload');
    const id = bytes.toString('ascii', at, at + 4);
    if (id === 'fmt ') fmt = bytes.subarray(at + 8, end);
    if (id === 'data') { assert(!data, 'single fixture PCM data chunk'); data = bytes.subarray(at + 8, end); }
    at = end + size % 2;
  }
  assert(fmt && fmt.length >= 16 && data, 'fixture format and data');
  assert.equal(fmt.readUInt16LE(0), 1); assert.equal(fmt.readUInt16LE(2), 1);
  assert.equal(fmt.readUInt32LE(4), 16000); assert.equal(fmt.readUInt16LE(12), 2); assert.equal(fmt.readUInt16LE(14), 16);
  assert.equal(data.length % 2, 0);
  return { fmt: fmt.subarray(0, 16), data, frames: data.length / 2, seconds: data.length / 32000 };
}

async function fixture(run) {
  const generated = path.join(run.output, 'synthetic-voice.wav');
  const script = String.raw`
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Speech
$request=[Console]::In.ReadToEnd()|ConvertFrom-Json
$recognizers=@([System.Speech.Recognition.SpeechRecognitionEngine]::InstalledRecognizers()|Where-Object {$_.Culture.TwoLetterISOLanguageName -eq 'en'})
if(!$recognizers.Count){throw 'English recognizer unavailable'}
$speaker=New-Object System.Speech.Synthesis.SpeechSynthesizer
$voice=$speaker.GetInstalledVoices()|Where-Object {$_.VoiceInfo.Culture.TwoLetterISOLanguageName -eq 'en'}|Select-Object -First 1
if(!$voice){throw 'English fixture voice unavailable'}
$speaker.SelectVoice($voice.VoiceInfo.Name)
$format=New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(16000,[System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen,[System.Speech.AudioFormat.AudioChannel]::Mono)
$speaker.SetOutputToWaveFile([string]$request.path,$format)
$speaker.SpeakSsml('<speak version="1.0" xmlns="http://www.w3.org/2001/10/synthesis" xml:lang="en-GB">This is a reading document.<break time="1500ms"/>The application converts files to markdown.</speak>')
$speaker.Dispose()
`;
  await processBytes('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script], JSON.stringify({ path: generated }));
  const voice = pcmWave(fs.readFileSync(generated));
  const leadingFrames = 16000, tailFrames = 8 * 16000;
  const data = Buffer.concat([Buffer.alloc(leadingFrames * 2), voice.data, Buffer.alloc(tailFrames * 2)]);
  const header = Buffer.alloc(44);
  header.write('RIFF', 0); header.writeUInt32LE(36 + data.length, 4); header.write('WAVEfmt ', 8);
  header.writeUInt32LE(16, 16); voice.fmt.copy(header, 20); header.write('data', 36); header.writeUInt32LE(data.length, 40);
  const source = path.join(run.documents, 'Synthetic-speech.wav');
  fs.writeFileSync(source, Buffer.concat([header, data]));
  const result = { source, seconds: data.length / 32000, frames: data.length / 2, leadingFrames, tailFrames,
    sourceSha256: hash(fs.readFileSync(source)) };
  run.report.fixture = { pcmFrames: result.frames, sampleRate: 16000, leadingSilentFrames: leadingFrames,
    trailingSilentFrames: tailFrames, durationSeconds: result.seconds, sha256: result.sourceSha256 };
  return result;
}

async function freePort() {
  const server = net.createServer();
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  const port = server.address().port;
  await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
  return port;
}

async function startHost(run, environment = {}) {
  const port = await freePort(), origin = 'http://127.0.0.1:' + port;
  const env = { ...process.env, READMD_DATA_DIR: run.profile, READMD_WORKSPACE: run.documents,
    READMD_ASSETS_DIR: path.join(ROOT, 'assets'), READMD_EXTRA_ROOTS: '', READMD_REQUIRE_TOKEN: '1', ...environment };
  if (Object.hasOwn(environment, 'PATH')) for (const key of Object.keys(env)) {
    if (key.toLowerCase() === 'path' && key !== 'PATH') delete env[key];
  }
  delete env.READMD_APP_TOKEN;
  const child = spawn(run.binary, ['--no-window', '--port', String(port), '--host', '127.0.0.1', '--require-token',
    '--data-dir', run.profile, '--workspace', run.documents, '--assets', path.join(ROOT, 'assets')],
  { cwd: ROOT, windowsHide: true, stdio: 'ignore', env });
  let launchError, token;
  child.once('error', () => { launchError = true; });
  const closed = new Promise(resolve => child.once('close', (code, signal) => resolve({ code, signal })));
  const deadline = Date.now() + 30000;
  try {
    while (!token) {
      assert(!launchError, 'HTTP host launch failed'); assert.equal(child.exitCode, null, 'HTTP host exited before readiness');
      assert(Date.now() < deadline, 'HTTP readiness deadline');
      try {
        const response = await fetch(origin + '/', { signal: AbortSignal.timeout(1500) });
        if (response.ok) token = /<meta name="readmd-app-token" content="([^"]+)"/.exec(await response.text())?.[1];
      } catch {}
      if (!token) await delay(100);
    }
  } catch (error) { child.kill(); await closed; throw error; }
  async function api(method, route, payload) {
    const response = await fetch(origin + route, { method, headers: { 'x-readmd-token': token, 'x-readmd-app-token': token,
      ...(payload === undefined ? {} : { 'content-type': 'application/json' }) },
    body: payload === undefined ? undefined : JSON.stringify(payload), signal: AbortSignal.timeout(90000) });
    return { status: response.status, data: await response.json() };
  }
  async function stop() {
    if (child.exitCode === null) child.kill();
    let timer;
    try { return await Promise.race([closed, new Promise((_, reject) => { timer = setTimeout(() => reject(Error('owned host did not exit')), 5000); })]); }
    finally { clearTimeout(timer); }
  }
  // This operator-only hardening header stays in memory and is never reported.
  return { child, origin, api, stop, browserHeaders: { 'x-readmd-token': token } };
}

function timestamp(seconds) {
  const total = Math.floor(seconds), s = String(total % 60).padStart(2, '0'), m = String(Math.floor(total / 60) % 60).padStart(2, '0');
  return total < 3600 ? m + ':' + s : String(Math.floor(total / 3600)).padStart(2, '0') + ':' + m + ':' + s;
}
function duration(content) {
  const value = /^duration:\s*"([\d:]+)"\s*$/m.exec(content)?.[1];
  assert(value, 'real recognized document includes media duration');
  return value;
}
function recognized(content, requireTimestamps = true) {
  assert.match(content, /^model:\s*"system-speech"\s*$/m, 'real installed engine metadata');
  if (requireTimestamps) assert.match(content, /\*\*\[\d{2}:\d{2}(?::\d{2})?\]\*\*\s+\S/, 'real recognized segments');
  else {
    // The existing enabled profile can intentionally hide visible timestamps.
    const body = content.replace(/^---[\s\S]*?^---\s*$/m, '').split('\n')
      .map(line => line.trim()).filter(line => line && !/^[#>]/.test(line)).join(' ');
    assert.match(body, /\b[a-z]+(?:\s+[a-z]+){2}/i, 'real recognized synthetic speech remains after profile formatting');
  }
  assert(!content.includes('status: "unprocessed"'), 'a missing-engine notice is not transcription success');
}
function record(run, name) { run.report.checks.push({ name, passed: true }); }
function failure(run, phase, error) {
  const entry = { phase, error: String(error.message).replaceAll(run.output, '<fixture>').replaceAll(ROOT, '<workspace>').slice(0, 600) };
  if (!run.report.firstFailure) run.report.firstFailure = entry;
  else (run.report.secondaryFailures ||= []).push(entry);
  process.exitCode = 1;
}

async function observeRecognitionChild(parentPid) {
  // Inspect only children of our owned kernel. Never collect command lines or
  // enumerate the user's unrelated processes in a report.
  const script = String.raw`
$ErrorActionPreference='Stop'
$ownedParent=[int]([Console]::In.ReadToEnd()|ConvertFrom-Json).parent
[Console]::WriteLine('ready')
$deadline=[DateTime]::UtcNow.AddSeconds(25)
while([DateTime]::UtcNow -lt $deadline) {
  $worker=Get-CimInstance Win32_Process -Filter ("ParentProcessId={0} AND Name='powershell.exe'" -f $ownedParent)|Select-Object -First 1
  if($worker){@{pid=[int]$worker.ProcessId;created=$worker.CreationDate.ToUniversalTime().ToString('o')}|ConvertTo-Json -Compress;exit 0}
  Start-Sleep -Milliseconds 20
}
exit 2
`;
  const child = spawn('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script],
    { windowsHide: true, stdio: ['pipe', 'pipe', 'ignore'] });
  child.stdin.on('error', () => {}); child.stdin.end(JSON.stringify({ parent: parentPid }));
  let readyResolve, readyReject, observedResolve, observedReject, buffer = '', wasReady = false;
  const ready = new Promise((resolve, reject) => { readyResolve = resolve; readyReject = reject; });
  const observed = new Promise((resolve, reject) => { observedResolve = resolve; observedReject = reject; });
  // The observation can fail before the caller finishes its start request.
  observed.catch(() => {});
  child.stdout.setEncoding('utf8');
  child.stdout.on('data', text => {
    buffer += text;
    if (buffer.length > 4096) { child.kill(); return; }
    for (;;) {
      const at = buffer.indexOf('\n'); if (at < 0) break;
      const line = buffer.slice(0, at).trim(); buffer = buffer.slice(at + 1);
      if (line === 'ready') { wasReady = true; readyResolve(); }
      else if (line) { try { observedResolve(JSON.parse(line)); } catch { observedReject(Error('owned worker observation failed')); } }
    }
  });
  const done = new Promise(resolve => child.once('close', resolve));
  child.once('error', () => { readyReject(Error('owned worker observer unavailable')); observedReject(Error('owned worker observer unavailable')); });
  child.once('close', code => { if (!wasReady) readyReject(Error('owned worker observer did not start')); if (code !== 0) observedReject(Error('No actual recognition process observed')); });
  await ready;
  return { observed, async stop() { if (child.exitCode === null) child.kill(); await done; } };
}

async function workerExists(worker) {
  const script = String.raw`
$ErrorActionPreference='Stop'
$owned=([Console]::In.ReadToEnd()|ConvertFrom-Json)
$worker=Get-CimInstance Win32_Process -Filter ("ProcessId={0} AND Name='powershell.exe'" -f [int]$owned.pid)
@{exists=[bool]($worker -and $worker.CreationDate.ToUniversalTime().ToString('o') -eq $owned.created)}|ConvertTo-Json -Compress
`;
  const result = await processBytes('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script], JSON.stringify(worker), 10000, 4096);
  return JSON.parse(result.toString('utf8').trim()).exists;
}

async function cancellation(run, host, source) {
  const copy = path.join(run.documents, 'Cancel-speech.wav'); fs.copyFileSync(source, copy);
  const observer = await observeRecognitionChild(host.child.pid);
  try {
    const started = await host.api('POST', '/api/convert/batch', { paths: [copy], confirm: true, language: 'en' });
    assert.equal(started.status, 200); assert.equal(typeof started.data.job, 'string');
    const worker = await observer.observed;
    assert(worker && Number.isInteger(worker.pid) && worker.created, 'Actual child process identity');
    const beforeCancel = Date.now();
    const cancelled = await host.api('POST', '/api/convert/cancel', { job: started.data.job });
    assert.equal(cancelled.status, 200); assert.equal(cancelled.data.ok, true);
    let final;
    for (;;) {
      const progress = await host.api('GET', '/api/convert/progress?job=' + encodeURIComponent(started.data.job));
      assert.equal(progress.status, 200);
      if (progress.data.finished) { final = progress.data; break; }
      assert(Date.now() - beforeCancel < 10000, 'Cancellation completion deadline'); await delay(50);
    }
    assert.equal(final.items[0].status, 'canceled'); assert.equal(final.running, false);
    assert(!fs.existsSync(path.join(run.documents, 'Cancel-speech.md')), 'Cancellation commits no transcript');
    assert.equal(await workerExists(worker), false, 'The actual owned recognition process was reaped');
    run.report.cancellation = { observedRealRecognitionChild: true, ownedWorkerReaped: true,
      completedMilliseconds: Date.now() - beforeCancel, transcriptWritten: false };
  } finally { await observer.stop(); }
}
function writeReport(run) {
  run.report.finishedAt = new Date().toISOString();
  const file = path.join(run.output, 'report.json');
  fs.writeFileSync(file, JSON.stringify(run.report, null, 2));
  console.log(JSON.stringify({ passed: run.report.passed, checks: run.report.checks.length,
    firstFailure: run.report.firstFailure?.phase, report: path.relative(ROOT, file) }));
}

async function main() {
  const run = ownedRun('service'); let host, directHost, phase = 'synthetic fixture';
  run.report.surface = 'real loopback service and installed Windows recognizer';
  try {
    const media = await fixture(run); host = await startHost(run);
    phase = 'WAV full media duration';
    const first = await host.api('POST', '/api/transcribe', { path: media.source, language: 'en' });
    assert.equal(first.status, 200); assert.equal(first.data.ok, true); recognized(first.data.content);
    run.report.observedDuration = duration(first.data.content);
    run.report.recognitionWithoutDurationSha256 = hash(first.data.content.replace(/^duration:[^\n]*\n\n?/m, ''));
    run.report.recognizedTimestamps = [...first.data.content.matchAll(/\*\*\[(\d{2}:\d{2}(?::\d{2})?)\]\*\*/g)].map(match => match[1]);
    assert.equal(run.report.observedDuration, timestamp(media.seconds), 'Media duration must include the eight seconds of trailing silence');
    record(run, 'Real WAV transcription reports all PCM frames, including leading and trailing silence');
    phase = 'existing conversion preview';
    const preview = await host.api('GET', '/api/convert?p=' + encodeURIComponent(media.source) + '&preview=1&language=en');
    assert.equal(preview.status, 200); assert.equal(preview.data.saved, false); recognized(preview.data.content, false);
    assert.equal(duration(preview.data.content), timestamp(media.seconds));
    assert(!fs.existsSync(path.join(run.documents, 'Synthetic-speech.md')), 'preview does not write a document');
    record(run, 'The existing file conversion path returns the correct duration without writing an output');
    phase = 'compressed input normalized frames';
    const compressed = path.join(run.documents, 'Synthetic-speech.mp3');
    await processBytes('ffmpeg', ['-nostdin', '-v', 'error', '-y', '-i', media.source, compressed]);
    const decoded = await processBytes('ffmpeg', ['-nostdin', '-v', 'error', '-i', compressed, '-vn', '-ac', '1', '-ar', '16000', '-f', 's16le', '-c:a', 'pcm_s16le', 'pipe:1']);
    assert.equal(decoded.length % 2, 0); const decodedSeconds = decoded.length / 32000;
    const mp3 = await host.api('POST', '/api/transcribe', { path: compressed, language: 'en' });
    assert.equal(mp3.status, 200); recognized(mp3.data.content); assert.equal(duration(mp3.data.content), timestamp(decodedSeconds));
    run.report.compressed = { decodedFrames: decoded.length / 2, sampleRate: 16000, expectedDuration: timestamp(decodedSeconds) };
    record(run, 'MP3 duration is measured from the PCM stream actually supplied to the recognizer');
    phase = 'video longer than its audio track';
    const video = path.join(run.documents, 'Synthetic-video.mp4');
    await processBytes('ffmpeg', ['-nostdin', '-v', 'error', '-y', '-f', 'lavfi', '-i', 'color=c=black:s=16x16:r=1:d=21',
      '-i', media.source, '-map', '0:v:0', '-map', '1:a:0', '-c:v', 'mpeg4', '-c:a', 'aac', video]);
    const videoPcm = await processBytes('ffmpeg', ['-nostdin', '-v', 'error', '-i', video, '-vn', '-ac', '1', '-ar', '16000', '-f', 's16le', '-c:a', 'pcm_s16le', 'pipe:1']);
    assert.equal(videoPcm.length % 2, 0); const audioSeconds = videoPcm.length / 32000;
    assert(audioSeconds < 20, 'The synthetic video has an intentionally longer image track');
    const movie = await host.api('POST', '/api/transcribe', { path: video, language: 'en' });
    assert.equal(movie.status, 200); recognized(movie.data.content); assert.equal(duration(movie.data.content), timestamp(audioSeconds));
    run.report.video = { imageTrackSeconds: 21, decodedAudioFrames: videoPcm.length / 2, expectedAudioDuration: timestamp(audioSeconds) };
    record(run, 'Video transcription duration describes its actual decoded audio stream, not a longer image track');
    phase = 'corrupt media rejection';
    const corrupt = path.join(run.documents, 'Corrupt.wav'); fs.writeFileSync(corrupt, 'RIFF truncated synthetic fixture');
    const rejected = await host.api('POST', '/api/transcribe', { path: corrupt, language: 'en' });
    assert.equal(rejected.status, 422); assert.equal(rejected.data.ok, false); assert(!rejected.data.content);
    assert(!fs.existsSync(path.join(run.documents, 'Corrupt.md')));
    record(run, 'Damaged media fails without returning or writing a fabricated transcription');
    phase = 'actual recognition cancellation';
    await cancellation(run, host, media.source);
    record(run, 'Cancel while the real recognition child exists reaps that child and writes no transcript');
    phase = 'explicit save and actual reopen';
    const destination = path.join(run.documents, 'Saved-transcript.md'); fs.writeFileSync(destination, '');
    const blank = await host.api('GET', '/api/file?p=' + encodeURIComponent(destination)); assert.equal(blank.status, 200);
    const saved = await host.api('POST', '/api/save', { path: destination, content: preview.data.content, encoding: 'utf-8',
      expected_mtime: blank.data.mtime, expected_revision: blank.data.revision });
    assert.equal(saved.status, 200); assert.equal(saved.data.ok, true);
    const reopened = await host.api('GET', '/api/file?p=' + encodeURIComponent(destination)); assert.equal(reopened.status, 200);
    const persisted = fs.readFileSync(destination, 'utf8'); assert.equal(duration(persisted), timestamp(media.seconds));
    assert.equal(reopened.data.original || reopened.data.content, persisted);
    assert.equal(hash(fs.readFileSync(media.source)), media.sourceSha256);
    record(run, 'An explicit guarded save and real reopen preserve corrected duration and original media bytes');
    phase = 'direct WAV without FFmpeg';
    const directProfile = path.join(run.output, 'no-ffmpeg-profile'); fs.mkdirSync(directProfile);
    const systemOnly = [path.join(process.env.SystemRoot, 'System32'), path.join(process.env.SystemRoot, 'System32/WindowsPowerShell/v1.0')].join(path.delimiter);
    directHost = await startHost({ ...run, profile: directProfile }, { PATH: systemOnly });
    const direct = await directHost.api('POST', '/api/transcribe', { path: media.source, language: 'en' });
    assert.equal(direct.status, 200); recognized(direct.data.content); assert.equal(duration(direct.data.content), timestamp(media.seconds));
    const missingDecoder = await directHost.api('POST', '/api/transcribe', { path: compressed, language: 'en' });
    assert.equal(missingDecoder.status, 422); assert.equal(missingDecoder.data.ok, false); assert(!missingDecoder.data.content);
    assert.equal(missingDecoder.data.error_detail, 'ffmpeg_unavailable');
    const malformed = Buffer.from(fs.readFileSync(media.source)); malformed.writeUInt32LE(malformed.length + 1000, 40);
    const malformedPath = path.join(run.documents, 'Truncated-pcm.wav'); fs.writeFileSync(malformedPath, malformed);
    const invalidPcm = await directHost.api('POST', '/api/transcribe', { path: malformedPath, language: 'en' });
    assert.equal(invalidPcm.status, 422); assert.equal(invalidPcm.data.ok, false); assert(!invalidPcm.data.content);
    assert.match(invalidPcm.data.error_detail, /invalid_wave_duration/);
    record(run, 'System-only PATH preserves real direct WAV recognition; compressed input reports its missing decoder and malformed PCM stays an error');
    phase = 'valid non-PCM WAV compatibility';
    const mulaw = path.join(run.documents, 'Synthetic-mulaw.wav');
    await processBytes('ffmpeg', ['-nostdin', '-v', 'error', '-y', '-i', media.source, '-ar', '8000', '-ac', '1', '-c:a', 'pcm_mulaw', mulaw]);
    const nonPcm = await directHost.api('POST', '/api/transcribe', { path: mulaw, language: 'en' });
    assert.equal(nonPcm.status, 200); assert.equal(nonPcm.data.ok, true); recognized(nonPcm.data.content);
    assert(!/^duration:/m.test(nonPcm.data.content), 'Unsupported duration encoding omits the field while retaining real recognition');
    record(run, 'A valid mu-law WAV still produces real speech without FFmpeg and omits its unproven full-duration field');
    run.report.passed = true;
  } catch (error) { failure(run, phase, error); }
  finally {
    for (const [name, owned] of [['directHostExit', directHost], ['ownedHostExit', host]]) if (owned) {
      try { run.report[name] = await owned.stop(); } catch (error) { failure(run, 'host cleanup', error); run.report.passed = false; }
    }
    writeReport(run);
  }
}

module.exports = { ROOT, hash, delay, ownedRun, fixture, startHost, timestamp, duration, recognized, record, failure, writeReport, processBytes, cancellation };
if (require.main === module) main().catch(() => { console.error('Media validation could not start'); process.exitCode = 1; });
