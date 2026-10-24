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
    Ok(Plan { expanded_bytes, allocation_bytes, directory_bytes, metadata_bytes,
        safety_bytes: SAFETY_BYTES, additional_bytes, allocation_unit })
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

#[cfg(windows)]
fn capacity(directory: &Path) -> ApiResult<(u64, u64)> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{core::PCWSTR, Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDiskFreeSpaceW, GetVolumePathNameW}};
    real_directory(directory)?;
    let name: Vec<u16> = directory.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut volume = vec![0u16; 32768];
    let (mut available, mut sectors, mut bytes_per_sector) = (0u64, 0u32, 0u32);
    let query_error = || ApiError::new(422, "capability_install_failed").noted("reason", "space_query");
    unsafe {
        GetDiskFreeSpaceExW(PCWSTR(name.as_ptr()), Some(&mut available), None, None).map_err(|_| query_error())?;
        GetVolumePathNameW(PCWSTR(name.as_ptr()), &mut volume).map_err(|_| query_error())?;
        GetDiskFreeSpaceW(PCWSTR(volume.as_ptr()), Some(&mut sectors), Some(&mut bytes_per_sector), None, None).map_err(|_| query_error())?;
    }
    let unit = (sectors as u64).checked_mul(bytes_per_sector as u64).filter(|n| *n > 0).ok_or_else(query_error)?;
    Ok((available, unit))
}

pub(super) fn check(directory: &Path, lock: &Value) -> ApiResult<Option<Plan>> {
    #[cfg(windows)] {
        let (available, unit) = capacity(directory)?;
        let plan = estimate(lock, unit)?;
        fits(&plan, available)?;
        Ok(Some(plan))
    }
    #[cfg(not(windows))] {
        // Other platforms retain their existing transactional writer. This
        // Windows quota-aware preflight does not certify their disk admission.
        let _ = (directory, lock);
        Ok(None)
    }
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
}
