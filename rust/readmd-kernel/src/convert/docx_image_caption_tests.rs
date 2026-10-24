//! Real native image/caption roundtrips. External Word/process acceptance stays
//! with the stdio suite; these tests never simulate a successful engine.
use super::*;
use crate::md_ast::{self, Block, Document, Inline, ListItem};
use serde_json::{json, Value};
use std::collections::HashSet;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const PIC: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
// A complete 1x1 RGBA8 PNG, independently checked chunk CRCs and decoded IDAT.
const PNG: &[u8] = &[137,80,78,71,13,10,26,10,0,0,0,13,73,72,68,82,0,0,0,1,0,0,0,1,8,6,0,0,0,31,21,196,137,0,0,0,13,73,68,65,84,120,218,99,96,96,96,248,15,0,1,4,1,0,128,187,209,91,0,0,0,0,73,69,78,68,174,66,96,130];

fn child<'a>(node: &'a XmlNode, name: &str) -> Option<&'a XmlNode> {
    node.children.iter().find(|child| child.name == name)
}
fn root(parts: &[(String, Vec<u8>)], part: &str) -> XmlNode {
    docx_notes::xml(docx_notes::unique(parts, part).unwrap()).unwrap()
}
fn text(node: &XmlNode) -> String {
    std::iter::once(node).chain(node.descendants()).filter(|node|node.name=="w:t")
        .map(|node|node.text.as_str()).collect()
}
fn image(alt: &str) -> Block {
    Block::Paragraph(vec![Inline::Image { src: "pixel.png".into(), alt: alt.into(), title: String::new() }])
}
fn caption() -> Block {
    Block::Caption { label: crate::cross_references::Label {
        id: "fig:local".into(), kind: "figure".into(), number: 1, prefix: "Figure".into(), numbering: None,
    }, inlines: vec![Inline::Text(" — Local caption".into())] }
}
fn styled(block: Block) -> Block {
    Block::Styled { attributes: json!({"paragraph":{"keepNext":true}}), block: Box::new(block) }
}
fn section(id: &str) -> Block {
    Block::SectionStart(serde_json::from_value(json!({"id":id,"break":"continuous"})).unwrap())
}
fn request() -> Value {
    json!({"typography":{"font":"Arial","size":12},"frontMatter":{"mode":"none"},
        "cover":{"enabled":false},"toc":{"enabled":false},"footer":{"pageNumbers":false}})
}
fn build_doc(doc: &Document) -> crate::docx_writer::DocxPackage {
    let package = crate::docx_writer::build(doc, &request(), "image-caption.md", &|_|Some(("png".into(), PNG.to_vec())));
    assert!(!package.parts.is_empty(), "{:?}", package.warns);
    assert!(package.warns.is_empty(), "{:?}", package.warns);
    package
}
fn build_source(source: &str, base: &Path, options: &Value) -> crate::docx_writer::DocxPackage {
    let resolved = crate::publication_profile::resolve_for_format("docx", source, options).unwrap();
    let style = crate::export_styles::sanitize_options(Some(resolved.values()));
    let (doc, warnings) = crate::mdexport::publication_document(source, base.to_str().unwrap(), resolved.values()).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let images = |reference: &str| {
        let reference = percent_encoding::percent_decode_str(reference).decode_utf8().ok()?;
        Some(("png".into(), fs::read(base.join(reference.as_ref())).ok()?))
    };
    let mut package = crate::docx_writer::build_with_lines(&doc, &style, "image-caption.md", &images, resolved.line_context());
    assert!(!package.parts.is_empty(), "{:?}", package.warns);
    assert!(package.warns.is_empty(), "{:?}", package.warns);
    docx_numbering_import::attach_source_info(&mut package.parts, source).unwrap();
    package
}
fn native_visual_alts(parts: &[(String, Vec<u8>)]) -> Vec<String> {
    root(parts,"word/document.xml").descendants().into_iter().filter(|node|node.name=="w:p")
        .filter(|paragraph|child(paragraph,"w:pPr").and_then(|pr|child(pr,"w:pStyle")).and_then(|style|style.attr("w:val"))==Some("Caption"))
        .filter(|paragraph|!paragraph.descendants().iter().any(|node|matches!(node.name.as_str(),"w:fldSimple"|"w:fldChar")))
        .map(text).collect()
}
fn description_values(parts: &[(String, Vec<u8>)]) -> Vec<Option<String>> {
    root(parts,"word/document.xml").descendants().into_iter().filter(|node|node.name=="wp:docPr")
        .map(|node|node.attr("descr").map(str::to_string)).collect()
}

// This native-size oracle reads OOXML defaults/style chains/direct properties;
// it does not use RunStyleIndex, the guard under test, or writer declarations.
fn native_axes(styles: &XmlNode, paragraph: &XmlNode, run: &XmlNode) -> [Option<u32>; 2] {
    fn overlay(pair: &mut [Option<u32>; 2], properties: Option<&XmlNode>) {
        for (index, name) in ["w:sz","w:szCs"].into_iter().enumerate() {
            if let Some(raw) = properties.and_then(|properties|child(properties,name)).and_then(|size|size.attr("w:val")) {
                pair[index] = Some(raw.parse().unwrap());
            }
        }
    }
    fn style(pair: &mut [Option<u32>; 2], styles: &XmlNode, id: &str, seen: &mut HashSet<String>) {
        assert!(seen.insert(id.into()), "cyclic native test style");
        let definition = styles.children.iter().find(|node|node.name=="w:style"&&node.attr("w:styleId")==Some(id)).unwrap();
        if let Some(base)=child(definition,"w:basedOn").and_then(|base|base.attr("w:val")) { style(pair,styles,base,seen); }
        overlay(pair,child(definition,"w:rPr"));
    }
    let mut pair = [None,None];
    overlay(&mut pair,child(styles,"w:docDefaults").and_then(|defaults|child(defaults,"w:rPrDefault")).and_then(|defaults|child(defaults,"w:rPr")));
    if let Some(id)=child(paragraph,"w:pPr").and_then(|properties|child(properties,"w:pStyle")).and_then(|value|value.attr("w:val")) { style(&mut pair,styles,id,&mut HashSet::new()); }
    if let Some(id)=child(run,"w:rPr").and_then(|properties|child(properties,"w:rStyle")).and_then(|value|value.attr("w:val")) { style(&mut pair,styles,id,&mut HashSet::new()); }
    overlay(&mut pair,child(run,"w:rPr")); pair
}
fn assert_native_images(parts: &[(String, Vec<u8>)], expected: &[&str]) {
    let document = root(parts,"word/document.xml");
    let relations = root(parts,"word/_rels/document.xml.rels");
    let pictures = document.descendants().into_iter().filter(|node|node.name=="wp:inline").collect::<Vec<_>>();
    assert_eq!(pictures.len(),expected.len());
    for (picture,alt) in pictures.into_iter().zip(expected) {
        assert_eq!(child(picture,"wp:docPr").unwrap().attr("descr"),Some(*alt));
        let nv = picture.descendants().into_iter().find(|node|node.name=="pic:cNvPr").unwrap();
        assert_eq!(nv.attr("descr"),Some(*alt));
        let blips = picture.descendants().into_iter().filter(|node|node.name=="a:blip").collect::<Vec<_>>();assert_eq!(blips.len(),1);
        let id = blips[0].attr("r:embed").unwrap();
        let matching = relations.children.iter().filter(|node|node.attr("Id")==Some(id)).collect::<Vec<_>>();assert_eq!(matching.len(),1);
        let rel = matching[0];assert_eq!(rel.attr("Type"),Some(format!("{R}/image").as_str()));assert!(rel.attr("TargetMode").is_none());
        let member = docx_notes::member("word",rel.attr("Target").unwrap()).unwrap();
        let bytes = docx_notes::unique(parts,&member).unwrap();assert_eq!(bytes,PNG);
        assert_eq!(format!("{:x}",Sha256::digest(bytes)),"dc5687b50f70cb95379bfced5a0eae768dd4382cd6b393ee77d65bbdd6373fbf");
    }
}
fn assert_numbered_figures(parts: &[(String, Vec<u8>)]) {
    let document = root(parts,"word/document.xml");let styles = root(parts,"word/styles.xml");
    let body = child(&document,"w:body").unwrap();
    for (id,chapter) in [("fig:a","1"),("fig:b","2")] {
        let bookmark = crate::docx_writer::bookmark_name(id);
        let paragraphs=body.children.iter().filter(|node|node.name=="w:p"&&node.children.iter().any(|node|node.name=="w:bookmarkStart"&&node.attr("w:name")==Some(bookmark.as_str()))).collect::<Vec<_>>();assert_eq!(paragraphs.len(),1);
        let paragraph=paragraphs[0];assert_eq!(text(paragraph),format!("Figure {chapter}-1: Same title"));
        let fields=paragraph.children.iter().filter(|node|node.name=="w:fldSimple").collect::<Vec<_>>();assert_eq!(fields.len(),2);
        assert_eq!(fields[0].attr("w:instr").unwrap().split_whitespace().collect::<Vec<_>>(),["STYLEREF","1","\\s"]);assert_eq!(text(fields[0]),chapter);
        assert_eq!(fields[1].attr("w:instr").unwrap().split_whitespace().collect::<Vec<_>>(),["SEQ","ReadMDFigure","\\s","1","\\*","ARABIC"]);assert_eq!(text(fields[1]),"1");
        let contributors=paragraph.descendants().into_iter().filter(|node|node.name=="w:r"&&!text(node).is_empty()).collect::<Vec<_>>();assert!(!contributors.is_empty());
        for run in contributors { assert_eq!(native_axes(&styles,paragraph,run),[Some(18),Some(24)]); }
        let references=body.children.iter().filter(|node|node.name=="w:p").flat_map(|paragraph|paragraph.descendants().into_iter().map(move |node|(paragraph,node)))
            .filter(|(_,node)|node.name=="w:fldSimple"&&node.attr("w:instr").is_some_and(|code|code.split_whitespace().collect::<Vec<_>>()==["REF",bookmark.as_str(),"\\h"])).collect::<Vec<_>>();assert_eq!(references.len(),1);
        let (paragraph,reference)=references[0];assert_eq!(text(reference),format!("Figure {chapter}-1"));
        for run in reference.descendants().into_iter().filter(|node|node.name=="w:r"&&!text(node).is_empty()) { assert_eq!(native_axes(&styles,paragraph,run),[Some(24),Some(24)]); }
    }
    assert!(native_visual_alts(parts).is_empty());assert_native_images(parts,&["One","Two"]);
}
fn source_identities(source: &str) {
    let doc = md_ast::parse_prepared(source,&crate::publication_rich::Projection::new(source)).unwrap();
    let captions=doc.blocks.iter().filter_map(|block|if let Block::Caption {label,..}=block.unstyled(){Some(label)}else{None}).collect::<Vec<_>>();assert_eq!(captions.len(),2);
    assert_eq!(captions.iter().map(|label|label.id.as_str()).collect::<Vec<_>>(),["fig:a","fig:b"]);
    let mut alts=Vec::new();let mut references=Vec::new();
    fn inspect(nodes: &[Inline], alts: &mut Vec<String>, references: &mut Vec<String>) {
        for inline in nodes { match inline {
            Inline::Image{alt,..}=>alts.push(alt.clone()),
            Inline::Link{href,children,..}=>{references.push(href.clone());inspect(children,alts,references);},
            Inline::AuthorStyled{children,..}|Inline::Styled{children,..}|Inline::Display{children,..}|Inline::Citation{children,..}|Inline::Strong(children)|Inline::Emph(children)|Inline::Strike(children)=>inspect(children,alts,references),
            _=>(),
        }}
    }
    for block in &doc.blocks { if let Block::Paragraph(inlines)=block.unstyled(){inspect(inlines,&mut alts,&mut references);} }
    assert_eq!(alts,["One","Two"]);assert_eq!(references,["#fig:a","#fig:b"]);
    assert!(doc.blocks.iter().any(|block|matches!(block.unstyled(),Block::Paragraph(inlines) if md_ast::inline_plain(inlines)=="Final body guard.")));
    assert_eq!(crate::publication_profile::from_source(source).unwrap().options["typography"]["size"].as_f64(),Some(12.0));
}

#[test]
fn native_two_chapter_real_png_figures_reopen_with_current_fields_ids_axes_and_alt() {
    let temp = tempfile::tempdir().unwrap();fs::write(temp.path().join("pixel.png"),PNG).unwrap();
    let source = "---\ntitle: Image caption native return\nlang: en-US\n---\n\n<!-- readmd-numbering:v1 {\"headings\":{\"levels\":[{}]},\"captions\":{\"figure\":{\"scope\":\"chapter\",\"separator\":\"-\",\"captionSeparator\":\": \"}}} -->\n\n# Alpha {#alpha}\n\n![One](pixel.png)\n\n:label[fig:a] Same title\n\n# Beta {#beta}\n\n![Two](pixel.png)\n\n:label[fig:b] Same title\n\nSee :ref[fig:a] and :ref[fig:b].\n\nFinal body guard.\n";
    let first = build_source(source,temp.path(),&request());assert_numbered_figures(&first.parts);
    let before = first.parts.clone();let imported = docx_convert(&first.parts,&temp.path().join("first.docx"),false).unwrap();assert_eq!(first.parts,before);source_identities(&imported);
    // The source carries the body baseline, not an out-of-band export preset.
    let tx = crate::document_transaction::ContentTransaction {base_revision:crate::document_transaction::revision(&imported),origin:"mcp".into(),edits:vec![]};
    let saved = crate::document_transaction::apply(&imported,&tx).unwrap();assert!(!saved.changed);assert_eq!(saved.content,imported);
    let path=temp.path().join("ordinary.md");crate::content::write_text_atomic(&path,&saved.content).unwrap();
    let reopened=fs::read_to_string(&path).unwrap();assert_eq!(reopened,imported);
    let second=build_source(&reopened,temp.path(),&json!({}));assert_numbered_figures(&second.parts);
    let again=docx_convert(&second.parts,&temp.path().join("second.docx"),false).unwrap();source_identities(&again);
}

#[test]
fn current_native_alt_overrides_old_source_identity_metadata_without_activating_markup() {
    let temp=tempfile::tempdir().unwrap();fs::write(temp.path().join("pixel.png"),PNG).unwrap();
    let source="![Stored](pixel.png)\n\n:label[fig:local] Local caption\n";
    let mut package=build_source(source,temp.path(),&request());
    let current="Current [alt] & <script> $x$ :math[x^2] 🦊\r\nline2\ttab literal &#10;";
    let (_,bytes)=package.parts.iter_mut().find(|(part,_)|part=="word/document.xml").unwrap();
    let word=String::from_utf8(bytes.clone()).unwrap();assert_eq!(word.matches("descr=\"Stored\"").count(),2);
    let attribute=crate::docx_writer::xml_escape(current).replace('\t',"&#9;").replace('\r',"&#13;").replace('\n',"&#10;");
    *bytes=word.replace("descr=\"Stored\"",&format!("descr=\"{attribute}\"")).into_bytes();
    let imported=docx_convert(&package.parts,&temp.path().join("changed.docx"),false).unwrap();
    let parsed=md_ast::parse_prepared(&imported,&crate::publication_rich::Projection::new(&imported)).unwrap();
    let images=parsed.blocks.iter().filter_map(|block|match block.unstyled(){Block::Paragraph(inlines)=>match inlines.as_slice(){[Inline::Image {alt,..}]=>Some(alt.as_str()),_=>None},_=>None}).collect::<Vec<_>>();
    assert_eq!(images,[current]);assert!(!imported.contains("<script>"));
    let returned=build_source(&imported,temp.path(),&json!({}));assert_eq!(description_values(&returned.parts),[Some(current.into())]);assert!(native_visual_alts(&returned.parts).is_empty());
}

#[test]
fn old_double_caption_package_is_not_reconnected_across_its_content_barrier() {
    let temp=tempfile::tempdir().unwrap();fs::write(temp.path().join("pixel.png"),PNG).unwrap();
    let source="![Stored](pixel.png)\n\n:label[fig:local] Local caption\n";
    let mut package=build_source(source,temp.path(),&request());
    let (_,bytes)=package.parts.iter_mut().find(|(part,_)|part=="word/document.xml").unwrap();
    let word=String::from_utf8(bytes.clone()).unwrap();
    let caption_start="<w:p><w:pPr><w:pStyle w:val=\"Caption\"/><w:jc w:val=\"center\"/><w:keepNext w:val=\"0\"/></w:pPr>";
    assert_eq!(word.matches(caption_start).count(),1);
    let barrier="<w:p><w:pPr><w:pStyle w:val=\"Caption\"/></w:pPr><w:r><w:t>Stored</w:t></w:r></w:p>";
    *bytes=word.replacen(caption_start,&format!("{barrier}{caption_start}"),1).into_bytes();
    let original=package.parts.clone();
    let error=docx_convert(&package.parts,&temp.path().join("old-double.docx"),false).unwrap_err();
    assert!(error.starts_with(crate::publication_inline_text::UNSUPPORTED),"{error}");assert!(error.contains("author-sized live field"));assert_eq!(package.parts,original);
}

#[test]
fn writer_suppresses_only_the_immediate_figure_caption_and_respects_styled_and_legacy_images() {
    for blocks in [vec![image("Legacy")],vec![image("Legacy"),Block::Paragraph(vec![Inline::Text("Author prose".into())])]] {
        let output=build_doc(&Document {front_matter:None,blocks});assert_eq!(native_visual_alts(&output.parts),["Legacy"]);assert_eq!(description_values(&output.parts),[Some("Legacy".into())]);
    }
    for blocks in [vec![image(""),caption()],vec![image("Named"),caption()],vec![styled(image("Named")),styled(caption())]] {
        let output=build_doc(&Document {front_matter:None,blocks});assert!(native_visual_alts(&output.parts).is_empty());assert_eq!(description_values(&output.parts).len(),1);
    }
    let mut table_caption=caption();let Block::Caption {label,..}=&mut table_caption else{unreachable!()};label.kind="table".into();label.id="tbl:wrong-kind".into();
    let output=build_doc(&Document {front_matter:None,blocks:vec![image("Wrong kind"),table_caption]});assert_eq!(native_visual_alts(&output.parts),["Wrong kind"]);
    let output=build_doc(&Document {front_matter:None,blocks:vec![image("First"),caption(),image("Second")]});assert_eq!(native_visual_alts(&output.parts),["Second"]);assert_eq!(description_values(&output.parts),[Some("First".into()),Some("Second".into())]);
}

#[test]
fn image_caption_context_never_crosses_author_paragraphs_sections_or_nested_siblings() {
    let quote=Block::Quote(vec![caption()]);
    let cases=vec![
        vec![image("Barrier"),Block::Paragraph(vec![Inline::Text("Authored barrier".into())]),caption()],
        vec![section("first"),image("Barrier"),section("second"),caption()],
        vec![image("Barrier"),quote],
        vec![Block::Quote(vec![image("Barrier")]),caption()],
        vec![Block::List {ordered:false,start:1,items:vec![
            ListItem {task:None,blocks:vec![Block::Paragraph(vec![Inline::Text("List intro".into())]),image("Barrier")]},
            ListItem {task:None,blocks:vec![caption()]},
        ]}],
    ];
    for blocks in cases { let output=build_doc(&Document {front_matter:None,blocks});assert_eq!(native_visual_alts(&output.parts),["Barrier"]);assert_eq!(description_values(&output.parts),[Some("Barrier".into())]); }
}

fn drawing_xml(body: &str) -> XmlNode {
    docx_notes::xml(format!(r#"<w:drawing xmlns:w="{W}" xmlns:a="{A}" xmlns:wp="{WP}" xmlns:pic="{PIC}" xmlns:r="{R}">{body}</w:drawing>"#).as_bytes()).unwrap()
}
fn described(description: Option<&str>, picture: Option<&str>, id: &str) -> String {
    let descr=|value:Option<&str>|value.map(|value|format!(" descr=\"{}\"",crate::docx_writer::xml_escape(value).replace('\t',"&#9;").replace('\r',"&#13;").replace('\n',"&#10;"))).unwrap_or_default();
    format!(r#"<wp:inline><wp:docPr{}/><a:graphic><a:graphicData><pic:pic><pic:nvPicPr><pic:cNvPr{}/></pic:nvPicPr><pic:blipFill><a:blip r:embed="{id}"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline>"#,descr(description),descr(picture))
}
fn scoped_descriptions(drawing: &XmlNode) -> Vec<(String, Option<String>)> {
    docx_drawing_images(drawing).unwrap().into_iter().map(|(blip,alt)|(blip.attr("r:embed").unwrap().into(),alt.map(str::to_string))).collect()
}

#[test]
fn current_drawing_description_priority_keeps_empty_and_missing_distinct_per_picture() {
    let fragments=[described(Some("First"),Some("Fallback1"),"one"),described(Some(""),Some("Never"),"empty"),described(None,Some("Picture fallback"),"fallback"),described(None,None,"missing")].join("");
    let drawing=drawing_xml(&fragments);
    assert_eq!(scoped_descriptions(&drawing),vec![("one".into(),Some("First".into())),("empty".into(),Some("".into())),("fallback".into(),Some("Picture fallback".into())),("missing".into(),None)]);
    let two_pictures=format!(r#"<wp:anchor><wp:docPr/><a:graphic><a:graphicData>{}{}</a:graphicData></a:graphic></wp:anchor>"#,
        r#"<pic:pic><pic:nvPicPr><pic:cNvPr descr="Own one"/></pic:nvPicPr><a:blip r:embed="one"/></pic:pic>"#,
        r#"<pic:pic><pic:nvPicPr><pic:cNvPr descr="Own two"/></pic:nvPicPr><a:blip r:embed="two"/></pic:pic>"#);
    assert_eq!(scoped_descriptions(&drawing_xml(&two_pictures)),vec![("one".into(),Some("Own one".into())),("two".into(),Some("Own two".into()))]);
}

#[test]
fn drawing_aliases_resolve_but_local_rebinding_and_foreign_attributes_cannot_claim_alt() {
    let alias=described(Some("Actual"),Some("Picture"),"native").replace("wp:","q:").replace("pic:","p:");
    let drawing=docx_notes::xml(format!(r#"<w:drawing xmlns:w="{W}" xmlns:a="{A}" xmlns:r="{R}" xmlns:q="{WP}" xmlns:p="{PIC}">{alias}</w:drawing>"#).as_bytes()).unwrap();
    assert_eq!(scoped_descriptions(&drawing),vec![("native".into(),Some("Actual".into()))]);
    let fake_wp=described(Some("FORGED"),Some("Real picture"),"safe").replace("<wp:docPr",r#"<wp:docPr xmlns:wp="urn:fake""#);
    assert_eq!(scoped_descriptions(&drawing_xml(&fake_wp)),vec![("safe".into(),Some("Real picture".into()))]);
    let fake_pic=described(None,Some("FORGED"),"safe").replace("<pic:cNvPr",r#"<pic:cNvPr xmlns:pic="urn:fake""#);
    assert_eq!(scoped_descriptions(&drawing_xml(&fake_pic)),vec![("safe".into(),None)]);
    let foreign_attribute=described(None,None,"safe").replace("<wp:docPr/>",r#"<wp:docPr xmlns:f="urn:fake" f:descr="FORGED"/>"#).replace("<pic:cNvPr/>",r#"<pic:cNvPr xmlns:f="urn:fake" f:descr="FORGED"/>"#);
    assert_eq!(scoped_descriptions(&drawing_xml(&foreign_attribute)),vec![("safe".into(),None)]);
    for fragment in [described(Some("A"),None,"safe").replace("<wp:docPr descr=\"A\"/>","<wp:docPr descr=\"A\"/><wp:docPr descr=\"B\"/>"),described(None,Some("A"),"safe").replace("<pic:cNvPr descr=\"A\"/>","<pic:cNvPr descr=\"A\"/><pic:cNvPr descr=\"B\"/>")] {
        assert_eq!(docx_drawing_images(&drawing_xml(&fragment)).unwrap_err(),docx_notes::INVALID);
    }
}

#[test]
fn native_alt_source_encoding_keeps_punctuation_entities_html_unicode_and_line_breaks_literal() {
    let values=["quote \" and ' [brackets] (parentheses) \\ slash", "&amp; &lt;literal&gt; <script>alert(1)</script>", "literal &#10; &#13; &#9; &amp;#10;", "$x$ :math[x^2] :ref[fig:fake] [link](https://example.invalid)", "Ελληνικά 中文 🦊", "line1\r\nline2\ttab", ""];
    for value in values {
        let drawing=drawing_xml(&described(Some(value),Some("NEVER"),"img"));let images=docx_drawing_images(&drawing).unwrap();assert_eq!(images.len(),1);assert_eq!(images[0].1,Some(value));
        let source=format!("![{}](pixel.png)",docx_image_alt_source(images[0].1));
        crate::publication_inline_text::validate_source(&source).unwrap();
        let document=md_ast::parse_prepared(&source,&crate::publication_rich::Projection::new(&source)).unwrap();
        let [Block::Paragraph(inlines)]=document.blocks.as_slice()else{panic!("alt activated a new source block: {source}")};
        let [Inline::Image {alt,src,..}]=inlines.as_slice()else{panic!("alt activated a non-image inline: {source}")};
        assert_eq!(alt,value);assert_eq!(src,"pixel.png");
        assert!(!source.contains("<script>"));
    }
    assert_eq!(docx_image_alt_source(None),"image");assert_eq!(docx_image_alt_source(Some("")),"");
}

#[test]
fn unknown_and_mixed_live_fields_keep_the_existing_precise_rejection_contract() {
    let temp=tempfile::tempdir().unwrap();
    let base=build_doc(&Document {front_matter:None,blocks:vec![Block::Paragraph(vec![Inline::Text("Field replacement sentinel".into())])]});
    let replace=|body:&str| {let mut parts=base.parts.clone();let (_,bytes)=parts.iter_mut().find(|(name,_)|name=="word/document.xml").unwrap();let original=String::from_utf8(bytes.clone()).unwrap();let start=original.find("<w:body>").unwrap()+"<w:body>".len();let end=original.rfind("<w:sectPr>").unwrap();*bytes=format!("{}{body}{}",&original[..start],&original[end..]).into_bytes();parts};
    for body in [
        r#"<w:p><w:fldSimple w:instr=" DATE "><w:r><w:rPr><w:sz w:val="60"/></w:rPr><w:t>Current date</w:t></w:r></w:fldSimple></w:p>"#,
        r#"<w:p><w:fldSimple w:instr=" REF unknown \h "><w:r><w:t>One</w:t></w:r><w:r><w:rPr><w:szCs w:val="60"/></w:rPr><w:t>Two</w:t></w:r></w:fldSimple></w:p>"#,
        r#"<w:tbl><w:tr><w:tc><w:p><w:fldSimple w:instr=" DATE "><w:r><w:rPr><w:sz w:val="60"/></w:rPr><w:t>Native table field</w:t></w:r></w:fldSimple></w:p></w:tc></w:tr></w:tbl>"#,
    ] {
        let parts=replace(body);let original=parts.clone();let error=docx_convert(&parts,&temp.path().join("guard.docx"),false).unwrap_err();assert!(error.starts_with(crate::publication_inline_text::UNSUPPORTED),"{error}");assert!(error.contains("author-sized live field"));assert_eq!(parts,original);
    }
}
