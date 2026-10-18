use super::*;

fn patch(source: &str, value: Value) -> transaction::TransactionResult {
    let tx = plan_patch(source, &transaction::revision(source), &value).unwrap();
    transaction::apply(source, &tx).unwrap()
}

#[test]
fn literal_names_and_identifiers_keep_commas_and_leading_zeros() {
    let source="author: 'Curie, Marie'\naffiliation: 'University, Laboratory'\nkeywords: 'one, two'\nfunding:\n  - funder: Council\n    award: 00127\nisbn: '978-0-306-40615-7'\n";
    let m = resolve(Some(source), &json!({}), "paper.md").unwrap();
    assert_eq!(m.authors.len(), 1);
    assert_eq!(m.authors[0].name, "Curie, Marie");
    assert_eq!(m.affiliations[0].name, "University, Laboratory");
    assert_eq!(m.keywords, vec!["one, two"]);
    assert_eq!(m.funding[0].award.as_deref(), Some("00127"));
    assert_eq!(m.identifiers[0].value, "978-0-306-40615-7");
}
#[test]
fn source_relationships_are_typed_and_ordered() {
    let source="authors:\n  - id: a1\n    name: Ada\n    affiliationIds: [i1, i2]\n    corresponding: true\n    orcid: 0000-0002-1825-0097\n  - id: a2\n    name: Bob\naffiliations:\n  - {id: i1, name: 'First, Lab'}\n  - {id: i2, name: Second}\nfunding:\n  - {funder: Council, recipientIds: [a2, a1]}\n";
    let m = resolve(Some(source), &json!({}), "paper").unwrap();
    assert!(m.authors[0].corresponding);
    assert_eq!(m.authors[0].affiliation_ids, vec!["i1", "i2"]);
    assert_eq!(m.funding[0].recipient_ids, vec!["a2", "a1"]);
}
#[test]
fn layers_distinguish_inherit_value_and_clear() {
    let source = "title: Source\nauthor: Ada\nsubtitle: Old\n";
    let m = resolve_layers(
        Some(source),
        &[
            json!({"meta":{"title":"","author":"","clear":["subtitle"]}}),
            json!({"meta":{"title":null,"subtitle":"New","authors":[]}}),
        ],
        "file.md",
    )
    .unwrap();
    assert_eq!(m.title, "Source");
    assert_eq!(m.subtitle.as_deref(), Some("New"));
    assert!(m.authors.is_empty());
    assert!(m.cleared_fields.contains("authors"));
    assert!(!m.cleared_fields.contains("subtitle"));
    assert_eq!(
        resolve(
            Some(source),
            &json!({"meta":{"clear":["subtitle"],"subtitle":"Conflict"}}),
            ""
        )
        .unwrap_err()
        .code,
        "publication_metadata_clear_conflict"
    );
}
#[test]
fn option_merge_does_not_resurrect_cleared_values() {
    let base = json!({"meta":{"subtitle":"Old","authors":["Ada"]},"page":{"size":"A4"}});
    let clear = merge_options(
        &base,
        &json!({"meta":{"clear":["subtitle"],"authors":[]},"page":{"size":null}}),
    )
    .unwrap();
    assert_eq!(clear.pointer("/meta/clear"), Some(&json!(["subtitle"])));
    assert_eq!(clear.pointer("/meta/authors"), Some(&json!([])));
    assert_eq!(clear.pointer("/page/size"), Some(&json!("A4")));
    let next = merge_options(&clear, &json!({"meta":{"subtitle":"New"}})).unwrap();
    assert!(next.pointer("/meta/clear").is_none());
    assert_eq!(next.pointer("/meta/subtitle"), Some(&json!("New")));
}
#[test]
fn aliases_conflict_but_p02_language_fallback_is_preserved() {
    assert_eq!(
        resolve(Some("author: Ada\nauthors: [Bob]\n"), &json!({}), "")
            .unwrap_err()
            .code,
        "publication_metadata_alias_conflict"
    );
    let m = resolve(
        Some("lang: de_DE\nlanguage: ja\n"),
        &json!({"meta":{"language":"bad--tag","lang":"fr_CA"}}),
        "",
    )
    .unwrap();
    assert_eq!(m.language.as_deref(), Some("fr-CA"));
}
#[test]
fn unicode_error_range_survives_a_literal_abstract() {
    let source="\u{feff}---\r\ntitle: '中文🧭'\r\nabstract: |\r\n  第一行𝛂\r\n  第二行\r\nauthors:\r\n  - name: Ada\r\n    orcid: '0000-0002-1825-0098'\r\n---\r\n正文\r\n";
    let error = from_source(source, &json!({}), "").unwrap_err();
    assert_eq!(error.code, "publication_metadata_orcid");
    let from = transaction::byte_offset(source, error.from).unwrap();
    let to = transaction::byte_offset(source, error.to).unwrap();
    assert_eq!(&source[from..to], "'0000-0002-1825-0098'");
}
#[test]
fn yaml_is_bounded_and_does_not_construct_or_expand() {
    for source in [
        "authors: &a [Ada]\n",
        "authors: *a\n",
        "title: !!str Unsafe\n",
        "title: One\n---\ntitle: Two\n",
    ] {
        assert!(resolve(Some(source), &json!({}), "").is_err(), "{source}");
    }
    let deep = format!("custom: {}0{}\n", "[".repeat(18), "]".repeat(18));
    assert_eq!(
        resolve(Some(&deep), &json!({}), "").unwrap_err().code,
        "publication_metadata_budget"
    );
    let oversized = format!("title: '{}'\n", "a".repeat(MAX_FRONT_MATTER));
    assert_eq!(
        resolve(Some(&oversized), &json!({}), "").unwrap_err().code,
        "publication_metadata_budget"
    );
    assert!(resolve(Some("# Comment only\n\n"), &json!({}), "").is_ok());
}
#[test]
fn invalid_ids_and_checksums_have_field_errors() {
    for (value, code) in [
        (
            json!({"authors":[{"id":"a","name":"Ada"},{"id":"a","name":"Bob"}]}),
            "publication_metadata_duplicate_id",
        ),
        (
            json!({"authors":[{"name":"Ada","affiliationIds":["missing"]}]}),
            "publication_metadata_reference",
        ),
        (
            json!({"identifiers":[{"scheme":"isbn","value":"9780306406158"}]}),
            "publication_metadata_isbn",
        ),
        (
            json!({"identifiers":[{"scheme":"orcid","value":"0000-0002-1825-0097"}]}),
            "publication_metadata_identifier_role",
        ),
    ] {
        assert_eq!(
            resolve(None, &json!({"meta":value}), "").unwrap_err().code,
            code
        );
    }
}
#[test]
fn one_name_edit_is_one_utf16_value_edit_with_inverse() {
    let source="\u{feff}---\r\n# retained\r\ntitle: '中文🧭' # title\r\nauthors:\r\n  - id: a1\r\n    name: 'Ada, A.' # author\r\n    private: 'leave: me'\r\n  - id: a2\r\n    name: Bob\r\nfunding: [{funder: Council, recipientIds: [a1]}]\r\ncustom: {x: 9} # untouched\r\n---\r\n# Body 🧭\r\n";
    let mut authors = json!(from_source(source, &json!({}), "").unwrap().authors);
    authors[0]["name"] = json!("Élodie, 甲");
    let tx = plan_patch(
        source,
        &transaction::revision(source),
        &json!({"authors":authors}),
    )
    .unwrap();
    assert_eq!(tx.edits.len(), 1);
    let result = transaction::apply(source, &tx).unwrap();
    assert_eq!(
        result.content,
        source.replace("'Ada, A.'", "\"Élodie, 甲\"")
    );
    let inverse = ContentTransaction {
        base_revision: result.revision.clone(),
        origin: "editor".into(),
        edits: result.inverse,
    };
    assert_eq!(
        transaction::apply(&result.content, &inverse)
            .unwrap()
            .content,
        source
    );
}
#[test]
fn reordering_authors_moves_original_rows_and_comments() {
    let source="---\nauthors:\n  - id: a1\n    name: Ada # first\n    private: {x: 1}\n  - id: a2\n    name: Bob # second\ncustom: keep\n---\nBody\n";
    let mut authors = json!(from_source(source, &json!({}), "").unwrap().authors);
    authors.as_array_mut().unwrap().reverse();
    let result = patch(source, json!({"authors":authors}));
    assert_eq!(result.content,"---\nauthors:\n  - id: a2\n    name: Bob # second\n  - id: a1\n    name: Ada # first\n    private: {x: 1}\ncustom: keep\n---\nBody\n");
}

#[test]
fn leading_author_comments_follow_crlf_unicode_rows_and_inverse_is_exact() {
    let prefix = "\u{feff}---\r\n# Parent header stays here.\r\ntitle: \"中文🧭\"\r\nauthors:\r\n";
    let first = "  # First author's leading comment.\r\n  - id: \"linh\"\r\n    name: \"Nguyen, Linh\"\r\n    affiliationIds: [north, east]\r\n    private: {last: \"first row, final value\"} # inline stays\r\n";
    let second = "  # This comment travels with the second author.\r\n  # 第二位作者的相邻注释 🧭\r\n  - id: \"mo\"\r\n    name: \"林墨\"\r\n    affiliationIds: [east]\r\n";
    let suffix = "# Parent trailing comment stays here.\r\naffiliations: [{id: north, name: \"North, Institute\"}, {id: east, name: \"东岸研究院\"}]\r\nfunding: [{funder: Council, recipientIds: [linh, mo]}]\r\nunknown-extension: {note: \"a,b: unchanged\", enabled: true}\r\n---\r\n# Body 😀 中文 remains unchanged.\r\n";
    let source = [prefix, first, second, suffix].concat();
    let mut authors = json!(from_source(&source, &json!({}), "").unwrap().authors);
    authors.as_array_mut().unwrap().reverse();
    let result = patch(&source, json!({"authors":authors}));
    assert_eq!(result.content, [prefix, second, first, suffix].concat());
    assert!(result.content.contains("# This comment travels with the second author.\r\n  # 第二位作者的相邻注释 🧭\r\n  - id: \"mo\""));
    let model = from_source(&result.content, &json!({}), "").unwrap();
    assert_eq!(model.authors.iter().map(|person|person.id.as_str()).collect::<Vec<_>>(), vec!["mo", "linh"]);
    assert_eq!(model.funding[0].recipient_ids, vec!["linh", "mo"]);
    let inverse = ContentTransaction {base_revision:result.revision.clone(), origin:"editor".into(), edits:result.inverse};
    assert_eq!(transaction::apply(&result.content, &inverse).unwrap().content, source);
}

#[test]
fn duplicate_person_names_match_ids_and_rename_preserves_leading_comments() {
    let prefix = "---\r\nauthors:\r\n";
    let first = "  # Identity first only.\r\n  - id: first\r\n    name: \"共同姓名\"\r\n    private: \"first private value\"\r\n";
    let second = "  # Identity second only.\r\n  - id: second\r\n    name: \"共同姓名\" # original name note\r\n    private: \"second private value\"\r\n";
    let suffix = "unknown: {parent: keep}\r\n---\r\nBody 🧭\r\n";
    let source = [prefix, first, second, suffix].concat();
    let mut authors = json!(from_source(&source, &json!({}), "").unwrap().authors);
    let mut reversed = authors.clone(); reversed.as_array_mut().unwrap().reverse();
    let reordered = patch(&source, json!({"authors":reversed}));
    assert_eq!(reordered.content, [prefix, second, first, suffix].concat());
    let inverse = ContentTransaction {base_revision:reordered.revision.clone(), origin:"editor".into(), edits:reordered.inverse};
    assert_eq!(transaction::apply(&reordered.content, &inverse).unwrap().content, source);
    authors[1]["name"] = json!("第二位，校订");
    let tx = plan_patch(&source, &transaction::revision(&source), &json!({"authors":authors})).unwrap();
    assert_eq!(tx.edits.len(), 1);
    let renamed = transaction::apply(&source, &tx).unwrap();
    assert_eq!(renamed.content, [prefix, first, &second.replace("\"共同姓名\"", "\"第二位，校订\""), suffix].concat());
    let inverse = ContentTransaction {base_revision:renamed.revision.clone(), origin:"editor".into(), edits:renamed.inverse};
    assert_eq!(transaction::apply(&renamed.content, &inverse).unwrap().content, source);
}

#[test]
fn leading_comment_ownership_stops_at_blank_indent_value_and_parent_boundaries() {
    // The blank line makes this a stationary array-header comment, rather
    // than the first row's leading trivia. Literal/deeper hashes stay in A.
    let prefix = "---\r\nparent-before: \"last parent value\"\r\nauthors:\r\n  # Stationary header comment.\r\n\r\n";
    let first = "  - id: first\r\n    name: First\r\n    private: |-\r\n      previous item's final literal value\r\n      # literal content, not row trivia\r\n    # deeper first-item comment\r\n\r\n";
    let second = "  # Next-row comment one.\r\n  # Next-row comment two.\r\n  - id: second\r\n    name: Second\r\n";
    let suffix = "# Parent-scope comment.\r\nunknown-extension: {note: \"parent after, untouched\"}\r\n---\r\nBody remains exact.\r\n";
    let source = [prefix, first, second, suffix].concat();
    let mut authors = json!(from_source(&source, &json!({}), "").unwrap().authors);
    authors.as_array_mut().unwrap().reverse();
    let result = patch(&source, json!({"authors":authors}));
    assert_eq!(result.content, [prefix, second, first, suffix].concat());
    for comment in ["# Stationary header comment.", "# literal content, not row trivia", "# deeper first-item comment", "# Next-row comment one.", "# Next-row comment two.", "# Parent-scope comment."] {
        assert_eq!(result.content.matches(comment).count(), 1);
    }
    let inverse = ContentTransaction {base_revision:result.revision.clone(), origin:"editor".into(), edits:result.inverse};
    assert_eq!(transaction::apply(&result.content, &inverse).unwrap().content, source);
}
#[test]
fn add_delete_and_flow_values_retain_unowned_properties() {
    let source="---\nauthors: [{id: a1, name: Ada, private: yes}, {id: a2, name: Bob}] # note\ncustom: keep\n---\nBody\n";
    let mut authors = json!(from_source(source, &json!({}), "").unwrap().authors);
    authors.as_array_mut().unwrap().reverse();
    authors[1]["name"] = json!("New");
    let result = patch(source, json!({"authors":authors}));
    assert!(result.content.contains("private: yes"));
    assert!(result.content.contains("# note\ncustom: keep\n---\nBody\n"));
    let empty = patch(&result.content, json!({"authors":[]}));
    assert!(from_source(&empty.content, &json!({}), "")
        .unwrap()
        .authors
        .is_empty());
}
#[test]
fn no_op_and_stale_patch_protect_source() {
    let source = "---\nauthor: 'Ada, A.'\ncustom: keep\n---\nBody\n";
    let m = from_source(source, &json!({}), "").unwrap();
    let result = patch(source, json!({"authors":m.authors}));
    assert!(!result.changed);
    assert_eq!(result.content, source);
    assert_eq!(
        plan_patch(source, "stale", &json!({"title":"New"}))
            .unwrap_err()
            .code,
        "transaction_conflict"
    );
    let plain = "\u{feff}# 中文\r\n";
    let added = patch(plain, json!({"title":"Title"}));
    assert_eq!(
        added.content,
        "\u{feff}---\r\n\"title\": \"Title\"\r\n---\r\n# 中文\r\n"
    );
}
#[test]
fn flow_reorder_and_clear_preserve_comment_bytes() {
    let source="---\nauthors: [ # head\n  {id: a1, name: Ada}, # first\n  {id: a2, name: Bob} # tail\n]\ncustom: keep\n---\nBody\n";
    let mut authors = json!(from_source(source, &json!({}), "").unwrap().authors);
    authors.as_array_mut().unwrap().reverse();
    let changed = patch(source, json!({"authors":authors}));
    for comment in ["# head", "# first", "# tail"] {
        assert!(changed.content.contains(comment));
    }
    assert!(changed.content.find("id: a2").unwrap() < changed.content.find("id: a1").unwrap());
    let cleared = patch(&changed.content, json!({"authors":[]}));
    assert!(cleared.content.contains("authors: [] # head\n"));
    for comment in ["# head", "# first", "# tail"] {
        assert!(cleared.content.contains(comment));
        assert_eq!(cleared.content.matches(comment).count(), 1);
    }
    assert!(cleared.content.ends_with("custom: keep\n---\nBody\n"));
    assert_eq!(
        transaction::apply(&cleared.content, &transaction::ContentTransaction{base_revision:cleared.revision.clone(),origin:"editor".into(),edits:cleared.inverse.clone()})
            .unwrap()
            .content,
        changed.content
    );
    assert!(from_source(&cleared.content, &json!({}), "")
        .unwrap()
        .authors
        .is_empty());
}
#[test]
fn legacy_citation_and_presentation_patch_preserves_other_options() {
    let source="---\npresentation:\n  theme: black # theme\n  controls: false\nbibliography: [old.bib]\ncsl: apa\nlang: de\n---\nBody\n";
    let result = patch(
        source,
        json!({"presentation":{"theme":"white"},"bibliography":["new.bib"],"csl":"ieee"}),
    );
    assert!(result.content.contains("controls: false"));
    assert!(result.content.contains("# theme"));
    assert!(result.content.contains("lang: de\n---\nBody\n"));
}
#[test]
fn get_patch_and_projection_are_plain_and_language_owned() {
    let source="---\ntitle: '<script>title</script>'\nlang: de\nabstract: '**literal** $x$'\nkeywords: 'a, b'\n---\nBody\n";
    let get = handle(&json!({"content":source})).unwrap();
    assert_eq!(get["sourceValues"]["keywords"], json!(["a, b"]));
    assert!(get["ranges"]["title"]["from"].is_number());
    let result=handle(&json!({"action":"patch","content":source,"base_revision":get["revision"],"patch":{"subtitle":"New"}})).unwrap();
    assert!(result["changed"].as_bool().unwrap());
    let mut m = from_source(source, &json!({}), "").unwrap();
    assert!(front_matter_blocks(&m, &json!({})).is_empty());
    let blocks = front_matter_blocks(&m, &json!({"frontMatter":{"mode":"paper"}}));
    assert!(blocks.iter().all(|b| matches!(b, Block::Paragraph(_))));
    let literal = format!("{blocks:?}");
    assert!(literal.contains("Zusammenfassung"));
    assert!(literal.contains("**literal** $x$"));
    assert!(!literal.contains("Math("));
    m.abstracts[0]
        .text
        .push_str("\nSecond line 🧭\r\nThird line");
    let blocks = front_matter_blocks(&m, &json!({"frontMatter":{"mode":"paper"}}));
    assert!(blocks.iter().any(|b|matches!(b,Block::Paragraph(v) if v.iter().filter(|i|matches!(i,Inline::HardBreak)).count()==2)));
    let unsupported = "lang: zu-ZA\nabstract: Literal\nkeywords: 'a, b'\n";
    let fallback = resolve(
        Some(unsupported),
        &json!({"frontMatter":{"mode":"paper"}}),
        "",
    )
    .unwrap();
    let warning = fallback
        .warning_items
        .iter()
        .find(|w| w["code"] == "publication_metadata_label_fallback")
        .unwrap();
    assert_eq!(warning["language"], "zu-ZA");
    assert_eq!(warning["fallback"], "en");
    assert_eq!(warning["labels"], json!(["abstract", "keywords"]));
    assert_eq!(warning["path"], "language");
    assert_eq!(warning["coordinateSpace"], "frontMatter");
    assert_eq!(
        &unsupported
            [warning["from"].as_u64().unwrap() as usize..warning["to"].as_u64().unwrap() as usize],
        "zu-ZA"
    );
    let overridden = resolve(
        Some(unsupported),
        &json!({"frontMatter":{"mode":"paper","labels":{"abstract":"초록","keywords":"주제어"}}}),
        "",
    )
    .unwrap();
    assert!(overridden
        .warning_items
        .iter()
        .all(|w| w["code"] != "publication_metadata_label_fallback"));
    assert!(resolve(Some(unsupported), &json!({}), "")
        .unwrap()
        .warning_items
        .iter()
        .all(|w| w["code"] != "publication_metadata_label_fallback"));
}

#[test]
fn chinese_content_labels_follow_explicit_locale_script_and_region() {
    assert_ne!(
        content_labels("zh-CN").unwrap(),
        content_labels("zh-TW").unwrap()
    );
    for (language, expected) in [
        ("zh-CN", "zh-CN"),
        ("zh-TW", "zh-TW"),
        ("zh-HK", "zh-HK"),
        ("zh-Hant", "zh-TW"),
        ("zh-Hant-TW", "zh-TW"),
        ("zh-Hant-HK", "zh-HK"),
        ("zh-Hant-MO", "zh-HK"),
        ("zh-MO", "zh-HK"),
        ("zh-Hans", "zh-CN"),
        ("zh-Hans-CN", "zh-CN"),
        ("zh-SG", "zh-CN"),
        ("ZH_hant_hk", "zh-HK"),
    ] {
        assert!(
            std::ptr::eq(
                content_labels(language).unwrap(),
                content_labels(expected).unwrap()
            ),
            "{language} must use {expected} content labels"
        );
    }
}


#[test]
fn bounded_extension_member_ranges_keep_scalar_flow_and_empty_mapping_boundaries(){
    let cases=[
        ("custom: \"false\"","\"false\"",json!("false")),
        ("custom: false","false",json!(false)),
        ("custom: 0013","0013",json!("0013")),
        ("custom: {nested: {enabled: false}, token: \"0013\"}","{nested: {enabled: false}, token: \"0013\"}",json!({"nested":{"enabled":false},"token":"0013"})),
        ("custom: {}","{}",json!({})),
        ("custom: []","[]",json!([])),
        ("custom:\n  alpha: one\n  beta: \"中文😀\"","alpha: one\n  beta: \"中文😀\"",json!({"alpha":"one","beta":"中文😀"})),
    ];
    for newline in ["\n","\r\n"]{for (declaration,lexical,wanted) in &cases{
        let declaration=declaration.replace('\n',newline);let lexical=lexical.replace('\n',newline);
        let body=format!("prefix: \"中文😀\"{newline}{declaration}{newline}after: retained{newline}");
        let (value,from,to)=bounded_front_matter_member(&body,&["custom"]).unwrap().unwrap();
        assert_eq!(&value,wanted);let at=body.find(&lexical).unwrap();
        assert_eq!((from,to),(body[..at].encode_utf16().count(),body[..at+lexical.len()].encode_utf16().count()),"{body:?}");
        assert!(bounded_front_matter_member(&body,&["absent"]).unwrap().is_none());
    }}
}
