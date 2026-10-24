use super::*;

const DEFAULTS: &str = r#"<w:rPrDefault><w:rPr><w:rFonts w:ascii="STIX Two Math" w:hAnsi="STIX Two Math" w:eastAsia="Noto Serif CJK" w:cs="STIX Two Math"/><w:b w:val="0"/><w:i w:val="0"/><w:color w:val="262626"/><w:sz w:val="24"/><w:szCs w:val="24"/><w:rtl w:val="0"/><w:lang w:val="en-US" w:eastAsia="zh-CN"/></w:rPr></w:rPrDefault>"#;

fn template(defaults: &str) -> String {
    let original = "<w:rPrDefault><w:rPr><w:color w:val=\"262626\"/><w:sz w:val=\"22\"/></w:rPr></w:rPrDefault>";
    assert!(TEMPLATE.contains(original)); TEMPLATE.replace(original, defaults)
}
fn with_defaults(xml: &str, defaults: &str) -> String {
    xml.replacen(&format!("<w:styles xmlns:w=\"{WORD_NS}\">"), &format!("<w:styles xmlns:w=\"{WORD_NS}\"><w:docDefaults>{defaults}</w:docDefaults>"), 1)
}
fn native_with(template: &str, registry: &NativeParagraphRegistry, output: &NativeRunProjection, old: &super::super::super::NativeParagraphProjection) -> String {
    let styles = template.replace("</w:styles>", &format!("{}</w:styles>", registry.style_xml()));
    format!("{styles}<w:pPr xmlns:w=\"{WORD_NS}\">{}</w:pPr>{}", output.ppr_xml.as_deref().unwrap_or(&old.ppr_xml), output.content)
}
fn assert_effective_equal(a: &NativeParagraphSource, b: &NativeParagraphSource) {
    assert_eq!(a.runs.runs.len(), b.runs.runs.len());
    for (a, b) in a.runs.runs.iter().zip(&b.runs.runs) { assert_eq!(a.role, b.role); assert_eq!(a.effective, b.effective); assert_eq!(a.effective_properties, b.effective_properties); }
}

#[test]
fn explicit_matching_globals_admit_unstyled_runs_under_the_real_template_host() {
    let target = template(DEFAULTS);
    let source = format!(r#"<w:styles xmlns:w="{WORD_NS}"><w:docDefaults>{DEFAULTS}</w:docDefaults></w:styles><m:oMath xmlns:m="{MATH_NS}" xmlns:w="{WORD_NS}"><m:r><m:t>x</m:t></m:r><m:r><w:rPr><w:rFonts w:ascii="STIX Two Math"/></w:rPr><m:t>x</m:t></m:r><m:f><m:fPr><m:ctrlPr/></m:fPr><m:num><m:r><m:t>x</m:t></m:r></m:num><m:den><m:r><m:t>x</m:t></m:r></m:den></m:f></m:oMath>"#);
    let input = captured(&source); let before = input.clone();
    assert!(input.chain.is_empty() && input.characters.is_empty() && input.runs.automatic.is_none());
    let mut registry = NativeParagraphRegistry::prepare(&target, &[input.clone()]).unwrap();
    let output = registry.project_runs(&input, &materialized(&source), Some("ReadMDMath")).unwrap();
    assert_eq!((output.native_runs, output.materialized_runs), (5, 0));
    assert_eq!(output.host_style_id.as_deref(), Some("ReadMDMath"));
    assert!(output.ppr_xml.is_none() && registry.style_xml().is_empty());
    let runs = word_runs(&parse_xml(&output.content).unwrap());
    assert!(child(&runs[0], "rPr").is_none() && child(&runs[2], "rPr").is_none());
    assert_eq!(crate::math_document::xml_semantics(child(&runs[1], "rPr").unwrap()), crate::math_document::xml_semantics(input.runs.runs[1].direct.as_ref().unwrap()));
    assert!(!output.content.contains("rStyle") && !output.content.contains("w:lang") && !output.content.contains("w:szCs"));
    assert_effective_equal(&input, &captured(&format!("{target}{}", output.content))); assert_eq!(input, before);
}

#[test]
fn matching_complete_global_defaults_restore_true_inheritance_without_changing_the_template() {
    let source = with_defaults(&fixture(), DEFAULTS); let input = captured(&source); let before = input.clone();
    let target = template(DEFAULTS); let mut registry = NativeParagraphRegistry::prepare(&target, &[input.clone()]).unwrap();
    assert_eq!(input.runs.global.state, GlobalRunState::Explicit); assert_eq!(input.runs.global, registry.template_run_context);
    let old = legacy(&mut registry, &input); let bytes = materialized(&source);
    assert!(bytes.contains("w:lang") && bytes.contains("w:szCs") && bytes.contains("w:rFonts"));
    let output = registry.project_runs(&input, &bytes, Some(&old.selected_style_id)).unwrap();
    assert_eq!((output.native_runs, output.materialized_runs), (6, 0)); assert!(!output.loss_reasons.contains(&"run-document-defaults-not-registered"));
    for run in word_runs(&parse_xml(&output.content).unwrap()) {
        if let Some(pr) = child(&run, "rPr") { for property in ["lang", "szCs", "rFonts", "rtl"] { assert!(child(pr, property).is_none(), "A source-inherited {property} became direct"); } }
    }
    let roundtrip = captured(&native_with(&target, &registry, &output, &old)); assert_effective_equal(&input, &roundtrip);
    assert!(!registry.style_xml().contains("docDefaults") && !registry.style_xml().contains("default="));
    assert_eq!(registry.template.root.as_ref().unwrap().xml(), WordMathStyles::parse(&target).unwrap().root.unwrap().xml());
    assert_eq!(input, before); assert!(source.contains(DEFAULTS));
}

#[test]
fn matching_globals_keep_authored_direct_nodes_and_partial_font_language_attributes() {
    let direct = "<w:rFonts w:ascii=\"STIX Two Math\"/><w:szCs w:val=\"24\"/><w:lang w:val=\"en-US\"/>";
    let source = with_defaults(&fixture().replacen("<w:rStyle w:val=\"Character\"/>", &format!("<w:rStyle w:val=\"Character\"/>{direct}"), 1), DEFAULTS);
    let input = captured(&source); let target = template(DEFAULTS); let mut registry = NativeParagraphRegistry::prepare(&target, &[input.clone()]).unwrap(); let old = legacy(&mut registry, &input);
    let output = registry.project_runs(&input, &materialized(&source), Some(&old.selected_style_id)).unwrap(); assert_eq!(output.native_runs, 6);
    let runs = word_runs(&parse_xml(&output.content).unwrap()); let pr = child(&runs[0], "rPr").unwrap();
    for property in ["rFonts", "szCs", "lang"] {
        let original = child(input.runs.runs[0].direct.as_ref().unwrap(), property).unwrap();
        assert_eq!(crate::math_document::xml_semantics(child(pr, property).unwrap()), crate::math_document::xml_semantics(original));
    }
    assert_eq!(child(pr, "rFonts").unwrap().attributes.keys().filter(|key| !key.starts_with("xmlns")).count(), 1);
    assert_eq!(child(pr, "lang").unwrap().attributes.keys().filter(|key| !key.starts_with("xmlns")).count(), 1);
    assert_effective_equal(&input, &captured(&native_with(&target, &registry, &output, &old)));
}

#[test]
fn non_four_axis_global_differences_still_materialize_even_when_four_axes_are_identical() {
    for changed in [DEFAULTS.replace("en-US", "fr-FR"), DEFAULTS.replace("Noto Serif CJK", "Other Family"), DEFAULTS.replace("w:szCs w:val=\"24\"", "w:szCs w:val=\"26\""), DEFAULTS.replace("w:rtl w:val=\"0\"", "w:rtl w:val=\"false\"")] {
        let source = with_defaults(&fixture(), &changed); let input = captured(&source); let target = template(DEFAULTS);
        let reference = captured(&with_defaults(&fixture(), DEFAULTS));
        assert_eq!(input.runs.runs.iter().map(|run| &run.effective).collect::<Vec<_>>(), reference.runs.runs.iter().map(|run| &run.effective).collect::<Vec<_>>());
        let mut registry = NativeParagraphRegistry::prepare(&target, &[input.clone()]).unwrap(); let old = legacy(&mut registry, &input); let bytes = materialized(&source);
        let output = registry.project_runs(&input, &bytes, Some(&old.selected_style_id)).unwrap();
        assert_eq!((output.native_runs, output.materialized_runs), (0, 6)); assert_eq!(output.content, bytes);
        assert!(output.loss_reasons.contains(&"run-document-defaults-not-registered")); assert!(registry.run_used.is_empty());
    }
}

#[test]
fn a_fully_direct_shadow_does_not_certify_incompatible_global_source_declarations() {
    let run = "<w:rPr><w:rFonts w:ascii=\"STIX Two Math\"/><w:b w:val=\"0\"/><w:i w:val=\"0\"/><w:color w:val=\"262626\"/><w:sz w:val=\"24\"/><w:szCs w:val=\"24\"/><w:rtl w:val=\"0\"/><w:lang w:val=\"en-US\"/></w:rPr>";
    let source = format!(r#"<w:styles xmlns:w="{WORD_NS}"><w:docDefaults>{}</w:docDefaults></w:styles><m:oMath xmlns:m="{MATH_NS}" xmlns:w="{WORD_NS}"><m:r>{run}<m:t>x</m:t></m:r></m:oMath>"#, DEFAULTS.replace("en-US", "fr-FR"));
    let input = captured(&source); let target = template(DEFAULTS); let mut registry = NativeParagraphRegistry::prepare(&target, &[input.clone()]).unwrap();
    assert_eq!(input.runs.runs[0].effective, [Axis::Toggle(false), Axis::Toggle(false), Axis::Size(24), Axis::Color([0x26; 3])]);
    let output = registry.project_runs(&input, &materialized(&source), Some("ReadMDMath")).unwrap();
    assert_eq!(output.native_runs, 0); assert!(output.loss_reasons.contains(&"run-document-defaults-not-registered")); assert!(registry.style_xml().is_empty());
}

#[test]
fn missing_empty_and_explicit_global_declarations_remain_asymmetric() {
    let target = template(DEFAULTS);
    for (declaration, state) in [(None, GlobalRunState::Missing), (Some("<w:rPrDefault/>"), GlobalRunState::Empty), (Some("<w:rPrDefault><w:rPr/></w:rPrDefault>"), GlobalRunState::Empty)] {
        let source = declaration.map_or_else(fixture, |defaults| with_defaults(&fixture(), defaults)); let input = captured(&source);
        assert_eq!(input.runs.global.state, state); let mut registry = NativeParagraphRegistry::prepare(&target, &[input.clone()]).unwrap(); let old = legacy(&mut registry, &input);
        assert_eq!(registry.project_runs(&input, &materialized(&source), Some(&old.selected_style_id)).unwrap().native_runs, 6);
        assert_eq!(captured(&source), input);
    }
    let missing = captured(&fixture()); let empty = captured(&with_defaults(&fixture(), "<w:rPrDefault/>"));
    assert_ne!(missing.key, empty.key); assert_ne!(missing.runs.global, empty.runs.global);
}

#[test]
fn current_global_edits_reach_only_actual_inheritors_and_two_native_generations_stay_native() {
    let target = template(DEFAULTS); let mut source = with_defaults(&fixture(), DEFAULTS);
    let first = captured(&source);
    for _ in 0..2 {
        let input = captured(&source); let before = input.clone(); let mut registry = NativeParagraphRegistry::prepare(&target, &[input.clone()]).unwrap(); let old = legacy(&mut registry, &input);
        let output = registry.project_runs(&input, &materialized(&source), Some(&old.selected_style_id)).unwrap(); assert_eq!(output.native_runs, 6);
        source = native_with(&target, &registry, &output, &old); assert_eq!(input, before); assert_effective_equal(&first, &captured(&source));
    }
    for (before, after, key) in [("en-US", "fr-FR", "lang"), ("Noto Serif CJK", "Another CJK", "rFonts"), ("w:szCs w:val=\"24\"", "w:szCs w:val=\"28\"", "szCs")] {
        let changed = captured(&source.replace(before, after));
        for (old, new) in first.runs.runs.iter().zip(&changed.runs.runs) { assert_eq!(old.effective, new.effective); assert_ne!(old.effective_properties[key], new.effective_properties[key]); }
    }
}

fn automatic_source(defaults: &str) -> String {
    let styles = format!(r#"<w:styles xmlns:w="{WORD_NS}"><w:docDefaults>{defaults}</w:docDefaults><w:style w:type="character" w:styleId="AutoBase"><w:rPr><w:b/><w:i/><w:color w:val="336699"/><w:sz w:val="30"/><w:lang w:val="fr-FR"/></w:rPr></w:style><w:style w:type="character" w:styleId="Auto" w:default="1"><w:basedOn w:val="AutoBase"/><w:rPr><w:b/><w:i w:val="0"/></w:rPr></w:style></w:styles>"#);
    format!(r#"{styles}<m:oMath xmlns:m="{MATH_NS}" xmlns:w="{WORD_NS}"><m:r><m:t>x</m:t></m:r><m:r><w:rPr><w:b w:val="off"/><w:i w:val="off"/><w:color w:val="445566"/><w:sz w:val="24"/></w:rPr><m:t>x</m:t></m:r></m:oMath>"#)
}
fn automatic_template(defaults: &str) -> String {
    let root = parse_xml(&automatic_source(defaults)).unwrap(); let styles = root.children().find(|node| word(node, "styles")).unwrap();
    template(defaults).replace("</w:styles>", &format!("{}</w:styles>", styles.children().filter(|node| word(node, "style")).map(XmlNode::xml).collect::<String>()))
}

#[test]
fn automatic_character_default_reuses_only_the_existing_actual_complete_chain() {
    let source = automatic_source(DEFAULTS); let input = captured(&source); let before = input.clone(); let target = automatic_template(DEFAULTS);
    assert!(input.runs.runs.iter().all(|run| run.automatic && run.character.is_none())); assert!(input.characters.is_empty());
    let mut registry = NativeParagraphRegistry::prepare(&target, &[input.clone()]).unwrap();
    let output = registry.project_runs(&input, &materialized(&source), Some("ReadMDMath")).unwrap();
    assert_eq!((output.native_runs, output.materialized_runs), (2, 0)); assert!(registry.style_xml().is_empty());
    assert!(!output.content.contains("rStyle") && !output.content.contains("lang"));
    let native = format!("{target}{}", output.content); let imported = captured(&native); assert_effective_equal(&input, &imported);
    assert!(imported.runs.runs.iter().all(|run| run.automatic)); assert_eq!(input, before);
    // A current target default style edit is genuinely inherited, while the
    // second run's explicit color and size remain authored direct overrides.
    let changed = captured(&native.replace("w:val=\"336699\"", "w:val=\"117733\"").replace("w:val=\"30\"", "w:val=\"32\""));
    assert_eq!(changed.runs.runs[0].effective[2..], [Axis::Size(32), Axis::Color([0x11, 0x77, 0x33])]);
    assert_eq!(changed.runs.runs[1].effective[2..], input.runs.runs[1].effective[2..]);
}

#[test]
fn automatic_default_id_ancestry_non_four_properties_and_presence_are_not_current_value_shortcuts() {
    let source = automatic_source(DEFAULTS); let input = captured(&source);
    let actual = automatic_template(DEFAULTS);
    for target in [template(DEFAULTS), actual.replace("AutoBase", "DifferentBase"), actual.replace("fr-FR", "de-DE"), actual.replace("w:default=\"1\"", "w:default=\"true\"")] {
        let mut registry = NativeParagraphRegistry::prepare(&target, &[input.clone()]).unwrap(); let bytes = materialized(&source);
        let output = registry.project_runs(&input, &bytes, Some("ReadMDMath")).unwrap();
        assert_eq!((output.native_runs, output.materialized_runs), (0, 2)); assert_eq!(output.content, bytes);
        assert!(output.loss_reasons.contains(&"automatic-character-default-not-registered")); assert!(registry.style_xml().is_empty());
    }
}

#[test]
fn automatic_defaults_with_missing_empty_globals_cannot_inherit_uncovered_target_properties() {
    for defaults in ["", "<w:rPrDefault><w:rPr/></w:rPrDefault>"] {
        let source = automatic_source(defaults); let input = captured(&source); let target = automatic_template(DEFAULTS);
        assert_ne!(input.runs.global.state, GlobalRunState::Explicit);
        let mut registry = NativeParagraphRegistry::prepare(&target, &[input.clone()]).unwrap(); let bytes = materialized(&source);
        let output = registry.project_runs(&input, &bytes, Some("ReadMDMath")).unwrap();
        assert_eq!((output.native_runs, output.materialized_runs), (0, 2)); assert_eq!(output.content, bytes);
        assert!(output.loss_reasons.contains(&"incompatible-native-run-context"));
        assert!(!output.loss_reasons.contains(&"run-document-defaults-not-registered"));
        assert!(registry.style_xml().is_empty() && registry.run_used.is_empty());
    }
}

#[test]
fn automatic_defaults_can_cover_real_target_globals_without_changing_missing_empty_source_state() {
    let cover = |source: String| source.replacen("<w:rPr><w:b/>", "<w:rPr><w:rFonts w:ascii=\"STIX Two Math\" w:hAnsi=\"STIX Two Math\" w:eastAsia=\"Noto Serif CJK\" w:cs=\"STIX Two Math\"/><w:b/>", 1)
        .replacen("<w:lang w:val=\"fr-FR\"/>", "<w:szCs w:val=\"24\"/><w:rtl w:val=\"0\"/><w:lang w:val=\"fr-FR\" w:eastAsia=\"zh-CN\"/>", 1);
    let target = cover(automatic_template(DEFAULTS));
    for defaults in ["", "<w:rPrDefault><w:rPr/></w:rPrDefault>"] {
        let source = cover(automatic_source(defaults)); let input = captured(&source); let before = input.clone();
        assert_ne!(input.runs.global.state, GlobalRunState::Explicit);
        let mut registry = NativeParagraphRegistry::prepare(&target, &[input.clone()]).unwrap();
        let output = registry.project_runs(&input, &materialized(&source), Some("ReadMDMath")).unwrap();
        assert_eq!((output.native_runs, output.materialized_runs), (2, 0)); assert!(registry.style_xml().is_empty());
        assert!(!output.content.contains("rStyle") && !output.content.contains("w:lang") && !output.content.contains("w:szCs"));
        assert_effective_equal(&input, &captured(&format!("{target}{}", output.content))); assert_eq!(input, before);
    }
}

#[test]
fn unresolved_theme_dependencies_cannot_be_certified_by_equal_fallback_fonts_or_colors() {
    let defaults = DEFAULTS.replace("w:ascii=\"STIX Two Math\"", "w:ascii=\"STIX Two Math\" w:asciiTheme=\"minorHAnsi\"");
    let scheme = format!(r#"<a:fontScheme xmlns:a="{}" name="Owned"><a:minorFont><a:latin typeface="STIX Two Math"/></a:minorFont></a:fontScheme>"#, crate::math_document::DRAWING_NS);
    let source = format!("{scheme}{}", with_defaults(&fixture(), &defaults)); let input = captured(&source); let target = template(&defaults);
    assert!(input.runs.global.theme_dependent); let mut registry = NativeParagraphRegistry::prepare(&target, &[input.clone()]).unwrap(); let old = legacy(&mut registry, &input);
    let output = registry.project_runs(&input, &materialized(&source), Some(&old.selected_style_id)).unwrap();
    assert_eq!(output.native_runs, 0); assert!(output.loss_reasons.contains(&"run-document-defaults-not-registered")); assert!(registry.run_used.is_empty());
    assert!(source.contains("asciiTheme"));
}

#[test]
fn compatible_and_incompatible_global_sources_share_only_legitimately_referenced_views() {
    let target = template(DEFAULTS); let good = with_defaults(&fixture(), DEFAULTS); let bad = with_defaults(&fixture(), &DEFAULTS.replace("en-US", "fr-FR"));
    let sources = [captured(&good), captured(&bad)]; let mut expected = None;
    for order in [[0, 1], [1, 0]] {
        let mut registry = NativeParagraphRegistry::prepare(&target, &sources).unwrap();
        for index in order { let old = legacy(&mut registry, &sources[index]); let output = registry.project_runs(&sources[index], &materialized(if index == 0 { &good } else { &bad }), Some(&old.selected_style_id)).unwrap(); assert_eq!(output.native_runs > 0, index == 0); }
        assert_eq!(registry.legacy_uses.len(), 1); assert_eq!(registry.legacy_refs.len(), 2);
        let emitted = registry.style_xml(); assert!(!emitted.contains("docDefaults") && !emitted.contains("default="));
        if let Some(old) = &expected { assert_eq!(&emitted, old); } else { expected = Some(emitted); }
        assert_eq!(registry.template.root.as_ref().unwrap().xml(), WordMathStyles::parse(&target).unwrap().root.unwrap().xml());
    }
}

#[test]
fn complete_effective_binding_rejects_non_four_axis_materialized_tampering_and_cancelled_capture() {
    let target = template(DEFAULTS); let source = with_defaults(&fixture(), DEFAULTS); let input = captured(&source); let mut registry = NativeParagraphRegistry::prepare(&target, &[input.clone()]).unwrap(); let old = legacy(&mut registry, &input); let bytes = materialized(&source);
    let changed = bytes.replace("en-US", "fr-FR"); assert_ne!(changed, bytes);
    assert_eq!(registry.project_runs(&input, &changed, Some(&old.selected_style_id)), Err("math_invalid_source")); assert!(registry.run_used.is_empty());
    let before = registry.style_xml(); crate::speech::with_cancel(std::sync::Arc::new(|| true), || {
        assert_eq!(registry.project_runs(&input, &bytes, Some(&old.selected_style_id)), Err("cancelled"));
        assert_eq!(super::super::super::capture(&parse_xml(&source).unwrap()), Err("cancelled"));
    }); assert_eq!(registry.style_xml(), before);
}

#[test]
fn full_property_work_budget_is_checked_before_mutating_or_cloning() {
    let root = parse_xml(&format!(r#"<w:rPr xmlns:w="{WORD_NS}"><w:lang w:val="en-US"/><w:szCs w:val="24"/></w:rPr>"#)).unwrap();
    let pr = root.children().next().unwrap(); let mut properties = Properties::new();
    apply(&mut properties, Some(pr), false).unwrap(); let original = properties.clone();
    let mut work = MAX_BYTES;
    assert_eq!(context::clone_properties(&properties, &mut work), Err("math_budget"));
    let mut work = MAX_BYTES;
    assert_eq!(context::apply_properties(&mut properties, Some(pr), false, &mut work), Err("math_budget"));
    assert_eq!(properties, original);
}
