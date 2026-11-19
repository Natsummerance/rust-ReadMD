use super::*;
use rusqlite::Connection;
use std::fs;
use tempfile::TempDir;

fn open_document(service: &Service, path: &str, raw: &str) -> Value {
    service
        .handle(&json!({"action":"open","path":path,"content":raw,
        "disk_content":raw,"disk_revision":document_transaction::revision(raw),
        "encoding":if raw.starts_with('\u{feff}'){"utf-8-sig"}else{"utf-8"},
        "line_ending":if raw.contains("\r\n"){"crlf"}else{"lf"}}))
        .unwrap()
}
fn request(snapshot: &Value, action: &str, call: &str, extra: Value) -> Value {
    let mut request = json!({"action":action,"document_id":snapshot["document_id"],
        "generation":snapshot["generation"],"review_revision":snapshot["review_revision"],"call_id":call});
    for (key, value) in extra.as_object().unwrap() {
        request[key] = value.clone();
    }
    request
}
fn change(service: &Service, snapshot: &Value, action: &str, call: &str, extra: Value) -> Value {
    service
        .handle(&request(snapshot, action, call, extra))
        .unwrap()
}
fn read(service: &Service, snapshot: &Value) -> Value {
    service
        .handle(&json!({"action":"read","document_id":snapshot["document_id"]}))
        .unwrap()
}
fn utf16_range(source: &str, needle: &str, occurrence: usize) -> (usize, usize) {
    let byte = source.match_indices(needle).nth(occurrence).unwrap().0;
    let from = source[..byte].encode_utf16().count();
    (from, from + needle.encode_utf16().count())
}
fn events(directory: &TempDir) -> i64 {
    Connection::open(directory.path().join("review.sqlite3"))
        .unwrap()
        .query_row("SELECT COUNT(*) FROM review_events", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn undo_restores_exact_pending_ranges_after_enclosing_manual_replacement_and_restart() {
    let directory = TempDir::new().unwrap();
    let service = Service::new(directory.path());
    let original = "前😀\nA orig C\nA orig C\n";
    let pending_body = "前😀\nA orig C\nA next C\n";
    let manual_body = "前😀\nA orig C\nA next C\n\nTail\n";
    let opened = open_document(&service, "/exact-undo.md", original);
    let proposed = change(&service, &opened, "propose", "exact-undo-propose", json!({
        "origin":"ai", "edits":[{"from":15,"to":19,"insert":"next"}]
    }));
    let id = proposed["changes"][0]["id"].clone();
    assert_eq!(proposed["content"], pending_body);
    let mut current = change(&service, &proposed, "edit", "exact-undo-manual", json!({
        "origin":"editor", "edits":[{"from":13,"to":22,"insert":"A next C\n\nTail\n"}]
    }));
    assert_eq!(current["content"], manual_body);
    assert_eq!(current["changes"][0]["decision"], "conflict");
    assert_eq!(current["changes"][0]["taken_over"], true);
    for round in 0..2 {
        current = change(&service, &current, "undo", &format!("exact-undo-{round}"), json!({}));
        assert_eq!(current["content"], pending_body);
        let restored = &current["changes"][0];
        assert_eq!(restored["id"], id);
        assert_eq!(restored["from"], 15);
        assert_eq!(restored["to"], 19);
        assert_eq!(restored["original"], "orig");
        assert_eq!(restored["replacement"], "next");
        assert_eq!(restored["decision"], "pending");
        assert_eq!(restored["application"], "applied");
        assert_eq!(restored["taken_over"], false);
        if round == 0 {
            current = change(&service, &current, "redo", "exact-redo", json!({}));
            assert_eq!(current["content"], manual_body);
            assert_eq!(current["changes"][0]["decision"], "conflict");
            assert_eq!(current["changes"][0]["taken_over"], true);
        }
    }
    drop(service);
    let service = Service::new(directory.path());
    let reopened = read(&service, &current);
    assert_eq!(reopened["content"], pending_body);
    assert_eq!(reopened["changes"][0]["id"], id);
    assert_eq!(reopened["changes"][0]["from"], 15);
    assert_eq!(reopened["changes"][0]["to"], 19);
    let withdrawn = change(&service, &reopened, "resolve", "exact-withdraw", json!({
        "decision":"withdraw", "ids":[id]
    }));
    assert_eq!(withdrawn["content"], original);
    assert_eq!(withdrawn["changes"][0]["decision"], "withdrawn");
}

#[test]
fn export_acceptance_is_scoped_to_frozen_versions_and_survives_restart() {
    let directory = TempDir::new().unwrap();
    let service = Service::new(directory.path());
    let original = open_document(&service, "/export-review.md", "A B C");
    let proposed = change(&service, &original, "propose", "export-ab", json!({"origin":"ai","task_id":"owner",
        "edits":[{"from":0,"to":1,"insert":"a"},{"from":2,"to":3,"insert":"b"}]}));
    let frozen = service.freeze_export_snapshot(&proposed).unwrap();
    let later = change(&service, &proposed, "propose", "export-c", json!({"origin":"ai","task_id":"new-owner",
        "edits":[{"from":4,"to":5,"insert":"c"}]}));
    let accepted = service.accept_export_snapshot(&frozen, "verified-export").unwrap();
    assert_eq!(accepted["accepted_count"], 2);
    assert_eq!(accepted["conflict_count"], 0);
    assert_eq!(accepted["content"], later["content"]);
    assert_eq!(accepted["generation"], later["generation"]);
    assert_eq!(accepted["changes"][2]["decision"], "pending");
    assert_eq!(accepted["changes"][0]["original"], "A");
    let after_events = events(&directory);
    drop(service);
    let service = Service::new(directory.path());
    assert_eq!(service.accept_export_snapshot(&frozen, "verified-export").unwrap(), accepted);
    assert_eq!(events(&directory), after_events);
    let undone = change(&service, &accepted, "undo", "undo-export", json!({}));
    assert_eq!(undone["changes"][0]["decision"], "pending");
    assert_eq!(undone["changes"][1]["decision"], "pending");
    assert_eq!(undone["changes"][2]["decision"], "pending");
    assert_eq!(undone["content"], later["content"]);
}

#[test]
fn export_acceptance_reports_touched_and_withdrawn_candidates_without_accepting_them() {
    let directory = TempDir::new().unwrap();
    let service = Service::new(directory.path());
    let original = open_document(&service, "/export-conflict.md", "A B D");
    let proposed = change(&service, &original, "propose", "export-abd", json!({"origin":"ai","task_id":"owner",
        "edits":[{"from":0,"to":1,"insert":"a"},{"from":2,"to":3,"insert":"b"},{"from":4,"to":5,"insert":"d"}]}));
    let frozen = service.freeze_export_snapshot(&proposed).unwrap();
    let touched = change(&service, &proposed, "edit", "touch-d", json!({"edits":[{"from":4,"to":5,"insert":"D"}]}));
    let withdrawn = change(&service, &touched, "resolve", "withdraw-b", json!({"decision":"withdraw","ids":[touched["changes"][1]["id"]]}));
    let accepted = service.accept_export_snapshot(&frozen, "partial-export").unwrap();
    assert_eq!(accepted["accepted_count"], 1);
    assert_eq!(accepted["conflict_count"], 2);
    assert_eq!(accepted["changes"][0]["decision"], "accepted");
    assert_eq!(accepted["changes"][1]["decision"], "withdrawn");
    assert_eq!(accepted["changes"][2]["decision"], "conflict");
    assert_eq!(accepted["content"], withdrawn["content"]);
}

#[test]
fn export_acceptance_preserves_dependency_ownership_and_rejects_forged_content() {
    let directory = TempDir::new().unwrap();
    let service = Service::new(directory.path());
    let original = open_document(&service, "/export-dependency.md", "A B");
    let a = change(&service, &original, "propose", "dependency-a", json!({"origin":"ai","task_id":"owner",
        "edits":[{"from":0,"to":1,"insert":"a"}]}));
    let b = change(&service, &a, "propose", "dependency-b", json!({"origin":"ai","task_id":"owner-b", "dependencies":[a["changes"][0]["id"]],
        "edits":[{"from":2,"to":3,"insert":"b"}]}));
    let frozen = service.freeze_export_snapshot(&b).unwrap();
    let mut forged = frozen.clone(); forged["content"] = json!("forged");
    assert_eq!(service.accept_export_snapshot(&forged, "forged-export").unwrap_err(), "review_conflict");
    let taken_over = change(&service, &b, "edit", "take-a", json!({"edits":[{"from":0,"to":1,"insert":"A"}]}));
    let accepted = service.accept_export_snapshot(&frozen, "dependency-export").unwrap();
    assert_eq!(accepted["accepted_count"], 0);
    assert_eq!(accepted["conflict_count"], 2);
    assert_eq!(accepted["content"], taken_over["content"]);
    let mut altered = frozen.clone(); altered["pending_changes"][0]["task_id"] = json!("different-owner");
    assert_eq!(service.accept_export_snapshot(&altered, "dependency-export").unwrap_err(), "review_call_conflict");
}

#[test]
fn export_freeze_refuses_unapplied_proposal_and_has_no_public_accept_action() {
    let directory = TempDir::new().unwrap();
    let service = Service::new(directory.path());
    let original = open_document(&service, "/export-unapplied.md", "A");
    let proposed = change(&service, &original, "propose", "unapplied", json!({"application":"proposed",
        "edits":[{"from":0,"to":1,"insert":"a"}]}));
    assert_eq!(service.freeze_export_snapshot(&proposed).unwrap_err(), "review_conflict");
    assert_eq!(service.handle(&request(&proposed, "accept_export_snapshot", "public-forbidden", json!({}))).unwrap_err(), "review_invalid_request");
}

#[test]
fn trusted_task_followup_preserves_ai_owner_id_original_and_receipt() {
    let directory=TempDir::new().unwrap(); let service=Service::new(directory.path());
    let original=open_document(&service,"draft:task-followup","Original tail");
    let first_request=request(&original,"propose","owned-first",json!({"origin":"ai","task_id":"trusted-task",
        "edits":[{"from":0,"to":8,"insert":"Candidate"}],"dependencies":[]}));
    let first=service.propose_task(&first_request).unwrap();
    let owner=first["changes"][0]["id"].clone();
    let followup=request(&first,"propose","owned-followup",json!({"origin":"ai","task_id":"trusted-task",
        "edits":[{"from":0,"to":9,"insert":"Improved"}],"dependencies":[owner]}));
    let second=service.propose_task(&followup).unwrap();
    assert_eq!(second["changes"].as_array().unwrap().len(),1);
    assert_eq!(second["changes"][0]["id"],owner);
    assert_eq!(second["changes"][0]["original"],"Original");
    assert_eq!(second["changes"][0]["replacement"],"Improved");
    assert_eq!(second["changes"][0]["origin"],"ai");
    assert_eq!(second["changes"][0]["task_id"],"trusted-task");
    assert_eq!(second["changes"][0]["taken_over"],false);
    assert_eq!(second["changes"][0]["decision"],"pending");
    assert!(second["changes"][0]["candidate_versions"].as_array().unwrap().contains(&json!("Candidate")));
    assert_eq!(service.propose_task(&followup).unwrap(),second);
    let receipt=service.handle(&json!({"action":"receipt","document_id":second["document_id"],"call_id":"owned-followup"})).unwrap();
    assert_eq!(receipt["request"],followup);
    assert_eq!(service.handle(&followup).unwrap_err(),"review_call_conflict");
    let withdrawn=change(&service,&second,"resolve","withdraw-owned",json!({"decision":"withdraw","ids":[owner]}));
    assert_eq!(withdrawn["content"],"Original tail");
}

#[test]
fn trusted_task_followup_rejects_foreign_accepted_taken_over_and_unowned_extension() {
    for mode in ["foreign","accepted","taken_over","extension"] {
        let directory=TempDir::new().unwrap();let service=Service::new(directory.path());
        let original=open_document(&service,"draft:task-ownership","A tail");
        let mut current=service.propose_task(&request(&original,"propose","owner",json!({"origin":"ai","task_id":"first-task",
            "edits":[{"from":0,"to":1,"insert":"a"}],"dependencies":[]}))).unwrap();
        let owner=current["changes"][0]["id"].clone();
        if mode=="accepted" { current=change(&service,&current,"resolve","accept-owner",json!({"decision":"accept","ids":[owner]})); }
        if mode=="taken_over" { current=change(&service,&current,"edit","human-takeover",json!({"edits":[{"from":0,"to":1,"insert":"H"}]})); }
        let followup=request(&current,"propose","reject-followup",json!({"origin":"ai","task_id":if mode=="foreign"{"other-task"}else{"first-task"},
            "edits":[{"from":0,"to":if mode=="extension"{3}else{1},"insert":"replacement"}],"dependencies":[owner]}));
        let before=read(&service,&current);
        assert_eq!(service.propose_task(&followup).unwrap_err(),"review_conflict","{mode}");
        assert_eq!(read(&service,&current),before,"{mode}");
    }
}
fn save(service: &Service, snapshot: &Value, call: &str) -> (Value, String) {
    let content = snapshot["content"].as_str().unwrap();
    let disk_text = if snapshot["line_ending"] == "crlf" {
        content.replace('\n', "\r\n")
    } else {
        content.into()
    };
    let raw = if snapshot["encoding"] == "utf-8-sig" {
        format!("\u{feff}{disk_text}")
    } else {
        disk_text
    };
    let revision = document_transaction::revision(&raw);
    let prepared = change(
        service,
        snapshot,
        "prepare_save",
        &format!("{call}-prepare"),
        json!({
        "content":content,"disk_revision":snapshot["disk_revision"],"expected_disk_revision":revision}),
    );
    let committed=service.handle(&json!({"action":"commit_save","document_id":snapshot["document_id"],
        "intent_id":prepared["intent_id"],"call_id":format!("{call}-commit"),"disk_revision":revision,"content":content})).unwrap();
    (committed, raw)
}

#[test]
fn review_same_text_blocks_manual_edit_accept_save_restart_and_individual_withdrawals() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("Review.md");
    let raw =
        "\u{feff}---\r\n# retained metadata\r\n---\r\n甲😀 original\r\nC\r\n甲😀 original\r\n";
    fs::write(&path, raw).unwrap();
    let document_path = path.to_string_lossy();
    let service = Service::new(directory.path());
    let initial = open_document(&service, &document_path, raw);
    assert!(!initial["content"].as_str().unwrap().starts_with('\u{feff}'));
    assert!(!initial["content"].as_str().unwrap().contains('\r'));
    let (c, _) = utf16_range(initial["content"].as_str().unwrap(), "C", 0);
    let session = change(
        &service,
        &initial,
        "edit",
        "session",
        json!({"edits":[{"from":c,"to":c,"insert":"SESSION_ONLY "}]}),
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), raw);
    let source = session["content"].as_str().unwrap();
    let (af, at) = utf16_range(source, "original", 0);
    let (bf, bt) = utf16_range(source, "original", 1);
    let proposed = change(
        &service,
        &session,
        "propose",
        "ai-ab",
        json!({"origin":"ai","task_id":"fixture-task",
        "edits":[{"from":af,"to":at,"insert":"AI-reviewed"},{"from":bf,"to":bt,"insert":"AI-reviewed"}]}),
    );
    let a = proposed["changes"][0]["id"].clone();
    let b = proposed["changes"][1]["id"].clone();
    assert_ne!(a, b);
    let (c, _) = utf16_range(proposed["content"].as_str().unwrap(), "C", 0);
    let human = change(
        &service,
        &proposed,
        "edit",
        "human",
        json!({"edits":[{"from":c+1,"to":c+1,"insert":" HUMAN_STAYS"}]}),
    );
    let accepted = change(
        &service,
        &human,
        "resolve",
        "accept-a",
        json!({"ids":[a],"decision":"accept"}),
    );
    assert_eq!(accepted["content"], human["content"]);
    assert_eq!(accepted["generation"], human["generation"]);
    let (saved, saved_raw) = save(&service, &accepted, "save-one");
    fs::write(&path, &saved_raw).unwrap();
    drop(service);
    let service = Service::new(directory.path());
    let reopened = open_document(
        &service,
        &document_path,
        &fs::read_to_string(&path).unwrap(),
    );
    assert_eq!(reopened["document_id"], saved["document_id"]);
    assert_eq!(reopened["changes"][0]["decision"], "accepted");
    let withdrawn = change(
        &service,
        &reopened,
        "resolve",
        "withdraw-b",
        json!({"ids":[b],"decision":"withdraw"}),
    );
    assert_eq!(withdrawn["content"],"---\n# retained metadata\n---\n甲😀 AI-reviewed\nSESSION_ONLY C HUMAN_STAYS\n甲😀 original\n");
    let (_, saved_raw) = save(&service, &withdrawn, "save-two");
    fs::write(&path, &saved_raw).unwrap();
    drop(service);
    let service = Service::new(directory.path());
    let reopened = open_document(&service, &document_path, &saved_raw);
    let withdrawn = change(
        &service,
        &reopened,
        "resolve",
        "withdraw-accepted-a",
        json!({"ids":[a],"decision":"withdraw"}),
    );
    assert_eq!(
        withdrawn["content"],
        "---\n# retained metadata\n---\n甲😀 original\nSESSION_ONLY C HUMAN_STAYS\n甲😀 original\n"
    );
    assert_eq!(withdrawn["changes"][0]["decision"], "withdrawn");
    let (_, final_raw) = save(&service, &withdrawn, "save-three");
    fs::write(&path, &final_raw).unwrap();
    drop(service);
    let service = Service::new(directory.path());
    let final_state = open_document(
        &service,
        &document_path,
        &fs::read_to_string(&path).unwrap(),
    );
    assert!(final_state["changes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|change| change["decision"] == "withdrawn"));
    let db = Connection::open(directory.path().join("review.sqlite3")).unwrap();
    let accepted_events: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM review_events WHERE kind='resolve' AND payload LIKE '%accepted%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(accepted_events >= 1);
}

#[test]
fn review_persistent_undo_redo_keeps_monotonic_generation_for_same_text() {
    let directory = TempDir::new().unwrap();
    let service = Service::new(directory.path());
    let initial = open_document(&service, "draft:undo", "😀x");
    let edited = change(
        &service,
        &initial,
        "edit",
        "edit",
        json!({"edits":[{"from":2,"to":3,"insert":"y"}]}),
    );
    let undone = change(&service, &edited, "undo", "undo", json!({}));
    assert_eq!(undone["content"], initial["content"]);
    assert_eq!(undone["revision"], initial["revision"]);
    assert!(undone["generation"].as_u64().unwrap() > initial["generation"].as_u64().unwrap());
    assert_eq!(
        service
            .handle(&request(
                &initial,
                "propose",
                "late-ai",
                json!({"edits":[{"from":2,"to":3,"insert":"late"}]})
            ))
            .unwrap_err(),
        "review_stale_generation"
    );
    drop(service);
    let service = Service::new(directory.path());
    let redone = change(
        &service,
        &read(&service, &undone),
        "redo",
        "redo",
        json!({}),
    );
    assert_eq!(redone["content"], "😀y");
    let db = Connection::open(directory.path().join("review.sqlite3")).unwrap();
    let payload: String = db
        .query_row(
            "SELECT payload FROM review_events WHERE kind='undo'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&payload).unwrap()["compensates_event_id"],
        1
    );
}

#[test]
fn review_same_call_receipt_survives_restart_and_different_input_is_rejected() {
    let directory = TempDir::new().unwrap();
    let service = Service::new(directory.path());
    let initial = open_document(&service, "draft:receipt", "same same");
    let proposed_request = request(
        &initial,
        "propose",
        "one-call",
        json!({"ai_request_digest":"digest-one",
        "edits":[{"from":5,"to":9,"insert":"changed"}]}),
    );
    let result = service.handle(&proposed_request).unwrap();
    drop(service);
    let service = Service::new(directory.path());
    assert_eq!(
        service.handle(&canonical(&proposed_request)).unwrap(),
        result
    );
    assert_eq!(events(&directory), 1);
    let found = service
        .handle(
            &json!({"action":"receipt","document_id":initial["document_id"],"call_id":"one-call"}),
        )
        .unwrap();
    assert_eq!(found["request"], proposed_request);
    assert_eq!(found["snapshot"], result);
    assert_eq!(found["found"], true);
    let mut altered = proposed_request;
    altered["ai_request_digest"] = json!("digest-two");
    assert_eq!(
        service.handle(&altered).unwrap_err(),
        "review_call_conflict"
    );
    assert_eq!(events(&directory), 1);
}

#[test]
fn review_overlapping_human_edit_retains_versions_and_bulk_withdraw_rolls_back() {
    let directory = TempDir::new().unwrap();
    let service = Service::new(directory.path());
    let initial = open_document(&service, "draft:overlap", "aaaa BBBB");
    let proposed = change(
        &service,
        &initial,
        "propose",
        "ab",
        json!({"edits":[{"from":0,"to":4,"insert":"AAAA"},{"from":5,"to":9,"insert":"bbbb"}]}),
    );
    let ids = proposed["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].clone())
        .collect::<Vec<_>>();
    let human = change(
        &service,
        &proposed,
        "edit",
        "human",
        json!({"edits":[{"from":6,"to":7,"insert":"HUMAN"}]}),
    );
    assert_eq!(human["changes"][1]["decision"], "conflict");
    assert_eq!(human["changes"][1]["original"], "BBBB");
    assert_eq!(human["changes"][1]["replacement"], "bbbb");
    assert_eq!(human["changes"][1]["current"], "bHUMANbb");
    let count = events(&directory);
    assert_eq!(
        service
            .handle(&request(
                &human,
                "resolve",
                "withdraw-all",
                json!({"ids":ids,"decision":"withdraw"})
            ))
            .unwrap_err(),
        "review_conflict"
    );
    assert_eq!(read(&service, &human), read(&service, &human));
    assert_eq!(read(&service, &human)["content"], human["content"]);
    assert_eq!(events(&directory), count);
    let undone = change(&service, &human, "undo", "undo-human", json!({}));
    assert_eq!(undone["changes"][1]["decision"], "pending");
    assert_eq!(undone["changes"][1]["taken_over"], false);
    let withdrawn = change(
        &service,
        &undone,
        "resolve",
        "now-withdraw-all",
        json!({"ids":ids,"decision":"withdraw"}),
    );
    assert_eq!(withdrawn["content"], initial["content"]);
}

#[test]
fn review_surrogate_and_overlapping_transactions_do_not_write_events() {
    let directory = TempDir::new().unwrap();
    let service = Service::new(directory.path());
    let initial = open_document(&service, "draft:range", "😀abc");
    for (extra, error) in [
        (
            json!({"edits":[{"from":1,"to":2,"insert":"x"}]}),
            "transaction_invalid_range",
        ),
        (
            json!({"edits":[{"from":2,"to":4,"insert":"x"},{"from":3,"to":4,"insert":"y"}]}),
            "transaction_overlapping_edits",
        ),
    ] {
        assert_eq!(
            service
                .handle(&request(&initial, "propose", error, extra))
                .unwrap_err(),
            error
        );
    }
    assert_eq!(events(&directory), 0);
    assert_eq!(read(&service, &initial)["content"], "😀abc");
}

#[test]
fn review_budget_failure_and_sqlite_failure_rollback_content_events_and_receipts() {
    let directory = TempDir::new().unwrap();
    let service = Service::new(directory.path());
    let initial = open_document(&service, "draft:budget", "keep");
    let request = request(
        &initial,
        "propose",
        "budget-call",
        json!({"edits":[{"from":0,"to":4,"insert":"new"}]}),
    );
    let tiny = Service::with_limit(directory.path(), 1);
    assert_eq!(tiny.handle(&request).unwrap_err(), "review_budget");
    assert_eq!(events(&directory), 0);
    assert_eq!(read(&service, &initial)["content"], "keep");
    let db = Connection::open(directory.path().join("review.sqlite3")).unwrap();
    db.execute_batch("CREATE TRIGGER review_test_fail BEFORE INSERT ON review_receipts BEGIN SELECT RAISE(ABORT,'injected write failure'); END;").unwrap();
    assert_eq!(
        service.handle(&request).unwrap_err(),
        "review_persistence_failed"
    );
    assert_eq!(events(&directory), 0);
    assert_eq!(read(&service, &initial)["changes"], json!([]));
    assert_eq!(read(&service, &initial)["content"], "keep");
    db.execute_batch("DROP TRIGGER review_test_fail").unwrap();
    let done = service.handle(&request).unwrap();
    assert_eq!(done["content"], "new");
    assert_eq!(events(&directory), 1);
}

#[test]
fn review_migration_failure_rolls_back_and_newer_schema_is_untouched() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("review.sqlite3");
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE review_documents(sentinel TEXT);INSERT INTO review_documents VALUES('retained')").unwrap();
    let service = Service::new(directory.path());
    assert_eq!(
        service
            .handle(&json!({"action":"lookup","path":"anything"}))
            .unwrap_err(),
        "review_persistence_failed"
    );
    let version: i64 = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 0);
    let created: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='review_receipts'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(created, 0);
    let sentinel: String = db
        .query_row("SELECT sentinel FROM review_documents", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(sentinel, "retained");
    db.pragma_update(None, "user_version", 99).unwrap();
    assert_eq!(
        service
            .handle(&json!({"action":"lookup","path":"anything"}))
            .unwrap_err(),
        "review_unsupported_schema"
    );
    assert_eq!(
        db.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        99
    );
}

#[test]
fn review_save_intent_blocks_edits_and_before_disk_recovery_aborts_only_intent() {
    let directory = TempDir::new().unwrap();
    let service = Service::new(directory.path());
    let initial = open_document(&service, "draft:save", "old");
    let proposed = change(
        &service,
        &initial,
        "propose",
        "proposal",
        json!({"edits":[{"from":0,"to":3,"insert":"candidate"}]}),
    );
    let prepared = change(
        &service,
        &proposed,
        "prepare_save",
        "prepare",
        json!({"content":"candidate","disk_revision":proposed["disk_revision"],"expected_disk_revision":document_transaction::revision("candidate")}),
    );
    assert_eq!(
        service
            .handle(&request(
                &prepared,
                "edit",
                "blocked",
                json!({"edits":[{"from":0,"to":0,"insert":"no"}]})
            ))
            .unwrap_err(),
        "review_save_in_progress"
    );
    drop(service);
    let service = Service::new(directory.path());
    let reopened = open_document(&service, "draft:save", "old");
    assert!(reopened["save_intent"].is_null());
    assert_eq!(reopened["content"], "candidate");
    assert_eq!(reopened["changes"][0]["original"], "old");
}

#[test]
fn review_save_crash_after_rename_finishes_and_unexpected_disk_is_preserved() {
    let directory = TempDir::new().unwrap();
    let service = Service::new(directory.path());
    let initial = open_document(&service, "draft:committed", "old");
    let proposed = change(
        &service,
        &initial,
        "propose",
        "propose",
        json!({"edits":[{"from":0,"to":3,"insert":"candidate"}]}),
    );
    let prepared = change(
        &service,
        &proposed,
        "prepare_save",
        "prepare",
        json!({"content":"candidate","disk_revision":proposed["disk_revision"],"expected_disk_revision":document_transaction::revision("candidate")}),
    );
    drop(service);
    let service = Service::new(directory.path());
    let reopened = open_document(&service, "draft:committed", "candidate");
    assert!(reopened["save_intent"].is_null());
    assert_eq!(
        reopened["disk_revision"],
        document_transaction::revision("candidate")
    );
    assert_eq!(reopened["changes"], prepared["changes"]);
    let external = open_document(&service, "draft:committed", "external content");
    assert_eq!(external["status"], "needs_recovery");
    assert_eq!(external["content"], "candidate");
    assert_eq!(external["recovery"]["external_content"], "external content");
    assert_eq!(
        service
            .handle(&request(
                &external,
                "resolve",
                "blocked",
                json!({"ids":[external["changes"][0]["id"]],"decision":"withdraw"})
            ))
            .unwrap_err(),
        "review_needs_recovery"
    );
}

#[test]
fn review_save_precise_system_rewrite_maps_review_and_validates_entire_transform() {
    let directory = TempDir::new().unwrap();
    let service = Service::new(directory.path());
    let initial = open_document(&service, "draft:rewrite", "asset A original");
    let proposed = change(
        &service,
        &initial,
        "propose",
        "propose",
        json!({"edits":[{"from":8,"to":16,"insert":"candidate"}]}),
    );
    let malformed = request(
        &proposed,
        "prepare_save",
        "bad-plan",
        json!({"content":proposed["content"],"disk_revision":proposed["disk_revision"],
        "expected_disk_revision":"encoded-sha","saved_content":"unproven replacement"}),
    );
    assert_eq!(
        service.handle(&malformed).unwrap_err(),
        "review_save_conflict"
    );
    assert_eq!(events(&directory), 1);
    let saved = "longer-asset A candidate";
    let prepared = change(
        &service,
        &proposed,
        "prepare_save",
        "good-plan",
        json!({"content":proposed["content"],"disk_revision":proposed["disk_revision"],
        "expected_disk_revision":document_transaction::revision(saved),"saved_content":saved,
        "edit_steps":[{"edits":[{"from":0,"to":5,"insert":"longer-asset"}]}]}),
    );
    let committed=service.handle(&json!({"action":"commit_save","document_id":proposed["document_id"],"intent_id":prepared["intent_id"],
        "call_id":"commit","disk_revision":document_transaction::revision(saved),"content":saved})).unwrap();
    let withdrawn = change(
        &service,
        &committed,
        "resolve",
        "withdraw",
        json!({"ids":[committed["changes"][0]["id"]],"decision":"withdraw"}),
    );
    assert_eq!(withdrawn["content"], "longer-asset A original");
    assert_eq!(withdrawn["changes"][0]["from"], 15);
}

#[test]
fn review_save_as_rebinds_only_after_commit_and_reserves_target_path() {
    let directory = TempDir::new().unwrap();
    let service = Service::new(directory.path());
    let initial = open_document(&service, "draft:saveas", "body");
    let prepared = change(
        &service,
        &initial,
        "prepare_save",
        "prepare",
        json!({"content":"body","disk_revision":initial["disk_revision"],
        "target_path":"target.md","target_disk_revision":"missing","expected_disk_revision":document_transaction::revision("body")}),
    );
    assert_eq!(prepared["path"], "draft:saveas");
    let found = service
        .handle(&json!({"action":"lookup","path":"target.md"}))
        .unwrap();
    assert_eq!(found["snapshot"]["document_id"], initial["document_id"]);
    let another = open_document(&service, "draft:other", "body");
    assert_eq!(service.handle(&request(&another,"prepare_save","another-prepare",json!({"content":"body","disk_revision":another["disk_revision"],
        "target_path":"target.md","target_disk_revision":"missing","expected_disk_revision":document_transaction::revision("body")}))).unwrap_err(),"review_save_conflict");
    let source_reopened = open_document(&service, "draft:saveas", "body");
    assert_eq!(source_reopened["status"], "needs_recovery");
    let committed=service.handle(&json!({"action":"commit_save","document_id":initial["document_id"],"intent_id":prepared["intent_id"],
        "call_id":"commit","disk_revision":document_transaction::revision("body"),"content":"body"})).unwrap();
    assert_eq!(committed["path"], "target.md");
    assert_eq!(committed["status"], "ready");
    assert_eq!(
        service
            .handle(&json!({"action":"lookup","path":"draft:saveas"}))
            .unwrap()["found"],
        false
    );
}

#[test]
fn review_proposed_application_keeps_content_and_accept_never_reinserts() {
    let directory = TempDir::new().unwrap();
    let service = Service::new(directory.path());
    let initial = open_document(&service, "draft:proposal", "original");
    let proposed = change(
        &service,
        &initial,
        "propose",
        "propose",
        json!({"application":"proposed","edits":[{"from":0,"to":8,"insert":"candidate"}]}),
    );
    assert_eq!(proposed["content"], "original");
    assert_eq!(proposed["generation"], initial["generation"]);
    let accepted = change(
        &service,
        &proposed,
        "resolve",
        "accept",
        json!({"ids":[proposed["changes"][0]["id"]],"decision":"accept"}),
    );
    assert_eq!(accepted["content"], "original");
    assert_eq!(accepted["changes"][0]["application"], "proposed");
}

#[test]
fn review_asset_original_remains_pinned_after_save_rewrite_and_history_prune() {
    let directory = TempDir::new().unwrap();
    let service = Service::new(directory.path());
    let asset = directory
        .path()
        .join("resource-staging")
        .join("retained.svg");
    let reference = format!(
        "/raw?p={}",
        percent_encoding::utf8_percent_encode(
            &asset.to_string_lossy(),
            percent_encoding::NON_ALPHANUMERIC
        )
    );
    let original = format!("![kept]({reference})");
    let initial = open_document(&service, "draft:pins", &original);
    let proposed = change(
        &service,
        &initial,
        "propose",
        "replace-image",
        json!({"edits":[{"from":0,"to":original.encode_utf16().count(),"insert":"text"}]}),
    );
    assert_eq!(
        service.pinned_assets().unwrap(),
        vec![asset.to_string_lossy().into_owned()]
    );
    // Exercise the actual ordinary history's ten-entry pruning policy.
    for index in 0..20 {
        crate::document_history::record(
            directory.path(),
            "draft:pins",
            "",
            "pins.md",
            "version",
            "save",
            &format!("ordinary-{index}"),
        )
        .unwrap();
    }
    assert_eq!(
        crate::document_history::list(directory.path(), Some("draft:pins"))
            .unwrap()
            .len(),
        10
    );
    drop(service);
    let service = Service::new(directory.path());
    assert_eq!(service.pinned_assets().unwrap().len(), 1);
    let withdrawn = change(
        &service,
        &read(&service, &proposed),
        "resolve",
        "withdraw",
        json!({"ids":[proposed["changes"][0]["id"]],"decision":"withdraw"}),
    );
    assert_eq!(withdrawn["content"], original);
}

#[test]
fn review_independent_connections_use_one_cas_and_preserve_unsaved_content() {
    let directory = TempDir::new().unwrap();
    let first = Service::new(directory.path());
    let second = Service::new(directory.path());
    let initial = open_document(&first, "draft:cas", "old");
    let edited = change(
        &first,
        &initial,
        "edit",
        "one",
        json!({"edits":[{"from":0,"to":3,"insert":"unsaved"}]}),
    );
    assert_eq!(
        second
            .handle(&request(
                &initial,
                "edit",
                "two",
                json!({"edits":[{"from":0,"to":3,"insert":"stale"}]})
            ))
            .unwrap_err(),
        "review_stale_generation"
    );
    let reopened = open_document(&second, "draft:cas", "old");
    assert_eq!(reopened["content"], edited["content"]);
    assert_eq!(reopened["document_id"], initial["document_id"]);
}

#[test]
fn review_discard_restores_saved_anchors_preserves_acceptance_and_keeps_unsaved_baselines() {
    let directory = TempDir::new().unwrap();
    let service = Service::new(directory.path());
    let initial = open_document(&service, "draft:discard", "A old B old C");
    let proposed = change(
        &service,
        &initial,
        "propose",
        "ab",
        json!({"edits":[
        {"from":2,"to":5,"insert":"new-a"},{"from":8,"to":11,"insert":"new-b"}]}),
    );
    let a = proposed["changes"][0]["id"].clone();
    let b = proposed["changes"][1]["id"].clone();
    let (saved, raw) = save(&service, &proposed, "save");
    let accepted = change(
        &service,
        &saved,
        "resolve",
        "accept-a",
        json!({"ids":[a],"decision":"accept"}),
    );
    let withdrawn = change(
        &service,
        &accepted,
        "resolve",
        "withdraw-b",
        json!({"ids":[b],"decision":"withdraw"}),
    );
    let human = change(
        &service,
        &withdrawn,
        "edit",
        "human",
        json!({"edits":[{"from":0,"to":0,"insert":"prefix "}]}),
    );
    let unsaved = change(
        &service,
        &human,
        "propose",
        "unsaved",
        json!({"edits":[{"from":0,"to":6,"insert":"UNSAVED"}]}),
    );
    let discarded = change(
        &service,
        &unsaved,
        "discard",
        "discard",
        json!({"disk_revision":saved["disk_revision"],"disk_content":normalize(&raw)}),
    );
    assert_eq!(discarded["content"], saved["content"]);
    assert_eq!(discarded["changes"].as_array().unwrap().len(), 2);
    assert_eq!(discarded["changes"][0]["decision"], "accepted");
    assert_eq!(discarded["changes"][1]["decision"], "pending");
    assert_eq!(discarded["changes"][1]["from"], saved["changes"][1]["from"]);
    assert!(discarded["generation"].as_u64().unwrap() > unsaved["generation"].as_u64().unwrap());
    drop(service);
    let service = Service::new(directory.path());
    let reopened = open_document(&service, "draft:discard", &raw);
    assert_eq!(reopened["content"], saved["content"]);
    let withdrawn = change(
        &service,
        &reopened,
        "resolve",
        "withdraw-after-discard",
        json!({"ids":[b],"decision":"withdraw"}),
    );
    assert_eq!(withdrawn["content"], "A new-a B old C");
    let db = Connection::open(directory.path().join("review.sqlite3")).unwrap();
    let retained: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM review_events WHERE payload LIKE '%UNSAVED%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(retained > 0);
}

#[test]
fn review_discard_initial_unsaved_buffer_uses_actual_disk_checkpoint_and_refuses_external_change() {
    let directory = TempDir::new().unwrap();
    let service = Service::new(directory.path());
    let disk = "disk body";
    let initial = service
        .handle(
            &json!({"action":"open","path":"draft:initial","content":"unsaved buffer",
        "disk_content":disk,"disk_revision":document_transaction::revision(disk)}),
        )
        .unwrap();
    let before = events(&directory);
    assert_eq!(
        service
            .handle(&request(
                &initial,
                "discard",
                "bad-discard",
                json!({"disk_revision":"changed","disk_content":"external"})
            ))
            .unwrap_err(),
        "review_save_conflict"
    );
    assert_eq!(events(&directory), before);
    assert_eq!(read(&service, &initial)["content"], "unsaved buffer");
    let discarded = change(
        &service,
        &initial,
        "discard",
        "discard",
        json!({"disk_revision":initial["disk_revision"],"disk_content":disk}),
    );
    assert_eq!(discarded["content"], disk);
    assert!(discarded["changes"].as_array().unwrap().is_empty());
}

#[test]
fn review_prepare_reserves_completion_budget_before_file_side_effect() {
    let directory = TempDir::new().unwrap();
    let service = Service::new(directory.path());
    let initial = open_document(&service, "draft:reserve", "retained");
    let baseline = service
        .store
        .run(|tx| review_store::used_bytes(tx))
        .unwrap();
    let limited = Service::with_limit(directory.path(), baseline + 3000);
    let prepared = request(
        &initial,
        "prepare_save",
        "prepare",
        json!({"content":"retained","disk_revision":initial["disk_revision"],
        "expected_disk_revision":document_transaction::revision("retained")}),
    );
    assert_eq!(limited.handle(&prepared).unwrap_err(), "review_budget");
    assert!(read(&service, &initial)["save_intent"].is_null());
    assert_eq!(events(&directory), 0);
}

fn entropy_text(length: usize, mut state: u64) -> String {
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (b'!' + (state % 93) as u8) as char
        })
        .collect()
}

#[test]
fn review_thousand_edits_keep_large_entropy_source_and_review_baselines_bounded_and_receipts_exact()
{
    let directory = TempDir::new().unwrap();
    let service = Service::new(directory.path());
    let source = entropy_text(100 * 1024, 0x4d595df4d0f33173);
    let initial = open_document(&service, "draft:typing-budget", &source);
    let mut state = change(
        &service,
        &initial,
        "propose",
        "large-pending",
        json!({"edits":[{
        "from":20*1024,"to":100*1024,"insert":entropy_text(80*1024,0x9e3779b97f4a7c15)}]}),
    );
    let mut old_receipts = Vec::new();
    for index in 0..1000 {
        let typed = request(
            &state,
            "edit",
            &format!("typing-{index}"),
            json!({"edits":[{"from":0,"to":0,"insert":((b'a'+(index%26) as u8) as char).to_string()}]}),
        );
        state = service.handle(&typed).unwrap();
        if matches!(index, 0 | 499 | 999) {
            old_receipts.push((typed, state.clone()));
        }
    }
    let undone = change(&service, &state, "undo", "undo-latest", json!({}));
    let (saved, _) = save(&service, &undone, "typing-save");
    let end = saved["content"].as_str().unwrap().encode_utf16().count();
    let extra = change(
        &service,
        &saved,
        "edit",
        "after-save",
        json!({"edits":[{"from":end,"to":end,"insert":"!"}]}),
    );
    let discarded = change(
        &service,
        &extra,
        "discard",
        "discard-after-save",
        json!({"disk_revision":saved["disk_revision"],"disk_content":saved["content"]}),
    );
    assert_eq!(discarded["content"], saved["content"]);
    let logical = service.store.run(review_store::used_bytes).unwrap();
    let physical = fs::metadata(directory.path().join("review.sqlite3"))
        .unwrap()
        .len();
    eprintln!("review_typing_1000 logical_bytes={logical} database_bytes={physical}");
    assert!(
        logical < 16 * 1024 * 1024,
        "logical storage grew to {logical}"
    );
    assert!(physical < 16 * 1024 * 1024, "SQLite grew to {physical}");
    drop(service);
    let service = Service::new(directory.path());
    for (request, receipt) in old_receipts {
        assert_eq!(service.handle(&request).unwrap(), receipt);
        let lookup=service.handle(&json!({"action":"receipt","document_id":request["document_id"],"call_id":request["call_id"]})).unwrap();
        assert_eq!(lookup["snapshot"], receipt);
        assert_eq!(lookup["request"], request);
    }
    let db = Connection::open(directory.path().join("review.sqlite3")).unwrap();
    let text_count: i64 = db
        .query_row("SELECT COUNT(*) FROM review_texts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        text_count, 2,
        "anchor moves should not duplicate original/candidate versions"
    );
}

#[test]
fn review_schema_one_full_receipt_survives_migration_and_later_edits() {
    let directory = TempDir::new().unwrap();
    let service = Service::new(directory.path());
    let initial = open_document(&service, "draft:legacy", "old");
    let request = request(
        &initial,
        "edit",
        "legacy-call",
        json!({"edits":[{"from":0,"to":3,"insert":"new"}]}),
    );
    let response = service.handle(&request).unwrap();
    drop(service);
    let db = Connection::open(directory.path().join("review.sqlite3")).unwrap();
    db.execute(
        "UPDATE review_receipts SET payload=?1 WHERE call_id='legacy-call'",
        [json!({"request":request,"snapshot":response}).to_string()],
    )
    .unwrap();
    db.execute_batch("DROP TABLE review_texts;PRAGMA user_version=1")
        .unwrap();
    drop(db);
    let service = Service::new(directory.path());
    let later = change(
        &service,
        &response,
        "edit",
        "later",
        json!({"edits":[{"from":0,"to":0,"insert":"later "}]}),
    );
    assert_eq!(later["content"], "later new");
    assert_eq!(service.handle(&request).unwrap(), response);
    let lookup=service.handle(&json!({"action":"receipt","document_id":initial["document_id"],"call_id":"legacy-call"})).unwrap();
    assert_eq!(lookup["request"], request);
    assert_eq!(lookup["snapshot"], response);
    let db = Connection::open(directory.path().join("review.sqlite3")).unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        review_store::SCHEMA_VERSION
    );
}

#[test]
fn review_save_reserves_peak_rewrite_capacity_and_one_byte_short_rolls_back() {
    let directory = TempDir::new().unwrap();
    let service = Service::new(directory.path());
    let source = entropy_text(64 * 1024, 0x4d595df4d0f33173);
    let candidate = entropy_text(64 * 1024, 0x9e3779b97f4a7c15);
    let rewritten = entropy_text(64 * 1024, 0x2545f4914f6cdd1d);
    let setup = |service: &Service| {
        let initial = open_document(service, "draft:peak-capacity", &source);
        change(
            service,
            &initial,
            "propose",
            "proposal",
            json!({"edits":[{"from":0,"to":source.len(),"insert":candidate}]}),
        )
    };
    let prepare_request = |state: &Value| {
        request(
            state,
            "prepare_save",
            "prepare",
            json!({"content":candidate,
        "disk_revision":state["disk_revision"],"expected_disk_revision":document_transaction::revision(&rewritten),
        "saved_content":rewritten,"edit_steps":[{"edits":[{"from":0,"to":candidate.len(),"insert":rewritten}]}]}),
        )
    };
    let state = setup(&service);
    let prepared = service.handle(&prepare_request(&state)).unwrap();
    let reserved = prepared["save_intent"]["reserved_bytes"].as_u64().unwrap() as usize;
    let total = service.store.run(review_store::used_bytes).unwrap();
    let (peak_growth,final_growth)=service.store.run(|tx|with_text_context(tx,|| {
        let mut document=load(tx,state["document_id"].as_str().unwrap())?;
        let intent=document.save_intent.clone().unwrap();
        let before=review_store::used_bytes(tx)?;
        tx.execute_batch("SAVEPOINT peak_measure").map_err(|_|"review_persistence_failed")?;
        finish_intent(tx,&mut document,&intent,&intent.expected_disk_revision)?;
        let peak=review_store::used_bytes(tx)?.saturating_sub(before);
        persist(tx,&document)?;
        let call="x".repeat(256);
        let commit=json!({"action":"commit_save","document_id":document.document_id,"intent_id":intent.intent_id,
            "call_id":call,"disk_revision":intent.expected_disk_revision,"content":intent.saved_content,"encoding":intent.encoding,"line_ending":intent.line_ending});
        let mut receipt=snapshot(&document);receipt["call_id"]=json!(call);receipt["durable"]=json!(true);
        review_store::put_receipt(tx,&document.document_id,&call,&"0".repeat(64),&compact_receipt(&commit,&receipt))?;
        let final_growth=review_store::used_bytes(tx)?.saturating_sub(before-reserved);
        tx.execute_batch("ROLLBACK TO peak_measure;RELEASE peak_measure").map_err(|_|"review_persistence_failed")?;
        Ok((peak,final_growth))
    })).unwrap();
    assert!(
        peak_growth > final_growth,
        "test must exercise peak rather than net storage growth"
    );
    assert_eq!(reserved, peak_growth + 8192);
    let low_directory = TempDir::new().unwrap();
    let low = Service::with_limit(low_directory.path(), total - 1);
    let low_state = setup(&low);
    assert_eq!(
        low.handle(&prepare_request(&low_state)).unwrap_err(),
        "review_budget"
    );
    assert!(read(&Service::new(low_directory.path()), &low_state)["save_intent"].is_null());
    assert_eq!(events(&low_directory), 1);
    let exact_directory = TempDir::new().unwrap();
    let exact = Service::with_limit(exact_directory.path(), total);
    let exact_state = setup(&exact);
    let exact_prepared = exact.handle(&prepare_request(&exact_state)).unwrap();
    assert_eq!(exact.store.run(review_store::used_bytes).unwrap(), total);
    let committed=exact.handle(&json!({"action":"commit_save","document_id":exact_state["document_id"],"intent_id":exact_prepared["intent_id"],
        "call_id":"commit","disk_revision":document_transaction::revision(&rewritten),"content":rewritten})).unwrap();
    assert_eq!(committed["content"], rewritten);
    assert!(committed["save_intent"].is_null());
}

#[test]
fn review_old_proposal_receipt_loads_archived_text_after_undo_and_new_edit() {
    let directory = TempDir::new().unwrap();
    let service = Service::new(directory.path());
    let initial = open_document(&service, "draft:archived-receipt", "old body");
    let proposal = request(
        &initial,
        "propose",
        "proposal",
        json!({"edits":[{"from":0,"to":3,"insert":"candidate"}]}),
    );
    let committed = service.handle(&proposal).unwrap();
    let undone = change(&service, &committed, "undo", "undo-proposal", json!({}));
    let edited = change(
        &service,
        &undone,
        "edit",
        "new-branch",
        json!({"edits":[{"from":0,"to":0,"insert":"different "}]}),
    );
    assert!(edited["changes"].as_array().unwrap().is_empty());
    assert_eq!(edited["can_redo"], false);
    let serialized = service
        .store
        .run(|tx| review_store::by_id(tx, initial["document_id"].as_str().unwrap()))
        .unwrap()
        .unwrap();
    assert!(
        !serialized.contains("$review_text"),
        "active state must not keep the old proposal's text references"
    );
    drop(service);
    let service = Service::new(directory.path());
    assert_eq!(service.handle(&proposal).unwrap(), committed);
    let lookup = service
        .handle(
            &json!({"action":"receipt","document_id":initial["document_id"],"call_id":"proposal"}),
        )
        .unwrap();
    assert_eq!(lookup["snapshot"], committed);
    assert_eq!(lookup["request"], proposal);
}

#[test]
fn review_relative_shared_asset_original_is_pinned_after_remove_save_prune_and_save_as() {
    let directory = TempDir::new().unwrap();
    let old = directory.path().join("old");
    let new = directory.path().join("new");
    fs::create_dir_all(old.join("readmd-assets")).unwrap();
    fs::create_dir_all(new.join("readmd-assets")).unwrap();
    let original_asset = old.join("readmd-assets").join("shared pic.svg");
    fs::write(&original_asset, b"<svg>shared original</svg>").unwrap();
    let first = "![first](readmd-assets/shared%20pic.svg)";
    let source = format!("{first}\n![second](readmd-assets/shared%20pic.svg)\n");
    let source_path = old.join("Review.md");
    fs::write(&source_path, &source).unwrap();
    let service = Service::new(directory.path());
    let initial = open_document(&service, &source_path.to_string_lossy(), &source);
    let removed = change(
        &service,
        &initial,
        "propose",
        "remove-first",
        json!({"edits":[{"from":0,"to":first.len(),"insert":""}]}),
    );
    let (saved, raw) = save(&service, &removed, "source-save");
    fs::write(&source_path, &raw).unwrap();
    for index in 0..20 {
        crate::document_history::record(
            directory.path(),
            "shared-asset",
            &source_path.to_string_lossy(),
            "Review.md",
            "version",
            "save",
            &format!("ordinary-{index}"),
        )
        .unwrap();
    }
    assert_eq!(
        crate::document_history::list(directory.path(), Some("shared-asset"))
            .unwrap()
            .len(),
        10
    );
    assert!(service
        .pinned_assets()
        .unwrap()
        .contains(&original_asset.to_string_lossy().into_owned()));
    let before = saved["content"].as_str().unwrap();
    let needle = "readmd-assets/shared%20pic.svg";
    let (from, to) = utf16_range(before, needle, 0);
    let planned = before.replace(needle, "readmd-assets/copied.svg");
    let target = new.join("Review.md");
    let copied = new.join("readmd-assets").join("copied.svg");
    fs::copy(&original_asset, &copied).unwrap();
    let prepared = change(
        &service,
        &saved,
        "prepare_save",
        "save-as-prepare",
        json!({"content":before,"disk_revision":saved["disk_revision"],
        "target_path":target.to_string_lossy(),"target_disk_revision":"missing","expected_disk_revision":document_transaction::revision(&planned),
        "saved_content":planned,"edit_steps":[{"edits":[{"from":from,"to":to,"insert":"readmd-assets/copied.svg"}]}]}),
    );
    fs::write(&target, &planned).unwrap();
    let committed=service.handle(&json!({"action":"commit_save","document_id":saved["document_id"],"intent_id":prepared["intent_id"],
        "call_id":"save-as-commit","disk_revision":document_transaction::revision(&planned),"content":planned})).unwrap();
    assert_eq!(committed["changes"][0]["decision"], "pending");
    drop(service);
    let service = Service::new(directory.path());
    let reopened = open_document(&service, &target.to_string_lossy(), &planned);
    assert_eq!(reopened["changes"][0]["original"], first);
    let pins = service.pinned_assets().unwrap();
    assert!(
        pins.contains(&original_asset.to_string_lossy().into_owned()),
        "old source base must survive SaveAs: {pins:?}"
    );
    assert!(
        pins.contains(&copied.to_string_lossy().into_owned()),
        "new current base must be pinned: {pins:?}"
    );
    assert_eq!(
        fs::read(&original_asset).unwrap(),
        b"<svg>shared original</svg>"
    );
}

#[test]
fn review_asset_pins_exclude_urls_and_malformed_percent_or_utf8_and_keep_bare_destinations() {
    let directory = TempDir::new().unwrap();
    let base = directory.path().join("docs");
    fs::create_dir_all(&base).unwrap();
    let service = Service::new(directory.path());
    let document_path = base.join("Review.md");
    let body="![url](https://example.test/image.svg)\n![data](data:image/png;base64,AAAA)\n![network](//example.test/x.svg)\n![invalid](readmd-assets/bad%ZZ.svg)\n![utf8](readmd-assets/bad%FF.svg)\n![valid](readmd-assets/%E5%9B%BE%20pic.svg)\n";
    let state = open_document(&service, &document_path.to_string_lossy(), body);
    let pins = service.pinned_assets().unwrap();
    assert_eq!(
        pins,
        vec![base
            .join("readmd-assets")
            .join("图 pic.svg")
            .to_string_lossy()
            .into_owned()]
    );
    let original = "![bare](readmd-assets/original.svg)";
    let bare = open_document(&service, &base.join("Bare.md").to_string_lossy(), original);
    let (from, to) = utf16_range(original, "readmd-assets/original.svg", 0);
    let proposal = change(
        &service,
        &bare,
        "propose",
        "replace-only-destination",
        json!({"edits":[{"from":from,"to":to,"insert":"readmd-assets/new.svg"}]}),
    );
    let (saved, _) = save(&service, &proposal, "bare-save");
    assert!(service.pinned_assets().unwrap().contains(
        &base
            .join("readmd-assets")
            .join("original.svg")
            .to_string_lossy()
            .into_owned()
    ));
    assert_eq!(
        read(&service, &saved)["changes"][0]["original"],
        "readmd-assets/original.svg"
    );
    assert_eq!(state["path"], document_path.to_string_lossy().as_ref());
    assert!(local_asset_path("/raw?p=data%3Aimage%2Fpng%3Bbase64%2CAAAA").is_none());
    assert!(local_asset_path("/raw?p=bad%FF.svg").is_none());
}

#[test]
fn review_explicit_keep_current_resolves_conflict_without_clobbering_and_survives_restart_undo() {
    let directory = TempDir::new().unwrap();
    let service = Service::new(directory.path());
    let initial = open_document(&service, "draft:keep-current", "original");
    let proposed = change(
        &service,
        &initial,
        "propose",
        "proposal",
        json!({"edits":[{"from":0,"to":8,"insert":"candidate"}]}),
    );
    let id = proposed["changes"][0]["id"].clone();
    assert_eq!(
        service
            .handle(&request(
                &proposed,
                "resolve",
                "invalid-normal",
                json!({"ids":[id],"decision":"accept","resolution":"keep_current"})
            ))
            .unwrap_err(),
        "review_invalid_request"
    );
    let human = change(
        &service,
        &proposed,
        "edit",
        "human",
        json!({"edits":[{"from":0,"to":9,"insert":"human version"}]}),
    );
    assert_eq!(
        service
            .handle(&request(
                &human,
                "resolve",
                "ordinary-accept",
                json!({"ids":[id],"decision":"accept"})
            ))
            .unwrap_err(),
        "review_conflict"
    );
    assert_eq!(
        service
            .handle(&request(
                &human,
                "resolve",
                "unknown-resolution",
                json!({"ids":[id],"decision":"accept","resolution":"restore_original"})
            ))
            .unwrap_err(),
        "review_invalid_request"
    );
    let accept = request(
        &human,
        "resolve",
        "keep",
        json!({"ids":[id],"decision":"accept","resolution":"keep_current"}),
    );
    let kept = service.handle(&accept).unwrap();
    assert_eq!(kept["content"], "human version");
    assert_eq!(kept["generation"], human["generation"]);
    assert_eq!(kept["changes"][0]["decision"], "accepted");
    assert_eq!(kept["changes"][0]["taken_over"], true);
    assert_eq!(kept["changes"][0]["original"], "original");
    assert_eq!(kept["changes"][0]["replacement"], "candidate");
    drop(service);
    let service = Service::new(directory.path());
    let reopened = open_document(&service, "draft:keep-current", "original");
    assert_eq!(reopened["changes"], kept["changes"]);
    assert_eq!(
        service
            .handle(&request(
                &human,
                "resolve",
                "stale-keep",
                json!({"ids":[id],"decision":"accept","resolution":"keep_current"})
            ))
            .unwrap_err(),
        "review_stale_revision"
    );
    assert_eq!(
        service
            .handle(&request(
                &reopened,
                "resolve",
                "unsafe-withdraw",
                json!({"ids":[id],"decision":"withdraw"})
            ))
            .unwrap_err(),
        "review_conflict"
    );
    let undone = change(&service, &reopened, "undo", "undo-keep", json!({}));
    assert_eq!(undone["content"], "human version");
    assert_eq!(undone["changes"][0]["decision"], "conflict");
    let redone = change(&service, &undone, "redo", "redo-keep", json!({}));
    assert_eq!(redone["content"], "human version");
    assert_eq!(redone["changes"][0]["decision"], "accepted");
    assert_eq!(service.handle(&accept).unwrap(), kept);
}

#[test]
fn review_lifetime_event_counter_crosses_request_limit_without_blocking_save_withdraw_undo_or_discard(
) {
    let directory = TempDir::new().unwrap();
    let service = Service::new(directory.path());
    let initial = open_document(&service, "draft:lifetime-boundary", "alpha original");
    let proposal = request(
        &initial,
        "propose",
        "proposal",
        json!({"edits":[{"from":6,"to":14,"insert":"candidate"}]}),
    );
    let old_receipt = service.handle(&proposal).unwrap();
    service
        .store
        .run(|tx| {
            with_text_context(tx, || {
                let mut document = load(tx, initial["document_id"].as_str().unwrap())?;
                // Seed only the boundary counters. Existing event/receipt/baseline rows
                // are retained and every operation below runs the production service.
                document.last_event_id = (MAX_OPERATIONS - 1) as u64;
                document.generation = (MAX_OPERATIONS - 1) as u64;
                document.review_revision = (MAX_OPERATIONS - 1) as u64;
                persist(tx, &document)
            })
        })
        .unwrap();
    let near = read(&service, &initial);
    let crossed = change(
        &service,
        &near,
        "edit",
        "cross",
        json!({"edits":[{"from":0,"to":0,"insert":"human "}]}),
    );
    assert_eq!(crossed["last_event_id"], MAX_OPERATIONS as u64);
    assert_eq!(crossed["generation"], MAX_OPERATIONS as u64);
    let (saved, _) = save(&service, &crossed, "boundary-save");
    assert!(saved["last_event_id"].as_u64().unwrap() > MAX_OPERATIONS as u64);
    let withdrawn = change(
        &service,
        &saved,
        "resolve",
        "withdraw",
        json!({"ids":[saved["changes"][0]["id"]],"decision":"withdraw"}),
    );
    assert_eq!(withdrawn["content"], "human alpha original");
    let undone = change(&service, &withdrawn, "undo", "undo-withdraw", json!({}));
    assert_eq!(undone["content"], saved["content"]);
    let discarded = change(
        &service,
        &undone,
        "discard",
        "discard",
        json!({"disk_revision":saved["disk_revision"],"disk_content":saved["content"]}),
    );
    assert_eq!(discarded["content"], saved["content"]);
    assert_eq!(discarded["changes"][0]["original"], "original");
    assert_eq!(discarded["changes"][0]["decision"], "pending");
    assert!(discarded["generation"].as_u64().unwrap() > crossed["generation"].as_u64().unwrap());
    drop(service);
    let service = Service::new(directory.path());
    assert_eq!(service.handle(&proposal).unwrap(), old_receipt);
    assert_eq!(read(&service, &initial)["content"], saved["content"]);
    let db = Connection::open(directory.path().join("review.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM review_texts", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert!(events(&directory) >= 9);
}

#[test]
fn review_monotonic_counters_refuse_sqlite_integer_overflow_atomically() {
    for field in ["last_event_id", "generation", "review_revision"] {
        let directory = TempDir::new().unwrap();
        let service = Service::new(directory.path());
        let initial = open_document(&service, "draft:counter-overflow", "body");
        service
            .store
            .run(|tx| {
                with_text_context(tx, || {
                    let mut document = load(tx, initial["document_id"].as_str().unwrap())?;
                    match field {
                        "last_event_id" => document.last_event_id = i64::MAX as u64,
                        "generation" => document.generation = i64::MAX as u64,
                        _ => document.review_revision = i64::MAX as u64,
                    }
                    persist(tx, &document)
                })
            })
            .unwrap();
        let before = read(&service, &initial);
        assert_eq!(
            service
                .handle(&request(
                    &before,
                    "edit",
                    "overflow",
                    json!({"edits":[{"from":0,"to":0,"insert":"new "}]})
                ))
                .unwrap_err(),
            "review_budget"
        );
        assert_eq!(read(&service, &initial), before);
        assert_eq!(events(&directory), 0);
    }
}

// Independent literal expectations: review-anchor-undo-adversarial-spec-01.
// These tests call the actual Service/SQLite history, not the geometry helper.
#[cfg(test)]
mod exact_anchor_compensation_regressions {
    use super::*;
    const ORIGINAL: &str = "😀|aaOOaa|bbOObb|tailOO";
    const SEEDED: &str = "😀|aaCCaa|bbCCbb|tailCC";
    fn seed(service: &Service, path: &str, proposed: bool) -> Value {
        let first = open_document(service, path, ORIGINAL);
        change(service, &first, "propose", "seed", json!({"origin":"ai",
            "application":if proposed {"proposed"} else {"applied"},
            "edits":[{"from":5,"to":7,"insert":"CC"},{"from":12,"to":14,"insert":"CC"},{"from":21,"to":23,"insert":"CC"}]}))
    }
    fn ids(head: &Value) -> Vec<Value> {
        [5,12,21].iter().map(|from| head["changes"].as_array().unwrap().iter()
            .find(|change| change["from"] == *from).unwrap()["id"].clone()).collect()
    }
    fn item<'a>(head: &'a Value, id: &Value) -> &'a Value {
        head["changes"].as_array().unwrap().iter().find(|change| change["id"] == *id).unwrap()
    }
    fn semantic(head: &Value, ids: &[Value]) -> Value {
        json!(ids.iter().map(|id| {
            let current = item(head,id);
            ["id","from","to","original","replacement","decision","application","taken_over",
                "origin","task_id","candidate_versions","dependencies","restore_original"]
                .iter().map(|key| ((*key).to_string(),current[*key].clone()))
                .collect::<serde_json::Map<String,Value>>()
        }).collect::<Vec<_>>())
    }
    fn ranges(head: &Value, ids: &[Value], expected: [[u64;2];3]) {
        assert_eq!(head["changes"].as_array().unwrap().len(),3);
        for (id, pair) in ids.iter().zip(expected) {
            assert_eq!(item(head,id)["from"],pair[0]);assert_eq!(item(head,id)["to"],pair[1]);
        }
    }
    fn rewrite_save(service: &Service, head: &Value, saved: &str, edits: Value) -> Value {
        let prepare_call=uuid::Uuid::new_v4().to_string();
        let commit_call=uuid::Uuid::new_v4().to_string();
        let prepared=change(service,head,"prepare_save",&prepare_call,json!({
            "content":head["content"],"disk_revision":head["disk_revision"],
            "expected_disk_revision":document_transaction::revision(saved),"saved_content":saved,
            "edit_steps":[{"edits":edits}]}));
        service.handle(&json!({"action":"commit_save","document_id":head["document_id"],
            "intent_id":prepared["intent_id"],"call_id":commit_call,
            "disk_revision":document_transaction::revision(saved),"content":saved})).unwrap()
    }
    #[test]
    fn independent_eighteen_ranges_restart_undo_redo_and_withdraw() {
        let cases: Vec<(&str,Value,&str,[[u64;2];3],[bool;3])> = vec![
            ("whole-grow-emoji",json!([{"from":10,"to":16,"insert":"bb🙂CCbb"}]),"😀|aaCCaa|bb🙂CCbb|tailCC",[[5, 7], [10, 18], [23, 25]],[false, true, false]),
            ("whole-shrink",json!([{"from":10,"to":16,"insert":"CC"}]),"😀|aaCCaa|CC|tailCC",[[5, 7], [10, 12], [17, 19]],[false, true, false]),
            ("whole-equal-length",json!([{"from":10,"to":16,"insert":"zzCCzz"}]),"😀|aaCCaa|zzCCzz|tailCC",[[5, 7], [10, 16], [21, 23]],[false, true, false]),
            ("whole-delete",json!([{"from":10,"to":16,"insert":""}]),"😀|aaCCaa||tailCC",[[5, 7], [10, 10], [15, 17]],[false, true, false]),
            ("partial-left-grow",json!([{"from":11,"to":13,"insert":"LLLL"}]),"😀|aaCCaa|bLLLLCbb|tailCC",[[5, 7], [11, 16], [23, 25]],[false, true, false]),
            ("partial-left-shrink",json!([{"from":11,"to":13,"insert":"L"}]),"😀|aaCCaa|bLCbb|tailCC",[[5, 7], [11, 13], [20, 22]],[false, true, false]),
            ("partial-left-equal",json!([{"from":11,"to":13,"insert":"LL"}]),"😀|aaCCaa|bLLCbb|tailCC",[[5, 7], [11, 14], [21, 23]],[false, true, false]),
            ("partial-right-grow",json!([{"from":13,"to":15,"insert":"RRRR"}]),"😀|aaCCaa|bbCRRRRb|tailCC",[[5, 7], [12, 17], [23, 25]],[false, true, false]),
            ("partial-right-shrink",json!([{"from":13,"to":15,"insert":"R"}]),"😀|aaCCaa|bbCRb|tailCC",[[5, 7], [12, 14], [20, 22]],[false, true, false]),
            ("partial-right-equal",json!([{"from":13,"to":15,"insert":"RR"}]),"😀|aaCCaa|bbCRRb|tailCC",[[5, 7], [12, 15], [21, 23]],[false, true, false]),
            ("inner-equal-control",json!([{"from":12,"to":13,"insert":"Z"}]),"😀|aaCCaa|bbZCbb|tailCC",[[5, 7], [12, 14], [21, 23]],[false, true, false]),
            ("one-enclosing-both-ids",json!([{"from":3,"to":16,"insert":"xxaaCCaa|bbCCbbyy"}]),"😀|xxaaCCaa|bbCCbbyy|tailCC",[[3, 20], [3, 20], [25, 27]],[true, true, false]),
            ("multi-range-shrink-grow",json!([{"from":3,"to":9,"insert":"aCCa"},{"from":10,"to":16,"insert":"bbbbCCbbbb"}]),"😀|aCCa|bbbbCCbbbb|tailCC",[[3, 7], [8, 18], [23, 25]],[true, true, false]),
            ("multi-range-net-zero",json!([{"from":3,"to":9,"insert":"aaaaCCaa"},{"from":10,"to":16,"insert":"CCbb"}]),"😀|aaaaCCaa|CCbb|tailCC",[[3, 11], [12, 16], [21, 23]],[true, true, false]),
            ("insert-at-left-boundary",json!([{"from":12,"to":12,"insert":"X"}]),"😀|aaCCaa|bbXCCbb|tailCC",[[5, 7], [13, 15], [22, 24]],[false, false, false]),
            ("insert-at-right-boundary",json!([{"from":14,"to":14,"insert":"Y"}]),"😀|aaCCaa|bbCCYbb|tailCC",[[5, 7], [12, 14], [22, 24]],[false, false, false]),
            ("left-adjacent-replacement",json!([{"from":10,"to":12,"insert":"B"}]),"😀|aaCCaa|BCCbb|tailCC",[[5, 7], [11, 13], [20, 22]],[false, false, false]),
            ("right-adjacent-replacement",json!([{"from":14,"to":16,"insert":"BBB"}]),"😀|aaCCaa|bbCCBBB|tailCC",[[5, 7], [12, 14], [22, 24]],[false, false, false]),
        ];
        for (label,edits,after,forward,taken) in cases {
            let directory=TempDir::new().unwrap();let service=Service::new(directory.path());
            let seeded=seed(&service,&format!("draft:{label}"),false);let ids=ids(&seeded);
            assert_eq!(seeded["content"],SEEDED);ranges(&seeded,&ids,[[5,7],[12,14],[21,23]]);
            let manual=change(&service,&seeded,"edit","manual",json!({"origin":"editor","edits":edits}));
            assert_eq!(manual["content"],after,"{label}");ranges(&manual,&ids,forward);
            for (id,taken) in ids.iter().zip(taken) {
                assert_eq!(item(&manual,id)["taken_over"],taken,"{label}");
                assert_eq!(item(&manual,id)["decision"],if taken {"conflict"} else {"pending"},"{label}");
            }
            drop(service);let service=Service::new(directory.path());let current=read(&service,&manual);
            let undone=change(&service,&current,"undo","undo",json!({}));
            assert_eq!(undone["content"],SEEDED,"{label}");assert_eq!(semantic(&undone,&ids),semantic(&seeded,&ids),"{label}");
            assert!(undone["generation"].as_u64().unwrap()>manual["generation"].as_u64().unwrap());
            let redone=change(&service,&undone,"redo","redo",json!({}));
            assert_eq!(redone["content"],after,"{label}");assert_eq!(semantic(&redone,&ids),semantic(&manual,&ids),"{label}");
            let back=change(&service,&redone,"undo","undo-again",json!({}));
            let withdrawn=change(&service,&back,"resolve","withdraw",json!({"ids":ids,"decision":"withdraw"}));
            assert_eq!(withdrawn["content"],ORIGINAL,"{label}");
        }
    }
    #[test]
    fn independent_five_zero_spans_keep_exact_point_through_history() {
        let cases: Vec<(&str,Value,&str,[u64;2],bool)> = vec![
            ("zero-insert-same-point",json!([{"from":4,"to":4,"insert":"🙂"}]),"😀|a🙂b|tail",[4, 6],true),
            ("zero-enclosing-replacement",json!([{"from":3,"to":5,"insert":"A🙂B"}]),"😀|A🙂B|tail",[3, 7],true),
            ("zero-left-adjacent",json!([{"from":3,"to":4,"insert":"AA"}]),"😀|AAb|tail",[5, 5],false),
            ("zero-right-adjacent",json!([{"from":4,"to":5,"insert":"BBBB"}]),"😀|aBBBB|tail",[4, 4],false),
            ("zero-earlier-surrogate-safe",json!([{"from":0,"to":2,"insert":"X"}]),"X|ab|tail",[3, 3],false),
        ];
        for(label,edits,after,forward,taken)in cases {
            let directory=TempDir::new().unwrap();let service=Service::new(directory.path());
            let first=open_document(&service,&format!("draft:{label}"),"😀|aXXb|tail");
            let seeded=change(&service,&first,"propose","seed",json!({"edits":[{"from":4,"to":6,"insert":""}]}));
            let id=seeded["changes"][0]["id"].clone();let ids=vec![id.clone()];
            let manual=change(&service,&seeded,"edit","manual",json!({"edits":edits}));
            assert_eq!(manual["content"],after);assert_eq!(item(&manual,&id)["from"],forward[0]);assert_eq!(item(&manual,&id)["to"],forward[1]);assert_eq!(item(&manual,&id)["taken_over"],taken);
            drop(service);let service=Service::new(directory.path());
            let undone=change(&service,&read(&service,&manual),"undo","undo",json!({}));
            assert_eq!(undone["content"],"😀|ab|tail");assert_eq!(semantic(&undone,&ids),semantic(&seeded,&ids),"{label}");
            let redone=change(&service,&undone,"redo","redo",json!({}));assert_eq!(redone["content"],after);assert_eq!(semantic(&redone,&ids),semantic(&manual,&ids));
        }
    }
    #[test]
    fn applied_deletion_proposal_redo_recreates_absent_zero_anchor() {
        let directory=TempDir::new().unwrap();let service=Service::new(directory.path());
        let first=open_document(&service,"draft:zero-proposal","😀|aXXb|tail");
        let seeded=change(&service,&first,"propose","seed",json!({"edits":[{"from":4,"to":6,"insert":""}]}));
        let id=seeded["changes"][0]["id"].clone();
        let undone=change(&service,&seeded,"undo","undo",json!({}));assert!(undone["changes"].as_array().unwrap().is_empty());
        let redone=change(&service,&undone,"redo","redo",json!({}));assert_eq!(redone["content"],"😀|ab|tail");assert_eq!(semantic(&redone,&[id.clone()]),semantic(&seeded,&[id]));
    }
    #[test]
    fn applied_multi_proposal_redo_recreates_absent_positive_spans() {
        let directory=TempDir::new().unwrap();let service=Service::new(directory.path());let seeded=seed(&service,"draft:applied-redo",false);let ids=ids(&seeded);
        let undone=change(&service,&seeded,"undo","undo",json!({}));assert_eq!(undone["content"],ORIGINAL);assert!(undone["changes"].as_array().unwrap().is_empty());
        drop(service);let service=Service::new(directory.path());let redone=change(&service,&read(&service,&undone),"redo","redo",json!({}));assert_eq!(redone["content"],SEEDED);assert_eq!(semantic(&redone,&ids),semantic(&seeded,&ids));
    }
    #[test]
    fn multiple_raw_inverse_fragments_and_shared_source_boundaries_restore_exactly() {
        for edits in [
            json!([{"from":0,"to":0,"insert":"ZZ"},{"from":10,"to":16,"insert":"(bbCCbb)"}]),
            json!([{"from":3,"to":5,"insert":"Q"},{"from":5,"to":7,"insert":"R"}]),
            json!([{"from":3,"to":5,"insert":""},{"from":5,"to":7,"insert":""}]),
            json!([{"from":3,"to":5,"insert":""},{"from":5,"to":7,"insert":"R"}]),
        ] {
            let directory=TempDir::new().unwrap();let service=Service::new(directory.path());let seeded=seed(&service,"draft:raw",false);let ids=ids(&seeded);
            let manual=change(&service,&seeded,"edit","manual",json!({"edits":edits}));
            service.store.run(|tx|with_text_context(tx,||{let doc=load(tx,manual["document_id"].as_str().unwrap())?;let action=doc.undo.last().unwrap();assert_eq!(action.edits.len(),2);assert_eq!(action.inverse.len(),2);assert_eq!(action.inverse_expected.len(),2);Ok(())})).unwrap();
            drop(service);let service=Service::new(directory.path());let undone=change(&service,&read(&service,&manual),"undo","undo",json!({}));
            assert_eq!(undone["content"],SEEDED);assert_eq!(semantic(&undone,&ids),semantic(&seeded,&ids));
            let redone=change(&service,&undone,"redo","redo",json!({}));assert_eq!(redone["content"],manual["content"]);assert_eq!(semantic(&redone,&ids),semantic(&manual,&ids));
        }
    }
    #[test]
    fn metadata_accept_and_enclosing_later_pair_do_not_replay_lossy_geometry() {
        let directory=TempDir::new().unwrap();let service=Service::new(directory.path());let seeded=seed(&service,"draft:later",false);let ids=ids(&seeded);
        let accepted=change(&service,&seeded,"resolve","accept",json!({"ids":[ids[1]],"decision":"accept"}));
        let later=change(&service,&accepted,"edit","later",json!({"edits":[{"from":3,"to":16,"insert":"(aaCCaa|bbCCbb)"}]}));
        let undo_later=change(&service,&later,"undo","undo-later",json!({}));assert_eq!(undo_later["content"],SEEDED);assert_eq!(semantic(&undo_later,&ids),semantic(&accepted,&ids));
        let undo_accept=change(&service,&undo_later,"undo","undo-accept",json!({}));assert_eq!(undo_accept["content"],SEEDED);assert_eq!(semantic(&undo_accept,&ids),semantic(&seeded,&ids));assert_eq!(undo_accept["generation"],undo_later["generation"]);
    }
    #[test]
    fn metadata_accept_preserves_actual_nonundoable_save_prefix_geometry() {
        let directory=TempDir::new().unwrap();let service=Service::new(directory.path());let seeded=seed(&service,"draft:save-prefix",false);let ids=ids(&seeded);
        let accepted=change(&service,&seeded,"resolve","accept",json!({"ids":[ids[1]],"decision":"accept"}));
        let saved=rewrite_save(&service,&accepted,"PFX|😀|aaCCaa|bbCCbb|tailCC",json!([{"from":0,"to":0,"insert":"PFX|"}]));
        let undone=change(&service,&saved,"undo","undo-accept",json!({}));assert_eq!(undone["content"],saved["content"]);ranges(&undone,&ids,[[9,11],[16,18],[25,27]]);assert_eq!(item(&undone,&ids[1])["decision"],"pending");assert_eq!(undone["generation"],saved["generation"]);
    }
    #[test]
    fn proposed_metadata_redo_after_real_save_preserves_ids_without_applying_text() {
        let directory=TempDir::new().unwrap();let service=Service::new(directory.path());let seeded=seed(&service,"draft:proposed-save",true);let ids=ids(&seeded);assert_eq!(seeded["content"],ORIGINAL);
        let undone=change(&service,&seeded,"undo","undo",json!({}));assert!(undone["changes"].as_array().unwrap().is_empty());
        let saved=rewrite_save(&service,&undone,"PFX|😀|aaOOaa|bbOObb|tailOO",json!([{"from":0,"to":0,"insert":"PFX|"}]));
        drop(service);let service=Service::new(directory.path());let redone=change(&service,&read(&service,&saved),"redo","redo",json!({}));assert_eq!(redone["content"],saved["content"]);ranges(&redone,&ids,[[9,11],[16,18],[25,27]]);for id in ids{assert_eq!(item(&redone,&id)["application"],"proposed");assert_eq!(item(&redone,&id)["decision"],"pending");assert_eq!(item(&redone,&id)["taken_over"],false);}assert_eq!(redone["generation"],saved["generation"]);
    }
    #[test]
    fn ordinary_new_body_edit_clears_metadata_only_redo_as_actual_service_contract() {
        let directory=TempDir::new().unwrap();let service=Service::new(directory.path());let seeded=seed(&service,"draft:redo-cleared",true);
        let undone=change(&service,&seeded,"undo","undo",json!({}));let edited=change(&service,&undone,"edit","new-input",json!({"edits":[{"from":0,"to":0,"insert":"X"}]}));assert_eq!(edited["can_redo"],false);let count=events(&directory);let before=read(&service,&edited);
        assert_eq!(service.handle(&request(&edited,"redo","forbidden-redo",json!({}))).unwrap_err(),"review_redo_unavailable");assert_eq!(read(&service,&edited),before);assert_eq!(events(&directory),count);
    }
    #[test]
    fn ambiguous_absent_metadata_redo_fails_without_mutating_history_or_receipts() {
        let directory=TempDir::new().unwrap();let service=Service::new(directory.path());let seeded=seed(&service,"draft:ambiguous-proposed",true);
        let undone=change(&service,&seeded,"undo","undo",json!({}));let saved=rewrite_save(&service,&undone,"😀|aaOOaa|changed|tailOO",json!([{"from":10,"to":16,"insert":"changed"}]));
        let before=read(&service,&saved);let count=events(&directory);let request=request(&before,"redo","ambiguous-redo",json!({}));assert_eq!(service.handle(&request).unwrap_err(),"review_conflict");assert_eq!(read(&service,&saved),before);assert_eq!(events(&directory),count);let receipt=service.handle(&json!({"action":"receipt","document_id":saved["document_id"],"call_id":"ambiguous-redo"})).unwrap();assert_eq!(receipt["found"],false);
    }
    #[test]
    fn semantic_decisions_and_applications_survive_enclosing_manual_undo() {
        for mode in ["pending","accepted","conflict","withdrawn","proposed","accepted-proposed"] {
            let directory=TempDir::new().unwrap();let service=Service::new(directory.path());let proposed=mode=="proposed"||mode=="accepted-proposed";let mut baseline=seed(&service,&format!("draft:{mode}"),proposed);let ids=ids(&baseline);
            if mode=="accepted"||mode=="accepted-proposed"{baseline=change(&service,&baseline,"resolve","accept",json!({"ids":[ids[1]],"decision":"accept"}));}
            if mode=="withdrawn"{baseline=change(&service,&baseline,"resolve","withdraw",json!({"ids":[ids[1]],"decision":"withdraw"}));}
            if mode=="conflict"{baseline=change(&service,&baseline,"edit","inner",json!({"edits":[{"from":12,"to":13,"insert":"Z"}]}));}
            ranges(&baseline,&ids,[[5,7],[12,14],[21,23]]);
            let manual=change(&service,&baseline,"edit","outer",json!({"edits":[{"from":10,"to":16,"insert":"WRAPPED"}]}));let undone=change(&service,&manual,"undo","undo",json!({}));assert_eq!(undone["content"],baseline["content"],"{mode}");assert_eq!(semantic(&undone,&ids),semantic(&baseline,&ids),"{mode}");
        }
    }
    #[test]
    fn enclosing_undo_after_later_enclosing_undo_preserves_body_inverse_source() {
        let directory=TempDir::new().unwrap();let service=Service::new(directory.path());
        let seeded=seed(&service,"draft:two-enclosing",false);let ids=ids(&seeded);
        let target=change(&service,&seeded,"edit","target",json!({"edits":[{"from":10,"to":16,"insert":"bb🙂CCbb"}]}));
        assert_eq!(target["content"],"😀|aaCCaa|bb🙂CCbb|tailCC");
        let later=change(&service,&target,"edit","later",json!({"edits":[{"from":3,"to":18,"insert":"(aaCCaa|bb🙂CCbb)"}]}));
        let undo_later=change(&service,&later,"undo","undo-later",json!({}));
        assert_eq!(undo_later["content"],target["content"]);assert_eq!(semantic(&undo_later,&ids),semantic(&target,&ids));
        drop(service);let service=Service::new(directory.path());
        let undo_target=change(&service,&read(&service,&undo_later),"undo","undo-target",json!({}));
        assert_eq!(undo_target["content"],SEEDED);assert_eq!(semantic(&undo_target,&ids),semantic(&seeded,&ids));
        let redo_target=change(&service,&undo_target,"redo","redo-target",json!({}));
        assert_eq!(redo_target["content"],target["content"]);assert_eq!(semantic(&redo_target,&ids),semantic(&target,&ids));
    }
    #[test]
    fn absent_proposed_redo_uses_exact_inverse_save_pair_without_lossy_replay() {
        let directory=TempDir::new().unwrap();let service=Service::new(directory.path());
        let seeded=seed(&service,"draft:proposed-save-pair",true);let ids=ids(&seeded);
        let undone=change(&service,&seeded,"undo","undo",json!({}));
        assert!(undone["changes"].as_array().unwrap().is_empty());assert_eq!(undone["can_redo"],true);
        let first_save=rewrite_save(&service,&undone,"😀|aaOOaa|WRAP|tailOO",json!([{"from":10,"to":16,"insert":"WRAP"}]));
        assert_eq!(first_save["can_redo"],true);
        let second_save=rewrite_save(&service,&first_save,ORIGINAL,json!([{"from":10,"to":14,"insert":"bbOObb"}]));
        assert_eq!(second_save["can_redo"],true);assert_eq!(second_save["content"],ORIGINAL);
        drop(service);let service=Service::new(directory.path());
        let redone=change(&service,&read(&service,&second_save),"redo","redo",json!({}));
        assert_eq!(redone["content"],ORIGINAL);assert_eq!(semantic(&redone,&ids),semantic(&seeded,&ids));
        assert_eq!(redone["generation"],second_save["generation"]);
    }
    #[test]
    fn absent_proposed_redo_refuses_noninverse_same_coordinate_save_pair_atomically() {
        let directory=TempDir::new().unwrap();let service=Service::new(directory.path());
        let seeded=seed(&service,"draft:proposed-noninverse-save",true);
        let undone=change(&service,&seeded,"undo","undo",json!({}));
        assert!(undone["changes"].as_array().unwrap().is_empty());assert_eq!(undone["can_redo"],true);
        let first_save=rewrite_save(&service,&undone,"😀|aaOOaa|WRAP|tailOO",json!([{"from":10,"to":16,"insert":"WRAP"}]));
        let second_save=rewrite_save(&service,&first_save,"😀|aaOOaa|bbXXbb|tailOO",json!([{"from":10,"to":14,"insert":"bbXXbb"}]));
        assert_eq!(second_save["can_redo"],true);assert_eq!(second_save["content"],"😀|aaOOaa|bbXXbb|tailOO");
        let before=read(&service,&second_save);let count=events(&directory);
        let attempt=request(&before,"redo","noninverse-save-redo",json!({}));
        assert_eq!(service.handle(&attempt).unwrap_err(),"review_conflict");
        assert_eq!(read(&service,&second_save),before);assert_eq!(events(&directory),count);
        assert_eq!(service.handle(&json!({"action":"receipt","document_id":before["document_id"],"call_id":"noninverse-save-redo"})).unwrap()["found"],false);
    }
    const MIXED_ZERO_SOURCE: &str = "😀|ab|![保留](owned.svg)\n";
    const MIXED_ZERO_BODY: &str = "😀|β😀|![保留](owned.svg)\n";
    fn mixed_zero_proposal(service: &Service, delete_first: bool) -> Value {
        let first = open_document(service, "draft:mixed-zero-boundary", MIXED_ZERO_SOURCE);
        let edits = if delete_first {
            json!([{"from":3,"to":4,"insert":""},{"from":4,"to":5,"insert":"β😀"}])
        } else {
            json!([{"from":3,"to":4,"insert":"β😀"},{"from":4,"to":5,"insert":""}])
        };
        change(service, &first, "propose", "mixed-propose", json!({
            "origin":"ai","task_id":"mixed-zero-owned","application":"applied","edits":edits
        }))
    }
    fn mixed_zero_ids(head: &Value) -> Vec<Value> {
        assert_eq!(head["content"], MIXED_ZERO_BODY);
        assert_eq!(head["changes"].as_array().unwrap().len(), 2);
        ["a", "b"].iter().map(|original| head["changes"].as_array().unwrap().iter()
            .find(|change| change["original"] == *original).unwrap()["id"].clone()).collect()
    }
    fn mixed_zero_exact(head: &Value, ids: &[Value], delete_first: bool) {
        assert_eq!(head["content"], MIXED_ZERO_BODY);
        assert_eq!(head["changes"].as_array().unwrap().len(), 2);
        let expected = if delete_first {
            [(3, 3, "a", ""), (3, 6, "b", "β😀")]
        } else {
            [(3, 6, "a", "β😀"), (6, 6, "b", "")]
        };
        for (id, (from, to, original, replacement)) in ids.iter().zip(expected) {
            let current = item(head, id);
            assert_eq!(current["from"], from);
            assert_eq!(current["to"], to);
            assert_eq!(current["original"], original);
            assert_eq!(current["replacement"], replacement);
            assert_eq!(current["decision"], "pending");
            assert_eq!(current["application"], "applied");
            assert_eq!(current["taken_over"], false);
            assert_eq!(current["origin"], "ai");
            assert_eq!(current["task_id"], "mixed-zero-owned");
        }
    }
    fn mixed_zero_restart_cycle(delete_first: bool) {
        let directory = TempDir::new().unwrap();
        let service = Service::new(directory.path());
        let proposed = mixed_zero_proposal(&service, delete_first);
        let ids = mixed_zero_ids(&proposed);
        mixed_zero_exact(&proposed, &ids, delete_first);
        let undone = change(&service, &proposed, "undo", "mixed-undo", json!({}));
        assert_eq!(undone["content"], MIXED_ZERO_SOURCE);
        assert!(undone["changes"].as_array().unwrap().is_empty());
        assert_eq!(undone["can_redo"], true);
        drop(service);
        let service = Service::new(directory.path());
        let reopened = read(&service, &undone);
        assert_eq!(reopened["content"], MIXED_ZERO_SOURCE);
        assert!(reopened["changes"].as_array().unwrap().is_empty());
        assert_eq!(reopened["can_redo"], true);
        let redone = change(&service, &reopened, "redo", "mixed-redo", json!({}));
        mixed_zero_exact(&redone, &ids, delete_first);
        assert_eq!(semantic(&redone, &ids), semantic(&proposed, &ids));
        for id in &ids {
            assert!(item(&redone, id)["version"].as_u64().unwrap()
                > item(&proposed, id)["version"].as_u64().unwrap());
            assert!(item(&redone, id)["last_event_id"].as_u64().unwrap()
                > item(&proposed, id)["last_event_id"].as_u64().unwrap());
        }
        drop(service);
        let service = Service::new(directory.path());
        let persisted = read(&service, &redone);
        mixed_zero_exact(&persisted, &ids, delete_first);
        assert_eq!(semantic(&persisted, &ids), semantic(&proposed, &ids));
    }
    #[test]
    fn absent_zero_at_left_edge_of_restored_source_restarts_and_redoes() {
        mixed_zero_restart_cycle(true);
    }
    #[test]
    fn absent_zero_at_right_edge_of_restored_source_restarts_and_redoes() {
        mixed_zero_restart_cycle(false);
    }
    #[test]
    fn absent_zero_exact_empty_source_keeps_its_saved_position() {
        let directory = TempDir::new().unwrap();
        let service = Service::new(directory.path());
        let proposed = mixed_zero_proposal(&service, false);
        let ids = mixed_zero_ids(&proposed);
        mixed_zero_exact(&proposed, &ids, false);
        let undone = change(&service, &proposed, "undo", "mixed-undo", json!({}));
        assert_eq!(undone["content"], MIXED_ZERO_SOURCE);
        assert!(undone["changes"].as_array().unwrap().is_empty());
        assert_eq!(undone["can_redo"], true);
        // B's own exact empty source keeps its point after the independently
        // inserted X; A's neighboring nonempty edge is not competing ownership.
        let saved = rewrite_save(&service, &undone, "😀|aXb|![保留](owned.svg)\n",
            json!([{"from":4,"to":4,"insert":"X"}]));
        assert_eq!(saved["content"], "😀|aXb|![保留](owned.svg)\n");
        assert!(saved["changes"].as_array().unwrap().is_empty());
        assert_eq!(saved["can_redo"], true);
        drop(service);
        let service = Service::new(directory.path());
        let reopened = read(&service, &saved);
        assert_eq!(reopened["can_redo"], true);
        let redone = change(&service, &reopened, "redo", "mixed-saved-redo", json!({}));
        assert_eq!(redone["content"], "😀|β😀X|![保留](owned.svg)\n");
        assert_eq!(redone["changes"].as_array().unwrap().len(), 2);
        assert_eq!(item(&redone, &ids[0])["from"], 3);
        assert_eq!(item(&redone, &ids[0])["to"], 6);
        assert_eq!(item(&redone, &ids[1])["from"], 7);
        assert_eq!(item(&redone, &ids[1])["to"], 7);
        let mut expected = semantic(&proposed, &ids);
        expected[1]["from"] = json!(7);
        expected[1]["to"] = json!(7);
        assert_eq!(semantic(&redone, &ids), expected);
        for id in &ids {
            assert!(item(&redone, id)["version"].as_u64().unwrap()
                > item(&proposed, id)["version"].as_u64().unwrap());
            assert!(item(&redone, id)["last_event_id"].as_u64().unwrap()
                > item(&proposed, id)["last_event_id"].as_u64().unwrap());
        }
        drop(service);
        let service = Service::new(directory.path());
        let persisted = read(&service, &redone);
        assert_eq!(persisted["content"], "😀|β😀X|![保留](owned.svg)\n");
        assert_eq!(persisted["changes"].as_array().unwrap().len(), 2);
        assert_eq!(semantic(&persisted, &ids), expected);
    }
    #[test]
    fn trusted_raw_inverse_replays_support_receipts_export_and_discard() {
        for consumer in ["receipt","export","discard"] {
            let directory=TempDir::new().unwrap();let service=Service::new(directory.path());let first=open_document(&service,&format!("draft:{consumer}"),ORIGINAL);let seed_request=request(&first,"propose","seed",json!({"edits":[{"from":5,"to":7,"insert":"CC"},{"from":12,"to":14,"insert":"CC"},{"from":21,"to":23,"insert":"CC"}]}));let seeded=service.handle(&seed_request).unwrap();let frozen=service.freeze_export_snapshot(&seeded).unwrap();let manual=change(&service,&seeded,"edit","manual",json!({"edits":[{"from":3,"to":5,"insert":""},{"from":5,"to":7,"insert":""}]}));
            match consumer {
                "receipt"=>{let count=events(&directory);assert_eq!(service.handle(&seed_request).unwrap(),seeded);assert_eq!(events(&directory),count);},
                "export"=>{let accepted=service.accept_export_snapshot(&frozen,"verified-export").unwrap();assert_eq!(accepted["accepted_count"],0);assert_eq!(accepted["conflict_count"],3);assert_eq!(accepted["content"],manual["content"]);},
                _=>{let discarded=change(&service,&manual,"discard","discard",json!({"disk_revision":first["disk_revision"],"disk_content":ORIGINAL}));assert_eq!(discarded["content"],ORIGINAL);assert!(discarded["changes"].as_array().unwrap().is_empty());},
            }
        }
    }
}

// Append-only preparation for review_tests.rs; actual Service/SQLite paths.
// Not compiled or executed by the delegate. No helper-only success oracle.
#[cfg(test)]
mod adjacent_withdraw_provenance_regressions {
    use super::*;
    const ORIGINAL: &str = "😀|ab|![保留](owned.svg)\n";
    fn item<'a>(head: &'a Value, id: &Value) -> &'a Value {
        head["changes"].as_array().unwrap().iter().find(|change| change["id"] == *id).unwrap()
    }
    fn semantic(head: &Value, ids: &[Value]) -> Value {
        json!(ids.iter().map(|id| {
            let current = item(head, id);
            ["id", "from", "to", "original", "replacement", "decision", "application", "taken_over",
                "origin", "task_id", "candidate_versions", "dependencies", "restore_original"]
                .iter().map(|key| ((*key).to_string(), current[*key].clone()))
                .collect::<serde_json::Map<String, Value>>()
        }).collect::<Vec<_>>())
    }
    fn save_visible(service: &Service, head: &Value, label: &str) -> Value {
        let body = head["content"].as_str().unwrap();
        let revision = document_transaction::revision(body);
        let prepared = change(service, head, "prepare_save", &format!("{label}-prepare"), json!({
            "content":body, "disk_revision":head["disk_revision"], "expected_disk_revision":revision,
            "saved_content":body, "edit_steps":[]
        }));
        service.handle(&json!({"action":"commit_save", "document_id":head["document_id"],
            "intent_id":prepared["intent_id"], "call_id":format!("{label}-commit"),
            "disk_revision":revision, "content":body})).unwrap()
    }
    fn cycle(mixed: bool, reverse_ids: bool) {
        let directory = TempDir::new().unwrap();
        let service = Service::new(directory.path());
        let opened = open_document(&service, "draft:adjacent-withdraw-provenance", ORIGINAL);
        let replacement = if mixed { "β😀" } else { "" };
        let pending_body = if mixed { "😀|β😀|![保留](owned.svg)\n" } else { "😀||![保留](owned.svg)\n" };
        let proposed = change(&service, &opened, "propose", "adjacent-propose", json!({
            "origin":"ai", "task_id":"adjacent-owned", "edits":[
                {"from":3,"to":4,"insert":""}, {"from":4,"to":5,"insert":replacement}]
        }));
        assert_eq!(proposed["content"], pending_body);
        let ids: Vec<Value> = ["a", "b"].iter().map(|original| {
            let found = proposed["changes"].as_array().unwrap().iter()
                .filter(|change| change["original"] == *original).collect::<Vec<_>>();
            assert_eq!(found.len(), 1);
            found[0]["id"].clone()
        }).collect();
        assert_eq!(proposed["changes"].as_array().unwrap().len(), 2);
        assert_eq!(item(&proposed, &ids[0])["from"], 3);
        assert_eq!(item(&proposed, &ids[0])["to"], 3);
        assert_eq!(item(&proposed, &ids[1])["from"], 3);
        assert_eq!(item(&proposed, &ids[1])["to"], if mixed { 6 } else { 3 });
        let mut requested = ids.clone();
        if reverse_ids { requested.reverse(); }
        let operation = request(&proposed, "resolve", "adjacent-withdraw", json!({
            "decision":"withdraw", "ids":requested
        }));
        let before_events = events(&directory);
        let withdrawn = service.handle(&operation).unwrap();
        assert_eq!(withdrawn["content"], ORIGINAL);
        assert_eq!(withdrawn["review_revision"].as_u64().unwrap(), proposed["review_revision"].as_u64().unwrap() + 1);
        assert_eq!(withdrawn["generation"].as_u64().unwrap(), proposed["generation"].as_u64().unwrap() + 1);
        assert_eq!(events(&directory), before_events + 1);
        let mut expected = semantic(&proposed, &ids);
        for (index, (from, to)) in [(3, 4), (4, 5)].into_iter().enumerate() {
            expected[index]["from"] = json!(from);
            expected[index]["to"] = json!(to);
            expected[index]["decision"] = json!("withdrawn");
            expected[index]["application"] = json!("proposed");
            expected[index]["taken_over"] = json!(false);
        }
        assert_eq!(semantic(&withdrawn, &ids), expected);
        for id in &ids {
            assert!(item(&withdrawn, id)["version"].as_u64().unwrap() > item(&proposed, id)["version"].as_u64().unwrap());
        }
        drop(service);
        let service = Service::new(directory.path());
        let reopened = read(&service, &withdrawn);
        assert_eq!(reopened["content"], ORIGINAL);
        assert_eq!(semantic(&reopened, &ids), expected);
        let undone = change(&service, &reopened, "undo", "adjacent-undo", json!({}));
        assert_eq!(undone["content"], pending_body);
        assert_eq!(semantic(&undone, &ids), semantic(&proposed, &ids));
        assert_eq!(undone["review_revision"].as_u64().unwrap(), reopened["review_revision"].as_u64().unwrap() + 1);
        assert_eq!(undone["can_redo"], true);
        // Trusted save protocol only: no physical I/O claim from a Service unit.
        let saved_pending = save_visible(&service, &undone, "pending-save");
        assert_eq!(saved_pending["content"], pending_body);
        assert_eq!(semantic(&saved_pending, &ids), semantic(&proposed, &ids));
        assert_eq!(saved_pending["can_redo"], true);
        drop(service);
        let service = Service::new(directory.path());
        let pending_reopened = read(&service, &saved_pending);
        assert_eq!(pending_reopened["content"], pending_body);
        assert_eq!(semantic(&pending_reopened, &ids), semantic(&proposed, &ids));
        let redone = change(&service, &pending_reopened, "redo", "adjacent-redo", json!({}));
        assert_eq!(redone["content"], ORIGINAL);
        assert_eq!(semantic(&redone, &ids), expected);
        assert_eq!(redone["review_revision"].as_u64().unwrap(), pending_reopened["review_revision"].as_u64().unwrap() + 1);
        let saved_withdrawn = save_visible(&service, &redone, "withdrawn-save");
        assert_eq!(semantic(&saved_withdrawn, &ids), expected);
        drop(service);
        let service = Service::new(directory.path());
        let final_head = read(&service, &saved_withdrawn);
        assert_eq!(final_head["content"], ORIGINAL);
        assert_eq!(semantic(&final_head, &ids), expected);
        // Exact same mutation request must replay its historical receipt.
        assert_eq!(service.handle(&operation).unwrap(), withdrawn);
    }
    #[test]
    fn withdraw_all_adjacent_deletes_normal_ids_is_one_durable_history_unit() { cycle(false, false); }
    #[test]
    fn withdraw_all_adjacent_deletes_reverse_ids_is_one_durable_history_unit() { cycle(false, true); }
    #[test]
    fn withdraw_all_adjacent_delete_replace_normal_ids_survives_undo_save_restart_redo() { cycle(true, false); }
    #[test]
    fn withdraw_all_adjacent_delete_replace_reverse_ids_survives_undo_save_restart_redo() { cycle(true, true); }

    #[test]
    fn creation_proof_survives_proposal_redo_and_an_unrelated_shift() {
        let directory = TempDir::new().unwrap();
        let service = Service::new(directory.path());
        let opened = open_document(&service, "draft:shifted-withdraw", ORIGINAL);
        let proposed = service.propose_task(&request(&opened, "propose", "trusted-proposal", json!({
            "origin":"ai", "task_id":"owned-adjacent", "edits":[
                {"from":3,"to":4,"insert":""}, {"from":4,"to":5,"insert":"β😀"}]
        }))).unwrap();
        let ids: Vec<_> = ["a", "b"].iter().map(|original| proposed["changes"].as_array().unwrap()
            .iter().find(|change| change["original"] == *original).unwrap()["id"].clone()).collect();
        let undone = change(&service, &proposed, "undo", "undo-proposal", json!({}));
        let redone = change(&service, &undone, "redo", "redo-proposal", json!({}));
        assert_eq!(semantic(&redone, &ids), semantic(&proposed, &ids));
        let shifted = change(&service, &redone, "edit", "unrelated-prefix", json!({
            "edits":[{"from":0,"to":0,"insert":"新😀"}]
        }));
        let saved = save_visible(&service, &shifted, "save-prefixed");
        drop(service);
        let service = Service::new(directory.path());
        let reopened = read(&service, &saved);
        let withdrawn = change(&service, &reopened, "resolve", "withdraw-shifted", json!({
            "decision":"withdraw", "ids":[ids[1],ids[0]]
        }));
        assert_eq!(withdrawn["content"], format!("新😀{ORIGINAL}"));
        assert_eq!(withdrawn["transaction_edits"], json!([{"from":6,"to":9,"insert":"ab"}]));
        assert_eq!(item(&withdrawn, &ids[0])["current"], "a");
        assert_eq!(item(&withdrawn, &ids[1])["current"], "b");
        let undone = change(&service, &withdrawn, "undo", "undo-shifted-withdrawal", json!({}));
        assert_eq!(undone["content"], reopened["content"]);
        assert_eq!(semantic(&undone, &ids), semantic(&reopened, &ids));
    }

    fn stamp(directory: &TempDir, document_id: &str) -> (String, i64, i64, i64) {
        let db = Connection::open_with_flags(directory.path().join("review.sqlite3"), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        (db.query_row("SELECT payload FROM review_documents WHERE id=?1", [document_id], |row| row.get(0)).unwrap(),
         db.query_row("SELECT COUNT(*) FROM review_events WHERE document_id=?1", [document_id], |row| row.get(0)).unwrap(),
         db.query_row("SELECT COUNT(*) FROM review_receipts WHERE document_id=?1", [document_id], |row| row.get(0)).unwrap(),
         db.query_row("SELECT COUNT(*) FROM review_texts", [], |row| row.get(0)).unwrap())
    }
    #[test]
    fn unproven_collapsed_sources_do_not_guess_id_order_or_cross_action_frames() {
        for mode in ["equal-text", "separate-actions"] {
            let directory = TempDir::new().unwrap();
            let service = Service::new(directory.path());
            let initial = if mode == "equal-text" { "😀|aa|![保留](owned.svg)\n" } else { ORIGINAL };
            let opened = open_document(&service, "draft:unproven-withdraw", initial);
            let proposed = if mode == "equal-text" {
                change(&service, &opened, "propose", "seed", json!({"origin":"ai", "edits":[
                    {"from":3,"to":4,"insert":""}, {"from":4,"to":5,"insert":""}]}))
            } else {
                let a = change(&service, &opened, "propose", "seed-a", json!({"origin":"ai", "edits":[{"from":3,"to":4,"insert":""}]}));
                change(&service, &a, "propose", "seed-b", json!({"origin":"ai", "edits":[{"from":3,"to":4,"insert":""}]}))
            };
            assert_eq!(proposed["content"], "😀||![保留](owned.svg)\n");
            let ids = proposed["changes"].as_array().unwrap().iter().map(|change| change["id"].clone()).collect::<Vec<_>>();
            assert_eq!(ids.len(), 2);
            let before = read(&service, &proposed);
            let document_id = before["document_id"].as_str().unwrap();
            let before_db = stamp(&directory, document_id);
            let call = "unproven-withdraw";
            assert_eq!(service.handle(&request(&before, "resolve", call, json!({"decision":"withdraw", "ids":ids}))).unwrap_err(), "transaction_overlapping_edits");
            assert_eq!(read(&service, &proposed), before);
            assert_eq!(stamp(&directory, document_id), before_db);
            assert_eq!(service.handle(&json!({"action":"receipt", "document_id":document_id, "call_id":call})).unwrap()["found"], false);
        }
    }
}
