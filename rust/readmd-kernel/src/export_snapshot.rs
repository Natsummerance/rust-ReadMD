//! Durable, owned inputs for reviewed exports. Output publication belongs to
//! the server; this journal records intent before that owner's atomic rename.
use crate::review;
use pulldown_cmark::{Event, Options, Parser, Tag};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs::{self, File}, io::{Read, Write}, path::{Component, Path, PathBuf}, sync::OnceLock, time::{Duration, SystemTime, UNIX_EPOCH}};
use uuid::Uuid;

pub const MAX_SNAPSHOT_BYTES: usize = 64 * 1024 * 1024;
const MAX_RESOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_FILES: usize = 256;
const MAX_SNAPSHOTS: usize = 128;
const INVALID: &str = "export_snapshot_invalid_request";
const IO: &str = "export_snapshot_persistence_failed";
const BUDGET: &str = "export_snapshot_budget";

pub struct Service { data_dir: PathBuf, byte_limit: usize }

/// Private rendering inputs. `review` retains the exact authoritative text;
/// `content` only projects verified resources into owned paths/data URIs.
#[derive(Clone, Debug)]
pub struct Frozen {
    pub snapshot_id: String,
    pub content: String,
    pub base_dir: PathBuf,
    pub options: Value,
    pub resolved_options: Value,
    pub format: String,
    pub name: String,
    pub assets_dir: PathBuf,
    pub engine_identity: Value,
    pub asset_identity: Value,
    pub font_identity: Value,
    pub review: Value,
    pub protected_paths: Vec<PathBuf>,
    pub runtime_ready: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct Resource { path: String, bytes: usize, sha256: String }
#[derive(Clone, Serialize, Deserialize, PartialEq)]
struct Output { target: PathBuf, target_before: Option<String>, expected_sha256: String, dependencies: Vec<(PathBuf, String)> }
#[derive(Clone, Serialize, Deserialize)]
struct Record {
    snapshot_id: String, call_id: String, fingerprint: String, format: String, name: String,
    review: Value, content: String, options: Value, resolved_options: Value,
    state: String, resources: Vec<Resource>, engine_identity: Value, font_identity: Value,
    protected_paths: Vec<PathBuf>, figures: Option<Value>, output: Option<Output>, receipt: Option<Value>, error: Option<String>,
    #[serde(default)] created_at: u64,
    #[serde(default)] discarded: bool,
    #[serde(default="runtime_ready_default")] runtime_ready: bool,
    #[serde(default)] pending_engine: bool,
    #[serde(default)] pending_fonts: bool,
}

impl Service {
    /// Ordinary reader construction creates neither directories nor a database.
    pub fn new(data_dir: &Path) -> Self { Self { data_dir: data_dir.to_path_buf(), byte_limit: MAX_SNAPSHOT_BYTES } }
    fn root(&self) -> PathBuf { self.data_dir.join("export-snapshots") }
    fn directory(&self, id: &str) -> Result<PathBuf, &'static str> {
        if Uuid::parse_str(id).ok().is_none_or(|value| value.to_string() != id) { return Err(INVALID); }
        Ok(self.root().join(id))
    }
    fn run<T>(&self, f: impl FnOnce(&Transaction<'_>) -> Result<T, &'static str>) -> Result<T, &'static str> {
        if !self.data_dir.is_absolute() { return Err(INVALID); }
        fs::create_dir_all(&self.data_dir).map_err(|_| IO)?;
        check_directory(&self.data_dir)?;
        match fs::create_dir(self.root()) {
            Ok(()) => (), Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (), Err(_) => return Err(IO),
        }
        check_directory(&self.root())?;
        let database = self.root().join("records.sqlite3");
        match fs::symlink_metadata(&database) {
            Ok(metadata) if !plain(&metadata)||!metadata.is_file()=>return Err(IO),
            Err(error) if error.kind()!=std::io::ErrorKind::NotFound=>return Err(IO),_=>(),
        }
        let mut connection = Connection::open(database).map_err(|_| IO)?;
        connection.busy_timeout(Duration::from_secs(3)).map_err(|_| IO)?;
        let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0)).map_err(|_| IO)?;
        if version > 1 { return Err("export_snapshot_unsupported_schema"); }
        connection.pragma_update(None, "synchronous", "FULL").map_err(|_| IO)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|_| IO)?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS export_snapshots(id TEXT PRIMARY KEY NOT NULL,call_id TEXT UNIQUE NOT NULL,fingerprint TEXT NOT NULL,payload TEXT NOT NULL,logical_bytes INTEGER NOT NULL); PRAGMA user_version=1;").map_err(|_| IO)?;
        let result = f(&tx)?;
        let used: i64 = tx.query_row("SELECT COALESCE(SUM(logical_bytes),0) FROM export_snapshots", [], |row| row.get(0)).map_err(|_| IO)?;
        if used < 0 || used as u64 > self.byte_limit as u64 { return Err(BUDGET); }
        tx.commit().map_err(|_| IO)?;
        Ok(result)
    }

    pub fn prepare(&self, review: &review::Service, request: &Value, assets_dir: &Path) -> Result<Value, &'static str> {
        let call_id = text(request, "call_id")?;
        if call_id.is_empty() || call_id.len() > 256 { return Err(INVALID); }
        let format = text(request, "format")?;
        if !matches!(format, "html" | "pdf" | "docx" | "epub" | "tex") { return Err("unsupported_format"); }
        let name = text(request, "suggested_name")?;
        if name.is_empty() || name.len() > 256 || name.chars().any(|c| c.is_control() || "/\\:".contains(c)) { return Err(INVALID); }
        let options = request.get("options").filter(|value| value.is_object()).ok_or(INVALID)?.clone();
        let resolved_options = request.get("resolved_options").cloned().unwrap_or(Value::Null);
        if !resolved_options.is_null() && resolved_options.get("canonical") != Some(&options) { return Err(INVALID); }
        let fingerprint = match request.get("prepare_identity") {
            None=>digest(&serde_json::to_vec(&canonical(request)).map_err(|_| INVALID)?),
            Some(value)=>{let identity=value.as_str().filter(|identity|hash_valid(identity)).ok_or(INVALID)?;identity.into()},
        };
        self.cleanup_finished()?;
        self.run(|tx| {
            if let Some((old, payload)) = tx.query_row("SELECT fingerprint,payload FROM export_snapshots WHERE call_id=?1", [call_id],
                |row| Ok((row.get::<_,String>(0)?, row.get::<_,String>(1)?))).optional().map_err(|_| IO)? {
                if old != fingerprint { return Err("export_snapshot_call_conflict"); }
                return Ok(metadata(&decode(&payload)?));
            }
            let count: i64 = tx.query_row("SELECT COUNT(*) FROM export_snapshots WHERE COALESCE(json_extract(payload,'$.discarded'),0)=0", [], |row| row.get(0)).map_err(|_| IO)?;
            let count=usize::try_from(count).map_err(|_|IO)?;
            if count >= MAX_SNAPSHOTS { return Err(BUDGET); }
            let frozen_review = review.freeze_export_snapshot(request)?;
            let original = text(&frozen_review, "content")?;
            if original.len().saturating_add(options.to_string().len()).saturating_add(resolved_options.to_string().len()) > self.byte_limit { return Err(BUDGET); }
            let document_path = Path::new(text(&frozen_review, "path")?);
            let draft = text(&frozen_review, "path")?.strip_prefix("draft:");
            let base = if let Some(draft) = draft {
                if draft.is_empty() || draft.len() > 180 || !draft.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c,b'_'|b'-'|b'.')) { return Err(INVALID); }
                None
            } else {
                let base = document_path.parent().filter(|path| path.is_absolute()).ok_or(INVALID)?;
                check_directory(base)?; Some(base)
            };
            let snapshot_id = Uuid::new_v4().to_string();
            let directory = self.directory(&snapshot_id)?;
            // Staging is owned by this request. No recovery or cleanup scanner
            // ever adopts an unknown directory as a snapshot or an output.
            let temporary = tempfile::Builder::new().prefix("preparing-").tempdir_in(self.root()).map_err(|_| IO)?;
            fs::create_dir(temporary.path().join("source")).map_err(|_| IO)?;
            fs::create_dir(temporary.path().join("assets")).map_err(|_| IO)?;
            let mut record = Record { snapshot_id, call_id: call_id.into(), fingerprint, format: format.into(), name: name.into(),
                review: frozen_review.clone(), content: original.into(), options, resolved_options, state: "prepared".into(), resources: Vec::new(),
                engine_identity: Value::Null, font_identity: Value::Null,
                protected_paths: if draft.is_some() { Vec::new() } else { vec![absolute_target(document_path)?] },
                figures: None, output: None, receipt: None, error: None, created_at: now(), discarded: false,
                runtime_ready:true,pending_engine:false,pending_fonts:false };
            // Only the profile-owned staging protocol can remap an absolute
            // reference. Model-supplied file paths do not grant read authority.
            let staged = crate::document_assets::discover_staged(original, &self.data_dir).map_err(|_| "export_snapshot_resource_path")?;
            let mut staged_bytes = 0usize;
            for asset in &staged {
                let path = Path::new(&asset.path);
                let bytes = read_absolute(path, MAX_RESOURCE_BYTES)?;
                staged_bytes = staged_bytes.checked_add(bytes.len()).ok_or(BUDGET)?;
                if staged_bytes > self.byte_limit { return Err(BUDGET); }
                record.protected_paths.push(absolute_target(path)?);
            }
            if !staged.is_empty() {
                let mut materialized = crate::document_assets::materialize_staged(original, &temporary.path().join("source/document.md"), &staged, &self.data_dir)
                    .map_err(|_| "export_snapshot_resource_path")?;
                record.content = materialized.text.clone();
                for asset in &materialized.assets {
                    let path = Path::new(&asset.path);
                    let relative = path.strip_prefix(temporary.path()).map_err(|_| IO)?.to_string_lossy().replace('\\', "/");
                    let bytes = read_absolute(path, MAX_RESOURCE_BYTES)?;
                    record.resources.push(Resource { path: relative, bytes: bytes.len(), sha256: digest(&bytes) });
                }
                materialized.commit();
            }
            let names = resource_names(&record.content, &record.options, format)?;
            for name in names {
                let relative = resource_path(&name)?;
                let frozen_name = format!("source/{}", path_string(&relative)?);
                if record.resources.iter().any(|resource| resource.path == frozen_name) { continue; }
                let source = base.ok_or("export_snapshot_resource_path")?.join(&relative);
                let bytes = read_absolute(&source, MAX_RESOURCE_BYTES)?;
                validate_resource(&relative, &bytes)?;
                record.protected_paths.push(absolute_target(&source)?);
                copy_resource(temporary.path(), &frozen_name, &bytes, &mut record.resources)?;
                check_record_budget(&record, self.byte_limit)?;
            }
            if format == "html" {
                freeze_html_projection(&mut record,temporary.path())?;
                for name in ["marked.min.js", "mathjax/tex-svg.js"] {
                    let source = if assets_dir.join(name).exists() { assets_dir.join(name) } else { assets_dir.join("vendor").join(name) };
                    let bytes = read_absolute(&source, MAX_RESOURCE_BYTES)?;
                    if bytes.is_empty() { return Err("export_snapshot_resource_missing"); }
                    copy_resource(temporary.path(), &format!("assets/{name}"), &bytes, &mut record.resources)?;
                }
            }
            // Locks are application resources, never request-nominated paths.
            // Their bytes are frozen; installed engines/fonts remain protected
            // by their normal renderer leases and exact runtime revalidation.
            for name in runtime_locks(format,&record.options) {
                let source = assets_dir.join("capabilities").join(&name);
                if source.exists() {
                    let bytes = read_absolute(&source, MAX_RESOURCE_BYTES)?;
                    copy_resource(temporary.path(), &format!("assets/capabilities/{name}"), &bytes, &mut record.resources)?;
                }
            }
            (record.engine_identity, record.font_identity) = runtime_identity(&self.data_dir, assets_dir, format, &record.options)?;
            record.font_identity["document"] = json!(record.resources.iter().filter(|resource|font_resource(&resource.path)).collect::<Vec<_>>());
            record.pending_engine=record.engine_identity["pending"]==true;
            record.pending_fonts=needs_fonts(format,&record.content,&record.options)&&managed_fonts_absent(&record.font_identity);
            record.runtime_ready=!record.pending_engine&&!record.pending_fonts;
            let supplied = record.options.pointer("/figures/rendered").cloned();
            if let Some(rendered) = supplied { assign_figures(&mut record, &rendered)?; }
            check_record_budget(&record, self.byte_limit)?;
            // Admission counts all retained inputs and receipts. It never
            // evicts an unfinished output intent to admit a newer snapshot.
            let used: i64 = tx.query_row("SELECT COALESCE(SUM(logical_bytes),0) FROM export_snapshots", [], |row| row.get(0)).map_err(|_| IO)?;
            let used=usize::try_from(used).map_err(|_|IO)?;
            if used.saturating_add(record_bytes(&record)?) > self.byte_limit { return Err(BUDGET); }
            for resource in &record.resources { sync_directory(temporary.path().join(storage_path(&resource.path)?).parent().ok_or(IO)?)?; }
            sync_directory(temporary.path())?;
            let prepared_path=temporary.keep();
            fs::rename(&prepared_path, &directory).map_err(|_| IO)?;
            sync_directory(&self.root())?;
            // A failed durable insert leaves an unknown owned directory for
            // diagnosis, never an acknowledged or automatically exported job.
            put(tx, &record)?;
            Ok(metadata(&record))
        })
    }

    pub fn get(&self, id: &str) -> Result<Value, &'static str> {
        self.directory(id)?;
        self.run(|tx| Ok(metadata(&get(tx, id)?)))
    }
    pub fn status(&self, id: &str) -> Result<Value, &'static str> { self.get(id) }

    /// Resolve an identical public prepare call before re-resolving any live
    /// source styles. The API owner derives this identity, never a client flag.
    pub fn lookup_prepare(&self,call_id:&str,identity:&str)->Result<Option<Value>,&'static str> {
        if call_id.is_empty()||call_id.len()>256||!hash_valid(identity) {return Err(INVALID);}
        match fs::symlink_metadata(self.root().join("records.sqlite3")) {
            Err(error) if error.kind()==std::io::ErrorKind::NotFound=>return Ok(None),
            Err(_)=>return Err(IO),Ok(metadata) if !plain(&metadata)||!metadata.is_file()=>return Err(IO),_=>(),
        }
        self.run(|tx| {
            let found=tx.query_row("SELECT fingerprint,payload FROM export_snapshots WHERE call_id=?1",[call_id],
                |row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?))).optional().map_err(|_|IO)?;
            match found {
                Some((previous,payload)) if previous==identity=>Ok(Some(metadata(&decode(&payload)?))),
                Some(_)=>Err("export_snapshot_call_conflict"),None=>Ok(None),
            }
        })
    }

    pub fn load(&self, id: &str) -> Result<Frozen, &'static str> {
        let directory = self.directory(id)?;
        self.run(|tx| {
            let record = get(tx, id)?;
            if record.discarded || matches!(record.state.as_str(), "cancelled" | "failed") { return Err("export_snapshot_unavailable"); }
            check_directory(&directory)?;
            for resource in &record.resources {
                let bytes = read_absolute(&directory.join(storage_path(&resource.path)?), MAX_RESOURCE_BYTES)?;
                if bytes.len() != resource.bytes || digest(&bytes) != resource.sha256 { return Err("export_snapshot_resource_changed"); }
            }
            Ok(Frozen { snapshot_id: record.snapshot_id, content: record.content, base_dir: directory.join("source"), options: record.options,
                resolved_options: record.resolved_options, format: record.format, name: record.name, assets_dir: directory.join("assets"),
                engine_identity: record.engine_identity, asset_identity:json!(record.resources), font_identity: record.font_identity,
                review: record.review, protected_paths: record.protected_paths,runtime_ready:record.runtime_ready })
        })
    }

    pub fn validate_runtime(&self, frozen: &Frozen, assets_dir: &Path) -> Result<(), &'static str> {
        if !frozen.runtime_ready {return Err("export_snapshot_runtime_unavailable");}
        let (engine, fonts) = runtime_identity(&self.data_dir, assets_dir, &frozen.format, &frozen.options)?;
        let mut expected=frozen.font_identity.clone();
        let document=expected.as_object_mut().ok_or(IO)?.remove("document").unwrap_or_else(||json!([]));
        if engine != frozen.engine_identity || fonts != expected { return Err("export_snapshot_runtime_changed"); }
        let directory=self.directory(&frozen.snapshot_id)?;
        for resource in document.as_array().ok_or(IO)? {
            let bytes=read_absolute(&directory.join(storage_path(text(resource,"path")?)?),MAX_RESOURCE_BYTES)?;
            if digest(&bytes)!=text(resource,"sha256")? {return Err("export_snapshot_runtime_changed");}
        }
        Ok(())
    }

    /// Installation happens outside the snapshot transaction. Only a runtime
    /// that was unavailable may be filled afterwards; source and image bytes
    /// remain the already-owned inputs and are never rediscovered here.
    pub fn bind_runtime(&self,id:&str,assets_dir:&Path)->Result<Value,&'static str> {
        let directory=self.directory(id)?;
        self.run(|tx| {
            let mut record=get(tx,id)?;
            if record.state!="prepared"||record.output.is_some()||record.discarded {return Err("export_snapshot_commit_in_progress");}
            let (engine,mut fonts)=runtime_identity(&self.data_dir,assets_dir,&record.format,&record.options)?;
            let mut expected_fonts=record.font_identity.clone();
            let document=expected_fonts.as_object_mut().ok_or(IO)?.remove("document").unwrap_or_else(||json!([]));
            if record.runtime_ready {
                if engine!=record.engine_identity||fonts!=expected_fonts {return Err("export_snapshot_runtime_changed");}
                return Ok(metadata(&record));
            }
            if engine["pending"]==true {return Err(missing_engine_error(&record.options));}
            if needs_fonts(&record.format,&record.content,&record.options)&&managed_fonts_absent(&fonts) {return Err("publication_fonts_unavailable");}
            let mut expected_engine=record.engine_identity.clone();let mut actual_engine=engine.clone();
            if record.pending_engine {
                for value in [&mut expected_engine,&mut actual_engine] {
                    let object=value.as_object_mut().ok_or(IO)?;
                    for key in ["pending","binary","manifest_sha256"] {object.remove(key);}
                }
            }
            if expected_engine!=actual_engine {return Err("export_snapshot_runtime_changed");}
            // Core fonts and previously available managed fonts stay fixed.
            // A missing managed pack can be filled alongside a pending engine.
            if record.pending_fonts || (record.pending_engine&&managed_fonts_absent(&expected_fonts)) {
                let mut actual_fonts=fonts.clone();
                for value in [&mut expected_fonts,&mut actual_fonts] {
                    let object=value.as_object_mut().ok_or(IO)?;object.remove("managed");object.remove("manifest");
                }
                if expected_fonts!=actual_fonts {return Err("export_snapshot_runtime_changed");}
            } else if expected_fonts!=fonts {return Err("export_snapshot_runtime_changed");}
            let old_bytes=record_bytes(&record)?;
            let mut manifests=Vec::new();
            for name in runtime_locks(&record.format,&record.options) {
                let source=assets_dir.join("capabilities").join(&name);
                if !source.exists() {continue;}
                let bytes=read_absolute(&source,MAX_RESOURCE_BYTES)?;
                let path=format!("assets/capabilities/{name}");
                if let Some(previous)=record.resources.iter().find(|resource|resource.path==path) {
                    if previous.sha256!=digest(&bytes)||previous.bytes!=bytes.len() {return Err("export_snapshot_runtime_changed");}
                } else {record.resources.push(Resource {path:path.clone(),bytes:bytes.len(),sha256:digest(&bytes)});}
                manifests.push((path,bytes));
            }
            fonts["document"]=document;
            record.engine_identity=engine;record.font_identity=fonts;
            record.runtime_ready=true;record.pending_engine=false;record.pending_fonts=false;
            check_record_budget(&record,self.byte_limit)?;
            let used:i64=tx.query_row("SELECT COALESCE(SUM(logical_bytes),0) FROM export_snapshots",[],|row|row.get(0)).map_err(|_|IO)?;
            let used=usize::try_from(used).map_err(|_|IO)?;
            if used.checked_sub(old_bytes).ok_or(IO)?.saturating_add(record_bytes(&record)?)>self.byte_limit {return Err(BUDGET);}
            for (name,bytes) in manifests {ensure_owned_resource(&directory,&name,&bytes)?;}
            sync_directory(&directory)?;
            put(tx,&record)?;
            Ok(metadata(&record))
        })
    }

    pub fn complete_figures(&self, id: &str, rendered: &Value) -> Result<Value, &'static str> {
        self.directory(id)?;
        self.run(|tx| {
            let mut record = get(tx, id)?;
            if record.state != "prepared" || record.discarded { return Err("export_snapshot_commit_in_progress"); }
            if let Some(previous) = &record.figures {
                if canonical(previous) != canonical(rendered) { return Err("export_snapshot_figures_conflict"); }
                return Ok(metadata(&record));
            }
            let old_bytes=record_bytes(&record)?;
            assign_figures(&mut record, rendered)?;
            // Rendered labels can introduce CJK after the initial runtime
            // binding. Only a font pack absent from that identity may be filled.
            if needs_fonts(&record.format,&record.content,&record.options)&&managed_fonts_absent(&record.font_identity) {
                record.pending_fonts=true;record.runtime_ready=false;
            }
            check_record_budget(&record, self.byte_limit)?;
            let used:i64=tx.query_row("SELECT COALESCE(SUM(logical_bytes),0) FROM export_snapshots",[],|row|row.get(0)).map_err(|_|IO)?;
            let used=usize::try_from(used).map_err(|_|IO)?;
            if used.checked_sub(old_bytes).ok_or(IO)?.saturating_add(record_bytes(&record)?)>self.byte_limit {return Err(BUDGET);}
            put(tx, &record)?;
            Ok(metadata(&record))
        })
    }

    /// Must be durable before the caller renames its completed artifact. An
    /// intent can only be reconciled; a second target never replaces it.
    pub fn begin_commit(&self, id: &str, target: &Path, before_hash: Option<&str>, expected_hash: &str) -> Result<Value, &'static str> {
        self.begin_commit_bundle(id, target, before_hash, expected_hash, &[])
    }
    pub fn begin_commit_bundle(&self, id: &str, target: &Path, before_hash: Option<&str>, expected_hash: &str, dependencies: &[(PathBuf, String)]) -> Result<Value, &'static str> {
        self.directory(id)?;
        if !hash_valid(expected_hash) || before_hash.is_some_and(|hash| !hash_valid(hash)) || dependencies.len() > MAX_FILES { return Err(INVALID); }
        let target = absolute_target(target)?;
        let mut dependencies = dependencies.iter().map(|(path, hash)| {
            if !hash_valid(hash) { return Err(INVALID); }
            Ok((absolute_future_target(path)?, hash.clone()))
        }).collect::<Result<Vec<_>, _>>()?;
        dependencies.sort_by(|a,b| a.0.cmp(&b.0));
        if dependencies.windows(2).any(|pair| pair[0].0 == pair[1].0) { return Err(INVALID); }
        let output = Output { target, target_before: before_hash.map(str::to_string), expected_sha256: expected_hash.into(), dependencies };
        self.run(|tx| {
            let mut record = get(tx, id)?;
            if let Some(previous) = &record.output {
                if previous != &output { return Err("export_snapshot_commit_conflict"); }
                return Ok(metadata(&record));
            }
            if record.state != "prepared" || record.discarded { return Err("export_snapshot_unavailable"); }
            if !record.runtime_ready {return Err("export_snapshot_runtime_unavailable");}
            required_figures(&record)?;
            for path in std::iter::once(&output.target).chain(output.dependencies.iter().map(|(path,_)| path)) {
                let root = dunce::canonicalize(self.root()).map_err(|_| IO)?;
                if path_within(path, &root) || record.protected_paths.iter().any(|source| paths_equal(path, source)) {
                    return Err("export_snapshot_output_is_source");
                }
            }
            if output.dependencies.iter().any(|(path,_)| paths_equal(path, &output.target)) { return Err(INVALID); }
            record.output = Some(output);
            record.state = "commit_intent".into();
            put(tx, &record)?;
            Ok(metadata(&record))
        })
    }

    /// Reconciliation hashes only the already-recorded destination. It never
    /// renders, overwrites, retargets or assumes a missing output succeeded.
    pub fn finish(&self, review: &review::Service, id: &str) -> Result<Value, &'static str> {
        self.directory(id)?;
        self.run(|tx| {
            let mut record = get(tx, id)?;
            if record.receipt.is_some() {
                let mut value=metadata(&record);value["artifact_current"]=json!(artifact_current(record.output.as_ref()));return Ok(value);
            }
            let output = record.output.as_ref().ok_or("export_snapshot_no_commit_intent")?;
            if !artifact_current(Some(output)) {
                record.state = "needs_recovery".into();
                record.error = Some("export_snapshot_output_unverified".into());
                put(tx, &record)?;
                let mut value=metadata(&record);value["artifact_current"]=json!(false);return Ok(value);
            }
            let acceptance = match review.accept_export_snapshot(&record.review, &format!("export-snapshot:{}", record.snapshot_id)) {
                Ok(receipt) => receipt,
                Err(error) => {
                    record.state = "needs_recovery".into(); record.error = Some(error.into()); put(tx, &record)?;
                    let mut value=metadata(&record);value["artifact_current"]=json!(true);return Ok(value);
                }
            };
            record.receipt = Some(json!({"durable":true,"artifact_sha256":output.expected_sha256,
                "accepted_ids":acceptance["accepted_ids"],"accepted_count":acceptance["accepted_count"],
                "conflict_ids":acceptance["conflict_ids"],"conflict_count":acceptance["conflict_count"],
                "generation":acceptance["generation"],"review_revision":acceptance["review_revision"]}));
            record.state = if acceptance["conflict_count"].as_u64().unwrap_or(0) == 0 { "finished" } else { "acceptance_conflict" }.into();
            record.error = None;
            put(tx, &record)?;
            let mut value=metadata(&record);value["artifact_current"]=json!(true);Ok(value)
        })
    }
    pub fn reconcile(&self, review: &review::Service, id: &str) -> Result<Value, &'static str> { self.finish(review, id) }
    pub fn cancel(&self, id: &str) -> Result<Value, &'static str> { self.stop(id, "cancelled", None) }
    pub fn fail(&self, id: &str, error: &str) -> Result<Value, &'static str> {
        if error.is_empty() || error.len() > 128 || !error.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') { return Err(INVALID); }
        self.stop(id, "failed", Some(error))
    }
    fn stop(&self, id: &str, state: &str, error: Option<&str>) -> Result<Value, &'static str> {
        self.directory(id)?;
        self.run(|tx| {
            let mut record = get(tx, id)?;
            if record.output.is_some() { return Err("export_snapshot_commit_in_progress"); }
            if record.state == state { return Ok(metadata(&record)); }
            if record.state != "prepared" { return Err("export_snapshot_unavailable"); }
            record.state = state.into(); record.error = error.map(str::to_string); put(tx, &record)?;
            Ok(metadata(&record))
        })
    }

    /// Explicitly release owned inputs. Receipts and call fingerprints survive;
    /// output files and sidecars are never removed. An unknown commit remains
    /// retained until verified, even under quota pressure.
    pub fn discard(&self, id: &str) -> Result<Value, &'static str> {
        let directory = self.directory(id)?;
        self.run(|tx| {
            let mut record = get(tx, id)?;
            if record.discarded { return Ok(metadata(&record)); }
            if record.output.is_some() && record.receipt.is_none() { return Err("export_snapshot_commit_in_progress"); }
            if record.state == "prepared" { record.state = "cancelled".into(); }
            if !matches!(record.state.as_str(), "finished" | "acceptance_conflict" | "cancelled" | "failed") { return Err("export_snapshot_unavailable"); }
            remove_inputs(&directory, &record.resources)?;
            let pending = record.review["pending_changes"].as_array().into_iter().flatten().map(|change| json!({"id":change["id"]})).collect::<Vec<_>>();
            record.review = json!({"document_id":record.review["document_id"],"generation":record.review["generation"],
                "review_revision":record.review["review_revision"],"revision":record.review["revision"],"pending_changes":pending});
            record.content.clear(); record.options = json!({}); record.resolved_options = Value::Null;
            record.resources.clear(); record.figures = None; record.protected_paths.clear();
            record.engine_identity = Value::Null; record.font_identity = Value::Null; record.discarded = true;
            put(tx, &record)?;
            Ok(metadata(&record))
        })
    }

    /// Keep recent completed inputs for previews; compact old inputs after
    /// seven days, or oldest completed inputs when retained bytes exceed 75%.
    /// Neither path ever retires an unfinished output or its sidecars.
    pub fn cleanup_finished(&self) -> Result<usize, &'static str> {
        let candidates = self.run(|tx| {
            let mut statement = tx.prepare("SELECT payload,logical_bytes FROM export_snapshots ORDER BY rowid").map_err(|_| IO)?;
            let rows = statement.query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,i64>(1)?))).map_err(|_| IO)?;
            let mut total = 0usize; let mut candidates = Vec::new();
            for row in rows {
                let (payload, bytes) = row.map_err(|_| IO)?; let bytes=usize::try_from(bytes).map_err(|_|IO)?; total = total.saturating_add(bytes);
                let record = decode(&payload)?;
                if !record.discarded && (record.receipt.is_some() || matches!(record.state.as_str(),"cancelled"|"failed")) {
                    candidates.push((record.snapshot_id,record.created_at,bytes));
                }
            }
            let pressure = total > self.byte_limit.saturating_mul(3)/4;
            let mut selected = Vec::new();
            for (id, created, bytes) in candidates {
                if now().saturating_sub(created) > 7*24*60*60 || (pressure && total > self.byte_limit/2) {
                    total = total.saturating_sub(bytes); selected.push(id);
                }
            }
            Ok(selected)
        })?;
        for id in &candidates { self.discard(id)?; }
        Ok(candidates.len())
    }
    #[cfg(test)]
    fn with_limit(data_dir: &Path, byte_limit: usize) -> Self { Self { data_dir: data_dir.into(), byte_limit } }
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, &'static str> { value.get(key).and_then(Value::as_str).ok_or(INVALID) }
fn digest(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn now() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() }
fn runtime_ready_default()->bool {true}
fn hash_valid(hash: &str) -> bool { hash.len() == 64 && hash.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)) }
fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(object) => { let mut keys = object.keys().collect::<Vec<_>>(); keys.sort(); let mut result = serde_json::Map::new();
            for key in keys { result.insert(key.clone(), canonical(&object[key])); } Value::Object(result) },
        Value::Array(values) => Value::Array(values.iter().map(canonical).collect()), other => other.clone(),
    }
}
fn get(tx: &Transaction<'_>, id: &str) -> Result<Record, &'static str> {
    let payload: String = tx.query_row("SELECT payload FROM export_snapshots WHERE id=?1", [id], |row| row.get(0)).optional().map_err(|_| IO)?
        .ok_or("export_snapshot_not_found")?;
    decode(&payload)
}
fn decode(payload: &str) -> Result<Record, &'static str> { serde_json::from_str(payload).map_err(|_| IO) }
fn record_bytes(record: &Record) -> Result<usize, &'static str> {
    record.resources.iter().try_fold(serde_json::to_vec(record).map_err(|_| IO)?.len(), |total, resource| total.checked_add(resource.bytes).ok_or(BUDGET))
}
fn check_record_budget(record: &Record, limit: usize) -> Result<(), &'static str> {
    if record.resources.len() > MAX_FILES || record_bytes(record)? > limit { Err(BUDGET) } else { Ok(()) }
}
fn put(tx: &Transaction<'_>, record: &Record) -> Result<(), &'static str> {
    let payload = serde_json::to_string(record).map_err(|_| IO)?;
    tx.execute("INSERT INTO export_snapshots(id,call_id,fingerprint,payload,logical_bytes) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload,logical_bytes=excluded.logical_bytes",
        params![record.snapshot_id,record.call_id,record.fingerprint,payload,i64::try_from(record_bytes(record)?).map_err(|_|BUDGET)?]).map_err(|_| IO)?;
    Ok(())
}
fn metadata(record: &Record) -> Value {
    let ids = record.review["pending_changes"].as_array().into_iter().flatten().map(|change| change["id"].clone()).collect::<Vec<_>>();
    let mut result = json!({"ok":true,"snapshot_id":record.snapshot_id,"document_id":record.review["document_id"],
        "generation":record.review["generation"],"review_revision":record.review["review_revision"],"format":record.format,"suggested_name":record.name,
        "state":record.state,"pending_count":ids.len(),"pending_ids":ids,"resource_count":record.resources.len(),"figures_complete":required_figures(record).is_ok(),"discarded":record.discarded,
        "runtime_ready":record.runtime_ready,"pending_engine":record.pending_engine,"pending_fonts":record.pending_fonts,"runtime_engine":record.engine_identity["engine"]});
    if let Some(output) = &record.output {
        result["output"] = json!({"target":output.target,"target_before":output.target_before,"expected_sha256":output.expected_sha256,
            "dependencies":output.dependencies.iter().map(|(path,hash)|json!({"path":path,"sha256":hash})).collect::<Vec<_>>()});
    }
    if let Some(receipt) = &record.receipt { result["receipt"] = receipt.clone(); }
    if let Some(error) = &record.error { result["error_code"] = json!(error); }
    result
}
fn artifact_current(output:Option<&Output>)->bool {
    let Some(output)=output else{return false;};
    let matches=|path:&Path,expected:&str|read_absolute(path,MAX_SNAPSHOT_BYTES).is_ok_and(|bytes|digest(&bytes)==expected);
    matches(&output.target,&output.expected_sha256)&&output.dependencies.iter().all(|(path,hash)|matches(path,hash))
}

fn assign_figures(record: &mut Record, rendered: &Value) -> Result<(), &'static str> {
    let all = rendered.as_array().ok_or(INVALID)?;
    if all.len() > 128 || rendered.to_string().len() > 16 * 1024 * 1024 { return Err(BUDGET); }
    let mut profile = record.options.get("figures").cloned().unwrap_or_else(|| json!({}));
    profile.as_object_mut().ok_or(INVALID)?.remove("rendered");
    let fences = crate::scientific_figures::fences(&record.content, &profile);
    let mut seen = BTreeSet::new();
    for figure in all {
        let key = text(figure, "key")?;
        if !seen.insert(key) || !fences.iter().any(|fence| fence["key"] == key) { return Err("export_snapshot_figures_conflict"); }
        let svg = text(figure, "svg")?;
        crate::svg_render::validate(svg.as_bytes()).map_err(|_| "svg_unsafe")?;
    }
    record.figures = Some(rendered.clone());
    if !record.options["figures"].is_object() { record.options["figures"] = json!({}); }
    record.options["figures"]["rendered"] = rendered.clone();
    if record.resolved_options.is_object() { record.resolved_options["canonical"] = record.options.clone(); }
    required_figures(record)
}
fn required_figures(record: &Record) -> Result<(), &'static str> {
    let mut profile = record.options.get("figures").cloned().unwrap_or_else(|| json!({}));
    if let Some(object) = profile.as_object_mut() { object.remove("rendered"); }
    let fences = crate::scientific_figures::fences(&record.content, &profile);
    for fence in fences {
        let engine = fence["engine"].as_str().unwrap_or("");
        let client = !matches!(engine, "wsd" | "d2" | "ditaa");
        if client && !record.figures.as_ref().and_then(Value::as_array).is_some_and(|all| all.iter().any(|figure| figure["key"] == fence["key"])) {
            return Err("diagram_client_renderer_required");
        }
    }
    Ok(())
}

fn resource_names(content: &str, options: &Value, format: &str) -> Result<BTreeSet<String>, &'static str> {
    let mut names = BTreeSet::new();
    let mut add = |name: &str| -> Result<(), &'static str> {
        let name = name.trim();
        if name.is_empty() || name.starts_with('#') { return Ok(()); }
        if name.starts_with("data:") { validate_inline_resource(name)?; return Ok(()); }
        resource_path(name)?;
        names.insert(name.to_string());
        if names.len() > MAX_FILES { return Err(BUDGET); }
        Ok(())
    };
    for (_,reference) in crate::document_assets::image_references(content) { add(&reference)?; }
    for event in Parser::new_ext(content, Options::all()) {
        match event {
            Event::Start(Tag::Image { dest_url, .. }) => add(&dest_url)?,
            Event::Html(html) | Event::InlineHtml(html) => {
                for (_,reference) in html_references(&html)? { add(&reference)?; }
            }
            _ => (),
        }
    }
    let document = crate::md_ast::parse(content);
    let embedded = crate::publication_docx_citations::Resources::from_markdown(content).map_err(|_| "academic_resource")?;
    if embedded.is_none() {
        for name in crate::publication_metadata::bibliography_sources(content).map_err(|_| "academic_resource")? { add(&name)?; }
        if let Some(style) = options.pointer("/citations/style").and_then(Value::as_str).map(str::to_string).or_else(|| document.meta("csl")) {
            if style.to_ascii_lowercase().ends_with(".csl") { add(&style)?; }
        }
    }
    if format == "epub" {
        if let Some(cover) = options.pointer("/epub/cover").and_then(Value::as_str).filter(|value| !value.is_empty()).map(str::to_string).or_else(|| document.meta("cover")) { add(&cover)?; }
    }
    // Diagram engines may otherwise read host files or fetch remote data. A
    // snapshot never grants such dynamic dependencies implicitly.
    let profile = options.get("figures").cloned().unwrap_or_else(|| json!({}));
    for fence in crate::scientific_figures::fences(content, &profile) {
        let code = fence["code"].as_str().unwrap_or("").to_ascii_lowercase();
        if ["http://", "https://", "file://", "!include", "\\input", "\\include", "image=", "\"url\"", "'url'"].iter().any(|key| code.contains(key)) {
            return Err("export_snapshot_dynamic_resource");
        }
        if matches!(fence["engine"].as_str(),Some("vega"|"vega-lite")) {
            let spec:Value=serde_json::from_str(fence["code"].as_str().ok_or(INVALID)?).map_err(|_|"diagram_invalid_input")?;
            if dynamic_json_resource(&spec) { return Err("export_snapshot_dynamic_resource"); }
        }
    }
    Ok(names)
}

fn dynamic_json_resource(value:&Value)->bool {
    match value {Value::Object(object)=>object.iter().any(|(key,value)|key.eq_ignore_ascii_case("url")||dynamic_json_resource(value)),
        Value::Array(values)=>values.iter().any(dynamic_json_resource),_=>false}
}
fn validate_inline_resource(reference:&str)->Result<(),&'static str> {
    use base64::Engine;
    let (header,payload)=reference.split_once(',').ok_or("export_snapshot_dynamic_resource")?;
    if payload.len()>MAX_RESOURCE_BYTES {return Err(BUDGET);}
    if !["data:image/png;base64","data:image/jpeg;base64","data:image/gif;base64","data:image/webp;base64","data:image/svg+xml;base64",
        "data:image/bmp;base64","data:image/tiff;base64","data:font/woff;base64","data:font/woff2;base64","data:font/ttf;base64","data:font/otf;base64"].contains(&header) {
        return Err("export_snapshot_dynamic_resource");
    }
    let bytes=base64::engine::general_purpose::STANDARD.decode(payload).map_err(|_|"export_snapshot_dynamic_resource")?;
    if header=="data:image/svg+xml;base64" {crate::svg_render::validate(&bytes).map_err(|_|"svg_unsafe")?;}
    Ok(())
}

fn html_references(html:&str)->Result<Vec<(std::ops::Range<usize>,String)>,&'static str> {
    static TAGS:OnceLock<regex::Regex>=OnceLock::new();
    static ATTRIBUTES:OnceLock<regex::Regex>=OnceLock::new();
    static URLS:OnceLock<regex::Regex>=OnceLock::new();
    static STYLES:OnceLock<regex::Regex>=OnceLock::new();
    static DYNAMIC_IMAGES:OnceLock<regex::Regex>=OnceLock::new();
    let tags=TAGS.get_or_init(||regex::Regex::new(r#"(?is)<([a-z][a-z0-9:-]*)\b([^<>]*)>"#).expect("fixed HTML tags"));
    let attributes=ATTRIBUTES.get_or_init(||regex::Regex::new(r#"(?is)\b(src|poster|background|(?:xlink:)?href)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#).expect("fixed HTML resources"));
    let urls=URLS.get_or_init(||regex::Regex::new(r#"(?is)url\s*\(\s*["']?([^\s"')]+)"#).expect("fixed CSS resources"));
    let lower=html.to_ascii_lowercase();
    if lower.contains("@import") || lower.contains("srcset") { return Err("export_snapshot_dynamic_resource"); }
    if STYLES.get_or_init(||regex::Regex::new(r"(?is)<style\b|\bstyle\s*=").expect("fixed CSS scope")).is_match(html) {
        if html.contains('\\')||html.contains('&')
            ||DYNAMIC_IMAGES.get_or_init(||regex::Regex::new(r"(?i)\b(?:image-set|image|cross-fade|paint|element)\s*\(").expect("fixed unsupported CSS images")).is_match(html) {
            return Err("export_snapshot_dynamic_resource");
        }
    }
    let mut result=Vec::new();
    for tag in tags.captures_iter(html) {
        let kind=tag[1].to_ascii_lowercase();
        if matches!(kind.as_str(),"script"|"iframe"|"object"|"embed"|"link"|"base"|"video"|"audio"|"source"|"meta"|"foreignobject"|"animate"|"animatetransform"|"set") {
            return Err("export_snapshot_dynamic_resource");
        }
        let body=tag.get(2).ok_or(INVALID)?;
        for attribute in attributes.captures_iter(body.as_str()) {
            let key=attribute[1].to_ascii_lowercase();
            let value=attribute.get(2).or_else(||attribute.get(3)).or_else(||attribute.get(4)).ok_or(INVALID)?;
            if key.ends_with("href") && kind=="a" {continue;}
            if key.ends_with("href") && kind=="use" {
                if kind=="use" && !value.as_str().starts_with('#') { return Err("export_snapshot_dynamic_resource"); }
                continue;
            }
            if value.as_str().contains('&') { return Err("export_snapshot_dynamic_resource"); }
            result.push((body.start()+value.start()..body.start()+value.end(),value.as_str().into()));
        }
    }
    for url in urls.captures_iter(html) {
        let value=url.get(1).ok_or(INVALID)?;
        result.push((value.range(),value.as_str().into()));
    }
    Ok(result)
}

fn resource_uri(root:&Path,reference:&str)->Result<String,&'static str> {
    use base64::Engine;
    let relative=resource_path(reference)?;
    let mime=match relative.extension().and_then(|value|value.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "png"=>"image/png","jpg"|"jpeg"=>"image/jpeg","gif"=>"image/gif","webp"=>"image/webp","svg"=>"image/svg+xml",
        "bmp"=>"image/bmp","tif"|"tiff"=>"image/tiff","woff"=>"font/woff","woff2"=>"font/woff2","ttf"=>"font/ttf","otf"=>"font/otf",
        _=>return Err("export_snapshot_dynamic_resource"),
    };
    let bytes=read_absolute(&root.join("source").join(relative),MAX_RESOURCE_BYTES)?;
    Ok(format!("data:{mime};base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes)))
}

fn freeze_html_projection(record:&mut Record,root:&Path)->Result<(),&'static str> {
    let mut assets=Vec::new();let mut seen=BTreeSet::new();
    for (_,reference) in crate::document_assets::image_references(&record.content) {
        if reference.starts_with("data:image/") || !seen.insert(reference.clone()) { continue; }
        let relative=resource_path(&reference)?;let path=root.join("source").join(relative);
        assets.push(crate::document_assets::AssetHandle { path:path.to_string_lossy().into_owned(),
            name:path.file_name().and_then(|value|value.to_str()).ok_or(INVALID)?.into(),reference });
    }
    if !assets.is_empty() {
        // Existing materialization preserves reference image labels/titles and
        // owned author delimiters while producing literal inline destinations.
        let mut materialized=crate::document_assets::materialize(&record.content,&root.join("source/document.md"),&assets)
            .map_err(|_|"export_snapshot_resource_path")?;
        record.content=materialized.text.clone();
        for asset in &materialized.assets {
            let path=Path::new(&asset.path);let relative=path.strip_prefix(root).map_err(|_|IO)?;
            let name=path_string(relative)?;let bytes=read_absolute(path,MAX_RESOURCE_BYTES)?;
            if !record.resources.iter().any(|resource|resource.path==name) {
                record.resources.push(Resource {path:name,bytes:bytes.len(),sha256:digest(&bytes)});
            }
        }
        materialized.commit();
    }
    let mut edits=Vec::new();
    for (range,reference) in crate::document_assets::image_references(&record.content) {
        if reference.starts_with("data:image/") { continue; }
        let raw=&record.content[range.clone()];
        let at=raw.rfind("](").ok_or("export_snapshot_dynamic_resource")?+2;
        let after=&raw[at..];let leading=after.len()-after.trim_start().len();
        let start=at+leading+usize::from(after.trim_start().starts_with('<'));
        if !raw[start..].starts_with(&reference) { return Err("export_snapshot_dynamic_resource"); }
        edits.push((range.start+start..range.start+start+reference.len(),resource_uri(root,&reference)?));
    }
    for (event,range) in Parser::new_ext(&record.content,Options::all()).into_offset_iter() {
        let (Event::Html(html)|Event::InlineHtml(html))=event else {continue;};
        for (local,reference) in html_references(&html)? {
            if reference.starts_with("data:") || reference.starts_with('#') || reference.is_empty() {continue;}
            edits.push((range.start+local.start..range.start+local.end,resource_uri(root,&reference)?));
        }
    }
    edits.sort_by_key(|(range,_)|(range.start,range.end));
    if edits.windows(2).any(|pair|pair[0].0.end>pair[1].0.start) {return Err("export_snapshot_dynamic_resource");}
    for (range,value) in edits.into_iter().rev() {record.content.replace_range(range,&value);}
    Ok(())
}

fn resource_path(name: &str) -> Result<PathBuf, &'static str> {
    if name.is_empty() || name.len() > 4096 || name.chars().any(|c| c.is_control() || "\\:?*\"<>|".contains(c)) || name.starts_with('/') {
        return Err("export_snapshot_resource_path");
    }
    let decoded = percent_encoding::percent_decode_str(name).decode_utf8().map_err(|_| "export_snapshot_resource_path")?;
    storage_path(&decoded)
}
fn storage_path(name: &str) -> Result<PathBuf, &'static str> {
    if name.is_empty() || name.len() > 4096 || name.chars().any(|c| c.is_control() || "\\:?*\"<>|".contains(c)) { return Err("export_snapshot_resource_path"); }
    let mut result = PathBuf::new();
    for component in Path::new(name).components() {
        match component { Component::Normal(part) => result.push(part), Component::CurDir => (), _ => return Err("export_snapshot_resource_path") }
    }
    if result.as_os_str().is_empty() { return Err("export_snapshot_resource_path"); }
    Ok(result)
}
fn path_string(path: &Path) -> Result<String, &'static str> { Ok(path.to_str().ok_or(INVALID)?.replace('\\', "/")) }
fn font_resource(path:&str)->bool {Path::new(path).extension().and_then(|value|value.to_str()).is_some_and(|ext|["ttf","otf","woff","woff2"].iter().any(|value|ext.eq_ignore_ascii_case(value)))}
fn validate_resource(path: &Path, bytes: &[u8]) -> Result<(), &'static str> {
    if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("svg")) { crate::svg_render::validate(bytes).map_err(|_| "svg_unsafe")?; }
    if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("css")) {
        return Err("export_snapshot_dynamic_resource");
    }
    Ok(())
}
fn copy_resource(root: &Path, name: &str, bytes: &[u8], resources: &mut Vec<Resource>) -> Result<(), &'static str> {
    let relative = storage_path(name)?;
    let name = path_string(&relative)?;
    if let Some(previous) = resources.iter().find(|resource| resource.path == name) {
        if previous.sha256 != digest(bytes) { return Err("export_snapshot_resource_changed"); }
        return Ok(());
    }
    if resources.len() >= MAX_FILES || bytes.len() > MAX_RESOURCE_BYTES { return Err(BUDGET); }
    let path = root.join(relative);
    fs::create_dir_all(path.parent().ok_or(IO)?).map_err(|_| IO)?;
    check_directory(path.parent().ok_or(IO)?)?;
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|_| IO)?;
    file.write_all(bytes).map_err(|_| IO)?;
    file.sync_all().map_err(|_| IO)?;
    sync_directory(path.parent().ok_or(IO)?)?;
    resources.push(Resource { path: name, bytes: bytes.len(), sha256: digest(bytes) });
    Ok(())
}
fn ensure_owned_resource(root:&Path,name:&str,bytes:&[u8])->Result<(),&'static str> {
    check_directory(root)?;check_directory(&root.join("assets"))?;
    let path=root.join(storage_path(name)?);
    match fs::symlink_metadata(&path) {
        Ok(_)=>{
            let existing=read_absolute(&path,MAX_RESOURCE_BYTES)?;
            if digest(&existing)!=digest(bytes) {return Err("export_snapshot_resource_changed");}
            Ok(())
        }
        Err(error) if error.kind()==std::io::ErrorKind::NotFound=>copy_resource(root,name,bytes,&mut Vec::new()),
        Err(_)=>Err(IO),
    }
}
fn plain(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() { return false; }
    #[cfg(windows)] { use std::os::windows::fs::MetadataExt; if metadata.file_attributes() & 0x400 != 0 { return false; } }
    true
}
fn check_directory(path: &Path) -> Result<(), &'static str> {
    let opened = open_absolute(path)?;
    if !opened.file.metadata().map_err(|_| IO)?.is_dir() { return Err("export_snapshot_resource_path"); }
    Ok(())
}
/// Application discovery constructs ancestor candidates with `..`. Validate
/// every directory before removing that step, so normalization cannot hide a
/// symlink/reparse traversal. Document-nominated resource paths stay strict.
fn application_assets_directory(path:&Path)->Result<PathBuf,&'static str> {
    let absolute=if path.is_absolute() {path.to_path_buf()}else{std::env::current_dir().map_err(|_|IO)?.join(path)};
    let mut directory=PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::Prefix(_)|Component::RootDir=>directory.push(component.as_os_str()),
            Component::Normal(part)=>{directory.push(part);check_directory(&directory)?;}
            Component::CurDir=>(),
            Component::ParentDir=>{
                if !matches!(directory.components().next_back(),Some(Component::Normal(_)))||!directory.pop() {return Err("export_snapshot_resource_path");}
            }
        }
    }
    check_directory(&directory)?;
    dunce::canonicalize(directory).map_err(|_|"export_snapshot_resource_path")
}
struct Opened { file: File, _ancestors: Vec<File> }
fn read_absolute(path: &Path, maximum: usize) -> Result<Vec<u8>, &'static str> {
    let opened = open_absolute(path)?;
    let before = opened.file.metadata().map_err(|_| "export_snapshot_resource_missing")?;
    if !plain(&before) || !before.is_file() { return Err("export_snapshot_resource_path"); }
    if before.len() > maximum as u64 { return Err(BUDGET); }
    let mut bytes = Vec::new();
    (&opened.file).take(maximum as u64 + 1).read_to_end(&mut bytes).map_err(|_| "export_snapshot_resource_missing")?;
    let after = opened.file.metadata().map_err(|_| "export_snapshot_resource_changed")?;
    if bytes.len() > maximum { return Err(BUDGET); }
    if before.len() != after.len() || bytes.len() as u64 != before.len() || before.modified().ok() != after.modified().ok() { return Err("export_snapshot_resource_changed"); }
    Ok(bytes)
}

#[cfg(unix)]
fn open_absolute(path: &Path) -> Result<Opened, &'static str> {
    use std::{ffi::CString, os::{fd::{AsRawFd, FromRawFd}, unix::ffi::OsStrExt}};
    if !path.is_absolute() { return Err("export_snapshot_resource_path"); }
    let flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK;
    let root = unsafe { libc::open(c"/".as_ptr(), flags | libc::O_DIRECTORY) };
    if root < 0 { return Err("export_snapshot_resource_path"); }
    let mut current = unsafe { File::from_raw_fd(root) };
    let mut ancestors = Vec::new();
    for component in path.components() {
        match component {
            Component::RootDir => (),
            Component::Normal(part) => {
                if !current.metadata().map_err(|_| IO)?.is_dir() { return Err("export_snapshot_resource_path"); }
                let name = CString::new(part.as_bytes()).map_err(|_| "export_snapshot_resource_path")?;
                let next = unsafe { libc::openat(current.as_raw_fd(), name.as_ptr(), flags) };
                if next < 0 {
                    let error = std::io::Error::last_os_error();
                    return Err(if matches!(error.raw_os_error(),Some(libc::ELOOP | libc::ENOTDIR)) {
                        "export_snapshot_resource_path"
                    } else { "export_snapshot_resource_missing" });
                }
                ancestors.push(current); current = unsafe { File::from_raw_fd(next) };
            }
            _ => return Err("export_snapshot_resource_path"),
        }
    }
    Ok(Opened { file: current, _ancestors: ancestors })
}

#[cfg(windows)]
fn open_absolute(path: &Path) -> Result<Opened, &'static str> {
    use std::os::windows::fs::OpenOptionsExt;
    if !path.is_absolute() { return Err("export_snapshot_resource_path"); }
    let mut current = PathBuf::new(); let mut ancestors = Vec::new();
    let parts = path.components().collect::<Vec<_>>();
    for (index, part) in parts.iter().enumerate() {
        match part {
            Component::Prefix(_) | Component::RootDir => current.push(part.as_os_str()),
            Component::Normal(_) => {
                current.push(part.as_os_str());
                let endpoint = index + 1 == parts.len();
                let file = fs::OpenOptions::new().read(true).custom_flags(0x0220_0000).share_mode(if endpoint { 1 } else { 3 }).open(&current)
                    .map_err(|_| "export_snapshot_resource_missing")?;
                let metadata = file.metadata().map_err(|_| "export_snapshot_resource_missing")?;
                if !plain(&metadata) || (!endpoint && !metadata.is_dir()) { return Err("export_snapshot_resource_path"); }
                if endpoint { return Ok(Opened { file, _ancestors: ancestors }); }
                ancestors.push(file);
            }
            _ => return Err("export_snapshot_resource_path"),
        }
    }
    let file = fs::OpenOptions::new().read(true).custom_flags(0x0220_0000).share_mode(3).open(&current).map_err(|_| "export_snapshot_resource_path")?;
    Ok(Opened { file, _ancestors: ancestors })
}

#[cfg(not(any(windows, unix)))]
fn open_absolute(_path: &Path) -> Result<Opened, &'static str> { Err("export_snapshot_resource_path") }

fn absolute_target(path: &Path) -> Result<PathBuf, &'static str> {
    if !path.is_absolute() || path.components().any(|part| matches!(part, Component::ParentDir | Component::CurDir)) { return Err(INVALID); }
    let parent = path.parent().ok_or(INVALID)?;
    check_directory(parent)?;
    let parent = dunce::canonicalize(parent).map_err(|_| INVALID)?;
    Ok(parent.join(path.file_name().ok_or(INVALID)?))
}
fn absolute_future_target(path:&Path)->Result<PathBuf,&'static str> {
    if !path.is_absolute() || path.components().any(|part|matches!(part,Component::ParentDir|Component::CurDir))
        || path.to_string_lossy().split(['/', '\\']).any(|part|matches!(part,"."|"..")) {return Err(INVALID);}
    for component in path.components() {
        if let Component::Normal(part)=component {
            if part.to_str().is_none_or(|part|part.is_empty()||part.chars().any(|c|c.is_control()||":*?\"<>|".contains(c))) {return Err(INVALID);}
        }
    }
    let leaf=path.file_name().ok_or(INVALID)?;
    let mut ancestor=path.parent().ok_or(INVALID)?;
    let mut missing=Vec::new();
    loop {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata)=>{
                if !plain(&metadata)||!metadata.is_dir() {return Err("export_snapshot_resource_path");}
                check_directory(ancestor)?;
                let mut result=dunce::canonicalize(ancestor).map_err(|_|IO)?;
                for part in missing.into_iter().rev() {result.push(part);}
                return Ok(result.join(leaf));
            }
            Err(error) if error.kind()==std::io::ErrorKind::NotFound=>{
                missing.push(ancestor.file_name().ok_or(INVALID)?);
                ancestor=ancestor.parent().ok_or(INVALID)?;
            }
            Err(_)=>return Err(IO),
        }
    }
}
fn paths_equal(left: &Path, right: &Path) -> bool {
    #[cfg(windows)] { return left.to_string_lossy().eq_ignore_ascii_case(&right.to_string_lossy()); }
    #[cfg(not(windows))] { left == right }
}
fn path_within(path: &Path, root: &Path) -> bool {
    #[cfg(windows)] { return path.to_string_lossy().to_ascii_lowercase().starts_with(&(root.to_string_lossy().to_ascii_lowercase()+"\\")) || paths_equal(path,root); }
    #[cfg(not(windows))] { path.starts_with(root) }
}
fn sync_directory(path: &Path) -> Result<(), &'static str> {
    #[cfg(unix)] { File::open(path).and_then(|file|file.sync_all()).map_err(|_| IO)?; }
    #[cfg(not(unix))] { let _ = path; }
    Ok(())
}

fn remove_inputs(directory: &Path, resources: &[Resource]) -> Result<(), &'static str> {
    if !directory.exists() { return Ok(()); }
    check_directory(directory)?;
    let mut parents = BTreeSet::new();
    for resource in resources {
        let relative = storage_path(&resource.path)?;
        let path = directory.join(&relative);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind()==std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(IO), Ok(metadata) if !plain(&metadata) => return Err("export_snapshot_resource_changed"), _ => (),
        }
        let bytes = read_absolute(&path, MAX_RESOURCE_BYTES)?;
        if bytes.len()!=resource.bytes || digest(&bytes)!=resource.sha256 { return Err("export_snapshot_resource_changed"); }
        // Capture a leaf before deletion. A replacement with unknown bytes is
        // retained in the private quarantine, never recursively reclaimed.
        let quarantine = directory.join(format!("discarding-{}",Uuid::new_v4()));
        fs::create_dir(&quarantine).map_err(|_| IO)?;
        let captured = quarantine.join("resource");
        fs::rename(&path,&captured).map_err(|_| IO)?;
        let bytes = read_absolute(&captured,MAX_RESOURCE_BYTES)?;
        if bytes.len()!=resource.bytes || digest(&bytes)!=resource.sha256 { return Err("export_snapshot_resource_changed"); }
        fs::remove_file(captured).map_err(|_| IO)?;
        fs::remove_dir(quarantine).map_err(|_| IO)?;
        let mut parent = relative.parent();
        while let Some(path) = parent.filter(|path|!path.as_os_str().is_empty()) { parents.insert(directory.join(path)); parent=path.parent(); }
    }
    let mut parents=parents.into_iter().collect::<Vec<_>>(); parents.sort_by_key(|path|std::cmp::Reverse(path.components().count()));
    parents.extend([directory.join("source"),directory.join("assets")]);
    for path in parents { let _=fs::remove_dir(path); }
    let _=fs::remove_dir(directory);
    Ok(())
}

fn runtime_locks(format:&str,options:&Value)->Vec<String> {
    let engine=if format=="pdf" {options.pointer("/pdf/engine").and_then(Value::as_str).unwrap_or("rust")}else{"rust"};
    let mut names=vec![format!("fonts-{}-universal.lock.json",crate::publication_fonts::VERSION)];
    if engine=="typst"||format=="docx" {names.push(format!("typst-0.15.1-{}.lock.json",crate::capability_packs::platform()));}
    if matches!(engine,"xelatex"|"pdflatex") {names.push(format!("latex-{}-{}.lock.json",crate::latex_compile::VERSION,crate::capability_packs::platform()));}
    names
}
fn needs_fonts(format:&str,content:&str,options:&Value)->bool {
    (format=="docx"||(format=="pdf"&&options.pointer("/pdf/engine").and_then(Value::as_str).is_none_or(|engine|matches!(engine,"rust"|"typst"))))
        &&crate::publication_fonts::needs_cjk(&format!("{content}{options}"))
}
fn managed_fonts_absent(fonts:&Value)->bool {fonts["managed"].as_array().is_none_or(Vec::is_empty)}
fn missing_engine_error(options:&Value)->&'static str {
    if options.pointer("/pdf/engine").and_then(Value::as_str)==Some("typst") {"typst_engine_unavailable"}else{"latex_engine_unavailable"}
}
fn runtime_identity(data: &Path, assets: &Path, format: &str, options: &Value) -> Result<(Value, Value), &'static str> {
    if crate::speech::cancelled() {return Err("cancelled");}
    let selected = if format == "pdf" { options.pointer("/pdf/engine").and_then(Value::as_str).unwrap_or("rust") } else { "rust" };
    if !matches!(selected, "rust" | "typst" | "xelatex" | "pdflatex") { return Err(INVALID); }
    let mut engine = json!({"application":env!("CARGO_PKG_VERSION"),"format":format,"engine":selected,
        "platform":crate::capability_packs::platform(),"snapshot_schema":1});
    if format=="docx" {
        engine["projection"]=match crate::capability_packs::lease(data,assets,"typst") {
            Ok(lease)=>{
                let lock=crate::capability_packs::resource_lock(assets,"typst").map_err(|_|"export_snapshot_runtime_unavailable")?;
                let path=lease.directory.join(if cfg!(windows){"bin/typst.exe"}else{"bin/typst"});
                let bytes=read_absolute(&path,MAX_SNAPSHOT_BYTES)?;
                json!({"engine":"typst","path":path,"sha256":digest(&bytes),"manifest_sha256":digest(&serde_json::to_vec(&canonical(&lock)).map_err(|_|IO)?)})
            }
            Err(_)=>json!({"engine":"rust"}),
        };
    }
    let svg_root=application_assets_directory(&crate::paths::assets_dir())?.join("engines/resvg").join(crate::capability_packs::platform());
    let svg_program=svg_root.join(if cfg!(windows){"resvg.exe"}else{"resvg"});
    engine["svg_worker"]=if svg_program.exists() {
        let bytes=read_absolute(&svg_program,MAX_SNAPSHOT_BYTES)?;
        let lock=read_absolute(&svg_root.join("lock.json"),MAX_RESOURCE_BYTES)?;
        json!({"path":svg_program,"sha256":digest(&bytes),"lock_sha256":digest(&lock)})
    } else {Value::Null};
    if selected != "rust" {
        let pack = if selected == "typst" { "typst" } else { "latex" };
        if let Ok(lease)=crate::capability_packs::lease(data,assets,pack) {
            let lock = crate::capability_packs::resource_lock(assets, pack).map_err(|_| "export_snapshot_runtime_unavailable")?;
            engine["manifest_sha256"] = json!(digest(&serde_json::to_vec(&canonical(&lock)).map_err(|_| IO)?));
            // The validated manifest binds every package/resource; the executable
            // identity additionally hashes the actual opened file used to run it.
            let binary = if selected == "typst" { if cfg!(windows) { "bin/typst.exe" } else { "bin/typst" } }
                else if selected == "xelatex" { "bin/windows/xelatex.exe" } else { "bin/windows/pdflatex.exe" };
            let path = lease.directory.join(binary);
            let actual = read_absolute(&path, MAX_SNAPSHOT_BYTES)?;
            engine["binary"] = json!({"path":path,"sha256":digest(&actual)});
        } else {engine["pending"]=json!(true);}
    }
    let mut fonts = json!({"version":crate::publication_fonts::VERSION,"core":[
        {"name":"STIXTwoMath-Regular.ttf","sha256":digest(crate::publication_fonts::MATH)},
        {"name":"DejaVuSans.ttf","sha256":digest(crate::publication_fonts::CORE_TEXT_FALLBACK)},
        {"name":"Waree-Regular.otf","sha256":digest(crate::publication_fonts::CORE_THAI_FALLBACK)}],"managed":[]});
    match crate::capability_packs::leased_font_resources(data, assets) {
        Ok((resources, _lease)) => {
            let mut identities = Vec::new();
            for path in [&resources.sans, &resources.serif] {
                for path in [path.clone(), path.with_file_name(path.file_name().ok_or(INVALID)?.to_string_lossy().replace("-Regular", "-Bold"))] {
                    let bytes = read_absolute(&path, MAX_SNAPSHOT_BYTES)?;
                    identities.push(json!({"path":path,"sha256":digest(&bytes),"bytes":bytes.len()}));
                }
            }
            fonts["managed"] = json!(identities);
            fonts["manifest"] = json!(resources.fingerprint);
        }
        Err(_) => (),
    }
    if crate::speech::cancelled() {return Err("cancelled");}
    Ok((engine, fonts))
}

#[cfg(test)]
#[path = "export_snapshot_tests.rs"]
mod tests;
