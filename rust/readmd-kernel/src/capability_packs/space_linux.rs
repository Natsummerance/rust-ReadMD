//! Query and bind the selected Linux allocation domain, not the host root FS.
use super::*;
use std::os::{
    fd::AsRawFd,
    unix::fs::{MetadataExt, OpenOptionsExt},
};

fn query_error(operation: &str) -> ApiError {
    ApiError::new(422, "capability_install_failed")
        .noted("reason", "space_query")
        .noted("operation", operation)
}
fn open(directory: &Path) -> ApiResult<fs::File> {
    real_directory(directory).map_err(|_| query_error("open_directory"))?;
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY | libc::O_NONBLOCK)
        .open(directory)
        .map_err(|_| query_error("open_directory"))
}
fn stat(file: &fs::File) -> ApiResult<libc::statvfs> {
    let mut value = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::fstatvfs(file.as_raw_fd(), value.as_mut_ptr()) } != 0 {
        return Err(query_error("fstatvfs"));
    }
    Ok(unsafe { value.assume_init() })
}
fn mount_id(file: &fs::File) -> ApiResult<u64> {
    let mut value = std::mem::MaybeUninit::<libc::statx>::uninit();
    if unsafe {
        libc::statx(
            file.as_raw_fd(),
            b"\0".as_ptr().cast(),
            libc::AT_EMPTY_PATH,
            libc::STATX_MNT_ID,
            value.as_mut_ptr(),
        )
    } != 0
    {
        return Err(query_error("statx_mount_id"));
    }
    let value = unsafe { value.assume_init() };
    if value.stx_mask & libc::STATX_MNT_ID == 0 || value.stx_mnt_id == 0 {
        return Err(query_error("statx_mount_id"));
    }
    Ok(value.stx_mnt_id)
}
#[derive(Debug)]
pub(super) struct Anchor {
    lookup: PathBuf,
    canonical: PathBuf,
    file: fs::File,
    device: u64,
    inode: u64,
    mount: u64,
    fsid: u64,
}
impl Anchor {
    pub(super) fn read(directory: &Path) -> ApiResult<(Self, u64, u64, String, Option<u64>)> {
        let lookup = std::path::absolute(directory).map_err(|_| query_error("absolute"))?;
        let file = open(&lookup)?;
        let canonical = lookup
            .canonicalize()
            .map_err(|_| query_error("canonicalize"))?;
        let metadata = file.metadata().map_err(|_| query_error("metadata"))?;
        let capacity = stat(&file)?;
        let unit = capacity.f_frsize as u64;
        if unit == 0 || unit > 16 * 1024 * 1024 {
            return Err(query_error("allocation_unit"));
        }
        let available = (capacity.f_bavail as u64)
            .checked_mul(unit)
            .ok_or_else(|| query_error("available_bytes"))?;
        // Dynamic-inode filesystems can report no finite total. Do not
        // interpret that as zero available inode capacity.
        let inodes = (capacity.f_files > 0).then_some(capacity.f_favail as u64);
        let mount = mount_id(&file)?;
        let fsid = capacity.f_fsid as u64;
        let identity = format!("linux:{}:{fsid:016x}:{mount}", metadata.dev());
        let anchor = Self {
            lookup,
            canonical,
            file,
            device: metadata.dev(),
            inode: metadata.ino(),
            mount,
            fsid,
        };
        anchor.verify()?;
        Ok((anchor, available, unit, identity, inodes))
    }
    pub(super) fn verify(&self) -> ApiResult<()> {
        let current = open(&self.lookup)?;
        let metadata = current.metadata().map_err(|_| query_error("metadata"))?;
        if metadata.dev() != self.device
            || metadata.ino() != self.inode
            || self
                .lookup
                .canonicalize()
                .map_err(|_| query_error("canonicalize"))?
                != self.canonical
            || mount_id(&current)? != self.mount
            || mount_id(&self.file)? != self.mount
            || stat(&current)?.f_fsid as u64 != self.fsid
            || stat(&self.file)?.f_fsid as u64 != self.fsid
        {
            return Err(query_error("volume_identity_changed"));
        }
        Ok(())
    }
}
