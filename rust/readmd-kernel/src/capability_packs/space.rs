//! Additional installation space, not another copy of an existing runtime.
//! The downloaded input and old target already consume the OS-reported free
//! space; staging-to-target and target-to-backup are same-volume renames.
use super::*;

const SAFETY_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, serde::Serialize)]
pub(super) struct Plan {
    pub expanded_bytes: u64,
    pub allocation_bytes: u64,
    pub directory_bytes: u64,
    pub metadata_bytes: u64,
    pub safety_bytes: u64,
    pub additional_bytes: u64,
    pub allocation_unit: u64,
    pub additional_inodes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_inodes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_identity: Option<String>,
    #[cfg(target_os = "macos")]
    #[serde(skip)]
    anchor: Option<macos::Anchor>,
    #[cfg(target_os = "linux")]
    #[serde(skip)]
    anchor: Option<linux::Anchor>,
    #[cfg(windows)]
    #[serde(skip)]
    anchor: Option<windows_anchor::Anchor>,
}

fn round_allocation(bytes: u64, unit: u64) -> ApiResult<u64> {
    if unit == 0 { return Err(error()); }
    bytes.checked_add(unit - 1).and_then(|n| (n / unit).checked_mul(unit)).ok_or_else(error)
}

fn estimate(lock: &Value, allocation_unit: u64) -> ApiResult<Plan> {
    if allocation_unit == 0 || allocation_unit > 16 * 1024 * 1024 { return Err(error()); }
    let files = checked_files(lock)?;
    let mut directories = BTreeSet::new();
    let (mut expanded_bytes, mut allocation_bytes) = (0u64, 0u64);
    for file in files {
        let bytes = file["size"].as_u64().ok_or_else(error)?;
        expanded_bytes = expanded_bytes.checked_add(bytes).ok_or_else(error)?;
        allocation_bytes = allocation_bytes.checked_add(round_allocation(bytes, allocation_unit)?).ok_or_else(error)?;
        let name = file["path"].as_str().ok_or_else(error)?;
        for (index, _) in name.match_indices('/') { directories.insert(&name[..index]); }
    }
    // Include the payload root and control directories. Small files can use
    // considerably more disk than the sum of their logical content lengths.
    let directory_bytes = (directories.len() as u64 + 4).checked_mul(allocation_unit).ok_or_else(error)?;
    let manifest_bytes = serde_json::to_vec(lock).map_err(|_| error())?.len() as u64;
    // Synced ownership and replacement journals can coexist with their atomic
    // replacement. Also reserve one allocation unit per file for metadata.
    let metadata_bytes = round_allocation(manifest_bytes, allocation_unit)?.checked_mul(2)
        .and_then(|n| n.checked_add((files.len() as u64 + 4).checked_mul(allocation_unit)?)).ok_or_else(error)?;
    let additional_bytes = allocation_bytes.checked_add(directory_bytes)
        .and_then(|n| n.checked_add(metadata_bytes)).and_then(|n| n.checked_add(SAFETY_BYTES)).ok_or_else(error)?;
    let additional_inodes = (files.len() as u64).checked_add(directories.len() as u64).and_then(|n|n.checked_add(16)).ok_or_else(error)?;
    Ok(Plan { expanded_bytes, allocation_bytes, directory_bytes, metadata_bytes,
        safety_bytes: SAFETY_BYTES, additional_bytes, allocation_unit,
        additional_inodes, available_inodes: None, available_bytes: None, volume_identity: None,
        #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
        anchor: None })
}

#[cfg(target_os = "linux")]
#[path = "space_linux.rs"]
mod linux;
#[cfg(windows)]
#[path = "space_windows.rs"]
mod windows_anchor;

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::os::{fd::AsRawFd, unix::fs::{MetadataExt, OpenOptionsExt}};

    fn query_error(operation: &str) -> ApiError {
        ApiError::new(422, "capability_install_failed").noted("reason", "space_query").noted("operation", operation)
    }
    fn open(directory: &Path) -> ApiResult<fs::File> {
        real_directory(directory)?;
        fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
            .open(directory).map_err(|_| query_error("open_directory"))
    }
    fn stat(file: &fs::File) -> ApiResult<libc::statfs> {
        let mut value = std::mem::MaybeUninit::<libc::statfs>::uninit();
        if unsafe { libc::fstatfs(file.as_raw_fd(), value.as_mut_ptr()) } != 0 {
            return Err(query_error("fstatfs"));
        }
        Ok(unsafe { value.assume_init() })
    }
    // fsid_t's fields are private on some libc versions. Copy only its two
    // documented 32-bit words, never padding or mount paths into a receipt.
    fn fsid(stat: &libc::statfs) -> [i32; 2] {
        let mut words = [0i32; 2];
        unsafe { std::ptr::copy_nonoverlapping((&stat.f_fsid as *const libc::fsid_t).cast::<u8>(), words.as_mut_ptr().cast::<u8>(), 8); }
        words
    }
    #[derive(Debug)]
    pub(super) struct Anchor {
        lookup: PathBuf,
        directory: PathBuf,
        file: fs::File,
        device: u64,
        inode: u64,
        fsid: [i32; 2],
    }
    impl Anchor {
        pub(super) fn read(directory: &Path) -> ApiResult<(Self, u64, u64, String)> {
            // Reject the caller's final symlink before canonicalizing. Keep
            // the selected canonical namespace and its opened identity.
            let lookup = std::path::absolute(directory).map_err(|_| query_error("absolute"))?;
            let file = open(&lookup)?;
            let canonical = lookup.canonicalize().map_err(|_| query_error("canonicalize"))?;
            let metadata = file.metadata().map_err(|_| query_error("metadata"))?;
            let stat = stat(&file)?;
            let unit = u64::from(stat.f_bsize);
            let available = stat.f_bavail.checked_mul(unit).ok_or_else(|| query_error("available_bytes"))?;
            if unit == 0 || unit > 16 * 1024 * 1024 { return Err(query_error("allocation_unit")); }
            let id = fsid(&stat);
            let identity = format!("macos:{}:{:08x}:{:08x}", metadata.dev(), id[0], id[1]);
            let anchor = Self { lookup, directory: canonical, file, device: metadata.dev(), inode: metadata.ino(), fsid: id };
            anchor.verify()?;
            Ok((anchor, available, unit, identity))
        }
        pub(super) fn verify(&self) -> ApiResult<()> {
            let current = open(&self.lookup)?;
            let metadata = current.metadata().map_err(|_| query_error("metadata"))?;
            if metadata.dev() != self.device || metadata.ino() != self.inode
                || self.lookup.canonicalize().map_err(|_| query_error("canonicalize"))? != self.directory
                || fsid(&stat(&current)?) != self.fsid || fsid(&stat(&self.file)?) != self.fsid {
                return Err(query_error("volume_identity_changed"));
            }
            Ok(())
        }
    }
}

fn fits(plan: &Plan, available_bytes: u64) -> ApiResult<()> {
    if available_bytes < plan.additional_bytes {
        return Err(ApiError::new(507, "capability_install_failed")
            .noted("reason", "insufficient_space")
            .noted_json("required_bytes", json!(plan.additional_bytes))
            .noted_json("available_bytes", json!(available_bytes)));
    }
    Ok(())
}
fn fits_inodes(plan: &Plan, available: Option<u64>) -> ApiResult<()> {
    if let Some(available) = available.filter(|n| *n < plan.additional_inodes) {
        return Err(ApiError::new(507,"capability_install_failed").noted("reason","insufficient_space").noted("dimension","inodes")
            .noted_json("required_inodes",json!(plan.additional_inodes)).noted_json("available_inodes",json!(available)));
    }
    Ok(())
}

#[cfg(windows)]
fn capacity(directory: &Path) -> ApiResult<(u64, u64)> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{core::PCWSTR, Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDiskFreeSpaceW, GetVolumePathNameW}};
    real_directory(directory)?;
    // These volume queries do not consistently inherit longPathAware from
    // their host. Rust obtains the existing directory's absolute Win32
    // extended path by handle; unlike prefix concatenation this also handles
    // relative paths, UNC shares and an already extended input correctly.
    // Keep the caller's directory checks and use this only for the OS query.
    let query_error = |operation: &str, native_code: Option<i32>| {
        let mut error = ApiError::new(422, "capability_install_failed")
            .noted("reason", "space_query").noted("operation", operation);
        if let Some(code) = native_code { error = error.noted_json("native_code", json!(code)); }
        error
    };
    let canonical = directory.canonicalize()
        .map_err(|error| query_error("canonicalize", error.raw_os_error()))?;
    let mut name: Vec<u16> = canonical.as_os_str().encode_wide().collect();
    // UNC roots require a trailing separator. Using one for every existing
    // directory also avoids treating a volume root as drive-relative.
    if name.last() != Some(&(b'\\' as u16)) { name.push(b'\\' as u16); }
    if name.contains(&0) || name.len() >= 32767 { return Err(query_error("path", None)); }
    name.push(0);
    let mut volume = vec![0u16; 32768];
    let (mut available, mut sectors, mut bytes_per_sector) = (0u64, 0u32, 0u32);
    unsafe {
        GetDiskFreeSpaceExW(PCWSTR(name.as_ptr()), Some(&mut available), None, None)
            .map_err(|error| query_error("GetDiskFreeSpaceExW", Some(error.code().0)))?;
        GetVolumePathNameW(PCWSTR(name.as_ptr()), &mut volume)
            .map_err(|error| query_error("GetVolumePathNameW", Some(error.code().0)))?;
        GetDiskFreeSpaceW(PCWSTR(volume.as_ptr()), Some(&mut sectors), Some(&mut bytes_per_sector), None, None)
            .map_err(|error| query_error("GetDiskFreeSpaceW", Some(error.code().0)))?;
    }
    let unit = (sectors as u64).checked_mul(bytes_per_sector as u64).filter(|n| *n > 0)
        .ok_or_else(|| query_error("allocation_unit", None))?;
    Ok((available, unit))
}

pub(super) fn check(directory: &Path, lock: &Value) -> ApiResult<Option<Plan>> {
    #[cfg(windows)] {
        let (anchor, available, unit, identity) = windows_anchor::Anchor::read(directory)?;
        let mut plan = estimate(lock, unit)?;
        fits(&plan, available)?;
        plan.available_bytes = Some(available);
        plan.volume_identity = Some(identity);
        plan.anchor = Some(anchor);
        Ok(Some(plan))
    }
    #[cfg(target_os = "macos")] {
        let (anchor, available, unit, identity) = macos::Anchor::read(directory)?;
        let mut plan = estimate(lock, unit)?;
        fits(&plan, available)?;
        plan.available_bytes = Some(available);
        plan.volume_identity = Some(identity);
        plan.anchor = Some(anchor);
        Ok(Some(plan))
    }
    #[cfg(target_os = "linux")] {
        let (anchor, available, unit, identity, inodes) = linux::Anchor::read(directory)?;
        let mut plan = estimate(lock, unit)?; fits(&plan,available)?; fits_inodes(&plan,inodes)?;
        plan.available_bytes=Some(available);plan.volume_identity=Some(identity);plan.available_inodes=inodes;plan.anchor=Some(anchor);
        Ok(Some(plan))
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))] {
        // Other platforms retain their existing transactional writer. This
        // Windows quota-aware preflight does not certify their disk admission.
        let _ = (directory, lock);
        Ok(None)
    }
}

pub(super) fn verify(plan: &Option<Plan>) -> ApiResult<()> {
    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    if let Some(plan) = plan { plan.anchor.as_ref().ok_or_else(error)?.verify()?; }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    let _ = plan;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Value {
        json!({"schema":1,"id":"fonts","version":"test","platform":"universal","files":[
            {"path":"data/a","size":1,"sha256":"0".repeat(64)},
            {"path":"data/b","size":4097,"sha256":"1".repeat(64)},
            {"path":"empty","size":0,"sha256":"2".repeat(64)}]})
    }
    #[test]
    fn installation_space_accounts_for_clusters_directories_journals_and_headroom() {
        let plan = estimate(&fixture(), 4096).unwrap();
        assert_eq!(plan.expanded_bytes, 4098);
        assert_eq!(plan.allocation_bytes, 3 * 4096);
        assert_eq!(plan.directory_bytes, 5 * 4096);
        assert!(plan.metadata_bytes >= 9 * 4096);
        assert_eq!(plan.additional_bytes, plan.allocation_bytes + plan.directory_bytes + plan.metadata_bytes + SAFETY_BYTES);
        assert!(estimate(&fixture(), 65536).unwrap().additional_bytes > plan.additional_bytes);
    }
    #[test]
    fn installation_space_rejects_underflow_and_keeps_exact_boundary() {
        let plan = estimate(&fixture(), 4096).unwrap();
        let failure = fits(&plan, plan.additional_bytes - 1).unwrap_err();
        assert_eq!(failure.status, 507);assert_eq!(failure.code, "capability_install_failed");
        fits(&plan, plan.additional_bytes).unwrap();
        assert!(estimate(&fixture(), 0).is_err());
        assert!(estimate(&fixture(), 16 * 1024 * 1024 + 1).is_err());
        assert!(round_allocation(u64::MAX, 4096).is_err());
        let mut invalid = fixture();invalid["files"][0]["path"] = json!("../outside");
        assert!(estimate(&invalid, 4096).is_err());
    }
    #[test]
    fn installation_space_checks_finite_inode_budget_independently_of_free_bytes() {
        let plan=estimate(&fixture(),4096).unwrap();
        assert_eq!(plan.additional_inodes,20);
        assert_eq!(fits_inodes(&plan,Some(19)).unwrap_err().status,507);
        fits_inodes(&plan,Some(20)).unwrap();fits_inodes(&plan,None).unwrap();
    }
    #[cfg(target_os="linux")]
    #[test]
    fn installation_space_linux_reads_selected_volume_fragment_size_and_inode_domain() {
        use std::os::unix::ffi::OsStrExt;
        let root=tempfile::tempdir().unwrap();fs::write(root.path().join("original"),b"keep bytes").unwrap();
        let plan=check(root.path(),&fixture()).unwrap().expect("Linux must query the actual allocation domain");
        let name=std::ffi::CString::new(root.path().as_os_str().as_bytes()).unwrap();
        let mut native=std::mem::MaybeUninit::<libc::statvfs>::uninit();assert_eq!(unsafe{libc::statvfs(name.as_ptr(),native.as_mut_ptr())},0);
        let native=unsafe{native.assume_init()};assert_eq!(plan.allocation_unit,native.f_frsize as u64);
        assert!(plan.available_bytes.unwrap()>0);assert!(plan.available_bytes.unwrap()<=native.f_blocks as u64*native.f_frsize as u64);
        assert_eq!(plan.available_inodes.is_some(),native.f_files>0);if let Some(count)=plan.available_inodes{assert!(count<=native.f_files as u64);}
        assert!(plan.volume_identity.as_deref().unwrap().starts_with("linux:"));verify(&Some(plan)).unwrap();
        assert_eq!(fs::read(root.path().join("original")).unwrap(),b"keep bytes");assert_eq!(fs::read_dir(root.path()).unwrap().count(),1);
        assert!(check(&root.path().join("missing"),&fixture()).is_err());
    }
    #[cfg(target_os="linux")]
    #[test]
    fn installation_space_linux_refuses_directory_replacement_and_final_symlink() {
        let root=tempfile::tempdir().unwrap();let directory=root.path().join("runtime");fs::create_dir(&directory).unwrap();
        fs::write(directory.join("old"),b"old pack").unwrap();let plan=check(&directory,&fixture()).unwrap();
        let moved=root.path().join("held");fs::rename(&directory,&moved).unwrap();fs::create_dir(&directory).unwrap();fs::write(directory.join("occupant"),b"unknown").unwrap();
        assert!(verify(&plan).is_err());assert_eq!(fs::read(moved.join("old")).unwrap(),b"old pack");assert_eq!(fs::read(directory.join("occupant")).unwrap(),b"unknown");
        let link=root.path().join("redirect");std::os::unix::fs::symlink(&moved,&link).unwrap();assert!(check(&link,&fixture()).is_err());
    }
    #[cfg(any(target_os="linux",target_os="macos"))]
    #[test]
    fn installation_space_rechecks_the_selected_parent_alias_without_touching_either_directory() {
        let root=tempfile::tempdir().unwrap();let a=root.path().join("a");let b=root.path().join("b");
        for base in [&a,&b]{fs::create_dir_all(base.join("runtime")).unwrap();fs::write(base.join("runtime/original"),base.file_name().unwrap().as_encoded_bytes()).unwrap();}
        let alias=root.path().join("alias");std::os::unix::fs::symlink(&a,&alias).unwrap();let plan=check(&alias.join("runtime"),&fixture()).unwrap();
        fs::remove_file(&alias).unwrap();std::os::unix::fs::symlink(&b,&alias).unwrap();assert!(verify(&plan).is_err());
        assert_eq!(fs::read(a.join("runtime/original")).unwrap(),b"a");assert_eq!(fs::read(b.join("runtime/original")).unwrap(),b"b");
    }
    #[cfg(windows)]
    #[test]
    fn installation_space_windows_pins_namespace_and_reports_real_volume_without_blocking_child_moves() {
        let root=tempfile::tempdir().unwrap();let directory=root.path().join("runtime");fs::create_dir(&directory).unwrap();
        fs::write(directory.join("before"),b"old bytes").unwrap();let plan=check(&directory,&fixture()).unwrap().unwrap();
        assert!(plan.volume_identity.as_deref().unwrap().starts_with("windows:"));assert!(plan.available_bytes.unwrap()>0);
        assert!(fs::rename(&directory,root.path().join("replaced")).is_err());
        fs::rename(directory.join("before"),directory.join("after")).unwrap();verify(&Some(plan)).unwrap();
        assert_eq!(fs::read(directory.join("after")).unwrap(),b"old bytes");
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn installation_space_macos_reads_actual_volume_and_fundamental_allocation_unit() {
        use std::os::unix::ffi::OsStrExt;
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("existing-runtime"), b"keep original").unwrap();
        let plan = check(dir.path(), &fixture()).unwrap().unwrap();
        let name = std::ffi::CString::new(dir.path().as_os_str().as_bytes()).unwrap();
        let mut other = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        assert_eq!(unsafe { libc::statvfs(name.as_ptr(), other.as_mut_ptr()) }, 0);
        let other = unsafe { other.assume_init() };
        assert_eq!(plan.allocation_unit, other.f_frsize);
        assert!(plan.available_bytes.unwrap() > 0);
        assert!(plan.volume_identity.as_deref().unwrap().starts_with("macos:"));
        assert!(plan.additional_bytes > plan.expanded_bytes);
        let mut too_large = estimate(&fixture(), plan.allocation_unit).unwrap();
        too_large.additional_bytes = plan.available_bytes.unwrap().checked_add(plan.allocation_unit).unwrap();
        assert_eq!(fits(&too_large, plan.available_bytes.unwrap()).unwrap_err().status, 507);
        verify(&Some(plan)).unwrap();
        assert_eq!(fs::read(dir.path().join("existing-runtime")).unwrap(), b"keep original");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        assert!(check(&dir.path().join("missing"), &fixture()).is_err());
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn installation_space_macos_refuses_replaced_directory_and_symlink_without_cleanup() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("runtime"); fs::create_dir(&directory).unwrap();
        fs::write(directory.join("old-pack"), b"old bytes").unwrap();
        let plan = check(&directory, &fixture()).unwrap();
        let moved = root.path().join("moved"); fs::rename(&directory, &moved).unwrap();
        fs::create_dir(&directory).unwrap(); fs::write(directory.join("outside"), b"occupant").unwrap();
        assert!(verify(&plan).is_err());
        assert_eq!(fs::read(moved.join("old-pack")).unwrap(), b"old bytes");
        assert_eq!(fs::read(directory.join("outside")).unwrap(), b"occupant");
        let link = root.path().join("redirect");
        std::os::unix::fs::symlink(&moved, &link).unwrap();
        assert!(check(&link, &fixture()).is_err());
        assert_eq!(fs::read_dir(&moved).unwrap().count(), 1);
    }
    #[cfg(windows)]
    #[test]
    fn installation_space_reads_real_caller_capacity_without_allocating_payload() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("existing-runtime"), b"unchanged previous version").unwrap();
        let (available, unit) = capacity(dir.path()).unwrap();
        assert!(available > 0);assert!(unit.is_power_of_two());
        let mut plan = estimate(&fixture(), unit).unwrap();
        // Use the real OS result to test rejection without filling the user's
        // disk. This is not a real low-volume or hard-quota installation test.
        plan.additional_bytes = available.checked_add(unit).unwrap();
        assert_eq!(fits(&plan, available).unwrap_err().status, 507);
        assert_eq!(fs::read(dir.path().join("existing-runtime")).unwrap(), b"unchanged previous version");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        assert!(capacity(&dir.path().join("missing")).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn installation_space_queries_long_unicode_and_already_extended_directories() {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        let temporary = tempfile::tempdir().unwrap();
        let mut directory = temporary.path().to_path_buf();
        for index in 0..6 { directory.push(format!("{index}-ReadMD 中文 publication {}", "x".repeat(45))); }
        fs::create_dir_all(&directory).unwrap();
        assert!(directory.as_os_str().encode_wide().count() > 400);
        let sentinel = directory.join("unchanged.txt");
        fs::write(&sentinel, b"existing installation remains untouched").unwrap();

        // tempfile may itself return an extended path. Force a normal drive
        // path as well so the test cannot accidentally exercise only \\?\.
        let extended = directory.canonicalize().unwrap();
        let wide = extended.as_os_str().encode_wide().collect::<Vec<_>>();
        assert!(wide.starts_with(&[92, 92, 63, 92]));
        let ordinary_wide = if wide.starts_with(&[92, 92, 63, 92, 85, 78, 67, 92]) {
            [vec![92, 92], wide[8..].to_vec()].concat()
        } else { wide[4..].to_vec() };
        let ordinary = PathBuf::from(std::ffi::OsString::from_wide(&ordinary_wide));
        let (_, short_unit) = capacity(temporary.path()).unwrap();
        for path in [&ordinary, &extended, &directory.join(".")] {
            let (available, unit) = capacity(path).unwrap();
            assert!(available > 0);
            assert_eq!(unit, short_unit);
            assert!(check(path, &fixture()).unwrap().is_some());
        }
        assert_eq!(fs::read(&sentinel).unwrap(), b"existing installation remains untouched");
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        assert!(capacity(&directory.join("missing")).is_err());
    }
}
