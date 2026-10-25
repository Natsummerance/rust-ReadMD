//! Finished-task reclamation on Linux/macOS. Every lookup is descriptor-relative
//! and refuses links. A private, never-shared quarantine captures a leaf before
//! its recorded identity is checked again. Unknown or interrupted state stays
//! retained; there is no recursive delete or overwrite fallback.
use super::*;
use std::{
    ffi::{CString, OsStr},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{ffi::OsStrExt, fs::MetadataExt},
    },
};

const DIRECTORY: i32 = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;

fn name(value: &OsStr) -> io::Result<CString> {
    if value.as_bytes().is_empty()
        || value.as_bytes().contains(&b'/')
        || value == "."
        || value == ".."
    {
        return Err(invalid());
    }
    CString::new(value.as_bytes()).map_err(|_| invalid())
}
fn descriptor(result: i32) -> io::Result<File> {
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { File::from_raw_fd(result) })
    }
}
fn open_at(parent: &File, leaf: &OsStr, flags: i32) -> io::Result<File> {
    let leaf = name(leaf)?;
    let file = descriptor(unsafe {
        libc::openat(
            parent.as_raw_fd(),
            leaf.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
            0o600,
        )
    })?;
    let metadata = file.metadata()?;
    if !metadata.is_file() && !metadata.is_dir() {
        return Err(invalid());
    }
    Ok(file)
}

struct Directory {
    path: PathBuf,
    // Keeping every ancestor open also prevents inode reuse while validating
    // the original chain against its present names. Unix handles do not forbid
    // rename as Windows delete-sharing guards do.
    anchors: Vec<File>,
    identities: Vec<Identity>,
}
impl Directory {
    fn open(path: &Path) -> io::Result<Self> {
        if !path.is_absolute() {
            return Err(invalid());
        }
        let root = descriptor(unsafe { libc::open(c"/".as_ptr(), DIRECTORY) })?;
        let mut result = Self {
            path: PathBuf::from("/"),
            identities: vec![identity_file(&root, &Kind::Directory)?],
            anchors: vec![root],
        };
        for component in path.components() {
            match component {
                Component::RootDir => (),
                Component::Normal(leaf) => result = result.child(leaf)?,
                _ => return Err(invalid()),
            }
        }
        Ok(result)
    }
    fn file(&self) -> &File {
        self.anchors.last().unwrap()
    }
    fn identity(&self) -> &Identity {
        self.identities.last().unwrap()
    }
    fn child(&self, leaf: &OsStr) -> io::Result<Self> {
        let file = open_at(self.file(), leaf, DIRECTORY)?;
        let mut anchors = self
            .anchors
            .iter()
            .map(File::try_clone)
            .collect::<io::Result<Vec<_>>>()?;
        let mut identities = self.identities.clone();
        identities.push(identity_file(&file, &Kind::Directory)?);
        anchors.push(file);
        Ok(Self {
            path: self.path.join(leaf),
            anchors,
            identities,
        })
    }
    fn still_named(&self) -> bool {
        Self::open(&self.path).is_ok_and(|current| current.identities == self.identities)
    }
}

pub(super) fn open_directory(path: &Path) -> io::Result<File> {
    Directory::open(path)?.file().try_clone()
}
pub(super) fn open_file(path: &Path, writable: bool, create: bool) -> io::Result<File> {
    let parent = Directory::open(path.parent().ok_or_else(invalid)?)?;
    let flags = (if writable {
        libc::O_RDWR
    } else {
        libc::O_RDONLY
    }) | (if create {
        libc::O_CREAT | libc::O_EXCL
    } else {
        0
    });
    let file = open_at(parent.file(), path.file_name().ok_or_else(invalid)?, flags)?;
    identity_file(&file, &Kind::File)?;
    Ok(file)
}
pub(super) fn open_identity(path: &Path) -> io::Result<File> {
    if path == Path::new("/") {
        return open_directory(path);
    }
    let parent = Directory::open(path.parent().ok_or_else(invalid)?)?;
    open_at(
        parent.file(),
        path.file_name().ok_or_else(invalid)?,
        libc::O_RDONLY,
    )
}
pub(super) fn birth_identity(file: &File, metadata: &fs::Metadata) -> u64 {
    #[cfg(target_os = "linux")]
    {
        // File::metadata may use fstat, which has no birth field on Linux.
        // statx(AT_EMPTY_PATH) queries the same opened inode, never a new path.
        let mut attributes = std::mem::MaybeUninit::<libc::statx>::zeroed();
        if unsafe {
            libc::statx(
                file.as_raw_fd(),
                c"".as_ptr(),
                libc::AT_EMPTY_PATH | libc::AT_SYMLINK_NOFOLLOW,
                libc::STATX_BTIME,
                attributes.as_mut_ptr(),
            )
        } == 0
        {
            let attributes = unsafe { attributes.assume_init() };
            if attributes.stx_mask & libc::STATX_BTIME != 0 && attributes.stx_btime.tv_sec >= 0 {
                return (attributes.stx_btime.tv_sec as u64)
                    .checked_mul(1_000_000_000)
                    .and_then(|seconds| seconds.checked_add(attributes.stx_btime.tv_nsec as u64))
                    .unwrap_or(0);
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = file;
    metadata
        .created()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|duration| u64::try_from(duration.as_nanos()).ok())
        .unwrap_or(0)
}

fn rename_noreplace(
    from: &Directory,
    source: &OsStr,
    to: &Directory,
    target: &OsStr,
) -> io::Result<()> {
    let source = name(source)?;
    let target = name(target)?;
    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::renameat2(
            from.file().as_raw_fd(),
            source.as_ptr(),
            to.file().as_raw_fd(),
            target.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    #[cfg(target_os = "macos")]
    let result = unsafe {
        libc::renameatx_np(
            from.file().as_raw_fd(),
            source.as_ptr(),
            to.file().as_raw_fd(),
            target.as_ptr(),
            libc::RENAME_EXCL,
        )
    };
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    return Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Task quarantine requires atomic no-replace rename",
    ));
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
fn unlink(parent: &Directory, leaf: &OsStr, kind: &Kind) -> io::Result<()> {
    let leaf = name(leaf)?;
    let flags = if *kind == Kind::Directory {
        libc::AT_REMOVEDIR
    } else {
        0
    };
    if unsafe { libc::unlinkat(parent.file().as_raw_fd(), leaf.as_ptr(), flags) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
fn matches_at(parent: &Directory, leaf: &OsStr, identities: &[Identity], kind: &Kind) -> bool {
    open_at(
        parent.file(),
        leaf,
        if *kind == Kind::Directory {
            DIRECTORY
        } else {
            libc::O_RDONLY
        },
    )
    .and_then(|file| identity_file(&file, kind))
    // Without birth identity, a previously unlinked inode could have been
    // reused before recovery opened it. Such filesystems retain the task.
    .is_ok_and(|actual| actual.created != 0 && identities.contains(&actual))
}
fn absent_at(parent: &Directory, leaf: &OsStr) -> bool {
    open_at(parent.file(), leaf, libc::O_RDONLY)
        .is_err_and(|error| error.kind() == io::ErrorKind::NotFound)
}
fn quarantine(parent: &Directory) -> io::Result<(std::ffi::OsString, Directory)> {
    // A bare UUID also occupies a conservative registry reservation if the
    // final WAL is captured here when the process dies. Its prefix/name alone
    // never authorizes recovery to adopt or delete the unknown directory.
    let leaf = std::ffi::OsString::from(uuid::Uuid::new_v4().simple().to_string());
    let raw = name(&leaf)?;
    if unsafe { libc::mkdirat(parent.file().as_raw_fd(), raw.as_ptr(), 0o700) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let directory = parent.child(&leaf)?;
    let metadata = directory.file().metadata()?;
    if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
        return Err(invalid());
    }
    Ok((leaf, directory))
}
fn remove_quarantine(parent: &Directory, leaf: &OsStr, directory: &Directory) -> bool {
    if !parent.still_named()
        || !directory.still_named()
        || !matches_at(
            parent,
            leaf,
            std::slice::from_ref(directory.identity()),
            &Kind::Directory,
        )
    {
        return false;
    }
    unlink(parent, leaf, &Kind::Directory).is_ok()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RemovalStage {
    BeforeRename,
    Isolated,
    BeforeUnlink,
}
fn remove_at(
    parent: &Directory,
    leaf: &OsStr,
    identities: &[Identity],
    kind: &Kind,
    checkpoint: &mut impl FnMut(RemovalStage, &Path, &Path) -> io::Result<()>,
) -> io::Result<bool> {
    if !matches!(kind, Kind::File | Kind::Directory) || !parent.still_named() {
        return Ok(false);
    }
    let original = match open_at(
        parent.file(),
        leaf,
        if *kind == Kind::Directory {
            DIRECTORY
        } else {
            libc::O_RDONLY
        },
    ) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(true),
        Err(_) => return Ok(false),
    };
    let identity = identity_file(&original, kind)?;
    if identity.created == 0 || !identities.contains(&identity) {
        return Ok(false);
    }
    let path = parent.path.join(leaf);
    checkpoint(RemovalStage::BeforeRename, &path, &path)?;
    if !parent.still_named() {
        return Ok(false);
    }
    let (quarantine_name, isolated) = quarantine(parent)?;
    let captured = OsStr::new("entry");
    if let Err(error) = rename_noreplace(parent, leaf, &isolated, captured) {
        remove_quarantine(parent, &quarantine_name, &isolated);
        return if error.kind() == io::ErrorKind::NotFound {
            Ok(true)
        } else {
            Ok(false)
        };
    }
    // This path is never handed to an engine. On process death, its unknown
    // directory remains inside the retained task rather than being re-adopted.
    let captured_path = isolated.path.join(captured);
    checkpoint(RemovalStage::Isolated, &path, &captured_path)?;
    let restore = || {
        // Never overwrite a replacement installed at the original name. On
        // collision, preserve both it and the captured object for inspection.
        if rename_noreplace(&isolated, captured, parent, leaf).is_ok() {
            remove_quarantine(parent, &quarantine_name, &isolated);
        }
        false
    };
    if !parent.still_named()
        || !isolated.still_named()
        || !absent_at(parent, leaf)
        || !matches_at(&isolated, captured, std::slice::from_ref(&identity), kind)
    {
        return Ok(restore());
    }
    checkpoint(RemovalStage::BeforeUnlink, &path, &captured_path)?;
    if !parent.still_named()
        || !isolated.still_named()
        || !absent_at(parent, leaf)
        || !matches_at(&isolated, captured, std::slice::from_ref(&identity), kind)
    {
        return Ok(restore());
    }
    if unlink(&isolated, captured, kind).is_err() {
        return Ok(restore());
    }
    Ok(remove_quarantine(parent, &quarantine_name, &isolated))
}

pub(super) fn remove_identified(
    path: &Path,
    identities: &[Identity],
    kind: &Kind,
    target: Option<&Path>,
) -> io::Result<bool> {
    if target.is_some() || *kind == Kind::Link {
        return Ok(false);
    }
    let parent = match Directory::open(path.parent().ok_or_else(invalid)?) {
        Ok(parent) => parent,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(true),
        Err(_) => return Ok(false),
    };
    remove_at(
        &parent,
        path.file_name().ok_or_else(invalid)?,
        identities,
        kind,
        &mut |_, _, _| Ok(()),
    )
}

pub(super) fn reclaim_work(record: &Record) -> io::Result<bool> {
    if !record.finished || !record.refs.is_empty() || !record.resources.is_empty() {
        return Ok(false);
    }
    let root = match Directory::open(&record.header.root) {
        Ok(root) => root,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(true),
        Err(_) => return Ok(false),
    };
    if root.identity() != &record.header.identity {
        return Ok(false);
    }
    let mut entries = record.entries.iter().collect::<Vec<_>>();
    entries.sort_by(|(a, x), (b, y)| {
        (x.kind != Kind::File)
            .cmp(&(y.kind != Kind::File))
            .then_with(|| b.len().cmp(&a.len()))
    });
    for (path, entry) in entries {
        if entry.kind == Kind::Link || entry.target.is_some() {
            continue;
        }
        let path = relative(path)?;
        let mut parent = Directory::open(&record.header.root)?;
        let mut key = PathBuf::new();
        let mut accepted = parent.identities == root.identities;
        for component in path.parent().unwrap_or(Path::new("")).components() {
            let Component::Normal(leaf) = component else {
                accepted = false;
                break;
            };
            key.push(leaf);
            let Some(known) = key
                .to_str()
                .and_then(|key| record.entries.get(key))
                .filter(|entry| entry.kind == Kind::Directory)
            else {
                accepted = false;
                break;
            };
            let Ok(child) = parent.child(leaf) else {
                accepted = false;
                break;
            };
            if !known.identities.contains(child.identity()) {
                accepted = false;
                break;
            }
            parent = child;
        }
        if accepted {
            remove_at(
                &parent,
                path.file_name().ok_or_else(invalid)?,
                &entry.identities,
                &entry.kind,
                &mut |_, _, _| Ok(()),
            )?;
        }
    }
    if !root.still_named() {
        return Ok(false);
    }
    remove_identified(
        &record.header.root,
        std::slice::from_ref(&record.header.identity),
        &Kind::Directory,
        None,
    )
}

pub(super) fn link_journal(source: &Path, witness: &Path, retiring: &Retirement) -> io::Result<()> {
    let source = Directory::open(source)?;
    let destination = Directory::open(witness.parent().ok_or_else(invalid)?)?;
    if source.identity() != &retiring.directory
        || !source.still_named()
        || !matches_at(
            &source,
            OsStr::new("record.jsonl"),
            std::slice::from_ref(&retiring.journal),
            &Kind::File,
        )
    {
        return Err(invalid());
    }
    let from = c"record.jsonl";
    let to = name(witness.file_name().ok_or_else(invalid)?)?;
    // linkat without AT_SYMLINK_FOLLOW never dereferences a replaced leaf.
    if unsafe {
        libc::linkat(
            source.file().as_raw_fd(),
            from.as_ptr(),
            destination.file().as_raw_fd(),
            to.as_ptr(),
            0,
        )
    } == 0
    {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, FileTypeExt, PermissionsExt};

    fn fresh_file(path: &Path, bytes: &[u8]) -> Identity {
        fs::write(path, bytes).unwrap();
        let identity = identity_path(path, Kind::File).unwrap();
        assert_ne!(
            identity.created, 0,
            "A filesystem without birth identity must retain tasks"
        );
        identity
    }
    fn known_task(data: &Path) -> Workspace {
        let work = Workspace::create(data, "test-unix-finished").unwrap();
        work.scope(|| {
            create_dir(&work.path().join("nested")).unwrap();
            write(
                &work.path().join("nested/result.txt"),
                b"Owned nested output",
            )
            .unwrap();
            write(&work.path().join("result.txt"), b"Owned root output").unwrap();
        });
        work
    }
    fn assert_no_reservation(data: &Path) {
        let root = journal_root(data, false).unwrap();
        let names = registry::candidates(&root).unwrap();
        assert!(
            names.is_empty(),
            "Unreclaimed task or quarantine: {names:?}"
        );
        assert_eq!(
            registry::reserved(&root, &names, &profile(data).unwrap()).unwrap(),
            0
        );
    }

    #[test]
    fn finished_files_and_nested_directories_are_reclaimed_without_accumulating_budget() {
        let data = tempfile::tempdir().unwrap();
        // Twelve real finish/drop cycles exceed the former eight-task ceiling.
        for _ in 0..12 {
            let work = known_task(data.path());
            let root = work.path().to_owned();
            assert_eq!(
                fs::metadata(&root).unwrap().permissions().mode() & 0o777,
                0o700
            );
            let journal = journal_root(data.path(), false).unwrap().join(work.id());
            assert_eq!(
                fs::metadata(journal).unwrap().permissions().mode() & 0o777,
                0o700
            );
            work.finish().unwrap();
            drop(work);
            assert!(!root.exists());
            assert_no_reservation(data.path());
        }
    }
    #[test]
    fn creation_keeps_preexisting_directory_permissions_unchanged() {
        let data = tempfile::tempdir().unwrap();
        let canonical = data.path().canonicalize().unwrap();
        let journal = canonical.join(PROTOCOL);
        fs::create_dir(&journal).unwrap();
        fs::set_permissions(&journal, fs::Permissions::from_mode(0o750)).unwrap();
        let temp = std::env::temp_dir().canonicalize().unwrap();
        let parent = temp.join(format!("rmdw-{}", &profile(&canonical).unwrap()[..12]));
        fs::create_dir(&parent).unwrap();
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o750)).unwrap();
        let work = Workspace::create(&canonical, "test-unix-permissions").unwrap();
        assert_eq!(
            fs::metadata(&journal).unwrap().permissions().mode() & 0o777,
            0o750
        );
        assert_eq!(
            fs::metadata(&parent).unwrap().permissions().mode() & 0o777,
            0o750
        );
        assert_eq!(
            fs::metadata(work.path()).unwrap().permissions().mode() & 0o777,
            0o700
        );
        work.finish().unwrap();
        drop(work);
        fs::remove_dir(parent).unwrap();
    }
    #[test]
    fn finished_pins_and_unfinished_crashes_remain_retained() {
        let data = tempfile::tempdir().unwrap();
        let work = known_task(data.path());
        let root = work.path().to_owned();
        let id = work.id();
        let revision = work.pin("unix-review").unwrap();
        work.finish().unwrap();
        assert_eq!(recover(data.path(), 32).unwrap().running, 1);
        drop(work);
        assert_eq!(recover(data.path(), 32).unwrap().retained, 1);
        assert_eq!(
            fs::read(root.join("result.txt")).unwrap(),
            b"Owned root output"
        );
        release(data.path(), &id, revision, "unix-review").unwrap();
        assert_eq!(recover(data.path(), 32).unwrap().removed, 1);
        assert_no_reservation(data.path());

        let unfinished = Workspace::create(data.path(), "test-unix-unfinished").unwrap();
        let root = unfinished.path().to_owned();
        unfinished.scope(|| write(&root.join("needed.txt"), b"Unfinished output").unwrap());
        drop(unfinished);
        assert_eq!(recover(data.path(), 32).unwrap().retained, 1);
        assert_eq!(
            fs::read(root.join("needed.txt")).unwrap(),
            b"Unfinished output"
        );
        // Explicit fixture cleanup does not grant production recovery new rights.
        fs::remove_file(root.join("needed.txt")).unwrap();
        fs::remove_dir(root).unwrap();
    }
    #[test]
    fn unknown_files_keep_the_task_and_their_identity_until_explicitly_removed() {
        let data = tempfile::tempdir().unwrap();
        let work = known_task(data.path());
        let root = work.path().to_owned();
        let foreign = root.join("foreign.txt");
        let identity = fresh_file(&foreign, b"Keep foreign bytes");
        work.finish().unwrap();
        drop(work);
        assert!(!root.join("result.txt").exists());
        assert!(!root.join("nested").exists());
        assert_eq!(identity_path(&foreign, Kind::File).unwrap(), identity);
        assert_eq!(fs::read(&foreign).unwrap(), b"Keep foreign bytes");
        assert_eq!(recover(data.path(), 32).unwrap().preserved, 1);
        fs::remove_file(foreign).unwrap();
        assert_eq!(recover(data.path(), 32).unwrap().removed, 1);
        assert_no_reservation(data.path());
    }
    #[test]
    fn unknown_journal_files_retain_both_retirement_wals_and_one_reservation() {
        let data = tempfile::tempdir().unwrap();
        let work = known_task(data.path());
        let id = work.id();
        let revision = work.pin("unix-journal-unknown").unwrap();
        work.finish().unwrap();
        drop(work);
        let root = journal_root(data.path(), false).unwrap();
        let journal = root.join(&id);
        let foreign = journal.join("foreign.txt");
        let identity = fresh_file(&foreign, b"Foreign journal content");
        release(data.path(), &id, revision, "unix-journal-unknown").unwrap();
        assert_eq!(recover(data.path(), 32).unwrap().preserved, 1);
        assert_eq!(identity_path(&foreign, Kind::File).unwrap(), identity);
        assert_eq!(
            identity_path(&journal.join("record.jsonl"), Kind::File).unwrap(),
            identity_path(&witness_path(&root, &id), Kind::File).unwrap()
        );
        let names = registry::candidates(&root).unwrap();
        assert_eq!(names, vec![id]);
        assert_eq!(
            registry::reserved(&root, &names, &profile(data.path()).unwrap()).unwrap(),
            registry::default_budget()
        );
        fs::remove_file(foreign).unwrap();
        assert_eq!(recover(data.path(), 32).unwrap().removed, 1);
        assert_no_reservation(data.path());
    }
    #[test]
    fn replacement_files_and_symlinked_ancestors_are_never_followed_or_deleted() {
        for link in [false, true] {
            let data = tempfile::tempdir().unwrap();
            let work = Workspace::create(data.path(), "test-unix-foreign").unwrap();
            let root = work.path().to_owned();
            work.scope(|| {
                create_dir(&root.join("nested")).unwrap();
                write(&root.join("nested/result.txt"), b"Owned input").unwrap();
            });
            fs::rename(root.join("nested"), root.join("moved-original")).unwrap();
            let outside = data.path().join("foreign");
            fs::create_dir(&outside).unwrap();
            let outside = outside.canonicalize().unwrap();
            let foreign = outside.join("result.txt");
            let identity = fresh_file(&foreign, b"Foreign ancestor output");
            if link {
                symlink(&outside, root.join("nested")).unwrap();
            } else {
                fs::create_dir(root.join("nested")).unwrap();
                fs::hard_link(&foreign, root.join("nested/result.txt")).unwrap();
            }
            work.finish().unwrap();
            drop(work);
            assert_eq!(fs::read(&foreign).unwrap(), b"Foreign ancestor output");
            assert_eq!(identity_path(&foreign, Kind::File).unwrap(), identity);
            assert_eq!(
                fs::read(root.join("nested/result.txt")).unwrap(),
                b"Foreign ancestor output"
            );
            assert_eq!(
                fs::read(root.join("moved-original/result.txt")).unwrap(),
                b"Owned input"
            );
            assert_eq!(recover(data.path(), 32).unwrap().preserved, 1);
            if link {
                fs::remove_file(root.join("nested")).unwrap();
            } else {
                fs::remove_file(root.join("nested/result.txt")).unwrap();
                fs::remove_dir(root.join("nested")).unwrap();
            }
            fs::remove_file(root.join("moved-original/result.txt")).unwrap();
            fs::remove_dir(root.join("moved-original")).unwrap();
            assert_eq!(recover(data.path(), 32).unwrap().removed, 1);
        }
    }
    #[test]
    fn leaf_symlinks_special_files_and_stale_file_identities_are_refused() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().canonicalize().unwrap();
        let leaf = root.join("owned");
        let identity = fresh_file(&leaf, b"Original");
        fs::rename(&leaf, root.join("original")).unwrap();
        let replacement = fresh_file(&leaf, b"Foreign replacement");
        assert!(!remove_identified(&leaf, &[identity.clone()], &Kind::File, None).unwrap());
        assert_eq!(identity_path(&leaf, Kind::File).unwrap(), replacement);
        fs::remove_file(&leaf).unwrap();
        symlink(root.join("original"), &leaf).unwrap();
        assert!(!remove_identified(&leaf, &[identity], &Kind::File, None).unwrap());
        assert_eq!(fs::read(root.join("original")).unwrap(), b"Original");
        fs::remove_file(&leaf).unwrap();
        let raw = CString::new(leaf.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(raw.as_ptr(), 0o600) }, 0);
        assert!(open_identity(&leaf).is_err());
        assert!(fs::symlink_metadata(&leaf).unwrap().file_type().is_fifo());
    }
    #[test]
    fn checkpoints_cannot_redirect_cleanup_through_a_replaced_ancestor() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().canonicalize().unwrap();
        let parent = root.join("parent");
        let outside = root.join("outside");
        fs::create_dir(&parent).unwrap();
        fs::create_dir(&outside).unwrap();
        let identity = fresh_file(&parent.join("owned"), b"Owned output");
        let foreign = fresh_file(&outside.join("owned"), b"Foreign output");
        let directory = Directory::open(&parent).unwrap();
        let removed = remove_at(
            &directory,
            OsStr::new("owned"),
            &[identity],
            &Kind::File,
            &mut |stage, _, _| {
                if stage == RemovalStage::BeforeRename {
                    fs::rename(&parent, root.join("moved-parent"))?;
                    symlink(&outside, &parent)?;
                }
                Ok(())
            },
        )
        .unwrap();
        assert!(!removed);
        assert_eq!(
            fs::read(root.join("moved-parent/owned")).unwrap(),
            b"Owned output"
        );
        assert_eq!(
            identity_path(&outside.join("owned"), Kind::File).unwrap(),
            foreign
        );
        assert_eq!(fs::read(outside.join("owned")).unwrap(), b"Foreign output");
    }
    #[test]
    fn replacement_captured_between_check_and_rename_is_restored_without_deletion() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().canonicalize().unwrap();
        let path = root.join("owned");
        let identity = fresh_file(&path, b"Owned output");
        let directory = Directory::open(&root).unwrap();
        let mut foreign = None;
        assert!(!remove_at(
            &directory,
            OsStr::new("owned"),
            &[identity],
            &Kind::File,
            &mut |stage, _, _| {
                if stage == RemovalStage::BeforeRename {
                    fs::rename(&path, root.join("saved-original"))?;
                    foreign = Some(fresh_file(&path, b"Foreign replacement"));
                }
                Ok(())
            }
        )
        .unwrap());
        assert_eq!(identity_path(&path, Kind::File).unwrap(), foreign.unwrap());
        assert_eq!(fs::read(&path).unwrap(), b"Foreign replacement");
        assert_eq!(
            fs::read(root.join("saved-original")).unwrap(),
            b"Owned output"
        );
    }
    #[test]
    fn reoccupied_names_and_errors_preserve_both_foreign_and_quarantined_objects() {
        for fail in [false, true] {
            let fixture = tempfile::tempdir().unwrap();
            let root = fixture.path().canonicalize().unwrap();
            let path = root.join("owned");
            let identity = fresh_file(&path, b"Owned output");
            let directory = Directory::open(&root).unwrap();
            let mut captured = None;
            let result = remove_at(
                &directory,
                OsStr::new("owned"),
                &[identity],
                &Kind::File,
                &mut |stage, original, isolated| {
                    if stage == RemovalStage::Isolated {
                        captured = Some(isolated.to_owned());
                        fs::write(original, b"Foreign reoccupied name")?;
                        if fail {
                            return Err(io::Error::other("Injected ordinary cleanup error"));
                        }
                    }
                    Ok(())
                },
            );
            if fail {
                assert!(result.is_err());
            } else {
                assert!(!result.unwrap());
            }
            let captured = captured.unwrap();
            assert_eq!(fs::read(path).unwrap(), b"Foreign reoccupied name");
            assert_eq!(fs::read(&captured).unwrap(), b"Owned output");
            assert_eq!(
                fs::metadata(captured.parent().unwrap())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
    }
    #[test]
    fn a_replaced_quarantined_leaf_is_rechecked_before_unlink() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().canonicalize().unwrap();
        let identity = fresh_file(&root.join("owned"), b"Owned output");
        let directory = Directory::open(&root).unwrap();
        let mut saved = None;
        assert!(!remove_at(
            &directory,
            OsStr::new("owned"),
            &[identity],
            &Kind::File,
            &mut |stage, _, captured| {
                if stage == RemovalStage::BeforeUnlink {
                    let original = captured.with_file_name("saved-original");
                    fs::rename(captured, &original)?;
                    saved = Some(original);
                    fs::write(captured, b"Foreign quarantine replacement")?;
                }
                Ok(())
            }
        )
        .unwrap());
        assert_eq!(
            fs::read(root.join("owned")).unwrap(),
            b"Foreign quarantine replacement"
        );
        assert_eq!(fs::read(saved.unwrap()).unwrap(), b"Owned output");
    }
    #[test]
    fn no_replace_rename_never_overwrites_an_existing_destination() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().canonicalize().unwrap();
        fresh_file(&root.join("source"), b"Source");
        let foreign = fresh_file(&root.join("destination"), b"Keep destination");
        let directory = Directory::open(&root).unwrap();
        assert_eq!(
            rename_noreplace(
                &directory,
                OsStr::new("source"),
                &directory,
                OsStr::new("destination")
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read(root.join("source")).unwrap(), b"Source");
        assert_eq!(
            identity_path(&root.join("destination"), Kind::File).unwrap(),
            foreign
        );
    }
    #[test]
    fn finished_retirement_wal_resumes_after_each_synchronous_interruption() {
        for boundary in [
            RetirementStage::IntentSynced,
            RetirementStage::WitnessLinked,
            RetirementStage::LeaseRemoved,
            RetirementStage::JournalRemoved,
            RetirementStage::DirectoryRemoved,
        ] {
            let data = tempfile::tempdir().unwrap();
            let work = known_task(data.path());
            let id = work.id();
            let revision = work.pin("unix-retirement").unwrap();
            work.finish().unwrap();
            drop(work);
            release(data.path(), &id, revision, "unix-retirement").unwrap();
            let journal = journal_root(data.path(), false).unwrap().join(&id);
            assert!(recover_one_at(data.path(), &journal, &mut |stage, _, _| {
                if stage == boundary {
                    Err(io::Error::other("Injected retirement interruption"))
                } else {
                    Ok(())
                }
            })
            .is_err());
            assert_eq!(recover(data.path(), 32).unwrap().removed, 1, "{boundary:?}");
            assert_no_reservation(data.path());
        }
    }
    #[test]
    fn interrupted_final_wal_quarantine_remains_a_conservative_unknown_reservation() {
        let data = tempfile::tempdir().unwrap();
        let work = known_task(data.path());
        let id = work.id();
        let revision = work.pin("unix-final-wal").unwrap();
        work.finish().unwrap();
        drop(work);
        release(data.path(), &id, revision, "unix-final-wal").unwrap();
        let root = journal_root(data.path(), false).unwrap();
        assert!(
            recover_one_at(data.path(), &root.join(&id), &mut |stage, _, _| {
                if stage == RetirementStage::DirectoryRemoved {
                    Err(io::Error::other("Before final WAL cleanup"))
                } else {
                    Ok(())
                }
            })
            .is_err()
        );
        let witness = witness_path(&root, &id);
        let identity = identity_path(&witness, Kind::File).unwrap();
        let directory = Directory::open(&root).unwrap();
        let mut captured = None;
        assert!(remove_at(
            &directory,
            witness.file_name().unwrap(),
            &[identity],
            &Kind::File,
            &mut |stage, _, path| {
                if stage == RemovalStage::Isolated {
                    captured = Some(path.to_owned());
                    Err(io::Error::other("After final WAL capture"))
                } else {
                    Ok(())
                }
            }
        )
        .is_err());
        let captured = captured.unwrap();
        assert!(captured.exists());
        let names = registry::candidates(&root).unwrap();
        assert_eq!(names.len(), 1);
        assert_eq!(
            registry::reserved(&root, &names, &profile(data.path()).unwrap()).unwrap(),
            registry::default_budget()
        );
        let report = recover(data.path(), 32).unwrap();
        assert_eq!(report.removed, 0);
        assert_eq!(report.failed, 1);
        assert!(
            captured.exists(),
            "Unknown quarantine must never be adopted by name"
        );
    }
}
