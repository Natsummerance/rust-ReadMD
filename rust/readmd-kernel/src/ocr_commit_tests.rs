//! OCR adapter tests inject synthetic recognition text; these do not claim
//! native recognition-engine or UI acceptance.
use super::*;
use std::{cell::Cell,collections::HashMap,fs};

fn fixture() -> (tempfile::TempDir,App,Request,PathBuf) {
    let root=tempfile::tempdir().unwrap();let assets=root.path().join("assets");let data=root.path().join("data");fs::create_dir(&assets).unwrap();fs::create_dir(&data).unwrap();
    let app=App::bootstrap(crate::paths::AppPaths::with_dirs(&data,root.path(),&assets)).unwrap();app.modules.lock().unwrap().set("ocr","ready");
    let input=root.path().join("scan.png");fs::write(&input,b"synthetic image placeholder").unwrap();let output=root.path().join("scan.md");
    let req=Request {method:"GET".into(),path:"/api/ocr".into(),query:HashMap::from([
        ("p".into(),input.to_str().unwrap().into()),("save".into(),"1".into()),("on_exists".into(),"overwrite".into())]),headers:HashMap::new(),body:Vec::new()};
    (root,app,req,output)
}
fn answered(response:&Response)->Value {serde_json::from_slice(&response.body).unwrap()}

#[test]
fn output_moved_while_recognition_runs_is_not_recreated() {
    let (root,app,req,output)=fixture();fs::write(&output,b"original").unwrap();let retained=root.path().join("retained.md");let called=Cell::new(false);
    let result=h_ocr_with_engine(&app,&req,||{called.set(true);true},|_|{fs::rename(&output,&retained).unwrap();Ok("Recognized synthetic text.".into())});
    assert_eq!(result.status,409);assert_eq!(answered(&result)["error_code"],"document_operation_missing");assert!(!called.get());
    assert!(!output.exists());assert_eq!(fs::read(retained).unwrap(),b"original");
}

#[test]
fn target_created_during_recognition_is_preserved_even_when_overwrite_was_requested() {
    let (_root,app,req,output)=fixture();let called=Cell::new(false);
    let result=h_ocr_with_engine(&app,&req,||{called.set(true);true},|_|{fs::write(&output,b"foreign").unwrap();Ok("Recognized synthetic text.".into())});
    assert_eq!(result.status,409);assert_eq!(answered(&result)["error_code"],"document_operation_target_exists");assert!(!called.get());assert_eq!(fs::read(output).unwrap(),b"foreign");
}

#[test]
fn atomic_task_cancellation_at_publish_boundary_keeps_the_output_missing() {
    let (_root,app,req,output)=fixture();let id=format!("ocr-commit-{}",uuid::Uuid::new_v4());let task=crate::cancel::register(&id).unwrap();
    let result=crate::speech::with_cancel(task.checker(),||h_ocr_with_engine(&app,&req,|| {
        assert_eq!(crate::cancel::cancel(&id),crate::cancel::CancelState::Cancelling);task.try_commit()
    },|_|Ok("Recognized synthetic text.".into())));
    assert_eq!(result.status,499);assert_eq!(answered(&result)["error_code"],"cancelled");assert!(!output.exists());
}

#[test]
fn read_only_empty_skipped_and_identical_results_never_enter_the_publish_hook() {
    for mode in ["read-only","empty","skip","identical"] {
        let (_root,app,mut req,output)=fixture();let text="Recognized synthetic text.";
        match mode {"read-only"=>{req.query.remove("save");},"skip"=>{fs::write(&output,b"existing").unwrap();req.query.insert("on_exists".into(),"skip".into());},
            "identical"=>fs::write(&output,readmd_fix::fix_markdown(text).text).unwrap(),_=>{}}
        let result=h_ocr_with_engine(&app,&req,||panic!("no publication was needed"),|_|Ok(if mode=="empty" {String::new()} else {text.into()}));
        assert_eq!(result.status,200);if matches!(mode,"read-only"|"empty"){assert!(!output.exists());}
    }
}

#[test]
fn successful_ocr_commit_invokes_its_hook_once_and_preserves_recognition_text() {
    let (_root,app,req,output)=fixture();let calls=Cell::new(0);let text="Recognized synthetic text.";
    let result=h_ocr_with_engine(&app,&req,||{calls.set(calls.get()+1);true},|_|Ok(text.into()));
    assert_eq!(result.status,200);assert_eq!(answered(&result)["saved"],true);assert_eq!(calls.get(),1);assert_eq!(fs::read_to_string(output).unwrap(),readmd_fix::fix_markdown(text).text);
}
