//! Synthetic regression controls for actual inherited-size failures.
use super::*;
use serde_json::json;

fn no_image(_: &str) -> Option<String> {
    None
}
fn style(body: f64, heading: f64, cell: f64) -> Value {
    crate::export_styles::sanitize_options(Some(
        &json!({"typography":{"size":body},"headings":{"h1":{"size":heading},"h6":{"size":heading}},"table":{"cellSize":cell},"cover":{"enabled":false},"toc":{"enabled":false}}),
    ))
}
fn source(width: &str, glyph: &str) -> String {
    crate::math_document::parse(
        &format!("<math><mspace width='{width}'/><mi>{glyph}</mi></math>"),
        "mathml",
        false,
    )
    .unwrap()
    .root
    .xml()
}
fn math(width: &str, glyph: &str) -> Value {
    let xml = source(width, glyph);
    let root = crate::math_document::parse_xml(&xml).unwrap();
    json!({"t":"math","omml":xml,"latex":crate::math_formats::tex(&root),"fallback":true,"display":false})
}
fn error(run: &Run) -> &PdfError {
    match &run.content {
        RunContent::Error(error) => error,
        _ => panic!("Expected typed mathematical failure, got {run:?}"),
    }
}
fn image(run: &Run) -> &InlineImage {
    match &run.content {
        RunContent::Image(image) if image.math.is_some() => image,
        _ => panic!("Expected actual vector formula, got {run:?}"),
    }
}
fn story(blocks: &Value, options: &Value) -> (Vec<Flowable>, Styles, DocGeometry) {
    let st = Styles::build(options, "MicrosoftYaHei");
    let geom = DocGeometry::from_style(options);
    (
        build_story(
            blocks,
            options,
            "",
            &no_image,
            &st,
            "MicrosoftYaHei",
            &geom,
            &mut vec![],
            &[],
        ),
        st,
        geom,
    )
}
fn fixture_error() -> Run {
    let options = style(24., 24., 24.);
    // Body size is sanitized to 20pt: 1600em = 32000pt > 20000pt.
    assert_eq!(Styles::build(&options, "MicrosoftYaHei").body.size, 20.);
    inline_markup(&json!([math("1600em", "x")]), &options, "", &no_image).remove(0)
}

#[test]
fn mathml_space_pdf_sanitized_sizes_have_distinct_scalar_budget_boundaries() {
    let options = style(24., 24., 24.);
    let body = Styles::build(&options, "MicrosoftYaHei").body.size;
    let table = options["table"]["cellSize"].as_f64().unwrap();
    assert_eq!((body, table), (20., 16.));
    // The documented scalar limit applies to the space itself. Extra glyph
    // ink/advance is separately constrained by the actual page frame.
    for (points, boundary) in [(body, 1000), (table, 1250)] {
        assert_eq!(points * f64::from(boundary), 20000.);
        let valid = math(&format!("{boundary}em"), "x");
        let runs = inline_markup_at_size(&json!([valid]), &options, "", &no_image, points);
        assert_eq!(runs[0].size, points);
        assert!(image(&runs[0]).width > 20000.);
        let invalid = math(&format!("{}em", boundary + 1), "x");
        let runs = inline_markup_at_size(&json!([invalid]), &options, "", &no_image, points);
        assert_eq!(error(&runs[0]).message, "math_budget");
    }
}

#[test]
fn mathml_space_pdf_inline_uses_real_size_and_never_stages_text_or_png_fallback() {
    let dir = tempfile::tempdir().unwrap();
    for fallback in [false, true] {
        // 1600em is 19200pt at 12pt, but 32000pt at sanitized body 20pt.
        let mut node = math("1600em", "x");
        node["fallback"] = json!(fallback);
        node["png"] = json!("aW52YWxpZA==");
        node["w"] = json!(1);
        node["h"] = json!(1);
        let runs = inline_markup(
            &json!([node.clone()]),
            &style(12., 12., 12.),
            dir.path().to_str().unwrap(),
            &no_image,
        );
        assert!(image(&runs[0]).width > 19200.);
        assert_eq!(runs[0].size, 12.);
        let runs = inline_markup(
            &json!([node]),
            &style(24., 24., 24.),
            dir.path().to_str().unwrap(),
            &no_image,
        );
        assert_eq!(runs.len(), 1);
        assert_eq!(error(&runs[0]).message, "math_budget");
        assert_eq!(error(&runs[0]).kind, PdfErrorKind::Invalid);
        assert!(
            std::fs::read_dir(dir.path()).unwrap().next().is_none(),
            "Rejected math cannot materialize a fallback image"
        );
    }
}

#[test]
fn mathml_space_pdf_missing_vector_is_a_typed_error_for_both_fallback_policies() {
    for fallback in [false, true] {
        let mut node = math("1em", "\u{10ffff}");
        node["fallback"] = json!(fallback);
        let runs = inline_markup(&json!([node.clone()]), &style(12., 12., 12.), "", &no_image);
        assert_eq!(error(&runs[0]).message, "math_unsupported_extension");
        let blocks = json!([{"type":"paragraph","text":[node]}]);
        assert_eq!(
            build(&blocks, &style(12., 12., 12.), "", &no_image, &mut vec![])
                .unwrap_err()
                .message,
            "math_unsupported_extension"
        );
    }
}

#[test]
fn mathml_space_pdf_retarget_removes_rejected_geometry_but_keeps_plain_office_behavior() {
    let options = style(12., 24., 24.);
    let original = inline_markup(&json!([math("1000em", "x")]), &options, "", &no_image);
    let saved = original[0].clone();
    let changed = retarget_runs(&original, 24., saved.color);
    assert_eq!(error(&changed[0]).message, "math_budget");
    assert!(!changed[0].is_image());
    assert_eq!(original[0], saved);
    for size in [0., -1., f64::NAN, f64::INFINITY] {
        assert_eq!(
            error(&retarget_runs(&original, size, saved.color)[0]).message,
            "math_invalid_property"
        );
    }
    let ordinary = inline_markup(
        &json!([{"t":"math","latex":"x^2"}]),
        &options,
        "",
        &no_image,
    );
    let unchanged = retarget_runs(&ordinary, f64::NAN, ordinary[0].color);
    assert_eq!(unchanged[0], ordinary[0]);
    let mut mono = original.clone();
    mono[0].face = FaceStyle::Mono;
    assert_eq!(
        error(&retarget_runs(&mono, 24., mono[0].color)[0]).message,
        "math_budget",
        "A math error cannot hide behind a mono face"
    );
}

#[test]
fn mathml_space_pdf_heading_and_table_construct_at_final_size_before_body_budget() {
    // Body 24 -> 20pt and cell 24 -> 16pt are public sanitizer policy.
    // 1600em remains valid at the actual 12pt positive size, and exceeds
    // the budget at both 24pt heading and 16pt table negative sizes.
    let node = math("1600em", "x");
    let rejected = style(12., 24., 24.);
    assert_eq!(Styles::build(&rejected, "MicrosoftYaHei").head[5].size, 24.);
    assert_eq!(rejected["table"]["cellSize"].as_f64(), Some(16.));
    let blocks = json!([{"type":"heading","level":6,"text":[node.clone()]},{"type":"table","header":[[node.clone()]],"rows":[[[node.clone()]]]}]);
    let (flows, _, _) = story(&blocks, &style(24., 12., 12.));
    let heading = flows
        .iter()
        .find_map(|flow| {
            if let Flowable::Paragraph { runs, st, .. } = flow {
                (st.name == "H6").then_some(runs)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(heading[0].size, 12.);
    assert!(image(&heading[0]).width > 19200.);
    let table = flows
        .iter()
        .find_map(|flow| {
            if let Flowable::Table(table) = flow {
                Some(table)
            } else {
                None
            }
        })
        .unwrap();
    for row in &table.rows {
        for cell in row {
            assert_eq!(cell.runs[0].size, 12.);
            assert!(image(&cell.runs[0]).width > 19200.);
        }
    }
    let (flows, _, _) = story(&blocks, &style(12., 24., 24.));
    assert!(flows
        .iter()
        .any(|flow| matches!(flow,Flowable::Error(error) if error.message=="math_budget")));
    let table = flows
        .iter()
        .find_map(|flow| {
            if let Flowable::Table(table) = flow {
                Some(table)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(error(&table.rows[0][0].runs[0]).message, "math_budget");
    assert_eq!(
        build(&blocks, &style(12., 24., 24.), "", &no_image, &mut vec![])
            .unwrap_err()
            .message,
        "math_budget"
    );
}

#[test]
fn mathml_space_pdf_quote_list_and_note_paths_use_their_rendered_sizes() {
    let node = math("1600em", "x");
    let blocks = json!([{"type":"quote","blocks":[{"type":"paragraph","text":[node.clone()]},{"type":"heading","text":[node.clone()]}]},{"type":"list","items":[{"text":[node.clone()]}]}]);
    let options = style(24., 24., 24.);
    let mut st = Styles::build(&options, "MicrosoftYaHei");
    let geom = DocGeometry::from_style(&options);
    st.quote.size = 12.;
    st.li.size = 12.;
    let flows = build_story(
        &blocks,
        &options,
        "",
        &no_image,
        &st,
        "MicrosoftYaHei",
        &geom,
        &mut vec![],
        &[],
    );
    for flow in &flows {
        check_flow_errors(flow).unwrap();
        match flow {
            Flowable::Table(table) => {
                for row in &table.rows {
                    for cell in row {
                        assert!(cell.runs.iter().any(|run| run.is_image()));
                        for run in cell.runs.iter().filter(|run| run.is_image()) {
                            assert_eq!(run.size, 12.);
                        }
                    }
                }
            }
            Flowable::Paragraph { runs, .. } => {
                let formula = runs.iter().find(|run| run.is_image()).unwrap();
                assert_eq!(formula.size, 12.);
            }
            _ => (),
        }
    }
    let mut note = PStyle::child(&st.body, "Footnote");
    note.size = 12.;
    let mut note_flows = vec![];
    add_inline_par(
        &mut note_flows,
        &json!([node.clone()]),
        note,
        &options,
        "",
        &no_image,
        None,
    );
    check_flow_errors(&note_flows[0]).unwrap();
    let xml = source("1600em", "x");
    let doc = crate::md_ast::Document {
        front_matter: None,
        blocks: vec![crate::md_ast::Block::FootnoteDef {
            label: "owned-note".into(),
            blocks: vec![crate::md_ast::Block::Paragraph(vec![
                crate::md_ast::Inline::OfficeMath {
                    latex: "x".into(),
                    omml: xml,
                },
            ])],
        }],
    };
    // The current basic PDF projects authored notes into body paragraphs;
    // validate that actual body size, without inventing a dedicated note size.
    let rendered = crate::md_ast::to_render_json(&doc.blocks);
    assert_eq!(
        build(&rendered, &options, "", &no_image, &mut vec![])
            .unwrap_err()
            .message,
        "math_budget"
    );
    let mut st = Styles::build(&style(12., 12., 12.), "MicrosoftYaHei");
    st.quote.size = 24.;
    st.li.size = 24.;
    let flows = build_story(
        &blocks,
        &style(12., 12., 12.),
        "",
        &no_image,
        &st,
        "MicrosoftYaHei",
        &geom,
        &mut vec![],
        &[],
    );
    assert!(flows.iter().any(|flow| check_flow_errors(flow).is_err()));
}

#[test]
fn mathml_space_pdf_run_errors_survive_every_flow_kind_and_direct_paginate() {
    let options = style(12., 12., 12.);
    let (_, st, geom) = story(&json!([]), &options);
    let failed = fixture_error();
    let plain = text_run("body", FaceStyle::Regular, 12., st.body.color);
    let (table_story, _, _) = story(
        &json!([{"type":"table","header":[[{"t":"text","v":"Header"}]],"rows":[]}]),
        &options,
    );
    let mut table = table_story
        .into_iter()
        .find_map(|flow| {
            if let Flowable::Table(table) = flow {
                Some(table)
            } else {
                None
            }
        })
        .unwrap();
    table.rows[0][0].runs = vec![failed.clone()];
    let cases = vec![
        Flowable::Paragraph {
            runs: vec![failed.clone()],
            st: st.body.clone(),
            anchor: None,
        },
        Flowable::Table(table),
        Flowable::BibliographyEntry {
            first: vec![failed.clone()],
            runs: vec![plain.clone()],
            st: st.body.clone(),
            first_width: 40.,
        },
        Flowable::BibliographyEntry {
            first: vec![plain],
            runs: vec![failed],
            st: st.body.clone(),
            first_width: 40.,
        },
    ];
    for flow in cases {
        assert_eq!(check_flow_errors(&flow).unwrap_err().message, "math_budget");
        let mut doc = ExportDoc::new();
        assert_eq!(
            paginate(&[flow], &geom, &st, &mut doc).unwrap_err().message,
            "math_budget"
        );
    }
}

#[test]
fn mathml_space_pdf_simple_bibliography_checks_errors_before_empty_label_filtering() {
    for first in [true, false] {
        let node = math("1600em", "x");
        let item = if first {
            json!({"firstField":[node],"text":[{"t":"text","v":"Body"}]})
        } else {
            json!({"firstField":[],"text":[node]})
        };
        let blocks = json!([{"type":"bibliography","title":"References","items":[item]}]);
        assert_eq!(
            build(&blocks, &style(24., 24., 24.), "", &no_image, &mut vec![])
                .unwrap_err()
                .message,
            "math_budget"
        );
    }
}

#[test]
fn mathml_space_pdf_recursive_csl_checks_before_visibility_and_atom_filtering() {
    let (_, st, geom) = story(&json!([]), &style(12., 12., 12.));
    let failed = fixture_error();
    assert_eq!(
        csl_paragraph_rows(&[failed.clone()], 0., 200., &st.body, false, geom.height)
            .unwrap_err()
            .message,
        "math_budget"
    );
    let flow = vec![CslFlow::Columns {
        left: vec![CslFlow::Block {
            children: vec![CslFlow::Inline(vec![failed])],
            indent: true,
        }],
        right: vec![],
    }];
    assert!(csl_visible(&flow));
    assert_eq!(
        csl_position_rows(&flow, 0., 200., &st.body, 40., false, false, geom.height)
            .unwrap_err()
            .message,
        "math_budget"
    );
    let runs = inline_markup(
        &json!([math("1em", "")]),
        &style(12., 12., 12.),
        "",
        &no_image,
    );
    assert!(
        !csl_paragraph_rows(&runs, 0., 200., &st.body, false, geom.height)
            .unwrap()
            .is_empty(),
        "Logical zero-ink spacing is not empty content"
    );
}

#[test]
fn mathml_space_pdf_display_guards_body_times_one_point_one_before_any_downsampling() {
    let mut node = math("1600em", "x");
    node["type"] = json!("math");
    node["display"] = json!(true);
    let options = style(12., 12., 12.);
    crate::math_spacing::require_vector_for_spacing_at_size(&node, 12.).unwrap();
    assert_eq!(
        build(&json!([node]), &options, "", &no_image, &mut vec![])
            .unwrap_err()
            .message,
        "math_budget"
    );
    let mut wide = math("100em", "x");
    wide["type"] = json!("math");
    wide["display"] = json!(true);
    assert_eq!(
        build(&json!([wide]), &options, "", &no_image, &mut vec![])
            .unwrap_err()
            .message,
        crate::api_codes::PUBLICATION_LAYOUT_GEOMETRY
    );
    let (flows, _, geom) = story(
        &json!([{"type":"math","display":true,"latex":"x".repeat(1000),"fallback":true}]),
        &options,
    );
    let formula = flows
        .iter()
        .find_map(|flow| {
            if let Flowable::ImageBlock(image) = flow {
                image.math.as_ref()
            } else {
                None
            }
        })
        .unwrap();
    assert!(
        formula.width <= geom.avail + 1e-6,
        "Ordinary Office/TeX keeps its existing fit behavior"
    );
}

#[test]
fn mathml_space_pdf_direct_json_and_markdown_failure_protect_existing_outputs() {
    let options = style(12., 24., 24.);
    let node = math("1000em", "x");
    let blocks = json!([{"type":"heading","level":1,"text":[node]}]);
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("owned.pdf");
    let scratch = dir.path().join("unused-png-cache");
    std::fs::write(&target, b"existing owned PDF").unwrap();
    let failure = render(
        &blocks,
        target.to_str().unwrap(),
        &options,
        scratch.to_str().unwrap(),
        &no_image,
        &mut vec![],
    )
    .unwrap_err();
    assert_eq!(failure.message, "math_budget");
    assert_eq!(std::fs::read(&target).unwrap(), b"existing owned PDF");
    assert!(!scratch.exists());
    let xml = source("1000em", "x");
    let dsl = crate::math_migration::readable(&xml).unwrap();
    let markdown = format!("# :math[{dsl}]\n");
    let failure = crate::mdexport::export_pdf(
        &markdown,
        "",
        target.to_str().unwrap(),
        &options,
        "Synthetic Budget",
        std::path::Path::new("assets"),
    )
    .unwrap_err();
    assert_eq!(failure, "math_budget");
    assert_eq!(std::fs::read(&target).unwrap(), b"existing owned PDF");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    let document = build(
        &json!([{"type":"paragraph","text":[math("1em","x")]}]),
        &style(12., 12., 12.),
        "",
        &no_image,
        &mut vec![],
    )
    .unwrap();
    assert!(
        document
            .pages
            .iter()
            .flat_map(|page| &page.items)
            .any(|item| matches!(item, DrawItem::Formula { .. })),
        "Supported space reaches a real PDF vector item"
    );
}

#[test]
fn mathml_space_pdf_quote_heading_and_callout_title_cannot_flatten_invalid_math() {
    let xml = format!(
        "<m:oMath xmlns:q='{}'><q:mspace width='1em'>",
        crate::math_spacing::MATHML_NS
    );
    let malformed =
        json!({"t":"math","omml":xml,"latex":"must not survive as a substitute","fallback":true});
    let blocks = json!([{"type":"quote","blocks":[{"type":"heading","text":[malformed]}]}]);
    assert_eq!(
        build(&blocks, &style(12., 12., 12.), "", &no_image, &mut vec![])
            .unwrap_err()
            .message,
        "math_invalid_xml"
    );
    let blocks =
        json!([{"type":"quote","kind":"note","title":[math("1em","\u{10ffff}")],"blocks":[]}]);
    assert_eq!(
        build(&blocks, &style(12., 12., 12.), "", &no_image, &mut vec![])
            .unwrap_err()
            .message,
        "math_unsupported_extension"
    );
}

#[test]
fn mathml_space_pdf_small_actual_size_passes_budget_but_rejects_frame_overflow() {
    let node = math("2000em", "x");
    assert_eq!(
        crate::math_spacing::require_vector_for_spacing_at_size(&node, 12.),
        Err("math_budget")
    );
    crate::math_spacing::require_vector_for_spacing_at_size(&node, 8.).unwrap();
    let blocks = json!([{"type":"heading","level":6,"text":[node]}]);
    let options = style(12., 8., 8.);
    let (flows, _, geom) = story(&blocks, &options);
    let heading = flows
        .iter()
        .find(|flow| matches!(flow,Flowable::Paragraph{st,..} if st.name=="H6"))
        .unwrap();
    check_flow_errors(heading).unwrap();
    if let Flowable::Paragraph { runs, .. } = heading {
        assert_eq!(runs[0].size, 8.);
        assert!(image(&runs[0]).width > 16000.);
    } else {
        unreachable!();
    }
    assert_eq!(
        check_flow_spacing_geometry(heading, &geom)
            .unwrap_err()
            .message,
        crate::api_codes::PUBLICATION_LAYOUT_GEOMETRY
    );
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("owned.pdf");
    std::fs::write(&target, b"owned original").unwrap();
    let dsl = crate::math_migration::readable(&source("2000em", "x")).unwrap();
    let markdown = format!("###### :math[{dsl}]\n");
    // The old fixed-12pt export precheck returned math_budget here. The real
    // 8pt budget succeeds; the actual indivisible box then fails frame geometry.
    assert_eq!(
        crate::mdexport::export_pdf(
            &markdown,
            "",
            target.to_str().unwrap(),
            &options,
            "Synthetic Small Heading",
            std::path::Path::new("assets")
        )
        .unwrap_err(),
        crate::api_codes::PUBLICATION_LAYOUT_GEOMETRY
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"owned original");
}

#[test]
fn mathml_space_pdf_every_cell_and_bibliography_column_rejects_physical_overflow() {
    let options = style(12., 12., 12.);
    let node = math("100em", "x");
    crate::math_spacing::require_vector_for_spacing_at_size(&node, 12.).unwrap();
    for blocks in [
        json!([{"type":"paragraph","text":[node.clone()]}]),
        json!([{"type":"table","header":[[node.clone()]],"rows":[]}]),
        json!([{"type":"bibliography","items":[{"firstField":[node.clone()],"text":[{"t":"text","v":"Body"}]}]}]),
        json!([{"type":"bibliography","items":[{"firstField":[{"t":"text","v":"1"}],"text":[node]}]}]),
    ] {
        assert_eq!(
            build(&blocks, &options, "", &no_image, &mut vec![])
                .unwrap_err()
                .message,
            crate::api_codes::PUBLICATION_LAYOUT_GEOMETRY
        );
    }
}

#[test]
fn mathml_space_pdf_unsupported_wrappers_reject_instead_of_flattening_or_omitting() {
    let options = style(12., 12., 12.);
    let node = math("1em", "x");
    for wrapper in [
        json!({"t":"strike","v":[node.clone()]}),
        json!({"t":"unknown","v":[node.clone()]}),
        json!({"v":[node.clone()]}),
    ] {
        assert_eq!(
            build(
                &json!([{"type":"paragraph","text":[wrapper]}]),
                &options,
                "",
                &no_image,
                &mut vec![]
            )
            .unwrap_err()
            .message,
            "math_unsupported_extension"
        );
    }
    for inner in [
        json!({"type":"math","omml":node["omml"],"latex":"x","display":true}),
        json!({"type":"list","items":[{"text":[node.clone()]}]}),
    ] {
        assert_eq!(
            build(
                &json!([{"type":"quote","blocks":[inner]}]),
                &options,
                "",
                &no_image,
                &mut vec![]
            )
            .unwrap_err()
            .message,
            "math_unsupported_extension"
        );
    }
    for block in [
        json!({"type":"unknown","text":[node.clone()]}),
        json!({"text":[node]}),
    ] {
        assert_eq!(
            build(&json!([block]), &options, "", &no_image, &mut vec![])
                .unwrap_err()
                .message,
            "math_unsupported_extension"
        );
    }
}
