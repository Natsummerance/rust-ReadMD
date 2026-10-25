//! Real w:pPr bytes for decorated ordinary paragraphs (w:pBdr / w:shd).
use super::*;
use serde_json::json;

fn document_xml(source: &str) -> String {
    let doc = crate::md_ast::parse(source);
    let style = crate::export_styles::sanitize(&json!({}));
    let pkg = crate::docx_writer::build(&doc, &style, "test", &|_| None);
    assert!(pkg.warns.is_empty(), "{:?}", pkg.warns);
    String::from_utf8(
        pkg.parts.iter().find(|(part, _)| part == "word/document.xml").unwrap().1.clone(),
    ).unwrap()
}

fn paragraph_of<'a>(xml: &'a str, marker: &str) -> &'a str {
    xml.split("<w:p>").find(|p| p.contains(marker)
        // Attributes never survive as visible text; the marker's own run only.
        && !p.contains(crate::publication_blocks::PREFIX)).unwrap()
}

const LEFT: &str = r#"{"paragraph":{"border":{"left":{"color":"4472C4","widthPt":2.5,"spacePt":4}}}}"#;
const FILL: &str = r#"{"paragraph":{"shading":{"fill":"DCE6F1"}}}"#;

#[test]
fn decoration_single_border_edge_writes_native_pbdr() {
    let xml = document_xml(&format!("<!-- readmd:layout v1 {LEFT} -->\n\nLOCAL paragraph.\n"));
    let paragraph = paragraph_of(&xml, "LOCAL");
    // The XML serializer emits attributes in sorted order; only the element
    // sequence (pBdr position, edge set) is part of the contract.
    assert!(paragraph.contains(
        "<w:pBdr><w:left w:color=\"4472C4\" w:space=\"4\" w:sz=\"20\" w:val=\"single\"/></w:pBdr>"),
        "{paragraph}");
    // No shading was declared: the paragraph itself carries no w:shd.
    assert!(!paragraph.contains("w:shd"));
    // Only the decorated paragraph is affected.
    let both = document_xml(&format!(
        "<!-- readmd:layout v1 {LEFT} -->\n\nLOCAL paragraph.\n\nUNCHANGED\n"));
    assert!(!both.split("<w:p>").find(|p| p.contains("UNCHANGED")).unwrap().contains("w:pBdr"));
}

#[test]
fn decoration_rounds_eighths_half_up_in_both_directions() {
    for (width, sz) in [(2.5, "20"), (2.6, "21")] {
        let xml = document_xml(&format!(
            "<!-- readmd:layout v1 {{\"paragraph\":{{\"border\":{{\"left\":{{\"color\":\"4472C4\",\"widthPt\":{width},\"spacePt\":4}}}}}}}} -->\n\nLOCAL\n"));
        assert!(paragraph_of(&xml, "LOCAL").contains(
            &format!("<w:left w:color=\"4472C4\" w:space=\"4\" w:sz=\"{sz}\" w:val=\"single\"/>")),
            "{width} → {sz}");
    }
}

#[test]
fn decoration_all_four_edges_keep_ct_pbdr_sequence() {
    let edges = ["top", "left", "bottom", "right"].iter().map(|side| format!(
        "\"{side}\":{{\"color\":\"4472C4\",\"widthPt\":1.0,\"spacePt\":0}}")).collect::<Vec<_>>().join(",");
    let xml = document_xml(&format!(
        "<!-- readmd:layout v1 {{\"paragraph\":{{\"border\":{{{edges}}}}}}} -->\n\nLOCAL\n"));
    let paragraph = paragraph_of(&xml, "LOCAL");
    assert!(paragraph.contains("<w:pBdr><w:top w:color=\"4472C4\" w:space=\"0\" w:sz=\"8\" w:val=\"single\"/>\
<w:left w:color=\"4472C4\" w:space=\"0\" w:sz=\"8\" w:val=\"single\"/>\
<w:bottom w:color=\"4472C4\" w:space=\"0\" w:sz=\"8\" w:val=\"single\"/>\
<w:right w:color=\"4472C4\" w:space=\"0\" w:sz=\"8\" w:val=\"single\"/></w:pBdr>"), "{paragraph}");
}

#[test]
fn decoration_fill_and_border_coexist_with_pbdr_before_shd() {
    let both = format!("{{\"paragraph\":{{\"shading\":{{\"fill\":\"DCE6F1\"}},\"border\":{{\"left\":{{\"color\":\"4472C4\",\"widthPt\":2.5,\"spacePt\":4}}}}}}}}");
    let xml = document_xml(&format!("<!-- readmd:layout v1 {both} -->\n\nLOCAL\n"));
    let paragraph = paragraph_of(&xml, "LOCAL");
    assert!(paragraph.contains("<w:shd w:color=\"auto\" w:fill=\"DCE6F1\" w:val=\"clear\"/>"));
    let pbdr = paragraph.find("<w:pBdr>").unwrap();
    let shd = paragraph.find("<w:shd ").unwrap();
    assert!(pbdr < shd, "CT_PPrBase requires pBdr before shd");
}

#[test]
fn decoration_coexists_with_align_line_spacing_and_size_in_p_order() {
    let combined = json!({"text":{"size":16},
        "paragraph":{"shading":{"fill":"DCE6F1"},"border":{"left":{"color":"4472C4","widthPt":2.5,"spacePt":4}},
            "align":"center","lineSpacing":{"mode":"exact","pt":18}}});
    let xml = document_xml(&format!(
        "<!-- readmd:layout v1 {combined} -->\n\nLOCAL **strong**\n"));
    let paragraph = paragraph_of(&xml, "LOCAL");
    assert!(paragraph.contains("<w:jc w:val=\"center\"/>"));
    assert!(paragraph.contains("w:line=\"360\" w:lineRule=\"exact\""));
    assert!(paragraph.contains("w:sz w:val=\"32\""));
    let order = ["<w:pBdr>", "<w:shd ", "<w:spacing ", "<w:ind ", "<w:jc "];
    let positions: Vec<usize> = order.iter().map(|tag| paragraph.find(tag)
        .unwrap_or_else(|| panic!("missing {tag} in {paragraph}"))).collect();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{paragraph}");
}

#[test]
fn undecorated_documents_keep_their_exact_paragraph_bytes() {
    let plain = "PLAIN body paragraph.\n\n> A quote.\n\n```js\ncode\n```\n";
    let xml = document_xml(plain);
    assert!(!xml.contains("w:pBdr"), "no paragraph border may appear from nowhere");
    assert!(!xml.contains("<w:shd "), "no paragraph shading may appear from nowhere");
    // The built-in quote/code decoration keeps living in styles.xml, not in
    // these document paragraphs.
    assert!(xml.contains("w:val=\"Quote\""));
    assert!(xml.contains("w:val=\"Code\""));
    // Source-level determinism: the same source exports identical pPr bytes.
    assert_eq!(document_xml(plain), document_xml(plain));
    let decorated = format!("<!-- readmd:layout v1 {FILL} -->\n\nLOCAL\n");
    assert_eq!(document_xml(&decorated), document_xml(&decorated));
}

#[test]
fn decorated_export_reproduces_identical_ppr_from_the_same_source() {
    let source = format!(
        "<!-- readmd:layout v1 {FILL} -->\n\nFirst decorated.\n\n<!-- readmd:layout v1 {LEFT} -->\n\nSecond decorated.\n");
    let first = document_xml(&source);
    let second = document_xml(&source);
    assert_eq!(first, second);
    assert!(first.contains("w:fill=\"DCE6F1\""));
    assert!(first.contains("w:color=\"4472C4\""));
}

#[test]
fn declaration_with_rich_text_tab_footnote_and_non_bmp_stays_intact() {
    let source = format!("<!-- readmd:layout v1 {FILL} -->\n\nBody\twith 🚀 and [^n] note.\n\n[^n]: Note.\n");
    let xml = document_xml(&source);
    let paragraph = paragraph_of(&xml, "with");
    assert!(paragraph.contains("<w:shd w:color=\"auto\" w:fill=\"DCE6F1\" w:val=\"clear\"/>"));
    assert!(paragraph.contains("🚀"));
    // The existing '\t' → w:tab path keeps its bytes.
    assert!(paragraph.contains("<w:tab/>"));
    assert!(paragraph.contains("w:footnoteReference"));
}
