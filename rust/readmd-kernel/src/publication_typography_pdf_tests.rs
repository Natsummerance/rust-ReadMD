//! Real PDF content streams and formula outlines, not style-value mirrors.
use super::*;
use serde_json::{json,Value};
use std::sync::Arc;

fn raw(mode:Value)->Value {
    json!({"frontMatter":{"mode":"none"},"cover":{"enabled":false},"toc":{"enabled":false},
        "footer":{"pageNumbers":false,"text":""},"header":{"text":""},
        "typography":{"size":"11pt","spacing":0,"lineSpacing":mode}})
}
fn block()->Value {json!([{"type":"paragraph","text":[{"t":"text","v":"FIRST"},{"t":"br"},
    {"t":"text","v":"SECOND"},{"t":"br"},{"t":"text","v":"THIRD"}]}])}
fn document(blocks:&Value,options:&Value)->RenderedDoc {
    build(blocks,&crate::export_styles::sanitize(options),"",&|_|None,&mut Vec::new()).unwrap()
}
fn op_baselines(bytes:&[u8])->Vec<f64> {
    let doc=lopdf::Document::load_mem(bytes).unwrap();
    let mut values=Vec::new();
    for id in doc.get_pages().values() {
        let content=lopdf::content::Content::decode(&doc.get_page_content(*id).unwrap()).unwrap();
        let mut render_mode=0;
        let mut saved_modes=Vec::new();
        for operation in content.operations {
            match operation.operator.as_str() {
                "q"=>saved_modes.push(render_mode),
                "Q"=>render_mode=saved_modes.pop().expect("balanced PDF graphics state"),
                "Tr"=>render_mode=operation.operands[0].as_i64().unwrap(),
                _=>{},
            }
            // Math's invisible ToUnicode copy layer is positioned at its
            // bounding-box bottom; it is not a visible paragraph baseline.
            if operation.operator=="Tm" && render_mode!=3 {
                let value=operation.operands[5].as_float().unwrap() as f64;
                if values.last().is_none_or(|last:&f64| (*last-value).abs()>0.001) {values.push(value);}
            }
        }
    }
    values
}
fn assert_gap(bytes:&[u8],expected:f64) {
    let baselines=op_baselines(bytes); assert_eq!(baselines.len(),3,"{baselines:?}");
    for pair in baselines.windows(2) { assert!((pair[0]-pair[1]-expected).abs()<0.004,"{baselines:?} expected {expected}"); }
    let pdf=lopdf::Document::load_mem(bytes).unwrap();let text=pdf.extract_text(&[1]).unwrap();
    for marker in ["FIRST","SECOND","THIRD"] {assert_eq!(text.matches(marker).count(),1,"{text}");}
}

#[test]
fn p04_pdf_multiple_exact_atleast_have_actual_pt_baselines() {
    for (mode,expected) in [(json!({"mode":"multiple","multiple":1.5}),16.5),
        (json!({"mode":"exact","pt":18}),18.),(json!({"mode":"atLeast","pt":22}),22.)] {
        let doc=document(&block(),&raw(mode));assert_gap(&write_pdf(&doc),expected);
    }
}

#[test]
fn p04_pdf_local_exact_pt_is_not_scaled_by_local_font_size() {
    for size in [8.,11.,16.] {
        let mut blocks=block();blocks[0]["publicationStyle"]=json!({"id":"local-p04","text":{"size":size},
            "paragraph":{"lineSpacing":{"mode":"exact","pt":24}}});
        let doc=document(&blocks,&raw(json!({"mode":"multiple","multiple":1.6})));
        assert_gap(&write_pdf(&doc),24.);
        let sizes=doc.pages[0].items.iter().filter_map(|item|if let DrawItem::Text{spans,..}=item{Some(spans)}else{None})
            .flatten().map(|span|span.size).collect::<Vec<_>>();
        assert!(!sizes.is_empty());assert!(sizes.iter().all(|actual|(*actual-size).abs()<1e-9));
    }
}

#[test]
fn p04_pdf_nested_style_layers_obey_sparse_inheritance_and_explicit_legacy() {
    let mut blocks=block();blocks[0]["publicationStyle"]=json!([
        {"id":"outer","paragraph":{"lineSpacing":{"mode":"exact","pt":30}}},
        {"id":"inner","text":{"size":12},"paragraph":{"lineHeight":1.5}}]);
    assert_gap(&write_pdf(&document(&blocks,&raw(json!({"mode":"atLeast","pt":40})))),18.);
    blocks[0]["publicationStyle"]=json!([{"text":{"size":12}},
        {"paragraph":{"lineSpacing":{"mode":"exact","pt":24}}}]);
    assert_gap(&write_pdf(&document(&blocks,&raw(json!({"mode":"atLeast","pt":40})))),24.);
}

fn tall()->Value {
    let omml=r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math" xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><m:r><w:rPr><w:sz w:val="96"/></w:rPr><m:t>x</m:t></m:r></m:oMath>"#;
    json!([{"type":"paragraph","text":[{"t":"text","v":"FIRST"},{"t":"br"},
        {"t":"text","v":"SECOND"},{"t":"math","latex":"x","omml":omml},{"t":"br"},{"t":"text","v":"THIRD"}]}])
}
fn formula(doc:&RenderedDoc)->(f64,&Arc<crate::math_layout::Formula>) {
    doc.pages.iter().flat_map(|page|&page.items).find_map(|item|if let DrawItem::Formula{y,f,..}=item{Some((*y,f))}else{None}).unwrap()
}

#[test]
fn p04_pdf_atleast_and_multiple_expand_for_fixed_48pt_formula_without_rescaling_it() {
    let oracle:Value=serde_json::from_str(include_str!("../tests/fixtures/math-font-geometry-stix.json")).unwrap();
    let name=oracle["characters"]["𝑥"].as_str().unwrap();
    let expected=oracle["glyphs"][name]["advance"].as_f64().unwrap()*48./oracle["unitsPerEm"].as_f64().unwrap();
    for mode in [json!({"mode":"atLeast","pt":18}),json!({"mode":"multiple","multiple":1.5})] {
        let doc=document(&tall(),&raw(mode)); let (baseline,f)=formula(&doc);
        assert!((f.width-expected).abs()<0.004,"{} expected {expected}",f.width);
        let ys=op_baselines(&write_pdf(&doc));assert_eq!(ys.len(),3);
        assert!((ys[1]-baseline).abs()<0.004);
        assert!(ys[0]-ys[1] >= 11.*0.207+f.height-0.004,"{ys:?}");
        assert!(ys[1]-ys[2] >= f.depth+11.*0.718-0.004,"{ys:?}");
        assert!(ys[0]-ys[1]>18.);
        let outline=f.draw(0.,0.,"0 0 0");assert!(outline.contains(" c")||outline.contains(" l"));
        let pdf=lopdf::Document::load_mem(&write_pdf(&doc)).unwrap();
        let operations=lopdf::content::Content::decode(&pdf.get_page_content(*pdf.get_pages().values().next().unwrap()).unwrap()).unwrap();
        assert!(operations.operations.iter().any(|op|op.operator=="c"||op.operator=="l"));
    }
}

#[test]
fn p04_pdf_exact_tall_collision_has_structured_block_and_line_geometry() {
    let mut blocks=tall();blocks[0]["publicationStyle"]=json!({"id":"tall-p04"});
    let error=build(&blocks,&crate::export_styles::sanitize(&raw(json!({"mode":"exact","pt":18}))),"",&|_|None,&mut Vec::new()).unwrap_err();
    let body=crate::latex_compile::error_body(&error.message);
    assert_eq!(body["error_code"],"publication_layout_geometry");
    assert_eq!(body["details"]["coordinateSpace"],"pdfParagraphLine");assert_eq!(body["details"]["blockIndex"],1);
    assert_eq!(body["details"]["blockId"],"tall-p04");assert_eq!(body["details"]["line"],2);
    assert_eq!(body["details"]["mode"],"exact");assert_eq!(body["details"]["requestedPt"],18.);
    assert!(body["details"]["requiredPt"].as_f64().unwrap()>18.);
}

#[test]
fn p04_pdf_exact_huge_line_fails_before_writing_the_existing_target() {
    let root=tempfile::tempdir().unwrap();let target=root.path().join("protected.pdf");
    std::fs::write(&target,b"ORIGINAL PDF TARGET").unwrap();
    let mut blocks=block();blocks[0]["publicationStyle"]=json!({"text":{"size":72},"paragraph":{"lineSpacing":{"mode":"exact","pt":6}}});
    let error=render(&blocks,target.to_str().unwrap(),&crate::export_styles::sanitize(&raw(json!({"mode":"exact","pt":18}))),"",&|_|None,&mut Vec::new()).unwrap_err();
    assert!(error.message.starts_with("publication_layout_geometry: "));assert_eq!(std::fs::read(target).unwrap(),b"ORIGINAL PDF TARGET");
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(),1);
}

#[test]
fn p04_pdf_body_mode_does_not_change_heading_list_quote_code_note_or_csl_story_defaults() {
    let source="# HEADING\n\nBody first.  \nBody second.\n\n- LIST first.  \n  LIST second.\n\n> QUOTE first.  \n> QUOTE second.\n\n```text\nCODE first\nCODE second\n```\n\nA note.[^a]\n\n[^a]: NOTE first.  \n    NOTE second.\n";
    let ast=crate::md_ast::parse(source);let values=crate::md_ast::to_publication_render_json(&ast.blocks);
    let typed=crate::export_styles::sanitize(&raw(json!({"mode":"exact","pt":24})));
    let mut legacy=raw(Value::Null);legacy["typography"].as_object_mut().unwrap().remove("lineSpacing");
    legacy["typography"]["lineHeight"]=json!(1.6);let legacy=crate::export_styles::sanitize(&legacy);
    let font=register_fonts();let geom=DocGeometry::from_style(&typed);
    let before=build_story(&values,&legacy,"",&|_|None,&Styles::build(&legacy,&font),&font,&geom,&mut Vec::new(),&[]);
    let after=build_story(&values,&typed,"",&|_|None,&Styles::build(&typed,&font),&font,&geom,&mut Vec::new(),&[]);
    fn special(story:&[Flowable])->Vec<String>{story.iter().filter_map(|flow|match flow{
        Flowable::Paragraph{runs,st,..} if st.name!="Body"||runs_plain_text(runs).contains("NOTE")=>Some(format!("{:?}:{:?}:{:?}",st.name,st.leading,st.line_spacing)),
        Flowable::Preformatted{st,..}=>Some(format!("code:{}:{:?}",st.leading,st.line_spacing)),
        Flowable::Table(table)=>Some(format!("table:{table:?}")),_=>None}).collect()}
    assert_eq!(special(&before),special(&after));assert!(!special(&before).is_empty());
    for flow in after {if let Flowable::Paragraph{runs,st,..}=flow{let text=runs_plain_text(&runs);
        if text.starts_with("Body first") {assert_eq!(st.line_spacing,Some(crate::publication_typography::LineSpacing::ExactPt(24.)));}
        if text.contains("NOTE") {assert!(st.line_spacing.is_none());}
    }}
}

#[test]
fn p04_pdf_legacy_without_new_parameters_preserves_all_old_draw_records() {
    let source="# Legacy\n\nA legacy **bold** paragraph.  \nSame story second line.\n\n- Item one\n- Item two\n\n> Quoted text\n";
    let ast=crate::md_ast::parse(source);let old=crate::md_ast::to_render_json(&ast.blocks);
    let projected=crate::md_ast::to_publication_render_json(&ast.blocks);
    let style=crate::export_styles::sanitize(&json!({"frontMatter":{"mode":"none"},"typography":{"lineHeight":1.55}}));
    let a=build(&old,&style,"",&|_|None,&mut Vec::new()).unwrap();
    let b=build(&projected,&style,"",&|_|None,&mut Vec::new()).unwrap();
    assert_eq!(format!("{:?}",a.pages),format!("{:?}",b.pages));
    assert_eq!(op_baselines(&write_pdf(&a)),op_baselines(&write_pdf(&b)));
}

#[test]
fn p04_pdf_styled_json_is_metadata_only_and_id_only_declarations_do_not_add_it() {
    let source="<!-- readmd:layout v1 {\"id\":\"p04-identity\"} -->\n\nReadable text.\n";
    let ast=crate::md_ast::parse(source);let json=crate::md_ast::to_render_json(&ast.blocks);
    assert!(json.as_array().unwrap().iter().all(|b|b.get("publicationStyle").is_none()));
    let styled="<!-- readmd:layout v1 {\"id\":\"p04-style\",\"paragraph\":{\"lineSpacing\":{\"mode\":\"exact\",\"pt\":18}}} -->\n\nReadable text.\n";
    let ast=crate::md_ast::parse(styled);let values=crate::md_ast::to_publication_render_json(&ast.blocks);
    assert_eq!(values[0]["publicationStyle"]["id"],"p04-style");
    assert_eq!(crate::md_ast::inline_plain(if let crate::md_ast::Block::Styled{block,..}=&ast.blocks[0]{
        if let crate::md_ast::Block::Paragraph(inlines)=block.as_ref(){inlines}else{panic!("paragraph")}
    }else{panic!("style")}),"Readable text.");
    let bytes=write_pdf(&document(&values,&raw(Value::Null)));
    let pdf=lopdf::Document::load_mem(&bytes).unwrap();let text=pdf.extract_text(&[1]).unwrap();
    assert!(text.contains("Readable text."));assert!(!text.contains("publicationStyle")&&!text.contains("readmd:layout")&&!text.contains("p04-style"));
    assert!(styled.starts_with("<!-- readmd:layout"));
}

#[test]
fn p04_pdf_direct_json_rejects_invalid_or_container_spacing_before_image_resolution() {
    let calls=std::cell::Cell::new(0);let resolve=|_:&str|{calls.set(calls.get()+1);None};
    for attributes in [json!({"paragraph":{"lineSpacing":{"mode":"exact","pt":18},"lineHeight":1.5}}),
        json!({"paragraph":{"lineSpacing":{"mode":"exact","pt":18,"extra":true}}})] {
        let values=json!([{"type":"paragraph","publicationStyle":attributes,"text":[{"t":"image","src":"missing.png"}]}]);
        assert!(build(&values,&json!({}),"",&resolve,&mut Vec::new()).is_err());
    }
    let values=json!([{"type":"quote","blocks":[{"type":"paragraph","publicationContext":"quote",
        "publicationStyle":{"paragraph":{"lineSpacing":{"mode":"atLeast","pt":20}}},"text":[]}]}]);
    assert_eq!(build(&values,&json!({}),"",&resolve,&mut Vec::new()).unwrap_err().message,crate::publication_typography::UNSUPPORTED);
    assert_eq!(calls.get(),0);
    for context in [None,Some("body"),Some("quote")] {
        let mut values=json!([{"type":"quote","blocks":[{"type":"paragraph",
            "publicationStyle":{"paragraph":{"lineSpacing":{"mode":"exact","pt":18}}},
            "text":[{"t":"image","src":"missing.png"}]}]}]);
        if let Some(context)=context{values[0]["blocks"][0]["publicationContext"]=json!(context);}
        let error=build(&values,&json!({}),"",&resolve,&mut Vec::new()).unwrap_err();
        assert_eq!(error.message,crate::publication_typography::UNSUPPORTED);assert_eq!(calls.get(),0);
    }
}

#[test]
fn p04_pdf_geometry_error_parser_keeps_only_valid_structured_numeric_details() {
    let good=json!({"coordinateSpace":"pdfParagraphLine","blockIndex":1,"blockId":"abc","line":2,"mode":"exact",
        "requestedPt":18,"requiredPt":42,"availableWidthPt":400,"availableHeightPt":600});
    assert!(crate::latex_compile::error_body(&format!("publication_layout_geometry: {good}"))["details"].is_object());
    for (key,value) in [("line",json!(0)),("mode",json!("legacy")),("requiredPt",json!("42")),("extra",json!(true))] {
        let mut bad=good.clone();bad[key]=value;
        assert!(crate::latex_compile::error_body(&format!("publication_layout_geometry: {bad}"))["details"].is_string());
    }
    let mut baseline=good.clone();baseline["mode"]=json!("legacy");baseline["baselineShiftPt"]=json!(-2.25);
    assert_eq!(crate::latex_compile::error_body(&format!("publication_layout_geometry: {baseline}"))["details"]["baselineShiftPt"],json!(-2.25));
    for value in [json!(-72.01),json!(72.01),json!("2"),json!(false),Value::Null] {
        baseline["baselineShiftPt"]=value;
        assert!(crate::latex_compile::error_body(&format!("publication_layout_geometry: {baseline}"))["details"].is_string());
    }
}

#[test]
fn p04_pdf_typed_pagination_keeps_all_lines_in_page_bounds_and_preserves_order() {
    let nodes=(0..150).flat_map(|index|[json!({"t":"text","v":format!("P04LINE{index:03}")}),json!({"t":"br"})]).collect::<Vec<_>>();
    let doc=document(&json!([{"type":"paragraph","text":nodes}]),&raw(json!({"mode":"exact","pt":18})));
    assert!(doc.pages.len()>1);let text=doc.pages.iter().map(|page|page.text.as_str()).collect::<String>();
    let mut last=0;for index in 0..150{let marker=format!("P04LINE{index:03}");assert_eq!(text.matches(&marker).count(),1);
        let next=text.find(&marker).unwrap();assert!(next>=last);last=next;}
    for page in &doc.pages {for item in &page.items {if let DrawItem::Text{y,text,..}=item{
        if text.starts_with("P04LINE") {assert!(*y>=doc.geom.bottom+FRAME_PAD+11.*0.207-0.004);
            assert!(*y<=doc.geom.page_h-doc.geom.top-FRAME_PAD-11.*0.718+0.004);}
    }}}
}

#[test]
fn p04_pdf_cancellation_after_preflight_propagates_without_panicking() {
    use std::sync::atomic::{AtomicBool,Ordering};
    let options=crate::export_styles::sanitize(&raw(json!({"mode":"exact","pt":18})));
    let styles=Styles::build(&options,"Helvetica");let geom=DocGeometry::from_style(&options);
    let mut paragraph=styles.body.clone();paragraph.line_spacing=Some(crate::publication_typography::LineSpacing::ExactPt(18.));
    let runs=vec![text_run("PREFLIGHT PASSED",FaceStyle::Regular,11.,paragraph.color)];
    publication_paragraph::plan(&runs,&paragraph,geom.avail,geom.height-2.*FRAME_PAD).unwrap();
    let token=Arc::new(AtomicBool::new(false));token.store(true,Ordering::Release);
    let observed=token.clone();let story=vec![Flowable::Paragraph{runs,st:paragraph,anchor:None}];
    let error=crate::speech::with_cancel(Arc::new(move||observed.load(Ordering::Acquire)),||
        paginate(&story,&geom,&styles,&mut ExportDoc::new())).unwrap_err();
    assert_eq!(error.message,"cancelled");
}

#[test]
fn p04_pdf_changed_frame_after_preflight_revalidates_real_width_and_returns_geometry() {
    let options=crate::export_styles::sanitize(&raw(json!({"mode":"exact","pt":18})));
    let styles=Styles::build(&options,"Helvetica");let original=DocGeometry::from_style(&options);
    let mut paragraph=styles.body.clone();paragraph.line_spacing=Some(crate::publication_typography::LineSpacing::ExactPt(18.));
    paragraph.publication_block=Some((2,Some("narrow-frame".into())));
    let runs=vec![text_run("INDIVISIBLE",FaceStyle::Regular,11.,paragraph.color)];
    publication_paragraph::plan(&runs,&paragraph,original.avail,original.height-2.*FRAME_PAD).unwrap();
    let mut narrow=original.clone();narrow.avail=5.;
    let story=vec![Flowable::Paragraph{runs,st:paragraph,anchor:None}];
    let error=paginate(&story,&narrow,&styles,&mut ExportDoc::new()).unwrap_err();
    let body=crate::latex_compile::error_body(&error.message);
    assert_eq!(body["error_code"],"publication_layout_geometry");assert_eq!(body["details"]["blockId"],"narrow-frame");
    assert_eq!(body["details"]["availableWidthPt"],5.);assert!(body["details"]["requiredPt"].as_f64().unwrap()>5.);
}

#[test]
fn p04_pdf_hanging_indent_must_stay_in_the_real_frame_and_balanced_left_indent_is_valid() {
    let mut values=json!([{"type":"paragraph","publicationStyle":{"id":"hanging",
        "paragraph":{"hanging":40,"lineSpacing":{"mode":"exact","pt":18}}},
        "text":[{"t":"text","v":"HANG"}]}]);
    let style=crate::export_styles::sanitize(&raw(Value::Null));
    let error=build(&values,&style,"",&|_|None,&mut Vec::new()).unwrap_err();
    let body=crate::latex_compile::error_body(&error.message);assert_eq!(body["error_code"],"publication_layout_geometry");
    assert_eq!(body["details"]["blockId"],"hanging");
    values[0]["publicationStyle"]["paragraph"]["left"]=json!(40);
    let doc=build(&values,&style,"",&|_|None,&mut Vec::new()).unwrap();
    let x=doc.pages[0].items.iter().find_map(|item|if let DrawItem::Text{x,text,..}=item{
        text.contains("HANG").then_some(*x)}else{None}).unwrap();
    assert!((x-doc.geom.left-FRAME_PAD).abs()<0.004);
    assert!(lopdf::Document::load_mem(&write_pdf(&doc)).unwrap().extract_text(&[1]).unwrap().contains("HANG"));
}

#[test]
fn p04_pdf_cancel_after_serialization_cannot_commit_over_the_existing_file() {
    use std::sync::atomic::{AtomicBool,Ordering};
    let options=crate::export_styles::sanitize(&raw(json!({"mode":"exact","pt":18})));
    let bytes=render_bytes(&block(),&options,"",&|_|None,&mut Vec::new()).unwrap();
    assert_eq!(lopdf::Document::load_mem(&bytes).unwrap().get_pages().len(),1);
    let root=tempfile::tempdir().unwrap();let target=root.path().join("Cancelled.pdf");
    std::fs::write(&target,b"OLD TARGET AFTER SERIALIZATION").unwrap();
    let token=Arc::new(AtomicBool::new(false));token.store(true,Ordering::Release);
    let observed=token.clone();let error=crate::speech::with_cancel(Arc::new(move||observed.load(Ordering::Acquire)),||
        commit_rendered(&bytes,target.to_str().unwrap())).unwrap_err();
    assert_eq!(error.message,"cancelled");assert_eq!(std::fs::read(target).unwrap(),b"OLD TARGET AFTER SERIALIZATION");
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(),1);
}

#[test]
fn p04_pdf_accented_winansi_cannot_bypass_conservative_ink_clearance() {
    // Export uses original DejaVu 2.37: FontTools hhea (1901+483)/2048*11
    // gives 12.8046875pt. No-context Helvetica keeps its own conservative bound.
    let values=json!([{"type":"paragraph","text":[{"t":"text","v":"g"},{"t":"br"},{"t":"text","v":"Å"}]}]);
    let style=crate::export_styles::sanitize(&raw(json!({"mode":"exact","pt":11})));
    let error=build(&values,&style,"",&|_|None,&mut Vec::new()).unwrap_err();
    let body=crate::latex_compile::error_body(&error.message);
    assert_eq!(body["error_code"],"publication_layout_geometry");
    assert!((body["details"]["requiredPt"].as_f64().unwrap()-12.8046875).abs()<1e-9);
    for mode in [json!({"mode":"multiple","multiple":1}),json!({"mode":"atLeast","pt":6})] {
        let bytes=write_pdf(&document(&values,&raw(mode)));let ys=op_baselines(&bytes);
        assert_eq!(ys.len(),2);assert!((ys[0]-ys[1]-12.8046875).abs()<0.004,"{ys:?}");
        assert!(lopdf::Document::load_mem(&bytes).unwrap().extract_text(&[1]).unwrap().contains('Å'));
    }
    // The direct helper has no owned context. Preserve the existing locked
    // MuPDF outward whole-font Helvetica bound, 1.077em above + .300em below.
    assert_eq!(crate::pdf_fonts::proportional_advance('g',false,11.),None);
    let runs=inline_markup(&values[0]["text"],&style,"",&|_|None);
    let mut legacy=PStyle::base("Helvetica",11.,17.6);
    legacy.line_spacing=Some(crate::publication_typography::LineSpacing::Multiple(1.));
    let planned=publication_paragraph::plan(&runs,&legacy,400.,700.).unwrap();
    assert!((planned.offsets[1]-11.*(1.077+0.300)).abs()<1e-9);
}
