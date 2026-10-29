//! Filesystem primitives for the future document-operation protocol.
//!
//! NOT a rename service or an authorization grant. Callers must first take
//! SAVE_LOCK, then this profile gate, then any authorization/review transaction.
//! Never hold the gate across UI, network, AI, conversion or other long work.
//! Only participating processes using the same data directory are coordinated.
//! External writers can still race a pathname rename: no-replace protects the
//! destination, but observation/hash checking is not an inode/content CAS.
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    ffi::{OsStr, OsString},
    fmt, fs,
    io::{self, Read},
    marker::PhantomData,
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

#[cfg(windows)]
#[path = "document_operations/windows.rs"]
mod native;
#[cfg(windows)]
#[path = "document_operations/refactor_windows.rs"]
pub(crate) mod refactor_windows;
#[cfg(windows)]
#[path = "document_operations/refactor_path_checks.rs"]
mod refactor_path_checks;
#[cfg(all(unix, any(target_os = "linux", target_os = "macos")))]
#[path = "document_operations/unix.rs"]
mod native;
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
#[path = "document_operations/unsupported.rs"]
mod native;

pub const LOCK_FILE_NAME: &str = "document-operations.lock";
pub const MAX_LOCK_WAIT: Duration = Duration::from_secs(3);
pub const MAX_CAPTURE_BYTES: u64 = 32 * 1024 * 1024;

pub(crate) fn validate_directory(path: &Path) -> Result<PathBuf> {
    let directory = native::Directory::open(path).map_err(Error::io)?;
    #[cfg(windows)]
    directory.verify_case_mode().map_err(Error::io)?;
    directory.revalidate().map_err(Error::io)?;
    Ok(directory.path().to_path_buf())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Busy,
    InvalidPath,
    InvalidLock,
    Unsupported,
    Missing,
    IdentityChanged,
    RevisionChanged,
    TargetExists,
    Storage,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveState {
    /// No move was attempted, or an unsupported primitive refused before moving.
    NotAttempted,
    /// A native call returned an error; observe both names, never guess/rollback.
    Unknown,
    /// The native call succeeded; later verification/flush failed.
    Moved,
}
#[derive(Debug)]
pub struct Error {
    pub kind: ErrorKind,
    pub move_state: MoveState,
    cause: Option<io::Error>,
}
pub type Result<T> = std::result::Result<T, Error>;
impl Error {
    fn new(kind: ErrorKind) -> Self {
        Self {
            kind,
            move_state: MoveState::NotAttempted,
            cause: None,
        }
    }
    fn io(error: io::Error) -> Self {
        let kind = if error.kind() == io::ErrorKind::Unsupported {
            ErrorKind::Unsupported
        } else {
            ErrorKind::Storage
        };
        Self {
            kind,
            move_state: MoveState::NotAttempted,
            cause: Some(error),
        }
    }
    fn after_move(mut self) -> Self {
        self.move_state = MoveState::Moved;
        self
    }
    pub fn code(&self) -> &'static str {
        match self.kind {
            ErrorKind::Busy => "document_operation_busy",
            ErrorKind::InvalidPath => "document_operation_path",
            ErrorKind::InvalidLock => "document_operation_lock",
            ErrorKind::Unsupported => "document_operation_unsupported",
            ErrorKind::Missing => "document_operation_missing",
            ErrorKind::IdentityChanged => "document_operation_identity_changed",
            ErrorKind::RevisionChanged => "document_operation_revision_changed",
            ErrorKind::TargetExists => "document_operation_target_exists",
            ErrorKind::Storage => "document_operation_storage",
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause.as_ref().map(|error| error as _)
    }
}

/// Persist as a string using as_str()/parse(); it is NOT an access credential.
/// v1: platform, volume/dev (16 hex), file ID (32 hex), birth (32 hex).
/// Windows birth is FILETIME ticks; Unix birth is nanoseconds since Unix epoch.
/// Strong identity is refused when birth/file-ID support is unavailable. IDs
/// remain subject to the filesystem's recycling/persistence guarantees.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FileIdentity(String);
impl FileIdentity {
    fn native(platform: &str, volume: u64, index: u128, birth: u128) -> Self {
        Self(format!(
            "readmd-file-v1:{platform}:{volume:016x}:{index:032x}:{birth:032x}"
        ))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn parse(value: &str) -> Result<Self> {
        if value.len() > 128 {
            return Err(Error::new(ErrorKind::Unsupported));
        }
        let fields = value.split(':').collect::<Vec<_>>();
        if fields.len() != 5
            || fields[0] != "readmd-file-v1"
            || !matches!(fields[1], "windows" | "unix")
        {
            return Err(Error::new(ErrorKind::Unsupported));
        }
        for (field, width) in fields[2..].iter().zip([16, 32, 32]) {
            if field.len() != width
                || !field
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(Error::new(ErrorKind::Unsupported));
            }
        }
        if fields[3].bytes().all(|byte| byte == b'0')
            || (fields[1] == "windows" && fields[4].bytes().all(|byte| byte == b'0'))
        {
            return Err(Error::new(ErrorKind::Unsupported));
        }
        Ok(Self(value.to_owned()))
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileFingerprint {
    pub identity: FileIdentity,
    /// Lower-case SHA-256 of raw bytes, before decoding/normalization.
    pub revision: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedReason {
    UnsafePath,
    NotRegularFile,
    MultipleLinks,
    IdentityUnavailable,
    TooLarge,
}
#[derive(Debug)]
pub enum Observation {
    Missing,
    Regular(ObservedFile),
    Unsupported(UnsupportedReason),
}
/// Lightweight identity observation for existing-file authorization. Unlike
/// rename capture this reads no body, has no size limit and allows hard links.
/// Each authorization must still bind one exact path and its content revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdentityObservation {
    Missing,
    Regular(FileIdentity),
    Unsupported(UnsupportedReason),
}

pub struct ObservedFile {
    parent: native::Directory,
    name: OsString,
    _file: fs::File,
    fingerprint: FileFingerprint,
    bytes: Vec<u8>,
}
impl fmt::Debug for ObservedFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ObservedFile")
            .field("fingerprint", &self.fingerprint)
            .field("bytes", &self.bytes.len())
            .finish_non_exhaustive()
    }
}
impl ObservedFile {
    pub fn fingerprint(&self) -> &FileFingerprint {
        &self.fingerprint
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Info {
    identity: FileIdentity,
    length: u64,
    modified: u128,
    changed: u128,
    links: u64,
    regular: bool,
    redirected: bool,
}
fn unsafe_path() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "Unsafe document path")
}
fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "Native document identity/no-replace unavailable",
    )
}
fn lock_io(error: io::Error) -> Error {
    let invalid = matches!(
        error.kind(),
        io::ErrorKind::InvalidData | io::ErrorKind::NotFound
    );
    let mut result = Error::io(error);
    if invalid {
        result.kind = ErrorKind::InvalidLock;
    }
    result
}
fn leaf(name: &OsStr) -> Result<()> {
    let mut parts = Path::new(name).components();
    if !matches!(parts.next(), Some(Component::Normal(_)))
        || parts.next().is_some()
        || !native::valid_leaf(name)
    {
        return Err(Error::new(ErrorKind::InvalidPath));
    }
    Ok(())
}
fn observation_io(error: io::Error) -> Result<Observation> {
    match error.kind() {
        io::ErrorKind::InvalidData => Ok(Observation::Unsupported(UnsupportedReason::UnsafePath)),
        io::ErrorKind::Unsupported => Ok(Observation::Unsupported(
            UnsupportedReason::IdentityUnavailable,
        )),
        _ => Err(Error::io(error)),
    }
}
fn capture(parent: native::Directory, name: OsString, maximum_bytes: u64) -> Result<Observation> {
    let mut file = match parent.open_file(&name) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            parent.revalidate().map_err(Error::io)?;
            return Ok(Observation::Missing);
        }
        Err(error) => return observation_io(error),
    };
    let before = match native::info(&file) {
        Ok(info) => info,
        Err(error) => return observation_io(error),
    };
    if before.redirected {
        return Ok(Observation::Unsupported(UnsupportedReason::UnsafePath));
    }
    if !before.regular {
        return Ok(Observation::Unsupported(UnsupportedReason::NotRegularFile));
    }
    if before.links != 1 {
        return Ok(Observation::Unsupported(UnsupportedReason::MultipleLinks));
    }
    let maximum_bytes = maximum_bytes.min(MAX_CAPTURE_BYTES);
    if before.length > maximum_bytes {
        return Ok(Observation::Unsupported(UnsupportedReason::TooLarge));
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(maximum_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(Error::io)?;
    if bytes.len() as u64 > maximum_bytes {
        return Ok(Observation::Unsupported(UnsupportedReason::TooLarge));
    }
    let after = native::info(&file).map_err(Error::io)?;
    if before != after || after.length != bytes.len() as u64 {
        return Err(Error::new(ErrorKind::RevisionChanged));
    }
    parent.revalidate().map_err(Error::io)?;
    let named = parent
        .open_file(&name)
        .and_then(|file| native::info(&file))
        .map_err(Error::io)?;
    if named != after {
        return Err(Error::new(ErrorKind::IdentityChanged));
    }
    let fingerprint = FileFingerprint {
        identity: after.identity,
        revision: format!("{:x}", Sha256::digest(&bytes)),
    };
    Ok(Observation::Regular(ObservedFile {
        parent,
        name,
        _file: file,
        fingerprint,
        bytes,
    }))
}
/// Missing means only an absent leaf in an existing, verified directory.
/// Missing/redirected/unreadable parents never become permission to create.
pub fn observe(path: &Path, maximum_bytes: u64) -> Result<Observation> {
    if !path.is_absolute() {
        return Err(Error::new(ErrorKind::InvalidPath));
    }
    let name = path
        .file_name()
        .ok_or_else(|| Error::new(ErrorKind::InvalidPath))?;
    leaf(name)?;
    let parent = match native::Directory::open(
        path.parent()
            .ok_or_else(|| Error::new(ErrorKind::InvalidPath))?,
    ) {
        Ok(parent) => parent,
        Err(error) => return observation_io(error),
    };
    capture(parent, name.to_owned(), maximum_bytes)
}

fn identity_io(error: io::Error) -> Result<IdentityObservation> {
    match error.kind() {
        io::ErrorKind::InvalidData => Ok(IdentityObservation::Unsupported(
            UnsupportedReason::UnsafePath,
        )),
        io::ErrorKind::Unsupported => Ok(IdentityObservation::Unsupported(
            UnsupportedReason::IdentityUnavailable,
        )),
        _ => Err(Error::io(error)),
    }
}
/// No body is read or hashed. Missing has the same leaf-only meaning as observe.
/// Unsupported filesystems must not be represented by a fabricated identity.
/// Callers choose an explicit compatibility policy for ordinary open/save;
/// unsupported identity must never enable the strict rename protocol.
pub fn observe_identity(path: &Path) -> Result<IdentityObservation> {
    if !path.is_absolute() {
        return Err(Error::new(ErrorKind::InvalidPath));
    }
    let name = path
        .file_name()
        .ok_or_else(|| Error::new(ErrorKind::InvalidPath))?;
    leaf(name)?;
    let parent = match native::Directory::open(
        path.parent()
            .ok_or_else(|| Error::new(ErrorKind::InvalidPath))?,
    ) {
        Ok(parent) => parent,
        Err(error) => return identity_io(error),
    };
    let file = match parent.open_file(name) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            parent.revalidate().map_err(Error::io)?;
            return Ok(IdentityObservation::Missing);
        }
        Err(error) => return identity_io(error),
    };
    let before = match native::info(&file) {
        Ok(info) => info,
        Err(error) => return identity_io(error),
    };
    if before.redirected {
        return Ok(IdentityObservation::Unsupported(
            UnsupportedReason::UnsafePath,
        ));
    }
    if !before.regular {
        return Ok(IdentityObservation::Unsupported(
            UnsupportedReason::NotRegularFile,
        ));
    }
    parent.revalidate().map_err(Error::io)?;
    let named = parent
        .open_file(name)
        .and_then(|file| native::info(&file))
        .map_err(Error::io)?;
    if named.identity != before.identity || named.redirected || !named.regular {
        return Err(Error::new(ErrorKind::IdentityChanged));
    }
    Ok(IdentityObservation::Regular(before.identity))
}

pub struct Guard {
    directory: native::Directory,
    file: fs::File,
    identity: FileIdentity,
    _not_sync: PhantomData<Cell<()>>,
}
impl fmt::Debug for Guard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DocumentOperationsGuard")
            .finish_non_exhaustive()
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        native::unlock(&self.file);
    }
}
/// Existing data directory only. Canonical profile aliases use one stable empty
/// lock. It is never truncated, unlinked, PID-written or adopted by stale time.
pub fn acquire(data_dir: &Path, wait: Duration) -> Result<Guard> {
    let canonical = data_dir.canonicalize().map_err(Error::io)?;
    let directory = native::Directory::open(&canonical).map_err(lock_io)?;
    let started = Instant::now();
    let wait = wait.min(MAX_LOCK_WAIT);
    loop {
        match directory.try_lock(OsStr::new(LOCK_FILE_NAME)) {
            Ok(Some(file)) => {
                let info = native::info(&file).map_err(Error::io)?;
                if !info.regular || info.redirected || info.length != 0 || info.links != 1 {
                    native::unlock(&file);
                    return Err(Error::new(ErrorKind::InvalidLock));
                }
                let guard = Guard {
                    directory,
                    file,
                    identity: info.identity,
                    _not_sync: PhantomData,
                };
                guard.revalidate()?;
                return Ok(guard);
            }
            Ok(None) => (),
            Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                return Err(Error::new(ErrorKind::InvalidLock))
            }
            Err(error) => return Err(Error::io(error)),
        }
        if started.elapsed() >= wait {
            return Err(Error::new(ErrorKind::Busy));
        }
        std::thread::sleep(Duration::from_millis(10).min(wait.saturating_sub(started.elapsed())));
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveReceipt {
    pub fingerprint: FileFingerprint,
}
impl Guard {
    pub fn verify_profile(&self, data_dir: &Path) -> Result<()> {
        self.revalidate()?;
        let canonical = data_dir.canonicalize().map_err(Error::io)?;
        let directory = native::Directory::open(&canonical).map_err(Error::io)?;
        if directory.identity() != self.directory.identity() {
            return Err(Error::new(ErrorKind::InvalidLock));
        }
        Ok(())
    }
    pub fn revalidate(&self) -> Result<()> {
        self.directory.revalidate().map_err(lock_io)?;
        let info = native::info(&self.file).map_err(Error::io)?;
        if !info.regular
            || info.redirected
            || info.length != 0
            || info.links != 1
            || info.identity != self.identity
        {
            return Err(Error::new(ErrorKind::InvalidLock));
        }
        if !self
            .directory
            .lock_still_named(OsStr::new(LOCK_FILE_NAME), &self.identity)
            .map_err(lock_io)?
        {
            return Err(Error::new(ErrorKind::InvalidLock));
        }
        Ok(())
    }
    pub fn verify_existing(
        &self,
        path: &Path,
        expected: &FileFingerprint,
        maximum_bytes: u64,
    ) -> Result<ObservedFile> {
        self.revalidate()?;
        let observed = match observe(path, maximum_bytes)? {
            Observation::Regular(file) => file,
            Observation::Missing => return Err(Error::new(ErrorKind::Missing)),
            Observation::Unsupported(_) => return Err(Error::new(ErrorKind::Unsupported)),
        };
        if observed.fingerprint.identity != expected.identity {
            return Err(Error::new(ErrorKind::IdentityChanged));
        }
        if observed.fingerprint.revision != expected.revision {
            return Err(Error::new(ErrorKind::RevisionChanged));
        }
        Ok(observed)
    }
    /// Identity only: caller must separately recheck the stored content revision
    /// under this gate. This is not an atomic compare-and-swap against external
    /// writers, nor authorization for another pathname sharing the same object.
    pub fn verify_existing_identity(
        &self,
        path: &Path,
        expected: &FileIdentity,
    ) -> Result<FileIdentity> {
        self.revalidate()?;
        let identity = match observe_identity(path)? {
            IdentityObservation::Regular(identity) => identity,
            IdentityObservation::Missing => return Err(Error::new(ErrorKind::Missing)),
            IdentityObservation::Unsupported(_) => return Err(Error::new(ErrorKind::Unsupported)),
        };
        if identity != *expected {
            return Err(Error::new(ErrorKind::IdentityChanged));
        }
        Ok(identity)
    }
    /// Preflight only. This never authorizes creation or substitutes for a
    /// no-clobber publishing primitive after this check.
    pub fn verify_missing(&self, path: &Path) -> Result<()> {
        self.revalidate()?;
        if !path.is_absolute() {
            return Err(Error::new(ErrorKind::InvalidPath));
        }
        let name = path
            .file_name()
            .ok_or_else(|| Error::new(ErrorKind::InvalidPath))?;
        leaf(name)?;
        let parent = native::Directory::open(
            path.parent()
                .ok_or_else(|| Error::new(ErrorKind::InvalidPath))?,
        )
        .map_err(Error::io)?;
        if parent.exists(name).map_err(Error::io)? {
            return Err(Error::new(ErrorKind::TargetExists));
        }
        parent.revalidate().map_err(Error::io)
    }
    /// Same parent, no temporary name, no copy/delete or overwrite fallback.
    /// Caller must persist the rename intent before calling and retain it for
    /// Unknown/Moved errors. Never attempt a destructive automatic rollback.
    pub fn move_same_directory_noreplace(
        &self,
        source: &ObservedFile,
        target_name: &OsStr,
    ) -> Result<MoveReceipt> {
        self.revalidate()?;
        leaf(target_name)?;
        if target_name == source.name {
            return Err(Error::new(ErrorKind::InvalidPath));
        }
        source.parent.revalidate().map_err(Error::io)?;
        let path = source.parent.path().join(&source.name);
        let _verified =
            self.verify_existing(&path, &source.fingerprint, source.bytes.len() as u64)?;
        if source.parent.exists(target_name).map_err(Error::io)? {
            return Err(Error::new(ErrorKind::TargetExists));
        }
        #[cfg(test)]
        tests::move_barrier(false);
        if let Err(error) = source.parent.rename_noreplace(&source.name, target_name) {
            let unsupported = error.kind() == io::ErrorKind::Unsupported;
            let exists = error.kind() == io::ErrorKind::AlreadyExists;
            let mut result = Error::io(error);
            if exists {
                result.kind = ErrorKind::TargetExists;
            }
            result.move_state = if unsupported {
                MoveState::NotAttempted
            } else {
                MoveState::Unknown
            };
            return Err(result);
        }
        #[cfg(test)]
        tests::move_barrier(true);
        self.revalidate().map_err(Error::after_move)?;
        source
            .parent
            .revalidate()
            .map_err(Error::io)
            .map_err(Error::after_move)?;
        if source
            .parent
            .exists(&source.name)
            .map_err(Error::io)
            .map_err(Error::after_move)?
        {
            return Err(Error::new(ErrorKind::IdentityChanged).after_move());
        }
        let target = source.parent.path().join(target_name);
        let observed = self
            .verify_existing(&target, &source.fingerprint, source.bytes.len() as u64)
            .map_err(Error::after_move)?;
        source
            .parent
            .sync()
            .map_err(Error::io)
            .map_err(Error::after_move)?;
        Ok(MoveReceipt {
            fingerprint: observed.fingerprint.clone(),
        })
    }
}

#[cfg(test)]
#[path = "document_operations/tests.rs"]
mod tests;
