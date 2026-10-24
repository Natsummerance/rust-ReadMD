use super::*;
use lopdf::{dictionary, Stream};
use sha2::{Digest, Sha256};

const RAW: &[u8] = include_bytes!("../tests/fixtures/typst-baseline-paint-native.pdf");
fn input() -> Instrumented {
    let mut input = super::super::instrument("Native paint proof.\n").unwrap();
    input.nonce = "0123456789abcdef0123456789abcdef".into();
    input.nodes[0].id = "probe".into();
    input
}
fn requests() -> Vec<BaselineLinkRequest> {
    vec![BaselineLinkRequest {
        block_id: "probe".into(),
        rise_pt: 3.,
        expected_links: 2,
    }]
}
fn protected() -> Vec<BaselineProtectedRequest> {
    ["protected-code", "protected-math"]
        .into_iter()
        .map(|id| BaselineProtectedRequest {
            block_id: "probe".into(),
            witness_id: id.into(),
        })
        .collect()
}
fn save(doc: &mut Document) -> Vec<u8> {
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}
fn change_content(change: impl FnOnce(Vec<u8>) -> Vec<u8>) -> Vec<u8> {
    let mut doc = Document::load_mem(RAW).unwrap();
    let page = *doc.get_pages().values().next().unwrap();
    let bytes = change(doc.get_page_content(page).unwrap());
    let stream = doc.add_object(Stream::new(dictionary! {}, bytes));
    doc.get_dictionary_mut(page)
        .unwrap()
        .set("Contents", stream);
    save(&mut doc)
}
fn run(bytes: &[u8]) -> Result<(Vec<u8>, PageMap), String> {
    extract_and_clean_with_baseline_and_paint(bytes, &input(), &requests(), &protected())
}

#[test]
fn real_single_compile_native_strong_em_code_and_math_are_paint_proven_and_cleaned() {
    // Parent's locked 0.15.1 observation; the generated source, sole C68 SVG,
    // fonts and command are independently bound in raw-paint-01/command.json.
    // This original 25,660-byte fixture is embedded, never .cache-dependent.
    assert_eq!(
        format!("{:x}", Sha256::digest(RAW)),
        "a8e3120399b3cbce4c5d7e91c8462e4c11097d377d57b9e1f6a8071f0ca49899"
    );
    let original = Document::load_mem(RAW).unwrap();
    let (bytes, map) = run(RAW).unwrap();
    let final_doc = Document::load_mem(&bytes).unwrap();
    let (_, annotations) =
        graph::pages_and_annotations(&final_doc, &input(), &mut Budget::new()).unwrap();
    assert_eq!(annotations.len(), 2);
    assert_eq!(map.pages.len(), 1);
    assert!(!bytes
        .windows(input().nonce.len())
        .any(|b| b == input().nonce.as_bytes()));
    for page in original.get_pages().values() {
        assert_eq!(
            original.get_page_content(*page).unwrap(),
            final_doc.get_page_content(*page).unwrap()
        );
    }
    for value in original.objects.values() {
        if let Ok(d) = value.as_dict() {
            if let Ok(id) = d.get(b"FontFile2").and_then(Object::as_reference) {
                assert_eq!(
                    original.get_object(id).unwrap(),
                    final_doc.get_object(id).unwrap()
                );
            }
        }
    }
    let witnesses = protected().iter().map(|p| p.witness_id.clone()).collect();
    let (_, before) = graph::pages_and_annotations_with_witnesses(
        &original,
        &input(),
        &witnesses,
        &mut Budget::new(),
    )
    .unwrap();
    for annotation in annotations {
        let old = before.iter().find(|old| old.key == annotation.key).unwrap();
        assert!((annotation.rect[1] - old.rect[1] - 3.).abs() < 0.0002);
        assert!((annotation.rect[3] - old.rect[3] - 3.).abs() < 0.0002);
        assert_eq!(
            (annotation.rect[0], annotation.rect[2]),
            (old.rect[0], old.rect[2])
        );
    }
}

fn without_real_links() -> Vec<u8> {
    let input = input();
    let mut doc = Document::load_mem(RAW).unwrap();
    let witnesses = protected().iter().map(|p| p.witness_id.clone()).collect();
    let mut budget = Budget::new();
    let (pages, annotations) =
        graph::pages_and_annotations_with_witnesses(&doc, &input, &witnesses, &mut budget).unwrap();
    let graph = Graph::read(&doc, &pages, &mut budget).unwrap();
    let real = annotations
        .iter()
        .filter(|a| a.temporary.is_none())
        .collect::<Vec<_>>();
    assert_eq!(real.len(), 2);
    let keys = real.iter().map(|a| a.key).collect::<BTreeSet<_>>();
    for annotation in &real {
        let tag = graph.objrs[&annotation.id].tag;
        let parent = graph.tags[&tag].parent;
        let children = graph.tags[&tag]
            .kids
            .iter()
            .filter_map(|kid| {
                if let graph::Kind::Tag(id) = &kid.kind {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(children.len(), 1);
        let old = doc
            .get_dictionary(parent)
            .unwrap()
            .get(b"K")
            .unwrap()
            .as_array()
            .unwrap()
            .clone();
        let mut next = Vec::new();
        for kid in old {
            if kid.as_reference().ok() == Some(tag) {
                next.extend(children.iter().copied().map(Object::Reference));
            } else {
                next.push(kid);
            }
        }
        doc.get_dictionary_mut(parent).unwrap().set("K", next);
        for child in children {
            doc.get_dictionary_mut(child).unwrap().set("P", parent);
        }
        doc.objects.remove(&tag);
        doc.objects.remove(&annotation.id);
    }
    for page in &pages {
        let retained = page
            .annotation_ids
            .iter()
            .filter(|id| !real.iter().any(|a| a.id == **id))
            .copied()
            .map(Object::Reference)
            .collect::<Vec<_>>();
        if let Some(id) = page.annotation_array {
            doc.objects.insert(id, Object::Array(retained));
        } else {
            doc.get_dictionary_mut(page.id)
                .unwrap()
                .set("Annots", retained);
        }
    }
    let retained = graph
        .tree
        .entries
        .iter()
        .filter(|(key, _)| !keys.contains(*key))
        .collect::<Vec<_>>();
    let nums = retained
        .iter()
        .flat_map(|(key, value)| [Object::Integer(**key), (*value).clone()])
        .collect::<Vec<_>>();
    let limits = vec![
        Object::Integer(*retained.first().unwrap().0),
        Object::Integer(*retained.last().unwrap().0),
    ];
    let tree = if let Some(root) = graph.tree.root {
        doc.get_dictionary_mut(root).unwrap()
    } else {
        doc.get_dictionary_mut(graph.root)
            .unwrap()
            .get_mut(b"ParentTree")
            .unwrap()
            .as_dict_mut()
            .unwrap()
    };
    tree.set("Nums", nums);
    tree.set("Limits", limits);
    save(&mut doc)
}
#[test]
fn no_real_links_still_requires_and_accepts_full_native_text_code_math_paint() {
    let raw = without_real_links();
    let mut requests = requests();
    requests[0].expected_links = 0;
    let (output, map) =
        extract_and_clean_with_baseline_and_paint(&raw, &input(), &requests, &protected()).unwrap();
    assert!(!map.hits.is_empty());
    let doc = Document::load_mem(&output).unwrap();
    let (_, annotations) =
        graph::pages_and_annotations(&doc, &input(), &mut Budget::new()).unwrap();
    assert!(annotations.is_empty());
    let broken = change_content(|b| {
        String::from_utf8(b)
            .unwrap()
            .replacen("/MCID 0", "/MCID 99", 1)
            .into_bytes()
    });
    assert!(run(&broken).is_err());
}
#[test]
fn actual_marked_content_coverage_rejects_missing_duplicate_wrong_mcid_and_unmarked_paint() {
    for change in [0, 1, 2, 3, 4] {
        let bytes = change_content(|b| {
            let text = String::from_utf8(b).unwrap();
            match change {
                0 => text.replacen("/MCID 0", "/MCID 99", 1),
                1 => text.replacen("/MCID 1/", "/MCID 0/", 1),
                2 => text.replacen("/Span<</MCID 0>>BDC", "/Artifact BMC", 1),
                3 => format!("{text}\n0 0 m 10 10 l 1 w S\n"),
                _ => text
                    .replacen(
                        "/Span<</MCID 0>>BDC",
                        "/Span<</MCID 0>>BDC /Artifact BMC",
                        1,
                    )
                    .replacen("ET\nQ\nEMC", "ET\nQ\nEMC EMC", 1),
            }
            .into_bytes()
        });
        assert!(run(&bytes).is_err(), "variant {change}");
    }
}
#[test]
fn equivalent_nonidentity_ctm_and_text_matrix_preserve_the_actual_glyph_proof() {
    let bytes = change_content(|b| {
        String::from_utf8(b)
            .unwrap()
            .replacen(
                "1 0 0 -1 34.015747 243.64882 cm",
                "2 0 0 -2 34.015747 243.64882 cm",
                1,
            )
            .replacen(
                "/f0 11 Tf 1 0 0 -1 0 0 Tm",
                "/f0 11 Tf 0.5 0 0 -0.5 0 0 Tm",
                1,
            )
            .into_bytes()
    });
    let (output, _) = run(&bytes).unwrap();
    let before = Document::load_mem(&bytes).unwrap();
    let after = Document::load_mem(&output).unwrap();
    for page in before.get_pages().values() {
        assert_eq!(
            before.get_page_content(*page).unwrap(),
            after.get_page_content(*page).unwrap()
        );
    }
}
#[test]
fn explicit_cid_to_gid_stream_is_resolved_from_the_embedded_font_program() {
    let mut doc = Document::load_mem(RAW).unwrap();
    let fonts = doc
        .objects
        .iter()
        .filter_map(|(id, o)| {
            o.as_dict()
                .ok()
                .filter(|d| {
                    d.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"CIDFontType2")
                })
                .map(|_| *id)
        })
        .collect::<Vec<_>>();
    for id in fonts {
        let map = (0..=u16::MAX)
            .flat_map(u16::to_be_bytes)
            .collect::<Vec<_>>();
        let stream = doc.add_object(Stream::new(dictionary! {}, map));
        doc.get_dictionary_mut(id)
            .unwrap()
            .set("CIDToGIDMap", stream);
    }
    assert!(run(&save(&mut doc)).is_ok());
}
#[test]
fn introduced_paper_edge_collision_and_invisible_target_text_are_not_accepted() {
    for change in [0, 1, 2] {
        let bytes = change_content(|b| {
            let text = String::from_utf8(b).unwrap();
            match change {
                0 => text.replacen("34.015747 243.64882 cm", "34.015747 999.64882 cm", 1),
                1 => text.replacen("34.015747 181.64882 cm", "34.015747 243.64882 cm", 1),
                _ => text.replacen("BT 0 Tr", "BT 3 Tr", 1),
            }
            .into_bytes()
        });
        assert!(run(&bytes).is_err(), "variant {change}");
    }
}
#[test]
fn protected_sources_cannot_be_missing_duplicated_or_claim_another_block() {
    let mut mismatch = requests();
    mismatch[0].expected_links = 1;
    assert!(
        extract_and_clean_with_baseline_and_paint(RAW, &input(), &mismatch, &protected()).is_err()
    );
    for kind in [0, 1, 2] {
        let mut p = protected();
        match kind {
            0 => {
                p.pop();
            }
            1 => p.push(p[0].clone()),
            _ => p[0].block_id = "other".into(),
        };
        assert!(extract_and_clean_with_baseline_and_paint(RAW, &input(), &requests(), &p).is_err());
    }
}
#[test]
fn visible_alpha_paths_cannot_be_misclassified_as_transparent_math_hotspots() {
    let mut doc = Document::load_mem(RAW).unwrap();
    let id = doc
        .objects
        .iter()
        .find_map(|(id, o)| {
            o.as_dict()
                .ok()
                .filter(|d| {
                    d.get(b"Type").and_then(Object::as_name).ok() == Some(b"ExtGState")
                        && d.get(b"ca").ok().and_then(super::super::number) == Some(0.)
                })
                .map(|_| *id)
        })
        .unwrap();
    // Independent MuPDF inspection retained in p04-first-failure-analysis:
    // the largest native hotspot is [100.19807,217.51662,265.52634,231.38105],
    // wholly inside [100.13194,217.44882,265.59247,231.44882]. Its original
    // geometry is valid even after making it visible; neither neighbour
    // reaches it. Preserve that positive instead of assuming a collision.
    doc.get_dictionary_mut(id).unwrap().set("ca", 1);
    let bytes = save(&mut doc);
    let (output, _) = run(&bytes).unwrap();
    let cleaned = Document::load_mem(&output).unwrap();
    assert_eq!(
        cleaned
            .get_dictionary(id)
            .unwrap()
            .get(b"ca")
            .ok()
            .and_then(super::super::number),
        Some(1.)
    );
    for page in doc.get_pages().values() {
        assert_eq!(
            doc.get_page_content(*page).unwrap(),
            cleaned.get_page_content(*page).unwrap()
        );
    }

    // Widen only the first actual hotspot from 25000 to 26000 SVG units.
    // Its full path now ends at PDF x=272.13948, beyond the unchanged
    // image clip x=265.59247. Alpha zero paints no ink; alpha one must
    // produce the precise clipped-ink error, with its real source identity.
    let overwide = change_content(|b| {
        let text = String::from_utf8(b).unwrap();
        let original = "0 -1279 m 25000 -1279 l 25000 817.5 l 0 817.5 l";
        assert_eq!(text.matches(original).count(), 1);
        text.replacen(
            original,
            "0 -1279 m 26000 -1279 l 26000 817.5 l 0 817.5 l",
            1,
        )
        .into_bytes()
    });
    let (transparent, _) = run(&overwide).unwrap();
    let transparent = Document::load_mem(&transparent).unwrap();
    assert_eq!(
        transparent
            .get_dictionary(id)
            .unwrap()
            .get(b"ca")
            .ok()
            .and_then(super::super::number),
        Some(0.)
    );
    let mut visible = Document::load_mem(&overwide).unwrap();
    visible.get_dictionary_mut(id).unwrap().set("ca", 1);
    let error = run(&save(&mut visible)).unwrap_err();
    let details: serde_json::Value =
        serde_json::from_str(error.strip_prefix("publication_layout_geometry: ").unwrap()).unwrap();
    assert_eq!(details["coordinateSpace"], "pdfBaselineInk");
    assert_eq!(details["reason"], "clipped-ink");
    assert_eq!(details["blockId"], "probe");
    assert_eq!(details["page"], 1);
    assert!(details["ink"][2].as_f64().unwrap() > 272.13);
}
#[test]
fn malformed_font_mapping_and_unknown_draw_operators_fail_before_cleanup() {
    for kind in [0, 1, 2] {
        let mut doc = Document::load_mem(RAW).unwrap();
        match kind {
            0 => {
                let id = doc
                    .objects
                    .iter()
                    .find_map(|(id, o)| {
                        o.as_dict()
                            .ok()
                            .filter(|d| {
                                d.get(b"Subtype").and_then(Object::as_name).ok()
                                    == Some(b"CIDFontType2")
                            })
                            .map(|_| *id)
                    })
                    .unwrap();
                let map = doc.add_object(Stream::new(dictionary! {}, vec![0; 4]));
                doc.get_dictionary_mut(id).unwrap().set("CIDToGIDMap", map);
            }
            1 => {
                let id = doc
                    .objects
                    .iter()
                    .find_map(|(id, o)| {
                        o.as_dict()
                            .ok()
                            .filter(|d| d.has(b"FontFile2"))
                            .map(|_| *id)
                    })
                    .unwrap();
                doc.get_dictionary_mut(id).unwrap().remove(b"FontFile2");
            }
            _ => {
                let bytes = change_content(|mut b| {
                    b.extend(b"\n/Sh0 sh\n");
                    b
                });
                assert!(run(&bytes).is_err());
                continue;
            }
        }
        assert!(run(&save(&mut doc)).is_err(), "font variant {kind}");
    }
}
#[test]
fn real_link_rectangles_must_cover_each_actual_glyph_and_every_fragment_uniquely() {
    let input = input();
    let mut doc = Document::load_mem(RAW).unwrap();
    let witnesses = protected().iter().map(|p| p.witness_id.clone()).collect();
    let (_, annotations) =
        graph::pages_and_annotations_with_witnesses(&doc, &input, &witnesses, &mut Budget::new())
            .unwrap();
    let annotation = annotations.iter().find(|a| a.temporary.is_none()).unwrap();
    let mut rect = annotation.rect;
    rect[0] += 2.;
    doc.get_dictionary_mut(annotation.id).unwrap().set(
        "Rect",
        rect.iter()
            .map(|n| Object::Real(*n as f32))
            .collect::<Vec<_>>(),
    );
    assert!(run(&save(&mut doc)).is_err());
}
