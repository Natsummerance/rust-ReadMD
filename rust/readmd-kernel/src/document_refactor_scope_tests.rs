use super::*;

fn verify_under_gate(app: &Arc<App>, plan: &Value) -> ApiResult<()> {
    let _save = crate::document_history::SAVE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let gate = ops::acquire(&app.paths.data_dir, Duration::from_secs(3)).map_err(native_error)?;
    gate.verify_profile(&app.paths.data_dir)
        .map_err(native_error)?;
    verify_manifest(app, plan)
}

#[test]
fn a_nonparticipant_changed_or_added_after_planning_fails_the_locked_manifest_check() {
    for existed_at_preview in [false, true] {
        let (_temp, app, request) = super::tests::fixture();
        let nonparticipant = app.paths.workspace.join("unrelated.md");
        if existed_at_preview {
            fs::write(&nonparticipant, "# No inbound link yet\n").unwrap();
        }
        let plan = build_plan(&app, &request).unwrap();
        assert!(plan["files"].as_array().unwrap().iter().all(|file| {
            file["source_path"].as_str().unwrap() != nonparticipant.to_str().unwrap()
        }));
        let originals: Vec<_> = plan["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|file| {
                let path = PathBuf::from(file["source_path"].as_str().unwrap());
                let bytes = fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect();

        // This represents the interval after a fresh pre-gate plan but before
        // publication admission. The locked verifier must use the full scan,
        // not just the files already selected as writable participants.
        fs::write(&nonparticipant, "[New inbound link](chapter.md)\n").unwrap();
        assert_eq!(
            verify_under_gate(&app, &plan).unwrap_err().code,
            "review_conflict"
        );
        for (path, bytes) in originals {
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
        assert!(!Path::new(plan["target_path"].as_str().unwrap()).exists());
        assert!(app
            .review
            .refactor_record(request["operation_id"].as_str().unwrap())
            .unwrap()
            .is_none());
        for file in plan["files"].as_array().unwrap() {
            for key in ["stage_path", "backup_path"] {
                assert!(!Path::new(file[key].as_str().unwrap()).exists());
            }
        }
    }
}

#[test]
fn unrelated_broken_links_with_missing_parents_do_not_abort_an_otherwise_valid_preview() {
    let (_temp, app, request) = super::tests::fixture();
    let broken = app.paths.workspace.join("broken-links.md");
    let bytes = b"[Missing note](missing-notes/no-note.md)\n![Missing image](missing-images/no-image.png)\n";
    fs::write(&broken, bytes).unwrap();
    let plan = build_plan(&app, &request).unwrap();
    assert!(plan["diagnostics"].as_array().unwrap().is_empty());
    assert!(plan["files"]
        .as_array()
        .unwrap()
        .iter()
        .all(|file| { file["source_path"].as_str().unwrap() != broken.to_str().unwrap() }));
    verify_under_gate(&app, &plan).unwrap();
    assert_eq!(fs::read(broken).unwrap(), bytes);
    assert!(!Path::new(plan["target_path"].as_str().unwrap()).exists());
}

#[cfg(windows)]
#[test]
fn an_excluded_managed_resource_link_is_not_opened_for_alias_auditing() {
    use std::os::windows::fs::OpenOptionsExt;
    let (_temp, app, request) = super::tests::fixture();
    let resources = app.paths.workspace.join("readmd-assets");
    fs::create_dir(&resources).unwrap();
    let resource = resources.join("locked.md");
    fs::write(&resource, b"synthetic opaque managed resource\n").unwrap();
    fs::write(
        app.paths.workspace.join("managed-link.md"),
        "[Opaque managed resource](readmd-assets/locked.md)\n",
    )
    .unwrap();
    let locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&resource)
        .unwrap();
    // If the scanner or physical-alias audit enters the excluded directory,
    // its attempted read/identity open fails with a sharing violation.
    let plan = build_plan(&app, &request).unwrap();
    assert!(plan["diagnostics"].as_array().unwrap().is_empty());
    verify_under_gate(&app, &plan).unwrap();
    assert!(plan["files"]
        .as_array()
        .unwrap()
        .iter()
        .all(|file| { !Path::new(file["source_path"].as_str().unwrap()).starts_with(&resources) }));
    drop(locked);
    assert_eq!(
        fs::read(resource).unwrap(),
        b"synthetic opaque managed resource\n"
    );
}
