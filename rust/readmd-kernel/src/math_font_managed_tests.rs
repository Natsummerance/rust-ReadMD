//! Document-level font diagnostics describe the actual bundled face. A
//! catalogue alias does not make Cambria Math the STIX asset that is painted.
use super::font_warnings;

fn formula(math_font: Option<&str>) -> String {
    let settings = math_font
        .map(|name| format!(r#"<m:mathPr><m:mathFont m:val="{name}"/></m:mathPr>"#))
        .unwrap_or_default();
    format!("{settings}<m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>")
}
fn fallbacks(math_font: &str) -> Vec<serde_json::Value> {
    let parsed = crate::omml::parse(&formula(Some(math_font))).unwrap();
    font_warnings(&parsed)
        .into_iter()
        .filter(|warning| warning["code"] == "font_fallback")
        .collect()
}
fn ink(math_font: Option<&str>) -> String {
    let xml = formula(math_font);
    super::layout_source("x", Some(&xml), false, 12.0)
        .expect("bundled math font lays out")
        .draw(0.0, 0.0, "0 0 0")
}

#[test]
fn bundled_math_font_is_accepted_without_fallback_diagnostics() {
    assert!(fallbacks("STIX Two Math").is_empty());
    assert_eq!(ink(Some("STIX Two Math")), ink(None));
}

#[test]
fn substituted_math_font_names_including_aliases_report_one_real_fallback() {
    for name in ["Cambria Math", "Foo Math", "Times New Roman"] {
        let warnings = fallbacks(name);
        assert_eq!(warnings.len(), 1, "{name}: {warnings:?}");
        assert_eq!(warnings[0]["font"], name, "the diagnostic keeps the requested spelling");
        assert_eq!(warnings[0]["actual"], "STIX Two Math", "actual is the bundled math face");
        // The run-level predicate reports the mapped text family; the
        // document-level math font never does.
        assert_ne!(warnings[0]["actual"], "ReadMD Serif SC");
        // The fallback is real behaviour: the ink still equals the default
        // formula, so the warning neither lies about adoption nor about loss.
        assert_eq!(ink(Some(name)), ink(None), "ink must match the default formula for {name}");
    }
}

#[test]
fn math_font_defaults_conflict_and_settings_retention_kept_despite_warning_policy() {
    let named = formula(Some("Cambria Math"));
    let bare = formula(None);
    let mut merger = crate::math_defaults::DocxDefaultsMerge::default();
    let content = merger.accept(&named, false).unwrap();
    assert!(content.contains("oMath"), "{content}");
    // The accepted settings keep the original spelling for word/settings.xml.
    assert!(merger.settings().expect("settings").xml().contains(r#"m:val="Cambria Math""#));
    assert_eq!(
        merger.accept(&bare, false).unwrap_err(),
        crate::math_defaults::DefaultsError::Conflict { property: "mathFont", first_formula: 1, formula: 2 }
    );
}
