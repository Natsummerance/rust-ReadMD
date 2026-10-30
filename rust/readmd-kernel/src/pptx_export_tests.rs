use super::*;
use std::fs;

fn strings(deck:&Deck)->String {
    deck.slides.iter().flat_map(|slide|&slide.shapes).map(|shape|match shape{
        Shape::Text{lines,..}=>lines.iter().map(|line|plain(line)).collect::<String>(),
        Shape::Table{rows,..}=>rows.iter().flatten().flatten().map(|line|plain(line)).collect(),
        Shape::Image{alt,..}=>alt.clone(),
    }).collect()
}
fn assert_bounds(deck:&Deck){
    for slide in &deck.slides {for shape in &slide.shapes{let rect=match shape{Shape::Text{rect,..}|Shape::Image{rect,..}|Shape::Table{rect,..}=>rect};
        assert!(rect[0]>=0.&&rect[1]>=0.&&rect[0]+rect[2]<=W+0.01&&rect[1]+rect[3]<=H+0.01,"{rect:?}");
    }}
}

#[test]
fn editable_powerpoint_has_real_parts_notes_links_and_native_table_not_html(){
    let source="# 中文研究报告\n\n**重要结论**与[来源](https://example.test/report)。\n\n- 可编辑条目\n\n<!-- note -->\n演讲者备注\n\n<!-- slide -->\n## 数据\n\n|项目|结果|\n|---|---|\n|观测|42|\n";
    let deck=build(source,"","Report",&json!({"theme":"white"})).unwrap();assert_eq!(deck.slides.len(),2);assert_bounds(&deck);
    assert!(strings(&deck).contains("重要结论"));assert!(strings(&deck).contains("可编辑条目"));assert!(deck.slides[0].notes.contains("演讲者备注"));
    let bytes=package(&deck,"中文研究报告").unwrap();assert_eq!(&bytes[..4],b"PK\x03\x04");
    let entries=crate::convert::parse_zip_entries(&bytes).unwrap();
    let part=|name:&str|String::from_utf8(entries.iter().find(|(path,_)|path==name).unwrap().1.clone()).unwrap();
    assert!(part("[Content_Types].xml").contains("presentationml.presentation.main+xml"));
    assert!(part("ppt/presentation.xml").contains("cx=\"12192000\" cy=\"6858000\""));
    assert!(part("ppt/slides/slide1.xml").contains("<a:t xml:space=\"preserve\">重要结论</a:t>"));
    assert!(part("ppt/slides/slide1.xml").contains("<a:buChar char=\"•\"/>"));
    assert!(part("ppt/slides/slide2.xml").contains("<a:tbl>"));
    assert!(part("ppt/slides/slide2.xml").contains("uri=\"http://schemas.openxmlformats.org/drawingml/2006/table\""));
    assert!(part("ppt/notesSlides/notesSlide1.xml").contains("演讲者备注"));
    assert!(part("ppt/slides/_rels/slide1.xml.rels").contains("Target=\"https://example.test/report\" TargetMode=\"External\""));
    for (path,data) in entries.iter().filter(|(path,_)|path.ends_with(".xml")||path.ends_with(".rels")){
        crate::math_document::parse_xml(std::str::from_utf8(data).unwrap()).unwrap_or_else(|error|panic!("{path}: {error}"));
    }
}

#[test]
fn long_body_and_code_continue_without_losing_characters_or_becoming_tiny(){
    let body=(0..90).map(|n|format!("段落{n:03}正文，不应被裁剪。\n\n")).collect::<String>();
    let code=(0..110).map(|n|format!("let item_{n:03} = \"complete source\";\n")).collect::<String>();
    let source=format!("# Overflow\n\n{body}\n```rust\n{code}```");
    let deck=build(&source,"","Overflow",&json!({})).unwrap();assert!(deck.slides.len()>5);assert_bounds(&deck);
    let text=strings(&deck);for n in 0..90{assert!(text.contains(&format!("段落{n:03}正文，不应被裁剪。")));}for n in 0..110{assert!(text.contains(&format!("item_{n:03}")));}
    assert!(deck.slides.iter().flat_map(|slide|&slide.shapes).all(|shape|!matches!(shape,Shape::Text{size,..}if *size<18.)));
}

#[test]
fn automatic_source_chunks_keep_the_section_heading_on_all_continuation_slides(){
    let source=format!("# Actual heading\n\n{}",(0..60).map(|n|format!("Record {n:03}: all words survive and the heading stays associated with this section.\n\n")).collect::<String>());
    let deck=build(&source,"","Different filename",&json!({})).unwrap();assert!(deck.slides.len()>5);
    for slide in &deck.slides {let Shape::Text{lines,..}=&slide.shapes[0]else{panic!("missing heading");};assert_eq!(plain(&lines[0]),"Actual heading");}
}

#[test]
fn table_rows_repeat_headers_and_preserve_all_cells_on_continuation(){
    let mut source="# Long table\n\n|Observation|Result|\n|---|---|\n".to_string();for n in 0..60{source.push_str(&format!("|row-{n:03}|value-{n:03}|\n"));}
    let deck=build(&source,"","Data",&json!({})).unwrap();assert!(deck.slides.len()>3);assert_bounds(&deck);
    let text=strings(&deck);for n in 0..60{assert!(text.contains(&format!("row-{n:03}")));assert!(text.contains(&format!("value-{n:03}")));}
    for shape in deck.slides.iter().flat_map(|s|&s.shapes){if let Shape::Table{rows,..}=shape{assert_eq!(plain(&rows[0][0][0]),"Observation");}}
}

#[test]
fn literal_separators_and_headings_in_fences_remain_editable_code(){
    for fence in ["```","~~~~","````"]{
        let source=format!("# Code\n\n{fence}md\n## literal heading\n---\n<!-- slide -->\n{fence}\n\n---\n\n# Last\n\nFinal.");
        let deck=build(&source,"","Test",&json!({})).unwrap();assert_eq!(deck.slides.len(),2,"{fence}");let text=strings(&deck);assert!(text.contains("## literal heading"));assert!(text.contains("<!-- slide -->"));assert!(text.contains("Final."));
    }
}

#[test]
fn markdown_quote_and_entities_are_not_exported_as_literal_markup(){
    let deck=build("# Research & development\n\n> **A & B** remain a quotation.\n","","Deck",&json!({})).unwrap();
    let body=strings(&deck);assert!(body.contains("Research & development"));assert!(body.contains("A & B"));assert!(!body.contains("&amp;"));assert!(!body.contains('>'));
    assert!(matches!(deck.slides[0].shapes[0],Shape::Text{center:true,size:44.,..}));
}

#[test]
fn original_image_bytes_are_embedded_and_missing_images_do_not_create_output(){
    let dir=tempfile::tempdir().unwrap();let image=dir.path().join("素材.png");let out=dir.path().join("演示.pptx");
    let mut data=std::io::Cursor::new(Vec::new());image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(8,4,image::Rgba([240,30,20,255]))).write_to(&mut data,image::ImageFormat::Png).unwrap();let bytes=data.into_inner();fs::write(&image,&bytes).unwrap();
    let source="# Image\n\n![图像](素材.png)";let deck=build(source,dir.path().to_str().unwrap(),"Image",&json!({})).unwrap();assert_eq!(deck.media.len(),1);assert_eq!(deck.media[0].bytes,bytes);assert_bounds(&deck);
    if let Shape::Image{rect,..}=deck.slides[0].shapes.last().unwrap(){assert!((rect[2]/rect[3]-2.).abs()<0.0001);}else{panic!("image missing");}
    fs::remove_file(image).unwrap();assert!(export(source,dir.path().to_str().unwrap(),out.to_str().unwrap(),&json!({}),"Image").is_err());assert!(!out.exists());
}

#[test]
fn vector_images_keep_original_svg_and_a_real_png_fallback(){
    let dir=tempfile::tempdir().unwrap();let svg=br##"<svg xmlns="http://www.w3.org/2000/svg" width="160" height="80"><path d="M10 60 L80 10 L150 60" stroke="#2255CC" fill="none" stroke-width="5"/></svg>"##;
    fs::write(dir.path().join("diagram.svg"),svg).unwrap();let deck=build("# Vector\n\n![Diagram](diagram.svg)",dir.path().to_str().unwrap(),"Vector",&json!({})).unwrap();
    assert_eq!(deck.media[0].svg.as_deref(),Some(svg.as_slice()));assert!(image::load_from_memory(&deck.media[0].bytes).is_ok());
    let entries=crate::convert::parse_zip_entries(&package(&deck,"Vector").unwrap()).unwrap();assert!(entries.iter().any(|(name,bytes)|name=="ppt/media/image0.svg"&&bytes==svg));
    let slide=entries.iter().find(|(name,_)|name=="ppt/slides/slide1.xml").unwrap();let xml=std::str::from_utf8(&slide.1).unwrap();assert!(xml.contains("asvg:svgBlip"));assert!(xml.contains("r:embed=\"image0\""));assert!(preview_html(&deck).unwrap().contains("data:image/svg+xml;base64,"));
}

#[test]
fn unsupported_html_and_cancel_keep_existing_output_untouched(){
    let dir=tempfile::tempdir().unwrap();let out=dir.path().join("existing.pptx");fs::write(&out,b"original bytes").unwrap();
    assert!(export("# Test\n\n<video src=\"movie.mp4\"></video>","",out.to_str().unwrap(),&json!({}),"Test").is_err());assert_eq!(fs::read(&out).unwrap(),b"original bytes");
    crate::speech::with_cancel(std::sync::Arc::new(||true),||{assert_eq!(export("# Test","",out.to_str().unwrap(),&json!({}),"Test").err().as_deref(),Some("cancelled"));});assert_eq!(fs::read(out).unwrap(),b"original bytes");
}
