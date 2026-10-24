use super::*;
use crate::md_ast::{self, Align, ListItem};

fn style(value:Value)->AuthorTextStyle{AuthorTextStyle::from_value(&value).unwrap()}
fn prepared(source:&str)->crate::ApiResult<md_ast::Document>{md_ast::parse_prepared(source,&crate::publication_rich::Projection::new(source))}
fn coords(source:&str,needle:&str)->(usize,usize){let start=source.find(needle).unwrap();(utf16(source,start),utf16(source,start+needle.len()))}
fn invert(result:&TransactionResult)->String{tx::apply(&result.content,&ContentTransaction{base_revision:result.revision.clone(),origin:"editor".into(),edits:result.inverse.clone()}).unwrap().content}
fn count_authors(blocks:&[Block])->usize{let mut count=0;md_ast::walk_inlines(blocks,&mut|node|if matches!(node,Inline::AuthorStyled{..}){count+=1;});count}

#[test]
fn half_points_keep_the_native_domain_without_ui_clamps(){
    for half in [1,2,19,60,145,i32::MAX as u32]{let value=HalfPoints::new(half).unwrap();assert_eq!(value.half_points(),half);assert_eq!(HalfPoints::from_pt(value.to_pt()).unwrap(),value);assert_eq!(serde_json::from_value::<HalfPoints>(json!(value)).unwrap(),value);}
    for half in [0,i32::MAX as u32+1,u32::MAX]{assert_eq!(HalfPoints::new(half),Err(INVALID));}
    for invalid in [0.,-0.5,0.1,30.25,f64::INFINITY,f64::NEG_INFINITY,f64::NAN,i32::MAX as f64]{assert_eq!(HalfPoints::from_pt(invalid),Err(INVALID));}
    assert_eq!(json!(HalfPoints::new(40).unwrap()),json!(20));
    assert_eq!(json!(HalfPoints::new(41).unwrap()),json!(20.5));
}

#[test]
fn author_json_is_sparse_typed_and_separate_from_csl(){
    let value=json!({"size":30,"sizeCs":"base","cs":false,"rtl":true});let parsed=style(value.clone());assert_eq!(parsed.to_value(),value);
    assert_eq!(style(json!({"size":null,"sizeCs":null,"cs":null,"rtl":null})),AuthorTextStyle::default());
    for value in [json!([]),Value::Null,json!({"size":"30"}),json!({"size":30.25}),json!({"sizeCs":0}),json!({"cs":0}),json!({"rtl":"false"}),json!({"bold":true}),json!({"size_cs":10})]{assert_eq!(AuthorTextStyle::from_value(&value),Err(INVALID),"{value}");}
    assert!(serde_json::from_str::<AuthorTextStyle>(r#"{"size":10,"size":20}"#).is_err());
    assert!(serde_json::from_str::<AuthorTextStyle>(r#"{"cs":true,"cs":false}"#).is_err());
    for raw in ["[]","[null,null,null,null]","null","true","30"]{assert!(serde_json::from_str::<AuthorTextStyle>(raw).is_err(),"{raw}");}
    assert!(serde_json::from_value::<crate::publication_rich::RichFormat>(json!({"size":30})).is_err());
}

#[test]
fn cascade_axes_origins_and_explicit_false_are_independent(){
    let base=ResolvedTextStyle::base(11.2,9.3);let outer=style(json!({"size":30,"sizeCs":10,"cs":true,"rtl":true})).resolve(&base,&base);
    let inner=style(json!({"size":"base","cs":false})).resolve(&outer,&base);
    assert_eq!((inner.size,inner.size_cs),(11.2,10.));assert_eq!((inner.size_origin,inner.size_cs_origin),(SizeOrigin::BlockBase,SizeOrigin::Author));
    assert_eq!((inner.cs,inner.rtl),(Some(false),Some(true)));
    let inherited=AuthorTextStyle::default().resolve(&inner,&base);assert_eq!(inherited,inner);
    let other=style(json!({"sizeCs":"base","rtl":false})).resolve(&outer,&base);
    assert_eq!((other.size,other.size_cs),(30.,9.3));assert_eq!((other.size_origin,other.size_cs_origin),(SizeOrigin::Author,SizeOrigin::BlockBase));assert_eq!(other.rtl,Some(false));
    assert!(!SizeOrigin::Inherited.is_explicit());assert!(SizeOrigin::Author.is_explicit());assert!(SizeOrigin::BlockBase.is_explicit());
}

#[test]
fn canonical_source_and_strict_reserved_attributes(){
    let value=style(json!({"size":30,"sizeCs":10.5,"cs":true,"rtl":false}));let tag=value.open_tag();
    assert_eq!(tag,"<span data-readmd-text=\"v1\" data-size-pt=\"30\" data-size-cs-pt=\"10.5\" data-cs=\"true\" data-rtl=\"false\">");assert_eq!(read_open_tag(&tag).unwrap(),Some(value));
    assert_eq!(AuthorTextStyle::default().wrap_source("body"),"body");
    for tag in ["<span data-readmd-text=\"v2\">","<span data-readmd-text=\"v1\" class=\"a\">","<span data-readmd-text=\"v1\" data-size-pt=\"0\">","<span data-readmd-text=\"v1\" data-size-pt=\"10.1\">","<span data-readmd-text=\"v1\" data-size-pt=\"NaN\">","<span data-readmd-text=\"v1\" data-cs=\"True\">","<span data-readmd-text=\"v1\" data-rtl=\"0\">","<span data-readmd-text=\"v1\" DATA-CS=\"true\" data-cs=\"false\">","<span data-readmd-text=\"v1\"/>","<b data-readmd-text=\"v1\">","<span data-readmd-text=v1>","<span data-readmd-text=\"v1\"data-cs=\"true\">"]{assert_eq!(read_open_tag(tag),Err(INVALID),"{tag}");}
    assert_eq!(read_open_tag("<span class=ordinary>").unwrap(),None);
}

#[test]
fn paragraph_first_span_is_live_without_an_authored_prefix(){
    let source="<span data-readmd-text=\"v1\" data-size-pt=\"30\">**Strong** [link](https://example.test)</span>\n";
    let doc=prepared(source).unwrap();assert_eq!(count_authors(&doc.blocks),1);
    let Block::Paragraph(values)=doc.blocks[0].unstyled()else{panic!("first run must stay a paragraph: {:?}",doc.blocks)};
    assert_eq!(md_ast::inline_plain(values),"Strong link");let Inline::AuthorStyled{style,children}=&values[0]else{panic!("missing author node")};assert_eq!(style.size,CascadeSize::Points(HalfPoints::new(60).unwrap()));assert!(matches!(children[0],Inline::Strong(_)));assert!(children.iter().any(|node|matches!(node,Inline::Link{href,..}if href=="https://example.test")));
    let nodes=get(source).unwrap().nodes;let span=nodes.iter().find(|node|node.kind=="span").unwrap();assert_eq!(span.bytes,0..source.trim_end().len());assert_eq!(span.content_bytes.start,source.find('>').unwrap()+1);
    assert!(nodes.iter().filter(|node|node.kind=="text").all(|node|!node.bytes.is_empty()));
}

#[test]
fn temporary_paragraph_prefix_never_erases_authored_word_joiners_or_bom(){
    let source="\u{feff}<span data-readmd-text=\"v1\" data-size-pt=\"30\">\u{2060}A\u{2060}</span>\r\n\r\n<span data-readmd-text=\"v1\" data-size-cs-pt=\"10\">B</span>\r\n";
    let events=markdown_events(source).unwrap();let text=events.iter().filter_map(|(event,_)|if let Event::Text(value)=event{Some(value.as_ref())}else{None}).collect::<String>();assert_eq!(text,"\u{2060}A\u{2060}B");
    for(event,range)in &events{assert!(source.get(range.clone()).is_some());if matches!(event,Event::Text(_)){assert!(!range.is_empty());}}
    let nodes=get(source).unwrap().nodes;assert_eq!(nodes.iter().find(|node|node.kind=="span").unwrap().from,1);
    let leaf=nodes.iter().find(|node|node.kind=="text"&&source[node.bytes.clone()].contains('A')).unwrap();assert_eq!(&source[leaf.bytes.clone()],"\u{2060}A\u{2060}");assert_eq!(leaf.to-leaf.from,3);
    let doc=prepared(source).unwrap();assert_eq!(count_authors(&doc.blocks),2);let mut body=String::new();md_ast::walk_inlines(&doc.blocks,&mut|node|if let Inline::Text(value)=node{body.push_str(value)});assert_eq!(body,"\u{2060}A\u{2060}B");
}

#[test]
fn nested_ordinary_spans_do_not_steal_owned_closing_tags(){
    let source="P <span data-readmd-text=\"v1\" data-size-pt=\"30\">A <span class=ordinary>B</span> <span data-readmd-text=\"v1\" data-size-cs-pt=\"10\">C</span>D</span>.";
    let document=get(source).unwrap();let spans=document.nodes.iter().filter(|node|node.kind=="span").collect::<Vec<_>>();assert_eq!(spans.len(),2);assert_eq!(spans[1].parent.as_deref(),Some(spans[0].id.as_str()));
    let doc=prepared(source).unwrap();assert_eq!(count_authors(&doc.blocks),2);let Block::Paragraph(values)=&doc.blocks[0]else{panic!()};assert_eq!(md_ast::inline_plain(values),"P A B CD.");
}

#[test]
fn code_comments_html_and_escaped_examples_cannot_authorize_styles(){
    let marker="<span data-readmd-text=\"v1\" data-size-pt=\"bad\">example</span>";
    for source in [format!("```html\n{marker}\n```"),format!("`{marker}`"),format!("<!-- {marker} -->"),format!("<pre>\n{marker}\n</pre>"),format!("<div>\n{marker}\n</div>"),format!("\\{marker}")]{
        assert!(validate_source(&source).is_ok(),"{source}");assert!(!get(&source).unwrap().nodes.iter().any(|node|node.kind=="span"),"{source}");assert_eq!(count_authors(&prepared(&source).unwrap().blocks),0,"{source}");
    }
}

#[test]
fn source_spans_cannot_cross_block_or_markdown_format_ownership(){
    for source in ["<span data-readmd-text=\"v1\" data-size-pt=\"30\">one\n\ntwo</span>","A **<span data-readmd-text=\"v1\" data-size-pt=\"30\">one** two</span>","A <span data-readmd-text=\"v1\">one", "A <span data-readmd-text=\"v1\" data-size-pt=\"30\"", "| A | B |\n|---|---|\n| <span data-readmd-text=\"v1\">one | two</span> |"]{assert_eq!(validate_source(source),Err(INVALID),"{source}");assert!(prepared(source).is_err(),"fallible export must reject: {source}");}
    assert_eq!(validate_source("A ![<span data-readmd-text=\"v1\">alt</span>](x.png)"),Err(UNSUPPORTED));
}

#[test]
fn oversized_multibyte_marker_returns_budget_without_slicing_a_scalar(){
    let source=format!("<span data-readmd-text=\"v1\" data-size-pt=\"{}\">body</span>","字".repeat(MAX_MARKER_BYTES));assert_eq!(validate_source(&source),Err(BUDGET));
    let mut nested=String::new();for _ in 0..=MAX_DEPTH{nested.push_str("<span data-readmd-text=\"v1\" data-size-pt=\"30\">");}nested.push('x');for _ in 0..=MAX_DEPTH{nested.push_str("</span>");}assert_eq!(validate_source(&nested),Err(BUDGET));
}

#[test]
fn malformed_live_multibyte_openers_never_slice_or_lose_late_markers(){
    for source in [format!("A <span data-readmd-text=bad {}","字".repeat(2000)),format!("A <span {} data-readmd-text=bad","字".repeat(2000))]{assert_eq!(std::panic::catch_unwind(||validate_source(&source)).expect("live malformed UTF-8 must not panic"),Err(BUDGET));}
    assert_eq!(validate_source(&format!("A <span data-readmd-text=bad {}","字".repeat(12))),Err(INVALID));
}

#[test]
fn malformed_scan_keeps_later_code_markers_inert_on_a_crowded_long_line(){
    let crowded=format!("A {} END\r\n\r\nTAIL_TARGET.\r\n","<span ".repeat(MAX_NODES/4));
    for example in ["", "\r\n```html\r\n<SPAN DATA-READMD-TEXT=bad>INERT</SPAN>\r\n```\r\n"]{
        let source=format!("{crowded}{example}");
        assert!(validate_source(&source).is_ok());
        assert!(!get(&source).unwrap().nodes.iter().any(|node|node.kind=="span"));
        let(from,to)=coords(&source,"TAIL_TARGET");let result=apply(&source,&tx::revision(&source),from,to,&json!({"size":30})).unwrap();
        assert_eq!(invert(&result.result),source);
        let spans=get(&result.result.content).unwrap().nodes.into_iter().filter(|node|node.kind=="span").collect::<Vec<_>>();
        assert_eq!(spans.len(),1);assert_eq!(&result.result.content[spans[0].content_bytes.clone()],"TAIL_TARGET");
    }
}

#[test]
fn malformed_scan_finds_late_reserved_markers_and_keeps_bom_utf8_live_ranges(){
    let late=format!("\u{feff}文 {}字 DATA-READMD-TEXT=bad","<SPAN ".repeat(MAX_MARKER_BYTES/2));
    assert_eq!(std::panic::catch_unwind(||get(&late)).expect("late UTF-8 marker must not panic").unwrap_err(),BUDGET);
    let short="\u{feff}文 <SPAN 字 DATA-READMD-TEXT=bad";
    assert_eq!(std::panic::catch_unwind(||get(short)).expect("short UTF-8 marker must not panic").unwrap_err(),INVALID);
    let source="\u{feff}文 \\<span data-readmd-text=bad\r\n\r\n<SPAN DATA-READMD-TEXT='v1' data-size-pt='30'>𝛂字</SPAN>\r\n";
    let nodes=get(source).unwrap().nodes;let spans=nodes.iter().filter(|node|node.kind=="span").collect::<Vec<_>>();assert_eq!(spans.len(),1);
    let span=spans[0];assert_eq!(span.bytes.start,source.find("<SPAN").unwrap());assert_eq!(span.from,utf16(source,span.bytes.start));
    assert_eq!(&source[span.content_bytes.clone()],"𝛂字");assert_eq!(span.content_to-span.content_from,3);
    for node in nodes{assert!(source.get(node.bytes).is_some());}
}

#[test]
fn ast_walk_plain_and_json_preserve_author_children_in_every_container(){
    let styled=Inline::AuthorStyled{style:style(json!({"size":30,"sizeCs":10,"rtl":false})),children:vec![Inline::Strong(vec![Inline::Text("text".into())]),Inline::Link{href:"#same".into(),title:String::new(),children:vec![Inline::Text("jump".into())]}]};
    let blocks=vec![Block::Styled{attributes:json!({"text":{"size":15}}),block:Box::new(Block::Paragraph(vec![styled.clone()]))},Block::Quote(vec![Block::Paragraph(vec![styled.clone()])]),Block::List{ordered:false,start:1,items:vec![ListItem{task:None,blocks:vec![Block::Paragraph(vec![styled.clone()])]}]},Block::Table{aligns:vec![Align::Left],header:vec![vec![styled.clone()]],rows:vec![vec![vec![styled.clone()]]]},Block::FootnoteDef{label:"same".into(),blocks:vec![Block::Paragraph(vec![styled])]}];
    assert_eq!(count_authors(&blocks),6);let value=md_ast::to_render_json(&blocks);let serialized=value.to_string();assert_eq!(serialized.matches("\"author-styled\"").count(),6);assert!(serialized.contains("\"sizeCs\":10"));assert!(serialized.contains("\"publicationNoteLabel\":\"same\""));
    for block in &blocks{if let Block::Paragraph(values)=block.unstyled(){assert_eq!(md_ast::inline_plain(values),"textjump");}}
}

#[test]
fn every_author_note_keeps_its_own_nested_story_base_label(){
    let source="Body[^first][^second][^plain].\n\n[^first]: <span data-readmd-text=\"v1\" data-size-pt=\"30\">one</span>\n\n    > <span data-readmd-text=\"v1\" data-size-cs-pt=\"10\">quote</span>\n\n[^second]: <span data-readmd-text=\"v1\" data-size-cs-pt=\"9\">two</span>\n\n    - nested\n\n[^plain]: independent plain note\n";
    let doc=prepared(source).unwrap();assert_eq!(count_authors(&doc.blocks),3);let values=md_ast::to_render_json(&doc.blocks);let notes=values.as_array().unwrap().iter().filter(|value|value.get("publicationNoteLabel").is_some()).collect::<Vec<_>>();assert!(notes.iter().any(|value|value["publicationNoteLabel"]=="first"));assert!(notes.iter().any(|value|value["publicationNoteLabel"]=="second"));assert!(notes.iter().any(|value|value["publicationNoteLabel"]=="plain"));
    fn verify(value:&Value,label:&str){match value{Value::Object(object)=>{if object.contains_key("type"){assert_eq!(object["publicationNoteLabel"],label);}for value in object.values(){verify(value,label)}},Value::Array(values)=>for value in values{verify(value,label)},_=>()}}
    for value in notes{verify(value,value["publicationNoteLabel"].as_str().unwrap());}
    let legacy=md_ast::to_render_json(&md_ast::parse("Plain\n\n[^n]: old note").blocks);assert!(!legacy.to_string().contains("publicationContext"));assert!(!legacy.to_string().contains("publicationNoteLabel"));
}

#[test]
fn repeated_text_targets_use_source_identity_and_edits_are_invertible(){
    let source="\u{feff}---\r\ntitle: Preserve\r\n---\r\n\r\nRepeat 𝛂 and Repeat. [link](https://example.test/a?b=1)\r\n";let second=source.rfind("Repeat").unwrap();let from=utf16(source,second);let to=utf16(source,second+6);
    let result=apply(source,&tx::revision(source),from,to,&json!({"size":30,"sizeCs":10,"cs":false})).unwrap();assert!(result.result.changed);assert_eq!(result.transaction.edits.len(),1);assert_eq!(invert(&result.result),source);assert!(result.result.content.starts_with("\u{feff}---\r\ntitle: Preserve\r\n---\r\n\r\nRepeat 𝛂 and "));assert!(result.result.content.ends_with(". [link](https://example.test/a?b=1)\r\n"));
    let spans=get(&result.result.content).unwrap().nodes.into_iter().filter(|node|node.kind=="span").collect::<Vec<_>>();assert_eq!(spans.len(),1);assert_eq!(&result.result.content[spans[0].content_bytes.clone()],"Repeat");
}

#[test]
fn applying_to_a_whole_formatted_link_preserves_its_href_and_heading_identity(){
    let source="# Same heading\n\n**[same](#same-heading)** and note[^n].\n\n[^n]: body\n";let(from,to)=coords(source,"**[same](#same-heading)**");let result=apply(source,&tx::revision(source),from,to,&json!({"sizeCs":10,"rtl":false})).unwrap();assert_eq!(invert(&result.result),source);assert_eq!(semantic_source(source),semantic_source(&result.result.content));
    let doc=prepared(&result.result.content).unwrap();let Block::Heading{id,..}=&doc.blocks[0]else{panic!()};assert_eq!(id,"same-heading");let mut links=Vec::new();let mut refs=Vec::new();md_ast::walk_inlines(&doc.blocks,&mut|node|match node{Inline::Link{href,..}=>links.push(href.clone()),Inline::FootnoteRef(label)=>refs.push(label.clone()),_=>()});assert_eq!(links,vec!["#same-heading"]);assert_eq!(refs,vec!["n"]);
}

#[test]
fn exact_span_patch_is_sparse_and_noop_keeps_original_source_bytes(){
    let source="A <SPAN DATA-READMD-TEXT='v1' data-size-pt='30' data-size-cs-pt='10' data-cs='false'>word</SPAN>.\r\n";let span=get(source).unwrap().nodes.into_iter().find(|node|node.kind=="span").unwrap();let no_op=apply(source,&tx::revision(source),span.from,span.to,&json!({"size":30})).unwrap();assert!(!no_op.result.changed);assert!(no_op.transaction.edits.is_empty());assert_eq!(no_op.result.content,source);
    let edited=apply(source,&tx::revision(source),span.content_from,span.content_to,&json!({"size":null})).unwrap();let current=get(&edited.result.content).unwrap().nodes.into_iter().find(|node|node.kind=="span").unwrap();assert_eq!(current.style,style(json!({"sizeCs":10,"cs":false})));assert_eq!(invert(&edited.result),source);
    let removed=clear(&edited.result.content,&edited.result.revision,current.from,current.to,&["sizeCs","cs"],false).unwrap();assert_eq!(removed.result.content,"A word.\r\n");
}

#[test]
fn partial_clear_splits_only_the_selected_plain_source_range(){
    let source="<span data-readmd-text=\"v1\" data-size-pt=\"30\" data-size-cs-pt=\"10\" data-rtl=\"false\">left middle right</span>";let(from,to)=coords(source,"middle");let edited=clear(source,&tx::revision(source),from,to,&["size"],false).unwrap();assert_eq!(invert(&edited.result),source);let spans=get(&edited.result.content).unwrap().nodes.into_iter().filter(|node|node.kind=="span").collect::<Vec<_>>();assert_eq!(spans.len(),3);assert_eq!(spans[0].style.size,CascadeSize::Points(HalfPoints::new(60).unwrap()));assert_eq!(spans[1].style,style(json!({"sizeCs":10,"rtl":false})));assert_eq!(&edited.result.content[spans[1].content_bytes.clone()],"middle");assert_eq!(spans[2].style.size,CascadeSize::Points(HalfPoints::new(60).unwrap()));
}

#[test]
fn block_base_clear_is_a_nested_explicit_override_not_a_global_size_change(){
    let source="<span data-readmd-text=\"v1\" data-size-pt=\"30\" data-size-cs-pt=\"10\">left middle right</span>";let(from,to)=coords(source,"middle");let edited=clear(source,&tx::revision(source),from,to,&["sizeCs"],true).unwrap();let spans=get(&edited.result.content).unwrap().nodes.into_iter().filter(|node|node.kind=="span").collect::<Vec<_>>();assert_eq!(spans.len(),2);assert_eq!(spans[1].parent.as_deref(),Some(spans[0].id.as_str()));assert_eq!(spans[1].style,style(json!({"sizeCs":"base"})));assert_eq!(invert(&edited.result),source);assert_eq!(clear(source,&tx::revision(source),from,to,&["rtl"],true).unwrap_err(),INVALID);
}

#[test]
fn unsafe_ranges_stale_revisions_and_entity_interiors_are_rejected(){
    let source="A 𝛂 &amp; \\* [label](https://example.test) and `code`\r\nnext\n\nsecond.";let revision=tx::revision(source);let patch=json!({"size":30});
    assert_eq!(apply(source,"stale",0,1,&patch).unwrap_err(),"transaction_conflict");assert_eq!(apply(source,&revision,3,4,&patch).unwrap_err(),"transaction_invalid_range");
    for needle in ["amp","https://example.test","cod"]{let(from,to)=coords(source,needle);assert_eq!(apply(source,&revision,from,to,&patch).unwrap_err(),TARGET,"{needle}");}
    let(start,end)=coords(source,"* ");assert_eq!(apply(source,&revision,start,end,&patch).unwrap_err(),TARGET);
    let newline=source.find("\r\n").unwrap()+1;assert_eq!(apply(source,&revision,utf16(source,newline),utf16(source,newline+1),&patch).unwrap_err(),"transaction_invalid_range");
    let(from,to)=coords(source,"next\n\nsecond");assert_eq!(apply(source,&revision,from,to,&patch).unwrap_err(),TARGET);
    assert_eq!(apply(source,&revision,0,0,&patch).unwrap_err(),TARGET);
}

#[test]
fn safe_entity_escape_code_and_reference_atoms_remain_content_identical(){
    let source="A &amp; \\* `code` [^n].\n\n[^n]: body\n";for needle in ["&amp;","\\*","`code`","[^n]"]{let(from,to)=coords(source,needle);let result=apply(source,&tx::revision(source),from,to,&json!({"size":30})).unwrap();assert_eq!(semantic_source(source),semantic_source(&result.result.content),"{needle}");assert_eq!(invert(&result.result),source);}
}

#[test]
fn live_formula_atoms_require_their_own_model_in_every_nonempty_author_scope(){
    for attributes in ["data-size-pt=\"30\"","data-size-cs-pt=\"10\"","data-size-pt=\"base\"","data-cs=\"false\"","data-rtl=\"false\""]{
        for formula in ["$x^2$","$$x^2$$",":math[x^2]"]{
            for prefix in ["","A "]{
                let source=format!("{prefix}<span data-readmd-text=\"v1\" {attributes}>{formula}</span>");
                let original=source.clone();let revision=tx::revision(&source);
                assert!(get(&source).unwrap().nodes.iter().any(|node|node.kind=="span"),"structural get must keep source inspectable: {source}");
                assert_eq!(validate_source(&source),Err(UNSUPPORTED),"{source}");
                assert_eq!(prepared(&source).unwrap_err().code,UNSUPPORTED,"{source}");
                assert_eq!(source,original);assert_eq!(tx::revision(&source),revision);
            }
        }
    }
    let source="<span data-readmd-text=\"v1\" data-size-pt=\"30\"><span data-readmd-text=\"v1\">:math[x^2]</span></span>";
    assert_eq!(validate_source(source),Err(UNSUPPORTED));
}

#[test]
fn formula_guard_uses_live_raw_nodes_instead_of_code_entities_links_or_alt_text(){
    for source in [
        "<span data-readmd-text=\"v1\" data-size-pt=\"30\">`$x$ :math[x^2]` &#58;math[x^2] \\:math[x^2]</span>",
        "<span data-readmd-text=\"v1\" data-size-pt=\"30\">![literal $x$ :math[x^2]](owned.png) text</span>",
        "A <span data-readmd-text=\"v1\" data-size-pt=\"30\">[literal :math[x^2]](https://example.test)</span>",
        "A [<span data-readmd-text=\"v1\" data-size-pt=\"30\">:math[x^2]</span>](https://example.test)",
        "A <code><span data-readmd-text=\"v1\" data-size-pt=\"30\">:math[x^2]</span></code>",
        "```html\n<span data-readmd-text=\"v1\" data-size-pt=\"30\">$x$ :math[x^2]</span>\n```\n",
        "<!-- <span data-readmd-text=\"v1\" data-size-pt=\"30\">$x$ :math[x^2]</span> -->\n",
        "<pre><span data-readmd-text=\"v1\" data-size-pt=\"30\">$x$ :math[x^2]</span></pre>\n",
        "<span data-readmd-text=\"v1\">$x^2$</span>",
        "$x^2$ :math[y^2] <span data-readmd-text=\"v1\" data-size-pt=\"30\">text [link](https://example.test) [^n]</span>\n\n[^n]: note\n",
    ]{
        assert_eq!(validate_source(source),Ok(()),"{source}");
        assert!(prepared(source).is_ok(),"{source}");
    }
}

#[test]
fn current_office_math_carrier_is_guarded_but_structural_clear_preserves_it(){
    let omml=crate::latex2omml::latex_to_omml("x",false);
    let metadata=crate::math_document::metadata("x",&omml,"author-guard-current").unwrap();
    let source=format!("\u{feff}A <span data-readmd-text=\"v1\" data-size-pt=\"30\">$x${metadata}</span>\r\n");
    assert!(crate::math_document::read_metadata(&metadata,"x").unwrap().1);
    assert_eq!(validate_source(&source),Err(UNSUPPORTED));
    let span=get(&source).unwrap().nodes.into_iter().find(|node|node.kind=="span").unwrap();
    let cleared=clear(&source,&tx::revision(&source),span.from,span.to,&["size"],false).unwrap();
    assert_eq!(cleared.result.content,format!("\u{feff}A $x${metadata}\r\n"));
    assert_eq!(invert(&cleared.result),source);assert_eq!(validate_source(&cleared.result.content),Ok(()));
    let document=prepared(&cleared.result.content).unwrap();let mut math=Vec::new();
    md_ast::walk_inlines(&document.blocks,&mut|node|if let Inline::OfficeMath{latex,omml}=node{math.push((latex.clone(),omml.clone()));});
    assert_eq!(math,vec![("x".into(),omml)]);
}

#[test]
fn applying_author_properties_to_a_formula_never_emits_an_unsupported_transaction(){
    for formula in ["$x^2$",":math[x^2]"]{
        let source=format!("\u{feff}A {formula} Z\r\n");let original=source.clone();let revision=tx::revision(&source);let(from,to)=coords(&source,formula);
        assert_eq!(apply(&source,&revision,from,to,&json!({"size":30})).unwrap_err(),TARGET);
        assert_eq!(source,original);assert_eq!(tx::revision(&source),revision);
        let styled=format!("\u{feff}A <span data-readmd-text=\"v1\" data-size-pt=\"30\">{formula}</span> Z\r\n");
        let span=get(&styled).unwrap().nodes.into_iter().find(|node|node.kind=="span").unwrap();
        let cleared=clear(&styled,&tx::revision(&styled),span.content_from,span.content_to,&["size"],false).unwrap();
        assert_eq!(cleared.result.content,source);assert_eq!(invert(&cleared.result),styled);
    }
    let source="<span data-readmd-text=\"v1\" data-size-pt=\"30\">:math[x^2]</span>";let revision=tx::revision(source);
    let span=get(source).unwrap().nodes.into_iter().find(|node|node.kind=="span").unwrap();
    let cleared=clear(source,&revision,span.from,span.to,&["size"],false).unwrap();
    assert_eq!(cleared.transaction.base_revision,revision);assert_eq!(cleared.transaction.origin,"editor");
    assert_eq!(cleared.transaction.edits,vec![tx::TextEdit{from:span.from,to:span.content_from,insert:String::new()},tx::TextEdit{from:span.content_to,to:span.to,insert:String::new()}]);
    assert!(cleared.result.changed);assert_eq!(cleared.result.content,":math[x^2]");
    assert_eq!(cleared.result.revision,tx::revision(":math[x^2]"));assert_eq!(invert(&cleared.result),source);
    assert_eq!(tx::revision(source),revision);assert_eq!(validate_source(source),Err(UNSUPPORTED));
    assert_eq!(validate_source(&cleared.result.content),Ok(()));
    let reopened=get(&cleared.result.content).unwrap();assert_eq!(reopened.revision,cleared.result.revision);assert!(!reopened.nodes.iter().any(|node|node.kind=="span"));
    let document=prepared(&cleared.result.content).unwrap();assert_eq!(count_authors(&document.blocks),0);
    let mut formulas=document.blocks.iter().filter(|block|matches!(block.unstyled(),Block::Math(_)|Block::OfficeMath{..})).count();
    md_ast::walk_inlines(&document.blocks,&mut|node|if matches!(node,Inline::Math(_)|Inline::OfficeMath{..}){formulas+=1;});
    assert_eq!(formulas,1,"cleared source must reopen as one real formula, not literal DSL text");
}

#[test]
fn malformed_or_cross_format_partial_clear_never_changes_source(){
    let source="<span data-readmd-text=\"v1\" data-size-pt=\"30\">**left middle** right</span>";let(from,to)=coords(source,"middle");assert_eq!(clear(source,&tx::revision(source),from,to,&["size"],false).unwrap_err(),TARGET);
    let result=apply(source,&tx::revision(source),from,to,&json!({"sizeCs":10})).unwrap();assert_eq!(invert(&result.result),source);assert_eq!(semantic_source(source),semantic_source(&result.result.content));
}

#[test]
fn clearing_across_independent_scopes_cannot_silently_report_a_noop(){
    let source="<span data-readmd-text=\"v1\" data-size-pt=\"30\">one</span> <span data-readmd-text=\"v1\" data-size-pt=\"20\">two</span>";let nodes=get(source).unwrap().nodes;let spans=nodes.iter().filter(|node|node.kind=="span").collect::<Vec<_>>();assert_eq!(clear(source,&tx::revision(source),spans[0].content_from,spans[1].content_to,&["size"],false).unwrap_err(),TARGET);
    let unchanged=clear("plain",&tx::revision("plain"),0,5,&["size"],false).unwrap();assert!(!unchanged.result.changed);assert!(unchanged.transaction.edits.is_empty());
}

#[test]
fn handle_and_schema_require_the_same_typed_action_shapes(){
    let source="word";let revision=tx::revision(source);let response=handle(&json!({"action":"apply","content":source,"base_revision":revision,"from":0,"to":4,"patch":{"size":30,"rtl":false}})).unwrap();assert_eq!(response["ok"],true);assert_eq!(response["result"]["changed"],true);
    let document=handle(&json!({"action":"get","content":response["result"]["content"]})).unwrap();assert_eq!(document["nodes"].as_array().unwrap().iter().filter(|node|node["kind"]=="span").count(),1);
    for request in [json!({"action":"get","content":source,"patch":{}}),json!({"action":"apply","content":source,"base_revision":revision,"from":0,"to":4}),json!({"action":"apply","content":source,"base_revision":revision,"from":0.5,"to":4,"patch":{}}),json!({"action":"apply","content":source,"base_revision":revision,"from":0,"to":4,"patch":{"font":30}}),json!({"action":"clear","content":source,"base_revision":revision,"from":0,"to":4,"fields":["size","size"]}),json!({"action":"clear","content":source,"base_revision":revision,"from":0,"to":4,"fields":[]}),json!({"action":"clear","content":source,"base_revision":revision,"from":0,"to":4,"fields":["cs"],"to_block_base":true}),json!({"action":"clear","content":source,"base_revision":revision,"from":0,"to":4,"to_block_base":null})]{assert_eq!(handle(&request).unwrap_err(),INVALID,"{request}");}
    let schema=request_schema();assert_eq!(schema["oneOf"].as_array().unwrap().len(),3);for shape in schema["oneOf"].as_array().unwrap(){assert_eq!(shape["additionalProperties"],false);}
}
