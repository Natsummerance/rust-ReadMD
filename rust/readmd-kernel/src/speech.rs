//! Offline file transcription through the installed Windows speech engine.
//! Rust owns decoding, process limits and result validation. No Python, pip,
//! microphone capture or cloud service is involved.

use serde_json::{json, Value};
use std::{io::Write, path::Path, process::{Command, Stdio}, time::{Duration, Instant}};

thread_local! { static CANCEL: std::cell::RefCell<Option<std::sync::Arc<dyn Fn()->bool+Send+Sync>>> = std::cell::RefCell::new(None); }
pub fn with_cancel<T>(check:std::sync::Arc<dyn Fn()->bool+Send+Sync>,action:impl FnOnce()->T)->T {
    struct Reset(Option<std::sync::Arc<dyn Fn()->bool+Send+Sync>>);
    impl Drop for Reset {fn drop(&mut self){CANCEL.with(|c|*c.borrow_mut()=self.0.take());}}
    let previous=CANCEL.with(|c|c.replace(Some(check)));let _reset=Reset(previous);action()
}
fn cancelled()->bool {CANCEL.with(|c|c.borrow().as_ref().is_some_and(|f|f()))}

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
  $cursor = 0.0
  while ($true) {
    try { $result = $recognizer.Recognize() }
    catch {
      # SAPI disposes the input at EOF after its final nonempty result.
      if ($segments.Count -gt 0 -and $_.Exception.InnerException -is [System.InvalidOperationException]) { break }
      throw
    }
    if (!$result) { break }
    # AudioPosition is absolute within the input stream, including earlier
    # phrases. Adding the previous phrase's end double-counts elapsed time.
    $start = $result.Audio.AudioPosition.TotalSeconds
    $segments.Add(@{start=$start;text=$result.Text;confidence=[double]$result.Confidence})
    $cursor = $start + $result.Audio.Duration.TotalSeconds
  }
  $recognizer.Dispose()
  @{ok=$true;engine='system-speech';language=$engine.Culture.Name;duration=$cursor;segments=@($segments.ToArray())} | ConvertTo-Json -Compress -Depth 5
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
    let result = request(json!({"path":input,"language":language.unwrap_or("auto")}), Duration::from_secs(600))?;
    if result.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(result.get("error").and_then(Value::as_str).unwrap_or("speech_recognition_failed").into());
    }
    Ok(result)
}

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
