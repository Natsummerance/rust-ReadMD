use super::*;
use crate::math_document::WORD_NS;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

const SPACING: &str = "<m:preSp m:val=\"120\"/><m:postSp m:val=\"240\"/><m:interSp m:val=\"40\"/><m:intraSp m:val=\"60\"/>";
fn office(properties: Option<&str>, display: bool) -> Block {
    let properties = properties.map(|s| format!("<m:mathPr>{s}</m:mathPr>")).unwrap_or_default();
    let omml = format!("{properties}<m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>");
    if display { Block::OfficeMath { latex: "x".into(), omml } } else { Block::Paragraph(vec![Inline::OfficeMath { latex: "x".into(), omml }]) }
}
fn off(display: bool) -> Block { office(Some(&format!("<m:dispDef m:val=\"0\"/>{SPACING}")), display) }
fn document(blocks: Vec<Block>) -> Document { Document { front_matter: None, blocks } }
fn latex(s: &str) -> Block { Block::Math(s.into()) }
fn ordinary_formula() -> MathDocument { math_document::parse("x", "latex", true).unwrap() }
fn xml(block: &Block) -> &str { if let Block::OfficeMath { omml, .. } = block { omml } else { panic!("Expected rendering OfficeMath") } }
fn state(xml: &str) -> MathDefaultsState { MathDefaultsState::from_xml(&math_document::parse_xml(xml).unwrap()).unwrap() }
fn error_code<T: std::fmt::Debug>(value: Result<T, DefaultsError>, code: &str) { assert_eq!(value.unwrap_err(), source(code)); }
fn conflict_property(doc: &Document, property: &'static str) { assert!(matches!(DocumentDefaultsPlan::analyze(doc), Err(DefaultsError::Conflict { property: actual, .. }) if actual == property)); }

#[test]
fn ordinary_alone_and_compatible_explicit_on_keep_borrowed_author_tree() {
    for author in [document(vec![latex("x")]), document(vec![office(Some("<m:dispDef m:val=\"1\"/>"), true), latex("x")]), Document::default()] {
        let original = author.clone(); let plan = DocumentDefaultsPlan::analyze(&author).unwrap(); assert!(!plan.is_active());
        assert!(matches!(plan.render_document(&author).unwrap(), Cow::Borrowed(_))); assert_eq!(author, original); plan.validate_doc(&author).unwrap();
    }
}

#[test]
fn all_four_display_spacings_and_false_are_projected_in_both_source_orders() {
    for reverse in [false, true] {
        let mut blocks = vec![latex("x"), off(true)]; if reverse { blocks.reverse(); }
        let author = document(blocks); let original = author.clone(); let plan = DocumentDefaultsPlan::analyze(&author).unwrap(); assert!(plan.is_active());
        let rendered = plan.render_document(&author).unwrap(); let index = if reverse { 1 } else { 0 }; let effective = state(xml(&rendered.blocks[index])).effective;
        assert!(!effective.display_defaults); assert_eq!([effective.pre_spacing, effective.post_spacing, effective.inter_spacing, effective.intra_spacing], [120, 240, 40, 60]);
        let root = crate::omml::parse(xml(&rendered.blocks[index])).unwrap(); let style = crate::omml::math_paragraph_style(&root, None); assert_eq!([style.before, style.after, style.inter, style.intra], [6., 12., 2., 3.]);
        assert_eq!(rendered.blocks[1 - index], author.blocks[1 - index]); assert_eq!(author, original);
    }
}

#[test]
fn full_zero_based_occurrences_keep_repeated_latex_distinct_from_native_sites() {
    let author = document(vec![latex("x"), off(true), latex("x")]); let plan = DocumentDefaultsPlan::analyze(&author).unwrap();
    assert_eq!(plan.formula_count(), 3); assert!(plan.is_ordinary(0)); assert!(!plan.is_ordinary(1)); assert!(plan.is_ordinary(2)); assert!(!plan.is_ordinary(3));
    let original = ordinary_formula(); for index in [0, 2] { assert!(!MathDefaultsState::from_xml(&plan.render_formula(&original, index, true).unwrap().root).unwrap().effective.display_defaults); }
    error_code(plan.render_formula(&original, 1, true), "transaction_conflict"); error_code(plan.render_formula(&original, 3, true), "transaction_conflict"); error_code(plan.render_formula(&original, 2, false), "transaction_conflict");
}

#[test]
fn inline_provider_never_supplies_display_spacings_or_small_fraction() {
    let inline = office(Some(&format!("<m:smallFrac/><m:dispDef m:val=\"false\"/>{SPACING}")), false);
    for author in [document(vec![inline.clone(), latex("x")]), document(vec![latex("x"), inline])] {
        let plan = DocumentDefaultsPlan::analyze(&author).unwrap(); let rendered = plan.render_document(&author).unwrap(); let index = if matches!(author.blocks[0], Block::Math(_)) { 0 } else { 1 }; let effective = state(xml(&rendered.blocks[index])).effective;
        assert!(!effective.display_defaults); assert!(!effective.small_fraction); assert_eq!([effective.pre_spacing, effective.post_spacing, effective.inter_spacing, effective.intra_spacing], [0; 4]);
    }
}

#[test]
fn inline_ordinary_consumes_global_false_without_borrowing_word_paragraphs() {
    let author = document(vec![off(true), Block::Paragraph(vec![Inline::Text("keep ".into()), Inline::Math("x".into())])]); let plan = DocumentDefaultsPlan::analyze(&author).unwrap();
    let original = math_document::parse("x", "latex", false).unwrap(); let rendered = plan.render_formula(&original, 1, false).unwrap(); assert!(!MathDefaultsState::from_xml(&rendered.root).unwrap().effective.display_defaults);
    let projected = plan.render_document(&author).unwrap(); assert!(matches!(&projected.blocks[1], Block::Paragraph(v) if matches!(&v[0], Inline::Text(s) if s == "keep ")));
}

#[test]
fn explicit_on_implicit_on_and_off_remain_conflicts_in_both_orders() {
    for on in ["<m:dispDef/>", "<m:dispDef m:val=\"on\"/>", "<m:dispDef m:val=\"1\"/>"] {
        for pair in [[office(Some(on), true), off(true)], [off(true), office(Some(on), true)]] { conflict_property(&document(pair.into_iter().collect()), "dispDef"); }
    }
}

#[test]
fn source_display_fences_cannot_lose_explicit_conflicts_at_paragraph_boundaries() {
    let disabled="row(office.mathPr(office.dispDef(val: \"0\")), office.oMathPara(office.oMath(x)))";
    let requirements=["row(office.mathPr(office.dispDef()), office.oMathPara(office.oMath(x)))",
                      "row(office.mathPr(office.dispDef(val: \"on\")), office.oMathPara(office.oMath(x)))",
                      "row(office.mathPr(office.dispDef(val: \"1\")), office.oMathPara(office.oMath(x)))",
                      "office.oMathPara(office.oMath(x))"];
    for eol in ["\n","\r\n"] { for enabled in requirements { for reverse in [false,true] {
        let bodies=if reverse {[enabled,disabled]}else{[disabled,enabled]};
        // Fences legally interrupt these paragraphs. No QA-inserted blank
        // lines or hand-built author AST can hide the production source path.
        let source=format!("M07_PREFIX_😀{eol}```readmd-math v1{eol}{}{eol}```{eol}M07_SUFFIX_KEEP{eol}```readmd-math v1{eol}{}{eol}```{eol}AFTER_𝛂",bodies[0],bodies[1]);
        let original=source.clone();
        let author=crate::md_ast::parse(&source);
        let native=author.blocks.iter().filter_map(|block|match block {Block::OfficeMath {omml,..}=>Some(omml),_=>None}).collect::<Vec<_>>();
        assert_eq!(native.len(),2,"Native declaration became ordinary math: {source}");
        assert_eq!(author.blocks.len(),5);
        for (actual,body) in native.iter().zip(bodies) {
            let expected=crate::math_dsl::compile(body,true).unwrap().0;
            assert_eq!(math_document::xml_semantics(&math_document::parse_xml(actual).unwrap()),math_document::xml_semantics(&expected));
        }
        let error=DocumentDefaultsPlan::analyze(&author).unwrap_err();
        assert!(matches!(&error,DefaultsError::Conflict {property,..} if *property=="dispDef"),"{error:?}");
        let rendered_json=crate::md_ast::to_render_json(&author.blocks);
        assert_eq!(crate::math_render_json::resolve(&rendered_json).unwrap_err(),error.message());
        assert_eq!(source,original);
    }}}
}

#[test]
fn native_absence_and_empty_mathpr_are_hard_legacy_requirements() {
    for bare in [office(None, true), office(Some(""), true)] { for pair in [[bare.clone(), off(true)], [off(true), bare]] { conflict_property(&document(pair.into_iter().collect()), "dispDef"); } }
}

#[test]
fn incompatible_display_providers_do_not_get_resolved_by_source_order() {
    let changed = office(Some("<m:dispDef m:val=\"0\"/><m:preSp m:val=\"121\"/><m:postSp m:val=\"240\"/><m:interSp m:val=\"40\"/><m:intraSp m:val=\"60\"/>"), true);
    for blocks in [vec![off(true), latex("x"), changed.clone()], vec![changed, latex("x"), off(true)]] { conflict_property(&document(blocks), "preSp"); }
}

#[test]
fn every_non_admitted_global_difference_remains_strict() {
    for (property, extra) in [("mathFont", "<m:mathFont m:val=\"STIX Two Math\"/>"), ("brkBin", "<m:brkBin m:val=\"after\"/>"), ("brkBinSub", "<m:brkBinSub m:val=\"+-\"/>"), ("smallFrac", "<m:smallFrac/>")] {
        let native = office(Some(&format!("{extra}<m:dispDef m:val=\"0\"/>{SPACING}")), true);
        for blocks in [vec![native.clone(), latex("x")], vec![latex("x"), native]] { conflict_property(&document(blocks), property); }
    }
    for (property, properties) in [("wrapIndent", "<m:wrapIndent m:val=\"0\"/>"), ("lMargin", "<m:lMargin/>"), ("smallFrac", "<m:smallFrac/>")] { conflict_property(&document(vec![office(Some(properties), true), latex("x")]), property); }
}

#[test]
fn disabled_only_numbers_are_not_reinterpreted_as_explicit_zero_or_inherited_word_axes() {
    // wrapIndent and wrapRight occupy the same schema Choice. Exercise both
    // legal declarations independently rather than bypassing that validator.
    for wrap in ["<m:wrapIndent m:val=\"240\"/>", "<m:wrapRight/>"] {
        let native = office(Some(&format!("<m:dispDef m:val=\"off\"/><m:lMargin/><m:rMargin/><m:defJc m:val=\"right\"/>{SPACING}{wrap}<m:intLim m:val=\"undOvr\"/>")), true);
        let author = document(vec![native.clone(), latex("x")]); let plan = DocumentDefaultsPlan::analyze(&author).unwrap(); let rendered = plan.render_document(&author).unwrap(); assert_eq!(rendered.blocks[0], native);
        let value = state(xml(&rendered.blocks[1])).effective; assert_eq!([value.left_margin, value.right_margin, value.wrap_indent], [0; 3]); assert!(!value.wrap_right); assert!(!value.integral_limits);
    }
}

#[test]
fn ordinary_render_copy_never_copies_word_styles_or_paragraph_context() {
    let mut native = off(true); if let Block::OfficeMath { omml, .. } = &mut native { omml.insert_str(0, "<w:styles><w:style w:type=\"paragraph\" w:styleId=\"Equation\"><w:pPr><w:ind w:left=\"1mm\"/></w:pPr></w:style></w:styles><w:pPr><w:pStyle w:val=\"Equation\"/></w:pPr>"); }
    let author = document(vec![native.clone(), latex("x")]); let plan = DocumentDefaultsPlan::analyze(&author).unwrap(); let rendered = plan.render_document(&author).unwrap(); assert_eq!(rendered.blocks[0], native);
    let root = math_document::parse_xml(xml(&rendered.blocks[1])).unwrap(); let mut pending = vec![&root]; while let Some(node) = pending.pop() { assert_ne!(node.namespace, WORD_NS); pending.extend(node.children()); }
}

#[test]
fn rendered_formula_preserves_all_author_fields_and_existing_node_ids() {
    let author = document(vec![off(true), latex("\\frac{α}{𝛂}")]); let plan = DocumentDefaultsPlan::analyze(&author).unwrap(); let original = math_document::parse("\\frac{α}{𝛂}", "latex", true).unwrap(); let before = serde_json::to_vec(&original).unwrap(); let rendered = plan.render_formula(&original, 1, true).unwrap();
    assert_eq!(serde_json::to_vec(&original).unwrap(), before); assert_eq!(rendered.source, original.source); assert_eq!(rendered.revision, original.revision); assert_eq!(rendered.dialect, original.dialect); assert!(same_nodes(&rendered, &original));
    assert_eq!(&rendered.root.content[1..], original.root.content.as_slice()); let first = rendered.root.children().next().unwrap(); assert_eq!(first.id, "rd-document-defaults"); assert!(original.root.children().all(|node| node.id != first.id));
    error_code(plan.render_formula(&rendered, 1, true), "transaction_conflict");
}

#[test]
fn native_formula_is_self_validated_but_never_receives_ordinary_projection() {
    let native = off(true); let author = document(vec![native.clone(), latex("x")]); let plan = DocumentDefaultsPlan::analyze(&author).unwrap(); let original = math_document::parse(xml(&native), "omml", true).unwrap(); let rendered = plan.render_formula(&original, 0, true).unwrap(); assert_eq!(serde_json::to_vec(&rendered).unwrap(), serde_json::to_vec(&original).unwrap());
}

#[test]
fn changed_source_revision_dialect_root_or_source_nodes_cannot_reuse_binding() {
    let author = document(vec![off(true), latex("x")]); let plan = DocumentDefaultsPlan::analyze(&author).unwrap(); let original = ordinary_formula();
    let mut candidates = Vec::new(); let mut changed = original.clone(); changed.source = "y".into(); changed.revision = transaction::revision("y"); candidates.push(changed); let mut changed = original.clone(); changed.revision = "old".into(); candidates.push(changed); let mut changed = original.clone(); changed.dialect = "omml".into(); candidates.push(changed); let mut changed = original.clone(); changed.root.id = "forged".into(); candidates.push(changed); let mut changed = original.clone(); changed.nodes[0].id = "other-token".into(); candidates.push(changed);
    for changed in candidates { error_code(plan.render_formula(&changed, 1, true), "transaction_conflict"); }
}

#[test]
fn stale_author_tree_and_second_pass_projection_are_rejected() {
    let author = document(vec![off(true), latex("x")]); let plan = DocumentDefaultsPlan::analyze(&author).unwrap(); let rendered = plan.render_document(&author).unwrap(); error_code(plan.render_document(&rendered), "transaction_conflict"); error_code(plan.validate_doc(&rendered), "transaction_conflict");
    let mut changed = author.clone(); changed.blocks.push(latex("x")); error_code(plan.render_document(&changed), "transaction_conflict"); assert_ne!(DocumentDefaultsPlan::analyze(&changed).unwrap().fingerprint(), plan.fingerprint());
}

#[test]
fn fingerprint_covers_context_source_body_and_exact_crlf_spelling() {
    let author = document(vec![off(true), Block::Paragraph(vec![Inline::Text("甲😀\r\n".into())]), latex("x")]); let plan = DocumentDefaultsPlan::analyze(&author).unwrap(); assert_eq!(plan.fingerprint().len(), 64);
    for changed in [document(vec![off(true), Block::Paragraph(vec![Inline::Text("甲😀\n".into())]), latex("x")]), document(vec![off(true), Block::Paragraph(vec![Inline::Text("changed".into())]), latex("x")])] { error_code(plan.validate_doc(&changed), "transaction_conflict"); }
}

#[test]
fn traversal_uses_one_math_sequence_across_wrappers_tables_notes_and_lists() {
    let author = document(vec![off(true), Block::Quote(vec![latex("a")]), Block::List { ordered: false, start: 1, items: vec![crate::md_ast::ListItem { task: None, blocks: vec![Block::Paragraph(vec![Inline::Strong(vec![Inline::Math("b".into())])])] }] }, Block::Table { aligns: vec![crate::md_ast::Align::None], header: vec![vec![Inline::Math("c".into())]], rows: vec![vec![vec![Inline::Math("d".into())]]] }, Block::FootnoteDef { label: "note".into(), blocks: vec![latex("e")] }]);
    let plan = DocumentDefaultsPlan::analyze(&author).unwrap(); assert_eq!(plan.formula_count(), 6); for (index, (text, display)) in [("a", true), ("b", false), ("c", false), ("d", false), ("e", true)].into_iter().enumerate() { plan.render_formula(&math_document::parse(text, "latex", display).unwrap(), index + 1, display).unwrap(); }
    let rendered = plan.render_document(&author).unwrap(); assert!(matches!(&rendered.blocks[1], Block::Quote(v) if matches!(&v[0], Block::OfficeMath {..}))); assert_eq!(author.blocks[0], rendered.blocks[0]);
}

#[test]
fn ordinary_body_code_and_literal_dollars_are_not_sources_of_math_defaults() {
    let author = document(vec![Block::Code { lang: "text".into(), text: "<m:dispDef m:val=\"0\"/> $$x$$".into() }, Block::Paragraph(vec![Inline::Text("$not math$".into()), Inline::Code("office.mathPr()".into())]), latex("x")]); let plan = DocumentDefaultsPlan::analyze(&author).unwrap(); assert_eq!(plan.formula_count(), 1); assert!(!plan.is_active()); assert!(matches!(plan.render_document(&author).unwrap(), Cow::Borrowed(_)));
}

#[test]
fn malformed_native_mathpr_retains_namespace_duplicate_and_required_value_guards() {
    for properties in ["<m:dispDef m:val=\"0\"/><m:dispDef m:val=\"0\"/>", "<m:dispDef m:val=\"0\"/><m:preSp/>", "<x:dispDef xmlns:x=\"urn:foreign\" x:val=\"0\"/>"] { error_code(DocumentDefaultsPlan::analyze(&document(vec![office(Some(properties), true), latex("x")])), "math_invalid_property"); }
}

#[test]
fn streaming_inventory_handles_a_million_character_body_without_projecting_it() {
    let author = document(vec![Block::Paragraph(vec![Inline::Text("a".repeat(1_000_000))]), latex("x")]); let plan = DocumentDefaultsPlan::analyze(&author).unwrap(); assert!(matches!(plan.render_document(&author).unwrap(), Cow::Borrowed(_))); assert_eq!(plan.formula_count(), 1);
}

#[test]
fn document_formula_node_and_depth_budgets_fail_before_cloning() {
    error_code(DocumentDefaultsPlan::analyze(&Document { front_matter: Some("x".repeat(transaction::MAX_DOCUMENT_BYTES + 1)), blocks: vec![latex("x")] }), "math_budget");
    error_code(DocumentDefaultsPlan::analyze(&document((0..=transaction::MAX_OPERATIONS).map(|_| latex("")).collect())), "math_budget");
    error_code(DocumentDefaultsPlan::analyze(&document(vec![Block::Paragraph((0..=MAX_AST_NODES).map(|_| Inline::Text(String::new())).collect()), latex("x")])), "math_budget");
    let mut nested = latex("x"); for _ in 0..=MAX_DEPTH { nested = Block::Quote(vec![nested]); } error_code(DocumentDefaultsPlan::analyze(&document(vec![nested])), "math_budget");
}

#[test]
fn cancellation_interrupts_analysis_fingerprinting_and_rendering() {
    let author = document(vec![off(true), latex("x")]); let plan = DocumentDefaultsPlan::analyze(&author).unwrap(); let original = ordinary_formula();
    crate::speech::with_cancel(Arc::new(|| true), || { error_code(DocumentDefaultsPlan::analyze(&author), "cancelled"); error_code(plan.validate_doc(&author), "cancelled"); error_code(plan.render_document(&author), "cancelled"); error_code(plan.render_formula(&original, 1, true), "cancelled"); });
    let count = Arc::new(AtomicUsize::new(0)); let check = count.clone(); let large = document(vec![Block::Paragraph(vec![Inline::Text("x".repeat(1_000_000))]), latex("x")]); crate::speech::with_cancel(Arc::new(move || check.fetch_add(1, Ordering::Relaxed) > 8), || error_code(DocumentDefaultsPlan::analyze(&large), "cancelled"));
}

#[test]
fn no_math_documents_keep_borrowed_behavior_without_mathematical_node_caps() {
    let author = document(vec![Block::Paragraph((0..=MAX_AST_NODES).map(|_| Inline::Text(String::new())).collect())]); let plan = DocumentDefaultsPlan::analyze(&author).unwrap(); assert_eq!(plan.formula_count(), 0); assert!(!plan.is_active()); assert!(matches!(plan.render_document(&author).unwrap(), Cow::Borrowed(_)));
    error_code(plan.validate_doc(&document(vec![latex("x")])), "transaction_conflict");
}
