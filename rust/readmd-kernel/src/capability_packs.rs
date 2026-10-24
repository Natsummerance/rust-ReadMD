//! Offline capability resources with a lock shipped alongside application assets.
//! JSON v1 and streamed v2 share validation, leases, probing and atomic commit.
use crate::{
    server::{ok_json, Request, Response},
    ApiError, ApiResult, App,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    io::{Cursor, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};
mod transaction;
mod replacement;
mod staging;
mod legacy;
mod history;
mod paths;
mod space;
static INSTALL: Mutex<()> = Mutex::new(());
// Resource scopes enter under the same lock as install/repair/uninstall, then
// publish their busy lease before releasing it. A file-path lease must not
// race with a completed validation followed by an uninstall.
pub(crate) fn resource_guard() -> std::sync::MutexGuard<'static, ()> {
    INSTALL.lock().unwrap_or_else(|e| e.into_inner())
}
fn resource_guard_checked(check: &dyn Fn() -> ApiResult<()>) -> ApiResult<std::sync::MutexGuard<'static, ()>> {
    loop {
        check()?;
        if crate::speech::cancelled() { return Err(ApiError::new(422,"cancelled")); }
        match INSTALL.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(std::sync::TryLockError::Poisoned(error)) => return Ok(error.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => std::thread::sleep(std::time::Duration::from_millis(20)),
        }
    }
}
const ID: &str = "pdfium";
const VERSION: &str = "7881";
#[derive(Clone, Copy)]
struct Spec {
    id: &'static str,
    version: &'static str,
    platform: &'static str,
}
pub(crate) fn platform() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "windows-x64",
        ("windows", "aarch64") => "windows-arm64",
        ("macos", "aarch64") => "macos-arm64",
        ("macos", "x86_64") => "macos-x64",
        ("linux", "aarch64") => "linux-arm64",
        ("linux", "x86_64") => "linux-x64",
        _ => "unsupported",
    }
}
fn spec(id: &str) -> ApiResult<Spec> {
    match id {
        "pdfium" if platform() != "unsupported" => Ok(Spec {
            id: ID,
            version: VERSION,
            platform: platform(),
        }),
        "typst" if platform() != "unsupported" => Ok(Spec {
            id: "typst",
            version: "0.15.1",
            platform: platform(),
        }),
        "ocr" if platform() != "unsupported" => Ok(Spec {
            id: "ocr",
            version: "5.5.3-r1",
            platform: platform(),
        }),
        "fonts" => Ok(Spec {
            id: "fonts",
            version: crate::publication_fonts::VERSION,
            platform: "universal",
        }),
        "latex" if cfg!(windows) && platform() == "windows-x64" => Ok(Spec {
            id: "latex",
            version: crate::latex_compile::VERSION,
            platform: "windows-x64",
        }),
        _ => Err(error()),
    }
}
fn error() -> ApiError {
    ApiError::new(422, "capability_pack_invalid")
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn root(app: &App) -> PathBuf {
    crate::capability_store::root(&app.paths.data_dir).join(ID)
}
fn root_for(data: &Path, spec: Spec) -> PathBuf {
    crate::capability_store::root(data).join(spec.id)
}
fn managed_parent(data: &Path, selected: Spec, create: bool) -> ApiResult<PathBuf> {
    let mut path = data.canonicalize().map_err(|_| error())?;
    for part in ["capabilities", crate::capability_store::PROTOCOL, selected.id] {
        path.push(part);
        if create {
            match fs::create_dir(&path) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(_) => return Err(error()),
            }
        }
        real_directory(&path)?;
    }
    Ok(path)
}
pub fn directory(app: &App) -> PathBuf {
    root(app).join(VERSION)
}
fn lock(app: &App) -> ApiResult<Value> {
    lock_for(&app.paths.assets_dir, spec(ID)?)
}
fn lock_for(assets: &Path, spec: Spec) -> ApiResult<Value> {
    let file = assets.join(format!(
        "capabilities/{}-{}-{}.lock.json",
        spec.id, spec.version, spec.platform
    ));
    if fs::metadata(&file).map_err(|_| error())?.len()
        > crate::capability_archive::MAX_MANIFEST as u64
    {
        return Err(error());
    }
    let value: Value =
        serde_json::from_slice(&fs::read(file).map_err(|_| error())?).map_err(|_| error())?;
    if !matches!(value["schema"].as_u64(), Some(1 | 2))
        || value["id"] != spec.id
        || value["version"] != spec.version
        || value["platform"] != spec.platform
    {
        return Err(error());
    }
    checked_files(&value)?;
    if value["schema"] == 1
        && fs::metadata(assets.join(format!(
            "capabilities/{}-{}-{}.lock.json",
            spec.id, spec.version, spec.platform
        )))
        .map_err(|_| error())?
        .len()
            > 65536
    {
        return Err(error());
    }
    Ok(value)
}
pub(crate) fn resource_lock(assets: &Path, id: &str) -> ApiResult<Value> {
    lock_for(assets, spec(id)?)
}
/// Identity of a locked runtime for an already-generated artifact. This is
/// deliberately not a readiness check: execution still takes a validated
/// lease, and missing/uninstalled runtimes must invalidate cached previews.
pub(crate) fn runtime_revision(data: &Path, assets: &Path, id: &str) -> ApiResult<String> {
    runtime_revision_checked(data,assets,id,&||Ok(()))
}
pub(crate) fn runtime_revision_checked(data:&Path,assets:&Path,id:&str,check:&(dyn Fn()->ApiResult<()>+Sync))->ApiResult<String>{
    check()?;
    let selected = spec(id)?;
    let expected = lock_for(assets, selected)?;
    prepare_legacy_access(data, assets, selected, &expected, check)?;
    check()?;
    let _process=crate::capability_process::try_acquire(data,id,crate::capability_process::Mode::Shared)?
        .ok_or_else(||ApiError::new(409,busy_error(id)))?;
    if transactions_blocking(&root_for(data, selected), assets, selected, &expected)? { return Err(ApiError::new(409,"capability_install_busy")); }
    let parent = managed_parent(data, selected, false)?;
    let directory = parent.join(selected.version);
    real_directory(&directory)?;
    // Uninstall may retain unknown user files and hence the version directory.
    // Its mere existence must not keep the old preview identity alive.
    let first = checked_files(&expected)?.first().ok_or_else(error)?;
    let file = parent_for(&directory, first["path"].as_str().ok_or_else(error)?)?;
    let metadata = fs::symlink_metadata(&file).map_err(|_| error())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || Some(metadata.len()) != first["size"].as_u64() {
        return Err(error());
    }
    check()?;
    Ok(hash(&serde_json::to_vec(&expected).map_err(|_| error())?))
}
fn checked_files(lock: &Value) -> ApiResult<&Vec<Value>> {
    if lock["schema"] == 2 {
        crate::capability_archive::validate_manifest(lock).map_err(archive_error)?;
        return lock["files"].as_array().ok_or_else(error);
    }
    if lock["schema"] != 1 {
        return Err(error());
    }
    let files = lock["files"]
        .as_array()
        .filter(|f| (3..=64).contains(&f.len()))
        .ok_or_else(error)?;
    let mut names = HashSet::new();
    for item in files {
        let name = item["path"].as_str().ok_or_else(error)?;
        let digest = item["sha256"].as_str().unwrap_or("");
        if !crate::capability_archive::safe_name(name)
            || !names.insert(name.to_ascii_lowercase())
            || item["size"].as_u64().is_none_or(|n| n > 128 * 1024 * 1024)
            || digest.len() != 64
            || !digest
                .bytes()
                .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
        {
            return Err(error());
        }
    }
    for name in &names {
        for (i, _) in name.match_indices('/') {
            if names.contains(&name[..i]) {
                return Err(error());
            }
        }
    }
    Ok(files)
}
fn archive_error(e: crate::capability_archive::Error) -> ApiError {
    match e {
        crate::capability_archive::Error::Cancelled => ApiError::new(422, "cancelled"),
        crate::capability_archive::Error::Io(_) => ApiError::new(422, "capability_install_failed"),
        _ => error(),
    }
}
fn real_directory(dir: &Path) -> ApiResult<()> {
    let m = fs::symlink_metadata(dir).map_err(|_| error())?;
    if !m.is_dir() || m.file_type().is_symlink() {
        return Err(error());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x400 != 0 {
            return Err(error());
        }
    }
    Ok(())
}
fn parent_for(dir: &Path, name: &str) -> ApiResult<PathBuf> {
    if !crate::capability_archive::safe_name(name) {
        return Err(error());
    }
    real_directory(dir)?;
    let mut parent = dir.to_path_buf();
    let parts = name.split('/').collect::<Vec<_>>();
    for part in &parts[..parts.len() - 1] {
        parent.push(part);
        real_directory(&parent)?;
    }
    if !parent
        .canonicalize()
        .map_err(|_| error())?
        .starts_with(dir.canonicalize().map_err(|_| error())?)
    {
        return Err(error());
    }
    Ok(parent.join(parts.last().ok_or_else(error)?))
}
fn resource_file(path:&Path)->ApiResult<(fs::File,u64)> {
    #[cfg(windows)]
    let file={
        use std::os::windows::fs::OpenOptionsExt;
        use windows::Win32::Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT,FILE_SHARE_READ};
        // Inspect and hash the same opened object. Do not follow a final
        // reparse point or allow concurrent writes/renames while hashing it.
        fs::OpenOptions::new().read(true).custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .share_mode(FILE_SHARE_READ.0).open(path).map_err(|e|if e.raw_os_error()==Some(32) {
                ApiError::new(409,"capability_install_busy")
            } else { error() })?
    };
    #[cfg(not(windows))]
    let file={
        let metadata=fs::symlink_metadata(path).map_err(|_|error())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() { return Err(error()); }
        fs::File::open(path).map_err(|_|error())?
    };
    #[cfg(windows)] let size={
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::{Foundation::HANDLE,Storage::FileSystem::{
            GetFileInformationByHandle,BY_HANDLE_FILE_INFORMATION,FILE_ATTRIBUTE_DIRECTORY,FILE_ATTRIBUTE_REPARSE_POINT,
        }};
        // One query contains exactly the attributes and length required here.
        // Do not fetch unrelated timestamp/tag records for every tiny file.
        let mut info=BY_HANDLE_FILE_INFORMATION::default();
        unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()),&mut info).map_err(|_|error())?; }
        if info.dwFileAttributes&(FILE_ATTRIBUTE_DIRECTORY.0|FILE_ATTRIBUTE_REPARSE_POINT.0)!=0 { return Err(error()); }
        ((info.nFileSizeHigh as u64)<<32)|info.nFileSizeLow as u64
    };
    #[cfg(not(windows))] let size={
        let metadata=file.metadata().map_err(|_|error())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() { return Err(error()); }
        metadata.len()
    };
    Ok((file,size))
}
#[cfg(test)]
fn hash_file(path: &Path, check: &dyn Fn() -> ApiResult<()>) -> ApiResult<String> {
    let (reader,size)=resource_file(path)?;
    hash_reader(reader,size,check)
}
fn hash_reader(reader:impl Read, size:u64, check:&dyn Fn()->ApiResult<()>) -> ApiResult<String> {
    // On Windows resource_file obtained the size from this same handle and
    // denies writes/deletes for its lifetime. Take stops at that known length
    // without issuing an extra OS read for EOF on every small runtime file.
    // Other platforms must still read EOF: their ordinary open does not deny
    // concurrent growth. Both paths require exactly the expected byte count.
    #[cfg(windows)] let mut reader=reader.take(size);
    #[cfg(not(windows))] let mut reader=reader;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut bytes=0u64;
    loop {
        check()?;
        if crate::speech::cancelled() {
            return Err(ApiError::new(422, "cancelled"));
        }
        let n = reader.read(&mut buffer).map_err(|_| error())?;
        if n == 0 {
            break;
        }
        bytes=bytes.checked_add(n as u64).filter(|n| *n<=size).ok_or_else(error)?;
        digest.update(&buffer[..n]);
    }
    if bytes!=size { return Err(error()); }
    Ok(format!("{:x}", digest.finalize()))
}
fn validate(dir: &Path, lock: &Value) -> ApiResult<()> {
    validate_checked(dir, lock, &|| Ok(()))
}
fn validation_check(check:&dyn Fn()->ApiResult<()>) -> ApiResult<()> {
    check()?;
    if crate::speech::cancelled() { return Err(ApiError::new(422,"cancelled")); }
    Ok(())
}
fn validation_lanes(files:usize) -> usize {
    if files<=256 { 1 } else { std::thread::available_parallelism().map_or(1,|available|available.get().min(8)) }
}
fn validation_directories(dir:&Path,files:&[Value],check:&dyn Fn()->ApiResult<()>) -> ApiResult<BTreeSet<PathBuf>> {
    let mut parents=BTreeSet::from([dir.to_path_buf()]);
    for item in files {
        validation_check(check)?;
        let name=item["path"].as_str().ok_or_else(error)?;
        if !crate::capability_archive::safe_name(name) { return Err(error()); }
        if let Some((ancestors,_))=name.rsplit_once('/') {
            let mut parent=dir.to_path_buf();
            for part in ancestors.split('/') {
                validation_check(check)?;
                parent.push(part);parents.insert(parent.clone());
            }
        }
    }
    Ok(parents)
}
fn validate_directory_partition(parents:&BTreeSet<PathBuf>,canonical:&Path,lane:usize,lanes:usize,check:&dyn Fn()->ApiResult<()>) -> ApiResult<()> {
    for parent in parents.iter().skip(lane).step_by(lanes) {
        validation_check(check)?;
        real_directory(parent)?;
        if !parent.canonicalize().map_err(|_|error())?.starts_with(canonical) { return Err(error()); }
    }
    Ok(())
}
fn validation_workers(lanes:usize,check:&(dyn Fn()->ApiResult<()>+Sync),work:impl Fn(usize,&dyn Fn()->ApiResult<()>)->ApiResult<()>+Sync) -> ApiResult<()> {
    let failed=std::sync::atomic::AtomicBool::new(false);
    let failure=Mutex::new(None);
    let cancellation=crate::speech::cancellation_check();
    std::thread::scope(|scope| {
        for lane in 0..lanes {
            let failed=&failed;let failure=&failure;let cancellation=cancellation.clone();let work=&work;
            scope.spawn(move || crate::speech::with_cancel(cancellation, || {
                let result=work(lane,&|| {
                    if failed.load(std::sync::atomic::Ordering::Acquire) { return Err(ApiError::new(422,"cancelled")); }
                    check()
                });
                if let Err(error)=result {
                    let mut first=failure.lock().unwrap_or_else(|e|e.into_inner());
                    // Publish the original failure before siblings observe
                    // cancellation; their cancelled error cannot replace it.
                    if first.is_none() { *first=Some(error); }
                    failed.store(true,std::sync::atomic::Ordering::Release);
                }
            }));
        }
    });
    if let Some(error)=failure.into_inner().unwrap_or_else(|e|e.into_inner()) { return Err(error); }
    Ok(())
}
fn validate_directories(parents:&BTreeSet<PathBuf>,canonical:&Path,lanes:usize,check:&(dyn Fn()->ApiResult<()>+Sync)) -> ApiResult<()> {
    let lanes=lanes.min(parents.len()).max(1);
    if lanes==1 { validate_directory_partition(parents,canonical,0,1,check) }
    else { validation_workers(lanes,check,|lane,check|validate_directory_partition(parents,canonical,lane,lanes,check)) }
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum ValidationPhase { BeforeDirectories, Files, FilesJoined, AfterDirectories }
fn validate_checked(dir: &Path, lock: &Value, check: &(dyn Fn() -> ApiResult<()> + Sync)) -> ApiResult<()> {
    validate_checked_phases(dir,lock,check,&|_|Ok(()))
}
// Private phase boundaries let tests observe a completed join precisely,
// without deriving a phase from callback counts or mutating production state.
fn validate_checked_phases(dir:&Path,lock:&Value,check:&(dyn Fn()->ApiResult<()>+Sync),phase:&dyn Fn(ValidationPhase)->ApiResult<()>) -> ApiResult<()> {
    let started=std::time::Instant::now();
    log::debug!(target:"readmd::capability", "validation begins");
    validation_check(check)?;
    real_directory(dir)?;
    let canonical = dir.canonicalize().map_err(|_| error())?;
    let files=checked_files(lock)?;
    // One validation-local set covers the root and every intermediate
    // directory. No trusted metadata survives this validation call. Distinct
    // parents occur once per phase, with bounded parallel I/O in each phase.
    let parents=validation_directories(dir,files,check)?;
    let lanes=validation_lanes(files.len());
    phase(ValidationPhase::BeforeDirectories)?;
    validate_directories(&parents,&canonical,lanes,check)?;
    // All pre-check workers joined before any file opens. Each file still
    // provides its exact size/SHA through the same write/delete-denying,
    // OPEN_REPARSE_POINT handle. Even one-core large runtimes use a scoped
    // hash worker so the private phase contract is independent of core count.
    phase(ValidationPhase::Files)?;
    if files.len()<=256 { validate_files_partition(dir,files,0,1,check)?; }
    else { validation_workers(lanes,check,|lane,check|validate_files_partition(dir,files,lane,lanes,check))?; }
    phase(ValidationPhase::FilesJoined)?;
    // Hash workers have all joined before post-check workers start; no stage
    // overlaps another or multiplies the bounded I/O lane count.
    phase(ValidationPhase::AfterDirectories)?;
    validate_directories(&parents,&canonical,lanes,check)?;
    validation_check(check)?;
    if dir.canonicalize().map_err(|_|error())?!=canonical { return Err(error()); }
    log::debug!(target:"readmd::capability", "validation finished elapsed_ms={}",started.elapsed().as_millis());
    Ok(())
}
// Legacy migration and transaction recovery borrow a non-Sync cancellation
// callback and need a standalone serial validation entry. Keep their original
// six-argument contract, including all ancestor checks and the final root
// binding. The parallel hot path above uses the files-only helper directly.
fn validate_partition(dir:&Path,canonical:&Path,files:&[Value],lane:usize,lanes:usize,check:&dyn Fn()->ApiResult<()>) -> ApiResult<()> {
    validation_check(check)?;
    if lanes==0||lane>=lanes { return Err(error()); }
    let parents=validation_directories(dir,files,check)?;
    validate_directory_partition(&parents,canonical,0,1,check)?;
    validate_files_partition(dir,files,lane,lanes,check)?;
    validate_directory_partition(&parents,canonical,0,1,check)?;
    validation_check(check)?;
    if dir.canonicalize().map_err(|_|error())?!=canonical { return Err(error()); }
    Ok(())
}
fn validate_files_partition(dir:&Path,files:&[Value],lane:usize,lanes:usize,check:&dyn Fn()->ApiResult<()>) -> ApiResult<()> {
    let started=std::time::Instant::now();
    for (index,item) in files.iter().skip(lane).step_by(lanes).enumerate() {
        if index%4096==0 { log::debug!(target:"readmd::capability", "validation lane={} files={} elapsed_ms={}",lane,index,started.elapsed().as_millis()); }
        validation_check(check)?;
        let name = item["path"].as_str().ok_or_else(error)?;
        // safe_name and every ancestor were checked while collecting the
        // shared directory set. The immutable manifest is borrowed throughout.
        let file = dir.join(name);
        let (reader,actual_size)=resource_file(&file)?;
        let size = item["size"].as_u64().ok_or_else(error)?;
        if actual_size != size || item["sha256"].as_str() != Some(hash_reader(reader, size, check)?.as_str()) {
            return Err(error());
        }
    }
    log::debug!(target:"readmd::capability", "validation lane={} finished elapsed_ms={}",lane,started.elapsed().as_millis());
    Ok(())
}
fn leases() -> &'static Mutex<BTreeMap<PathBuf, usize>> {
    static LEASES: OnceLock<Mutex<BTreeMap<PathBuf, usize>>> = OnceLock::new();
    LEASES.get_or_init(|| Mutex::new(BTreeMap::new()))
}
fn lease_key(data: &Path, selected: Spec) -> PathBuf {
    let base = dunce::canonicalize(data).unwrap_or_else(|_| data.to_path_buf());
    let path = root_for(&base, selected).join(selected.version);
    #[cfg(windows)]
    let path = PathBuf::from(path.to_string_lossy().to_lowercase());
    dunce::simplified(&path).to_path_buf()
}
pub(crate) struct ResourceLease {
    pub directory: PathBuf,
    key: PathBuf,
    _process:crate::capability_process::Guard,
}
impl Drop for ResourceLease {
    fn drop(&mut self) {
        let mut all = leases().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(n) = all.get_mut(&self.key) {
            *n -= 1;
            if *n == 0 {
                all.remove(&self.key);
            }
        }
    }
}
pub(crate) fn lease(data: &Path, assets: &Path, id: &str) -> ApiResult<ResourceLease> {
    lease_checked(data, assets, id, &|| Ok(()))
}
pub(crate) fn lease_checked(data: &Path, assets: &Path, id: &str, check: &(dyn Fn() -> ApiResult<()> + Sync)) -> ApiResult<ResourceLease> {
    lease_reserved(data, assets, id, check, || Ok(())).map(|(lease, ())| lease)
}
pub(crate) fn lease_reserved<T>(data: &Path, assets: &Path, id: &str, check: &(dyn Fn() -> ApiResult<()> + Sync), reserve: impl FnOnce() -> ApiResult<T>) -> ApiResult<(ResourceLease, T)> {
    check()?;
    let selected = spec(id)?;
    // Reject a missing/invalid application lock before creating coordination
    // files or taking a worker slot. This is immutable configuration, not the
    // installed runtime; runtime paths are still checked under the lease.
    let expected = lock_for(assets, selected)?;
    let (_guard,process)=loop {
        let guard=resource_guard_checked(check)?;
        check()?;
        if crate::speech::cancelled() { return Err(ApiError::new(422,"cancelled")); }
        let Some(mut process)=crate::capability_process::try_acquire(data,id,crate::capability_process::Mode::Shared)? else {
            drop(guard);
            std::thread::sleep(std::time::Duration::from_millis(20));
            continue;
        };
        let migration=legacy::needed(data,selected)?;
        if transactions_pending(&root_for(data,selected))? || migration {
            let blocking=transactions_blocking(&root_for(data,selected),assets,selected,&expected)? || migration;
            drop(process);
            if let Some(exclusive)=crate::capability_process::try_acquire(data,id,crate::capability_process::Mode::Exclusive)? {
                recover_locked(data,assets,selected,&expected,check)?;
                legacy::migrate(data,selected,&expected,check)?;
                drop(exclusive);
            } else if blocking {
                drop(guard);
                std::thread::sleep(std::time::Duration::from_millis(20));
                continue;
            }
            let Some(shared)=crate::capability_process::try_acquire(data,id,crate::capability_process::Mode::Shared)? else {
                drop(guard);
                std::thread::sleep(std::time::Duration::from_millis(20));
                continue;
            };
            // Recheck after upgrading: another process could have acquired
            // the exclusive lock between guards and left a new transaction.
            if transactions_blocking(&root_for(data,selected),assets,selected,&expected)? {
                drop(shared);
                drop(guard);
                std::thread::sleep(std::time::Duration::from_millis(20));
                continue;
            }
            process=shared;
        }
        break (guard,process);
    };
    let reservation = reserve()?;
    managed_parent(data, selected, false)?;
    let directory = root_for(data, selected).join(selected.version);
    let key = lease_key(data, selected);
    *leases()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry(key.clone())
        .or_default() += 1;
    let lease = ResourceLease { directory, key, _process:process };
    // Pin before releasing the install guard, including during validation.
    // Uninstall/repair see this temporary lease and must refuse. Hashing may
    // then run concurrently instead of serializing entire large runtimes.
    // Failure drops both the lease and the caller's process reservation.
    drop(_guard);
    validate_checked(&lease.directory, &expected, check)?;
    Ok((lease, reservation))
}
fn busy_error(id:&str)->&'static str {
    match id { ID=>"pdf_busy","typst"=>"typst_busy","latex"=>"latex_busy",_=>"capability_install_busy" }
}
fn local_busy_code(app: &App, id: &str) -> Option<&'static str> {
    let selected = spec(id).ok()?;
    let leased = leases()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&lease_key(&app.paths.data_dir, selected))
        .copied()
        .unwrap_or(0)
        > 0;
    let active = match id {
        ID => crate::pdf_workspace::busy(app),
        "ocr" => crate::ocr_portable::busy(),
        "typst" => crate::typst_compile::busy(),
        "fonts" => crate::publication_fonts::busy(),
        "latex" => crate::latex_compile::busy(),
        _ => false,
    };
    if leased || active {
        Some(busy_error(id))
    } else {
        None
    }
}
pub(crate) fn busy_code(app:&App,id:&str)->ApiResult<Option<&'static str>> {
    spec(id)?;
    if let Some(code)=local_busy_code(app,id) { return Ok(Some(code)); }
    Ok(crate::capability_process::try_acquire(&app.paths.data_dir,id,crate::capability_process::Mode::Exclusive)?
        .is_none().then_some(busy_error(id)))
}
pub fn installed(app: &App) -> bool {
    let Ok(selected)=spec(ID) else { return false; };
    let Ok(expected)=lock(app) else { return false; };
    if prepare_legacy_access(&app.paths.data_dir,&app.paths.assets_dir,selected,&expected,&||Ok(())).is_err() { return false; }
    let Ok(Some(_process))=crate::capability_process::try_acquire(&app.paths.data_dir,ID,crate::capability_process::Mode::Shared) else { return false; };
    lock(app)
        .and_then(|lock| {
            if transactions_blocking(&root(app), &app.paths.assets_dir, selected, &lock)? { return Err(error()); }
            validate(&directory(app), &lock)
        })
        .is_ok()
}
pub(crate) fn typst_engine(data: &Path, assets: &Path) -> Result<(PathBuf, String), String> {
    typst_engine_checked(data,assets,&||Ok(())).map_err(|_|"typst_engine_unavailable".into())
}
pub(crate) fn typst_engine_checked(data:&Path,assets:&Path,check:&(dyn Fn()->ApiResult<()>+Sync))->ApiResult<(PathBuf,String)>{
    check()?;
    let s=spec("typst")?;
    let expected=lock_for(assets,s)?;
    let lease=lease_checked(data,assets,"typst",check)?;
    let name = if cfg!(windows) {
        "bin/typst.exe"
    } else {
        "bin/typst"
    };
    let fingerprint = expected["files"]
        .as_array()
        .and_then(|files| files.iter().find(|f| f["path"] == name))
        .and_then(|file| file["sha256"].as_str())
        .ok_or_else(error)?;
    check()?;
    Ok((lease.directory.join(name),fingerprint.into()))
}
pub(crate) fn ocr_engine(data: &Path, assets: &Path) -> Result<(PathBuf, PathBuf), String> {
    let selected = spec("ocr").map_err(|_| "ocr_no_engine")?;
    let lock = lock_for(assets, selected).map_err(|_| "ocr_no_engine")?;
    prepare_legacy_access(data,assets,selected,&lock,&||Ok(())).map_err(|_|"ocr_no_engine")?;
    let _process=crate::capability_process::try_acquire(data,"ocr",crate::capability_process::Mode::Shared)
        .ok().flatten().ok_or("ocr_no_engine")?;
    let dir = root_for(data, selected).join(selected.version);
    validate(&dir, &lock).map_err(|_| "ocr_no_engine")?;
    Ok((
        dir.join(if cfg!(windows) {
            "bin/tesseract.exe"
        } else {
            "bin/tesseract"
        }),
        dir.join("tessdata"),
    ))
}
pub(crate) fn font_resources(
    data: &Path,
    assets: &Path,
) -> Result<crate::publication_fonts::Resources, String> {
    leased_font_resources(data,assets).map(|(resources,_)|resources)
}
pub(crate) fn leased_font_resources(data:&Path,assets:&Path)->Result<(crate::publication_fonts::Resources,ResourceLease),String> {
    leased_font_resources_checked(data,assets,&||Ok(())).map_err(|_|"publication_fonts_unavailable".to_string())
}
pub(crate) fn leased_font_resources_checked(data:&Path,assets:&Path,check:&(dyn Fn()->ApiResult<()>+Sync))->ApiResult<(crate::publication_fonts::Resources,ResourceLease)> {
    let lease=lease_checked(data,assets,"fonts",check)?;
    let selected = spec("fonts")?;
    let expected = lock_for(assets, selected)?;
    let dir = &lease.directory;
    Ok((crate::publication_fonts::Resources {
        sans: dir.join("fonts/ReadMDSansSC-Regular.ttf"),
        serif: dir.join("fonts/ReadMDSerifSC-Regular.ttf"),
        fingerprint: hash(
            &serde_json::to_vec(&expected).map_err(|_|error())?,
        ),
    },lease))
}
pub(crate) fn pdfium_resources(data: &Path, assets: &Path) -> Result<(PathBuf, PathBuf), String> {
    let selected = spec(ID).map_err(|_| "pdf_engine_unavailable")?;
    let lock = lock_for(assets, selected).map_err(|_| "pdf_engine_unavailable")?;
    prepare_legacy_access(data,assets,selected,&lock,&||Ok(())).map_err(|_|"pdf_engine_unavailable")?;
    let _process=crate::capability_process::try_acquire(data,ID,crate::capability_process::Mode::Shared)
        .ok().flatten().ok_or("pdf_engine_unavailable")?;
    let dir = root_for(data, selected).join(selected.version);
    validate(&dir, &lock).map_err(|_| "pdf_engine_unavailable")?;
    let (worker, library) = pdfium_names(std::env::consts::OS).ok_or("pdf_engine_unavailable")?;
    Ok((dir.join("bin").join(worker), dir.join("bin").join(library)))
}
pub(crate) fn pdfium_names(os: &str) -> Option<(&'static str, &'static str)> {
    match os {
        "windows" => Some(("readmd-pdf-worker.exe", "pdfium.dll")),
        "macos" => Some(("readmd-pdf-worker", "libpdfium.dylib")),
        "linux" => Some(("readmd-pdf-worker", "libpdfium.so")),
        _ => None,
    }
}
fn probe(id: &str, directory: &Path, data: &Path) -> Result<(), String> {
    match id {
        ID => crate::pdf_workspace::probe_directory(directory)
            .map_err(|_| "capability_pack_invalid".into()),
        "ocr" => crate::ocr_portable::probe_directory(directory,data),
        "fonts" => crate::publication_fonts::probe_directory(directory),
        "latex" => crate::latex_compile::probe_directory(directory,data),
        _ => crate::typst_compile::probe_directory(directory,data),
    }
}
pub(crate) fn ready(app: &Arc<App>, id: &str) -> bool {
    let _guard = resource_guard();
    let Ok(Some(_process))=crate::capability_process::try_acquire(&app.paths.data_dir,id,crate::capability_process::Mode::Exclusive) else { return false; };
    if local_busy_code(app, id).is_some() {
        return false;
    }
    let Ok(selected) = spec(id) else {
        return false;
    };
    let Ok(lock) = lock_for(&app.paths.assets_dir, selected) else {
        return false;
    };
    if recover_locked(&app.paths.data_dir,&app.paths.assets_dir,selected,&lock,&||Ok(())).is_err() { return false; }
    if legacy::migrate(&app.paths.data_dir,selected,&lock,&||Ok(())).is_err() { return false; }
    let target = root_for(&app.paths.data_dir, selected).join(selected.version);
    if validate(&target, &lock).is_err() {
        return false;
    }
    probe(id, &target, &app.paths.data_dir).is_ok()
}
struct Removal {
    unknown: bool,
    cleanup_pending: bool,
}
#[cfg(test)]
fn remove_known(dir: &Path, lock: &Value) -> ApiResult<Removal> {
    transaction::remove(dir, lock)
}
fn transactions_pending(parent:&Path) -> ApiResult<bool> {
    Ok(transaction::pending(parent)? || replacement::pending(parent)? || staging::pending(parent)?)
}
fn transactions_blocking(parent:&Path, assets:&Path, selected:Spec, lock:&Value) -> ApiResult<bool> {
    if !transaction::pending(parent)? && !replacement::pending(parent)? { return Ok(false); }
    let trusted=history::Trusted::load(assets,selected,lock)?;
    Ok(transaction::blocking_trusted(parent,&trusted)? || replacement::blocking_trusted(parent,&trusted)?)
}
// The caller must hold INSTALL and this capability's exclusive OS guard.
fn recover_locked(data:&Path, assets:&Path, selected:Spec, lock:&Value, check:&dyn Fn()->ApiResult<()>) -> ApiResult<()> {
    if transactions_pending(&root_for(data,selected))? {
        let parent=managed_parent(data,selected,false)?;
        let trusted=history::Trusted::load(assets,selected,lock)?;
        transaction::recover_trusted(&parent,&trusted,check)?;
        replacement::recover_trusted(&parent,&trusted,check)?;
        staging::recover_trusted(&parent,&trusted,check)?;
    }
    Ok(())
}
// Metadata/readiness paths use the same first-use migration as actual workers.
// Existing private resources keep their fast shared-read path.
fn prepare_legacy_access(data:&Path, assets:&Path, selected:Spec, lock:&Value, check:&dyn Fn()->ApiResult<()>) -> ApiResult<()> {
    if !legacy::needed(data,selected)? { return Ok(()); }
    let _guard=resource_guard_checked(check)?;
    let _process=crate::capability_process::try_acquire(data,selected.id,crate::capability_process::Mode::Exclusive)?
        .ok_or_else(||ApiError::new(409,busy_error(selected.id)))?;
    recover_locked(data,assets,selected,lock,check)?;
    legacy::migrate(data,selected,lock,check)
}
pub(crate) struct InstallHooks<'a> {
    pub cancelled: &'a dyn Fn() -> bool,
    pub phase: &'a dyn Fn(&'static str),
    pub progress: &'a dyn Fn(crate::capability_archive::Progress),
    pub committing: &'a dyn Fn() -> ApiResult<()>,
}
fn ignore_phase(_phase: &'static str) {}
fn ignore_progress(_progress: crate::capability_archive::Progress) {}
fn direct_commit() -> ApiResult<()> {
    if crate::speech::cancelled() {
        Err(ApiError::new(422, "cancelled"))
    } else {
        Ok(())
    }
}
impl InstallHooks<'static> {
    fn direct() -> Self {
        Self {
            cancelled: &crate::speech::cancelled,
            phase: &ignore_phase,
            progress: &ignore_progress,
            committing: &direct_commit,
        }
    }
}
impl InstallHooks<'_> {
    fn check(&self) -> ApiResult<()> {
        if (self.cancelled)() {
            Err(ApiError::new(422, "cancelled"))
        } else {
            Ok(())
        }
    }
}
enum Input<'a> {
    Json(Value),
    Stream {
        reader: Box<dyn Read + 'a>,
        header: Value,
    },
}
impl Input<'_> {
    fn manifest(&self) -> &Value {
        match self {
            Self::Json(pack) => &pack["manifest"],
            Self::Stream { header, .. } => header,
        }
    }
}
fn input<'a>(mut reader: impl Read + Seek + 'a, size: u64) -> ApiResult<Input<'a>> {
    if size > crate::capability_archive::MAX_DOWNLOAD {
        return Err(error());
    }
    let mut magic = [0u8; 8];
    let complete = match reader.read_exact(&mut magic) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => false,
        Err(_) => return Err(error()),
    };
    reader.seek(SeekFrom::Start(0)).map_err(|_| error())?;
    if complete && &magic == crate::capability_archive::MAGIC {
        let header =
            crate::capability_archive::inspect_header(&mut reader).map_err(archive_error)?;
        reader.seek(SeekFrom::Start(0)).map_err(|_| error())?;
        return Ok(Input::Stream {
            reader: Box::new(reader),
            header,
        });
    }
    if size > 128 * 1024 * 1024 {
        return Err(error());
    }
    let mut bytes = Vec::new();
    reader
        .take(128 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| error())?;
    if bytes.len() > 128 * 1024 * 1024 {
        return Err(error());
    }
    Ok(Input::Json(
        serde_json::from_slice(&bytes).map_err(|_| error())?,
    ))
}
// v1 resources may be re-encoded without changing their trusted identity, as
// v1 already permits raw/gzip payloads. Only compression framing comes from the
// inspected header; every path, byte digest, size and review must match the app.
fn stream_manifest(expected: &Value, header: &Value) -> ApiResult<Value> {
    crate::capability_archive::validate_manifest(header).map_err(archive_error)?;
    if expected["schema"] == 2 {
        if expected != header {
            return Err(error());
        }
        return Ok(expected.clone());
    }
    let mut projected = header.clone();
    projected["schema"] = json!(1);
    for file in projected["files"].as_array_mut().ok_or_else(error)? {
        let object = file.as_object_mut().ok_or_else(error)?;
        if object.get("executable").and_then(Value::as_bool) == Some(true)
            && !object
                .get("path")
                .and_then(Value::as_str)
                .is_some_and(|p| p.starts_with("bin/"))
        {
            return Err(error());
        }
        object.remove("compressedSize");
        object.remove("executable");
    }
    if &projected != expected {
        return Err(error());
    }
    Ok(header.clone())
}
fn legacy_manifest(expected: &Value) -> Value {
    let mut value = expected.clone();
    value["schema"] = json!(1);
    if let Some(files) = value["files"].as_array_mut() {
        for file in files {
            if let Some(object) = file.as_object_mut() {
                object.remove("compressedSize");
                object.remove("executable");
            }
        }
    }
    value
}
struct DigestReader<'a> {
    reader: &'a mut dyn Read,
    digest: Sha256,
    bytes: u64,
}
impl Read for DigestReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let n = self.reader.read(buffer)?;
        self.bytes += n as u64;
        if self.bytes > crate::capability_archive::MAX_DOWNLOAD {
            return Err(std::io::Error::other("Capability input budget"));
        }
        self.digest.update(&buffer[..n]);
        Ok(n)
    }
}
fn module_state(app: &App, id: &str, ready: bool) {
    if matches!(id, ID | "ocr") {
        let (module, code) = if id == ID {
            ("pdf_editor", "pdf_engine_unavailable")
        } else {
            ("ocr", "ocr_no_engine")
        };
        let mut registry = app.modules.lock().unwrap_or_else(|e| e.into_inner());
        if ready {
            registry.set(module, "ready");
        } else {
            registry.set_error(module, code);
        }
    }
}
pub fn endpoint(app: &Arc<App>, req: &Request) -> ApiResult<Response> {
    if req.method != "POST" {
        return Err(ApiError::new(405, "method_not_allowed"));
    }
    let v = req.json()?;
    if matches!(
        v["action"].as_str(),
        Some(
            "catalog"
                | "download"
                | "download_status"
                | "cancel_download"
                | "import"
                | "upload_start"
                | "upload_finish"
                | "uninstall_start"
        )
    ) {
        return crate::capability_delivery::endpoint(app, &v);
    }
    operate(app, &v, None, &InstallHooks::direct())
}
pub(crate) fn install_bytes(app: &Arc<App>, bytes: &[u8], id: &str) -> ApiResult<Response> {
    if bytes.len() > 128 * 1024 * 1024 {
        return Err(error());
    }
    let pack = input(Cursor::new(bytes), bytes.len() as u64)?;
    let selected = if id.is_empty() {
        pack.manifest()["id"].as_str().ok_or_else(error)?
    } else {
        id
    };
    let selected = selected.to_string();
    operate(
        app,
        &json!({"action":"install","id":selected}),
        Some(pack),
        &InstallHooks::direct(),
    )
}
pub(crate) fn install_file(
    app: &Arc<App>,
    path: &Path,
    id: &str,
    hooks: &InstallHooks<'_>,
) -> ApiResult<Response> {
    hooks.check()?;
    let file = fs::File::open(path).map_err(|_| error())?;
    let metadata = file.metadata().map_err(|_| error())?;
    if !metadata.is_file() {
        return Err(error());
    }
    let pack = input(file, metadata.len())?;
    let selected = if id.is_empty() {
        pack.manifest()["id"].as_str().ok_or_else(error)?
    } else {
        id
    };
    let selected = selected.to_string();
    operate(
        app,
        &json!({"action":"install","id":selected}),
        Some(pack),
        hooks,
    )
}
pub(crate) fn uninstall_with_hooks(app: &Arc<App>, id: &str, hooks: &InstallHooks<'_>) -> ApiResult<Response> {
    operate(app, &json!({"action":"uninstall","id":id}), None, hooks)
}
fn operate(
    app: &Arc<App>,
    v: &Value,
    provided: Option<Input<'_>>,
    hooks: &InstallHooks<'_>,
) -> ApiResult<Response> {
    let action = v["action"].as_str().unwrap_or("status");
    if action == "install" && provided.is_none() {
        let path = app
            .paths
            .resolve_doc(v["path"].as_str().ok_or_else(error)?)
            .map_err(|_| error())?;
        return install_file(app, &path, v["id"].as_str().unwrap_or(""), hooks);
    }
    let _lock = resource_guard_checked(&||hooks.check())?;
    // Installation identifies the inert package from its locked manifest; old
    // clients need not know the engine ID before selecting the file.
    hooks.check()?;
    let pack = provided;
    let owned_id = v["id"]
        .as_str()
        .or_else(|| pack.as_ref().and_then(|p| p.manifest()["id"].as_str()))
        .unwrap_or(ID)
        .to_string();
    let id = owned_id.as_str();
    if action == "install" && pack.as_ref().is_some_and(|p| p.manifest()["id"] != id) {
        return Err(error());
    }
    let selected = spec(id)?;
    let pack_root = root_for(&app.paths.data_dir, selected);
    let target = pack_root.join(selected.version);
    let expected = lock_for(&app.paths.assets_dir, selected)?;
    let is_pdf = selected.id == ID;
    let set_pdf = |ready: bool| module_state(app, selected.id, ready);
    let probe = |dir: &Path| probe(selected.id, dir, &app.paths.data_dir);
    // All cooperating desktop, MCP and installer processes use this stable
    // per-capability lock. Status may inspect under a shared lease when a
    // reader is active, but must never probe through another writer.
    let exclusive=crate::capability_process::try_acquire(&app.paths.data_dir,id,crate::capability_process::Mode::Exclusive)?;
    let process_busy=exclusive.is_none();
    let _process=if let Some(guard)=exclusive { guard } else if action=="status" {
        match crate::capability_process::try_acquire(&app.paths.data_dir,id,crate::capability_process::Mode::Shared)? {
            Some(guard)=>guard,
            None=>return ok_json(json!({"ok":true,"id":selected.id,"version":selected.version,"platform":selected.platform,
                "installed":false,"ready":false,"busy":true,"busy_code":busy_error(id),"present":target.exists()})),
        }
    } else { return Err(ApiError::new(409,busy_error(id))); };
    let mut migration_error=None;
    if !process_busy && local_busy_code(app,id).is_none() {
        recover_locked(&app.paths.data_dir,&app.paths.assets_dir,selected,&expected,&||hooks.check())?;
        if action=="status" {
            if let Err(error)=legacy::migrate(&app.paths.data_dir,selected,&expected,&||hooks.check()) {
                if error.code=="cancelled" || error.status==409 { return Err(error); }
                // A damaged old runtime is not installed. Keep the normal
                // missing-pack/install flow available; do not claim readiness
                // or turn a recoverable legacy input into a broken panel.
                migration_error=Some(error.code);
            }
        }
    }
    if action == "status" {
        let busy = local_busy_code(app,id).or_else(||process_busy.then_some(busy_error(id)));
        let valid = !transactions_blocking(&root_for(&app.paths.data_dir,selected),&app.paths.assets_dir,selected,&expected)? && validate(&target, &expected).is_ok();
        let ready = valid && busy.is_none() && probe(&target).is_ok();
        let installed = ready || (valid && busy.is_some());
        if !installed && busy.is_none() {
            set_pdf(false);
        }
        return ok_json(
            json!({"ok":true,"id":selected.id,"version":selected.version,"platform":selected.platform,"installed":installed,"ready":ready,"busy":busy.is_some(),"busy_code":busy,"present":target.exists(),"cleanup_pending":transactions_pending(&root_for(&app.paths.data_dir,selected))?,"legacy_migration_error":migration_error}),
        );
    }
    if let Some(code) = local_busy_code(app, id) {
        return Err(ApiError::new(409, code));
    }
    if action == "uninstall" {
        (hooks.phase)("removing");
        hooks.check()?;
        let committing = || {
            (hooks.committing)()?;
            legacy::disable(&app.paths.data_dir, selected, &expected)
        };
        // Remove only the files named by the application lock. Never follow a
        // directory traversal or recursively remove user-created extra files.
        let preserved = if target.is_dir() {
            managed_parent(&app.paths.data_dir, selected, false)?;
            transaction::remove_checked(&target, &expected, &|| hooks.check(), hooks.progress, &committing)?
        } else {
            hooks.check()?;
            committing()?;
            Removal {
                unknown: false,
                cleanup_pending: false,
            }
        };
        set_pdf(false);
        return ok_json(
            json!({"ok":true,"id":selected.id,"installed":false,"unknown_files_preserved":preserved.unknown,"cleanup_pending":preserved.cleanup_pending}),
        );
    }
    if action != "install" {
        return Err(error());
    }
    let pack = pack.ok_or_else(error)?;
    hooks.check()?;
    // Hold profile-wide admission across extraction/probe/commit. Different
    // capability writers in another process must not both spend the same
    // observed free bytes. Resource readers keep their independent leases.
    let _space_admission = crate::capability_process::install_admission(&app.paths.data_dir)?;
    hooks.check()?;
    managed_parent(&app.paths.data_dir, selected, true)?;
    let space_result = space::check(&pack_root, &expected);
    // Cancellation wins even when it arrives during the native quota query.
    hooks.check()?;
    let installation_space = space_result?;
    (hooks.phase)("extracting");
    let staging = match pack {
        Input::Stream { mut reader, header } => {
            let manifest = stream_manifest(&expected, &header)?;
            // Reject an unavailable/mismatched catalog before expanding tens
            // of thousands of files. Digest verification still follows the
            // bounded stream; a QA lock is never itself a distribution entry.
            let entry = if expected["schema"] == 2 {
                Some(crate::capability_delivery::checked_entry(&app.paths.assets_dir, id)?)
            } else { None };
            let mut hashed = DigestReader {
                reader: reader.as_mut(),
                digest: Sha256::new(),
                bytes: 0,
            };
            let staging = staging::Stage::create(&pack_root, &expected)?;
            crate::capability_archive::stage_into(
                &mut hashed,
                &manifest,
                &staging.path(),
                |root| {
                    #[cfg(windows)]
                    if id == "latex" { crate::worker_sandbox::prepare_empty_runtime(root).map_err(crate::capability_archive::Error::Io)?; }
                    #[cfg(not(windows))]
                    let _ = root;
                    Ok(())
                },
                || (hooks.cancelled)(),
                |p| (hooks.progress)(p),
            )
            .map_err(archive_error)?;
            if let Some(entry) = entry {
                if entry["format"] != "stream-v2"
                    || entry["downloadBytes"].as_u64() != Some(hashed.bytes)
                    || entry["sha256"].as_str()
                        != Some(format!("{:x}", hashed.digest.finalize()).as_str())
                {
                    return Err(ApiError::new(422, "capability_download_integrity"));
                }
            }
            staging
        }
        Input::Json(pack) => {
            if pack["manifest"] != legacy_manifest(&expected) {
                return Err(error().noted("stage", "manifest"));
            }
            let payload = pack["payload"].as_object().ok_or_else(error)?;
            let files = checked_files(&expected)?;
            if files.len() > 64
                || files
                    .iter()
                    .any(|file| file["size"].as_u64().is_none_or(|n| n > 128 * 1024 * 1024))
            {
                return Err(error());
            }
            if payload.len() != files.len() {
                return Err(error().noted("stage", "payload-count"));
            }
            let staging = staging::Stage::create(&pack_root, &expected)?;
            for item in files {
                hooks.check()?;
                let name = item["path"].as_str().ok_or_else(error)?;
                if name.split('/').any(|p| {
                    p.is_empty()
                        || p == "."
                        || p == ".."
                        || !p
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
                }) {
                    return Err(error());
                }
                let payload_error = || error().noted("stage", "payload").noted("file", name);
                let item_payload = &payload[name];
                let compressed = item_payload["compression"] == "gzip";
                let bytes = STANDARD
                    .decode(
                        if compressed {
                            item_payload["data"].as_str()
                        } else {
                            item_payload.as_str()
                        }
                        .ok_or_else(payload_error)?,
                    )
                    .map_err(|_| payload_error())?;
                let bytes = if compressed {
                    let size = item["size"]
                        .as_u64()
                        .filter(|n| *n <= 128 * 1024 * 1024)
                        .ok_or_else(payload_error)?;
                    let mut output = Vec::new();
                    flate2::read::GzDecoder::new(bytes.as_slice())
                        .take(size + 1)
                        .read_to_end(&mut output)
                        .map_err(|_| payload_error())?;
                    output
                } else {
                    bytes
                };
                if bytes.len() as u64 != item["size"].as_u64().ok_or_else(payload_error)?
                    || item["sha256"].as_str() != Some(hash(&bytes).as_str())
                {
                    return Err(payload_error().noted("reason", "digest-or-size"));
                }
                let file = staging.path().join(name);
                fs::create_dir_all(file.parent().ok_or_else(payload_error)?)
                    .map_err(|_| payload_error())?;
                let mut output = fs::OpenOptions::new().write(true).create_new(true).open(file).map_err(|_| payload_error())?;
                use std::io::Write;
                output.write_all(&bytes).and_then(|_| output.sync_all()).map_err(|_| payload_error())?;
                log::debug!(target:"readmd::capability::transaction", "staging payload written");
            }
            staging
        }
    };
    #[cfg(unix)]
    for item in checked_files(&expected)? {
        let name = item["path"].as_str().ok_or_else(error)?;
        if (expected["schema"] == 1 && name.starts_with("bin/")) || item["executable"] == true {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(staging.path().join(name), fs::Permissions::from_mode(0o755))
                .map_err(|_| error())?;
        }
    }
    hooks.check()?;
    log::debug!(target:"readmd::capability::transaction", "staging validation");
    validate(staging.path(), &expected).map_err(|e| e.noted("stage", "validation"))?;
    (hooks.phase)("probing");
    probe(staging.path()).map_err(|failure| {
        if is_pdf {
            error()
        } else {
            ApiError::with(
                422,
                failure
                    .split(": ")
                    .next()
                    .unwrap_or("typst_engine_unavailable"),
                &failure,
            )
        }
    })?;
    log::debug!(target:"readmd::capability::transaction", "staging probed");
    hooks.check()?;
    // A valid byte set can still be unusable due to extra loader files. Probe
    // the actual existing directory before deciding that repair is unnecessary.
    let unchanged =
        target.exists() && validate(&target, &expected).is_ok() && probe(&target).is_ok();
    hooks.check()?;
    (hooks.committing)()?;
    if unchanged {
        let cleanup = staging.cleanup();
        set_pdf(true);
        return ok_json(json!({"ok":true,"id":selected.id,"installed":true,"unchanged":true,
            "unknown_files_preserved":cleanup.unknown,"cleanup_pending":cleanup.cleanup_pending,"installation_space":installation_space}));
    }
    let committed = replacement::commit(&staging.path(), &target, &expected)?;
    let cleanup = staging.cleanup();
    set_pdf(true);
    ok_json(json!({"ok":true,"id":selected.id,"installed":true,"repaired":committed.repaired,
        "unknown_files_preserved":committed.unknown || cleanup.unknown,"cleanup_pending":committed.cleanup_pending || cleanup.cleanup_pending,"installation_space":installation_space}))
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Explicit real-pack CLI fixture. Ordinary unit-test counts do not claim
    /// this installation flow: the maintainer supplies owned paths and runs it.
    #[test]
    fn installation_space_offline_lifecycle_child() {
        let Some(data) = std::env::var_os("READMD_SPACE_INSTALL_DATA") else { return; };
        let data=PathBuf::from(data);
        let assets=PathBuf::from(std::env::var_os("READMD_SPACE_INSTALL_ASSETS").expect("assets"));
        let pack=PathBuf::from(std::env::var_os("READMD_SPACE_INSTALL_PACK").expect("offline pack"));
        let workspace=data.join("documents");fs::create_dir_all(&workspace).unwrap();
        let app=Arc::new(App::bootstrap(crate::paths::AppPaths::with_dirs(&data,&workspace,&assets)).unwrap());
        let sentinel=workspace.join("untouched.md");fs::write(&sentinel,b"Owned sentinel remains unchanged").unwrap();
        let admission=crate::capability_process::install_admission(&data).unwrap();
        let failure=install_file(&app,&pack,"fonts",&InstallHooks::direct()).err().unwrap();
        assert_eq!(failure.code,"capability_install_busy");drop(admission);
        let decode=|response:Response| {assert_eq!(response.status,200);serde_json::from_slice::<Value>(&response.body).unwrap()};
        let installed=decode(install_file(&app,&pack,"fonts",&InstallHooks::direct()).unwrap());
        assert_eq!(installed["installed"],true);
        #[cfg(windows)] {
            assert!(installed["installation_space"]["additional_bytes"].as_u64().unwrap()>installed["installation_space"]["expanded_bytes"].as_u64().unwrap());
            assert!(installed["installation_space"]["allocation_unit"].as_u64().unwrap()>0);
        }
        let repeated=decode(install_file(&app,&pack,"fonts",&InstallHooks::direct()).unwrap());
        assert_eq!(repeated["unchanged"],true);
        let selected=spec("fonts").unwrap();let expected=lock_for(&assets,selected).unwrap();
        let target=root_for(&data,selected).join(selected.version);
        let font=checked_files(&expected).unwrap().iter().find_map(|entry|entry["path"].as_str().filter(|name|name.ends_with(".ttf"))).unwrap();
        fs::write(target.join(font),b"Owned damaged font fixture").unwrap();
        let cancel=||false;let phase=|_:&str|{};let progress=|_:crate::capability_archive::Progress|{};
        let committing=||Err(ApiError::new(422,"cancelled"));
        let hooks=InstallHooks{cancelled:&cancel,phase:&phase,progress:&progress,committing:&committing};
        let cancelled=install_file(&app,&pack,"fonts",&hooks).err().unwrap();
        assert_eq!(cancelled.code,"cancelled");
        assert_eq!(fs::read(target.join(font)).unwrap(),b"Owned damaged font fixture");
        drop(crate::capability_process::install_admission(&data).unwrap());
        let repaired=decode(install_file(&app,&pack,"fonts",&InstallHooks::direct()).unwrap());
        validate(&target,&expected).unwrap();probe("fonts",&target,&data).unwrap();
        assert_eq!(fs::read(&sentinel).unwrap(),b"Owned sentinel remains unchanged");
        let result=json!({"passed":true,"busyRejectedBeforePayload":true,"installed":installed,"repeatUnchanged":repeated["unchanged"],"cancelPreservedDamagedOriginal":true,"admissionReleasedAfterCancel":true,"repaired":repaired,"sentinelUnchanged":true,"realLowDiskOrHardQuotaCertified":false});
        fs::write(data.join("install-space-result.json"),serde_json::to_vec_pretty(&result).unwrap()).unwrap();
        println!("INSTALL_SPACE_RESULT {}",result);
    }
    pub(super) fn fixture(dir: &Path) -> Value {
        let mut files = Vec::new();
        for name in ["bin/engine", "data/model", "NOTICE"] {
            let bytes = format!("reviewed {name}").into_bytes();
            let file = dir.join(name);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, &bytes).unwrap();
            files.push(json!({"path":name,"size":bytes.len(),"sha256":hash(&bytes)}));
        }
        json!({"schema":1,"id":"fonts","version":crate::publication_fonts::VERSION,"platform":"universal","files":files,"review":{"license":"test fixture only"}})
    }
    fn stream(lock: &Value) -> Value {
        let mut result = lock.clone();
        result["schema"] = json!(2);
        for file in result["files"].as_array_mut().unwrap() {
            file["compressedSize"] = json!(64);
        }
        result
    }
    #[test]
    fn reencoding_preserves_all_trusted_resource_identity() {
        let temporary = tempfile::tempdir().unwrap();
        let expected = fixture(temporary.path());
        let header = stream(&expected);
        assert!(stream_manifest(&expected, &header).is_ok());
        assert!(stream_manifest(&header, &header).is_ok());
        for pointer in ["/id", "/review/license", "/files/0/sha256", "/files/0/path"] {
            let mut changed = header.clone();
            *changed.pointer_mut(pointer).unwrap() = json!("untrusted");
            assert!(stream_manifest(&expected, &changed).is_err());
        }
        let mut changed = header.clone();
        changed["files"][1]["executable"] = json!(true);
        assert!(stream_manifest(&expected, &changed).is_err());
        let mut changed = header;
        changed["files"][0]["executable"] = json!(true);
        assert!(stream_manifest(&expected, &changed).is_ok());
    }
    #[test]
    fn format_detection_handles_fragmented_reads() {
        struct Fragmented(Cursor<Vec<u8>>);
        impl Read for Fragmented {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                let n = buffer.len().min(3);
                self.0.read(&mut buffer[..n])
            }
        }
        impl Seek for Fragmented {
            fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
                self.0.seek(position)
            }
        }
        let temporary = tempfile::tempdir().unwrap();
        let lock = stream(&fixture(temporary.path()));
        let header = serde_json::to_vec(&lock).unwrap();
        let mut bytes = crate::capability_archive::MAGIC.to_vec();
        bytes.extend_from_slice(&(header.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&header);
        let size = bytes.len() as u64;
        assert!(matches!(
            input(Fragmented(Cursor::new(bytes)), size).unwrap(),
            Input::Stream { .. }
        ));
        assert!(input(
            Cursor::new(b"{}"),
            crate::capability_archive::MAX_DOWNLOAD + 1
        )
        .is_err());
    }
    #[test]
    fn uninstall_preserves_unknown_nested_files_in_place() {
        let temporary = tempfile::tempdir().unwrap();
        let target = temporary.path().join(crate::publication_fonts::VERSION);
        let lock = fixture(&target);
        let unknown = target.join("data/user/notes.txt");
        fs::create_dir_all(unknown.parent().unwrap()).unwrap();
        fs::write(&unknown, b"keep exactly here").unwrap();
        let result = remove_known(&target, &lock).unwrap();
        assert!(result.unknown);
        assert!(!result.cleanup_pending);
        assert_eq!(fs::read(unknown).unwrap(), b"keep exactly here");
        for file in lock["files"].as_array().unwrap() {
            assert!(!target.join(file["path"].as_str().unwrap()).exists());
        }
        assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 1);
    }
    #[test]
    #[cfg(windows)]
    fn locked_uninstall_rolls_back_previously_moved_files() {
        use std::os::windows::fs::OpenOptionsExt;
        let temporary = tempfile::tempdir().unwrap();
        let target = temporary.path().join(crate::publication_fonts::VERSION);
        let lock = fixture(&target);
        let _held = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(target.join("data/model"))
            .unwrap();
        assert!(remove_known(&target, &lock).is_err());
        assert_eq!(
            fs::read(target.join("bin/engine")).unwrap(),
            b"reviewed bin/engine"
        );
        assert!(target.join("NOTICE").is_file());
        drop(_held);
        validate(&target, &lock).unwrap();
        assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 1);
        assert!(!remove_known(&target, &lock).unwrap().unknown);
    }
    #[test]
    fn leases_register_before_guard_release_and_drop_by_reference_count() {
        let temporary = tempfile::tempdir().unwrap();
        let data = temporary.path().join("data");
        let assets = temporary.path().join("assets");
        let selected = spec("fonts").unwrap();
        let target = root_for(&data, selected).join(selected.version);
        let lock = fixture(&target);
        let key = lease_key(&data, selected);
        fs::create_dir_all(assets.join("capabilities")).unwrap();
        fs::write(
            assets.join(format!(
                "capabilities/fonts-{}-universal.lock.json",
                selected.version
            )),
            serde_json::to_vec(&lock).unwrap(),
        )
        .unwrap();
        let first = lease(&data, &assets, "fonts").unwrap();
        let second = lease(&data, &assets, "fonts").unwrap();
        assert_eq!(leases().lock().unwrap().get(&key), Some(&2));
        drop(first);
        assert_eq!(leases().lock().unwrap().get(&key), Some(&1));
        drop(second);
        assert!(!leases().lock().unwrap().contains_key(&key));
        fs::write(target.join("data/model"), b"changed").unwrap();
        assert!(lease(&data, &assets, "fonts").is_err());
        assert!(!leases().lock().unwrap().contains_key(&key));
    }
    #[test]
    fn waiting_process_writer_preserves_cancellation_deadline_and_reservations() {
        use std::{sync::atomic::{AtomicBool,Ordering},time::{Duration,Instant}};
        let root=tempfile::tempdir().unwrap();let data=root.path().join("data");let assets=root.path().join("assets");
        let selected=spec("fonts").unwrap();let target=root_for(&data,selected).join(selected.version);
        let lock=fixture(&target);fs::create_dir_all(assets.join("capabilities")).unwrap();
        fs::write(assets.join(format!("capabilities/fonts-{}-universal.lock.json",selected.version)),serde_json::to_vec(&lock).unwrap()).unwrap();
        let writer=crate::capability_process::try_acquire(&data,"fonts",crate::capability_process::Mode::Exclusive).unwrap().unwrap();
        let reserved=AtomicBool::new(false);let deadline=Instant::now()+Duration::from_millis(40);
        let result=lease_reserved(&data,&assets,"fonts",&|| {
            if Instant::now()>=deadline { Err(ApiError::new(422,"latex_timeout")) } else { Ok(()) }
        },|| {reserved.store(true,Ordering::Release);Ok(())});
        assert_eq!(result.err().unwrap().code,"latex_timeout");assert!(!reserved.load(Ordering::Acquire));
        let cancelled=Arc::new(AtomicBool::new(false));
        std::thread::scope(|scope| {
            let flag=cancelled.clone();
            let job=scope.spawn(||crate::speech::with_cancel(Arc::new(move||flag.load(Ordering::Acquire)),||lease(&data,&assets,"fonts")));
            std::thread::sleep(Duration::from_millis(30));cancelled.store(true,Ordering::Release);
            assert_eq!(job.join().unwrap().err().unwrap().code,"cancelled");
        });
        assert!(!leases().lock().unwrap().contains_key(&lease_key(&data,selected)));
        drop(writer);let resource=lease(&data,&assets,"fonts").unwrap();drop(resource);validate(&target,&lock).unwrap();
    }
    #[test]
    fn validation_cancellation_does_not_change_installed_files() {
        let temporary = tempfile::tempdir().unwrap();
        let lock = fixture(temporary.path());
        let failure =
            crate::speech::with_cancel(Arc::new(|| true), || validate(temporary.path(), &lock))
                .unwrap_err();
        assert_eq!(failure.code, "cancelled");
        validate(temporary.path(), &lock).unwrap();
    }
    #[test]
    fn interrupted_hash_and_lock_wait_keep_the_runtime_intact() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let temporary = tempfile::tempdir().unwrap();
        let file = temporary.path().join("large-resource");
        fs::write(&file, vec![42u8; 1024 * 1024]).unwrap();
        let checks = AtomicUsize::new(0);
        let failure = hash_file(&file, &|| {
            if checks.fetch_add(1, Ordering::AcqRel) >= 3 {
                Err(ApiError::new(422, "latex_timeout"))
            } else { Ok(()) }
        }).unwrap_err();
        assert_eq!(failure.code, "latex_timeout");
        assert_eq!(fs::metadata(&file).unwrap().len(), 1024 * 1024);
        let _held = resource_guard();
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(30);
        let failure = resource_guard_checked(&|| {
            if std::time::Instant::now() >= deadline { Err(ApiError::new(422, "latex_timeout")) }
            else { Ok(()) }
        }).err().unwrap();
        assert_eq!(failure.code, "latex_timeout");
        assert!(deadline.elapsed() < std::time::Duration::from_millis(200));
    }
    #[test]
    fn runtime_revision_expires_when_uninstall_keeps_unknown_files() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("profile");
        let assets = root.path().join("assets");
        let selected = spec("fonts").unwrap();
        let directory = root_for(&data, selected).join(selected.version);
        let lock = fixture(&directory);
        fs::create_dir_all(assets.join("capabilities")).unwrap();
        fs::write(assets.join(format!("capabilities/fonts-{}-universal.lock.json", selected.version)), serde_json::to_vec(&lock).unwrap()).unwrap();
        assert!(runtime_revision(&data, &assets, "fonts").is_ok());
        let unknown = directory.join("user-note.txt"); fs::write(&unknown, b"keep").unwrap();
        assert!(remove_known(&directory, &lock).unwrap().unknown);
        assert!(directory.is_dir()); assert!(runtime_revision(&data, &assets, "fonts").is_err());
        assert_eq!(fs::read(unknown).unwrap(), b"keep");
    }
    #[test]
    fn validation_rechecks_changed_ancestor_after_hashing() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        let root = tempfile::tempdir().unwrap();
        let lock = large_fixture(root.path());
        let caller=std::thread::current().id();
        let worker_checks=AtomicUsize::new(0);
        let hashing=AtomicBool::new(false);
        let changed=AtomicBool::new(false);
        let checks_at_change=AtomicUsize::new(0);
        let folder = root.path().join("chunks");
        let archived = root.path().join("prior-chunks");
        // The explicit private join boundary runs on the caller after all
        // file workers finish, independently of callbacks and CPU count.
        let result = validate_checked_phases(root.path(), &lock, &|| {
            if hashing.load(Ordering::Acquire)&&std::thread::current().id()!=caller {
                worker_checks.fetch_add(1,Ordering::AcqRel);
            }
            Ok(())
        },&|phase| {
            if phase==ValidationPhase::Files { hashing.store(true,Ordering::Release); }
            if phase==ValidationPhase::FilesJoined {
                assert_eq!(std::thread::current().id(),caller);
                hashing.store(false,Ordering::Release);changed.store(true,Ordering::Release);
                checks_at_change.store(worker_checks.load(Ordering::Acquire),Ordering::Release);
                fs::rename(&folder, &archived).unwrap();
                fs::write(&folder, b"not a directory").unwrap();
            }
            Ok(())
        });
        assert!(changed.load(Ordering::Acquire));assert!(archived.is_dir());
        assert_eq!(result.unwrap_err().code,"capability_pack_invalid");
        assert!(checks_at_change.load(Ordering::Acquire)>=lock["files"].as_array().unwrap().len());
        assert_eq!(worker_checks.load(Ordering::Acquire),checks_at_change.load(Ordering::Acquire),"Every hash worker must have joined before the post-directory check");
        fs::remove_file(&folder).unwrap();fs::rename(&archived, &folder).unwrap();
        validate(root.path(), &lock).unwrap();
    }
    #[test]
    fn validation_shares_root_and_all_ancestor_checks_without_persistent_trust() {
        let root=tempfile::tempdir().unwrap();
        let files=vec![json!({"path":"one/two/a"}),json!({"path":"one/two/b"}),json!({"path":"one/three/c"}),json!({"path":"top"})];
        let parents=validation_directories(root.path(),&files,&||Ok(())).unwrap();
        assert_eq!(parents,BTreeSet::from([root.path().to_path_buf(),root.path().join("one"),root.path().join("one/two"),root.path().join("one/three")]));
        fs::create_dir_all(root.path().join("one/two")).unwrap();fs::create_dir(root.path().join("one/three")).unwrap();
        let canonical=root.path().canonicalize().unwrap();validate_directories(&parents,&canonical,validation_lanes(300),&||Ok(())).unwrap();
        fs::remove_dir(root.path().join("one/two")).unwrap();fs::write(root.path().join("one/two"),b"ordinary file replaces a checked intermediate directory").unwrap();
        assert_eq!(validate_directories(&parents,&canonical,validation_lanes(300),&||Ok(())).unwrap_err().code,"capability_pack_invalid");
        let cancelled:ApiResult<_>=crate::speech::with_cancel(Arc::new(||true),||validation_directories(root.path(),&files,&||Ok(())));
        assert_eq!(cancelled.unwrap_err().code,"cancelled","Collecting paths must inherit cancellation before any file hashing");
        let cancelled:ApiResult<_>=crate::speech::with_cancel(Arc::new(||true),||validate_directories(&parents,&canonical,validation_lanes(300),&||Ok(())));
        assert_eq!(cancelled.unwrap_err().code,"cancelled","Directory phases must not mask cancellation with a later path error");
    }
    #[test]
    fn managed_tex_integrity_phase_timing() {
        use std::{io::Write,time::{Duration,Instant}};
        // Ordinary unit runs do not read a large external runtime. An explicit
        // filtered invocation supplies all three paths; its JSON, rather than
        // the ordinary test count, is the timing evidence.
        let Some(root)=std::env::var_os("READMD_TEX_TIMING_ROOT") else { return; };
        let started=Instant::now();
        let root=PathBuf::from(root);
        let lock_path=PathBuf::from(std::env::var_os("READMD_TEX_TIMING_LOCK").expect("READMD_TEX_TIMING_LOCK"));
        let report=PathBuf::from(std::env::var_os("READMD_TEX_TIMING_REPORT").expect("READMD_TEX_TIMING_REPORT"));
        let lock_bytes=fs::read(lock_path).unwrap();
        let lock:Value=serde_json::from_slice(&lock_bytes).unwrap();
        let boundaries=Mutex::new(Vec::new());
        // This invokes exactly the production validator once. In particular,
        // there is no readiness probe or preliminary runtime hash pass.
        let result=validate_checked_phases(&root,&lock,&|| {
            if started.elapsed()>=Duration::from_secs(120) { Err(ApiError::new(422,"latex_timeout")) }
            else { Ok(()) }
        },&|phase| {
            boundaries.lock().unwrap().push(json!({"phase":format!("{phase:?}"),"elapsedMs":started.elapsed().as_millis()}));
            Ok(())
        });
        let value=json!({
            "schemaVersion":1,"scope":"one production integrity validation; no compiler execution",
            "processCold":true,"osCacheState":"uncontrolled","coldPerformanceCertified":false,
            "readyProbeBeforeTiming":false,"runtimeModified":false,"deadlineMs":120000,
            "files":lock["files"].as_array().unwrap().len(),
            "bytes":lock["files"].as_array().unwrap().iter().map(|file|file["size"].as_u64().unwrap()).sum::<u64>(),
            "manifestSha256":hash(&lock_bytes),"lanes":validation_lanes(lock["files"].as_array().unwrap().len()),
            "elapsedMs":started.elapsed().as_millis(),"boundaries":boundaries.into_inner().unwrap(),
            "ok":result.is_ok(),"errorCode":result.as_ref().err().map(|failure|failure.code.as_str())
        });
        // An existing observation is never overwritten, including failures.
        let mut output=fs::OpenOptions::new().write(true).create_new(true).open(report).unwrap();
        output.write_all(&serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        output.write_all(b"\n").unwrap();
        println!("{value}");
        result.unwrap();
    }
    #[test]
    fn single_lane_validation_still_rechecks_ancestors_after_complete_hashes() {
        let root=tempfile::tempdir().unwrap();let lock=fixture(root.path());
        assert_eq!(validation_lanes(lock["files"].as_array().unwrap().len()),1);
        let folder=root.path().join("data");let archived=root.path().join("prior-data");
        let result=validate_checked_phases(root.path(),&lock,&||Ok(()),&|phase| {
            if phase==ValidationPhase::FilesJoined {
                fs::rename(&folder,&archived).unwrap();fs::write(&folder,b"changed after all single-lane hashes").unwrap();
            }
            Ok(())
        });
        assert!(archived.is_dir());assert_eq!(result.unwrap_err().code,"capability_pack_invalid");
        fs::remove_file(&folder).unwrap();fs::rename(&archived,&folder).unwrap();validate(root.path(),&lock).unwrap();
    }
    #[test]
    fn legacy_partition_entry_keeps_serial_hashes_ancestors_and_root_binding() {
        let root=tempfile::tempdir().unwrap();let lock=fixture(root.path());
        let files=lock["files"].as_array().unwrap();let canonical=root.path().canonicalize().unwrap();
        // Cell deliberately supplies a non-Sync callback like the existing
        // migration/recovery callers. Restoring only an argument list must
        // not silently require Sync or omit their integrity checks.
        let checks=std::cell::Cell::new(0usize);
        let check=|| {checks.set(checks.get()+1);Ok(())};
        validate_partition(root.path(),&canonical,files,0,1,&check).unwrap();assert!(checks.get()>files.len());
        for (index,item) in files.iter().enumerate() {
            let file=root.path().join(item["path"].as_str().unwrap());let original=fs::read(&file).unwrap();
            assert!(!original.is_empty());let mut corrupted=original.clone();corrupted[0]^=1;fs::write(&file,&corrupted).unwrap();
            assert_eq!(validate_partition(root.path(),&canonical,files,index,files.len(),&check).unwrap_err().code,"capability_pack_invalid","Every selected lane must hash its bytes, including same-length corruption");
            fs::write(&file,original).unwrap();
        }
        assert_eq!(validate_partition(root.path(),canonical.parent().unwrap(),files,0,1,&check).unwrap_err().code,"capability_pack_invalid","A containing directory is not the exact canonical root");
        let folder=root.path().join("data");let archived=root.path().join("prior-data");fs::rename(&folder,&archived).unwrap();fs::write(&folder,b"changed ancestor").unwrap();
        assert_eq!(validate_partition(root.path(),&canonical,files,0,1,&check).unwrap_err().code,"capability_pack_invalid");
        fs::remove_file(&folder).unwrap();fs::rename(&archived,&folder).unwrap();
        let cancelled=crate::speech::with_cancel(Arc::new(||true),||validate_partition(root.path(),&canonical,files,0,1,&check));
        assert_eq!(cancelled.unwrap_err().code,"cancelled");validate_partition(root.path(),&canonical,files,0,1,&check).unwrap();
    }
    #[test]
    fn validation_pins_resources_without_holding_the_global_install_guard() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("profile");let assets = root.path().join("assets");
        let selected = spec("fonts").unwrap();let target = root_for(&data, selected).join(selected.version);
        let lock = fixture(&target);fs::create_dir_all(assets.join("capabilities")).unwrap();
        fs::write(assets.join(format!("capabilities/fonts-{}-universal.lock.json", selected.version)), serde_json::to_vec(&lock).unwrap()).unwrap();
        let key = lease_key(&data, selected);let observed = AtomicBool::new(false);
        let lease = lease_checked(&data, &assets, "fonts", &|| {
            // Observe the actual pinned phase, independent of other tests
            // temporarily holding the guard and causing more wait checks.
            let pinned=leases().lock().unwrap().contains_key(&key);
            if pinned && !observed.swap(true,Ordering::AcqRel) {
                let deadline = std::time::Instant::now()+std::time::Duration::from_secs(1);
                let _available = resource_guard_checked(&|| {
                    if std::time::Instant::now() >= deadline { Err(ApiError::new(422,"latex_timeout")) } else { Ok(()) }
                })?;
                assert_eq!(leases().lock().unwrap().get(&key), Some(&1));
            }
            Ok(())
        }).unwrap();
        assert!(observed.load(Ordering::Acquire));drop(lease);
        assert!(!leases().lock().unwrap().contains_key(&key));
    }
    fn large_fixture(dir:&Path)->Value {
        let mut lock=stream(&fixture(dir));
        for index in 0..300 {
            let name=format!("chunks/{index:03}");let bytes=vec![42u8;256];
            fs::create_dir_all(dir.join("chunks")).unwrap();fs::write(dir.join(&name),&bytes).unwrap();
            lock["files"].as_array_mut().unwrap().push(json!({"path":name,"size":bytes.len(),"sha256":hash(&bytes),"compressedSize":64}));
        }
        lock
    }
    #[test]
    fn parallel_validation_hashes_every_lane_and_joins_before_returning() {
        let root=tempfile::tempdir().unwrap();let lock=large_fixture(root.path());
        let files=lock["files"].as_array().unwrap();let lanes=validation_lanes(files.len());
        assert!((1..=8).contains(&lanes));assert_eq!(validation_lanes(256),1);
        let caller=std::thread::current().id();
        let hashing=std::sync::atomic::AtomicBool::new(false);
        let observed=Mutex::new(HashSet::new());
        validate_checked_phases(root.path(),&lock,&|| {
            let current=std::thread::current().id();if hashing.load(std::sync::atomic::Ordering::Acquire)&&current!=caller{observed.lock().unwrap().insert(current);}Ok(())
        },&|phase| {
            hashing.store(phase==ValidationPhase::Files,std::sync::atomic::Ordering::Release);Ok(())
        }).unwrap();
        assert_eq!(observed.lock().unwrap().len(),lanes,"Caller pre/post checks must not count as a hashing lane");
        for lane in 0..lanes {
            let (index,item)=files.iter().enumerate().find(|(index,item)|index%lanes==lane&&item["path"].as_str().unwrap().starts_with("chunks/")).unwrap();
            assert_eq!(index%lanes,lane);let file=root.path().join(item["path"].as_str().unwrap());
            let original=fs::read(&file).unwrap();let mut changed=original.clone();changed[0]^=1;fs::write(&file,&changed).unwrap();
            // Same-length corruption in each lane must still fail; parallel
            // success cannot hide the first actual integrity error.
            assert_eq!(validate(root.path(),&lock).unwrap_err().code,"capability_pack_invalid");
            fs::write(file,original).unwrap();
        }
        use std::sync::atomic::{AtomicUsize,Ordering};
        let checks=AtomicUsize::new(0);
        let failure=validate_checked_phases(root.path(),&lock,&|| {
            // Trigger only actual hash-lane activity. Parallel directory
            // checks and caller planning must not trigger this cancellation.
            if hashing.load(Ordering::Acquire)&&std::thread::current().id()!=caller&&checks.fetch_add(1,Ordering::AcqRel)>20 { Err(ApiError::new(422,"latex_timeout")) } else { Ok(()) }
        },&|phase| {
            hashing.store(phase==ValidationPhase::Files,Ordering::Release);Ok(())
        }).unwrap_err();
        assert_eq!(failure.code,"latex_timeout");let finished=checks.load(Ordering::Acquire);
        std::thread::sleep(std::time::Duration::from_millis(30));
        assert_eq!(checks.load(Ordering::Acquire),finished,"No background validator may outlive its caller");
        validate(root.path(),&lock).unwrap();
    }
    #[test]
    fn parallel_validation_inherits_caller_cancellation_without_changing_resources() {
        let root=tempfile::tempdir().unwrap();let lock=large_fixture(root.path());
        let error=crate::speech::with_cancel(Arc::new(||true),||validate(root.path(),&lock)).unwrap_err();
        assert_eq!(error.code,"cancelled");validate(root.path(),&lock).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn opened_resource_handle_blocks_writes_and_rename_while_hashing() {
        let root=tempfile::tempdir().unwrap();let file=root.path().join("resource");
        fs::write(&file,b"original").unwrap();
        let (reader,size)=resource_file(&file).unwrap();assert_eq!(size,8);
        assert!(fs::write(&file,b"changed!").is_err());
        assert!(fs::rename(&file,root.path().join("moved")).is_err());
        assert_eq!(hash_reader(reader,size,&||Ok(())).unwrap(),hash(b"original"));
        fs::write(&file,b"changed!").unwrap();
        assert!(resource_file(root.path()).is_err());
    }
    #[test]
    fn resource_hash_reads_all_bytes_without_redundant_windows_eof_io() {
        use std::{io::Cursor,sync::atomic::{AtomicUsize,Ordering}};
        struct Counted<'a> { source:Cursor<Vec<u8>>, reads:&'a AtomicUsize }
        impl Read for Counted<'_> {
            fn read(&mut self,buffer:&mut [u8])->std::io::Result<usize> {
                self.reads.fetch_add(1,Ordering::AcqRel);self.source.read(buffer)
            }
        }
        for size in [0usize,1,65535,65536,65537,131072] {
            let data=vec![42u8;size];let reads=AtomicUsize::new(0);
            let reader=Counted { source:Cursor::new(data.clone()),reads:&reads };
            assert_eq!(hash_reader(reader,size as u64,&||Ok(())).unwrap(),hash(&data));
            let expected=size.div_ceil(65536)+usize::from(!cfg!(windows));
            assert_eq!(reads.load(Ordering::Acquire),expected,"size={size}");
        }
    }
    #[test]
    fn resource_hash_rejects_short_reads_io_failure_and_midstream_cancel() {
        use std::{io::Cursor,sync::atomic::{AtomicUsize,Ordering}};
        struct Partial(Cursor<&'static [u8]>);
        impl Read for Partial {
            fn read(&mut self,buffer:&mut [u8])->std::io::Result<usize> {
                let limit=buffer.len().min(3);self.0.read(&mut buffer[..limit])
            }
        }
        assert_eq!(hash_reader(Partial(Cursor::new(b"complete")),8,&||Ok(())).unwrap(),hash(b"complete"));
        assert_eq!(hash_reader(Cursor::new(b"short"),6,&||Ok(())).unwrap_err().code,"capability_pack_invalid");
        struct Broken;
        impl Read for Broken {
            fn read(&mut self,_:&mut [u8])->std::io::Result<usize> { Err(std::io::ErrorKind::PermissionDenied.into()) }
        }
        assert_eq!(hash_reader(Broken,1,&||Ok(())).unwrap_err().code,"capability_pack_invalid");
        let calls=AtomicUsize::new(0);
        let cancelled=hash_reader(Cursor::new(vec![42u8;131072]),131072,&|| {
            if calls.fetch_add(1,Ordering::AcqRel)>0 { Err(ApiError::new(422,"cancelled")) } else { Ok(()) }
        }).unwrap_err();
        assert_eq!(cancelled.code,"cancelled");
        #[cfg(not(windows))]
        assert_eq!(hash_reader(Cursor::new(b"grown"),4,&||Ok(())).unwrap_err().code,"capability_pack_invalid");
    }
}
