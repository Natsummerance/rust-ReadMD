use super::*;
use serde_json::json;

#[test]
fn p04_baseline_physical_points_are_finite_bounded_and_quantised_only_at_docx_boundary() {
    for (pt, half) in [(-72.,-144),(-2.5,-5),(-0.25,-1),(-0.249,0),(0.,0),(0.249,0),(0.25,1),(3.,6),(72.,144)] {
        let parsed=parse(&json!(pt)).unwrap();assert_eq!(parsed.requested_pt(),pt);
        assert_eq!(parsed.docx_half_points(),half);assert_eq!(parsed.docx_actual_pt(),half as f64/2.);
    }
    for raw in [Value::Null,json!(true),json!("3"),json!("3pt"),json!([]),json!({"pt":3}),json!(-72.001),json!(72.001),json!(1e100)] {
        assert_eq!(parse(&raw),Err(INVALID),"{raw}");
        assert!(crate::publication_typography::validate_request(&json!({"typography":{"baselineShiftPt":raw}})).is_err() || raw.is_null());
    }
    assert!(serde_json::from_str::<Value>("1e999").is_err());
}

#[test]
fn p04_baseline_sparse_absence_zero_and_null_keep_distinct_parent_semantics() {
    let base=json!({"typography":{"baselineShiftPt":3,"spacing":9}});
    let parent=global(&base).unwrap();
    assert_eq!(resolve(parent,&json!({}),true),Ok(parent));
    assert_eq!(resolve(parent,&json!({}),false),Ok(None));
    assert_eq!(resolve(parent,&json!({"baselineShiftPt":0}),true).unwrap().unwrap().requested_pt(),0.);
    assert_eq!(local(&json!({"baselineShiftPt":null})),Err(INVALID));
    let inherited=crate::publication_metadata::merge_options(&base,&json!({"typography":{"baselineShiftPt":null}})).unwrap();
    assert_eq!(global(&inherited).unwrap(),parent);assert_eq!(inherited["typography"]["spacing"],9);
    let zero=crate::publication_metadata::merge_options(&base,&json!({"typography":{"baselineShiftPt":0}})).unwrap();
    assert_eq!(zero["typography"]["baselineShiftPt"],0);
    let plain=crate::export_styles::sanitize(&json!({"typography":{"spacing":9}}));
    assert!(plain["typography"].get("baselineShiftPt").is_none());
    assert_eq!(crate::publication_typography::request_error_field(&json!({"typography":{"baselineShiftPt":"3"}})),"typography.baselineShiftPt");
}

#[test]
fn p04_baseline_second_duplicate_edit_clear_and_inverse_preserve_exact_source() {
    let paragraph="Identical 中文 e\u{301} 😀.\r\n";
    let original=format!("\u{feff}# Heading\r\n\r\n{paragraph}\r\n{paragraph}");
    let model=crate::publication_blocks::analyze(&original).unwrap();
    let target=model.nodes.iter().filter(|node|node.kind=="paragraph").nth(1).unwrap();
    let result=crate::publication_blocks::apply(&original,&model.revision,&target.id,&json!({"text":{"baselineShiftPt":-2.5}})).unwrap();
    let edited=result["result"]["content"].as_str().unwrap();
    assert!(edited.starts_with(&format!("\u{feff}# Heading\r\n\r\n{paragraph}\r\n<!-- readmd:layout")));
    assert!(edited.ends_with(paragraph));assert_eq!(edited.matches(paragraph).count(),2);
    let fresh=crate::publication_blocks::analyze(edited).unwrap();
    assert_eq!(fresh.nodes.iter().find(|node|node.id==target.id).unwrap().style["text"]["baselineShiftPt"],-2.5);
    assert_eq!(crate::publication_blocks::apply(edited,&model.revision,&target.id,&json!({"text":{"baselineShiftPt":3}})).unwrap_err(),"transaction_conflict");
    let cleared=crate::publication_blocks::apply(edited,&fresh.revision,&target.id,&json!({"text":{"baselineShiftPt":null}})).unwrap();
    assert_eq!(cleared["result"]["content"].as_str().unwrap(),original);
    let edits=serde_json::from_value(result["result"]["inverse"].clone()).unwrap();
    let inverse=crate::document_transaction::ContentTransaction {base_revision:crate::document_transaction::revision(edited),origin:"editor".into(),edits};
    assert_eq!(crate::document_transaction::apply(edited,&inverse).unwrap().content,original);
}

#[test]
fn p04_baseline_consumer_capabilities_reject_even_zero_and_escaped_fields() {
    for pt in [0.,3.] {
        let options=json!({"typography":{"baselineShiftPt":pt}});
        for (format,engine) in [("docx","rust"),("pdf","rust")] {
            let mut request=options.clone();request["pdf"]=json!({"engine":engine});
            assert!(crate::publication_typography::validate_export(format,"Body.",&request).is_ok());
        }
        for (format,engine) in [("html","rust"),("epub","rust"),("tex","rust"),("pdf","typst"),("pdf","xelatex")] {
            let mut request=options.clone();request["pdf"]=json!({"engine":engine});
            assert_eq!(crate::publication_typography::validate_export(format,"Body.",&request),Err(UNSUPPORTED));
        }
        assert_eq!(crate::publication_typography::validate_preview("Body.",&options),Err(UNSUPPORTED));
    }
    let source="<!-- readmd:layout v1 {\"id\":\"escaped\",\"text\":{\"baselineShift\\u0050t\":0}} -->\n\nBody.\n";
    assert_eq!(validate_source(source,&json!({}),Consumer::Unsupported),Err(UNSUPPORTED));
    assert!(validate_source(source,&json!({}),Consumer::Reader).is_ok());
    assert!(used(source,&json!({})).unwrap());
    let math="<!-- readmd:layout v1 {\"id\":\"math\",\"text\":{\"baselineShiftPt\":0}} -->\n\n$$x$$\n";
    assert_eq!(validate_source(math,&json!({}),Consumer::Docx),Err(INVALID));
}

#[test]
fn p04_baseline_reader_map_offsets_only_text_without_changing_paragraph_geometry_css() {
    let source="<!-- readmd:layout v1 {\"id\":\"reader\",\"text\":{\"baselineShiftPt\":3}} -->\n\nBody `CODE` $x$.\n";
    let (content,rules)=crate::publication_blocks::view(source).unwrap();
    assert!(content.contains("Body `CODE` $x$."));let rule=rules.as_object().unwrap().values().next().unwrap();
    assert_eq!(rule["--readmd-baseline-shift"],"3pt");
    for forbidden in ["top","transform","font-size","position","vertical-align"] { assert!(rule.get(forbidden).is_none()); }
}

#[test]
fn p04_baseline_pdf_source_csl_boundary_is_checked_before_any_resource_projection() {
    let request=json!({"typography":{"baselineShiftPt":3}});
    assert_eq!(validate_source("BODY [@missing].",&request,Consumer::RustPdf),Err(UNSUPPORTED));
    assert!(validate_source("BODY `[@not-a-citation]` $x$.",&request,Consumer::RustPdf).is_ok());
    assert!(validate_source("BODY.\n\n> Dedicated [@not-shifted].",&request,Consumer::RustPdf).is_ok());
    let zero="<!-- readmd:layout v1 {\"id\":\"zero-csl\",\"text\":{\"baselineShiftPt\":0}} -->\n\nBODY [@reset].";
    assert!(validate_source(zero,&request,Consumer::RustPdf).is_ok());
    let owned=tempfile::tempdir().unwrap();let target=owned.path().join("protected.pdf");let data=owned.path().join("profile-not-created");let assets=owned.path().join("assets-not-created");
    std::fs::write(&target,b"KEEP CSL TARGET").unwrap();
    let error=crate::mdexport::export_document_with_data("pdf","---\nbibliography: missing-owned.json\n---\nBODY [@missing].",owned.path().to_str().unwrap(),target.to_str().unwrap(),&request,"QA",&assets,&data).unwrap_err();
    assert_eq!(error,UNSUPPORTED);assert_eq!(std::fs::read(&target).unwrap(),b"KEEP CSL TARGET");assert!(!data.exists()&&!assets.exists());
    assert_eq!(std::fs::read_dir(owned.path()).unwrap().count(),1);
}

#[test]
fn p04_baseline_absent_does_not_replace_legacy_unsupported_style_text_with_an_error_document() {
    use crate::md_ast::{Block,Document,Inline};
    for attributes in [json!({"text":{"font":"TeX Gyre Termes","unknown":true}}),
        json!({"text":{"font":"TeX Gyre Termes}\\input{injected}"}})] {
        let doc=Document {blocks:vec![Block::Quote(vec![Block::Styled {attributes:attributes.clone(),
            block:Box::new(Block::Paragraph(vec![Inline::Text("Preserved text.".into())]))}])],..Default::default()};
        for consumer in [Consumer::Docx,Consumer::RustPdf,Consumer::Reader,Consumer::Unsupported] {
            assert!(validate_document(&doc,&json!({}),consumer).is_ok(),"{consumer:?}: {attributes}");
        }
        // Strict source validation remains in force; compatibility belongs to
        // direct legacy writer ASTs, not a bypass for readable declarations.
        if attributes["text"].get("unknown").is_some() {
            assert_eq!(crate::publication_blocks::validate_style(&attributes),Err(INVALID));
            let mut opted_in=attributes.clone();opted_in["text"]["baselineShiftPt"]=json!(0);
            let strict=Document {blocks:vec![Block::Styled {attributes:opted_in,
                block:Box::new(Block::Paragraph(vec![Inline::Text("Invalid mixed request".into())]))}],..Default::default()};
            for consumer in [Consumer::Docx,Consumer::RustPdf,Consumer::Reader,Consumer::Unsupported] {
                assert_eq!(validate_document(&strict,&json!({}),consumer),Err(INVALID),"{consumer:?}");
            }
        }
        let tex=crate::latex_writer::render_document(&doc,&crate::latex_writer::LatexOptions::default(),&|_|None);
        assert!(tex.contains("Preserved text."));assert!(!tex.contains("injected"));
    }
    for value in [Value::Null,json!("3"),json!(73)] {
        let doc=Document {blocks:vec![Block::Styled {attributes:json!({"text":{"baselineShiftPt":value}}),
            block:Box::new(Block::Paragraph(vec![Inline::Text("Invalid new request".into())]))}],..Default::default()};
        assert_eq!(validate_document(&doc,&json!({}),Consumer::Unsupported),Err(INVALID));
    }
}
