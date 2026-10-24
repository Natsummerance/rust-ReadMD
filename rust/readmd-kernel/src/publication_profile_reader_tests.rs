use super::*;

fn annotation(value: Value) -> String {
    format!("<!--readmd-numbering-import:v1 {}-->", serde_json::to_string(&value).unwrap())
}
fn conflict() -> Value {
    json!({"code":"docx_numbering_import_conflict","reason":"caption_result_modified",
        "kind":"table","source_id":"tab:current","visible_content_preserved":true})
}

#[test]
fn unknown_and_invalid_profiles_remain_readable_with_current_utf16_diagnostics() {
    for (member, code, path) in [
        ("{version: 1, future: '007'}", "publication_metadata_field", "source.readmd.publication.future"),
        ("{version: 1, footnotes: {start: bad}}", "publication_note_policy_invalid", "source.readmd.publication.footnotes.start"),
        ("{version: 1, footnotes: {kindByLabel: {absent: endnote}}}", "publication_note_binding_mismatch", "source.readmd.publication.footnotes.kindByLabel[\"absent\"]"),
        ("KEEP_ME", "publication_metadata_type", "source.readmd.publication"),
    ] {
        let source = format!("\u{feff}---\r\ntitle: '😀 中文'\r\nreadmd: {{publication: {member}}}\r\n---\r\nKEEP_VISIBLE 😀\r\n");
        let prepared = prepare(&source, "not-a-resource-directory").unwrap();
        assert_eq!(prepared.content, source);
        assert_eq!(prepared.data["active"], true);
        let diagnostics = prepared.data["diagnostics"].as_array().unwrap();
        assert_eq!(diagnostics.len(), 1, "{source}");
        let diagnostic = &diagnostics[0];
        assert_eq!(diagnostic["code"], code);
        assert_eq!(diagnostic["path"], path);
        assert_eq!(diagnostic["key"], path);
        assert_eq!(diagnostic["revision"], crate::document_transaction::revision(&source));
        let start = source.find(member).unwrap();
        assert_eq!(diagnostic["from"], source[..start].encode_utf16().count());
        assert_eq!(diagnostic["to"], source[..start + member.len()].encode_utf16().count());
        assert_eq!(prepared.warnings, vec![format!("{code}: {path}")]);
        assert!(crate::publication_profile::resolve(&source, &json!({})).is_err()
            || code == "publication_note_binding_mismatch");
    }
}

#[test]
fn source_profiles_never_coerce_numeric_labels_or_unknown_values_and_do_not_add_reader_defaults() {
    let source = "---\nreadmd:\n  publication:\n    version: 1\n    future: {start: '00123', size: '007'}\n    footnotes:\n      kinds:\n        footnote: {start: 0, size: 30}\n      kindByLabel: {'00123': footnote, start: footnote, size: footnote}\n---\nText[^00123][^start][^size]\n\n[^00123]: Numeric label\n\n[^start]: Named start\n\n[^size]: Named size\n";
    let profile = crate::publication_profile::from_source(source).unwrap();
    assert_eq!(profile.source_values["future"]["start"], "00123");
    assert_eq!(profile.source_values["future"]["size"], "007");
    for label in ["00123", "start", "size"] { assert_eq!(profile.options["footnotes"]["kindByLabel"][label], "footnote"); }
    assert_eq!(profile.options["footnotes"]["kinds"]["footnote"]["start"], 0);
    assert_eq!(profile.options["footnotes"]["kinds"]["footnote"]["size"], 30);
    let prepared = prepare(source, "not-a-resource-directory").unwrap();
    assert_eq!(prepared.content, source);
    assert_eq!(prepared.data["diagnostics"].as_array().unwrap().len(), 1);
    let ordinary = prepare("# Ordinary\n\nVisible text.\n", "not-a-resource-directory").unwrap();
    assert_eq!(ordinary.data["active"], false);
    assert!(ordinary.data["diagnostics"].as_array().unwrap().is_empty());
    assert!(ordinary.warnings.is_empty());
}

#[test]
fn actual_numbering_annotations_surface_current_ranges_without_replacing_visible_content() {
    let comment = annotation(json!([conflict()]));
    let source = format!("KEEP_CURRENT 😀 中文\r\n\r\n{comment}\r\nAFTER_CURRENT\r\n");
    let prepared = prepare(&source, "not-a-resource-directory").unwrap();
    assert_eq!(prepared.content, source);
    assert_eq!(prepared.data["active"], true);
    let diagnostic = &prepared.data["diagnostics"][0];
    assert_eq!(diagnostic["code"], "docx_numbering_import_conflict");
    assert_eq!(diagnostic["reason"], "caption_result_modified");
    assert_eq!(diagnostic["key"], "tab:current");
    assert_eq!(diagnostic["path"], "source.readmd-numbering-import");
    let from = source.find(&comment).unwrap();
    assert_eq!(diagnostic["from"], source[..from].encode_utf16().count());
    assert_eq!(diagnostic["to"], source[..from + comment.len()].encode_utf16().count());
    assert_eq!(diagnostic["revision"], crate::document_transaction::revision(&source));
    assert_eq!(prepared.warnings, vec!["docx_numbering_import_conflict: tab:current"]);
}

#[test]
fn numbering_annotations_in_code_frontmatter_and_nested_examples_are_inert() {
    let comment = annotation(json!([conflict()]));
    for source in [
        format!("````html\n{comment}\n````"),
        format!("    {comment}"),
        format!("> {comment}"),
        format!("- example\n\n    {comment}"),
        format!("<pre>\n{comment}\n</pre>"),
        format!("<span>{comment}</span>"),
        format!("---\nexample: |\n    {comment}\n---\nBody"),
    ] {
        assert!(numbering_import_diagnostics(&source).is_empty(), "{source}");
        let prepared = prepare(&source, "not-a-resource-directory").unwrap();
        assert_eq!(prepared.content, source);
        assert!(prepared.data["diagnostics"].as_array().unwrap().is_empty(), "{source}");
        assert!(prepared.warnings.is_empty(), "{source}");
    }
}

#[test]
fn damaged_unknown_and_over_budget_numbering_annotations_keep_the_source_and_warn() {
    for comment in [
        "<!--readmd-numbering-import:v999 []-->".into(),
        "<!--readmd-numbering-import:v1 not json-->".into(),
        annotation(json!([{"code":"untrusted_code","reason":"fake","visible_content_preserved":true}])),
        annotation(json!([{"code":"docx_numbering_import_conflict","reason":"fake","visible_content_preserved":true,"from":123}])),
        format!("<!--readmd-numbering-import:v1 {}-->", " ".repeat(256 * 1024)),
    ] {
        let source = format!("KEEP_CURRENT\n\n{comment}\n");
        let prepared = prepare(&source, "not-a-resource-directory").unwrap();
        assert_eq!(prepared.content, source);
        let diagnostic = &prepared.data["diagnostics"][0];
        assert!(matches!(diagnostic["code"].as_str(), Some("docx_numbering_invalid" | "docx_numbering_budget")));
        assert_eq!(diagnostic["visible_content_preserved"], true);
        assert_eq!(diagnostic["revision"], crate::document_transaction::revision(&source));
        assert_eq!(prepared.warnings.len(), 1);
    }
    let comments = std::iter::repeat_n(annotation(json!([conflict()])), 9).collect::<Vec<_>>().join("\n\n");
    let diagnostics = numbering_import_diagnostics(&comments);
    assert_eq!(diagnostics.len(), 9);
    assert_eq!(diagnostics[8]["code"], "docx_numbering_budget");
}
