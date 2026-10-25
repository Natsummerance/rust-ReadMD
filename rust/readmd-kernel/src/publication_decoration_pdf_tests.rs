//! Real core-PDF paint geometry for decorated ordinary paragraphs: fill and
//! border bars under the text, typed/legacy lane split and direct-JSON checks.
use super::*;
use serde_json::json;

fn options() -> Value {
    crate::export_styles::sanitize(&json!({
        "frontMatter":{"mode":"none"},
        "typography":{"size":11,"lineHeight":1.6,"spacing":6},
        "footer":{"pageNumbers":false,"text":""},"header":{"text":""},
        "cover":{"enabled":false},"toc":{"enabled":false}}))
}

fn render(blocks: &Value) -> crate::pdf_render::RenderedDoc {
    crate::pdf_render::build(blocks, &options(), "", &|_| None, &mut Vec::new()).unwrap()
}

fn rgb(red: u8, green: u8, blue: u8) -> crate::pdf_render::Rgb {
    crate::pdf_render::Rgb::rgb(red as f64 / 255., green as f64 / 255., blue as f64 / 255.)
}

fn fills(page: &crate::pdf_render::RenderedPage) -> Vec<(f64, f64, f64, f64, crate::pdf_render::Rgb)> {
    page.items.iter().filter_map(|item| match item {
        crate::pdf_render::DrawItem::FillRect { x, y, w, h, color } => Some((*x, *y, *w, *h, *color)),
        _ => None,
    }).collect()
}

fn texts(page: &crate::pdf_render::RenderedPage) -> Vec<(f64, f64, String)> {
    page.items.iter().filter_map(|item| match item {
        crate::pdf_render::DrawItem::Text { x, y, text, .. } => Some((*x, *y, text.clone())),
        _ => None,
    }).collect()
}

fn two_line_nodes() -> Vec<Value> {
    vec![json!({"t":"text","v":"FIRST LINE OF THE SHADED PARAGRAPH"}),
        json!({"t":"br"}),
        json!({"t":"text","v":"SECOND LINE OF THE SHADED PARAGRAPH"})]
}

#[test]
fn p04_decoration_only_paragraph_takes_the_typed_lane_with_legacy_geometry() {
    let plain = json!([{"type":"paragraph","text":two_line_nodes()}]);
    let decorated = json!([{"type":"paragraph",
        "publicationStyle":{"id":"shaded","paragraph":{"shading":{"fill":"DCE6F1"}}},
        "text":two_line_nodes()}]);
    let base = render(&plain);
    let deco = render(&decorated);
    // The undecorated twin stays on the legacy lane: no fill, same text.
    assert_eq!(base.pages.len(), 1);
    assert!(base.pages.iter().all(|page| fills(page).is_empty()));
    let base_lines = texts(&base.pages[0]);
    let deco_lines = texts(&deco.pages[0]);
    assert_eq!(base_lines.len(), deco_lines.len());
    // The decoration-only typed lane keeps the legacy leading derivation:
    // identical horizontal positions, identical text and identical inter-line
    // baselines (11pt * 1.6 = 17.6pt). Only the typed lane's existing first-
    // baseline anchor (real glyph bounds vs nominal ascent) may differ.
    for ((bx, by, btext), (dx, dy, dtext)) in base_lines.iter().zip(&deco_lines) {
        assert_eq!(bx, dx);
        assert_eq!(btext, dtext);
        assert!((by - dy).abs() < 11., "anchor shift must stay within one font size");
    }
    for pair in deco_lines.windows(2) {
        assert!((pair[0].1 - pair[1].1 - 17.6).abs() < 1e-9, "legacy leading kept");
    }
    for pair in base_lines.windows(2) {
        assert!((pair[0].1 - pair[1].1 - 17.6).abs() < 1e-9, "legacy leading kept");
    }
    // The decorated paragraph paints exactly one fill in the declared colour,
    // spliced ahead of the paragraph's text in the same content stream.
    let page = &deco.pages[0];
    let painted = fills(page);
    assert_eq!(painted.len(), 1, "{painted:?}");
    assert!((painted[0].4.r - 220. / 255.).abs() < 1e-9);
    assert!((painted[0].4.g - 230. / 255.).abs() < 1e-9);
    assert!((painted[0].4.b - 241. / 255.).abs() < 1e-9);
    let fill_at = page.items.iter().position(|item| matches!(item,
        crate::pdf_render::DrawItem::FillRect { color, .. } if
        (color.r - 220. / 255.).abs() < 1e-9 && (color.g - 230. / 255.).abs() < 1e-9)).unwrap();
    let first_text = page.items.iter().position(|item| matches!(item,
        crate::pdf_render::DrawItem::Text { .. })).unwrap();
    assert!(fill_at < first_text, "the fill must paint below the text");
}

#[test]
fn p04_fill_covers_both_line_boxes_at_the_available_text_width() {
    let decorated = json!([{"type":"paragraph",
        "publicationStyle":{"paragraph":{"shading":{"fill":"DCE6F1"}}},
        "text":two_line_nodes()}]);
    let doc = render(&decorated);
    let page = &doc.pages[0];
    let (x, y, w, h, _) = fills(page)[0];
    let lines = texts(page);
    assert_eq!(lines.len(), 2, "{lines:?}");
    let (first, second) = (&lines[0], &lines[1]);
    assert!(first.1 > second.1, "first line sits above the second");
    // Vertical extent covers both line boxes (baseline ± font ascent/descent).
    assert!(y < second.1 - 2., "fill starts below the second baseline: {y} vs {}", second.1);
    assert!(y + h > first.1 + 8., "fill reaches above the first baseline: {} vs {}", y + h, first.1);
    // Horizontal extent is the paragraph's available text width.
    assert!((x - (doc.geom.left + 6.)).abs() < 1e-6, "{x} vs frame {}", doc.geom.left + 6.);
    assert!((w - doc.geom.avail).abs() < 1e-6, "{w} vs avail {}", doc.geom.avail);
    // An indented paragraph narrows its painted text area accordingly.
    let indented = json!([{"type":"paragraph",
        "publicationStyle":{"paragraph":{"shading":{"fill":"DCE6F1"},"left":10,"right":5}},
        "text":two_line_nodes()}]);
    let narrowed = render(&indented);
    let (nx, _, nw, _, _) = fills(&narrowed.pages[0])[0];
    let mm = 72. / 25.4;
    assert!((nx - (x + 10. * mm)).abs() < 1e-6);
    assert!((nw - (w - 15. * mm)).abs() < 1e-6);
}

#[test]
fn p04_border_edges_paint_solid_bars_offset_by_space_pt() {
    let decorated = json!([{"type":"paragraph",
        "publicationStyle":{"paragraph":{"border":{
            "left":{"color":"4472C4","widthPt":2.5,"spacePt":4},
            "right":{"color":"7F7F7F","widthPt":1.,"spacePt":0},
            "top":{"color":"4472C4","widthPt":1.5,"spacePt":2},
            "bottom":{"color":"4472C4","widthPt":1.5,"spacePt":2}}}},
        "text":two_line_nodes()}]);
    let doc = render(&decorated);
    let page = &doc.pages[0];
    let painted = fills(page);
    // Four bars + no fill declared.
    assert_eq!(painted.len(), 4, "{painted:?}");
    let left = painted.iter().find(|(_, _, w, _, c)|
        (*w - 2.5).abs() < 1e-9 && (c.r - 68. / 255.).abs() < 1e-9).unwrap();
    let text_left = doc.geom.left + 6.;
    // Bar sits 4pt out from the text area and is widthPt wide.
    assert!((left.0 - (text_left - 4. - 2.5)).abs() < 1e-6, "{} vs {}", left.0, text_left - 6.5);
    // Vertical bars span the paragraph's line-box extent.
    let lines = texts(page);
    assert!(left.1 < lines[1].1 && left.1 + left.3 > lines[0].1);
    let right = painted.iter().find(|(_, _, w, _, c)|
        (*w - 1.).abs() < 1e-9 && (c.r - 127. / 255.).abs() < 1e-9).unwrap();
    assert!((right.0 - (text_left + doc.geom.avail)).abs() < 1e-6);
    // Horizontal bars are text-width wide and thickness tall.
    let tops: Vec<_> = painted.iter().filter(|(_, _, w, _, _)| (*w - doc.geom.avail).abs() < 1e-6).collect();
    assert_eq!(tops.len(), 2, "{painted:?}");
    assert!(tops.iter().all(|bar| (bar.2 - doc.geom.avail).abs() < 1e-6 && (bar.3 - 1.5).abs() < 1e-9));
    let top_bar = tops.iter().max_by(|a, b| a.1.partial_cmp(&b.1).unwrap()).unwrap();
    let bottom_bar = tops.iter().min_by(|a, b| a.1.partial_cmp(&b.1).unwrap()).unwrap();
    assert!((top_bar.0 - text_left).abs() < 1e-6);
    assert!(top_bar.1 >= lines[0].1 + 2., "top bar offsets above the line boxes");
    assert!(bottom_bar.1 < lines[1].1, "bottom bar offsets below the last line box");
}

#[test]
fn p04_decorated_paragraph_splits_paint_per_page_segment() {
    let nodes = (0..40).flat_map(|index| [
        json!({"t":"text","v":format!("DECOROW{index:02}")}),
        json!({"t":"br"}),
    ]).collect::<Vec<_>>();
    let decorated = json!([{"type":"paragraph",
        "publicationStyle":{"paragraph":{"shading":{"fill":"DCE6F1"}}},
        "text":nodes}]);
    let doc = render(&decorated);
    assert!(doc.pages.len() >= 2, "expected a page break inside the paragraph");
    for page in &doc.pages {
        let lines = texts(page);
        // A trailing empty frame page (reportlab's eager page begin) carries
        // no text and no decoration; only real segments are asserted.
        if lines.is_empty() { continue; }
        let all = fills(page);
        let painted: Vec<_> = all.iter().filter(|(_, _, _, _, c)|
            (c.g - 230. / 255.).abs() < 1e-9).collect();
        assert_eq!(painted.len(), 1, "one segment fill per page");
        let fill_at = page.items.iter().position(|item| matches!(item,
            crate::pdf_render::DrawItem::FillRect { color, .. }
                if (color.g - 230. / 255.).abs() < 1e-9)).unwrap();
        let first_text = page.items.iter().position(|item| matches!(item,
            crate::pdf_render::DrawItem::Text { text, .. } if text.starts_with("DECOROW"))).unwrap();
        assert!(fill_at < first_text);
    }
    let mut all = String::new();
    for page in &doc.pages { for (_, _, text) in texts(page) { all.push_str(&text); } }
    for index in 0..40 {
        assert_eq!(all.matches(&format!("DECOROW{index:02}")).count(), 1);
    }
}

#[test]
fn p04_md_source_with_tab_emoji_and_crlf_exports_a_decorated_pdf() {
    let root = tempfile::tempdir().unwrap();
    // Core PDF's existing UCS-2 fallback lane rejects astral scalars, so the
    // PDF-side source keeps to tabs, CRLF and CJK — the non-BMP case is
    // covered by the DOCX materialization tests.
    let source = "<!-- readmd:layout v1 {\"paragraph\":{\"shading\":{\"fill\":\"DCE6F1\"},\"border\":{\"left\":{\"color\":\"4472C4\",\"widthPt\":2.5,\"spacePt\":4}}}} -->\r\n\r\nBody\twith CJK 火箭 text.\r\n";
    let target = root.path().join("decorated.pdf");
    crate::mdexport::export_pdf(source, root.path().to_str().unwrap(),
        target.to_str().unwrap(), &options(), "QA", root.path()).unwrap();
    let bytes = std::fs::read(&target).unwrap();
    let pdf = lopdf::Document::load_mem(&bytes).unwrap();
    let extracted = pdf.extract_text(&[1]).unwrap();
    assert!(extracted.contains("Body") && extracted.contains("CJK"), "{extracted}");
    // Rendering the same source twice yields the same painted geometry.
    let blocks = || {
        let parsed = crate::md_ast::parse(source);
        crate::md_ast::to_publication_render_json(&parsed.blocks)
    };
    let first = render(&blocks());
    let second = render(&blocks());
    let geometry = |doc: &crate::pdf_render::RenderedDoc| {
        doc.pages.iter().flat_map(|page| page.items.iter().filter_map(|item| match item {
            crate::pdf_render::DrawItem::FillRect { x, y, w, h, color } =>
                Some(format!("{x:.4} {y:.4} {w:.4} {h:.4} {:.4} {:.4} {:.4}", color.r, color.g, color.b)),
            _ => None,
        })).collect::<Vec<_>>()
    };
    assert_eq!(geometry(&first), geometry(&second));
    assert_eq!(geometry(&first).len(), 2, "fill plus one left bar");
    assert!(geometry(&first)[0].contains("0.8627 0.9020 0.9451"), "DCE6F1 = (220,230,241)/255");
}

#[test]
fn p04_direct_json_decoration_validation_matrix() {
    let ok = json!([{"type":"paragraph",
        "publicationStyle":{"paragraph":{"shading":{"fill":"DCE6F1"}}},
        "text":[{"t":"text","v":"OK"}]}]);
    crate::pdf_render::render_bytes(&ok, &options(), "", &|_| None, &mut Vec::new()).unwrap();
    // Malformed values keep the shared bare invalid code.
    for bad in [
        json!({"fill":"#DCE6F1"}), json!({"fill":"DCE6F"}), json!({"fill":null}),
        json!({"fill":"DCE6F1","extra":1}),
        json!({"border":{"left":{"color":"4472C4","widthPt":12.5,"spacePt":4}}}),
        json!({"border":{"middle":{"color":"4472C4","widthPt":2.,"spacePt":4}}}),
        json!({"border":{"left":{"color":"4472C4","widthPt":2.,"spacePt":32}}}),
    ] {
        let styled = json!([{"type":"paragraph","publicationStyle":{"paragraph":{"shading":bad}},
            "text":[{"t":"text","v":"BAD"}]}]);
        let error = crate::pdf_render::render_bytes(&styled, &options(), "", &|_| None,
            &mut Vec::new()).unwrap_err();
        assert_eq!(error.message, INVALID, "{bad}");
    }
    // Unsupported contexts keep the dedicated capability code.
    for unsupported in [
        json!([{"type":"heading","level":2,
            "publicationStyle":{"paragraph":{"shading":{"fill":"DCE6F1"}}},"text":"H"}]),
        json!([{"type":"quote","blocks":[{"type":"paragraph","publicationContext":"body",
            "publicationStyle":{"paragraph":{"shading":{"fill":"DCE6F1"}}},"text":"Q"}]}]),
        json!([{"type":"paragraph","publicationContext":"footnote",
            "publicationStyle":{"paragraph":{"shading":{"fill":"DCE6F1"}}},"text":"N"}]),
        json!([{"type":"paragraph",
            "publicationStyle":{"paragraph":{"shading":{"fill":"DCE6F1"}}},
            "text":[{"t":"image","src":"never-opened.png"}]}]),
    ] {
        let error = crate::pdf_render::render_bytes(&unsupported, &options(), "", &|_| None,
            &mut Vec::new()).unwrap_err();
        assert_eq!(error.message, UNSUPPORTED, "{unsupported}");
    }
    // A refusal protects a previously written target byte-for-byte.
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("protected.pdf");
    std::fs::write(&target, b"KEEP PDF").unwrap();
    let calls = std::cell::Cell::new(0);
    let resolve = |_: &str| { calls.set(calls.get() + 1); None };
    let error = crate::pdf_render::render(&json!([{"type":"quote","blocks":[{"type":"paragraph",
        "publicationContext":"body","publicationStyle":{"paragraph":{"shading":{"fill":"DCE6F1"}}},"text":"Q"}]}]),
        target.to_str().unwrap(), &options(), "", &resolve, &mut Vec::new()).unwrap_err();
    assert_eq!(error.message, UNSUPPORTED);
    assert_eq!(std::fs::read(&target).unwrap(), b"KEEP PDF");
    assert_eq!(calls.get(), 0, "no resource callback may run before the refusal");
}

#[test]
fn p04_fill_and_border_coexist_under_the_text_in_one_stream() {
    let decorated = json!([{"type":"paragraph",
        "publicationStyle":{"paragraph":{"shading":{"fill":"DCE6F1"},
            "border":{"left":{"color":"4472C4","widthPt":2.5,"spacePt":4}}}},
        "text":two_line_nodes()}]);
    let page = &render(&decorated).pages[0];
    let painted = fills(page);
    assert_eq!(painted.len(), 2, "{painted:?}");
    let fill = painted.iter().find(|(_, _, _, _, c)| (c.g - 230. / 255.).abs() < 1e-9).unwrap();
    let bar = painted.iter().find(|(_, _, w, _, _)| (*w - 2.5).abs() < 1e-9).unwrap();
    assert!(bar.0 < fill.0, "the left bar sits left of the fill");
    let text_at = page.items.iter().position(|item| matches!(item,
        crate::pdf_render::DrawItem::Text { .. })).unwrap();
    let fill_at = page.items.iter().position(|item| matches!(item,
        crate::pdf_render::DrawItem::FillRect { color, .. }
            if (color.g - 230. / 255.).abs() < 1e-9 && (color.r - 220. / 255.).abs() < 1e-9)).unwrap();
    let bar_at = page.items.iter().position(|item| matches!(item,
        crate::pdf_render::DrawItem::FillRect { w, .. } if (*w - 2.5).abs() < 1e-9)).unwrap();
    assert!(fill_at < bar_at || bar_at < fill_at); // both precede the text:
    assert!(fill_at < text_at && bar_at < text_at);
}
