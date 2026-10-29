//! Windows no-clobber publication and conservative recovery. Caller holds the
//! application save lock and the profile gate for the complete operation.
use super::*;
use crate::document_operations::refactor_windows::WindowsLease;
use std::ffi::OsStr;

struct Held {
    original: WindowsLease,
    stage: Option<WindowsLease>,
}
fn field<'a>(file: &'a Value, key: &str) -> ApiResult<&'a str> {
    text(file, key).map_err(review_api::error)
}
fn leaf<'a>(value: &'a str) -> ApiResult<&'a OsStr> {
    Path::new(value)
        .file_name()
        .ok_or_else(|| review_api::error("review_invalid_request"))
}
fn expected(lease: &WindowsLease, file: &Value, stage: bool) -> bool {
    let id = if stage {
        file["stage_identity"].as_str()
    } else {
        file["source_identity"].as_str()
    };
    let revision = if stage {
        file["after_disk_revision"].as_str()
    } else {
        file["before_disk_revision"].as_str()
    };
    id == Some(lease.fingerprint().identity.as_str())
        && revision == Some(lease.fingerprint().revision.as_str())
}
fn changed(file: &Value) -> bool {
    file["before_disk_revision"] != file["after_disk_revision"]
}
fn current(app: &Arc<App>, id: &str) -> ApiResult<Value> {
    app.review
        .refactor_record(id)
        .map_err(review_api::error)?
        .ok_or_else(|| review_api::error("review_persistence_failed"))
}
fn store_state(app: &Arc<App>, id: &str, state: &str) -> ApiResult<Value> {
    app.review
        .refactor_update(id, &json!({"state":state}))
        .map_err(review_api::error)
}
fn absent(path: &Path) -> ApiResult<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(review_api::error("document_operation_target_exists")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(review_api::error("document_operation_storage")),
    }
}

/// The hook exists only in Rust unit tests and is not a process setting or API
/// backdoor. Real-transport acceptance uses OS locks/collisions separately.
fn commit_with(
    app: &Arc<App>,
    plan: &Value,
    mut checkpoint: impl FnMut(usize) -> ApiResult<()>,
) -> ApiResult<Value> {
    let id = field(plan, "operation_id")?;
    let files = plan["files"]
        .as_array()
        .ok_or_else(|| review_api::error("review_invalid_request"))?;
    let mut held = Vec::with_capacity(files.len());
    // Nothing is created until ALL originals and all preconditions are leased.
    for file in files {
        let original =
            WindowsLease::open(Path::new(field(file, "source_path")?)).map_err(native_error)?;
        original
            .verify_metadata_revision(field(file, "source_metadata_revision")?)
            .map_err(native_error)?;
        if !expected(&original, file, false) {
            return Err(review_api::error("document_operation_revision_changed"));
        }
        let raw = crate::text_encoding::encode(field(file, "before")?, field(file, "encoding")?)
            .map_err(|_| review_api::error("rename_unsupported_file"))?;
        if original.bytes() != raw {
            return Err(review_api::error("document_operation_revision_changed"));
        }
        for key in ["stage_path", "backup_path"] {
            absent(Path::new(field(file, key)?))?;
        }
        if file["source_path"] != file["target_path"] {
            absent(Path::new(field(file, "target_path")?))?;
        }
        held.push(Held {
            original,
            stage: None,
        });
    }
    let mut record = app
        .review
        .refactor_prepare(plan)
        .map_err(review_api::error)?;
    if record["state"] != "prepared" {
        drop(held);
        return recover_or_cleanup(app, record);
    }
    let result = (|| {
        for (index, (file, held_file)) in files.iter().zip(held.iter_mut()).enumerate() {
            if !changed(file) {
                continue;
            }
            let bytes =
                crate::text_encoding::encode(field(file, "after")?, field(file, "encoding")?)
                    .map_err(|_| review_api::error("rename_unsupported_file"))?;
            held_file.stage = Some(
                WindowsLease::create_stage(Path::new(field(file, "stage_path")?), &bytes)
                    .map_err(native_error)?,
            );
            // Retain creation ownership if validation fails before journaling.
            let stage = held_file
                .stage
                .as_ref()
                .ok_or_else(|| review_api::error("review_persistence_failed"))?;
            if stage.fingerprint().revision != field(file, "after_disk_revision")? {
                return Err(review_api::error("review_conflict"));
            }
            held_file
                .original
                .verify_replacement(stage)
                .map_err(native_error)?;
            record["plan"]["files"][index]["stage_identity"] =
                json!(stage.fingerprint().identity.as_str());
            // The creating handle stays live across the durable ID write.
            record = app
                .review
                .refactor_update(id, &json!({"plan":record["plan"]}))
                .map_err(review_api::error)?;
        }
        store_state(app, id, "applying")?;
        checkpoint(0)?;
        for (index, (file, held_file)) in files.iter().zip(held.iter_mut()).enumerate() {
            if changed(file) {
                held_file
                    .original
                    .move_to(leaf(field(file, "backup_path")?)?)
                    .map_err(native_error)?;
                checkpoint(index * 2 + 1)?;
                held_file
                    .stage
                    .as_mut()
                    .ok_or_else(|| review_api::error("review_persistence_failed"))?
                    .move_to(leaf(field(file, "target_path")?)?)
                    .map_err(native_error)?;
            } else if file["source_path"] != file["target_path"] {
                held_file
                    .original
                    .move_to(leaf(field(file, "target_path")?)?)
                    .map_err(native_error)?;
            }
            checkpoint(index * 2 + 2)?;
        }
        for (file, held_file) in files.iter().zip(&held) {
            let actual = if changed(file) {
                held_file
                    .stage
                    .as_ref()
                    .ok_or_else(|| review_api::error("review_persistence_failed"))?
            } else {
                &held_file.original
            };
            actual.verify_current().map_err(native_error)?;
            if actual.fingerprint().revision != field(file, "after_disk_revision")? {
                return Err(review_api::error("review_conflict"));
            }
        }
        checkpoint(usize::MAX)?;
        app.review
            .refactor_finish(id, "committed")
            .map_err(review_api::error)?;
        current(app, id)
    })();
    match result {
        Ok(record) => {
            drop(held);
            cleanup(app, record)
        }
        Err(error) => {
            #[cfg(test)]
            eprintln!("synthetic refactor publication failed: {}", error.code);
            // A failed SQLite COMMIT can have an uncertain outcome. Consult
            // the persisted state before choosing any compensating movement.
            let latest = current(app, id)?;
            if latest["state"] == "committed" {
                drop(held);
                return cleanup(app, latest);
            }
            // If a stage's identity could not be journaled, only its still-held
            // creating handle may clean it. Never later delete that name.
            for (index, entry) in held.iter_mut().enumerate() {
                if latest["plan"]["files"][index]["stage_identity"].is_null() {
                    if let Some(stage) = entry.stage.take() {
                        let _ = stage.remove();
                    }
                }
            }
            drop(held);
            let mut recovered = recover_or_cleanup(app, latest)?;
            recovered["failure_code"] = json!(error.code);
            if let Some(receipt) = recovered.get_mut("receipt").filter(|v| v.is_object()) {
                receipt["error_code"] = json!(error.code);
            }
            Ok(recovered)
        }
    }
}
pub(super) fn commit(app: &Arc<App>, plan: &Value) -> ApiResult<Value> {
    commit_with(app, plan, |_| Ok(()))
}

enum Located {
    Missing,
    Owned(WindowsLease),
    Foreign,
}
fn locate(path: &Path, file: &Value, stage: bool) -> Located {
    match WindowsLease::open(path) {
        Ok(lease)
            if expected(&lease, file, stage)
                && lease
                    .verify_metadata_revision(
                        file["source_metadata_revision"].as_str().unwrap_or(""),
                    )
                    .is_ok() =>
        {
            Located::Owned(lease)
        }
        Err(e) if e.kind == ops::ErrorKind::Missing => Located::Missing,
        _ => Located::Foreign,
    }
}
fn remove_owned(path: &Path, file: &Value, stage: bool) -> bool {
    match locate(path, file, stage) {
        Located::Missing => true,
        Located::Owned(lease) => {
            if !stage
                && lease
                    .verify_metadata_revision(
                        file["source_metadata_revision"].as_str().unwrap_or(""),
                    )
                    .is_err()
            {
                return false;
            }
            lease.remove().is_ok()
        }
        Located::Foreign => false,
    }
}
fn rollback_file(file: &Value) -> ApiResult<bool> {
    let source = Path::new(field(file, "source_path")?);
    let target = Path::new(field(file, "target_path")?);
    let backup = Path::new(field(file, "backup_path")?);
    let stage = Path::new(field(file, "stage_path")?);
    let mut original = None;
    let mut original_location = None;
    for path in [source, backup, target] {
        if original_location.is_some_and(|old| old == path) {
            continue;
        }
        if let Located::Owned(lease) = locate(path, file, false) {
            // Only a single strong original is valid; hardlinks are refused by
            // the native lease. Do not infer ownership from equal content.
            if original.is_some() {
                return Ok(false);
            }
            original = Some(lease);
            original_location = Some(path);
        }
    }
    let Some(mut original) = original else {
        #[cfg(test)]
        eprintln!("synthetic rollback original unavailable");
        return Ok(false);
    };
    if original.path() != source {
        match locate(source, file, true) {
            Located::Owned(candidate) => {
                if let Err(error) = candidate.remove() {
                    #[cfg(test)]
                    eprintln!(
                        "synthetic rollback remove published stage: {}",
                        error.code()
                    );
                    return Ok(false);
                }
            }
            Located::Missing => {}
            Located::Foreign => {
                #[cfg(test)]
                eprintln!("synthetic rollback source occupied");
                return Ok(false);
            }
        }
        if let Err(error) = original.move_to(
            source
                .file_name()
                .ok_or_else(|| review_api::error("review_invalid_request"))?,
        ) {
            #[cfg(test)]
            eprintln!("synthetic rollback restore original: {}", error.code());
            return Ok(false);
        }
    }
    if original.verify_current().is_err() {
        return Ok(false);
    }
    // Keeping the original leased pins the restored source while its generated
    // temporary output is removed. Never touch a foreign occupied target.
    let mut clean = true;
    if target != source {
        clean &= remove_owned(target, file, true);
    }
    clean &= remove_owned(stage, file, true);
    // No backup should remain after moving its original back. An unexpected
    // object at that private name is preserved and keeps recovery pending.
    clean &= absent(backup).is_ok();
    Ok(clean)
}

fn cleanup(_app: &Arc<App>, mut record: Value) -> ApiResult<Value> {
    let committed = record["state"] == "committed";
    let files = record["plan"]["files"]
        .as_array()
        .ok_or_else(|| review_api::error("review_persistence_failed"))?;
    let mut pending = false;
    for file in files {
        // A never-registered creation may be an unrelated object now. Keep it
        // and surface its pending cleanup instead of claiming it by pathname.
        let stage = Path::new(field(file, "stage_path")?);
        if file["stage_identity"].is_null() {
            pending |= fs::symlink_metadata(stage).is_ok();
        } else {
            pending |= !remove_owned(stage, file, true);
        }
        let backup = Path::new(field(file, "backup_path")?);
        if committed && changed(file) {
            pending |= !remove_owned(backup, file, false);
        } else {
            pending |= fs::symlink_metadata(backup).is_ok();
        }
    }
    if pending {
        record["receipt"]["cleanup_pending"] = json!(true);
    }
    Ok(record)
}

/// An incomplete operation is rolled back, never silently rolled forward.
/// Recovery cannot rely on the process-local authorization set after restart;
/// its authority is limited to exact identities in this private journal.
pub(super) fn recover_or_cleanup(app: &Arc<App>, record: Value) -> ApiResult<Value> {
    if record["state"] == "committed" || record["state"] == "rolled_back" {
        return cleanup(app, record);
    }
    if record["state"] == "abandoned" {
        return Ok(record);
    }
    let id = field(&record, "operation_id")?;
    store_state(app, id, "rolling_back")?;
    let files = record["plan"]["files"]
        .as_array()
        .ok_or_else(|| review_api::error("review_persistence_failed"))?;
    let mut complete = true;
    // Always attempt other safe files even if one member has a foreign writer.
    for file in files.iter().rev() {
        complete &= rollback_file(file).unwrap_or(false);
    }
    app.review
        .refactor_finish(
            id,
            if complete {
                "rolled_back"
            } else {
                "recovery_pending"
            },
        )
        .map_err(review_api::error)?;
    let record = current(app, id)?;
    if complete {
        cleanup(app, record)
    } else {
        Ok(record)
    }
}

#[cfg(test)]
#[path = "document_refactor_fs_tests.rs"]
mod tests;
