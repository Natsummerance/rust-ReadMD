//! Deterministic commit-boundary tests using only owned synthetic files.
use super::*;
use crate::document_operations::{self, ErrorKind};
use std::{cell::{Cell, RefCell}, rc::Rc, time::Duration};

thread_local! {
    static BEFORE_GATE: RefCell<Option<Box<dyn FnOnce()>>> = const { RefCell::new(None) };
    static BEFORE_PUBLISH: RefCell<Option<Box<dyn FnOnce()>>> = const { RefCell::new(None) };
}

pub(super) fn commit_barrier(before_publish: bool) {
    let callback = if before_publish {
        BEFORE_PUBLISH.with(|slot| slot.borrow_mut().take())
    } else {
        BEFORE_GATE.with(|slot| slot.borrow_mut().take())
    };
    if let Some(callback) = callback { callback(); }
}

struct ResetHooks;
impl Drop for ResetHooks {
    fn drop(&mut self) {
        BEFORE_GATE.with(|slot| { slot.borrow_mut().take(); });
        BEFORE_PUBLISH.with(|slot| { slot.borrow_mut().take(); });
    }
}
fn hook(before_publish: bool, callback: impl FnOnce() + 'static) -> ResetHooks {
    let callback: Box<dyn FnOnce()> = Box::new(callback);
    if before_publish {
        BEFORE_PUBLISH.with(|slot| *slot.borrow_mut() = Some(callback));
    } else {
        BEFORE_GATE.with(|slot| *slot.borrow_mut() = Some(callback));
    }
    ResetHooks
}
fn fixture() -> (tempfile::TempDir, App, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let assets = root.path().join("assets");
    fs::create_dir(&assets).unwrap();
    let app = App::bootstrap(crate::paths::AppPaths::with_dirs(
        &root.path().join("profile"), root.path(), &assets,
    )).unwrap();
    let file = root.path().join("synthetic.md");
    fs::write(&file, b"Original\n").unwrap();
    (root, app, file)
}
fn edit(app: &App, path: &Path, replacement: &str) -> Result<Value, String> {
    edit_document(app, path, &revision(b"Original\n"), "Original", replacement,
        false, false, true, None)
}
fn reserve(app: &App, source: &Path, target: &Path) {
    let source = absolute_file(source).unwrap();
    let bytes = fs::read(&source).unwrap();
    let identity = edit_source_identity(&source).unwrap();
    let revision = revision(&bytes);
    app.review.prepare_rename(&json!({
        "operation_id":"synthetic-edit-reservation", "source_path":source,
        "target_path":target, "source_identity":identity.as_str(),
        "expected_disk_revision":revision, "disk_revision":revision,
        "disk_content":"Original\n", "encoding":"utf-8", "line_ending":"lf"
    })).unwrap();
}

#[test]
fn edit_commit_rejects_moved_or_same_byte_replaced_sources_at_both_boundaries() {
    for before_publish in [false, true] {
        for replace in [false, true] {
            let (root, app, file) = fixture();
            let retained = root.path().join("retained-original.md");
            let source = file.clone();
            let preserved = retained.clone();
            let fired = Rc::new(Cell::new(false));
            let callback_fired = fired.clone();
            let _hook = hook(before_publish, move || {
                fs::rename(&source, &preserved).unwrap();
                if replace { fs::write(&source, b"Original\n").unwrap(); }
                callback_fired.set(true);
            });
            assert_eq!(edit(&app, &file, "Edited").unwrap_err(), "document_conflict");
            assert!(fired.get());
            assert_eq!(fs::read(&retained).unwrap(), b"Original\n");
            if replace {
                assert_eq!(fs::read(&file).unwrap(), b"Original\n");
                assert_ne!(edit_source_identity(&file).unwrap(), edit_source_identity(&retained).unwrap());
            } else {
                assert!(!file.exists(), "A moved source must not be recreated");
            }
            document_operations::acquire(&app.paths.data_dir, Duration::ZERO).unwrap();
        }
    }
}

#[test]
fn edit_commit_checks_source_and_target_rename_reservations_without_touching_assets() {
    for edit_target in [false, true] {
        let (root, app, source) = fixture();
        let target = root.path().join("reserved-target.md");
        reserve(&app, &source, &target);
        // An outside writer can populate an already reserved target. Neither
        // endpoint becomes editable until the persisted rename is resolved.
        fs::write(&target, b"Original\n").unwrap();
        let staged = crate::document_assets::stage(&app.paths.data_dir,
            b"retained staged image", "synthetic", "png").unwrap();
        let replacement = format!("![image]({})", staged.reference);
        let requested = if edit_target { &target } else { &source };
        assert_eq!(edit(&app, requested, &replacement).unwrap_err(), "rename_recovery_pending");
        assert_eq!(fs::read(&source).unwrap(), b"Original\n");
        assert_eq!(fs::read(&target).unwrap(), b"Original\n");
        assert_eq!(fs::read(&staged.path).unwrap(), b"retained staged image");
        assert!(!root.path().join("readmd-assets").exists());
        for path in [&source, &target] {
            assert!(crate::review_api::pending_rename(&app, path).unwrap().is_some());
        }
    }
}

#[test]
fn edit_commit_rechecks_raw_revision_and_rolls_back_only_its_own_resources() {
    let (root, app, file) = fixture();
    let folder = root.path().join("readmd-assets");
    fs::create_dir(&folder).unwrap();
    let foreign = folder.join("unrelated.png");
    fs::write(&foreign, b"FOREIGN IMAGE").unwrap();
    let staged = crate::document_assets::stage(&app.paths.data_dir,
        b"retained staged image", "synthetic", "png").unwrap();
    let replacement = format!("![image]({})", staged.reference);
    let source = file.clone();
    let _hook = hook(true, move || { fs::write(&source, b"EXTERNAL TEXT\n").unwrap(); });
    assert_eq!(edit(&app, &file, &replacement).unwrap_err(), "document_conflict");
    assert_eq!(fs::read(&file).unwrap(), b"EXTERNAL TEXT\n");
    assert_eq!(fs::read(&foreign).unwrap(), b"FOREIGN IMAGE");
    assert_eq!(fs::read(&staged.path).unwrap(), b"retained staged image");
    assert_eq!(fs::read_dir(&folder).unwrap().count(), 1);
    let entries = crate::document_history::list(&app.paths.data_dir, None).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(crate::document_history::read(&app.paths.data_dir, &entries[0].id).unwrap().1, "Original\n");
    document_operations::acquire(&app.paths.data_dir, Duration::ZERO).unwrap();
}

#[test]
fn edit_commit_gate_covers_resource_commit_but_not_edit_planning() {
    let (root, app, file) = fixture();
    let staged = crate::document_assets::stage(&app.paths.data_dir,
        b"retained staged image", "synthetic", "png").unwrap();
    let replacement = format!("![image]({})", staged.reference);
    let profile = app.paths.data_dir.clone();
    let _before = hook(false, move || {
        document_operations::acquire(&profile, Duration::ZERO).unwrap();
    });
    let profile = app.paths.data_dir.clone();
    let folder = root.path().join("readmd-assets");
    let _publish = hook(true, move || {
        assert_eq!(document_operations::acquire(&profile, Duration::ZERO).unwrap_err().kind, ErrorKind::Busy);
        assert_eq!(fs::read_dir(&folder).unwrap().count(), 1);
    });
    let response = edit(&app, &file, &replacement).unwrap();
    assert_eq!(response["changed"], true);
    let saved = response["saved_content"].as_str().unwrap();
    assert!(saved.contains("readmd-assets/"));
    assert_eq!(fs::read(&file).unwrap(), saved.as_bytes());
    let permanent = response["saved_assets"][0]["path"].as_str().unwrap();
    assert_eq!(fs::read(permanent).unwrap(), b"retained staged image");
    document_operations::acquire(&app.paths.data_dir, Duration::ZERO).unwrap();
}

#[test]
fn edit_commit_does_not_fall_back_when_profile_gate_is_busy_or_invalid() {
    for invalid in [false, true] {
        let (root, app, file) = fixture();
        let staged = crate::document_assets::stage(&app.paths.data_dir,
            b"retained staged image", "synthetic", "png").unwrap();
        let replacement = format!("![image]({})", staged.reference);
        let held = if invalid {
            fs::write(app.paths.data_dir.join(document_operations::LOCK_FILE_NAME), b"FOREIGN LOCK").unwrap();
            None
        } else {
            Some(document_operations::acquire(&app.paths.data_dir, Duration::ZERO).unwrap())
        };
        let preview = edit_document(&app, &file, &revision(b"Original\n"), "Original",
            &replacement, false, true, false, None).unwrap();
        assert_eq!(preview["changed"], true);
        assert_eq!(preview["dry_run"], true);
        assert_eq!(edit(&app, &file, "Original").unwrap()["changed"], false);
        assert_eq!(edit(&app, &file, &replacement).unwrap_err(),
            if invalid { "document_operation_lock" } else { "document_operation_busy" });
        assert_eq!(fs::read(&file).unwrap(), b"Original\n");
        assert_eq!(fs::read(&staged.path).unwrap(), b"retained staged image");
        assert!(!root.path().join("readmd-assets").exists());
        if invalid {
            assert_eq!(fs::read(app.paths.data_dir.join(document_operations::LOCK_FILE_NAME)).unwrap(), b"FOREIGN LOCK");
        }
        drop(held);
    }
}
