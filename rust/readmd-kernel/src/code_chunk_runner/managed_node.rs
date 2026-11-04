//! Application-bound Node resources. No PATH fallback, downloads or user
//! configuration discovery. The compatibility runner is a separate caller.
use super::*;
use crate::{ApiError, ApiResult};
use sha2::{Digest, Sha256};
use std::sync::atomic::AtomicUsize;

pub(crate) const VERSION: &str = "24.22.2";
static ACTIVE: AtomicUsize = AtomicUsize::new(0);

fn failure(code: &str, lang: &str) -> Map<String, Value> {
    let mut result = denied_dict(code, lang);
    insert_str(&mut result, "error_code", code);
    result
}
fn check(deadline: Instant) -> ApiResult<()> {
    if crate::speech::cancelled() { return Err(ApiError::new(422, "execution_cancelled")); }
    if Instant::now() >= deadline { return Err(ApiError::new(422, "execution_timeout")); }
    Ok(())
}
fn version_at_least(value: &str, major: u32, minor: u32) -> bool {
    let mut parts = value.trim().split('.').map(str::parse::<u32>);
    matches!((parts.next(), parts.next()), (Some(Ok(a)), Some(Ok(b))) if (a, b) >= (major, minor))
}
fn platform_supported() -> bool {
    #[cfg(target_os = "macos")]
    {
        let mut bytes = [0u8; 128]; let mut length = bytes.len();
        let result = unsafe { libc::sysctlbyname(c"kern.osproductversion".as_ptr(), bytes.as_mut_ptr().cast(), &mut length, std::ptr::null_mut(), 0) };
        return result == 0 && length <= bytes.len() && std::ffi::CStr::from_bytes_until_nul(&bytes)
            .ok().and_then(|s| s.to_str().ok()).is_some_and(|v| version_at_least(v, 13, 5));
    }
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        let value = unsafe { std::ffi::CStr::from_ptr(libc::gnu_get_libc_version()) };
        return value.to_str().ok().is_some_and(|v| version_at_least(v, 2, 28));
    }
    #[cfg(windows)]
    {
        #[repr(C)] struct Version { size: u32, major: u32, minor: u32, build: u32, platform: u32, service: [u16; 128] }
        #[link(name = "ntdll")] extern "system" { fn RtlGetVersion(info: *mut Version) -> i32; }
        let mut value = Version { size: std::mem::size_of::<Version>() as u32, major: 0, minor: 0, build: 0, platform: 0, service: [0; 128] };
        return unsafe { RtlGetVersion(&mut value) } >= 0 && value.major >= 10;
    }
    #[allow(unreachable_code)] false
}
fn binary_name() -> &'static str { if cfg!(windows) { "node.exe" } else { "node" } }

pub(super) fn execute(paths: &crate::paths::AppPaths, code: &str, lang: &str, timeout: u64, cwd: Option<&Path>) -> Map<String, Value> {
    let timeout = clamp_timeout(timeout);
    let deadline = Instant::now() + Duration::from_secs(timeout);
    if let Err(error) = check(deadline) { return failure(&error.code, lang); }
    if !platform_supported() { return failure("capability_pack_unavailable", lang); }
    if let Err(error) = crate::capability_process::check_worker_capacity(&paths.data_dir, "node") {
        return failure(&error.code, lang);
    }
    let resources = crate::capability_packs::lease_reserved(&paths.data_dir, &paths.assets_dir, "node", &|| check(deadline), || {
        crate::capability_process::reserve_worker(&paths.data_dir, "node", &ACTIVE)
    });
    let (lease, _worker) = match resources {
        Ok(resources) => resources,
        Err(error) => return failure(match error.code.as_str() {
            "cancelled" | "execution_cancelled" => "execution_cancelled",
            "execution_timeout" => "execution_timeout",
            "capability_install_busy" => "capability_install_busy",
            _ => "capability_pack_unavailable",
        }, lang),
    };
    if let Err(error) = check(deadline) { return failure(&error.code, lang); }
    let lock = match crate::capability_packs::resource_lock(&paths.assets_dir, "node") {
        Ok(lock) => lock, Err(_) => return failure("capability_pack_unavailable", lang),
    };
    let manifest = match serde_json::to_vec(&lock) {
        Ok(bytes) => format!("{:x}", Sha256::digest(bytes)),
        Err(_) => return failure("capability_pack_unavailable", lang),
    };
    let (script, _directory) = match write_temp_script(".js", code, cwd) {
        Ok(script) => script, Err(_) => return failure("execution_failed", lang),
    };
    // Node expands '*' in fs grants. A literal wildcard in an externally
    // selected temp root cannot be converted to a broader grant.
    if script.to_string_lossy().contains('*') { return failure("path_access_not_allowed", lang); }
    let mut allow = OsString::from("--allow-fs-read="); allow.push(&script);
    let program = lease.directory.join("bin").join(binary_name());
    let mut result = run_process_until(&[program.into_os_string(), os("--permission"), allow, script.into_os_string()], cwd, timeout, Some("node"), Some(deadline));
    set_lang(&mut result, lang);
    result.insert("runtime".into(), serde_json::json!({"id":"node","version":VERSION,
        "manifest_sha256":manifest}));
    result
}

pub(crate) fn probe_directory(directory: &Path, data: &Path) -> Result<(), String> {
    if !platform_supported() { return Err("capability_pack_unavailable".into()); }
    if crate::speech::cancelled() { return Err("cancelled".into()); }
    let task = crate::task_workspace::Workspace::create(data, "node-probe").map_err(|_| "capability_install_failed")?;
    let mut can_retire = true;
    let result = task.scope(|| {
        let program = task.path().join(binary_name());
        crate::task_workspace::copy(&directory.join("bin").join(binary_name()), &program).map_err(|_| "capability_pack_invalid")?;
        let source = "console.log(JSON.stringify({version:process.version,sum:[6,7].reduce((a,b)=>a*b,1),unicode:'中文😀',write:process.permission.has('fs.write')}))";
        let result = run_process(&[program.into_os_string(), os("--permission"), os("--eval"), os(source)], Some(task.path()), 8, Some("node"));
        can_retire = get_str(&result, "error_code") != "execution_stop_pending";
        if crate::speech::cancelled() { return Err("cancelled".into()); }
        let expected = serde_json::json!({"version":format!("v{VERSION}"),"sum":42,"unicode":"中文😀","write":false});
        if !is_truthy(&result) || result.get("exit_code").and_then(Value::as_i64) != Some(0)
            || serde_json::from_str::<Value>(&get_str(&result, "stdout")).ok() != Some(expected)
            || !get_str(&result, "stderr").is_empty() {
            return Err("capability_pack_invalid".into());
        }
        Ok(())
    });
    // The private executable copy has no recovery payload once the owned
    // process/pipes have stopped. Incomplete termination still retains its
    // journal; never make Drop delete a potentially running worker's files.
    if can_retire { task.finish().map_err(|_| "capability_install_failed")?; }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn minimum_os_versions_fail_closed() {
        for value in ["13.0", "13.4.9", "12.9", "garbled", "14", "13.x"] { assert!(!version_at_least(value, 13, 5)); }
        for value in ["13.5", "13.5.1", "14.0", "26.1"] { assert!(version_at_least(value, 13, 5)); }
        assert!(!version_at_least("2.27", 2, 28)); assert!(version_at_least("2.28", 2, 28));
    }
    #[test]
    fn empty_profile_never_falls_back_to_a_host_node() {
        let data = tempfile::tempdir().unwrap(); let assets = tempfile::tempdir().unwrap();
        let paths = crate::paths::AppPaths::with_dirs(data.path(), &data.path().join("workspace"), assets.path());
        let result = execute(&paths, "console.log(6*7)", "javascript", 5, None);
        assert_eq!(get_str(&result, "error_code"), "capability_pack_unavailable");
        assert_eq!(get_str(&result, "stdout"), "");
        assert_eq!(std::fs::read_dir(data.path()).unwrap().count(), 0);
    }
    #[test]
    fn pre_cancelled_request_does_not_inspect_or_create_resources() {
        let data = tempfile::tempdir().unwrap(); let assets = tempfile::tempdir().unwrap();
        let paths = crate::paths::AppPaths::with_dirs(data.path(), data.path(), assets.path());
        let result = crate::speech::with_cancel(Arc::new(|| true), || execute(&paths, "console.log(42)", "js", 5, None));
        assert_eq!(get_str(&result, "error_code"), "execution_cancelled");
        assert_eq!(std::fs::read_dir(data.path()).unwrap().count(), 0);
    }
}
