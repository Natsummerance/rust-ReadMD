//! Bounded, compressed recovery storage. Never writes beside the user's document.
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{io::{Read, Write}, path::Path, sync::Mutex};

static LOCK: Mutex<()> = Mutex::new(());
pub static SAVE_LOCK: Mutex<()> = Mutex::new(());
pub const MAX_CONTENT: usize = 16 * 1024 * 1024;
const MAX_STORAGE: u64 = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 256;
const PER_DOCUMENT: usize = 10;
const RETENTION_MS: i64 = 30 * 24 * 60 * 60 * 1000;

#[derive(Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: String, pub doc_key: String, pub path: String, pub name: String,
    pub kind: String, pub reason: String, pub created: i64, pub bytes: u64,
    #[serde(default)]
    pub context: serde_json::Value,
}
fn now() -> i64 { crate::content::unix_millis(std::time::SystemTime::now()) }
fn valid_id(id: &str) -> bool { id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit()) }
fn entries(dir: &Path) -> Vec<Entry> {
    let mut out = Vec::new();
    if let Ok(files) = std::fs::read_dir(dir) {
        for file in files.flatten() {
            if file.path().extension().and_then(|e| e.to_str()) != Some("json") { continue; }
            if file.metadata().map(|m| m.len() > 8192).unwrap_or(true) { continue; }
            if let Ok(bytes) = std::fs::read(file.path()) {
                if bytes.len() > 8192 { continue; }
                if let Ok(entry) = serde_json::from_slice::<Entry>(&bytes) {
                    if valid_id(&entry.id) && file.path().file_stem().and_then(|v| v.to_str()) == Some(entry.id.as_str()) && dir.join(format!("{}.gz", entry.id)).is_file() { out.push(entry); }
                }
            }
        }
    }
    out.sort_by(|a,b| b.created.cmp(&a.created).then(b.id.cmp(&a.id)));
    out
}
fn remove(dir: &Path, id: &str) -> Result<(), String> {
    if !valid_id(id) { return Err("invalid_history_id".into()); }
    for suffix in ["json", "gz"] {
        let file = dir.join(format!("{id}.{suffix}"));
        match std::fs::remove_file(file) {
            Ok(()) => (), Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}
fn prune(dir: &Path, protected: Option<&str>) -> Result<(), String> {
    let mut seen = std::collections::HashMap::<String, usize>::new();
    let mut total = 0; let mut count = 0;
    let mut candidates = entries(dir);
    // Remove incomplete payload/metadata pairs left by an interrupted atomic write.
    let valid: std::collections::HashSet<_> = candidates.iter().map(|e| e.id.clone()).collect();
    if let Ok(files) = std::fs::read_dir(dir) {
        for file in files.flatten() {
            let path = file.path();
            if !matches!(path.extension().and_then(|s| s.to_str()), Some("gz" | "json")) { continue; }
            if let Some(id) = path.file_stem().and_then(|s| s.to_str()) {
                if valid_id(id) && !valid.contains(id) { remove(dir, id)?; }
            }
        }
    }
    candidates.sort_by_key(|entry| protected != Some(entry.id.as_str()));
    for entry in candidates {
        let n = seen.entry(entry.doc_key.clone()).or_default(); *n += 1;
        let size = ["gz", "json"].iter().map(|suffix| std::fs::metadata(dir.join(format!("{}.{suffix}", entry.id))).map(|m| m.len()).unwrap_or(0)).sum::<u64>();
        let keep = protected == Some(entry.id.as_str()) ||
            (entry.created >= now() - RETENTION_MS && *n <= PER_DOCUMENT && count < MAX_ENTRIES && total + size <= MAX_STORAGE);
        if keep { total += size; count += 1; } else { remove(dir, &entry.id)?; }
    }
    Ok(())
}
pub fn record(data: &Path, key: &str, path: &str, name: &str, kind: &str, reason: &str, text: &str) -> Result<Entry, String> {
    let context = serde_json::json!({"dir": Path::new(path).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()});
    record_with_context(data, key, path, name, kind, reason, text, context)
}
pub fn record_with_context(data: &Path, key: &str, path: &str, name: &str, kind: &str, reason: &str, text: &str, context: serde_json::Value) -> Result<Entry, String> {
    if text.len() > MAX_CONTENT { return Err("recovery_document_too_large".into()); }
    if key.is_empty() || key.len() > 4096 || path.len() > 4096 || name.len() > 1024 { return Err("invalid_recovery_document".into()); }
    if !matches!(kind, "draft" | "version" | "checkpoint" | "discarded") { return Err("invalid_recovery_kind".into()); }
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = data.join("document-history");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let id = format!("{:x}", Sha256::digest(format!("{key}\0{kind}\0{text}").as_bytes()));
    let entry = Entry { id: id.clone(), doc_key: key.into(), path: path.into(), name: name.into(),
        kind: kind.into(), reason: reason.chars().take(128).collect(), created: now(), bytes: text.len() as u64, context };
    let metadata = serde_json::to_vec(&entry).map_err(|e| e.to_string())?;
    if metadata.len() > 8192 { return Err("invalid_recovery_document".into()); }
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    let compressed = encoder.finish().map_err(|e| e.to_string())?;
    crate::content::write_bytes_atomic(&dir.join(format!("{id}.gz")), &compressed).map_err(|e| e.to_string())?;
    crate::content::write_bytes_atomic(&dir.join(format!("{id}.json")), &metadata).map_err(|e| e.to_string())?;
    // One current autosaved draft per document; explicitly discarded drafts remain recoverable.
    if kind == "draft" {
        for old in entries(&dir) {
            if old.doc_key == key && old.kind == "draft" && old.id != id { remove(&dir, &old.id)?; }
        }
    }
    prune(&dir, Some(&id))?;
    Ok(entry)
}
pub fn list(data: &Path, key: Option<&str>) -> Result<Vec<Entry>, String> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = data.join("document-history"); prune(&dir, None)?;
    Ok(entries(&dir).into_iter().filter(|e| key.map(|k| e.doc_key == k).unwrap_or(true)).collect())
}
pub fn read(data: &Path, id: &str) -> Result<(Entry, String), String> {
    if !valid_id(id) { return Err("invalid_history_id".into()); }
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = data.join("document-history");
    let entry = entries(&dir).into_iter().find(|e| e.id == id).ok_or("history_not_found")?;
    let file = std::fs::File::open(dir.join(format!("{id}.gz"))).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    GzDecoder::new(file).take((MAX_CONTENT + 1) as u64).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_CONTENT || bytes.len() as u64 != entry.bytes { return Err("corrupt_history".into()); }
    let text = String::from_utf8(bytes).map_err(|_| "corrupt_history".to_string())?;
    let expected = format!("{:x}", Sha256::digest(format!("{}\0{}\0{text}", entry.doc_key, entry.kind).as_bytes()));
    if expected != id { return Err("corrupt_history".into()); }
    Ok((entry, text))
}
pub fn delete(data: &Path, id: &str) -> Result<(), String> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner()); remove(&data.join("document-history"), id)
}
pub fn clear_draft(data: &Path, key: &str) -> Result<(), String> {
    clear_draft_if(data, key, None, None)
}
pub fn clear_draft_if(data: &Path, key: &str, saved: Option<&str>, before: Option<i64>) -> Result<(), String> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = data.join("document-history");
    let hash = saved.map(|text| format!("{:x}", Sha256::digest(format!("{key}\0draft\0{text}").as_bytes())));
    for entry in entries(&dir) {
        if entry.doc_key == key && entry.kind == "draft" &&
            (saved.is_none() || hash.as_deref() == Some(entry.id.as_str()) || before.map(|time| entry.created <= time).unwrap_or(false)) { remove(&dir, &entry.id)?; }
    }
    Ok(())
}
pub fn checkpoint_file(data: &Path, path: &Path, reason: &str) -> Result<Option<Entry>, String> {
    if !path.is_file() { return Ok(None); }
    let text = crate::content::read_text(path).map_err(|e| e.to_string())?;
    let key = crate::paths::canonicalize_or_clean(path).to_string_lossy().to_string();
    record(data, &key, &key, path.file_name().and_then(|n| n.to_str()).unwrap_or("document"), "version", reason, &text).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compressed_recovery_survives_restart_and_never_creates_document_siblings() {
        let data = tempfile::tempdir().unwrap(); let docs = tempfile::tempdir().unwrap();
        let path = docs.path().join("note.md"); std::fs::write(&path, "原稿").unwrap();
        let entry = checkpoint_file(data.path(), &path, "save").unwrap().unwrap();
        assert_eq!(read(data.path(), &entry.id).unwrap().1, "原稿");
        assert_eq!(std::fs::read_dir(docs.path()).unwrap().count(), 1);
        let first = record(data.path(), "draft", "", "new.md", "draft", "edit", "one").unwrap();
        let second = record(data.path(), "draft", "", "new.md", "draft", "edit", "two").unwrap();
        assert!(read(data.path(), &first.id).is_err());
        assert_eq!(read(data.path(), &second.id).unwrap().1, "two");
        clear_draft(data.path(), "draft").unwrap(); assert!(read(data.path(), &second.id).is_err());
    }
    #[test]
    fn deduplication_bounds_validation_and_expiry() {
        let data = tempfile::tempdir().unwrap();
        for n in 0..15 { record(data.path(), "doc", "", "name", "version", "save", &n.to_string()).unwrap(); }
        assert_eq!(list(data.path(), Some("doc")).unwrap().len(), PER_DOCUMENT);
        let a = record(data.path(), "same", "", "n", "checkpoint", "ai", "same text").unwrap();
        let b = record(data.path(), "same", "", "n", "checkpoint", "ai", "same text").unwrap(); assert_eq!(a.id,b.id);
        assert_eq!(list(data.path(), Some("same")).unwrap().len(), 1);
        assert!(read(data.path(), "../../settings").is_err());
        assert!(record(data.path(), "x", "", "n", "draft", "edit", &"x".repeat(MAX_CONTENT+1)).is_err());
        let mut expired = b.clone(); expired.created = now() - RETENTION_MS - 1;
        std::fs::write(data.path().join(format!("document-history/{}.json", b.id)), serde_json::to_vec(&expired).unwrap()).unwrap();
        assert!(list(data.path(), Some("same")).unwrap().is_empty());
    }

    #[test]
    fn interrupted_pairs_are_pruned_and_late_save_keeps_a_newer_draft() {
        let data = tempfile::tempdir().unwrap();
        let draft = record(data.path(), "doc", "", "n", "draft", "edit", "newer").unwrap();
        clear_draft_if(data.path(), "doc", Some("earlier"), Some(draft.created - 1)).unwrap();
        assert_eq!(read(data.path(), &draft.id).unwrap().1, "newer");
        let orphan = "a".repeat(64);
        let dir = data.path().join("document-history");
        std::fs::write(dir.join(format!("{orphan}.gz")), "orphan").unwrap();
        assert_eq!(list(data.path(), None).unwrap().len(), 1);
        assert!(!dir.join(format!("{orphan}.gz")).exists());
        std::fs::write(dir.join(format!("{}.gz", draft.id)), "broken").unwrap();
        assert!(read(data.path(), &draft.id).is_err());
        clear_draft_if(data.path(), "doc", Some("newer"), None).unwrap();
        assert!(list(data.path(), None).unwrap().is_empty());
        let context = serde_json::json!({"dir":"C:/source", "assets":[{"name":"photo.png","path":"C:/source/photo.png"}]});
        let restored = record_with_context(data.path(), "copy", "", "copy.md", "draft", "edit", "![photo](photo.png)", context.clone()).unwrap();
        assert_eq!(read(data.path(), &restored.id).unwrap().0.context, context);
    }
}
