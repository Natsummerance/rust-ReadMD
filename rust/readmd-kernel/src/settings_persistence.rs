//! Checked settings read/merge/publication. This independent lock never enters
//! the document gate. Success means the data and replacement were synchronized;
//! ordinary preference keys retain last-writer semantics. Recovery sessions use
//! a disk-backed version CAS and bounded idempotency receipts under this lock.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, io::{self, Read, Write}, path::Path, time::{Duration, Instant}};

const LOCK_NAME: &str = "settings-write.lock";
const WAIT: Duration = Duration::from_secs(3);
const MAX_SETTINGS_BYTES: u64 = 16 * 1024 * 1024;
const SESSION_VERSION: &str = "__readmd_session_version";
const SESSION_RECEIPTS: &str = "__readmd_session_receipts";
const MAX_SESSION_RECEIPTS: usize = 256;
const MAX_SESSION_RECEIPT_BYTES: usize = 256 * 1024;

fn invalid(message: &'static str) -> io::Error { io::Error::new(io::ErrorKind::InvalidData, message) }
fn plain(metadata: &fs::Metadata) -> bool {
    if !metadata.is_file() || metadata.file_type().is_symlink() { return false; }
    #[cfg(windows)] {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 { return false; }
    }
    true
}

#[derive(Debug)]
struct SettingsLock(fs::File);
impl SettingsLock {
    fn acquire(directory: &Path, wait: Duration) -> io::Result<Self> {
        let path = directory.join(LOCK_NAME);
        let started = Instant::now();
        loop {
            if let Ok(metadata) = fs::symlink_metadata(&path) {
                if !plain(&metadata) || metadata.len() != 0 { return Err(invalid("Invalid settings lock")); }
            }
            if let Some(file) = native::try_lock(&path)? {
                let metadata = file.metadata()?;
                if !plain(&metadata) || metadata.len() != 0 || !native::lock_matches(&file, &path)? {
                    return Err(invalid("Invalid settings lock"));
                }
                return Ok(Self(file));
            }
            if started.elapsed() >= wait {
                return Err(io::Error::new(io::ErrorKind::WouldBlock, "Settings writer busy"));
            }
            std::thread::sleep(Duration::from_millis(10).min(wait.saturating_sub(started.elapsed())));
        }
    }
}
impl Drop for SettingsLock {
    fn drop(&mut self) { native::unlock(&self.0); }
}

fn protected_patch(patch: &Value) -> bool {
    patch.as_object().is_some_and(|object| object.keys().any(|key|
        matches!(key.as_str(), "session"|"session_version"|"session_receipts"|"expected_session_version")
            || key == SESSION_VERSION || key == SESSION_RECEIPTS))
}

/// Shared by every generic Settings/Settings::set/App write, not only HTTP.
pub(crate) fn merge(path: &Path, patch: &Value, defaults: &Value) -> io::Result<Value> {
    if protected_patch(patch) { return Err(invalid("session_version_required")); }
    let mut locked = open_locked(path, defaults)?;
    let object = locked.current.as_object_mut().ok_or_else(|| invalid("Settings must be an object"))?;
    if let Some(patch) = patch.as_object() {
        for (key, value) in patch {
            if value.is_null() { object.remove(key); } else { object.insert(key.clone(), value.clone()); }
        }
    }
    locked.publish(&locked.current)?;
    Ok(locked.current)
}

pub(crate) fn generic_error_code(error: &io::Error) -> &'static str {
    if error.kind() == io::ErrorKind::InvalidData && error.to_string() == "session_version_required" {
        "session_version_required"
    } else { "settings_persistence_failed" }
}

struct LockedSettings {
    _lock: SettingsLock,
    directory: std::path::PathBuf,
    target: std::path::PathBuf,
    permissions: Option<fs::Permissions>,
    current: Value,
}

fn open_locked(path: &Path, defaults: &Value) -> io::Result<LockedSettings> {
    let parent = path.parent().filter(|path| !path.as_os_str().is_empty()).unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let directory = fs::canonicalize(parent)?;
    let name = path.file_name().ok_or_else(|| invalid("Invalid settings path"))?;
    let target = directory.join(name);
    // This lock covers the disk read, merge, temporary write and all syncs.
    let lock = SettingsLock::acquire(&directory, WAIT)?;
    let mut permissions = None;
    let current = match fs::symlink_metadata(&target) {
        Ok(metadata) => {
            if !plain(&metadata) { return Err(invalid("Settings target is not a plain file")); }
            if metadata.len() > MAX_SETTINGS_BYTES { return Err(invalid("Settings exceed storage limit")); }
            permissions = Some(metadata.permissions());
            let mut bytes = Vec::new();
            fs::File::open(&target)?.take(MAX_SETTINGS_BYTES + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_SETTINGS_BYTES { return Err(invalid("Settings exceed storage limit")); }
            serde_json::from_slice::<Value>(&bytes).map_err(|_| invalid("Invalid settings JSON"))?
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => defaults.clone(),
        Err(error) => return Err(error),
    };
    if !current.is_object() { return Err(invalid("Settings must be an object")); }
    Ok(LockedSettings {_lock:lock,directory,target,permissions,current})
}

impl LockedSettings {
fn publish(&self, current: &Value) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(&current).map_err(|_| invalid("Invalid settings value"))?;
    if bytes.len() as u64 > MAX_SETTINGS_BYTES { return Err(invalid("Settings exceed storage limit")); }
    let mut temporary = tempfile::Builder::new().prefix(".readmd-settings-").tempfile_in(&self.directory)?;
    temporary.write_all(&bytes)?;
    if let Some(permissions) = &self.permissions { temporary.as_file().set_permissions(permissions.clone())?; }
    #[cfg(test)] tests::fault(tests::Stage::BeforeFileSync)?;
    temporary.as_file().sync_all()?;
    #[cfg(test)] tests::fault(tests::Stage::BeforeReplace)?;
    // Close the synchronized temporary before the Windows replace. Never copy
    // into or truncate the destination as a fallback after a failed replace.
    let temporary = temporary.into_temp_path();
    native::replace(temporary.as_ref(), &self.target)?;
    #[cfg(test)] tests::fault(tests::Stage::BeforeDirectorySync)?;
    native::sync_directory(&self.directory)?;
    Ok(())
}
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionVersion {
    pub(crate) epoch: String,
    pub(crate) generation: String,
}
impl SessionVersion {
    fn parse(value: &Value) -> Result<Self, SessionError> {
        let version: Self = serde_json::from_value(value.clone()).map_err(|_|SessionError::invalid())?;
        let epoch=uuid::Uuid::parse_str(&version.epoch).map_err(|_|SessionError::invalid())?;
        let generation=version.generation.parse::<u64>().map_err(|_|SessionError::invalid())?;
        if epoch.to_string()!=version.epoch || generation.to_string()!=version.generation { return Err(SessionError::invalid()); }
        Ok(version)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionReceipt {
    call_id: String,
    fingerprint: String,
    expected_session_version: SessionVersion,
    session_version: SessionVersion,
}

/// Deliberately contains no disk path, configuration value or newer version.
#[derive(Clone, Debug)]
pub(crate) struct SessionError { code: &'static str, status: u16 }
impl SessionError {
    fn invalid() -> Self { Self {code:"session_version_invalid",status:422} }
    fn required() -> Self { Self {code:"session_version_required",status:428} }
    fn conflict() -> Self { Self {code:"session_version_conflict",status:409} }
    pub(crate) fn code(&self) -> &'static str {self.code}
    pub(crate) fn status(&self) -> u16 {self.status}
}
impl From<io::Error> for SessionError {
    fn from(_: io::Error) -> Self {Self {code:"settings_persistence_failed",status:507}}
}

pub(crate) struct SessionCommit {
    pub(crate) settings: Value,
    pub(crate) response: Value,
}

fn valid_call_id(value: &str) -> bool {
    !value.is_empty() && value.len()<=128 && value.bytes().all(|byte|byte.is_ascii_alphanumeric() || matches!(byte,b'_'|b'-'|b':'))
}
fn session_metadata(current: &Value) -> Result<Option<(SessionVersion,Vec<SessionReceipt>)>,SessionError> {
    let (version,receipts)=match (current.get(SESSION_VERSION),current.get(SESSION_RECEIPTS)) {
        (None,None)=>return Ok(None),
        (Some(version),Some(receipts))=>(SessionVersion::parse(version)?,receipts),
        _=>return Err(SessionError::invalid()),
    };
    let receipts:Vec<SessionReceipt>=serde_json::from_value(receipts.clone()).map_err(|_|SessionError::invalid())?;
    if receipts.len()>MAX_SESSION_RECEIPTS || serde_json::to_vec(&receipts).map_err(|_|SessionError::invalid())?.len()>MAX_SESSION_RECEIPT_BYTES {return Err(SessionError::invalid());}
    let mut ids=std::collections::HashSet::new();
    for receipt in &receipts {
        let expected=SessionVersion::parse(&json!(receipt.expected_session_version))?;
        let result=SessionVersion::parse(&json!(receipt.session_version))?;
        if !valid_call_id(&receipt.call_id) || !ids.insert(&receipt.call_id)
            || receipt.fingerprint.len()!=64 || !receipt.fingerprint.bytes().all(|byte|byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || expected.epoch!=version.epoch || result.epoch!=version.epoch
            || expected.generation.parse::<u64>().ok().and_then(|n|n.checked_add(1))!=result.generation.parse::<u64>().ok()
            || result.generation.parse::<u64>().map_err(|_|SessionError::invalid())?
                > version.generation.parse::<u64>().map_err(|_|SessionError::invalid())? {
            return Err(SessionError::invalid());
        }
    }
    Ok(Some((version,receipts)))
}

fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(object)=>{
            let mut keys=object.keys().collect::<Vec<_>>();keys.sort();
            let mut sorted=serde_json::Map::new();for key in keys {sorted.insert(key.clone(),canonical(&object[key]));}Value::Object(sorted)
        },
        Value::Array(array)=>Value::Array(array.iter().map(canonical).collect()),
        _=>value.clone(),
    }
}
fn session_fingerprint(expected: &SessionVersion, session: &Value) -> String {
    use sha2::Digest;
    format!("{:x}",sha2::Sha256::digest(canonical(&json!({"expected_session_version":expected,"session":session})).to_string().as_bytes()))
}
fn session_response(session: &Value, version: &SessionVersion, call_id: Option<&str>) -> Value {
    let mut response=json!({"ok":true,"protocol":"readmd-session-cas-v1","session":session,"session_version":version});
    if let Some(call_id)=call_id {response["persisted"]=json!(true);response["call_id"]=json!(call_id);}
    response
}

/// Reads the disk object and initializes only our version metadata. It never
/// substitutes the App cache or defaults for an invalid existing configuration.
pub(crate) fn read_session(path: &Path, defaults: &Value) -> Result<SessionCommit,SessionError> {
    let mut locked=open_locked(path,defaults)?;
    let version=match session_metadata(&locked.current)? {
        Some((version,_))=>version,
        None=>{
            let version=SessionVersion {epoch:uuid::Uuid::new_v4().to_string(),generation:"0".into()};
            locked.current[SESSION_VERSION]=json!(version);
            locked.current[SESSION_RECEIPTS]=json!([]);
            locked.publish(&locked.current)?;version
        },
    };
    let response=session_response(locked.current.get("session").unwrap_or(&Value::Null),&version,None);
    Ok(SessionCommit {settings:locked.current,response})
}

pub(crate) fn write_session(path: &Path, request: &Value, defaults: &Value) -> Result<SessionCommit,SessionError> {
    let expected=request.get("expected_session_version").filter(|value|!value.is_null()).ok_or_else(SessionError::required)?;
    let call_id=request.get("call_id").and_then(Value::as_str).filter(|id|valid_call_id(id)).ok_or_else(SessionError::required)?;
    let expected=SessionVersion::parse(expected)?;
    let session=request.get("session").ok_or_else(SessionError::invalid)?;
    if !session.is_null() && !session.is_object() {return Err(SessionError::invalid());}
    let fingerprint=session_fingerprint(&expected,session);
    let mut locked=open_locked(path,defaults)?;
    let (current,mut receipts)=session_metadata(&locked.current)?.ok_or_else(SessionError::conflict)?;
    if let Some(receipt)=receipts.iter().find(|receipt|receipt.call_id==call_id) {
        if receipt.fingerprint!=fingerprint || receipt.expected_session_version!=expected {
            return Err(SessionError {code:"session_request_conflict",status:409});
        }
        // A historical success must not certify that a superseded recovery
        // intent is still the current durable session.
        if receipt.session_version!=current {return Err(SessionError::conflict());}
        if session_fingerprint(&expected,locked.current.get("session").unwrap_or(&Value::Null))!=fingerprint {
            return Err(SessionError::invalid());
        }
        // Re-publish unchanged data to settle a previous post-replace sync
        // failure before returning a durable acknowledgement on replay.
        locked.publish(&locked.current)?;
        let response=session_response(session,&current,Some(call_id));
        return Ok(SessionCommit {settings:locked.current,response});
    }
    if expected!=current {return Err(SessionError::conflict());}
    let next=current.generation.parse::<u64>().map_err(|_|SessionError::invalid())?.checked_add(1).ok_or_else(SessionError::invalid)?;
    let version=SessionVersion {epoch:current.epoch,generation:next.to_string()};
    receipts.push(SessionReceipt {call_id:call_id.into(),fingerprint,expected_session_version:expected,session_version:version.clone()});
    while receipts.len()>MAX_SESSION_RECEIPTS || serde_json::to_vec(&receipts).map_err(|_|SessionError::invalid())?.len()>MAX_SESSION_RECEIPT_BYTES {receipts.remove(0);}
    locked.current["session"]=session.clone();
    locked.current[SESSION_VERSION]=json!(version);
    locked.current[SESSION_RECEIPTS]=json!(receipts);
    locked.publish(&locked.current)?;
    let response=session_response(session,&version,Some(call_id));
    Ok(SessionCommit {settings:locked.current,response})
}

#[cfg(unix)]
mod native {
    use super::*;
    use std::os::{fd::AsRawFd, unix::fs::{MetadataExt, OpenOptionsExt}};
    pub(super) fn try_lock(path: &Path) -> io::Result<Option<fs::File>> {
        let file = fs::OpenOptions::new().read(true).write(true).create(true).truncate(false)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC).open(path)?;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 { return Ok(Some(file)); }
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::WouldBlock { Ok(None) } else { Err(error) }
    }
    pub(super) fn lock_matches(file: &fs::File, path: &Path) -> io::Result<bool> {
        let opened = file.metadata()?;
        let named = fs::symlink_metadata(path)?;
        Ok(opened.nlink() == 1 && named.nlink() == 1 && plain(&named)
            && opened.dev() == named.dev() && opened.ino() == named.ino())
    }
    pub(super) fn unlock(file: &fs::File) { unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN); } }
    pub(super) fn replace(from: &Path, to: &Path) -> io::Result<()> { fs::rename(from, to) }
    pub(super) fn sync_directory(path: &Path) -> io::Result<()> { fs::File::open(path)?.sync_all() }
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::os::windows::{ffi::OsStrExt, fs::{MetadataExt, OpenOptionsExt}, io::AsRawHandle};
    use windows::{core::PCWSTR, Win32::{Foundation::HANDLE, Storage::FileSystem::{
        GetFileInformationByHandle, MoveFileExW, BY_HANDLE_FILE_INFORMATION,
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    }}};
    pub(super) fn try_lock(path: &Path) -> io::Result<Option<fs::File>> {
        match fs::OpenOptions::new().read(true).write(true).create(true).truncate(false)
            .share_mode(0).custom_flags(0x00200000).open(path) {
            Ok(file) => Ok(Some(file)),
            Err(error) if matches!(error.raw_os_error(), Some(32 | 33)) => Ok(None),
            Err(error) => Err(error),
        }
    }
    pub(super) fn lock_matches(file: &fs::File, _path: &Path) -> io::Result<bool> {
        let mut information = BY_HANDLE_FILE_INFORMATION::default();
        unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut information) }
            .map_err(|_| io::Error::last_os_error())?;
        Ok(information.nNumberOfLinks == 1 && file.metadata()?.file_attributes() & 0x400 == 0)
    }
    pub(super) fn unlock(_file: &fs::File) {}
    pub(super) fn replace(from: &Path, to: &Path) -> io::Result<()> {
        let from = from.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
        let to = to.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
        unsafe { MoveFileExW(PCWSTR(from.as_ptr()), PCWSTR(to.as_ptr()), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH) }
            .map_err(|_| io::Error::last_os_error())
    }
    // MoveFileExW uses WRITE_THROUGH; Windows read-only directory handles do
    // not support FlushFileBuffers. The temporary file was synchronized above.
    pub(super) fn sync_directory(_path: &Path) -> io::Result<()> { Ok(()) }
}

#[cfg(not(any(unix, windows)))]
mod native {
    use super::*;
    fn unsupported() -> io::Error { io::Error::new(io::ErrorKind::Unsupported, "Checked settings storage unsupported") }
    pub(super) fn try_lock(_path: &Path) -> io::Result<Option<fs::File>> { Err(unsupported()) }
    pub(super) fn lock_matches(_file: &fs::File, _path: &Path) -> io::Result<bool> { Err(unsupported()) }
    pub(super) fn unlock(_file: &fs::File) {}
    pub(super) fn replace(_from: &Path, _to: &Path) -> io::Result<()> { Err(unsupported()) }
    pub(super) fn sync_directory(_path: &Path) -> io::Result<()> { Err(unsupported()) }
}

#[cfg(test)]
#[path = "settings_persistence_tests.rs"]
mod tests;
