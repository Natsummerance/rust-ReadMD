//! Bounded replacement compatibility. This does not claim to copy every NTFS
//! backup stream or privileged audit SACL; unsupported metadata is preserved by
//! refusing the replacement or retaining its original backup.
use super::*;
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::HANDLE,
        Security::{
            GetKernelObjectSecurity, ATTRIBUTE_SECURITY_INFORMATION, DACL_SECURITY_INFORMATION,
            GROUP_SECURITY_INFORMATION, LABEL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
            PSECURITY_DESCRIPTOR, SCOPE_SECURITY_INFORMATION,
        },
        Storage::FileSystem::{
            FindClose, FindFirstStreamW, FindNextStreamW, FindStreamInfoStandard,
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_ARCHIVE,
            FILE_ATTRIBUTE_NORMAL, WIN32_FIND_STREAM_DATA,
        },
    },
};

const MAX_SECURITY_BYTES: usize = 64 * 1024;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Metadata {
    attributes: u32,
    security: Vec<u8>,
}

impl Metadata {
    pub(super) fn revision(&self) -> String {
        let mut digest = Sha256::new();
        digest.update(b"readmd-windows-metadata-v1\0");
        digest.update(self.attributes.to_le_bytes());
        digest.update(&self.security);
        format!("{:x}", digest.finalize())
    }
}

fn unsupported_metadata() -> Error {
    Error::new(ErrorKind::Unsupported)
}

fn attributes(file: &fs::File) -> Result<u32> {
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut information) }
        .map_err(|_| unsupported_metadata())?;
    // Reject hidden/system/readonly, compressed/encrypted/sparse, offline,
    // reparse/cloud recall, EA and all other nonordinary attributes.
    let allowed = FILE_ATTRIBUTE_ARCHIVE.0 | FILE_ATTRIBUTE_NORMAL.0;
    if information.dwFileAttributes & !allowed != 0 {
        return Err(unsupported_metadata());
    }
    Ok(information.dwFileAttributes)
}

fn security(file: &fs::File) -> Result<Vec<u8>> {
    // These portions are queryable with READ_CONTROL (part of GENERIC_READ).
    // Do not request SACL_SECURITY_INFORMATION or enable any token privilege.
    let requested = OWNER_SECURITY_INFORMATION.0
        | GROUP_SECURITY_INFORMATION.0
        | DACL_SECURITY_INFORMATION.0
        | LABEL_SECURITY_INFORMATION.0
        | ATTRIBUTE_SECURITY_INFORMATION.0
        | SCOPE_SECURITY_INFORMATION.0;
    let handle = HANDLE(file.as_raw_handle());
    let mut needed = 0u32;
    let first = unsafe { GetKernelObjectSecurity(handle, requested, None, 0, &mut needed) };
    if !first
        .as_ref()
        .err()
        .is_some_and(|error| error.code().0 & 0xffff == 122)
        || needed == 0
        || needed as usize > MAX_SECURITY_BYTES
    {
        return Err(unsupported_metadata());
    }
    // DWORD-aligned storage for the self-relative security descriptor. Zeroed
    // padding makes its exact byte comparison deterministic and fail-closed.
    let capacity = needed;
    let mut storage = vec![0u32; (capacity as usize).div_ceil(4)];
    unsafe {
        GetKernelObjectSecurity(
            handle,
            requested,
            Some(PSECURITY_DESCRIPTOR(storage.as_mut_ptr().cast())),
            capacity,
            &mut needed,
        )
    }
    .map_err(|_| unsupported_metadata())?;
    if needed == 0 || needed > capacity {
        return Err(unsupported_metadata());
    }
    Ok(unsafe {
        std::slice::from_raw_parts(storage.as_ptr().cast::<u8>(), needed as usize).to_vec()
    })
}

struct StreamSearch(HANDLE);
impl Drop for StreamSearch {
    fn drop(&mut self) {
        let _ = unsafe { FindClose(self.0) };
    }
}

fn no_extra_streams(path: &Path) -> Result<()> {
    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut stream = WIN32_FIND_STREAM_DATA::default();
    let search = StreamSearch(
        unsafe {
            FindFirstStreamW(
                PCWSTR(wide.as_ptr()),
                FindStreamInfoStandard,
                (&mut stream as *mut WIN32_FIND_STREAM_DATA).cast(),
                None,
            )
        }
        .map_err(|_| unsupported_metadata())?,
    );
    let length = stream
        .cStreamName
        .iter()
        .position(|ch| *ch == 0)
        .ok_or_else(unsupported_metadata)?;
    if &stream.cStreamName[..length] != "::$DATA".encode_utf16().collect::<Vec<_>>().as_slice() {
        return Err(unsupported_metadata());
    }
    // One unnamed stream is the only supported layout. Enumeration failure or
    // a second stream is never treated as proof that extra data is absent.
    match unsafe {
        FindNextStreamW(
            search.0,
            (&mut stream as *mut WIN32_FIND_STREAM_DATA).cast(),
        )
    } {
        Err(error) if error.code().0 & 0xffff == 38 => Ok(()),
        _ => Err(unsupported_metadata()),
    }
}

pub(super) fn capture(file: &fs::File, path: &Path) -> Result<Metadata> {
    let attributes = attributes(file)?;
    no_extra_streams(path)?;
    let security = security(file)?;
    Ok(Metadata {
        attributes,
        security,
    })
}
