//! P04 decoration validation contract: value matrix, target whitelist,
//! ordinary-paragraph context, consumer rejections with protected old
//! targets, reader/preview separation and unchanged behaviour for prior keys.
use super::*;
use serde_json::json;

const DECORATED: &str = "<!-- readmd:layout v1 {\"paragraph\":{\"shading\":{\"fill\":\"DCE6F1\"}}} -->\n\nDecorated body.\n";

fn style(paragraph: &Value) -> Value { json!({ "id": "block-test", "paragraph": paragraph }) }

#[test]
fn decoration_value_matrix_is_strict_through_parser_and_style_table() {
    let edge = |width: f64, space: i64| json!({"color":"4472C4","widthPt":width,"spacePt":space});
    let valid = [
        json!({"shading":{"fill":"DCE6F1"}}),
        json!({"shading":{"fill":"dce6f1"}}),
        json!({"border":{"left":edge(2.5,4)}}),
        json!({"border":{"top":edge(0.25,0),"left":edge(2.5,4),"bottom":edge(12.,31),"right":edge(1.,15)}}),
        json!({"border":{"left":{"color":"000000","widthPt":0.25,"spacePt":0}}}),
        json!({"border":{"right":{"color":"FFFFFF","widthPt":12.0,"spacePt":31}}}),
        json!({"shading":{"fill":"DCE6F1"},"border":{"left":edge(2.5,4)}}),
        // Sparse-reset leftovers are not declarations.
        json!({"shading":{}}),
        json!({"shading":{},"border":{}}),
    ];
    for value in valid {
        let parsed = parse(&value).unwrap();
        if value["shading"].as_object().is_some_and(|o| !o.is_empty())
            || value["border"].as_object().is_some_and(|o| !o.is_empty()) {
            assert!(parsed.is_some(), "{value}");
        } else {
            assert!(parsed.is_none(), "{value}");
        }
        crate::publication_blocks::validate_style(&style(&value)).unwrap();
    }
    let invalid = [
        // hex: wrong length, '#'-prefixed, non-hex digits, non-string
        json!({"shading":{"fill":"DCE6F"}}), json!({"shading":{"fill":"#DCE6F1"}}),
        json!({"shading":{"fill":"DCE6F11"}}), json!({"shading":{"fill":"GGE6F1"}}),
        json!({"shading":{"fill":220}}),
        // unknown keys in either object, unknown edge names
        json!({"shading":{"fill":"DCE6F1","alpha":50}}),
        json!({"shading":{"background":"DCE6F1"}}),
        json!({"border":{"middle":edge(2.5,4)}}),
        json!({"border":{"left":edge(2.5,4),"diagonal":edge(2.5,4)}}),
        // widthPt outside [0.25,12]
        json!({"border":{"left":{"color":"4472C4","widthPt":0.24,"spacePt":4}}}),
        json!({"border":{"left":{"color":"4472C4","widthPt":12.01,"spacePt":4}}}),
        json!({"border":{"left":{"color":"4472C4","widthPt":-1.,"spacePt":4}}}),
        // spacePt outside [0,31] or not an integer
        json!({"border":{"left":{"color":"4472C4","widthPt":2.5,"spacePt":32}}}),
        json!({"border":{"left":{"color":"4472C4","widthPt":2.5,"spacePt":-1}}}),
        json!({"border":{"left":{"color":"4472C4","widthPt":2.5,"spacePt":4.5}}}),
        // edge shape: missing/extra keys
        json!({"border":{"left":{"color":"4472C4","widthPt":2.5}}}),
        json!({"border":{"left":{"color":"4472C4","widthPt":2.5,"spacePt":4,"style":"dashed"}}}),
        // nulls anywhere are denied
        json!({"shading":null}), json!({"shading":{"fill":null}}),
        json!({"border":null}), json!({"border":{"left":null}}),
        json!({"shading":"DCE6F1"}),
    ];
    for value in invalid {
        assert_eq!(parse(&value), Err(INVALID), "{value}");
        assert_eq!(
            crate::publication_blocks::validate_style(&style(&value)),
            Err("publication_local_style_invalid"),
            "{value}"
        );
    }
    // The style-table branch delegates to the same single parser per key.
    for (key, value) in [
        ("shading", json!({"fill":"DCE6F1"})),
        ("border", json!({"left":{"color":"4472C4","widthPt":2.5,"spacePt":4}})),
    ] {
        assert!(parse_key(key, &value).unwrap().is_some());
    }
    assert_eq!(parse_key("shading", &json!({"fill":"DCE6F"})), Err(INVALID));
    assert_eq!(parse_key("border", &json!({"middle":{}})), Err(INVALID));
}

#[test]
fn decoration_targets_only_ordinary_paragraph_kinds() {
    let shading = r#"{"paragraph":{"shading":{"fill":"DCE6F1"}}}"#;
    for source in [
        format!("<!-- readmd:layout v1 {shading} -->\n\n# Heading\n"),
        format!("<!-- readmd:layout v1 {shading} -->\n\n:label[eq:target] Estimate\n"),
        format!("<!-- readmd:layout v1 {shading} -->\n\n```readmd-math v1\nfrac(a,b)\n```\n"),
        // The same rule applies to a border declaration.
        format!("<!-- readmd:layout v1 {{\"paragraph\":{{\"border\":{{\"left\":{{\"color\":\"4472C4\",\"widthPt\":2.5,\"spacePt\":4}}}}}}}} -->\n\n## Sub heading\n"),
    ] {
        let diagnostics = crate::publication_blocks::analyze(&source).unwrap().diagnostics;
        assert_eq!(diagnostics[0]["code"], "publication_local_style_invalid", "{source}");
        assert_eq!(crate::publication_blocks::validate_source(&source).unwrap_err(),
            "publication_local_style_invalid");
    }
    assert!(crate::publication_blocks::analyze(DECORATED).unwrap().diagnostics.is_empty());
    crate::publication_blocks::validate_source(DECORATED).unwrap();
}

#[test]
fn decoration_context_rejects_non_ordinary_stories() {
    let inline = r#"<!-- readmd:layout v1 {"paragraph":{"shading":{"fill":"DCE6F1"}}} -->"#;
    for source in [
        format!("> Quoted text {inline}\n"),
        format!("- List item {inline}\n"),
        format!("[^n]: Note text {inline}\n\nBody.\n"),
    ] {
        // The declaration itself attaches cleanly; only its context is wrong.
        assert!(crate::publication_blocks::analyze(&source).unwrap().diagnostics.is_empty(), "{source}");
        for consumer in [Consumer::Docx, Consumer::RustPdf] {
            assert_eq!(validate_source_consumer(consumer, &source), Err(UNSUPPORTED), "{source}");
        }
        assert_eq!(validate_source_consumer(Consumer::Unsupported, &source), Err(UNSUPPORTED));
        assert_eq!(
            crate::publication_typography::validate_export("docx", &source, &json!({})),
            Err(UNSUPPORTED));
        assert_eq!(
            crate::publication_typography::validate_export("pdf", &source, &json!({})),
            Err(UNSUPPORTED));
    }
    // An ordinary body paragraph passes both supporting consumers.
    let body = format!("Ordinary body {inline}\n");
    for consumer in [Consumer::Docx, Consumer::RustPdf] {
        validate_source_consumer(consumer, &body).unwrap();
    }
    crate::publication_typography::validate_export("docx", &body, &json!({})).unwrap();
    crate::publication_typography::validate_export("pdf", &body, &json!({})).unwrap();
}

#[test]
fn decoration_rejecting_exports_preserve_old_targets_without_assets() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    for (extension, run) in [
        ("tex", Box::new(|base: &str, target: &str| crate::mdexport::export_tex(
            DECORATED, base, target, &json!({}), "QA").map(|_| ())) as Box<dyn Fn(&str,&str)->Result<(),String>>),
        ("epub", Box::new(|base: &str, target: &str| crate::mdexport::export_epub(
            DECORATED, base, target, &json!({}), "QA").map(|_| ()))),
        ("html", Box::new(|base: &str, target: &str| crate::mdexport::export_html(
            DECORATED, base, target, &json!({}), "QA", std::path::Path::new(base).join("assets").as_path()).map(|_| ()))),
        ("typst.pdf", Box::new(|base: &str, target: &str| crate::typst_compile::export_pdf(
            DECORATED, base, target, &json!({"pdf":{"engine":"typst"}}), "QA",
            std::path::Path::new(base), std::path::Path::new(base)).map(|_| ()))),
    ] {
        let target = dir.path().join(format!("old.{extension}"));
        std::fs::write(&target, b"original owned result").unwrap();
        assert_eq!(run(base, target.to_str().unwrap()).unwrap_err(), UNSUPPORTED, "{extension}");
        assert_eq!(std::fs::read(&target).unwrap(), b"original owned result", "{extension}");
        assert!(!target.with_extension("assets").exists(), "{extension}");
    }
}

#[test]
fn decoration_reader_accepts_without_weakening_preview_and_export_guards() {
    assert_eq!(crate::publication_typography::validate_reader_view(DECORATED, &json!({})), Ok(()));
    assert_eq!(crate::publication_typography::validate_preview(DECORATED, &json!({})), Err(UNSUPPORTED));
    let (_, rules) = crate::publication_blocks::view(DECORATED).unwrap();
    assert_eq!(rules.as_object().unwrap().values().next().unwrap()["background-color"], "#DCE6F1");
    // Rust PDF preview accepts exactly like its export; typst/xelatex previews
    // refuse exactly like their exports.
    crate::publication_typography::validate_pdf_preview("pdf", DECORATED, &json!({"pdf":{"engine":"rust"}})).unwrap();
    crate::publication_typography::validate_pdf_preview("pdf", DECORATED, &json!({})).unwrap();
    assert_eq!(crate::publication_typography::validate_pdf_preview("pdf", DECORATED,
        &json!({"pdf":{"engine":"typst"}})), Err(UNSUPPORTED));
    assert_eq!(crate::publication_typography::validate_pdf_preview("pdf", DECORATED,
        &json!({"pdf":{"engine":"xelatex"}})), Err(UNSUPPORTED));
    // Flow (epub/html) and unknown formats refuse at the export gate.
    for format in ["epub", "html", "typ", "rtf"] {
        assert_eq!(crate::publication_typography::validate_export(format, DECORATED, &json!({})), Err(UNSUPPORTED));
    }
}

#[test]
fn decoration_used_reads_parsed_declarations_not_prose_or_code() {
    assert_eq!(used(DECORATED).unwrap(), true);
    assert_eq!(used("Plain body.\n").unwrap(), false);
    assert_eq!(used("A paragraph mentions shading without declaring it.\n").unwrap(), false);
    assert_eq!(used("```md\n<!-- readmd:layout v1 {\"paragraph\":{\"shading\":{\"fill\":\"DCE6F1\"}}} -->\n```\n").unwrap(), false);
    // Emptied reset leftovers are not declarations.
    assert_eq!(used("<!-- readmd:layout v1 {\"paragraph\":{\"shading\":{}}} -->\n\nBody\n").unwrap(), false);
    // validate_input now routes decorated sources onto the typed publication
    // projection (publicationStyle) for the core PDF renderer.
    assert_eq!(crate::publication_typography::validate_input(DECORATED, &json!({})).unwrap(), true);
    assert_eq!(crate::publication_typography::validate_input("Plain body.\n", &json!({})).unwrap(), false);
}

#[test]
fn invalid_decoration_values_surface_as_bare_invalid_with_diagnostics() {
    let source = "<!-- readmd:layout v1 {\"paragraph\":{\"shading\":{\"fill\":\"DCE6F\"}}} -->\n\nBody\n";
    let diagnostics = crate::publication_blocks::analyze(source).unwrap().diagnostics;
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0]["code"], "publication_local_style_invalid");
    assert!(diagnostics[0]["from"].as_u64().unwrap() < diagnostics[0]["to"].as_u64().unwrap());
    assert_eq!(crate::publication_blocks::validate_source(source).unwrap_err(),
        "publication_local_style_invalid");
    // The editing surface refuses the same patch before touching the source.
    let plain = "Body\n";
    let model = crate::publication_blocks::analyze(plain).unwrap();
    assert_eq!(
        crate::publication_blocks::apply(plain, &model.revision, &model.nodes[0].id,
            &json!({"paragraph":{"shading":{"fill":"DCE6"}}})).unwrap_err(),
        "publication_local_style_invalid");
}

#[test]
fn decoration_source_roundtrip_preserves_and_resets_the_declaration() {
    let plain = "One.\n";
    let model = crate::publication_blocks::analyze(plain).unwrap();
    let id = model.nodes[0].id.clone();
    let applied = crate::publication_blocks::apply(plain, &model.revision, &id,
        &json!({"paragraph":{"shading":{"fill":"DCE6F1"},"border":{"left":{"color":"4472C4","widthPt":2.5,"spacePt":4}}}})).unwrap();
    let decorated = applied["result"]["content"].as_str().unwrap();
    assert!(decorated.contains(crate::publication_blocks::PREFIX));
    crate::publication_blocks::validate_source(decorated).unwrap();
    assert_eq!(used(decorated).unwrap(), true);
    let current = crate::publication_blocks::analyze(decorated).unwrap();
    // Same declaration re-exported from the same source is byte-stable.
    let again = crate::publication_blocks::apply(decorated, &current.revision, &id,
        &json!({"paragraph":{"shading":{"fill":"DCE6F1"},"border":{"left":{"color":"4472C4","widthPt":2.5,"spacePt":4}}}})).unwrap();
    assert_eq!(again["result"]["changed"], false);
    // Sparse reset removes a whole declared key; nested single-edge nulls are
    // not part of the patch vocabulary (the style table would reject them).
    let cleared = crate::publication_blocks::apply(decorated, &current.revision, &id,
        &json!({"paragraph":{"border":null}})).unwrap();
    let content = cleared["result"]["content"].as_str().unwrap();
    assert!(content.contains("\"shading\":{\"fill\":\"DCE6F1\"}"));
    assert!(!content.contains("\"border\""));
    let cleared_model = crate::publication_blocks::analyze(content).unwrap();
    let reset = crate::publication_blocks::apply(content, &cleared_model.revision, &id,
        &json!({"paragraph":{"shading":null}})).unwrap();
    assert_eq!(reset["result"]["content"], plain);
}

#[test]
fn existing_key_validation_behaviour_is_unchanged() {
    // Valid prior keys keep passing, alone or beside the new decoration keys.
    for good in [
        json!({"text":{"size":14,"color":"#234567","bold":true,"trackingPt":0.5},
               "paragraph":{"align":"right","before":6,"after":6,"left":8,"lineHeight":1.5}}),
        json!({"text":{"size":20},"paragraph":{"align":"center","lineSpacing":{"mode":"exact","pt":18}}}),
        json!({"paragraph":{"shading":{"fill":"DCE6F1"},"align":"center"}}),
        json!({"text":{"baselineShiftPt":3}}),
    ] {
        let mut style = good;
        style["id"] = json!("block-test");
        crate::publication_blocks::validate_style(&style).unwrap();
    }
    // Invalid prior keys keep failing with the same bare code.
    for bad in [
        json!({"text":{"size":999}}),
        json!({"text":{"color":"123456"}}),
        json!({"paragraph":{"align":"middle"}}),
        json!({"paragraph":{"before":101}}),
        json!({"paragraph":{"left":61}}),
        json!({"paragraph":{"firstLine":2,"hanging":3}}),
        json!({"paragraph":{"lineSpacing":{"mode":"exact","pt":5}}}),
        json!({"text":{"lineSpacing":{"mode":"exact","pt":14}}}),
        json!({"unknown":{"key":1}}),
    ] {
        assert_eq!(crate::publication_blocks::validate_style(&bad),
            Err("publication_local_style_invalid"), "{bad}");
    }
    // Existing targets keep their own rules: math still accepts size/align.
    let math_style = json!({"text":{"size":20,"color":"#123456"},"paragraph":{"align":"left"}});
    assert_eq!(crate::publication_blocks::analyze(
        &format!("<!-- readmd:layout v1 {} -->\n\n```readmd-math v1\nfrac(a,b)\n```\n", math_style))
        .unwrap().diagnostics.is_empty(), true);
}
