//! Verified Windows package handoff. Portable replacement is transactional;
//! the helper runs from staging and waits for the resident reader to release its executable.
use std::{path::{Path, PathBuf}, io::{Read, Write}, time::{Duration, Instant}};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Plan { package: PathBuf, target: PathBuf, sha256: String, parent_pid: u32, data: PathBuf, assets: PathBuf, workspace: PathBuf }

#[cfg(windows)]
fn wait_for_process(pid: u32) -> bool {
    use std::ffi::c_void;
    #[link(name="kernel32")] extern "system" {
        fn OpenProcess(access:u32, inherit:i32, pid:u32) -> *mut c_void;
        fn WaitForSingleObject(handle:*mut c_void, ms:u32) -> u32;
        fn CloseHandle(handle:*mut c_void) -> i32;
    }
    unsafe {
        let handle = OpenProcess(0x00100000, 0, pid);
        if handle.is_null() { return true; }
        let result = WaitForSingleObject(handle, 120000); CloseHandle(handle); result == 0
    }
}

/// The restarted reader removes only its own exited helper, never arbitrary staging files.
pub fn cleanup_previous_helper() {
    #[cfg(windows)] {
        let path = std::env::var_os("READMD_UPDATE_HELPER_PATH").map(PathBuf::from);
        let pid = std::env::var("READMD_UPDATE_HELPER_PID").ok().and_then(|v| v.parse::<u32>().ok());
        std::env::remove_var("READMD_UPDATE_HELPER_PATH"); std::env::remove_var("READMD_UPDATE_HELPER_PID");
        let (Some(path), Some(pid)) = (path, pid) else { return };
        let expected = std::env::temp_dir().join("ReadMDUpdates").canonicalize().ok();
        if expected.is_none() || path.parent().and_then(|p| p.canonicalize().ok()) != expected { return; }
        let Some(id) = path.file_name().and_then(|n| n.to_str()).and_then(|n| n.strip_prefix("readmd-update-helper-")).and_then(|n| n.strip_suffix(".exe")) else { return };
        if uuid::Uuid::parse_str(id).is_err() { return; }
        let plan = path.with_file_name(format!("readmd-update-plan-{id}.json"));
        std::thread::spawn(move || { if wait_for_process(pid) { let _ = std::fs::remove_file(path); let _ = std::fs::remove_file(plan); } });
    }
}

pub fn take_result(data: &Path) -> Option<&'static str> {
    let report = data.join("update-result.json");
    let bytes = std::fs::read(&report).ok()?;
    let _ = std::fs::remove_file(report);
    if bytes.len() > 1024 { return None; }
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    (value["ok"] == false && value["error_code"] == "update_replace_failed").then_some("update_replace_failed")
}

pub fn package_kind(path: &Path) -> Option<&'static str> {
    let name = path.file_name()?.to_str()?.to_ascii_lowercase();
    if !name.ends_with(".exe") { return None; }
    if name.contains("setup") { Some("installer") } else if name.contains("portable") { Some("portable") } else { None }
}
fn executable(path: &Path) -> bool {
    let mut signature = [0; 2];
    std::fs::File::open(path).and_then(|mut file| file.read_exact(&mut signature)).is_ok() && signature == *b"MZ"
}

pub fn launch(package: &Path, sha256: &str, paths: &crate::paths::AppPaths) -> Result<bool, &'static str> {
    if !executable(package) { return Err("update_package_invalid"); }
    #[cfg(windows)] {
        match package_kind(package) {
            Some("installer") => {
                if crate::native_system::shell_open(&package.to_string_lossy()) { Ok(true) } else { Err("update_installer_launch_failed") }
            }
            Some("portable") => {
                use std::os::windows::process::CommandExt;
                let target = std::env::current_exe().map_err(|_| "update_apply_failed")?;
                let dir = package.parent().ok_or("update_apply_failed")?;
                let id = uuid::Uuid::new_v4().simple().to_string();
                let helper = dir.join(format!("readmd-update-helper-{id}.exe"));
                let plan_path = dir.join(format!("readmd-update-plan-{id}.json"));
                let plan = Plan { package: package.into(), target: target.clone(), sha256:sha256.into(), parent_pid:std::process::id(),
                    data:paths.data_dir.clone(), assets:paths.assets_dir.clone(), workspace:paths.workspace.clone() };
                std::fs::copy(target, &helper).map_err(|_| "update_apply_failed")?;
                let result = (|| {
                    let bytes = serde_json::to_vec(&plan).map_err(|_| "update_apply_failed")?;
                    let mut file = std::fs::OpenOptions::new().create_new(true).write(true).open(&plan_path).map_err(|_| "update_apply_failed")?;
                    file.write_all(&bytes).and_then(|_| file.sync_all()).map_err(|_| "update_apply_failed")?;
                    drop(file);
                    std::process::Command::new(&helper).arg("--install-update").arg(&plan_path).creation_flags(0x08000000)
                        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
                        .spawn().map_err(|_| "update_helper_launch_failed")?;
                    Ok(true)
                })();
                if result.is_err() { let _ = std::fs::remove_file(&helper); let _ = std::fs::remove_file(&plan_path); }
                result
            }
            _ => Err("update_package_manual_install"),
        }
    }
    #[cfg(not(windows))] { let _ = (sha256, paths); Err("update_package_manual_install") }
}

/// Copies beside the destination, syncs it, and restores the previous file on failure.
fn replace_portable(package: &Path, target: &Path, expected: &str) -> Result<(), String> {
    if crate::crypto::sha256_file(package).map_err(|e| e.to_string())? != expected { return Err("update_package_changed".into()); }
    let parent = target.parent().ok_or("update_target_invalid")?;
    let id = uuid::Uuid::new_v4().simple().to_string();
    let staged = parent.join(format!(".readmd-update-{id}.new"));
    let backup = parent.join(format!(".readmd-update-{id}.old"));
    let result = (|| {
        let mut output = std::fs::OpenOptions::new().create_new(true).write(true).open(&staged).map_err(|e| e.to_string())?;
        let mut input = std::fs::File::open(package).map_err(|e| e.to_string())?;
        std::io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
        output.flush().and_then(|_| output.sync_all()).map_err(|e| e.to_string())?;
        drop(output);
        if crate::crypto::sha256_file(&staged).map_err(|e| e.to_string())? != expected { return Err("update_package_changed".into()); }
        std::fs::rename(target, &backup).map_err(|e| e.to_string())?;
        if let Err(e) = std::fs::rename(&staged, target) { let _ = std::fs::rename(&backup, target); return Err(e.to_string()); }
        // A single rollback copy belongs to staging, never beside the user's documents.
        let rollback = package.parent().ok_or("update_target_invalid")?.join("ReadMD-previous.exe");
        if std::fs::copy(&backup, rollback).is_ok() { let _ = std::fs::remove_file(&backup); }
        Ok(())
    })();
    let _ = std::fs::remove_file(staged);
    result
}

pub fn run_helper(plan_path: &Path) -> Result<(), String> {
    let bytes = std::fs::read(plan_path).map_err(|e| e.to_string())?;
    let plan: Plan = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    #[cfg(windows)] if !wait_for_process(plan.parent_pid) { return Err("update_reader_still_running".into()); }
    let deadline = Instant::now() + Duration::from_secs(15);
    let result = loop {
        match replace_portable(&plan.package, &plan.target, &plan.sha256) {
            Ok(()) => break Ok(()),
            Err(error) if Instant::now() >= deadline || error == "update_package_changed" => break Err(error),
            Err(_) => std::thread::sleep(Duration::from_millis(250)),
        }
    };
    if result.is_ok() {
        let _ = std::fs::remove_file(&plan.package); let _ = std::fs::remove_file(plan_path);
    } else {
        let report = plan.data.join("update-result.json");
        let _ = std::fs::write(report, serde_json::json!({"ok":false,"error_code":"update_replace_failed"}).to_string());
    }
    // Failure also reopens the intact old reader; the original is never sacrificed.
    let helper = std::env::current_exe().map_err(|e| e.to_string())?;
    std::process::Command::new(&plan.target).args(["--data-dir"]).arg(&plan.data).arg("--assets").arg(&plan.assets).arg("--workspace").arg(&plan.workspace)
        .env("READMD_UPDATE_HELPER_PATH", helper).env("READMD_UPDATE_HELPER_PID", std::process::id().to_string())
        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().map_err(|e| e.to_string())?;
    result
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn replacement_failure_is_reported_once_without_echoing_diagnostics() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("update-result.json"), br#"{"ok":false,"error_code":"update_replace_failed","private_diagnostic":"ignored"}"#).unwrap();
        assert_eq!(take_result(dir.path()), Some("update_replace_failed"));
        assert_eq!(take_result(dir.path()), None);
    }
    #[test] fn portable_update_preserves_data_and_keeps_one_rollback() {
        let dir = tempfile::tempdir().unwrap(); let staged = dir.path().join("updates"); std::fs::create_dir(&staged).unwrap();
        let target = dir.path().join("readmd.exe"); let package = staged.join("ReadMD-portable.exe");
        std::fs::write(&target, b"old executable").unwrap(); std::fs::write(&package, b"new executable").unwrap();
        let user_data = dir.path().join("notes.md"); std::fs::write(&user_data,b"personal document").unwrap();
        replace_portable(&package, &target, &crate::crypto::sha256_hex(b"new executable")).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(),b"new executable");
        assert_eq!(std::fs::read(staged.join("ReadMD-previous.exe")).unwrap(),b"old executable");
        assert_eq!(std::fs::read(user_data).unwrap(),b"personal document");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),3);
    }
    #[test] fn changed_package_leaves_original_and_directory_intact() {
        let dir = tempfile::tempdir().unwrap(); let target=dir.path().join("readmd.exe");let package=dir.path().join("package.exe");
        std::fs::write(&target,b"original").unwrap();std::fs::write(&package,b"tampered").unwrap();
        assert_eq!(replace_portable(&package,&target,&crate::crypto::sha256_hex(b"expected")).unwrap_err(),"update_package_changed");
        assert_eq!(std::fs::read(target).unwrap(),b"original");assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),2);
    }
}
