use super::*;
use crate::{md_ast::{self, ListItem}, pdf_render::{self, DrawItem}};

const SOURCE:&str="First[^zeta], other[^alpha], repeat[^zeta], last[^beta]. Literal [word-footnote-1].\n\n[^beta]: Third body.\n\n[^zeta]: First body.\n\n[^alpha]: Second body.\n";
fn style(format:&str)->Value{crate::export_styles::sanitize(&json!({"typography":{"size":11},"cover":{"enabled":false},"toc":{"enabled":false},"footer":{"pageNumbers":false},"frontMatter":{"mode":"none"},"footnotes":{"numberFormat":format,"start":7,"restart":"continuous","size":9}}))}
fn project(doc:&Document,style:&Value)->Value{
    let plan=Projection::analyze(doc,style).unwrap().unwrap();
    md_ast::to_rust_note_render_json(&doc.blocks,&plan,0,style).unwrap()
}
fn markers(value:&Value)->Vec<Marker>{
    fn walk(value:&Value,out:&mut Vec<Marker>){match value{Value::Array(values)=>for v in values{walk(v,out)},Value::Object(object)=>{if value["t"]=="note-marker"{out.push(Marker::parse(value).unwrap());}else{for child in object.values(){walk(child,out)}}},_=>()}}
    let mut out=Vec::new();walk(value,&mut out);out
}
fn mutate_markers(value:&mut Value,f:&mut impl FnMut(&mut Value)){
    match value{Value::Array(values)=>for child in values{mutate_markers(child,f)},Value::Object(object)=>{if object.get("t").and_then(Value::as_str)==Some("note-marker"){f(value);}else{for child in object.values_mut(){mutate_markers(child,f)}}},_=>()}
}
fn rendered(value:&Value,style:&Value)->pdf_render::RenderedDoc{
    let mut warnings=Vec::new();let doc=pdf_render::build(value,style,"",&|_|None,&mut warnings).unwrap();
    assert!(!warnings.iter().any(|s|s.starts_with("pdf_anchor_missing")),"{warnings:?}");doc
}
fn destinations(doc:&pdf_render::RenderedDoc)->BTreeMap<String,(usize,f64,f64)>{
    let mut out=BTreeMap::new();for(index,page)in doc.pages.iter().enumerate(){for item in &page.items{if let DrawItem::InlineDestination{name,left,baseline}=item{assert!(out.insert(name.clone(),(index,*left,*baseline)).is_none());}}}out
}

#[test]
fn first_use_numbering_keeps_identity_repetition_all_five_formats_and_legacy_json(){
    let doc=md_ast::parse(SOURCE);let original=doc.clone();let legacy=md_ast::to_render_json(&doc.blocks);
    for(format,expected)in[("decimal",["7","8","7","9"]),("lowerRoman",["vii","viii","vii","ix"]),("upperRoman",["VII","VIII","VII","IX"]),("lowerLetter",["g","h","g","i"]),("upperLetter",["G","H","G","I"]) ]{
        let options=style(format);let value=project(&doc,&options);let all=markers(&value);
        let refs=all.iter().filter(|m|m.role=="reference").collect::<Vec<_>>();
        assert_eq!(refs.iter().map(|m|m.number.as_str()).collect::<Vec<_>>(),expected);
        assert_eq!(refs.iter().map(|m|m.label.as_str()).collect::<Vec<_>>(),["zeta","alpha","zeta","beta"]);
        assert_eq!(refs[0].note,refs[2].note);assert_ne!(refs[0].reference,refs[2].reference);
        assert_eq!(all.iter().filter(|m|m.role=="definition").map(|m|(m.label.as_str(),m.number.as_str())).collect::<Vec<_>>(),[("beta",expected[3]),("zeta",expected[0]),("alpha",expected[1])]);
        assert_eq!(all.iter().filter(|m|m.role=="backlink").map(Marker::text).collect::<Vec<_>>(),["[2]"]);
        assert!(value.to_string().contains("Literal [word-footnote-1]."));
        assert_eq!(validated_destinations(&value,&options).unwrap().len(),7);
    }
    assert_eq!(doc,original);assert_eq!(md_ast::to_render_json(&doc.blocks),legacy);
    assert_eq!(md_ast::inlines_json(&[Inline::FootnoteRef("slug".into())]),json!([{"t":"text","v":"[slug]"}]));
    assert!(Projection::analyze(&md_ast::parse("Literal [^undefined], [word-footnote-1]."),&style("decimal")).unwrap().is_none());
}

#[test]
fn definitions_before_references_and_real_quote_list_cell_paths_keep_first_use(){
    let mut doc=md_ast::parse(SOURCE);let defs=doc.blocks.split_off(1);let body=doc.blocks.pop().unwrap();doc.blocks=defs;doc.blocks.push(body);
    let options=style("upperRoman");assert!(validate_json(&project(&doc,&options),&options).is_ok());
    let custom=Document{front_matter:None,blocks:vec![
        Block::Quote(vec![Block::Paragraph(vec![Inline::Text("[!NOTE] Title ".into()),Inline::FootnoteRef("a".into()),Inline::SoftBreak,Inline::Text("Body ".into()),Inline::FootnoteRef("b".into())])]),
        Block::List{ordered:false,start:1,items:vec![ListItem{task:None,blocks:vec![Block::Paragraph(vec![Inline::FootnoteRef("a".into())])]}]},
        Block::Table{aligns:vec![],header:vec![vec![Inline::Text("Header".into())]],rows:vec![vec![vec![Inline::FootnoteRef("b".into())]]]},
        Block::FootnoteDef{label:"b".into(),blocks:vec![Block::Paragraph(vec![Inline::Text("B".into())])]},
        Block::FootnoteDef{label:"a".into(),blocks:vec![Block::Paragraph(vec![Inline::Text("A".into())])]},
    ]};
    let value=project(&custom,&options);assert_eq!(markers(&value).iter().filter(|m|m.role=="reference").map(|m|m.label.as_str()).collect::<Vec<_>>(),["a","b","a","b"]);
    assert_eq!(destinations(&rendered(&value,&options)).len(),6);
}

#[test]
fn marker_graph_is_closed_style_bound_and_rejects_whole_note_wrong_numbers(){
    let options=style("decimal");let good=project(&md_ast::parse(SOURCE),&options);assert!(validate_json(&good,&options).is_ok());
    for changed in [json!("99"),json!("NaN"),json!(7)]{
        let mut value=good.clone();mutate_markers(&mut value,&mut|v|if v["label"]=="zeta"{v["number"]=changed.clone()});assert_eq!(validate_json(&value,&options).unwrap_err(),INVALID);
    }
    for field in ["role","note","reference","extra"]{
        let mut value=good.clone();let first=&mut value[0]["text"][1];assert_eq!(first["t"],"note-marker");first[field]=json!("forged");assert_eq!(validate_json(&value,&options).unwrap_err(),INVALID);
    }
    let mut duplicate=good.clone();let token=duplicate[0]["text"][1].clone();duplicate[0]["text"].as_array_mut().unwrap().push(token);assert_eq!(validate_json(&duplicate,&options).unwrap_err(),INVALID);
    let mut omitted=good.clone();mutate_markers(&mut omitted,&mut|v|if v["role"]=="backlink"{*v=json!({"t":"text","v":""})});assert_eq!(validate_json(&omitted,&options).unwrap_err(),INVALID);
    let mut wrong_style=options.clone();wrong_style["footnotes"]["start"]=json!(8);assert_eq!(validate_json(&good,&wrong_style).unwrap_err(),INVALID);
    for(key,raw)in[("start",json!(0)),("start",json!(10_000)),("numberFormat",json!("NaN")),("restart",json!("unknown"))]{let mut bad=options.clone();bad["footnotes"][key]=raw;assert_eq!(validate_json(&good,&bad).unwrap_err(),UNSUPPORTED);}
    let mut collision=good.clone();let note=collision[0]["text"][1]["note"].clone();collision.as_array_mut().unwrap().insert(0,json!({"type":"heading","id":note,"text":[{"t":"text","v":"COLLISION"}]}));assert_eq!(validate_json(&collision,&options).unwrap_err(),INVALID);
}

#[test]
fn unsupported_owned_wrappers_reject_but_future_metadata_is_opaque(){
    let options=style("decimal");let good=project(&md_ast::parse(SOURCE),&options);let token=good[0]["text"][1].clone();
    for wrapped in [json!({"t":"future","v":[token.clone()]}),json!({"v":[token.clone()]}),json!({"t":"future","text":[token.clone()]})]{
        let value=json!([{"type":"paragraph","text":[wrapped]}]);assert_eq!(validate_json(&value,&options).unwrap_err(),INVALID);
    }
    for value in [json!([{"type":"future","blocks":[{"type":"paragraph","text":[token.clone()]}]}]),json!([token.clone()])]{assert_eq!(validate_json(&value,&options).unwrap_err(),INVALID);}
    let opaque=json!([{"type":"paragraph","text":[{"t":"text","v":"KEEP"}],"future":{"text":[token.clone()]},"metadata":{"v":[token]}}]);assert!(validate_json(&opaque,&options).is_ok());
}

#[test]
fn semantic_missing_duplicates_nested_and_paged_policies_fail_without_changing_target(){
    let options=style("decimal");let mut missing=Document::default();missing.blocks.push(Block::Paragraph(vec![Inline::FootnoteRef("missing".into())]));assert!(matches!(Projection::analyze(&missing,&options),Err(e)if e==INVALID));
    let mut duplicate=md_ast::parse(SOURCE);duplicate.blocks.push(duplicate.blocks[1].clone());assert!(matches!(Projection::analyze(&duplicate,&options),Err(e)if e==INVALID));
    let nested=md_ast::parse("One[^a].\n\n[^a]: Nested[^b].\n\n[^b]: B.\n");assert!(matches!(Projection::analyze(&nested,&options),Err(e)if e==UNSUPPORTED));
    for restart in ["page","section"]{let mut policy=options.clone();policy["footnotes"]["restart"]=json!(restart);assert!(matches!(Projection::analyze(&md_ast::parse(SOURCE),&policy),Err(e)if e==UNSUPPORTED));}
    let mut multi=options.clone();multi["footnotes"]["kindByLabel"]=json!({"alpha":"endnote"});assert!(Projection::analyze(&md_ast::parse(SOURCE),&multi).is_err());
    let mut value=project(&md_ast::parse(SOURCE),&options);value[0]["text"][1]["number"]=json!("99");let dir=tempfile::tempdir().unwrap();let path=dir.path().join("protected.pdf");std::fs::write(&path,b"OLD-EXACT").unwrap();
    assert!(pdf_render::render(&value,path.to_str().unwrap(),&options,"",&|_|None,&mut Vec::new()).is_err());assert_eq!(std::fs::read(path).unwrap(),b"OLD-EXACT");
}

#[test]
fn glyph_baseline_destinations_and_every_pdf_goto_are_real_and_author_size_is_kept(){
    let options=style("upperRoman");let mut doc=md_ast::parse(SOURCE);
    let Block::Paragraph(inlines)=&mut doc.blocks[0]else{panic!()};
    let token=inlines.iter_mut().find(|v|matches!(v,Inline::FootnoteRef(label)if label=="zeta")).unwrap();*token=Inline::AuthorStyled{style:crate::publication_inline_text::AuthorTextStyle::from_value(&json!({"size":10,"sizeCs":10})).unwrap(),children:vec![token.clone()]};
    let value=project(&doc,&options);let all=markers(&value);let layout=rendered(&value,&options);let targets=destinations(&layout);assert_eq!(targets.len(),7);
    let first=all.iter().find(|m|m.role=="reference").unwrap();let(pi,x,y)=targets[&first.reference];let page=&layout.pages[pi];
    assert!(page.items.iter().any(|item|matches!(item,DrawItem::Text{x:tx,y:ty,text,spans}if text=="VII"&&(*tx-x).abs()<0.000001&&(*ty-y).abs()<0.000001&&spans.iter().all(|s|(s.size-7.).abs()<0.000001))));
    let links=layout.pages.iter().flat_map(|p|&p.items).filter_map(|item|if let DrawItem::Link{href,..}=item{Some(href)}else{None}).collect::<Vec<_>>();assert_eq!(links.len(),8);
    for marker in &all{let href=marker.href();assert_eq!(links.iter().filter(|actual|actual.as_str()==href).count(),all.iter().filter(|m|m.href()==href).count());}
    let bytes=pdf_render::render_bytes(&value,&options,"",&|_|None,&mut Vec::new()).unwrap();let pdf=lopdf::Document::load_mem(&bytes).unwrap();let mut annotations=0;
    for page_id in pdf.get_pages().values(){let page=pdf.get_object(*page_id).unwrap().as_dict().unwrap();if let Ok(array)=page.get(b"Annots").and_then(lopdf::Object::as_array){for ann in array{
        let ann=pdf.get_object(ann.as_reference().unwrap()).unwrap().as_dict().unwrap();let dest=ann.get(b"Dest").unwrap().as_array().unwrap();assert_eq!(dest.len(),5);assert_eq!(dest[1].as_name().unwrap(),b"XYZ");assert!(dest[2].as_float().unwrap().is_finite()&&dest[3].as_float().unwrap().is_finite());annotations+=1;
    }}}
    assert_eq!(annotations,8);
}

#[test]
fn paginated_repeated_table_header_keeps_forward_links_and_returns_to_first_paint(){
    let options=style("decimal");let mut rows=Vec::new();for n in 0..110{rows.push(vec![vec![Inline::Text(format!("ROW{n:03} data"))]]);}
    let doc=Document{front_matter:None,blocks:vec![Block::Table{aligns:vec![],header:vec![vec![Inline::Text("HEADER".into()),Inline::FootnoteRef("table-note".into())]],rows},Block::FootnoteDef{label:"table-note".into(),blocks:vec![Block::Paragraph(vec![Inline::Text("TABLE NOTE".into())])]}]};
    let value=project(&doc,&options);let marker=markers(&value).into_iter().find(|m|m.role=="reference").unwrap();let layout=rendered(&value,&options);assert!(layout.pages.len()>1);let targets=destinations(&layout);assert_eq!(targets.len(),2);
    let first=targets[&marker.reference];let mut forwards=Vec::new();for(index,page)in layout.pages.iter().enumerate(){for item in &page.items{if let DrawItem::Link{x,y,w,h,href}=item{if href==&marker.href(){forwards.push((index,*x,*y,*w,*h));}}}}
    assert!(forwards.len()>1);assert_eq!(forwards[0].0,first.0);assert!((forwards[0].1-first.1).abs()<0.001);assert!(first.2>=forwards[0].2&&first.2<=forwards[0].2+forwards[0].4);
    assert_eq!(layout.pages.iter().flat_map(|p|&p.items).filter(|item|matches!(item,DrawItem::Link{href,..}if href==&format!("#{}",marker.reference))).count(),1);
    let pdf=lopdf::Document::load_mem(&pdf_render::render_bytes(&value,&options,"",&|_|None,&mut Vec::new()).unwrap()).unwrap();let pages=pdf.get_pages();let mut returns=0;
    for page_id in pages.values(){let page=pdf.get_object(*page_id).unwrap().as_dict().unwrap();if let Ok(array)=page.get(b"Annots").and_then(lopdf::Object::as_array){for ann in array{let dict=pdf.get_object(ann.as_reference().unwrap()).unwrap().as_dict().unwrap();let dest=dict.get(b"Dest").unwrap().as_array().unwrap();if dest[0].as_reference().ok()==pages.get(&((first.0+1)as u32)).copied()&&(dest[2].as_float().unwrap()-first.1 as f32).abs()<0.001&&(dest[3].as_float().unwrap()-first.2 as f32).abs()<0.001{returns+=1;}}}}
    assert_eq!(returns,1);
}

#[test]
fn adjacent_references_have_separate_forward_rects_and_preserve_author_state(){
    let options=style("decimal");let doc=md_ast::parse("ADJACENT[^n][^n].\n\n[^n]: Body.\n");let value=project(&doc,&options);let all=markers(&value);let refs=all.iter().filter(|m|m.role=="reference").collect::<Vec<_>>();let layout=rendered(&value,&options);let targets=destinations(&layout);
    let forward=layout.pages.iter().flat_map(|p|&p.items).filter_map(|item|if let DrawItem::Link{x,y,w,h,href}=item{(href==&refs[0].href()).then_some((*x,*y,*w,*h))}else{None}).collect::<Vec<_>>();
    assert_eq!(forward.len(),2);assert!((forward[0].0+forward[0].2-forward[1].0).abs()<0.001);for(index,marker)in refs.iter().enumerate(){assert!((targets[&marker.reference].1-forward[index].0).abs()<0.001);}
    let literal=md_ast::parse("[AB](https://example.test) [CD](https://example.test)");assert!(Projection::analyze(&literal,&options).unwrap().is_none());
}

#[test]
fn endnote_selected_legacy_policy_keeps_roman_start_without_changing_source(){
    let doc=md_ast::parse(SOURCE);let original=doc.clone();let mut options=style("decimal");options["footnotes"]["position"]=json!("document");options["footnotes"]["kindByLabel"]=json!({"zeta":"endnote","alpha":"endnote","beta":"endnote"});options["footnotes"]["kinds"]=json!({"endnote":{"numberFormat":"upperRoman","start":7,"restart":"continuous","placement":"docEnd"}});
    let projected=crate::publication_note_policies::project_legacy(&doc,&options).unwrap();assert_eq!(projected["footnotes"]["position"],"document");let value=project(&doc,&projected);assert_eq!(markers(&value).iter().filter(|m|m.role=="reference").map(|m|m.number.as_str()).collect::<Vec<_>>(),["VII","VIII","VII","IX"]);assert_eq!(doc,original);
}

#[test]
fn ten_thousand_references_are_bounded_component_work_and_cancel_before_projection(){
    let options=style("decimal");let refs=(0..10_000).map(|n|Inline::FootnoteRef(format!("n{}",n%3))).collect();let mut doc=Document::default();doc.blocks.push(Block::Paragraph(refs));
    for n in 0..3{doc.blocks.push(Block::FootnoteDef{label:format!("n{n}"),blocks:vec![Block::Paragraph(vec![Inline::Text(format!("NOTE{n}"))])]});}
    let value=project(&doc,&options);let all=markers(&value);assert_eq!(all.iter().filter(|m|m.role=="reference").count(),10_000);assert_eq!(all.iter().filter(|m|m.role=="backlink").count(),9_997);assert_eq!(validated_destinations(&value,&options).unwrap().len(),10_003);
    crate::speech::with_cancel(std::sync::Arc::new(||true),||{assert!(matches!(Projection::analyze(&doc,&options),Err(e)if e=="cancelled"));assert_eq!(validate_json(&value,&options).unwrap_err(),"cancelled");});
    let mut deep=Block::Paragraph(vec![Inline::Text("deep".into())]);for _ in 0..140{deep=Block::Quote(vec![deep]);}assert!(matches!(Projection::analyze(&Document{front_matter:None,blocks:vec![deep]},&options),Err(e)if e==BUDGET));
    let oversized=Document{front_matter:None,blocks:vec![Block::Code{lang:String::new(),text:"x".repeat(crate::document_transaction::MAX_DOCUMENT_BYTES+1)}]};assert!(matches!(Projection::analyze(&oversized,&options),Err(e)if e==BUDGET));
}

#[test]
fn high_start_letter_numbers_keep_legal_word_format_and_derived_token_bound(){
    assert_eq!((MAX_NOTES+9999-1).div_ceil(26),1645);assert!(1645<=MAX_NUMBER_BYTES);
    let doc=md_ast::parse(SOURCE);let mut options=style("upperLetter");options["footnotes"]["start"]=json!(9999);
    let value=project(&doc,&options);let all=markers(&value);let refs=all.iter().filter(|m|m.role=="reference").collect::<Vec<_>>();
    for(marker,number)in refs.iter().zip([9999usize,10000,9999,10001]){let c=(b'A'+((number-1)%26)as u8)as char;let expected=c.to_string().repeat((number-1)/26+1);assert_eq!(marker.number,expected);assert!(marker.number.len()>64&&marker.number.len()<=MAX_NUMBER_BYTES);}
    let mut forged=value.clone();mutate_markers(&mut forged,&mut|v|if v["label"]=="zeta"{v["number"]=json!("A".repeat(MAX_NUMBER_BYTES+1))});assert_eq!(validate_json(&forged,&options).unwrap_err(),BUDGET);
}

#[test]
fn large_reference_graph_is_rejected_before_allocating_generated_marker_graph(){
    let mut doc=Document::default();doc.blocks.push(Block::Paragraph((0..50_000).map(|_|Inline::FootnoteRef("a".into())).collect()));doc.blocks.push(Block::FootnoteDef{label:"a".into(),blocks:vec![Block::Paragraph(vec![Inline::Text("BODY".into())])]});
    assert!(matches!(Projection::analyze(&doc,&style("decimal")),Err(e)if e==BUDGET));
    let Block::Paragraph(references)=&doc.blocks[0]else{panic!()};assert_eq!(references.len(),50_000);assert!(references.iter().all(|node|matches!(node,Inline::FootnoteRef(label)if label=="a")));
}
