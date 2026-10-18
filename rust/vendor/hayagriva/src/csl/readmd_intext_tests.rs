//! Original tests of the optional author-layout hook; no upstream fixture data.
use super::*;
use crate::types::{EntryType, Person};

fn style(layout: &str) -> IndependentStyle {
    IndependentStyle::from_xml(&format!(r#"<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text"><info><title>Original author layout hook test</title><id>https://readmd.test/intext-hook</id></info><citation>{layout}</citation></style>"#)).unwrap()
}
fn entry() -> crate::Entry {
    let mut entry = crate::Entry::new("atlas", EntryType::Book);
    entry.set_authors(vec![Person {
        name: "Park".into(),
        given_name: Some("Adrian".into()),
        prefix: None,
        suffix: None,
        comma_suffix: false,
        alias: None,
    }]);
    entry
}

#[test]
fn readmd_intext_layout_is_explicit_and_restores_the_prose_locator() {
    let style = style(
        r#"<layout prefix="(" suffix=")"><group delimiter=" / "><names variable="author"><name form="short"/></names><text variable="locator"/></group></layout>"#,
    );
    let author_style = self::style(
        r#"<layout><names variable="author"><name/></names><text variable="locator" prefix=" WRONG "/></layout>"#,
    );
    let intext = InText {
        layout: author_style.citation.layout,
        name_options: author_style.citation.name_options,
    };
    let entry = entry();
    let mut driver = BibliographyDriver::new();
    for purpose in [
        Some(CitePurpose::Author),
        Some(CitePurpose::SuppressAuthor),
        None,
        Some(CitePurpose::Prose),
    ] {
        let item = CitationItem::new(
            &entry,
            Some(SpecificLocator(Locator::Page, LocatorPayload::Str("27"))),
            None,
            false,
            purpose,
        );
        driver.citation(
            CitationRequest::new(vec![item], &style, None, &[], None).with_intext(Some(&intext)),
        );
    }
    let rendered = driver.finish(BibliographyRequest::new(&style, None, &[]));
    let text: Vec<_> = rendered
        .citations
        .iter()
        .map(|cite| format!("{:#}", cite.citation))
        .collect();
    assert_eq!(
        text,
        ["Adrian Park", "(27)", "(Park / 27)", "Adrian Park (27)"]
    );
}

#[test]
fn readmd_intext_absence_preserves_the_existing_author_fallback() {
    let style = style(
        r#"<layout prefix="(" suffix=")"><names variable="author"><name form="short"/></names></layout>"#,
    );
    let entry = entry();
    let mut driver = BibliographyDriver::new();
    for explicit_none in [false, true] {
        let item = CitationItem::new(&entry, None, None, false, Some(CitePurpose::Author));
        let request = CitationRequest::from_items(vec![item], &style, &[]);
        driver.citation(if explicit_none {
            request.with_intext(None)
        } else {
            request
        });
    }
    let rendered = driver.finish(BibliographyRequest::new(&style, None, &[]));
    assert!(
        rendered
            .citations
            .iter()
            .all(|cite| format!("{:#}", cite.citation) == "Park")
    );
}
