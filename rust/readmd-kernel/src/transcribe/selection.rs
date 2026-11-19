//! Audio tracks and time ranges share a source-relative clock with subtitles.
use super::draft::{self, Checkpoint, Chunk, RunnerIdentity, StepRequest};
use crate::{server::Response, speech, whisper_cpp};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
    time::Duration,
};

const SAMPLE_RATE: u64 = 16_000;
const MAX_SELECTION_MS: u64 = 10 * 60 * 1000;
pub(super) const MAX_TASK_SELECTION_MS: u64 = 4 * 60 * 60 * 1000;
const CHUNK_MS: u64 = 2 * 60 * 1000;
const MAX_PREVIEW_MS: u64 = 15 * 1000;
pub(super) const MAX_TIMESTAMP_MS: u64 = 9_000_000_000_000_000;
const LOCAL_FORMATS: &str = "aac,asf,avi,flac,matroska,webm,mov,mp3,ogg,wav";
static RECOGNITION: Mutex<()> = Mutex::new(());

#[derive(Debug, Serialize)]
struct Track {
    index: u32,
    codec: String,
    channels: Option<u32>,
    language: Option<String>,
    title: Option<String>,
    start_ms: u64,
    end_ms: Option<u64>,
    default: bool,
}

#[derive(Debug, Serialize)]
struct Media {
    duration_ms: Option<u64>,
    tracks: Vec<Track>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(super) struct Selection {
    pub(super) track: u32,
    pub(super) start_ms: u64,
    pub(super) end_ms: u64,
}

impl Selection {
    fn read(body: &Value, max_length: u64) -> Result<Self, &'static str> {
        let selection: Self =
            serde_json::from_value(body.clone()).map_err(|_| "media_selection_invalid")?;
        if selection.start_ms >= selection.end_ms || selection.end_ms > MAX_TIMESTAMP_MS {
            return Err("media_selection_invalid");
        }
        if selection.length() > max_length {
            return Err("media_range_too_long");
        }
        Ok(selection)
    }

    pub(super) fn valid(&self, max_length: u64) -> bool {
        self.start_ms < self.end_ms
            && self.end_ms <= MAX_TIMESTAMP_MS
            && self.length() <= max_length
    }

    fn length(&self) -> u64 {
        self.end_ms - self.start_ms
    }

    fn validate_track(&self, media: &Media) -> Result<(), &'static str> {
        let track = media
            .tracks
            .iter()
            .find(|track| track.index == self.track)
            .ok_or("media_audio_track_invalid")?;
        if track
            .end_ms
            .or(media.duration_ms)
            .is_some_and(|end| self.start_ms >= end)
            || self.end_ms <= track.start_ms
        {
            return Err("media_selection_invalid");
        }
        Ok(())
    }
}

struct Decoder {
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
    resources: Option<whisper_cpp::DecoderResources>,
}

impl Decoder {
    fn open(profile: &Path, assets: &Path, pinned: Option<&str>) -> Result<Self, &'static str> {
        if !pinned.is_some_and(|identity| identity.starts_with("host:")) {
            match whisper_cpp::decoder(profile, assets) {
                Ok(Some(resources)) => {
                    return Ok(Self {
                        ffmpeg: resources.ffmpeg.clone(),
                        ffprobe: resources.ffprobe.clone(),
                        resources: Some(resources),
                    });
                }
                Err(error) if error == "cancelled" => return Err("cancelled"),
                Err(error) if error == "media_busy" => return Err("media_busy"),
                _ if pinned.is_some() => return Err("media_resume_conflict"),
                _ => (),
            }
        }
        Self::from_host()
    }

    fn identity(&self) -> Result<String, &'static str> {
        if let Some(resources) = &self.resources {
            return Ok(format!("managed:{}", resources.identity));
        }
        let identify = |path: &Path| revision(path).map_err(|_| "ffmpeg_unavailable");
        Ok(format!(
            "host:{}:{}",
            identify(&self.ffmpeg)?,
            identify(&self.ffprobe)?
        ))
    }

    fn from_host() -> Result<Self, &'static str> {
        // This is an explicitly reported host dependency. Only real .exe
        // files are considered on Windows so these paths are never shell scripts.
        let executable = |name: &str| {
            let name = if cfg!(windows) {
                format!("{name}.exe")
            } else {
                name.into()
            };
            crate::diagrams::which(&name).and_then(|path| std::path::absolute(path).ok())
        };
        Ok(Self {
            ffmpeg: executable("ffmpeg").ok_or("ffmpeg_unavailable")?,
            ffprobe: executable("ffprobe").ok_or("ffmpeg_unavailable")?,
            resources: None,
        })
    }

    fn command(&self, program: &Path) -> Command {
        let mut command = crate::silent_command(program);
        if self.resources.is_some() {
            command.env_clear();
        }
        command
    }

    fn probe(&self, path: &Path) -> Result<Media, &'static str> {
        let output = speech::run(
            self.command(&self.ffprobe)
                .args([
                    "-v", "error",
                    "-select_streams", "a",
                    "-show_entries",
                    "format=duration,start_time:stream=index,codec_name,channels,start_time,duration:stream_tags=language,title:stream_disposition=default",
                    "-of", "json",
                    "-protocol_whitelist", "file",
                    "-format_whitelist", LOCAL_FORMATS,
                    "-i",
                ])
                .arg(path),
            &[],
            Duration::from_secs(15),
        )
        .map_err(|error| process_error(&error, "media_probe_failed"))?;
        let info: Value = serde_json::from_slice(&output).map_err(|_| "media_probe_failed")?;
        probe_info(&info)
    }

    fn decode(&self, path: &Path, selection: &Selection, wave: &Path) -> Result<u64, &'static str> {
        // FFmpeg's seek is relative to the same zero as the playback timeline.
        // Keep the demuxer timestamps through decoding and pad a delayed track
        // at its real offset before resetting the PCM origin to the range start.
        let start = selection.start_ms;
        let start_arg = format!("{}.{:03}", start / 1000, start % 1000);
        let first_sample = start * (SAMPLE_RATE / 1000);
        let samples = selection.length() * (SAMPLE_RATE / 1000);
        let filter = format!(
            "aresample={SAMPLE_RATE}:async=1:first_pts={first_sample},atrim=end_sample={samples},asetpts=PTS-STARTPTS"
        );
        speech::run(
            self.command(&self.ffmpeg)
                .args([
                    "-nostdin",
                    "-v",
                    "error",
                    "-y",
                    "-copyts",
                    "-start_at_zero",
                    "-filter_threads",
                    "1",
                    "-threads",
                    "2",
                    "-protocol_whitelist",
                    "file",
                    "-format_whitelist",
                    LOCAL_FORMATS,
                    "-ss",
                ])
                .arg(start_arg)
                .arg("-i")
                .arg(path)
                .arg("-map")
                .arg(format!("0:{}", selection.track))
                .arg("-af")
                .arg(filter)
                .args(["-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le", "-f", "wav"])
                .arg(wave),
            &[],
            Duration::from_secs(120),
        )
        .map_err(|error| process_error(&error, "media_decode_failed"))?;
        let decoded_ms = speech::wave_duration(wave)
            .map_err(|error| process_error(&error, "media_decode_failed"))?
            .and_then(milliseconds)
            .filter(|length| *length > 0 && *length <= selection.length())
            .ok_or("media_audio_empty")?;
        Ok(decoded_ms)
    }
}

fn process_error(error: &str, fallback: &'static str) -> &'static str {
    match error {
        "cancelled" => "cancelled",
        "speech_timeout" => "media_timeout",
        "media_busy" => "media_busy",
        "speech_language_unavailable" => "speech_language_unavailable",
        _ => fallback,
    }
}

fn seconds(value: &Value) -> Option<f64> {
    value
        .as_str()?
        .parse::<f64>()
        .ok()
        .filter(|time| time.is_finite())
}

fn milliseconds(seconds: f64) -> Option<u64> {
    let ms = (seconds * 1000.0).round();
    (0.0..=MAX_TIMESTAMP_MS as f64)
        .contains(&ms)
        .then_some(ms as u64)
}

fn probe_info(info: &Value) -> Result<Media, &'static str> {
    let format_start = seconds(&info["format"]["start_time"]).unwrap_or(0.0);
    let duration_ms = seconds(&info["format"]["duration"]).and_then(milliseconds);
    let streams = info["streams"].as_array().ok_or("media_probe_failed")?;
    let mut tracks = Vec::new();
    for stream in streams {
        let index = stream["index"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or("media_probe_failed")?;
        let start =
            (seconds(&stream["start_time"]).unwrap_or(format_start) - format_start).max(0.0);
        let tag = |key: &str| stream["tags"][key].as_str().map(str::to_owned);
        tracks.push(Track {
            index,
            codec: stream["codec_name"].as_str().unwrap_or("").into(),
            channels: stream["channels"]
                .as_u64()
                .and_then(|n| u32::try_from(n).ok()),
            language: tag("language").filter(|language| language != "und"),
            title: tag("title"),
            start_ms: milliseconds(start).ok_or("media_probe_failed")?,
            end_ms: seconds(&stream["duration"])
                .and_then(|duration| milliseconds(start + duration)),
            default: stream["disposition"]["default"].as_u64() == Some(1),
        });
    }
    if tracks.is_empty() {
        return Err("media_audio_empty");
    }
    Ok(Media {
        duration_ms,
        tracks,
    })
}

fn revision(path: &Path) -> Result<String, &'static str> {
    let file = fs::File::open(path).map_err(|_| "file_not_found")?;
    let metadata = file.metadata().map_err(|_| "file_not_found")?;
    if !metadata.is_file() {
        return Err("file_not_found");
    }
    let mut hash = Sha256::new();
    hash.update(path.as_os_str().as_encoded_bytes());
    hash.update(metadata.len().to_le_bytes());
    hash.update(format!("{:?}", metadata.modified().ok()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        hash.update(metadata.dev().to_le_bytes());
        hash.update(metadata.ino().to_le_bytes());
    }
    #[cfg(windows)]
    {
        use std::os::windows::{fs::MetadataExt, io::AsRawHandle};
        use windows::Win32::{
            Foundation::HANDLE,
            Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION},
        };
        let mut identity = BY_HANDLE_FILE_INFORMATION::default();
        unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut identity) }
            .map_err(|_| "media_io_failed")?;
        hash.update(identity.dwVolumeSerialNumber.to_le_bytes());
        hash.update(identity.nFileIndexHigh.to_le_bytes());
        hash.update(identity.nFileIndexLow.to_le_bytes());
        hash.update(metadata.creation_time().to_le_bytes());
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn ensure_revision(path: &Path, expected: &str) -> Result<(), &'static str> {
    if revision(path).ok().as_deref() != Some(expected) {
        return Err("media_source_changed");
    }
    Ok(())
}

enum Recognizer {
    System,
    Whisper(whisper_cpp::Engine),
}

impl Recognizer {
    fn open(
        profile: &Path,
        assets: &Path,
        pinned: Option<&str>,
    ) -> Result<Option<Self>, &'static str> {
        if pinned != Some("system-speech") {
            match whisper_cpp::open(profile, assets) {
                Ok(Some(engine)) => return Ok(Some(Self::Whisper(engine))),
                Err(error) if error == "cancelled" => return Err("cancelled"),
                Err(error) if error == "media_busy" => return Err("media_busy"),
                _ if pinned.is_some() => return Err("media_resume_conflict"),
                _ => (),
            }
        }
        Ok((speech::capabilities()["ok"] == true).then_some(Self::System))
    }

    fn name(&self) -> &'static str {
        match self {
            Self::System => "system-speech",
            Self::Whisper(_) => "whisper.cpp",
        }
    }

    fn identity(&self) -> &str {
        match self {
            Self::System => "system-speech",
            Self::Whisper(engine) => engine.identity(),
        }
    }

    fn languages(&self) -> Value {
        match self {
            Self::System => speech::capabilities()["languages"].clone(),
            Self::Whisper(_) => whisper_cpp::languages(),
        }
    }

    fn recognize(&self, wave: &Path, language: Option<&str>) -> Result<Value, String> {
        match self {
            Self::System => speech::recognize_wave(wave, language),
            Self::Whisper(engine) => engine.recognize(wave, language),
        }
    }
}

fn inspect(
    decoder: &Decoder,
    path: &Path,
    expected: &str,
    profile: &Path,
    assets: &Path,
) -> Result<Response, &'static str> {
    let media = decoder.probe(path)?;
    let track = media
        .tracks
        .iter()
        .find(|track| track.default)
        .unwrap_or(&media.tracks[0]);
    let sample = Selection {
        track: track.index,
        start_ms: track.start_ms,
        end_ms: track
            .start_ms
            .checked_add(250)
            .ok_or("media_probe_failed")?,
    };
    let work = tempfile::tempdir().map_err(|_| "media_io_failed")?;
    decoder.decode(path, &sample, &work.path().join("check.wav"))?;
    let recognizer = Recognizer::open(profile, assets, None)?;
    let managed = decoder.resources.is_some();
    ensure_revision(path, expected)?;
    Ok(Response::json(&json!({
        "ok": true,
        "source_revision": expected,
        "media": media,
        "decoder": {
            "engine": "ffmpeg",
            "source": if managed { "managed-package" } else { "host-path" },
            "managed": managed,
            "verified": true,
        },
        "recognizer": {
            "engine": recognizer.as_ref().map(Recognizer::name),
            "source": recognizer.as_ref().map(|engine| match engine {
                Recognizer::System => "windows-system",
                Recognizer::Whisper(_) => "managed-package",
            }),
            "available": recognizer.is_some(),
            "languages": recognizer.as_ref().map_or_else(|| json!([]), Recognizer::languages),
        },
        "max_selection_ms": MAX_SELECTION_MS,
        "max_task_selection_ms": MAX_TASK_SELECTION_MS,
        "max_preview_ms": MAX_PREVIEW_MS,
    })))
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Cue {
    pub(super) start_ms: u64,
    pub(super) end_ms: u64,
    pub(super) text: String,
}

fn cues(result: &Value, source_start: u64, decoded_ms: u64) -> Result<Vec<Cue>, &'static str> {
    let rows = result["segments"]
        .as_array()
        .ok_or("media_recognition_failed")?;
    let mut cues: Vec<Cue> = rows
        .iter()
        .filter_map(|row| {
            let start_ms = milliseconds(row["start"].as_f64()?)?;
            let end_ms = milliseconds(row["end"].as_f64()?)?.min(decoded_ms);
            let text = row["text"].as_str()?.trim();
            if start_ms >= end_ms || text.is_empty() {
                return None;
            }
            Some(Cue {
                start_ms: source_start + start_ms,
                end_ms: source_start + end_ms,
                text: text.into(),
            })
        })
        .collect();
    cues.sort_by_key(|cue| (cue.start_ms, cue.end_ms));
    Ok(cues)
}

fn vtt_time(ms: u64) -> String {
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

fn vtt(cues: &[Cue]) -> String {
    let mut output = String::from("WEBVTT\n\n");
    for cue in cues {
        output.push_str(&format!(
            "{} --> {}\n",
            vtt_time(cue.start_ms),
            vtt_time(cue.end_ms)
        ));
        let lines = cue
            .text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty());
        for line in lines {
            output.push_str(
                &line
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;"),
            );
            output.push('\n');
        }
        output.push('\n');
    }
    output
}

fn recognize_selection(
    path: &Path,
    selection: &Selection,
    wave: &Path,
    decoded_ms: u64,
    language: Option<&str>,
    recognize: impl FnOnce(&Path, Option<&str>) -> Result<Value, String>,
) -> Result<Response, &'static str> {
    let result = recognize(wave, language)
        .map_err(|error| process_error(&error, "media_recognition_failed"))?;
    let cues = cues(&result, selection.start_ms, decoded_ms)?;
    if cues.is_empty() {
        return Err("speech_no_result");
    }
    let segments: Vec<super::Segment> = cues
        .iter()
        .map(|cue| super::Segment {
            start: cue.start_ms as f64 / 1000.0,
            text: cue.text.clone(),
        })
        .collect();
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("transcript");
    let language = result["language"].as_str();
    let engine = result["engine"].as_str().unwrap_or("system-speech");
    let format = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    let content = super::format_segments(
        &segments,
        Some(name),
        language,
        Some(decoded_ms as f64 / 1000.0),
        Some(format),
        Some(engine),
    );
    Ok(Response::json(&json!({
        "ok": true,
        "track": selection.track,
        "start_ms": selection.start_ms,
        "end_ms": selection.start_ms + decoded_ms,
        "name": name,
        "language": language,
        "engine": engine,
        "content": content,
        "cues": cues,
        "vtt": vtt(&cues),
    })))
}

fn recognize_chunk(
    decoder: &Decoder,
    source: &Path,
    source_revision: &str,
    step: &Checkpoint,
    recognize: impl FnOnce(&Path, Option<&str>) -> Result<Value, String>,
) -> Result<Chunk, &'static str> {
    let selection = Selection {
        track: step.selection.track,
        start_ms: step.next_ms,
        end_ms: (step.next_ms + CHUNK_MS).min(step.selection.end_ms),
    };
    let work = tempfile::tempdir().map_err(|_| "media_io_failed")?;
    let wave = work.path().join("chunk.wav");
    let decoded_ms = match decoder.decode(source, &selection, &wave) {
        Ok(length) => length,
        // A movie container may continue after its selected audio track ended.
        // An empty first chunk still indicates an unusable selection.
        Err("media_audio_empty") if step.next_ms > step.selection.start_ms => {
            ensure_revision(source, source_revision)?;
            return Ok(Chunk {
                next_ms: step.next_ms,
                done: true,
                language: step.resolved_language.clone(),
                cues: Vec::new(),
            });
        }
        Err(code) => return Err(code),
    };
    ensure_revision(source, source_revision)?;
    let language = step.resolved_language.as_deref().unwrap_or(&step.language);
    let result = recognize(&wave, Some(language))
        .map_err(|error| process_error(&error, "media_recognition_failed"))?;
    if result["engine"].as_str() != Some(step.runner.engine.as_str()) {
        return Err("media_resume_conflict");
    }
    let resolved = result["language"]
        .as_str()
        .ok_or("media_recognition_failed")?;
    if step
        .resolved_language
        .as_deref()
        .is_some_and(|language| language != resolved)
    {
        return Err("media_resume_conflict");
    }
    // Decoder rounding and codec padding can leave a few milliseconds at the
    // end of an otherwise complete window. Keep contiguous, disjoint windows.
    // Every cue belongs to exactly one window, so retry cannot duplicate a cue.
    let short = selection.length().saturating_sub(decoded_ms) > 20;
    let next_ms = if short {
        selection.start_ms + decoded_ms
    } else {
        selection.end_ms
    };
    let cues = cues(&result, selection.start_ms, decoded_ms)?;
    ensure_revision(source, source_revision)?;
    Ok(Chunk {
        next_ms,
        done: short || next_ms == step.selection.end_ms,
        language: Some(resolved.to_owned()),
        cues,
    })
}

fn transcribe_step(
    profile: &Path,
    assets: &Path,
    body: &Value,
    path: &str,
    source: &Path,
    source_revision: &str,
    language: &str,
) -> Result<Response, &'static str> {
    if body["source_revision"].as_str() != Some(source_revision) {
        return Err("media_source_changed");
    }
    let request: StepRequest =
        serde_json::from_value(body.clone()).map_err(|_| "media_selection_invalid")?;
    let selection = Selection::read(body, MAX_TASK_SELECTION_MS)?;
    let _exclusive = RECOGNITION.try_lock().map_err(|_| "media_busy")?;
    let pinned = draft::saved_runner(profile, &request)?;
    let decoder = Decoder::open(
        profile,
        assets,
        pinned
            .as_ref()
            .map(|runner| runner.decoder_revision.as_str()),
    )?;
    let recognizer = Recognizer::open(
        profile,
        assets,
        pinned.as_ref().map(|runner| runner.engine.as_str()),
    )?
    .ok_or("speech_engine_unavailable")?;
    let runner = RunnerIdentity {
        engine: recognizer.name().into(),
        recognizer_revision: recognizer.identity().into(),
        decoder_revision: decoder.identity()?,
    };
    selection.validate_track(&decoder.probe(source)?)?;
    let (record, run) = draft::start_step(
        profile,
        path,
        source_revision,
        &selection,
        language,
        &runner,
        &request,
    )?;
    if !run {
        return Ok(record.response());
    }
    let step = record.checkpoint.ok_or("media_resume_conflict")?;
    let chunk = recognize_chunk(
        &decoder,
        source,
        source_revision,
        &step,
        |wave, language| recognizer.recognize(wave, language),
    )?;
    let next = draft::accept_chunk(profile, &step, chunk)?;
    Ok(next.response())
}

fn run(
    profile: &Path,
    assets: &Path,
    body: &Value,
    input_path: &str,
    language: Option<&str>,
) -> Result<Response, &'static str> {
    let operation = body["operation"]
        .as_str()
        .ok_or("media_operation_invalid")?;
    let path = std::path::absolute(input_path).map_err(|_| "file_not_found")?;
    let expected = revision(&path)?;
    if let Some(sent) = body.get("source_revision") {
        if sent.as_str() != Some(&expected) {
            return Err("media_source_changed");
        }
    }
    let max_length = match operation {
        "inspect" => {
            return inspect(
                &Decoder::open(profile, assets, None)?,
                &path,
                &expected,
                profile,
                assets,
            )
        }
        "transcribe_step" => {
            return transcribe_step(
                profile,
                assets,
                body,
                input_path,
                &path,
                &expected,
                language.unwrap_or("auto"),
            )
        }
        "preview" => MAX_PREVIEW_MS,
        "transcribe" => MAX_SELECTION_MS,
        _ => return Err("media_operation_invalid"),
    };
    let selection = Selection::read(body, max_length)?;
    let decoder = Decoder::open(profile, assets, None)?;
    let recognition = if operation == "transcribe" {
        let exclusive = RECOGNITION.try_lock().map_err(|_| "media_busy")?;
        let engine = Recognizer::open(profile, assets, None)?.ok_or("speech_engine_unavailable")?;
        Some((exclusive, engine))
    } else {
        None
    };
    let media = decoder.probe(&path)?;
    selection.validate_track(&media)?;
    let work = tempfile::tempdir().map_err(|_| "media_io_failed")?;
    let wave = work.path().join("selection.wav");
    let decoded_ms = decoder.decode(&path, &selection, &wave)?;
    ensure_revision(&path, &expected)?;
    if let Some((_exclusive, engine)) = recognition {
        let response = recognize_selection(
            &path,
            &selection,
            &wave,
            decoded_ms,
            language,
            |wave, language| engine.recognize(wave, language),
        )?;
        ensure_revision(&path, &expected)?;
        Ok(response)
    } else {
        let bytes = fs::read(&wave).map_err(|_| "media_io_failed")?;
        Ok(Response::raw_bytes(200, "audio/wav", bytes).header("Cache-Control", "no-store"))
    }
}

pub(crate) fn endpoint(
    profile: &Path,
    assets: &Path,
    body: &Value,
    path: &str,
    language: Option<&str>,
) -> Response {
    match run(profile, assets, body, path, language) {
        Ok(response) => response,
        Err(code) => {
            let status = match code {
                "cancelled" => 499,
                "media_busy"
                | "media_source_changed"
                | "media_resume_conflict"
                | "media_resume_required"
                | "media_draft_conflict" => 409,
                "media_draft_unavailable" => 507,
                "file_not_found" => 404,
                "speech_engine_unavailable" | "ffmpeg_unavailable" => 503,
                "media_operation_invalid"
                | "media_selection_invalid"
                | "media_audio_track_invalid"
                | "media_range_too_long" => 400,
                _ => 422,
            };
            Response::json_status(status, &json!({"ok": false, "error_code": code}))
        }
    }
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
