//! Package/domain checks. These deliberately do not claim Word interoperability.
use super::*;
use crate::publication_docx_citations::{self as domain, Manifest, Resources};
use serde_json::json;

fn style(class: &str) -> String {
    format!(
        r#"<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="{class}"><info><title>ReadMD citation return test</title><id>urn:readmd:test:citations-return</id></info><citation><layout prefix="(" suffix=")" delimiter="; "><group delimiter=":"><text variable="citation-number"/><names variable="author"><name form="short"/></names><text variable="locator"/></group></layout></citation><bibliography><layout><text variable="title"/><text variable="publisher" prefix=" / "/></layout></bibliography></style>"#
    )
}
fn prepare(class: &str, body: &str) -> (tempfile::TempDir, String, Vec<(String, Vec<u8>)>) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("refs.json"),r#"[{"id":42,"type":"book","title":"FIRST_TITLE","author":[{"family":"Alpha"}],"publisher":"PUB_A"},{"id":"same","type":"book","title":"SECOND_TITLE","author":[{"family":"Beta"}],"publisher":"PUB_B"}]"#).unwrap();
    fs::write(dir.path().join("house.csl"), style(class)).unwrap();
    let source=format!("---\nbibliography: refs.json\ncsl: house.csl\nlang: en-US\n---\n{body}\n\n<!--readmd:bibliography-->\n");
    let prepared = crate::publication::prepare(&source, dir.path().to_str().unwrap()).unwrap();
    let document = crate::md_ast::parse_prepared(&source, &prepared.projection).unwrap();
    let mut package = crate::docx_writer::build(
        &document,
        &json!({"frontMatter":{"mode":"none"}}),
        "citation-test.md",
        &|_| None,
    );
    assert!(!package.parts.is_empty(), "{:?}", package.warns);
    domain::attach_context(
        &mut package.parts,
        &source,
        dir.path().to_str().unwrap(),
        &json!({}),
    )
    .unwrap();
    (dir, source, package.parts)
}
fn manifest(parts: &[(String, Vec<u8>)]) -> Manifest {
    Manifest::parse(
        &parts
            .iter()
            .find(|(name, _)| name == domain::PART)
            .unwrap()
            .1,
    )
    .unwrap()
}
fn update_manifest(parts: &mut [(String, Vec<u8>)], change: impl FnOnce(&mut Manifest)) {
    let mut m = manifest(parts);
    change(&mut m);
    parts
        .iter_mut()
        .find(|(name, _)| name == domain::PART)
        .unwrap()
        .1 = m.xml().unwrap().into_bytes();
}
fn control_mut<'a>(node: &'a mut XmlNode, value: &str) -> Option<&'a mut XmlNode> {
    if tag(node) == Some(value) {
        return Some(node);
    }
    for child in &mut node.children {
        if let Some(found) = control_mut(child, value) {
            return Some(found);
        }
    }
    None
}
fn node_mut<'a>(node: &'a mut XmlNode, name: &str) -> Option<&'a mut XmlNode> {
    if node.name == name {
        return Some(node);
    }
    for child in &mut node.children {
        if let Some(found) = node_mut(child, name) {
            return Some(found);
        }
    }
    None
}
fn alter(parts: &mut [(String, Vec<u8>)], part: &str, f: impl FnOnce(&mut XmlNode)) {
    let bytes = &mut parts.iter_mut().find(|(name, _)| name == part).unwrap().1;
    let mut root = docx_notes::xml(bytes).unwrap();
    f(&mut root);
    *bytes = docx_xml_fragment(&root, &[]).into_bytes();
}
fn alter_render(parts: &mut [(String, Vec<u8>)], record: &Record, value: &str) {
    alter(parts, &record.part, |root| {
        let control = control_mut(root, &record.tag()).unwrap();
        let content = node_mut(control, "w:sdtContent").unwrap();
        node_mut(content, "w:t").unwrap().text = value.into();
    });
}
fn replace_content(parts: &mut [(String, Vec<u8>)], record: &Record, xml: &str) {
    alter(parts, &record.part, |root| {
        let control = control_mut(root, &record.tag()).unwrap();
        let content = node_mut(control, "w:sdtContent").unwrap();
        content.children = docx_notes::xml(format!("<w:sdtContent xmlns:w=\"{}\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">{xml}</w:sdtContent>",crate::math_document::WORD_NS).as_bytes()).unwrap().children;
    });
}
fn real_link(parts: &mut [(String, Vec<u8>)]) {
    let bytes = &mut parts
        .iter_mut()
        .find(|(name, _)| name == "word/_rels/document.xml.rels")
        .unwrap()
        .1;
    *bytes=String::from_utf8(bytes.clone()).unwrap().replace("</Relationships>","<Relationship Id=\"literalRealLink\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" Target=\"https://example.test/real?a=1&amp;b=2\" TargetMode=\"External\"/></Relationships>").into_bytes();
}
fn parsed_text(source: &str) -> String {
    pulldown_cmark::Parser::new_ext(source, pulldown_cmark::Options::all())
        .filter_map(|event| match event {
            pulldown_cmark::Event::Text(text) | pulldown_cmark::Event::Code(text) => {
                Some(text.into_string())
            }
            _ => None,
        })
        .collect()
}
fn imported(parts: &[(String, Vec<u8>)]) -> String {
    docx_convert(parts, Path::new("portable.docx"), false).unwrap()
}
fn cites(source: &str) -> Vec<crate::publication_rich::SourceCitation> {
    crate::publication::citations::scan(source, "en-US")
        .unwrap()
        .citations
}

#[test]
fn local_citation_controls_preserve_numeric_locators_affixes_duplicates_and_body_edits() {
    let (_dir,source,mut parts)=prepare("in-text","PRIVATE_BODY_SENTINEL 😀 [see @42, p. 12 **tail**]. Again [see @42, p. 12 **tail**]. Manual[^m].\n\n[^m]: Note [-@same, chap. 3].");
    let m = manifest(&parts);
    let records = m
        .records
        .iter()
        .filter(|r| r.kind == "citation")
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 3);
    assert_eq!(records[0].source, records[1].source);
    assert_ne!(records[0].id, records[1].id);
    assert_eq!(
        m.source_revision,
        crate::document_transaction::revision(&source)
    );
    let xml = String::from_utf8(
        parts
            .iter()
            .find(|(name, _)| name == domain::PART)
            .unwrap()
            .1
            .clone(),
    )
    .unwrap();
    assert!(!xml.contains("PRIVATE_BODY_SENTINEL"));
    assert!(xml.contains("<rc:item key=\"42\""));
    assert!(xml.contains("<rc:locator>12</rc:locator>"));
    alter(&mut parts, "word/document.xml", |root| {
        node_mut(root, "w:t").unwrap().text = "EDITED_BODY 😀 ".into()
    });
    let returned = imported(&parts);
    assert!(returned.contains("EDITED_BODY 😀"));
    assert_eq!(cites(&returned).len(), 3);
    assert_eq!(returned.matches("[see @42, p. 12 **tail**]").count(), 2);
    assert!(returned.contains("[-@same, chap. 3]"));
    assert!(returned.contains("[^word-footnote-1]"));
    assert!(!returned.contains("citation-import:v1"));
    let resources = Resources::from_markdown(&returned).unwrap().unwrap();
    assert_eq!(resources.bibliography, vec!["refs.json"]);
    assert_eq!(resources.style, "house.csl");
    let scan = cites(&returned);
    assert_eq!(scan[0].items[0].cite.key, "42");
    assert_eq!(scan[0].items[0].cite.locator.as_deref(), Some("12"));
    assert!(scan[0].items[0]
        .cite
        .prefix
        .as_deref()
        .unwrap()
        .contains("see"));
    assert!(scan[0].items[0]
        .cite
        .suffix
        .as_deref()
        .unwrap()
        .contains("tail"));
}

#[test]
fn modifying_only_second_repeated_source_uses_its_control_identity_and_rejects_stale_edits() {
    let (_dir, _source, mut parts) = prepare("in-text", "First [@42, p. 12]. Second [@42, p. 12].");
    update_manifest(&mut parts, |m| {
        m.records
            .iter_mut()
            .filter(|r| r.kind == "citation")
            .nth(1)
            .unwrap()
            .source = "[compare @42, p. 99 **changed**]".into()
    });
    let returned = imported(&parts);
    let scan = cites(&returned);
    assert_eq!(scan.len(), 2);
    assert_eq!(scan[0].items[0].cite.locator.as_deref(), Some("12"));
    assert_eq!(scan[1].items[0].cite.locator.as_deref(), Some("99"));
    assert!(returned.contains("Second [compare @42, p. 99 **changed**]"));
    let tx = crate::document_transaction::ContentTransaction {
        base_revision: crate::document_transaction::revision(&returned),
        origin: "mcp".into(),
        edits: vec![crate::document_transaction::TextEdit {
            from: scan[1].span.from,
            to: scan[1].span.to,
            insert: "[@42, p. 100]".into(),
        }],
    };
    let changed = crate::document_transaction::apply(&returned, &tx).unwrap();
    assert!(changed.content.contains("Second [@42, p. 100]"));
    assert_eq!(
        crate::document_transaction::apply(&changed.content, &tx).unwrap_err(),
        "transaction_conflict"
    );
}

#[test]
fn visible_and_mixed_changes_preserve_current_render_and_both_source_versions() {
    let (_dir, _source, mut parts) = prepare("in-text", "First [@42, p. 12]. Second [@42, p. 12].");
    let record = manifest(&parts)
        .records
        .into_iter()
        .filter(|r| r.kind == "citation")
        .nth(1)
        .unwrap();
    alter_render(&mut parts, &record, "USER_CITATION @same");
    update_manifest(&mut parts, |m| {
        m.records
            .iter_mut()
            .find(|r| r.id == record.id)
            .unwrap()
            .source = "[see @same, p. 77]".into()
    });
    let returned = imported(&parts);
    assert!(returned.contains("USER_CITATION &#64;same"));
    assert!(returned.contains("citation_source_and_render_modified"));
    assert!(returned.contains("original_source\": \"[@42, p. 12]"));
    assert!(returned.contains("current_source\": \"[see @same, p. 77]"));
    assert_eq!(cites(&returned).len(), 1);
    assert!(returned.contains("First [@42, p. 12]"));
}

#[test]
fn edited_citation_keeps_edge_spaces_without_joining_neighboring_prose() {
    let (_dir, _source, mut parts) = prepare("in-text", "BEFORE[@42]AFTER");
    let record = manifest(&parts)
        .records
        .into_iter()
        .find(|r| r.kind == "citation")
        .unwrap();
    // Replacing the whole displayed citation also removes its other CSL runs.
    // A first-run edit would legitimately retain the remaining old text.
    alter(&mut parts, &record.part, |root| {
        let control = control_mut(root, &record.tag()).unwrap();
        let content = node_mut(control, "w:sdtContent").unwrap();
        let replacement = docx_notes::xml(
            format!(
                "<w:r xmlns:w=\"{}\"><w:t xml:space=\"preserve\"> USER_CITATION </w:t></w:r>",
                crate::math_document::WORD_NS
            )
            .as_bytes(),
        )
        .unwrap();
        content.children = vec![replacement];
    });
    let returned = imported(&parts);
    assert!(
        returned.contains("BEFORE USER_CITATION AFTER"),
        "{returned}"
    );
    assert!(returned.contains("citation_render_modified"));
    assert!(cites(&returned).is_empty());
}

#[test]
fn cross_story_move_keeps_visible_text_and_diagnoses_original_source_without_guessing() {
    let (_dir, source, mut parts) = prepare(
        "in-text",
        "Move [see @42, p. 12 **tail**]. Manual[^m].\n\n[^m]: Authored [-@same, chap. 3].",
    );
    let record = manifest(&parts)
        .records
        .into_iter()
        .find(|record| record.kind == "citation" && record.part == "word/document.xml")
        .unwrap();
    fn take(node: &mut XmlNode, value: &str) -> Option<XmlNode> {
        if let Some(index) = node
            .children
            .iter()
            .position(|child| tag(child) == Some(value))
        {
            return Some(node.children.remove(index));
        }
        node.children
            .iter_mut()
            .find_map(|child| take(child, value))
    }
    let mut moved = None;
    alter(&mut parts, "word/document.xml", |root| {
        moved = take(root, &record.tag())
    });
    alter(&mut parts, "word/footnotes.xml", |root| {
        let story = root
            .children
            .iter_mut()
            .find(|node| node.name == "w:footnote" && node.attr("w:id") == Some("1"))
            .unwrap();
        node_mut(story, "w:p")
            .unwrap()
            .children
            .push(moved.take().unwrap());
    });
    let returned = imported(&parts);
    assert_eq!(cites(&returned).len(), 1);
    assert!(parsed_text(&returned).contains("Alpha"));
    assert!(returned.contains("Authored [-@same, chap. 3]"));
    let diagnostics = domain::import_diagnostics(&returned);
    let moved = diagnostics
        .iter()
        .find(|row| row["reason"] == "citation_binding_story_changed")
        .unwrap();
    assert_eq!(moved["original_source"], record.original_source);
    assert_eq!(moved["current_source"], record.source);
    assert_eq!(
        moved["source_revision"],
        crate::document_transaction::revision(&source)
    );
    assert_eq!(moved["part"], "word/footnotes.xml");
    let empty = tempfile::tempdir().unwrap();
    let prepared = crate::publication::prepare(&returned, empty.path().to_str().unwrap()).unwrap();
    assert_eq!(prepared.projection.citations.len(), 1);
    assert!(prepared
        .warnings
        .iter()
        .any(|code| code == domain::CONFLICT));
}

#[test]
fn modified_citation_literal_syntax_stays_visible_without_reactivating_and_keeps_real_rich_content()
{
    let literal="USER_LITERAL <!--HIDDEN--> <b>NOT_HTML</b> [@same] [FAKE](https://fake.test) *literal* &lt; &amp; :ref[eq:fake] $$x$$";
    for mixed in [false, true] {
        let (dir, _source, mut parts) =
            prepare("in-text", "First [@42, p. 12]. Second [@42, p. 12].");
        let record = manifest(&parts)
            .records
            .into_iter()
            .filter(|record| record.kind == "citation")
            .nth(1)
            .unwrap();
        real_link(&mut parts);
        replace_content(&mut parts,&record,&format!("<w:r><w:rPr><w:b/></w:rPr><w:t>{}</w:t></w:r><w:hyperlink r:id=\"literalRealLink\"><w:r><w:rPr><w:i/></w:rPr><w:t> REAL_LINK [literal] </w:t></w:r></w:hyperlink><w:r><w:rPr><w:rFonts w:ascii=\"Consolas\"/></w:rPr><w:t>`CODE` [@same]</w:t></w:r>",crate::docx_writer::xml_escape(literal)));
        if mixed {
            update_manifest(&mut parts, |manifest| {
                manifest
                    .records
                    .iter_mut()
                    .find(|row| row.id == record.id)
                    .unwrap()
                    .source = "[@same, p. 99]".into()
            });
        }
        let returned = imported(&parts);
        assert!(parsed_text(&returned).contains(literal), "{returned}");
        assert!(parsed_text(&returned).contains("`CODE` [@same]"));
        assert_eq!(cites(&returned).len(), 1);
        let events = pulldown_cmark::Parser::new_ext(&returned, pulldown_cmark::Options::all())
            .collect::<Vec<_>>();
        let links = events
            .iter()
            .filter_map(|event| match event {
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link { dest_url, .. }) => {
                    Some(dest_url.as_ref())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(links, vec!["https://example.test/real?a=1&b=2"]);
        let mut html = String::new();
        pulldown_cmark::html::push_html(&mut html, events.into_iter());
        assert!(html.contains("<strong>USER_LITERAL"));
        assert!(html.contains("<em>REAL_LINK"));
        assert!(html.contains("&lt;!--HIDDEN--&gt;"));
        assert!(!html.contains("<b>NOT_HTML</b>"));
        let prepared =
            crate::publication::prepare(&returned, dir.path().to_str().unwrap()).unwrap();
        assert_eq!(prepared.projection.citations.len(), 1);
        assert!(parsed_text(&prepared.content).contains(literal));
        assert!(prepared
            .warnings
            .iter()
            .any(|code| code == domain::CONFLICT));
        assert!(domain::import_diagnostics(&returned)
            .iter()
            .any(|row| row["reason"]
                == if mixed {
                    "citation_source_and_render_modified"
                } else {
                    "citation_render_modified"
                }));
    }
}

#[test]
fn modified_bibliography_literal_markup_and_code_do_not_create_new_citations_or_hide_text() {
    let (dir, _source, mut parts) = prepare("in-text", "Body [@42].");
    let literal =
        "USER_BIBLIOGRAPHY <!--HIDDEN--> [@same] [FAKE](https://fake.test) &lt; &amp; *literal*";
    let record = manifest(&parts)
        .records
        .into_iter()
        .find(|record| record.kind == "bibliography")
        .unwrap();
    real_link(&mut parts);
    replace_content(&mut parts,&record,&format!("<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>{}</w:t></w:r><w:hyperlink r:id=\"literalRealLink\"><w:r><w:t>REAL_LINK</w:t></w:r></w:hyperlink></w:p><w:p><w:pPr><w:pStyle w:val=\"Code\"/></w:pPr><w:r><w:t>```\n[@same] &lt;!--CODE_LITERAL--&gt;\n```</w:t></w:r></w:p>",crate::docx_writer::xml_escape(literal)));
    let returned = imported(&parts);
    assert!(parsed_text(&returned).contains(literal));
    assert!(parsed_text(&returned).contains("```\n[@same] <!--CODE_LITERAL-->\n```"));
    let links = pulldown_cmark::Parser::new(&returned)
        .filter_map(|event| match event {
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link { dest_url, .. }) => {
                Some(dest_url.into_string())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(links, vec!["https://example.test/real?a=1&b=2"]);
    assert_eq!(cites(&returned).len(), 1);
    let prepared = crate::publication::prepare(&returned, dir.path().to_str().unwrap()).unwrap();
    assert!(prepared.projection.bibliography.is_none());
    assert_eq!(prepared.projection.citations.len(), 1);
    assert!(parsed_text(&prepared.content).contains(literal));
    assert!(prepared
        .warnings
        .iter()
        .any(|code| code == domain::CONFLICT));
}

#[test]
fn edited_bibliography_is_visible_and_suppresses_automatic_regeneration() {
    let (_dir, _source, mut parts) = prepare("in-text", "Text [@42].");
    let record = manifest(&parts)
        .records
        .into_iter()
        .find(|r| r.kind == "bibliography")
        .unwrap();
    alter_render(&mut parts, &record, "USER_BIBLIOGRAPHY");
    let returned = imported(&parts);
    assert!(returned.contains("USER_BIBLIOGRAPHY"));
    assert!(returned.contains("FIRST_TITLE"));
    assert!(returned.contains("citation_render_modified"));
    assert!(!returned.contains("\n<!--readmd:bibliography-->"));
    assert!(
        Resources::from_markdown(&returned)
            .unwrap()
            .unwrap()
            .manual_bibliography
    );
    let empty = tempfile::tempdir().unwrap();
    let prepared = crate::publication::prepare(&returned, empty.path().to_str().unwrap()).unwrap();
    assert!(prepared.projection.bibliography.is_none());
    assert_eq!(prepared.content.matches("USER_BIBLIOGRAPHY").count(), 1);
}

#[test]
fn imported_resources_prepare_and_export_again_after_moving_away_from_source_files() {
    let (dir, _source, parts) = prepare(
        "in-text",
        "Portable [see @42, p. 12 **tail**]. Again [@same, p. 7].",
    );
    let returned = imported(&parts);
    drop(dir);
    let empty = tempfile::tempdir().unwrap();
    let base = empty.path().to_str().unwrap();
    let prepared = crate::publication::prepare(&returned, base).unwrap();
    assert_eq!(prepared.projection.citations.len(), 2);
    assert!(prepared.projection.bibliography.is_some());
    assert!(!crate::publication::resource_revision(&returned, base)
        .unwrap()
        .is_empty());
    let epub = crate::publication::prepare_with_language(&returned, base, Some("fr-FR")).unwrap();
    assert_eq!(epub.data["locale"], "fr-FR");
    let mut alias_source=returned.clone();
    let original_profile=crate::publication_profile::from_source(&returned).unwrap();
    let original_resources=Resources::from_markdown(&returned).unwrap();
    for(key,value)in[("csl","./house.csl"),("lang","de-DE")]{
        let tx=crate::publication_metadata::patch_source_member(&alias_source,&crate::document_transaction::revision(&alias_source),&[key],&json!(value)).unwrap();
        alias_source=crate::document_transaction::apply(&alias_source,&tx).unwrap().content;
    }
    assert_eq!(crate::publication_profile::from_source(&alias_source).unwrap().source_values,original_profile.source_values);
    assert_eq!(original_profile.options["typography"]["size"],11.0);
    assert_eq!(Resources::from_markdown(&alias_source).unwrap(),original_resources);
    let metadata_blocks=pulldown_cmark::Parser::new_ext(&alias_source,crate::md_ast::options()).filter(|event|matches!(event,pulldown_cmark::Event::Start(pulldown_cmark::Tag::MetadataBlock(_)))).count();
    assert_eq!(metadata_blocks,1,"The resource alias must share the imported publication profile's front matter");
    let aliased = crate::publication::prepare(&alias_source, base).unwrap();
    assert_eq!(aliased.data["locale"], "de-DE");
    let explicit = crate::publication::prepare_with_options(
        &alias_source,
        base,
        &json!({"meta":{"language":"fr-FR"}}),
    )
    .unwrap();
    assert_eq!(explicit.data["locale"], "fr-FR");
    fs::write(empty.path().join("host-only.csl"), style("note")).unwrap();
    let options = json!({"citations":{"style":"host-only.csl"}});
    assert_eq!(
        crate::publication::prepare_with_options(&returned, base, &options)
            .err()
            .unwrap()
            .code,
        "academic_resource"
    );
    assert!(Resources::capture(&returned, base, &options).is_err());
    let output = empty.path().join("again.docx");
    let result = crate::mdexport::export_docx(
        &returned,
        base,
        output.to_str().unwrap(),
        &json!({"frontMatter":{"mode":"none"}}),
        "portable.md",
    )
    .unwrap();
    assert!(result.ok);
    let zipped = parse_zip_entries(&fs::read(output).unwrap()).unwrap();
    let twice = imported(&zipped);
    assert_eq!(cites(&twice).len(), 2);
    assert!(twice.contains("[see @42, p. 12 **tail**]"));
    assert!(!twice.contains("citation-import:v1"));
}

#[test]
fn duplicate_control_never_selects_one_of_two_identical_visible_occurrences() {
    let (_dir, _source, mut parts) = prepare("in-text", "Text [@42, p. 12].");
    let record = manifest(&parts)
        .records
        .into_iter()
        .find(|r| r.kind == "citation")
        .unwrap();
    alter(&mut parts, "word/document.xml", |root| {
        let clone = control_mut(root, &record.tag()).unwrap().clone();
        let paragraph = XmlNode {
            name: "w:p".into(),
            local: "p".into(),
            attrs: Vec::new(),
            children: vec![clone],
            text: String::new(),
            tail: String::new(),
        };
        node_mut(root, "w:body").unwrap().children.push(paragraph);
    });
    let producer_error = binding_signatures(&parts).unwrap_err();
    assert!(producer_error.starts_with("docx_citation_invalid: duplicate_binding:"), "{producer_error}");
    let returned = imported(&parts);
    assert!(returned.contains("duplicate_citation_binding"));
    assert!(cites(&returned).is_empty());
    assert!(returned.matches("Alpha").count() >= 2);
}

fn relation_fixture(story: &str, relationships: &str) -> Vec<(String, Vec<u8>)> {
    let (base, file) = story.rsplit_once('/').unwrap();
    vec![(format!("{base}/_rels/{file}.rels"), format!("<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{relationships}</Relationships>").into_bytes())]
}
fn changed_relationship(parts: &mut [(String, Vec<u8>)], story: &str, id: &str, attribute: &str, value: &str) {
    use crate::math_document::XmlPart;
    let (base, file) = story.rsplit_once('/').unwrap();
    let name = format!("{base}/_rels/{file}.rels");
    let bytes = &mut parts.iter_mut().find(|(path, _)| path == &name).unwrap().1;
    let mut tree = crate::math_document::parse_xml(std::str::from_utf8(bytes).unwrap()).unwrap();
    let root = tree.content.iter_mut().find_map(|part| match part { XmlPart::Element(node) => Some(node), _ => None }).unwrap();
    let node = root.content.iter_mut().find_map(|part| match part {
        XmlPart::Element(node) if node.attributes.get("Id").is_some_and(|found| found == id) => Some(node),
        _ => None,
    }).unwrap();
    node.attributes.insert(attribute.into(), value.into());
    *bytes = tree.xml().into_bytes();
}
fn control_relationship(parts: &[(String, Vec<u8>)], record: &Record) -> String {
    let mut root = docx_notes::xml(&parts.iter().find(|(name, _)| name == &record.part).unwrap().1).unwrap();
    node_mut(control_mut(&mut root, &record.tag()).unwrap(), "w:hyperlink").unwrap().attr("r:id").unwrap().to_string()
}
fn rendered_links(source: &str) -> Vec<String> {
    // Match the actual consumer: Options::all also enables the incompatible
    // legacy footnote grammar, which can misclassify a definition whose first
    // block is its source-owned local-layout comment.
    let document=crate::md_ast::parse(source);
    let mut links=Vec::new();
    crate::md_ast::walk_inlines(&document.blocks,&mut|inline| {
        if let crate::md_ast::Inline::Link{href,..}=inline { links.push(href.clone()); }
    });
    links
}

#[test]
fn visible_link_checks_follow_the_consumer_through_profile_and_local_note_layout() {
    let source="---\nreadmd: {publication: {version: 1, footnotes: {kindByLabel: {word-footnote-1: footnote}}}}\n---\nBody [outside](https://outside.example/). Note[^word-footnote-1].\n\n[^word-footnote-1]: <!-- readmd:layout v1 {\"paragraph\":{\"keepNext\":false}} -->\n\n    Current [evidence](<https://new.example/>).\n\n<!--readmd:citation-import:v1\n{\"original_source\":\"[old](https://old.example/)\"}\n-->\n";
    let links=rendered_links(source);
    assert_eq!(links,vec!["https://outside.example/","https://new.example/"]);
    assert!(!links.contains(&"https://old.example/".to_string()));
    let document=crate::md_ast::parse(source);
    assert_eq!(document.blocks.iter().filter(|block|matches!(block.unstyled(),crate::md_ast::Block::FootnoteDef{..})).count(),1);
}

#[test]
fn contextual_signatures_bind_only_referenced_relationship_targets_and_modes() {
    let story = "word/document.xml";
    let fragment = "<w:hyperlink r:id=\"used\"><w:r><w:t>evidence</w:t></w:r></w:hyperlink>";
    let relations = "<Relationship Id=\"used\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" Target=\"https://old.example/\" TargetMode=\"External\"/><Relationship Id=\"unrelated\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" Target=\"https://unrelated.example/\" TargetMode=\"External\"/>";
    let parts = relation_fixture(story, relations);
    let baseline = domain::context_signature(&parts, story, fragment).unwrap();
    assert_eq!(domain::context_signature(&parts, story, fragment).unwrap(), baseline);
    let mut unrelated = parts.clone();
    changed_relationship(&mut unrelated, story, "unrelated", "Target", "https://different.example/");
    assert_eq!(domain::context_signature(&unrelated, story, fragment).unwrap(), baseline);
    let mut target = parts.clone();
    changed_relationship(&mut target, story, "used", "Target", "https://new.example/");
    assert_ne!(domain::context_signature(&target, story, fragment).unwrap(), baseline);
    changed_relationship(&mut target, story, "used", "TargetMode", "Internal");
    assert!(domain::context_signature(&target, story, fragment).is_err());
    let mut kind = parts;
    changed_relationship(&mut kind, story, "used", "Type", "urn:changed-relationship-kind");
    assert_ne!(domain::context_signature(&kind, story, fragment).unwrap(), baseline);
}

#[test]
fn contextual_note_image_signatures_bind_payload_bytes_in_each_story_and_never_resolve_host_paths() {
    for story in ["word/document.xml", "word/footnotes.xml", "word/endnotes.xml"] {
        for attribute in ["id", "embed", "link"] {
            let fragment = format!("<w:r><a:blip xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" r:{attribute}=\"image\"/></w:r>");
            let mut parts = relation_fixture(story, "<Relationship Id=\"image\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/owned.bin\"/>");
            parts.push(("word/media/owned.bin".into(), vec![0, 255, 128, 1]));
            parts.push(("word/media/unrelated.bin".into(), vec![3, 4]));
            let baseline = domain::context_signature(&parts, story, &fragment).unwrap();
            parts.last_mut().unwrap().1.push(5);
            assert_eq!(domain::context_signature(&parts, story, &fragment).unwrap(), baseline);
            parts.iter_mut().find(|(name, _)| name == "word/media/owned.bin").unwrap().1[3] = 2;
            assert_ne!(domain::context_signature(&parts, story, &fragment).unwrap(), baseline);
            for target in ["../../outside.bin", "C:/private/secret.png", "file:///private/secret.png", "%2e%2e/%2e%2e/private.png", "media/%ZZ.bin"] {
                changed_relationship(&mut parts, story, "image", "Target", target);
                assert!(domain::context_signature(&parts, story, &fragment).is_err(), "{target}");
            }
            changed_relationship(&mut parts, story, "image", "Target", "https://offline.invalid/never-fetched.png");
            changed_relationship(&mut parts, story, "image", "TargetMode", "External");
            assert!(domain::context_signature(&parts, story, &fragment).is_ok());
        }
    }
}

#[test]
fn missing_duplicate_and_corrupt_direct_relationships_cannot_certify_a_fragment() {
    let fragment = "<w:hyperlink r:id=\"used\"><w:r><w:t>evidence</w:t></w:r></w:hyperlink>";
    let relation = "<Relationship Id=\"used\" Type=\"urn:hyperlink\" Target=\"https://example.test/\" TargetMode=\"External\"/>";
    for relations in [String::new(), format!("{relation}{relation}"), relation.replace("Target=", "MissingTarget="), relation.replace("External", "Unknown")] {
        assert!(domain::context_signature(&relation_fixture("word/document.xml", &relations), "word/document.xml", fragment).is_err());
    }
    assert!(domain::context_signature(&[], "word/document.xml", fragment).is_err());
    let mut duplicate_part = relation_fixture("word/document.xml", relation);
    duplicate_part.push(duplicate_part[0].clone());
    assert!(domain::context_signature(&duplicate_part, "word/document.xml", fragment).is_err());
    for xml in ["<!DOCTYPE Relationships SYSTEM 'file:///host-secret'><Relationships/>", "<Relationships>", "<Relationships xmlns=\"urn:spoof\"/>"] {
        assert!(domain::context_signature(&[("word/_rels/document.xml.rels".into(), xml.as_bytes().to_vec())], "word/document.xml", fragment).is_err());
    }
    let mut missing_payload = relation_fixture("word/document.xml", &relation.replace("https://example.test/", "media/missing.png").replace("External", "Internal"));
    assert!(domain::context_signature(&missing_payload, "word/document.xml", fragment).is_err());
    missing_payload.push(("word/media/missing.png".into(), vec![0]));
    missing_payload.push(("word/media/missing.png".into(), vec![1]));
    assert!(domain::context_signature(&missing_payload, "word/document.xml", fragment).is_err());
}

#[test]
fn target_only_edits_keep_current_links_and_sources_in_body_and_generated_notes() {
    for class in ["in-text", "note"] {
        let (_dir, _source, mut parts) = prepare(class, "Text [see [evidence](https://old.example/) @42]. Outside [unrelated](https://unrelated.example/).");
        let records = manifest(&parts).records;
        assert!(records.iter().all(|record| record.render_context_version == domain::RENDER_CONTEXT_VERSION));
        let record = records.iter().find(|record| record.kind == "citation" && (class != "note" || record.part == "word/footnotes.xml")).unwrap();
        let unchanged = imported(&parts);
        assert_eq!(cites(&unchanged).len(), 1);
        assert!(!unchanged.contains("citation-import:v1"));
        let rid = control_relationship(&parts, record);
        let before_note = record.note.as_ref().map(|note| note_signature(&parts, note.end, note.id).unwrap());
        changed_relationship(&mut parts, &record.part, &rid, "Target", "https://new.example/");
        if let Some(note) = &record.note { assert_ne!(note_signature(&parts, note.end, note.id).unwrap(), before_note.unwrap()); }
        let returned = imported(&parts);
        assert!(returned.contains("original_source"));
        assert!(returned.contains("https://old.example/"));
        assert!(rendered_links(&returned).contains(&"https://new.example/".to_string()),"{class}: {returned}");
        assert!(!rendered_links(&returned).contains(&"https://old.example/".to_string()),"{class}: {returned}");
        assert!(cites(&returned).is_empty());
        let prepared = crate::publication::prepare(&returned, "unavailable-original-resource-directory").unwrap();
        assert!(prepared.projection.citations.is_empty());
        assert!(prepared.warnings.iter().any(|warning| warning == domain::CONFLICT));
        assert!(rendered_links(&prepared.content).contains(&"https://new.example/".to_string()));
        if class == "note" { assert!(returned.contains("generated_citation_note_modified_or_unbound")); }
    }
}

#[test]
fn unrelated_relationship_changes_do_not_disable_editable_citation_recovery() {
    let (_dir, _source, mut parts) = prepare("in-text", "Text [see [evidence](https://old.example/) @42]. Outside [unrelated](https://unrelated.example/).");
    let bytes = &mut parts.iter_mut().find(|(name, _)| name == "word/_rels/document.xml.rels").unwrap().1;
    *bytes = String::from_utf8(bytes.clone()).unwrap().replace("https://unrelated.example/", "https://changed-unrelated.example/").into_bytes();
    let returned = imported(&parts);
    assert_eq!(cites(&returned).len(), 1);
    assert!(returned.contains("[see [evidence](https://old.example/) @42]"));
    assert!(returned.contains("https://changed-unrelated.example/"));
    assert!(!returned.contains("citation-import:v1"));
}

#[test]
fn legacy_contextless_signatures_preserve_visible_content_and_source_instead_of_restoring() {
    let (_dir, _source, mut parts) = prepare("in-text", "Text [see [evidence](https://old.example/) @42].");
    let mut legacy_signatures = HashMap::new();
    for (path, bytes) in parts.iter().filter(|(path, _)| path == "word/document.xml") {
        let root = docx_notes::xml(bytes).unwrap();
        let ns = namespaces(&root);
        let mut controls = Vec::new();
        collect(&root, &mut controls);
        for node in controls { legacy_signatures.insert((path.clone(), tag(node).unwrap().to_string()), domain::render_signature(&fragment(content(node).unwrap(), &ns)).unwrap()); }
    }
    update_manifest(&mut parts, |manifest| {
        for record in &mut manifest.records {
            record.render_context_version = 1;
            record.render_sha256 = legacy_signatures[&(record.part.clone(), record.tag())].clone();
            record.binding = crate::document_transaction::revision(&format!("ReadMD citation binding 1\0{}\0{}\0{}\0{}\0{}\0{}\0{:?}", record.id, record.kind, record.part, record.original_source, record.render_sha256, record.generated, record.note.as_ref().map(|note| (note.end, note.id, note.story_sha256.as_str()))));
        }
    });
    let bytes = &mut parts.iter_mut().find(|(name, _)| name == domain::PART).unwrap().1;
    *bytes = String::from_utf8(bytes.clone()).unwrap().replace(" render_context_version=\"1\"", "").into_bytes();
    assert!(manifest(&parts).records.iter().all(|record| record.render_context_version == 1));
    let returned = imported(&parts);
    assert!(cites(&returned).is_empty());
    assert!(returned.contains("citation_render_signature_unsupported"));
    assert!(returned.contains("original_source"));
    assert!(rendered_links(&returned).contains(&"https://old.example/".to_string()));
    assert!(parsed_text(&returned).contains("Alpha"));
    let resources = Resources::from_markdown(&returned).unwrap().unwrap();
    assert!(resources.manual_bibliography);
    let prepared = crate::publication::prepare(&returned, "unavailable-original-resource-directory").unwrap();
    assert!(prepared.projection.citations.is_empty());
    assert!(prepared.projection.bibliography.is_none());
}

#[test]
fn removed_bibliography_and_citation_controls_are_not_reinserted() {
    let (_dir, _source, mut parts) =
        prepare("in-text", "First [@42, p. 12]. Second [@same, p. 7].");
    let m = manifest(&parts);
    let bibliography = m.records.iter().find(|r| r.kind == "bibliography").unwrap();
    let citation = m
        .records
        .iter()
        .filter(|r| r.kind == "citation")
        .nth(1)
        .unwrap();
    fn remove(node: &mut XmlNode, tags: &[String]) {
        node.children
            .retain(|child| !tag(child).is_some_and(|value| tags.iter().any(|tag| tag == value)));
        for child in &mut node.children {
            remove(child, tags);
        }
    }
    alter(&mut parts, "word/document.xml", |root| {
        remove(root, &[bibliography.tag(), citation.tag()])
    });
    let returned = imported(&parts);
    assert_eq!(cites(&returned).len(), 1);
    assert_eq!(cites(&returned)[0].items[0].cite.key, "42");
    assert!(returned.contains("bibliography_control_removed"));
    assert!(returned.contains("citation_control_removed"));
    assert!(returned.contains("original_source\": \"[@same, p. 7]"));
    let resources = Resources::from_markdown(&returned).unwrap().unwrap();
    assert!(resources.manual_bibliography);
    let empty = tempfile::tempdir().unwrap();
    let prepared = crate::publication::prepare(&returned, empty.path().to_str().unwrap()).unwrap();
    assert!(prepared.projection.bibliography.is_none());
    assert_eq!(prepared.projection.citations.len(), 1);
    assert_eq!(cites(&returned).len(), 1);
    assert!(prepared
        .warnings
        .iter()
        .any(|warning| warning == domain::CONFLICT));
}

#[test]
fn generated_footnotes_and_endnotes_restore_only_when_entire_note_is_unchanged() {
    for end in [false, true] {
        let (dir, source, _) = prepare(
            "note",
            "Normal [@42, p. 12]. Prose @same [p. 7]. Manual[^m].\n\n[^m]: Authored [@42, p. 3].",
        );
        let prepared = crate::publication::prepare(&source, dir.path().to_str().unwrap()).unwrap();
        let document = crate::md_ast::parse_prepared(&source, &prepared.projection).unwrap();
        let options = json!({"frontMatter":{"mode":"none"},"footnotes":{"position":if end{"document"}else{"page"}}});
        let mut package = crate::docx_writer::build(&document, &options, "note-test.md", &|_| None);
        assert!(!package.parts.is_empty(), "{:?}", package.warns);
        domain::attach_context(
            &mut package.parts,
            &source,
            dir.path().to_str().unwrap(),
            &options,
        )
        .unwrap();
        let m = manifest(&package.parts);
        let anchors = m
            .records
            .iter()
            .filter(|r| r.generated && r.part == "word/document.xml")
            .collect::<Vec<_>>();
        assert_eq!(anchors.len(), 2);
        assert!(anchors
            .iter()
            .all(|r| r.note.as_ref().is_some_and(|n| n.story_sha256.len() == 64)));
        let returned = imported(&package.parts);
        assert!(returned.contains("Normal [@42, p. 12]"));
        assert!(returned.contains("Prose @same [p. 7]"));
        assert_eq!(cites(&returned).len(), 3);
        assert!(!returned.contains("citation-import:v1"));
        assert!(returned.contains("Authored [@42, p. 3]"));
        let document=crate::md_ast::parse(&returned);
        let definitions=document.blocks.iter().filter_map(|block|match block.unstyled(){
            crate::md_ast::Block::FootnoteDef{label,blocks}=>Some((label,blocks)),_=>None,
        }).collect::<Vec<_>>();
        assert_eq!(definitions.len(),1,"Only the actual manual note may remain: {returned}");
        let (label,blocks)=definitions[0];
        let paragraphs=blocks.iter().filter_map(|block|match block.unstyled(){
            crate::md_ast::Block::Paragraph(inlines)=>Some(crate::md_ast::inline_plain(inlines)),_=>None,
        }).collect::<Vec<_>>();
        assert_eq!(paragraphs,vec!["Authored [@42, p. 3]."],"{returned}");
        assert!(label.starts_with(if end{"word-endnote-"}else{"word-footnote-"}));
        let mut references=Vec::new();
        crate::md_ast::walk_inlines(&document.blocks,&mut|inline|{
            if let crate::md_ast::Inline::FootnoteRef(label)=inline{references.push(label.clone());}
        });
        assert_eq!(references,vec![label.clone()],"The manual note must retain its exact current reference identity: {returned}");
        let note = anchors[0].note.as_ref().unwrap().clone();
        let part = if end {
            "word/endnotes.xml"
        } else {
            "word/footnotes.xml"
        };
        alter(&mut package.parts, part, |root| {
            let kind = if end { "w:endnote" } else { "w:footnote" };
            let story = root
                .children
                .iter_mut()
                .find(|n| n.name == kind && n.attr("w:id") == Some(note.id.to_string().as_str()))
                .unwrap();
            let extra = docx_notes::xml(
                format!(
                    "<w:p xmlns:w=\"{}\"><w:r><w:t>USER_ADDED_OUTSIDE_CONTROL</w:t></w:r></w:p>",
                    crate::math_document::WORD_NS
                )
                .as_bytes(),
            )
            .unwrap();
            story.children.push(extra);
        });
        let changed = imported(&package.parts);
        assert!(changed.contains("USER_ADDED_OUTSIDE_CONTROL"));
        assert!(changed.contains("generated_citation_note_modified_or_unbound"));
        assert!(changed.contains("original_source\": \"[@42, p. 12]"));
        assert!(changed.contains("Normal [^word-"));
        assert!(changed.contains("Prose @same [p. 7]"));
        assert!(changed.contains("Authored [@42, p. 3]"));
    }
}

#[test]
fn resources_are_portable_and_comment_safe_for_legal_csl_and_json_strings() {
    let dir = tempfile::tempdir().unwrap();
    let title = "literal --> <!-- 中文😀 & preserved";
    fs::write(
        dir.path().join("refs.json"),
        json!([{"id":"42","type":"book","title":title}]).to_string(),
    )
    .unwrap();
    fs::write(
        dir.path().join("house.csl"),
        style("in-text")
            .replace("FIRST_UNUSED_SENTINEL", "unused")
            .replace(
                "<text variable=\"title\"/>",
                "<text variable=\"title\"/><text value=\" &lt;!-- --&gt; &amp; \"/>",
            ),
    )
    .unwrap();
    let absolute = dir
        .path()
        .join("refs.json")
        .to_string_lossy()
        .replace('\\', "/");
    let source = format!("---\nbibliography: '{absolute}'\ncsl: house.csl\n---\nText [@42].");
    let resources = Resources::capture(&source, dir.path().to_str().unwrap(), &json!({}))
        .unwrap()
        .unwrap();
    assert_eq!(resources.bibliography, vec!["refs.json"]);
    let capsule = resources.markdown_capsule().unwrap();
    assert_eq!(capsule.matches("<!--").count(), 1);
    assert_eq!(capsule.matches("-->").count(), 1);
    assert!(!capsule.contains(&dir.path().to_string_lossy().to_string()));
    let read = Resources::from_markdown(&format!("{capsule}\nText [@42]."))
        .unwrap()
        .unwrap();
    assert_eq!(read, resources);
    assert_eq!(
        serde_json::from_str::<Vec<Value>>(read.read("refs.json").unwrap()).unwrap()[0]["title"],
        title
    );
    assert!(Resources::from_markdown(&format!("{capsule}\n{capsule}")).is_err());
    let mut invalid = resources.clone();
    invalid.files[0].sha256 = "0".repeat(64);
    let raw = serde_json::to_string_pretty(&invalid)
        .unwrap()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e");
    assert!(
        Resources::from_markdown(&format!("<!--readmd:citation-resources:v1\n{raw}\n-->")).is_err()
    );
    let mut invalid = resources;
    invalid.files[0].text.push('\u{0000}');
    assert!(invalid.validate().is_err());
}

#[test]
fn invalid_resource_digest_and_namespace_preserve_render_with_explicit_error() {
    for spoof_namespace in [false, true] {
        let (_dir, _source, mut parts) = prepare("in-text", "Text [@42, p. 12].");
        let part = &mut parts
            .iter_mut()
            .find(|(name, _)| name == domain::PART)
            .unwrap()
            .1;
        if spoof_namespace {
            *part = String::from_utf8(part.clone())
                .unwrap()
                .replace(domain::NS, "urn:spoofed-citations")
                .into_bytes();
        } else {
            let mut root = docx_notes::xml(part).unwrap();
            let node = root
                .children
                .iter_mut()
                .find(|n| n.local == "resources")
                .unwrap();
            let mut resources: Resources = serde_json::from_str(&node.text).unwrap();
            resources.files[0].sha256 = "0".repeat(64);
            node.text = serde_json::to_string(&resources).unwrap();
            *part = docx_xml_fragment(&root, &[]).into_bytes();
        }
        let returned = imported(&parts);
        assert!(returned.contains("Alpha"));
        assert!(returned.contains("FIRST_TITLE"));
        assert!(returned.contains("citation_source_package_invalid"));
        assert!(cites(&returned).is_empty());
        assert!(Resources::from_markdown(&returned).unwrap().is_none());
    }
}

#[test]
fn source_context_mismatch_is_rejected_and_external_citation_fields_are_not_guessed() {
    let (dir, source, mut parts) = prepare("in-text", "Text [@42].");
    assert!(domain::attach_context(
        &mut parts,
        &source.replace("[@42]", "[@same]"),
        dir.path().to_str().unwrap(),
        &json!({})
    )
    .is_err());
    let document=format!("<w:document xmlns:w=\"{}\"><w:body><w:p><w:fldSimple w:instr=\"CITATION external-key\"><w:r><w:t>EXTERNAL_RENDER</w:t></w:r></w:fldSimple></w:p></w:body></w:document>",crate::math_document::WORD_NS);
    let returned = imported(&[("word/document.xml".into(), document.into_bytes())]);
    assert_eq!(returned, "EXTERNAL_RENDER");
    assert!(cites(&returned).is_empty());
}

#[test]
fn render_signature_tolerates_run_splits_and_prefix_aliases_but_preserves_format_changes() {
    let joined = "<w:r><w:rPr><w:i/></w:rPr><w:t>Alpha Beta</w:t></w:r>";
    let split="<x:r xmlns:x=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" x:rsidR=\"001\"><x:rPr><x:i/></x:rPr><x:t>Alpha </x:t></x:r><w:r><w:rPr><w:i/></w:rPr><w:t>Beta</w:t></w:r>";
    assert_eq!(
        domain::render_signature(joined).unwrap(),
        domain::render_signature(split).unwrap()
    );
    assert_ne!(
        domain::render_signature(joined).unwrap(),
        domain::render_signature(&joined.replace("<w:i/>", "<w:b/>")).unwrap()
    );
}

#[test]
fn text_digest_only_merges_actual_word_text_leaves_and_permitted_editor_attributes() {
    let joined="<w:r><w:t>Alpha Beta</w:t></w:r>";
    let split="<w:r w:rsidR=\"001\"><w:t xml:space=\"preserve\">Alpha </w:t></w:r><w:r><w:t>Beta</w:t></w:r>";
    let baseline=domain::render_signature(joined).unwrap();
    assert_eq!(domain::render_signature(split).unwrap(),baseline);
    for changed in [
        "<w:r><x:t xmlns:x=\"urn:foreign\">Alpha Beta</x:t></w:r>",
        "<w:r><w:t xmlns:x=\"urn:foreign\" x:custom=\"changed\">Alpha Beta</w:t></w:r>",
        "<w:r xmlns:x=\"urn:foreign\" x:custom=\"changed\"><w:t>Alpha Beta</w:t></w:r>",
        "<w:r xmlns:x=\"urn:foreign\" x:rsidR=\"changed\"><w:t>Alpha Beta</w:t></w:r>",
        "<w:r><w:t xml:space=\"unsupported\">Alpha Beta</w:t></w:r>",
        "<w:r><w:t w:rsidR=\"001\">Alpha Beta</w:t></w:r>",
        "<w:r><w:t>Alpha Beta<x:hidden xmlns:x=\"urn:foreign\">changed</x:hidden></w:t></w:r>",
    ] {
        assert_ne!(domain::render_signature(changed).unwrap(),baseline,"{changed}");
        assert_ne!(domain::context_signature(&[],"word/document.xml",changed).unwrap(),domain::context_signature(&[],"word/document.xml",joined).unwrap());
    }
}

#[test]
fn foreign_text_or_unrecognized_text_run_attributes_never_restore_the_old_citation() {
    let (_dir,_source,original)=prepare("in-text","Kept body [see @42, p. 12 **tail**].");
    let record=manifest(&original).records.into_iter().find(|record|record.kind=="citation").unwrap();
    for change in ["namespace","text_attribute","run_attribute"] {
        let mut parts=original.clone();let mut visible=Vec::new();
        alter(&mut parts,&record.part,|root| {
            let control=control_mut(root,&record.tag()).unwrap();let content=node_mut(control,"w:sdtContent").unwrap();
            let target=node_mut(content,if change=="run_attribute"{"w:r"}else{"w:t"}).unwrap();
            target.attrs.push(("xmlns:x".into(),"urn:foreign".into()));
            if change=="namespace" {target.name="x:t".into();}
            else {target.attrs.push(("x:custom".into(),"changed".into()));}
            visible=content.descendants().into_iter().filter(|node|node.name=="w:t"&&!node.text.is_empty()).map(|node|node.text.clone()).collect();
        });
        let returned=imported(&parts);assert!(cites(&returned).is_empty(),"{change}");
        assert!(returned.contains("citation_render_modified"));assert!(returned.contains("original_source"));
        let text=parsed_text(&returned);assert!(text.contains("Kept body"));
        for value in visible {assert!(text.contains(&value),"{change}: {value}");}
    }
}

fn cached_note_parts(count:usize)->Vec<(String,Vec<u8>)> {
    const W:&str=crate::math_document::WORD_NS;
    let ns=format!("xmlns:w=\"{W}\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"");
    let control=|index:usize|format!("<w:sdt><w:sdtPr><w:tag w:val=\"readmd:citation:v1:cache-{index}\"/></w:sdtPr><w:sdtContent><w:r><w:t>Evidence {index}</w:t><a:blip r:embed=\"shared\"/></w:r></w:sdtContent></w:sdt>");
    let notes=(1..=count).map(|id|format!("<w:footnote w:id=\"{id}\"><w:p>{}</w:p></w:footnote>",control(id))).collect::<String>();
    let rels=b"<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"shared\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/shared.bin\"/></Relationships>".to_vec();
    vec![("word/document.xml".into(),format!("<w:document {ns}><w:body><w:p>{}</w:p></w:body></w:document>",control(0)).into_bytes()),
        ("word/footnotes.xml".into(),format!("<w:footnotes {ns}>{notes}</w:footnotes>").into_bytes()),
        ("word/_rels/document.xml.rels".into(),rels.clone()),("word/_rels/footnotes.xml.rels".into(),rels),
        ("word/media/shared.bin".into(),vec![7;32768])]
}

#[test]
fn sdt_and_generated_notes_share_one_story_parse_and_one_payload_hash() {
    let parts=cached_note_parts(64);let signatures=PackageSignatures::new(&parts);
    assert_eq!(signatures.bindings().unwrap().len(),65);
    let mut digests=HashSet::new();
    for id in 1..=64 {let value=signatures.note(false,id).unwrap();assert_eq!(signatures.note(false,id).unwrap(),value);digests.insert(value);}
    assert_eq!(digests.len(),64);assert_eq!(signatures.story_parses.get(),2);assert_eq!(signatures.note_digests.get(),64);
    assert_eq!(signatures.context.payload_work(),(1,32768));
    let (_dir,_source,parts)=prepare("note","First [@42, p. 1]. Second [@same, p. 2]. Third [@42, p. 3].");
    let root=docx_notes::xml(&parts.iter().find(|(name,_)|name=="word/document.xml").unwrap().1).unwrap();
    let rels=docx_notes::relationships(&parts,"word/document.xml").unwrap();
    let imported=Citations::load(&parts,el_child(&root,"w:body").unwrap(),&rels);
    assert_eq!(imported.signature_work,(2,3,0,0));assert_eq!(imported.generated.len(),3);
}

#[test]
fn payload_budget_is_shared_across_sdt_and_generated_note_digest_checks() {
    let mut parts=cached_note_parts(2);
    let note=&mut parts.iter_mut().find(|(name,_)|name=="word/footnotes.xml").unwrap().1;
    *note=String::from_utf8(note.clone()).unwrap().replacen("r:embed=\"shared\"","r:embed=\"second\"",1).into_bytes();
    let rels=&mut parts.iter_mut().find(|(name,_)|name=="word/_rels/footnotes.xml.rels").unwrap().1;
    *rels=String::from_utf8(rels.clone()).unwrap().replace("</Relationships>","<Relationship Id=\"second\" Type=\"urn:image\" Target=\"media/second.bin\"/></Relationships>").into_bytes();
    parts.push(("word/media/second.bin".into(),vec![8;16]));
    let mut signatures=PackageSignatures::new(&parts);signatures.context=domain::ContextSignatures::with_payload_limit(&parts,32768);
    assert!(signatures.bindings().is_err());assert_eq!(signatures.context.payload_work(),(1,32768));
    assert!(signatures.note(false,1).is_err());assert!(signatures.note(false,1).is_err());
    assert!(signatures.note(false,2).is_ok());assert_eq!(signatures.context.payload_work(),(1,32768));
    assert_eq!(signatures.story_parses.get(),2);assert_eq!(signatures.note_digests.get(),2);
}

#[test]
fn damaged_and_duplicate_note_stories_are_cached_as_unverifiable() {
    for damage in ["duplicate_note","malformed","duplicate_part"] {
        let mut parts=cached_note_parts(2);
        let index=parts.iter().position(|(name,_)|name=="word/footnotes.xml").unwrap();
        match damage {
            "duplicate_note"=>{parts[index].1=String::from_utf8(parts[index].1.clone()).unwrap().replace("</w:footnotes>","<w:footnote w:id=\"1\"><w:p/></w:footnote></w:footnotes>").into_bytes();},
            "malformed"=>parts[index].1=b"<w:footnotes".to_vec(),
            _=>parts.push(parts[index].clone()),
        }
        let signatures=PackageSignatures::new(&parts);
        for id in [1,2,1,2] {assert!(signatures.note(false,id).is_err(),"{damage}");}
        assert_eq!(signatures.stories.borrow().len(),1);assert_eq!(signatures.notes.borrow().len(),2);
        assert_eq!(signatures.story_parses.get(),if damage=="duplicate_part"{0}else{1});assert_eq!(signatures.note_digests.get(),0);
    }
}


fn generated_author_family_package(end:bool,bibliography_first:bool)->(tempfile::TempDir,String,Vec<(String,Vec<u8>)>){
    generated_author_family_package_mode(end,bibliography_first,false)
}
fn generated_author_family_package_mode(end:bool,bibliography_first:bool,prose:bool)->(tempfile::TempDir,String,Vec<(String,Vec<u8>)>){
    generated_author_family_package_style(end,bibliography_first,prose,&style("note"))
}
fn generated_author_family_package_style(end:bool,bibliography_first:bool,prose:bool,csl:&str)->(tempfile::TempDir,String,Vec<(String,Vec<u8>)>){
    let first=if prose{"@42 [p. 12]"}else{"[@42, p. 12]"};
    let (dir,source,_)=prepare("note",&format!("BODY_BEGIN First {first}. Same [@42, p. 12]. Author :cite[author-only @same]. Manual[^m], repeated[^m]. BODY_END.

[^m]: MANUAL_CURRENT **strong** and [evidence](https://example.test/current)."));
    fs::write(dir.path().join("house.csl"),csl).unwrap();
    let prepared=crate::publication::prepare(&source,dir.path().to_str().unwrap()).unwrap();let doc=crate::md_ast::parse_prepared(&source,&prepared.projection).unwrap();let options=json!({"frontMatter":{"mode":"none"},"footnotes":{"size":10,"position":if end{"document"}else{"page"}}});let mut output=crate::docx_writer::build(&doc,&options,"Generated.md",&|_|None);assert!(!output.parts.is_empty(),"{:?}",output.warns);domain::attach_context(&mut output.parts,&source,dir.path().to_str().unwrap(),&options).unwrap();
    if bibliography_first{alter(&mut output.parts,"word/document.xml",|document|{let body=node_mut(document,"w:body").unwrap();let at=body.children.iter().position(|node|tag(node).is_some_and(|value|value.starts_with("readmd:bibliography:"))).unwrap();let bibliography=body.children.remove(at);body.children.insert(0,bibliography);});}
    (dir,source,output.parts)
}
fn generated_author_bookmark_markers(parts:&mut[(String,Vec<u8>)],end:bool){
    let ids=manifest(parts).records.iter().filter(|record|record.generated&&record.part=="word/document.xml").map(|record|{let note=record.note.as_ref().unwrap();assert_eq!(note.end,end);note.id}).collect::<Vec<_>>();assert_eq!(ids.len(),2);assert_ne!(ids[0],ids[1]);
    let signatures=ids.iter().map(|id|note_signature(parts,end,*id).unwrap()).collect::<Vec<_>>();let auxiliary=parts.iter().filter(|(name,_)|name.starts_with("customXml/")).cloned().collect::<Vec<_>>();let story=if end{"word/endnotes.xml"}else{"word/footnotes.xml"};let marker_name=if end{"w:endnoteRef"}else{"w:footnoteRef"};let kind=if end{"w:endnote"}else{"w:footnote"};
    alter(parts,story,|root|{for (ordinal,id)in ids.iter().enumerate(){let note=root.children.iter_mut().find(|node|node.name==kind&&node.attr("w:id")==Some(id.to_string().as_str())).unwrap();let paragraph=node_mut(note,"w:p").unwrap();let at=paragraph.children.iter().position(|node|node.name=="w:r"&&node.children.iter().any(|child|child.name==marker_name)).unwrap();let bookmark=900+ordinal;let name=if ordinal==0{"MarkerAnchor".to_string()}else{format!("MarkerAnchor{ordinal}")};
        let start=docx_notes::xml(format!(r#"<w:bookmarkStart xmlns:w="{}" w:id="{bookmark}" w:name="{name}"/>"#,crate::math_document::WORD_NS).as_bytes()).unwrap();let finish=docx_notes::xml(format!(r#"<w:bookmarkEnd xmlns:w="{}" w:id="{bookmark}"/>"#,crate::math_document::WORD_NS).as_bytes()).unwrap();paragraph.children.insert(at,start);paragraph.children.insert(at+2,finish);
    }});
    for (id,signature)in ids.iter().zip(signatures){assert_eq!(note_signature(parts,end,*id).unwrap(),signature,"legal native bookmarks must not invalidate a verified generated-note signature");}assert_eq!(parts.iter().filter(|(name,_)|name.starts_with("customXml/")).cloned().collect::<Vec<_>>(),auxiliary,"bookmarks must not re-sign citation provenance");
}
fn exact_native_text(node:&XmlNode)->String{std::iter::once(node).chain(node.descendants()).filter(|node|node.name=="w:t").map(|node|node.text.as_str()).collect()}
fn generated_author_native_snapshot(parts:&[(String,Vec<u8>)],end:bool)->(String,Vec<(String,String,Vec<(String,Option<u32>,Option<u32>,Option<bool>,Option<bool>)>)>,Vec<usize>){
    let document=docx_notes::xml(docx_notes::unique(parts,"word/document.xml").unwrap()).unwrap();let body=el_child(&document,"w:body").unwrap();let rels=docx_notes::relationships(parts,"word/document.xml").unwrap();let notes=docx_notes::Notes::load(parts,body,&rels).unwrap();let index=super::super::docx_text_style::RunStyleIndex::build(parts,&rels,body,&notes).unwrap();let reference=if end{"w:endnoteReference"}else{"w:footnoteReference"};let identities=notes.settings_used().iter().enumerate().map(|(ordinal,identity)|(*identity,ordinal)).collect::<HashMap<_,_>>();
    // Use native first-reference ordinals rather than reassignable Word ids.
    // A wrong final repeat can preserve both the old total4 and all3 note
    // bodies; its target must still stay the actual manual note.
    let refs=body.descendants().into_iter().filter(|node|node.name==reference).map(|node|{let id=node.attr("w:id").unwrap().parse::<i32>().unwrap();*identities.get(&(end,id)).expect("actual reference targets a current native note")}).collect::<Vec<_>>();
    let mut rows=Vec::new();for (kind,id)in notes.settings_used(){assert_eq!(*kind,end);let (part,note)=notes.settings_note(*kind,*id).unwrap();let mut runs:Vec<(String,Option<u32>,Option<u32>,Option<bool>,Option<bool>)>=Vec::new();for run in note.descendants().into_iter().filter(|node|node.name=="w:r"){let text=exact_native_text(run);if text.is_empty(){continue;}let metrics=index.run(part,run).unwrap();if let Some(previous)=runs.last_mut().filter(|previous|(previous.1,previous.2,previous.3,previous.4)==(metrics.size,metrics.size_cs,metrics.cs,metrics.rtl)){previous.0.push_str(&text);}else{runs.push((text,metrics.size,metrics.size_cs,metrics.cs,metrics.rtl));}}rows.push((part.to_string(),exact_native_text(note),runs));} (exact_native_text(body),rows,refs)
}
fn generated_author_expected_diagnostics(original:&Manifest)->Vec<Value>{
    // Two generated notes each own a main anchor and a note binding. The
    // same family also owns one author-only citation and one bibliography.
    // Derive identities and source spans from that immutable native manifest,
    // never from the diagnostics or warnings being tested.
    assert_eq!(original.records.len(),6);assert_eq!(original.records.iter().filter(|record|record.generated).count(),4);assert_eq!(original.records.iter().filter(|record|!record.generated&&record.kind=="citation").count(),1);assert_eq!(original.records.iter().filter(|record|record.kind=="bibliography").count(),1);
    original.records.iter().map(|record|{
        let reason=if record.generated{"generated_citation_note_author_format_not_reproducible"}else{"citation_family_author_format_not_reproducible"};
        let mut expected=json!({"code":domain::CONFLICT,"reason":reason,"binding":record.tag(),"part":record.part,"original_source":record.original_source,"current_source":record.source,"source_revision":original.source_revision,"visible_content_preserved":true});if let Some(span)=&record.span{expected["origin_span"]=json!(span);}expected
    }).collect()
}
fn assert_generated_author_imports(source:&str,expected:&[Value]){
    let mut actual=domain::import_diagnostics(source);let mut expected=expected.to_vec();
    let key=|diagnostic:&Value|(diagnostic["binding"].as_str().expect("binding in every expected current-family diagnostic").to_string(),diagnostic["part"].as_str().expect("story in every expected current-family diagnostic").to_string());
    actual.sort_by_key(&key);expected.sort_by_key(&key);
    // Sort only to ignore native traversal order (bibliography can come
    // first). Keep every row and every field: duplicates, omissions, extra
    // diagnostics and changed source/part/span/reason must all fail.
    assert_eq!(actual,expected,"each native citation-family binding needs exactly its complete current-source diagnostic");
}
fn generated_author_reexport(source:&str,expected_imports:&[Value])->Vec<(String,Vec<u8>)>{
    assert_generated_author_imports(source,expected_imports);
    let options=crate::publication_profile::resolve_for_format("docx",source,&json!({"frontMatter":{"mode":"none"}})).unwrap();let (doc,warnings)=crate::mdexport::publication_document(source,"unavailable-original-resource-directory",options.values()).unwrap();
    // publication emits one warning for each import diagnostic, without
    // deduplication. This list comes from the independently expected native
    // bindings above, not from the source parser's actual diagnostic count.
    let expected_warnings=expected_imports.iter().map(|diagnostic|{assert_eq!(diagnostic["code"],domain::CONFLICT);domain::CONFLICT.to_string()}).collect::<Vec<_>>();assert_eq!(warnings,expected_warnings);
    let style=crate::export_styles::sanitize_options(Some(options.values()));let package=crate::docx_writer::build_with_lines(&doc,&style,"Reopened.md",&|_|None,options.line_context());assert!(!package.parts.is_empty(),"{:?}",package.warns);assert!(package.warns.is_empty(),"{:?}",package.warns);package.parts
}

#[test]
fn mixed_mode_changed_note_template_keeps_current_author_axes_body_repeats_resources_and_entire_literal_family(){
    for end in [false,true]{for bibliography_first in [false,true]{
        let (_dir,_source,mut parts)=generated_author_family_package_mode(end,bibliography_first,true);let original=manifest(&parts);let resources=original.resources.clone().unwrap();let story=if end{"word/endnotes.xml"}else{"word/footnotes.xml"};let before_story=docx_notes::unique(&parts,story).unwrap().to_vec();let before_manifest=docx_notes::unique(&parts,domain::PART).unwrap().to_vec();
        alter(&mut parts,"word/styles.xml",|root|{let id=if end{"EndnoteText"}else{"FootnoteText"};let style=root.children.iter_mut().find(|node|node.name=="w:style"&&node.attr("w:styleId")==Some(id)).unwrap();let properties=node_mut(style,"w:rPr").unwrap();for name in ["w:sz","w:szCs"]{node_mut(properties,name).unwrap().attrs.iter_mut().find(|(key,_)|key=="w:val").unwrap().1="60".into();}});
        assert_eq!(docx_notes::unique(&parts,story).unwrap(),before_story);assert_eq!(docx_notes::unique(&parts,domain::PART).unwrap(),before_manifest,"native-only style edit must not re-sign provenance");let current=generated_author_native_snapshot(&parts,end);assert_eq!(current.1.len(),3);assert_eq!(current.2.len(),4);assert_eq!(current.2,vec![0,1,2,2],"both repeated references must retain the current manual note identity");assert!(current.0.contains("BODY_BEGIN")&&current.0.contains("BODY_END")&&current.0.contains("FIRST_TITLE / PUB_A")&&current.0.contains("SECOND_TITLE / PUB_B"));
        let before=parts.clone();let returned=imported(&parts);assert_eq!(parts,before,"in-memory conversion changed its input package");assert!(cites(&returned).is_empty(),"the complete dependency family must stay current literal content, not be partially renumbered: {returned}");let recovered=Resources::from_markdown(&returned).unwrap().unwrap();assert!(recovered.manual_bibliography);assert_eq!(recovered.bibliography,resources.bibliography);assert_eq!(recovered.style,resources.style);assert_eq!(recovered.files.iter().map(|file|(&file.name,&file.sha256,&file.text)).collect::<Vec<_>>(),resources.files.iter().map(|file|(&file.name,&file.sha256,&file.text)).collect::<Vec<_>>());
        let expected_imports=generated_author_expected_diagnostics(&original);assert!(!domain::import_diagnostics(&returned).iter().any(|diagnostic|diagnostic["reason"]=="citation_render_modified"),"unchanged XML must not be falsely called modified");
        let second=generated_author_reexport(&returned,&expected_imports);let actual=generated_author_native_snapshot(&second,end);assert_eq!(actual.0,current.0,"current complete body, numbering and bibliography text changed");assert_eq!(actual.1,current.1,"every current note text, marker gap and independent author axis must survive source return");assert_eq!(actual.2,current.2,"repeated/manual/generated reference targets or counts changed");
        let reopened=imported(&second);let third=generated_author_reexport(&reopened,&[]);assert_eq!(generated_author_native_snapshot(&third,end),actual,"a second source reopen must not resurrect the original source citation format");
    }}
}

#[test]
fn equivalent_generated_note_family_still_restores_structured_citations_without_conflicts(){
    for end in [false,true]{for bibliography_first in [false,true]{for bookmarked in [false,true]{let (_dir,_source,mut parts)=generated_author_family_package(end,bibliography_first);if bookmarked{generated_author_bookmark_markers(&mut parts,end);}let returned=imported(&parts);assert_eq!(cites(&returned).len(),3);assert!(!returned.contains("citation-import:v1"));assert!(returned.contains("<!--readmd:bibliography-->"));assert!(!Resources::from_markdown(&returned).unwrap().unwrap().manual_bibliography);let doc=crate::md_ast::parse(&returned);let definitions=doc.blocks.iter().filter_map(|block|if let crate::md_ast::Block::FootnoteDef{label,..}=block.unstyled(){Some(label)}else{None}).collect::<Vec<_>>();assert_eq!(definitions.len(),1,"only the actual author note remains: {returned}");let mut references=Vec::new();crate::md_ast::walk_inlines(&doc.blocks,&mut|inline|if let crate::md_ast::Inline::FootnoteRef(label)=inline{references.push(label.clone());});assert_eq!(references,vec![definitions[0].clone(),definitions[0].clone()]);}}}
}


#[test]
fn generated_author_marker_format_errors_propagate_from_inline_or_preceding_bibliography_restore(){
    for end in [false,true]{for bibliography_first in [false,true]{for bookmarked in [false,true]{
        let (_dir,_source,mut parts)=generated_author_family_package(end,bibliography_first);if bookmarked{generated_author_bookmark_markers(&mut parts,end);}let reference=if end{"EndnoteReference"}else{"FootnoteReference"};
        let identities=manifest(&parts).records.iter().filter(|record|record.generated&&record.part=="word/document.xml").map(|record|record.note.as_ref().unwrap().id).collect::<Vec<_>>();let signatures=identities.iter().map(|id|note_signature(&parts,end,*id).unwrap()).collect::<Vec<_>>();let auxiliary=parts.iter().filter(|(name,_)|name.starts_with("customXml/")).cloned().collect::<Vec<_>>();
        alter(&mut parts,"word/styles.xml",|root|{let style=root.children.iter_mut().find(|node|node.name=="w:style"&&node.attr("w:styleId")==Some(reference)).unwrap();let properties=node_mut(style,"w:rPr").unwrap();properties.children.push(docx_notes::xml(format!(r#"<w:sz xmlns:w="{}" w:val="60"/>"#,crate::math_document::WORD_NS).as_bytes()).unwrap());});
        for (id,signature)in identities.iter().zip(signatures){assert_eq!(note_signature(&parts,end,*id).unwrap(),signature,"style-only mutation must still have the same verified native note signature");}assert_eq!(parts.iter().filter(|(name,_)|name.starts_with("customXml/")).cloned().collect::<Vec<_>>(),auxiliary);
        let document=docx_notes::xml(docx_notes::unique(&parts,"word/document.xml").unwrap()).unwrap();let body=el_child(&document,"w:body").unwrap();let rels=docx_notes::relationships(&parts,"word/document.xml").unwrap();let notes=docx_notes::Notes::load(&parts,body,&rels).unwrap();let index=super::super::docx_text_style::RunStyleIndex::build(&parts,&rels,body,&notes).unwrap();let marker_name=if end{"w:endnoteRef"}else{"w:footnoteRef"};
        for id in &identities{let (part,note)=notes.settings_note(end,*id).unwrap();let markers=note.descendants().into_iter().filter(|node|node.name=="w:r"&&node.children.iter().any(|child|child.name==marker_name)).collect::<Vec<_>>();assert_eq!(markers.len(),1);assert_eq!(index.run(part,markers[0]).unwrap().size,Some(60),"check the actual current native marker, even when a bookmark is its first sibling");let error=index.generated_note_author_reproducible(part,note).unwrap_err();assert_eq!(error.code,"author_inline_text_unsupported");assert_eq!(error.part,part);}
        let before=parts.clone();let error=docx_convert(&parts,Path::new("marker-format.docx"),false).unwrap_err();assert!(error.starts_with("author_inline_text_unsupported:"),"{error}");let details:Value=serde_json::from_str(error.split_once(':').unwrap().1).unwrap();assert_eq!(details["part"],if end{"word/endnotes.xml"}else{"word/footnotes.xml"});assert_eq!(details["detail"],"generated note marker or gap author axes cannot be reproduced by the source note marker template");assert_eq!(parts,before,"typed structural-format rejection changed the original package");
    }}}
}


#[test]
fn generated_author_diagnostic_oracle_rejects_missing_duplicate_and_changed_binding_sources(){
    let raw="[@42, p. 12]";let span=crate::publication_rich::SourceSpan::new(raw,0..raw.len()).unwrap();let expected=[("owned-main","word/document.xml"),("owned-note","word/footnotes.xml")].into_iter().map(|(binding,part)|json!({"code":domain::CONFLICT,"reason":"generated_citation_note_author_format_not_reproducible","binding":binding,"part":part,"original_source":raw,"current_source":raw,"source_revision":"0".repeat(64),"origin_span":span,"visible_content_preserved":true})).collect::<Vec<_>>();
    let annotation=|diagnostics:&[Value]|format!("<!--readmd:citation-import:v1\n{}\n-->\n",json!({"version":1,"diagnostics":diagnostics}));assert_generated_author_imports(&annotation(&expected),&expected);
    let mut variants=Vec::new();let mut missing=expected.clone();missing.pop();variants.push(missing);let mut duplicate=expected.clone();duplicate.push(duplicate[0].clone());variants.push(duplicate);
    for field in ["binding","part","original_source","current_source","source_revision","reason"]{let mut changed=expected.clone();changed[0][field]=json!(match field{"part"=>"word/endnotes.xml","source_revision"=>"1111111111111111111111111111111111111111111111111111111111111111","reason"=>"citation_family_author_format_not_reproducible",_=>"different"});variants.push(changed);}
    let mut span_changed=expected.clone();span_changed[0]["origin_span"]["from"]=json!(1);span_changed[0]["origin_span"]["to"]=json!(raw.encode_utf16().count()+1);variants.push(span_changed);
    for changed in variants{let source=annotation(&changed);assert!(std::panic::catch_unwind(||assert_generated_author_imports(&source,&expected)).is_err(),"forged/missing/duplicate current-source diagnostic was accepted: {source}");}
}

fn generated_author_change_template(parts:&mut[(String,Vec<u8>)],end:bool){
    alter(parts,"word/styles.xml",|root|{
        let id=if end{"EndnoteText"}else{"FootnoteText"};
        let style=root.children.iter_mut().find(|node|node.name=="w:style"&&node.attr("w:styleId")==Some(id)).unwrap();
        let properties=node_mut(style,"w:rPr").unwrap();for name in ["w:sz","w:szCs"]{node_mut(properties,name).unwrap().attrs.iter_mut().find(|(key,_)|key=="w:val").unwrap().1="60".into();}
    });
}
fn generated_author_active_reexport(source:&str)->Vec<(String,Vec<u8>)>{
    let mut parts=generated_author_reexport(source,&[]);
    domain::attach_context(&mut parts,source,"unavailable-original-resource-directory",&json!({})).unwrap();
    parts
}

#[test]
fn uniform_generated_notes_become_active_authored_notes_without_changing_current_paint_or_family_order(){
    for end in [false,true]{for bibliography_first in [false,true]{
        let (_dir,_source,mut parts)=generated_author_family_package(end,bibliography_first);
        if bibliography_first{generated_author_bookmark_markers(&mut parts,end);}
        let original=manifest(&parts);let resources=original.resources.as_ref().unwrap();let story=if end{"word/endnotes.xml"}else{"word/footnotes.xml"};
        let original_story=docx_notes::unique(&parts,story).unwrap().to_vec();let original_aux=docx_notes::unique(&parts,domain::PART).unwrap().to_vec();
        generated_author_change_template(&mut parts,end);
        assert_eq!(docx_notes::unique(&parts,story).unwrap(),original_story);assert_eq!(docx_notes::unique(&parts,domain::PART).unwrap(),original_aux,"style-only current values must not re-sign the historical carrier");
        let before=parts.clone();let current=generated_author_native_snapshot(&parts,end);let returned=imported(&parts);assert_eq!(parts,before);
        assert!(domain::import_diagnostics(&returned).is_empty(),"{returned}");
        let recovered=Resources::from_markdown(&returned).unwrap().unwrap();assert!(!recovered.manual_bibliography);assert_eq!(recovered.bibliography,resources.bibliography);assert_eq!(recovered.style,resources.style);
        let prepared=crate::publication::prepare(&returned,"unavailable-original-resource-directory").unwrap();
        let citations=&prepared.projection.citations;assert_eq!(citations.len(),3);let authored=citations.iter().filter(|citation|citation.source.context.in_note).collect::<Vec<_>>();assert_eq!(authored.len(),2);
        assert_eq!(authored.iter().map(|citation|citation.source.context.note_number).collect::<Vec<_>>(),vec![Some(1),Some(2)]);
        assert_ne!(authored[0].source.span.byte_from,authored[1].source.span.byte_from,"equal citation text must keep two independent native note identities");
        assert!(authored.iter().all(|citation|citation.source.span.source=="[@42, p. 12]"&&citation.source.items[0].cite.key=="42"));
        let author_style=crate::publication_inline_text::AuthorTextStyle::from_value(&json!({"size":10,"sizeCs":10})).unwrap();
        assert_eq!(returned.matches(&author_style.wrap_source("[@42, p. 12]")).count(),2,"each actual current note has one readable author scope: {returned}");
        let doc=crate::md_ast::parse(&returned);assert_eq!(doc.blocks.iter().filter(|block|matches!(block.unstyled(),crate::md_ast::Block::FootnoteDef{..})).count(),3);
        let second=generated_author_active_reexport(&returned);let native=manifest(&second);
        assert!(native.records.iter().all(|record|!record.generated),"the new authored-note package must not falsely preserve generated provenance");
        let notes=native.records.iter().filter(|record|record.kind=="citation"&&record.part==story).collect::<Vec<_>>();assert_eq!(notes.len(),2);assert!(notes.iter().all(|record|record.note.as_ref().is_some_and(|note|note.end==end)&&record.source=="[@42, p. 12]"));
        assert_eq!(native.records.iter().filter(|record|record.kind=="bibliography").count(),1);
        assert_eq!(generated_author_native_snapshot(&second,end),current,"full native body/note text, current sizes and repeated-reference topology must survive");
        let reopened=imported(&second);let third=generated_author_active_reexport(&reopened);
        assert_eq!(generated_author_native_snapshot(&third,end),current,"the next native return must not resurrect the old generated formatting");
        assert!(manifest(&third).records.iter().all(|record|!record.generated));
    }}
}

#[test]
fn generated_authored_recovery_keeps_non_equivalent_csl_note_layouts_in_the_complete_literal_family(){
    let original=style("note");
    for csl in [
        original.replace(r#"prefix="(" suffix=")" "#, ""),
        original.replace(r#"prefix="(" suffix=")" "#, r#"prefix="(" "#),
        original.replace(r#"<text variable="citation-number"/>"#,r#"<text term="ibid"/>"#),
    ] { for end in [false,true] {
        let (_dir,_source,mut parts)=generated_author_family_package_style(end,true,false,&csl);
        let original_manifest=manifest(&parts);
        generated_author_change_template(&mut parts,end);
        let current=generated_author_native_snapshot(&parts,end);
        let before=parts.clone();let returned=imported(&parts);assert_eq!(parts,before);
        assert!(cites(&returned).is_empty(),"new authored context must not silently change the original visible note: {returned}");
        let resources=Resources::from_markdown(&returned).unwrap().unwrap();assert!(resources.manual_bibliography);
        let expected=generated_author_expected_diagnostics(&original_manifest);assert_generated_author_imports(&returned,&expected);
        assert_eq!(generated_author_native_snapshot(&generated_author_reexport(&returned,&expected),end),current,"non-equivalent layouts must retain every current text/run/reference through literal re-export");
    }}
}

#[test]
fn one_modified_or_duplicate_member_prevents_partial_authored_family_reactivation(){
    for duplicate in [false,true]{
        let (_dir,_source,mut parts)=generated_author_family_package(false,true);let original=manifest(&parts);let anchors=original.records.iter().filter(|record|record.generated&&record.part=="word/document.xml").collect::<Vec<_>>();
        generated_author_change_template(&mut parts,false);
        if duplicate{
            alter(&mut parts,"word/document.xml",|document|{let copy=control_mut(document,&anchors[0].tag()).unwrap().clone();let body=node_mut(document,"w:body").unwrap();let paragraph=body.children.iter_mut().find(|node|node.name=="w:p").unwrap();paragraph.children.push(copy);});
        }else{let id=anchors[0].note.as_ref().unwrap().id;replace_content(&mut parts,anchors[0],&format!(r#"<w:r><w:t>CURRENT_CHANGED_ANCHOR</w:t><w:footnoteReference w:id="{id}"/></w:r>"#));}
        let before=parts.clone();let returned=imported(&parts);assert_eq!(parts,before);
        assert!(cites(&returned).is_empty(),"no good member may reactivate after one family binding changes: {returned}");
        assert!(Resources::from_markdown(&returned).unwrap().unwrap().manual_bibliography);
        assert!(returned.contains("FIRST_TITLE")&&returned.contains("SECOND_TITLE")&&returned.contains("MANUAL_CURRENT"));
        assert!(domain::import_diagnostics(&returned).iter().any(|diagnostic|diagnostic["reason"]=="citation_family_author_format_not_reproducible"));
    }
}
