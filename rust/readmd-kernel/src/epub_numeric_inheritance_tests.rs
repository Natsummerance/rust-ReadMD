//! Sparse EPUB numeric input through the real CSS, package and preview paths.
use super::{numeric_error_field, numeric_profile, validate_numeric_request, NumericProfile, NUMERIC_INVALID};
use serde_json::{json, Value};

const SOURCE: &str = "---\ntitle: Numeric inheritance\nlanguage: en-US\n---\n# Opening\n\nFIRST_BODY remains readable.[^n]\n\n## Details\n\nA second paragraph.\n\n# Ending\n\nLAST_BODY remains unchanged.\n\n[^n]: Original note.\n";

fn entries(options: &Value) -> Result<Vec<crate::mdexport::ZipEntry>, String> {
    crate::mdexport::epub_build_entries(&json!(SOURCE), "", options, "Numeric inheritance", "Author", "zh-CN", "numeric-inherit", "2026-01-01T00:00:00Z").map(|value| value.0)
}
fn text<'a>(entries: &'a [crate::mdexport::ZipEntry], name: &str) -> &'a str {
    std::str::from_utf8(&entries.iter().find(|entry| entry.path == name).unwrap().data).unwrap()
}

#[test]
fn absent_and_null_values_inherit_without_materializing_the_sparse_input() {
    for epub in [json!({}), json!({"fontSize":null,"lineHeight":null,"marginV":null,"marginH":null})] {
        let options = json!({"typography":{"size":10.5,"lineHeight":1.55},"epub":epub});
        let original = options.clone();
        assert_eq!(numeric_profile(&options).unwrap(), NumericProfile {font_size:10.5,line_height:1.55,margin_v:5.0,margin_h:8.0});
        let css = crate::mdexport::build_epub_css(&options).unwrap();
        for expected in ["font-size: 10.5pt", "line-height: 1.55", "margin: 5% 8%"] { assert!(css.contains(expected), "{css}"); }
        entries(&options).unwrap();
        assert_eq!(options, original);
    }
    let defaults = crate::export_styles::default_style();
    let resolved = numeric_profile(&json!({})).unwrap();
    assert_eq!(resolved.font_size, defaults["typography"]["size"].as_f64().unwrap());
    assert_eq!(resolved.line_height, defaults["typography"]["lineHeight"].as_f64().unwrap());
}

#[test]
fn zero_margins_are_explicit_and_independent_of_the_other_axis() {
    for (epub, expected) in [
        (json!({"marginV":0}), "margin: 0% 8%"),
        (json!({"marginH":0}), "margin: 5% 0%"),
        (json!({"marginV":0,"marginH":0}), "margin: 0% 0%"),
        (json!({"marginV":0,"marginH":7.25}), "margin: 0% 7.25%"),
    ] {
        let options = json!({"epub":epub});
        assert!(crate::mdexport::build_epub_css(&options).unwrap().contains(expected));
        let built = entries(&options).unwrap();
        assert!(text(&built,"OEBPS/style.css").contains(expected));
    }
}

#[test]
fn explicit_type_and_line_height_keep_valid_api_boundaries_and_clear_to_body() {
    for (font, line, margin) in [(6.0,1.0,0.0),(72.0,5.0,40.0),(61.0,4.5,23.0),(14.25,1.75,6.5)] {
        let mut options = json!({"typography":{"size":10.5,"lineHeight":1.55},"epub":{"fontSize":font,"lineHeight":line,"marginV":margin,"marginH":margin}});
        assert_eq!(numeric_profile(&options).unwrap().font_size,font);
        assert_eq!(numeric_profile(&options).unwrap().line_height,line);
        entries(&options).unwrap();
        for key in ["fontSize","lineHeight","marginV","marginH"] { options["epub"].as_object_mut().unwrap().remove(key); }
        assert_eq!(numeric_profile(&options).unwrap(),NumericProfile{font_size:10.5,line_height:1.55,margin_v:5.0,margin_h:8.0});
        assert_eq!(options["epub"],json!({}));
    }
}

#[test]
fn real_package_and_preview_agree_on_inherited_metrics_language_and_source() {
    let dir = tempfile::tempdir().unwrap();
    let options = json!({"typography":{"size":10.5,"lineHeight":1.55},"epub":{"marginV":0,"marginH":0}});
    let built = entries(&options).unwrap();
    let css = text(&built,"OEBPS/style.css");
    assert!(css.contains("font-size: 10.5pt")); assert!(css.contains("line-height: 1.55")); assert!(css.contains("margin: 0% 0%"));
    assert!(text(&built,"OEBPS/content.opf").contains("<dc:language>en-US</dc:language>"));
    for name in ["OEBPS/nav.xhtml","OEBPS/chapter_1.xhtml","OEBPS/chapter_2.xhtml"] {
        let xhtml=text(&built,name); assert!(xhtml.contains("lang=\"en-US\"")); assert!(xhtml.contains("xml:lang=\"en-US\""));
    }
    let preview=crate::epub_preview::prepare(SOURCE,dir.path().to_str().unwrap(),&options,"Numeric inheritance").unwrap();
    let html=preview["html"].as_str().unwrap();
    for expected in [css,"FIRST_BODY","LAST_BODY","lang=\"en-US\""] { assert!(html.contains(expected)); }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),0);
}

#[test]
fn invalid_values_fail_all_publication_paths_before_replacing_an_old_target() {
    let dir=tempfile::tempdir().unwrap(); let target=dir.path().join("protected.epub");
    std::fs::write(&target,b"ORIGINAL EPUB TARGET").unwrap();
    for (key,bad) in [
        ("fontSize",json!(0)),("fontSize",json!(-1)),("fontSize",json!(72.01)),("fontSize",json!("14")),
        ("lineHeight",json!(0)),("lineHeight",json!(0.99)),("lineHeight",json!(5.01)),("lineHeight",json!(true)),
        ("marginV",json!(-0.1)),("marginH",json!(40.01)),("marginH",json!("0")),("marginV",json!([])),
        ("fontSize",json!({})),("lineHeight",json!("")),
    ] {
        let mut options=json!({"epub":{}}); options["epub"][key]=bad;
        assert_eq!(validate_numeric_request(&options),Err(NUMERIC_INVALID));
        assert_eq!(numeric_error_field(&options),format!("epub.{key}"));
        assert_eq!(crate::mdexport::build_epub_css(&options).unwrap_err(),NUMERIC_INVALID);
        assert_eq!(entries(&options).err().as_deref(),Some(NUMERIC_INVALID));
        assert_eq!(crate::epub_preview::prepare(SOURCE,dir.path().to_str().unwrap(),&options,"Book").unwrap_err(),NUMERIC_INVALID);
        assert_eq!(crate::mdexport::export_epub(SOURCE,dir.path().to_str().unwrap(),target.to_str().unwrap(),&options,"Book").unwrap_err(),NUMERIC_INVALID);
        assert_eq!(std::fs::read(&target).unwrap(),b"ORIGINAL EPUB TARGET");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),1);
    }
    for bad in [json!(false),json!(""),json!([])] { assert_eq!(validate_numeric_request(&json!({"epub":bad})),Err(NUMERIC_INVALID)); }
}
