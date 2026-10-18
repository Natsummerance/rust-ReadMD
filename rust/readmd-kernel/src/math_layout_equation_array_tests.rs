//! Hand-calculated coordinates and the existing independent FontTools oracle.
//! No expectation is generated from a rendered formula or Word screenshot.
use super::*;
use serde_json::Value;

fn near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-8,
        "actual={actual}, expected={expected}"
    );
}
fn run(text: &str) -> String {
    format!("<m:r><m:rPr><m:sty m:val=\"p\"/></m:rPr><m:t>{text}</m:t></m:r>")
}
fn array(properties: &str, rows: &[String], id: &str) -> String {
    format!("<m:eqArr readmdId=\"rd-formula-token-omml-{id}\"><m:eqArrPr>{properties}</m:eqArrPr>{}</m:eqArr>",rows.iter().map(|r|format!("<m:e>{r}</m:e>")).collect::<String>())
}
fn source(body: &str) -> String {
    format!("<m:oMath>{body}</m:oMath>")
}
fn basic(properties: &str) -> String {
    array(
        properties,
        &[run("a&amp;b&amp;x&amp;y"), run("x&amp;y&amp;b&amp;a")],
        "array",
    )
}
fn glyphs(f: &Formula) -> Vec<(u16, f64, f64, f64)> {
    f.items
        .iter()
        .filter_map(|i| {
            if let Item::Glyph {
                gid, x, y, scale, ..
            } = *i
            {
                Some((gid, x * f.size, y * f.size, scale * f.size))
            } else {
                None
            }
        })
        .collect()
}
fn oracle() -> Value {
    serde_json::from_str(include_str!(
        "../tests/fixtures/math-font-geometry-stix.json"
    ))
    .unwrap()
}
fn metric(data: &Value, ch: &str, key: &str) -> f64 {
    data["glyphs"][data["characters"][ch].as_str().unwrap()][key]
        .as_f64()
        .unwrap()
}

#[test]
fn equation_array_vector_four_combinations_match_independent_font_coordinates_at_multiple_sizes() {
    use sha2::{Digest, Sha256};
    let data = oracle();
    let font = fonts(false).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&*font.fonts[0].data)),
        data["fontSha256"].as_str().unwrap()
    );
    let units = data["unitsPerEm"].as_f64().unwrap();
    let (a, b, x, y) = (
        metric(&data, "a", "advance") / units,
        metric(&data, "b", "advance") / units,
        metric(&data, "x", "advance") / units,
        metric(&data, "y", "advance") / units,
    );
    let columns = [a.max(x) + b.max(y), x.max(b) + y.max(a)];
    let natural = columns.iter().sum::<f64>();
    for size in [12., 24.] {
        for maximum in [false, true] {
            for flush in [false, true] {
                let props = format!(
                    "<m:maxDist m:val=\"{}\"/><m:objDist m:val=\"{}\"/>",
                    u8::from(maximum),
                    u8::from(flush)
                );
                let f = layout_source_constrained(
                    "",
                    Some(&source(&basic(&props))),
                    true,
                    size,
                    Some(96.),
                )
                .unwrap();
                assert!(f.equation_array_diagnostics().is_empty());
                let expected_width = if maximum { 96. } else { natural * size };
                near(f.width, expected_width);
                let extra = expected_width - natural * size;
                let outer = if flush { 0. } else { extra / 3. };
                let gap = if flush { extra } else { extra / 3. };
                let anchors = [
                    outer + a.max(x) * size,
                    outer + columns[0] * size + gap + x.max(b) * size,
                ];
                let g = glyphs(&f);
                assert_eq!(g.len(), 8);
                // Actual b/y/a paths start exactly at the shared anchors on both rows.
                for index in [1, 5] {
                    near(g[index].1, anchors[0]);
                }
                for index in [3, 7] {
                    near(g[index].1, anchors[1]);
                }
                assert!(g.iter().all(|g| (g.3 - size).abs() < 1e-9));
                assert_eq!(
                    g.iter().map(|g| g.0).collect::<Vec<_>>(),
                    ["a", "b", "x", "y", "x", "y", "b", "a"]
                        .iter()
                        .map(|c| metric(&data, c, "gid") as u16)
                        .collect::<Vec<_>>()
                );
            }
        }
    }
}

#[test]
fn equation_array_word_physical_size_and_relayout_do_not_double_scale() {
    let body=basic("<m:maxDist/><m:objDist/>").replace("<m:r>","<m:r><w:rPr xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:sz w:val=\"36\"/></w:rPr>");
    let xml = source(&body);
    let a = layout_source_constrained("", Some(&xml), true, 12., Some(96.)).unwrap();
    let b = a.at_size(24.).unwrap();
    near(a.width, 96.);
    near(b.width, 96.);
    let ag = glyphs(&a);
    let bg = glyphs(&b);
    assert_eq!(ag.len(), 8);
    assert_eq!(ag.len(), bg.len());
    for (a, b) in ag.iter().zip(bg) {
        near(a.1, b.1);
        near(a.2, b.2);
        near(a.3, 18.);
        near(b.3, 18.);
    }
}

#[test]
fn equation_array_actual_operator_edges_align_without_losing_original_binary_spacing() {
    let first = format!("{}{}{}{}", run("x"), run("&amp;"), run("+"), run("y"));
    let second = format!("{}{}{}", run("&amp;"), run("+"), run("y"));
    let f = layout_source(
        "",
        Some(&source(&array("", &[first, second], "operators"))),
        true,
        12.,
    )
    .unwrap();
    let g = glyphs(&f);
    assert_eq!(g.len(), 5);
    near(g[1].1, g[3].1);
    let data = oracle();
    let advance = metric(&data, "x", "advance") / data["unitsPerEm"].as_f64().unwrap() * 12.;
    near(g[1].1 - g[0].1 - advance, 12. * 4. / 18.);
    assert!(f.equation_array_diagnostics().is_empty());
}

#[test]
fn equation_array_run_fragments_preserve_original_utf16_owners_and_source_bytes() {
    let original = source(&array(
        "",
        &[
            "<m:r readmdId=\"rd-formula-token-omml-original\"><m:t>𝑎&amp;b&amp;x&amp;y</m:t></m:r>"
                .into(),
        ],
        "source",
    ));
    let f = layout_source("", Some(&original), true, 12.).unwrap();
    assert_eq!(f.source_xml.as_deref(), Some(original.as_str()));
    let regions = f
        .regions
        .iter()
        .filter(|r| r.kind == "run-text")
        .collect::<Vec<_>>();
    assert!(regions
        .iter()
        .any(|r| r.id == "rd-formula-token-omml-original-text-3" && r.text_range == Some((3, 4))));
    assert!(regions
        .iter()
        .any(|r| r.id == "rd-formula-token-omml-original-text-5" && r.text_range == Some((5, 6))));
    assert!(regions.iter().all(|r| !r.id.contains("-text-0-text-")));
    assert_eq!(f.items.len(), 4);
}

#[test]
fn equation_array_nested_structures_reserve_real_overhead_and_never_take_the_page_twice() {
    let equation = basic("<m:maxDist/><m:objDist/>");
    for body in [
        format!(
            "<m:f><m:num>{equation}</m:num><m:den>{}</m:den></m:f>",
            run("1")
        ),
        format!("<m:rad><m:radPr><m:degHide/></m:radPr><m:e>{equation}</m:e></m:rad>"),
        format!("<m:d><m:e>{equation}</m:e></m:d>"),
        format!("<m:borderBox><m:e>{equation}</m:e></m:borderBox>"),
        format!(
            "<m:func><m:fName>{}</m:fName><m:e>{equation}</m:e></m:func>",
            run("sin")
        ),
        format!(
            "<m:m><m:mr><m:e>{equation}</m:e><m:e>{}</m:e></m:mr></m:m>",
            run("1")
        ),
        format!(
            "<m:sSup><m:e>{equation}</m:e><m:sup>{}</m:sup></m:sSup>",
            run("2")
        ),
        format!("{equation}{equation}"),
    ] {
        let f = layout_source_constrained("", Some(&source(&body)), true, 12., Some(96.)).unwrap();
        assert!(
            f.equation_array_diagnostics().is_empty(),
            "{body}: {:?}",
            f.equation_array_diagnostics()
        );
        near(f.width, 96.);
        assert!(
            f.ink_bounds().unwrap().2 * 12. <= 96. + 0.3,
            "{body}: {:?}",
            f.ink_bounds()
        );
    }
}

#[test]
fn equation_array_inner_distribution_uses_its_column_not_outer_spacers() {
    let inner = basic("<m:maxDist/><m:objDist/>").replace("omml-array", "omml-inner");
    let first = format!("{inner}{}", run("&amp;a&amp;x&amp;y"));
    let second = run("123123&amp;b&amp;x&amp;y");
    let outer = array("<m:maxDist/><m:objDist/>", &[first, second], "outer");
    let f = layout_source_constrained("", Some(&source(&outer)), true, 12., Some(120.)).unwrap();
    near(f.width, 120.);
    let inner = f
        .regions
        .iter()
        .find(|r| r.id == "rd-formula-token-omml-inner")
        .unwrap();
    let data = oracle();
    let expected = 6. * metric(&data, "1", "advance") / data["unitsPerEm"].as_f64().unwrap() * 12.;
    near(inner.w * 12., expected);
    assert!(inner.w * 12. < 60.);
    assert!(f.equation_array_diagnostics().is_empty());
}

#[test]
fn equation_array_narrow_capacity_reports_source_and_keeps_unclipped_geometry() {
    let xml = source(&basic("<m:maxDist/><m:objDist/>"));
    let f = layout_source_constrained("", Some(&xml), true, 12., Some(10.)).unwrap();
    let issue = f
        .equation_array_diagnostics()
        .iter()
        .find(|d| d.code == "math_equation_array_overflow")
        .unwrap();
    assert_eq!(
        issue.node_id.as_deref(),
        Some("rd-formula-token-omml-array")
    );
    near(issue.required_points.unwrap(), 24.084);
    near(issue.available_points.unwrap(), 10.);
    near(f.width, 24.084);
    assert_eq!(glyphs(&f).len(), 8);
    assert_eq!(f.source_xml.as_deref(), Some(xml.as_str()));
    let (left, _, right, _) = f.ink_bounds().unwrap();
    assert!(right > left);
    assert!(f.interactive_svg().contains("viewBox="));
}

#[test]
fn equation_array_run_and_operator_box_alignment_share_mathematical_paragraph_scope() {
    for anchor in [
        format!("<m:r><m:rPr><m:sty m:val=\"p\"/><m:aln/></m:rPr><m:t>-x</m:t></m:r>"),
        "<m:box><m:boxPr><m:opEmu/><m:aln/></m:boxPr><m:e><m:r><m:t>==</m:t></m:r></m:e></m:box>"
            .into(),
    ] {
        let xml = format!(
            "<m:oMathPara><m:oMath>{}{anchor}</m:oMath><m:oMath>{}{anchor}</m:oMath></m:oMathPara>",
            run("a"),
            run("123")
        );
        let f = layout_source_constrained("", Some(&xml), true, 12., Some(120.)).unwrap();
        let points = f
            .regions
            .iter()
            .filter(|r| r.kind == "paragraph-explicit-anchor")
            .collect::<Vec<_>>();
        assert_eq!(points.len(), 2);
        near(points[0].x, points[1].x);
        assert!(points[0].y > points[1].y);
        assert!(f.equation_array_diagnostics().is_empty());
    }
}

#[test]
fn equation_array_ambiguous_source_is_reported_and_original_rendering_is_retained() {
    let xml = source(&array(
        "<m:maxDist/>",
        &["<m:r><m:rPr><m:nor/></m:rPr><m:t>A&amp;B</m:t></m:r>".into()],
        "literal",
    ));
    let f = layout_source_constrained("", Some(&xml), true, 12., Some(120.)).unwrap();
    assert!(f
        .equation_array_diagnostics()
        .iter()
        .any(|d| d.code == "math_equation_array_literal_alignment"));
    assert_eq!(glyphs(&f).len(), 3);
    assert_eq!(f.source_xml.as_deref(), Some(xml.as_str()));
    let doc = crate::math_document::parse(&xml, "omml", true).unwrap();
    let result =
        crate::math_document::vector_preview_with_diagnostics(&doc, true, 12., Some(120.)).unwrap();
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.code == "math_equation_array_literal_alignment"));
    assert!(result.svg.contains("data-math-ink"));
}

#[test]
fn equation_array_mathml_uses_real_cells_and_reports_unavailable_container_distribution() {
    let root = omml::parse(&source(&basic("<m:maxDist/><m:objDist/>"))).unwrap();
    let mathml = omml::to_mathml(&root, true, "");
    assert_eq!(mathml.matches("<mtd ").count(), 8);
    assert!(!mathml.contains("&amp;"));
    assert!(!mathml.contains("malignmark"));
    assert!(mathml.contains("text-align:right"));
    assert!(mathml.contains("text-align:left"));
    assert!(
        omml::mathml_warnings(&root).contains(&"math_equation_array_mathml_distribution".into())
    );
    let plain = omml::parse(&source(&basic(""))).unwrap();
    assert!(
        !omml::mathml_warnings(&plain).contains(&"math_equation_array_mathml_distribution".into())
    );
}
