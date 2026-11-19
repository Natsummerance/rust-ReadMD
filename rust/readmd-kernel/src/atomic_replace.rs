//! Checked Windows replacement shared by settings and atomic document writes.
use std::{fs, io, os::windows::{ffi::OsStrExt, fs::MetadataExt, io::AsRawHandle}, path::Path};
use windows::{core::PCWSTR, Win32::{Foundation::HANDLE, Storage::FileSystem::{
    FileBasicInfo, MoveFileExW, SetFileInformationByHandle, FILE_BASIC_INFO,
    FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_TEMPORARY, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
}}};

fn set_attributes(file: &fs::File, attributes: u32) -> io::Result<()> {
    // Zero timestamps leave the existing times unchanged. Only update the
    // attributes on this original handle, never a newly resolved pathname.
    let basic = FILE_BASIC_INFO { FileAttributes: attributes, ..Default::default() };
    unsafe {
        SetFileInformationByHandle(HANDLE(file.as_raw_handle()), FileBasicInfo,
            (&basic as *const FILE_BASIC_INFO).cast(), std::mem::size_of::<FILE_BASIC_INFO>() as u32)
    }.map_err(|error| io::Error::from_raw_os_error(error.code().0 & 0xffff))
}

pub(crate) fn replace(
    temporary: &mut tempfile::TempPath,
    to: &Path,
    staged_file: &fs::File,
    before_published_sync: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    // NamedTempFile starts with TEMPORARY, which must not survive a successful
    // persistence. Preserve the other staged attributes and refuse publication
    // if clearing this bit fails, as tempfile::persist does before its rename.
    let attributes = staged_file.metadata()?.file_attributes();
    if attributes & FILE_ATTRIBUTE_TEMPORARY.0 != 0 {
        let permanent = attributes & !FILE_ATTRIBUTE_TEMPORARY.0;
        set_attributes(staged_file, if permanent == 0 { FILE_ATTRIBUTE_NORMAL.0 } else { permanent })?;
    }
    let from_wide = temporary.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
    let to_wide = to.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
    match unsafe {
        MoveFileExW(PCWSTR(from_wide.as_ptr()), PCWSTR(to_wide.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)
    } {
        Ok(()) => {
            temporary.disable_cleanup(true);
            Ok(())
        }
        Err(error) => {
            // Capture the API's error, not a later thread-local LastError.
            let code = error.code().0;
            if code != 0x80070005u32 as i32 {
                return Err(io::Error::from_raw_os_error(code & 0xffff));
            }
            // A reader sharing DELETE can still deny legacy MoveFileEx
            // replacement. Rust >= our 1.88 MSRV uses FileRenameInfoEx/POSIX
            // on this precise ACCESS_DENIED case. Unsupported filesystems,
            // older OSes and deny-DELETE readers still fail safely.
            fs::rename(&*temporary, to)?;
            // Match TempPath::persist: after a successful move the old name
            // belongs to nobody here, even if a later sync reports failure.
            temporary.disable_cleanup(true);
            before_published_sync()?;
            // Keep the original staged object across rename. A sync error is
            // post-publication uncertainty and must propagate to the caller.
            staged_file.sync_all()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, os::windows::fs::OpenOptionsExt};

    #[test]
    fn replacement_clears_temporary_and_preserves_other_staged_attributes() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("receipt.json");
        fs::write(&target, b"old receipt").unwrap();
        let reader = fs::OpenOptions::new().read(true).share_mode(7).open(&target).unwrap();
        let mut staged = tempfile::NamedTempFile::new_in(root.path()).unwrap();
        staged.write_all(b"new receipt").unwrap();
        let attributes = staged.as_file().metadata().unwrap().file_attributes();
        assert_ne!(attributes & FILE_ATTRIBUTE_TEMPORARY.0, 0);
        let hidden = windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_HIDDEN.0;
        set_attributes(staged.as_file(), attributes | hidden).unwrap();
        staged.as_file().sync_all().unwrap();
        let (file, mut temporary) = staged.into_parts();
        replace(&mut temporary, &target, &file, || Ok(())).unwrap();
        let published = file.metadata().unwrap().file_attributes();
        assert_eq!(published & FILE_ATTRIBUTE_TEMPORARY.0, 0);
        assert_ne!(published & hidden, 0);
        assert_eq!(fs::read(&target).unwrap(), b"new receipt");
        let reused_name = temporary.to_path_buf();
        fs::write(&reused_name, b"external reused name").unwrap();
        drop(temporary);
        assert_eq!(fs::read(reused_name).unwrap(), b"external reused name");
        drop(reader);
    }

    #[test]
    fn failed_attribute_update_keeps_the_existing_destination() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("receipt.json");
        fs::write(&target, b"preserve original").unwrap();
        let staged = tempfile::NamedTempFile::new_in(root.path()).unwrap();
        assert_ne!(staged.as_file().metadata().unwrap().file_attributes() & FILE_ATTRIBUTE_TEMPORARY.0, 0);
        // GENERIC_READ cannot set basic attributes. The original writable
        // handle remains alive, so this checks failure before any rename.
        let reader = fs::OpenOptions::new().read(true).share_mode(7).open(staged.path()).unwrap();
        let (_file, mut temporary) = staged.into_parts();
        assert!(replace(&mut temporary, &target, &reader, || Ok(())).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"preserve original");
        assert!(temporary.exists());
    }
}
