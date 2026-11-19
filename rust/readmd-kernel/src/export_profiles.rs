//! One built-in catalog for the desktop, API and external clients. Profiles
//! contain output parameters, not canned example documents or engine claims.
use serde_json::Value;
use std::sync::OnceLock;
pub fn catalog() -> &'static [Value] {
    static CATALOG: OnceLock<Vec<Value>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        ["../../../assets/data/export-profiles.json", "../../../assets/data/export-templates.json"]
            .into_iter()
            .zip([
                include_str!("../../../assets/data/export-profiles.json"),
                include_str!("../../../assets/data/export-templates.json"),
            ])
            .flat_map(|(path, text)| {
                let mut parsed: Value = serde_json::from_str(text).unwrap_or_else(|_| panic!("built-in export catalog {path}"));
                parsed["profiles"].take().as_array().expect("export profiles").clone()
            }).collect()
    })
}
pub fn preset(id: &str) -> Option<Value> {
    crate::export_styles::preset(id).or_else(|| {
        catalog()
            .iter()
            .find(|p| p["id"] == id)
            .and_then(|p| p.get("options"))
            .cloned()
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builtins_have_unique_names_supported_parameters_and_reusable_formats() {
        let mut ids = std::collections::HashSet::new();
        for p in catalog() {
            let id = p["id"].as_str().unwrap();
            assert!(ids.insert(id));
            let options = preset(id).unwrap();
            let sanitized = crate::export_styles::sanitize(&options);
            for (key, value) in options.as_object().unwrap() {
                assert!(sanitized.get(key).is_some());
                if key == "epub" {
                    assert_eq!(&sanitized[key], value);
                }
            }
            let formats = p["formats"].as_array().unwrap();
            assert!(!formats.is_empty());
            assert!(formats.iter().all(|value| matches!(value.as_str(),
                Some("pdf" | "docx" | "epub" | "html" | "tex" | "pptx" | "png" | "png-pages"))));
        }
        assert_eq!(
            catalog().iter().filter(|p| p["category"] == "book").count(),
            5
        );
        assert_eq!(
            catalog()
                .iter()
                .filter(|p| p["category"] == "academic")
                .count(),
            5
        );
        assert_eq!(
            preset("book_novel").unwrap()["headings"]["h1"]["pageBreakBefore"],
            true
        );
    }

    #[test]
    fn book_and_common_profiles_change_actual_word_pdf_and_epub_output() {
        let source="---\ntitle: Evidence workshop\n---\n# One\n\nA substantial paragraph with evidence.\n\n| Topic | Result |\n|---|---|\n| Inquiry | Recheck |\n\n```rust\nlet evidence = 42;\n```\n\n# Two\n\nA second chapter.\n";
        let dir = tempfile::tempdir().unwrap();
        for p in catalog() {
            if !["common", "book", "academic"].contains(&p["category"].as_str().unwrap_or("")) { continue; }
            let id = p["id"].as_str().unwrap();
            let options = preset(id).unwrap();
            let style = crate::export_styles::sanitize(&options);
            let doc = crate::md_ast::parse(source);
            let word = crate::docx_writer::build(&doc, &style, "Workshop", &|_| None);
            let xml = word
                .parts
                .iter()
                .find(|p| p.0 == "word/document.xml")
                .unwrap();
            let xml = String::from_utf8_lossy(&xml.1);
            assert!(xml.contains("A second chapter."), "{id}");
            let pdf = dir.path().join(format!("{id}.pdf"));
            crate::mdexport::export_pdf(
                source,
                "",
                pdf.to_str().unwrap(),
                &options,
                "Workshop",
                dir.path(),
            )
            .unwrap();
            let parsed = lopdf::Document::load(&pdf).unwrap();
            let pages = parsed.get_pages();
            let first = pages.values().next().unwrap();
            let box_ = parsed
                .get_object(*first)
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"MediaBox")
                .unwrap()
                .as_array()
                .unwrap();
            let (width, height) = crate::export_styles::page_dimensions(&style);
            let number = |v: &lopdf::Object| {
                v.as_float()
                    .map(|v| v as f64)
                    .or_else(|_| v.as_i64().map(|v| v as f64))
                    .unwrap()
            };
            assert!(
                (number(&box_[2]) - width * crate::pdf_render::MM).abs() < 0.1
                    && (number(&box_[3]) - height * crate::pdf_render::MM).abs() < 0.1,
                "{id}"
            );
            if style["cover"]["enabled"] == true {
                assert!(
                    crate::pdf_text::extract(&std::fs::read(&pdf).unwrap())
                        .unwrap()
                        .contains("Evidence workshop"),
                    "{id}"
                );
            }
            if p["category"] != "academic" {
                let (entries, _) = crate::mdexport::epub_build_entries(
                    &serde_json::json!(source),
                    "",
                    &options,
                    "Workshop",
                    "Authors",
                    "en",
                    "test",
                    "2026-10-09T00:00:00Z",
                )
                .unwrap();
                let css = String::from_utf8_lossy(
                    &entries
                        .iter()
                        .find(|e| e.path == "OEBPS/style.css")
                        .unwrap()
                        .data,
                );
                assert!(
                    css.contains(style["typography"]["color"].as_str().unwrap()),
                    "{id}"
                );
                assert!(
                    css.contains(&format!(
                        "text-indent:{}mm",
                        style["typography"]["firstLineIndent"].as_f64().unwrap()
                    )),
                    "{id}"
                );
                if p["category"] == "book" {
                    assert!(entries.iter().any(|e| e.path == "OEBPS/cover.xhtml"));
                }
            }
        }
    }
}
