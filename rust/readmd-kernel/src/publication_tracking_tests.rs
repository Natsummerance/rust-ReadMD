use super::*;
use serde_json::json;

#[test]
fn p04_tracking_is_strict_signed_pt_and_twips_quantisation_is_symmetric() {
    for (pt, twips) in [(-5., -100), (-1.03, -21), (-0.025, -1), (-0.024, 0), (0., 0), (0.024, 0), (0.025, 1), (1.03, 21), (20., 400)] {
        let value = parse(&json!(pt)).unwrap(); assert_eq!(value.requested_pt(), pt); assert_eq!(value.docx_twips(), twips);
    }
    for raw in [Value::Null, json!(true), json!("1"), json!("1pt"), json!([]), json!({"pt":1}), json!(-5.001), json!(20.001), json!(1e100)] { assert_eq!(parse(&raw), Err(INVALID), "{raw}"); }
    assert!(serde_json::from_str::<Value>("1e999").is_err());
}

#[test]
fn p04_tracking_absence_null_and_explicit_zero_have_distinct_layer_semantics() {
    let parent = global(&json!({"typography":{"trackingPt":1.25}})).unwrap();
    assert_eq!(resolve(parent, &json!({}), true), Ok(parent));
    assert_eq!(resolve(parent, &json!({}), false), Ok(None));
    assert_eq!(resolve(parent, &json!({"trackingPt":0}), true).unwrap().unwrap().docx_twips(), 0);
    assert_eq!(resolve(parent, &json!({"trackingPt":null}), true), Err(INVALID));
    let base = json!({"typography":{"trackingPt":1.25,"spacing":9}});
    let merged = crate::publication_metadata::merge_options(&base, &json!({"typography":{"trackingPt":null}})).unwrap();
    assert_eq!(global(&merged).unwrap(), parent); assert_eq!(merged["typography"]["spacing"], 9);
    let zero = crate::publication_metadata::merge_options(&base, &json!({"typography":{"trackingPt":0}})).unwrap();
    assert_eq!(zero["typography"]["trackingPt"], 0); assert!(validate_request(&zero).is_ok());
    let old = crate::export_styles::sanitize(&json!({"typography":{"spacing":9}}));
    assert!(old["typography"].get("trackingPt").is_none()); assert_eq!(old["typography"]["spacing"], 9.);
}

#[test]
fn p04_tracking_source_transaction_removes_override_without_rewriting_bom_crlf_or_unicode() {
    let source = "\u{feff}<!-- readmd:layout v1 {\"id\":\"track\",\"text\":{\"trackingPt\":1.03,\"color\":\"#334455\"}} -->\r\n\r\nOriginal 中文 e\u{301} 👩\u{200d}💻.\r\n";
    let model = crate::publication_blocks::analyze(source).unwrap(); let node = model.nodes.iter().find(|n| n.id=="track").unwrap();
    let result = crate::publication_blocks::apply(source, &model.revision, &node.id, &json!({"text":{"trackingPt":null}})).unwrap();
    let content = result["result"]["content"].as_str().unwrap(); assert!(content.starts_with('\u{feff}')); assert!(content.ends_with("Original 中文 e\u{301} 👩\u{200d}💻.\r\n")); assert!(!content.contains("trackingPt")); assert!(content.contains("#334455"));
    let inverse: Vec<crate::document_transaction::TextEdit> = serde_json::from_value(result["result"]["inverse"].clone()).unwrap();
    let transaction = crate::document_transaction::ContentTransaction {base_revision:crate::document_transaction::revision(content),origin:"editor".into(),edits:inverse};
    assert_eq!(crate::document_transaction::apply(content,&transaction).unwrap().content, source);
    assert!(crate::publication_blocks::validate_style(&json!({"text":{"trackingPt":null}})).is_err());
}

#[test]
fn p04_tracking_other_formats_and_previews_cannot_silently_drop_even_zero_or_escaped_keys() {
    for pt in [0., 1.] { let options=json!({"typography":{"trackingPt":pt}});
        assert!(crate::publication_typography::validate_export("docx","Body.",&options).is_ok());
        for (format,engine) in [("html","rust"),("epub","rust"),("tex","rust"),("pdf","rust"),("pdf","typst"),("pdf","xelatex")] {
            let mut options=options.clone(); options["pdf"]=json!({"engine":engine});
            assert_eq!(crate::publication_typography::validate_export(format,"Body.",&options),Err(UNSUPPORTED));
        }
        assert_eq!(crate::publication_typography::validate_preview("Body.",&options),Err(UNSUPPORTED));
    }
    let source=r#"<!-- readmd:layout v1 {"id":"escape","text":{"tracking\u0050t":1}} -->

Body.
"#;
    assert!(validate_consumer(source,&json!({}),true).is_ok()); assert_eq!(validate_consumer(source,&json!({}),false),Err(UNSUPPORTED));
}

#[test]
fn p04_tracking_actual_math_container_and_nested_layers_are_rejected_before_rendering() {
    use crate::md_ast::{Block, Inline};
    let mut doc=crate::md_ast::parse("Body.");
    let tracked=|block| Block::Styled {attributes:json!({"text":{"trackingPt":0}}),block:Box::new(block)};
    doc.blocks=vec![tracked(Block::Math("x".into()))]; assert_eq!(validate_document(&doc,&json!({}),true),Err(INVALID));
    doc.blocks=vec![tracked(tracked(Block::Paragraph(vec![Inline::Text("Body.".into())])))]; assert_eq!(validate_document(&doc,&json!({}),true),Err(UNSUPPORTED));
    let source="<!-- readmd:layout v1 {\"id\":\"heading\",\"text\":{\"trackingPt\":1}} -->\n\n# Heading\n";
    assert_eq!(validate_consumer(source,&json!({}),true),Err(UNSUPPORTED));
    let source="> <!-- readmd:layout v1 {\"id\":\"quote\",\"text\":{\"trackingPt\":1}} -->\n>\n> Quote body.\n";
    // The source grammar rejects a nested declaration before tracking runs.
    assert_eq!(validate_consumer(source,&json!({}),true),Err("publication_local_style_orphan"));
    doc.blocks=vec![Block::Quote(vec![tracked(Block::Paragraph(vec![Inline::Text("Quote body.".into())]))])];
    assert_eq!(validate_document(&doc,&json!({}),true),Err(UNSUPPORTED));
}

#[test]
fn p04_tracking_preset_and_layer_errors_point_to_the_original_first_failing_field() {
    use crate::publication_typography::{merge_error_field,request_error_field};
    let bad_tracking=json!({"typography":{"trackingPt":"1","lineSpacing":{"mode":"exact","pt":0}}});
    let bad_line=json!({"typography":{"lineSpacing":{"mode":"exact","pt":0}}});
    assert_eq!(request_error_field(&bad_tracking),"typography.trackingPt");
    assert_eq!(request_error_field(&bad_line),"typography.lineSpacing");
    assert_eq!(merge_error_field(&bad_line,&bad_tracking),"typography.lineSpacing");
    assert_eq!(crate::publication_metadata::merge_options(&json!({}),&bad_tracking).unwrap_err().path,"typography.trackingPt");
    assert_eq!(crate::publication_metadata::merge_options(&bad_tracking,&json!({})).unwrap_err().path,"typography.trackingPt");
}
