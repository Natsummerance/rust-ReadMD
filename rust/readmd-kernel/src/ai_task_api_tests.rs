use super::*;
use std::{fs,path::Path};
fn app(root:&Path)->Arc<App>{let assets=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets").canonicalize().unwrap();Arc::new(App::bootstrap(crate::paths::AppPaths::with_dirs(&root.join("data"),root,&assets)).unwrap())}
fn draft(app:&Arc<App>)->Value{app.review.handle(&json!({"action":"open","path":"draft:test-note","content":"Original prose.","disk_revision":"missing","disk_content":"","encoding":"utf-8","line_ending":"lf"})).unwrap()}
fn create(app:&Arc<App>,s:&Value)->TaskRecord{app.ai_tasks.create(s,TaskRequest{start_call_id:"synthetic-start".into(),document_id:s["document_id"].as_str().unwrap().into(),instruction:"Improve the prose".into(),connection:crate::ai_harness::ConnectionRef{provider_id:"synthetic-test".into(),credential_id:None,model:"synthetic-test".into()},allowed_ranges:None,budget:Budget::default(),template:None,stream:false,caller_digest:"0".repeat(64)}).unwrap()}
#[test]
fn empty_task_listing_does_not_create_task_or_review_database(){let dir=tempfile::tempdir().unwrap();let runtime=app(dir.path());let value=dispatch(&runtime,&json!({"action":"list"})).unwrap();assert_eq!(value["tasks"],json!([]));assert!(!runtime.paths.data_dir.join("ai-tasks.sqlite3").exists());assert!(!runtime.paths.data_dir.join("review.sqlite3").exists());}
#[test]
fn status_is_public_only_and_pause_survives_new_app_without_auto_run(){
    let dir=tempfile::tempdir().unwrap();let runtime=app(dir.path());let task=create(&runtime,&draft(&runtime));let _permit=runtime.ai_tasks.start(&task.id).unwrap();
    let status=dispatch(&runtime,&json!({"action":"status","task_id":task.id})).unwrap();let text=status["task"].to_string();assert!(!text.contains("synthetic-test"));assert!(!text.contains("instruction"));assert_eq!(status["task"]["state"],"running");
    let paused=dispatch(&runtime,&json!({"action":"pause","task_id":task.id})).unwrap();assert_eq!(paused["task"]["state"],"paused");drop(runtime);
    let reopened=app(dir.path());let listed=dispatch(&reopened,&json!({"action":"list"})).unwrap();assert_eq!(listed["tasks"][0]["state"],"paused");assert!(dispatch(&reopened,&json!({"action":"resume","task_id":task.id})).is_err());
    assert_eq!(reopened.ai_tasks.get(&task.id).unwrap().state,"paused");
}
#[test]
fn startup_turns_interrupted_running_job_into_explicit_resume_state(){
    let dir=tempfile::tempdir().unwrap();let runtime=app(dir.path());let task=create(&runtime,&draft(&runtime));let permit=runtime.ai_tasks.start(&task.id).unwrap();drop(permit);drop(runtime);
    let reopened=app(dir.path());let t=reopened.ai_tasks.get(&task.id).unwrap();assert_eq!(t.state,"needs_user_resume");assert_eq!(t.rounds,0);assert!(t.elapsed_ms<=180_000);
}
#[test]
fn start_rejects_credential_or_scope_override_before_any_provider_call(){
    let dir=tempfile::tempdir().unwrap();let runtime=app(dir.path());let s=draft(&runtime);
    for field in ["base_url","headers","api_key","system","workspace"]{
        let mut body=json!({"action":"start","doc_id":s["document_id"],"generation":s["generation"],"review_revision":s["review_revision"],"request":"Improve","call_id":"start"});body[field]=json!("forbidden");
        let error=dispatch(&runtime,&body).unwrap_err();assert_eq!(error.code,"review_not_authorized");
    }
    assert!(!runtime.paths.data_dir.join("ai-tasks.sqlite3").exists());assert_eq!(fs::read_dir(runtime.paths.data_dir.join("staging")).ok().map(|d|d.count()).unwrap_or(0),0);
}

#[test]
fn durable_start_retry_does_not_reresolve_changed_connection_or_document(){
    let dir=tempfile::tempdir().unwrap();let runtime=app(dir.path());let s=draft(&runtime);
    let body=json!({"action":"start","doc_id":s["document_id"],"generation":s["generation"],"review_revision":s["review_revision"],"request":"Improve the prose","call_id":"stable-start","budget":{"max_rounds":4,"max_elapsed_ms":60000}});
    let mut identity=body.clone();let object=identity.as_object_mut().unwrap();object.remove("action");object.remove("call_id");
    let task=runtime.ai_tasks.create(&s,TaskRequest{start_call_id:"stable-start".into(),document_id:s["document_id"].as_str().unwrap().into(),instruction:"Improve the prose".into(),connection:crate::ai_harness::ConnectionRef{provider_id:"synthetic-missing-connection".into(),credential_id:None,model:"synthetic-test".into()},allowed_ranges:None,budget:Budget{max_rounds:4,max_elapsed_ms:60000},template:None,stream:true,caller_digest:caller_digest(&identity).unwrap()}).unwrap();
    runtime.review.handle(&json!({"action":"edit","document_id":s["document_id"],"generation":s["generation"],"review_revision":s["review_revision"],"call_id":"later-user-edit","origin":"editor","edits":[{"from":0,"to":0,"insert":"Later user text. "}]})).unwrap();
    let repeated=dispatch(&runtime,&body).unwrap();assert_eq!(repeated["task"]["id"],task.id);assert_eq!(repeated["task"]["state"],"queued");assert_eq!(repeated["task"]["rounds"],0);
    let mut changed=body;changed["request"]=json!("Different instruction");assert_eq!(dispatch(&runtime,&changed).unwrap_err().code,"review_call_conflict");
}

#[test]
fn corrupt_optional_task_store_does_not_disable_basic_reading(){
    let dir=tempfile::tempdir().unwrap();fs::create_dir_all(dir.path().join("data")).unwrap();let db=dir.path().join("data/ai-tasks.sqlite3");fs::write(&db,b"invalid optional task store").unwrap();
    let runtime=app(dir.path());let file=dir.path().join("Readable.md");fs::write(&file,"# Still readable\n").unwrap();let route=format!("/api/file?p={}",percent_encoding::utf8_percent_encode(&file.to_string_lossy(),percent_encoding::NON_ALPHANUMERIC));
    assert_eq!(crate::server::call_in_process(&runtime,"GET",&route,None).0,200);assert_eq!(dispatch(&runtime,&json!({"action":"list"})).unwrap_err().code,"review_persistence_failed");assert_eq!(fs::read(&db).unwrap(),b"invalid optional task store");
}

#[test]
fn saved_task_connection_uses_stable_id_instead_of_display_name(){
    let dir=tempfile::tempdir().unwrap();let runtime=app(dir.path());
    let write=|name:&str|fs::write(runtime.paths.data_dir.join("ai.json"),json!({"schema_version":crate::ai::CONFIG_SCHEMA_VERSION,"providers":[{"id":"local-stable-test","name":name,"base_url":"http://127.0.0.1:1/v1","category":"local","format":"openai","mode":"chat","models":["synthetic-model"]}],"current":{"provider_id":null,"provider":name,"model":"synthetic-model"}}).to_string()).unwrap();
    write("Original synthetic name");let body=json!({"request":"Improve","provider":"Original synthetic name"});let route=crate::server::review_task_route(&runtime,&body).unwrap();let saved=crate::server::review_task_connection(&runtime,&body,&route).unwrap();assert_eq!(saved.provider_id,"local-stable-test");
    write("Renamed synthetic name");let body=json!({"request":"Improve","provider":saved.provider_id,"model":saved.model});assert!(crate::server::review_task_route(&runtime,&body).is_ok());
    let route=crate::server::review_task_route(&runtime,&json!({"request":"Improve"})).unwrap();assert_eq!(crate::server::review_task_connection(&runtime,&json!({"request":"Improve"}),&route).unwrap().provider_id,"local-stable-test");
}
