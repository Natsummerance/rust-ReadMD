//! Private, bounded checkpoints for native AI tasks. Never part of document history.
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::{Path, PathBuf}, time::{Duration, SystemTime, UNIX_EPOCH}};

pub(crate) const SCHEMA_VERSION: i64 = 2;
pub(crate) const MAX_STORE_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const TRANSITION_RESERVE_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Budget { pub max_rounds: u32, pub max_elapsed_ms: u64 }
impl Default for Budget {
    fn default() -> Self { Self { max_rounds: 64, max_elapsed_ms: 30 * 60 * 1000 } }
}
impl Budget {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.max_rounds == 0 || self.max_rounds > 256 || self.max_elapsed_ms == 0 || self.max_elapsed_ms > 120 * 60 * 1000 {
            return Err("review_ai_budget");
        }
        Ok(())
    }
}

/// Opaque references to an existing saved connection. No endpoint, headers or secrets.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionRef {
    pub provider_id: String,
    #[serde(default)] pub credential_id: Option<String>,
    pub model: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FrozenTemplate {
    pub skill_id: String,
    pub fingerprint: String,
    pub rendered: String,
    pub language: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskRequest {
    pub start_call_id: String,
    pub document_id: String,
    pub instruction: String,
    pub connection: ConnectionRef,
    #[serde(default="default_stream")] pub stream: bool,
    #[serde(default)] pub caller_digest: String,
    #[serde(default)] pub allowed_ranges: Option<Vec<[usize; 2]>>,
    #[serde(default)] pub budget: Budget,
    #[serde(default)] pub template: Option<FrozenTemplate>,
}
fn default_stream()->bool {true}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Checkpoint {
    pub content: String,
    pub generation: u64,
    pub review_revision: u64,
    pub revision: String,
    pub last_event_id: u64,
    pub allowed_ranges: Option<Vec<[usize; 2]>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ToolIntent {
    pub key: String,
    pub native_id: Option<String>,
    pub name: String,
    pub arguments: Value,
    pub digest: String,
    pub review_call_id: String,
    pub review_request: Option<Value>,
    pub phase: String,
    #[serde(default)] pub reserved_bytes: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PendingBatch { pub calls: Vec<ToolIntent>, pub next: usize }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CallReceipt { pub name: String, pub digest: String, pub completed: bool }

/// Local checkpoint. Use `public_view`, never serialize this private record to an API.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub document_id: String,
    pub state: String,
    pub rounds: u32,
    pub elapsed_ms: u64,
    pub last_event_id: u64,
    pub error_code: Option<String>,
    pub(crate) request: TaskRequest,
    pub(crate) fingerprint: String,
    pub(crate) initial: Checkpoint,
    pub(crate) checkpoint: Checkpoint,
    pub(crate) owner: Option<String>,
    pub(crate) active_since_ms: Option<u64>,
    pub(crate) wire_bytes: u64,
    pub(crate) pending: Option<PendingBatch>,
    pub(crate) calls: BTreeMap<String, CallReceipt>,
    #[serde(default)] pub(crate) reserved_bytes: usize,
}
impl TaskRecord {
    pub fn instruction(&self) -> &str { &self.request.instruction }
    pub fn connection(&self) -> &ConnectionRef { &self.request.connection }
    pub fn template(&self) -> Option<&FrozenTemplate> { self.request.template.as_ref() }
    pub fn budget(&self) -> &Budget { &self.request.budget }
    /// Original frozen authority for a start-call replay. Later checkpoints
    /// never replace these bytes or tokens.
    pub fn initial_snapshot(&self)->Value {
        let mut snapshot=json!({"document_id":self.document_id,"content":self.initial.content,
            "generation":self.initial.generation,"review_revision":self.initial.review_revision,
            "revision":self.initial.revision,"last_event_id":self.initial.last_event_id});
        if let Some(ranges)=&self.initial.allowed_ranges{snapshot["allowed_ranges"]=json!(ranges);}
        snapshot
    }
    pub fn public_view(&self) -> Value {
        json!({"id":self.id,"document_id":self.document_id,"state":self.state,
            "rounds":self.rounds,"elapsed_ms":self.effective_elapsed(now_ms()),
            "last_event_id":self.last_event_id,"error_code":self.error_code,
            "budget":self.request.budget})
    }
    pub(crate) fn effective_elapsed(&self, now: u64) -> u64 {
        self.elapsed_ms.saturating_add(self.active_since_ms.map(|then| now.saturating_sub(then)).unwrap_or(0))
    }
    pub(crate) fn charge_time(&mut self) {
        let now = now_ms();
        self.elapsed_ms = self.effective_elapsed(now);
        self.active_since_ms = if self.state == "running" { Some(now) } else { None };
    }
    pub(crate) fn charge_recovery_time(&mut self) {
        // A crash has no reliable stop timestamp. Charge at most the bounded
        // last request window, rather than treating days offline as AI work.
        let unknown=self.active_since_ms.map(|then|now_ms().saturating_sub(then).min(180_000)).unwrap_or(0);
        self.elapsed_ms=self.elapsed_ms.saturating_add(unknown);
        self.active_since_ms=None;
    }
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis().min(u64::MAX as u128) as u64
}

pub(crate) struct Store { pub(crate) path: PathBuf, pub(crate) byte_limit: usize }
impl Store {
    pub(crate) fn new(data_dir: &Path) -> Self { Self { path: data_dir.join("ai-tasks.sqlite3"), byte_limit: MAX_STORE_BYTES-1024*1024 } }
    fn connect(&self) -> Result<Connection, &'static str> {
        if let Some(parent) = self.path.parent() { std::fs::create_dir_all(parent).map_err(|_| "review_persistence_failed")?; }
        let mut connection = Connection::open(&self.path).map_err(|_| "review_persistence_failed")?;
        connection.busy_timeout(Duration::from_secs(3)).map_err(|_| "review_persistence_failed")?;
        let migration = connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|_| "review_persistence_failed")?;
        let version: i64 = migration.pragma_query_value(None, "user_version", |row| row.get(0)).map_err(|_| "review_persistence_failed")?;
        if version > SCHEMA_VERSION { return Err("review_unsupported_schema"); }
        if version == 0 {
            migration.execute_batch("CREATE TABLE IF NOT EXISTS ai_tasks (
                id TEXT PRIMARY KEY NOT NULL, start_call_id TEXT UNIQUE NOT NULL,
                fingerprint TEXT NOT NULL, payload TEXT NOT NULL);
                CREATE TABLE IF NOT EXISTS ai_task_events (
                task_id TEXT NOT NULL, event_id INTEGER NOT NULL, payload TEXT NOT NULL,
                PRIMARY KEY(task_id,event_id));").map_err(|_| "review_persistence_failed")?;
        }
        if version<2 {
            migration.execute_batch("ALTER TABLE ai_tasks ADD COLUMN state TEXT NOT NULL DEFAULT 'queued';
                ALTER TABLE ai_tasks ADD COLUMN owner TEXT;
                UPDATE ai_tasks SET state=COALESCE(json_extract(payload,'$.state'),'queued'),owner=json_extract(payload,'$.owner');").map_err(|_|"review_persistence_failed")?;
        }
        migration.prepare("SELECT id,start_call_id,fingerprint,payload,state,owner FROM ai_tasks LIMIT 0").map_err(|_| "review_persistence_failed")?;
        migration.prepare("SELECT task_id,event_id,payload FROM ai_task_events LIMIT 0").map_err(|_| "review_persistence_failed")?;
        if version < SCHEMA_VERSION { migration.pragma_update(None, "user_version", SCHEMA_VERSION).map_err(|_| "review_persistence_failed")?; }
        migration.commit().map_err(|_| "review_persistence_failed")?;
        connection.pragma_update(None, "journal_mode", "WAL").map_err(|_| "review_persistence_failed")?;
        connection.pragma_update(None, "synchronous", "FULL").map_err(|_| "review_persistence_failed")?;
        connection.pragma_update(None, "wal_autocheckpoint", 64).map_err(|_| "review_persistence_failed")?;
        connection.pragma_update(None, "journal_size_limit", 1024 * 1024).map_err(|_| "review_persistence_failed")?;
        let size: i64 = connection.pragma_query_value(None, "page_size", |row| row.get(0)).map_err(|_| "review_persistence_failed")?;
        connection.pragma_update(None, "max_page_count", (MAX_STORE_BYTES as i64 / size.max(1)).max(128)).map_err(|_| "review_persistence_failed")?;
        Ok(connection)
    }
    pub(crate) fn run<T>(&self, f: impl FnOnce(&Transaction<'_>) -> Result<T, &'static str>) -> Result<T, &'static str> {
        let mut connection = self.connect()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|_| "review_persistence_failed")?;
        let result = f(&tx)?;
        if used_bytes(&tx)? > self.byte_limit { return Err("review_ai_budget"); }
        tx.commit().map_err(|_| "review_persistence_failed")?;
        Ok(result)
    }
}
pub(crate) fn used_bytes(tx:&Transaction<'_>)->Result<usize,&'static str> {
    // Reserved completion capacity is admitted before a separate review-store
    // transaction can commit. Terminal transitions release their own reserve.
    let used:i64=tx.query_row("SELECT
        COALESCE((SELECT SUM(length(CAST(payload AS BLOB))+length(id)+length(start_call_id)+length(fingerprint)+length(state)+COALESCE(length(owner),0)+COALESCE(json_extract(payload,'$.reserved_bytes'),0)) FROM ai_tasks),0)
        +COALESCE((SELECT SUM(COALESCE(json_extract(tool.value,'$.reserved_bytes'),0)) FROM ai_tasks,json_each(ai_tasks.payload,'$.pending.calls') AS tool),0)
        +COALESCE((SELECT SUM(length(CAST(payload AS BLOB))+length(task_id)+16) FROM ai_task_events),0)",[],|row|row.get(0)).map_err(|_|"review_persistence_failed")?;
    usize::try_from(used).map_err(|_|"review_persistence_failed")
}
pub(crate) fn load(tx: &Transaction<'_>, id: &str) -> Result<TaskRecord, &'static str> {
    let payload: Option<String> = tx.query_row("SELECT payload FROM ai_tasks WHERE id=?1", [id], |row| row.get(0)).optional().map_err(|_| "review_persistence_failed")?;
    serde_json::from_str(&payload.ok_or("review_not_found")?).map_err(|_| "review_persistence_failed")
}
pub(crate) fn find_start(tx: &Transaction<'_>, call: &str) -> Result<Option<TaskRecord>, &'static str> {
    let payload: Option<String> = tx.query_row("SELECT payload FROM ai_tasks WHERE start_call_id=?1", [call], |row| row.get(0)).optional().map_err(|_| "review_persistence_failed")?;
    payload.map(|payload| serde_json::from_str(&payload).map_err(|_| "review_persistence_failed")).transpose()
}
pub(crate) fn all(tx: &Transaction<'_>) -> Result<Vec<TaskRecord>, &'static str> {
    let mut statement = tx.prepare("SELECT payload FROM ai_tasks ORDER BY rowid DESC LIMIT 512").map_err(|_| "review_persistence_failed")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0)).map_err(|_| "review_persistence_failed")?;
    rows.map(|row| serde_json::from_str(&row.map_err(|_| "review_persistence_failed")?).map_err(|_| "review_persistence_failed")).collect()
}
pub(crate) fn put(tx: &Transaction<'_>, task: &TaskRecord) -> Result<(), &'static str> {
    let payload = serde_json::to_string(task).map_err(|_| "review_persistence_failed")?;
    tx.execute("INSERT INTO ai_tasks(id,start_call_id,fingerprint,payload,state,owner) VALUES(?1,?2,?3,?4,?5,?6)
        ON CONFLICT(id) DO UPDATE SET payload=excluded.payload,state=excluded.state,owner=excluded.owner", params![task.id,task.request.start_call_id,task.fingerprint,payload,task.state,task.owner]).map_err(|_| "review_persistence_failed")?;
    Ok(())
}
/// The stream checker reads only indexed identity/status columns, never source
/// JSON, and never initializes or migrates a profile from its network thread.
pub(crate) fn owner_matches(path:&Path,id:&str,owner:&str)->Result<bool,&'static str> {
    let connection=Connection::open_with_flags(path,rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY|rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX).map_err(|_|"review_persistence_failed")?;
    connection.busy_timeout(Duration::from_millis(25)).map_err(|_|"review_persistence_failed")?;
    let found:Option<(String,Option<String>)>=connection.query_row("SELECT state,owner FROM ai_tasks WHERE id=?1",[id],|row|Ok((row.get(0)?,row.get(1)?))).optional().map_err(|_|"review_persistence_failed")?;
    Ok(found.is_some_and(|(state,found)|state=="running"&&found.as_deref()==Some(owner)))
}
pub(crate) fn event(tx: &Transaction<'_>, task: &mut TaskRecord, kind: &'static str, tool: Option<&str>) -> Result<(), &'static str> {
    task.last_event_id = task.last_event_id.checked_add(1).filter(|n| *n <= i64::MAX as u64).ok_or("review_ai_budget")?;
    // All fields are an application-owned enum, counter, or localization key.
    // Arguments, provider text, URLs, connection references and credentials are never events.
    let mut value = json!({"id":task.last_event_id,"type":kind,"state":task.state,"round":task.rounds});
    if let Some(tool) = tool { value["tool"] = json!(tool); }
    if let Some(code) = &task.error_code { value["error_code"] = json!(code); }
    tx.execute("INSERT INTO ai_task_events(task_id,event_id,payload) VALUES(?1,?2,?3)", params![task.id,task.last_event_id as i64,value.to_string()]).map_err(|_| "review_persistence_failed")?;
    Ok(())
}
pub(crate) fn events(tx: &Transaction<'_>, id: &str, after: u64) -> Result<Vec<Value>, &'static str> {
    let after = i64::try_from(after).map_err(|_| "review_invalid_request")?;
    let mut statement = tx.prepare("SELECT payload FROM ai_task_events WHERE task_id=?1 AND event_id>?2 ORDER BY event_id LIMIT 256").map_err(|_| "review_persistence_failed")?;
    let rows = statement.query_map(params![id,after], |row| row.get::<_, String>(0)).map_err(|_| "review_persistence_failed")?;
    rows.map(|row| serde_json::from_str(&row.map_err(|_| "review_persistence_failed")?).map_err(|_| "review_persistence_failed")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn budget_rejects_unadvertised_fields_and_excessive_limits() {
        assert!(serde_json::from_value::<Budget>(json!({"max_rounds":64,"max_elapsed_ms":1800000,"reset_on_resume":true})).is_err());
        assert_eq!(Budget{max_rounds:257,max_elapsed_ms:1800000}.validate(),Err("review_ai_budget"));
        assert_eq!(Budget{max_rounds:64,max_elapsed_ms:7200001}.validate(),Err("review_ai_budget"));
        assert_eq!(Budget::default().max_rounds,64);assert_eq!(Budget::default().max_elapsed_ms,1800000);
    }
    #[test]
    fn lazy_newer_schema_and_byte_limit_are_observable() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::new(directory.path());
        assert!(!store.path.exists());
        store.run(|_| Ok(())).unwrap();
        let connection = Connection::open(&store.path).unwrap();
        connection.pragma_update(None,"user_version",SCHEMA_VERSION+1).unwrap();
        assert_eq!(store.run(|_| Ok(())).unwrap_err(),"review_unsupported_schema");
    }
    #[test]
    fn malformed_migration_rolls_back_version_and_ddl() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::new(directory.path());
        let connection = Connection::open(&store.path).unwrap();
        connection.execute_batch("CREATE TABLE ai_tasks(wrong TEXT)").unwrap();
        assert_eq!(store.run(|_| Ok(())).unwrap_err(),"review_persistence_failed");
        assert_eq!(connection.pragma_query_value(None,"user_version",|row|row.get::<_,i64>(0)).unwrap(),0);
    }
    #[test]
    fn schema_one_migrates_small_owner_columns_without_decoding_source_checkpoints() {
        let directory=tempfile::tempdir().unwrap();let store=Store::new(directory.path());let connection=Connection::open(&store.path).unwrap();
        connection.execute_batch("CREATE TABLE ai_tasks(id TEXT PRIMARY KEY,start_call_id TEXT UNIQUE,fingerprint TEXT,payload TEXT);
            CREATE TABLE ai_task_events(task_id TEXT,event_id INTEGER,payload TEXT,PRIMARY KEY(task_id,event_id));PRAGMA user_version=1;").unwrap();
        connection.execute("INSERT INTO ai_tasks VALUES('task','start','fingerprint',?1)",[json!({"state":"running","owner":"nonce","source":"not a decodable TaskRecord"}).to_string()]).unwrap();
        store.run(|_|Ok(())).unwrap();assert!(owner_matches(&store.path,"task","nonce").unwrap());assert!(!owner_matches(&store.path,"task","other-owner").unwrap());
        assert_eq!(connection.pragma_query_value(None,"user_version",|row|row.get::<_,i64>(0)).unwrap(),SCHEMA_VERSION);
    }
}
