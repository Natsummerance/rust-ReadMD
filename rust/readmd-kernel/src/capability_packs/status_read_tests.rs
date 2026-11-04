//! Controlled validation barriers exercise lock lifetime independently of
//! Node startup speed. HTTP acceptance separately uses the real locked pack.
use super::*;
use std::{
    cell::RefCell,
    io,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
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

const STATUS_VALIDATION_CHILD: &str =
    "capability_packs::status_read_tests::actual_status_validation_runtime_lease_child";
const STATUS_VALIDATION_OWNER: &str = "ReadMD actual status validation lease test v1";

#[derive(Default)]
struct ReadGateState {
    hits: AtomicUsize,
    paused: AtomicBool,
    resumed: AtomicBool,
    failed: AtomicBool,
}
struct ReadEntered {
    path: PathBuf,
    nonce: String,
    bytes: usize,
    deadline: Instant,
}
struct ReadGate {
    path: PathBuf,
    nonce: String,
    entered: mpsc::Sender<ReadEntered>,
    resume: mpsc::Receiver<String>,
    state: Arc<ReadGateState>,
}
thread_local! {
    static READ_GATE: RefCell<Option<ReadGate>> = RefCell::new(None);
}
struct ScopedReadGate(Option<ReadGate>);
impl ScopedReadGate {
    fn install(gate: ReadGate) -> Self {
        Self(READ_GATE.with(|slot| slot.replace(Some(gate))))
    }
}
impl Drop for ScopedReadGate {
    fn drop(&mut self) {
        READ_GATE.with(|slot| *slot.borrow_mut() = self.0.take());
    }
}
fn receive_before<T>(receiver: &mpsc::Receiver<T>, deadline: Instant) -> Result<T, String> {
    let now = Instant::now();
    if now >= deadline {
        return Err("deadline expired before receive".into());
    }
    let result = receiver
        .recv_timeout(deadline.saturating_duration_since(now))
        .map_err(|error| error.to_string());
    if Instant::now() >= deadline {
        return Err("receive returned after the absolute deadline".into());
    }
    result
}
impl ReadGate {
    fn pause(self, bytes: usize) -> io::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(5);
        self.state.hits.fetch_add(1, Ordering::AcqRel);
        self.state.paused.store(true, Ordering::Release);
        let result: Result<(), String> = (|| {
            self.entered
                .send(ReadEntered {
                    path: self.path.clone(),
                    nonce: self.nonce.clone(),
                    bytes,
                    deadline,
                })
                .map_err(|error| error.to_string())?;
            let token = receive_before(&self.resume, deadline)?;
            if token != self.nonce {
                return Err("wrong read-gate resume nonce".into());
            }
            Ok(())
        })();
        if result.is_ok() {
            self.state.resumed.store(true, Ordering::Release);
        } else {
            self.state.failed.store(true, Ordering::Release);
        }
        self.state.paused.store(false, Ordering::Release);
        result.map_err(|message| io::Error::new(io::ErrorKind::Other, message))
    }
}
struct ObservedReader {
    file: fs::File,
    gate: Option<ReadGate>,
}
impl Read for ObservedReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let n = self.file.read(buffer)?;
        // Pause only after this original File has actually supplied bytes,
        // before hash_reader can digest them or complete the real validation.
        if n > 0 {
            if let Some(gate) = self.gate.take() {
                gate.pause(n)?;
            }
        }
        Ok(n)
    }
}
pub(super) fn observed_reader(path: &Path, file: fs::File) -> impl Read {
    let gate = READ_GATE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let armed = slot.as_ref()?;
        // Other tests and the Run thread do not canonicalize or pause. The
        // gate is taken before read(), so no TLS borrow is held while waiting.
        if dunce::canonicalize(path).ok().as_ref() == Some(&armed.path) {
            slot.take()
        } else {
            None
        }
    });
    ObservedReader { file, gate }
}

struct StatusThread {
    cancel: Arc<AtomicBool>,
    resume: Option<mpsc::Sender<String>>,
    nonce: String,
    handle: Option<std::thread::JoinHandle<()>>,
}
impl StatusThread {
    fn resume(&mut self) {
        self.resume
            .take()
            .expect("read gate already released")
            .send(self.nonce.clone())
            .expect("real status stopped before read gate resumed");
    }
    fn join(&mut self) {
        self.handle.take().unwrap().join().expect("status panicked");
    }
}
impl Drop for StatusThread {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(resume) = self.resume.take() {
            let _ = resume.send(self.nonce.clone());
        }
        if let Some(handle) = self.handle.take() {
            // Release the read gate before joining, including on assertion
            // unwind. The parent's owned-child watchdog bounds a stuck join.
            let _ = handle.join();
        }
    }
}

fn actual_status_validation(root: &Path, nonce: &str) -> Value {
    let (data, assets, selected, expected) = fixture(root, "fonts");
    let target = root_for(&data, selected).join(selected.version);
    let read_path = dunce::canonicalize(target.join("bin/engine")).unwrap();
    let original = checked_files(&expected)
        .unwrap()
        .iter()
        .map(|item| {
            let name = item["path"].as_str().unwrap().to_string();
            let bytes = fs::read(target.join(&name)).unwrap();
            (name, bytes)
        })
        .collect::<Vec<_>>();
    let expected_read_size = fs::metadata(&read_path).unwrap().len();
    let sentinel = target.join("owned-user-note");
    let sentinel_bytes = b"Owned unknown file must remain unchanged";
    fs::write(&sentinel, sentinel_bytes).unwrap();
    let workspace = root.join("workspace");
    fs::create_dir(&workspace).unwrap();
    assert!(target.is_dir());
    assert!(!legacy::needed(&data, selected).unwrap());
    assert!(!transactions_pending(&root_for(&data, selected)).unwrap());
    assert!(!leases().lock().unwrap().contains_key(&lease_key(&data, selected)));
    let app = Arc::new(
        App::bootstrap(crate::paths::AppPaths::with_dirs(&data, &workspace, &assets)).unwrap(),
    );
    let (entered_tx, entered_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let state = Arc::new(ReadGateState::default());
    let cancel = Arc::new(AtomicBool::new(false));
    let gate = ReadGate {
        path: read_path.clone(),
        nonce: nonce.into(),
        entered: entered_tx,
        resume: resume_rx,
        state: state.clone(),
    };
    let status_app = app.clone();
    let status_cancel = cancel.clone();
    let entry_deadline = Instant::now() + Duration::from_secs(5);
    let handle = std::thread::spawn(move || {
        let result = crate::speech::with_cancel(
            Arc::new(move || status_cancel.load(Ordering::Acquire)),
            || {
                let _gate = ScopedReadGate::install(gate);
                operate(
                    &status_app,
                    &json!({"action":"status","id":"fonts"}),
                    None,
                    &InstallHooks::direct(),
                )
            },
        );
        let _ = done_tx.send(result);
    });
    let mut status = StatusThread {
        cancel,
        resume: Some(resume_tx),
        nonce: nonce.into(),
        handle: Some(handle),
    };
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let entered = receive_before(&entered_rx, entry_deadline)
            .expect("actual operate/status validation did not read the fixture");
        assert_eq!(entered.path, read_path);
        assert_eq!(entered.nonce, nonce);
        assert!(entered.bytes > 0 && entered.bytes as u64 <= expected_read_size);
        assert!(Instant::now() < entered.deadline);
        assert_eq!(state.hits.load(Ordering::Acquire), 1);
        assert!(state.paused.load(Ordering::Acquire));
        assert!(matches!(done_rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
        eprintln!("STATUS_VALIDATION_PHASE actual-read-entered bytes={}", entered.bytes);

        let started = Instant::now();
        let lease_deadline = started + Duration::from_secs(2);
        let result = lease_checked(&data, &assets, "fonts", &|| {
            if Instant::now() < lease_deadline {
                Ok(())
            } else {
                Err(ApiError::new(422, "status_validation_test_deadline"))
            }
        });
        let returned_at = Instant::now();
        assert!(
            returned_at < lease_deadline,
            "Run lease returned after absolute 2s preparation deadline: {}",
            result.as_ref().err().map_or("ok", |error| error.code.as_str())
        );
        let run = result.expect("real runtime lease must enter while status read is paused");
        let lease_elapsed_ms = returned_at.duration_since(started).as_millis();
        assert!(Instant::now() < entered.deadline);
        assert!(state.paused.load(Ordering::Acquire));
        assert!(!state.resumed.load(Ordering::Acquire));
        assert!(!state.failed.load(Ordering::Acquire));
        assert!(matches!(done_rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
        assert_eq!(leases().lock().unwrap().get(&lease_key(&data, selected)), Some(&1));
        no_writer(&data, "fonts");
        eprintln!("STATUS_VALIDATION_PHASE runtime-lease-acquired elapsed_ms={lease_elapsed_ms}");

        // Keep this same actual use guard until the real response and join.
        let completion_deadline = Instant::now() + Duration::from_secs(3);
        status.resume();
        let response = receive_before(&done_rx, completion_deadline)
            .expect("actual status did not finish after read resumed")
            .expect("actual status returned an API error");
        assert_eq!(response.status, 200);
        let body: Value = serde_json::from_slice(&response.body).unwrap();
        for field in ["ok", "installed", "busy", "present"] {
            assert_eq!(body[field], true, "status {field}");
        }
        assert_eq!(body["ready"], false);
        assert_eq!(body["busy_code"], "capability_install_busy");
        assert_eq!(state.hits.load(Ordering::Acquire), 1);
        assert!(state.resumed.load(Ordering::Acquire));
        assert!(!state.paused.load(Ordering::Acquire));
        assert!(!state.failed.load(Ordering::Acquire));
        assert_eq!(run.directory, target);
        assert_eq!(leases().lock().unwrap().get(&lease_key(&data, selected)), Some(&1));
        no_writer(&data, "fonts");
        status.join();
        assert!(Instant::now() < completion_deadline, "status join exceeded absolute deadline");
        assert_eq!(leases().lock().unwrap().get(&lease_key(&data, selected)), Some(&1));
        drop(run);
        released(&data, "fonts");
        assert!(!leases().lock().unwrap().contains_key(&lease_key(&data, selected)));
        for (name, bytes) in &original {
            assert_eq!(fs::read(target.join(name)).unwrap(), *bytes);
        }
        assert_eq!(fs::read(&sentinel).unwrap(), sentinel_bytes);
        json!({
            "schema":1,"owner":STATUS_VALIDATION_OWNER,"nonce":nonce,
            "fixtureFiles":original.len(),"readPath":"bin/engine",
            "readBytes":entered.bytes,"resourceBytes":expected_read_size,"readGateHits":1,
            "leaseElapsedMs":lease_elapsed_ms,"status":body,
            "actualOperate":true,"actualPositiveFileRead":true,
            "leaseAcquiredWhileReadPaused":true,"sameLeaseHeldThroughResponseAndJoin":true,
            "explicitGateResume":true,"statusJoined":true,
            "exclusiveBlockedWhileLeased":true,"exclusiveAvailableAfterRelease":true,
            "leaseMapCleared":true,"resourceBytesUnchanged":true,"sentinelUnchanged":true
        })
    }));
    // On failure this cancels, releases the gate, then joins, before the
    // original assertion resumes unwinding. No thread::scope join precedes it.
    drop(status);
    match outcome {
        Ok(receipt) => receipt,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

struct OwnedStatusChild(Child);
impl Drop for OwnedStatusChild {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

#[test]
fn actual_status_validation_admits_runtime_lease_while_reading() {
    // INSTALL is process-global: isolate this timing assertion from unrelated
    // parallel install tests, without running any capability executable.
    let root = tempfile::Builder::new()
        .prefix("readmd-status-validation-")
        .tempdir()
        .unwrap();
    let path = dunce::canonicalize(root.path()).unwrap();
    let nonce = uuid::Uuid::new_v4().to_string();
    fs::write(path.join("owner.marker"), nonce.as_bytes()).unwrap();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let stdout = fs::File::create(path.join("child.stdout")).unwrap();
        let stderr = fs::File::create(path.join("child.stderr")).unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut child = OwnedStatusChild(
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", STATUS_VALIDATION_CHILD, "--ignored", "--test-threads=1", "--nocapture"])
                .env("READMD_STATUS_VALIDATION_ROOT", &path)
                .env("READMD_STATUS_VALIDATION_NONCE", &nonce)
                .stdin(Stdio::null())
                .stdout(Stdio::from(stdout))
                .stderr(Stdio::from(stderr))
                .spawn()
                .unwrap(),
        );
        let exit = loop {
            let exited = child.0.try_wait().unwrap();
            assert!(Instant::now() < deadline, "owned status validation child exceeded 20s watchdog");
            if let Some(exit) = exited {
                break exit;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(exit.success(), "owned status validation child failed: {exit}");
        let bytes = fs::File::open(path.join("receipt.json"))
            .unwrap()
            .take(16385)
            .bytes()
            .collect::<io::Result<Vec<_>>>()
            .unwrap();
        assert!(bytes.len() <= 16384, "unexpectedly large child receipt");
        let receipt: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(receipt["schema"], 1);
        assert_eq!(receipt["owner"], STATUS_VALIDATION_OWNER);
        assert_eq!(receipt["nonce"], nonce);
        assert_eq!(receipt["fixtureFiles"], 3);
        assert_eq!(receipt["readPath"], "bin/engine");
        assert_eq!(receipt["readGateHits"], 1);
        assert!(receipt["readBytes"].as_u64().unwrap() > 0);
        assert!(receipt["readBytes"].as_u64().unwrap() <= receipt["resourceBytes"].as_u64().unwrap());
        assert!(receipt["leaseElapsedMs"].as_u64().unwrap() < 2000);
        for field in [
            "actualOperate", "actualPositiveFileRead", "leaseAcquiredWhileReadPaused",
            "sameLeaseHeldThroughResponseAndJoin", "explicitGateResume", "statusJoined",
            "exclusiveBlockedWhileLeased", "exclusiveAvailableAfterRelease", "leaseMapCleared",
            "resourceBytesUnchanged", "sentinelUnchanged",
        ] {
            assert_eq!(receipt[field], true, "missing complete child receipt: {field}");
        }
        for field in ["ok", "installed", "busy", "present"] {
            assert_eq!(receipt["status"][field], true);
        }
        assert_eq!(receipt["status"]["ready"], false);
        assert_eq!(receipt["status"]["busy_code"], "capability_install_busy");
        assert!(Instant::now() < deadline, "child receipt arrived after absolute watchdog");
        receipt
    }));
    match outcome {
        Ok(receipt) => {
            // The exact child was reaped and all its threads are gone before
            // deleting this owned directory. Cleanup failure cannot pass.
            let retained = root.keep();
            fs::remove_dir_all(&retained)
                .unwrap_or_else(|error| panic!("owned test cleanup failed; kept {}: {error}", retained.display()));
            println!("STATUS_VALIDATION_RESULT {receipt}");
        }
        Err(panic) => {
            let retained = root.keep();
            eprintln!("Owned status validation first failure retained at {}", retained.display());
            for name in ["child.stdout", "child.stderr"] {
                if let Ok(file) = fs::File::open(retained.join(name)) {
                    let mut bytes = Vec::new();
                    let _ = file.take(32768).read_to_end(&mut bytes);
                    eprintln!("{name}:\n{}", String::from_utf8_lossy(&bytes));
                }
            }
            std::panic::resume_unwind(panic);
        }
    }
}

#[test]
#[ignore = "Only the exact owned status-validation parent may invoke this child"]
fn actual_status_validation_runtime_lease_child() {
    let root = PathBuf::from(std::env::var_os("READMD_STATUS_VALIDATION_ROOT").expect("owned root"));
    let nonce = std::env::var("READMD_STATUS_VALIDATION_NONCE").expect("owned nonce");
    uuid::Uuid::parse_str(&nonce).expect("nonce must be a UUID");
    assert!(root.is_absolute());
    assert!(root.file_name().unwrap().to_string_lossy().starts_with("readmd-status-validation-"));
    assert_eq!(dunce::canonicalize(&root).unwrap(), root);
    assert_eq!(fs::read(root.join("owner.marker")).unwrap(), nonce.as_bytes());
    assert!(!root.join("receipt.json").exists());
    let receipt = actual_status_validation(&root, &nonce);
    fs::write(root.join("receipt.json"), serde_json::to_vec(&receipt).unwrap()).unwrap();
}
