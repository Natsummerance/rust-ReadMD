//! Strict, single-compilation link ownership for ordinary Typst baseline rises.
//! Private witnesses are fully rewritten away before a Rect-only incremental
//! revision. Native StructParent keys, never URI/text/order or cross-render IDs,
//! carry the verified real annotations across that cleanup boundary.
use super::{Dictionary, Document, Hit, Instrumented, Object, ObjectId, PageMap};
use std::collections::{BTreeMap, BTreeSet};

#[path = "publication_pagination_baseline_clean.rs"]
mod clean;
#[path = "publication_pagination_baseline_graph.rs"]
mod graph;
#[path = "publication_pagination_baseline_paint.rs"]
mod paint;
#[cfg(test)]
#[path = "publication_pagination_baseline_paint_tests.rs"]
mod paint_tests;
use graph::{Annotation, Graph, Page};

const INVALID: &str = "typst_invalid_output";
const UNSUPPORTED: &str = crate::api_codes::PUBLICATION_BASELINE_UNSUPPORTED;
const LIMIT: &str = "typst_output_limit";
const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_LINKS: usize = 131072;
const MAX_DEPTH: usize = 64;

#[derive(Clone, Debug)]
pub struct BaselineLinkRequest {
    pub block_id: String,
    pub rise_pt: f64,
    /// Native logical Link groups, excluding the outer source witness. A
    /// single logical link may own many page/line annotation fragments.
    pub expected_links: usize,
}

/// A source-owned CODE/equation object which retains the unshifted baseline.
/// Its identity is emitted by the same writer/compile, never inferred from a
/// font, copied text, URI destination of a real link, or a neighbouring glyph.
#[derive(Clone, Debug)]
pub struct BaselineProtectedRequest {
    pub block_id: String,
    pub witness_id: String,
}

// The legacy preview keeps its existing source/geometry rules. Only the
// ownership check for its removed private action objects is shared here.
pub(super) fn exclusive_temporary_actions(
    doc: &Document,
    temporary: &BTreeSet<ObjectId>,
) -> Result<BTreeSet<ObjectId>, String> {
    clean::temporary_actions(doc, temporary, &mut Budget::new())
}

struct Budget(usize);
impl Budget {
    fn new() -> Self {
        Self(2_000_000)
    }
    fn step(&mut self, depth: usize) -> Result<(), String> {
        if crate::speech::cancelled() {
            return Err("cancelled".into());
        }
        if depth > MAX_DEPTH || self.0 == 0 {
            return Err(LIMIT.into());
        }
        self.0 -= 1;
        Ok(())
    }
}

#[derive(Clone)]
struct Owned {
    key: i64,
    source: String,
    page: u32,
    page_id: ObjectId,
    original: Dictionary,
    rect: [f64; 4],
}
struct Proof {
    wrappers: BTreeMap<ObjectId, String>,
    protected: BTreeMap<ObjectId, String>,
    temporary_annotations: BTreeSet<ObjectId>,
    real: Vec<Owned>,
}

fn request_map<'a>(
    input: &Instrumented,
    requests: &'a [BaselineLinkRequest],
) -> Result<BTreeMap<String, &'a BaselineLinkRequest>, String> {
    if input.nonce.len() != 32
        || !input.nonce.bytes().all(|b| b.is_ascii_hexdigit())
        || input.nodes.len() > crate::publication_blocks::MAX_NODES
        || requests.len() > crate::publication_blocks::MAX_NODES
    {
        return Err(INVALID.into());
    }
    let mut known = BTreeMap::new();
    for node in &input.nodes {
        if known.insert(node.id.as_str(), node).is_some() {
            return Err(INVALID.into());
        }
    }
    let mut values = BTreeMap::new();
    for request in requests {
        let node = known.get(request.block_id.as_str()).ok_or(INVALID)?;
        if node.kind != "paragraph" || node.nested || !node.editable {
            return Err(UNSUPPORTED.into());
        }
        if !request.rise_pt.is_finite()
            || !(-72. ..=72.).contains(&request.rise_pt)
            || request.expected_links > MAX_LINKS
            || values.insert(request.block_id.clone(), request).is_some()
        {
            return Err(INVALID.into());
        }
    }
    Ok(values)
}

fn owner(
    graph: &Graph,
    wrappers: &BTreeMap<ObjectId, String>,
    mut tag: ObjectId,
) -> Result<Option<String>, String> {
    let mut found = BTreeSet::new();
    for _ in 0..MAX_DEPTH {
        if let Some(source) = wrappers.get(&tag) {
            found.insert(source.clone());
        }
        if tag == graph.root {
            return if found.len() <= 1 {
                Ok(found.into_iter().next())
            } else {
                Err(INVALID.into())
            };
        }
        tag = graph.tags.get(&tag).ok_or(INVALID)?.parent;
    }
    Err(LIMIT.into())
}

fn tag_ancestors(graph: &Graph, mut tag: ObjectId, budget: &mut Budget) -> Result<Vec<ObjectId>, String> {
    let mut path = Vec::new();
    for _ in 0..MAX_DEPTH {
        budget.step(0)?;
        if tag == graph.root { return Ok(path); }
        path.push(tag);
        tag = graph.tags.get(&tag).ok_or(INVALID)?.parent;
    }
    Err(LIMIT.into())
}

fn prove(
    input: &Instrumented,
    requests: &BTreeMap<String, &BaselineLinkRequest>,
    annotations: &[Annotation],
    graph: &Graph,
    protected: &BTreeMap<String, String>,
    require_paint: bool,
    budget: &mut Budget,
) -> Result<Proof, String> {
    graph.check_annotations(annotations)?;
    let known = input
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut wrappers = BTreeMap::new();
    let mut source_wrappers = BTreeMap::new();
    let mut source_candidates = BTreeMap::<String, BTreeSet<ObjectId>>::new();
    let real_tags = annotations.iter().filter(|annotation| annotation.temporary.is_none())
        .map(|annotation| graph.objrs.get(&annotation.id).map(|objr|objr.tag).ok_or(INVALID))
        .collect::<Result<BTreeSet<_>, _>>()?;
    let mut protected_tags = BTreeMap::new();
    let mut protected_sources = BTreeMap::new();
    for annotation in annotations.iter().filter(|a| a.temporary.is_some()) {
        budget.step(0)?;
        let source = annotation.temporary.as_ref().ok_or(INVALID)?;
        let tag = graph.objrs.get(&annotation.id).ok_or(INVALID)?.tag;
        if graph.tags.get(&tag).ok_or(INVALID)?.role != b"Link" {
            return Err(INVALID.into());
        }
        if protected.contains_key(source) {
            if protected_tags.get(&tag).is_some_and(|id| id != source)
                || protected_sources.get(source).is_some_and(|id| *id != tag)
            {
                return Err(INVALID.into());
            }
            protected_tags.insert(tag, source.clone());
            protected_sources.insert(source.clone(), tag);
            continue;
        }
        source_candidates.entry(source.clone()).or_default().insert(tag);
    }
    // Typst can flush a source witness's OBJR into the active descendant
    // real Link at a page break. The source must still have an actually
    // witnessed outer Link which is an ancestor of every such attachment.
    // Never manufacture an unwitnessed LCA or select an owner by URL/Rect.
    for (source, candidates) in &source_candidates {
        let first = *candidates.iter().next().ok_or(INVALID)?;
        let path = tag_ancestors(graph, first, budget)?;
        let wrapper = *path.iter().rev().find(|id| candidates.contains(*id)).ok_or(INVALID)?;
        for candidate in candidates {
            if !tag_ancestors(graph, *candidate, budget)?.contains(&wrapper) {
                return Err(INVALID.into());
            }
        }
        if protected_tags.contains_key(&wrapper)
            || real_tags.contains(&wrapper)
            || wrappers.insert(wrapper, source.clone()).is_some()
        { return Err(INVALID.into()); }
        source_wrappers.insert(source.clone(), wrapper);
    }
    let temporary_annotations = annotations.iter().filter(|annotation| annotation.temporary.is_some())
        .map(|annotation| annotation.id).collect::<BTreeSet<_>>();
    for source in requests.keys() {
        if !source_wrappers.contains_key(source) {
            return Err(UNSUPPORTED.into());
        }
    }
    for (witness, block) in protected {
        let tag = *protected_sources.get(witness).ok_or(UNSUPPORTED)?;
        let wrapper = *source_wrappers.get(block).ok_or(UNSUPPORTED)?;
        if owner(graph, &wrappers, tag)?.as_ref() != Some(block)
            || graph.nearest_paragraph(tag)? != Some(graph.wrapper_paragraph(wrapper, budget)?)
        {
            return Err(INVALID.into());
        }
    }
    for (tag, source) in &wrappers {
        if owner(graph, &wrappers, *tag)?.as_ref() != Some(source) {
            return Err(INVALID.into());
        }
    }
    let mut logical = BTreeMap::<String, Vec<ObjectId>>::new();
    for (id, tag) in &graph.tags {
        budget.step(0)?;
        if tag.role == b"Link" && !wrappers.contains_key(id) && !protected_tags.contains_key(id) {
            if let Some(source) = owner(graph, &wrappers, *id)? {
                if requests.contains_key(&source) {
                    logical.entry(source).or_default().push(*id);
                }
            }
        }
    }
    for (source, request) in requests {
        let links = logical.get(source).map(Vec::as_slice).unwrap_or(&[]);
        if links.len() != request.expected_links {
            return Err(INVALID.into());
        }
        let wrapper = *source_wrappers.get(source).ok_or(INVALID)?;
        let paragraph = graph.wrapper_paragraph(wrapper, budget)?;
        for candidate in source_candidates.get(source).ok_or(INVALID)? {
            if *candidate != wrapper && graph.nearest_paragraph(*candidate)? != Some(paragraph) {
                return Err(INVALID.into());
            }
        }
        for link in links {
            if graph.nearest_paragraph(*link)? != Some(paragraph) {
                return Err(INVALID.into());
            }
            graph.ordinary_link_ancestry(*link, wrapper)?;
            // The qualified native K protocol pairs a nonempty same-page
            // content group with each OBJR. No trailing content, empty group,
            // unrepresented page or silent missing fragment is accepted.
            if require_paint {
                graph.link_content_inventory(*link, &temporary_annotations, budget)?;
            } else {
                graph.check_link_fragments(*link, &temporary_annotations, budget)?;
            }
        }
    }
    let mut real = Vec::new();
    for annotation in annotations.iter().filter(|a| a.temporary.is_none()) {
        budget.step(0)?;
        let tag = graph.objrs.get(&annotation.id).ok_or(INVALID)?.tag;
        let source = owner(graph, &wrappers, tag)?.ok_or(UNSUPPORTED)?;
        if !known.contains(source.as_str()) {
            return Err(INVALID.into());
        }
        if requests
            .get(&source)
            .is_some_and(|request| request.rise_pt != 0.)
        {
            graph::supported_rect_annotation(&annotation.original)?;
        }
        if require_paint {
            graph::supported_rect_annotation(&annotation.original)?;
        }
        real.push(Owned {
            key: annotation.key,
            source,
            page: annotation.page,
            page_id: annotation.page_id,
            original: annotation.original.clone(),
            rect: annotation.rect,
        });
    }
    Ok(Proof {
        wrappers,
        protected: protected_tags,
        temporary_annotations,
        real,
    })
}

fn moved_rect(rect: [f64; 4], rise: f64, media: [f64; 4]) -> Result<[f64; 4], String> {
    let mut next = rect;
    for index in [1, 3] {
        let value = (rect[index] + rise) as f32;
        if !value.is_finite() {
            return Err(INVALID.into());
        }
        next[index] = value as f64;
    }
    if next[0] < media[0] || next[1] < media[1] || next[2] > media[2] || next[3] > media[3] {
        return Err(UNSUPPORTED.into());
    }
    Ok(next)
}
fn hit(rect: [f64; 4], media: [f64; 4], source: &str, page: u32) -> Option<Hit> {
    let left = rect[0].max(media[0]);
    let right = rect[2].min(media[2]);
    let low = rect[1].max(media[1]);
    let high = rect[3].min(media[3]);
    (right > left && high > low).then(|| Hit {
        id: source.into(),
        page,
        rect: [left - media[0], media[3] - high, right - left, high - low],
    })
}

/// Empty requests retain the exact legacy extractor. Nonempty requests require
/// the original mapped PDF from this render and refuse incomplete ownership.
pub fn extract_and_clean_with_baseline(
    bytes: &[u8],
    input: &Instrumented,
    requests: &[BaselineLinkRequest],
) -> Result<(Vec<u8>, PageMap), String> {
    extract(bytes, input, requests, &[], false)
}

/// Product entry point. Unlike the graph-only component API above, this also
/// proves all actual marked-content paint, source-owned protection, embedded
/// glyph outlines, physical paper bounds and introduced ink collisions.
pub fn extract_and_clean_with_baseline_and_paint(
    bytes: &[u8],
    input: &Instrumented,
    requests: &[BaselineLinkRequest],
    protected: &[BaselineProtectedRequest],
) -> Result<(Vec<u8>, PageMap), String> {
    extract(bytes, input, requests, protected, true)
}

fn extract(
    bytes: &[u8],
    input: &Instrumented,
    requests: &[BaselineLinkRequest],
    protected: &[BaselineProtectedRequest],
    require_paint: bool,
) -> Result<(Vec<u8>, PageMap), String> {
    if requests.is_empty() {
        if !protected.is_empty() {
            return Err(INVALID.into());
        }
        return super::extract_and_clean(bytes, input.clone());
    }
    let requests = request_map(input, requests)?;
    if protected.len() > MAX_LINKS {
        return Err(LIMIT.into());
    }
    let known = input
        .nodes
        .iter()
        .map(|n| n.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut protected_map = BTreeMap::new();
    for p in protected {
        if !requests.contains_key(&p.block_id)
            || known.contains(p.witness_id.as_str())
            || p.witness_id.is_empty()
            || p.witness_id.len() > 128
            || !p
                .witness_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            || protected_map
                .insert(p.witness_id.clone(), p.block_id.clone())
                .is_some()
        {
            return Err(INVALID.into());
        }
    }
    let witnesses = protected_map.keys().cloned().collect::<BTreeSet<_>>();
    if bytes.len() > MAX_BYTES {
        return Err(LIMIT.into());
    }
    let mut budget = Budget::new();
    budget.step(0)?;
    let mut doc = Document::load_mem(bytes).map_err(|_| INVALID)?;
    if doc.objects.len() > 262144 {
        return Err(LIMIT.into());
    }
    if doc.trailer.has(b"Prev") || doc.trailer.has(b"XRefStm") || doc.trailer.has(b"Encrypt") {
        return Err(UNSUPPORTED.into());
    }
    let (pages, annotations) =
        graph::pages_and_annotations_with_witnesses(&doc, input, &witnesses, &mut budget)?;
    let graph = Graph::read(&doc, &pages, &mut budget)?;
    let proof = prove(
        input,
        &requests,
        &annotations,
        &graph,
        &protected_map,
        require_paint,
        &mut budget,
    )?;
    let paint = if require_paint {
        Some(paint::verify(
            &doc,
            &pages,
            &graph,
            &proof,
            &requests,
            &annotations,
            &mut budget,
        )?)
    } else {
        None
    };
    let mut all_wrappers = proof.wrappers.clone();
    all_wrappers.extend(proof.protected.clone());
    let cleaned = clean::rewrite(
        &mut doc,
        input,
        &pages,
        &annotations,
        &graph,
        &all_wrappers,
        &mut budget,
    )?;
    let clean_doc = Document::load_mem(&cleaned).map_err(|_| INVALID)?;
    if let Some(paint) = &paint {
        paint.check_unchanged(&clean_doc, &pages, &mut budget)?;
    }
    let (clean_pages, clean_annotations) =
        graph::pages_and_annotations(&clean_doc, input, &mut budget)?;
    if clean_annotations.iter().any(|a| a.temporary.is_some()) {
        return Err(INVALID.into());
    }
    let clean_graph = Graph::read(&clean_doc, &clean_pages, &mut budget)?;
    clean_graph.check_annotations(&clean_annotations)?;
    if clean_pages.len() != pages.len() || clean_annotations.len() != proof.real.len() {
        return Err(INVALID.into());
    }
    let by_key = clean_annotations
        .iter()
        .map(|a| (a.key, a))
        .collect::<BTreeMap<_, _>>();
    let mut updates = BTreeMap::new();
    for real in &proof.real {
        let annotation = by_key.get(&real.key).ok_or(INVALID)?;
        if annotation.page != real.page
            || annotation.page_id != real.page_id
            || annotation.original != real.original
        {
            return Err(INVALID.into());
        }
        if let Some(request) = requests.get(&real.source).filter(|r| r.rise_pt != 0.) {
            let media = pages.get(real.page as usize - 1).ok_or(INVALID)?.media;
            updates.insert(
                annotation.id,
                moved_rect(real.rect, request.rise_pt, media)?,
            );
        }
    }
    let output = clean::incremental(&cleaned, clean_doc, &updates, input, &mut budget)?;
    let final_doc = Document::load_mem(&output).map_err(|_| INVALID)?;
    let (_, final_annotations) = graph::pages_and_annotations(&final_doc, input, &mut budget)?;
    let final_by_key = final_annotations
        .iter()
        .map(|a| (a.key, a))
        .collect::<BTreeMap<_, _>>();
    let mut hits = Vec::new();
    // A source wrapper can enclose protected CODE/Math at the original rise.
    // Its source hit conservatively includes both native and raised envelopes;
    // only genuine ordinary-text link annotations receive a PDF Rect update.
    for annotation in annotations.iter().filter(|a| a.temporary.is_some()) {
        let source = annotation.temporary.as_ref().ok_or(INVALID)?;
        if protected_map.contains_key(source) {
            continue;
        }
        let rise = requests.get(source).map_or(0., |r| r.rise_pt);
        let mut rect = annotation.rect;
        rect[1] = rect[1].min(rect[1] + rise);
        rect[3] = rect[3].max(rect[3] + rise);
        if let Some(value) = hit(
            rect,
            pages[annotation.page as usize - 1].media,
            source,
            annotation.page,
        ) {
            hits.push(value);
        }
    }
    for real in &proof.real {
        let annotation = final_by_key.get(&real.key).ok_or(INVALID)?;
        if let Some(value) = hit(
            annotation.rect,
            pages[real.page as usize - 1].media,
            &real.source,
            real.page,
        ) {
            hits.push(value);
        }
    }
    if let Some(paint) = &paint {
        paint.append_source_hits(input, &pages, &mut hits, &mut budget)?;
    }
    if hits.len() > MAX_LINKS {
        return Err(LIMIT.into());
    }
    Ok((
        output,
        PageMap {
            revision: input.revision.clone(),
            nodes: input.nodes.clone(),
            hits,
            pages: pages
                .iter()
                .map(|p| [p.media[2] - p.media[0], p.media[3] - p.media[1]])
                .collect(),
        },
    ))
}

#[cfg(test)]
#[path = "publication_pagination_baseline_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "publication_pagination_baseline_multipage_tests.rs"]
mod multipage_tests;
