//! Installation commit journal. Caller holds INSTALL and the exclusive OS
//! capability guard. Staging is already verified and probed before entering.
//! A crash before the new tree is in place restores the previous tree; after
//! the new tree is in place recovery verifies it before committing forward.
use super::transaction::{
    exists, failure, file_at, fingerprint, journal_candidates, move_checked, nonce, prune, regular,
};
use super::*;
use serde::{Deserialize, Serialize};
use std::io::Write;

const PREFIX: &str = "install-txn-";
const JOURNAL: &str = "replacement-v1.json";
const OWNER: &str = "ReadMD capability replacement v1";

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
    target: String,
    staging: String,
    previous: String,
    had_previous: bool,
    phase: Phase,
}
struct Plan {
    parent: PathBuf,
    directory: PathBuf,
    record: Record,
}
pub(super) struct Commit {
    pub repaired: bool,
    pub unknown: bool,
    pub cleanup_pending: bool,
}

fn staging_name(name: &str) -> bool {
    let suffix = name
        .strip_prefix("stream-install-")
        .or_else(|| name.strip_prefix("install-"));
    suffix.is_some_and(|s| {
        (6..=64).contains(&s.len()) && s.bytes().all(|c| c.is_ascii_alphanumeric())
    })
}
impl Plan {
    fn prepare(staging: &Path, target: &Path, lock: &Value) -> ApiResult<Self> {
        real_directory(staging)?;
        let staging = staging.canonicalize().map_err(|_| error())?;
        let parent = staging.parent().ok_or_else(error)?.to_path_buf();
        real_directory(&parent)?;
        let target_name = lock["version"].as_str().ok_or_else(error)?;
        if !crate::capability_archive::safe_name(target_name)
            || target_name.contains('/')
            || target
                .parent()
                .and_then(|p| p.canonicalize().ok())
                .as_deref()
                != Some(parent.as_path())
            || target.file_name().and_then(|p| p.to_str()) != Some(target_name)
        {
            return Err(error());
        }
        let stage_name = staging
            .file_name()
            .and_then(|p| p.to_str())
            .ok_or_else(error)?;
        if !staging_name(stage_name) {
            return Err(error());
        }
        let had_previous = exists(target)?;
        if had_previous {
            real_directory(target)?;
        }
        let id = uuid::Uuid::new_v4().to_string();
        let directory = parent.join(format!("{PREFIX}{id}"));
        let previous = format!("replaced-{id}");
        if exists(&parent.join(&previous))? {
            return Err(error());
        }
        fs::create_dir(&directory).map_err(|_| error())?;
        let plan = Self {
            parent,
            directory,
            record: Record {
                schema: 1,
                owner: OWNER.into(),
                nonce: id,
                manifest: fingerprint(lock)?,
                target: target_name.into(),
                staging: stage_name.into(),
                previous,
                had_previous,
                phase: Phase::Prepared,
            },
        };
        plan.write(true)?;
        Ok(plan)
    }
    fn read(directory: &Path, lock: &Value) -> ApiResult<Self> {
        real_directory(directory)?;
        let directory = directory.canonicalize().map_err(|_| error())?;
        let parent = directory.parent().ok_or_else(error)?.to_path_buf();
        let (file, size) = resource_file(&directory.join(JOURNAL))?;
        if size == 0 || size > 16384 {
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
            || directory.file_name().and_then(|p| p.to_str())
                != Some(format!("{PREFIX}{}", record.nonce).as_str())
            || record.manifest != fingerprint(lock)?
            || Some(record.target.as_str()) != lock["version"].as_str()
            || !crate::capability_archive::safe_name(&record.target)
            || record.target.contains('/')
            || !staging_name(&record.staging)
            || record.previous != format!("replaced-{}", record.nonce)
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
        let mut temp = tempfile::Builder::new()
            .prefix("journal-write-")
            .tempfile_in(&self.directory)
            .map_err(|_| error())?;
        temp.write_all(&bytes)
            .and_then(|_| temp.as_file().sync_all())
            .map_err(|_| error())?;
        if initial {
            temp.persist_noclobber(target).map_err(|_| error())?;
        } else {
            temp.persist(target).map_err(|_| error())?;
        }
        #[cfg(unix)]
        if fs::File::open(&self.directory)
            .and_then(|f| f.sync_all())
            .is_err()
        {
            log::warn!(target:"readmd::capability::transaction", "replacement directory flush failed after atomic replacement");
        }
        Ok(())
    }
    fn resolve(&mut self, lock: &Value, check: &dyn Fn() -> ApiResult<()>) -> ApiResult<()> {
        if self.record.phase != Phase::Prepared {
            return Ok(());
        }
        check()?;
        let target = self.parent.join(&self.record.target);
        let staging = self.parent.join(&self.record.staging);
        let previous = self.parent.join(&self.record.previous);
        let (has_target, has_stage, has_previous) =
            (exists(&target)?, exists(&staging)?, exists(&previous)?);
        if has_stage {
            real_directory(&staging)?;
            if self.record.had_previous {
                match (has_target, has_previous) {
                    (true, false) => real_directory(&target)?, // rename not started, or already restored
                    (false, true) => {
                        real_directory(&previous)?;
                        // No clobber: user-created destinations are never overwritten.
                        if exists(&target)? {
                            return Err(failure(&self.directory));
                        }
                        move_checked(&previous, &target).map_err(|_| failure(&self.directory))?;
                    }
                    _ => return Err(failure(&self.directory)),
                }
            } else if has_target || has_previous {
                return Err(failure(&self.directory));
            }
            self.record.phase = Phase::RolledBack;
        } else {
            if !has_target || has_previous != self.record.had_previous {
                return Err(failure(&self.directory));
            }
            // A missing stage alone is not proof of a completed install.
            real_directory(&target)?;
            let canonical = target
                .canonicalize()
                .map_err(|_| failure(&self.directory))?;
            validate_partition(&target, &canonical, checked_files(lock)?, 0, 1, check)?;
            self.record.phase = Phase::Committed;
        }
        self.write(false)
    }
    fn finish(&self, lock: &Value, check: &dyn Fn() -> ApiResult<()>) -> ApiResult<Removal> {
        self.finish_trusted(lock, &history::Trusted::only(lock)?, check)
    }
    fn finish_trusted(
        &self,
        lock: &Value,
        trusted: &history::Trusted<'_>,
        check: &dyn Fn() -> ApiResult<()>,
    ) -> ApiResult<Removal> {
        check()?;
        let mut retained = false;
        let mut cleanup_pending = false;
        match self.record.phase {
            Phase::Prepared => return Err(failure(&self.directory)),
            Phase::Committed => {
                let previous = self.parent.join(&self.record.previous);
                if transaction::pending_source_trusted(
                    &self.parent,
                    &self.record.previous,
                    trusted,
                )? {
                    // The child journal owns remaining originals, including
                    // any rollback conflict. Never start another removal over
                    // the same paths and erase data the child preserved.
                    retained = true;
                    cleanup_pending = true;
                } else if exists(&previous)? {
                    real_directory(&previous)?;
                    let result = transaction::remove_checked(&previous, lock, check, &ignore_progress, check)?;
                    retained = result.unknown;
                    cleanup_pending = result.cleanup_pending;
                }
            }
            Phase::RolledBack => {
                let staging = self.parent.join(&self.record.staging);
                if exists(&staging)? {
                    real_directory(&staging)?;
                    let files = checked_files(lock)?
                        .iter()
                        .map(|f| f["path"].as_str().ok_or_else(error).map(str::to_string))
                        .collect::<ApiResult<Vec<_>>>()?;
                    for name in &files {
                        check()?;
                        if let Some(path) = file_at(&staging, name, false)? {
                            if exists(&path)? {
                                regular(&path)?;
                                fs::remove_file(&path).map_err(|_| failure(&self.directory))?;
                            }
                        }
                    }
                    prune(&staging, &files)?;
                    retained = exists(&staging)?;
                }
            }
        }
        regular(&self.directory.join(JOURNAL))?;
        fs::remove_file(self.directory.join(JOURNAL)).map_err(|_| failure(&self.directory))?;
        let _ = fs::remove_dir(&self.directory); // empty only; preserve unknown additions
        Ok(Removal {
            unknown: retained || exists(&self.directory)?,
            cleanup_pending,
        })
    }
}

pub(super) fn pending(parent: &Path) -> ApiResult<bool> {
    Ok(!journal_candidates(parent, PREFIX, JOURNAL)?.is_empty())
}
// Validate every replacement record before allowing a staging reaper to act.
// Even a committed/rolled-back record can still own a recovery dependency.
pub(super) fn references_trusted(
    parent: &Path,
    staging: &str,
    trusted: &history::Trusted<'_>,
) -> ApiResult<bool> {
    let mut referenced = false;
    for directory in journal_candidates(parent, PREFIX, JOURNAL)? {
        let lock = trusted
            .for_record(&directory, JOURNAL)
            .map_err(|_| failure(&directory))?;
        let plan = Plan::read(&directory, lock).map_err(|_| failure(&directory))?;
        referenced |= plan.record.staging == staging;
    }
    Ok(referenced)
}
#[cfg(test)]
pub(super) fn blocking(parent: &Path, lock: &Value) -> ApiResult<bool> {
    blocking_trusted(parent, &history::Trusted::only(lock)?)
}
pub(super) fn blocking_trusted(parent: &Path, trusted: &history::Trusted<'_>) -> ApiResult<bool> {
    let mut blocked = false;
    for directory in journal_candidates(parent, PREFIX, JOURNAL)? {
        let lock = trusted
            .for_record(&directory, JOURNAL)
            .map_err(|_| failure(&directory))?;
        let plan = Plan::read(&directory, lock).map_err(|_| failure(&directory))?;
        blocked |= plan.record.phase == Phase::Prepared
            && Some(plan.record.target.as_str()) == trusted.current["version"].as_str();
    }
    Ok(blocked)
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
    for directory in journal_candidates(parent, PREFIX, JOURNAL)? {
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
        if let Err(error) = plan.resolve(lock, &checked) {
            if interrupted.get()
                || Some(plan.record.target.as_str()) == trusted.current["version"].as_str()
            {
                return Err(error);
            }
            check()?;
            log::warn!(target:"readmd::capability::transaction", "previous version replacement recovery retained");
            continue;
        }
        if let Err(error) = plan.finish_trusted(lock, trusted, &checked) {
            if interrupted.get() {
                return Err(error);
            }
            check()?;
            log::warn!(target:"readmd::capability::transaction", "replacement cleanup pending");
        }
    }
    Ok(())
}

pub(super) fn commit(staging: &Path, target: &Path, lock: &Value) -> ApiResult<Commit> {
    // The durable staging owner outlives this call. It checks all replacement
    // references before cleanup, including failures during journal preparation.
    let mut plan = Plan::prepare(staging, target, lock)?;
    log::debug!(target:"readmd::capability::transaction", "replacement prepared");
    let target = plan.parent.join(&plan.record.target);
    let previous = plan.parent.join(&plan.record.previous);
    let result = (|| {
        if plan.record.had_previous {
            real_directory(&target)?;
            if exists(&previous)? {
                return Err(error());
            }
            move_checked(&target, &previous)?;
            log::debug!(target:"readmd::capability::transaction", "replacement previous moved");
        }
        if exists(&target)? {
            return Err(error());
        }
        real_directory(&staging)?;
        move_checked(&staging, &target)?;
        log::debug!(target:"readmd::capability::transaction", "replacement candidate moved");
        plan.record.phase = Phase::Committed;
        plan.write(false)?;
        log::debug!(target:"readmd::capability::transaction", "replacement committed");
        Ok(())
    })();
    if let Err(original) = result {
        plan.record.phase = Phase::Prepared;
        // Reconcile exactly as after process death. An already-placed valid
        // candidate commits forward; conflicts preserve all trees and records.
        plan.resolve(lock, &|| Ok(()))?;
        if plan.record.phase == Phase::RolledBack {
            let _ = plan.finish(lock, &|| Ok(()));
            return Err(original);
        }
    }
    let cleanup = plan.finish(lock, &|| Ok(())).unwrap_or(Removal {
        unknown: true,
        cleanup_pending: true,
    });
    Ok(Commit {
        repaired: plan.record.had_previous,
        unknown: cleanup.unknown,
        cleanup_pending: cleanup.cleanup_pending,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn staging_reaper_preserves_replacement_dependencies_and_conflicts() {
        let root = tempfile::tempdir().unwrap();
        let (unowned, lock) = candidate(root.path());
        let stage = staging::Stage::create(root.path(), &lock).unwrap();
        for file in checked_files(&lock).unwrap() {
            let name = file["path"].as_str().unwrap();
            let destination = file_at(stage.path(), name, true).unwrap().unwrap();
            fs::copy(unowned.path().join(name), destination).unwrap();
        }
        fs::write(stage.path().join("user-note"), b"preserve").unwrap();
        let target = previous(root.path(), &lock);
        let mut plan = Plan::prepare(stage.path(), &target, &lock).unwrap();
        staging::recover(root.path(), &lock, &|| Ok(())).unwrap();
        assert!(staging::pending(root.path()).unwrap());
        assert!(stage.path().join("bin/engine").exists());
        let original = plan.record.staging.clone();
        plan.record.staging = "../foreign".into();
        plan.write(false).unwrap();
        staging::recover(root.path(), &lock, &|| Ok(())).unwrap();
        assert!(stage.path().join("bin/engine").exists());
        assert!(staging::pending(root.path()).unwrap());
        plan.record.staging = original;
        plan.write(false).unwrap();
        // Recovery ordering resolves rollback ownership before the stage reaper.
        recover(root.path(), &lock, &|| Ok(())).unwrap();
        staging::recover(root.path(), &lock, &|| Ok(())).unwrap();
        assert!(!staging::pending(root.path()).unwrap());
        assert!(!stage.path().join("bin/engine").exists());
        assert_eq!(
            fs::read(stage.path().join("user-note")).unwrap(),
            b"preserve"
        );
        assert_eq!(
            fs::read(target.join("bin/engine")).unwrap(),
            b"old bytes needing repair"
        );
    }
    fn candidate(parent: &Path) -> (tempfile::TempDir, Value) {
        let staging = tempfile::Builder::new()
            .prefix("install-")
            .tempdir_in(parent)
            .unwrap();
        let files = ["bin/engine", "data/model", "NOTICE"]
            .into_iter()
            .map(|name| {
                let bytes = format!("new verified {name}").into_bytes();
                let path = staging.path().join(name);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, &bytes).unwrap();
                json!({"path":name,"size":bytes.len(),"sha256":hash(&bytes)})
            })
            .collect::<Vec<_>>();
        (
            staging,
            json!({"schema":1,"id":"fonts","version":crate::publication_fonts::VERSION,
            "platform":"universal","files":files}),
        )
    }
    fn previous(parent: &Path, lock: &Value) -> PathBuf {
        let target = parent.join(lock["version"].as_str().unwrap());
        for file in lock["files"].as_array().unwrap() {
            let path = target.join(file["path"].as_str().unwrap());
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"old bytes needing repair").unwrap();
        }
        fs::write(target.join("user-note"), b"keep old unknown file").unwrap();
        target
    }
    #[test]
    fn interrupted_replacement_before_candidate_restores_previous_and_preserves_extras() {
        for old_moved in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let (stage, lock) = candidate(root.path());
            let target = previous(root.path(), &lock);
            fs::write(
                stage.path().join("new-user-note"),
                b"keep staging unknown file",
            )
            .unwrap();
            let stage = stage.keep();
            let plan = Plan::prepare(&stage, &target, &lock).unwrap();
            if old_moved {
                fs::rename(&target, root.path().join(&plan.record.previous)).unwrap();
            }
            recover(root.path(), &lock, &|| Ok(())).unwrap();
            assert_eq!(
                fs::read(target.join("bin/engine")).unwrap(),
                b"old bytes needing repair"
            );
            assert_eq!(
                fs::read(target.join("user-note")).unwrap(),
                b"keep old unknown file"
            );
            assert_eq!(
                fs::read(stage.join("new-user-note")).unwrap(),
                b"keep staging unknown file"
            );
            assert!(!stage.join("bin/engine").exists());
            assert!(!pending(root.path()).unwrap());
            recover(root.path(), &lock, &|| Ok(())).unwrap();
        }
    }
    #[test]
    fn interrupted_candidate_commits_only_after_validation_and_retains_old_unknown_files() {
        for had_previous in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let (stage, lock) = candidate(root.path());
            let target = root.path().join(lock["version"].as_str().unwrap());
            if had_previous {
                previous(root.path(), &lock);
            }
            let stage = stage.keep();
            let plan = Plan::prepare(&stage, &target, &lock).unwrap();
            let backup = root.path().join(&plan.record.previous);
            if had_previous {
                fs::rename(&target, &backup).unwrap();
            }
            fs::rename(&stage, &target).unwrap();
            fs::write(target.join("bin/engine"), b"corrupt candidate").unwrap();
            assert!(recover(root.path(), &lock, &|| Ok(())).is_err());
            assert!(blocking(root.path(), &lock).unwrap());
            if had_previous {
                assert_eq!(
                    fs::read(backup.join("bin/engine")).unwrap(),
                    b"old bytes needing repair"
                );
            }
            fs::write(target.join("bin/engine"), b"new verified bin/engine").unwrap();
            let error =
                recover(root.path(), &lock, &|| Err(ApiError::new(422, "cancelled"))).unwrap_err();
            assert_eq!(error.code, "cancelled");
            assert!(pending(root.path()).unwrap());
            recover(root.path(), &lock, &|| Ok(())).unwrap();
            validate(&target, &lock).unwrap();
            if had_previous {
                assert_eq!(
                    fs::read(backup.join("user-note")).unwrap(),
                    b"keep old unknown file"
                );
                assert!(!backup.join("bin/engine").exists());
            }
            assert!(!pending(root.path()).unwrap());
        }
    }
    #[test]
    fn conflicting_target_and_forged_journal_never_overwrite_or_delete_user_data() {
        let root = tempfile::tempdir().unwrap();
        let (stage, lock) = candidate(root.path());
        let target = previous(root.path(), &lock);
        let stage = stage.keep();
        let mut plan = Plan::prepare(&stage, &target, &lock).unwrap();
        let backup = root.path().join(&plan.record.previous);
        fs::rename(&target, &backup).unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(target.join("user-created"), b"never clobber").unwrap();
        assert!(recover(root.path(), &lock, &|| Ok(())).is_err());
        assert_eq!(
            fs::read(target.join("user-created")).unwrap(),
            b"never clobber"
        );
        assert_eq!(
            fs::read(backup.join("bin/engine")).unwrap(),
            b"old bytes needing repair"
        );
        assert!(stage.join("bin/engine").is_file());
        plan.record.staging = "../user-data".into();
        plan.write(false).unwrap();
        assert!(recover(root.path(), &lock, &|| Ok(())).is_err());
        assert!(blocking(root.path(), &lock).is_err());
        assert!(plan.directory.join(JOURNAL).is_file());
    }
    #[cfg(windows)]
    #[test]
    fn failed_previous_rename_restores_old_runtime_without_recursive_cleanup() {
        let root = tempfile::tempdir().unwrap();
        let (stage, lock) = candidate(root.path());
        let target = previous(root.path(), &lock);
        let pinned = resource_file(&target.join("bin/engine")).unwrap();
        fs::write(stage.path().join("user-note"), b"keep staged extra").unwrap();
        let staged = stage.path().to_path_buf();
        assert!(commit(&stage.keep(), &target, &lock).is_err());
        assert_eq!(
            fs::read(target.join("bin/engine")).unwrap(),
            b"old bytes needing repair"
        );
        assert_eq!(
            fs::read(staged.join("user-note")).unwrap(),
            b"keep staged extra"
        );
        assert!(!staged.join("bin/engine").exists());
        assert!(!pending(root.path()).unwrap());
        drop(pinned);
    }
    #[test]
    fn actual_install_owner_crash_recovers_each_rename_boundary_on_first_lease() {
        use std::{
            process::{Child, Command, Stdio},
            sync::OnceLock,
            time::{Duration, Instant},
        };
        static CHILD: OnceLock<(PathBuf, String)> = OnceLock::new();
        struct Barrier;
        impl log::Log for Barrier {
            fn enabled(&self, m: &log::Metadata) -> bool {
                m.target() == "readmd::capability::transaction"
            }
            fn log(&self, record: &log::Record) {
                if !self.enabled(record.metadata()) {
                    return;
                }
                let (root, boundary) = CHILD.get().unwrap();
                let expected = format!("replacement {boundary}");
                if record.args().to_string() != expected {
                    return;
                }
                fs::write(root.join("child.ready"), expected).unwrap();
                let until = Instant::now() + Duration::from_secs(30);
                while !root.join("child.stop").is_file() {
                    assert!(
                        Instant::now() < until,
                        "Parent did not stop installation child"
                    );
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            fn flush(&self) {}
        }
        static BARRIER: Barrier = Barrier;
        if let Some(root) = std::env::var_os("READMD_REPLACEMENT_CHILD_ROOT") {
            let root = PathBuf::from(root);
            let boundary = std::env::var("READMD_REPLACEMENT_CHILD_BOUNDARY").unwrap();
            assert!(matches!(
                boundary.as_str(),
                "previous moved" | "candidate moved" | "committed"
            ));
            CHILD.set((root.clone(), boundary)).unwrap();
            log::set_logger(&BARRIER).unwrap();
            log::set_max_level(log::LevelFilter::Debug);
            let parent = root.join("profile/capabilities/managed-v2/fonts");
            let (stage, lock) = candidate(&parent);
            let target = parent.join(crate::publication_fonts::VERSION);
            let _local = resource_guard();
            let _process = crate::capability_process::try_acquire(
                &root.join("profile"),
                "fonts",
                crate::capability_process::Mode::Exclusive,
            )
            .unwrap()
            .unwrap();
            commit(&stage.keep(), &target, &lock).unwrap();
            return;
        }
        struct OwnedChild(Child);
        impl Drop for OwnedChild {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        for boundary in ["previous moved", "candidate moved", "committed"] {
            let root = tempfile::tempdir().unwrap();
            let data = root.path().join("profile");
            let assets = root.path().join("assets");
            let parent = data.join("capabilities/managed-v2/fonts");
            fs::create_dir_all(&parent).unwrap();
            fs::create_dir_all(assets.join("capabilities")).unwrap();
            let (stage, lock) = candidate(&parent);
            drop(stage);
            let target = previous(&parent, &lock);
            fs::write(
                assets.join(format!(
                    "capabilities/fonts-{}-universal.lock.json",
                    crate::publication_fonts::VERSION
                )),
                serde_json::to_vec(&lock).unwrap(),
            )
            .unwrap();
            let mut child = OwnedChild(Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "capability_packs::replacement::tests::actual_install_owner_crash_recovers_each_rename_boundary_on_first_lease", "--test-threads=1"])
                .env("READMD_REPLACEMENT_CHILD_ROOT", root.path()).env("READMD_REPLACEMENT_CHILD_BOUNDARY", boundary)
                .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::inherit()).spawn().unwrap());
            let until = Instant::now() + Duration::from_secs(30);
            while !root.path().join("child.ready").is_file() {
                assert!(
                    child.0.try_wait().unwrap().is_none(),
                    "Installation child exited before boundary"
                );
                assert!(
                    Instant::now() < until,
                    "Installation child did not reach boundary"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(crate::capability_process::try_acquire(
                &data,
                "fonts",
                crate::capability_process::Mode::Exclusive
            )
            .unwrap()
            .is_none());
            child.0.kill().unwrap();
            assert!(!child.0.wait().unwrap().success());
            drop(child);
            let result = lease(&data, &assets, "fonts");
            if boundary == "previous moved" {
                // Recovery restored the damaged old installation, so resource
                // validation correctly refuses it; it must not claim readiness.
                assert!(result.is_err());
                assert_eq!(
                    fs::read(target.join("bin/engine")).unwrap(),
                    b"old bytes needing repair"
                );
                assert_eq!(
                    fs::read(target.join("user-note")).unwrap(),
                    b"keep old unknown file"
                );
            } else {
                let lease = result.unwrap();
                validate(&lease.directory, &lock).unwrap();
                let backups = fs::read_dir(&parent)
                    .unwrap()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_name().to_string_lossy().starts_with("replaced-"))
                    .collect::<Vec<_>>();
                assert_eq!(backups.len(), 1);
                assert_eq!(
                    fs::read(backups[0].path().join("user-note")).unwrap(),
                    b"keep old unknown file"
                );
                assert!(!backups[0].path().join("bin/engine").exists());
            }
            assert!(!transactions_pending(&parent).unwrap());
        }
    }
}
