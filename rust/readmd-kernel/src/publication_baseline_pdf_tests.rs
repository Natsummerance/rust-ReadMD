//! Actual emitted PDF origins, preserved Unicode/font bytes and geometry.
use super::*;
use serde_json::json;

fn options(shift:Option<f64>)->Value {
    let mut value=json!({"frontMatter":{"mode":"none"},"typography":{"size":11,"lineHeight":1.6,"spacing":6},
        "footer":{"pageNumbers":false,"text":""},"header":{"text":""},"cover":{"enabled":false},"toc":{"enabled":false}});
    if let Some(shift)=shift {value["typography"]["baselineShiftPt"]=json!(shift);}
    crate::export_styles::sanitize(&value)
}
fn blocks()->Value {
    // Exercise the real adapter. Link children are carried in `text`, whereas
    // emphasis children use `v`; a hand-built wrong-key node paints no link.
    let parsed=crate::md_ast::parse("ORDINARY `CODE` [LINK](https://example.invalid/shift) $x^2+1$\n");
    crate::md_ast::to_publication_render_json(&parsed.blocks)
}
fn doc(blocks:&Value,shift:Option<f64>)->RenderedDoc {build(blocks,&options(shift),"",&|_|None,&mut Vec::new()).unwrap()}
fn text<'a>(doc:&'a RenderedDoc,marker:&str)->(f64,f64,&'a Vec<TextSpan>) {
    let found=doc.pages.iter().flat_map(|p|&p.items).filter_map(|item|match item{
        DrawItem::Text{x,y,text,spans} if text.contains(marker)=>Some((*x,*y,spans)),_=>None}).collect::<Vec<_>>();
    assert_eq!(found.len(),1,"{marker}");found[0]
}
fn marked_span<'a>(doc:&'a RenderedDoc,marker:&str)->(f64,f64,&'a TextSpan) {
    let mut found=Vec::new();
    for item in doc.pages.iter().flat_map(|page|&page.items) {
        if let DrawItem::Text {x,y,spans,..}=item {
            let mut pen=*x;
            for span in spans {
                let decoded=span_plain(doc,std::slice::from_ref(span)).unwrap();
                if decoded.contains(marker) {
                    // These markers start their own face/colour span; preceding
                    // code or ordinary text may share a DrawItem, never a font.
                    assert!(decoded.starts_with(marker),"{decoded:?}");
                    found.push((pen,*y,span));
                }
                pen+=span.advance;
            }
        }
    }
    assert_eq!(found.len(),1,"{marker}");found[0]
}
fn visible_pdf_origins(bytes:&[u8])->Vec<(f64,f64)> {
    let pdf=lopdf::Document::load_mem(bytes).unwrap();let mut origins=Vec::new();
    for id in pdf.get_pages().values() {
        let content=lopdf::content::Content::decode(&pdf.get_page_content(*id).unwrap()).unwrap();let mut mode=0;let mut stack=Vec::new();
        for op in content.operations {match op.operator.as_str(){"q"=>stack.push(mode),"Q"=>mode=stack.pop().unwrap(),"Tr"=>mode=op.operands[0].as_i64().unwrap(),
            "Tm" if mode!=3=>origins.push((op.operands[4].as_float().unwrap()as f64,op.operands[5].as_float().unwrap()as f64)),_=>()}}
    }
    origins
}

#[test]
fn p04_baseline_pdf_actual_text_matrices_move_signed_pt_without_font_or_advance_changes() {
    let base=doc(&blocks(),Some(0.));let base_bytes=write_pdf(&base);
    for shift in [3.,-2.5] {
        let shifted=doc(&blocks(),Some(shift));let bytes=write_pdf(&shifted);let origins=visible_pdf_origins(&bytes);
        for marker in ["ORDINARY","LINK"] {
            let (x,y,span)=marked_span(&shifted,marker);let (bx,by,bspan)=marked_span(&base,marker);
            assert!((x-bx).abs()<1e-9);assert!((y-by-shift).abs()<1e-9);
            assert_eq!(span.bytes,bspan.bytes);assert_eq!(span.face,bspan.face);assert_eq!(span.size,bspan.size);assert_eq!(span.color,bspan.color);assert!((span.advance-bspan.advance).abs()<1e-9);
            assert!(origins.iter().any(|(px,py)|(px-x).abs()<0.002&&(py-y).abs()<0.002),"Visible PDF Tm missing {marker} at {x},{y}");
        }
        let (cx,cy,cspan)=marked_span(&shifted,"CODE");let (bcx,bcy,bcspan)=marked_span(&base,"CODE");
        assert_eq!((cx,cy),(bcx,bcy));assert_eq!(cspan.size,bcspan.size);assert_eq!(cspan.bytes,bcspan.bytes);assert_eq!(cspan.advance,bcspan.advance);
        let formula=|doc:&RenderedDoc|doc.pages.iter().flat_map(|p|&p.items).find_map(|item|match item {DrawItem::Formula{x,y,f,..}=>Some((*x,*y,f.width,f.height,f.depth,f.draw(0.,0.,"0 0 0"))),_=>None}).unwrap();
        assert_eq!(formula(&shifted),formula(&base));
        let link=|doc:&RenderedDoc|doc.pages.iter().flat_map(|p|&p.items).find_map(|item|match item {DrawItem::Link{x,y,w,h,href}if href=="https://example.invalid/shift"=>Some((*x,*y,*w,*h)),_=>None}).unwrap();
        let (x,y,w,h)=link(&shifted);let(bx,by,bw,bh)=link(&base);assert_eq!((x,w,h),(bx,bw,bh));assert!((y-by-shift).abs()<1e-9);
        let pdf=lopdf::Document::load_mem(&bytes).unwrap();let extracted=pdf.extract_text(&[1]).unwrap();for marker in ["ORDINARY","CODE","LINK"]{assert_eq!(extracted.matches(marker).count(),1);}
        let control=lopdf::Document::load_mem(&base_bytes).unwrap().extract_text(&[1]).unwrap();
        assert_eq!(extracted.chars().filter(|c|!c.is_whitespace()).collect::<String>(),control.chars().filter(|c|!c.is_whitespace()).collect::<String>());
        assert_eq!(shifted.embedded.fonts.len(),base.embedded.fonts.len());
        for(actual,control)in shifted.embedded.fonts.iter().zip(&base.embedded.fonts){match(actual,control){
            (Some(actual),Some(control))=>{assert_eq!(actual.file,control.file);assert_eq!(actual.unicode,control.unicode);},(None,None)=>(),_=>panic!("Font roles changed")}}
    }
}

#[test]
fn p04_baseline_pdf_layers_zero_restore_and_semantic_sup_sub_are_independent() {
    let source=json!([{"type":"paragraph","publicationStyle":[{"text":{"baselineShiftPt":-2.5}},{"text":{"baselineShiftPt":0}}],"text":[{"t":"text","v":"RESET"}]}]);
    let reset=doc(&source,Some(3.));let plain=doc(&json!([{"type":"paragraph","text":[{"t":"text","v":"RESET"}]}]),None);
    assert_eq!(text(&reset,"RESET").0,text(&plain,"RESET").0);assert_eq!(text(&reset,"RESET").1,text(&plain,"RESET").1);
    let styled=json!([{"type":"paragraph","text":[{"t":"text","v":"NORMAL "},{"t":"styled","format":{"vertical_align":"sup"},"v":[{"t":"text","v":"SUP"}]},{"t":"text","v":" "},{"t":"styled","format":{"vertical_align":"sub"},"v":[{"t":"text","v":"SUB"}]}]}]);
    let base=doc(&styled,Some(0.));let raised=doc(&styled,Some(3.));
    for marker in ["NORMAL","SUP","SUB"] {let(x,y,spans)=text(&raised,marker);let(bx,by,bspans)=text(&base,marker);assert_eq!(x,bx);assert!((y-by-3.).abs()<1e-9);assert_eq!(spans[0].size,bspans[0].size);}
    let(_,normal,_) =text(&raised,"NORMAL");let(_,sup,s)=text(&raised,"SUP");let(_,sub,b)=text(&raised,"SUB");
    assert!((sup-normal-11.*0.35).abs()<1e-9);assert!((sub-normal+11.*0.2).abs()<1e-9);assert!((s[0].size-7.7).abs()<1e-9);assert!((b[0].size-7.7).abs()<1e-9);
}

#[test]
fn p04_baseline_pdf_extreme_and_invalid_geometry_preserve_old_target_before_resource_calls() {
    use std::cell::Cell;let root=tempfile::tempdir().unwrap();let target=root.path().join("protected.pdf");std::fs::write(&target,b"KEEP PDF").unwrap();let calls=Cell::new(0);let resolve=|_:&str|{calls.set(calls.get()+1);None};
    for (blocks,style,code) in [
        (json!([{"type":"paragraph","text":[{"t":"image","src":"not-opened.png"}]}]),json!({"typography":{"baselineShiftPt":"3"}}),crate::publication_typography::INVALID),
        (json!([{"type":"quote","blocks":[{"type":"paragraph","publicationContext":"body","publicationStyle":{"text":{"baselineShiftPt":0}},"text":"BODY"}]}]),json!({}),crate::publication_typography::baseline::UNSUPPORTED),
    ] {let error=render(&blocks,target.to_str().unwrap(),&style,"",&resolve,&mut Vec::new()).unwrap_err();assert_eq!(error.message,code);assert_eq!(std::fs::read(&target).unwrap(),b"KEEP PDF");}
    assert_eq!(calls.get(),0);
    let mut style=options(Some(72.));style["page"]["marginTop"]=json!(0.);
    let error=render(&json!([{"type":"paragraph","text":[{"t":"text","v":"BEYOND PAPER"}]}]),target.to_str().unwrap(),&style,"",&resolve,&mut Vec::new()).unwrap_err();
    assert!(error.message.starts_with("publication_layout_geometry:"));assert_eq!(std::fs::read(&target).unwrap(),b"KEEP PDF");
}

#[test]
fn p04_baseline_pdf_generated_metadata_front_keeps_real_glyph_origins_while_body_moves() {
    let source="---\ntitle: FRONTTITLE\nsubtitle: FRONTSUBTITLE\nlanguage: en\n---\n\nBODYONLY\n";
    let owned=tempfile::tempdir().unwrap();let mut outputs=Vec::new();
    for (name,shift)in [("legacy",None),("zero",Some(0.)),("raised",Some(3.))] {
        let mut style=options(shift);style["frontMatter"]=json!({"mode":"paper"});
        let path=owned.path().join(format!("{name}.pdf"));
        crate::mdexport::export_pdf(source,owned.path().to_str().unwrap(),path.to_str().unwrap(),&style,"QA",owned.path()).unwrap();
        let bytes=std::fs::read(&path).unwrap();let pdf=lopdf::Document::load_mem(&bytes).unwrap();
        let extracted=pdf.extract_text(&[1]).unwrap();
        for marker in ["FRONTTITLE","FRONTSUBTITLE","BODYONLY"] {assert_eq!(extracted.matches(marker).count(),1,"{extracted}");}
        let origins=visible_pdf_origins(&bytes);assert_eq!(origins.len(),3,"{origins:?}");outputs.push(origins);
    }
    assert_eq!(outputs[0],outputs[1],"Absent and explicit-zero baseline changed legacy front/body layout");
    assert_eq!(&outputs[1][..2],&outputs[2][..2],"Global body baseline changed generated metadata glyphs");
    assert!((outputs[2][2].0-outputs[1][2].0).abs()<0.002);
    assert!((outputs[2][2].1-outputs[1][2].1-3.).abs()<0.002);
}

#[test]
fn p04_baseline_pdf_front_context_does_not_authorise_local_or_nested_overrides() {
    let calls=std::cell::Cell::new(0);let resolve=|_:&str|{calls.set(calls.get()+1);None};
    for block in [json!({"type":"paragraph","publicationContext":"frontMatter","publicationStyle":{"text":{"baselineShiftPt":0}},"text":[{"t":"image","src":"not-read.png"}]}),
        json!({"type":"quote","blocks":[{"type":"paragraph","publicationContext":"frontMatter","publicationStyle":{"text":{"baselineShiftPt":0}},"text":"NOT AUTHORISED"}]})] {
        assert_eq!(build(&json!([block]),&options(Some(3.)),"",&resolve,&mut Vec::new()).unwrap_err().message,crate::publication_typography::baseline::UNSUPPORTED);
    }
    assert_eq!(calls.get(),0);
}

#[test]
fn p04_baseline_pdf_large_signed_offsets_keep_first_glyph_rise_and_paginate_without_overlap() {
    let nodes=(0..28).flat_map(|index|[json!({"t":"text","v":format!("SHIFTROW{index:03}")}),json!({"t":"br"})]).collect::<Vec<_>>();
    let blocks=json!([{"type":"paragraph","text":nodes}]);let control=doc(&blocks,Some(0.));
    let first=text(&control,"SHIFTROW000").1;
    for shift in [50.,-50.] {
        let actual=doc(&blocks,Some(shift));assert!(actual.pages.len()>1);
        assert!((text(&actual,"SHIFTROW000").1-first-shift).abs()<1e-7,"Paragraph anchoring cancelled {shift}pt rise");
        let mut all=String::new();
        for page in &actual.pages {
            let mut previous:Option<f64>=None;
            for item in &page.items {if let DrawItem::Text{y,text,spans,..}=item{
                if !text.starts_with("SHIFTROW"){continue;}
                assert!(spans.iter().all(|span|(span.size-11.).abs()<1e-9));
                // Locked DejaVu original hhea: ascent=1901, depth=483,
                // upem=2048. These bounds are independent of the layout plan.
                let upper=1901.*11./2048.;let lower=483.*11./2048.;
                assert!(*y+upper<=page.h+0.004&&*y-lower>=-0.004,"Visible glyph beyond paper: {shift},{y}");
                if let Some(last)=previous{assert!(last-lower>=*y+upper-0.004,"Overlapping raised/lowered rows {last},{y}");}
                previous=Some(*y);all.push_str(text);
            }}
        }
        for index in 0..28{assert_eq!(all.matches(&format!("SHIFTROW{index:03}")).count(),1);}
        let bytes=write_pdf(&actual);let origins=visible_pdf_origins(&bytes);assert_eq!(origins.len(),28);
        let pdf=lopdf::Document::load_mem(&bytes).unwrap();let pages=pdf.get_pages().keys().copied().collect::<Vec<_>>();let extracted=pdf.extract_text(&pages).unwrap();
        for index in 0..28{assert_eq!(extracted.matches(&format!("SHIFTROW{index:03}")).count(),1);}
    }
}

#[test]
fn p04_baseline_pdf_positive_rise_cannot_paint_over_prior_paragraph_or_image() {
    let owned=tempfile::tempdir().unwrap();let target=owned.path().join("protected.pdf");std::fs::write(&target,b"KEEP ADJACENT CONTENT").unwrap();
    let blocks=json!([{"type":"paragraph","text":[{"t":"text","v":"PRIOR CONTENT"}]},
        {"type":"paragraph","publicationStyle":{"id":"colliding-baseline","text":{"baselineShiftPt":50}},"text":[{"t":"text","v":"RAISED CONTENT"}]}]);
    let error=render(&blocks,target.to_str().unwrap(),&options(None),"",&|_|None,&mut Vec::new()).unwrap_err();
    let body=crate::latex_compile::error_body(&error.message);assert_eq!(body["error_code"],"publication_layout_geometry");assert_eq!(body["details"]["blockId"],"colliding-baseline");assert_eq!(body["details"]["blockIndex"],2);assert_eq!(body["details"]["baselineShiftPt"],50.);
    assert_eq!(std::fs::read(&target).unwrap(),b"KEEP ADJACENT CONTENT");
    let style=options(None);let styles=Styles::build(&style,"Helvetica");let geom=DocGeometry::from_style(&style);let mut export=ExportDoc::new();let mut sheet=Sheet::new(&geom,&styles,&mut export);sheet.track_ink=true;
    sheet.add_image(&InlineImage {src:"owned-placeholder.png".into(),width:100.,height:50.,math:None});
    let mut raised=styles.body.clone();raised.baseline_shift=Some(crate::publication_typography::baseline::parse(&json!(50)).unwrap());
    let context=InlineCtx {face:raised.face,color:raised.color,href:None,size:raised.size,base_face:Face::Helvetica,text_style:TextStyle {baseline_shift_pt:50.,..TextStyle::default()}};
    let runs=inline_markup_ctx(&json!([{"t":"text","v":"RAISED AFTER IMAGE"}]),&style,"",&|_|None,&context);
    assert!(sheet.add_typed_paragraph(&runs,&raised,0.,None).unwrap_err().message.starts_with("publication_layout_geometry:"));
}
