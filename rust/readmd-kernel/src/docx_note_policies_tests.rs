use super::*;
use crate::math_document::XmlNode;
use serde_json::json;

fn part<'a>(package:&'a DocxPackage,name:&str)->&'a str{std::str::from_utf8(&package.parts.iter().find(|(key,_)|key==name).unwrap().1).unwrap()}
fn all<'a>(node:&'a XmlNode,name:&str,out:&mut Vec<&'a XmlNode>){if node.name==name{out.push(node);}for child in node.children(){all(child,name,out);}}
fn descendants<'a>(node:&'a XmlNode,name:&str)->Vec<&'a XmlNode>{let mut out=Vec::new();all(node,name,&mut out);out}
fn normal<'a>(root:&'a XmlNode,name:&str)->&'a XmlNode{root.children().find(|n|n.name==name&&!n.attributes.contains_key("w:type")).unwrap()}
fn options()->Value{json!({"footnotes":{"kinds":{"footnote":{"size":30,"sizeCs":31,"lineSpacing":{"mode":"exact","pt":14.25},"numberFormat":"chicago","start":0,"restart":"page","placement":"beneathText","separator":"none","continuationSeparator":"line","continuationNotice":"Foot continued","longNote":"keepParagraphs"},"endnote":{"size":4.5,"sizeCs":5,"lineSpacing":{"mode":"atLeast","pt":15.75},"numberFormat":"upperRoman","start":65535,"restart":"section","placement":"sectEnd","separator":"line","continuationSeparator":"none","continuationNotice":"End continued","longNote":"split"}},"kindByLabel":{"same-f":"footnote","same-e":"endnote"}}})}
fn source()->&'static str{"Body[^same-f], end[^same-e], foot again[^same-f].\n\n[^same-f]: Identical author text.\n\n[^same-e]: Identical author text.\n"}

#[test]
fn two_native_stories_have_independent_settings_styles_ids_and_repeat_identity(){
    let style=crate::export_styles::sanitize(&options());let package=build(&crate::md_ast::parse(source()),&style,"Mixed",&|_|None);assert!(package.warns.is_empty(),"{:?}",package.warns);
    let body=part(&package,"word/document.xml");assert_eq!(body.matches("<w:footnoteReference w:id=\"1\"/>").count(),2);assert_eq!(body.matches("<w:endnoteReference w:id=\"1\"/>").count(),1);
    assert!(body.contains("<w:footnotePr><w:pos w:val=\"beneathText\"/></w:footnotePr>"));
    let settings=crate::math_document::parse_xml(part(&package,"word/settings.xml")).unwrap();
    for(name,start,format,restart,pos)in[("w:footnotePr","0","chicago","eachPage",None),("w:endnotePr","65535","upperRoman","eachSect",Some("sectEnd"))]{
        let properties=descendants(&settings,name)[0];
        assert_eq!(properties.children().find(|n|n.name=="w:numStart").unwrap().attributes["w:val"],start);
        assert_eq!(properties.children().find(|n|n.name=="w:numFmt").unwrap().attributes["w:val"],format);
        assert_eq!(properties.children().find(|n|n.name=="w:numRestart").unwrap().attributes["w:val"],restart);
        assert_eq!(properties.children().find(|n|n.name=="w:pos").map(|n|n.attributes["w:val"].as_str()),pos);
    }
    let styles=crate::math_document::parse_xml(part(&package,"word/styles.xml")).unwrap();
    for(id,size,complex,line,rule)in[("FootnoteText","60","62","285","exact"),("EndnoteText","9","10","315","atLeast")]{
        let style=descendants(&styles,"w:style").into_iter().find(|n|n.attributes.get("w:styleId").is_some_and(|v|v==id)).unwrap();
        assert_eq!(descendants(style,"w:sz")[0].attributes["w:val"],size);assert_eq!(descendants(style,"w:szCs")[0].attributes["w:val"],complex);
        assert_eq!(descendants(style,"w:spacing")[0].attributes["w:line"],line);assert_eq!(descendants(style,"w:spacing")[0].attributes["w:lineRule"],rule);
    }
    for(kind,line,rule,size,complex)in[("footnote","285","exact","60","62"),("endnote","315","atLeast","9","10")]{
        let xml=crate::math_document::parse_xml(part(&package,&format!("word/{kind}s.xml"))).unwrap();let root=xml.children().next().unwrap();let note=normal(root,&format!("w:{kind}"));
        assert_eq!(note.attributes["w:id"],"1");assert!(note.xml().contains("Identical author text"));
        let spacing=descendants(note,"w:spacing")[0];assert_eq!(spacing.attributes["w:line"],line);assert_eq!(spacing.attributes["w:lineRule"],rule);
        assert!(descendants(note,"w:sz").iter().any(|n|n.attributes["w:val"]==size));assert!(descendants(note,"w:szCs").iter().any(|n|n.attributes["w:val"]==complex));
        let notice=root.children().find(|n|n.attributes.get("w:type").is_some_and(|t|t=="continuationNotice")).unwrap();assert_eq!(notice.attributes["w:id"],"2");
    }
    assert!(!part(&package,"word/footnotes.xml").contains("<w:separator/>"));assert!(part(&package,"word/footnotes.xml").contains("<w:continuationSeparator/>"));
    assert!(part(&package,"word/endnotes.xml").contains("<w:separator/>"));assert!(!part(&package,"word/endnotes.xml").contains("<w:continuationSeparator/>"));
    let types=part(&package,"[Content_Types].xml");assert!(types.contains("/word/footnotes.xml")&&types.contains("/word/endnotes.xml"));
    let relationships=part(&package,"word/_rels/document.xml.rels");assert!(relationships.contains("Target=\"footnotes.xml\"")&&relationships.contains("Target=\"endnotes.xml\""));
}

#[test]
fn legal_wide_flat_native_values_survive_sanitizing_and_do_not_shift_identity(){
    let doc=crate::md_ast::parse("Body[^n].\n\n[^n]: Note.\n");
    for start in [0,12000,65535]{
        let style=crate::export_styles::sanitize(&json!({"footnotes":{"size":30,"start":start,"numberFormat":"chicago"}}));
        assert_eq!(style["footnotes"]["start"],start);assert_eq!(style["footnotes"]["size"],30);assert_eq!(style["footnotes"]["numberFormat"],"chicago");
        let built=build(&doc,&style,"Native",&|_|None);assert!(built.warns.is_empty());
        assert!(part(&built,"word/settings.xml").contains(&format!("<w:numStart w:val=\"{start}\"/>")));
        assert!(part(&built,"word/settings.xml").contains("<w:numFmt w:val=\"chicago\"/>"));
        assert!(part(&built,"word/document.xml").contains("<w:footnoteReference w:id=\"1\"/>"));assert!(part(&built,"word/footnotes.xml").contains("<w:sz w:val=\"60\"/>"));
    }
}

#[test]
fn note_local_flow_preserves_kind_spacing_and_does_not_turn_keep_next_into_long_note(){
    let mut doc=crate::md_ast::parse(source());
    for block in &mut doc.blocks{if let Block::FootnoteDef{blocks,..}=block{let original=blocks.remove(0);blocks.insert(0,Block::Styled{attributes:json!({"paragraph":{"keepNext":true}}),block:Box::new(original)});}}
    let package=build(&doc,&options(),"Native",&|_|None);assert!(package.warns.is_empty(),"{:?}",package.warns);
    for(kind,keep,line,rule)in[("footnote","1","285","exact"),("endnote","0","315","atLeast")]{
        let root=crate::math_document::parse_xml(part(&package,&format!("word/{kind}s.xml"))).unwrap();let note=descendants(&root,&format!("w:{kind}")).into_iter().find(|n|!n.attributes.contains_key("w:type")).unwrap();
        assert_eq!(descendants(note,"w:keepNext")[0].attributes["w:val"],"1");assert_eq!(descendants(note,"w:keepLines")[0].attributes["w:val"],keep);
        let spacing=descendants(note,"w:spacing")[0];assert_eq!(spacing.attributes["w:line"],line);assert_eq!(spacing.attributes["w:lineRule"],rule);
    }
}

#[test]
fn inherited_native_spacing_outside_local_ui_range_still_applies_with_local_flow(){
    let doc=crate::md_ast::parse("Body[^n].\n\n[^n]: Note.\n");let mut doc=doc;
    for block in &mut doc.blocks{if let Block::FootnoteDef{blocks,..}=block{let p=blocks.remove(0);blocks.push(Block::Styled{attributes:json!({"paragraph":{"keepNext":true}}),block:Box::new(p)});}}
    let style=json!({"footnotes":{"kinds":{"footnote":{"lineSpacing":{"mode":"exact","pt":200}}},"kindByLabel":{"n":"footnote"}}});
    let package=build(&doc,&style,"Native",&|_|None);assert!(package.warns.is_empty(),"{:?}",package.warns);
    let root=crate::math_document::parse_xml(part(&package,"word/footnotes.xml")).unwrap();let note=descendants(&root,"w:footnote").into_iter().find(|n|!n.attributes.contains_key("w:type")).unwrap();
    assert_eq!(descendants(note,"w:spacing")[0].attributes["w:line"],"4000");assert_eq!(descendants(note,"w:spacing")[0].attributes["w:lineRule"],"exact");
}

#[test]
fn both_note_relationship_parts_own_their_links_and_cached_shared_media(){
    let png={let mut buffer=std::io::Cursor::new(Vec::new());image::DynamicImage::new_rgba8(2,2).write_to(&mut buffer,image::ImageFormat::Png).unwrap();buffer.into_inner()};
    let loader=|_:&str|Some(("png".into(),png.clone()));
    let doc=crate::md_ast::parse("![Body image](pixel.png) Body[^same-f] and[^same-e].\n\n[^same-f]: [Foot only](https://example.invalid/foot) ![Cached](pixel.png).\n\n[^same-e]: [End only](https://example.invalid/end) ![Cached](pixel.png).\n");
    let package=build(&doc,&options(),"Native",&loader);assert!(package.warns.is_empty(),"{:?}",package.warns);
    let foot=part(&package,"word/_rels/footnotes.xml.rels");let end=part(&package,"word/_rels/endnotes.xml.rels");
    assert!(foot.contains("https://example.invalid/foot"));assert!(!foot.contains("https://example.invalid/end"));assert!(end.contains("https://example.invalid/end"));assert!(!end.contains("https://example.invalid/foot"));
    assert!(foot.contains("media/image1.png"));assert!(end.contains("media/image1.png"));assert_eq!(package.parts.iter().filter(|(p,_)|p.starts_with("word/media/")).count(),1);
}

#[test]
fn changed_binding_and_unknown_number_format_stop_before_any_package_is_returned(){
    let doc=crate::md_ast::parse(source());let mut style=options();style["footnotes"]["kindByLabel"]["deleted-label"]=json!("endnote");let package=build(&doc,&style,"Native",&|_|None);assert!(package.parts.is_empty());assert!(package.warns.contains(&note_policy::BINDING_MISMATCH.into()));
    let mut style=options();style["footnotes"]["kinds"]["footnote"]["numberFormat"]=json!("unknown-native-number");let package=build(&doc,&style,"Native",&|_|None);assert!(package.parts.is_empty());assert!(package.warns.contains(&note_policy::INVALID.into()));
}

#[test]
fn native_notice_is_not_truncated_and_custom_format_is_xml_escaped(){
    let mut style=options();style["footnotes"]["kinds"]["endnote"]["continuationNotice"]=json!("N".repeat(500));style["footnotes"]["kinds"]["footnote"]["numberPattern"]=json!("A & \"B\"");
    let style=crate::export_styles::sanitize(&style);let package=build(&crate::md_ast::parse(source()),&style,"Native",&|_|None);assert!(package.warns.is_empty());
    assert!(part(&package,"word/endnotes.xml").contains(&"N".repeat(500)));assert!(part(&package,"word/settings.xml").contains("w:format=\"A &amp; &quot;B&quot;\""));
}
