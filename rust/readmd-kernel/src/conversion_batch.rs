//! Durable document batches for the conversion workbench. The legacy batch
//! API still places its outputs alongside the input files.
use super::{
    ConvertJobItem, OnExists, convert_target, mdcheck_base_dir, plan_converted_output,
    save_converted_assets_prepared,
};
use crate::App;
use crate::cancel::CancelState;
use crate::document_operations::{self as files, ErrorKind, FileIdentity, Guard, Observation};
use crate::error::{ApiError, ApiResult};
use crate::server::{Request, Response, app_token_authorized};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, SystemTime};

#[path = "conversion_batch/links.rs"]
mod links;
use links::relocate_relative_links;

pub(super) const MAX_ITEMS: usize = 200;
const MAX_RECEIPT: u64 = 8 * 1024 * 1024;
const RECORD_FILE: &str = "receipt.json";
type Cancellation = Arc<Mutex<bool>>;
static ACTIVE: LazyLock<Mutex<HashMap<PathBuf, Cancellation>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Deserialize, Serialize)]
struct Batch {
    schema: u8,
    id: String,
    output_dir: PathBuf,
    output_identity: String,
    language: Option<String>,
    attempt: u32,
    pending: Option<Publication>,
    items: Vec<Item>,
}

#[derive(Deserialize, Serialize)]
struct Item {
    source: PathBuf,
    result: ConvertJobItem,
}

#[derive(Deserialize, Serialize)]
struct Publication {
    index: usize,
    identity: String,
    sha256: String,
    #[serde(default)]
    length: Option<u64>,
}

impl Batch {
    fn finished(&self) -> bool {
        self.items.iter().all(|item| item.result.done)
    }

    fn start_response(&self) -> Response {
        Response::json(&json!({
            "job": self.id,
            "total": self.items.len(),
            "attempt": self.attempt,
        }))
    }

    fn progress_response(&self) -> Response {
        let items = self
            .items
            .iter()
            .map(|item| &item.result)
            .collect::<Vec<_>>();
        Response::json(&json!({
            "job": self.id,
            "attempt": self.attempt,
            "output_dir": self.output_dir,
            "running": !self.finished(),
            "finished": self.finished(),
            "done": items.iter().filter(|item| item.done).count(),
            "total": items.len(),
            "items": items,
        }))
    }

    fn verify_output(&self) -> Result<(), &'static str> {
        let identity =
            files::directory_identity(&self.output_dir).map_err(|_| "batch_output_unavailable")?;
        if identity.as_str() != self.output_identity {
            return Err("batch_output_unavailable");
        }
        Ok(())
    }
}

impl Item {
    fn reset(&mut self) {
        self.result.status = "queued".into();
        self.result.out = None;
        self.result.engine = None;
        self.result.warns = None;
        self.result.error = None;
        self.result.error_code = None;
        self.result.error_details = None;
        self.result.fidelity_report = None;
        self.result.done = false;
    }

    fn finish(&mut self, status: &str, error_code: Option<&str>) {
        self.result.status = status.to_owned();
        self.result.done = true;
        self.result.error_code = error_code.map(str::to_owned);
        self.result.out = if status == "ok" {
            self.result.planned_out.clone()
        } else {
            None
        };
        if status != "ok" {
            self.result.fidelity_report = None;
        }
    }
}

pub(crate) fn valid_id(id: &str) -> bool {
    id.strip_prefix("b-").is_some_and(|raw| {
        raw.len() == 32
            && raw
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

pub(super) fn require_token(app: &Arc<App>, req: &Request) -> ApiResult<()> {
    if !app_token_authorized(app, req) {
        return Err(ApiError::plain_text(403, "forbidden"));
    }
    Ok(())
}

fn storage_error() -> ApiError {
    ApiError::new(507, "batch_storage_unavailable")
}

fn missing_job() -> ApiError {
    ApiError::new(404, "job_not_found")
}

fn private_dir(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

fn root(app: &App, create: bool) -> ApiResult<Option<PathBuf>> {
    let profile = dunce::canonicalize(&app.paths.data_dir).map_err(|_| storage_error())?;
    let path = profile.join("conversion-batches");
    match fs::symlink_metadata(&path) {
        Ok(_) => (),
        Err(error) if error.kind() == io::ErrorKind::NotFound && !create => return Ok(None),
        Err(error) if error.kind() == io::ErrorKind::NotFound => match private_dir(&path) {
            Ok(()) => (),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
            Err(_) => return Err(storage_error()),
        },
        Err(_) => return Err(storage_error()),
    }
    files::validate_directory(&path)
        .map(Some)
        .map_err(|_| storage_error())
}

fn receipts(root: &Path) -> ApiResult<Vec<(SystemTime, String)>> {
    let mut records: Vec<_> = fs::read_dir(root)
        .map_err(|_| storage_error())?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let id = entry.file_name().into_string().ok()?;
            if !valid_id(&id) || !entry.file_type().ok()?.is_dir() {
                return None;
            }
            let receipt = fs::symlink_metadata(entry.path().join(RECORD_FILE)).ok()?;
            receipt
                .is_file()
                .then(|| (receipt.modified().unwrap_or(SystemTime::UNIX_EPOCH), id))
        })
        .collect();
    records.sort_unstable_by(|left, right| right.cmp(left));
    Ok(records)
}

fn history_response(root: Option<&Path>) -> ApiResult<Response> {
    let mut history = Vec::new();
    if let Some(root) = root {
        for (modified, id) in receipts(root)? {
            let info = read_receipt(&root.join(&id), &id).ok();
            let name = info
                .as_ref()
                .map(|batch| crate::convert::basename(&batch.items[0].result.src));
            let total = info.as_ref().map(|batch| batch.items.len());
            let fidelity_reports = info.as_ref().map(|batch| {
                batch
                    .items
                    .iter()
                    .filter(|item| {
                        item.result.status == "ok" && item.result.fidelity_report.is_some()
                    })
                    .count()
            });
            let modified = modified
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_millis() as u64)
                .unwrap_or(0);
            history.push(json!({ "job": id, "modified": modified, "name": name, "total": total, "fidelity_reports": fidelity_reports }));
        }
    }
    Ok(Response::json(&json!({ "batches": history })))
}

fn read_receipt(dir: &Path, id: &str) -> ApiResult<Batch> {
    // A worker replaces this file atomically while progress reads it. Retry
    // a raced observation, including an unlinked old handle (nlink=0). The
    // final capture must still be regular, single-linked and fully validated.
    let mut attempt = 0;
    let observed = loop {
        match files::observe(&dir.join(RECORD_FILE), MAX_RECEIPT) {
            Ok(Observation::Unsupported(files::UnsupportedReason::MultipleLinks))
                if attempt < 2 =>
            {
                attempt += 1;
                std::thread::yield_now();
            }
            Ok(observed) => break observed,
            Err(error)
                if attempt < 2
                    && matches!(
                        error.kind,
                        ErrorKind::IdentityChanged | ErrorKind::RevisionChanged
                    ) =>
            {
                attempt += 1;
                std::thread::yield_now();
            }
            Err(_) => return Err(storage_error()),
        }
    };
    let file = match observed {
        Observation::Missing => return Err(missing_job()),
        Observation::Unsupported(_) => return Err(storage_error()),
        Observation::Regular(file) => file,
    };
    let batch: Batch = serde_json::from_slice(file.bytes()).map_err(|_| storage_error())?;
    if batch.schema != 1
        || batch.id != id
        || batch.items.is_empty()
        || batch.items.len() > MAX_ITEMS
        || !batch.output_dir.is_absolute()
        || FileIdentity::parse(&batch.output_identity).is_err()
        || batch.pending.as_ref().is_some_and(|pending| {
            pending.index >= batch.items.len()
                || batch.items[pending.index].result.done
                || FileIdentity::parse(&pending.identity).is_err()
                || pending.sha256.len() != 64
                || !pending.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        || batch.items.iter().enumerate().any(|(index, item)| {
            !item.source.is_absolute()
                || item.result.planned_out.as_deref()
                    != batch
                        .output_dir
                        .join(output_name(&item.result.src, index))
                        .to_str()
        })
    {
        return Err(storage_error());
    }
    Ok(batch)
}

fn persist(lease: &Guard, dir: &Path, batch: &Batch) -> Result<(), &'static str> {
    let error = "batch_storage_unavailable";
    lease.verify_profile(dir).map_err(|_| error)?;
    let bytes = serde_json::to_vec(batch).map_err(|_| error)?;
    if bytes.len() as u64 > MAX_RECEIPT {
        return Err(error);
    }
    crate::content::write_bytes_atomic(&dir.join(RECORD_FILE), &bytes).map_err(|_| error)
}

fn job_dir(root: &Path, id: &str) -> ApiResult<PathBuf> {
    if !valid_id(id) {
        return Err(ApiError::bad_request("invalid_request"));
    }
    let dir = root.join(id);
    match fs::symlink_metadata(&dir) {
        Ok(meta) if meta.is_dir() => Ok(dir),
        Ok(_) => Err(storage_error()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Err(missing_job()),
        Err(_) => Err(storage_error()),
    }
}

fn lease(dir: &Path) -> ApiResult<Guard> {
    files::acquire(dir, Duration::ZERO).map_err(|err| {
        if err.kind == ErrorKind::Busy {
            ApiError::new(409, "batch_running")
        } else {
            storage_error()
        }
    })
}

/// A prepared result may have been published immediately before the host
/// exited. Match the staged identity, length and digest before claiming it.
fn recover(lease: &Guard, dir: &Path, batch: &mut Batch) -> ApiResult<()> {
    if batch.finished() {
        return Ok(());
    }
    let owned_output = batch.verify_output().is_ok();
    if let Some(pending) = batch.pending.take() {
        let item = &mut batch.items[pending.index];
        let target = Path::new(item.result.planned_out.as_deref().unwrap_or(""));
        if owned_output
            && files::matches_publication(
                target,
                &pending.identity,
                &pending.sha256,
                pending.length,
            )
            .unwrap_or(false)
        {
            item.finish("ok", None);
        }
    }
    for item in batch.items.iter_mut().filter(|item| !item.result.done) {
        item.finish("error", Some("batch_interrupted"));
    }
    persist(lease, dir, batch).map_err(|_| storage_error())
}

pub(super) fn progress(app: &Arc<App>, req: &Request, id: &str) -> ApiResult<Response> {
    require_token(app, req)?;
    let root = root(app, false)?;
    if req.q("history") == Some("1") {
        return history_response(root.as_deref());
    }
    let root = root.ok_or_else(missing_job)?;
    let id = if req.q("latest") == Some("1") {
        receipts(&root)?
            .into_iter()
            .next()
            .map(|(_, id)| id)
            .ok_or_else(missing_job)?
    } else {
        id.to_owned()
    };
    let dir = job_dir(&root, &id)?;
    let mut batch = read_receipt(&dir, &id)?;
    if !batch.finished() {
        match lease(&dir) {
            Ok(lease) => {
                // The other worker may have finished between reading and
                // acquiring its lock. Always recover from the fresh receipt.
                batch = read_receipt(&dir, &id)?;
                recover(&lease, &dir, &mut batch)?;
            }
            Err(error) if error.status == 409 => (),
            Err(error) => return Err(error),
        }
    }
    Ok(batch.progress_response())
}

pub(crate) fn cancel(app: &Arc<App>, req: &Request, id: &str) -> ApiResult<CancelState> {
    require_token(app, req)?;
    let checked_dir = || {
        root(app, false)?
            .ok_or_else(missing_job)
            .and_then(|root| job_dir(&root, id))
    };
    let dir = match checked_dir() {
        Ok(dir) => dir,
        Err(error) if error.status == 404 => return Ok(CancelState::Unknown),
        Err(error) => return Err(error),
    };
    let cancellation = ACTIVE
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(&dir)
        .cloned();
    if let Some(cancellation) = cancellation {
        let mut canceled = cancellation.lock().unwrap_or_else(|p| p.into_inner());
        if read_receipt(&dir, id)?.finished() {
            return Ok(CancelState::Finished);
        }
        *canceled = true;
        return Ok(CancelState::Cancelling);
    }
    let lease = lease(&dir)?;
    let mut batch = read_receipt(&dir, id)?;
    recover(&lease, &dir, &mut batch)?;
    Ok(CancelState::Finished)
}

fn output_name(source: &str, index: usize) -> String {
    let basename = crate::convert::basename(source);
    let (stem, _) = crate::convert::splitext_raw(&basename);
    let mut safe = String::new();
    for character in stem.chars() {
        if safe.len() + character.len_utf8() > 160 {
            break;
        }
        safe.push(
            if character.is_control() || r#"\/:*?"<>|"#.contains(character) {
                '_'
            } else {
                character
            },
        );
    }
    let stem = safe.trim_matches(|c: char| c.is_whitespace() || c == '.');
    format!(
        "{:03} - {}.md",
        index + 1,
        if stem.is_empty() { "document" } else { stem }
    )
}

fn input_paths(app: &Arc<App>, body: &Value) -> ApiResult<Vec<(String, PathBuf)>> {
    let paths = body
        .get("paths")
        .and_then(Value::as_array)
        .ok_or_else(|| ApiError::bad_request("invalid_request"))?;
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for value in paths {
        let src = value
            .as_str()
            .ok_or_else(|| ApiError::bad_request("invalid_request"))?;
        let raw = Path::new(src);
        let working = if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            convert_target(app, src).ok_or_else(|| ApiError::bad_request("invalid_request"))?
        };
        let source = dunce::canonicalize(&working).unwrap_or_else(|_| {
            PathBuf::from(crate::convert::py_abspath(&working.to_string_lossy()))
        });
        let identity = match files::observe_identity(&source) {
            Ok(files::IdentityObservation::Regular(identity)) => identity.as_str().to_owned(),
            _ => crate::convert::py_normcase(&source.to_string_lossy()),
        };
        if seen.insert(identity) {
            items.push((src.to_owned(), source));
        }
        if items.len() > MAX_ITEMS {
            return Err(ApiError::new(413, "batch_too_many_files"));
        }
    }
    if items.is_empty() {
        return Err(ApiError::bad_request("no_convertible_files"));
    }
    Ok(items)
}

fn selected_parent(body: &Value) -> ApiResult<Option<PathBuf>> {
    let Some(value) = body.get("output_parent") else {
        return Ok(None);
    };
    let error = || ApiError::new(422, "batch_output_unavailable");
    let directory = value
        .as_str()
        .map(Path::new)
        .filter(|path| path.is_absolute())
        .ok_or_else(error)?;
    let directory = dunce::canonicalize(directory).map_err(|_| error())?;
    files::validate_directory(&directory)
        .map(Some)
        .map_err(|_| error())
}

pub(super) fn start(app: &Arc<App>, req: &Request, body: &Value) -> ApiResult<Response> {
    require_token(app, req)?;
    if let Some(id) = body.get("retry").and_then(Value::as_str) {
        return retry(app, id, body.get("attempt").and_then(Value::as_u64));
    }
    let (lease, dir, batch) = prepare_batch(app, body)?;
    run(app, lease, dir, batch)
}

fn prepare_batch(app: &Arc<App>, body: &Value) -> ApiResult<(Guard, PathBuf, Batch)> {
    let inputs = input_paths(app, body)?;
    let parent = selected_parent(body)?;
    let id = match body.get("job") {
        None => format!("b-{}", uuid::Uuid::new_v4().simple()),
        Some(value) => value
            .as_str()
            .filter(|id| valid_id(id))
            .ok_or_else(|| ApiError::bad_request("invalid_request"))?
            .to_owned(),
    };
    let root = root(app, true)?.ok_or_else(storage_error)?;
    let dir = root.join(&id);
    private_dir(&dir).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            ApiError::new(409, "batch_job_exists")
        } else {
            storage_error()
        }
    })?;
    let lease = lease(&dir)?;
    let output_dir = parent
        .map(|parent| parent.join(format!("ReadMD-{}", &id[2..])))
        .unwrap_or_else(|| dir.join("output"));
    private_dir(&output_dir).map_err(|_| ApiError::new(422, "batch_output_unavailable"))?;
    let output_identity = files::directory_identity(&output_dir)
        .map_err(|_| ApiError::new(422, "batch_output_unavailable"))?
        .as_str()
        .to_owned();
    let items = inputs
        .into_iter()
        .enumerate()
        .map(|(index, (src, source))| {
            let output = output_dir.join(output_name(&src, index));
            Item {
                source,
                result: ConvertJobItem {
                    src,
                    planned_out: Some(output.to_string_lossy().into_owned()),
                    status: "queued".to_owned(),
                    out: None,
                    engine: None,
                    warns: None,
                    error: None,
                    error_code: None,
                    error_details: None,
                    fidelity_report: None,
                    done: false,
                },
            }
        })
        .collect();
    let batch = Batch {
        schema: 1,
        id,
        output_dir,
        output_identity,
        language: body
            .get("language")
            .and_then(Value::as_str)
            .map(str::to_owned),
        attempt: 1,
        pending: None,
        items,
    };
    if persist(&lease, &dir, &batch).is_err() {
        if batch.verify_output().is_ok() {
            let _ = fs::remove_dir(&batch.output_dir);
        }
        return Err(storage_error());
    }
    Ok((lease, dir, batch))
}

fn retry(app: &Arc<App>, id: &str, attempt: Option<u64>) -> ApiResult<Response> {
    let root = root(app, false)?.ok_or_else(missing_job)?;
    let dir = job_dir(&root, id)?;
    let lease = lease(&dir)?;
    let mut batch = read_receipt(&dir, id)?;
    recover(&lease, &dir, &mut batch)?;
    if attempt != Some(batch.attempt.into()) {
        return Err(ApiError::new(409, "batch_attempt_changed"));
    }
    batch
        .verify_output()
        .map_err(|code| ApiError::new(422, code))?;
    for item in &mut batch.items {
        if matches!(item.result.status.as_str(), "error" | "canceled") {
            item.reset();
        }
    }
    if batch.finished() {
        return Err(ApiError::new(409, "batch_no_retry_items"));
    }
    batch.attempt = batch.attempt.checked_add(1).ok_or_else(storage_error)?;
    persist(&lease, &dir, &batch).map_err(|_| storage_error())?;
    run(app, lease, dir, batch)
}

fn run(app: &App, lease: Guard, dir: PathBuf, mut batch: Batch) -> ApiResult<Response> {
    let response = batch.start_response();
    let paths = app.paths.clone();
    let cancellation = Arc::new(Mutex::new(false));
    ACTIVE
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(dir.clone(), cancellation.clone());
    let worker_dir = dir.clone();
    let worker = std::thread::Builder::new()
        .name("readmd-conversion-batch".into())
        .spawn(move || {
            // Remove the entry before releasing the receipt lease. A new attempt
            // can then register only after this worker has left.
            struct Registered(PathBuf);
            impl Drop for Registered {
                fn drop(&mut self) {
                    ACTIVE
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .remove(&self.0);
                }
            }
            let lease = lease;
            let _registered = Registered(worker_dir.clone());
            let outcome = crate::ocr_portable::with_paths(&paths, || {
                run_items(
                    &paths.data_dir,
                    &lease,
                    &worker_dir,
                    &mut batch,
                    &cancellation,
                    convert_item,
                )
            });
            if let Err(code) = outcome {
                log::warn!("batch {} stopped before its next receipt: {code}", batch.id);
            }
        });
    if worker.is_err() {
        ACTIVE
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&dir);
        return Err(ApiError::new(503, "batch_start_failed"));
    }
    Ok(response)
}

fn convert_item(
    data: &Path,
    source: &Path,
    language: Option<&str>,
    source_bytes: Option<&[u8]>,
) -> crate::convert::ConvertTriple {
    let source = source.to_string_lossy();
    crate::plugin_manager::convert_document_with_source(
        &crate::plugin_manager::Sandbox::new(data),
        &source,
        true,
        language,
        source_bytes,
    )
}

fn run_items(
    data: &Path,
    lease: &Guard,
    dir: &Path,
    batch: &mut Batch,
    cancellation: &Cancellation,
    convert: impl Fn(&Path, &Path, Option<&str>, Option<&[u8]>) -> crate::convert::ConvertTriple,
) -> Result<(), &'static str> {
    for index in 0..batch.items.len() {
        if batch.items[index].result.done {
            continue;
        }
        if *cancellation.lock().unwrap_or_else(|p| p.into_inner()) {
            for item in batch.items[index..]
                .iter_mut()
                .filter(|item| !item.result.done)
            {
                item.finish("canceled", None);
            }
            return persist(lease, dir, batch);
        }
        batch.items[index].result.status = "running".into();
        persist(lease, dir, batch)?;

        let source = batch.items[index].source.clone();
        let target = PathBuf::from(
            batch.items[index]
                .result
                .planned_out
                .as_deref()
                .unwrap_or(""),
        );
        let plan = plan_converted_output(data, target, OnExists::Skip, false, false);
        let result = if let Err(code) = batch.verify_output() {
            Err(code.into())
        } else if let Err(code) = &plan.expected {
            Err(code.clone())
        } else if !source.is_file() {
            Err("file_not_found".into())
        } else if crate::convert::is_win7()
            && !crate::convert::WIN7_CONVERT_EXTS
                .contains(&crate::convert::ext_of(&source.to_string_lossy()).as_str())
        {
            Err("unsupported_on_legacy_windows".into())
        } else {
            let cancel = cancellation.clone();
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::speech::with_cancel(
                    Arc::new(move || *cancel.lock().unwrap_or_else(|p| p.into_inner())),
                    || {
                        let before = crate::convert::fidelity_source::observe(&source);
                        let converted = convert(
                            data,
                            &source,
                            batch.language.as_deref(),
                            before.as_ref().map(|file| file.bytes()),
                        );
                        let report = (!converted.text.trim().is_empty()).then(|| {
                            crate::convert::fidelity_source::report_if_current(
                                &source,
                                &converted,
                                before.as_ref(),
                            )
                        });
                        (converted, report)
                    },
                )
            }))
            .map_err(|_| "conversion_failed".into())
        };

        let result = result.map(|(mut converted, report)| {
            let (fixed, mut warns) =
                crate::convert::mdcheck_check(&converted.text, &mdcheck_base_dir(&source));
            warns.truncate(16);
            for warning in &mut warns {
                if let Some(msg) = warning["msg"].as_str() {
                    warning["msg"] = json!(msg.chars().take(512).collect::<String>());
                }
            }
            converted.text = relocate_relative_links(fixed, &source, &batch.output_dir);
            (converted, warns, report)
        });
        // Leave the cancellation endpoint responsive during asset writes,
        // temp-file sync and receipt preparation. Pin its lock only for the
        // last no-replace publication below.
        let check_cancel = || {
            if *cancellation.lock().unwrap_or_else(|p| p.into_inner()) {
                Err("cancelled".to_string())
            } else {
                Ok(())
            }
        };
        let mut status = "error";
        let mut code = None;
        let mut commit_guard = None;
        if check_cancel().is_err() {
            status = "canceled";
        } else {
            match result {
                Err(error) => code = Some(error),
                Ok((result, _, _)) if result.text.is_empty() && result.error.is_some() => {
                    let failure = crate::conversion_errors::body(&result.error.unwrap_or_default());
                    let row = &mut batch.items[index].result;
                    row.error = failure["error"].as_str().map(str::to_owned);
                    row.error_details = failure.get("error_details").cloned();
                    code = failure["error_code"].as_str().map(str::to_owned);
                }
                Ok((result, _, _)) if result.text.trim().is_empty() => {
                    code = Some("empty_output".into())
                }
                Ok((result, warns, report)) => {
                    batch.items[index].result.engine = Some(result.engine);
                    batch.items[index].result.warns = Some(warns);
                    // Stage this exact extraction's report in the publication
                    // receipt, so recovery can retain it after a committed file.
                    batch.items[index].result.fidelity_report = report;
                    let committed = save_converted_assets_prepared(
                        data,
                        &result.text,
                        &plan,
                        &result.assets,
                        check_cancel,
                        |text, temporary| {
                            batch.verify_output()?;
                            let identity = match files::observe_identity(temporary) {
                                Ok(files::IdentityObservation::Regular(identity)) => {
                                    identity.as_str().to_owned()
                                }
                                _ => return Err("conversion_write_failed".into()),
                            };
                            batch.pending = Some(Publication {
                                index,
                                identity,
                                sha256: format!("{:x}", Sha256::digest(text.as_bytes())),
                                length: Some(text.len() as u64),
                            });
                            persist(lease, dir, batch)?;
                            let guard = cancellation.lock().unwrap_or_else(|p| p.into_inner());
                            if *guard {
                                return Err("cancelled".into());
                            }
                            commit_guard = Some(guard);
                            Ok(())
                        },
                    );
                    match committed {
                        Err(error) if error == "cancelled" => status = "canceled",
                        Ok((_, true, _, _, _)) => status = "ok",
                        Ok(_) => code = Some("output_exists".into()),
                        Err(error) => code = Some(error),
                    }
                    batch.pending = None;
                }
            }
        }
        batch.items[index].finish(status, code.as_deref());
        persist(lease, dir, batch)?;
        drop(commit_guard);
    }
    Ok(())
}

#[cfg(test)]
#[path = "conversion_batch_tests.rs"]
mod tests;
