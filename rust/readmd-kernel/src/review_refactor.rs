//! Review participation in a journaled document refactor. Filesystem evidence,
//! authorization and native leases belong to the trusted document owner.
use super::*;
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};

const MAX_REFACTOR_FILES: usize = 128;
const MAX_REFACTOR_RECORDS: i64 = 4096;
const MAX_PENDING_REFACTORS: i64 = 64;

#[derive(Clone, Deserialize)]
struct RefactorFile {
    source_path: String,
    target_path: String,
    before: String,
    after: String,
    before_disk_revision: String,
    after_disk_revision: String,
    encoding: String,
    line_ending: String,
    edits: Vec<TextEdit>,
    review_head: Option<Value>,
    stage_path: String,
    backup_path: String,
    source_identity: String,
    stage_identity: Option<String>,
}

#[derive(Deserialize)]
struct RefactorPlan {
    operation_id: String,
    plan_hash: String,
    workspace_root: String,
    source_path: String,
    target_path: String,
    files: Vec<RefactorFile>,
}

fn refactor_head(document: &Document) -> Value {
    json!({"document_id":document.document_id,"generation":document.generation,
        "revision":document.revision,"review_revision":document.review_revision,
        "disk_revision":document.disk_revision})
}

fn identity_string(value: &str) -> Result<(), &'static str> {
    if value.is_empty()
        || value.len() > 256
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'-' | b'_'))
    {
        return Err("review_invalid_request");
    }
    Ok(())
}

fn absolute_path(value: &str) -> Result<(), &'static str> {
    path(value)?;
    if value.starts_with("draft:") || !Path::new(value).is_absolute() {
        return Err("review_invalid_request");
    }
    Ok(())
}

fn equal_path(left: &str, right: &str) -> bool {
    // The native owner supplies canonical Windows paths. The reservation table
    // also uses NOCASE; an alternate spelling must not create another session.
    left.eq_ignore_ascii_case(right)
}

fn same_parent(left: &str, right: &str) -> bool {
    match (Path::new(left).parent(), Path::new(right).parent()) {
        (Some(left), Some(right)) => left
            .to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy()),
        _ => false,
    }
}

fn parse_plan(value: &Value) -> Result<RefactorPlan, &'static str> {
    let files = value["files"].as_array().ok_or("review_invalid_request")?;
    if files.is_empty() || files.len() > MAX_REFACTOR_FILES {
        return Err("review_budget");
    }
    let mut text_bytes = 0usize;
    for file in files {
        for field in ["before", "after"] {
            let content = string(file, field)?;
            text_bytes = text_bytes
                .checked_add(content.len())
                .ok_or("review_budget")?;
            if content.len() > MAX_DOCUMENT_BYTES || text_bytes > 32 * 1024 * 1024 {
                return Err("review_budget");
            }
        }
    }
    let plan: RefactorPlan =
        serde_json::from_value(value.clone()).map_err(|_| "review_invalid_request")?;
    rename_id(&plan.operation_id)?;
    if !rename_hash(&plan.plan_hash) {
        return Err("review_invalid_request");
    }
    for value in [&plan.workspace_root, &plan.source_path, &plan.target_path] {
        absolute_path(value)?;
    }
    if equal_path(&plan.source_path, &plan.target_path)
        || !same_parent(&plan.source_path, &plan.target_path)
        || plan.files.is_empty()
        || plan.files.len() > MAX_REFACTOR_FILES
    {
        return Err("review_invalid_request");
    }
    let mut source_paths = HashSet::new();
    let mut target_paths = HashSet::new();
    let mut private_paths = HashSet::new();
    let mut operations = 0usize;
    let mut has_source = false;
    for file in &plan.files {
        for value in [
            &file.source_path,
            &file.target_path,
            &file.stage_path,
            &file.backup_path,
        ] {
            absolute_path(value)?;
            if !same_parent(&file.source_path, value) {
                return Err("review_invalid_request");
            }
        }
        if !source_paths.insert(file.source_path.to_ascii_lowercase())
            || !target_paths.insert(file.target_path.to_ascii_lowercase())
            || !private_paths.insert(file.stage_path.to_ascii_lowercase())
            || !private_paths.insert(file.backup_path.to_ascii_lowercase())
        {
            return Err("review_invalid_request");
        }
        if equal_path(&file.source_path, &plan.source_path) {
            if !equal_path(&file.target_path, &plan.target_path) {
                return Err("review_invalid_request");
            }
            has_source = true;
        } else if !equal_path(&file.source_path, &file.target_path) {
            return Err("review_invalid_request");
        }
        if !rename_hash(&file.before_disk_revision) || !rename_hash(&file.after_disk_revision) {
            return Err("review_invalid_request");
        }
        identity_string(&file.source_identity)?;
        if let Some(identity) = &file.stage_identity {
            identity_string(identity)?;
        }
        let encoding =
            crate::text_encoding::canonical_name(&file.encoding).ok_or("review_invalid_request")?;
        if !matches!(
            file.line_ending.to_ascii_lowercase().as_str(),
            "lf" | "crlf" | "cr"
        ) {
            return Err("review_invalid_request");
        }
        let before = normalize(&file.before);
        let after = normalize(&file.after);
        checked_content(&before)?;
        checked_content(&after)?;
        if file.edits.iter().any(|edit| edit.insert.contains('\r')) {
            return Err("review_invalid_request");
        }
        operations = operations
            .checked_add(file.edits.len())
            .ok_or("review_budget")?;
        if operations > MAX_OPERATIONS {
            return Err("review_budget");
        }
        if apply(&before, &file.edits)?.content != after {
            return Err("review_save_conflict");
        }
        for (content, expected) in [
            (&file.before, &file.before_disk_revision),
            (&file.after, &file.after_disk_revision),
        ] {
            let bytes = crate::text_encoding::encode(content, encoding)
                .map_err(|_| "encoding_unrepresentable")?;
            if format!("{:x}", Sha256::digest(&bytes)) != *expected {
                return Err("review_save_conflict");
            }
        }
    }
    if !has_source
        || private_paths
            .iter()
            .any(|value| source_paths.contains(value) || target_paths.contains(value))
    {
        return Err("review_invalid_request");
    }
    Ok(plan)
}

fn immutable_plan(value: &Value) -> Value {
    let mut value = value.clone();
    if let Some(files) = value.get_mut("files").and_then(Value::as_array_mut) {
        for file in files {
            if let Some(file) = file.as_object_mut() {
                file.remove("stage_identity");
            }
        }
    }
    canonical(&value)
}

fn document_at(tx: &Transaction<'_>, requested: &str) -> Result<Option<Document>, &'static str> {
    let mut statement = tx
        .prepare("SELECT path FROM review_documents WHERE path=?1 COLLATE NOCASE OR pending_path=?1 COLLATE NOCASE")
        .map_err(|_| "review_persistence_failed")?;
    let paths = statement
        .query_map([requested], |row| row.get::<_, String>(0))
        .map_err(|_| "review_persistence_failed")?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|_| "review_persistence_failed")?;
    if paths.len() > 1 {
        return Err("review_save_conflict");
    }
    let Some(actual) = paths.first() else {
        return Ok(None);
    };
    let payload = review_store::by_path(tx, actual)?.ok_or("review_persistence_failed")?;
    let document = decode(tx, &payload)?;
    if !equal_path(&document.path, requested) {
        return Err("review_save_conflict");
    }
    Ok(Some(document))
}

fn check_head(document: &Document, expected: &Value) -> Result<(), &'static str> {
    if expected["document_id"] != document.document_id {
        return Err("review_save_conflict");
    }
    cas(document, expected)?;
    if expected["revision"] != document.revision
        || expected["disk_revision"] != document.disk_revision
    {
        return Err("review_save_conflict");
    }
    Ok(())
}

fn touches(from: usize, to: usize, edit: &TextEdit) -> bool {
    if from == to {
        edit.from <= from && from <= edit.to
    } else if edit.from == edit.to {
        from <= edit.from && edit.from <= to
    } else {
        from < edit.to && edit.from < to
    }
}

fn overlap(changes: &[Change], edits: &[TextEdit]) -> bool {
    changes.iter().any(|change| {
        (matches!(change.decision.as_str(), "pending" | "conflict")
            || change.application == "proposed")
            && edits
                .iter()
                .any(|edit| touches(change.from, change.to, edit))
    })
}

fn check_history(document: &Document, edits: &[TextEdit]) -> Result<(), &'static str> {
    if edits.is_empty() {
        return Ok(());
    }
    if overlap(&document.changes, edits) || overlap(&document.checkpoint.changes, edits) {
        return Err("refactor_review_overlap");
    }
    for action in document.undo.iter().chain(&document.redo) {
        for inverse in &action.inverse {
            let (mut from, mut to) = (inverse.from, inverse.to);
            for mapping in document
                .mappings
                .iter()
                .filter(|mapping| mapping.event_id > action.event_id)
            {
                map_geometry(&mut from, &mut to, &mapping.edits);
            }
            if edits.iter().any(|edit| touches(from, to, edit)) {
                return Err("refactor_undo_overlap");
            }
        }
        // Decision-only undo has no text inverse, but its saved anchor patches
        // may otherwise restore a candidate version with an obsolete link.
        for patch in &action.patches {
            for (change, include_event) in [(&patch.before, true), (&patch.after, false)] {
                let Some(change) = change else { continue };
                let (mut from, mut to) = (change.from, change.to);
                for mapping in document.mappings.iter().filter(|mapping| {
                    if include_event {
                        mapping.event_id >= action.event_id
                    } else {
                        mapping.event_id > action.event_id
                    }
                }) {
                    map_geometry(&mut from, &mut to, &mapping.edits);
                }
                if edits.iter().any(|edit| touches(from, to, edit)) {
                    return Err("refactor_undo_overlap");
                }
            }
        }
    }
    Ok(())
}

fn available_target(
    tx: &Transaction<'_>,
    target: &str,
    document: &Document,
) -> Result<(), &'static str> {
    let count: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM review_documents WHERE (path=?1 COLLATE NOCASE OR pending_path=?1 COLLATE NOCASE) AND id<>?2",
            params![target, document.document_id],
            |row| row.get(0),
        )
        .map_err(|_| "review_persistence_failed")?;
    if count != 0 || !review_store::path_available(tx, target, &document.document_id)? {
        return Err("review_save_conflict");
    }
    Ok(())
}

fn check_document(
    tx: &Transaction<'_>,
    document: &Document,
    file: &RefactorFile,
) -> Result<(), &'static str> {
    if !equal_path(&document.path, &file.source_path) {
        return Err("review_save_conflict");
    }
    if document.rename_intent.is_some() {
        return Err("rename_recovery_pending");
    }
    if document.save_intent.is_some() {
        return Err("review_save_in_progress");
    }
    if document.recovery.is_some() {
        return Err("review_needs_recovery");
    }
    let before = normalize(&file.before);
    if document.disk_revision != file.before_disk_revision
        || document.content != before
        || document.checkpoint.content != before
    {
        return Err("review_save_conflict");
    }
    available_target(tx, &file.target_path, document)?;
    check_history(document, &file.edits)
}

fn record(tx: &Transaction<'_>, id: &str) -> Result<Option<Value>, &'static str> {
    let payload: Option<(String, String, String)> = tx
        .query_row(
            "SELECT plan_hash,state,payload FROM review_refactors WHERE operation_id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|_| "review_persistence_failed")?;
    payload
        .map(|(plan_hash, state, payload)| {
            let value: Value =
                serde_json::from_str(&payload).map_err(|_| "review_persistence_failed")?;
            if value["operation_id"] != id
                || value["plan_hash"] != plan_hash
                || value["state"] != state
                || !value["plan_hash"].as_str().is_some_and(rename_hash)
                || !matches!(
                    value["state"].as_str(),
                    Some(
                        "prepared"
                            | "applying"
                            | "rolling_back"
                            | "recovery_pending"
                            | "committed"
                            | "rolled_back"
                            | "abandoned"
                    )
                )
            {
                return Err("review_persistence_failed");
            }
            Ok(value)
        })
        .transpose()
}

fn put_record(tx: &Transaction<'_>, value: &Value, reserved: usize) -> Result<(), &'static str> {
    let payload = serde_json::to_string(value).map_err(|_| "review_persistence_failed")?;
    let reserved = i64::try_from(reserved).map_err(|_| "review_budget")?;
    tx.execute(
        "INSERT INTO review_refactors(operation_id,plan_hash,state,payload,reserved_bytes) VALUES(?1,?2,?3,?4,?5)
         ON CONFLICT(operation_id) DO UPDATE SET state=excluded.state,payload=excluded.payload,reserved_bytes=excluded.reserved_bytes",
        params![string(value,"operation_id")?, string(value,"plan_hash")?,string(value,"state")?,payload,reserved],
    )
    .map_err(|_| "review_persistence_failed")?;
    Ok(())
}

fn reservation(tx: &Transaction<'_>, id: &str) -> Result<usize, &'static str> {
    let value: i64 = tx
        .query_row(
            "SELECT reserved_bytes FROM review_refactors WHERE operation_id=?1",
            [id],
            |row| row.get(0),
        )
        .map_err(|_| "review_persistence_failed")?;
    usize::try_from(value).map_err(|_| "review_persistence_failed")
}

fn update_reserved(
    tx: &Transaction<'_>,
    value: &Value,
    old_used: usize,
    reserved: usize,
) -> Result<(), &'static str> {
    put_record(tx, value, reserved)?;
    let growth = review_store::used_bytes(tx)?.saturating_sub(old_used);
    let remaining = reserved.checked_sub(growth).ok_or("review_budget")?;
    put_record(tx, value, remaining)
}

fn terminal(value: &Value) -> bool {
    matches!(
        value["state"].as_str(),
        Some("committed" | "rolled_back" | "abandoned")
    )
}

fn all_paths(plan: &RefactorPlan) -> Vec<&str> {
    let mut seen = HashSet::new();
    plan.files
        .iter()
        .flat_map(|file| {
            [
                file.source_path.as_str(),
                file.target_path.as_str(),
                file.stage_path.as_str(),
                file.backup_path.as_str(),
            ]
        })
        .filter(|value| seen.insert(value.to_ascii_lowercase()))
        .collect()
}

fn clear_paths(tx: &Transaction<'_>, id: &str) -> Result<(), &'static str> {
    tx.execute(
        "DELETE FROM review_refactor_paths WHERE operation_id=?1",
        [id],
    )
    .map_err(|_| "review_persistence_failed")?;
    Ok(())
}

fn check_paths(tx: &Transaction<'_>, plan: &RefactorPlan) -> Result<(), &'static str> {
    for path in all_paths(plan) {
        let owner: Option<String> = tx
            .query_row(
                "SELECT operation_id FROM review_refactor_paths WHERE path=?1 COLLATE NOCASE",
                [path],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| "review_persistence_failed")?;
        if owner.as_deref() != Some(plan.operation_id.as_str()) {
            return Err("review_persistence_failed");
        }
    }
    Ok(())
}

fn project_document(
    tx: &Transaction<'_>,
    document: &mut Document,
    file: &RefactorFile,
    operation_id: &str,
) -> Result<(), &'static str> {
    let checkpoint = document.checkpoint.clone();
    if !file.edits.is_empty() {
        let prior_review_revision = document.review_revision;
        let before_content = document.content.clone();
        let before_changes = document.changes.clone();
        let result = apply(&before_content, &file.edits)?;
        map_changes(document, &file.edits, &result.content, true)?;
        document.content = result.content;
        record_action(
            tx,
            document,
            "refactor_rewrite",
            "system",
            file.edits.clone(),
            &before_content,
            before_changes,
            None,
        )?;
        // The saved checkpoint carries its OWN decisions and candidates. It
        // must not adopt a later, unsaved accept/withdraw decision just because
        // a link was rewritten elsewhere in the file.
        let mut saved = document.clone();
        saved.review_revision = prior_review_revision;
        saved.changes = checkpoint.changes;
        let saved_result = apply(&checkpoint.content, &file.edits)?;
        map_changes(&mut saved, &file.edits, &saved_result.content, true)?;
        document.checkpoint.content = saved_result.content;
        document.checkpoint.changes = saved.changes;
        // Do not push a global content undo and do not clear existing stacks.
        // Admission proved that their ranges are disjoint from these edits.
        document.checkpoint.event_id = document.last_event_id;
    }
    if document.content != normalize(&file.after) {
        return Err("review_save_conflict");
    }
    document.path = file.target_path.clone();
    document.disk_revision = file.after_disk_revision.clone();
    document.encoding = file.encoding.clone();
    document.line_ending = file.line_ending.to_ascii_lowercase();
    let event = next_event(document)?;
    review_store::event(
        tx,
        &document.document_id,
        event,
        "commit_refactor",
        &json!({"operation_id":operation_id,"source_path":file.source_path,
            "target_path":file.target_path,"disk_revision":file.after_disk_revision}),
    )?;
    Ok(())
}

fn receipt_file(document: &Document, file: &RefactorFile, committed: bool) -> Value {
    json!({"old_path":file.source_path,
        "path":if committed{&file.target_path}else{&file.source_path},
        "before_disk_revision":file.before_disk_revision,
        "disk_revision":if committed{&file.after_disk_revision}else{&file.before_disk_revision},
        "source_identity":file.source_identity,
        "identity":if committed && file.before_disk_revision != file.after_disk_revision {
            file.stage_identity.as_deref()
        }else{Some(file.source_identity.as_str())},
        "review":snapshot(document)})
}

fn project_commit(
    tx: &Transaction<'_>,
    journal: &Value,
    mut peak: usize,
) -> Result<(Value, usize), &'static str> {
    let plan = parse_plan(&journal["plan"])?;
    check_paths(tx, &plan)?;
    let heads = journal["heads"]
        .as_array()
        .filter(|heads| heads.len() == plan.files.len())
        .ok_or("review_persistence_failed")?;
    let mut files = Vec::with_capacity(plan.files.len());
    for (file, head) in plan.files.iter().zip(heads) {
        let mut document = load(tx, string(head, "document_id")?)?;
        check_head(&document, head)?;
        check_document(tx, &document, file)?;
        project_document(tx, &mut document, file, &plan.operation_id)?;
        peak = peak.max(review_store::used_bytes(tx)?);
        persist(tx, &document)?;
        peak = peak.max(review_store::used_bytes(tx)?);
        files.push(receipt_file(&document, file, true));
    }
    let receipt = json!({"state":"committed","operation_id":plan.operation_id,
        "plan_hash":plan.plan_hash,"files":files});
    let mut completed = journal.clone();
    completed["state"] = json!("committed");
    completed["receipt"] = receipt.clone();
    clear_paths(tx, &plan.operation_id)?;
    put_record(tx, &completed, 0)?;
    peak = peak.max(review_store::used_bytes(tx)?);
    Ok((receipt, peak))
}

fn project_rollback(tx: &Transaction<'_>, journal: &Value) -> Result<(Value, usize), &'static str> {
    let plan = parse_plan(&journal["plan"])?;
    check_paths(tx, &plan)?;
    let heads = journal["heads"]
        .as_array()
        .filter(|heads| heads.len() == plan.files.len())
        .ok_or("review_persistence_failed")?;
    let mut files = Vec::with_capacity(plan.files.len());
    for (file, head) in plan.files.iter().zip(heads) {
        let document = load(tx, string(head, "document_id")?)?;
        check_head(&document, head)?;
        files.push(receipt_file(&document, file, false));
    }
    let receipt = json!({"state":"rolled_back","operation_id":plan.operation_id,
        "plan_hash":plan.plan_hash,"files":files});
    let mut completed = journal.clone();
    completed["state"] = json!("rolled_back");
    completed["receipt"] = receipt.clone();
    clear_paths(tx, &plan.operation_id)?;
    put_record(tx, &completed, 0)?;
    Ok((receipt, review_store::used_bytes(tx)?))
}

impl Service {
    /// Ordered private heads only. An absent store need not be created just to
    /// preview ordinary saved Markdown files.
    pub(crate) fn refactor_heads(&self, paths: &[String]) -> Result<Vec<Value>, &'static str> {
        if paths.len() > MAX_REFACTOR_FILES {
            return Err("review_budget");
        }
        for value in paths {
            absolute_path(value)?;
        }
        if !self.store.path.is_file() {
            return Ok(vec![Value::Null; paths.len()]);
        }
        self.store.run(|tx| {
            with_text_context(tx, || {
                paths
                    .iter()
                    .map(|path| {
                        Ok(document_at(tx, path)?
                            .as_ref()
                            .map(refactor_head)
                            .unwrap_or(Value::Null))
                    })
                    .collect()
            })
        })
    }

    /// Admission, all clean sessions, all path reservations and completion
    /// headroom become durable in ONE SQLite transaction, before native I/O.
    pub(crate) fn refactor_prepare(&self, value: &Value) -> Result<Value, &'static str> {
        let plan = parse_plan(value)?;
        self.store.run(|tx| {
            with_text_context(tx, || {
                if let Some(existing) = record(tx, &plan.operation_id)? {
                    if existing["plan_hash"] != plan.plan_hash {
                        return Err("review_call_conflict");
                    }
                    if existing["state"] != "abandoned"
                        && immutable_plan(&existing["plan"]) != immutable_plan(value)
                    {
                        return Err("review_call_conflict");
                    }
                    return Ok(existing);
                }
                let (count, pending): (i64, i64) = tx
                    .query_row(
                        "SELECT COUNT(*),COALESCE(SUM(state NOT IN ('committed','rolled_back','abandoned')),0) FROM review_refactors",
                        [], |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .map_err(|_| "review_persistence_failed")?;
                if count >= MAX_REFACTOR_RECORDS || pending >= MAX_PENDING_REFACTORS {
                    return Err("review_budget");
                }
                for path in all_paths(&plan) {
                    if review_store::pending_rename_at(tx, path)?.is_some() {
                        return Err("rename_recovery_pending");
                    }
                }
                let mut heads = Vec::with_capacity(plan.files.len());
                for file in &plan.files {
                    let document = match (document_at(tx, &file.source_path)?, &file.review_head) {
                        (Some(document), Some(expected)) => {
                            check_head(&document, expected)?;
                            document
                        }
                        (Some(_), None) | (None, Some(_)) => return Err("review_save_conflict"),
                        (None, None) => {
                            let before = normalize(&file.before);
                            let opened = open(tx, &json!({"path":file.source_path,"content":before,
                                "disk_content":before,"disk_revision":file.before_disk_revision,
                                "encoding":file.encoding,"line_ending":file.line_ending}))?;
                            load(tx, string(&opened,"document_id")?)?
                        }
                    };
                    check_document(tx, &document, file)?;
                    heads.push(refactor_head(&document));
                }
                let journal = json!({"operation_id":plan.operation_id,"plan_hash":plan.plan_hash,
                    "state":"prepared","plan":value,"receipt":null,"heads":heads,
                    "rollback_requested":false});
                put_record(tx, &journal, 0)?;
                for path in all_paths(&plan) {
                    tx.execute("INSERT INTO review_refactor_paths(path,operation_id) VALUES(?1,?2)",
                        params![path,plan.operation_id]).map_err(|_| "rename_recovery_pending")?;
                }
                let before = review_store::used_bytes(tx)?;
                tx.execute_batch("SAVEPOINT review_refactor_preflight")
                    .map_err(|_| "review_persistence_failed")?;
                let projected = project_commit(tx, &journal, before);
                tx.execute_batch("ROLLBACK TO review_refactor_preflight; RELEASE review_refactor_preflight")
                    .map_err(|_| "review_persistence_failed")?;
                let (_, commit_peak) = projected?;
                tx.execute_batch("SAVEPOINT review_refactor_rollback_preflight")
                    .map_err(|_| "review_persistence_failed")?;
                let rollback = project_rollback(tx,&journal);
                tx.execute_batch("ROLLBACK TO review_refactor_rollback_preflight; RELEASE review_refactor_rollback_preflight")
                    .map_err(|_| "review_persistence_failed")?;
                let (_,rollback_peak) = rollback?;
                let peak = commit_peak.max(rollback_peak);
                // Measure final document/events/receipt growth, then allow
                // bounded stage identities and phase/status growth without
                // charging those bytes twice when update consumes the reserve.
                let headroom = plan.files.len().checked_mul(4096)
                    .and_then(|bytes| bytes.checked_add(65536)).ok_or("review_budget")?;
                let reserved = peak.saturating_sub(before).checked_add(headroom)
                    .ok_or("review_budget")?;
                put_record(tx, &journal, reserved)?;
                Ok(journal)
            })
        })
    }

    pub(crate) fn refactor_record(&self, id: &str) -> Result<Option<Value>, &'static str> {
        rename_id(id)?;
        if !self.store.path.is_file() {
            return Ok(None);
        }
        self.store.run(|tx| record(tx, id))
    }

    /// Only phase and once-assigned stage identities may change. A retry with
    /// the original null identity never clears a previously durable identity.
    pub(crate) fn refactor_update(&self, id: &str, patch: &Value) -> Result<Value, &'static str> {
        rename_id(id)?;
        self.store.run(|tx| {
            let mut journal = record(tx, id)?.ok_or("rename_not_found")?;
            let original = journal.clone();
            if patch.get("operation_id").is_some_and(|value| value != id)
                || patch
                    .get("plan_hash")
                    .is_some_and(|value| value != &journal["plan_hash"])
                || patch
                    .get("heads")
                    .is_some_and(|value| value != &journal["heads"])
                || patch
                    .get("receipt")
                    .is_some_and(|value| value != &journal["receipt"])
            {
                return Err("review_call_conflict");
            }
            let supplied_plan = patch
                .get("plan")
                .or_else(|| patch.get("files").map(|_| patch));
            if let Some(plan) = supplied_plan {
                if immutable_plan(plan) != immutable_plan(&journal["plan"]) {
                    return Err("review_call_conflict");
                }
                let parsed = parse_plan(plan)?;
                let stored = journal["plan"]["files"]
                    .as_array_mut()
                    .ok_or("review_persistence_failed")?;
                for (file, proposed) in stored.iter_mut().zip(parsed.files) {
                    if let Some(identity) = proposed.stage_identity {
                        if file["stage_identity"]
                            .as_str()
                            .is_some_and(|old| old != identity.as_str())
                        {
                            return Err("review_call_conflict");
                        }
                        file["stage_identity"] = json!(identity);
                    }
                }
            }
            if terminal(&journal) {
                return Ok(original);
            }
            if let Some(state) = patch.get("state") {
                let state = state.as_str().ok_or("review_invalid_request")?;
                if !matches!(
                    state,
                    "prepared" | "applying" | "rolling_back" | "recovery_pending"
                ) {
                    return Err("review_invalid_request");
                }
                let old = journal["state"]
                    .as_str()
                    .ok_or("review_persistence_failed")?;
                if ((old == "rolling_back" || journal["rollback_requested"] == true)
                    && matches!(state, "prepared" | "applying"))
                    || (old == "applying" && state == "prepared")
                    || (old == "recovery_pending" && state == "prepared")
                {
                    return Err("review_call_conflict");
                }
                if state == "rolling_back" {
                    journal["rollback_requested"] = json!(true);
                }
                journal["state"] = json!(state);
            }
            let reserved = reservation(tx, id)?;
            let before = review_store::used_bytes(tx)?;
            update_reserved(tx, &journal, before, reserved)?;
            Ok(journal)
        })
    }

    /// The native owner has observed every affected endpoint before calling.
    /// Commit applies ALL review transformations atomically; rollback only
    /// releases reservations and leaves every document and its history intact.
    pub(crate) fn refactor_finish(&self, id: &str, state: &str) -> Result<Value, &'static str> {
        rename_id(id)?;
        if !matches!(state, "committed" | "rolled_back" | "recovery_pending") {
            return Err("review_invalid_request");
        }
        self.store.run(|tx| {
            with_text_context(tx, || {
                let mut journal = record(tx, id)?.ok_or("rename_not_found")?;
                if terminal(&journal) {
                    return Ok(journal["receipt"].clone());
                }
                if state == "recovery_pending" {
                    let reserved = reservation(tx, id)?;
                    let before = review_store::used_bytes(tx)?;
                    journal["state"] = json!(state);
                    update_reserved(tx, &journal, before, reserved)?;
                    return Ok(journal);
                }
                if state == "committed" {
                    if journal["state"] == "rolling_back" || journal["rollback_requested"] == true {
                        return Err("review_call_conflict");
                    }
                    let plan = parse_plan(&journal["plan"])?;
                    if plan.files.iter().any(|file| {
                        file.before_disk_revision != file.after_disk_revision
                            && file.stage_identity.is_none()
                    }) {
                        return Err("review_save_conflict");
                    }
                    tx.execute(
                        "UPDATE review_refactors SET reserved_bytes=0 WHERE operation_id=?1",
                        [id],
                    )
                    .map_err(|_| "review_persistence_failed")?;
                    return Ok(project_commit(tx, &journal, 0)?.0);
                }
                Ok(project_rollback(tx, &journal)?.0)
            })
        })
    }

    /// A durable cancellation tombstone prevents a delayed, previously unseen
    /// commit request from starting after the client has abandoned it.
    pub(crate) fn refactor_abandon(
        &self,
        id: &str,
        plan_hash: &str,
    ) -> Result<Value, &'static str> {
        rename_id(id)?;
        if !rename_hash(plan_hash) {
            return Err("review_invalid_request");
        }
        self.store.run(|tx| {
            if let Some(journal) = record(tx,id)? {
                if journal["plan_hash"] != plan_hash {
                    return Err("review_call_conflict");
                }
                return Ok(journal);
            }
            let count: i64 = tx.query_row("SELECT COUNT(*) FROM review_refactors",[],|row|row.get(0))
                .map_err(|_| "review_persistence_failed")?;
            if count >= MAX_REFACTOR_RECORDS {
                return Err("review_budget");
            }
            let journal = json!({"operation_id":id,"plan_hash":plan_hash,"state":"abandoned",
                "plan":null,"heads":[],"rollback_requested":false,
                "receipt":{"state":"abandoned","operation_id":id,"plan_hash":plan_hash,"files":[]}});
            put_record(tx,&journal,0)?;
            Ok(journal)
        })
    }
}

#[cfg(test)]
#[path = "review_refactor_tests.rs"]
mod tests;
