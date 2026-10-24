//! Cross-consumer checks. Core policy counterexamples live with the plan.
use crate::{docx_writer, epub_writer, md_ast};

fn native() -> &'static str {
    "```readmd-math v1\nrow(office.mathPr(office.dispDef(val: false), office.preSp(val: 120), office.postSp(val: 240), office.interSp(val: 40), office.intraSp(val: 60)), office.oMathPara(office.oMath(frac(a,b))))\n```\n\n"
}
fn source(reverse: bool) -> String {
    let plain="Plain body 😀 remains unchanged.\n\n$$\\frac{x}{y}$$\n\nInline $x^2$ and again $x^2$.\n\n";
    if reverse {format!("{}{plain}",native())}else{format!("{plain}{}",native())}
}
fn part(package:&docx_writer::DocxPackage,name:&str)->String {
    String::from_utf8(package.parts.iter().find(|(path,_)|path==name).unwrap().1.clone()).unwrap()
}

#[test]
fn document_defaults_export_docx_has_one_policy_and_keeps_author_document() {
    for reverse in [false,true] {
        let source=source(reverse);let doc=md_ast::parse(&source);let before=doc.clone();
        let output=docx_writer::build(&doc,&serde_json::json!({}),"mixed.md",&|_|None);
        assert!(!output.parts.is_empty(),"{:?}",output.warns);
        let settings=part(&output,"word/settings.xml");
        let root=crate::math_document::parse_xml(&settings).unwrap();
        let math_pr=root.children().next().unwrap().children().find(|node|node.local()=="mathPr").unwrap();
        assert!(!crate::math_defaults::MathDefaultsState::from_xml(math_pr).unwrap().effective.display_defaults);
        for property in ["<m:preSp m:val=\"120\"","<m:postSp m:val=\"240\"","<m:interSp m:val=\"40\"","<m:intraSp m:val=\"60\""] {
            assert!(settings.contains(property),"{settings}");
        }
        assert_eq!(settings.matches("<m:mathPr").count(),1);
        assert!(part(&output,"word/document.xml").contains("Plain body"));
        assert_eq!(doc,before);
    }
}

#[test]
fn document_defaults_export_epub_and_html_use_the_derived_math_without_changing_source() {
    for reverse in [false,true] {
        let source=source(reverse).replace('\n',"\r\n");let before=source.clone();let doc=md_ast::parse(&source);
        let chapters=epub_writer::render_chapters(&doc,"none",&mut |_|None);
        assert_eq!(chapters.len(),1);let body=&chapters[0].body;
        assert_eq!(body.matches("<math ").count(),4,"{body}");
        assert!(body.contains("+6pt")&&body.contains("+12pt"),"{body}");
        assert!(body.contains("Plain body"));
        let html=crate::html_math::lower_dsl(&source,12.).unwrap();
        assert_eq!(html.formulas,4);assert_eq!(html.markdown.matches("<svg ").count(),4);
        assert!(!html.markdown.contains("$$\\frac"));assert!(!html.markdown.contains("$x^2$"));
        assert!(html.markdown.contains("Plain body 😀 remains unchanged."));
        assert_eq!(source,before);
    }
}

#[test]
fn document_defaults_export_both_pdf_consumers_accept_the_same_author_document() {
    for reverse in [false,true] {
        let source=source(reverse);let doc=md_ast::parse(&source);let before=doc.clone();
        let directory=std::env::temp_dir().join(format!("readmd-document-defaults-{}",uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let typst=crate::typst_writer::render(&doc,&serde_json::json!({}),"",&directory,"mixed.md").unwrap();
        assert!(typst.text.contains("Plain body"));
        let json=md_ast::to_render_json(&doc.blocks);let original=json.clone();
        let mut warnings=Vec::new();
        let pdf=crate::pdf_render::render_bytes(&json,&crate::export_styles::default_style(),directory.to_str().unwrap(),&|_|None,&mut warnings).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));assert_eq!(json,original);assert_eq!(doc,before);
        // Remove only files created directly by this test; no shared cache.
        for entry in std::fs::read_dir(&directory).unwrap(){let path=entry.unwrap().path();assert!(path.is_file());std::fs::remove_file(path).unwrap();}
        std::fs::remove_dir(&directory).unwrap();
    }
}

#[test]
fn document_defaults_export_explicit_conflicts_fail_before_images_or_typst_files() {
    let conflict=format!("{}\n```readmd-math v1\nrow(office.mathPr(office.dispDef()), office.oMathPara(office.oMath(x)))\n```\n\n![unused](must-not-read.png)",native());
    let doc=md_ast::parse(&conflict);
    let output=docx_writer::build(&doc,&serde_json::json!({}),"bad.md",&|_|panic!("preflight must precede image IO"));
    assert!(output.parts.is_empty());assert!(output.warns[0].starts_with("math_document_defaults_conflict"));
    assert!(epub_writer::render_chapters(&doc,"none",&mut |_|panic!("preflight must precede image IO")).is_empty());
    let unused=std::env::temp_dir().join(format!("readmd-document-defaults-absent-{}",uuid::Uuid::new_v4()));
    let error=crate::typst_writer::render(&doc,&serde_json::json!({}),"",&unused,"bad.md").err().unwrap();
    assert!(error.starts_with("math_document_defaults_conflict"));assert!(!unused.exists());
    assert!(crate::html_math::lower_dsl(&conflict,12.).err().unwrap().starts_with("math_document_defaults_conflict"));
}

#[test]
fn document_defaults_pdf_does_not_interpret_math_in_unknown_extension_fields() {
    let doc=md_ast::parse(&source(false));let mut json=md_ast::to_render_json(&doc.blocks);
    let style=crate::export_styles::default_style();let mut baseline_warnings=Vec::new();
    let baseline=crate::pdf_render::build(&json,&style,"",&|_|None,&mut baseline_warnings).unwrap();
    json[0]["future"]=serde_json::json!({"type":"math","t":"math","latex":"\\hspace{999999em}","omml":false});
    let source_json=json.clone();let mut warnings=Vec::new();
    let result=crate::pdf_render::build(&json,&style,"",&|_|None,&mut warnings).unwrap();
    assert_eq!(json,source_json);assert_eq!(warnings,baseline_warnings);assert_eq!(result.pages.len(),baseline.pages.len());
    for (actual,expected) in result.pages.iter().zip(&baseline.pages){assert_eq!(actual.text,expected.text);assert_eq!(actual.items.len(),expected.items.len());}
}
