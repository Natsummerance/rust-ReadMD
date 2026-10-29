//! Durable content sessions and independent review decisions.
//!
//! File authorization and byte I/O belong to the server. This service only
//! accepts the actual disk identities supplied by that owner.
use crate::{
    document_transaction::{self, ContentTransaction, TextEdit},
    review_store::{self, Store},
};
use rusqlite::Transaction;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap, HashSet},
    path::{Component, Path, PathBuf},
    sync::{Arc, OnceLock},
};
use uuid::Uuid;

#[path = "review_refactor.rs"]
mod refactor;

pub const MAX_DOCUMENT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_OPERATIONS: usize = 4096;
const MAX_PENDING_RENAMES: usize = 64;
const MAX_RENAME_OPERATIONS: usize = 4096;

pub struct Service {
    store: Store,
}

// Immutable originals and candidates are shared by every anchor/version patch.
// The on-disk reference is independent of ordinary document-history retention.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ReviewText {
    hash: String,
    value: Arc<String>,
}
thread_local! {
    static TEXTS: RefCell<HashMap<String,Arc<String>>>=RefCell::new(HashMap::new());
    static PERSISTED_TEXTS: RefCell<HashSet<String>>=RefCell::new(HashSet::new());
}
impl ReviewText {
    fn new(value: String) -> Self {
        let hash = document_transaction::revision(&value);
        let value = TEXTS.with(|texts| {
            texts
                .borrow_mut()
                .entry(hash.clone())
                .or_insert_with(|| Arc::new(value))
                .clone()
        });
        Self { hash, value }
    }
    fn as_str(&self) -> &str {
        self.value.as_str()
    }
}
impl Serialize for ReviewText {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        json!({"$review_text":self.hash}).serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for ReviewText {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        if let Some(text) = value.as_str() {
            return Ok(Self::new(text.into()));
        }
        let hash = value
            .get("$review_text")
            .and_then(Value::as_str)
            .ok_or_else(|| <D::Error as serde::de::Error>::custom("invalid review text"))?;
        let text = TEXTS
            .with(|texts| texts.borrow().get(hash).cloned())
            .ok_or_else(|| <D::Error as serde::de::Error>::custom("missing review text"))?;
        Ok(Self {
            hash: hash.into(),
            value: text,
        })
    }
}
fn with_text_context<T>(
    _tx: &Transaction<'_>,
    f: impl FnOnce() -> Result<T, &'static str>,
) -> Result<T, &'static str> {
    let texts = HashMap::new();
    struct Restore(HashMap<String, Arc<String>>, HashSet<String>);
    impl Drop for Restore {
        fn drop(&mut self) {
            TEXTS.with(|slot| {
                slot.replace(std::mem::take(&mut self.0));
            });
            PERSISTED_TEXTS.with(|slot| {
                slot.replace(std::mem::take(&mut self.1));
            });
        }
    }
    let persisted = texts.keys().cloned().collect();
    let _restore = Restore(
        TEXTS.with(|slot| slot.replace(texts)),
        PERSISTED_TEXTS.with(|slot| slot.replace(persisted)),
    );
    f()
}
fn ensure_text_context(tx: &Transaction<'_>, payload: &str) -> Result<(), &'static str> {
    // All references are emitted by our serializer. Escaped source strings do
    // not match this canonical object-key shape. Each archived event is scanned
    // too, so a removed proposal can still reconstruct its original receipt.
    static REFERENCES: OnceLock<regex::Regex> = OnceLock::new();
    let references = REFERENCES.get_or_init(|| {
        regex::Regex::new(r#""\$review_text"\s*:\s*"([0-9a-f]{64})""#)
            .expect("fixed review reference pattern")
    });
    for capture in references.captures_iter(payload) {
        let hash = &capture[1];
        if TEXTS.with(|texts| texts.borrow().contains_key(hash)) {
            continue;
        }
        let text = review_store::text(tx, hash)?.ok_or("review_persistence_failed")?;
        if document_transaction::revision(&text) != hash {
            return Err("review_persistence_failed");
        }
        TEXTS.with(|texts| {
            texts.borrow_mut().insert(hash.into(), Arc::new(text));
        });
        PERSISTED_TEXTS.with(|known| {
            known.borrow_mut().insert(hash.into());
        });
    }
    Ok(())
}
fn flush_changes(tx: &Transaction<'_>, changes: &[Change]) -> Result<(), &'static str> {
    for change in changes {
        for text in std::iter::once(&change.original)
            .chain(std::iter::once(&change.replacement))
            .chain(change.candidate_versions.iter())
        {
            if !PERSISTED_TEXTS.with(|known| known.borrow().contains(&text.hash)) {
                review_store::put_text(tx, &text.hash, text.as_str())?;
                PERSISTED_TEXTS.with(|known| {
                    known.borrow_mut().insert(text.hash.clone());
                });
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Change {
    id: String,
    from: usize,
    to: usize,
    original: ReviewText,
    replacement: ReviewText,
    decision: String,
    application: String,
    version: u64,
    origin: String,
    task_id: Option<String>,
    taken_over: bool,
    #[serde(default)]
    candidate_versions: Vec<ReviewText>,
    #[serde(default)]
    dependencies: Vec<String>,
    #[serde(default)]
    last_event_id: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Patch {
    id: String,
    before: Option<Change>,
    after: Option<Change>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Action {
    event_id: u64,
    edits: Vec<TextEdit>,
    inverse: Vec<TextEdit>,
    inverse_expected: Vec<String>,
    patches: Vec<Patch>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Mapping {
    event_id: u64,
    edits: Vec<TextEdit>,
    inverse: Vec<TextEdit>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Checkpoint {
    content: String,
    changes: Vec<Change>,
    event_id: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SaveIntent {
    intent_id: String,
    generation: u64,
    review_revision: u64,
    source_path: String,
    target_path: String,
    disk_revision: String,
    target_disk_revision: String,
    expected_disk_revision: String,
    content: String,
    saved_content: String,
    edit_steps: Vec<Vec<TextEdit>>,
    encoding: String,
    line_ending: String,
    #[serde(default)]
    reserved_bytes: usize,
}

// A rename is metadata-only. It deliberately does not use finish_intent, which
// records a new saved checkpoint and would change discard/undo semantics.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct RenameIntent {
    operation_id: String,
    target_path: String,
    reserved_bytes: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct RenameOperation {
    operation_id: String,
    document_id: String,
    fingerprint: String,
    source_path: String,
    target_path: String,
    source_identity: String,
    disk_revision: String,
    generation: u64,
    review_revision: u64,
    state: String,
    error_code: Option<String>,
}

/// Only the trusted file adapter constructs this evidence. Equal bytes in a
/// different file are never evidence that our source was moved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RenameObservation {
    Missing,
    Regular { identity: String, revision: String },
    Unavailable,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Document {
    document_id: String,
    path: String,
    content: String,
    generation: u64,
    revision: String,
    review_revision: u64,
    disk_revision: String,
    encoding: String,
    line_ending: String,
    changes: Vec<Change>,
    last_event_id: u64,
    undo: Vec<Action>,
    redo: Vec<Action>,
    mappings: Vec<Mapping>,
    save_intent: Option<SaveIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rename_intent: Option<RenameIntent>,
    recovery: Option<Value>,
    checkpoint: Checkpoint,
}

impl Service {
    /// Does not create a directory or database until the first request.
    pub fn new(data_dir: &Path) -> Self {
        Self {
            store: Store::new(data_dir),
        }
    }

    pub fn handle(&self, request: &Value) -> Result<Value, &'static str> {
        let action = string(request, "action")?;
        if action == "pins" {
            return Ok(json!({"paths":self.pinned_assets()?}));
        }
        self.store.run(|tx| {
            with_text_context(tx, || {
                if action == "open" {
                    return open(tx, request);
                }
                if action == "lookup" {
                    let path = string(request, "path")?;
                    return match review_store::by_path(tx, path)? {
                        Some(payload) => {
                            Ok(json!({"found":true,"snapshot":snapshot(&decode(tx,&payload)?)}))
                        }
                        None => Ok(json!({"found":false})),
                    };
                }
                let id = string(request, "document_id")?;
                let mut document = load(tx, id)?;
                if action == "read" {
                    return Ok(snapshot(&document));
                }
                if action == "receipt" {
                    return match review_store::lookup_receipt(tx, id, string(request, "call_id")?)?
                    {
                        Some(receipt) => {
                            let mut receipt = expand_receipt(tx, &document, receipt)?;
                            receipt["found"] = json!(true);
                            Ok(receipt)
                        }
                        None => Ok(json!({"found":false})),
                    };
                }
                if document.rename_intent.is_some() || review_store::pending_rename_at(tx, &document.path)?.is_some() { return Err("rename_recovery_pending"); }
                if !matches!(
                    action,
                    "edit"
                        | "propose"
                        | "resolve"
                        | "undo"
                        | "redo"
                        | "prepare_save"
                        | "commit_save"
                        | "abort_save"
                        | "discard"
                ) {
                    return Err("review_invalid_request");
                }
                if action != "resolve" && request.get("resolution").is_some() {
                    return Err("review_invalid_request");
                }
                let call_id = string(request, "call_id")?;
                if call_id.is_empty() || call_id.len() > 256 {
                    return Err("review_invalid_request");
                }
                let fingerprint = document_transaction::revision(&canonical(request).to_string());
                if let Some(receipt) = review_store::receipt(tx, id, call_id, &fingerprint)? {
                    return Ok(expand_receipt(tx, &document, receipt)?["snapshot"].clone());
                }
                if action == "commit_save" || action == "abort_save" {
                    if action == "commit_save" {
                        commit_save(tx, &mut document, request)?;
                    } else {
                        abort_save(tx, &mut document, request)?;
                    }
                } else {
                    cas(&document, request)?;
                    if document.recovery.is_some() {
                        return Err("review_needs_recovery");
                    }
                    if document.save_intent.is_some() {
                        return Err("review_save_in_progress");
                    }
                    match action {
                        "edit" | "propose" => {
                            edit(tx, &mut document, request, action == "propose")?
                        }
                        "resolve" => resolve(tx, &mut document, request)?,
                        "undo" => compensate(tx, &mut document, false)?,
                        "redo" => compensate(tx, &mut document, true)?,
                        "prepare_save" => prepare_save(tx, &mut document, request)?,
                        "discard" => discard(tx, &mut document, request)?,
                        _ => return Err("review_invalid_request"),
                    }
                }
                persist(tx, &document)?;
                let mut receipt = snapshot(&document);
                receipt["call_id"] = json!(call_id);
                receipt["durable"] = json!(true);
                if let Some(intent) = &document.save_intent {
                    receipt["intent_id"] = json!(intent.intent_id);
                }
                let compact = compact_receipt(request, &receipt);
                review_store::put_receipt(tx, id, call_id, &fingerprint, &compact)?;
                Ok(receipt)
            })
        })
    }

    /// Internal export boundary. The caller supplies identities, never text or
    /// change IDs. No public review action can manufacture this authority.
    pub(crate) fn freeze_export_snapshot(&self, request: &Value) -> Result<Value, &'static str> {
        self.store.run(|tx| with_text_context(tx, || {
            let document = load(tx, string(request, "document_id")?)?;
            cas(&document, request)?;
            if document.recovery.is_some() { return Err("review_needs_recovery"); }
            if document.save_intent.is_some() { return Err("review_save_in_progress"); }
            if document.rename_intent.is_some() || review_store::pending_rename_at(tx, &document.path)?.is_some() { return Err("rename_recovery_pending"); }
            if document.changes.iter().any(|change| change.decision == "pending"
                && (change.application != "applied" || change.taken_over)) {
                return Err("review_conflict");
            }
            Ok(export_review(&document, &document.content, &document.changes))
        }))
    }

    /// Only the trusted task owner may dispatch this boundary after binding an
    /// AI tool call to its durable task. JSON review actions cannot enable it.
    pub(crate) fn propose_task(&self, request: &Value) -> Result<Value, &'static str> {
        if string(request,"action")? != "propose" || string(request,"origin")? != "ai"
            || request.get("application").is_some_and(|value|value!="applied") || request.get("resolution").is_some() {
            return Err("review_invalid_request");
        }
        let task = string(request,"task_id")?;
        let call = string(request,"call_id")?;
        if task.is_empty() || task.len()>256 || call.is_empty() || call.len()>256 { return Err("review_invalid_request"); }
        let fingerprint = document_transaction::revision(&canonical(&json!({"internal":"propose_task","request":request})).to_string());
        self.store.run(|tx| with_text_context(tx,|| {
            let id = string(request,"document_id")?;
            let mut document = load(tx,id)?;
            if document.rename_intent.is_some() || review_store::pending_rename_at(tx, &document.path)?.is_some() { return Err("rename_recovery_pending"); }
            if let Some(receipt)=review_store::receipt(tx,id,call,&fingerprint)? {
                return Ok(expand_receipt(tx,&document,receipt)?["snapshot"].clone());
            }
            cas(&document,request)?;
            if document.recovery.is_some() { return Err("review_needs_recovery"); }
            if document.save_intent.is_some() { return Err("review_save_in_progress"); }
            let requested = edits(request)?;
            if requested.is_empty() { return Err("review_invalid_request"); }
            let before_content = document.content.clone();
            let before_changes = document.changes.clone();
            let result = apply(&before_content,&requested)?;
            let mut updated = BTreeSet::new();
            let mut new_ranges = BTreeSet::new();
            for edit in &requested {
                let mut owners = Vec::new();
                for change in &document.changes {
                    let (mut from,mut to)=(change.from,change.to);
                    if !map_geometry(&mut from,&mut to,std::slice::from_ref(edit)) { continue; }
                    if change.origin!="ai" || change.task_id.as_deref()!=Some(task) || change.decision!="pending"
                        || change.application!="applied" || change.taken_over || edit.from<change.from || edit.to>change.to {
                        return Err("review_conflict");
                    }
                    owners.push(change.id.clone());
                }
                if owners.len()>1 { return Err("review_conflict"); }
                if let Some(owner)=owners.pop() { updated.insert(owner); }
                else { new_ranges.insert((edit.from,edit.to)); }
            }
            let supplied:Vec<String> = request.get("dependencies").map(|value|serde_json::from_value(value.clone()))
                .transpose().map_err(|_|"review_invalid_request")?.unwrap_or_default();
            let dependencies=supplied.iter().cloned().collect::<BTreeSet<_>>();
            if dependencies.len()!=supplied.len() || dependencies!=updated { return Err("review_conflict"); }
            if document.changes.len().saturating_add(new_ranges.len())>MAX_OPERATIONS { return Err("review_budget"); }
            // Only the ownership checks above authorize system-style candidate
            // rebasing. Public origin flags never reach this trusted branch.
            map_changes(&mut document,&requested,&result.content,true)?;
            document.content=result.content;
            let mut shift=0isize;
            for edit in sorted(&requested) {
                if new_ranges.contains(&(edit.from,edit.to)) {
                    let original=slice(&before_content,edit.from,edit.to)?.to_string();
                    let from=shifted(edit.from,shift);
                    document.changes.push(Change { id:Uuid::new_v4().to_string(),from,to:from+edit.insert.encode_utf16().count(),
                        original:ReviewText::new(original),replacement:ReviewText::new(edit.insert.clone()),decision:"pending".into(),application:"applied".into(),
                        version:next_counter(document.review_revision)?,origin:"ai".into(),task_id:Some(task.into()),taken_over:false,
                        candidate_versions:vec![ReviewText::new(edit.insert.clone())],dependencies:dependencies.iter().cloned().collect(),last_event_id:0 });
                }
                shift+=edit.insert.encode_utf16().count() as isize-(edit.to-edit.from) as isize;
            }
            let action=record_action(tx,&mut document,"propose","ai",requested,&before_content,before_changes,None)?;
            document.undo.push(action);document.redo.clear();persist(tx,&document)?;
            let mut receipt=snapshot(&document);receipt["call_id"]=json!(call);receipt["durable"]=json!(true);
            review_store::put_receipt(tx,id,call,&fingerprint,&compact_receipt(request,&receipt))?;
            Ok(receipt)
        }))
    }

    /// Accept only the exact candidates whose bytes reached a verified export.
    /// Independent later proposals are outside this transaction's scope. A
    /// durable receipt makes a crash between review commit and export receipt
    /// safe to replay, including when every frozen candidate now conflicts.
    pub(crate) fn accept_export_snapshot(&self, frozen: &Value, call_id: &str) -> Result<Value, &'static str> {
        if call_id.is_empty() || call_id.len() > 256 { return Err("review_invalid_request"); }
        let id = string(frozen, "document_id")?;
        let fingerprint = document_transaction::revision(&canonical(&json!({
            "action":"accept_export_snapshot", "frozen":frozen, "call_id":call_id
        })).to_string());
        self.store.run(|tx| with_text_context(tx, || {
            let mut document = load(tx, id)?;
            if document.rename_intent.is_some() || review_store::pending_rename_at(tx, &document.path)?.is_some() { return Err("rename_recovery_pending"); }
            if let Some(receipt) = review_store::receipt(tx, id, call_id, &fingerprint)? {
                return Ok(expand_receipt(tx, &document, receipt)?["snapshot"].clone());
            }
            let target = number(frozen, "last_event_id")?;
            if target > document.last_event_id { return Err("review_conflict"); }
            // Reconstruct the authoritative historical state before trusting
            // even an internal frozen candidate. Content must be the original
            // snapshot, not a caller-selected subset or a replacement string.
            let mut content = document.content.clone();
            for mapping in document.mappings.iter().rev().filter(|mapping| mapping.event_id > target) {
                content = apply(&content, &mapping.inverse)?.content;
            }
            let mut changes = document.changes.clone();
            for payload in review_store::events_after(tx, id, target)? {
                ensure_text_context(tx, &payload)?;
                let event: Value = serde_json::from_str(&payload).map_err(|_| "review_persistence_failed")?;
                if let Some(patches) = event.get("patches") {
                    let patches: Vec<Patch> = serde_json::from_value(patches.clone()).map_err(|_| "review_persistence_failed")?;
                    for patch in patches {
                        changes.retain(|change| change.id != patch.id);
                        if let Some(before) = patch.before { changes.push(before); }
                    }
                }
            }
            let historical = export_review(&document, &content, &changes);
            for field in ["content", "revision", "pending_changes", "dependency_changes"] {
                if historical[field] != frozen[field] { return Err("review_conflict"); }
            }
            if document.recovery.is_some() { return Err("review_needs_recovery"); }
            if document.save_intent.is_some() { return Err("review_save_in_progress"); }
            let pending = frozen["pending_changes"].as_array().ok_or("review_invalid_request")?;
            let dependencies = frozen["dependency_changes"].as_array().ok_or("review_invalid_request")?;
            let current = public_changes(&document.changes, &document.content);
            let unchanged = |expected: &Value| current.iter().any(|change| change == expected);
            let mut accepted = Vec::new();
            let mut conflicts = Vec::new();
            for expected in pending {
                let candidate = string(expected, "id")?;
                let mut needed = expected["dependencies"].as_array().ok_or("review_invalid_request")?.clone();
                let mut seen = BTreeSet::new();
                let mut dependencies_match = true;
                while let Some(dependency) = needed.pop() {
                    let dependency = dependency.as_str().ok_or("review_invalid_request")?;
                    if !seen.insert(dependency.to_string()) { continue; }
                    let Some(expected) = dependencies.iter().find(|change| change["id"] == dependency) else {
                        dependencies_match = false; break;
                    };
                    if !unchanged(expected) { dependencies_match = false; break; }
                    needed.extend(expected["dependencies"].as_array().ok_or("review_invalid_request")?.iter().cloned());
                }
                if expected["decision"] == "pending" && expected["application"] == "applied"
                    && expected["taken_over"] == false && unchanged(expected) && dependencies_match {
                    accepted.push(candidate.to_string());
                } else { conflicts.push(candidate.to_string()); }
            }
            if !accepted.is_empty() {
                let before_content = document.content.clone();
                let before_changes = document.changes.clone();
                let version = next_counter(document.review_revision)?;
                for change in &mut document.changes {
                    if accepted.contains(&change.id) { change.decision = "accepted".into(); change.version = version; }
                }
                let action = record_action(tx, &mut document, "accept_export_snapshot", "editor", Vec::new(),
                    &before_content, before_changes, None)?;
                document.undo.push(action);
                document.redo.clear();
                persist(tx, &document)?;
            }
            let mut receipt = snapshot(&document);
            receipt["call_id"] = json!(call_id);
            receipt["durable"] = json!(true);
            receipt["accepted_ids"] = json!(accepted);
            receipt["accepted_count"] = json!(accepted.len());
            receipt["conflict_ids"] = json!(conflicts);
            receipt["conflict_count"] = json!(conflicts.len());
            let compact = compact_receipt(&json!({"action":"accept_export_snapshot"}), &receipt);
            review_store::put_receipt(tx, id, call_id, &fingerprint, &compact)?;
            Ok(receipt)
        }))
    }

    /// Failure is observable so a janitor cannot turn a broken database into an
    /// empty retention set. Text in all review versions remains pinned.
    pub fn pinned_assets(&self) -> Result<Vec<String>, &'static str> {
        self.store.run(|tx| {
            let mut paths = BTreeSet::new();
            let texts = review_store::texts(tx)?
                .into_iter()
                .collect::<HashMap<_, _>>();
            for payload in review_store::all_payloads(tx)? {
                let value: Value =
                    serde_json::from_str(&payload).map_err(|_| "review_persistence_failed")?;
                let id = value["document_id"]
                    .as_str()
                    .ok_or("review_persistence_failed")?;
                let mut values = vec![value.clone()];
                for event in review_store::events_after(tx, id, 0)? {
                    values.push(
                        serde_json::from_str(&event).map_err(|_| "review_persistence_failed")?,
                    );
                }
                let mut bases = BTreeSet::new();
                for value in &values {
                    collect_asset_bases(value, &mut bases);
                }
                let mut visited = BTreeSet::new();
                for value in &values {
                    collect_pins(value, &bases, &texts, &mut visited, &mut paths)?;
                }
            }
            Ok(paths.into_iter().collect())
        })
    }

    #[cfg(test)]
    pub(crate) fn with_limit(data_dir: &Path, byte_limit: usize) -> Self {
        let mut service = Self::new(data_dir);
        service.store.byte_limit = byte_limit;
        service
    }
}

impl Service {
    /// Trusted adapter only. No public JSON action can manufacture file proof.
    /// A retry checks its original request before any filesystem access.
    pub(crate) fn rename_replay(&self, request: &Value) -> Result<Option<Value>, &'static str> {
        let fingerprint = rename_fingerprint(request)?;
        self.store.run(|tx| with_text_context(tx, || {
            let id = string(request, "operation_id")?;
            if review_store::rename_abandoned(tx, id)? {
                return Ok(Some(json!({"ok":false,"state":"abandoned","operation_id":id})));
            }
            let Some(operation) = load_rename(tx, id)? else { return Ok(None); };
            if operation.fingerprint != fingerprint { return Err("review_call_conflict"); }
            Ok(Some(rename_view(&operation, &load(tx, &operation.document_id)?)))
        }))
    }

    pub(crate) fn rename_status(&self, operation_id: &str) -> Result<Option<Value>, &'static str> {
        rename_id(operation_id)?;
        self.store.run(|tx| with_text_context(tx, || {
            if review_store::rename_abandoned(tx, operation_id)? {
                return Ok(Some(json!({"ok":false,"state":"abandoned","operation_id":operation_id})));
            }
            let Some(operation) = load_rename(tx, operation_id)? else { return Ok(None); };
            Ok(Some(rename_view(&operation, &load(tx, &operation.document_id)?)))
        }))
    }

    /// Called under the same profile gate as prepare and move. A durable
    /// tombstone prevents any delayed request from ever starting this ID.
    pub(crate) fn abandon_unstarted_rename(&self, operation_id: &str) -> Result<Value, &'static str> {
        rename_id(operation_id)?;
        self.store.run(|tx| with_text_context(tx, || {
            if review_store::rename_abandoned(tx, operation_id)? {
                return Ok(json!({"ok":false,"state":"abandoned","operation_id":operation_id}));
            }
            if let Some(operation) = load_rename(tx, operation_id)? {
                return Ok(rename_view(&operation, &load(tx, &operation.document_id)?));
            }
            let (all, _) = review_store::rename_counts(tx)?;
            if all >= MAX_RENAME_OPERATIONS { return Err("review_budget"); }
            review_store::abandon_rename(tx, operation_id)?;
            Ok(json!({"ok":false,"state":"abandoned","operation_id":operation_id}))
        }))
    }

    pub(crate) fn pending_rename(&self, path: &str) -> Result<Option<Value>, &'static str> {
        self.store.run(|tx| with_text_context(tx, || {
            let Some(payload) = review_store::pending_rename_at(tx, path)? else { return Ok(None); };
            let group:Value = serde_json::from_str(&payload).map_err(|_| "review_persistence_failed")?;
            if group.get("plan").is_some() { return Ok(Some(json!({"operation_id":group["operation_id"],"state":group["state"],"refactor":true}))); }
            let operation: RenameOperation = serde_json::from_value(group).map_err(|_| "review_persistence_failed")?;
            Ok(Some(rename_view(&operation, &load(tx, &operation.document_id)?)))
        }))
    }

    /// Source and target have already been observed under the file owner's
    /// SAVE_LOCK -> profile gate. This transaction creates a clean session only
    /// when none exists, and reserves both paths before permitting a move.
    pub(crate) fn prepare_rename(&self, request: &Value) -> Result<Value, &'static str> {
        let fingerprint = rename_fingerprint(request)?;
        let operation_id = string(request, "operation_id")?;
        let source = string(request, "source_path")?;
        let target = string(request, "target_path")?;
        let identity = string(request, "source_identity")?;
        if identity.is_empty() || identity.len() > 256 || !identity.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'-' | b'_')) {
            return Err("rename_unsupported_file");
        }
        let revision = string(request, "disk_revision")?;
        if !rename_hash(revision) { return Err("review_invalid_request"); }
        if revision != string(request, "expected_disk_revision")? { return Err("rename_revision_conflict"); }
        let disk_content = normalize(string(request, "disk_content")?);
        checked_content(&disk_content)?;
        self.store.run(|tx| with_text_context(tx, || {
            if review_store::rename_abandoned(tx, operation_id)? {
                return Ok(json!({"ok":false,"state":"abandoned","operation_id":operation_id}));
            }
            if let Some(operation) = load_rename(tx, operation_id)? {
                if operation.fingerprint != fingerprint { return Err("review_call_conflict"); }
                return Ok(rename_view(&operation, &load(tx, &operation.document_id)?));
            }
            for path in [source, target] {
                if review_store::pending_rename_at(tx, path)?.is_some() { return Err("rename_recovery_pending"); }
            }
            let (all, pending) = review_store::rename_counts(tx)?;
            if all >= MAX_RENAME_OPERATIONS || pending >= MAX_PENDING_RENAMES { return Err("review_budget"); }
            let mut document = match review_store::by_path(tx, source)? {
                Some(payload) => decode(tx, &payload)?,
                None => {
                    // Establishing the session and its rename intent is one
                    // transaction. A failed admission leaves no new session.
                    let opened = open(tx, &json!({"path":source,"content":disk_content,"disk_content":disk_content,
                        "disk_revision":revision,"encoding":request["encoding"],"line_ending":request["line_ending"]}))?;
                    load(tx, string(&opened, "document_id")?)?
                }
            };
            if document.path != source { return Err("review_save_conflict"); }
            if let Some(session) = request.get("review_session").filter(|value| !value.is_null()) {
                if session["document_id"] != document.document_id { return Err("review_save_conflict"); }
                cas(&document, session)?;
            }
            if document.rename_intent.is_some() || review_store::pending_rename_at(tx, &document.path)?.is_some() { return Err("rename_recovery_pending"); }
            if document.save_intent.is_some() { return Err("review_save_in_progress"); }
            if document.recovery.is_some() { return Err("review_needs_recovery"); }
            if document.disk_revision != revision { return Err("rename_revision_conflict"); }
            if document.content != disk_content { return Err("rename_unsaved_review"); }
            if !review_store::path_available(tx, target, &document.document_id)? { return Err("review_save_conflict"); }
            // Preparation, at most one pending marker and a terminal event
            // must all fit before allowing the owner to move any file.
            next_counter(next_counter(next_counter(document.last_event_id)?)?)?;
            let operation = RenameOperation { operation_id:operation_id.into(),document_id:document.document_id.clone(),fingerprint,
                source_path:source.into(),target_path:target.into(),source_identity:identity.into(),disk_revision:revision.into(),
                generation:document.generation,review_revision:document.review_revision,state:"prepared".into(),error_code:None };
            // Bounded completion headroom, not a claim that a later fsync or
            // SQLite commit cannot fail. A failure keeps this durable intent.
            let target_bytes=serde_json::to_string(target).map_err(|_|"review_persistence_failed")?.len();
            document.rename_intent = Some(RenameIntent { operation_id:operation_id.into(),target_path:target.into(),reserved_bytes:32 * 1024 + target_bytes });
            let event = next_event(&mut document)?;
            review_store::event(tx, &document.document_id, event, "prepare_rename", &json!({"operation_id":operation_id}))?;
            persist(tx, &document)?;
            put_rename(tx, &operation)?;
            Ok(rename_view(&operation, &document))
        }))
    }

    /// Never moves or rolls back a file. The adapter supplies two observations
    /// made under the same profile gate, including after a process restart.
    pub(crate) fn recover_rename(&self, operation_id: &str, source: &RenameObservation, target: &RenameObservation) -> Result<Value, &'static str> {
        rename_id(operation_id)?;
        self.store.run(|tx| with_text_context(tx, || {
            let mut operation = load_rename(tx, operation_id)?.ok_or("rename_not_found")?;
            let mut document = load(tx, &operation.document_id)?;
            if matches!(operation.state.as_str(), "committed" | "cancelled") { return Ok(rename_view(&operation, &document)); }
            let intent = document.rename_intent.as_ref().ok_or("review_persistence_failed")?;
            if intent.operation_id != operation.operation_id || intent.target_path != operation.target_path
                || document.path != operation.source_path || document.generation != operation.generation
                || document.review_revision != operation.review_revision || document.disk_revision != operation.disk_revision {
                return Err("review_persistence_failed");
            }
            let original = |value: &RenameObservation| matches!(value, RenameObservation::Regular {identity,revision}
                if identity == &operation.source_identity && revision == &operation.disk_revision);
            let state = if matches!(source, RenameObservation::Missing) && original(target) {
                "committed"
            } else if original(source) && (matches!(target, RenameObservation::Missing)
                || matches!(target, RenameObservation::Regular {identity,..} if identity != &operation.source_identity)) {
                "cancelled"
            } else { "recovery_pending" };
            if operation.state != state {
                let previous_charge=review_store::used_bytes(tx)?;
                operation.state = state.into();
                operation.error_code = match state { "cancelled"=>Some("rename_cancelled".into()), "recovery_pending"=>Some("rename_recovery_pending".into()), _=>None };
                if state == "committed" { document.path = operation.target_path.clone(); }
                if state != "recovery_pending" { document.rename_intent = None; }
                // Only path and rename bookkeeping change. In particular do
                // not touch checkpoint, source text, changes or undo/redo.
                let event = next_event(&mut document)?;
                review_store::event(tx, &document.document_id, event,
                    if state=="committed" {"commit_rename"} else if state=="cancelled" {"cancel_rename"} else {"rename_needs_recovery"},
                    &json!({"operation_id":operation_id}))?;
                persist(tx, &document)?;
                put_rename(tx, &operation)?;
                if state == "recovery_pending" {
                    // Consume reserved logical headroom for this new event;
                    // retaining the reservation in full would double-charge
                    // it and could reject a pending marker at the exact cap.
                    let growth=review_store::used_bytes(tx)?.saturating_sub(previous_charge);
                    let intent=document.rename_intent.as_mut().ok_or("review_persistence_failed")?;
                    intent.reserved_bytes=intent.reserved_bytes.checked_sub(growth).ok_or("review_budget")?;
                    persist(tx, &document)?;
                }
            }
            Ok(rename_view(&operation, &document))
        }))
    }
}

fn rename_id(value: &str) -> Result<(), &'static str> {
    if value.is_empty() || value.len()>128 || !value.bytes().all(|b|b.is_ascii_alphanumeric() || matches!(b,b'-'|b'_'|b':')) {
        return Err("review_invalid_request");
    }
    Ok(())
}
fn rename_hash(value: &str) -> bool { value.len()==64 && value.bytes().all(|b|b.is_ascii_hexdigit()) }
fn rename_fingerprint(request: &Value) -> Result<String, &'static str> {
    rename_id(string(request,"operation_id")?)?;
    let source=string(request,"source_path")?; let target=string(request,"target_path")?;
    for value in [source,target] { path(value)?; if value.starts_with("draft:") || !Path::new(value).is_absolute() { return Err("review_invalid_request"); } }
    if source==target || Path::new(source).parent()!=Path::new(target).parent() { return Err("review_invalid_request"); }
    if !rename_hash(string(request,"expected_disk_revision")?) { return Err("review_invalid_request"); }
    if let Some(expected) = request.get("expected_revision").filter(|v|!v.is_null()) {
        if !expected.as_str().is_some_and(rename_hash) { return Err("review_invalid_request"); }
    }
    Ok(document_transaction::revision(&canonical(&json!({"source":source,"target":target,
        "expected_disk_revision":request["expected_disk_revision"],"expected_revision":request.get("expected_revision").filter(|v|!v.is_null()),
        "review_session":request.get("review_session").filter(|v|!v.is_null())})).to_string()))
}
fn load_rename(tx: &Transaction<'_>, operation_id: &str) -> Result<Option<RenameOperation>, &'static str> {
    review_store::rename_operation(tx, operation_id)?.map(|payload| {
        let operation: RenameOperation = serde_json::from_str(&payload).map_err(|_|"review_persistence_failed")?;
        if operation.operation_id != operation_id || !matches!(operation.state.as_str(), "prepared"|"committed"|"cancelled"|"recovery_pending") {
            return Err("review_persistence_failed");
        }
        Ok(operation)
    }).transpose()
}
fn put_rename(tx: &Transaction<'_>, operation: &RenameOperation) -> Result<(), &'static str> {
    let payload=serde_json::to_string(operation).map_err(|_|"review_persistence_failed")?;
    review_store::put_rename(tx,&operation.operation_id,&operation.document_id,&operation.source_path,&operation.target_path,&operation.state,&payload)
}
fn rename_view(operation: &RenameOperation, document: &Document) -> Value {
    let current_path = if matches!(operation.state.as_str(), "committed"|"cancelled") { &document.path } else { &operation.target_path };
    let mut value=json!({"ok":operation.state=="committed","state":operation.state,"operation_id":operation.operation_id,
        "old_path":operation.source_path,"target_path":operation.target_path,"path":current_path,"review":snapshot(document),
        "moved":if operation.state=="committed"{Some(true)}else if operation.state=="cancelled"{Some(false)}else{None}});
    if let Some(code)=&operation.error_code { value["error_code"]=json!(code); }
    value
}

fn export_review(document: &Document, content: &str, changes: &[Change]) -> Value {
    let public = public_changes(changes, content);
    let mut pending = public.iter().filter(|change| change["decision"] == "pending").cloned().collect::<Vec<_>>();
    pending.sort_by(|a,b| a["id"].as_str().cmp(&b["id"].as_str()));
    let mut needed = pending.iter().flat_map(|change| change["dependencies"].as_array().into_iter().flatten())
        .filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>();
    let mut dependencies = BTreeSet::new();
    while let Some(id) = needed.pop() {
        if !dependencies.insert(id.clone()) { continue; }
        if let Some(change) = public.iter().find(|change| change["id"] == id) {
            needed.extend(change["dependencies"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_string));
        }
    }
    let dependency_changes = dependencies.iter().filter_map(|id| public.iter().find(|change| change["id"] == *id))
        .cloned().collect::<Vec<_>>();
    json!({"document_id":document.document_id,"path":document.path,"content":content,
        "generation":document.generation,"review_revision":document.review_revision,
        "revision":document_transaction::revision(content),"last_event_id":document.last_event_id,
        "pending_changes":pending,"dependency_changes":dependency_changes})
}

fn string<'a>(value: &'a Value, field: &str) -> Result<&'a str, &'static str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or("review_invalid_request")
}
fn number(value: &Value, field: &str) -> Result<u64, &'static str> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or("review_invalid_request")
}
fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut keys = object.keys().collect::<Vec<_>>();
            keys.sort();
            let mut result = serde_json::Map::new();
            for key in keys {
                result.insert(key.clone(), canonical(&object[key]));
            }
            Value::Object(result)
        }
        Value::Array(array) => Value::Array(array.iter().map(canonical).collect()),
        other => other.clone(),
    }
}
pub fn normalize(content: &str) -> String {
    content
        .strip_prefix('\u{feff}')
        .unwrap_or(content)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}
fn checked_content(content: &str) -> Result<(), &'static str> {
    if content.len() > MAX_DOCUMENT_BYTES {
        return Err("review_budget");
    }
    if content.contains('\r') || content.starts_with('\u{feff}') {
        return Err("review_invalid_request");
    }
    Ok(())
}
fn path(value: &str) -> Result<(), &'static str> {
    if value.is_empty()
        || value.len() > 32768
        || value.chars().any(|ch| matches!(ch, '\0' | '\r' | '\n'))
    {
        Err("review_invalid_request")
    } else {
        Ok(())
    }
}
fn decode(tx: &Transaction<'_>, payload: &str) -> Result<Document, &'static str> {
    ensure_text_context(tx, payload)?;
    serde_json::from_str(payload).map_err(|_| "review_persistence_failed")
}
fn load(tx: &Transaction<'_>, id: &str) -> Result<Document, &'static str> {
    decode(tx, &review_store::by_id(tx, id)?.ok_or("review_not_found")?)
}
fn persist(tx: &Transaction<'_>, document: &Document) -> Result<(), &'static str> {
    if document.save_intent.is_some() && document.rename_intent.is_some() { return Err("review_persistence_failed"); }
    flush_changes(tx, &document.changes)?;
    flush_changes(tx, &document.checkpoint.changes)?;
    for action in document.undo.iter().chain(&document.redo) {
        for patch in &action.patches {
            if let Some(before) = &patch.before {
                flush_changes(tx, std::slice::from_ref(before))?;
            }
            if let Some(after) = &patch.after {
                flush_changes(tx, std::slice::from_ref(after))?;
            }
        }
    }
    let payload = serde_json::to_string(document).map_err(|_| "review_persistence_failed")?;
    review_store::put_document(
        tx,
        &document.document_id,
        &document.path,
        document
            .rename_intent
            .as_ref()
            .map(|intent| intent.target_path.as_str())
            .or_else(|| document.save_intent.as_ref().map(|intent| intent.target_path.as_str())),
        &payload,
    )
}
fn snapshot(document: &Document) -> Value {
    let changes = public_changes(&document.changes, &document.content);
    let mut value = json!({"document_id":document.document_id,"path":document.path,"content":document.content,
        "generation":document.generation,"revision":document.revision,"review_revision":document.review_revision,
        "disk_revision":document.disk_revision,"encoding":document.encoding,"line_ending":document.line_ending,
        "saved_revision":document_transaction::revision(&document.checkpoint.content),
        "changes":changes,"last_event_id":document.last_event_id,"save_intent":document.save_intent,
        "recovery":document.recovery,"status":if document.rename_intent.is_some(){"rename_pending"}else if document.recovery.is_some(){"needs_recovery"}else{"ready"},
        "can_undo":!document.undo.is_empty(),"can_redo":!document.redo.is_empty()});
    if let Some(intent) = &document.rename_intent {
        value["rename"] = json!({"operation_id":intent.operation_id,"target_path":intent.target_path});
    }
    value
}
fn public_changes(changes: &[Change], content: &str) -> Vec<Value> {
    changes
        .iter()
        .map(|change| {
            let mut value=json!({"id":change.id,"from":change.from,"to":change.to,"original":change.original.as_str(),
                "replacement":change.replacement.as_str(),"decision":change.decision,"application":change.application,
                "version":change.version,"origin":change.origin,"task_id":change.task_id,"taken_over":change.taken_over,
                "candidate_versions":change.candidate_versions.iter().map(ReviewText::as_str).collect::<Vec<_>>(),
                "dependencies":change.dependencies,"last_event_id":change.last_event_id});
            value["current"] = json!(slice(content, change.from, change.to).ok());
            value
        })
        .collect()
}

fn compact_receipt(request: &Value, response: &Value) -> Value {
    let mut metadata = response.clone();
    let change_ids = response["changes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|change| change["id"].clone())
        .collect::<Vec<_>>();
    if let Some(object) = metadata.as_object_mut() {
        object.remove("content");
        object.remove("changes");
    }
    let mut compact_request = request.clone();
    for field in ["content", "disk_content", "saved_content"] {
        if request
            .get(field)
            .is_some_and(|value| value == &response["content"])
        {
            compact_request[field] = json!({"$review_response":"content"});
        } else if request
            .get(field)
            .is_some_and(|value| value == &response["save_intent"]["saved_content"])
        {
            compact_request[field] = json!({"$review_response":"saved_content"});
        }
    }
    if let Some(intent) = metadata
        .get_mut("save_intent")
        .and_then(Value::as_object_mut)
    {
        intent.remove("content");
        intent.remove("saved_content");
        intent.insert("$review_compact".into(), json!(true));
    }
    json!({"format":2,"request":compact_request,"snapshot":metadata,"change_ids":change_ids})
}
fn expand_receipt(
    tx: &Transaction<'_>,
    document: &Document,
    receipt: Value,
) -> Result<Value, &'static str> {
    // Schema-1 receipts were full JSON snapshots. Never reinterpret or discard
    // them during migration; they remain byte-for-byte equivalent JSON values.
    if receipt.get("format").and_then(Value::as_u64) != Some(2) {
        return Ok(receipt);
    }
    let mut metadata = receipt["snapshot"].clone();
    let target = number(&metadata, "last_event_id")?;
    if target > document.last_event_id {
        return Err("review_persistence_failed");
    }
    let mut content = document.content.clone();
    for mapping in document
        .mappings
        .iter()
        .rev()
        .filter(|mapping| mapping.event_id > target)
    {
        content = apply(&content, &mapping.inverse)?.content;
    }
    if document_transaction::revision(&content)
        != metadata["revision"]
            .as_str()
            .ok_or("review_persistence_failed")?
    {
        return Err("review_persistence_failed");
    }
    let mut changes = document.changes.clone();
    for payload in review_store::events_after(tx, &document.document_id, target)? {
        ensure_text_context(tx, &payload)?;
        let event: Value =
            serde_json::from_str(&payload).map_err(|_| "review_persistence_failed")?;
        if let Some(patches) = event.get("patches") {
            let patches: Vec<Patch> =
                serde_json::from_value(patches.clone()).map_err(|_| "review_persistence_failed")?;
            for patch in patches {
                changes.retain(|change| change.id != patch.id);
                if let Some(before) = patch.before {
                    changes.push(before);
                }
            }
        }
    }
    let ids: Vec<String> = serde_json::from_value(receipt["change_ids"].clone())
        .map_err(|_| "review_persistence_failed")?;
    let mut ordered = Vec::with_capacity(ids.len());
    for id in ids {
        ordered.push(
            changes
                .iter()
                .find(|change| change.id == id)
                .cloned()
                .ok_or("review_persistence_failed")?,
        );
    }
    metadata["content"] = json!(content);
    metadata["changes"] = json!(public_changes(&ordered, &content));
    if metadata["save_intent"].get("$review_compact") == Some(&json!(true)) {
        let steps: Vec<Vec<TextEdit>> =
            serde_json::from_value(metadata["save_intent"]["edit_steps"].clone())
                .map_err(|_| "review_persistence_failed")?;
        let mut saved = content.clone();
        for step in steps {
            saved = apply(&saved, &step)?.content;
        }
        metadata["save_intent"]["content"] = json!(content);
        metadata["save_intent"]["saved_content"] = json!(saved);
        metadata["save_intent"]
            .as_object_mut()
            .ok_or("review_persistence_failed")?
            .remove("$review_compact");
    }
    let mut request = receipt["request"].clone();
    for field in ["content", "disk_content", "saved_content"] {
        match request[field]
            .get("$review_response")
            .and_then(Value::as_str)
        {
            Some("content") => request[field] = metadata["content"].clone(),
            Some("saved_content") => {
                request[field] = metadata["save_intent"]["saved_content"].clone()
            }
            _ => (),
        }
    }
    Ok(json!({"request":request,"snapshot":metadata}))
}
fn cas(document: &Document, request: &Value) -> Result<(), &'static str> {
    if number(request, "generation")? != document.generation {
        return Err("review_stale_generation");
    }
    if number(request, "review_revision")? != document.review_revision {
        return Err("review_stale_revision");
    }
    Ok(())
}
fn next_event(document: &mut Document) -> Result<u64, &'static str> {
    document.last_event_id = next_counter(document.last_event_id)?;
    Ok(document.last_event_id)
}
fn next_counter(value: u64) -> Result<u64, &'static str> {
    value
        .checked_add(1)
        .filter(|next| *next <= i64::MAX as u64)
        .ok_or("review_budget")
}
fn open(tx: &Transaction<'_>, request: &Value) -> Result<Value, &'static str> {
    let requested_path = string(request, "path")?;
    path(requested_path)?;
    if review_store::pending_rename_at(tx, requested_path)?.is_some() { return Err("rename_recovery_pending"); }
    let content = normalize(string(request, "content")?);
    checked_content(&content)?;
    let disk_content = normalize(
        request
            .get("disk_content")
            .and_then(Value::as_str)
            .unwrap_or(string(request, "content")?),
    );
    checked_content(&disk_content)?;
    let disk = string(request, "disk_revision")?;
    if disk.is_empty() || disk.len() > 256 {
        return Err("review_invalid_request");
    }
    if let Some(payload) = review_store::by_path(tx, requested_path)? {
        let mut document = decode(tx, &payload)?;
        if document.rename_intent.is_some() || review_store::pending_rename_at(tx, &document.path)?.is_some() { return Err("rename_recovery_pending"); }
        if let Some(intent) = document.save_intent.clone() {
            if requested_path == intent.target_path {
                if disk == intent.expected_disk_revision && disk_content == intent.saved_content {
                    finish_intent(tx, &mut document, &intent, disk)?;
                } else if disk == intent.target_disk_revision {
                    // The target never changed. Discard only the file intent;
                    // unsaved content and review lineage remain durable.
                    let event = next_event(&mut document)?;
                    document.save_intent = None;
                    document.recovery = None;
                    review_store::event(
                        tx,
                        &document.document_id,
                        event,
                        "save_recovered_aborted",
                        &json!({"intent_id":intent.intent_id}),
                    )?;
                } else {
                    document.recovery = Some(
                        json!({"reason":"save_conflict","intent_id":intent.intent_id,
                        "external_path":requested_path,"external_disk_revision":disk,"external_content":disk_content}),
                    );
                }
            } else {
                document.recovery = Some(
                    json!({"reason":"save_target_unchecked","intent_id":intent.intent_id,
                    "target_path":intent.target_path,"external_path":requested_path,"external_disk_revision":disk,"external_content":disk_content}),
                );
            }
        } else if disk != document.disk_revision {
            document.recovery = Some(
                json!({"reason":"external_change","external_path":requested_path,
                "external_disk_revision":disk,"external_content":disk_content}),
            );
        } else if document.recovery.is_some() {
            document.recovery = None;
        }
        persist(tx, &document)?;
        return Ok(snapshot(&document));
    }
    let mut document = Document {
        document_id: Uuid::new_v4().to_string(),
        path: requested_path.into(),
        content: content.clone(),
        generation: 1,
        revision: document_transaction::revision(&content),
        review_revision: 0,
        disk_revision: disk.into(),
        encoding: request
            .get("encoding")
            .and_then(Value::as_str)
            .unwrap_or("utf-8")
            .into(),
        line_ending: request
            .get("line_ending")
            .and_then(Value::as_str)
            .unwrap_or(if string(request, "content")?.contains("\r\n") {
                "crlf"
            } else {
                "lf"
            })
            .into(),
        changes: Vec::new(),
        last_event_id: 0,
        undo: Vec::new(),
        redo: Vec::new(),
        mappings: Vec::new(),
        save_intent: None,
        rename_intent: None,
        recovery: None,
        checkpoint: Checkpoint {
            content: disk_content.clone(),
            changes: Vec::new(),
            event_id: 0,
        },
    };
    if document.content != disk_content {
        let content = document.content.clone();
        let action = record_action(
            tx,
            &mut document,
            "open_unsaved",
            "editor",
            vec![TextEdit {
                from: 0,
                to: disk_content.encode_utf16().count(),
                insert: content,
            }],
            &disk_content,
            Vec::new(),
            None,
        )?;
        document.undo.push(action);
    }
    persist(tx, &document)?;
    Ok(snapshot(&document))
}

fn edits(request: &Value) -> Result<Vec<TextEdit>, &'static str> {
    let edits: Vec<TextEdit> = serde_json::from_value(
        request
            .get("edits")
            .cloned()
            .ok_or("review_invalid_request")?,
    )
    .map_err(|_| "review_invalid_request")?;
    if edits.len() > MAX_OPERATIONS {
        return Err("review_budget");
    }
    for edit in &edits {
        if edit.insert.contains('\r') {
            return Err("review_invalid_request");
        }
    }
    Ok(edits)
}
fn apply(
    content: &str,
    edits: &[TextEdit],
) -> Result<document_transaction::TransactionResult, &'static str> {
    let result = document_transaction::apply(
        content,
        &ContentTransaction {
            base_revision: document_transaction::revision(content),
            origin: "review".into(),
            edits: edits.to_vec(),
        },
    )?;
    checked_content(&result.content)?;
    Ok(result)
}
fn slice(content: &str, from: usize, to: usize) -> Result<&str, &'static str> {
    if from > to {
        return Err("transaction_invalid_range");
    }
    let from = document_transaction::byte_offset(content, from)?;
    let to = document_transaction::byte_offset(content, to)?;
    Ok(&content[from..to])
}
fn sorted(edits: &[TextEdit]) -> Vec<TextEdit> {
    let mut result = edits.to_vec();
    result.sort_by_key(|edit| (edit.from, edit.to));
    result
}
fn shifted(point: usize, delta: isize) -> usize {
    point
        .checked_add_signed(delta)
        .expect("validated UTF16 mapping")
}
fn map_span(from: &mut usize, to: &mut usize, edit: &TextEdit) -> bool {
    let inserted = edit.insert.encode_utf16().count();
    let delta = inserted as isize - (edit.to - edit.from) as isize;
    // A later insertion exactly where an applied deletion lives takes over it.
    if *from == *to && edit.from == *from && edit.to == edit.from {
        *to += inserted;
        return true;
    }
    if edit.to <= *from {
        *from = shifted(*from, delta);
        *to = shifted(*to, delta);
        return false;
    }
    if edit.from >= *to {
        return false;
    }
    *from = if *from < edit.from { *from } else { edit.from };
    *to = if *to > edit.to {
        shifted(*to, delta)
    } else {
        edit.from + inserted
    };
    true
}
fn map_geometry(from: &mut usize, to: &mut usize, edits: &[TextEdit]) -> bool {
    let mut shift = 0isize;
    let mut touched = false;
    for edit in sorted(edits) {
        let adjusted = TextEdit {
            from: shifted(edit.from, shift),
            to: shifted(edit.to, shift),
            insert: edit.insert.clone(),
        };
        touched |= map_span(from, to, &adjusted);
        shift += edit.insert.encode_utf16().count() as isize - (edit.to - edit.from) as isize;
    }
    touched
}
fn map_changes(
    document: &mut Document,
    edits: &[TextEdit],
    content: &str,
    system: bool,
) -> Result<(), &'static str> {
    let next_version = next_counter(document.review_revision)?;
    for change in &mut document.changes {
        let prior = (change.from, change.to);
        let touched = map_geometry(&mut change.from, &mut change.to, edits);
        if touched {
            if system && change.application == "applied" && !change.taken_over {
                let replacement = slice(content, change.from, change.to)?.to_string();
                if replacement != change.replacement.as_str() {
                    if !change.candidate_versions.contains(&change.replacement) {
                        change.candidate_versions.push(change.replacement.clone());
                    }
                    change.replacement = ReviewText::new(replacement);
                }
            } else if change.decision != "withdrawn" {
                change.taken_over = true;
                change.decision = "conflict".into();
            }
        }
        if touched || prior != (change.from, change.to) {
            change.version = next_version;
        }
    }
    Ok(())
}
fn patches(before: &[Change], after: &[Change]) -> Vec<Patch> {
    let ids = before
        .iter()
        .chain(after)
        .map(|change| change.id.clone())
        .collect::<BTreeSet<_>>();
    ids.into_iter()
        .filter_map(|id| {
            let before = before.iter().find(|change| change.id == id).cloned();
            let after = after.iter().find(|change| change.id == id).cloned();
            (before != after).then_some(Patch { id, before, after })
        })
        .collect()
}
fn record_action(
    tx: &Transaction<'_>,
    document: &mut Document,
    kind: &str,
    origin: &str,
    edits: Vec<TextEdit>,
    before_content: &str,
    before_changes: Vec<Change>,
    compensates: Option<u64>,
) -> Result<Action, &'static str> {
    let result = apply(before_content, &edits)?;
    if result.content != document.content {
        return Err("review_conflict");
    }
    let event = next_event(document)?;
    document.review_revision = next_counter(document.review_revision)?;
    if !edits.is_empty() {
        document.generation = next_counter(document.generation)?;
    }
    document.revision = result.revision;
    flush_changes(tx, &before_changes)?;
    flush_changes(tx, &document.changes)?;
    let changed = patches(&before_changes, &document.changes);
    for change in &mut document.changes {
        if changed.iter().any(|patch| patch.id == change.id) {
            change.last_event_id = event;
        }
    }
    let patches = patches(&before_changes, &document.changes);
    let action = Action {
        event_id: event,
        edits: sorted(&edits),
        inverse: result.inverse,
        inverse_expected: sorted(&edits).into_iter().map(|edit| edit.insert).collect(),
        patches,
    };
    document.mappings.push(Mapping {
        event_id: event,
        edits: action.edits.clone(),
        inverse: action.inverse.clone(),
    });
    review_store::event(
        tx,
        &document.document_id,
        event,
        kind,
        &json!({"origin":origin,
        "generation":document.generation,"review_revision":document.review_revision,"revision":document.revision,
        "edits":action.edits,"inverse":action.inverse,"patches":action.patches,"compensates_event_id":compensates}),
    )?;
    Ok(action)
}
fn edit(
    tx: &Transaction<'_>,
    document: &mut Document,
    request: &Value,
    proposal: bool,
) -> Result<(), &'static str> {
    let origin = request
        .get("origin")
        .and_then(Value::as_str)
        .unwrap_or(if proposal { "ai" } else { "editor" });
    if !matches!(origin, "editor" | "undo" | "redo" | "system" | "ai") {
        return Err("transaction_invalid_origin");
    }
    let requested = edits(request)?;
    if requested.is_empty() {
        return Err("review_invalid_request");
    }
    let application = request
        .get("application")
        .and_then(Value::as_str)
        .unwrap_or("applied");
    if !matches!(application, "applied" | "proposed") {
        return Err("review_invalid_request");
    }
    let before_content = document.content.clone();
    let before_changes = document.changes.clone();
    let mut applied = requested.clone();
    if proposal && application == "proposed" {
        apply(&before_content, &requested)?;
        applied.clear();
    }
    let result = apply(&before_content, &applied)?;
    map_changes(document, &applied, &result.content, origin == "system")?;
    document.content = result.content;
    if proposal {
        if document.changes.len() + requested.len() > MAX_OPERATIONS {
            return Err("review_budget");
        }
        let dependencies: Vec<String> = request
            .get("dependencies")
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()
            .map_err(|_| "review_invalid_request")?
            .unwrap_or_default();
        if dependencies.iter().any(|id| {
            !document
                .changes
                .iter()
                .any(|change| &change.id == id && change.decision != "withdrawn")
        }) {
            return Err("review_conflict");
        }
        let mut shift = 0isize;
        for edit in sorted(&requested) {
            let original = slice(&before_content, edit.from, edit.to)?.to_string();
            let from = if application == "applied" {
                shifted(edit.from, shift)
            } else {
                edit.from
            };
            let to = if application == "applied" {
                from + edit.insert.encode_utf16().count()
            } else {
                edit.to
            };
            document.changes.push(Change {
                id: Uuid::new_v4().to_string(),
                from,
                to,
                original: ReviewText::new(original),
                replacement: ReviewText::new(edit.insert.clone()),
                decision: "pending".into(),
                application: application.into(),
                version: next_counter(document.review_revision)?,
                origin: origin.into(),
                task_id: request
                    .get("task_id")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                taken_over: false,
                candidate_versions: vec![ReviewText::new(edit.insert.clone())],
                dependencies: dependencies.clone(),
                last_event_id: 0,
            });
            shift += edit.insert.encode_utf16().count() as isize - (edit.to - edit.from) as isize;
        }
    }
    let action = record_action(
        tx,
        document,
        if proposal { "propose" } else { "edit" },
        origin,
        applied,
        &before_content,
        before_changes,
        None,
    )?;
    document.undo.push(action);
    document.redo.clear();
    Ok(())
}
fn resolve(
    tx: &Transaction<'_>,
    document: &mut Document,
    request: &Value,
) -> Result<(), &'static str> {
    let ids: Vec<String> = serde_json::from_value(
        request
            .get("ids")
            .cloned()
            .ok_or("review_invalid_request")?,
    )
    .map_err(|_| "review_invalid_request")?;
    if ids.is_empty()
        || ids.len() > MAX_OPERATIONS
        || ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
    {
        return Err("review_invalid_request");
    }
    let decision = string(request, "decision")?;
    if !matches!(decision, "accept" | "withdraw") {
        return Err("review_invalid_request");
    }
    let keep_current = match request.get("resolution") {
        None => false,
        Some(Value::String(value)) if value == "keep_current" && decision == "accept" => true,
        _ => return Err("review_invalid_request"),
    };
    let mut edits = Vec::new();
    for id in &ids {
        let change = document
            .changes
            .iter()
            .find(|change| &change.id == id)
            .ok_or("review_not_found")?;
        let conflicted = change.taken_over || change.decision == "conflict";
        if conflicted && !keep_current {
            return Err("review_conflict");
        }
        if keep_current && !conflicted {
            return Err("review_invalid_request");
        }
        if decision == "accept" && change.decision == "withdrawn" {
            return Err("review_conflict");
        }
        if decision == "withdraw"
            && change.decision != "withdrawn"
            && change.application == "applied"
        {
            if slice(&document.content, change.from, change.to)? != change.replacement.as_str() {
                return Err("review_conflict");
            }
            if document.changes.iter().any(|dependent| {
                dependent.dependencies.contains(id)
                    && dependent.decision != "withdrawn"
                    && !ids.contains(&dependent.id)
            }) {
                return Err("review_conflict");
            }
            edits.push(TextEdit {
                from: change.from,
                to: change.to,
                insert: change.original.as_str().into(),
            });
        }
    }
    let before_content = document.content.clone();
    let before_changes = document.changes.clone();
    let result = apply(&before_content, &edits)?;
    map_changes(document, &edits, &result.content, false)?;
    document.content = result.content;
    for change in &mut document.changes {
        if ids.contains(&change.id) {
            change.decision = if decision == "accept" {
                "accepted"
            } else {
                "withdrawn"
            }
            .into();
            if decision == "withdraw" {
                change.application = "proposed".into();
                change.taken_over = false;
            }
            change.version = next_counter(document.review_revision)?;
        }
    }
    let action = record_action(
        tx,
        document,
        "resolve",
        "editor",
        edits,
        &before_content,
        before_changes,
        None,
    )?;
    document.undo.push(action);
    document.redo.clear();
    Ok(())
}
fn compensate(
    tx: &Transaction<'_>,
    document: &mut Document,
    redo: bool,
) -> Result<(), &'static str> {
    let target = if redo {
        document.redo.pop().ok_or("review_redo_unavailable")?
    } else {
        document.undo.pop().ok_or("review_undo_unavailable")?
    };
    let mut edits = target.inverse.clone();
    for edit in &mut edits {
        for mapping in document
            .mappings
            .iter()
            .filter(|mapping| mapping.event_id > target.event_id)
        {
            map_geometry(&mut edit.from, &mut edit.to, &mapping.edits);
        }
    }
    for (edit, expected) in edits.iter().zip(&target.inverse_expected) {
        if slice(&document.content, edit.from, edit.to)? != expected {
            return Err("review_conflict");
        }
    }
    let before_content = document.content.clone();
    let before_changes = document.changes.clone();
    let result = apply(&before_content, &edits)?;
    map_changes(document, &edits, &result.content, false)?;
    document.content = result.content;
    for patch in &target.patches {
        document.changes.retain(|change| change.id != patch.id);
        if let Some(mut restored) = patch.before.clone() {
            // Rebase the stored range through the original event, every later
            // mapping and this compensation. No substring search is involved.
            for mapping in document
                .mappings
                .iter()
                .filter(|mapping| mapping.event_id >= target.event_id)
            {
                map_geometry(&mut restored.from, &mut restored.to, &mapping.edits);
            }
            map_geometry(&mut restored.from, &mut restored.to, &edits);
            restored.version = next_counter(document.review_revision)?;
            document.changes.push(restored);
        }
    }
    let action = record_action(
        tx,
        document,
        if redo { "redo" } else { "undo" },
        if redo { "redo" } else { "undo" },
        edits,
        &before_content,
        before_changes,
        Some(target.event_id),
    )?;
    if redo {
        document.undo.push(action);
    } else {
        document.redo.push(action);
    }
    Ok(())
}

fn prepare_save(
    tx: &Transaction<'_>,
    document: &mut Document,
    request: &Value,
) -> Result<(), &'static str> {
    if normalize(string(request, "content")?) != document.content {
        return Err("review_save_conflict");
    }
    if string(request, "disk_revision")? != document.disk_revision {
        return Err("review_save_conflict");
    }
    let target = request
        .get("target_path")
        .and_then(Value::as_str)
        .unwrap_or(&document.path);
    path(target)?;
    if !review_store::path_available(tx, target, &document.document_id)? {
        return Err("review_save_conflict");
    }
    let expected = string(request, "expected_disk_revision")?;
    if expected.is_empty() || expected.len() > 256 {
        return Err("review_invalid_request");
    }
    let mut steps = Vec::new();
    if let Some(input) = request.get("edit_steps") {
        let array = input.as_array().ok_or("review_invalid_request")?;
        if array.len() > MAX_OPERATIONS {
            return Err("review_budget");
        }
        for step in array {
            steps.push(edits(step)?);
        }
    } else if request.get("edits").is_some() {
        steps.push(edits(request)?);
    }
    let mut content = document.content.clone();
    let mut operations = 0usize;
    for step in &steps {
        operations += step.len();
        if operations > MAX_OPERATIONS {
            return Err("review_budget");
        }
        content = apply(&content, step)?.content;
    }
    let saved_content = normalize(
        request
            .get("saved_content")
            .and_then(Value::as_str)
            .unwrap_or(&document.content),
    );
    if content != saved_content {
        return Err("review_save_conflict");
    }
    checked_content(&saved_content)?;
    let encoding = request
        .get("encoding")
        .and_then(Value::as_str)
        .unwrap_or(&document.encoding);
    if crate::text_encoding::canonical_name(encoding).is_none() {
        return Err("review_invalid_request");
    }
    let line_ending = request
        .get("line_ending")
        .and_then(Value::as_str)
        .unwrap_or(&document.line_ending);
    if !matches!(line_ending, "lf" | "crlf" | "cr" | "LF" | "CRLF" | "CR") {
        return Err("review_invalid_request");
    }
    let mut intent = SaveIntent {
        intent_id: Uuid::new_v4().to_string(),
        generation: document.generation,
        review_revision: document.review_revision,
        source_path: document.path.clone(),
        target_path: target.into(),
        disk_revision: document.disk_revision.clone(),
        target_disk_revision: request
            .get("target_disk_revision")
            .and_then(Value::as_str)
            .unwrap_or(if target == document.path {
                &document.disk_revision
            } else {
                "missing"
            })
            .into(),
        expected_disk_revision: expected.into(),
        content: document.content.clone(),
        saved_content,
        edit_steps: steps,
        encoding: encoding.into(),
        line_ending: line_ending.to_ascii_lowercase(),
        reserved_bytes: 0,
    };
    let event = next_event(document)?;
    let mut logged_intent =
        serde_json::to_value(&intent).map_err(|_| "review_persistence_failed")?;
    if let Some(object) = logged_intent.as_object_mut() {
        object.remove("content");
        object.remove("saved_content");
    }
    review_store::event(
        tx,
        &document.document_id,
        event,
        "prepare_save",
        &json!({"intent":logged_intent}),
    )?;
    document.save_intent = Some(intent.clone());
    persist(tx, document)?;
    let before = review_store::used_bytes(tx)?;
    // Reserve the measured completion growth before permitting a filesystem
    // rename. This also checks that all required journal event slots remain.
    tx.execute_batch("SAVEPOINT review_save_preflight")
        .map_err(|_| "review_persistence_failed")?;
    let completion = (|| {
        let mut projected = document.clone();
        finish_intent(tx, &mut projected, &intent, &intent.expected_disk_revision)?;
        // New immutable rewrite versions and events exist before the prepared
        // row releases its old content. Reserve that peak, not only net growth.
        let peak = review_store::used_bytes(tx)?;
        persist(tx, &projected)?;
        let call = "x".repeat(256);
        let receipt_request = json!({"action":"commit_save","document_id":document.document_id,
            "intent_id":intent.intent_id,"call_id":call,"disk_revision":intent.expected_disk_revision,
            "content":intent.saved_content,"encoding":intent.encoding,"line_ending":intent.line_ending});
        let mut receipt = snapshot(&projected);
        receipt["call_id"] = json!(call);
        receipt["durable"] = json!(true);
        let compact = compact_receipt(&receipt_request, &receipt);
        review_store::put_receipt(tx, &document.document_id, &call, &"0".repeat(64), &compact)?;
        Ok::<_, &'static str>(peak.max(review_store::used_bytes(tx)?))
    })();
    tx.execute_batch("ROLLBACK TO review_save_preflight; RELEASE review_save_preflight")
        .map_err(|_| "review_persistence_failed")?;
    intent.reserved_bytes = completion?.saturating_sub(before).saturating_add(8192);
    document.save_intent = Some(intent);
    Ok(())
}
fn commit_save(
    tx: &Transaction<'_>,
    document: &mut Document,
    request: &Value,
) -> Result<(), &'static str> {
    let intent = document.save_intent.clone().ok_or("review_save_conflict")?;
    if string(request, "intent_id")? != intent.intent_id {
        return Err("review_save_conflict");
    }
    if string(request, "disk_revision")? != intent.expected_disk_revision
        || normalize(string(request, "content")?) != intent.saved_content
    {
        return Err("review_save_conflict");
    }
    if request
        .get("encoding")
        .and_then(Value::as_str)
        .is_some_and(|value| value != intent.encoding)
        || request
            .get("line_ending")
            .and_then(Value::as_str)
            .is_some_and(|value| value.to_ascii_lowercase() != intent.line_ending)
    {
        return Err("review_save_conflict");
    }
    finish_intent(tx, document, &intent, string(request, "disk_revision")?)
}
fn finish_intent(
    tx: &Transaction<'_>,
    document: &mut Document,
    intent: &SaveIntent,
    disk: &str,
) -> Result<(), &'static str> {
    if document.generation != intent.generation
        || document.review_revision != intent.review_revision
        || document.content != intent.content
    {
        return Err("review_save_conflict");
    }
    if !review_store::path_available(tx, &intent.target_path, &document.document_id)? {
        return Err("review_save_conflict");
    }
    // Each stage has coordinates in the preceding stage, never in disk CRLF.
    for step in &intent.edit_steps {
        if step.is_empty() {
            continue;
        }
        let before_content = document.content.clone();
        let before_changes = document.changes.clone();
        let result = apply(&before_content, step)?;
        map_changes(document, step, &result.content, true)?;
        document.content = result.content;
        record_action(
            tx,
            document,
            "save_rewrite",
            "system",
            step.clone(),
            &before_content,
            before_changes,
            None,
        )?;
    }
    if document.content != intent.saved_content {
        return Err("review_save_conflict");
    }
    document.path = intent.target_path.clone();
    document.disk_revision = disk.into();
    document.encoding = intent.encoding.clone();
    document.line_ending = intent.line_ending.clone();
    document.save_intent = None;
    document.recovery = None;
    let event = next_event(document)?;
    review_store::event(
        tx,
        &document.document_id,
        event,
        "commit_save",
        &json!({"intent_id":intent.intent_id,
        "disk_revision":disk,"generation":document.generation,"review_revision":document.review_revision,
        "source_path":intent.source_path,"target_path":intent.target_path}),
    )?;
    document.checkpoint = Checkpoint {
        content: document.content.clone(),
        changes: document.changes.clone(),
        event_id: event,
    };
    Ok(())
}

fn discard(
    tx: &Transaction<'_>,
    document: &mut Document,
    request: &Value,
) -> Result<(), &'static str> {
    if string(request, "disk_revision")? != document.disk_revision
        || normalize(string(request, "disk_content")?) != document.checkpoint.content
    {
        return Err("review_save_conflict");
    }
    let current_changes = document.changes.clone();
    let checkpoint = document.checkpoint.clone();
    let mappings = document
        .mappings
        .iter()
        .filter(|mapping| mapping.event_id > checkpoint.event_id)
        .cloned()
        .rev()
        .collect::<Vec<_>>();
    for mapping in mappings {
        if mapping.edits.is_empty() {
            continue;
        }
        let before_content = document.content.clone();
        let before_changes = document.changes.clone();
        let result = apply(&before_content, &mapping.inverse)?;
        map_changes(document, &mapping.inverse, &result.content, false)?;
        document.content = result.content;
        record_action(
            tx,
            document,
            "discard_compensation",
            "system",
            mapping.inverse,
            &before_content,
            before_changes,
            Some(mapping.event_id),
        )?;
    }
    if document.content != checkpoint.content {
        return Err("review_conflict");
    }
    let before_changes = document.changes.clone();
    document.changes = checkpoint.changes;
    for saved in &mut document.changes {
        if let Some(current) = current_changes.iter().find(|change| change.id == saved.id) {
            if current.original == saved.original
                && current.replacement == saved.replacement
                && current.application == saved.application
                && !current.taken_over
                && matches!(current.decision.as_str(), "pending" | "accepted")
            {
                saved.decision = current.decision.clone();
            }
        }
        saved.version = next_counter(document.review_revision)?;
    }
    let content = document.content.clone();
    record_action(
        tx,
        document,
        "discard",
        "system",
        Vec::new(),
        &content,
        before_changes,
        Some(checkpoint.event_id),
    )?;
    document.checkpoint.event_id = document.last_event_id;
    document.undo.clear();
    document.redo.clear();
    Ok(())
}
fn abort_save(
    tx: &Transaction<'_>,
    document: &mut Document,
    request: &Value,
) -> Result<(), &'static str> {
    let intent = document
        .save_intent
        .as_ref()
        .ok_or("review_save_conflict")?;
    if string(request, "intent_id")? != intent.intent_id {
        return Err("review_save_conflict");
    }
    let intent_id = intent.intent_id.clone();
    document.save_intent = None;
    document.recovery = None;
    let event = next_event(document)?;
    review_store::event(
        tx,
        &document.document_id,
        event,
        "abort_save",
        &json!({"intent_id":intent_id}),
    )?;
    Ok(())
}
fn collect_asset_bases(value: &Value, bases: &mut BTreeSet<PathBuf>) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if matches!(
                    key.as_str(),
                    "path" | "source_path" | "target_path" | "external_path"
                ) {
                    if let Some(path) = value.as_str().filter(|path| !path.starts_with("draft:")) {
                        let path = Path::new(path);
                        if path.is_absolute() {
                            if let Some(parent) = path.parent() {
                                bases.insert(parent.to_path_buf());
                            }
                        }
                    }
                }
                collect_asset_bases(value, bases);
            }
        }
        Value::Array(array) => {
            for value in array {
                collect_asset_bases(value, bases)
            }
        }
        _ => (),
    }
}
fn collect_pins(
    value: &Value,
    bases: &BTreeSet<PathBuf>,
    texts: &HashMap<String, String>,
    visited: &mut BTreeSet<String>,
    paths: &mut BTreeSet<String>,
) -> Result<(), &'static str> {
    match value {
        Value::Object(object) => {
            if let Some(hash) = object.get("$review_text").and_then(Value::as_str) {
                if visited.insert(hash.into()) {
                    let text = texts.get(hash).ok_or("review_persistence_failed")?;
                    if document_transaction::revision(text) != hash {
                        return Err("review_persistence_failed");
                    }
                    collect_text_pins(text, bases, paths);
                }
                return Ok(());
            }
            for value in object.values() {
                collect_pins(value, bases, texts, visited, paths)?;
            }
        }
        Value::Array(array) => {
            for value in array {
                collect_pins(value, bases, texts, visited, paths)?;
            }
        }
        Value::String(text) => collect_text_pins(text, bases, paths),
        _ => (),
    }
    Ok(())
}
fn decode_asset_path(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor] == b'%' {
            if cursor + 2 >= bytes.len()
                || !bytes[cursor + 1].is_ascii_hexdigit()
                || !bytes[cursor + 2].is_ascii_hexdigit()
            {
                return None;
            }
            cursor += 3;
        } else {
            cursor += 1;
        }
    }
    let decoded = percent_encoding::percent_decode_str(value)
        .decode_utf8()
        .ok()?
        .into_owned();
    if decoded.is_empty() || decoded.chars().any(char::is_control) {
        return None;
    }
    Some(decoded)
}
fn local_asset_path(reference: &str) -> Option<String> {
    let decoded = if let Some(query) = reference.strip_prefix("/raw?p=") {
        decode_asset_path(query.split('&').next().unwrap_or(""))?
    } else {
        let reference = reference.trim();
        if reference.starts_with('#') || reference.starts_with("//") {
            return None;
        }
        decode_asset_path(reference.split(['?', '#']).next().unwrap_or(""))?
    };
    let bytes = decoded.as_bytes();
    let drive = bytes.len() > 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\');
    if !drive {
        if let Some(colon) = decoded.find(':') {
            let scheme = &decoded[..colon];
            if !scheme.is_empty()
                && scheme
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
            {
                return None;
            }
        }
    }
    if decoded.starts_with("//") {
        return None;
    }
    Some(decoded)
}
fn lexical_asset_path(path: &Path) -> PathBuf {
    let mut output = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => (),
            Component::ParentDir => match output.components().next_back() {
                Some(Component::Normal(_)) => {
                    output.pop();
                }
                _ if !path.has_root() => output.push(".."),
                _ => (),
            },
            component => output.push(component.as_os_str()),
        }
    }
    output
}
fn collect_text_pins(text: &str, bases: &BTreeSet<PathBuf>, paths: &mut BTreeSet<String>) {
    let mut references = crate::document_assets::image_references(text)
        .into_iter()
        .map(|(_, reference)| reference)
        .collect::<Vec<_>>();
    // A precise edit can retain just an image destination or filename rather
    // than a whole Markdown image. Keep those immutable originals as well.
    let bare = text.trim();
    if bare.len() <= 32768
        && !bare.contains(['\r', '\n'])
        && !bare.contains("](")
        && !bare.starts_with("![")
    {
        let extension = Path::new(bare.split(['?', '#']).next().unwrap_or(""))
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if bare.starts_with("/raw?p=")
            || bare.contains("readmd-assets/")
            || bare.contains("readmd-assets\\")
            || matches!(
                extension.as_str(),
                "png"
                    | "jpg"
                    | "jpeg"
                    | "gif"
                    | "webp"
                    | "svg"
                    | "bmp"
                    | "tif"
                    | "tiff"
                    | "emf"
                    | "wmf"
                    | "jp2"
                    | "avif"
                    | "ico"
                    | "heic"
                    | "mp3"
                    | "m4a"
                    | "wav"
                    | "ogg"
                    | "flac"
                    | "mp4"
                    | "webm"
                    | "mov"
                    | "pdf"
                    | "bin"
            )
        {
            references.push(bare.into());
        }
    }
    for reference in references {
        let Some(local) = local_asset_path(&reference) else {
            continue;
        };
        let path = Path::new(&local);
        if path.is_absolute() {
            paths.insert(lexical_asset_path(path).to_string_lossy().into_owned());
        } else if bases.is_empty() {
            paths.insert(lexical_asset_path(path).to_string_lossy().into_owned());
        } else {
            for base in bases {
                paths.insert(
                    lexical_asset_path(&base.join(path))
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
}

#[cfg(test)]
#[path = "review_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "review_rename_tests.rs"]
mod rename_tests;
