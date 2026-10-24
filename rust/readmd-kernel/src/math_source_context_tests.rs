use super::*;

fn configured() -> &'static str {
    "```readmd-math v1\nrow(office.mathPr(office.dispDef(val: \"0\"), office.preSp(val: \"120\"), office.postSp(val: \"240\"), office.interSp(val: \"40\"), office.intraSp(val: \"60\")), office.oMathPara(office.oMath(x)))\n```"
}

#[test]
fn duplicate_occurrences_have_exact_original_crlf_astral_ranges() {
    let source = format!("\u{feff}😀\r\n\r\n$$x$$\r\n\r\n{}\r\n\r\n$$x$$",configured().replace('\n',"\r\n"));
    let a = analyze_source(&source).unwrap();
    assert_eq!(a.occurrences.len(),3); assert!(a.plan.is_active());
    assert_eq!(a.occurrences[0].source,a.occurrences[2].source);
    assert_ne!(a.occurrences[0].from,a.occurrences[2].from);
    for (ordinal,o) in a.occurrences.iter().enumerate() {
        assert_eq!(ordinal,o.ordinal);
        assert_eq!(tx::byte_offset(&source,o.from).unwrap(),o.byte_from);
        assert_eq!(tx::byte_offset(&source,o.to).unwrap(),o.byte_to);
        assert_eq!(tx::byte_offset(&source,o.body_from).unwrap(),o.body_byte_from);
        assert_eq!(tx::byte_offset(&source,o.body_to).unwrap(),o.body_byte_to);
        assert!(!o.render(&a).unwrap().root.xml().is_empty());
    }
    assert_eq!(a.author,md_ast::parse(&source));
}

#[test]
fn code_front_matter_html_metadata_and_escaped_examples_cannot_own_formula_ranges() {
    let source="---\nexample: '$fake$'\n---\n\n```md\n$fake$\n```\n\n`$fake$` \\$fake\\$\n\n<!-- $fake$ -->\n\n<script>$fake$</script>\n\n$x$";
    let a=analyze_source(source).unwrap();assert_eq!(a.occurrences.len(),1);
    let o=&a.occurrences[0];assert_eq!(&source[o.byte_from..o.byte_to],"$x$");assert!(!o.display);
}

#[test]
fn display_delimiter_inside_prose_uses_the_real_inline_ast_mode() {
    let source="before $$x$$ after\n\n$$ y $$";let a=analyze_source(source).unwrap();
    assert_eq!(a.occurrences.len(),2);assert!(!a.occurrences[0].display);assert!(a.occurrences[1].display);
    assert_eq!(a.occurrences[1].source,"y");
}

#[test]
fn real_native_metadata_is_bound_to_its_exact_occurrence_without_changing_source() {
    let xml=math_document::parse("x","latex",true).unwrap().root.xml();
    let metadata=math_document::metadata("x",&xml,"native-one").unwrap();
    let source=format!("$$x$${metadata}\n\n$$x$$");let a=analyze_source(&source).unwrap();
    assert_eq!(a.occurrences.len(),2);assert_eq!(a.occurrences[0].metadata.as_deref(),Some(metadata.as_str()));
    assert_eq!(a.occurrences[0].document.dialect,"omml");assert_eq!(a.occurrences[1].document.dialect,"latex");
    assert!(!a.plan.is_ordinary(0));assert!(a.plan.is_ordinary(1));
    assert_eq!(a.occurrences[0].source,"x");assert_eq!(a.author,md_ast::parse(&source));
}

#[test]
fn stale_carrier_is_rejected_instead_of_becoming_inheriting_ordinary_latex() {
    let xml=math_document::parse("x","latex",true).unwrap().root.xml();
    let metadata=math_document::metadata("x",&xml,"native-one").unwrap();
    assert!(analyze_source(&format!("$$y$${metadata}")).is_err());
}

#[test]
fn inline_nested_list_quote_table_and_footnote_order_matches_author_ast() {
    let source="- $a$\n\n> $b$\n\n| header |\n| --- |\n| $c$ |\n\n[^note]: $d$\n\n$e$";
    let a=analyze_source(source).unwrap();assert_eq!(a.occurrences.iter().map(|o|o.source.as_str()).collect::<Vec<_>>(),["a","b","c","d","e"]);
    assert!(a.occurrences.iter().all(|o|!o.display));
}

#[test]
fn unsupported_dsl_and_ambiguous_unmapped_image_math_do_not_succeed() {
    assert!(analyze_source("```readmd-math v9\nx\n```").is_err());
    assert!(analyze_source("![alt $x$](image.png)").is_err());
}

#[test]
fn nested_fences_use_parser_decoded_body_and_exact_outer_span_but_are_read_only() {
    let source="$$y$$\n\n> ```readmd-math v1\n> row(office.mathPr(office.dispDef(val: \"0\")),\n> office.oMathPara(office.oMath(x)))\n> ```\n\n- ```readmd-math v1\n  row(office.mathPr(office.dispDef(val: \"0\")), office.oMathPara(office.oMath(z)))\n  ```";
    let a=analyze_source(source).unwrap();assert_eq!(a.occurrences.len(),3);assert!(a.plan.is_active());
    for o in &a.occurrences[1..] {
        assert!(!o.patchable);assert_eq!(o.dialect,"dsl");assert!(source[o.byte_from..o.byte_to].contains("```readmd-math v1"));
        assert!(math_document::parse(&o.source,"dsl",true).is_ok());o.render(&a).unwrap();
        assert_eq!(tx::byte_offset(source,o.from).unwrap(),o.byte_from);assert_eq!(tx::byte_offset(source,o.to).unwrap(),o.byte_to);
    }
}

#[test]
fn opaque_v2_carrier_is_an_explicit_context_limit_and_is_never_stripped() {
    let xml="<m:oMath><m:r><m:t>a</m:t><z:custom xmlns:z=\"urn:z\"/></m:r></m:oMath>";
    let p=crate::math_extension::capture(xml,"context-opaque",false).unwrap();let metadata=p.metadata.unwrap();
    let source=format!(":math[{}]{}",p.source,metadata);
    assert_eq!(analyze_source(&source).unwrap_err(),"math_unsupported_extension");
    assert!(crate::math_extension::resolve(&p.source,&metadata,Some("context-opaque"),false).is_ok());
}
