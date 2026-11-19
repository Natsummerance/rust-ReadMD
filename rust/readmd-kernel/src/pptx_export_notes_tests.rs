use super::*;
use crate::math_document::{parse_xml, XmlNode};

fn child<'a>(node: &'a XmlNode, namespace: &str, name: &str) -> &'a XmlNode {
    node.children().find(|child| child.namespace == namespace && child.local() == name)
        .unwrap_or_else(|| panic!("missing {name} in {}", node.name))
}

#[test]
fn multiline_notes_survive_pagination_and_round_trip_as_editable_text() {
    let public_body = (0..32).map(|n| format!(
        "Public record {n:02}: December observation for the release review.\n\n"
    )).collect::<String>();
    let notes = format!(
        "讲稿开始 A & B\n\n### Private notes heading\n\n{}\n\n讲稿结束 😀",
        "Private detail ".repeat(70).trim_end()
    );
    let source = format!(
        "# Public start\n\n{public_body}<!-- note -->\n{notes}\n\n<!-- slide -->\n# Public boundary\n\nFinal slide."
    );
    let deck = build(&source, "", "Notes round-trip", &json!({})).unwrap();
    assert!(deck.slides.len() > 3);
    let html = preview_html(&deck).unwrap();
    assert!(html.contains("Public boundary"));
    for secret in ["讲稿开始", "Private notes heading", "Private detail", "讲稿结束"] {
        assert!(!html.contains(secret), "{secret} became slide content");
    }
    let bytes = package(&deck, "Notes round-trip").unwrap();
    let entries = crate::convert::parse_zip_entries(&bytes).unwrap();
    let part = |name: &str| {
        std::str::from_utf8(&entries.iter().find(|(path, _)| path == name).unwrap().1).unwrap()
    };
    for n in 1..=deck.slides.len() {
        let root = parse_xml(part(&format!("ppt/notesSlides/notesSlide{n}.xml"))).unwrap();
        let tree = child(child(child(&root, P, "notes"), P, "cSld"), P, "spTree");
        let shape = child(tree, P, "sp");
        let placeholder = child(child(child(shape, P, "nvSpPr"), P, "nvPr"), P, "ph");
        assert_eq!(placeholder.attributes["type"], "body");
        let paragraphs = child(shape, P, "txBody").children()
            .filter(|node| node.namespace == A && node.local() == "p")
            .map(XmlNode::text).collect::<Vec<_>>();
        let expected = if n < deck.slides.len() { notes.as_str() } else { "" };
        assert!(!paragraphs.is_empty());
        assert_eq!(paragraphs.join("\n"), expected);
        let slide_rels = part(&format!("ppt/slides/_rels/slide{n}.xml.rels"));
        assert!(slide_rels.contains(&format!("../notesSlides/notesSlide{n}.xml")));
        let note_rels = part(&format!("ppt/notesSlides/_rels/notesSlide{n}.xml.rels"));
        assert!(note_rels.contains(&format!("../slides/slide{n}.xml")));
        assert!(note_rels.contains("../notesMasters/notesMaster1.xml"));
    }

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Notes round-trip.pptx");
    std::fs::write(&path, bytes).unwrap();
    let imported = crate::convert::convert_verbose(path.to_str().unwrap(), true).unwrap();
    assert!(imported.success);
    assert_eq!(imported.engine.as_deref(), Some("pptx"));
    let content = imported.content.unwrap();
    assert_eq!(content.matches(&format!("### Notes\n{notes}")).count(), deck.slides.len() - 1);
    assert!(content.contains("Public boundary"));
}

#[test]
fn headings_inside_notes_stay_private_without_a_slide_marker() {
    let source = "# Public heading\n\nPublic text.\n\n<!-- note -->\nNote start\n\n### Private heading\n\nNote end";
    let deck = build(source, "", "Notes", &json!({})).unwrap();
    assert_eq!(deck.slides.len(), 1);
    let notes = "Note start\n\n### Private heading\n\nNote end";
    assert_eq!(deck.slides[0].notes, notes);
    assert!(!preview_html(&deck).unwrap().contains("Private heading"));
    assert_eq!(crate::mdexport::split_slides_structure(source),
        vec![vec![("# Public heading\n\nPublic text.".to_string(), notes.to_string())]]);
}

#[test]
fn formula_source_stays_with_its_own_image_when_a_table_continues() {
    let rows = "| cell |\n".repeat(24);
    let source = format!(
        "# Formula slide\n\n$$\nx+37\n$$\n\n| Data |\n| --- |\n{rows}\n<!-- note -->\nSpeaker annotation"
    );
    let deck = build(&source, "", "Formula notes", &json!({})).unwrap();
    assert!(deck.slides.len() >= 3);
    assert_eq!(deck.slides.iter().filter(|slide| slide.notes.contains("x+37")).count(), 1);
    for slide in &deck.slides {
        assert!(slide.notes.starts_with("Speaker annotation"));
        let has_formula = slide.shapes.iter().any(|shape| matches!(shape, Shape::Image { alt, .. } if alt.contains("x+37")));
        assert_eq!(slide.notes.contains("x+37"), has_formula);
    }
}

#[test]
fn explicit_slide_and_subslide_delimiters_still_end_speaker_notes() {
    for separator in ["<!-- slide -->", "<!-- subslide -->", "---", "--"] {
        let source = format!(
            "# First\n\nFirst body\n\n<!-- note -->\nPRIVATE_NOTE\n\n{separator}\n\n# Second\n\nPUBLIC_BOUNDARY"
        );
        let deck = build(&source, "", "Boundaries", &json!({})).unwrap();
        assert_eq!(deck.slides.len(), 2, "{separator}");
        assert_eq!(deck.slides[0].notes, "PRIVATE_NOTE", "{separator}");
        assert!(deck.slides[1].notes.is_empty(), "{separator}");
        let html = preview_html(&deck).unwrap();
        assert!(html.contains("PUBLIC_BOUNDARY") && !html.contains("PRIVATE_NOTE"), "{separator}");
    }
}

#[test]
fn presentation_html_keeps_long_notes_in_one_panel_and_example_markup_as_text() {
    let first = "visible paragraph for a long section. ".repeat(12);
    let tail = "note detail for the speaker. ".repeat(40);
    let source = format!(
        "# Public\n\n{first}\n\n{first}\n\n~~~html\n<!-- note -->\n<example>\n</TeXtArEa >\n~~~\n\n<!-- note -->\nNote start.\n\n{tail}\n\nNOTES_ONLY_TAIL\n\n```html\n<note-example>\n</textarea>"
    );
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let html = crate::mdexport::render_presentation_html(&source, "Notes HTML", "white", "none", false, &assets).unwrap();
    let templates = html.split("<textarea data-template>").skip(1).map(|segment| {
        segment.split_once("</textarea>").unwrap().0
    }).collect::<Vec<_>>();
    assert!(templates.len() > 1);
    let opening = "&lt;aside class=\"notes\"&gt;";
    assert_eq!(html.matches(opening).count(), 1);
    let (slide, note) = templates.last().unwrap().split_once(opening).unwrap();
    assert!(!slide.contains("NOTES_ONLY_TAIL"));
    assert!(templates.iter().all(|template| !template.contains("READMDSLIDELITERAL")));
    assert!(templates.iter().any(|template| template.contains("<!-- note -->\n<example>\n&lt;/TeXtArEa >")));
    assert!(note.contains("NOTES_ONLY_TAIL"));
    assert!(note.contains("&amp;lt;note-example&amp;gt;"));
    assert!(note.contains("&amp;lt;/textarea&amp;gt;"));
    assert!(note.contains("&lt;/code&gt;&lt;/pre&gt;"));
    assert!(note.contains("&lt;/aside&gt;"));
    assert!(!html.contains("<note-example>"));
}
