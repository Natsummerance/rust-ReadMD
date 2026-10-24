use super::*;

#[test]
fn higher_legacy_fields_override_lower_specific_kinds_before_recursive_merge() {
    let lower=json!({"page":{"columns":2},"footnotes":{"size":9,"kinds":{"footnote":{"size":8,"start":27},"endnote":{"size":10,"restart":"section"}}}});
    let higher=json!({"footnotes":{"size":30,"start":0,"kinds":{"endnote":{"size":31}}}});
    let merged=crate::publication_metadata::merge_options(&lower,&higher).unwrap();
    let policies=parse_layer(&merged).unwrap();
    assert_eq!(policies.policy(NoteKind::Footnote).size,Some(30.));
    assert_eq!(policies.policy(NoteKind::Endnote).size,Some(31.));
    assert_eq!(policies.policy(NoteKind::Footnote).start,Some(0));
    assert_eq!(policies.policy(NoteKind::Endnote).start,Some(0));
    assert_eq!(policies.policy(NoteKind::Endnote).restart,Some(NoteRestart::Section));
    assert_eq!(merged["page"]["columns"],2);
    assert_eq!(lower["footnotes"]["kinds"]["footnote"]["size"],8);
}

#[test]
fn typed_and_legacy_spacing_replace_the_union_at_the_correct_layer() {
    let lower=json!({"footnotes":{"lineHeight":1.2,"kinds":{"footnote":{"lineSpacing":{"mode":"exact","pt":14.25}},"endnote":{"lineSpacing":{"mode":"atLeast","pt":16}}}}});
    let higher=json!({"footnotes":{"lineHeight":2,"kinds":{"endnote":{"lineSpacing":{"mode":"exact","pt":3.5}}}}});
    let merged=crate::publication_metadata::merge_options(&lower,&higher).unwrap();
    let policies=parse_layer(&merged).unwrap();
    assert_eq!(policies.policy(NoteKind::Footnote).line_spacing,Some(LineSpacing::Multiple(2.)));
    assert_eq!(policies.policy(NoteKind::Endnote).line_spacing,Some(LineSpacing::ExactPt(3.5)));
    let third=crate::publication_metadata::merge_options(&merged,&json!({"footnotes":{"kinds":{"footnote":{"lineHeight":1.5}}}})).unwrap();
    assert_eq!(parse_layer(&third).unwrap().policy(NoteKind::Footnote).line_spacing,Some(LineSpacing::Multiple(1.5)));
    assert!(parse_layer(&json!({"footnotes":{"lineHeight":1,"lineSpacing":{"mode":"exact","pt":14}}})).is_err());
}

#[test]
fn null_inherits_but_zero_false_and_empty_notice_are_not_clears() {
    let lower=json!({"footnotes":{"start":7,"continuationNotice":"Continued","kinds":{"footnote":{"size":8},"endnote":{"size":10}},"kindByLabel":{"a":"footnote","b":"endnote"}}});
    let over=json!({"footnotes":{"start":0,"continuationNotice":"","kinds":{"footnote":{"size":null},"endnote":null},"kindByLabel":{"a":null}}});
    let merged=crate::publication_metadata::merge_options(&lower,&over).unwrap();
    let policies=parse_layer(&merged).unwrap();
    assert_eq!(policies.policy(NoteKind::Footnote).start,Some(0));
    assert_eq!(policies.policy(NoteKind::Footnote).size,Some(8.));
    assert_eq!(policies.policy(NoteKind::Endnote).size,Some(10.));
    assert_eq!(policies.policy(NoteKind::Footnote).continuation_notice.as_deref(),Some(""));
    assert_eq!(policies.kind_by_label.get("a"),Some(&NoteKind::Footnote));
    let inherited=crate::publication_metadata::merge_options(&merged,&json!({"footnotes":null})).unwrap();
    assert_eq!(parse_layer(&inherited).unwrap(),policies);
    assert!(parse_layer(&json!({"footnotes":{"start":false}})).is_err());
}

#[test]
fn native_metrics_and_start_are_preserved_without_ui_clamping() {
    let options=json!({"footnotes":{"kinds":{"footnote":{"size":2.5,"sizeCs":45,"start":65535,"lineSpacing":{"mode":"multiple","multiple":241.0/240.0}},"endnote":{"size":100,"start":0,"lineSpacing":{"mode":"atLeast","pt":2.05},"numberFormat":"chicago"}}}});
    let policies=parse_layer(&options).unwrap();
    assert_eq!(policies.policy(NoteKind::Footnote).size,Some(2.5));
    assert_eq!(policies.policy(NoteKind::Footnote).start,Some(65535));
    assert_eq!(policies.policy(NoteKind::Endnote).size,Some(100.));
    assert_eq!(policies.policy(NoteKind::Endnote).start,Some(0));
    assert_eq!(policies.policy(NoteKind::Footnote).line_spacing.unwrap().docx().line,241);
    assert_eq!(policies.policy(NoteKind::Endnote).line_spacing.unwrap().docx().line,41);
    assert_eq!(parse_layer(&policies.to_options()).unwrap(),policies);
    for value in [json!(-1),json!(65536),json!(1.25),json!("7")]{assert!(parse_footnotes(&json!({"start":value})).is_err());}
    assert!(parse_footnotes(&json!({"size":8.1})).is_err());
    assert!(parse_footnotes(&json!({"lineSpacing":{"mode":"exact","pt":14.01}})).is_err());
}

#[test]
fn exact_labels_route_two_kinds_without_prefix_heuristics() {
    let policies=parse_footnotes(&json!({"position":"document","kindByLabel":{"word-endnote-1":"footnote","ordinary":"endnote"}})).unwrap();
    assert_eq!(policies.kind_for_label("word-endnote-1"),NoteKind::Footnote);
    assert_eq!(policies.kind_for_label("word-footnote-9"),NoteKind::Endnote);
    let doc=crate::md_ast::parse("A[^word-endnote-1] and B[^ordinary].\n\n[^word-endnote-1]: Foot body\n[^ordinary]: End body\n");
    validate_bindings(&doc,&policies).unwrap();
    assert_eq!(used_kinds(&doc,&policies),BTreeSet::from(NoteKind::ALL));
    let renamed=crate::md_ast::parse("A[^renamed] and B[^ordinary].\n\n[^renamed]: Foot body\n[^ordinary]: End body\n");
    let error=validate_bindings(&renamed,&policies).unwrap_err();
    assert_eq!(error.code,BINDING_MISMATCH);assert!(error.path.contains("word-endnote-1"));
    assert_eq!(policies.kind_for_label("ordinary"),NoteKind::Endnote);
}

#[test]
fn duplicate_definitions_do_not_silently_select_a_binding() {
    let mut doc=Document::default();
    doc.blocks=vec![Block::FootnoteDef{label:"a".into(),blocks:vec![]},Block::FootnoteDef{label:"a".into(),blocks:vec![]}];
    let policies=parse_footnotes(&json!({"kindByLabel":{"a":"footnote"}})).unwrap();
    assert_eq!(validate_bindings(&doc,&policies).unwrap_err().code,BINDING_MISMATCH);
}

#[test]
fn kinds_have_separate_placement_domains_and_unknown_values_fail_located() {
    for (kind,placement) in [("footnote","sectEnd"),("endnote","beneathText")] {
        let error=parse_footnotes(&json!({"kinds":{kind:{"placement":placement}}})).unwrap_err();
        assert_eq!(error.path,format!("footnotes.kinds.{kind}.placement"));
    }
    assert!(parse_footnotes(&json!({"placement":"pageBottom"})).is_err());
    for value in [json!({"kinds":{"footnote":{"other":1}}}),json!({"kindByLabel":{"a":"chapter"}}),json!({"numberFormat":"decimall"}),json!({"restart":"chapter"}),json!({"lineSpacing":{"mode":"exact","pt":14,"multiple":1}})] {
        assert_eq!(parse_footnotes(&value).unwrap_err().code,INVALID);
    }
}

#[test]
fn native_extensions_are_rejected_by_unadapted_consumers_before_output() {
    let doc=crate::md_ast::parse("A[^a].\n\n[^a]: Body\n");
    let old=json!({"footnotes":{"size":9,"lineHeight":1.2,"position":"document","longNote":"keepParagraphs"}});
    validate_consumer(Consumer::Other,&doc,&old).unwrap();
    let native=json!({"footnotes":{"kinds":{"footnote":{"placement":"beneathText"}},"kindByLabel":{"a":"footnote"}}});
    validate_consumer(Consumer::Docx,&doc,&native).unwrap();
    assert_eq!(validate_consumer(Consumer::Other,&doc,&native).unwrap_err().code,UNSUPPORTED);
    assert!(!parse_layer(&json!({})).unwrap().declared);
    assert!(parse_layer(&json!({})).unwrap().to_value().as_object().unwrap().is_empty());
    for flat in [json!({"start":0}),json!({"start":12000}),json!({"start":65535}),json!({"size":30}),json!({"numberFormat":"chicago"})]{
        let options=json!({"footnotes":flat});validate_consumer(Consumer::Docx,&doc,&options).unwrap();
        assert_eq!(validate_consumer(Consumer::Other,&doc,&options).unwrap_err().code,UNSUPPORTED);
    }
}

#[test]
fn equivalent_single_native_kind_lowers_without_mutating_the_source_policy() {
    let doc=crate::md_ast::parse("A[^word-footnote-7], repeated[^word-footnote-7].\n\n[^word-footnote-7]: End body.\n");
    let options=json!({"page":{"columns":2},"footnotes":{"position":"document","title":"End matters","kinds":{"endnote":{"size":10.5,"sizeCs":10.5,"lineSpacing":{"mode":"multiple","multiple":241.0/240.0},"numberFormat":"upperLetter","start":27,"restart":"section","placement":"docEnd","separator":"none","continuationSeparator":"line","continuationNotice":"Continued","longNote":"keepParagraphs"}},"kindByLabel":{"word-footnote-7":"endnote"}}});
    let before=options.clone();let projected=project_legacy(&doc,&options).unwrap();
    assert_eq!(options,before);assert_eq!(projected["page"],options["page"]);
    assert_eq!(projected["footnotes"]["position"],"document");assert_eq!(projected["footnotes"]["size"],10.5);
    assert_eq!(projected["footnotes"]["lineHeight"],json!(241.0/240.0));
    for key in ["numberFormat","start","restart","separator","continuationSeparator","continuationNotice","longNote"]{
        assert_eq!(projected["footnotes"][key],options["footnotes"]["kinds"]["endnote"][key]);
    }
    assert_eq!(projected["footnotes"]["title"],"End matters");
    for key in ["kinds","kindByLabel","sizeCs","lineSpacing","placement"]{assert!(projected["footnotes"].get(key).is_none());}
    validate_consumer(Consumer::Other,&doc,&options).unwrap();
}

#[test]
fn mixed_actual_kinds_and_non_equivalent_native_leaves_refuse_projection() {
    let one=crate::md_ast::parse("A[^a].\n\n[^a]: Body.\n");
    for patch in [json!({"start":0}),json!({"start":12000}),json!({"size":30}),json!({"size":9,"sizeCs":10}),json!({"lineSpacing":{"mode":"exact","pt":12}}),json!({"lineSpacing":{"mode":"atLeast","pt":12}}),json!({"numberFormat":"chicago"}),json!({"numberPattern":"A"}),json!({"placement":"beneathText"})]{
        let options=json!({"footnotes":{"kinds":{"footnote":patch},"kindByLabel":{"a":"footnote"}}});
        assert_eq!(project_legacy(&one,&options).unwrap_err().code,UNSUPPORTED);
    }
    let mixed=crate::md_ast::parse("A[^a] and B[^b].\n\n[^a]: Foot.\n\n[^b]: End.\n");
    let options=json!({"footnotes":{"kindByLabel":{"a":"footnote","b":"endnote"}}});
    let error=project_legacy(&mixed,&options).unwrap_err();assert_eq!(error.code,UNSUPPORTED);assert_eq!(error.path,"footnotes.kindByLabel");
    let uncertain=json!({"footnotes":{"position":"page","kinds":{"footnote":{"size":9},"endnote":{"size":9}},"kindByLabel":{"a":"endnote"}}});
    let error=project_legacy(&one,&uncertain).unwrap_err();assert_eq!(error.code,UNSUPPORTED);assert_eq!(error.path,"footnotes.position","an unprepared CSL-generated footnote must not silently move to endnotes");
}

#[test]
fn unreachable_definition_references_do_not_create_a_false_mixed_document() {
    use crate::md_ast::Inline;
    let mut doc=Document::default();doc.blocks=vec![
        Block::Paragraph(vec![Inline::FootnoteRef("used".into())]),
        Block::FootnoteDef{label:"used".into(),blocks:vec![Block::Paragraph(vec![Inline::Text("Foot body".into())])]},
        Block::FootnoteDef{label:"unused".into(),blocks:vec![Block::Paragraph(vec![Inline::FootnoteRef("end".into())])]},
        Block::FootnoteDef{label:"end".into(),blocks:vec![]},
    ];
    let options=json!({"footnotes":{"kindByLabel":{"used":"footnote","unused":"footnote","end":"endnote"}}});
    let policies=parse_layer(&options).unwrap();assert_eq!(used_kinds(&doc,&policies),BTreeSet::from([NoteKind::Footnote]));
    assert_eq!(project_legacy(&doc,&options).unwrap()["footnotes"]["position"],"page");
}

#[test]
fn flat_text_outside_legacy_limits_cannot_bypass_projection_validation() {
    let doc=crate::md_ast::parse("A[^a].\n\n[^a]: Body.\n");
    for key in ["title","continuationNotice"]{
        for text in ["N".repeat(121),"Continued\nnext line".into(),"Continued\tlabel".into()]{
            let options=json!({"footnotes":{key:text}});let before=options.clone();
            validate_consumer(Consumer::Docx,&doc,&options).unwrap();
            let error=project_legacy(&doc,&options).unwrap_err();assert_eq!(error.code,UNSUPPORTED);assert_eq!(error.path,format!("footnotes.{key}"));assert_eq!(options,before);
        }
    }
    let compatible=json!({"footnotes":{"title":"N".repeat(120),"continuationNotice":""}});
    assert_eq!(project_legacy(&doc,&compatible).unwrap(),compatible);
}

#[test]
fn validated_note_title_and_notice_survive_the_legacy_sanitizer_unchanged() {
    let title=format!("{}\nNext\tpart","Title".repeat(30));let notice=format!("{}\nContinued\tlabel","Notice".repeat(30));
    let source=json!({"footnotes":{"title":title,"continuationNotice":notice}});
    let expected=parse_layer(&source).unwrap();let sanitized=crate::export_styles::sanitize(&source);
    assert_eq!(sanitized["footnotes"]["title"],source["footnotes"]["title"]);assert_eq!(sanitized["footnotes"]["continuationNotice"],source["footnotes"]["continuationNotice"]);
    let retained=parse_layer(&sanitized).unwrap();assert_eq!(retained.title,expected.title);assert_eq!(retained.common.continuation_notice,expected.common.continuation_notice);
    let doc=crate::md_ast::parse("A[^a].\n\n[^a]: Body.\n");assert_eq!(project_legacy(&doc,&sanitized).unwrap_err().code,UNSUPPORTED);
}

#[test]
fn section_policy_vocabulary_remains_strict_outside_numbering_and_footnote_position(){
    for(placement,expected)in[("pageBottom",NotePlacement::PageBottom),("beneathText",NotePlacement::BeneathText)]{
        let value=json!({"kinds":{"footnote":{"placement":placement}}});let parsed=parse_section(&value).unwrap();
        assert_eq!(parsed.footnote_placement,Some(expected));assert_eq!(parsed.to_value(),value);assert_eq!(serde_json::from_value::<SectionNotePolicies>(value).unwrap(),parsed);
    }
    for value in [json!(null),json!({}),json!({"kinds":null}),json!({"kinds":{"footnote":{"placement":null}}})]{assert!(parse_section(&value).unwrap().is_empty());}
    for(value,path)in[
        (json!({"placement":"pageBottom"}),"section.footnotes.placement"),
        (json!({"kinds":{"endnote":{"placement":"docEnd"}}}),"section.footnotes.kinds.endnote.placement"),
        (json!({"kinds":{"footnote":{"size":10}}}),"section.footnotes.kinds.footnote.size"),
        (json!({"kinds":{"footnote":{"placement":"sectEnd"}}}),"section.footnotes.kinds.footnote.placement"),
        (json!({"kinds":{"footnote":{"placement":false}}}),"section.footnotes.kinds.footnote.placement"),
        (json!({"kindByLabel":{"a":"footnote"}}),"section.footnotes.kindByLabel"),
    ]{let error=parse_section(&value).unwrap_err();assert_eq!(error.code,INVALID);assert_eq!(error.path,path);assert!(serde_json::from_value::<SectionNotePolicies>(value).is_err());}
    for raw in [r#"{"kinds":{"footnote":{"placement":"pageBottom","placement":"beneathText"}}}"#,r#"{"kinds":{"footnote":{},"footnote":{}}}"#,r#"{"kinds":{},"kinds":{}}"#]{assert!(serde_json::from_str::<SectionNotePolicies>(raw).is_err(),"duplicate source fields must not be collapsed: {raw}");}
    assert!(SectionNotePolicies{footnote_placement:Some(NotePlacement::DocEnd),..Default::default()}.validate().is_err());
}

#[test]
fn section_resolution_changes_only_its_position_and_preserves_wide_global_values(){
    let global=json!({"page":{"columns":2},"footnotes":{"size":30,"start":65535,"kinds":{"footnote":{"placement":"pageBottom","sizeCs":31},"endnote":{"placement":"docEnd","size":40}},"kindByLabel":{"a":"footnote","b":"endnote"}}});let before=global.clone();
    let patch=parse_section(&json!({"kinds":{"footnote":{"placement":"beneathText"}}})).unwrap();let resolved=resolve_section(&global,&patch).unwrap();
    let mut expected=global.clone();expected["footnotes"]["kinds"]["footnote"]["placement"]=json!("beneathText");assert_eq!(resolved,expected);assert_eq!(global,before);
    assert_eq!(resolve_section(&global,&SectionNotePolicies::default()).unwrap(),global);assert_eq!(resolve_section(&json!({}),&SectionNotePolicies::default()).unwrap(),json!({}));
    assert_eq!(resolve_section(&global,&patch).unwrap()["footnotes"]["kindByLabel"],global["footnotes"]["kindByLabel"]);
}

#[test]
fn section_positions_are_checked_even_without_a_global_note_options_object(){
    let source="<!-- readmd-section:v1 {\"id\":\"first\",\"footnotes\":{\"kinds\":{\"footnote\":{\"placement\":\"beneathText\"}}}} -->\n\nBody[^a].\n\n<!-- readmd-section:v1 {\"id\":\"second\"} -->\n\nLater[^a].\n\n[^a]: Note.\n";
    let doc=crate::md_ast::parse(source);assert!(has_section_policies(&doc));validate_consumer(Consumer::Docx,&doc,&json!({})).unwrap();let before=doc.clone();
    for result in [project_legacy(&doc,&json!({})),validate_consumer(Consumer::Other,&doc,&json!({})).map(|_|json!({}))]{let error=result.unwrap_err();assert_eq!(error.code,UNSUPPORTED);assert_eq!(error.path,"sections[\"first\"].footnotes.kinds.footnote.placement");}
    assert_eq!(doc,before,"consumer validation must not erase readable section source policies");
}

#[test]
fn page_bottom_section_policies_lower_only_after_every_scope_is_proven_equivalent(){
    let all="<!-- readmd-section:v1 {\"id\":\"a\",\"footnotes\":{\"kinds\":{\"footnote\":{\"placement\":\"pageBottom\"}}}} -->\n\nA[^a].\n\n<!-- readmd-section:v1 {\"id\":\"b\",\"footnotes\":{\"kinds\":{\"footnote\":{\"placement\":\"pageBottom\"}}}} -->\n\nB[^a].\n\n[^a]: Note.\n";
    let doc=crate::md_ast::parse(all);assert_eq!(project_legacy(&doc,&json!({})).unwrap(),json!({}));
    let global=json!({"footnotes":{"kinds":{"footnote":{"placement":"beneathText","size":10,"lineHeight":1.2}}}});let before=global.clone();let flat=project_legacy(&doc,&global).unwrap();assert_eq!(flat["footnotes"]["size"].as_f64(),Some(10.));assert_eq!(flat["footnotes"]["position"],"page");assert!(flat["footnotes"].get("placement").is_none());assert_eq!(global,before);
    let omitted=all.replacen(",\"footnotes\":{\"kinds\":{\"footnote\":{\"placement\":\"pageBottom\"}}}","",2);let omitted=crate::md_ast::parse(&omitted);assert!(!has_section_policies(&omitted));assert_eq!(project_legacy(&omitted,&global).unwrap_err().code,UNSUPPORTED);
    let prefix=crate::md_ast::parse(&format!("Visible prefix[^a].\n\n{all}"));let error=project_legacy(&prefix,&global).unwrap_err();assert_eq!(error.path,"sections[\"_document\"].footnotes.kinds.footnote.placement");
}

#[test]
fn malformed_reserved_section_policies_cannot_fall_through_as_hidden_html(){
    for raw in [
        r#"<!-- readmd-section:v1 {"id":"a","footnotes":{"kinds":{"footnote":{"placement":"chapterEnd"}}}} -->"#,
        r#"<!-- readmd-section:v1 {"id":"a","footnotes":{"kinds":{"endnote":{"placement":"docEnd"}}}} -->"#,
        r#"<!-- readmd-section:v1 {"id":"a","footnotes":{"kinds":{"footnote":{"start":65536}}}} -->"#,
    ]{
        let doc=crate::md_ast::parse(&format!("{raw}\n\nA[^a].\n\n[^a]: Note.\n"));
        for consumer in [Consumer::Docx,Consumer::Other]{let error=validate_consumer(consumer,&doc,&json!({})).unwrap_err();assert_eq!(error.code,"publication_section_invalid");assert_eq!(error.path,"sections");}
        assert_eq!(project_legacy(&doc,&json!({})).unwrap_err().code,"publication_section_invalid");
    }
}

#[test]
fn section_numbering_keeps_both_native_kinds_and_explicit_defaults_sparse() {
    let value = json!({"kinds":{
        "footnote":{"placement":"beneathText","numberFormat":"upperRoman","start":7,"restart":"section"},
        "endnote":{"numberFormat":"upperLetter","numberPattern":"A","start":27,"restart":"continuous"}
    }});
    let parsed = parse_section(&value).unwrap();
    assert_eq!(parsed.footnote_numbering.number_format.as_deref(), Some("upperRoman"));
    assert_eq!(parsed.footnote_numbering.start, Some(7));
    assert_eq!(parsed.footnote_numbering.restart, Some(NoteRestart::Section));
    assert_eq!(parsed.endnote_numbering.number_format.as_deref(), Some("upperLetter"));
    assert_eq!(parsed.endnote_numbering.number_pattern.as_deref(), Some("A"));
    assert_eq!(parsed.endnote_numbering.start, Some(27));
    assert_eq!(parsed.endnote_numbering.restart, Some(NoteRestart::Continuous));
    assert_eq!(parsed.to_value(), value);
    assert_eq!(serde_json::from_str::<SectionNotePolicies>(&value.to_string()).unwrap(), parsed);

    let defaults = json!({"kinds":{"footnote":{"numberFormat":"decimal","start":0,"restart":"continuous"}}});
    let explicit = parse_section(&defaults).unwrap();
    assert!(!explicit.is_empty());
    assert_eq!(explicit.to_value(), defaults);
    assert!(explicit.endnote_numbering.is_empty());
    assert_eq!(SectionNotePolicies::default().to_value(), json!({}));
    for value in [
        json!(null), json!({}), json!({"kinds":null}),
        json!({"kinds":{"footnote":null,"endnote":null}}),
        json!({"kinds":{"footnote":{"numberFormat":null,"numberPattern":null,"start":null,"restart":null}}}),
    ] {
        assert!(parse_section(&value).unwrap().is_empty());
        assert!(serde_json::from_value::<SectionNotePolicies>(value).unwrap().is_empty());
    }
}

#[test]
fn section_numbering_preserves_every_native_format_restart_and_u16_start() {
    assert_eq!(WORD_NUMBER_FORMATS.len(), 63);
    for kind in NoteKind::ALL {
        for format in WORD_NUMBER_FORMATS {
            for start in [0u16, 1, 27, 12000, u16::MAX] {
                for restart in ["continuous", "section", "page"] {
                    let value = json!({"kinds":{(kind.as_str()):{
                        "numberFormat":format,"start":start,"restart":restart
                    }}});
                    let parsed = parse_section(&value).unwrap();
                    assert_eq!(parsed.to_value(), value);
                    assert_eq!(serde_json::from_str::<SectionNotePolicies>(&value.to_string()).unwrap(), parsed);
                }
            }
        }
    }
}

#[test]
fn section_numbering_rejects_arrays_and_fields_owned_by_document_policies() {
    for value in [
        json!([]), json!([{}]), json!({"kinds":[]}),
        json!({"kinds":{"footnote":[]}}), json!({"kinds":{"endnote":[]}}),
        json!({"kinds":{"footnote":{"start":[]}}}),
        json!({"kinds":{"endnote":{"numberFormat":[]}}}),
    ] {
        assert!(parse_section(&value).is_err(), "{value}");
        assert!(serde_json::from_str::<SectionNotePolicies>(&value.to_string()).is_err(), "{value}");
    }
    for kind in NoteKind::ALL {
        for key in ["size", "sizeCs", "lineHeight", "lineSpacing", "separator", "continuationSeparator", "continuationNotice", "longNote", "position", "suppression", "unknown"] {
            // Unsupported keys remain invalid even when null could otherwise
            // look like an omitted inherited value.
            let value = json!({"kinds":{(kind.as_str()):{key:null}}});
            let error = parse_section(&value).unwrap_err();
            assert_eq!(error.path, format!("section.footnotes.kinds.{}.{key}", kind.as_str()));
            assert!(serde_json::from_value::<SectionNotePolicies>(value).is_err());
        }
    }
    assert!(parse_section(&json!({"kinds":{"endnote":{"placement":null}}})).is_err());
}

#[test]
fn section_numbering_rejects_duplicate_keys_before_null_or_defaults_can_hide_them() {
    for raw in [
        r#"{"kinds":null,"kinds":{}}"#,
        r#"{"kinds":{"footnote":null,"footnote":{}}}"#,
        r#"{"kinds":{"endnote":{},"endnote":{}}}"#,
        r#"{"kinds":{"footnote":{"numberFormat":null,"numberFormat":"decimal"}}}"#,
        r#"{"kinds":{"endnote":{"numberPattern":"A","numberPattern":"A"}}}"#,
        r#"{"kinds":{"footnote":{"start":null,"start":0}}}"#,
        r#"{"kinds":{"endnote":{"restart":"continuous","restart":"continuous"}}}"#,
    ] {
        assert!(serde_json::from_str::<SectionNotePolicies>(raw).is_err(), "{raw}");
    }
}

#[test]
fn section_numbering_validates_integer_format_and_bounded_pattern_without_clamping() {
    for kind in NoteKind::ALL {
        for start in [json!(-1), json!(65536), json!(1.5), json!(1.0), json!(true), json!("7")] {
            let value = json!({"kinds":{(kind.as_str()):{"start":start}}});
            assert_eq!(parse_section(&value).unwrap_err().path, format!("section.footnotes.kinds.{}.start", kind.as_str()));
            assert!(serde_json::from_str::<SectionNotePolicies>(&value.to_string()).is_err());
        }
        for format in [json!("futureFormat"), json!("Decimal"), json!(7), json!(false)] {
            assert!(parse_section(&json!({"kinds":{(kind.as_str()):{"numberFormat":format}}})).is_err());
        }
        for pattern in [json!("A".repeat(1025)), json!("A\u{0000}B"), json!(7), json!(false)] {
            assert!(parse_section(&json!({"kinds":{(kind.as_str()):{"numberPattern":pattern}}})).is_err());
        }
        for pattern in ["".to_string(), "A".repeat(1024), "A\tB\nC\rD".to_string()] {
            let value = json!({"kinds":{(kind.as_str()):{"numberFormat":"custom","numberPattern":pattern}}});
            assert_eq!(parse_section(&value).unwrap().to_value(), value);
        }
    }
    assert!(SectionNoteNumberingPatch { number_format:Some("bad".into()), ..Default::default() }.validate("test").is_err());
    assert!(SectionNoteNumberingPatch { number_pattern:Some("A".repeat(1025)), ..Default::default() }.validate("test").is_err());
}

#[test]
fn section_explicit_format_clears_only_its_inherited_pattern_and_keeps_native_typography() {
    let global = json!({"page":{"columns":2},"footnotes":{
        "numberFormat":"custom","numberPattern":"COMMON","start":3,"restart":"page",
        "kinds":{
            "footnote":{"size":10.5,"sizeCs":30,"lineSpacing":{"mode":"exact","pt":14.25},"numberPattern":"FOOT","placement":"pageBottom"},
            "endnote":{"size":30,"numberPattern":"END","placement":"docEnd"}
        },"kindByLabel":{"f":"footnote","e":"endnote"}
    }});
    let before = global.clone();
    let patch = parse_section(&json!({"kinds":{"footnote":{"numberFormat":"decimal","start":0,"restart":"continuous"}}})).unwrap();
    let resolved = resolve_section(&global, &patch).unwrap();
    let effective = parse_layer(&resolved).unwrap();
    let footnote = effective.policy(NoteKind::Footnote);
    assert_eq!(footnote.number_format.as_deref(), Some("decimal"));
    assert_eq!(footnote.number_pattern, None);
    assert_eq!(footnote.start, Some(0));
    assert_eq!(footnote.restart, Some(NoteRestart::Continuous));
    assert_eq!(footnote.size, Some(10.5));
    assert_eq!(footnote.size_cs, Some(30.));
    assert_eq!(footnote.line_spacing, Some(LineSpacing::ExactPt(14.25)));
    assert_eq!(effective.policy(NoteKind::Endnote), parse_layer(&global).unwrap().policy(NoteKind::Endnote));
    assert_eq!(resolved["footnotes"]["kindByLabel"], global["footnotes"]["kindByLabel"]);
    assert_eq!(resolved["page"], global["page"]);
    assert!(resolved["footnotes"]["kinds"]["footnote"].get("numberPattern").is_none());
    assert_eq!(global, before);

    let common_only = json!({"footnotes":{"numberFormat":"custom","numberPattern":"COMMON"}});
    let resolved = parse_layer(&resolve_section(&common_only, &patch).unwrap()).unwrap();
    assert_eq!(resolved.policy(NoteKind::Footnote).number_pattern, None);
    assert_eq!(resolved.policy(NoteKind::Endnote).number_pattern.as_deref(), Some("COMMON"));
    let both = parse_section(&json!({"kinds":{"footnote":{"numberFormat":"decimal"},"endnote":{"numberFormat":"lowerRoman"}}})).unwrap();
    let resolved = parse_layer(&resolve_section(&common_only, &both).unwrap()).unwrap();
    for kind in NoteKind::ALL { assert_eq!(resolved.policy(kind).number_pattern, None); }
}

#[test]
fn section_pattern_only_inherits_effective_format_and_start_only_does_not_request_restart() {
    let global = json!({"footnotes":{"numberFormat":"decimal","numberPattern":"GLOBAL","start":7,"restart":"page","kinds":{"endnote":{"numberFormat":"upperLetter"}}}});
    let patch = parse_section(&json!({"kinds":{"footnote":{"numberPattern":"A"},"endnote":{"numberPattern":"B","start":27}}})).unwrap();
    let resolved = parse_layer(&resolve_section(&global, &patch).unwrap()).unwrap();
    assert_eq!(resolved.policy(NoteKind::Footnote).number_format.as_deref(), Some("decimal"));
    assert_eq!(resolved.policy(NoteKind::Endnote).number_format.as_deref(), Some("upperLetter"));
    assert_eq!(resolved.policy(NoteKind::Footnote).number_pattern.as_deref(), Some("A"));
    assert_eq!(resolved.policy(NoteKind::Endnote).number_pattern.as_deref(), Some("B"));
    assert_eq!(resolved.policy(NoteKind::Footnote).start, Some(7));
    assert_eq!(resolved.policy(NoteKind::Endnote).start, Some(27));
    for kind in NoteKind::ALL { assert_eq!(resolved.policy(kind).restart, Some(NoteRestart::Page)); }
    assert_eq!(patch.endnote_numbering.number_format, None);
    assert_eq!(patch.endnote_numbering.restart, None);

    let defaults = parse_layer(&resolve_section(&json!({}), &patch).unwrap()).unwrap();
    assert_eq!(defaults.policy(NoteKind::Footnote).number_format.as_deref(), Some("decimal"));
    assert_eq!(defaults.policy(NoteKind::Endnote).number_format.as_deref(), Some("lowerRoman"));
    let start_only = parse_section(&json!({"kinds":{"footnote":{"start":65535}}})).unwrap();
    let value = resolve_section(&json!({}), &start_only).unwrap();
    assert_eq!(value, json!({"footnotes":{"kinds":{"footnote":{"start":65535}}}}));
}

fn two_uniform_note_scopes(notes:Value)->Document {
    let declaration=|id:&str|format!("<!-- readmd-section:v1 {} -->",json!({"id":id,"break":"continuous","footnotes":notes}));
    crate::md_ast::parse(&format!("{}\n\nFirst[^a].\n\n{}\n\nSecond[^b], same[^a], third[^c].\n\n[^a]: First definition.\n\n[^b]: Second definition.\n\n[^c]: Third definition.\n",declaration("first"),declaration("last")))
}

#[test]
fn uniform_section_numbering_lowers_real_legacy_fields_without_changing_source_or_page_geometry() {
    let doc=two_uniform_note_scopes(json!({"kinds":{"footnote":{"numberFormat":"upperLetter","start":27,"restart":"section"}}}));
    let before_doc=doc.clone();
    let global=json!({"page":{"size":"A4","columns":1},"footnotes":{"size":10.5,"lineHeight":1.2,"title":"Notes"}});
    let before_global=global.clone();
    let flat=project_legacy(&doc,&global).unwrap();
    assert_eq!(flat["footnotes"]["numberFormat"],"upperLetter");
    assert_eq!(flat["footnotes"]["start"],27);
    assert_eq!(flat["footnotes"]["restart"],"section");
    assert_eq!(flat["footnotes"]["position"],"page");
    assert_eq!(flat["footnotes"]["size"],10.5);
    assert_eq!(flat["footnotes"]["lineHeight"],1.2);
    assert_eq!(flat["footnotes"]["title"],"Notes");
    assert!(flat["footnotes"].get("kinds").is_none());
    assert_eq!(flat["page"],global["page"]);
    let numbers=crate::publication_layout::note_numbers(&doc,&flat);
    assert_eq!(numbers["a"],27);
    assert_eq!(numbers["b"],27);
    assert_eq!(numbers["c"],28);
    assert_eq!(crate::publication_layout::note_number(flat["footnotes"]["numberFormat"].as_str().unwrap(),numbers["b"]),"AA");
    assert_eq!(crate::publication_layout::note_number(flat["footnotes"]["numberFormat"].as_str().unwrap(),numbers["c"]),"BB");
    assert_eq!(project_legacy(&doc,&flat).unwrap(),flat,"repeated exporter/preflight entry must be stable");
    assert_eq!(doc,before_doc);
    assert_eq!(global,before_global);
    validate_consumer(Consumer::Other,&doc,&global).unwrap();
}

#[test]
fn uniform_section_numbering_covers_both_stories_five_formats_and_continuous_or_section_counters() {
    for kind in NoteKind::ALL {
        for format in crate::publication_layout::NUMBER_FORMATS {
            for restart in ["continuous","section"] {
                let doc=two_uniform_note_scopes(json!({"kinds":{(kind.as_str()):{"numberFormat":format,"start":27,"restart":restart}}}));
                let options=if kind.is_end(){json!({"footnotes":{"position":"document"}})}else{json!({})};
                let flat=project_legacy(&doc,&options).unwrap();
                assert_eq!(flat["footnotes"]["numberFormat"],*format);
                assert_eq!(flat["footnotes"]["start"],27);
                assert_eq!(flat["footnotes"]["restart"],restart);
                assert_eq!(flat["footnotes"]["position"],if kind.is_end(){"document"}else{"page"});
                let numbers=crate::publication_layout::note_numbers(&doc,&flat);
                assert_eq!(numbers["a"],27);
                assert_eq!(numbers["b"],if restart=="section"{27}else{28});
                assert_eq!(numbers["c"],if restart=="section"{28}else{29});
            }
        }
    }
}

#[test]
fn legacy_projection_checks_implicit_prefixes_empty_scopes_and_global_inheritance() {
    let declaration=|id:&str,notes:Value|format!("<!-- readmd-section:v1 {} -->",json!({"id":id,"footnotes":notes}));
    let notes=json!({"kinds":{"footnote":{"numberFormat":"upperLetter","start":27,"restart":"continuous"}}});
    let prefix=crate::md_ast::parse(&format!("Prefix without a note.\n\n{}\n\nBody[^a].\n\n[^a]: Definition.\n",declaration("first",notes.clone())));
    assert_eq!(project_legacy(&prefix,&json!({})).unwrap_err().path,"sections[\"first\"].footnotes.kinds.footnote.numberFormat");
    let empty=crate::md_ast::parse(&format!("{}\n\nBody[^a].\n\n{}\n\n{}\n\nLast[^a].\n\n[^a]: Definition.\n",declaration("first",notes.clone()),declaration("empty",json!({"kinds":{"footnote":{"numberFormat":"decimal","start":1,"restart":"continuous"}}})),declaration("last",notes.clone())));
    assert_eq!(project_legacy(&empty,&json!({})).unwrap_err().path,"sections[\"empty\"].footnotes.kinds.footnote.numberFormat");
    let omitted=crate::md_ast::parse(&format!("{}\n\nBody[^a].\n\n<!-- readmd-section:v1 {{\"id\":\"omitted\"}} -->\n\nLast[^a].\n\n[^a]: Definition.\n",declaration("first",notes.clone())));
    assert_eq!(project_legacy(&omitted,&json!({})).unwrap_err().path,"sections[\"omitted\"].footnotes.kinds.footnote.numberFormat");
    let matching=json!({"footnotes":{"numberFormat":"upperLetter","start":27,"restart":"continuous"}});
    assert!(project_legacy(&omitted,&matching).is_ok());
    let hidden_prefix=crate::md_ast::parse(&format!("[^a]: Definition.\n\n<!-- ordinary hidden prefix -->\n\n{}\n\nBody[^a].\n",declaration("first",notes)));
    assert!(project_legacy(&hidden_prefix,&json!({})).is_ok(),"definitions and hidden HTML do not invent an implicit printed scope");
}

#[test]
fn explicit_section_defaults_compare_equal_to_inherited_defaults_and_keep_source_presence() {
    let source="Prefix.\n\n<!-- readmd-section:v1 {\"id\":\"explicit\",\"footnotes\":{\"kinds\":{\"footnote\":{\"numberFormat\":\"decimal\",\"start\":1,\"restart\":\"continuous\"}}}} -->\n\nBody[^a].\n\n<!-- readmd-section:v1 {\"id\":\"omitted\"} -->\n\nLast[^a].\n\n[^a]: Definition.\n";
    let doc=crate::md_ast::parse(source);
    let before=doc.clone();
    let flat=project_legacy(&doc,&json!({})).unwrap();
    assert_eq!(flat["footnotes"]["numberFormat"],"decimal");
    assert_eq!(flat["footnotes"]["start"],1);
    assert_eq!(flat["footnotes"]["restart"],"continuous");
    let scopes=crate::publication_sections::resolve(&doc,&json!({})).unwrap();
    assert!(scopes[0].note_overrides.is_none());
    assert_eq!(scopes[1].note_overrides.as_ref().unwrap().footnote_numbering.start,Some(1));
    assert!(scopes[2].note_overrides.is_none());
    assert_eq!(doc,before);
}

#[test]
fn section_origin_each_page_never_borrows_the_global_legacy_warning_exception() {
    for kind in NoteKind::ALL {
        let notes=json!({"kinds":{(kind.as_str()):{"restart":"page"}}});
        let doc=two_uniform_note_scopes(notes);
        let options=json!({"footnotes":{"position":if kind.is_end(){"document"}else{"page"},"restart":"page"}});
        for consumer in [Consumer::Other,Consumer::Html] {
            let error=validate_consumer(consumer,&doc,&options).unwrap_err();
            assert_eq!(error.code,UNSUPPORTED);
            assert_eq!(error.path,format!("sections[\"first\"].footnotes.kinds.{}.restart",kind.as_str()));
        }
        let unused=crate::md_ast::parse(&format!("<!-- readmd-section:v1 {} -->\n\nNo note references.\n",json!({"id":"unused","footnotes":{"kinds":{(kind.as_str()):{"restart":"page"}}}})));
        assert_eq!(project_legacy(&unused,&options).unwrap_err().path,format!("sections[\"unused\"].footnotes.kinds.{}.restart",kind.as_str()));
    }
    let global=json!({"footnotes":{"restart":"page"}});
    let old=crate::md_ast::parse("Body[^a].\n\n[^a]: Definition.\n");
    assert_eq!(project_legacy(&old,&global).unwrap(),global);
    assert!(validate_html(&old,&global).is_ok());
    assert!(crate::publication_layout::note_warnings("html",&global).contains(&"publication_note_page_restart_unsupported".into()));
}

#[test]
fn uniform_section_numbering_keeps_native_only_values_and_patterns_out_of_legacy_consumers() {
    for leaf in [
        json!({"numberFormat":"chicago"}),json!({"numberFormat":"none"}),
        json!({"start":0}),json!({"start":12000}),json!({"start":65535}),
        json!({"numberPattern":""}),json!({"numberFormat":"decimal","numberPattern":"A"}),
    ] {
        let doc=two_uniform_note_scopes(json!({"kinds":{"footnote":leaf}}));
        validate_consumer(Consumer::Docx,&doc,&json!({})).unwrap();
        assert_eq!(project_legacy(&doc,&json!({})).unwrap_err().code,UNSUPPORTED);
    }
}

#[test]
fn projection_validates_meaningful_unused_kind_policies_and_does_not_drop_a_second_kind_override() {
    for notes in [
        json!({"kinds":{"footnote":{"start":1},"endnote":{"numberPattern":"A"}}}),
        json!({"kinds":{"footnote":{"start":1},"endnote":{"numberFormat":"chicago"}}}),
        json!({"kinds":{"footnote":{"start":1},"endnote":{"start":65535}}}),
        json!({"kinds":{"footnote":{"start":1},"endnote":{"start":2}}}),
    ] {
        let doc=two_uniform_note_scopes(notes.clone());
        let before=doc.clone();
        let error=project_legacy(&doc,&json!({})).unwrap_err();
        assert!(error.path.contains("endnote"),"{}",error.path);
        assert_eq!(doc,before);
        let no_notes=crate::md_ast::parse(&format!("<!-- readmd-section:v1 {} -->\n\nNo notes.\n",json!({"id":"unused","footnotes":notes})));
        assert_eq!(project_legacy(&no_notes,&json!({})).unwrap_err().code,UNSUPPORTED);
    }
    let explicit_defaults=two_uniform_note_scopes(json!({"kinds":{"footnote":{"start":1},"endnote":{"numberFormat":"decimal","start":1,"restart":"continuous"}}}));
    assert_eq!(project_legacy(&explicit_defaults,&json!({})).unwrap_err().path,"sections[\"first\"].footnotes.kinds.endnote.numberFormat","an independent unused kind override cannot be represented by one flat kind");
    let unused_uniform=crate::md_ast::parse("<!-- readmd-section:v1 {\"id\":\"unused\",\"footnotes\":{\"kinds\":{\"footnote\":{\"numberFormat\":\"upperLetter\",\"start\":27}}}} -->\n\nNo notes.\n");
    assert_eq!(project_legacy(&unused_uniform,&json!({})).unwrap()["footnotes"]["start"],27,"representable unused configuration still participates in lowering");
}

#[test]
fn section_numbering_preserves_existing_mixed_kind_binding_and_native_typography_guards() {
    let doc=two_uniform_note_scopes(json!({"kinds":{"footnote":{"start":1}}}));
    for options in [
        json!({"footnotes":{"kindByLabel":{"a":"footnote","b":"endnote"}}}),
        json!({"footnotes":{"size":30}}),
        json!({"footnotes":{"size":9,"sizeCs":10}}),
        json!({"footnotes":{"lineSpacing":{"mode":"exact","pt":14.25}}}),
    ] { assert_eq!(project_legacy(&doc,&options).unwrap_err().code,UNSUPPORTED); }
    let bad_binding=json!({"footnotes":{"kindByLabel":{"missing":"footnote"}}});
    assert_eq!(project_legacy(&doc,&bad_binding).unwrap_err().code,BINDING_MISMATCH);
}

#[test]
fn html_and_default_only_consumers_accept_only_default_effective_section_numbering() {
    let defaults=two_uniform_note_scopes(json!({"kinds":{"footnote":{"numberFormat":"decimal","start":1,"restart":"continuous"}}}));
    validate_consumer(Consumer::Html,&defaults,&json!({})).unwrap();
    validate_default_section_numbering(&defaults,&json!({})).unwrap();
    let replaced_global=json!({"footnotes":{"numberFormat":"upperRoman","start":27,"restart":"section"}});
    validate_html(&defaults,&replaced_global).unwrap();
    for(leaf,key)in[
        (json!({"numberFormat":"upperLetter","start":27}),"numberFormat"),
        (json!({"numberFormat":"decimal","start":27}),"start"),
        (json!({"restart":"section"}),"restart"),
    ] {
        let doc=two_uniform_note_scopes(json!({"kinds":{"footnote":leaf}}));
        assert!(project_legacy(&doc,&json!({})).is_ok());
        let error=validate_consumer(Consumer::Html,&doc,&json!({})).unwrap_err();
        assert_eq!(error.path,format!("sections[\"first\"].footnotes.kinds.footnote.{key}"));
        assert!(validate_default_section_numbering(&doc,&json!({})).is_err());
    }
    let inherited_page=two_uniform_note_scopes(json!({"kinds":{"footnote":{"start":1}}}));
    let global=json!({"footnotes":{"restart":"page"}});
    assert!(project_legacy(&inherited_page,&global).is_ok());
    assert_eq!(validate_html(&inherited_page,&global).unwrap_err().path,"sections[\"first\"].footnotes.kinds.footnote.restart");
}

#[test]
fn default_only_guard_is_selected_by_sparse_numbering_provenance_not_placement_or_old_global_options() {
    let old=crate::md_ast::parse("Body[^a].\n\n[^a]: Definition.\n");
    let global=json!({"footnotes":{"numberFormat":"upperLetter","start":27,"restart":"page"}});
    assert!(!has_section_numbering(&old));
    validate_html(&old,&global).unwrap();
    validate_default_section_numbering(&old,&global).unwrap();
    let placement=two_uniform_note_scopes(json!({"kinds":{"footnote":{"placement":"pageBottom"}}}));
    assert!(!has_section_numbering(&placement));
    validate_html(&placement,&global).unwrap();
    let nulls=two_uniform_note_scopes(json!({"kinds":{"footnote":{"start":null,"restart":null}}}));
    assert!(!has_section_numbering(&nulls));
    validate_html(&nulls,&global).unwrap();
    let explicit=two_uniform_note_scopes(json!({"kinds":{"footnote":{"start":1}}}));
    assert!(has_section_numbering(&explicit));
    assert!(validate_html(&explicit,&global).is_err());
}

#[test]
fn omitted_section_endnote_format_uses_native_lower_roman_before_flat_equivalence(){
    let doc=two_uniform_note_scopes(json!({"kinds":{"endnote":{"start":1}}}));
    let options=json!({"footnotes":{"position":"document"}});
    let flat=project_legacy(&doc,&options).unwrap();
    assert_eq!(flat["footnotes"]["numberFormat"],"lowerRoman");
    assert_eq!(flat["footnotes"]["start"],1);
    assert_eq!(flat["footnotes"]["position"],"document");
    for result in [validate_html(&doc,&options),validate_default_section_numbering(&doc,&options)]{
        let error=result.unwrap_err();assert_eq!(error.code,UNSUPPORTED);
        assert_eq!(error.path,"sections[\"first\"].footnotes.kinds.endnote.numberFormat");
    }
    let explicit_decimal=two_uniform_note_scopes(json!({"kinds":{"endnote":{"numberFormat":"decimal","start":1}}}));
    validate_html(&explicit_decimal,&options).unwrap();
    assert_eq!(project_legacy(&explicit_decimal,&options).unwrap()["footnotes"]["numberFormat"],"decimal");
    assert_eq!(project_legacy(&crate::md_ast::parse("Body[^a].\n\n[^a]: Definition.\n"),&options).unwrap(),options,"global-only legacy requests retain their established contract");
}
