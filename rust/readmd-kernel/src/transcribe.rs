//! 音视频转写：`src/readmd_modules/transcribe.py` 的 Rust 对等实现。
//!
//! 通过Rust管理的系统离线识别器处理文件；非WAV由已有ffmpeg转PCM。
//! 不加载Python插件。没有引擎时保留明确的降级说明；真实解码/识别失败
//! 返回错误，不能用说明Markdown冒充成功转写。

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Python `SUPPORTED_AUDIO_VIDEO_EXTS`，顺序与内容都不许改动。
pub const SUPPORTED_AUDIO_VIDEO_EXTS: &[&str] = &[
    ".mp3", ".wav", ".m4a", ".mp4", ".flac", ".ogg", ".webm", ".aac", ".wma", ".mkv", ".mov",
    ".avi",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TranscribeErrorCode {
    #[serde(rename = "transcribe_dependency_missing")]
    TranscribeDependencyMissing,
    #[serde(rename = "transcribe_audio_not_found")]
    TranscribeAudioNotFound,
    #[serde(rename = "transcribe_audio_format_unsupported")]
    TranscribeAudioFormatUnsupported,
    #[serde(rename = "transcribe_process_failed")]
    TranscribeProcessFailed,
    #[serde(rename = "transcribe_invalid_audio")]
    TranscribeInvalidAudio,
}

#[derive(Debug, Clone)]
pub struct TranscribeError {
    pub code: String,
    pub error_code: TranscribeErrorCode,
}

impl std::fmt::Display for TranscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code)
    }
}

/// Python `transcribe.load()`：刻意保持轻量，只返回 True。
pub fn load() -> Result<(), String> {
    Ok(())
}

/// Python `is_supported_media(path)`。
pub fn is_supported_media(path: &str) -> bool {
    let ext = extension(path);
    SUPPORTED_AUDIO_VIDEO_EXTS.contains(&ext.as_str())
}

/// `os.path.splitext(path)[1].lower()`。
fn extension(path: &str) -> String {
    let tail = match path.rfind(['/', '\\']) {
        Some(i) => &path[i + 1..],
        None => path,
    };
    match tail.rfind('.') {
        Some(0) | None => String::new(),
        Some(i) => tail[i..].to_ascii_lowercase(),
    }
}

/// Python `os.path.splitext(path)[1].lstrip('.').lower()`。
fn extension_stripped(path: &str) -> String {
    extension(path).trim_start_matches('.').to_string()
}

fn basename(path: &str) -> String {
    match path.rfind(['/', '\\']) {
        Some(i) => path[i + 1..].to_string(),
        None => path.to_string(),
    }
}

/// Python `format_timestamp(seconds, brackets=True)`。
pub fn format_timestamp(seconds: f64, brackets: bool) -> String {
    let total = seconds as i64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let secs = total % 60;
    let ts = if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, minutes, secs)
    } else {
        format!("{:02}:{:02}", minutes, secs)
    };
    if brackets {
        format!("[{}]", ts)
    } else {
        ts
    }
}

/// 一条 Whisper 段落，只需要 `start` 与 `text`。
#[derive(Debug, Clone, Default)]
pub struct Segment {
    pub start: f64,
    pub text: String,
}

/// Python `format_segments(...)`。
pub fn format_segments(
    segments: &[Segment],
    title: Option<&str>,
    language: Option<&str>,
    duration: Option<f64>,
    file_format: Option<&str>,
    model_name: Option<&str>,
) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut frontmatter: Vec<String> = Vec::new();
    if let Some(t) = title {
        frontmatter.push(format!("title: \"{}\"", t));
    }
    if let Some(f) = file_format {
        frontmatter.push(format!("format: \"{}\"", f));
    }
    if let Some(d) = duration {
        if d >= 0.0 {
            frontmatter.push(format!("duration: \"{}\"", format_timestamp(d, false)));
        }
    }
    if let Some(m) = model_name {
        frontmatter.push(format!("model: \"{}\"", m));
    }
    if let Some(l) = language {
        frontmatter.push(format!("language: \"{}\"", l));
    }
    if !frontmatter.is_empty() {
        lines.push("---".to_string());
        lines.extend(frontmatter);
        lines.push("---".to_string());
    }
    if let Some(t) = title {
        lines.push(format!("# 音频/视频转写：{}", t));
    }
    if let Some(l) = language {
        lines.push(format!("> 识别语言：`{}`", l));
    }
    for seg in segments {
        let text = seg.text.trim();
        if !text.is_empty() {
            lines.push(format!("**{}** {}", format_timestamp(seg.start, true), text));
        }
    }
    let has_valid = segments.iter().any(|s| !s.text.trim().is_empty());
    if !has_valid {
        if let Some(t) = title {
            if !t.is_empty() {
                lines.push("> （未识别到有效语音内容）".to_string());
            }
        }
    }
    let joined = lines.join("\n\n");
    format!("{}\n", joined.trim())
}

/// Stable code for the "no transcription engine" state, for UI localisation.
pub const TRANSCRIBE_UNAVAILABLE: &str = "transcribe_unavailable";

/// The placeholder document written when no local transcription engine exists.
/// It states the fact plainly; it does not ask the user to install Python
/// packages (ReadMD has no Python runtime).
pub fn make_whisper_notice(path: &str, details: &str) -> String {
    let title = basename(path);
    let ext = extension_stripped(path);
    format!(
        "---\ntitle: \"{title}\"\nformat: \"{ext}\"\nstatus: \"unprocessed\"\n---\n\n\
         # 音频/视频转写：{title}\n\n\
         > **{details}**\n>\n\
         > 系统离线语音识别器不可用，当前文件未转写为文字。\n\
         > 可以先用其他工具导出字幕（`.srt` / `.vtt` / `.txt`），再用 ReadMD 打开或转换。\n"
    )
}

/// Python `transcribe_to_md(path, language=None, model_name='base')`
/// → `(text, error)`，两者都可能为 `None`。
pub fn transcribe_to_md(
    path: &str,
    language: Option<&str>,
    model_name: &str,
) -> (Option<String>, Option<String>) {
    if !Path::new(path).is_file() {
        return (None, Some("file_not_found".to_string()));
    }
    if !is_supported_media(path) {
        return (None, Some("unsupported_media_format".to_string()));
    }
    let _ = model_name;
    if crate::speech::capabilities().get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
        return (None, Some(TRANSCRIBE_UNAVAILABLE.to_string()));
    }
    match crate::speech::transcribe(path, language) {
        Ok(result) => {
            let segments: Vec<Segment> = result.get("segments").and_then(serde_json::Value::as_array).into_iter().flatten()
                .filter_map(|s| Some(Segment { start:s.get("start")?.as_f64()?, text:s.get("text")?.as_str()?.to_string() })).collect();
            if segments.iter().all(|s|s.text.trim().is_empty()) {return (None,Some("speech_no_result".into()));}
            (Some(format_segments(&segments, Some(&basename(path)), result.get("language").and_then(serde_json::Value::as_str),
                result.get("duration").and_then(serde_json::Value::as_f64), Some(&extension_stripped(path)), Some("system-speech"))), None)
        }
        Err(error) => (None, Some(error)),
    }
}

/// 既有内核调用点（`convert.rs` 的音视频分支、`batch2::h_transcribe`）使用的
/// 兼容入口：Python 的 `convert._convert_media` 拿到 notice 文本时算成功。
pub fn transcribe_audio(
    audio_path: &str,
    language: Option<&str>,
    model: Option<&str>,
) -> Result<String, TranscribeError> {
    let model_name = model.unwrap_or("base");
    let (text, err) = transcribe_to_md(audio_path, language, model_name);
    match text {
        Some(t) if err.is_none() && !t.trim().is_empty() => Ok(t),
        _ => Err(TranscribeError {
            code: err.unwrap_or_else(|| "transcribe_empty".to_string()),
            error_code: if !Path::new(audio_path).is_file() {
                TranscribeErrorCode::TranscribeAudioNotFound
            } else {
                TranscribeErrorCode::TranscribeDependencyMissing
            },
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_media(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("readmd-transcribe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(name);
        let mut handle = std::fs::File::create(&file).unwrap();
        handle.write_all(b"fake audio bytes").unwrap();
        file
    }

    #[test]
    fn module_load_is_always_ready_like_python() {
        assert!(load().is_ok());
    }

    #[test]
    fn supported_media_matches_the_python_extension_tuple() {
        assert_eq!(SUPPORTED_AUDIO_VIDEO_EXTS.len(), 12);
        for name in [".mp3", ".wav", ".m4a", ".mp4", ".flac", ".ogg", ".webm", ".aac", ".wma", ".mkv", ".mov", ".avi"] {
            assert!(is_supported_media(&format!("C:\\m\\x{}", name)), "{}", name);
        }
        assert!(is_supported_media("C:\\m\\x.MP3"));
        assert!(!is_supported_media("C:\\m\\x.flv"));
        assert!(!is_supported_media("C:\\m\\notes.txt"));
        assert!(!is_supported_media("C:\\m\\noext"));
    }

    #[test]
    fn timestamps_match_the_python_formatter() {
        assert_eq!(format_timestamp(65.9, true), "[01:05]");
        assert_eq!(format_timestamp(65.9, false), "01:05");
        assert_eq!(format_timestamp(3661.0, true), "[01:01:01]");
        assert_eq!(format_timestamp(0.0, false), "00:00");
    }

    #[test]
    fn degraded_notice_is_explicitly_unprocessed() {
        let file = temp_media("degraded.mp3");
        let path = file.to_str().unwrap();
        let notice = make_whisper_notice(path, "未检测到可用的系统离线语音识别器");
        assert!(notice.starts_with("---\ntitle: \"degraded.mp3\"\nformat: \"mp3\"\nstatus: \"unprocessed\"\n---\n\n"), "{notice}");
        assert!(notice.contains("# 音频/视频转写：degraded.mp3"), "{notice}");
        let lower = notice.to_lowercase();
        assert!(!lower.contains("pip") && !lower.contains("python") && !notice.contains("插件中心"), "{notice}");
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn missing_and_unsupported_files_short_circuit() {
        let (text, err) = transcribe_to_md("C:\\nope\\gone.mp3", None, "base");
        assert!(text.is_none());
        assert_eq!(err.as_deref(), Some("file_not_found"));
        let file = temp_media("notes.txt");
        let (text, err) = transcribe_to_md(file.to_str().unwrap(), None, "base");
        assert!(text.is_none());
        assert_eq!(err.as_deref(), Some("unsupported_media_format"));
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn segments_render_frontmatter_and_empty_notice() {
        let md = format_segments(
            &[Segment { start: 0.0, text: "你好".into() }, Segment { start: 75.0, text: "  ".into() }],
            Some("a.mp3"),
            Some("zh"),
            Some(80.0),
            Some("mp3"),
            Some("base"),
        );
        assert!(md.starts_with("---\n\ntitle: \"a.mp3\"\n"), "{md}");
        assert!(md.contains("duration: \"01:20\""), "{md}");
        assert!(md.contains("**[00:00]** 你好"), "{md}");
        assert!(!md.contains("未识别到有效语音内容"), "{md}");

        let empty = format_segments(&[], Some("a.mp3"), None, None, Some("mp3"), Some("base"));
        assert!(empty.contains("> （未识别到有效语音内容）"), "{empty}");
        assert!(empty.ends_with('\n'), "{empty}");
    }
}
