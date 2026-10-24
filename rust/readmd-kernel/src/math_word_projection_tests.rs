//! Host boundaries exercise real writer entry points before resources.
use super::*;
use crate::md_ast::{Align,ListItem};
use serde_json::json;

fn xml(enabled:bool,left:u32,right:u32)->String{
    format!(r#"<w:pPr><w:ind w:left="{left}" w:right="{right}"/><w:jc w:val="right"/></w:pPr><m:mathPr><m:dispDef m:val="{}"/></m:mathPr><m:oMathPara><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath></m:oMathPara>"#,u8::from(enabled))
}
fn formula(enabled:bool)->Block{Block::OfficeMath{latex:"x".into(),omml:xml(enabled,720,240)}}
fn probe()->Block{Block::Paragraph(vec![Inline::Image{src:"missing-host-probe.png".into(),alt:"owned probe".into(),title:String::new()}])}
fn document(mut blocks:Vec<Block>)->Document{blocks.insert(0,probe());Document{blocks,..Default::default()}}
fn hosts(enabled:bool)->Vec<Document>{vec![
    document(vec![Block::Styled{attributes:json!({"id":"math-owned"}),block:Box::new(formula(enabled))}]),
    document(vec![Block::Styled{attributes:json!({"text":{"color":"#334455"}}),block:Box::new(formula(enabled))}]),
    document(vec![Block::Styled{attributes:json!({"paragraph":{"left":12.,"right":8.}}),block:Box::new(Block::Styled{attributes:json!({"paragraph":{"left":7.,"right":3.}}),block:Box::new(formula(enabled))})}]),
    document(vec![Block::Quote(vec![formula(enabled)])]),
    document(vec![Block::List{ordered:true,start:1,items:vec![ListItem{task:None,blocks:vec![Block::Paragraph(vec![Inline::Text("Item".into())]),formula(enabled)]}]}]),
    document(vec![Block::Paragraph(vec![Inline::FootnoteRef("n".into())]),Block::FootnoteDef{label:"n".into(),blocks:vec![formula(enabled)]}]),
]}

#[test]
fn new_word_axes_in_unproven_hosts_fail_before_docx_callbacks_and_typst_resources(){
    let style=crate::export_styles::default_style();let dir=tempfile::tempdir().unwrap();
    for enabled in [true,false]{
        let body=Document{blocks:vec![formula(enabled)],..Default::default()};
        for target in ["docx","typst","rust","html","epub"]{validate_document_with_style(&body,target,&style).unwrap();}
        for doc in hosts(enabled){
            for target in ["docx","typst","rust","html","epub"]{assert_eq!(validate_document_with_style(&doc,target,&style).unwrap_err(),"math_unsupported_extension","{target}");}
            let output=crate::docx_writer::build(&doc,&style,"Math",&|_|panic!("host preflight accessed image"));
            assert!(output.parts.is_empty());assert_eq!(output.warns,["math_unsupported_extension"]);
            let error=crate::typst_writer::render(&doc,&style,"missing",dir.path(),"Math").err().unwrap();
            assert_eq!(error,"math_unsupported_extension");assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),0);
            assert!(crate::epub_writer::render_chapters_with_options(&doc,"none",&mut |_:&str|->Option<String>{panic!("EPUB host preflight accessed image")},&style).is_empty());
        }
    }
}

#[test]
fn logical_word_alignment_does_not_open_special_hosts_or_native_tex_before_resources(){
    let style=crate::export_styles::default_style();let dir=tempfile::tempdir().unwrap();
    for logical in ["start","end"] {
        let source=xml(false,720,240).replace("w:val=\"right\"",&format!("w:val=\"{logical}\""));
        let formula=Block::OfficeMath{latex:"x".into(),omml:source};
        let body=Document{blocks:vec![formula.clone()],..Default::default()};
        for target in ["docx","typst","rust","html","epub"]{validate_document_with_style(&body,target,&style).unwrap();}
        assert_eq!(validate_document(&body,"tex").unwrap_err(),"math_unsupported_extension");
        let docs=[
            document(vec![Block::Styled{attributes:json!({"text":{"color":"#334455"}}),block:Box::new(formula.clone())}]),
            document(vec![Block::Quote(vec![formula.clone()])]),
            document(vec![Block::List{ordered:true,start:1,items:vec![ListItem{task:None,blocks:vec![formula.clone()]}]}]),
            document(vec![Block::Paragraph(vec![Inline::FootnoteRef("n".into())]),Block::FootnoteDef{label:"n".into(),blocks:vec![formula]}]),
        ];
        for doc in docs {
            for target in ["docx","typst","rust","html","epub"]{assert_eq!(validate_document_with_style(&doc,target,&style).unwrap_err(),"math_unsupported_extension");}
            let output=crate::docx_writer::build(&doc,&style,"Math",&|_|panic!("logical host preflight accessed image"));
            assert!(output.parts.is_empty());assert_eq!(output.warns,["math_unsupported_extension"]);
            assert_eq!(crate::typst_writer::render(&doc,&style,"missing",dir.path(),"Math").err().unwrap(),"math_unsupported_extension");
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),0);
        }
    }
}

#[test]
fn exact_word_units_keep_the_existing_inline_tex_and_special_host_boundaries(){
    let style=crate::export_styles::default_style();let dir=tempfile::tempdir().unwrap();
    for enabled in [true,false] {
        let source=xml(enabled,0,0).replace(r#"w:left="0" w:right="0""#,r#"w:left="1mm" w:right="18.125pt""#);
        let formula=Block::OfficeMath{latex:"x".into(),omml:source.clone()};
        let body=Document{blocks:vec![formula.clone()],..Default::default()};
        for target in ["docx","typst","rust","html","epub"]{validate_document_with_style(&body,target,&style).unwrap();}
        if enabled{validate_document(&body,"tex").unwrap();}else{assert_eq!(validate_document(&body,"tex").unwrap_err(),"math_unsupported_extension");}
        let inline=source.replace("<m:oMathPara>","").replace("</m:oMathPara>","");
        for target in ["docx","typst","rust","html","epub","tex"]{validate_formula(&inline,false,None,target).unwrap();}
        for doc in [
            document(vec![Block::Styled{attributes:json!({"text":{"color":"#334455"}}),block:Box::new(formula.clone())}]),
            document(vec![Block::Quote(vec![formula.clone()])]),
            document(vec![Block::List{ordered:true,start:1,items:vec![ListItem{task:None,blocks:vec![formula.clone()]}]}]),
            document(vec![Block::Paragraph(vec![Inline::FootnoteRef("n".into())]),Block::FootnoteDef{label:"n".into(),blocks:vec![formula]}]),
        ] {
            for target in ["docx","typst","rust","html","epub"]{assert_eq!(validate_document_with_style(&doc,target,&style).unwrap_err(),"math_unsupported_extension");}
            let output=crate::docx_writer::build(&doc,&style,"Math",&|_|panic!("unit host preflight accessed image"));
            assert!(output.parts.is_empty());assert_eq!(output.warns,["math_unsupported_extension"]);
            assert_eq!(crate::typst_writer::render(&doc,&style,"missing",dir.path(),"Math").err().unwrap(),"math_unsupported_extension");
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),0);
        }
    }
}

#[test]
fn exact_unit_domain_or_precision_failure_precedes_images_and_keeps_the_old_target(){
    let style=crate::export_styles::default_style();let dir=tempfile::tempdir().unwrap();
    for raw in ["1584.000000000000000000000000000001pt".to_string(),format!("0.{}1mm","0".repeat(64)),"-1mm".into()] {
        let source=xml(false,0,0).replace(r#"w:left="0""#,&format!(r#"w:left="{raw}""#));
        let tree=crate::math_document::parse_xml(&source).unwrap();let before=tree.xml();
        let blocks=json!([{"type":"image","src":"not-readable"},{"type":"math","omml":source,"latex":"x","fallback":true}]);
        assert_eq!(validate_json_at_width(&blocks,"rust",Some(420.)).unwrap_err(),"math_unsupported_extension");
        let error=crate::pdf_render::build(&blocks,&style,"",&|_:&str|->Option<String>{panic!("invalid unit preflight accessed image")},&mut Vec::new()).err().unwrap();
        assert_eq!(error.message,"math_unsupported_extension");
        let doc=document(vec![Block::OfficeMath{latex:"x".into(),omml:source}]);
        let output=crate::docx_writer::build(&doc,&style,"Math",&|_|panic!("invalid native unit preflight accessed image"));
        assert!(output.parts.is_empty());assert_eq!(output.warns,["math_unsupported_extension"]);
        let target=dir.path().join("old.docx");std::fs::write(&target,b"old owned unit target").unwrap();
        let dsl=crate::math_dsl::decompile(&tree).unwrap();let md=format!("![bad](missing.png)\n\n```readmd-math v1\n{dsl}\n```\n");
        assert_eq!(crate::mdexport::export_docx(&md,dir.path().to_str().unwrap(),target.to_str().unwrap(),&json!({}),"Math").unwrap_err(),"math_unsupported_extension");
        assert_eq!(std::fs::read(&target).unwrap(),b"old owned unit target");assert_eq!(tree.xml(),before);
    }
}

#[test]
fn ordinary_nested_formulas_and_valid_inline_math_keep_their_prior_contract(){
    let ordinary=xml(false,720,240).split("</w:pPr>").nth(1).unwrap().to_string();
    let doc=Document{blocks:vec![Block::Quote(vec![Block::OfficeMath{latex:"x".into(),omml:ordinary.clone()}])],..Default::default()};
    for target in ["docx","typst","rust","html","epub"]{validate_document(&doc,target).unwrap();}
    let inline=xml(false,720,240).replace("<m:oMathPara>","").replace("</m:oMathPara>","");
    for target in ["docx","typst","rust","html","epub","tex"]{validate_formula(&inline,false,None,target).unwrap();}
}

#[test]
fn display_carriers_cannot_be_hidden_in_inline_links_or_table_cells(){
    let inline=||Inline::OfficeMath{latex:"x".into(),omml:xml(false,720,240)};
    let docs=[
        document(vec![Block::Paragraph(vec![inline()])]),
        document(vec![Block::Paragraph(vec![Inline::Link{href:"https://example.test/owned".into(),title:String::new(),children:vec![inline()]}])]),
        document(vec![Block::Table{aligns:vec![Align::Left],header:vec![vec![Inline::Text("Head".into())]],rows:vec![vec![vec![inline()]]]}]),
    ];
    for doc in docs{for target in ["docx","typst","rust","html","epub","tex"]{assert_eq!(validate_document(&doc,target).unwrap_err(),"math_invalid_source");}}
}

#[test]
fn direct_pdf_json_tracks_style_layers_note_and_quote_hosts_before_callbacks(){
    let math=json!({"type":"math","display":true,"latex":"x","omml":xml(false,720,240),"fallback":true});
    let probe=json!({"type":"image","src":"missing-host-probe.png"});
    let mut styled=math.clone();styled["publicationStyle"]=json!({"text":{"color":"#334455"}});
    let mut layered=math.clone();layered["publicationStyle"]=json!([{"paragraph":{"left":12.}},{"paragraph":{"right":8.}}]);
    let mut note=math.clone();note["publicationContext"]=json!("footnote");note["publicationNoteLabel"]=json!("n");
    let style=crate::export_styles::default_style();let callback=|_:&str|->Option<String>{panic!("PDF host preflight accessed image")};
    for blocks in [json!([probe.clone(),styled]),json!([probe.clone(),layered]),json!([probe.clone(),note]),json!([probe,{"type":"quote","blocks":[math]}])]{
        assert_eq!(validate_json_at_width(&blocks,"rust",Some(420.)).unwrap_err(),"math_unsupported_extension");
        let error=crate::pdf_render::build(&blocks,&style,"",&callback,&mut Vec::new()).err().unwrap();assert_eq!(error.message,"math_unsupported_extension");
    }
}

fn section(width:f64,left:f64,right:f64)->Block{
    Block::SectionStart(serde_json::from_value(json!({"id":"section","break":"nextPage","page":{"size":"Custom","width":width,"height":297.,"marginLeft":left,"marginRight":right}})).unwrap())
}

#[test]
fn actual_narrow_section_is_checked_before_earlier_images_and_wide_section_is_admitted(){
    let style=crate::export_styles::default_style();let dir=tempfile::tempdir().unwrap();
    let narrow=document(vec![section(80.,30.,30.),Block::OfficeMath{latex:"x".into(),omml:xml(false,720,480)}]);
    // Its global body admits 60pt of indents; the actual 20mm section cannot.
    validate_document_at_width(&narrow,"docx",Some(420.)).unwrap();
    for target in ["docx","typst"]{assert_eq!(validate_document_with_style(&narrow,target,&style).unwrap_err(),"math_unsupported_extension");}
    let output=crate::docx_writer::build(&narrow,&style,"Math",&|_|panic!("section preflight accessed image"));assert!(output.parts.is_empty());assert_eq!(output.warns,["math_unsupported_extension"]);
    assert_eq!(crate::typst_writer::render(&narrow,&style,"missing",dir.path(),"Math").err().unwrap(),"math_unsupported_extension");assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),0);
    let wide=Document{blocks:vec![section(210.,18.,18.),Block::OfficeMath{latex:"x".into(),omml:xml(false,720,480)}],..Default::default()};
    let small=crate::export_styles::sanitize(&json!({"page":{"size":"Custom","width":80.,"height":200.,"marginLeft":30.,"marginRight":30.}}));
    for target in ["docx","typst"]{validate_document_with_style(&wide,target,&small).unwrap();}
}

#[test]
fn inactive_native_word_axes_survive_instead_of_becoming_center_without_indents(){
    for enabled in [true,false]{
        let raw=xml(enabled,720,240);let source=crate::math_document::parse_xml(&raw).unwrap();
        let result=omml::docx_math_projection_tree(source.clone(),true).unwrap();
        assert_eq!(result.paragraph,Some(omml::DocxMathParagraph{alignment:Some("right".into()),left:Some("720".into()),right:Some("240".into())}));
        assert_eq!(source.xml(),crate::math_document::parse_xml(&raw).unwrap().xml());
    }
    // Inactive native Word jc is stored verbatim, including logical values and
    // alignments outside the finite LTR domain. It does not become a math jc.
    for jc in ["start","end","both","mediumKashida","distribute","numTab","highKashida","lowKashida","thaiDistribute"]{
        let raw=xml(true,720,240).replace("w:val=\"right\"",&format!("w:val=\"{jc}\""));
        let result=omml::docx_math_projection_tree(crate::math_document::parse_xml(&raw).unwrap(),true).unwrap();assert_eq!(result.paragraph.unwrap().alignment.as_deref(),Some(jc));
    }
    let huge=Document{blocks:vec![Block::OfficeMath{latex:"x".into(),omml:xml(true,31681,0)}],..Default::default()};
    validate_document(&huge,"html").unwrap();assert_eq!(validate_document(&huge,"docx").unwrap_err(),"math_unsupported_extension");
    for raw in ["-1mm".to_string(),"1584.000000000000000000000000000001pt".into(),format!("0.{}1mm","0".repeat(64))] {
        let source=xml(true,0,0).replace(r#"w:left="0""#,&format!(r#"w:left="{raw}""#));
        let inactive=Document{blocks:vec![Block::OfficeMath{latex:"x".into(),omml:source}],..Default::default()};
        validate_document(&inactive,"html").unwrap();
        assert_eq!(validate_document(&inactive,"docx").unwrap_err(),"math_unsupported_extension");
    }
}
