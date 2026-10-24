use super::*;
use serde_json::json;
use crate::publication_inline_text::{SizeOrigin,UNSUPPORTED};

fn style()->Value{crate::export_styles::sanitize(&json!({"typography":{"size":11},"cover":{"enabled":false},"toc":{"enabled":false}}))}
fn wrapper(text:Value,children:Value)->Value{json!({"t":"author-styled","text":text,"v":children})}
fn text(value:&str)->Value{json!({"t":"text","v":value})}
fn runs(value:Value)->Vec<Run>{inline_markup(&value,&style(),"",&|_|None)}

#[test]
fn retarget_preserves_absolute_and_rebases_both_independent_reset_axes(){
    let value=wrapper(json!({"size":30,"sizeCs":10}),json!([
        text("ABS "),wrapper(json!({"size":"base"}),json!([text("BASE ")])),
        wrapper(json!({"sizeCs":"base","cs":true}),json!([text("CSBASE")]))
    ]));
    let original=runs(value);assert_eq!(runs_plain_text(&original),"ABS BASE CSBASE");
    let changed=retarget_runs_pair(&original,17.25,9.75,hex_parse("#334455").unwrap());
    assert_eq!(changed.iter().map(|r|r.size).collect::<Vec<_>>(),vec![30.,17.25,9.75]);
    assert_eq!(changed[0].text_style.author.unwrap().size_origin,SizeOrigin::Author);
    assert_eq!(changed[1].text_style.author.unwrap().size_origin,SizeOrigin::BlockBase);
    assert_eq!(changed[2].text_style.author.unwrap().size_cs_origin,SizeOrigin::BlockBase);
    let again=retarget_runs_pair(&changed,21.,8.,changed[0].color);
    assert_eq!(again.iter().map(|r|r.size).collect::<Vec<_>>(),vec![30.,21.,8.]);
}

fn caption_block(author:Value,children:Value)->Value{
    json!({"type":"caption","id":"tbl:pdf-caption","text":children,"publicationStyle":{"id":"pdf-caption","text":{"author":author}}})
}

#[test]
fn caption_block_axes_style_label_title_and_effective_inline_base_in_actual_story(){
    let block=caption_block(json!({"size":30,"sizeCs":10,"cs":false,"rtl":false}),json!([
        text("Table 1 — TITLE "),wrapper(json!({"cs":true}),json!([text("COMPLEX ")])),
        wrapper(json!({"size":45,"sizeCs":18}),json!([text("INNER "),wrapper(json!({"size":"base","sizeCs":"base"}),json!([text("RESET "),wrapper(json!({"cs":true}),json!([text("CSRESET")]))]))]))
    ]));
    let options=style();let source=json!([block,{"type":"paragraph","text":[text("AFTER")]}]);
    publication_paragraph::validate(&source,&options).unwrap();
    let styles=Styles::build(&options,"Helvetica");let geom=DocGeometry::from_style(&options);
    let story=build_story(&source,&options,"",&|_|None,&styles,"Helvetica",&geom,&mut Vec::new(),&[]);
    let paragraphs=story.iter().filter_map(|item|if let Flowable::Paragraph{runs,..}=item{Some(runs)}else{None}).collect::<Vec<_>>();
    let caption=paragraphs.iter().find(|runs|runs_plain_text(runs).contains("TITLE")).unwrap();check_run_errors(caption).unwrap();
    for (needle,points) in [("Table 1 — TITLE ",30.),("COMPLEX ",10.),("INNER ",45.),("RESET ",30.),("CSRESET",10.)]{
        assert_eq!(caption.iter().find(|run|run.text()==needle).unwrap().size,points,"{needle}");
    }
    assert!(paragraphs.iter().find(|runs|runs_plain_text(runs)=="AFTER").unwrap().iter().all(|run|run.size==11.));
}

#[test]
fn caption_json_wrong_target_math_rtl_and_huge_values_remain_errors(){
    for author in [json!({"rtl":true}),json!({"size":4096.5})]{
        let block=caption_block(author,json!([text("TITLE")]));let options=style();
        if publication_paragraph::validate(&json!([block.clone()]),&options).is_ok(){
            assert!(publication_paragraph::style(&block,0,PStyle::base("Helvetica",11.,14.),&options,&mut Vec::new()).is_err());
        }
    }
    let mut wrong=caption_block(json!({"size":30}),json!([text("BODY")]));wrong["type"]=json!("paragraph");
    assert!(publication_paragraph::validate(&json!([wrong]),&style()).is_err());
    assert!(publication_paragraph::validate(&json!([caption_block(json!({"size":30}),json!([{"t":"math","v":"x"}]))]),&style()).is_err());
}

#[test]
fn caption_layered_legacy_size_does_not_erase_inherited_axis_routing(){
    let mut block=caption_block(json!({"size":30,"sizeCs":10,"cs":true,"rtl":false}),json!([
        text("COMPLEX "),wrapper(json!({"cs":false}),json!([text("WESTERN ")])),
        wrapper(json!({"size":"base","sizeCs":"base"}),json!([text("RESET")]))
    ]));
    block["publicationStyle"]=json!([block["publicationStyle"].clone(),{"text":{"size":17.5,"author":{"size":30}}}]);
    let options=style();publication_paragraph::validate(&json!([block.clone()]),&options).unwrap();
    let caption=publication_paragraph::style(&block,0,PStyle::base("Helvetica",11.,14.),&options,&mut Vec::new()).unwrap();
    let runs=publication_paragraph::inline(&block["text"],&options,"",&|_|None,&caption);
    check_run_errors(&runs).unwrap();
    for (needle,points) in [("COMPLEX ",17.5),("WESTERN ",30.),("RESET",17.5)]{
        assert_eq!(runs.iter().find(|run|run.text()==needle).unwrap().size,points,"{needle}");
    }
}

#[test]
fn forced_complex_latin_keeps_link_face_code_and_explicit_equal_origin(){
    let value=wrapper(json!({"size":11,"sizeCs":30,"cs":true}),json!([
        {"t":"link","href":"https://example.test/a","text":[{"t":"bold","v":[text("Latin")]}]},
        {"t":"code","v":" code"}
    ]));
    let original=runs(value);check_run_errors(&original).unwrap();
    let changed=retarget_runs(&original,9.,hex_parse("#334455").unwrap());
    assert_eq!(runs_plain_text(&changed),"Latin code");assert!(changed.iter().all(|r|r.size==30.));
    assert_eq!(changed[0].face,FaceStyle::Bold);assert_eq!(changed[0].href.as_deref(),Some("https://example.test/a"));
    assert_eq!(changed[1].face,FaceStyle::Mono);
    let equal=runs(wrapper(json!({"size":11}),json!([text("same")])));
    assert_eq!(retarget_runs(&equal,18.,equal[0].color)[0].size,11.);
}

#[test]
fn shaping_and_bidi_fail_explicitly_but_latin_and_cjk_keep_the_western_axis(){
    for source in ["العربية","עברית","हिन्दी","ไทย","e\u{301}","👩\u{200d}💻"]{
        let result=runs(wrapper(json!({"size":30,"sizeCs":10}),json!([text(source)])));
        let error=check_run_errors(&result).unwrap_err();assert!(error.message.starts_with("author_inline_text_pdf_shaping_unsupported: "),"{}",error.message);
    }
    let safe=runs(wrapper(json!({"size":30,"sizeCs":10}),json!([text("Latin 中文")])));
    check_run_errors(&safe).unwrap();assert_eq!(runs_plain_text(&safe),"Latin 中文");assert!(safe.iter().all(|r|r.size==30.));
}

#[test]
fn author_size_drives_width_and_line_extent_as_well_as_paint(){
    let original=runs(json!([text("small "),wrapper(json!({"size":30}),json!([text("WIDE")]))]));
    let base=PStyle::base("Helvetica",11.,14.);
    let lines=paragraph_lines(&original,&base,500.);assert_eq!(lines.len(),1);
    let wide=lines[0].atoms.iter().find(|a|a.text=="WIDE").unwrap();
    assert_eq!(wide.size,30.);assert_eq!(wide.width,str_width("WIDE",wide.face,wide.base,30.));
    assert!(line_height(&lines[0],&base)>30.);
    let narrow=retarget_runs(&original,7.,base.color);assert_eq!(narrow.iter().find(|r|r.text()=="WIDE").unwrap().size,30.);
}

#[test]
fn a_selection_inside_a_word_does_not_create_a_new_break_opportunity(){
    let original=runs(json!([text("X "),text("ab"),wrapper(json!({"size":30}),json!([text("CD")])),text("ef")]));
    let atoms=atoms_from_runs(&original);let word_width=atoms.iter().filter(|atom|["ab","CD","ef"].contains(&atom.text.as_str())).map(|atom|atom.width).sum::<f64>();
    assert!(atoms.iter().find(|atom|atom.text=="CD").unwrap().join_left);assert!(atoms.iter().find(|atom|atom.text=="ef").unwrap().join_left);
    let lines=paragraph_lines(&original,&PStyle::base("Helvetica",11.,14.),word_width+0.1);
    assert_eq!(lines.iter().map(|line|line.text.as_str()).collect::<Vec<_>>(),vec!["X","abCDef"]);
}

#[test]
fn author_note_json_uses_note_base_and_local_block_base_without_losing_text(){
    let value=json!([{"type":"paragraph","publicationContext":"footnote","publicationNoteLabel":"n","publicationStyle":{"text":{"size":13.5}},"text":[text("[n] "),wrapper(json!({"size":30}),json!([text("BIG "),wrapper(json!({"size":"base"}),json!([text("RESET")]))]))]}]);
    let mut options=style();options["footnotes"]["size"]=json!(9.5);
    let styles=Styles::build(&options,"Helvetica");let geom=DocGeometry::from_style(&options);
    let story=build_story(&value,&options,"",&|_|None,&styles,"Helvetica",&geom,&mut Vec::new(),&[]);
    let Flowable::Paragraph{runs,..}=&story[0]else{panic!("{story:?}")};
    assert_eq!(runs_plain_text(runs),"[n] BIG RESET");assert_eq!(runs.iter().find(|r|r.text()=="RESET").unwrap().size,13.5);
}

#[test]
fn huge_or_invalid_author_values_are_errors_not_clamped(){
    for values in [json!({"size":4096.5}),json!({"size":0}),json!({"size":10.25}),json!({"size":30,"evil":"script"})]{
        let result=runs(wrapper(values,json!([text("visible")])));assert!(check_run_errors(&result).is_err());
    }
    let result=runs(wrapper(json!({"sizeCs":4096.5}),json!([text("ASCII")])));assert_eq!(check_run_errors(&result).unwrap_err().message,UNSUPPORTED);
}

fn transparent_rtl_container(child:Value)->Value{
    json!({"t":"bold","v":[{"t":"italic","v":[{"t":"strike","v":[{"t":"styled","format":{},"v":[{"t":"display","display":"right-inline","v":[{"t":"link","href":"https://example.test/cleared","text":[child]}]}]}]}]}]})
}

#[test]
fn effective_false_rtl_survives_transparent_containers_without_guarding_empty_nodes(){
    let cleared=wrapper(json!({"rtl":false}),json!([text("CLEARED"),{"t":"code","v":"CODE"},{"t":"math","latex":"x","fallback":true}]));
    let value=wrapper(json!({"rtl":true,"size":30,"sizeCs":10}),json!([wrapper(json!({"rtl":true}),json!([])),text(""),{"t":"code","v":""},transparent_rtl_container(cleared)]));
    let result=runs(value);check_run_errors(&result).unwrap();
    assert!(runs_plain_text(&result).contains("CLEAREDCODE"));
    assert!(result.iter().any(|run|run.text()=="CLEARED"&&run.size==30.&&run.href.as_deref()==Some("https://example.test/cleared")));
    assert!(result.iter().any(|run|run.is_image()||run.text()=="x"));
    let empty=runs(wrapper(json!({"rtl":true}),json!([text(""),{"t":"code","v":""},{"t":"image","src":"unused.png","alt":""},{"t":"link","href":"https://example.test/invisible","text":[]}])));
    check_run_errors(&empty).unwrap();assert!(runs_plain_text(&empty).is_empty());
}

#[test]
fn effective_true_math_and_real_images_guard_without_erasing_shaping_errors(){
    let directory=tempfile::tempdir().unwrap();let file=directory.path().join("owned.png");
    image::RgbImage::from_pixel(2,2,image::Rgb([24,48,96])).save(&file).unwrap();
    let png=std::fs::read(&file).unwrap();let path=file.to_string_lossy().into_owned();
    for atom in [json!({"t":"math","latex":"x","fallback":false,"png":png,"w":8,"h":8}),json!({"t":"image","src":"owned.png","alt":"IMAGE"}),json!({"t":"br"})]{
        let value=wrapper(json!({"rtl":true}),json!([transparent_rtl_container(wrapper(json!({"rtl":false}),json!([text("CLEARED")]))),atom]));
        let result=inline_markup(&value,&style(),directory.path().to_str().unwrap(),&|_|Some(path.clone()));
        assert_eq!(check_run_errors(&result).unwrap_err().message,UNSUPPORTED);
    }
    for source in ["TRUE_LEAF","العربية"]{
        let result=runs(wrapper(json!({"rtl":true}),json!([text(source)])));
        assert!(check_run_errors(&result).unwrap_err().message.starts_with("author_inline_text_pdf_shaping_unsupported: "));
    }
    let noteref=crate::md_ast::inlines_json(&[crate::md_ast::Inline::FootnoteRef("n".into())]);
    let result=runs(wrapper(json!({"rtl":true}),noteref));
    assert!(check_run_errors(&result).unwrap_err().message.starts_with("author_inline_text_pdf_shaping_unsupported: "));
}

#[test]
fn missing_image_visible_fallback_uses_itemizer_and_current_size(){
    let directory=tempfile::tempdir().unwrap();let missing=directory.path().join("missing-owned.png").to_string_lossy().into_owned();
    let value=|attributes:Value,alt:&str|wrapper(attributes,json!([{"t":"image","src":"missing-owned.png","alt":alt}]));
    let result=inline_markup(&value(json!({"size":30,"sizeCs":10,"cs":true}),"FALLBACK"),&style(),"",&|_|Some(missing.clone()));
    check_run_errors(&result).unwrap();assert_eq!(runs_plain_text(&result),"FALLBACK");assert_eq!(result[0].size,10.);
    for (attributes,alt) in [(json!({"rtl":true}),"FALLBACK"),(json!({"size":30}),"العربية")]{
        let result=inline_markup(&value(attributes,alt),&style(),"",&|_|Some(missing.clone()));
        assert!(check_run_errors(&result).unwrap_err().message.starts_with("author_inline_text_pdf_shaping_unsupported: "));
    }
}

#[test]
fn caption_shaping_preflight_precedes_real_image_callbacks_and_preserves_existing_target(){
    use std::cell::Cell;
    let directory=tempfile::tempdir().unwrap();let target=directory.path().join("protected-caption.pdf");let original=b"KEEP EXISTING CAPTION PDF";
    std::fs::write(&target,original).unwrap();
    for (children,from,to) in [
        (json!([text("A العربية")]),2,16),(json!([{"t":"code","v":"A العربية"}]),2,16),
        (json!([{"t":"link","href":"https://example.test/ignored","text":"A العربية"}]),2,16),
        (json!([{"t":"bold","v":"A العربية"}]),2,16),
        (json!([transparent_rtl_container(wrapper(json!({"size":45,"sizeCs":18}),json!([
            wrapper(json!({"size":"base","sizeCs":"base"}),json!([text("A العربية")]))
        ])))]),2,16),
        (json!([text("A e\u{301}")]),0,5),
    ]{
        let blocks=json!([
            {"type":"paragraph","text":[{"t":"image","src":"resource-probe.png","alt":"probe"}]},
            caption_block(json!({"size":30,"sizeCs":10,"cs":false,"rtl":false}),children)
        ]);let before=blocks.clone();let calls=Cell::new(0);let mut warnings=Vec::new();
        let error=render(&blocks,target.to_str().unwrap(),&style(),directory.path().to_str().unwrap(),&|_|{calls.set(calls.get()+1);None},&mut warnings).unwrap_err();
        let(code,raw)=error.message.split_once(": ").expect("shaping details must survive preflight");assert_eq!(code,"author_inline_text_pdf_shaping_unsupported");
        let details:Value=serde_json::from_str(raw).unwrap();assert_eq!(details["consumer"],"rustPdf");assert_eq!(details["coordinateSpace"],"textUtf8Bytes");assert_eq!(details["from"],from);assert_eq!(details["to"],to);
        assert_eq!(calls.get(),0,"caption text must be checked before resolving an earlier image");assert!(warnings.is_empty());assert_eq!(blocks,before);assert_eq!(std::fs::read(&target).unwrap(),original);assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(),1);
    }
}

#[test]
fn caption_shaping_preflight_keeps_normalized_visible_text_flags_layers_and_empty_legacy_scope(){
    let mut caption=caption_block(json!({"size":30,"sizeCs":10,"cs":true,"rtl":false}),json!([
        text("FORCEDLATIN "),wrapper(json!({"cs":false,"size":45,"sizeCs":19}),json!([
            {"t":"link","href":"العربية","text":[text("WESTERN ")]},
            wrapper(json!({"size":"base","sizeCs":"base"}),json!([text("RESET")])),
            wrapper(json!({"rtl":true}),json!([wrapper(json!({"rtl":false}),json!([{"t":"code","v":" CLEARED"}]))]))
        ])),{"t":"image","src":"العربية.png","alt":"العربية"},{"t":"unknown","v":"العربية"}
    ]));
    caption["publicationStyle"]=json!([caption["publicationStyle"].clone(),{"text":{"size":17.5,"author":{"size":30}}}]);
    let blocks=json!([caption]);let before=blocks.clone();publication_paragraph::validate(&blocks,&style()).unwrap();assert_eq!(blocks,before);
    let legacy=json!([caption_block(json!({}),json!([text("العربية")]))]);publication_paragraph::validate(&legacy,&style()).unwrap();
    let layout=crate::publication_blocks::comment(&json!({"id":"pure-shaping-caption","text":{"author":{"size":30,"sizeCs":10}}})).unwrap();
    let source=format!("| C |\n|---|\n| V |\n\n{layout}\n\n:label[tbl:pure] A العربية\n");let original=source.clone();let error=validate_caption_author_source(&source,&style()).unwrap_err();assert!(error.starts_with("author_inline_text_pdf_shaping_unsupported: "));assert_eq!(source,original);
    for example in [format!("```md\n{source}\n```\n"),format!("<pre>\n{source}\n</pre>\n")]{validate_caption_author_source(&example,&style()).unwrap();}
}

#[test]
fn caption_shaping_preflight_bounds_its_pure_walk_before_resources(){
    use std::cell::Cell;
    let directory=tempfile::tempdir().unwrap();let target=directory.path().join("protected-budget.pdf");std::fs::write(&target,b"KEEP BUDGET TARGET").unwrap();
    let blocks=json!([
        {"type":"paragraph","text":[{"t":"image","src":"resource-probe.png"}]},
        caption_block(json!({"size":30}),Value::Array(vec![text("x");crate::publication_inline_text::MAX_NODES+1]))
    ]);assert_eq!(publication_paragraph::validate_caption_author(&blocks,&style()).unwrap_err().message,crate::publication_inline_text::BUDGET);
    let calls=Cell::new(0);let mut warnings=Vec::new();let error=render(&blocks,target.to_str().unwrap(),&style(),directory.path().to_str().unwrap(),&|_|{calls.set(calls.get()+1);None},&mut warnings).unwrap_err();
    assert_eq!(error.message,crate::publication_inline_text::BUDGET);assert_eq!(calls.get(),0);assert!(warnings.is_empty());assert_eq!(std::fs::read(&target).unwrap(),b"KEEP BUDGET TARGET");assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(),1);
}
