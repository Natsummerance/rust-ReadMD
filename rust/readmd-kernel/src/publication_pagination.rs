//! Exact source identity carried through typesetting. Temporary PDF link
//! rectangles provide engine geometry, then are removed from the deliverable,
//! including their accessibility references. No user source is rewritten.
use crate::publication_blocks::{self, Node};
use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};

pub struct Instrumented {
    pub content: String,
    pub nonce: String,
    pub revision: String,
    pub nodes: Vec<Node>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Hit {
    pub id: String,
    pub page: u32,
    pub rect: [f64; 4], // x, y, width, height; points from physical page top-left
}
#[derive(Clone, Debug, Serialize)]
pub struct PageMap {
    pub revision: String,
    pub nodes: Vec<Node>,
    pub hits: Vec<Hit>,
    pub pages: Vec<[f64; 2]>,
}
pub fn prefix(nonce: &str) -> String {
    format!("readmd-internal:publication/{nonce}/")
}
pub fn marker_id<'a>(comment: &'a str, nonce: &str) -> Option<&'a str> {
    let value = comment.trim().strip_prefix("<!-- readmd:page-source ")?;
    let (scope, id) = value.strip_suffix(" -->")?.split_once(' ')?;
    (scope == nonce
        && !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)))
    .then_some(id)
}
pub(crate) fn is_marker(comment: &str) -> bool {
    let Some(value) = comment.trim().strip_prefix("<!-- readmd:page-source ") else {
        return false;
    };
    let Some((scope, _)) = value.split_once(' ') else {
        return false;
    };
    scope.len() == 32
        && scope.bytes().all(|b| b.is_ascii_hexdigit())
        && marker_id(comment, scope).is_some()
}
pub fn instrument(source: &str) -> Result<Instrumented, String> {
    let model = publication_blocks::analyze(source).map_err(str::to_string)?;
    if let Some(d) = model.diagnostics.first() {
        return Err(d["code"]
            .as_str()
            .unwrap_or("publication_local_style_invalid")
            .into());
    }
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let nodes = model
        .nodes
        .into_iter()
        .filter(|n| n.editable)
        .collect::<Vec<_>>();
    let mut content = source.to_string();
    let eol = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    for node in nodes.iter().rev() {
        if node.nested {
            let at=node.directive.as_ref().map_or(node.inline_content.end,|r|r.start);
            content.insert_str(at,&format!("<!-- readmd:page-source {nonce} {} -->",node.id));
            continue;
        }
        let at = node
            .directive
            .as_ref()
            .map_or(node.bytes.start, |r| r.start);
        let start = source[..at]
            .rfind('\n')
            .map_or(if source.starts_with('\u{feff}') { 3 } else { 0 }, |n| {
                n + 1
            });
        content.insert_str(
            start,
            &format!("<!-- readmd:page-source {nonce} {} -->{eol}{eol}", node.id),
        );
    }
    Ok(Instrumented {
        content,
        nonce,
        revision: model.revision,
        nodes,
    })
}
fn resolved<'a>(doc: &'a Document, value: &'a Object) -> Option<&'a Object> {
    match value {
        Object::Reference(id) => doc.get_object(*id).ok(),
        other => Some(other),
    }
}
fn dictionary<'a>(doc: &'a Document, value: &'a Object) -> Option<&'a Dictionary> {
    resolved(doc, value)?.as_dict().ok()
}
fn number(value: &Object) -> Option<f64> {
    match value {
        Object::Integer(n) => Some(*n as f64),
        Object::Real(n) => Some(*n as f64),
        _ => None,
    }
}
fn media_box(doc: &Document, mut id: ObjectId) -> Result<[f64; 4], String> {
    for _ in 0..16 {
        let d = doc.get_dictionary(id).map_err(|_| "typst_invalid_output")?;
        if let Ok(value) = d.get(b"MediaBox") {
            let a = resolved(doc, value)
                .and_then(|v| v.as_array().ok())
                .ok_or("typst_invalid_output")?;
            let values = a
                .iter()
                .map(number)
                .collect::<Option<Vec<_>>>()
                .ok_or("typst_invalid_output")?;
            if values.len() == 4
                && values.iter().all(|n| n.is_finite())
                && values[2] > values[0]
                && values[3] > values[1]
            {
                return Ok([values[0], values[1], values[2], values[3]]);
            }
            return Err("typst_invalid_output".into());
        }
        id = d
            .get(b"Parent")
            .and_then(Object::as_reference)
            .map_err(|_| "typst_invalid_output")?;
    }
    Err("typst_invalid_output".into())
}
fn objr_target(doc: &Document, value: &Object) -> Option<ObjectId> {
    let d = dictionary(doc, value)?;
    (d.get(b"Type").ok()?.as_name().ok()? == b"OBJR")
        .then(|| d.get(b"Obj").ok()?.as_reference().ok())
        .flatten()
}
fn annotation_hit(
    annotation: &Dictionary,
    media: [f64; 4],
    target: &str,
    page: u32,
) -> Result<Option<Hit>, String> {
    let r = annotation
        .get(b"Rect")
        .ok()
        .and_then(|v| v.as_array().ok())
        .and_then(|a| a.iter().map(number).collect::<Option<Vec<_>>>())
        .filter(|v| v.len() == 4 && v.iter().all(|n| n.is_finite()))
        .ok_or("typst_invalid_output")?;
    let left = r[0].min(r[2]).max(media[0]);
    let right = r[0].max(r[2]).min(media[2]);
    let low = r[1].min(r[3]).max(media[1]);
    let high = r[1].max(r[3]).min(media[3]);
    Ok((right > left && high > low).then(|| Hit {
        id: target.into(),
        page,
        rect: [left - media[0], media[3] - high, right - left, high - low],
    }))
}
fn parent_tree_ids(doc: &Document) -> BTreeSet<ObjectId> {
    let mut result = BTreeSet::new();
    let root = doc
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|v| dictionary(doc, v));
    let parent = root
        .and_then(|d| d.get(b"StructTreeRoot").ok())
        .and_then(|v| dictionary(doc, v))
        .and_then(|d| d.get(b"ParentTree").ok())
        .and_then(|v| v.as_reference().ok());
    let mut pending = parent.into_iter().collect::<Vec<_>>();
    while let Some(id) = pending.pop() {
        if !result.insert(id) || result.len() > 32768 {
            continue;
        }
        if let Ok(d) = doc.get_dictionary(id) {
            if let Ok(Object::Array(kids)) = d.get(b"Kids") {
                pending.extend(kids.iter().filter_map(|v| v.as_reference().ok()));
            }
        }
    }
    result
}
pub fn extract_and_clean(bytes: &[u8], input: Instrumented) -> Result<(Vec<u8>, PageMap), String> {
    let mut doc = Document::load_mem(bytes).map_err(|_| "typst_invalid_output")?;
    let prefix = prefix(&input.nonce);
    let known = input
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n))
        .collect::<HashMap<_, _>>();
    let mut hits = Vec::new();
    let mut removed = BTreeSet::new();
    let mut parents = BTreeSet::new();
    let mut pages = Vec::new();
    let mut page_annotations = Vec::new();
    let mut annotation_targets = HashMap::new();
    for (page, id) in doc.get_pages() {
        let media = media_box(&doc, id)?;
        let width = media[2] - media[0];
        let height = media[3] - media[1];
        pages.push([width, height]);
        let d = doc.get_dictionary(id).map_err(|_| "typst_invalid_output")?;
        let annotations = d
            .get(b"Annots")
            .ok()
            .and_then(|v| resolved(&doc, v))
            .and_then(|v| v.as_array().ok())
            .cloned()
            .unwrap_or_default();
        let mut keep = Vec::new();
        for value in annotations {
            let a = dictionary(&doc, &value);
            let action = a
                .and_then(|d| d.get(b"A").ok())
                .and_then(|v| dictionary(&doc, v));
            let uri = action
                .and_then(|d| d.get(b"URI").ok())
                .and_then(|v| v.as_str().ok())
                .and_then(|v| std::str::from_utf8(v).ok());
            if let Some(target) = uri.and_then(|v| v.strip_prefix(&prefix)) {
                if !known.contains_key(target) {
                    return Err("typst_invalid_output".into());
                }
                let a = a.ok_or("typst_invalid_output")?;
                if let Some(hit) = annotation_hit(a, media, target, page)? {
                    hits.push(hit);
                }
                if hits.len() > 131072 {
                    return Err("typst_output_limit".into());
                }
                if let Ok(id) = value.as_reference() {
                    removed.insert(id);
                    annotation_targets.insert(id, target.to_string());
                }
                if let Ok(n) = a.get(b"StructParent").and_then(Object::as_i64) {
                    parents.insert(n);
                }
            } else {
                keep.push(value);
            }
        }
        page_annotations.push((id, keep));
    }
    // A real hyperlink overrides the outer temporary URI over its own glyphs.
    // Recover those rectangles through the PDF's actual tagged parent chain,
    // keeping the original action intact rather than guessing from text.
    let mut wrappers = HashMap::new();
    let mut original_structures = Vec::new();
    for (id, object) in &doc.objects {
        if let Ok(d) = object.as_dict() {
            if d.get(b"Type").ok().and_then(|v| v.as_name().ok()) != Some(b"StructElem") {
                continue;
            }
            if let Ok(k) = d.get(b"K") {
                let values = match k {
                    Object::Array(a) => a.clone(),
                    v => vec![v.clone()],
                };
                for v in values {
                    if let Some(annotation) = objr_target(&doc, &v) {
                        if let Some(target) = annotation_targets.get(&annotation) {
                            wrappers.insert(*id, target.clone());
                        } else {
                            original_structures.push((annotation, *id));
                        }
                    }
                }
            }
        }
    }
    let mut original_targets = HashMap::new();
    for (annotation, mut element) in original_structures {
        let mut seen = BTreeSet::new();
        for _ in 0..64 {
            if !seen.insert(element) {
                break;
            }
            if let Some(target) = wrappers.get(&element) {
                original_targets.insert(annotation, target.clone());
                break;
            }
            let Some(parent) = doc
                .get_dictionary(element)
                .ok()
                .and_then(|d| d.get(b"P").ok())
                .and_then(|v| v.as_reference().ok())
            else {
                break;
            };
            element = parent;
        }
    }
    for (page, (id, annotations)) in page_annotations.iter().enumerate() {
        let media = media_box(&doc, *id)?;
        for value in annotations {
            if let Some(target) = value
                .as_reference()
                .ok()
                .and_then(|id| original_targets.get(&id))
            {
                if let Some(hit) = annotation_hit(
                    dictionary(&doc, value).ok_or("typst_invalid_output")?,
                    media,
                    target,
                    page as u32 + 1,
                )? {
                    hits.push(hit);
                }
            }
        }
    }
    if hits.len() > 131072 {
        return Err("typst_output_limit".into());
    }
    let trees = parent_tree_ids(&doc);
    let mut cleaned = Vec::new();
    for (id, object) in &doc.objects {
        if let Ok(d) = object.as_dict() {
            if let Ok(k) = d.get(b"K") {
                let values = match k {
                    Object::Array(a) => a.clone(),
                    v => vec![v.clone()],
                };
                if values
                    .iter()
                    .any(|v| objr_target(&doc, v).is_some_and(|id| removed.contains(&id)))
                {
                    let mut next = d.clone();
                    let retained = values
                        .into_iter()
                        .filter(|v| !objr_target(&doc, v).is_some_and(|id| removed.contains(&id)))
                        .collect::<Vec<_>>();
                    if retained.is_empty() {
                        next.remove(b"K");
                    } else {
                        next.set("K", retained);
                    }
                    if next.get(b"S").ok().and_then(|v| v.as_name().ok()) == Some(b"Link") {
                        next.set("S", Object::Name(b"Span".to_vec()));
                    }
                    cleaned.push((*id, Object::Dictionary(next)));
                }
            }
        }
    }
    for (id, value) in cleaned {
        doc.objects.insert(id, value);
    }
    for id in trees {
        if let Ok(d) = doc.get_object_mut(id).and_then(Object::as_dict_mut) {
            if let Ok(Object::Array(nums)) = d.get_mut(b"Nums") {
                let kept = nums
                    .chunks(2)
                    .filter(|pair| {
                        pair.len() == 2
                            && !pair[0].as_i64().ok().is_some_and(|n| parents.contains(&n))
                    })
                    .flat_map(|p| p.iter().cloned())
                    .collect();
                *nums = kept;
            }
        }
    }
    for (id, annotations) in page_annotations {
        let d = doc
            .get_object_mut(id)
            .and_then(Object::as_dict_mut)
            .map_err(|_| "typst_invalid_output")?;
        if annotations.is_empty() {
            d.remove(b"Annots");
        } else {
            d.set("Annots", annotations);
        }
    }
    // Remove detached OBJR objects too; none may retain an annotation reference.
    let objrs = doc
        .objects
        .iter()
        .filter_map(|(id, v)| {
            objr_target(&doc, v)
                .filter(|target| removed.contains(target))
                .map(|_| *id)
        })
        .collect::<Vec<_>>();
    for id in removed.into_iter().chain(objrs) {
        doc.objects.remove(&id);
    }
    let mut output = Vec::new();
    doc.save_to(&mut output)
        .map_err(|_| "typst_invalid_output")?;
    Ok((
        output,
        PageMap {
            revision: input.revision,
            nodes: input.nodes,
            hits,
            pages,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn markers_preserve_duplicate_identity_and_precede_local_attributes() {
        let source = "\u{feff}# Title\r\n\r\nSame.\r\n\r\nSame.\r\n";
        let first = publication_blocks::analyze(source).unwrap();
        let styled = publication_blocks::apply(
            source,
            &first.revision,
            &first.nodes[2].id,
            &serde_json::json!({"text":{"size":16}}),
        )
        .unwrap();
        let mapped = instrument(styled["result"]["content"].as_str().unwrap()).unwrap();
        assert_eq!(mapped.nodes.len(), 3);
        assert_ne!(mapped.nodes[1].id, mapped.nodes[2].id);
        assert!(mapped.content.starts_with('\u{feff}'));
        let doc = crate::md_ast::parse(&mapped.content);
        assert!(doc.blocks.windows(2).any(|p|matches!(&p[0],crate::md_ast::Block::Html(s)if marker_id(s,&mapped.nonce)==Some(mapped.nodes[2].id.as_str()))&&matches!(&p[1],crate::md_ast::Block::Styled{..})));
    }
    #[test]
    fn untrusted_markers_cannot_claim_an_identity_and_nested_paragraphs_are_mapped() {
        let mapped =
            instrument("> Nested.\n\nSame.\n\n<!-- readmd:page-source attacker fake -->\n")
                .unwrap();
        assert_eq!(mapped.nodes.len(), 2);
        assert!(mapped.nodes[0].nested);
        assert!(mapped.content.starts_with("> Nested.<!-- readmd:page-source "));
        assert_eq!(
            marker_id("<!-- readmd:page-source attacker fake -->", &mapped.nonce),
            None
        );
    }
    #[test]
    fn nested_markers_preserve_tight_structure_and_bind_styled_content_once(){
        let source="> Repeated **text**\n>\n> - Repeated **text**\n>   - Child [link](https://example.invalid)\n";
        let model=publication_blocks::analyze(source).unwrap();let selected=&model.nodes[1];
        let result=publication_blocks::apply(source,&model.revision,&selected.id,&serde_json::json!({"text":{"size":16}})).unwrap();let updated=result["result"]["content"].as_str().unwrap();let mapped=instrument(updated).unwrap();assert_eq!(mapped.nodes.len(),3);
        assert!(mapped.content.contains(&format!("{} --><!-- readmd:layout",selected.id)));
        let doc=crate::md_ast::parse(&mapped.content);let style=crate::export_styles::sanitize(&serde_json::json!({}));let dir=tempfile::tempdir().unwrap();let rendered=crate::typst_writer::render_mapped(&doc,&style,"",dir.path(),"nested",&mapped.nonce).unwrap();
        for node in &mapped.nodes {assert_eq!(rendered.text.matches(&format!("link(\"{}{}\")",prefix(&mapped.nonce),node.id)).count(),1,"{}",node.title);}
        assert!(!rendered.text.contains("readmd:page-source"));assert!(rendered.text.contains("size:16pt"));assert!(rendered.text.contains("https://example.invalid"));
    }
    #[test]
    fn cleanup_preserves_real_links_tags_and_page_labels_without_private_actions() {
        use lopdf::dictionary;
        let input = instrument("Same.\n\nSame.\n").unwrap();
        let target = input.nodes[1].id.clone();
        let uri = format!("{}{}", prefix(&input.nonce), target);
        let mut doc = Document::with_version("1.7");
        let page = doc.new_object_id();
        let pages = doc.new_object_id();
        let temporary=doc.add_object(dictionary!{"Type"=>"Annot","Subtype"=>"Link","Rect"=>vec![15.into(),40.into(),75.into(),55.into()],"A"=>dictionary!{"S"=>"URI","URI"=>Object::string_literal(uri.as_bytes())},"StructParent"=>0});
        let original=doc.add_object(dictionary!{"Type"=>"Annot","Subtype"=>"Link","Rect"=>vec![80.into(),40.into(),90.into(),55.into()],"A"=>dictionary!{"S"=>"URI","URI"=>Object::string_literal("https://example.test")},"StructParent"=>1});
        let structure=doc.add_object(dictionary!{"Type"=>"StructElem","S"=>"Link","K"=>vec![Object::Dictionary(dictionary!{"Type"=>"OBJR","Obj"=>temporary}),0.into()]});
        let real_structure=doc.add_object(dictionary!{"Type"=>"StructElem","S"=>"Link","P"=>structure,"K"=>Object::Dictionary(dictionary!{"Type"=>"OBJR","Obj"=>original})});
        let parents = doc.add_object(
            dictionary! {"Nums"=>vec![0.into(),structure.into(),1.into(),real_structure.into()]},
        );
        let tree=doc.add_object(dictionary!{"Type"=>"StructTreeRoot","ParentTree"=>parents,"K"=>vec![Object::Reference(structure),real_structure.into()]});
        let labels = doc.add_object(
            dictionary! {"Nums"=>vec![0.into(),Object::Dictionary(dictionary!{"S"=>"D"})]},
        );
        doc.objects.insert(page,Object::Dictionary(dictionary!{"Type"=>"Page","Parent"=>pages,"Annots"=>vec![Object::Reference(temporary),original.into()]}));
        doc.objects.insert(pages,Object::Dictionary(dictionary!{"Type"=>"Pages","Kids"=>vec![Object::Reference(page)],"Count"=>1,"MediaBox"=>vec![10.into(),20.into(),210.into(),320.into()]}));
        let root=doc.add_object(dictionary!{"Type"=>"Catalog","Pages"=>pages,"StructTreeRoot"=>tree,"PageLabels"=>labels});
        doc.trailer.set("Root", root);
        let mut raw = Vec::new();
        doc.save_to(&mut raw).unwrap();
        let (bytes, map) = extract_and_clean(&raw, input).unwrap();
        assert_eq!(map.hits.len(), 2);
        assert_eq!(map.hits[0].id, target);
        assert_eq!(map.hits[0].rect, [5., 265., 60., 15.]);
        assert_eq!(map.hits[1].rect, [70., 265., 10., 15.]);
        let cleaned = Document::load_mem(&bytes).unwrap();
        assert!(!cleaned.objects.contains_key(&temporary));
        assert!(cleaned.objects.contains_key(&original));
        assert_eq!(
            cleaned
                .get_dictionary(structure)
                .unwrap()
                .get(b"S")
                .unwrap()
                .as_name()
                .unwrap(),
            b"Span"
        );
        assert_eq!(
            cleaned
                .get_dictionary(structure)
                .unwrap()
                .get(b"K")
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            cleaned
                .get_dictionary(parents)
                .unwrap()
                .get(b"Nums")
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            cleaned
                .get_dictionary(labels)
                .unwrap()
                .get(b"Nums")
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(!format!("{:?}", cleaned.objects).contains(&uri));
    }
    #[test]
    fn preparation_preserves_equation_caption_and_reference_semantics() {
        let input = instrument(
            "```readmd-math v1\nfrac(a,b)\n```\n\n:label[eq:ratio] Ratio\n\nSee :ref[eq:ratio].\n",
        )
        .unwrap();
        let (source, warnings) = crate::mdexport::prepare_export_source(
            "pdf",
            &input.content,
            "",
            &serde_json::json!({}),
        )
        .unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let doc = crate::md_ast::parse(&source);
        assert!(doc
            .blocks
            .iter()
            .any(|b| matches!(b, crate::md_ast::Block::Caption { .. })));
        assert!(format!("{doc:?}").contains("Equation 1"));
    }
}
