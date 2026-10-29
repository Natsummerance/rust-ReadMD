//! Synthetic, offline adapters and process-boundary tests. No user profile,
//! endpoint, credential or model request is read or used here.
use super::*;
use std::{fs, process::{Command, Stdio}, time::Instant};

fn runtime(root: &Path) -> Arc<App> {
    let assets=root.join("assets"); fs::create_dir_all(&assets).unwrap();
    let data=root.join("data"); fs::create_dir_all(&data).unwrap();
    Arc::new(App::bootstrap(crate::paths::AppPaths::with_dirs(&data,root,&assets)).unwrap())
}
fn authorize(app: &Arc<App>, file: &Path) {
    let (status,_) = crate::server::call_in_process(app,"GET",&format!("/api/file?p={}",
        percent_encoding::utf8_percent_encode(file.to_str().unwrap(),percent_encoding::NON_ALPHANUMERIC)),None);
    assert_eq!(status,200);
}
fn with_gate<T>(app: &Arc<App>, call: impl FnOnce(&Guard)->T) -> T {
    let _save=crate::document_history::SAVE_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let gate=document_operations::acquire(&app.paths.data_dir,Duration::from_secs(3)).unwrap(); call(&gate)
}
fn observation(file: &Path) -> ObservedFile {
    match document_operations::observe(file,RENAME_READ_LIMIT).unwrap() {
        Observation::Regular(file)=>file,
        other=>panic!("synthetic regular file could not be observed: {other:?}"),
    }
}
fn body(operation: &str, bytes: &[u8]) -> Value {
    let (text,_)=crate::text_encoding::detect_and_decode(bytes);
    json!({"operation_id":operation,"expected_disk_revision":digest(bytes),"expected_revision":digest(text.as_bytes())})
}

#[test]
fn real_adapter_preserves_encoded_bytes_identity_and_same_review_after_restart() {
    for encoding in ["utf-8-sig","utf-16-le","gbk"] {
        let root=tempfile::tempdir().unwrap(); let app=runtime(root.path());
        let source=root.path().join("source.md"); let target=root.path().join("renamed.md");
        let text="合成资料\r\n正文保持原样\r\n"; let bytes=crate::text_encoding::encode(text,encoding).unwrap(); fs::write(&source,&bytes).unwrap();
        authorize(&app,&source);
        let (status,saved)=crate::server::call_in_process(&app,"POST","/api/review",Some(&json!({"action":"open","path":source.to_str().unwrap()})));
        assert_eq!(status,200); let request=body(&format!("encoded-{encoding}"),&bytes);
        assert_ne!(request["expected_disk_revision"],request["expected_revision"],"raw and decoded CAS must remain distinct");
        with_gate(&app,|gate| {
            let original=observation(&source); let prepared=prepare_rename(&app,&request,&source,&target,&original,gate).unwrap();
            assert_eq!(prepared["review"]["document_id"],saved["document_id"]);
            gate.move_same_directory_noreplace(&original,target.file_name().unwrap()).unwrap();
        });
        assert_eq!(fs::read(&target).unwrap(),bytes); assert!(!source.exists()); drop(app);
        let restarted=runtime(root.path()); assert!(crate::server::review_authorized_path(target.to_str().unwrap()).is_none());
        let recovered=with_gate(&restarted,|gate|recover_rename(&restarted,request["operation_id"].as_str().unwrap(),gate).unwrap());
        assert_eq!(recovered["state"],"committed"); assert_eq!(recovered["review"]["document_id"],saved["document_id"]);
        assert_eq!(recovered["review"]["disk_revision"],digest(&bytes)); assert_eq!(recovered["review"]["content"],normalize(text));
        assert!(crate::server::review_authorized_path(target.to_str().unwrap()).is_none(),"recovery must not grant target authorization");
        authorize(&restarted,&target);
        let (status,resumed)=crate::server::call_in_process(&restarted,"POST","/api/review",Some(&json!({"action":"resume","path":target.to_str().unwrap()})));
        assert_eq!(status,200); assert_eq!(resumed["snapshot"]["document_id"],saved["document_id"]); assert_eq!(fs::read(&target).unwrap(),bytes);
    }
}

#[test]
fn adapter_rejects_stale_raw_or_decoded_cas_before_creating_an_intent() {
    let root=tempfile::tempdir().unwrap(); let app=runtime(root.path()); let source=root.path().join("source.md"); let target=root.path().join("target.md");
    let bytes=crate::text_encoding::encode("原文\r\n","utf-16-le").unwrap(); fs::write(&source,&bytes).unwrap(); authorize(&app,&source);
    for field in ["expected_disk_revision","expected_revision"] {
        let mut request=body(field,&bytes); request[field]=json!(digest(b"stale"));
        let failure=with_gate(&app,|gate|prepare_rename(&app,&request,&source,&target,&observation(&source),gate).unwrap_err());
        assert_eq!(failure.code,"rename_revision_conflict"); assert_eq!(failure.status,409);
        assert_eq!(fs::read(&source).unwrap(),bytes); assert!(!target.exists());
    }
    assert!(!app.paths.data_dir.join("review.sqlite3").exists());
}

#[test]
fn wrong_profile_gate_cannot_prepare_or_recover_a_rename() {
    let root=tempfile::tempdir().unwrap(); let other=tempfile::tempdir().unwrap(); let app=runtime(root.path()); let other_app=runtime(other.path());
    let source=root.path().join("source.md"); let target=root.path().join("target.md"); fs::write(&source,b"saved").unwrap(); authorize(&app,&source);
    let failure=with_gate(&other_app,|gate|prepare_rename(&app,&body("wrong-gate",b"saved"),&source,&target,&observation(&source),gate).unwrap_err());
    assert_eq!(failure.code,"document_operation_lock"); assert!(!app.paths.data_dir.join("review.sqlite3").exists());
    let failure=with_gate(&other_app,|gate|recover_rename(&app,"wrong-gate",gate).unwrap_err()); assert_eq!(failure.code,"document_operation_lock");
    assert_eq!(fs::read(&source).unwrap(),b"saved"); assert!(!target.exists());
}

#[test]
fn real_same_hash_foreign_target_cannot_complete_a_pending_rename() {
    let root=tempfile::tempdir().unwrap(); let app=runtime(root.path()); let source=root.path().join("source.md"); let target=root.path().join("target.md");
    fs::write(&source,b"same bytes").unwrap(); authorize(&app,&source);
    with_gate(&app,|gate| {
        let original=observation(&source); prepare_rename(&app,&body("foreign-same-hash",b"same bytes"),&source,&target,&original,gate).unwrap();
        fs::write(&target,b"same bytes").unwrap();
        assert_ne!(original.fingerprint().identity,observation(&target).fingerprint().identity);
        let cancelled=recover_rename(&app,"foreign-same-hash",gate).unwrap(); assert_eq!(cancelled["state"],"cancelled"); assert_eq!(cancelled["moved"],false);
    });
    assert_eq!(fs::read(&source).unwrap(),b"same bytes"); assert_eq!(fs::read(&target).unwrap(),b"same bytes");
    assert!(crate::server::review_authorized_path(target.to_str().unwrap()).is_none());
}

#[test]
fn external_removal_with_foreign_same_hash_stays_pending_without_authorization() {
    let root=tempfile::tempdir().unwrap(); let app=runtime(root.path()); let source=root.path().join("source.md"); let target=root.path().join("target.md");
    fs::write(&source,b"same bytes").unwrap(); authorize(&app,&source);
    with_gate(&app,|gate| {
        let original=observation(&source); prepare_rename(&app,&body("missing-source",b"same bytes"),&source,&target,&original,gate).unwrap();
        fs::write(&target,b"same bytes").unwrap(); fs::remove_file(&source).unwrap();
        let pending=recover_rename(&app,"missing-source",gate).unwrap(); assert_eq!(pending["state"],"recovery_pending"); assert!(pending["moved"].is_null());
    });
    assert_eq!(fs::read(&target).unwrap(),b"same bytes"); assert!(!source.exists());
    assert!(crate::server::review_authorized_path(target.to_str().unwrap()).is_none());
}

#[test]
fn review_open_queued_before_rename_cannot_create_a_stale_source_session() {
    let root=tempfile::tempdir().unwrap(); let app=runtime(root.path()); let source=root.path().join("source.md"); let target=root.path().join("target.md");
    fs::write(&source,b"saved").unwrap(); authorize(&app,&source);
    let saving=crate::document_history::SAVE_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let gate=document_operations::acquire(&app.paths.data_dir,Duration::from_secs(3)).unwrap();
    let ready=Arc::new(std::sync::Barrier::new(2)); let other_ready=ready.clone(); let other_app=app.clone(); let other_source=source.clone();
    let queued=std::thread::spawn(move|| {
        other_ready.wait();
        crate::server::call_in_process(&other_app,"POST","/api/review",Some(&json!({"action":"open","path":other_source.to_str().unwrap()})))
    });
    ready.wait();
    let original=observation(&source); let prepared=prepare_rename(&app,&body("queued-open",b"saved"),&source,&target,&original,&gate).unwrap();
    gate.move_same_directory_noreplace(&original,target.file_name().unwrap()).unwrap();
    assert_eq!(recover_rename(&app,"queued-open",&gate).unwrap()["state"],"committed"); drop(gate); drop(saving);
    let (status,_)=queued.join().unwrap(); assert!(matches!(status,403|409),"old-path open must recheck after acquiring the gate");
    let old=app.review.handle(&json!({"action":"lookup","path":source.to_str().unwrap()})).unwrap(); assert_eq!(old["found"],false);
    let new=app.review.handle(&json!({"action":"lookup","path":target.to_str().unwrap()})).unwrap(); assert_eq!(new["snapshot"]["document_id"],prepared["review"]["document_id"]);
    assert_eq!(fs::read(&target).unwrap(),b"saved"); assert!(!source.exists());
}

#[test]
fn pending_rename_is_visible_to_another_process_for_mutations_and_legacy_saves() {
    let root=tempfile::tempdir().unwrap(); let app=runtime(root.path()); let source=root.path().join("source.md"); let target=root.path().join("target.md");
    fs::write(&source,b"saved").unwrap(); authorize(&app,&source);
    with_gate(&app,|gate|prepare_rename(&app,&body("cross-process",b"saved"),&source,&target,&observation(&source),gate).unwrap());
    let mut child=Command::new(std::env::current_exe().unwrap())
        .args(["--exact","review_api::rename_integration_tests::pending_child_process","--nocapture","--test-threads=1"])
        .env("READMD_SYNTHETIC_RENAME_CHILD",root.path())
        .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let deadline=Instant::now()+Duration::from_secs(30);
    loop {
        if child.try_wait().unwrap().is_some() { break; }
        if Instant::now()>=deadline { let _=child.kill(); let _=child.wait(); panic!("synthetic rename child exceeded timeout"); }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output=child.wait_with_output().unwrap(); assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
    let output_text=String::from_utf8_lossy(&output.stdout); assert!(output_text.contains("1 passed"),"child entry must actually execute");
    assert_eq!(fs::read(&source).unwrap(),b"saved"); assert!(!target.exists());
    with_gate(&app,|gate|assert_eq!(recover_rename(&app,"cross-process",gate).unwrap()["state"],"cancelled"));
    assert!(SaveGuard::prepare(&app,&json!({"content":"saved"}),&source,"saved",&[]).unwrap().is_none(),"ordinary identical save is available again after cancellation");
}

#[test]
fn pending_child_process() {
    let Some(root)=std::env::var_os("READMD_SYNTHETIC_RENAME_CHILD") else { return; };
    let root=PathBuf::from(root); let app=runtime(&root); let source=root.join("source.md"); let target=root.join("target.md");
    let found=app.review.handle(&json!({"action":"lookup","path":source.to_str().unwrap()})).unwrap(); assert_eq!(found["found"],true);
    let state=&found["snapshot"]; assert_eq!(state["status"],"rename_pending");
    for action in ["edit","propose","resolve","undo","redo","discard","prepare_save","commit_save","abort_save"] {
        let request=json!({"action":action,"document_id":state["document_id"],"generation":state["generation"],
            "review_revision":state["review_revision"],"call_id":action});
        assert_eq!(app.review.handle(&request).unwrap_err(),"rename_recovery_pending");
    }
    for path in [&source,&target] {
        match SaveGuard::prepare(&app,&json!({"content":"saved"}),path,"saved",&[]) {
            Err(error)=>assert_eq!(error.code,"rename_recovery_pending"),
            Ok(_)=>panic!("legacy save must not bypass another process's rename intent"),
        }
    }
    assert_eq!(fs::read(&source).unwrap(),b"saved"); assert!(!target.exists());
}
