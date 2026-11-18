use super::*;
use crate::math_document::XmlNode;

const SOURCE:&str="# Owned PPTX style\n\n| Label | Group | Amount |\n| :--- | :---: | ---: |\n| Alpha | X | 12.40 |\n| Beta | Y | 7.20 |\n";
fn settings(kind:&str)->Value{json!({"theme":"white","transition":"none","table":{"style":kind,"borderColor":"#112233","borderWidth":1.75}})}
fn descendants<'a>(node:&'a XmlNode,local:&str,found:&mut Vec<&'a XmlNode>){
    if node.namespace==A&&node.local()==local{found.push(node);}
    for child in node.children(){descendants(child,local,found);}
}
fn child<'a>(node:&'a XmlNode,local:&str)->&'a XmlNode{node.children().find(|child|child.namespace==A&&child.local()==local).unwrap_or_else(||panic!("missing {local} in {}",node.name))}
fn native_tables(deck:&Deck)->Vec<XmlNode>{
    let bytes=package(deck,"Owned table").unwrap();let entries=crate::convert::parse_zip_entries(&bytes).unwrap();let mut tables=Vec::new();
    assert!(!entries.iter().any(|(name,_)|name.starts_with("ppt/media/")),"ordinary tables must not become pictures");
    for (name,bytes) in entries.iter().filter(|(name,_)|name.starts_with("ppt/slides/slide")&&name.ends_with(".xml")){
        let source=std::str::from_utf8(bytes).unwrap();assert!(!source.contains("<p:pic>"),"{name}");
        let root=crate::math_document::parse_xml(source).unwrap();let mut found=Vec::new();descendants(&root,"tbl",&mut found);tables.extend(found.into_iter().cloned());
    }
    tables
}
fn rows(table:&XmlNode)->Vec<Vec<&XmlNode>>{table.children().filter(|row|row.namespace==A&&row.local()=="tr").map(|row|row.children().filter(|cell|cell.namespace==A&&cell.local()=="tc").collect()).collect()}
fn assert_cell(cell:&XmlNode,align:&str,color:&str,widths:[i64;4]){
    let mut paragraphs=Vec::new();descendants(child(cell,"txBody"),"pPr",&mut paragraphs);assert!(!paragraphs.is_empty());
    for paragraph in paragraphs{assert_eq!(paragraph.attributes.get("algn").map(String::as_str),Some(align));}
    let properties=child(cell,"tcPr");
    for (side,width) in ["lnL","lnR","lnT","lnB"].into_iter().zip(widths){
        let line=child(properties,side);
        if width==0 {assert_eq!(line.children().count(),1,"{side}");assert_eq!(line.children().next().unwrap().local(),"noFill","{side}");}
        else{
            assert_eq!(line.attributes.get("w").unwrap().parse::<i64>().unwrap(),width,"{side}");
            assert_eq!(child(child(line,"solidFill"),"srgbClr").attributes.get("val").map(String::as_str),Some(color),"{side}");
            assert!(!line.children().any(|node|node.local()=="noFill"));
        }
    }
}
fn assert_three_line(table:&XmlNode,aligns:&[&str],color:&str){
    let rows=rows(table);assert!(rows.len()>1);
    for (index,row) in rows.iter().enumerate(){
        assert_eq!(row.len(),aligns.len());
        let widths=if index==0{[0,0,44450,22225]}else if index+1==rows.len(){[0,0,0,44450]}else{[0,0,0,0]};
        for (cell,align) in row.iter().zip(aligns){assert_cell(cell,align,color,widths);}
    }
}

#[test]
fn three_line_rules_and_column_alignment_reach_editable_ooxml(){
    let deck=build(SOURCE,"","Owned",&settings("three-line")).unwrap();let tables=native_tables(&deck);assert_eq!(tables.len(),1);
    let table=&tables[0];assert_eq!(rows(table).len(),3);assert_three_line(table,&["l","ctr","r"],"112233");
    assert!(table.text().contains("AlphaX12.40"));assert!(table.text().contains("BetaY7.20"));
    let preview=preview_html(&deck).unwrap();assert!(preview.contains("border-top:3.5pt solid #112233;"));assert!(preview.contains("border-bottom:1.75pt solid #112233;"));assert!(preview.contains("border-bottom:3.5pt solid #112233;"));
    for value in ["border-left:0;","border-right:0;","text-align:left;","text-align:center;","text-align:right;"]{assert!(preview.contains(value),"{value}");}
    assert!(!preview.contains("border:1px solid #8A9299"));
}

#[test]
fn every_continuation_has_three_rules_repeated_header_and_all_rows_once(){
    let mut source="# Owned continuation\n\n| Label | Group | Amount |\n| :--- | :---: | ---: |\n".to_string();for n in 0..60{source.push_str(&format!("| row-{n:03} | G | {n}.40 |\n"));}
    let deck=build(&source,"","Owned",&settings("three-line")).unwrap();let tables=native_tables(&deck);assert_eq!(tables.len(),8);
    let mut content=String::new();
    for table in &tables{
        assert_three_line(table,&["l","ctr","r"],"112233");let rows=rows(table);assert_eq!(rows[0].iter().map(|cell|cell.text()).collect::<Vec<_>>(),vec!["Label","Group","Amount"]);
        for row in &rows[1..]{content.push_str(&row[0].text());content.push('\n');}
    }
    for n in 0..60{assert_eq!(content.lines().filter(|line|*line==format!("row-{n:03}")).count(),1);}
    for shape in deck.slides.iter().flat_map(|slide|&slide.shapes){if let Shape::Table{rect,..}=shape{assert!(rect[0]>=0.&&rect[1]>=0.&&rect[0]+rect[2]<=W&&rect[1]+rect[3]<=H);}}
}

#[test]
fn grid_and_default_and_preset_borders_do_not_inherit_three_line_state(){
    let _=build(SOURCE,"","Three",&settings("three-line")).unwrap();
    for (options,color,width) in [(settings("grid"),"112233",22225),(json!({}),"C8CDD4",9525),(crate::export_styles::preset_style("business"),"9FB3D1",9525)]{
        let deck=build(SOURCE,"","Grid",&options).unwrap();let tables=native_tables(&deck);assert_eq!(tables.len(),1);
        for row in rows(&tables[0]){for (cell,align) in row.iter().zip(["l","ctr","r"]){assert_cell(cell,align,color,[width;4]);}}
        let grid_preview=preview_html(&deck).unwrap();assert!(!grid_preview.contains("border-left:0;"));
    }
}

#[test]
fn unspecified_column_uses_sanitized_alignment_and_explicit_column_wins(){
    let source="# Fallback\n\n| Unspecified | Amount |\n| --- | ---: |\n| Unicode 😀 & text | 8.40 |\n";
    for (fallback,expected) in [("left","l"),("center","ctr"),("right","r"),("justify","just"),("invalid","l")]{
        let mut options=settings("three-line");options["table"]["align"]=json!(fallback);
        let deck=build(source,"","Fallback",&options).unwrap();let tables=native_tables(&deck);assert_eq!(tables.len(),1);assert_three_line(&tables[0],&[expected,"r"],"112233");
        assert!(tables[0].text().contains("Unicode 😀 & text"));
    }
}

#[test]
fn header_only_table_has_only_outer_rules_without_vertical_theme_fallback(){
    let deck=build("# Header only\n\n| Label | Amount |\n| :--- | ---: |\n","","Header",&settings("three-line")).unwrap();let tables=native_tables(&deck);assert_eq!(tables.len(),1);let rows=rows(&tables[0]);assert_eq!(rows.len(),1);
    for (cell,align) in rows[0].iter().zip(["l","r"]){assert_cell(cell,align,"112233",[0,0,44450,44450]);}
}

#[test]
fn style_sanitization_bounds_rules_and_zero_grid_width_stays_invisible(){
    let options=json!({"table":{"style":"three-line","borderColor":"#112233\"/><a:bad/>","borderWidth":999,"align":"right\" style=\"bad"}});let original=options.clone();
    let deck=build(SOURCE,"","Safe",&options).unwrap();let tables=native_tables(&deck);assert_eq!(options,original);let first_rows=rows(&tables[0]);
    for (cell,align) in first_rows[0].iter().zip(["l","ctr","r"]){assert_cell(cell,align,"C8CDD4",[0,0,76200,38100]);}
    assert!(!preview_html(&deck).unwrap().contains("<a:bad"));
    let grid=build(SOURCE,"","Zero",&json!({"table":{"style":"grid","borderWidth":0}})).unwrap();let tables=native_tables(&grid);
    for row in rows(&tables[0]){for (cell,align) in row.iter().zip(["l","ctr","r"]){assert_cell(cell,align,"C8CDD4",[0,0,0,0]);}}
}

#[test]
fn table_header_weight_and_width_match_preview_and_editable_grid() {
    let source = "# Slide\n\n| Plain | **Authored** |\n| --- | --- |\n| Regular | **Strong** |\n";
    for (width_pct, header_bold) in [(50, false), (75, true), (100, true)] {
        let options = json!({"table":{"widthPct":width_pct,"headerBold":header_bold}});
        let deck = build(source, "", "Slide", &options).unwrap();
        let table = &native_tables(&deck)[0];
        let preview = preview_html(&deck).unwrap();
        let width = WIDTH * f64::from(width_pct) / 100.;
        let left = (W - width) / 2.;
        let grid = child(table, "tblGrid");
        assert_eq!(grid.children().count(), 2);
        for col in grid.children() {
            assert_eq!(col.attributes["w"], emu(width / 2.).to_string());
        }
        assert!(preview.contains(&format!("left:{left}px;")));
        assert!(preview.contains(&format!("width:{width}px;")));
        for (row, expected) in rows(table).iter().zip([[header_bold, true], [false, true]]) {
            for (cell, bold) in row.iter().zip(expected) {
                let mut styles = Vec::new();
                descendants(cell, "rPr", &mut styles);
                assert_eq!(styles.len(), 1);
                assert_eq!(styles[0].attributes["b"], if bold { "1" } else { "0" });
                let text = cell.text();
                let fragment = preview.split_once(&format!(">{text}</span>")).unwrap().0;
                let run_style = fragment.rsplit_once("<span style=").unwrap().1;
                assert!(run_style.contains(if bold { "font-weight:700" } else { "font-weight:400" }));
            }
        }
    }
}

#[test]
fn pptx_table_track_size_fallback_keeps_all_editable_merged_text() {
    let source = concat!(
        "# Slide\n\n| Head {width=160} | Pair |\n| --- | --- |\n",
        "| One {colspan=2 rowspan=2} | Two |\n",
        "| Three {height=112} | Four |\n",
    );
    let options = settings("grid");
    let deck = build(source, "", "Slide", &options).unwrap();
    let warning = crate::api_codes::WARN_PUBLICATION_TABLE_TRACK_SIZE_FALLBACK;
    assert_eq!(deck.warnings.iter().filter(|value| value.as_str() == warning).count(), 1);
    let tables = native_tables(&deck);
    let rows = rows(&tables[0]);
    assert_eq!(rows[1][0].attributes["gridSpan"], "2");
    assert_eq!(rows[1][0].attributes["rowSpan"], "2");
    assert_eq!(rows[1][0].text(), "OneTwoThreeFour");
    assert!(rows[2][0].text().is_empty());
}

fn preview_tables(deck: &Deck) -> Vec<XmlNode> {
    let html = preview_html(deck).unwrap();
    html.split("<table ").skip(1).map(|part| {
        let (table, _) = part.split_once("</table>").unwrap();
        let root = crate::math_document::parse_xml(&format!("<table {table}</table>")).unwrap();
        let table = root.children().next().unwrap().clone();
        table
    }).collect()
}

fn preview_rows(table: &XmlNode) -> impl Iterator<Item = &XmlNode> {
    table.children().filter(|row| row.local() == "tr")
}

fn assert_row_colors(cells: &[&XmlNode], preview: &XmlNode, ink: &str, fill: Option<&str>) {
    assert_eq!(cells.len(), preview.children().count());
    for (cell, html) in cells.iter().zip(preview.children()) {
        assert_eq!(cell.text(), html.text());
        let properties = child(cell, "tcPr");
        let css = &html.attributes["style"];
        match fill {
            Some(color) => {
                assert_eq!(child(child(properties, "solidFill"), "srgbClr").attributes["val"], color);
                assert!(css.contains(&format!("background-color:#{color};color:#{ink};")), "{css}");
            }
            None => {
                child(properties, "noFill");
                assert!(!css.contains("background-color:"), "{css}");
            }
        }
        let text = child(cell, "txBody");
        child(text, "p");
        let mut runs = Vec::new();
        descendants(text, "rPr", &mut runs);
        for run in runs {
            assert_eq!(child(child(run, "solidFill"), "srgbClr").attributes["val"], ink);
        }
    }
}

fn assert_colors(deck: &Deck, expected: &[(&str, Option<&str>)]) {
    let native = native_tables(deck);
    let preview = preview_tables(deck);
    assert_eq!(native.len(), 1);
    assert_eq!(preview.len(), 1);
    let rows = rows(&native[0]);
    assert_eq!(rows.len(), expected.len());
    assert_eq!(preview_rows(&preview[0]).count(), expected.len());
    for ((cells, html), &(ink, fill)) in rows.iter().zip(preview_rows(&preview[0])).zip(expected) {
        assert_row_colors(cells, html, ink, fill);
    }
}

#[test]
fn editable_grid_colors_match_preview_and_banding_can_be_disabled() {
    let mut options = json!({
        "theme":"white",
        "table":{"style":"grid","headerBg":"#68225b","headerColor":"#f8ffd0","banded":true,"bandColor":"#e9f2fb"}
    });
    let deck = build(SOURCE, "", "Colors", &options).unwrap();
    assert_colors(&deck, &[
        ("F8FFD0", Some("68225B")),
        ("24292F", None),
        ("24292F", Some("E9F2FB")),
    ]);

    options["table"]["banded"] = json!(false);
    let deck = build(SOURCE, "", "Colors", &options).unwrap();
    assert_colors(&deck, &[
        ("F8FFD0", Some("68225B")),
        ("24292F", None),
        ("24292F", None),
    ]);
}

#[test]
fn presets_and_dark_slides_keep_band_ink_legible() {
    let business = crate::export_styles::preset_style("business");
    let minimal = crate::export_styles::preset_style("minimal");
    for (options, expected) in [
        (business, [("FFFFFF", Some("1F3864")), ("24292F", None), ("24292F", Some("EEF3FA"))]),
        (minimal, [("333333", Some("EEF1F5")), ("24292F", None), ("24292F", None)]),
        (json!({"pptxLayout":"launch"}), [("FFFFFF", Some("3B6EF5")), ("F1F5FF", None), ("000000", Some("F3F5F9"))]),
        (json!({"table":{"bandColor":"#171717"}}), [("FFFFFF", Some("3B6EF5")), ("24292F", None), ("FFFFFF", Some("171717"))]),
        (json!({"theme":"night","table":{"bandColor":"#171717"}}), [("FFFFFF", Some("3B6EF5")), ("F2EEE2", None), ("F2EEE2", Some("171717"))]),
    ] {
        let deck = build(SOURCE, "", "Preset", &options).unwrap();
        assert_colors(&deck, &expected);
    }
}

#[test]
fn three_line_tables_keep_slide_ink_and_restore_grid_colors_on_style_switch() {
    let mut options = json!({
        "theme":"night",
        "table":{"style":"three-line","headerBg":"#101820","headerColor":"#e8eedd","banded":true,"bandColor":"#eeeeee"}
    });
    let original = options.clone();
    let deck = build(SOURCE, "", "Three lines", &options).unwrap();
    assert_colors(&deck, &[("F2EEE2", None); 3]);
    assert_eq!(options, original);

    options["table"]["style"] = json!("grid");
    let deck = build(SOURCE, "", "Grid", &options).unwrap();
    assert_colors(&deck, &[
        ("E8EEDD", Some("101820")),
        ("F2EEE2", None),
        ("000000", Some("EEEEEE")),
    ]);
}

#[test]
fn invalid_table_colors_use_sanitized_hex_and_normalize_safe_case_and_newline() {
    let options = json!({"table":{
        "headerBg":"#100d1e\"/><a:bad/>",
        "headerColor":"#E5EEd3\n",
        "bandColor":"url(bad)"
    }});
    let deck = build(SOURCE, "", "Safe colors", &options).unwrap();
    assert_colors(&deck, &[
        ("E5EED3", Some("3B6EF5")),
        ("24292F", None),
        ("24292F", Some("F3F5F9")),
    ]);
}

#[test]
fn even_body_row_keeps_its_band_on_every_fragment_with_repeated_headers() {
    let long_cell = "banded fragment ".repeat(64);
    let source = format!(
        "# Color continuation\n\n| Label | Value |\n| --- | --- |\n| Before | Plain |\n| Wrapped | {long_cell} |\n| After | Plain |\n| Again | Band |\n"
    );
    let deck = build(&source, "", "Continuation", &json!({
        "table":{"widthPct":50,"headerBg":"#68225b","headerColor":"#f8ffd0","bandColor":"#e9f2fb"}
    })).unwrap();
    let native = native_tables(&deck);
    let preview = preview_tables(&deck);
    assert!(native.len() > 2);
    assert_eq!(native.len(), preview.len());
    let mut split_fragments = 0;
    for (table, html_table) in native.iter().zip(&preview) {
        let rows = rows(table);
        assert_eq!(rows.len(), preview_rows(html_table).count());
        for (index, (cells, html)) in rows.iter().zip(preview_rows(html_table)).enumerate() {
            let label = cells[0].text();
            let fill = if index == 0 {
                assert_eq!(label, "Label");
                assert_row_colors(cells, html, "F8FFD0", Some("68225B"));
                continue;
            } else if matches!(label.as_str(), "Wrapped" | "") {
                assert!(!cells[1].text().is_empty());
                split_fragments += 1;
                Some("E9F2FB")
            } else if label == "Again" {
                Some("E9F2FB")
            } else {
                assert!(matches!(label.as_str(), "Before" | "After"));
                None
            };
            assert_row_colors(cells, html, "24292F", fill);
        }
    }
    assert!(split_fragments > 1);
}

#[test]
fn merged_cells_keep_editable_grid_markers_and_all_original_paragraphs() {
    let source = concat!(
        "# Merged cells\n\n",
        "| Header A {colspan=2 align=center} | Header B | Header C | Header D |\n",
        "| :--- | :---: | ---: | :---: |\n",
        "| **Cell anchor** {colspan=2 rowspan=2 align=right valign=bottom} | [East text](https://example.test/source) | Left override {align=left valign=top} | Middle override {valign=middle} |\n",
        "| South text | Diagonal text | Visible C | Visible D |\n",
    );
    let deck = build(source, "", "Merged", &settings("three-line")).unwrap();
    let tables = native_tables(&deck);
    assert_eq!(tables.len(), 1);
    let table = &tables[0];
    let native = rows(table);
    assert_eq!(native.iter().map(Vec::len).collect::<Vec<_>>(), [4, 4, 4]);
    assert_eq!(child(table, "tblGrid").children().count(), 4);
    let header = native[0][0];
    assert_eq!(header.attributes["gridSpan"], "2");
    assert_eq!(header.text(), "Header AHeader B");
    assert_eq!(native[0][1].attributes["hMerge"], "1");

    let origin = native[1][0];
    assert_eq!(origin.attributes["rowSpan"], "2");
    assert_eq!(origin.attributes["gridSpan"], "2");
    assert_eq!(child(origin, "tcPr").attributes["anchor"], "b");
    let paragraphs: Vec<_> = child(origin, "txBody").children().filter(|node| node.local() == "p").collect();
    let expected = ["Cell anchor", "East text", "South text", "Diagonal text"];
    assert_eq!(paragraphs.iter().map(|paragraph| paragraph.text()).collect::<Vec<_>>(), expected);
    for paragraph in &paragraphs {
        assert_eq!(child(paragraph, "pPr").attributes["algn"], "r");
    }
    assert_eq!(child(child(child(paragraphs[0], "r"), "rPr"), "latin").attributes["typeface"], "Arial");
    assert_eq!(child(child(paragraphs[0], "r"), "rPr").attributes["b"], "1");
    child(child(child(paragraphs[1], "r"), "rPr"), "hlinkClick");

    for (cell, attrs) in [
        (native[1][1], [("rowSpan", "2"), ("hMerge", "1")]),
        (native[2][0], [("gridSpan", "2"), ("vMerge", "1")]),
        (native[2][1], [("hMerge", "1"), ("vMerge", "1")]),
    ] {
        assert_eq!(cell.attributes.len(), attrs.len());
        for (name, value) in attrs {
            assert_eq!(cell.attributes[name], value);
        }
        assert!(cell.text().is_empty());
    }
    assert_cell(native[1][2], "l", "112233", [0, 0, 0, 0]);
    assert_eq!(child(native[1][2], "tcPr").attributes["anchor"], "t");
    assert_cell(native[1][3], "ctr", "112233", [0, 0, 0, 0]);
    assert_eq!(child(native[1][3], "tcPr").attributes["anchor"], "ctr");
    assert_eq!(child(child(origin, "tcPr"), "lnB").attributes["w"], "44450");
    let body_heights: i64 = table.children().filter(|row| row.local() == "tr").skip(1)
        .map(|row| row.attributes["h"].parse::<i64>().unwrap()).sum();
    assert!(body_heights >= emu(4. * 25. + 16.));

    let tables = preview_tables(&deck);
    let visible: Vec<Vec<_>> = preview_rows(&tables[0]).map(|row| row.children().collect()).collect();
    assert_eq!(visible.iter().map(Vec::len).collect::<Vec<_>>(), [3, 3, 2]);
    assert_eq!(visible[0][0].attributes["colspan"], "2");
    assert_eq!(visible[0][0].text(), header.text());
    let origin = visible[1][0];
    assert_eq!(origin.attributes["rowspan"], "2");
    assert_eq!(origin.attributes["colspan"], "2");
    assert_eq!(origin.children().map(XmlNode::text).collect::<Vec<_>>(), expected);
    assert!(origin.attributes["style"].contains("vertical-align:bottom;"));
    assert!(origin.attributes["style"].contains("text-align:right;"));
    assert!(origin.attributes["style"].contains("border-bottom:3.5pt solid #112233;"));
    assert!(visible[1][1].attributes["style"].contains("text-align:left;"));
    assert!(visible[1][2].attributes["style"].contains("vertical-align:middle;"));
    assert!(!preview_html(&deck).unwrap().contains("{colspan="));
}

#[test]
fn page_break_keeps_connected_rows_and_repeats_merged_header() {
    let mut source = "# Continuation\n\n| Lead {colspan=2} | Label | Value |\n| --- | --- | --- |\n".to_string();
    for index in 0..7 {
        source.push_str(&format!("| Before {index} | Regular | {index} |\n"));
    }
    source.push_str(concat!(
        "| Span first {rowspan=2} | Other first | Extra first |\n",
        "| Span second | Other second {rowspan=2} | Extra second |\n",
        "| Remains | Other third | Extra third |\n",
    ));
    let deck = build(&source, "", "Continuation", &settings("grid")).unwrap();
    let tables = native_tables(&deck);
    assert_eq!(tables.len(), 2);
    let first = rows(&tables[0]);
    let second = rows(&tables[1]);
    assert_eq!(first.len(), 8);
    assert_eq!(second.len(), 4);
    assert_eq!(first[0][0].text(), "LeadLabel");
    assert_eq!(second[0][0].text(), "LeadLabel");
    assert_eq!(second[0][0].attributes["gridSpan"], "2");
    assert_eq!(second[1][0].text(), "Span firstSpan second");
    assert_eq!(second[2][0].attributes["vMerge"], "1");
    assert_eq!(second[2][1].text(), "Other secondOther third");
    assert_eq!(second[3][1].attributes["vMerge"], "1");
    let html = preview_tables(&deck);
    assert_eq!(html.len(), tables.len());
    assert_eq!(preview_rows(&html[1]).map(|row| row.children().count()).collect::<Vec<_>>(), [2, 3, 2, 2]);
}

#[test]
fn a_merge_larger_than_a_slide_refuses_before_replacing_pptx() {
    let mut tall = "# Oversized\n\n| A | B |\n| --- | --- |\n| Origin {rowspan=9} | First |\n".to_string();
    for index in 1..9 {
        tall.push_str(&format!("| Covered {index} | Visible {index} |\n"));
    }
    let wrapped = format!("# Oversized\n\n| A | B |\n| --- | --- |\n| {} {{colspan=2}} | Covered |\n", "a line of retained text ".repeat(160));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("existing.pptx");
    std::fs::write(&path, b"original bytes").unwrap();
    let options = json!({});
    for source in [&tall, &wrapped] {
        let unsupported = Some(crate::api_codes::PUBLICATION_TABLE_UNSUPPORTED);
        assert_eq!(preview(source, "", "Oversized", &options).err().as_deref(), unsupported);
        assert_eq!(export(source, "", path.to_str().unwrap(), &options, "Oversized").err().as_deref(), unsupported);
        assert_eq!(std::fs::read(&path).unwrap(), b"original bytes");
    }
}
