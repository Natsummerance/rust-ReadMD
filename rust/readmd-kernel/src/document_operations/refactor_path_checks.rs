//! Directory case-mode admission for lossless document refactors.
//!
//! Query the exact directory handle already pinned by `native::Directory`.
//! This grants no pathname authority, does not open or follow a path, and does
//! not change a directory's flags. Refactor admission and every revalidation
//! must call it for every ancestor anchor. An unsupported query fails closed.

use std::{fs::File, io};

#[cfg(windows)]
use std::os::windows::{fs::MetadataExt, io::AsRawHandle};
#[cfg(windows)]
use windows::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::{
        FileCaseSensitiveInfo, GetFileInformationByHandleEx, FILE_CASE_SENSITIVE_INFO,
    },
};

/// Require a known case-insensitive Windows directory, queried by object handle.
///
/// `FILE_CASE_SENSITIVE_INFO::Flags == 0` is the only accepted Windows result.
/// The enabled bit, future unknown bits, and every query failure are unsupported.
/// A zero-initialized output buffer is never accepted after a failed query.
/// Non-Windows planners compare path components exactly, so no case-mode query
/// is necessary there. The supplied handle must still refer to a directory.
///
/// Holding a directory handle pins object identity, but this query is a current
/// observation; it is not a lock against another process changing its flags.
/// Callers must repeat it on the retained anchors at every refactor verification.
pub(crate) fn verify_directory_case_mode(file: &File) -> io::Result<()> {
    let metadata = file.metadata()?;
    if !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "document refactor case-mode handle is not a directory",
        ));
    }
    #[cfg(windows)]
    {
        // The caller also verifies native file identity and reparse ancestry.
        // Refuse a reparse directory here as an independent local precondition.
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return validate_case_query(None);
        }
        let mut information = FILE_CASE_SENSITIVE_INFO::default();
        let queried = unsafe {
            GetFileInformationByHandleEx(
                HANDLE(file.as_raw_handle()),
                FileCaseSensitiveInfo,
                (&mut information as *mut FILE_CASE_SENSITIVE_INFO).cast(),
                std::mem::size_of::<FILE_CASE_SENSITIVE_INFO>() as u32,
            )
        };
        validate_case_query(queried.ok().map(|()| information.Flags))
    }
    #[cfg(not(windows))]
    {
        Ok(())
    }
}

#[cfg(any(windows, test))]
fn validate_case_query(flags: Option<u32>) -> io::Result<()> {
    if flags == Some(0) {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "document refactor requires a known case-insensitive directory",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_successful_zero_flag_query_is_admitted() {
        assert!(validate_case_query(Some(0)).is_ok());
        // Bit 0 is FILE_CS_FLAG_CASE_SENSITIVE_DIR. Unknown future flag bits
        // must not silently weaken the planner's case-comparison assumptions.
        for flags in [None, Some(1), Some(2), Some(3), Some(u32::MAX)] {
            assert_eq!(
                validate_case_query(flags).unwrap_err().kind(),
                io::ErrorKind::Unsupported
            );
        }
    }

    #[test]
    fn a_regular_file_handle_is_rejected_before_the_directory_query() {
        let file = tempfile::NamedTempFile::new().unwrap();
        assert_eq!(
            verify_directory_case_mode(file.as_file())
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[cfg(windows)]
    #[test]
    fn queries_an_actual_default_windows_directory_handle() {
        use std::os::windows::fs::OpenOptionsExt;
        let directory = tempfile::tempdir().unwrap();
        let handle = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .custom_flags(0x02000000 | 0x00200000) // BACKUP_SEMANTICS | OPEN_REPARSE_POINT
            .open(directory.path())
            .unwrap();
        verify_directory_case_mode(&handle).unwrap();
    }
}
