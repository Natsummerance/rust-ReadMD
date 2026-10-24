use super::*;
use serde_json::{json,Value};
use crate::publication_inline_text::AuthorTextStyle;

fn authored(attributes:Value,children:Vec<Inline>)->Inline{Inline::AuthorStyled{style:AuthorTextStyle::from_value(&attributes).unwrap(),children}}
fn leaf(text:&str)->Inline{Inline::Text(text.into())}
fn style()->Value{crate::export_styles::sanitize(&json!({"typography":{"size":11.25},"cover":{"enabled":false}}))}
fn output(blocks:Vec<Block>,settings:&Value)->String{render_chapters_with_options(&Document{blocks,..Default::default()},"none",&mut |_|None,settings).into_iter().map(|chapter|chapter.body).collect()}

#[test]
fn xhtml_uses_numeric_point_sizes_and_keeps_scripts_links_and_escaped_content(){
    let inline=authored(json!({"size":30,"sizeCs":10}),vec![leaf("Latin 中文 "),leaf("العربية"),authored(json!({"cs":true}),vec![Inline::Strong(vec![Inline::Link{href:"https://example.test/a?b=1&c=2".into(),title:String::new(),children:vec![leaf("FORCED")]}])]),Inline::Code("<script>&".into())]);
    let source=output(vec![Block::Paragraph(vec![inline])],&style());
    assert!(source.contains("<span style=\"font-size:30pt\">Latin 中文 </span>"),"{source}");
    assert!(source.contains("<span style=\"font-size:10pt\">العربية</span>"));assert!(!source.contains("dir=\"rtl\""));
    assert!(source.contains("<strong><a href=\"https://example.test/a?b=1&amp;c=2\"><span style=\"font-size:10pt\">FORCED</span></a></strong>"));
    assert!(source.contains("<code><span style=\"font-size:30pt\">&lt;script&gt;&amp;</span></code>"));
    assert!(!source.contains("data-readmd-text")&&!source.contains("<script>"));
    crate::math_document::parse_xml(&format!("<body>{source}</body>")).unwrap();
}

#[test]
fn nested_base_uses_exact_body_local_heading_and_table_contexts(){
    let reset=|name:&str|authored(json!({"size":30}),vec![authored(json!({"size":"base"}),vec![leaf(name)])]);
    let mut settings=style();settings["headings"]["h1"]["size"]=json!(24);settings["table"]["cellSize"]=json!(10.5);
    let source=output(vec![Block::Paragraph(vec![reset("BODY")]),Block::Styled{attributes:json!({"text":{"size":17.5}}),block:Box::new(Block::Paragraph(vec![reset("LOCAL")]))},Block::Heading{level:1,id:"h".into(),inlines:vec![reset("HEAD")],numbering:None},Block::Table{aligns:Vec::new(),header:vec![vec![reset("CELL")]],rows:Vec::new()}],&settings);
    for (size,name) in [("11.25","BODY"),("17.5","LOCAL"),("24","HEAD"),("10.5","CELL")]{assert!(source.contains(&format!("<span style=\"font-size:{size}pt\">{name}</span>")),"{source}");}
}

#[test]
fn note_base_and_repeated_reference_identity_survive_outer_author_sizes(){
    let mut settings=style();settings["footnotes"]["size"]=json!(9.5);
    let source=output(vec![Block::Paragraph(vec![authored(json!({"size":30}),vec![leaf("CALL"),Inline::FootnoteRef("n".into())]),Inline::FootnoteRef("n".into())]),Block::FootnoteDef{label:"n".into(),blocks:vec![Block::Paragraph(vec![authored(json!({"size":"base"}),vec![leaf("NOTE")])])]}],&settings);
    assert!(source.contains("<span style=\"font-size:9.5pt\">NOTE</span>"),"{source}");
    assert_eq!(source.matches("role=\"doc-noteref\"").count(),2);assert_eq!(source.matches("role=\"doc-footnote\"").count(),1);
    let root=crate::math_document::parse_xml(&format!("<body xmlns:epub=\"http://www.idpf.org/2007/ops\">{source}</body>")).unwrap();
    fn ids(node:&crate::math_document::XmlNode,out:&mut Vec<String>){if let Some(id)=node.attributes.get("id"){out.push(id.clone());}for child in node.children(){ids(child,out);}}
    let mut values=Vec::new();ids(&root,&mut values);let mut unique=values.clone();unique.sort();unique.dedup();assert_eq!(values.len(),unique.len());
}

#[test]
fn real_fallible_packaging_rejects_oversized_author_values(){
    let doc=Document{blocks:vec![Block::Paragraph(vec![authored(json!({"size":4096.5}),vec![leaf("visible")])])],..Default::default()};
    let request=crate::publication_navigation::Request::default();
    let error=render_chapters_with_navigation(&doc,"none",&mut |_|None,&style(),0,&request).err().unwrap();
    assert_eq!(error.code,crate::publication_inline_text::UNSUPPORTED);assert_eq!(error.path,"epub.authorText");
}

#[test]
fn unmarked_chapter_retains_the_existing_plain_projection(){
    assert_eq!(output(vec![Block::Paragraph(vec![leaf("PLAIN "),Inline::Code("code".into())])],&style()),"<p>PLAIN <code>code</code></p>\n");
}

#[test]
fn explicit_word_rtl_cannot_be_exported_as_a_css_embedding(){
    let doc=Document{blocks:vec![Block::Paragraph(vec![authored(json!({"size":30,"sizeCs":10,"rtl":true}),vec![leaf("Latin 123 العربية - 45")])])],..Default::default()};
    let request=crate::publication_navigation::Request::default();let error=render_chapters_with_navigation(&doc,"none",&mut |_|None,&style(),0,&request).err().unwrap();assert_eq!(error.code,crate::publication_inline_text::UNSUPPORTED);
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
    let doc=Document{blocks:vec![Block::Paragraph(vec![inline,leaf(" RESTORED")])],..Default::default()};
    let request=crate::publication_navigation::Request::default();
    let chapters=render_chapters_with_navigation(&doc,"none",&mut |_|None,&style(),0,&request).unwrap();
    let source=chapters.into_iter().map(|chapter|chapter.body).collect::<String>();
    assert!(source.contains("<span style=\"font-size:30pt\">CLEARED LATIN</span>"),"{source}");
    assert!(source.contains("<code><span style=\"font-size:30pt\">CODE</span></code>"));
    assert!(source.contains("href=\"https://example.test/clear-rtl\""));
    assert!(source.contains(" RESTORED"));assert!(!source.contains("dir=\"rtl\"")&&!source.contains("readmd-export-error"));
}

#[test]
fn effective_true_after_a_cleared_subtree_still_rejects_real_leaves_and_atoms(){
    for atom in [leaf("UNCLEARED"),Inline::Code("CODE".into()),Inline::SoftBreak,Inline::HardBreak,Inline::FootnoteRef("n".into()),Inline::Link{href:"https://example.test/fallback".into(),title:String::new(),children:Vec::new()}]{
        let inline=authored(json!({"rtl":true}),vec![transparent_containers(authored(json!({"rtl":false}),vec![leaf("CLEARED")])),atom]);
        let doc=Document{blocks:vec![Block::Paragraph(vec![inline])],..Default::default()};
        let request=crate::publication_navigation::Request::default();
        let error=render_chapters_with_navigation(&doc,"none",&mut |_|None,&style(),0,&request).err().unwrap();
        assert_eq!(error.code,crate::publication_inline_text::UNSUPPORTED);assert_eq!(error.path,"epub.authorText");
    }
}

fn caption(author:Value,children:Vec<Inline>)->Block{
    Block::Styled{attributes:json!({"text":{"author":author}}),block:Box::new(Block::Caption{
        label:crate::cross_references::Label{id:"tbl:author-caption".into(),kind:"table".into(),number:7,prefix:"Table".into(),numbering:None},inlines:children,
    })}
}

#[test]
fn caption_author_styles_label_number_title_and_inner_overrides_without_leaking(){
    let children=vec![leaf(" Western 中文 "),leaf("العربية"),authored(json!({"size":18.5}),vec![Inline::Strong(vec![Inline::Link{href:"https://example.test/caption?a=1&b=2".into(),title:String::new(),children:vec![leaf("INNER")]}])]),authored(json!({"cs":true}),vec![leaf("FORCED")]),Inline::Link{href:"https://example.test/fallback".into(),title:String::new(),children:Vec::new()},Inline::Image{src:"missing.png".into(),alt:"MISSING".into(),title:String::new()}];
    let doc=Document{blocks:vec![caption(json!({"size":30,"sizeCs":10,"cs":false,"rtl":false}),children),Block::Paragraph(vec![leaf("AFTER")])],..Default::default()};
    let request=crate::publication_navigation::Request::default();
    let source=render_chapters_with_navigation(&doc,"none",&mut |_|None,&style(),0,&request).unwrap().into_iter().map(|chapter|chapter.body).collect::<String>();
    assert!(source.contains(&format!("class=\"publication-caption\" id=\"{}\"",xml_id("tbl:author-caption"))),"{source}");
    assert!(source.contains("<span style=\"font-size:30pt\">Table 7</span>"));
    assert!(source.contains("<span style=\"font-size:30pt\"> Western 中文 </span>"));
    assert!(source.contains("<span style=\"font-size:10pt\">العربية</span>"));
    assert!(source.contains("<strong><a href=\"https://example.test/caption?a=1&amp;b=2\"><span style=\"font-size:18.5pt\">INNER</span></a></strong>"));
    assert!(source.contains("<span style=\"font-size:10pt\">FORCED</span>"));
    assert!(source.contains("<a href=\"https://example.test/fallback\"><span style=\"font-size:30pt\">https://example.test/fallback</span></a>"));
    assert!(source.contains("<span class=\"img-missing\"><span style=\"font-size:30pt\">[MISSING]</span></span>"));
    assert!(source.ends_with("<p>AFTER</p>\n"));
    crate::math_document::parse_xml(&format!("<body>{source}</body>")).unwrap();
}

#[test]
fn caption_single_axes_and_explicit_false_keep_the_caption_size_pair(){
    for (author,western,complex) in [(json!({"size":30}),30.,10.),(json!({"sizeCs":14.5}),10.,14.5)]{
        let source=output(vec![caption(author,vec![leaf("WEST"),leaf("العربية")])],&style());
        for (value,size) in [("Table 7",western),("WEST",western),("العربية",complex)]{assert!(source.contains(&format!("<span style=\"font-size:{size}pt\">{value}</span>")),"{source}");}
    }
    let source=output(vec![caption(json!({"size":30,"sizeCs":10,"cs":true,"rtl":false}),vec![leaf("COMPLEX"),authored(json!({"cs":false,"rtl":false}),vec![leaf("WESTERN")])])],&style());
    assert!(source.contains("<span style=\"font-size:10pt\">Table 7</span>"));
    assert!(source.contains("<span style=\"font-size:10pt\">COMPLEX</span>"));
    assert!(source.contains("<span style=\"font-size:30pt\">WESTERN</span>"));
}

#[test]
fn caption_inline_base_returns_to_its_authored_pair_and_local_missing_axis(){
    let reset=|western:&str,complex:&str|authored(json!({"size":45,"sizeCs":18}),vec![
        leaf("OVERRIDE"),authored(json!({"size":"base","sizeCs":"base"}),vec![leaf(western),authored(json!({"cs":true}),vec![leaf(complex)])]),
    ]);
    let mut local=caption(json!({"size":30}),vec![reset("LOCALWEST","LOCALCS")]);let Block::Styled{attributes,..}=&mut local else{unreachable!()};attributes["text"]["size"]=json!(17.5);
    let source=output(vec![caption(json!({"size":30,"sizeCs":10}),vec![reset("BASEWEST","BASECS")]),local],&style());
    for(value,size)in[("OVERRIDE",45.),("BASEWEST",30.),("BASECS",10.),("LOCALWEST",30.),("LOCALCS",17.5)]{assert!(source.contains(&format!("<span style=\"font-size:{size}pt\">{value}</span>")),"{source}");}
}

#[test]
fn caption_empty_author_is_byte_identical_to_existing_styled_caption_output(){
    let marked=caption(json!({}),vec![leaf("UNCHANGED"),Inline::Code("code".into())]);
    let mut ordinary=marked.clone();let Block::Styled{attributes,..}=&mut ordinary else{unreachable!()};attributes["text"].as_object_mut().unwrap().remove("author");
    assert_eq!(output(vec![marked],&style()),output(vec![ordinary],&style()));
}

#[test]
fn caption_author_math_direction_and_capacity_keep_fallible_diagnostics(){
    for block in [caption(json!({"size":30}),vec![Inline::Math("x".into())]),caption(json!({"sizeCs":4096.5}),vec![leaf("LIMIT")]),caption(json!({"size":30,"rtl":true}),vec![authored(json!({"rtl":false}),vec![leaf("TITLE")])])]{
        let doc=Document{blocks:vec![block],..Default::default()};let request=crate::publication_navigation::Request::default();
        let error=render_chapters_with_navigation(&doc,"none",&mut |_|None,&style(),0,&request).err().unwrap();
        assert_eq!(error.code,crate::publication_inline_text::UNSUPPORTED);assert_eq!(error.path,"epub.authorText");
    }
}

#[test]
fn prepared_repeated_caption_titles_keep_their_own_author_size_and_reference_identity(){
    let comment=crate::publication_blocks::comment(&json!({"id":"caption-source","text":{"author":{"size":30,"sizeCs":10}}})).unwrap();
    let raw=format!("| A |\n|---|\n| V |\n\n{comment}\n\n:label[tbl:first] RepeatedTitle\n\nRepeatedTitle\n\n| B |\n|---|\n| W |\n\n:label[tbl:second] RepeatedTitle\n\nSee :ref[tbl:first] and :ref[tbl:second].\n");
    let prepared=crate::publication::prepare(&raw,"").unwrap();let doc=crate::md_ast::parse_prepared(&raw,&prepared.projection).unwrap();
    let source=render_chapters_with_navigation(&doc,"none",&mut |_|None,&style(),0,&crate::publication_navigation::Request::default()).unwrap().into_iter().map(|chapter|chapter.body).collect::<String>();
    assert_eq!(source.matches("font-size:30pt\">Table 1</span>").count(),1,"{source}");
    assert_eq!(source.matches("font-size:30pt\"> — RepeatedTitle</span>").count(),1,"{source}");
    assert!(source.contains("<p>RepeatedTitle</p>"));assert!(source.contains(&format!("id=\"{}\">Table 2 — RepeatedTitle</p>",xml_id("tbl:second"))));
    assert!(source.contains(&format!("href=\"chapter_1.xhtml#{}\"",xml_id("tbl:first")))&&source.contains(&format!("href=\"chapter_1.xhtml#{}\"",xml_id("tbl:second"))));
}

#[test]
fn caption_native_sizes_keep_the_existing_chapter_and_sequence_number_projection(){
    for size in [10.5,11.,14.]{
        let mut block=caption(json!({"size":size,"sizeCs":10}),vec![leaf(" NATIVE")]);let Block::Styled{block:inner,..}=&mut block else{unreachable!()};let Block::Caption{label,..}=inner.as_mut()else{unreachable!()};
        label.number=4;label.numbering=Some(crate::cross_references::CaptionNumbering{format:"upperRoman".into(),start:1,ordinal:4,chapter:Some(crate::cross_references::ChapterNumbering{level:1,id:"chapter".into(),number:"2".into()}),separator:".".into(),label_separator:" ".into(),caption_separator:" — ".into(),sequence:Some("NativeTable".into())});
        let source=output(vec![block],&style());
        for value in ["Table 2.IV"," NATIVE"]{assert!(source.contains(&format!("<span style=\"font-size:{size}pt\">{value}</span>")),"{source}");}
    }
}

fn section_note_document(restart:&str,second_start:u32)->Document{
    let section=|id:&str,start|format!("<!-- readmd-section:v1 {} -->",json!({"id":id,"break":"continuous","footnotes":{"kinds":{"footnote":{"numberFormat":"upperLetter","start":start,"restart":restart}}}}));
    crate::md_ast::parse(&format!("{}\n\nFirst[^a].\n\n{}\n\nSecond[^b], again[^a].\n\n[^a]: Alpha.\n\n[^b]: Beta.\n",section("first",27),section("second",second_start)))
}

#[test]
fn uniform_section_notes_lower_into_real_xhtml_number_labels_and_stable_backlinks(){
    for(restart,aa,bb)in[("section",3,0),("continuous",2,1)]{
        let doc=section_note_document(restart,27);let settings=style();let before=settings.clone();
        let direct=render_chapters_with_options(&doc,"none",&mut |_|None,&settings);let fallible=render_chapters_with_navigation(&doc,"none",&mut |_|None,&settings,0,&crate::publication_navigation::Request::default()).unwrap();
        let source=direct.iter().map(|chapter|chapter.body.as_str()).collect::<String>();let checked=fallible.iter().map(|chapter|chapter.body.as_str()).collect::<String>();
        assert_eq!(source.matches("<sup>AA</sup>").count(),aa,"{source}");assert_eq!(source.matches("<sup>BB</sup>").count(),bb);
        assert_eq!(checked.matches("<sup>AA</sup>").count(),aa);assert_eq!(checked.matches("<sup>BB</sup>").count(),bb);
        assert!(source.contains("id=\"fn-1\"")&&source.contains("id=\"fn-2\"")&&source.contains("href=\"#fnref-1-2\""));
        assert_eq!(source.matches("Alpha.").count(),1);assert_eq!(source.matches("Beta.").count(),1);assert_eq!(settings,before);
    }
}

#[test]
fn differing_section_note_policies_fail_before_direct_or_fallible_image_callbacks(){
    let mut doc=section_note_document("section",28);doc.blocks.insert(1,Block::Paragraph(vec![Inline::Image{src:"probe.png".into(),alt:"PROBE".into(),title:String::new()}]));let calls=std::cell::Cell::new(0);let settings=style();
    assert!(render_chapters_with_options(&doc,"none",&mut |_|{calls.set(calls.get()+1);None},&settings).is_empty());
    let error=render_chapters_with_navigation(&doc,"none",&mut |_|{calls.set(calls.get()+1);None},&settings,0,&crate::publication_navigation::Request::default()).err().unwrap();
    assert_eq!(error.code,crate::publication_note_policies::UNSUPPORTED);assert!(error.path.contains("sections"));assert_eq!(calls.get(),0);
}
