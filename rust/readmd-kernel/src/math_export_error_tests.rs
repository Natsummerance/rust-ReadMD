//! Actual shared HTTP/MCP export-entry controls; no managed engine or mock API.
use crate::{server, App};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc};

fn app(root: &Path) -> Arc<App> {
    let data = root.join("profile");
    let workspace = root.join("documents");
    let assets = root.join("assets");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&assets).unwrap();
    Arc::new(App::bootstrap(crate::paths::AppPaths::with_dirs(&data, &workspace, &assets)).unwrap())
}
fn options() -> Value {
    json!({"pdf":{"engine":"rust"},"typography":{"size":12},"headings":{"h1":{"size":20}},
        "cover":{"enabled":false},"toc":{"enabled":false},"footer":{"pageNumbers":false}})
}
fn http(app: &Arc<App>, route: &str, body: Value) -> Value {
    let (status, value) = server::call_in_process(app, "POST", route, Some(&body));
    assert_eq!(status, 200, "{route}: {value}");
    value
}
fn assert_error(value: &Value, code: &str) {
    assert_eq!(value["ok"], false, "{value}");
    assert_eq!(value["error_code"], code, "{value}");
}

#[test]
fn math_export_error_exact_codes_and_details_do_not_classify_raw_messages() {
    for code in ["math_budget", "math_invalid_xml", "math_unsupported_extension"] {
        assert_eq!(super::error_body(code), json!({"ok":false,"error_code":code,"details":""}));
        assert_eq!(super::error_body(&format!("{code}: token 7")), json!({"ok":false,"error_code":code,"details":"token 7"}));
    }
    for raw in [
        "math_budgetary", "math_budget/private.tex", "math_budget\nprivate", " math_budget",
        "MATH_BUDGET", "math_budget = engine text", "math_not_a_registered_code",
        "Unable to read math_budget: private/path", "math_unsupported_extension_extra",
    ] {
        assert_eq!(super::error_body(raw), json!({"ok":false,"error":raw}));
    }
}

#[test]
fn math_export_error_http_and_pdf_preview_keep_budget_and_protect_owned_target() {
    let root = tempfile::tempdir().unwrap();
    let app = app(root.path());
    let output = app.paths.workspace.join("protected.pdf");
    std::fs::write(&output, b"OWNED EXISTING PDF").unwrap();
    // Actual H1 is 20pt, so 1100em = 22000pt exceeds the 20000pt scalar limit.
    let source = "# :math[space(width: \"1100em\")]\n";
    let args = json!({"format":"pdf","content":source,"baseDir":app.paths.workspace,
        "out_path":output,"suggestedName":"Synthetic budget","options":options()});
    assert_error(&http(&app, "/api/export", args.clone()), "math_budget");
    assert_eq!(std::fs::read(&output).unwrap(), b"OWNED EXISTING PDF");
    let key = crate::export_preview::key_with_data("pdf", source,
        app.paths.workspace.to_str().unwrap(), &options(), "Synthetic budget",
        &app.paths.data_dir, &app.paths.assets_dir);
    assert!(crate::export_preview::cached(&key).is_none());
    assert_error(&http(&app, "/api/export/preview", args), "math_budget");
    assert!(crate::export_preview::cached(&key).is_none(), "A failed formula cannot create a usable preview artifact");
    assert_eq!(std::fs::read(&output).unwrap(), b"OWNED EXISTING PDF");
}

#[test]
fn math_export_error_http_office_and_tex_and_docx_preview_reject_spacing() {
    let root = tempfile::tempdir().unwrap();
    let app = app(root.path());
    let source = ":math[row(a, space(width: \"1em\", height: \"2pt\"), b)]\n";
    for format in ["docx", "tex"] {
        let output = app.paths.workspace.join(format!("protected.{format}"));
        std::fs::write(&output, b"OWNED ORIGINAL").unwrap();
        let args = json!({"format":format,"content":source,"baseDir":app.paths.workspace,
            "out_path":output,"options":options()});
        assert_error(&http(&app, "/api/export", args.clone()), "math_unsupported_extension");
        assert_eq!(std::fs::read(&output).unwrap(), b"OWNED ORIGINAL");
        if format == "docx" {
            assert_error(&http(&app, "/api/export/preview", args), "math_unsupported_extension");
            assert_eq!(std::fs::read(&output).unwrap(), b"OWNED ORIGINAL");
        }
    }
}

#[test]
fn math_export_error_mcp_budget_and_office_rejections_keep_typed_content() {
    let root = tempfile::tempdir().unwrap();
    let app = app(root.path());
    for (format, source, code) in [
        ("pdf", "# :math[space(width: \"1100em\")]\n", "math_budget"),
        ("docx", ":math[space(width: \"1em\")]\n", "math_unsupported_extension"),
        ("tex", ":math[space(width: \"1em\")]\n", "math_unsupported_extension"),
    ] {
        let output = app.paths.workspace.join(format!("protected.{format}"));
        std::fs::write(&output, b"OWNED ORIGINAL").unwrap();
        let result = crate::mcp::call_tool(&app, "readmd_export_document", &json!({
            "markdown_content":source,"output_path":output,"output_format":format,
            "base_dir":app.paths.workspace,"options":options(),"overwrite":true,"confirm":true,
        }));
        assert_eq!(result["isError"], true, "{result}");
        assert_error(&result["structuredContent"], code);
        let text: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_error(&text, code);
        assert_eq!(std::fs::read(&output).unwrap(), b"OWNED ORIGINAL");
    }
}

#[test]
fn math_export_error_direct_math_http_keeps_its_existing_422_code() {
    let root = tempfile::tempdir().unwrap();
    let app = app(root.path());
    let (status, value) = server::call_in_process(&app, "POST", "/api/math/parse", Some(&json!({
        "source":"<math><mspace width='1000em'/></math>","dialect":"mathml","font_size":24,
    })));
    assert_eq!(status, 422);
    assert_error(&value, "math_budget");
}
