//! Bounded, previewed document rename. Filesystem publication and durable
//! review admission share one protocol; no caller supplies replacement text.
use crate::{
    document_operations as ops,
    document_transaction::TextEdit,
    paths, review_api,
    server::{self, Request, Response},
    ApiError, ApiResult, App,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

#[cfg(windows)]
#[path = "document_refactor_fs.rs"]
mod filesystem;

const MAX_FILES: usize = 4096;
const MAX_SCAN_BYTES: usize = 64 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_PARTICIPANTS: usize = 64;
const MAX_TRANSACTION_BYTES: usize = 12 * 1024 * 1024;

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, &'static str> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or("review_invalid_request")
}
fn valid_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b':' | b'-'))
}
fn path_eq(a: &Path, b: &Path) -> bool {
    paths::path_starts_with(a, b) && paths::path_starts_with(b, a)
}
fn native_error(error: ops::Error) -> ApiError {
    review_api::error(error.code())
}
fn safe_directory(path: &Path) -> ApiResult<PathBuf> {
    let path = ops::validate_directory(path).map_err(native_error)?;
    Ok(path)
}
fn ignored(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    name.starts_with('.')
        || matches!(
            lower.as_str(),
            "node_modules"
                | "target"
                | "dist"
                | "build"
                | "vendor"
                | "readmd-assets"
                | "__pycache__"
                | "uploads"
                | "resource-staging"
                | "history"
                | "capability-packs"
                | "plugins"
                | "export-snapshots"
        )
        || lower.ends_with(".assets")
}
fn document(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| matches!(s.to_ascii_lowercase().as_str(), "md" | "markdown" | "mdown"))
}
fn redirected(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}
fn enumerate(
    root: &Path,
    dir: &Path,
    depth: usize,
    out: &mut Vec<PathBuf>,
    visited: &mut usize,
) -> ApiResult<()> {
    if depth > 32 {
        return Err(review_api::error("review_budget"));
    }
    safe_directory(dir)?;
    let mut entries = fs::read_dir(dir)
        .map_err(|_| review_api::error("document_operation_storage"))?
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|_| review_api::error("document_operation_storage"))?;
    entries.sort_by_key(|entry| entry.file_name());
    *visited = visited.saturating_add(entries.len());
    if *visited > MAX_FILES * 4 {
        return Err(review_api::error("review_budget"));
    }
    if entries.len() > MAX_FILES || out.len().saturating_add(entries.len()) > MAX_FILES * 4 {
        return Err(review_api::error("review_budget"));
    }
    for entry in entries {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Err(review_api::error("rename_unsupported_file"));
        };
        if ignored(name) {
            continue;
        }
        let path = entry.path();
        if !paths::path_starts_with(&path, root) {
            return Err(review_api::error("review_invalid_request"));
        }
        let meta = fs::symlink_metadata(&path)
            .map_err(|_| review_api::error("document_operation_storage"))?;
        if redirected(&meta) {
            return Err(review_api::error("rename_unsupported_file"));
        }
        if meta.is_dir() {
            enumerate(root, &path, depth + 1, out, visited)?;
        } else if meta.is_file() && document(&path) {
            if out.len() >= MAX_FILES || meta.len() > MAX_FILE_BYTES {
                return Err(review_api::error("review_budget"));
            }
            out.push(path);
        }
    }
    Ok(())
}

fn raw_edits(
    source: &str,
    edits: &[crate::document_refactor_links::LinkSourceEdit],
) -> ApiResult<(String, Vec<TextEdit>)> {
    let mut after = source.to_owned();
    for edit in edits.iter().rev() {
        if edit.start > edit.end
            || !source.is_char_boundary(edit.start)
            || !source.is_char_boundary(edit.end)
            || edit.end > source.len()
        {
            return Err(review_api::error("review_invalid_request"));
        }
        after.replace_range(edit.start..edit.end, &edit.replacement);
    }
    let normalized = edits
        .iter()
        .map(|edit| TextEdit {
            from: review_api::normalize(&source[..edit.start])
                .encode_utf16()
                .count(),
            to: review_api::normalize(&source[..edit.end])
                .encode_utf16()
                .count(),
            insert: edit.replacement.clone(),
        })
        .collect::<Vec<_>>();
    let before = review_api::normalize(source);
    let applied = crate::document_transaction::apply(
        &before,
        &crate::document_transaction::ContentTransaction {
            base_revision: crate::document_transaction::revision(&before),
            origin: "workspace".into(),
            edits: normalized.clone(),
        },
    )
    .map_err(review_api::error)?;
    if applied.content != review_api::normalize(&after) {
        return Err(review_api::error("review_conflict"));
    }
    Ok((after, normalized))
}

fn build_plan(app: &Arc<App>, request: &Value) -> ApiResult<Value> {
    let id = text(request, "operation_id").map_err(review_api::error)?;
    if !valid_id(id) {
        return Err(review_api::error("review_invalid_request"));
    }
    let supplied = server::rename_supplied_path(text(request, "path").map_err(review_api::error)?)
        .ok_or_else(|| review_api::error("review_invalid_request"))?;
    let supplied_observation = ops::observe(&supplied, MAX_FILE_BYTES).map_err(native_error)?;
    let ops::Observation::Regular(supplied_file) = supplied_observation else {
        return Err(review_api::error("rename_unsupported_file"));
    };
    let source = paths::canonical_existing(&supplied)
        .map_err(|_| review_api::error("document_operation_missing"))?;
    if !path_eq(&supplied, &source) {
        return Err(review_api::error("rename_unsupported_file"));
    }
    if !document(&source)
        || !server::rename_document_path_allowed(app, &source)
        || server::review_authorized_path(&source.to_string_lossy()).is_none()
    {
        return Err(review_api::error("review_not_authorized"));
    }
    // The explicit workspace parameter is a bounded scope for this confirmed
    // operation. It is never inferred from an independently opened file.
    let root_supplied = PathBuf::from(text(request, "workspace_root").map_err(review_api::error)?);
    if !root_supplied.is_absolute() {
        return Err(review_api::error("review_invalid_request"));
    }
    safe_directory(&root_supplied)?;
    let root = paths::canonical_existing(&root_supplied)
        .map_err(|_| review_api::error("review_invalid_request"))?;
    if !paths::path_starts_with(&source, &root) || path_eq(&source, &root) {
        return Err(review_api::error("review_not_authorized"));
    }
    if source
        .strip_prefix(&root)
        .map_err(|_| review_api::error("review_not_authorized"))?
        .components()
        .any(|c| c.as_os_str().to_str().is_none_or(ignored))
    {
        return Err(review_api::error("review_not_authorized"));
    }
    let extension = source
        .extension()
        .and_then(|s| s.to_str())
        .ok_or_else(|| review_api::error("review_invalid_request"))?;
    let stem = server::validate_rename_stem(
        text(request, "new_stem").map_err(review_api::error)?,
        &format!(".{extension}"),
    )
    .map_err(|_| review_api::error("review_invalid_request"))?;
    let target = source.with_file_name(format!("{stem}.{extension}"));
    if path_eq(&target, &source) {
        return Err(review_api::error("rename_unsupported_file"));
    }
    match fs::symlink_metadata(&target) {
        Ok(_) => return Err(review_api::error("document_operation_target_exists")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(review_api::error("document_operation_storage")),
    }
    let mut names = Vec::new();
    enumerate(&root, &root, 0, &mut names, &mut 0)?;
    names.sort();
    let mut manifest = Vec::new();
    let mut files = Vec::new();
    let mut diagnostics = Vec::new();
    let mut identities = HashSet::new();
    let mut scan_bytes = 0usize;
    let mut transaction_bytes = 0usize;
    let mut found_source = false;
    let mut alias_queries = 0usize;
    for name in names {
        if !server::rename_document_path_allowed(app, &name) {
            return Err(review_api::error("review_not_authorized"));
        }
        let ops::Observation::Regular(observed) =
            ops::observe(&name, MAX_FILE_BYTES).map_err(native_error)?
        else {
            return Err(review_api::error("rename_unsupported_file"));
        };
        let fp = observed.fingerprint();
        if !identities.insert(fp.identity.as_str().to_owned()) {
            return Err(review_api::error("rename_unsupported_file"));
        }
        scan_bytes = scan_bytes.saturating_add(observed.bytes().len());
        if scan_bytes > MAX_SCAN_BYTES {
            return Err(review_api::error("review_budget"));
        }
        manifest.push(json!({"path":name,"identity":fp.identity.as_str(),"revision":fp.revision}));
        let (before, encoding) = crate::text_encoding::detect_and_decode(observed.bytes());
        if crate::text_encoding::encode(&before, encoding)
            .ok()
            .as_deref()
            != Some(observed.bytes())
        {
            return Err(review_api::error("rename_unsupported_file"));
        }
        let links = crate::document_refactor_links::plan_link_updates(
            &before, &name, &source, &target, &root,
        )
        .map_err(|_| review_api::error("review_invalid_request"))?;
        alias_queries = alias_queries.saturating_add(links.local_references.len());
        if alias_queries > 65536 {
            return Err(review_api::error("review_budget"));
        }
        for reference in &links.local_references {
            if !server::rename_document_path_allowed(app, &reference.path)
                || reference.path.strip_prefix(&root).ok().is_none_or(|p| {
                    p.components()
                        .any(|c| c.as_os_str().to_str().is_none_or(ignored))
                })
            {
                continue;
            }
            match fs::symlink_metadata(&reference.path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => return Err(review_api::error("document_operation_storage")),
                Ok(_) => {}
            }
            let observation = match ops::observe_identity(&reference.path) {
                Ok(value) => value,
                Err(_)
                    if fs::symlink_metadata(&reference.path)
                        .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
                {
                    continue
                }
                Err(error) => return Err(native_error(error)),
            };
            match observation {
                ops::IdentityObservation::Regular(identity)
                    if identity == supplied_file.fingerprint().identity =>
                {
                    if !links
                        .edits
                        .iter()
                        .any(|edit| edit.start >= reference.start && edit.end <= reference.end)
                        && !links
                            .diagnostics
                            .iter()
                            .any(|d| d.start < reference.end && reference.start < d.end)
                    {
                        diagnostics.push(json!({"path":name.strip_prefix(&root).unwrap_or(&name),"code":"physical_alias_reference",
                            "from":review_api::normalize(&before[..reference.start]).encode_utf16().count(),
                            "to":review_api::normalize(&before[..reference.end]).encode_utf16().count()}));
                    }
                }
                ops::IdentityObservation::Unsupported(ops::UnsupportedReason::NotRegularFile) => {}
                ops::IdentityObservation::Unsupported(_) => {
                    return Err(review_api::error("rename_unsupported_file"))
                }
                _ => {}
            }
        }
        for diagnostic in links.diagnostics {
            diagnostics.push(
                json!({"path":name.strip_prefix(&root).unwrap_or(&name),"code":diagnostic.code,
                "from":review_api::normalize(&before[..diagnostic.start]).encode_utf16().count(),
                "to":review_api::normalize(&before[..diagnostic.end]).encode_utf16().count()}),
            );
        }
        let is_source = path_eq(&name, &source);
        if is_source {
            found_source = true;
            if fp != supplied_file.fingerprint() {
                return Err(review_api::error("document_operation_identity_changed"));
            }
        }
        if !is_source && links.edits.is_empty() {
            continue;
        }
        if review_api::pending_rename(app, &name)?.is_some() {
            return Err(review_api::error("rename_recovery_pending"));
        }
        let (after, edits) = raw_edits(&before, &links.edits)?;
        let encoded = crate::text_encoding::encode(&after, encoding)
            .map_err(|_| review_api::error("rename_unsupported_file"))?;
        if review_api::normalize(&before).len() > crate::review::MAX_DOCUMENT_BYTES
            || review_api::normalize(&after).len() > crate::review::MAX_DOCUMENT_BYTES
            || encoded.len() as u64 > MAX_FILE_BYTES
        {
            return Err(review_api::error("review_budget"));
        }
        transaction_bytes = transaction_bytes.saturating_add(before.len() + after.len());
        if transaction_bytes > MAX_TRANSACTION_BYTES || files.len() >= MAX_PARTICIPANTS {
            return Err(review_api::error("review_budget"));
        }
        #[cfg(windows)]
        let source_metadata_revision = {
            let lease = ops::refactor_windows::WindowsLease::open(&name).map_err(native_error)?;
            if lease.fingerprint() != fp {
                return Err(review_api::error("document_operation_identity_changed"));
            }
            lease.metadata_revision()
        };
        #[cfg(not(windows))]
        let source_metadata_revision = String::new();
        let index = files.len();
        let nonce = &hash(id.as_bytes())[..24];
        let parent = name
            .parent()
            .ok_or_else(|| review_api::error("review_invalid_request"))?;
        let stage = parent.join(format!(".readmd-refactor-{nonce}-{index}.tmp"));
        let backup = parent.join(format!(".readmd-refactor-{nonce}-{index}.bak"));
        files.push(json!({"source_path":name,"target_path":if is_source{target.clone()}else{name.clone()},
            "before":before,"after":after,"encoding":encoding,"line_ending":if before.contains("\r\n"){"crlf"}else if before.contains('\r'){"cr"}else{"lf"},
            "edits":edits,"before_disk_revision":fp.revision,"after_disk_revision":hash(&encoded),
            "source_identity":fp.identity.as_str(),"source_metadata_revision":source_metadata_revision,"stage_identity":Value::Null,"stage_path":stage,"backup_path":backup}));
    }
    if !found_source {
        return Err(review_api::error("review_not_authorized"));
    }
    let participant_paths = files
        .iter()
        .map(|v| v["source_path"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let heads = app
        .review
        .refactor_heads(&participant_paths)
        .map_err(review_api::error)?;
    for (file, head) in files.iter_mut().zip(heads) {
        file["review_head"] = head;
    }
    let mut plan = json!({"version":1,"operation_id":id,"workspace_root":root,"source_path":source,"target_path":target,
        "scan_digest":hash(&serde_json::to_vec(&manifest).map_err(|_|review_api::error("review_invalid_request"))?),
        "scan_files":manifest.len(),"scan_bytes":scan_bytes,"files":files,"diagnostics":diagnostics});
    plan["plan_hash"] = json!(hash(
        &serde_json::to_vec(&plan).map_err(|_| review_api::error("review_invalid_request"))?
    ));
    Ok(plan)
}

fn preview(plan: &Value) -> Value {
    json!({"ok":true,"operation_id":plan["operation_id"],"plan_hash":plan["plan_hash"],
        "source_path":plan["source_path"],"target_path":plan["target_path"],"files":plan["files"],
        "diagnostics":plan["diagnostics"],"scan_files":plan["scan_files"]})
}

// The expensive syntax plan is computed outside the gate. Re-read the bounded
// workspace membership and raw revisions under the gate so a ReadMD writer
// cannot introduce a reference while confirmation waited for another save.
// An unrelated external process can still edit nonparticipants afterwards;
// this is a checked workspace snapshot, not a filesystem-wide atomic lock.
fn verify_manifest(app: &Arc<App>, plan: &Value) -> ApiResult<()> {
    let root = Path::new(text(plan, "workspace_root").map_err(review_api::error)?);
    let mut names = Vec::new();
    enumerate(root, root, 0, &mut names, &mut 0)?;
    names.sort();
    let mut manifest = Vec::with_capacity(names.len());
    let mut bytes = 0usize;
    for name in names {
        if !server::rename_document_path_allowed(app, &name) {
            return Err(review_api::error("review_not_authorized"));
        }
        let ops::Observation::Regular(observed) =
            ops::observe(&name, MAX_FILE_BYTES).map_err(native_error)?
        else {
            return Err(review_api::error("review_conflict"));
        };
        bytes = bytes.saturating_add(observed.bytes().len());
        if bytes > MAX_SCAN_BYTES {
            return Err(review_api::error("review_budget"));
        }
        manifest.push(json!({"path":name,"identity":observed.fingerprint().identity.as_str(),"revision":observed.fingerprint().revision}));
    }
    if plan["scan_digest"].as_str()
        != Some(
            hash(
                &serde_json::to_vec(&manifest)
                    .map_err(|_| review_api::error("review_invalid_request"))?,
            )
            .as_str(),
        )
    {
        return Err(review_api::error("review_conflict"));
    }
    Ok(())
}
fn request_matches(record: &Value, request: &Value) -> ApiResult<()> {
    if record["plan_hash"] != request["plan_hash"] {
        return Err(review_api::error("review_call_conflict"));
    }
    if request["action"] == "commit" && !record["plan"].is_null() {
        let plan = &record["plan"];
        let p = server::rename_supplied_path(text(request, "path").map_err(review_api::error)?)
            .ok_or_else(|| review_api::error("review_invalid_request"))?;
        let root = PathBuf::from(text(request, "workspace_root").map_err(review_api::error)?);
        let target = PathBuf::from(text(plan, "target_path").map_err(review_api::error)?);
        let source = PathBuf::from(text(plan, "source_path").map_err(review_api::error)?);
        let extension = source.extension().and_then(|s| s.to_str()).unwrap_or("");
        let stem = server::validate_rename_stem(
            text(request, "new_stem").map_err(review_api::error)?,
            &format!(".{extension}"),
        )
        .map_err(|_| review_api::error("review_invalid_request"))?;
        if !path_eq(&paths::canonicalize_or_clean(&p), &source)
            || !path_eq(
                &paths::canonicalize_or_clean(&root),
                Path::new(text(plan, "workspace_root").map_err(review_api::error)?),
            )
            || source.with_file_name(format!("{stem}.{extension}")) != target
        {
            return Err(review_api::error("review_call_conflict"));
        }
    }
    Ok(())
}

fn settled(_app: &Arc<App>, record: Value) -> ApiResult<Value> {
    let mut receipt=record.get("receipt").filter(|v|v.is_object()).cloned().unwrap_or_else(||json!({
        "ok":false,"state":record["state"],"operation_id":record["operation_id"],"plan_hash":record["plan_hash"],
        "path":record["plan"]["source_path"],"target_path":record["plan"]["target_path"],"error_code":"rename_recovery_pending"}));
    if receipt["state"] == "committed" || receipt["state"] == "rolled_back" {
        if let Some(files) = receipt["files"].as_array() {
            for file in files {
                let old = Path::new(text(file, "old_path").map_err(review_api::error)?);
                let current = Path::new(text(file, "path").map_err(review_api::error)?);
                if !path_eq(old, current) {
                    server::forget_authorized_save(old);
                }
                // An actual open later verifies bytes/review; a receipt alone is
                // never allowed to authorize an object with a recycled name.
                if let Ok(ops::Observation::Regular(actual)) = ops::observe(current, MAX_FILE_BYTES)
                {
                    let expected_id = if receipt["state"] == "committed" {
                        file.get("identity").or_else(|| file.get("source_identity"))
                    } else {
                        file.get("source_identity")
                    };
                    if actual.fingerprint().revision == file["disk_revision"].as_str().unwrap_or("")
                        && expected_id.and_then(Value::as_str)
                            == Some(actual.fingerprint().identity.as_str())
                    {
                        let (decoded, _) = crate::text_encoding::detect_and_decode(actual.bytes());
                        server::remember_authorized_save_verified(
                            current,
                            actual.fingerprint().identity.clone(),
                            hash(decoded.as_bytes()),
                        );
                    }
                }
            }
        }
    }
    receipt["ok"] = json!(receipt["state"] == "committed");
    if let Some(source) = receipt
        .get("files")
        .and_then(Value::as_array)
        .and_then(|files| {
            files
                .iter()
                .find(|f| f["old_path"] == record["plan"]["source_path"])
        })
    {
        let path = source["path"].clone();
        receipt["path"] = path;
        receipt["target_path"] = record["plan"]["target_path"].clone();
    }
    Ok(receipt)
}

/// Shared desktop/MCP service. All mutations remain on the authenticated local
/// transport; explicit scope + immutable preview must precede confirmation.
pub fn execute(app: &Arc<App>, request: &Value) -> ApiResult<Value> {
    let action = text(request, "action").map_err(review_api::error)?;
    let id = text(request, "operation_id").map_err(review_api::error)?;
    if !valid_id(id) {
        return Err(review_api::error("review_invalid_request"));
    }
    if action == "preview" {
        return build_plan(app, request).map(|plan| preview(&plan));
    }
    let plan_hash = text(request, "plan_hash").map_err(review_api::error)?;
    if !valid_hash(plan_hash)
        || !matches!(action, "commit" | "status" | "abandon")
        || (action == "commit" && request["confirmed"] != true)
    {
        return Err(review_api::error("review_invalid_request"));
    }
    // A retry must consult the durable operation before opening a source which
    // may already have moved. Rechecked again under the cross-process gate.
    let previous = app.review.refactor_record(id).map_err(review_api::error)?;
    let plan = if action == "commit" && previous.is_none() {
        let candidate = build_plan(app, request)?;
        if candidate["plan_hash"] != plan_hash {
            return Err(review_api::error("review_conflict"));
        }
        if candidate["diagnostics"]
            .as_array()
            .is_none_or(|v| !v.is_empty())
        {
            return Err(review_api::error("review_conflict"));
        }
        Some(candidate)
    } else {
        None
    };
    let _save = crate::document_history::SAVE_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let gate = ops::acquire(&app.paths.data_dir, Duration::from_secs(3)).map_err(native_error)?;
    gate.verify_profile(&app.paths.data_dir)
        .map_err(native_error)?;
    if let Some(current) = app.review.refactor_record(id).map_err(review_api::error)? {
        request_matches(&current, request)?;
        #[cfg(windows)]
        let current = filesystem::recover_or_cleanup(app, current)?;
        return settled(app, current);
    }
    if action == "status" {
        return Ok(
            json!({"ok":false,"state":"not_started","operation_id":id,"plan_hash":plan_hash}),
        );
    }
    if action == "abandon" {
        return settled(
            app,
            app.review
                .refactor_abandon(id, plan_hash)
                .map_err(review_api::error)?,
        );
    }
    let plan = plan.ok_or_else(|| review_api::error("review_conflict"))?;
    verify_manifest(app, &plan)?;
    #[cfg(windows)]
    {
        settled(app, filesystem::commit(app, &plan)?)
    }
    #[cfg(not(windows))]
    {
        let _ = plan;
        Err(review_api::error("document_operation_unsupported"))
    }
}

pub fn endpoint(app: &Arc<App>, req: &Request) -> ApiResult<Response> {
    if req.method != "POST" {
        return Err(ApiError::new(405, "method_not_allowed"));
    }
    execute(app, &req.json()?).and_then(server::ok_json)
}

#[cfg(test)]
#[path = "document_refactor_scope_tests.rs"]
mod scope_tests;

#[cfg(test)]
#[path = "document_refactor_tests.rs"]
mod tests;
