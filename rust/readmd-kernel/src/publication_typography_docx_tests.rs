//! Isolated draft tests: actual OPC ZIP parts and source transactions.
//! These have not been compiled/run and do not certify Word pagination.
use crate::math_document::{parse_xml, XmlNode};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Read;

fn package(source: &str, options: &Value) -> crate::docx_writer::DocxPackage {
    crate::docx_writer::build(&crate::md_ast::parse(source), options, "P04 synthetic", &|_| None)
}

fn unzip(package: crate::docx_writer::DocxPackage) -> BTreeMap<String, Vec<u8>> {
    assert!(!package.parts.is_empty(), "{:?}", package.warns);
    let entries = package.parts.into_iter().map(|(path, data)| crate::mdexport::ZipEntry { path, data, compress: true }).collect::<Vec<_>>();
    let bytes = crate::mdexport::write_zip_at(&entries, 0, 0).unwrap();
    unzip_bytes(&bytes)
}

pub(crate) fn unzip_bytes(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let u16at = |i| u16::from_le_bytes(bytes[i..i + 2].try_into().unwrap());
    let u32at = |i| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
    let mut at = 0; let mut result = BTreeMap::new();
    while bytes.get(at..at + 4) == Some(&b"PK\x03\x04"[..]) {
        assert_eq!(u16at(at + 6) & 8, 0);
        let method = u16at(at + 8); let packed = u32at(at + 18) as usize; let size = u32at(at + 22) as usize;
        let name_len = u16at(at + 26) as usize; let extra_len = u16at(at + 28) as usize;
        let name = std::str::from_utf8(&bytes[at + 30..at + 30 + name_len]).unwrap().to_string();
        let data_at = at + 30 + name_len + extra_len; let data = &bytes[data_at..data_at + packed];
        let decoded = if method == 8 { let mut out = Vec::new(); flate2::read::DeflateDecoder::new(data).read_to_end(&mut out).unwrap(); out }
            else { assert_eq!(method, 0); data.to_vec() };
        assert_eq!(decoded.len(), size);
        let mut crc = flate2::Crc::new(); crc.update(&decoded); assert_eq!(crc.sum(), u32at(at + 14));
        assert!(result.insert(name, decoded).is_none()); at = data_at + packed;
    }
    assert_eq!(bytes.get(at..at + 4), Some(&b"PK\x01\x02"[..]));
    assert_eq!(bytes.get(bytes.len() - 22..bytes.len() - 18), Some(&b"PK\x05\x06"[..]));
    for (name, data) in &result {
        if name.ends_with(".xml") || name.ends_with(".rels") { parse_xml(std::str::from_utf8(data).unwrap()).unwrap(); }
    }
    result
}

fn xml(parts: &BTreeMap<String, Vec<u8>>, name: &str) -> XmlNode {
    parse_xml(std::str::from_utf8(&parts[name]).unwrap()).unwrap()
}
fn descendants<'a>(node: &'a XmlNode, local: &str) -> Vec<&'a XmlNode> {
    let mut out = Vec::new(); if node.local() == local { out.push(node); }
    for child in node.children() { out.extend(descendants(child, local)); } out
}
fn style<'a>(root: &'a XmlNode, id: &str) -> &'a XmlNode {
    descendants(root, "style").into_iter().find(|n| n.attributes.get("w:styleId").map(String::as_str) == Some(id)).unwrap()
}
fn paragraph<'a>(root: &'a XmlNode, value: &str) -> &'a XmlNode {
    let found = descendants(root, "p").into_iter().filter(|p| p.text() == value).collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "{value}"); found[0]
}
fn line(node: &XmlNode, value: &str, rule: &str) {
    let ppr = node.children().find(|n| n.local() == "pPr").unwrap();
    let spacing = ppr.children().filter(|n| n.local() == "spacing").collect::<Vec<_>>();
    assert_eq!(spacing.len(), 1); assert_eq!(spacing[0].attributes["w:line"], value);
    assert_eq!(spacing[0].attributes["w:lineRule"], rule);
    assert_eq!(ppr.children().find(|n| n.local() == "snapToGrid").unwrap().attributes["w:val"], "0");
}
fn declaration(id: &str, value: Value) -> String {
    format!("<!-- readmd:layout v1 {} -->\n\n", json!({"id":id,"paragraph":{"lineSpacing":value}}))
}

#[test]
fn p04_docx_global_exact_targets_body_style_without_changing_special_story_defaults() {
    let options = json!({"frontMatter":{"mode":"none"},"typography":{"lineSpacing":{"mode":"exact","pt":18}}});
    let parts = unzip(package("# Heading\n\nBODY\n\n> QUOTE\n\n- LIST\n\n`CODE`\n\n|A|B|\n|---|---|\n|C|D|\n\nNOTE[^a]\n\n[^a]: FOOTNOTE\n", &options));
    let styles = xml(&parts, "word/styles.xml"); line(style(&styles, "BodyText"), "360", "exact");
    for id in ["Normal", "Heading1", "Code", "Quote", "ListParagraph", "TableText", "FootnoteText", "TOC1"] {
        assert!(!style(&styles, id).xml().contains("w:lineRule=\"exact\""), "Global body mode contaminated {id}");
    }
    let defaults = descendants(&styles, "docDefaults")[0];
    assert!(defaults.xml().contains("w:lineRule=\"auto\""));
    assert_eq!(descendants(&xml(&parts, "word/document.xml"), "t").into_iter().map(|n| n.text()).collect::<String>().matches("BODY").count(), 1);
}

#[test]
fn p04_docx_local_exact_atleast_and_multiple_use_native_units_and_keep_text_order() {
    let source = [declaration("exact", json!({"mode":"exact","pt":14.33})), "EXACT α 中文\n\n".into(),
        declaration("least", json!({"mode":"atLeast","pt":20})), "MINIMUM β 中文\n\n".into(),
        declaration("multiple", json!({"mode":"multiple","multiple":3})), "MULTIPLE γ 中文\n\n".into()].concat();
    let parts = unzip(package(&source, &json!({"frontMatter":{"mode":"none"}})));
    let document = xml(&parts, "word/document.xml");
    for (text, value, rule) in [("EXACT α 中文","287","exact"),("MINIMUM β 中文","400","atLeast"),("MULTIPLE γ 中文","720","auto")] {
        line(paragraph(&document, text), value, rule);
    }
    let actual = document.text(); assert!(actual.find("EXACT α 中文").unwrap() < actual.find("MINIMUM β 中文").unwrap());
    assert!(actual.find("MINIMUM β 中文").unwrap() < actual.find("MULTIPLE γ 中文").unwrap());
    assert!(!actual.contains("readmd:layout") && !actual.contains("lineSpacing"));
}

#[test]
fn p04_docx_text_only_override_inherits_exact_but_local_legacy_multiplier_can_reset_it() {
    let source = "<!-- readmd:layout v1 {\"id\":\"text\",\"text\":{\"color\":\"#334455\"}} -->\n\nCOLORED\n\n<!-- readmd:layout v1 {\"id\":\"legacy\",\"paragraph\":{\"lineHeight\":2}} -->\n\nLOCAL_LEGACY\n";
    let parts = unzip(package(source, &json!({"frontMatter":{"mode":"none"},"typography":{"lineSpacing":{"mode":"exact","pt":18}}})));
    let document = xml(&parts, "word/document.xml"); line(paragraph(&document,"COLORED"),"360","exact");
    line(paragraph(&document,"LOCAL_LEGACY"),"480","auto");
    assert!(paragraph(&document,"COLORED").xml().contains("334455"));
}

#[test]
fn p04_source_null_transaction_restores_inheritance_and_inverse_keeps_bom_crlf() {
    let source = "\u{feff}<!-- readmd:layout v1 {\"id\":\"p04\",\"paragraph\":{\"lineSpacing\":{\"mode\":\"exact\",\"pt\":18},\"after\":3}} -->\r\n\r\n相同 😀 paragraph\r\n\r\n相同 😀 paragraph\r\n";
    let before = crate::publication_blocks::analyze(source).unwrap();
    let result = crate::publication_blocks::apply(source, &before.revision, "p04", &json!({"paragraph":{"lineSpacing":null}})).unwrap();
    let changed = result["result"]["content"].as_str().unwrap(); assert!(changed.starts_with('\u{feff}'));
    assert!(changed.contains("\r\n\r\n相同 😀 paragraph\r\n\r\n相同 😀 paragraph\r\n"));
    let after = crate::publication_blocks::analyze(changed).unwrap();
    let node = after.nodes.iter().find(|n| n.id == "p04").unwrap();
    assert!(node.style["paragraph"].get("lineSpacing").is_none()); assert_eq!(node.style["paragraph"]["after"],3);
    let inverse = crate::document_transaction::ContentTransaction { base_revision:crate::document_transaction::revision(changed), origin:"editor".into(), edits:serde_json::from_value(result["result"]["inverse"].clone()).unwrap() };
    assert_eq!(crate::document_transaction::apply(changed,&inverse).unwrap().content,source);
    assert_eq!(crate::publication_blocks::apply(source,"stale","p04",&json!({"paragraph":{"lineSpacing":null}})).unwrap_err(),"transaction_conflict");
}

#[test]
fn p04_docx_bad_typed_value_is_rejected_before_any_resource_loader() {
    use std::cell::Cell;
    let count = Cell::new(0);
    let load = |_: &str| { count.set(count.get()+1); Some(("png".into(),b"not-needed".to_vec())) };
    let doc = crate::md_ast::parse("![synthetic](local.png)");
    let bad = crate::docx_writer::build(&doc,&json!({"typography":{"lineSpacing":{"mode":"exact","pt":0}}}),"Synthetic",&load);
    assert!(bad.parts.is_empty()); assert_eq!(bad.warns,vec![crate::publication_typography::INVALID.to_string()]); assert_eq!(count.get(),0);
    assert_eq!(crate::publication_blocks::validate_style(&json!({"id":"bad","paragraph":{"lineSpacing":{"mode":"exact","pt":18},"lineHeight":1}})).unwrap_err(),crate::publication_typography::INVALID);
}

#[test]
fn p04_absent_new_fields_preserve_stable_legacy_docx_parts() {
    let source = "# Legacy\n\nBODY\n\n- LIST\n\n> QUOTE\n\nNOTE[^a]\n\n[^a]: FOOTNOTE\n";
    let old = json!({"frontMatter":{"mode":"none"},"typography":{"lineHeight":1.75}});
    let mut inherited = old.clone(); inherited["typography"]["lineSpacing"] = Value::Null;
    let a = unzip(package(source,&old)); let b = unzip(package(source,&inherited));
    for name in ["word/document.xml","word/styles.xml","word/settings.xml","word/numbering.xml","word/footnotes.xml"] {
        assert_eq!(a[name],b[name],"Missing/null typed override changed {name}");
    }
    assert!(std::str::from_utf8(&a["word/styles.xml"]).unwrap().contains("w:line=\"420\" w:lineRule=\"auto\""));
}
