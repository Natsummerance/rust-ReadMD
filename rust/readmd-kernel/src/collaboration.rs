//! Durable Yrs sessions. This service has no authority over host document paths.
//! Tokens are returned once and stored only as SHA-256 digests. An acknowledgement
//! means the update is committed to SQLite, independently of local-file saving.
use crate::{
    document_transaction::{self, ContentTransaction},
    ApiError, ApiResult,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use yrs::updates::{decoder::Decode, encoder::Encode};
use yrs::{
    Assoc, Doc, GetString, IndexedSequence, OffsetKind, Options, ReadTxn, StateVector, StickyIndex,
    Text, Transact, Update,
};

const MAX_UPDATE: usize = 1024 * 1024;
const MAX_DOCUMENT: usize = 8 * 1024 * 1024;
const MAX_ROOMS: usize = 64;
const COMPACT_AFTER: i64 = 128;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Owner,
    Editor,
    Reviewer,
    Reader,
}
impl Role {
    fn text(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Editor => "editor",
            Self::Reviewer => "reviewer",
            Self::Reader => "reader",
        }
    }
    fn parse(s: &str) -> Self {
        match s {
            "owner" => Self::Owner,
            "editor" => Self::Editor,
            "reviewer" => Self::Reviewer,
            _ => Self::Reader,
        }
    }
    fn edits(self) -> bool {
        matches!(self, Self::Owner | Self::Editor)
    }
}
#[derive(Clone, Serialize)]
pub struct Credential {
    pub room: String,
    pub member: String,
    pub token: String,
    pub role: Role,
}
struct Auth {
    member: String,
    role: Role,
    name: String,
}
struct Presence {
    value: Value,
    seen: i64,
}
struct State {
    db: Connection,
    rooms: HashMap<String, Doc>,
    presence: HashMap<String, HashMap<String, Presence>>,
}
pub struct Service {
    state: Mutex<State>,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .min(i64::MAX as u64) as i64
}
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}
fn err(status: u16, code: &'static str) -> ApiError {
    ApiError::new(status, code)
}
fn database(_: rusqlite::Error) -> ApiError {
    err(503, "collaboration_storage")
}
fn invalid() -> ApiError {
    err(422, "collaboration_invalid")
}
fn decode(s: &str, limit: usize) -> ApiResult<Vec<u8>> {
    if s.len() > limit.div_ceil(3) * 4 {
        return Err(err(413, "collaboration_budget"));
    }
    let bytes = STANDARD.decode(s).map_err(|_| invalid())?;
    if bytes.len() > limit {
        return Err(err(413, "collaboration_budget"));
    }
    Ok(bytes)
}
fn fresh() -> Doc {
    Doc::with_options(Options {
        offset_kind: OffsetKind::Utf16,
        ..Default::default()
    })
}
fn full(doc: &Doc) -> Vec<u8> {
    doc.transact()
        .encode_state_as_update_v1(&StateVector::default())
}
fn content(doc: &Doc) -> String {
    doc.get_or_insert_text("content")
        .get_string(&doc.transact())
}
fn restore(db: &Connection, room: &str) -> ApiResult<Doc> {
    let snapshot: Vec<u8> = db
        .query_row(
            "SELECT snapshot FROM rooms WHERE id=?1 AND closed=0",
            [room],
            |r| r.get(0),
        )
        .map_err(database)?;
    let doc = fresh();
    doc.transact_mut()
        .apply_update(Update::decode_v1(&snapshot).map_err(|_| invalid())?)
        .map_err(|_| invalid())?;
    let mut statement = db
        .prepare("SELECT bytes FROM updates WHERE room=?1 ORDER BY seq")
        .map_err(database)?;
    let rows = statement
        .query_map([room], |r| r.get::<_, Vec<u8>>(0))
        .map_err(database)?;
    for row in rows {
        doc.transact_mut()
            .apply_update(Update::decode_v1(&row.map_err(database)?).map_err(|_| invalid())?)
            .map_err(|_| invalid())?;
    }
    Ok(doc)
}
impl State {
    fn authenticate(&self, room: &str, secret: &str) -> ApiResult<Auth> {
        if secret.len() != 64 {
            return Err(err(403, "collaboration_forbidden"));
        }
        self.db.query_row("SELECT m.id,m.role,m.name FROM members m JOIN rooms r ON r.id=m.room WHERE m.room=?1 AND m.digest=?2 AND m.revoked=0 AND r.closed=0",params![room,document_transaction::revision(secret)],|row|Ok(Auth{member:row.get(0)?,role:Role::parse(&row.get::<_,String>(1)?),name:row.get(2)?})).optional().map_err(database)?.ok_or_else(||err(403,"collaboration_forbidden"))
    }
    fn load(&mut self, room: &str) -> ApiResult<()> {
        if !self.rooms.contains_key(room) {
            if self.rooms.len() >= MAX_ROOMS {
                self.rooms.clear();
            }
            self.rooms.insert(room.into(), restore(&self.db, room)?);
        }
        Ok(())
    }
    fn apply(&mut self, room: &str, member: &str, bytes: &[u8]) -> ApiResult<()> {
        if bytes.len() > MAX_UPDATE {
            return Err(err(413, "collaboration_budget"));
        }
        let update = Update::decode_v1(bytes).map_err(|_| invalid())?;
        self.load(room)?;
        let doc = &self.rooms[room];
        if doc.transact_mut().apply_update(update).is_err() {
            self.rooms.remove(room);
            return Err(invalid());
        }
        // Includes pending out-of-order updates, so compaction cannot discard them.
        let snapshot = full(doc);
        if snapshot.len() > MAX_DOCUMENT || content(doc).len() > MAX_DOCUMENT {
            self.rooms.remove(room);
            return Err(err(413, "collaboration_budget"));
        }
        let persist = (|| -> rusqlite::Result<()> {
            let tx = self.db.transaction()?;
            tx.execute(
                "INSERT INTO updates(room,member,bytes,created) VALUES(?1,?2,?3,?4)",
                params![room, member, bytes, now()],
            )?;
            let (count, size): (i64, i64) = tx.query_row(
                "SELECT COUNT(*),COALESCE(SUM(length(bytes)),0) FROM updates WHERE room=?1",
                [room],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if count >= COMPACT_AFTER || size >= 4 * 1024 * 1024 {
                tx.execute(
                    "UPDATE rooms SET snapshot=?2 WHERE id=?1",
                    params![room, snapshot],
                )?;
                tx.execute("DELETE FROM updates WHERE room=?1", [room])?;
            }
            tx.commit()
        })();
        if let Err(e) = persist {
            self.rooms.remove(room);
            return Err(database(e));
        }
        Ok(())
    }
}
impl Service {
    /// Administrator-only filesystem operation, deliberately absent from LAN API.
    /// VACUUM INTO includes committed WAL updates in a consistent snapshot.
    pub fn backup(&self, destination: &Path) -> ApiResult<()> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        backup_connection(&state.db, destination)
    }
    /// Restore into a new profile only; never replace an active database/WAL.
    pub fn restore_backup(source: &Path, destination: &Path) -> ApiResult<()> {
        let db = Connection::open_with_flags(source, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(database)?;
        db.busy_timeout(std::time::Duration::from_secs(3)).map_err(database)?;
        let check: String = db.query_row("PRAGMA quick_check", [], |r| r.get(0)).map_err(database)?;
        if check != "ok" { return Err(err(422, "collaboration_backup_invalid")); }
        // Reject unrelated SQLite files rather than silently creating empty rooms.
        db.prepare("SELECT r.id,r.snapshot,m.digest,u.bytes,a.bytes,v.body FROM rooms r LEFT JOIN members m ON m.room=r.id LEFT JOIN updates u ON u.room=r.id LEFT JOIN attachments a ON a.room=r.id LEFT JOIN reviews v ON v.room=r.id LIMIT 0").map_err(database)?;
        backup_connection(&db, destination)
    }
    pub fn open(path: &Path) -> ApiResult<Self> {
        let db = Connection::open(path).map_err(database)?;
        db.busy_timeout(std::time::Duration::from_secs(3))
            .map_err(database)?;
        db.execute_batch("PRAGMA auto_vacuum=INCREMENTAL; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;
            CREATE TABLE IF NOT EXISTS rooms(id TEXT PRIMARY KEY,title TEXT NOT NULL,snapshot BLOB NOT NULL,created INTEGER NOT NULL,closed INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE IF NOT EXISTS members(id TEXT PRIMARY KEY,room TEXT NOT NULL REFERENCES rooms(id),digest TEXT NOT NULL UNIQUE,role TEXT NOT NULL,name TEXT NOT NULL,revoked INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE IF NOT EXISTS updates(seq INTEGER PRIMARY KEY,room TEXT NOT NULL REFERENCES rooms(id),member TEXT NOT NULL,bytes BLOB NOT NULL,created INTEGER NOT NULL);
            CREATE INDEX IF NOT EXISTS updates_room ON updates(room,seq);
            CREATE TABLE IF NOT EXISTS reviews(id TEXT PRIMARY KEY,room TEXT NOT NULL REFERENCES rooms(id),member TEXT NOT NULL,body TEXT NOT NULL,created INTEGER NOT NULL,status TEXT NOT NULL DEFAULT 'open');
            CREATE TABLE IF NOT EXISTS attachments(id TEXT PRIMARY KEY,room TEXT NOT NULL REFERENCES rooms(id),name TEXT NOT NULL,mime TEXT NOT NULL,bytes BLOB NOT NULL);") .map_err(database)?;
        // Ended sessions are recoverable for 30 days, with an explicit maximum
        // of 64 retained sessions. Never discard an active acknowledged room.
        db.execute_batch("CREATE TABLE IF NOT EXISTS retention(room TEXT PRIMARY KEY REFERENCES rooms(id),closed_at INTEGER NOT NULL);").map_err(database)?;
        db.execute("INSERT OR IGNORE INTO retention(room,closed_at) SELECT id,?1 FROM rooms WHERE closed=1",[now()]).map_err(database)?;
        let mut state = State {
            db,
            rooms: HashMap::new(),
            presence: HashMap::new(),
        };
        prune(&mut state)?;
        Ok(Self {
            state: Mutex::new(state),
        })
    }
    pub fn create(&self, title: &str, text: &str, name: &str) -> ApiResult<Credential> {
        if title.len() > 512 || name.len() > 128 || text.len() > MAX_DOCUMENT / 2 {
            return Err(err(413, "collaboration_budget"));
        }
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let count: i64 =
            s.db.query_row("SELECT COUNT(*) FROM rooms WHERE closed=0", [], |r| {
                r.get(0)
            })
            .map_err(database)?;
        if count >= MAX_ROOMS as i64 {
            return Err(err(429, "collaboration_budget"));
        }
        let doc = fresh();
        doc.get_or_insert_text("content")
            .insert(&mut doc.transact_mut(), 0, text);
        let credential = Credential {
            room: id(),
            member: id(),
            token: token(),
            role: Role::Owner,
        };
        let tx = s.db.transaction().map_err(database)?;
        tx.execute(
            "INSERT INTO rooms(id,title,snapshot,created) VALUES(?1,?2,?3,?4)",
            params![credential.room, title, full(&doc), now()],
        )
        .map_err(database)?;
        tx.execute(
            "INSERT INTO members(id,room,digest,role,name) VALUES(?1,?2,?3,'owner',?4)",
            params![
                credential.member,
                credential.room,
                document_transaction::revision(&credential.token),
                name
            ],
        )
        .map_err(database)?;
        tx.commit().map_err(database)?;
        s.rooms.insert(credential.room.clone(), doc);
        Ok(credential)
    }
    pub fn invite(
        &self,
        room: &str,
        secret: &str,
        role: Role,
        name: &str,
    ) -> ApiResult<Credential> {
        if name.len() > 128 || role == Role::Owner {
            return Err(invalid());
        }
        let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if s.authenticate(room, secret)?.role != Role::Owner {
            return Err(err(403, "collaboration_forbidden"));
        }
        let count: i64 =
            s.db.query_row(
                "SELECT COUNT(*) FROM members WHERE room=?1 AND revoked=0",
                [room],
                |r| r.get(0),
            )
            .map_err(database)?;
        if count >= 256 {
            return Err(err(429, "collaboration_budget"));
        }
        let credential = Credential {
            room: room.into(),
            member: id(),
            token: token(),
            role,
        };
        s.db.execute(
            "INSERT INTO members(id,room,digest,role,name) VALUES(?1,?2,?3,?4,?5)",
            params![
                credential.member,
                room,
                document_transaction::revision(&credential.token),
                role.text(),
                name
            ],
        )
        .map_err(database)?;
        Ok(credential)
    }
    pub fn permission(
        &self,
        room: &str,
        secret: &str,
        member: &str,
        role: Option<Role>,
    ) -> ApiResult<()> {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let auth = s.authenticate(room, secret)?;
        if auth.role != Role::Owner || member == auth.member || role == Some(Role::Owner) {
            return Err(err(403, "collaboration_forbidden"));
        }
        let changed = match role {
            Some(role) => s.db.execute(
                "UPDATE members SET role=?3 WHERE room=?1 AND id=?2 AND role!='owner'",
                params![room, member, role.text()],
            ),
            None => s.db.execute(
                "UPDATE members SET revoked=1 WHERE room=?1 AND id=?2 AND role!='owner'",
                params![room, member],
            ),
        }
        .map_err(database)?;
        if changed == 0 {
            return Err(err(404, "collaboration_not_found"));
        }
        if let Some(p) = s.presence.get_mut(room) {
            p.remove(member);
        }
        Ok(())
    }
    pub fn sync(
        &self,
        room: &str,
        secret: &str,
        vector: &str,
        update: Option<&str>,
        presence: Option<Value>,
    ) -> ApiResult<Value> {
        let vector = StateVector::decode_v1(&decode(vector, 64 * 1024)?).map_err(|_| invalid())?;
        let bytes = update.map(|u| decode(u, MAX_UPDATE)).transpose()?;
        if presence
            .as_ref()
            .is_some_and(|p| p.to_string().len() > 4096)
        {
            return Err(err(413, "collaboration_budget"));
        }
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let auth = s.authenticate(room, secret)?;
        if let Some(bytes) = bytes.as_ref() {
            if !auth.role.edits() {
                return Err(err(403, "collaboration_forbidden"));
            }
            s.apply(room, &auth.member, bytes)?;
        }
        s.load(room)?;
        let doc = &s.rooms[room];
        let txn = doc.transact();
        let response_update = STANDARD.encode(txn.encode_state_as_update_v1(&vector));
        let response_vector = STANDARD.encode(txn.state_vector().encode_v1());
        drop(txn);
        let peers = s.presence.entry(room.into()).or_default();
        let time = now();
        peers.retain(|_, p| time.saturating_sub(p.seen) < 45);
        if let Some(value) = presence {
            peers.insert(auth.member.clone(),Presence{value:json!({"member":auth.member,"name":auth.name,"role":auth.role,"selection":value}),seen:time});
        }
        Ok(
            json!({"ok":true,"update":response_update,"vector":response_vector,"role":auth.role,"member":auth.member,"synced":true,"local_file_saved":false,"peers":peers.values().map(|p|p.value.clone()).collect::<Vec<_>>() }),
        )
    }
    pub fn snapshot(&self, room: &str, secret: &str) -> ApiResult<Value> {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let auth = s.authenticate(room, secret)?;
        s.load(room)?;
        let text = content(&s.rooms[room]);
        let title: String =
            s.db.query_row("SELECT title FROM rooms WHERE id=?1", [room], |r| r.get(0))
                .map_err(database)?;
        Ok(
            json!({"ok":true,"title":title,"content":text,"revision":document_transaction::revision(&text),"role":auth.role,"member":auth.member}),
        )
    }
    pub fn review(&self, room: &str, secret: &str, body: Value) -> ApiResult<String> {
        if body.to_string().len() > 128 * 1024
            || body["text"].as_str().is_none_or(|s| s.len() > 32 * 1024)
        {
            return Err(invalid());
        }
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let auth = s.authenticate(room, secret)?;
        if auth.role == Role::Reader {
            return Err(err(403, "collaboration_forbidden"));
        }
        let mut body = body;
        body.as_object_mut().ok_or_else(invalid)?.remove("_anchors");
        if !body["transaction"].is_null() {
            s.load(room)?;
            let doc = &s.rooms[room];
            let source = content(doc);
            let tx: ContentTransaction =
                serde_json::from_value(body["transaction"].clone()).map_err(|_| invalid())?;
            if tx.origin != "review" {
                return Err(invalid());
            }
            document_transaction::apply(&source, &tx)
                .map_err(|_| err(409, "collaboration_conflict"))?;
            let text = doc.get_or_insert_text("content");
            let txn = doc.transact();
            let mut anchors = Vec::new();
            for edit in &tx.edits {
                let from =
                    document_transaction::byte_offset(&source, edit.from).map_err(|_| invalid())?;
                let to =
                    document_transaction::byte_offset(&source, edit.to).map_err(|_| invalid())?;
                anchors.push(json!({"from":text.sticky_index(&txn,edit.from as u32,Assoc::After).ok_or_else(invalid)?,"to":text.sticky_index(&txn,edit.to as u32,if edit.from==edit.to{Assoc::After}else{Assoc::Before}).ok_or_else(invalid)?,"original":&source[from..to]}));
            }
            body["_anchors"] = json!(anchors);
        }
        let count: i64 =
            s.db.query_row("SELECT COUNT(*) FROM reviews WHERE room=?1", [room], |r| {
                r.get(0)
            })
            .map_err(database)?;
        if count >= 10000 {
            return Err(err(429, "collaboration_budget"));
        }
        let size: i64 =
            s.db.query_row(
                "SELECT COALESCE(SUM(length(body)),0) FROM reviews WHERE room=?1",
                [room],
                |r| r.get(0),
            )
            .map_err(database)?;
        if size as usize + body.to_string().len() > 8 * 1024 * 1024 {
            return Err(err(413, "collaboration_budget"));
        }
        let review = id();
        s.db.execute(
            "INSERT INTO reviews(id,room,member,body,created) VALUES(?1,?2,?3,?4,?5)",
            params![review, room, auth.member, body.to_string(), now()],
        )
        .map_err(database)?;
        Ok(review)
    }
    pub fn reviews(&self, room: &str, secret: &str) -> ApiResult<Value> {
        let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.authenticate(room, secret)?;
        let mut query=s.db.prepare("SELECT r.id,r.member,r.body,r.status,m.name FROM reviews r JOIN members m ON m.id=r.member WHERE r.room=?1 ORDER BY r.created,r.id").map_err(database)?;
        let rows=query.query_map([room],|r|Ok(json!({"id":r.get::<_,String>(0)?,"member":r.get::<_,String>(1)?,"body":serde_json::from_str::<Value>(&r.get::<_,String>(2)?).unwrap_or(Value::Null),"status":r.get::<_,String>(3)?,"name":r.get::<_,String>(4)?}))).map_err(database)?;
        Ok(
            json!({"ok":true,"reviews":rows.collect::<rusqlite::Result<Vec<_>>>().map_err(database)?}),
        )
    }
    pub fn resolve(&self, room: &str, secret: &str, review: &str, accept: bool) -> ApiResult<()> {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let auth = s.authenticate(room, secret)?;
        if !auth.role.edits() {
            return Err(err(403, "collaboration_forbidden"));
        }
        let body: String =
            s.db.query_row(
                "SELECT body FROM reviews WHERE id=?1 AND room=?2 AND status='open'",
                params![review, room],
                |r| r.get(0),
            )
            .optional()
            .map_err(database)?
            .ok_or_else(|| err(409, "collaboration_conflict"))?;
        let v: Value = serde_json::from_str(&body).map_err(|_| invalid())?;
        if accept && !v["transaction"].is_null() {
            s.load(room)?;
            let source = content(&s.rooms[room]);
            let mut tx: ContentTransaction =
                serde_json::from_value(v["transaction"].clone()).map_err(|_| invalid())?;
            if tx.base_revision != document_transaction::revision(&source) {
                // Resolve server-created relative positions, then compare the exact
                // reviewed slice. Unrelated edits may move a suggestion; edits
                // within its range must remain an explicit conflict.
                let anchors = v["_anchors"]
                    .as_array()
                    .filter(|a| a.len() == tx.edits.len())
                    .ok_or_else(|| err(409, "collaboration_conflict"))?;
                let doc = &s.rooms[room];
                let text = doc.get_or_insert_text("content");
                let txn = doc.transact();
                for (edit, anchor) in tx.edits.iter_mut().zip(anchors) {
                    let resolve = |key: &str| -> ApiResult<usize> {
                        let position: StickyIndex =
                            serde_json::from_value(anchor[key].clone()).map_err(|_| invalid())?;
                        let offset = position
                            .get_offset(&txn)
                            .ok_or_else(|| err(409, "collaboration_conflict"))?;
                        if yrs::TextRef::from(offset.branch) != text {
                            return Err(invalid());
                        }
                        Ok(offset.index as usize)
                    };
                    edit.from = resolve("from")?;
                    edit.to = resolve("to")?;
                    let from = document_transaction::byte_offset(&source, edit.from)
                        .map_err(|_| invalid())?;
                    let to = document_transaction::byte_offset(&source, edit.to)
                        .map_err(|_| invalid())?;
                    if from > to || Some(&source[from..to]) != anchor["original"].as_str() {
                        return Err(err(409, "collaboration_conflict"));
                    }
                }
                tx.base_revision = document_transaction::revision(&source);
            }
            let result = document_transaction::apply(&source, &tx)
                .map_err(|_| err(409, "collaboration_conflict"))?;
            if result.content.len() > MAX_DOCUMENT {
                return Err(err(413, "collaboration_budget"));
            }
            // Work on a fork so rejection/storage failure cannot leak a partial edit.
            let doc = fresh();
            doc.transact_mut()
                .apply_update(Update::decode_v1(&full(&s.rooms[room])).map_err(|_| invalid())?)
                .map_err(|_| invalid())?;
            let vector = doc.transact().state_vector();
            let text = doc.get_or_insert_text("content");
            let mut edits = tx.edits;
            edits.sort_by_key(|e| std::cmp::Reverse(e.from));
            {
                let mut txn = doc.transact_mut();
                for e in edits {
                    if e.to > e.from {
                        text.remove_range(&mut txn, e.from as u32, (e.to - e.from) as u32);
                    }
                    if !e.insert.is_empty() {
                        text.insert(&mut txn, e.from as u32, &e.insert);
                    }
                }
            }
            let bytes = doc.transact().encode_state_as_update_v1(&vector);
            let snapshot = full(&doc);
            if bytes.len() > MAX_UPDATE || snapshot.len() > MAX_DOCUMENT {
                return Err(err(413, "collaboration_budget"));
            }
            // Review state and CRDT update are committed atomically.
            let dbtx = s.db.transaction().map_err(database)?;
            dbtx.execute(
                "INSERT INTO updates(room,member,bytes,created) VALUES(?1,?2,?3,?4)",
                params![room, auth.member, bytes, now()],
            )
            .map_err(database)?;
            dbtx.execute("UPDATE reviews SET status='accepted' WHERE id=?1", [review])
                .map_err(database)?;
            dbtx.commit().map_err(database)?;
            s.rooms.insert(room.into(), doc);
        } else {
            s.db.execute(
                "UPDATE reviews SET status=?2 WHERE id=?1",
                params![review, if accept { "accepted" } else { "rejected" }],
            )
            .map_err(database)?;
        }
        Ok(())
    }
    pub fn attachment(
        &self,
        room: &str,
        secret: &str,
        name: &str,
        mime: &str,
        bytes: &[u8],
    ) -> ApiResult<String> {
        if bytes.len() > 16 * 1024 * 1024 || name.len() > 256 || mime.len() > 128 {
            return Err(err(413, "collaboration_budget"));
        }
        let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !s.authenticate(room, secret)?.role.edits() {
            return Err(err(403, "collaboration_forbidden"));
        }
        // Deduplicate within the authorized room only; never disclose that an
        // attachment exists in a different session.
        if let Some(asset) =
            s.db.query_row(
                "SELECT id FROM attachments WHERE room=?1 AND mime=?2 AND bytes=?3",
                params![room, mime, bytes],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(database)?
        {
            return Ok(asset);
        }
        let size: i64 =
            s.db.query_row(
                "SELECT COALESCE(SUM(length(bytes)),0) FROM attachments WHERE room=?1",
                [room],
                |r| r.get(0),
            )
            .map_err(database)?;
        let total: i64 =
            s.db.query_row(
                "SELECT COALESCE(SUM(length(bytes)),0) FROM attachments",
                [],
                |r| r.get(0),
            )
            .map_err(database)?;
        if size as usize + bytes.len() > 64 * 1024 * 1024
            || total as usize + bytes.len() > 256 * 1024 * 1024
        {
            return Err(err(413, "collaboration_budget"));
        }
        let asset = id();
        s.db.execute(
            "INSERT INTO attachments(id,room,name,mime,bytes) VALUES(?1,?2,?3,?4,?5)",
            params![asset, room, name, mime, bytes],
        )
        .map_err(database)?;
        Ok(asset)
    }
    pub fn read_attachment(&self, room: &str, secret: &str, asset: &str) -> ApiResult<Vec<u8>> {
        let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.authenticate(room, secret)?;
        s.db.query_row(
            "SELECT bytes FROM attachments WHERE id=?1 AND room=?2",
            params![asset, room],
            |r| r.get(0),
        )
        .optional()
        .map_err(database)?
        .ok_or_else(|| err(404, "collaboration_not_found"))
    }
    pub fn attachments(&self, room: &str, secret: &str) -> ApiResult<Value> {
        let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.authenticate(room, secret)?;
        let mut q =
            s.db.prepare(
                "SELECT id,name,mime,length(bytes) FROM attachments WHERE room=?1 ORDER BY name,id",
            )
            .map_err(database)?;
        let rows=q.query_map([room],|r|Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"mime":r.get::<_,String>(2)?,"size":r.get::<_,i64>(3)?}))).map_err(database)?;
        Ok(
            json!({"ok":true,"attachments":rows.collect::<rusqlite::Result<Vec<_>>>().map_err(database)?}),
        )
    }
    pub fn members(&self, room: &str, secret: &str) -> ApiResult<Value> {
        let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let auth = s.authenticate(room, secret)?;
        if auth.role != Role::Owner {
            return Err(err(403, "collaboration_forbidden"));
        }
        let mut q = s
            .db
            .prepare("SELECT id,name,role FROM members WHERE room=?1 AND revoked=0 ORDER BY id")
            .map_err(database)?;
        let rows=q.query_map([room],|r|Ok(json!({"member":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"role":r.get::<_,String>(2)?}))).map_err(database)?;
        Ok(
            json!({"ok":true,"members":rows.collect::<rusqlite::Result<Vec<_>>>().map_err(database)?}),
        )
    }
    pub fn close(&self, room: &str, secret: &str) -> ApiResult<()> {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if s.authenticate(room, secret)?.role != Role::Owner {
            return Err(err(403, "collaboration_forbidden"));
        }
        let tx = s.db.transaction().map_err(database)?;
        tx.execute("UPDATE rooms SET closed=1 WHERE id=?1", [room])
            .map_err(database)?;
        tx.execute(
            "INSERT OR REPLACE INTO retention(room,closed_at) VALUES(?1,?2)",
            params![room, now()],
        )
        .map_err(database)?;
        tx.commit().map_err(database)?;
        s.rooms.remove(room);
        s.presence.remove(room);
        prune(&mut s)
    }
}

fn backup_connection(db: &Connection, destination: &Path) -> ApiResult<()> {
    if destination.exists() { return Err(err(409, "collaboration_backup_exists")); }
    let parent = destination.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let temporary = tempfile::Builder::new().prefix(".readmd-session-backup-").tempfile_in(parent)
        .map_err(|_| err(500, "collaboration_backup_failed"))?.into_temp_path();
    std::fs::remove_file(&temporary).map_err(|_| err(500, "collaboration_backup_failed"))?;
    let name = temporary.to_str().ok_or_else(|| err(422, "collaboration_backup_path"))?;
    db.execute("VACUUM INTO ?1", [name]).map_err(database)?;
    std::fs::OpenOptions::new().read(true).write(true).open(&temporary).and_then(|file| file.sync_all()).map_err(|_| err(500, "collaboration_backup_failed"))?;
    temporary.persist_noclobber(destination).map_err(|_| err(409, "collaboration_backup_exists"))?;
    Ok(())
}

fn prune(state: &mut State) -> ApiResult<()> {
    let mut query=state.db.prepare("SELECT room FROM retention WHERE closed_at<?1 OR room IN(SELECT room FROM retention ORDER BY closed_at DESC,room LIMIT -1 OFFSET 64)").map_err(database)?;
    let ids = query
        .query_map([now() - 30 * 24 * 3600], |r| r.get::<_, String>(0))
        .map_err(database)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(database)?;
    drop(query);
    if ids.is_empty() {
        return Ok(());
    }
    let tx = state.db.transaction().map_err(database)?;
    for room in ids {
        for table in ["updates", "reviews", "attachments", "members", "retention"] {
            tx.execute(&format!("DELETE FROM {table} WHERE room=?1"), [&room])
                .map_err(database)?;
        }
        tx.execute("DELETE FROM rooms WHERE id=?1 AND closed=1", [&room])
            .map_err(database)?;
    }
    tx.commit().map_err(database)?;
    // Incremental vacuum applies to new databases; older profiles retain their
    // SQLite format and can reuse freed pages without a blocking full VACUUM.
    state
        .db
        .execute_batch("PRAGMA incremental_vacuum(256);")
        .map_err(database)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backup_restores_committed_wal_reviews_and_attachments_without_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let service = Service::open(&dir.path().join("source.sqlite")).unwrap();
        let owner = service.create("Evidence.md", "# Evidence\n", "Author").unwrap();
        let client = initial(&service, &owner);
        let sv = client.transact().state_vector();
        client.get_or_insert_text("content").insert(&mut client.transact_mut(), 11, "New text 🦀");
        send(&service, &owner, &client, &sv).unwrap();
        service.review(&owner.room, &owner.token, json!({"text":"Check the sources"})).unwrap();
        let asset = service.attachment(&owner.room, &owner.token, "figure.svg", "image/svg+xml", b"<svg/>").unwrap();
        let backup = dir.path().join("backup.sqlite");
        service.backup(&backup).unwrap();
        let original = std::fs::read(&backup).unwrap();
        assert_eq!(service.backup(&backup).unwrap_err().status, 409);
        assert_eq!(std::fs::read(&backup).unwrap(), original);
        let restored = dir.path().join("restored.sqlite");
        Service::restore_backup(&backup, &restored).unwrap();
        let again = Service::open(&restored).unwrap();
        assert!(again.snapshot(&owner.room, &owner.token).unwrap()["content"].as_str().unwrap().contains("New text 🦀"));
        assert_eq!(again.read_attachment(&owner.room, &owner.token, &asset).unwrap(), b"<svg/>");
        assert_eq!(again.reviews(&owner.room, &owner.token).unwrap()["reviews"][0]["body"]["text"], "Check the sources");
        assert_eq!(Service::restore_backup(&backup, &restored).unwrap_err().status, 409);
        let unrelated = dir.path().join("unrelated.sqlite");
        Connection::open(&unrelated).unwrap().execute_batch("CREATE TABLE other(id INTEGER)").unwrap();
        assert!(Service::restore_backup(&unrelated, &dir.path().join("invalid.sqlite")).is_err());
        assert!(!dir.path().join("invalid.sqlite").exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().filter_map(Result::ok).filter(|e|e.file_name().to_string_lossy().starts_with(".readmd-session-backup-")).count(),0);
    }
    fn workspace() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("readmd-crdt-{}", id()));
        std::fs::create_dir(&p).unwrap();
        p
    }
    fn initial(service: &Service, c: &Credential) -> Doc {
        let d = fresh();
        let r = service
            .sync(&c.room, &c.token, &STANDARD.encode([0]), None, None)
            .unwrap();
        d.transact_mut()
            .apply_update(
                Update::decode_v1(&STANDARD.decode(r["update"].as_str().unwrap()).unwrap())
                    .unwrap(),
            )
            .unwrap();
        d
    }
    fn send(service: &Service, c: &Credential, d: &Doc, sv: &StateVector) -> ApiResult<Value> {
        service.sync(
            &c.room,
            &c.token,
            &STANDARD.encode(d.transact().state_vector().encode_v1()),
            Some(&STANDARD.encode(d.transact().encode_state_as_update_v1(sv))),
            None,
        )
    }
    #[test]
    fn fifty_offline_clients_converge_duplicate_updates_and_restart_durably() {
        let p = workspace();
        let path = p.join("collaboration.sqlite");
        let s = Service::open(&path).unwrap();
        let owner = s.create("Research", "Original 🦀", "Host").unwrap();
        let mut clients = Vec::new();
        for i in 0..50 {
            let c = s
                .invite(
                    &owner.room,
                    &owner.token,
                    Role::Editor,
                    &format!("Writer {i}"),
                )
                .unwrap();
            let d = initial(&s, &c);
            let sv = d.transact().state_vector();
            d.get_or_insert_text("content")
                .insert(&mut d.transact_mut(), 0, &format!("[{i}]"));
            clients.push((c, d, sv));
        }
        for (c, d, sv) in clients.iter().rev() {
            send(&s, c, d, sv).unwrap();
            send(&s, c, d, sv).unwrap();
        }
        let canonical = s.snapshot(&owner.room, &owner.token).unwrap()["content"]
            .as_str()
            .unwrap()
            .to_string();
        for (c, d, _) in &clients {
            let r = s
                .sync(
                    &c.room,
                    &c.token,
                    &STANDARD.encode(d.transact().state_vector().encode_v1()),
                    None,
                    None,
                )
                .unwrap();
            d.transact_mut()
                .apply_update(
                    Update::decode_v1(&STANDARD.decode(r["update"].as_str().unwrap()).unwrap())
                        .unwrap(),
                )
                .unwrap();
            assert_eq!(content(d), canonical);
        }
        drop(s);
        let s = Service::open(&path).unwrap();
        assert_eq!(
            s.snapshot(&owner.room, &owner.token).unwrap()["content"],
            canonical
        );
        assert!(canonical.ends_with("Original 🦀"));
        for i in 0..50 {
            assert!(canonical.contains(&format!("[{i}]")));
        }
        drop(s);
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn permissions_reviews_attachment_isolation_and_stale_changes_are_enforced() {
        let p = workspace();
        let s = Service::open(&p.join("db")).unwrap();
        let owner = s.create("Title", "abc", "Owner").unwrap();
        let reader = s
            .invite(&owner.room, &owner.token, Role::Reader, "Reader")
            .unwrap();
        let reviewer = s
            .invite(&owner.room, &owner.token, Role::Reviewer, "Reviewer")
            .unwrap();
        let editor = s
            .invite(&owner.room, &owner.token, Role::Editor, "Editor")
            .unwrap();
        let d = initial(&s, &editor);
        let sv = d.transact().state_vector();
        {
            let text = d.get_or_insert_text("content");
            let mut tx = d.transact_mut();
            text.remove_range(&mut tx, 1, 1);
            text.insert(&mut tx, 1, "B");
            text.insert(&mut tx, 3, "!");
        }
        assert_eq!(send(&s, &reader, &d, &sv).unwrap_err().status, 403);
        assert_eq!(
            s.review(&reader.room, &reader.token, json!({"text":"No"}))
                .unwrap_err()
                .status,
            403
        );
        let review=s.review(&reviewer.room,&reviewer.token,json!({"text":"Replace b","transaction":{"base_revision":document_transaction::revision("abc"),"origin":"review","edits":[{"from":1,"to":2,"insert":"β"}]}})).unwrap();
        send(&s, &editor, &d, &sv).unwrap();
        assert_eq!(
            s.resolve(&owner.room, &owner.token, &review, true)
                .unwrap_err()
                .status,
            409
        );
        assert_eq!(
            s.reviews(&owner.room, &owner.token).unwrap()["reviews"][0]["status"],
            "open"
        );
        let asset = s
            .attachment(
                &owner.room,
                &owner.token,
                "image.svg",
                "image/svg+xml",
                b"<svg/>",
            )
            .unwrap();
        assert_eq!(
            s.read_attachment(&reader.room, &reader.token, &asset)
                .unwrap(),
            b"<svg/>"
        );
        let other = s.create("Other", "", "Owner").unwrap();
        assert_eq!(
            s.read_attachment(&other.room, &other.token, &asset)
                .unwrap_err()
                .status,
            404
        );
        s.permission(&owner.room, &owner.token, &editor.member, None)
            .unwrap();
        assert_eq!(send(&s, &editor, &d, &sv).unwrap_err().status, 403);
        s.resolve(&owner.room, &owner.token, &review, false)
            .unwrap();
        assert_eq!(
            s.snapshot(&owner.room, &owner.token).unwrap()["content"],
            "aBc!"
        );
        s.close(&owner.room, &owner.token).unwrap();
        assert_eq!(
            s.snapshot(&reader.room, &reader.token).unwrap_err().status,
            403
        );
        drop(s);
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn suggestions_follow_unrelated_changes_and_survive_restart_without_replacing_neighbors() {
        let p = workspace();
        let path = p.join("db");
        let s = Service::open(&path).unwrap();
        let owner = s.create("Math", "α🦀 + x", "Owner").unwrap();
        let reviewer = s
            .invite(&owner.room, &owner.token, Role::Reviewer, "Reviewer")
            .unwrap();
        let review=s.review(&owner.room,&reviewer.token,json!({"text":"Use y","transaction":{"base_revision":document_transaction::revision("α🦀 + x"),"origin":"review","edits":[{"from":6,"to":7,"insert":"y"}]}})).unwrap();
        let d = initial(&s, &owner);
        let sv = d.transact().state_vector();
        d.get_or_insert_text("content")
            .insert(&mut d.transact_mut(), 0, "Note: ");
        send(&s, &owner, &d, &sv).unwrap();
        drop(s);
        let s = Service::open(&path).unwrap();
        s.resolve(&owner.room, &owner.token, &review, true).unwrap();
        assert_eq!(
            s.snapshot(&owner.room, &owner.token).unwrap()["content"],
            "Note: α🦀 + y"
        );
        assert_eq!(
            s.reviews(&owner.room, &owner.token).unwrap()["reviews"][0]["status"],
            "accepted"
        );
        assert_eq!(s.review(&owner.room,&reviewer.token,json!({"text":"Stale","transaction":{"base_revision":"old","origin":"review","edits":[]}})).unwrap_err().status,409);
        let asset = s
            .attachment(
                &owner.room,
                &owner.token,
                "图.svg",
                "image/svg+xml",
                b"<svg/>",
            )
            .unwrap();
        assert_eq!(
            asset,
            s.attachment(
                &owner.room,
                &owner.token,
                "duplicate.svg",
                "image/svg+xml",
                b"<svg/>"
            )
            .unwrap()
        );
        assert_eq!(
            s.attachments(&owner.room, &reviewer.token).unwrap()["attachments"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            s.members(&owner.room, &reviewer.token).unwrap_err().status,
            403
        );
        drop(s);
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn concurrent_three_ten_and_fifty_writers_and_out_of_order_compaction_are_durable() {
        for count in [3, 10, 50] {
            let p = workspace();
            let path = p.join("db");
            let service = std::sync::Arc::new(Service::open(&path).unwrap());
            let owner = service.create("Concurrent", "Original", "Owner").unwrap();
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(count));
            let mut handles = Vec::new();
            for index in 0..count {
                let credential = service
                    .invite(
                        &owner.room,
                        &owner.token,
                        Role::Editor,
                        &format!("Writer {index}"),
                    )
                    .unwrap();
                let doc = initial(&service, &credential);
                let vector = doc.transact().state_vector();
                let service = service.clone();
                let barrier = barrier.clone();
                handles.push(std::thread::spawn(move || {
                    barrier.wait();
                    doc.get_or_insert_text("content").insert(
                        &mut doc.transact_mut(),
                        0,
                        &format!("[{index}]"),
                    );
                    send(&service, &credential, &doc, &vector).unwrap();
                }));
            }
            for handle in handles {
                handle.join().unwrap();
            }
            let canonical = service.snapshot(&owner.room, &owner.token).unwrap()["content"]
                .as_str()
                .unwrap()
                .to_string();
            for index in 0..count {
                assert!(canonical.contains(&format!("[{index}]")));
            }
            // Send a dependent second update first, force compaction, restart,
            // then deliver the missing first update. Pending data must survive.
            let doc = initial(&service, &owner);
            let text = doc.get_or_insert_text("content");
            let v0 = doc.transact().state_vector();
            text.insert(&mut doc.transact_mut(), 0, "A");
            let first = doc.transact().encode_state_as_update_v1(&v0);
            let v1 = doc.transact().state_vector();
            text.insert(&mut doc.transact_mut(), 1, "B");
            let second = doc.transact().encode_state_as_update_v1(&v1);
            for _ in 0..130 {
                service
                    .sync(
                        &owner.room,
                        &owner.token,
                        &STANDARD.encode([0]),
                        Some(&STANDARD.encode(&second)),
                        None,
                    )
                    .unwrap();
            }
            drop(service);
            let service = Service::open(&path).unwrap();
            service
                .sync(
                    &owner.room,
                    &owner.token,
                    &STANDARD.encode([0]),
                    Some(&STANDARD.encode(&first)),
                    None,
                )
                .unwrap();
            assert_eq!(
                service.snapshot(&owner.room, &owner.token).unwrap()["content"],
                format!("AB{canonical}")
            );
            drop(service);
            std::fs::remove_dir_all(p).unwrap();
        }
    }
}
