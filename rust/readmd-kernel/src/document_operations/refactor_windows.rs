//! Strict Windows leases for a journaled, multi-file document refactor.
//!
//! The caller must hold SAVE_LOCK -> the profile gate and persist all original,
//! stage and backup names/identities before the first move. This module grants
//! no path authorization. Only create_new stages are written; existing files
//! are never written or truncated.
//! Every move is by the leased object handle, with replacement disabled.
use super::*;
use std::{
    io::{Seek, SeekFrom, Write},
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
};
use windows::Win32::{
    Foundation::{FILETIME, GENERIC_READ, GENERIC_WRITE, HANDLE},
    Storage::FileSystem::{
        FileDispositionInfo, FileRenameInfo, SetFileInformationByHandle, SetFileTime, DELETE,
        FILE_DISPOSITION_INFO, FILE_RENAME_INFO, FILE_RENAME_INFO_0, FILE_SHARE_READ,
    },
};

#[path = "refactor_metadata.rs"]
mod metadata;

const OPEN_REPARSE: u32 = 0x00200000;
const BACKUP: u32 = 0x02000000;
pub(crate) const MAX_LEASE_BYTES: u64 = 4 * 1024 * 1024;

#[cfg(test)]
thread_local! {
    static BEFORE_MOVE: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}

fn lease_error(error: io::Error) -> Error {
    let mut result = Error::io(error);
    if let Some(cause) = &result.cause {
        result.kind = match cause.raw_os_error() {
            Some(32 | 33) => ErrorKind::Busy,
            Some(80 | 183) => ErrorKind::TargetExists,
            Some(1 | 50) => ErrorKind::Unsupported,
            _ if cause.kind() == io::ErrorKind::NotFound => ErrorKind::Missing,
            _ if cause.kind() == io::ErrorKind::InvalidData => ErrorKind::InvalidPath,
            _ => result.kind,
        };
    }
    result
}

fn safe_info(file: &fs::File) -> Result<Info> {
    let info = native::info(file).map_err(lease_error)?;
    if info.redirected || !info.regular || info.links != 1 || info.length > MAX_LEASE_BYTES {
        return Err(Error::new(ErrorKind::Unsupported));
    }
    Ok(info)
}

fn read_bytes(file: &fs::File) -> Result<Vec<u8>> {
    let mut input = file;
    input.seek(SeekFrom::Start(0)).map_err(lease_error)?;
    let mut bytes = Vec::new();
    input
        .take(MAX_LEASE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(lease_error)?;
    if bytes.len() as u64 > MAX_LEASE_BYTES {
        return Err(Error::new(ErrorKind::Unsupported));
    }
    Ok(bytes)
}

fn delete_handle(file: &fs::File) -> Result<()> {
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfo,
            (&disposition as *const FILE_DISPOSITION_INFO).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    }
    .map_err(|error| lease_error(io::Error::from_raw_os_error(error.code().0 & 0xffff)))
}

fn windows_object(identity: &FileIdentity) -> Result<(&str, u64)> {
    let (object, birth) = identity
        .as_str()
        .rsplit_once(':')
        .filter(|(object, _)| object.starts_with("readmd-file-v1:windows:"))
        .ok_or_else(|| Error::new(ErrorKind::Unsupported))?;
    let birth = u128::from_str_radix(birth, 16)
        .ok()
        .and_then(|birth| u64::try_from(birth).ok())
        .filter(|birth| *birth != 0)
        .ok_or_else(|| Error::new(ErrorKind::Unsupported))?;
    Ok((object, birth))
}

/// Holds READ|WRITE|DELETE access while sharing only READ. Existing writable or
/// non-delete-sharing opens cause admission to fail before any mutation; new
/// ordinary writers and deleters cannot change this object while leased.
/// For existing files WRITE access is used only for FlushFileBuffers. The only
/// content write is create_stage's create_new file before it is published.
/// Read-only files are deliberately refused rather than weakening the lease.
pub(crate) struct WindowsLease {
    parent: native::Directory,
    name: OsString,
    path: PathBuf,
    file: fs::File,
    fingerprint: FileFingerprint,
    bytes: Vec<u8>,
    metadata: metadata::Metadata,
    // verify_current seeks the single underlying file cursor.
    _not_sync: PhantomData<Cell<()>>,
}

impl fmt::Debug for WindowsLease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WindowsDocumentLease")
            .field("fingerprint", &self.fingerprint)
            .field("bytes", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

impl WindowsLease {
    /// Open an existing, explicitly authorized pathname. Recovery uses this
    /// same function, then compares BOTH fingerprint fields with its journal;
    /// merely finding equal bytes or a journaled path is never ownership proof.
    pub(crate) fn open(path: &Path) -> Result<Self> {
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
        .map_err(lease_error)?;
        parent.verify_case_mode().map_err(lease_error)?;
        let path = parent.path().join(name);
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .access_mode(GENERIC_READ.0 | GENERIC_WRITE.0 | DELETE.0)
            .share_mode(FILE_SHARE_READ.0)
            .custom_flags(OPEN_REPARSE | BACKUP)
            .open(&path)
            .map_err(lease_error)?;
        let before = safe_info(&file)?;
        let bytes = read_bytes(&file)?;
        let metadata = metadata::capture(&file, &path)?;
        let after = safe_info(&file)?;
        if before != after || after.length != bytes.len() as u64 {
            return Err(Error::new(ErrorKind::RevisionChanged));
        }
        parent.revalidate().map_err(lease_error)?;
        let named = parent.open_file(name).map_err(lease_error)?;
        if safe_info(&named)? != after {
            return Err(Error::new(ErrorKind::IdentityChanged));
        }
        let fingerprint = FileFingerprint {
            identity: after.identity,
            revision: format!("{:x}", Sha256::digest(&bytes)),
        };
        Ok(Self {
            parent,
            name: name.to_owned(),
            path,
            file,
            fingerprint,
            bytes,
            metadata,
            _not_sync: PhantomData,
        })
    }

    /// Create a new, synchronized stage and retain its original creating
    /// handle, so no close/reopen gap can accidentally adopt a foreign file.
    /// A collision never opens or removes the existing name. If writing or
    /// verification fails, attempt deletion ONLY through this creating handle;
    /// a failed cleanup leaves the owned stage for journal/manual recovery.
    pub(crate) fn create_stage(path: &Path, bytes: &[u8]) -> Result<Self> {
        if !path.is_absolute() {
            return Err(Error::new(ErrorKind::InvalidPath));
        }
        if bytes.len() as u64 > MAX_LEASE_BYTES {
            return Err(Error::new(ErrorKind::Unsupported));
        }
        let name = path
            .file_name()
            .ok_or_else(|| Error::new(ErrorKind::InvalidPath))?;
        leaf(name)?;
        let parent = native::Directory::open(
            path.parent()
                .ok_or_else(|| Error::new(ErrorKind::InvalidPath))?,
        )
        .map_err(lease_error)?;
        parent.verify_case_mode().map_err(lease_error)?;
        let path = parent.path().join(name);
        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .access_mode(GENERIC_READ.0 | GENERIC_WRITE.0 | DELETE.0)
            .share_mode(FILE_SHARE_READ.0)
            .custom_flags(OPEN_REPARSE | BACKUP)
            .open(&path)
            .map_err(lease_error)?;
        let prepared = (|| {
            let created = safe_info(&file)?;
            if created.length != 0 {
                return Err(Error::new(ErrorKind::RevisionChanged));
            }
            file.write_all(bytes).map_err(lease_error)?;
            file.sync_all().map_err(lease_error)?;
            let before = safe_info(&file)?;
            let written = read_bytes(&file)?;
            let after = safe_info(&file)?;
            if before != after
                || after.identity != created.identity
                || after.length != bytes.len() as u64
                || written != bytes
            {
                return Err(Error::new(ErrorKind::RevisionChanged));
            }
            parent.revalidate().map_err(lease_error)?;
            let named = parent.open_file(name).map_err(lease_error)?;
            if safe_info(&named)? != after {
                return Err(Error::new(ErrorKind::IdentityChanged));
            }
            let metadata = metadata::capture(&file, &path)?;
            Ok((
                FileFingerprint {
                    identity: after.identity,
                    revision: format!("{:x}", Sha256::digest(bytes)),
                },
                metadata,
            ))
        })();
        let (fingerprint, metadata) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                let _ = delete_handle(&file);
                return Err(error);
            }
        };
        Ok(Self {
            parent,
            name: name.to_owned(),
            path,
            file,
            fingerprint,
            bytes: bytes.to_vec(),
            metadata,
            _not_sync: PhantomData,
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) fn fingerprint(&self) -> &FileFingerprint {
        &self.fingerprint
    }

    /// Persist this opaque digest alongside the original strong fingerprint.
    /// A recovery-opened backup has a NEW baseline; cleanup must compare this
    /// admission digest before deleting that object.
    pub(crate) fn metadata_revision(&self) -> String {
        self.metadata.revision()
    }

    pub(crate) fn verify_metadata_revision(&self, expected: &str) -> Result<()> {
        self.verify_current()?;
        if self.metadata.revision() != expected {
            return Err(Error::new(ErrorKind::Unsupported));
        }
        Ok(())
    }

    /// Fresh stages may inherit permissions different from an explicit source
    /// DACL/owner/group/label. Refuse before ANY original is moved; no security
    /// descriptor or source metadata is silently broadened or copied.
    pub(crate) fn verify_replacement(&self, stage: &Self) -> Result<()> {
        self.verify_current()?;
        stage.verify_current()?;
        if self.metadata != stage.metadata {
            return Err(Error::new(ErrorKind::Unsupported));
        }
        Ok(())
    }

    /// NTFS name tunneling may replace CreationTime when a file is renamed to
    /// a recently vacated name. Restore OUR live lease's birth only after the
    /// held handle still proves the same volume/file ID and exact raw bytes.
    /// This is never used to adopt a restarted file with a mismatched birth.
    fn preserve_birth_after_move(&self) -> Result<()> {
        let before = safe_info(&self.file)?;
        let (expected_object, birth) = windows_object(&self.fingerprint.identity)?;
        let (actual_object, _) = windows_object(&before.identity)?;
        if actual_object != expected_object {
            return Err(Error::new(ErrorKind::IdentityChanged));
        }
        if before.identity == self.fingerprint.identity {
            return Ok(());
        }
        let bytes = read_bytes(&self.file)?;
        let after = safe_info(&self.file)?;
        if before != after
            || after.length != bytes.len() as u64
            || format!("{:x}", Sha256::digest(&bytes)) != self.fingerprint.revision
        {
            return Err(Error::new(ErrorKind::RevisionChanged));
        }
        let creation = FILETIME {
            dwLowDateTime: birth as u32,
            dwHighDateTime: (birth >> 32) as u32,
        };
        // NULL access/write pointers leave both existing timestamps unchanged.
        // GENERIC_WRITE includes the required FILE_WRITE_ATTRIBUTES right.
        unsafe {
            SetFileTime(
                HANDLE(self.file.as_raw_handle()),
                Some(&creation as *const FILETIME),
                None,
                None,
            )
        }
        .map_err(|error| lease_error(io::Error::from_raw_os_error(error.code().0 & 0xffff)))?;
        if safe_info(&self.file)?.identity != self.fingerprint.identity {
            return Err(Error::new(ErrorKind::IdentityChanged));
        }
        Ok(())
    }

    /// Check the held object, raw bytes and its current name without reopening
    /// a writable handle. Ancestors remain pinned by native::Directory.
    pub(crate) fn verify_current(&self) -> Result<()> {
        self.parent.verify_case_mode().map_err(lease_error)?;
        self.parent.revalidate().map_err(lease_error)?;
        let before = safe_info(&self.file)?;
        if before.identity != self.fingerprint.identity {
            return Err(Error::new(ErrorKind::IdentityChanged));
        }
        let bytes = read_bytes(&self.file)?;
        let after = safe_info(&self.file)?;
        if before != after
            || after.length != bytes.len() as u64
            || format!("{:x}", Sha256::digest(&bytes)) != self.fingerprint.revision
        {
            return Err(Error::new(ErrorKind::RevisionChanged));
        }
        let named = self.parent.open_file(&self.name).map_err(lease_error)?;
        if safe_info(&named)? != after {
            return Err(Error::new(ErrorKind::IdentityChanged));
        }
        if metadata::capture(&self.file, &self.path)? != self.metadata {
            return Err(Error::new(ErrorKind::Unsupported));
        }
        self.parent.revalidate().map_err(lease_error)
    }

    /// Move this object to another leaf in the same pinned directory. This is
    /// also the rollback primitive: an externally occupied name is preserved.
    /// On native-call error, MoveState::Unknown requires journal observation;
    /// path() then remains the last confirmed name. On post-move error, path()
    /// already holds the target and the error has MoveState::Moved.
    pub(crate) fn move_to(&mut self, target_name: &OsStr) -> Result<MoveReceipt> {
        self.verify_current()?;
        leaf(target_name)?;
        if target_name == self.name {
            return Err(Error::new(ErrorKind::InvalidPath));
        }
        if self.parent.exists(target_name).map_err(lease_error)? {
            return Err(Error::new(ErrorKind::TargetExists));
        }
        let target = self.parent.path().join(target_name);
        let wide = target
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let name_bytes = (wide.len() - 1)
            .checked_mul(std::mem::size_of::<u16>())
            .and_then(|length| u32::try_from(length).ok())
            .ok_or_else(|| Error::new(ErrorKind::InvalidPath))?;
        let offset = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
        let size = offset
            .checked_add(wide.len() * std::mem::size_of::<u16>())
            .ok_or_else(|| Error::new(ErrorKind::InvalidPath))?
            .max(std::mem::size_of::<FILE_RENAME_INFO>());
        let buffer_size = u32::try_from(size).map_err(|_| Error::new(ErrorKind::InvalidPath))?;
        // usize storage provides the alignment FILE_RENAME_INFO requires, and
        // the entire flexible-array buffer is initialized before the syscall.
        let mut buffer = vec![0usize; size.div_ceil(std::mem::size_of::<usize>())];
        let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
        unsafe {
            (*info).Anonymous = FILE_RENAME_INFO_0 {
                ReplaceIfExists: false,
            };
            (*info).RootDirectory = HANDLE::default();
            (*info).FileNameLength = name_bytes;
            std::ptr::copy_nonoverlapping(
                wide.as_ptr(),
                buffer.as_mut_ptr().cast::<u8>().add(offset).cast::<u16>(),
                wide.len(),
            );
        }
        #[cfg(test)]
        BEFORE_MOVE.with(|hook| {
            if let Some(callback) = hook.borrow_mut().take() {
                callback();
            }
        });
        let moved = unsafe {
            SetFileInformationByHandle(
                HANDLE(self.file.as_raw_handle()),
                FileRenameInfo,
                info.cast(),
                buffer_size,
            )
        };
        if let Err(error) = moved {
            let mut error = lease_error(io::Error::from_raw_os_error(error.code().0 & 0xffff));
            error.move_state = MoveState::Unknown;
            return Err(error);
        }
        self.name = target_name.to_owned();
        self.path = target;
        self.preserve_birth_after_move()
            .map_err(Error::after_move)?;
        // Unlike pathname MoveFileExW, FileRenameInfo has no WRITE_THROUGH
        // flag. Settle the leased file's buffers and report post-move failure;
        // do not claim that a directory handle was durably synchronized.
        self.file
            .sync_all()
            .map_err(lease_error)
            .map_err(Error::after_move)?;
        self.verify_current().map_err(Error::after_move)?;
        Ok(MoveReceipt {
            fingerprint: self.fingerprint.clone(),
        })
    }

    /// Delete only this verified leased object, normally an owned stage/backup
    /// after a terminal journal commit. The journal must retain its exact
    /// path+fingerprint until this succeeds. A syscall failure leaves the file
    /// for recovery to reopen and compare; it never falls back to pathname
    /// deletion. Success marks the held object delete-pending and closes it.
    pub(crate) fn remove(self) -> Result<()> {
        self.verify_current()?;
        delete_handle(&self.file)?;
        drop(self);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use windows::Win32::{
        Foundation::{LocalFree, HLOCAL},
        Security::{
            Authorization::{GetSecurityInfo, SetSecurityInfo, SE_FILE_OBJECT},
            ACL, DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
            PSECURITY_DESCRIPTOR,
        },
        Storage::FileSystem::{READ_CONTROL, WRITE_DAC},
    };

    fn write_new(path: &Path, bytes: &[u8]) {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }

    fn protect_existing_dacl(path: &Path) {
        // Synthetic files only. Keep all existing grants exactly as returned;
        // setting the protected bit produces a distinct explicit descriptor
        // without granting an additional principal any access.
        let file = fs::OpenOptions::new()
            .access_mode(READ_CONTROL.0 | WRITE_DAC.0)
            .share_mode(7)
            .custom_flags(OPEN_REPARSE)
            .open(path)
            .unwrap();
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        let mut dacl: *mut ACL = std::ptr::null_mut();
        let result = unsafe {
            GetSecurityInfo(
                HANDLE(file.as_raw_handle()),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                None,
                None,
                Some(&mut dacl),
                None,
                Some(&mut descriptor),
            )
        };
        assert_eq!(result.0, 0);
        assert!(!dacl.is_null());
        let result = unsafe {
            SetSecurityInfo(
                HANDLE(file.as_raw_handle()),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                None,
                None,
                Some(dacl),
                None,
            )
        };
        unsafe {
            let _ = LocalFree(Some(HLOCAL(descriptor.0)));
        }
        assert_eq!(result.0, 0);
    }

    #[test]
    fn ordinary_inherited_metadata_is_compatible_with_a_fresh_stage() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("original.md");
        write_new(&path, b"original");
        let original = WindowsLease::open(&path).unwrap();
        let stage =
            WindowsLease::create_stage(&directory.path().join("stage.md"), b"updated").unwrap();
        original.verify_replacement(&stage).unwrap();
        assert_eq!(original.metadata_revision(), stage.metadata_revision());
        original
            .verify_metadata_revision(&original.metadata_revision())
            .unwrap();
        stage.remove().unwrap();
    }

    #[test]
    fn explicit_dacl_is_not_replaced_by_a_different_inherited_stage_acl() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("original.md");
        write_new(&path, b"original");
        protect_existing_dacl(&path);
        let original = WindowsLease::open(&path).unwrap();
        let descriptor = original.metadata_revision();
        let stage =
            WindowsLease::create_stage(&directory.path().join("stage.md"), b"updated").unwrap();
        assert_eq!(
            original.verify_replacement(&stage).unwrap_err().kind,
            ErrorKind::Unsupported
        );
        original.verify_metadata_revision(&descriptor).unwrap();
        stage.remove().unwrap();
        drop(original);
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(
            WindowsLease::open(&path).unwrap().metadata_revision(),
            descriptor
        );
    }

    #[test]
    fn alternate_data_stream_sentinel_is_refused_and_never_deleted() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("original.md");
        let stream = PathBuf::from(format!("{}:readmd-sentinel", path.to_string_lossy()));
        write_new(&path, b"original");
        write_new(&stream, b"KEEP-ALTERNATE-DATA");
        assert_eq!(
            WindowsLease::open(&path).unwrap_err().kind,
            ErrorKind::Unsupported
        );
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(fs::read(&stream).unwrap(), b"KEEP-ALTERNATE-DATA");
    }

    #[test]
    fn metadata_changed_after_admission_prevents_backup_cleanup() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("backup.md");
        write_new(&path, b"original");
        let original = WindowsLease::open(&path).unwrap();
        let admission = original.metadata_revision();
        protect_existing_dacl(&path);
        assert_eq!(
            original.verify_current().unwrap_err().kind,
            ErrorKind::Unsupported
        );
        assert_eq!(original.remove().unwrap_err().kind, ErrorKind::Unsupported);
        let reopened = WindowsLease::open(&path).unwrap();
        assert_eq!(
            reopened
                .verify_metadata_revision(&admission)
                .unwrap_err()
                .kind,
            ErrorKind::Unsupported
        );
        drop(reopened);
        assert_eq!(fs::read(path).unwrap(), b"original");
    }

    #[test]
    fn lease_blocks_external_writes_delete_and_pathname_moves() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("synthetic.md");
        write_new(&path, b"original\r\n");
        let lease = WindowsLease::open(&path).unwrap();
        assert!(fs::OpenOptions::new().write(true).open(&path).is_err());
        assert!(fs::remove_file(&path).is_err());
        assert!(fs::rename(&path, directory.path().join("foreign.md")).is_err());
        lease.verify_current().unwrap();
        assert_eq!(lease.bytes(), b"original\r\n");
        drop(lease);
        assert_eq!(fs::read(&path).unwrap(), b"original\r\n");
    }

    #[test]
    fn handle_move_and_reverse_move_preserve_identity_and_all_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("original.md");
        let bytes = b"\xef\xbb\xbf# Synthetic\r\n\x00\xff";
        write_new(&path, bytes);
        let mut lease = WindowsLease::open(&path).unwrap();
        let original = lease.fingerprint().clone();
        assert_eq!(
            lease.move_to(OsStr::new("backup.md")).unwrap().fingerprint,
            original
        );
        assert_eq!(lease.path(), directory.path().join("backup.md"));
        assert!(!path.exists());
        lease.verify_current().unwrap();
        assert_eq!(
            lease
                .move_to(OsStr::new("original.md"))
                .unwrap()
                .fingerprint,
            original
        );
        drop(lease);
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }

    #[test]
    fn no_replace_preserves_an_externally_occupied_destination() {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("original.md");
        let foreign = directory.path().join("foreign.md");
        write_new(&original, b"original");
        write_new(&foreign, b"foreign");
        let mut lease = WindowsLease::open(&original).unwrap();
        let error = lease.move_to(OsStr::new("foreign.md")).unwrap_err();
        assert_eq!(error.kind, ErrorKind::TargetExists);
        assert_eq!(error.move_state, MoveState::NotAttempted);
        lease.verify_current().unwrap();
        drop(lease);
        assert_eq!(fs::read(original).unwrap(), b"original");
        assert_eq!(fs::read(foreign).unwrap(), b"foreign");
    }

    #[test]
    fn native_move_never_replaces_a_destination_created_after_preflight() {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("original.md");
        let foreign = directory.path().join("foreign.md");
        write_new(&original, b"original");
        let mut lease = WindowsLease::open(&original).unwrap();
        let created = foreign.clone();
        BEFORE_MOVE.with(|hook| {
            hook.replace(Some(Box::new(move || write_new(&created, b"foreign"))));
        });
        let error = lease.move_to(OsStr::new("foreign.md")).unwrap_err();
        assert_eq!(error.move_state, MoveState::Unknown);
        lease.verify_current().unwrap();
        drop(lease);
        assert_eq!(fs::read(original).unwrap(), b"original");
        assert_eq!(fs::read(foreign).unwrap(), b"foreign");
    }

    #[test]
    fn staged_publish_and_safe_rollback_use_only_owned_objects() {
        let directory = tempfile::tempdir().unwrap();
        let original_path = directory.path().join("original.md");
        let stage_path = directory.path().join("stage.md");
        write_new(&original_path, b"original\r\n");
        let mut original = WindowsLease::open(&original_path).unwrap();
        let mut stage = WindowsLease::create_stage(&stage_path, b"updated\r\n").unwrap();
        let original_identity = original.fingerprint().clone();
        let stage_identity = stage.fingerprint().clone();
        original.move_to(OsStr::new("backup.md")).unwrap();
        stage.move_to(OsStr::new("original.md")).unwrap();
        assert_eq!(stage.fingerprint(), &stage_identity);
        stage.verify_current().unwrap();
        stage.move_to(OsStr::new("retired.md")).unwrap();
        original.move_to(OsStr::new("original.md")).unwrap();
        assert_eq!(original.fingerprint(), &original_identity);
        stage.remove().unwrap();
        original.verify_current().unwrap();
        drop(original);
        assert_eq!(fs::read(original_path).unwrap(), b"original\r\n");
        assert!(!directory.path().join("backup.md").exists());
        assert!(!directory.path().join("retired.md").exists());
    }

    #[test]
    fn vacated_name_tunneling_preserves_the_live_stage_birth_and_last_write_time() {
        let directory = tempfile::tempdir().unwrap();
        let original_path = directory.path().join("original.md");
        let stage_path = directory.path().join("stage.md");
        write_new(&original_path, b"original");
        let original_file = fs::OpenOptions::new()
            .write(true)
            .open(&original_path)
            .unwrap();
        // A distinct old creation time makes name tunneling deterministic even
        // on filesystems with coarser clock resolution or very fast machines.
        let creation = FILETIME {
            dwLowDateTime: 0x9ad00000,
            dwHighDateTime: 0x01d5c036,
        };
        unsafe {
            SetFileTime(
                HANDLE(original_file.as_raw_handle()),
                Some(&creation),
                None,
                None,
            )
        }
        .unwrap();
        original_file.sync_all().unwrap();
        drop(original_file);
        let mut original = WindowsLease::open(&original_path).unwrap();
        let mut stage = WindowsLease::create_stage(&stage_path, b"updated").unwrap();
        let stage_identity = stage.fingerprint().clone();
        let stage_info = safe_info(&stage.file).unwrap();
        assert_ne!(stage_info.identity, original.fingerprint.identity);
        assert_ne!(
            windows_object(&stage_info.identity).unwrap().1,
            windows_object(&original.fingerprint.identity).unwrap().1
        );
        original.move_to(OsStr::new("backup.md")).unwrap();
        stage.move_to(OsStr::new("original.md")).unwrap();
        assert_eq!(
            safe_info(&stage.file).unwrap().identity,
            stage_identity.identity
        );
        assert_eq!(
            safe_info(&stage.file).unwrap().modified,
            stage_info.modified
        );
        stage.verify_current().unwrap();
        drop(stage);
        assert_eq!(
            WindowsLease::open(&original_path).unwrap().fingerprint(),
            &stage_identity
        );
    }

    #[test]
    fn birth_correction_never_accepts_a_different_volume_or_file_id() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("stage.md");
        let mut stage = WindowsLease::create_stage(&path, b"stage").unwrap();
        let actual = stage.fingerprint.identity.clone();
        stage.fingerprint.identity = FileIdentity::native("windows", 0, 1, 2);
        assert_eq!(
            stage.preserve_birth_after_move().unwrap_err().kind,
            ErrorKind::IdentityChanged
        );
        assert_eq!(safe_info(&stage.file).unwrap().identity, actual);
        stage.fingerprint.identity = actual;
        stage.verify_current().unwrap();
    }

    #[test]
    fn stage_creation_retains_its_handle_and_never_adopts_an_existing_name() {
        let directory = tempfile::tempdir().unwrap();
        let stage_path = directory.path().join("stage.md");
        let stage = WindowsLease::create_stage(&stage_path, b"\xff\xfeA\0\r\0\n\0").unwrap();
        stage.verify_current().unwrap();
        assert!(fs::OpenOptions::new()
            .write(true)
            .open(&stage_path)
            .is_err());
        assert_eq!(
            WindowsLease::create_stage(&stage_path, b"foreign")
                .unwrap_err()
                .kind,
            ErrorKind::TargetExists
        );
        stage.remove().unwrap();
        assert!(!stage_path.exists());
        write_new(&stage_path, b"foreign");
        assert_eq!(
            WindowsLease::create_stage(&stage_path, b"updated")
                .unwrap_err()
                .kind,
            ErrorKind::TargetExists
        );
        assert_eq!(fs::read(stage_path).unwrap(), b"foreign");
    }

    #[test]
    fn unsafe_paths_hardlinks_and_oversize_are_refused_before_moving() {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("original.md");
        write_new(&original, b"original");
        assert_eq!(
            WindowsLease::open(Path::new("relative.md"))
                .unwrap_err()
                .kind,
            ErrorKind::InvalidPath
        );
        let linked = directory.path().join("linked.md");
        fs::hard_link(&original, &linked).unwrap();
        assert_eq!(
            WindowsLease::open(&original).unwrap_err().kind,
            ErrorKind::Unsupported
        );
        let oversized = directory.path().join("oversize.md");
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&oversized)
            .unwrap();
        file.set_len(MAX_LEASE_BYTES + 1).unwrap();
        drop(file);
        assert_eq!(
            WindowsLease::open(&oversized).unwrap_err().kind,
            ErrorKind::Unsupported
        );
        assert_eq!(fs::read(original).unwrap(), b"original");
    }
}
