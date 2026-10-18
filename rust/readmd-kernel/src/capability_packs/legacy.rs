//! Read-only migration of exact, currently locked resources. Never execute,
//! rename, restore or clean legacy trees: old clients do not follow our locks.
//! Copy into an owned stage, hash every byte and probe only the private copy.
use super::transaction::{exists, file_at, fingerprint};
use super::*;
use std::io::Write;

const RECEIPT: &str = "legacy-migration-v1.json";

fn source(data: &Path, selected: Spec) -> PathBuf {
    data.join("capabilities")
        .join(selected.id)
        .join(selected.version)
}
pub(super) fn needed(data: &Path, selected: Spec) -> ApiResult<bool> {
    let parent = root_for(data, selected);
    // Any receipt blocks automatic import, including an unknown/corrupt one.
    // It grants no deletion/execution rights and never overrides an explicit
    // install. A successful explicit uninstall must not resurrect old files.
    Ok(!exists(&parent.join(RECEIPT))?
        && !exists(&parent.join(selected.version))?
        && exists(&source(data, selected))?)
}
fn receipt(parent: &Path, lock: &Value, decision: &str) -> ApiResult<()> {
    real_directory(parent)?;
    let path = parent.join(RECEIPT);
    if exists(&path)? {
        return Ok(());
    } // preserve even an unrecognized marker
    let record = json!({"schema":1,"owner":"ReadMD legacy capability migration v1",
        "id":lock["id"],"version":lock["version"],"manifest":fingerprint(lock)?,
        "decision":decision,"nonce":uuid::Uuid::new_v4().to_string()});
    let mut temporary = tempfile::Builder::new()
        .prefix("policy-write-")
        .tempfile_in(parent)
        .map_err(|_| error())?;
    temporary
        .write_all(&serde_json::to_vec(&record).map_err(|_| error())?)
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|_| error())?;
    match temporary.persist_noclobber(&path) {
        Ok(_) => (),
        Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(_) => return Err(error()),
    }
    #[cfg(unix)]
    fs::File::open(parent)
        .and_then(|f| f.sync_all())
        .map_err(|_| error())?;
    Ok(())
}
pub(super) fn disable(data: &Path, selected: Spec, lock: &Value) -> ApiResult<()> {
    // Persist before removal. Failure leaves the resource in place rather than
    // allowing a successful uninstall to be undone by next-start migration.
    receipt(&managed_parent(data, selected, true)?, lock, "uninstalled")
}
pub(super) fn migrate(
    data: &Path,
    selected: Spec,
    lock: &Value,
    check: &dyn Fn() -> ApiResult<()>,
) -> ApiResult<()> {
    migrate_with(data, selected, lock, check, &|directory| {
        probe(selected.id, directory, data)
    })
}
fn migrate_with(
    data: &Path,
    selected: Spec,
    lock: &Value,
    check: &dyn Fn() -> ApiResult<()>,
    probe_copy: &dyn Fn(&Path) -> Result<(), String>,
) -> ApiResult<()> {
    check()?;
    if !needed(data, selected)? {
        return Ok(());
    }
    let base = data.canonicalize().map_err(|_| error())?;
    let mut origin = base.clone();
    for part in ["capabilities", selected.id, selected.version] {
        origin.push(part);
        real_directory(&origin)?;
    }
    if origin.canonicalize().map_err(|_| error())? != origin {
        return Err(error());
    }
    let files = checked_files(lock)?;
    let total = files.iter().try_fold(0u64, |sum, f| {
        sum.checked_add(f["size"].as_u64().ok_or_else(error)?)
            .ok_or_else(error)
    })?;
    if total > crate::capability_archive::MAX_EXPANDED {
        return Err(error());
    }
    // Count a migration as an active transfer too. For expanded trees above
    // the 2GiB input quota, reserve its entire byte budget conservatively.
    // This does not claim to be a profile-wide expanded-disk quota.
    let _transfer = crate::capability_process::reserve_transfer(
        data,
        total.clamp(1, crate::capability_archive::MAX_DOWNLOAD),
    )?;
    let parent = managed_parent(data, selected, true)?;
    let stage = staging::Stage::create(&parent, lock)?;
    #[cfg(windows)]
    if selected.id == "latex" {
        crate::worker_sandbox::prepare_empty_runtime(stage.path()).map_err(|_| error())?;
    }
    let mut buffer = [0u8; 64 * 1024];
    for (index, item) in files.iter().enumerate() {
        check()?;
        let name = item["path"].as_str().ok_or_else(error)?;
        let input = parent_for(&origin, name)?;
        let (mut input, size) = resource_file(&input)?;
        if Some(size) != item["size"].as_u64() {
            return Err(error());
        }
        let output = file_at(stage.path(), name, true)?.ok_or_else(error)?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .map_err(|_| error())?;
        let mut digest = Sha256::new();
        let mut written = 0u64;
        loop {
            check()?;
            let n = input.read(&mut buffer).map_err(|_| error())?;
            if n == 0 {
                break;
            }
            written = written.checked_add(n as u64).ok_or_else(error)?;
            if written > size {
                return Err(error());
            }
            output.write_all(&buffer[..n]).map_err(|_| error())?;
            digest.update(&buffer[..n]);
        }
        if written != size
            || Some(format!("{:x}", digest.finalize()).as_str()) != item["sha256"].as_str()
        {
            return Err(error());
        }
        output.sync_all().map_err(|_| error())?;
        #[cfg(unix)]
        if (lock["schema"] == 1 && name.starts_with("bin/")) || item["executable"] == true {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(stage.path().join(name), fs::Permissions::from_mode(0o755))
                .map_err(|_| error())?;
        }
        if index == 0 {
            log::debug!(target:"readmd::capability::transaction", "legacy migration first file copied");
        }
    }
    check()?;
    validate_partition(
        stage.path(),
        &stage.path().canonicalize().map_err(|_| error())?,
        files,
        0,
        1,
        check,
    )?;
    let result = probe_copy(stage.path());
    check()?;
    result.map_err(|_| error().noted("stage", "migration-probe"))?;
    log::debug!(target:"readmd::capability::transaction", "legacy migration probed");
    check()?;
    replacement::commit(stage.path(), &parent.join(selected.version), lock)?;
    // The valid committed tree can be used even if the post-commit receipt
    // write fails. Explicit uninstall always persists an opt-out first.
    if receipt(&parent, lock, "migrated").is_err() {
        log::warn!(target:"readmd::capability::transaction", "legacy migration receipt pending");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(data: &Path) -> (Spec, Value, PathBuf) {
        let selected = spec("fonts").unwrap();
        let origin = source(data, selected);
        let lock = super::super::tests::fixture(&origin);
        fs::write(origin.join("user-note"), b"preserve legacy extra").unwrap();
        (selected, lock, origin)
    }
    fn migrate_fixture(data: &Path, selected: Spec, lock: &Value) -> ApiResult<()> {
        migrate_with(data, selected, lock, &|| Ok(()), &|p| {
            validate(p, lock).map_err(|e| e.code)
        })
    }
    #[test]
    fn legacy_copy_is_independent_preserves_originals_and_does_not_import_unknowns() {
        let data = tempfile::tempdir().unwrap();
        let (selected, lock, origin) = fixture(data.path());
        let _guard = crate::capability_process::try_acquire(
            data.path(),
            selected.id,
            crate::capability_process::Mode::Exclusive,
        )
        .unwrap()
        .unwrap();
        migrate_fixture(data.path(), selected, &lock).unwrap();
        let target = root_for(data.path(), selected).join(selected.version);
        validate(&target, &lock).unwrap();
        assert!(!target.join("user-note").exists());
        let first = lock["files"][0]["path"].as_str().unwrap();
        let before = fs::read(target.join(first)).unwrap();
        fs::write(origin.join(first), b"old client changed source").unwrap();
        assert_eq!(fs::read(target.join(first)).unwrap(), before);
        assert_eq!(
            fs::read(origin.join("user-note")).unwrap(),
            b"preserve legacy extra"
        );
        assert!(!needed(data.path(), selected).unwrap());
        assert!(root_for(data.path(), selected).join(RECEIPT).exists());
    }
    #[test]
    fn explicit_uninstall_disables_automatic_legacy_resurrection_and_preserves_unknown_receipt() {
        let data = tempfile::tempdir().unwrap();
        let (selected, lock, origin) = fixture(data.path());
        disable(data.path(), selected, &lock).unwrap();
        assert!(!needed(data.path(), selected).unwrap());
        migrate_fixture(data.path(), selected, &lock).unwrap();
        assert!(!root_for(data.path(), selected)
            .join(selected.version)
            .exists());
        validate(&origin, &lock).unwrap();
        let data = tempfile::tempdir().unwrap();
        let (selected, lock, _) = fixture(data.path());
        let parent = managed_parent(data.path(), selected, true).unwrap();
        fs::write(parent.join(RECEIPT), b"unknown user file").unwrap();
        disable(data.path(), selected, &lock).unwrap();
        assert_eq!(
            fs::read(parent.join(RECEIPT)).unwrap(),
            b"unknown user file"
        );
        assert!(!needed(data.path(), selected).unwrap());
    }
    #[test]
    fn corrupt_legacy_bytes_and_failed_probe_never_create_an_installed_tree_or_change_legacy() {
        for corruption in [true, false] {
            let data = tempfile::tempdir().unwrap();
            let (selected, lock, origin) = fixture(data.path());
            let _guard = crate::capability_process::try_acquire(
                data.path(), selected.id, crate::capability_process::Mode::Exclusive,
            ).unwrap().unwrap();
            let first = lock["files"][0]["path"].as_str().unwrap();
            if corruption {
                fs::write(origin.join(first), b"damaged").unwrap();
            }
            let before = fs::read(origin.join(first)).unwrap();
            assert!(
                migrate_with(data.path(), selected, &lock, &|| Ok(()), &|_| Err(
                    "actual probe rejected".into()
                ))
                .is_err()
            );
            assert!(!root_for(data.path(), selected)
                .join(selected.version)
                .exists());
            assert_eq!(fs::read(origin.join(first)).unwrap(), before);
            // Stage::drop has a 100ms return budget, including under parallel
            // test load. Exercise the next guarded recovery rather than
            // incorrectly requiring synchronous cleanup within that budget.
            staging::recover(&root_for(data.path(), selected), &lock, &|| Ok(())).unwrap();
            assert!(!staging::pending(&root_for(data.path(), selected)).unwrap());
            assert!(!root_for(data.path(), selected).join(selected.version).exists());
            assert_eq!(fs::read(origin.join(first)).unwrap(), before);
        }
    }
    #[test]
    fn migration_cancellation_and_transfer_saturation_preserve_source_and_target() {
        let data = tempfile::tempdir().unwrap();
        let (selected, lock, origin) = fixture(data.path());
        let calls = std::cell::Cell::new(0);
        let err = migrate_with(
            data.path(),
            selected,
            &lock,
            &|| {
                calls.set(calls.get() + 1);
                if calls.get() > 3 {
                    Err(ApiError::new(422, "cancelled"))
                } else {
                    Ok(())
                }
            },
            &|_| Ok(()),
        )
        .unwrap_err();
        assert_eq!(err.code, "cancelled");
        validate(&origin, &lock).unwrap();
        let budget = crate::capability_process::reserve_transfer(
            data.path(),
            crate::capability_archive::MAX_DOWNLOAD,
        )
        .unwrap();
        assert_eq!(
            migrate_fixture(data.path(), selected, &lock)
                .unwrap_err()
                .code,
            "capability_install_busy"
        );
        assert!(!root_for(data.path(), selected)
            .join(selected.version)
            .exists());
        drop(budget);
        migrate_fixture(data.path(), selected, &lock).unwrap();
    }
    #[test]
    fn migration_does_not_overwrite_an_existing_private_destination() {
        let data = tempfile::tempdir().unwrap();
        let (selected, lock, origin) = fixture(data.path());
        let target = managed_parent(data.path(), selected, true)
            .unwrap()
            .join(selected.version);
        fs::create_dir(&target).unwrap();
        fs::write(target.join("user-note"), b"private user data").unwrap();
        migrate_fixture(data.path(), selected, &lock).unwrap();
        assert_eq!(
            fs::read(target.join("user-note")).unwrap(),
            b"private user data"
        );
        validate(&origin, &lock).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn concurrent_legacy_writer_is_busy_and_release_allows_a_verified_copy() {
        use std::os::windows::fs::OpenOptionsExt;
        use windows::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};
        let data = tempfile::tempdir().unwrap();
        let (selected, lock, origin) = fixture(data.path());
        let _guard = crate::capability_process::try_acquire(
            data.path(),
            selected.id,
            crate::capability_process::Mode::Exclusive,
        )
        .unwrap()
        .unwrap();
        let first = lock["files"][0]["path"].as_str().unwrap();
        let original = fs::read(origin.join(first)).unwrap();
        let writer = fs::OpenOptions::new()
            .write(true)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
            .open(origin.join(first))
            .unwrap();
        let error = migrate_fixture(data.path(), selected, &lock).unwrap_err();
        assert_eq!(
            (error.status, error.code.as_str()),
            (409, "capability_install_busy")
        );
        assert!(!root_for(data.path(), selected)
            .join(selected.version)
            .exists());
        assert_eq!(fs::read(origin.join(first)).unwrap(), original);
        // Drop has a bounded cleanup budget and may leave its durable plan.
        // The next guarded access recovers it, even while the old writer lives.
        staging::recover(&root_for(data.path(), selected), &lock, &|| Ok(())).unwrap();
        assert!(!staging::pending(&root_for(data.path(), selected)).unwrap());
        assert!(needed(data.path(), selected).unwrap());
        drop(writer);
        migrate_fixture(data.path(), selected, &lock).unwrap();
        validate(&origin, &lock).unwrap();
        assert_eq!(fs::read(origin.join(first)).unwrap(), original);
        validate(
            &root_for(data.path(), selected).join(selected.version),
            &lock,
        )
        .unwrap();
    }
    #[test]
    fn damaged_legacy_status_keeps_missing_pack_flow_and_uninstall_opt_out() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("profile");
        let assets = root.path().join("assets");
        let docs = root.path().join("documents");
        let (selected, lock, origin) = fixture(&data);
        fs::create_dir_all(assets.join("capabilities")).unwrap();
        fs::create_dir(&docs).unwrap();
        fs::write(
            assets.join(format!(
                "capabilities/fonts-{}-universal.lock.json",
                selected.version
            )),
            serde_json::to_vec(&lock).unwrap(),
        )
        .unwrap();
        let first = lock["files"][0]["path"].as_str().unwrap();
        fs::write(origin.join(first), b"corrupt legacy input").unwrap();
        let app = Arc::new(
            App::bootstrap(crate::paths::AppPaths::with_dirs(&data, &docs, &assets)).unwrap(),
        );
        let status = operate(
            &app,
            &json!({"action":"status", "id":"fonts"}),
            None,
            &InstallHooks::direct(),
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&status.body).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["installed"], false);
        assert_eq!(value["ready"], false);
        assert_eq!(value["legacy_migration_error"], "capability_pack_invalid");
        assert!(!root_for(&data, selected).join(selected.version).exists());
        assert_eq!(
            fs::read(origin.join(first)).unwrap(),
            b"corrupt legacy input"
        );
        operate(
            &app,
            &json!({"action":"uninstall", "id":"fonts"}),
            None,
            &InstallHooks::direct(),
        )
        .unwrap();
        assert!(!needed(&data, selected).unwrap());
        let status = operate(
            &app,
            &json!({"action":"status", "id":"fonts"}),
            None,
            &InstallHooks::direct(),
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&status.body).unwrap();
        assert!(value["legacy_migration_error"].is_null());
    }
    #[cfg(windows)]
    #[test]
    fn legacy_junction_is_preserved_and_never_imported() {
        use std::os::windows::process::CommandExt;
        let data = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let selected = spec("fonts").unwrap();
        let lock = super::super::tests::fixture(outside.path());
        let origin = source(data.path(), selected);
        fs::create_dir_all(origin.parent().unwrap()).unwrap();
        let output = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&origin)
            .arg(outside.path())
            .creation_flags(0x08000000)
            .output()
            .unwrap();
        assert!(output.status.success(), "Native junction fixture failed");
        assert_eq!(
            migrate_fixture(data.path(), selected, &lock)
                .unwrap_err()
                .code,
            "capability_pack_invalid"
        );
        assert!(!root_for(data.path(), selected)
            .join(selected.version)
            .exists());
        validate(outside.path(), &lock).unwrap();
        assert!(origin.exists());
    }
}
