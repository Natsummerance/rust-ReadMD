//! Private durable storage for review sessions. Ordinary history never owns it.
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

// Older kernels must reject this store before ignoring multi-file reservations.
pub(crate) const SCHEMA_VERSION: i64 = 5;
pub(crate) const DEFAULT_STORE_BYTES: usize = 64 * 1024 * 1024;

pub(crate) struct Store {
    pub(crate) path: PathBuf,
    pub(crate) byte_limit: usize,
}

impl Store {
    pub(crate) fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("review.sqlite3"),
            byte_limit: DEFAULT_STORE_BYTES,
        }
    }

    fn connect(&self) -> Result<Connection, &'static str> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| "review_persistence_failed")?;
        }
        let mut connection =
            Connection::open(&self.path).map_err(|_| "review_persistence_failed")?;
        connection
            .busy_timeout(Duration::from_secs(3))
            .map_err(|_| "review_persistence_failed")?;
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|_| "review_persistence_failed")?;
        if version > SCHEMA_VERSION {
            return Err("review_unsupported_schema");
        }
        // Version and all DDL share one transaction. A malformed old table must
        // never leave a half migration or an advanced user_version behind.
        let migration = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| "review_persistence_failed")?;
        let locked_version: i64 = migration
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|_| "review_persistence_failed")?;
        if locked_version > SCHEMA_VERSION {
            return Err("review_unsupported_schema");
        }
        if locked_version == 0 {
            migration.execute_batch(
                "CREATE TABLE IF NOT EXISTS review_documents (
                    id TEXT PRIMARY KEY NOT NULL, path TEXT UNIQUE NOT NULL,
                    pending_path TEXT UNIQUE, payload TEXT NOT NULL);
                 CREATE TABLE IF NOT EXISTS review_events (
                    document_id TEXT NOT NULL, event_id INTEGER NOT NULL,
                    kind TEXT NOT NULL, payload TEXT NOT NULL,
                    PRIMARY KEY(document_id,event_id));
                 CREATE TABLE IF NOT EXISTS review_receipts (
                    document_id TEXT NOT NULL, call_id TEXT NOT NULL,
                    fingerprint TEXT NOT NULL, payload TEXT NOT NULL,
                    PRIMARY KEY(document_id,call_id));
                 CREATE INDEX IF NOT EXISTS review_event_document ON review_events(document_id,event_id);"
            ).map_err(|_| "review_persistence_failed")?;
        }
        if locked_version < 2 {
            migration.execute_batch("CREATE TABLE IF NOT EXISTS review_texts (hash TEXT PRIMARY KEY NOT NULL,payload TEXT NOT NULL)")
                .map_err(|_|"review_persistence_failed")?;
        }
        if locked_version < 3 {
            migration.execute_batch(
                "CREATE TABLE IF NOT EXISTS review_renames (
                    operation_id TEXT PRIMARY KEY NOT NULL, document_id TEXT NOT NULL,
                    source_path TEXT NOT NULL, target_path TEXT NOT NULL,
                    state TEXT NOT NULL, payload TEXT NOT NULL);
                 CREATE INDEX IF NOT EXISTS review_rename_source ON review_renames(source_path,state);
                 CREATE INDEX IF NOT EXISTS review_rename_target ON review_renames(target_path,state);"
            ).map_err(|_| "review_persistence_failed")?;
        }
        if locked_version < 4 {
            migration.execute_batch("CREATE TABLE IF NOT EXISTS review_rename_abandoned (operation_id TEXT PRIMARY KEY NOT NULL)")
                .map_err(|_| "review_persistence_failed")?;
        }
        if locked_version < 5 {
            migration.execute_batch(
                "CREATE TABLE IF NOT EXISTS review_refactors (
                    operation_id TEXT PRIMARY KEY NOT NULL, plan_hash TEXT NOT NULL,
                    state TEXT NOT NULL, payload TEXT NOT NULL, reserved_bytes INTEGER NOT NULL);
                 CREATE TABLE IF NOT EXISTS review_refactor_paths (
                    path TEXT PRIMARY KEY COLLATE NOCASE NOT NULL, operation_id TEXT NOT NULL);
                 CREATE INDEX IF NOT EXISTS review_refactor_operation ON review_refactor_paths(operation_id);"
            ).map_err(|_| "review_persistence_failed")?;
        }
        // Check the complete shape before claiming this schema is supported.
        migration
            .prepare("SELECT id,path,pending_path,payload FROM review_documents LIMIT 0")
            .map_err(|_| "review_persistence_failed")?;
        migration
            .prepare("SELECT document_id,event_id,kind,payload FROM review_events LIMIT 0")
            .map_err(|_| "review_persistence_failed")?;
        migration
            .prepare("SELECT document_id,call_id,fingerprint,payload FROM review_receipts LIMIT 0")
            .map_err(|_| "review_persistence_failed")?;
        migration
            .prepare("SELECT hash,payload FROM review_texts LIMIT 0")
            .map_err(|_| "review_persistence_failed")?;
        migration
            .prepare("SELECT operation_id,document_id,source_path,target_path,state,payload FROM review_renames LIMIT 0")
            .map_err(|_| "review_persistence_failed")?;
        migration.prepare("SELECT operation_id FROM review_rename_abandoned LIMIT 0")
            .map_err(|_| "review_persistence_failed")?;
        migration.prepare("SELECT operation_id,plan_hash,state,payload,reserved_bytes FROM review_refactors LIMIT 0")
            .map_err(|_| "review_persistence_failed")?;
        migration.prepare("SELECT path,operation_id FROM review_refactor_paths LIMIT 0")
            .map_err(|_| "review_persistence_failed")?;
        if locked_version < SCHEMA_VERSION {
            migration
                .pragma_update(None, "user_version", SCHEMA_VERSION)
                .map_err(|_| "review_persistence_failed")?;
        }
        migration
            .commit()
            .map_err(|_| "review_persistence_failed")?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|_| "review_persistence_failed")?;
        connection
            .pragma_update(None, "synchronous", "FULL")
            .map_err(|_| "review_persistence_failed")?;
        connection
            .pragma_update(None, "wal_autocheckpoint", 128)
            .map_err(|_| "review_persistence_failed")?;
        connection
            .pragma_update(None, "journal_size_limit", 1024 * 1024)
            .map_err(|_| "review_persistence_failed")?;
        // Bound the main database as well as charging logical record bytes.
        let page_size: i64 = connection
            .pragma_query_value(None, "page_size", |row| row.get(0))
            .map_err(|_| "review_persistence_failed")?;
        let pages = (DEFAULT_STORE_BYTES as i64 / page_size.max(1)).max(128);
        connection
            .pragma_update(None, "max_page_count", pages)
            .map_err(|_| "review_persistence_failed")?;
        Ok(connection)
    }

    pub(crate) fn run<T>(
        &self,
        f: impl FnOnce(&Transaction<'_>) -> Result<T, &'static str>,
    ) -> Result<T, &'static str> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| "review_persistence_failed")?;
        let value = f(&transaction)?;
        let used = used_bytes(&transaction)?;
        if used > self.byte_limit {
            return Err("review_budget");
        }
        transaction
            .commit()
            .map_err(|_| "review_persistence_failed")?;
        Ok(value)
    }
}

pub(crate) fn used_bytes(tx: &Transaction<'_>) -> Result<usize, &'static str> {
    let used: i64=tx.query_row(
        "SELECT COALESCE((SELECT SUM(length(CAST(payload AS BLOB))+length(id)+length(path)+COALESCE(length(pending_path),0)
                  +COALESCE(json_extract(payload,'$.save_intent.reserved_bytes'),0)
                  +COALESCE(json_extract(payload,'$.rename_intent.reserved_bytes'),0)) FROM review_documents),0)
              + COALESCE((SELECT SUM(length(CAST(payload AS BLOB))+length(document_id)+length(kind)+16) FROM review_events),0)
              + COALESCE((SELECT SUM(length(CAST(payload AS BLOB))+length(document_id)+length(call_id)+length(fingerprint)) FROM review_receipts),0)
              + COALESCE((SELECT SUM(length(CAST(payload AS BLOB))+length(hash)) FROM review_texts),0)
              + COALESCE((SELECT SUM(length(CAST(payload AS BLOB))+length(operation_id)+length(document_id)+length(source_path)+length(target_path)+length(state)) FROM review_renames),0)
              + COALESCE((SELECT SUM(length(operation_id)) FROM review_rename_abandoned),0)
              + COALESCE((SELECT SUM(length(CAST(payload AS BLOB))+length(operation_id)+length(plan_hash)+length(state)+reserved_bytes) FROM review_refactors),0)
              + COALESCE((SELECT SUM(length(path)+length(operation_id)) FROM review_refactor_paths),0)",
        [],|row|row.get(0)).map_err(|_|"review_persistence_failed")?;
    usize::try_from(used).map_err(|_| "review_persistence_failed")
}

pub(crate) fn rename_operation(tx: &Transaction<'_>, operation: &str) -> Result<Option<String>, &'static str> {
    tx.query_row("SELECT payload FROM review_renames WHERE operation_id=?1", [operation], |row| row.get(0))
        .optional().map_err(|_| "review_persistence_failed")
}

pub(crate) fn pending_rename_at(tx: &Transaction<'_>, path: &str) -> Result<Option<String>, &'static str> {
    tx.query_row("SELECT payload FROM review_renames WHERE (source_path=?1 COLLATE NOCASE OR target_path=?1 COLLATE NOCASE) AND state IN ('prepared','recovery_pending')
        UNION ALL SELECT r.payload FROM review_refactors r JOIN review_refactor_paths p ON r.operation_id=p.operation_id WHERE p.path=?1 COLLATE NOCASE LIMIT 1", [path], |row| row.get(0))
        .optional().map_err(|_| "review_persistence_failed")
}

pub(crate) fn rename_counts(tx: &Transaction<'_>) -> Result<(usize, usize), &'static str> {
    let (all, pending): (i64, i64) = tx.query_row(
        "SELECT COUNT(*)+(SELECT COUNT(*) FROM review_rename_abandoned),COALESCE(SUM(state IN ('prepared','recovery_pending')),0) FROM review_renames",
        [], |row| Ok((row.get(0)?, row.get(1)?)),
    ).map_err(|_| "review_persistence_failed")?;
    Ok((usize::try_from(all).map_err(|_| "review_persistence_failed")?,
        usize::try_from(pending).map_err(|_| "review_persistence_failed")?))
}

pub(crate) fn rename_abandoned(tx: &Transaction<'_>, operation: &str) -> Result<bool, &'static str> {
    tx.query_row("SELECT 1 FROM review_rename_abandoned WHERE operation_id=?1", [operation], |_| Ok(()))
        .optional().map(|value| value.is_some()).map_err(|_| "review_persistence_failed")
}

pub(crate) fn abandon_rename(tx: &Transaction<'_>, operation: &str) -> Result<(), &'static str> {
    tx.execute("INSERT INTO review_rename_abandoned(operation_id) VALUES(?1)", [operation])
        .map_err(|_| "review_persistence_failed")?;
    Ok(())
}

pub(crate) fn put_rename(tx: &Transaction<'_>, operation: &str, document: &str, source: &str, target: &str, state: &str, payload: &str) -> Result<(), &'static str> {
    tx.execute("INSERT INTO review_renames(operation_id,document_id,source_path,target_path,state,payload) VALUES(?1,?2,?3,?4,?5,?6)
                ON CONFLICT(operation_id) DO UPDATE SET state=excluded.state,payload=excluded.payload",
        params![operation,document,source,target,state,payload]).map_err(|_| "review_persistence_failed")?;
    Ok(())
}

pub(crate) fn by_id(tx: &Transaction<'_>, id: &str) -> Result<Option<String>, &'static str> {
    tx.query_row(
        "SELECT payload FROM review_documents WHERE id=?1",
        [id],
        |row| row.get(0),
    )
    .optional()
    .map_err(|_| "review_persistence_failed")
}

pub(crate) fn by_path(tx: &Transaction<'_>, path: &str) -> Result<Option<String>, &'static str> {
    tx.query_row(
        "SELECT payload FROM review_documents WHERE path=?1 OR pending_path=?1",
        [path],
        |row| row.get(0),
    )
    .optional()
    .map_err(|_| "review_persistence_failed")
}

pub(crate) fn path_available(
    tx: &Transaction<'_>,
    path: &str,
    id: &str,
) -> Result<bool, &'static str> {
    let other: Option<String> = tx
        .query_row(
            "SELECT id FROM review_documents WHERE (path=?1 OR pending_path=?1) AND id<>?2 LIMIT 1",
            params![path, id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| "review_persistence_failed")?;
    Ok(other.is_none())
}

pub(crate) fn put_document(
    tx: &Transaction<'_>,
    id: &str,
    path: &str,
    pending_path: Option<&str>,
    payload: &str,
) -> Result<(), &'static str> {
    tx.execute("INSERT INTO review_documents(id,path,pending_path,payload) VALUES(?1,?2,?3,?4)
                ON CONFLICT(id) DO UPDATE SET path=excluded.path,pending_path=excluded.pending_path,payload=excluded.payload",
        params![id,path,pending_path,payload]).map_err(|_| "review_persistence_failed")?;
    Ok(())
}

pub(crate) fn event(
    tx: &Transaction<'_>,
    id: &str,
    event_id: u64,
    kind: &str,
    payload: &Value,
) -> Result<(), &'static str> {
    let event_id = i64::try_from(event_id).map_err(|_| "review_budget")?;
    let payload = serde_json::to_string(payload).map_err(|_| "review_persistence_failed")?;
    tx.execute(
        "INSERT INTO review_events(document_id,event_id,kind,payload) VALUES(?1,?2,?3,?4)",
        params![id, event_id, kind, payload],
    )
    .map_err(|_| "review_persistence_failed")?;
    Ok(())
}

pub(crate) fn receipt(
    tx: &Transaction<'_>,
    id: &str,
    call: &str,
    fingerprint: &str,
) -> Result<Option<Value>, &'static str> {
    let found: Option<(String, String)> = tx
        .query_row(
            "SELECT fingerprint,payload FROM review_receipts WHERE document_id=?1 AND call_id=?2",
            params![id, call],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|_| "review_persistence_failed")?;
    match found {
        Some((old, payload)) if old == fingerprint => {
            let value: Value =
                serde_json::from_str(&payload).map_err(|_| "review_persistence_failed")?;
            Ok(Some(value))
        }
        Some(_) => Err("review_call_conflict"),
        None => Ok(None),
    }
}

pub(crate) fn put_receipt(
    tx: &Transaction<'_>,
    id: &str,
    call: &str,
    fingerprint: &str,
    payload: &Value,
) -> Result<(), &'static str> {
    let payload = serde_json::to_string(payload).map_err(|_| "review_persistence_failed")?;
    tx.execute(
        "INSERT INTO review_receipts(document_id,call_id,fingerprint,payload) VALUES(?1,?2,?3,?4)",
        params![id, call, fingerprint, payload],
    )
    .map_err(|_| "review_persistence_failed")?;
    Ok(())
}

pub(crate) fn lookup_receipt(
    tx: &Transaction<'_>,
    id: &str,
    call: &str,
) -> Result<Option<Value>, &'static str> {
    let payload: Option<String> = tx
        .query_row(
            "SELECT payload FROM review_receipts WHERE document_id=?1 AND call_id=?2",
            params![id, call],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| "review_persistence_failed")?;
    payload
        .map(|payload| serde_json::from_str(&payload).map_err(|_| "review_persistence_failed"))
        .transpose()
}

pub(crate) fn all_payloads(tx: &Transaction<'_>) -> Result<Vec<String>, &'static str> {
    let mut statement = tx
        .prepare("SELECT payload FROM review_documents ORDER BY id")
        .map_err(|_| "review_persistence_failed")?;
    let rows = statement
        .query_map([], |row| row.get(0))
        .map_err(|_| "review_persistence_failed")?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| "review_persistence_failed")
}

pub(crate) fn put_text(tx: &Transaction<'_>, hash: &str, text: &str) -> Result<(), &'static str> {
    tx.execute(
        "INSERT OR IGNORE INTO review_texts(hash,payload) VALUES(?1,?2)",
        params![hash, text],
    )
    .map_err(|_| "review_persistence_failed")?;
    Ok(())
}

pub(crate) fn texts(tx: &Transaction<'_>) -> Result<Vec<(String, String)>, &'static str> {
    let mut statement = tx
        .prepare("SELECT hash,payload FROM review_texts")
        .map_err(|_| "review_persistence_failed")?;
    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|_| "review_persistence_failed")?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| "review_persistence_failed")
}

pub(crate) fn text(tx: &Transaction<'_>, hash: &str) -> Result<Option<String>, &'static str> {
    tx.query_row(
        "SELECT payload FROM review_texts WHERE hash=?1",
        [hash],
        |row| row.get(0),
    )
    .optional()
    .map_err(|_| "review_persistence_failed")
}

pub(crate) fn events_after(
    tx: &Transaction<'_>,
    id: &str,
    event: u64,
) -> Result<Vec<String>, &'static str> {
    let event = i64::try_from(event).map_err(|_| "review_persistence_failed")?;
    let mut statement=tx.prepare("SELECT payload FROM review_events WHERE document_id=?1 AND event_id>?2 ORDER BY event_id DESC")
        .map_err(|_|"review_persistence_failed")?;
    let rows = statement
        .query_map(params![id, event], |row| row.get(0))
        .map_err(|_| "review_persistence_failed")?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| "review_persistence_failed")
}
