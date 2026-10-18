//! Synthetic independent semantic/geometry and transactional export controls.
use super::*;
use crate::math_document::{self, MathPatch, XmlNode};

fn spaces(node: &XmlNode) -> Vec<&XmlNode> {
    let mut out = if is_spacing(node) {
        vec![node]
    } else {
        Vec::new()
    };
    for child in node.children() {
        out.extend(spaces(child));
    }
    out
}
fn imported(source: &str) -> math_document::MathDocument {
    math_document::parse(source, "mathml", false).unwrap()
}
fn formula(doc: &math_document::MathDocument, size: f64) -> crate::math_layout::Formula {
    crate::math_layout::layout_source("", Some(&doc.root.xml()), false, size).unwrap()
}
fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-8, "{actual} != {expected}");
}
fn ink_group(formula: &crate::math_layout::Formula) -> XmlNode {
    let svg = math_document::parse_xml(&formula.interactive_svg()).unwrap();
    let path = svg.children()
        .next()
        .unwrap()
        .children()
        .find(|node| node.attributes.get("data-math-ink").map(String::as_str) == Some("true"))
        .unwrap()
        .clone();
    path
}
fn raw_x_height() -> f64 {
    // Read the independent OpenType OS/2 sxHeight field directly, not the
    // renderer's constants or the ttf-parser getter used by production.
    let font = include_bytes!("../../../assets/fonts/STIXTwoMath-Regular.ttf");
    let u16_at = |at: usize| u16::from_be_bytes(font[at..at + 2].try_into().unwrap());
    let u32_at = |at: usize| u32::from_be_bytes(font[at..at + 4].try_into().unwrap()) as usize;
    let table = |tag: &[u8]| {
        (0..usize::from(u16_at(4)))
            .find_map(|index| {
                let at = 12 + index * 16;
                (&font[at..at + 4] == tag).then(|| u32_at(at + 8))
            })
            .unwrap()
    };
    let head = table(b"head");
    let os2 = table(b"OS/2");
    assert!(u16_at(os2) >= 2);
    let x = i16::from_be_bytes(font[os2 + 86..os2 + 88].try_into().unwrap());
    assert!(x > 0);
    f64::from(x) / f64::from(u16_at(head + 18))
}

#[test]
fn mathml_space_import_dsl_mathml_roundtrip_preserves_units_omission_and_fingerprint() {
    let source = r#"<math><mi>x</mi><mspace width=" 1.25EM " height="2pt" depth="0px"/><mi>y</mi><mspace/></math>"#;
    let doc = imported(source);
    assert!(doc.diagnostics.is_empty());
    assert_eq!(doc.source, source);
    let original = math_document::xml_semantics(&doc.root);
    let dsl = crate::math_migration::readable(&doc.root.xml()).unwrap();
    assert!(dsl.contains("space(width: \" 1.25EM \", height: \"2pt\", depth: \"0px\")"));
    assert!(dsl.contains("space()"));
    let rebuilt = math_document::parse(&dsl, "dsl", false).unwrap();
    assert_eq!(math_document::xml_semantics(&rebuilt.root), original);
    let mml = crate::omml::to_mathml(&crate::omml::parse(&rebuilt.root.xml()).unwrap(), false, "");
    assert!(mml.contains("width=\" 1.25EM \""));
    assert!(mml.contains("depth=\"0px\""));
    let again = imported(&mml);
    assert_eq!(spaces(&again.root).len(), 2);
    assert_eq!(
        Spacing::from_xml(spaces(&again.root)[0]).unwrap(),
        Spacing::from_xml(spaces(&doc.root)[0]).unwrap()
    );
    assert!(!doc.root.xml().contains(SOURCE_ID));
    assert!(!doc.root.xml().contains("<m:t> </m:t>"));
}

#[test]
fn mathml_space_zero_ink_boxes_have_real_physical_and_font_relative_dimensions() {
    for size in [12., 24.] {
        for (width, expected) in [
            ("16px", 12.),
            ("1em", size),
            ("2ex", 2. * size * raw_x_height()),
            ("1pc", 12.),
        ] {
            let doc = imported(&format!(
                "<math><mspace width='{width}' height='18pt' depth='4px'/></math>"
            ));
            let f = formula(&doc, size);
            near(f.width, expected);
            near(f.height, 18.);
            near(f.depth, 3.);
            assert!(ink_group(&f).children().next().is_none());
            assert!(!f.svg("#000000").contains("<path"));
            assert!(!f.interactive_svg().contains("<text"));
        }
    }
}

#[test]
fn mathml_space_scripts_and_explicit_mathsize_scale_relative_units_once() {
    let script=imported("<math><mstyle scriptlevel='+1'><mspace width='1em' height='2ex' depth='0.5em'/></mstyle></math>");
    let scaled = formula(&script, 12.);
    let oracle: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/math-font-geometry-stix.json"
    ))
    .unwrap();
    let percent = oracle["constants"]["ScriptPercentScaleDown"]
        .as_f64()
        .unwrap()
        / 100.;
    near(scaled.width, 12. * percent);
    near(scaled.height, 24. * percent * raw_x_height());
    near(scaled.depth, 6. * percent);
    let physical=imported("<math><mstyle scriptlevel='+1'><mspace width='1em' height='2ex' depth='4px' mathsize='18pt'/></mstyle></math>");
    for size in [12., 24.] {
        let f = formula(&physical, size);
        near(f.width, 18.);
        near(f.height, 36. * raw_x_height());
        near(f.depth, 3.);
    }
}

#[test]
fn mathml_space_resolved_physical_and_real_font_budgets_are_not_clamped() {
    for source in [
        "<math><mspace width='300in'/></math>",
        "<math><mspace height='200in' depth='100in'/></math>",
        "<math><mspace width='1em' mathsize='300in'/></math>",
    ] {
        assert_eq!(
            math_document::parse(source, "mathml", false).unwrap_err(),
            "math_budget"
        );
    }
    let doc = imported("<math><mspace width='50em'/></math>");
    let xml = doc.root.xml();
    crate::math_layout::spacing_budget(&xml, false, 12.).unwrap();
    near(formula(&doc, 12.).width, 600.);
    assert_eq!(
        crate::math_layout::spacing_budget(&xml, false, 512.).unwrap_err(),
        "math_budget"
    );
    let json = serde_json::json!({"omml":xml,"display":false});
    assert_eq!(
        require_vector_for_spacing_at_size(&json, 512.).unwrap_err(),
        "math_budget"
    );
    let tall = Spacing::from_attributes([("height", "25em"), ("depth", "25em")]).unwrap();
    assert_eq!(
        tall.dimensions(512., raw_x_height()).unwrap_err(),
        "math_budget"
    );
    let fixed = imported("<math><mspace width='50em' mathsize='18pt'/></math>");
    crate::math_layout::spacing_budget(&fixed.root.xml(), false, 512.).unwrap();
    near(formula(&fixed, 512.).width, 900.);
    let script = imported("<math><mstyle scriptlevel='+1'><mspace width='50em'/></mstyle></math>");
    crate::math_layout::spacing_budget(&script.root.xml(), false, 512.).unwrap();
}

#[test]
fn mathml_space_preserves_neighbor_operator_spacing_and_does_not_change_ink_height() {
    let base = imported("<math><mi>x</mi><mo>+</mo><mi>y</mi></math>");
    let padded = imported(
        "<math><mi>x</mi><mspace width='12pt'/><mo>+</mo><mspace width='0px'/><mi>y</mi></math>",
    );
    for size in [12., 24.] {
        let a = formula(&base, size);
        let b = formula(&padded, size);
        near(b.width - a.width, 12.);
        near(a.height, b.height);
        near(a.depth, b.depth);
    }
    let plain = imported("<math><mi>x</mi></math>");
    let tall = imported("<math><mi>x</mi><mspace height='30pt' depth='10pt'/></math>");
    let a = formula(&plain, 12.);
    let b = formula(&tall, 12.);
    near(a.width, b.width);
    near(b.height, 30.);
    near(b.depth, 10.);
    assert_eq!(ink_group(&a).xml(), ink_group(&b).xml());
}

#[test]
fn mathml_space_ranges_are_exact_utf16_and_follow_reordered_root_slots() {
    let source="<math><!-- 😀 --><mi>𝑎</mi><mroot><mspace width='1em'/><mspace width=\"2pt\" depth='0px'></mspace></mroot></math>";
    let doc = imported(source);
    assert_eq!(doc.nodes.len(), 2);
    for raw in [
        "<mspace width='1em'/>",
        "<mspace width=\"2pt\" depth='0px'></mspace>",
    ] {
        let byte = source.find(raw).unwrap();
        let from = source[..byte].encode_utf16().count();
        let to = from + raw.encode_utf16().count();
        let node = doc
            .nodes
            .iter()
            .find(|node| node.from == from && node.to == to)
            .unwrap();
        assert_eq!(node.kind, "mspace");
        assert_eq!(node.tex, raw);
        assert!(spaces(&doc.root).iter().any(|space| space.id == node.id));
        for slot in &node.slots {
            let a = crate::document_transaction::byte_offset(source, slot.from).unwrap();
            let b = crate::document_transaction::byte_offset(source, slot.to).unwrap();
            let expected = match slot.role.as_str() {
                "width" => {
                    if raw.contains("1em") {
                        "1em"
                    } else {
                        "2pt"
                    }
                }
                "depth" => "0px",
                _ => panic!("Unexpected dimension slot"),
            };
            assert_eq!(&source[a..b], expected);
        }
    }
    assert_eq!(doc.source, source);
    assert!(!doc.root.xml().contains(SOURCE_ID));
}

#[test]
fn mathml_space_dsl_attribute_patch_preserves_other_dimensions_and_original_source() {
    let source =
        "// 😀 author comment\nrow(a, space(width: \"1em\", height: \"2pt\", depth: \"0px\"), b)";
    let doc = math_document::parse(source, "dsl", false).unwrap();
    let id = doc
        .nodes
        .iter()
        .find(|node| node.kind == "mspace")
        .unwrap()
        .id
        .clone();
    let patched = math_document::patch(
        source,
        "dsl",
        false,
        &doc.revision,
        &MathPatch::SetAttribute {
            node_id: id,
            name: "width".into(),
            value: Some("16px".into()),
        },
    )
    .unwrap();
    assert_eq!(doc.source, source);
    assert!(patched.source.starts_with("// 😀 author comment\n"));
    let space = Spacing::from_xml(spaces(&patched.root)[0]).unwrap();
    assert_eq!(space.width.unwrap().source, "16px");
    assert_eq!(space.height.unwrap().source, "2pt");
    assert_eq!(space.depth.unwrap().source, "0px");
    let invalid = math_document::patch(
        &patched.source,
        "dsl",
        false,
        &patched.revision,
        &MathPatch::SetAttribute {
            node_id: spaces(&patched.root)[0].id.clone(),
            name: "width".into(),
            value: Some("-1em".into()),
        },
    )
    .unwrap_err();
    assert_eq!(invalid, "math_unsupported_extension");
    assert!(patched.source.contains("16px"));
}

#[test]
fn mathml_space_mathml_patch_changes_only_the_owned_element_and_keeps_dialect() {
    let prefix = "<math><!-- 😀 author --><mi>𝑎</mi>";
    let suffix = "<!-- kept --><mi>y</mi></math>";
    let source = format!("{prefix}<mspace width=' 1EM ' height='2pt' depth='0px'/>{suffix}");
    let doc = imported(&source);
    let id = doc.nodes[0].id.clone();
    let changed = math_document::patch(
        &source,
        "mathml",
        false,
        &doc.revision,
        &MathPatch::SetAttribute {
            node_id: id,
            name: "width".into(),
            value: Some("16px".into()),
        },
    )
    .unwrap();
    assert_eq!(changed.dialect, "mathml");
    assert!(changed.source.starts_with(prefix));
    assert!(changed.source.ends_with(suffix));
    let spacing = Spacing::from_xml(spaces(&changed.root)[0]).unwrap();
    assert_eq!(spacing.width.unwrap().source, "16px");
    assert_eq!(spacing.height.unwrap().source, "2pt");
    assert_eq!(spacing.depth.unwrap().source, "0px");
    assert_eq!(doc.source, source);
    assert_eq!(changed.nodes.len(), 1);
}

#[test]
fn mathml_space_generated_placeholder_marker_is_not_inferred_from_author_dimensions() {
    let authored=imported("<math><mtable><mtr><mtd><mspace width='0.45em' height='0.65em' depth='0.05em'/></mtd></mtr></mtable></math>");
    assert!(authored.diagnostics.is_empty());
    assert_eq!(spaces(&authored.root).len(), 1);
    assert!(!authored.root.xml().contains("plcHide"));
    let producer = "<m:oMath><m:m><m:mr><m:e/></m:mr></m:m></m:oMath>";
    let mml = crate::omml::to_mathml(&crate::omml::parse(producer).unwrap(), true, "");
    assert!(mml.contains("data-readmd-matrix-placeholder=\"true\""));
    let recovered = imported(&mml);
    assert!(spaces(&recovered.root).is_empty());
    assert!(recovered.diagnostics.is_empty());
    for corrupt in [
        mml.replace("height=\"0.65em\"", "height=\"0.9em\""),
        mml.replace(
            "data-readmd-matrix-placeholder=\"true\"",
            "data-readmd-matrix-placeholder=\"other\"",
        ),
        mml.replace("<mspace ", "<mspace linebreak=\"newline\" "),
        mml.replace("<mrow>", "<mrow onclick=\"bad\">"),
        mml.replace("<mrow>", "<mrow><!-- authored -->"),
    ] {
        assert_eq!(
            math_document::parse(&corrupt, "mathml", true).unwrap_err(),
            "math_unsupported_extension"
        );
    }
}

#[test]
fn mathml_space_office_and_tex_exports_reject_before_replacing_owned_outputs() {
    let doc = imported("<math><mi>x</mi><mspace width='1em' height='2pt'/><mi>y</mi></math>");
    let xml = doc.root.xml();
    assert_eq!(
        native_office(&doc.root).unwrap_err(),
        "math_unsupported_extension"
    );
    assert_eq!(
        crate::omml::docx_math_fragment(&xml, true).unwrap_err(),
        "math_unsupported_extension"
    );
    let dsl = crate::math_migration::readable(&xml).unwrap();
    let markdown = format!("# Synthetic space\n\n```readmd-math v1\n{dsl}\n```\n");
    let parsed = crate::md_ast::parse(&markdown);
    assert!(document_has_spacing(&parsed).unwrap());
    let package = crate::docx_writer::build(&parsed, &serde_json::json!({}), "Space", &|_| None);
    assert!(package.parts.is_empty());
    assert_eq!(package.warns[0], "math_unsupported_extension");
    let dir = tempfile::tempdir().unwrap();
    for format in ["docx", "tex"] {
        let target = dir.path().join(format!("existing.{format}"));
        std::fs::write(&target, b"existing owned output").unwrap();
        let error = if format == "docx" {
            crate::mdexport::export_docx(
                &markdown,
                "",
                target.to_str().unwrap(),
                &serde_json::json!({}),
                "Space",
            )
        } else {
            crate::mdexport::export_tex(
                &markdown,
                "",
                target.to_str().unwrap(),
                &serde_json::json!({}),
                "Space",
            )
        }
        .unwrap_err();
        assert_eq!(error, "math_unsupported_extension");
        assert_eq!(std::fs::read(&target).unwrap(), b"existing owned output");
    }
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        2,
        "No font/assets side effects on a rejected projection"
    );
}

#[test]
fn mathml_space_pdf_refuses_text_fallback_and_protects_existing_output() {
    let doc = imported("<math><mspace width='1em'/><mi>\u{10ffff}</mi></math>");
    let dsl = crate::math_migration::readable(&doc.root.xml()).unwrap();
    let markdown = format!("```readmd-math v1\n{dsl}\n```\n");
    let parsed = crate::md_ast::parse(&markdown);
    let render = crate::md_ast::to_render_json(&parsed.blocks);
    assert_eq!(
        require_vector_for_spacing(&render).unwrap_err(),
        "math_unsupported_extension"
    );
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("owned.pdf");
    std::fs::write(&target, b"existing owned PDF").unwrap();
    let error = crate::mdexport::export_pdf(
        &markdown,
        "",
        target.to_str().unwrap(),
        &serde_json::json!({}),
        "Space",
        std::path::Path::new("assets"),
    )
    .unwrap_err();
    assert_eq!(error, "math_unsupported_extension");
    assert_eq!(std::fs::read(&target).unwrap(), b"existing owned PDF");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    let supported = imported("<math><mspace width='1em'/><mi>x</mi></math>");
    require_vector_for_spacing(&serde_json::json!({"omml":supported.root.xml(),"display":false}))
        .unwrap();
}

#[test]
fn mathml_space_foreign_xml_and_invalid_dimensions_never_become_character_spaces() {
    for (value, code) in [
        ("-1em", "math_unsupported_extension"),
        ("50%", "math_unsupported_extension"),
        ("2rem", "math_unsupported_extension"),
        ("NaNpt", "math_invalid_property"),
        ("2garbage", "math_invalid_property"),
    ] {
        let source = format!("<math><mspace width='{value}'/></math>");
        assert_eq!(
            math_document::parse(&source, "mathml", false).unwrap_err(),
            code
        );
        let dsl = format!("space(width: \"{value}\")");
        assert_eq!(math_document::parse(&dsl, "dsl", false).unwrap_err(), code);
    }
    for source in [
        "<math><mspace xmlns='urn:fake' width='1em'/></math>",
        "<math><mspace width='1em' onclick='bad'/></math>",
        "<math><mspace readmdId='authored' width='1em'/></math>",
    ] {
        assert!(math_document::parse(source, "mathml", false).is_err());
    }
    for (value, code) in [
        ("nonsense", "math_invalid_property"),
        ("0pt", "math_unsupported_extension"),
        ("50%", "math_unsupported_extension"),
    ] {
        let inherited =
            format!("<math><mstyle mathsize='{value}'><mspace width='1em'/></mstyle></math>");
        assert_eq!(
            math_document::parse(&inherited, "mathml", false).unwrap_err(),
            code
        );
    }
    for dsl in [
        "space(\"1em\")",
        "space(width: 1)",
        "space(unknown: \"1em\")",
        "office.mspace(width: \"1em\")",
    ] {
        assert!(math_document::parse(dsl, "dsl", false).is_err());
    }
}

#[test]
fn mathml_space_pure_office_documents_keep_original_semantics_and_native_projection() {
    let source="<m:oMath><m:f><m:num><m:r><m:t>x</m:t></m:r></m:num><m:den><m:r><m:t>y</m:t></m:r></m:den></m:f></m:oMath>";
    let doc = math_document::parse(source, "omml", false).unwrap();
    let fingerprint = math_document::xml_semantics(&doc.root);
    assert!(!contains_spacing(&doc.root));
    assert!(native_office(&doc.root).is_ok());
    let dsl = crate::math_migration::readable(source).unwrap();
    let rebuilt = math_document::parse(&dsl, "dsl", false).unwrap();
    assert_eq!(fingerprint, math_document::xml_semantics(&rebuilt.root));
    assert!(crate::omml::docx_math_fragment(source, false).is_ok());
    assert_eq!(doc.source, source);
}
