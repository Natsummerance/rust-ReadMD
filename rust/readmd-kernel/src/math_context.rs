//! Bounded, process-local handles for immutable, source-verified math contexts.
use crate::{document_transaction as tx, math_document::{self, MathDocument}, math_source_context::{self, SourceAnalysis}};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::VecDeque, sync::{Arc, Mutex}, time::{Duration, Instant}};

const MAX_CONTEXTS: usize = 8;
const MAX_BYTES: usize = 64 * 1024 * 1024;
const TTL: Duration = Duration::from_secs(300);
const CONFLICT: &str = "transaction_conflict";

#[derive(Default)]
pub(crate) struct Runtime { state: Mutex<State> }
#[derive(Default)]
struct State { entries: VecDeque<Arc<Entry>>, bytes: usize }
struct Entry {
    id: String, nonce: String, revision: String, source: String, analysis: SourceAnalysis,
    charge: usize, expires: Instant,
}
#[derive(Clone)]
pub(crate) struct Binding { entry: Arc<Entry>, ordinal: usize }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selector { id: String, revision: String, fingerprint: String, from: usize, to: usize }

impl State {
    fn expire(&mut self, now: Instant) {
        // Expired handles are unusable, but in-flight references still count
        // against the cache budget until their render finishes.
        self.entries.retain(|entry| entry.expires > now || Arc::strong_count(entry) > 1);
        self.bytes = self.entries.iter().map(|entry| entry.charge).sum();
    }
}

impl Runtime {
    pub(crate) fn register(&self, source: &str, expected: &str) -> Result<Value, String> {
        self.register_view(source,expected,None)
    }
    pub(crate) fn register_view(&self, source: &str, expected: &str, view: Option<&str>) -> Result<Value, String> {
        if source.len() > tx::MAX_DOCUMENT_BYTES { return Err("math_budget".into()); }
        if tx::revision(source) != expected { return Err(CONFLICT.into()); }
        let analysis = math_source_context::analyze_source(source)?;
        let mut spans=Vec::new();
        if let Some(view)=view {
            if view.len()>tx::MAX_DOCUMENT_BYTES {return Err("math_budget".into());}
            let other=if view==source {None} else {Some(math_source_context::analyze_source(view)?)};
            let projected=other.as_ref().unwrap_or(&analysis);
            if projected.occurrences.len()!=analysis.occurrences.len() {return Err("math_unsupported_extension".into());}
            for (original,projected) in analysis.occurrences.iter().zip(&projected.occurrences) {
                if original.ordinal!=projected.ordinal || original.source!=projected.source || original.dialect!=projected.dialect
                    || original.display!=projected.display || original.metadata!=projected.metadata
                    || serde_json::to_value(&original.document).ok()!=serde_json::to_value(&projected.document).ok() {
                    return Err("math_unsupported_extension".into());
                }
                spans.push((projected.byte_from,projected.byte_to,original.ordinal));
            }
        }
        let entry = self.insert(source.to_owned(), analysis)?;
        let mut response=json!({"ok":true,"context":entry.description()});
        if let Some(view)=view {
            let mut output=String::with_capacity(view.len());let mut cursor=0;
            if entry.analysis.plan.is_active() {
                for (from,to,ordinal) in spans {
                    output.push_str(&view[cursor..from]);
                    output.push_str(&format!("<!--readmd:math-slot:v1:{}:{}-->",entry.nonce,ordinal));
                    output.extend(view[from..to].chars().filter(|c|matches!(c,'\r'|'\n')));cursor=to;
                }
            }
            output.push_str(&view[cursor..]);response["view_content"]=json!(output);
        }
        Ok(response)
    }
    fn insert(&self, source: String, analysis: SourceAnalysis) -> Result<Arc<Entry>, String> {
        analysis.plan.validate_doc(&analysis.author).map_err(|e|e.message())?;
        // Charge raw source plus its retained models/descriptor strings. Bounds
        // apply to every snapshot, including prospective patch snapshots.
        // The source is also retained by the author AST and the plan's source
        // inventory. Charge those copies, not just the original String.
        let mut charge = source.len().checked_mul(3).ok_or("math_budget")?;
        for occurrence in &analysis.occurrences {
            let model = serde_json::to_vec(&occurrence.document).map_err(|_| "math_budget")?;
            charge = charge.checked_add(model.len().checked_mul(3).ok_or("math_budget")?)
                .and_then(|n| n.checked_add(occurrence.source.len()))
                .and_then(|n| n.checked_add(occurrence.metadata.as_ref().map_or(0, String::len)))
                .ok_or("math_budget")?;
            if charge > MAX_BYTES { return Err("math_budget".into()); }
        }
        if charge > MAX_BYTES { return Err("math_budget".into()); }
        let entry = Arc::new(Entry {id:uuid::Uuid::new_v4().to_string(), nonce:uuid::Uuid::new_v4().to_string(), revision:tx::revision(&source), source,
            analysis, charge, expires:Instant::now() + TTL});
        let mut state = self.state.lock().map_err(|_| CONFLICT)?;
        state.expire(Instant::now());
        while state.entries.len() >= MAX_CONTEXTS || state.bytes + charge > MAX_BYTES {
            let position = state.entries.iter().position(|entry| Arc::strong_count(entry) == 1).ok_or("math_budget")?;
            let old = state.entries.remove(position).ok_or("math_budget")?; state.bytes -= old.charge;
        }
        state.bytes += charge; state.entries.push_back(entry.clone()); Ok(entry)
    }
    pub(crate) fn bind(&self, request: &Value) -> Result<Option<Binding>, String> {
        let Some(context) = request.get("document_context") else { return Ok(None); };
        let selector: Selector = serde_json::from_value(context.clone()).map_err(|_| "invalid_json")?;
        let mut state = self.state.lock().map_err(|_| CONFLICT)?;
        state.expire(Instant::now());
        let position = state.entries.iter().position(|entry| entry.id == selector.id && entry.expires > Instant::now()).ok_or(CONFLICT)?;
        let entry = state.entries.remove(position).ok_or(CONFLICT)?;
        state.entries.push_back(entry.clone()); drop(state);
        if selector.revision != entry.revision || selector.fingerprint != entry.analysis.plan.fingerprint() { return Err(CONFLICT.into()); }
        // UTF-16 range validation rejects surrogate splits even if a malformed
        // selector happens to fit the serialized document's byte length.
        tx::byte_offset(&entry.source, selector.from).map_err(str::to_owned)?;
        tx::byte_offset(&entry.source, selector.to).map_err(str::to_owned)?;
        let matches = entry.analysis.occurrences.iter().filter(|o| o.from == selector.from && o.to == selector.to).collect::<Vec<_>>();
        if matches.len() != 1 { return Err("transaction_invalid_range".into()); }
        let ordinal = matches[0].ordinal;
        let binding = Binding {entry, ordinal}; binding.check_request(request)?; Ok(Some(binding))
    }
    pub(crate) fn current(&self, binding: &Binding) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|_| CONFLICT)?; state.expire(Instant::now());
        if state.entries.iter().any(|entry| Arc::ptr_eq(entry, &binding.entry) && entry.expires > Instant::now()) { Ok(()) } else { Err(CONFLICT.into()) }
    }
    pub(crate) fn patch_binding(&self, original: &Binding, changed: &MathDocument) -> Result<Binding, String> {
        self.current(original)?;
        let occurrence = original.occurrence();
        // A native metadata carrier needs an explicit complete-source rebind;
        // replacing its visible LaTeX alone would leave a stale hidden author.
        if !occurrence.patchable || occurrence.metadata.is_some() || occurrence.document.dialect != changed.dialect { return Err(CONFLICT.into()); }
        let before = crate::math_defaults::MathDefaultsState::from_xml(&occurrence.document.root).map_err(str::to_owned)?;
        let after = crate::math_defaults::MathDefaultsState::from_xml(&changed.root).map_err(str::to_owned)?;
        if before != after { return Err(CONFLICT.into()); }
        let source = &original.entry.source;
        let mut candidate = String::with_capacity(source.len() + changed.source.len());
        candidate.push_str(&source[..occurrence.body_byte_from]); candidate.push_str(&changed.source);
        candidate.push_str(&source[occurrence.body_byte_to..]);
        if candidate.len() > tx::MAX_DOCUMENT_BYTES { return Err("math_budget".into()); }
        let analysis = math_source_context::analyze_source(&candidate)?;
        let replacement = analysis.occurrences.get(original.ordinal).ok_or(CONFLICT)?;
        let end = occurrence.byte_to.checked_sub(occurrence.body_byte_to-occurrence.body_byte_from)
            .and_then(|end|end.checked_add(changed.source.len())).ok_or("math_budget")?;
        if replacement.byte_from != occurrence.byte_from || replacement.byte_to != end || replacement.source != changed.source
            || replacement.dialect != occurrence.dialect || replacement.display != occurrence.display
            || analysis.plan.is_ordinary(original.ordinal) != original.entry.analysis.plan.is_ordinary(original.ordinal)
            || serde_json::to_value(&replacement.document).ok() != serde_json::to_value(changed).ok() { return Err(CONFLICT.into()); }
        let entry = self.insert(candidate, analysis)?;
        Ok(Binding {entry, ordinal:original.ordinal})
    }
}

impl Entry {
    fn description(&self) -> Value { json!({"id":self.id,"revision":self.revision,"fingerprint":self.analysis.plan.fingerprint(),
        "active":self.analysis.plan.is_active(),"nonce":self.nonce,"ttl_ms":TTL.as_millis(),"occurrences":self.analysis.occurrences}) }
}
impl Binding {
    fn occurrence(&self) -> &crate::math_source_context::Occurrence { &self.entry.analysis.occurrences[self.ordinal] }
    fn check_request(&self, request: &Value) -> Result<(), String> {
        let occurrence = self.occurrence();
        let source = request["source"].as_str().ok_or("invalid_json")?;
        let dialect = request.get("dialect").map_or(Ok("latex"), |v| v.as_str().ok_or("invalid_json"))?;
        let display = request.get("display").map_or(Ok(false), |v| v.as_bool().ok_or("invalid_json"))?;
        let metadata = request.get("metadata").map(|v| v.as_str().ok_or("invalid_json")).transpose()?;
        if source != occurrence.source || dialect != occurrence.dialect || display != occurrence.display
            || metadata != occurrence.metadata.as_deref() { return Err(CONFLICT.into()); }
        Ok(())
    }
    pub(crate) fn author(&self, document: &MathDocument) -> Result<(), String> {
        if serde_json::to_value(document).ok() != serde_json::to_value(&self.occurrence().document).ok() { return Err(CONFLICT.into()); }
        Ok(())
    }
    pub(crate) fn document(&self) -> &MathDocument { &self.occurrence().document }
    pub(crate) fn render(&self, document: &MathDocument) -> Result<MathDocument, String> {
        self.author(document)?;
        self.entry.analysis.plan.render_formula(document, self.ordinal, self.occurrence().display).map_err(|e| e.message())
    }
    pub(crate) fn response(&self) -> Value {
        let o = self.occurrence(); json!({"id":self.entry.id,"revision":self.entry.revision,
            "fingerprint":self.entry.analysis.plan.fingerprint(),"from":o.from,"to":o.to})
    }
}

#[cfg(test)]
#[path = "math_context_tests.rs"]
mod tests;
