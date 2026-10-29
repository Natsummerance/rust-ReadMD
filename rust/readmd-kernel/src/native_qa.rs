//! Explicit debug-only WKWebView integration driver. No public route, relaxed
//! CSP, chooser substitute or token substitute. Never compiled into a release.
use std::{fs, io::Write, path::{Path, PathBuf}, sync::OnceLock};
use serde_json::Value;

static OUTPUT: OnceLock<PathBuf> = OnceLock::new();
pub struct Driver { script: Option<String> }
impl Driver {
    pub fn from_profile(profile: &Path) -> Option<Self> {
        if std::env::var("READMD_NATIVE_QA").as_deref() != Ok("1") { return None; }
        let load = || -> Option<String> {
            if fs::read_to_string(profile.join(".readmd-native-qa-owned")).ok()?.trim() != "readmd-synthetic-native-qa-v1" { return None; }
            let script = fs::read_to_string(profile.join("native-qa.js")).ok()?;
            if script.len() > 64 * 1024 { return None; }
            Some(script)
        };
        let script = load().unwrap_or_else(|| {
            eprintln!("readmd: native QA requires an explicitly owned synthetic profile");
            std::process::exit(2)
        });
        let _ = OUTPUT.set(profile.join("native-qa-result.json"));
        Some(Self { script: Some(script) })
    }
    pub fn tick(&mut self, reader: &wry::WebView, ready: bool) {
        if ready {
            if let Some(script) = self.script.take() { let _ = reader.evaluate_script(&script); }
        }
    }
}
pub fn record(body: &str) -> bool {
    let Some(output) = OUTPUT.get() else { return false; };
    if body.len() > 16 * 1024 { return false; }
    let Ok(value) = serde_json::from_str::<Value>(body) else { return false; };
    let Some(result) = value.get("readmd_native_qa_result") else { return false; };
    let label = |value: &Value| value.as_str().is_some_and(|s| !s.is_empty() && s.len() < 128 && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'));
    if !result["ok"].is_boolean() || !label(&result["phase"]) || !result["checks"].as_array().is_some_and(|items| items.len() <= 64 && items.iter().all(label)) { return true; }
    // Do not accept document text, URLs, paths, tokens or arbitrary diagnostics.
    let safe = serde_json::json!({"ok":result["ok"],"phase":result["phase"],"checks":result["checks"]});
    if let Ok(mut file) = fs::OpenOptions::new().write(true).create_new(true).open(output) {
        if let Ok(bytes) = serde_json::to_vec_pretty(&safe) { let _ = file.write_all(&bytes).and_then(|_| file.sync_all()); }
    }
    true
}
