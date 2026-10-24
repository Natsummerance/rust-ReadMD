//! Pre-resource admission only. Real native PDFs and paint/annotation proofs
//! are tested separately; successful preflight is not production acceptance.
use super::*;
use crate::md_ast::{Block, Document, Inline, ListItem};
use crate::publication_rich::{RichDecoration, RichFormat, RichVerticalAlign};
use serde_json::json;

fn options(pt: f64) -> Value { json!({"typography":{"baselineShiftPt":pt},"pdf":{"engine":"typst"}}) }
fn document(block: Block) -> Document { Document {blocks:vec![block],..Default::default()} }
fn paragraph(values: Vec<Inline>) -> Block { Block::Paragraph(values) }
fn text() -> Inline { Inline::Text("Glyphs gyp 中 e\u{301}.".into()) }
fn link(children: Vec<Inline>) -> Inline { Inline::Link {href:"https://example.invalid/repeated".into(),title:String::new(),children} }
fn local(block: Block, pt: f64) -> Block {
    Block::Styled {attributes:json!({"text":{"baselineShiftPt":pt}}),block:Box::new(block)}
}

#[test]
fn p04_typst_preflight_admits_physical_offsets_and_author_sizes_without_quantising() {
    let style=crate::publication_inline_text::AuthorTextStyle::from_value(&json!({"size":17.5,"sizeCs":12})).unwrap();
    let values=vec![text(),Inline::Strong(vec![text()]),Inline::Emph(vec![text()]),Inline::SoftBreak,
        Inline::AuthorStyled {style,children:vec![link(vec![Inline::Strong(vec![text()])])]},
        Inline::HardBreak,Inline::Styled {format:RichFormat::default(),children:vec![text()]},
        Inline::Code("CODE".into()),Inline::Math("x^2".into()),
        Inline::OfficeMath {latex:"x".into(),omml:"<m:oMath/>".into()}];
    let doc=document(paragraph(values));
    for pt in [-2.5,0.,3.,3.25] {
        assert!(validate_document(&doc,&options(pt),Consumer::Typst).is_ok(),"{pt}");
        assert_eq!(global(&options(pt)).unwrap().unwrap().requested_pt(),pt);
    }
}

#[test]
fn p04_typst_preflight_global_body_does_not_shift_headings_quotes_lists_or_notes() {
    let unsupported=|| vec![Inline::Strike(vec![text()]),Inline::FootnoteRef("n".into())];
    let doc=Document {blocks:vec![
        Block::Heading {level:1,id:"heading".into(),inlines:unsupported(),numbering:None},
        Block::Quote(vec![paragraph(unsupported())]),
        Block::List {ordered:false,start:1,items:vec![ListItem {task:None,blocks:vec![paragraph(unsupported())]}]},
        Block::FootnoteDef {label:"n".into(),blocks:vec![paragraph(unsupported())]},
        paragraph(vec![text()]),
    ],..Default::default()};
    assert!(validate_document(&doc,&options(3.),Consumer::Typst).is_ok());
}

#[test]
fn p04_typst_preflight_local_nonbody_and_nested_zero_are_not_capability_bypasses() {
    for pt in [0.,3.] {
        let blocks=vec![
            local(Block::Heading {level:1,id:"h".into(),inlines:vec![text()],numbering:None},pt),
            local(Block::Code {lang:"text".into(),text:"code".into()},pt),
            Block::Quote(vec![local(paragraph(vec![text()]),pt)]),
            Block::List {ordered:true,start:1,items:vec![ListItem {task:None,blocks:vec![local(paragraph(vec![text()]),pt)]}]},
            Block::FootnoteDef {label:"n".into(),blocks:vec![local(paragraph(vec![text()]),pt)]},
        ];
        for block in blocks {
            assert_eq!(validate_document(&document(block),&json!({}),Consumer::Typst),Err(UNSUPPORTED));
        }
        assert_eq!(validate_document(&document(local(Block::Math("x".into()),pt)),&json!({}),Consumer::Typst),Err(INVALID));
    }
}

#[test]
fn p04_typst_preflight_zero_paragraph_reset_uses_legacy_semantics_then_parent_resumes() {
    let doc=Document {blocks:vec![local(paragraph(vec![Inline::Strike(vec![text()]),Inline::FootnoteRef("n".into())]),0.),
        paragraph(vec![text()])],..Default::default()};
    assert!(validate_document(&doc,&options(3.),Consumer::Typst).is_ok());
    let no_reset=document(paragraph(vec![Inline::Strike(vec![text()])]));
    assert_eq!(validate_document(&no_reset,&options(3.),Consumer::Typst),Err(UNSUPPORTED));
    assert!(validate_document(&no_reset,&options(0.),Consumer::Typst).is_ok());
}

#[test]
fn p04_typst_preflight_semantic_sup_sub_decorations_and_display_are_not_flattened() {
    let mut styles=Vec::new();
    for alignment in [RichVerticalAlign::Sup,RichVerticalAlign::Sub] {
        let mut format=RichFormat::default();format.vertical_align=alignment;styles.push(format);
    }
    let mut underlined=RichFormat::default();underlined.text_decoration=RichDecoration::Underline;styles.push(underlined);
    for format in styles {
        let doc=document(paragraph(vec![Inline::AuthorStyled {style:Default::default(),children:vec![Inline::Styled {format,children:vec![text()]}]}]));
        assert_eq!(validate_document(&doc,&options(3.),Consumer::Typst),Err(UNSUPPORTED));
    }
    for value in [Inline::Strike(vec![text()]),Inline::FootnoteRef("n".into()),
        Inline::Image {src:"never-read.png".into(),alt:String::new(),title:String::new()},
        Inline::Html("<u>RAW HTML</u>".into()),
        Inline::Display {display:crate::publication_rich::RichDisplay::Block,children:vec![text()]}]
    { assert_eq!(validate_document(&document(paragraph(vec![value])),&options(3.),Consumer::Typst),Err(UNSUPPORTED)); }
}

#[test]
fn p04_typst_preflight_real_links_require_only_uniform_visible_ordinary_children() {
    let repeated=link(vec![Inline::Strong(vec![text()])]);
    assert!(validate_document(&document(paragraph(vec![repeated.clone(),repeated])),&options(3.),Consumer::Typst).is_ok());
    for child in [Inline::Code("code".into()),Inline::Math("x".into()),
        Inline::OfficeMath {latex:"x".into(),omml:"<m:oMath/>".into()},
        Inline::FootnoteRef("n".into()),link(vec![text()])]
    {
        let doc=document(paragraph(vec![link(vec![text(),child])]));
        assert_eq!(validate_document(&doc,&options(3.),Consumer::Typst),Err(UNSUPPORTED));
    }
    for children in [vec![],vec![Inline::Text(" \n\t".into())],vec![Inline::HardBreak],vec![Inline::Html("<!-- empty -->".into())]] {
        assert_eq!(validate_document(&document(paragraph(vec![link(children)])),&options(3.),Consumer::Typst),Err(UNSUPPORTED));
    }
}

#[test]
fn p04_typst_source_csl_is_rejected_before_resolution_but_not_code_math_or_reset() {
    assert_eq!(validate_source("BODY [@missing].",&options(3.),Consumer::Typst),Err(UNSUPPORTED));
    assert!(validate_source("BODY `[@not-a-citation]` $x$.",&options(3.),Consumer::Typst).is_ok());
    assert!(validate_source("BODY.\n\n> Dedicated [@not-shifted].",&options(3.),Consumer::Typst).is_ok());
    let reset="<!-- readmd:layout v1 {\"id\":\"reset-csl\",\"text\":{\"baselineShiftPt\":0}} -->\n\nBODY [@reset].";
    assert!(validate_source(reset,&options(3.),Consumer::Typst).is_ok());
    assert!(validate_source("BODY [@legacy].",&options(0.),Consumer::Typst).is_ok());
}

#[test]
fn p04_typst_unsupported_body_preserves_destination_and_does_not_read_or_stage_assets() {
    let root=tempfile::tempdir().unwrap();let data=root.path().join("profile-not-created");let assets=root.path().join("assets-not-created");
    let target=root.path().join("protected.pdf");std::fs::write(&target,b"KEEP ORIGINAL TARGET").unwrap();
    for source in ["![private](never-read.png)","BODY ~~strike~~.","BODY [@missing].",
        "BODY [code `inside`](https://example.invalid).",
        "<!-- readmd:layout v1 {\"id\":\"heading\",\"text\":{\"baselineShiftPt\":0}} -->\n\n# Unsupported local heading"]
    {
        let error=crate::mdexport::export_document_with_data("pdf",source,root.path().to_str().unwrap(),
            target.to_str().unwrap(),&options(3.),"QA",&assets,&data).unwrap_err();
        assert_eq!(error,UNSUPPORTED,"{source}");
        assert_eq!(std::fs::read(&target).unwrap(),b"KEEP ORIGINAL TARGET");
        assert!(!data.exists()&&!assets.exists());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(),1);
    }
}

#[test]
fn p04_typst_source_preview_and_export_share_baseline_admission() {
    for pt in [-2.5,0.,3.,3.25] {
        let request=options(pt);
        assert!(crate::publication_typography::validate_export("pdf","BODY **bold** [link](https://example.invalid).",&request).is_ok());
        assert!(crate::publication_typography::validate_pdf_preview("pdf","BODY **bold** [link](https://example.invalid).",&request).is_ok());
        assert_eq!(crate::publication_typography::validate_export("pdf","BODY ~~strike~~.",&request).is_ok(),pt==0.);
        assert_eq!(crate::publication_typography::validate_pdf_preview("pdf","BODY ~~strike~~.",&request).is_ok(),pt==0.);
    }
}
