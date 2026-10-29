use super::*;

fn app(root: &Path) -> Arc<App> {
    let assets = root.join("assets");
    std::fs::create_dir_all(&assets).unwrap();
    Arc::new(
        App::bootstrap(crate::paths::AppPaths::with_dirs(
            &root.join("data"),
            root,
            &assets,
        ))
        .unwrap(),
    )
}
fn api(app: &Arc<App>, route: &str, body: &Value) -> (u16, Value) {
    crate::server::call_in_process(app, "POST", route, Some(body))
}
fn open(app: &Arc<App>, file: &Path) -> Value {
    let (status, _) = crate::server::call_in_process(
        app,
        "GET",
        &format!(
            "/api/file?p={}",
            percent_encoding::utf8_percent_encode(
                &file.to_string_lossy(),
                percent_encoding::NON_ALPHANUMERIC
            )
        ),
        None,
    );
    assert_eq!(status, 200);
    let (status, value) = api(
        app,
        "/api/review",
        &json!({"action":"open","path":file.to_string_lossy()}),
    );
    assert_eq!(status, 200, "{}", value);
    value
}
fn mutate(app: &Arc<App>, state: &Value, call: &str, action: &str, extra: Value) -> Value {
    let mut body = json!({"action":action,"document_id":state["document_id"],"generation":state["generation"],
        "review_revision":state["review_revision"],"call_id":call});
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let (status, next) = api(app, "/api/review", &body);
    assert_eq!(status, 200, "{}", next);
    next
}
fn source_edit(state: &Value, needle: &str, replacement: &str, offset: usize) -> Value {
    let content = state["content"].as_str().unwrap();
    let byte = content.match_indices(needle).nth(offset).unwrap().0;
    let from = content[..byte].encode_utf16().count();
    json!({"from":from,"to":from+needle.encode_utf16().count(),"insert":replacement})
}
fn save_body(file: &Path, state: &Value) -> Value {
    json!({"path":file.to_string_lossy(),"content":state["content"].as_str().unwrap().replace('\n',"\r\n"),
        "encoding":"utf-8-sig","review_session":{"document_id":state["document_id"],
            "generation":state["generation"],"review_revision":state["review_revision"],"call_id":uuid::Uuid::new_v4().to_string()}})
}

#[test]
fn durable_api_saves_reopens_and_withdraws_only_selected_duplicate_block() {
    let root = tempfile::tempdir().unwrap();
    let runtime = app(root.path());
    let file = root.path().join("Review.md");
    let original = "😀\r\n\r\nRepeat sentence.\r\n\r\nRepeat sentence.\r\n\r\nC original.\r\n";
    let mut bytes = vec![0xef, 0xbb, 0xbf];
    bytes.extend_from_slice(original.as_bytes());
    std::fs::write(&file, &bytes).unwrap();
    let first = open(&runtime, &file);
    assert_eq!(first["line_ending"], "crlf");
    assert!(!first["content"].as_str().unwrap().contains('\r'));
    let edits = json!([
        source_edit(&first, "Repeat sentence.", "AI corrected sentence.", 0),
        source_edit(&first, "Repeat sentence.", "AI corrected sentence.", 1)
    ]);
    let proposed = mutate(
        &runtime,
        &first,
        "proposal",
        "propose",
        json!({"edits":edits,"origin":"ai"}),
    );
    let ids = proposed["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].clone())
        .collect::<Vec<_>>();
    assert_ne!(ids[0], ids[1]);
    assert_eq!(std::fs::read(&file).unwrap(), bytes);
    let edited = mutate(
        &runtime,
        &proposed,
        "hand-c",
        "edit",
        json!({"edits":[source_edit(&proposed,"C original.","C manually edited.",0)],"origin":"editor"}),
    );
    let accepted = mutate(
        &runtime,
        &edited,
        "accept-a",
        "resolve",
        json!({"ids":[ids[0]],"decision":"accept"}),
    );
    let (status, saved) = api(&runtime, "/api/save", &save_body(&file, &accepted));
    assert_eq!(status, 200, "{}", saved);
    assert_eq!(saved["review"]["changes"][0]["decision"], "accepted");
    assert_eq!(saved["review"]["changes"][1]["decision"], "pending");
    let on_disk = std::fs::read(&file).unwrap();
    assert!(on_disk.starts_with(&[0xef, 0xbb, 0xbf]));
    assert_eq!(std::str::from_utf8(&on_disk[3..]).unwrap(),"😀\r\n\r\nAI corrected sentence.\r\n\r\nAI corrected sentence.\r\n\r\nC manually edited.\r\n");
    drop(runtime);
    let restarted = app(root.path());
    let reopened = open(&restarted, &file);
    assert_eq!(reopened["document_id"], first["document_id"]);
    assert_eq!(reopened["changes"].as_array().unwrap().len(), 2);
    let withdrawn = mutate(
        &restarted,
        &reopened,
        "withdraw-b",
        "resolve",
        json!({"ids":[ids[1]],"decision":"withdraw"}),
    );
    assert_eq!(
        withdrawn["content"],
        "😀\n\nAI corrected sentence.\n\nRepeat sentence.\n\nC manually edited.\n"
    );
    let undo = mutate(&restarted, &withdrawn, "undo-withdraw", "undo", json!({}));
    assert_eq!(undo["content"], reopened["content"]);
    let redo = mutate(&restarted, &undo, "redo-withdraw", "redo", json!({}));
    assert_eq!(redo["content"], withdrawn["content"]);
    let (status, saved_again) = api(&restarted, "/api/save", &save_body(&file, &redo));
    assert_eq!(status, 200, "{}", saved_again);
    let reopened_again = open(&restarted, &file);
    let accepted_withdrawn = mutate(
        &restarted,
        &reopened_again,
        "withdraw-accepted-a",
        "resolve",
        json!({"ids":[ids[0]],"decision":"withdraw"}),
    );
    assert_eq!(
        accepted_withdrawn["content"],
        "😀\n\nRepeat sentence.\n\nRepeat sentence.\n\nC manually edited.\n"
    );
}

#[test]
fn durable_api_authority_and_legacy_save_cannot_bypass_session() {
    let root = tempfile::tempdir().unwrap();
    let runtime = app(root.path());
    let file = root.path().join("Authority.md");
    std::fs::write(&file, "Original.\n").unwrap();
    // Bootstrap authorizes its workspace generally for reading, but review
    // mutation must be preceded by opening this exact file in the service.
    let other = root.path().join("Not-opened.md");
    std::fs::write(&other, "Private context").unwrap();
    let (status, _) = api(
        &runtime,
        "/api/review",
        &json!({"action":"open","path":other.to_string_lossy()}),
    );
    assert_eq!(status, 403);
    let first = open(&runtime, &file);
    let edit = source_edit(&first, "Original.", "Reviewed.", 0);
    let proposed = mutate(
        &runtime,
        &first,
        "once",
        "propose",
        json!({"edits":[edit],"origin":"ai"}),
    );
    let duplicate = mutate(
        &runtime,
        &first,
        "once",
        "propose",
        json!({"edits":[edit],"origin":"ai"}),
    );
    assert_eq!(duplicate, proposed);
    let (status, rejected) = api(
        &runtime,
        "/api/review",
        &json!({"action":"propose","document_id":first["document_id"],"generation":first["generation"],"review_revision":first["review_revision"],"call_id":"once","edits":[{"from":0,"to":1,"insert":"Other"}],"origin":"ai"}),
    );
    assert_eq!(status, 409);
    assert_eq!(rejected["error_code"], "review_call_conflict");
    for action in [
        "prepare_save",
        "commit_save",
        "abort_save",
        "pins",
        "lookup",
        "receipt",
    ] {
        let (status, _) = api(
            &runtime,
            "/api/review",
            &json!({"action":action,"document_id":first["document_id"]}),
        );
        assert_eq!(status, 422, "{action}");
    }
    let (status, denied) = api(
        &runtime,
        "/api/save",
        &json!({"path":file.to_string_lossy(),"content":"Bypass","encoding":"utf-8"}),
    );
    assert_eq!(status, 409);
    assert_eq!(denied["error_code"], "review_save_conflict");
    assert_eq!(std::fs::read_to_string(file).unwrap(), "Original.\n");
}

#[test]
fn durable_api_refuses_external_overwrite_and_does_not_trust_discard_disk_fields() {
    let root = tempfile::tempdir().unwrap();
    let runtime = app(root.path());
    let file = root.path().join("External.md");
    std::fs::write(&file, "Original.\n").unwrap();
    let first = open(&runtime, &file);
    let edited = mutate(
        &runtime,
        &first,
        "edit",
        "edit",
        json!({"edits":[source_edit(&first,"Original.","Unsaved draft.",0)],"origin":"editor"}),
    );
    std::fs::write(&file, "External author.\n").unwrap();
    let (status, refused) = api(&runtime, "/api/save", &save_body(&file, &edited));
    assert_eq!(status, 409, "{}", refused);
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "External author.\n"
    );
    let (status, _) = api(
        &runtime,
        "/api/review",
        &json!({"action":"discard","document_id":edited["document_id"],"generation":edited["generation"],"review_revision":edited["review_revision"],"call_id":"spoof-disk","disk_revision":first["disk_revision"],"disk_content":"Original.\n"}),
    );
    assert_eq!(status, 409);
    let (status, retained) = api(
        &runtime,
        "/api/review",
        &json!({"action":"read","document_id":first["document_id"]}),
    );
    assert_eq!(status, 200);
    assert_eq!(retained["content"], "Unsaved draft.\n");
}

#[test]
fn durable_api_draft_discard_and_encoding_failure_keep_file_and_session_safe() {
    let root = tempfile::tempdir().unwrap();
    let runtime = app(root.path());
    let (status, draft) = api(
        &runtime,
        "/api/review",
        &json!({"action":"open","path":"draft:test_unsaved","content":"Unsaved creation."}),
    );
    assert_eq!(status, 200);
    let discarded = mutate(&runtime, &draft, "discard-new", "discard", json!({}));
    assert_eq!(discarded["content"], "");
    let file = root.path().join("Encoding.md");
    std::fs::write(&file, "Original.\n").unwrap();
    let first = open(&runtime, &file);
    let edited = mutate(
        &runtime,
        &first,
        "emoji",
        "propose",
        json!({"edits":[source_edit(&first,"Original.","Emoji 😀.",0)],"origin":"ai"}),
    );
    let mut body = save_body(&file, &edited);
    body["encoding"] = json!("gbk");
    let (status, error) = api(&runtime, "/api/save", &body);
    assert_eq!(status, 422, "{}", error);
    assert_eq!(error["error_code"], "encoding_unrepresentable");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "Original.\n");
    let (status, state) = api(
        &runtime,
        "/api/review",
        &json!({"action":"read","document_id":first["document_id"]}),
    );
    assert_eq!(status, 200);
    assert_eq!(state["changes"][0]["decision"], "pending");
    assert!(state.get("save_intent").is_none_or(Value::is_null));
}

#[test]
fn durable_api_known_encoded_bytes_keep_requested_encoding_after_save() {
    let root = tempfile::tempdir().unwrap();
    let runtime = app(root.path());
    let file = root.path().join("Ascii-legacy.md");
    std::fs::write(&file, "Original.\n").unwrap();
    let first = open(&runtime, &file);
    let updated = mutate(
        &runtime,
        &first,
        "ascii-propose",
        "propose",
        json!({"edits":[source_edit(&first,"Original.","Updated.",0)],"origin":"ai"}),
    );
    let mut body = save_body(&file, &updated);
    body["encoding"] = json!("gb18030");
    let (status, result) = api(&runtime, "/api/save", &body);
    assert_eq!(status, 200, "{}", result);
    assert_eq!(std::fs::read(&file).unwrap(), b"Updated.\r\n");
    assert_eq!(result["review"]["encoding"], "gb18030");
    assert_eq!(result["review"]["changes"][0]["decision"], "pending");
    assert_eq!(result["review_edit_steps"], json!([]));
}

#[test]
fn durable_api_precise_asset_save_keeps_middle_review_and_rewrites_both_sides() {
    let root = tempfile::tempdir().unwrap();
    let runtime = app(root.path());
    let file = root.path().join("Assets.md");
    let raw = "![left](left.svg)\r\n\r\nMiddle original.\r\n\r\n![right](right.svg)\r\n";
    std::fs::write(&file, raw).unwrap();
    let left = root.path().join("left.svg");
    let right = root.path().join("right.svg");
    std::fs::write(&left,"<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'><rect width='10' height='10'/></svg>").unwrap();
    std::fs::write(&right,"<svg xmlns='http://www.w3.org/2000/svg' width='12' height='12'><circle cx='6' cy='6' r='4'/></svg>").unwrap();
    let left_before = std::fs::read(&left).unwrap();
    let initial = open(&runtime, &file);
    let proposed = mutate(
        &runtime,
        &initial,
        "asset-middle",
        "propose",
        json!({"edits":[source_edit(&initial,"Middle original.","Middle reviewed.",0)],"origin":"ai"}),
    );
    let mut body = save_body(&file, &proposed);
    body["assets"] = json!([
        {"path":left.to_string_lossy(),"name":"left.svg","reference":"left.svg"},
        {"path":right.to_string_lossy(),"name":"right.svg","reference":"right.svg"}]);
    let (status, saved) = api(&runtime, "/api/save", &body);
    assert_eq!(status, 200, "{}", saved);
    let text = saved["review"]["content"].as_str().unwrap();
    assert_eq!(text.matches("readmd-assets/").count(), 2);
    assert_eq!(
        saved["review_edit_steps"][0]["edits"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(saved["review"]["changes"][0]["decision"], "pending");
    assert_eq!(saved["review"]["changes"][0]["taken_over"], false);
    let restored = mutate(
        &runtime,
        &saved["review"],
        "asset-middle-withdraw",
        "resolve",
        json!({"ids":[saved["review"]["changes"][0]["id"]],"decision":"withdraw"}),
    );
    assert!(restored["content"]
        .as_str()
        .unwrap()
        .contains("Middle original."));
    assert_eq!(
        restored["content"]
            .as_str()
            .unwrap()
            .matches("readmd-assets/")
            .count(),
        2
    );
    assert!(runtime
        .review
        .pinned_assets()
        .unwrap()
        .iter()
        .any(|path| path.contains("readmd-assets")));
    assert_eq!(std::fs::read(&left).unwrap(), left_before);
}

#[test]
fn durable_api_moving_hidden_review_image_refuses_before_file_or_resource_commit() {
    let root = tempfile::tempdir().unwrap();
    let runtime = app(root.path());
    let source = root.path().join("source");
    let dest = root.path().join("dest");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&dest).unwrap();
    let file = source.join("Original.md");
    std::fs::write(&file, "![original](illustration.svg)\n").unwrap();
    std::fs::write(
        source.join("illustration.svg"),
        "<svg xmlns='http://www.w3.org/2000/svg'/>",
    )
    .unwrap();
    let first = open(&runtime, &file);
    let proposed = mutate(
        &runtime,
        &first,
        "remove-image",
        "propose",
        json!({"edits":[source_edit(&first,"![original](illustration.svg)","Reviewed plain prose.",0)],"origin":"ai"}),
    );
    let target = dest.join("Moved.md");
    std::fs::write(&target, "Old target.\n").unwrap();
    let _ = open(&runtime, &target);
    let (status, error) = api(&runtime, "/api/save", &save_body(&target, &proposed));
    assert_eq!(status, 409);
    assert_eq!(error["error_code"], "review_asset_relocation_required");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "Old target.\n");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "![original](illustration.svg)\n"
    );
    assert!(!dest.join("readmd-assets").exists());
    let (status, current) = api(
        &runtime,
        "/api/review",
        &json!({"action":"read","document_id":first["document_id"]}),
    );
    assert_eq!(status, 200);
    assert_eq!(current["changes"][0]["decision"], "pending");
    assert!(current["save_intent"].is_null());
}

#[cfg(windows)]
#[test]
fn durable_api_real_windows_exclusive_file_lock_preserves_original_and_draft() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = tempfile::tempdir().unwrap();
    let runtime = app(root.path());
    let file = root.path().join("Locked.md");
    std::fs::write(&file, "Original.\n").unwrap();
    let first = open(&runtime, &file);
    let updated = mutate(
        &runtime,
        &first,
        "locked-propose",
        "propose",
        json!({"edits":[source_edit(&first,"Original.","Updated.",0)],"origin":"ai"}),
    );
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&file)
        .unwrap();
    let (status, result) = api(&runtime, "/api/save", &save_body(&file, &updated));
    assert_eq!(status, 409, "{}", result);
    drop(lock);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "Original.\n");
    let (status, current) = api(
        &runtime,
        "/api/review",
        &json!({"action":"read","document_id":first["document_id"]}),
    );
    assert_eq!(status, 200);
    assert_eq!(current["content"], "Updated.\n");
    assert_eq!(current["changes"][0]["decision"], "pending");
    assert!(current["save_intent"].is_null());
}
