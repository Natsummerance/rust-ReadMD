//! Staged resources and rollback-safe materialization shared by conversion and saving.
//! A preview never owns files beside the input document.
use crate::content;
use pulldown_cmark::{Event, Parser, Tag};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{cell::RefCell, collections::HashMap, fs, path::{Path, PathBuf}};

const MAX_ASSET_BYTES: usize = 32 * 1024 * 1024;
const MAX_ASSET_TOTAL: usize = 512 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetHandle {
    pub path: String,
    pub name: String,
    pub reference: String,
}

struct Staging { root: PathBuf, assets: Vec<AssetHandle>, bytes: usize, error: Option<String> }
thread_local! { static STAGING: RefCell<Option<Staging>> = const { RefCell::new(None) }; }

/// The legacy parsers are synchronous. A scoped sink avoids changing their format
/// adapters while isolating concurrent HTTP requests and restoring state on panic.
pub fn with_staging<T>(data_dir: &Path, f: impl FnOnce() -> T) -> (T, Vec<AssetHandle>, Option<String>) {
    struct Restore(Option<Staging>);
    impl Drop for Restore { fn drop(&mut self) { STAGING.with(|s| { *s.borrow_mut() = self.0.take(); }); } }
    let _restore = Restore(STAGING.with(|s| s.replace(Some(Staging {
        root: data_dir.join("resource-staging"), assets: Vec::new(), bytes: 0, error: None,
    }))));
    let result = f();
    let stage = STAGING.with(|s| s.borrow_mut().take()).expect("resource staging scope");
    (result, stage.assets, stage.error)
}

fn extension(ext: &str) -> &str {
    match ext { "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "tif" | "tiff" | "emf" | "wmf" | "jp2" => ext, _ => "bin" }
}

pub fn stage(data_dir: &Path, bytes: &[u8], name: &str, ext: &str) -> Result<AssetHandle, String> {
    stage_in(&data_dir.join("resource-staging"), bytes, name, ext)
}

fn stage_in(root: &Path, bytes: &[u8], name: &str, ext: &str) -> Result<AssetHandle, String> {
    if bytes.is_empty() || bytes.len() > MAX_ASSET_BYTES { return Err("asset_size_limit".into()); }
    let ext = ext.trim_start_matches('.').to_ascii_lowercase();
    let ext = extension(&ext);
    let hash = format!("{:x}", Sha256::digest(bytes));
    let path = root.join(format!("{hash}.{ext}"));
    fs::create_dir_all(root).map_err(|_| "asset_staging_failed")?;
    if path.exists() {
        if fs::read(&path).ok().as_deref() != Some(bytes) { return Err("asset_identity_conflict".into()); }
    } else { content::write_bytes_atomic(&path, bytes).map_err(|_| "asset_staging_failed")?; }
    let stem: String = Path::new(name).file_stem().and_then(|s| s.to_str()).unwrap_or("image")
        .chars().filter(|c| c.is_alphanumeric() || "-_".contains(*c)).take(48).collect();
    let name = format!("{}.{}", if stem.is_empty() { "image" } else { &stem }, ext);
    let reference = format!("/raw?p={}", percent_encoding::utf8_percent_encode(&path.to_string_lossy(), percent_encoding::NON_ALPHANUMERIC));
    Ok(AssetHandle { path: path.to_string_lossy().into_owned(), name, reference })
}

/// `None` means no staging scope; `Some(None)` means an extraction failed.
pub fn extract(source: &Path, bytes: &[u8], ext: &str) -> Option<Option<String>> {
    STAGING.with(|slot| {
        let mut slot = slot.borrow_mut();
        let stage = slot.as_mut()?;
        if stage.error.is_some() { return Some(None); }
        if stage.bytes.saturating_add(bytes.len()) > MAX_ASSET_TOTAL || stage.assets.len() >= 10_000 {
            stage.error = Some("asset_total_limit".into()); return Some(None);
        }
        let name = source.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
        match stage_in(&stage.root, bytes, name, ext) {
            Ok(asset) => {
                let reference = asset.reference.clone();
                if !stage.assets.iter().any(|a| a.path == asset.path) {
                    stage.bytes += bytes.len(); stage.assets.push(asset);
                }
                Some(Some(reference))
            }
            Err(e) => { stage.error = Some(e); Some(None) }
        }
    })
}

/// Only Markdown image destinations are rewritten, never matching prose or code.
/// Unused staged images are not copied. Reference-style images are resolved to
/// equivalent inline images when their asset destination changes.
pub fn image_references(text: &str) -> Vec<(std::ops::Range<usize>, String)> {
    Parser::new(text).into_offset_iter().filter_map(|(e, r)| match e {
        Event::Start(Tag::Image { dest_url, .. }) => Some((r, dest_url.into_string())), _ => None,
    }).collect()
}

pub struct Materialized {
    pub text: String,
    pub assets: Vec<AssetHandle>,
    pub edits: Vec<crate::document_transaction::TextEdit>,
    created: Vec<PathBuf>,
    folder: Option<PathBuf>,
    committed: bool,
}
impl Materialized { pub fn commit(&mut self) { self.committed = true; } }
impl Drop for Materialized {
    fn drop(&mut self) {
        if self.committed { return; }
        for path in &self.created { let _ = fs::remove_file(path); }
        if let Some(dir) = &self.folder { let _ = fs::remove_dir(dir); }
    }
}

/// Caller holds the shared save lock through the document commit. Existing files
/// are never overwritten. On any error or rejected document write, Drop removes
/// only resources created by this transaction.
pub fn materialize(text: &str, target: &Path, assets: &[AssetHandle]) -> Result<Materialized, String> {
    let mut result = Materialized { text: text.into(), assets: Vec::new(), edits: Vec::new(), created: Vec::new(), folder: None, committed: false };
    let refs = image_references(text);
    let folder = target.parent().ok_or("asset_target_invalid")?.join("readmd-assets");
    if fs::symlink_metadata(&folder).is_ok_and(|m| m.file_type().is_symlink()) { return Err("asset_directory_symlink".into()); }
    let mut mappings = HashMap::new();
    let mut content_hashes = HashMap::new();
    // Index once per save rather than scanning the shared directory for every
    // image in a large book. Files are still compared before being reused.
    let existing: HashMap<_, _> = fs::read_dir(&folder).into_iter().flatten().filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| { let name = entry.file_name(); let name = name.to_string_lossy(); let (_, suffix) = name.rsplit_once('-')?; Some((suffix.to_string(), entry.path())) }).collect();
    let mut total = 0usize;
    for asset in assets {
        let mut aliases = vec![asset.reference.clone(), asset.path.clone(), asset.path.replace('\\', "/")];
        // CommonMark consumes backslashes before punctuation in legacy Windows
        // destinations (e.g. a temporary directory beginning with a dot).
        for raw in aliases.clone() {
            if let Some((_, decoded)) = image_references(&format!("![](<{raw}>)")).into_iter().next() { aliases.push(decoded); }
        }
        if !refs.iter().any(|(_, r)| aliases.contains(r)) { continue; }
        let source = Path::new(&asset.path);
        let metadata = fs::metadata(source).map_err(|_| "asset_missing")?;
        if !metadata.is_file() || metadata.len() > MAX_ASSET_BYTES as u64 { return Err("asset_size_limit".into()); }
        let bytes = fs::read(source).map_err(|_| "asset_read_failed")?;
        total = total.saturating_add(bytes.len());
        if total > MAX_ASSET_TOTAL { return Err("asset_total_limit".into()); }
        let hash = format!("{:x}", Sha256::digest(&bytes));
        let raw = Path::new(&asset.name).file_name().and_then(|n| n.to_str()).filter(|n| *n != "." && *n != "..").ok_or("asset_name_invalid")?;
        let stem: String = Path::new(raw).file_stem().and_then(|n| n.to_str()).unwrap_or("image").chars().filter(|c| c.is_alphanumeric() || "-_".contains(*c)).take(64).collect();
        let ext = extension(Path::new(raw).extension().and_then(|n| n.to_str()).unwrap_or("bin"));
        // Content identity, rather than an incrementing copy number, permits
        // shared deduplication across documents and names without scanning files.
        let name = format!("{}-{}.{}", if stem.is_empty() { "image" } else { &stem }, &hash[..24], ext);
        let dest = if let Some(path) = content_hashes.get(&(hash.clone(), ext.to_string())) { PathBuf::from(path) } else {
            // Reuse this content under a different readable name from a previous save.
            let suffix = format!("{}.{}", &hash[..24], ext);
            existing.get(&suffix).cloned().unwrap_or_else(|| folder.join(name))
        };
        if dest.exists() {
            if fs::read(&dest).ok().as_deref() != Some(bytes.as_slice()) { return Err("asset_identity_conflict".into()); }
        } else {
            if !folder.exists() { fs::create_dir(&folder).map_err(|_| "asset_directory_failed")?; result.folder = Some(folder.clone()); }
            content::write_bytes_atomic(&dest, &bytes).map_err(|_| "asset_write_failed")?;
            result.created.push(dest.clone());
        }
        content_hashes.insert((hash, ext.to_string()), dest.clone());
        let reference = format!("readmd-assets/{}", percent_encoding::utf8_percent_encode(&dest.file_name().unwrap().to_string_lossy(), percent_encoding::NON_ALPHANUMERIC));
        for old in aliases { mappings.insert(old, reference.clone()); }
        if !result.assets.iter().any(|a| a.path == dest.to_string_lossy()) {
            result.assets.push(AssetHandle { path: dest.to_string_lossy().into_owned(), name: dest.file_name().unwrap().to_string_lossy().into_owned(), reference });
        }
    }
    let titles: HashMap<_, _> = Parser::new(text).into_offset_iter().filter_map(|(event, range)| match event {
        Event::Start(Tag::Image { title, .. }) => Some((range.start, title.into_string())), _ => None,
    }).collect();
    for (range, old) in refs.into_iter().rev() {
        let Some(new) = mappings.get(&old) else { continue; };
        let raw = &text[range.clone()];
        if let Some(at) = raw.rfind("](") {
            let after = &raw[at + 2..];
            let leading = after.len() - after.trim_start().len();
            let start = at + 2 + leading + usize::from(after.trim_start().starts_with('<'));
            if raw[start..].starts_with(&old) {
                result.edits.push(crate::document_transaction::TextEdit { from: text[..range.start + start].encode_utf16().count(), to: text[..range.start + start + old.len()].encode_utf16().count(), insert: new.clone() });
                result.text.replace_range(range.start + start..range.start + start + old.len(), new);
                continue;
            }
        }
        // Obtain the original label and resolved title, including reference
        // definitions. The old definition itself remains available to links.
        let mut depth = 0; let mut end = None; let mut escaped = false;
        for (i, c) in raw.char_indices().skip(1) {
            if escaped { escaped = false; continue; }
            if c == '\\' { escaped = true; continue; }
            if c == '[' { depth += 1; }
            if c == ']' { depth -= 1; if depth == 0 { end = Some(i); break; } }
        }
        let end = end.ok_or("asset_reference_invalid")?;
        let title = titles.get(&range.start).filter(|s| !s.is_empty()).map(|s| format!(" \"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))).unwrap_or_default();
        let insert = format!("![{}]({new}{title})", &raw[2..end]);
        result.edits.push(crate::document_transaction::TextEdit { from: text[..range.start].encode_utf16().count(), to: text[..range.end].encode_utf16().count(), insert: insert.clone() });
        result.text.replace_range(range, &insert);
    }
    result.edits.sort_by_key(|edit| edit.from);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_is_central_and_materialization_is_deduplicated_and_reversible() {
        let dir = tempfile::tempdir().unwrap(); let docs = dir.path().join("docs"); fs::create_dir(&docs).unwrap();
        let ((), assets, error) = with_staging(dir.path(), || {
            assert!(extract(&docs.join("book.epub"), b"image bytes", "png").unwrap().is_some());
        });
        assert!(error.is_none()); assert_eq!(fs::read_dir(&docs).unwrap().count(), 0);
        let text = format!("中文 🧪\n\n![cover]({})\n\n`{}`\n\n````\n![code]({})\n````", assets[0].reference, assets[0].reference, assets[0].reference);
        let saved;
        { let mut m = materialize(&text, &docs.join("one.md"), &assets).unwrap();
            assert_eq!(m.assets.len(), 1); assert!(m.text.contains("![cover](readmd-assets/"));
            let transaction = crate::document_transaction::ContentTransaction { base_revision: crate::document_transaction::revision(&text), origin: "editor".into(), edits: m.edits.clone() };
            assert_eq!(crate::document_transaction::apply(&text, &transaction).unwrap().content, m.text);
            assert!(m.text.ends_with(&format!("![code]({})\n````", assets[0].reference))); saved = m.assets[0].path.clone(); m.commit(); }
        let mut renamed = assets.clone(); renamed[0].name = "different.png".into();
        { let mut m = materialize(&text, &docs.join("two.md"), &renamed).unwrap(); assert_eq!(m.assets[0].path, saved); m.commit(); }
        assert_eq!(fs::read_dir(docs.join("readmd-assets")).unwrap().count(), 1);
        let another = stage(dir.path(), b"another", "another", "png").unwrap();
        { let m = materialize(&format!("![new]({})", another.reference), &docs.join("failed.md"), &[another]).unwrap(); assert!(Path::new(&m.assets[0].path).exists()); }
        assert_eq!(fs::read_dir(docs.join("readmd-assets")).unwrap().count(), 1);
    }
    #[test]
    fn unused_resources_are_ignored_missing_used_resources_fail_and_reference_images_work() {
        let dir = tempfile::tempdir().unwrap(); let asset = stage(dir.path(), b"pixels", "plot", "png").unwrap();
        let text = format!("![plot][p]\n\n[p]: {} \"Caption\"\n", asset.reference);
        let m = materialize(&text, &dir.path().join("book.md"), &[asset.clone()]).unwrap();
        assert!(m.text.starts_with("![plot](readmd-assets/")); assert!(m.text.contains(" \"Caption\")")); drop(m);
        assert!(!dir.path().join("readmd-assets").exists());
        fs::remove_file(&asset.path).unwrap();
        assert!(materialize("nothing", &dir.path().join("book.md"), &[asset.clone()]).is_ok());
        assert!(materialize(&format!("![]({})", asset.reference), &dir.path().join("book.md"), &[asset]).is_err());
        assert!(!dir.path().join("readmd-assets").exists());
    }
}
