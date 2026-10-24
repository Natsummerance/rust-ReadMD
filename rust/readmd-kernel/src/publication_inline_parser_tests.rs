use super::*;
use crate::md_ast::{self,Block,Inline};
use pulldown_cmark::CowStr;

fn prepared(source:&str)->md_ast::Document{md_ast::parse_prepared(source,&crate::publication_rich::Projection::new(source)).unwrap()}
fn authors(doc:&md_ast::Document)->usize{let mut count=0;md_ast::walk_inlines(&doc.blocks,&mut|node|if matches!(node,Inline::AuthorStyled{..}){count+=1;});count}

#[test]
fn normal_events_borrow_large_text_and_keep_real_bom_offsets(){
    for prefix in ["","\u{feff}"]{
        let source=format!("{prefix}{}","Ordinary UTF8 Σ content without author syntax ".repeat(4096));
        assert!(!has_owned_syntax(&source));let events=markdown_events(&source).unwrap();let texts=events.iter().filter_map(|(event,range)|if let Event::Text(value)=event{Some((value,range))}else{None}).collect::<Vec<_>>();assert!(!texts.is_empty());
        for(value,range)in texts{assert!(matches!(value,CowStr::Borrowed(_)),"normal text must borrow, rather than own another large allocation");assert_eq!(value.as_ref().as_ptr(),source[range.clone()].as_ptr());assert_eq!(value.as_ref(),&source[range.clone()]);assert!(range.start>=prefix.len());}
        let document=prepared(&source);assert_eq!(authors(&document),0);assert_eq!(document.blocks.len(),1);
        let blocks=crate::publication_blocks::analyze(&source).unwrap();assert_eq!(blocks.nodes.len(),1);assert_eq!(blocks.nodes[0].bytes.start,prefix.len());assert_eq!(blocks.nodes[0].from,prefix.encode_utf16().count());assert_eq!(blocks.nodes[0].digest,crate::document_transaction::revision(&source[blocks.nodes[0].bytes.clone()]));
    }
}

#[test]
fn uppercase_owned_first_span_stays_inline_with_and_without_bom(){
    for prefix in ["","\u{feff}"]{
        let source=format!("{prefix}<SPAN DATA-READMD-TEXT=\"v1\" DATA-SIZE-PT=\"30\">First Σ</SPAN>\n");assert!(has_owned_syntax(&source));
        let document=prepared(&source);assert_eq!(authors(&document),1);let Block::Paragraph(values)=&document.blocks[0]else{panic!("first owned span became HTML: {:?}",document.blocks)};assert_eq!(md_ast::inline_plain(values),"First Σ");
        let Inline::AuthorStyled{style,..}=&values[0]else{panic!("owned style missing")};assert_eq!(style.size,CascadeSize::Points(HalfPoints::new(60).unwrap()));
        let blocks=crate::publication_blocks::analyze(&source).unwrap();assert_eq!(blocks.nodes.len(),1);assert_eq!(blocks.nodes[0].kind,"paragraph");assert_eq!(blocks.nodes[0].bytes.start,prefix.len());assert_eq!(blocks.nodes[0].from,prefix.encode_utf16().count());
    }
}

#[test]
fn owned_marker_candidate_in_code_remains_an_inert_borrowed_example(){
    let example="<SPAN DATA-READMD-TEXT=\"v1\" DATA-SIZE-PT=\"30\">example</SPAN>";
    let source=format!("\u{feff}```html\n{example}\n```\n\nOrdinary paragraph.\n");assert!(has_owned_syntax(&source));let events=markdown_events(&source).unwrap();assert!(events.iter().any(|(event,_)|matches!(event,Event::Text(CowStr::Borrowed(value))if value.contains("DATA-READMD-TEXT"))));
    let document=prepared(&source);assert_eq!(authors(&document),0);assert!(matches!(&document.blocks[0],Block::Code{lang,text}if lang=="html"&&text.trim_end()==example));let blocks=crate::publication_blocks::analyze(&source).unwrap();assert!(blocks.diagnostics.is_empty());assert!(blocks.nodes.iter().any(|node|node.kind=="paragraph"&&source[node.bytes.clone()].contains("Ordinary paragraph")));
}

#[test]
fn casefold_candidate_detection_does_not_allocate_a_lowercase_source(){
    for value in ["data-readmd-text","DATA-READMD-TEXT","DaTa-ReAdMd-TeXt","prefix DATA-readmd-TEXT suffix"]{assert!(has_owned_syntax(value),"{value}");}
    for value in ["","normal Unicode Σ正文","data-readmd-tex","data-readmd text","data-readmd-texт"]{assert!(!has_owned_syntax(value),"{value}");}
}
