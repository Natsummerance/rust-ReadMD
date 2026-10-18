//! Cross-entry regressions. Native engine geometry and actual applications
//! remain separate tests; source strings alone never certify a PDF engine.
use crate::{md_ast,mdexport,publication_sections,typst_writer};
use serde_json::json;
use std::fs;

const WORKFLOW:&str=include_str!("fixtures/publication-sections-workflow.md");
fn options()->serde_json::Value{json!({"cover":{"enabled":false},"toc":{"enabled":false},"frontMatter":{"mode":"none"},"typography":{"font":"Arial","size":11}})}

#[test]
fn publication_structure_typst_long_cells_keep_author_text_styles_and_local_scope() {
    use crate::publication_rich::{RichBibliography,RichBibliographyItem,RichDisplay as D,RichNode as N};
    let group=|display,children|N::Group{children,display:Some(display),entry_index:Some(0)};
    let label="LONGUNSPACEDLABEL".repeat(12);
    let body="LONGUNSPACEDBODY".repeat(10);
    let mut bibliography=RichBibliography{title:"References".into(),items:vec![RichBibliographyItem{key:"scope".into(),first_field:vec![],rich:vec![
        group(D::LeftMargin,vec![group(D::LeftMargin,vec![N::text(&label)]),group(D::RightInline,vec![N::text("LEFTBODY100")])]),
        group(D::RightInline,vec![N::text(&body),N::text(" العربية 中文 e\u{301} 😀 ")]),
    ]}],hanging_indent:false,line_spacing:1,entry_spacing:0,source_span:None};
    bibliography.normalize_layout().unwrap();
    let doc=md_ast::Document{blocks:vec![md_ast::Block::Paragraph(vec![md_ast::Inline::Text("OUTSIDE_BIBLIOGRAPHY".into())]),md_ast::Block::Bibliography(bibliography)],..Default::default()};
    let dir=tempfile::tempdir().unwrap();
    let source=typst_writer::render(&doc,&options(),"",dir.path(),"Cells").unwrap().text;
    // Wrapping belongs to each actual grid cell, including the right hand
    // body of nested columns. Natural-width measurements stay unwrapped.
    assert_eq!(source.matches("#readmd-csl-cell[").count(),4);
    assert!(source.contains("measure(["));
    assert!(source.contains(&format!("#text(\"{label}\")")));
    assert!(source.contains(&format!("#text(\"{body}\")")));
    assert!(source.contains("العربية 中文 e\u{301} 😀"));
    assert!(!source.contains('\u{200b}')&&!source.contains('\u{00ad}'));
    assert!(source.find("OUTSIDE_BIBLIOGRAPHY").unwrap()<source.find("#let readmd-csl-cell").unwrap());
    assert_eq!(source.matches("#let readmd-csl-cell").count(),1);
    // This is a writer regression, not a substitute for real PDF ink bounds
    // and copyable-text checks in publication-structure-native.cjs.
}

#[test]
fn epub_default_heading_ink_follows_reader_theme_and_preserves_custom_profile_colour(){
    let css=crate::epub_writer::profile_css(&options());
    for level in 1..=6{
        let heading=css.lines().find(|line|line.starts_with(&format!("h{level}{{font-family:"))).unwrap();
        assert!(heading.contains("color:inherit;"),"Default h{level} must not disappear in a reader's night theme: {heading}");
    }
    let css=crate::epub_writer::profile_css(&json!({"headings":{"h1":{"color":"#467acc"},"h2":{"color":"#8a246e"}}}));
    assert!(css.lines().any(|line|line.starts_with("h1{font-family:")&&line.contains("color:#467acc;")));
    assert!(css.lines().any(|line|line.starts_with("h2{font-family:")&&line.contains("color:#8a246e;")));
}

#[test]
fn epub_complex_display_hangs_actual_fullwidth_paragraphs_without_nested_indent_or_cell_duplicates(){
    use crate::publication_rich::{RichBibliography,RichBibliographyItem,RichDisplay as D,RichNode as N};
    let group=|display,children|N::Group{children,display:Some(display),entry_index:Some(0)};
    let mut bib=RichBibliography{title:"References".into(),items:vec![RichBibliographyItem{key:"a".into(),first_field:vec![],rich:vec![
        group(D::Block,vec![N::text("Heading")]),group(D::Indent,vec![group(D::Indent,vec![N::text("Deep")])]),
        group(D::LeftMargin,vec![N::text("[9]")]),group(D::RightInline,vec![group(D::Block,vec![N::text("Body")])]),
        group(D::Block,vec![N::text("Tail")]),
    ]}],hanging_indent:true,line_spacing:3,entry_spacing:2,source_span:None};bib.normalize_layout().unwrap();
    let doc=md_ast::Document{front_matter:None,blocks:vec![md_ast::Block::Bibliography(bib)]};
    let chapters=crate::epub_writer::render_chapters(&doc,"none",&mut |_|None);assert_eq!(chapters.len(),1);
    let root=crate::math_document::parse_xml(&chapters[0].body).unwrap();
    fn paragraphs<'a>(node:&'a crate::math_document::XmlNode,out:&mut Vec<&'a crate::math_document::XmlNode>){
        if node.attributes.get("class").is_some_and(|c|c.split_whitespace().any(|c|c=="readmd-csl-layout-paragraph")){out.push(node);}
        for child in node.children(){paragraphs(child,out);}
    }
    let mut actual=Vec::new();paragraphs(&root,&mut actual);
    assert_eq!(actual.iter().map(|node|node.text()).collect::<Vec<_>>(),["Heading","Deep","[9]","Body","Tail"]);
    for node in actual{let hanging=node.attributes["class"].contains("readmd-csl-layout-hanging");assert_eq!(hanging,["Heading","Deep","Tail"].contains(&node.text().as_str()));}
    assert_eq!(chapters[0].body.matches("readmd-csl-layout-indent").count(),2);
}

#[test]
fn publication_structure_flow_keeps_code_examples_and_original_source_while_creating_real_targets(){
    let dir=tempfile::tempdir().unwrap();
    let source="\u{feff}---\r\ntitle: Source identity\r\n---\r\n\r\n<!-- readmd-section:v1 {\"id\":\"main\"} -->\r\n\r\n# Main\r\n\r\n```text\r\n<!-- readmd-section:v1 {\"id\":\"literal\"} -->\r\n```\r\n";
    let projected=mdexport::section_flow_source(source,&crate::export_styles::sanitize(&options())).unwrap();
    assert!(projected.contains("id=\"readmd-section-main\""));
    assert!(projected.contains("<!-- readmd-section:v1 {\"id\":\"literal\"} -->"));
    assert_eq!(projected.matches("publication-section-boundary").count(),1);
    assert!(projected.starts_with("\u{feff}---\r\ntitle: Source identity\r\n"));
    let original=dir.path().join("original.md");fs::write(&original,source).unwrap();
    let doc=crate::publication::prepare_with_options(source,dir.path().to_str().unwrap(),&options()).unwrap();
    assert_eq!(doc.data["source_sections"].as_array().unwrap().len(),1);
    assert_eq!(doc.projection.revision,crate::document_transaction::revision(source));
    assert_eq!(fs::read_to_string(original).unwrap(),source);
}

#[test]
fn publication_structure_epub_chapters_own_the_boundary_of_the_following_heading(){
    let source="<!-- readmd-section:v1 {\"id\":\"front\"} -->\n\n# Preface\n\nBefore.\n\n<!-- readmd-section:v1 {\"id\":\"main\"} -->\n\n# Main\n\n[Return](#readmd-section-front).\n";
    let chapters=crate::epub_writer::render_chapters(&md_ast::parse(source),"h1",&mut |_|None);
    assert_eq!(chapters.len(),2,"A marker alone must not create an empty preface chapter");
    assert!(chapters[0].body.contains("id=\"h-readmd-section-front\""));
    assert!(!chapters[0].body.contains("id=\"h-readmd-section-main\""));
    assert!(chapters[1].body.contains("id=\"h-readmd-section-main\""));
    assert!(chapters[1].body.contains("href=\"chapter_1.xhtml#h-readmd-section-front\""));
    for chapter in chapters{assert!(!chapter.body.contains("readmd-section:v1"));}
}

#[test]
fn publication_structure_invalid_declarations_protect_every_legacy_export_target_and_input_directory(){
    let dir=tempfile::tempdir().unwrap();let original=dir.path().join("source.md");
    let source="<!-- readmd-section:v1 {\"id\":\"main\",\"page\":{\"columns\":9}} -->\n\nBody";fs::write(&original,source).unwrap();
    for format in ["docx","epub","tex","html","pdf"]{
        let target=dir.path().join(format!("old.{format}"));fs::write(&target,b"existing owned result").unwrap();
        let result=match format{
            "docx"=>mdexport::export_docx(source,dir.path().to_str().unwrap(),target.to_str().unwrap(),&options(),"Source"),
            "epub"=>mdexport::export_epub(source,dir.path().to_str().unwrap(),target.to_str().unwrap(),&options(),"Source"),
            "tex"=>mdexport::export_tex(source,dir.path().to_str().unwrap(),target.to_str().unwrap(),&options(),"Source"),
            "html"=>mdexport::export_html(source,dir.path().to_str().unwrap(),target.to_str().unwrap(),&options(),"Source",dir.path()),
            _=>mdexport::export_pdf(source,dir.path().to_str().unwrap(),target.to_str().unwrap(),&options(),"Source",dir.path()),
        };
        assert_eq!(result.err().as_deref(),Some("publication_section_geometry"),"{format}");
        assert_eq!(fs::read(&target).unwrap(),b"existing owned result");
    }
    assert_eq!(fs::read_to_string(original).unwrap(),source);
    assert!(fs::read_dir(dir.path()).unwrap().all(|entry|entry.unwrap().file_type().unwrap().is_file()));
}

#[test]
fn publication_structure_core_pdf_explicitly_requires_a_real_paged_engine_without_replacing_output(){
    let dir=tempfile::tempdir().unwrap();let target=dir.path().join("existing.pdf");fs::write(&target,b"original PDF result").unwrap();
    let result=mdexport::export_pdf(WORKFLOW,dir.path().to_str().unwrap(),target.to_str().unwrap(),&options(),"Workflow",dir.path());
    assert_eq!(result.err().as_deref(),Some("publication_sections_require_paged_engine"));
    assert_eq!(fs::read(target).unwrap(),b"original PDF result");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(),1);
}

#[test]
fn publication_structure_managed_writer_uses_physical_parity_and_current_section_geometry(){
    let dir=tempfile::tempdir().unwrap();let doc=md_ast::parse(WORKFLOW);
    let source=typst_writer::render(&doc,&options(),"",dir.path(),"Workflow").unwrap().text;
    assert!(source.contains("#pagebreak(weak:true,to:\"odd\")"));
    assert!(source.contains("width:297mm,height:210mm"));
    assert!(source.contains("#readmd-section-columns("));
    assert!(source.contains("count:2,gutter:8mm"));
    assert!(source.contains("starts-page:false")==false,"Fixture contains no continuous section");
    assert_eq!(source.matches("#metadata((index:").count(),4);
    assert!(!source.contains("readmd-section:v1"));
    let sections=publication_sections::resolve(&doc,&crate::export_styles::sanitize(&options())).unwrap();
    let tex=crate::latex_writer::render_document(&doc,&crate::latex_writer::LatexOptions{title:String::new(),bib_engine:String::new(),publication_sections:sections,..Default::default()},&|_|None);
    assert!(tex.contains("\\ifodd\\value{readmdphysicalpage}"));
    assert!(tex.contains("layoutwidth=297mm,layoutheight=210mm"));
    assert!(tex.contains("\\renewcommand{\\thepage}{\\roman{page}}"));
    assert!(tex.contains("\\renewcommand{\\thepage}{\\Alph{page}}"));
    assert_eq!(tex.matches("\\hypertarget{readmd-section-").count(),4);
    assert!(!tex.contains("readmd-section:v1"));
}
