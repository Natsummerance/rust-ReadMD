//! Original Office page layout, through the pinned office-layout worker.
//!
//! This consumes an already-authorized byte snapshot, never the source path.
//! A missing engine is an explicit capability error; Markdown/HTML reflow is
//! not a fallback for this view. Workers receive no inherited credentials,
//! profile, tool search path, stdin, or document-relative filesystem access.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

const VERSION: &str = "0.8.6";
const COMMIT: &str = "96e218055a41261c3c12c0ea1b07f143424f368d";
const INVALID: &str = "office_preview_invalid";
const UNAVAILABLE: &str = "office_preview_unavailable";
const TOO_LARGE: &str = "office_preview_too_large";
const FAILED: &str = "office_preview_failed";
const MAX_INPUT: usize = 64 * 1024 * 1024;
const MAX_PDF: u64 = 64 * 1024 * 1024;
const MAX_LOG: u64 = 1024 * 1024;
const MAX_FILES: usize = 8192;
const TIMEOUT: Duration = Duration::from_secs(45);
static ACTIVE: AtomicBool = AtomicBool::new(false);
type Result<T> = std::result::Result<T, &'static str>;

struct Slot;
impl Drop for Slot { fn drop(&mut self) { ACTIVE.store(false, Ordering::Release); } }
fn checkpoint(deadline: Instant) -> Result<()> {
    if crate::speech::cancelled() { Err("office_preview_cancelled") }
    else if Instant::now() >= deadline { Err("office_preview_timeout") } else { Ok(()) }
}

/// DOCX/PPTX -> actual vector PDF using the captured source bytes.
pub(crate) fn render_pdf(source_name: &OsStr, ext: &str, bytes: &[u8], paths: &crate::paths::AppPaths) -> Result<Vec<u8>> {
    if !matches!(ext, "docx" | "pptx") { return Err(INVALID); }
    let name = snapshot_name(source_name, ext)?;
    if bytes.len() > MAX_INPUT { return Err(TOO_LARGE); }
    let deadline = Instant::now() + TIMEOUT;
    checkpoint(deadline)?;
    let _slot = ACTIVE.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map(|_| Slot).map_err(|_| "office_preview_busy")?;
    crate::office_package::validate(ext, bytes, deadline)?;
    let engine = resolve(paths, deadline)?;
    let task = crate::task_workspace::Workspace::create(&paths.data_dir, "office-preview")
        .map_err(|e| if e.kind() == std::io::ErrorKind::WouldBlock { "office_preview_busy" } else { FAILED })?;
    task.run_to_completion(|| render_at(&name, bytes, &engine, task.path(), deadline))
        .map_err(|_| FAILED)?
}

fn snapshot_name(source_name: &OsStr, ext: &str) -> Result<PathBuf> {
    let text = source_name.to_str().ok_or(INVALID)?;
    let path = Path::new(source_name);
    if text.is_empty() || text.len() > 240 || text.encode_utf16().count() > 240
        || text.chars().any(|c| c.is_control() || "\\/:\"<>|?*".contains(c))
        || text.ends_with([' ', '.']) || path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
        || !path.extension().and_then(OsStr::to_str).is_some_and(|s| s.eq_ignore_ascii_case(ext)) { return Err(INVALID); }
    let stem = path.file_stem().and_then(OsStr::to_str).ok_or(INVALID)?;
    let reserved = stem.split('.').next().unwrap_or("").to_ascii_uppercase();
    if stem.trim().is_empty() || matches!(reserved.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || reserved.strip_prefix("COM").or_else(|| reserved.strip_prefix("LPT")).is_some_and(|s| matches!(s, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")) { return Err(INVALID); }
    Ok(path.with_extension(ext))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u8,
    engine: String,
    engine_version: String,
    source_commit: String,
    source_version: String,
    platform: String,
    executable: String,
    sha256: String,
    files: Vec<RuntimeFile>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeFile { path: String, size: u64, sha256: String }
struct Engine { program: PathBuf, root: PathBuf }
fn real(path: &Path, directory: bool) -> Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path).map_err(|_| UNAVAILABLE)?;
    if metadata.file_type().is_symlink() || if directory { !metadata.is_dir() } else { !metadata.is_file() } { return Err(UNAVAILABLE); }
    #[cfg(windows)] {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 { return Err(UNAVAILABLE); }
    }
    Ok(metadata)
}
fn member_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let relative = Path::new(relative);
    if relative.as_os_str().is_empty() || relative.components().any(|c| !matches!(c, Component::Normal(_))) { return Err(UNAVAILABLE); }
    let mut target = root.to_path_buf();
    let count = relative.components().count();
    for (index, component) in relative.components().enumerate() {
        target.push(component.as_os_str());
        real(&target, index + 1 < count)?;
    }
    Ok(target)
}
fn hash_file(path: &Path, expected_size: u64, expected: &str, deadline: Instant) -> Result<()> {
    if expected.len() != 64 || !expected.bytes().all(|c| c.is_ascii_hexdigit()) || real(path, false)?.len() != expected_size { return Err(UNAVAILABLE); }
    let mut options = OpenOptions::new(); options.read(true);
    #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.custom_flags(libc::O_NOFOLLOW); }
    let mut file = options.open(path).map_err(|_| UNAVAILABLE)?;
    if file.metadata().map_err(|_| UNAVAILABLE)?.len() != expected_size { return Err(UNAVAILABLE); }
    let mut digest = Sha256::new(); let mut count = 0u64; let mut buffer = [0u8; 64 * 1024];
    loop {
        checkpoint(deadline)?;
        let n = file.read(&mut buffer).map_err(|_| UNAVAILABLE)?;
        if n == 0 { break; }
        count = count.checked_add(n as u64).filter(|&n| n <= expected_size).ok_or(UNAVAILABLE)?;
        digest.update(&buffer[..n]);
    }
    if count != expected_size || format!("{:x}", digest.finalize()) != expected.to_ascii_lowercase() { return Err(UNAVAILABLE); }
    Ok(())
}
fn managed(root: &Path, deadline: Instant) -> Result<Engine> {
    real(root, true)?;
    let root = root.canonicalize().map_err(|_| UNAVAILABLE)?;
    let manifest_path = member_path(&root, "runtime.json")?;
    if real(&manifest_path, false)?.len() > 1024 * 1024 { return Err(UNAVAILABLE); }
    let bytes = fs::read(manifest_path).map_err(|_| UNAVAILABLE)?;
    let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|_| UNAVAILABLE)?;
    let expected = if cfg!(windows) { "bin/readmd-office-render.exe" } else { "bin/readmd-office-render" };
    let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "macos-arm64", ("macos", "x86_64") => "macos-x64",
        ("windows", "x86_64") => "windows-x64", ("windows", "aarch64") => "windows-arm64",
        ("linux", "x86_64") => "linux-x64", ("linux", "aarch64") => "linux-arm64", _ => return Err(UNAVAILABLE),
    };
    if manifest.schema != 1 || manifest.engine != "office-layout" || manifest.engine_version != VERSION
        || manifest.source_commit != COMMIT || manifest.source_version != "0.8.5" || manifest.platform != platform
        || manifest.executable != expected || manifest.files.is_empty()
        || manifest.files.len() > MAX_FILES { return Err(UNAVAILABLE); }
    let mut names = BTreeSet::new(); let mut total = 0u64; let mut executable = false;
    names.insert("runtime.json".to_owned());
    for entry in &manifest.files {
        checkpoint(deadline)?;
        if !names.insert(entry.path.clone()) { return Err(UNAVAILABLE); }
        total = total.checked_add(entry.size).filter(|&n| n <= 1024 * 1024 * 1024).ok_or(UNAVAILABLE)?;
        let file = member_path(&root, &entry.path)?;
        hash_file(&file, entry.size, &entry.sha256, deadline)?;
        if entry.path == expected {
            if entry.sha256 != manifest.sha256 { return Err(UNAVAILABLE); }
            executable = true;
        }
    }
    // In particular, an unlisted DLL cannot enter the Windows loader path.
    for (index, item) in walkdir::WalkDir::new(&root).follow_links(false).into_iter().enumerate() {
        if index > MAX_FILES + 128 { return Err(UNAVAILABLE); }
        let item = item.map_err(|_| UNAVAILABLE)?;
        if item.depth() > 24 || item.file_type().is_symlink() { return Err(UNAVAILABLE); }
        if item.file_type().is_file() {
            let relative = item.path().strip_prefix(&root).map_err(|_| UNAVAILABLE)?.to_string_lossy().replace('\\', "/");
            if !names.contains(&relative) { return Err(UNAVAILABLE); }
        }
    }
    if !executable { return Err(UNAVAILABLE); }
    Ok(Engine { program: root.join(expected), root })
}
fn resolve(paths: &crate::paths::AppPaths, deadline: Instant) -> Result<Engine> {
    // Executable provenance belongs to the application distribution. An
    // adjacent profile-owned manifest cannot authorize arbitrary worker code.
    for root in [paths.assets_dir.join("office/runtime")] {
        match fs::symlink_metadata(&root) {
            Ok(_) => return managed(&root, deadline),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err(UNAVAILABLE),
        }
    }
    Err(UNAVAILABLE)
}

fn create(path: &Path, bytes: &[u8]) -> Result<File> {
    let mut file = OpenOptions::new().read(true).write(true).create_new(true).open(path).map_err(|_| FAILED)?;
    crate::task_workspace::track_created(path, &file).map_err(|_| FAILED)?;
    file.write_all(bytes).map_err(|_| FAILED)?;
    Ok(file)
}
fn render_at(name: &Path, bytes: &[u8], engine: &Engine, directory: &Path, deadline: Instant) -> Result<Vec<u8>> {
    let input = directory.join(name);
    let output = input.with_extension("pdf");
    let _source = create(&input, bytes)?;
    let target = create(&output, &[])?;
    let log = create(&directory.join("engine.log"), &[])?;
    for name in ["home", "tmp", "config", "cache", "data"] {
        crate::task_workspace::create_dir(&directory.join(name)).map_err(|_| FAILED)?;
    }
    let mut command = command(engine, directory)?;
    command.arg("--input").arg(&input).arg("--output").arg(&output);
    // Windows' creation-time job owner supplies its own minimal bootstrap.
    #[cfg(not(windows))] {
        command.env_clear();
        for (key, name) in [("HOME", "home"), ("TMPDIR", "tmp"), ("TMP", "tmp"), ("TEMP", "tmp"),
            ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache"), ("XDG_DATA_HOME", "data")] {
            command.env(key, directory.join(name));
        }
        command.env("LANG", "en_US.UTF-8").env("LC_ALL", "en_US.UTF-8");
    }
    command.env("RAYON_NUM_THREADS", "2").env("READMD_OFFICE_OFFLINE", "1");
    #[cfg(unix)] unsafe {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
        command.pre_exec(|| {
            for (resource, value) in [(libc::RLIMIT_CORE, 0), (libc::RLIMIT_CPU, 45),
                (libc::RLIMIT_FSIZE, MAX_PDF), (libc::RLIMIT_NOFILE, 256)] {
                let limit = libc::rlimit { rlim_cur: value as libc::rlim_t, rlim_max: value as libc::rlim_t };
                if libc::setrlimit(resource, &limit) != 0 { return Err(std::io::Error::last_os_error()); }
            }
            #[cfg(target_os = "linux")] {
                let limit = libc::rlimit { rlim_cur: 2 * 1024 * 1024 * 1024, rlim_max: 2 * 1024 * 1024 * 1024 };
                if libc::setrlimit(libc::RLIMIT_AS, &limit) != 0 { return Err(std::io::Error::last_os_error()); }
            }
            Ok(())
        });
    }
    checkpoint(deadline)?;
    let worker = crate::worker_sandbox::spawn_managed(&mut command, directory, &log, &log).map_err(|_| UNAVAILABLE)?;
    let mut child = OwnedWorker { child: worker, stopped: false };
    let mut last_budget = Instant::now();
    let status = loop {
        checkpoint(deadline)?;
        if target.metadata().map_err(|_| FAILED)?.len() > MAX_PDF || log.metadata().map_err(|_| FAILED)?.len() > MAX_LOG { return Err(TOO_LARGE); }
        if last_budget.elapsed() >= Duration::from_millis(150) {
            if memory_exceeded(child.child.id()) { return Err(TOO_LARGE); }
            let mut total = 0u64;
            for (index, item) in walkdir::WalkDir::new(directory).follow_links(false).into_iter().enumerate() {
                let item = item.map_err(|_| FAILED)?;
                if index > 2048 || item.depth() > 24 || item.file_type().is_symlink() { return Err(TOO_LARGE); }
                if item.file_type().is_file() { total = total.saturating_add(item.metadata().map_err(|_| FAILED)?.len()); }
                if total > 256 * 1024 * 1024 { return Err(TOO_LARGE); }
            }
            last_budget = Instant::now();
        }
        match child.child.try_wait().map_err(|_| FAILED)? {
            Some(status) => break status,
            None => std::thread::sleep(Duration::from_millis(15)),
        }
    };
    // Terminate any descendants before checking the completed output handle.
    child.stop();
    if status != 0 { return Err(FAILED); }
    checkpoint(deadline)?;
    if target.metadata().map_err(|_| FAILED)?.len() > MAX_PDF { return Err(TOO_LARGE); }
    let mut result = Vec::new(); target.take(MAX_PDF + 1).read_to_end(&mut result).map_err(|_| FAILED)?;
    if result.len() as u64 > MAX_PDF { return Err(TOO_LARGE); }
    if !result.starts_with(b"%PDF-") { return Err(FAILED); }
    let pdf = lopdf::Document::load_mem(&result).map_err(|_| FAILED)?;
    let pages = pdf.get_pages();
    if pages.is_empty() || pages.len() > 512 { return Err(TOO_LARGE); }
    checkpoint(deadline)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_preserves_only_the_original_safe_filename() {
        assert_eq!(snapshot_name(OsStr::new("论文 draft.DOCX"), "docx").unwrap(), Path::new("论文 draft.docx"));
        for name in ["../document.docx", "/document.docx", "..\\document.docx", "file:document.docx", "CON.docx", "LPT9.docx", "document.docx ", "document.pdf"] {
            assert_eq!(snapshot_name(OsStr::new(name), "docx"), Err(INVALID));
        }
    }
}
struct OwnedWorker { child: crate::worker_sandbox::ManagedWorker, stopped: bool }
impl OwnedWorker {
    fn stop(&mut self) {
        if self.stopped { return; }
        self.stopped = true;
        #[cfg(unix)] unsafe { libc::kill(-(self.child.id() as i32), libc::SIGKILL); }
        let _ = self.child.stop();
    }
}
impl Drop for OwnedWorker { fn drop(&mut self) { self.stop(); } }

fn memory_exceeded(pid: u32) -> bool {
    #[cfg(target_os = "macos")] unsafe {
        let mut info: libc::proc_taskinfo = std::mem::zeroed();
        let size = std::mem::size_of_val(&info) as libc::c_int;
        let read = libc::proc_pidinfo(pid as libc::c_int, libc::PROC_PIDTASKINFO, 0,
            (&mut info as *mut libc::proc_taskinfo).cast(), size);
        // A process which has just exited may already be absent here. Its
        // actual exit status is collected immediately after this budget pass.
        read == size && info.pti_resident_size > 2 * 1024 * 1024 * 1024
    }
    #[cfg(not(target_os = "macos"))] { let _ = pid; false }
}

fn command(engine: &Engine, directory: &Path) -> Result<Command> {
    #[cfg(target_os = "macos")] {
        fn quoted(path: &Path) -> String { format!("\"{}\"", path.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"")) }
        // Headless export needs the system font services. User home/volume
        // trees are denied, with only this runtime and task root carved out.
        // Network denial is independent of the XML resource preflight.
        let profile = format!("(version 1)(allow default)(deny network*)(deny file-read* (subpath \"/Users\") (subpath \"/Volumes\"))(deny file-write*)(allow file-read* (subpath {}) (subpath {}))(allow file-write* (subpath {}))", quoted(&engine.root), quoted(directory), quoted(directory));
        let mut command = crate::silent_command("/usr/bin/sandbox-exec");
        command.arg("-p").arg(profile).arg(&engine.program);
        return Ok(command);
    }
    #[cfg(not(target_os = "macos"))] {
        let _ = directory;
        Ok(crate::silent_command(&engine.program))
    }
}
