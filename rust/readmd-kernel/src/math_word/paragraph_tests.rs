//! Pure resolver regression tests; source identity and presence are observable.
use super::*;
use WordParagraphDeclaration as D;
fn root(styles:&str,direct:&str)->XmlNode {
    crate::math_document::parse_xml(&format!(r#"<w:styles xmlns:w="{WORD_NS}">{styles}</w:styles>{direct}<m:oMathPara xmlns:m="{MATH_NS}"><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath></m:oMathPara>"#)).unwrap()
}
fn ppr(body:&str)->String {format!(r#"<w:pPr xmlns:w="{WORD_NS}">{body}</w:pPr>"#)}
fn finite(styles:&str,direct:&str)->WordFiniteParagraph {word_paragraph_context(&root(styles,direct)).unwrap().finite_display(false).unwrap()}
fn style(id:&str,based:&str,properties:&str,default:bool)->String {
    format!(r#"<w:style w:type="paragraph" w:styleId="{id}" w:default="{}">{based}<w:pPr>{properties}</w:pPr></w:style>"#,if default{"1"}else{"0"})
}
#[test]
fn direct_ppr_without_any_styles_resolves_both_axes_and_explicit_zero() {
    let source=format!(r#"{}<m:oMathPara xmlns:m="{MATH_NS}"><m:oMath/></m:oMathPara>"#,ppr(r#"<w:ind w:left="720" w:right="0"/><w:jc w:val="right"/>"#));
    let parsed=crate::math_document::parse_xml(&source).unwrap();let before=parsed.xml();
    let context=word_paragraph_context(&parsed).unwrap();let actual=context.finite_display(false).unwrap();
    assert_eq!(actual.left.as_ref().unwrap().value.points(),36.);assert_eq!(actual.right.as_ref().unwrap().value.points(),0.);
    assert_eq!(actual.alignment.unwrap().value,WordParagraphJustification::Right);
    assert_eq!(context.layers.last().unwrap().origin,WordParagraphOrigin::Direct);assert_eq!(parsed.xml(),before);
}
#[test]
fn missing_empty_ppr_empty_ind_and_explicit_zero_remain_distinct() {
    let missing=word_paragraph_context(&root("","")).unwrap();assert_eq!(missing.layers.last().unwrap().declaration,D::Missing);
    let empty=word_paragraph_context(&root("",&ppr(""))).unwrap();assert_eq!(empty.layers.last().unwrap().declaration,D::Empty);
    let ind=word_paragraph_context(&root("",&ppr("<w:ind/>"))).unwrap();
    let D::Explicit(properties)=&ind.layers.last().unwrap().declaration else{panic!()};assert_eq!(properties.indent,D::Empty);
    assert!(ind.finite_display(false).unwrap().left.is_none());
    let zero=finite("",&ppr(r#"<w:ind w:left="0"/>"#));assert_eq!(zero.left.unwrap().value.points(),0.);assert!(zero.right.is_none());
}
#[test]
fn defaults_based_on_and_direct_merge_each_ind_axis_independently() {
    let defaults=r#"<w:docDefaults><w:pPrDefault><w:pPr><w:ind w:left="240" w:right="120"/><w:jc w:val="left"/></w:pPr></w:pPrDefault></w:docDefaults>"#;
    let styles=format!("{defaults}{}{}",style("Base","",r#"<w:ind w:left="480"/>"#,false),style("Equation",r#"<w:basedOn w:val="Base"/>"#,r#"<w:ind w:right="360"/><w:jc w:val="right"/>"#,false));
    let actual=finite(&styles,&ppr(r#"<w:pStyle w:val="Equation"/><w:ind w:left="0"/>"#));
    assert_eq!(actual.left.unwrap().value.points(),0.);assert_eq!(actual.right.as_ref().unwrap().value.points(),18.);
    assert_eq!(actual.right.unwrap().origin,WordParagraphOrigin::Style("Equation".into()));
    let cleared=finite(&styles,&ppr(r#"<w:pStyle w:val="Equation"/><w:ind/>"#));
    assert_eq!(cleared.left.unwrap().value.points(),24.);assert_eq!(cleared.right.unwrap().value.points(),18.);
}
#[test]
fn exact_units_preserve_each_author_origin_and_the_other_sparse_axis() {
    for (raw,points) in [("1mm",360./127.),("0.1cm",360./127.),("0.5in",36.),("18.125pt",18.125),("1.5pc",18.),("1.5pi",18.)] {
        let defaults=|left:&str|format!(r#"<w:docDefaults><w:pPrDefault><w:pPr><w:ind w:left="{left}" w:right="18.125pt"/></w:pPr></w:pPrDefault></w:docDefaults>"#);
        let cases=[
            (defaults(raw),ppr(""),WordParagraphOrigin::DocDefaults),
            (format!("{}{}{}",defaults("1in"),style("Base","",&format!(r#"<w:ind w:left="{raw}"/>"#),false),style("Equation",r#"<w:basedOn w:val="Base"/>"#,"",false)),ppr(r#"<w:pStyle w:val="Equation"/>"#),WordParagraphOrigin::Style("Base".into())),
            (format!("{}{}",defaults("1in"),style("P","",r#"<w:ind w:left="2pc"/>"#,false)),ppr(&format!(r#"<w:pStyle w:val="P"/><w:ind w:left="{raw}"/>"#)),WordParagraphOrigin::Direct),
        ];
        for (styles,direct,origin) in cases {
            let source=root(&styles,&direct);let before=source.xml();
            let context=word_paragraph_context(&source).unwrap();let context_before=context.clone();
            let authored=&context.effective.indent["left"];assert_eq!(authored.value.raw(),raw);assert_eq!(authored.origin,origin);
            let output=context.finite_display(false).unwrap();let left=output.left.unwrap();
            assert_eq!(left.origin,origin);assert!((left.value.points()-points).abs()<1e-12,"{raw}");
            assert_eq!(output.right.unwrap(),WordParagraphValue{value:RationalPoints::unit("18.125pt").unwrap(),origin:WordParagraphOrigin::DocDefaults});
            assert_eq!(context.effective.indent["right"].value.raw(),"18.125pt");
            assert_eq!(context,context_before);assert_eq!(source.xml(),before);
        }
    }
}
#[test]
fn legal_unit_precision_limits_are_explicit_without_expanding_axes_or_negative_domain() {
    for raw in [format!("0.{}1mm","0".repeat(64)),"9".repeat(80)] {
        let source=root("",&ppr(&format!(r#"<w:ind w:left="{raw}"/>"#)));let before=source.xml();
        let context=word_paragraph_context(&source).unwrap();let issues=context.finite_display(false).unwrap_err();
        assert_eq!(issues.len(),1);assert_eq!(issues[0].reason,"unsupported-measure-precision");
        assert_eq!(issues[0].field,"word.pPr.ind.left");assert_eq!(issues[0].value,raw);assert_eq!(issues[0].origin,WordParagraphOrigin::Direct);
        assert_eq!(context.effective.indent["left"].value.raw(),raw);assert_eq!(source.xml(),before);
    }
    for (axis,raw) in [("left","-0.0001mm"),("right","1584.000000000000000000000000000001pt"),("start","1mm"),("end","1cm"),("firstLine","0.1cm"),("hanging","1.5pc"),("leftChars","0")] {
        let source=root("",&ppr(&format!(r#"<w:ind w:{axis}="{raw}"/>"#)));let before=source.xml();
        let context=word_paragraph_context(&source).unwrap();let issues=context.finite_display(true).unwrap_err();
        assert_eq!(issues.len(),1);assert_eq!(issues[0].field,format!("word.pPr.ind.{axis}"));
        assert_eq!(issues[0].reason,"unsupported-indent-domain-or-axis");assert_eq!(issues[0].value,raw);assert_eq!(source.xml(),before);
    }
    let raw=format!("-000.{}mm","0".repeat(80));let zero=finite("",&ppr(&format!(r#"<w:ind w:right="{raw}"/>"#)));
    assert!(zero.left.is_none());assert_eq!(zero.right.unwrap().value.points(),0.);
    let a=finite("",&ppr(r#"<w:ind w:left="1mm"/>"#));let b=finite("",&ppr(r#"<w:ind w:left="0.1cm"/>"#));
    assert_eq!(a,b);assert!((a.inner_width(100.).unwrap()-(100.-360./127.)).abs()<1e-12);
    assert_eq!(a.inner_width(360./127.),Err("math_unsupported_extension"));
}
#[test]
fn explicit_unrelated_style_does_not_absorb_default_paragraph_style() {
    let styles=format!("{}{}",style("Default","",r#"<w:ind w:left="999"/><w:jc w:val="left"/>"#,true),style("Other","",r#"<w:ind w:right="240"/><w:jc w:val="right"/>"#,false));
    assert_eq!(finite(&styles,"").left.unwrap().value.points(),49.95);
    let other=finite(&styles,&ppr(r#"<w:pStyle w:val="Other"/>"#));assert!(other.left.is_none());assert_eq!(other.right.unwrap().value.points(),12.);
}
#[test]
fn paragraph_guards_are_absolute_and_direct_false_clears_inherited_true() {
    let inherited=r#"<w:bidi/><w:adjustRightInd/><w:snapToGrid/><w:mirrorIndents/>"#;
    let styles=style("P","",inherited,true);
    assert_eq!(word_paragraph_context(&root(&styles,"")).unwrap().finite_display(false).unwrap_err().len(),4);
    let direct=ppr(r#"<w:bidi w:val="0"/><w:adjustRightInd w:val="false"/><w:snapToGrid w:val="off"/><w:mirrorIndents w:val="0"/>"#);
    let context=word_paragraph_context(&root(&styles,&direct)).unwrap();assert!(context.finite_display(false).is_ok());
    for value in context.effective.guards.values(){assert!(!value.value);assert_eq!(value.origin,WordParagraphOrigin::Direct);}
    let D::Explicit(properties)=&context.layers[1].declaration else{panic!()};assert_eq!(properties.guards["bidi"],D::Empty);
}
#[test]
fn local_math_alignment_shadows_only_word_jc_and_not_ind_or_guard() {
    let context=word_paragraph_context(&root("",&ppr(r#"<w:ind w:left="720"/><w:jc w:val="both"/>"#))).unwrap();
    assert!(context.finite_display(false).is_err());let finite=context.finite_display(true).unwrap();assert!(finite.alignment.is_none());assert_eq!(finite.left.unwrap().value.points(),36.);
    let blocked=word_paragraph_context(&root("",&ppr(r#"<w:bidi/><w:jc w:val="both"/>"#))).unwrap();
    assert_eq!(blocked.finite_display(true).unwrap_err()[0].field,"word.pPr.bidi");
}
#[test]
fn all_fixed_jc_values_parse_and_only_five_enter_horizontal_ltr_finite_projection() {
    for value in ["left","start","center","right","end","both","mediumKashida","distribute","numTab","highKashida","lowKashida","thaiDistribute"] {
        let context=word_paragraph_context(&root("",&ppr(&format!(r#"<w:jc w:val="{value}"/>"#)))).unwrap();
        let before=context.clone();let projected=context.finite_display(false);
        assert_eq!(projected.is_ok(),matches!(value,"left"|"start"|"center"|"right"|"end"),"{value}");
        assert_eq!(context.effective.alignment.as_ref().unwrap().value.as_str(),value);
        if let Ok(projected)=projected {
            let alignment=projected.alignment.unwrap();
            assert_eq!(alignment.value.as_str(),match value{"start"=>"left","end"=>"right",other=>other});
            assert_eq!(alignment.origin,WordParagraphOrigin::Direct);
        }
        assert_eq!(context,before);
    }
}
#[test]
fn logical_jc_projects_only_the_output_copy_at_each_actual_inheritance_origin() {
    for (native,opposite,physical) in [("start","end",WordParagraphJustification::Left),("end","start",WordParagraphJustification::Right)] {
        let defaults=|jc|format!(r#"<w:docDefaults><w:pPrDefault><w:pPr><w:ind w:left="240" w:right="120"/><w:jc w:val="{jc}"/></w:pPr></w:pPrDefault></w:docDefaults>"#);
        let selected=|jc|format!("{}{}{}",defaults(opposite),
            style("Base","",&format!(r#"<w:jc w:val="{jc}"/>"#),false),
            style("Equation",r#"<w:basedOn w:val="Base"/>"#,"",false));
        let cases=[
            (format!("{}{}",defaults(native),style("Unused","",&format!(r#"<w:jc w:val="{opposite}"/>"#),false)),ppr(""),WordParagraphOrigin::DocDefaults),
            (selected(native),ppr(r#"<w:pStyle w:val="Equation"/>"#),WordParagraphOrigin::Style("Base".into())),
            (selected(opposite),ppr(&format!(r#"<w:pStyle w:val="Equation"/><w:jc w:val="{native}"/>"#)),WordParagraphOrigin::Direct),
        ];
        for (styles,direct,origin) in cases {
            let source=root(&styles,&direct);let xml=source.xml();
            let context=word_paragraph_context(&source).unwrap();let before=context.clone();
            let authored=context.effective.alignment.as_ref().unwrap();
            assert_eq!(authored.value.as_str(),native);assert_eq!(authored.origin,origin);
            let projected=context.finite_display(false).unwrap();let alignment=projected.alignment.unwrap();
            assert_eq!(alignment.value,physical);assert_eq!(alignment.origin,origin);
            assert_eq!(projected.left.unwrap(),WordParagraphValue{value:RationalPoints::twips("240").unwrap(),origin:WordParagraphOrigin::DocDefaults});
            assert_eq!(projected.right.unwrap(),WordParagraphValue{value:RationalPoints::twips("120").unwrap(),origin:WordParagraphOrigin::DocDefaults});
            assert_eq!(context,before);assert_eq!(source.xml(),xml);
        }
    }
}
#[test]
fn logical_jc_needs_proven_lrtb_but_preserves_physical_tb_and_shadowing_behavior() {
    for native in ["start","end"] {
        for direction in ["lrTb","tb","tbRl","rl","btLr","lr","lrTbV","tbV","tbRlV","rlV","tbLrV","lrV"] {
            let source=root("",&ppr(&format!(r#"<w:jc w:val="{native}"/><w:textDirection w:val="{direction}"/>"#)));
            let xml=source.xml();let context=word_paragraph_context(&source).unwrap();
            if direction=="lrTb" {assert!(context.finite_display(false).is_ok());}
            else {let issues=context.finite_display(false).unwrap_err();assert_eq!(issues.len(),1);
                assert_eq!(issues[0].field,"word.pPr.textDirection");assert_eq!(issues[0].origin,WordParagraphOrigin::Direct);assert_eq!(issues[0].value,direction);}
            if direction=="tb" {assert!(context.finite_display(true).unwrap().alignment.is_none());}
            assert_eq!(source.xml(),xml);assert_eq!(context.effective.alignment.as_ref().unwrap().value.as_str(),native);
        }
        for inherited in ["tb","tbRl"] {
        let styles=style("P","",&format!(r#"<w:textDirection w:val="{inherited}"/>"#),true);
        let context=word_paragraph_context(&root(&styles,&ppr(&format!(r#"<w:jc w:val="{native}"/>"#)))).unwrap();
        let issues=context.finite_display(false).unwrap_err();assert_eq!(issues[0].origin,WordParagraphOrigin::Style("P".into()));
        assert!(word_paragraph_context(&root(&styles,&ppr(&format!(r#"<w:jc w:val="{native}"/><w:textDirection w:val="lrTb"/>"#)))).unwrap().finite_display(false).is_ok());
        }
    }
    for physical in ["left","center","right"] {
        assert!(finite("",&ppr(&format!(r#"<w:jc w:val="{physical}"/><w:textDirection w:val="tb"/>"#))).alignment.is_some());
    }
}
#[test]
fn logical_jc_uses_effective_bidi_and_direct_false_without_rewriting_native_origin() {
    for native in ["start","end"] {
        let styles=style("P","",&format!(r#"<w:bidi/><w:jc w:val="{native}"/><w:textDirection w:val="lrTb"/>"#),true);
        let blocked=word_paragraph_context(&root(&styles,"")).unwrap();
        let issues=blocked.finite_display(false).unwrap_err();assert_eq!(issues.len(),1);assert_eq!(issues[0].field,"word.pPr.bidi");assert_eq!(issues[0].origin,WordParagraphOrigin::Style("P".into()));
        let source=root(&styles,&ppr(r#"<w:bidi w:val="false"/>"#));let xml=source.xml();
        let context=word_paragraph_context(&source).unwrap();let before=context.clone();
        let alignment=context.finite_display(false).unwrap().alignment.unwrap();
        assert_eq!(alignment.value.as_str(),if native=="start"{"left"}else{"right"});
        assert_eq!(alignment.origin,WordParagraphOrigin::Style("P".into()));
        assert_eq!(context.effective.guards["bidi"],WordParagraphValue{value:false,origin:WordParagraphOrigin::Direct});
        assert_eq!(context.effective.alignment.as_ref().unwrap().value.as_str(),native);assert_eq!(context,before);assert_eq!(source.xml(),xml);
        assert_eq!(blocked.finite_display(true).unwrap_err()[0].field,"word.pPr.bidi");
    }
}
#[test]
fn logical_jc_does_not_expand_indent_guard_or_unresolved_host_domains() {
    for native in ["start","end"] {
        for axis in ["start","end","leftChars","rightChars","startChars","endChars","firstLine","hanging"] {
            let context=word_paragraph_context(&root("",&ppr(&format!(r#"<w:ind w:{axis}="0"/><w:jc w:val="{native}"/>"#)))).unwrap();
            let issues=context.finite_display(false).unwrap_err();assert_eq!(issues.len(),1);assert_eq!(issues[0].field,format!("word.pPr.ind.{axis}"));
        }
        for (property,field) in [("<w:bidi/>","bidi"),("<w:mirrorIndents/>","mirrorIndents"),("<w:adjustRightInd/>","adjustRightInd"),("<w:snapToGrid/>","snapToGrid"),("<w:tabs/>","tabs"),("<w:spacing/>","spacing"),("<w:framePr/>","framePr")] {
            let context=word_paragraph_context(&root("",&ppr(&format!(r#"{property}<w:jc w:val="{native}"/>"#)))).unwrap();
            let issues=context.finite_display(false).unwrap_err();assert_eq!(issues.len(),1);assert_eq!(issues[0].field,format!("word.pPr.{field}"));
            assert_eq!(context.finite_display(true).unwrap_err()[0].field,format!("word.pPr.{field}"));
        }
    }
}
#[test]
fn numeric_lexemes_are_preserved_and_zero_is_not_missing() {
    for raw in ["+000","-0","0"," 0 ","31680"] {
        let context=word_paragraph_context(&root("",&ppr(&format!(r#"<w:ind w:left="{raw}"/>"#)))).unwrap();
        assert_eq!(context.effective.indent["left"].value.raw(),raw);
        assert!(context.finite_display(false).is_ok());
    }
}
#[test]
fn valid_wider_native_ind_domains_are_preserved_then_accurately_unsupported() {
    for (attribute,raw) in [("left","-1"),("right","31681"),("left","999999999999999999999999999999"),("left","55.8800000000000000001cm"),("right","-2pt"),("start","0"),("leftChars","0"),("firstLine","0"),("hanging","0")] {
        let source=root("",&ppr(&format!(r#"<w:ind w:{attribute}="{raw}"/>"#)));let before=source.xml();
        let context=word_paragraph_context(&source).unwrap();assert_eq!(context.effective.indent[attribute].value.raw(),raw);
        let error=context.finite_display(false).unwrap_err();assert_eq!(error[0].field,format!("word.pPr.ind.{attribute}"));assert_eq!(source.xml(),before);
    }
}
#[test]
fn missing_required_jc_and_pstyle_values_and_invalid_measures_are_errors() {
    for body in ["<w:jc/>",r#"<w:jc w:val=""/>"#,r#"<w:jc w:val="centerGroup"/>"#,"<w:pStyle/>",r#"<w:ind w:left="1.5"/>"#,r#"<w:ind w:left="1em"/>"#,r#"<w:ind w:left="+1.5pt"/>"#,r#"<w:ind w:firstLine="-1"/>"#,r#"<w:ind w:leftChars="2147483648"/>"#,r#"<w:bidi w:val="maybe"/>"#] {
        assert_eq!(word_paragraph_context(&root("",&ppr(body))).unwrap_err(),"math_invalid_property","{body}");
    }
}
#[test]
fn alias_uri_is_accepted_but_unqualified_or_spoofed_value_has_no_power() {
    let xml=format!(r#"<v:pPr xmlns:v="{WORD_NS}"><v:ind v:left="720"/><v:jc v:val="right"/></v:pPr><m:oMath xmlns:m="{MATH_NS}"/>"#);
    let parsed=crate::math_document::parse_xml(&xml).unwrap();assert_eq!(word_paragraph_context(&parsed).unwrap().finite_display(false).unwrap().left.unwrap().value.points(),36.);
    for body in [r#"<w:jc val="right"/>"#,r#"<w:jc xmlns:x="urn:fake" x:val="right"/>"#,r#"<w:ind xmlns:x="urn:fake" x:left="720"/>"#] {
        assert_eq!(word_paragraph_context(&root("",&ppr(body))).unwrap_err(),"math_invalid_property");
    }
}
#[test]
fn semantic_duplicate_alias_attributes_are_rejected() {
    let source=format!(r#"<w:pPr xmlns:w="{WORD_NS}" xmlns:v="{WORD_NS}"><w:ind w:left="1" v:left="2"/></w:pPr><m:oMath xmlns:m="{MATH_NS}"/>"#);
    let root=crate::math_document::parse_xml(&source).unwrap();assert_eq!(word_paragraph_context(&root).unwrap_err(),"math_invalid_property");
}
#[test]
fn selected_particle_order_cardinality_and_parent_types_are_checked() {
    for body in [r#"<w:jc w:val="left"/><w:ind w:left="0"/>"#,r#"<w:ind/><w:ind/>"#,r#"<w:jc w:val="left"/><w:jc w:val="right"/>"#] {
        assert_eq!(word_paragraph_context(&root("",&ppr(body))).unwrap_err(),"math_invalid_property");
    }
    let styles=style("P","",r#"<w:pStyle w:val="P"/>"#,true);assert!(word_paragraph_context(&root(&styles,"")).is_err());
    let duplicate=format!("{}{}<m:oMath xmlns:m=\"{MATH_NS}\"/>",ppr(""),ppr(""));
    assert!(word_paragraph_context(&crate::math_document::parse_xml(&duplicate).unwrap()).is_err());
}
#[test]
fn defaults_particle_order_and_pprdefault_child_cardinality_are_checked() {
    for styles in [r#"<w:docDefaults><w:pPrDefault/><w:rPrDefault/></w:docDefaults>"#,r#"<w:docDefaults><w:pPrDefault/><w:pPrDefault/></w:docDefaults>"#,r#"<w:docDefaults><w:pPrDefault><w:pPr/><w:pPr/></w:pPrDefault></w:docDefaults>"#] {
        assert!(word_paragraph_context(&root(styles,"")).is_err());
    }
}
#[test]
fn missing_cyclic_wrong_type_and_malformed_based_on_stay_errors() {
    let direct=ppr(r#"<w:pStyle w:val="P"/>"#);
    for styles in [String::new(),style("P",r#"<w:basedOn w:val="P"/>"#,"",false),style("P","<w:basedOn/>","",false),style("P",r#"<w:basedOn w:val="Q"/><w:basedOn w:val="Q"/>"#,"",false),r#"<w:style w:type="character" w:styleId="P"/>"#.into()] {
        assert_eq!(word_paragraph_context(&root(&styles,&direct)).unwrap_err(),"math_invalid_property");
    }
}
#[test]
fn reused_chain_limit_and_unreferenced_style_isolation_are_unchanged() {
    let mut styles=String::new();for index in 0..129 {let based=if index==0{String::new()}else{format!(r#"<w:basedOn w:val="P{}"/>"#,index-1)};styles.push_str(&style(&format!("P{index}"),&based,"",false));}
    assert!(word_paragraph_context(&root(&styles,&ppr(r#"<w:pStyle w:val="P128"/>"#))).is_err());
    assert!(word_paragraph_context(&root(&styles,&ppr(r#"<w:pStyle w:val="P127"/>"#))).is_ok());
    let isolated=format!("{}{}",style("P","",r#"<w:jc w:val="left"/>"#,false),style("Unused",r#"<w:basedOn w:val="Unused"/>"#,"",false));
    assert!(word_paragraph_context(&root(&isolated,&ppr(r#"<w:pStyle w:val="P"/>"#))).unwrap().finite_display(false).is_ok());
}
#[test]
fn text_direction_is_typed_and_effective_direct_lrtb_clears_inherited_vertical() {
    let styles=style("P","",r#"<w:textDirection w:val="tbRl"/>"#,true);
    assert!(word_paragraph_context(&root(&styles,"")).unwrap().finite_display(false).is_err());
    assert!(word_paragraph_context(&root(&styles,&ppr(r#"<w:textDirection w:val="lrTb"/>"#))).unwrap().finite_display(false).is_ok());
    assert!(word_paragraph_context(&root("",&ppr(r#"<w:textDirection w:val="unknown"/>"#))).is_err());
}
#[test]
fn horizontal_text_direction_aliases_are_supported_without_rewriting_source() {
    for value in ["lrTb","tb"] {
        let source=root("",&ppr(&format!(r#"<w:textDirection w:val="{value}"/>"#)));let before=source.xml();
        let context=word_paragraph_context(&source).unwrap();
        assert!(context.finite_display(false).is_ok(),"{value}");
        assert_eq!(context.effective.text_direction.as_ref().unwrap().value,value);
        assert_eq!(source.xml(),before);
    }
    for value in ["tbRl","rl","btLr","lr","lrTbV","tbV","tbRlV","rlV","tbLrV","lrV"] {
        let context=word_paragraph_context(&root("",&ppr(&format!(r#"<w:textDirection w:val="{value}"/>"#)))).unwrap();
        let issues=context.finite_display(false).unwrap_err();assert_eq!(issues.len(),1);
        assert_eq!(issues[0].field,"word.pPr.textDirection");assert_eq!(issues[0].value,value);
    }
}
#[test]
fn direct_horizontal_alias_overrides_inherited_vertical_and_reverse_still_blocks() {
    let vertical=style("P","",r#"<w:textDirection w:val="tbRl"/>"#,true);
    assert!(word_paragraph_context(&root(&vertical,"")).unwrap().finite_display(false).is_err());
    for value in ["lrTb","tb"] {
        let source=root(&vertical,&ppr(&format!(r#"<w:textDirection w:val="{value}"/>"#)));let before=source.xml();
        let context=word_paragraph_context(&source).unwrap();assert!(context.finite_display(false).is_ok());
        let direction=context.effective.text_direction.as_ref().unwrap();
        assert_eq!(direction.value,value);assert_eq!(direction.origin,WordParagraphOrigin::Direct);assert_eq!(source.xml(),before);
        let inherited=style("P","",&format!(r#"<w:textDirection w:val="{value}"/>"#),true);
        assert!(word_paragraph_context(&root(&inherited,"")).unwrap().finite_display(false).is_ok());
        let blocked=word_paragraph_context(&root(&inherited,&ppr(r#"<w:textDirection w:val="tbRl"/>"#))).unwrap().finite_display(false).unwrap_err();
        assert_eq!(blocked.len(),1);assert_eq!(blocked[0].origin,WordParagraphOrigin::Direct);assert_eq!(blocked[0].value,"tbRl");
    }
}
#[test]
fn unresolved_paragraph_layout_is_preserved_and_never_claimed_consumed() {
    let source=root("",&ppr(r#"<w:spacing w:after="160"/><w:ind w:left="0"/>"#));let before=source.xml();
    let context=word_paragraph_context(&source).unwrap();assert_eq!(source.xml(),before);
    let error=context.finite_display(false).unwrap_err();assert_eq!(error[0].field,"word.pPr.spacing");assert_eq!(error[0].reason,"unimplemented-property-cascade");
}
#[test]
fn higher_legal_same_axis_values_shadow_valid_unsupported_lower_values() {
    let styles=style("P","",r#"<w:ind w:left="-1cm" w:right="120"/><w:jc w:val="both"/>"#,true);
    let direct=ppr(r#"<w:ind w:left="720"/><w:jc w:val="right"/>"#);
    let context=word_paragraph_context(&root(&styles,&direct)).unwrap();let actual=context.finite_display(false).unwrap();
    assert_eq!(actual.left.unwrap().value.points(),36.);assert_eq!(actual.right.unwrap().value.points(),6.);assert_eq!(actual.alignment.unwrap().value,WordParagraphJustification::Right);
    assert_eq!(context.layers[1].origin,WordParagraphOrigin::Style("P".into()));
}
#[test]
fn paragraph_mark_run_properties_do_not_set_paragraph_or_text_axes() {
    let actual=finite("",&ppr(r#"<w:rPr><w:sz w:val="400"/><w:rtl/></w:rPr>"#));assert!(actual.left.is_none()&&actual.right.is_none()&&actual.alignment.is_none());
}
#[test]
fn width_guard_is_pure_and_never_clamps_or_fabricates_a_missing_axis() {
    let actual=finite("",&ppr(r#"<w:ind w:left="720" w:right="360"/>"#));assert_eq!(actual.inner_width(100.).unwrap(),46.);
    for width in [0.,54.,-1.,f64::INFINITY,f64::NAN]{assert_eq!(actual.inner_width(width).unwrap_err(),"math_unsupported_extension");}
    let sparse=finite("",&ppr(r#"<w:ind w:right="0"/>"#));assert!(sparse.left.is_none());assert_eq!(sparse.right.unwrap().value.points(),0.);
}
