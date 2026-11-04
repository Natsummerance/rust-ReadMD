//! Controlled validation barriers exercise lock lifetime independently of
//! Node startup speed. HTTP acceptance separately uses the real locked pack.
use super::*;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

fn fixture(root: &Path, id: &str) -> (PathBuf, PathBuf, Spec, Value) {
    let data = root.join("data");
    let assets = root.join("assets");
    let selected = spec(id).unwrap();
    let target = root_for(&data, selected).join(selected.version);
    let mut expected = super::tests::fixture(&target);
    expected["id"] = json!(selected.id);
    expected["version"] = json!(selected.version);
    expected["platform"] = json!(selected.platform);
    fs::create_dir_all(assets.join("capabilities")).unwrap();
    fs::write(
        assets.join(format!(
            "capabilities/{}-{}-{}.lock.json",
            selected.id, selected.version, selected.platform
        )),
        serde_json::to_vec(&expected).unwrap(),
    )
    .unwrap();
    (data, assets, selected, expected)
}
fn pin(data: &Path, selected: Spec) -> StatusRead {
    let guard = resource_guard();
    let exclusive = crate::capability_process::try_acquire(
        data,
        selected.id,
        crate::capability_process::Mode::Exclusive,
    )
    .unwrap()
    .unwrap();
    status_read(guard, data, selected, exclusive, false, None)
        .unwrap()
        .unwrap()
}
fn no_writer(data: &Path, id: &str) {
    assert!(crate::capability_process::try_acquire(
        data,
        id,
        crate::capability_process::Mode::Exclusive
    )
    .unwrap()
    .is_none());
}
fn released(data: &Path, id: &str) {
    assert!(crate::capability_process::try_acquire(
        data,
        id,
        crate::capability_process::Mode::Exclusive
    )
    .unwrap()
    .is_some());
}

#[test]
fn pinned_status_validation_allows_run_shared_lease_without_allowing_uninstall() {
    let root = tempfile::tempdir().unwrap();
    let (data, assets, selected, expected) = fixture(root.path(), "node");
    let (entered_tx, entered_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let resume_rx = Mutex::new(resume_rx);
    std::thread::scope(|scope| {
        let reader = scope.spawn(|| {
            let reader = pin(&data, selected);
            assert!(reader.busy.is_none());
            let first = AtomicBool::new(true);
            validate_checked(&reader.directory, &expected, &|| {
                if first.swap(false, Ordering::AcqRel) {
                    entered_tx.send(()).unwrap();
                    resume_rx
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(5))
                        .unwrap();
                }
                Ok(())
            })
            .unwrap();
            assert!(reader.valid(&assets, &expected).unwrap());
            no_writer(&data, "node");
        });
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        // This is a preparation bound, not the HTTP cancellation deadline.
        let deadline = Instant::now() + Duration::from_secs(2);
        drop(
            resource_guard_checked(&|| {
                if Instant::now() < deadline {
                    Ok(())
                } else {
                    Err(error())
                }
            })
            .expect("status must release INSTALL before validating"),
        );
        let run = lease(&data, &assets, "node").unwrap();
        assert_eq!(
            leases().lock().unwrap().get(&lease_key(&data, selected)),
            Some(&1),
            "status itself must not publish an execution lease"
        );
        no_writer(&data, "node");
        let second_slot = crate::capability_process::try_worker_slot(&data, "node")
            .unwrap()
            .unwrap();
        assert!(
            crate::capability_process::try_worker_slot(&data, "node")
                .unwrap()
                .is_none(),
            "probe plus one run retain the existing two-slot bound"
        );
        drop(second_slot);
        drop(run);
        no_writer(&data, "node");
        resume_tx.send(()).unwrap();
        reader.join().unwrap();
    });
    released(&data, "node");
    assert!(!leases()
        .lock()
        .unwrap()
        .contains_key(&lease_key(&data, selected)));
}

#[test]
fn probe_slot_exhaustion_is_busy_while_pinned_resource_still_allows_readers() {
    let root = tempfile::tempdir().unwrap();
    let (data, assets, selected, expected) = fixture(root.path(), "node");
    let first = crate::capability_process::try_worker_slot(&data, "node")
        .unwrap()
        .unwrap();
    let second = crate::capability_process::try_worker_slot(&data, "node")
        .unwrap()
        .unwrap();
    let reader = pin(&data, selected);
    assert_eq!(reader.busy, Some("capability_install_busy"));
    assert!(reader._probe_slot.is_none());
    assert!(reader.valid(&assets, &expected).unwrap());
    no_writer(&data, "node");
    assert!(crate::capability_process::try_acquire(
        &data,
        "node",
        crate::capability_process::Mode::Shared
    )
    .unwrap()
    .is_some());
    drop(reader);
    drop(first);
    drop(second);
    released(&data, "node");
}

#[test]
fn another_reader_remains_conservatively_busy_and_does_not_start_an_extra_probe() {
    let root = tempfile::tempdir().unwrap();
    let (data, assets, selected, expected) = fixture(root.path(), "node");
    let first = pin(&data, selected);
    let guard = resource_guard();
    assert!(crate::capability_process::try_acquire(
        &data,
        "node",
        crate::capability_process::Mode::Exclusive
    )
    .unwrap()
    .is_none());
    let shared = crate::capability_process::try_acquire(
        &data,
        "node",
        crate::capability_process::Mode::Shared,
    )
    .unwrap()
    .unwrap();
    let second = status_read(guard, &data, selected, shared, true, None)
        .unwrap()
        .unwrap();
    assert_eq!(second.busy, Some("capability_install_busy"));
    assert!(second._probe_slot.is_none());
    assert!(second.valid(&assets, &expected).unwrap());
    let remaining = crate::capability_process::try_worker_slot(&data, "node")
        .unwrap()
        .unwrap();
    assert!(crate::capability_process::try_worker_slot(&data, "node")
        .unwrap()
        .is_none());
    drop(remaining);
    drop(first);
    no_writer(&data, "node");
    drop(second);
    released(&data, "node");
}

#[test]
fn invalid_probe_slot_object_is_preserved_and_all_acquired_guards_are_released() {
    let root = tempfile::tempdir().unwrap();
    let (data, _assets, selected, _expected) = fixture(root.path(), "node");
    let guard = resource_guard();
    let exclusive = crate::capability_process::try_acquire(
        &data,
        "node",
        crate::capability_process::Mode::Exclusive,
    )
    .unwrap()
    .unwrap();
    let sentinel = data.join("capabilities/managed-v2/.locks/node.worker-0.lock");
    fs::write(&sentinel, b"Owned unknown object").unwrap();
    assert!(status_read(guard, &data, selected, exclusive, false, None).is_err());
    assert_eq!(fs::read(sentinel).unwrap(), b"Owned unknown object");
    released(&data, "node");
    assert!(!leases()
        .lock()
        .unwrap()
        .contains_key(&lease_key(&data, selected)));
}

#[test]
fn read_scope_rechecks_blocking_transaction_before_accepting_valid_payload() {
    let root = tempfile::tempdir().unwrap();
    let (data, assets, selected, expected) = fixture(root.path(), "node");
    let reader = pin(&data, selected);
    assert!(reader.valid(&assets, &expected).unwrap());
    let nonce = uuid::Uuid::new_v4().to_string();
    let journal = root_for(&data, selected).join(format!("remove-txn-{nonce}"));
    fs::create_dir(&journal).unwrap();
    let record = json!({"schema":1,"owner":"ReadMD capability removal v1","nonce":nonce,"manifest":transaction::fingerprint(&expected).unwrap(),"source":selected.version,"files":[],"phase":"prepared"});
    fs::write(
        journal.join("removal-v1.json"),
        serde_json::to_vec(&record).unwrap(),
    )
    .unwrap();
    let sentinel = journal.join("unknown-note");
    fs::write(&sentinel, b"Do not recover or remove under a read lease").unwrap();
    // A synthetic prepared record models what a cross-process writer could
    // leave in the Exclusive -> Shared gap. Payload bytes remain valid.
    validate(&reader.directory, &expected).unwrap();
    assert!(!reader.valid(&assets, &expected).unwrap());
    assert_eq!(
        fs::read(sentinel).unwrap(),
        b"Do not recover or remove under a read lease"
    );
    no_writer(&data, "node");
    drop(reader);
    released(&data, "node");
}

#[test]
fn failed_or_cancelled_validation_drops_pin_and_probe_admission_without_changing_input() {
    for cancelled in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let (data, assets, selected, expected) = fixture(root.path(), "node");
        let reader = pin(&data, selected);
        if cancelled {
            assert!(
                validate_checked(&reader.directory, &expected, &|| Err(ApiError::new(
                    422,
                    "cancelled"
                )))
                .is_err()
            );
        } else {
            fs::write(reader.directory.join("NOTICE"), b"Owned invalid bytes").unwrap();
            assert!(!reader.valid(&assets, &expected).unwrap());
            assert_eq!(
                fs::read(reader.directory.join("NOTICE")).unwrap(),
                b"Owned invalid bytes"
            );
        }
        no_writer(&data, "node");
        drop(reader);
        released(&data, "node");
        let first = crate::capability_process::try_worker_slot(&data, "node")
            .unwrap()
            .unwrap();
        let second = crate::capability_process::try_worker_slot(&data, "node")
            .unwrap()
            .unwrap();
        drop(first);
        drop(second);
    }
}
