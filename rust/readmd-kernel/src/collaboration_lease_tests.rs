//! These tests use real filesystem leases and SQLite connections, not PID mocks.
use super::*;

fn busy(path: &Path) {
    let error = Service::open(path)
        .err()
        .expect("A second writer must be refused");
    assert_eq!(error.status, 409);
    assert_eq!(error.code, "collaboration_busy");
}

fn client(service: &Service, owner: &Credential) -> Doc {
    let doc = fresh();
    let response = service
        .sync(&owner.room, &owner.token, &STANDARD.encode([0]), None, None)
        .unwrap();
    doc.transact_mut()
        .apply_update(
            Update::decode_v1(
                &STANDARD
                    .decode(response["update"].as_str().unwrap())
                    .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    doc
}

#[test]
fn one_writer_owns_the_canonical_database_until_sqlite_is_closed() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("collaboration.sqlite");
    let first = Service::open(&path).unwrap();
    let owner = first
        .create("Durable", "Acknowledged content 🦀", "Owner")
        .unwrap();
    busy(&path);
    let child = data.path().join("child");
    std::fs::create_dir(&child).unwrap();
    busy(&child.join("..").join("collaboration.sqlite"));
    // Ownership is per database, not a global process-wide collaboration lock.
    let independent = Service::open(&data.path().join("independent.sqlite")).unwrap();
    drop(independent);
    drop(first);
    let marker = data.path().join("collaboration.sqlite.owner.lock");
    assert_eq!(std::fs::metadata(&marker).unwrap().len(), 0);
    let replacement = Service::open(&path).unwrap();
    assert_eq!(
        replacement.snapshot(&owner.room, &owner.token).unwrap()["content"],
        "Acknowledged content 🦀"
    );
    busy(&path);
    drop(replacement);
    assert!(marker.is_file(), "Stable lock names must never be removed");
}

#[test]
fn separate_read_only_backup_remains_available_while_the_writer_lease_is_held() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("collaboration.sqlite");
    let service = Service::open(&path).unwrap();
    let owner = service.create("Live", "Original", "Owner").unwrap();
    let doc = client(&service, &owner);
    let vector = doc.transact().state_vector();
    doc.get_or_insert_text("content")
        .insert(&mut doc.transact_mut(), 8, " WAL edit 🦀");
    service
        .sync(
            &owner.room,
            &owner.token,
            &STANDARD.encode(vector.encode_v1()),
            Some(&STANDARD.encode(doc.transact().encode_state_as_update_v1(&vector))),
            None,
        )
        .unwrap();
    let asset = service
        .attachment(
            &owner.room,
            &owner.token,
            "source.txt",
            "text/plain",
            b"Synthetic source",
        )
        .unwrap();
    busy(&path);
    let backup = data.path().join("online.sqlite");
    Service::backup_file(&path, &backup).unwrap();
    let original_backup = std::fs::read(&backup).unwrap();
    assert_eq!(
        Service::backup_file(&path, &backup).unwrap_err().status,
        409
    );
    assert_eq!(std::fs::read(&backup).unwrap(), original_backup);
    let restored = data.path().join("restored.sqlite");
    Service::restore_backup(&backup, &restored).unwrap();
    let restored = Service::open(&restored).unwrap();
    assert_eq!(
        restored.snapshot(&owner.room, &owner.token).unwrap()["content"],
        "Original WAL edit 🦀"
    );
    assert_eq!(
        restored
            .read_attachment(&owner.room, &owner.token, &asset)
            .unwrap(),
        b"Synthetic source"
    );
    assert_eq!(
        service.snapshot(&owner.room, &owner.token).unwrap()["content"],
        "Original WAL edit 🦀"
    );
}

#[test]
fn invalid_lock_objects_and_failed_restore_do_not_create_or_overwrite_a_database() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("blocked.sqlite");
    let marker = data.path().join("blocked.sqlite.owner.lock");
    std::fs::write(&marker, b"Keep this unrelated file").unwrap();
    assert_eq!(
        Service::open(&path).err().unwrap().code,
        "collaboration_storage"
    );
    assert_eq!(std::fs::read(&marker).unwrap(), b"Keep this unrelated file");
    assert!(!path.exists());
    let directory_target = data.path().join("directory.sqlite");
    std::fs::create_dir(data.path().join("directory.sqlite.owner.lock")).unwrap();
    assert_eq!(
        Service::open(&directory_target).err().unwrap().code,
        "collaboration_storage"
    );
    assert!(!directory_target.exists());
    let source = data.path().join("source.sqlite");
    let service = Service::open(&source).unwrap();
    let owner = service
        .create("Retained", "Keep acknowledged content", "Owner")
        .unwrap();
    let backup = data.path().join("backup.sqlite");
    service.backup(&backup).unwrap();
    assert!(Service::restore_backup(&backup, &path).is_err());
    assert!(!path.exists());
    assert_eq!(
        Service::restore_backup(&backup, &source)
            .unwrap_err()
            .status,
        409
    );
    assert_eq!(
        service.snapshot(&owner.room, &owner.token).unwrap()["content"],
        "Keep acknowledged content"
    );
}

#[test]
fn hard_linked_database_aliases_cannot_create_a_second_writer() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("collaboration.sqlite");
    let service = Service::open(&path).unwrap();
    let owner = service.create("Retained", "Keep content", "Owner").unwrap();
    let alias = data.path().join("alias.sqlite");
    std::fs::hard_link(&path, &alias).unwrap();
    assert_eq!(
        Service::open(&alias).err().unwrap().code,
        "collaboration_storage"
    );
    std::fs::remove_file(&alias).unwrap();
    assert_eq!(
        service.snapshot(&owner.room, &owner.token).unwrap()["content"],
        "Keep content"
    );
}

#[test]
fn writer_lease_is_cross_process_and_process_termination_releases_it() {
    const CHILD: &str = "READMD_COLLABORATION_LEASE_CHILD";
    if let Some(path) = std::env::var_os(CHILD) {
        let path = std::path::PathBuf::from(path);
        let _service = Service::open(&path).unwrap();
        std::fs::write(path.with_extension("ready"), b"ready").unwrap();
        std::thread::sleep(std::time::Duration::from_secs(15));
        panic!("The parent must terminate its owned lease-test process");
    }
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("collaboration.sqlite");
    let service = Service::open(&path).unwrap();
    let owner = service
        .create("Durable", "Acknowledged before the child starts", "Owner")
        .unwrap();
    drop(service);
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", "collaboration::lease_tests::writer_lease_is_cross_process_and_process_termination_releases_it", "--test-threads=1"])
        .env(CHILD, &path).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = Child(command.spawn().unwrap());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !path.with_extension("ready").is_file() {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "The child exited before acquiring its lease"
        );
        assert!(
            std::time::Instant::now() < deadline,
            "Lease readiness deadline"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    busy(&path);
    Service::backup_file(&path, &data.path().join("live-child.sqlite")).unwrap();
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    let replacement = Service::open(&path).unwrap();
    assert_eq!(
        replacement.snapshot(&owner.room, &owner.token).unwrap()["content"],
        "Acknowledged before the child starts"
    );
}

#[cfg(unix)]
#[test]
fn symlinked_lock_names_are_rejected_without_following_them() {
    let data = tempfile::tempdir().unwrap();
    let target = data.path().join("retained");
    std::fs::write(&target, b"Keep target").unwrap();
    std::os::unix::fs::symlink(&target, data.path().join("collaboration.sqlite.owner.lock"))
        .unwrap();
    assert_eq!(
        Service::open(&data.path().join("collaboration.sqlite"))
            .err()
            .unwrap()
            .code,
        "collaboration_storage"
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"Keep target");
    assert!(!data.path().join("collaboration.sqlite").exists());
}
