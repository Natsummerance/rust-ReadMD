//! Stable C ABI for ArkTS/N-API. No new runtime, renderer or AI implementation.
//! The caller supplies app-private absolute directories; credentials remain in
//! the Rust store. CString ownership is explicit across the ABI boundary.
use std::ffi::{c_char, CStr, CString};
use std::path::Path;
use std::sync::{Arc, Mutex};
use serde_json::json;
static SERVER: Mutex<Option<crate::server::EmbeddedServer>> = Mutex::new(None);

/// # Safety
/// Both arguments must point to valid NUL-terminated UTF-8 strings for this
/// call's lifetime. Release the return value only via readmd_kernel_free.
#[no_mangle]
pub unsafe extern "C" fn readmd_kernel_start(data: *const c_char, assets: *const c_char) -> *mut c_char {
    let result = std::panic::catch_unwind(|| -> Result<serde_json::Value, &'static str> {
        if data.is_null() || assets.is_null() { return Err("invalid_directory"); }
        let data = unsafe { CStr::from_ptr(data) }.to_str().map_err(|_| "invalid_directory")?;
        let assets = unsafe { CStr::from_ptr(assets) }.to_str().map_err(|_| "invalid_directory")?;
        let data = Path::new(data); let assets = Path::new(assets);
        if !data.is_absolute() || !assets.is_absolute() || !assets.join("index.html").is_file() { return Err("invalid_directory"); }
        let mut server = SERVER.lock().map_err(|_| "kernel_unavailable")?;
        if server.is_some() { return Err("kernel_already_started"); }
        std::fs::create_dir_all(data).map_err(|_| "data_directory_unavailable")?;
        let workspace = data.join("documents");
        std::fs::create_dir_all(&workspace).map_err(|_| "data_directory_unavailable")?;
        let paths = crate::paths::AppPaths::with_dirs(data, &workspace, assets);
        let app = Arc::new(crate::App::bootstrap(paths).map_err(|_| "kernel_bootstrap_failed")?);
        let running = crate::server::start_embedded(app).map_err(|_| "kernel_start_failed")?;
        let value = json!({"ok":true,"url":format!("http://127.0.0.1:{}/",running.port)});
        *server = Some(running); Ok(value)
    });
    let value = match result { Ok(Ok(v)) => v, Ok(Err(code)) => json!({"ok":false,"error_code":code}), Err(_) => json!({"ok":false,"error_code":"kernel_panicked"}) };
    CString::new(value.to_string()).unwrap().into_raw()
}
#[no_mangle]
pub extern "C" fn readmd_kernel_stop() {
    if let Ok(mut guard) = SERVER.lock() { guard.take(); }
}
/// # Safety
/// pointer is null, or an unfreed value returned by readmd_kernel_start.
#[no_mangle]
pub unsafe extern "C" fn readmd_kernel_free(pointer: *mut c_char) {
    if !pointer.is_null() { drop(unsafe { CString::from_raw(pointer) }); }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn embedded_rust_round_trip_and_lifecycle() {
        let root = tempfile::tempdir().unwrap();
        let assets = root.path().join("assets"); std::fs::create_dir(&assets).unwrap();
        std::fs::write(assets.join("index.html"), "<html><head></head><body>ReadMD</body></html>").unwrap();
        let data = CString::new(root.path().join("data").to_str().unwrap()).unwrap();
        let assets = CString::new(assets.to_str().unwrap()).unwrap();
        let ptr = unsafe { readmd_kernel_start(data.as_ptr(),assets.as_ptr()) };
        let result: serde_json::Value = serde_json::from_str(unsafe {CStr::from_ptr(ptr)}.to_str().unwrap()).unwrap();
        unsafe {readmd_kernel_free(ptr)};
        assert_eq!(result["ok"],true);
        let url=result["url"].as_str().unwrap();
        let response=ureq::get(url).call().unwrap(); assert_eq!(response.status(),200);
        let second=unsafe {readmd_kernel_start(data.as_ptr(),assets.as_ptr())};
        assert!(unsafe {CStr::from_ptr(second)}.to_str().unwrap().contains("kernel_already_started"));
        unsafe {readmd_kernel_free(second)};
        readmd_kernel_stop();
        assert!(ureq::get(url).call().is_err());
        let invalid=unsafe {readmd_kernel_start(std::ptr::null(),assets.as_ptr())};
        assert!(unsafe {CStr::from_ptr(invalid)}.to_str().unwrap().contains("invalid_directory"));
        unsafe {readmd_kernel_free(invalid)};
    }
}
