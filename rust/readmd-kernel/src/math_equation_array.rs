//! Source-preserving Office equation-array plans and a font-independent solver.
//!
//! Ampersands alternate alignment/separator within an argument, including across
//! run boundaries. This module neither rewrites author XML nor infers an anchor
//! from an equals sign. Physical width is supplied by the immediate container.

use crate::omml::{self, Node};
use serde::Serialize;

pub(crate) const MAX_MARKERS: usize = 16_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarkerKind {
    Alignment,
    Separator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunMarker {
    pub offset: usize,
    pub kind: MarkerKind,
    pub consumes_character: bool,
}

/// A rendering diagnostic retains source identity and physical point values.
/// Text offsets are within the original Office run, measured in UTF-16.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EquationArrayDiagnostic {
    pub code: &'static str,
    pub node_id: Option<String>,
    pub text_range: Option<(usize, usize)>,
    pub required_points: Option<f64>,
    pub available_points: Option<f64>,
}

impl EquationArrayDiagnostic {
    pub(crate) fn at_node(
        code: &'static str,
        n: &Node,
        text_range: Option<(usize, usize)>,
    ) -> Self {
        Self {
            code,
            node_id: n.attr("readmdId").map(str::to_owned),
            text_range,
            required_points: None,
            available_points: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Flags {
    pub maximum: bool,
    pub flush: bool,
}

impl Flags {
    pub fn read(n: &Node) -> Self {
        Self {
            maximum: n.enabled("eqArrPr", "maxDist", false),
            flush: n.enabled("eqArrPr", "objDist", false),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RowPlan {
    /// An output-only copy with nonpainting markers; its run text is unchanged.
    pub view: Node,
    pub has_markers: bool,
    pub marker_count: usize,
    pub diagnostics: Vec<EquationArrayDiagnostic>,
}

/// Explicit aln is an anchor at a run/complete operator box's leading edge.
pub(crate) fn explicit_alignment(n: &Node) -> bool {
    n.is_math()
        && match n.name.as_str() {
            "r" => n.enabled("rPr", "aln", false),
            "box" => n.enabled("boxPr", "opEmu", false) && n.enabled("boxPr", "aln", false),
            _ => false,
        }
}

/// Scan only the current horizontal scope. Fractions/scripts/inner arrays and
/// complete boxes remain atoms; their children cannot steal an outer anchor.
/// `ampersands=false` is the ordinary mathematical-paragraph aln path.
pub(crate) fn plan_row(row: &Node, ampersands: bool) -> RowPlan {
    struct Scan {
        count: usize,
        raw: usize,
        explicit: usize,
        issues: Vec<EquationArrayDiagnostic>,
    }
    fn walk(n: &mut Node, ampersands: bool, s: &mut Scan) {
        if n.is_props() {
            return;
        }
        n.equation_markers.clear();
        n.equation_word = None;
        n.equation_context = None;
        n.equation_glue = None;
        if explicit_alignment(n) {
            s.explicit += 1;
            s.count += 1;
            n.equation_markers.push(RunMarker {
                offset: 0,
                kind: MarkerKind::Alignment,
                consumes_character: false,
            });
        }
        if n.is_math() && n.name == "r" {
            let text = n.run_text();
            let literal = n.enabled("rPr", "lit", false) || omml::run_style(n).normal_text;
            let mut offset = 0;
            let mut literal_reported = false;
            let mut visible = String::new();
            for c in text.chars() {
                if ampersands && c == '&' {
                    if literal {
                        if !literal_reported {
                            s.issues.push(EquationArrayDiagnostic::at_node(
                                "math_equation_array_literal_alignment",
                                n,
                                Some((offset, offset + 1)),
                            ));
                            literal_reported = true;
                        }
                    } else {
                        s.raw += 1;
                        s.count += 1;
                        if s.count <= MAX_MARKERS {
                            n.equation_markers.push(RunMarker {
                                offset,
                                kind: if s.raw % 2 == 1 {
                                    MarkerKind::Alignment
                                } else {
                                    MarkerKind::Separator
                                },
                                consumes_character: true,
                            });
                        }
                    }
                } else {
                    visible.push(c);
                }
                offset += c.len_utf16();
            }
            n.equation_word = Some(omml::is_word(&visible));
            return;
        }
        if !n.is_math() || matches!(n.name.as_str(), "root" | "oMath" | "oMathPara" | "e") {
            for c in &mut n.children {
                walk(c, ampersands, s);
            }
        }
    }
    let mut view = row.clone();
    let mut scan = Scan {
        count: 0,
        raw: 0,
        explicit: 0,
        issues: Vec::new(),
    };
    walk(&mut view, ampersands, &mut scan);
    if scan.count > MAX_MARKERS {
        scan.issues.push(EquationArrayDiagnostic::at_node(
            "math_equation_array_marker_budget",
            row,
            None,
        ));
    }
    if scan.explicit > 1 || scan.explicit > 0 && scan.raw > 0 {
        scan.issues.push(EquationArrayDiagnostic::at_node(
            "math_equation_array_mixed_alignment",
            row,
            None,
        ));
    }
    if !scan.issues.is_empty() {
        view = row.clone();
    } else {
        // Freeze atom boundary classes before splitting a run at a marker.
        // Continuation fragments inherit lexical style but not a second glue.
        fn freeze(nodes: &mut [Node], context: omml::SpacingContext) {
            let plans = omml::spacing_plans(nodes, context);
            let mut previous = None;
            for (n, plan) in nodes.iter_mut().zip(plans).filter(|(n, _)| !n.is_props()) {
                n.equation_context = Some(plan.context);
                n.equation_glue = Some((previous, plan.first));
                if !n.is_math() || matches!(n.name.as_str(), "root" | "oMath" | "oMathPara" | "e") {
                    freeze(&mut n.children, plan.context);
                }
                if plan.last.is_some() {
                    previous = plan.last;
                }
            }
        }
        freeze(&mut view.children, omml::SpacingContext::default());
    }
    RowPlan {
        view,
        has_markers: scan.count > 0 && scan.issues.is_empty(),
        marker_count: scan.count,
        diagnostics: scan.issues,
    }
}

/// Whether a subtree can consume extra horizontal room in its parent slot.
/// Script/degree/limit branches have independent intrinsic-sized containers.
pub(crate) fn stretches(n: &Node) -> bool {
    if n.is_props() {
        return false;
    }
    if n.is_math() && n.name == "eqArr" {
        if !Flags::read(n).maximum {
            return false;
        }
        let plans = n
            .children
            .iter()
            .filter(|r| r.is_math() && r.name == "e")
            .map(|r| plan_row(r, true))
            .collect::<Vec<_>>();
        return plans.iter().any(|p| p.has_markers)
            && plans.iter().all(|p| p.diagnostics.is_empty())
            && plans.iter().map(|p| p.marker_count).sum::<usize>() <= MAX_MARKERS;
    }
    if matches!(n.name.as_str(), "sub" | "sup" | "deg" | "lim") {
        return false;
    }
    n.children.iter().any(stretches)
}

pub(crate) fn contains_array(n: &Node) -> bool {
    !n.is_props() && ((n.is_math() && n.name == "eqArr") || n.children.iter().any(contains_array))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ColumnMeasure {
    pub width: f64,
    pub anchor: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ColumnPosition {
    pub origin: f64,
    pub width: f64,
    pub anchor: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Solution {
    pub width: f64,
    pub natural_width: f64,
    pub columns: Vec<ColumnPosition>,
    pub overflow: bool,
    pub single_column_distribution: bool,
}

/// Independent algebra. Values can be points or em, but must share one unit.
/// On overflow the natural, unclipped result is retained and explicitly tagged.
pub(crate) fn solve(
    rows: &[Vec<ColumnMeasure>],
    flags: Flags,
    available: Option<f64>,
) -> Result<Solution, &'static str> {
    if available.is_some_and(|v| !v.is_finite() || v < 0.0) {
        return Err("math_equation_array_invalid_geometry");
    }
    let count = rows.iter().map(Vec::len).max().unwrap_or(0);
    if count > MAX_MARKERS + 1 {
        return Err("math_equation_array_marker_budget");
    }
    let mut left = vec![0.0f64; count];
    let mut right = vec![0.0f64; count];
    let mut unaligned = vec![0.0f64; count];
    let mut aligned = false;
    for row in rows {
        for (j, cell) in row.iter().enumerate() {
            if !cell.width.is_finite() || cell.width < 0.0 {
                return Err("math_equation_array_invalid_geometry");
            }
            if let Some(anchor) = cell.anchor {
                if !anchor.is_finite() || anchor < 0.0 || anchor > cell.width + 1e-9 {
                    return Err("math_equation_array_invalid_geometry");
                }
                aligned = true;
                left[j] = left[j].max(anchor);
                right[j] = right[j].max((cell.width - anchor).max(0.0));
            } else {
                unaligned[j] = unaligned[j].max(cell.width);
            }
        }
    }
    let widths: Vec<_> = (0..count)
        .map(|j| (left[j] + right[j]).max(unaligned[j]))
        .collect();
    let natural_width: f64 = widths.iter().sum();
    if !natural_width.is_finite() {
        return Err("math_equation_array_invalid_geometry");
    }
    let overflow = available.is_some_and(|v| natural_width > v + 1e-9);
    let width = if flags.maximum && aligned {
        available.unwrap_or(natural_width).max(natural_width)
    } else {
        natural_width
    };
    let extra = width - natural_width;
    // A single rigid column cannot flush both edges without changing its text.
    // Keep its common anchor, center the group, and expose this policy to callers.
    let single_column_distribution = count == 1 && extra > 0.0;
    let (outer, gap) = if count <= 1 {
        (extra / 2.0, 0.0)
    } else if flags.flush {
        (0.0, extra / (count - 1) as f64)
    } else {
        let v = extra / (count + 1) as f64;
        (v, v)
    };
    let mut x = outer;
    let columns = widths
        .into_iter()
        .enumerate()
        .map(|(j, width)| {
            let column = ColumnPosition {
                origin: x,
                width,
                anchor: x + left[j],
            };
            x += width + gap;
            column
        })
        .collect();
    Ok(Solution {
        width,
        natural_width,
        columns,
        overflow,
        single_column_distribution,
    })
}

/// MathML gets the same semantic cells, without flattening structural atoms.
#[derive(Debug, Clone, Default)]
pub(crate) struct NodeColumn {
    pub before: Vec<Node>,
    pub after: Vec<Node>,
    pub aligned: bool,
}

pub(crate) fn node_columns(row: &RowPlan) -> Vec<NodeColumn> {
    fn fragment(n: &Node, text: String, offset: usize, origin: std::sync::Arc<Node>) -> Node {
        let source = n.attr("readmdId").map(|id| (id.to_owned(), offset));
        Node {
            name: n.name.clone(),
            prefix: n.prefix.clone(),
            namespace: n.namespace.clone(),
            attrs: source
                .as_ref()
                .map(|(id, _)| vec![("readmdId".into(), format!("{id}-text-{offset}"))])
                .unwrap_or_default(),
            children: vec![Node {
                name: "t".into(),
                namespace: n.namespace.clone(),
                text,
                ..Default::default()
            }],
            math_defaults: n.math_defaults,
            mathml_format: n.mathml_format,
            mathml_script_level: n.mathml_script_level,
            equation_source: source,
            equation_origin: Some(origin),
            equation_word: n.equation_word,
            equation_context: n.equation_context,
            equation_glue: n.equation_glue,
            ..Default::default()
        }
    }
    fn glue(n: &Node, origin: std::sync::Arc<Node>) -> Node {
        Node {
            name: "readmdEquationGlue".into(),
            namespace: n.namespace.clone(),
            equation_origin: Some(origin),
            equation_glue: n.equation_glue,
            equation_context: n.equation_context,
            mathml_format: n.mathml_format,
            mathml_script_level: n.mathml_script_level,
            ..Default::default()
        }
    }
    fn push_node(mut n: Node, columns: &mut Vec<NodeColumn>) {
        let column = columns.last_mut().unwrap();
        // Boundary glue belongs before the anchored object's leading edge,
        // including when '&' is a separate run. Align actual operators, not
        // a different amount of invisible leading glue on each row.
        if column.aligned
            && column.after.is_empty()
            && n.equation_glue
                .is_some_and(|(a, b)| omml::spacing_gap(a, b, false) != 0.0)
        {
            let origin = n
                .equation_origin
                .clone()
                .unwrap_or_else(|| std::sync::Arc::new(n.clone()));
            column.before.push(glue(&n, origin));
            n.equation_glue = None;
        }
        if column.aligned {
            column.after.push(n);
        } else {
            column.before.push(n);
        }
    }
    fn marker(kind: MarkerKind, columns: &mut Vec<NodeColumn>) {
        if kind == MarkerKind::Alignment {
            columns.last_mut().unwrap().aligned = true;
        } else {
            columns.push(NodeColumn::default());
        }
    }
    fn walk(n: &Node, columns: &mut Vec<NodeColumn>) {
        if n.is_props() {
            return;
        }
        if n.is_math() && n.name == "r" && !n.equation_markers.is_empty() {
            let origin = std::sync::Arc::new(n.clone());
            let mut markers = n.equation_markers.iter().peekable();
            let (mut offset, mut start, mut text) = (0, 0, String::new());
            let mut pending_glue = n.equation_glue;
            for c in n.run_text().chars() {
                let mut consume = false;
                while markers.peek().is_some_and(|m| m.offset == offset) {
                    let m = markers.next().unwrap();
                    if !text.is_empty() {
                        let mut node =
                            fragment(n, std::mem::take(&mut text), start, origin.clone());
                        node.equation_glue = pending_glue.take();
                        push_node(node, columns);
                    } else if pending_glue
                        .is_some_and(|(a, b)| omml::spacing_gap(a, b, false) != 0.0)
                    {
                        let mut node = glue(n, origin.clone());
                        node.equation_glue = pending_glue.take();
                        push_node(node, columns);
                    }
                    marker(m.kind, columns);
                    consume |= m.consumes_character;
                    start = offset
                        + if m.consumes_character {
                            c.len_utf16()
                        } else {
                            0
                        };
                }
                if !consume {
                    text.push(c);
                }
                offset += c.len_utf16();
            }
            if !text.is_empty() {
                let mut node = fragment(n, text, start, origin.clone());
                node.equation_glue = pending_glue;
                push_node(node, columns);
            }
            for m in markers {
                marker(m.kind, columns);
            }
        } else if !n.is_math() || matches!(n.name.as_str(), "root" | "oMath" | "oMathPara" | "e") {
            for c in &n.children {
                walk(c, columns);
            }
        } else {
            if !n.equation_markers.is_empty()
                && n.equation_glue
                    .is_some_and(|(a, b)| omml::spacing_gap(a, b, false) != 0.0)
            {
                push_node(glue(n, std::sync::Arc::new(n.clone())), columns);
            }
            for m in &n.equation_markers {
                marker(m.kind, columns);
            }
            let mut node = n.clone();
            if !node.equation_markers.is_empty() {
                node.equation_glue = None;
            }
            node.equation_markers.clear();
            push_node(node, columns);
        }
    }
    let mut result = vec![NodeColumn::default()];
    walk(&row.view, &mut result);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(left: f64, right: f64) -> ColumnMeasure {
        ColumnMeasure {
            width: left + right,
            anchor: Some(left),
        }
    }
    fn row(body: &str) -> Node {
        omml::parse(&format!(
            "<m:oMath><m:eqArr><m:e>{body}</m:e></m:eqArr></m:oMath>"
        ))
        .unwrap()
        .child("oMath")
        .unwrap()
        .child("eqArr")
        .unwrap()
        .child("e")
        .unwrap()
        .clone()
    }
    fn text(nodes: &[Node]) -> String {
        nodes.iter().map(Node::run_text).collect()
    }

    #[test]
    fn equation_array_distribution_four_combinations_use_independent_coordinates() {
        let rows = vec![
            vec![cell(10., 12.), cell(8., 6.)],
            vec![cell(18., 4.), cell(5., 14.)],
        ];
        for (maximum, flush, width, origins, anchors) in [
            (false, false, 52., [0., 30.], [18., 38.]),
            (false, true, 52., [0., 30.], [18., 38.]),
            (true, false, 100., [16., 62.], [34., 70.]),
            (true, true, 100., [0., 78.], [18., 86.]),
        ] {
            let s = solve(&rows, Flags { maximum, flush }, Some(100.)).unwrap();
            assert_eq!(s.width, width);
            assert_eq!(s.natural_width, 52.);
            assert!(!s.overflow);
            assert_eq!(
                s.columns.iter().map(|v| v.origin).collect::<Vec<_>>(),
                origins
            );
            assert_eq!(
                s.columns.iter().map(|v| v.anchor).collect::<Vec<_>>(),
                anchors
            );
        }
    }

    #[test]
    fn equation_array_narrow_intrinsic_and_single_column_do_not_shrink_or_divide_by_zero() {
        let rows = vec![vec![cell(18., 12.)]];
        let flags = Flags {
            maximum: true,
            flush: true,
        };
        let narrow = solve(&rows, flags, Some(12.)).unwrap();
        assert_eq!(narrow.width, 30.);
        assert!(narrow.overflow);
        assert_eq!(narrow.columns[0].anchor, 18.);
        let intrinsic = solve(&rows, flags, None).unwrap();
        assert_eq!(intrinsic.width, 30.);
        let wide = solve(&rows, flags, Some(100.)).unwrap();
        assert_eq!(wide.columns[0].origin, 35.);
        assert!(wide.single_column_distribution);
        assert_eq!(wide.columns[0].anchor, 53.);
        assert!(solve(&rows, flags, Some(f64::NAN)).is_err());
        assert!(solve(&[vec![cell(-1., 2.)]], flags, None).is_err());
        assert!(solve(
            &[vec![ColumnMeasure {
                width: 1.,
                anchor: Some(2.)
            }]],
            flags,
            None
        )
        .is_err());
    }

    #[test]
    fn equation_array_markers_cross_runs_preserve_empty_and_trailing_columns() {
        for body in ["<m:r><m:t>a&amp;b&amp;&amp;&amp;x&amp;y&amp;</m:t></m:r>",
            "<m:r><m:t>a&amp;</m:t></m:r><m:r><m:t>b&amp;&amp;</m:t></m:r><m:r><m:t>&amp;x&amp;y&amp;</m:t></m:r>"] {
            let plan = plan_row(&row(body), true); assert!(plan.diagnostics.is_empty());
            let columns = node_columns(&plan); assert_eq!(columns.len(), 4);
            assert_eq!((text(&columns[0].before), text(&columns[0].after)), ("a".into(), "b".into()));
            assert!(columns[1].aligned && columns[1].before.is_empty() && columns[1].after.is_empty());
            assert_eq!((text(&columns[2].before), text(&columns[2].after)), ("x".into(), "y".into()));
            assert!(columns[3].before.is_empty() && columns[3].after.is_empty());
        }
    }

    #[test]
    fn equation_array_marker_offsets_use_original_utf16_and_keep_source_unmodified() {
        let original = row("<m:r readmdId=\"rd-formula-token-omml-1\"><m:t>𝑎&amp;b</m:t></m:r>");
        let frozen = original.clone();
        let plan = plan_row(&original, true);
        assert_eq!(plan.view.children[0].equation_markers[0].offset, 2);
        assert_eq!(original, frozen);
        let columns = node_columns(&plan);
        assert_eq!(
            columns[0].after[0].attr("readmdId"),
            Some("rd-formula-token-omml-1-text-3")
        );
        let unmarked = plan_row(&row("<m:r><m:t>a=b=c</m:t></m:r>"), true);
        assert!(!unmarked.has_markers);
        assert_eq!(node_columns(&unmarked).len(), 1);
    }

    #[test]
    fn equation_array_explicit_alignment_requires_operator_emulator_and_false_is_off() {
        for (properties, aligned) in [
            ("<m:aln/>", false),
            ("<m:opEmu/><m:aln/>", true),
            ("<m:opEmu/><m:aln m:val=\"0\"/>", false),
            ("<m:opEmu m:val=\"false\"/><m:aln/>", false),
        ] {
            let plan = plan_row(&row(&format!("<m:r><m:t>x</m:t></m:r><m:box><m:boxPr>{properties}</m:boxPr><m:e><m:r><m:t>==</m:t></m:r></m:e></m:box>")), true);
            assert_eq!(plan.has_markers, aligned);
            let columns = node_columns(&plan);
            if aligned {
                assert_eq!(columns[0].after[0].name, "box");
                assert_eq!(
                    columns[0].after[0].child("e").unwrap().children[0].run_text(),
                    "=="
                );
            }
        }
        let plan = plan_row(
            &row("<m:r><m:t>abc</m:t></m:r><m:r><m:rPr><m:aln/></m:rPr><m:t>-x</m:t></m:r>"),
            false,
        );
        assert!(plan.has_markers);
        assert_eq!(text(&node_columns(&plan)[0].before), "abc");
    }

    #[test]
    fn equation_array_ambiguous_markers_report_instead_of_silently_selecting_one() {
        for (body, code) in [
            ("<m:r><m:rPr><m:nor/></m:rPr><m:t>A&amp;B</m:t></m:r>", "math_equation_array_literal_alignment"),
            ("<m:r><m:rPr><m:aln/></m:rPr><m:t>a&amp;b</m:t></m:r>", "math_equation_array_mixed_alignment"),
            ("<m:r><m:rPr><m:aln/></m:rPr><m:t>a</m:t></m:r><m:r><m:rPr><m:aln/></m:rPr><m:t>b</m:t></m:r>", "math_equation_array_mixed_alignment"),
        ] {
            let original = row(body); let plan = plan_row(&original, true);
            assert!(!plan.has_markers); assert_eq!(plan.view, original); assert!(plan.diagnostics.iter().any(|d| d.code == code));
        }
        let nested = row("<m:f><m:num><m:r><m:t>a&amp;b</m:t></m:r></m:num><m:den><m:r><m:t>1</m:t></m:r></m:den></m:f><m:r><m:t>&amp;x</m:t></m:r>");
        let plan = plan_row(&nested, true);
        assert!(plan.has_markers);
        assert_eq!(node_columns(&plan).len(), 1);
    }

    #[test]
    fn equation_array_many_fragments_share_one_immutable_original_run() {
        let original = row(&format!(
            "<m:r><m:t>{}</m:t></m:r>",
            "a&amp;b&amp;".repeat(256)
        ));
        let plan = plan_row(&original, true);
        let columns = node_columns(&plan);
        let origins = columns
            .iter()
            .flat_map(|c| c.before.iter().chain(&c.after))
            .filter_map(|n| n.equation_origin.as_ref())
            .collect::<Vec<_>>();
        assert!(origins.len() > 256);
        for origin in &origins[1..] {
            assert!(std::sync::Arc::ptr_eq(origins[0], origin));
        }
        assert_eq!(origins[0].run_text(), original.children[0].run_text());
    }

    #[test]
    fn equation_array_replanning_is_idempotent_and_budget_failure_keeps_all_text() {
        let original = row("<m:r><m:t>a&amp;b&amp;x&amp;y</m:t></m:r>");
        let first = plan_row(&original, true);
        let second = plan_row(&first.view, true);
        assert_eq!(first.marker_count, second.marker_count);
        assert_eq!(first.view, second.view);
        let large = row(&format!(
            "<m:r><m:t>{}</m:t></m:r>",
            "a&amp;".repeat(MAX_MARKERS + 1)
        ));
        let plan = plan_row(&large, true);
        assert_eq!(plan.view, large);
        assert!(!plan.has_markers);
        assert!(plan
            .diagnostics
            .iter()
            .any(|d| d.code == "math_equation_array_marker_budget"));
    }
}
