use super::*;
use crate::math_document::parse_xml;

#[path = "native_run_context_tests.rs"]
mod root_context;

const TEMPLATE: &str = r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:color w:val="262626"/><w:sz w:val="22"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr/></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"/><w:style w:type="paragraph" w:styleId="ReadMDMath"/><w:style w:type="character" w:styleId="CodeChar"/></w:styles>"#;

fn fixture() -> String {
    format!(r#"<w:styles xmlns:w="{WORD_NS}">
<w:style w:type="paragraph" w:styleId="PBase"><w:pPr><w:ind w:left="1mm"/></w:pPr><w:rPr><w:b/><w:i/><w:color w:val="336699"/><w:sz w:val="24"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Equation"><w:basedOn w:val="PBase"/><w:pPr><w:jc w:val="left"/></w:pPr><w:rPr><w:b/><w:i w:val="0"/></w:rPr></w:style>
<w:style w:type="character" w:styleId="CBase"><w:rPr><w:b/><w:i/><w:color w:val="993366"/><w:sz w:val="30"/></w:rPr></w:style>
<w:style w:type="character" w:styleId="Character"><w:basedOn w:val="CBase"/><w:rPr><w:b w:val="false"/></w:rPr></w:style>
</w:styles><w:pPr xmlns:w="{WORD_NS}"><w:pStyle w:val="Equation"/><w:ind w:right="18.125pt"/></w:pPr>
<m:mathPr xmlns:m="{MATH_NS}"><m:dispDef m:val="0"/></m:mathPr><m:oMathPara xmlns:m="{MATH_NS}" xmlns:w="{WORD_NS}"><m:oMath>
<m:r><w:rPr><w:rStyle w:val="Character"/></w:rPr><m:t>x</m:t></m:r>
<m:r><w:rPr><w:rStyle w:val="Character"/><w:b w:val="0"/><w:i w:val="off"/><w:color w:val="445566"/><w:sz w:val="024"/></w:rPr><m:t>x</m:t></m:r>
<m:r><m:t>x</m:t></m:r>
<m:f><m:fPr><m:ctrlPr><w:rPr><w:rStyle w:val="Character"/></w:rPr></m:ctrlPr></m:fPr><m:num><m:r><m:t>x</m:t></m:r></m:num><m:den><m:r><m:t>x</m:t></m:r></m:den></m:f>
</m:oMath></m:oMathPara>"#)
}

fn captured(source: &str) -> NativeParagraphSource { super::super::capture(&parse_xml(source).unwrap()).unwrap().unwrap() }
fn materialized(source: &str) -> String { crate::omml::docx_math_projection_tree(parse_xml(source).unwrap(), true).unwrap().content }
fn legacy(registry: &mut NativeParagraphRegistry, source: &NativeParagraphSource) -> super::super::NativeParagraphProjection {
    let NativeParagraphResolution::Native(value) = registry.resolve(source).unwrap() else { panic!("Expected legacy paragraph view") }; value
}
fn project(source: &str) -> (NativeParagraphRegistry, NativeRunProjection, super::super::NativeParagraphProjection) {
    let input = captured(source); let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[input.clone()]).unwrap();
    let old = legacy(&mut registry, &input);
    let result = registry.project_runs(&input, &materialized(source), Some(&old.selected_style_id)).unwrap();
    (registry, result, old)
}
fn replay(registry: &NativeParagraphRegistry, result: &NativeRunProjection, old: &super::super::NativeParagraphProjection) -> String {
    let styles = TEMPLATE.replace("</w:styles>", &format!("{}</w:styles>", registry.style_xml()));
    format!("{styles}<w:pPr xmlns:w=\"{WORD_NS}\">{}</w:pPr>{}", result.ppr_xml.as_deref().unwrap_or(&old.ppr_xml), result.content)
}
fn word_runs(root: &XmlNode) -> Vec<XmlNode> {
    let mut runs = Vec::new(); visit_runs(root, &mut |node| { runs.push(node.clone()); Ok(()) }).unwrap(); runs
}
fn styles_by_id(xml: &str) -> BTreeMap<String, XmlNode> {
    parse_xml(xml).unwrap().children().map(|node| (attr(node, "styleId").unwrap().to_string(), node.clone())).collect()
}

#[test]
fn native_view_restores_sparse_declarations_and_real_character_inheritance() {
    let source = fixture(); let before = source.clone(); let (registry, output, old) = project(&source);
    assert_eq!((output.native_runs, output.materialized_runs), (6, 0)); assert!(output.loss_reasons.is_empty());
    assert_ne!(output.host_style_id.as_deref(), Some(old.selected_style_id.as_str()));
    let runs = word_runs(&parse_xml(&output.content).unwrap());
    let first = child(&runs[0], "rPr").unwrap();
    assert_eq!(first.children().map(XmlNode::local).collect::<Vec<_>>(), ["rStyle"]);
    assert_eq!(attr(child(first, "rStyle").unwrap(), "val"), Some("Character"));
    let second = child(&runs[1], "rPr").unwrap();
    assert_eq!(attr(child(second, "b").unwrap(), "val"), Some("0"));
    assert_eq!(attr(child(second, "i").unwrap(), "val"), Some("off"));
    assert_eq!(attr(child(second, "sz").unwrap(), "val"), Some("024"));
    assert!(child(&runs[2], "rPr").is_none());
    let roundtrip = captured(&replay(&registry, &output, &old));
    assert_eq!(roundtrip.runs.runs[0].effective, [Axis::Toggle(true), Axis::Toggle(false), Axis::Size(30), Axis::Color([0x99, 0x33, 0x66])]);
    assert_eq!(roundtrip.runs.runs[1].effective, [Axis::Toggle(false), Axis::Toggle(false), Axis::Size(24), Axis::Color([0x44, 0x55, 0x66])]);
    assert_eq!(roundtrip.runs.runs[2].effective, [Axis::Toggle(false), Axis::Toggle(true), Axis::Size(24), Axis::Color([0x33, 0x66, 0x99])]);
    assert_eq!(roundtrip.runs.runs[3].role, "ctrlPr"); assert_eq!(roundtrip.runs.runs[3].effective, roundtrip.runs.runs[0].effective);
    assert_eq!(source, before);
    let definitions = styles_by_id(&registry.style_xml());
    assert!(definitions.values().any(|style| attr(style, "type") == Some("character")));
    assert!(!registry.style_xml().contains("w:default="));
    // Only the final run-enabled host is referenced by this paragraph.
    assert!(!definitions.contains_key(&old.selected_style_id));
    assert!(registry.legacy_refs.is_empty() && registry.legacy_uses.is_empty());
    assert!(child(&definitions[output.host_style_id.as_ref().unwrap()], "rPr").is_some());
}

#[test]
fn successful_run_host_keeps_shared_legacy_ancestors_of_a_materialized_source() {
    let a = fixture();
    let b = a.replace("Equation", "Fallback").replace("<w:style w:type=\"paragraph\" w:styleId=\"PBase\">",
        "<w:docDefaults><w:rPrDefault><w:rPr><w:color w:val=\"112233\"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type=\"paragraph\" w:styleId=\"PBase\">");
    let sources = [captured(&a), captured(&b)];
    assert_eq!(sources[0].chain[0].key, sources[1].chain[0].key);
    let mut expected = None;
    for order in [[0, 1], [1, 0]] {
        let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &sources).unwrap();
        let mut outputs = BTreeMap::new();
        for index in order {
            let old = legacy(&mut registry, &sources[index]);
            let output = registry.project_runs(&sources[index], &materialized(if index == 0 { &a } else { &b }), Some(&old.selected_style_id)).unwrap();
            outputs.insert(index, (old, output));
        }
        assert_eq!(outputs[&0].1.native_runs, 6);
        assert_eq!(outputs[&1].1.materialized_runs, 6);
        assert!(outputs[&1].1.loss_reasons.contains(&"run-document-defaults-not-registered"));
        let definitions = styles_by_id(&registry.style_xml());
        assert!(!definitions.contains_key(&outputs[&0].0.selected_style_id));
        assert!(definitions.contains_key(&outputs[&1].0.selected_style_id));
        let shared = &sources[1].chain[0].key;
        assert_eq!(registry.legacy_refs.get(shared), Some(&1));
        assert!(child(&definitions[&registry.ids[shared]], "rPr").is_none());
        assert_eq!(registry.legacy_refs.len(), 2);
        assert_eq!(registry.legacy_uses.len(), 1);
        let current = captured(&replay(&registry, &outputs[&1].1, &outputs[&1].0));
        assert_eq!(current.runs.runs.iter().map(|run| &run.effective).collect::<Vec<_>>(), sources[1].runs.runs.iter().map(|run| &run.effective).collect::<Vec<_>>());
        if let Some(old) = &expected { assert_eq!(&registry.style_xml(), old); }
        else { expected = Some(registry.style_xml()); }
    }
}

#[test]
fn paragraph_only_source_retains_its_shared_ancestor_after_another_run_view_succeeds() {
    let xml = |selected: &str, direct: &str| format!(r#"<w:styles xmlns:w="{WORD_NS}"><w:style w:type="paragraph" w:styleId="Common"><w:pPr><w:ind w:left="1mm"/></w:pPr></w:style><w:style w:type="paragraph" w:styleId="Run"><w:basedOn w:val="Common"/></w:style><w:style w:type="paragraph" w:styleId="Plain"><w:basedOn w:val="Common"/></w:style></w:styles><w:pPr xmlns:w="{WORD_NS}"><w:pStyle w:val="{selected}"/></w:pPr><m:mathPr xmlns:m="{MATH_NS}"><m:dispDef m:val="0"/></m:mathPr><m:oMath xmlns:m="{MATH_NS}" xmlns:w="{WORD_NS}"><m:r>{direct}<m:t>x</m:t></m:r></m:oMath>"#);
    let a = xml("Run", "<w:rPr><w:b w:val=\"0\"/><w:i w:val=\"0\"/><w:color w:val=\"336699\"/><w:sz w:val=\"24\"/></w:rPr>");
    let b = xml("Plain", "");
    let sources = [captured(&a), captured(&b)];
    assert!(!sources[1].runs.associated);
    let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &sources).unwrap();
    let plain = legacy(&mut registry, &sources[1]);
    let unchanged = registry.project_runs(&sources[1], &materialized(&b), Some(&plain.selected_style_id)).unwrap();
    assert_eq!((unchanged.native_runs, unchanged.materialized_runs), (0, 0));
    let old = legacy(&mut registry, &sources[0]);
    let output = registry.project_runs(&sources[0], &materialized(&a), Some(&old.selected_style_id)).unwrap();
    assert_eq!(output.native_runs, 1);
    let definitions = styles_by_id(&registry.style_xml());
    assert!(!definitions.contains_key(&old.selected_style_id));
    assert!(definitions.contains_key(&plain.selected_style_id));
    assert_eq!(registry.legacy_refs.len(), 2);
    assert_eq!(registry.legacy_uses.get(&sources[1].key), Some(&1));
    let current = parse_xml(&replay(&registry, &unchanged, &plain)).unwrap();
    let context = crate::math_word::word_paragraph_context(&current).unwrap();
    assert_eq!(context.effective.indent["left"].value.raw(), "1mm");
    assert_eq!(context.effective.indent["left"].origin, crate::math_word::WordParagraphOrigin::Style(registry.ids[&sources[1].chain[0].key].clone()));
}

#[test]
fn repeated_resolves_own_separate_legacy_uses_and_projection_never_underflows_them() {
    let source = fixture(); let input = captured(&source); let before = input.clone();
    let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[input.clone()]).unwrap();
    let first = legacy(&mut registry, &input); let second = legacy(&mut registry, &input);
    assert_eq!(first, second);
    assert_eq!(registry.legacy_uses.get(&input.key), Some(&2));
    assert!(registry.legacy_refs.values().all(|refs| *refs == 2));
    let bytes = materialized(&source);
    let native = registry.project_runs(&input, &bytes, Some(&second.selected_style_id)).unwrap();
    assert_eq!(native.native_runs, 6);
    let definitions = styles_by_id(&registry.style_xml());
    assert!(child(&definitions[&first.selected_style_id], "rPr").is_none());
    assert_eq!(registry.legacy_uses.get(&input.key), Some(&1));
    assert!(registry.legacy_refs.values().all(|refs| *refs == 1));
    assert_eq!(registry.project_runs(&input, &bytes, Some(&first.selected_style_id)).unwrap(), native);
    assert!(registry.legacy_refs.is_empty() && registry.legacy_uses.is_empty());
    let only_native = registry.style_xml();
    assert_eq!(registry.project_runs(&input, &bytes, Some(&first.selected_style_id)).unwrap(), native);
    assert_eq!(registry.style_xml(), only_native);
    // A later standalone resolve is still a real legacy reference. Invalid
    // materialized input must not consume it or alter the emitted catalogue.
    let standalone = legacy(&mut registry, &input);
    let with_legacy = registry.style_xml();
    assert!(styles_by_id(&with_legacy).contains_key(&standalone.selected_style_id));
    assert_eq!(registry.project_runs(&input, &bytes.replace("<m:t>x</m:t>", "<m:t>y</m:t>"), Some(&standalone.selected_style_id)), Err("math_invalid_source"));
    assert_eq!(registry.style_xml(), with_legacy);
    assert_eq!(registry.legacy_uses.get(&input.key), Some(&1));
    assert_eq!(input, before);
}

#[test]
fn current_character_style_changes_and_direct_removal_recompute_real_values() {
    let (registry, output, old) = project(&fixture());
    let current = replay(&registry, &output, &old).replace("w:val=\"993366\"", "w:val=\"112233\"").replace("w:val=\"30\"", "w:val=\"32\"");
    let current = captured(&current);
    assert_eq!(current.runs.runs[0].effective[2..], [Axis::Size(32), Axis::Color([0x11, 0x22, 0x33])]);
    assert_eq!(current.runs.runs[1].effective[2..], [Axis::Size(24), Axis::Color([0x44, 0x55, 0x66])]);
    let raw = replay(&registry, &output, &old);
    let mut root = parse_xml(&raw).unwrap();
    fn clear_second(node: &mut XmlNode, ordinal: &mut usize) {
        if node.namespace == MATH_NS && node.local() == "r" {
            if *ordinal == 1 {
                let pr = node.content.iter_mut().find_map(|part| if let XmlPart::Element(pr) = part { word(pr, "rPr").then_some(pr) } else { None }).unwrap();
                pr.content.retain(|part| !matches!(part, XmlPart::Element(child) if word(child, "b")));
            } *ordinal += 1;
        }
        for part in &mut node.content { if let XmlPart::Element(child) = part { clear_second(child, ordinal); } }
    }
    clear_second(&mut root, &mut 0);
    let current = captured(&root.xml());
    assert_eq!(current.runs.runs[1].effective[0], Axis::Toggle(true));
}

#[test]
fn source_and_template_missing_are_not_equal_to_explicit_false_or_size_color() {
    let source = format!(r#"<w:styles xmlns:w="{WORD_NS}"><w:style w:type="paragraph" w:styleId="P"><w:rPr><w:b w:val="0"/></w:rPr></w:style></w:styles><w:pPr xmlns:w="{WORD_NS}"><w:pStyle w:val="P"/></w:pPr><m:oMath xmlns:m="{MATH_NS}"><m:r><m:t>x</m:t></m:r></m:oMath>"#);
    let input = captured(&source); assert_eq!(input.runs.runs[0].effective, [Axis::Missing, Axis::Missing, Axis::Missing, Axis::Missing]);
    let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[input.clone()]).unwrap(); let old = legacy(&mut registry, &input);
    let bytes = materialized(&source);
    let output = registry.project_runs(&input, &bytes, Some(&old.selected_style_id)).unwrap();
    assert_eq!(output.content, bytes); assert_eq!((output.native_runs, output.materialized_runs), (0, 1));
    assert!(output.loss_reasons.contains(&"incompatible-native-run-context")); assert!(output.ppr_xml.is_none());
    assert!(!registry.style_xml().contains("w:rPr"));
    let template = TEMPLATE.replace("<w:color w:val=\"262626\"/><w:sz w:val=\"22\"/>", "<w:b w:val=\"0\"/>");
    let mut registry = NativeParagraphRegistry::prepare(&template, &[input.clone()]).unwrap(); let old = legacy(&mut registry, &input);
    assert_eq!(registry.project_runs(&input, &bytes, Some(&old.selected_style_id)).unwrap().native_runs, 0);
}

#[test]
fn paragraph_only_sources_do_not_create_a_new_run_loss_or_change_selected_id() {
    let source = format!(r#"<w:styles xmlns:w="{WORD_NS}"><w:style w:type="paragraph" w:styleId="P"><w:pPr><w:ind w:left="1mm"/></w:pPr></w:style></w:styles><w:pPr xmlns:w="{WORD_NS}"><w:pStyle w:val="P"/></w:pPr><m:oMath xmlns:m="{MATH_NS}"><m:r><m:t>x</m:t></m:r></m:oMath>"#);
    let input = captured(&source); assert!(!input.runs.associated);
    let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[input.clone()]).unwrap(); let old = legacy(&mut registry, &input);
    let bytes = materialized(&source); let output = registry.project_runs(&input, &bytes, Some(&old.selected_style_id)).unwrap();
    assert_eq!(old.selected_style_id, "P"); assert_eq!(output.content, bytes);
    assert_eq!((output.native_runs, output.materialized_runs), (0, 0)); assert!(output.loss_reasons.is_empty());
    assert!(output.host_style_id.is_none() && output.ppr_xml.is_none()); assert!(!registry.style_xml().contains("w:rPr"));
}

#[test]
fn empty_character_chain_materializes_without_dangling_references_or_fabricated_values() {
    let source = format!(r#"<w:styles xmlns:w="{WORD_NS}"><w:style w:type="character" w:styleId="Empty"><w:rPr><w:b w:val="0"/></w:rPr></w:style></w:styles><m:oMath xmlns:m="{MATH_NS}" xmlns:w="{WORD_NS}" xmlns:f="urn:foreign"><m:r><w:rPr><w:rStyle w:val="Empty"/><!--keep--><f:keep/></w:rPr><m:t>x</m:t></m:r></m:oMath>"#);
    let input = captured(&source); let bytes = materialized(&source);
    assert!(!bytes.contains("rStyle") && !bytes.contains("w:b")); assert!(bytes.contains("keep") && bytes.contains("f:keep"));
    let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[input.clone()]).unwrap();
    let output = registry.project_runs(&input, &bytes, Some("ReadMDMath")).unwrap();
    assert_eq!(output.native_runs, 0); assert_eq!(output.content, bytes); assert!(registry.style_xml().is_empty());
    assert!(source.contains("rStyle"));
}

#[test]
fn direct_four_axes_without_a_paragraph_chain_use_only_a_proven_template_host() {
    let source = format!(r#"<m:oMath xmlns:m="{MATH_NS}" xmlns:w="{WORD_NS}"><m:r><w:rPr><w:b w:val="off"/><w:i w:val="0"/><w:color w:val="aAbBcC"/><w:sz w:val="+24"/></w:rPr><m:t>x</m:t></m:r></m:oMath>"#);
    let input = captured(&source); assert!(input.chain.is_empty()); let original = input.clone();
    let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[input.clone()]).unwrap();
    let output = registry.project_runs(&input, &materialized(&source), Some("ReadMDMath")).unwrap();
    assert_eq!((output.native_runs, output.materialized_runs), (1, 0)); assert!(output.ppr_xml.is_none());
    assert!(output.content.contains("+24") && output.content.contains("aAbBcC")); assert!(registry.style_xml().is_empty()); assert_eq!(input, original);
    assert_eq!(registry.project_runs(&input, &materialized(&source), Some("Normal")), Err("math_invalid_source"));
}

#[test]
fn incompatible_paragraph_defaults_cannot_be_reenabled_through_a_run_projection() {
    let source = format!(r#"<w:styles xmlns:w="{WORD_NS}"><w:docDefaults><w:pPrDefault><w:pPr><w:ind w:left="1mm"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:styleId="P"><w:pPr><w:jc w:val="left"/></w:pPr></w:style><w:style w:type="character" w:styleId="C"><w:rPr><w:b/><w:color w:val="336699"/><w:sz w:val="24"/></w:rPr></w:style></w:styles><w:pPr xmlns:w="{WORD_NS}"><w:pStyle w:val="P"/></w:pPr><m:oMath xmlns:m="{MATH_NS}" xmlns:w="{WORD_NS}"><m:r><w:rPr><w:rStyle w:val="C"/></w:rPr><m:t>x</m:t></m:r></m:oMath>"#);
    let input = captured(&source); let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[input.clone()]).unwrap();
    assert!(matches!(registry.resolve(&input).unwrap(), NativeParagraphResolution::Materialized(loss) if loss.code == "incompatible-paragraph-doc-defaults"));
    let output = registry.project_runs(&input, &materialized(&source), Some("ReadMDMath")).unwrap();
    assert_eq!(output.native_runs, 1); assert!(output.ppr_xml.is_none());
    assert_eq!(output.host_style_id.as_deref(), Some("ReadMDMath"));
    let definitions = styles_by_id(&registry.style_xml()); assert_eq!(definitions.len(), 1);
    assert_eq!(attr(&definitions["C"], "type"), Some("character"));
    assert!(!registry.style_xml().contains("docDefaults") && !registry.style_xml().contains("1mm"));
}

#[test]
fn unknown_hosts_and_atomic_mismatch_do_not_activate_otherwise_valid_styles() {
    let source = fixture(); let input = captured(&source); let bytes = materialized(&source);
    let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[input.clone()]).unwrap();
    let output = registry.project_runs(&input, &bytes, None).unwrap();
    assert_eq!(output.content, bytes); assert_eq!((output.native_runs, output.materialized_runs), (0, 6));
    assert!(output.loss_reasons.contains(&"unproven-native-run-host")); assert!(registry.style_xml().is_empty());
    let incompatible = source.replace("<w:i/>", "").replace("<w:i w:val=\"0\"/>", "").replace("<w:i w:val=\"off\"/>", "");
    let input = captured(&incompatible); let template = TEMPLATE.replace("<w:color w:val=\"262626\"/>", "<w:i/><w:color w:val=\"262626\"/>");
    let mut registry = NativeParagraphRegistry::prepare(&template, &[input.clone()]).unwrap(); let old = legacy(&mut registry, &input);
    let bytes = materialized(&incompatible); let output = registry.project_runs(&input, &bytes, Some(&old.selected_style_id)).unwrap();
    assert_eq!(output.content, bytes); assert_eq!(output.native_runs, 0); assert!(registry.run_used.is_empty());
    assert!(!registry.style_xml().contains("w:type=\"character\""));
}

#[test]
fn an_unprepared_nested_source_can_only_keep_its_verified_materialized_output() {
    let source = fixture(); let input = captured(&source); let bytes = materialized(&source);
    let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[]).unwrap();
    let output = registry.project_runs(&input, &bytes, None).unwrap();
    assert_eq!(output.content, bytes); assert_eq!((output.native_runs, output.materialized_runs), (0, 6));
    assert!(output.loss_reasons.contains(&"unproven-native-run-host")); assert!(registry.style_xml().is_empty());
    assert_eq!(registry.project_runs(&input, &bytes.replace("<m:t>x</m:t>", "<m:t>y</m:t>"), None), Err("math_invalid_source"));
    assert_eq!(registry.project_runs(&input, &bytes, Some("ReadMDMath")), Err("math_invalid_source"));
}

#[test]
fn typed_collisions_and_same_leaf_with_different_parent_have_order_stable_aliases() {
    let a = fixture().replace("Character", "CodeChar");
    let b = a.replace("993366", "123456").replace("w:val=\"30\"", "w:val=\"32\"");
    let sources = [captured(&a), captured(&b)];
    let mut forward = NativeParagraphRegistry::prepare(TEMPLATE, &sources).unwrap();
    let mut reverse = NativeParagraphRegistry::prepare(TEMPLATE, &[sources[1].clone(), sources[0].clone()]).unwrap();
    let mut results = Vec::new();
    for (registry, order) in [(&mut forward, [0, 1]), (&mut reverse, [1, 0])] {
        let mut outputs = BTreeMap::new();
        for index in order { let old = legacy(registry, &sources[index]); outputs.insert(index, registry.project_runs(&sources[index], &materialized(if index == 0 { &a } else { &b }), Some(&old.selected_style_id)).unwrap()); }
        results.push(outputs);
    }
    assert_eq!(results[0], results[1]); assert_eq!(forward.style_xml(), reverse.style_xml());
    let definitions = styles_by_id(&forward.style_xml());
    assert_eq!(definitions.len(), forward.legacy_refs.len() + forward.run_used.len());
    assert!(!definitions.contains_key("CodeChar"));
    let leaves = definitions.values().filter(|node| attr(node, "type") == Some("character") && child(node, "basedOn").is_some()).collect::<Vec<_>>();
    assert_eq!(leaves.len(), 2); assert_ne!(attr(leaves[0], "styleId"), attr(leaves[1], "styleId"));
    // The namespace of a style ID is shared across paragraph and character.
    let paragraph = fixture().replace("Character", "Equation").replace("w:styleId=\"Equation\"><w:basedOn w:val=\"PBase\"", "w:styleId=\"ParagraphOnly\"><w:basedOn w:val=\"PBase\"").replace("<w:pStyle w:val=\"Equation\"/>", "<w:pStyle w:val=\"ParagraphOnly\"/>");
    let third = captured(&paragraph); let registry = NativeParagraphRegistry::prepare(TEMPLATE, &[sources[0].clone(), third]).unwrap();
    assert_eq!(registry.ids.values().collect::<BTreeSet<_>>().len(), registry.ids.len());
}

#[test]
fn non_four_axes_keep_effective_direct_values_and_math_properties_are_untouched() {
    let source = fixture().replace("<w:rStyle w:val=\"Character\"/>", "<w:rStyle w:val=\"Character\"/><w:lang w:val=\"fr-FR\"/>").replace("<m:t>x</m:t>", "<m:rPr><m:sty m:val=\"p\"/></m:rPr><m:t>x</m:t>");
    let (registry, output, old) = project(&source);
    assert_eq!(output.native_runs, 6); assert!(output.loss_reasons.contains(&"other-run-properties-materialized"));
    let runs = word_runs(&parse_xml(&output.content).unwrap());
    assert_eq!(attr(child(child(&runs[0], "rPr").unwrap(), "lang").unwrap(), "val"), Some("fr-FR"));
    let first = crate::omml::parse(&replay(&registry, &output, &old)).unwrap();
    fn check(node: &crate::omml::Node, count: &mut usize) {
        if node.is_math() && node.name == "r" { let style = crate::omml::run_style(node); assert!(!style.bold && style.upright); *count += 1; }
        for child in &node.children { check(child, count); }
    }
    let mut count = 0; check(&first, &mut count); assert_eq!(count, 5);
}

#[test]
fn auto_theme_size_and_global_defaults_remain_explicit_materialized_boundaries() {
    let source = fixture();
    for (index, changed, reason) in [
        (0, source.replace("993366", "auto"), "nonliteral-native-run-color"),
        (1, source.replace("w:color w:val=\"993366\"", "w:color w:val=\"993366\" w:themeColor=\"accent1\""), "nonliteral-native-run-color"),
        (2, source.replace("w:val=\"30\"", "w:val=\"3277\""), "unsupported-native-run-size"),
        (3, source.replace("<w:style w:type=\"paragraph\" w:styleId=\"PBase\">", "<w:docDefaults><w:rPrDefault><w:rPr><w:b/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type=\"paragraph\" w:styleId=\"PBase\">"), "run-document-defaults-not-registered"),
    ] {
        let input = captured(&changed); let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[input.clone()]).unwrap(); let old = legacy(&mut registry, &input);
        let bytes = materialized(&changed); let output = registry.project_runs(&input, &bytes, Some(&old.selected_style_id)).unwrap();
        assert_eq!(output.content, bytes, "case {index}"); assert_eq!(output.native_runs, 0); assert!(output.loss_reasons.contains(&reason), "{:?}", output.loss_reasons);
        assert!(registry.run_used.is_empty());
    }
    let automatic = source.replace("w:styleId=\"Character\"", "w:styleId=\"Character\" w:default=\"1\"").replace("<w:rStyle w:val=\"Character\"/>", "");
    let (_, output, _) = project(&automatic); assert_eq!(output.native_runs, 0); assert!(output.loss_reasons.contains(&"automatic-character-default-not-registered"));
}

#[test]
fn malformed_character_references_cannot_be_promoted_to_native_or_silent_fallback() {
    for source in [fixture().replace("<w:basedOn w:val=\"CBase\"/>", "<w:basedOn w:val=\"Character\"/>"),
                   fixture().replace("<w:basedOn w:val=\"CBase\"/>", "<w:basedOn w:val=\"Missing\"/>"),
                   fixture().replace("<w:rStyle w:val=\"Character\"/>", "<w:rStyle w:val=\"PBase\"/>"),
                   fixture().replace("<w:rStyle w:val=\"Character\"/>", "<w:rStyle w:val=\"Character\"/><w:rStyle w:val=\"CBase\"/>"),
                   fixture().replace("<w:b w:val=\"false\"/>", "<w:b/><w:b w:val=\"false\"/>")] {
        assert_eq!(super::super::capture(&parse_xml(&source).unwrap()), Err("math_invalid_property"));
    }
}

#[test]
fn no_global_or_template_mutation_is_needed_for_a_successful_view() {
    let source = fixture(); let (registry, output, _) = project(&source); assert!(output.native_runs > 0);
    assert_eq!(registry.template.root.as_ref().unwrap().xml(), WordMathStyles::parse(TEMPLATE).unwrap().root.unwrap().xml());
    let definitions = registry.style_xml();
    assert!(!definitions.contains("docDefaults") && !definitions.contains("theme") && !definitions.contains("w:default="));
    assert!(!styles_by_id(&definitions).contains_key("Normal")); assert!(!styles_by_id(&definitions).contains_key("ReadMDMath"));
}

#[test]
fn identity_proof_rejects_deleted_reordered_changed_or_role_forged_math_sites() {
    let source = fixture(); let input = captured(&source); let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[input.clone()]).unwrap(); let old = legacy(&mut registry, &input);
    let bytes = materialized(&source);
    for changed in [bytes.replace("<m:t>x</m:t>", "<m:t>y</m:t>"),
                    bytes.replace("m:num", "m:den").replace("m:den", "m:num"),
                    bytes.replace("m:ctrlPr", "m:r"), bytes.replacen("<m:t>x</m:t>", "", 1)] {
        assert_eq!(registry.project_runs(&input, &changed, Some(&old.selected_style_id)), Err("math_invalid_source"));
    }
    let other = captured(&source.replace("993366", "113355"));
    assert_eq!(registry.project_runs(&other, &materialized(&source), Some(&old.selected_style_id)), Err("math_invalid_source"));
    assert!(registry.run_used.is_empty());
}

#[test]
fn normalization_can_insert_defaults_but_cannot_rewrite_existing_jc_or_limloc() {
    for declaration in ["<m:jc m:val=\"right\"/>", "<m:jc/>"] {
        let source = fixture().replace("<m:oMath>", &format!("<m:oMathParaPr>{declaration}</m:oMathParaPr><m:oMath>"));
        let input = captured(&source); let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[input.clone()]).unwrap(); let old = legacy(&mut registry, &input);
        let bytes = materialized(&source);
        assert_eq!(registry.project_runs(&input, &bytes, Some(&old.selected_style_id)).unwrap().native_runs, 6);
        let changed = if declaration.contains("right") { bytes.replace("m:val=\"right\"", "m:val=\"left\"") }
            else { bytes.replace("m:val=\"centerGroup\"", "m:val=\"left\"") };
        assert_ne!(changed, bytes);
        assert_eq!(registry.project_runs(&input, &changed, Some(&old.selected_style_id)), Err("math_invalid_source"));
    }
    let nary = "<m:nary><m:naryPr><m:limLoc m:val=\"subSup\"/></m:naryPr><m:sub><m:r><m:t>0</m:t></m:r></m:sub><m:sup><m:r><m:t>1</m:t></m:r></m:sup><m:e><m:r><m:t>x</m:t></m:r></m:e></m:nary>";
    for explicit in [true, false] {
        let nary = if explicit { nary.into() } else { nary.replace("<m:limLoc m:val=\"subSup\"/>", "") };
        let source = fixture().replace("</m:oMath>", &format!("{nary}</m:oMath>"));
        let input = captured(&source); let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[input.clone()]).unwrap(); let old = legacy(&mut registry, &input);
        let bytes = materialized(&source);
        assert_eq!(registry.project_runs(&input, &bytes, Some(&old.selected_style_id)).unwrap().native_runs, 9);
        if explicit {
            let changed = bytes.replace("m:val=\"subSup\"", "m:val=\"undOvr\""); assert_ne!(changed, bytes);
            assert_eq!(registry.project_runs(&input, &changed, Some(&old.selected_style_id)), Err("math_invalid_source"));
        }
    }
}

#[test]
fn omitted_default_style_and_empty_direct_pr_remain_original_source_declarations() {
    let source = fixture().replace("w:styleId=\"Equation\"", "w:styleId=\"Equation\" w:default=\"true\"").replace("<w:pStyle w:val=\"Equation\"/>", "").replace("<m:r><m:t>x</m:t></m:r>", "<m:r><w:rPr/><m:t>x</m:t></m:r>");
    let input = captured(&source); let original = input.clone(); let (registry, output, old) = project(&source);
    assert!(old.selection_was_default); assert!(output.ppr_xml.as_ref().unwrap().contains("pStyle"));
    let runs = word_runs(&parse_xml(&output.content).unwrap()); assert!(child(&runs[2], "rPr").unwrap().children().next().is_none());
    assert_eq!(input, original); assert!(!registry.style_xml().contains("w:default="));
}

#[test]
fn namespace_aliases_and_internal_node_ids_do_not_change_run_definition_identity() {
    let source = fixture(); let aliased = source.replace("w:", "q:").replace("xmlns:w", "xmlns:q");
    let a = captured(&source); let b = captured(&aliased);
    assert_eq!(a.chain.iter().map(|definition| &definition.key).collect::<Vec<_>>(), b.chain.iter().map(|definition| &definition.key).collect::<Vec<_>>());
    assert_eq!(a.characters.iter().map(|definition| &definition.key).collect::<Vec<_>>(), b.characters.iter().map(|definition| &definition.key).collect::<Vec<_>>());
    let mut root = parse_xml(&source).unwrap(); root.id = "different".into(); let c = super::super::capture(&root).unwrap().unwrap();
    assert_eq!(a.runs.source_digest, c.runs.source_digest);
    let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[a, b]).unwrap();
    assert!(registry.style_xml().is_empty()); let old = legacy(&mut registry, &c);
    assert_eq!(registry.project_runs(&c, &materialized(&source), Some(&old.selected_style_id)).unwrap().native_runs, 6);
}

#[test]
fn source_rpr_comments_are_budgeted_and_cancellation_precedes_activation() {
    let input = captured(&fixture()); let mut registry = NativeParagraphRegistry::prepare(TEMPLATE, &[input.clone()]).unwrap(); let old = legacy(&mut registry, &input);
    let bytes = materialized(&fixture());
    crate::speech::with_cancel(std::sync::Arc::new(|| true), || {
        assert_eq!(registry.project_runs(&input, &bytes, Some(&old.selected_style_id)), Err("cancelled"));
        assert_eq!(super::super::capture(&parse_xml(&fixture()).unwrap()), Err("cancelled"));
    });
    assert!(registry.run_used.is_empty());
    let long = fixture().replace("<w:rStyle w:val=\"Character\"/>", &format!("<w:rStyle w:val=\"Character\"/><!--{}-->", "x".repeat(16000)));
    assert!(captured(&long).description_bytes() > input.description_bytes() + 16000);
    assert_eq!(registry.project_runs(&input, &"x".repeat(MAX_BYTES + 1), Some(&old.selected_style_id)), Err("math_budget"));
}
