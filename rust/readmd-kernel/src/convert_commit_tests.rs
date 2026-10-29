//! Offline expected-before publication tests; every file is synthetic.
use super::*;
use crate::document_operations::{self, Observation};
use serde_json::json;

fn fixture() -> (tempfile::TempDir,PathBuf,PathBuf) {
    let root=tempfile::tempdir().unwrap(); let data=root.path().join("data"); fs::create_dir(&data).unwrap();
    let output=root.path().join("output.md"); (root,data,output)
}

#[test]
fn new_output_is_noclobber_and_does_not_create_a_review_database() {
    let (_root,data,output)=fixture(); let plan=capture_md_output_plan(&data,&output,false).unwrap();
    assert!(!data.join("review.sqlite3").exists());
    fs::write(&output,b"foreign file").unwrap();
    assert_eq!(write_md_output_plan(&data,&plan,"converted").unwrap_err(),"document_operation_target_exists");
    assert_eq!(fs::read(&output).unwrap(),b"foreign file");
}

#[test]
fn planned_existing_output_moved_during_conversion_is_never_recreated() {
    let (root,data,output)=fixture(); fs::write(&output,b"original").unwrap();
    let plan=capture_md_output_plan(&data,&output,true).unwrap(); let retained=root.path().join("renamed.md"); fs::rename(&output,&retained).unwrap();
    assert_eq!(write_md_output_plan(&data,&plan,"converted").unwrap_err(),"document_operation_missing");
    assert!(!output.exists()); assert_eq!(fs::read(retained).unwrap(),b"original");
}

#[test]
fn equal_bytes_in_a_replacement_file_do_not_satisfy_the_original_identity() {
    let (root,data,output)=fixture(); fs::write(&output,b"original").unwrap(); let plan=capture_md_output_plan(&data,&output,true).unwrap();
    let retained=root.path().join("retained.md"); fs::rename(&output,&retained).unwrap(); fs::write(&output,b"original").unwrap();
    assert_eq!(write_md_output_plan(&data,&plan,"converted").unwrap_err(),"document_operation_identity_changed");
    assert_eq!(fs::read(&output).unwrap(),b"original"); assert_eq!(fs::read(retained).unwrap(),b"original");
}

#[test]
fn external_byte_edit_and_pending_rename_each_block_publication() {
    let (_root,data,output)=fixture(); fs::write(&output,b"original").unwrap(); let plan=capture_md_output_plan(&data,&output,true).unwrap();
    fs::write(&output,b"external edit").unwrap();
    assert_eq!(write_md_output_plan(&data,&plan,"converted").unwrap_err(),"document_operation_revision_changed"); assert_eq!(fs::read(&output).unwrap(),b"external edit");
    let plan=capture_md_output_plan(&data,&output,true).unwrap();
    let observed=match document_operations::observe(&output,document_operations::MAX_CAPTURE_BYTES).unwrap() {Observation::Regular(file)=>file,_=>panic!("synthetic regular file")};
    let target=output.with_file_name("renamed.md");
    crate::review::Service::new(&data).prepare_rename(&json!({"operation_id":"conversion-pending","source_path":output.to_str().unwrap(),"target_path":target.to_str().unwrap(),
        "source_identity":observed.fingerprint().identity.as_str(),"expected_disk_revision":observed.fingerprint().revision,"disk_revision":observed.fingerprint().revision,
        "disk_content":"external edit","encoding":"utf-8","line_ending":"lf"})).unwrap();
    assert_eq!(write_md_output_plan(&data,&plan,"converted").unwrap_err(),"rename_recovery_pending"); assert_eq!(fs::read(&output).unwrap(),b"external edit"); assert!(!target.exists());
}

#[test]
fn rejected_final_commit_preserves_bytes_and_removes_only_its_temporary_output() {
    let (root,data,output)=fixture(); let plan=capture_md_output_plan(&data,&output,false).unwrap();
    assert_eq!(write_md_output_plan_with_commit(&data,&plan,"converted",||false).unwrap_err(),"cancelled"); assert!(!output.exists());
    assert!(fs::read_dir(root.path()).unwrap().all(|entry|!entry.unwrap().file_name().to_string_lossy().starts_with(".readmd-convert-")));
}

#[test]
fn publish_time_noclobber_preserves_a_foreign_file_created_after_final_revalidation() {
    let (_root,data,output)=fixture(); let plan=capture_md_output_plan(&data,&output,false).unwrap();
    let error=write_md_output_plan_with_commit(&data,&plan,"converted",||{fs::write(&output,b"late foreign file").unwrap();true}).unwrap_err();
    assert_eq!(error,"document_operation_target_exists"); assert_eq!(fs::read(&output).unwrap(),b"late foreign file");
}

#[test]
fn successful_explicit_overwrite_keeps_a_recoverable_checkpoint() {
    let (_root,data,output)=fixture(); fs::write(&output,b"original").unwrap(); let plan=capture_md_output_plan(&data,&output,true).unwrap();
    write_md_output_plan(&data,&plan,"converted").unwrap(); assert_eq!(fs::read(&output).unwrap(),b"converted");
    let entries=crate::document_history::list(&data,None).unwrap(); assert!(entries.iter().any(|entry|entry.reason=="conversion_overwrite"));
}

#[test]
fn already_held_save_lock_uses_the_guarded_writer_without_relocking() {
    let (_root,data,output)=fixture();
    let _saving=crate::document_history::SAVE_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    write_md_managed_locked(&data,output.to_str().unwrap(),"converted",false).unwrap(); assert_eq!(fs::read(&output).unwrap(),b"converted");
}
