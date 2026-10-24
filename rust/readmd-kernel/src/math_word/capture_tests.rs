//! Independent direct-pPr capture-presence regressions.
use super::*;
fn node(source:&str)->XmlNode {crate::math_document::parse_xml(source).unwrap().children().next().unwrap().clone()}
fn equation()->XmlNode {node(&format!(r#"<m:oMath xmlns:m="{MATH_NS}"><m:r><m:t>x</m:t></m:r></m:oMath>"#))}
#[test]
fn direct_ppr_without_styles_is_captured_without_inventing_styles() {
    let direct=node(&format!(r#"<w:pPr xmlns:w="{WORD_NS}"><w:ind w:left="0"/><w:jc w:val="right"/></w:pPr>"#));let before=direct.xml();
    let actual=WordMathStyles::default().capture(Some(&direct),&equation()).unwrap();
    assert_eq!(actual.len(),1);assert!(word(&actual[0],"pPr"));assert_eq!(actual[0].xml(),before);assert_eq!(direct.xml(),before);
}
#[test]
fn empty_direct_ppr_and_ind_presence_survive_empty_catalogue() {
    let styles=WordMathStyles::parse(&format!(r#"<w:styles xmlns:w="{WORD_NS}"/>"#)).unwrap();
    for properties in ["","<w:ind/>"] {
        let direct=node(&format!(r#"<w:pPr xmlns:w="{WORD_NS}">{properties}</w:pPr>"#));
        let actual=styles.capture(Some(&direct),&equation()).unwrap();assert_eq!(actual.len(),1);assert_eq!(actual[0].xml(),direct.xml());
    }
    assert!(styles.capture(None,&equation()).unwrap().is_empty());
}
#[test]
fn direct_pstyle_with_no_catalogue_is_not_silently_dropped() {
    let direct=node(&format!(r#"<w:pPr xmlns:w="{WORD_NS}"><w:pStyle w:val="Missing"/></w:pPr>"#));
    assert_eq!(WordMathStyles::default().capture(Some(&direct),&equation()).unwrap_err(),"math_invalid_property");
}
#[test]
fn capture_keeps_only_the_actual_referenced_chain_plus_defaults_and_direct() {
    let styles=WordMathStyles::parse(&format!(r#"<w:styles xmlns:w="{WORD_NS}"><w:docDefaults><w:pPrDefault><w:pPr><w:ind w:right="120"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:styleId="Base"/><w:style w:type="paragraph" w:styleId="Equation"><w:basedOn w:val="Base"/></w:style><w:style w:type="paragraph" w:styleId="Unused"/></w:styles>"#)).unwrap();
    let direct=node(&format!(r#"<w:pPr xmlns:w="{WORD_NS}"><w:pStyle w:val="Equation"/></w:pPr>"#));
    let actual=styles.capture(Some(&direct),&equation()).unwrap();assert_eq!(actual.len(),2);
    assert!(actual[0].xml().contains("Base")&&actual[0].xml().contains("Equation")&&!actual[0].xml().contains("Unused"));assert_eq!(actual[1].xml(),direct.xml());
}

fn catalogue(body:&str)->Result<WordMathStyles> {
    WordMathStyles::parse(&format!(r#"<w:styles xmlns:w="{WORD_NS}" xmlns:q="{WORD_NS}" xmlns:f="urn:foreign">{body}</w:styles>"#))
}
#[test]
fn duplicate_style_identity_attributes_never_choose_the_first_alias() {
    for attributes in [
        r#"w:type="paragraph" w:styleId="P" q:styleId="Other""#,
        r#"w:type="paragraph" w:styleId="P" q:styleId="P""#,
        r#"w:type="paragraph" q:type="character" w:styleId="P""#,
        r#"w:type="paragraph" w:styleId="P" w:default="0" q:default="1""#,
        r#"xmlns:g="urn:foreign" w:type="paragraph" w:styleId="P" f:styleId="X" g:styleId="Y""#,
    ] {
        assert_eq!(catalogue(&format!("<w:style {attributes}/>")).err(),Some("math_invalid_property"),"{attributes}");
    }
    let source=format!(r#"<w:styles xmlns:w="{WORD_NS}" xmlns:q="{WORD_NS}" w:default="0" q:default="1"><w:style w:type="paragraph" w:styleId="P"/></w:styles>"#);
    assert_eq!(WordMathStyles::parse(&source).err(),Some("math_invalid_property"));
}
#[test]
fn selected_based_on_alias_duplicates_cannot_choose_an_ancestor() {
    let paragraph=node(&format!(r#"<w:pPr xmlns:w="{WORD_NS}"><w:pStyle w:val="P"/></w:pPr>"#));
    for reference in [
        r#"<w:basedOn w:val="Base" q:val="Other"/>"#,
        r#"<w:basedOn w:val="Base" q:val="Base"/>"#,
        r#"<w:basedOn w:val="Base"/><q:basedOn q:val="Other"/>"#,
    ] {
        let styles=catalogue(&format!(r#"<w:style w:type="paragraph" w:styleId="Base"/><w:style w:type="paragraph" w:styleId="Other"/><w:style w:type="paragraph" w:styleId="P">{reference}</w:style>"#)).unwrap();
        assert_eq!(styles.chain("P","paragraph").unwrap_err(),"math_invalid_property","{reference}");
        assert_eq!(styles.capture(Some(&paragraph),&equation()).unwrap_err(),"math_invalid_property");
    }
}
#[test]
fn different_namespace_same_name_attributes_are_distinct_and_have_no_word_authority() {
    let styles=catalogue(r#"<q:style q:type="paragraph" q:styleId="Base"/><q:style q:type="paragraph" f:type="character" q:styleId="P" f:styleId="Foreign" q:default="1" f:default="not-word"><q:basedOn q:val="Base" f:val="Missing"/></q:style>"#).unwrap();
    assert_eq!(styles.paragraph_id(None),Some("P"));
    let chain=styles.chain("P","paragraph").unwrap();
    assert_eq!(chain.iter().map(|style|attr(style,"styleId").unwrap()).collect::<Vec<_>>(),vec!["Base","P"]);
    assert!(styles.chain("Foreign","paragraph").is_err());
    let paragraph=node(&format!(r#"<w:pPr xmlns:w="{WORD_NS}"><w:pStyle w:val="P"/></w:pPr>"#));
    let captured=styles.capture(Some(&paragraph),&equation()).unwrap();
    assert!(captured[0].xml().contains("f:styleId=\"Foreign\""));
    assert!(captured[0].xml().contains("f:val=\"Missing\""));
}
#[test]
fn foreign_identity_fields_cannot_supply_missing_word_identity_type_or_default() {
    assert_eq!(catalogue(r#"<w:style w:type="paragraph" f:styleId="P"/>"#).err(),Some("math_invalid_property"));
    let wrong_type=catalogue(r#"<w:style f:type="paragraph" w:styleId="P"/>"#).unwrap();
    assert_eq!(wrong_type.chain("P","paragraph").unwrap_err(),"math_invalid_property");
    let foreign_default=catalogue(r#"<w:style w:type="paragraph" w:styleId="P" f:default="1"/>"#).unwrap();
    assert_eq!(foreign_default.paragraph_id(None),None);
    assert_eq!(foreign_default.chain("P","paragraph").unwrap().len(),1);
}
#[test]
fn valid_alias_catalogue_does_not_traverse_unused_broken_chains() {
    let styles=catalogue(r#"<q:style q:type="paragraph" q:styleId="Base"/><q:style q:type="paragraph" q:styleId="P" q:default="1"><q:basedOn q:val="Base"/></q:style><q:style q:type="paragraph" q:styleId="UnusedCycle"><q:basedOn q:val="UnusedCycle"/></q:style><q:style q:type="paragraph" q:styleId="UnusedMissing"><q:basedOn q:val="Missing"/></q:style>"#).unwrap();
    let chain=styles.chain(styles.paragraph_id(None).unwrap(),"paragraph").unwrap();
    assert_eq!(chain.iter().map(|style|attr(style,"styleId").unwrap()).collect::<Vec<_>>(),vec!["Base","P"]);
    let captured=styles.capture(None,&equation()).unwrap();
    assert_eq!(captured.len(),1);let xml=captured[0].xml();
    assert!(xml.contains("Base")&&xml.contains("P"));assert!(!xml.contains("UnusedCycle")&&!xml.contains("UnusedMissing"));
    assert_eq!(styles.chain("UnusedCycle","paragraph").unwrap_err(),"math_invalid_property");
    assert_eq!(styles.chain("UnusedMissing","paragraph").unwrap_err(),"math_invalid_property");
}
