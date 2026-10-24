use super::*;

#[derive(Clone)]
pub(super) struct Page {
    pub id: ObjectId,
    pub media: [f64; 4],
    pub annotation_array: Option<ObjectId>,
    pub annotation_ids: Vec<ObjectId>,
    pub struct_parents: Option<i64>,
}
#[derive(Clone)]
pub(super) struct Annotation {
    pub id: ObjectId,
    pub page: u32,
    pub page_id: ObjectId,
    pub key: i64,
    pub rect: [f64; 4],
    pub original: Dictionary,
    pub temporary: Option<String>,
    pub action: Option<ObjectId>,
}
#[derive(Clone)]
pub(super) struct Content {
    page: Option<ObjectId>,
    mcid: i64,
    tag: ObjectId,
    form: bool,
}
#[derive(Clone)]
pub(super) enum Kind {
    Tag(ObjectId),
    Content(Content),
    Objr(ObjectId),
}
#[derive(Clone)]
pub(super) struct Kid {
    pub value: Object,
    pub kind: Kind,
}
pub(super) struct Tag {
    pub parent: ObjectId,
    pub role: Vec<u8>,
    pub kids: Vec<Kid>,
}
pub(super) struct Objr {
    pub tag: ObjectId,
    pub page: ObjectId,
    pub indirect: Option<ObjectId>,
}
pub(super) struct NumberTree {
    pub root: Option<ObjectId>,
    pub nodes: BTreeSet<ObjectId>,
    pub arrays: BTreeSet<ObjectId>,
    pub entries: BTreeMap<i64, Object>,
}
pub(super) struct Graph {
    pub root: ObjectId,
    pub tags: BTreeMap<ObjectId, Tag>,
    pub objrs: BTreeMap<ObjectId, Objr>,
    pub tree: NumberTree,
    pub k_arrays: BTreeMap<ObjectId, ObjectId>,
    page_keys: BTreeSet<i64>,
    mcid_parents: BTreeMap<ObjectId, Vec<Option<ObjectId>>>,
}

pub(super) fn direct<'a>(doc: &'a Document, value: &'a Object) -> Result<&'a Object, String> {
    match value {
        Object::Reference(id) => doc.objects.get(id).ok_or_else(|| INVALID.into()),
        other => Ok(other),
    }
}
fn dict(doc: &Document, id: ObjectId) -> Result<&Dictionary, String> {
    doc.objects
        .get(&id)
        .and_then(|v| v.as_dict().ok())
        .ok_or_else(|| INVALID.into())
}
fn reference(value: &Object) -> Result<ObjectId, String> {
    value.as_reference().map_err(|_| INVALID.into())
}
fn key(value: &Object) -> Result<i64, String> {
    value
        .as_i64()
        .ok()
        .filter(|n| *n >= 0)
        .ok_or_else(|| INVALID.into())
}
fn rect(value: &Object) -> Result<[f64; 4], String> {
    let values = value
        .as_array()
        .map_err(|_| INVALID)?
        .iter()
        .map(super::super::number)
        .collect::<Option<Vec<_>>>()
        .ok_or(INVALID)?;
    if values.len() != 4
        || !values.iter().all(|n| n.is_finite())
        || values[2] <= values[0]
        || values[3] <= values[1]
    {
        return Err(INVALID.into());
    }
    Ok([values[0], values[1], values[2], values[3]])
}
fn page_ids(doc: &Document, budget: &mut Budget) -> Result<Vec<ObjectId>, String> {
    fn visit(
        doc: &Document,
        id: ObjectId,
        parent: Option<ObjectId>,
        out: &mut Vec<ObjectId>,
        seen: &mut BTreeSet<ObjectId>,
        depth: usize,
        budget: &mut Budget,
    ) -> Result<usize, String> {
        budget.step(depth)?;
        if !seen.insert(id) {
            return Err(INVALID.into());
        }
        let d = dict(doc, id)?;
        if d.get(b"Parent").ok().map(reference).transpose()? != parent {
            return Err(INVALID.into());
        }
        match d
            .get(b"Type")
            .and_then(Object::as_name)
            .map_err(|_| INVALID)?
        {
            b"Page" if parent.is_some() => {
                if out.len() >= 100000 {
                    return Err(LIMIT.into());
                }
                out.push(id);
                Ok(1)
            }
            b"Pages" => {
                let kids = direct(doc, d.get(b"Kids").map_err(|_| INVALID)?)?
                    .as_array()
                    .map_err(|_| INVALID)?;
                let mut count = 0usize;
                for kid in kids {
                    count = count
                        .checked_add(visit(
                            doc,
                            reference(kid)?,
                            Some(id),
                            out,
                            seen,
                            depth + 1,
                            budget,
                        )?)
                        .ok_or(LIMIT)?;
                }
                if usize::try_from(key(d.get(b"Count").map_err(|_| INVALID)?)?).ok() != Some(count)
                {
                    return Err(INVALID.into());
                }
                Ok(count)
            }
            _ => Err(INVALID.into()),
        }
    }
    let catalog = reference(doc.trailer.get(b"Root").map_err(|_| INVALID)?)?;
    let root = reference(dict(doc, catalog)?.get(b"Pages").map_err(|_| INVALID)?)?;
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    visit(doc, root, None, &mut out, &mut seen, 0, budget)?;
    Ok(out)
}
pub(super) fn supported_rect_annotation(annotation: &Dictionary) -> Result<(), String> {
    if annotation.has(b"QuadPoints") || annotation.has(b"AP") {
        return Err(UNSUPPORTED.into());
    }
    if let Ok(value) = annotation.get(b"Border") {
        let border = value.as_array().map_err(|_| UNSUPPORTED)?;
        if border.len() < 3 || super::super::number(&border[2]) != Some(0.) {
            return Err(UNSUPPORTED.into());
        }
    }
    if let Ok(value) = annotation.get(b"BS") {
        let d = value.as_dict().map_err(|_| UNSUPPORTED)?;
        if d.get(b"W").ok().and_then(super::super::number) != Some(0.) {
            return Err(UNSUPPORTED.into());
        }
    }
    Ok(())
}

pub(super) fn pages_and_annotations(
    doc: &Document,
    input: &Instrumented,
    budget: &mut Budget,
) -> Result<(Vec<Page>, Vec<Annotation>), String> {
    pages_and_annotations_with_witnesses(doc, input, &BTreeSet::new(), budget)
}

pub(super) fn pages_and_annotations_with_witnesses(
    doc: &Document,
    input: &Instrumented,
    witnesses: &BTreeSet<String>,
    budget: &mut Budget,
) -> Result<(Vec<Page>, Vec<Annotation>), String> {
    let known = input
        .nodes
        .iter()
        .map(|n| n.id.as_str())
        .collect::<BTreeSet<_>>();
    let prefix = super::super::prefix(&input.nonce);
    let mut pages = Vec::new();
    let mut annotations = Vec::new();
    let mut ids = BTreeSet::new();
    let mut keys = BTreeSet::new();
    let mut page_keys = BTreeSet::new();
    for (index, id) in page_ids(doc, budget)?.into_iter().enumerate() {
        let number = index as u32 + 1;
        budget.step(0)?;
        if pages.len() >= 100000 {
            return Err(LIMIT.into());
        }
        let media = super::super::media_box(doc, id)?;
        if media[2] - media[0] > 20000. || media[3] - media[1] > 20000. {
            return Err(UNSUPPORTED.into());
        }
        let mut ancestor = id;
        let mut seen = BTreeSet::new();
        for depth in 0..MAX_DEPTH {
            budget.step(depth)?;
            if !seen.insert(ancestor) {
                return Err(INVALID.into());
            }
            let d = dict(doc, ancestor)?;
            if d.get(b"Rotate")
                .ok()
                .is_some_and(|v| v.as_i64().ok() != Some(0))
                || d.get(b"UserUnit")
                    .ok()
                    .is_some_and(|v| super::super::number(v) != Some(1.))
            {
                return Err(UNSUPPORTED.into());
            }
            if let Ok(crop) = d.get(b"CropBox") {
                if rect(direct(doc, crop)?)? != media {
                    return Err(UNSUPPORTED.into());
                }
            }
            match d.get(b"Parent") {
                Ok(parent) => ancestor = reference(parent)?,
                Err(_) => break,
            }
            if depth + 1 == MAX_DEPTH {
                return Err(LIMIT.into());
            }
        }
        let page = dict(doc, id)?;
        let struct_parents = page.get(b"StructParents").ok().map(key).transpose()?;
        if struct_parents.is_some_and(|key| !page_keys.insert(key)) {
            return Err(INVALID.into());
        }
        let (values, array) = match page.get(b"Annots") {
            Ok(value) => (
                direct(doc, value)?.as_array().map_err(|_| INVALID)?.clone(),
                value.as_reference().ok(),
            ),
            Err(_) => (Vec::new(), None),
        };
        let mut annotation_ids = Vec::new();
        for value in values {
            budget.step(0)?;
            let aid = reference(&value)?;
            if !ids.insert(aid) {
                return Err(INVALID.into());
            }
            let a = dict(doc, aid)?;
            if a.get(b"Type").and_then(Object::as_name).ok() != Some(b"Annot")
                || a.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"Link")
            {
                return Err(UNSUPPORTED.into());
            }
            if a.get(b"P")
                .ok()
                .is_some_and(|v| v.as_reference().ok() != Some(id))
            {
                return Err(INVALID.into());
            }
            let parent = key(a.get(b"StructParent").map_err(|_| INVALID)?)?;
            if !keys.insert(parent) {
                return Err(INVALID.into());
            }
            let bounds = rect(direct(doc, a.get(b"Rect").map_err(|_| INVALID)?)?)?;
            let action = a.get(b"A").ok().and_then(|v| v.as_reference().ok());
            let action_dict = a
                .get(b"A")
                .ok()
                .map(|v| direct(doc, v))
                .transpose()?
                .and_then(|v| v.as_dict().ok());
            let uri = action_dict
                .filter(|d| d.get(b"S").and_then(Object::as_name).ok() == Some(b"URI"))
                .and_then(|d| d.get(b"URI").ok())
                .and_then(|v| v.as_str().ok());
            let temporary = if let Some(target) = uri
                .and_then(|v| std::str::from_utf8(v).ok())
                .and_then(|v| v.strip_prefix(&prefix))
            {
                if !known.contains(target) && !witnesses.contains(target) {
                    return Err(INVALID.into());
                }
                Some(target.to_string())
            } else {
                None
            };
            annotations.push(Annotation {
                id: aid,
                page: number,
                page_id: id,
                key: parent,
                rect: bounds,
                original: a.clone(),
                temporary,
                action,
            });
            annotation_ids.push(aid);
            if annotations.len() > MAX_LINKS {
                return Err(LIMIT.into());
            }
        }
        pages.push(Page {
            id,
            media,
            annotation_array: array,
            annotation_ids,
            struct_parents,
        });
    }
    if pages.is_empty() || keys.iter().any(|key| page_keys.contains(key)) {
        return Err(INVALID.into());
    }
    for (id, value) in &doc.objects {
        budget.step(0)?;
        if value
            .as_dict()
            .ok()
            .and_then(|d| d.get(b"Type").ok())
            .and_then(|v| v.as_name().ok())
            == Some(b"Annot")
            && !ids.contains(id)
        {
            return Err(INVALID.into());
        }
    }
    Ok((pages, annotations))
}

impl NumberTree {
    fn read(doc: &Document, root: &Object, budget: &mut Budget) -> Result<Self, String> {
        fn visit(
            doc: &Document,
            value: &Object,
            tree: &mut NumberTree,
            depth: usize,
            budget: &mut Budget,
        ) -> Result<Option<(i64, i64)>, String> {
            budget.step(depth)?;
            if let Some(id) = value.as_reference().ok() {
                if !tree.nodes.insert(id) {
                    return Err(INVALID.into());
                }
            }
            let d = direct(doc, value)?.as_dict().map_err(|_| INVALID)?;
            let mut bounds: Option<(i64, i64)> = None;
            for name in [b"Nums".as_slice(), b"Kids".as_slice(), b"Limits".as_slice()] {
                if let Some(array) = d.get(name).ok().and_then(|v| v.as_reference().ok()) {
                    if !tree.arrays.insert(array) {
                        return Err(INVALID.into());
                    }
                }
            }
            match (d.get(b"Nums").ok(), d.get(b"Kids").ok()) {
                (Some(nums), None) => {
                    let values = direct(doc, nums)?.as_array().map_err(|_| INVALID)?;
                    if values.len() % 2 != 0 {
                        return Err(INVALID.into());
                    }
                    let mut previous = None;
                    for pair in values.chunks(2) {
                        budget.step(depth)?;
                        let n = key(&pair[0])?;
                        if previous.is_some_and(|p| n <= p)
                            || tree.entries.insert(n, pair[1].clone()).is_some()
                        {
                            return Err(INVALID.into());
                        }
                        previous = Some(n);
                        bounds = Some((bounds.map_or(n, |b| b.0), n));
                        if tree.entries.len() > MAX_LINKS + 100000 {
                            return Err(LIMIT.into());
                        }
                    }
                }
                (None, Some(kids)) => {
                    let kids = direct(doc, kids)?.as_array().map_err(|_| INVALID)?;
                    if kids.is_empty() {
                        return Err(INVALID.into());
                    }
                    for child in kids {
                        reference(child)?;
                        let next = visit(doc, child, tree, depth + 1, budget)?.ok_or(INVALID)?;
                        if bounds.is_some_and(|b| next.0 <= b.1) {
                            return Err(INVALID.into());
                        }
                        bounds = Some((bounds.map_or(next.0, |b| b.0), next.1));
                    }
                }
                _ => return Err(INVALID.into()),
            }
            if let Ok(limits) = d.get(b"Limits") {
                let values = direct(doc, limits)?.as_array().map_err(|_| INVALID)?;
                if values.len() != 2 || Some((key(&values[0])?, key(&values[1])?)) != bounds {
                    return Err(INVALID.into());
                }
            }
            Ok(bounds)
        }
        let mut tree = Self {
            root: root.as_reference().ok(),
            nodes: BTreeSet::new(),
            arrays: BTreeSet::new(),
            entries: BTreeMap::new(),
        };
        visit(doc, root, &mut tree, 0, budget)?;
        Ok(tree)
    }
}

impl Graph {
    /// The graph and the paint stream must describe precisely the same MCIDs.
    /// This inventory is identity only; no glyph/text/annotation rectangle is
    /// used to infer a content owner's source block.
    pub(super) fn paint_inventory(&self) -> Result<BTreeMap<(ObjectId, i64), ObjectId>, String> {
        let mut out = BTreeMap::new();
        for tag in self.tags.values() {
            for kid in &tag.kids {
                if let Kind::Content(content) = &kid.kind {
                    if content.form
                        || out
                            .insert((content.page.ok_or(INVALID)?, content.mcid), content.tag)
                            .is_some()
                    {
                        return Err(UNSUPPORTED.into());
                    }
                }
            }
        }
        Ok(out)
    }

    pub(super) fn read(
        doc: &Document,
        pages: &[Page],
        budget: &mut Budget,
    ) -> Result<Self, String> {
        let catalog = reference(doc.trailer.get(b"Root").map_err(|_| INVALID)?)?;
        let root = reference(
            dict(doc, catalog)?
                .get(b"StructTreeRoot")
                .map_err(|_| INVALID)?,
        )?;
        let d = dict(doc, root)?;
        if d.get(b"Type").and_then(Object::as_name).ok() != Some(b"StructTreeRoot") {
            return Err(INVALID.into());
        }
        if let Ok(value) = d.get(b"RoleMap") {
            let roles = direct(doc, value)?.as_dict().map_err(|_| INVALID)?;
            // Typst 0.15.1's fixed aliases cannot redefine Link/P or create
            // another path to an owner. Unknown role substitution is refused.
            for (name, target) in roles.iter() {
                let allowed = match name.as_slice() {
                    b"Datetime" | b"Strong" | b"Em" => b"Span".as_slice(),
                    b"Terms" => b"Part".as_slice(),
                    b"Title" => b"P".as_slice(),
                    _ => return Err(UNSUPPORTED.into()),
                };
                if target.as_name().ok() != Some(allowed) {
                    return Err(UNSUPPORTED.into());
                }
            }
        }
        let tree = NumberTree::read(doc, d.get(b"ParentTree").map_err(|_| INVALID)?, budget)?;
        let mut graph = Self {
            root,
            tags: BTreeMap::new(),
            objrs: BTreeMap::new(),
            tree,
            k_arrays: BTreeMap::new(),
            page_keys: BTreeSet::new(),
            mcid_parents: BTreeMap::new(),
        };
        let mut seen = BTreeSet::new();
        let root_kids = graph.kids(
            doc,
            d.get(b"K").map_err(|_| INVALID)?,
            root,
            None,
            0,
            &mut seen,
            budget,
        )?;
        if root_kids.is_empty()
            || root_kids
                .iter()
                .any(|kid| !matches!(&kid.kind, Kind::Tag(_)))
        {
            return Err(INVALID.into());
        }
        for page in pages {
            if let Some(key) = page.struct_parents {
                if !graph.page_keys.insert(key) {
                    return Err(INVALID.into());
                }
                let values = direct(doc, graph.tree.entries.get(&key).ok_or(INVALID)?)?
                    .as_array()
                    .map_err(|_| INVALID)?;
                let mut parents = Vec::new();
                for value in values {
                    budget.step(0)?;
                    parents.push(match value {
                        Object::Null => None,
                        Object::Reference(id) if graph.tags.contains_key(id) => Some(*id),
                        _ => return Err(INVALID.into()),
                    });
                }
                graph.mcid_parents.insert(page.id, parents);
            }
        }
        let page_ids = pages.iter().map(|p| p.id).collect::<BTreeSet<_>>();
        if graph
            .objrs
            .values()
            .any(|objr| !page_ids.contains(&objr.page))
        {
            return Err(INVALID.into());
        }
        // Both directions matter: an MCID present in ParentTree but omitted
        // from K is evidence of unrepresented visible content, not coverage.
        let mut content_keys = BTreeSet::new();
        for tag in graph.tags.values() {
            for kid in &tag.kids {
                if let Kind::Content(value) = &kid.kind {
                    budget.step(0)?;
                    if value.form {
                        return Err(UNSUPPORTED.into());
                    }
                    let page = value.page.ok_or(INVALID)?;
                    let index = usize::try_from(value.mcid).map_err(|_| INVALID)?;
                    if !content_keys.insert((page, index))
                        || graph
                            .mcid_parents
                            .get(&page)
                            .and_then(|values| values.get(index))
                            .copied()
                            .flatten()
                            != Some(value.tag)
                    {
                        return Err(INVALID.into());
                    }
                }
            }
        }
        for (page, values) in &graph.mcid_parents {
            for (index, value) in values.iter().enumerate() {
                budget.step(0)?;
                if value.is_some() && !content_keys.contains(&(*page, index)) {
                    return Err(INVALID.into());
                }
            }
        }
        // Unreachable tags/OBJRs cannot supply a second, unchecked owner.
        let reached_objrs = graph
            .objrs
            .values()
            .filter_map(|o| o.indirect)
            .collect::<BTreeSet<_>>();
        for (id, object) in &doc.objects {
            budget.step(0)?;
            if let Ok(d) = object.as_dict() {
                match d.get(b"Type").and_then(Object::as_name).ok() {
                    Some(b"StructElem") if !graph.tags.contains_key(id) => {
                        return Err(INVALID.into())
                    }
                    Some(b"OBJR") if !reached_objrs.contains(id) => return Err(INVALID.into()),
                    _ => (),
                }
            }
        }
        Ok(graph)
    }
    fn tag(
        &mut self,
        doc: &Document,
        id: ObjectId,
        parent: ObjectId,
        inherited: Option<ObjectId>,
        depth: usize,
        seen: &mut BTreeSet<ObjectId>,
        budget: &mut Budget,
    ) -> Result<(), String> {
        budget.step(depth)?;
        if !seen.insert(id) || self.tags.len() >= MAX_LINKS {
            return Err(INVALID.into());
        }
        let d = dict(doc, id)?;
        if reference(d.get(b"P").map_err(|_| INVALID)?)? != parent {
            return Err(INVALID.into());
        }
        let role = d
            .get(b"S")
            .and_then(Object::as_name)
            .map_err(|_| INVALID)?
            .to_vec();
        let page = d.get(b"Pg").ok().map(reference).transpose()?.or(inherited);
        let kids = match d.get(b"K") {
            Ok(value) => self.kids(doc, value, id, page, depth + 1, seen, budget)?,
            Err(_) => Vec::new(),
        };
        self.tags.insert(id, Tag { parent, role, kids });
        Ok(())
    }
    fn kids(
        &mut self,
        doc: &Document,
        value: &Object,
        owner: ObjectId,
        page: Option<ObjectId>,
        depth: usize,
        seen: &mut BTreeSet<ObjectId>,
        budget: &mut Budget,
    ) -> Result<Vec<Kid>, String> {
        budget.step(depth)?;
        let indirect = value.as_reference().ok();
        let object = direct(doc, value)?;
        if let Object::Array(values) = object {
            if let Some(id) = indirect {
                if self.k_arrays.insert(id, owner).is_some() {
                    return Err(INVALID.into());
                }
            }
            let mut out = Vec::new();
            for item in values {
                out.extend(self.kids(doc, item, owner, page, depth + 1, seen, budget)?);
            }
            return Ok(out);
        }
        let kind = match object {
            Object::Integer(n) if *n >= 0 => Kind::Content(Content {
                page,
                mcid: *n,
                tag: owner,
                form: false,
            }),
            Object::Dictionary(d) => match d
                .get(b"Type")
                .and_then(Object::as_name)
                .map_err(|_| INVALID)?
            {
                b"StructElem" => {
                    let id = indirect.ok_or(INVALID)?;
                    self.tag(doc, id, owner, page, depth + 1, seen, budget)?;
                    Kind::Tag(id)
                }
                b"OBJR" => {
                    let target = reference(d.get(b"Obj").map_err(|_| INVALID)?)?;
                    let pg = reference(d.get(b"Pg").map_err(|_| INVALID)?)?;
                    if self
                        .objrs
                        .insert(
                            target,
                            Objr {
                                tag: owner,
                                page: pg,
                                indirect,
                            },
                        )
                        .is_some()
                    {
                        return Err(INVALID.into());
                    }
                    Kind::Objr(target)
                }
                b"MCR" => Kind::Content(Content {
                    page: d.get(b"Pg").ok().map(reference).transpose()?.or(page),
                    mcid: key(d.get(b"MCID").map_err(|_| INVALID)?)?,
                    tag: owner,
                    form: d.has(b"Stm"),
                }),
                _ => return Err(INVALID.into()),
            },
            _ => return Err(INVALID.into()),
        };
        Ok(vec![Kid {
            value: value.clone(),
            kind,
        }])
    }
    pub(super) fn check_annotations(&self, annotations: &[Annotation]) -> Result<(), String> {
        if annotations.len() != self.objrs.len() {
            return Err(INVALID.into());
        }
        for annotation in annotations {
            let objr = self.objrs.get(&annotation.id).ok_or(INVALID)?;
            if objr.page != annotation.page_id
                || self.tags.get(&objr.tag).ok_or(INVALID)?.role != b"Link"
                || self.page_keys.contains(&annotation.key)
                || self
                    .tree
                    .entries
                    .get(&annotation.key)
                    .and_then(|v| v.as_reference().ok())
                    != Some(objr.tag)
            {
                return Err(INVALID.into());
            }
        }
        // No unknown number-tree owner (including unproved form content) is
        // accepted in this qualified page/annotation protocol.
        let keys = annotations.iter().map(|a| a.key).collect::<BTreeSet<_>>();
        if self.tree.entries.len() != keys.len() + self.page_keys.len()
            || self
                .tree
                .entries
                .keys()
                .any(|key| !keys.contains(key) && !self.page_keys.contains(key))
        {
            return Err(INVALID.into());
        }
        Ok(())
    }
    pub(super) fn nearest_paragraph(&self, mut id: ObjectId) -> Result<Option<ObjectId>, String> {
        for _ in 0..MAX_DEPTH {
            if id == self.root {
                return Ok(None);
            }
            let tag = self.tags.get(&id).ok_or(INVALID)?;
            if tag.role == b"P" {
                return Ok(Some(id));
            }
            id = tag.parent;
        }
        Err(LIMIT.into())
    }
    pub(super) fn wrapper_paragraph(
        &self,
        wrapper: ObjectId,
        budget: &mut Budget,
    ) -> Result<ObjectId, String> {
        let mut paragraphs = self
            .nearest_paragraph(wrapper)?
            .into_iter()
            .collect::<BTreeSet<_>>();
        let mut pending = vec![wrapper];
        while let Some(id) = pending.pop() {
            budget.step(0)?;
            let tag = self.tags.get(&id).ok_or(INVALID)?;
            if tag.role == b"P" {
                paragraphs.insert(id);
            }
            pending.extend(tag.kids.iter().filter_map(|kid| {
                if let Kind::Tag(id) = &kid.kind {
                    Some(*id)
                } else {
                    None
                }
            }));
        }
        if paragraphs.len() != 1 {
            return Err(UNSUPPORTED.into());
        }
        paragraphs.into_iter().next().ok_or_else(|| INVALID.into())
    }
    fn content(
        &self,
        id: ObjectId,
        out: &mut Vec<Content>,
        depth: usize,
        budget: &mut Budget,
    ) -> Result<(), String> {
        budget.step(depth)?;
        let tag = self.tags.get(&id).ok_or(INVALID)?;
        if !matches!(tag.role.as_slice(), b"Span" | b"Strong" | b"Em") {
            return Err(UNSUPPORTED.into());
        }
        for kid in &tag.kids {
            match &kid.kind {
                Kind::Content(value) => out.push(value.clone()),
                Kind::Tag(child) => self.content(*child, out, depth + 1, budget)?,
                Kind::Objr(_) => return Err(UNSUPPORTED.into()),
            }
        }
        Ok(())
    }
    pub(super) fn check_link_fragments(
        &self,
        id: ObjectId,
        temporary: &BTreeSet<ObjectId>,
        budget: &mut Budget,
    ) -> Result<(), String> {
        let mut content = Vec::new();
        let mut covered = 0usize;
        let mut seen = BTreeSet::new();
        for kid in &self.tags.get(&id).ok_or(INVALID)?.kids {
            budget.step(0)?;
            match &kid.kind {
                Kind::Content(value) => content.push(value.clone()),
                Kind::Tag(child) => self.content(*child, &mut content, 0, budget)?,
                Kind::Objr(annotation) => {
                    let objr = self.objrs.get(annotation).ok_or(INVALID)?;
                    if temporary.contains(annotation) {
                        if objr.tag != id { return Err(INVALID.into()); }
                        continue;
                    }
                    if objr.tag != id || content.is_empty() {
                        return Err(INVALID.into());
                    }
                    for value in content.drain(..) {
                        if value.form {
                            return Err(UNSUPPORTED.into());
                        }
                        if value.page != Some(objr.page) || !seen.insert((objr.page, value.mcid)) {
                            return Err(INVALID.into());
                        }
                        let parent = self
                            .mcid_parents
                            .get(&objr.page)
                            .and_then(|a| usize::try_from(value.mcid).ok().and_then(|n| a.get(n)))
                            .copied()
                            .flatten();
                        if parent != Some(value.tag) {
                            return Err(INVALID.into());
                        }
                    }
                    covered += 1;
                }
            }
        }
        if covered == 0 || !content.is_empty() {
            return Err(INVALID.into());
        }
        Ok(())
    }
    /// For the paint-qualified path, styled native links legitimately put
    /// OBJR before their Strong/Em subtree. Preserve exact structural owner
    /// identity and let the actual glyph/advance proof establish a unique
    /// annotation fragment, instead of guessing ownership from K order.
    pub(super) fn link_content_inventory(
        &self,
        id: ObjectId,
        temporary: &BTreeSet<ObjectId>,
        budget: &mut Budget,
    ) -> Result<Vec<(ObjectId, i64)>, String> {
        let mut content = Vec::new();
        let mut annotations = 0;
        for kid in &self.tags.get(&id).ok_or(INVALID)?.kids {
            budget.step(0)?;
            match &kid.kind {
                Kind::Content(value) => content.push(value.clone()),
                Kind::Tag(child) => self.content(*child, &mut content, 0, budget)?,
                Kind::Objr(annotation) => {
                    if self.objrs.get(annotation).ok_or(INVALID)?.tag != id {
                        return Err(INVALID.into());
                    }
                    if !temporary.contains(annotation) { annotations += 1; }
                }
            }
        }
        if content.is_empty() || annotations == 0 {
            return Err(INVALID.into());
        }
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for c in content {
            let page = c.page.ok_or(INVALID)?;
            if c.form {
                return Err(UNSUPPORTED.into());
            }
            if !seen.insert((page, c.mcid)) {
                return Err(INVALID.into());
            }
            if self
                .mcid_parents
                .get(&page)
                .and_then(|a| usize::try_from(c.mcid).ok().and_then(|n| a.get(n)))
                .copied()
                .flatten()
                != Some(c.tag)
            {
                return Err(INVALID.into());
            }
            out.push((page, c.mcid));
        }
        Ok(out)
    }
    pub(super) fn ordinary_link_ancestry(
        &self,
        link: ObjectId,
        wrapper: ObjectId,
    ) -> Result<(), String> {
        let mut id = self.tags.get(&link).ok_or(INVALID)?.parent;
        for _ in 0..MAX_DEPTH {
            if id == wrapper {
                return Ok(());
            }
            let tag = self.tags.get(&id).ok_or(INVALID)?;
            if !matches!(tag.role.as_slice(), b"P" | b"Span" | b"Strong" | b"Em") {
                return Err(UNSUPPORTED.into());
            }
            id = tag.parent;
        }
        Err(LIMIT.into())
    }
}
