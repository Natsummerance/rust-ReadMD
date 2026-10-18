//! Owned transient work, separate from documents and recoverable content.
//! Only explicitly created file identities are reclaimable. Never recurse
//! through unknown files, reparse points or a directory inferred by prefix.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};
mod registry;

const PROTOCOL: &str = "worker-tasks-v1";
const MAX_RECORD: u64 = 4 * 1024 * 1024;
const MAX_ENTRIES: usize = 16_384;
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "Invalid task ownership record")
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Identity {
    volume: u64,
    index: u64,
    created: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
enum Kind {
    File,
    Directory,
    Link,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    schema: u8,
    id: String,
    profile: String,
    root: PathBuf,
    identity: Identity,
    purpose: String,
    // Legacy records predate admission budgets. Zero means unknown, not free.
    #[serde(default)]
    reserved_bytes: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
enum Event {
    Header {
        value: Header,
    },
    Created {
        path: String,
        identity: Identity,
        kind: Kind,
        target: Option<PathBuf>,
    },
    Reference {
        revision: u64,
        name: String,
        present: bool,
    },
    NativeResource {
        resource: NativeResource,
    },
    Finished,
}
/// OS resources are recorded only after the creating API reports success.
/// Names are bound to this task; arbitrary profiles/device names are invalid.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", deny_unknown_fields)]
pub(crate) enum NativeResource {
    AppContainer { name: String },
    DosDevice { name: String, target: Vec<u16> },
}
impl NativeResource {
    fn name(&self) -> &str {
        match self {
            Self::AppContainer { name } | Self::DosDevice { name, .. } => name,
        }
    }
    fn validate(&self, task_id: &str) -> io::Result<()> {
        let prefix = format!("ReadMD.Worker.{task_id}.");
        match self {
            Self::AppContainer { name } if name == &format!("{prefix}profile") => Ok(()),
            Self::DosDevice { name, target }
                if (name == &format!("{prefix}runtime")
                    || name == &format!("{prefix}workspace"))
                    && target.starts_with(&[92, 63, 63, 92])
                    && target.len() > 4
                    && target.len() <= 32_767
                    && !target.contains(&0) => Ok(()),
            _ => Err(invalid()),
        }
    }
}
#[derive(Clone)]
struct Entry {
    identities: Vec<Identity>,
    kind: Kind,
    target: Option<PathBuf>,
}
struct Record {
    header: Header,
    entries: BTreeMap<String, Entry>,
    resources: BTreeMap<String, NativeResource>,
    refs: BTreeSet<String>,
    revision: u64,
    finished: bool,
}
impl Record {
    fn validate(&self, event: &Event) -> io::Result<()> {
        match event {
            Event::Header { .. } => return Err(invalid()),
            Event::Created {
                path,
                identity,
                kind,
                target,
            } => {
                relative(path)?;
                if self.entries.len() >= MAX_ENTRIES && !self.entries.contains_key(path) {
                    return Err(invalid());
                }
                if (*kind == Kind::Link) != target.is_some() {
                    return Err(invalid());
                }
                if let Some(e) = self.entries.get(path) {
                    if e.kind != *kind
                        || e.target != *target
                        || (!e.identities.contains(identity) && e.identities.len() >= 8)
                    {
                        return Err(invalid());
                    }
                }
            }
            Event::Reference { revision, name, .. } => {
                if !crate::cancel::valid_id(name) || Some(*revision) != self.revision.checked_add(1)
                {
                    return Err(invalid());
                }
            }
            Event::NativeResource { resource } => {
                if self.header.purpose != "latex-sandbox" {
                    return Err(invalid());
                }
                resource.validate(&self.header.id)?;
                if self.resources.get(resource.name()).is_some_and(|old| old != resource) {
                    return Err(invalid());
                }
            }
            Event::Finished => (),
        }
        Ok(())
    }
    fn apply(&mut self, event: Event) -> io::Result<()> {
        self.validate(&event)?;
        match event {
            Event::Header { .. } => return Err(invalid()),
            Event::Created {
                path,
                identity,
                kind,
                target,
            } => {
                relative(&path)?;
                if self.entries.len() >= MAX_ENTRIES && !self.entries.contains_key(&path) {
                    return Err(invalid());
                }
                if (kind == Kind::Link) != target.is_some() {
                    return Err(invalid());
                }
                let e = self.entries.entry(path).or_insert_with(|| Entry {
                    identities: Vec::new(),
                    kind: kind.clone(),
                    target: target.clone(),
                });
                if !e.identities.contains(&identity) {
                    e.identities.push(identity);
                }
            }
            Event::Reference {
                revision,
                name,
                present,
            } => {
                if !crate::cancel::valid_id(&name)
                    || revision != self.revision.checked_add(1).ok_or_else(invalid)?
                {
                    return Err(invalid());
                }
                self.revision = revision;
                if present {
                    self.refs.insert(name);
                } else {
                    self.refs.remove(&name);
                }
            }
            Event::NativeResource { resource } => {
                self.resources.insert(resource.name().into(), resource);
            }
            Event::Finished => self.finished = true,
        }
        Ok(())
    }
}
fn relative(value: &str) -> io::Result<&Path> {
    let path = Path::new(value);
    if value.is_empty()
        || value.len() > 4096
        || value.contains(['\\', ':', '\0'])
        || !path.components().all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(invalid());
    }
    Ok(path)
}
fn plain(path: &Path, directory: bool) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink()
        || meta.is_dir() != directory
        || (!directory && !meta.is_file())
    {
        return Err(invalid());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err(invalid());
        }
    }
    Ok(())
}
fn directory(path: &Path) -> io::Result<()> {
    match fs::create_dir(path) {
        Ok(()) => (),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e),
    }
    plain(path, true)
}
fn profile(data: &Path) -> io::Result<String> {
    let canonical = data.canonicalize()?;
    plain(&canonical, true)?;
    #[cfg(windows)]
    let bytes: Vec<u8> = {
        use std::os::windows::ffi::OsStrExt;
        canonical
            .as_os_str()
            .encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect()
    };
    #[cfg(unix)]
    let bytes = {
        use std::os::unix::ffi::OsStrExt;
        canonical.as_os_str().as_bytes().to_vec()
    };
    #[cfg(not(any(windows, unix)))]
    let bytes = canonical.to_string_lossy().as_bytes().to_vec();
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn journal_root(data: &Path, create: bool) -> io::Result<PathBuf> {
    let base = data.canonicalize()?;
    plain(&base, true)?;
    let root = base.join(PROTOCOL);
    if create {
        directory(&root)?;
    } else {
        plain(&root, true)?;
    }
    if root.canonicalize()? != root {
        return Err(invalid());
    }
    Ok(root)
}
fn owner(path: &Path, create: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(create);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0).custom_flags(0x00200000);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        #[cfg(target_os = "linux")]
        const NOFOLLOW: i32 = 0x20000;
        #[cfg(target_os = "macos")]
        const NOFOLLOW: i32 = 0x100;
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        const NOFOLLOW: i32 = 0;
        options.custom_flags(NOFOLLOW);
    }
    let file = options.open(path)?;
    if file.metadata()?.len() != 0 {
        return Err(invalid());
    }
    identity_file(&file, &Kind::File)?;
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        unsafe extern "C" {
            fn flock(fd: std::ffi::c_int, op: std::ffi::c_int) -> std::ffi::c_int;
        }
        if unsafe { flock(file.as_raw_fd(), 2 | 4) } != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(file)
}
fn append(file: &mut File, event: &Event) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(event)?;
    bytes.push(b'\n');
    if file.metadata()?.len().saturating_add(bytes.len() as u64) > MAX_RECORD {
        return Err(invalid());
    }
    file.write_all(&bytes)?;
    file.sync_data()
}
fn load(path: &Path) -> io::Result<Record> {
    let mut options = OpenOptions::new();
    options.read(true);
    registry::nofollow(&mut options);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1).custom_flags(0x00200000);
    }
    load_file(options.open(path)?)
}
fn load_file(file: File) -> io::Result<Record> {
    identity_file(&file, &Kind::File)?;
    if file.metadata()?.len() > MAX_RECORD {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    file.take(MAX_RECORD + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_RECORD {
        return Err(invalid());
    }
    let end = bytes
        .iter()
        .rposition(|c| *c == b'\n')
        .map(|n| n + 1)
        .ok_or_else(invalid)?;
    // An interrupted final append is not an instruction. Preserve all paths
    // it may have mentioned; only the completed prefix can identify files.
    let mut events = bytes[..end]
        .split(|c| *c == b'\n')
        .filter(|s| !s.is_empty());
    let Event::Header { value: header } =
        serde_json::from_slice(events.next().ok_or_else(invalid)?)?
    else {
        return Err(invalid());
    };
    if header.schema != 1
        || !valid_uuid(&header.id)
        || header.profile.len() != 64
        || !header
            .profile
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        || !header.root.is_absolute()
        || !crate::cancel::valid_id(&header.purpose)
        || (header.reserved_bytes != 0
            && header.reserved_bytes != registry::budget(&header.purpose))
    {
        return Err(invalid());
    }
    let mut record = Record {
        header,
        entries: BTreeMap::new(),
        resources: BTreeMap::new(),
        refs: BTreeSet::new(),
        revision: 0,
        finished: false,
    };
    for line in events {
        record.apply(serde_json::from_slice(line)?)?;
    }
    Ok(record)
}
fn valid_uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| id.simple().to_string() == value)
}
struct State {
    record: Record,
    journal: Option<File>,
}
impl State {
    fn commit(&mut self, event: Event) -> io::Result<()> {
        self.record.validate(&event)?;
        let mut journal = self.journal.take().ok_or_else(invalid)?;
        // After a partial append, refuse further writes; recovery reads only
        // the completed prefix, and never converts an incomplete line to a command.
        append(&mut journal, &event)?;
        self.record.apply(event)?;
        self.journal = Some(journal);
        Ok(())
    }
}
struct Inner {
    data: PathBuf,
    journal_dir: PathBuf,
    root: PathBuf,
    state: Mutex<State>,
    owner: Option<File>,
}
/// A fresh transient workspace. Pins are explicit recovery references, not
/// ordinary cache ages. The record never contains document bodies or secrets.
#[derive(Clone)]
pub struct Workspace {
    inner: Arc<Inner>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CreationStage {
    JournalDirectory,
    Lease,
    WorkRoot,
    JournalFile,
    HeaderSynced,
}
// Roll back ordinary creation errors by handle-derived identities. An abrupt
// process death before the durable header is a different case: its unknown
// objects are preserved, never adopted merely because their names match.
struct Creation {
    directory: PathBuf,
    identity: Identity,
    lease: Option<File>,
    root: Option<(PathBuf, Identity)>,
    journal: Option<(PathBuf, Identity)>,
    work_parents: Vec<File>,
    committed: bool,
}
impl Drop for Creation {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        let lease = self.lease.take();
        let lease_id = lease
            .as_ref()
            .and_then(|f| identity_file(f, &Kind::File).ok());
        drop(lease);
        if let Some((path, id)) = &self.root {
            if !remove_identified(path, std::slice::from_ref(id), &Kind::Directory, None)
                .unwrap_or(false)
            {
                return;
            }
        }
        if let Some((path, id)) = &self.journal {
            if !remove_identified(path, std::slice::from_ref(id), &Kind::File, None)
                .unwrap_or(false)
            {
                return;
            }
        }
        if let Some(id) = lease_id {
            if !remove_identified(&self.directory.join("owner.lock"), &[id], &Kind::File, None)
                .unwrap_or(false)
            {
                return;
            }
        }
        let _ = remove_identified(
            &self.directory,
            std::slice::from_ref(&self.identity),
            &Kind::Directory,
            None,
        );
    }
}
thread_local! { static CURRENT: RefCell<Option<Arc<Inner>>> = const { RefCell::new(None) }; }
impl Workspace {
    pub fn create(data: &Path, purpose: &str) -> io::Result<Self> {
        Self::create_at(data, purpose, |_, _| Ok(()))
    }
    fn create_at(
        data: &Path,
        purpose: &str,
        mut checkpoint: impl FnMut(CreationStage, &Path) -> io::Result<()>,
    ) -> io::Result<Self> {
        if !crate::cancel::valid_id(purpose) {
            return Err(invalid());
        }
        let data = data.canonicalize()?;
        let profile = profile(&data)?;
        let journal_root = journal_root(&data, true)?;
        let _journal_parents = absolute_parents(&journal_root)?;
        let _journal_guard = open_parent(&journal_root)?;
        identity_file(&_journal_guard, &Kind::Directory)?;
        let temp = std::env::temp_dir().canonicalize()?;
        plain(&temp, true)?;
        let _admission = registry::admit(&journal_root, &profile, purpose, &temp)?;
        let id = uuid::Uuid::new_v4().simple().to_string();
        let journal_dir = journal_root.join(&id);
        fs::create_dir(&journal_dir)?;
        let mut creation = Creation {
            identity: identity_path(&journal_dir, Kind::Directory)?,
            directory: journal_dir.clone(),
            lease: None,
            root: None,
            journal: None,
            work_parents: Vec::new(),
            committed: false,
        };
        checkpoint(CreationStage::JournalDirectory, &journal_dir)?;
        creation.lease = Some(owner(&journal_dir.join("owner.lock"), true)?);
        checkpoint(CreationStage::Lease, &journal_dir)?;
        let parent = temp.join(format!("rmdw-{}", &profile[..12]));
        directory(&parent)?;
        if parent.canonicalize()? != parent {
            return Err(invalid());
        }
        creation.work_parents = absolute_parents(&parent)?;
        let work_guard = open_parent(&parent)?;
        identity_file(&work_guard, &Kind::Directory)?;
        creation.work_parents.push(work_guard);
        let root = parent.join(&id);
        fs::create_dir(&root)?;
        let root_id = identity_path(&root, Kind::Directory)?;
        creation.root = Some((root.clone(), root_id.clone()));
        checkpoint(CreationStage::WorkRoot, &root)?;
        let header = Header {
            schema: 1,
            id,
            profile,
            root: root.clone(),
            identity: root_id,
            purpose: purpose.into(),
            reserved_bytes: registry::budget(purpose),
        };
        let mut journal = OpenOptions::new()
            .read(true)
            .append(true)
            .create_new(true)
            .open(journal_dir.join("record.jsonl"))?;
        creation.journal = Some((
            journal_dir.join("record.jsonl"),
            identity_file(&journal, &Kind::File)?,
        ));
        checkpoint(CreationStage::JournalFile, &root)?;
        append(
            &mut journal,
            &Event::Header {
                value: header.clone(),
            },
        )?;
        checkpoint(CreationStage::HeaderSynced, &root)?;
        creation.committed = true;
        Ok(Self {
            inner: Arc::new(Inner {
                data,
                journal_dir,
                root,
                owner: creation.lease.take(),
                state: Mutex::new(State {
                    record: Record {
                        header,
                        entries: BTreeMap::new(),
                        resources: BTreeMap::new(),
                        refs: BTreeSet::new(),
                        revision: 0,
                        finished: false,
                    },
                    journal: Some(journal),
                }),
            }),
        })
    }
    pub fn path(&self) -> &Path {
        &self.inner.root
    }
    pub fn id(&self) -> String {
        self.inner
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .record
            .header
            .id
            .clone()
    }
    pub fn lease(&self) -> &File {
        self.inner.owner.as_ref().unwrap()
    }
    pub(crate) fn register_native_resource(&self, resource: NativeResource) -> io::Result<()> {
        self.inner.state.lock().unwrap_or_else(|e| e.into_inner())
            .commit(Event::NativeResource { resource })
    }
    pub fn scope<T>(&self, action: impl FnOnce() -> T) -> T {
        struct Restore(Option<Arc<Inner>>);
        impl Drop for Restore {
            fn drop(&mut self) {
                CURRENT.with(|c| *c.borrow_mut() = self.0.take());
            }
        }
        let _restore = Restore(CURRENT.with(|c| c.replace(Some(self.inner.clone()))));
        action()
    }
    pub fn pin(&self, reference: &str) -> io::Result<u64> {
        let mut state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
        let revision = state.record.revision.checked_add(1).ok_or_else(invalid)?;
        let event = Event::Reference {
            revision,
            name: reference.into(),
            present: true,
        };
        if !crate::cancel::valid_id(reference) {
            return Err(invalid());
        }
        state.commit(event)?;
        Ok(revision)
    }
    pub fn finish(&self) -> io::Result<()> {
        let mut state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
        state.commit(Event::Finished)
    }
}
impl Drop for Inner {
    fn drop(&mut self) {
        drop(
            self.state
                .get_mut()
                .unwrap_or_else(|e| e.into_inner())
                .journal
                .take(),
        );
        drop(self.owner.take());
        // Child-inherited leases prevent this attempt while an engine is alive.
        // An incomplete/unknown task is preserved for background recovery.
        let _ = recover_one(&self.data, &self.journal_dir);
    }
}
fn track(
    inner: &Inner,
    path: &Path,
    file: &File,
    kind: Kind,
    target: Option<PathBuf>,
) -> io::Result<()> {
    let name = path
        .strip_prefix(&inner.root)
        .map_err(|_| invalid())?
        .to_str()
        .ok_or_else(invalid)?
        .replace('\\', "/");
    relative(&name)?;
    let identity = identity_file(file, &kind)?;
    let mut state = inner.state.lock().unwrap_or_else(|e| e.into_inner());
    let event = Event::Created {
        path: name,
        identity,
        kind,
        target,
    };
    state.commit(event)
}
fn scoped(path: &Path) -> Option<Arc<Inner>> {
    CURRENT.with(|c| {
        c.borrow()
            .as_ref()
            .filter(|i| path.starts_with(&i.root))
            .cloned()
    })
}
/// Register a newly created handle, never adopt a file discovered by scanning.
pub(crate) fn track_created(path: &Path, file: &File) -> io::Result<()> {
    if let Some(inner) = scoped(path) {
        track(&inner, path, file, Kind::File, None)?;
    }
    Ok(())
}
pub(crate) fn track_atomic(temp: &tempfile::NamedTempFile, target: &Path) -> io::Result<bool> {
    let Some(inner) = scoped(target) else {
        return Ok(false);
    };
    match fs::symlink_metadata(target) {
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Task output already exists",
            ))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => (),
        Err(e) => return Err(e),
    }
    track(&inner, temp.path(), temp.as_file(), Kind::File, None)?;
    track(&inner, target, temp.as_file(), Kind::File, None)?;
    Ok(true)
}
pub(crate) fn write(path: &Path, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    let Some(inner) = scoped(path) else {
        return fs::write(path, bytes);
    };
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)?;
    track(&inner, path, &file, Kind::File, None)?;
    file.write_all(bytes.as_ref())?;
    Ok(())
}
pub(crate) fn copy(source: &Path, target: &Path) -> io::Result<()> {
    let mut source = File::open(source)?;
    let mut target_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    track_created(target, &target_file)?;
    io::copy(&mut source, &mut target_file)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        target_file.set_permissions(fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
pub(crate) fn create_dir(path: &Path) -> io::Result<()> {
    fs::create_dir(path)?;
    if let Some(inner) = scoped(path) {
        let file = open_identity(path, false)?;
        track(&inner, path, &file, Kind::Directory, None)?;
    }
    Ok(())
}
pub(crate) fn track_link(path: &Path, target: &Path) -> io::Result<()> {
    if let Some(inner) = scoped(path) {
        // Windows read_link may simplify a short junction target even though
        // its creation input was verbatim. Validate the resolved destination,
        // then persist the actual reparse spelling for later exact comparison.
        // Recovery must not need to resolve a possibly unavailable target.
        let actual_target = fs::read_link(path)?;
        if actual_target.canonicalize()? != target.canonicalize()? {
            return Err(invalid());
        }
        let file = open_identity(path, false)?;
        track(&inner, path, &file, Kind::Link, Some(actual_target))?;
    }
    Ok(())
}
/// A child inherits the exact OS lease, never a path that it can reopen later.
pub(crate) fn current_lease(directory: &Path) -> io::Result<Option<File>> {
    scoped(directory)
        .map(|inner| inner.owner.as_ref().ok_or_else(invalid)?.try_clone())
        .transpose()
}
/// Release an explicit recovery reference after independently persisting its
/// replacement. Revision guards make stale/repeated release requests harmless.
pub fn release(data: &Path, id: &str, expected_revision: u64, reference: &str) -> io::Result<u64> {
    if !valid_uuid(id) || !crate::cancel::valid_id(reference) {
        return Err(invalid());
    }
    let dir = journal_root(data, false)?.join(id);
    let _parents = absolute_parents(&dir)?;
    let _directory = open_parent(&dir)?;
    identity_file(&_directory, &Kind::Directory)?;
    plain(&dir, true)?;
    let _lease = owner(&dir.join("owner.lock"), false)?;
    // Validate and modify the same opened file. Holding the parent alone does
    // not prevent a leaf from being replaced between two path-based opens.
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    registry::nofollow(&mut options);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1).custom_flags(0x00200000);
    }
    let mut journal = options.open(dir.join("record.jsonl"))?;
    let record = load_file(journal.try_clone()?)?;
    if record.header.id != id
        || record.header.profile != profile(data)?
        || record.revision != expected_revision
        || !record.refs.contains(reference)
    {
        return Err(invalid());
    }
    let revision = expected_revision.checked_add(1).ok_or_else(invalid)?;
    journal.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut journal)
        .take(MAX_RECORD + 1)
        .read_to_end(&mut bytes)?;
    let complete = bytes
        .iter()
        .rposition(|c| *c == b'\n')
        .ok_or_else(invalid)?
        + 1;
    journal.set_len(complete as u64)?;
    journal.seek(SeekFrom::End(0))?;
    let event = Event::Reference {
        revision,
        name: reference.into(),
        present: false,
    };
    record.validate(&event)?;
    append(&mut journal, &event)?;
    Ok(revision)
}
#[derive(Default, Debug, Serialize)]
pub struct RecoveryReport {
    pub visited: usize,
    pub removed: usize,
    pub running: usize,
    pub retained: usize,
    pub preserved: usize,
    pub failed: usize,
    pub cursor_unavailable: bool,
}
#[derive(PartialEq)]
enum Outcome {
    Removed,
    Running,
    Retained,
    Preserved,
}
fn recover_one(data: &Path, journal_dir: &Path) -> io::Result<Outcome> {
    let _journal_parents = absolute_parents(journal_dir)?;
    plain(journal_dir, true)?;
    let journal_dir_id = identity_path(journal_dir, Kind::Directory)?;
    let lease = match owner(&journal_dir.join("owner.lock"), false) {
        Ok(file) => file,
        Err(e) if e.raw_os_error() == Some(32) || e.kind() == io::ErrorKind::WouldBlock => {
            return Ok(Outcome::Running)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            // Cleanup removes the lease marker first. A crash between the two
            // leaf deletions leaves a validated journal that can be resumed.
            let record = load(&journal_dir.join("record.jsonl"))?;
            if record.header.profile != profile(data)?
                || journal_dir.file_name().and_then(|n| n.to_str()) != Some(&record.header.id)
            {
                return Err(invalid());
            }
            owner(&journal_dir.join("owner.lock"), true)?
        }
        Err(e) => return Err(e),
    };
    let record = load(&journal_dir.join("record.jsonl"))?;
    if record.header.profile != profile(data)?
        || journal_dir.file_name().and_then(|n| n.to_str()) != Some(&record.header.id)
    {
        return Err(invalid());
    }
    if !record.refs.is_empty() {
        return Ok(Outcome::Retained);
    }
    #[cfg(not(windows))]
    if !record.finished {
        return Ok(Outcome::Retained);
    } // owner-crash containment not certified there
    let root = &record.header.root;
    if root.file_name().and_then(|n| n.to_str()) != Some(&record.header.id)
        || root
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            != Some(&format!("rmdw-{}", &record.header.profile[..12]))
    {
        return Err(invalid());
    }
    // The exclusive task lease also covers inherited worker handles. Recover
    // known OS resources even if work-file cleanup must preserve unknown files.
    // Older readers reject the new event rather than silently discarding it.
    for resource in record.resources.values() {
        if !crate::worker_sandbox::recover_native_resource(resource)? {
            return Ok(Outcome::Preserved);
        }
    }
    let _work_parents = absolute_parents(root)?;
    match fs::symlink_metadata(root) {
        Ok(_) => {
            if identity_path(root, Kind::Directory)? != record.header.identity
                || root.canonicalize()? != *root
            {
                return Ok(Outcome::Preserved);
            }
            let mut entries: Vec<_> = record.entries.iter().collect();
            entries.sort_by(|(a, x), (b, y)| {
                (x.kind != Kind::File)
                    .cmp(&(y.kind != Kind::File))
                    .then_with(|| b.len().cmp(&a.len()))
            });
            for (name, entry) in entries {
                let Some(_parents) = hold_parents(root, name, &record)? else {
                    continue;
                };
                remove_identified(
                    &root.join(relative(name)?),
                    &entry.identities,
                    &entry.kind,
                    entry.target.as_deref(),
                )?;
            }
            if !remove_identified(
                root,
                &[record.header.identity.clone()],
                &Kind::Directory,
                None,
            )? {
                return Ok(Outcome::Preserved);
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => (),
        Err(e) => return Err(e),
    }
    let journal_id = identity_path(&journal_dir.join("record.jsonl"), Kind::File)?;
    let lease_id = identity_file(&lease, &Kind::File)?;
    drop(lease);
    if !remove_identified(
        &journal_dir.join("owner.lock"),
        &[lease_id],
        &Kind::File,
        None,
    )? {
        return Ok(Outcome::Preserved);
    }
    if !remove_identified(
        &journal_dir.join("record.jsonl"),
        &[journal_id],
        &Kind::File,
        None,
    )? {
        return Ok(Outcome::Preserved);
    }
    // Leaf-only; a user-created extra journal file is retained.
    if remove_identified(journal_dir, &[journal_dir_id], &Kind::Directory, None)? {
        Ok(Outcome::Removed)
    } else {
        Ok(Outcome::Preserved)
    }
}
// Hold every absolute ancestor without delete-sharing. A concurrently replaced
// parent cannot redirect a later path-based open into somebody else's files.
fn absolute_parents(path: &Path) -> io::Result<Vec<File>> {
    let parent = path.parent().ok_or_else(invalid)?;
    let mut current = PathBuf::new();
    let mut handles = Vec::new();
    for component in parent.components() {
        current.push(component);
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        let file = open_parent(&current)?;
        identity_file(&file, &Kind::Directory)?;
        handles.push(file);
    }
    Ok(handles)
}
fn hold_parents(root: &Path, name: &str, record: &Record) -> io::Result<Option<Vec<File>>> {
    let mut handles = Vec::new();
    let mut path = root.to_owned();
    let file = open_parent(&path)?;
    if identity_file(&file, &Kind::Directory)? != record.header.identity {
        return Ok(None);
    }
    handles.push(file);
    let relative = relative(name)?;
    for component in relative.parent().unwrap_or(Path::new("")).components() {
        path.push(component);
        let key = path
            .strip_prefix(root)
            .map_err(|_| invalid())?
            .to_str()
            .ok_or_else(invalid)?
            .replace('\\', "/");
        let Some(entry) = record
            .entries
            .get(&key)
            .filter(|e| e.kind == Kind::Directory)
        else {
            return Ok(None);
        };
        let file = match open_parent(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        match identity_file(&file, &Kind::Directory) {
            Ok(id) if entry.identities.contains(&id) => (),
            _ => return Ok(None),
        }
        handles.push(file);
    }
    Ok(Some(handles))
}
#[cfg(windows)]
fn open_parent(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    OpenOptions::new()
        .access_mode(0x80)
        .share_mode(3)
        .custom_flags(0x02000000 | 0x00200000)
        .open(path)
}
#[cfg(not(windows))]
fn open_parent(path: &Path) -> io::Result<File> {
    open_identity(path, false)
}
pub fn recover(data: &Path, maximum: usize) -> io::Result<RecoveryReport> {
    let mut report = RecoveryReport::default();
    let root = match journal_root(data, false) {
        Ok(root) => root,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(report),
        Err(e) => return Err(e),
    };
    let _parents = absolute_parents(&root)?;
    let _directory = open_parent(&root)?;
    identity_file(&_directory, &Kind::Directory)?;
    let gate = match registry::gate(&root, "recovery.lock") {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(report),
        Err(e) => return Err(e),
    };
    let mut names = registry::candidates(&root)?;
    let mut cursor = registry::Cursor::open(&root, &profile(data)?).ok();
    report.cursor_unavailable = cursor.is_none();
    if let Some(cursor) = &cursor {
        cursor.order(&mut names);
    }
    for name in names.into_iter().take(maximum.min(registry::MAX_TASKS)) {
        report.visited += 1;
        match recover_one(data, &root.join(&name)) {
            Ok(Outcome::Removed) => report.removed += 1,
            Ok(Outcome::Running) => report.running += 1,
            Ok(Outcome::Retained) => report.retained += 1,
            Ok(Outcome::Preserved) => report.preserved += 1,
            Err(_) => report.failed += 1,
        }
        if let Some(cursor) = &mut cursor {
            cursor.advance(&name)?;
        }
    }
    drop(gate);
    Ok(report)
}
pub(crate) fn recover_background(data: &Path) {
    let data = data.to_owned();
    let _ = std::thread::Builder::new()
        .name("readmd-task-recovery".into())
        .spawn(move || {
            if let Ok(report) = recover(&data, registry::MAX_TASKS) {
                log::debug!(target:"readmd::tasks","workspace recovery: {:?}",report);
            }
        });
}

#[cfg(windows)]
fn open_identity(path: &Path, deleting: bool) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    OpenOptions::new()
        .access_mode(0x80 | if deleting { 0x10000 } else { 0 })
        .share_mode(if deleting { 1 } else { 7 })
        .custom_flags(0x02000000 | 0x00200000)
        .open(path)
}
#[cfg(windows)]
fn identity_file(file: &File, kind: &Kind) -> io::Result<Identity> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION},
    };
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info)
            .map_err(|e| io::Error::from_raw_os_error(e.code().0 & 0xffff))?;
    }
    let actual = if info.dwFileAttributes & 0x400 != 0 {
        Kind::Link
    } else if info.dwFileAttributes & 0x10 != 0 {
        Kind::Directory
    } else {
        Kind::File
    };
    if actual != *kind {
        return Err(invalid());
    }
    Ok(Identity {
        volume: info.dwVolumeSerialNumber as u64,
        index: ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64,
        created: ((info.ftCreationTime.dwHighDateTime as u64) << 32)
            | info.ftCreationTime.dwLowDateTime as u64,
    })
}
fn identity_path(path: &Path, kind: Kind) -> io::Result<Identity> {
    identity_file(&open_identity(path, false)?, &kind)
}
#[cfg(windows)]
fn remove_identified(
    path: &Path,
    identities: &[Identity],
    kind: &Kind,
    target: Option<&Path>,
) -> io::Result<bool> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{
            FileDispositionInfo, SetFileInformationByHandle, FILE_DISPOSITION_INFO,
        },
    };
    let file = match open_identity(path, true) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(true),
        Err(e) if e.raw_os_error() == Some(32) => return Ok(false),
        Err(e) => return Err(e),
    };
    match identity_file(&file, kind) {
        Ok(id) if identities.contains(&id) => (),
        _ => return Ok(false),
    }
    if let Some(target) = target {
        if fs::read_link(path)? != target {
            return Ok(false);
        }
    }
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    match unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfo,
            (&disposition as *const FILE_DISPOSITION_INFO).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    } {
        Ok(()) => Ok(true),
        Err(e) if matches!(e.code().0 & 0xffff, 145 | 32 | 5) => Ok(false),
        Err(e) => Err(io::Error::from_raw_os_error(e.code().0 & 0xffff)),
    }
}
#[cfg(unix)]
fn open_identity(path: &Path, _deleting: bool) -> io::Result<File> {
    File::open(path)
}
#[cfg(unix)]
fn identity_file(file: &File, kind: &Kind) -> io::Result<Identity> {
    use std::os::unix::fs::MetadataExt;
    let m = file.metadata()?;
    if m.is_dir() != (*kind == Kind::Directory) || *kind == Kind::Link {
        return Err(invalid());
    }
    Ok(Identity {
        volume: m.dev(),
        index: m.ino(),
        created: m
            .created()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0),
    })
}
#[cfg(not(windows))]
fn remove_identified(
    _path: &Path,
    _identities: &[Identity],
    _kind: &Kind,
    _target: Option<&Path>,
) -> io::Result<bool> {
    // Do not claim unlinkat/descriptor-relative native validation from Windows.
    Ok(false)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn native_resource_records_bind_names_targets_and_ownership_to_one_task() {
        let data = tempfile::tempdir().unwrap();
        let task = Workspace::create(data.path(), "latex-sandbox").unwrap();
        let resource = NativeResource::DosDevice {
            name: format!("ReadMD.Worker.{}.runtime", task.id()),
            target: "\\??\\C:\\owned-runtime".encode_utf16().collect(),
        };
        task.register_native_resource(resource.clone()).unwrap();
        task.register_native_resource(resource.clone()).unwrap();
        let mut foreign = resource.clone();
        let NativeResource::DosDevice { target, .. } = &mut foreign else { unreachable!() };
        target.push(0);
        assert!(task.register_native_resource(foreign).is_err());
        let foreign = NativeResource::AppContainer { name: "ReadMD.Worker.somebody-else.profile".into() };
        assert!(task.register_native_resource(foreign).is_err());
        {
            let state = task.inner.state.lock().unwrap();
            assert_eq!(state.record.resources.len(), 1);
            assert_eq!(state.record.resources.values().next(), Some(&resource));
        }
        let event = Event::NativeResource { resource: resource.clone() };
        let decoded: Event = serde_json::from_slice(&serde_json::to_vec(&event).unwrap()).unwrap();
        assert!(matches!(decoded, Event::NativeResource { resource: value } if value == resource));
        let ordinary = Workspace::create(data.path(), "ocr-recognize").unwrap();
        assert!(ordinary.register_native_resource(NativeResource::AppContainer {
            name: format!("ReadMD.Worker.{}.profile", ordinary.id()),
        }).is_err());
    }
    #[test]
    fn creation_errors_roll_back_only_captured_identities() {
        let data = tempfile::tempdir().unwrap();
        for expected in [
            CreationStage::JournalDirectory,
            CreationStage::Lease,
            CreationStage::WorkRoot,
            CreationStage::JournalFile,
            CreationStage::HeaderSynced,
        ] {
            let mut created_root = None;
            let error = Workspace::create_at(data.path(), "test-create-error", |stage, path| {
                if stage == CreationStage::WorkRoot {
                    created_root = Some(path.to_owned());
                }
                if stage == expected {
                    return Err(io::Error::new(
                        io::ErrorKind::StorageFull,
                        "Injected write failure",
                    ));
                }
                Ok(())
            })
            .err()
            .unwrap();
            assert_eq!(error.kind(), io::ErrorKind::StorageFull);
            assert!(
                created_root.is_none_or(|root| !root.exists()),
                "{expected:?}"
            );
            assert!(
                registry::candidates(&journal_root(data.path(), false).unwrap())
                    .unwrap()
                    .is_empty(),
                "{expected:?}"
            );
        }
        let mut root = None;
        assert!(
            Workspace::create_at(data.path(), "test-create-foreign", |stage, path| {
                if stage == CreationStage::WorkRoot {
                    fs::write(path.join("unknown.txt"), b"Keep unknown content")?;
                    root = Some(path.to_owned());
                    return Err(io::Error::other("Injected creation failure"));
                }
                Ok(())
            })
            .is_err()
        );
        let root = root.unwrap();
        assert_eq!(
            fs::read(root.join("unknown.txt")).unwrap(),
            b"Keep unknown content"
        );
        // No durable header exists; the background reaper must not adopt it.
        assert_eq!(recover(data.path(), 128).unwrap().failed, 1);
        assert!(root.join("unknown.txt").exists());
        fs::remove_file(root.join("unknown.txt")).unwrap();
        fs::remove_dir(&root).unwrap();
    }
    #[test]
    fn recovery_cursor_rotates_past_pinned_records() {
        let data = tempfile::tempdir().unwrap();
        let mut tasks = Vec::new();
        for _ in 0..3 {
            let work = Workspace::create(data.path(), "test-fair-recovery").unwrap();
            work.scope(|| write(&work.path().join("baseline.txt"), b"Recovery baseline").unwrap());
            let revision = work.pin("pending-review").unwrap();
            tasks.push((work.id(), work.path().to_owned(), revision));
            drop(work);
        }
        tasks.sort_by(|a, b| a.0.cmp(&b.0));
        release(data.path(), &tasks[2].0, tasks[2].2, "pending-review").unwrap();
        let mut reports = Vec::new();
        for _ in 0..3 {
            reports.push(recover(data.path(), 1).unwrap());
        }
        assert_eq!(
            reports.iter().map(|r| r.removed).sum::<usize>(),
            1,
            "{reports:?}"
        );
        assert!(!tasks[2].1.exists());
        for (id, root, revision) in tasks.iter().take(2) {
            assert_eq!(
                fs::read(root.join("baseline.txt")).unwrap(),
                b"Recovery baseline"
            );
            release(data.path(), id, *revision, "pending-review").unwrap();
        }
        assert_eq!(recover(data.path(), 128).unwrap().removed, 2);
    }
    #[test]
    fn admission_preserves_full_reservations_until_recovery() {
        let data = tempfile::tempdir().unwrap();
        let mut tasks = Vec::new();
        for _ in 0..8 {
            tasks.push(Workspace::create(data.path(), "test-reservation").unwrap());
        }
        let root = journal_root(data.path(), false).unwrap();
        assert_eq!(registry::candidates(&root).unwrap().len(), 8);
        assert_eq!(
            Workspace::create(data.path(), "test-ninth")
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(registry::candidates(&root).unwrap().len(), 8);
        let records = tasks
            .iter()
            .map(|task| task.inner.journal_dir.clone())
            .collect::<Vec<_>>();
        assert_eq!(recover(data.path(), 128).unwrap().running, 8);
        assert!(records.iter().all(|dir| dir.join("record.jsonl").exists()));
        drop(tasks);
        assert!(registry::candidates(&root).unwrap().is_empty());
        let replacement = Workspace::create(data.path(), "test-replacement").unwrap();
        drop(replacement);
        assert!(registry::candidates(&root).unwrap().is_empty());
    }
    #[test]
    fn legacy_ocr_records_remain_recoverable_and_reserve_conservatively() {
        let data = tempfile::tempdir().unwrap();
        let work = Workspace::create(data.path(), "ocr-recognize").unwrap();
        let id = work.id();
        let root = work.path().to_owned();
        work.scope(|| write(&root.join("scan.pnm"), b"Known scan input").unwrap());
        let revision = work.pin("legacy-test").unwrap();
        drop(work);
        let journal_root = journal_root(data.path(), false).unwrap();
        let path = journal_root.join(&id).join("record.jsonl");
        let bytes = fs::read(&path).unwrap();
        let first = bytes.iter().position(|b| *b == b'\n').unwrap();
        let mut header: serde_json::Value = serde_json::from_slice(&bytes[..first]).unwrap();
        header["value"]
            .as_object_mut()
            .unwrap()
            .remove("reserved_bytes");
        let mut updated = serde_json::to_vec(&header).unwrap();
        updated.extend_from_slice(&bytes[first..]);
        fs::write(&path, updated).unwrap();
        assert_eq!(load(&path).unwrap().header.reserved_bytes, 0);
        assert_eq!(
            registry::reserved(&journal_root, &[id.clone()], &profile(data.path()).unwrap())
                .unwrap(),
            registry::default_budget()
        );
        assert_eq!(recover(data.path(), 128).unwrap().retained, 1);
        release(data.path(), &id, revision, "legacy-test").unwrap();
        assert_eq!(recover(data.path(), 128).unwrap().removed, 1);
        assert!(!root.exists());
    }
    #[test]
    fn foreign_cursor_is_preserved_and_recovery_falls_back() {
        let data = tempfile::tempdir().unwrap();
        let work = Workspace::create(data.path(), "test-foreign-cursor").unwrap();
        let root = work.path().to_owned();
        let id = work.id();
        let revision = work.pin("keep-until-release").unwrap();
        drop(work);
        let path = journal_root(data.path(), false)
            .unwrap()
            .join("recovery.cursor");
        fs::write(&path, b"User-owned cursor filename").unwrap();
        release(data.path(), &id, revision, "keep-until-release").unwrap();
        let report = recover(data.path(), 128).unwrap();
        assert!(report.cursor_unavailable, "{report:?}");
        assert_eq!(report.removed, 1);
        assert!(!root.exists());
        assert_eq!(fs::read(path).unwrap(), b"User-owned cursor filename");
    }
    #[test]
    fn creation_child() {
        let Some(data) = std::env::var_os("READMD_TASK_CREATION_CHILD") else {
            return;
        };
        let target = std::env::var("READMD_TASK_CREATION_STAGE").unwrap();
        let _work = Workspace::create_at(Path::new(&data), "test-create-crash", |stage, path| {
            if format!("{stage:?}") == target {
                println!("CREATION_READY {}", serde_json::to_string(&path).unwrap());
                io::stdout().flush().unwrap();
                std::thread::sleep(std::time::Duration::from_secs(30));
            }
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn admission_child() {
        let Some(data) = std::env::var_os("READMD_TASK_ADMISSION_CHILD") else {
            return;
        };
        let mut tasks = Vec::new();
        for index in 0..8 {
            let work = Workspace::create(Path::new(&data), "test-native-admission").unwrap();
            work.scope(|| {
                write(
                    &work.path().join("baseline.txt"),
                    b"Protected recovery baseline",
                )
                .unwrap()
            });
            if index == 0 {
                assert_eq!(work.pin("review-fixture").unwrap(), 1);
            }
            tasks.push(work);
        }
        println!(
            "ADMISSION_READY {}",
            serde_json::to_string(
                &tasks
                    .iter()
                    .map(|work| serde_json::json!({"id":work.id(),"root":work.path()}))
                    .collect::<Vec<_>>()
            )
            .unwrap()
        );
        io::stdout().flush().unwrap();
        use std::io::BufRead;
        for line in io::stdin().lock().lines() {
            match line.unwrap().as_str() {
                "release-one" if tasks.len() == 8 => {
                    drop(tasks.pop());
                    println!("ADMISSION_RELEASED");
                    io::stdout().flush().unwrap();
                }
                _ => panic!("Unexpected fixture control"),
            }
        }
    }
    #[test]
    fn release_child() {
        let Some(data) = std::env::var_os("READMD_TASK_RELEASE_CHILD") else {
            return;
        };
        let id = std::env::var("READMD_TASK_RELEASE_ID").unwrap();
        assert_eq!(
            release(Path::new(&data), &id, 1, "review-fixture").unwrap(),
            2
        );
        println!("REFERENCE_RELEASED");
        io::stdout().flush().unwrap();
    }
    #[test]
    fn abrupt_process_exit_at_creation_boundaries_preserves_unknown_state() {
        for stage in [
            CreationStage::JournalDirectory,
            CreationStage::Lease,
            CreationStage::WorkRoot,
            CreationStage::JournalFile,
            CreationStage::HeaderSynced,
        ] {
            let data = tempfile::tempdir().unwrap();
            let observer = tempfile::tempdir().unwrap();
            let log_path = observer.path().join("creation.log");
            let log = File::create(&log_path).unwrap();
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "task_workspace::tests::creation_child",
                    "--nocapture",
                ])
                .env("READMD_TASK_CREATION_CHILD", data.path())
                .env("READMD_TASK_CREATION_STAGE", format!("{stage:?}"));
            let mut child =
                crate::worker_sandbox::spawn_managed(&mut command, observer.path(), &log, &log)
                    .unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            let ready = loop {
                let text = fs::read_to_string(&log_path).unwrap();
                if let Some(line) = text.lines().find_map(|s| s.strip_prefix("CREATION_READY ")) {
                    break serde_json::from_str::<PathBuf>(line).unwrap();
                }
                assert!(
                    std::time::Instant::now() < deadline && child.try_wait().unwrap().is_none(),
                    "{stage:?}: {text}"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            };
            let journals = journal_root(data.path(), false).unwrap();
            let ids = registry::candidates(&journals).unwrap();
            assert_eq!(ids.len(), 1, "{stage:?}");
            assert_eq!(
                Workspace::create(data.path(), "test-competing-create")
                    .err()
                    .unwrap()
                    .kind(),
                io::ErrorKind::WouldBlock
            );
            let other_profile = tempfile::tempdir().unwrap();
            drop(Workspace::create(other_profile.path(), "test-independent-profile").unwrap());
            let before = recover(data.path(), 128).unwrap();
            if stage == CreationStage::JournalDirectory {
                assert_eq!(before.failed, 1, "{before:?}");
            } else {
                assert_eq!(before.running, 1, "{before:?}");
            }
            child.stop().unwrap();
            // Stopping must confirm the root process itself, not just the
            // job's accounting counter, before any recovery attempt.
            assert!(child.try_wait().unwrap().is_some(), "{stage:?}: worker still alive");
            drop(child);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            let after = loop {
                let report = recover(data.path(), 128).unwrap();
                if report.running == 0 {
                    break report;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "{stage:?}: {report:?}"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            };
            if stage == CreationStage::HeaderSynced {
                assert_eq!(after.removed, 1, "{after:?}");
                assert!(!ready.exists());
            } else {
                assert_eq!(after.failed, 1, "{stage:?}: {after:?}");
                assert!(ready.exists());
                if matches!(stage, CreationStage::WorkRoot | CreationStage::JournalFile) {
                    // This child created the isolated empty fixture, but no
                    // durable ownership identity grants the reaper its deletion.
                    assert_eq!(fs::read_dir(&ready).unwrap().count(), 0);
                    fs::remove_dir(&ready).unwrap();
                }
            }
            println!("CREATION_BOUNDARY {stage:?} {after:?}");
        }
    }
    #[test]
    fn known_files_reclaimed_unknown_and_replacements_preserved() {
        let data = tempfile::tempdir().unwrap();
        let work = Workspace::create(data.path(), "test-owned").unwrap();
        let root = work.path().to_owned();
        work.scope(|| {
            write(&root.join("known.txt"), b"temporary").unwrap();
            write(&root.join("replaced.txt"), b"old").unwrap();
        });
        fs::remove_file(root.join("replaced.txt")).unwrap();
        fs::write(root.join("replaced.txt"), b"User replacement").unwrap();
        fs::write(root.join("unknown.txt"), b"Keep user file").unwrap();
        drop(work);
        assert!(!root.join("known.txt").exists());
        assert_eq!(
            fs::read(root.join("replaced.txt")).unwrap(),
            b"User replacement"
        );
        assert_eq!(
            fs::read(root.join("unknown.txt")).unwrap(),
            b"Keep user file"
        );
        assert_eq!(recover(data.path(), 32).unwrap().preserved, 1);
        // Test owns this isolated root; remove only its explicitly named files.
        fs::remove_file(root.join("replaced.txt")).unwrap();
        fs::remove_file(root.join("unknown.txt")).unwrap();
        assert_eq!(recover(data.path(), 32).unwrap().removed, 1);
        assert!(!root.exists());
    }
    #[test]
    fn live_lease_and_recovery_reference_prevent_cleanup() {
        let data = tempfile::tempdir().unwrap();
        let work = Workspace::create(data.path(), "test-pinned").unwrap();
        let root = work.path().to_owned();
        let id = work.id();
        work.scope(|| write(&root.join("needed.txt"), b"Recovery baseline").unwrap());
        let revision = work.pin("review-original-1").unwrap();
        assert_eq!(recover(data.path(), 32).unwrap().running, 1);
        drop(work);
        assert_eq!(recover(data.path(), 32).unwrap().retained, 1);
        assert!(root.join("needed.txt").exists());
        assert!(release(data.path(), &id, revision + 1, "review-original-1").is_err());
        assert!(root.join("needed.txt").exists());
        assert_eq!(
            release(data.path(), &id, revision, "review-original-1").unwrap(),
            revision + 1
        );
        assert!(release(data.path(), &id, revision, "review-original-1").is_err());
        assert_eq!(recover(data.path(), 32).unwrap().removed, 1);
        assert!(!root.exists());
    }
    #[test]
    fn atomic_content_is_tracked_without_document_directory_files() {
        let data = tempfile::tempdir().unwrap();
        let work = Workspace::create(data.path(), "test-atomic").unwrap();
        let root = work.path().to_owned();
        work.scope(|| {
            crate::content::write_bytes_atomic(&root.join("result.txt"), b"Atomic result").unwrap()
        });
        assert_eq!(fs::read(root.join("result.txt")).unwrap(), b"Atomic result");
        drop(work);
        assert!(!root.exists());
    }
    #[test]
    fn task_output_refuses_overwriting_a_foreign_file() {
        let data = tempfile::tempdir().unwrap();
        let work = Workspace::create(data.path(), "test-foreign-output").unwrap();
        let root = work.path().to_owned();
        fs::write(root.join("output.txt"), b"Keep user content").unwrap();
        assert!(work
            .scope(|| crate::content::write_bytes_atomic(&root.join("output.txt"), b"replacement"))
            .is_err());
        drop(work);
        assert_eq!(
            fs::read(root.join("output.txt")).unwrap(),
            b"Keep user content"
        );
        fs::remove_file(root.join("output.txt")).unwrap();
        assert_eq!(recover(data.path(), 32).unwrap().removed, 1);
    }
    #[test]
    fn nested_scopes_restore_after_panics_and_track_directories() {
        let data = tempfile::tempdir().unwrap();
        let a = Workspace::create(data.path(), "test-outer").unwrap();
        let b = Workspace::create(data.path(), "test-inner").unwrap();
        let roots = (a.path().to_owned(), b.path().to_owned());
        a.scope(|| {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                b.scope(|| {
                    create_dir(&b.path().join("images")).unwrap();
                    write(&b.path().join("images/diagram.svg"), b"temporary").unwrap();
                    panic!("test scoped restore");
                })
            }));
            write(&a.path().join("outer.txt"), b"temporary").unwrap();
        });
        drop((a, b));
        assert!(!roots.0.exists());
        assert!(!roots.1.exists());
    }
    #[test]
    fn malformed_records_and_incomplete_paths_never_grant_cleanup() {
        let data = tempfile::tempdir().unwrap();
        let work = Workspace::create(data.path(), "test-record").unwrap();
        let id = work.id();
        let root = work.path().to_owned();
        work.scope(|| write(&root.join("known.txt"), b"temporary").unwrap());
        let pin = work.pin("keep-for-test").unwrap();
        drop(work);
        let journal = journal_root(data.path(), false)
            .unwrap()
            .join(&id)
            .join("record.jsonl");
        OpenOptions::new()
            .append(true)
            .open(&journal)
            .unwrap()
            .write_all(b"{\"type\":\"Created\",\"path\":\"unknown.txt")
            .unwrap();
        fs::write(root.join("unknown.txt"), b"Keep incomplete instruction").unwrap();
        release(data.path(), &id, pin, "keep-for-test").unwrap();
        assert_eq!(recover(data.path(), 32).unwrap().preserved, 1);
        assert!(!root.join("known.txt").exists());
        assert!(root.join("unknown.txt").exists());
        fs::remove_file(root.join("unknown.txt")).unwrap();
        assert_eq!(recover(data.path(), 32).unwrap().removed, 1);
        for value in [
            "../outside",
            "a/../../b",
            "/absolute",
            "a\\b",
            "C:relative",
            "",
        ] {
            assert!(relative(value).is_err());
        }
    }
    #[test]
    fn interrupted_marker_cleanup_resumes_without_recursive_deletion() {
        let data = tempfile::tempdir().unwrap();
        let work = Workspace::create(data.path(), "test-marker").unwrap();
        let id = work.id();
        let root = work.path().to_owned();
        let revision = work.pin("keep-for-test").unwrap();
        drop(work);
        release(data.path(), &id, revision, "keep-for-test").unwrap();
        fs::remove_dir(&root).unwrap();
        let dir = journal_root(data.path(), false).unwrap().join(id);
        fs::remove_file(dir.join("owner.lock")).unwrap();
        assert_eq!(recover(data.path(), 32).unwrap().removed, 1);
        assert!(!dir.exists());
    }
    #[test]
    fn locked_file_is_preserved_then_reclaimed_on_a_later_pass() {
        use std::os::windows::fs::OpenOptionsExt;
        let data = tempfile::tempdir().unwrap();
        let work = Workspace::create(data.path(), "test-file-lock").unwrap();
        let root = work.path().to_owned();
        work.scope(|| write(&root.join("output.bin"), b"temporary").unwrap());
        let file = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(root.join("output.bin"))
            .unwrap();
        drop(work);
        assert_eq!(recover(data.path(), 32).unwrap().preserved, 1);
        assert!(root.join("output.bin").exists());
        drop(file);
        assert_eq!(recover(data.path(), 32).unwrap().removed, 1);
        assert!(!root.exists());
    }
    #[test]
    fn replaced_directory_is_never_traversed() {
        let data = tempfile::tempdir().unwrap();
        let work = Workspace::create(data.path(), "test-parent-replaced").unwrap();
        let root = work.path().to_owned();
        work.scope(|| {
            create_dir(&root.join("images")).unwrap();
            write(&root.join("images/image.svg"), b"temporary").unwrap();
        });
        fs::rename(root.join("images"), root.join("moved-original")).unwrap();
        fs::create_dir(root.join("images")).unwrap();
        fs::write(root.join("images/image.svg"), b"User replacement").unwrap();
        drop(work);
        assert_eq!(
            fs::read(root.join("images/image.svg")).unwrap(),
            b"User replacement"
        );
        assert!(root.join("moved-original/image.svg").exists());
        fs::remove_file(root.join("images/image.svg")).unwrap();
        fs::remove_dir(root.join("images")).unwrap();
        fs::remove_file(root.join("moved-original/image.svg")).unwrap();
        fs::remove_dir(root.join("moved-original")).unwrap();
        assert_eq!(recover(data.path(), 32).unwrap().removed, 1);
    }
    #[test]
    fn lease_child() {
        if let Some(path) = std::env::var_os("READMD_TASK_LEASE_CHILD") {
            let error = owner(Path::new(&path), false).unwrap_err();
            assert_eq!(error.raw_os_error(), Some(32));
            println!("LEASE_HELD");
            std::thread::sleep(std::time::Duration::from_secs(30));
        }
    }
    #[test]
    fn inherited_worker_lease_prevents_cleanup_after_parent_scope_ends() {
        let data = tempfile::tempdir().unwrap();
        let work = Workspace::create(data.path(), "test-child-lease").unwrap();
        let root = work.path().to_owned();
        let mut child = work.scope(|| {
            write(&root.join("input.bin"), b"temporary").unwrap();
            assert!(current_lease(&root).unwrap().is_some());
            let log = File::create(root.join("worker.log")).unwrap();
            track_created(&root.join("worker.log"), &log).unwrap();
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "task_workspace::tests::lease_child",
                    "--nocapture",
                ])
                .env(
                    "READMD_TASK_LEASE_CHILD",
                    work.inner.journal_dir.join("owner.lock"),
                );
            crate::worker_sandbox::spawn_managed(&mut command, &root, &log, &log).unwrap()
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while std::time::Instant::now() < deadline
            && !fs::read_to_string(root.join("worker.log"))
                .unwrap()
                .contains("LEASE_HELD")
            && child.try_wait().unwrap().is_none()
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(fs::read_to_string(root.join("worker.log"))
            .unwrap()
            .contains("LEASE_HELD"));
        assert!(child.try_wait().unwrap().is_none());
        drop(work);
        let report = recover(data.path(), 32).unwrap();
        assert_eq!(report.running, 1, "{report:?}");
        assert!(root.join("input.bin").exists());
        child.stop().unwrap();
        drop(child);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut reports = Vec::new();
        while root.exists() && std::time::Instant::now() < deadline {
            reports.push(recover(data.path(), 32).unwrap());
            if root.exists() {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        assert!(
            !root.exists(),
            "Post-stop recovery: {reports:?}; remaining: {:?}",
            fs::read_dir(&root).map(|items| items
                .filter_map(Result::ok)
                .map(|e| e.file_name())
                .collect::<Vec<_>>())
        );
        assert!(!root.exists());
    }
}
