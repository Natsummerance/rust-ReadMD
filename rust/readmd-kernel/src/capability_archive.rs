//! Bounded v2 capability extraction into an owned, disposable staging root.
//! This is not a readiness check or an installer: the shared capability service
//! must still hold its resource lock, probe the engine and atomically commit.
//! Only an application-reviewed manifest is accepted. No script is executed.
use flate2::bufread::GzDecoder;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::{self, BufReader, Read, Write},
    path::{Path, PathBuf},
};

pub const MAGIC: &[u8; 8] = b"RMDPKG2\n";
pub const MAX_MANIFEST: usize = 8 * 1024 * 1024;
pub const MAX_DOWNLOAD: u64 = 2 * 1024 * 1024 * 1024;
pub const MAX_EXPANDED: u64 = 4 * 1024 * 1024 * 1024;
const MAX_FILE: u64 = 256 * 1024 * 1024;
// Complete pinned CTeX input currently needs 34,822 entries. Keep the
// independent 8 MiB header/4 GiB expanded limits; a larger file allowance
// does not authorize any extra file outside the application's fixed lock.
pub const MAX_FILES: usize = 40_000;
const BUFFER: usize = 64 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("capability_pack_invalid")]
    Invalid,
    #[error("cancelled")]
    Cancelled,
    #[error("capability_install_failed")]
    Io(#[from] io::Error),
}
type Result<T> = std::result::Result<T, Error>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u8,
    id: String,
    version: String,
    platform: String,
    files: Vec<Entry>,
    review: Value,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Entry {
    path: String,
    size: u64,
    compressed_size: u64,
    sha256: String,
    #[serde(default)]
    executable: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct Progress {
    pub files_done: usize,
    pub files_total: usize,
    pub expanded_bytes: u64,
    pub expanded_total: u64,
}

fn token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !matches!(value, "." | "..")
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}
pub(crate) fn safe_name(value: &str) -> bool {
    if value.is_empty() || value.len() > 240 || !value.is_ascii() {
        return false;
    }
    value.split('/').all(|part| {
        let stem = part
            .split('.')
            .next()
            .unwrap_or("")
            .trim_end()
            .to_ascii_uppercase();
        !part.is_empty()
            && part.len() <= 128
            && !matches!(part, "." | "..")
            && !part.ends_with([' ', '.'])
            && !matches!(
                stem.as_str(),
                "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" | "CLOCK$"
            )
            && !(stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
            && part
                .bytes()
                .all(|b| b >= 32 && b < 127 && !b"<>:\"\\|?*".contains(&b))
    })
}
fn manifest(expected: &Value) -> Result<(Manifest, u64)> {
    if serde_json::to_vec(expected)
        .map_err(|_| Error::Invalid)?
        .len()
        > MAX_MANIFEST
    {
        return Err(Error::Invalid);
    }
    let manifest: Manifest =
        serde_json::from_value(expected.clone()).map_err(|_| Error::Invalid)?;
    if manifest.schema != 2
        || !token(&manifest.id)
        || !token(&manifest.version)
        || !token(&manifest.platform)
        || !manifest.review.is_object()
        || manifest.files.is_empty()
        || manifest.files.len() > MAX_FILES
    {
        return Err(Error::Invalid);
    }
    let mut names = HashSet::new();
    let mut expanded = 0u64;
    let mut compressed = (MAX_MANIFEST + MAGIC.len() + 4) as u64;
    for file in &manifest.files {
        if !safe_name(&file.path)
            || !names.insert(file.path.to_ascii_lowercase())
            || file.size > MAX_FILE
            || file.compressed_size < 20
            || file.compressed_size > MAX_FILE + 1024 * 1024
            || file.sha256.len() != 64
            || !file
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
        {
            return Err(Error::Invalid);
        }
        expanded = expanded.checked_add(file.size).ok_or(Error::Invalid)?;
        compressed = compressed
            .checked_add(file.compressed_size)
            .ok_or(Error::Invalid)?;
    }
    if expanded > MAX_EXPANDED || compressed > MAX_DOWNLOAD {
        return Err(Error::Invalid);
    }
    for name in &names {
        for (index, _) in name.match_indices('/') {
            if names.contains(&name[..index]) {
                return Err(Error::Invalid);
            }
        }
    }
    Ok((manifest, expanded))
}
pub(crate) fn validate_manifest(expected: &Value) -> Result<()> {
    manifest(expected).map(|_| ())
}
fn real_directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(Error::Invalid);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(Error::Invalid);
        }
    }
    Ok(())
}
fn destination(root: &Path, name: &str) -> Result<PathBuf> {
    let mut current = root.to_path_buf();
    let parts = name.split('/').collect::<Vec<_>>();
    for part in &parts[..parts.len() - 1] {
        current.push(part);
        match fs::create_dir(&current) {
            Ok(()) => (),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e.into()),
        }
        real_directory(&current)?;
    }
    if !current.canonicalize()?.starts_with(root) {
        return Err(Error::Invalid);
    }
    Ok(current.join(parts.last().ok_or(Error::Invalid)?))
}
fn cancelled(check: &mut impl FnMut() -> bool) -> Result<()> {
    if check() {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
/// Inspect bounded metadata only. This does not authenticate a package; callers
/// must compare resource identity with an application lock before extraction.
pub(crate) fn inspect_header(mut input: impl Read) -> Result<Value> {
    read_header(&mut input, &mut || false)
}
fn read_header(input: &mut impl Read, check_cancel: &mut impl FnMut() -> bool) -> Result<Value> {
    let mut magic = [0u8; 8];
    input.read_exact(&mut magic).map_err(|_| Error::Invalid)?;
    if &magic != MAGIC {
        return Err(Error::Invalid);
    }
    let mut length = [0u8; 4];
    input.read_exact(&mut length).map_err(|_| Error::Invalid)?;
    let length = u32::from_le_bytes(length) as usize;
    if length == 0 || length > MAX_MANIFEST {
        return Err(Error::Invalid);
    }
    let mut header = vec![0u8; length];
    for chunk in header.chunks_mut(BUFFER) {
        cancelled(check_cancel)?;
        input.read_exact(chunk).map_err(|_| Error::Invalid)?;
    }
    serde_json::from_slice(&header).map_err(|_| Error::Invalid)
}

/// Parse and stage a v2 stream. `expected` must come from the application lock,
/// never from the upload itself. The caller owns the staging parent and must
/// check the complete archive's catalog length/digest at the transport boundary.
/// Success returns a TempDir, not a ready/installed engine. Dropping it removes
/// only this newly created task root; previous installations are untouched.
pub fn stage(
    input: impl Read,
    expected: &Value,
    parent: &Path,
    check_cancel: impl FnMut() -> bool,
    report: impl FnMut(Progress),
) -> Result<tempfile::TempDir> {
    stage_prepared(input, expected, parent, |_| Ok(()), check_cancel, report)
}
pub(crate) fn stage_prepared(
    input: impl Read,
    expected: &Value,
    parent: &Path,
    prepare: impl FnOnce(&Path) -> Result<()>,
    check_cancel: impl FnMut() -> bool,
    report: impl FnMut(Progress),
) -> Result<tempfile::TempDir> {
    real_directory(parent)?;
    let temporary = tempfile::Builder::new()
        .prefix("stream-install-")
        .tempdir_in(parent)?;
    stage_into(input, expected, temporary.path(), prepare, check_cancel, report)?;
    Ok(temporary)
}
/// Extract into a root whose durable ownership/lifecycle the installer manages.
/// Never delete the caller's directory, including on cancellation or corruption.
pub(crate) fn stage_into(
    mut input: impl Read,
    expected: &Value,
    root: &Path,
    prepare: impl FnOnce(&Path) -> Result<()>,
    mut check_cancel: impl FnMut() -> bool,
    mut report: impl FnMut(Progress),
) -> Result<()> {
    cancelled(&mut check_cancel)?;
    let (manifest, expanded_total) = manifest(expected)?;
    let actual = read_header(&mut input, &mut check_cancel)?;
    if &actual != expected {
        return Err(Error::Invalid);
    }
    cancelled(&mut check_cancel)?;
    real_directory(root)?;
    let root = root.canonicalize()?;
    real_directory(&root)?;
    prepare(&root)?;
    cancelled(&mut check_cancel)?;
    let mut progress = Progress {
        files_done: 0,
        files_total: manifest.files.len(),
        expanded_bytes: 0,
        expanded_total,
    };
    report(progress);
    let mut buffer = [0u8; BUFFER];
    for entry in &manifest.files {
        cancelled(&mut check_cancel)?;
        let file = destination(&root, &entry.path)?;
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&file)?;
        let limited = (&mut input).take(entry.compressed_size);
        let mut decoder = GzDecoder::new(BufReader::with_capacity(BUFFER, limited));
        let mut written = 0u64;
        let mut digest = Sha256::new();
        loop {
            cancelled(&mut check_cancel)?;
            let count = decoder.read(&mut buffer).map_err(|_| Error::Invalid)?;
            if count == 0 {
                break;
            }
            written = written.checked_add(count as u64).ok_or(Error::Invalid)?;
            if written > entry.size {
                return Err(Error::Invalid);
            }
            output.write_all(&buffer[..count])?;
            digest.update(&buffer[..count]);
            progress.expanded_bytes += count as u64;
            if progress.expanded_bytes == count as u64 {
                log::debug!(target:"readmd::capability::transaction", "staging payload written");
            }
            report(progress);
        }
        // bufread's single-member decoder leaves any compressed tail unread.
        // Reject it, including a second member or unexplained segment padding.
        let remaining = decoder.into_inner();
        if !remaining.buffer().is_empty()
            || remaining.into_inner().limit() != 0
            || written != entry.size
            || format!("{:x}", digest.finalize()) != entry.sha256
        {
            return Err(Error::Invalid);
        }
        cancelled(&mut check_cancel)?;
        output.flush()?;
        output.sync_all()?;
        #[cfg(unix)]
        if entry.executable {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&file, fs::Permissions::from_mode(0o755))?;
        }
        #[cfg(not(unix))]
        let _ = entry.executable;
        progress.files_done += 1;
        report(progress);
    }
    cancelled(&mut check_cancel)?;
    if input.read(&mut buffer[..1])? != 0 {
        return Err(Error::Invalid);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::GzEncoder, Compression};
    use serde_json::json;
    use std::{cell::Cell, io::Cursor};
    #[test]
    fn managed_extraction_failure_leaves_caller_ownership_and_unknown_files_intact() {
        let (expected, payload) = fixture(vec![("large.dat", vec![0; 2 * 1024 * 1024])]);
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("managed"); fs::create_dir(&root).unwrap();
        fs::write(root.join("user-note"), b"never recursively delete").unwrap();
        let cancelled = Cell::new(false);
        let result = stage_into(Cursor::new(stream(&expected, &payload)), &expected, &root, |_| Ok(()),
            || cancelled.get(), |p| if p.expanded_bytes >= BUFFER as u64 { cancelled.set(true); });
        assert!(matches!(result, Err(Error::Cancelled)));
        assert_eq!(fs::read(root.join("user-note")).unwrap(), b"never recursively delete");
        assert_eq!(fs::metadata(root.join("large.dat")).unwrap().len(), BUFFER as u64);
        // Reusing a nonempty root must not truncate a same-name file.
        let result = stage_into(Cursor::new(stream(&expected, &payload)), &expected, &root,
            |_| Ok(()), || false, |_| {});
        assert!(result.is_err());
        assert_eq!(fs::metadata(root.join("large.dat")).unwrap().len(), BUFFER as u64);
    }
    fn fixture(files: Vec<(&str, Vec<u8>)>) -> (Value, Vec<Vec<u8>>) {
        let mut entries = Vec::new();
        let mut payload = Vec::new();
        for (name, bytes) in files {
            let mut gzip = GzEncoder::new(Vec::new(), Compression::fast());
            gzip.write_all(&bytes).unwrap();
            let gzip = gzip.finish().unwrap();
            entries.push(json!({"path":name,"size":bytes.len(),"compressedSize":gzip.len(),"sha256":format!("{:x}",Sha256::digest(&bytes))}));
            payload.push(gzip);
        }
        (
            json!({"schema":2,"id":"fixture","version":"1","platform":"universal","files":entries,"review":{"license":"Synthetic QA, no downloaded engine"}}),
            payload,
        )
    }
    fn stream(manifest: &Value, payload: &[Vec<u8>]) -> Vec<u8> {
        let header = serde_json::to_vec(manifest).unwrap();
        let mut data = MAGIC.to_vec();
        data.extend_from_slice(&(header.len() as u32).to_le_bytes());
        data.extend_from_slice(&header);
        for part in payload {
            data.extend_from_slice(part);
        }
        data
    }
    fn reject(expected: &Value, bytes: &[u8]) {
        let parent = tempfile::tempdir().unwrap();
        let sentinel = parent.path().join("previous-version");
        fs::create_dir(&sentinel).unwrap();
        fs::write(sentinel.join("keep"), "previous").unwrap();
        assert!(stage(
            Cursor::new(bytes),
            expected,
            parent.path(),
            || false,
            |_| {}
        )
        .is_err());
        assert_eq!(
            fs::read_to_string(sentinel.join("keep")).unwrap(),
            "previous"
        );
        assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 1);
    }
    #[test]
    fn staging_preparation_precedes_payload_and_cancel_removes_only_stage() {
        let (expected, payload) = fixture(vec![("bin/engine", b"synthetic".to_vec())]);
        let bytes = stream(&expected, &payload);
        let parent = tempfile::tempdir().unwrap();
        let previous = parent.path().join("previous.txt");fs::write(&previous, b"keep").unwrap();
        let prepared = Cell::new(false);
        let result = stage_prepared(Cursor::new(bytes), &expected, parent.path(), |root| {
            assert!(root.starts_with(parent.path().canonicalize().unwrap()));
            assert_eq!(fs::read_dir(root).unwrap().count(), 0);
            prepared.set(true);Ok(())
        }, || prepared.get(), |_| {});
        assert!(matches!(result, Err(Error::Cancelled)));
        assert_eq!(fs::read(previous).unwrap(), b"keep");
        assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 1);
    }
    #[test]
    fn more_than_v1_file_budget_streams_in_small_transport_chunks() {
        let names = (0..97)
            .map(|i| format!("texmf/part-{i}.txt"))
            .collect::<Vec<_>>();
        let files = names
            .iter()
            .map(|name| (name.as_str(), vec![b'x'; 32769]))
            .collect();
        let (expected, payload) = fixture(files);
        let bytes = stream(&expected, &payload);
        struct Fragmented(Cursor<Vec<u8>>);
        impl Read for Fragmented {
            fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
                assert!(output.len() <= BUFFER);
                let count = output.len().min(37);
                self.0.read(&mut output[..count])
            }
        }
        let parent = tempfile::tempdir().unwrap();
        let mut last = None;
        let staged = stage(
            Fragmented(Cursor::new(bytes)),
            &expected,
            parent.path(),
            || false,
            |p| last = Some(p),
        )
        .unwrap();
        assert_eq!(
            fs::read(staged.path().join("texmf/part-96.txt")).unwrap(),
            vec![b'x'; 32769]
        );
        let last = last.unwrap();
        assert_eq!(last.files_done, 97);
        assert_eq!(last.expanded_bytes, last.expanded_total);
        drop(staged);
        assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 0);
    }
    #[test]
    fn paths_collisions_and_file_parent_conflicts_are_rejected() {
        let (original, payload) = fixture(vec![("bin/engine", b"test".to_vec())]);
        for name in [
            "/absolute",
            "../outside",
            "a/../b",
            "C:/outside",
            "a\\b",
            "a:b",
            "CON.txt",
            "CON .txt",
            "a/COM1 .exe",
            "CONIN$",
            "CONOUT$",
            "a\"b",
            "a/aux",
            "LPT2.log",
            "end.",
            "end ",
            "a//b",
            "a/*",
            "a/\u{7f}",
            "中.txt",
        ] {
            let mut expected = original.clone();
            expected["files"][0]["path"] = json!(name);
            reject(&expected, &stream(&expected, &payload));
        }
        for second in ["BIN/ENGINE", "bin/engine/child"] {
            let (expected, payload) = fixture(vec![
                ("bin/engine", b"first".to_vec()),
                (second, b"second".to_vec()),
            ]);
            reject(&expected, &stream(&expected, &payload));
        }
    }
    #[test]
    fn truncated_corrupt_trailing_and_concatenated_gzip_are_rejected() {
        let (expected, payload) = fixture(vec![("bin/engine", b"valid payload".to_vec())]);
        let bytes = stream(&expected, &payload);
        reject(&expected, &bytes[..bytes.len() - 1]);
        let mut extra = bytes.clone();
        extra.push(1);
        reject(&expected, &extra);
        let mut corrupt = payload.clone();
        corrupt[0][10] ^= 0xff;
        reject(&expected, &stream(&expected, &corrupt));
        let mut concat = payload.clone();
        concat[0].extend_from_slice(&payload[0]);
        let mut joined = expected.clone();
        joined["files"][0]["compressedSize"] = json!(concat[0].len());
        reject(&joined, &stream(&joined, &concat));
        let mut tail = payload.clone();
        tail[0].extend_from_slice(b"unexplained padding");
        let mut padded = expected.clone();
        padded["files"][0]["compressedSize"] = json!(tail[0].len());
        reject(&padded, &stream(&padded, &tail));
        let mut changed = expected.clone();
        changed["version"] = json!("2");
        reject(&expected, &stream(&changed, &payload));
    }
    #[test]
    fn expanded_bomb_and_declared_budget_fail_without_commit() {
        let (original, payload) = fixture(vec![("large.dat", vec![0; 4 * 1024 * 1024])]);
        let mut small = original.clone();
        small["files"][0]["size"] = json!(1);
        reject(&small, &stream(&small, &payload));
        for (field, size) in [("size", MAX_FILE + 1), ("compressedSize", MAX_DOWNLOAD + 1)] {
            let mut large = original.clone();
            large["files"][0][field] = json!(size);
            reject(&large, &stream(&large, &payload));
        }
        let mut digest = original.clone();
        digest["files"][0]["sha256"] = json!("0".repeat(64));
        reject(&digest, &stream(&digest, &payload));
        let mut oversized = MAGIC.to_vec();
        oversized.extend_from_slice(&((MAX_MANIFEST + 1) as u32).to_le_bytes());
        reject(&original, &oversized);
    }
    #[test]
    fn large_file_inventory_is_bounded_independently_of_header_size() {
        let (mut expected, _) = fixture(vec![("part", Vec::new())]);
        let entry = expected["files"][0].clone();
        expected["files"] = Value::Array(
            (0..MAX_FILES)
                .map(|i| {
                    let mut entry = entry.clone();
                    entry["path"] = json!(format!("texmf/{i}.sty"));
                    entry
                })
                .collect(),
        );
        assert!(validate_manifest(&expected).is_ok());
        expected["files"].as_array_mut().unwrap().push(
            json!({"path":"extra.sty", "size":0, "compressedSize":20, "sha256":"0".repeat(64)}),
        );
        assert!(validate_manifest(&expected).is_err());
        expected["files"].as_array_mut().unwrap().pop();
        for (i, item) in expected["files"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .enumerate()
        {
            item["path"] = json!(format!("{}/{i}.sty", "x".repeat(120)));
        }
        assert!(serde_json::to_vec(&expected).unwrap().len() > MAX_MANIFEST);
        assert!(validate_manifest(&expected).is_err());
    }
    #[test]
    fn cancellation_during_extraction_removes_only_staged_files() {
        let (expected, payload) = fixture(vec![("large.dat", vec![0; 2 * 1024 * 1024])]);
        let bytes = stream(&expected, &payload);
        let parent = tempfile::tempdir().unwrap();
        let flag = Cell::new(false);
        assert!(matches!(
            stage(
                Cursor::new(bytes),
                &expected,
                parent.path(),
                || flag.get(),
                |p| {
                    if p.expanded_bytes >= BUFFER as u64 {
                        flag.set(true);
                    }
                }
            ),
            Err(Error::Cancelled)
        ));
        assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 0);
    }
}
