//! `w:rPrChange` is a Word review fact (author, date and the pre-revision
//! rPr), never a current format property. It stays verbatim in the author
//! source and in any rPr container the projection does not rewrite; whenever
//! the projection rebuilds an rPr from effective properties, the stale
//! revision record is stripped instead of forging history.
use super::*;
use crate::math_document::xml_semantics;

const AUTHOR: &str = "审阅者";
const DATE: &str = "2026-01-01T00:00:00Z";
const CHANGE: &str = concat!(
    r#"<w:rPrChange w:author="审阅者" w:date="2026-01-01T00:00:00Z">"#,
    r#"<w:rPr><w:i/></w:rPr></w:rPrChange>"#
);

/// Form A carries other effective formatting (here `<w:b/>`) beside the
/// revision record, plus a style catalogue so the projection walk runs.
fn form_a(rpr: &str) -> String {
    format!(r#"<w:styles xmlns:w="{WORD_NS}"/><m:oMath xmlns:m="{MATH_NS}" xmlns:w="{WORD_NS}"><m:r><w:rPr>{rpr}</w:rPr><m:t>x</m:t></m:r></m:oMath>"#)
}
fn form_a_with_revision() -> String { form_a(&format!("<w:b/>{CHANGE}")) }
fn form_a_without_revision() -> String { form_a("<w:b/>") }
/// Form B holds only the revision record and no context injects formatting.
fn form_b_math() -> String {
    format!(r#"<m:oMath xmlns:m="{MATH_NS}" xmlns:w="{WORD_NS}"><m:r><w:rPr>{CHANGE}</w:rPr><m:t>x</m:t></m:r></m:oMath>"#)
}

fn materialized(xml: &str) -> String {
    let mut root = crate::math_document::parse_xml(xml).unwrap();
    materialize(&mut root).unwrap();
    root.xml()
}
fn root_oMath(root: &XmlNode) -> Option<&XmlNode> {
    if root.local() == "oMath" { return Some(root); }
    root.children().find(|n| n.local() == "oMath")
}
fn first_run_style(xml: &str) -> crate::omml::RunStyle {
    fn find(node: &crate::omml::Node) -> Option<&crate::omml::Node> {
        if node.is_math() && node.name == "r" { return Some(node); }
        node.children.iter().find_map(find)
    }
    let parsed = crate::omml::parse(xml).unwrap();
    crate::omml::run_style(find(&parsed).expect("a run"))
}
fn docx_body(markdown: &str) -> (String, String) {
    let document = crate::md_ast::parse(markdown);
    let mut omml = String::new();
    for block in &document.blocks {
        if let crate::md_ast::Block::OfficeMath { omml: xml, .. } = block { omml = xml.clone(); }
    }
    let before = document.clone();
    let package = crate::docx_writer::build(
        &document,
        &serde_json::json!({"cover":{"enabled":false},"toc":{"enabled":false}}),
        "rpr-change",
        &|_| None,
    );
    assert_eq!(document, before, "export never mutates the author model");
    let body = String::from_utf8(
        package.parts.iter().find(|(path, _)| path == "word/document.xml")
            .unwrap_or_else(|| panic!("missing document.xml: {:?}", package.warns)).1.clone(),
    ).unwrap();
    (body, omml)
}

#[test]
fn rpr_change_roundtrips_through_readable_dsl_with_author_and_date() {
    for (label, xml) in [("form A", form_a_with_revision()), ("form B", form_b_math())] {
        let doc = crate::math_document::parse(&xml, "omml", false).unwrap();
        let omml = doc.root.children().find(|n| n.local() == "oMath").unwrap();
        let dsl = crate::math_dsl::decompile(omml).unwrap();
        for needle in ["rPrChange", "审阅者", "2026-01-01T00:00:00Z", "word.i"] {
            assert!(dsl.contains(needle), "{label} kept {needle} in the DSL: {dsl}");
        }
        let (round, _) = crate::math_dsl::compile(&dsl, false).unwrap();
        // compile reparses its own serialization, so both sides sit under #root.
        let round = root_oMath(&round).unwrap_or(&round);
        assert_eq!(xml_semantics(omml), xml_semantics(round), "{label} must be lossless");
    }
}

#[test]
fn rpr_change_form_a_replacement_projection_strips_revision_and_keeps_axes() {
    let xml = form_a_with_revision();
    let doc = crate::math_document::parse(&xml, "omml", false).unwrap();
    let original = doc.root.xml();
    let out = materialized(&xml);
    assert_eq!(doc.root.xml(), original, "materialize never mutates the author source");
    assert!(!out.contains("rPrChange") && !out.contains(AUTHOR) && !out.contains(DATE), "{out}");
    assert!(out.contains("w:b"), "the effective bold value survives: {out}");
    // The four visible axes are bit-identical to the revision-free twin.
    let plain = materialized(&form_a_without_revision());
    assert_eq!(first_run_style(&out), first_run_style(&plain));
    assert!(first_run_style(&out).bold);
}

#[test]
fn rpr_change_form_a_docx_fragment_has_no_revision_or_author() {
    let (fragment, _) = crate::omml::docx_math_fragment(&form_a_with_revision(), false).unwrap();
    assert!(!fragment.contains("rPrChange") && !fragment.contains(AUTHOR) && !fragment.contains(DATE), "{fragment}");
    assert!(fragment.contains("w:b"), "{fragment}");
    let (plain, _) = crate::omml::docx_math_fragment(&form_a_without_revision(), false).unwrap();
    assert_eq!(first_run_style(&fragment), first_run_style(&plain));
}

#[test]
fn rpr_change_in_styles_and_docdefaults_never_leaks_into_run_projection() {
    let xml = format!(
        concat!(
            r#"<w:styles xmlns:w="{W}">"#,
            r#"<w:docDefaults><w:rPrDefault><w:rPr><w:i/>{C}</w:rPr></w:rPrDefault></w:docDefaults>"#,
            r#"<w:style w:type="paragraph" w:styleId="P"><w:rPr><w:b/>{C}</w:rPr></w:style>"#,
            r#"</w:styles><w:pPr xmlns:w="{W}"><w:pStyle w:val="P"/></w:pPr>"#,
            r#"<m:oMath xmlns:m="{M}" xmlns:w="{W}"><m:r><m:t>x</m:t></m:r></m:oMath>"#
        ),
        W = WORD_NS, M = MATH_NS, C = CHANGE
    );
    let out = materialized(&xml);
    assert!(!out.contains("rPrChange") && !out.contains(AUTHOR), "{out}");
    // Both real formats still apply: docDefaults italic and catalogue bold.
    assert!(out.contains("w:i") && out.contains("w:b"), "{out}");
    let style = first_run_style(&out);
    assert!(style.bold && style.word_italic == Some(true));
}

#[test]
fn rpr_change_form_a_native_docx_projection_strips_revision_source_keeps_it() {
    let dsl = format!(
        concat!(
            r#"row(word.styles(word.style(word.rPr(word.b()), "#,
            r#"type: "paragraph", styleId: "Equation")), "#,
            r#"word.pPr(word.pStyle(val: "Equation")), "#,
            r#"office.oMathPara(office.oMath(office.r(word.rPr(word.b(), "#,
            r#"word.rPrChange(word.rPr(word.i()), author: "{A}", date: "{D}")), office.t("x")))))"#
        ),
        A = AUTHOR, D = DATE
    );
    let markdown = format!("```readmd-math v1\n{dsl}\n```\n\n");
    let (body, omml) = docx_body(&markdown);
    // The persistent anchor keeps the revision fact in the author source.
    assert!(omml.contains("rPrChange") && omml.contains(AUTHOR) && omml.contains(DATE), "{omml}");
    // The exported native projection follows the replacement branch.
    assert!(!body.contains("rPrChange") && !body.contains(AUTHOR) && !body.contains(DATE), "{body}");
    assert!(body.contains("w:b"), "the projected bold axis survives: {body}");
}

#[test]
fn rpr_change_form_b_preserves_revision_verbatim_in_projection_and_native_export() {
    // Without any context the projection leaves the formula untouched.
    let bare = form_b_math();
    assert_eq!(materialized(&bare), bare, "an untouched container keeps the revision bytes");
    // With a catalogue but no injected formatting, the keep branch retains
    // the original rPr container (only rStyle would be dropped).
    let with_styles = format!(r#"<w:styles xmlns:w="{WORD_NS}"/>{bare}"#);
    let kept = materialized(&with_styles);
    assert!(kept.contains("rPrChange") && kept.contains(AUTHOR) && kept.contains(DATE), "{kept}");
    assert!(!kept.contains("w:styles"), "the context is still removed: {kept}");
    // The native export restores the verbatim record from the run source.
    let dsl = format!(
        concat!(
            r#"row(word.styles(), office.oMathPara(office.oMath(office.r(word.rPr("#,
            r#"word.rPrChange(word.rPr(word.i()), author: "{A}", date: "{D}")), office.t("x")))))"#
        ),
        A = AUTHOR, D = DATE
    );
    let markdown = format!("```readmd-math v1\n{dsl}\n```\n\n");
    let (body, omml) = docx_body(&markdown);
    assert!(omml.contains("rPrChange") && omml.contains(AUTHOR), "{omml}");
    assert!(body.contains("rPrChange") && body.contains(AUTHOR) && body.contains(DATE), "{body}");
}



