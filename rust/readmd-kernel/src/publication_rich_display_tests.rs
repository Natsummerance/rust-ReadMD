//! Hand-authored structure expectations follow CSL 1.0.2 Display. These unit
//! tests are not copies of, or substitutes for, the private official RESULTs.
use super::*;
use serde_json::json;

fn group(display: Option<RichDisplay>, entry: usize, children: Vec<RichNode>) -> RichNode {
    RichNode::Group {
        children,
        display,
        entry_index: Some(entry),
    }
}
fn block(display: RichDisplay, text: &str) -> RichNode {
    group(Some(display), 7, vec![RichNode::text(text)])
}
fn pair(label: &str, body: &str) -> Vec<RichNode> {
    vec![
        block(RichDisplay::LeftMargin, label),
        block(RichDisplay::RightInline, body),
    ]
}
fn item(nodes: Vec<RichNode>) -> RichBibliographyItem {
    RichBibliographyItem {
        key: "source-key".into(),
        rich: nodes,
        first_field: vec![],
    }
}
fn walk<'a>(nodes: &'a [RichLayoutNode], output: &mut Vec<&'a RichLayoutNode>) {
    for node in nodes {
        output.push(node);
        match node {
            RichLayoutNode::Inline { .. } => {}
            RichLayoutNode::Columns { left, right } => {
                walk(left, output);
                walk(right, output);
            }
            RichLayoutNode::Block { children, .. }
            | RichLayoutNode::Group { children, .. }
            | RichLayoutNode::Link { children, .. } => walk(children, output),
        }
    }
}
fn all(nodes: &[RichLayoutNode]) -> Vec<&RichLayoutNode> {
    let mut output = Vec::new();
    walk(nodes, &mut output);
    output
}
fn raw_text(nodes: &[RichLayoutNode]) -> String {
    nodes
        .iter()
        .map(|node| match node {
            RichLayoutNode::Inline { children } => plain(children),
            RichLayoutNode::Columns { left, right } => raw_text(left) + &raw_text(right),
            RichLayoutNode::Block { children, .. }
            | RichLayoutNode::Group { children, .. }
            | RichLayoutNode::Link { children, .. } => raw_text(children),
        })
        .collect()
}
fn columns(nodes: &[RichLayoutNode]) -> usize {
    all(nodes)
        .iter()
        .filter(|node| matches!(node, RichLayoutNode::Columns { .. }))
        .count()
}

#[test]
fn display_layout_preserves_multiple_rows_full_width_headings_and_annotation() {
    let mut nodes = vec![block(RichDisplay::Block, "Author heading")];
    nodes.extend(pair("[9]", "First body"));
    nodes.push(block(RichDisplay::Indent, "Annotation"));
    nodes.extend(pair("[100]", "Second body"));
    nodes.push(block(RichDisplay::Block, "Closing paragraph"));
    let mut item = item(nodes);
    let original = item.clone();
    item.normalize_display_columns().unwrap();
    assert_eq!(item, original);
    let flow = item.layout().unwrap();
    assert_eq!(columns(&flow), 2);
    assert_eq!(
        all(&flow)
            .iter()
            .filter(|node| matches!(node, RichLayoutNode::Block { indent: true, .. }))
            .count(),
        1
    );
    assert_eq!(
        raw_text(&flow),
        "Author heading[9]First bodyAnnotation[100]Second bodyClosing paragraph"
    );
    assert!(
        matches!(flow.first(), Some(RichLayoutNode::Group { children, .. })
        if matches!(children.first(), Some(RichLayoutNode::Block { indent: false, .. })))
    );
    item.normalize_display_columns().unwrap();
    assert_eq!(item, original, "normalization is idempotent");
}

#[test]
fn display_layout_requires_flow_only_for_real_recursive_structure() {
    let inline = |text: &str| RichLayoutNode::Inline {
        children: vec![RichNode::text(text)],
    };
    let identity = |children| RichLayoutNode::Group {
        children,
        entry_index: Some(7),
    };
    let block = |children, indent| RichLayoutNode::Block { children, indent };
    let pair = RichLayoutNode::Columns {
        left: vec![identity(vec![inline("[100]")])],
        right: vec![RichLayoutNode::Link {
            href: "https://example.test/body".into(),
            children: vec![identity(vec![inline("Body")])],
        }],
    };
    let compatible = vec![
        vec![],
        vec![inline("Plain")],
        vec![
            identity(vec![
                inline("First "),
                RichLayoutNode::Link {
                    href: "https://example.test/inline".into(),
                    children: vec![inline("linked")],
                },
            ]),
            inline(" last"),
        ],
        vec![pair.clone()],
        vec![identity(vec![identity(vec![pair.clone()])])],
        vec![block(vec![identity(vec![inline("Full width")])], false)],
        vec![identity(vec![block(vec![inline("Full width")], false)])],
    ];
    for flow in compatible {
        assert!(!layout_requires_flow(&flow), "compatible flow: {flow:?}");
    }
    let recursive = vec![
        vec![
            block(vec![inline("One")], false),
            block(vec![inline("Two")], false),
        ],
        vec![block(vec![inline("Indented")], true)],
        vec![block(vec![block(vec![inline("Nested")], false)], false)],
        vec![block(vec![pair.clone()], false)],
        vec![RichLayoutNode::Columns {
            left: vec![inline("Outer")],
            right: vec![pair.clone()],
        }],
        vec![RichLayoutNode::Columns {
            left: vec![inline("Label")],
            right: vec![block(vec![inline("Paragraph")], false)],
        }],
        vec![pair.clone(), pair.clone()],
        vec![inline("Prefix"), pair.clone()],
        vec![pair.clone(), inline("Suffix")],
        vec![RichLayoutNode::Link {
            href: "https://example.test/whole-row".into(),
            children: vec![pair],
        }],
        vec![RichLayoutNode::Link {
            href: "https://example.test/whole-block".into(),
            children: vec![block(vec![inline("Owned block")], false)],
        }],
    ];
    for flow in recursive {
        assert!(layout_requires_flow(&flow), "recursive flow: {flow:?}");
    }
}

#[test]
fn display_layout_requires_flow_ignores_only_ascii_trivia_around_a_compatible_boundary() {
    let inline = |text: &str| RichLayoutNode::Inline {
        children: vec![RichNode::text(text)],
    };
    let boundary = RichLayoutNode::Columns {
        left: vec![inline("[9]")],
        right: vec![inline("Body")],
    };
    for trivia in ["", " ", "\t\r\n\x0c"] {
        let flow = vec![
            inline(trivia),
            RichLayoutNode::Group {
                entry_index: Some(4),
                children: vec![boundary.clone()],
            },
            RichLayoutNode::Link {
                href: "https://example.test/trivia".into(),
                children: vec![inline(trivia)],
            },
        ];
        assert!(!layout_requires_flow(&flow));
    }
    for visible in ["\u{a0}", "\u{2003}", ".", "suffix"] {
        assert!(
            layout_requires_flow(&[boundary.clone(), inline(visible)]),
            "{visible:?} is content, not XML indentation"
        );
    }
    let mut item = RichBibliographyItem {
        key: "unpaired".into(),
        first_field: vec![],
        rich: vec![group(
            Some(RichDisplay::RightInline),
            5,
            vec![RichNode::text("Full width")],
        )],
    };
    item.normalize_display_columns().unwrap();
    let original = item.clone();
    assert!(!layout_requires_flow(&item.layout().unwrap()));
    assert_eq!(
        item, original,
        "the predicate must not erase original display or entry identity"
    );
}

#[test]
fn display_layout_recurses_inside_columns_blocks_and_nested_indents() {
    let nested = group(Some(RichDisplay::Indent), 20, pair("inner", "Inner body"));
    let body = group(
        Some(RichDisplay::RightInline),
        21,
        vec![
            block(RichDisplay::Block, "Paragraph one"),
            group(Some(RichDisplay::Indent), 22, vec![nested]),
            block(RichDisplay::Block, "Paragraph two"),
        ],
    );
    let nodes = vec![block(RichDisplay::LeftMargin, "outer"), body];
    let flow = layout(&nodes).unwrap();
    assert_eq!(columns(&flow), 2);
    assert_eq!(
        all(&flow)
            .iter()
            .filter(|node| matches!(node, RichLayoutNode::Block { indent: true, .. }))
            .count(),
        2
    );
    let RichLayoutNode::Columns { right, .. } = &flow[0] else {
        panic!("outer column row missing")
    };
    assert_eq!(
        columns(right),
        1,
        "nested row must stay within its outer body cell"
    );
    assert_eq!(
        raw_text(&flow),
        "outerParagraph oneinnerInner bodyParagraph two"
    );
    let mut item = item(nodes);
    let original = item.clone();
    item.normalize_display_columns().unwrap();
    assert_eq!(item, original);
}

#[test]
fn display_layout_pairs_transparent_groups_but_not_real_block_or_text_boundaries() {
    let left = group(None, 10, vec![block(RichDisplay::LeftMargin, "label")]);
    let right = group(None, 11, vec![block(RichDisplay::RightInline, "body")]);
    let nodes = vec![left.clone(), RichNode::text(" \r\n"), right.clone()];
    let flow = layout(&nodes).unwrap();
    assert_eq!(columns(&flow), 1);
    assert_eq!(raw_text(&flow), "label \r\nbody");
    for entry in [10, 11] {
        assert!(all(&flow).iter().any(|node| matches!(node, RichLayoutNode::Group { entry_index: Some(actual), .. } if *actual == entry)));
    }
    for barrier in [
        RichNode::text("authored middle"),
        block(RichDisplay::Block, "Block"),
        block(RichDisplay::Indent, "Indent"),
    ] {
        let flow = layout(&[left.clone(), barrier, right.clone()]).unwrap();
        assert_eq!(
            columns(&flow),
            0,
            "a label cannot jump across another content block"
        );
        assert!(!layout_has_columns(&flow));
        assert_eq!(raw_text(&flow).matches("label").count(), 1);
        assert_eq!(raw_text(&flow).matches("body").count(), 1);
    }
}

#[test]
fn display_layout_keeps_consecutive_right_blocks_as_one_label_and_distinct_paragraphs() {
    let mut nodes = pair("label", "Paragraph one");
    nodes.push(block(RichDisplay::RightInline, "Paragraph two"));
    nodes.push(block(RichDisplay::Indent, "Following annotation"));
    let flow = layout(&nodes).unwrap();
    assert_eq!(columns(&flow), 1);
    let RichLayoutNode::Columns { left, right } = &flow[0] else {
        panic!("missing columns")
    };
    assert_eq!(raw_text(left), "label");
    assert_eq!(
        right
            .iter()
            .filter(|node| matches!(node, RichLayoutNode::Block { indent: false, .. }))
            .count(),
        2
    );
    assert_eq!(raw_text(right), "Paragraph oneParagraph two");
    assert!(!raw_text(right).contains("Following annotation"));
}

#[test]
fn display_layout_existing_first_field_can_itself_contain_nested_rows_without_losing_them() {
    let mut item = RichBibliographyItem {
        key: "already-split".into(),
        first_field: pair("inner label", "inner body"),
        rich: pair("body label", "body body"),
    };
    let original = item.clone();
    item.normalize_display_columns().unwrap();
    assert_eq!(item, original);
    let flow = item.layout().unwrap();
    assert_eq!(columns(&flow), 3);
    let RichLayoutNode::Columns { left, right } = &flow[0] else {
        panic!("processor split missing")
    };
    assert_eq!(columns(left), 1);
    assert_eq!(columns(right), 1);
    assert_eq!(raw_text(&flow), "inner labelinner bodybody labelbody body");
    let wrapped = RichBibliographyItem {
        key: "wrapped-field".into(),
        first_field: vec![group(Some(RichDisplay::LeftMargin), 30, pair("a", "b"))],
        rich: vec![block(RichDisplay::RightInline, "Outer body")],
    };
    let flow = wrapped.layout().unwrap();
    assert_eq!(columns(&flow), 2);
    assert_eq!(raw_text(&flow), "abOuter body");
}

#[test]
fn display_layout_retains_format_link_identity_unicode_and_json_roundtrip() {
    let formatted = RichNode::Text {
        text: "😀 Abcxyz\r\n中文".into(),
        format: RichFormat {
            font_style: RichFontStyle::Italic,
            font_variant: RichFontVariant::SmallCaps,
            font_weight: RichFontWeight::Bold,
            text_decoration: RichDecoration::Underline,
            vertical_align: RichVerticalAlign::Sup,
        },
    };
    let nodes = vec![RichNode::Link {
        href: "https://example.test/evidence".into(),
        children: vec![
            group(Some(RichDisplay::Block), 71, vec![formatted.clone()]),
            group(Some(RichDisplay::Indent), 72, pair("[12]", "Linked body")),
        ],
    }];
    let flow = layout(&nodes).unwrap();
    assert!(
        matches!(&flow[0], RichLayoutNode::Link { href, .. } if href == "https://example.test/evidence")
    );
    assert_eq!(columns(&flow), 1);
    assert!(all(&flow).iter().any(
        |node| matches!(node, RichLayoutNode::Inline { children } if children.contains(&formatted))
    ));
    assert_eq!(raw_text(&flow), "😀 Abcxyz\r\n中文[12]Linked body");
    let json = serde_json::to_value(&flow).unwrap();
    assert_eq!(
        serde_json::from_value::<Vec<RichLayoutNode>>(json).unwrap(),
        flow
    );
}

#[test]
fn display_layout_enforces_original_rich_budgets_and_safe_links() {
    let too_many = vec![RichNode::text("x"); MAX_RICH_NODES + 1];
    assert_eq!(layout(&too_many).unwrap_err().code, "academic_rich_budget");
    let mut nested = vec![RichNode::text("leaf")];
    for depth in 0..=MAX_RICH_DEPTH {
        nested = vec![group(Some(RichDisplay::Indent), depth, nested)];
    }
    assert_eq!(layout(&nested).unwrap_err().code, "academic_rich_budget");
    let unsafe_link = vec![RichNode::Link {
        href: "javascript:alert(1)".into(),
        children: pair("x", "y"),
    }];
    assert_eq!(layout(&unsafe_link).unwrap_err().code, "academic_rich_link");
    assert!(layout(&vec![RichNode::text("x"); MAX_RICH_NODES]).is_ok());
    assert!(layout(&[]).unwrap().is_empty());
}

#[test]
fn display_empty_blocks_keep_identity_without_blank_paragraphs_but_nbsp_stays_content() {
    let nodes = vec![
        group(Some(RichDisplay::Block), 41, vec![]),
        block(RichDisplay::Indent, " \r\n"),
        block(RichDisplay::LeftMargin, ""),
        block(RichDisplay::RightInline, ""),
        RichNode::text("Body"),
    ];
    let flow = layout(&nodes).unwrap();
    assert!(layout_is_inline(&flow));
    assert_eq!(columns(&flow), 0);
    assert_eq!(raw_text(&flow), " \r\nBody");
    assert!(all(&flow).iter().any(|node| matches!(
        node,
        RichLayoutNode::Group {
            entry_index: Some(41),
            ..
        }
    )));
    assert!(!layout_html(&flow)
        .unwrap()
        .contains("readmd-csl-layout-block"));
    let visible_space = layout(&[block(RichDisplay::Block, "\u{a0}")]).unwrap();
    assert!(
        !layout_is_inline(&visible_space),
        "NBSP produces a real line box"
    );
    assert!(layout_html(&visible_space)
        .unwrap()
        .contains("readmd-csl-layout-block"));
}

#[test]
fn display_layout_html_keeps_recursive_blocks_links_text_and_adaptive_unclipped_cells() {
    let nodes = vec![
        block(RichDisplay::Block, "Full width"),
        block(RichDisplay::LeftMargin, "Very long label"),
        group(
            Some(RichDisplay::RightInline),
            81,
            vec![RichNode::Link {
                href: "https://example.test/\"<evidence>".into(),
                children: vec![
                    group(
                        Some(RichDisplay::Indent),
                        82,
                        pair("Inner label", "<script>literal</script>"),
                    ),
                    block(RichDisplay::Block, "Following paragraph"),
                ],
            }],
        ),
    ];
    let flow = layout(&nodes).unwrap();
    let html = layout_html(&flow).unwrap();
    assert_eq!(
        html.matches("class=\"readmd-csl-layout-columns\"").count(),
        2
    );
    assert_eq!(html.matches("class=\"readmd-csl-layout-left\"").count(), 2);
    assert_eq!(html.matches("fit-content(35%) minmax(0,1fr)").count(), 2);
    assert!(html.contains("readmd-csl-layout-indent") && html.contains("padding-left:4em"));
    assert!(html.contains("overflow-wrap:anywhere") && html.contains("column-gap:.75em"));
    assert!(
        !html.contains("overflow:hidden")
            && !html.contains("text-overflow")
            && !html.contains("<script>")
    );
    assert!(
        html.contains("&lt;script&gt;literal&lt;/script&gt;")
            && html.contains("&quot;&lt;evidence&gt;")
    );
    assert_eq!(html.matches("Following paragraph").count(), 1);
    let invalid = vec![RichLayoutNode::Link {
        href: "javascript:alert(1)".into(),
        children: flow.clone(),
    }];
    assert_eq!(
        layout_html(&invalid).unwrap_err().code,
        "academic_rich_link"
    );
    let invalid_inline = vec![RichLayoutNode::Inline { children: nodes }];
    assert_eq!(
        layout_html(&invalid_inline).unwrap_err().code,
        "academic_rich_unsupported"
    );
}

#[test]
fn display_reader_complex_bibliography_keeps_source_and_does_not_forge_measured_ready() {
    let source = "😀 Before\r\n\r\n<!--readmd:bibliography-->\r\n\r\nAfter\r\n";
    let marker = "<!--readmd:bibliography-->";
    let at = source.find(marker).unwrap();
    let mut nodes = vec![block(RichDisplay::Block, "Author heading")];
    nodes.extend(pair("[100]", "First body"));
    nodes.push(group(
        Some(RichDisplay::Indent),
        91,
        pair("Nested label", "Nested body"),
    ));
    nodes.extend(pair("[9000]", "Second body"));
    let complex = item(nodes);
    let original = complex.clone();
    let mut bibliography = RichBibliography {
        title: "References".into(),
        items: vec![
            RichBibliographyItem {
                key: "simple".into(),
                rich: vec![RichNode::text("Simple body")],
                first_field: vec![RichNode::text("[9]")],
            },
            complex,
        ],
        hanging_indent: false,
        line_spacing: 2,
        entry_spacing: 1,
        source_span: Some(SourceSpan::new(source, at..at + marker.len()).unwrap()),
    };
    bibliography.normalize_layout().unwrap();
    assert_eq!(bibliography.items[1], original);
    let mut projection = Projection::new(source);
    projection.bibliography = Some(bibliography);
    let output = projection.reader_content(source).unwrap();
    assert!(
        output.contains("readmd-bibliography-complex")
            && output.contains("data-csl-layout=\"complex\"")
    );
    assert_eq!(
        output
            .matches("class=\"readmd-csl-layout-columns\"")
            .count(),
        4
    );
    assert!(output.contains("margin-bottom:2em") && output.contains("margin-bottom:0em"));
    assert!(
        !output.contains("data-csl-layout-ready")
            && !output.contains("data-slot=")
            && !output.contains(&projection.nonce)
    );
    assert!(output.starts_with("😀 Before\r\n\r\n") && output.ends_with("\r\n\r\nAfter\r\n"));
    assert_eq!(projection.bibliography.as_ref().unwrap().items[1], original);
    assert!(projection
        .reader_content(&source.replace("Before", "Changed"))
        .is_err());
}

#[test]
fn display_reader_hangs_each_full_width_paragraph_but_no_column_descendant() {
    let inline = |text: &str| RichLayoutNode::Inline {
        children: vec![RichNode::text(text)],
    };
    let paragraph = |text: &str| RichLayoutNode::Block {
        children: vec![inline(text)],
        indent: false,
    };
    let flow = vec![
        inline("Prefix "),
        RichLayoutNode::Group {
            children: vec![inline("continued ")],
            entry_index: Some(3),
        },
        RichLayoutNode::Link {
            href: "https://example.test/evidence".into(),
            children: vec![inline("linked")],
        },
        paragraph("Before columns"),
        RichLayoutNode::Columns {
            left: vec![paragraph("Long label")],
            right: vec![
                paragraph("Column paragraph one"),
                RichLayoutNode::Block {
                    indent: true,
                    children: vec![paragraph("Indented cell paragraph")],
                },
                RichLayoutNode::Columns {
                    left: vec![inline("Nested label")],
                    right: vec![paragraph("Nested cell paragraph")],
                },
                paragraph("Column paragraph two"),
            ],
        },
        RichLayoutNode::Block {
            indent: true,
            children: vec![paragraph("After one"), paragraph("After two")],
        },
    ];
    let output = layout_html_with_hanging(&flow, true).unwrap();
    let xml = format!("<root>{output}</root>");
    use crate::math_document::{parse_xml, XmlNode};
    fn class(node: &XmlNode, name: &str) -> bool {
        node.attributes
            .get("class")
            .is_some_and(|value| value.split_ascii_whitespace().any(|part| part == name))
    }
    fn visit<'a>(
        node: &'a XmlNode,
        hanging_ancestors: usize,
        in_cell: bool,
        nodes: &mut Vec<&'a XmlNode>,
    ) {
        let hanging = class(node, "readmd-csl-layout-hanging");
        if hanging {
            assert_eq!(
                hanging_ancestors, 0,
                "nested block wrappers must not compound hanging"
            );
            assert!(!in_cell, "the entire cell subtree disables hanging");
        }
        if class(node, "readmd-csl-layout-columns") {
            assert_eq!(
                hanging_ancestors, 0,
                "a hanging paragraph must not wrap a column row"
            );
        }
        nodes.push(node);
        let in_cell = in_cell
            || class(node, "readmd-csl-layout-left")
            || class(node, "readmd-csl-layout-right");
        for child in node.children() {
            visit(
                child,
                hanging_ancestors + usize::from(hanging),
                in_cell,
                nodes,
            );
        }
    }
    let document = parse_xml(&xml).unwrap();
    let mut nodes = Vec::new();
    visit(&document, 0, false, &mut nodes);
    let hanging: Vec<_> = nodes
        .iter()
        .copied()
        .filter(|node| class(node, "readmd-csl-layout-hanging"))
        .collect();
    assert_eq!(
        hanging.len(),
        4,
        "one prefix paragraph and three full-width explicit paragraphs"
    );
    assert_eq!(hanging[0].text(), "Prefix continued linked");
    assert!(nodes
        .iter()
        .any(|node| node.attributes.get("href").map(String::as_str)
            == Some("https://example.test/evidence")
            && node.text() == "linked"));
    assert!(!layout_html(&flow)
        .unwrap()
        .contains("readmd-csl-layout-hanging"));
}

#[test]
fn display_reader_flow_entries_do_not_inherit_legacy_grid_or_entry_hanging() {
    let source = "😀 Source\n\n<!--readmd:bibliography-->\n\nAfter";
    let marker = "<!--readmd:bibliography-->";
    let at = source.find(marker).unwrap();
    let mut rich = vec![block(RichDisplay::Block, "Heading")];
    rich.extend(pair("[100]", "Body"));
    rich.push(block(RichDisplay::Block, "Closing"));
    let mut projection = Projection::new(source);
    projection.bibliography = Some(RichBibliography {
        title: "References".into(),
        hanging_indent: true,
        line_spacing: 2,
        entry_spacing: 1,
        source_span: Some(SourceSpan::new(source, at..at + marker.len()).unwrap()),
        items: vec![
            RichBibliographyItem {
                key: "simple".into(),
                first_field: vec![RichNode::text("[9]")],
                rich: vec![RichNode::text("Simple body")],
            },
            item(rich),
        ],
    });
    let original = projection.bibliography.clone();
    let output = projection.reader_content(source).unwrap();
    assert_eq!(output.matches("readmd-bibliography-flow-entry").count(), 2);
    assert_eq!(output.matches("readmd-csl-layout-hanging").count(), 2);
    assert!(
        !output.contains("readmd-bibliography-hanging")
            && !output.contains("readmd-bibliography-has-first-field")
            && !output.contains("grid-template-columns:subgrid")
    );
    assert!(output.starts_with("😀 Source\n\n") && output.ends_with("\n\nAfter"));
    assert_eq!(projection.bibliography, original);
}

const STRUCTURE_STYLE: &str = r#"<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text"><info><title>ReadMD original recursive display structure test</title><id>https://readmd.test/display-structure</id></info><citation><layout><text variable="citation-number"/></layout></citation><bibliography><layout><group display="block"><text variable="title"/></group><text variable="volume" display="left-margin"/><group display="right-inline"><text variable="container-title" font-style="italic"/></group><text variable="abstract" display="indent"/></layout></bibliography></style>"#;

#[test]
fn academic_display_projection_exposes_full_width_heading_columns_and_annotation_without_markdown_claim(
) {
    let request = crate::academic::parse_request(json!({"format":"csl-json","data":r#"[{"id":"a","type":"book","title":"Heading","volume":"100","container-title":"Body","abstract":"Annotation"}]"#,"csl":STRUCTURE_STYLE,"clusters":[[{"key":"a"}]]})).unwrap();
    let response = crate::academic::citations(&request).unwrap();
    let bibliography = &response["bibliography"]["items"][0];
    let flow: Vec<RichLayoutNode> = serde_json::from_value(bibliography["layout"].clone()).unwrap();
    assert_eq!(columns(&flow), 1);
    assert_eq!(raw_text(&flow), "Heading100BodyAnnotation");
    assert_eq!(
        all(&flow)
            .iter()
            .filter(|node| matches!(node, RichLayoutNode::Block { indent: true, .. }))
            .count(),
        1
    );
    assert_eq!(bibliography["markdown_format_exact"], false);
    assert!(response["citations"][0]["layout"].is_array());
    assert!(response["citations"][0]["inline_layout"].is_array());
    assert!(response["citations"][0]["note_layout"].is_array());
}

#[test]
fn display_projection_preserves_exact_source_bytes_utf16_revision_and_nonce_slots() {
    let directory = tempfile::tempdir().unwrap();
    let references = r#"[{"id":"a","type":"book","title":"Heading","volume":"100","container-title":"Body","abstract":"Annotation"}]"#;
    std::fs::write(directory.path().join("references.json"), references).unwrap();
    std::fs::write(directory.path().join("display.csl"), STRUCTURE_STYLE).unwrap();
    let source = "---\r\nbibliography: references.json\r\ncsl: display.csl\r\n---\r\n😀 [@a] and [@a].\r\n\r\n<!--readmd:bibliography-->\r\n";
    let prepared = crate::publication::prepare(source, directory.path().to_str().unwrap()).unwrap();
    prepared.projection.verify_source(source).unwrap();
    assert_eq!(prepared.projection.citations.len(), 2);
    for (citation, (byte_from, _)) in prepared
        .projection
        .citations
        .iter()
        .zip(source.match_indices("[@a]"))
    {
        assert_eq!(
            (citation.source.span.byte_from, citation.source.span.byte_to),
            (byte_from, byte_from + 4)
        );
        assert_eq!(
            citation.source.span.from,
            source[..byte_from].encode_utf16().count()
        );
        assert_eq!(citation.source.span.source, "[@a]");
    }
    let bibliography = prepared.projection.bibliography.as_ref().unwrap();
    assert_eq!(columns(&bibliography.items[0].layout().unwrap()), 1);
    assert!(
        bibliography.items[0].first_field.is_empty(),
        "complex tree cannot be reduced to the legacy one-row split"
    );
    assert_eq!(
        bibliography.source_span.as_ref().unwrap().source.trim(),
        "<!--readmd:bibliography-->"
    );
    let instrumented = prepared.projection.instrument(source).unwrap();
    assert_eq!(instrumented.matches("data-slot=").count(), 3);
    assert!(instrumented.starts_with("---\r\nbibliography:"));
    assert!(prepared
        .projection
        .instrument(&source.replace("and", "then"))
        .is_err());
    assert_eq!(
        std::fs::read_to_string(directory.path().join("references.json")).unwrap(),
        references
    );
    assert_eq!(
        std::fs::read_to_string(directory.path().join("display.csl")).unwrap(),
        STRUCTURE_STYLE
    );
}

// Small independently authored controls for the restricted RESULT adapter.
// The explicit private official-fixture command lives in examples/.
#[derive(Clone, Debug, PartialEq, Eq)]
enum OracleToken {
    Text(String, RichFormat),
    Start(RichDisplay),
    End(RichDisplay),
}
fn oracle_text(output: &mut Vec<OracleToken>, text: &str, format: &RichFormat) {
    if text.is_empty() {
        return;
    }
    if let Some(OracleToken::Text(previous, previous_format)) = output.last_mut() {
        if previous_format == format {
            previous.push_str(text);
            return;
        }
    }
    output.push(OracleToken::Text(text.into(), format.clone()));
}
fn expected_result_tokens(result: &str) -> Vec<Vec<OracleToken>> {
    use quick_xml::{events::Event, reader::Reader};
    let mut reader = Reader::from_str(result);
    let mut stack: Vec<(String, RichFormat, Option<RichDisplay>)> = Vec::new();
    let mut entries: Vec<Vec<OracleToken>> = Vec::new();
    let mut in_entry = false;
    loop {
        match reader
            .read_event()
            .expect("independent RESULT must be valid restricted HTML/XML")
        {
            Event::Start(start) => {
                let name = start.local_name().as_ref().to_string();
                let mut format = stack
                    .last()
                    .map(|frame| frame.1.clone())
                    .unwrap_or_default();
                let mut class = None;
                for attribute in start.attributes() {
                    let attribute = attribute.unwrap();
                    assert_eq!(
                        attribute.key.as_ref(),
                        "class",
                        "unreviewed RESULT attribute"
                    );
                    class = Some(
                        attribute
                            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .unwrap()
                            .into_owned(),
                    );
                }
                let display = match (name.as_str(), class.as_deref()) {
                    ("div", Some("csl-bib-body")) => None,
                    ("div", Some("csl-entry")) => {
                        assert!(!in_entry);
                        in_entry = true;
                        entries.push(Vec::new());
                        None
                    }
                    ("div", Some("csl-block")) => Some(RichDisplay::Block),
                    ("div", Some("csl-indent")) => Some(RichDisplay::Indent),
                    ("div", Some("csl-left-margin")) => Some(RichDisplay::LeftMargin),
                    ("div", Some("csl-right-inline")) => Some(RichDisplay::RightInline),
                    ("i" | "em", None) => {
                        format.font_style = RichFontStyle::Italic;
                        None
                    }
                    _ => panic!("unreviewed independent RESULT element {name}/{class:?}"),
                };
                if let Some(display) = display {
                    entries
                        .last_mut()
                        .unwrap()
                        .push(OracleToken::Start(display));
                }
                // Retain the entry marker only in this parser's frame name.
                let frame_name = if class.as_deref() == Some("csl-entry") {
                    "entry".into()
                } else {
                    name
                };
                stack.push((frame_name, format, display));
            }
            Event::End(_) => {
                let (name, _, display) = stack.pop().expect("balanced independent RESULT");
                if let Some(display) = display {
                    entries.last_mut().unwrap().push(OracleToken::End(display));
                }
                if name == "entry" {
                    in_entry = false;
                }
            }
            Event::Text(text) => {
                let value = text.as_ref();
                // Only HTML source indentation between structural divs is
                // irrelevant. Inline punctuation and all other whitespace stay exact.
                if value.trim().is_empty()
                    && value.contains('\n')
                    && stack.last().is_some_and(|frame| {
                        frame.0 == "entry" || (frame.0 == "div" && frame.2.is_none())
                    })
                {
                    continue;
                }
                if in_entry {
                    oracle_text(entries.last_mut().unwrap(), value, &stack.last().unwrap().1);
                } else {
                    assert!(value.trim().is_empty());
                }
            }
            Event::GeneralRef(reference) => {
                let value = reference.as_ref();
                let c = match value {
                    "amp" => '&',
                    "lt" => '<',
                    "gt" => '>',
                    "quot" => '"',
                    "apos" => '\'',
                    _ if value.starts_with("#x") => {
                        char::from_u32(u32::from_str_radix(&value[2..], 16).unwrap()).unwrap()
                    }
                    _ if value.starts_with('#') => {
                        char::from_u32(value[1..].parse().unwrap()).unwrap()
                    }
                    _ => panic!("unreviewed independent RESULT entity"),
                };
                assert!(in_entry);
                oracle_text(
                    entries.last_mut().unwrap(),
                    &c.to_string(),
                    &stack.last().unwrap().1,
                );
            }
            Event::Eof => break,
            _ => panic!("unreviewed independent RESULT event"),
        }
    }
    assert!(stack.is_empty() && !in_entry);
    entries
}
fn actual_result_tokens(flow: &[RichLayoutNode]) -> Vec<OracleToken> {
    fn visit(flow: &[RichLayoutNode], output: &mut Vec<OracleToken>) {
        for node in flow {
            match node {
                RichLayoutNode::Inline { children } => {
                    for child in children {
                        let RichNode::Text { text, format } = child else {
                            panic!("display projection leaked an inline container")
                        };
                        oracle_text(output, text, format);
                    }
                }
                RichLayoutNode::Group { children, .. } => visit(children, output),
                RichLayoutNode::Link { .. } => {
                    panic!("official display fixture hyperlink was not reviewed")
                }
                RichLayoutNode::Block { children, indent } => {
                    let display = if *indent {
                        RichDisplay::Indent
                    } else {
                        RichDisplay::Block
                    };
                    output.push(OracleToken::Start(display));
                    visit(children, output);
                    output.push(OracleToken::End(display));
                }
                RichLayoutNode::Columns { left, right } => {
                    output.push(OracleToken::Start(RichDisplay::LeftMargin));
                    visit(left, output);
                    output.push(OracleToken::End(RichDisplay::LeftMargin));
                    if right
                        .iter()
                        .any(|node| matches!(node, RichLayoutNode::Block { indent: false, .. }))
                    {
                        for node in right {
                            if let RichLayoutNode::Block {
                                children,
                                indent: false,
                            } = node
                            {
                                output.push(OracleToken::Start(RichDisplay::RightInline));
                                visit(children, output);
                                output.push(OracleToken::End(RichDisplay::RightInline));
                            } else {
                                visit(std::slice::from_ref(node), output);
                            }
                        }
                    } else {
                        output.push(OracleToken::Start(RichDisplay::RightInline));
                        visit(right, output);
                        output.push(OracleToken::End(RichDisplay::RightInline));
                    }
                }
            }
        }
    }
    let mut output = Vec::new();
    visit(flow, &mut output);
    output
}

#[test]
fn display_independent_result_adapter_preserves_style_whitespace_and_structural_boundaries() {
    let expected = expected_result_tokens("<div class=\"csl-bib-body\">\n<div class=\"csl-entry\">\n<div class=\"csl-block\">Header</div>\n<div class=\"csl-left-margin\">[9]</div><div class=\"csl-right-inline\"> <i>Body</i> tail</div><div class=\"csl-indent\">Note &amp; detail</div>\n</div>\n</div>");
    let italic = RichFormat {
        font_style: RichFontStyle::Italic,
        ..Default::default()
    };
    let nodes = vec![
        block(RichDisplay::Block, "Header"),
        block(RichDisplay::LeftMargin, "[9]"),
        group(
            Some(RichDisplay::RightInline),
            8,
            vec![
                RichNode::text(" "),
                RichNode::Text {
                    text: "Body".into(),
                    format: italic,
                },
                RichNode::text(" tail"),
            ],
        ),
        block(RichDisplay::Indent, "Note & detail"),
    ];
    assert_eq!(
        expected,
        vec![actual_result_tokens(&layout(&nodes).unwrap())]
    );
    let mut wrong = nodes.clone();
    if let RichNode::Group { display, .. } = &mut wrong[3] {
        *display = Some(RichDisplay::Block);
    }
    assert_ne!(
        expected,
        vec![actual_result_tokens(&layout(&wrong).unwrap())]
    );
    let whitespace = expected_result_tokens(
        "<div class=\"csl-bib-body\"><div class=\"csl-entry\"><i> \n </i></div></div>",
    );
    assert!(matches!(&whitespace[0][0], OracleToken::Text(text, _) if text == " \n "));
    assert!(std::panic::catch_unwind(|| expected_result_tokens(
        "<div class=\"unreviewed\">x</div>"
    ))
    .is_err());
}
