use super::*;
use crate::math_document::{parse_xml, MATH_NS};

const TEMPLATE: &str = r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:pPrDefault><w:pPr/></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:pPr><w:jc w:val="left"/></w:pPr></w:style><w:style w:type="paragraph" w:styleId="ReadMDMath"><w:name w:val="Equation"/></w:style></w:styles>"#;

fn style(id: &str, parent: Option<&str>, ppr: Option<&str>, default: bool) -> String {
    format!("<w:style w:type=\"paragraph\" w:styleId=\"{}\"{}>{}{}<w:rPr><w:b/></w:rPr></w:style>",
        attribute(id), if default { " w:default=\"1\"" } else { "" },
        parent.map_or(String::new(), |id| format!("<w:basedOn w:val=\"{}\"/>", attribute(id))),
        ppr.map_or(String::new(), |value| format!("<w:pPr>{value}</w:pPr>")))
}
fn tree(styles: &str, direct: Option<&str>, defaults: &str) -> XmlNode {
    parse_xml(&format!("<w:styles xmlns:w=\"{WORD_NS}\">{defaults}{styles}</w:styles>{}<m:oMathPara xmlns:m=\"{MATH_NS}\"><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath></m:oMathPara>",
        direct.map_or(String::new(), |value| format!("<w:pPr xmlns:w=\"{WORD_NS}\">{value}</w:pPr>")))).unwrap()
}
fn source(styles: &str, direct: Option<&str>, defaults: &str) -> NativeParagraphSource {
    capture(&tree(styles, direct, defaults)).unwrap().unwrap()
}
fn equation(left: &str) -> String {
    format!("{}{}", style("Base", None, Some(&format!("<w:ind w:left=\"{left}\" w:right=\"18.125pt\"/>")), false),
        style("Equation", Some("Base"), Some(r#"<w:jc w:val="end"/>"#), false))
}
fn native(registry: &mut NativeParagraphRegistry, source: &NativeParagraphSource) -> NativeParagraphProjection {
    let NativeParagraphResolution::Native(value) = registry.resolve(source).unwrap() else { panic!("expected native chain"); };
    value
}
fn replay(registry: &NativeParagraphRegistry, value: &NativeParagraphProjection) -> XmlNode {
    let styles = TEMPLATE.replace("</w:styles>", &format!("{}</w:styles>", registry.style_xml()));
    parse_xml(&format!("{styles}<w:pPr xmlns:w=\"{WORD_NS}\">{}</w:pPr><m:oMath xmlns:m=\"{MATH_NS}\"/>", value.ppr_xml)).unwrap()
}
fn all_styles(xml: &str) -> Vec<XmlNode> {
    parse_xml(xml).unwrap().children().filter(|node| word(node, "style")).cloned().collect()
}

#[test]
fn capture_is_pure_and_not_reconstructed_from_effective_axes() {
    let original = tree(&equation("1mm"), Some(r#"<w:pStyle w:val="Equation"/><w:ind w:left="0"/>"#), "");
    let before = original.clone(); let captured = capture(&original).unwrap().unwrap();
    assert_eq!(original, before); assert_eq!(captured.direct.as_ref().unwrap(), child(&original, "pPr").unwrap());
    assert_eq!(captured.chain.len(), 2); assert_eq!(captured.defaults_kind(), NativeParagraphDefaultsKind::Missing);
    assert_eq!(attr(child(captured.chain[0].ppr.as_ref().unwrap(), "ind").unwrap(), "left"), Some("1mm"));
    let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[captured.clone()]).unwrap();
    assert!(registry.style_xml().is_empty()); let value = native(&mut registry, &captured);
    assert_eq!(value.selected_style_id, "Equation"); assert!(value.aliases.is_empty());
    assert!(!value.ppr_xml.contains("18.125pt")); assert!(value.ppr_xml.contains("w:left=\"0\""));
    let imported = replay(&registry, &value); let context = super::super::word_paragraph_context(&imported).unwrap();
    assert_eq!(context.effective.indent["left"].value.raw(), "0");
    assert_eq!(context.effective.indent["right"].value.raw(), "18.125pt");
    assert_eq!(context.effective.indent["right"].origin, super::super::paragraph::WordParagraphOrigin::Style("Base".into()));
    assert!(!registry.style_xml().contains("w:rPr"));
    assert_eq!(original, before);
}

#[test]
fn sparse_missing_empty_containers_and_explicit_zero_do_not_merge() {
    let styles = equation("1mm");
    let missing = source(&styles, Some(r#"<w:pStyle w:val="Equation"/>"#), "");
    let empty = source(&styles, Some(r#"<w:pStyle w:val="Equation"/><w:ind/>"#), "");
    let zero = source(&styles, Some(r#"<w:pStyle w:val="Equation"/><w:ind w:left="0"/>"#), "");
    let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[missing.clone(), empty.clone(), zero.clone()]).unwrap();
    let absent = native(&mut registry, &missing); let present = native(&mut registry, &empty); let explicit = native(&mut registry, &zero);
    assert!(!absent.ppr_xml.contains("w:ind")); assert!(present.ppr_xml.contains("<w:ind ")); assert!(!present.ppr_xml.contains("w:left="));
    assert!(explicit.ppr_xml.contains("w:left=\"0\""));
    let inherited = super::super::word_paragraph_context(&replay(&registry, &present)).unwrap();
    assert_eq!(inherited.effective.indent["left"].value.raw(), "1mm");
    assert_eq!(inherited.effective.indent["left"].origin, super::super::paragraph::WordParagraphOrigin::Style("Base".into()));
}

#[test]
fn default_selection_is_explicitly_scoped_without_exporting_a_global_default() {
    let styles = style("DefaultMath", None, Some(r#"<w:ind w:left="2pc" w:right="1.5pi"/><w:jc w:val="start"/>"#), true);
    let sources = [source(&styles, None, ""), source(&styles, Some(""), "")];
    let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &sources).unwrap();
    for source in &sources {
        let before = source.clone(); let result = native(&mut registry, source);
        assert!(result.selection_was_default); assert!(result.ppr_xml.contains("w:val=\"DefaultMath\""));
        assert_eq!(*source, before); let context = super::super::word_paragraph_context(&replay(&registry, &result)).unwrap();
        assert_eq!(context.effective.indent["left"].origin, super::super::paragraph::WordParagraphOrigin::Style("DefaultMath".into()));
    }
    assert!(!registry.style_xml().contains("w:default=")); assert_eq!(all_styles(&registry.style_xml()).len(), 1);
}

#[test]
fn three_default_states_are_retained_and_nonempty_globals_report_loss() {
    let styles = equation("1mm");
    for (defaults, wanted, fallback) in [("", NativeParagraphDefaultsKind::Missing, false),
        ("<w:docDefaults><w:pPrDefault/></w:docDefaults>", NativeParagraphDefaultsKind::Empty, false),
        ("<w:docDefaults><w:pPrDefault><w:pPr/></w:pPrDefault></w:docDefaults>", NativeParagraphDefaultsKind::Empty, false),
        (r#"<w:docDefaults><w:pPrDefault><w:pPr><w:ind w:left="0"/></w:pPr></w:pPrDefault></w:docDefaults>"#, NativeParagraphDefaultsKind::Explicit, true),
        ("<w:docDefaults><w:pPrDefault><w:pPr><w:ind/></w:pPr></w:pPrDefault></w:docDefaults>", NativeParagraphDefaultsKind::Explicit, true)] {
        let value = source(&styles, Some(r#"<w:pStyle w:val="Equation"/>"#), defaults);let before = value.clone();
        assert_eq!(value.defaults_kind(), wanted); let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[value.clone()]).unwrap();
        let result = registry.resolve(&value).unwrap();
        if fallback { let NativeParagraphResolution::Materialized(loss) = result else { panic!() };
            assert_eq!(loss.code, "incompatible-paragraph-doc-defaults"); assert_eq!(loss.source_defaults, wanted); assert!(registry.style_xml().is_empty());
        } else { assert!(matches!(result, NativeParagraphResolution::Native(_))); }
        assert_eq!(value, before);
    }
    let direct_only = source("", Some(r#"<w:ind w:left="1mm"/>"#), "");
    let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[direct_only.clone()]).unwrap();
    assert_eq!(registry.resolve(&direct_only).unwrap(), NativeParagraphResolution::NotApplicable);
    let incompatible = source("", None, r#"<w:docDefaults><w:pPrDefault><w:pPr><w:ind w:left="1mm"/></w:pPr></w:pPrDefault></w:docDefaults>"#);
    let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[incompatible.clone()]).unwrap();
    assert!(matches!(registry.resolve(&incompatible).unwrap(), NativeParagraphResolution::Materialized(_)));
}

#[test]
fn template_and_cross_source_collisions_rewrite_every_based_on_without_first_wins() {
    let a = source(&equation("1mm"), Some(r#"<w:pStyle w:val="Equation"/>"#), "");
    let b_styles = format!("{}{}{}", style("Normal", None, Some(r#"<w:ind w:left="0.5in"/>"#), false),
        style("ReadMDMath", Some("Normal"), Some(r#"<w:ind w:right="1.5pi"/>"#), false), style("Equation", Some("ReadMDMath"), Some(r#"<w:jc w:val="right"/>"#), false));
    let b = source(&b_styles, Some(r#"<w:pStyle w:val="Equation"/>"#), "");
    let mut forward = NativeParagraphRegistry::prepare(TEMPLATE, &[a.clone(), b.clone()]).unwrap();
    let mut reverse = NativeParagraphRegistry::prepare(TEMPLATE, &[b.clone(), a.clone()]).unwrap();
    let fa = native(&mut forward, &a); let fb = native(&mut forward, &b); let rb = native(&mut reverse, &b); let ra = native(&mut reverse, &a);
    assert_eq!(fa, ra); assert_eq!(fb, rb); assert_eq!(forward.style_xml(), reverse.style_xml());
    assert_ne!(fa.selected_style_id, fb.selected_style_id); assert_ne!(fa.selected_style_id, "Equation");
    assert!(fb.aliases.iter().any(|row| row.source_id == "Normal" && row.native_id != "Normal"));
    assert!(fb.aliases.iter().any(|row| row.source_id == "ReadMDMath" && row.native_id != "ReadMDMath"));
    let styles = all_styles(&forward.style_xml());let ids = styles.iter().map(|node| attr(node, "styleId").unwrap()).collect::<BTreeSet<_>>();
    assert_eq!(styles.len(), ids.len());
    for style in &styles { if let Some(parent) = child(style, "basedOn").and_then(|node| attr(node, "val")) { assert!(ids.contains(parent)); } }
    let context = super::super::word_paragraph_context(&replay(&forward, &fb)).unwrap();
    assert_eq!(context.effective.indent["left"].value.raw(), "0.5in");assert_eq!(context.effective.indent["right"].value.raw(), "1.5pi");
}

#[test]
fn identical_leaf_with_different_ancestor_definitions_is_not_merged() {
    let a = source(&equation("1mm"), Some(r#"<w:pStyle w:val="Equation"/>"#), "");
    let b = source(&equation("2mm"), Some(r#"<w:pStyle w:val="Equation"/>"#), "");
    let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[a.clone(), b.clone(), a.clone()]).unwrap();
    let pa = native(&mut registry, &a);let pb = native(&mut registry, &b);
    assert_ne!(pa.selected_style_id, pb.selected_style_id);assert_eq!(all_styles(&registry.style_xml()).len(), 4);
    for (source, expected) in [(&pa, "1mm"), (&pb, "2mm")] {
        let actual = super::super::word_paragraph_context(&replay(&registry, source)).unwrap();assert_eq!(actual.effective.indent["left"].value.raw(), expected);
    }
}

#[test]
fn namespace_aliases_are_structural_equivalents_and_never_style_authority_by_prefix() {
    let root = tree(&equation("1mm"), Some(r#"<w:pStyle w:val="Equation"/>"#), "");
    let aliased = parse_xml(&root.xml().replace("w:", "q:").replace("xmlns:w", "xmlns:q")).unwrap();
    let a = capture(&root).unwrap().unwrap();let b = capture(&aliased).unwrap().unwrap();
    let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[a.clone(), b.clone()]).unwrap();
    assert_eq!(native(&mut registry, &a), native(&mut registry, &b));assert_eq!(all_styles(&registry.style_xml()).len(), 2);
    let bad = format!("{}<m:oMath xmlns:m=\"{MATH_NS}\"/>", r#"<w:pPr xmlns:w="urn:foreign"><w:pStyle w:val="Equation"/></w:pPr>"#);
    assert!(capture(&parse_xml(&bad).unwrap()).is_err());
}

#[test]
fn unit_and_integer_lexical_values_survive_attribute_roundtrips() {
    for raw in ["1mm", "0.1cm", "0.5in", "18.125pt", "2pc", "1.5pi", " +000720\t", "\r\n+00057 "] {
        let styles = equation(&attribute(raw));
        let direct = format!("<w:pStyle w:val=\"Equation\"/><w:ind w:right=\"{}\"/>", attribute(raw));
        let original = tree(&styles, Some(&direct), "");let before = original.clone();let value = capture(&original).unwrap().unwrap();
        let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[value.clone()]).unwrap();let result = native(&mut registry, &value);
        let output = replay(&registry, &result);let context = super::super::word_paragraph_context(&output).unwrap();
        assert_eq!(context.effective.indent["left"].value.raw(), raw);assert_eq!(context.effective.indent["right"].value.raw(), raw);
        assert_eq!(original, before);
        if raw.contains('\t') { assert!(result.ppr_xml.contains("&#9;") && registry.style_xml().contains("&#9;")); }
        if raw.contains('\r') { assert!(result.ppr_xml.contains("&#13;&#10;") && registry.style_xml().contains("&#13;&#10;")); }
    }
}

#[test]
fn style_ids_and_selectors_escape_xml_and_ui_names_do_not_collide_with_template() {
    let id = "A&B\"<Eq";let styles = style(id, None, Some(r#"<w:ind w:left="1mm"/>"#), false);
    let direct = format!("<w:pStyle w:val=\"{}\"/>", attribute(id));let value = source(&styles, Some(&direct), "");
    let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[value.clone()]).unwrap();let result = native(&mut registry, &value);
    assert_eq!(result.selected_style_id, id);assert!(result.ppr_xml.contains("&amp;") && result.ppr_xml.contains("&quot;"));
    assert!(super::super::word_paragraph_context(&replay(&registry, &result)).is_ok());
    let value = source(&equation("1mm"), Some(r#"<w:pStyle w:val="Equation"/>"#), "");
    let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[value.clone()]).unwrap();native(&mut registry, &value);
    let styles = all_styles(&registry.style_xml());
    assert_eq!(styles.iter().find(|style| attr(style, "styleId") == Some("Equation")).and_then(|style| child(style, "name")).and_then(|name| attr(name, "val")), Some("Equation (1)"));
}

#[test]
fn only_used_chains_are_emitted_and_preparation_is_not_authority_for_other_sources() {
    let a = source(&equation("1mm"), Some(r#"<w:pStyle w:val="Equation"/>"#), "");
    let b = source(&style("Unused", None, Some(r#"<w:ind w:right="2pc"/>"#), false), Some(r#"<w:pStyle w:val="Unused"/>"#), "");
    let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[a.clone(), b.clone()]).unwrap();assert!(registry.style_xml().is_empty());
    native(&mut registry, &a);assert!(!registry.style_xml().contains("Unused"));
    let other = source(&equation("3mm"), Some(r#"<w:pStyle w:val="Equation"/>"#), "");
    assert_eq!(registry.resolve(&other), Err("math_invalid_source"));
    let no_paragraph = parse_xml(&format!("<m:oMath xmlns:m=\"{MATH_NS}\"/>")).unwrap();assert!(capture(&no_paragraph).unwrap().is_none());
}

#[test]
fn aliases_survive_reimport_and_modified_current_definitions_override_old_digest_names() {
    let value = source(&style("Normal", None, Some(r#"<w:ind w:left="1mm"/>"#), false), Some(r#"<w:pStyle w:val="Normal"/>"#), "");
    let mut first = NativeParagraphRegistry::prepare(TEMPLATE, &[value.clone()]).unwrap();let a = native(&mut first, &value);
    let reimport = capture(&replay(&first, &a)).unwrap().unwrap();let mut second = NativeParagraphRegistry::prepare(TEMPLATE, &[reimport.clone()]).unwrap();let b = native(&mut second, &reimport);
    assert_eq!(a.selected_style_id, b.selected_style_id);assert_eq!(first.style_xml(), second.style_xml());
    let current = parse_xml(&replay(&first, &a).xml().replace("w:left=\"1mm\"", "w:left=\"2mm\"")).unwrap();
    let edited = capture(&current).unwrap().unwrap();let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[edited.clone()]).unwrap();let result = native(&mut registry, &edited);
    assert_eq!(result.selected_style_id, a.selected_style_id);let context = super::super::word_paragraph_context(&replay(&registry, &result)).unwrap();
    assert_eq!(context.effective.indent["left"].value.raw(), "2mm");assert!(!registry.style_xml().contains("w:left=\"1mm\""));
}

#[test]
fn mark_properties_are_loss_only_for_native_chains_and_never_emitted_as_unvalidated_xml() {
    let marked = source(&equation("1mm"), Some(r#"<w:pStyle w:val="Equation"/><w:rPr><w:b/></w:rPr>"#), "");
    let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[marked.clone()]).unwrap();
    let NativeParagraphResolution::Materialized(loss) = registry.resolve(&marked).unwrap() else { panic!() };
    assert_eq!(loss.code, "paragraph-mark-properties-not-registered");assert!(registry.style_xml().is_empty());
    let direct = source("", Some("<w:rPr><w:b/></w:rPr>"), "");let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[direct.clone()]).unwrap();
    assert_eq!(registry.resolve(&direct).unwrap(), NativeParagraphResolution::NotApplicable);
}

#[test]
fn malformed_unknown_dangling_cyclic_or_foreign_sources_do_not_fallback() {
    for styles in [String::new(), style("Equation", Some("Missing"), Some(""), false), style("Equation", Some("Equation"), Some(""), false),
        format!("{}{}", style("Equation", None, Some(""), false), style("Equation", None, Some(""), false)),
        r#"<w:style w:type="paragraph" w:styleId="Equation"><w:basedOn xmlns:f="urn:foreign" f:val="Base"/></w:style>"#.into()] {
        assert!(capture(&tree(&styles, Some(r#"<w:pStyle w:val="Equation"/>"#), "")).is_err());
    }
    for ppr in [r#"<w:ind w:left="1px"/>"#, r#"<w:ind w:left="1mm"/><w:ind/>"#, r#"<w:jc w:val="left"/><w:ind/>"#, r#"<w:bidi/><w:ind w:left="1mm"/>"#] {
        assert!(capture(&tree(&style("Equation", None, Some(ppr), false), Some(r#"<w:pStyle w:val="Equation"/>"#), "")).is_err());
    }
    assert!(NativeParagraphRegistry::prepare("<w:styles xmlns:w=\"urn:foreign\"/>", &[]).is_err());
}

#[test]
fn capture_rejects_forged_xml_names_or_attribute_characters_before_projection() {
    let original = tree(&equation("1mm"), Some(r#"<w:pStyle w:val="Equation"/>"#), "");
    for invalid in ['\u{1}', '\u{fffe}', '\u{ffff}'] {
        let mut root = original.clone();
        let XmlPart::Element(styles) = &mut root.content[0] else { panic!() };
        let XmlPart::Element(style) = &mut styles.content[0] else { panic!() };
        style.attributes.insert("w:styleId".into(), format!("Base{invalid}"));
        assert_eq!(capture(&root), Err("math_invalid_property"));
    }
    let mut root = original;
    let XmlPart::Element(styles) = &mut root.content[0] else { panic!() };
    let XmlPart::Element(style) = &mut styles.content[0] else { panic!() };
    style.name = "w:style/><w:forged".into();
    assert_eq!(capture(&root), Err("math_invalid_property"));
}

#[test]
fn budgets_are_bounded_by_native_descriptions_and_chain_depth() {
    let a = source(&equation("1mm"), Some(r#"<w:pStyle w:val="Equation"/>"#), "");
    assert_eq!(a.description_bytes(), a.retained_bytes + a.key.len() + a.chain.iter().map(|definition| definition.key.len()).sum::<usize>());
    let many = vec![a.clone(); MAX_SOURCES + 1];assert!(matches!(NativeParagraphRegistry::prepare(TEMPLATE, &many), Err("math_budget")));
    let padding = "0".repeat(15000);let large = source(&equation(&format!("{padding}1mm")), Some(r#"<w:pStyle w:val="Equation"/>"#), "");
    let count = MAX_BYTES / large.description_bytes() + 1;assert!(count <= MAX_SOURCES);
    assert!(matches!(NativeParagraphRegistry::prepare(TEMPLATE, &vec![large; count]), Err("math_budget")));
    for count in [128, 129] {
        let mut styles = String::new();for index in 0..count { styles.push_str(&style(&format!("P{index}"), (index > 0).then(|| format!("P{}", index - 1)).as_deref(), Some(""), false)); }
        let source = tree(&styles, Some(&format!("<w:pStyle w:val=\"P{}\"/>", count - 1)), "");
        assert_eq!(capture(&source).is_ok(), count == 128);
    }
}

#[test]
fn retained_comments_and_unregistered_mark_subtrees_are_charged_without_formula_runs() {
    let ordinary = tree(&equation("1mm"), Some(r#"<w:pStyle w:val="Equation"/>"#), "");
    let original = capture(&ordinary).unwrap().unwrap();
    let padding = "x".repeat(16000);
    for direct in [format!(r#"<w:pStyle w:val="Equation"/><!--{padding}-->"#),
        format!(r#"<w:pStyle w:val="Equation"/><w:rPr><w:b/><!--{padding}--></w:rPr>"#)] {
        let value = source(&equation("1mm"), Some(&direct), "");
        assert!(value.description_bytes() >= original.description_bytes() + padding.len());
    }
    let with_long_run = parse_xml(&ordinary.xml().replace(">x</m:t>", &format!(">{padding}</m:t>"))).unwrap();
    assert_eq!(capture(&with_long_run).unwrap().unwrap().description_bytes(), original.description_bytes());
}

#[test]
fn both_real_template_ids_and_distinct_prepared_definitions_have_explicit_caps() {
    let mut template = format!("<w:styles xmlns:w=\"{WORD_NS}\">");
    for index in 0..=MAX_DEFINITIONS {
        template.push_str(&format!("<w:style w:type=\"paragraph\" w:styleId=\"T{index}\"/>"));
    }
    template.push_str("</w:styles>");
    assert!(matches!(NativeParagraphRegistry::prepare(&template, &[]), Err("math_budget")));

    // A two-definition first chain plus 4095 independent definitions reaches
    // the definition limit before the separate 4096-source/16 MiB limits.
    let mut sources = vec![source(&equation("1mm"), Some(r#"<w:pStyle w:val="Equation"/>"#), "")];
    for index in 1..MAX_SOURCES {
        let id = format!("S{index}");
        sources.push(source(&style(&id, None, None, false), Some(&format!("<w:pStyle w:val=\"{id}\"/>")), ""));
    }
    assert!(sources.iter().map(NativeParagraphSource::description_bytes).sum::<usize>() < MAX_BYTES);
    assert!(matches!(NativeParagraphRegistry::prepare(TEMPLATE, &sources), Err("math_budget")));
}
