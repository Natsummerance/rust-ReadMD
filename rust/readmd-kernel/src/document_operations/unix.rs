use super::*;
use std::ffi::CString;
use std::os::{
    fd::{AsRawFd, FromRawFd},
    unix::{ffi::OsStrExt, fs::MetadataExt},
};
const DIRECTORY: i32 = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
pub(super) fn valid_leaf(name: &OsStr) -> bool {
    !name.as_bytes().contains(&0) && !name.as_bytes().contains(&b'/')
}
fn leaf_cstring(value: &OsStr) -> io::Result<CString> {
    CString::new(value.as_bytes()).map_err(|_| unsafe_path())
}
fn opened(value: i32) -> io::Result<fs::File> {
    if value >= 0 {
        return Ok(unsafe { fs::File::from_raw_fd(value) });
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ELOOP) {
        Err(unsafe_path())
    } else {
        Err(error)
    }
}
fn at(parent: &fs::File, leaf: &OsStr, flags: i32) -> io::Result<fs::File> {
    let leaf = leaf_cstring(leaf)?;
    opened(unsafe {
        libc::openat(
            parent.as_raw_fd(),
            leaf.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
            0o600,
        )
    })
}
pub(super) fn info(file: &fs::File) -> io::Result<Info> {
    let metadata = file.metadata()?;
    let birth = metadata
        .created()
        .map_err(|_| unsupported())?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| unsupported())?
        .as_nanos();
    if metadata.ino() == 0 {
        return Err(unsupported());
    }
    Ok(Info {
        identity: FileIdentity::native("unix", metadata.dev(), metadata.ino() as u128, birth),
        length: metadata.len(),
        modified: ((metadata.mtime() as i128) * 1_000_000_000 + metadata.mtime_nsec() as i128)
            as u128,
        changed: ((metadata.ctime() as i128) * 1_000_000_000 + metadata.ctime_nsec() as i128)
            as u128,
        links: metadata.nlink(),
        regular: metadata.is_file(),
        redirected: metadata.file_type().is_symlink(),
    })
}
pub(super) struct Directory {
    path: PathBuf,
    anchors: Vec<fs::File>,
    identities: Vec<FileIdentity>,
}
impl Directory {
    pub(super) fn open(path: &Path) -> io::Result<Self> {
        if !path.is_absolute() {
            return Err(unsafe_path());
        }
        let root = opened(unsafe { libc::open(c"/".as_ptr(), DIRECTORY) })?;
        let mut result = Self {
            path: PathBuf::from("/"),
            identities: vec![info(&root)?.identity],
            anchors: vec![root],
        };
        for component in path.components() {
            match component {
                Component::RootDir => (),
                Component::Normal(leaf) if valid_leaf(leaf) => {
                    let file = at(result.anchors.last().unwrap(), leaf, DIRECTORY)?;
                    if !file.metadata()?.is_dir() {
                        return Err(unsafe_path());
                    }
                    result.identities.push(info(&file)?.identity);
                    result.anchors.push(file);
                    result.path.push(leaf);
                }
                _ => return Err(unsafe_path()),
            }
        }
        Ok(result)
    }
    fn file(&self) -> &fs::File {
        self.anchors.last().unwrap()
    }
    pub(super) fn path(&self) -> &Path {
        &self.path
    }
    pub(super) fn identity(&self) -> &FileIdentity {
        self.identities.last().unwrap()
    }
    pub(super) fn revalidate(&self) -> io::Result<()> {
        if Self::open(&self.path)?.identities != self.identities {
            return Err(unsafe_path());
        }
        Ok(())
    }
    pub(super) fn open_file(&self, name: &OsStr) -> io::Result<fs::File> {
        at(self.file(), name, libc::O_RDONLY)
    }
    pub(super) fn exists(&self, name: &OsStr) -> io::Result<bool> {
        let name = leaf_cstring(name)?;
        let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
        if unsafe {
            libc::fstatat(
                self.file().as_raw_fd(),
                name.as_ptr(),
                metadata.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } == 0
        {
            return Ok(true);
        }
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::NotFound {
            Ok(false)
        } else {
            Err(error)
        }
    }
    pub(super) fn try_lock(&self, name: &OsStr) -> io::Result<Option<fs::File>> {
        let file = at(self.file(), name, libc::O_RDWR | libc::O_CREAT).map_err(|error| {
            // Unix rejects a directory before a file handle is returned. It is
            // an invalid existing lock object, not a storage failure to repair.
            if error.raw_os_error() == Some(libc::EISDIR) { unsafe_path() } else { error }
        })?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() != 0 || metadata.nlink() != 1 {
            return Err(unsafe_path());
        }
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(Some(file));
        }
        let error = io::Error::last_os_error();
        if matches!(
            error.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
        ) {
            Ok(None)
        } else {
            Err(error)
        }
    }
    pub(super) fn lock_still_named(
        &self,
        name: &OsStr,
        expected: &FileIdentity,
    ) -> io::Result<bool> {
        Ok(info(&self.open_file(name)?)?.identity == *expected)
    }
    pub(super) fn rename_noreplace(&self, source: &OsStr, target: &OsStr) -> io::Result<()> {
        let source = leaf_cstring(source)?;
        let target = leaf_cstring(target)?;
        #[cfg(target_os = "linux")]
        let result = unsafe {
            libc::renameat2(
                self.file().as_raw_fd(),
                source.as_ptr(),
                self.file().as_raw_fd(),
                target.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        #[cfg(target_os = "macos")]
        let result = unsafe {
            libc::renameatx_np(
                self.file().as_raw_fd(),
                source.as_ptr(),
                self.file().as_raw_fd(),
                target.as_ptr(),
                libc::RENAME_EXCL,
            )
        };
        if result == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if matches!(
            error.raw_os_error(),
            Some(libc::ENOSYS | libc::EINVAL | libc::EOPNOTSUPP)
        ) {
            Err(unsupported())
        } else {
            Err(error)
        }
    }
    pub(super) fn sync(&self) -> io::Result<()> {
        self.file().sync_all()
    }
}
pub(super) fn unlock(file: &fs::File) {
    loop {
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) } == 0 {
            break;
        }
        if io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
            break;
        }
    }
}
