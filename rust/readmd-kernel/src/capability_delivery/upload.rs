//! Bounded browser transport for the common capability installer. Uploaded
//! bytes never become executable resources before the shared probe and commit.
use super::*;
use std::{
    io::{Seek, SeekFrom},
    path::PathBuf,
};

pub(crate) const CHUNK_BYTES: usize = 4 * 1024 * 1024;
const IDLE: Duration = Duration::from_secs(15 * 60);
const MARKER: &[u8] = b"ReadMD capability upload v1\n";

pub(super) struct State {
    _transfer: crate::capability_process::TransferReservation,
    directory: PathBuf,
    owner: Option<fs::File>,
    file: Option<fs::File>,
    body_created: bool,
    total: u64,
    received: u64,
    chunks: BTreeMap<u64, (usize, String)>,
    touched: Instant,
}
fn plain(path: &Path, directory: bool) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if metadata.file_type().is_symlink()
        || if directory {
            !metadata.is_dir()
        } else {
            !metadata.is_file()
        }
    {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    true
}
fn root(data: &Path, create: bool) -> ApiResult<PathBuf> {
    let base = data
        .canonicalize()
        .map_err(|_| failure("capability_install_failed"))?;
    let mut directory = base.clone();
    for name in ["capabilities", crate::capability_store::PROTOCOL, "transfers"] {
        directory.push(name);
        if create {
            match fs::create_dir(&directory) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(_) => return Err(failure("capability_install_failed")),
            }
        }
        if !plain(&directory, true) {
            return Err(failure("capability_pack_invalid"));
        }
    }
    let directory = directory
        .canonicalize()
        .map_err(|_| failure("capability_install_failed"))?;
    if !directory.starts_with(base) {
        return Err(failure("capability_pack_invalid"));
    }
    Ok(directory)
}
fn owner(path: &Path, create: bool) -> std::io::Result<fs::File> {
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    return Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "Upload ownership is not implemented for this platform",
    ));
    let mut options = fs::OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create_new(create)
        .truncate(false);
    // A live owner cannot be opened by a second uploader/cleanup process.
    // Do not allow its marker to be renamed/unlinked while the owner is live.
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options
            .share_mode(0)
            .custom_flags(windows::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT.0);
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        use std::os::unix::fs::OpenOptionsExt;
        #[cfg(target_os = "linux")]
        const NOFOLLOW: i32 = 0x20000;
        #[cfg(target_os = "macos")]
        const NOFOLLOW: i32 = 0x100;
        options.custom_flags(NOFOLLOW);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Invalid upload owner marker",
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid upload owner marker",
            ));
        }
    }
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        unsafe extern "C" {
            fn flock(fd: std::ffi::c_int, operation: std::ffi::c_int) -> std::ffi::c_int;
        }
        // LOCK_EX | LOCK_NB; macOS/Linux flock is held by the open description
        // and released when the last descriptor is closed. No extra runtime.
        loop {
            if unsafe { flock(file.as_raw_fd(), 2 | 4) } == 0 {
                break;
            }
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }
    Ok(file)
}
fn remove_plain(path: &Path) {
    if plain(path, false) {
        let _ = fs::remove_file(path);
    }
}
impl Drop for State {
    fn drop(&mut self) {
        drop(self.file.take());
        if !plain(&self.directory, true)
            || self.directory.canonicalize().ok().as_ref() != Some(&self.directory)
            || !self
                .directory
                .parent()
                .is_some_and(|p| plain(p, true) && p.parent().is_some_and(|p| plain(p, true)))
        {
            return;
        }
        if self.owner.is_some() {
            if self.body_created {
                remove_plain(&self.directory.join("body.readmd-pack"));
            }
            // Release only after the payload is gone. Another reaper can then
            // finish the same marker; upload UUIDs are never reused.
            drop(self.owner.take());
            remove_plain(&self.directory.join("owner.lock"));
        }
        // Never recursively remove files a user added to a former task directory.
        let _ = fs::remove_dir(&self.directory);
    }
}
impl State {
    fn create(data: &Path, total: u64) -> ApiResult<Self> {
        // Reserve before creating any transfer directory or payload. Keep it
        // through upload_finish until the worker drops its last State handle.
        let transfer = crate::capability_process::reserve_transfer(data, total)?;
        let directory = root(data, true)?.join(format!("upload-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).map_err(|_| failure("capability_install_failed"))?;
        let mut state = Self {
            _transfer: transfer,
            directory,
            owner: None,
            file: None,
            body_created: false,
            total,
            received: 0,
            chunks: BTreeMap::new(),
            touched: Instant::now(),
        };
        state.owner = Some(
            owner(&state.directory.join("owner.lock"), true)
                .map_err(|_| failure("capability_install_failed"))?,
        );
        let lock = state.owner.as_mut().unwrap();
        lock.write_all(MARKER)
            .and_then(|_| lock.sync_all())
            .map_err(|_| failure("capability_install_failed"))?;
        state.create_body()?;
        Ok(state)
    }
    fn create_body(&mut self) -> ApiResult<()> {
        self.file = Some(
            fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(self.directory.join("body.readmd-pack"))
                .map_err(|_| failure("capability_install_failed"))?,
        );
        self.body_created = true;
        Ok(())
    }
    fn append(&mut self, offset: u64, bytes: &[u8], digest: &str) -> ApiResult<u64> {
        if bytes.is_empty()
            || bytes.len() > CHUNK_BYTES
            || offset % CHUNK_BYTES as u64 != 0
            || offset >= self.total
            || bytes.len() as u64 != (self.total - offset).min(CHUNK_BYTES as u64)
        {
            return Err(failure("capability_pack_invalid"));
        }
        if digest.len() != 64 || format!("{:x}", Sha256::digest(bytes)) != digest {
            return Err(failure("capability_download_integrity"));
        }
        if offset < self.received {
            if self.chunks.get(&offset) != Some(&(bytes.len(), digest.to_string())) {
                return Err(failure("capability_download_integrity"));
            }
            self.touched = Instant::now();
            return Ok(self.received);
        }
        if offset != self.received {
            return Err(failure("capability_pack_invalid"));
        }
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| failure("capability_task_missing"))?;
        if file
            .metadata()
            .map_err(|_| failure("capability_install_failed"))?
            .len()
            != self.received
        {
            return Err(failure("capability_download_integrity"));
        }
        file.seek(SeekFrom::Start(offset))
            .and_then(|_| file.write_all(bytes))
            .map_err(|_| failure("capability_install_failed"))?;
        self.received += bytes.len() as u64;
        self.chunks
            .insert(offset, (bytes.len(), digest.to_string()));
        self.touched = Instant::now();
        Ok(self.received)
    }
    fn finalize(&mut self) -> ApiResult<PathBuf> {
        if self.received != self.total {
            return Err(failure("capability_pack_invalid"));
        }
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| failure("capability_task_missing"))?;
        if file
            .metadata()
            .map_err(|_| failure("capability_install_failed"))?
            .len()
            != self.total
        {
            return Err(failure("capability_download_integrity"));
        }
        file.sync_all()
            .map_err(|_| failure("capability_install_failed"))?;
        drop(self.file.take());
        Ok(self.directory.join("body.readmd-pack"))
    }
}
fn token_matches(expected: &str, actual: &str) -> bool {
    expected.len() == actual.len()
        && expected
            .as_bytes()
            .iter()
            .zip(actual.as_bytes())
            .fold(0u8, |n, (a, b)| n | (a ^ b))
            == 0
}
fn authorized<'a>(
    map: &'a mut BTreeMap<String, Job>,
    app: &App,
    id: &str,
    token: &str,
) -> ApiResult<&'a mut Job> {
    let job = map
        .get_mut(id)
        .filter(|j| {
            j.scope == scope(app)
                && j.upload_identity
                    .as_ref()
                    .is_some_and(|(_, secret)| token_matches(secret, token))
        })
        .ok_or_else(|| failure("capability_task_missing"))?;
    Ok(job)
}
pub(super) fn cleanup(data: &Path) {
    let Ok(directory) = root(data, false) else {
        return;
    };
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten().take(4096) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name
            .strip_prefix("upload-")
            .is_some_and(|s| uuid::Uuid::parse_str(s).is_ok_and(|id| id.to_string() == s))
        {
            continue;
        }
        let directory = entry.path();
        if !plain(&directory, true) {
            continue;
        }
        let marker = directory.join("owner.lock");
        if !plain(&marker, false) {
            // This may be the interval between mkdir and create_new(owner),
            // or a user's directory. A UUID-shaped name is not ownership.
            continue;
        }
        // This lock, rather than a PID or timeout guess, proves that no live
        // transfer owns the directory. Unknown/reparse entries are preserved.
        let Ok(mut owner) = owner(&marker, false) else {
            continue;
        };
        if !plain(&directory, true)
            || directory.canonicalize().ok().as_ref() != Some(&directory)
            || !directory
                .parent()
                .is_some_and(|p| plain(p, true) && p.parent().is_some_and(|p| plain(p, true)))
        {
            continue;
        }
        if owner
            .metadata()
            .is_ok_and(|m| m.len() == MARKER.len() as u64)
        {
            let mut bytes = Vec::new();
            if (&mut owner)
                .take(MARKER.len() as u64 + 1)
                .read_to_end(&mut bytes)
                .is_ok()
                && bytes == MARKER
            {
                remove_plain(&directory.join("body.readmd-pack"));
                drop(owner);
                remove_plain(&marker);
                let _ = fs::remove_dir(directory);
            }
        }
    }
}
fn expire(now: Instant) {
    let mut retired = Vec::new();
    let mut map = jobs().lock().unwrap_or_else(|e| e.into_inner());
    for (id, job) in map.iter_mut() {
        let idle = job.phase == "uploading"
            && job.upload.as_ref().is_some_and(|state| {
                // A queued chunk/finalizer may hold an Arc before locking it.
                // Do not retire its state merely because try_lock is free.
                Arc::strong_count(state) == 1 && state
                    .try_lock()
                    .is_ok_and(|state| now.saturating_duration_since(state.touched) >= IDLE)
            });
        if idle {
            job.cancel = true;
            job.phase = "cancelling";
            retired.push((id.clone(), job.upload.take()));
        }
    }
    drop(map);
    for (id, state) in retired {
        // No file I/O under the global task lock. Publish terminal status only
        // after the last owned state releases handles and transfer capacity.
        drop(state);
        let mut map = jobs().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(job) = map.get_mut(&id).filter(|j| j.phase=="cancelling" && j.cancel && j.upload.is_none()) {
            job.phase = "error";
            job.error = Some("capability_task_missing".into());
        }
    }
}
fn reaper() {
    static START: OnceLock<()> = OnceLock::new();
    START.get_or_init(|| {
        let _ = std::thread::Builder::new()
            .name("readmd-upload-expiry".into())
            .spawn(|| loop {
                std::thread::sleep(Duration::from_secs(30));
                expire(Instant::now());
            });
    });
}
pub(super) fn endpoint(app: &Arc<App>, v: &Value) -> ApiResult<Response> {
    if v["action"] == "upload_finish" {
        return complete(app, v);
    }
    if v["action"] != "upload_start" {
        return Err(failure("capability_pack_invalid"));
    }
    let engine = v["id"].as_str().unwrap_or("");
    let entry = checked_entry(&app.paths.assets_dir, engine)?;
    let total = v["total"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= crate::capability_archive::MAX_DOWNLOAD)
        .ok_or_else(|| failure("capability_pack_invalid"))?;
    match v["format"].as_str() {
        Some("stream-v2") => (),
        Some("json-v1") if total <= MAX_BYTES => (),
        _ => return Err(failure("capability_pack_invalid")),
    }
    if entry["format"] == "stream-v2" && entry["downloadBytes"].as_u64() != Some(total) {
        return Err(failure("capability_download_integrity"));
    }
    let key = v["request_key"]
        .as_str()
        .filter(|s| uuid::Uuid::parse_str(s).is_ok_and(|id| id.to_string() == *s))
        .ok_or_else(|| failure("capability_pack_invalid"))?;
    if let Some(code) = crate::capability_packs::busy_code(app, engine)? {
        return Err(ApiError::new(409, code));
    }
    expire(Instant::now());
    let owned = scope(app);
    let mut map = jobs().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(job) = map
        .values()
        .find(|j| j.scope == owned && j.engine == engine && active(j.phase))
    {
        if job.phase == "uploading"
            && job.total == total
            && job
                .upload_identity
                .as_ref()
                .is_some_and(|(request, _)| request == key)
        {
            let mut value = job.value();
            value["upload_token"] = json!(job.upload_identity.as_ref().unwrap().1);
            value["chunk_bytes"] = json!(CHUNK_BYTES);
            return ok_json(value);
        }
        return Err(ApiError::new(409, "capability_install_busy"));
    }
    let pending = map
        .values()
        .filter(|j| j.scope == owned && j.upload_identity.is_some() && active(j.phase))
        .collect::<Vec<_>>();
    if pending.len() >= 4
        || pending
            .iter()
            .map(|j| j.total)
            .sum::<u64>()
            .saturating_add(total)
            > crate::capability_archive::MAX_DOWNLOAD
    {
        return Err(ApiError::new(409, "capability_install_busy"));
    }
    if map.len() >= 64 {
        if let Some(id) = map
            .iter()
            .find(|(_, j)| !active(j.phase))
            .map(|(id, _)| id.clone())
        {
            map.remove(&id);
        } else {
            return Err(ApiError::new(409, "capability_install_busy"));
        }
    }
    let state = State::create(&app.paths.data_dir, total)?;
    let secret = uuid::Uuid::new_v4().simple().to_string();
    let job = Job {
        id: uuid::Uuid::new_v4().to_string(),
        engine: engine.into(),
        scope: owned,
        phase: "uploading",
        total,
        upload: Some(Arc::new(Mutex::new(state))),
        upload_identity: Some((key.to_string(), secret.clone())),
        ..Job::default()
    };
    let mut value = job.value();
    value["upload_token"] = json!(secret);
    value["chunk_bytes"] = json!(CHUNK_BYTES);
    map.insert(job.id.clone(), job);
    drop(map);
    reaper();
    ok_json(value)
}
pub(super) fn chunk(app: &Arc<App>, req: &crate::server::Request) -> ApiResult<Response> {
    if req.method != "POST" {
        return Err(ApiError::new(405, "method_not_allowed"));
    }
    if req.body.len() > CHUNK_BYTES {
        return Err(failure("capability_pack_invalid"));
    }
    let id = req.q("job").unwrap_or("");
    let secret = req.header("x-readmd-upload-token").unwrap_or("");
    let offset = req
        .q("offset")
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| failure("capability_pack_invalid"))?;
    let state = {
        let mut map = jobs().lock().unwrap_or_else(|e| e.into_inner());
        let job = authorized(&mut map, app, id, secret)?;
        if job.cancel {
            return Err(failure("cancelled"));
        }
        if job.phase != "uploading" {
            return Err(failure("capability_task_missing"));
        }
        job.upload
            .clone()
            .ok_or_else(|| failure("capability_task_missing"))?
    };
    let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
    {
        let mut map = jobs().lock().unwrap_or_else(|e| e.into_inner());
        if authorized(&mut map, app, id, secret)?.cancel {
            return Err(failure("cancelled"));
        }
    }
    let received = match state.append(
        offset,
        &req.body,
        req.header("x-readmd-chunk-sha256").unwrap_or(""),
    ) {
        Ok(received) => received,
        Err(error) => {
            if error.code == "capability_install_failed" {
                drop(state);
                let mut map = jobs().lock().unwrap_or_else(|e| e.into_inner());
                if let Ok(job) = authorized(&mut map, app, id, secret) {
                    job.phase = "error";
                    job.error = Some(error.code.clone());
                    job.upload.take();
                }
            }
            return Err(error);
        }
    };
    let mut map = jobs().lock().unwrap_or_else(|e| e.into_inner());
    let job = authorized(&mut map, app, id, secret)?;
    if job.cancel {
        return Err(failure("cancelled"));
    }
    job.received = received;
    ok_json(job.value())
}
fn complete(app: &Arc<App>, v: &Value) -> ApiResult<Response> {
    let id = v["job"].as_str().unwrap_or("");
    let secret = v["upload_token"].as_str().unwrap_or("");
    let state = {
        let mut map = jobs().lock().unwrap_or_else(|e| e.into_inner());
        let job = authorized(&mut map, app, id, secret)?;
        if job.phase != "uploading" {
            return ok_json(job.value());
        }
        job.upload
            .clone()
            .ok_or_else(|| failure("capability_task_missing"))?
    };
    let mut state_guard = state.lock().unwrap_or_else(|e| e.into_inner());
    {
        let mut map = jobs().lock().unwrap_or_else(|e| e.into_inner());
        let job = authorized(&mut map, app, id, secret)?;
        if job.phase != "uploading" {
            return ok_json(job.value());
        }
        if job.cancel {
            return Err(failure("cancelled"));
        }
    }
    let path = state_guard.finalize()?;
    let mut map = jobs().lock().unwrap_or_else(|e| e.into_inner());
    let job = authorized(&mut map, app, id, secret)?;
    if job.phase != "uploading" {
        return ok_json(job.value());
    }
    if job.cancel {
        return Err(failure("cancelled"));
    }
    job.phase = "extracting";
    job.upload.take();
    let result = job.value();
    let engine = job.engine.clone();
    drop(map);
    drop(state_guard);
    let app = app.clone();
    let owned = id.to_string();
    let worker_id = owned.clone();
    if std::thread::Builder::new()
        .name("readmd-upload-install".into())
        .spawn(move || {
            let result = install(&app, &path, &engine, &worker_id);
            drop(state);
            finish(&worker_id, result);
        })
        .is_err()
    {
        finish(&owned, Err("capability_install_failed".into()));
        return Err(failure("capability_install_failed"));
    }
    ok_json(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn global_budget_refuses_before_upload_creation_and_is_shared_with_other_transports() {
        let data=tempfile::tempdir().unwrap();
        let all=crate::capability_process::reserve_transfer(data.path(),crate::capability_archive::MAX_DOWNLOAD).unwrap();
        assert_eq!(State::create(data.path(),7).err().unwrap().code,"capability_install_busy");
        assert!(!data.path().join("capabilities/managed-v2/transfers").exists());drop(all);
        let mut states=Vec::new();for _ in 0..4 { states.push(State::create(data.path(),7).unwrap()); }
        assert_eq!(crate::capability_process::reserve_transfer(data.path(),1).err().unwrap().code,"capability_install_busy");
        states.pop();let other=crate::capability_process::reserve_transfer(data.path(),1).unwrap();
        assert_eq!(State::create(data.path(),7).err().unwrap().code,"capability_install_busy");
        drop((states,other));
        drop(crate::capability_process::reserve_transfer(data.path(),crate::capability_archive::MAX_DOWNLOAD).unwrap());
    }
    #[test]
    fn idempotent_upload_start_does_not_reserve_twice_and_cancel_releases_capacity() {
        let data=tempfile::tempdir().unwrap();let app=make_app(data.path());
        let body=json!({"action":"upload_start","id":"fonts","format":"json-v1","total":7,"request_key":uuid::Uuid::new_v4().to_string()});
        let first:Value=serde_json::from_slice(&endpoint(&app,&body).unwrap().body).unwrap();
        for _ in 0..8 {
            let again:Value=serde_json::from_slice(&endpoint(&app,&body).unwrap().body).unwrap();
            assert_eq!(again["job"],first["job"]);assert_eq!(again["upload_token"],first["upload_token"]);
        }
        let remainder=crate::capability_process::reserve_transfer(data.path(),crate::capability_archive::MAX_DOWNLOAD-64*1024*1024).unwrap();
        assert_eq!(crate::capability_process::reserve_transfer(data.path(),1).err().unwrap().code,"capability_install_busy");
        super::super::endpoint(&app,&json!({"action":"cancel_download","job":first["job"]})).unwrap();
        drop(crate::capability_process::reserve_transfer(data.path(),1).unwrap());drop(remainder);
        let retired=jobs().lock().unwrap().remove(first["job"].as_str().unwrap());drop(retired);
        drop(crate::capability_process::reserve_transfer(data.path(),crate::capability_archive::MAX_DOWNLOAD).unwrap());
    }
    fn sha(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }
    fn make_app(data: &Path) -> Arc<App> {
        let docs = data.join("documents");
        fs::create_dir_all(&docs).unwrap();
        Arc::new(
            App::bootstrap(crate::paths::AppPaths::with_dirs(
                data,
                &docs,
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../assets")
                    .as_path(),
            ))
            .unwrap(),
        )
    }
    fn register(app: &App, state: State) -> (String, String, PathBuf) {
        let id = uuid::Uuid::new_v4().to_string();
        let secret = uuid::Uuid::new_v4().simple().to_string();
        let directory = state.directory.clone();
        let job = Job {
            id: id.clone(),
            scope: scope(app),
            engine: "typst".into(),
            phase: "uploading",
            total: state.total,
            upload: Some(Arc::new(Mutex::new(state))),
            upload_identity: Some((uuid::Uuid::new_v4().to_string(), secret.clone())),
            ..Job::default()
        };
        jobs().lock().unwrap().insert(id.clone(), job);
        (id, secret, directory)
    }
    fn request(id: &str, secret: &str, bytes: &[u8]) -> crate::server::Request {
        crate::server::Request {
            method: "POST".into(),
            path: "/api/capabilities/upload/chunk".into(),
            query: std::collections::HashMap::from([
                ("job".into(), id.into()),
                ("offset".into(), "0".into()),
            ]),
            headers: std::collections::HashMap::from([
                ("x-readmd-upload-token".into(), secret.into()),
                ("x-readmd-chunk-sha256".into(), sha(bytes)),
            ]),
            body: bytes.to_vec(),
        }
    }
    #[test]
    fn active_installation_keeps_its_upload_reservation_until_terminal() {
        let data = tempfile::tempdir().unwrap();
        let app = make_app(data.path());
        let (id, _, _) = register(&app, State::create(data.path(), 7).unwrap());
        {
            let mut map = jobs().lock().unwrap();
            let job = map.get_mut(&id).unwrap();
            job.phase = "extracting";
            // The real worker owns the stage after upload_finish; the job's
            // transport identity and reservation survive until it terminates.
            job.upload.take();
        }
        let body = json!({"action":"upload_start","id":"ocr","format":"stream-v2",
            "total":crate::capability_archive::MAX_DOWNLOAD-6,
            "request_key":uuid::Uuid::new_v4().to_string()});
        assert_eq!(
            endpoint(&app, &body).err().unwrap().code,
            "capability_install_busy"
        );
        jobs().lock().unwrap().get_mut(&id).unwrap().phase = "ready";
        assert!(endpoint(&app, &body).is_ok());
        let retired = {
            let mut map = jobs().lock().unwrap();
            let keys = map
                .iter()
                .filter(|(_, j)| j.scope == scope(&app))
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            keys.into_iter()
                .filter_map(|id| map.remove(&id))
                .collect::<Vec<_>>()
        };
        drop(retired);
    }
    #[test]
    fn ordered_chunks_digest_duplicate_and_incomplete_finish_are_enforced() {
        let data = tempfile::tempdir().unwrap();
        let total = CHUNK_BYTES as u64 + 17;
        let mut state = State::create(data.path(), total).unwrap();
        let directory = state.directory.clone();
        let bytes = vec![71; CHUNK_BYTES];
        let digest = sha(&bytes);
        assert!(state.finalize().is_err());
        assert!(state
            .append(CHUNK_BYTES as u64, &[3; 17], &sha(&[3; 17]))
            .is_err());
        assert!(state.append(0, &bytes, &"a".repeat(64)).is_err());
        assert_eq!(
            fs::metadata(directory.join("body.readmd-pack"))
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            state.append(0, &bytes, &digest).unwrap(),
            CHUNK_BYTES as u64
        );
        assert_eq!(
            state.append(0, &bytes, &digest).unwrap(),
            CHUNK_BYTES as u64
        );
        assert_eq!(
            fs::metadata(directory.join("body.readmd-pack"))
                .unwrap()
                .len(),
            CHUNK_BYTES as u64
        );
        let changed = vec![72; CHUNK_BYTES];
        assert!(state.append(0, &changed, &sha(&changed)).is_err());
        assert!(state.finalize().is_err());
        assert_eq!(
            state
                .append(CHUNK_BYTES as u64, &[3; 17], &sha(&[3; 17]))
                .unwrap(),
            total
        );
        let file = state.finalize().unwrap();
        assert_eq!(fs::metadata(&file).unwrap().len(), total);
        let saved = fs::read(file).unwrap();
        assert_eq!(&saved[..CHUNK_BYTES], bytes.as_slice());
        assert_eq!(&saved[CHUNK_BYTES..], &[3; 17]);
        drop(state);
        assert!(!directory.exists());
    }
    #[test]
    fn live_owner_is_not_reaped_orphan_cleanup_preserves_unknown_files() {
        let data = tempfile::tempdir().unwrap();
        let mut state = State::create(data.path(), 7).unwrap();
        let directory = state.directory.clone();
        state.append(0, b"fixture", &sha(b"fixture")).unwrap();
        cleanup(data.path());
        assert!(directory.join("body.readmd-pack").exists());
        let user = directory.join("user-note.txt");
        fs::write(&user, b"Keep exactly here").unwrap();
        drop(state.file.take());
        drop(state.owner.take());
        cleanup(data.path());
        assert!(!directory.join("body.readmd-pack").exists());
        assert!(!directory.join("owner.lock").exists());
        assert_eq!(fs::read(&user).unwrap(), b"Keep exactly here");
        drop(state);
        assert!(user.is_file());
        let foreign = root(data.path(), true)
            .unwrap()
            .join(format!("upload-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&foreign).unwrap();
        fs::write(foreign.join("owner.lock"), b"unrecognized marker").unwrap();
        fs::write(foreign.join("body.readmd-pack"), b"keep").unwrap();
        cleanup(data.path());
        assert_eq!(fs::read(foreign.join("body.readmd-pack")).unwrap(), b"keep");
    }
    #[test]
    fn unmarked_or_damaged_uploads_are_not_ownership_proof() {
        let data = tempfile::tempdir().unwrap();
        let parent = root(data.path(), true).unwrap();
        let unmarked = parent.join(format!("upload-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&unmarked).unwrap();
        cleanup(data.path());
        assert!(unmarked.is_dir());
        // Startup must not remove the mkdir-before-marker interval of a live
        // creator, nor infer ownership merely from an empty UUID directory.
        let mut guard = owner(&unmarked.join("owner.lock"), true).unwrap();
        guard.write_all(MARKER).unwrap();
        cleanup(data.path());
        assert!(unmarked.join("owner.lock").is_file());
        drop(guard);
        let damaged = parent.join(format!("upload-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&damaged).unwrap();
        fs::write(damaged.join("owner.lock"), &MARKER[..MARKER.len() - 1]).unwrap();
        fs::write(damaged.join("body.readmd-pack"), b"Unrecognized payload").unwrap();
        cleanup(data.path());
        assert!(!unmarked.exists());
        assert_eq!(
            fs::read(damaged.join("body.readmd-pack")).unwrap(),
            b"Unrecognized payload"
        );
        assert_eq!(
            fs::read(damaged.join("owner.lock")).unwrap(),
            &MARKER[..MARKER.len() - 1]
        );
    }
    #[test]
    fn failed_payload_creation_does_not_delete_a_foreign_file() {
        let data = tempfile::tempdir().unwrap();
        let directory = root(data.path(), true)
            .unwrap()
            .join(format!("upload-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let mut guard = owner(&directory.join("owner.lock"), true).unwrap();
        guard.write_all(MARKER).unwrap();
        let body = directory.join("body.readmd-pack");
        fs::write(&body, b"Foreign file before create_new").unwrap();
        let mut state = State {
            _transfer: crate::capability_process::reserve_transfer(data.path(), 7).unwrap(),
            directory: directory.clone(),
            owner: Some(guard),
            file: None,
            body_created: false,
            total: 7,
            received: 0,
            chunks: BTreeMap::new(),
            touched: Instant::now(),
        };
        assert!(state.create_body().is_err());
        drop(state);
        assert_eq!(fs::read(body).unwrap(), b"Foreign file before create_new");
        assert!(!directory.join("owner.lock").exists());
    }
    #[cfg(windows)]
    #[test]
    fn live_upload_owner_marker_cannot_be_renamed_or_unlinked() {
        let data = tempfile::tempdir().unwrap();
        let state = State::create(data.path(), 7).unwrap();
        let directory = state.directory.clone();
        let marker = directory.join("owner.lock");
        assert!(fs::rename(&marker, directory.join("moved.lock")).is_err());
        assert!(fs::remove_file(&marker).is_err());
        assert!(owner(&marker, false).is_err());
        cleanup(data.path());
        assert!(marker.exists());
        drop(state);
        assert!(!directory.exists());
    }
    #[cfg(unix)]
    #[test]
    fn upload_owner_does_not_follow_a_symbolic_link() {
        let data = tempfile::tempdir().unwrap();
        let target = data.path().join("outside-marker");
        fs::write(&target, MARKER).unwrap();
        let link = data.path().join("redirected-marker");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(owner(&link, false).is_err());
        assert_eq!(fs::read(target).unwrap(), MARKER);
    }
    #[test]
    fn cross_process_upload_owner_and_orphan_cleanup() {
        use std::process::{Child, Stdio};
        const TEST: &str =
            "capability_delivery::upload::tests::cross_process_upload_owner_and_orphan_cleanup";
        struct Owned(Child);
        impl Drop for Owned {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        if let Some(data) = std::env::var_os("READMD_UPLOAD_OWNER_TEST_ROOT") {
            let data = PathBuf::from(data);
            let mode = std::env::var("READMD_UPLOAD_OWNER_TEST_MODE").unwrap();
            let state = if mode == "owner" {
                let mut state = State::create(&data, 7).unwrap();
                state.append(0, b"fixture", &sha(b"fixture")).unwrap();
                fs::write(
                    state.directory.join("user-note.txt"),
                    b"Synthetic unknown file",
                )
                .unwrap();
                Some(state)
            } else {
                assert_eq!(mode, "idle");
                None
            };
            let name = state.as_ref().map(|state| {
                state
                    .directory
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            });
            fs::write(data.join("ready.json"), serde_json::to_vec(&name).unwrap()).unwrap();
            fs::write(data.join("ready.signal"), b"ready").unwrap();
            let deadline = Instant::now() + Duration::from_secs(30);
            while !data.join("stop.signal").is_file() {
                assert!(
                    Instant::now() < deadline,
                    "Parent did not stop upload helper"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            drop(state);
            return;
        }
        fn launch(data: &Path, mode: &str) -> Owned {
            let mut child = Owned(
                crate::silent_command(std::env::current_exe().unwrap())
                    .args(["--exact", TEST, "--test-threads=1"])
                    .env("READMD_UPLOAD_OWNER_TEST_ROOT", data)
                    .env("READMD_UPLOAD_OWNER_TEST_MODE", mode)
                    .stdout(Stdio::null())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .unwrap(),
            );
            let deadline = Instant::now() + Duration::from_secs(10);
            while !data.join("ready.signal").is_file() {
                assert!(
                    child.0.try_wait().unwrap().is_none(),
                    "Upload helper exited before ready"
                );
                assert!(Instant::now() < deadline, "Upload helper ready deadline");
                std::thread::sleep(Duration::from_millis(10));
            }
            child
        }
        let data = tempfile::tempdir().unwrap();
        let mut child = launch(data.path(), "owner");
        let name: Option<String> =
            serde_json::from_slice(&fs::read(data.path().join("ready.json")).unwrap()).unwrap();
        let name = name.unwrap();
        assert!(name
            .strip_prefix("upload-")
            .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok()));
        let directory = root(data.path(), false).unwrap().join(name);
        cleanup(data.path());
        assert_eq!(
            fs::read(directory.join("body.readmd-pack")).unwrap(),
            b"fixture"
        );
        assert!(directory.join("owner.lock").is_file());
        child.0.kill().unwrap();
        assert!(!child.0.wait().unwrap().success());
        cleanup(data.path());
        assert!(!directory.join("body.readmd-pack").exists());
        assert!(!directory.join("owner.lock").exists());
        assert_eq!(
            fs::read(directory.join("user-note.txt")).unwrap(),
            b"Synthetic unknown file"
        );
        let mut replacement = State::create(data.path(), 7).unwrap();
        replacement.append(0, b"fixture", &sha(b"fixture")).unwrap();
        assert_ne!(replacement.directory, directory);
        cleanup(data.path());
        assert_eq!(
            fs::read(replacement.directory.join("body.readmd-pack")).unwrap(),
            b"fixture"
        );
        drop(replacement);
        // A still-live unrelated child must not inherit its parent's marker or
        // payload handles and pin normal Drop cleanup after the parent closes.
        let second = tempfile::tempdir().unwrap();
        let state = State::create(second.path(), 7).unwrap();
        let directory = state.directory.clone();
        let mut idle = launch(second.path(), "idle");
        drop(state);
        assert!(!directory.exists());
        assert!(idle.0.try_wait().unwrap().is_none());
        fs::write(second.path().join("stop.signal"), b"stop").unwrap();
        assert!(idle.0.wait().unwrap().success());
    }
    #[test]
    fn token_and_profile_authorization_precede_writes_and_cancel_removes_only_owned_stage() {
        let data = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let app = make_app(data.path());
        let second = make_app(other.path());
        let (id, secret, directory) = register(&app, State::create(data.path(), 7).unwrap());
        assert_eq!(
            chunk(&app, &request(&id, "wrong", b"fixture"))
                .err()
                .unwrap()
                .code,
            "capability_task_missing"
        );
        assert_eq!(
            chunk(&second, &request(&id, &secret, b"fixture"))
                .err()
                .unwrap()
                .code,
            "capability_task_missing"
        );
        assert_eq!(
            fs::metadata(directory.join("body.readmd-pack"))
                .unwrap()
                .len(),
            0
        );
        chunk(&app, &request(&id, &secret, b"fixture")).unwrap();
        super::super::endpoint(&app, &json!({"action":"cancel_download","job":id})).unwrap();
        assert!(!directory.exists());
        assert_eq!(
            chunk(&app, &request(&id, &secret, b"fixture"))
                .err()
                .unwrap()
                .code,
            "cancelled"
        );
        let response = complete(&app, &json!({"job":id,"upload_token":secret})).unwrap();
        let value: Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(value["phase"], "cancelled");
        jobs().lock().unwrap().remove(&id);
    }
    #[test]
    fn idle_session_expires_without_committing_or_exposing_upload_secret() {
        let data = tempfile::tempdir().unwrap();
        let app = make_app(data.path());
        let mut state = State::create(data.path(), 7).unwrap();
        state.touched = Instant::now()
            .checked_sub(IDLE + Duration::from_secs(1))
            .unwrap();
        let (id, secret, directory) = register(&app, state);
        expire(Instant::now());
        // Another real expiry worker can already own this retirement. Observe
        // its one execution reaching terminal status, rather than racing Drop.
        let until = Instant::now() + Duration::from_secs(2);
        let job = loop {
            let mut map = jobs().lock().unwrap();
            if map.get(&id).is_some_and(|j|j.phase=="error") { break map.remove(&id).unwrap(); }
            drop(map);
            assert!(Instant::now()<until,"Expired upload did not finish cleanup");
            std::thread::yield_now();
        };
        assert_eq!(job.phase, "error");
        assert_eq!(job.error.as_deref(), Some("capability_task_missing"));
        assert!(job.cancel);
        assert!(job.upload.is_none());
        assert!(!directory.exists());
        assert!(!job.value().to_string().contains(&secret));
    }
    #[test]
    fn idle_expiry_does_not_retire_a_queued_request_with_a_live_state_reference() {
        let data = tempfile::tempdir().unwrap(); let app = make_app(data.path());
        let state = State::create(data.path(), 7).unwrap();
        let (id, _, directory) = register(&app, state);
        let queued = {
            let map=jobs().lock().unwrap();
            let queued=map.get(&id).unwrap().upload.clone().unwrap();
            // Acquire the real queued reference before making the fixture
            // idle; another expiry worker must not race fixture preparation.
            queued.lock().unwrap().touched=Instant::now().checked_sub(IDLE+Duration::from_secs(1)).unwrap();
            queued
        };
        expire(Instant::now());
        assert_eq!(jobs().lock().unwrap().get(&id).unwrap().phase,"uploading");
        assert!(directory.join("body.readmd-pack").exists());
        // Retire once after the queued request finishes; do not re-run an
        // operation that failed. This exercises the intended state transition.
        drop(queued);
        expire(Instant::now());
        let until = Instant::now()+Duration::from_secs(2);
        loop {
            let map=jobs().lock().unwrap();
            if map.get(&id).is_some_and(|j|j.phase=="error") { break; }
            drop(map); assert!(Instant::now()<until); std::thread::yield_now();
        }
        let job=jobs().lock().unwrap().remove(&id).unwrap();
        assert!(job.upload.is_none()); assert!(!directory.exists());
    }
}
