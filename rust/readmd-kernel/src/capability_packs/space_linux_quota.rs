//! Combine filesystem availability with limits on newly created files. All
//! native queries use the installation anchor's descriptor and mount ID.
use super::{query_error, ApiResult};
use std::{
    fs::File,
    io,
    os::fd::AsRawFd,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Default)]
struct Policy {
    user: bool,
    project: bool,
    project_statfs: bool,
    xfs: bool,
}

fn mount_policy(info: &[u8], mount: u64) -> Option<Policy> {
    let id = mount.to_string();
    for line in info.split(|b| *b == b'\n') {
        let mut fields = line.split(|b| *b == b' ');
        if fields.next() != Some(id.as_bytes()) {
            continue;
        }
        fields.find(|&f| f == b"-")?;
        let fs_type = fields.next()?;
        fields.next()?; // source may contain escaped or non-UTF-8 path bytes.
        let mut p = Policy {
            xfs: fs_type == b"xfs",
            project_statfs: matches!(fs_type, b"xfs" | b"ext4"),
            ..Policy::default()
        };
        for opt in fields.next()?.split(|b| *b == b',') {
            match opt {
                b"usrquota" | b"uquota" | b"quota" => p.user = true,
                b"prjquota" | b"pquota" => p.project = true,
                _ if opt.starts_with(b"usrjquota=") => p.user = true,
                _ => {}
            }
        }
        return Some(p);
    }
    None
}

#[repr(C)]
#[derive(Default)]
struct FsXattr {
    flags: u32,
    extsize: u32,
    nextents: u32,
    project: u32,
    cowextsize: u32,
    pad: [u8; 8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Project {
    id: u32,
    inherit: bool,
}

impl Project {
    pub(super) fn read(file: &File) -> ApiResult<Self> {
        let mut attrs = FsXattr::default();
        let request = libc::_IOR::<FsXattr>(u32::from(b'X'), 31);
        // FS_IOC_FSGETXATTR writes exactly one initialized UAPI fsxattr.
        if unsafe { libc::ioctl(file.as_raw_fd(), request, &mut attrs) } < 0 {
            return Err(query_error("project_identity"));
        }
        Ok(Self {
            id: attrs.project,
            inherit: attrs.flags & 0x200 != 0,
        })
    }
}

// Q_GETQUOTA reports limits in 1024-byte units, current space in bytes,
// inode values in units of one, and grace expiration in epoch seconds.
fn quota(file: &File, kind: i32, id: u32) -> io::Result<libc::dqblk> {
    let mut record = std::mem::MaybeUninit::<libc::dqblk>::zeroed();
    let op = libc::QCMD(libc::Q_GETQUOTA, kind);
    if unsafe {
        libc::syscall(
            libc::SYS_quotactl_fd,
            file.as_raw_fd(),
            op,
            id,
            record.as_mut_ptr(),
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { record.assume_init() })
}

fn remaining(hard: u64, soft: u64, expiry: u64, used: u64, unit: u64, now: u64) -> Option<u64> {
    [Some(hard), (expiry != 0 && expiry <= now).then_some(soft)]
        .into_iter()
        .flatten()
        .filter(|v| *v != 0)
        .min()
        .map(|limit| limit.saturating_mul(unit).saturating_sub(used))
}

fn bound(
    record: libc::dqblk,
    now: u64,
    bytes: &mut u64,
    inodes: &mut Option<u64>,
) -> ApiResult<()> {
    let valid = record.dqb_valid;
    if valid & libc::QIF_LIMITS != libc::QIF_LIMITS || valid & libc::QIF_USAGE != libc::QIF_USAGE {
        return Err(query_error("quota_usage"));
    }
    if let Some(limit) = remaining(
        record.dqb_bhardlimit,
        record.dqb_bsoftlimit,
        record.dqb_btime,
        record.dqb_curspace,
        1024,
        now,
    ) {
        *bytes = (*bytes).min(limit);
    }
    if let Some(limit) = remaining(
        record.dqb_ihardlimit,
        record.dqb_isoftlimit,
        record.dqb_itime,
        record.dqb_curinodes,
        1,
        now,
    ) {
        *inodes = Some(inodes.unwrap_or(limit).min(limit));
    }
    Ok(())
}

pub(super) fn apply(
    file: &File,
    mount: u64,
    bytes: &mut u64,
    inodes: &mut Option<u64>,
) -> ApiResult<Option<Project>> {
    let info = std::fs::read("/proc/self/mountinfo").map_err(|_| query_error("quota_mount"))?;
    let p = mount_policy(&info, mount).ok_or_else(|| query_error("quota_mount"))?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| query_error("quota_clock"))?
        .as_secs();
    let uid = unsafe { libc::geteuid() };
    if p.user && !(p.xfs && uid == 0) {
        let record = quota(file, libc::USRQUOTA, uid).map_err(|_| query_error("user_quota"))?;
        bound(record, now, bytes, inodes)?;
    }
    if !p.project {
        return Ok(None);
    }
    let project = Project::read(file)?;
    let new_project = if project.inherit { project.id } else { 0 };
    // XFS reserves ID 0 for defaults and never enforces its limits.
    if p.xfs && new_project == 0 {
        return Ok(Some(project));
    }
    match quota(file, 2 /* PRJQUOTA */, new_project) {
        Ok(record) => bound(record, now, bytes, inodes)?,
        // On ext4 and XFS, fstatvfs on an inheriting directory already
        // includes that project's limits even for unprivileged callers.
        Err(e)
            if project.inherit
                && p.project_statfs
                && matches!(
                    e.raw_os_error(),
                    Some(libc::EPERM | libc::EACCES | libc::ENOSYS)
                ) => {}
        Err(_) => return Err(query_error("project_quota")),
    }
    Ok(Some(project))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enforced_mount_policies_and_quota_units() {
        let info=b"40 1 8:1 / /a rw - xfs /dev/a rw,uqnoenforce,pqnoenforce\n41 1 8:2 / /b rw shared:1 - ext4 /dev/b rw,usrquota,prjquota\n";
        assert!(!mount_policy(info, 40).unwrap().user);
        let p = mount_policy(info, 41).unwrap();
        assert!(p.user && p.project && p.project_statfs);
        assert_eq!(remaining(8, 4, 10, 4095, 1024, 10), Some(1));
        assert_eq!(remaining(8, 4, 10, 4095, 1024, 9), Some(4097));
        assert_eq!(remaining(0, 0, 0, 18, 1, 10), None);
    }
}
