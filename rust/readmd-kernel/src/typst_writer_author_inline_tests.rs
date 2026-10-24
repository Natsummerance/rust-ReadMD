use super::*;
use serde_json::json;

fn authored(attributes:Value,children:Vec<Inline>)->Inline{Inline::AuthorStyled{style:AuthorTextStyle::from_value(&attributes).unwrap(),children}}
fn leaf(text:&str)->Inline{Inline::Text(text.into())}
fn output(blocks:Vec<Block>,options:Value)->Result<Source,String>{let dir=tempfile::tempdir().unwrap();render(&Document{blocks,..Default::default()},&options,"",dir.path(),"Author").map(|source|source)}
fn options()->Value{json!({"frontMatter":{"mode":"none"},"cover":{"enabled":false},"toc":{"enabled":false},"typography":{"size":11.25}})}

#[test]
fn native_text_segments_keep_independent_axes_and_escaping(){
    let source=output(vec![Block::Paragraph(vec![authored(json!({"size":30,"sizeCs":10}),vec![leaf("Latin 中文 "),leaf("العربية"),authored(json!({"cs":true}),vec![leaf("forced Latin")]),Inline::Code("#evil[{}]".into())])])],options()).unwrap().text;
    assert!(source.contains("#text(size:30pt)[#text(\"Latin 中文 \")]"),"{source}");
    assert!(source.contains("#text(size:10pt)[#text(\"العربية\")]"));assert!(!source.contains("dir:rtl"));
    assert!(source.contains("#text(size:10pt)[#text(\"forced Latin\")]"));
    assert!(source.contains("#text(size:30pt)[#{ show raw: set text(size:30pt); raw(\"#evil[{}]\") }]"));
}

#[test]
fn block_base_is_scoped_to_real_local_and_note_contexts(){
    let reset=||authored(json!({"size":30}),vec![leaf("BIG "),authored(json!({"size":"base"}),vec![leaf("RESET")])]);
    let blocks=vec![
        Block::Paragraph(vec![reset()]),
        Block::Styled{attributes:json!({"text":{"size":17.5}}),block:Box::new(Block::Paragraph(vec![reset()]))},
        Block::Paragraph(vec![leaf("Call "),Inline::FootnoteRef("n".into())]),
        Block::FootnoteDef{label:"n".into(),blocks:vec![Block::Paragraph(vec![reset()])]},
    ];
    let mut settings=options();settings["footnotes"]=json!({"size":9.5});
    let source=output(blocks,settings).unwrap().text;
    for size in ["11.25","17.5","9.5"]{assert!(source.contains(&format!("#text(size:{size}pt)[#text(\"RESET\")]")),"{source}");}
    assert_eq!(source.matches("#text(size:30pt)[#text(\"BIG \")]").count(),3);
}

#[test]
fn author_link_and_inline_code_have_identical_visible_and_metric_sizes(){
    let mut settings=options();settings["typography"]["lineSpacing"]=json!({"mode":"exact","pt":60});
    let inline=authored(json!({"size":30}),vec![Inline::Strong(vec![Inline::Link{href:"https://example.test/author".into(),title:String::new(),children:vec![leaf("LINK")]}]),Inline::Code("CODE".into())]);
    let source=output(vec![Block::Paragraph(vec![inline])],settings).unwrap().text;
    assert_eq!(source.matches("#text(size:30pt)[#text(\"LINK\")]").count(),2,"{source}");
    assert_eq!(source.matches("#text(size:30pt)[#{ show raw: set text(size:30pt); raw(\"CODE\") }]").count(),2);
    assert_eq!(source.matches("#link(\"https://example.test/author\")").count(),1);
}

#[test]
fn author_code_absolute_metrics_follow_explicit_axis_and_nested_block_base(){
    let source=output(vec![Block::Paragraph(vec![
        authored(json!({"size":10.5}),vec![Inline::Code("CODE105".into())]),
        authored(json!({"size":30}),vec![authored(json!({"size":"base"}),vec![Inline::Code("NESTEDBASE".into())])]),
        authored(json!({"size":30,"sizeCs":10.5}),vec![authored(json!({"cs":true}),vec![Inline::Code("FORCEDCS".into())])]),
        authored(json!({"sizeCs":30,"cs":true}),vec![authored(json!({"sizeCs":"base"}),vec![Inline::Code("CSBASE".into())])]),
    ])],options()).unwrap().text;
    for(marker,size)in[("CODE105","10.5"),("NESTEDBASE","11.25"),("FORCEDCS","10.5"),("CSBASE","11.25")]{
        assert!(source.contains(&format!("#text(size:{size}pt)[#{{ show raw: set text(size:{size}pt); raw(\"{marker}\") }}]")),"{source}");
    }
    assert_eq!(source.matches("show raw: set text(size:").count(),4);
}

#[test]
fn ordinary_empty_and_other_axis_code_retain_native_raw_relative_size(){
    let old=output(vec![Block::Paragraph(vec![Inline::Code("OLD".into())])],options()).unwrap().text;
    let source=output(vec![Block::Paragraph(vec![
        Inline::Code("OLD".into()),
        authored(json!({}),vec![Inline::Code("EMPTY".into())]),
        authored(json!({"sizeCs":30}),vec![Inline::Code("OTHERAXIS".into())]),
        authored(json!({"size":30,"cs":true}),vec![Inline::Code("INHERITEDCS".into())]),
    ])],options()).unwrap().text;
    assert!(old.contains("#raw(\"OLD\")"));assert!(source.contains("#raw(\"OLD\")"));
    for marker in ["EMPTY","OTHERAXIS","INHERITEDCS"]{
        assert!(source.contains(&format!("#text(size:11.25pt)[#raw(\"{marker}\")]")),"{source}");
    }
    assert!(!old.contains("show raw: set text(size:"));assert!(!source.contains("show raw: set text(size:"));
}

#[test]
fn headings_and_table_cells_reset_to_their_own_structural_base(){
    let reset=|name:&str|authored(json!({"size":30}),vec![authored(json!({"size":"base"}),vec![leaf(name)])]);
    let mut settings=options();settings["headings"]=json!({"h1":{"size":24}});settings["table"]=json!({"cellSize":10.5});
    let source=output(vec![Block::Heading{level:1,id:"h".into(),inlines:vec![reset("HEAD")],numbering:None},Block::Table{aligns:Vec::new(),header:vec![vec![reset("CELL")]],rows:Vec::new()}],settings).unwrap().text;
    assert!(source.contains("#text(size:24pt)[#text(\"HEAD\")]"));assert!(source.contains("#text(size:10.5pt)[#text(\"CELL\")]"));
}

#[test]
fn schema_sizes_above_consumer_capacity_are_rejected_without_clamping(){
    let result=output(vec![Block::Paragraph(vec![authored(json!({"sizeCs":4096.5}),vec![leaf("ASCII")])])],options());
    assert!(matches!(result,Err(error)if error==crate::publication_inline_text::UNSUPPORTED));
}

#[test]
fn explicit_word_rtl_is_not_approximated_by_a_bidi_embedding(){
    let result=output(vec![Block::Paragraph(vec![authored(json!({"size":30,"sizeCs":10,"rtl":true}),vec![leaf("Latin 123 العربية - 45")])])],options());
    assert!(matches!(result,Err(error)if error==crate::publication_inline_text::UNSUPPORTED));
}

fn transparent_containers(child:Inline)->Inline{
    let child=Inline::Link{href:"https://example.test/clear-rtl".into(),title:String::new(),children:vec![child]};
    let child=Inline::Citation{source:crate::publication_rich::SourceCitation{span:crate::publication_rich::SourceSpan::new("[@source]",0..9).unwrap(),items:Vec::new(),context:Default::default()},children:vec![child]};
    let child=Inline::Display{display:crate::publication_rich::RichDisplay::RightInline,children:vec![child]};
    let child=Inline::Styled{format:Default::default(),children:vec![child]};
    Inline::Strong(vec![Inline::Emph(vec![Inline::Strike(vec![child])])])
}

#[test]
fn nested_false_overrides_word_rtl_through_transparent_containers(){
    let inline=authored(json!({"size":30,"sizeCs":10,"rtl":true}),vec![
        authored(json!({"rtl":true}),Vec::new()),leaf(""),Inline::Code(String::new()),
        transparent_containers(authored(json!({"rtl":false}),vec![leaf("CLEARED LATIN"),Inline::Code("CODE".into())])),
    ]);
    let source=output(vec![Block::Paragraph(vec![inline,leaf(" RESTORED")])],options()).unwrap().text;
    assert!(source.contains("#text(size:30pt)[#text(\"CLEARED LATIN\")]"),"{source}");
    assert!(source.contains("#text(size:30pt)[#{ show raw: set text(size:30pt); raw(\"CODE\") }]"));
    assert!(source.contains("#link(\"https://example.test/clear-rtl\")"));
    assert!(source.contains("RESTORED"));assert!(!source.contains("dir:rtl"));
}

#[test]
fn effective_true_after_a_cleared_subtree_still_rejects_real_leaves_and_atoms(){
    for atom in [leaf("UNCLEARED"),Inline::Code("CODE".into()),Inline::SoftBreak,Inline::HardBreak,Inline::FootnoteRef("n".into())]{
        let inline=authored(json!({"rtl":true}),vec![transparent_containers(authored(json!({"rtl":false}),vec![leaf("CLEARED")])),atom]);
        let result=output(vec![Block::Paragraph(vec![inline])],options());
        assert!(matches!(result,Err(error)if error==crate::publication_inline_text::UNSUPPORTED));
    }
}

fn caption(author:Value,children:Vec<Inline>)->Block{
    Block::Styled{attributes:json!({"text":{"author":author}}),block:Box::new(Block::Caption{
        label:crate::cross_references::Label{id:"tbl:author-caption".into(),kind:"table".into(),number:7,prefix:"Table".into(),numbering:None},inlines:children,
    })}
}

#[test]
fn caption_label_number_and_title_use_itemized_author_sizes_and_inner_override(){
    let children=vec![leaf(" Western 中文 "),leaf("العربية"),authored(json!({"size":18.5}),vec![Inline::Strong(vec![Inline::Link{href:"https://example.test/caption".into(),title:String::new(),children:vec![leaf("INNER")]}])]),authored(json!({"cs":true}),vec![Inline::Code("FORCED".into())])];
    let source=output(vec![caption(json!({"size":30,"sizeCs":10,"cs":false,"rtl":false}),children),Block::Paragraph(vec![leaf("AFTER")])],options()).unwrap().text;
    assert!(source.contains("#text(size:30pt)[#text(\"Table 7\")]"),"{source}");
    assert!(source.contains("#text(size:30pt)[#text(\" Western 中文 \")]"));
    assert!(source.contains("#text(size:10pt)[#text(\"العربية\")]"));
    assert!(source.contains("#link(\"https://example.test/caption\")[#text(size:18.5pt)[#text(\"INNER\")]]"));
    assert!(source.contains("#text(size:10pt)[#{ show raw: set text(size:10pt); raw(\"FORCED\") }]"));
    assert!(source.contains(&format!("#label({})",string(&anchor("tbl:author-caption")))));
    assert!(source.contains("#par[#text(\"AFTER\")]"));
}

#[test]
fn caption_single_axes_and_false_cs_keep_independent_caption_inheritance(){
    for (author,western,complex) in [(json!({"size":30}),30.,10.),(json!({"sizeCs":14.5}),10.,14.5)]{
        let source=output(vec![caption(author,vec![leaf("WEST"),leaf("العربية")])],options()).unwrap().text;
        for (value,size) in [("Table 7",western),("WEST",western),("العربية",complex)]{assert!(source.contains(&format!("#text(size:{size}pt)[#text({})]",string(value))),"{source}");}
    }
    let source=output(vec![caption(json!({"size":30,"sizeCs":10,"cs":true,"rtl":false}),vec![leaf("COMPLEX"),authored(json!({"cs":false,"rtl":false}),vec![leaf("WESTERN")])])],options()).unwrap().text;
    for value in ["Table 7","COMPLEX"]{assert!(source.contains(&format!("#text(size:10pt)[#text({})]",string(value))),"{source}");}
    assert!(source.contains("#text(size:30pt)[#text(\"WESTERN\")]"));
}

#[test]
fn caption_inline_base_uses_the_authored_pair_and_legacy_local_missing_axis(){
    let reset=|western:&str,complex:&str|authored(json!({"size":45,"sizeCs":18}),vec![
        leaf("OVERRIDE"),authored(json!({"size":"base","sizeCs":"base"}),vec![Inline::Code(western.into()),authored(json!({"cs":true}),vec![Inline::Code(complex.into())])]),
    ]);
    let mut local=caption(json!({"size":30}),vec![reset("LOCALWEST","LOCALCS")]);let Block::Styled{attributes,..}=&mut local else{unreachable!()};attributes["text"]["size"]=json!(17.5);
    let source=output(vec![caption(json!({"size":30,"sizeCs":10}),vec![reset("BASEWEST","BASECS")]),local],options()).unwrap().text;
    assert!(source.contains("#text(size:45pt)[#text(\"OVERRIDE\")]"),"{source}");
    for(value,size)in[("BASEWEST",30.),("BASECS",10.),("LOCALWEST",30.),("LOCALCS",17.5)]{assert!(source.contains(&format!("#text(size:{size}pt)[#{{ show raw: set text(size:{size}pt); raw({}) }}]",string(value))),"{source}");}
}

#[test]
fn caption_empty_author_keeps_the_existing_native_caption_source(){
    let marked=caption(json!({}),vec![leaf("UNCHANGED"),Inline::Code("code".into())]);
    let mut ordinary=marked.clone();let Block::Styled{attributes,..}=&mut ordinary else{unreachable!()};attributes["text"].as_object_mut().unwrap().remove("author");
    assert_eq!(output(vec![marked],options()).unwrap().text,output(vec![ordinary],options()).unwrap().text);
}

#[test]
fn caption_author_math_direction_and_capacity_do_not_gain_unsupported_engine_claims(){
    for block in [caption(json!({"size":30}),vec![Inline::Math("x".into())]),caption(json!({"sizeCs":4096.5}),vec![leaf("LIMIT")]),caption(json!({"size":30,"rtl":true}),vec![authored(json!({"rtl":false}),vec![leaf("TITLE")])])]{
        let error=output(vec![block],options()).err().unwrap();assert_eq!(error,crate::publication_inline_text::UNSUPPORTED);
    }
}

#[test]
fn prepared_repeated_caption_titles_preserve_size_ownership_and_native_links(){
    let comment=crate::publication_blocks::comment(&json!({"id":"caption-source","text":{"author":{"size":30,"sizeCs":10}}})).unwrap();
    let raw=format!("| A |\n|---|\n| V |\n\n{comment}\n\n:label[tbl:first] RepeatedTitle\n\nRepeatedTitle\n\n| B |\n|---|\n| W |\n\n:label[tbl:second] RepeatedTitle\n\nSee :ref[tbl:first] and :ref[tbl:second].\n");
    let prepared=crate::publication::prepare(&raw,"").unwrap();let doc=crate::md_ast::parse_prepared(&raw,&prepared.projection).unwrap();let dir=tempfile::tempdir().unwrap();
    let source=render(&doc,&options(),"",dir.path(),"Caption").unwrap().text;
    assert_eq!(source.matches("#text(size:30pt)[#text(\"Table 1\")]").count(),1,"{source}");
    assert_eq!(source.matches("#text(size:30pt)[#text(\" — RepeatedTitle\")]").count(),1);
    assert!(source.contains("#par[#text(\"RepeatedTitle\")]"));
    for id in ["tbl:first","tbl:second"]{assert!(source.contains(&format!("#label({})",string(&anchor(id)))));assert!(source.contains(&format!("#link(label({}))",string(&anchor(id)))));}
    assert!(source.contains("#text(\"Table 2\") #text(\" — RepeatedTitle\")"));
}

#[test]
fn caption_native_sizes_keep_chapter_and_sequence_numbers_without_native_counter_replacement(){
    for size in [10.5,11.,14.]{
        let mut block=caption(json!({"size":size,"sizeCs":10}),vec![leaf(" NATIVE")]);let Block::Styled{block:inner,..}=&mut block else{unreachable!()};let Block::Caption{label,..}=inner.as_mut()else{unreachable!()};
        label.number=4;label.numbering=Some(crate::cross_references::CaptionNumbering{format:"upperRoman".into(),start:1,ordinal:4,chapter:Some(crate::cross_references::ChapterNumbering{level:1,id:"chapter".into(),number:"2".into()}),separator:".".into(),label_separator:" ".into(),caption_separator:" — ".into(),sequence:Some("NativeTable".into())});
        let source=output(vec![block],options()).unwrap().text;
        for value in ["Table 2.IV"," NATIVE"]{assert!(source.contains(&format!("#text(size:{size}pt)[#text({})]",string(value))),"{source}");}
    }
}

fn section_note_document(restart:&str,second_start:u32)->Document{
    let section=|id:&str,start|format!("<!-- readmd-section:v1 {} -->",json!({"id":id,"break":"continuous","footnotes":{"kinds":{"footnote":{"numberFormat":"upperLetter","start":start,"restart":restart}}}}));
    crate::md_ast::parse(&format!("{}\n\nFirst[^a].\n\n{}\n\nSecond[^b], again[^a].\n\n[^a]: Alpha.\n\n[^b]: Beta.\n",section("first",27),section("second",second_start)))
}

#[test]
fn uniform_section_notes_feed_real_typst_counter_updates_and_repeated_note_identity(){
    for(restart,updates)in[("section",3),("continuous",1)]{
        let doc=section_note_document(restart,27);let settings=options();let before=settings.clone();let dir=tempfile::tempdir().unwrap();
        let source=render(&doc,&settings,"",dir.path(),"Notes").unwrap().text;
        assert_eq!(source.matches("#counter(footnote).update(26)").count(),updates,"{source}");
        assert!(source.contains(&format!("#set footnote(numbering:{})",number_format("upperLetter"))));
        assert_eq!(source.matches("#footnote[").count(),2);assert_eq!(source.matches("#footnote(label(").count(),1);
        assert_eq!(source.matches("#text(\"Alpha.\")").count(),1);assert_eq!(source.matches("#text(\"Beta.\")").count(),1);
        assert!(source.contains(&format!("#footnote(label({}))",string(&private_anchor("note","a")))));assert_eq!(settings,before);
    }
}

#[test]
fn differing_section_note_numbering_refuses_before_typst_resource_staging(){
    let mut doc=section_note_document("section",28);doc.blocks.insert(1,Block::Paragraph(vec![Inline::Image{src:"probe.png".into(),alt:"PROBE".into(),title:String::new()}]));let root=tempfile::tempdir().unwrap();let staging=root.path().join("uncreated");
    let error=render(&doc,&options(),"",&staging,"Notes").err().unwrap();assert!(error.starts_with(crate::publication_note_policies::UNSUPPORTED),"{error}");assert!(error.contains("sections"));assert!(!staging.exists());
}
