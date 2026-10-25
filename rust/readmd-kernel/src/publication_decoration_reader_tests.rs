//! Reader API and local CSS contracts. Native export refusal controls remain
//! in publication_decoration_tests; these tests never launch a rendering app.
use super::*;
use serde_json::json;
use std::{path::Path, sync::Arc};

fn paragraph() -> Value {
    json!({"shading":{"fill":"dCe6F1"},"border":{
        "top":{"color":"112233","widthPt":0.25,"spacePt":0},
        "left":{"color":"4472C4","widthPt":2.6,"spacePt":4},
        "bottom":{"color":"445566","widthPt":12,"spacePt":31},
        "right":{"color":"778899","widthPt":1.25,"spacePt":7}
    }})
}

fn directive() -> String {
    format!("<!-- readmd:layout v1 {} -->", json!({"id":"reader-decoration","paragraph":paragraph()}))
}

fn app(root: &Path) -> Arc<crate::App> {
    let assets = root.join("assets");
    std::fs::create_dir_all(&assets).unwrap();
    Arc::new(crate::App::bootstrap(crate::paths::AppPaths::with_dirs(&root.join("profile"), root, &assets)).unwrap())
}

#[test]
fn p04_decoration_reader_preserves_all_sides_exact_points_and_source_identity() {
    let source = format!("\u{feff}{}\r\n\r\nOriginal 中文 e\u{301} 👩\u{200d}💻 **bold** [@sample].\r\n", directive());
    let original = source.clone();
    let revision = crate::document_transaction::revision(&source);
    crate::publication_typography::validate_reader_view(&source, &json!({})).unwrap();
    let (view, rules) = crate::publication_blocks::view(&source).unwrap();
    assert!(view.starts_with('\u{feff}'));
    assert!(view.contains("data-readmd-layout=\"reader-decoration-"));
    assert!(view.ends_with("\r\n\r\nOriginal 中文 e\u{301} 👩\u{200d}💻 **bold** [@sample].\r\n"));
    assert_eq!(rules.as_object().unwrap().len(), 1);
    let css = rules.as_object().unwrap().values().next().unwrap();
    assert_eq!(css["background-color"], "#dCe6F1");
    for (side, color, width, space) in [
        ("top", "#112233", "0.25pt", "0pt"), ("left", "#4472C4", "2.6pt", "4pt"),
        ("bottom", "#445566", "12pt", "31pt"), ("right", "#778899", "1.25pt", "7pt"),
    ] {
        assert_eq!(css[format!("border-{side}-color")], color);
        assert_eq!(css[format!("border-{side}-style")], "solid");
        assert_eq!(css[format!("border-{side}-width")], width);
        assert_eq!(css[format!("padding-{side}")], space);
    }
    assert_eq!(css.as_object().unwrap().len(), 17);
    assert_eq!(source, original);
    assert_eq!(crate::document_transaction::revision(&source), revision);
    // A trailing top-level directive uses the same ordinary-paragraph lane.
    let inline = format!("Original body {}\n", directive());
    let (view, rules) = crate::publication_blocks::view(&inline).unwrap();
    assert!(view.contains("data-readmd-layout-inline=\"true\""));
    assert_eq!(rules.as_object().unwrap().values().next().unwrap()["padding-bottom"], "31pt");
}

#[test]
fn p04_decoration_reader_http_keeps_csl_numbering_and_original_utf16_ranges() {
    let root = tempfile::tempdir().unwrap();
    let app = app(root.path());
    let bib = "@book{kant,title={Critique of Pure Reason},author={Kant, Immanuel},year={1781}}";
    std::fs::write(root.path().join("refs.bib"), bib).unwrap();
    let source = format!("\u{feff}---\r\nbibliography: refs.bib\r\ncsl: apa\r\n---\r\n<!-- readmd-numbering:v1 {{\"headings\":{{\"levels\":[{{}}]}}}} -->\r\n\r\n# Original heading {{#stable}}\r\n\r\n{}\r\n\r\n😀 中文 [@kant, p. 42].\r\n\r\nAgain [@kant].\r\n", directive());
    let original = root.path().join("Original.md");
    std::fs::write(&original, &source).unwrap();
    let (status, body) = crate::server::call_in_process(&app, "POST", "/api/academic/document", Some(&json!({
        "content":source,"baseDir":root.path(),"view":true,"options":{}
    })));
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["ok"], true, "{body}");
    let display = body["content"].as_str().unwrap();
    assert!(display.contains("data-readmd-layout=\"reader-decoration-"), "{display}");
    assert_eq!(display.matches("data-readmd-citation-index=").count(), 2, "{display}");
    assert!(display.contains("Kant") && display.contains("42"), "{display}");
    assert_eq!(body["source_numbering"]["headings"][0]["id"], "stable");
    assert_eq!(body["source_numbering"]["headings"][0]["numbering"]["text"], "1");
    let projection: crate::publication_rich::Projection = serde_json::from_value(body["projection"].clone()).unwrap();
    projection.verify_source(&source).unwrap();
    assert_eq!(projection.citations.len(), 2);
    for (index, citation) in projection.citations.iter().enumerate() {
        let span = &citation.source.span;
        assert_eq!(&source[span.byte_from..span.byte_to], span.source);
        assert_eq!(span.from, source[..span.byte_from].encode_utf16().count());
        assert_eq!(body["source_citations"][index]["from"], span.from);
        assert_eq!(body["source_citations"][index]["to"], span.to);
    }
    let css = body["layout"].as_object().unwrap().values().next().unwrap();
    assert_eq!(css["background-color"], "#dCe6F1");
    assert_eq!(css["border-left-width"], "2.6pt");
    assert_eq!(std::fs::read_to_string(original).unwrap(), source);
    assert_eq!(std::fs::read_to_string(root.path().join("refs.bib")).unwrap(), bib);
}

#[test]
fn p04_decoration_reader_http_still_rejects_invalid_values_context_and_physical_spacing() {
    let root = tempfile::tempdir().unwrap();
    let app = app(root.path());
    let invalid = r##"<!-- readmd:layout v1 {"paragraph":{"shading":{"fill":"#DCE6F1"}}} -->"##;
    for (source, code) in [
        (format!("{invalid}\n\nBody.\n"), INVALID),
        (format!("{}\n\n# Heading\n", directive()), INVALID),
        (format!("> Quoted {}\n", directive()), UNSUPPORTED),
        (format!("- Listed {}\n", directive()), UNSUPPORTED),
        (format!("Body[^n].\n\n[^n]: Note {}\n", directive()), UNSUPPORTED),
        (format!("<!-- readmd:layout v1 {} -->\n\nBody.\n", json!({"paragraph":{
            "shading":{"fill":"DCE6F1"},"lineSpacing":{"mode":"exact","pt":18}
        }})), crate::publication_typography::UNSUPPORTED),
    ] {
        // A missing resource must never hide the earlier decoration refusal.
        let source = format!("---\nbibliography: missing.json\n---\n{source}");
        let (status, body) = crate::server::call_in_process(&app, "POST", "/api/academic/document", Some(&json!({
            "content":source,"baseDir":root.path(),"view":true
        })));
        assert_eq!(status, 422, "{body}");
        assert_eq!(body["error_code"], code, "{source}: {body}");
        assert!(body.get("layout").is_none());
        assert!(body.get("projection").is_none());
    }
    let source = format!("{}\n\nBody.\n", directive());
    assert_eq!(crate::publication_typography::validate_reader_view(&source, &json!({
        "typography":{"lineSpacing":{"mode":"exact","pt":18}}
    })), Err(crate::publication_typography::UNSUPPORTED));
    assert_eq!(crate::publication_typography::validate_preview(&source, &json!({})), Err(UNSUPPORTED));
    for (format, engine) in [("html", "rust"), ("epub", "rust"), ("tex", "xelatex"), ("pdf", "typst"), ("pdf", "xelatex")] {
        assert_eq!(crate::publication_typography::validate_export(format, &source, &json!({"pdf":{"engine":engine}})), Err(UNSUPPORTED));
    }
}
