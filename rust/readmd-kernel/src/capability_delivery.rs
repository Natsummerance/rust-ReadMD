//! Owned, pinned capability delivery. Never executes a package script or
//! searches PATH for an engine. Only the app's reviewed catalog selects bytes.
use crate::{
    server::{ok_json, Response},
    ApiError, ApiResult, App,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::Path,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

const MAX_BYTES: u64 = 128 * 1024 * 1024;
const REPOSITORY: &str = "Natsummerance/tool-ReadMD";
mod upload;
pub(crate) const UPLOAD_CHUNK_BYTES: usize = upload::CHUNK_BYTES;
#[derive(Clone, Default)]
struct Job {
    id: String,
    engine: String,
    scope: String,
    phase: &'static str,
    received: u64,
    total: u64,
    progress: Option<crate::capability_archive::Progress>,
    error: Option<String>,
    cancel: bool,
    upload: Option<Arc<Mutex<upload::State>>>,
    upload_identity: Option<(String, String)>,
    uninstall: bool,
    result: Option<Value>,
}
impl Job {
    fn value(&self) -> Value {
        json!({"ok":true,"job":self.id,"id":self.engine,"operation":if self.uninstall {"uninstall"} else {"install"},"phase":self.phase,"received":self.received,"total":self.total,"files_done":self.progress.map(|p|p.files_done),"files_total":self.progress.map(|p|p.files_total),"expanded_bytes":self.progress.map(|p|p.expanded_bytes),"expanded_total":self.progress.map(|p|p.expanded_total),"error_code":self.error,"result":self.result})
    }
}
fn jobs() -> &'static Mutex<BTreeMap<String, Job>> {
    static JOBS: OnceLock<Mutex<BTreeMap<String, Job>>> = OnceLock::new();
    JOBS.get_or_init(|| Mutex::new(BTreeMap::new()))
}
fn failure(code: &str) -> ApiError {
    ApiError::new(422, code)
}
fn catalog_at(assets: &Path) -> ApiResult<Value> {
    let file = assets.join("capabilities/catalog.json");
    if fs::metadata(&file)
        .map_err(|_| failure("capability_catalog_invalid"))?
        .len()
        > 65536
    {
        return Err(failure("capability_catalog_invalid"));
    }
    let catalog: Value =
        serde_json::from_slice(&fs::read(file).map_err(|_| failure("capability_catalog_invalid"))?)
            .map_err(|_| failure("capability_catalog_invalid"))?;
    if catalog["schema"] != 1 || catalog["repository"] != REPOSITORY || !catalog["packs"].is_array()
    {
        return Err(failure("capability_catalog_invalid"));
    }
    Ok(catalog)
}
pub(crate) fn checked_entry(assets: &Path, id: &str) -> ApiResult<Value> {
    let catalog = catalog_at(assets)?;
    let entry = catalog["packs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| {
            p["id"] == id
                && (p["platform"] == crate::capability_packs::platform()
                    || p["platform"] == "universal")
        })
        .ok_or_else(|| failure("capability_pack_unavailable"))?
        .clone();
    validate_entry(&entry)?;
    let lock = crate::capability_packs::resource_lock(assets, id)
        .map_err(|_| failure("capability_catalog_invalid"))?;
    let size = lock["files"]
        .as_array()
        .ok_or_else(|| failure("capability_catalog_invalid"))?
        .iter()
        .try_fold(0u64, |n, f| n.checked_add(f["size"].as_u64()?))
        .ok_or_else(|| failure("capability_catalog_invalid"))?;
    if size != entry["installedBytes"].as_u64().unwrap_or(0)
        || entry["version"] != lock["version"]
        || entry["platform"] != lock["platform"]
    {
        return Err(failure("capability_catalog_invalid"));
    }
    if lock["schema"] == 2
        && (entry["format"] != "stream-v2"
            || entry["manifestSha256"].as_str()
                != Some(
                    format!(
                        "{:x}",
                        Sha256::digest(
                            serde_json::to_vec(&lock)
                                .map_err(|_| failure("capability_catalog_invalid"))?
                        )
                    )
                    .as_str(),
                ))
    {
        return Err(failure("capability_catalog_invalid"));
    }
    Ok(entry)
}
fn validate_entry(entry: &Value) -> ApiResult<()> {
    let sha = entry["sha256"].as_str().unwrap_or("");
    let url = entry["url"].as_str().unwrap_or("");
    let prefix = format!("https://github.com/{REPOSITORY}/releases/download/");
    let tail = url.strip_prefix(&prefix).unwrap_or("");
    let maximum = match entry["format"].as_str() {
        None | Some("json-v1") => MAX_BYTES,
        Some("stream-v2") => {
            let hash = entry["manifestSha256"].as_str().unwrap_or("");
            if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(failure("capability_catalog_invalid"));
            }
            crate::capability_archive::MAX_DOWNLOAD
        }
        _ => return Err(failure("capability_catalog_invalid")),
    };
    if !matches!(
        entry["id"].as_str(),
        Some("pdfium" | "typst" | "ocr" | "fonts" | "latex")
    ) || !entry["version"].as_str().is_some_and(|s| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
    }) || sha.len() != 64
        || !sha.bytes().all(|b| b.is_ascii_hexdigit())
        || entry["downloadBytes"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= maximum)
            .is_none()
        || tail.split('/').count() != 2
        || tail.split('/').any(|s| {
            s.is_empty()
                || s == "."
                || s == ".."
                || !s
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        })
        || !tail.ends_with(".readmd-pack")
    {
        return Err(failure("capability_catalog_invalid"));
    }
    Ok(())
}
fn scope(app: &App) -> String {
    app.paths.data_dir.to_string_lossy().into_owned()
}
fn update(id: &str, action: impl FnOnce(&mut Job)) {
    if let Some(job) = jobs().lock().unwrap_or_else(|e| e.into_inner()).get_mut(id) {
        action(job);
    }
}
fn is_cancelled(id: &str) -> bool {
    jobs()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(id)
        .is_none_or(|j| j.cancel)
}
fn active(phase: &str) -> bool {
    matches!(
        phase,
        "uploading" | "downloading" | "extracting" | "probing" | "removing" | "committing" | "cancelling"
    )
}
fn cancel(job: &mut Job) -> ApiResult<()> {
    if job.phase == "committing" {
        return Err(ApiError::new(409, "capability_install_busy"));
    }
    if job.phase == "uploading" {
        job.cancel = true;
        job.phase = "cancelled";
    } else if matches!(job.phase, "downloading" | "extracting" | "probing" | "removing") {
        job.cancel = true;
        job.phase = "cancelling";
    }
    Ok(())
}
fn commit(id: &str) -> ApiResult<()> {
    let mut map = jobs().lock().unwrap_or_else(|e| e.into_inner());
    let job = map.get_mut(id).ok_or_else(|| failure("cancelled"))?;
    if job.cancel {
        return Err(failure("cancelled"));
    }
    job.phase = "committing";
    Ok(())
}
fn install(app: &Arc<App>, path: &Path, engine: &str, id: &str) -> Result<(), String> {
    let check = || is_cancelled(id);
    let phase = |phase| {
        update(id, |j| {
            if !j.cancel {
                j.phase = phase;
            }
        })
    };
    let progress = |p| update(id, |j| j.progress = Some(p));
    let committing = || commit(id);
    let hooks = crate::capability_packs::InstallHooks {
        cancelled: &check,
        phase: &phase,
        progress: &progress,
        committing: &committing,
    };
    let owned = id.to_string();
    crate::speech::with_cancel(Arc::new(move || is_cancelled(&owned)), || {
        crate::capability_packs::install_file(app, path, engine, &hooks)
    })
    .map(|_| ())
    .map_err(|e| e.code)
}
fn finish(id: &str, result: Result<(), String>) {
    update(id, |j| match result {
        Ok(()) => j.phase = if j.uninstall { "removed" } else { "ready" },
        Err(code) => {
            // A rollback/recovery failure must remain visible even if the user
            // requested cancellation. "Cancelled" promises intact originals.
            if code == "cancelled" {
                j.phase = "cancelled";
            } else {
                j.phase = "error";
                j.error = Some(code);
            }
        }
    });
}
fn download(app: &Arc<App>, entry: &Value, id: &str, _transfer: crate::capability_process::TransferReservation) -> Result<(), String> {
    let total = entry["downloadBytes"]
        .as_u64()
        .ok_or("capability_catalog_invalid")?;
    // The same proxy logic as application updates; no credential-bearing URL
    // is accepted from the caller or recorded in events/errors.
    let response = crate::updater::update_agent_with_read_timeout(
        Duration::from_secs(600),
        5,
        Duration::from_secs(2),
    )
    .get(entry["url"].as_str().ok_or("capability_catalog_invalid")?)
    .call()
    .map_err(|_| "capability_download_failed")?;
    let host = response
        .get_url()
        .strip_prefix("https://")
        .and_then(|url| url.split('/').next());
    if !matches!(
        host,
        Some(
            "github.com" | "release-assets.githubusercontent.com" | "objects.githubusercontent.com"
        )
    ) {
        return Err("capability_download_failed".into());
    }
    if response
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
        .is_some_and(|size| size != total)
    {
        return Err("capability_download_integrity".into());
    }
    let temporary = app.paths.data_dir.join("capabilities/downloads");
    fs::create_dir_all(&temporary).map_err(|_| "capability_install_failed")?;
    let mut file =
        tempfile::NamedTempFile::new_in(&temporary).map_err(|_| "capability_install_failed")?;
    let mut reader = response.into_reader();
    let mut digest = Sha256::new();
    let mut received = 0u64;
    let deadline = Instant::now() + Duration::from_secs(600);
    let mut buffer = [0u8; 65536];
    loop {
        if is_cancelled(id) {
            return Err("cancelled".into());
        }
        if Instant::now() > deadline {
            return Err("capability_download_failed".into());
        }
        let count = reader
            .read(&mut buffer)
            .map_err(|_| "capability_download_failed")?;
        if count == 0 {
            break;
        }
        received += count as u64;
        if received > total {
            return Err("capability_download_integrity".into());
        }
        file.write_all(&buffer[..count])
            .map_err(|_| "capability_install_failed")?;
        digest.update(&buffer[..count]);
        update(id, |j| j.received = received);
    }
    if received != total
        || format!("{:x}", digest.finalize()) != entry["sha256"].as_str().unwrap_or("")
    {
        return Err("capability_download_integrity".into());
    }
    file.as_file_mut()
        .sync_all()
        .map_err(|_| "capability_install_failed")?;
    // Extraction and real probing remain cancellable. Only the short atomic
    // commit closes cancellation, under the same job lock as the UI request.
    install(app, file.path(), entry["id"].as_str().unwrap_or(""), id)
}
fn start_uninstall(app: &Arc<App>, v: &Value) -> ApiResult<Response> {
    let owned = scope(app);
    let engine = v["id"].as_str().unwrap_or("");
    checked_entry(&app.paths.assets_dir, engine)?;
    let existing = |map: &BTreeMap<String, Job>| -> ApiResult<Option<Value>> {
        if let Some(job) = map.values().find(|j| j.scope == owned && j.engine == engine && active(j.phase)) {
            if !job.uninstall { return Err(ApiError::new(409, "capability_install_busy")); }
            return Ok(Some(job.value()));
        }
        Ok(None)
    };
    if let Some(value) = existing(&jobs().lock().unwrap_or_else(|e| e.into_inner()))? {
        return ok_json(value);
    }
    if let Some(code) = crate::capability_packs::busy_code(app, engine)? {
        return Err(ApiError::new(409, code));
    }
    let mut map = jobs().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(value) = existing(&map)? { return ok_json(value); }
    if map.len() >= 64 {
        let retired = map.iter().find(|(_, j)| !active(j.phase)).map(|(id, _)| id.clone());
        if let Some(id) = retired { map.remove(&id); }
        else { return Err(ApiError::new(409, "capability_install_busy")); }
    }
    let job = Job { id: uuid::Uuid::new_v4().to_string(), engine: engine.into(), scope: owned,
        phase: "removing", uninstall: true, ..Job::default() };
    let response = job.value();
    let id = job.id.clone();
    map.insert(id.clone(), job);
    drop(map);
    let app = app.clone();
    let engine = engine.to_owned();
    let worker = id.clone();
    if std::thread::Builder::new().name("readmd-capability-remove".into()).spawn(move || {
        let check = || is_cancelled(&worker);
        let phase = |phase| update(&worker, |j| { if !j.cancel { j.phase = phase; } });
        let progress = |p| update(&worker, |j| j.progress = Some(p));
        let committing = || commit(&worker);
        let hooks = crate::capability_packs::InstallHooks { cancelled: &check, phase: &phase, progress: &progress, committing: &committing };
        let cancellation = worker.clone();
        let result = crate::speech::with_cancel(Arc::new(move || is_cancelled(&cancellation)), || {
            crate::capability_packs::uninstall_with_hooks(&app, &engine, &hooks)
        }).and_then(|response| serde_json::from_slice::<Value>(&response.body).map_err(|_| failure("capability_install_failed")));
        match result {
            Ok(value) => { update(&worker, |j| j.result = Some(value)); finish(&worker, Ok(())); }
            Err(error) => finish(&worker, Err(error.code)),
        }
    }).is_err() {
        finish(&id, Err("capability_install_failed".into()));
        return Err(failure("capability_install_failed"));
    }
    ok_json(response)
}
pub fn endpoint(app: &Arc<App>, v: &Value) -> ApiResult<Response> {
    let action = v["action"].as_str().unwrap_or("");
    if matches!(action, "upload_start" | "upload_finish") {
        return upload::endpoint(app, v);
    }
    if action == "uninstall_start" { return start_uninstall(app, v); }
    let owned = scope(app);
    if action == "catalog" {
        let mut catalog = catalog_at(&app.paths.assets_dir)?;
        catalog["platform"] = json!(crate::capability_packs::platform());
        let entries = catalog["packs"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| {
                p["platform"] == crate::capability_packs::platform() || p["platform"] == "universal"
            })
            .map(|entry| {
                let mut e = entry.clone();
                e["available"] = json!(checked_entry(
                    &app.paths.assets_dir,
                    e["id"].as_str().unwrap_or("")
                )
                .is_ok());
                e
            })
            .collect::<Vec<_>>();
        catalog["packs"] = json!(entries);
        catalog["ok"] = json!(true);
        return ok_json(catalog);
    }
    if matches!(action, "download_status" | "cancel_download") {
        let mut map = jobs().lock().unwrap_or_else(|e| e.into_inner());
        let job = map
            .get_mut(v["job"].as_str().unwrap_or(""))
            .filter(|j| j.scope == owned)
            .ok_or_else(|| failure("capability_task_missing"))?;
        if action == "cancel_download" {
            cancel(job)?;
        }
        let retired = if job.phase == "cancelled" {
            job.upload.take()
        } else {
            None
        };
        let result = job.value();
        drop(map);
        drop(retired);
        return ok_json(result);
    }
    if !matches!(action, "download" | "import") {
        return Err(failure("capability_pack_invalid"));
    }
    let engine = v["id"].as_str().unwrap_or("");
    let entry = checked_entry(&app.paths.assets_dir, engine)?;
    {
        let map = jobs().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(job) = map
            .values()
            .find(|j| j.scope == owned && j.engine == engine && active(j.phase))
        {
            if job.uninstall { return Err(ApiError::new(409, "capability_install_busy")); }
            return ok_json(job.value());
        }
    }
    if let Some(code) = crate::capability_packs::busy_code(app, engine)? {
        return Err(ApiError::new(409, code));
    }
    let imported = if action == "import" {
        let path = app
            .paths
            .resolve_doc(
                v["path"]
                    .as_str()
                    .ok_or_else(|| failure("capability_pack_invalid"))?,
            )
            .map_err(|_| failure("capability_pack_invalid"))?;
        let metadata = fs::metadata(&path).map_err(|_| failure("capability_pack_invalid"))?;
        if !metadata.is_file() || metadata.len() > crate::capability_archive::MAX_DOWNLOAD {
            return Err(failure("capability_pack_invalid"));
        }
        Some((path, metadata.len()))
    } else {
        None
    };
    if action == "download" && crate::capability_packs::ready(app, engine) {
        return ok_json(
            json!({"ok":true,"id":engine,"phase":"ready","installed":true,"unchanged":true}),
        );
    }
    let mut map = jobs().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(job) = map
        .values()
        .find(|j| j.scope == owned && j.engine == engine && active(j.phase))
    {
        if job.uninstall { return Err(ApiError::new(409, "capability_install_busy")); }
        return ok_json(job.value());
    }
    if map.len() >= 64 {
        let remove = map
            .iter()
            .find(|(_, j)| !active(j.phase))
            .map(|(id, _)| id.clone());
        if let Some(id) = remove {
            map.remove(&id);
        } else {
            return Err(ApiError::new(409, "capability_install_busy"));
        }
    }
    let transfer = crate::capability_process::reserve_transfer(&app.paths.data_dir, imported
        .as_ref().map(|(_,n)|*n).unwrap_or(entry["downloadBytes"].as_u64().unwrap()))?;
    let job = Job {
        id: uuid::Uuid::new_v4().to_string(),
        engine: engine.into(),
        scope: owned,
        phase: if imported.is_some() {
            "extracting"
        } else {
            "downloading"
        },
        received: 0,
        total: imported
            .as_ref()
            .map(|(_, n)| *n)
            .unwrap_or(entry["downloadBytes"].as_u64().unwrap()),
        progress: None,
        error: None,
        cancel: false,
        ..Job::default()
    };
    let result = job.value();
    let id = job.id.clone();
    map.insert(id.clone(), job);
    drop(map);
    let app = app.clone();
    let worker_id = id.clone();
    if std::thread::Builder::new()
        .name("readmd-capability-install".into())
        .spawn(move || {
            let result = if let Some((path, _)) = imported {
                let _transfer = transfer;
                install(&app, &path, entry["id"].as_str().unwrap_or(""), &worker_id)
            } else {
                download(&app, &entry, &worker_id, transfer)
            };
            finish(&worker_id, result);
        })
        .is_err()
    {
        finish(&id, Err("capability_install_failed".into()));
        return Err(failure("capability_install_failed"));
    }
    ok_json(result)
}
pub fn upload(app: &Arc<App>, req: &crate::server::Request) -> ApiResult<Response> {
    if req.method != "POST" {
        return Err(ApiError::new(405, "method_not_allowed"));
    }
    let id = req.q("id").unwrap_or("");
    if !id.is_empty() {
        checked_entry(&app.paths.assets_dir, id)?;
    }
    let _transfer=crate::capability_process::reserve_transfer(&app.paths.data_dir,req.body.len() as u64)?;
    // Raw transport avoids base64 expansion and does not retain another copy
    // under uploads/ after a successful or failed offline installation.
    crate::capability_packs::install_bytes(app, &req.body, id)
}
pub fn upload_chunk(app: &Arc<App>, req: &crate::server::Request) -> ApiResult<Response> {
    upload::chunk(app, req)
}
pub(crate) fn cleanup_uploads(data: &Path) {
    upload::cleanup(data);
}
/// Installation helper: no document indexing, no browser and no system engine
/// lookup. NSIS uses the exact same catalog, verification and engine probe.
pub fn install_cli(id: &str, assets: Option<&Path>) -> Result<(), String> {
    let mut paths = crate::paths::AppPaths::resolve().map_err(|_| "capability_install_failed")?;
    if let Some(assets) = assets {
        paths.assets_dir = assets.to_path_buf();
    }
    paths.workspace = paths.data_dir.join("capabilities/installer");
    let app = Arc::new(App::bootstrap(paths).map_err(|_| "capability_install_failed")?);
    let entry = checked_entry(&app.paths.assets_dir, id).map_err(|e| e.code)?;
    if crate::capability_packs::ready(&app, id) {
        return Ok(());
    }
    let transfer=crate::capability_process::reserve_transfer(&app.paths.data_dir,
        entry["downloadBytes"].as_u64().unwrap()).map_err(|e|e.code)?;
    let job = Job {
        id: uuid::Uuid::new_v4().to_string(),
        engine: id.into(),
        scope: scope(&app),
        phase: "downloading",
        received: 0,
        total: entry["downloadBytes"].as_u64().unwrap(),
        progress: None,
        error: None,
        cancel: false,
        ..Job::default()
    };
    let id = job.id.clone();
    jobs()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id.clone(), job);
    let result = download(&app, &entry, &id, transfer);
    jobs().lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_transfer_limit_rejects_download_import_and_raw_upload_before_work() {
        let data=tempfile::tempdir().unwrap();let docs=data.path().join("documents");
        fs::create_dir(&docs).unwrap();
        let assets=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let app=Arc::new(App::bootstrap(crate::paths::AppPaths::with_dirs(data.path(),&docs,&assets)).unwrap());
        let file=docs.join("Offline.readmd-pack");fs::write(&file,b"foreign unparsed input").unwrap();
        let all=crate::capability_process::reserve_transfer(data.path(),crate::capability_archive::MAX_DOWNLOAD).unwrap();
        for body in [json!({"action":"download","id":"fonts"}),json!({"action":"import","id":"fonts","path":file})] {
            assert_eq!(endpoint(&app,&body).err().unwrap().code,"capability_install_busy");
        }
        let request=crate::server::Request{method:"POST".into(),path:"/api/capabilities/upload".into(),
            query:std::collections::HashMap::from([("id".into(),"fonts".into())]),headers:Default::default(),body:b"unparsed bytes".to_vec()};
        assert_eq!(upload(&app,&request).err().unwrap().code,"capability_install_busy");
        assert!(!jobs().lock().unwrap().values().any(|j|j.scope==scope(&app)));
        assert!(!data.path().join("capabilities/managed-v2/transfers").exists());
        assert!(!data.path().join("capabilities/downloads").exists());
        assert_eq!(fs::read(file).unwrap(),b"foreign unparsed input");drop(all);
        drop(crate::capability_process::reserve_transfer(data.path(),crate::capability_archive::MAX_DOWNLOAD).unwrap());
    }
    #[test]
    fn delivery_rejects_arbitrary_repositories_paths_and_sizes() {
        let valid = json!({"id":"typst","version":"0.15.1","url":"https://github.com/Natsummerance/tool-ReadMD/releases/download/packs-1/typst.readmd-pack","sha256":"a".repeat(64),"downloadBytes":100});
        assert!(validate_entry(&valid).is_ok());
        for url in ["http://github.com/Natsummerance/tool-ReadMD/releases/download/packs-1/a.readmd-pack","https://github.com/other/repo/releases/download/v1/a.readmd-pack","https://github.com/Natsummerance/tool-ReadMD/releases/download/../a.readmd-pack","https://github.com/Natsummerance/tool-ReadMD/releases/download/v1/a.readmd-pack?token=x"]{let mut v=valid.clone();v["url"]=json!(url);assert!(validate_entry(&v).is_err());}
        for n in [0, MAX_BYTES + 1] {
            let mut v = valid.clone();
            v["downloadBytes"] = json!(n);
            assert!(validate_entry(&v).is_err());
        }
    }
    #[test]
    fn catalog_describes_only_reviewed_platform_bytes() {
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let catalog = catalog_at(&assets).unwrap();
        for entry in catalog["packs"].as_array().unwrap() {
            validate_entry(entry).unwrap();
            assert!(entry["installedBytes"].as_u64().unwrap() > 0);
        }
        if cfg!(all(windows, target_arch = "x86_64")) {
            assert!(checked_entry(&assets, "typst").is_ok());
            assert!(checked_entry(&assets, "pdfium").is_ok());
        }
        assert!(checked_entry(&assets, "unknown").is_err());
    }
    #[test]
    fn cancellation_and_commit_have_one_atomic_boundary() {
        for phase in ["downloading", "extracting", "probing", "removing"] {
            let id = uuid::Uuid::new_v4().to_string();
            let mut job = Job {
                id: id.clone(),
                engine: "typst".into(),
                scope: "unit".into(),
                phase,
                received: 0,
                total: 1,
                progress: None,
                error: None,
                cancel: false,
                ..Job::default()
            };
            cancel(&mut job).unwrap();
            assert_eq!(job.phase, "cancelling");
            jobs().lock().unwrap().insert(id.clone(), job);
            assert_eq!(commit(&id).unwrap_err().code, "cancelled");
            finish(&id, Err("cancelled".into()));
            assert_eq!(
                jobs().lock().unwrap().remove(&id).unwrap().phase,
                "cancelled"
            );
        }
        let id = uuid::Uuid::new_v4().to_string();
        jobs().lock().unwrap().insert(
            id.clone(),
            Job {
                id: id.clone(),
                engine: "typst".into(),
                scope: "unit".into(),
                phase: "probing",
                received: 1,
                total: 1,
                progress: None,
                error: None,
                cancel: false,
                ..Job::default()
            },
        );
        commit(&id).unwrap();
        {
            let mut all = jobs().lock().unwrap();
            let job = all.get_mut(&id).unwrap();
            assert_eq!(cancel(job).unwrap_err().code, "capability_install_busy");
            assert!(!job.cancel);
        }
        finish(&id, Ok(()));
        assert_eq!(jobs().lock().unwrap().remove(&id).unwrap().phase, "ready");
    }
    #[test]
    fn removal_completion_reports_result_and_recovery_errors_win_over_cancel() {
        for failed in [false, true] {
            let id = uuid::Uuid::new_v4().to_string();
            jobs().lock().unwrap().insert(id.clone(), Job { id: id.clone(), uninstall: true,
                cancel: failed, phase: "committing", result: Some(json!({"installed":false,"cleanup_pending":true})), ..Job::default() });
            finish(&id, if failed { Err("capability_install_failed".into()) } else { Ok(()) });
            let job = jobs().lock().unwrap().remove(&id).unwrap();
            assert_eq!(job.phase, if failed { "error" } else { "removed" });
            if failed { assert_eq!(job.error.as_deref(), Some("capability_install_failed")); }
            else { assert_eq!(job.value()["result"]["cleanup_pending"], true); }
        }
    }
    #[test]
    fn large_stream_delivery_requires_bounded_format_and_manifest_digest() {
        let mut entry = json!({"id":"typst","version":"0.15.1","url":"https://github.com/Natsummerance/tool-ReadMD/releases/download/packs-1/typst.readmd-pack","sha256":"a".repeat(64),"downloadBytes":MAX_BYTES+1,"format":"stream-v2","manifestSha256":"b".repeat(64)});
        assert!(validate_entry(&entry).is_ok());
        entry["downloadBytes"] = json!(crate::capability_archive::MAX_DOWNLOAD + 1);
        assert!(validate_entry(&entry).is_err());
        entry["downloadBytes"] = json!(MAX_BYTES + 1);
        entry["manifestSha256"] = Value::Null;
        assert!(validate_entry(&entry).is_err());
        entry["format"] = json!("json-v1");
        assert!(validate_entry(&entry).is_err());
    }
}
