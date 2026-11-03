//! Resource-safe compensations, derived from exact reachable history states.
//! Original events remain immutable; an exact-state projection accompanies
//! each still-reachable action when the trusted save adapter moves resources.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct CompensationProjection {
    content_revision: String,
    target_revision: String,
    changes_revision: String,
    inverse: Vec<TextEdit>,
    inverse_expected: Vec<String>,
    changes: Vec<Change>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct HistoryProjection {
    redo: bool,
    event_id: u64,
    projection: CompensationProjection,
}
pub(crate) struct ResourceState {
    pub content: String,
    pub steps: Vec<Vec<TextEdit>>,
    pub restorations: Vec<Value>,
}
// Bound the expanded planning representation too. Deduplicated ReviewText
// references in SQLite can otherwise expand many times across history frames
// before the persistent store's completion budget gets a chance to reject it.
struct PlanBudget(usize);
impl std::io::Write for PlanBudget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_sub(bytes.len())
            .ok_or_else(|| std::io::Error::other("history budget"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl PlanBudget {
    fn charge(&mut self, value: &impl Serialize) -> Result<(), &'static str> {
        serde_json::to_writer(self, value).map_err(|_| "review_budget")
    }
    fn changes(&mut self, changes: &[Change]) -> Result<(), &'static str> {
        for change in changes {
            // Metadata plus every expanded immutable text, without first
            // allocating a public_changes array or escaped JSON string.
            self.charge(change)?;
            for text in std::iter::once(&change.original)
                .chain(std::iter::once(&change.replacement))
                .chain(change.restore_original.iter())
                .chain(&change.candidate_versions)
            {
                self.charge(&text.as_str())?;
            }
        }
        Ok(())
    }
}
fn frame(document: &Document) -> Document {
    Document {
        document_id: document.document_id.clone(),
        path: document.path.clone(),
        content: document.content.clone(),
        generation: document.generation,
        revision: document.revision.clone(),
        review_revision: document.review_revision,
        disk_revision: document.disk_revision.clone(),
        encoding: document.encoding.clone(),
        line_ending: document.line_ending.clone(),
        changes: document.changes.clone(),
        last_event_id: document.last_event_id,
        undo: Vec::new(),
        redo: Vec::new(),
        mappings: Vec::new(),
        save_intent: None,
        rename_intent: None,
        recovery: None,
        checkpoint: Checkpoint {
            content: String::new(),
            changes: Vec::new(),
            event_id: document.last_event_id,
        },
    }
}

fn changes_revision(changes: &[Change]) -> String {
    let mut values = public_changes(changes, "");
    values.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    for value in &mut values {
        if let Some(object) = value.as_object_mut() {
            for key in ["version", "last_event_id", "candidate_versions", "current"] {
                object.remove(key);
            }
        }
    }
    document_transaction::revision(&canonical(&json!(values)).to_string())
}
fn project_state(document: &Document, plan: ResourceState) -> Result<Document, &'static str> {
    let mut projected = frame(document);
    for step in &plan.steps {
        let result = apply(&projected.content, step)?;
        map_changes(&mut projected, step, &result.content, true)?;
        projected.content = result.content;
    }
    if projected.content != plan.content {
        return Err("review_asset_relocation_required");
    }
    for value in plan.restorations {
        let change = projected
            .changes
            .iter_mut()
            .find(|c| Some(c.id.as_str()) == value["id"].as_str())
            .ok_or("review_not_found")?;
        if value["from"].as_u64() != Some(change.from as u64)
            || value["to"].as_u64() != Some(change.to as u64)
        {
            return Err("review_save_conflict");
        }
        change.restore_original = Some(ReviewText::new(string(&value, "content")?.into()));
    }
    Ok(projected)
}

/// Transform only the known operation's envelopes. Resource rewrites can
/// enlarge several URL fragments to one address, so merge matching envelopes
/// on both sides before checking every untouched gap byte-for-byte.
fn projected_edits(
    before: &str,
    after: &str,
    edits: &[TextEdit],
    left: &[Vec<TextEdit>],
    right: &[Vec<TextEdit>],
    new_before: &str,
    new_after: &str,
) -> Result<Vec<TextEdit>, &'static str> {
    if apply(before, edits)?.content != after {
        return Err("review_conflict");
    }
    let mut spans = Vec::<(usize, usize, usize, usize)>::new();
    let mut delta = 0isize;
    for edit in sorted(edits) {
        let (mut a, mut b) = (edit.from, edit.to);
        let mut c = shifted(edit.from, delta);
        let mut d = c + edit.insert.encode_utf16().count();
        for step in left {
            map_geometry(&mut a, &mut b, step);
        }
        for step in right {
            map_geometry(&mut c, &mut d, step);
        }
        if let Some(last) = spans.last_mut().filter(|last| a <= last.1 || c <= last.3) {
            last.0 = last.0.min(a);
            last.1 = last.1.max(b);
            last.2 = last.2.min(c);
            last.3 = last.3.max(d);
        } else {
            spans.push((a, b, c, d));
        }
        delta += edit.insert.encode_utf16().count() as isize - (edit.to - edit.from) as isize;
    }
    let (mut a_end, mut b_end) = (0, 0);
    let mut result = Vec::new();
    for (a, b, c, d) in spans {
        if slice(new_before, a_end, a)? != slice(new_after, b_end, c)? {
            return Err("review_asset_relocation_required");
        }
        let insert = slice(new_after, c, d)?;
        if slice(new_before, a, b)? != insert {
            result.push(TextEdit {
                from: a,
                to: b,
                insert: insert.into(),
            });
        }
        a_end = b;
        b_end = d;
    }
    if slice(new_before, a_end, new_before.encode_utf16().count())?
        != slice(new_after, b_end, new_after.encode_utf16().count())?
        || apply(new_before, &result)?.content != new_after
    {
        return Err("review_asset_relocation_required");
    }
    Ok(result)
}

impl Service {
    pub(crate) fn resource_save_history(
        &self,
        source: &Value,
        mut project: impl FnMut(&Value) -> Result<ResourceState, &'static str>,
    ) -> Result<Value, &'static str> {
        self.store.run(|tx| {
            with_text_context(tx, || {
                let original = load(tx, string(source, "document_id")?)?;
                if source["generation"].as_u64() != Some(original.generation)
                    || source["review_revision"].as_u64() != Some(original.review_revision)
                {
                    return Err("review_stale_generation");
                }
                // If neither the current body nor any retained operation can
                // contain an image/math opener, a reachable state cannot invent
                // one. Keep ordinary text Save As independent of history length.
                let marker = |text: &str| text.contains(['!', '<']);
                let changes = |values: &[Change]| {
                    values.iter().any(|c| {
                        marker(c.original.as_str())
                            || marker(c.replacement.as_str())
                            || c.restore_original
                                .as_ref()
                                .is_some_and(|t| marker(t.as_str()))
                    })
                };
                if !marker(&original.content)
                    && !changes(&original.changes)
                    && !original.undo.iter().chain(&original.redo).any(|a| {
                        a.resource_projection.is_some()
                            || a.edits.iter().chain(&a.inverse).any(|e| marker(&e.insert))
                            || a.patches.iter().any(|p| {
                                p.before
                                    .iter()
                                    .chain(p.after.iter())
                                    .any(|c| changes(std::slice::from_ref(c)))
                            })
                    })
                {
                    return Ok(json!([]));
                }
                collect(tx, &original, |state, _| project(state))
            })
        })
    }
}
fn collect(
    tx: &Transaction<'_>,
    original: &Document,
    mut project: impl FnMut(&Value, &[Mapping]) -> Result<ResourceState, &'static str>,
) -> Result<Value, &'static str> {
    if original.undo.len() + original.redo.len() > MAX_OPERATIONS {
        return Err("review_budget");
    }
    struct RestorePersisted(HashSet<String>);
    impl Drop for RestorePersisted {
        fn drop(&mut self) {
            PERSISTED_TEXTS.with(|slot| {
                slot.replace(std::mem::take(&mut self.0));
            });
        }
    }
    let _persisted = RestorePersisted(PERSISTED_TEXTS.with(|slot| slot.borrow().clone()));
    // Probe the real compensator, then roll back every temporary event.
    // Nothing here advances the durable document, receipts or counters.
    tx.execute_batch("SAVEPOINT resource_history_probe")
        .map_err(|_| "review_persistence_failed")?;
    let planned = (|| {
        let mut plans = Vec::new();
        let mut budget = PlanBudget(review_store::DEFAULT_STORE_BYTES);
        for redo in [false, true] {
            let mut current = original.clone();
            let first_mapping = current.mappings.len();
            loop {
                if crate::speech::cancelled() {
                    return Err("cancelled");
                }
                let stack = if redo { &current.redo } else { &current.undo };
                let Some(target) = stack.last() else { break };
                let event_id = target.event_id;
                let before = frame(&current);
                PlanBudget(review_store::DEFAULT_STORE_BYTES).changes(&before.changes)?;
                let left = project(&snapshot(&before), &current.mappings[first_mapping..])?;
                let left_steps = left.steps.clone();
                compensate(tx, &mut current, redo)?;
                PlanBudget(review_store::DEFAULT_STORE_BYTES).changes(&current.changes)?;
                let edits = current
                    .mappings
                    .last()
                    .ok_or("review_conflict")?
                    .edits
                    .clone();
                let right = project(&snapshot(&current), &current.mappings[first_mapping..])?;
                let right_steps = right.steps.clone();
                let from = project_state(&before, left)?;
                let to = project_state(&current, right)?;
                let inverse = projected_edits(
                    &before.content,
                    &current.content,
                    &edits,
                    &left_steps,
                    &right_steps,
                    &from.content,
                    &to.content,
                )?;
                let expected = inverse
                    .iter()
                    .map(|e| slice(&from.content, e.from, e.to).map(str::to_string))
                    .collect::<Result<Vec<_>, _>>()?;
                budget.charge(&inverse)?;
                budget.charge(&expected)?;
                budget.changes(&to.changes)?;
                budget.charge(&"x".repeat(512))?; // fixed projection envelope
                                                  // Carry plain original text across this read boundary;
                                                  // temporary probe text references will be rolled back.
                plans.push(json!({"redo":redo,"event_id":event_id,"projection":{
                            "content_revision":document_transaction::revision(&from.content),"target_revision":document_transaction::revision(&to.content),"changes_revision":changes_revision(&from.changes),
                            "inverse":inverse,"inverse_expected":expected,"changes":public_changes(&to.changes,"")}}));
            }
        }
        Ok(json!(plans))
    })();
    tx.execute_batch("ROLLBACK TO resource_history_probe; RELEASE resource_history_probe")
        .map_err(|_| "review_persistence_failed")?;
    planned
}

pub(super) fn for_refactor(
    tx: &Transaction<'_>,
    document: &Document,
    edits: &[TextEdit],
) -> Result<Vec<HistoryProjection>, &'static str> {
    if edits.is_empty()
        || !document
            .undo
            .iter()
            .chain(&document.redo)
            .any(|a| a.resource_projection.is_some())
    {
        return Ok(Vec::new());
    }
    let expected = edits
        .iter()
        .map(|e| slice(&document.content, e.from, e.to).map(str::to_string))
        .collect::<Result<Vec<_>, _>>()?;
    let plans = collect(tx, document, |state, mappings| {
        let content = string(state, "content")?;
        let mut mapped = edits.to_vec();
        for edit in &mut mapped {
            for mapping in mappings {
                map_geometry(&mut edit.from, &mut edit.to, &mapping.edits);
            }
        }
        for (edit, expected) in mapped.iter().zip(&expected) {
            if slice(content, edit.from, edit.to)? != expected {
                return Err("refactor_undo_overlap");
            }
        }
        Ok(ResourceState {
            content: apply(content, &mapped)?.content,
            steps: vec![mapped],
            restorations: Vec::new(),
        })
    })?;
    let plans = parse(document, Some(&plans))?;
    validate_head(
        document,
        &plans,
        &apply(&document.content, edits)?.content,
        &[edits.to_vec()],
        &[],
    )?;
    Ok(plans)
}

pub(super) fn parse(
    document: &Document,
    value: Option<&Value>,
) -> Result<Vec<HistoryProjection>, &'static str> {
    let plans: Vec<HistoryProjection> = value
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()
        .map_err(|_| "review_invalid_request")?
        .unwrap_or_default();
    if plans.is_empty() {
        return Ok(plans);
    }
    let expected = document
        .undo
        .iter()
        .map(|a| (false, a.event_id))
        .chain(document.redo.iter().map(|a| (true, a.event_id)))
        .collect::<BTreeSet<_>>();
    if plans.len() > MAX_OPERATIONS
        || plans.len() != expected.len()
        || plans
            .iter()
            .map(|p| (p.redo, p.event_id))
            .collect::<BTreeSet<_>>()
            != expected
    {
        return Err("review_save_conflict");
    }
    let known = document
        .changes
        .iter()
        .chain(document.undo.iter().chain(&document.redo).flat_map(|a| {
            a.patches
                .iter()
                .flat_map(|p| p.before.iter().chain(p.after.iter()))
        }))
        .chain(
            document
                .undo
                .iter()
                .chain(&document.redo)
                .filter_map(|a| a.resource_projection.as_ref())
                .flat_map(|p| p.changes.iter()),
        )
        .map(|c| (c.id.as_str(), c))
        .collect::<HashMap<_, _>>();
    for plan in &plans {
        if ![
            &plan.projection.content_revision,
            &plan.projection.target_revision,
            &plan.projection.changes_revision,
        ]
        .iter()
        .all(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
            || plan.projection.inverse.len() != plan.projection.inverse_expected.len()
        {
            return Err("review_invalid_request");
        }
        for edit in &plan.projection.inverse {
            checked_content(&edit.insert)?;
        }
        for change in &plan.projection.changes {
            if change.from > change.to {
                return Err("review_invalid_request");
            }
            let old = known.get(change.id.as_str()).ok_or("review_not_found")?;
            if old.original != change.original
                || old.origin != change.origin
                || old.task_id != change.task_id
                || old.dependencies != change.dependencies
            {
                return Err("review_save_conflict");
            }
        }
    }
    for (redo, stack) in [(false, &document.undo), (true, &document.redo)] {
        let mut previous: Option<&CompensationProjection> = None;
        for action in stack.iter().rev() {
            let current = &plans
                .iter()
                .find(|p| p.redo == redo && p.event_id == action.event_id)
                .ok_or("review_save_conflict")?
                .projection;
            if let Some(before) = previous {
                if before.target_revision != current.content_revision
                    || changes_revision(&before.changes) != current.changes_revision
                {
                    return Err("review_save_conflict");
                }
            }
            previous = Some(current);
        }
    }
    Ok(plans)
}
pub(super) fn validate_head(
    document: &Document,
    plans: &[HistoryProjection],
    saved: &str,
    steps: &[Vec<TextEdit>],
    restores: &[RestoreProjection],
) -> Result<(), &'static str> {
    if plans.is_empty() {
        return Ok(());
    }
    let projected = project_state(
        document,
        ResourceState {
            content: saved.into(),
            steps: steps.to_vec(),
            restorations: restores
                .iter()
                .map(|r| json!({"id":r.id,"from":r.from,"to":r.to,"content":r.content}))
                .collect(),
        },
    )?;
    for (redo, stack) in [(false, &document.undo), (true, &document.redo)] {
        let mut content = projected.content.clone();
        let mut changes = projected.changes.clone();
        for action in stack.iter().rev() {
            if crate::speech::cancelled() {
                return Err("cancelled");
            }
            let proof = &plans
                .iter()
                .find(|p| p.redo == redo && p.event_id == action.event_id)
                .ok_or("review_save_conflict")?
                .projection;
            if proof.content_revision != document_transaction::revision(&content)
                || proof.changes_revision != changes_revision(&changes)
            {
                return Err("review_save_conflict");
            }
            for (edit, expected) in proof.inverse.iter().zip(&proof.inverse_expected) {
                if slice(&content, edit.from, edit.to)? != expected {
                    return Err("review_save_conflict");
                }
            }
            let next = apply(&content, &proof.inverse)?;
            if next.revision != proof.target_revision {
                return Err("review_save_conflict");
            }
            for change in &proof.changes {
                slice(&next.content, change.from, change.to)?;
            }
            content = next.content;
            changes = proof.changes.clone();
        }
    }
    Ok(())
}
pub(super) fn attach(
    document: &mut Document,
    plans: &[HistoryProjection],
) -> Result<(), &'static str> {
    if !plans.is_empty() {
        let content = document_transaction::revision(&document.content);
        let changes = changes_revision(&document.changes);
        for (redo, stack) in [(false, &document.undo), (true, &document.redo)] {
            if let Some(action) = stack.last() {
                let proof = &plans
                    .iter()
                    .find(|p| p.redo == redo && p.event_id == action.event_id)
                    .ok_or("review_save_conflict")?
                    .projection;
                if proof.content_revision != content || proof.changes_revision != changes {
                    return Err("review_save_conflict");
                }
            }
        }
    }
    for plan in plans {
        let stack = if plan.redo {
            &mut document.redo
        } else {
            &mut document.undo
        };
        stack
            .iter_mut()
            .find(|a| a.event_id == plan.event_id)
            .ok_or("review_save_conflict")?
            .resource_projection = Some(plan.projection.clone());
    }
    Ok(())
}
pub(super) fn compensate_projection(
    document: &mut Document,
    projection: &CompensationProjection,
) -> Result<Vec<TextEdit>, &'static str> {
    if document.revision != projection.content_revision
        || changes_revision(&document.changes) != projection.changes_revision
    {
        return Err("review_conflict");
    }
    for (edit, expected) in projection.inverse.iter().zip(&projection.inverse_expected) {
        if slice(&document.content, edit.from, edit.to)? != expected {
            return Err("review_conflict");
        }
    }
    let result = apply(&document.content, &projection.inverse)?;
    if result.revision != projection.target_revision {
        return Err("review_conflict");
    }
    document.content = result.content;
    document.changes = projection.changes.clone();
    let version = next_counter(document.review_revision)?;
    for change in &mut document.changes {
        change.version = version;
    }
    Ok(projection.inverse.clone())
}
pub(super) fn reverse(
    document: &Document,
    content: &str,
    changes: Vec<Change>,
    action: &Action,
) -> CompensationProjection {
    CompensationProjection {
        content_revision: document.revision.clone(),
        target_revision: document_transaction::revision(content),
        changes_revision: changes_revision(&document.changes),
        inverse: action.inverse.clone(),
        inverse_expected: action.inverse_expected.clone(),
        changes,
    }
}
pub(super) fn flush(
    tx: &Transaction<'_>,
    projection: &CompensationProjection,
) -> Result<(), &'static str> {
    flush_changes(tx, &projection.changes)
}
pub(super) fn flush_plans(
    tx: &Transaction<'_>,
    plans: &[HistoryProjection],
) -> Result<(), &'static str> {
    for plan in plans {
        flush(tx, &plan.projection)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn change(service: &Service, head: &Value, action: &str, extra: Value) -> Value {
        let mut request = json!({"action":action,"document_id":head["document_id"],"generation":head["generation"],"review_revision":head["review_revision"],"call_id":Uuid::new_v4().to_string()});
        request
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        service.handle(&request).unwrap()
    }
    fn projected(state: &Value) -> Result<ResourceState, &'static str> {
        let text = string(state, "content")?;
        let edits = crate::document_assets::image_references(text)
            .into_iter()
            .map(|(range, url)| TextEdit {
                from: text[..range.start].encode_utf16().count(),
                to: text[..range.end].encode_utf16().count(),
                insert: format!("readmd-assets/{url}"),
            })
            .collect::<Vec<_>>();
        let content = apply(text, &edits)?.content;
        Ok(ResourceState {
            content,
            steps: vec![edits],
            restorations: Vec::new(),
        })
    }
    #[test]
    fn malformed_projection_chain_or_original_cannot_prepare_a_save() {
        for fault in ["none", "head", "chain", "range", "original"] {
            let root = tempfile::tempdir().unwrap();
            let service = Service::new(root.path());
            let text = "😀 Intro\n![x](old.svg)";
            let file = root.path().join("fixture.md");
            std::fs::write(&file, text).unwrap();
            let first=service.handle(&json!({"action":"open","path":file,"content":text,"disk_content":text,"disk_revision":document_transaction::revision(text)})).unwrap();
            let proposed = change(
                &service,
                &first,
                "propose",
                json!({"edits":[{"from":3,"to":8,"insert":"Title"}]}),
            );
            let body = proposed["content"].as_str().unwrap();
            let at = body.find("old.svg").unwrap();
            let from = body[..at].encode_utf16().count();
            let head = change(
                &service,
                &proposed,
                "edit",
                json!({"edits":[{"from":from,"to":from+7,"insert":"new.svg"}]}),
            );
            let mut plans = service.resource_save_history(&head, projected).unwrap();
            assert_eq!(plans.as_array().unwrap().len(), 2);
            match fault {
                "head" => plans[0]["projection"]["content_revision"] = json!("f".repeat(64)),
                "chain" => plans[1]["projection"]["content_revision"] = json!("e".repeat(64)),
                "range" => plans[0]["projection"]["inverse"][0]["from"] = json!(1),
                "original" => {
                    plans[0]["projection"]["changes"][0]["original"] = json!("forged original")
                }
                "none" => (),
                _ => unreachable!(),
            }
            let state = projected(&head).unwrap();
            let request = json!({"action":"prepare_save","document_id":head["document_id"],"generation":head["generation"],"review_revision":head["review_revision"],"call_id":Uuid::new_v4().to_string(),
                "content":head["content"],"disk_revision":head["disk_revision"],"expected_disk_revision":"expected","saved_content":state.content,
                "edit_steps":state.steps.iter().map(|edits|json!({"edits":edits})).collect::<Vec<_>>(),"history_projections":plans});
            let result = service.handle(&request);
            if fault == "none" {
                let intent =
                    result.expect("untampered independently constructed fixture must prepare");
                service.handle(&json!({"action":"abort_save","document_id":head["document_id"],"intent_id":intent["intent_id"],"call_id":Uuid::new_v4().to_string()})).unwrap();
            } else {
                assert!(result.is_err(), "{fault} must fail before any save intent");
            }
            let current = service
                .handle(&json!({"action":"read","document_id":head["document_id"]}))
                .unwrap();
            assert_eq!(current["content"], head["content"]);
            assert_eq!(current["changes"], head["changes"]);
            assert!(current["save_intent"].is_null());
        }
    }
}
