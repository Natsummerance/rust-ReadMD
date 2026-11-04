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
