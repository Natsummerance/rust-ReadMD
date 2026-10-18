//! Synthetic CSL-spec structure cases, not copies of the private upstream
//! CSL/INPUT/RESULT fixtures. These tests inspect real zipped DOCX parts. They
//! do not certify Word application pagination, fonts or save/reopen behaviour.
use super::*;
use crate::math_document::XmlNode;
use crate::publication_rich::{RichDecoration, RichDisplay, RichFontStyle, RichFontVariant, RichFontWeight, RichFormat, RichVerticalAlign};
use std::collections::BTreeMap;
use std::io::Read;

fn text_node(value: &str) -> RichNode { RichNode::text(value) }
fn group(display: Option<RichDisplay>, entry_index: Option<usize>, children: Vec<RichNode>) -> RichNode {
    RichNode::Group { display, entry_index, children }
}
fn block(children: Vec<RichNode>) -> RichNode { group(Some(RichDisplay::Block), None, children) }
fn indent(children: Vec<RichNode>) -> RichNode { group(Some(RichDisplay::Indent), None, children) }
fn pair(label: Vec<RichNode>, body: Vec<RichNode>) -> Vec<RichNode> {
    vec![group(Some(RichDisplay::LeftMargin), None, label), group(Some(RichDisplay::RightInline), None, body)]
}
fn item(key: &str, rich: Vec<RichNode>) -> RichBibliographyItem {
    RichBibliographyItem { key: key.into(), first_field: vec![], rich }
}
fn document(items: Vec<RichBibliographyItem>, line: u32, entry: u32, hanging: bool) -> Document {
    let mut doc = crate::md_ast::parse("Before references");
    doc.blocks.push(Block::Bibliography(RichBibliography {
        title: "References".into(), items, line_spacing: line, entry_spacing: entry, hanging_indent: hanging, source_span: None,
    }));
    doc.blocks.extend(crate::md_ast::parse("After references").blocks); doc
}

/// Independent local ZIP-header/CRC/deflate reader. The writer is exercised all
/// the way to real OPC ZIP bytes, rather than inspecting its unzipped structs.
fn unzip(package: DocxPackage) -> BTreeMap<String, Vec<u8>> {
    assert!(!package.parts.is_empty(), "{:?}", package.warns);
    let entries = package.parts.into_iter().map(|(path, data)| crate::mdexport::ZipEntry { path, data, compress: true }).collect::<Vec<_>>();
    let zip = crate::mdexport::write_zip_at(&entries, 0, 0).unwrap();
    let u16at = |i| u16::from_le_bytes(zip[i..i+2].try_into().unwrap());
    let u32at = |i| u32::from_le_bytes(zip[i..i+4].try_into().unwrap());
    let mut at = 0; let mut parts = BTreeMap::new();
    while zip.get(at..at+4) == Some(&b"PK\x03\x04"[..]) {
        assert_eq!(u16at(at+6) & 8, 0, "Known-length local members must not need data descriptors");
        let method = u16at(at+8); let crc = u32at(at+14); let packed = u32at(at+18) as usize; let size = u32at(at+22) as usize;
        let name_len = u16at(at+26) as usize; let extra_len = u16at(at+28) as usize;
        let name = std::str::from_utf8(&zip[at+30..at+30+name_len]).unwrap().to_string();
        let data_at = at+30+name_len+extra_len; let data = &zip[data_at..data_at+packed];
        let decoded = if method == 8 {
            let mut decoded = Vec::new(); flate2::read::DeflateDecoder::new(data).read_to_end(&mut decoded).unwrap(); decoded
        } else { assert_eq!(method, 0); data.to_vec() };
        assert_eq!(decoded.len(), size); let mut checksum = flate2::Crc::new(); checksum.update(&decoded); assert_eq!(checksum.sum(), crc);
        assert!(parts.insert(name, decoded).is_none()); at = data_at+packed;
    }
    assert_eq!(zip.get(at..at+4), Some(&b"PK\x01\x02"[..]));
    assert_eq!(zip.get(zip.len()-22..zip.len()-18), Some(&b"PK\x05\x06"[..]));
    assert_eq!(u16at(zip.len()-12) as usize, parts.len());
    for (name, bytes) in &parts {
        if name.ends_with(".xml") || name.ends_with(".rels") { crate::math_document::parse_xml(std::str::from_utf8(bytes).unwrap()).unwrap_or_else(|code| panic!("{name}: {code}")); }
    }
    parts
}
fn build_parts(doc: &Document, options: &Value) -> BTreeMap<String, Vec<u8>> {
    unzip(build(doc, &crate::export_styles::sanitize(options), "Synthetic bibliography", &|_| None))
}
fn xml(parts: &BTreeMap<String, Vec<u8>>, name: &str) -> XmlNode {
    crate::math_document::parse_xml(std::str::from_utf8(&parts[name]).unwrap()).unwrap()
}
fn all<'a>(node: &'a XmlNode, name: &str) -> Vec<&'a XmlNode> {
    let mut out = Vec::new(); if node.name == name { out.push(node); }
    for child in node.children() { out.extend(all(child, name)); } out
}
fn child<'a>(node: &'a XmlNode, name: &str) -> &'a XmlNode {
    node.children().find(|node| node.name == name).unwrap_or_else(|| panic!("{} has no {name}", node.name))
}
fn descendant<'a>(node: &'a XmlNode, name: &str) -> &'a XmlNode {
    all(node, name).into_iter().next().unwrap_or_else(|| panic!("{} has no descendant {name}", node.name))
}
fn entry<'a>(node: &'a XmlNode, key: &str) -> &'a XmlNode {
    let tag = format!("readmd-csl-entry:{key}");
    all(node, "w:sdt").into_iter().find(|node| {
        node.children().find(|node| node.name == "w:sdtPr").is_some_and(|pr| {
            pr.children().any(|node| node.name == "w:tag" && node.attributes.get("w:val") == Some(&tag))
        })
    }).unwrap_or_else(|| panic!("Missing entry {key}"))
}
fn visible_paragraphs(node: &XmlNode) -> Vec<&XmlNode> {
    all(node, "w:p").into_iter().filter(|node| !node.text().trim().is_empty()).collect()
}
fn number(node: &XmlNode, key: &str) -> i64 { node.attributes[key].parse().unwrap() }
fn assert_native_tables(node: &XmlNode) {
    for table in all(node, "w:tbl") {
        let props = child(table, "w:tblPr");
        assert_eq!(child(props, "w:tblLayout").attributes["w:type"], "fixed");
        assert!(all(table, "w:cantSplit").is_empty()); assert!(all(table, "w:trHeight").is_empty());
        let width = number(child(props, "w:tblW"), "w:w");
        let grid = child(table, "w:tblGrid").children().map(|node| number(node, "w:w")).collect::<Vec<_>>();
        assert_eq!(grid.len(), 2); assert_eq!(grid.iter().sum::<i64>(), width);
        assert!(grid[0] <= width * 35 / 100, "Labels obey the current container's physical cap");
        for row in table.children().filter(|node| node.name == "w:tr") {
            assert_eq!(row.children().filter(|node| node.name == "w:tc").count(), 2);
            for cell in row.children().filter(|node| node.name == "w:tc") {
                assert_eq!(cell.children().last().unwrap().name, "w:p", "Every nested table cell has a legal paragraph tail");
                assert_eq!(child(child(cell, "w:tcPr"), "w:vAlign").attributes["w:val"], "top");
            }
        }
    }
}

#[test]
fn docx_display_blocks_and_nested_indent_are_native_paragraphs() {
    for size in [9., 17.] {
        let doc = document(vec![item("blocks", vec![
            text_node("PREFIX"), block(vec![text_node("AUTHOR")]),
            indent(vec![text_node("INDENT_ONE"), indent(vec![text_node("INDENT_TWO")]), text_node("INDENT_RESUME")]),
            text_node("SUFFIX"),
        ])], 1, 0, false);
        let parts = build_parts(&doc, &serde_json::json!({"typography":{"size":size,"firstLineIndent":6,"spacing":20}}));
        let root = xml(&parts, "word/document.xml"); let entry = entry(&root, "blocks");
        let paragraphs = visible_paragraphs(entry);
        assert_eq!(paragraphs.iter().map(|p|p.text()).collect::<Vec<_>>(), ["PREFIX","AUTHOR","INDENT_ONE","INDENT_TWO","INDENT_RESUME","SUFFIX"]);
        for (paragraph, ems) in paragraphs.iter().zip([0,0,4,8,4,0]) {
            let ind = descendant(paragraph, "w:ind"); assert_eq!(number(ind, "w:left"), (size*20.*f64::from(ems)) as i64);
            assert_eq!(ind.attributes["w:firstLine"], "0");
            assert_eq!(descendant(paragraph, "w:spacing").attributes["w:afterLines"], "0");
        }
        assert!(all(entry, "w:br").is_empty()); assert!(all(entry, "w:tbl").is_empty());
        // The XML parser's synthetic root also contains the newline following
        // the XML declaration. Body copy/order must be checked on w:body.
        let body = descendant(&root, "w:body");
        assert_eq!(visible_paragraphs(body).iter().map(|p|p.text()).collect::<Vec<_>>(),
            ["Before references","References","PREFIX","AUTHOR","INDENT_ONE","INDENT_TWO","INDENT_RESUME","SUFFIX","After references"]);
        assert_eq!(body.text(), "Before referencesReferencesPREFIXAUTHORINDENT_ONEINDENT_TWOINDENT_RESUMESUFFIXAfter references");
    }
}

#[test]
fn docx_complex_hanging_is_two_em_per_fullwidth_paragraph_and_never_enters_cells() {
    for size in [9.,17.] {
        let mut rich = vec![block(vec![text_node("FULLWIDTH")]),indent(vec![text_node("INDENTED")])];
        rich.extend(pair(vec![text_node("LABEL")],vec![block(vec![text_node("CELL_BODY")]),indent(vec![text_node("CELL_INDENT")])]));
        let parts = build_parts(&document(vec![item("hanging",rich)],1,0,true),&serde_json::json!({"typography":{"size":size}}));
        let root = xml(&parts,"word/document.xml"); let entry = entry(&root,"hanging");
        for (value,left_ems) in [("FULLWIDTH",2),("INDENTED",6)] {
            let p = visible_paragraphs(entry).into_iter().find(|p|p.text()==value).unwrap();
            let ind = descendant(p,"w:ind"); assert_eq!(number(ind,"w:left"),(size*20.*f64::from(left_ems)) as i64);
            assert_eq!(number(ind,"w:hanging"),(size*40.) as i64); assert!(!ind.attributes.contains_key("w:firstLine"));
        }
        for cell in all(entry,"w:tc") {for p in visible_paragraphs(cell) {
            let ind = descendant(p,"w:ind");assert!(!ind.attributes.contains_key("w:hanging"));assert_eq!(ind.attributes["w:firstLine"],"0");
            assert_eq!(number(ind,"w:left"),if p.text()=="CELL_INDENT" {(size*80.) as i64}else{0});
        }}
    }
}

#[test]
fn docx_declared_section_links_target_native_stable_bookmarks_without_hijacking_headings() {
    let doc = crate::md_ast::parse(concat!(
        "[Declared section](#readmd-section-main)\n\n",
        "<!-- readmd-section:v1 {\"id\":\"main\",\"break\":\"nextPage\"} -->\n\n",
        "# Main text\n\n[Declared again](#readmd-section-main)\n\n",
        "# readmd-section-main-title\n\n[Ordinary heading](#readmd-section-main-title)\n",
    ));
    let parts = build_parts(&doc,&serde_json::json!({})); let root = xml(&parts,"word/document.xml");
    let links = all(&root,"w:hyperlink"); let bookmarks = all(&root,"w:bookmarkStart");
    let anchor = section_bookmark_name("main");
    assert_eq!(links.iter().filter(|link|link.attributes.get("w:anchor")==Some(&anchor)).count(),2);
    assert_eq!(bookmarks.iter().filter(|mark|mark.attributes.get("w:name")==Some(&anchor)).count(),1);
    let ordinary = links.iter().find(|link|link.text()=="Ordinary heading").unwrap();
    assert_eq!(ordinary.attributes["w:anchor"],bookmark_name("readmd-section-main-title"));
    assert!(bookmarks.iter().any(|mark|mark.attributes.get("w:name")==ordinary.attributes.get("w:anchor")));
    let doc = crate::md_ast::parse("# readmd-section-main\n\n[Same name, no section](#readmd-section-main)\n");
    let parts = build_parts(&doc,&serde_json::json!({})); let root = xml(&parts,"word/document.xml");
    let link = descendant(&root,"w:hyperlink"); assert_eq!(link.attributes["w:anchor"],bookmark_name("readmd-section-main"));
    assert_ne!(link.attributes["w:anchor"],anchor,"An undeclared fragment remains a heading link");
}

#[test]
fn docx_transparent_groups_and_links_preserve_paragraphs_urls_and_entry_identity() {
    let href = "https://example.test/?q=中文&emoji=😀";
    let doc = document(vec![item("key<&😀", vec![block(vec![
        text_node("BEGIN "), group(None, Some(7), vec![RichNode::Link { href: href.into(), children: vec![
            text_node("LINK "), group(None, Some(11), vec![text_node("MIDDLE")]), text_node(" ENDLINK"),
        ]}]), text_node(" END"),
    ])])], 1, 0, false);
    let parts = build_parts(&doc, &serde_json::json!({})); let root = xml(&parts, "word/document.xml"); let entry = entry(&root, "key<&😀");
    assert_eq!(visible_paragraphs(entry).len(), 1); assert_eq!(entry.text(), "BEGIN LINK MIDDLE ENDLINK END");
    let tags = all(entry,"w:tag").iter().map(|n|n.attributes["w:val"].as_str()).collect::<Vec<_>>();
    assert!(tags.contains(&"readmd-csl-index:7") && tags.contains(&"readmd-csl-index:11"));
    let ids = all(entry,"w:id").iter().map(|n|n.attributes["w:val"].clone()).collect::<Vec<_>>();
    assert_eq!(ids.iter().collect::<HashSet<_>>().len(), ids.len());
    let rels = xml(&parts, "word/_rels/document.xml.rels");
    let relation = all(&rels,"Relationship").into_iter().find(|n|n.attributes.get("Target").is_some_and(|v|v==href)).unwrap();
    let links = all(entry,"w:hyperlink"); assert_eq!(links.len(),3);
    for link in links { assert_eq!(link.attributes["r:id"],relation.attributes["Id"]); }
    assert!(!entry.text().contains("readmd-csl"), "Ownership is native metadata, not copied body text");
}

#[test]
fn docx_all_nested_labels_share_preference_and_multirow_body_has_one_label() {
    let mut first = vec![block(vec![text_node("HEADING_A")])];
    first.extend(pair(vec![text_node("[9]")],vec![text_node("BODY_A1")]));
    first.push(group(Some(RichDisplay::RightInline),None,vec![text_node("BODY_A2")]));
    let mut second = vec![block(vec![text_node("HEADING_B")])];
    second.extend(pair(vec![text_node("[100]")],pair(vec![text_node("[123456]")],vec![text_node("NESTED_BODY")])));
    let doc = document(vec![item("a",first),item("b",second)],1,0,false);
    let parts = build_parts(&doc,&serde_json::json!({"typography":{"size":11}})); let root = xml(&parts,"word/document.xml");
    assert_native_tables(&root); let tables = all(&root,"w:tbl"); assert_eq!(tables.len(),3);
    // Independent reviewed DejaVu hmtx values: brackets 799, digits 1303,
    // UPEM 2048. The longest label is nested, not a top-level first_field.
    let preferred = (((2_f64*799.+6.*1303.)/2048.+0.75)*11.*20.).ceil() as i64;
    for table in tables {
        let width = number(child(child(table,"w:tblPr"),"w:tblW"),"w:w");
        let columns = child(table,"w:tblGrid").children().collect::<Vec<_>>();
        assert_eq!(number(columns[0],"w:w"),preferred.min(width*35/100));
        let cells = child(table,"w:tr").children().filter(|n|n.name=="w:tc").collect::<Vec<_>>();
        assert_eq!(number(child(child(child(cells[0],"w:tcPr"),"w:tcMar"),"w:right"),"w:w"),165);
    }
    let a = entry(&root,"a"); assert_eq!(a.text().matches("[9]").count(),1);
    let cells = child(descendant(a,"w:tbl"),"w:tr").children().filter(|n|n.name=="w:tc").collect::<Vec<_>>();
    assert_eq!(visible_paragraphs(cells[1]).iter().map(|n|n.text()).collect::<Vec<_>>(),["BODY_A1","BODY_A2"]);
}

#[test]
fn docx_columns_nested_in_both_cells_fit_adaptively_and_end_in_paragraphs() {
    let rich = pair(pair(vec![text_node("L")],vec![text_node("LEFT_BODY")]),pair(vec![text_node("R")],vec![text_node("RIGHT_BODY")]));
    let doc = document(vec![item("nested",rich)],1,0,false);
    let parts = build_parts(&doc,&serde_json::json!({})); let root = xml(&parts,"word/document.xml"); let entry = entry(&root,"nested");
    assert_native_tables(entry); assert_eq!(all(entry,"w:tbl").len(),3); assert_eq!(entry.text(),"LLEFT_BODYRRIGHT_BODY");
    let outer = descendant(entry,"w:tbl"); let cells = child(outer,"w:tr").children().filter(|n|n.name=="w:tc").collect::<Vec<_>>();
    for cell in cells { assert_eq!(all(cell,"w:tbl").len(),1); assert_eq!(cell.children().last().unwrap().name,"w:p"); }
}

#[test]
fn docx_long_labels_wrap_in_narrow_containers_without_font_shrink_or_loss() {
    let label = "LONG_UNSPACED_LABEL_".repeat(12)+"中文😀END_LABEL";
    let mut rich = vec![block(vec![text_node("HEADER")])]; rich.extend(pair(vec![text_node(&label)],vec![text_node("BODY END_BODY")]));
    let simple = RichBibliographyItem {key:"short".into(),first_field:vec![text_node("[9]")],rich:vec![text_node("SHORT_BODY")]};
    let doc = document(vec![item("long",rich),simple],1,0,false);
    let parts = build_parts(&doc,&serde_json::json!({"page":{"size":"A5","columns":2,"columnGap":6},"typography":{"size":11,"font":"SimSun"}}));
    let root = xml(&parts,"word/document.xml"); let entry = entry(&root,"long"); assert_native_tables(entry);
    let table = descendant(entry,"w:tbl"); let width = number(child(child(table,"w:tblPr"),"w:tblW"),"w:w");
    assert_eq!(number(child(table,"w:tblGrid").children().next().unwrap(),"w:w"),width*35/100);
    let simple_p = visible_paragraphs(&root).into_iter().find(|p|p.text()=="[9]SHORT_BODY").unwrap();
    assert_eq!(number(descendant(simple_p,"w:ind"),"w:left"),width*35/100,"A long complex label cannot push a simple entry's body off the page");
    let label_p = visible_paragraphs(entry).into_iter().find(|p|p.text()==label).unwrap();
    assert_eq!(descendant(label_p,"w:wordWrap").attributes["w:val"],"1");
    assert!(all(label_p,"w:sz").is_empty() && all(label_p,"w:szCs").is_empty());
    assert!(all(table,"w:noWrap").is_empty() && all(table,"w:tcFitText").is_empty());
    assert_eq!(entry.text().matches(&label).count(),1);
    let styles = std::str::from_utf8(&parts["word/styles.xml"]).unwrap();
    assert!(styles.contains("w:ascii=\"SimSun\"") && styles.contains("<w:sz w:val=\"22\"/>"));
}

#[test]
fn docx_unpaired_display_is_fullwidth_and_multiple_pairs_keep_document_order() {
    let mut rich = vec![group(Some(RichDisplay::LeftMargin),None,vec![text_node("ORPHAN_LEFT")]),text_node("BOUNDARY"),group(Some(RichDisplay::RightInline),None,vec![text_node("ORPHAN_RIGHT")])];
    rich.extend(pair(vec![text_node("L1")],vec![text_node("BODY1")])); rich.push(block(vec![text_node("BETWEEN")]));
    rich.extend(pair(vec![text_node("L2")],vec![text_node("BODY2")])); rich.push(text_node("SUFFIX"));
    let parts = build_parts(&document(vec![item("ordered",rich)],1,0,false),&serde_json::json!({})); let root = xml(&parts,"word/document.xml"); let entry = entry(&root,"ordered");
    assert_eq!(all(entry,"w:tbl").len(),2); assert_native_tables(entry);
    assert_eq!(entry.text(),"ORPHAN_LEFTBOUNDARYORPHAN_RIGHTL1BODY1BETWEENL2BODY2SUFFIX");
    for name in ["ORPHAN_LEFT","BOUNDARY","ORPHAN_RIGHT","BETWEEN","SUFFIX"] {
        let p = visible_paragraphs(entry).into_iter().find(|p|p.text()==name).unwrap();
        assert_eq!(descendant(p,"w:ind").attributes["w:left"],"0");
    }
}

#[test]
fn docx_complex_entry_spacing_is_added_only_after_complete_entries() {
    for (line, gap) in [(1,0),(1,2),(3,0),(3,2)] { for size in [9.,17.] {
        let entries = ["a","b"].into_iter().map(|key| {
            let mut rich = vec![block(vec![text_node(&format!("HEAD_{key}"))])];
            rich.extend(pair(vec![text_node("[100]")],vec![text_node(&format!("FIRST_{key}"))]));
            rich.push(group(Some(RichDisplay::RightInline),None,vec![text_node(&format!("LAST_{key}"))])); item(key,rich)
        }).collect();
        let parts = build_parts(&document(entries,line,gap,false),&serde_json::json!({"typography":{"size":size,"spacing":24,"keepLines":true}}));
        let root = xml(&parts,"word/document.xml");
        for (key, expected_after) in [("a",line*gap*100),("b",0)] {
            let entry = entry(&root,key);
            for p in visible_paragraphs(entry) {
                let spacing = descendant(p,"w:spacing");
                let last = p.text()=="[100]" || p.text()==format!("LAST_{key}");
                assert_eq!(number(spacing,"w:afterLines"),i64::from(if last {expected_after}else{0}));
                assert_eq!(number(spacing,"w:line"),i64::from(line*240));
                assert_eq!(spacing.attributes["w:lineRule"],"auto"); assert_eq!(spacing.attributes["w:after"],"0");
                assert_eq!(descendant(p,"w:keepNext").attributes["w:val"],"0"); assert_eq!(descendant(p,"w:keepLines").attributes["w:val"],"0");
            }
            for p in all(entry,"w:p").into_iter().filter(|p|p.text().is_empty()) {
                let spacing = descendant(p,"w:spacing"); assert_eq!(spacing.attributes["w:line"],"1"); assert_eq!(spacing.attributes["w:afterLines"],"0");
            }
        }
    } }
}

#[test]
fn docx_existing_first_field_keeps_outer_and_internal_native_columns() {
    let first_field = vec![RichNode::Text { text:"ROOT_LABEL".into(),format:RichFormat {font_weight:RichFontWeight::Bold,..Default::default()} }];
    let mut rich = vec![block(vec![text_node("AUTHOR")])]; rich.extend(pair(vec![text_node("INNER")],vec![text_node("BODY")])); rich.push(text_node("END"));
    let source_item = RichBibliographyItem {key:"first-field".into(),first_field,rich};
    let before = serde_json::to_value(&source_item).unwrap();
    let doc = document(vec![source_item],1,0,false); let parts = build_parts(&doc,&serde_json::json!({})); let root = xml(&parts,"word/document.xml"); let entry = entry(&root,"first-field");
    assert_native_tables(entry); assert_eq!(all(entry,"w:tbl").len(),2); assert_eq!(entry.text(),"ROOT_LABELAUTHORINNERBODYEND");
    assert_eq!(entry.text().matches("ROOT_LABEL").count(),1); assert!(!all(entry,"w:b").is_empty());
    let Block::Bibliography(bib) = &doc.blocks[1] else {panic!("Bibliography remains in its source slot")};
    assert_eq!(serde_json::to_value(&bib.items[0]).unwrap(),before);
}

#[test]
fn docx_simple_first_field_keeps_tabs_and_uses_complex_bibliography_label_preference() {
    let simple = RichBibliographyItem {key:"simple".into(),first_field:vec![text_node("[9]")],rich:vec![text_node("SIMPLE_BODY")]};
    let mut rich = vec![block(vec![text_node("HEAD")])]; rich.extend(pair(vec![text_node("[123456]")],vec![text_node("COMPLEX_BODY")]));
    let parts = build_parts(&document(vec![simple,item("complex",rich)],3,2,false),&serde_json::json!({"typography":{"size":11}}));
    let root = xml(&parts,"word/document.xml"); let p = visible_paragraphs(&root).into_iter().find(|p|p.text()=="[9]SIMPLE_BODY").unwrap();
    assert_eq!(all(p,"w:tab").len(),2, "One positioned tab stop and one native tab run");
    let preferred = (((2_f64*799.+6.*1303.)/2048.+0.75)*11.*20.).ceil() as i64;
    assert_eq!(number(descendant(p,"w:ind"),"w:left"),preferred);
    assert_eq!(number(descendant(p,"w:ind"),"w:hanging"),preferred);
    assert_eq!(number(descendant(p,"w:spacing"),"w:line"),720); assert_eq!(number(descendant(p,"w:spacing"),"w:afterLines"),600);
    assert_eq!(number(descendant(entry(&root,"complex"),"w:gridCol"),"w:w"),preferred);
}

#[test]
fn docx_consumed_first_field_display_retains_the_existing_tab_paragraph() {
    let entry_item = RichBibliographyItem {
        key:"legacy".into(),
        first_field:vec![group(Some(RichDisplay::LeftMargin),Some(2),vec![text_node("[9]")])],
        rich:vec![group(Some(RichDisplay::RightInline),Some(2),vec![text_node("LEGACY_BODY")])],
    };
    let parts = build_parts(&document(vec![entry_item],3,2,false),&serde_json::json!({}));
    let root = xml(&parts,"word/document.xml");
    assert!(all(&root,"w:tbl").is_empty());
    let p = visible_paragraphs(&root).into_iter().find(|p|p.text()=="[9]LEGACY_BODY").unwrap();
    assert_eq!(all(p,"w:tab").len(),2); assert!(all(p,"w:br").is_empty());
    assert_eq!(number(descendant(p,"w:spacing"),"w:line"),720);
    assert_eq!(number(descendant(p,"w:spacing"),"w:afterLines"),0,"Last entry does not acquire a gap");
}

#[test]
fn docx_long_multi_block_entries_keep_native_runs_and_allow_page_splitting() {
    let formats = [
        RichFormat {font_style:RichFontStyle::Oblique,font_variant:RichFontVariant::SmallCaps,font_weight:RichFontWeight::Bold,text_decoration:RichDecoration::Underline,vertical_align:RichVerticalAlign::Sup},
        RichFormat {vertical_align:RichVerticalAlign::Sub,..Default::default()},
    ];
    let rich = pair(vec![text_node("[999]")],(0..120).map(|index| block(vec![RichNode::Text {
        text:format!("PARA_{index:03} 原Unicode Abcxyz😀 full text retained."),format:formats[index%formats.len()].clone(),
    }])).collect());
    let doc = document(vec![item("long-body",rich)],1,0,false);
    let parts = build_parts(&doc,&serde_json::json!({"typography":{"keepLines":true},"table":{"headerBold":true,"banded":true}}));
    let root = xml(&parts,"word/document.xml"); let entry = entry(&root,"long-body"); assert_native_tables(entry);
    assert_eq!(visible_paragraphs(entry).len(),121); assert_eq!(all(entry,"w:tbl").len(),1);
    assert!(entry.text().contains("PARA_000") && entry.text().contains("PARA_119")); assert_eq!(entry.text().matches("😀").count(),120);
    for property in ["w:b","w:i","w:smallCaps","w:u"] { assert_eq!(all(entry,property).len(),60); }
    assert_eq!(all(entry,"w:vertAlign").iter().filter(|n|n.attributes["w:val"]=="superscript").count(),60);
    assert_eq!(all(entry,"w:vertAlign").iter().filter(|n|n.attributes["w:val"]=="subscript").count(),60);
    for p in visible_paragraphs(entry) { assert_eq!(descendant(p,"w:keepLines").attributes["w:val"],"0"); }
    assert!(all(entry,"w:shd").is_empty() && all(entry,"w:tblHeader").is_empty(), "Bibliography is not an ordinary banded data table");
}

#[test]
fn docx_invalid_display_geometry_and_light_reject_the_whole_package() {
    let options = crate::export_styles::sanitize(&serde_json::json!({"page":{"size":"A5","columns":3,"columnGap":6}}));
    let too_deep = indent(vec![indent(vec![indent(vec![text_node("CANNOT_FIT")])])]);
    let doc = document(vec![item("narrow",vec![too_deep])],1,0,false);
    let failure = build(&doc,&options,"Synthetic",&|_|None);
    assert!(failure.parts.is_empty()); assert_eq!(failure.warns[0],"publication_layout_geometry"); assert!(failure.warns[1].starts_with("bibliography.items[0].layout"));
    for nested in [false,true] {
        let light = RichNode::Text {text:"LIGHT".into(),format:RichFormat {font_weight:RichFontWeight::Light,..Default::default()}};
        let rich = if nested {vec![block(vec![RichNode::Link {href:"https://example.test".into(),children:vec![light]}])]}else{vec![light]};
        let failure = build(&document(vec![item("light",rich)],1,0,false),&options,"Synthetic",&|_|None);
        assert!(failure.parts.is_empty()); assert_eq!(failure.warns[0],"academic_rich_unsupported"); assert!(failure.warns[1].ends_with(".font_weight"));
    }
    for (line,gap) in [(0,1),(u32::MAX,u32::MAX)] {
        let failure = build(&document(vec![item("spacing",vec![block(vec![text_node("BODY")])])],line,gap,false),&options,"Synthetic",&|_|None);
        assert!(failure.parts.is_empty()); assert_eq!(failure.warns[0],"academic_rich_spacing");
    }
    let mut nested = text_node("BUDGET"); for _ in 0..20 {nested = block(vec![nested]);}
    let failure = build(&document(vec![item("budget",vec![nested])],1,0,false),&options,"Synthetic",&|_|None);
    assert!(failure.parts.is_empty()); assert_eq!(failure.warns[0],"academic_rich_budget");
}
