//! Authorized adapters for durable document review. Business state lives in review.
use crate::{
    document_operations::{self, Guard, ObservedFile, Observation},
    document_transaction::{self as tx, ContentTransaction, TextEdit},
    server::{Request, Response},
    ApiError, ApiResult, App,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

pub fn error(code: &'static str) -> ApiError {
    let status = if code.ends_with("_not_found") {
        404
    } else if code.contains("budget") {
        413
    } else if code.contains("persistence") || code == "document_operation_storage" {
        507
    } else if code.contains("conflict")
        || code.contains("stale")
        || code.contains("recovery")
        || code.contains("in_progress")
        || code == "rename_unsaved_review"
        || code == "document_operation_busy"
        || code == "document_operation_lock"
        || code == "document_operation_identity_changed"
        || code == "document_operation_revision_changed"
        || code == "document_operation_target_exists"
        || code == "document_operation_missing"
        || code == "review_asset_relocation_required"
    {
        409
    } else {
        422
    };
    ApiError::new(status, code)
}

pub fn normalize(source: &str) -> String {
    source
        .strip_prefix('\u{feff}')
        .unwrap_or(source)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}
fn ending(source: &str) -> &'static str {
    if source.contains("\r\n") && !source.replace("\r\n", "").contains('\n') {
        "crlf"
    } else if source.contains('\r') && !source.contains('\n') {
        "cr"
    } else {
        "lf"
    }
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn disk(path: &Path) -> ApiResult<(String, String, String)> {
    use std::io::Read;
    const LIMIT: u64 = 32 * 1024 * 1024;
    if std::fs::metadata(path).is_ok_and(|meta| meta.len() > LIMIT) {
        return Err(error("review_budget"));
    }
    match std::fs::File::open(path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(LIMIT + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| error("review_save_conflict"))?;
            if bytes.len() as u64 > LIMIT {
                return Err(error("review_budget"));
            }
            let (text, encoding) = crate::text_encoding::detect_and_decode(&bytes);
            Ok((digest(&bytes), text, encoding.into()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(("missing".into(), String::new(), "utf-8".into()))
        }
        Err(_) => Err(error("review_save_conflict")),
    }
}
fn draft_path(path: &str) -> bool {
    path.strip_prefix("draft:").is_some_and(|id| {
        !id.is_empty()
            && id.len() <= 180
            && id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
    })
}
fn retained_relative_images(source: &Value) -> bool {
    source["changes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|change| change["decision"] != "withdrawn")
        .any(|change| {
            crate::document_assets::image_references(change["original"].as_str().unwrap_or(""))
                .iter()
                .any(|(_, reference)| {
                    let text = reference.trim();
                    !text.is_empty()
                        && !text.starts_with('#')
                        && !text.starts_with('/')
                        && !text.starts_with("data:")
                        && !text.starts_with("file:")
                        && !text.contains("://")
                        && !Path::new(text).is_absolute()
                })
        })
}
fn authorized(path: &str) -> ApiResult<String> {
    if draft_path(path) {
        return Ok(path.into());
    }
    crate::server::review_authorized_path(path)
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or_else(|| ApiError::new(403, "review_not_authorized"))
}
pub(crate) fn snapshot(app: &Arc<App>, value: &Value) -> ApiResult<Value> {
    let id = value["document_id"]
        .as_str()
        .ok_or_else(|| error("review_invalid_request"))?;
    let result = app
        .review
        .handle(&json!({"action":"read","document_id":id}))
        .map_err(error)?;
    authorized(
        result["path"]
            .as_str()
            .ok_or_else(|| error("review_invalid_request"))?,
    )?;
    Ok(result)
}

const RENAME_READ_LIMIT: u64 = 32 * 1024 * 1024;

fn rename_request(body: &Value, source: &Path, target: &Path) -> ApiResult<Value> {
    let source = source.to_str().ok_or_else(|| error("rename_unsupported_file"))?;
    let target = target.to_str().ok_or_else(|| error("rename_unsupported_file"))?;
    Ok(json!({"operation_id":body["operation_id"],"source_path":source,"target_path":target,
        "expected_disk_revision":body["expected_disk_revision"],
        "expected_revision":body.get("expected_revision").filter(|value|!value.is_null()),
        "review_session":body.get("review_session").filter(|value|!value.is_null())}))
}

/// Trusted rename adapter, called with SAVE_LOCK -> this profile's gate held.
/// The owner retains source_file for its no-replace move after preparation.
/// This function neither moves a file nor adds any path authorization.
pub(crate) fn prepare_rename(
    app: &Arc<App>, body: &Value, source: &Path, target: &Path,
    source_file: &ObservedFile, gate: &Guard,
) -> ApiResult<Value> {
    gate.verify_profile(&app.paths.data_dir).map_err(|e| error(e.code()))?;
    let mut request = rename_request(body, source, target)?;
    let actual_authorized = authorized(source.to_str().ok_or_else(|| error("rename_unsupported_file"))?)?;
    if Path::new(&actual_authorized) != source { return Err(ApiError::new(403, "review_not_authorized")); }
    let observed = gate.verify_existing(source, source_file.fingerprint(), RENAME_READ_LIMIT)
        .map_err(|e| error(e.code()))?;
    if request["expected_disk_revision"].as_str() != Some(observed.fingerprint().revision.as_str()) {
        return Err(error("rename_revision_conflict"));
    }
    gate.verify_missing(target).map_err(|e| error(e.code()))?;
    let (text, encoding) = crate::text_encoding::detect_and_decode(observed.bytes());
    if let Some(expected) = body.get("expected_revision").filter(|value|!value.is_null()) {
        let expected=expected.as_str().ok_or_else(||error("review_invalid_request"))?;
        if expected != digest(text.as_bytes()) { return Err(error("rename_revision_conflict")); }
    }
    request["source_identity"] = json!(observed.fingerprint().identity.as_str());
    request["disk_revision"] = json!(observed.fingerprint().revision);
    request["disk_content"] = json!(normalize(&text));
    request["encoding"] = json!(encoding);
    request["line_ending"] = json!(ending(&text));
    app.review.prepare_rename(&request).map_err(error)
}

/// Retry lookup precedes observing a source that may already have moved. The
/// operation ID is bound to these exact request fields, not a fresh move.
pub(crate) fn replay_rename(app: &Arc<App>, body: &Value, source: &Path, target: &Path) -> ApiResult<Option<Value>> {
    let request = rename_request(body, source, target)?;
    if !app.paths.data_dir.join("review.sqlite3").is_file() { return Ok(None); }
    app.review.rename_replay(&request).map_err(error)
}

/// This returns private profile state. Only a token-authenticated owner may
/// expose it; no returned path becomes authorized for filesystem operations.
pub(crate) fn rename_status(app: &Arc<App>, operation_id: &str) -> ApiResult<Option<Value>> {
    if operation_id.is_empty() || operation_id.len()>128
        || !operation_id.bytes().all(|b|b.is_ascii_alphanumeric() || matches!(b,b'-'|b'_'|b':')) {
        return Err(error("review_invalid_request"));
    }
    if !app.paths.data_dir.join("review.sqlite3").is_file() { return Ok(None); }
    app.review.rename_status(operation_id).map_err(error)
}

pub(crate) fn pending_rename(app: &App, path: &Path) -> ApiResult<Option<Value>> {
    pending_rename_in_profile(&app.paths.data_dir, path)
}

/// Background writers share the same durable reservation check without
/// bootstrapping an App or creating a review database for ordinary conversion.
pub(crate) fn pending_rename_in_profile(data_dir: &Path, path: &Path) -> ApiResult<Option<Value>> {
    if !data_dir.join("review.sqlite3").is_file() { return Ok(None); }
    crate::review::Service::new(data_dir)
        .pending_rename(path.to_str().ok_or_else(|| error("rename_unsupported_file"))?).map_err(error)
}

fn observe_rename(path: &Path) -> crate::review::RenameObservation {
    match document_operations::observe(path, RENAME_READ_LIMIT) {
        Ok(Observation::Missing) => crate::review::RenameObservation::Missing,
        Ok(Observation::Regular(file)) => crate::review::RenameObservation::Regular {
            identity:file.fingerprint().identity.as_str().into(), revision:file.fingerprint().revision.clone(),
        },
        // Permission, storage, unsupported identity and redirection failures
        // cannot establish absence. Keep the durable intent and both files.
        Ok(Observation::Unsupported(_)) | Err(_) => crate::review::RenameObservation::Unavailable,
    }
}

/// Under SAVE_LOCK -> gate, examine only the two persisted endpoints. No caller
/// path, authorization mutation, filesystem move or destructive rollback is
/// accepted here. A DB error leaves the durable intent for a later retry.
pub(crate) fn recover_rename(app: &Arc<App>, operation_id: &str, gate: &Guard) -> ApiResult<Value> {
    gate.verify_profile(&app.paths.data_dir).map_err(|e| error(e.code()))?;
    let current = rename_status(app, operation_id)?.ok_or_else(|| error("rename_not_found"))?;
    if matches!(current["state"].as_str(), Some("committed"|"cancelled")) { return Ok(current); }
    let source = current["old_path"].as_str().ok_or_else(|| error("review_persistence_failed"))?;
    let target = current["target_path"].as_str().ok_or_else(|| error("review_persistence_failed"))?;
    app.review.recover_rename(operation_id, &observe_rename(Path::new(source)), &observe_rename(Path::new(target))).map_err(error)
}

pub fn handle(app: &Arc<App>, req: &Request) -> ApiResult<Response> {
    if req.method != "POST" {
        return Err(ApiError::new(405, "method_not_allowed"));
    }
    let mut value = req.json()?;
    if value["origin"] == "system" || value.get("application").is_some_and(|v| v != "applied") {
        return Err(error("review_invalid_request"));
    }
    let action = value["action"].as_str().unwrap_or("").to_string();
    // Keep the same order as save/rename. Drafts have no filesystem endpoint
    // and retain their existing availability without creating an OS gate.
    let physical_open = matches!(action.as_str(), "open"|"resume")
        && !value["path"].as_str().is_some_and(draft_path);
    let _save_guard = physical_open.then(|| crate::document_history::SAVE_LOCK.lock().unwrap_or_else(|e|e.into_inner()));
    let _document_guard = if physical_open {
        Some(document_operations::acquire(&app.paths.data_dir, Duration::from_secs(3)).map_err(|e|error(e.code()))?)
    } else { None };
    if action == "open" || action == "resume" {
        let path = authorized(
            value["path"]
                .as_str()
                .ok_or_else(|| error("review_invalid_request"))?,
        )?;
        if !draft_path(&path) && pending_rename(app, Path::new(&path))?.is_some() {
            return Err(error("rename_recovery_pending"));
        }
        if action == "resume" {
            if !app.paths.data_dir.join("review.sqlite3").is_file() {
                return Ok(Response::json(&json!({"found":false})));
            }
            let found = app
                .review
                .handle(&json!({"action":"lookup","path":path}))
                .map_err(error)?;
            if found["found"] != true {
                return Ok(Response::json(&json!({"found":false})));
            }
            value["action"] = json!("open");
        }
        if draft_path(&path) {
            value["disk_revision"] = json!("missing");
            value["encoding"] = json!("utf-8");
            value["line_ending"] = json!("lf");
            value["disk_content"] = json!("");
            if action == "resume" {
                value["content"] = json!("");
            }
        } else {
            let (revision, text, encoding) = disk(Path::new(&path))?;
            if revision == "missing" {
                return Err(error("review_save_conflict"));
            }
            value["disk_revision"] = json!(revision);
            value["encoding"] = json!(encoding);
            value["line_ending"] = json!(ending(&text));
            value["disk_content"] = json!(normalize(&text));
            if action == "resume" || value.get("content").is_none() {
                value["content"] = json!(normalize(&text));
            }
        }
        value["path"] = json!(path);
        if let Some(content) = value["content"].as_str() {
            value["content"] = json!(normalize(content));
        }
    } else {
        if !matches!(
            action.as_str(),
            "read" | "edit" | "propose" | "resolve" | "undo" | "redo" | "discard"
        ) {
            return Err(error("review_invalid_request"));
        }
        let current = snapshot(app, &value)?;
        if action == "discard" {
            let path = current["path"]
                .as_str()
                .ok_or_else(|| error("review_invalid_request"))?;
            let (revision, text, _) = if draft_path(path) {
                ("missing".to_string(), String::new(), String::new())
            } else {
                disk(Path::new(path))?
            };
            value["disk_revision"] = json!(revision);
            value["disk_content"] = json!(normalize(&text));
        }
    }
    let result = app.review.handle(&value).map_err(error)?;
    Ok(Response::json(&if action == "resume" {
        json!({"found":true,"snapshot":result})
    } else {
        result
    }))
}

pub fn ai(app: &Arc<App>, req: &Request) -> ApiResult<Response> {
    if req.method != "POST" {
        return Err(ApiError::new(405, "method_not_allowed"));
    }
    let value = req.json()?;
    let source = snapshot(app, &value)?;
    if source.get("rename").is_some() { return Err(error("rename_recovery_pending")); }
    let call = value["call_id"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 128)
        .ok_or_else(|| error("review_invalid_request"))?;
    let request = value["request"]
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= 32 * 1024)
        .ok_or_else(|| error("review_invalid_request"))?;
    // Freeze enabled Skill instructions before checking a receipt. Its content
    // participates in replay identity, while rendering uses the authorized
    // source only for a fresh request after the CAS checks below.
    let template = crate::server::review_task_template(app, &value)?;
    let request_digest = ai_request_digest(&value, request, template.as_ref());
    let receipt = app
        .review
        .handle(&json!({"action":"receipt","document_id":source["document_id"],"call_id":call}))
        .map_err(error)?;
    if receipt["found"] == true {
        if receipt["request"]["ai_request_digest"] != request_digest {
            return Err(error("review_call_conflict"));
        }
        return Ok(Response::json(&receipt["snapshot"]));
    }
    if value["generation"] != source["generation"] {
        return Err(error("review_stale_generation"));
    }
    if value["review_revision"] != source["review_revision"] {
        return Err(error("review_stale_revision"));
    }
    let ranges = validate_ranges(&source, &value)?;
    let mut scoped = source.clone();
    if !ranges.is_empty() {
        scoped["allowed_ranges"] = json!(ranges);
    }
    let task_id = value["task_id"].as_str().unwrap_or(call);
    let task = crate::cancel::register(task_id).ok_or_else(|| error("review_invalid_request"))?;
    let proposal = crate::server::review_model_proposal(app, &value, &scoped, &task, template.as_ref())?;
    let edits: Vec<TextEdit> = serde_json::from_value(proposal["edits"].clone())
        .map_err(|_| error("review_ai_invalid_tool"))?;
    if !ranges.is_empty()
        && edits.iter().any(|edit| {
            !ranges
                .iter()
                .any(|(from, to)| edit.from >= *from && edit.to <= *to)
        })
    {
        return Err(error("review_not_authorized"));
    }
    if crate::speech::cancelled() || !task.try_commit() {
        return Err(error("cancelled"));
    }
    let response=app.review.handle(&json!({
        "action":"propose","document_id":source["document_id"],"generation":source["generation"],
        "review_revision":source["review_revision"],"call_id":call,"origin":"ai",
        "edits":edits,"task_id":value["task_id"].as_str().unwrap_or(call),"ai_request_digest":request_digest
    })).map_err(error)?;
    Ok(Response::json(&response))
}

fn ai_request_digest(value: &Value, request: &str, template: Option<&crate::server::ReviewTaskTemplate>) -> String {
    let mut identity = json!({
        "request":request,"ranges":value.get("ranges"),"provider":value.get("provider"),
        "credential_id":value.get("credential_id"),"model":value.get("model"),
        "generation":value.get("generation"),"review_revision":value.get("review_revision")
    });
    if let Some(template) = template { identity["selected_skill"] = template.identity(); }
    digest(serde_json::to_string(&identity).unwrap_or_default().as_bytes())
}

pub(crate) fn validate_ranges(source: &Value, value: &Value) -> ApiResult<Vec<(usize, usize)>> {
    let Some(raw) = value.get("ranges") else {
        return Ok(vec![]);
    };
    let raw = raw
        .as_array()
        .filter(|r| !r.is_empty() && r.len() <= 4096)
        .ok_or_else(|| error("review_invalid_request"))?;
    let content = source["content"]
        .as_str()
        .ok_or_else(|| error("review_invalid_request"))?;
    let mut result = Vec::new();
    for row in raw {
        let from = row["from"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| error("review_invalid_request"))?;
        let to = row["to"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| error("review_invalid_request"))?;
        if from > to {
            return Err(error("transaction_invalid_range"));
        }
        tx::byte_offset(content, from).map_err(error)?;
        tx::byte_offset(content, to).map_err(error)?;
        result.push((from, to));
    }
    result.sort_unstable();
    if result.windows(2).any(|w| w[0].1 > w[1].0) {
        return Err(error("transaction_overlapping_edits"));
    }
    Ok(result)
}

/// Translate exact raw source edits into CodeMirror's normalized LF coordinates.
fn normalized_edits(source: &str, edits: &[TextEdit], saved: &str) -> ApiResult<Vec<TextEdit>> {
    let mut output = Vec::new();
    for edit in edits {
        let from = tx::byte_offset(source, edit.from).map_err(error)?;
        let to = tx::byte_offset(source, edit.to).map_err(error)?;
        if (from > 0
            && source.as_bytes().get(from - 1) == Some(&b'\r')
            && source.as_bytes().get(from) == Some(&b'\n'))
            || (to > 0
                && source.as_bytes().get(to - 1) == Some(&b'\r')
                && source.as_bytes().get(to) == Some(&b'\n'))
        {
            return Err(error("transaction_invalid_range"));
        }
        output.push(TextEdit {
            from: normalize(&source[..from]).encode_utf16().count(),
            to: normalize(&source[..to]).encode_utf16().count(),
            insert: normalize(&edit.insert),
        });
    }
    let normalized = normalize(source);
    let check = tx::apply(
        &normalized,
        &ContentTransaction {
            base_revision: tx::revision(&normalized),
            origin: "review".into(),
            edits: output.clone(),
        },
    )
    .map_err(error)?;
    if check.content != normalize(saved) {
        return Err(error("review_save_conflict"));
    }
    Ok(output)
}

pub struct SaveGuard {
    app: Arc<App>,
    document_id: Value,
    intent_id: Value,
    call_id: String,
    expected: String,
    planned: String,
    encoding: String,
    line_ending: String,
    target: PathBuf,
    target_before: String,
    file_committed: bool,
    finished: bool,
    edit_steps: Value,
}
impl SaveGuard {
    /// All source edits are provided by materialization/migration, never inferred
    /// from a broad prefix/suffix diff spanning untouched review blocks.
    pub fn prepare(
        app: &Arc<App>,
        body: &Value,
        target: &Path,
        asset_text: &str,
        asset_edits: &[TextEdit],
    ) -> ApiResult<Option<Self>> {
        let Some(review) = body.get("review_session") else {
            // Legacy callers may still save ordinary documents. A file owned
            // by a durable review session must not silently bypass its receipt.
            if app.paths.data_dir.join("review.sqlite3").is_file() {
                let found = app
                    .review
                    .handle(&json!({"action":"lookup","path":target.to_string_lossy()}))
                    .map_err(error)?;
                if found["found"] == true {
                    if found["snapshot"].get("rename").is_some() { return Err(error("rename_recovery_pending")); }
                    let (raw_revision, disk_text, _) = disk(target)?;
                    let encoding = body["encoding"].as_str().unwrap_or("utf-8");
                    let encoded = crate::text_encoding::encode(asset_text, encoding)
                        .map_err(|_| error("encoding_unrepresentable"))?;
                    if digest(&encoded) != raw_revision
                        || normalize(&disk_text) != found["snapshot"]["content"]
                    {
                        return Err(error("review_save_conflict"));
                    }
                }
            }
            return Ok(None);
        };
        if !review.is_object() {
            return Err(error("review_invalid_request"));
        }
        let source = snapshot(app, review)?;
        let raw = body["content"]
            .as_str()
            .ok_or_else(|| error("review_invalid_request"))?;
        if normalize(raw) != source["content"] {
            return Err(error("review_save_conflict"));
        }
        let path = source["path"]
            .as_str()
            .ok_or_else(|| error("review_invalid_request"))?;
        let source_dir = (!draft_path(path))
            .then(|| Path::new(path).parent())
            .flatten();
        let moving = source_dir.map(crate::paths::canonicalize_or_clean)
            != target.parent().map(crate::paths::canonicalize_or_clean);
        if moving && retained_relative_images(&source) {
            // Retention protects the old bytes, but cannot by itself relocate
            // hidden originals. Refuse an unsupported image-review move rather
            // than return a successful save whose later withdrawal is broken.
            return Err(error("review_asset_relocation_required"));
        }
        let before = if draft_path(path) {
            "missing".into()
        } else {
            disk(Path::new(path))?.0
        };
        let target_before = disk(target)?.0;
        let migration = if matches!(
            crate::content::ext_of(target).as_str(),
            "md" | "markdown" | "mkd"
        ) && asset_text.contains("<!--readmd:math:")
        {
            Some(crate::math_migration::plan(asset_text).map_err(error)?)
        } else {
            None
        };
        let planned = migration
            .as_ref()
            .map_or(asset_text, |m| m.content.as_str());
        let mut steps = Vec::<Value>::new();
        if raw != asset_text {
            steps.push(json!({"edits":normalized_edits(raw,asset_edits,asset_text)?}));
        }
        if let Some(m) = &migration {
            if asset_text != m.content {
                steps.push(json!({"edits":normalized_edits(asset_text,&m.edits,&m.content)?}));
            }
        }
        let mut projected = normalize(raw);
        let mut view_steps = Vec::new();
        for step in &steps {
            let edits: Vec<TextEdit> = serde_json::from_value(step["edits"].clone())
                .map_err(|_| error("review_invalid_request"))?;
            projected = tx::apply(
                &projected,
                &ContentTransaction {
                    base_revision: tx::revision(&projected),
                    origin: "review".into(),
                    edits,
                },
            )
            .map_err(error)?
            .content;
            view_steps.push(json!({"edits":step["edits"],"content":projected}));
        }
        let encoding = body["encoding"].as_str().unwrap_or("utf-8");
        let bytes = crate::text_encoding::encode(planned, encoding)
            .map_err(|_| error("encoding_unrepresentable"))?;
        let expected = digest(&bytes);
        let call = review["call_id"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 128)
            .ok_or_else(|| error("review_invalid_request"))?;
        let result=app.review.handle(&json!({
            "action":"prepare_save","document_id":source["document_id"],"generation":review["generation"],
            "review_revision":review["review_revision"],"call_id":call,"content":normalize(raw),
            "disk_revision":before,"expected_disk_revision":expected,"saved_content":normalize(planned),
            "edit_steps":steps,"encoding":encoding,"line_ending":ending(planned),
            "target_path":target.to_string_lossy(),"target_disk_revision":target_before
        })).map_err(error)?;
        let intent = result
            .get("intent_id")
            .or_else(|| result.get("save_intent").and_then(|v| v.get("id")))
            .cloned()
            .ok_or_else(|| error("review_invalid_request"))?;
        Ok(Some(Self {
            app: app.clone(),
            document_id: source["document_id"].clone(),
            intent_id: intent,
            call_id: call.into(),
            expected,
            planned: normalize(planned),
            encoding: encoding.into(),
            line_ending: ending(planned).into(),
            target: target.into(),
            target_before,
            file_committed: false,
            finished: false,
            edit_steps: json!(view_steps),
        }))
    }
    pub fn edit_steps(&self) -> Value {
        self.edit_steps.clone()
    }
    pub fn verify_target(&self) -> ApiResult<()> {
        if disk(&self.target)?.0 != self.target_before {
            return Err(error("review_save_conflict"));
        }
        Ok(())
    }
    pub fn committed(&mut self) -> ApiResult<Value> {
        self.file_committed = true;
        let (actual, _, _) = disk(&self.target)?;
        if actual != self.expected {
            return Err(error("review_needs_recovery"));
        }
        let result=self.app.review.handle(&json!({
            "action":"commit_save","document_id":self.document_id,"intent_id":self.intent_id,
            "call_id":format!("commit:{}",self.call_id),"disk_revision":actual,"content":self.planned,
            "encoding":self.encoding,"line_ending":self.line_ending
        })).map_err(error)?;
        self.finished = true;
        Ok(result)
    }
}
impl Drop for SaveGuard {
    fn drop(&mut self) {
        if !self.finished && !self.file_committed {
            let _ = self.app.review.handle(
                &json!({"action":"abort_save","document_id":self.document_id,
                "intent_id":self.intent_id,"call_id":format!("abort:{}",self.call_id)}),
            );
        }
    }
}

#[cfg(test)]
#[path = "review_api_tests.rs"]
mod integration_tests;

#[cfg(test)]
#[path = "review_rename_api_tests.rs"]
mod rename_integration_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_skill_receipt_replays_after_source_changes_but_never_after_template_changes() {
        let root=tempfile::tempdir().unwrap();
        let assets=root.path().join("assets");std::fs::create_dir_all(&assets).unwrap();
        let app=Arc::new(App::bootstrap(crate::paths::AppPaths::with_dirs(&root.path().join("data"),root.path(),&assets)).unwrap());
        let folder=assets.join("skills/review-replay");std::fs::create_dir_all(&folder).unwrap();
        let write=|body:&str|std::fs::write(folder.join("SKILL.md"),format!("---\nname: review-replay\ndescription: Synthetic replay task\n---\n{body}\n")).unwrap();
        write("Original task {{document}} {{selection}} {{request}} {{language}}");
        let (status,source)=crate::server::call_in_process(&app,"POST","/api/review",Some(&json!({"action":"open","path":"draft:skill-replay","content":"word"})));
        assert_eq!(status,200);
        let body=json!({"document_id":source["document_id"],"generation":source["generation"],"review_revision":source["review_revision"],
            "call_id":"skill-replay-call","request":"Review the selected word.","ranges":[{"from":0,"to":4}],"skill_id":"review-replay","language":"fr"});
        let template=crate::server::review_task_template(&app,&body).unwrap().unwrap();
        let fingerprint=ai_request_digest(&body,body["request"].as_str().unwrap(),Some(&template));
        let committed=app.review.handle(&json!({"action":"propose","document_id":source["document_id"],"generation":source["generation"],
            "review_revision":source["review_revision"],"call_id":"skill-replay-call","origin":"ai",
            "edits":[{"from":0,"to":4,"insert":""}],"ai_request_digest":fingerprint})).unwrap();
        assert_eq!(committed["content"],"");
        let (status,replayed)=crate::server::call_in_process(&app,"POST","/api/review/ai",Some(&body));
        assert_eq!(status,200,"receipt replay must precede rendering the now-empty required document");
        assert_eq!(replayed,committed);
        write("Altered task {{document}} {{selection}} {{request}} {{language}}");
        let (status,changed)=crate::server::call_in_process(&app,"POST","/api/review/ai",Some(&body));
        assert_eq!(status,409);assert_eq!(changed,json!({"ok":false,"error_code":"review_call_conflict"}));
        write("Original task {{document}} {{selection}} {{request}} {{language}}");
        let mut language=body.clone();language["language"]=json!("de");
        let (status,changed)=crate::server::call_in_process(&app,"POST","/api/review/ai",Some(&language));
        assert_eq!(status,409);assert_eq!(changed["error_code"],"review_call_conflict");
        std::fs::write(folder.join("readmd.skill.json"),br#"{"enabled":false}"#).unwrap();
        let (status,disabled)=crate::server::call_in_process(&app,"POST","/api/review/ai",Some(&body));
        assert_eq!(status,400);assert_eq!(disabled,json!({"ok":false,"error_code":"review_invalid_request"}));
        let mut missing=body;missing["skill_id"]=json!("missing-synthetic-skill");
        let (status,missing)=crate::server::call_in_process(&app,"POST","/api/review/ai",Some(&missing));
        assert_eq!(status,400);assert_eq!(missing,json!({"ok":false,"error_code":"review_invalid_request"}));
    }

    #[test]
    fn exact_system_edits_do_not_span_unmodified_review() {
        let raw = "😀\r\n![a](old-a)\r\nKEEP B\r\n![b](old-b)\r\n";
        let a = raw.find("old-a").unwrap();
        let b = raw.find("old-b").unwrap();
        let edits = vec![
            TextEdit {
                from: raw[..a].encode_utf16().count(),
                to: raw[..a + 5].encode_utf16().count(),
                insert: "asset-a".into(),
            },
            TextEdit {
                from: raw[..b].encode_utf16().count(),
                to: raw[..b + 5].encode_utf16().count(),
                insert: "asset-b".into(),
            },
        ];
        let saved = raw.replace("old-a", "asset-a").replace("old-b", "asset-b");
        let mapped = normalized_edits(raw, &edits, &saved).unwrap();
        assert_eq!(mapped.len(), 2);
        assert!(
            mapped[0].to
                < normalize(raw[..raw.find("KEEP B").unwrap()].as_ref())
                    .encode_utf16()
                    .count()
        );
        assert!(normalized_edits(raw, &[], &saved).is_err());
    }
    #[test]
    fn draft_identity_does_not_authorize_paths() {
        assert!(draft_path("draft:tab_123-abc"));
        for bad in [
            "draft:",
            "draft:../../secret",
            "draft:C:\\secret",
            "draft:tab/a",
            "draft:你好",
        ] {
            assert!(!draft_path(bad));
        }
    }
}
