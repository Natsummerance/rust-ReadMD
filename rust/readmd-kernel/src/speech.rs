//! Offline file transcription through the installed Windows speech engine.
//! Rust owns decoding, process limits and result validation. No Python, pip,
//! microphone capture or cloud service is involved.

use serde_json::{json, Value};
use std::{io::{Read, Seek, SeekFrom, Write}, path::Path, process::{Command, Stdio}, time::{Duration, Instant}};

thread_local! { static CANCEL: std::cell::RefCell<Option<std::sync::Arc<dyn Fn()->bool+Send+Sync>>> = std::cell::RefCell::new(None); }
pub fn with_cancel<T>(check:std::sync::Arc<dyn Fn()->bool+Send+Sync>,action:impl FnOnce()->T)->T {
    struct Reset(Option<std::sync::Arc<dyn Fn()->bool+Send+Sync>>);
    impl Drop for Reset {fn drop(&mut self){CANCEL.with(|c|*c.borrow_mut()=self.0.take());}}
    let previous=CANCEL.with(|c|c.replace(Some(check)));let _reset=Reset(previous);action()
}
pub(crate) fn cancelled()->bool {CANCEL.with(|c|c.borrow().as_ref().is_some_and(|f|f()))}
pub(crate) fn cancellation_check()->std::sync::Arc<dyn Fn()->bool+Send+Sync> {
    CANCEL.with(|c|c.borrow().clone()).unwrap_or_else(||std::sync::Arc::new(||false))
}

const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
[Console]::InputEncoding = New-Object System.Text.UTF8Encoding($false)
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
try {
  Add-Type -AssemblyName System.Speech
  $request = [Console]::In.ReadToEnd() | ConvertFrom-Json
  $engines = @([System.Speech.Recognition.SpeechRecognitionEngine]::InstalledRecognizers())
  if ($request.probe) {
    $languages = @($engines | ForEach-Object { @{code=$_.Culture.Name;name=$_.Culture.DisplayName} })
    @{ok=($engines.Count -gt 0);engine='system-speech';languages=$languages} | ConvertTo-Json -Compress -Depth 5
    exit
  }
  $language = [string]$request.language
  if (!$language -or $language -eq 'auto') { $language = [System.Globalization.CultureInfo]::CurrentUICulture.Name }
  $engine = $engines | Where-Object { $_.Culture.Name -eq $language } | Select-Object -First 1
  if (!$engine) { $engine = $engines | Where-Object { $_.Culture.TwoLetterISOLanguageName -eq $language.Split('-')[0] } | Select-Object -First 1 }
  if (!$engine) { throw 'speech_language_unavailable' }
  $recognizer = New-Object System.Speech.Recognition.SpeechRecognitionEngine($engine.Id)
  $recognizer.LoadGrammar((New-Object System.Speech.Recognition.DictationGrammar))
  $recognizer.SetInputToWaveFile([string]$request.path)
  $segments = New-Object 'System.Collections.Generic.List[object]'
  $recognized = 'readmd-recognized-' + [Guid]::NewGuid().ToString('N')
  $completed = 'readmd-completed-' + [Guid]::NewGuid().ToString('N')
  try {
    Register-ObjectEvent -InputObject $recognizer -EventName SpeechRecognized -SourceIdentifier $recognized | Out-Null
    Register-ObjectEvent -InputObject $recognizer -EventName RecognizeCompleted -SourceIdentifier $completed | Out-Null
    # Keep one recognition operation for the complete input. Repeated synchronous
    # Recognize calls restart SAPI's position origin between phrases.
    $recognizer.RecognizeAsync([System.Speech.Recognition.RecognizeMode]::Multiple)
    while ($true) {
      $event = Wait-Event -Timeout 1
      if (!$event) { continue }
      try {
        if ($event.SourceIdentifier -eq $recognized) {
          $result = $event.SourceEventArgs.Result
          if ($result) {
            $start = $result.Audio.AudioPosition.TotalSeconds
            $segments.Add(@{start=$start;text=$result.Text;confidence=[double]$result.Confidence})
          }
        } elseif ($event.SourceIdentifier -eq $completed) {
          if ($event.SourceEventArgs.Error) { throw $event.SourceEventArgs.Error }
          if ($event.SourceEventArgs.Cancelled) { throw 'cancelled' }
          break
        }
      } finally { Remove-Event -EventIdentifier $event.EventIdentifier }
    }
  } finally {
    Unregister-Event -SourceIdentifier $recognized -ErrorAction SilentlyContinue
    Unregister-Event -SourceIdentifier $completed -ErrorAction SilentlyContinue
  }
  $recognizer.Dispose()
  @{ok=$true;engine='system-speech';language=$engine.Culture.Name;segments=@($segments.ToArray())} | ConvertTo-Json -Compress -Depth 5
} catch {
  if ($recognizer) { $recognizer.Dispose() }
  @{ok=$false;code='speech_recognition_failed';error=$_.Exception.Message} | ConvertTo-Json -Compress
}
"#;

fn run(command: &mut Command, payload: &[u8], timeout: Duration) -> Result<Vec<u8>, String> {
    let mut input = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    input.write_all(payload).map_err(|e| e.to_string())?;
    let output = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    let mut child = command.stdin(Stdio::from(input.reopen().map_err(|e| e.to_string())?))
        .stdout(Stdio::from(output.reopen().map_err(|e| e.to_string())?))
        .stderr(Stdio::null()).spawn().map_err(|e| format!("speech_engine_unavailable: {e}"))?;
    let deadline = Instant::now() + timeout;
    let status = loop {
        if cancelled() {let _=child.kill();let _=child.wait();return Err("cancelled".into());}
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(40)),
            other => {
                let _ = child.kill(); let _ = child.wait();
                return Err(if other.is_err() { "speech_process_failed" } else { "speech_timeout" }.into());
            }
        }
    };
    if !status.success() { return Err("speech_process_failed".into()); }
    if output.as_file().metadata().map_err(|e| e.to_string())?.len() > 8 * 1024 * 1024 { return Err("speech_output_too_large".into()); }
    std::fs::read(output.path()).map_err(|e| e.to_string())
}

fn request(payload: Value, timeout: Duration) -> Result<Value, String> {
    if !cfg!(windows) { return Err("speech_platform_unavailable".into()); }
    let bytes = run(crate::silent_command("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT]), &serde_json::to_vec(&payload).map_err(|e| e.to_string())?, timeout)?;
    let text = String::from_utf8(bytes).map_err(|_| "speech_invalid_output")?;
    serde_json::from_str(text.trim_start_matches('\u{feff}').trim()).map_err(|_| "speech_invalid_output".into())
}

pub fn capabilities() -> Value {
    static CAPABILITIES: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    CAPABILITIES.get_or_init(|| request(json!({"probe":true}), Duration::from_secs(15))
        .unwrap_or_else(|e| json!({"ok":false,"engine":"system-speech","languages":[],"error":e}))).clone()
}

/// Complete decoded audio duration, rather than the end of the last phrase.
/// Read only RIFF headers with bounded seeks; never allocate or scan PCM data.
/// Other containers/encodings remain the installed recognizer's decision and
/// have no duration field when their sample count cannot be established here.
fn wave_duration(path: &Path) -> Result<Option<f64>, String> {
    const INVALID: &str = "audio_decode_failed: invalid_wave_duration";
    fn invalid(known_frames: Option<(u32, u16)>) -> Result<Option<f64>, String> {
        if known_frames.is_some() { Err(INVALID.into()) } else { Ok(None) }
    }
    if cancelled() { return Err("cancelled".into()); }
    let mut file = std::fs::File::open(path).map_err(|_| INVALID.to_string())?;
    let length = file.metadata().map_err(|_| INVALID.to_string())?.len();
    if length < 12 { return Ok(None); }
    let mut header = [0u8; 12];
    file.read_exact(&mut header).map_err(|_| INVALID.to_string())?;
    // In particular, do not reject RF64/RIFX or compressed WAV formats that a
    // host without FFmpeg could already pass directly to System.Speech.
    if &header[..4] != b"RIFF" || &header[8..] != b"WAVE" { return Ok(None); }
    let declared = u32::from_le_bytes(header[4..8].try_into().unwrap());
    // Streaming RIFF writers also use an unknown-length sentinel. The host
    // may accept it, but it cannot establish a complete frame count here.
    if declared == u32::MAX { return Ok(None); }
    let end = u64::from(declared) + 8;
    if end < 12 { return Ok(None); }
    let truncated = end > length;
    let limit = end.min(length);
    let mut at = 12u64;
    let mut chunks = 0usize;
    let mut frames = None;
    let mut format_seen = false;
    let mut data_bytes = None;
    while at < limit {
        if cancelled() { return Err("cancelled".into()); }
        chunks += 1;
        // A valid host-specific file may exceed our metadata budget. Leave its
        // recognition path intact and omit duration rather than reject it.
        if chunks > 4096 { return Ok(None); }
        if limit - at < 8 { return invalid(frames); }
        file.seek(SeekFrom::Start(at)).map_err(|_| INVALID.to_string())?;
        let mut chunk = [0u8; 8];
        file.read_exact(&mut chunk).map_err(|_| INVALID.to_string())?;
        let size = u64::from(u32::from_le_bytes(chunk[4..].try_into().unwrap()));
        if size == u64::from(u32::MAX) { return Ok(None); }
        let payload_end = at + 8 + size;
        let next = payload_end + size % 2;
        if next > limit { return invalid(frames); }
        match &chunk[..4] {
            b"fmt " => {
                if format_seen { return Ok(None); }
                format_seen = true;
                if size < 16 { return Ok(None); }
                let mut format = [0u8; 40];
                let count = size.min(format.len() as u64) as usize;
                file.read_exact(&mut format[..count]).map_err(|_| INVALID.to_string())?;
                let tag = u16::from_le_bytes(format[..2].try_into().unwrap());
                let codec = if tag == 0xfffe && size >= 40
                    && u16::from_le_bytes(format[16..18].try_into().unwrap()) >= 22
                    && &format[26..40] == b"\0\0\0\0\x10\0\x80\0\0\xaa\0\x38\x9b\x71" {
                    u16::from_le_bytes(format[24..26].try_into().unwrap())
                } else { tag };
                if matches!(codec, 1 | 3) {
                    let channels = u16::from_le_bytes(format[2..4].try_into().unwrap());
                    let rate = u32::from_le_bytes(format[4..8].try_into().unwrap());
                    let byte_rate = u32::from_le_bytes(format[8..12].try_into().unwrap());
                    let align = u16::from_le_bytes(format[12..14].try_into().unwrap());
                    let bits = u16::from_le_bytes(format[14..16].try_into().unwrap());
                    if channels == 0 || rate == 0 || bits == 0 || align == 0 { return Err(INVALID.into()); }
                    if bits % 8 != 0 || (codec == 3 && !matches!(bits, 32 | 64)) { return Ok(None); }
                    if u32::from(channels).checked_mul(u32::from(bits / 8)) != Some(u32::from(align))
                        || rate.checked_mul(u32::from(align)) != Some(byte_rate) {
                        return Err(INVALID.into());
                    }
                    frames = Some((rate, align));
                }
            }
            b"data" => {
                if data_bytes.replace(size).is_some() { return Ok(None); }
            }
            _ => {}
        }
        at = next;
    }
    let Some((rate, align)) = frames else { return Ok(None); };
    // A nonstandard WAV can put audio in a wavl LIST rather than one top-level
    // data chunk. Preserve the recognizer's existing acceptance of such files.
    let Some(bytes) = data_bytes else { return Ok(None); };
    if truncated || bytes % u64::from(align) != 0 { return Err(INVALID.into()); }
    if cancelled() { return Err("cancelled".into()); }
    Ok(Some((bytes / u64::from(align)) as f64 / f64::from(rate)))
}

pub fn transcribe(path: &str, language: Option<&str>) -> Result<Value, String> {
    if !Path::new(path).is_file() { return Err("file_not_found".into()); }
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let wave = dir.path().join("speech.wav");
    // Normalise arbitrary supported audio/video to PCM; paths are argv values,
    // never interpolated into a shell program.
    let ffmpeg = crate::diagrams::which("ffmpeg");
    let input = if let Some(ffmpeg) = ffmpeg {
        run(crate::silent_command(ffmpeg).args(["-nostdin","-v","error","-y","-i",path,"-vn","-ac","1","-ar","16000","-c:a","pcm_s16le"]).arg(&wave), &[], Duration::from_secs(180))
            .map_err(|e| format!("audio_decode_failed: {e}"))?;
        wave.as_path()
    } else if Path::new(path).extension().is_some_and(|e| e.eq_ignore_ascii_case("wav")) {
        Path::new(path)
    } else { return Err("ffmpeg_unavailable".into()); };
    // For non-WAV inputs this is the selected decoded audio stream, including
    // codec trimming/padding, not the duration of a video's longer image track.
    let duration = wave_duration(input)?;
    let mut result = request(json!({"path":input,"language":language.unwrap_or("auto")}), Duration::from_secs(600))?;
    if result.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(result.get("error").and_then(Value::as_str).unwrap_or("speech_recognition_failed").into());
    }
    if cancelled() { return Err("cancelled".into()); }
    if let Some(duration) = duration { result["duration"] = json!(duration); }
    Ok(result)
}

#[cfg(test)]
#[path = "speech_duration_tests.rs"]
mod duration_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_input_never_runs_a_process() {
        assert_eq!(transcribe("/readmd/nonexistent/audio.wav", Some("en")).unwrap_err(), "file_not_found");
    }
    #[test]
    fn subprocess_timeout_is_reaped() {
        #[cfg(windows)] {
            let result = run(crate::silent_command("powershell.exe").args(["-NoProfile","-NonInteractive","-Command","Start-Sleep -Seconds 20"]), &[], Duration::from_millis(80));
            assert_eq!(result.unwrap_err(), "speech_timeout");
        }
    }
    #[test]
    fn cancellation_reaps_a_running_decoder_and_restores_thread_state() {
        #[cfg(windows)] {
            let result=with_cancel(std::sync::Arc::new(||true),||run(crate::silent_command("powershell.exe").args(["-NoProfile","-Command","Start-Sleep -Seconds 20"]),&[],Duration::from_secs(10)));
            assert_eq!(result.unwrap_err(),"cancelled");assert!(!cancelled());
        }
    }
    #[test]
    fn installed_system_engine_transcribes_real_pcm_and_compressed_media() {
        #[cfg(windows)] {
            if capabilities().get("ok").and_then(Value::as_bool)!=Some(true) {return;}
            let english=capabilities()["languages"].as_array().unwrap().iter().any(|v|v["code"].as_str().is_some_and(|s|s.starts_with("en")));
            if !english {return;}
            let dir=tempfile::tempdir().unwrap();let wave=dir.path().join("test.wav");
            let script=r#"Add-Type -AssemblyName System.Speech; $p=([Console]::In.ReadToEnd()|ConvertFrom-Json).path; $s=New-Object System.Speech.Synthesis.SpeechSynthesizer; $v=$s.GetInstalledVoices()|Where-Object {$_.VoiceInfo.Culture.Name -like 'en-*'}|Select-Object -First 1; if(!$v){exit 2}; $s.SelectVoice($v.VoiceInfo.Name); $s.SetOutputToWaveFile($p); $s.SpeakSsml('<speak version="1.0" xmlns="http://www.w3.org/2001/10/synthesis" xml:lang="en-GB">This is a reading document.<break time="2000ms"/>The application converts files to markdown.</speak>'); $s.Dispose()"#;
            run(crate::silent_command("powershell.exe").args(["-NoProfile","-NonInteractive","-Command",script]),&serde_json::to_vec(&json!({"path":wave})).unwrap(),Duration::from_secs(20)).expect("create fixture without playing audio");
            let result=transcribe(wave.to_str().unwrap(),Some("en")).unwrap();
            assert_eq!(result["engine"],"system-speech");assert!(result["segments"].as_array().unwrap().iter().any(|s|s["text"].as_str().is_some_and(|s|!s.is_empty())));
            // Two separated phrases exercise absolute stream positions. The
            // last recognized audio cannot end after the actual WAV input.
            let bytes=std::fs::read(&wave).unwrap();let mut at=12;let mut rate=0;let mut data=0;
            while at+8<=bytes.len() {
                let size=u32::from_le_bytes(bytes[at+4..at+8].try_into().unwrap()) as usize;
                if at+8+size>bytes.len(){break;}
                if &bytes[at..at+4]==b"fmt " && size>=12 {rate=u32::from_le_bytes(bytes[at+16..at+20].try_into().unwrap());}
                if &bytes[at..at+4]==b"data" {data=size;}
                at+=8+size+(size%2);
            }
            assert!(rate>0 && data>0);
            assert!(result["segments"].as_array().unwrap().len()>=2,"separated phrases: {result}");
            assert!(result["duration"].as_f64().unwrap()<=data as f64/rate as f64+0.1,"timestamps exceed input duration: {result}");
            if let Some(ffmpeg)=crate::diagrams::which("ffmpeg") {
                let compressed=dir.path().join("test.mp3");
                run(crate::silent_command(ffmpeg).args(["-nostdin","-v","error","-y","-i"]).arg(&wave).arg(&compressed),&[],Duration::from_secs(20)).unwrap();
                let result=transcribe(compressed.to_str().unwrap(),Some("en")).unwrap();assert!(!result["segments"].as_array().unwrap().is_empty());
            }
            assert!(transcribe(wave.to_str().unwrap(),Some("xx-unavailable")).is_err());
        }
    }
}
