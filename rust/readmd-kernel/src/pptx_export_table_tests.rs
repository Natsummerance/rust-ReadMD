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

fn preview_tables(deck: &Deck) -> Vec<XmlNode> {
    let html = preview_html(deck).unwrap();
    html.split("<table ").skip(1).map(|part| {
        let (table, _) = part.split_once("</table>").unwrap();
        let root = crate::math_document::parse_xml(&format!("<table {table}</table>")).unwrap();
        let table = root.children().next().unwrap().clone();
        table
    }).collect()
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
    assert_eq!(preview[0].children().count(), expected.len());
    for ((cells, html), &(ink, fill)) in rows.iter().zip(preview[0].children()).zip(expected) {
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
        assert_eq!(rows.len(), html_table.children().count());
        for (index, (cells, html)) in rows.iter().zip(html_table.children()).enumerate() {
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
