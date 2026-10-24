use super::*;
use lopdf::{dictionary, Stream};

const REAL_URI: &str = "https://example.invalid/repeated";

#[derive(Clone)]
struct Fragment {
    id: ObjectId,
    key: i64,
    source: usize,
    page: usize,
}
struct Fixture {
    doc: Document,
    input: Instrumented,
    pages: Vec<ObjectId>,
    pages_root: ObjectId,
    catalog: ObjectId,
    tree: ObjectId,
    parents: ObjectId,
    paragraphs: Vec<ObjectId>,
    wrappers: Vec<ObjectId>,
    temporary: Vec<ObjectId>,
    actions: Vec<ObjectId>,
    links: Vec<ObjectId>,
    fragments: Vec<Fragment>,
    mcid_arrays: Vec<ObjectId>,
    content: ObjectId,
}

fn dmut(doc: &mut Document, id: ObjectId) -> &mut Dictionary {
    doc.objects.get_mut(&id).unwrap().as_dict_mut().unwrap()
}
fn mcr(page: ObjectId, mcid: usize) -> Object {
    Object::Dictionary(dictionary! {"Type"=>"MCR", "Pg"=>page, "MCID"=>mcid as i64})
}
fn objr(page: ObjectId, annotation: ObjectId) -> Object {
    Object::Dictionary(dictionary! {"Type"=>"OBJR", "Pg"=>page, "Obj"=>annotation})
}
fn content_key(values: &mut [Vec<Object>], page: usize, tag: ObjectId) -> usize {
    let key = values[page].len();
    values[page].push(Object::Reference(tag));
    key
}
fn annotation(
    doc: &mut Document,
    page: ObjectId,
    key: i64,
    rect: [i64; 4],
    uri: &str,
    indirect_action: bool,
) -> (ObjectId, Option<ObjectId>) {
    let action = dictionary! {"Type"=>"Action", "S"=>"URI", "URI"=>Object::string_literal(uri)};
    let (action, aid) = if indirect_action {
        let id = doc.add_object(action);
        (Object::Reference(id), Some(id))
    } else {
        (Object::Dictionary(action), None)
    };
    let id = doc.add_object(dictionary! {"Type"=>"Annot", "Subtype"=>"Link", "P"=>page,
    "Rect"=>rect.into_iter().map(Object::Integer).collect::<Vec<_>>(),
    "Border"=>vec![0.into(),0.into(),0.into()], "StructParent"=>key,
    "A"=>action, "Contents"=>Object::string_literal(uri), "F"=>4});
    (id, aid)
}
impl Fixture {
    // Two duplicate source nodes, the same URI, one logical link split into
    // three fragments over two pages, plus a paragraph with no real links.
    fn new() -> Self {
        let input = super::super::instrument("Same.\n\nSame.\n\nNo link.\n").unwrap();
        assert_eq!(input.nodes.len(), 3);
        assert_ne!(input.nodes[0].id, input.nodes[1].id);
        let mut doc = Document::with_version("1.7");
        let pages_root = doc.new_object_id();
        let pages = vec![doc.new_object_id(), doc.new_object_id()];
        let tree = doc.new_object_id();
        let document = doc.new_object_id();
        let parents = doc.new_object_id();
        let font = doc
            .add_object(dictionary! {"Type"=>"Font", "Subtype"=>"Type1", "BaseFont"=>"Helvetica"});
        let content = doc.add_object(Stream::new(
            Dictionary::new(),
            b"BT /F0 11 Tf (Original content) Tj ET".to_vec(),
        ));
        let mut page_annots = vec![Vec::new(), Vec::new()];
        let mut mcids = vec![Vec::new(), Vec::new()];
        let mut pt = BTreeMap::new();
        let mut next_key = 0i64;
        let mut paragraphs = Vec::new();
        let mut wrappers = Vec::new();
        let mut temporary = Vec::new();
        let mut actions = Vec::new();
        let mut links = Vec::new();
        let mut fragments = Vec::new();
        for source in 0..3 {
            let paragraph = doc.new_object_id();
            let wrapper = doc.new_object_id();
            paragraphs.push(paragraph);
            wrappers.push(wrapper);
            let uri = format!(
                "{}{}",
                super::super::prefix(&input.nonce),
                input.nodes[source].id
            );
            let (temp, action) = annotation(
                &mut doc,
                pages[0],
                next_key,
                [15, 220 - source as i64 * 40, 175, 235 - source as i64 * 40],
                &uri,
                true,
            );
            pt.insert(next_key, Object::Reference(wrapper));
            next_key += 1;
            page_annots[0].push(Object::Reference(temp));
            temporary.push(temp);
            actions.push(action.unwrap());
            let key = content_key(&mut mcids, 0, wrapper);
            let mut wrapper_k = vec![Object::Integer(key as i64), objr(pages[0], temp)];
            if source < 2 {
                let link = doc.new_object_id();
                links.push(link);
                let fragment_pages = if source == 0 { vec![0] } else { vec![0, 0, 1] };
                let mut link_k = Vec::new();
                for (index, page) in fragment_pages.into_iter().enumerate() {
                    let (id, _) = annotation(
                        &mut doc,
                        pages[page],
                        next_key,
                        [
                            40 + index as i64 * 30,
                            100 + source as i64 * 20,
                            65 + index as i64 * 30,
                            112 + source as i64 * 20,
                        ],
                        REAL_URI,
                        false,
                    );
                    fragments.push(Fragment {
                        id,
                        key: next_key,
                        source,
                        page,
                    });
                    pt.insert(next_key, Object::Reference(link));
                    next_key += 1;
                    page_annots[page].push(Object::Reference(id));
                    let key = content_key(&mut mcids, page, link);
                    link_k.push(if page == 0 {
                        Object::Integer(key as i64)
                    } else {
                        mcr(pages[page], key)
                    });
                    // Exercise direct and indirect OBJRs in the same proof.
                    let value = objr(pages[page], id);
                    link_k.push(if index == 1 {
                        Object::Reference(doc.add_object(value))
                    } else {
                        value
                    });
                }
                doc.objects.insert(link, Object::Dictionary(dictionary! {"Type"=>"StructElem", "S"=>"Link", "P"=>wrapper, "Pg"=>pages[0], "K"=>link_k}));
                wrapper_k.push(Object::Reference(link));
            }
            // An unrelated Code sibling is permitted and is never moved.
            let code = doc.new_object_id();
            let key = content_key(&mut mcids, 0, code);
            doc.objects.insert(code, Object::Dictionary(dictionary! {"Type"=>"StructElem", "S"=>"Code", "P"=>wrapper, "Pg"=>pages[0], "K"=>key as i64}));
            wrapper_k.push(Object::Reference(code));
            doc.objects.insert(wrapper, Object::Dictionary(dictionary! {"Type"=>"StructElem", "S"=>"Link", "P"=>paragraph, "Pg"=>pages[0], "K"=>wrapper_k}));
            doc.objects.insert(paragraph, Object::Dictionary(dictionary! {"Type"=>"StructElem", "S"=>"P", "P"=>document, "K"=>vec![Object::Reference(wrapper)]}));
        }
        let mut mcid_arrays = Vec::new();
        for (index, page) in pages.iter().copied().enumerate() {
            let array = doc.add_object(Object::Array(mcids[index].clone()));
            mcid_arrays.push(array);
            pt.insert(10000 + index as i64, Object::Reference(array));
            doc.objects.insert(page, Object::Dictionary(dictionary! {"Type"=>"Page", "Parent"=>pages_root,
                "StructParents"=>10000 + index as i64, "Annots"=>page_annots[index].clone(), "Contents"=>content,
                "Resources"=>dictionary! {"Font"=>dictionary! {"F0"=>font}}}));
        }
        let nums = pt
            .into_iter()
            .flat_map(|(key, value)| [Object::Integer(key), value])
            .collect::<Vec<_>>();
        doc.objects
            .insert(parents, Object::Dictionary(dictionary! {"Nums"=>nums}));
        doc.objects.insert(document, Object::Dictionary(dictionary! {"Type"=>"StructElem", "S"=>"Document", "P"=>tree, "K"=>paragraphs.iter().copied().map(Object::Reference).collect::<Vec<_>>()}));
        doc.objects.insert(tree, Object::Dictionary(dictionary! {"Type"=>"StructTreeRoot", "ParentTree"=>parents, "K"=>vec![Object::Reference(document)]}));
        doc.objects.insert(pages_root, Object::Dictionary(dictionary! {"Type"=>"Pages", "Kids"=>pages.iter().copied().map(Object::Reference).collect::<Vec<_>>(), "Count"=>2, "MediaBox"=>vec![10.into(),20.into(),210.into(),320.into()]}));
        let catalog = doc.add_object(
            dictionary! {"Type"=>"Catalog", "Pages"=>pages_root, "StructTreeRoot"=>tree},
        );
        doc.trailer.set("Root", catalog);
        Self {
            doc,
            input,
            pages,
            pages_root,
            catalog,
            tree,
            parents,
            paragraphs,
            wrappers,
            temporary,
            actions,
            links,
            fragments,
            mcid_arrays,
            content,
        }
    }
    fn bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.doc.clone().save_to(&mut out).unwrap();
        out
    }
    fn requests(&self, rise: f64) -> Vec<BaselineLinkRequest> {
        vec![
            BaselineLinkRequest {
                block_id: self.input.nodes[1].id.clone(),
                rise_pt: rise,
                expected_links: 1,
            },
            BaselineLinkRequest {
                block_id: self.input.nodes[2].id.clone(),
                rise_pt: rise,
                expected_links: 0,
            },
        ]
    }
    fn reject(&self, code: &str) {
        assert_eq!(
            extract_and_clean_with_baseline(&self.bytes(), &self.input, &self.requests(3.))
                .unwrap_err(),
            code
        );
    }
}

fn parsed(bytes: &[u8], input: &Instrumented) -> (Document, Vec<Page>, Vec<Annotation>) {
    let doc = Document::load_mem(bytes).unwrap();
    let (pages, annotations) =
        graph::pages_and_annotations(&doc, input, &mut Budget::new()).unwrap();
    (doc, pages, annotations)
}
fn assert_rect_only(before: &[u8], after: &[u8], changes: &BTreeMap<ObjectId, [f64; 4]>) {
    assert!(after.starts_with(before));
    let a = Document::load_mem(before).unwrap();
    let b = Document::load_mem(after).unwrap();
    for (id, value) in &a.objects {
        if value.type_name().ok() == Some(b"XRef") {
            continue;
        }
        let mut expected = value.clone();
        if let Some(rect) = changes.get(id) {
            let dictionary = expected.as_dict_mut().unwrap();
            let mut values = graph::direct(&a, dictionary.get(b"Rect").unwrap())
                .unwrap()
                .as_array()
                .unwrap()
                .clone();
            // Reparsed PDF integer lexemes have Integer representation. This
            // still compares the entire ordinary object/key set exactly.
            for index in [1, 3] {
                values[index] = if rect[index].fract() == 0. {
                    Object::Integer(rect[index] as i64)
                } else {
                    Object::Real(rect[index] as f32)
                };
            }
            dictionary.set("Rect", values);
        }
        assert_eq!(b.objects.get(id), Some(&expected), "ordinary object {id:?}");
    }
    assert!(b
        .objects
        .iter()
        .all(|(id, value)| a.objects.contains_key(id) || value.type_name().ok() == Some(b"XRef")));
}

#[test]
fn duplicate_sources_same_uri_multifragment_multipage_and_zero_link_paragraph() {
    let fixture = Fixture::new();
    let raw = fixture.bytes();
    let (base, _) =
        extract_and_clean_with_baseline(&raw, &fixture.input, &fixture.requests(0.)).unwrap();
    let (output, map) =
        extract_and_clean_with_baseline(&raw, &fixture.input, &fixture.requests(3.)).unwrap();
    let (_, pages, annotations) = parsed(&output, &fixture.input);
    assert_eq!(annotations.len(), 4);
    assert_eq!(map.nodes.len(), 3);
    assert_eq!(map.pages, vec![[200., 300.]; 2]);
    let mut changes = BTreeMap::new();
    for fragment in &fixture.fragments {
        let a = annotations.iter().find(|a| a.key == fragment.key).unwrap();
        let original = fixture.doc.get_dictionary(fragment.id).unwrap();
        assert_eq!(a.original.get(b"A").unwrap(), original.get(b"A").unwrap());
        let old = original
            .get(b"Rect")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(super::super::number)
            .collect::<Option<Vec<_>>>()
            .unwrap();
        let rise = if fragment.source == 1 { 3. } else { 0. };
        assert_eq!(a.rect, [old[0], old[1] + rise, old[2], old[3] + rise]);
        assert_eq!(a.page as usize, fragment.page + 1);
        if rise != 0. {
            changes.insert(a.id, a.rect);
        }
        let expected_hit = hit(
            a.rect,
            pages[fragment.page].media,
            &fixture.input.nodes[fragment.source].id,
            a.page,
        )
        .unwrap();
        assert!(map.hits.iter().any(|h| h.id == expected_hit.id
            && h.page == expected_hit.page
            && h.rect == expected_hit.rect));
    }
    assert_eq!(changes.len(), 3);
    assert_rect_only(&base, &output, &changes);
    let (doc, _, _) = parsed(&output, &fixture.input);
    assert!(fixture
        .temporary
        .iter()
        .chain(&fixture.actions)
        .all(|id| !doc.objects.contains_key(id)));
    assert!(fixture.wrappers.iter().all(|id| doc
        .get_dictionary(*id)
        .unwrap()
        .get(b"S")
        .unwrap()
        .as_name()
        .unwrap()
        == b"Span"));
    assert!(!output
        .windows(fixture.input.nonce.len())
        .any(|w| w == fixture.input.nonce.as_bytes()));
    assert!(!format!("{:?}", doc.objects).contains("readmd-internal:publication/"));
}

#[test]
fn fractional_negative_rise_preserves_x_width_height_and_requires_witness_for_zero_links() {
    let fixture = Fixture::new();
    let raw = fixture.bytes();
    let (output, _) =
        extract_and_clean_with_baseline(&raw, &fixture.input, &fixture.requests(-2.5)).unwrap();
    let (_, _, annotations) = parsed(&output, &fixture.input);
    for a in annotations.iter().filter(|a| {
        fixture
            .fragments
            .iter()
            .any(|f| f.source == 1 && f.key == a.key)
    }) {
        assert_eq!(a.rect[1], 117.5);
        assert_eq!(a.rect[3], 129.5);
        assert_eq!(a.rect[2] - a.rect[0], 25.);
        assert_eq!(a.rect[3] - a.rect[1], 12.);
    }
    let mut missing = Fixture::new();
    let id = missing.temporary[2];
    dmut(&mut missing.doc, id).set(
        "A",
        dictionary! {"S"=>"URI", "URI"=>Object::string_literal(REAL_URI)},
    );
    // The third source still has no logical links, but has lost its witness.
    missing.reject(UNSUPPORTED);
}

#[test]
fn indirect_annotation_k_arrays_and_nested_number_tree_are_cleaned_without_dangling_references() {
    let mut f = Fixture::new();
    let mut deleted = Vec::new();
    for page in f.pages.clone() {
        let values = f
            .doc
            .get_dictionary(page)
            .unwrap()
            .get(b"Annots")
            .unwrap()
            .clone();
        let id = f.doc.add_object(values);
        dmut(&mut f.doc, page).set("Annots", id);
    }
    for wrapper in f.wrappers.clone() {
        let values = f
            .doc
            .get_dictionary(wrapper)
            .unwrap()
            .get(b"K")
            .unwrap()
            .clone();
        let id = f.doc.add_object(values);
        deleted.push(id);
        dmut(&mut f.doc, wrapper).set("K", id);
    }
    let nums = f
        .doc
        .get_dictionary(f.parents)
        .unwrap()
        .get(b"Nums")
        .unwrap()
        .as_array()
        .unwrap()
        .clone();
    let mut leaves = Vec::new();
    for chunk in nums.chunks(8) {
        let numbers = f.doc.add_object(Object::Array(chunk.to_vec()));
        deleted.push(numbers);
        let limits = f.doc.add_object(Object::Array(vec![
            chunk[0].clone(),
            chunk[chunk.len() - 2].clone(),
        ]));
        deleted.push(limits);
        let leaf = f
            .doc
            .add_object(dictionary! {"Nums"=>numbers,"Limits"=>limits});
        deleted.push(leaf);
        leaves.push(Object::Reference(leaf));
    }
    let kids = f.doc.add_object(Object::Array(leaves));
    deleted.push(kids);
    let parent = dmut(&mut f.doc, f.parents);
    parent.remove(b"Nums");
    parent.set("Kids", kids);
    parent.set(
        "Limits",
        vec![nums[0].clone(), nums[nums.len() - 2].clone()],
    );
    let (output, _) =
        extract_and_clean_with_baseline(&f.bytes(), &f.input, &f.requests(3.)).unwrap();
    let (doc, pages, annotations) = parsed(&output, &f.input);
    let graph = Graph::read(&doc, &pages, &mut Budget::new()).unwrap();
    graph.check_annotations(&annotations).unwrap();
    assert!(deleted.iter().all(|id| !doc.objects.contains_key(id)));
    assert!(!doc.get_dictionary(f.parents).unwrap().has(b"Kids"));
    assert_eq!(graph.tree.entries.len(), annotations.len() + pages.len());
}

#[test]
fn real_native_typst_fixture_accepts_code_siblings_and_changes_only_eight_rects() {
    // Original 0.15.1 component; 8 logical links, 32 source fragments. The
    // physical +3pt glyph/18pt baseline oracle is external to this unit test.
    // This fixture is always compiled into the test; no .cache/skip fallback.
    use sha2::{Digest, Sha256};
    let raw = include_bytes!("../tests/fixtures/typst-baseline-native-plus3.pdf");
    assert_eq!(
        format!("{:x}", Sha256::digest(raw)),
        "98e5185073ea8eab32d79e7f58bbba86868e0c4607efd15896d0a1e03bc7b53e"
    );
    let mut input = super::super::instrument("Native paragraph.\n").unwrap();
    input.nonce = "9a0a36b7c2897f6f82fa3795ef5bec7c".into();
    input.nodes[0].id = "probe".into();
    let request = |rise| {
        vec![BaselineLinkRequest {
            block_id: "probe".into(),
            rise_pt: rise,
            expected_links: 8,
        }]
    };
    let (base, _) = extract_and_clean_with_baseline(raw, &input, &request(0.)).unwrap();
    let (output, map) = extract_and_clean_with_baseline(raw, &input, &request(3.)).unwrap();
    let (_, _, before) = parsed(&base, &input);
    let (_, pages, after) = parsed(&output, &input);
    assert_eq!(before.len(), 8);
    assert_eq!(after.len(), 8);
    let mut changes = BTreeMap::new();
    for a in after {
        let old = before.iter().find(|old| old.key == a.key).unwrap();
        assert!((a.rect[1] - old.rect[1] - 3.).abs() < 0.0001);
        assert!((a.rect[3] - old.rect[3] - 3.).abs() < 0.0001);
        assert_eq!(a.rect[0], old.rect[0]);
        assert_eq!(a.rect[2], old.rect[2]);
        changes.insert(a.id, a.rect);
        let expected = hit(a.rect, pages[a.page as usize - 1].media, "probe", a.page).unwrap();
        assert!(map
            .hits
            .iter()
            .any(|h| h.id == "probe" && h.page == a.page && h.rect == expected.rect));
    }
    assert_eq!(changes.len(), 8);
    assert_rect_only(&base, &output, &changes);
}

#[test]
fn expected_groups_unknown_source_duplicate_source_and_missing_witness_refuse() {
    let f = Fixture::new();
    let mut requests = f.requests(3.);
    requests[0].expected_links = 3;
    assert_eq!(
        extract_and_clean_with_baseline(&f.bytes(), &f.input, &requests).unwrap_err(),
        INVALID
    );
    let mut f = Fixture::new();
    let uri = format!("{}unknown", super::super::prefix(&f.input.nonce));
    dmut(&mut f.doc, f.actions[0]).set("URI", Object::string_literal(uri));
    f.reject(INVALID);
    let mut f = Fixture::new();
    let uri = format!(
        "{}{}",
        super::super::prefix(&f.input.nonce),
        f.input.nodes[0].id
    );
    dmut(&mut f.doc, f.actions[1]).set("URI", Object::string_literal(uri));
    f.reject(INVALID);
}

#[test]
fn duplicate_or_missing_objr_and_unrepresented_content_refuse() {
    let mut f = Fixture::new();
    let k = dmut(&mut f.doc, f.links[1])
        .get_mut(b"K")
        .unwrap()
        .as_array_mut()
        .unwrap();
    k.push(k[1].clone());
    f.reject(INVALID);
    let mut f = Fixture::new();
    dmut(&mut f.doc, f.links[1])
        .get_mut(b"K")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .remove(1);
    f.reject(INVALID);
    let mut f = Fixture::new();
    dmut(&mut f.doc, f.links[1])
        .get_mut(b"K")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .remove(0);
    f.reject(INVALID);
    let mut f = Fixture::new();
    f.doc
        .objects
        .get_mut(&f.mcid_arrays[0])
        .unwrap()
        .as_array_mut()
        .unwrap()
        .push(Object::Reference(f.links[1]));
    f.reject(INVALID);
    let mut f = Fixture::new();
    let k = dmut(&mut f.doc, f.links[1])
        .get_mut(b"K")
        .unwrap()
        .as_array_mut()
        .unwrap();
    k.swap(0, 1);
    f.reject(INVALID);
}

#[test]
fn cycles_parent_disagreement_and_duplicate_annotation_on_two_pages_refuse() {
    let mut f = Fixture::new();
    dmut(&mut f.doc, f.links[1])
        .get_mut(b"K")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .push(Object::Reference(f.wrappers[1]));
    f.reject(INVALID);
    let mut f = Fixture::new();
    dmut(&mut f.doc, f.links[1]).set("P", f.wrappers[0]);
    f.reject(INVALID);
    let mut f = Fixture::new();
    dmut(&mut f.doc, f.pages[1])
        .get_mut(b"Annots")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .push(Object::Reference(f.fragments[0].id));
    f.reject(INVALID);
    let mut f = Fixture::new();
    dmut(&mut f.doc, f.pages_root)
        .get_mut(b"Kids")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .push(Object::Reference(f.pages_root));
    f.reject(INVALID);
    let mut f = Fixture::new();
    dmut(&mut f.doc, f.paragraphs[1]).set("P", f.paragraphs[1]);
    f.reject(INVALID);
}

#[test]
fn bad_parent_tree_owner_duplicates_limits_unknown_key_and_page_mapping_refuse() {
    let mut f = Fixture::new();
    dmut(&mut f.doc, f.parents)
        .get_mut(b"Nums")
        .unwrap()
        .as_array_mut()
        .unwrap()[1] = Object::Reference(f.links[0]);
    f.reject(INVALID);
    let mut f = Fixture::new();
    let nums = dmut(&mut f.doc, f.parents)
        .get_mut(b"Nums")
        .unwrap()
        .as_array_mut()
        .unwrap();
    nums[2] = nums[0].clone();
    f.reject(INVALID);
    let mut f = Fixture::new();
    dmut(&mut f.doc, f.parents).set("Limits", vec![0.into(), 1.into()]);
    f.reject(INVALID);
    let mut f = Fixture::new();
    dmut(&mut f.doc, f.parents)
        .get_mut(b"Nums")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .extend([20000.into(), Object::Reference(f.links[0])]);
    f.reject(INVALID);
    let mut f = Fixture::new();
    f.doc
        .objects
        .get_mut(&f.mcid_arrays[0])
        .unwrap()
        .as_array_mut()
        .unwrap()[0] = Object::Reference(f.links[1]);
    f.reject(INVALID);
    let mut f = Fixture::new();
    let pt = f.parents;
    let d = dmut(&mut f.doc, pt);
    d.remove(b"Nums");
    d.set("Kids", vec![Object::Reference(pt)]);
    f.reject(INVALID);
}

#[test]
fn wrong_objr_page_shared_action_and_surviving_private_bytes_refuse() {
    let mut f = Fixture::new();
    let k = dmut(&mut f.doc, f.links[1])
        .get_mut(b"K")
        .unwrap()
        .as_array_mut()
        .unwrap();
    k[1].as_dict_mut().unwrap().set("Pg", f.pages[1]);
    f.reject(INVALID);
    let mut f = Fixture::new();
    dmut(&mut f.doc, f.catalog).set("RetainedAction", f.actions[1]);
    f.reject(INVALID);
    let mut f = Fixture::new();
    let mut stream = Stream::new(
        Dictionary::new(),
        format!("hidden {}", f.input.nonce).into_bytes(),
    );
    stream.compress().unwrap();
    f.doc.objects.insert(f.content, Object::Stream(stream));
    f.reject(INVALID);
    let mut f = Fixture::new();
    let utf16 = f
        .input
        .nonce
        .bytes()
        .flat_map(|b| [0, b])
        .collect::<Vec<_>>();
    dmut(&mut f.doc, f.catalog).set("Hidden", Object::string_literal(utf16));
    f.reject(INVALID);
}

#[test]
fn unsupported_hit_shapes_rotations_linked_code_and_out_of_paper_are_typed_errors() {
    let mut f = Fixture::new();
    dmut(&mut f.doc, f.fragments[1].id).set("QuadPoints", vec![0.into(); 8]);
    f.reject(UNSUPPORTED);
    let mut f = Fixture::new();
    dmut(&mut f.doc, f.pages[0]).set("Rotate", 90);
    f.reject(UNSUPPORTED);
    let mut f = Fixture::new();
    let link = f.links[1];
    let code = f.doc.new_object_id();
    let mcid = f
        .doc
        .get_dictionary(link)
        .unwrap()
        .get(b"K")
        .unwrap()
        .as_array()
        .unwrap()[0]
        .as_i64()
        .unwrap();
    f.doc.objects.insert(
        code,
        Object::Dictionary(
            dictionary! {"Type"=>"StructElem","S"=>"Code","P"=>link,"Pg"=>f.pages[0],"K"=>mcid},
        ),
    );
    dmut(&mut f.doc, link)
        .get_mut(b"K")
        .unwrap()
        .as_array_mut()
        .unwrap()[0] = Object::Reference(code);
    f.doc
        .objects
        .get_mut(&f.mcid_arrays[0])
        .unwrap()
        .as_array_mut()
        .unwrap()[mcid as usize] = Object::Reference(code);
    f.reject(UNSUPPORTED);
    let mut f = Fixture::new();
    dmut(&mut f.doc, f.fragments[1].id)
        .set("Rect", vec![40.into(), 300.into(), 65.into(), 319.into()]);
    f.reject(UNSUPPORTED);
    let mut f = Fixture::new();
    dmut(&mut f.doc, f.tree).set("RoleMap", dictionary! {"Link"=>"Span"});
    f.reject(UNSUPPORTED);
}

#[test]
fn empty_requests_are_exact_legacy_behavior_and_request_validation_precedes_cleanup() {
    let f = Fixture::new();
    let raw = f.bytes();
    let (old, old_map) = super::super::extract_and_clean(&raw, f.input.clone()).unwrap();
    let (new, new_map) = extract_and_clean_with_baseline(&raw, &f.input, &[]).unwrap();
    assert_eq!(old, new);
    assert_eq!(
        serde_json::to_value(old_map).unwrap(),
        serde_json::to_value(new_map).unwrap()
    );
    let mut requests = f.requests(3.);
    requests.push(requests[0].clone());
    assert_eq!(
        extract_and_clean_with_baseline(&raw, &f.input, &requests).unwrap_err(),
        INVALID
    );
    for rise in [f64::NAN, 73., -73.] {
        assert_eq!(
            extract_and_clean_with_baseline(&raw, &f.input, &f.requests(rise)).unwrap_err(),
            INVALID
        );
    }
}

#[test]
fn legacy_cleanup_removes_exclusive_indirect_actions_and_uri_objects() {
    let mut f = Fixture::new();
    let mut private_uri_ids = Vec::new();
    for action in f.actions.clone() {
        let uri = f
            .doc
            .get_dictionary(action)
            .unwrap()
            .get(b"URI")
            .unwrap()
            .clone();
        let id = f.doc.add_object(uri);
        private_uri_ids.push(id);
        dmut(&mut f.doc, action).set("URI", id);
    }
    let (bytes, map) = super::super::extract_and_clean(&f.bytes(), f.input.clone()).unwrap();
    let doc = Document::load_mem(&bytes).unwrap();
    assert!(f
        .actions
        .iter()
        .chain(&private_uri_ids)
        .all(|id| !doc.objects.contains_key(id)));
    assert!(!format!("{:?}", doc.objects).contains("readmd-internal:publication/"));
    assert!(!bytes
        .windows(f.input.nonce.len())
        .any(|w| w == f.input.nonce.as_bytes()));
    assert_eq!(map.nodes.len(), 3);
    assert_eq!(map.hits.len(), 7);
    assert!(f
        .fragments
        .iter()
        .all(|fragment| doc.objects.get(&fragment.id) == f.doc.objects.get(&fragment.id)));
}

#[test]
fn legacy_cleanup_refuses_surviving_action_or_uri_references() {
    let mut f = Fixture::new();
    dmut(&mut f.doc, f.catalog).set("Shared", f.actions[0]);
    assert_eq!(
        super::super::extract_and_clean(&f.bytes(), f.input.clone()).unwrap_err(),
        INVALID
    );
    let mut f = Fixture::new();
    let uri = f
        .doc
        .get_dictionary(f.actions[0])
        .unwrap()
        .get(b"URI")
        .unwrap()
        .clone();
    let id = f.doc.add_object(uri);
    dmut(&mut f.doc, f.actions[0]).set("URI", id);
    dmut(&mut f.doc, f.catalog).set("Shared", id);
    assert_eq!(
        super::super::extract_and_clean(&f.bytes(), f.input.clone()).unwrap_err(),
        INVALID
    );
}

#[test]
fn legacy_cleanup_of_real_native_fixture_keeps_existing_mapping_and_real_actions() {
    let raw = include_bytes!("../tests/fixtures/typst-baseline-native-plus3.pdf");
    let mut input = super::super::instrument("Native paragraph.\n").unwrap();
    input.nonce = "9a0a36b7c2897f6f82fa3795ef5bec7c".into();
    input.nodes[0].id = "probe".into();
    let mut native = Document::load_mem(raw).unwrap();
    let mut indirect = Vec::new();
    // This variant preserves the real Typst graph and moves only its private
    // actions to standalone objects to exercise the legacy leak regression.
    for (_, page) in native.get_pages() {
        let annotations = native
            .get_dictionary(page)
            .unwrap()
            .get(b"Annots")
            .unwrap()
            .as_array()
            .unwrap()
            .clone();
        for value in annotations {
            let id = value.as_reference().unwrap();
            let a = native.get_dictionary(id).unwrap();
            let action = a.get(b"A").unwrap().as_dict().unwrap();
            let uri = action.get(b"URI").unwrap().as_str().unwrap();
            if uri.starts_with(b"readmd-internal:publication/") {
                let value = Object::Dictionary(action.clone());
                let aid = native.add_object(value);
                indirect.push(aid);
                dmut(&mut native, id).set("A", aid);
            }
        }
    }
    assert_eq!(indirect.len(), 32);
    let mut variant = Vec::new();
    native.save_to(&mut variant).unwrap();
    let (bytes, map) = super::super::extract_and_clean(&variant, input.clone()).unwrap();
    let doc = Document::load_mem(&bytes).unwrap();
    assert!(indirect.iter().all(|id| !doc.objects.contains_key(id)));
    assert_eq!(map.hits.len(), 40);
    assert!(!format!("{:?}", doc.objects).contains("readmd-internal:publication/"));
    let (_, _, annotations) = parsed(&bytes, &input);
    assert_eq!(annotations.len(), 8);
    for a in annotations {
        assert_eq!(doc.objects.get(&a.id), native.objects.get(&a.id));
    }
}
