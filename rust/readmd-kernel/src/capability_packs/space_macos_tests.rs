//! Opt-in destructive capacity test on a small, explicitly owned disk image.
//! No model inference; the genuine locked offline font pack is the consumer.
use super::*;
use std::{cell::RefCell, io::Write, os::{fd::AsRawFd, unix::fs::MetadataExt}};

struct Filler { path: PathBuf, bytes: u64 }
impl Filler {
    fn full(root: &Path) -> Self {
        let path = root.join("readmd-owned-space-filler");
        let mut filler = Self { path, bytes: 0 };
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&filler.path).unwrap();
        let block = vec![0x5a; 1024 * 1024];
        loop {
            assert!(filler.bytes < 512 * 1024 * 1024, "owned small-volume bound exceeded");
            match file.write(&block) {
                Ok(0) => panic!("zero-byte filler write"),
                Ok(n) => filler.bytes += n as u64,
                Err(error) => { assert_eq!(error.raw_os_error(), Some(libc::ENOSPC)); break; }
            }
        }
        // HFS+ can report allocation failure on write before flushing. The
        // original write above must specifically be the native ENOSPC result.
        let _ = file.sync_all();
        filler
    }
}
impl Drop for Filler { fn drop(&mut self) { let _ = fs::remove_file(&self.path); } }

#[test]
#[ignore = "requires an owned <=512MiB native disk image and the genuine offline font pack"]
fn native_small_volume_preflight_enospc_cancel_and_reopen() {
    let root = PathBuf::from(std::env::var_os("READMD_SPACE_OWNED_VOLUME").expect("owned volume"));
    let assets = PathBuf::from(std::env::var_os("READMD_SPACE_INSTALL_ASSETS").expect("assets"));
    let pack = PathBuf::from(std::env::var_os("READMD_SPACE_INSTALL_PACK").expect("offline pack"));
    let report = PathBuf::from(std::env::var_os("READMD_SPACE_REPORT_PATH").expect("report outside small volume"));
    assert_eq!(fs::read(root.join(".readmd-owned-small-volume-v1")).unwrap(), b"ReadMD P0 capacity evidence only\n");
    let volume = fs::File::open(&root).unwrap();
    let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
    assert_eq!(unsafe { libc::fstatfs(volume.as_raw_fd(), stat.as_mut_ptr()) }, 0);
    let stat = unsafe { stat.assume_init() };
    assert!(stat.f_blocks * u64::from(stat.f_bsize) <= 512 * 1024 * 1024);
    assert_ne!(fs::metadata(&root).unwrap().dev(), fs::metadata(&pack).unwrap().dev(), "refuse filling host volume");
    assert!(!report.starts_with(&root));
    let data = root.join("profile"); let docs = root.join("documents");
    let boot = || Arc::new(App::bootstrap(crate::paths::AppPaths::with_dirs(&data, &docs, &assets)).unwrap());
    if std::env::var_os("READMD_SPACE_REOPEN_ONLY").is_some() {
        let app = boot();
        let saved = install_file(&app, &pack, "fonts", &InstallHooks::direct()).unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&saved.body).unwrap()["unchanged"], true);
        let current = app.review.handle(&json!({"action":"lookup","path":docs.join("保留原文.md")})).unwrap();
        assert_eq!(current["snapshot"]["changes"][0]["decision"], "pending");
        assert_eq!(current["snapshot"]["content"], "# 未保存审阅所需原文\n仍待用户决定。\n");
        let mut value: Value = serde_json::from_slice(&fs::read(&report).unwrap()).unwrap();
        value["new_process_reopened"] = json!(true);
        value["pending_review_retained"] = json!(true);
        fs::write(&report, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        return;
    }
    fs::create_dir(&docs).unwrap();
    let app = boot();
    let original = docs.join("保留原文.md"); fs::write(&original, "# 未保存审阅所需原文\n").unwrap();
    let original_bytes = fs::read(&original).unwrap();
    let first = app.review.handle(&json!({"action":"open","path":original,"content":"# 未保存审阅所需原文\n",
        "disk_content":"# 未保存审阅所需原文\n","disk_revision":hash(&original_bytes),"encoding":"utf-8","line_ending":"lf"})).unwrap();
    app.review.handle(&json!({"action":"propose","document_id":first["document_id"],"generation":first["generation"],
        "review_revision":first["review_revision"],"call_id":"owned-capacity-review","origin":"ai",
        "edits":[{"from":"# 未保存审阅所需原文\n".encode_utf16().count(),"to":"# 未保存审阅所需原文\n".encode_utf16().count(),"insert":"仍待用户决定。\n"}]})).unwrap();
    let decode = |response: Response| { assert_eq!(response.status, 200); serde_json::from_slice::<Value>(&response.body).unwrap() };
    let installed = decode(install_file(&app, &pack, "fonts", &InstallHooks::direct()).unwrap());
    assert_eq!(installed["installed"], true);
    assert!(installed["installation_space"]["available_bytes"].as_u64().unwrap() > 0);
    assert!(installed["installation_space"]["volume_identity"].as_str().unwrap().starts_with("macos:"));
    let selected = spec("fonts").unwrap(); let expected = lock_for(&assets, selected).unwrap();
    let target = root_for(&data, selected).join(selected.version);
    let font = checked_files(&expected).unwrap().iter().find_map(|f| f["path"].as_str().filter(|s| s.ends_with(".ttf"))).unwrap().to_string();
    let font_before = fs::read(target.join(&font)).unwrap();

    // An external writer spends space after admission, before extraction.
    let held = RefCell::new(None);
    let phase = |phase: &str| if phase == "extracting" { *held.borrow_mut() = Some(Filler::full(&root)); };
    let no_cancel = || false; let progress = |_: crate::capability_archive::Progress| {}; let committing = || Ok(());
    let hooks = InstallHooks { cancelled: &no_cancel, phase: &phase, progress: &progress, committing: &committing };
    let failure = install_file(&app, &pack, "fonts", &hooks).err().expect("ENOSPC must fail installation");
    assert!(held.borrow().as_ref().unwrap().bytes > 0);
    assert_eq!(fs::read(target.join(&font)).unwrap(), font_before);
    assert_eq!(fs::read(&original).unwrap(), original_bytes);
    drop(held.borrow_mut().take());
    drop(app);
    let app = boot();
    let recovered = decode(install_file(&app, &pack, "fonts", &InstallHooks::direct()).unwrap());
    assert_eq!(recovered["unchanged"], true);
    validate(&target, &expected).unwrap(); probe("fonts", &target, &data).unwrap();

    // Full volume before admission must fail before creating a staging tree.
    let filler = Filler::full(&root);
    let mut phases = Vec::<String>::new();
    let phases = RefCell::new(&mut phases);
    let phase = |value: &str| phases.borrow_mut().push(value.into());
    let hooks = InstallHooks { cancelled: &no_cancel, phase: &phase, progress: &progress, committing: &committing };
    let denied = install_file(&app, &pack, "fonts", &hooks).err().unwrap();
    assert_eq!(denied.status, 507);
    assert!(!phases.borrow().iter().any(|p| p == "extracting"));
    assert_eq!(fs::read(target.join(&font)).unwrap(), font_before);
    drop(filler);

    let cancel_commit = || Err(ApiError::new(422, "cancelled"));
    let hooks = InstallHooks { cancelled: &no_cancel, phase: &ignore_phase, progress: &progress, committing: &cancel_commit };
    let cancelled = install_file(&app, &pack, "fonts", &hooks).err().unwrap();
    assert_eq!(cancelled.code, "cancelled");
    assert_eq!(fs::read(target.join(&font)).unwrap(), font_before);
    drop(app);
    let app = boot();
    let final_install = decode(install_file(&app, &pack, "fonts", &InstallHooks::direct()).unwrap());
    assert_eq!(final_install["unchanged"], true);
    assert_eq!(fs::read(&original).unwrap(), original_bytes);
    fs::write(report, serde_json::to_vec_pretty(&json!({"passed":true,"genuine_locked_fonts":true,
        "capacity":installed["installation_space"],"external_writer_after_admission":true,"native_enospc":true,
        "first_failure_code":failure.code,"old_pack_unchanged":true,"document_unchanged":true,
        "preflight_denied_status":denied.status,"no_stage_on_preflight_denial":true,
        "cancelled_before_commit":true,"service_reopened":true,"new_process_reopened":false,"model_inference":false,
        "real_power_loss_certified":false})).unwrap()).unwrap();
}
