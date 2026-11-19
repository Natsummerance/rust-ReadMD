//! Checked Windows replacement shared by settings and atomic document writes.
use std::{fs, io, os::windows::ffi::OsStrExt, path::Path};
use windows::{core::PCWSTR, Win32::Storage::FileSystem::{
    MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
}};

pub(crate) fn replace(
    from: &Path,
    to: &Path,
    staged_file: &fs::File,
    before_published_sync: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    let from_wide = from.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
    let to_wide = to.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
    match unsafe {
        MoveFileExW(PCWSTR(from_wide.as_ptr()), PCWSTR(to_wide.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)
    } {
        Ok(()) => Ok(()),
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
            fs::rename(from, to)?;
            before_published_sync()?;
            // Keep the original staged object across rename. A sync error is
            // post-publication uncertainty and must propagate to the caller.
            staged_file.sync_all()
        }
    }
}
