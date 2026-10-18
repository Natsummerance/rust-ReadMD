//! Retain directory identities and reuse handles, not mutable path checks.
//! Unix keeps the original per-lookup checks until native equivalents are tested.
use super::*;

pub(super) struct Tree {
    root: PathBuf,
    #[cfg(windows)]
    canonical: Option<PathBuf>,
    #[cfg(windows)]
    _ancestors: Vec<fs::File>,
    #[cfg(windows)]
    directories: BTreeMap<String, fs::File>,
}

impl Tree {
    pub(super) fn new(root: &Path) -> ApiResult<Self> {
        #[cfg(windows)]
        {
            let root = std::path::absolute(root).map_err(|_| error())?;
            let mut ancestors = Vec::new();
            let mut parents = root.parent().ok_or_else(error)?.ancestors().collect::<Vec<_>>();
            parents.reverse();
            for parent in parents {
                if !parent.as_os_str().is_empty() {
                    ancestors.push(pin(parent).map_err(native_error)?);
                }
            }
            let mut tree = Self { root, canonical: None, _ancestors: ancestors, directories: BTreeMap::new() };
            tree.root(false)?;
            Ok(tree)
        }
        #[cfg(not(windows))]
        Ok(Self { root: root.to_path_buf() })
    }

    #[cfg(windows)]
    fn root(&mut self, create: bool) -> ApiResult<bool> {
        if self.canonical.is_some() { return Ok(true); }
        let file = match pin(&self.root) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if !create { return Ok(false); }
                fs::create_dir(&self.root).map_err(|_| error())?;
                pin(&self.root).map_err(native_error)?
            }
            Err(e) => return Err(native_error(e)),
        };
        self.canonical = Some(self.root.canonicalize().map_err(|_| error())?);
        self.directories.insert(String::new(), file);
        Ok(true)
    }

    pub(super) fn file(&mut self, name: &str, create: bool) -> ApiResult<Option<PathBuf>> {
        #[cfg(windows)]
        {
            if !crate::capability_archive::safe_name(name) { return Err(error()); }
            if !self.root(create)? { return Ok(None); }
            let parts = name.split('/').collect::<Vec<_>>();
            let mut relative = String::new();
            for part in &parts[..parts.len() - 1] {
                if !relative.is_empty() { relative.push('/'); }
                relative.push_str(part);
                if let Some(file) = self.directories.get(&relative) {
                    directory(file).map_err(native_error)?;
                    continue;
                }
                let path = self.root.join(&relative);
                let file = match pin(&path) {
                    Ok(file) => file,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        if !create { return Ok(None); }
                        fs::create_dir(&path).map_err(|_| error())?;
                        pin(&path).map_err(native_error)?
                    }
                    Err(e) => return Err(native_error(e)),
                };
                if !path.canonicalize().map_err(|_| error())?
                    .starts_with(self.canonical.as_ref().ok_or_else(error)?) {
                    return Err(error());
                }
                self.directories.insert(relative.clone(), file);
            }
            directory(self.directories.get("").ok_or_else(error)?).map_err(native_error)?;
            // Write sharing is needed for child renames. In-place reparse
            // changes remain possible, so recheck the final parent on EVERY
            // lookup. Never treat a retained handle as immutable path trust.
            let mut path = self.root.clone();
            for part in &parts[..parts.len() - 1] { path.push(part); }
            if !path.canonicalize().map_err(|_| error())?
                .starts_with(self.canonical.as_ref().ok_or_else(error)?) {
                return Err(error());
            }
            path.push(parts.last().ok_or_else(error)?);
            Ok(Some(path))
        }
        #[cfg(not(windows))]
        transaction::file_at(&self.root, name, create)
    }
}

#[cfg(windows)]
fn native_error(e: std::io::Error) -> ApiError {
    if e.raw_os_error() == Some(32) { ApiError::new(409, "capability_install_busy") } else { error() }
}

#[cfg(windows)]
fn pin(path: &Path) -> std::io::Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT};
    // An attributes-only handle does NOT enforce share protection against a
    // rename. Actual read access plus denied delete is required here. Denying
    // write also blocks legitimate child moves, so mutable attributes are
    // checked afresh and every final parent is resolved before use.
    let file = fs::OpenOptions::new().read(true)
        .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;
    directory(&file)?;
    Ok(file)
}

#[cfg(windows)]
fn directory(file: &fs::File) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::{Foundation::HANDLE, Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT}};
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info)
        .map_err(|e| std::io::Error::from_raw_os_error(e.code().0 & 0xffff))?; }
    if info.dwFileAttributes & (FILE_ATTRIBUTE_DIRECTORY.0 | FILE_ATTRIBUTE_REPARSE_POINT.0)
        != FILE_ATTRIBUTE_DIRECTORY.0 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid capability directory"));
    }
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn cached_directories_and_ancestors_cannot_be_renamed_until_guard_drops() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("owned");
        fs::create_dir_all(root.join("nested/deep")).unwrap();
        let mut tree = Tree::new(&root).unwrap();
        tree.file("nested/deep/one", false).unwrap().unwrap();
        tree.file("nested/deep/two", false).unwrap().unwrap();
        assert_eq!(tree.directories.len(), 3);
        let one = root.join("nested/deep/one");
        fs::write(&one, b"owned").unwrap();
        // Protection must allow the very child rename used by uninstall.
        super::super::transaction::move_checked(&one, &root.join("nested/deep/two")).unwrap();
        for path in [&root, &root.join("nested"), &root.join("nested/deep")] {
            assert!(fs::rename(path, path.with_extension("replaced")).is_err(), "Pinned path was renamed: {path:?}");
        }
        assert!(fs::rename(temp.path(), temp.path().with_extension("replaced")).is_err());
        drop(tree);
        fs::rename(root.join("nested/deep"), root.join("nested/replaced")).unwrap();
    }

    #[test]
    fn cached_lookup_creates_only_requested_parents_and_rejects_redirection() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("owned");
        let outside = temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let mut tree = Tree::new(&root).unwrap();
        assert!(tree.file("missing/file", false).unwrap().is_none());
        let file = tree.file("nested/deep/file", true).unwrap().unwrap();
        fs::write(file, b"owned").unwrap();
        assert!(tree.file("../outside/file", true).is_err());
        // Even before a redirected child enters the cache, it is rejected.
        use std::os::windows::process::CommandExt;
        let output = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(root.join("redirected")).arg(&outside)
            .creation_flags(0x08000000).output().unwrap();
        assert!(output.status.success(), "Native junction fixture failed");
        assert!(tree.file("redirected/file", true).is_err());
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
        drop(tree);
        fs::remove_dir(root.join("redirected")).unwrap(); // link only
    }

    #[test]
    fn in_place_reparse_change_is_not_trusted_by_cached_directory_handle() {
        use std::os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle};
        use windows::Win32::{Foundation::HANDLE, System::IO::DeviceIoControl};
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("owned");
        let outside = temp.path().join("outside");
        fs::create_dir(&root).unwrap();fs::create_dir(&outside).unwrap();
        fs::write(outside.join("file"), b"preserve external bytes").unwrap();
        let mut tree = Tree::new(&root).unwrap();
        tree.file("cached/file", true).unwrap().unwrap();
        let link = root.join("cached");
        let target = outside.canonicalize().unwrap();
        let canonical = target.as_os_str().encode_wide().collect::<Vec<_>>();
        assert!(canonical.starts_with(&[92,92,63,92]));
        let mut substitute = vec![92u16,63,63,92];substitute.extend_from_slice(&canonical[4..]);
        let bytes = substitute.len() * 2;
        let mut data = Vec::new();
        data.extend_from_slice(&0xa0000003u32.to_le_bytes());
        data.extend_from_slice(&((8 + bytes + 4) as u16).to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&(bytes as u16).to_le_bytes());
        data.extend_from_slice(&((bytes + 2) as u16).to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        for word in substitute { data.extend_from_slice(&word.to_le_bytes()); }
        data.extend_from_slice(&[0,0,0,0]);
        let write = fs::OpenOptions::new().write(true).share_mode(7).custom_flags(0x02200000).open(&link).unwrap();
        let mut returned = 0;
        unsafe { DeviceIoControl(HANDLE(write.as_raw_handle()), 0x000900a4,
            Some(data.as_ptr().cast()), data.len() as u32, None, 0, Some(&mut returned), None).unwrap(); }
        drop(write);
        assert!(tree.file("cached/file", false).is_err());
        assert_eq!(fs::read(outside.join("file")).unwrap(), b"preserve external bytes");
        drop(tree);
        fs::remove_dir(link).unwrap(); // remove the fixture reparse only
    }
}
