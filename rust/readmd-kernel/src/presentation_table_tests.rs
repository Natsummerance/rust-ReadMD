use serde_json::{Value, json};
use std::{fs, path::PathBuf, sync::Arc};

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

const SOURCE: &str = concat!(
    "# Research\n\n",
    "| Claim {width=240} | Reference {width=360} | Expression {width=300} |\n",
    "|---|---|---|\n",
    "| **Origin** {colspan=2 rowspan=2 align=center valign=bottom height=125} | [Source][evidence] | $x+y$ |\n",
    "| *Next row* {height=90} | \x60<literal>&\x60 | ![Pixel](sample.png \"Pixel title\") |\n\n",
    "[evidence]: https://example.invalid/result?a=1&b=2\n",
);

fn slides(html: &str) -> &str {
    html.split_once("<div class=\"slides\">")
        .unwrap()
        .1
        .split_once("</textarea>")
        .unwrap()
        .0
}

#[test]
fn presentation_table_preview_and_export_share_merged_tracks_and_inline_rich_content() {
    let root = tempfile::tempdir().unwrap();
    let base = dunce::canonicalize(root.path()).unwrap();
    let profile = base.join("profile");
    let fixture = base.join("sample.png");
    use base64::Engine;
    fs::write(&fixture, base64::engine::general_purpose::STANDARD.decode(
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/hXsAAAAASUVORK5CYII="
    ).unwrap()).unwrap();
    let app = Arc::new(
        crate::App::bootstrap(crate::paths::AppPaths::with_dirs(
            &profile,
            &base,
            &assets(),
        ))
        .unwrap(),
    );
    let options = json!({"theme":"white","transition":"none","table":{"style":"three-line","borderWidth":1.0,"borderColor":"#227744"}});
    let preview_request = json!({"content":SOURCE,"format":"presentation","baseDir":base,"suggestedName":"Research","options":options});
    let (status, preview) =
        crate::server::call_in_process(&app, "POST", "/api/export/preview", Some(&preview_request));
    assert_eq!(status, 200, "{preview}");
    assert_eq!(preview["ok"], true, "{preview}");
    let preview = preview["html"].as_str().unwrap();
    let target = base.join("research.html");
    let mut payload = preview_request.clone();
    payload["out_path"] = json!(target);
    let (status, saved) =
        crate::server::call_in_process(&app, "POST", "/api/export", Some(&payload));
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["ok"], true, "{saved}");
    let exported = fs::read_to_string(&target).unwrap();
    let body = slides(preview);
    assert_eq!(body, slides(&exported));
    for html in [preview, &exported] {
        assert!(html.contains(".reveal .slides thead th{border-bottom:1pt solid #227744}"));
        assert!(html.contains("overflow-y: auto !important"));
        assert!(html.contains("section.readmd-table-slide>*{flex-shrink:0}"));
    }
    for needle in [
        "<colgroup>",
        "width: 26.6667%",
        "width: 40.0000%",
        "width: 33.3333%",
        "<tr height=\"125\">",
        "<tr height=\"90\">",
        "colspan=\"2\" rowspan=\"2\" align=\"center\" valign=\"bottom\"",
        "<strong>Origin</strong>",
        "<em>Next row</em>",
        "<code>&amp;lt;literal&amp;gt;&amp;amp;</code>",
        "href=\"https://example.invalid/result?a=1&amp;amp;b=2\"",
        "$x+y$",
        "<img src=\"data:image/png;base64,",
        "alt=\"Pixel\"",
        "title=\"Pixel title\"",
    ] {
        assert!(body.contains(needle), "{needle}\n{body}");
    }
    let indices =
        ["Origin", ">Source<", "Next row", "literal"].map(|needle| body.find(needle).unwrap());
    assert!(indices.windows(2).all(|n| n[0] < n[1]), "{body}");
    assert!(!body.contains("{width="));
    assert!(!body.contains("{colspan="));
    assert_eq!(fs::read_dir(&base).unwrap().count(), 3);
}

#[test]
fn presentation_table_larger_than_one_slide_remains_whole_and_scrolls_from_the_top() {
    let mut markdown = "| Data {width=240} | Value {width=360} |\n|---|---|\n".to_owned();
    for row in 0..12 {
        markdown += &format!("| Row {row:02} {{height=100}} | Value {row:02} |\n");
    }
    let html = crate::mdexport::render_presentation_html(
        &markdown,
        "Long table",
        "black",
        "none",
        false,
        &assets(),
    )
    .unwrap();
    assert_eq!(html.matches("<textarea data-template>").count(), 1);
    let body = slides(&html);
    assert!(body.contains("class=\"readmd-table-slide\""));
    assert_eq!(body.matches("<tr height=\"100\">").count(), 12);
    assert!(body.find("Row 00").unwrap() < body.find("Row 11").unwrap());
    assert!(body.contains("width:600.0000px"));
    assert!(html.contains("justify-content:safe center!important"));
}

#[test]
fn presentation_table_invalid_geometry_preserves_existing_output_and_stays_an_explicit_preview_error()
 {
    let root = tempfile::tempdir().unwrap();
    let base = dunce::canonicalize(root.path()).unwrap();
    let app = Arc::new(
        crate::App::bootstrap(crate::paths::AppPaths::with_dirs(
            &base.join("profile"),
            &base,
            &assets(),
        ))
        .unwrap(),
    );
    let file = base.join("protected.html");
    fs::write(&file, b"existing user output").unwrap();
    let content = SOURCE.replace("colspan=2", "colspan=7");
    let payload = json!({"format":"presentation","content":content,"out_path":file,"options":{"table":{"style":"three-line"}}});
    for route in [
        "/api/export",
        "/api/export/preview",
        "/api/export/presentation",
    ] {
        let (status, result) = crate::server::call_in_process(&app, "POST", route, Some(&payload));
        assert!(matches!(status, 200 | 422), "{route}: {result}");
        assert_eq!(result["ok"], false, "{route}: {result}");
        assert_eq!(
            result["error_code"],
            crate::api_codes::PUBLICATION_TABLE_UNSUPPORTED,
            "{route}: {result}"
        );
        assert_eq!(fs::read(&file).unwrap(), b"existing user output");
    }
}

#[test]
fn presentation_table_preserves_raw_html_spans_without_activating_cell_html() {
    let source = concat!(
        "<table><colgroup><col style=\"width: 120px\" /></colgroup>",
        "<thead><tr><th>Head</th></tr></thead><tbody><tr><td rowspan=\"2\">Merged</td></tr>",
        "<tr></tr></tbody></table>\n\n",
        "| Header {width=100} | Reference |\n",
        "|---|---|\n",
        "| <em onmouseover=\"example()\">Emphasis</em> {colspan=2} | [unsafe](javascript:example) |\n",
    );
    let html = crate::mdexport::render_styled_presentation_html(
        source,
        "Raw",
        "black",
        "none",
        false,
        &assets(),
        &Value::Null,
    )
    .unwrap();
    let body = slides(&html);
    assert!(body.contains("<colgroup><col style=\"width: 120px\" /></colgroup>"));
    assert!(body.contains("rowspan=\"2\">Merged"));
    assert!(body.contains("colspan=\"2\"><em>Emphasis</em>"));
    assert!(body.contains("<a>unsafe</a>"));
    assert!(!body.contains("onmouseover=") && !body.contains("href=\"javascript:"));
}
