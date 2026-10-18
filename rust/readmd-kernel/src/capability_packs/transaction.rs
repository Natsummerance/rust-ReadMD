//! Durable removal journal. Callers hold INSTALL and the capability's
//! exclusive OS lock. No prefix-only cleanup and no recursive deletion.
use super::*;
use serde::{Deserialize, Serialize};
use std::io::Write;

const JOURNAL: &str = "removal-v1.json";
const OWNER: &str = "ReadMD capability removal v1";
const PREFIX: &str = "remove-txn-";
const MAX_TRANSACTIONS: usize = 64;

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Prepared,
    Committed,
    RolledBack,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: u8,
    owner: String,
    nonce: String,
    manifest: String,
    source: String,
    files: Vec<String>,
    phase: Phase,
}

struct Plan {
    parent: PathBuf,
    directory: PathBuf,
    record: Record,
}

pub(super) fn failure(directory: &Path) -> ApiError {
    ApiError::new(422, "capability_install_failed")
        .noted("stage", "recovery")
        .noted(
            "recovery_directory",
            directory.file_name().unwrap_or_default().to_string_lossy(),
        )
}
pub(super) fn fingerprint(lock: &Value) -> ApiResult<String> {
    Ok(hash(&serde_json::to_vec(lock).map_err(|_| error())?))
}
pub(super) fn nonce(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value)
}
fn source_name(name: &str, lock: &Value) -> bool {
    crate::capability_archive::safe_name(name)
        && !name.contains('/')
        && (Some(name) == lock["version"].as_str()
            || name.strip_prefix("replaced-").is_some_and(nonce))
}
pub(super) fn exists(path: &Path) -> ApiResult<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(error()),
    }
}
pub(super) fn regular(path: &Path) -> ApiResult<()> {
    let metadata = fs::symlink_metadata(path).map_err(|_| error())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(error());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(error());
        }
    }
    Ok(())
}
pub(super) fn move_checked(source: &Path, destination: &Path) -> ApiResult<()> {
    if exists(destination)? {
        return Err(error());
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::{
            core::PCWSTR,
            Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH},
        };
        let wide = |path: &Path| -> ApiResult<Vec<u16>> {
            let mut text = path.as_os_str().encode_wide().collect::<Vec<_>>();
            if text.contains(&0) {
                return Err(error());
            }
            text.push(0);
            Ok(text)
        };
        let (source, destination) = (wide(source)?, wide(destination)?);
        // No REPLACE_EXISTING and no cross-volume copy/delete fallback.
        unsafe {
            MoveFileExW(
                PCWSTR(source.as_ptr()),
                PCWSTR(destination.as_ptr()),
                MOVEFILE_WRITE_THROUGH,
            )
        }
        .map_err(|_| error())
    }
    #[cfg(not(windows))]
    {
        // Preserves existing Unix behavior. Atomic no-clobber directory moves
        // and native adversarial races remain platform acceptance gaps.
        fs::rename(source, destination).map_err(|_| error())
    }
}
// A missing ancestor is different from a redirected or non-directory one.
pub(super) fn file_at(root: &Path, name: &str, create: bool) -> ApiResult<Option<PathBuf>> {
    if !crate::capability_archive::safe_name(name) {
        return Err(error());
    }
    if !exists(root)? {
        if !create {
            return Ok(None);
        }
        fs::create_dir(root).map_err(|_| error())?;
    }
    real_directory(root)?;
    let canonical = root.canonicalize().map_err(|_| error())?;
    let parts = name.split('/').collect::<Vec<_>>();
    let mut parent = root.to_path_buf();
    for part in &parts[..parts.len() - 1] {
        parent.push(part);
        if !exists(&parent)? {
            if !create {
                return Ok(None);
            }
            fs::create_dir(&parent).map_err(|_| error())?;
        }
        real_directory(&parent)?;
        if !parent
            .canonicalize()
            .map_err(|_| error())?
            .starts_with(&canonical)
        {
            return Err(error());
        }
    }
    Ok(Some(parent.join(parts.last().ok_or_else(error)?)))
}
fn candidates(parent: &Path) -> ApiResult<Vec<PathBuf>> {
    journal_candidates(parent, PREFIX, JOURNAL)
}
pub(super) fn journal_candidates(
    parent: &Path,
    prefix: &str,
    journal: &str,
) -> ApiResult<Vec<PathBuf>> {
    if !exists(parent)? {
        return Ok(Vec::new());
    }
    real_directory(parent)?;
    let mut result = Vec::new();
    let mut count = 0;
    for entry in fs::read_dir(parent).map_err(|_| error())? {
        count += 1;
        if count > 4096 {
            return Err(error());
        }
        let entry = entry.map_err(|_| error())?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.strip_prefix(prefix).is_some_and(nonce) {
            continue;
        }
        let dir = entry.path();
        // A directory name alone is not ownership. Never follow a reparse.
        if real_directory(&dir).is_err() {
            continue;
        }
        if !exists(&dir.join(journal))? {
            continue;
        }
        result.push(dir);
        if result.len() > MAX_TRANSACTIONS {
            return Err(error());
        }
    }
    result.sort();
    Ok(result)
}
pub(super) fn pending(parent: &Path) -> ApiResult<bool> {
    Ok(!candidates(parent)?.is_empty())
}
pub(super) fn pending_source_trusted(
    parent: &Path,
    source: &str,
    trusted: &history::Trusted<'_>,
) -> ApiResult<bool> {
    let mut found = false;
    for directory in candidates(parent)? {
        let lock = trusted
            .for_record(&directory, JOURNAL)
            .map_err(|_| failure(&directory))?;
        let plan = Plan::read(&directory, lock).map_err(|_| failure(&directory))?;
        found |= plan.record.source == source;
    }
    Ok(found)
}
// Caller holds a shared or exclusive OS guard. A committed cleanup is not a
// half-removed runtime; only PREPARED records require recovery before use.
#[cfg(test)]
pub(super) fn blocking(parent: &Path, lock: &Value) -> ApiResult<bool> {
    blocking_trusted(parent, &history::Trusted::only(lock)?)
}
pub(super) fn blocking_trusted(parent: &Path, trusted: &history::Trusted<'_>) -> ApiResult<bool> {
    let mut blocked = false;
    for directory in candidates(parent)? {
        let lock = trusted
            .for_record(&directory, JOURNAL)
            .map_err(|_| failure(&directory))?;
        let plan = Plan::read(&directory, lock).map_err(|_| failure(&directory))?;
        blocked |= plan.record.phase == Phase::Prepared
            && Some(plan.record.source.as_str()) == trusted.current["version"].as_str();
    }
    Ok(blocked)
}

impl Plan {
    #[cfg(test)]
    fn prepare(source: &Path, lock: &Value) -> ApiResult<Self> {
        Self::prepare_checked(source, lock, &|| Ok(()))
    }
    fn prepare_checked(source: &Path, lock: &Value, check: &dyn Fn() -> ApiResult<()>) -> ApiResult<Self> {
        check()?;
        real_directory(source)?;
        let source = source.canonicalize().map_err(|_| error())?;
        let parent = source.parent().ok_or_else(error)?.to_path_buf();
        real_directory(&parent)?;
        let name = source
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(error)?;
        if !source_name(name, lock) {
            return Err(error());
        }
        let mut files = Vec::new();
        let mut source_paths = paths::Tree::new(&source)?;
        // Validate the complete plan before touching any installed file.
        for file in checked_files(lock)? {
            check()?;
            let name = file["path"].as_str().ok_or_else(error)?;
            if let Some(path) = source_paths.file(name, false)? {
                if exists(&path)? {
                    regular(&path)?;
                    files.push(name.to_string());
                }
            }
        }
        drop(source_paths);
        check()?;
        let id = uuid::Uuid::new_v4().to_string();
        let directory = parent.join(format!("{PREFIX}{id}"));
        fs::create_dir(&directory).map_err(|_| error())?;
        let record = Record {
            schema: 1,
            owner: OWNER.into(),
            nonce: id,
            manifest: fingerprint(lock)?,
            source: name.into(),
            files,
            phase: Phase::Prepared,
        };
        let mut plan = Self {
            parent,
            directory,
            record,
        };
        // The journal directory deliberately has no recursive Drop cleanup.
        // Preparation failures can leave inert metadata, never lost originals.
        // A cancelled preparation can now retire its known empty directories;
        // a crash is recoverable even before the last backup parent is created.
        plan.write(true)?;
        let prepared = (|| {
            let mut backup_paths = paths::Tree::new(&plan.directory)?;
            for file in &plan.record.files {
                check()?;
                backup_paths.file(file, true)?;
            }
            Ok(())
        })();
        if let Err(original) = prepared {
            plan.record.phase = Phase::RolledBack;
            return match plan.write(false).and_then(|_| plan.finish(&|| Ok(()))) {
                Ok(_) => Err(original),
                Err(_) => Err(failure(&plan.directory)),
            };
        }
        Ok(plan)
    }
    fn read(directory: &Path, lock: &Value) -> ApiResult<Self> {
        real_directory(directory)?;
        let directory = directory.canonicalize().map_err(|_| error())?;
        let parent = directory.parent().ok_or_else(error)?.to_path_buf();
        let (file, size) = resource_file(&directory.join(JOURNAL))?;
        if size > crate::capability_archive::MAX_MANIFEST as u64 || size == 0 {
            return Err(error());
        }
        let mut bytes = Vec::new();
        file.take(size + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| error())?;
        if bytes.len() as u64 != size {
            return Err(error());
        }
        let record: Record = serde_json::from_slice(&bytes).map_err(|_| error())?;
        if record.schema != 1
            || record.owner != OWNER
            || !nonce(&record.nonce)
            || directory.file_name().and_then(|n| n.to_str())
                != Some(format!("{PREFIX}{}", record.nonce).as_str())
            || record.manifest != fingerprint(lock)?
            || !source_name(&record.source, lock)
        {
            return Err(error());
        }
        let allowed = checked_files(lock)?
            .iter()
            .map(|f| f["path"].as_str().ok_or_else(error))
            .collect::<ApiResult<BTreeSet<_>>>()?;
        let mut seen = BTreeSet::new();
        if record.files.len() > allowed.len()
            || record
                .files
                .iter()
                .any(|name| !allowed.contains(name.as_str()) || !seen.insert(name.as_str()))
        {
            return Err(error());
        }
        Ok(Self {
            parent,
            directory,
            record,
        })
    }
    fn write(&self, initial: bool) -> ApiResult<()> {
        real_directory(&self.parent)?;
        real_directory(&self.directory)?;
        if self.directory.canonicalize().map_err(|_| error())?.parent()
            != Some(self.parent.as_path())
        {
            return Err(error());
        }
        let target = self.directory.join(JOURNAL);
        if !initial {
            regular(&target)?;
        }
        let bytes = serde_json::to_vec(&self.record).map_err(|_| error())?;
        if bytes.len() > crate::capability_archive::MAX_MANIFEST {
            return Err(error());
        }
        let mut temporary = tempfile::Builder::new()
            .prefix("journal-write-")
            .tempfile_in(&self.directory)
            .map_err(|_| error())?;
        temporary
            .write_all(&bytes)
            .and_then(|_| temporary.as_file().sync_all())
            .map_err(|_| error())?;
        if initial {
            temporary.persist_noclobber(target).map_err(|_| error())?;
        } else {
            temporary.persist(target).map_err(|_| error())?;
        }
        // A directory fsync is available on Unix. Windows atomic replacement
        // and file flushing are used, but abrupt power-loss is not certified.
        #[cfg(unix)]
        if fs::File::open(&self.directory)
            .and_then(|f| f.sync_all())
            .is_err()
        {
            log::warn!(target:"readmd::capability::transaction", "journal directory flush failed after atomic replacement");
        }
        Ok(())
    }
    fn rollback(&mut self, check: &dyn Fn() -> ApiResult<()>) -> ApiResult<()> {
        let source = self.parent.join(&self.record.source);
        let mut backup_paths = paths::Tree::new(&self.directory)?;
        let mut source_paths = paths::Tree::new(&source)?;
        for name in self.record.files.iter().rev() {
            check()?;
            let present = |tree: &mut paths::Tree| -> ApiResult<Option<PathBuf>> {
                match tree.file(name, false)? {
                    Some(path) if exists(&path)? => Ok(Some(path)),
                    _ => Ok(None),
                }
            };
            let backup = present(&mut backup_paths)?;
            let original = present(&mut source_paths)?;
            match (backup, original) {
                (Some(_), Some(_)) | (None, None) => return Err(failure(&self.directory)),
                (Some(backup), None) => {
                    regular(&backup)?;
                    let destination = source_paths.file(name, true)?.ok_or_else(error)?;
                    // Never replace new user data at a restored path.
                    if exists(&destination)? {
                        return Err(failure(&self.directory));
                    }
                    move_checked(&backup, &destination).map_err(|_| failure(&self.directory))?;
                }
                (None, Some(original)) => regular(&original)?,
            }
        }
        drop((backup_paths, source_paths));
        self.record.phase = Phase::RolledBack;
        self.write(false)
    }
    fn finish(&self, check: &dyn Fn() -> ApiResult<()>) -> ApiResult<Removal> {
        self.finish_progress(check, &ignore_progress)
    }
    fn finish_progress(&self, check: &dyn Fn() -> ApiResult<()>, progress: &dyn Fn(crate::capability_archive::Progress)) -> ApiResult<Removal> {
        let source = self.parent.join(&self.record.source);
        if self.record.phase == Phase::Prepared {
            return Err(failure(&self.directory));
        }
        if self.record.phase == Phase::Committed {
            let mut backup_paths = paths::Tree::new(&self.directory)?;
            for (index, name) in self.record.files.iter().enumerate() {
                check()?;
                if let Some(path) = backup_paths.file(name, false)? {
                    if exists(&path)? {
                        regular(&path)?;
                        fs::remove_file(path).map_err(|_| failure(&self.directory))?;
                    }
                }
                progress(removal_progress(self.record.files.len() + index + 1, self.record.files.len()));
            }
            drop(backup_paths);
            prune_checked(&source, &self.record.files, check)?;
        }
        prune_children_checked(&self.directory, &self.record.files, check)?;
        check()?;
        regular(&self.directory.join(JOURNAL))?;
        fs::remove_file(self.directory.join(JOURNAL)).map_err(|_| failure(&self.directory))?;
        match fs::remove_dir(&self.directory) {
            Ok(()) => (),
            Err(_) if self.directory.is_dir() => (), // unknown files retained
            Err(_) => return Err(failure(&self.directory)),
        }
        Ok(Removal {
            unknown: source.exists() || self.directory.exists(),
            cleanup_pending: false,
        })
    }
}
fn prune_children_checked(
    root: &Path,
    files: &[String],
    check: &dyn Fn() -> ApiResult<()>,
) -> ApiResult<()> {
    if !exists(root)? {
        return Ok(());
    }
    real_directory(root)?;
    let mut directories = BTreeSet::new();
    for file in files {
        check()?;
        for (i, _) in file.match_indices('/') {
            directories.insert(file[..i].to_string());
        }
    }
    let mut directories = directories.into_iter().collect::<Vec<_>>();
    directories.sort_by_key(|p| std::cmp::Reverse(p.len()));
    for name in directories {
        check()?;
        if let Some(path) = file_at(root, &format!("{name}/placeholder"), false)? {
            let directory = path.parent().ok_or_else(error)?;
            real_directory(directory)?;
            let _ = fs::remove_dir(directory); // empty only; unknowns survive
        }
    }
    Ok(())
}
pub(super) fn prune(root: &Path, files: &[String]) -> ApiResult<()> {
    prune_checked(root, files, &|| Ok(()))
}
pub(super) fn prune_checked(
    root: &Path,
    files: &[String],
    check: &dyn Fn() -> ApiResult<()>,
) -> ApiResult<()> {
    prune_children_checked(root, files, check)?;
    check()?;
    if exists(root)? {
        real_directory(root)?;
        let _ = fs::remove_dir(root);
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn remove(source: &Path, lock: &Value) -> ApiResult<Removal> {
    remove_checked(source, lock, &|| Ok(()), &ignore_progress, &|| Ok(()))
}
fn removal_progress(done: usize, files: usize) -> crate::capability_archive::Progress {
    crate::capability_archive::Progress { files_done: done, files_total: files * 2, expanded_bytes: 0, expanded_total: 0 }
}
pub(super) fn remove_checked(
    source: &Path, lock: &Value, check: &dyn Fn() -> ApiResult<()>,
    progress: &dyn Fn(crate::capability_archive::Progress), committing: &dyn Fn() -> ApiResult<()>,
) -> ApiResult<Removal> {
    let mut plan = Plan::prepare_checked(source, lock, check)?;
    progress(removal_progress(0, plan.record.files.len()));
    let source = plan.parent.join(&plan.record.source);
    let result = (|| {
        let mut source_paths = paths::Tree::new(&source)?;
        let mut backup_paths = paths::Tree::new(&plan.directory)?;
        for (index, name) in plan.record.files.iter().enumerate() {
            check()?;
            let original = source_paths.file(name, false)?.ok_or_else(error)?;
            regular(&original)?;
            let backup = backup_paths.file(name, false)?.ok_or_else(error)?;
            if exists(&backup)? {
                return Err(error());
            }
            move_checked(&original, &backup)?;
            progress(removal_progress(index + 1, plan.record.files.len()));
            if index == 0 {
                log::debug!(target:"readmd::capability::transaction", "removal first file moved");
            }
        }
        drop((source_paths, backup_paths));
        check()?;
        committing()?;
        plan.record.phase = Phase::Committed;
        plan.write(false)?;
        log::debug!(target:"readmd::capability::transaction", "removal committed");
        Ok(())
    })();
    if let Err(original) = result {
        // A failed commit marker is still PREPARED on disk; always roll back.
        plan.record.phase = Phase::Prepared;
        return match plan
            .rollback(&|| Ok(()))
            .and_then(|_| plan.finish(&|| Ok(())))
        {
            Ok(_) => Err(original),
            Err(_) => Err(failure(&plan.directory)),
        };
    }
    // Once committed, a cleanup error does not turn success into a rollback.
    Ok(plan.finish_progress(&|| Ok(()), progress).unwrap_or(Removal {
        unknown: true,
        cleanup_pending: true,
    }))
}

#[cfg(test)]
pub(super) fn recover(
    parent: &Path,
    lock: &Value,
    check: &dyn Fn() -> ApiResult<()>,
) -> ApiResult<()> {
    recover_trusted(parent, &history::Trusted::only(lock)?, check)
}
pub(super) fn recover_trusted(
    parent: &Path,
    trusted: &history::Trusted<'_>,
    check: &dyn Fn() -> ApiResult<()>,
) -> ApiResult<()> {
    for directory in candidates(parent)? {
        check()?;
        let lock = trusted
            .for_record(&directory, JOURNAL)
            .map_err(|_| failure(&directory))?;
        let mut plan = Plan::read(&directory, lock).map_err(|_| failure(&directory))?;
        let interrupted = std::cell::Cell::new(false);
        let checked = || {
            let result = check();
            if result.is_err() {
                interrupted.set(true);
            }
            result
        };
        if plan.record.phase == Phase::Prepared {
            if let Err(error) = plan.rollback(&checked) {
                if interrupted.get()
                    || Some(plan.record.source.as_str()) == trusted.current["version"].as_str()
                {
                    return Err(error);
                }
                // An old replaced tree cannot change the current runtime.
                // Retain its conflicting originals without blocking readers.
                check()?;
                log::warn!(target:"readmd::capability::transaction", "previous runtime removal recovery pending");
                continue;
            }
        }
        if let Err(error) = plan.finish(&checked) {
            // Cancellation/deadline errors are never downgraded to cleanup.
            if interrupted.get() {
                return Err(error);
            }
            check()?;
            log::warn!(target:"readmd::capability::transaction", "committed removal cleanup pending");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(parent: &Path) -> (PathBuf, Value) {
        let source = parent.join(crate::publication_fonts::VERSION);
        let mut files = Vec::new();
        for name in ["bin/engine", "data/model", "NOTICE"] {
            let bytes = format!("original {name}").into_bytes();
            let path = source.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, &bytes).unwrap();
            files.push(json!({"path":name,"size":bytes.len(),"sha256":hash(&bytes)}));
        }
        (
            source,
            json!({"schema":1,"id":"fonts","version":crate::publication_fonts::VERSION,"platform":"universal","files":files}),
        )
    }
    fn move_first(plan: &Plan) {
        let name = &plan.record.files[0];
        fs::rename(
            plan.parent.join(&plan.record.source).join(name),
            plan.directory.join(name),
        )
        .unwrap();
    }
    #[test]
    fn cancelled_removal_preparation_and_movement_restore_the_installed_runtime() {
        for during_preparation in [true, false] {
            let root = tempfile::tempdir().unwrap();
            let (source, lock) = fixture(root.path());
            fs::write(source.join("user-note"), b"preserve user note").unwrap();
            let cancelled = std::cell::Cell::new(false);
            let error = remove_checked(&source, &lock, &|| {
                if cancelled.get() || (during_preparation && pending(root.path())?) {
                    Err(ApiError::new(422, "cancelled"))
                } else { Ok(()) }
            }, &|p| {
                if !during_preparation && p.files_done > 0 { cancelled.set(true); }
            }, &|| panic!("Cancellation must precede commit")).err().unwrap();
            assert_eq!(error.code, "cancelled");
            validate(&source, &lock).unwrap();
            assert_eq!(fs::read(source.join("user-note")).unwrap(), b"preserve user note");
            assert!(!pending(root.path()).unwrap());
        }
    }

    #[test]
    fn refused_commit_rolls_back_and_success_progress_finishes_with_unknowns_preserved() {
        let root = tempfile::tempdir().unwrap();
        let (source, lock) = fixture(root.path());
        let error = remove_checked(&source, &lock, &|| Ok(()), &ignore_progress,
            &|| Err(ApiError::new(422, "cancelled"))).err().unwrap();
        assert_eq!(error.code, "cancelled");
        validate(&source, &lock).unwrap();
        assert!(!pending(root.path()).unwrap());
        fs::write(source.join("user-note"), b"preserve user note").unwrap();
        let last = std::cell::Cell::new(0);
        let total = std::cell::Cell::new(0);
        let result = remove_checked(&source, &lock, &|| Ok(()), &|p| {
            assert!(p.files_done >= last.get());
            assert!(p.files_done <= p.files_total);
            last.set(p.files_done); total.set(p.files_total);
        }, &|| Ok(())).unwrap();
        assert_eq!(last.get(), total.get());
        assert!(last.get() > 0 && result.unknown && !result.cleanup_pending);
        assert_eq!(fs::read(source.join("user-note")).unwrap(), b"preserve user note");
    }
    #[test]
    fn interrupted_removal_restores_originals_and_is_idempotent() {
        let root = tempfile::tempdir().unwrap();
        let (source, lock) = fixture(root.path());
        let plan = Plan::prepare(&source, &lock).unwrap();
        move_first(&plan);
        drop(plan);
        recover(root.path(), &lock, &|| Ok(())).unwrap();
        validate(&source, &lock).unwrap();
        recover(root.path(), &lock, &|| Ok(())).unwrap();
        assert!(!pending(root.path()).unwrap());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }
    #[test]
    fn committed_removal_resumes_without_restoring_and_preserves_extras() {
        let root = tempfile::tempdir().unwrap();
        let (source, lock) = fixture(root.path());
        let mut plan = Plan::prepare(&source, &lock).unwrap();
        for name in &plan.record.files {
            fs::rename(source.join(name), plan.directory.join(name)).unwrap();
        }
        fs::write(source.join("user-note"), b"keep original extra").unwrap();
        fs::write(plan.directory.join("recovery-note"), b"keep recovery extra").unwrap();
        plan.record.phase = Phase::Committed;
        plan.write(false).unwrap();
        let directory = plan.directory.clone();
        drop(plan);
        recover(root.path(), &lock, &|| Ok(())).unwrap();
        assert_eq!(
            fs::read(source.join("user-note")).unwrap(),
            b"keep original extra"
        );
        assert_eq!(
            fs::read(directory.join("recovery-note")).unwrap(),
            b"keep recovery extra"
        );
        assert!(!source.join("bin/engine").exists());
        assert!(!pending(root.path()).unwrap());
    }
    #[test]
    fn rollback_conflict_preserves_both_and_cancel_preserves_journal() {
        let root = tempfile::tempdir().unwrap();
        let (source, lock) = fixture(root.path());
        let plan = Plan::prepare(&source, &lock).unwrap();
        move_first(&plan);
        fs::write(source.join(&plan.record.files[0]), b"new user bytes").unwrap();
        assert!(recover(root.path(), &lock, &|| Ok(())).is_err());
        assert_eq!(
            fs::read(source.join(&plan.record.files[0])).unwrap(),
            b"new user bytes"
        );
        assert_eq!(
            fs::read(plan.directory.join(&plan.record.files[0])).unwrap(),
            b"original bin/engine"
        );
        let error =
            recover(root.path(), &lock, &|| Err(ApiError::new(422, "cancelled"))).unwrap_err();
        assert_eq!(error.code, "cancelled");
        assert!(plan.directory.join(JOURNAL).is_file());
    }
    #[test]
    fn foreign_manifest_and_paths_fail_closed_unmarked_legacy_is_preserved() {
        let root = tempfile::tempdir().unwrap();
        let (source, lock) = fixture(root.path());
        let legacy = root.path().join(format!("remove-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&legacy).unwrap();
        fs::write(legacy.join("original"), b"legacy recovery").unwrap();
        recover(root.path(), &lock, &|| Ok(())).unwrap();
        assert_eq!(
            fs::read(legacy.join("original")).unwrap(),
            b"legacy recovery"
        );
        let mut plan = Plan::prepare(&source, &lock).unwrap();
        move_first(&plan);
        plan.record.files[0] = "../outside".into();
        plan.write(false).unwrap();
        assert!(recover(root.path(), &lock, &|| Ok(())).is_err());
        assert_eq!(
            fs::read(plan.directory.join("bin/engine")).unwrap(),
            b"original bin/engine"
        );
    }

    #[test]
    fn first_resource_lease_recovers_removal_and_pending_preview_is_invalid() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("profile");
        let assets = root.path().join("assets");
        let parent = data.join("capabilities/managed-v2/fonts");
        fs::create_dir_all(&parent).unwrap();
        fs::create_dir_all(assets.join("capabilities")).unwrap();
        let (source, lock) = fixture(&parent);
        fs::write(
            assets.join(format!(
                "capabilities/fonts-{}-universal.lock.json",
                crate::publication_fonts::VERSION
            )),
            serde_json::to_vec(&lock).unwrap(),
        )
        .unwrap();
        let plan = Plan::prepare(&source, &lock).unwrap();
        move_first(&plan);
        drop(plan);
        assert_eq!(
            runtime_revision(&data, &assets, "fonts").unwrap_err().code,
            "capability_install_busy"
        );
        let error = lease_checked(&data, &assets, "fonts", &|| {
            Err(ApiError::new(422, "cancelled"))
        })
        .err()
        .unwrap();
        assert_eq!(error.code, "cancelled");
        assert!(pending(&parent).unwrap());
        let lease = lease(&data, &assets, "fonts").unwrap();
        validate(&lease.directory, &lock).unwrap();
        assert!(!pending(&parent).unwrap());
        assert!(runtime_revision(&data, &assets, "fonts").is_ok());
    }

    #[test]
    fn committed_cleanup_failure_does_not_block_repaired_runtime_or_other_readers() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("profile");
        let assets = root.path().join("assets");
        let parent = data.join("capabilities/managed-v2/fonts");
        fs::create_dir_all(&parent).unwrap();
        fs::create_dir_all(assets.join("capabilities")).unwrap();
        let (source, lock) = fixture(&parent);
        fs::write(
            assets.join(format!(
                "capabilities/fonts-{}-universal.lock.json",
                crate::publication_fonts::VERSION
            )),
            serde_json::to_vec(&lock).unwrap(),
        )
        .unwrap();
        let replaced = parent.join(format!("replaced-{}", uuid::Uuid::new_v4()));
        fs::rename(&source, &replaced).unwrap();
        let mut plan = Plan::prepare(&replaced, &lock).unwrap();
        for name in &plan.record.files {
            fs::rename(replaced.join(name), plan.directory.join(name)).unwrap();
        }
        plan.record.phase = Phase::Committed;
        plan.write(false).unwrap();
        // User data at the old source path makes pruning fail. It must not be
        // removed, and that harmless cleanup must not invalidate a new runtime.
        prune(&replaced, &plan.record.files).unwrap();
        fs::write(&replaced, b"user data at old backup name").unwrap();
        fixture(&parent);
        recover(&parent, &lock, &|| Ok(())).unwrap();
        assert!(pending(&parent).unwrap());
        assert!(!blocking(&parent, &lock).unwrap());
        assert_eq!(
            fs::read(&replaced).unwrap(),
            b"user data at old backup name"
        );
        assert!(runtime_revision(&data, &assets, "fonts").is_ok());
        let first = lease(&data, &assets, "fonts").unwrap();
        // The second reader cannot upgrade the OS lock. A committed cleanup
        // must still allow it through without waiting for the first reader.
        let second = lease_checked(&data, &assets, "fonts", &|| Ok(())).unwrap();
        validate(&first.directory, &lock).unwrap();
        validate(&second.directory, &lock).unwrap();
        assert!(pending(&parent).unwrap());
    }

    #[test]
    fn committed_cleanup_preserves_one_shot_deadline_error() {
        let root = tempfile::tempdir().unwrap();
        let (source, lock) = fixture(root.path());
        let mut plan = Plan::prepare(&source, &lock).unwrap();
        for name in &plan.record.files {
            fs::rename(source.join(name), plan.directory.join(name)).unwrap();
        }
        plan.record.phase = Phase::Committed;
        plan.write(false).unwrap();
        let calls = std::cell::Cell::new(0);
        let error = recover(root.path(), &lock, &|| {
            calls.set(calls.get() + 1);
            if calls.get() == 2 {
                Err(ApiError::new(422, "worker_deadline"))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert_eq!(error.code, "worker_deadline");
        assert_eq!(calls.get(), 2);
        assert!(plan.directory.join(&plan.record.files[0]).is_file());
        recover(root.path(), &lock, &|| Ok(())).unwrap();
        assert!(!pending(root.path()).unwrap());
    }

    #[test]
    fn conflicting_previous_removal_retains_both_without_invalidating_current_runtime() {
        let root = tempfile::tempdir().unwrap();
        let (source, lock) = fixture(root.path());
        let previous = root
            .path()
            .join(format!("replaced-{}", uuid::Uuid::new_v4()));
        move_checked(&source, &previous).unwrap();
        let plan = Plan::prepare(&previous, &lock).unwrap();
        move_first(&plan);
        fs::write(
            previous.join(&plan.record.files[0]),
            b"new data in old directory",
        )
        .unwrap();
        fixture(root.path());
        recover(root.path(), &lock, &|| Ok(())).unwrap();
        assert!(pending(root.path()).unwrap());
        assert!(!blocking(root.path(), &lock).unwrap());
        validate(&source, &lock).unwrap();
        assert_eq!(
            fs::read(previous.join(&plan.record.files[0])).unwrap(),
            b"new data in old directory"
        );
        assert_eq!(
            fs::read(plan.directory.join(&plan.record.files[0])).unwrap(),
            b"original bin/engine"
        );
        assert_eq!(
            recover(root.path(), &lock, &|| Err(ApiError::new(422, "cancelled")))
                .unwrap_err()
                .code,
            "cancelled"
        );
    }

    #[test]
    fn replacement_recovery_never_redeletes_a_conflicting_child_removal() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("profile");
        let parent = data.join("capabilities/managed-v2/fonts");
        fs::create_dir_all(&parent).unwrap();
        let (source, lock) = fixture(&parent);
        let id = uuid::Uuid::new_v4().to_string();
        let previous_name = format!("replaced-{id}");
        let previous = parent.join(&previous_name);
        move_checked(&source, &previous).unwrap();
        let child = Plan::prepare(&previous, &lock).unwrap();
        move_first(&child);
        fs::write(
            previous.join(&child.record.files[0]),
            b"user conflict must survive",
        )
        .unwrap();
        fixture(&parent);
        // Model a process dying after a child cleanup failed but before its
        // committed replacement journal could be retired.
        let installation = parent.join(format!("install-txn-{id}"));
        fs::create_dir(&installation).unwrap();
        fs::write(installation.join("replacement-v1.json"), serde_json::to_vec(&json!({
            "schema":1,"owner":"ReadMD capability replacement v1","nonce":id,
            "manifest":fingerprint(&lock).unwrap(),"target":crate::publication_fonts::VERSION,
            "staging":"install-abcdef","previous":previous_name,"had_previous":true,"phase":"committed"
        })).unwrap()).unwrap();
        for _ in 0..2 {
            recover_locked(
                &data,
                &data.join("missing-assets"),
                spec("fonts").unwrap(),
                &lock,
                &|| Ok(()),
            )
            .unwrap();
            assert_eq!(
                fs::read(previous.join(&child.record.files[0])).unwrap(),
                b"user conflict must survive"
            );
            assert_eq!(
                fs::read(child.directory.join(&child.record.files[0])).unwrap(),
                b"original bin/engine"
            );
            validate(&source, &lock).unwrap();
            assert!(!transactions_blocking(
                &parent,
                &data.join("missing-assets"),
                spec("fonts").unwrap(),
                &lock
            )
            .unwrap());
        }
        assert!(pending(&parent).unwrap());
        assert!(!replacement::pending(&parent).unwrap());
    }

    #[cfg(windows)]
    #[test]
    fn moves_refuse_existing_file_and_directory_destinations() {
        let root = tempfile::tempdir().unwrap();
        let (from, to) = (root.path().join("from"), root.path().join("to"));
        fs::write(&from, b"source bytes").unwrap();
        fs::write(&to, b"destination bytes").unwrap();
        assert!(move_checked(&from, &to).is_err());
        assert_eq!(fs::read(&from).unwrap(), b"source bytes");
        assert_eq!(fs::read(&to).unwrap(), b"destination bytes");
        let (from, to) = (root.path().join("from-dir"), root.path().join("to-dir"));
        fs::create_dir(&from).unwrap();
        fs::create_dir(&to).unwrap();
        assert!(move_checked(&from, &to).is_err());
        assert!(from.is_dir() && to.is_dir());
    }

    #[cfg(windows)]
    #[test]
    fn pinned_committed_backup_defers_cleanup_then_retries() {
        let root = tempfile::tempdir().unwrap();
        let (source, lock) = fixture(root.path());
        let mut plan = Plan::prepare(&source, &lock).unwrap();
        for name in &plan.record.files {
            fs::rename(source.join(name), plan.directory.join(name)).unwrap();
        }
        plan.record.phase = Phase::Committed;
        plan.write(false).unwrap();
        let pinned = resource_file(&plan.directory.join(&plan.record.files[0])).unwrap();
        recover(root.path(), &lock, &|| Ok(())).unwrap();
        assert!(pending(root.path()).unwrap());
        assert!(!blocking(root.path(), &lock).unwrap());
        assert!(plan.directory.join(&plan.record.files[0]).is_file());
        drop(pinned);
        recover(root.path(), &lock, &|| Ok(())).unwrap();
        assert!(!pending(root.path()).unwrap());
    }

    #[test]
    fn real_owner_crash_recovers_prepared_and_committed_removal() {
        use std::{
            process::{Child, Command, Stdio},
            sync::OnceLock,
            time::{Duration, Instant},
        };
        static CHILD: OnceLock<(PathBuf, String)> = OnceLock::new();
        struct Barrier;
        impl log::Log for Barrier {
            fn enabled(&self, metadata: &log::Metadata) -> bool {
                metadata.target() == "readmd::capability::transaction"
            }
            fn log(&self, record: &log::Record) {
                if !self.enabled(record.metadata()) {
                    return;
                }
                let (root, stage) = CHILD.get().unwrap();
                let expected = if stage == "prepared" {
                    "removal first file moved"
                } else {
                    "removal committed"
                };
                if record.args().to_string() != expected {
                    return;
                }
                fs::write(root.join("child.ready"), expected).unwrap();
                let until = Instant::now() + Duration::from_secs(30);
                while !root.join("child.stop").is_file() {
                    assert!(
                        Instant::now() < until,
                        "Parent did not stop transaction child"
                    );
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            fn flush(&self) {}
        }
        static BARRIER: Barrier = Barrier;
        if let Some(root) = std::env::var_os("READMD_REMOVAL_CHILD_ROOT") {
            let root = PathBuf::from(root);
            let stage = std::env::var("READMD_REMOVAL_CHILD_STAGE").unwrap();
            assert!(matches!(stage.as_str(), "prepared" | "committed"));
            CHILD.set((root.clone(), stage)).unwrap();
            log::set_logger(&BARRIER).unwrap();
            log::set_max_level(log::LevelFilter::Debug);
            let lock: Value =
                serde_json::from_slice(&fs::read(root.join("lock.json")).unwrap()).unwrap();
            let _local = resource_guard();
            let _process = crate::capability_process::try_acquire(
                &root,
                "fonts",
                crate::capability_process::Mode::Exclusive,
            )
            .unwrap()
            .unwrap();
            remove(&root.join(crate::publication_fonts::VERSION), &lock).unwrap();
            return;
        }
        struct OwnedChild(Child);
        impl Drop for OwnedChild {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        for stage in ["prepared", "committed"] {
            let root = tempfile::tempdir().unwrap();
            let (source, lock) = fixture(root.path());
            fs::write(
                root.path().join("lock.json"),
                serde_json::to_vec(&lock).unwrap(),
            )
            .unwrap();
            fs::write(source.join("user-note"), b"keep user file").unwrap();
            let mut child=OwnedChild(Command::new(std::env::current_exe().unwrap())
                .args(["--exact","capability_packs::transaction::tests::real_owner_crash_recovers_prepared_and_committed_removal","--test-threads=1"])
                .env("READMD_REMOVAL_CHILD_ROOT",root.path()).env("READMD_REMOVAL_CHILD_STAGE",stage)
                .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::inherit()).spawn().unwrap());
            let until = Instant::now() + Duration::from_secs(30);
            while !root.path().join("child.ready").is_file() {
                assert!(
                    child.0.try_wait().unwrap().is_none(),
                    "Transaction child exited before barrier"
                );
                assert!(
                    Instant::now() < until,
                    "Transaction child did not reach durable phase"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(crate::capability_process::try_acquire(
                root.path(),
                "fonts",
                crate::capability_process::Mode::Exclusive
            )
            .unwrap()
            .is_none());
            child.0.kill().unwrap();
            assert!(!child.0.wait().unwrap().success());
            drop(child);
            let _local = resource_guard();
            let _process = crate::capability_process::try_acquire(
                root.path(),
                "fonts",
                crate::capability_process::Mode::Exclusive,
            )
            .unwrap()
            .unwrap();
            recover(root.path(), &lock, &|| Ok(())).unwrap();
            assert_eq!(
                fs::read(source.join("user-note")).unwrap(),
                b"keep user file"
            );
            if stage == "prepared" {
                validate(&source, &lock).unwrap();
            } else {
                for file in checked_files(&lock).unwrap() {
                    assert!(!source.join(file["path"].as_str().unwrap()).exists());
                }
            }
            assert!(!pending(root.path()).unwrap());
        }
    }
}
