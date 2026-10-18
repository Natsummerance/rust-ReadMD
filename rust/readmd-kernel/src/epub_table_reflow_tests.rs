//! Actual EPUB entry/XML regressions for the large-font data-table fix.
//! Static package checks do not certify independent reader geometry.
use super::*;
use serde_json::{json, Value};

const SOURCE: &str = "# Original table\n\n| Measure | Count | Interpretation |\n|---|---:|---|\n| Alpha 😀 中文 | 27 | Interpretation stays complete at a reader-selected large font. |\n| Beta | 3 | [Original URL](https://example.invalid/long/ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEFGHIJKLMNOPQRSTUVWXYZABCDEFGHIJKLMNOPQRSTUVWXYZ) |\n\n### Following heading\n\nOriginal following paragraph.\n";
fn package(options: &Value) -> (Vec<crate::mdexport::ZipEntry>, String, String) {
    let before = SOURCE.to_string();
    let (entries, _) = crate::mdexport::epub_build_entries(&json!(SOURCE), "", options,
        "Original table", "Synthetic Author", "en", "urn:uuid:12345678-1234-4234-8234-123456789abc", "2026-10-15T00:00:00Z").unwrap();
    assert_eq!(SOURCE, before);
    let entry = |name: &str| String::from_utf8(entries.iter().find(|e|e.path==name).unwrap().data.clone()).unwrap();
    let css=entry("OEBPS/style.css");let chapter=entry("OEBPS/chapter_1.xhtml");
    (entries,css,chapter)
}
fn options() -> Value { json!({"frontMatter":{"mode":"none"},"cover":{"enabled":false},"epub":{"splitLevel":"none"},"typography":{"size":11},"table":{"widthPct":62,"cellSize":13,"cellPadding":4}}) }
fn descendants<'a>(node:&'a crate::math_document::XmlNode,name:&str,out:&mut Vec<&'a crate::math_document::XmlNode>){
    if node.local()==name {out.push(node);}for child in node.children(){descendants(child,name,out);}
}
fn chapter_xml(chapter: &str) -> crate::math_document::XmlNode {
    // The EPUB generator owns this exact harmless HTML5 doctype. Verify the
    // complete fixed prolog before removing it for the strict math XML reader;
    // never strip an arbitrary DTD, entity declaration or chapter content.
    const PROLOG: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE html>\n";
    let body = chapter.strip_prefix(PROLOG).expect("Generated XHTML must retain its exact XML declaration and HTML5 doctype");
    assert!(!body.contains("<!DOCTYPE") && !body.contains("<!ENTITY"), "No second DTD/entity declaration is allowed");
    crate::math_document::parse_xml(body).unwrap()
}

#[test]
fn packaged_epub_data_table_reflows_without_changing_cells_links_or_alignment(){
    let (_,css,chapter)=package(&options());
    assert_eq!(css.matches(DATA_TABLE_REFLOW_CSS).count(),1,"Actual package must include exactly one shared reflow policy");
    let xml=chapter_xml(&chapter);let mut tables=Vec::new();descendants(&xml,"table",&mut tables);assert_eq!(tables.len(),1);
    let mut rows=Vec::new();descendants(tables[0],"tr",&mut rows);assert_eq!(rows.len(),3);
    let values=rows.iter().map(|row|row.children().map(|cell|cell.text()).collect::<Vec<_>>()).collect::<Vec<_>>();
    assert_eq!(values,vec![vec!["Measure","Count","Interpretation"],vec!["Alpha 😀 中文","27","Interpretation stays complete at a reader-selected large font."],vec!["Beta","3","Original URL"]]);
    for row in rows {let cells=row.children().collect::<Vec<_>>();assert_eq!(cells[1].attributes.get("style").unwrap(),"text-align:right");}
    let mut links=Vec::new();descendants(tables[0],"a",&mut links);assert_eq!(links.len(),1);
    assert_eq!(links[0].attributes.get("href").unwrap(),"https://example.invalid/long/ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEFGHIJKLMNOPQRSTUVWXYZABCDEFGHIJKLMNOPQRSTUVWXYZ");
    assert!(chapter.find("</table>").unwrap()<chapter.find("Following heading").unwrap());assert!(chapter.contains("Original following paragraph."));
}

#[test]
fn packaged_epub_reflow_keeps_author_table_width_size_and_padding(){
    // Existing export_styles::sanitize accepts 50..100%; out-of-range finite
    // values are clamped at those boundaries before every writer sees them.
    for (requested, expected) in [(50,50),(62,62),(95,95),(100,100),(37,50),(-8,50),(101,100),(300,100)] {let mut opts=options();opts["table"]["widthPct"]=json!(requested);let (_,css,_)=package(&opts);
        assert!(css.contains(&format!("table{{width:{expected}%}}")),"Requested table width {requested}% must keep the existing sanitized {expected}% contract");
        assert!(css.contains(&format!("table{{font-size:{}em}}",13.0/11.0)),"Do not shrink the author's table type");
        assert!(css.contains("padding:4px"));assert!(!DATA_TABLE_REFLOW_CSS.contains("font-size")&&!DATA_TABLE_REFLOW_CSS.contains("overflow: hidden")&&!DATA_TABLE_REFLOW_CSS.contains("transform"));
    }
}

#[test]
fn packaged_epub_scientific_table_keeps_three_line_rules_with_reflow(){
    let mut opts=options();opts["table"]["style"]=json!("three-line");opts["table"]["borderColor"]=json!("#123456");opts["table"]["borderWidth"]=json!(1.25);
    let (_,css,chapter)=package(&opts);assert!(css.contains(DATA_TABLE_REFLOW_CSS));
    assert!(css.contains("body table{border-collapse:collapse;border:2.5pt solid #123456;border-left:0;border-right:0}"));
    assert!(css.contains("body thead th{border-bottom:1.25pt solid #123456}"));assert!(css.contains("tbody tr:nth-child(even) td{background:transparent}"));
    let xml=chapter_xml(&chapter);let mut tables=Vec::new();descendants(&xml,"table",&mut tables);assert_eq!(tables.len(),1);assert!(tables[0].text().contains("Interpretation stays complete"));
}

#[test]
fn data_table_reflow_does_not_convert_csl_intrinsic_label_table_to_fixed_columns(){
    use crate::publication_rich::{RichBibliography,RichBibliographyItem,RichNode};
    let doc=Document{front_matter:None,blocks:vec![Block::Table{aligns:vec![Align::Left],header:vec![vec![Inline::Text("Data".into())]],rows:vec![vec![vec![Inline::Text("Original cell".into())]]]},
        Block::Bibliography(RichBibliography{title:"References".into(),items:vec![RichBibliographyItem{key:"study".into(),first_field:vec![RichNode::text("[100]")],rich:vec![RichNode::text("Original reference body")] }],hanging_indent:false,line_spacing:1,entry_spacing:0,source_span:None})]};
    let chapters=render_chapters(&doc,"none",&mut |_|None);let xml=crate::math_document::parse_xml(&chapters[0].body).unwrap();let mut tables=Vec::new();descendants(&xml,"table",&mut tables);assert_eq!(tables.len(),2);
    assert!(tables[0].attributes.get("class").is_none());assert_eq!(tables[1].attributes.get("class").unwrap(),"bibliography-columns");
    assert_eq!(tables[1].text(),"[100]Original reference body");
    assert!(EXTRA_CSS.contains(".bibliography-columns { width: 100%; border-collapse: collapse; border: 0; table-layout: auto;"));
    assert!(EXTRA_CSS.contains("width: 1%; white-space: nowrap; padding-right: .75em;"));
    assert!(DATA_TABLE_REFLOW_CSS.lines().filter(|line|line.contains('{')).all(|line|line.starts_with("table:not(.bibliography-columns)")),"The new policy must not target CSL presentation cells");
}

#[test]
fn packaged_epub_default_stripes_follow_reader_surface_without_recoloring_custom_palette(){
    let (_, css, chapter) = package(&options());
    assert!(css.contains("table:not(.bibliography-columns) tbody tr:nth-child(even){background:rgba(127,127,127,0.10)}"));
    assert!(!css.contains("background:#f3f5f9"));
    assert!(css.contains(".footnotes { font-size: 0.88em; color: inherit;"));
    assert!(!css.contains(".footnotes { font-size: 0.88em; color: #475569;"));
    assert!(chapter.contains("Beta"));
    let mut custom = options();
    custom["table"]["bandColor"] = json!("#cceedd");
    let (_, custom_css, _) = package(&custom);
    assert!(custom_css.contains("tbody tr:nth-child(even){background:#cceedd}"));
    assert!(!custom_css.contains("rgba(127,127,127,0.10)"));
    custom["table"]["banded"] = json!(false);
    let (_, disabled_css, _) = package(&custom);
    assert!(!disabled_css.contains("tbody tr:nth-child(even)"));
    // Ordinary print/Word style values remain physical, opaque author values.
    assert_eq!(crate::export_styles::sanitize(&options())["table"]["bandColor"], "#f3f5f9");
}
