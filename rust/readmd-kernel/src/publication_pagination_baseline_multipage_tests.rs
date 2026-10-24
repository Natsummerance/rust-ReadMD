use super::*;
use lopdf::dictionary;
use sha2::{Digest, Sha256};

// Exact locked Typst 0.15.1 raw output from the retained C71 #067 request.
// It has 3 pages, 3 logical user links, 53 real fragments and 6 private OBJRs.
// Two private OBJRs are attached to descendant real Link elements at page
// breaks. All source/MCID/ParentTree identities remain original, not invented.
const RAW: &[u8] = include_bytes!("../tests/fixtures/typst-baseline-multipage-native.pdf");
const MAPPING: &[u8] = include_bytes!("../tests/fixtures/typst-baseline-multipage-native.json");

fn fixture() -> (Instrumented, Vec<BaselineLinkRequest>) {
    let metadata: serde_json::Value = serde_json::from_slice(MAPPING).unwrap();
    let mut input = super::super::instrument(metadata["source"].as_str().unwrap()).unwrap();
    let nonce = metadata["nonce"].as_str().unwrap();
    input.content = input.content.replace(&input.nonce, nonce);
    input.nonce = nonce.into();
    assert_eq!(input.revision, metadata["revision"].as_str().unwrap());
    for (node, captured) in input.nodes.iter().zip(metadata["nodes"].as_array().unwrap()) {
        assert_eq!(node.id, captured["id"].as_str().unwrap());
        assert_eq!(node.digest, captured["digest"].as_str().unwrap());
    }
    assert_eq!(input.nodes.len(), metadata["nodes"].as_array().unwrap().len());
    assert!(metadata["protected"].as_array().unwrap().is_empty());
    let requests = metadata["requests"].as_array().unwrap().iter().map(|request| BaselineLinkRequest {
        block_id: request["block_id"].as_str().unwrap().into(),
        rise_pt: request["rise_pt"].as_f64().unwrap(),
        expected_links: request["expected_links"].as_u64().unwrap() as usize,
    }).collect();
    (input, requests)
}

fn source_proof(doc: &Document, input: &Instrumented, requests: &[BaselineLinkRequest])
    -> Result<(Vec<Page>, Vec<Annotation>, Graph, Proof), String>
{
    let mut budget = Budget::new();
    let (pages, annotations) = graph::pages_and_annotations(doc, input, &mut budget)?;
    let graph = Graph::read(doc, &pages, &mut budget)?;
    let requests = request_map(input, requests)?;
    let proof = prove(input, &requests, &annotations, &graph, &BTreeMap::new(), true, &mut budget)?;
    Ok((pages, annotations, graph, proof))
}

fn save(doc: &mut Document) -> Vec<u8> {
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

#[test]
fn actual_multipage_private_fragments_have_one_witnessed_outer_source_ancestor() {
    assert_eq!(format!("{:x}", Sha256::digest(RAW)), "80ecd0f9415ee6e551839017736507820a6df472bb3cd688a53e3bc879f6483a");
    assert_eq!(format!("{:x}", Sha256::digest(MAPPING)), "e043efed2f60cbf0af3d50012d0945ebce8cfc6b6307b7cfa6e5a93f87a4c9b6");
    let (input, requests) = fixture();
    let doc = Document::load_mem(RAW).unwrap();
    let (pages, annotations, graph, proof) = source_proof(&doc, &input, &requests).expect("actual native source/link proof");
    assert_eq!(pages.len(), 3);
    assert_eq!(proof.real.len(), 53);
    assert_eq!(proof.temporary_annotations.len(), 6);
    assert_eq!(proof.wrappers.len(), 4);
    let mut descendant_witnesses = 0;
    for annotation in annotations.iter().filter(|a| a.temporary.is_some()) {
        let source = annotation.temporary.as_ref().unwrap();
        let tag = graph.objrs.get(&annotation.id).unwrap().tag;
        let canonical = *proof.wrappers.iter().find(|(_, id)| *id == source).unwrap().0;
        assert!(tag_ancestors(&graph, tag, &mut Budget::new()).unwrap().contains(&canonical));
        assert_eq!(owner(&graph, &proof.wrappers, tag).unwrap().as_ref(), Some(source));
        descendant_witnesses += usize::from(tag != canonical);
    }
    assert_eq!(descendant_witnesses, 2);
    assert_eq!(requests.iter().map(|request| proof.real.iter().filter(|real| real.source == request.block_id).count()).collect::<Vec<_>>(), vec![26, 26, 1]);
}

#[test]
fn actual_multipage_all_53_links_are_paint_proven_cleaned_and_only_rect_shifted() {
    let (input, requests) = fixture();
    let original = Document::load_mem(RAW).unwrap();
    let (_, _, _, before) = source_proof(&original, &input, &requests).unwrap();
    let (bytes, map) = extract_and_clean_with_baseline_and_paint(RAW, &input, &requests, &[])
        .expect("complete actual native multipage paint/cleanup proof");
    assert_eq!(map.pages.len(), 3);
    let (_, original_annotations, _, _) = source_proof(&original, &input, &requests).unwrap();
    let heading = input.nodes.iter().find(|node| node.kind == "heading").unwrap();
    let heading_witness = original_annotations.iter()
        .find(|annotation| annotation.temporary.as_ref() == Some(&heading.id)).unwrap();
    // Independently measured from this exact native fixture: the unshifted
    // heading's ReadMDSansSC-Bold GID1 has UPEM=1000, outline xMin=-4,
    // hmtx=(641,-4) and size=20pt. A logical wrapper omits its 0.08pt overhang.
    let heading_ink = [39.605039520263674, 40.86505859375, 52.58503952026367, 55.68505859375];
    assert_eq!(heading_witness.page, 1);
    let tolerance = 0.002;
    assert!(heading_witness.rect[0] > heading_ink[0] + tolerance);
    assert!(map.hits.iter().any(|hit| hit.id == heading.id && hit.page == 1
        && hit.rect[0] <= heading_ink[0] + tolerance
        && hit.rect[1] <= heading_ink[1] + tolerance
        && hit.rect[0] + hit.rect[2] >= heading_ink[2] - tolerance
        && hit.rect[1] + hit.rect[3] >= heading_ink[3] - tolerance),
        "Actual unshifted negative-LSB heading ink needs a proven source hit");
    for hit in &map.hits {
        assert!(input.nodes.iter().any(|node| node.id == hit.id));
        assert!(original_annotations.iter().any(|annotation| annotation.page == hit.page
            && annotation.temporary.as_ref() == Some(&hit.id)),
            "Ink envelopes retain the actual source's native page, never another page");
    }
    assert!(!bytes.windows(input.nonce.len()).any(|part| part == input.nonce.as_bytes()));
    let cleaned = Document::load_mem(&bytes).unwrap();
    let mut budget = Budget::new();
    let (pages, annotations) = graph::pages_and_annotations(&cleaned, &input, &mut budget).unwrap();
    let graph = Graph::read(&cleaned, &pages, &mut budget).unwrap();
    assert_eq!(annotations.len(), 53);
    assert!(annotations.iter().all(|annotation| annotation.temporary.is_none()));
    for annotation in annotations {
        let old = before.real.iter().find(|old| old.key == annotation.key).unwrap();
        let mut expected = old.original.clone();
        expected.set("Rect", moved_rect(old.rect, -2.5, pages[old.page as usize - 1].media).unwrap()
            .into_iter().map(|coordinate| Object::Real(coordinate as f32)).collect::<Vec<_>>());
        assert_eq!(annotation.original, expected);
        assert_eq!(graph.tags[&graph.objrs[&annotation.id].tag].role.as_slice(), b"Link");
    }
    for page in original.get_pages().values() {
        assert_eq!(original.get_page_content(*page).unwrap(), cleaned.get_page_content(*page).unwrap());
    }
}

fn replace_uri(doc: &mut Document, id: ObjectId, uri: &str) {
    doc.get_dictionary_mut(id).unwrap().set("A", dictionary! {
        "S" => Object::Name(b"URI".to_vec()), "URI" => Object::string_literal(uri)
    });
}

#[test]
fn private_source_fragments_on_incomparable_branches_cannot_claim_a_common_owner() {
    let (input, requests) = fixture();
    let mut doc = Document::load_mem(RAW).unwrap();
    let (_, annotations, graph, proof) = source_proof(&doc, &input, &requests).unwrap();
    let witness = annotations.iter().find(|annotation| annotation.temporary.as_ref() == Some(&requests[0].block_id)
        && !proof.wrappers.contains_key(&graph.objrs[&annotation.id].tag)).unwrap();
    replace_uri(&mut doc, witness.id, &format!("{}{}", super::super::prefix(&input.nonce), requests[1].block_id));
    assert_eq!(source_proof(&doc, &input, &requests).err().as_deref(), Some(INVALID));
}

#[test]
fn an_unwitnessed_outer_tag_cannot_be_invented_or_a_real_link_relabelled_as_wrapper() {
    let (input, requests) = fixture();
    let mut doc = Document::load_mem(RAW).unwrap();
    let (_, annotations, graph, proof) = source_proof(&doc, &input, &requests).unwrap();
    let witness = annotations.iter().find(|annotation| annotation.temporary.as_ref() == Some(&requests[0].block_id)
        && proof.wrappers.contains_key(&graph.objrs[&annotation.id].tag)).unwrap();
    replace_uri(&mut doc, witness.id, "https://example.invalid/extra-real");
    assert_eq!(source_proof(&doc, &input, &requests).err().as_deref(), Some(INVALID));
}

#[test]
fn a_missing_real_glyph_fragment_is_not_replaced_by_a_private_source_envelope() {
    let (input, requests) = fixture();
    let mut doc = Document::load_mem(RAW).unwrap();
    let (_, _, _, proof) = source_proof(&doc, &input, &requests).unwrap();
    let first = &proof.real[0];
    let mut rect = first.rect;
    rect[2] = rect[0] + 0.1;
    let annotation = doc.objects.iter().find_map(|(id, value)| value.as_dict().ok()
        .filter(|dictionary| dictionary.get(b"StructParent").and_then(Object::as_i64).ok() == Some(first.key))
        .map(|_| *id)).unwrap();
    doc.get_dictionary_mut(annotation).unwrap().set("Rect", rect.into_iter().map(|v|Object::Real(v as f32)).collect::<Vec<_>>());
    assert!(extract_and_clean_with_baseline_and_paint(&save(&mut doc), &input, &requests, &[]).is_err());
}

#[test]
fn overlapping_real_fragments_remain_ambiguous_even_when_private_objrs_are_excluded() {
    let (input, requests) = fixture();
    let mut doc = Document::load_mem(RAW).unwrap();
    let (_, annotations, _, proof) = source_proof(&doc, &input, &requests).unwrap();
    let first = &proof.real[0];
    let second = proof.real.iter().skip(1).find(|real|real.source == first.source && real.page_id == first.page_id).unwrap();
    let annotation = annotations.iter().find(|annotation|annotation.key == second.key).unwrap();
    doc.get_dictionary_mut(annotation.id).unwrap().set("Rect", first.rect.into_iter().map(|v|Object::Real(v as f32)).collect::<Vec<_>>());
    assert!(extract_and_clean_with_baseline_and_paint(&save(&mut doc), &input, &requests, &[]).is_err());
}

#[test]
fn a_real_fragment_with_only_valid_empty_outline_glyphs_is_not_accepted() {
    let (input, requests) = fixture();
    let mut doc = Document::load_mem(RAW).unwrap();
    let page = *doc.get_pages().values().next().unwrap();
    let mut content = lopdf::content::Content::decode(&doc.get_page_content(page).unwrap()).unwrap();
    let mut marks = Vec::new();
    let mut changed = 0usize;
    fn blank(value: &mut Object, changed: &mut usize) {
        match value {
            Object::String(bytes, _) => {
                assert_eq!(bytes.len() % 2, 0);
                for code in bytes.chunks_exact_mut(2) {
                    // The real embedded Identity-H face's GID 9 has no
                    // outline and a verified, nonzero 224/1000 advance.
                    code.copy_from_slice(&[0, 9]);
                    *changed += 1;
                }
            }
            Object::Array(items) => for item in items { blank(item, changed); },
            Object::Integer(_) | Object::Real(_) => (),
            _ => panic!("Unexpected native text-show operand"),
        }
    }
    for operation in &mut content.operations {
        match operation.operator.as_str() {
            "BDC" => marks.push(operation.operands.last().unwrap().as_dict().ok()
                .and_then(|properties| properties.get(b"MCID").ok())
                .and_then(|value| value.as_i64().ok())),
            "BMC" => marks.push(None),
            "EMC" => { marks.pop().unwrap(); },
            "Tj" | "TJ" if marks.last() == Some(&Some(2)) => {
                assert_eq!(operation.operands.len(), 1);
                blank(&mut operation.operands[0], &mut changed);
            }
            _ => (),
        }
    }
    assert!(marks.is_empty());
    assert!(changed > 0, "The original first link fragment must actually change");
    let stream = doc.add_object(lopdf::Stream::new(dictionary! {}, content.encode().unwrap()));
    doc.get_dictionary_mut(page).unwrap().set("Contents", stream);
    let (_, _, _, proof) = source_proof(&doc, &input, &requests).unwrap();
    assert_eq!(proof.real.len(), 53, "Every original OBJR and annotation remains");
    assert_eq!(extract_and_clean_with_baseline_and_paint(&save(&mut doc), &input, &requests, &[])
        .err().as_deref(), Some(INVALID));
}
