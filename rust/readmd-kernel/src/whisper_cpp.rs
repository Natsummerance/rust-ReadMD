//! Offline media runtimes authorized by the application's capability locks.
use crate::{capability_packs, speech, task_workspace, worker_sandbox, ApiError, ApiResult};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

pub(crate) const WHISPER_ID: &str = "whisper";
pub(crate) const WHISPER_VERSION: &str = "1.9.5";
pub(crate) const DECODER_ID: &str = "media-ffmpeg";
pub(crate) const DECODER_VERSION: &str = "9.0.2";
pub(crate) const ENGINE: &str = "whisper.cpp";
// The upstream LFS object for ggml-base-q5_1.bin at
// https://huggingface.co/ggerganov/whisper.cpp/tree/5359861c739e955e79d9a303bcbc70fb988958b1
const MODEL: &str = "models/ggml-base-q5_1.bin";
const MODEL_BYTES: u64 = 59_707_625;
const MODEL_SHA256: &str = "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898";
const INPUT_LIMIT: u64 = 128 * 1024 * 1024;
const OUTPUT_LIMIT: u64 = 8 * 1024 * 1024;
const LOG_LIMIT: u64 = 256 * 1024;
const PROBE_DURATION: f64 = 0.25;
static ACTIVE: AtomicUsize = AtomicUsize::new(0);

pub(crate) fn is_media_pack(id: &str) -> bool {
    matches!(id, WHISPER_ID | DECODER_ID)
}

// The multilingual base vocabulary supports these 99 Whisper language IDs.
// Cantonese (yue, ID 99) requires the extended large-v3 vocabulary.
const LANGUAGE_CODES: &str = "en zh de es ru ko fr ja pt tr pl ca nl ar sv it id hi fi vi he uk el ms cs ro da hu ta no th ur hr bg lt la mi ml cy sk te fa lv bn sr az sl kn et mk br eu is hy ne mn bs kk sq sw gl mr pa si km sn yo so af oc ka be tg sd gu am yi lo uz fo ht ps tk nn mt sa lb my bo tl mg as tt haw ln ha ba jw su";

fn language_code(language: Option<&str>) -> Result<String, &'static str> {
    let language = language.unwrap_or_default().trim();
    if language.is_empty() || language.eq_ignore_ascii_case("auto") {
        return Ok("auto".into());
    }
    if language.len() > 35 || !language.is_ascii() {
        return Err("speech_language_unavailable");
    }
    let mut parts = language.split(['-', '_']);
    let code = parts.next().unwrap_or_default().to_ascii_lowercase();
    if !LANGUAGE_CODES.split_whitespace().any(|valid| valid == code)
        || parts.any(|part| {
            part.is_empty() || part.len() > 8 || !part.bytes().all(|b| b.is_ascii_alphanumeric())
        })
    {
        return Err("speech_language_unavailable");
    }
    Ok(code)
}

pub(crate) fn languages() -> Value {
    let entries = LANGUAGE_CODES.split_whitespace().map(|code| {
        let name = match code {
            "en" => "English",
            "zh" => "中文",
            "ja" => "日本語",
            "ko" => "한국어",
            "fr" => "Français",
            "de" => "Deutsch",
            "es" => "Español",
            "ru" => "Русский",
            "pt" => "Português",
            "ar" => "العربية",
            "hi" => "हिन्दी",
            _ => code,
        };
        json!({"code": code, "name": name})
    });
    Value::Array(entries.collect())
}

fn executable(id: &str, platform: &str) -> &'static [&'static str] {
    match (id, platform.starts_with("windows-")) {
        (WHISPER_ID, true) => &["whisper-cli.exe"],
        (WHISPER_ID, false) => &["whisper-cli"],
        (DECODER_ID, true) => &["ffmpeg.exe", "ffprobe.exe"],
        (DECODER_ID, false) => &["ffmpeg", "ffprobe"],
        _ => &[],
    }
}

/// Called only for manifests already resolved from the application asset tree.
/// Require a portable build with a fixed executable inventory and license files.
pub(crate) fn valid_manifest(id: &str, lock: &Value) -> bool {
    let Some(platform) = lock["platform"].as_str() else {
        return false;
    };
    let Some(files) = lock["files"].as_array() else {
        return false;
    };
    if lock["schema"] != 2
        || files.len() > 32
        || !files
            .iter()
            .any(|f| f["path"] == "LICENSE" && f["size"].as_u64().is_some_and(|size| size > 0))
    {
        return false;
    }
    let binaries = executable(id, platform);
    if binaries.is_empty()
        || binaries.iter().any(|name| {
            !files.iter().any(|file| {
                file["path"] == format!("bin/{name}")
                    && file["executable"] == true
                    && file["size"].as_u64().is_some_and(|size| size > 0)
            })
        })
    {
        return false;
    }
    if id == WHISPER_ID
        && !files.iter().any(|file| {
            file["path"] == MODEL && file["size"] == MODEL_BYTES && file["sha256"] == MODEL_SHA256
        })
    {
        return false;
    }
    files.iter().all(|file| {
        let Some(path) = file["path"].as_str() else {
            return false;
        };
        if let Some(name) = path.strip_prefix("bin/") {
            return binaries.contains(&name) && file["executable"] == true;
        }
        file["executable"] != true
            && (matches!(path, "LICENSE" | "NOTICE" | "SOURCE")
                || path.starts_with("licenses/")
                || (id == WHISPER_ID && path == MODEL))
    })
}

// The worker loads system dependencies and the reviewed portable executable.
// Extra loader files in a modified old installation must not enter that path.
fn binary_inventory(directory: &Path, id: &str) -> Result<(), &'static str> {
    let expected = executable(id, capability_packs::platform());
    let entries = fs::read_dir(directory.join("bin")).map_err(|_| "capability_pack_invalid")?;
    let mut count = 0;
    for entry in entries {
        let entry = entry.map_err(|_| "capability_pack_invalid")?;
        if !entry.file_type().is_ok_and(|kind| kind.is_file())
            || !entry
                .file_name()
                .to_str()
                .is_some_and(|name| expected.contains(&name))
        {
            return Err("capability_pack_invalid");
        }
        count += 1;
    }
    if count != expected.len() {
        return Err("capability_pack_invalid");
    }
    Ok(())
}

fn cancellation() -> ApiResult<()> {
    if speech::cancelled() {
        Err(ApiError::new(422, "cancelled"))
    } else {
        Ok(())
    }
}

fn worker_error(error: ApiError) -> String {
    if error.status == 409 {
        "media_busy".into()
    } else {
        error.code
    }
}

fn probe_receipt(parent: &Path, id: &str, version: &str, fingerprint: &str) -> (PathBuf, Vec<u8>) {
    let path = parent.join(format!("media-probe-v1-{fingerprint}.json"));
    let expected = json!({
        "schema":1,"id":id,"version":version,"platform":capability_packs::platform(),
        "manifest_sha256":fingerprint,"result":"passed"
    });
    (
        path,
        serde_json::to_vec(&expected).expect("fixed media receipt"),
    )
}

fn receipt_matches(path: &Path, expected: &[u8]) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|meta| meta.file_type().is_file() && meta.len() == expected.len() as u64)
        && File::open(path).is_ok_and(|file| {
            bounded_read(&file, expected.len() as u64).is_ok_and(|bytes| bytes == expected)
        })
}

fn lock_fingerprint(lock: &Value) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(lock).expect("JSON manifest"))
    )
}

pub(crate) fn installed_probe(directory: &Path, lock: &Value) -> Result<(), String> {
    let id = lock["id"].as_str().ok_or("capability_pack_invalid")?;
    let version = lock["version"].as_str().ok_or("capability_pack_invalid")?;
    require_receipt(directory, id, version, &lock_fingerprint(lock))
}

fn require_receipt(
    directory: &Path,
    id: &str,
    version: &str,
    fingerprint: &str,
) -> Result<(), String> {
    let parent = directory.parent().ok_or("capability_pack_invalid")?;
    let (path, expected) = probe_receipt(parent, id, version, fingerprint);
    if !receipt_matches(&path, &expected) {
        return Err("capability_pack_invalid".into());
    }
    binary_inventory(directory, id).map_err(str::to_owned)
}

pub(crate) fn record_install_probe(parent: &Path, lock: &Value) -> ApiResult<()> {
    let failure = || ApiError::new(422, "capability_install_failed");
    let (path, expected) = probe_receipt(
        parent,
        lock["id"].as_str().ok_or_else(failure)?,
        lock["version"].as_str().ok_or_else(failure)?,
        &lock_fingerprint(lock),
    );
    if receipt_matches(&path, &expected) {
        return Ok(());
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|_| failure())?;
    temporary
        .write_all(&expected)
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|_| failure())?;
    temporary.persist_noclobber(path).map_err(|_| failure())?;
    Ok(())
}

pub(crate) fn clear_install_probe(parent: &Path, lock: &Value) -> bool {
    let (Some(id), Some(version)) = (lock["id"].as_str(), lock["version"].as_str()) else {
        return false;
    };
    let (path, expected) = probe_receipt(parent, id, version, &lock_fingerprint(lock));
    !receipt_matches(&path, &expected) || fs::remove_file(path).is_ok()
}

fn read_resources(
    data: &Path,
    assets: &Path,
    id: &str,
    version: &str,
) -> Result<Option<(capability_packs::ResourceLease, String)>, String> {
    cancellation().map_err(worker_error)?;
    let Some(lock_name) = capability_packs::resource_lock_filename(id) else {
        return Ok(None);
    };
    let exists = |path: &Path| match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        _ => Err("capability_pack_invalid"),
    };
    if !exists(&assets.join("capabilities").join(lock_name))? {
        return Ok(None);
    }
    // Both documents are maintainer inputs. The archive contains neither a
    // trust upgrade nor a way to select its own executable.
    let entry = crate::capability_delivery::checked_entry(assets, id).map_err(worker_error)?;
    let identity = entry["manifestSha256"]
        .as_str()
        .ok_or("capability_pack_invalid")?
        .to_owned();
    let lease = match capability_packs::lease_checked(data, assets, id, &cancellation) {
        Ok(lease) => lease,
        Err(error)
            if error.code == "capability_pack_invalid"
                && !exists(&crate::capability_store::root(data).join(id).join(version))? =>
        {
            return Ok(None)
        }
        Err(error) => return Err(worker_error(error)),
    };
    require_receipt(&lease.directory, id, version, &identity)?;
    Ok(Some((lease, identity)))
}

pub(crate) fn busy() -> bool {
    ACTIVE.load(Ordering::Acquire) > 0
}

pub(crate) struct Engine {
    data: PathBuf,
    identity: String,
    lease: capability_packs::ResourceLease,
}

pub(crate) fn open(data: &Path, assets: &Path) -> Result<Option<Engine>, String> {
    let Some((lease, identity)) = read_resources(data, assets, WHISPER_ID, WHISPER_VERSION)? else {
        return Ok(None);
    };
    Ok(Some(Engine {
        data: data.to_path_buf(),
        identity,
        lease,
    }))
}

pub(crate) struct DecoderResources {
    pub(crate) ffmpeg: PathBuf,
    pub(crate) ffprobe: PathBuf,
    pub(crate) identity: String,
    _lease: capability_packs::ResourceLease,
}

pub(crate) fn decoder(data: &Path, assets: &Path) -> Result<Option<DecoderResources>, String> {
    let Some((lease, identity)) = read_resources(data, assets, DECODER_ID, DECODER_VERSION)? else {
        return Ok(None);
    };
    let names = executable(DECODER_ID, capability_packs::platform());
    let bin = lease.directory.join("bin");
    Ok(Some(DecoderResources {
        ffmpeg: bin.join(names[0]),
        ffprobe: bin.join(names[1]),
        identity,
        _lease: lease,
    }))
}

fn command(program: &Path) -> Command {
    let mut command = crate::silent_command(program);
    // Windows managed workers build their own bootstrap environment. Unix
    // must also discard inherited loader overrides and user engine settings.
    command.env_clear();
    command
}

struct Receipt {
    status: u32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn task_error(error: std::io::Error) -> &'static str {
    if error.kind() == std::io::ErrorKind::WouldBlock {
        "media_busy"
    } else {
        "media_io_failed"
    }
}

fn bounded_read(file: &File, limit: u64) -> Result<Vec<u8>, &'static str> {
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(task_error)?;
    if bytes.len() as u64 > limit {
        return Err("speech_output_too_large");
    }
    Ok(bytes)
}

fn run(
    command: &mut Command,
    work: &Path,
    input: Option<&File>,
    destination: Option<&File>,
    time: Duration,
) -> Result<Receipt, &'static str> {
    let output = tempfile::NamedTempFile::new_in(work).map_err(task_error)?;
    let errors = tempfile::NamedTempFile::new_in(work).map_err(task_error)?;
    task_workspace::track_created(output.path(), output.as_file()).map_err(task_error)?;
    task_workspace::track_created(errors.path(), errors.as_file()).map_err(task_error)?;
    if speech::cancelled() {
        return Err("cancelled");
    }
    let mut worker = if let Some(input) = input {
        worker_sandbox::spawn_managed_with_input(
            command,
            work,
            input,
            output.as_file(),
            errors.as_file(),
        )
    } else {
        worker_sandbox::spawn_managed(command, work, output.as_file(), errors.as_file())
    }
    .map_err(|_| "speech_engine_unavailable")?;
    let deadline = Instant::now() + time;
    let within_limit = |file: &File, cap| -> Result<(), &'static str> {
        if file.metadata().map_err(task_error)?.len() > cap {
            return Err("speech_output_too_large");
        }
        Ok(())
    };
    let status = loop {
        if speech::cancelled() {
            return Err("cancelled");
        }
        if Instant::now() >= deadline {
            return Err("speech_timeout");
        }
        within_limit(output.as_file(), LOG_LIMIT)?;
        within_limit(errors.as_file(), LOG_LIMIT)?;
        if let Some(file) = destination {
            within_limit(file, OUTPUT_LIMIT)?;
        }
        if let Some(status) = worker.try_wait().map_err(|_| "speech_process_failed")? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    worker.stop().map_err(|_| "speech_process_failed")?;
    // Inherited stdout/stderr share a file position with the child on Unix.
    Ok(Receipt {
        status,
        stdout: bounded_read(&output.reopen().map_err(task_error)?, LOG_LIMIT)?,
        stderr: bounded_read(&errors.reopen().map_err(task_error)?, LOG_LIMIT)?,
    })
}

fn version_matches(id: &str, name: &str, bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    let first = text.lines().next().unwrap_or_default();
    if id == WHISPER_ID {
        return first.strip_prefix("whisper.cpp version: ") == Some(WHISPER_VERSION);
    }
    let program = name.trim_end_matches(".exe");
    first
        .strip_prefix(&format!("{program} version "))
        .and_then(|tail| tail.split_whitespace().next())
        .is_some_and(|version| version.strip_prefix('n').unwrap_or(version) == DECODER_VERSION)
}

pub(crate) fn probe_directory(id: &str, directory: &Path, data: &Path) -> Result<(), String> {
    binary_inventory(directory, id)?;
    let task = task_workspace::Workspace::create(data, "media-engine-probe").map_err(task_error)?;
    let result = task.scope(|| -> Result<(), String> {
        let names = executable(id, capability_packs::platform());
        let bin = std::path::absolute(directory.join("bin")).map_err(task_error)?;
        for name in names {
            let program = bin.join(name);
            let argument = if id == WHISPER_ID {
                "--version"
            } else {
                "-version"
            };
            let receipt = run(
                command(&program).arg(argument),
                task.path(),
                None,
                None,
                Duration::from_secs(8),
            )?;
            if receipt.status != 0 || !version_matches(id, name, &receipt.stdout) {
                return Err("capability_pack_invalid".into());
            }
        }
        if id == WHISPER_ID {
            let wave = task.path().join("probe.wav");
            task_workspace::write(&wave, probe_wave()).map_err(task_error)?;
            let wave = File::open(&wave).map_err(task_error)?;
            recognize_in(
                directory,
                task.path(),
                &wave,
                PROBE_DURATION,
                "en",
                "1",
                Duration::from_secs(30),
            )?;
            Ok(())
        } else {
            probe_decoder(&bin.join(names[0]), &bin.join(names[1]), task.path())
        }
    });
    task.finish().map_err(task_error)?;
    result
}

fn probe_wave() -> Vec<u8> {
    const SAMPLES: u32 = 4000;
    const BYTES: u32 = SAMPLES * 2;
    let mut bytes = Vec::with_capacity(44 + BYTES as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + BYTES).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x01\0");
    bytes.extend_from_slice(&16_000_u32.to_le_bytes());
    bytes.extend_from_slice(&32_000_u32.to_le_bytes());
    bytes.extend_from_slice(b"\x02\0\x10\0data");
    bytes.extend_from_slice(&BYTES.to_le_bytes());
    bytes.resize(44 + BYTES as usize, 0);
    bytes
}

fn output_file(path: &Path) -> Result<File, &'static str> {
    let file = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(path)
        .map_err(task_error)?;
    task_workspace::track_created(path, &file).map_err(task_error)?;
    Ok(file)
}

fn probe_decoder(ffmpeg: &Path, ffprobe: &Path, work: &Path) -> Result<(), String> {
    const TIME: Duration = Duration::from_secs(8);
    const INVALID: &str = "capability_pack_invalid";
    let wave = output_file(&work.join("probe.wav"))?;
    let generated = run(
        command(ffmpeg).args([
            "-nostdin",
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=16000:duration=0.25",
            "-threads",
            "1",
            "-c:a",
            "pcm_s16le",
            "-f",
            "wav",
            "probe.wav",
        ]),
        work,
        None,
        Some(&wave),
        TIME,
    )?;
    if generated.status != 0 {
        return Err(INVALID.into());
    }
    let probed = run(
        command(ffprobe).args([
            "-v",
            "error",
            "-protocol_whitelist",
            "file",
            "-show_entries",
            "stream=codec_name,sample_rate,channels",
            "-of",
            "json",
            "-i",
            "probe.wav",
        ]),
        work,
        None,
        None,
        TIME,
    )?;
    let metadata: Value = serde_json::from_slice(&probed.stdout).map_err(|_| INVALID)?;
    if probed.status != 0
        || metadata["streams"]
            != json!([
                {"codec_name":"pcm_s16le","sample_rate":"16000","channels":1}
            ])
    {
        return Err(INVALID.into());
    }
    let pcm = output_file(&work.join("decoded.pcm"))?;
    let decoded = run(
        command(ffmpeg).args([
            "-nostdin",
            "-v",
            "error",
            "-y",
            "-protocol_whitelist",
            "file",
            "-format_whitelist",
            "wav",
            "-i",
            "probe.wav",
            "-map",
            "0:a:0",
            "-threads",
            "1",
            "-filter_threads",
            "1",
            "-af",
            "aresample=16000:async=1:first_pts=0,atrim=end_sample=4000,asetpts=PTS-STARTPTS",
            "-ac",
            "1",
            "-ar",
            "16000",
            "-c:a",
            "pcm_s16le",
            "-f",
            "s16le",
            "decoded.pcm",
        ]),
        work,
        None,
        Some(&pcm),
        TIME,
    )?;
    let decoded_bytes = bounded_read(&pcm, 8000)?;
    if decoded.status != 0 || decoded_bytes.len() != 8000 || !decoded_bytes.iter().any(|b| *b != 0)
    {
        return Err(INVALID.into());
    }
    Ok(())
}

impl Engine {
    pub(crate) fn identity(&self) -> &str {
        &self.identity
    }

    pub(crate) fn recognize(&self, wave: &Path, requested: Option<&str>) -> Result<Value, String> {
        let language = language_code(requested)?;
        cancellation().map_err(worker_error)?;
        let _worker = crate::capability_process::reserve_worker(&self.data, WHISPER_ID, &ACTIVE)
            .map_err(worker_error)?;
        let input = File::open(wave).map_err(|_| "audio_decode_failed")?;
        if !input
            .metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.len() <= INPUT_LIMIT)
        {
            return Err("audio_decode_failed".into());
        }
        let duration = speech::wave_duration(wave)?
            .filter(|duration| *duration > 0.0)
            .ok_or("audio_decode_failed")?;
        let task = task_workspace::Workspace::create(&self.data, "whisper-recognize")
            .map_err(task_error)?;
        let result = task.scope(|| {
            recognize_in(
                &self.lease.directory,
                task.path(),
                &input,
                duration,
                &language,
                "2",
                Duration::from_secs(600),
            )
        });
        task.finish().map_err(task_error)?;
        result
    }
}

fn recognize_in(
    directory: &Path,
    work: &Path,
    input: &File,
    duration: f64,
    language: &str,
    threads: &str,
    timeout: Duration,
) -> Result<Value, String> {
    let name = executable(WHISPER_ID, capability_packs::platform())[0];
    let program = std::path::absolute(directory.join("bin").join(name)).map_err(task_error)?;
    let model = directory.join(MODEL);
    #[cfg(windows)]
    let model = stage_windows_model(&model, work)?;
    #[cfg(not(windows))]
    let model = std::path::absolute(model).map_err(task_error)?;
    let output = output_file(&work.join("transcript.json"))?;
    let receipt = run(
        command(&program)
            .current_dir(work)
            .args([
                "--no-gpu",
                "--no-prints",
                "--threads",
                threads,
                "--language",
                language,
                "--model",
            ])
            .arg(model)
            .args([
                "--file",
                "-",
                "--output-json",
                "--output-file",
                "transcript",
            ]),
        work,
        Some(input),
        Some(&output),
        timeout,
    )?;
    if receipt.status != 0 {
        return Err(recognition_error(receipt.status, &receipt.stderr).into());
    }
    cancellation().map_err(worker_error)?;
    parse_transcript(&bounded_read(&output, OUTPUT_LIMIT)?, language, duration)
        .map_err(str::to_owned)
}

#[cfg(windows)]
fn stage_windows_model(source: &Path, work: &Path) -> Result<PathBuf, &'static str> {
    // The CLI opens narrow argv paths. Keep all arguments ASCII on Windows
    // even when the ReadMD profile lives under a different code page.
    let name = "model.bin";
    let destination = work.join(name);
    let mut source = File::open(source).map_err(|_| "capability_pack_invalid")?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .map_err(task_error)?;
    task_workspace::track_created(&destination, &output).map_err(task_error)?;
    let mut hash = Sha256::new();
    let mut size = 0;
    let mut buffer = [0; 65_536];
    loop {
        if speech::cancelled() {
            return Err("cancelled");
        }
        let count = source.read(&mut buffer).map_err(task_error)?;
        if count == 0 {
            break;
        }
        size += count as u64;
        if size > MODEL_BYTES {
            return Err("capability_pack_invalid");
        }
        hash.update(&buffer[..count]);
        output.write_all(&buffer[..count]).map_err(task_error)?;
    }
    if size != MODEL_BYTES || format!("{:x}", hash.finalize()) != MODEL_SHA256 {
        return Err("capability_pack_invalid");
    }
    Ok(PathBuf::from(name))
}

fn recognition_error(status: u32, log: &[u8]) -> &'static str {
    let errors = String::from_utf8_lossy(log);
    if errors.contains("error: unknown language") {
        "speech_language_unavailable"
    } else if errors.contains("error: failed to read audio file") {
        "audio_decode_failed"
    } else if status == 3
        && (errors.contains("error: model file not found")
            || errors.contains("error: failed to initialize whisper context"))
    {
        "speech_engine_unavailable"
    } else {
        "speech_recognition_failed"
    }
}

#[derive(Deserialize)]
struct RawTranscript {
    result: RawLanguage,
    transcription: Vec<RawSegment>,
}
#[derive(Deserialize)]
struct RawLanguage {
    language: String,
}
#[derive(Deserialize)]
struct RawSegment {
    offsets: Offsets,
    text: String,
}
#[derive(Deserialize)]
struct Offsets {
    from: u64,
    to: u64,
}

fn parse_transcript(bytes: &[u8], requested: &str, duration: f64) -> Result<Value, &'static str> {
    const INVALID: &str = "speech_invalid_output";
    let output: RawTranscript = serde_json::from_slice(bytes).map_err(|_| INVALID)?;
    let language = output.result.language;
    if !LANGUAGE_CODES
        .split_whitespace()
        .any(|valid| valid == language)
        || (requested != "auto" && requested != language)
        || output.transcription.len() > 20_000
    {
        return Err(INVALID);
    }
    let mut segments = Vec::with_capacity(output.transcription.len());
    let max_ms = (duration * 1000.0).round() as u64;
    let mut previous = 0;
    for item in output.transcription {
        let Offsets { from, to } = item.offsets;
        let text = item.text.trim();
        // The pinned multilingual base model emits this exact non-speech
        // marker for short silence, with a 10-second padded-context endpoint
        // even when the real input is 250 ms. It is not a subtitle. Preserve
        // the empty-speech result without relaxing real text timestamps.
        let blank_audio = text == "[BLANK_AUDIO]";
        // Whisper uses 10 ms timestamp units. Clamp only the few ticks
        // spilling past the final sample and reject unrelated time origins.
        if to < from
            || from < previous
            || from > max_ms.saturating_add(30)
            || (!blank_audio && to > max_ms.saturating_add(30))
            || item.text.len() > 65_536
        {
            return Err(INVALID);
        }
        previous = from;
        if blank_audio {
            continue;
        }
        let end = to.min(max_ms);
        if from < end && !text.is_empty() {
            segments.push(
                json!({"start": from as f64 / 1000.0, "end": end as f64 / 1000.0, "text": text}),
            );
        }
    }
    Ok(json!({"ok":true,"engine":ENGINE,"language":language,"segments":segments}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whisper_json_uses_millisecond_offsets_and_reports_the_actual_language() {
        let fixture = br#"{
          "result":{"language":"zh"},
          "transcription":[
            {"timestamps":{"from":"00:00:00,125","to":"00:00:01,000"},"offsets":{"from":120,"to":1000},"text":" first "},
            {"offsets":{"from":1050,"to":2020},"text":" second "}
          ]
        }"#;
        let output = parse_transcript(fixture, "auto", 2.0).unwrap();
        assert_eq!(output["language"], "zh");
        assert_eq!(output["engine"], ENGINE);
        assert_eq!(
            output["segments"],
            json!([
                {"start":0.12,"end":1.0,"text":"first"}, {"start":1.05,"end":2.0,"text":"second"}
            ])
        );
        assert_eq!(
            parse_transcript(fixture, "en", 2.0).unwrap_err(),
            "speech_invalid_output"
        );
        assert_eq!(
            parse_transcript(fixture, "auto", 1.0).unwrap_err(),
            "speech_invalid_output"
        );
        assert!(parse_transcript(br#"{"result":{"language":"zh"},"transcription":[{"offsets":{"from":60,"to":20},"text":"bad"}]}"#, "auto", 1.0).is_err());
        assert_eq!(language_code(Some("ZH-Hant-TW")).unwrap(), "zh");
        assert!(language_code(Some("yue")).is_err());
        assert!(language_code(Some("zh--TW")).is_err());
    }

    #[test]
    fn short_silence_discards_the_real_models_padded_blank_audio_marker_only() {
        // Captured from the pinned 1.9.5 CPU engine and base-q5_1 model on
        // the installer's 250-ms PCM probe, rather than a mocked engine.
        let blank = br#"{"result":{"language":"en"},"transcription":[{"offsets":{"from":0,"to":10000},"text":" [BLANK_AUDIO]"}]}"#;
        let output = parse_transcript(blank, "en", 0.25).unwrap();
        assert_eq!(output["segments"], json!([]));
        for text in ["real speech", "real speech [BLANK_AUDIO]", "[blank_audio]"] {
            let bytes = serde_json::to_vec(&json!({"result":{"language":"en"},"transcription":[{"offsets":{"from":0,"to":10000},"text":text}]})).unwrap();
            assert_eq!(parse_transcript(&bytes, "en", 0.25).unwrap_err(), "speech_invalid_output");
        }
        let unrelated = br#"{"result":{"language":"en"},"transcription":[{"offsets":{"from":1000,"to":10000},"text":"[BLANK_AUDIO]"}]}"#;
        assert_eq!(parse_transcript(unrelated, "en", 0.25).unwrap_err(), "speech_invalid_output");
    }

    #[test]
    fn missing_application_lock_never_treats_a_directory_or_path_binary_as_ready() {
        let data = tempfile::tempdir().unwrap();
        let assets = tempfile::tempdir().unwrap();
        for (id, version) in [(WHISPER_ID, WHISPER_VERSION), (DECODER_ID, DECODER_VERSION)] {
            let installed = crate::capability_store::root(data.path())
                .join(id)
                .join(version);
            fs::create_dir_all(installed.join("bin")).unwrap();
            for name in executable(id, capability_packs::platform()) {
                fs::write(installed.join("bin").join(name), b"not a worker").unwrap();
            }
        }
        assert!(open(data.path(), assets.path()).unwrap().is_none());
        assert!(decoder(data.path(), assets.path()).unwrap().is_none());
        assert!(!data.path().join("capabilities/managed-v2/.locks").exists());
    }

    #[test]
    fn the_whisper_lock_binds_the_upstream_model_and_excludes_loader_overrides() {
        let mut lock = json!({"schema":2,"platform":"windows-x64","files":[
            {"path":"LICENSE","size":8},
            {"path":"bin/whisper-cli.exe","size":1024,"executable":true},
            {"path":MODEL,"size":MODEL_BYTES,"sha256":MODEL_SHA256}
        ]});
        assert!(valid_manifest(WHISPER_ID, &lock));
        lock["files"][2]["sha256"] = json!("0".repeat(64));
        assert!(!valid_manifest(WHISPER_ID, &lock));
        lock["files"][2]["sha256"] = json!(MODEL_SHA256);
        lock["files"]
            .as_array_mut()
            .unwrap()
            .push(json!({"path":"bin/unknown.dll","size":8}));
        assert!(!valid_manifest(WHISPER_ID, &lock));
    }

    #[test]
    fn readiness_depends_on_the_install_probe_for_the_same_manifest() {
        let media = tempfile::tempdir().unwrap();
        let target = media.path().join(WHISPER_VERSION);
        fs::create_dir_all(target.join("bin")).unwrap();
        fs::write(
            target
                .join("bin")
                .join(executable(WHISPER_ID, capability_packs::platform())[0]),
            b"this executable must never be launched by status",
        )
        .unwrap();
        let lock =
            json!({"id":WHISPER_ID,"version":WHISPER_VERSION,"sha256":"fixed test manifest"});
        assert!(installed_probe(&target, &lock).is_err());
        record_install_probe(media.path(), &lock).unwrap();
        assert!(installed_probe(&target, &lock).is_ok());
        let mut changed = lock.clone();
        changed["sha256"] = json!("different tested bytes");
        assert!(installed_probe(&target, &changed).is_err());
        assert!(clear_install_probe(media.path(), &lock));
        assert!(installed_probe(&target, &lock).is_err());
    }

    #[test]
    #[ignore = "requires a native FFmpeg and FFprobe selected for the probe"]
    fn ffmpeg_install_probe_generates_probes_and_decodes_actual_pcm() {
        let ffmpeg = PathBuf::from(std::env::var_os("READMD_TEST_MEDIA_FFMPEG").unwrap());
        let ffprobe = PathBuf::from(std::env::var_os("READMD_TEST_MEDIA_FFPROBE").unwrap());
        assert!(ffmpeg.is_absolute() && ffprobe.is_absolute());
        let data = tempfile::tempdir().unwrap();
        let workspace =
            task_workspace::Workspace::create(data.path(), "media-engine-probe").unwrap();
        workspace
            .scope(|| probe_decoder(&ffmpeg, &ffprobe, workspace.path()))
            .unwrap();
        workspace.finish().unwrap();
    }
}
