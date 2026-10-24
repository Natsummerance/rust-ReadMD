use super::*;
use crate::md_ast::{Block,Document,Inline};

fn caption(attributes:Value,inlines:Vec<Inline>)->Block{
    Block::Styled{attributes,block:Box::new(Block::Caption{label:crate::cross_references::Label{id:"tbl:author".into(),kind:"table".into(),number:1,prefix:"Table".into(),numbering:None},inlines})}
}

#[test]
fn caption_author_budget_does_not_limit_ordinary_legacy_blocks(){
    let ordinary=Document{front_matter:None,blocks:vec![Block::Hr;MAX_NODES+1]};
    assert_eq!(validate_author_document(&ordinary,true),Ok(()));assert_eq!(validate_author_document(&ordinary,false),Ok(()));
    let too_many=Document{front_matter:None,blocks:vec![caption(json!({"text":{"author":{"size":11}}}),Vec::new());MAX_NODES+1]};
    assert_eq!(validate_author_document(&too_many,true),Err(crate::publication_inline_text::BUDGET));
    let long_caption=Document{front_matter:None,blocks:vec![caption(json!({"text":{"author":{"size":11}}}),vec![Inline::Text("x".into());MAX_NODES+1])]};
    assert_eq!(validate_author_document(&long_caption,true),Err(crate::publication_inline_text::BUDGET));
}

#[test]
fn caption_author_schema_target_math_and_inert_examples_are_checked(){
    for author in [json!({"size":"base","sizeCs":"base","cs":false,"rtl":false}),json!({"size":11}),json!({"sizeCs":12.5}),json!({"cs":false,"rtl":false})]{assert_eq!(validate_style(&json!({"text":{"author":author}})),Ok(()));}
    for author in [json!({"size":11.25}),json!({"size":0}),json!({"size":-1}),json!({"sizeCs":"inherit"}),json!({"cs":"false"}),json!({"future":1})]{assert!(validate_style(&json!({"text":{"author":author}})).is_err(),"{author}");}
    let source="| C |\n|---|\n| V |\n\n<!-- readmd:layout v1 {\"id\":\"caption-author\",\"text\":{\"author\":{\"size\":11}}} -->\n\n:label[tbl:author] Caption\n";
    assert_eq!(validate_author_consumer(source,true),Ok(()));assert_eq!(validate_author_consumer(source,false),Err(crate::publication_inline_text::UNSUPPORTED));
    let code=format!("```md\n{source}\n```\n");assert_eq!(validate_author_consumer(&code,false),Ok(()));
    let body=Document{front_matter:None,blocks:vec![Block::Styled{attributes:json!({"text":{"author":{"size":11}}}),block:Box::new(Block::Paragraph(vec![Inline::Text("Body".into())]))}]};assert_eq!(validate_author_document(&body,true),Err(crate::publication_inline_text::INVALID));
    let math=Document{front_matter:None,blocks:vec![caption(json!({"text":{"author":{"size":11}}}),vec![Inline::Math("x".into())])]};assert_eq!(validate_author_document(&math,true),Err(crate::publication_inline_text::UNSUPPORTED));
}

#[test]
fn prepared_caption_markers_bridge_exact_ranges_and_keep_duplicate_titles_separate(){
    let source="\u{feff}😀 Repeated title\r\n\r\n| C |\r\n|---|\r\n| V |\r\n\r\n<!-- readmd:layout v1 {\"id\":\"caption-first\",\"text\":{\"author\":{\"size\":30,\"sizeCs\":10}}} -->\r\n\r\n:label[tbl:first] Repeated title\r\n\r\n| C |\r\n|---|\r\n| V |\r\n\r\n<!-- readmd:layout v1 {\"id\":\"caption-second\",\"text\":{\"author\":{\"size\":18,\"sizeCs\":8}}} -->\r\n\r\n:label[tbl:second] Repeated title\r\n\r\nRepeated title\r\n";
    let before=source.as_bytes().to_vec();let original=analyze(source).unwrap();let projected=crate::cross_references::project(source);assert!(projected.diagnostics.is_empty(),"{:?}",projected.diagnostics);let prepared=analyze(&projected.content).unwrap();assert!(prepared.diagnostics.is_empty(),"{:?}",prepared.diagnostics);
    let authored=prepared.nodes.iter().filter(|node|node.style.pointer("/text/author").is_some()).collect::<Vec<_>>();assert_eq!(authored.len(),2);
    for(node,id,label,points)in[(authored[0],"caption-first","tbl:first",30.),(authored[1],"caption-second","tbl:second",18.)]{
        assert_eq!(node.id,id);assert_eq!(node.kind,"caption");assert_eq!(node.style["text"]["author"]["size"],points);assert!(node.editable);
        let raw=&projected.content[node.bytes.clone()];let end=raw.find("-->").unwrap()+3;assert_eq!(crate::cross_references::read_marker(&raw[..end]).unwrap().id,label);assert!(raw.contains(&format!("<span id=\"{label}\" class=\"publication-caption\"></span>")));assert!(!raw.contains("| V |"));assert_eq!(node.from,projected.content[..node.bytes.start].encode_utf16().count());assert_eq!(node.to,projected.content[..node.bytes.end].encode_utf16().count());
        let original=original.nodes.iter().find(|original|original.id==id).unwrap();assert_eq!(source[original.directive.clone().unwrap()],projected.content[node.directive.clone().unwrap()]);
    }
    assert_eq!(crate::md_ast::parse(source),crate::md_ast::parse(&projected.content));assert_eq!(source.as_bytes(),before);
}

#[test]
fn caption_render_direction_guard_keeps_native_and_flow_contracts_separate(){
    let native=Document{front_matter:None,blocks:vec![caption(json!({"text":{"author":{"size":30,"rtl":true}}}),vec![Inline::Text("123".into())])]};
    assert_eq!(validate_author_document(&native,true),Ok(()));assert_eq!(validate_author_render_document(&native),Err(crate::publication_inline_text::UNSUPPORTED));
    for author in [json!({}),json!({"size":30,"sizeCs":10,"rtl":false}),json!({"cs":false,"rtl":false})]{let doc=Document{front_matter:None,blocks:vec![caption(json!({"text":{"author":author}}),vec![Inline::Text("Text".into())])]};assert_eq!(validate_author_render_document(&doc),Ok(()));}
    let source="| C |\n|---|\n| V |\n\n<!-- readmd:layout v1 {\"id\":\"caption-rtl\",\"text\":{\"author\":{\"size\":30,\"rtl\":true}}} -->\n\n:label[tbl:rtl] 123\n";
    assert_eq!(validate_author_consumer(source,true),Ok(()));assert_eq!(validate_author_render_consumer(source),Err(crate::publication_inline_text::UNSUPPORTED));
    for inert in [format!("```md\n{source}\n```\n"),format!("<pre>\n{source}\n</pre>\n"),format!("<script>\n{source}\n</script>\n"),format!("<style>\n{source}\n</style>\n"),format!("<textarea>\n{source}\n</textarea>\n")]{
        let model=analyze(&inert).unwrap();assert!(model.diagnostics.is_empty(),"{inert}: {:?}",model.diagnostics);assert!(model.nodes.iter().all(|node|node.style["text"].get("author").is_none()));assert_eq!(validate_author_render_consumer(&inert),Ok(()));
    }
    // Ending a raw block must reactivate an independently introduced layout
    // comment, including its precise flow-only RTL rejection.
    let after=format!("<pre>\n{source}\n</pre>\n\n{source}");let model=analyze(&after).unwrap();assert!(model.diagnostics.is_empty(),"{:?}",model.diagnostics);assert_eq!(model.nodes.iter().filter(|node|node.style["text"].get("author").is_some()).count(),1);assert_eq!(validate_author_render_consumer(&after),Err(crate::publication_inline_text::UNSUPPORTED));
}
