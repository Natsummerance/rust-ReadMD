//! In-process real handlers, persisted OPC files and atomic refusal controls.
//! No managed engine, UI chooser or host font is needed for these tests.
use crate::math_document::{parse_xml, XmlNode};
use crate::publication_typography::{INVALID, UNSUPPORTED};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;

fn inspect_pdf_lines(path: &Path, gap: f64) {
    let pdf=lopdf::Document::load(path).unwrap();
    let mut baselines=Vec::new();
    for id in pdf.get_pages().values() {
        let content=lopdf::content::Content::decode(&pdf.get_page_content(*id).unwrap()).unwrap();
        for operation in content.operations {if operation.operator=="Tm" {
            let y=operation.operands[5].as_float().unwrap() as f64;
            if baselines.last().is_none_or(|last:&f64|(*last-y).abs()>0.001){baselines.push(y);}
        }}
    }
    assert_eq!(baselines.len(),3,"{baselines:?}");
    for pair in baselines.windows(2){assert!((pair[0]-pair[1]-gap).abs()<0.004,"{baselines:?}");}
    let text=pdf.extract_text(&[1]).unwrap();for word in ["FIRST","SECOND","THIRD"]{assert_eq!(text.matches(word).count(),1,"{text}");}
}

#[test]
fn p04_rust_pdf_direct_http_mcp_and_preview_write_real_exact_baselines() {
    let root=tempfile::tempdir().unwrap();let app=app(root.path());
    let original="\u{feff}FIRST  \r\nSECOND  \r\nTHIRD\r\n";
    let source=root.path().join("Source.md");std::fs::write(&source,original).unwrap();
    let mut value=options();value["footer"]=json!({"pageNumbers":false,"text":""});value["header"]=json!({"text":""});
    let direct=root.path().join("Direct.pdf");
    crate::mdexport::export_pdf(original,root.path().to_str().unwrap(),direct.to_str().unwrap(),&value,"Synthetic",root.path()).unwrap();
    inspect_pdf_lines(&direct,18.);
    let http=root.path().join("HTTP.pdf");let (status,body)=crate::server::call_in_process(&app,"POST","/api/export",Some(&json!({
        "format":"pdf","content":original,"out_path":http,"options":value,"suggestedName":"Synthetic"
    })));
    assert_eq!(status,200,"{body}");assert_eq!(body["ok"],true,"{body}");inspect_pdf_lines(&http,18.);
    let mcp=root.path().join("MCP.pdf");let result=crate::mcp::call_tool(&app,"readmd_export_document",&json!({
        "markdown_content":original,"output_format":"pdf","output_path":mcp,"options":value,"confirm":true
    }));
    assert_ne!(result["isError"],true,"{result}");assert_eq!(result["structuredContent"]["ok"],true,"{result}");inspect_pdf_lines(&mcp,18.);
    let preview=crate::export_preview::prepare_with_data("pdf",original,root.path().to_str().unwrap(),&value,
        "Synthetic",root.path(),&root.path().join("profile")).unwrap();
    inspect_pdf_lines(&preview.pdf,18.);assert_eq!(preview.projection_engine,"rust");
    assert_eq!(std::fs::read_to_string(source).unwrap(),original);
}

#[test]
fn p04_rust_pdf_exact_failure_is_structured_for_direct_http_mcp_and_preview_and_atomic() {
    let root=tempfile::tempdir().unwrap();let app=app(root.path());
    let original="<!-- readmd:layout v1 {\"id\":\"p04-collision\",\"text\":{\"size\":72},\"paragraph\":{\"lineSpacing\":{\"mode\":\"exact\",\"pt\":6}}} -->\r\n\r\nFIRST  \r\nSECOND\r\n";
    let target=root.path().join("Protected.pdf");std::fs::write(&target,b"KEEP THE OLD PDF").unwrap();
    let value=json!({"frontMatter":{"mode":"none"}});
    let direct=crate::mdexport::export_pdf(original,"",target.to_str().unwrap(),&value,"Synthetic",root.path()).unwrap_err();
    fn detail(body:&Value){assert_eq!(body["error_code"],"publication_layout_geometry","{body}");
        assert_eq!(body["details"]["coordinateSpace"],"pdfParagraphLine","{body}");assert_eq!(body["details"]["blockId"],"p04-collision");
        assert_eq!(body["details"]["line"],2);assert_eq!(body["details"]["mode"],"exact");
        assert_eq!(body["details"]["requestedPt"],6.);assert!(body["details"]["requiredPt"].as_f64().unwrap()>6.);}
    detail(&crate::latex_compile::error_body(&direct));
    let (status,body)=crate::server::call_in_process(&app,"POST","/api/export",Some(&json!({
        "format":"pdf","content":original,"out_path":target,"options":value,"task_id":"p04-atomic-exact"
    })));
    assert_eq!(status,200,"{body}");detail(&body);
    assert_eq!(crate::cancel::cancel("p04-atomic-exact"),crate::cancel::CancelState::Unknown);
    let result=crate::mcp::call_tool(&app,"readmd_export_document",&json!({
        "markdown_content":original,"output_format":"pdf","output_path":target,"options":value,"overwrite":true,"confirm":true
    }));
    assert_eq!(result["isError"],true,"{result}");detail(&result["structuredContent"]);
    let preview=crate::export_preview::prepare_with_data("pdf",original,"",&value,"Synthetic",root.path(),&root.path().join("profile"));
    detail(&crate::latex_compile::error_body(&preview.err().unwrap()));
    let (status,body)=crate::server::call_in_process(&app,"POST","/api/export/preview",Some(&json!({
        "format":"pdf","content":original,"options":value,"suggestedName":"Synthetic"
    })));
    assert_eq!(status,200,"{body}");detail(&body);assert!(body.get("key").is_none());
    assert_eq!(std::fs::read(target).unwrap(),b"KEEP THE OLD PDF");
    assert!(!std::fs::read_dir(root.path()).unwrap().any(|entry|entry.unwrap().file_name().to_string_lossy().contains(".readmd-")));
}

#[test]
fn p04_rust_pdf_preset_legacy_context_survives_sparse_mode_and_cannot_forge_managed_consumer() {
    let mut override_value=options();override_value["footer"]=json!({"pageNumbers":false,"text":""});override_value["header"]=json!({"text":""});
    let inherited=crate::publication_typography::ResolvedOptions::from_request(&json!({"typography":{"lineHeight":1.55}})).unwrap()
        .merge(&override_value).unwrap();
    let style=crate::export_styles::sanitize(inherited.values());
    let blocks=crate::md_ast::to_publication_render_json(&crate::md_ast::parse("Body.\n\n> Legacy quote first.  \n> Legacy quote second.\n").blocks);
    let doc=crate::pdf_render::build_with_lines(&blocks,&style,"",&|_|None,&mut Vec::new(),inherited.line_context()).unwrap();
    let bytes=crate::pdf_render::write_pdf(&doc);let pdf=lopdf::Document::load_mem(&bytes).unwrap();
    let content=lopdf::content::Content::decode(&pdf.get_page_content(*pdf.get_pages().values().next().unwrap()).unwrap()).unwrap();
    let mut ys=Vec::new();for operation in content.operations {if operation.operator=="Tm"{
        let y=operation.operands[5].as_float().unwrap() as f64;
        if ys.last().is_none_or(|last:&f64|(*last-y).abs()>0.001){ys.push(y);}
    }}
    assert_eq!(ys.len(),3,"{ys:?}");assert!((ys[1]-ys[2]-11.*1.55).abs()<0.004,"{ys:?}");
    let copy=pdf.extract_text(&[1]).unwrap();assert!(copy.contains("Legacy quote first.")&&copy.contains("Legacy quote second."));
    let stories=crate::pdf_render::Styles::build(&style,"Helvetica");
    assert!(stories.body.line_spacing.is_none());
    for consumer in [crate::publication_typography::Consumer::Unsupported] {
        assert_eq!(crate::publication_typography::validate_consumer(consumer,"Body.",&json!({
            "pdf":{"engine":"rust"},"typography":{"lineSpacing":{"mode":"exact","pt":18}}
        })),Err(UNSUPPORTED));
    }
}

fn options() -> Value {
    json!({"frontMatter":{"mode":"none"},"typography":{"lineSpacing":{"mode":"exact","pt":18}}})
}

fn local_source() -> &'static str {
    "<!-- readmd:layout v1 {\"id\":\"p04\",\"paragraph\":{\"lineSpacing\":{\"mode\":\"atLeast\",\"pt\":20}}} -->\r\n\r\nSynthetic source paragraph.\r\n"
}

// Typst and TeX support ordinary body paragraphs. Explicit heading spacing is
// still outside that consumer and must fail before engine/resource access.
fn typst_unsupported_source() -> &'static str {
    "<!-- readmd:layout v1 {\"id\":\"p04-heading\",\"paragraph\":{\"lineSpacing\":{\"mode\":\"exact\",\"pt\":18}}} -->\n\n# Synthetic heading\n"
}

#[test]
fn p04_typst_supported_body_reaches_engine_gate_without_overwriting_output() {
    let root = tempfile::tempdir().unwrap();
    let app = app(root.path());
    let mut value = options(); value["pdf"] = json!({"engine":"typst"});
    let source = "FIRST  \nSECOND  \nTHIRD\n";
    assert_eq!(crate::publication_typography::validate_export("pdf", source, &value), Ok(()));
    let output = root.path().join("protected.pdf");
    std::fs::write(&output, b"OWNED EXISTING PDF").unwrap();
    let direct = crate::typst_compile::export_pdf(source, "", output.to_str().unwrap(), &value,
        "Synthetic", &app.paths.data_dir, &app.paths.assets_dir).unwrap_err();
    assert_eq!(direct, "typst_engine_unavailable");
    for route in ["/api/export", "/api/export/preview"] {
        let (status, body) = crate::server::call_in_process(&app, "POST", route, Some(&json!({
            "format":"pdf","content":source,"out_path":output,"options":value,
        })));
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["ok"], false);
        assert_eq!(body["error_code"], "typst_engine_unavailable", "{body}");
        assert!(body.get("key").is_none());
    }
    let result = crate::mcp::call_tool(&app, "readmd_export_document", &json!({
        "markdown_content":source,"output_format":"pdf","output_path":output,
        "options":value,"overwrite":true,"confirm":true,
    }));
    assert_eq!(result["isError"], true);
    assert_eq!(result["structuredContent"]["error_code"], "typst_engine_unavailable");
    assert_eq!(std::fs::read(output).unwrap(), b"OWNED EXISTING PDF");
}

fn app(root: &Path) -> Arc<crate::App> {
    let assets = root.join("assets");
    std::fs::create_dir_all(&assets).unwrap();
    Arc::new(crate::App::bootstrap(crate::paths::AppPaths::with_dirs(
        &root.join("profile"), root, &assets,
    )).unwrap())
}

fn children<'a>(node: &'a XmlNode, local: &str) -> Vec<&'a XmlNode> {
    let mut found = Vec::new();
    if node.local() == local { found.push(node); }
    for child in node.children() { found.extend(children(child, local)); }
    found
}

fn inspect_docx(path: &Path, body_line: &str, body_rule: &str, legacy_line: &str) {
    let parts = crate::publication_typography_docx_tests::unzip_bytes(&std::fs::read(path).unwrap());
    let styles = parse_xml(std::str::from_utf8(&parts["word/styles.xml"]).unwrap()).unwrap();
    let body = children(&styles, "style").into_iter().find(|node| {
        node.attributes.get("w:styleId").map(String::as_str) == Some("BodyText")
    }).unwrap();
    let spacing = children(body, "spacing");
    assert_eq!(spacing.len(), 1);
    assert_eq!(spacing[0].attributes["w:line"], body_line);
    assert_eq!(spacing[0].attributes["w:lineRule"], body_rule);
    assert_eq!(children(body, "snapToGrid")[0].attributes["w:val"], "0");
    let defaults = children(&styles, "docDefaults")[0];
    assert_eq!(children(defaults, "spacing")[0].attributes["w:line"], legacy_line);
    assert_eq!(children(defaults, "spacing")[0].attributes["w:lineRule"], "auto");
    let document = parse_xml(std::str::from_utf8(&parts["word/document.xml"]).unwrap()).unwrap();
    assert_eq!(document.text().matches("Synthetic source paragraph.").count(), 1);
    for data in parts.values() {
        let text = String::from_utf8_lossy(data);
        assert!(!text.contains("_publicationLineSpacingLegacy"));
        assert!(!text.contains("readmd:layout"));
    }
}

#[test]
fn p04_direct_unimplemented_exports_refuse_before_resources_and_preserve_old_files() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("not-created-profile");
    let assets = root.path().join("not-created-assets");
    let base = root.path().to_str().unwrap();
    for (source, typed) in [("Synthetic source paragraph.", options()),
        (local_source(), json!({"frontMatter":{"mode":"none"}}))] {
        for (format, engine) in [("html", "rust"), ("epub", "rust"), ("tex", "rust"),
            ("pdf", "typst"), ("pdf", "xelatex")] {
            let target = root.path().join(format!("protected-{format}-{engine}.{format}"));
            std::fs::write(&target, b"P04 ORIGINAL OUTPUT").unwrap();
            let mut value = typed.clone();
            value["pdf"] = json!({"engine":engine});
            // Missing bibliography must not be opened before capability refusal.
            value["bibliography"] = json!("missing-references.json");
            let source = if engine == "typst" || engine == "xelatex" || matches!(format,"tex"|"latex") { typst_unsupported_source() } else { source };
            let error = crate::mdexport::export_document_with_data(format, source, base,
                target.to_str().unwrap(), &value, "Synthetic", &assets, &data).unwrap_err();
            assert_eq!(error, UNSUPPORTED, "{format}/{engine}");
            assert_eq!(std::fs::read(&target).unwrap(), b"P04 ORIGINAL OUTPUT");
            assert!(!target.with_extension("assets").exists());
        }
    }
    assert!(!data.exists() && !assets.exists());
    assert!(!std::fs::read_dir(root.path()).unwrap().any(|item| {
        item.unwrap().file_name().to_string_lossy().contains("readmd-part")
    }));
}

#[test]
fn p04_public_format_entrypoints_cannot_bypass_new_value_guards() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("protected.output");
    std::fs::write(&target, b"UNCHANGED").unwrap();
    let value = options();
    let output = target.to_str().unwrap();
    let source = "Synthetic source paragraph.";
    let errors = [
        crate::mdexport::export_html(source, "", output, &value, "Synthetic", root.path()).unwrap_err(),
        crate::mdexport::export_epub(source, "", output, &value, "Synthetic").unwrap_err(),
        crate::mdexport::export_tex(typst_unsupported_source(), "", output, &value, "Synthetic").unwrap_err(),
        crate::typst_compile::export_pdf(typst_unsupported_source(), "", output, &value, "Synthetic", root.path(), root.path()).unwrap_err(),
        crate::latex_compile::export_pdf_with_data(typst_unsupported_source(), "", output, &value, "Synthetic", root.path(), root.path()).unwrap_err(),
    ];
    assert!(errors.iter().all(|error| error == UNSUPPORTED), "{errors:?}");
    assert_eq!(std::fs::read(&target).unwrap(), b"UNCHANGED");
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn p04_invalid_sparse_requests_fail_before_default_fill_or_docx_output() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("protected.docx");
    std::fs::write(&target, b"KEEP DOCX").unwrap();
    for value in [
        json!({"typography":{"lineSpacing":{"mode":"exact","pt":0}}}),
        json!({"typography":{"lineSpacing":{"mode":"exact","pt":18},"lineHeight":1.6}}),
        json!({"typography":{"lineSpacing":{"mode":"exact","pt":18,"multiple":1.6}}}),
        json!({"_publicationLineSpacingLegacy":1.55,"typography":{"lineSpacing":{"mode":"exact","pt":18}}}),
    ] {
        assert_eq!(crate::mdexport::export_docx("![missing](missing.png)", "", target.to_str().unwrap(),
            &value, "Synthetic").unwrap_err(), INVALID);
        assert_eq!(std::fs::read(&target).unwrap(), b"KEEP DOCX");
    }
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn p04_previews_reject_before_cache_keys_or_engine_leases_including_docx_projection() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("not-created-profile");
    let assets = root.path().join("not-created-assets");
    for (source, mut value) in [("Synthetic source paragraph.", options()),
        (local_source(), json!({"frontMatter":{"mode":"none"}}))] {
        for format in ["pdf", "docx"] {
            for engine in ["rust", "typst", "xelatex"] {
                value["pdf"] = json!({"engine":engine});
                let guarded_source = if format == "pdf" && matches!(engine,"typst"|"xelatex") { typst_unsupported_source() } else { source };
                let result=crate::export_preview::prepare_with_data(format,guarded_source,"",&value,"Synthetic",&assets,&data);
                if format=="pdf" && engine=="rust" {
                    let artifact=result.unwrap();
                    assert_eq!(artifact.projection_engine,"rust");
                    let pdf=lopdf::Document::load(&artifact.pdf).unwrap();
                    assert!(pdf.extract_text(&[1]).unwrap().contains("Synthetic source paragraph."));
                } else {assert_eq!(result.err().unwrap(),UNSUPPORTED);}
            }
        }
        assert_eq!(crate::epub_preview::prepare(source, "", &value, "Synthetic").unwrap_err(), UNSUPPORTED);
    }
    assert!(!data.exists() && !assets.exists());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn p04_http_export_and_preview_return_typed_errors_and_keep_existing_bytes() {
    let root = tempfile::tempdir().unwrap();
    let app = app(root.path());
    for (format, engine) in [("html", "rust"), ("epub", "rust"), ("tex", "rust"),
        ("pdf", "typst"), ("pdf", "xelatex")] {
        let target = root.path().join(format!("http-{format}-{engine}.{format}"));
        std::fs::write(&target, b"HTTP ORIGINAL").unwrap();
        let mut value = options(); value["pdf"] = json!({"engine":engine});
        let source = if engine == "typst" || engine == "xelatex" || matches!(format,"tex"|"latex") { typst_unsupported_source() } else { "Synthetic source paragraph." };
        let (status, body) = crate::server::call_in_process(&app, "POST", "/api/export", Some(&json!({
            "format":format,"content":source,"out_path":target,
            "options":value,"task_id":format!("p04-http-{format}-{engine}")
        })));
        assert_eq!(status, 200, "{body}"); assert_eq!(body["ok"], false);
        assert_eq!(body["error_code"], UNSUPPORTED, "{body}");
        assert_eq!(std::fs::read(&target).unwrap(), b"HTTP ORIGINAL");
        assert_eq!(crate::cancel::cancel(&format!("p04-http-{format}-{engine}")), crate::cancel::CancelState::Unknown);
    }
    for format in ["docx", "epub"] {
        let (status, body) = crate::server::call_in_process(&app, "POST", "/api/export/preview", Some(&json!({
            "format":format,"content":local_source(),"options":{},"suggestedName":"Synthetic"
        })));
        assert_eq!(status, 200, "{body}"); assert_eq!(body["error_code"], UNSUPPORTED);
        assert!(body.get("key").is_none());
    }
    let (status, body) = crate::server::call_in_process(&app, "POST", "/api/academic/document", Some(&json!({
        "content":local_source(),"options":{},"view":true
    })));
    assert_eq!(status, 422, "{body}"); assert_eq!(body["error_code"], UNSUPPORTED);
    assert_eq!(crate::publication_blocks::view(local_source()).unwrap_err(), UNSUPPORTED);
}

#[test]
fn p04_http_docx_writes_native_spacing_and_does_not_edit_source() {
    let root = tempfile::tempdir().unwrap();
    let app = app(root.path());
    let source = root.path().join("Source.md");
    let original = b"Synthetic source paragraph.\r\n";
    std::fs::write(&source, original).unwrap();
    let target = root.path().join("Actual.docx");
    let (status, body) = crate::server::call_in_process(&app, "POST", "/api/export", Some(&json!({
        "format":"docx","content":std::str::from_utf8(original).unwrap(),"baseDir":root.path(),
        "out_path":target,"options":options(),"suggestedName":"Synthetic"
    })));
    assert_eq!(status, 200, "{body}"); assert_eq!(body["ok"], true, "{body}");
    inspect_docx(&target, "360", "exact", "384");
    assert_eq!(std::fs::read(&source).unwrap(), original);
}

#[test]
fn p04_mcp_refusal_keeps_typed_code_and_never_replaces_old_target() {
    let root = tempfile::tempdir().unwrap();
    let app = app(root.path());
    for (format, engine) in [("html", "rust"), ("tex", "rust"),
        ("pdf", "typst"), ("pdf", "xelatex")] {
        let target = root.path().join(format!("mcp-{format}-{engine}.{format}"));
        std::fs::write(&target, b"MCP ORIGINAL").unwrap();
        let mut value = options(); value["pdf"] = json!({"engine":engine});
        let source = if engine == "typst" || engine == "xelatex" || matches!(format,"tex"|"latex") { typst_unsupported_source() } else { "Synthetic source paragraph." };
        let result = crate::mcp::call_tool(&app, "readmd_export_document", &json!({
            "markdown_content":source,"output_format":format,"output_path":target,
            "options":value,"overwrite":true,"confirm":true
        }));
        assert_eq!(result["isError"], true, "{result}");
        assert_eq!(result["structuredContent"]["error_code"], UNSUPPORTED, "{result}");
        assert_eq!(std::fs::read(&target).unwrap(), b"MCP ORIGINAL");
    }
    let target = root.path().join("invalid.docx"); std::fs::write(&target, b"MCP ORIGINAL").unwrap();
    for value in [json!({"typography":{"lineSpacing":{"mode":"exact","pt":18},"lineHeight":1.6}}),
        json!({"_publicationLineSpacingLegacy":1.55,"typography":{"lineSpacing":{"mode":"exact","pt":18}}})] {
        let result = crate::mcp::call_tool(&app, "readmd_export_document", &json!({
            "markdown_content":"Synthetic source paragraph.","output_format":"docx","output_path":target,
            "options":value,"overwrite":true,"confirm":true
        }));
        assert_eq!(result["isError"], true); assert_eq!(result["structuredContent"]["error_code"], INVALID, "{result}");
        assert_eq!(result["structuredContent"]["path"], "typography.lineSpacing");
        assert_eq!(std::fs::read(&target).unwrap(), b"MCP ORIGINAL");
    }
}

#[test]
fn p04_mcp_docx_sparse_override_preserves_preset_legacy_story_spacing() {
    let root = tempfile::tempdir().unwrap(); let app = app(root.path());
    let target = root.path().join("Actual-mcp.docx");
    let original = "\u{feff}Synthetic source paragraph.\r\n\r\n> Quoted legacy story.\r\n";
    let source = root.path().join("Source.md"); std::fs::write(&source, original).unwrap();
    let result = crate::mcp::call_tool(&app, "readmd_export_document", &json!({
        "markdown_content":original,"output_format":"docx","output_path":target,"style_preset":"minimal",
        "options":{"frontMatter":{"mode":"none"},"typography":{"lineSpacing":{"mode":"exact","pt":18}},"meta":{"clear":["publisher"]}},
        "overwrite":false,"confirm":true
    }));
    assert_ne!(result["isError"], true, "{result}"); assert_eq!(result["structuredContent"]["ok"], true, "{result}");
    inspect_docx(&target, "360", "exact", "372");
    assert_eq!(std::fs::read_to_string(&source).unwrap(), original);
}

#[test]
fn p04_preset_validation_precedes_sanitize_and_persistence() {
    let root = tempfile::tempdir().unwrap(); let app = app(root.path());
    let valid = json!({"custom":{"P04 native":{"frontMatter":{"mode":"none"},"typography":{"lineSpacing":{"mode":"atLeast","pt":20}},"meta":{"publisher":"Explicit"}}}});
    let (status, body) = crate::server::call_in_process(&app, "POST", "/api/export/presets", Some(&valid));
    assert_eq!(status, 200, "{body}"); assert_eq!(body["ok"], true);
    let persisted = app.paths.data_dir.join("export_presets.json"); let before = std::fs::read(&persisted).unwrap();
    let (status, body) = crate::server::call_in_process(&app, "GET", "/api/export/presets", None);
    assert_eq!(status, 200); let typed = &body["custom"]["P04 native"]["typography"];
    assert_eq!(typed["lineSpacing"], json!({"mode":"atLeast","pt":20}));
    assert!(typed.get("lineHeight").is_none()); assert!(!body.to_string().contains("_publicationLineSpacingLegacy"));
    for invalid in [
        json!({"custom":{"P04 native":{"typography":{"lineHeight":1.6,"lineSpacing":{"mode":"exact","pt":18}}}}}),
        json!({"last":{"fmt":"docx","options":{"typography":{"lineHeight":1.6,"lineSpacing":{"mode":"exact","pt":18}}}}}),
        json!({"custom":{"P04 native":{"_publicationLineSpacingLegacy":1.55}}}),
    ] {
        let (status, body) = crate::server::call_in_process(&app, "POST", "/api/export/presets", Some(&invalid));
        assert_eq!(status, 422, "{body}"); assert_eq!(body["error_code"], INVALID);
        assert_eq!(std::fs::read(&persisted).unwrap(), before);
    }
}
