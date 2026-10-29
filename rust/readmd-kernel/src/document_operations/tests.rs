//! Real synthetic filesystem/process tests. No cleanup or user documents.
//! Set READMD_DOCUMENT_OPERATIONS_TEST_ROOT to a dedicated evidence directory;
//! otherwise each run retains new cases under the current workspace's .cache.
use super::*;
use std::{
    io::Write,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
thread_local! { static BEFORE_MOVE: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
static AFTER_MOVE: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) }; }
pub(super) fn move_barrier(after: bool) {
    let callback = if after {
        AFTER_MOVE.with(|hook| hook.borrow_mut().take())
    } else {
        BEFORE_MOVE.with(|hook| hook.borrow_mut().take())
    };
    if let Some(callback) = callback {
        callback();
    }
}
fn root() -> PathBuf {
    let base = std::env::var_os("READMD_DOCUMENT_OPERATIONS_TEST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap()
                .join(".cache")
                .join("v006-document-operations-native")
        });
    assert!(base.is_absolute(), "Test evidence root must be absolute");
    fs::create_dir_all(&base).unwrap();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = base.join(format!(
        "case-{}-{stamp}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&path).unwrap();
    path
}
fn write_new(path: &Path, bytes: &[u8]) {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    file.write_all(bytes).unwrap();
    file.sync_all().unwrap();
}
fn regular(path: &Path) -> ObservedFile {
    match observe(path, MAX_CAPTURE_BYTES).unwrap() {
        Observation::Regular(file) => file,
        other => panic!("Expected regular synthetic file: {other:?}"),
    }
}
fn profile(root: &Path) -> PathBuf {
    let path = root.join("profile");
    fs::create_dir(&path).unwrap();
    path
}
fn guard(path: &Path) -> Guard {
    acquire(path, Duration::ZERO).unwrap()
}

#[test]
fn identity_round_trip_is_strict_and_has_no_path_or_authority() {
    let id = FileIdentity::native("windows", 12, 34, 56);
    assert_eq!(FileIdentity::parse(id.as_str()).unwrap(), id);
    for bad in ["", "readmd-file-v2:windows:0000000000000000:00000000000000000000000000000000:00000000000000000000000000000000", "readmd-file-v1:unknown:0000000000000000:00000000000000000000000000000000:00000000000000000000000000000000", "readmd-file-v1:windows:c:22:38"] {
        assert_eq!(FileIdentity::parse(bad).unwrap_err().kind, ErrorKind::Unsupported);
    }
    assert!(FileIdentity::parse(&id.as_str().replace("000c", "000C")).is_err());
    for bad in [
        "readmd-file-v1:windows:0000000000000001:00000000000000000000000000000000:00000000000000000000000000000001",
        "readmd-file-v1:windows:0000000000000001:00000000000000000000000000000001:00000000000000000000000000000000",
    ] { assert_eq!(FileIdentity::parse(bad).unwrap_err().kind, ErrorKind::Unsupported); }
}

#[test]
fn raw_bytes_and_identity_are_captured_without_decoding_or_writing() {
    let root = root();
    let path = root.join("original.md");
    let bytes = b"\xef\xbb\xbf# Synthetic\r\n\x00\xff";
    write_new(&path, bytes);
    let observed = regular(&path);
    assert_eq!(observed.bytes(), bytes);
    assert_eq!(
        observed.fingerprint().revision,
        format!("{:x}", Sha256::digest(bytes))
    );
    assert_eq!(regular(&path).fingerprint(), observed.fingerprint());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert!(!format!("{observed:?}").contains(root.to_str().unwrap()));
}

#[test]
fn only_an_absent_leaf_under_a_valid_existing_parent_is_missing() {
    let root = root();
    assert!(matches!(
        observe(&root.join("absent.md"), 100).unwrap(),
        Observation::Missing
    ));
    assert!(!matches!(
        observe(&root.join("absent-parent/leaf.md"), 100),
        Ok(Observation::Missing)
    ));
    assert_eq!(
        observe(Path::new("relative.md"), 100).unwrap_err().kind,
        ErrorKind::InvalidPath
    );
    assert!(matches!(
        observe(&root, 100).unwrap(),
        Observation::Unsupported(UnsupportedReason::NotRegularFile)
    ));
}

#[test]
fn capture_budget_is_bounded_and_does_not_change_the_file() {
    let root = root();
    let path = root.join("budget.md");
    write_new(&path, b"four");
    assert!(matches!(
        observe(&path, 3).unwrap(),
        Observation::Unsupported(UnsupportedReason::TooLarge)
    ));
    assert_eq!(regular(&path).bytes(), b"four");
    let empty = root.join("empty.md");
    write_new(&empty, b"");
    assert!(matches!(
        observe(&empty, 0).unwrap(),
        Observation::Regular(_)
    ));
}

#[test]
fn fixed_empty_profile_gate_is_exclusive_bounded_and_never_deleted() {
    let root = root();
    let data = profile(&root);
    let first = guard(&data);
    let started = Instant::now();
    let error = acquire(&data, Duration::from_millis(70)).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Busy);
    assert_eq!(error.code(), "document_operation_busy");
    assert!(started.elapsed() < Duration::from_secs(2));
    let child = data.join("child");
    fs::create_dir(&child).unwrap();
    assert_eq!(
        acquire(&child.join(".."), Duration::ZERO).unwrap_err().kind,
        ErrorKind::Busy
    );
    first.verify_profile(&data).unwrap();
}

#[test]
fn caller_cannot_extend_gate_contention_beyond_the_three_second_budget() {
    let root = root();
    let data = profile(&root);
    let _owner = guard(&data);
    let started = Instant::now();
    let error = acquire(&data, Duration::from_secs(60)).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Busy);
    assert!(started.elapsed() >= MAX_LOCK_WAIT);
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(fs::metadata(data.join(LOCK_FILE_NAME)).unwrap().len(), 0);
}

#[test]
fn guard_drop_releases_the_same_empty_marker_and_wrong_profile_is_rejected() {
    let root = root();
    let data = profile(&root);
    let other = root.join("other-profile");
    fs::create_dir(&other).unwrap();
    let first = guard(&data);
    assert_eq!(
        first.verify_profile(&other).unwrap_err().kind,
        ErrorKind::InvalidLock
    );
    let marker = data.join(LOCK_FILE_NAME);
    assert_eq!(fs::metadata(&marker).unwrap().len(), 0);
    drop(first);
    let next = guard(&data);
    next.revalidate().unwrap();
    assert_eq!(fs::read_dir(&data).unwrap().count(), 1);
    drop(next);
    assert_eq!(fs::metadata(marker).unwrap().len(), 0);
}

#[test]
fn unrelated_nonempty_directory_and_hardlinked_lock_objects_are_preserved() {
    for mode in [0, 1, 2] {
        let root = root();
        let data = profile(&root);
        let lock = data.join(LOCK_FILE_NAME);
        match mode {
            0 => write_new(&lock, b"Keep unrelated bytes"),
            1 => fs::create_dir(&lock).unwrap(),
            _ => {
                let original = root.join("another-empty-file");
                write_new(&original, b"");
                fs::hard_link(&original, &lock).unwrap();
            }
        }
        assert_eq!(
            acquire(&data, Duration::ZERO).unwrap_err().kind,
            ErrorKind::InvalidLock
        );
        if mode == 0 {
            assert_eq!(fs::read(&lock).unwrap(), b"Keep unrelated bytes");
        } else {
            assert!(lock.exists());
        }
    }
}

#[test]
fn existing_authority_rejects_missing_replaced_and_changed_files_without_creating_anything() {
    let root = root();
    let data = profile(&root);
    let path = root.join("original.md");
    let held = root.join("preserved-original.md");
    write_new(&path, b"Original bytes");
    let observed = regular(&path);
    let gate = guard(&data);
    gate.verify_existing(&path, observed.fingerprint(), 100)
        .unwrap();
    fs::rename(&path, &held).unwrap();
    assert_eq!(
        gate.verify_existing(&path, observed.fingerprint(), 100)
            .unwrap_err()
            .kind,
        ErrorKind::Missing
    );
    assert!(!path.exists());
    write_new(&path, b"Original bytes");
    assert_eq!(
        gate.verify_existing(&path, observed.fingerprint(), 100)
            .unwrap_err()
            .kind,
        ErrorKind::IdentityChanged
    );
    let replacement = regular(&path);
    fs::write(&path, b"Changed! bytes").unwrap();
    assert_eq!(
        gate.verify_existing(&path, replacement.fingerprint(), 100)
            .unwrap_err()
            .kind,
        ErrorKind::RevisionChanged
    );
    assert_eq!(fs::read(&held).unwrap(), b"Original bytes");
    assert_eq!(fs::read(&path).unwrap(), b"Changed! bytes");
}

#[test]
fn hardlinked_document_is_unsupported_even_when_both_names_have_identical_bytes() {
    let root = root();
    let path = root.join("original.md");
    let alias = root.join("alias.md");
    write_new(&path, b"Synthetic");
    fs::hard_link(&path, &alias).unwrap();
    assert!(matches!(
        observe(&path, 100).unwrap(),
        Observation::Unsupported(UnsupportedReason::MultipleLinks)
    ));
    assert_eq!(fs::read(path).unwrap(), b"Synthetic");
    assert_eq!(fs::read(alias).unwrap(), b"Synthetic");
}

#[test]
fn identity_only_has_no_capture_budget_and_allows_regular_hardlinks() {
    let root = root();
    let data = profile(&root);
    let path = root.join("large.md");
    let alias = root.join("alias.md");
    write_new(&path, b"Synthetic large-file header\r\n");
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(40 * 1024 * 1024)
        .unwrap();
    let identity = match observe_identity(&path).unwrap() {
        IdentityObservation::Regular(identity) => identity,
        other => panic!("Expected identity without reading a 40 MiB body: {other:?}"),
    };
    assert!(matches!(
        observe(&path, u64::MAX).unwrap(),
        Observation::Unsupported(UnsupportedReason::TooLarge)
    ));
    fs::hard_link(&path, &alias).unwrap();
    let gate = guard(&data);
    assert_eq!(
        gate.verify_existing_identity(&path, &identity).unwrap(),
        identity
    );
    assert_eq!(
        observe_identity(&alias).unwrap(),
        IdentityObservation::Regular(identity)
    );
    assert!(matches!(
        observe(&path, u64::MAX).unwrap(),
        Observation::Unsupported(UnsupportedReason::MultipleLinks)
    ));
    assert_eq!(fs::metadata(path).unwrap().len(), 40 * 1024 * 1024);
    assert_eq!(fs::metadata(alias).unwrap().len(), 40 * 1024 * 1024);
}

#[test]
fn identity_only_is_not_content_revision_authority_and_rejects_missing_or_replaced_objects() {
    let root = root();
    let data = profile(&root);
    let path = root.join("original.md");
    let held = root.join("preserved-original.md");
    write_new(&path, b"Original");
    let gate = guard(&data);
    let identity = match observe_identity(&path).unwrap() {
        IdentityObservation::Regular(identity) => identity,
        other => panic!("Expected identity: {other:?}"),
    };
    fs::write(&path, b"Changed in place").unwrap();
    assert_eq!(
        gate.verify_existing_identity(&path, &identity).unwrap(),
        identity,
        "Caller must separately verify the saved content revision"
    );
    fs::rename(&path, &held).unwrap();
    assert_eq!(
        observe_identity(&path).unwrap(),
        IdentityObservation::Missing
    );
    assert_eq!(
        gate.verify_existing_identity(&path, &identity)
            .unwrap_err()
            .kind,
        ErrorKind::Missing
    );
    assert!(!path.exists());
    write_new(&path, b"Changed in place");
    assert_eq!(
        gate.verify_existing_identity(&path, &identity)
            .unwrap_err()
            .kind,
        ErrorKind::IdentityChanged
    );
    assert!(!matches!(
        observe_identity(&root.join("absent-parent/leaf.md")),
        Ok(IdentityObservation::Missing)
    ));
    assert_eq!(
        observe_identity(Path::new("relative.md")).unwrap_err().kind,
        ErrorKind::InvalidPath
    );
    assert_eq!(fs::read(held).unwrap(), b"Changed in place");
    assert_eq!(fs::read(path).unwrap(), b"Changed in place");
}

#[test]
fn same_directory_move_keeps_identity_and_raw_bytes_and_old_existing_authority_stays_invalid() {
    let root = root();
    let data = profile(&root);
    let source = root.join("original.md");
    let target = root.join("renamed.md");
    write_new(&source, b"\xef\xbb\xbf# Preserve\r\n");
    let observed = regular(&source);
    let fingerprint = observed.fingerprint().clone();
    let gate = guard(&data);
    gate.verify_missing(&target).unwrap();
    let receipt = gate
        .move_same_directory_noreplace(&observed, OsStr::new("renamed.md"))
        .unwrap();
    assert_eq!(receipt.fingerprint, fingerprint);
    assert!(!source.exists());
    assert_eq!(regular(&target).fingerprint(), &fingerprint);
    assert_eq!(
        gate.verify_existing(&source, &fingerprint, 100)
            .unwrap_err()
            .kind,
        ErrorKind::Missing
    );
    assert_eq!(
        fs::read_dir(&root).unwrap().count(),
        2,
        "No hidden temporary rename path"
    );
}

#[test]
fn existing_file_directory_and_same_byte_third_party_target_are_never_overwritten() {
    for mode in [0, 1, 2] {
        let root = root();
        let data = profile(&root);
        let source = root.join("original.md");
        let target = root.join("target.md");
        write_new(&source, b"Original");
        if mode == 1 {
            fs::create_dir(&target).unwrap();
        } else {
            write_new(
                &target,
                if mode == 0 {
                    b"Third party"
                } else {
                    b"Original"
                },
            );
        }
        let observed = regular(&source);
        let gate = guard(&data);
        let error = gate
            .move_same_directory_noreplace(&observed, OsStr::new("target.md"))
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::TargetExists);
        assert_eq!(error.move_state, MoveState::NotAttempted);
        assert_eq!(fs::read(&source).unwrap(), b"Original");
        if mode == 1 {
            assert!(target.is_dir());
        } else {
            assert_eq!(
                fs::read(target).unwrap(),
                if mode == 0 {
                    b"Third party".as_slice()
                } else {
                    b"Original".as_slice()
                }
            );
        }
    }
}

#[test]
fn deterministic_target_creation_after_precheck_is_refused_by_the_real_native_primitive() {
    let root = root();
    let data = profile(&root);
    let source = root.join("original.md");
    let target = root.join("target.md");
    write_new(&source, b"Original");
    let observed = regular(&source);
    let gate = guard(&data);
    let contender = target.clone();
    BEFORE_MOVE.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move || write_new(&contender, b"Late third party")))
    });
    let error = gate
        .move_same_directory_noreplace(&observed, OsStr::new("target.md"))
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::TargetExists);
    assert_eq!(error.move_state, MoveState::Unknown);
    assert_eq!(fs::read(source).unwrap(), b"Original");
    assert_eq!(fs::read(target).unwrap(), b"Late third party");
}

#[test]
fn after_move_conflict_reports_moved_and_preserves_both_names_without_rollback() {
    let root = root();
    let data = profile(&root);
    let source = root.join("original.md");
    let target = root.join("target.md");
    write_new(&source, b"Original");
    let observed = regular(&source);
    let gate = guard(&data);
    let contender = source.clone();
    AFTER_MOVE.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move || {
            write_new(&contender, b"New external source")
        }))
    });
    let error = gate
        .move_same_directory_noreplace(&observed, OsStr::new("target.md"))
        .unwrap_err();
    assert_eq!(error.move_state, MoveState::Moved);
    assert_eq!(fs::read(source).unwrap(), b"New external source");
    assert_eq!(fs::read(target).unwrap(), b"Original");
}

#[test]
fn external_source_replacement_after_precheck_is_reported_after_move_without_deleting_either_object(
) {
    let root = root();
    let data = profile(&root);
    let source = root.join("original.md");
    let held = root.join("preserved-original.md");
    let target = root.join("target.md");
    write_new(&source, b"Original");
    let observed = regular(&source);
    let gate = guard(&data);
    let boundary_source = source.clone();
    let boundary_held = held.clone();
    BEFORE_MOVE.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move || {
            fs::rename(&boundary_source, &boundary_held).unwrap();
            write_new(&boundary_source, b"External");
        }))
    });
    let error = gate
        .move_same_directory_noreplace(&observed, OsStr::new("target.md"))
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::IdentityChanged);
    assert_eq!(error.move_state, MoveState::Moved);
    assert_eq!(fs::read(held).unwrap(), b"Original");
    assert_eq!(fs::read(target).unwrap(), b"External");
    assert!(!source.exists());
}

#[test]
fn target_name_never_selects_a_different_directory_or_windows_stream() {
    let root = root();
    let data = profile(&root);
    let source = root.join("original.md");
    write_new(&source, b"Original");
    let observed = regular(&source);
    let gate = guard(&data);
    for name in ["", ".", "..", "../outside.md", "nested/target.md"] {
        assert_eq!(
            gate.move_same_directory_noreplace(&observed, OsStr::new(name))
                .unwrap_err()
                .kind,
            ErrorKind::InvalidPath
        );
    }
    #[cfg(windows)]
    for name in [
        "target.md:stream",
        "CON",
        "LPT1.txt",
        "COM¹.md",
        "target.md.",
        "target.md ",
        "nested\\target.md",
    ] {
        assert_eq!(
            gate.move_same_directory_noreplace(&observed, OsStr::new(name))
                .unwrap_err()
                .kind,
            ErrorKind::InvalidPath
        );
    }
    assert_eq!(fs::read(source).unwrap(), b"Original");
}

#[cfg(windows)]
#[test]
fn case_only_alias_is_explicitly_refused_without_a_temporary_name() {
    let root = root();
    let data = profile(&root);
    let source = root.join("Case.md");
    write_new(&source, b"Original");
    let observed = regular(&source);
    let gate = guard(&data);
    assert_eq!(
        gate.move_same_directory_noreplace(&observed, OsStr::new("case.md"))
            .unwrap_err()
            .kind,
        ErrorKind::TargetExists
    );
    assert!(fs::read_dir(&root)
        .unwrap()
        .any(|entry| entry.unwrap().file_name() == "Case.md"));
    assert_eq!(fs::read(source).unwrap(), b"Original");
}

#[cfg(windows)]
fn create_junction(link: &Path, target: &Path) {
    use std::os::windows::{fs::MetadataExt, process::CommandExt};
    // PathBuf preserves input separators. cmd's mklink treats unquoted `/`
    // segments as switches, so rebuild both fixture paths with native ones.
    let link = link.components().collect::<PathBuf>();
    let target = target.components().collect::<PathBuf>();
    let output = Command::new("cmd.exe")
        .args(["/D", "/C", "mklink", "/J"])
        .arg(&link)
        .arg(&target)
        .creation_flags(0x08000000)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Synthetic junction creation failed: status={} stdout={:?} stderr={:?}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_ne!(
        fs::symlink_metadata(&link).unwrap().file_attributes() & 0x400,
        0,
        "Synthetic junction must be a real reparse point"
    );
    assert_eq!(
        fs::canonicalize(&link).unwrap(),
        fs::canonicalize(&target).unwrap(),
        "Synthetic junction must redirect to the owned target"
    );
}

#[cfg(windows)]
#[test]
fn windows_junction_parent_leaf_and_lock_are_refused_without_following_redirects() {
    let root = root();
    let actual = root.join("actual target");
    let redirected = root.join("redirected path");
    fs::create_dir(&actual).unwrap();
    let original = actual.join("original.md");
    write_new(&original, b"Preserve junction target");
    create_junction(&redirected, &actual);
    assert_eq!(
        fs::read(redirected.join("original.md")).unwrap(),
        b"Preserve junction target"
    );
    assert!(matches!(
        observe(&redirected.join("original.md"), 100).unwrap(),
        Observation::Unsupported(UnsupportedReason::UnsafePath)
    ));
    assert!(matches!(
        observe_identity(&redirected.join("original.md")).unwrap(),
        IdentityObservation::Unsupported(UnsupportedReason::UnsafePath)
    ));
    assert!(matches!(
        observe(&redirected, 100).unwrap(),
        Observation::Unsupported(UnsupportedReason::UnsafePath)
    ));
    let data = profile(&root);
    let lock = data.join(LOCK_FILE_NAME);
    create_junction(&lock, &actual);
    assert_eq!(
        acquire(&data, Duration::ZERO).unwrap_err().kind,
        ErrorKind::InvalidLock
    );
    assert_eq!(fs::read(original).unwrap(), b"Preserve junction target");
}

fn child(mode: &str, data: &Path, marker: &Path) -> Child {
    let name = format!(
        "{}::lease_child",
        module_path!()
            .split("::")
            .skip(1)
            .collect::<Vec<_>>()
            .join("::")
    );
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--ignored", "--exact", &name, "--nocapture"])
        .env("READMD_DOCOPS_CHILD", mode)
        .env("READMD_DOCOPS_CHILD_PROFILE", data)
        .env("READMD_DOCOPS_CHILD_MARKER", marker)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.spawn().unwrap()
}
fn wait_marker(child: &mut Child, marker: &Path) {
    let started = Instant::now();
    while !marker.exists() {
        assert!(
            child.try_wait().unwrap().is_none(),
            "Synthetic child exited before owning its gate"
        );
        if started.elapsed() > Duration::from_secs(8) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("Synthetic child gate deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
#[ignore = "Only the deterministic process parent invokes this synthetic helper"]
fn lease_child() {
    let Some(mode) = std::env::var_os("READMD_DOCOPS_CHILD") else {
        return;
    };
    let data = PathBuf::from(std::env::var_os("READMD_DOCOPS_CHILD_PROFILE").unwrap());
    let marker = PathBuf::from(std::env::var_os("READMD_DOCOPS_CHILD_MARKER").unwrap());
    if mode == "busy" {
        assert_eq!(
            acquire(&data, Duration::from_millis(50)).unwrap_err().kind,
            ErrorKind::Busy
        );
        return;
    }
    let _gate = guard(&data);
    write_new(&marker, b"ready");
    let started = Instant::now();
    while !marker.with_extension("release").exists() {
        assert!(started.elapsed() < Duration::from_secs(15));
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn two_real_processes_contend_and_graceful_exit_releases_the_gate() {
    let root = root();
    let data = profile(&root);
    let ready = root.join("ready");
    let gate = guard(&data);
    let mut probe = child("busy", &data, &ready);
    assert!(probe.wait().unwrap().success());
    drop(gate);
    let mut owner = child("hold", &data, &ready);
    wait_marker(&mut owner, &ready);
    assert_eq!(
        acquire(&data, Duration::ZERO).unwrap_err().kind,
        ErrorKind::Busy
    );
    write_new(&ready.with_extension("release"), b"release");
    assert!(owner.wait().unwrap().success());
    drop(guard(&data));
    assert_eq!(fs::metadata(data.join(LOCK_FILE_NAME)).unwrap().len(), 0);
}
#[test]
fn abrupt_owned_process_exit_releases_without_pid_cleanup_or_lock_deletion() {
    let root = root();
    let data = profile(&root);
    let ready = root.join("ready");
    let mut owner = child("hold", &data, &ready);
    wait_marker(&mut owner, &ready);
    assert_eq!(
        acquire(&data, Duration::ZERO).unwrap_err().kind,
        ErrorKind::Busy
    );
    owner.kill().unwrap();
    owner.wait().unwrap();
    drop(guard(&data));
    assert_eq!(fs::read_dir(data).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn unix_leaf_parent_and_lock_symlinks_are_not_followed() {
    use std::os::unix::fs::symlink;
    let root = root();
    let target = root.join("target.md");
    let alias = root.join("alias.md");
    write_new(&target, b"Original");
    symlink(&target, &alias).unwrap();
    assert!(matches!(
        observe(&alias, 100).unwrap(),
        Observation::Unsupported(_)
    ));
    let redirected = root.join("redirected");
    symlink(&root, &redirected).unwrap();
    assert!(!matches!(
        observe(&redirected.join("target.md"), 100),
        Ok(Observation::Regular(_))
    ));
    let data = profile(&root);
    symlink(&target, data.join(LOCK_FILE_NAME)).unwrap();
    assert!(acquire(&data, Duration::ZERO).is_err());
    assert_eq!(fs::read(target).unwrap(), b"Original");
}
