//! Protocol tests use synthetic evidence deliberately: these verify durable
//! review transactions, not native no-replace or file-identity guarantees.
use super::*;
use rusqlite::Connection;
use std::fs;
use tempfile::TempDir;

struct Fixture {
    directory: TempDir,
    source: String,
    target: String,
}
impl Fixture {
    fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let source = directory.path().join("original.md").to_str().unwrap().into();
        let target = directory.path().join("renamed.md").to_str().unwrap().into();
        Self { directory, source, target }
    }
    fn service(&self) -> Service { Service::new(self.directory.path()) }
    fn request(&self, operation: &str, raw: &str) -> Value {
        json!({"operation_id":operation,"source_path":self.source,"target_path":self.target,
            "source_identity":"synthetic-original","expected_disk_revision":document_transaction::revision(raw),
            "disk_revision":document_transaction::revision(raw),"disk_content":normalize(raw),
            "encoding":if raw.starts_with('\u{feff}'){"utf-8-sig"}else{"utf-8"},
            "line_ending":if raw.contains("\r\n"){"crlf"}else{"lf"}})
    }
    fn original(&self, raw: &str) -> RenameObservation {
        RenameObservation::Regular { identity:"synthetic-original".into(), revision:document_transaction::revision(raw) }
    }
}
fn open_saved(service: &Service, path: &str, raw: &str) -> Value {
    service.handle(&json!({"action":"open","path":path,"content":normalize(raw),"disk_content":normalize(raw),
        "disk_revision":document_transaction::revision(raw),"encoding":"utf-8","line_ending":"lf"})).unwrap()
}
fn command(state: &Value, action: &str, call: &str, extra: Value) -> Value {
    let mut body=json!({"action":action,"call_id":call,"document_id":state["document_id"],
        "generation":state["generation"],"review_revision":state["review_revision"]});
    body.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone()); body
}
fn change(service: &Service, state: &Value, action: &str, call: &str, extra: Value) -> Value {
    service.handle(&command(state,action,call,extra)).unwrap()
}
fn document(service: &Service, state: &Value) -> Value {
    service.store.run(|tx|with_text_context(tx,|| {
        serde_json::to_value(load(tx,state["document_id"].as_str().unwrap())?).map_err(|_|"review_persistence_failed")
    })).unwrap()
}
fn without_rename_bookkeeping(mut value: Value) -> Value {
    for field in ["path","last_event_id","rename_intent"] { value.as_object_mut().unwrap().remove(field); }
    value
}
fn event_count(fixture: &Fixture) -> i64 {
    Connection::open(fixture.directory.path().join("review.sqlite3")).unwrap()
        .query_row("SELECT COUNT(*) FROM review_events",[],|row|row.get(0)).unwrap()
}
fn save_review(service: &Service, state: &Value) -> Value {
    let raw=state["content"].as_str().unwrap();
    let prepared=change(service,state,"prepare_save","save-prepare",json!({"content":raw,
        "disk_revision":state["disk_revision"],"expected_disk_revision":document_transaction::revision(raw),"saved_content":raw}));
    service.handle(&json!({"action":"commit_save","document_id":state["document_id"],"call_id":"save-commit",
        "intent_id":prepared["save_intent"]["intent_id"],"disk_revision":document_transaction::revision(raw),"content":raw})).unwrap()
}

#[test]
fn abandoned_unstarted_id_survives_restart_and_never_runs_a_delayed_request() {
    let fixture=Fixture::new();
    let request=fixture.request("abandoned-operation","saved");
    let service=fixture.service();
    assert_eq!(service.abandon_unstarted_rename("abandoned-operation").unwrap()["state"],"abandoned");
    drop(service);
    let service=fixture.service();
    assert_eq!(service.rename_status("abandoned-operation").unwrap().unwrap()["state"],"abandoned");
    assert_eq!(service.rename_replay(&request).unwrap().unwrap()["state"],"abandoned");
    assert_eq!(service.prepare_rename(&request).unwrap()["state"],"abandoned");
    assert_eq!(event_count(&fixture),0);
}

#[test]
fn abandoning_an_already_prepared_id_never_erases_the_real_intent() {
    let fixture=Fixture::new(); let service=fixture.service();
    let prepared=service.prepare_rename(&fixture.request("already-started","saved")).unwrap();
    assert_eq!(service.abandon_unstarted_rename("already-started").unwrap(),prepared);
    assert_eq!(service.rename_status("already-started").unwrap().unwrap(),prepared);
}

#[test]
fn rename_without_session_creates_clean_session_and_reserves_both_paths_atomically() {
    let fixture=Fixture::new(); let service=fixture.service(); let request=fixture.request("new-session","saved\n");
    let prepared=service.prepare_rename(&request).unwrap();
    assert_eq!(prepared["state"],"prepared"); assert_eq!(prepared["ok"],false); assert!(prepared["moved"].is_null());
    assert_eq!(prepared["review"]["path"],fixture.source); assert_eq!(prepared["target_path"],fixture.target);
    assert_eq!(prepared["review"]["status"],"rename_pending");
    assert_eq!(prepared["review"]["rename"]["operation_id"],"new-session");
    for path in [&fixture.source,&fixture.target] {
        assert_eq!(service.handle(&json!({"action":"open","path":path,"content":"saved\n","disk_revision":request["disk_revision"]})).unwrap_err(),"rename_recovery_pending");
        let found=service.handle(&json!({"action":"lookup","path":path})).unwrap();
        assert_eq!(found["snapshot"]["document_id"],prepared["review"]["document_id"]);
    }
    assert!(prepared.to_string().find("synthetic-original").is_none(),"file identities are not public snapshot fields");
}

#[test]
fn existing_saved_session_is_used_even_without_review_session_and_null_replays() {
    let fixture=Fixture::new(); let service=fixture.service();
    let saved=open_saved(&service,&fixture.source,"saved");
    let mut request=fixture.request("implicit-session","saved");
    let prepared=service.prepare_rename(&request).unwrap();
    assert_eq!(prepared["review"]["document_id"],saved["document_id"]);
    request["review_session"]=Value::Null;
    assert_eq!(service.rename_replay(&request).unwrap().unwrap(),prepared);
    let count=event_count(&fixture);
    assert_eq!(service.prepare_rename(&request).unwrap(),prepared); assert_eq!(event_count(&fixture),count);
    request["target_path"]=json!(fixture.directory.path().join("another.md").to_str().unwrap());
    assert_eq!(service.rename_replay(&request).unwrap_err(),"review_call_conflict");
}

#[test]
fn rename_admission_rejects_stale_cas_unsaved_text_and_raw_revision_conflict() {
    let fixture=Fixture::new(); let service=fixture.service(); let saved=open_saved(&service,&fixture.source,"saved");
    let edited=change(&service,&saved,"edit","edit",json!({"edits":[{"from":0,"to":5,"insert":"unsaved"}]}));
    let before=document(&service,&edited); let mut request=fixture.request("stale","saved");
    request["review_session"]=saved.clone();
    assert_eq!(service.prepare_rename(&request).unwrap_err(),"review_stale_generation");
    request.as_object_mut().unwrap().remove("review_session");
    assert_eq!(service.prepare_rename(&request).unwrap_err(),"rename_unsaved_review");
    let mut external=fixture.request("external","external"); external["expected_disk_revision"]=json!(document_transaction::revision("external"));
    assert_eq!(service.prepare_rename(&external).unwrap_err(),"rename_revision_conflict");
    let mut wrong_expected=fixture.request("wrong-expected","saved"); wrong_expected["expected_disk_revision"]=json!(document_transaction::revision("different"));
    assert_eq!(service.prepare_rename(&wrong_expected).unwrap_err(),"rename_revision_conflict");
    assert_eq!(document(&service,&edited),before); assert!(service.rename_status("stale").unwrap().is_none());
}

#[test]
fn rename_admission_rejects_save_intent_and_external_recovery() {
    let fixture=Fixture::new(); let service=fixture.service(); let saved=open_saved(&service,&fixture.source,"saved");
    let prepared=change(&service,&saved,"prepare_save","save-pending",json!({"content":"saved","disk_revision":saved["disk_revision"],"expected_disk_revision":saved["disk_revision"]}));
    assert_eq!(service.prepare_rename(&fixture.request("during-save","saved")).unwrap_err(),"review_save_in_progress");
    change(&service,&prepared,"abort_save","abort",json!({"intent_id":prepared["save_intent"]["intent_id"]}));
    open_saved(&service,&fixture.source,"external");
    assert_eq!(service.prepare_rename(&fixture.request("during-recovery","external")).unwrap_err(),"review_needs_recovery");
}

#[test]
fn failed_rename_budget_rolls_back_session_and_journal_before_a_file_move() {
    let fixture=Fixture::new(); let service=Service::with_limit(fixture.directory.path(),1024);
    fs::write(&fixture.source,b"saved").unwrap();
    assert_eq!(service.prepare_rename(&fixture.request("no-budget","saved")).unwrap_err(),"review_budget");
    let db=Connection::open(fixture.directory.path().join("review.sqlite3")).unwrap();
    for table in ["review_documents","review_events","review_renames"] {
        let count:i64=db.query_row(&format!("SELECT COUNT(*) FROM {table}"),[],|row|row.get(0)).unwrap(); assert_eq!(count,0);
    }
    assert_eq!(fs::read(&fixture.source).unwrap(),b"saved"); assert!(!Path::new(&fixture.target).exists());
}

#[test]
fn pending_marker_uses_reserved_headroom_at_the_exact_logical_budget() {
    let fixture=Fixture::new(); let service=fixture.service();
    service.prepare_rename(&fixture.request("exact-cap","saved")).unwrap();
    let charged=service.store.run(review_store::used_bytes).unwrap(); drop(service);
    let service=Service::with_limit(fixture.directory.path(),charged);
    let pending=service.recover_rename("exact-cap",&RenameObservation::Unavailable,&RenameObservation::Unavailable).unwrap();
    assert_eq!(pending["state"],"recovery_pending"); assert!(service.store.run(review_store::used_bytes).unwrap()<=charged);
    let committed=service.recover_rename("exact-cap",&RenameObservation::Missing,&fixture.original("saved")).unwrap(); assert_eq!(committed["state"],"committed");
}

#[test]
fn rename_reserves_event_counter_capacity_before_admitting_a_move() {
    let fixture=Fixture::new(); let service=fixture.service(); let saved=open_saved(&service,&fixture.source,"saved");
    service.store.run(|tx|with_text_context(tx,|| {
        let mut state=load(tx,saved["document_id"].as_str().unwrap())?;
        state.last_event_id=i64::MAX as u64-2; persist(tx,&state)
    })).unwrap();
    let before=document(&service,&saved);
    assert_eq!(service.prepare_rename(&fixture.request("counter-cap","saved")).unwrap_err(),"review_budget");
    assert_eq!(document(&service,&saved),before); assert!(service.rename_status("counter-cap").unwrap().is_none());
}

#[test]
fn pending_rename_blocks_all_review_mutations_across_independent_services() {
    let fixture=Fixture::new(); let owner=fixture.service(); let other=fixture.service();
    let saved=open_saved(&owner,&fixture.source,"saved"); let frozen=owner.freeze_export_snapshot(&saved).unwrap();
    owner.prepare_rename(&fixture.request("pending-mutations","saved")).unwrap(); let before=document(&owner,&saved);
    for action in ["edit","propose","resolve","undo","redo","discard","prepare_save","commit_save","abort_save"] {
        assert_eq!(other.handle(&command(&saved,action,action,json!({}))).unwrap_err(),"rename_recovery_pending", "{action}");
    }
    assert_eq!(other.propose_task(&command(&saved,"propose","task",json!({"origin":"ai","task_id":"task","edits":[]}))).unwrap_err(),"rename_recovery_pending");
    assert_eq!(other.freeze_export_snapshot(&saved).unwrap_err(),"rename_recovery_pending");
    assert_eq!(other.accept_export_snapshot(&frozen,"export").unwrap_err(),"rename_recovery_pending");
    assert_eq!(document(&owner,&saved),before);
}

#[test]
fn restart_before_move_cancels_and_preserves_existing_review() {
    let fixture=Fixture::new(); let owner=fixture.service(); let saved=open_saved(&owner,&fixture.source,"saved");
    let before=without_rename_bookkeeping(document(&owner,&saved)); owner.prepare_rename(&fixture.request("before-move","saved")).unwrap(); drop(owner);
    let restarted=fixture.service(); let cancelled=restarted.recover_rename("before-move",&fixture.original("saved"),&RenameObservation::Missing).unwrap();
    assert_eq!(cancelled["state"],"cancelled"); assert_eq!(cancelled["moved"],false); assert_eq!(cancelled["path"],fixture.source);
    assert_eq!(cancelled["error_code"],"rename_cancelled"); assert_eq!(cancelled["review"]["document_id"],saved["document_id"]);
    assert_eq!(without_rename_bookkeeping(document(&restarted,&saved)),before);
    let count=event_count(&fixture); assert_eq!(restarted.recover_rename("before-move",&RenameObservation::Unavailable,&RenameObservation::Unavailable).unwrap(),cancelled);
    assert_eq!(event_count(&fixture),count);
}

#[test]
fn restart_after_move_commits_only_path_and_keeps_saved_checkpoint_undo_redo_and_originals() {
    let fixture=Fixture::new(); let owner=fixture.service(); let initial=open_saved(&owner,&fixture.source,"Alpha Beta");
    let proposed=change(&owner,&initial,"propose","proposal",json!({"origin":"ai","edits":[{"from":0,"to":5,"insert":"alpha"}]}));
    let saved=save_review(&owner,&proposed);
    // Persist a metadata decision after saving. A rename must not promote it
    // into the saved checkpoint as finish_intent would do.
    let accepted=change(&owner,&saved,"resolve","accept",json!({"decision":"accept","ids":[saved["changes"][0]["id"]]}));
    let undone=change(&owner,&accepted,"undo","undo-decision",json!({}));
    let before=without_rename_bookkeeping(document(&owner,&undone));
    assert_eq!(before["checkpoint"]["changes"][0]["decision"],"pending");
    let raw=undone["content"].as_str().unwrap(); fs::write(&fixture.source,raw).unwrap();
    owner.prepare_rename(&fixture.request("after-move",raw)).unwrap();
    // Only the journal recovery is under test; native no-replace has dedicated
    // tests in document_operations and is not claimed by this synthetic move.
    fs::rename(&fixture.source,&fixture.target).unwrap(); drop(owner);
    let restarted=fixture.service(); let committed=restarted.recover_rename("after-move",&RenameObservation::Missing,&fixture.original(raw)).unwrap();
    assert_eq!(committed["state"],"committed"); assert_eq!(committed["ok"],true); assert_eq!(committed["moved"],true);
    assert_eq!(committed["review"]["document_id"],initial["document_id"]); assert_eq!(committed["review"]["path"],fixture.target);
    assert_eq!(committed["review"]["changes"][0]["original"],"Alpha"); assert_eq!(committed["review"]["can_redo"],true);
    assert_eq!(without_rename_bookkeeping(document(&restarted,&committed["review"])),before);
    let redone=change(&restarted,&committed["review"],"redo","redo-after-rename",json!({}));
    assert_eq!(redone["changes"][0]["decision"],"accepted");
    let discarded=change(&restarted,&redone,"discard","discard-after-rename",json!({"disk_revision":redone["disk_revision"],"disk_content":raw}));
    // Discard restores saved content but intentionally preserves an accepted
    // review decision for unchanged text (the established review contract).
    assert_eq!(discarded["changes"][0]["decision"],"accepted"); assert_eq!(discarded["content"],raw);
    assert_eq!(fs::read(&fixture.target).unwrap(),raw.as_bytes()); assert!(!Path::new(&fixture.source).exists());
}

#[test]
fn same_hash_in_foreign_target_is_never_committed_and_neither_file_is_removed() {
    let fixture=Fixture::new(); let service=fixture.service(); fs::write(&fixture.source,b"saved").unwrap();
    service.prepare_rename(&fixture.request("foreign-target","saved")).unwrap(); fs::write(&fixture.target,b"saved").unwrap();
    let foreign=RenameObservation::Regular { identity:"synthetic-foreign".into(),revision:document_transaction::revision("saved") };
    let cancelled=service.recover_rename("foreign-target",&fixture.original("saved"),&foreign).unwrap();
    assert_eq!(cancelled["state"],"cancelled"); assert_eq!(fs::read(&fixture.source).unwrap(),b"saved"); assert_eq!(fs::read(&fixture.target).unwrap(),b"saved");
}

#[test]
fn ambiguous_endpoints_keep_intent_block_mutations_and_can_later_recover_idempotently() {
    let fixture=Fixture::new(); let service=fixture.service(); let prepared=service.prepare_rename(&fixture.request("ambiguous","saved")).unwrap();
    let original=fixture.original("saved"); let foreign=RenameObservation::Regular { identity:"other".into(),revision:document_transaction::revision("saved") };
    for (source,target) in [(&RenameObservation::Missing,&foreign),(&original,&original),(&RenameObservation::Missing,&RenameObservation::Missing),(&RenameObservation::Unavailable,&original)] {
        let pending=service.recover_rename("ambiguous",source,target).unwrap(); assert_eq!(pending["state"],"recovery_pending"); assert!(pending["moved"].is_null());
        assert_eq!(pending["error_code"],"rename_recovery_pending"); assert_eq!(pending["review"]["path"],fixture.source);
    }
    let count=event_count(&fixture);
    service.recover_rename("ambiguous",&RenameObservation::Unavailable,&original).unwrap(); assert_eq!(event_count(&fixture),count);
    assert_eq!(service.handle(&command(&prepared["review"],"undo","blocked",json!({}))).unwrap_err(),"rename_recovery_pending");
    let committed=service.recover_rename("ambiguous",&RenameObservation::Missing,&original).unwrap(); let count=event_count(&fixture);
    assert_eq!(service.recover_rename("ambiguous",&RenameObservation::Missing,&original).unwrap(),committed); assert_eq!(event_count(&fixture),count);
}

#[test]
fn moved_file_and_sql_write_fault_retain_a_retryable_intent_without_rollback() {
    let fixture=Fixture::new(); let service=fixture.service(); fs::write(&fixture.source,b"saved").unwrap();
    let prepared=service.prepare_rename(&fixture.request("sql-fault","saved")).unwrap(); fs::rename(&fixture.source,&fixture.target).unwrap();
    let db=Connection::open(fixture.directory.path().join("review.sqlite3")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_rename_commit BEFORE INSERT ON review_events WHEN NEW.kind='commit_rename' BEGIN SELECT RAISE(ABORT,'synthetic fault'); END").unwrap();
    assert_eq!(service.recover_rename("sql-fault",&RenameObservation::Missing,&fixture.original("saved")).unwrap_err(),"review_persistence_failed");
    assert_eq!(service.rename_status("sql-fault").unwrap().unwrap()["state"],"prepared");
    assert_eq!(document(&service,&prepared["review"])["path"],fixture.source); assert_eq!(fs::read(&fixture.target).unwrap(),b"saved"); assert!(!Path::new(&fixture.source).exists());
    db.execute_batch("DROP TRIGGER reject_rename_commit").unwrap(); drop(db); drop(service);
    let restarted=fixture.service(); assert_eq!(restarted.recover_rename("sql-fault",&RenameObservation::Missing,&fixture.original("saved")).unwrap()["state"],"committed");
}

#[test]
fn operation_retry_uses_current_document_path_after_a_later_rename() {
    let fixture=Fixture::new(); let service=fixture.service(); let first=fixture.request("first","saved");
    service.prepare_rename(&first).unwrap(); service.recover_rename("first",&RenameObservation::Missing,&fixture.original("saved")).unwrap();
    let mut second=fixture.request("second","saved"); second["source_path"]=json!(fixture.target); let third=fixture.directory.path().join("third.md").to_str().unwrap().to_owned(); second["target_path"]=json!(third);
    service.prepare_rename(&second).unwrap(); service.recover_rename("second",&RenameObservation::Missing,&fixture.original("saved")).unwrap();
    let replay=service.rename_replay(&first).unwrap().unwrap(); assert_eq!(replay["path"],third); assert_eq!(replay["target_path"],fixture.target); assert_eq!(replay["review"]["path"],third);
}

#[test]
fn schema_two_migrates_without_changing_payload_receipts_or_retained_text() {
    let fixture=Fixture::new(); let service=fixture.service(); let initial=open_saved(&service,&fixture.source,"original");
    let proposal=change(&service,&initial,"propose","old-proposal",json!({"edits":[{"from":0,"to":8,"insert":"candidate"}]}));
    let expected=service.handle(&json!({"action":"read","document_id":proposal["document_id"]})).unwrap();
    drop(service); let db=Connection::open(fixture.directory.path().join("review.sqlite3")).unwrap();
    let before:String=db.query_row("SELECT payload FROM review_documents",[],|row|row.get(0)).unwrap();
    let receipt:String=db.query_row("SELECT payload FROM review_receipts",[],|row|row.get(0)).unwrap();
    let texts:i64=db.query_row("SELECT COUNT(*) FROM review_texts",[],|row|row.get(0)).unwrap();
    db.execute_batch("DROP TABLE review_renames; PRAGMA user_version=2").unwrap(); drop(db);
    let service=fixture.service(); let found=service.handle(&json!({"action":"read","document_id":proposal["document_id"]})).unwrap(); assert_eq!(found,expected);
    let db=Connection::open(fixture.directory.path().join("review.sqlite3")).unwrap();
    assert_eq!(db.query_row("SELECT payload FROM review_documents",[],|row|row.get::<_,String>(0)).unwrap(),before);
    assert_eq!(db.query_row("SELECT payload FROM review_receipts",[],|row|row.get::<_,String>(0)).unwrap(),receipt);
    assert_eq!(db.query_row("SELECT COUNT(*) FROM review_texts",[],|row|row.get::<_,i64>(0)).unwrap(),texts);
    let version:i64=db.pragma_query_value(None,"user_version",|row|row.get(0)).unwrap(); assert_eq!(version,crate::review_store::SCHEMA_VERSION); assert!(version>2,"an old version-2 kernel must reject before mutation");
}

#[test]
fn pending_rename_limit_is_checked_without_deleting_recovery_records() {
    let fixture=Fixture::new(); let service=fixture.service(); let request=fixture.request("active","saved");
    service.prepare_rename(&request).unwrap();
    service.store.run(|tx| {
        for index in 1..MAX_PENDING_RENAMES {
            review_store::put_rename(tx,&format!("synthetic-{index}"),"unused",&format!("source-{index}"),&format!("target-{index}"),"prepared","{}")?;
        } Ok(())
    }).unwrap();
    let mut another=request.clone(); another["operation_id"]=json!("over-limit"); another["source_path"]=json!(fixture.directory.path().join("other.md").to_str().unwrap()); another["target_path"]=json!(fixture.directory.path().join("other-new.md").to_str().unwrap());
    assert_eq!(service.prepare_rename(&another).unwrap_err(),"review_budget"); assert_eq!(service.rename_status("active").unwrap().unwrap()["state"],"prepared");
}
