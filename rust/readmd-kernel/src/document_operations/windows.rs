use super::*;
use std::os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle};
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{
            FileBasicInfo, FileIdInfo, GetFileInformationByHandle, GetFileInformationByHandleEx,
            MoveFileExW, BY_HANDLE_FILE_INFORMATION, FILE_BASIC_INFO, FILE_ID_INFO,
            MOVEFILE_WRITE_THROUGH,
        },
    },
};

const OPEN_REPARSE: u32 = 0x00200000;
const BACKUP: u32 = 0x02000000;
fn native_error(error: windows::core::Error) -> io::Error {
    io::Error::from_raw_os_error(error.code().0 & 0xffff)
}
pub(super) fn valid_leaf(name: &OsStr) -> bool {
    let wide = name.encode_wide().collect::<Vec<_>>();
    if wide.iter().any(|value| matches!(*value, 0 | 47 | 58 | 92))
        || wide.last().is_some_and(|value| matches!(*value, 32 | 46))
    {
        return false;
    }
    let display = name.to_string_lossy();
    let stem = display
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_ascii_uppercase();
    !matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "CLOCK$"
            | "CONIN$"
            | "CONOUT$"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
            | "COM¹"
            | "COM²"
            | "COM³"
            | "LPT¹"
            | "LPT²"
            | "LPT³"
    )
}
pub(super) fn info(file: &fs::File) -> io::Result<Info> {
    let handle = HANDLE(file.as_raw_handle());
    let mut basic = FILE_BASIC_INFO::default();
    let mut id = FILE_ID_INFO::default();
    let mut traditional = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(handle, &mut traditional).map_err(native_error)?;
        GetFileInformationByHandleEx(
            handle,
            FileIdInfo,
            (&mut id as *mut FILE_ID_INFO).cast(),
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )
        .map_err(|_| unsupported())?;
        GetFileInformationByHandleEx(
            handle,
            FileBasicInfo,
            (&mut basic as *mut FILE_BASIC_INFO).cast(),
            std::mem::size_of::<FILE_BASIC_INFO>() as u32,
        )
        .map_err(|_| unsupported())?;
    }
    let index = u128::from_be_bytes(id.FileId.Identifier);
    let birth = ((traditional.ftCreationTime.dwHighDateTime as u64) << 32)
        | traditional.ftCreationTime.dwLowDateTime as u64;
    if index == 0 || birth == 0 {
        return Err(unsupported());
    }
    Ok(Info {
        identity: FileIdentity::native("windows", id.VolumeSerialNumber, index, birth as u128),
        length: ((traditional.nFileSizeHigh as u64) << 32) | traditional.nFileSizeLow as u64,
        modified: basic.LastWriteTime as u64 as u128,
        changed: basic.ChangeTime as u64 as u128,
        links: traditional.nNumberOfLinks as u64,
        regular: traditional.dwFileAttributes & 0x10 == 0,
        redirected: traditional.dwFileAttributes & 0x400 != 0,
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
        let mut current = PathBuf::new();
        let mut anchors = Vec::new();
        let mut identities = Vec::new();
        for component in path.components() {
            match component {
                Component::Prefix(prefix) => {
                    use std::path::Prefix;
                    if !matches!(
                        prefix.kind(),
                        Prefix::Disk(_)
                            | Prefix::VerbatimDisk(_)
                            | Prefix::UNC(_, _)
                            | Prefix::VerbatimUNC(_, _)
                    ) {
                        return Err(unsafe_path());
                    }
                    current.push(prefix.as_os_str());
                    continue;
                }
                Component::RootDir => current.push(component.as_os_str()),
                Component::Normal(name) if valid_leaf(name) => current.push(name),
                _ => return Err(unsafe_path()),
            }
            // Real read access and denied DELETE sharing pin each ancestor.
            // WRITE sharing permits child renames; reparse attributes are
            // checked again during every revalidation.
            let file = fs::OpenOptions::new()
                .read(true)
                .share_mode(3)
                .custom_flags(BACKUP | OPEN_REPARSE)
                .open(&current)?;
            let information = info(&file)?;
            if information.regular || information.redirected {
                return Err(unsafe_path());
            }
            identities.push(information.identity);
            anchors.push(file);
        }
        if anchors.is_empty() {
            return Err(unsafe_path());
        }
        Ok(Self {
            path: current,
            anchors,
            identities,
        })
    }
    pub(super) fn path(&self) -> &Path {
        &self.path
    }
    pub(super) fn identity(&self) -> &FileIdentity {
        self.identities.last().unwrap()
    }
    pub(super) fn verify_case_mode(&self) -> io::Result<()> {
        for file in &self.anchors {
            super::refactor_path_checks::verify_directory_case_mode(file)?;
        }
        Ok(())
    }
    pub(super) fn revalidate(&self) -> io::Result<()> {
        let current = Self::open(&self.path)?;
        if current.identities != self.identities
            || self
                .anchors
                .iter()
                .any(|file| !info(file).is_ok_and(|value| !value.regular && !value.redirected))
        {
            return Err(unsafe_path());
        }
        Ok(())
    }
    pub(super) fn open_file(&self, name: &OsStr) -> io::Result<fs::File> {
        fs::OpenOptions::new()
            .read(true)
            .share_mode(7)
            .custom_flags(OPEN_REPARSE | BACKUP)
            .open(self.path.join(name))
    }
    pub(super) fn exists(&self, name: &OsStr) -> io::Result<bool> {
        match fs::symlink_metadata(self.path.join(name)) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }
    pub(super) fn try_lock(&self, name: &OsStr) -> io::Result<Option<fs::File>> {
        match fs::symlink_metadata(self.path.join(name)) {
            Ok(metadata) => {
                use std::os::windows::fs::MetadataExt;
                if !metadata.is_file()
                    || metadata.file_attributes() & 0x400 != 0
                    || metadata.len() != 0
                {
                    return Err(unsafe_path());
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
        match fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .custom_flags(OPEN_REPARSE | BACKUP)
            .open(self.path.join(name))
        {
            Ok(file) => Ok(Some(file)),
            Err(error) if matches!(error.raw_os_error(), Some(32 | 33)) => Ok(None),
            Err(error) => Err(error),
        }
    }
    pub(super) fn lock_still_named(
        &self,
        name: &OsStr,
        expected: &FileIdentity,
    ) -> io::Result<bool> {
        // Attributes-only access does not request read/write/delete sharing.
        // The live exclusive lock handle prevents replacement/unlink itself.
        let file = fs::OpenOptions::new()
            .access_mode(0x80)
            .share_mode(7)
            .custom_flags(OPEN_REPARSE | BACKUP)
            .open(self.path.join(name))?;
        Ok(info(&file)?.identity == *expected)
    }
    pub(super) fn rename_noreplace(&self, source: &OsStr, target: &OsStr) -> io::Result<()> {
        let wide = |name: &OsStr| {
            self.path
                .join(name)
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>()
        };
        let (source, target) = (wide(source), wide(target));
        // No REPLACE_EXISTING, COPY_ALLOWED or two-stage temporary rename.
        unsafe {
            MoveFileExW(
                PCWSTR(source.as_ptr()),
                PCWSTR(target.as_ptr()),
                MOVEFILE_WRITE_THROUGH,
            )
        }
        .map_err(native_error)
    }
    pub(super) fn sync(&self) -> io::Result<()> {
        // MoveFileExW uses WRITE_THROUGH. A read-only directory handle cannot
        // be FlushFileBuffers'd; do not turn this into a false extra guarantee.
        Ok(())
    }
}
pub(super) fn unlock(_: &fs::File) { /* Closing the exclusive handle releases it. */
}
