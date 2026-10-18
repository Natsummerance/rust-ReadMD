use super::*;
use crate::math_document::{self, XmlNode};
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!("../tests/fixtures/math-font-geometry-stix.json")).unwrap()
}
fn advance(oracle: &Value, c: &str) -> f64 {
    let name = oracle["characters"][c].as_str().unwrap();
    oracle["glyphs"][name]["advance"].as_f64().unwrap() / oracle["unitsPerEm"].as_f64().unwrap()
}
fn source(maximum: bool, flush: bool, points: Option<u16>) -> String {
    let word = points.map(|value| format!("<w:rPr><w:sz w:val=\"{}\"/></w:rPr>", value * 2)).unwrap_or_default();
    let run = |text: &str| format!("<m:r><m:rPr><m:sty m:val=\"p\"/></m:rPr>{word}<m:t>{}</m:t></m:r>", esc(text));
    format!("<m:oMath xmlns:m=\"{}\" xmlns:w=\"{}\"><m:eqArr><m:eqArrPr><m:maxDist m:val=\"{}\"/><m:objDist m:val=\"{}\"/></m:eqArrPr><m:e>{}</m:e><m:e>{}</m:e></m:eqArr></m:oMath>", math_document::MATH_NS, math_document::WORD_NS, u8::from(maximum), u8::from(flush), run("a&b&x&y"), run("x&y&b&a"))
}
fn find<'a>(root: &'a XmlNode, name: &str) -> &'a XmlNode {
    if root.local() == name { return root; }
    root.children().find_map(|node| {
        if node.local() == name { Some(node) } else {
            fn descend<'a>(node: &'a XmlNode, name: &str) -> Option<&'a XmlNode> {
                if node.local() == name { return Some(node); }
                node.children().find_map(|child| descend(child, name))
            }
            descend(node, name)
        }
    }).unwrap()
}
fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-9, "actual={actual}, independent expected={expected}");
}
fn pt(node: &XmlNode, attribute: &str) -> f64 {
    node.attributes[attribute].strip_suffix("pt").unwrap().parse().unwrap()
}
fn independent(oracle: &Value, size: f64, available: f64, maximum: bool, flush: bool) -> ([f64; 2], [f64; 2], f64) {
    let a = advance(oracle, "a") * size;
    let b = advance(oracle, "b") * size;
    let x = advance(oracle, "x") * size;
    let y = advance(oracle, "y") * size;
    let left = [a.max(x), x.max(b)];
    let widths = [left[0] + b.max(y), left[1] + y.max(a)];
    let natural = widths[0] + widths[1];
    let allocated = if maximum { available.max(natural) } else { natural };
    let gap = (allocated - natural) / if flush { 1.0 } else { 3.0 };
    let start = if flush { 0.0 } else { gap };
    ([start, start + widths[0] + gap], [start + left[0], start + widths[0] + gap + left[1]], allocated)
}

#[test]
fn mathml_equation_array_finite_four_flags_and_sizes_match_independent_font_oracle() {
    let oracle = fixture();
    for size in [12.0, 24.0] {
        for width in [20.0, 24.084 * size / 12.0, 96.0] {
            for maximum in [false, true] { for flush in [false, true] {
                let xml = source(maximum, flush, None);
                let parsed = parse(&xml).unwrap();
                let output = to_mathml_constrained(&parsed, true, &xml, size, Some(width)).unwrap();
                let profile = output.profile.as_ref().expect("Supported finite run profile");
                let (origins, anchors, allocated) = independent(&oracle, size, width, maximum, flush);
                near(profile.allocated_points, allocated);
                near(profile.natural_points, 2.007 * size);
                assert_eq!(profile.overflow, width + 1e-9 < profile.natural_points);
                assert!(!output.warnings.iter().any(|code| code == "math_equation_array_mathml_distribution"));
                assert_eq!(output.warnings.iter().any(|code| code == "math_equation_array_overflow"), profile.overflow);
                let tree = math_document::parse_xml(&output.output).unwrap();
                let table = find(&tree, "mtable");
                assert_eq!(table.attributes["align"], "axis");
                let rows = table.children().collect::<Vec<_>>();
                assert_eq!(rows.len(), 2);
                for row in rows {
                    let cells = row.children().collect::<Vec<_>>();
                    assert_eq!(cells.len(), 7, "Two semantic pairs and three independent gap cells");
                    let mut x = 0.0;
                    for cell in cells {
                        if cell.attributes.contains_key("data-readmd-equation-gap") {
                            x += pt(cell.children().next().unwrap(), "width");
                        } else {
                            let group = cell.attributes["data-readmd-equation-column"].parse::<usize>().unwrap();
                            let box_node = cell.children().next().unwrap();
                            if cell.attributes["data-readmd-equation-side"] == "before" { near(x, origins[group]); }
                            else { near(x, anchors[group]); }
                            x += pt(box_node, "width");
                        }
                    }
                    near(x, allocated);
                }
                assert!(!output.output.contains("display:grid") && !output.output.contains("display:contents"));
            }}
        }
    }
}

#[test]
fn mathml_equation_array_finite_office_physical_run_size_does_not_scale_twice() {
    let xml = source(true, false, Some(18));
    let parsed = parse(&xml).unwrap();
    let a = to_mathml_constrained(&parsed, true, "", 12.0, Some(96.0)).unwrap();
    let b = to_mathml_constrained(&parsed, true, "", 24.0, Some(96.0)).unwrap();
    near(a.profile.as_ref().unwrap().natural_points, 2.007 * 18.0);
    near(b.profile.as_ref().unwrap().natural_points, 2.007 * 18.0);
    for output in [&a.output, &b.output] {
        assert!(output.contains("mathsize=\"18pt\""));
        let tree = math_document::parse_xml(output).unwrap();
        let first_row = find(&tree, "mtable").children().next().unwrap();
        near(pt(first_row.children().next().unwrap().children().next().unwrap(), "width"), (96.0 - 2.007 * 18.0) / 3.0);
    }
}

#[test]
fn mathml_equation_array_unbounded_and_unsupported_keep_compatibility_output_and_warning() {
    let xml = source(true, true, None);
    let parsed = parse(&xml).unwrap();
    let unbounded = to_mathml_constrained(&parsed, true, "", 12.0, None).unwrap();
    assert_eq!(unbounded.output, to_mathml(&parsed, true, ""));
    assert!(unbounded.profile.is_none());
    assert!(unbounded.warnings.iter().any(|v| v == "math_equation_array_mathml_distribution"));
    let inline = to_mathml_constrained(&parsed, false, "", 12.0, Some(96.0)).unwrap();
    assert!(inline.profile.is_none());
    assert_eq!(inline.output, to_mathml(&parsed, false, ""));
    let replacements = [
        ("a&amp;b&amp;x&amp;y", "a&amp;b&amp;x"), // Unaligned spanning final group.
        ("</m:t></m:r>", "</m:t></m:r><m:f><m:num/><m:den/></m:f>"),
        ("<m:sty m:val=\"p\"/>", "<m:sty m:val=\"b\"/>"),
    ];
    for (from, to) in replacements {
        let xml = xml.replacen(from, to, 1);
        let parsed = parse(&xml).unwrap();
        let output = to_mathml_constrained(&parsed, true, "", 12.0, Some(96.0)).unwrap();
        assert!(output.profile.is_none(), "Unsupported shape/style must not receive a finite profile");
        assert!(output.warnings.iter().any(|v| v == "math_equation_array_mathml_distribution"));
        assert_eq!(output.output, to_mathml(&parsed, true, ""));
    }
    let nested = xml.replace("<m:eqArr>", "<m:f><m:num><m:eqArr>").replace("</m:eqArr>", "</m:eqArr></m:num><m:den/></m:f>");
    assert!(to_mathml_constrained(&parse(&nested).unwrap(), true, "", 12.0, Some(96.0)).unwrap().profile.is_none());
}

#[test]
fn mathml_equation_array_projection_keeps_source_revision_ids_and_does_not_infer_reverse_source() {
    let xml = source(true, false, None);
    let doc = math_document::parse(&xml, "omml", true).unwrap();
    let before = serde_json::to_value(&doc).unwrap();
    let output = math_document::preview_constrained(&doc, true, 24.0, Some(120.0)).unwrap();
    assert!(output.profile.is_some());
    assert!(output.output.contains("rd-formula-token-omml-"));
    assert!(!output.output.contains("annotation") && !output.output.contains("&amp;"));
    assert_eq!(serde_json::to_value(&doc).unwrap(), before);
    assert_eq!(crate::math_formats::import_mathml(&output.output).err().unwrap(), "math_unsupported_extension");
    let unknown = output.output.replace("eqarr-core-runs-finite-v1", "unknown-finite-v999");
    assert_eq!(crate::math_formats::import_mathml(&unknown).err().unwrap(), "math_unsupported_extension");
}

#[test]
fn mathml_equation_array_projection_rejects_bad_geometry_and_real_physical_budget() {
    let xml = source(true, false, None);
    let parsed = parse(&xml).unwrap();
    for size in [0.0, 3.0, 513.0, f64::NAN, f64::INFINITY] {
        assert_eq!(to_mathml_constrained(&parsed, true, "", size, Some(96.0)).err(), Some("math_invalid_property"));
    }
    for width in [0.0, -1.0, 20001.0, f64::NAN, f64::INFINITY] {
        assert_eq!(to_mathml_constrained(&parsed, true, "", 12.0, Some(width)).err(), Some("math_invalid_property"));
    }
    let huge = xml.replace("a&amp;b&amp;x&amp;y", &format!("{}&amp;b&amp;x&amp;y", "a".repeat(100)));
    assert_eq!(to_mathml_constrained(&parse(&huge).unwrap(), true, "", 512.0, Some(96.0)).err(), Some("math_budget"));
}
