//! Recovery authority comes from application assets, never a manifest found
//! inside a writable runtime, stage or journal. Historical locks authorize
//! recovery only; installation/execution still use the current resource lock.
use super::*;
use serde::Deserialize;

pub(super) struct Trusted<'a> {
    pub current: &'a Value,
    current_fingerprint: String,
    previous: BTreeMap<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn register(assets: &Path, old: &Value) {
        let fingerprint = transaction::fingerprint(old).unwrap();
        fs::create_dir_all(assets.join("capabilities/history")).unwrap();
        fs::write(
            assets.join(format!("capabilities/history/{fingerprint}.lock.json")),
            serde_json::to_vec(old).unwrap(),
        )
        .unwrap();
        fs::write(assets.join("capabilities/recovery-locks.json"),serde_json::to_vec(&json!({"schema":1,"manifests":[{"id":old["id"],"platform":old["platform"],"fingerprint":fingerprint}]})).unwrap()).unwrap();
    }
    fn setup(root: &Path) -> (PathBuf, PathBuf, PathBuf, Value, Value) {
        let data = root.join("profile");
        fs::create_dir(&data).unwrap();
        let selected = spec("fonts").unwrap();
        let parent = managed_parent(&data, selected, true).unwrap();
        let current = super::super::tests::fixture(&parent.join(selected.version));
        let mut old = super::super::tests::fixture(&parent.join("2026.10.09"));
        old["version"] = json!("2026.10.09");
        let assets = root.join("assets");
        register(&assets, &old);
        (data, assets, parent, current, old)
    }
    fn removal(parent: &Path, lock: &Value, committed: bool) -> PathBuf {
        let nonce = uuid::Uuid::new_v4().to_string();
        let directory = parent.join(format!("remove-txn-{nonce}"));
        fs::create_dir(&directory).unwrap();
        let source = lock["version"].as_str().unwrap();
        let files = lock["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["path"].as_str().unwrap())
            .collect::<Vec<_>>();
        for name in files.iter().take(if committed { files.len() } else { 1 }) {
            let output = transaction::file_at(&directory, name, true)
                .unwrap()
                .unwrap();
            transaction::move_checked(&parent.join(source).join(name), &output).unwrap();
        }
        fs::write(directory.join("removal-v1.json"),serde_json::to_vec(&json!({"schema":1,"owner":"ReadMD capability removal v1","nonce":nonce,"manifest":transaction::fingerprint(lock).unwrap(),"source":source,"files":files,"phase":if committed{"committed"}else{"prepared"}})).unwrap()).unwrap();
        directory
    }
    #[test]
    fn mixed_current_and_historical_removals_restore_or_finish_without_changing_current_files() {
        for committed in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let (data, assets, parent, current, old) = setup(root.path());
            let old_directory = removal(&parent, &old, committed);
            fs::write(
                old_directory.join("keep-extra"),
                b"preserve historic user data",
            )
            .unwrap();
            let current_directory = removal(&parent, &current, false);
            recover_locked(&data, &assets, spec("fonts").unwrap(), &current, &|| Ok(())).unwrap();
            validate(&parent.join(current["version"].as_str().unwrap()), &current).unwrap();
            if committed {
                assert!(!parent.join("2026.10.09/bin/engine").exists());
            } else {
                validate(&parent.join("2026.10.09"), &old).unwrap();
            }
            assert!(!old_directory.join("removal-v1.json").exists());
            assert_eq!(
                fs::read(old_directory.join("keep-extra")).unwrap(),
                b"preserve historic user data"
            );
            assert!(!current_directory.join("removal-v1.json").exists());
            assert!(
                !transactions_blocking(&parent, &assets, spec("fonts").unwrap(), &current).unwrap()
            );
        }
    }
    #[test]
    fn historical_replacement_and_staging_recover_rollback_and_valid_candidate_forward() {
        for forward in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let (data, assets, parent, current, old) = setup(root.path());
            let nonce = uuid::Uuid::new_v4().to_string();
            let previous = parent.join(format!("replaced-{nonce}"));
            let target = parent.join("2026.10.09");
            fs::write(target.join("keep-extra"), b"original prior version extra").unwrap();
            transaction::move_checked(&target, &previous).unwrap();
            let stage = staging::Stage::create(&parent, &old).unwrap();
            super::super::tests::fixture(stage.path());
            let stage_name = stage
                .path()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();
            if forward {
                transaction::move_checked(stage.path(), &target).unwrap();
            }
            std::mem::forget(stage);
            let directory = parent.join(format!("install-txn-{nonce}"));
            fs::create_dir(&directory).unwrap();
            fs::write(directory.join("replacement-v1.json"),serde_json::to_vec(&json!({"schema":1,"owner":"ReadMD capability replacement v1","nonce":nonce,"manifest":transaction::fingerprint(&old).unwrap(),"target":"2026.10.09","staging":stage_name,"previous":previous.file_name().unwrap().to_str().unwrap(),"had_previous":true,"phase":"prepared"})).unwrap()).unwrap();
            recover_locked(&data, &assets, spec("fonts").unwrap(), &current, &|| Ok(())).unwrap();
            validate(&parent.join(current["version"].as_str().unwrap()), &current).unwrap();
            validate(&target, &old).unwrap();
            assert!(!directory.join("replacement-v1.json").exists());
            assert!(!staging::pending(&parent).unwrap());
            assert!(!parent.join(stage_name).exists());
            let original = if forward { previous } else { target };
            assert_eq!(
                fs::read(original.join("keep-extra")).unwrap(),
                b"original prior version extra"
            );
        }
    }
    #[test]
    fn unlisted_or_tampered_historical_manifests_cannot_authorize_recovery() {
        for unlisted in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let (data, assets, parent, current, old) = setup(root.path());
            let directory = removal(&parent, &old, false);
            let bytes = fs::read(directory.join("bin/engine")).unwrap();
            if unlisted {
                fs::remove_file(assets.join("capabilities/recovery-locks.json")).unwrap();
            } else {
                let fingerprint = transaction::fingerprint(&old).unwrap();
                let mut changed = old.clone();
                changed["files"][0]["path"] = json!("user-private-file");
                fs::write(
                    assets.join(format!("capabilities/history/{fingerprint}.lock.json")),
                    serde_json::to_vec(&changed).unwrap(),
                )
                .unwrap();
            }
            assert!(
                recover_locked(&data, &assets, spec("fonts").unwrap(), &current, &|| Ok(()))
                    .is_err()
            );
            assert_eq!(fs::read(directory.join("bin/engine")).unwrap(), bytes);
            assert!(directory.join("removal-v1.json").is_file());
            validate(&parent.join(current["version"].as_str().unwrap()), &current).unwrap();
        }
    }
    #[test]
    fn historical_rollback_conflict_preserves_both_originals_without_blocking_other_current_version(
    ) {
        let root = tempfile::tempdir().unwrap();
        let (data, assets, parent, current, old) = setup(root.path());
        let directory = removal(&parent, &old, false);
        fs::write(
            parent.join("2026.10.09/bin/engine"),
            b"user later edited old engine path",
        )
        .unwrap();
        recover_locked(&data, &assets, spec("fonts").unwrap(), &current, &|| Ok(())).unwrap();
        assert_eq!(
            fs::read(parent.join("2026.10.09/bin/engine")).unwrap(),
            b"user later edited old engine path"
        );
        assert_eq!(
            fs::read(directory.join("bin/engine")).unwrap(),
            b"reviewed bin/engine"
        );
        assert!(directory.join("removal-v1.json").is_file());
        assert!(
            !transactions_blocking(&parent, &assets, spec("fonts").unwrap(), &current).unwrap()
        );
        assert_eq!(
            recover_locked(&data, &assets, spec("fonts").unwrap(), &current, &|| Err(
                ApiError::new(422, "cancelled")
            ))
            .unwrap_err()
            .code,
            "cancelled"
        );
        validate(&parent.join(current["version"].as_str().unwrap()), &current).unwrap();
    }
    #[test]
    fn catalog_is_bounded_and_does_not_trust_a_version_or_path_from_a_journal() {
        let root = tempfile::tempdir().unwrap();
        let (_, assets, _, current, old) = setup(root.path());
        let fingerprint = transaction::fingerprint(&old).unwrap();
        for descriptor in [
            json!({"id":"fonts","platform":"universal","fingerprint":"../escape"}),
            json!({"id":"other","platform":"universal","fingerprint":fingerprint}),
            json!({"id":"fonts","platform":"universal","fingerprint":fingerprint,"path":"../escape"}),
        ] {
            fs::write(
                assets.join("capabilities/recovery-locks.json"),
                serde_json::to_vec(&json!({"schema":1,"manifests":[descriptor]})).unwrap(),
            )
            .unwrap();
            assert!(Trusted::load(&assets, spec("fonts").unwrap(), &current).is_err());
        }
        let entry = json!({"id":"fonts","platform":"universal","fingerprint":fingerprint});
        for entries in [vec![entry.clone(), entry.clone()], vec![entry; 33]] {
            fs::write(
                assets.join("capabilities/recovery-locks.json"),
                serde_json::to_vec(&json!({"schema":1,"manifests":entries})).unwrap(),
            )
            .unwrap();
            assert!(Trusted::load(&assets, spec("fonts").unwrap(), &current).is_err());
        }
    }
    #[test]
    fn no_catalog_does_not_create_assets_or_grant_untrusted_manifest_authority() {
        let root = tempfile::tempdir().unwrap();
        let current = super::super::tests::fixture(&root.path().join("runtime"));
        let assets = root.path().join("absent-assets");
        let trusted = Trusted::load(&assets, spec("fonts").unwrap(), &current).unwrap();
        assert!(!assets.exists());
        let directory = root.path().join("journal");
        fs::create_dir(&directory).unwrap();
        fs::write(
            directory.join("staging-v1.json"),
            b"{\"manifest\":\"untrusted\",\"files\":[\"../private\"]}",
        )
        .unwrap();
        assert!(trusted.for_record(&directory, "staging-v1.json").is_err());
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    schema: u8,
    manifests: Vec<Entry>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    id: String,
    platform: String,
    fingerprint: String,
}
fn fingerprint_name(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn read(path: &Path, limit: u64) -> ApiResult<Value> {
    let (file, size) = resource_file(path)?;
    if size == 0 || size > limit {
        return Err(error());
    }
    let mut bytes = Vec::with_capacity(size as usize);
    file.take(size + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| error())?;
    if bytes.len() as u64 != size {
        return Err(error());
    }
    serde_json::from_slice(&bytes).map_err(|_| error())
}
impl<'a> Trusted<'a> {
    pub(super) fn only(current: &'a Value) -> ApiResult<Self> {
        Ok(Self {
            current,
            current_fingerprint: transaction::fingerprint(current)?,
            previous: BTreeMap::new(),
        })
    }
    pub(super) fn load(assets: &Path, selected: Spec, current: &'a Value) -> ApiResult<Self> {
        let mut trusted = Self::only(current)?;
        let root = assets.join("capabilities");
        let catalog = root.join("recovery-locks.json");
        if !transaction::exists(&catalog)? {
            return Ok(trusted);
        }
        real_directory(&root)?;
        let catalog: Catalog =
            serde_json::from_value(read(&catalog, 65536)?).map_err(|_| error())?;
        if catalog.schema != 1 || catalog.manifests.len() > 32 {
            return Err(error());
        }
        let mut entries = BTreeSet::new();
        for entry in catalog.manifests {
            if !matches!(
                entry.id.as_str(),
                "pdfium" | "typst" | "ocr" | "fonts" | "latex"
            ) || !matches!(
                entry.platform.as_str(),
                "universal"
                    | "windows-x64"
                    | "windows-arm64"
                    | "linux-x64"
                    | "linux-arm64"
                    | "macos-x64"
                    | "macos-arm64"
            ) || !fingerprint_name(&entry.fingerprint)
                || !entries.insert((
                    entry.id.clone(),
                    entry.platform.clone(),
                    entry.fingerprint.clone(),
                ))
            {
                return Err(error());
            }
            if entry.id != selected.id || entry.platform != selected.platform {
                continue;
            }
            if trusted.previous.len() >= 8 {
                return Err(error());
            }
            let directory = root.join("history");
            real_directory(&directory)?;
            let value = read(
                &directory.join(format!("{}.lock.json", entry.fingerprint)),
                crate::capability_archive::MAX_MANIFEST as u64,
            )?;
            let version = value["version"].as_str().ok_or_else(error)?;
            if !matches!(value["schema"].as_u64(), Some(1 | 2))
                || value["id"] != selected.id
                || value["platform"] != selected.platform
                || version.len() > 64
                || !crate::capability_archive::safe_name(version)
                || version.contains('/')
                || transaction::fingerprint(&value)? != entry.fingerprint
            {
                return Err(error());
            }
            checked_files(&value)?;
            if entry.fingerprint != trusted.current_fingerprint {
                trusted.previous.insert(entry.fingerprint, value);
            }
        }
        Ok(trusted)
    }
    pub(super) fn for_record(&self, directory: &Path, name: &str) -> ApiResult<&Value> {
        real_directory(directory)?;
        // This untrusted hint only selects an already trusted lock. The
        // original Plan::read then validates every field and ownership rule.
        let record = read(
            &directory.join(name),
            crate::capability_archive::MAX_MANIFEST as u64,
        )?;
        let fingerprint = record["manifest"].as_str().ok_or_else(error)?;
        if fingerprint == self.current_fingerprint {
            return Ok(self.current);
        }
        self.previous.get(fingerprint).ok_or_else(error)
    }
}
