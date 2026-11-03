//! Real save-copy service and files; native chooser acceptance is separate.
use super::*;
use crate::document_transaction as tx;

fn runtime(root: &Path) -> Arc<App> {
    let assets = root.join("app-assets"); std::fs::create_dir_all(&assets).unwrap();
    Arc::new(App::bootstrap(paths::AppPaths::with_dirs(&root.join("profile"), root, &assets)).unwrap())
}
fn open(app: &Arc<App>, file: &Path) -> Value {
    let encoded = percent_encoding::utf8_percent_encode(&file.to_string_lossy(), percent_encoding::NON_ALPHANUMERIC).to_string();
    let (status, _) = call_in_process(app, "GET", &format!("/api/file?p={encoded}"), None);
    assert_eq!(status, 200);
    let (status, value) = call_in_process(app, "POST", "/api/review", Some(&json!({"action":"open","path":file})));
    assert_eq!(status, 200, "{value}"); value
}
fn mutate(app: &Arc<App>, state: &Value, action: &str, extra: Value) -> Value {
    let mut body = json!({"action":action,"document_id":state["document_id"],"generation":state["generation"],
        "review_revision":state["review_revision"],"call_id":uuid::Uuid::new_v4().to_string()});
    body.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    let (status, value) = call_in_process(app, "POST", "/api/review", Some(&body));
    assert_eq!(status, 200, "{value}"); value
}
fn at(text: &str, needle: &str, occurrence: usize, insert: &str) -> Value {
    let start = text.match_indices(needle).nth(occurrence).unwrap().0;
    let from = text[..start].encode_utf16().count();
    json!({"from":from,"to":from+needle.encode_utf16().count(),"insert":insert})
}
fn body(file: &Path, state: &Value) -> Value {
    json!({"content":state["content"].as_str().unwrap().replace('\n',"\r\n"),"encoding":"utf-8-sig","base_dir":file.parent(),
        "review_session":{"document_id":state["document_id"],"generation":state["generation"],
        "review_revision":state["review_revision"],"call_id":uuid::Uuid::new_v4().to_string()}})
}
fn save(app: &Arc<App>, file: &Path, state: &Value, target: &Path) -> Value {
    let response = save_document_copy(app, &body(file, state), target.to_str().unwrap()).unwrap();
    let value: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(response.status, 200, "{value}"); assert_eq!(value["ok"], true); value
}
fn image_bytes(text: &str, base: &Path) -> Vec<Vec<u8>> {
    crate::document_assets::image_references(text).into_iter().map(|(_, reference)| {
        assert!(reference.starts_with("readmd-assets/"), "resource must use the shared resource directory");
        let reference = percent_encoding::percent_decode_str(&reference).decode_utf8().unwrap();
        std::fs::read(base.join(reference.as_ref())).unwrap()
    }).collect()
}
fn input(root: &Path, content: &str) -> (PathBuf, PathBuf) {
    let source = root.join("原件 中文😀"); let dest = root.join("另存 中文😀");
    std::fs::create_dir(&source).unwrap(); std::fs::create_dir(&dest).unwrap();
    let file = source.join("重复段落.md");
    let mut bytes = vec![0xef, 0xbb, 0xbf]; bytes.extend_from_slice(content.replace('\n',"\r\n").as_bytes());
    std::fs::write(&file, bytes).unwrap(); (file, dest)
}

#[test]
fn hidden_image_move_reopens_withdraws_and_keeps_original_bytes_and_identity_separate() {
    let root = tempfile::tempdir().unwrap(); let app = runtime(root.path());
    let raw = "😀\n\n![原图](illustration.svg)\n\n重复。\n\n重复。\n";
    let (file, dest) = input(root.path(), raw);
    let image = b"<svg xmlns='http://www.w3.org/2000/svg'><circle r='5'/></svg>";
    std::fs::write(file.parent().unwrap().join("illustration.svg"), image).unwrap();
    let before = std::fs::read(&file).unwrap(); let first = open(&app, &file);
    let proposed = mutate(&app, &first, "propose", json!({"origin":"ai","edits":[at(raw,"![原图](illustration.svg)",0,"待审正文。")]}));
    let target = dest.join("新身份.md"); let saved = save(&app, &file, &proposed, &target);
    assert_eq!(saved["review"]["changes"][0]["original"], "![原图](illustration.svg)");
    assert!(saved["review"]["changes"][0]["restore_original"].as_str().unwrap().contains("readmd-assets/"));
    let bytes = std::fs::read(&target).unwrap(); assert!(bytes.starts_with(&[0xef,0xbb,0xbf]));
    assert!(std::str::from_utf8(&bytes[3..]).unwrap().contains("\r\n"));
    assert_eq!(std::fs::read(&file).unwrap(), before);
    drop(app); let app = runtime(root.path()); let reopened = open(&app, &target);
    assert_eq!(reopened["changes"][0]["decision"], "pending");
    let withdrawn = mutate(&app, &reopened, "resolve", json!({"ids":[reopened["changes"][0]["id"]],"decision":"withdraw"}));
    assert_eq!(image_bytes(withdrawn["content"].as_str().unwrap(), &dest), vec![image.to_vec()]);
    assert_eq!(withdrawn["content"].as_str().unwrap().matches("重复。").count(), 2);
    let original = open(&app, &file); assert_ne!(original["document_id"], reopened["document_id"]);
    assert_eq!(std::fs::read(file.parent().unwrap().join("illustration.svg")).unwrap(), image);
    assert_eq!(std::fs::read(&file).unwrap(), before);
}

#[test]
fn partial_image_address_move_has_scoped_withdrawal_and_one_undo_redo_without_crossing_duplicates() {
    let root = tempfile::tempdir().unwrap(); let app = runtime(root.path());
    let raw = "![one](alpha-old.svg)\n\n![two](alpha-old.svg)\n";
    let (file, dest) = input(root.path(), raw);
    let old = b"<svg>old</svg>"; let new = b"<svg>new</svg>";
    std::fs::write(file.parent().unwrap().join("alpha-old.svg"), old).unwrap();
    std::fs::write(file.parent().unwrap().join("alpha-new.svg"), new).unwrap();
    let first = open(&app, &file);
    let proposed = mutate(&app, &first, "propose", json!({"origin":"ai","edits":[at(raw,"old",0,"new")]}));
    let saved = save(&app, &file, &proposed, &dest.join("局部地址.md")); let state = &saved["review"];
    assert_eq!(state["changes"][0]["original"], "old");
    assert_eq!(image_bytes(state["content"].as_str().unwrap(), &dest), vec![new.to_vec(),old.to_vec()]);
    let undone = mutate(&app, state, "undo", json!({}));
    assert_eq!(image_bytes(undone["content"].as_str().unwrap(), &dest), vec![old.to_vec(),old.to_vec()]);
    let redone = mutate(&app, &undone, "redo", json!({}));
    assert_eq!(image_bytes(redone["content"].as_str().unwrap(), &dest), vec![new.to_vec(),old.to_vec()]);
    let withdrawn = mutate(&app, &redone, "resolve", json!({"ids":[redone["changes"][0]["id"]],"decision":"withdraw"}));
    assert_eq!(image_bytes(withdrawn["content"].as_str().unwrap(), &dest), vec![old.to_vec(),old.to_vec()]);
    assert_eq!(std::fs::read_dir(dest.join("readmd-assets")).unwrap().count(), 2);
}

#[test]
fn accepted_hidden_image_can_move_again_and_withdraw_without_reading_previous_directory() {
    let root = tempfile::tempdir().unwrap(); let app = runtime(root.path());
    let raw = "![图](image.svg)\n"; let (file, dest) = input(root.path(), raw); let image = b"<svg>retained</svg>";
    std::fs::write(file.parent().unwrap().join("image.svg"), image).unwrap();
    let first = open(&app, &file);
    let proposed = mutate(&app, &first, "propose", json!({"origin":"ai","edits":[at(raw,"![图](image.svg)",0,"当前正文。")]}));
    let accepted = mutate(&app, &proposed, "resolve", json!({"ids":[proposed["changes"][0]["id"]],"decision":"accept"}));
    let second = dest.join("第二份.md"); let saved = save(&app, &file, &accepted, &second);
    let third = root.path().join("第三个目录"); std::fs::create_dir(&third).unwrap();
    let final_saved = save(&app, &second, &saved["review"], &third.join("最终.md"));
    let hold = root.path().join("暂时离线"); std::fs::rename(&dest, &hold).unwrap();
    let state = &final_saved["review"];
    let withdrawn = mutate(&app, state, "resolve", json!({"ids":[state["changes"][0]["id"]],"decision":"withdraw"}));
    assert_eq!(image_bytes(withdrawn["content"].as_str().unwrap(), &third), vec![image.to_vec()]);
}

#[test]
fn missing_second_hidden_image_rolls_back_owned_resources_and_preserves_pending_review() {
    let root = tempfile::tempdir().unwrap(); let app = runtime(root.path());
    let raw = "![a](first.svg)\n\n![b](missing.svg)\n"; let (file, dest) = input(root.path(), raw);
    std::fs::write(file.parent().unwrap().join("first.svg"), b"<svg>first</svg>").unwrap();
    let first = open(&app, &file);
    let proposed = mutate(&app, &first, "propose", json!({"origin":"ai","edits":[at(raw,"![a](first.svg)",0,"第一段。"),at(raw,"![b](missing.svg)",0,"第二段。")]}));
    let target = dest.join("不能假成功.md");
    let failure = save_document_copy(&app, &body(&file, &proposed), target.to_str().unwrap()).err().expect("missing hidden image must refuse save");
    assert_eq!(failure.code, "review_asset_relocation_required");
    assert!(!target.exists()); assert!(!dest.join("readmd-assets").exists());
    let current = app.review.handle(&json!({"action":"read","document_id":first["document_id"]})).unwrap();
    assert_eq!(current["content"], proposed["content"]); assert!(current["save_intent"].is_null());
    assert!(current["changes"].as_array().unwrap().iter().all(|c| c["decision"] == "pending"));
    assert_eq!(tx::revision(current["changes"][0]["original"].as_str().unwrap()), tx::revision("![a](first.svg)"));
}

#[test]
fn retained_resource_completion_budget_rejection_preserves_draft_and_removes_owned_folder() {
    let root = tempfile::tempdir().unwrap(); let mut app = runtime(root.path());
    let raw = "![a](first.svg)\n\n![b](second.svg)\n"; let (file, dest) = input(root.path(), raw);
    for name in ["first.svg", "second.svg"] { std::fs::write(file.parent().unwrap().join(name), name).unwrap(); }
    let first = open(&app, &file);
    let proposed = mutate(&app, &first, "propose", json!({"origin":"ai","edits":[at(raw,"![a](first.svg)",0,"第一段。"),at(raw,"![b](second.svg)",0,"第二段。")]}));
    let data = root.path().join("profile");
    let mut db = rusqlite::Connection::open(data.join("review.sqlite3")).unwrap();
    let tx = db.transaction().unwrap(); let used = crate::review_store::used_bytes(&tx).unwrap(); drop(tx); drop(db);
    Arc::get_mut(&mut app).unwrap().review = crate::review::Service::with_limit(&data, used + 1024);
    let target = dest.join("预算不足.md");
    let failure = save_document_copy(&app, &body(&file, &proposed), target.to_str().unwrap()).err().expect("completion budget must reject");
    assert_eq!(failure.code, "review_budget");
    assert!(!target.exists()); assert!(!dest.join("readmd-assets").exists());
    Arc::get_mut(&mut app).unwrap().review = crate::review::Service::new(&data);
    let current = app.review.handle(&json!({"action":"read","document_id":first["document_id"]})).unwrap();
    assert_eq!(current["content"], proposed["content"]); assert!(current["save_intent"].is_null());
    assert_eq!(current["changes"][0]["original"], "![a](first.svg)");
}

#[cfg(target_os = "macos")]
#[test]
fn native_immutable_target_failure_keeps_originals_pending_changes_and_resource_rollback() {
    let root = tempfile::tempdir().unwrap(); let app = runtime(root.path());
    let raw = "![图](image.svg)\n"; let (file, dest) = input(root.path(), raw);
    std::fs::write(file.parent().unwrap().join("image.svg"), b"<svg>original</svg>").unwrap();
    let first = open(&app, &file);
    let proposed = mutate(&app, &first, "propose", json!({"origin":"ai","edits":[at(raw,"![图](image.svg)",0,"待审正文。")]}));
    let target = dest.join("锁定目标.md"); std::fs::write(&target, b"immutable external target\n").unwrap();
    struct Unlock(PathBuf);
    impl Drop for Unlock { fn drop(&mut self) { let _ = std::process::Command::new("/usr/bin/chflags").arg("nouchg").arg(&self.0).status(); } }
    let _unlock = Unlock(target.clone());
    assert!(std::process::Command::new("/usr/bin/chflags").arg("uchg").arg(&target).status().unwrap().success());
    let response = save_document_copy(&app, &body(&file, &proposed), target.to_str().unwrap()).unwrap();
    assert_ne!(response.status, 200); assert_eq!(serde_json::from_slice::<Value>(&response.body).unwrap()["ok"], false);
    assert_eq!(std::fs::read(&target).unwrap(), b"immutable external target\n");
    assert!(!dest.join("readmd-assets").exists());
    let current = app.review.handle(&json!({"action":"read","document_id":first["document_id"]})).unwrap();
    assert_eq!(current["content"], proposed["content"]); assert!(current["save_intent"].is_null());
    assert_eq!(current["changes"][0]["decision"], "pending");
}

#[test]
fn foreign_content_at_hidden_resource_name_is_never_overwritten_or_removed() {
    let root = tempfile::tempdir().unwrap(); let app = runtime(root.path());
    let raw = "![图](image.svg)\n"; let (file, dest) = input(root.path(), raw); let image = b"<svg>expected</svg>";
    std::fs::write(file.parent().unwrap().join("image.svg"), image).unwrap();
    let first = open(&app, &file);
    let proposed = mutate(&app, &first, "propose", json!({"origin":"ai","edits":[at(raw,"![图](image.svg)",0,"待审正文。")]}));
    let resources = dest.join("readmd-assets"); std::fs::create_dir(&resources).unwrap();
    use sha2::{Digest, Sha256};
    let occupant = resources.join(format!("image-{}.svg", &format!("{:x}", Sha256::digest(image))[..24]));
    std::fs::write(&occupant, b"external occupant").unwrap();
    let target = dest.join("不能覆盖.md");
    assert!(save_document_copy(&app, &body(&file, &proposed), target.to_str().unwrap()).is_err());
    assert_eq!(std::fs::read(&occupant).unwrap(), b"external occupant");
    assert_eq!(std::fs::read_dir(&resources).unwrap().count(), 1); assert!(!target.exists());
    let current = app.review.handle(&json!({"action":"read","document_id":first["document_id"]})).unwrap();
    assert_eq!(current["content"], proposed["content"]); assert!(current["save_intent"].is_null());
}

#[test]
fn accept_before_move_then_undo_accept_keeps_projected_replacement_and_withdrawal_valid() {
    let root = tempfile::tempdir().unwrap(); let app = runtime(root.path());
    let raw = "![图](alpha-old.svg)\n"; let (file, dest) = input(root.path(), raw);
    let old = b"<svg>old</svg>";
    std::fs::write(file.parent().unwrap().join("alpha-old.svg"), old).unwrap();
    std::fs::write(file.parent().unwrap().join("alpha-new.svg"), b"<svg>new</svg>").unwrap();
    let first = open(&app, &file);
    let proposed = mutate(&app, &first, "propose", json!({"origin":"ai","edits":[at(raw,"old",0,"new")]}));
    let accepted = mutate(&app, &proposed, "resolve", json!({"ids":[proposed["changes"][0]["id"]],"decision":"accept"}));
    let saved = save(&app, &file, &accepted, &dest.join("同意后另存.md"));
    let undone_accept = mutate(&app, &saved["review"], "undo", json!({}));
    assert_eq!(undone_accept["changes"][0]["decision"], "pending");
    let withdrawn = mutate(&app, &undone_accept, "resolve", json!({"ids":[undone_accept["changes"][0]["id"]],"decision":"withdraw"}));
    assert_eq!(image_bytes(withdrawn["content"].as_str().unwrap(), &dest), vec![old.to_vec()]);
}

#[test]
fn unprojectable_manual_image_history_is_explicitly_refused_before_creating_a_copy() {
    let root = tempfile::tempdir().unwrap(); let app = runtime(root.path());
    let raw = "![图](alpha-old.svg)\n"; let (file, dest) = input(root.path(), raw);
    std::fs::write(file.parent().unwrap().join("alpha-old.svg"), b"<svg>old</svg>").unwrap();
    std::fs::write(file.parent().unwrap().join("alpha-new.svg"), b"<svg>new</svg>").unwrap();
    let first = open(&app, &file);
    let edited = mutate(&app, &first, "edit", json!({"origin":"editor","edits":[at(raw,"old",0,"new")]}));
    let target = dest.join("历史须保留.md");
    let failure = save_document_copy(&app, &body(&file, &edited), target.to_str().unwrap()).err().expect("unproven resource history must refuse");
    assert_eq!(failure.code, "review_asset_relocation_required");
    assert!(!target.exists()); assert!(!dest.join("readmd-assets").exists());
    let current = app.review.handle(&json!({"action":"read","document_id":first["document_id"]})).unwrap();
    assert_eq!(current["content"], edited["content"]); assert!(current["save_intent"].is_null());
    let undone = mutate(&app, &current, "undo", json!({})); assert_eq!(undone["content"], raw);
}

#[test]
fn same_directory_save_with_already_materialized_assets_does_not_rewrite_or_reject_plain_history() {
    let root = tempfile::tempdir().unwrap(); let app = runtime(root.path());
    let raw = "![图](image.svg)\n"; let (file, dest) = input(root.path(), raw);
    std::fs::write(file.parent().unwrap().join("image.svg"), b"<svg>image</svg>").unwrap();
    let first = open(&app, &file); let target = dest.join("日常保存.md");
    let saved = save(&app, &file, &first, &target);
    let text = saved["review"]["content"].as_str().unwrap();
    let edited = mutate(&app, &saved["review"], "edit", json!({"origin":"editor","edits":[at(text,"图",0,"新图")]}));
    let mut body = body(&target, &edited); body["path"] = json!(target); body["assets"] = saved["saved_assets"].clone();
    let (status, response) = call_in_process(&app, "POST", "/api/save", Some(&body));
    assert_eq!(status, 200, "{response}"); assert_eq!(response["ok"], true);
    assert_eq!(response["review_edit_steps"], json!([]));
    assert_eq!(image_bytes(response["review"]["content"].as_str().unwrap(), &dest), vec![b"<svg>image</svg>".to_vec()]);
}

#[test]
fn a_legacy_direct_save_cannot_move_visible_images_without_the_resource_writer() {
    let root = tempfile::tempdir().unwrap(); let app = runtime(root.path());
    let raw = "![图](image.svg)\n"; let (file, dest) = input(root.path(), raw);
    std::fs::write(file.parent().unwrap().join("image.svg"), b"<svg>source</svg>").unwrap();
    let first = open(&app, &file); let target = dest.join("目标.md");
    let failure = crate::review_api::SaveGuard::prepare(&app, &body(&file, &first), &target, raw, &[]).err().expect("legacy writer cannot omit dependencies");
    assert_eq!(failure.code, "review_asset_relocation_required");
    assert!(!target.exists()); assert!(!dest.join("readmd-assets").exists());
    assert!(app.review.handle(&json!({"action":"read","document_id":first["document_id"]})).unwrap()["save_intent"].is_null());
}

#[test]
fn version_five_review_store_migrates_atomically_before_new_semantic_payloads() {
    let root = tempfile::tempdir().unwrap(); let app = runtime(root.path());
    let raw = "原文。\n"; let (file, _) = input(root.path(), raw);
    let first = open(&app, &file);
    let proposed = mutate(&app, &first, "propose", json!({"origin":"ai","edits":[at(raw,"原文",0,"待审正文")]}));
    let db_path = root.path().join("profile/review.sqlite3"); drop(app);
    // The fixture has no new optional fields or SQL shape differences. Set
    // the old semantic version, independently of current connection creation.
    let db = rusqlite::Connection::open(&db_path).unwrap(); db.pragma_update(None,"user_version",5).unwrap(); drop(db);
    let app = runtime(root.path());
    let current = app.review.handle(&json!({"action":"read","document_id":first["document_id"]})).unwrap();
    assert_eq!(current["content"], proposed["content"]); assert_eq!(current["changes"][0]["original"], "原文");
    assert_eq!(current["changes"][0]["decision"], "pending");
    let db = rusqlite::Connection::open(&db_path).unwrap();
    let version:i64=db.pragma_query_value(None,"user_version",|row|row.get(0)).unwrap();
    assert_eq!(version,6); assert!(version>5,"a version-five writer must refuse this semantic format");
}

#[test]
fn saved_file_before_review_commit_recovers_hidden_resource_projection_in_a_new_process() {
    const CHILD: &str = "READMD_RESOURCE_INTENT_CHILD_ROOT";
    let raw = "![图](image.svg)\n";
    let image = b"<svg>recover after file commit</svg>";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root); let app = runtime(&root); let (file, dest) = input(&root, raw);
        std::fs::write(file.parent().unwrap().join("image.svg"), image).unwrap();
        let first = open(&app, &file);
        let proposed = mutate(&app, &first, "propose", json!({"origin":"ai","edits":[at(raw,"![图](image.svg)",0,"等待用户决定。")]}));
        let target = dest.join("中断后恢复.md"); let body = body(&file, &proposed); let text = body["content"].as_str().unwrap();
        // Drive the actual prepare and atomic writer, then end this dedicated
        // process without Rust destructors before the SQLite completion. This
        // is a controlled committed-file interruption, not a user Quit or power loss.
        let _guard = crate::review_api::SaveGuard::prepare(&app, &body, &target, text, &[]).unwrap().unwrap();
        let saved = save_text_atomic_inner(&target, text, "utf-8-sig", None, &app.paths.data_dir, true);
        assert_eq!(saved["ok"], true);
        std::process::exit(0);
    }
    let root = tempfile::tempdir().unwrap();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "server::review_resource_save_tests::saved_file_before_review_commit_recovers_hidden_resource_projection_in_a_new_process", "--test-threads=1"])
        .env(CHILD, root.path()).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).status().unwrap();
    assert!(status.success());
    let app = runtime(root.path()); let dest = root.path().join("另存 中文😀"); let target = dest.join("中断后恢复.md");
    let recovered = open(&app, &target);
    assert!(recovered["save_intent"].is_null()); assert_eq!(recovered["changes"][0]["decision"], "pending");
    assert_eq!(recovered["changes"][0]["original"], "![图](image.svg)");
    let withdrawn = mutate(&app, &recovered, "resolve", json!({"ids":[recovered["changes"][0]["id"]],"decision":"withdraw"}));
    assert_eq!(image_bytes(withdrawn["content"].as_str().unwrap(), &dest), vec![image.to_vec()]);
    let source = root.path().join("原件 中文😀");
    assert_eq!(std::fs::read(source.join("image.svg")).unwrap(), image);
    let original = std::fs::read(source.join("重复段落.md")).unwrap();
    assert_eq!(&original[3..], raw.replace('\n', "\r\n").as_bytes());
}
