//! Source/revision contracts used by the direct spacing property controls.
use super::*;
use crate::math_document::{self, MathPatch, XmlNode};

fn authored_spaces(node: &XmlNode) -> Vec<&XmlNode> {
    let mut result = if is_spacing(node) {
        vec![node]
    } else {
        Vec::new()
    };
    for child in node.children() {
        result.extend(authored_spaces(child));
    }
    result
}
fn property(
    doc: &math_document::MathDocument,
    index: usize,
    name: &str,
    value: Option<&str>,
) -> Result<math_document::MathDocument> {
    let node = authored_spaces(&doc.root)[index];
    math_document::patch(
        &doc.source,
        &doc.dialect,
        false,
        &doc.revision,
        &MathPatch::SetAttribute {
            node_id: node.id.clone(),
            name: name.into(),
            value: value.map(str::to_owned),
        },
    )
}

#[test]
fn mathml_space_property_control_targets_only_the_second_nested_dsl_occurrence() {
    let repeated = r#"frac(row(a, space(width: " 1.25EM ", height: "2Pt", depth: "0pX"), b), c)"#;
    let source = format!("// 😀 keep this source comment\nrow({repeated},\n    {repeated})");
    let doc = math_document::parse(&source, "dsl", false).unwrap();
    let target = doc
        .nodes
        .iter()
        .filter(|node| node.kind == "mspace")
        .nth(1)
        .unwrap();
    let from = crate::document_transaction::byte_offset(&source, target.from).unwrap();
    let to = crate::document_transaction::byte_offset(&source, target.to).unwrap();
    let before = math_document::xml_semantics(authored_spaces(&doc.root)[0]);
    let changed = property(&doc, 1, "width", Some(" 16PX ")).unwrap();
    assert_eq!(doc.source, source);
    assert!(changed.source.starts_with(&source[..from]));
    assert!(changed.source.ends_with(&source[to..]));
    assert_eq!(
        math_document::xml_semantics(authored_spaces(&changed.root)[0]),
        before
    );
    let second = Spacing::from_xml(authored_spaces(&changed.root)[1]).unwrap();
    assert_eq!(second.width.unwrap().source, " 16PX ");
    assert_eq!(second.height.unwrap().source, "2Pt");
    assert_eq!(second.depth.unwrap().source, "0pX");
    assert_ne!(changed.revision, doc.revision);
}

#[test]
fn mathml_space_property_control_omission_and_explicit_zero_are_distinct() {
    for name in ["width", "height", "depth"] {
        let initial = math_document::parse(
            r#"row(a, space(width: "1EM", height: "2Pt", depth: "3pX"), b)"#,
            "dsl",
            false,
        )
        .unwrap();
        let absent = property(&initial, 0, name, None).unwrap();
        assert!(!authored_spaces(&absent.root)[0]
            .attributes
            .contains_key(name));
        for zero in ["0", "0PX", "0.000pT"] {
            let explicit = property(&absent, 0, name, Some(zero)).unwrap();
            assert_eq!(
                authored_spaces(&explicit.root)[0]
                    .attributes
                    .get(name)
                    .unwrap(),
                zero
            );
            let readable = crate::math_migration::readable(&explicit.root.xml()).unwrap();
            let roundtrip = math_document::parse(&readable, "dsl", false).unwrap();
            assert_eq!(
                math_document::xml_semantics(&explicit.root),
                math_document::xml_semantics(&roundtrip.root)
            );
            let removed = property(&explicit, 0, name, None).unwrap();
            assert_eq!(
                math_document::xml_semantics(&removed.root),
                math_document::xml_semantics(&absent.root)
            );
        }
    }
}

#[test]
fn mathml_space_property_control_invalid_and_stale_changes_keep_the_last_draft() {
    let original = math_document::parse(r#"row(a, space(width: "1em"), b)"#, "dsl", false).unwrap();
    let current = property(&original, 0, "height", Some("2PT")).unwrap();
    let fingerprint = math_document::xml_semantics(&current.root);
    for (value, code) in [
        ("-1em", "math_unsupported_extension"),
        ("50%", "math_unsupported_extension"),
        ("nonsense", "math_invalid_property"),
        (" ", "math_invalid_property"),
        ("4096in", "math_budget"),
    ] {
        assert_eq!(
            property(&current, 0, "width", Some(value)).unwrap_err(),
            code
        );
        assert_eq!(math_document::xml_semantics(&current.root), fingerprint);
    }
    let stale = math_document::patch(
        &current.source,
        "dsl",
        false,
        &original.revision,
        &MathPatch::SetAttribute {
            node_id: authored_spaces(&current.root)[0].id.clone(),
            name: "depth".into(),
            value: Some("0".into()),
        },
    );
    assert_eq!(stale.unwrap_err(), "transaction_conflict");
    let corrected = property(&current, 0, "width", Some(".50Em")).unwrap();
    assert_eq!(
        Spacing::from_xml(authored_spaces(&corrected.root)[0])
            .unwrap()
            .width
            .unwrap()
            .source,
        ".50Em"
    );
    assert_eq!(
        authored_spaces(&corrected.root)[0]
            .attributes
            .get("height")
            .unwrap(),
        "2PT"
    );
}

#[test]
fn mathml_space_property_control_mathml_uses_exact_utf16_element_boundaries() {
    let prefix = "<math><!-- 😀 UTF16 prefix --><mi>𝑎</mi><mspace width='1EM'/>";
    let selected = "<mspace width='1EM' height='2Pt' depth='0PX'/>";
    let suffix = "<!-- preserve this suffix --><mi>y</mi></math>";
    let source = format!("{prefix}{selected}{suffix}");
    let original = math_document::parse(&source, "mathml", false).unwrap();
    let target = original
        .nodes
        .iter()
        .filter(|n| n.kind == "mspace")
        .nth(1)
        .unwrap();
    assert_eq!(target.from, prefix.encode_utf16().count());
    assert_eq!(target.to - target.from, selected.encode_utf16().count());
    let changed = property(&original, 1, "height", None).unwrap();
    assert_eq!(changed.dialect, "mathml");
    assert!(changed.source.starts_with(prefix) && changed.source.ends_with(suffix));
    let second = authored_spaces(&changed.root)[1];
    assert!(!second.attributes.contains_key("height"));
    assert_eq!(second.attributes.get("width").unwrap(), "1EM");
    assert_eq!(second.attributes.get("depth").unwrap(), "0PX");
    assert_eq!(original.source, source);
}
