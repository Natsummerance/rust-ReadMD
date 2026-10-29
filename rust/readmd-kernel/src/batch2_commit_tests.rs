//! Commit boundaries only: no native parser, OCR engine, network or user data.
use super::*;
use std::{fs,sync::atomic::{AtomicBool,AtomicUsize,Ordering}};

fn fixture() -> (tempfile::TempDir,PathBuf,PathBuf) {
    let root=tempfile::tempdir().unwrap(); let data=root.path().join("data");fs::create_dir(&data).unwrap();
    let target=root.path().join("output.md");(root,data,target)
}

#[test]
fn renamed_planned_output_is_not_recreated_by_the_conversion_commit() {
    let (root,data,target)=fixture();fs::write(&target,b"original").unwrap();
    let plan=plan_converted_output(&data,target.clone(),OnExists::Overwrite,false,false);
    let renamed=root.path().join("renamed.md");fs::rename(&target,&renamed).unwrap();
    assert_eq!(save_converted(&data,"converted",&plan).unwrap_err(),"document_operation_missing");
    assert!(!target.exists());assert_eq!(fs::read(renamed).unwrap(),b"original");
}

#[test]
fn rename_mode_keeps_its_initial_chosen_name_and_refuses_a_later_collision() {
    let (_root,data,target)=fixture();fs::write(&target,b"original").unwrap();
    let plan=plan_converted_output(&data,target.clone(),OnExists::Rename,false,false);assert_ne!(plan.target,target);
    fs::write(&plan.target,b"foreign").unwrap();
    assert_eq!(save_converted(&data,"converted",&plan).unwrap_err(),"document_operation_target_exists");
    assert_eq!(fs::read(&plan.target).unwrap(),b"foreign");assert_eq!(fs::read(target).unwrap(),b"original");
}

#[test]
fn cancellation_after_materialization_rolls_resources_back_before_releasing_the_gate() {
    let (root,data,target)=fixture();let asset=crate::document_assets::stage(&data,b"synthetic pixels","plot","png").unwrap();
    let text=format!("![plot]({})",asset.reference);let plan=plan_converted_output(&data,target.clone(),OnExists::Skip,false,false);
    let calls=Arc::new(AtomicUsize::new(0));let observed=calls.clone();let callback_data=data.clone();let folder=root.path().join("readmd-assets");let callback_folder=folder.clone();
    let published=Arc::new(AtomicBool::new(false));let observed_published=published.clone();
    let check=Arc::new(move|| {
        observed.fetch_add(1,Ordering::SeqCst);
        // Parsing image references also checks cancellation. Callback counts
        // are not a phase boundary: arm only after the complete synthetic
        // resource is visible, before any document publication.
        let resource_ready=fs::read_dir(&callback_folder).ok().is_some_and(|entries|entries
            .filter_map(Result::ok).any(|entry|fs::read(entry.path()).is_ok_and(|bytes|bytes==b"synthetic pixels")));
        if !resource_ready {return false;}
        observed_published.store(true,Ordering::SeqCst);
        assert!(callback_folder.is_dir(),"final cancellation is after materialization");
        let error=crate::document_operations::acquire(&callback_data,std::time::Duration::ZERO).unwrap_err();
        assert_eq!(error.kind,crate::document_operations::ErrorKind::Busy);true
    });
    let result=crate::speech::with_cancel(check,||save_converted_assets(&data,&text,&plan,&[asset],true));
    assert_eq!(result.unwrap_err(),"cancelled");assert!(calls.load(Ordering::SeqCst)>=2);assert!(published.load(Ordering::SeqCst),"cancellation must reach a published resource");assert!(!target.exists());assert!(!folder.exists());
    crate::document_operations::acquire(&data,std::time::Duration::ZERO).unwrap();
}

#[test]
fn batch_commit_holding_jobs_mutex_does_not_reenter_its_cancellation_checker() {
    let (_root,data,target)=fixture();let plan=plan_converted_output(&data,target.clone(),OnExists::Skip,false,false);
    let _jobs=CONVERT_JOBS.lock().unwrap_or_else(|e|e.into_inner());
    let result=crate::speech::with_cancel(Arc::new(||panic!("batch commit must not reacquire jobs mutex")),
        ||save_converted_assets(&data,"converted",&plan,&[],false)).unwrap();
    assert!(result.1);assert_eq!(fs::read(target).unwrap(),b"converted");
}

#[test]
fn preview_and_skip_publish_no_resources_and_take_no_commit_gate() {
    let (root,data,target)=fixture();fs::write(&target,b"existing").unwrap();
    let asset=crate::document_assets::stage(&data,b"synthetic pixels","plot","png").unwrap();let text=format!("![plot]({})",asset.reference);
    let preview=plan_converted_output(&data,target.clone(),OnExists::Rename,false,true);assert_eq!(preview.target,target);
    let skip=plan_converted_output(&data,target.clone(),OnExists::Skip,false,false);
    let _gate=crate::document_operations::acquire(&data,std::time::Duration::ZERO).unwrap();
    assert!(!save_converted_assets(&data,&text,&preview,&[asset.clone()],true).unwrap().1);
    assert!(save_converted_assets(&data,&text,&skip,&[asset],true).unwrap().2);
    assert_eq!(fs::read(target).unwrap(),b"existing");assert!(!root.path().join("readmd-assets").exists());
}
