use super::*;
use crate::math_document::XmlNode;

fn part<'a>(package:&'a DocxPackage,name:&str)->&'a str {
    std::str::from_utf8(&package.parts.iter().find(|(key,_)|key==name).unwrap().1).unwrap()
}
fn nodes<'a>(node:&'a XmlNode,name:&str,out:&mut Vec<&'a XmlNode>){
    if node.name==name{out.push(node);}for child in node.children(){nodes(child,name,out);}
}

#[test]
fn native_note_restart_separators_and_notices_keep_normal_note_ids_disjoint() {
    let doc=crate::md_ast::parse("<!-- readmd-section:v1 {\"id\":\"a\"} -->\n\nFirst[^a] and again[^a].\n\n<!-- readmd-section:v1 {\"id\":\"b\",\"break\":\"continuous\"} -->\n\nSecond[^b].\n\n[^a]: Alpha.\n\n[^b]: Beta.\n");
    for position in ["page","document"] {for (restart,native) in [("continuous","continuous"),("section","eachSect"),("page","eachPage")]{
        let style=crate::export_styles::sanitize(&serde_json::json!({"footnotes":{"position":position,"restart":restart,"start":27,"separator":"none","continuationSeparator":"line","continuationNotice":"Continued <next> & \"page\""}}));
        let package=build(&doc,&style,"Notes",&|_|None);
        assert!(package.warns.is_empty(),"{:?}",package.warns);
        let kind=if position=="page"{"footnote"}else{"endnote"};
        let settings=part(&package,"word/settings.xml");
        assert!(settings.contains(&format!("<w:numRestart w:val=\"{native}\"/>")));
        assert!(settings.contains("<w:numStart w:val=\"27\"/>"));
        assert!(settings.contains(&format!("<w:{kind} w:id=\"3\"/>")));
        let body=part(&package,"word/document.xml");
        assert_eq!(body.matches(&format!("<w:{kind}Reference w:id=\"1\"/>")).count(),2);
        assert_eq!(body.matches(&format!("<w:{kind}Reference w:id=\"2\"/>")).count(),1);
        let xml=part(&package,&format!("word/{kind}s.xml"));
        assert!(!xml.contains("<w:separator/>"));
        assert!(xml.contains("<w:continuationSeparator/>"));
        assert!(xml.contains("Continued &lt;next&gt; &amp; &quot;page&quot;"));
        let parsed=crate::math_document::parse_xml(xml).unwrap();let mut entries=Vec::new();nodes(&parsed,&format!("w:{kind}"),&mut entries);
        let normal=entries.iter().filter(|node|!node.attributes.contains_key("w:type")).map(|node|node.attributes["w:id"].as_str()).collect::<Vec<_>>();
        assert_eq!(normal,vec!["1","2"]);
        let notices=entries.iter().filter(|node|node.attributes.get("w:type").is_some_and(|value|value=="continuationNotice")).collect::<Vec<_>>();
        assert_eq!(notices.len(),1);assert_eq!(notices[0].attributes["w:id"],"3");
    }}
}

#[test]
fn note_flow_keeps_paragraphs_without_binding_an_entire_long_note_or_overriding_local_flow() {
    let fragment="<w:p><w:pPr><w:pStyle w:val=\"FootnoteText\"/><w:spacing w:after=\"0\"/></w:pPr><w:r><w:t>Default</w:t></w:r></w:p><w:p><w:pPr><w:keepLines w:val=\"0\"/></w:pPr><w:r><w:t>Explicit split</w:t></w:r></w:p>";
    assert_eq!(note_paragraph_flow(fragment,&serde_json::json!({})).unwrap(),fragment);
    let changed=note_paragraph_flow(fragment,&serde_json::json!({"footnotes":{"longNote":"keepParagraphs"}})).unwrap();
    assert!(changed.contains("<w:pStyle w:val=\"FootnoteText\"/><w:keepLines w:val=\"1\"/><w:spacing"));
    assert_eq!(changed.matches("<w:keepLines w:val=\"0\"/>").count(),1);
    assert!(!changed.contains("<w:keepNext"));
    let paragraphs=(0..180).map(|index|format!("    Long note paragraph {index:03}, with enough structure to remain independently editable.\n\n")).collect::<String>();
    let source=format!("Body[^a], repeated[^a].\n\n[^a]: Long note start.\n\n{paragraphs}");
    for policy in ["split","keepParagraphs"]{
        let package=build(&crate::md_ast::parse(&source),&serde_json::json!({"footnotes":{"longNote":policy}}),"Long",&|_|None);
        assert!(package.warns.is_empty());let notes=part(&package,"word/footnotes.xml");
        assert_eq!(notes.matches("Long note paragraph").count(),180);
        assert_eq!(notes.matches("<w:footnoteRef/>").count(),1);
        assert!(!notes.contains("<w:keepNext w:val=\"1\"/>"));
    }
}

#[test]
fn omitted_note_controls_preserve_native_legacy_default_settings() {
    let package=build(&crate::md_ast::parse("Text[^a].\n\n[^a]: A.\n"),&serde_json::json!({}),"Old",&|_|None);
    let settings=part(&package,"word/settings.xml");let notes=part(&package,"word/footnotes.xml");
    assert!(!settings.contains("<w:numRestart"));
    assert!(!notes.contains("continuationNotice"));
    assert!(notes.contains("<w:separator/>")&&notes.contains("<w:continuationSeparator/>"));
}
