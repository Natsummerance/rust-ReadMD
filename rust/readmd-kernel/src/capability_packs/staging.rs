//! Durable extraction ownership before the replacement journal exists. Every
//! caller holds INSTALL and the capability's exclusive, non-inherited OS guard
//! until Stage drops. Recovery acquires the same guards, never a PID heuristic.
//! The control record is a sibling of the payload, so it cannot become a loader
//! input or be mistaken for a file belonging to an installed runtime.
use super::transaction::{
    exists, failure, file_at, fingerprint, journal_candidates, nonce, prune_checked, regular,
};
use super::*;
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    time::{Duration, Instant},
};

const PREFIX: &str = "stage-txn-";
const JOURNAL: &str = "staging-v1.json";
const OWNER: &str = "ReadMD capability staging v1";

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Planned,
    Owned,
}
#[derive(Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: u8,
    owner: String,
    nonce: String,
    manifest: String,
    staging: String,
    files: Vec<String>,
    phase: Phase,
}
struct Plan {
    parent: PathBuf,
    directory: PathBuf,
    record: Record,
}
pub(super) struct Stage<'a> {
    plan: Plan,
    lock: &'a Value,
    root: PathBuf,
}

fn planned_files(lock: &Value) -> ApiResult<Vec<String>> {
    checked_files(lock)?
        .iter()
        .map(|f| f["path"].as_str().map(str::to_owned).ok_or_else(error))
        .collect()
}
impl Plan {
    fn prepare(parent: &Path, lock: &Value) -> ApiResult<Self> {
        let files = planned_files(lock)?;
        real_directory(parent)?;
        let parent = parent.canonicalize().map_err(|_| error())?;
        let id = uuid::Uuid::new_v4();
        let directory = parent.join(format!("{PREFIX}{id}"));
        fs::create_dir(&directory).map_err(|_| error())?;
        // No payload root or file exists before a synced ownership record.
        log::debug!(target:"readmd::capability::transaction", "staging control created");
        let plan = Self {
            parent,
            directory,
            record: Record {
                schema: 1,
                owner: OWNER.into(),
                nonce: id.to_string(),
                manifest: fingerprint(lock)?,
                staging: format!("install-{}", id.simple()),
                files,
                phase: Phase::Planned,
            },
        };
        plan.write(true)?;
        log::debug!(target:"readmd::capability::transaction", "staging planned");
        Ok(plan)
    }
    fn read(directory: &Path, lock: &Value) -> ApiResult<Self> {
        real_directory(directory)?;
        let directory = directory.canonicalize().map_err(|_| error())?;
        let parent = directory.parent().ok_or_else(error)?.to_path_buf();
        real_directory(&parent)?;
        let (file, size) = resource_file(&directory.join(JOURNAL))?;
        if size == 0 || size > crate::capability_archive::MAX_MANIFEST as u64 {
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
            || record.files != planned_files(lock)?
            || record.staging
                != format!(
                    "install-{}",
                    uuid::Uuid::parse_str(&record.nonce)
                        .map_err(|_| error())?
                        .simple()
                )
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
        let mut temp = tempfile::Builder::new()
            .prefix("journal-write-")
            .tempfile_in(&self.directory)
            .map_err(|_| error())?;
        temp.write_all(&serde_json::to_vec(&self.record).map_err(|_| error())?)
            .and_then(|_| temp.as_file().sync_all())
            .map_err(|_| error())?;
        if initial {
            temp.persist_noclobber(target).map_err(|_| error())?;
        } else {
            temp.persist(target).map_err(|_| error())?;
        }
        #[cfg(unix)]
        for dir in [&self.directory, &self.parent] {
            fs::File::open(dir)
                .and_then(|f| f.sync_all())
                .map_err(|_| error())?;
        }
        Ok(())
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
        // A lost/changed record is never permission to delete from its old plan.
        let actual = Self::read(&self.directory, lock)?;
        if actual.record != self.record || actual.parent != self.parent {
            return Err(error());
        }
        if replacement::references_trusted(&self.parent, &self.record.staging, trusted)? {
            return Ok(Removal {
                unknown: false,
                cleanup_pending: true,
            });
        }
        let stage = self.parent.join(&self.record.staging);
        if self.record.phase == Phase::Owned && exists(&stage)? {
            real_directory(&stage)?;
            if stage.canonicalize().map_err(|_| error())?.parent() != Some(self.parent.as_path()) {
                return Err(error());
            }
            for name in &self.record.files {
                check()?;
                if let Some(path) = file_at(&stage, name, false)? {
                    if exists(&path)? {
                        regular(&path)?;
                        fs::remove_file(path).map_err(|_| failure(&self.directory))?;
                    }
                }
            }
            prune_checked(&stage, &self.record.files, check)?;
        }
        // PLANNED never proves ownership of an existing payload root: a crash
        // between mkdir and the OWNED update leaves only an empty, retained root.
        let retained = exists(&stage)?;
        check()?;
        regular(&self.directory.join(JOURNAL))?;
        fs::remove_file(self.directory.join(JOURNAL)).map_err(|_| failure(&self.directory))?;
        let _ = fs::remove_dir(&self.directory); // empty only, preserve additions
        Ok(Removal {
            unknown: retained || exists(&self.directory)?,
            cleanup_pending: false,
        })
    }
}
impl<'a> Stage<'a> {
    pub(super) fn create(parent: &Path, lock: &'a Value) -> ApiResult<Self> {
        let mut plan = Plan::prepare(parent, lock)?;
        // create_dir, never create_dir_all or reuse. If this fails, recovery of
        // the PLANNED record preserves any same-name foreign directory.
        fs::create_dir(plan.parent.join(&plan.record.staging)).map_err(|_| error())?;
        log::debug!(target:"readmd::capability::transaction", "staging root created");
        plan.record.phase = Phase::Owned;
        plan.write(false)?;
        log::debug!(target:"readmd::capability::transaction", "staging ownership recorded");
        let root = plan.parent.join(&plan.record.staging);
        Ok(Self { plan, lock, root })
    }
    pub(super) fn path(&self) -> &Path {
        &self.root
    }
    pub(super) fn cleanup(&self) -> Removal {
        // Cleanup is best effort; do not delay cancel/return for a 40k-file
        // tree. A durable record lets the next guarded access finish the work.
        let until = Instant::now() + Duration::from_millis(100);
        self.plan
            .finish(self.lock, &|| {
                if Instant::now() < until {
                    Ok(())
                } else {
                    Err(ApiError::new(422, "cancelled"))
                }
            })
            .unwrap_or(Removal {
                unknown: true,
                cleanup_pending: true,
            })
    }
}
impl Drop for Stage<'_> {
    fn drop(&mut self) {
        if self.plan.directory.join(JOURNAL).exists() && self.cleanup().cleanup_pending {
            log::debug!(target:"readmd::capability::transaction", "staging cleanup pending");
        }
    }
}
pub(super) fn pending(parent: &Path) -> ApiResult<bool> {
    Ok(!journal_candidates(parent, PREFIX, JOURNAL)?.is_empty())
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
        // Unrecognized/mismatched ownership is retained. It is neither an
        // installed engine nor a reason to destroy a valid installed engine.
        let Ok(lock) = trusted.for_record(&directory, JOURNAL) else {
            continue;
        };
        let Ok(plan) = Plan::read(&directory, lock) else {
            continue;
        };
        let interrupted = std::cell::Cell::new(false);
        let checked = || {
            let result = check();
            if result.is_err() {
                interrupted.set(true);
            }
            result
        };
        if let Err(error) = plan.finish_trusted(lock, trusted, &checked) {
            if interrupted.get() {
                return Err(error);
            }
            check()?;
            log::warn!(target:"readmd::capability::transaction", "staging recovery retained pending files");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Value {
        json!({"schema":1,"id":"fonts","version":"2026.10.10","platform":"universal",
            "files":(["bin/engine","data/model","NOTICE"].into_iter().map(|p|
                json!({"path":p,"size":1,"sha256":hash(b"x")})).collect::<Vec<_>>())})
    }
    fn put(stage: &Stage<'_>) {
        for f in &stage.plan.record.files {
            let path = file_at(&stage.path(), f, true).unwrap().unwrap();
            fs::write(path, b"partial owned payload").unwrap();
        }
    }
    #[test]
    fn staging_cancel_drop_and_recovery_remove_only_owned_planned_files() {
        let root = tempfile::tempdir().unwrap();
        let lock = fixture();
        let foreign = root.path().join("install-unmarked");
        fs::create_dir(&foreign).unwrap();
        fs::write(foreign.join("keep"), b"foreign").unwrap();
        let stage = Stage::create(root.path(), &lock).unwrap();
        put(&stage);
        fs::write(stage.path().join("user-note"), b"keep").unwrap();
        fs::write(
            stage.plan.directory.join("user-note"),
            b"keep journal extra",
        )
        .unwrap();
        let path = stage.path().to_path_buf();
        let control = stage.plan.directory.clone();
        drop(stage);
        // Drop deliberately stops after 100ms and retains a durable plan.
        // A loaded scheduler can exhaust that budget even for this small
        // fixture. Verify the documented next guarded access, rather than
        // assuming cancellation always finishes synchronous disk cleanup.
        recover(root.path(), &lock, &|| Ok(())).unwrap();
        assert_eq!(fs::read(path.join("user-note")).unwrap(), b"keep");
        assert!(!path.join("bin/engine").exists());
        assert_eq!(
            fs::read(control.join("user-note")).unwrap(),
            b"keep journal extra"
        );
        assert!(!pending(root.path()).unwrap());
        let stage = Stage::create(root.path(), &lock).unwrap();
        put(&stage);
        let path = stage.path().to_path_buf();
        std::mem::forget(stage); // simulate absent Drop after death
        let error =
            recover(root.path(), &lock, &|| Err(ApiError::new(422, "cancelled"))).unwrap_err();
        assert_eq!(error.code, "cancelled");
        assert!(path.join("NOTICE").exists());
        recover(root.path(), &lock, &|| Ok(())).unwrap();
        assert!(!path.exists());
        assert_eq!(fs::read(foreign.join("keep")).unwrap(), b"foreign");
        recover(root.path(), &lock, &|| Ok(())).unwrap();
    }
    #[test]
    fn staging_planned_or_forged_records_do_not_claim_payload_or_arbitrary_paths() {
        let root = tempfile::tempdir().unwrap();
        let lock = fixture();
        let plan = Plan::prepare(root.path(), &lock).unwrap();
        let path = plan.parent.join(&plan.record.staging);
        fs::create_dir(&path).unwrap();
        fs::write(path.join("NOTICE"), b"same name not proven owned").unwrap();
        recover(root.path(), &lock, &|| Ok(())).unwrap();
        assert_eq!(
            fs::read(path.join("NOTICE")).unwrap(),
            b"same name not proven owned"
        );
        let stage = Stage::create(root.path(), &lock).unwrap();
        put(&stage);
        let control = stage.plan.directory.clone();
        let path = stage.path().to_path_buf();
        std::mem::forget(stage);
        let mut plan = Plan::read(&control, &lock).unwrap();
        plan.record.files.push("../outside".into());
        plan.write(false).unwrap();
        recover(root.path(), &lock, &|| Ok(())).unwrap();
        assert!(path.join("NOTICE").exists());
        assert!(control.join(JOURNAL).exists());
        let changed = json!({"schema":1,"id":"fonts","version":"2026.10.10","platform":"universal",
            "files":(["bin/engine","data/model","NOTICE"].into_iter().map(|p|
                json!({"path":p,"size":2,"sha256":hash(b"xx")})).collect::<Vec<_>>())});
        recover(root.path(), &changed, &|| Ok(())).unwrap();
        assert!(path.join("NOTICE").exists());
        let stage = Stage::create(root.path(), &lock).unwrap();
        put(&stage);
        let path = stage.path().to_path_buf();
        fs::write(
            stage.plan.directory.join(JOURNAL),
            b"damaged ownership record",
        )
        .unwrap();
        drop(stage);
        recover(root.path(), &lock, &|| Ok(())).unwrap();
        assert!(path.join("NOTICE").exists());
    }
    #[cfg(windows)]
    #[test]
    fn staging_junction_cannot_redirect_cleanup_into_external_data() {
        use std::os::windows::process::CommandExt;
        let root = tempfile::tempdir().unwrap();
        let lock = fixture();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("model"), b"never touch external data").unwrap();
        let stage = Stage::create(root.path(), &lock).unwrap();
        let output = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(stage.path().join("data"))
            .arg(outside.path())
            .creation_flags(0x08000000)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "Could not construct native junction fixture"
        );
        let path = stage.path().to_path_buf();
        std::mem::forget(stage);
        recover(root.path(), &lock, &|| Ok(())).unwrap();
        assert!(pending(root.path()).unwrap());
        assert!(path.join("data").exists());
        assert_eq!(
            fs::read(outside.path().join("model")).unwrap(),
            b"never touch external data"
        );
    }
    #[test]
    fn staging_interrupted_cleanup_keeps_a_durable_resume_plan() {
        let root = tempfile::tempdir().unwrap();
        let lock = fixture();
        let stage = Stage::create(root.path(), &lock).unwrap();
        put(&stage);
        let checks = std::cell::Cell::new(0);
        let result = stage.plan.finish(&lock, &|| {
            checks.set(checks.get() + 1);
            if checks.get() > 2 {
                Err(ApiError::new(422, "cancelled"))
            } else {
                Ok(())
            }
        });
        assert_eq!(result.err().unwrap().code, "cancelled");
        assert!(!stage.path().join("bin/engine").exists());
        assert!(stage.path().join("data/model").exists());
        assert!(pending(root.path()).unwrap());
        recover(root.path(), &lock, &|| Ok(())).unwrap();
        assert!(!stage.path().exists());
        assert!(!pending(root.path()).unwrap());
    }
    #[cfg(windows)]
    #[test]
    fn staging_locked_file_retains_journal_then_resumes_after_release() {
        let root = tempfile::tempdir().unwrap();
        let lock = fixture();
        let stage = Stage::create(root.path(), &lock).unwrap();
        put(&stage);
        let pinned = resource_file(&stage.path().join("NOTICE")).unwrap();
        let path = stage.path().to_path_buf();
        std::mem::forget(stage);
        recover(root.path(), &lock, &|| Ok(())).unwrap();
        assert!(pending(root.path()).unwrap());
        assert!(path.join("NOTICE").exists());
        drop(pinned);
        recover(root.path(), &lock, &|| Ok(())).unwrap();
        assert!(!pending(root.path()).unwrap());
        assert!(!path.exists());
    }
    #[cfg(unix)]
    #[test]
    fn staging_redirected_ancestor_preserves_external_files() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let lock = fixture();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("model"), b"never touch").unwrap();
        let stage = Stage::create(root.path(), &lock).unwrap();
        symlink(outside.path(), stage.path().join("data")).unwrap();
        let path = stage.path().to_path_buf();
        std::mem::forget(stage);
        recover(root.path(), &lock, &|| Ok(())).unwrap();
        assert!(pending(root.path()).unwrap());
        assert!(path.join("data").is_symlink());
        assert_eq!(
            fs::read(outside.path().join("model")).unwrap(),
            b"never touch"
        );
    }
}
