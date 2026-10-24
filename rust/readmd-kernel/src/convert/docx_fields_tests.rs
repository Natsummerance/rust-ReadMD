//! Visible-result filtering of real Word complex fields, independent of captions.
use super::*;
const W: &str = crate::math_document::WORD_NS;
const M: &str = crate::math_document::MATH_NS;
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
fn run(value: &str) -> String {
    format!("<w:r>{value}</w:r>")
}
fn marker(kind: &str) -> String {
    run(&format!("<w:fldChar w:fldCharType=\"{kind}\"/>"))
}
fn text(value: &str) -> String {
    run(&format!("<w:t xml:space=\"preserve\">{value}</w:t>"))
}
fn instruction(value: &str) -> String {
    run(&format!(
        "<w:instrText>{}</w:instrText>",
        crate::docx_writer::xml_escape(value)
    ))
}
fn entries(body: &str) -> Vec<(String, Vec<u8>)> {
    vec![("word/document.xml".into(),format!("<w:document xmlns:w=\"{W}\" xmlns:m=\"{M}\" xmlns:r=\"{R}\"><w:body>{body}</w:body></w:document>").into_bytes())]
}
fn convert(body: &str) -> String {
    docx_convert(&entries(body), Path::new("fields.docx"), false).unwrap()
}

#[test]
fn nested_expression_keeps_only_outer_visible_cache_and_following_text() {
    let source = format!(
        "<w:p>{}{}{}{}{}{}{}{}{}<w:r><w:rPr><w:b/></w:rPr><w:t>VI</w:t></w:r>{}{}</w:p>",
        marker("begin"),
        instruction("= "),
        marker("begin"),
        instruction("SEQ Figure \\s 1"),
        marker("separate"),
        text("2"),
        marker("end"),
        instruction(" + 4 \\* ROMAN"),
        marker("separate"),
        marker("end"),
        text(" after")
    );
    assert_eq!(convert(&source), "**VI** after");
}

#[test]
fn markers_and_text_in_the_same_run_follow_nested_field_stages() {
    let source="<w:p><w:r><w:t>BEFORE </w:t><w:fldChar w:fldCharType=\"begin\"/><w:instrText>= </w:instrText><w:fldChar w:fldCharType=\"begin\"/><w:instrText>SEQ Figure</w:instrText><w:fldChar w:fldCharType=\"separate\"/><w:t>HIDDEN_INNER_CACHE</w:t><w:fldChar w:fldCharType=\"end\"/><w:instrText>+ 4</w:instrText><w:fldChar w:fldCharType=\"separate\"/><w:t>7</w:t><w:fldChar w:fldCharType=\"end\"/><w:t> AFTER</w:t></w:r></w:p>";
    assert_eq!(convert(source), "BEFORE 7 AFTER");
}

#[test]
fn formula_link_formatting_and_note_are_kept_in_result_not_in_outer_code() {
    let source=format!("<w:p>{}{}{}{}{}{}{}{}<w:hyperlink r:id=\"link\"><w:r><w:rPr><w:i/></w:rPr><w:t>VISIBLE_LINK</w:t></w:r></w:hyperlink><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath><w:r><w:footnoteReference w:id=\"1\"/></w:r>{}{}</w:p>",marker("begin"),instruction("= "),marker("begin"),instruction("EQ \\f(1,2)"),marker("separate"),text("HIDDEN_FORMULA_CACHE"),marker("end"),marker("separate"),marker("end"),text(" TAIL"));
    let mut parts = entries(&source);
    parts.push(("word/_rels/document.xml.rels".into(),format!("<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"link\" Type=\"{R}/hyperlink\" Target=\"https://example.test/result\" TargetMode=\"External\"/><Relationship Id=\"note\" Type=\"{R}/footnotes\" Target=\"footnotes.xml\"/></Relationships>").into_bytes()));
    parts.push(("word/footnotes.xml".into(),format!("<w:footnotes xmlns:w=\"{W}\"><w:footnote w:id=\"1\"><w:p><w:r><w:t>NOTE_RESULT</w:t></w:r></w:p></w:footnote></w:footnotes>").into_bytes()));
    let value = docx_convert(&parts, Path::new("fields.docx"), false).unwrap();
    assert!(value.contains("[*VISIBLE_LINK*](https://example.test/result)"));
    assert!(value.contains("$x$"));
    assert!(value.contains("[^word-footnote-1]"));
    assert!(value.contains("NOTE_RESULT"));
    assert!(value.contains("TAIL"));
    assert!(!value.contains("HIDDEN_FORMULA_CACHE"));
    assert!(!value.contains("SEQ"));
}

#[test]
fn complex_fields_cross_paragraphs_and_simple_fields_keep_their_cached_result() {
    let source=format!("<w:p>{}{}</w:p><w:p>{}{}{}{}{}</w:p><w:p>{}{}</w:p><w:p>{}{}</w:p><w:p><w:fldSimple w:instr=\"SEQ Figure\">{}</w:fldSimple>{}</w:p>",marker("begin"),instruction("="),marker("begin"),instruction("SEQ Figure"),marker("separate"),text("HIDDEN_3"),marker("end"),marker("separate"),text("9"),marker("end"),text("TAIL"),text("SIMPLE"),text(" AFTER"));
    assert_eq!(convert(&source), "9\n\nTAIL\n\nSIMPLE AFTER");
}

#[test]
fn unclosed_or_unseparated_fields_preserve_ordinary_text_and_hide_instruction_text() {
    for field in [
        format!(
            "{}{}{}",
            marker("begin"),
            instruction("BROKEN_INSTRUCTION"),
            text("KEEP_BODY")
        ),
        format!(
            "{}{}{}{}",
            marker("begin"),
            instruction("BROKEN_INSTRUCTION"),
            text("KEEP_BODY"),
            marker("end")
        ),
        format!(
            "{}{}{}{}",
            marker("begin"),
            instruction("BROKEN_INSTRUCTION"),
            marker("separate"),
            text("KEEP_BODY")
        ),
        format!(
            "{}{}{}",
            marker("end"),
            marker("separate"),
            text("KEEP_BODY")
        ),
    ] {
        assert_eq!(convert(&format!("<w:p>{field}</w:p>")), "KEEP_BODY");
    }
}

#[test]
fn hidden_hyperlink_and_math_caches_are_not_rendered_or_resolved() {
    let source=format!("<w:p>{}{}<w:hyperlink r:id=\"missing-hidden-target\">{}</w:hyperlink><m:oMath><m:r><m:t>HIDDEN_MATH</m:t></m:r></m:oMath>{}{}{}</w:p>",marker("begin"),instruction("= "),text("HIDDEN_LINK"),marker("separate"),text("VISIBLE"),marker("end"));
    assert_eq!(convert(&source), "VISIBLE");
}

#[test]
fn source_owned_citation_control_stays_editable_inside_visible_field_result() {
    use crate::md_ast::{Block, Document, Inline};
    let citation = crate::publication::citations::scan("[@42]", "en-US")
        .unwrap()
        .citations
        .remove(0);
    let document = Document {
        front_matter: None,
        blocks: vec![Block::Paragraph(vec![Inline::Citation {
            source: citation,
            children: vec![Inline::Text("CITE_RESULT".into())],
        }])],
    };
    let mut package = crate::docx_writer::build(
        &document,
        &serde_json::json!({"frontMatter":{"mode":"none"}}),
        "citation.md",
        &|_| None,
    );
    assert!(!package.parts.is_empty(), "{:?}", package.warns);
    let bytes = &mut package
        .parts
        .iter_mut()
        .find(|(name, _)| name == "word/document.xml")
        .unwrap()
        .1;
    let mut root = docx_notes::xml(bytes).unwrap();
    let body = root
        .children
        .iter_mut()
        .find(|n| n.name == "w:body")
        .unwrap();
    let paragraph = body.children.iter_mut().find(|n| n.name == "w:p").unwrap();
    let control = paragraph
        .children
        .iter()
        .find(|n| n.name == "w:sdt")
        .unwrap()
        .clone();
    let before = docx_notes::xml(
        format!(
            "<w:p xmlns:w=\"{W}\">{}{}{}{}{}{}{}{}</w:p>",
            marker("begin"),
            instruction("="),
            marker("begin"),
            instruction("SEQ X"),
            marker("separate"),
            text("HIDDEN_CACHE"),
            marker("end"),
            marker("separate")
        )
        .as_bytes(),
    )
    .unwrap();
    let after = docx_notes::xml(
        format!(
            "<w:p xmlns:w=\"{W}\">{}{}</w:p>",
            marker("end"),
            text(" TAIL")
        )
        .as_bytes(),
    )
    .unwrap();
    paragraph.children = before.children;
    paragraph.children.push(control);
    paragraph.children.extend(after.children);
    *bytes = docx_xml_fragment(&root, &[]).into_bytes();
    let value = docx_convert(&package.parts, Path::new("citation.docx"), false).unwrap();
    assert_eq!(crate::publication_profile::from_source(&value).unwrap().options["typography"]["size"],11.0);
    // This helper ends at the closing YAML delimiter, before its line ending.
    // Assert that exact boundary as well as every original body byte; do not
    // trim whitespace from the visible field result.
    assert_eq!(crate::md_ast::source_without_front_matter(&value), "\n[@42] TAIL");
    assert!(!value.contains("citation-import:v1"));

    // A hand-edited citation inside the visible field result keeps whitespace
    // supplied by the control rather than joining the adjacent ordinary runs.
    let bytes = &mut package
        .parts
        .iter_mut()
        .find(|(name, _)| name == "word/document.xml")
        .unwrap()
        .1;
    let mut root = docx_notes::xml(bytes).unwrap();
    fn edit(node: &mut XmlNode) {
        if node.name == "w:sdtContent" {
            fn text(node: &mut XmlNode) -> bool {
                if node.name == "w:t" {
                    node.text = " USER_CITATION ".into();
                    return true;
                }
                for child in &mut node.children {
                    if text(child) {
                        return true;
                    }
                }
                false
            }
            text(node);
            return;
        }
        for child in &mut node.children {
            edit(child);
        }
    }
    edit(&mut root);
    let paragraph = root
        .children
        .iter_mut()
        .find(|n| n.name == "w:body")
        .unwrap()
        .children
        .iter_mut()
        .find(|n| n.name == "w:p")
        .unwrap();
    paragraph.children.insert(
        0,
        docx_notes::xml(format!("<w:r xmlns:w=\"{W}\"><w:t>BEFORE</w:t></w:r>").as_bytes())
            .unwrap(),
    );
    *bytes = docx_xml_fragment(&root, &[]).into_bytes();
    let changed = docx_convert(&package.parts, Path::new("citation.docx"), false).unwrap();
    assert!(changed.contains("BEFORE USER_CITATION  TAIL"));
    assert!(!changed.contains("HIDDEN_CACHE"));

    // Malformed fields inside the fallback must use the same original node
    // addresses as the preflight pass, so their ordinary text stays visible.
    let bytes = &mut package
        .parts
        .iter_mut()
        .find(|(name, _)| name == "word/document.xml")
        .unwrap()
        .1;
    let mut root = docx_notes::xml(bytes).unwrap();
    fn malformed(node: &mut XmlNode) {
        if node.name == "w:sdtContent" {
            node.children = docx_notes::xml(
                format!(
                    "<w:p xmlns:w=\"{W}\">{}{}{}{}</w:p>",
                    marker("begin"),
                    instruction("BROKEN_INSTRUCTION"),
                    text(" USER_MALFORMED "),
                    marker("end")
                )
                .as_bytes(),
            )
            .unwrap()
            .children;
            return;
        }
        for child in &mut node.children {
            malformed(child);
        }
    }
    malformed(&mut root);
    *bytes = docx_xml_fragment(&root, &[]).into_bytes();
    let malformed = docx_convert(&package.parts, Path::new("citation.docx"), false).unwrap();
    assert!(
        malformed.contains("BEFORE USER_MALFORMED  TAIL"),
        "{malformed}"
    );
    assert!(!malformed.contains("BROKEN_INSTRUCTION"));
    assert!(!malformed.contains("HIDDEN_CACHE"));
}

#[test]
fn complex_field_nesting_has_a_bounded_import_budget() {
    let source = format!(
        "<w:p>{}{}</w:p>",
        marker("begin").repeat(129),
        text("NEVER_SILENTLY_TRUNCATED")
    );
    assert_eq!(
        docx_convert(&entries(&source), Path::new("fields.docx"), false).unwrap_err(),
        "docx_field_budget"
    );
}
