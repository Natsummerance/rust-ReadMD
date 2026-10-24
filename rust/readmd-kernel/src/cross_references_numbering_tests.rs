use super::*;
use crate::document_transaction::{apply, byte_offset, revision, ContentTransaction};
use crate::md_ast::{Block, Inline};

fn config(value: Value) -> NumberingConfig { serde_json::from_value(value).unwrap() }
fn source(value: Value, body: &str) -> String { format!("{}\n\n{body}", numbering_comment(&config(value)).unwrap()) }
fn outline() -> Value { json!({"headings":{"levels":[{},{}]}}) }
fn chapter_config() -> Value {
    json!({"headings":{"levels":[{},{}]},"captions":{
        "figure":{"scope":"chapter","separator":"-","captionSeparator":": "},
        "table":{"scope":"chapter","separator":"."},
        "equation":{"scope":"chapter","separator":"."}}})
}
fn text_labels(projection: &Projection) -> Vec<&str> { projection.labels.iter().map(|v| v["text"].as_str().unwrap()).collect() }

#[test]
fn legacy_projection_and_marker_keep_the_exact_four_field_contract() {
    let input = "# Author title\n\n$$x$$\n\n:label[eq:first] First\n\nSee :ref[eq:first].\n";
    let projected = project(input);
    assert!(projected.numbering.is_none());
    assert_eq!(text_labels(&projected), vec!["Equation 1"]);
    assert!(projected.content.contains("<!--readmd:caption:v1 {\"id\":\"eq:first\",\"kind\":\"equation\",\"number\":1,\"prefix\":\"Equation\"}-->"));
    assert!(projected.labels[0].get("numbering").is_none());
    assert!(numbering_edit(input, None).unwrap().is_none());
    assert_eq!(strip_numbering_for_view(input).unwrap(), input);
}

#[test]
fn strict_config_roundtrips_and_bounds_every_native_counter_input() {
    let valid = config(chapter_config());
    let comment = numbering_comment(&valid).unwrap();
    assert_eq!(read_numbering_comment(&comment).unwrap(), Some(valid));
    for value in [json!({}), json!({"unknown":1}), json!({"headings":{"levels":[]}}),
        json!({"headings":{"levels":[{"format":"arabic"}]}}),
        json!({"headings":{"levels":[{"start":-1}]}}),
        json!({"headings":{"levels":[{"start":10001}]}}),
        json!({"headings":{"levels":[{"restartAfter":1}]}}),
        json!({"headings":{"levels":[{}, {"restartAfter":2}]}}),
        json!({"headings":{"levels":[{}],"separator":"%2"}}),
        json!({"headings":{"levels":[{}],"suffix":"newline"}}),
        json!({"captions":{"figure":{"scope":"chapter"}}}),
        json!({"captions":{"figure":{"scope":"section"}}}),
        json!({"captions":{"figure":{"start":-1}}}),
        json!({"captions":{"figure":{"format":"hex"}}}),
        json!({"captions":{"figure":{"chapterLevel":0}}}),
        json!({"captions":{"other":{}}}),
        json!({"captions":{"figure":{"prefix":"injected\nline"}}}),
        json!({"captions":{"figure":{"captionSeparator":"\t"}}}),
        json!({"captions":{"figure":{"unknown":true}}})] {
        let raw = format!("{NUMBERING_PREFIX}v1 {value} -->");
        assert_eq!(read_numbering_comment(&raw), Err(NUMBERING_INVALID), "{value}");
    }
    let oversized = format!("{NUMBERING_PREFIX}v1 {} -->", " ".repeat(MAX_NUMBERING_BYTES));
    assert_eq!(read_numbering_comment(&oversized), Err(NUMBERING_BUDGET));
    assert_eq!(read_numbering_comment("<!-- readmd-numbering:v2 {} -->"), Err(NUMBERING_INVALID));
}

#[test]
fn native_level_templates_suffixes_legal_numbers_zero_and_explicit_off_share_one_counter_plan() {
    let input = source(json!({"headings":{"levels":[
        {"format":"decimal","start":0,"text":"Part %1)","suffix":"tab"},
        {"format":"upperLetter","start":28,"text":"[%2/%1/%2] %x %9","suffix":"nothing"},
        {"format":"lowerRoman","start":3,"text":"%1:%2:%3","isLegal":true,"suffix":"space"}
    ]}}),"# Zero {#zero}\n\n## Letter\n\n### Legal\n\n# Manual {.unnumbered}\n\n# One\n\n## Reset\n");
    let plan=analyze_numbering(&input).unwrap().unwrap();
    assert_eq!(plan.headings.iter().map(|heading|heading.numbering.as_ref().map(HeadingNumbering::full_text)).collect::<Vec<_>>(),
        vec![Some("Part 0)\t".into()),Some("[BB/0/BB] %x ".into()),Some("0:28:3 ".into()),None,Some("Part 1)\t".into()),Some("[BB/1/BB] %x ".into())]);
    assert!(plan.headings[3].unnumbered);
    assert_eq!(plan.headings[0].numbering.as_ref().unwrap().rules[1].suffix,"nothing");
    assert_eq!(numbering_edit(&input,Some(&plan.config)).unwrap(),None);
}

#[test]
fn zero_caption_start_and_safe_foreign_sequence_roundtrip_without_underflow_or_field_injection() {
    let input=source(json!({"captions":{"equation":{"start":0,"sequence":"Native_Equation"}}}),
        "$$x$$\n\n:label[eq:zero] First\n\n$$y$$\n\n:label[eq:one] Next\n\nSee :ref[eq:zero].\n");
    let projected=project(&input);
    assert!(projected.diagnostics.is_empty());
    assert_eq!(text_labels(&projected),vec!["Equation 0","Equation 1"]);
    let label=projected.content.lines().find_map(read_marker).unwrap();
    assert_eq!(label.number,0);assert_eq!(label.sequence(),"Native_Equation");
    assert_eq!(read_marker(&marker(&label)),Some(label));
    for sequence in ["", "9bad", "Native \\r 9", "A\" \\h", "A-B", "Z\nOther"] {
        assert_eq!(validate_numbering_config(&config(json!({"captions":{"equation":{"sequence":sequence}}}))),Err(NUMBERING_INVALID));
    }
    assert_eq!(validate_numbering_config(&config(json!({"captions":{"equation":{"sequence":"same"},"figure":{"sequence":"SAME"}}}))),Err(NUMBERING_INVALID));
}

#[test]
fn source_api_preserves_sparse_null_and_explicit_default_choices_in_one_reversible_edit() {
    let original=json!({"headings":{"levels":[{"start":0,"text":null,"isLegal":false}]}});
    let input=format!("\u{feff}<!-- readmd-numbering:v1 {original} -->\r\n\r\n# Body\r\n");
    let get=numbering_request(&json!({"content":input})).unwrap();
    assert_eq!(get["config"],original);
    let mut explicit=original.clone();explicit["headings"]["levels"][0]["format"]=json!("decimal");
    let changed=numbering_request(&json!({"action":"apply","content":input,"base_revision":get["revision"],"config":explicit})).unwrap();
    assert_eq!(changed["result"]["changed"],true);
    assert_eq!(changed["config"],explicit);
    assert_eq!(changed["transaction"]["edits"].as_array().unwrap().len(),1);
    let content=changed["result"]["content"].as_str().unwrap();
    assert!(content.ends_with("# Body\r\n"));assert!(!content.replace("\r\n","").contains('\n'));
    let inverse:Vec<crate::document_transaction::TextEdit>=serde_json::from_value(changed["result"]["inverse"].clone()).unwrap();
    let inverse=ContentTransaction{base_revision:revision(content),origin:"editor".into(),edits:inverse};
    assert_eq!(apply(content,&inverse).unwrap().content,input);
    let same=numbering_request(&json!({"action":"apply","content":content,"base_revision":revision(content),"config":explicit})).unwrap();
    assert_eq!(same["result"]["changed"],false);
    assert!(valid_sequence("图_甲"));
}

#[test]
fn declarations_only_take_effect_before_real_top_level_body() {
    let comment = numbering_comment(&config(outline())).unwrap();
    for literal in [format!("```md\n{comment}\n```\n"), format!("    {comment}\n"),
        format!("---\nexample: '{comment}'\n---\n# Literal\n"), format!("`{comment}`\n")] {
        assert!(analyze_numbering(&literal).unwrap().is_none(), "{literal}");
        assert_eq!(project(&literal).content, literal);
    }
    for invalid in [format!("# Too late\n\n{comment}\n"), format!("{comment}\n\n{comment}\n"),
        format!("> {comment}\n"), format!("- {comment}\n"), format!("Text {comment}\n"),
        format!("[^x]: {comment}\n\nA[^x].\n")] {
        assert_eq!(analyze_numbering(&invalid), Err(NUMBERING_INVALID), "{invalid}");
        assert_eq!(project(&invalid).content, invalid);
    }
}

#[test]
fn all_six_levels_share_the_exact_native_level_text_and_suffix() {
    let input = source(json!({"headings":{"levels":[{},{},{},{},{},{}],"separator":"-","suffix":"tab"}}),
        "# One\n\n## Two\n\n### Three\n\n#### Four\n\n##### Five\n\n###### Six\n");
    let plan = analyze_numbering(&input).unwrap().unwrap();
    let last = plan.headings[5].numbering.as_ref().unwrap();
    assert_eq!(last.numbers, vec![1,1,1,1,1,1]);
    assert_eq!(last.text, "1-1-1-1-1-1");
    assert_eq!(last.suffix, "\t");
    assert_eq!(last.rules[5].text, "%1-%2-%3-%4-%5-%6");
    assert_eq!(last.rules.iter().map(|r| r.restart_after).collect::<Vec<_>>(), vec![0,1,2,3,4,5]);
}

#[test]
fn explicit_ancestor_restart_and_continuous_children_match_the_counter_plan() {
    let input = source(json!({"headings":{"levels":[{"format":"upperRoman","start":2},
        {"format":"lowerLetter","restartAfter":0},{"format":"lowerRoman","restartAfter":1}],"suffix":"nothing"}}),
        "# A\n\n## B\n\n### C\n\n## D\n\n### E\n\n# F\n\n## G\n\n### H\n");
    let plan = analyze_numbering(&input).unwrap().unwrap();
    assert_eq!(plan.headings.iter().map(|h| h.numbering.as_ref().unwrap().full_text()).collect::<Vec<_>>(),
        vec!["II","II.a","II.a.i","II.b","II.b.ii","III","III.c","III.c.i"]);
    assert_eq!(plan.headings[0].numbering.as_ref().unwrap().rules[1].restart_after, 0);
}

#[test]
fn missing_ancestors_and_counter_overflow_fail_without_inventing_headings() {
    let input = source(outline(), "## Missing first level\n");
    assert_eq!(analyze_numbering(&input), Err(NUMBERING_HIERARCHY));
    let input = source(json!({"headings":{"levels":[{"start":10000}]}}), "# Last\n\n# Overflow\n");
    assert_eq!(analyze_numbering(&input), Err(NUMBERING_BUDGET));
}

#[test]
fn nested_headings_and_examples_never_consume_outline_numbers() {
    let input = source(outline(), "# Same\n\n> # Same\n\n- ## Same\n\n```md\n# Same\n```\n\n# Same\n\n### Deeper but unnumbered\n");
    let plan = analyze_numbering(&input).unwrap().unwrap();
    assert_eq!(plan.headings.len(), 3);
    assert_eq!(plan.headings.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(), vec!["same","same-3","deeper-but-unnumbered"]);
    assert_eq!(plan.headings[1].numbering.as_ref().unwrap().text, "2");
    assert!(plan.headings[2].numbering.is_none());
}

#[test]
fn captions_and_references_reset_per_kind_per_chapter_and_keep_stable_ids() {
    let body = "See :ref[eq:b], :ref[tbl:b], :ref[fig:b].\n\n# First {#first}\n\n$$x$$\n\n:label[eq:a] First\n\n| A |\n|---|\n| B |\n\n:label[tbl:a] First\n\n![A](a.png)\n\n:label[fig:a] First\n\n$$y$$\n\n:label[eq:a2]\n\n# Second {#second}\n\n$$z$$\n\n:label[eq:b] Second\n\n| A |\n|---|\n| C |\n\n:label[tbl:b] Second\n\n![B](b.png)\n\n:label[fig:b] **Second**\n";
    let projected = project(&source(chapter_config(), body));
    assert!(projected.diagnostics.is_empty(), "{:?}", projected.diagnostics);
    assert_eq!(text_labels(&projected), vec!["Equation 1.1","Table 1.1","Figure 1-1","Equation 1.2","Equation 2.1","Table 2.1","Figure 2-1"]);
    assert!(projected.content.contains("[Equation 2.1](#eq:b)"));
    assert!(projected.content.contains("Figure 2-1: **Second**"));
    assert_eq!(projected.labels[4]["numbering"]["chapter"]["id"], "second");
    let changed = body.replace("# First {#first}", "# Renamed {#first}");
    let renamed = project(&source(chapter_config(), &changed));
    let identities = |p: &Projection| p.labels.iter().map(|label| (label["id"].clone(), label["text"].clone(), label["numbering"].clone())).collect::<Vec<_>>();
    assert_eq!(identities(&renamed), identities(&projected));
}

#[test]
fn chapter_reorder_changes_numbers_without_changing_label_identity() {
    let a = "# A {#a}\n\n$$a$$\n\n:label[eq:a]\n\n";
    let b = "# B {#b}\n\n$$b$$\n\n:label[eq:b]\n\n";
    let first = project(&source(chapter_config(), &format!("{a}{b}:ref[eq:b]")));
    let reordered = project(&source(chapter_config(), &format!("{b}{a}:ref[eq:b]")));
    assert_eq!(first.labels[1]["id"], reordered.labels[0]["id"]);
    assert!(first.content.contains("[Equation 2.1](#eq:b)"));
    assert!(reordered.content.contains("[Equation 1.1](#eq:b)"));
}

#[test]
fn caption_format_start_and_native_ordinal_are_distinct() {
    let input = source(json!({"headings":{"levels":[{"format":"upperRoman","start":3}]},
        "captions":{"equation":{"scope":"chapter","format":"upperLetter","start":26,"separator":"/","labelSeparator":":","captionSeparator":" = "}}}),
        "# Chapter\n\n$$x$$\n\n:label[eq:a] First\n\n$$y$$\n\n:label[eq:b] Next\n\n$$z$$\n\n:label[eq:c] Third\n");
    let projected = project(&input);
    assert_eq!(text_labels(&projected), vec!["Equation:III/Z","Equation:III/AA","Equation:III/BB"]);
    assert_eq!(projected.labels[0]["number"], 26);
    assert_eq!(projected.labels[1]["numbering"]["ordinal"], 2);
    assert_eq!(projected.labels[1]["numbering"]["start"], 26);
    assert_eq!(projected.labels[2]["numbering"]["ordinal"], 3);
    assert_eq!(word_number_format("upperLetter"), "ALPHABETIC");
    assert_eq!(word_number_format("lowerRoman"), "roman");
}

#[test]
fn nonconfigured_kinds_and_source_language_keep_legacy_behavior() {
    let config = numbering_comment(&config(json!({"headings":{"levels":[{}]},"captions":{"figure":{"scope":"chapter"}}}))).unwrap();
    let input = format!("---\nlang: zh-CN\n---\n{config}\n\n# 章\n\n$$x$$\n\n:label[eq:x]\n\n![图](x.png)\n\n:label[fig:x]\n");
    let projected = project(&input);
    assert_eq!(text_labels(&projected), vec!["公式 1","图 1.1"]);
    assert!(projected.labels[0].get("numbering").is_none());
    assert!(projected.labels[1].get("numbering").is_some());
}

#[test]
fn chapter_scoped_caption_requires_the_selected_actual_heading() {
    let body = "# A\n\n## AA\n\n$$x$$\n\n:label[eq:a]\n\n# B\n\n$$y$$\n\n:label[eq:missing]\n";
    let input = source(json!({"headings":{"levels":[{},{}]},"captions":{"equation":{"scope":"chapter","chapterLevel":2}}}), body);
    let projected = project(&input);
    assert_eq!(text_labels(&projected), vec!["Equation 1.1.1"]);
    assert!(projected.diagnostics.iter().any(|d| d["code"] == NUMBERING_CHAPTER_MISSING && d["key"] == "eq:missing"));
    assert_eq!(validate_source(&input), Err(NUMBERING_CHAPTER_MISSING));
    assert!(projected.content.contains(":label[eq:missing]"));
}

#[test]
fn inline_math_and_mixed_display_prose_cannot_declare_block_equations() {
    for body in ["Inline $x$.\n\n:label[eq:no]\n", "Before $$x$$ after.\n\n:label[eq:no]\n"] {
        let projected = project(body);
        assert!(projected.labels.is_empty());
        assert_eq!(projected.content, body);
        assert_eq!(projected.diagnostics[0]["code"], "academic_invalid_label");
    }
    for body in ["$$x$$\n\n:label[eq:yes]\n", "$$\nx\n$$\n\n:label[eq:yes]\n", "```readmd-math v1\nx\n```\n\n:label[eq:yes]\n"] {
        let projected = project(body);
        assert_eq!(projected.labels.len(), 1, "{body}: {:?}", projected.diagnostics);
    }
}

#[test]
fn an_imported_display_math_metadata_comment_is_invisible_to_caption_association() {
    let omml=r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:r><m:t>x</m:t></m:r></m:oMath>"#;
    let metadata=crate::math_document::metadata("x",omml,"caption-math").unwrap();
    assert!(is_math_capsule_comment(&metadata));
    assert!(is_math_capsule_comment(&format!(" \r\n{metadata}\t\r\n")));
    for body in [format!("$$x$${metadata}\n\n:label[eq:yes] Current title\n"),
        format!("$$\nx\n$${metadata}\r\n\r\n:label[eq:yes] Current title\r\n"),
        format!("$$x$$\n\n{metadata}\n\n:label[eq:yes] Current title\n"),
        format!("$$x$$\r\n\r\n  {metadata}\t  \r\n\r\n:label[eq:yes] Current title\r\n")] {
        let projected=project(&body);
        assert_eq!(text_labels(&projected),vec!["Equation 1"],"{:?}",projected.diagnostics);
        assert!(projected.diagnostics.is_empty());
        assert!(projected.content.contains(&metadata));
        assert_eq!(projected.labels[0]["id"],"eq:yes");
    }
    for fragment in [format!("Inline $x${metadata}"),format!("Before $$x$${metadata}"),
        format!("$$x$${metadata} after."),format!("$$x$${metadata} $$y$$"),
        format!("> $$x$${metadata}"),format!("```md\n$$x$${metadata}\n```")] {
        let body=format!("{fragment}\n\n:label[eq:no]\n");
        let projected=project(&body);
        assert!(projected.labels.is_empty(),"{body}");
        assert_eq!(projected.content,body);
        assert!(projected.diagnostics.iter().any(|diagnostic|diagnostic["code"]=="academic_invalid_label"));
    }
    // A block HTML event contains the entire line, including visible text
    // after its comment. Prefix matching must not hide that intervening body.
    for bridge in [format!("{metadata} VISIBLE_PROSE"),format!("VISIBLE_PROSE {metadata}"),
        format!("{metadata} VISIBLE_PROSE <!-- author -->"),format!("{metadata}{metadata}"),
        format!("{metadata}<!-- author -->"),"<!--readmd:math:v1:-->".into(),
        "<!--readmd:math:v1:unterminated".into()] {
        assert!(!is_math_capsule_comment(&bridge),"{bridge}");
        let body=format!("$$x$$\n\n{bridge}\n\n:label[eq:no]\n");
        let projected=project(&body);
        assert!(projected.labels.is_empty(),"{body}");
        assert_eq!(projected.content,body);
        // An unterminated HTML comment owns the following label text; it is
        // still literal and cannot declare an automatic caption.
        if bridge.ends_with("-->") || bridge.contains("-->") {
            assert!(projected.diagnostics.iter().any(|diagnostic|diagnostic["code"]=="academic_invalid_label"),"{body}");
        }
    }
    let repeated=format!("$$x$${metadata}{metadata}\n\n:label[eq:no]\n");
    let projected=project(&repeated);
    assert!(projected.labels.is_empty());
    assert_eq!(projected.content,repeated);
    assert!(projected.diagnostics.iter().any(|diagnostic|diagnostic["code"]=="academic_invalid_label"));
}

#[test]
fn caption_marker_roundtrip_rejects_inconsistent_numbers_and_escapes_comments() {
    let input = source(json!({"captions":{"equation":{"prefix":"Eq --> *[x] &amp;","captionSeparator":" < "}}}), "$$x$$\n\n:label[eq:x] Content\n");
    let projected = project(&input);
    let marker_line = projected.content.lines().find(|line| line.starts_with("<!--readmd:caption:v1")).unwrap();
    assert!(!marker_line.contains("Eq -->"));
    let label = read_marker(marker_line).unwrap();
    assert_eq!(label.prefix, "Eq --> *[x] &amp;");
    assert_eq!(read_marker(&marker(&label)), Some(label.clone()));
    let mut invalid = label;
    invalid.numbering.as_mut().unwrap().ordinal = 2;
    assert!(read_marker(&marker(&invalid)).is_none());
    let doc = crate::md_ast::parse(&input);
    assert!(doc.blocks.iter().any(|block| matches!(block, Block::Caption {label,..} if label.prefix == "Eq --> *[x] &amp;")));
}

#[test]
fn typed_caption_prefix_consumes_only_complete_text_and_balances_projected_author_runs(){
    let label=Label{id:"tbl:split-prefix".into(),kind:"table".into(),number:1,prefix:"Table العربية".into(),numbering:None};let nonce="0123456789abcdef0123456789abcdef";
    let content=format!("{}\n\n<span id=\"{}\" class=\"publication-caption\"></span><span data-readmd-author-run=\"{nonce}-0\" style=\"font-size:30pt\">Table </span><span data-readmd-author-run=\"{nonce}-1\" style=\"font-size:10pt\">العربية </span><span data-readmd-author-run=\"{nonce}-2\" style=\"font-size:30pt\">1 — **TITLE**</span>\n",marker(&label),label.id);
    let document=crate::md_ast::parse(&content);let [Block::Caption{label:actual,inlines}]=document.blocks.as_slice()else{panic!("complete typed prefix was not attached: {:?}",document.blocks)};assert_eq!(actual,&label);assert_eq!(crate::md_ast::inline_plain(inlines)," — TITLE");let mut depth=0;for value in inlines{if let crate::md_ast::Inline::Html(value)=value{if projected_author_run_open(value){depth+=1;}else if value=="</span>"{assert!(depth>0,"an already consumed label run left an orphan closer");depth-=1;}}}assert_eq!(depth,0);
}

#[test]
fn typed_caption_requires_exact_anchor_and_complete_label_before_any_title(){
    let label=Label{id:"tbl:strict-prefix".into(),kind:"table".into(),number:27,prefix:"Table".into(),numbering:None};let nonce="0123456789abcdef0123456789abcdef";
    for body in [format!("<span id=\"tbl:other\" class=\"publication-caption\"></span>Table 27 — TITLE"),"Table 27 — TITLE".into(),format!("<span id=\"{}\" class=\"publication-caption\"></span>Table 2",label.id),format!("<span id=\"{}\" class=\"publication-caption\"></span><span style=\"font-size:30pt\">Table 27</span> — TITLE",label.id),format!("<span id=\"{}\" class=\"publication-caption\"></span><span data-readmd-author-run=\"{nonce}-0\" style=\"font-size:30pt;position:fixed\">Table 27</span> — TITLE",label.id)]{
        let source=format!("{}\n\n{body}\n",marker(&label));let document=crate::md_ast::parse(&source);assert!(!document.blocks.iter().any(|block|matches!(block.unstyled(),Block::Caption{..})),"a false anchor/prefix became a caption: {body}");
    }
}

#[test]
fn repeated_projection_is_content_idempotent_and_literal_examples_survive() {
    let input = source(chapter_config(), "# One\n\n$$x$$\n\n:label[eq:x]\n\n:ref[eq:x]\n\n`:ref[eq:x]`\n\n```md\n:label[eq:hidden]\n:ref[eq:x]\n```\n\n> $$y$$\n>\n> :label[eq:nested]\n");
    let once = project(&input);
    let twice = project(&once.content);
    assert_eq!(twice.content, once.content);
    assert_eq!(once.labels.len(), 1);
    assert!(once.content.contains("`:ref[eq:x]`"));
    assert!(once.content.contains("```md\n:label[eq:hidden]\n:ref[eq:x]\n```"));
    assert!(once.content.contains(":label[eq:nested]"));
}

#[test]
fn source_owned_edit_preserves_bom_crlf_and_has_an_exact_transaction_inverse() {
    let input = "\u{feff}---\r\ntitle: 😀 Boundary\r\n---\r\n# Original\r\n\r\nText.\r\n";
    let config = config(outline());
    let edit = numbering_edit(input, Some(&config)).unwrap().unwrap();
    let result = apply(input, &ContentTransaction { base_revision: revision(input), origin: "editor".into(), edits: vec![edit] }).unwrap();
    assert!(result.content.starts_with("\u{feff}---\r\ntitle: 😀 Boundary\r\n---\r\n<!-- readmd-numbering:"));
    assert!(!result.content.replace("\r\n", "").contains('\n'));
    assert!(numbering_edit(&result.content, Some(&config)).unwrap().is_none());
    let undone = apply(&result.content, &ContentTransaction { base_revision: result.revision, origin: "editor".into(), edits: result.inverse }).unwrap();
    assert_eq!(undone.content, input);
    let clear = numbering_edit(&result.content, None).unwrap().unwrap();
    let cleared = apply(&result.content, &ContentTransaction { base_revision: revision(&result.content), origin: "editor".into(), edits: vec![clear] }).unwrap();
    assert!(!cleared.content.contains(NUMBERING_PREFIX));
    assert!(cleared.content.ends_with("# Original\r\n\r\nText.\r\n"));
}

#[test]
fn utf16_ranges_and_original_heading_ids_survive_numbering_attachment() {
    let input = source(outline(), "# 😀 **Same** {#stable}\n\n## Same\n\n# 😀 **Same** {#stable}\n");
    let plan = analyze_numbering(&input).unwrap().unwrap();
    for heading in &plan.headings {
        assert_eq!(&input[byte_offset(&input, heading.from).unwrap()..byte_offset(&input, heading.to).unwrap()], &input[heading.bytes.clone()]);
    }
    let doc = crate::md_ast::parse(&input);
    let headings: Vec<_> = doc.blocks.iter().filter_map(|b| if let Block::Heading {id,inlines,numbering,..} = b.unstyled() { Some((id, inlines, numbering)) } else { None }).collect();
    assert_eq!(headings.len(), 3);
    assert_eq!(headings[0].0, "stable");
    assert_eq!(headings[2].0, "stable-1");
    assert_eq!(headings[2].2.as_ref().unwrap().text, "2");
    assert!(headings[0].1.iter().any(|i| matches!(i, Inline::Strong(_))));
    assert_eq!(crate::md_ast::inline_plain(headings[0].1), "😀 Same");
}

#[test]
fn numbering_edit_rejects_a_new_missing_chapter_before_returning_an_edit() {
    let body = "$$x$$\n\n:label[eq:x]\n";
    assert_eq!(numbering_edit(body, Some(&config(chapter_config()))), Err(NUMBERING_CHAPTER_MISSING));
    assert_eq!(body, "$$x$$\n\n:label[eq:x]\n");
}
