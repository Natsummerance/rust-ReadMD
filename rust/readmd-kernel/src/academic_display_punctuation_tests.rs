//! Original small regressions for punctuation found by independent display RESULTs.
//! No upstream fixture text or expected HTML is embedded here.
use crate::math_document::{parse_xml, XmlNode};
use crate::publication_rich::{plain, RichDisplay, RichFontStyle, RichLayoutNode, RichNode};
use serde_json::{json, Value};

fn process(body: &str, data: Value, locale: &str) -> (crate::academic::Request, Value) {
    let style = format!(
        r#"<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text"><info><title>ReadMD original punctuation regression</title><id>https://readmd.test/display-punctuation</id></info>{body}</style>"#
    );
    let request = crate::academic::parse_request(json!({"format":"csl-json","data":serde_json::to_string(&data).unwrap(),"csl":style,"locale":locale,"clusters":[[{"key":"a"}]]})).unwrap();
    let output = crate::academic::citations(&request).unwrap();
    (request, output)
}
fn rendered(output: &Value) -> &str {
    output["bibliography"]["items"][0]["text"].as_str().unwrap()
}

fn assert_italic(output: &Value, target: &str) {
    fn typed(nodes: &[RichNode], target: &str) -> bool {
        nodes.iter().any(|node| match node {
            RichNode::Text { text, format } => {
                text == target && format.font_style == RichFontStyle::Italic
            }
            RichNode::Group { children, .. } | RichNode::Link { children, .. } => {
                typed(children, target)
            }
        })
    }
    fn safe_html(node: &XmlNode, target: &str) -> bool {
        (node.local() == "span"
            && node.text() == target
            && node.attributes.get("class").is_some_and(|classes| {
                classes
                    .split_ascii_whitespace()
                    .any(|class| class == "readmd-csl-italic")
            })
            && node.attributes.get("style").is_some_and(|style| {
                style
                    .split(';')
                    .any(|declaration| declaration.trim() == "font-style:italic")
            }))
            || node.children().any(|child| safe_html(child, target))
    }
    let item = &output["bibliography"]["items"][0];
    let nodes: Vec<RichNode> = serde_json::from_value(item["rich"].clone()).unwrap();
    assert!(typed(&nodes, target), "typed italic target lost: {item}");
    let html = item["html"].as_str().unwrap();
    assert!(
        safe_html(&parse_xml(html).unwrap(), target),
        "safe HTML italic target lost: {html}"
    );
}

#[test]
fn academic_display_prefix_joins_only_surviving_affix_boundaries_not_field_whitespace() {
    let style = r#"<citation><layout><text variable="title"/></layout></citation><bibliography><layout><text variable="title" suffix=", "/><group prefix=" (cited: " suffix=")."><text variable="container-title" font-style="italic"/></group><group delimiter=", " prefix=" "><text value="part"/><text variable="volume" prefix=" vol. "/></group></layout></bibliography>"#;
    let data = json!([{"id":"a","type":"book","title":"Two  Words","container-title":"Journal","volume":"7"}]);
    let (request, output) = process(style, data.clone(), "en-US");
    assert_eq!(
        rendered(&output),
        "Two  Words, (cited: Journal). part, vol. 7"
    );
    assert_italic(&output, "Journal");
    assert_eq!(
        serde_json::from_str::<Value>(&request.data).unwrap(),
        data,
        "source data is never typeset in place"
    );
}

#[test]
fn academic_display_empty_prefix_rolls_back_and_nonbreaking_or_display_spaces_stay_distinct() {
    let style = r#"<citation><layout><text variable="title"/></layout></citation><bibliography><layout><text variable="title" suffix=" "/><group prefix=" "><text variable="publisher"/></group><text value="Tail"/></layout></bibliography>"#;
    let (_, output) = process(
        style,
        json!([{"id":"a","type":"book","title":"Title"}]),
        "en-US",
    );
    assert_eq!(
        rendered(&output),
        "Title Tail",
        "a discarded prefix must not delete prior sibling whitespace"
    );
    let style = r#"<citation><layout><text variable="title"/></layout></citation><bibliography><layout><text variable="title" suffix="&#160;"/><text variable="publisher" prefix=" "/></layout></bibliography>"#;
    let (_, output) = process(
        style,
        json!([{"id":"a","type":"book","title":"Title","publisher":"House"}]),
        "en-US",
    );
    assert_eq!(rendered(&output), "Title\u{a0} House");
    let style = r#"<citation><layout><text variable="title"/></layout></citation><bibliography><layout><text variable="citation-number" suffix=" " display="left-margin"/><text variable="title" prefix=" " display="right-inline"/></layout></bibliography>"#;
    let (_, output) = process(
        style,
        json!([{"id":"a","type":"book","title":"Body"}]),
        "en-US",
    );
    let item = &output["bibliography"]["items"][0];
    let original: Vec<RichNode> = serde_json::from_value(item["rich"].clone()).unwrap();
    let first: Vec<RichNode> = serde_json::from_value(item["first_field_rich"].clone()).unwrap();
    assert!(
        first.is_empty() && item["first_field"].is_null(),
        "explicit display stays in rich; only second-field-align populates first_field: {item}"
    );
    fn displayed_text(nodes: &[RichNode], display: RichDisplay, output: &mut Vec<String>) {
        for node in nodes {
            match node {
                RichNode::Group {
                    children,
                    display: actual,
                    ..
                } => {
                    if *actual == Some(display) {
                        output.push(plain(children));
                    }
                    displayed_text(children, display, output);
                }
                RichNode::Link { children, .. } => displayed_text(children, display, output),
                RichNode::Text { .. } => {}
            }
        }
    }
    let mut original_left = Vec::new();
    let mut original_right = Vec::new();
    displayed_text(&original, RichDisplay::LeftMargin, &mut original_left);
    displayed_text(&original, RichDisplay::RightInline, &mut original_right);
    assert_eq!(original_left, ["1 "], "original left display: {item}");
    assert_eq!(original_right, [" Body"], "original right display: {item}");
    fn columns<'a>(
        nodes: &'a [RichLayoutNode],
        output: &mut Vec<(&'a [RichLayoutNode], &'a [RichLayoutNode])>,
    ) {
        for node in nodes {
            match node {
                RichLayoutNode::Columns { left, right } => {
                    output.push((left, right));
                    columns(left, output);
                    columns(right, output);
                }
                RichLayoutNode::Block { children, .. }
                | RichLayoutNode::Group { children, .. }
                | RichLayoutNode::Link { children, .. } => columns(children, output),
                RichLayoutNode::Inline { .. } => {}
            }
        }
    }
    fn flow_text(nodes: &[RichLayoutNode]) -> String {
        nodes
            .iter()
            .map(|node| match node {
                RichLayoutNode::Inline { children } => plain(children),
                RichLayoutNode::Block { children, .. }
                | RichLayoutNode::Group { children, .. }
                | RichLayoutNode::Link { children, .. } => flow_text(children),
                RichLayoutNode::Columns { left, right } => flow_text(left) + &flow_text(right),
            })
            .collect()
    }
    let flow: Vec<RichLayoutNode> = serde_json::from_value(item["layout"].clone()).unwrap();
    let mut pairs = Vec::new();
    columns(&flow, &mut pairs);
    assert_eq!(pairs.len(), 1, "one actual left/right column pair: {item}");
    assert_eq!(flow_text(pairs[0].0), "1 ", "left cell: {item}");
    assert_eq!(
        flow_text(pairs[0].1),
        " Body",
        "independent column cells do not share whitespace normalization: {item}"
    );
}

#[test]
fn academic_display_normal_names_and_titles_use_typographic_apostrophes_and_nested_locale_quotes() {
    let style = r#"<citation><layout><text variable="title"/></layout></citation><bibliography><layout><group delimiter=", "><names variable="author"><name form="short"/></names><text variable="title" quotes="true"/><text variable="container-title" font-style="italic"/></group></layout></bibliography>"#;
    let data = json!([{"id":"a","type":"book","author":[{"family":"O'Riley","given":"Sam"}],"title":"Owner's \"Inner\" Book","container-title":"Society's Journal"}]);
    let (request, output) = process(style, data.clone(), "en-US");
    assert_eq!(
        rendered(&output),
        "O’Riley, “Owner’s ‘Inner’ Book,” Society’s Journal"
    );
    assert_italic(&output, "Society’s Journal");
    assert_eq!(serde_json::from_str::<Value>(&request.data).unwrap(), data);
}

#[test]
fn academic_display_quote_terms_follow_explicit_locales_without_changing_machine_values_or_affixes()
{
    for (language, outer, inner) in [
        ("en-US", ("“", "”"), ("‘", "’")),
        ("de-DE", ("„", "“"), ("‚", "‘")),
        ("fr-FR", ("«", "»"), ("‹", "›")),
    ] {
        let style = format!(
            r#"<locale xml:lang="{language}"><style-options punctuation-in-quote="false"/><terms><term name="open-quote">{}</term><term name="close-quote">{}</term><term name="open-inner-quote">{}</term><term name="close-inner-quote">{}</term></terms></locale><citation><layout><text variable="title"/></layout></citation><bibliography><layout><group delimiter=" | "><text variable="title" quotes="true"/><text variable="citation-key"/><text variable="ISBN"/><text variable="URL"/></group></layout></bibliography>"#,
            outer.0, outer.1, inner.0, inner.1
        );
        let data = json!([{"id":"a","type":"book","title":"A \"Quoted\" Book","citation-key":"key's-id","ISBN":"identifier's-code","URL":"https://example.test/path'part"}]);
        let (mut request, output) = process(&style, data, language);
        assert_eq!(rendered(&output), format!("{}A {}Quoted{} Book{} | key's-id | identifier's-code | https://example.test/path'part", outer.0, inner.0, inner.1, outer.1));
        fn has_link(nodes: &[RichNode], target: &str) -> bool {
            nodes.iter().any(|node| match node {
                RichNode::Link { href, children } => href == target || has_link(children, target),
                RichNode::Group { children, .. } => has_link(children, target),
                _ => false,
            })
        }
        let nodes: Vec<RichNode> =
            serde_json::from_value(output["bibliography"]["items"][0]["rich"].clone()).unwrap();
        assert!(has_link(&nodes, "https://example.test/path'part"));
        request.clusters[0][0].prefix = Some("User's \"literal\" ".into());
        request.clusters[0][0].suffix = Some(" 'suffix'".into());
        let citation = crate::academic::citations(&request).unwrap();
        assert!(citation["citations"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("User's \"literal\" "));
        assert!(citation["citations"][0]["text"]
            .as_str()
            .unwrap()
            .ends_with(" 'suffix'"));
    }
}

#[test]
fn academic_display_mixed_identifier_fields_preserve_uri_apostrophes_and_source_data() {
    let style = r#"<citation><layout><text variable="title"/></layout></citation><bibliography><layout><group delimiter=" | "><text variable="title"/><text variable="license"/><text variable="references"/><text variable="source"/></group></layout></bibliography>"#;
    let data = json!([{
        "id":"a", "type":"book", "title":"Owner's Book",
        "license":"https://licenses.example.test/owner's-license",
        "references":"https://references.example.test/paper's-reference",
        "source":"https://source.example.test/source's-record"
    }]);
    let (request, output) = process(style, data.clone(), "en-US");
    let expected = "Owner’s Book | https://licenses.example.test/owner's-license | https://references.example.test/paper's-reference | https://source.example.test/source's-record";
    assert_eq!(rendered(&output), expected);
    let item = &output["bibliography"]["items"][0];
    let nodes: Vec<RichNode> = serde_json::from_value(item["rich"].clone()).unwrap();
    assert_eq!(plain(&nodes), expected);
    assert_eq!(
        parse_xml(item["html"].as_str().unwrap()).unwrap().text(),
        expected
    );
    assert_eq!(serde_json::from_str::<Value>(&request.data).unwrap(), data);
}

#[test]
fn academic_display_month_strip_periods_excludes_own_affixes_and_restores_parent_scope() {
    let locale = r#"<locale xml:lang="en-US"><terms><term name="month-08" form="short">Abg.</term></terms></locale>"#;
    let citation = "<citation><layout><text variable=\"title\"/></layout></citation>";
    let data = json!([{"id":"a","type":"book","title":"Title","issued":{"date-parts":[[2021,8]]}}]);
    for (suffix, expected) in [(". ", "Abg. 2021 Tail."), ("; ", "Abg; 2021 Tail.")] {
        let style = format!(
            r#"{locale}{citation}<bibliography><layout><date variable="issued"><date-part name="month" form="short" strip-periods="true" suffix="{suffix}"/><date-part name="year"/></date><text value="Tail." prefix=" "/></layout></bibliography>"#
        );
        let (_, output) = process(&style, data.clone(), "en-US");
        assert_eq!(rendered(&output), expected);
    }
    let style = format!(
        r#"{locale}<macro name="date"><date variable="issued"><date-part name="month" form="short" suffix=". "/><date-part name="year"/></date></macro>{citation}<bibliography><layout><text macro="date" strip-periods="true" suffix="."/><text value="Tail." prefix=" "/></layout></bibliography>"#
    );
    let (_, output) = process(&style, data, "en-US");
    assert_eq!(
        rendered(&output),
        "Abg 2021. Tail.",
        "a parent macro scope applies to child affixes, but not its own suffix or the next sibling"
    );
}

#[test]
fn academic_display_date_part_text_case_does_not_recase_its_own_suffix() {
    let style = r#"<locale xml:lang="en-US"><terms><term name="month-08" form="short">Abg.</term></terms></locale><citation><layout><text variable="title"/></layout></citation><bibliography><layout><date variable="issued"><date-part name="month" form="short" strip-periods="true" text-case="uppercase" suffix=". literal "/><date-part name="year"/></date></layout></bibliography>"#;
    let (_, output) = process(
        style,
        json!([{"id":"a","type":"book","title":"Title","issued":{"date-parts":[[2021,8]]}}]),
        "en-US",
    );
    assert_eq!(rendered(&output), "ABG. literal 2021");
}
