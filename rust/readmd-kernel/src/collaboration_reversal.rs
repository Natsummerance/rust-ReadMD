//! Reversing a shared acceptance is a new durable event, not a mutation of the
//! proposal or original decision. Only its exact acknowledged result is safe.
use super::*;
use crate::document_transaction::TextEdit;

pub(super) fn migrate(db: &Connection) -> ApiResult<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS review_reversals(review TEXT PRIMARY KEY REFERENCES reviews(id),room TEXT NOT NULL REFERENCES rooms(id),body TEXT NOT NULL);
        CREATE INDEX IF NOT EXISTS review_reversals_room ON review_reversals(room);").map_err(database)
}

pub(super) struct Decision {
    pub generation: i64,
    revision: String,
    before_revision: String,
    forward: ContentTransaction,
    inverse: Vec<TextEdit>,
}
fn revision_shape(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(super) fn decision(body: &Value) -> Option<Decision> {
    // Legacy migration removed untrusted _generation. Old/unknown decisions,
    // comments and no-op acceptances never acquire reversal authority.
    let proposed = body["_generation"].as_i64().filter(|g| *g >= 0)?;
    body["transaction"].as_object()?;
    let value = &body["_decision"];
    if !matches!(value["method"].as_str()?, "accept" | "manual") { return None; }
    let base = value["base_generation"].as_i64().filter(|g| *g >= proposed)?;
    let generation = value["result_generation"].as_i64().filter(|g| Some(*g) == base.checked_add(1))?;
    let revision = value["result_revision"].as_str().filter(|s| revision_shape(s))?.to_string();
    let before_revision = value["base_revision"].as_str().filter(|s| revision_shape(s) && *s != revision.as_str())?.to_string();
    let forward: ContentTransaction = serde_json::from_value(value["transaction"].clone()).ok()?;
    let inverse: Vec<TextEdit> = serde_json::from_value(value["inverse"].clone()).ok()?;
    if forward.origin != "review" || forward.base_revision != before_revision || forward.edits.is_empty() || inverse.is_empty()
        || forward.edits.len() > document_transaction::MAX_OPERATIONS || inverse.len() > document_transaction::MAX_OPERATIONS { return None; }
    Some(Decision { generation, revision, before_revision, forward, inverse })
}

impl Service {
    pub fn reverse_review(&self, room: &str, secret: &str, review: &str, generation: i64, revision: &str) -> ApiResult<Value> {
        if generation < 0 || !revision_shape(revision) { return Err(invalid()); }
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let auth = s.authenticate(room, secret)?;
        if !auth.role.edits() { return Err(err(403, "collaboration_forbidden")); }
        let (raw, status): (String, String) = s.db.query_row("SELECT body,status FROM reviews WHERE id=?1 AND room=?2",params![review,room],|r|Ok((r.get(0)?,r.get(1)?)))
            .optional().map_err(database)?.ok_or_else(||err(404,"collaboration_not_found"))?;
        let decision_revision = document_transaction::revision(&raw);
        let previous: Option<String> = s.db.query_row("SELECT body FROM review_reversals WHERE review=?1 AND room=?2",params![review,room],|r|r.get(0)).optional().map_err(database)?;
        if let Some(previous) = previous {
            let receipt: Value = serde_json::from_str(&previous).map_err(|_|invalid())?;
            // A lost response may be retried even after subsequent edits. The
            // immutable receipt acknowledges this exact action, never reapplies.
            if receipt["member"] == auth.member && receipt["base_generation"] == generation && receipt["base_revision"] == revision && receipt["decision_revision"] == decision_revision {
                return Ok(json!({"ok":true,"reversal":receipt,"replayed":true}));
            }
            return Err(err(409,"collaboration_conflict"));
        }
        let body: Value = serde_json::from_str(&raw).map_err(|_|invalid())?;
        let decision = decision(&body).filter(|_|status == "accepted").ok_or_else(||err(409,"collaboration_conflict"))?;
        s.load(room)?;
        let source = content(&s.rooms[room]);
        if generation != decision.generation || generation != review_generation(&s.db,room)? || revision != decision.revision || revision != document_transaction::revision(&source) {
            return Err(err(409,"collaboration_conflict"));
        }
        let inverse = ContentTransaction { base_revision: revision.into(), origin: "review".into(), edits: decision.inverse };
        let result = document_transaction::apply(&source,&inverse).map_err(|_|err(409,"collaboration_conflict"))?;
        if !result.changed || result.revision != decision.before_revision || document_transaction::apply(&result.content,&decision.forward).map_or(true,|r|r.content != source) {
            return Err(err(409,"collaboration_conflict"));
        }
        if result.content.len() > MAX_DOCUMENT { return Err(err(413,"collaboration_budget")); }
        let doc = fresh();
        doc.transact_mut().apply_update(Update::decode_v1(&full(&s.rooms[room])).map_err(|_|invalid())?).map_err(|_|invalid())?;
        let vector = doc.transact().state_vector();
        let text = doc.get_or_insert_text("content");
        let mut edits = inverse.edits;
        edits.sort_by_key(|e|std::cmp::Reverse(e.from));
        {
            let mut txn = doc.transact_mut();
            for edit in edits {
                if edit.to > edit.from { text.remove_range(&mut txn,edit.from as u32,(edit.to-edit.from) as u32); }
                if !edit.insert.is_empty() { text.insert(&mut txn,edit.from as u32,&edit.insert); }
            }
        }
        let bytes = doc.transact().encode_state_as_update_v1(&vector);
        if bytes.len() > MAX_UPDATE || full(&doc).len() > MAX_DOCUMENT { return Err(err(413,"collaboration_budget")); }
        let dbtx = s.db.transaction().map_err(database)?;
        dbtx.execute("INSERT INTO updates(room,member,bytes,created) VALUES(?1,?2,?3,?4)",params![room,auth.member,bytes,now()]).map_err(database)?;
        let receipt = json!({"member":auth.member,"name":auth.name,"created":now(),"base_generation":generation,"base_revision":revision,"result_generation":review_generation(&dbtx,room)?,"result_revision":result.revision,"decision_revision":decision_revision});
        review_body_budget(&dbtx,room,"",&receipt)?;
        dbtx.execute("INSERT INTO review_reversals(review,room,body) VALUES(?1,?2,?3)",params![review,room,receipt.to_string()]).map_err(database)?;
        dbtx.commit().map_err(database)?;
        s.rooms.insert(room.into(),doc);
        Ok(json!({"ok":true,"reversal":receipt,"replayed":false}))
    }
}

#[cfg(test)]
#[path = "collaboration_reversal_tests.rs"]
mod tests;
