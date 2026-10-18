use super::*;
use crate::{docx_writer, md_ast};

const WORKFLOW: &str = include_str!("fixtures/publication-sections-workflow.md");
fn s(value: Value) -> Section {
    serde_json::from_value(value).unwrap()
}
fn marker(value: Value) -> String {
    comment(&s(value)).unwrap()
}
fn style() -> Value {
    crate::export_styles::sanitize(&json!({"cover":{"enabled":false},"toc":{"enabled":false}}))
}
fn package(source: &str, style: &Value) -> docx_writer::DocxPackage {
    let package = docx_writer::build(&md_ast::parse(source), style, "sections.md", &|_| None);
    assert!(!package.parts.is_empty(), "{:?}", package.warns);
    for (name, bytes) in &package.parts {
        if name.ends_with(".xml") || name.ends_with(".rels") {
            let xml = std::str::from_utf8(bytes).unwrap();
            let mut reader = quick_xml::Reader::from_str(xml);
            loop {
                match reader
                    .read_event()
                    .unwrap_or_else(|e| panic!("{name}: {e}"))
                {
                    quick_xml::events::Event::Start(e) | quick_xml::events::Event::Empty(e) => {
                        for a in e.attributes().with_checks(true) {
                            a.unwrap_or_else(|e| panic!("{name}: {e}"));
                        }
                    }
                    quick_xml::events::Event::Eof => break,
                    _ => (),
                }
            }
        }
    }
    package
}
fn part<'a>(p: &'a docx_writer::DocxPackage, name: &str) -> &'a str {
    std::str::from_utf8(
        &p.parts
            .iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("missing {name}"))
            .1,
    )
    .unwrap()
}
fn sections(p: &docx_writer::DocxPackage) -> Vec<&str> {
    part(p, "word/document.xml")
        .split("<w:sectPr>")
        .skip(1)
        .map(|x| x.split_once("</w:sectPr>").unwrap().0)
        .collect()
}

#[test]
fn section_comments_are_strict_sparse_bounded_and_injection_safe() {
    let section = s(json!({"id":"body","header":{"linkPrevious":false,"text":"A --> B < C & D"}}));
    let raw = comment(&section).unwrap();
    assert!(!raw.contains("null"));
    assert_eq!(raw.matches("-->").count(), 1);
    assert_eq!(read_comment(&raw).unwrap().unwrap(), section);
    for raw in [
        "<!-- readmd-section:v2 {\"id\":\"a\"} -->",
        "<!-- readmd-section:v1 {\"id\":\"a\",\"id\":\"b\"} -->",
        "<!-- readmd-section:v1 {\"id\":\"a\",\"unknown\":1} -->",
        "<!-- readmd-section:v1 {\"id\":\"a\",\"page\":{\"columns\":1.5}} -->",
        "<!-- readmd-section:v1 {\"id\":\"a\",\"break\":\"column\"} -->",
    ] {
        assert_eq!(read_comment(raw).unwrap_err(), INVALID, "{raw}");
    }
    for id in ["", "_document", "a\"x", "中文", "a b"] {
        assert!(validate_section(&s(json!({"id":id}))).is_err());
    }
    let too_long = format!(
        "{PREFIX}v1 {{\"id\":\"a\",\"unknown\":\"{}\"}} -->",
        "x".repeat(MAX_DECLARATION_BYTES)
    );
    assert_eq!(read_comment(&too_long).unwrap_err(), BUDGET);
}

#[test]
fn section_source_ranges_follow_bom_crlf_chinese_and_astral_utf16() {
    let directive = marker(json!({"id":"wide","header":{"linkPrevious":false,"text":"中文😀"}}));
    let source = format!("\u{feff}前言😀\r\n\r\n{directive}\r\n\r\n正文\r\n");
    let declarations = analyze(&source).unwrap();
    assert_eq!(declarations.len(), 1);
    let d = &declarations[0];
    assert_eq!(d.section.id, "wide");
    assert_eq!(d.from, source[..d.bytes.start].encode_utf16().count());
    assert_eq!(d.to, source[..d.bytes.end].encode_utf16().count());
    assert!(source[d.bytes.clone()].starts_with(PREFIX));
    let projected = strip_for_view(&source).unwrap();
    assert!(!projected.contains(PREFIX));
    assert!(!projected.contains("linkPrevious"));
    assert!(projected.starts_with("\u{feff}前言😀\r\n"));
    assert!(projected.ends_with("正文\r\n"));
    assert_eq!(
        projected.matches('\n').count(),
        source.matches('\n').count()
    );
}

#[test]
fn section_source_respects_literal_code_and_rejects_nested_or_inline_boundaries() {
    let d = marker(json!({"id":"body"}));
    let literal = format!("```html\n{d}\n```\n\n    {d}\n\n`{d}`\n");
    assert!(analyze(&literal).unwrap().is_empty());
    assert_eq!(strip_for_view(&literal).unwrap(), literal);
    for bad in [
        format!("> {d}\n"),
        format!("- {d}\n"),
        format!("Text {d} tail\n"),
    ] {
        assert_eq!(validate_source(&bad).unwrap_err(), INVALID, "{bad}");
        assert_eq!(
            validate_document(&md_ast::parse(&bad)).unwrap_err(),
            INVALID
        );
    }
}

#[test]
fn section_ids_survive_body_edits_and_reordering_but_duplicates_are_rejected() {
    let first = marker(json!({"id":"first"}));
    let second = marker(json!({"id":"second"}));
    for body in ["Hello", "A substantially edited 中文😀 body"] {
        let source = format!("{first}\n\n{body}\n\n{second}\n\nEnd\n");
        assert_eq!(
            analyze(&source)
                .unwrap()
                .iter()
                .map(|d| d.section.id.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
    }
    assert_eq!(
        analyze(&format!("{second}\n\nB\n\n{first}\n\nA\n"))
            .unwrap()
            .iter()
            .map(|d| d.section.id.as_str())
            .collect::<Vec<_>>(),
        ["second", "first"]
    );
    let duplicate = format!("{first}\n\nA\n\n{first}\n\nB\n");
    assert_eq!(validate_source(&duplicate).unwrap_err(), INVALID);
    assert_eq!(
        resolve(&md_ast::parse(&duplicate), &style()).unwrap_err(),
        INVALID
    );
}

#[test]
fn section_limits_cover_source_and_direct_typed_documents() {
    let mut source = String::new();
    let mut blocks = Vec::new();
    for i in 0..=MAX_SECTIONS {
        let v = s(json!({"id":format!("s{i}")}));
        source.push_str(&format!("{}\n\n", comment(&v).unwrap()));
        blocks.push(Block::SectionStart(v));
    }
    assert_eq!(validate_source(&source).unwrap_err(), BUDGET);
    assert_eq!(
        validate_document(&Document {
            blocks,
            ..Default::default()
        })
        .unwrap_err(),
        BUDGET
    );
}

#[test]
fn section_resolver_retains_top_level_ranges_and_containers_without_flattening() {
    let doc = md_ast::parse(WORKFLOW);
    let all = resolve(&doc, &style()).unwrap();
    assert_eq!(
        all.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        ["front", "main", "wide-table", "appendix"]
    );
    assert_eq!(all[0].page_start, Some(1));
    assert_eq!(all[1].page_start, Some(1));
    assert_eq!(all[2].page_start, None);
    assert_eq!(all[3].page_start, Some(1));
    assert_eq!(all[0].style["footer"]["numberFormat"], "lowerRoman");
    assert_eq!(all[1].style["page"]["columns"], 2);
    assert_eq!(all[2].style["page"]["orientation"], "landscape");
    assert_eq!(all[3].style["footer"]["numberFormat"], "upperLetter");
    assert!(doc.blocks[all[1].block_range.clone()]
        .iter()
        .any(|b| matches!(b, Block::Quote(_))));
    assert!(doc.blocks[all[1].block_range.clone()]
        .iter()
        .any(|b| matches!(b, Block::List { .. })));
    assert!(doc.blocks[all[2].block_range.clone()]
        .iter()
        .any(|b| matches!(b, Block::Table { .. })));
}

#[test]
fn section_first_patch_inherits_global_then_later_start_omission_continues() {
    let d = md_ast::parse(&format!(
        "{}\n\nA\n\n{}\n\nB\n",
        marker(json!({"id":"a"})),
        marker(json!({"id":"b","numbering":{"format":"upperRoman"}}))
    ));
    let global = crate::export_styles::sanitize(
        &json!({"page":{"size":"A5","columns":2},"footer":{"start":7}}),
    );
    let ss = resolve(&d, &global).unwrap();
    assert_eq!(ss.len(), 2);
    assert_eq!(ss[0].style["page"]["size"], "A5");
    assert_eq!(ss[0].style["page"]["columns"], 2);
    assert_eq!(ss[0].page_start, Some(7));
    assert_eq!(ss[1].page_start, None);
    assert!(!ss[0].header_link_previous);
    assert!(ss[1].header_link_previous);
    assert!(ss[1].footer_link_previous);
}

#[test]
fn section_implicit_prefix_and_explicit_empty_sections_are_preserved() {
    let a = marker(json!({"id":"a"}));
    let b = marker(json!({"id":"b"}));
    let doc = md_ast::parse(&format!("Prefix\n\n{a}\n\n{b}\n"));
    let ss = resolve(&doc, &style()).unwrap();
    assert_eq!(
        ss.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        ["_document", "a", "b"]
    );
    assert!(ss[1].block_range.is_empty());
    assert!(ss[2].block_range.is_empty());
    assert!(!ss[0].explicit);
    let first = resolve(&md_ast::parse(&format!("{a}\n\nBody\n")), &style()).unwrap();
    assert_eq!(first.len(), 1);
}

#[test]
fn section_continuous_rejects_physical_page_changes_but_allows_columns() {
    for patch in [
        json!({"size":"A5"}),
        json!({"orientation":"landscape"}),
        json!({"marginLeft":22}),
        json!({"gutter":3}),
    ] {
        let raw = format!(
            "A\n\n{}\n\nB\n",
            marker(json!({"id":"b","break":"continuous","page":patch}))
        );
        assert_eq!(
            resolve(&md_ast::parse(&raw), &style()).unwrap_err(),
            CONTINUOUS_PAGE
        );
    }
    let raw = format!(
        "A\n\n{}\n\nB\n",
        marker(json!({"id":"b","break":"continuous","page":{"columns":2}}))
    );
    assert_eq!(
        resolve(&md_ast::parse(&raw), &style()).unwrap()[1].break_kind,
        SectionBreak::Continuous
    );
}

#[test]
fn section_invalid_geometry_is_rejected_without_clamping_or_output_parts() {
    for p in [
        json!({"columns":0}),
        json!({"columns":4}),
        json!({"columnGap":0}),
        json!({"width":90}),
        json!({"size":"Custom","width":90}),
        json!({"size":"custom"}),
    ] {
        assert_eq!(
            validate_section(&s(json!({"id":"a","page":p}))).unwrap_err(),
            GEOMETRY
        );
    }
    let raw = marker(
        json!({"id":"a","page":{"size":"Custom","width":80,"height":80,"marginLeft":35,"marginRight":35}}),
    );
    let doc = md_ast::parse(&format!("{raw}\n\nBody\n"));
    assert_eq!(resolve(&doc, &style()).unwrap_err(), GEOMETRY);
    let p = docx_writer::build(&doc, &style(), "bad.md", &|_| None);
    assert!(p.parts.is_empty());
    assert!(p.warns.contains(&GEOMETRY.into()));
}

#[test]
fn section_running_content_requires_explicit_independence_and_mode_enablement() {
    for v in [
        json!({"id":"a","header":{"text":"New"}}),
        json!({"id":"a","header":{"linkPrevious":true,"text":"New"}}),
        json!({"id":"a","footer":{"linkPrevious":false,"text":"Bad\nline"}}),
    ] {
        assert_eq!(validate_section(&s(v)).unwrap_err(), INVALID);
    }
    let raw = marker(json!({"id":"a","header":{"linkPrevious":false,"evenText":"Even"}}));
    assert_eq!(
        resolve(&md_ast::parse(&raw), &style()).unwrap_err(),
        INVALID
    );
}

#[test]
fn section_metadata_is_not_an_inline_copy_search_or_heading_payload() {
    let raw = marker(
        json!({"id":"SecretIdentity","header":{"linkPrevious":false,"text":"Hidden running configuration"}}),
    );
    let doc = md_ast::parse(&format!("{raw}\n\n# Real heading\n\nVisible body\n"));
    let mut copied = String::new();
    md_ast::walk_inlines(&doc.blocks, &mut |i| {
        if let md_ast::Inline::Text(t) = i {
            copied.push_str(t)
        }
    });
    assert!(copied.contains("Visible body"));
    assert!(!copied.contains("SecretIdentity"));
    assert!(!copied.contains("Hidden running"));
    let projected = md_ast::to_render_json(&doc.blocks);
    assert_eq!(projected[0]["type"], "section");
    assert_eq!(projected[0]["section"]["id"], "SecretIdentity");
    assert!(projected[0].get("text").is_none());
}

#[test]
fn docx_sections_emit_native_workflow_properties_at_the_correct_section_end() {
    let p = package(WORKFLOW, &style());
    let xml = part(&p, "word/document.xml");
    let ss = sections(&p);
    assert_eq!(ss.len(), 4);
    assert!(ss[0].contains("w:fmt=\"lowerRoman\" w:start=\"1\""));
    assert!(ss[0].contains("w:num=\"1\""));
    assert!(ss[1].contains("w:fmt=\"decimal\" w:start=\"1\""));
    assert!(ss[1].contains("w:num=\"2\""));
    assert!(ss[2].contains("w:orient=\"landscape\""));
    assert!(ss[2].contains("w:w=\"16838\" w:h=\"11906\""));
    assert!(!ss[2].contains("w:start="));
    assert!(ss[3].contains("w:type w:val=\"oddPage\""));
    assert!(ss[3].contains("w:fmt=\"upperLetter\" w:start=\"1\""));
    assert!(xml.find("<w:sectPr>").unwrap() > xml.find("Roman-numbered").unwrap());
    assert!(xml.rfind("<w:sectPr>").unwrap() > xml.find("Appendix content").unwrap());
    assert!(xml.ends_with("</w:sectPr></w:body></w:document>"));
    assert_eq!(
        xml.matches("<w:pPr><w:spacing w:before=\"0\" w:after=\"0\" w:line=\"20\"")
            .count(),
        3
    );
}

#[test]
fn docx_sections_all_four_start_types_are_native_even_when_layout_is_identical() {
    let source = [
        ("a", "nextPage"),
        ("b", "continuous"),
        ("c", "oddPage"),
        ("d", "evenPage"),
    ]
    .into_iter()
    .map(|(id, kind)| format!("{}\n\n{id} body\n\n", marker(json!({"id":id,"break":kind}))))
    .collect::<String>();
    let p = package(&source, &style());
    let ss = sections(&p);
    assert_eq!(ss.len(), 4);
    for (section, kind) in ss
        .iter()
        .zip(["nextPage", "continuous", "oddPage", "evenPage"])
    {
        assert!(section.contains(&format!("<w:type w:val=\"{kind}\"/>")));
    }
}

#[test]
fn docx_sections_native_header_links_and_blank_independent_parts_do_not_leak() {
    let source = format!(
        "{}\n\nA\n\n{}\n\nB\n\n{}\n\nC\n",
        marker(json!({"id":"a","header":{"linkPrevious":false,"text":"UNIQUE FRONT"}})),
        marker(json!({"id":"b","header":{"linkPrevious":true}})),
        marker(json!({"id":"c","header":{"linkPrevious":false}}))
    );
    let p = package(&source, &style());
    let ss = sections(&p);
    assert!(ss[0].contains("<w:headerReference"));
    assert!(!ss[1].contains("<w:headerReference"));
    assert!(ss[2].contains("<w:headerReference"));
    assert!(part(&p, "word/header-section-0-default.xml").contains("UNIQUE FRONT"));
    assert!(!part(&p, "word/header-section-2-default.xml").contains("UNIQUE FRONT"));
    assert!(!p
        .parts
        .iter()
        .any(|(n, _)| n == "word/header-section-1-default.xml"));
    let rels = part(&p, "word/_rels/document.xml.rels");
    assert!(rels.contains("Target=\"header-section-2-default.xml\""));
    assert!(part(&p, "[Content_Types].xml").contains("/word/header-section-2-default.xml"));
    assert!(!part(&p, "word/document.xml").contains("UNIQUE FRONT"));
}

#[test]
fn docx_sections_real_footer_fields_and_number_restarts_remain_separate() {
    let source = format!(
        "{}\n\nA\n\n{}\n\nB\n\n{}\n\nC\n",
        marker(
            json!({"id":"a","numbering":{"format":"upperRoman","start":7},"footer":{"linkPrevious":false}})
        ),
        marker(json!({"id":"b"})),
        marker(json!({"id":"c","numbering":{"format":"decimal","start":4}}))
    );
    let p = package(&source, &style());
    let ss = sections(&p);
    assert!(ss[0].contains("w:start=\"7\""));
    assert!(!ss[1].contains("w:start="));
    assert!(ss[2].contains("w:start=\"4\""));
    let footer = part(&p, "word/footer-section-0-default.xml");
    assert!(footer.contains(" PAGE "));
    assert!(footer.contains("VII"));
    assert!(!ss[1].contains("<w:footerReference"));
    assert!(part(&p, "word/settings.xml").contains("<w:updateFields w:val=\"true\"/>"));
}

#[test]
fn docx_sections_empty_and_table_ending_sections_keep_native_carrier_paragraphs() {
    let source = format!(
        "{}\n\n{}\n\n| A | B |\n|---|---|\n| C | D |\n\n{}\n",
        marker(json!({"id":"empty"})),
        marker(json!({"id":"table"})),
        marker(json!({"id":"lastEmpty","break":"evenPage"}))
    );
    let p = package(&source, &style());
    let xml = part(&p, "word/document.xml");
    assert_eq!(sections(&p).len(), 3);
    assert!(xml.contains("</w:tbl><w:p><w:pPr><w:spacing"));
    for id in ["empty", "table", "lastEmpty"] {
        assert!(xml.contains(&docx_writer::section_bookmark_name(id)));
    }
    assert!(sections(&p)[2].contains("w:type w:val=\"evenPage\""));
}

#[test]
fn docx_sections_bookmark_identity_survives_body_changes_and_heading_insertions() {
    let d = marker(json!({"id":"stable-identity"}));
    let a = package(&format!("{d}\n\n# Original\n\nA\n"), &style());
    let b = package(
        &format!("{d}\n\n# Added\n\n## Edited\n\n中文😀 B\n"),
        &style(),
    );
    let stable = docx_writer::section_bookmark_name("stable-identity");
    assert_ne!(stable, docx_writer::bookmark_name("section:stable-identity"));
    assert!(part(&a, "word/document.xml").contains(&stable));
    assert!(part(&b, "word/document.xml").contains(&stable));
    assert!(stable.len() <= 40);
}

#[test]
fn docx_sections_custom_paper_margins_columns_and_first_even_parts_are_explicit() {
    let raw = marker(
        json!({"id":"custom","page":{"size":"Custom","width":200,"height":300,"marginTop":25,"marginBottom":26,"marginLeft":21,"marginRight":22,"columns":3,"columnGap":6,"gutter":3,"differentFirstPage":true},"header":{"linkPrevious":false,"text":"Odd","evenText":"Even","firstText":"First"}}),
    );
    let st = crate::export_styles::sanitize(
        &json!({"page":{"differentOddEven":true},"cover":{"enabled":false}}),
    );
    let p = package(&format!("{raw}\n\nBody\n"), &st);
    let ss = sections(&p);
    assert!(ss[0].contains("w:w=\"11339\" w:h=\"17008\""));
    assert!(ss[0].contains("w:num=\"3\" w:space=\"340\""));
    assert!(ss[0].contains("<w:titlePg/>"));
    for (mode, text) in [("default", "Odd"), ("even", "Even"), ("first", "First")] {
        assert!(part(&p, &format!("word/header-section-0-{mode}.xml")).contains(text));
    }
    assert!(part(&p, "word/settings.xml").contains("<w:evenAndOddHeaders/>"));
}

#[test]
fn docx_no_section_declaration_retains_the_legacy_single_and_cover_toc_layout() {
    let plain = package("# Heading\n\nBody\n", &style());
    let ordinary = package("<!-- ordinary comment -->\n\n# Heading\n\nBody\n", &style());
    assert_eq!(
        part(&plain, "word/document.xml"),
        part(&ordinary, "word/document.xml")
    );
    assert_eq!(sections(&plain).len(), 1);
    assert!(!part(&plain, "word/document.xml")
        .contains(&docx_writer::section_bookmark_name("_document")));
    let st = crate::export_styles::sanitize(
        &json!({"page":{"columns":2},"cover":{"enabled":true},"toc":{"enabled":true}}),
    );
    let legacy = package("# Body\n\nText\n", &st);
    let ss = sections(&legacy);
    assert_eq!(ss.len(), 2);
    assert!(ss[0].contains("w:num=\"1\""));
    assert!(ss[1].contains("w:num=\"2\""));
}

#[test]
fn docx_sections_recompute_real_table_grids_for_each_current_column_width() {
    let table = "| A | B |\n|---|---|\n| C | D |\n";
    let raw = format!(
        "{}\n\n{table}\n\n{}\n\n{table}",
        marker(json!({"id":"narrow","page":{"columns":2}})),
        marker(json!({"id":"wide","page":{"orientation":"landscape","columns":1}}))
    );
    let p = package(&raw, &style());
    let grids = part(&p, "word/document.xml")
        .split("<w:tblGrid>")
        .skip(1)
        .map(|part| {
            part.split_once("</w:tblGrid>")
                .unwrap()
                .0
                .split("<w:gridCol w:w=\"")
                .skip(1)
                .map(|s| s.split('"').next().unwrap().parse::<u32>().unwrap())
                .sum::<u32>()
        })
        .collect::<Vec<_>>();
    assert_eq!(grids.len(), 2);
    assert!(grids[1] > grids[0] * 3, "{grids:?}");
    assert!(part(&p, "word/document.xml").contains("<w:tbl>"));
}

#[test]
fn docx_sections_keep_real_notes_math_and_relationships_in_the_workflow() {
    let p = package(WORKFLOW, &style());
    assert!(part(&p, "word/document.xml").contains("<m:oMath"));
    assert!(part(&p, "word/document.xml").contains("<w:footnoteReference"));
    assert!(part(&p, "word/footnotes.xml").contains("The note is a real Word note"));
    assert!(part(&p, "word/_rels/document.xml.rels").contains("https://example.invalid/sections"));
}
