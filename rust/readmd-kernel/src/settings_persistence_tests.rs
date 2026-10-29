//! Synthetic storage tests. Native Windows cases are separate from simulated
//! sync faults; passing either does not substitute for the other.
use super::*;
use crate::settings::Settings;
use serde_json::json;
use std::{cell::Cell, path::PathBuf, process::{Command, Stdio}};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Stage { BeforeFileSync, BeforeReplace, BeforeDirectorySync }
thread_local! { static FAILURE: Cell<Option<Stage>> = const { Cell::new(None) }; }
pub(super) fn fault(stage: Stage) -> io::Result<()> {
    FAILURE.with(|failure| {
        if failure.get() == Some(stage) {
            failure.set(None);
            Err(io::Error::other("Injected settings persistence failure"))
        } else { Ok(()) }
    })
}
struct ResetFault;
impl Drop for ResetFault { fn drop(&mut self) { FAILURE.with(|failure| failure.set(None)); } }
fn fail(stage: Stage) -> ResetFault { FAILURE.with(|failure| failure.set(Some(stage))); ResetFault }
fn fixture() -> (tempfile::TempDir, PathBuf, Settings) {
    let root = tempfile::Builder::new().prefix("readmd-settings-probe-").tempdir().unwrap();
    let path = root.path().join("settings.json");
    fs::write(&path, br#"{"theme":"dark","keep":{"nested":true},"remove":"old"}"#).unwrap();
    let settings = Settings::load(&path);
    (root, path, settings)
}
fn disk(path: &Path) -> Value { serde_json::from_slice(&fs::read(path).unwrap()).unwrap() }
fn no_temporary(directory: &Path) {
    assert!(!fs::read_dir(directory).unwrap().any(|entry|
        entry.unwrap().file_name().to_string_lossy().starts_with(".readmd-settings-")));
}

#[test]
fn checked_existing_target_is_atomically_replaced_and_null_still_deletes() {
    let (root, path, mut settings) = fixture();
    let unrelated = path.with_extension("json.tmp");
    fs::write(&unrelated, b"UNKNOWN EXISTING FILE").unwrap();
    let preference = json!({"version":1,"synthetic":[{"pending":"synthetic-operation"}]});
    let committed = settings.merge_checked(&json!({"syntheticPreference":preference,"remove":null})).unwrap();
    assert_eq!(committed, disk(&path));
    assert_eq!(Settings::load(&path).get("syntheticPreference"), preference);
    assert_eq!(committed["theme"], "dark");
    assert_eq!(committed["keep"], json!({"nested":true}));
    assert!(committed.get("remove").is_none());
    assert_eq!(fs::read(&unrelated).unwrap(), b"UNKNOWN EXISTING FILE");
    assert_eq!(fs::metadata(root.path().join(LOCK_NAME)).unwrap().len(), 0);
    no_temporary(root.path());
}

#[test]
fn stale_settings_instances_merge_the_current_disk_object() {
    let (_root, path, mut first) = fixture();
    let mut stale = Settings::load(&path);
    first.merge_checked(&json!({"syntheticPreference":{"pending":"synthetic"}})).unwrap();
    stale.merge_checked(&json!({"fontSize":19})).unwrap();
    assert_eq!(disk(&path)["syntheticPreference"], json!({"pending":"synthetic"}));
    assert_eq!(disk(&path)["fontSize"], 19);
    // Ordinary same-field preferences retain their existing last-writer API.
    stale.merge_checked(&json!({"syntheticPreference":{"pending":"other-synthetic"}})).unwrap();
    assert_eq!(disk(&path)["syntheticPreference"], json!({"pending":"other-synthetic"}));
}

#[test]
fn app_checked_entry_point_propagates_failure_without_advancing_memory() {
    let root = tempfile::tempdir().unwrap();
    let assets = root.path().join("assets"); fs::create_dir(&assets).unwrap();
    let profile = root.path().join("profile");
    let app = crate::App::bootstrap(crate::paths::AppPaths::with_dirs(&profile, root.path(), &assets)).unwrap();
    let committed = app.update_settings_checked(&json!({"syntheticPreference":{"pending":"synthetic"}})).unwrap();
    let target = profile.join("settings.json");
    assert_eq!(disk(&target), committed);
    fs::remove_file(&target).unwrap(); fs::create_dir(&target).unwrap();
    assert!(app.update_settings_checked(&json!({"syntheticPreference":{"pending":"replacement"}})).is_err());
    assert_eq!(app.settings_all(), committed);
}

#[test]
fn checked_io_failure_keeps_memory_and_unknown_disk_objects() {
    let (root, path, mut settings) = fixture();
    let memory = settings.all();
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    let foreign = path.join("preserve.txt");
    fs::write(&foreign, b"FOREIGN SETTINGS TARGET").unwrap();
    assert!(settings.merge_checked(&json!({"syntheticPreference":{"pending":"synthetic"}})).is_err());
    assert_eq!(settings.all(), memory);
    assert_eq!(fs::read(&foreign).unwrap(), b"FOREIGN SETTINGS TARGET");
    // The legacy Value-returning entry point also never claims new memory.
    assert_eq!(settings.merge(&json!({"theme":"light"})), memory);
    assert_eq!(fs::read(&foreign).unwrap(), b"FOREIGN SETTINGS TARGET");
    no_temporary(root.path());

    let blocked = root.path().join("parent-is-a-file");
    fs::write(&blocked, b"OWNED BLOCKER").unwrap();
    let mut missing = Settings::load(&blocked.join("settings.json"));
    assert!(missing.merge_checked(&json!({"syntheticPreference":{}})).is_err());
    assert_eq!(fs::read(&blocked).unwrap(), b"OWNED BLOCKER");
}

#[test]
fn checked_bad_lock_and_invalid_json_never_destroy_existing_settings() {
    let (root, path, mut settings) = fixture();
    let before = fs::read(&path).unwrap();
    let lock = root.path().join(LOCK_NAME);
    fs::write(&lock, b"FOREIGN NONEMPTY LOCK").unwrap();
    assert!(settings.merge_checked(&json!({"syntheticPreference":{}})).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::read(&lock).unwrap(), b"FOREIGN NONEMPTY LOCK");
    fs::remove_file(&lock).unwrap();
    fs::write(&path, b"{INVALID OWNED JSON").unwrap();
    let memory = settings.all();
    assert!(settings.merge_checked(&json!({"syntheticPreference":{}})).is_err());
    assert_eq!(settings.all(), memory);
    assert_eq!(fs::read(&path).unwrap(), b"{INVALID OWNED JSON");
}

#[test]
fn checked_simulated_sync_or_replace_failure_never_returns_success() {
    for stage in [Stage::BeforeFileSync, Stage::BeforeReplace, Stage::BeforeDirectorySync] {
        let (root, path, mut settings) = fixture();
        let before = fs::read(&path).unwrap();
        let memory = settings.all();
        let _fault = fail(stage);
        assert!(settings.merge_checked(&json!({"syntheticPreference":{"pending":"synthetic"}})).is_err());
        assert_eq!(settings.all(), memory);
        if stage != Stage::BeforeDirectorySync {
            assert_eq!(fs::read(&path).unwrap(), before);
        } else {
            // Replacement may already have happened. A failed final sync is
            // uncertain persistence, not a successful durability receipt.
            assert_eq!(disk(&path)["syntheticPreference"], json!({"pending":"synthetic"}));
        }
        no_temporary(root.path());
        settings.merge_checked(&json!({"retry":true})).unwrap();
        assert_eq!(disk(&path)["retry"], true);
    }
}

#[cfg(windows)]
#[test]
fn checked_windows_locked_destination_preserves_old_bytes_and_can_retry() {
    use std::os::windows::fs::OpenOptionsExt;
    let (root, path, mut settings) = fixture();
    let before = fs::read(&path).unwrap();
    let memory = settings.all();
    // Read sharing allows inspection but denies WRITE and DELETE/replace.
    let held = fs::OpenOptions::new().read(true).share_mode(1).open(&path).unwrap();
    assert!(settings.merge_checked(&json!({"syntheticPreference":{"pending":"synthetic"}})).is_err());
    assert_eq!(settings.all(), memory);
    assert_eq!(fs::read(&path).unwrap(), before);
    no_temporary(root.path());
    drop(held);
    settings.merge_checked(&json!({"syntheticPreference":{"pending":"synthetic"}})).unwrap();
    assert_eq!(disk(&path)["syntheticPreference"], json!({"pending":"synthetic"}));
}

#[test]
fn settings_lock_excludes_a_real_second_process_and_releases_on_drop() {
    let (root, _path, _settings) = fixture();
    let token = uuid::Uuid::new_v4().to_string();
    fs::write(root.path().join("probe.marker"), token.as_bytes()).unwrap();
    let held = SettingsLock::acquire(root.path(), Duration::ZERO).unwrap();
    probe(root.path(), &token, "busy");
    drop(held);
    probe(root.path(), &token, "available");
}

fn probe(root: &Path, token: &str, expected: &str) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .arg("--exact").arg("settings_persistence::tests::settings_lock_probe")
        .arg("--ignored").arg("--nocapture")
        .env("READMD_SETTINGS_PROBE_ROOT", root).env("READMD_SETTINGS_PROBE_TOKEN", token)
        .env("READMD_SETTINGS_PROBE_EXPECTED", expected)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() { break status; }
        if start.elapsed() > Duration::from_secs(10) {
            let _ = child.kill(); let _ = child.wait(); panic!("Synthetic settings lock probe timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(status.success(), "Synthetic settings lock probe failed");
}

#[test]
#[ignore = "Spawned only by the owned synthetic settings-lock test"]
fn settings_lock_probe() {
    let Some(root) = std::env::var_os("READMD_SETTINGS_PROBE_ROOT").map(PathBuf::from) else { return; };
    let token = std::env::var("READMD_SETTINGS_PROBE_TOKEN").unwrap();
    assert!(root.file_name().unwrap().to_string_lossy().starts_with("readmd-settings-probe-"));
    assert_eq!(fs::read(root.join("probe.marker")).unwrap(), token.as_bytes());
    match std::env::var("READMD_SETTINGS_PROBE_EXPECTED").unwrap().as_str() {
        "busy" => assert_eq!(SettingsLock::acquire(&root, Duration::ZERO).unwrap_err().kind(), io::ErrorKind::WouldBlock),
        "available" => { SettingsLock::acquire(&root, Duration::ZERO).unwrap(); },
        _ => panic!("Invalid synthetic probe expectation"),
    }
}

fn session_request(version: &Value, call_id: &str, session: Value) -> Value {
    json!({"session":session,"expected_session_version":version,"call_id":call_id})
}
fn session_error(result: Result<Value, SessionError>, status: u16, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.status(), status);
    assert_eq!(error.code(), code);
}

#[test]
fn session_migration_preserves_legacy_session_and_every_unknown_setting() {
    let (root, path, mut settings) = fixture();
    let legacy = json!({"theme":"custom","unknown":{"array":[1,null,{"nested":true}]},
        "session":{"version":1,"tabs":[{"renamePending":{"operation_id":"synthetic-intent"}}]}});
    fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    // Settings was loaded before this disk change; migration must not use it.
    let response = settings.read_session_checked().unwrap();
    assert_eq!(response["protocol"],"readmd-session-cas-v1");
    let version = &response["session_version"];
    assert_eq!(version["generation"], "0");
    assert!(uuid::Uuid::parse_str(version["epoch"].as_str().unwrap()).is_ok());
    assert_eq!(response["session"], legacy["session"]);
    let persisted = disk(&path);
    for (key, value) in legacy.as_object().unwrap() { assert_eq!(persisted[key], *value); }
    assert_eq!(persisted[SESSION_VERSION], *version);
    assert_eq!(persisted[SESSION_RECEIPTS], json!([]));
    assert_eq!(settings.read_session_checked().unwrap(), response);
    assert_eq!(settings.all(), persisted);
    assert_eq!(fs::metadata(root.path().join(LOCK_NAME)).unwrap().len(), 0);
    no_temporary(root.path());
}

#[test]
fn session_stale_instances_cannot_erase_a_durable_recovery_intent() {
    let (_root, path, mut first) = fixture();
    let mut stale = Settings::load(&path);
    let original = first.read_session_checked().unwrap();
    let stale_read = stale.read_session_checked().unwrap();
    assert_eq!(original, stale_read);
    let saved = first.write_session_checked(&session_request(&original["session_version"], "intent-save",
        json!({"tabs":[{"renamePending":{"operation_id":"synthetic-intent"}}]}))).unwrap();
    let before = fs::read(&path).unwrap();
    session_error(stale.write_session_checked(&session_request(&stale_read["session_version"], "stale-save",
        json!({"tabs":[]}))),409,"session_version_conflict");
    assert_eq!(fs::read(&path).unwrap(),before);
    // Ordinary preferences still merge the newest disk and preserve its CAS.
    stale.merge_checked(&json!({"fontSize":19})).unwrap();
    assert_eq!(disk(&path)["session"],saved["session"]);
    assert_eq!(disk(&path)[SESSION_VERSION],saved["session_version"]);
    assert_eq!(stale.read_session_checked().unwrap()["session"],saved["session"]);
}

#[test]
fn session_generation_and_epoch_prevent_aba() {
    let (_root, path, mut settings) = fixture();
    let initial=settings.read_session_checked().unwrap();
    let a=json!({"tabs":[{"synthetic":"A"}]});
    let first=settings.write_session_checked(&session_request(&initial["session_version"],"aba-a",a.clone())).unwrap();
    let second=settings.write_session_checked(&session_request(&first["session_version"],"aba-b",json!({"tabs":[]}))).unwrap();
    let third=settings.write_session_checked(&session_request(&second["session_version"],"aba-a-again",a.clone())).unwrap();
    assert_eq!(third["session"],first["session"]);
    assert_eq!(third["session_version"]["generation"],"3");
    session_error(settings.write_session_checked(&session_request(&first["session_version"],"aba-stale",a.clone())),409,"session_version_conflict");
    fs::remove_file(&path).unwrap();
    let replacement=settings.read_session_checked().unwrap();
    assert_eq!(replacement["session_version"]["generation"],"0");
    assert_ne!(replacement["session_version"]["epoch"],initial["session_version"]["epoch"]);
    session_error(settings.write_session_checked(&session_request(&initial["session_version"],"old-epoch",a)),409,"session_version_conflict");
}

#[test]
fn session_replay_only_acknowledges_the_current_commit_and_rejects_id_reuse() {
    let (_root,path,mut settings)=fixture();
    let initial=settings.read_session_checked().unwrap();
    let request=session_request(&initial["session_version"],"same-call",json!({"nested":{"b":2,"a":1}}));
    let first=settings.write_session_checked(&request).unwrap();
    assert_eq!(first["protocol"],"readmd-session-cas-v1");
    let reordered=session_request(&initial["session_version"],"same-call",json!({"nested":{"a":1,"b":2}}));
    assert_eq!(settings.write_session_checked(&reordered).unwrap(),first);
    assert_eq!(disk(&path)[SESSION_RECEIPTS].as_array().unwrap().len(),1);
    session_error(settings.write_session_checked(&session_request(&initial["session_version"],"same-call",json!({"nested":{}}))),409,"session_request_conflict");
    session_error(settings.write_session_checked(&session_request(&first["session_version"],"same-call",request["session"].clone())),409,"session_request_conflict");
    settings.write_session_checked(&session_request(&first["session_version"],"next-call",json!({"tabs":[]}))).unwrap();
    let before=fs::read(&path).unwrap();
    session_error(settings.write_session_checked(&request),409,"session_version_conflict");
    assert_eq!(fs::read(&path).unwrap(),before);
}

#[test]
fn session_receipts_are_bounded_and_evicted_requests_conflict() {
    let (_root,path,mut settings)=fixture();
    let initial=settings.read_session_checked().unwrap();
    let oldest=session_request(&initial["session_version"],"bounded-0",json!({"n":0}));
    let mut response=settings.write_session_checked(&oldest).unwrap();
    for n in 1..=MAX_SESSION_RECEIPTS {
        response=settings.write_session_checked(&session_request(&response["session_version"],&format!("bounded-{n}"),json!({"n":n}))).unwrap();
    }
    let persisted=disk(&path);
    let receipts=persisted[SESSION_RECEIPTS].as_array().unwrap();
    assert_eq!(receipts.len(),MAX_SESSION_RECEIPTS);
    assert_eq!(receipts.first().unwrap()["call_id"],"bounded-1");
    assert!(serde_json::to_vec(receipts).unwrap().len()<=MAX_SESSION_RECEIPT_BYTES);
    let before=fs::read(&path).unwrap();
    session_error(settings.write_session_checked(&oldest),409,"session_version_conflict");
    assert_eq!(fs::read(&path).unwrap(),before);
    assert_eq!(response["session_version"]["generation"],(MAX_SESSION_RECEIPTS+1).to_string());
}

#[test]
fn clearing_session_advances_version_without_removing_metadata() {
    let (_root,path,mut settings)=fixture();
    let initial=settings.read_session_checked().unwrap();
    let response=settings.write_session_checked(&session_request(&initial["session_version"],"clear-session",Value::Null)).unwrap();
    assert_eq!(response["session"],Value::Null);
    assert_eq!(response["session_version"]["generation"],"1");
    assert!(disk(&path).get("session").unwrap().is_null());
    assert_eq!(disk(&path)[SESSION_VERSION],response["session_version"]);
    assert_eq!(settings.read_session_checked().unwrap()["session_version"],response["session_version"]);
}

#[test]
fn generic_settings_paths_cannot_modify_session_or_reserved_fields() {
    let (_root,path,mut settings)=fixture();
    let initialized=settings.read_session_checked().unwrap();
    let before=fs::read(&path).unwrap();
    let memory=settings.all();
    for key in ["session","session_version","session_receipts","expected_session_version",SESSION_VERSION,SESSION_RECEIPTS] {
        for value in [Value::Null,json!({"synthetic":"bypass"})] {
            let mut patch=json!({"theme":"must-not-commit"});patch[key]=value.clone();
            let error=settings.merge_checked(&patch).unwrap_err();
            assert_eq!(generic_error_code(&error),"session_version_required");
            assert_eq!(settings.merge(&patch),memory);
            assert_eq!(settings.set(key,value),memory);
            assert_eq!(settings.all(),memory);
            assert_eq!(fs::read(&path).unwrap(),before);
        }
    }
    settings.merge_checked(&json!({"theme":"light"})).unwrap();
    assert_eq!(disk(&path)[SESSION_VERSION],initialized["session_version"]);
}

#[test]
fn session_missing_or_invalid_preconditions_do_not_touch_disk() {
    let (_root,path,mut settings)=fixture();
    let version=settings.read_session_checked().unwrap()["session_version"].clone();
    let before=fs::read(&path).unwrap();
    for request in [json!({}),json!({"session":{},"call_id":"missing"}),
        json!({"session":{},"call_id":"null","expected_session_version":null}),
        json!({"session":{},"expected_session_version":version}),
        json!({"session":{},"call_id":"bad id","expected_session_version":version})] {
        session_error(settings.write_session_checked(&request),428,"session_version_required");
    }
    for bad in [json!({}),json!({"epoch":"invalid","generation":"0"}),
        json!({"epoch":version["epoch"],"generation":0}),
        json!({"epoch":version["epoch"],"generation":"01"}),
        json!({"epoch":version["epoch"],"generation":"18446744073709551616"}),
        json!({"epoch":version["epoch"],"generation":"0","unknown":true})] {
        session_error(settings.write_session_checked(&session_request(&bad,"invalid-version",json!({}))),422,"session_version_invalid");
    }
    session_error(settings.write_session_checked(&session_request(&version,"invalid-session",json!([]))),422,"session_version_invalid");
    assert_eq!(fs::read(&path).unwrap(),before);
}

#[test]
fn session_invalid_metadata_and_generation_exhaustion_never_reset_data() {
    for metadata in [json!({"epoch":"invalid","generation":"0"}),
        json!({"epoch":uuid::Uuid::new_v4().to_string(),"generation":u64::MAX.to_string()})] {
        let (_root,path,mut settings)=fixture();
        let mut data=disk(&path);data[SESSION_VERSION]=metadata.clone();data[SESSION_RECEIPTS]=json!([]);
        fs::write(&path,serde_json::to_vec(&data).unwrap()).unwrap();
        let before=fs::read(&path).unwrap();let memory=settings.all();
        if metadata["epoch"]=="invalid" {
            session_error(settings.read_session_checked(),422,"session_version_invalid");
        } else {
            session_error(settings.write_session_checked(&session_request(&metadata,"overflow",json!({}))),422,"session_version_invalid");
        }
        assert_eq!(settings.all(),memory);assert_eq!(fs::read(&path).unwrap(),before);
    }
    let (_root,path,mut settings)=fixture();
    let mut data=disk(&path);data[SESSION_RECEIPTS]=json!([]);
    fs::write(&path,serde_json::to_vec(&data).unwrap()).unwrap();let before=fs::read(&path).unwrap();
    session_error(settings.read_session_checked(),422,"session_version_invalid");
    assert_eq!(fs::read(&path).unwrap(),before);
}

#[test]
fn session_migration_sync_failures_preserve_memory_and_unknown_data() {
    for stage in [Stage::BeforeFileSync,Stage::BeforeReplace,Stage::BeforeDirectorySync] {
        let (root,path,mut settings)=fixture();let before=fs::read(&path).unwrap();let memory=settings.all();
        let _fault=fail(stage);
        session_error(settings.read_session_checked(),507,"settings_persistence_failed");
        assert_eq!(settings.all(),memory);
        if stage!=Stage::BeforeDirectorySync {assert_eq!(fs::read(&path).unwrap(),before);}
        assert_eq!(disk(&path)["keep"],json!({"nested":true}));
        let retry=settings.read_session_checked().unwrap();
        assert_eq!(retry["session_version"]["generation"],"0");no_temporary(root.path());
    }
}

#[test]
fn session_failed_or_uncertain_commit_can_retry_same_call_without_double_advance() {
    for stage in [Stage::BeforeFileSync,Stage::BeforeReplace,Stage::BeforeDirectorySync] {
        let (root,path,mut settings)=fixture();
        let initial=settings.read_session_checked().unwrap();
        let request=session_request(&initial["session_version"],"uncertain-call",json!({"pending":"synthetic"}));
        let before=fs::read(&path).unwrap();let memory=settings.all();let _fault=fail(stage);
        session_error(settings.write_session_checked(&request),507,"settings_persistence_failed");
        assert_eq!(settings.all(),memory);
        if stage!=Stage::BeforeDirectorySync {assert_eq!(fs::read(&path).unwrap(),before);}
        else {assert_eq!(disk(&path)[SESSION_VERSION]["generation"],"1");}
        let retried=settings.write_session_checked(&request).unwrap();
        assert_eq!(retried["session_version"]["generation"],"1");
        assert_eq!(disk(&path)[SESSION_RECEIPTS].as_array().unwrap().len(),1);
        assert_eq!(settings.write_session_checked(&request).unwrap(),retried);
        no_temporary(root.path());
    }
}

#[cfg(windows)]
#[test]
fn session_windows_denied_replace_cannot_acknowledge_or_advance_memory() {
    use std::os::windows::fs::OpenOptionsExt;
    let (_root,path,mut settings)=fixture();
    let initial=settings.read_session_checked().unwrap();let memory=settings.all();let before=fs::read(&path).unwrap();
    let request=session_request(&initial["session_version"],"denied-replace",json!({"pending":"synthetic"}));
    let held=fs::OpenOptions::new().read(true).share_mode(1).open(&path).unwrap();
    session_error(settings.write_session_checked(&request),507,"settings_persistence_failed");
    assert_eq!(settings.all(),memory);assert_eq!(fs::read(&path).unwrap(),before);drop(held);
    assert_eq!(settings.write_session_checked(&request).unwrap()["session_version"]["generation"],"1");
}

#[test]
fn session_two_processes_with_one_expected_version_have_exactly_one_winner() {
    let (root,path,mut settings)=fixture();let initial=settings.read_session_checked().unwrap();
    let token=uuid::Uuid::new_v4().to_string();fs::write(root.path().join("probe.marker"),token.as_bytes()).unwrap();
    fs::write(root.path().join("cas-version.json"),serde_json::to_vec(&initial["session_version"]).unwrap()).unwrap();
    let children=["a","b"].into_iter().map(|id| {
        Command::new(std::env::current_exe().unwrap()).arg("--exact")
            .arg("settings_persistence::tests::session_cas_probe").arg("--ignored").arg("--nocapture")
            .env("READMD_SETTINGS_PROBE_ROOT",root.path()).env("READMD_SETTINGS_PROBE_TOKEN",&token)
            .env("READMD_SETTINGS_PROBE_ID",id).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap()
    }).collect::<Vec<_>>();
    for mut child in children {
        let start=Instant::now();loop {
            if let Some(status)=child.try_wait().unwrap() {assert!(status.success(),"Synthetic session CAS child failed");break;}
            if start.elapsed()>Duration::from_secs(10) {let _=child.kill();let _=child.wait();panic!("Synthetic session CAS child timed out");}
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    let mut outcomes=["a","b"].map(|id|fs::read_to_string(root.path().join(format!("cas-{id}.result"))).unwrap());
    outcomes.sort();assert_eq!(outcomes,["200","409"]);
    let persisted=disk(&path);assert_eq!(persisted[SESSION_VERSION]["generation"],"1");
    assert_eq!(persisted[SESSION_RECEIPTS].as_array().unwrap().len(),1);
    assert_eq!(persisted["keep"],json!({"nested":true}));
}

#[test]
#[ignore = "Spawned only by the owned synthetic session CAS test"]
fn session_cas_probe() {
    let Some(root)=std::env::var_os("READMD_SETTINGS_PROBE_ROOT").map(PathBuf::from) else {return;};
    assert!(root.file_name().unwrap().to_string_lossy().starts_with("readmd-settings-probe-"));
    assert_eq!(fs::read(root.join("probe.marker")).unwrap(),std::env::var("READMD_SETTINGS_PROBE_TOKEN").unwrap().as_bytes());
    let id=std::env::var("READMD_SETTINGS_PROBE_ID").unwrap();assert!(matches!(id.as_str(),"a"|"b"));
    let version=disk(&root.join("cas-version.json"));let mut settings=Settings::load(&root.join("settings.json"));
    let result=settings.write_session_checked(&session_request(&version,&format!("child-{id}"),json!({"writer":id})));
    let status=match result {Ok(_)=>200,Err(error)=>{assert_eq!(error.code(),"session_version_conflict");error.status()}};
    fs::write(root.join(format!("cas-{id}.result")),status.to_string()).unwrap();
}
