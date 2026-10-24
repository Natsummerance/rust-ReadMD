use super::*;
use std::io::Read;

fn references(
    value: &Object,
    owner: ObjectId,
    out: &mut BTreeMap<ObjectId, BTreeSet<ObjectId>>,
    depth: usize,
    budget: &mut Budget,
) -> Result<(), String> {
    budget.step(depth)?;
    match value {
        Object::Reference(id) => {
            out.entry(*id).or_default().insert(owner);
        }
        Object::Array(values) => {
            for value in values {
                references(value, owner, out, depth + 1, budget)?;
            }
        }
        Object::Dictionary(d) => {
            for (_, value) in d.iter() {
                references(value, owner, out, depth + 1, budget)?;
            }
        }
        Object::Stream(stream) => {
            for (_, value) in stream.dict.iter() {
                references(value, owner, out, depth + 1, budget)?;
            }
        }
        _ => (),
    }
    Ok(())
}
fn incoming(
    doc: &Document,
    budget: &mut Budget,
) -> Result<BTreeMap<ObjectId, BTreeSet<ObjectId>>, String> {
    let mut out = BTreeMap::new();
    for (id, value) in &doc.objects {
        references(value, *id, &mut out, 0, budget)?;
    }
    references(
        &Object::Dictionary(doc.trailer.clone()),
        (0, u16::MAX),
        &mut out,
        0,
        budget,
    )?;
    Ok(out)
}
fn exclusively(
    uses: &BTreeMap<ObjectId, BTreeSet<ObjectId>>,
    id: ObjectId,
    allowed: &BTreeSet<ObjectId>,
) -> Result<(), String> {
    if uses
        .get(&id)
        .is_some_and(|owners| !owners.is_subset(allowed))
    {
        return Err(INVALID.into());
    }
    Ok(())
}
fn dict_mut(doc: &mut Document, id: ObjectId) -> Result<&mut Dictionary, String> {
    doc.objects
        .get_mut(&id)
        .and_then(|v| v.as_dict_mut().ok())
        .ok_or_else(|| INVALID.into())
}

pub(super) fn temporary_actions(
    doc: &Document,
    temporary: &BTreeSet<ObjectId>,
    budget: &mut Budget,
) -> Result<BTreeSet<ObjectId>, String> {
    if temporary.is_empty() {
        return Ok(BTreeSet::new());
    }
    let uses = incoming(doc, budget)?;
    let mut removed = BTreeSet::new();
    let mut uris = BTreeSet::new();
    for id in temporary {
        budget.step(0)?;
        let annotation = doc
            .objects
            .get(id)
            .ok_or(INVALID)?
            .as_dict()
            .map_err(|_| INVALID)?;
        let action = annotation.get(b"A").map_err(|_| INVALID)?;
        if let Object::Reference(id) = action {
            exclusively(&uses, *id, temporary)?;
            removed.insert(*id);
        }
        let action = graph::direct(doc, action)?.as_dict().map_err(|_| INVALID)?;
        if let Object::Reference(uri) = action.get(b"URI").map_err(|_| INVALID)? {
            if doc.objects.get(uri).ok_or(INVALID)?.as_str().is_err() {
                return Err(INVALID.into());
            }
            uris.insert(*uri);
        }
    }
    let mut owners = removed.clone();
    owners.extend(temporary.iter().copied());
    for uri in &uris {
        exclusively(&uses, *uri, &owners)?;
    }
    removed.extend(uris);
    Ok(removed)
}

pub(super) fn rewrite(
    doc: &mut Document,
    input: &Instrumented,
    pages: &[Page],
    annotations: &[Annotation],
    graph: &Graph,
    wrappers: &BTreeMap<ObjectId, String>,
    budget: &mut Budget,
) -> Result<Vec<u8>, String> {
    let uses = incoming(doc, budget)?;
    let temporary = annotations
        .iter()
        .filter(|a| a.temporary.is_some())
        .map(|a| a.id)
        .collect::<BTreeSet<_>>();
    let keys = annotations
        .iter()
        .filter(|a| a.temporary.is_some())
        .map(|a| a.key)
        .collect::<BTreeSet<_>>();
    let mut removed = temporary.clone();
    for annotation in annotations.iter().filter(|a| a.temporary.is_some()) {
        if let Some(action) = annotation.action {
            // Shared actions belonging entirely to removed witness fragments
            // are safe; any surviving/indirect third-party reference refuses.
            exclusively(&uses, action, &temporary)?;
            removed.insert(action);
        }
        if let Some(id) = graph.objrs.get(&annotation.id).and_then(|o| o.indirect) {
            removed.insert(id);
        }
    }
    for (id, tag) in &graph.tags {
        budget.step(0)?;
        if !tag
            .kids
            .iter()
            .any(|kid| matches!(&kid.kind, graph::Kind::Objr(target) if temporary.contains(target)))
        {
            continue;
        }
        let retained = tag
            .kids
            .iter()
            .filter(
                |kid| !matches!(&kid.kind, graph::Kind::Objr(target) if temporary.contains(target)),
            )
            .map(|kid| kid.value.clone())
            .collect::<Vec<_>>();
        let arrays = graph
            .k_arrays
            .iter()
            .filter(|(_, owner)| **owner == *id)
            .map(|(array, _)| *array)
            .collect::<BTreeSet<_>>();
        let mut allowed = arrays.clone();
        allowed.insert(*id);
        for array in &arrays {
            exclusively(&uses, *array, &allowed)?;
        }
        removed.extend(arrays);
        let d = dict_mut(doc, *id)?;
        if retained.is_empty() {
            d.remove(b"K");
        } else {
            d.set("K", retained);
        }
        if wrappers.contains_key(id) {
            d.set("S", Object::Name(b"Span".to_vec()));
        }
    }
    let mut tree_owners = graph.tree.nodes.clone();
    tree_owners.extend(graph.tree.arrays.iter().copied());
    tree_owners.insert(graph.root);
    for id in &graph.tree.nodes {
        exclusively(&uses, *id, &tree_owners)?;
    }
    for id in &graph.tree.arrays {
        exclusively(&uses, *id, &tree_owners)?;
    }
    let retained = graph
        .tree
        .entries
        .iter()
        .filter(|(key, _)| !keys.contains(key))
        .collect::<Vec<_>>();
    let nums = retained
        .iter()
        .flat_map(|(key, value)| [Object::Integer(**key), (**value).clone()])
        .collect::<Vec<_>>();
    let d = if let Some(root) = graph.tree.root {
        dict_mut(doc, root)?
    } else {
        dict_mut(doc, graph.root)?
            .get_mut(b"ParentTree")
            .and_then(Object::as_dict_mut)
            .map_err(|_| INVALID)?
    };
    d.remove(b"Kids");
    d.remove(b"Limits");
    d.set("Nums", nums);
    if let (Some((first, _)), Some((last, _))) = (retained.first(), retained.last()) {
        d.set(
            "Limits",
            vec![Object::Integer(**first), Object::Integer(**last)],
        );
    }
    removed.extend(
        graph
            .tree
            .nodes
            .iter()
            .copied()
            .filter(|id| Some(*id) != graph.tree.root),
    );
    removed.extend(graph.tree.arrays.iter().copied());
    for page in pages {
        budget.step(0)?;
        let retained = page
            .annotation_ids
            .iter()
            .copied()
            .filter(|id| !temporary.contains(id))
            .map(Object::Reference)
            .collect::<Vec<_>>();
        if let Some(array) = page.annotation_array {
            exclusively(&uses, array, &BTreeSet::from([page.id]))?;
            doc.objects.insert(array, Object::Array(retained.clone()));
        }
        let d = dict_mut(doc, page.id)?;
        if retained.is_empty() {
            d.remove(b"Annots");
        } else if page.annotation_array.is_none() {
            d.set("Annots", retained);
        }
    }
    // Full save drops these structural containers. Remove them from the
    // in-memory proof scan too, so an old ObjStm cannot retain private bytes.
    removed.extend(doc.objects.iter().filter_map(|(id, value)| {
        value
            .type_name()
            .ok()
            .filter(|name| {
                [
                    b"ObjStm".as_slice(),
                    b"XRef".as_slice(),
                    b"Linearized".as_slice(),
                ]
                .contains(name)
            })
            .map(|_| *id)
    }));
    for id in &removed {
        doc.objects.remove(id);
    }
    let remaining = incoming(doc, budget)?;
    if removed.iter().any(|id| remaining.contains_key(id)) {
        return Err(INVALID.into());
    }
    no_private(doc, input, budget)?;
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).map_err(|_| INVALID)?;
    if bytes.len() > MAX_BYTES {
        return Err(LIMIT.into());
    }
    let parsed = Document::load_mem(&bytes).map_err(|_| INVALID)?;
    no_private(&parsed, input, budget)?;
    if has_private(&bytes, &needles(input)) {
        return Err(INVALID.into());
    }
    Ok(bytes)
}

fn needles(input: &Instrumented) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for value in [
        input.nonce.as_bytes(),
        b"readmd-internal:publication/".as_slice(),
        b"readmd:page-source".as_slice(),
    ] {
        out.push(value.to_vec());
        out.push(value.iter().flat_map(|b| [0, *b]).collect());
        out.push(value.iter().flat_map(|b| [*b, 0]).collect());
    }
    out
}
fn has_private(bytes: &[u8], needles: &[Vec<u8>]) -> bool {
    needles
        .iter()
        .any(|needle| bytes.windows(needle.len()).any(|value| value == needle))
}
fn no_private(doc: &Document, input: &Instrumented, budget: &mut Budget) -> Result<(), String> {
    fn scan(
        doc: &Document,
        value: &Object,
        needles: &[Vec<u8>],
        expanded: &mut usize,
        depth: usize,
        budget: &mut Budget,
    ) -> Result<(), String> {
        budget.step(depth)?;
        match value {
            Object::Name(bytes) | Object::String(bytes, _) if has_private(bytes, needles) => {
                return Err(INVALID.into())
            }
            Object::Array(values) => {
                for value in values {
                    scan(doc, value, needles, expanded, depth + 1, budget)?;
                }
            }
            Object::Dictionary(d) => {
                for (key, value) in d.iter() {
                    if has_private(key, needles) {
                        return Err(INVALID.into());
                    }
                    scan(doc, value, needles, expanded, depth + 1, budget)?;
                }
            }
            Object::Stream(stream) => {
                for (key, value) in stream.dict.iter() {
                    if has_private(key, needles) {
                        return Err(INVALID.into());
                    }
                    scan(doc, value, needles, expanded, depth + 1, budget)?;
                }
                if has_private(&stream.content, needles) {
                    return Err(INVALID.into());
                }
                let filters = match stream.dict.get(b"Filter") {
                    Err(_) => Vec::new(),
                    Ok(value) => match graph::direct(doc, value)? {
                        Object::Name(name) => vec![name.as_slice()],
                        Object::Array(values) => values
                            .iter()
                            .map(|v| v.as_name().map_err(|_| INVALID.into()))
                            .collect::<Result<Vec<_>, String>>()?,
                        _ => return Err(INVALID.into()),
                    },
                };
                let limit = 64 * 1024 * 1024usize;
                if filters.is_empty() {
                    *expanded = expanded.checked_add(stream.content.len()).ok_or(LIMIT)?;
                } else if filters == vec![b"FlateDecode".as_slice()] {
                    let remaining = limit.checked_sub(*expanded).ok_or(LIMIT)?;
                    let mut bytes = Vec::new();
                    flate2::read::ZlibDecoder::new(stream.content.as_slice())
                        .take(remaining as u64 + 1)
                        .read_to_end(&mut bytes)
                        .map_err(|_| INVALID)?;
                    *expanded = expanded.checked_add(bytes.len()).ok_or(LIMIT)?;
                    if has_private(&bytes, needles) {
                        return Err(INVALID.into());
                    }
                } else {
                    return Err(UNSUPPORTED.into());
                }
                if *expanded > limit {
                    return Err(LIMIT.into());
                }
            }
            _ => (),
        }
        Ok(())
    }
    let needles = needles(input);
    let mut expanded = 0;
    for value in doc.objects.values() {
        scan(doc, value, &needles, &mut expanded, 0, budget)?;
    }
    scan(
        doc,
        &Object::Dictionary(doc.trailer.clone()),
        &needles,
        &mut expanded,
        0,
        budget,
    )
}

fn coordinate(value: f64) -> Object {
    // lopdf serializes Real(123.0) as the integer lexeme `123`. Select that
    // canonical object before saving so the exact reparse check remains exact.
    if value.fract() == 0. && value >= i64::MIN as f64 && value < i64::MAX as f64 {
        Object::Integer(value as i64)
    } else {
        Object::Real(value as f32)
    }
}

pub(super) fn incremental(
    bytes: &[u8],
    doc: Document,
    updates: &BTreeMap<ObjectId, [f64; 4]>,
    input: &Instrumented,
    budget: &mut Budget,
) -> Result<Vec<u8>, String> {
    if updates.is_empty() {
        return Ok(bytes.to_vec());
    }
    let mut update = lopdf::IncrementalDocument::create_from(bytes.to_vec(), doc);
    for (id, rect) in updates {
        budget.step(0)?;
        let original = update
            .get_prev_documents()
            .objects
            .get(id)
            .ok_or(INVALID)?
            .as_dict()
            .map_err(|_| INVALID)?;
        let mut values = graph::direct(
            update.get_prev_documents(),
            original.get(b"Rect").map_err(|_| INVALID)?,
        )?
        .as_array()
        .map_err(|_| INVALID)?
        .clone();
        if values.len() != 4 {
            return Err(INVALID.into());
        }
        values[1] = coordinate(rect[1]);
        values[3] = coordinate(rect[3]);
        update
            .opt_clone_object_to_new_document(*id)
            .map_err(|_| INVALID)?;
        dict_mut(&mut update.new_document, *id)?.set("Rect", values);
    }
    if update.new_document.objects.len() != updates.len() {
        return Err(INVALID.into());
    }
    let mut output = Vec::new();
    update.save_to(&mut output).map_err(|_| INVALID)?;
    if output.len() > MAX_BYTES {
        return Err(LIMIT.into());
    }
    if !output.starts_with(bytes) {
        return Err(INVALID.into());
    }
    let parsed = Document::load_mem(&output).map_err(|_| INVALID)?;
    for (id, before) in &update.get_prev_documents().objects {
        budget.step(0)?;
        if before.type_name().ok() == Some(b"XRef") {
            continue;
        }
        let after = parsed.objects.get(id).ok_or(INVALID)?;
        if let Some(rect) = updates.get(id) {
            let mut expected = before.as_dict().map_err(|_| INVALID)?.clone();
            let mut values = graph::direct(
                update.get_prev_documents(),
                expected.get(b"Rect").map_err(|_| INVALID)?,
            )?
            .as_array()
            .map_err(|_| INVALID)?
            .clone();
            values[1] = coordinate(rect[1]);
            values[3] = coordinate(rect[3]);
            expected.set("Rect", values);
            if after != &Object::Dictionary(expected) {
                return Err(INVALID.into());
            }
        } else if after != before {
            return Err(INVALID.into());
        }
    }
    for (id, value) in &parsed.objects {
        if !update.get_prev_documents().objects.contains_key(id)
            && value.type_name().ok() != Some(b"XRef")
        {
            return Err(INVALID.into());
        }
    }
    no_private(&parsed, input, budget)?;
    if has_private(&output, &needles(input)) {
        return Err(INVALID.into());
    }
    Ok(output)
}
