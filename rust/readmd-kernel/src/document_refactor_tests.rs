use super::*;

pub(super) fn fixture() -> (tempfile::TempDir, Arc<App>, Value) {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("documents");
    let data = temp.path().join("profile");
    let assets = temp.path().join("assets");
    for path in [&workspace, &data, &assets] {
        fs::create_dir_all(path).unwrap();
    }
    let source = workspace.join("chapter.md");
    fs::write(&source, "# Chapter\r\n[Self](chapter.md#one)\r\n").unwrap();
    fs::write(
        workspace.join("index.md"),
        "# Index\n[Chapter](chapter.md?print=1#one)\n`[code](chapter.md)`\n",
    )
    .unwrap();
    fs::create_dir(workspace.join("notes")).unwrap();
    fs::write(
        workspace.join("notes/reading.md"),
        crate::text_encoding::encode(
            "[阅读][c]\r\n\r\n[c]: ../chapter.md \"原始标题\"\r\n",
            "utf-16-le",
        )
        .unwrap(),
    )
    .unwrap();
    let app =
        Arc::new(App::bootstrap(paths::AppPaths::with_dirs(&data, &workspace, &assets)).unwrap());
    authorize(&app, &source);
    let request = json!({"action":"preview","operation_id":format!("refactor-{}",uuid::Uuid::new_v4()),
        "path":source,"new_stem":"renamed","workspace_root":workspace});
    (temp, app, request)
}
pub(super) fn authorize(app: &Arc<App>, path: &Path) {
    let url = format!(
        "/api/file?p={}",
        percent_encoding::utf8_percent_encode(
            path.to_str().unwrap(),
            percent_encoding::NON_ALPHANUMERIC
        )
    );
    assert_eq!(server::call_in_process(app, "GET", &url, None).0, 200);
}
pub(super) fn confirm(request: &Value, plan: &Value) -> Value {
    let mut req = request.clone();
    req["action"] = json!("commit");
    req["confirmed"] = json!(true);
    req["plan_hash"] = plan["plan_hash"].clone();
    req
}

#[test]
fn preview_is_read_only_and_preserves_link_syntax_and_original_bytes() {
    let (_temp, app, req) = fixture();
    let plan = build_plan(&app, &req).unwrap();
    assert_eq!(plan["files"].as_array().unwrap().len(), 3);
    assert!(plan["diagnostics"].as_array().unwrap().is_empty());
    assert!(Path::new(req["path"].as_str().unwrap()).exists());
    assert!(
        !app.paths.data_dir.join("review.sqlite3").exists(),
        "preview of no sessions creates no review database"
    );
    let entry = plan["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["source_path"].as_str().unwrap().ends_with("index.md"))
        .unwrap();
    assert_eq!(
        entry["after"],
        "# Index\n[Chapter](renamed.md?print=1#one)\n`[code](chapter.md)`\n"
    );
    for file in plan["files"].as_array().unwrap() {
        assert!(!Path::new(file["stage_path"].as_str().unwrap()).exists());
    }
}

#[test]
fn preview_requires_open_source_and_explicit_containing_workspace() {
    let (_temp, app, mut req) = fixture();
    let unopened = app.paths.workspace.join("unopened.md");
    fs::write(&unopened, "text").unwrap();
    req["path"] = json!(unopened);
    assert_eq!(
        build_plan(&app, &req).unwrap_err().code,
        "review_not_authorized"
    );
    req["path"] = json!(app.paths.workspace.join("chapter.md"));
    req.as_object_mut().unwrap().remove("workspace_root");
    assert!(build_plan(&app, &req).is_err());
}

#[cfg(windows)]
#[test]
fn complete_group_commit_preserves_encoding_review_identity_and_replays_after_restart() {
    let (temp, app, req) = fixture();
    let plan = build_plan(&app, &req).unwrap();
    let initial=app.review.handle(&json!({"action":"open","path":req["path"],"content":"# Chapter\n[Self](chapter.md#one)\n",
        "disk_content":"# Chapter\n[Self](chapter.md#one)\n","disk_revision":plan["files"].as_array().unwrap().iter().find(|f|f["source_path"]==req["path"]).unwrap()["before_disk_revision"],"encoding":"utf-8","line_ending":"crlf"})).unwrap();
    let plan = build_plan(&app, &req).unwrap();
    let commit = confirm(&req, &plan);
    let result = execute(&app, &commit).unwrap();
    assert_eq!(result["state"], "committed");
    assert!(!app.paths.workspace.join("chapter.md").exists());
    assert_eq!(
        fs::read(app.paths.workspace.join("renamed.md")).unwrap(),
        b"# Chapter\r\n[Self](renamed.md#one)\r\n"
    );
    let original = plan["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["source_path"] == req["path"])
        .unwrap();
    let receipt = result["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["old_path"] == req["path"])
        .unwrap();
    assert_eq!(receipt["review"]["document_id"], initial["document_id"]);
    assert_eq!(original["encoding"], "utf-8");
    for file in plan["files"].as_array().unwrap() {
        assert_eq!(
            fs::read(Path::new(file["target_path"].as_str().unwrap())).unwrap(),
            crate::text_encoding::encode(
                file["after"].as_str().unwrap(),
                file["encoding"].as_str().unwrap()
            )
            .unwrap()
        );
        for key in ["stage_path", "backup_path"] {
            assert!(!Path::new(file[key].as_str().unwrap()).exists());
        }
    }
    let replay = execute(&app, &commit).unwrap();
    assert_eq!(replay, result);
    drop(app);
    let workspace = temp.path().join("documents");
    let data = temp.path().join("profile");
    let assets = temp.path().join("assets");
    let restarted =
        Arc::new(App::bootstrap(paths::AppPaths::with_dirs(&data, &workspace, &assets)).unwrap());
    assert_eq!(execute(&restarted, &commit).unwrap(), result);
    let mut incompatible = commit;
    incompatible["new_stem"] = json!("other");
    assert_eq!(
        execute(&restarted, &incompatible).unwrap_err().code,
        "review_call_conflict"
    );
}

#[cfg(windows)]
#[test]
fn changed_scan_membership_or_bytes_rejects_before_any_file_move() {
    let (_temp, app, req) = fixture();
    let plan = build_plan(&app, &req).unwrap();
    let commit = confirm(&req, &plan);
    let extra = app.paths.workspace.join("new-reference.md");
    fs::write(&extra, "[New](chapter.md)\n").unwrap();
    assert_eq!(execute(&app, &commit).unwrap_err().code, "review_conflict");
    assert!(app.paths.workspace.join("chapter.md").exists());
    assert!(!app.paths.workspace.join("renamed.md").exists());
    fs::remove_file(extra).unwrap();
    fs::write(app.paths.workspace.join("index.md"), "externally modified").unwrap();
    assert_eq!(execute(&app, &commit).unwrap_err().code, "review_conflict");
    assert!(app
        .review
        .refactor_record(req["operation_id"].as_str().unwrap())
        .unwrap()
        .is_none());
}

#[cfg(windows)]
#[test]
fn locked_reference_fails_admission_without_side_effects() {
    use std::os::windows::fs::OpenOptionsExt;
    let (_temp, app, req) = fixture();
    let plan = build_plan(&app, &req).unwrap();
    let locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(app.paths.workspace.join("index.md"))
        .unwrap();
    assert!(execute(&app, &confirm(&req, &plan)).is_err());
    assert!(app.paths.workspace.join("chapter.md").exists());
    assert!(app
        .review
        .refactor_record(req["operation_id"].as_str().unwrap())
        .unwrap()
        .is_none());
    drop(locked);
}

#[test]
fn directory_escape_private_destination_and_broken_encoding_never_get_silently_rewritten() {
    let (_temp, app, req) = fixture();
    fs::write(
        app.paths.workspace.join("index.md"),
        "[absolute](/chapter.md)\n",
    )
    .unwrap();
    let plan = build_plan(&app, &req).unwrap();
    assert!(!plan["diagnostics"].as_array().unwrap().is_empty());
    fs::write(
        app.paths.workspace.join("index.md"),
        [0xef, 0xbb, 0xbf, 0xff],
    )
    .unwrap();
    assert_eq!(
        build_plan(&app, &req).unwrap_err().code,
        "rename_unsupported_file"
    );
}
