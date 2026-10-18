//! Revision-guarded PDF drafts. History contains operations, not a PDF per edit.
//! PDFium is loaded only by a bounded, disposable capability worker.
use crate::{
    server::{ok_json, Request, Response},
    ApiError, ApiResult, App,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

const LIMIT: u64 = 256 * 1024 * 1024;
const DLL_SHA256: &str = "79d4676b656cfb1abcea88f9ade3b4b0826c5200382db5f4ec72a636c598c118";
static ACTIVE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static OPEN_GATE: Mutex<()> = Mutex::new(());
static DRAFT_STORAGE: Mutex<()> = Mutex::new(());
struct Session {
    _resources: Option<crate::capability_packs::ResourceLease>,
    directory: tempfile::TempDir,
    base: PathBuf,
    base_revision: String,
    created: i64,
    draft: Option<PathBuf>,
    profile: PathBuf,
    path: PathBuf,
    password: String,
    source_revision: String,
    revision: u64,
    operations: Vec<Value>,
    redo: Vec<Value>,
    saved: Vec<Value>,
    touched: Instant,
    /// Page object identities are valid only for the current operation revision.
    readonly: HashMap<u64, HashSet<u64>>,
}
type Sessions = HashMap<String, Arc<Mutex<Session>>>;
fn sessions() -> &'static Mutex<Sessions> {
    static TABLE: OnceLock<Mutex<Sessions>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}
pub fn busy(app: &App) -> bool {
    // A session worker may be entering its resource scope while holding the
    // session mutex. Never wait for it from the installer holding INSTALL.
    ACTIVE.load(std::sync::atomic::Ordering::Acquire) > 0
        || sessions()
            .try_lock()
            .map(|table|table.values()
            .any(|s| {
                s.try_lock()
                    .map(|s| s.profile == app.paths.data_dir)
                    .unwrap_or(true)
            })).unwrap_or(true)
}
fn error(code: &'static str) -> ApiError {
    ApiError::new(
        match code {
            "pdf_conflict" => 409,
            "pdf_session_expired" => 410,
            "pdf_engine_unavailable" | "publication_fonts_unavailable" => 503,
            "pdf_busy" => 429,
            "cancelled" => 499,
            _ => 422,
        },
        code,
    )
}
fn read(path: &Path) -> ApiResult<Vec<u8>> {
    let file = fs::File::open(path).map_err(|_| error("pdf_open_failed"))?;
    if file.metadata().map_err(|_| error("pdf_open_failed"))?.len() > LIMIT {
        return Err(error("pdf_limit"));
    }
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| error("pdf_open_failed"))?;
    if bytes.len() as u64 > LIMIT {
        return Err(error("pdf_limit"));
    }
    Ok(bytes)
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn fonts(directory: &Path) -> Value {
    let mut fonts = serde_json::Map::new();
    // Persisted legacy family names are aliases, never host font lookups.
    for (key, name) in [
        ("ReadMD Sans SC", "ReadMDSansSC-Regular.ttf"),
        ("ReadMD Serif SC", "ReadMDSerifSC-Regular.ttf"),
        ("Arial", "ReadMDSansSC-Regular.ttf"),
        ("SimHei", "ReadMDSansSC-Regular.ttf"),
        ("KaiTi", "ReadMDSerifSC-Regular.ttf"),
    ] {
        fonts.insert(key.into(), json!(directory.join("fonts").join(name)));
    }
    Value::Object(fonts)
}
fn resources(app: &App) -> ApiResult<(PathBuf, PathBuf)> {
    if std::env::var_os("READMD_PDF_WORKER").is_none() && std::env::var_os("READMD_PDFIUM_LIB").is_none() {
        // Platform-specific names and hashes come from the app's local pack
        // lock. A missing platform lock fails closed; it never searches PATH.
        return crate::capability_packs::pdfium_resources(&app.paths.data_dir,&app.paths.assets_dir)
            .map_err(|_|error("pdf_engine_unavailable"));
    }
    let exe = std::env::current_exe().map_err(|_| error("pdf_engine_unavailable"))?;
    let worker = std::env::var_os("READMD_PDF_WORKER")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let packed = crate::capability_packs::directory(app).join("bin/readmd-pdf-worker.exe");
            if crate::capability_packs::installed(app) {
                packed
            } else {
                exe.with_file_name(if cfg!(windows) {
                    "readmd-pdf-worker.exe"
                } else {
                    "readmd-pdf-worker"
                })
            }
        });
    let library = std::env::var_os("READMD_PDFIUM_LIB")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            crate::capability_packs::directory(app)
                .join("bin")
                .join(if cfg!(windows) {
                    "pdfium.dll"
                } else if cfg!(target_os = "macos") {
                    "libpdfium.dylib"
                } else {
                    "libpdfium.so"
                })
        });
    if !worker.is_file() || !library.is_file() {
        return Err(error("pdf_engine_unavailable"));
    }
    // Each platform pack must declare and pin its own digest. Only Windows x64
    // has been prepared so far; unreviewed libraries are never loaded implicitly.
    #[cfg(all(windows, target_arch = "x86_64"))]
    if hash(&read(&library)?) != DLL_SHA256 {
        return Err(error("pdf_engine_unavailable"));
    }
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    return Err(error("pdf_engine_unavailable"));
    Ok((worker, library))
}
fn worker(app: &App, mut input: Value, dir: &Path) -> ApiResult<Value> {
    let _resources=if std::env::var_os("READMD_PDF_WORKER").is_none()&&std::env::var_os("READMD_PDFIUM_LIB").is_none(){Some(crate::capability_packs::lease(&app.paths.data_dir,&app.paths.assets_dir,"pdfium").map_err(|_|error("pdf_engine_unavailable"))?)}else{None};
    // A replacement font is needed only for explicit replacement operations.
    // Hold the audited pack for the entire replay/save; no system fallback.
    let needs_fonts = input["operations"].as_array().is_some_and(|ops| ops.iter().any(|op|
        op["kind"] == "text" && op["font"].as_str().is_some_and(|name| !name.is_empty())));
    let _fonts = if needs_fonts {
        let lease = crate::capability_packs::lease(&app.paths.data_dir, &app.paths.assets_dir, "fonts")
            .map_err(|_| error("publication_fonts_unavailable"))?;
        input["fonts"] = fonts(&lease.directory);
        Some(lease)
    } else { None };
    let (executable,library)=resources(app)?;input["library"]=json!(library);
    worker_with_resources(&executable,input,dir)
}
fn worker_with_resources(executable:&Path,input:Value,dir:&Path)->ApiResult<Value>{
    use std::sync::atomic::Ordering;
    let active = ACTIVE.fetch_add(1, Ordering::AcqRel);
    struct Slot;
    impl Drop for Slot {
        fn drop(&mut self) {
            ACTIVE.fetch_sub(1, Ordering::AcqRel);
        }
    }
    let _slot = Slot;
    if active >= 2 {
        return Err(error("pdf_busy"));
    }
    execute_worker(executable, input, dir)
}
/// Read-only page rendering for the same OCR service used by desktop and MCP.
/// No session, copied PDF, sidecar or dependency on an OS PDF renderer.
pub(crate) fn raster_for_ocr(paths:&crate::paths::AppPaths,path:&str,page:usize)->Result<(usize,Vec<u8>),String>{
    use base64::Engine;
    if fs::metadata(path).map_or(true,|m|m.len()>LIMIT){return Err("ocr_failed".into());}
    if lopdf::Document::load(path).map_or(false,|d|d.is_encrypted()){return Err("pdf_read_only".into());}
    let _resources=crate::capability_packs::lease(&paths.data_dir,&paths.assets_dir,"pdfium").map_err(|_|"pdf_engine_unavailable")?;
    let (program,library)=crate::capability_packs::pdfium_resources(&paths.data_dir,&paths.assets_dir)?;
    let dir=tempfile::Builder::new().prefix("readmd-pdf-ocr-").tempdir().map_err(|_|"ocr_failed")?;
    let result=worker_with_resources(&program,json!({"action":"render","library":library,"pdf":path,"password":"","page":page,"width":2200,"operations":[]}),dir.path()).map_err(|e|e.code.to_string())?;
    let count=result["pages"].as_u64().filter(|n|*n>0&&*n<=10000).ok_or("ocr_failed")? as usize;
    if page>=count{return Err("ocr_failed".into());}
    let bytes=base64::engine::general_purpose::STANDARD.decode(result["image"].as_str().ok_or("ocr_failed")?).map_err(|_|"ocr_failed")?;Ok((count,bytes))
}
fn execute_worker(executable: &Path, mut input: Value, dir: &Path) -> ApiResult<Value> {
    if crate::speech::cancelled(){return Err(error("cancelled"));}
    let operation = tempfile::Builder::new()
        .prefix("worker-")
        .tempdir_in(dir)
        .map_err(|_| error("pdf_save_failed"))?;
    let request = operation.path().join("request.json");
    let output = operation.path().join("result.json");
    let password = input
        .as_object_mut()
        .and_then(|object| object.remove("password"))
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    if password.len() > 1024 || password.contains('\0') {
        return Err(error("pdf_invalid_operation"));
    }
    fs::write(
        &request,
        serde_json::to_vec(&input).map_err(|_| error("pdf_invalid_operation"))?,
    )
    .map_err(|_| error("pdf_save_failed"))?;
    let mut child = crate::silent_command(executable)
        .args([&request, &output])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| error("pdf_engine_unavailable"))?;
    // Authentication stays on an anonymous pipe, never in request files or
    // process arguments. Closing the pipe also bounds the worker's read.
    if child
        .stdin
        .take()
        .is_none_or(|mut stdin| stdin.write_all(password.as_bytes()).is_err())
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error("pdf_worker_failed"));
    }
    let started = Instant::now();
    loop {
        let aborted = if crate::speech::cancelled() {
            Some("cancelled")
        } else if started.elapsed() > Duration::from_secs(45) {
            Some("pdf_timeout")
        } else {
            None
        };
        if let Some(code) = aborted {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error(code));
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return Err(error("pdf_worker_failed"));
                }
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error("pdf_worker_failed"));
            }
        }
    }
    if fs::metadata(&output)
        .map_err(|_| error("pdf_worker_failed"))?
        .len()
        > 48 * 1024 * 1024
    {
        return Err(error("pdf_limit"));
    }
    let result: Value =
        serde_json::from_slice(&fs::read(output).map_err(|_| error("pdf_worker_failed"))?)
            .map_err(|_| error("pdf_worker_failed"))?;
    if result["ok"] != true {
        let code = result["error_code"].as_str().unwrap_or("pdf_worker_failed");
        return Err(ApiError::new(
            if code == "pdf_engine_unavailable" {
                503
            } else {
                422
            },
            code,
        ));
    }
    Ok(result)
}
pub fn probe_directory(dir: &Path) -> ApiResult<()> {
    let working = tempfile::tempdir().map_err(|_| error("pdf_save_failed"))?;
    let (worker,library)=crate::capability_packs::pdfium_names(std::env::consts::OS).ok_or_else(||error("pdf_engine_unavailable"))?;
    execute_worker(
        &dir.join("bin").join(worker),
        json!({"action":"probe","library":dir.join("bin").join(library)}),
        working.path(),
    )
    .map(|_| ())
}
pub fn probe(app: &App) -> Result<(), &'static str> {
    let dir = tempfile::tempdir().map_err(|_| "pdf_engine_unavailable")?;
    worker(app, json!({"action":"probe"}), dir.path())
        .map(|_| ())
        .map_err(|_| "pdf_engine_unavailable")
}
fn request(s: &Session, v: &Value, operations: &[Value]) -> Value {
    json!({"action":"render","pdf":s.base,"asset_root":s.base.parent().unwrap().join("assets"),"password":s.password,"operations":operations,"page":v["page"].as_u64().unwrap_or(0),"width":v["width"].as_u64().unwrap_or(1200),"allow_signatures":v["allow_signatures"].as_bool().unwrap_or(false)})
}
fn remember_readonly(s: &mut Session, result: &mut Value) {
    let mut protected = HashSet::new();
    if let Some(objects) = result["objects"].as_array_mut() {
        for item in objects {
            // Keep the already distributed worker compatible: it reports font
            // names but predates the readonly flag. Never let editing the copy
            // layer masquerade as changing the visible vector formula.
            if item["font"] == "ReadMDMathText" || item["readonly"] == true {
                item["readonly"] = json!(true);
                if let Some(id) = item["id"].as_u64() { protected.insert(id); }
            }
        }
    }
    if let Some(page) = result["page"].as_u64() { s.readonly.insert(page, protected); }
}

fn decorate(s: &mut Session, id: &str, mut result: Value) -> Value {
    remember_readonly(s, &mut result);
    result["session"] = json!(id);
    result["revision"] = json!(s.revision);
    result["dirty"] = json!(s.operations != s.saved);
    result["path"] = json!(s.path);
    result["source_revision"] = json!(s.source_revision);
    result["can_undo"] = json!(!s.operations.is_empty());
    result["can_redo"] = json!(!s.redo.is_empty());
    result["fonts"] = json!(["ReadMD Sans SC", "ReadMD Serif SC"]);
    result
}
fn draft_root(app: &App) -> PathBuf {
    app.paths.data_dir.join("pdf-drafts")
}
fn draft_id(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value)
}
fn storage_bytes(root: &Path) -> u64 {
    walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .fold(0u64, |total, m| total.saturating_add(m.len()))
}
fn asset_name(name: &str) -> bool {
    name.len() == 64 && name.bytes().all(|b| b.is_ascii_hexdigit())
}
fn remove_draft(app: &App, directory: &Path) {
    // Delete only the private, named draft files; leave unknown user files alone.
    if directory.parent() != Some(draft_root(app).as_path())
        || directory
            .file_name()
            .and_then(|s| s.to_str())
            .is_none_or(|s| !draft_id(s))
    {
        return;
    }
    let assets = directory.join("assets");
    if let Ok(entries) = fs::read_dir(&assets) {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|t| t.is_file())
                && entry.file_name().to_str().is_some_and(asset_name)
            {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    let _ = fs::remove_dir(assets);
    for name in ["base.pdf", "draft.json"] {
        let _ = fs::remove_file(directory.join(name));
    }
    let _ = fs::remove_dir(directory);
}
fn persist_draft(
    app: &App,
    s: &mut Session,
    operations: &[Value],
    redo: &[Value],
) -> ApiResult<()> {
    let _storage = DRAFT_STORAGE.lock().unwrap_or_else(|e| e.into_inner());
    let root = draft_root(app);
    fs::create_dir_all(&root).map_err(|_| error("pdf_save_failed"))?;
    let directory = s
        .draft
        .clone()
        .unwrap_or_else(|| root.join(uuid::Uuid::new_v4().to_string()));
    let staging = if s.draft.is_none() {
        Some(
            tempfile::Builder::new()
                .prefix("draft-")
                .tempdir_in(&root)
                .map_err(|_| error("pdf_save_failed"))?,
        )
    } else {
        None
    };
    let destination = staging
        .as_ref()
        .map(|d| d.path())
        .unwrap_or(directory.as_path());
    if staging.is_some() {
        let bytes = read(&s.base)?;
        let assets = s.base.parent().unwrap().join("assets");
        if storage_bytes(&root)
            .saturating_add(bytes.len() as u64)
            .saturating_add(storage_bytes(&assets))
            .saturating_add(2 * 1024 * 1024)
            > 512 * 1024 * 1024
        {
            return Err(error("pdf_limit"));
        }
        crate::content::write_bytes_atomic(&destination.join("base.pdf"), &bytes)
            .map_err(|_| error("pdf_save_failed"))?;
        if let Ok(entries) = fs::read_dir(&assets) {
            fs::create_dir_all(destination.join("assets")).map_err(|_| error("pdf_save_failed"))?;
            for entry in entries.flatten() {
                let name = entry.file_name();
                if entry.file_type().is_ok_and(|t| t.is_file())
                    && name.to_str().is_some_and(asset_name)
                {
                    fs::copy(entry.path(), destination.join("assets").join(name))
                        .map_err(|_| error("pdf_save_failed"))?;
                }
            }
        }
    }
    let meta = json!({"schema":1,"path":s.path,"source_revision":s.source_revision,"base_sha256":s.base_revision,"revision":s.revision+1,"operations":operations,"redo":redo,"saved":s.saved,"created":s.created});
    crate::content::write_text_atomic(&destination.join("draft.json"), &meta.to_string())
        .map_err(|_| error("pdf_save_failed"))?;
    if staging.is_some() {
        fs::rename(destination, &directory).map_err(|_| error("pdf_save_failed"))?;
    }
    if s.draft.is_none() {
        let old = std::mem::replace(&mut s.base, directory.join("base.pdf"));
        let _ = fs::remove_file(old);
        s.draft = Some(directory);
    }
    Ok(())
}
fn read_draft(app: &App, id: &str) -> ApiResult<(PathBuf, Value)> {
    if !draft_id(id) {
        return Err(error("pdf_invalid_operation"));
    }
    let directory = draft_root(app).join(id);
    let file = directory.join("draft.json");
    if fs::metadata(&file)
        .map_err(|_| error("pdf_session_expired"))?
        .len()
        > 2 * 1024 * 1024
    {
        return Err(error("pdf_limit"));
    }
    let meta: Value =
        serde_json::from_slice(&fs::read(file).map_err(|_| error("pdf_session_expired"))?)
            .map_err(|_| error("pdf_session_expired"))?;
    if meta["schema"] != 1
        || meta["base_sha256"].as_str() != Some(hash(&read(&directory.join("base.pdf"))?).as_str())
    {
        return Err(error("pdf_session_expired"));
    }
    Ok((directory, meta))
}
fn checkpoint(app: &App, path: &Path, bytes: &[u8]) -> ApiResult<String> {
    let directory = app.paths.data_dir.join("pdf-history");
    fs::create_dir_all(&directory).map_err(|_| error("pdf_save_failed"))?;
    let id = hash(format!("{}\0{}", path.display(), hash(bytes)).as_bytes());
    let target = directory.join(format!("{id}.pdf"));
    if !target.exists() {
        crate::content::write_bytes_atomic(&target, bytes).map_err(|_| error("pdf_save_failed"))?;
    }
    crate::content::write_text_atomic(&directory.join(format!("{id}.json")),&json!({"path":path,"id":id,"sha256":hash(bytes),"created":crate::content::unix_millis(std::time::SystemTime::now())}).to_string()).map_err(|_|error("pdf_save_failed"))?;
    let mut entries: Vec<_> = fs::read_dir(&directory)
        .map_err(|_| error("pdf_save_failed"))?
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) != Some("pdf") {
                return None;
            }
            let m = e.metadata().ok()?;
            Some((p, m.modified().ok()?, m.len()))
        })
        .collect();
    entries.sort_by_key(|e| std::cmp::Reverse(e.1));
    let mut total = 0u64;
    for (index, (file, modified, size)) in entries.into_iter().enumerate() {
        total = total.saturating_add(size);
        if file != target
            && (index >= 32
                || total > 512 * 1024 * 1024
                || modified
                    .elapsed()
                    .is_ok_and(|age| age > Duration::from_secs(30 * 24 * 60 * 60)))
        {
            let _ = fs::remove_file(file.with_extension("json"));
            let _ = fs::remove_file(file);
        }
    }
    Ok(id)
}
pub fn endpoint(app: &Arc<App>, req: &Request) -> ApiResult<Response> {
    if req.method != "POST" {
        return Err(ApiError::new(405, "method_not_allowed"));
    }
    let v = req.json()?;
    let task = v["task_id"].as_str().and_then(crate::cancel::register);
    let check = task
        .as_ref()
        .map(|t| t.checker())
        .unwrap_or_else(|| Arc::new(|| false));
    let external = crate::speech::cancellation_check();
    let check = Arc::new(move || check() || external());
    crate::speech::with_cancel(check, || handle(app, &v, task.as_ref()))
}
fn handle(
    app: &Arc<App>,
    v: &Value,
    task: Option<&crate::cancel::TaskGuard>,
) -> ApiResult<Response> {
    let action = v["action"].as_str().unwrap_or("open");
    if action == "probe" {
        probe(app).map_err(error)?;
        return ok_json(json!({"ok":true,"engine":"pdfium","version":"7881"}));
    }
    if action == "drafts" {
        let mut drafts = Vec::new();
        if let Ok(entries) = fs::read_dir(draft_root(app)) {
            for entry in entries.flatten() {
                let id = entry.file_name().to_string_lossy().into_owned();
                if !draft_id(&id) {
                    continue;
                }
                if let Ok((_, meta)) = read_draft(app, &id) {
                    if meta["operations"] != meta["saved"] {
                        drafts.push(json!({"id":id,"name":Path::new(meta["path"].as_str().unwrap_or("")).file_name().and_then(|s|s.to_str()),"created":meta["created"]}));
                    }
                }
            }
        }
        return ok_json(json!({"ok":true,"drafts":drafts}));
    }
    if action == "history" || action == "restore_history" {
        let path = app
            .paths
            .resolve_doc(v["path"].as_str().ok_or_else(|| error("pdf_open_failed"))?)
            .map_err(|_| error("pdf_open_failed"))?;
        if path
            .extension()
            .and_then(|s| s.to_str())
            .is_none_or(|s| !s.eq_ignore_ascii_case("pdf"))
        {
            return Err(error("pdf_open_failed"));
        }
        let root = app.paths.data_dir.join("pdf-history");
        if action == "history" {
            let mut history = Vec::new();
            if let Ok(entries) = fs::read_dir(&root) {
                for entry in entries.flatten() {
                    let file = entry.path();
                    if file.extension().and_then(|s| s.to_str()) != Some("json") {
                        continue;
                    }
                    if fs::metadata(&file).is_ok_and(|m| m.len() <= 65536) {
                        if let Ok(meta) = fs::read(&file)
                            .map_err(|_| ())
                            .and_then(|b| serde_json::from_slice::<Value>(&b).map_err(|_| ()))
                        {
                            if meta["path"].as_str() == path.to_str()
                                && meta["id"].as_str().is_some_and(asset_name)
                            {
                                history.push(json!({"id":meta["id"],"created":meta["created"]}));
                            }
                        }
                    }
                }
            }
            history.sort_by_key(|e| std::cmp::Reverse(e["created"].as_i64().unwrap_or(0)));
            return ok_json(
                json!({"ok":true,"history":history,"source_revision":hash(&read(&path)?)}),
            );
        }
        let _opening = OPEN_GATE.lock().unwrap_or_else(|e| e.into_inner());
        if sessions()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .any(|s| {
                s.lock()
                    .map(|s| s.profile == app.paths.data_dir && s.path == path)
                    .unwrap_or(true)
            })
        {
            return Err(error("pdf_busy"));
        }
        let id = v["checkpoint"]
            .as_str()
            .filter(|s| asset_name(s))
            .ok_or_else(|| error("pdf_invalid_operation"))?;
        let meta_path = root.join(format!("{id}.json"));
        if fs::metadata(&meta_path)
            .map_err(|_| error("pdf_session_expired"))?
            .len()
            > 65536
        {
            return Err(error("pdf_limit"));
        }
        let meta: Value =
            serde_json::from_slice(&fs::read(meta_path).map_err(|_| error("pdf_session_expired"))?)
                .map_err(|_| error("pdf_session_expired"))?;
        let bytes = read(&root.join(format!("{id}.pdf")))?;
        if meta["id"] != id
            || meta["path"].as_str() != path.to_str()
            || meta["sha256"].as_str() != Some(hash(&bytes).as_str())
        {
            return Err(error("pdf_session_expired"));
        }
        let _lock = crate::document_history::SAVE_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let original = read(&path)?;
        if v["expected_source_revision"].as_str() != Some(hash(&original).as_str()) {
            return Err(error("pdf_conflict"));
        }
        let recovery = checkpoint(app, &path, &original)?;
        if task.is_some_and(|t| !t.try_commit()) || crate::speech::cancelled() {
            return Err(error("cancelled"));
        }
        crate::content::write_bytes_atomic(&path, &bytes).map_err(|_| error("pdf_save_failed"))?;
        return ok_json(json!({"ok":true,"source_revision":hash(&bytes),"recovery":recovery}));
    }
    if action == "open" || action == "resume" {
        let _opening = OPEN_GATE.lock().unwrap_or_else(|e| e.into_inner());
        let resources=if std::env::var_os("READMD_PDF_WORKER").is_none()&&std::env::var_os("READMD_PDFIUM_LIB").is_none(){Some(crate::capability_packs::lease(&app.paths.data_dir,&app.paths.assets_dir,"pdfium").map_err(|_|error("pdf_engine_unavailable"))?)}else{None};
        let mut table = sessions().lock().unwrap_or_else(|e| e.into_inner());
        table.retain(|_, entry| {
            entry
                .lock()
                .map(|s| {
                    let keep = s.operations != s.saved
                        || s.touched.elapsed() < Duration::from_secs(8 * 60 * 60);
                    if !keep && s.profile == app.paths.data_dir {
                        if let Some(dir) = &s.draft {
                            remove_draft(app, dir);
                        }
                    }
                    keep
                })
                .unwrap_or(false)
        });
        if table.len() >= 4 {
            return Err(error("pdf_busy"));
        }
        drop(table);
        let mut s = if action == "resume" {
            let (directory, meta) = read_draft(app, v["draft"].as_str().unwrap_or(""))?;
            if sessions()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .values()
                .any(|s| {
                    s.lock()
                        .map(|s| s.draft.as_ref() == Some(&directory))
                        .unwrap_or(true)
                })
            {
                return Err(error("pdf_busy"));
            }
            let base = directory.join("base.pdf");
            let path = PathBuf::from(
                meta["path"]
                    .as_str()
                    .ok_or_else(|| error("pdf_session_expired"))?,
            );
            let password = v["password"].as_str().unwrap_or("");
            if password.len() > 1024 {
                return Err(error("pdf_limit"));
            }
            Session {
                _resources: resources,
                directory: tempfile::tempdir().map_err(|_| error("pdf_save_failed"))?,
                base,
                base_revision: meta["base_sha256"]
                    .as_str()
                    .ok_or_else(|| error("pdf_session_expired"))?
                    .into(),
                created: meta["created"].as_i64().unwrap_or(0),
                draft: Some(directory),
                profile: app.paths.data_dir.clone(),
                path,
                password: password.into(),
                source_revision: meta["source_revision"]
                    .as_str()
                    .ok_or_else(|| error("pdf_session_expired"))?
                    .into(),
                revision: meta["revision"].as_u64().unwrap_or(0),
                operations: serde_json::from_value(meta["operations"].clone())
                    .map_err(|_| error("pdf_session_expired"))?,
                redo: serde_json::from_value(meta["redo"].clone())
                    .map_err(|_| error("pdf_session_expired"))?,
                saved: serde_json::from_value(meta["saved"].clone())
                    .map_err(|_| error("pdf_session_expired"))?,
                touched: Instant::now(),
                readonly: HashMap::new(),
            }
        } else {
            let path = app
                .paths
                .resolve_doc(v["path"].as_str().ok_or_else(|| error("pdf_open_failed"))?)
                .map_err(|_| error("pdf_open_failed"))?;
            if path
                .extension()
                .and_then(|s| s.to_str())
                .is_none_or(|s| !s.eq_ignore_ascii_case("pdf"))
            {
                return Err(error("pdf_open_failed"));
            }
            let bytes = read(&path)?;
            let directory = tempfile::Builder::new()
                .prefix("readmd-pdf-")
                .tempdir()
                .map_err(|_| error("pdf_save_failed"))?;
            fs::write(directory.path().join("base.pdf"), &bytes)
                .map_err(|_| error("pdf_save_failed"))?;
            let password = v["password"].as_str().unwrap_or("");
            if password.len() > 1024 {
                return Err(error("pdf_limit"));
            }
            let base = directory.path().join("base.pdf");
            Session {
                _resources: resources,
                directory,
                base,
                base_revision: hash(&bytes),
                created: crate::content::unix_millis(std::time::SystemTime::now()),
                draft: None,
                profile: app.paths.data_dir.clone(),
                path,
                password: password.into(),
                source_revision: hash(&bytes),
                revision: 0,
                operations: vec![],
                redo: vec![],
                saved: vec![],
                touched: Instant::now(),
                readonly: HashMap::new(),
            }
        };
        let result = worker(app, request(&s, v, &s.operations), s.directory.path())?;
        let id = uuid::Uuid::new_v4().to_string();
        let result = decorate(&mut s, &id, result);
        let mut table = sessions().lock().unwrap_or_else(|e| e.into_inner());
        if table.len() >= 4 {
            return Err(error("pdf_busy"));
        }
        table.insert(id, Arc::new(Mutex::new(s)));
        return ok_json(result);
    }
    let id = v["session"]
        .as_str()
        .ok_or_else(|| error("pdf_session_expired"))?;
    let entry = sessions()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(id)
        .cloned()
        .ok_or_else(|| error("pdf_session_expired"))?;
    let mut s = entry.lock().unwrap_or_else(|e| e.into_inner());
    if s.profile != app.paths.data_dir {
        return Err(error("pdf_session_expired"));
    }
    if action == "close" {
        if v["discard"].as_bool().unwrap_or(false) || s.operations == s.saved {
            if let Some(dir) = s.draft.as_ref() {
                remove_draft(app, dir);
            }
        }
        drop(s);
        sessions()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
        return ok_json(json!({"ok":true}));
    }
    if v["revision"].as_u64() != Some(s.revision) {
        return Err(error("pdf_conflict"));
    }
    s.touched = Instant::now();
    match action {
        "render" => {
            let result = worker(app, request(&s, v, &s.operations), s.directory.path())?;
            ok_json(decorate(&mut s, id, result))
        }
        "edit" | "undo" | "redo" => {
            let mut operations = s.operations.clone();
            let mut redo = s.redo.clone();
            if action == "edit" {
                if !v["operation"].is_object() {
                    return Err(error("pdf_invalid_operation"));
                }
                let mut operation = v["operation"].clone();
                if matches!(operation["kind"].as_str(), Some("text" | "remove")) {
                    let page = operation["page"].as_u64().ok_or_else(|| error("pdf_invalid_operation"))?;
                    let object = operation["object"].as_u64().ok_or_else(|| error("pdf_invalid_operation"))?;
                    if !s.readonly.contains_key(&page) {
                        let mut target = v.clone();
                        target["page"] = json!(page);
                        target["width"] = json!(320);
                        let mut result = worker(app, request(&s, &target, &s.operations), s.directory.path())?;
                        remember_readonly(&mut s, &mut result);
                    }
                    if s.readonly.get(&page).is_some_and(|ids| ids.contains(&object)) {
                        return Err(error("pdf_unsupported_object"));
                    }
                }
                if operation["kind"] == "image" {
                    let _storage = DRAFT_STORAGE.lock().unwrap_or_else(|e| e.into_inner());
                    use base64::{engine::general_purpose::STANDARD, Engine};
                    let w = operation["width"].as_u64().unwrap_or(0);
                    let h = operation["height"].as_u64().unwrap_or(0);
                    if w == 0 || h == 0 || w.saturating_mul(h) > 4_000_000 {
                        return Err(error("pdf_limit"));
                    }
                    let bytes = STANDARD
                        .decode(
                            operation["rgba"]
                                .as_str()
                                .ok_or_else(|| error("pdf_invalid_operation"))?,
                        )
                        .map_err(|_| error("pdf_invalid_operation"))?;
                    if bytes.len() as u64 != w * h * 4 {
                        return Err(error("pdf_invalid_operation"));
                    }
                    let id = hash(&bytes);
                    let assets = s.base.parent().unwrap().join("assets");
                    fs::create_dir_all(&assets).map_err(|_| error("pdf_save_failed"))?;
                    let additional = if assets.join(&id).exists() {
                        0
                    } else {
                        bytes.len() as u64
                    };
                    if storage_bytes(&assets).saturating_add(additional) > 64 * 1024 * 1024
                        || s.draft.is_some()
                            && storage_bytes(&draft_root(app))
                                .saturating_add(additional)
                                .saturating_add(2 * 1024 * 1024)
                                > 512 * 1024 * 1024
                    {
                        return Err(error("pdf_limit"));
                    }
                    crate::content::write_bytes_atomic(&assets.join(&id), &bytes)
                        .map_err(|_| error("pdf_save_failed"))?;
                    operation.as_object_mut().unwrap().remove("rgba");
                    operation["asset"] = json!(id);
                }
                operations.push(operation);
                redo.clear();
            } else if action == "undo" {
                if let Some(op) = operations.pop() {
                    redo.push(op);
                }
            } else if let Some(op) = redo.pop() {
                operations.push(op);
            }
            if serde_json::to_vec(&operations)
                .map_err(|_| error("pdf_limit"))?
                .len()
                > 1024 * 1024
            {
                return Err(error("pdf_limit"));
            }
            let result = worker(app, request(&s, v, &operations), s.directory.path())?;
            if task.is_some_and(|t| !t.try_commit()) || crate::speech::cancelled() {
                return Err(error("cancelled"));
            }
            persist_draft(app, &mut s, &operations, &redo)?;
            s.operations = operations;
            s.redo = redo;
            s.revision += 1;
            s.readonly.clear();
            ok_json(decorate(&mut s, id, result))
        }
        "save" => {
            if s.operations == s.saved {
                return ok_json(json!({"ok":true,"unchanged":true,"revision":s.revision}));
            }
            let output = s.directory.path().join("result.pdf");
            let mut input = request(&s, v, &s.operations);
            input["action"] = json!("save");
            input["output"] = json!(output);
            worker(app, input, s.directory.path())?;
            let bytes = read(&output)?;
            let _lock = crate::document_history::SAVE_LOCK
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let original = read(&s.path)?;
            if hash(&original) != s.source_revision {
                return Err(error("pdf_conflict"));
            }
            let recovery = checkpoint(app, &s.path, &original)?;
            if task.is_some_and(|t| !t.try_commit()) || crate::speech::cancelled() {
                return Err(error("cancelled"));
            }
            crate::content::write_bytes_atomic(&s.path, &bytes)
                .map_err(|_| error("pdf_save_failed"))?;
            s.source_revision = hash(&bytes);
            s.saved = s.operations.clone();
            s.revision += 1;
            let operations = s.operations.clone();
            let redo = s.redo.clone();
            // The PDF has committed; recovery metadata failure must not report
            // a false save failure. The previous recovery draft remains valid.
            let recovery_updated = persist_draft(app, &mut s, &operations, &redo).is_ok();
            ok_json(
                json!({"ok":true,"revision":s.revision,"dirty":false,"recovery":recovery,"recovery_updated":recovery_updated,"bytes":bytes.len()}),
            )
        }
        _ => Err(error("pdf_invalid_operation")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_fonts_are_private_and_legacy_families_are_explicit_aliases() {
        let directory = tempfile::tempdir().unwrap();
        let mapped = fonts(directory.path());
        assert_eq!(mapped["Arial"], mapped["ReadMD Sans SC"]);
        assert_eq!(mapped["SimHei"], mapped["ReadMD Sans SC"]);
        assert_eq!(mapped["KaiTi"], mapped["ReadMD Serif SC"]);
        for path in mapped.as_object().unwrap().values() {
            assert!(Path::new(path.as_str().unwrap()).starts_with(directory.path()));
        }
        assert!(mapped.get("untrusted-font.ttf").is_none());
        assert_eq!(error("publication_fonts_unavailable").status, 503);
    }
    #[test]
    fn draft_uses_one_base_file_and_removes_only_owned_assets() {
        let data = tempfile::tempdir().unwrap();
        let docs = tempfile::tempdir().unwrap();
        let app = App::bootstrap(crate::paths::AppPaths::with_dirs(
            data.path(),
            docs.path(),
            Path::new("assets"),
        ))
        .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path().join("base.pdf");
        fs::write(&base, b"base").unwrap();
        let assets = directory.path().join("assets");
        fs::create_dir(&assets).unwrap();
        let asset = hash(b"rgba");
        fs::write(assets.join(&asset), b"rgba").unwrap();
        let mut s = Session {
            _resources: None,
            directory,
            base,
            base_revision: hash(b"base"),
            created: 123,
            draft: None,
            profile: data.path().into(),
            path: docs.path().join("test.pdf"),
            password: "not persisted".into(),
            source_revision: hash(b"source"),
            revision: 0,
            operations: vec![],
            redo: vec![],
            saved: vec![],
            touched: Instant::now(),
            readonly: HashMap::new(),
        };
        persist_draft(&app, &mut s, &[json!({"kind":"note","text":"one"})], &[]).unwrap();
        let root = s.draft.clone().unwrap();
        let first = fs::metadata(&s.base).unwrap().modified().unwrap();
        s.revision += 1;
        persist_draft(&app, &mut s, &[json!({"kind":"note","text":"two"})], &[]).unwrap();
        assert_eq!(fs::metadata(&s.base).unwrap().modified().unwrap(), first);
        let meta = fs::read_to_string(root.join("draft.json")).unwrap();
        assert!(!meta.contains("not persisted"));
        let (_, loaded) = read_draft(&app, root.file_name().unwrap().to_str().unwrap()).unwrap();
        assert_eq!(loaded["created"], 123);
        assert_eq!(loaded["revision"], 2);
        fs::write(root.join("unknown.txt"), b"keep").unwrap();
        remove_draft(&app, &root);
        assert!(root.join("unknown.txt").is_file());
        assert!(!root.join("assets").exists());
        assert!(!root.join("base.pdf").exists());
    }
    #[test]
    fn hashes_are_byte_sensitive_and_checkpoint_is_central_and_deduplicated() {
        assert_ne!(hash(b"%PDF-a"), hash(b"%PDF-b"));
        let data = tempfile::tempdir().unwrap();
        let docs = tempfile::tempdir().unwrap();
        let app = App::bootstrap(crate::paths::AppPaths::with_dirs(
            data.path(),
            docs.path(),
            Path::new("assets"),
        ))
        .unwrap();
        let file = docs.path().join("source.pdf");
        fs::write(&file, b"original").unwrap();
        let a = checkpoint(&app, &file, b"original").unwrap();
        let b = checkpoint(&app, &file, b"original").unwrap();
        assert_eq!(a, b);
        assert_eq!(fs::read_dir(docs.path()).unwrap().count(), 1);
        assert_eq!(
            fs::read_dir(data.path().join("pdf-history"))
                .unwrap()
                .count(),
            2
        );
        assert_eq!(
            fs::read(data.path().join("pdf-history").join(format!("{a}.pdf"))).unwrap(),
            b"original"
        );
    }
}
