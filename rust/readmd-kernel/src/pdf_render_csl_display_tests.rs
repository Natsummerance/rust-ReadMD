//! Hand-authored physical expectations for recursive CSL bibliography flow.
use super::*;
use crate::publication_rich::{RichFormat, RichLayoutNode as Layout, RichNode};
use serde_json::json;

fn inline(text: &str) -> Layout {
    Layout::Inline {
        children: vec![RichNode::text(text)],
    }
}
fn paragraph(text: &str) -> Layout {
    Layout::Block {
        children: vec![inline(text)],
        indent: false,
    }
}
fn columns(label: &str, body: Vec<Layout>) -> Layout {
    Layout::Columns {
        left: vec![inline(label)],
        right: body,
    }
}
fn style(size: f64) -> Value {
    json!({
        "page":{"size":"A4","marginTop":20,"marginBottom":20,"marginLeft":18,"marginRight":18},
        "typography":{"font":"Helvetica","size":size,"lineHeight":1.6,"spacing":6,"color":"#262626","align":"left"},
        "footer":{"pageNumbers":false},"link":{"color":"#2b6cb0"}
    })
}
fn bibliography(flows: Vec<Vec<Layout>>, line: u32, entry: u32, hanging: bool) -> Value {
    json!({"type":"bibliography","complexLayout":true,"lineSpacing":line,"entrySpacing":entry,"hangingIndent":hanging,
        "items":flows.into_iter().enumerate().map(|(index,layout)|json!({"key":format!("key-{index}"),"layout":layout,"layoutError":null,
            "firstField":[],"text":[{"t":"text","v":"FORBIDDEN_FLATTENED_TEXT"}]})).collect::<Vec<_>>()})
}
fn rows_for(blk: &Value, style: &Value) -> Vec<Vec<CslRow>> {
    let geom = DocGeometry::from_style(style);
    let mut st = Styles::build(style, "Helvetica").body;
    st.leading = st.size * blk["lineSpacing"].as_u64().unwrap() as f64;
    st.space_before = 0.;
    st.space_after = st.leading * blk["entrySpacing"].as_u64().unwrap() as f64;
    csl_bibliography_story(
        blk,
        style,
        "",
        &|_| None,
        &st,
        &geom,
        blk["hangingIndent"].as_bool().unwrap(),
    )
    .unwrap()
    .into_iter()
    .map(|flow| match flow {
        Flowable::RichBibliographyEntry { rows, .. } => rows.unwrap(),
        _ => panic!("recursive bibliography expected"),
    })
    .collect()
}
fn document(blocks: Value, style: &Value) -> RenderedDoc {
    build(&blocks, style, "", &|_| None, &mut Vec::new()).unwrap()
}
fn text<'a>(doc: &'a RenderedDoc) -> Vec<(f64, f64, &'a str, &'a [TextSpan])> {
    doc.pages
        .iter()
        .flat_map(|page| &page.items)
        .filter_map(|item| match item {
            DrawItem::Text { x, y, text, spans } => Some((*x, *y, text.as_str(), spans.as_slice())),
            _ => None,
        })
        .collect()
}
fn cell<'a>(rows: &'a [CslRow], text: &str) -> &'a CslCell {
    rows.iter()
        .flat_map(|row| &row.cells)
        .find(|cell| cell.line.text == text)
        .unwrap_or_else(|| panic!("missing {text}"))
}

#[test]
fn csl_pdf_display_measures_all_labels_and_caps_each_nested_parent_independently() {
    for size in [8., 11., 18.] {
        for widest in ["[100]".to_string(), "W".repeat(80)] {
            let st = style(size);
            let geom = DocGeometry::from_style(&st);
            let blk = bibliography(
                vec![
                    vec![
                        paragraph("Heading"),
                        columns("[9]", vec![paragraph("FirstBody")]),
                    ],
                    vec![columns(
                        &widest,
                        vec![
                            paragraph("SecondBody"),
                            columns("[10]", vec![paragraph("NestedBody")]),
                        ],
                    )],
                ],
                2,
                0,
                false,
            );
            let entries = rows_for(&blk, &st);
            let preferred = str_width(&widest, FaceStyle::Regular, Face::Helvetica, size)
                .max(str_width("[10]", FaceStyle::Regular, Face::Helvetica, size));
            let left = preferred.min(geom.avail * 0.35);
            let right_x = left + size * 0.75;
            assert!((cell(&entries[0], "FirstBody").x - right_x).abs() < 1e-6);
            assert!((cell(&entries[1], "SecondBody").x - right_x).abs() < 1e-6);
            let right_width = geom.avail - right_x;
            let nested_x = right_x + preferred.min(right_width * 0.35) + size * 0.75;
            assert!((cell(&entries[1], "NestedBody").x - nested_x).abs() < 1e-6);
            assert!((cell(&entries[0], "Heading").x).abs() < 1e-6);
            let label_text = entries[1]
                .iter()
                .flat_map(|row| &row.cells)
                .filter(|cell| cell.x == 0.)
                .map(|cell| cell.line.text.as_str())
                .collect::<String>();
            assert_eq!(
                label_text, widest,
                "wrapping must neither duplicate nor truncate the label"
            );
            for entry in entries {
                for row in entry {
                    for cell in row.cells {
                        assert!(
                            cell.line.width <= cell.width + 1e-6
                                && cell.x + cell.width <= geom.avail + 1e-6
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn csl_pdf_display_hanging_is_per_full_width_paragraph_and_disabled_through_cells() {
    let st = style(11.);
    let words = (0..90)
        .map(|i| format!("word{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    let blk = bibliography(
        vec![vec![
            paragraph(&format!("First {words}")),
            paragraph(&format!("Second {words}")),
            columns(
                "[9]",
                vec![
                    paragraph(&format!("CellOne {words}")),
                    Layout::Block {
                        indent: true,
                        children: vec![paragraph(&format!("CellTwo {words}"))],
                    },
                ],
            ),
            Layout::Block {
                indent: true,
                children: vec![paragraph(&format!("After {words}"))],
            },
        ]],
        1,
        0,
        true,
    );
    let entries = rows_for(&blk, &st);
    let rows = &entries[0];
    let mut section = "";
    let right_x = str_width("[9]", FaceStyle::Regular, Face::Helvetica, 11.) + 8.25;
    for row in rows {
        for cell in &row.cells {
            if cell.line.text == "[9]" {
                continue;
            }
            let first = ["First", "Second", "CellOne", "CellTwo", "After"]
                .into_iter()
                .find(|name| cell.line.text.starts_with(name));
            if let Some(name) = first {
                section = name;
            }
            let expected = match section {
                "First" | "Second" => {
                    if first.is_some() {
                        0.
                    } else {
                        22.
                    }
                }
                "CellOne" => right_x,
                "CellTwo" => right_x + 44.,
                "After" => 44. + if first.is_some() { 0. } else { 22. },
                _ => panic!("unknown paragraph"),
            };
            assert!(
                (cell.x - expected).abs() < 1e-6,
                "{section}, first={first:?}, x={}, expected={expected}",
                cell.x
            );
        }
    }
    assert!(
        rows.iter()
            .flat_map(|row| &row.cells)
            .filter(|cell| cell.line.text.starts_with("word"))
            .count()
            > 10
    );
}

#[test]
fn csl_pdf_display_multiple_right_paragraphs_paginate_without_repeated_label_or_internal_entry_gaps(
) {
    let mut st = style(11.);
    st["page"] = json!({"size":"Custom","width":100,"height":100,"marginTop":8,"marginBottom":8,"marginLeft":8,"marginRight":8});
    let words = (0..420)
        .map(|i| format!("token{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    let blk = bibliography(
        vec![
            vec![
                paragraph("AuthorHeading"),
                columns("1.", vec![paragraph("ColumnHeading"), paragraph(&words)]),
                paragraph("EntryTail"),
            ],
            vec![paragraph("SecondEntry")],
        ],
        2,
        1,
        true,
    );
    let doc = document(
        json!([blk,{"type":"paragraph","text":[{"t":"text","v":"Following"}]}]),
        &st,
    );
    assert!(
        doc.pages.len() > 3,
        "one long column must split across real pages"
    );
    let all = doc
        .pages
        .iter()
        .map(|page| page.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for i in 0..420 {
        assert_eq!(
            all.split_whitespace()
                .filter(|word| *word == format!("token{i}"))
                .count(),
            1,
            "token{i}"
        );
    }
    assert_eq!(
        all.split_whitespace().filter(|word| *word == "1.").count(),
        1
    );
    assert!(!all.contains("FORBIDDEN_FLATTENED_TEXT"));
    let spacing = document(
        json!([
            bibliography(vec![vec![columns("1.", vec![paragraph("ColumnHeading")]), paragraph("EntryTail")], vec![paragraph("SecondEntry")]], 2, 1, true),
            {"type":"paragraph","text":[{"t":"text","v":"Following"}]}
        ]),
        &style(11.),
    );
    assert_eq!(spacing.pages.len(), 1);
    let texts = text(&spacing);
    let baseline = |word: &str| texts.iter().find(|item| item.2 == word).unwrap().1;
    assert!(
        (baseline("ColumnHeading") - baseline("EntryTail") - 22.).abs() < 1e-6,
        "display paragraphs inside an entry add no entry gap"
    );
    assert!((baseline("EntryTail") - baseline("SecondEntry") - 44.).abs() < 1e-6);
    assert!((baseline("SecondEntry") - baseline("Following") - 22.).abs() < 1e-6);
    for page in &doc.pages {
        for item in &page.items {
            if let DrawItem::Text { x, y, spans, .. } = item {
                assert!(*x >= doc.geom.left + FRAME_PAD - 1e-6);
                assert!(
                    *x + spans.iter().map(|span| span.advance).sum::<f64>()
                        <= doc.geom.page_w - doc.geom.right - FRAME_PAD + 1e-6
                );
                assert!(
                    *y > doc.geom.bottom + FRAME_PAD
                        && *y < doc.geom.page_h - doc.geom.top - FRAME_PAD
                );
            }
        }
    }
    let parsed = lopdf::Document::load_mem(&write_pdf(&doc)).unwrap();
    assert_eq!(parsed.get_pages().len(), doc.pages.len());
}

#[test]
fn csl_pdf_display_keeps_real_font_script_underline_and_hyperlink_operations() {
    let format = |value: Value| serde_json::from_value::<RichFormat>(value).unwrap();
    let rich = Layout::Inline {
        children: vec![
            RichNode::text("Base "),
            RichNode::Text {
                text: "Rich".into(),
                format: format(
                    json!({"font_style":"italic","font_weight":"bold","text_decoration":"underline"}),
                ),
            },
            RichNode::Text {
                text: "2".into(),
                format: format(json!({"vertical_align":"sup"})),
            },
            RichNode::Text {
                text: "j".into(),
                format: format(json!({"vertical_align":"sub"})),
            },
        ],
    };
    let link = Layout::Link {
        href: "https://example.test/evidence?a=1&b=2".into(),
        children: vec![paragraph("LinkedOne"), paragraph("LinkedTwo")],
    };
    let doc = document(
        json!([bibliography(
            vec![vec![columns("[1]", vec![rich, link])]],
            2,
            0,
            false
        )]),
        &style(11.),
    );
    let texts = text(&doc);
    let base = texts.iter().find(|item| item.2.contains("Base")).unwrap();
    let sup = texts.iter().find(|item| item.2 == "2").unwrap();
    let sub = texts.iter().find(|item| item.2 == "j").unwrap();
    assert!(base
        .3
        .iter()
        .any(|span| span.face == Face::HelveticaBoldOblique));
    assert!(sup.1 > base.1 && sub.1 < base.1);
    assert!((sup.3[0].size - 7.7).abs() < 1e-6);
    assert_eq!(doc.pages.iter().flat_map(|page| &page.items).filter(|item| matches!(item,DrawItem::Link { href, .. } if href == "https://example.test/evidence?a=1&b=2")).count(), 2);
    assert!(doc.pages.iter().flat_map(|page| &page.items).any(
        |item| matches!(item,DrawItem::Line { x1, x2, width, .. } if x2 > x1 && *width >= 0.4)
    ));
    let parsed = lopdf::Document::load_mem(&write_pdf(&doc)).unwrap();
    assert_eq!(
        parsed
            .objects
            .values()
            .filter_map(|object| object.as_dict().ok())
            .filter(|dict| dict
                .get(b"Subtype")
                .ok()
                .and_then(|object| object.as_name().ok())
                == Some(b"Link"))
            .count(),
        2
    );
    let content = parsed
        .get_page_content(*parsed.get_pages().values().next().unwrap())
        .unwrap();
    let operators = lopdf::content::Content::decode(&content).unwrap();
    assert!(operators
        .operations
        .iter()
        .any(|operation| operation.operator == "S"));
}

#[test]
fn csl_pdf_display_rejects_invalid_transport_and_geometry_before_touching_target() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("protected.pdf");
    let sentinel = b"existing foreign target bytes";
    std::fs::write(&path, sentinel).unwrap();
    let valid = bibliography(vec![vec![paragraph("Valid")]], 1, 0, false);
    let mut cases = Vec::new();
    for layout in [
        Value::Null,
        json!({}),
        json!([{"type":"not-a-layout"}]),
        json!([{"type":"inline","children":[{"type":"group","children":[]}]}]),
        json!([{"type":"link","href":"javascript:alert(1)","children":[]}]),
    ] {
        let mut bad = valid.clone();
        bad["items"][0]["layout"] = layout;
        cases.push((bad, style(11.), "academic_rich_"));
    }
    let mut bad = valid.clone();
    bad["items"][0]["layoutError"] = json!("academic_rich_budget");
    cases.push((bad, style(11.), "academic_rich_"));
    let mut bad = valid.clone();
    bad["complexLayout"] = json!("true");
    cases.push((bad, style(11.), "academic_rich_"));
    let mut nested = inline("TooDeep");
    for _ in 0..40 {
        nested = Layout::Block {
            children: vec![nested],
            indent: false,
        };
    }
    cases.push((
        bibliography(vec![vec![nested]], 1, 0, false),
        style(11.),
        "academic_rich_budget",
    ));
    let mut nested = inline("CannotFit");
    for _ in 0..12 {
        nested = Layout::Block {
            children: vec![nested],
            indent: true,
        };
    }
    cases.push((
        bibliography(vec![vec![nested]], 1, 0, false),
        style(11.),
        "publication_layout_geometry",
    ));
    for (blk, st, code) in cases {
        let error = render(
            &json!([blk]),
            path.to_str().unwrap(),
            &st,
            "",
            &|_| None,
            &mut Vec::new(),
        )
        .unwrap_err();
        assert_eq!(error.kind, PdfErrorKind::Invalid);
        assert!(error.message.contains(code), "{error:?}");
        assert_eq!(std::fs::read(&path).unwrap(), sentinel);
    }
}

#[test]
fn csl_pdf_display_empty_scopes_add_no_rows_and_short_hanging_can_fit_without_a_continuation() {
    let st = style(11.);
    let style = Styles::build(&st, "Helvetica").body;
    let runs = vec![text_run("i", FaceStyle::Regular, 11., style.color)];
    let width = str_width("i", FaceStyle::Regular, Face::Helvetica, 11.) + 0.01;
    let rows = csl_paragraph_rows(&runs, 0., width, &style, true, 100.).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].cells[0].line.text, "i");
    assert!(csl_paragraph_rows(
        &vec![text_run("ii", FaceStyle::Regular, 11., style.color)],
        0.,
        width,
        &style,
        true,
        100.
    )
    .is_err());
    let empty = vec![
        Layout::Block {
            children: vec![],
            indent: true,
        },
        Layout::Columns {
            left: vec![],
            right: vec![],
        },
    ];
    let entries = rows_for(
        &bibliography(vec![empty, vec![paragraph("Visible")]], 1, 2, false),
        &st,
    );
    assert!(entries[0].is_empty() && entries[1].len() == 1);
}

#[test]
fn csl_pdf_display_simple_first_field_route_is_byte_identical_when_complex_flag_is_false() {
    for size in [8., 11., 18.] {
        let st = style(size);
        let old = json!([{"type":"bibliography","hangingIndent":true,"lineSpacing":2,"entrySpacing":1,"items":[
            {"firstField":[{"t":"text","v":"[9]"}],"text":[{"t":"text","v":"First body"}]},
            {"firstField":[{"t":"text","v":"[100]"}],"text":[{"t":"text","v":"Second body"}]}
        ]}]);
        let mut transported = old.clone();
        transported[0]["complexLayout"] = json!(false);
        transported[0]["items"][0]["layout"] = json!([paragraph("unused transport")]);
        let before = document(old, &st);
        let after = document(transported, &st);
        assert_eq!(write_pdf(&before), write_pdf(&after));
    }
}

#[test]
fn csl_pdf_display_typed_source_projection_reaches_physical_paragraphs_and_preserves_original_files(
) {
    const CSL: &str = r#"<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text"><info><title>ReadMD original PDF display test</title><id>https://readmd.test/pdf-display</id></info><citation><layout><text variable="citation-number"/></layout></citation><bibliography><layout><text variable="title" display="block"/><text variable="volume" display="left-margin"/><text variable="container-title" display="right-inline" font-style="italic"/><text variable="abstract" display="indent"/></layout></bibliography></style>"#;
    let references = r#"[{"id":"a","type":"book","title":"AuthorHeading","volume":"100","container-title":"ColumnBody","abstract":"Annotation"}]"#;
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("references.json"), references).unwrap();
    std::fs::write(directory.path().join("display.csl"), CSL).unwrap();
    let source = "---\r\nbibliography: references.json\r\ncsl: display.csl\r\n---\r\nSource [@a].\r\n\r\n<!--readmd:bibliography-->\r\n";
    let prepared = crate::publication::prepare(source, directory.path().to_str().unwrap()).unwrap();
    let original = prepared.projection.bibliography.clone();
    let ast = crate::md_ast::parse_prepared(source, &prepared.projection).unwrap();
    let blocks = crate::md_ast::to_render_json(&ast.blocks);
    assert!(blocks
        .as_array()
        .unwrap()
        .iter()
        .any(|blk| blk["type"] == "bibliography"
            && blk["complexLayout"] == true
            && blk["items"][0]["layout"].is_array()));
    let doc = document(blocks, &style(11.));
    let texts = text(&doc);
    let heading = texts.iter().find(|item| item.2 == "AuthorHeading").unwrap();
    let body = texts.iter().find(|item| item.2 == "ColumnBody").unwrap();
    let note = texts.iter().find(|item| item.2 == "Annotation").unwrap();
    assert!(heading.1 > body.1 && body.1 > note.1);
    assert!((note.0 - heading.0 - 44.).abs() < 1e-6);
    assert!(body
        .3
        .iter()
        .any(|span| span.face == Face::HelveticaOblique));
    assert!(!doc
        .pages
        .iter()
        .any(|page| page.text.contains("readmd-csl")
            || page.text.contains(&prepared.projection.nonce)));
    prepared.projection.verify_source(source).unwrap();
    assert_eq!(prepared.projection.bibliography, original);
    assert_eq!(
        std::fs::read_to_string(directory.path().join("references.json")).unwrap(),
        references
    );
    assert_eq!(
        std::fs::read_to_string(directory.path().join("display.csl")).unwrap(),
        CSL
    );
}
