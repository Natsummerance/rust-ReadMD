use super::*;

fn tx(source: &str, from: usize, to: usize, insert: &str) -> ContentTransaction {
    ContentTransaction { base_revision: document_transaction::revision(source), origin:"review".into(), edits:vec![TextEdit {from,to,insert:insert.into()}] }
}
fn accepted(service: &Service, owner: &Credential) -> String {
    let review = service.review(&owner.room,&owner.token,json!({"text":"Use B","transaction":tx("abc🦀",1,2,"B")})).unwrap();
    service.resolve(&owner.room,&owner.token,&review,true).unwrap();
    review
}
fn stamp(service: &Service, owner: &Credential) -> (i64,String) {
    let value = service.snapshot(&owner.room,&owner.token).unwrap();
    (value["generation"].as_i64().unwrap(),value["revision"].as_str().unwrap().to_string())
}
fn row(service: &Service, owner: &Credential, review: &str) -> Value {
    service.reviews(&owner.room,&owner.token).unwrap()["reviews"].as_array().unwrap().iter().find(|r|r["id"]==review).unwrap().clone()
}
fn client(service: &Service, owner: &Credential) -> Doc {
    let doc = fresh();
    let update = service.sync(&owner.room,&owner.token,&STANDARD.encode([0]),None,None).unwrap();
    doc.transact_mut().apply_update(Update::decode_v1(&STANDARD.decode(update["update"].as_str().unwrap()).unwrap()).unwrap()).unwrap();
    doc
}
fn send(service: &Service, owner: &Credential, doc: &Doc, before: &StateVector) {
    service.sync(&owner.room,&owner.token,&STANDARD.encode(before.encode_v1()),Some(&STANDARD.encode(doc.transact().encode_state_as_update_v1(before))),None).unwrap();
}

#[test]
fn reversal_retains_decision_and_retries_once_across_restart_and_later_edits() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("db");
    let service = Service::open(&path).unwrap();
    let owner = service.create("Reverse", "abc🦀", "Owner").unwrap();
    let editor = service.invite(&owner.room,&owner.token,Role::Editor,"Editor").unwrap();
    let review = accepted(&service,&owner);
    let original = row(&service,&owner,&review);
    assert_eq!(original["can_reverse"],true);
    let (generation,revision) = stamp(&service,&owner);
    let first = service.reverse_review(&editor.room,&editor.token,&review,generation,&revision).unwrap();
    assert_eq!(first["replayed"],false);
    assert_eq!(service.snapshot(&owner.room,&owner.token).unwrap()["content"],"abc🦀");
    assert_eq!(stamp(&service,&owner).0,generation+1);
    let reversed = row(&service,&owner,&review);
    assert_eq!(reversed["body"],original["body"]);
    assert_eq!(reversed["status"],"accepted");
    assert_eq!(reversed["can_reverse"],false);
    assert_eq!(reversed["reversal"],first["reversal"]);
    assert_eq!(reversed["reversal"]["member"],editor.member);
    let doc = client(&service,&owner);
    let vector = doc.transact().state_vector();
    doc.get_or_insert_text("content").insert(&mut doc.transact_mut(),0,"Later ");
    send(&service,&owner,&doc,&vector);
    drop(service);
    let service = Service::open(&path).unwrap();
    let before = service.snapshot(&owner.room,&owner.token).unwrap();
    let retry = service.reverse_review(&editor.room,&editor.token,&review,generation,&revision).unwrap();
    assert_eq!(retry["replayed"],true);
    assert_eq!(retry["reversal"],first["reversal"]);
    assert_eq!(service.snapshot(&owner.room,&owner.token).unwrap(),before);
    assert_eq!(before["content"],"Later abc🦀");
    assert_eq!(row(&service,&owner,&review)["body"],original["body"]);
    assert_eq!(service.reverse_review(&owner.room,&owner.token,&review,generation,&revision).unwrap_err().status,409);
    service.permission(&owner.room,&owner.token,&editor.member,Some(Role::Reviewer)).unwrap();
    assert_eq!(service.reverse_review(&editor.room,&editor.token,&review,generation,&revision).unwrap_err().status,403);
    let state = service.state.lock().unwrap();
    assert_eq!(state.db.query_row("SELECT count(*) FROM review_reversals",[],|r|r.get::<_,i64>(0)).unwrap(),1);
}

#[test]
fn reversal_rejects_stale_revision_aba_and_unauthorized_members_without_mutation() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("db");
    let service = Service::open(&path).unwrap();
    let owner = service.create("ABA", "abc🦀", "Owner").unwrap();
    let review = accepted(&service,&owner);
    let (generation,revision) = stamp(&service,&owner);
    for role in [Role::Reader,Role::Reviewer] {
        let member = service.invite(&owner.room,&owner.token,role,"Read only").unwrap();
        assert_eq!(row(&service,&member,&review)["can_reverse"],false);
        assert_eq!(service.reverse_review(&member.room,&member.token,&review,generation,&revision).unwrap_err().status,403);
    }
    let other = service.create("Other", "private", "Owner").unwrap();
    assert_eq!(service.reverse_review(&other.room,&other.token,&review,generation,&revision).unwrap_err().status,404);
    assert_eq!(service.reverse_review(&owner.room,&owner.token,&id(),generation,&revision).unwrap_err().status,404);
    assert_eq!(service.reverse_review(&owner.room,&owner.token,&review,generation,&document_transaction::revision("wrong")).unwrap_err().status,409);
    let doc = client(&service,&owner);
    let text = doc.get_or_insert_text("content");
    let vector = doc.transact().state_vector();
    text.insert(&mut doc.transact_mut(),5,"temporary");
    send(&service,&owner,&doc,&vector);
    let vector = doc.transact().state_vector();
    text.remove_range(&mut doc.transact_mut(),5,9);
    send(&service,&owner,&doc,&vector);
    drop(service);
    let service = Service::open(&path).unwrap();
    let before = service.snapshot(&owner.room,&owner.token).unwrap();
    assert_eq!(before["revision"],revision);
    assert_eq!(before["generation"],generation+2);
    for stale in [generation,generation+2] {
        assert_eq!(service.reverse_review(&owner.room,&owner.token,&review,stale,&revision).unwrap_err().status,409);
    }
    assert_eq!(row(&service,&owner,&review)["can_reverse"],false);
    assert!(row(&service,&owner,&review)["reversal"].is_null());
    assert_eq!(service.snapshot(&owner.room,&owner.token).unwrap(),before);
}

#[test]
fn reverse_uses_manual_unicode_inverse_and_existing_trusted_decision_after_migration() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("db");
    let service = Service::open(&path).unwrap();
    let source = "😀ab!";
    let owner = service.create("Manual",source,"Owner").unwrap();
    let review = service.review(&owner.room,&owner.token,json!({"text":"Suggestion","transaction":tx(source,2,4,"Suggested")})).unwrap();
    let manual = ContentTransaction { base_revision:document_transaction::revision(source), origin:"review".into(), edits:vec![TextEdit {from:2,to:3,insert:String::new()},TextEdit {from:3,to:4,insert:"β🦀".into()}] };
    service.resolve_with_transaction(&owner.room,&owner.token,&review,true,Some(manual),Some(0)).unwrap();
    assert_eq!(service.snapshot(&owner.room,&owner.token).unwrap()["content"],"😀β🦀!");
    let original = row(&service,&owner,&review)["body"].clone();
    let (generation,revision) = stamp(&service,&owner);
    drop(service);
    Connection::open(&path).unwrap().execute_batch("DROP TABLE review_reversals;").unwrap();
    let service = Service::open(&path).unwrap();
    service.reverse_review(&owner.room,&owner.token,&review,generation,&revision).unwrap();
    assert_eq!(service.snapshot(&owner.room,&owner.token).unwrap()["content"],source);
    assert_eq!(row(&service,&owner,&review)["body"],original);
}

#[test]
fn reversal_storage_failure_rolls_back_clock_crdt_and_receipt() {
    let data = tempfile::tempdir().unwrap();
    let service = Service::open(&data.path().join("db")).unwrap();
    let owner = service.create("Atomic","abc🦀","Owner").unwrap();
    let review = accepted(&service,&owner);
    let (generation,revision) = stamp(&service,&owner);
    let before = row(&service,&owner,&review);
    service.state.lock().unwrap().db.execute_batch("CREATE TRIGGER fail_reversal BEFORE INSERT ON review_reversals BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    assert_eq!(service.reverse_review(&owner.room,&owner.token,&review,generation,&revision).unwrap_err().status,503);
    assert_eq!(stamp(&service,&owner),(generation,revision.clone()));
    assert_eq!(service.snapshot(&owner.room,&owner.token).unwrap()["content"],"aBc🦀");
    assert_eq!(row(&service,&owner,&review),before);
    let state = service.state.lock().unwrap();
    assert_eq!(state.db.query_row("SELECT count(*) FROM updates",[],|r|r.get::<_,i64>(0)).unwrap(),1);
    assert_eq!(state.db.query_row("SELECT count(*) FROM review_reversals",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    state.db.execute_batch("DROP TRIGGER fail_reversal;").unwrap();
    drop(state);
    service.reverse_review(&owner.room,&owner.token,&review,generation,&revision).unwrap();
    assert_eq!(stamp(&service,&owner).0,generation+1);
}

#[test]
fn missing_unknown_or_old_decision_metadata_cannot_authorize_reversal() {
    let data = tempfile::tempdir().unwrap();
    let service = Service::open(&data.path().join("db")).unwrap();
    let owner = service.create("Legacy","abc🦀","Owner").unwrap();
    let review = accepted(&service,&owner);
    let (generation,revision) = stamp(&service,&owner);
    let original = row(&service,&owner,&review)["body"].clone();
    for (field,value) in [("_generation",Value::Null),("_decision",Value::Null)] {
        let mut old = original.clone();old[field]=value;
        service.state.lock().unwrap().db.execute("UPDATE reviews SET body=?2 WHERE id=?1",params![review,old.to_string()]).unwrap();
        assert_eq!(row(&service,&owner,&review)["can_reverse"],false);
        assert_eq!(service.reverse_review(&owner.room,&owner.token,&review,generation,&revision).unwrap_err().status,409);
    }
    for (field,value) in [("inverse",Value::Null),("method",json!("unknown")),("result_generation",json!(generation+1))] {
        let mut old = original.clone();old["_decision"][field]=value;
        service.state.lock().unwrap().db.execute("UPDATE reviews SET body=?2 WHERE id=?1",params![review,old.to_string()]).unwrap();
        assert_eq!(row(&service,&owner,&review)["can_reverse"],false);
        assert_eq!(service.reverse_review(&owner.room,&owner.token,&review,generation,&revision).unwrap_err().status,409);
    }
    let comment = service.review(&owner.room,&owner.token,json!({"text":"Comment"})).unwrap();
    service.resolve(&owner.room,&owner.token,&comment,true).unwrap();
    assert_eq!(row(&service,&owner,&comment)["can_reverse"],false);
    assert_eq!(service.reverse_review(&owner.room,&owner.token,&comment,generation,&revision).unwrap_err().status,409);
}
