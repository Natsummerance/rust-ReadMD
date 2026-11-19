//! Opt-in destructive capacity test on a small, explicitly owned disk image.
//! No model inference; the genuine locked offline font pack is the consumer.
use super::*;
use std::{cell::RefCell, io::Write};

struct VolumeInfo {
    total: u64,
    caller_available: u64,
    actual_available: u64,
}
#[cfg(unix)]
fn owned_volume(root: &Path, pack: &Path) -> VolumeInfo {
    use std::os::{fd::AsRawFd, unix::fs::MetadataExt};
    let file = fs::File::open(root).unwrap();
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    assert_eq!(
        unsafe { libc::fstatvfs(file.as_raw_fd(), stat.as_mut_ptr()) },
        0
    );
    let stat = unsafe { stat.assume_init() };
    assert_ne!(
        fs::metadata(root).unwrap().dev(),
        fs::metadata(pack).unwrap().dev(),
        "refuse filling the input/host volume"
    );
    VolumeInfo {
        total: stat.f_blocks as u64 * stat.f_frsize as u64,
        caller_available: stat.f_bavail as u64 * stat.f_frsize as u64,
        actual_available: stat.f_bfree as u64 * stat.f_frsize as u64,
    }
}
#[cfg(windows)]
fn owned_volume(root: &Path, pack: &Path) -> VolumeInfo {
    use std::os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle};
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::HANDLE,
            Storage::FileSystem::{
                FileIdInfo, GetDiskSpaceInformationW, GetFileInformationByHandleEx,
                GetVolumePathNameW, DISK_SPACE_INFORMATION, FILE_ID_INFO,
            },
        },
    };
    let wide = root
        .canonicalize()
        .unwrap()
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut volume = vec![0u16; 32768];
    let mut space = DISK_SPACE_INFORMATION::default();
    unsafe {
        GetVolumePathNameW(PCWSTR(wide.as_ptr()), &mut volume).unwrap();
        GetDiskSpaceInformationW(PCWSTR(volume.as_ptr()), &mut space).unwrap();
    }
    let serial = |path: &Path| {
        let file = fs::OpenOptions::new()
            .access_mode(0x80)
            .share_mode(7)
            .custom_flags(0x0220_0000)
            .open(path)
            .unwrap();
        let mut id = FILE_ID_INFO::default();
        unsafe {
            GetFileInformationByHandleEx(
                HANDLE(file.as_raw_handle()),
                FileIdInfo,
                (&mut id as *mut FILE_ID_INFO).cast(),
                std::mem::size_of::<FILE_ID_INFO>() as u32,
            )
            .unwrap();
        }
        id.VolumeSerialNumber
    };
    assert_ne!(
        serial(root),
        serial(pack),
        "refuse filling the input/host volume"
    );
    let unit = u64::from(space.SectorsPerAllocationUnit) * u64::from(space.BytesPerSector);
    assert!(unit > 0);
    VolumeInfo {
        total: space.ActualTotalAllocationUnits.checked_mul(unit).unwrap(),
        caller_available: space
            .CallerAvailableAllocationUnits
            .checked_mul(unit)
            .unwrap(),
        actual_available: space
            .ActualAvailableAllocationUnits
            .checked_mul(unit)
            .unwrap(),
    }
}
fn native_full(error: &std::io::Error) -> bool {
    #[cfg(unix)]
    {
        error.raw_os_error() == Some(libc::ENOSPC)
    }
    #[cfg(windows)]
    {
        matches!(error.raw_os_error(), Some(39 | 112))
    }
}

struct Filler {
    path: PathBuf,
    bytes: u64,
}
impl Filler {
    fn full(root: &Path) -> Self {
        let path = root.join("readmd-owned-space-filler");
        let mut filler = Self { path, bytes: 0 };
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&filler.path)
            .unwrap();
        let block = vec![0x5a; 1024 * 1024];
        let mut chunk = block.len();
        loop {
            assert!(
                filler.bytes < 512 * 1024 * 1024,
                "owned small-volume bound exceeded"
            );
            match file.write(&block[..chunk]) {
                Ok(0) => panic!("zero-byte filler write"),
                Ok(n) => filler.bytes += n as u64,
                Err(error) => {
                    assert!(
                        native_full(&error),
                        "unexpected native filler error: {error}"
                    );
                    // A failed 1MiB allocation can leave enough room for a
                    // small document. Exhaust even the last allocatable block
                    // before claiming that the document consumer sees ENOSPC.
                    if chunk > 1 {
                        chunk = (chunk / 4).max(1);
                    } else {
                        break;
                    }
                }
            }
        }
        // HFS+ can report allocation failure on write before flushing. The
        // original write above must specifically be the native ENOSPC result.
        let _ = file.sync_all();
        filler
    }
}
impl Drop for Filler {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[test]
#[ignore = "requires an owned <=512MiB native disk image and the genuine offline font pack"]
fn native_small_volume_preflight_enospc_cancel_and_reopen() {
    let root = PathBuf::from(std::env::var_os("READMD_SPACE_OWNED_VOLUME").expect("owned volume"));
    let assets = PathBuf::from(std::env::var_os("READMD_SPACE_INSTALL_ASSETS").expect("assets"));
    let pack = PathBuf::from(std::env::var_os("READMD_SPACE_INSTALL_PACK").expect("offline pack"));
    let report = PathBuf::from(
        std::env::var_os("READMD_SPACE_REPORT_PATH").expect("report outside small volume"),
    );
    assert_eq!(
        fs::read(root.join(".readmd-owned-small-volume-v1")).unwrap(),
        b"ReadMD P0 capacity evidence only\n"
    );
    let volume = owned_volume(&root, &pack);
    assert!(
        volume.total <= 512 * 1024 * 1024,
        "refuse filling a large volume"
    );
    assert!(!report.starts_with(&root));
    let data = root.join("profile");
    let docs = root.join("documents");
    let boot = || {
        Arc::new(App::bootstrap(crate::paths::AppPaths::with_dirs(&data, &docs, &assets)).unwrap())
    };
    #[cfg(windows)]
    if std::env::var_os("READMD_SPACE_QUOTA_REOPEN_ONLY").is_some() {
        let app = boot();
        let original = docs.join("保留原文.md");
        let current = app
            .review
            .handle(&json!({"action":"lookup","path":original}))
            .unwrap()["snapshot"]
            .clone();
        assert_eq!(current["changes"][0]["decision"], "pending");
        assert_eq!(current["content"], "# 未保存审阅所需原文\n仍待用户决定。\n");
        let mutate = |state: &Value, action: &str| {
            app.review.handle(&json!({"action":action,"document_id":state["document_id"],"generation":state["generation"],"review_revision":state["review_revision"],"call_id":format!("quota-{action}")})).unwrap()
        };
        let undone = mutate(&current, "undo");
        assert_eq!(undone["content"], "# 未保存审阅所需原文\n");
        let redone = mutate(&undone, "redo");
        assert_eq!(redone["content"], current["content"]);
        let url = format!(
            "/api/file?p={}",
            percent_encoding::utf8_percent_encode(
                &original.to_string_lossy(),
                percent_encoding::NON_ALPHANUMERIC
            )
        );
        assert_eq!(
            crate::server::call_in_process(&app, "GET", &url, None).0,
            200
        );
        let (status, saved) = crate::server::call_in_process(
            &app,
            "POST",
            "/api/save",
            Some(
                &json!({"path":original,"content":redone["content"],"encoding":"utf-8","review_session":{"document_id":redone["document_id"],"generation":redone["generation"],"review_revision":redone["review_revision"],"call_id":"quota-document-save"}}),
            ),
        );
        assert_eq!(status, 200, "{saved}");
        assert_eq!(saved["review"]["changes"][0]["decision"], "pending");
        assert_eq!(
            fs::read_to_string(original).unwrap(),
            redone["content"].as_str().unwrap()
        );
        let mut value: Value = serde_json::from_slice(&fs::read(&report).unwrap()).unwrap();
        value["new_process_reopened"] = json!(true);
        value["undo_redo_and_document_save_after_quota_refusal"] = json!(true);
        fs::write(&report, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        return;
    }
    if std::env::var_os("READMD_SPACE_REOPEN_ONLY").is_some() {
        let app = boot();
        let saved = install_file(&app, &pack, "fonts", &InstallHooks::direct()).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&saved.body).unwrap()["unchanged"],
            true
        );
        let current = app
            .review
            .handle(&json!({"action":"lookup","path":docs.join("保留原文.md")}))
            .unwrap();
        assert_eq!(current["snapshot"]["changes"][0]["decision"], "pending");
        assert_eq!(
            current["snapshot"]["content"],
            "# 未保存审阅所需原文\n仍待用户决定。\n"
        );
        let mut value: Value = serde_json::from_slice(&fs::read(&report).unwrap()).unwrap();
        value["new_process_reopened"] = json!(true);
        value["pending_review_retained"] = json!(true);
        fs::write(&report, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        return;
    }
    fs::create_dir(&docs).unwrap();
    let app = boot();
    let original = docs.join("保留原文.md");
    fs::write(&original, "# 未保存审阅所需原文\n").unwrap();
    let original_bytes = fs::read(&original).unwrap();
    let first = app.review.handle(&json!({"action":"open","path":original,"content":"# 未保存审阅所需原文\n",
        "disk_content":"# 未保存审阅所需原文\n","disk_revision":hash(&original_bytes),"encoding":"utf-8","line_ending":"lf"})).unwrap();
    app.review.handle(&json!({"action":"propose","document_id":first["document_id"],"generation":first["generation"],
        "review_revision":first["review_revision"],"call_id":"owned-capacity-review","origin":"ai",
        "edits":[{"from":"# 未保存审阅所需原文\n".encode_utf16().count(),"to":"# 未保存审阅所需原文\n".encode_utf16().count(),"insert":"仍待用户决定。\n"}]})).unwrap();
    #[cfg(windows)]
    if std::env::var_os("READMD_SPACE_QUOTA_ONLY").is_some() {
        assert!(
            volume.actual_available > 128 * 1024 * 1024,
            "fixture requires physical free space"
        );
        assert!(
            volume.caller_available <= 64 * 1024 * 1024,
            "the fixture's hard quota is not effective for this actual caller"
        );
        let denied = install_file(&app, &pack, "fonts", &InstallHooks::direct())
            .err()
            .expect("real caller quota must reject before payload");
        assert_eq!(denied.status, 507);
        assert_eq!(fs::read(&original).unwrap(), original_bytes);
        let current = app
            .review
            .handle(&json!({"action":"lookup","path":original}))
            .unwrap();
        assert_eq!(current["snapshot"]["changes"][0]["decision"], "pending");
        fs::write(report,serde_json::to_vec_pretty(&json!({"passed":true,"real_ntfs_hard_quota":true,"genuine_locked_fonts":true,"physical_available_bytes":volume.actual_available,"caller_available_bytes":volume.caller_available,"preflight_denied_status":denied.status,"original_unchanged":true,"pending_review_retained":true,"new_process_reopened":false,"model_inference":false})).unwrap()).unwrap();
        return;
    }
    let decode = |response: Response| {
        assert_eq!(response.status, 200);
        serde_json::from_slice::<Value>(&response.body).unwrap()
    };
    let installed = decode(install_file(&app, &pack, "fonts", &InstallHooks::direct()).unwrap());
    assert_eq!(installed["installed"], true);
    assert!(
        installed["installation_space"]["available_bytes"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(installed["installation_space"]["volume_identity"]
        .as_str()
        .unwrap()
        .starts_with(&format!("{}:", std::env::consts::OS)));
    let selected = spec("fonts").unwrap();
    let expected = lock_for(&assets, selected).unwrap();
    let target = root_for(&data, selected).join(selected.version);
    let font = checked_files(&expected)
        .unwrap()
        .iter()
        .find_map(|f| f["path"].as_str().filter(|s| s.ends_with(".ttf")))
        .unwrap()
        .to_string();
    let font_before = fs::read(target.join(&font)).unwrap();

    // An external writer spends space after admission, before extraction.
    let held = RefCell::new(None);
    let phase = |phase: &str| {
        if phase == "extracting" {
            *held.borrow_mut() = Some(Filler::full(&root));
        }
    };
    let no_cancel = || false;
    let progress = |_: crate::capability_archive::Progress| {};
    let committing = || Ok(());
    let hooks = InstallHooks {
        cancelled: &no_cancel,
        phase: &phase,
        progress: &progress,
        committing: &committing,
    };
    let failure = install_file(&app, &pack, "fonts", &hooks)
        .err()
        .expect("ENOSPC must fail installation");
    assert!(held.borrow().as_ref().unwrap().bytes > 0);
    assert_eq!(fs::read(target.join(&font)).unwrap(), font_before);
    assert_eq!(fs::read(&original).unwrap(), original_bytes);
    drop(held.borrow_mut().take());
    drop(app);
    let app = boot();
    let recovered = decode(install_file(&app, &pack, "fonts", &InstallHooks::direct()).unwrap());
    assert_eq!(recovered["unchanged"], true);
    validate(&target, &expected).unwrap();
    probe("fonts", &target, &data).unwrap();

    // Full volume before admission must fail before creating a staging tree.
    // Capture the already durable head before exhausting space: opening a WAL
    // database itself may need disk metadata, and a refused read is not loss.
    let current = app
        .review
        .handle(&json!({"action":"lookup","path":original}))
        .unwrap()["snapshot"]
        .clone();
    let url = format!(
        "/api/file?p={}",
        percent_encoding::utf8_percent_encode(
            &original.to_string_lossy(),
            percent_encoding::NON_ALPHANUMERIC
        )
    );
    assert_eq!(
        crate::server::call_in_process(&app, "GET", &url, None).0,
        200
    );
    let filler = Filler::full(&root);
    let mut phases = Vec::<String>::new();
    let phases = RefCell::new(&mut phases);
    let phase = |value: &str| phases.borrow_mut().push(value.into());
    let hooks = InstallHooks {
        cancelled: &no_cancel,
        phase: &phase,
        progress: &progress,
        committing: &committing,
    };
    let denied = install_file(&app, &pack, "fonts", &hooks).err().unwrap();
    assert_eq!(denied.status, 507);
    assert!(!phases.borrow().iter().any(|p| p == "extracting"));
    assert_eq!(fs::read(target.join(&font)).unwrap(), font_before);
    let (save_status, save_failure) = crate::server::call_in_process(
        &app,
        "POST",
        "/api/save",
        Some(&json!({"path":original,
        "content":current["content"],"encoding":"utf-8","review_session":{"document_id":current["document_id"],
        "generation":current["generation"],"review_revision":current["review_revision"],"call_id":"owned-enospc-save"}})),
    );
    assert_ne!(
        save_status, 200,
        "a full-volume save must not claim success: {save_failure}"
    );
    assert_eq!(fs::read(&original).unwrap(), original_bytes);
    drop(filler);

    let cancel_commit = || Err(ApiError::new(422, "cancelled"));
    let hooks = InstallHooks {
        cancelled: &no_cancel,
        phase: &ignore_phase,
        progress: &progress,
        committing: &cancel_commit,
    };
    let cancelled = install_file(&app, &pack, "fonts", &hooks).err().unwrap();
    assert_eq!(cancelled.code, "cancelled");
    assert_eq!(fs::read(target.join(&font)).unwrap(), font_before);
    drop(app);
    let app = boot();
    let final_install =
        decode(install_file(&app, &pack, "fonts", &InstallHooks::direct()).unwrap());
    assert_eq!(final_install["unchanged"], true);
    assert_eq!(fs::read(&original).unwrap(), original_bytes);
    fs::write(report, serde_json::to_vec_pretty(&json!({"passed":true,"genuine_locked_fonts":true,
        "capacity":installed["installation_space"],"external_writer_after_admission":true,"native_enospc":true,
        "first_failure_code":failure.code,"old_pack_unchanged":true,"document_unchanged":true,
        "preflight_denied_status":denied.status,"no_stage_on_preflight_denial":true,
        "cancelled_before_commit":true,"document_save_enospc_retained_draft":true,
        "service_reopened":true,"new_process_reopened":false,"model_inference":false,
        "real_power_loss_certified":false})).unwrap()).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires a dedicated CI tmpfs and permission to bind mount only its owned fixture"]
fn native_small_volume_bind_mount_and_inode_exhaustion() {
    use std::os::{fd::AsRawFd, unix::fs::MetadataExt};
    let root = PathBuf::from(std::env::var_os("READMD_SPACE_OWNED_VOLUME").expect("owned volume"));
    let assets = PathBuf::from(std::env::var_os("READMD_SPACE_INSTALL_ASSETS").expect("assets"));
    let pack = PathBuf::from(std::env::var_os("READMD_SPACE_INSTALL_PACK").expect("genuine pack"));
    let report =
        PathBuf::from(std::env::var_os("READMD_SPACE_EXTRA_REPORT_PATH").expect("outside report"));
    assert_eq!(
        fs::read(root.join(".readmd-owned-small-volume-v1")).unwrap(),
        b"ReadMD P0 capacity evidence only\n"
    );
    assert!(owned_volume(&root, &pack).total <= 512 * 1024 * 1024);
    assert!(!report.starts_with(&root));
    let selected = spec("fonts").unwrap();
    let lock = lock_for(&assets, selected).unwrap();
    let target = root_for(&root.join("profile"), selected).join(selected.version);
    let font = checked_files(&lock)
        .unwrap()
        .iter()
        .find_map(|f| f["path"].as_str().filter(|s| s.ends_with(".ttf")))
        .unwrap();
    let original_font = hash(&fs::read(target.join(font)).unwrap());
    let original_doc = fs::read(root.join("documents/保留原文.md")).unwrap();
    let folder = root.join("mount-anchor");
    fs::create_dir(&folder).unwrap();
    let before = fs::metadata(&folder).unwrap();
    let plan = space::check(&folder, &lock).unwrap();
    struct Bound(PathBuf);
    impl Drop for Bound {
        fn drop(&mut self) {
            assert!(std::process::Command::new("/usr/bin/sudo")
                .args(["-n", "--", "/usr/bin/umount"])
                .arg(&self.0)
                .status()
                .unwrap()
                .success());
        }
    }
    assert!(std::process::Command::new("/usr/bin/sudo")
        .args(["-n", "--", "/usr/bin/mount", "--bind"])
        .arg(&folder)
        .arg(&folder)
        .status()
        .unwrap()
        .success());
    let bound = Bound(folder.clone());
    let after = fs::metadata(&folder).unwrap();
    assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
    assert!(
        space::verify(&plan).is_err(),
        "same inode does not make a changed mount identity safe"
    );
    drop(bound);
    drop(plan);
    fs::remove_dir(&folder).unwrap();
    let plan = space::check(&root, &lock).unwrap().unwrap();
    struct Inodes {
        root: PathBuf,
        files: Vec<PathBuf>,
    }
    impl Drop for Inodes {
        fn drop(&mut self) {
            for file in &self.files {
                let _ = fs::remove_file(file);
            }
            let _ = fs::remove_dir(&self.root);
        }
    }
    let fill = root.join("inode-fixture");
    fs::create_dir(&fill).unwrap();
    let mut filler = Inodes {
        root: fill,
        files: Vec::new(),
    };
    loop {
        assert!(
            filler.files.len() < 8192,
            "refuse consuming an unbounded inode domain"
        );
        let path = filler.root.join(format!("owned-{}", filler.files.len()));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => {
                drop(file);
                filler.files.push(path);
            }
            Err(error) => {
                assert_eq!(error.raw_os_error(), Some(libc::ENOSPC));
                break;
            }
        }
    }
    let file = fs::File::open(&root).unwrap();
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    assert_eq!(
        unsafe { libc::fstatvfs(file.as_raw_fd(), stat.as_mut_ptr()) },
        0
    );
    let stat = unsafe { stat.assume_init() };
    assert_eq!(stat.f_favail, 0);
    assert!(stat.f_bavail as u64 * stat.f_frsize as u64 > plan.additional_bytes);
    let refused = space::check(&root, &lock).unwrap_err();
    assert_eq!(refused.status, 507);
    let count = filler.files.len();
    drop(filler);
    space::check(&root, &lock).unwrap().unwrap();
    assert_eq!(hash(&fs::read(target.join(font)).unwrap()), original_font);
    assert_eq!(
        fs::read(root.join("documents/保留原文.md")).unwrap(),
        original_doc
    );
    fs::write(report,serde_json::to_vec_pretty(&json!({"passed":true,"bind_mount_same_device_inode_changed_identity":true,"inode_enospc":true,"byte_headroom_sufficient":true,"temporary_inode_files_removed":count,"old_pack_and_document_unchanged":true,"model_inference":false})).unwrap()).unwrap();
}
