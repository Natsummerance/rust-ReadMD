//! Synthetic evidence exercises the review journal only. Native file leases
//! and publication have independent real Windows filesystem tests.
use super::*;
use rusqlite::Connection;
use tempfile::TempDir;

struct Fixture {
    directory: TempDir,
    plan: Value,
}

fn raw_revision(content: &str, encoding: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(crate::text_encoding::encode(content, encoding).unwrap())
    )
}

fn file_plan(root: &Path, name: &str, target: &str, before: &str) -> Value {
    let after = before.replace("old.md", "renamed.md");
    let normalized = normalize(before);
    let mut edits = Vec::new();
    for (offset, _) in normalized.match_indices("old.md") {
        let from = normalized[..offset].encode_utf16().count();
        edits.push(json!({"from":from,"to":from+6,"insert":"renamed.md"}));
    }
    json!({"source_path":root.join(name).to_str().unwrap(),"target_path":root.join(target).to_str().unwrap(),
        "before":before,"after":after,"before_disk_revision":raw_revision(before,"utf-8"),
        "after_disk_revision":raw_revision(&after,"utf-8"),"encoding":"utf-8",
        "line_ending":if before.contains("\r\n"){"crlf"}else{"lf"},"edits":edits,
        "review_head":null,"source_identity":format!("synthetic-{}",name.replace('.',"-")),"stage_identity":null,
        "stage_path":root.join(format!(".{name}.refactor-stage")).to_str().unwrap(),
        "backup_path":root.join(format!(".{name}.refactor-backup")).to_str().unwrap()})
}

impl Fixture {
    fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let root = directory.path();
        let plan = json!({"operation_id":"refactor-test-operation","plan_hash":"1".repeat(64),
        "workspace_root":root.to_str().unwrap(),"source_path":root.join("old.md").to_str().unwrap(),
        "target_path":root.join("renamed.md").to_str().unwrap(),"files":[
            file_plan(root,"old.md","renamed.md","# Source\r\n"),
            file_plan(root,"first.md","first.md","alpha beta\r\n\r\n[link](old.md)\r\n"),
            file_plan(root,"second.md","second.md","第二篇 😀 [link](old.md)\n")
        ]});
        Self { directory, plan }
    }
    fn service(&self) -> Service {
        Service::new(self.directory.path())
    }
    fn count(&self, table: &str) -> i64 {
        Connection::open(self.directory.path().join("review.sqlite3"))
            .unwrap()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
    }
    fn bind_heads(&mut self, service: &Service) {
        let paths = self.plan["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|file| string(file, "source_path").unwrap().to_string())
            .collect::<Vec<_>>();
        for (file, head) in self.plan["files"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .zip(service.refactor_heads(&paths).unwrap())
        {
            file["review_head"] = head;
        }
    }
    fn replace_first(&mut self, before: &str) {
        self.plan["files"][1] = file_plan(self.directory.path(), "first.md", "first.md", before);
    }
}

fn open_file(service: &Service, file: &Value) -> Value {
    service.handle(&json!({"action":"open","path":file["source_path"],"content":normalize(string(file,"before").unwrap()),
        "disk_content":normalize(string(file,"before").unwrap()),"disk_revision":file["before_disk_revision"],
        "encoding":file["encoding"],"line_ending":file["line_ending"]})).unwrap()
}

fn change(service: &Service, current: &Value, action: &str, extra: Value) -> Value {
    let mut request = json!({"action":action,"document_id":current["document_id"],
        "generation":current["generation"],"review_revision":current["review_revision"],
        "call_id":format!("test-{}",Uuid::new_v4())});
    request
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    service.handle(&request).unwrap()
}

fn save(service: &Service, current: &Value, raw: &str) -> Value {
    assert_eq!(current["content"], normalize(raw));
    let prepared = change(
        service,
        current,
        "prepare_save",
        json!({"content":current["content"],
        "saved_content":current["content"],"disk_revision":current["disk_revision"],
        "expected_disk_revision":raw_revision(raw,"utf-8"),"encoding":"utf-8",
        "line_ending":if raw.contains("\r\n"){"crlf"}else{"lf"}}),
    );
    service.handle(&json!({"action":"commit_save","document_id":current["document_id"],
        "call_id":format!("commit-{}",Uuid::new_v4()),"intent_id":prepared["save_intent"]["intent_id"],
        "disk_revision":raw_revision(raw,"utf-8"),"content":current["content"],"encoding":"utf-8",
        "line_ending":if raw.contains("\r\n"){"crlf"}else{"lf"}})).unwrap()
}

fn document(service: &Service, head: &Value) -> Document {
    service
        .store
        .run(|tx| with_text_context(tx, || load(tx, string(head, "document_id")?)))
        .unwrap()
}

fn stage_all(service: &Service, mut journal: Value) -> Value {
    for (index, file) in journal["plan"]["files"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        if file["before_disk_revision"] != file["after_disk_revision"] {
            file["stage_identity"] = json!(format!("synthetic-stage-{index}"));
        }
    }
    journal["state"] = json!("applying");
    service
        .refactor_update(string(&journal, "operation_id").unwrap(), &journal)
        .unwrap()
}

#[test]
fn heads_do_not_create_a_store_and_return_ordered_nulls() {
    let fixture = Fixture::new();
    let service = fixture.service();
    let paths = fixture.plan["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| string(file, "source_path").unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        service.refactor_heads(&paths).unwrap(),
        vec![Value::Null; 3]
    );
    assert!(!fixture.directory.path().join("review.sqlite3").exists());
}

#[test]
fn atomic_prepare_reserves_every_path_and_rollback_leaves_clean_sessions_unchanged() {
    let fixture = Fixture::new();
    let service = fixture.service();
    let journal = service.refactor_prepare(&fixture.plan).unwrap();
    assert_eq!(journal["state"], "prepared");
    assert_eq!(journal["heads"].as_array().unwrap().len(), 3);
    assert_eq!(fixture.count("review_documents"), 3);
    assert_eq!(fixture.count("review_refactor_paths"), 10);
    let before = journal["heads"]
        .as_array()
        .unwrap()
        .iter()
        .map(|head| serde_json::to_value(document(&service, head)).unwrap())
        .collect::<Vec<_>>();
    let receipt = service
        .refactor_finish("refactor-test-operation", "rolled_back")
        .unwrap();
    assert_eq!(receipt["state"], "rolled_back");
    assert_eq!(fixture.count("review_refactor_paths"), 0);
    for (head, before) in journal["heads"].as_array().unwrap().iter().zip(before) {
        assert_eq!(
            serde_json::to_value(document(&service, head)).unwrap(),
            before
        );
    }
    assert_eq!(
        service
            .refactor_finish("refactor-test-operation", "committed")
            .unwrap(),
        receipt
    );
    assert_eq!(
        service
            .refactor_record("refactor-test-operation")
            .unwrap()
            .unwrap()["state"],
        "rolled_back"
    );
}

#[test]
fn existing_heads_are_cas_bound_and_failed_admission_rolls_back_every_new_session() {
    let mut fixture = Fixture::new();
    let service = fixture.service();
    let current = open_file(&service, &fixture.plan["files"][1]);
    fixture.bind_heads(&service);
    let edited = change(
        &service,
        &current,
        "edit",
        json!({"edits":[{"from":0,"to":5,"insert":"ALPHA"}]}),
    );
    let before = serde_json::to_value(document(&service, &edited)).unwrap();
    assert_eq!(
        service.refactor_prepare(&fixture.plan),
        Err("review_stale_generation")
    );
    assert_eq!(fixture.count("review_documents"), 1);
    assert_eq!(fixture.count("review_refactors"), 0);
    assert_eq!(fixture.count("review_refactor_paths"), 0);
    assert_eq!(
        serde_json::to_value(document(&service, &edited)).unwrap(),
        before
    );
}

#[test]
fn same_id_replays_immutable_plan_and_stage_identity_is_write_once() {
    let fixture = Fixture::new();
    let service = fixture.service();
    let prepared = service.refactor_prepare(&fixture.plan).unwrap();
    let staged = stage_all(&service, prepared.clone());
    assert_eq!(service.refactor_prepare(&fixture.plan).unwrap(), staged);
    let mut changed = fixture.plan.clone();
    changed["files"][1]["source_identity"] = json!("synthetic-other");
    assert_eq!(
        service.refactor_prepare(&changed),
        Err("review_call_conflict")
    );
    let mut retry = prepared.clone();
    retry["state"] = json!("applying");
    assert_eq!(
        service
            .refactor_update("refactor-test-operation", &retry)
            .unwrap(),
        staged
    );
    let mut changed = staged.clone();
    changed["plan"]["files"][1]["stage_identity"] = json!("synthetic-foreign");
    assert_eq!(
        service.refactor_update("refactor-test-operation", &changed),
        Err("review_call_conflict")
    );
    assert_eq!(
        service
            .refactor_record("refactor-test-operation")
            .unwrap()
            .unwrap(),
        staged
    );
}

#[test]
fn commit_updates_all_documents_once_and_preserves_identity_and_original_line_endings_in_plan() {
    let fixture = Fixture::new();
    let service = fixture.service();
    let prepared = service.refactor_prepare(&fixture.plan).unwrap();
    assert_eq!(
        service.refactor_finish("refactor-test-operation", "committed"),
        Err("review_save_conflict")
    );
    let staged = stage_all(&service, prepared.clone());
    let receipt = service
        .refactor_finish("refactor-test-operation", "committed")
        .unwrap();
    assert_eq!(receipt["state"], "committed");
    assert_eq!(fixture.count("review_refactor_paths"), 0);
    for (index, file) in receipt["files"].as_array().unwrap().iter().enumerate() {
        assert_eq!(
            file["review"]["document_id"],
            prepared["heads"][index]["document_id"]
        );
        assert_eq!(file["path"], fixture.plan["files"][index]["target_path"]);
        assert_eq!(
            file["review"]["content"],
            normalize(string(&fixture.plan["files"][index], "after").unwrap())
        );
        assert_eq!(file["review"]["generation"], if index == 0 { 1 } else { 2 });
        assert_eq!(
            file["identity"],
            if index == 0 {
                fixture.plan["files"][index]["source_identity"].clone()
            } else {
                staged["plan"]["files"][index]["stage_identity"].clone()
            }
        );
        assert_eq!(
            document(&service, &file["review"]).checkpoint.content,
            file["review"]["content"].as_str().unwrap()
        );
    }
    assert_eq!(
        service
            .refactor_finish("refactor-test-operation", "committed")
            .unwrap(),
        receipt
    );
    assert_eq!(
        service.refactor_prepare(&fixture.plan).unwrap()["receipt"],
        receipt
    );
    assert!(staged["plan"]["files"][1]["before"]
        .as_str()
        .unwrap()
        .contains("\r\n"));
}

#[test]
fn disjoint_saved_review_keeps_originals_checkpoint_decisions_and_undo_redo() {
    let mut fixture = Fixture::new();
    let service = fixture.service();
    let opened = open_file(&service, &fixture.plan["files"][1]);
    let proposal = change(
        &service,
        &opened,
        "propose",
        json!({"origin":"ai","edits":[
        {"from":0,"to":5,"insert":"ALPHA"},{"from":6,"to":10,"insert":"BETA"}]}),
    );
    let raw = "ALPHA BETA\r\n\r\n[link](old.md)\r\n";
    let saved = save(&service, &proposal, raw);
    let first = saved["changes"][0]["id"].clone();
    let second = saved["changes"][1]["id"].clone();
    let accepted_first = change(
        &service,
        &saved,
        "resolve",
        json!({"decision":"accept","ids":[first]}),
    );
    let accepted_second = change(
        &service,
        &accepted_first,
        "resolve",
        json!({"decision":"accept","ids":[second.clone()]}),
    );
    let current = change(&service, &accepted_second, "undo", json!({}));
    let before = document(&service, &current);
    assert_eq!(before.changes[0].decision, "accepted");
    assert_eq!(before.checkpoint.changes[0].decision, "pending");
    assert!(!before.undo.is_empty() && !before.redo.is_empty());
    fixture.replace_first(raw);
    fixture.bind_heads(&service);
    let prepared = service.refactor_prepare(&fixture.plan).unwrap();
    stage_all(&service, prepared);
    let receipt = service
        .refactor_finish("refactor-test-operation", "committed")
        .unwrap();
    let state = &receipt["files"][1]["review"];
    let after = document(&service, state);
    assert_eq!(after.changes[0].original.as_str(), "alpha");
    assert_eq!(after.changes[1].original.as_str(), "beta");
    assert_eq!(after.changes[0].decision, "accepted");
    assert_eq!(after.checkpoint.changes[0].decision, "pending");
    assert_eq!(
        serde_json::to_value(&after.undo).unwrap(),
        serde_json::to_value(&before.undo).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&after.redo).unwrap(),
        serde_json::to_value(&before.redo).unwrap()
    );
    let redone = change(&service, state, "redo", json!({}));
    assert_eq!(
        redone["changes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|change| change["id"] == second)
            .unwrap()["decision"],
        "accepted"
    );
    let discarded = change(
        &service,
        &redone,
        "discard",
        json!({"disk_revision":state["disk_revision"],"disk_content":state["content"]}),
    );
    assert_eq!(
        discarded["content"],
        normalize(string(&fixture.plan["files"][1], "after").unwrap())
    );
}

#[test]
fn pending_and_proposed_overlap_fail_before_any_group_is_admitted() {
    for applied in [true, false] {
        let mut fixture = Fixture::new();
        let service = fixture.service();
        let opened = open_file(&service, &fixture.plan["files"][1]);
        let content = string(&opened, "content").unwrap();
        let byte = content.find("old.md").unwrap();
        let from = content[..byte].encode_utf16().count();
        let proposal = change(
            &service,
            &opened,
            "propose",
            json!({"origin":"ai",
            "application":if applied{"applied"}else{"proposed"},"edits":[{"from":from,"to":from+6,"insert":"old.md"}]}),
        );
        let current = if applied {
            save(
                &service,
                &proposal,
                string(&fixture.plan["files"][1], "before").unwrap(),
            )
        } else {
            proposal
        };
        fixture.bind_heads(&service);
        let before = serde_json::to_value(document(&service, &current)).unwrap();
        assert_eq!(
            service.refactor_prepare(&fixture.plan),
            Err("refactor_review_overlap")
        );
        assert_eq!(fixture.count("review_documents"), 1);
        assert_eq!(fixture.count("review_refactors"), 0);
        assert_eq!(fixture.count("review_refactor_paths"), 0);
        assert_eq!(
            serde_json::to_value(document(&service, &current)).unwrap(),
            before
        );
    }
}

#[test]
fn overlapping_historical_content_undo_is_rejected_without_rewriting_its_expected_text() {
    let mut fixture = Fixture::new();
    let service = fixture.service();
    let opened = open_file(&service, &fixture.plan["files"][1]);
    let content = string(&opened, "content").unwrap();
    let byte = content.find("old.md").unwrap();
    let from = content[..byte].encode_utf16().count();
    let changed = change(
        &service,
        &opened,
        "edit",
        json!({"edits":[{"from":from,"to":from+6,"insert":"old.md"}]}),
    );
    let saved = save(
        &service,
        &changed,
        string(&fixture.plan["files"][1], "before").unwrap(),
    );
    fixture.bind_heads(&service);
    let before = serde_json::to_value(document(&service, &saved)).unwrap();
    assert_eq!(
        service.refactor_prepare(&fixture.plan),
        Err("refactor_undo_overlap")
    );
    assert_eq!(fixture.count("review_refactors"), 0);
    assert_eq!(
        serde_json::to_value(document(&service, &saved)).unwrap(),
        before
    );
}

#[test]
fn insufficient_budget_rolls_back_new_sessions_journal_events_and_reservations() {
    let fixture = Fixture::new();
    let service = Service::with_limit(fixture.directory.path(), 1024);
    assert_eq!(
        service.refactor_prepare(&fixture.plan),
        Err("review_budget")
    );
    for table in [
        "review_documents",
        "review_refactors",
        "review_refactor_paths",
        "review_events",
    ] {
        assert_eq!(fixture.count(table), 0, "{table}");
    }
}

#[test]
fn reserved_completion_headroom_survives_stage_growth_and_pending_status_at_the_exact_cap() {
    let fixture = Fixture::new();
    let service = fixture.service();
    let prepared = service.refactor_prepare(&fixture.plan).unwrap();
    let charged = service.store.run(review_store::used_bytes).unwrap();
    drop(service);
    let service = Service::with_limit(fixture.directory.path(), charged);
    stage_all(&service, prepared);
    assert_eq!(
        service
            .refactor_finish("refactor-test-operation", "recovery_pending")
            .unwrap()["state"],
        "recovery_pending"
    );
    assert!(service.store.run(review_store::used_bytes).unwrap() <= charged);
    assert_eq!(
        service
            .refactor_finish("refactor-test-operation", "committed")
            .unwrap()["state"],
        "committed"
    );
}

#[test]
fn duplicate_case_alias_sessions_fail_closed() {
    let fixture = Fixture::new();
    let service = fixture.service();
    let file = &fixture.plan["files"][1];
    open_file(&service, file);
    let mut alias = file.clone();
    alias["source_path"] = json!(string(file, "source_path").unwrap().to_ascii_uppercase());
    open_file(&service, &alias);
    assert_eq!(
        service.refactor_heads(&[string(file, "source_path").unwrap().to_string()]),
        Err("review_save_conflict")
    );
}

#[test]
fn abandoned_unstarted_operation_is_durable_and_cannot_be_started_by_a_delayed_request() {
    let fixture = Fixture::new();
    let service = fixture.service();
    let id = string(&fixture.plan, "operation_id").unwrap();
    let hash = string(&fixture.plan, "plan_hash").unwrap();
    let tombstone = service.refactor_abandon(id, hash).unwrap();
    assert_eq!(tombstone["state"], "abandoned");
    drop(service);
    let service = fixture.service();
    assert_eq!(service.refactor_prepare(&fixture.plan).unwrap(), tombstone);
    assert_eq!(service.refactor_abandon(id, hash).unwrap(), tombstone);
    assert_eq!(
        service.refactor_abandon(id, &"2".repeat(64)),
        Err("review_call_conflict")
    );
    assert_eq!(fixture.count("review_documents"), 0);
    assert_eq!(fixture.count("review_refactor_paths"), 0);
}

#[test]
fn a_durable_rollback_decision_survives_pending_status_and_restart() {
    let fixture = Fixture::new();
    let service = fixture.service();
    service.refactor_prepare(&fixture.plan).unwrap();
    service
        .refactor_update("refactor-test-operation", &json!({"state":"rolling_back"}))
        .unwrap();
    let pending = service
        .refactor_finish("refactor-test-operation", "recovery_pending")
        .unwrap();
    assert_eq!(pending["rollback_requested"], true);
    drop(service);
    let service = fixture.service();
    assert_eq!(
        service.refactor_update("refactor-test-operation", &json!({"state":"applying"})),
        Err("review_call_conflict")
    );
    assert_eq!(
        service.refactor_finish("refactor-test-operation", "committed"),
        Err("review_call_conflict")
    );
    assert_eq!(
        service
            .refactor_finish("refactor-test-operation", "rolled_back")
            .unwrap()["state"],
        "rolled_back"
    );
}
