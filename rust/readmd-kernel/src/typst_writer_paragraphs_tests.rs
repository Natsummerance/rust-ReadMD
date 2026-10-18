//! Source/service regressions. Pinned-engine baseline proof is separate.
use super::*;
use crate::publication_typography::{self as typography, Consumer};
use serde_json::json;

fn options(mode: Value) -> Value {
    json!({"typography":{"font":"Arial","size":11,"spacing":0,"lineSpacing":mode},
        "frontMatter":{"mode":"none"},"cover":{"enabled":false},"toc":{"enabled":false},
        "pdf":{"engine":"typst"}})
}
fn exact(pt: f64) -> Value {json!({"mode":"exact","pt":pt})}
fn paragraph(value: &str) -> Block {Block::Paragraph(vec![Inline::Text(value.into())])}
fn render_case(doc: &Document, options: &Value) -> (tempfile::TempDir, Source) {
    let dir=tempfile::tempdir().unwrap();let source=render(doc,options,"",dir.path(),"M3").unwrap();(dir,source)
}

#[test]
fn typst_line_modes_are_supported_by_actual_consumer_and_pdf_preview_only() {
    for mode in [json!({"mode":"multiple","multiple":1.5}),exact(18.),json!({"mode":"atLeast","pt":30.})] {
        let options=options(mode);
        assert_eq!(typography::validate_export("pdf","Ordinary body.",&options),Ok(()));
        assert_eq!(typography::validate_pdf_preview("pdf","Ordinary body.",&options),Ok(()));
        assert_eq!(typography::validate_preview("Ordinary body.",&options),Err(typography::UNSUPPORTED));
        assert_eq!(typography::validate_consumer(Consumer::Unsupported,"Ordinary body.",&options),Err(typography::UNSUPPORTED));
        for format in ["html","epub"] {assert_eq!(typography::validate_export(format,"Ordinary body.",&options),Err(typography::UNSUPPORTED));}
        // TeX source export has its own native paragraph consumer now. The
        // browser-only preview remains unsupported for these exact metrics.
        for format in ["tex","latex"] {assert_eq!(typography::validate_export(format,"Ordinary body.",&options),Ok(()));}
        let mut spoof=options.clone();spoof["pdf"]["engine"]=json!("rust");
        assert_eq!(typography::validate_consumer(Consumer::Typst,"Ordinary body.",&spoof),Ok(()));
    }
}

#[test]
fn typst_line_spacing_rejects_explicit_local_container_and_nonparagraph_stories() {
    let styled=|block|Block::Styled{attributes:json!({"paragraph":{"lineSpacing":{"mode":"exact","pt":18}}}),block:Box::new(block)};
    for block in [Block::Quote(vec![styled(paragraph("Q"))]),
        Block::List{ordered:false,start:1,items:vec![crate::md_ast::ListItem{task:None,blocks:vec![styled(paragraph("L"))]}]},
        Block::FootnoteDef{label:"n".into(),blocks:vec![styled(paragraph("N"))]},
        styled(Block::Heading{level:1,id:"h".into(),inlines:vec![Inline::Text("Heading".into())]}),
        styled(Block::Math("x".into()))] {
        let doc=Document{blocks:vec![block],..Default::default()};
        assert_eq!(typography::validate_typst_document(&doc,&json!({})),Err(typography::UNSUPPORTED));
    }
}

#[test]
fn typst_stateful_or_unmeasured_inline_content_fails_before_resource_creation() {
    for inline in [Inline::FootnoteRef("n".into()),Inline::Image{src:"must-not-read.svg".into(),alt:"Image".into(),title:String::new()},
        Inline::Html("<span>Unknown</span>".into())] {
        let doc=Document{blocks:vec![Block::Paragraph(vec![inline])],..Default::default()};
        let dir=tempfile::tempdir().unwrap();
        assert_eq!(render(&doc,&options(exact(18.)),"",dir.path(),"M3").err().unwrap(),typography::UNSUPPORTED);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(),0);
    }
}

#[test]
fn typst_no_typed_spacing_keeps_original_native_paragraph_and_keeplines_source() {
    let doc=Document{blocks:vec![paragraph("Legacy paragraph.")],..Default::default()};
    let mut legacy=options(Value::Null);legacy["typography"]["lineHeight"]=json!(1.9);
    legacy["typography"]["keepLines"]=json!(true);
    let (_dir,one)=render_case(&doc,&legacy);legacy["typography"].as_object_mut().unwrap().remove("lineSpacing");
    let (_dir,two)=render_case(&doc,&legacy);
    assert_eq!(one.text,two.text);assert!(one.geometry.is_none()&&two.geometry.is_none());
    assert!(one.text.contains("#block(breakable:false)[#par[#text(\"Legacy paragraph.\")]]\n"));
    assert!(!one.text.contains("readmd-typed-paragraph"));
}

#[test]
fn typst_global_body_mode_does_not_change_metadata_heading_containers_or_csl() {
    use crate::publication_rich::{RichBibliography,RichBibliographyItem,RichNode};
    let bib=RichBibliography{title:"References".into(),items:vec![RichBibliographyItem{key:"a".into(),first_field:vec![],rich:vec![RichNode::text("Bibliography own spacing.")]}],hanging_indent:false,line_spacing:3,entry_spacing:2,source_span:None};
    let mut doc=Document{blocks:vec![Block::Heading{level:1,id:"h".into(),inlines:vec![Inline::Text("Heading".into())]},
        paragraph("Typed body."),Block::Quote(vec![paragraph("Quote own story.")]),
        Block::List{ordered:false,start:1,items:vec![crate::md_ast::ListItem{task:None,blocks:vec![paragraph("List own story.")]}]},
        Block::Bibliography(bib)],..Default::default()};
    doc.front_matter=Some("title: Metadata title\nauthors:\n  - id: a\n    name: Synthetic Author\nabstract: Metadata abstract.\n".into());
    let mut style=options(exact(18.));style["frontMatter"]["mode"]=json!("paper");
    let (_dir,source)=render_case(&doc,&style);
    assert_eq!(source.text.matches("#readmd-typed-paragraph(").count(),1);
    assert!(source.text.contains("#text(\"Metadata abstract.\")"));
    assert!(source.text.contains("leading:2em"));
    assert_eq!(doc.blocks.len(),5);
}

#[test]
fn typst_local_physical_pt_is_fixed_and_local_legacy_multiple_overrides_global() {
    for (mode,name,requested) in [(exact(18.),"exact",18.),
        (json!({"mode":"atLeast","pt":18}),"atLeast",18.),
        (json!({"mode":"multiple","multiple":1.5}),"multiple",36.)] {
        let doc=Document{blocks:vec![Block::Styled{attributes:json!({"id":"local","text":{"size":24},"paragraph":{"lineSpacing":mode}}),block:Box::new(paragraph("Large local text."))},paragraph("Following global.")],..Default::default()};
        let before=doc.clone();let (_dir,source)=render_case(&doc,&options(exact(20.)));
        assert!(source.text.contains(&format!("mode:\"{name}\",requested:{requested}pt,base-size:24pt")));
        assert!(source.text.contains("mode:\"exact\",requested:20pt,base-size:11pt"));
        assert_eq!(doc,before);
    }
    let doc=Document{blocks:vec![Block::Styled{attributes:json!({"paragraph":{"lineHeight":2}}),block:Box::new(paragraph("Old local override."))}],..Default::default()};
    let (_dir,source)=render_case(&doc,&options(exact(18.)));
    assert!(source.text.contains("mode:\"multiple\",requested:22pt,base-size:11pt"));
}

#[test]
fn typst_formula_is_rendered_once_and_metric_projection_has_no_resource_side_effect() {
    let doc=crate::md_ast::parse("FIRST  \nSECOND $\\frac{1}{2}$  \nTHIRD\n");
    let before=doc.clone();let (dir,source)=render_case(&doc,&options(exact(18.)));
    assert_eq!(source.text.matches("#image(\"resource-1.svg\"").count(),1);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(),1);
    let call=source.text.lines().find(|line|line.starts_with("#readmd-typed-paragraph(")).unwrap();
    let metric=call.split_once("metric:[").unwrap().1.split_once("],mode:").unwrap().0;
    assert!(!metric.contains("image(")&&!metric.contains("linebreak(")&&!metric.contains("footnote("));
    for word in ["FIRST","SECOND","THIRD"] {assert!(metric.contains(word));}
    assert!(call.contains("object-top:")&&call.contains("object-bottom:"));assert_eq!(doc,before);
}

#[test]
fn typst_math_inside_unmeasured_vertical_or_decoration_transform_is_explicitly_unsupported() {
    use crate::publication_rich::{RichFormat,RichVerticalAlign,RichDecoration};
    for format in [RichFormat{vertical_align:RichVerticalAlign::Sup,..Default::default()},
        RichFormat{text_decoration:RichDecoration::Underline,..Default::default()}] {
        assert_eq!(typography::validate_typst_inlines(&[Inline::Styled{format:format.clone(),children:vec![Inline::Math("x".into())]}]),Err(typography::UNSUPPORTED));
        assert_eq!(typography::validate_typst_inlines(&[Inline::Styled{format,children:vec![Inline::Text("Ordinary styled text".into())]}]),Ok(()));
    }
}

fn context() -> GeometryContext {
    GeometryContext{marker:"readmd-p04-0123456789abcdef0123456789abcdef:".into(),
        paragraphs:vec![paragraphs::ParagraphRequest{index:3,id:Some("p3".into()),requested:18.}]}
}
fn details() -> Value {json!({"coordinateSpace":"pdfParagraphLine","blockIndex":3,"blockId":"p3","line":1,"mode":"exact","requestedPt":18,"requiredPt":25.192,"availableWidthPt":493.22,"availableHeightPt":728.5})}
fn diagnostic(context: &GeometryContext, source: &Path, value: Value) -> String {
    format!("{}:21:6: error: panicked with: {}{}",source.display(),context.marker,value)
}

#[test]
fn typst_geometry_diagnostic_is_bound_to_render_nonce_and_exact_paragraph_request() {
    let dir=tempfile::tempdir().unwrap();let source=dir.path().join("document.typ");
    let context=context();let value=details();let result=context.diagnostic(&diagnostic(&context,&source,value.clone()),&source,dir.path()).unwrap();
    assert_eq!(result,format!("publication_layout_geometry: {value}"));
    assert!(crate::latex_compile::error_body(&result)["details"].is_object());
    if cfg!(windows) {
        let extended=format!("\\\\?\\{}",source.display());
        assert_eq!(context.diagnostic(&diagnostic(&context,Path::new(&extended),value),&source,dir.path()),Some(result));
    }
}

#[test]
fn typst_geometry_diagnostic_rejects_forged_unknown_and_malformed_payloads() {
    let dir=tempfile::tempdir().unwrap();let source=dir.path().join("document.typ");let context=context();
    for (key,value) in [("blockIndex",json!(4)),("blockId",json!("other")),("line",json!(2)),
        ("requestedPt",json!(19)),("requiredPt",json!(17)),("availableWidthPt",json!(-1)),
        ("availableHeightPt",json!(0)),("mode",json!("atLeast")),("extra",json!(true))] {
        let mut fields=details();fields[key]=value;
        assert!(context.diagnostic(&diagnostic(&context,&source,fields),&source,dir.path()).is_none(),"{key}");
    }
    let valid=diagnostic(&context,&source,details());
    for invalid in [valid.replace("0123456789abcdef","fedcba9876543210"),
        format!("source excerpt: {valid}"),valid.replace("panicked with:","assertion failed:"),
        valid.replace("document.typ:21:6:","other.typ:21:6:"),valid.clone()+" trailing unknown data",
        valid.replace("21:6:","0:6:"),valid.replace("21:6:","21:0:"),
        valid.replace("\"line\":1", "\"line\":1,\"line\":1"),
        format!("{valid}\n{}:22:1: error: unknown font family",source.display()),
        format!("{valid}\nerror: missing glyph"),format!("{valid}\n{valid}")] {
        assert!(context.diagnostic(&invalid,&source,dir.path()).is_none(),"{invalid}");
    }
    assert!(context.diagnostic(&valid,Path::new("document.typ"),dir.path()).is_none());
    assert!(context.diagnostic("document.typ:1:1: error: unknown font family",&source,dir.path()).is_none());
    assert!(context.diagnostic("document.typ:1:1: error: missing glyph: 😀",&source,dir.path()).is_none());
}

#[test]
fn typst_geometry_native_relative_stderr_is_bound_to_the_actual_worker_directory() {
    let dir=tempfile::tempdir().unwrap();let source=dir.path().join("document.typ");
    let actual=include_str!("../tests/fixtures/typst-paragraph-native-relative.stderr");
    let context=GeometryContext{marker:"readmd-p04-b3909ca834e240cba3334428f25cdfe6:".into(),
        paragraphs:vec![paragraphs::ParagraphRequest{index:1,id:None,requested:18.}]};
    let error=context.diagnostic(actual,&source,dir.path()).unwrap();
    let response=crate::latex_compile::error_body(&error);
    assert_eq!(response["error_code"],"publication_layout_geometry");
    assert_eq!(response["details"]["blockIndex"],1);assert!(response["details"].get("blockId").is_none());
    for (key,expected) in [("requestedPt",18.),("requiredPt",25.192),
        ("availableWidthPt",493.2283464566929),("availableHeightPt",728.503937007874)] {
        assert!((response["details"][key].as_f64().unwrap()-expected).abs()<1e-9,"{key}");
    }
    // The filename is not a general suffix rule. Another worker, a different
    // generated source, path aliases, log excerpts and extra errors fail shut.
    assert!(context.diagnostic(actual,&source,&dir.path().join("other-worker")).is_none());
    assert!(context.diagnostic(actual,&dir.path().join("other.typ"),dir.path()).is_none());
    assert!(context.diagnostic(actual,&source,Path::new(".")).is_none());
    for path in ["./document.typ","../document.typ","other/document.typ","document.typ.evil","xdocument.typ"] {
        let changed=actual.replacen("document.typ:",&format!("{path}:"),1);
        assert!(context.diagnostic(&changed,&source,dir.path()).is_none(),"{path}");
    }
    let other=dir.path().join("other-worker/document.typ");
    assert!(context.diagnostic(&actual.replacen("document.typ:",&format!("{}:",other.display()),1),&source,dir.path()).is_none());
    for changed in [format!("source excerpt: {actual}"),actual.replace("b3909ca834e240cba3334428f25cdfe6","00000000000000000000000000000000"),
        actual.replace("\"blockIndex\":1","\"blockIndex\":2"),actual.replace("\"requestedPt\":18.0","\"requestedPt\":19.0"),
        format!("{actual}document.typ:42:1: error: unknown font family")] {
        assert!(context.diagnostic(&changed,&source,dir.path()).is_none());
    }
}

#[test]
fn typst_typed_preview_retains_private_mapping_and_ignores_comment_only_metrics() {
    let raw="Body $x$ with <!-- author note --> text.\n";
    let input=crate::publication_pagination::instrument(raw).unwrap();
    let doc=crate::md_ast::parse(&input.content);let before=doc.clone();
    let dir=tempfile::tempdir().unwrap();
    let source=render_mapped(&doc,&options(exact(18.)),"",dir.path(),"M3",&input.nonce).unwrap();
    assert_eq!(source.text.matches("#readmd-typed-paragraph(").count(),1);
    assert!(!source.text.contains("author note"));
    for node in &input.nodes {
        assert_eq!(source.text.matches(&format!("link(\"{}{}\")",crate::publication_pagination::prefix(&input.nonce),node.id)).count(),1);
    }
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(),1);
    assert_eq!(doc,before);
}

#[test]
fn typst_typed_paragraph_cancelled_before_render_creates_no_resources() {
    let dir=tempfile::tempdir().unwrap();let doc=crate::md_ast::parse("Text $x$.\n");
    crate::speech::with_cancel(std::sync::Arc::new(||true),||{
        assert_eq!(render(&doc,&options(exact(18.)),"",dir.path(),"M3").err().unwrap(),"cancelled");
    });
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(),0);
}
