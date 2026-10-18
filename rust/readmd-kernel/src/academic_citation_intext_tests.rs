//! Hand-authored integration regressions for optional cs:intext compatibility.
//! Original upstream CSL/INPUT/RESULT remain private QA assets, not this source.
use crate::academic::{self, Request};
use crate::publication_rich::{RichFontStyle, RichFontVariant, RichNode, plain};
use serde_json::{Value, json};

const CITATION: &str = r#"<citation><layout prefix="(" suffix=")" delimiter="; "><group><group delimiter=" "><names variable="author"><name form="short"/></names><date variable="issued" form="text" date-parts="year"/></group><text variable="locator" prefix=", p. "/></group></layout></citation>"#;
const INTEXT: &str =
    r#"<intext><layout><names variable="author"><name/></names></layout></intext>"#;
fn source(class: &str, body: &str) -> String {
    format!(
        r#"<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="{class}"><info><title>ReadMD original author-source regression</title><id>https://readmd.test/intext-author-source</id></info>{CITATION}<bibliography><layout><text variable="title"/></layout></bibliography>{body}</style>"#
    )
}
fn data() -> Value {
    json!([
        {"id":"atlas","type":"book","title":"Atlas","author":[{"given":"Adrian","family":"Park"}],"issued":{"date-parts":[[2031]]}},
        {"id":"index","type":"book","title":"Index","author":[{"given":"Mira","family":"Reed"}],"issued":{"date-parts":[[2034]]}},
        {"id":"anonymous","type":"book","title":"Anonymous Field","issued":{"date-parts":[[2037]]}}
    ])
}
fn request(style: String, clusters: Value) -> Request {
    academic::parse_request(json!({"format":"csl-json","data":data().to_string(),"csl":style,"locale":"en-US","clusters":clusters})).unwrap()
}
fn texts(output: &Value) -> Vec<&str> {
    output["citations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|cite| cite["text"].as_str().unwrap())
        .collect()
}
fn nodes(value: &Value) -> Vec<RichNode> {
    serde_json::from_value(value.clone()).unwrap()
}
fn formatted(nodes: &[RichNode], target: &str) -> bool {
    nodes.iter().any(|node| match node {
        RichNode::Text { text, format } => {
            text.contains(target)
                && format.font_style == RichFontStyle::Italic
                && format.font_variant == RichFontVariant::SmallCaps
        }
        RichNode::Group { children, .. } | RichNode::Link { children, .. } => {
            formatted(children, target)
        }
    })
}

#[test]
fn academic_citation_intext_overrides_author_source_without_changing_standard_modes_or_raw_inputs()
{
    let style = source("in-text", INTEXT);
    let request = request(
        style.clone(),
        json!([[{"key":"atlas","mode":"author-only"}],[{"key":"atlas","mode":"suppress-author"}],[{"key":"index"}]]),
    );
    let output = academic::citations(&request).unwrap();
    assert_eq!(texts(&output), ["Adrian Park", "(2031)", "(Reed 2034)"]);
    assert_eq!(output["citations"][0]["note_number"], Value::Null);
    assert_eq!(output["bibliography"]["items"][0]["text"], "Atlas");
    assert_eq!(output["bibliography"]["items"][1]["text"], "Index");
    assert_eq!(request.csl.as_deref(), Some(style.as_str()));
    assert_eq!(
        serde_json::from_str::<Value>(&request.data).unwrap(),
        data()
    );
    assert_eq!(output["diagnostics"], json!([]));
}

#[test]
fn academic_citation_intext_absence_and_lexical_mentions_keep_the_short_author_fallback() {
    for extra in [
        "",
        "<!-- intext is not an XML element here -->",
        r#"<macro name="unused"><text value="&lt;intext&gt;"/></macro>"#,
    ] {
        let request = request(
            source("in-text", extra),
            json!([[{"key":"atlas","mode":"author-only"}],[{"key":"atlas"}]]),
        );
        assert_eq!(
            texts(&academic::citations(&request).unwrap()),
            ["Park", "(Park 2031)"]
        );
    }
}

#[test]
fn academic_citation_intext_inherits_its_own_name_options_and_preserves_typed_macro_formatting() {
    let extra = r#"<macro name="narrative-names"><names variable="author" font-style="italic"><name font-variant="small-caps"/></names></macro><intext initialize-with=". "><layout><text macro="narrative-names"/></layout></intext>"#;
    let request = request(
        source("in-text", extra),
        json!([[{"key":"atlas","mode":"author-only","prefix":"see ","suffix":" again"}],[{"key":"atlas"}]]),
    );
    let output = academic::citations(&request).unwrap();
    assert_eq!(texts(&output), ["see A. Park again", "(Park 2031)"]);
    let rich = nodes(&output["citations"][0]["rich"]);
    assert!(
        formatted(&rich, "Park"),
        "explicit intext macro formatting must survive the typed projection: {rich:?}"
    );
    assert!(
        output["citations"][0]["html"]
            .as_str()
            .unwrap()
            .contains("readmd-csl-smallcaps")
    );
}

#[test]
fn academic_citation_intext_prose_restores_locator_and_keeps_note_author_outside_the_note() {
    let extra = r#"<intext><layout><group delimiter=" "><names variable="author"><name/></names><text variable="locator" prefix="WRONG-LOCATOR-"/></group></layout></intext>"#;
    for class in ["in-text", "note"] {
        let mut request = request(
            source(class, extra),
            json!([[{"key":"atlas","mode":"author-only","locator":"27"}],[{"key":"atlas","mode":"prose","locator":"27"}]]),
        );
        request.contexts = vec![
            academic::CiteContext::default(),
            academic::CiteContext {
                note_number: Some(2),
                ..Default::default()
            },
        ];
        let output = academic::citations(&request).unwrap();
        assert_eq!(texts(&output)[0], "Adrian Park");
        assert_eq!(
            texts(&output)[1],
            if class == "note" {
                "Adrian Park 2031, p. 27"
            } else {
                "Adrian Park (2031, p. 27)"
            }
        );
        assert!(!texts(&output).join(" ").contains("WRONG-LOCATOR"));
        if class == "note" {
            assert_eq!(
                plain(&nodes(&output["citations"][1]["inline_rich"])),
                "Adrian Park"
            );
            assert_eq!(
                plain(&nodes(&output["citations"][1]["note_rich"])),
                "2031, p. 27"
            );
            assert_eq!(output["citations"][1]["note_number"], 2);
        }
    }
}

#[test]
fn academic_citation_intext_legal_anonymous_substitute_uses_the_explicit_layout() {
    let extra = r#"<intext><layout><names variable="author"><name/><substitute><text variable="title" font-style="italic"/></substitute></names></layout></intext>"#;
    let output = academic::citations(&request(
        source("in-text", extra),
        json!([[{"key":"anonymous","mode":"author-only"}]]),
    ))
    .unwrap();
    assert_eq!(texts(&output), ["Anonymous Field"]);
    assert_eq!(output["diagnostics"], json!([]));
    let html = output["citations"][0]["html"].as_str().unwrap();
    assert!(html.contains("readmd-csl-italic") && !html.contains("2037"));
}

#[test]
fn academic_citation_intext_prefixed_namespace_and_escaped_text_are_parsed_as_data() {
    let extra = r#"<c:intext xmlns:c="http://purl.org/net/xbiblio/csl"><c:layout><c:group delimiter=" "><c:text value="A&amp;B &lt;quoted&gt;"/><c:names variable="author"><c:name/></c:names></c:group></c:layout></c:intext>"#;
    let output = academic::citations(&request(
        source("in-text", extra),
        json!([[{"key":"atlas","mode":"author-only"}]]),
    ))
    .unwrap();
    assert_eq!(texts(&output), ["A&B <quoted> Adrian Park"]);
    let html = output["citations"][0]["html"].as_str().unwrap();
    assert!(html.contains("A&amp;B &lt;quoted&gt;") && !html.contains("<quoted>"));
}

#[test]
fn academic_citation_intext_invalid_or_unsupported_extensions_fail_even_for_normal_citations() {
    for extra in [
        "<intext/>",
        "<intext><names variable=\"author\"><name/></names></intext>",
        "<intext><layout/><layout/></intext>",
        "<intext xmlns=\"urn:wrong\"><layout/></intext>",
        "<macro name=\"wrong-position\"><intext><layout/></intext></macro>",
        "<intext collapse=\"year\"><layout/></intext>",
        "<intext><sort/><layout/></intext>",
        "<intext><layout><text variable=\"not-a-csl-variable\"/></layout></intext>",
        "<intext><layout/></intext><intext><layout/></intext>",
    ] {
        let style = source("in-text", extra);
        let error =
            academic::citations(&request(style.clone(), json!([[{"key":"atlas"}]]))).unwrap_err();
        assert_eq!(
            error.code, "academic_style",
            "unsupported extension silently accepted: {extra}"
        );
        assert_eq!(
            academic::is_note_style("unused", Some(&style))
                .unwrap_err()
                .code,
            "academic_style"
        );
    }
}

#[test]
fn academic_citation_intext_unprintable_author_is_not_rescued_by_affixes_or_another_item() {
    let empty = r#"<intext><layout><names variable="translator"><name/></names></layout></intext>"#;
    for mode in ["author-only", "prose"] {
        let mut request = request(
            source("in-text", empty),
            json!([[{"key":"atlas","mode":mode,"prefix":"see ","suffix":" again"}]]),
        );
        assert_eq!(
            academic::citations(&request).unwrap_err().code,
            "academic_unprintable_reference"
        );
        request.clusters[0].push(academic::Cite::new("index"));
        assert_eq!(
            academic::citations(&request).unwrap_err().code,
            "academic_unprintable_reference"
        );
    }
    let output = academic::citations(&request(
        source("in-text", empty),
        json!([[{"key":"atlas"}]]),
    ))
    .unwrap();
    assert_eq!(
        texts(&output),
        ["(Park 2031)"],
        "an unused valid intext does not change normal citations"
    );
}
