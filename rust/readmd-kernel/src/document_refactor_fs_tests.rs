use super::*;

fn fixture() -> (tempfile::TempDir, Arc<App>, Value) {
    let (temp, app, req) = super::super::tests::fixture();
    let plan = build_plan(&app, &req).unwrap();
    (temp, app, plan)
}
fn locked<T>(app: &Arc<App>, f: impl FnOnce() -> T) -> T {
    let _save = crate::document_history::SAVE_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _gate = ops::acquire(&app.paths.data_dir, Duration::from_secs(3)).unwrap();
    f()
}
fn verify_originals(plan: &Value) {
    for file in plan["files"].as_array().unwrap() {
        assert_eq!(
            fs::read(Path::new(file["source_path"].as_str().unwrap())).unwrap(),
            crate::text_encoding::encode(
                file["before"].as_str().unwrap(),
                file["encoding"].as_str().unwrap()
            )
            .unwrap()
        );
        for key in ["stage_path", "backup_path"] {
            assert!(!Path::new(file[key].as_str().unwrap()).exists());
        }
    }
    assert!(!Path::new(plan["target_path"].as_str().unwrap()).exists());
}

#[test]
fn controlled_failure_after_each_publication_step_restores_all_exact_original_bytes() {
    for step in [0, 1, 2, 3, 4, 5, 6, usize::MAX] {
        let (_temp, app, plan) = fixture();
        let result = locked(&app, || {
            commit_with(&app, &plan, |actual| {
                if actual == step {
                    Err(review_api::error("document_operation_storage"))
                } else {
                    Ok(())
                }
            })
        })
        .unwrap();
        assert_eq!(result["state"], "rolled_back", "step={step}");
        verify_originals(&plan);
        for file in plan["files"].as_array().unwrap() {
            assert!(review_api::pending_rename(
                &app,
                Path::new(file["source_path"].as_str().unwrap())
            )
            .unwrap()
            .is_none());
        }
    }
}

#[test]
fn foreign_occupant_is_preserved_while_other_files_rollback_and_later_recovery_finishes() {
    let (_temp, app, plan) = fixture();
    let file = &plan["files"][0];
    let source = Path::new(file["source_path"].as_str().unwrap());
    assert_ne!(file["before_disk_revision"], file["after_disk_revision"]);
    let result = locked(&app, || {
        commit_with(&app, &plan, |step| {
            if step == 1 {
                fs::write(source, b"FOREIGN").unwrap();
                return Err(review_api::error("document_operation_target_exists"));
            }
            Ok(())
        })
    })
    .unwrap();
    assert_eq!(result["state"], "recovery_pending");
    assert_eq!(fs::read(source).unwrap(), b"FOREIGN");
    assert!(Path::new(file["backup_path"].as_str().unwrap()).exists());
    for other in plan["files"].as_array().unwrap().iter().skip(1) {
        assert_eq!(
            fs::read(Path::new(other["source_path"].as_str().unwrap())).unwrap(),
            crate::text_encoding::encode(
                other["before"].as_str().unwrap(),
                other["encoding"].as_str().unwrap()
            )
            .unwrap()
        );
    }
    fs::remove_file(source).unwrap();
    let recovered = locked(&app, || {
        recover_or_cleanup(
            &app,
            current(&app, plan["operation_id"].as_str().unwrap()).unwrap(),
        )
    })
    .unwrap();
    assert_eq!(recovered["state"], "rolled_back");
    verify_originals(&plan);
}

#[test]
fn restart_rolls_back_an_interrupted_group_using_registered_strong_identities() {
    let (temp, app, mut plan) = fixture();
    locked(&app, || {
        app.review.refactor_prepare(&plan).unwrap();
        let file = &plan["files"][0];
        let bytes = crate::text_encoding::encode(
            file["after"].as_str().unwrap(),
            file["encoding"].as_str().unwrap(),
        )
        .unwrap();
        let mut stage =
            WindowsLease::create_stage(Path::new(file["stage_path"].as_str().unwrap()), &bytes)
                .unwrap();
        plan["files"][0]["stage_identity"] = json!(stage.fingerprint().identity.as_str());
        app.review
            .refactor_update(
                plan["operation_id"].as_str().unwrap(),
                &json!({"plan":plan}),
            )
            .unwrap();
        let file = &plan["files"][0];
        let mut original =
            WindowsLease::open(Path::new(file["source_path"].as_str().unwrap())).unwrap();
        original
            .move_to(leaf(file["backup_path"].as_str().unwrap()).unwrap())
            .unwrap();
        stage
            .move_to(leaf(file["target_path"].as_str().unwrap()).unwrap())
            .unwrap();
    });
    drop(app);
    let app = Arc::new(
        App::bootstrap(paths::AppPaths::with_dirs(
            &temp.path().join("profile"),
            &temp.path().join("documents"),
            &temp.path().join("assets"),
        ))
        .unwrap(),
    );
    let record = current(&app, plan["operation_id"].as_str().unwrap()).unwrap();
    let result = locked(&app, || recover_or_cleanup(&app, record)).unwrap();
    assert_eq!(result["state"], "rolled_back");
    verify_originals(&plan);
}

// Invoked by the real subprocess test below; it intentionally exits without
// running Drop so all native file handles disappear at a process boundary.
#[test]
fn refactor_crash_child_helper() {
    let Some(root) = std::env::var_os("READMD_REFACTOR_CHILD_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let workspace = root.join("documents");
    let data = root.join("profile");
    let assets = root.join("assets");
    for path in [&workspace, &data, &assets] {
        fs::create_dir_all(path).unwrap();
    }
    let source = workspace.join("chapter.md");
    fs::write(&source, "# Chapter\r\n").unwrap();
    fs::write(workspace.join("index.md"), "[chapter](chapter.md#one)\r\n").unwrap();
    fs::write(workspace.join("notes.md"), "[chapter](chapter.md)\n").unwrap();
    let app =
        Arc::new(App::bootstrap(paths::AppPaths::with_dirs(&data, &workspace, &assets)).unwrap());
    super::super::tests::authorize(&app, &source);
    let mut plan = build_plan(
        &app,
        &json!({"action":"preview","operation_id":"refactor-crash-owned-child",
        "path":source,"new_stem":"renamed","workspace_root":workspace}),
    )
    .unwrap();
    locked(&app, || {
        app.review.refactor_prepare(&plan).unwrap();
        let changed = plan["files"]
            .as_array()
            .unwrap()
            .iter()
            .position(changed)
            .unwrap();
        let file = &plan["files"][changed];
        let bytes = crate::text_encoding::encode(
            file["after"].as_str().unwrap(),
            file["encoding"].as_str().unwrap(),
        )
        .unwrap();
        let mut stage =
            WindowsLease::create_stage(Path::new(file["stage_path"].as_str().unwrap()), &bytes)
                .unwrap();
        let mut original =
            WindowsLease::open(Path::new(file["source_path"].as_str().unwrap())).unwrap();
        original.verify_replacement(&stage).unwrap();
        plan["files"][changed]["stage_identity"] = json!(stage.fingerprint().identity.as_str());
        app.review
            .refactor_update("refactor-crash-owned-child", &json!({"plan":plan}))
            .unwrap();
        fs::write(
            root.join("owned-plan.json"),
            serde_json::to_vec(&plan).unwrap(),
        )
        .unwrap();
        let file = &plan["files"][changed];
        original
            .move_to(leaf(file["backup_path"].as_str().unwrap()).unwrap())
            .unwrap();
        stage
            .move_to(leaf(file["target_path"].as_str().unwrap()).unwrap())
            .unwrap();
        std::process::exit(77);
    });
}

#[test]
fn an_actually_exited_child_is_recovered_by_a_new_owner_without_any_authorization_set() {
    use std::process::{Command, Stdio};
    let root = tempfile::tempdir().unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap());
    child
        .args([
            "--exact",
            "document_refactor::filesystem::tests::refactor_crash_child_helper",
            "--test-threads=1",
        ])
        .env_clear()
        .env("READMD_REFACTOR_CHILD_ROOT", root.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for key in ["SYSTEMROOT", "WINDIR", "PATH", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(key) {
            child.env(key, value);
        }
    }
    assert_eq!(child.status().unwrap().code(), Some(77));
    let plan: Value =
        serde_json::from_slice(&fs::read(root.path().join("owned-plan.json")).unwrap()).unwrap();
    let app = Arc::new(
        App::bootstrap(paths::AppPaths::with_dirs(
            &root.path().join("profile"),
            &root.path().join("documents"),
            &root.path().join("assets"),
        ))
        .unwrap(),
    );
    let original = Path::new(plan["source_path"].as_str().unwrap());
    assert!(server::review_authorized_path(original.to_str().unwrap()).is_none());
    let record = current(&app, "refactor-crash-owned-child").unwrap();
    let result = locked(&app, || recover_or_cleanup(&app, record)).unwrap();
    assert_eq!(result["state"], "rolled_back");
    verify_originals(&plan);
}
