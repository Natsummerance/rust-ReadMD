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
    assert_eq!(status, 200, "action={action}; response={value}; generation={}; review_revision={}",state["generation"],state["review_revision"]); value
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
fn manual_image_history_moves_reopens_and_replays_with_original_bytes() {
    let root = tempfile::tempdir().unwrap(); let app = runtime(root.path());
    let raw = "😀\n\n![one](alpha-old.svg)\n\n重复。\n\n![two](alpha-old.svg)\n";
    let (file, dest) = input(root.path(), raw);
    let old = b"<svg>manual original</svg>"; let new = b"<svg>manual replacement</svg>";
    std::fs::write(file.parent().unwrap().join("alpha-old.svg"), old).unwrap();
    std::fs::write(file.parent().unwrap().join("alpha-new.svg"), new).unwrap();
    let original_bytes = std::fs::read(&file).unwrap();
    let first = open(&app, &file);
    let image_edit = mutate(&app, &first, "edit", json!({"origin":"editor","edits":[at(raw,"old",0,"new")]}));
    let edited = mutate(&app, &image_edit, "edit", json!({"origin":"editor","edits":[{"from":0,"to":0,"insert":"后来输入。\n"}]}));
    let target = dest.join("保留手工历史.md"); let saved = save(&app, &file, &edited, &target);
    assert_eq!(image_bytes(saved["review"]["content"].as_str().unwrap(), &dest), vec![new.to_vec(), old.to_vec()]);
    assert_eq!(std::fs::read(&file).unwrap(), original_bytes);
    drop(app);
    std::fs::rename(file.parent().unwrap(), root.path().join("原目录离线")).unwrap();
    let app = runtime(root.path()); let reopened = open(&app, &target);
    let undone_text = mutate(&app, &reopened, "undo", json!({}));
    assert!(!undone_text["content"].as_str().unwrap().starts_with("后来输入。"));
    assert_eq!(image_bytes(undone_text["content"].as_str().unwrap(), &dest), vec![new.to_vec(), old.to_vec()]);
    let undone_image = mutate(&app, &undone_text, "undo", json!({}));
    assert_eq!(image_bytes(undone_image["content"].as_str().unwrap(), &dest), vec![old.to_vec(), old.to_vec()]);
    let redone_image = mutate(&app, &undone_image, "redo", json!({}));
    assert_eq!(image_bytes(redone_image["content"].as_str().unwrap(), &dest), vec![new.to_vec(), old.to_vec()]);
    let redone_text = mutate(&app, &redone_image, "redo", json!({}));
    assert_eq!(redone_text["content"], reopened["content"]);
    assert_eq!(std::fs::read_dir(dest.join("readmd-assets")).unwrap().count(), 2);
}

#[test]
fn missing_manual_history_image_is_refused_without_creating_a_copy_or_dropping_undo() {
    let root = tempfile::tempdir().unwrap(); let app = runtime(root.path());
    let raw = "![图](alpha-old.svg)\n"; let (file, dest) = input(root.path(), raw);
    std::fs::write(file.parent().unwrap().join("alpha-old.svg"), b"<svg>old</svg>").unwrap();
    std::fs::write(file.parent().unwrap().join("alpha-new.svg"), b"<svg>new</svg>").unwrap();
    let first = open(&app, &file);
    let edited = mutate(&app, &first, "edit", json!({"origin":"editor","edits":[at(raw,"old",0,"new")]}));
    std::fs::remove_file(file.parent().unwrap().join("alpha-old.svg")).unwrap();
    let target = dest.join("历史须保留.md");
    let failure = save_document_copy(&app, &body(&file, &edited), target.to_str().unwrap()).err().expect("missing historical resource must refuse");
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
fn version_five_and_six_review_stores_migrate_atomically_before_history_projections() {
    for old_version in [5,6] {
    let root = tempfile::tempdir().unwrap(); let app = runtime(root.path());
    let raw = "原文。\n"; let (file, _) = input(root.path(), raw);
    let first = open(&app, &file);
    let proposed = mutate(&app, &first, "propose", json!({"origin":"ai","edits":[at(raw,"原文",0,"待审正文")]}));
    let db_path = root.path().join("profile/review.sqlite3"); drop(app);
    // The fixture has no new optional fields or SQL shape differences. Set
    // the old semantic version, independently of current connection creation.
    let db = rusqlite::Connection::open(&db_path).unwrap(); db.pragma_update(None,"user_version",old_version).unwrap(); drop(db);
    let app = runtime(root.path());
    let current = app.review.handle(&json!({"action":"read","document_id":first["document_id"]})).unwrap();
    assert_eq!(current["content"], proposed["content"]); assert_eq!(current["changes"][0]["original"], "原文");
    assert_eq!(current["changes"][0]["decision"], "pending");
    let db = rusqlite::Connection::open(&db_path).unwrap();
    let version:i64=db.pragma_query_value(None,"user_version",|row|row.get(0)).unwrap();
    assert_eq!(version,7); assert!(version>old_version,"old writers must refuse the new history semantics");
    }
}

#[test]
fn manual_image_redo_branch_moves_twice_and_needs_neither_old_directory() {
    let root=tempfile::tempdir().unwrap();let app=runtime(root.path());
    let raw="😀 ![one](old.svg)\n";let(file,dest)=input(root.path(),raw);
    let old=b"<svg>old redo</svg>";let new=b"<svg>new redo</svg>";
    for(name,bytes)in [("old.svg",old.as_slice()),("new.svg",new.as_slice())]{std::fs::write(file.parent().unwrap().join(name),bytes).unwrap();}
    let first=open(&app,&file);let edited=mutate(&app,&first,"edit",json!({"edits":[at(raw,"old",0,"new")]}));
    let undone=mutate(&app,&edited,"undo",json!({}));assert_eq!(undone["can_redo"],true);
    let second=dest.join("second.md");let saved=save(&app,&file,&undone,&second);
    let third=root.path().join("第三个目录");std::fs::create_dir(&third).unwrap();let target=third.join("third.md");
    let saved_again=save(&app,&second,&saved["review"],&target);assert_eq!(saved_again["review"]["can_redo"],true);
    drop(app);std::fs::rename(file.parent().unwrap(),root.path().join("原件离线")).unwrap();std::fs::rename(&dest,root.path().join("第二份离线")).unwrap();
    let app=runtime(root.path());let reopened=open(&app,&target);let redo=mutate(&app,&reopened,"redo",json!({}));
    assert_eq!(image_bytes(redo["content"].as_str().unwrap(),&third),vec![new.to_vec()]);
    let undo=mutate(&app,&redo,"undo",json!({}));assert_eq!(image_bytes(undo["content"].as_str().unwrap(),&third),vec![old.to_vec()]);
}

#[test]
fn two_url_fragments_in_one_manual_action_merge_into_one_safe_compensation() {
    let root=tempfile::tempdir().unwrap();let app=runtime(root.path());
    let raw="![one](alpha-old.svg)\n\n![two](alpha-old.svg)\n";let(file,dest)=input(root.path(),raw);
    let old=b"<svg>alpha old</svg>";let new=b"<svg>beta new</svg>";
    std::fs::write(file.parent().unwrap().join("alpha-old.svg"),old).unwrap();std::fs::write(file.parent().unwrap().join("beta-new.svg"),new).unwrap();
    let first=open(&app,&file);let edited=mutate(&app,&first,"edit",json!({"edits":[at(raw,"alpha",0,"beta"),at(raw,"old",0,"new")]}));
    let saved=save(&app,&file,&edited,&dest.join("fragments.md"));
    let undone=mutate(&app,&saved["review"],"undo",json!({}));assert_eq!(image_bytes(undone["content"].as_str().unwrap(),&dest),vec![old.to_vec(),old.to_vec()]);
    let redone=mutate(&app,&undone,"redo",json!({}));assert_eq!(image_bytes(redone["content"].as_str().unwrap(),&dest),vec![new.to_vec(),old.to_vec()]);
}

#[test]
fn image_deletion_and_new_input_after_save_preserve_every_undo_redo_step() {
    let root=tempfile::tempdir().unwrap();let app=runtime(root.path());
    let raw="😀\n![one](old.svg)\n";let(file,dest)=input(root.path(),raw);let image=b"<svg>deleted but retained</svg>";
    std::fs::write(file.parent().unwrap().join("old.svg"),image).unwrap();let first=open(&app,&file);
    let deleted=mutate(&app,&first,"edit",json!({"edits":[at(raw,"![one](old.svg)",0,"")]}));
    let target=dest.join("deleted.md");let saved=save(&app,&file,&deleted,&target);
    let content=saved["review"]["content"].as_str().unwrap();let entered=mutate(&app,&saved["review"],"edit",json!({"edits":[{"from":content.encode_utf16().count(),"to":content.encode_utf16().count(),"insert":"新输入。\n"}]}));
    let undo_input=mutate(&app,&entered,"undo",json!({}));assert_eq!(undo_input["content"],saved["review"]["content"]);
    let undo_image=mutate(&app,&undo_input,"undo",json!({}));assert_eq!(image_bytes(undo_image["content"].as_str().unwrap(),&dest),vec![image.to_vec()]);
    let redo_image=mutate(&app,&undo_image,"redo",json!({}));let redo_input=mutate(&app,&redo_image,"redo",json!({}));assert_eq!(redo_input["content"],entered["content"]);
}

#[test]
fn unrelated_link_refactor_keeps_relocated_image_history_and_retention_pins() {
    // The copy uses real files. Refactor evidence below exercises its shared
    // review journal only; it does not claim a Mac native group rename lease.
    let root=tempfile::tempdir().unwrap();let app=runtime(root.path());
    let raw="![one](old.svg)\n\n[doc](old.md)\n";let(file,dest)=input(root.path(),raw);
    let old=b"<svg>old image</svg>";let new=b"<svg>new image</svg>";
    std::fs::write(file.parent().unwrap().join("old.svg"),old).unwrap();std::fs::write(file.parent().unwrap().join("new.svg"),new).unwrap();
    let first=open(&app,&file);let edited=mutate(&app,&first,"edit",json!({"edits":[at(raw,"old.svg",0,"new.svg")]}));
    let target=dest.join("linked.md");let saved=save(&app,&file,&edited,&target);let current=&saved["review"];
    let content=current["content"].as_str().unwrap();let rewrite=at(content,"old.md",0,"renamed.md");
    let after=tx::apply(content,&tx::ContentTransaction{base_revision:tx::revision(content),origin:"review".into(),edits:serde_json::from_value(json!([rewrite.clone()])).unwrap()}).unwrap().content;
    let source=dest.join("old.md");let renamed=dest.join("renamed.md");std::fs::write(&source,"# Source\n").unwrap();
    let bytes_revision=|text:&str,encoding:&str|{use sha2::{Digest,Sha256};format!("{:x}",Sha256::digest(crate::text_encoding::encode(text,encoding).unwrap()))};
    let make_file=|path:&Path,to:&Path,before:&str,after:&str,edits:Value,encoding:&str,id:&str|json!({
        "source_path":path,"target_path":to,"before":before,"after":after,"before_disk_revision":bytes_revision(before,encoding),"after_disk_revision":bytes_revision(after,encoding),
        "encoding":encoding,"line_ending":if before.contains("\r\n"){"crlf"}else{"lf"},"edits":edits,"review_head":null,"source_identity":id,"stage_identity":null,
        "stage_path":path.with_extension("refactor-stage"),"backup_path":path.with_extension("refactor-backup")});
    let mut plan=json!({"operation_id":"resource-history-refactor","plan_hash":"1".repeat(64),"workspace_root":root.path(),"source_path":source,"target_path":renamed,"files":[
        make_file(&source,&renamed,"# Source\n","# Source\n",json!([]),"utf-8","fixture-source"),
        make_file(&target,&target,&content.replace('\n',"\r\n"),&after.replace('\n',"\r\n"),json!([rewrite]),"utf-8-sig","fixture-linked")]});
    let paths=vec![source.to_string_lossy().into_owned(),target.to_string_lossy().into_owned()];
    let heads=app.review.refactor_heads(&paths).unwrap();for(i,head)in heads.into_iter().enumerate(){plan["files"][i]["review_head"]=head;}
    app.review.refactor_prepare(&plan).unwrap();plan["files"][1]["stage_identity"]=json!("fixture-stage");
    app.review.refactor_update("resource-history-refactor",&json!({"state":"applying","plan":plan})).unwrap();
    let committed=app.review.refactor_finish("resource-history-refactor","committed").unwrap();
    let head=&committed["files"][1]["review"];
    let undone=mutate(&app,head,"undo",json!({}));assert!(undone["content"].as_str().unwrap().contains("[doc](renamed.md)"));
    assert_eq!(image_bytes(undone["content"].as_str().unwrap(),&dest),vec![old.to_vec()]);
    let redone=mutate(&app,&undone,"redo",json!({}));assert_eq!(redone["content"],after);
    let pins=app.review.pinned_assets().unwrap();
    for entry in std::fs::read_dir(dest.join("readmd-assets")).unwrap(){let name=entry.unwrap().path().to_string_lossy().into_owned();assert!(pins.contains(&name),"all relocated undo and redo resources must remain pinned");}
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

#[test]
fn committed_manual_resource_history_recovers_in_a_new_process_with_old_directory_offline() {
    const CHILD:&str="READMD_MANUAL_HISTORY_CHILD_ROOT";
    let raw="😀\n\n![one](old.svg)\n";let old=b"<svg>recover old</svg>";let new=b"<svg>recover new</svg>";
    if let Some(root)=std::env::var_os(CHILD){
        let root=PathBuf::from(root);let app=runtime(&root);let(file,dest)=input(&root,raw);
        for(name,bytes)in [("old.svg",old.as_slice()),("new.svg",new.as_slice())]{std::fs::write(file.parent().unwrap().join(name),bytes).unwrap();}
        let first=open(&app,&file);let edited=mutate(&app,&first,"edit",json!({"edits":[at(raw,"old",0,"new")]}));
        let edited=mutate(&app,&edited,"edit",json!({"edits":[{"from":0,"to":0,"insert":"新增正文。\n"}]}));
        let target=dest.join("recovered.md");let body=body(&file,&edited);let text=body["content"].as_str().unwrap();
        let assets=crate::document_assets::local_handles(text,file.parent().unwrap());
        let resources=crate::document_assets::materialize(text,&target,&assets).unwrap();
        let _guard=crate::review_api::SaveGuard::prepare_with_assets(&app,&body,&target,&resources.text,&resources.edits,&assets).unwrap().unwrap();
        let saved=save_text_atomic_inner(&target,&resources.text,"utf-8-sig",None,&app.paths.data_dir,true);assert_eq!(saved["ok"],true);
        std::process::exit(0); // controlled file commit before SQLite completion
    }
    let root=tempfile::tempdir().unwrap();
    let status=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","server::review_resource_save_tests::committed_manual_resource_history_recovers_in_a_new_process_with_old_directory_offline","--test-threads=1"])
        .env(CHILD,root.path()).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).status().unwrap();assert!(status.success());
    std::fs::rename(root.path().join("原件 中文😀"),root.path().join("原件离线")).unwrap();
    let app=runtime(root.path());let dest=root.path().join("另存 中文😀");let target=dest.join("recovered.md");
    let recovered=open(&app,&target);assert!(recovered["save_intent"].is_null());
    let undo_text=mutate(&app,&recovered,"undo",json!({}));assert!(!undo_text["content"].as_str().unwrap().starts_with("新增正文。"));
    let undo_image=mutate(&app,&undo_text,"undo",json!({}));assert_eq!(image_bytes(undo_image["content"].as_str().unwrap(),&dest),vec![old.to_vec()]);
    let redo_image=mutate(&app,&undo_image,"redo",json!({}));let redo_text=mutate(&app,&redo_image,"redo",json!({}));assert_eq!(redo_text["content"],recovered["content"]);
    let saved=save(&app,&target,&redo_text,&target);drop(app);let app=runtime(root.path());let reopened=open(&app,&target);assert_eq!(reopened["content"],saved["review"]["content"]);
    let bytes=std::fs::read(root.path().join("原件离线/重复段落.md")).unwrap();assert_eq!(&bytes[3..],raw.replace('\n',"\r\n").as_bytes());
    for fault in ["missing", "changed"] {
        let root = tempfile::tempdir().unwrap();
        let status=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","server::review_resource_save_tests::committed_manual_resource_history_recovers_in_a_new_process_with_old_directory_offline","--test-threads=1"])
            .env(CHILD,root.path()).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).status().unwrap();assert!(status.success());
        std::fs::rename(root.path().join("原件 中文😀"),root.path().join("原件离线")).unwrap();
        let dest = root.path().join("另存 中文😀"); let target = dest.join("recovered.md"); let file_bytes = std::fs::read(&target).unwrap();
        let historical = std::fs::read_dir(dest.join("readmd-assets")).unwrap().map(|e| e.unwrap().path()).find(|p| p.file_name().unwrap().to_string_lossy().starts_with("old-")).unwrap();
        if fault == "missing" { std::fs::remove_file(&historical).unwrap(); } else { std::fs::write(&historical,b"external recovery occupant").unwrap(); }
        let app = runtime(root.path()); let blocked = open(&app,&target);
        assert_eq!(blocked["status"],"needs_recovery","a file commit alone cannot certify missing or changed undo resources");
        assert!(blocked["save_intent"].is_object()); assert_eq!(blocked["content"],"新增正文。\n😀\n\n![one](new.svg)\n");
        assert_eq!(std::fs::read(&target).unwrap(),file_bytes);
        if fault == "changed" { assert_eq!(std::fs::read(&historical).unwrap(),b"external recovery occupant"); }
        let request = json!({"action":"undo","document_id":blocked["document_id"],"generation":blocked["generation"],"review_revision":blocked["review_revision"],"call_id":"blocked-recovery-undo"});
        let (status,failure)=call_in_process(&app,"POST","/api/review",Some(&request));
        // The old source is deliberately offline. Its HTTP authorization can
        // reject first; the shared service must independently block the undo.
        assert!((status==403&&failure["error_code"]=="review_not_authorized")||(status==409&&failure["error_code"]=="review_needs_recovery"),"{status}: {failure}");
        assert_eq!(app.review.handle(&request).unwrap_err(),"review_needs_recovery");
        // An explicit fixture repair, never automatic product cleanup.
        std::fs::write(&historical,old).unwrap();let repaired=open(&app,&target);assert!(repaired["save_intent"].is_null());
        let untext=mutate(&app,&repaired,"undo",json!({}));let unimage=mutate(&app,&untext,"undo",json!({}));
        assert_eq!(image_bytes(unimage["content"].as_str().unwrap(),&dest),vec![old.to_vec()]);
    }
}

#[test]
fn taken_over_image_history_moves_twice_and_restores_each_review_decision() {
    let root = tempfile::tempdir().unwrap(); let app = runtime(root.path());
    let raw = "😀\n![原图](old.svg)\n\n重复。\n重复。\n";
    let (file, second) = input(root.path(), raw);
    let old = b"<svg>original review image</svg>";
    let candidate = b"<svg>AI candidate image</svg>";
    let manual = b"<svg>user image</svg>";
    for (name, bytes) in [("old.svg", old.as_slice()), ("candidate.svg", candidate.as_slice()), ("manual.svg", manual.as_slice())] {
        std::fs::write(file.parent().unwrap().join(name), bytes).unwrap();
    }
    let original = std::fs::read(&file).unwrap();
    let first = open(&app, &file);
    let proposed = mutate(&app, &first, "propose", json!({"origin":"ai","edits":[at(raw,"![原图](old.svg)",0,"![原图](candidate.svg)")]}));
    let edited = mutate(&app, &proposed, "edit", json!({"edits":[at(proposed["content"].as_str().unwrap(),"candidate.svg",0,"manual.svg")]}));
    assert_eq!(edited["changes"][0]["taken_over"], true);
    let accepted = mutate(&app, &edited, "resolve", json!({"ids":[edited["changes"][0]["id"]],"decision":"accept","resolution":"keep_current"}));
    let entered = mutate(&app, &accepted, "edit", json!({"edits":[{"from":0,"to":0,"insert":"后来输入。\n"}]}));
    let second_file = second.join("second.md"); let saved = save(&app, &file, &entered, &second_file);
    let third = root.path().join("第三个目录"); std::fs::create_dir(&third).unwrap(); let target = third.join("third.md");
    let saved = save(&app, &second_file, &saved["review"], &target);
    assert_eq!(saved["review"]["changes"][0]["original"], "![原图](old.svg)");
    drop(app);
    std::fs::rename(file.parent().unwrap(), root.path().join("原件离线")).unwrap();
    std::fs::rename(&second, root.path().join("第二份离线")).unwrap();
    let app = runtime(root.path()); let reopened = open(&app, &target);
    let mut current = reopened.clone();
    for (bytes, decision, taken_over) in [(manual.as_slice(), "accepted", true), (manual.as_slice(), "conflict", true), (candidate.as_slice(), "pending", false), (old.as_slice(), "", false)] {
        current = mutate(&app, &current, "undo", json!({}));
        assert_eq!(image_bytes(current["content"].as_str().unwrap(), &third), vec![bytes.to_vec()]);
        if decision.is_empty() { assert_eq!(current["changes"], json!([])); }
        else { assert_eq!(current["changes"][0]["decision"], decision); assert_eq!(current["changes"][0]["taken_over"], taken_over); }
    }
    for (bytes, decision, taken_over) in [(candidate.as_slice(), "pending", false), (manual.as_slice(), "conflict", true), (manual.as_slice(), "accepted", true), (manual.as_slice(), "accepted", true)] {
        current = mutate(&app, &current, "redo", json!({}));
        assert_eq!(image_bytes(current["content"].as_str().unwrap(), &third), vec![bytes.to_vec()]);
        assert_eq!(current["changes"][0]["decision"], decision); assert_eq!(current["changes"][0]["taken_over"], taken_over);
    }
    assert_eq!(current["content"], reopened["content"]);
    // A retained original is recovery data, never permission to overwrite a
    // manually owned range by withdrawing the old AI proposal.
    let request = json!({"action":"resolve","document_id":current["document_id"],"generation":current["generation"],"review_revision":current["review_revision"],"call_id":"no-takeover-withdraw","ids":[current["changes"][0]["id"]],"decision":"withdraw"});
    let (status, failure) = call_in_process(&app, "POST", "/api/review", Some(&request));
    assert_eq!(status, 409); assert_eq!(failure["error_code"], "review_conflict");
    assert_eq!(std::fs::read(root.path().join("原件离线/重复段落.md")).unwrap(), original);
}

#[test]
fn withdrawn_image_and_later_hand_edit_keep_both_history_directions_after_reopen() {
    let root = tempfile::tempdir().unwrap(); let app = runtime(root.path());
    let raw = "![原图](old.svg)\n"; let (file, dest) = input(root.path(), raw);
    let old = b"<svg>withdrawal original</svg>"; let new = b"<svg>withdrawal candidate</svg>"; let manual = b"<svg>withdrawal manual</svg>";
    for (name, bytes) in [("old.svg", old.as_slice()), ("new.svg", new.as_slice()), ("manual.svg", manual.as_slice())] { std::fs::write(file.parent().unwrap().join(name), bytes).unwrap(); }
    let first = open(&app, &file);
    let proposed = mutate(&app, &first, "propose", json!({"edits":[at(raw,"![原图](old.svg)",0,"![原图](new.svg)")]}));
    let withdrawn = mutate(&app, &proposed, "resolve", json!({"ids":[proposed["changes"][0]["id"]],"decision":"withdraw"}));
    let edited = mutate(&app, &withdrawn, "edit", json!({"edits":[at(raw,"old.svg",0,"manual.svg")]}));
    let target = dest.join("withdrawn.md"); let saved = save(&app, &file, &edited, &target);
    drop(app); std::fs::rename(file.parent().unwrap(), root.path().join("原件离线")).unwrap();
    let app = runtime(root.path()); let reopened = open(&app, &target);
    assert_eq!(reopened["content"], saved["review"]["content"]);
    let unedit = mutate(&app, &reopened, "undo", json!({}));
    assert_eq!(unedit["changes"][0]["decision"], "withdrawn"); assert_eq!(image_bytes(unedit["content"].as_str().unwrap(), &dest), vec![old.to_vec()]);
    let unwithdraw = mutate(&app, &unedit, "undo", json!({}));
    assert_eq!(unwithdraw["changes"][0]["decision"], "pending"); assert_eq!(image_bytes(unwithdraw["content"].as_str().unwrap(), &dest), vec![new.to_vec()]);
    let unpropose = mutate(&app, &unwithdraw, "undo", json!({})); assert_eq!(image_bytes(unpropose["content"].as_str().unwrap(), &dest), vec![old.to_vec()]);
    let repropose = mutate(&app, &unpropose, "redo", json!({})); let rewithdraw = mutate(&app, &repropose, "redo", json!({}));
    let reedit = mutate(&app, &rewithdraw, "redo", json!({}));
    assert_eq!(reedit["content"], reopened["content"]); assert_eq!(reedit["changes"][0]["decision"], "withdrawn");
    assert_eq!(image_bytes(reedit["content"].as_str().unwrap(), &dest), vec![manual.to_vec()]);
}

#[test]
fn manual_image_history_cancel_stale_and_budget_failures_preserve_source_and_undo() {
    for fault in ["cancel", "stale", "budget"] {
        let root = tempfile::tempdir().unwrap(); let mut app = runtime(root.path());
        let raw = "😀 ![图](old.svg)\n"; let (file, dest) = input(root.path(), raw);
        for name in ["old.svg", "new.svg"] { std::fs::write(file.parent().unwrap().join(name), name).unwrap(); }
        let original = std::fs::read(&file).unwrap();
        let first = open(&app, &file);
        let edited = mutate(&app, &first, "edit", json!({"edits":[at(raw,"old",0,"new")]}));
        let target = dest.join("failed.md"); let request = body(&file, &edited);
        let mut expected = edited.clone();
        if fault == "stale" { expected = mutate(&app, &edited, "edit", json!({"edits":[{"from":0,"to":0,"insert":"更新修订。\n"}]})); }
        if fault == "budget" {
            let data = root.path().join("profile"); let mut db = rusqlite::Connection::open(data.join("review.sqlite3")).unwrap();
            let transaction = db.transaction().unwrap(); let used = crate::review_store::used_bytes(&transaction).unwrap(); drop(transaction); drop(db);
            Arc::get_mut(&mut app).unwrap().review = crate::review::Service::with_limit(&data, used + 1024);
        }
        let attempt = || save_document_copy(&app, &request, target.to_str().unwrap());
        let failure = if fault == "cancel" { crate::speech::with_cancel(Arc::new(|| true), attempt) } else { attempt() };
        let failure = failure.err().expect("failure must occur before creating a document");
        assert_eq!(failure.code, match fault { "cancel" => "cancelled", "stale" => "review_save_conflict", _ => "review_budget" });
        assert!(!target.exists()); assert!(!dest.join("readmd-assets").exists()); assert_eq!(std::fs::read(&file).unwrap(), original);
        if fault == "budget" { Arc::get_mut(&mut app).unwrap().review = crate::review::Service::new(&root.path().join("profile")); }
        let current = app.review.handle(&json!({"action":"read","document_id":first["document_id"]})).unwrap();
        assert_eq!(current["content"], expected["content"]); assert!(current["save_intent"].is_null()); assert_eq!(current["can_undo"], true);
        let current = if fault == "stale" { mutate(&app, &current, "undo", json!({})) } else { current };
        let original_body = mutate(&app, &current, "undo", json!({})); assert_eq!(original_body["content"], raw);
    }
}

#[test]
fn prepared_history_refuses_changed_resource_and_preserves_external_bytes_on_abort() {
    let root = tempfile::tempdir().unwrap(); let app = runtime(root.path());
    let raw = "![图](old.svg)\n"; let (file, dest) = input(root.path(), raw);
    std::fs::write(file.parent().unwrap().join("old.svg"), b"<svg>original</svg>").unwrap();
    std::fs::write(file.parent().unwrap().join("new.svg"), b"<svg>replacement</svg>").unwrap();
    let first = open(&app, &file); let edited = mutate(&app, &first, "edit", json!({"edits":[at(raw,"old",0,"new")]}));
    let request = body(&file, &edited); let text = request["content"].as_str().unwrap(); let target = dest.join("cannot-commit.md");
    let handles = crate::document_assets::local_handles(text, file.parent().unwrap());
    let visible = crate::document_assets::materialize(text, &target, &handles).unwrap();
    let guard = crate::review_api::SaveGuard::prepare_with_assets(&app, &request, &target, &visible.text, &visible.edits, &handles).unwrap().unwrap();
    let historical = std::fs::read_dir(dest.join("readmd-assets")).unwrap().map(|e| e.unwrap().path()).find(|p| p.file_name().unwrap().to_string_lossy().starts_with("old-")).unwrap();
    // Truncate and rewrite the same file identity, rather than replacing it.
    std::fs::write(&historical, b"external in-place modification").unwrap();
    assert!(guard.verify_target().is_err(), "all planned historical bytes must be checked before the document writer");
    drop(guard); drop(visible);
    assert_eq!(std::fs::read(&historical).unwrap(), b"external in-place modification"); assert!(!target.exists());
    let current = app.review.handle(&json!({"action":"read","document_id":first["document_id"]})).unwrap();
    assert!(current["save_intent"].is_null()); assert_eq!(current["content"], edited["content"]);
    assert_eq!(mutate(&app, &current, "undo", json!({}))["content"], raw);
}

#[test]
fn enclosing_partial_image_undo_then_save_copy_retains_precise_metadata_and_hidden_resources() {
    let root = tempfile::tempdir().unwrap();
    let app = runtime(root.path());
    use sha2::Digest;
    let raw = "A😀|![one](old.svg)|ZZ\n";
    let expected_original = "\u{feff}A😀|![one](old.svg)|ZZ\r\n";
    assert!(!raw.contains('\r') && !raw.starts_with('\u{feff}'));
    let (file, dest) = input(root.path(), raw);
    assert_eq!(std::fs::read(&file).unwrap(),expected_original.as_bytes());
    let old = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"><path d="M0 0L1 1"/></svg>"#;
    let new = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"><path d="M0 1L1 0"/></svg>"#;
    assert_eq!(old.len(),84);assert_eq!(new.len(),84);
    assert_eq!(format!("{:x}",sha2::Sha256::digest(old)),"e9f329621cfb632a24bebdce7d173dddfe75d0bf46edddab6c0535b13eb65368");
    assert_eq!(format!("{:x}",sha2::Sha256::digest(new)),"7c2c8f3b040de0f1ff1d217d2ca52934b3738206e15e16c8fc71c8671d6ccf25");
    std::fs::write(file.parent().unwrap().join("old.svg"), old).unwrap();
    std::fs::write(file.parent().unwrap().join("new.svg"), new).unwrap();
    let first = open(&app, &file);
    let proposed = mutate(&app, &first, "propose", json!({"origin":"ai","edits":[
        {"from":11,"to":14,"insert":"new"},{"from":20,"to":22,"insert":"YY"}]}));
    let image_id = proposed["changes"].as_array().unwrap().iter().find(|x|x["from"]==11).unwrap()["id"].clone();
    let outside_id = proposed["changes"].as_array().unwrap().iter().find(|x|x["from"]==20).unwrap()["id"].clone();
    let manual = mutate(&app, &proposed, "edit", json!({"edits":[{"from":4,"to":19,"insert":"prefix ![one](new.svg)"}]}));
    let restored = mutate(&app, &manual, "undo", json!({}));
    assert_eq!(restored["content"], "A😀|![one](new.svg)|YY\n");
    let image = restored["changes"].as_array().unwrap().iter().find(|x|x["id"]==image_id).unwrap();
    assert_eq!(image["from"],11); assert_eq!(image["to"],14);
    assert_eq!(image["original"],"old"); assert_eq!(image["replacement"],"new");
    assert_eq!(image["decision"],"pending"); assert_eq!(image["application"],"applied"); assert_eq!(image["taken_over"],false);
    let accepted = mutate(&app, &restored, "resolve", json!({"ids":[image_id],"decision":"accept"}));
    let target = dest.join("exact-image.md");
    let saved = save(&app, &file, &accepted, &target);
    let new_uri = "readmd-assets/new%2D7c2c8f3b040de0f1ff1d217d%2Esvg";
    let old_uri = "readmd-assets/old%2De9f329621cfb632a24bebdce%2Esvg";
    let expected = "A😀|![one](readmd-assets/new%2D7c2c8f3b040de0f1ff1d217d%2Esvg)|YY\n";
    assert_eq!(saved["review"]["content"],expected);
    assert_eq!(std::fs::read(&file).unwrap(),expected_original.as_bytes());
    assert_eq!(std::fs::read(&target).unwrap(),format!("\u{feff}{}",expected.replace('\n',"\r\n")).as_bytes());
    let decode = |uri:&str| percent_encoding::percent_decode_str(uri).decode_utf8().unwrap().into_owned();
    assert_eq!(std::fs::read(dest.join(decode(old_uri))).unwrap(),old.as_slice());
    assert_eq!(std::fs::read(dest.join(decode(new_uri))).unwrap(),new.as_slice());
    let document_id=saved["review"]["document_id"].clone();
    drop(app);
    std::fs::rename(file.parent().unwrap(),root.path().join("original-offline")).unwrap();
    let app=runtime(root.path());
    let reopened=open(&app,&target);assert_eq!(reopened["document_id"],document_id);
    let undone=mutate(&app,&reopened,"undo",json!({}));
    assert_eq!(undone["content"],expected);assert_eq!(undone["generation"],reopened["generation"]);
    let image=undone["changes"].as_array().unwrap().iter().find(|x|x["id"]==image_id).unwrap();
    assert_eq!(image["from"],11);assert_eq!(image["to"],61);assert_eq!(image["original"],"old");assert_eq!(image["replacement"],new_uri);assert_eq!(image["restore_original"],old_uri);assert_eq!(image["decision"],"pending");assert_eq!(image["taken_over"],false);
    let outside=undone["changes"].as_array().unwrap().iter().find(|x|x["id"]==outside_id).unwrap();
    assert_eq!(outside["from"],63);assert_eq!(outside["to"],65);assert_eq!(outside["original"],"ZZ");assert_eq!(outside["replacement"],"YY");assert_eq!(outside["decision"],"pending");assert_eq!(outside["taken_over"],false);
    let withdrawn=mutate(&app,&undone,"resolve",json!({"ids":[image_id],"decision":"withdraw"}));
    assert_eq!(withdrawn["content"],"A😀|![one](readmd-assets/old%2De9f329621cfb632a24bebdce%2Esvg)|YY\n");
    assert_eq!(image_bytes(withdrawn["content"].as_str().unwrap(),&dest),vec![old.to_vec()]);
    let back=mutate(&app,&withdrawn,"undo",json!({}));assert_eq!(back["content"],expected);
    assert_eq!(image_bytes(back["content"].as_str().unwrap(),&dest),vec![new.to_vec()]);
    assert_eq!(std::fs::read(&target).unwrap(),format!("\u{feff}{}",expected.replace('\n',"\r\n")).as_bytes());
}
