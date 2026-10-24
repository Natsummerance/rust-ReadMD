use super::*;
use serde_json::{json,Value};
use crate::publication_inline_text::{AuthorTextStyle,UNSUPPORTED};

fn authored(attributes:Value,children:Vec<Inline>)->Inline{Inline::AuthorStyled{style:AuthorTextStyle::from_value(&attributes).unwrap(),children}}
fn leaf(text:&str)->Inline{Inline::Text(text.into())}
fn style()->Value{crate::export_styles::sanitize(&json!({"frontMatter":{"mode":"none"},"cover":{"enabled":false},"toc":{"enabled":false},"typography":{"size":11.25}}))}
fn body(blocks:Vec<Block>,settings:&Value)->(String,paragraphs::State){let doc=Document{blocks,..Default::default()};let (source,_,state,_)=render_body_with_bibliography(&doc,&|_|None,"",false,false,Some(settings),0);(source,state)}

#[test]
fn author_points_are_physical_bp_and_keep_link_code_and_bold_identity(){
    let inline=authored(json!({"size":30,"sizeCs":10}),vec![Inline::Strong(vec![Inline::Link{href:"https://example.test/a".into(),title:String::new(),children:vec![leaf("LINK_&")]}]),authored(json!({"cs":true}),vec![leaf("FORCED")]),Inline::Code("a b".into())]);
    let (source,state)=body(vec![Block::Paragraph(vec![inline])],&style());assert!(state.error.is_none());
    assert!(source.contains("\\textbf{\\href{https://example.test/a}{{\\fontsize{30bp}{\\baselineskip}\\selectfont{}LINK\\_\\&}}}"),"{source}");
    assert!(source.contains("{\\fontsize{10bp}{\\baselineskip}\\selectfont{}FORCED}"));
    assert!(source.contains("\\texttt{{\\fontsize{30bp}{\\baselineskip}\\selectfont{}a\\ b}}"));
}

#[test]
fn author_run_boundaries_preserve_only_the_authored_spaces_across_links_axes_and_nested_sizes(){
    let link=|value:&str|Inline::Link{href:"https://example.test/space-boundary".into(),title:String::new(),children:vec![leaf(value)]};
    let children=vec![
        leaf("LEFT"),link("LINK"),leaf(" "),
        authored(json!({"size":18.5}),vec![leaf("INNER"),authored(json!({"size":"base","sizeCs":"base"}),vec![leaf(" RESET")])]),
        leaf(" "),authored(json!({"cs":true}),vec![leaf("FORCED")]),leaf(" END"),
    ];
    let (source,state)=body(vec![caption(json!({"size":30,"sizeCs":10}),children)],&style());assert!(state.error.is_none(),"{:?}",state.error);
    let expected=concat!(
        "{\\fontsize{30bp}{\\baselineskip}\\selectfont{}Table 7}",
        "{\\fontsize{30bp}{\\baselineskip}\\selectfont{}LEFT}",
        "\\href{https://example.test/space-boundary}{{\\fontsize{30bp}{\\baselineskip}\\selectfont{}LINK}}",
        "{\\fontsize{30bp}{\\baselineskip}\\selectfont{} }",
        "{\\fontsize{18.5bp}{\\baselineskip}\\selectfont{}INNER}",
        "{\\fontsize{30bp}{\\baselineskip}\\selectfont{} RESET}",
        "{\\fontsize{30bp}{\\baselineskip}\\selectfont{} }",
        "{\\fontsize{10bp}{\\baselineskip}\\selectfont{}FORCED}",
        "{\\fontsize{30bp}{\\baselineskip}\\selectfont{} END}",
    );
    assert!(source.contains(expected),"The exact visible token stream must contain original spaces and no inserted boundary space: {source}");
    assert_eq!(source.matches("\\selectfont{} }").count(),2,"Only the two whitespace-only leaves may emit standalone spaces");
    assert!(!source.contains("\\selectfont  "),"A control-word delimiter must not consume an author space");
    let (plain,state)=body(vec![Block::Paragraph(vec![leaf("PLAIN LEFT LINK INNER RESET FORCED END")])],&style());assert!(state.error.is_none());assert_eq!(plain,"PLAIN LEFT LINK INNER RESET FORCED END\n\n");
}

#[test]
fn author_whitespace_keeps_code_spaces_and_never_adds_space_between_unseparated_runs(){
    let inline=authored(json!({"size":30,"sizeCs":10}),vec![
        leaf("A"),authored(json!({"cs":true}),vec![leaf("B")]),authored(json!({"size":18.5}),vec![leaf("C")]),
        authored(json!({"size":"base"}),vec![leaf("D")]),Inline::Code(" E F ".into()),
    ]);
    let (source,state)=body(vec![Block::Paragraph(vec![inline])],&style());assert!(state.error.is_none());
    let expected=concat!(
        "{\\fontsize{30bp}{\\baselineskip}\\selectfont{}A}",
        "{\\fontsize{10bp}{\\baselineskip}\\selectfont{}B}",
        "{\\fontsize{18.5bp}{\\baselineskip}\\selectfont{}C}",
        "{\\fontsize{11.25bp}{\\baselineskip}\\selectfont{}D}",
        "\\texttt{{\\fontsize{30bp}{\\baselineskip}\\selectfont{}\\ E\\ F\\ }}",
    );assert!(source.contains(expected),"{source}");assert!(!source.contains("\\selectfont{} "),"No ordinary space exists in these source leaves");
}

#[test]
fn new_block_baseline_and_reset_are_physical_and_do_not_leak_to_plain_blocks(){
    let inline=authored(json!({"size":30}),vec![leaf("BIG "),authored(json!({"size":"base"}),vec![leaf("RESET")])]);
    let (source,state)=body(vec![Block::Paragraph(vec![leaf("BEFORE "),inline.clone(),leaf(" AFTER")]),Block::Styled{attributes:json!({"text":{"size":17.5}}),block:Box::new(Block::Paragraph(vec![inline]))},Block::Paragraph(vec![leaf("PLAIN")])],&style());
    assert!(state.error.is_none());assert!(source.contains("{\\fontsize{11.25bp}{\\baselineskip}\\selectfont{}BEFORE }"));
    assert!(source.contains("{\\fontsize{11.25bp}{\\baselineskip}\\selectfont{}RESET}"));
    assert!(source.contains("{\\fontsize{17.5bp}{\\baselineskip}\\selectfont{}RESET}"));
    assert!(source.ends_with("PLAIN\n\n"),"{source}");
}

#[test]
fn typed_line_metric_uses_the_identical_sized_visible_token_list(){
    let mut settings=style();settings["typography"]["lineSpacing"]=json!({"mode":"exact","pt":60});
    let (source,state)=body(vec![Block::Paragraph(vec![leaf("BASE "),authored(json!({"size":30}),vec![leaf("BIG")])])],&settings);
    assert!(state.error.is_none(),"{:?}",state.error);assert!(state.geometry.is_some());
    assert_eq!(source.matches("{\\fontsize{30bp}{\\baselineskip}\\selectfont{}BIG}").count(),2,"{source}");
    assert_eq!(source.matches("{\\fontsize{11.25bp}{\\baselineskip}\\selectfont{}BASE }").count(),2);
}

#[test]
fn footnote_base_does_not_inherit_the_callout_author_override(){
    let mut settings=style();settings["footnotes"]["size"]=json!(9.5);
    let (source,state)=body(vec![Block::Paragraph(vec![authored(json!({"size":30}),vec![leaf("CALL"),Inline::FootnoteRef("n".into())])]),Block::FootnoteDef{label:"n".into(),blocks:vec![Block::Paragraph(vec![leaf("BASE "),authored(json!({"size":"base"}),vec![leaf("NOTE")])])]}],&settings);
    assert!(state.error.is_none());assert!(source.contains("\\footnote{{\\fontsize{9.5bp}{\\baselineskip}\\selectfont{}BASE }{\\fontsize{9.5bp}{\\baselineskip}\\selectfont{}NOTE}}"),"{source}");
}

#[test]
fn unimplemented_rtl_and_excessive_sizes_return_typed_errors(){
    for (attributes,text) in [(json!({"size":30,"sizeCs":10}),"العربية"),(json!({"size":4096.5}),"ASCII")]{
        let doc=Document{blocks:vec![Block::Paragraph(vec![authored(attributes,vec![leaf(text)])])],..Default::default()};
        let options=LatexOptions{publication_style:Some(style()),bib_engine:String::new(),..Default::default()};
        let error=render_document_with_geometry(&doc,&options,&|_|None).err().unwrap();assert!(error.starts_with(UNSUPPORTED),"{error}");
    }
}

#[test]
fn rtl_text_diagnostic_names_its_local_utf8_byte_coordinates(){
    for (attributes,text) in [(json!({"size":30,"sizeCs":10}),"العربية"),(json!({"size":30,"sizeCs":10}),"עברית"),(json!({"rtl":true}),"EXPLICIT_LATIN")]{
        let (_,state)=body(vec![Block::Paragraph(vec![authored(attributes,vec![leaf(text)])])],&style());
        let error=state.error.unwrap();
        let details:Value=serde_json::from_str(error.strip_prefix(&format!("{UNSUPPORTED}: ")).unwrap()).unwrap();
        assert_eq!(details,json!({"consumer":"tex","reason":"rtl","coordinateSpace":"textUtf8Bytes","from":0,"to":text.len()}));
        assert_eq!(crate::latex_compile::error_body(&error),json!({"ok":false,"error_code":UNSUPPORTED,"details":details}));
    }
}

fn transparent_rtl_container(child:Inline)->Inline{
    let child=Inline::Link{href:"https://example.test/cleared".into(),title:String::new(),children:vec![child]};
    let child=Inline::Citation{source:crate::publication_rich::SourceCitation{span:crate::publication_rich::SourceSpan::new("[@source]",0..9).unwrap(),items:Vec::new(),context:Default::default()},children:vec![child]};
    let child=Inline::Display{display:crate::publication_rich::RichDisplay::RightInline,children:vec![child]};
    let child=Inline::Styled{format:Default::default(),children:vec![child]};
    Inline::Strong(vec![Inline::Emph(vec![Inline::Strike(vec![child])])])
}

#[test]
fn effective_false_rtl_leaves_and_atoms_survive_transparent_ancestors(){
    let cleared=authored(json!({"rtl":false}),vec![leaf("CLEARED_LATIN"),Inline::Code("CODE".into()),Inline::Math("x".into()),Inline::Image{src:"owned.png".into(),alt:"IMAGE_ALT".into(),title:String::new()},Inline::FootnoteRef("n".into())]);
    let inline=authored(json!({"rtl":true,"size":30,"sizeCs":10}),vec![authored(json!({"rtl":true}),Vec::new()),leaf(""),Inline::Code(String::new()),Inline::Math(String::new()),transparent_rtl_container(cleared)]);
    let (source,state)=body(vec![Block::Paragraph(vec![inline,leaf(" RESTORED")]),Block::FootnoteDef{label:"n".into(),blocks:vec![Block::Paragraph(vec![leaf("NOTE_BODY")])]}],&style());
    assert!(state.error.is_none(),"{:?}: {source}",state.error);
    for value in ["CLEARED\\_LATIN","CODE","\\(x\\)","IMAGE\\_ALT","\\footnote{","NOTE\\_BODY","RESTORED"]{assert!(source.contains(value),"{value}: {source}");}
    assert!(source.contains("\\fontsize{30bp}"));
}

#[test]
fn effective_true_rtl_atoms_and_visible_fallbacks_fail_after_cleared_sibling(){
    for atom in [Inline::Math("x".into()),Inline::OfficeMath{latex:"x".into(),omml:crate::latex2omml::latex_to_omml("x",false)},Inline::Image{src:"owned.png".into(),alt:"IMAGE_ALT".into(),title:String::new()},Inline::FootnoteRef("n".into()),Inline::Link{href:"https://example.test/fallback".into(),title:String::new(),children:Vec::new()},Inline::SoftBreak,Inline::HardBreak]{
        let inline=authored(json!({"rtl":true}),vec![transparent_rtl_container(authored(json!({"rtl":false}),vec![leaf("CLEARED")])),atom]);
        let (_,state)=body(vec![Block::Paragraph(vec![inline]),Block::FootnoteDef{label:"n".into(),blocks:vec![Block::Paragraph(vec![leaf("NOTE")])]}],&style());
        assert!(state.error.as_deref().is_some_and(|error|error.starts_with(UNSUPPORTED)),"{:?}",state.error);
    }
    let doc=Document{blocks:vec![Block::Paragraph(vec![authored(json!({"rtl":true}),vec![Inline::Image{src:"owned.png".into(),alt:String::new(),title:String::new()}])])],..Default::default()};
    let (_,_,state,_)=render_body_with_bibliography(&doc,&|_|Some("owned.png".into()),"",false,false,Some(&style()),0);
    assert_eq!(state.error.as_deref(),Some(UNSUPPORTED));
}

#[test]
fn authored_empty_link_uses_selected_axis_and_invisible_atoms_do_not_guard(){
    let inline=authored(json!({"size":30,"sizeCs":10,"cs":true}),vec![Inline::Link{href:"https://example.test/fallback".into(),title:String::new(),children:Vec::new()}]);
    let (source,state)=body(vec![Block::Paragraph(vec![inline])],&style());assert!(state.error.is_none());
    assert!(source.contains("\\href{https://example.test/fallback}{{\\fontsize{10bp}{\\baselineskip}\\selectfont{}https://example.test/fallback}}"),"{source}");
    let empty=authored(json!({"rtl":true}),vec![leaf(""),Inline::Code(String::new()),Inline::Math(String::new()),Inline::Link{href:"#invisible".into(),title:String::new(),children:Vec::new()},Inline::Html(String::new()),authored(json!({"rtl":true}),Vec::new())]);
    let (_,state)=body(vec![Block::Paragraph(vec![empty])],&style());assert!(state.error.is_none(),"{:?}",state.error);
}

fn caption(author:Value,children:Vec<Inline>)->Block{
    Block::Styled{attributes:json!({"text":{"author":author}}),block:Box::new(Block::Caption{
        label:crate::cross_references::Label{id:"tbl:author-caption".into(),kind:"table".into(),number:7,prefix:"Table".into(),numbering:None},inlines:children,
    })}
}

#[test]
fn caption_label_number_title_and_overrides_use_physical_bp_with_links_intact(){
    let children=vec![leaf(" WEST "),authored(json!({"size":18.5}),vec![Inline::Strong(vec![Inline::Link{href:"https://example.test/caption".into(),title:String::new(),children:vec![leaf("INNER")]}])]),authored(json!({"cs":true}),vec![leaf("FORCED"),Inline::Code("CODE".into())])];
    let blocks=vec![caption(json!({"size":30,"sizeCs":10,"cs":false,"rtl":false}),children),Block::Paragraph(vec![leaf("AFTER")])];
    let doc=Document{blocks:blocks.clone(),..Default::default()};let options=LatexOptions{publication_style:Some(style()),bib_engine:String::new(),..Default::default()};
    let complete=render_document_with_geometry(&doc,&options,&|_|None).unwrap().0;
    assert!(complete.contains("{\\fontsize{30bp}{\\baselineskip}\\selectfont{}Table 7}"),"{complete}");
    assert!(complete.contains("{\\fontsize{30bp}{\\baselineskip}\\selectfont{} WEST }"));
    assert!(complete.contains("\\textbf{\\href{https://example.test/caption}{{\\fontsize{18.5bp}{\\baselineskip}\\selectfont{}INNER}}}"));
    assert!(complete.contains("{\\fontsize{10bp}{\\baselineskip}\\selectfont{}FORCED}"));
    assert!(complete.contains("\\texttt{{\\fontsize{10bp}{\\baselineskip}\\selectfont{}CODE}}"));
    assert!(complete.contains(&format!("\\phantomsection\\label{{{}}}",label_key("tbl:author-caption"))));
    assert!(body(blocks,&style()).0.ends_with("AFTER\n\n"));
    assert!(!has_unsupported_local_styles(&doc));
}

#[test]
fn caption_single_axes_and_false_flags_preserve_the_two_caption_sizes(){
    for (author,western,complex) in [(json!({"size":30}),30.,10.),(json!({"sizeCs":14.5}),10.,14.5)]{
        let (source,state)=body(vec![caption(author,vec![leaf("WEST"),authored(json!({"cs":true}),vec![leaf("COMPLEX")])])],&style());assert!(state.error.is_none(),"{:?}",state.error);
        for (value,size) in [("Table 7",western),("WEST",western),("COMPLEX",complex)]{assert!(source.contains(&format!("{{\\fontsize{{{size}bp}}{{\\baselineskip}}\\selectfont{{}}{value}}}")),"{source}");}
    }
    let (source,state)=body(vec![caption(json!({"size":30,"sizeCs":10,"cs":true,"rtl":false}),vec![leaf("COMPLEX"),authored(json!({"cs":false,"rtl":false}),vec![leaf("WESTERN")])])],&style());assert!(state.error.is_none());
    for value in ["Table 7","COMPLEX"]{assert!(source.contains(&format!("{{\\fontsize{{10bp}}{{\\baselineskip}}\\selectfont{{}}{value}}}")),"{source}");}
    assert!(source.contains("{\\fontsize{30bp}{\\baselineskip}\\selectfont{}WESTERN}"));
}

#[test]
fn caption_inline_base_resolves_to_authored_sizes_with_independent_local_inheritance(){
    let reset=|western:&str,complex:&str|authored(json!({"size":45,"sizeCs":18}),vec![
        leaf("OVERRIDE"),authored(json!({"size":"base","sizeCs":"base"}),vec![leaf(western),authored(json!({"cs":true}),vec![leaf(complex)])]),
    ]);
    let mut local=caption(json!({"size":30}),vec![reset("LOCALWEST","LOCALCS")]);let Block::Styled{attributes,..}=&mut local else{unreachable!()};attributes["text"]["size"]=json!(17.5);
    assert!(!has_unsupported_local_styles(&Document{blocks:vec![local.clone()],..Default::default()}));
    let (source,state)=body(vec![caption(json!({"size":30,"sizeCs":10}),vec![reset("BASEWEST","BASECS")]),local],&style());assert!(state.error.is_none(),"{:?}",state.error);
    for(value,size)in[("OVERRIDE",45.),("BASEWEST",30.),("BASECS",10.),("LOCALWEST",30.),("LOCALCS",17.5)]{assert!(source.contains(&format!("{{\\fontsize{{{size}bp}}{{\\baselineskip}}\\selectfont{{}}{value}}}")),"{source}");}
}

#[test]
fn caption_empty_author_keeps_legacy_tex_and_unrelated_style_warnings(){
    let marked=caption(json!({}),vec![leaf("UNCHANGED"),Inline::Code("code".into())]);
    let mut ordinary=marked.clone();let Block::Styled{attributes,..}=&mut ordinary else{unreachable!()};attributes["text"].as_object_mut().unwrap().remove("author");
    let (marked_source,marked_state)=body(vec![marked],&style());let (ordinary_source,ordinary_state)=body(vec![ordinary],&style());
    assert!(marked_state.error.is_none()&&ordinary_state.error.is_none());assert_eq!(marked_source,ordinary_source);
    let mut unrelated=caption(json!({"size":30}),vec![leaf("TITLE")]);let Block::Styled{attributes,..}=&mut unrelated else{unreachable!()};attributes["text"]["color"]=json!("#123456");
    assert!(has_unsupported_local_styles(&Document{blocks:vec![unrelated],..Default::default()}));
}

#[test]
fn caption_author_math_rtl_and_render_limit_remain_fallible_and_specific(){
    let options=LatexOptions{publication_style:Some(style()),bib_engine:String::new(),..Default::default()};
    for block in [caption(json!({"size":30}),vec![Inline::Math("x".into())]),caption(json!({"sizeCs":4096.5}),vec![leaf("LIMIT")]),caption(json!({"size":30,"rtl":true}),vec![authored(json!({"rtl":false}),vec![leaf("TITLE")])]),caption(json!({"size":30,"sizeCs":10,"rtl":false}),vec![leaf("العربية")])]{
        let error=render_document_with_geometry(&Document{blocks:vec![block],..Default::default()},&options,&|_|None).err().unwrap();assert!(error.starts_with(UNSUPPORTED),"{error}");
    }
    let (_,state)=body(vec![caption(json!({"size":30,"sizeCs":10,"rtl":false}),vec![leaf("العربية")])],&style());let error=state.error.unwrap();
    let details:Value=serde_json::from_str(error.strip_prefix(&format!("{UNSUPPORTED}: ")).unwrap()).unwrap();
    assert_eq!(details,json!({"consumer":"tex","reason":"rtl","coordinateSpace":"textUtf8Bytes","from":0,"to":"العربية".len()}));
}

#[test]
fn prepared_repeated_caption_titles_keep_author_sizes_and_ref_destinations(){
    let comment=crate::publication_blocks::comment(&json!({"id":"caption-source","text":{"author":{"size":30,"sizeCs":10}}})).unwrap();
    let raw=format!("| A |\n|---|\n| V |\n\n{comment}\n\n:label[tbl:first] RepeatedTitle\n\nRepeatedTitle\n\n| B |\n|---|\n| W |\n\n:label[tbl:second] RepeatedTitle\n\nSee :ref[tbl:first] and :ref[tbl:second].\n");
    let prepared=crate::publication::prepare(&raw,"").unwrap();let doc=crate::md_ast::parse_prepared(&raw,&prepared.projection).unwrap();
    let options=LatexOptions{publication_style:Some(style()),bib_engine:String::new(),..Default::default()};let source=render_document_with_geometry(&doc,&options,&|_|None).unwrap().0;
    assert_eq!(source.matches("{\\fontsize{30bp}{\\baselineskip}\\selectfont{}Table 1}").count(),1,"{source}");
    assert_eq!(source.matches("{\\fontsize{30bp}{\\baselineskip}\\selectfont{} — RepeatedTitle}").count(),1);
    assert!(source.contains("\nRepeatedTitle\n\n"));
    assert!(source.contains("\\begin{center}Table 2 — RepeatedTitle\\end{center}"));
    for id in ["tbl:first","tbl:second"]{assert!(source.contains(&format!("\\label{{{}}}",label_key(id))));assert!(source.contains(&format!("\\hyperref[{}]",label_key(id))));}
}

#[test]
fn caption_native_sizes_keep_the_projected_chapter_and_sequence_number(){
    for size in [10.5,11.,14.]{
        let mut block=caption(json!({"size":size,"sizeCs":10}),vec![leaf(" NATIVE")]);let Block::Styled{block:inner,..}=&mut block else{unreachable!()};let Block::Caption{label,..}=inner.as_mut()else{unreachable!()};
        label.number=4;label.numbering=Some(crate::cross_references::CaptionNumbering{format:"upperRoman".into(),start:1,ordinal:4,chapter:Some(crate::cross_references::ChapterNumbering{level:1,id:"chapter".into(),number:"2".into()}),separator:".".into(),label_separator:" ".into(),caption_separator:" — ".into(),sequence:Some("NativeTable".into())});
        let (source,state)=body(vec![block],&style());assert!(state.error.is_none(),"{:?}",state.error);
        for value in ["Table 2.IV"," NATIVE"]{assert!(source.contains(&format!("{{\\fontsize{{{size}bp}}{{\\baselineskip}}\\selectfont{{}}{value}}}")),"{source}");}
    }
}

fn section_note_document(number_format:&str,start:u32,restart:&str)->Document{
    let section=|id:&str|format!("<!-- readmd-section:v1 {} -->",json!({"id":id,"break":"continuous","footnotes":{"kinds":{"footnote":{"numberFormat":number_format,"start":start,"restart":restart}}}}));
    crate::md_ast::parse(&format!("{}\n\nFirst[^a].\n\n{}\n\nSecond[^b].\n\n[^a]: Alpha.\n\n[^b]: Beta.\n",section("first"),section("second")))
}

#[test]
fn new_section_note_numbering_has_a_tex_capability_gate_before_resources(){
    let options=LatexOptions{publication_style:Some(style()),bib_engine:String::new(),..Default::default()};let calls=std::cell::Cell::new(0);
    for(format,start,restart)in[("upperLetter",1,"continuous"),("decimal",27,"continuous"),("decimal",1,"section")]{
        let mut doc=section_note_document(format,start,restart);doc.blocks.insert(1,Block::Paragraph(vec![Inline::Image{src:"probe.png".into(),alt:"PROBE".into(),title:String::new()}]));
        let error=render_document_with_geometry(&doc,&options,&|_|{calls.set(calls.get()+1);None}).err().unwrap();assert!(error.starts_with(crate::publication_note_policies::UNSUPPORTED),"{error}");assert!(error.contains("sections"));
        assert!(validate_paragraphs(&doc,&options).unwrap_err().starts_with(crate::publication_note_policies::UNSUPPORTED));
        let legacy_body=render_body(&doc,&|_|{calls.set(calls.get()+1);None});assert!(legacy_body.starts_with("\\PackageError{ReadMD}"));assert!(!legacy_body.contains("PROBE"));
    }
    assert_eq!(calls.get(),0);
}

#[test]
fn default_section_numbering_and_the_legacy_tex_warning_contract_are_distinct(){
    let doc=section_note_document("decimal",1,"continuous");let options=LatexOptions{publication_style:Some(style()),bib_engine:String::new(),..Default::default()};
    let source=render_document_with_geometry(&doc,&options,&|_|None).unwrap().0;
    assert!(source.contains("\\footnote{Alpha.}")&&source.contains("\\footnote{Beta.}"),"{source}");
    let legacy=crate::md_ast::parse("Legacy[^a].\n\n[^a]: Old note.\n");let mut settings=style();settings["footnotes"]=json!({"numberFormat":"upperLetter","start":27,"restart":"section"});
    let options=LatexOptions{publication_style:Some(settings.clone()),bib_engine:String::new(),..Default::default()};assert!(render_document_with_geometry(&legacy,&options,&|_|None).is_ok());
    let warnings=crate::publication_layout::warnings("tex",&settings);assert!(warnings.iter().any(|warning|warning=="publication_layout_notes_unsupported"));assert!(warnings.iter().any(|warning|warning=="publication_note_section_restart_unsupported"));
    let mut doc=section_note_document("decimal",1,"continuous");
    for block in &mut doc.blocks{if let Block::FootnoteDef{label,blocks}=block{if label=="a"{*blocks=vec![Block::Paragraph(vec![authored(json!({"size":"base"}),vec![leaf("NOTEBASE")])])];}}}
    let mut settings=style();settings["footnotes"]=json!({"kinds":{"footnote":{"size":10.5,"sizeCs":10.5}}});
    let sections=crate::publication_sections::resolve(&doc,&settings).unwrap();
    let options=LatexOptions{publication_style:Some(settings),publication_sections:sections,bib_engine:String::new(),..Default::default()};
    let source=render_document_with_geometry(&doc,&options,&|_|None).unwrap().0;assert!(source.contains("\\footnote{{\\fontsize{10.5bp}{\\baselineskip}\\selectfont{}NOTEBASE}}"),"{source}");
}
