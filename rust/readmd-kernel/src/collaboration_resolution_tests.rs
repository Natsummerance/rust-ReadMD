use super::*;
use crate::document_transaction::TextEdit;

fn client(service: &Service, credential: &Credential) -> Doc {
    let doc = fresh();
    let initial = service.sync(&credential.room, &credential.token, &STANDARD.encode([0]), None, None).unwrap();
    doc.transact_mut().apply_update(Update::decode_v1(&STANDARD.decode(initial["update"].as_str().unwrap()).unwrap()).unwrap()).unwrap();
    doc
}
fn send(service: &Service, credential: &Credential, doc: &Doc, vector: &StateVector) {
    service.sync(&credential.room, &credential.token, &STANDARD.encode(vector.encode_v1()), Some(&STANDARD.encode(doc.transact().encode_state_as_update_v1(vector))), None).unwrap();
}
fn tx(source: &str, from: usize, to: usize, insert: &str) -> ContentTransaction {
    ContentTransaction { base_revision: document_transaction::revision(source), origin: "review".into(), edits: vec![TextEdit { from, to, insert: insert.into() }] }
}
fn generation(service: &Service, credential: &Credential) -> Option<i64> {
    service.snapshot(&credential.room, &credential.token).unwrap()["generation"].as_i64()
}

#[test]
fn structural_conflict_requires_a_fresh_authorized_merge_and_retains_original_history() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("collaboration.sqlite");
    let service = Service::open(&path).unwrap();
    let source = "# 甲🦀\n\nParagraph.\n";
    let owner = service.create("Sections", source, "Owner").unwrap();
    let editor = service.invite(&owner.room, &owner.token, Role::Editor, "Editor").unwrap();
    let reviewer = service.invite(&owner.room, &owner.token, Role::Reviewer, "Reviewer").unwrap();
    let proposal = tx(source, 2, 5, "Reviewed heading");
    let review = service.review(&owner.room, &reviewer.token, json!({"text":"Restructure heading","transaction":proposal,"_decision":{"method":"forged"},"_generation":999})).unwrap();
    assert_eq!(service.reviews(&owner.room, &owner.token).unwrap()["reviews"][0]["body"]["_generation"], 0);
    let doc = client(&service, &editor);
    let vector = doc.transact().state_vector();
    let text = doc.get_or_insert_text("content");
    {
        let mut txn = doc.transact_mut();
        text.remove_range(&mut txn, 2, 3);
        text.insert(&mut txn, 2, "Current heading");
    }
    send(&service, &editor, &doc, &vector);
    assert_eq!(service.resolve(&owner.room, &owner.token, &review, true).unwrap_err().status, 409);
    let preview = service.review_preview(&owner.room, &owner.token, &review).unwrap();
    assert_eq!(preview["conflict"], true);
    assert_eq!(preview["edits"][0]["original"], "甲🦀");
    assert_eq!(preview["status"], "open");
    let other = service.create("Other", "private", "Other").unwrap();
    assert_eq!(service.review_preview(&other.room, &other.token, &review).unwrap_err().status, 404);
    let current = preview["content"].as_str().unwrap();
    // A user explicitly selects the current heading, including when structural
    // deletion has collapsed the old anchors to an insertion point.
    let merge = tx(current, 2, 17, "Merged 🦀 heading");
    assert_eq!(service.resolve_with_transaction(&reviewer.room, &reviewer.token, &review, true, Some(merge.clone()), preview["generation"].as_i64()).unwrap_err().status, 403);
    service.permission(&owner.room, &owner.token, &editor.member, Some(Role::Reviewer)).unwrap();
    assert_eq!(service.resolve_with_transaction(&editor.room, &editor.token, &review, true, Some(merge.clone()), preview["generation"].as_i64()).unwrap_err().status, 403);
    service.permission(&owner.room, &owner.token, &editor.member, Some(Role::Editor)).unwrap();
    let vector = doc.transact().state_vector();
    let end = text.len(&doc.transact());
    text.insert(&mut doc.transact_mut(), end, "Unrelated later edit.\n");
    send(&service, &editor, &doc, &vector);
    assert_eq!(service.resolve_with_transaction(&owner.room, &owner.token, &review, true, Some(merge), preview["generation"].as_i64()).unwrap_err().status, 409);
    let before = service.snapshot(&owner.room, &owner.token).unwrap()["content"].as_str().unwrap().to_string();
    let history = service.reviews(&owner.room, &owner.token).unwrap();
    assert_eq!(history["reviews"][0]["status"], "open");
    assert!(history["reviews"][0]["body"]["_decision"].is_null());
    service.resolve_with_transaction(&editor.room, &editor.token, &review, true, Some(tx(&before, 2, 17, "Merged 🦀 heading")), generation(&service, &editor)).unwrap();
    let after = "# Merged 🦀 heading\n\nParagraph.\nUnrelated later edit.\n";
    assert_eq!(service.snapshot(&owner.room, &owner.token).unwrap()["content"], after);
    let history = service.reviews(&owner.room, &owner.token).unwrap();
    let item = &history["reviews"][0];
    assert_eq!(item["body"]["transaction"], json!(proposal));
    assert_eq!(item["body"]["_anchors"][0]["original"], "甲🦀");
    assert_eq!(item["body"]["_decision"]["method"], "manual");
    assert_eq!(item["body"]["_decision"]["member"], editor.member);
    let inverse: Vec<TextEdit> = serde_json::from_value(item["body"]["_decision"]["inverse"].clone()).unwrap();
    assert_eq!(document_transaction::apply(after, &ContentTransaction { base_revision: document_transaction::revision(after), origin: "review".into(), edits: inverse }).unwrap().content, before);
    assert_eq!(service.resolve(&owner.room, &owner.token, &review, false).unwrap_err().status, 409);
    drop(service);
    let service = Service::open(&path).unwrap();
    assert_eq!(service.snapshot(&owner.room, &owner.token).unwrap()["content"], after);
    assert_eq!(service.reviews(&owner.room, &owner.token).unwrap(), history);
}

#[test]
fn failed_merge_commit_and_invalid_ranges_never_change_text_or_review_status() {
    let data = tempfile::tempdir().unwrap();
    let service = Service::open(&data.path().join("db")).unwrap();
    let owner = service.create("Atomic", "😀abc", "Owner").unwrap();
    let review = service.review(&owner.room, &owner.token, json!({"text":"Change","transaction":tx("😀abc", 2, 3, "B")})).unwrap();
    for invalid_tx in [tx("😀abc", 1, 2, "X"), ContentTransaction { origin:"editor".into(), ..tx("😀abc", 2, 3, "X") }] {
        assert!(service.resolve_with_transaction(&owner.room, &owner.token, &review, true, Some(invalid_tx), generation(&service, &owner)).is_err());
    }
    service.state.lock().unwrap().db.execute_batch("CREATE TRIGGER reject_decision BEFORE UPDATE OF body ON reviews BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    assert_eq!(service.resolve_with_transaction(&owner.room, &owner.token, &review, true, Some(tx("😀abc", 2, 3, "X")), generation(&service, &owner)).unwrap_err().status, 503);
    assert_eq!(service.snapshot(&owner.room, &owner.token).unwrap()["content"], "😀abc");
    let history = service.reviews(&owner.room, &owner.token).unwrap();
    assert_eq!(history["reviews"][0]["status"], "open");
    assert!(history["reviews"][0]["body"]["_decision"].is_null());
    assert_eq!(generation(&service, &owner), Some(0));
    let state = service.state.lock().unwrap();
    assert_eq!(state.db.query_row("SELECT count(*) FROM updates", [], |r|r.get::<_,i64>(0)).unwrap(), 0);
}

#[test]
fn normal_accept_and_reject_record_decisions_without_a_manual_override() {
    let data = tempfile::tempdir().unwrap();
    let service = Service::open(&data.path().join("db")).unwrap();
    let owner = service.create("History", "abc", "Owner").unwrap();
    let review = service.review(&owner.room, &owner.token, json!({"text":"Change","transaction":tx("abc", 1, 2, "B")})).unwrap();
    service.resolve(&owner.room, &owner.token, &review, true).unwrap();
    let comment = service.review(&owner.room, &owner.token, json!({"text":"Keep this comment in history"})).unwrap();
    assert_eq!(service.resolve_with_transaction(&owner.room, &owner.token, &comment, true, Some(tx("aBc", 0, 3, "x")), generation(&service, &owner)).unwrap_err().status, 422);
    service.resolve(&owner.room, &owner.token, &comment, false).unwrap();
    let history = service.reviews(&owner.room, &owner.token).unwrap();
    let rows = history["reviews"].as_array().unwrap();
    assert_eq!(rows.iter().find(|r|r["id"]==review).unwrap()["body"]["_decision"]["method"], "accept");
    assert_eq!(rows.iter().find(|r|r["id"]==comment).unwrap()["body"]["_decision"]["method"], "reject");
    assert_eq!(service.snapshot(&owner.room, &owner.token).unwrap()["content"], "aBc");
}

#[test]
fn same_text_aba_manual_preview_is_stale_across_compaction_and_restart() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("db");
    let service = Service::open(&path).unwrap();
    let owner = service.create("ABA", "abc🦀", "Owner").unwrap();
    let make_review = || service.review(&owner.room, &owner.token, json!({"text":"Use B","transaction":tx("abc🦀",1,2,"B")})).unwrap();
    let manual = make_review();
    let automatic = make_review();
    let preview = service.review_preview(&owner.room, &owner.token, &manual).unwrap();
    let doc = client(&service, &owner);
    let text = doc.get_or_insert_text("content");
    let vector = doc.transact().state_vector();
    text.insert(&mut doc.transact_mut(), 5, "X");
    send(&service, &owner, &doc, &vector);
    let vector = doc.transact().state_vector();
    text.remove_range(&mut doc.transact_mut(), 5, 1);
    send(&service, &owner, &doc, &vector);
    assert_eq!(service.snapshot(&owner.room, &owner.token).unwrap()["revision"], preview["revision"]);
    // Duplicate deliveries still advance the durable journal clock, including
    // the 128th insert that compacts and deletes every journal sequence.
    for _ in 0..126 { send(&service, &owner, &doc, &vector); }
    assert_eq!(generation(&service, &owner), Some(128));
    assert_eq!(service.state.lock().unwrap().db.query_row("SELECT count(*) FROM updates",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    send(&service, &owner, &doc, &vector);
    assert_eq!(generation(&service, &owner), Some(129));
    drop(service);
    let service = Service::open(&path).unwrap();
    assert_eq!(generation(&service, &owner), Some(129));
    assert_eq!(service.snapshot(&owner.room, &owner.token).unwrap()["revision"], preview["revision"]);
    for expected in [preview["generation"].as_i64(), None] {
        assert_eq!(service.resolve_with_transaction(&owner.room,&owner.token,&manual,true,Some(tx("abc🦀",1,2,"Merged")),expected).unwrap_err().status,409);
    }
    let history = service.reviews(&owner.room,&owner.token).unwrap();
    assert!(history["reviews"].as_array().unwrap().iter().all(|r|r["status"]=="open"&&r["body"]["_decision"].is_null()));
    // Unrelated ABA leaves the trusted relative range intact, so the ordinary
    // accept path can safely relocate it despite the old proposal generation.
    service.resolve(&owner.room,&owner.token,&automatic,true).unwrap();
    assert_eq!(service.snapshot(&owner.room,&owner.token).unwrap()["content"],"aBc🦀");
    let fresh = service.review_preview(&owner.room,&owner.token,&manual).unwrap();
    assert_eq!(fresh["generation"],130);
    service.resolve_with_transaction(&owner.room,&owner.token,&manual,true,Some(tx("aBc🦀",1,2,"Merged")),fresh["generation"].as_i64()).unwrap();
    let history = service.reviews(&owner.room,&owner.token).unwrap();
    let decision = &history["reviews"].as_array().unwrap().iter().find(|r|r["id"]==manual).unwrap()["body"]["_decision"];
    assert_eq!(decision["base_generation"],130);
    assert_eq!(decision["result_generation"],131);
}

#[test]
fn legacy_clock_migration_removes_untrusted_generations_and_keeps_anchor_review_usable() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("db");
    let service = Service::open(&path).unwrap();
    let owner = service.create("Legacy", "abc", "Owner").unwrap();
    let review = service.review(&owner.room,&owner.token,json!({"text":"Legacy proposal","transaction":tx("abc",1,2,"B")})).unwrap();
    let doc = client(&service,&owner);
    let vector = doc.transact().state_vector();
    doc.get_or_insert_text("content").insert(&mut doc.transact_mut(),0,"Note: ");
    send(&service,&owner,&doc,&vector);
    drop(service);
    {
        let db = Connection::open(&path).unwrap();
        db.execute_batch("DROP TRIGGER collaboration_review_clock_update; DROP TRIGGER collaboration_review_clock_room; DROP TABLE review_clock;").unwrap();
        let raw: String = db.query_row("SELECT body FROM reviews WHERE id=?1",[&review],|r|r.get(0)).unwrap();
        let mut body: Value = serde_json::from_str(&raw).unwrap();
        body["_generation"] = json!(1); // An old caller could have supplied this.
        db.execute("UPDATE reviews SET body=?2 WHERE id=?1",params![review,body.to_string()]).unwrap();
    }
    let service = Service::open(&path).unwrap();
    assert_eq!(generation(&service,&owner),Some(1));
    assert!(service.reviews(&owner.room,&owner.token).unwrap()["reviews"][0]["body"]["_generation"].is_null());
    let preview = service.review_preview(&owner.room,&owner.token,&review).unwrap();
    assert_eq!(preview["edits"][0]["from"],7);
    service.resolve(&owner.room,&owner.token,&review,true).unwrap();
    assert_eq!(service.snapshot(&owner.room,&owner.token).unwrap()["content"],"Note: aBc");
    assert_eq!(generation(&service,&owner),Some(2));
}
