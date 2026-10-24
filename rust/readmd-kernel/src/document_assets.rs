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
fn asset_events(text: &str) -> impl Iterator<Item = (Event<'_>, std::ops::Range<usize>)> {
    let bom = usize::from(text.starts_with('\u{feff}')) * 3;
    Parser::new_ext(&text[bom..], crate::md_ast::options()).into_offset_iter()
        .map(move |(event, range)| (event, range.start + bom..range.end + bom))
}
fn source_asset_events(text: &str) -> Result<(Box<dyn Iterator<Item = (Event<'_>, std::ops::Range<usize>)> + '_>, std::collections::HashSet<(usize, usize)>), String> {
    if !crate::publication_inline_text::has_owned_syntax(text) { return Ok((Box::new(asset_events(text)), std::collections::HashSet::new())); }
    // Resource discovery must not turn a partial author draft into a document
    // validation error. Only authenticated delimiters are transparent; failure
    // retains ordinary opaque HTML semantics and grants no owned-span rights.
    let model = match crate::publication_inline_text::get(text) {
        Ok(model) => model,
        Err("cancelled") => return Err("cancelled".into()),
        Err(_) => return Ok((Box::new(asset_events(text)), std::collections::HashSet::new())),
    };
    let events = match crate::publication_inline_text::markdown_events(text) {
        Ok(events) => events,
        Err("cancelled") => return Err("cancelled".into()),
        Err(_) => return Ok((Box::new(asset_events(text)), std::collections::HashSet::new())),
    };
    let edges = model.nodes.into_iter().flat_map(|node| [node.opening, node.closing].into_iter().flatten())
        .map(|range| (range.start, range.end)).collect();
    Ok((Box::new(events.into_iter()), edges))
}

pub fn image_references(text: &str) -> Vec<(std::ops::Range<usize>, String)> {
    source_image_references(text).map(|images| images.into_iter().map(|(range, url, _)| (range, url)).collect()).unwrap_or_default()
}


/// This is a strict closed-block proof, not a tolerant HTML interpretation.
/// Uncertain HTML keeps the existing opaque-scope handling. Nothing inside a
/// proven block is authorized as a Markdown image.
fn complete_asset_html_block(raw: &str) -> bool {
    use quick_xml::{events::Event as XmlEvent, Reader};
    fn void_tag(name: &[u8]) -> bool {
        matches!(name.to_ascii_lowercase().as_slice(), b"area" | b"base" | b"br" | b"col" | b"embed" | b"hr" | b"img" | b"input" | b"link" | b"meta" | b"param" | b"source" | b"track" | b"wbr")
    }
    fn safe_tag(tag: &quick_xml::events::BytesStart<'_>) -> bool {
        let name = tag.name(); let name = name.as_ref().as_bytes();
        name.first().is_some_and(u8::is_ascii_alphabetic)
            && name.iter().all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
            && !matches!(name.to_ascii_lowercase().as_slice(), b"script" | b"style" | b"textarea" | b"title" | b"xmp" | b"iframe" | b"noembed" | b"noframes" | b"plaintext" | b"noscript")
            && tag.attributes().with_checks(true).take(129).enumerate().all(|(index, attribute)| index < 128 && attribute.is_ok())
    }
    let mut reader = Reader::from_str(raw); let mut names = Vec::<Vec<u8>>::new();
    // HTML void starts need no end tag. Every non-void end is still checked
    // against our exact stack; no implicit closing or error recovery is used.
    reader.config_mut().check_end_names = false;
    loop {
        match reader.read_event() {
            Ok(XmlEvent::Start(tag)) => {
                if !safe_tag(&tag) { return false; }
                if !void_tag(tag.name().as_ref().as_bytes()) {
                    if names.len() >= 128 { return false; }
                    names.push(tag.name().as_ref().as_bytes().to_vec());
                }
            },
            Ok(XmlEvent::Empty(tag)) => {
                // HTML ignores a self-closing slash on non-void elements.
                if !safe_tag(&tag) || !void_tag(tag.name().as_ref().as_bytes()) { return false; }
            },
            Ok(XmlEvent::End(tag)) => {
                if names.pop().as_deref() != Some(tag.name().as_ref().as_bytes()) { return false; }
            },
            Ok(XmlEvent::Text(_) | XmlEvent::Comment(_)) => (),
            Ok(XmlEvent::Eof) => return names.is_empty(),
            _ => return false,
        }
    }
}

/// Only real Markdown images are candidates. Generic HTML scopes, inline code,
/// image alternative text and ordinary link destinations grant no staged-file access.
fn source_image_references(text: &str) -> Result<Vec<(std::ops::Range<usize>, String, String)>, String> {
    use pulldown_cmark::TagEnd;
    let mut refs = Vec::new();
    let mut html = Vec::<String>::new();
    let mut images = 0usize;
    let mut complete_html_block = false;
    let (events, owned_edges) = source_asset_events(text)?;
    for (event, range) in events {
        if crate::speech::cancelled() { return Err("cancelled".into()); }
        match event {
            Event::Start(Tag::HtmlBlock) => {
                complete_html_block = text.get(range).is_some_and(complete_asset_html_block);
            },
            Event::End(TagEnd::HtmlBlock) => complete_html_block = false,
            // A fully balanced parser block cannot leave an opaque scope open.
            // Unproven blocks retain the existing cross-block scope fallback.
            Event::Html(_) if complete_html_block => (),
            Event::Html(raw) | Event::InlineHtml(raw) => {
                if owned_edges.contains(&(range.start, range.end)) { continue; }
                let raw = raw.trim();
                if raw.starts_with("<!--") || raw.starts_with("<!") || raw.starts_with("<?") { continue; }
                let closing = raw.starts_with("</");
                let name = raw.trim_start_matches('<').trim_start_matches('/').chars()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | ':'))
                    .collect::<String>().to_ascii_lowercase();
                if name.is_empty() { continue; }
                if closing {
                    if let Some(at) = html.iter().rposition(|tag| tag == &name) { html.truncate(at); }
                } else if !raw.ends_with("/>") && !matches!(name.as_str(), "area" | "base" | "br" | "col" | "embed" | "hr" | "img" | "input" | "link" | "meta" | "param" | "source" | "track" | "wbr") {
                    // Beyond the bounded opaque stack, certify no further images.
                    // An inert draft must not become a save validation error.
                    if html.len() >= 128 { return Ok(refs); }
                    html.push(name);
                }
            },
            Event::Start(Tag::Image { dest_url, title, .. }) => {
                if images == 0 && html.is_empty() { refs.push((range, dest_url.into_string(), title.into_string())); }
                images += 1;
            },
            Event::End(TagEnd::Image) => images = images.saturating_sub(1),
            _ => (),
        }
    }
    Ok(refs)
}

fn plain_asset_metadata(path: &Path, directory: bool) -> Result<fs::Metadata, String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "asset_missing")?;
    if metadata.file_type().is_symlink() || metadata.is_dir() != directory
        || !directory && !metadata.is_file() { return Err("asset_reference_invalid".into()); }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 { return Err("asset_reference_invalid".into()); }
    }
    Ok(metadata)
}

fn staged_path(data_dir: &Path, reference: &str) -> Result<(PathBuf, String), String> {
    let encoded = reference.strip_prefix("/raw?p=").ok_or("asset_reference_invalid")?;
    if encoded.is_empty() || encoded.contains(['&', '#', '?', '=']) { return Err("asset_reference_invalid".into()); }
    let raw = encoded.as_bytes();
    let mut at = 0;
    while at < raw.len() {
        if raw[at] == b'%' {
            if at + 2 >= raw.len() || !raw[at+1].is_ascii_hexdigit() || !raw[at+2].is_ascii_hexdigit() { return Err("asset_reference_invalid".into()); }
            at += 3;
        } else { at += 1; }
    }
    let query = encoded.replace('+', " ");
    let decoded = percent_encoding::percent_decode_str(&query).decode_utf8().map_err(|_| "asset_reference_invalid")?;
    if decoded.chars().any(char::is_control) || decoded.split(['/', '\\']).any(|part| matches!(part, "." | "..")) { return Err("asset_reference_invalid".into()); }
    let path = PathBuf::from(decoded.as_ref());
    let root = data_dir.join("resource-staging");
    if !data_dir.is_absolute() || !path.is_absolute() || path.parent() != Some(root.as_path()) { return Err("asset_reference_invalid".into()); }
    let filename = path.file_name().and_then(|name| name.to_str()).ok_or("asset_reference_invalid")?;
    let (digest, ext) = filename.split_once('.').ok_or("asset_reference_invalid")?;
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || ext.is_empty() || extension(ext) != ext { return Err("asset_reference_invalid".into()); }
    let digest = digest.to_string();
    plain_asset_metadata(data_dir, true)?;
    plain_asset_metadata(&root, true)?;
    plain_asset_metadata(&path, false)?;
    let canonical_root = dunce::canonicalize(&root).map_err(|_| "asset_missing")?;
    if dunce::canonicalize(&path).map_err(|_| "asset_missing")?.parent() != Some(canonical_root.as_path()) { return Err("asset_reference_invalid".into()); }
    Ok((path, digest))
}

/// Read again at the actual materialization boundary. A file replaced after
/// discovery can never be silently published under its old content identity.
fn read_staged(data_dir: &Path, asset: &AssetHandle) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let (path, expected) = staged_path(data_dir, &asset.reference)?;
    if path != Path::new(&asset.path) { return Err("asset_reference_invalid".into()); }
    let mut open = fs::OpenOptions::new(); open.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Open the endpoint itself and deny write/delete sharing during this read.
        open.custom_flags(0x00200000).share_mode(0x1);
    }
    #[cfg(unix)] {
        use std::os::unix::fs::OpenOptionsExt;
        open.custom_flags(libc::O_NOFOLLOW);
    }
    let file = open.open(&path).map_err(|_| "asset_read_failed")?;
    let metadata = file.metadata().map_err(|_| "asset_read_failed")?;
    if !metadata.is_file() { return Err("asset_reference_invalid".into()); }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 { return Err("asset_reference_invalid".into()); }
    }
    if metadata.len() > MAX_ASSET_BYTES as u64 { return Err("asset_size_limit".into()); }
    let mut bytes = Vec::new();
    file.take(MAX_ASSET_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|_| "asset_read_failed")?;
    if bytes.is_empty() || bytes.len() > MAX_ASSET_BYTES { return Err("asset_size_limit".into()); }
    if format!("{:x}", Sha256::digest(&bytes)) != expected { return Err("asset_identity_conflict".into()); }
    let (after, _) = staged_path(data_dir, &asset.reference)?;
    if after != path { return Err("asset_reference_invalid".into()); }
    Ok(bytes)
}

/// Discover only this profile's content-addressed staging files. This does not
/// create directories, rewrite source, scan the profile or establish a registry.
pub(crate) fn discover_staged(text: &str, data_dir: &Path) -> Result<Vec<AssetHandle>, String> {
    // All Markdown images have a literal '!'; their decoded destinations
    // may spell '/raw?' with character references in the original source.
    if !text.contains('!') { return Ok(Vec::new()); }
    let mut assets = Vec::new(); let mut seen = std::collections::HashSet::new(); let mut total = 0usize;
    for (_, reference, _) in source_image_references(text)? {
        if !reference.starts_with("/raw?") || !seen.insert(reference.clone()) { continue; }
        if assets.len() >= 10_000 { return Err("asset_total_limit".into()); }
        let (path, _) = staged_path(data_dir, &reference)?;
        let ext = path.extension().and_then(|value| value.to_str()).ok_or("asset_reference_invalid")?;
        let asset = AssetHandle { path: path.to_string_lossy().into_owned(), name: format!("image.{ext}"), reference };
        let bytes = read_staged(data_dir, &asset)?;
        total = total.saturating_add(bytes.len());
        if total > MAX_ASSET_TOTAL { return Err("asset_total_limit".into()); }
        assets.push(asset);
    }
    Ok(assets)
}

fn asset_handle(path: &Path) -> std::io::Result<fs::File> {
    let mut open = fs::OpenOptions::new(); open.read(true);
    #[cfg(windows)] {
        use std::os::windows::fs::OpenOptionsExt;
        open.custom_flags(0x0220_0000).share_mode(7);
    }
    #[cfg(unix)] {
        use std::os::unix::fs::OpenOptionsExt;
        open.custom_flags(libc::O_NOFOLLOW);
    }
    let file = open.open(path)?;
    let metadata = file.metadata()?;
    if metadata.file_type().is_symlink() { return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "asset reparse point")); }
    #[cfg(windows)] {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 { return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "asset reparse point")); }
    }
    Ok(file)
}
fn same_asset_identity(original: &fs::File, path: &Path) -> bool {
    asset_handle(path).is_ok_and(|current| same_asset_handles(original, &current))
}
fn same_asset_handles(original: &fs::File, current: &fs::File) -> bool {
    #[cfg(windows)] {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::{Foundation::HANDLE, Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle}};
        let mut before = BY_HANDLE_FILE_INFORMATION::default(); let mut after = BY_HANDLE_FILE_INFORMATION::default();
        unsafe {
            if GetFileInformationByHandle(HANDLE(original.as_raw_handle()), &mut before).is_err()
                || GetFileInformationByHandle(HANDLE(current.as_raw_handle()), &mut after).is_err() { return false; }
        }
        return before.dwVolumeSerialNumber == after.dwVolumeSerialNumber
            && before.nFileIndexHigh == after.nFileIndexHigh && before.nFileIndexLow == after.nFileIndexLow;
    }
    #[cfg(unix)] {
        use std::os::unix::fs::MetadataExt;
        let (Ok(before), Ok(after)) = (original.metadata(), current.metadata()) else { return false; };
        return before.dev() == after.dev() && before.ino() == after.ino();
    }
    #[cfg(not(any(windows, unix)))] { let _ = (original, current); false }
}
fn asset_matches(path: &Path, expected: &[u8]) -> std::io::Result<bool> {
    use std::io::Read;
    let mut open = fs::OpenOptions::new(); open.read(true);
    #[cfg(windows)] {
        use std::os::windows::fs::OpenOptionsExt;
        open.custom_flags(0x0020_0000).share_mode(1);
    }
    #[cfg(unix)] {
        use std::os::unix::fs::OpenOptionsExt;
        open.custom_flags(libc::O_NOFOLLOW);
    }
    let file = open.open(path)?; let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() != expected.len() as u64 { return Ok(false); }
    #[cfg(windows)] {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 { return Ok(false); }
    }
    let mut bytes = Vec::new(); file.take(expected.len() as u64 + 1).read_to_end(&mut bytes)?;
    Ok(bytes.len() == expected.len() && Sha256::digest(&bytes) == Sha256::digest(expected))
}
struct PendingAssetPublication { path: PathBuf, identity: fs::File, keep: bool }
impl Drop for PendingAssetPublication {
    fn drop(&mut self) {
        // Missing identity evidence is a reason to preserve, never to remove
        // an external replacement or an unprovable path.
        if !self.keep && same_asset_identity(&self.identity, &self.path) { let _ = fs::remove_file(&self.path); }
    }
}
fn capture_published_asset(file: fs::File, target: &Path,
    capture: impl FnOnce(&Path) -> std::io::Result<fs::File>) -> Result<fs::File, String>
{
    // persist_noclobber already returned an ownership witness. Retain it even
    // if obtaining the read-only long-lived handle fails.
    let mut pending = PendingAssetPublication { path: target.to_path_buf(), identity: file, keep: false };
    let identity = capture(target).map_err(|_| "asset_write_failed")?;
    if !same_asset_handles(&pending.identity, &identity) { return Err("asset_identity_conflict".into()); }
    pending.keep = true;
    Ok(identity)
}
fn persist_asset_without_replacement(folder: &Path, target: &Path, bytes: &[u8]) -> Result<Option<fs::File>, String> {
    use std::io::Write;
    let mut temporary = tempfile::Builder::new().prefix(".readmd-asset-").tempfile_in(folder).map_err(|_| "asset_write_failed")?;
    temporary.write_all(bytes).map_err(|_| "asset_write_failed")?;
    temporary.as_file().sync_all().map_err(|_| "asset_write_failed")?;
    match temporary.persist_noclobber(target) {
        // Read-only witnesses permit another verified alias to use read sharing.
        Ok(file) => capture_published_asset(file, target, asset_handle).map(Some),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            if asset_matches(target, bytes).unwrap_or(false) { Ok(None) } else { Err("asset_identity_conflict".into()) }
        },
        Err(_) => Err("asset_write_failed".into()),
    }
}

pub struct Materialized {
    pub text: String,
    pub assets: Vec<AssetHandle>,
    pub edits: Vec<crate::document_transaction::TextEdit>,
    created: Vec<(PathBuf, fs::File)>,
    folder: Option<(PathBuf, fs::File)>,
    committed: bool,
}
impl Materialized { pub fn commit(&mut self) { self.committed = true; } }
impl Drop for Materialized {
    fn drop(&mut self) {
        if self.committed { return; }
        while let Some((path, identity)) = self.created.pop() {
            if same_asset_identity(&identity, &path) { let _ = fs::remove_file(&path); }
        }
        if let Some((dir, identity)) = &self.folder {
            if same_asset_identity(identity, dir) { let _ = fs::remove_dir(dir); }
        }
    }
}

/// Caller holds the shared save lock through the document commit. Existing files
/// are never overwritten. On any error or rejected document write, Drop removes
/// only resources created by this transaction.
pub fn materialize(text: &str, target: &Path, assets: &[AssetHandle]) -> Result<Materialized, String> {
    materialize_inner(text, target, assets, None)
}
pub(crate) fn materialize_staged(text: &str, target: &Path, assets: &[AssetHandle], data_dir: &Path) -> Result<Materialized, String> {
    materialize_inner(text, target, assets, Some(data_dir))
}
fn materialize_inner(text: &str, target: &Path, assets: &[AssetHandle], staged_data: Option<&Path>) -> Result<Materialized, String> {
    let mut result = Materialized { text: text.into(), assets: Vec::new(), edits: Vec::new(), created: Vec::new(), folder: None, committed: false };
    // Ordinary saves acquire no additional source parsing/resource budget.
    if assets.is_empty() { return Ok(result); }
    let images = source_image_references(text)?;
    let refs = images.iter().map(|(range, url, _)| (range.clone(), url.clone())).collect::<Vec<_>>();
    let folder = target.parent().ok_or("asset_target_invalid")?.join("readmd-assets");
    match fs::symlink_metadata(&folder) {
        Ok(_) => { plain_asset_metadata(&folder, true).map_err(|_| "asset_directory_symlink")?; },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(_) => return Err("asset_directory_failed".into()),
    }
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
        let bytes = if let Some(data_dir) = staged_data { read_staged(data_dir, asset)? } else {
            let metadata = fs::metadata(source).map_err(|_| "asset_missing")?;
            if !metadata.is_file() || metadata.len() > MAX_ASSET_BYTES as u64 { return Err("asset_size_limit".into()); }
            fs::read(source).map_err(|_| "asset_read_failed")?
        };
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
            if !asset_matches(&dest, &bytes).unwrap_or(false) { return Err("asset_identity_conflict".into()); }
        } else {
            let created_directory = match fs::create_dir(&folder) {
                Ok(()) => true,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
                Err(_) => return Err("asset_directory_failed".into()),
            };
            plain_asset_metadata(&folder, true).map_err(|_| "asset_directory_symlink")?;
            let directory = asset_handle(&folder).map_err(|_| "asset_directory_failed")?;
            if created_directory { result.folder = Some((folder.clone(), directory)); }
            if result.folder.as_ref().is_some_and(|(path, identity)| !same_asset_identity(identity, path)) { return Err("asset_directory_symlink".into()); }
            if let Some(identity) = persist_asset_without_replacement(&folder, &dest, &bytes)? {
                result.created.push((dest.clone(), identity));
            }
        }
        content_hashes.insert((hash, ext.to_string()), dest.clone());
        let reference = format!("readmd-assets/{}", percent_encoding::utf8_percent_encode(&dest.file_name().unwrap().to_string_lossy(), percent_encoding::NON_ALPHANUMERIC));
        for old in aliases { mappings.insert(old, reference.clone()); }
        if !result.assets.iter().any(|a| a.path == dest.to_string_lossy()) {
            result.assets.push(AssetHandle { path: dest.to_string_lossy().into_owned(), name: dest.file_name().unwrap().to_string_lossy().into_owned(), reference });
        }
    }
    let titles: HashMap<_, _> = images.into_iter().map(|(range, _, title)| (range.start, title)).collect();
    for (range, old) in refs.into_iter().rev() {
        let Some(new) = mappings.get(&old) else { continue; };
        let raw = &text[range.clone()];
        if let Some(at) = raw.ends_with(')').then(|| raw.rfind("](")).flatten() {
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
    fn staged_markdown(asset: &AssetHandle) -> String { format!("![current]({})", asset.reference) }
    #[test]
    fn staged_discovery_uses_only_current_profile_real_images_and_reading_does_not_create_files() {
        let dir = tempfile::tempdir().unwrap();
        let asset = stage(dir.path(), b"owned pixels", "original", "png").unwrap();
        let image = staged_markdown(&asset);
        let source = format!("{image}\n\n{image}\n\n[link]({})\n\n`{image}`\n\n```md\n{image}\n```\n\n<div>\n{image}\n</div>\n\nA <code>{image}</code>\n", asset.reference);
        let refs = discover_staged(&source, dir.path()).unwrap();
        assert_eq!(refs.len(), 1); assert_eq!(refs[0].path, asset.path);
        assert!(!dir.path().join("readmd-assets").exists());
        let mut saved = materialize_staged(&source, &dir.path().join("saved.md"), &refs, dir.path()).unwrap();
        assert_eq!(saved.assets.len(), 1); assert_eq!(saved.edits.len(), 2);
        let tx = crate::document_transaction::ContentTransaction { base_revision: crate::document_transaction::revision(&source), origin: "editor".into(), edits: saved.edits.clone() };
        assert_eq!(crate::document_transaction::apply(&source, &tx).unwrap().content, saved.text);
        assert!(saved.text.contains(&format!("[link]({})", asset.reference)));
        assert!(saved.text.contains(&format!("<code>{image}</code>")));
        saved.commit();
    }
    #[test]
    fn staged_reference_rejects_query_injection_foreign_paths_traversal_and_invalid_utf8() {
        let dir = tempfile::tempdir().unwrap(); let foreign = tempfile::tempdir().unwrap();
        let asset = stage(dir.path(), b"own", "own", "png").unwrap();
        let other = stage(foreign.path(), b"other", "other", "png").unwrap();
        for reference in [format!("{}&p=other", asset.reference), format!("{}&extra=1", asset.reference), format!("{}#fragment", asset.reference), asset.reference.replacen("?p=", "?q=", 1), "/raw?p=%GG".into(), "/raw?p=%FF".into(), "/raw?p=%00".into(), other.reference, asset.reference.replace("resource%2Dstaging%5C", "resource%2Dstaging%5C%2E%2E%5Cresource%2Dstaging%5C").replace("resource%2Dstaging%2F", "resource%2Dstaging%2F%2E%2E%2Fresource%2Dstaging%2F")] {
            assert!(discover_staged(&format!("![]({reference})"), dir.path()).is_err(), "{reference}");
        }
        assert!(!dir.path().join("readmd-assets").exists());
    }
    #[test]
    fn staged_reference_rejects_missing_corrupt_bad_name_directory_and_over_budget_files() {
        let dir = tempfile::tempdir().unwrap(); let asset = stage(dir.path(), b"own", "own", "png").unwrap();
        let markdown = staged_markdown(&asset);
        fs::write(&asset.path, b"changed").unwrap();
        assert_eq!(discover_staged(&markdown, dir.path()).unwrap_err(), "asset_identity_conflict");
        fs::remove_file(&asset.path).unwrap(); assert_eq!(discover_staged(&markdown, dir.path()).unwrap_err(), "asset_missing");
        fs::create_dir(&asset.path).unwrap(); assert_eq!(discover_staged(&markdown, dir.path()).unwrap_err(), "asset_reference_invalid");
        fs::remove_dir(&asset.path).unwrap(); let file = fs::File::create(&asset.path).unwrap(); file.set_len(MAX_ASSET_BYTES as u64 + 1).unwrap(); drop(file);
        assert_eq!(discover_staged(&markdown, dir.path()).unwrap_err(), "asset_size_limit");
        let wrong = Path::new(&asset.path).parent().unwrap().join("not-a-digest.png"); fs::write(&wrong, b"own").unwrap();
        let reference = format!("/raw?p={}", percent_encoding::utf8_percent_encode(&wrong.to_string_lossy(), percent_encoding::NON_ALPHANUMERIC));
        assert_eq!(discover_staged(&format!("![]({reference})"), dir.path()).unwrap_err(), "asset_reference_invalid");
    }
    #[test]
    fn changing_staged_bytes_after_discovery_fails_before_any_materialized_resource() {
        let dir = tempfile::tempdir().unwrap(); let asset = stage(dir.path(), b"original pixels", "own", "png").unwrap();
        let source = staged_markdown(&asset); let discovered = discover_staged(&source, dir.path()).unwrap();
        fs::write(&asset.path, b"replacement pixels").unwrap();
        assert!(matches!(materialize_staged(&source, &dir.path().join("saved.md"), &discovered, dir.path()), Err(code) if code=="asset_identity_conflict"));
        assert!(!dir.path().join("readmd-assets").exists());
    }
    #[test]
    fn checked_materialization_drop_rolls_back_only_new_resources_and_reuses_existing_content() {
        let dir = tempfile::tempdir().unwrap(); let asset = stage(dir.path(), b"original pixels", "own", "png").unwrap();
        let source = staged_markdown(&asset); let discovered = discover_staged(&source, dir.path()).unwrap();
        let first;
        { let mut saved = materialize_staged(&source, &dir.path().join("one.md"), &discovered, dir.path()).unwrap(); first = saved.assets[0].path.clone(); saved.commit(); }
        { let saved = materialize_staged(&source, &dir.path().join("two.md"), &discovered, dir.path()).unwrap(); assert_eq!(saved.assets[0].path, first); }
        assert_eq!(fs::read(&first).unwrap(), b"original pixels");
        let new = stage(dir.path(), b"new pixels", "own", "png").unwrap(); let text = staged_markdown(&new); let found = discover_staged(&text, dir.path()).unwrap();
        let fresh;
        { let saved = materialize_staged(&text, &dir.path().join("three.md"), &found, dir.path()).unwrap(); fresh = saved.assets[0].path.clone(); assert!(Path::new(&fresh).is_file()); }
        assert!(!Path::new(&fresh).exists()); assert_eq!(fs::read(&first).unwrap(), b"original pixels");
    }
    #[cfg(unix)]
    #[test]
    fn staged_file_and_parent_symlinks_are_rejected_without_materialization() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap(); let foreign = tempfile::tempdir().unwrap();
        let asset = stage(dir.path(), b"own", "own", "png").unwrap(); let source = staged_markdown(&asset);
        let outside = foreign.path().join(Path::new(&asset.path).file_name().unwrap()); fs::write(&outside, b"own").unwrap();
        fs::remove_file(&asset.path).unwrap(); symlink(&outside, &asset.path).unwrap();
        assert_eq!(discover_staged(&source, dir.path()).unwrap_err(), "asset_reference_invalid");
        fs::remove_file(&asset.path).unwrap(); fs::remove_dir(dir.path().join("resource-staging")).unwrap();
        symlink(foreign.path(), dir.path().join("resource-staging")).unwrap();
        assert_eq!(discover_staged(&source, dir.path()).unwrap_err(), "asset_reference_invalid");
    }

    #[test]
    fn staged_images_share_real_footnote_metadata_and_reference_definition_semantics() {
        let dir = tempfile::tempdir().unwrap(); let asset = stage(dir.path(), b"owned pixels", "own", "png").unwrap();
        let source = format!("\u{feff}---\npreview: \"![not-an-image]({})\"\n---\n\nBody[^n].\n\n[^n]: Note first paragraph.\n\n    ![continued][photo]\n\n[photo]: {} \"Current title\"\n\n```md\n![example]({})\n```\n", asset.reference, asset.reference, asset.reference);
        let refs = discover_staged(&source, dir.path()).unwrap(); assert_eq!(refs.len(), 1);
        let saved = materialize_staged(&source, &dir.path().join("note.md"), &refs, dir.path()).unwrap();
        assert_eq!(saved.edits.len(), 1); assert_eq!(saved.assets.len(), 1);
        assert!(saved.text.contains("    ![continued](readmd-assets/")); assert!(saved.text.contains(" \"Current title\")"));
        assert!(saved.text.contains(&format!("preview: \"![not-an-image]({})\"", asset.reference)));
        assert!(saved.text.contains(&format!("```md\n![example]({})\n```", asset.reference)));
        let tx = crate::document_transaction::ContentTransaction { base_revision: crate::document_transaction::revision(&source), origin: "editor".into(), edits: saved.edits.clone() };
        assert_eq!(crate::document_transaction::apply(&source, &tx).unwrap().content, saved.text);
    }

    #[test]
    fn staged_owned_image_scope_is_transparent_but_forged_html_and_nested_alt_are_not() {
        let dir = tempfile::tempdir().unwrap(); let asset = stage(dir.path(), b"owned pixels", "own", "png").unwrap();
        let source = format!("<span data-readmd-text=\"v1\" data-size-pt=\"30\">![literal $x$]({})</span>\n", asset.reference);
        let refs = discover_staged(&source, dir.path()).unwrap(); assert_eq!(refs.len(), 1);
        let saved = materialize_staged(&source, &dir.path().join("owned.md"), &refs, dir.path()).unwrap();
        assert!(saved.text.starts_with("<span data-readmd-text=\"v1\" data-size-pt=\"30\">![literal $x$](readmd-assets/"));
        assert_eq!(saved.edits.len(), 1);
        let forged = format!("A <span class=\"readmd-author-run\">![fake]({})</span>", asset.reference);
        assert!(discover_staged(&forged, dir.path()).unwrap().is_empty());
        let nested = format!("![outer ![not-an-image]({})](ordinary.png)", asset.reference);
        assert!(discover_staged(&nested, dir.path()).unwrap().is_empty());
    }

    #[test]
    fn shared_selector_nested_same_uri_and_reference_parent_have_disjoint_exact_edits() {
        let dir = tempfile::tempdir().unwrap(); let asset = stage(dir.path(), b"owned pixels", "own", "png").unwrap();
        for reference_parent in [false, true] {
            let image = if reference_parent {
                format!("![outer ![inner]({})][photo]\r\n\r\n[photo]: {} \"Outer title\"", asset.reference, asset.reference)
            } else { format!("![outer ![inner]({})]({})", asset.reference, asset.reference) };
            let source = format!("\u{feff}Unicode 🚀\r\n\r\n{image}\r\n");
            assert_eq!(image_references(&source).len(), 1);
            let found = discover_staged(&source, dir.path()).unwrap(); assert_eq!(found.len(), 1);
            for checked in [false, true] {
                let saved = if checked { materialize_staged(&source, &dir.path().join("saved.md"), &found, dir.path()).unwrap() }
                    else { materialize(&source, &dir.path().join("saved.md"), &found).unwrap() };
                assert_eq!(saved.edits.len(), 1); assert_eq!(saved.assets.len(), 1);
                assert!(saved.text.contains(&format!("![outer ![inner]({})](readmd-assets/", asset.reference)));
                if reference_parent { assert!(saved.text.contains(" \"Outer title\")")); }
                let tx = crate::document_transaction::ContentTransaction { base_revision: crate::document_transaction::revision(&source), origin: "editor".into(), edits: saved.edits.clone() };
                let result = crate::document_transaction::apply(&source, &tx).unwrap(); assert_eq!(result.content, saved.text);
                let inverse = crate::document_transaction::ContentTransaction { base_revision: result.revision.clone(), origin: "editor".into(), edits: result.inverse };
                assert_eq!(crate::document_transaction::apply(&saved.text, &inverse).unwrap().content, source);
            }
        }
    }

    #[test]
    fn noclobber_asset_commit_verifies_existing_bytes_without_overwriting_any_file() {
        let dir = tempfile::tempdir().unwrap(); let target = dir.path().join("shared.png");
        fs::write(&target, b"existing content").unwrap();
        assert!(persist_asset_without_replacement(dir.path(), &target, b"existing content").unwrap().is_none());
        assert!(matches!(persist_asset_without_replacement(dir.path(), &target, b"new content"), Err(code) if code=="asset_identity_conflict"));
        assert_eq!(fs::read(&target).unwrap(), b"existing content"); assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[test]
    fn rollback_preserves_replaced_file_and_directory_identities() {
        for replace_directory in [false, true] {
            let dir = tempfile::tempdir().unwrap(); let asset = stage(dir.path(), b"owned image", "own", "png").unwrap();
            let source = staged_markdown(&asset); let found = discover_staged(&source, dir.path()).unwrap();
            let guard = materialize_staged(&source, &dir.path().join("failed.md"), &found, dir.path()).unwrap();
            let file = PathBuf::from(&guard.assets[0].path); let folder = file.parent().unwrap().to_path_buf();
            let retained_image = dir.path().join("retained-original.png");
            // Windows cannot rename an ancestor directory while a child file
            // handle remains open, even with FILE_SHARE_DELETE. Move this owned
            // child outside the directory first; retain both identity witnesses.
            fs::rename(&file, &retained_image).unwrap();
            let replacement = if replace_directory {
                let retained_directory = dir.path().join("retained-original-directory");
                fs::rename(&folder, &retained_directory).unwrap(); fs::create_dir(&folder).unwrap();
                let original_identity = asset_handle(&retained_directory).unwrap();
                let foreign_identity = asset_handle(&folder).unwrap();
                assert!(fs::read_dir(&folder).unwrap().next().is_none());
                Some((retained_directory, original_identity, foreign_identity))
            } else { fs::write(&file, b"foreign replacement").unwrap(); None };
            drop(guard);
            assert_eq!(fs::read(&retained_image).unwrap(), b"owned image");
            if let Some((retained_directory, original_identity, foreign_identity)) = replacement {
                // The foreign directory is empty, so only the identity guard
                // prevents its removal; non-emptiness cannot mask a regression.
                assert!(same_asset_identity(&foreign_identity, &folder));
                assert!(same_asset_identity(&original_identity, &retained_directory));
                assert!(fs::read_dir(&folder).unwrap().next().is_none());
                assert!(fs::read_dir(&retained_directory).unwrap().next().is_none());
                assert!(!file.exists());
            } else { assert_eq!(fs::read(&file).unwrap(), b"foreign replacement"); }
            assert!(folder.is_dir());
        }
    }
    #[test]
    fn two_valid_staged_uri_aliases_share_one_readable_permanent_identity() {
        let dir = tempfile::tempdir().unwrap(); let asset = stage(dir.path(), b"owned pixels", "own", "png").unwrap();
        let alias = asset.reference.replace("%2F", "%2f").replace("%5C", "%5c"); assert_ne!(alias, asset.reference);
        let source = format!("![one]({})\n\n![two]({alias})", asset.reference); let found = discover_staged(&source, dir.path()).unwrap(); assert_eq!(found.len(), 2);
        let saved = materialize_staged(&source, &dir.path().join("saved.md"), &found, dir.path()).unwrap();
        assert_eq!(saved.assets.len(), 1); assert_eq!(saved.edits.len(), 2);
        let tx = crate::document_transaction::ContentTransaction { base_revision: crate::document_transaction::revision(&source), origin: "editor".into(), edits: saved.edits.clone() };
        assert_eq!(crate::document_transaction::apply(&source, &tx).unwrap().content, saved.text);
    }

    #[test]
    fn invalid_owned_drafts_use_opaque_html_without_hiding_real_images_outside_the_scope() {
        let dir = tempfile::tempdir().unwrap(); let asset = stage(dir.path(), b"owned pixels", "own", "png").unwrap();
        for tag in ["<span data-readmd-text=\"future\">", "<span data-readmd-text=\"v1\" data-size-pt=\"draft\">"] {
            let inert = format!("A {tag}![opaque]({})</span>\n\n`![](/raw?p=outside)` [ordinary](/raw?p=outside)\n", asset.reference);
            assert!(discover_staged(&inert, dir.path()).unwrap().is_empty());
            let outside = format!("{inert}\n![real]({})\n", asset.reference);
            let found = discover_staged(&outside, dir.path()).unwrap(); assert_eq!(found.len(), 1);
            let saved = materialize_staged(&outside, &dir.path().join("draft.md"), &found, dir.path()).unwrap();
            assert_eq!(saved.edits.len(), 1); assert!(saved.text.starts_with(&inert)); assert!(saved.text.contains("![real](readmd-assets/"));
            let invalid_outside = format!("{inert}\n![real](/raw?p=outside)\n");
            assert_eq!(discover_staged(&invalid_outside, dir.path()).unwrap_err(), "asset_reference_invalid");
        }
        let unfinished = "A <span data-readmd-text=\"v1\" data-size-pt=\"30\">unfinished\n\n`![](/raw?p=outside)` [ordinary](/raw?p=outside)\n";
        assert!(discover_staged(unfinished, dir.path()).unwrap().is_empty());
        let deep_inert = format!("A <span data-readmd-text=\"future\">{} `![](/raw?p=outside)` [ordinary](/raw?p=outside)", "<span>".repeat(140));
        assert!(discover_staged(&deep_inert, dir.path()).unwrap().is_empty());
    }
    #[test]
    fn post_publication_capture_failure_uses_the_original_noclobber_identity_witness() {
        let dir = tempfile::tempdir().unwrap();
        for replace in [false, true] {
            let target = dir.path().join(if replace { "replaced.png" } else { "owned.png" });
            let mut temp = tempfile::Builder::new().tempfile_in(dir.path()).unwrap();
            use std::io::Write; temp.write_all(b"owned bytes").unwrap();
            let file = temp.persist_noclobber(&target).unwrap(); let retained = dir.path().join("retained-before-capture.png");
            let result = capture_published_asset(file, &target, |path| {
                if replace { fs::rename(path, &retained)?; fs::write(path, b"foreign replacement")?; }
                Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "owned deterministic capture failure"))
            });
            assert!(matches!(result, Err(code) if code=="asset_write_failed"));
            if replace { assert_eq!(fs::read(&target).unwrap(), b"foreign replacement"); assert_eq!(fs::read(&retained).unwrap(), b"owned bytes"); }
            else { assert!(!target.exists(), "a witnessed self-created file must roll back when capture fails"); }
        }
    }


    #[test]
    fn entity_encoded_staged_destinations_are_real_images_with_exact_replay() {
        let dir = tempfile::tempdir().unwrap(); let asset = stage(dir.path(), b"entity image", "own", "png").unwrap();
        for spelling in ["/raw&#63;p=", "&#47;raw?p=", "/&#114;aw?p=", "&#47;raw&#63;p="] {
            let encoded = asset.reference.replacen("/raw?p=", spelling, 1);
            for reference_image in [false, true] {
                let image = if reference_image { "![Entity alt][photo]".to_string() }
                    else { format!("![Entity alt]({encoded} \"Entity title\")") };
                let suffix = if reference_image { format!("\r\n\r\n[photo]: {encoded} \"Entity title\"\r\n") } else { "\r\n".to_string() };
                let prefix = "\u{feff}Unicode 🚀\r\n\r\n";
                let source = format!("{prefix}{image}{suffix}"); assert!(!source.contains("/raw?"));
                let actual = asset_events(&source).filter_map(|(event, range)| match event {
                    Event::Start(Tag::Image { dest_url, title, .. }) => Some((range, dest_url.into_string(), title.into_string())), _ => None,
                }).collect::<Vec<_>>();
                assert_eq!(actual.len(), 1); assert_eq!(actual[0].1, asset.reference); assert_eq!(actual[0].2, "Entity title");
                assert_eq!(&source[actual[0].0.clone()], image);
                let discovered = discover_staged(&source, dir.path()).unwrap(); assert_eq!(discovered.len(), 1); assert_eq!(discovered[0].path, asset.path);
                for checked in [false, true] {
                    let target = dir.path().join("entity.md");
                    let saved = if checked { materialize_staged(&source, &target, &discovered, dir.path()).unwrap() }
                        else { materialize(&source, &target, &discovered).unwrap() };
                    assert_eq!(saved.assets.len(), 1); assert_eq!(saved.edits.len(), 1); assert_eq!(fs::read(&saved.assets[0].path).unwrap(), b"entity image");
                    let rewritten = format!("![Entity alt]({} \"Entity title\")", saved.assets[0].reference);
                    assert_eq!(saved.text, format!("{prefix}{rewritten}{suffix}"));
                    let tx = crate::document_transaction::ContentTransaction { base_revision: crate::document_transaction::revision(&source), origin: "editor".into(), edits: saved.edits.clone() };
                    assert_eq!(crate::document_transaction::apply(&source, &tx).unwrap().content, saved.text);
                }
            }
        }
    }

    #[test]
    fn raw_html_blocks_stay_opaque_without_hiding_the_following_real_image() {
        use pulldown_cmark::TagEnd;
        let dir = tempfile::tempdir().unwrap(); let asset = stage(dir.path(), b"after html", "own", "png").unwrap();
        let image = staged_markdown(&asset);
        for html in ["<div>example</div>\n\n".to_string(), "<div><span>example</span></div>\n\n".to_string(), "<div><br></div>\n\n".to_string(), "<div><br/></div>\n\n".to_string(),
            format!("<div>\n![not-an-image]({})\n</div>\n\n", asset.reference),
            format!("<!-- ![not-an-image]({}) -->\n\n", asset.reference)] {
            assert!(discover_staged(&html, dir.path()).unwrap().is_empty());
            let source = format!("{html}{image}\n"); let mut block = false; let mut closed = false; let mut real = Vec::new();
            for (event, range) in asset_events(&source) {
                match event {
                    Event::Start(Tag::HtmlBlock) => { assert!(!block); block = true; },
                    Event::Html(_) => assert!(block),
                    Event::End(TagEnd::HtmlBlock) => { assert!(block); block = false; closed = true; },
                    Event::Start(Tag::Image { dest_url, .. }) => { assert!(!block && closed); real.push((range, dest_url.into_string())); },
                    _ => (),
                }
            }
            assert!(!block && closed); assert_eq!(real.len(), 1); assert_eq!(real[0].1, asset.reference);
            assert_eq!(real[0].0, html.len()..html.len() + image.len());
            let selected = image_references(&source); assert_eq!(selected.len(), 1); assert_eq!(selected[0], real[0]);
            let discovered = discover_staged(&source, dir.path()).unwrap(); assert_eq!(discovered.len(), 1);
            let saved = materialize_staged(&source, &dir.path().join("after-html.md"), &discovered, dir.path()).unwrap();
            assert_eq!(saved.edits.len(), 1); assert_eq!(saved.assets.len(), 1);
            assert_eq!(saved.text, format!("{html}![current]({})\n", saved.assets[0].reference));
            let tx = crate::document_transaction::ContentTransaction { base_revision: crate::document_transaction::revision(&source), origin: "editor".into(), edits: saved.edits.clone() };
            assert_eq!(crate::document_transaction::apply(&source, &tx).unwrap().content, saved.text);
        }
        let cross_block = format!("<div>\n\n![opaque]({})\n\n</div>\n\n", asset.reference);
        let source = format!("{cross_block}{image}\n");
        assert_eq!(asset_events(&source).filter(|(event, _)| matches!(event, Event::Start(Tag::Image { .. }))).count(), 2);
        let selected = image_references(&source); assert_eq!(selected.len(), 1); assert_eq!(selected[0].0, cross_block.len()..cross_block.len() + image.len());
        let discovered = discover_staged(&source, dir.path()).unwrap(); assert_eq!(discovered.len(), 1);
        let saved = materialize_staged(&source, &dir.path().join("cross-block.md"), &discovered, dir.path()).unwrap();
        assert_eq!(saved.edits.len(), 1); assert_eq!(saved.text, format!("{cross_block}![current]({})\n", saved.assets[0].reference));
        for inert in [format!("A <span>{image}</span>"), format!("A <code>{image}</code>"), format!("`{image}`"), format!("~~~md\n{image}\n~~~"), format!("<div>\n\n{image}\n")] {
            assert!(discover_staged(&inert, dir.path()).unwrap().is_empty());
        }
    }

    #[test]
    fn html_block_proof_requires_real_balanced_tags_not_quoted_or_commented_closers() {
        for complete in ["<div>example</div>", "<div title=\"quoted >\"><!-- </div> --><span>ok</span></div>", "<!-- <div> -->", "<div><br/></div>", "<div><br></div>"] {
            assert!(complete_asset_html_block(complete), "{complete}");
        }
        for opaque in ["<div>", "<div title=\"</div>\">", "<div><!-- </div> -->", "<div><span></div></span>", "<span/>", "<script>var text = '</div>';</script>", "<style>div:before {content: '</div>';}</style>", "<div title=\"unfinished>", "<div><br></br></div>", "<div><br></span>"] {
            assert!(!complete_asset_html_block(opaque), "{opaque}");
        }
        let attributes = (0..128).map(|index| format!(" data-a{index}=\"value\"")).collect::<String>();
        assert!(complete_asset_html_block(&format!("<div{attributes}></div>")));
        assert!(!complete_asset_html_block(&format!("<div{attributes} data-over=\"value\"></div>")));
    }

}
