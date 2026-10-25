//! Actual DOCX file exports: current formatting and retained review history
//! are checked independently, including the inline path with no run registry.
use super::*;
use serde_json::json;

const AUTHOR: &str = "修订作者";
const DATE: &str = "2026-01-01T00:00:00Z";

fn revision(old: &str) -> String {
    format!(r#"<w:rPrChange w:id="7" w:author="{AUTHOR}" w:date="{DATE}"><w:rPr>{old}</w:rPr></w:rPrChange>"#)
}

fn formula(context: &str, history: bool) -> String {
    let history = if history { revision("<w:i/>") } else { String::new() };
    format!(r#"{context}<m:oMath xmlns:m="{MATH_NS}" xmlns:w="{WORD_NS}"><m:r><w:rPr><w:b/>{history}</w:rPr><m:t>x</m:t></m:r></m:oMath>"#)
}

fn empty_styles() -> String { format!(r#"<w:styles xmlns:w="{WORD_NS}"/>"#) }
fn inherited_color() -> String {
    format!(r#"<w:styles xmlns:w="{WORD_NS}"><w:docDefaults><w:rPrDefault><w:rPr><w:color w:val="123456"/></w:rPr></w:rPrDefault></w:docDefaults></w:styles>"#)
}

fn markdown(xml: &str, display: bool, id: &str) -> String {
    let metadata = crate::math_document::metadata("x",xml,id).unwrap();
    if display { format!("$$x$$\n\n{metadata}\n") }
    else { format!("Before $x${metadata} after.\n") }
}

fn exported(source: &str) -> (String, Vec<String>) {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("source.md");
    let target = dir.path().join("revision.docx");
    std::fs::write(&original,source.as_bytes()).unwrap();
    let before = crate::md_ast::parse(source);
    let result = crate::mdexport::export_docx(source,dir.path().to_str().unwrap(),target.to_str().unwrap(),
        &json!({"cover":{"enabled":false},"toc":{"enabled":false}}),"Revision export").unwrap();
    assert!(result.ok,"{result:?}");
    assert_eq!(std::fs::read(&original).unwrap(),source.as_bytes(),"Export changed the author file");
    assert_eq!(crate::md_ast::parse(source),before,"Export changed the author AST");
    let bytes = std::fs::read(&target).unwrap();
    assert!(bytes.starts_with(b"PK\x03\x04"));
    let parts = crate::publication_typography_docx_tests::unzip_bytes(&bytes);
    (String::from_utf8(parts["word/document.xml"].clone()).unwrap(),result.warns.unwrap_or_default())
}

fn first_math_run(node: &XmlNode) -> Option<&XmlNode> {
    if node.namespace == MATH_NS && node.local() == "r" { return Some(node); }
    node.children().find_map(first_math_run)
}

fn check_current_bold_and_history(body: &str, kept: bool) {
    let root = crate::math_document::parse_xml(body).unwrap();
    let current = child(first_math_run(&root).expect("exported math run"),"rPr").expect("current properties");
    assert!(child(current,"b").is_some(),"Current bold was lost: {body}");
    assert!(child(current,"i").is_none(),"Historical italic became current formatting: {body}");
    let history = child(current,"rPrChange");
    assert_eq!(history.is_some(),kept,"{body}");
    if let Some(history) = history {
        assert_eq!(attr(history,"author"),Some(AUTHOR));
        assert_eq!(attr(history,"date"),Some(DATE));
        assert!(child(child(history,"rPr").unwrap(),"i").is_some(),"Old italic was lost: {body}");
    }
}

fn revision_warnings(warns: &[String]) -> usize {
    warns.iter().filter(|code|code.as_str()==WARN_REVISION_NOT_PRESERVED).count()
}

#[test]
fn rpr_change_real_docx_export_preserves_independent_history_inline_and_display() {
    for context in [String::new(),empty_styles()] {
        for display in [false,true] {
            let xml = formula(&context,true);
            let source = markdown(&xml,display,"independent-revision");
            let (body,warns) = exported(&source);
            check_current_bold_and_history(&body,true);
            assert_eq!(revision_warnings(&warns),0,"{warns:?}");
            assert!(!revision_not_preserved(&xml,&body).unwrap());
        }
    }
}

#[test]
fn rpr_change_real_docx_export_warns_when_contextual_history_is_removed_inline_and_display() {
    for display in [false,true] {
        let xml = formula(&inherited_color(),true);
        let source = markdown(&xml,display,"contextual-revision");
        let (body,warns) = exported(&source);
        check_current_bold_and_history(&body,false);
        assert_eq!(revision_warnings(&warns),1,"{warns:?}");
        assert!(revision_not_preserved(&xml,&body).unwrap());
        assert!(xml.contains(AUTHOR) && xml.contains(DATE) && xml.contains("<w:i/>"));
    }
}

#[test]
fn rpr_change_real_docx_export_does_not_warn_without_a_revision() {
    for context in [String::new(),empty_styles(),inherited_color()] {
        for display in [false,true] {
            let (body,warns) = exported(&markdown(&formula(&context,false),display,"no-revision"));
            check_current_bold_and_history(&body,false);
            assert_eq!(revision_warnings(&warns),0,"{warns:?}");
        }
    }
}

#[test]
fn rpr_change_real_docx_export_warns_once_for_multiple_actual_losses() {
    let xml = formula(&inherited_color(),true);
    let source = format!("{}\n{}",markdown(&xml,false,"lost-inline"),markdown(&xml,true,"lost-display"));
    let (body,warns) = exported(&source);
    assert!(!body.contains("rPrChange"));
    assert_eq!(revision_warnings(&warns),1,"{warns:?}");
}

#[test]
fn rpr_change_real_docx_export_reports_history_with_a_removed_style_reference() {
    let context = format!(r#"<w:styles xmlns:w="{WORD_NS}"><w:style w:type="character" w:styleId="Old"><w:rPr><w:i/></w:rPr></w:style></w:styles>"#);
    let history = revision(r#"<w:rStyle w:val="Old"/>"#);
    let xml = format!(r#"{context}<m:oMath xmlns:m="{MATH_NS}" xmlns:w="{WORD_NS}"><m:r><w:rPr>{history}</w:rPr><m:t>x</m:t></m:r></m:oMath>"#);
    for display in [false,true] {
        let (body,warns) = exported(&markdown(&xml,display,"old-style-revision"));
        assert!(!body.contains("rPrChange"),"Dangling historical style reference: {body}");
        assert_eq!(revision_warnings(&warns),1,"{warns:?}");
    }
}

#[test]
fn rpr_change_loss_comparison_ignores_namespace_spelling_but_preserves_record_counts() {
    let original = format!(r#"<w:rPr xmlns:w="{WORD_NS}">{}{}</w:rPr>"#,revision("<w:i/>"),revision("<w:i/>"));
    let same = original.replace("w:","q:").replace("xmlns:w=","xmlns:q=");
    assert!(!revision_not_preserved(&original,&same).unwrap());
    let one = format!(r#"<w:rPr xmlns:w="{WORD_NS}">{}</w:rPr>"#,revision("<w:i/>"));
    assert!(revision_not_preserved(&original,&one).unwrap());
}
