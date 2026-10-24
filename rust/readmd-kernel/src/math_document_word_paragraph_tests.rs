//! M07 source transactions are separate from finite display capability.
use super::*;

fn source()->String{
    "// author note 😀\r\nrow(\r\n  word.styles(\r\n    word.docDefaults(word.pPrDefault(word.pPr(word.ind(left: \"240\", right: \"120\"), word.jc(val: \"left\")))),\r\n    word.style(word.pPr(word.ind(left: \"480\")), type: \"paragraph\", styleId: \"Base\"),\r\n    word.style(word.basedOn(val: \"Base\"), word.pPr(word.ind(right: \"360\"), word.jc(val: \"right\")), type: \"paragraph\", styleId: \"Equation\")\r\n  ),\r\n  word.pPr(word.pStyle(val: \"Equation\"), word.ind(left: \"0\", right: \"240\"), word.jc(val: \"left\")),\r\n  office.mathPr(office.dispDef(val: \"0\")),\r\n  office.oMathPara(office.oMath(frac(α, β)))\r\n)".into()
}
fn word_nodes<'a>(root:&'a XmlNode,local:&str)->Vec<&'a XmlNode>{
    fn visit<'a>(node:&'a XmlNode,local:&str,out:&mut Vec<&'a XmlNode>){
        if node.namespace==WORD_NS&&node.local()==local{out.push(node);}
        for child in node.children(){visit(child,local,out);}
    }
    let mut out=Vec::new();visit(root,local,&mut out);out
}
fn node_id(doc:&MathDocument,local:&str)->String{word_nodes(&doc.root,local).last().unwrap().id.clone()}
fn attribute<'a>(node:&'a XmlNode,local:&str)->Option<&'a str>{
    node.attributes.iter().find(|(key,_)|key.rsplit(':').next()==Some(local)
        &&node.attribute_namespaces.get(*key).is_some_and(|uri|uri==WORD_NS)).map(|(_,value)|value.as_str())
}
fn change(doc:&MathDocument,local:&str,name:&str,value:Option<&str>)->Result<MathDocument,&'static str>{
    patch(&doc.source,&doc.dialect,true,&doc.revision,&MathPatch::SetAttribute{
        node_id:node_id(doc,local),name:name.into(),value:value.map(str::to_string)})
}
fn finite(doc:&MathDocument)->crate::math_word::WordFiniteParagraph{
    crate::math_word::word_paragraph_context(&doc.root).unwrap().finite_display(false).unwrap()
}

#[test]
fn word_jc_attribute_is_required_and_all_legal_enums_remain_source_editable(){
    let original=parse(&source(),"dsl",true).unwrap();
    for value in ["left","center","right","start","end","both","mediumKashida","distribute","numTab","highKashida","lowKashida","thaiDistribute"]{
        let changed=change(&original,"jc","w:val",Some(value)).unwrap();
        assert_eq!(attribute(word_nodes(&changed.root,"jc").last().unwrap(),"val"),Some(value));
        assert!(changed.source.starts_with("// author note 😀\r\n"));
        assert!(changed.source.ends_with("office.oMathPara(office.oMath(frac(α, β)))\r\n)"));
        let context=crate::math_word::word_paragraph_context(&changed.root).unwrap();
        assert_eq!(context.finite_display(false).is_ok(),matches!(value,"left"|"start"|"center"|"right"|"end"));
        assert_eq!(context.effective.alignment.as_ref().unwrap().value.as_str(),value);
    }
    for value in [None,Some(""),Some("centerGroup"),Some("LEFT"),Some("bogus")]{
        assert_eq!(change(&original,"jc","w:val",value).unwrap_err(),"math_invalid_property");
    }
    let restored=patch(&original.source,"dsl",true,&original.revision,&MathPatch::ReplaceSource{
        node_id:node_id(&original,"pPr"),text:"word.pPr(word.pStyle(val: \"Equation\"), word.ind(left: \"0\", right: \"240\"))".into()}).unwrap();
    assert_eq!(finite(&restored).alignment.unwrap().value,crate::math_word::WordParagraphJustification::Right);
    assert_eq!(original.source,source());
}

#[test]
fn logical_jc_scalar_edits_keep_exact_source_and_clearing_direct_restores_native_style_origin(){
    let input=source().replacen("word.jc(val: \"right\")","word.jc(val: \"end\")",1);
    let original=parse(&input,"dsl",true).unwrap();
    for native in ["start","end"] {
        let changed=change(&original,"jc","w:val",Some(native)).unwrap();
        let marker="word.jc(val: \"left\")";let at=input.rfind(marker).unwrap();
        let mut expected=input.clone();expected.replace_range(at..at+marker.len(),&format!("word.jc(val: \"{native}\")"));
        assert_eq!(changed.source,expected);assert_eq!(original.source,input);
        let context=crate::math_word::word_paragraph_context(&changed.root).unwrap();
        assert_eq!(context.effective.alignment.as_ref().unwrap().value.as_str(),native);
        assert_eq!(context.effective.alignment.as_ref().unwrap().origin,crate::math_word::WordParagraphOrigin::Direct);
        assert_eq!(finite(&changed).alignment.unwrap().value.as_str(),if native=="start"{"left"}else{"right"});
        assert!(preview_at_size(&changed,true,12.).unwrap().contains(if native=="start"{"text-align:left"}else{"text-align:right"}));
        let cleared=patch(&changed.source,"dsl",true,&changed.revision,&MathPatch::ReplaceSource{
            node_id:node_id(&changed,"pPr"),text:"word.pPr(word.pStyle(val: \"Equation\"), word.ind(left: \"0\", right: \"240\"))".into()}).unwrap();
        let context=crate::math_word::word_paragraph_context(&cleared.root).unwrap();let inherited=context.effective.alignment.as_ref().unwrap();
        assert_eq!(inherited.value.as_str(),"end");assert_eq!(inherited.origin,crate::math_word::WordParagraphOrigin::Style("Equation".into()));
        let finite=finite(&cleared);assert_eq!(finite.alignment.unwrap().value,crate::math_word::WordParagraphJustification::Right);
        assert_eq!(finite.left.unwrap().value.points(),0.);assert_eq!(finite.right.unwrap().value.points(),12.);
        assert!(cleared.source.starts_with("// author note 😀\r\n"));assert!(cleared.source.ends_with("office.oMathPara(office.oMath(frac(α, β)))\r\n)"));
    }
}

#[test]
fn word_indent_clear_restores_independent_style_axes_and_keeps_empty_presence(){
    let original=parse(&source(),"dsl",true).unwrap();let first=finite(&original);
    assert_eq!(first.left.unwrap().value.points(),0.);assert_eq!(first.right.unwrap().value.points(),12.);
    let cleared=change(&original,"ind","w:left",None).unwrap();let inherited=finite(&cleared);
    assert_eq!(inherited.left.unwrap().value.points(),24.);assert_eq!(inherited.right.unwrap().value.points(),12.);
    let reset=change(&cleared,"ind","w:left",Some("0")).unwrap();assert_eq!(finite(&reset).left.unwrap().value.points(),0.);
    let left=change(&reset,"ind","w:left",None).unwrap();let both=change(&left,"ind","w:right",None).unwrap();
    let ind=word_nodes(&both.root,"ind").last().copied().unwrap();
    assert!(attribute(ind,"left").is_none()&&attribute(ind,"right").is_none());
    assert_eq!(finite(&both).left.unwrap().value.points(),24.);assert_eq!(finite(&both).right.unwrap().value.points(),18.);
    assert!(both.source.contains("word.ind("));assert_eq!(both.source.matches("// author note 😀").count(),1);
}

#[test]
fn word_unit_scalar_edits_keep_exact_lexical_source_and_restore_each_inherited_axis(){
    let input=source().replace("left: \"480\"","left: \"0.1cm\"")
        .replace("right: \"360\"","right: \"18.125pt\"")
        .replace("word.ind(left: \"0\", right: \"240\")","word.ind(left: \"1mm\", right: \"1.5pi\")");
    let original=parse(&input,"dsl",true).unwrap();let before=finite(&original);
    assert_eq!(before.left.as_ref().unwrap().value.points(),360./127.);assert_eq!(before.right.as_ref().unwrap().value.points(),18.);
    let changed=change(&original,"ind","w:left",Some("0.1cm")).unwrap();
    assert_eq!(changed.source,input.replace("word.ind(left: \"1mm\", right: \"1.5pi\")","word.ind(left: \"0.1cm\", right: \"1.5pi\")"));
    assert_ne!(changed.revision,original.revision);assert_eq!(finite(&changed),before);
    assert_eq!(attribute(word_nodes(&changed.root,"ind").last().unwrap(),"left"),Some("0.1cm"));
    assert_eq!(change(&changed,"ind","w:left",Some("1mm")).unwrap().source,input);
    let stale=MathPatch::SetAttribute{node_id:node_id(&changed,"ind"),name:"w:left".into(),value:Some("1mm".into())};
    assert_eq!(patch(&changed.source,"dsl",true,&original.revision,&stale).unwrap_err(),"transaction_conflict");
    let left=change(&changed,"ind","w:left",None).unwrap();let left_finite=finite(&left);
    assert_eq!(left_finite.left.unwrap().origin,crate::math_word::WordParagraphOrigin::Style("Base".into()));
    assert_eq!(left_finite.right.unwrap().origin,crate::math_word::WordParagraphOrigin::Direct);
    let cleared=change(&left,"ind","w:right",None).unwrap();let context=crate::math_word::word_paragraph_context(&cleared.root).unwrap();
    assert_eq!(context.effective.indent["left"].value.raw(),"0.1cm");assert_eq!(context.effective.indent["right"].value.raw(),"18.125pt");
    assert_eq!(context.effective.indent["left"].origin,crate::math_word::WordParagraphOrigin::Style("Base".into()));
    assert_eq!(context.effective.indent["right"].origin,crate::math_word::WordParagraphOrigin::Style("Equation".into()));
    let native=crate::omml::docx_math_projection_tree(cleared.root.clone(),true).unwrap().paragraph.unwrap();
    assert_eq!((native.left.as_deref(),native.right.as_deref()),(Some("0.1cm"),Some("18.125pt")));
    assert!(cleared.source.starts_with("// author note 😀\r\n"));assert!(!cleared.source.replace("\r\n","").contains('\n'));
    assert_eq!(original.source,input);
}

#[test]
fn word_unit_edits_keep_schema_grammar_separate_from_exact_consumer_capacity(){
    let original=parse(&source(),"dsl",true).unwrap();
    for raw in ["1mm","0.1cm","0.5in","18.125pt","1.5pc","1.5pi","-0.000mm","0001.0000mm"] {
        let changed=change(&original,"ind","w:left",Some(raw)).unwrap();
        assert_eq!(attribute(word_nodes(&changed.root,"ind").last().unwrap(),"left"),Some(raw));
        assert!(crate::math_word::word_paragraph_context(&changed.root).unwrap().finite_display(false).is_ok(),"{raw}");
        assert_eq!(attribute(word_nodes(&changed.root,"ind").last().unwrap(),"right"),Some("240"));
    }
    for raw in ["1.0",".5mm","1.mm","+1mm","1MM"," 1mm","1e2pt","∞pt"] {
        assert_eq!(change(&original,"ind","w:left",Some(raw)).unwrap_err(),"math_invalid_property","{raw}");
    }
    for raw in [format!("0.{}1cm","0".repeat(64)),format!("{}in","9".repeat(80))] {
        let changed=change(&original,"ind","w:left",Some(&raw)).unwrap();
        let context=crate::math_word::word_paragraph_context(&changed.root).unwrap();let issues=context.finite_display(false).unwrap_err();
        assert_eq!(issues.len(),1);assert_eq!(issues[0].reason,"unsupported-measure-precision");assert_eq!(issues[0].value,raw);
        assert_eq!(context.effective.indent["left"].value.raw(),raw);assert!(changed.source.contains(&raw));
        assert_eq!(preview_at_size(&changed,true,12.).unwrap_err(),"math_unsupported_extension");
    }
}

#[test]
fn word_indent_source_edit_does_not_confuse_legal_unconsumed_values_with_schema_invalid(){
    let original=parse(&source(),"dsl",true).unwrap();
    for value in ["-1","31681","999999999999999999999999999999999999999","1584.000000000000000000000000000001pt","-1.5cm"]{
        let changed=change(&original,"ind","w:left",Some(value)).unwrap();
        assert_eq!(attribute(word_nodes(&changed.root,"ind").last().unwrap(),"left"),Some(value));
        assert!(crate::math_word::word_paragraph_context(&changed.root).unwrap().finite_display(false).is_err());
    }
    for value in ["","1px","3x","NaN","--1","1e2"]{
        assert_eq!(change(&original,"ind","w:left",Some(value)).unwrap_err(),"math_invalid_property","{value}");
    }
}

#[test]
fn word_parse_checks_clean_selected_syntax_and_preserves_legal_unsupported_source(){
    for property in ["word.jc()","word.jc(val: \"\")","word.jc(val: \"centerGroup\")","word.ind(left: \"1px\")","word.ind(left: \"\")"]{
        let input=format!("row(word.pPr({property}), office.oMath(x))");
        assert_eq!(parse(&input,"dsl",true).unwrap_err(),"math_invalid_property","{property}");
    }
    for property in ["word.jc(val: \"both\")","word.jc(val: \"start\"), word.textDirection(val: \"tb\")","word.jc(val: \"end\"), word.textDirection(val: \"tb\")","word.ind(left: \"-1\")","word.ind(left: \"99999999999999999999999999999999999\")","word.ind(right: \"1584.000000000000000000000000000001pt\")"]{
        let input=format!("// keep unsupported 😀\r\nrow(word.pPr({property}), office.oMath(x))");
        let doc=parse(&input,"dsl",true).unwrap();assert_eq!(doc.source,input);
        assert!(crate::math_word::word_paragraph_context(&doc.root).unwrap().finite_display(false).is_err(),"{property}");
    }
}

#[test]
fn word_checked_mathml_preview_reports_capability_and_width_before_rendering(){
    let original=parse(&source(),"dsl",true).unwrap();
    let unsupported=change(&original,"jc","w:val",Some("both")).unwrap();
    assert_eq!(preview_at_size(&unsupported,true,12.).unwrap_err(),"math_unsupported_extension");
    assert_eq!(preview_constrained(&unsupported,true,12.,Some(500.)).err().unwrap(),"math_unsupported_extension");
    // Original sparse left=0/right=240twips leaves no box at 12pt.
    assert_eq!(preview_constrained(&original,true,12.,Some(12.)).err().unwrap(),"math_unsupported_extension");
    assert!(preview_at_size(&original,true,12.).unwrap().contains("<math"));
}

#[test]
fn word_transactions_keep_the_dsl_budget_instead_of_the_legacy_latex_limit(){
    let input=format!("// {}😀\r\n{}","x".repeat(17000),source());assert!(input.len()>MAX_SOURCE);
    let original=parse(&input,"dsl",true).unwrap();let changed=change(&original,"ind","w:left",Some("120")).unwrap();
    assert!(changed.source.starts_with(&input[..input.find("row(").unwrap()]));assert_eq!(finite(&changed).left.unwrap().value.points(),6.);
    let too_large=format!("// {}\n{}","x".repeat(crate::math_dsl::MAX_SOURCE),source());
    assert_eq!(parse(&too_large,"dsl",true).unwrap_err(),"math_budget");
}

#[test]
fn word_scalar_token_edits_keep_utf16_comments_groups_and_unedited_axis_spelling(){
    let value="word.ind(left: ( // keep inside 😀\r\n  -\"001\" // keep tail α\r\n), right: \"00360\")";
    let input=format!("// source prefix 😀\r\nrow(word.pPr({value}, word.jc(val: left)), office.mathPr(office.dispDef(val: \"0\")), office.oMathPara(office.oMath(frac(α, β))))");
    let original=parse(&input,"dsl",true).unwrap();
    let ind_id=node_id(&original,"ind");let mapped=original.nodes.iter().find(|node|node.id==ind_id).unwrap();
    assert!(mapped.from>input[..input.find("word.ind").unwrap()].chars().count());
    let changed=change(&original,"ind","w:left",Some("0")).unwrap();
    for fragment in ["// source prefix 😀\r\n","// keep inside 😀\r\n","// keep tail α\r\n","right: \"00360\"","word.jc(val: left)"]{
        assert!(changed.source.contains(fragment),"Missing untouched source {fragment}: {}",changed.source);
    }
    assert!(!changed.source.contains("-\"001\""));assert_eq!(finite(&changed).left.unwrap().value.points(),0.);
    let cleared=change(&changed,"ind","w:left",None).unwrap();
    for fragment in ["// keep inside 😀\r\n","// keep tail α\r\n","right: \"00360\""]{assert!(cleared.source.contains(fragment));}
    assert!(attribute(word_nodes(&cleared.root,"ind").last().unwrap(),"left").is_none());
    assert_eq!(finite(&cleared).right.unwrap().value.points(),18.);
}

#[test]
fn word_attribute_addition_keeps_empty_container_comments_and_sparse_axes(){
    let input="row(word.pPr(word.ind( // empty but present 😀\r\n)), office.oMathPara(office.oMath(x)))";
    let doc=parse(input,"dsl",true).unwrap();let added=change(&doc,"ind","w:left",Some("0")).unwrap();
    assert!(added.source.contains("// empty but present 😀\r\n"));assert_eq!(finite(&added).left.unwrap().value.points(),0.);
    assert!(finite(&added).right.is_none());
    let removed=change(&added,"ind","w:left",None).unwrap();let ind=word_nodes(&removed.root,"ind").last().copied().unwrap();
    assert!(ind.attributes.keys().all(|key|key=="xmlns"||key.starts_with("xmlns:")));
    assert!(removed.source.contains("word.ind("));assert!(removed.source.contains("// empty but present 😀\r\n"));
    let absent=change(&removed,"ind","w:right",None).unwrap();assert_eq!(absent.source,removed.source);
}

#[test]
fn word_alias_attribute_update_addition_and_clear_use_real_uri_without_rebinding_siblings(){
    let xml=format!(r#"<q:pPr xmlns:q="{WORD_NS}" xmlns:p="{WORD_NS}"><q:ind q:left="00120"/><q:jc p:val="left"/></q:pPr><m:oMathPara xmlns:m="{MATH_NS}"><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath></m:oMathPara>"#);
    let doc=parse(&xml,"omml",true).unwrap();let right=change(&doc,"jc","w:val",Some("right")).unwrap();
    let jc=word_nodes(&right.root,"jc").last().copied().unwrap();assert_eq!(jc.attributes.get("p:val").map(String::as_str),Some("right"));
    assert_eq!(jc.attribute_namespaces.get("p:val").map(String::as_str),Some(WORD_NS));assert!(!jc.attributes.contains_key("w:val"));
    let alias=change(&right,"jc","p:val",Some("center")).unwrap();assert_eq!(finite(&alias).alignment.unwrap().value,crate::math_word::WordParagraphJustification::Center);
    // The unused p prefix is inherited from the actual ancestor binding.
    let added=change(&alias,"ind","p:right",Some("240")).unwrap();let ind=word_nodes(&added.root,"ind").last().copied().unwrap();
    assert_eq!(attribute(ind,"left"),Some("00120"));assert_eq!(attribute(ind,"right"),Some("240"));
    let removed=change(&added,"ind","q:right",None).unwrap();let ind=word_nodes(&removed.root,"ind").last().copied().unwrap();
    assert_eq!(attribute(ind,"left"),Some("00120"));assert!(attribute(ind,"right").is_none());
}

#[test]
fn word_unprefixed_elements_bind_new_word_attributes_locally(){
    let xml=format!(r#"<pPr xmlns="{WORD_NS}" xmlns:w="urn:foreign"><ind/></pPr><m:oMath xmlns:m="{MATH_NS}"><m:r><m:t>x</m:t></m:r></m:oMath>"#);
    let doc=parse(&xml,"omml",true).unwrap();let changed=change(&doc,"ind","w:left",Some("0")).unwrap();
    let paragraph=word_nodes(&changed.root,"pPr")[0];assert_eq!(paragraph.attributes.get("xmlns:w").map(String::as_str),Some("urn:foreign"));
    let ind=word_nodes(&changed.root,"ind")[0];assert_eq!(attribute(ind,"left"),Some("0"));
    assert_eq!(ind.attributes.get("xmlns:w").map(String::as_str),Some(WORD_NS));
}

#[test]
fn word_narrow_operations_do_not_repair_wrong_uri_duplicate_required_or_wrong_parent_input(){
    let math=format!(r#"<m:oMath xmlns:m="{MATH_NS}"><m:r><m:t>x</m:t></m:r></m:oMath>"#);
    for paragraph in [
        format!(r#"<w:pPr xmlns:w="{WORD_NS}" xmlns:f="urn:foreign"><w:jc f:val="left"/></w:pPr>"#),
        format!(r#"<w:pPr xmlns:w="{WORD_NS}" xmlns:f="urn:foreign"><w:jc w:val="left" f:val="preserve"/></w:pPr>"#),
        format!(r#"<w:pPr xmlns:w="{WORD_NS}" xmlns:q="{WORD_NS}"><w:jc w:val="left" q:val="right"/></w:pPr>"#),
        format!(r#"<w:pPr xmlns:w="{WORD_NS}"><w:jc/></w:pPr>"#),
    ]{
        let raw=format!("{paragraph}{math}");
        assert_eq!(parse(&raw,"omml",true).unwrap_err(),"math_invalid_property");
    }
    let raw=format!(r#"<w:rPr xmlns:w="{WORD_NS}"><w:jc w:val="left"/></w:rPr>{math}"#);
    let doc=parse(&raw,"omml",true).unwrap();assert_eq!(change(&doc,"jc","w:val",Some("right")).unwrap_err(),"math_invalid_property");
    let xml=format!(r#"<q:pPr xmlns:q="{WORD_NS}" xmlns:p="urn:foreign"><q:ind/></q:pPr>{math}"#);
    let doc=parse(&xml,"omml",true).unwrap();assert_eq!(change(&doc,"ind","p:right",Some("120")).unwrap_err(),"math_invalid_property");
    let doc=parse(&source(),"dsl",true).unwrap();
    for name in ["m:val","val","f:val","w:start","w:firstLine","w:styleId"]{
        assert_eq!(change(&doc,"jc",name,Some("left")).unwrap_err(),"math_invalid_property");
    }
}

#[test]
fn word_unselected_style_leaf_is_still_syntax_validated_without_finite_rejection(){
    let input="row(word.styles(word.style(word.pPr(word.jc(val: \"left\")), type: \"paragraph\", styleId: \"Unselected\")), word.pPr(word.ind(left: \"0\")), office.oMath(x))";
    let doc=parse(input,"dsl",true).unwrap();let changed=change(&doc,"jc","w:val",Some("both")).unwrap();
    assert!(changed.source.contains("word.jc(val: \"both\")"));assert!(finite(&changed).alignment.is_none());
    assert_eq!(change(&doc,"jc","w:val",Some("unknown")).unwrap_err(),"math_invalid_property");
}

#[test]
fn marked_preview_root_only_marks_real_math_namespace_and_does_not_mutate_source(){
    let doc=parse(&source(),"dsl",true).unwrap();let before=doc.root.xml();let marked=marked_root(&doc);
    for local in ["jc","ind","styles","style","basedOn","pStyle"]{
        for node in word_nodes(&marked,local){assert!(!node.attributes.contains_key("readmdId"),"{}",node.xml());}
    }
    assert!(marked.xml().contains("rd-formula-token-omml-"));
    assert_eq!(crate::math_word::word_paragraph_context(&marked).unwrap(),crate::math_word::word_paragraph_context(&doc.root).unwrap());
    assert_eq!(doc.root.xml(),before);assert_eq!(doc.source,source());
    let mut arbitrary=doc.clone();let mut node=XmlNode::new("other:jc");node.namespace="urn:foreign".into();
    node.id="foreign".into();arbitrary.root.content.push(XmlPart::Element(node));
    let foreign=marked_root(&arbitrary);assert!(!foreign.find("foreign").unwrap().attributes.contains_key("readmdId"));
}

#[test]
fn word_second_equal_formula_uses_current_markdown_utf16_range_and_invertible_document_transaction(){
    let formula=source();let fence=format!("```readmd-math v1\r\n{formula}\r\n```\r\n");
    let markdown=format!("\u{feff}前缀😀\r\n\r\n{fence}\r\n{fence}\r\n{fence}结尾中文\r\n");
    let ranges=crate::math_dsl::markdown_ranges(&markdown).unwrap();assert_eq!(ranges.len(),3);let target=&ranges[1];
    let body=&markdown[target.body_from..target.body_to];let doc=parse(body,"dsl",target.display).unwrap();
    for native in ["right","start","end"] {
    let changed=change(&doc,"jc","w:val",Some(native)).unwrap();
    assert_eq!(attribute(word_nodes(&changed.root,"jc").last().unwrap(),"val"),Some(native));
    assert_eq!(finite(&changed).alignment.unwrap().value.as_str(),if native=="start"{"left"}else{"right"});
    let tx=ContentTransaction{base_revision:transaction::revision(&markdown),origin:"formula".into(),edits:vec![TextEdit{
        from:pos(&markdown,target.body_from),to:pos(&markdown,target.body_to),insert:changed.source.clone()}]};
    let saved=transaction::apply(&markdown,&tx).unwrap();
    assert!(saved.content.starts_with(&markdown[..target.body_from]));assert!(saved.content.ends_with(&markdown[target.body_to..]));
    assert!(saved.content.starts_with('\u{feff}'));assert!(!saved.content.replace("\r\n","").contains('\n'));
    let inverse=ContentTransaction{base_revision:saved.revision.clone(),origin:"formula".into(),edits:saved.inverse};
    assert_eq!(transaction::apply(&saved.content,&inverse).unwrap().content,markdown);
    let stale=MathPatch::SetAttribute{node_id:node_id(&changed,"jc"),name:"w:val".into(),value:Some("center".into())};
    assert_eq!(patch(&changed.source,"dsl",true,&doc.revision,&stale).unwrap_err(),"transaction_conflict");
    assert_eq!(transaction::apply(&saved.content,&tx).unwrap_err(),"transaction_conflict");
    }
}

#[test]
fn word_fractional_unit_edit_targets_only_the_selected_equal_formula_and_inverts_exact_crlf_bytes(){
    let formula=source().replace("word.ind(left: \"0\", right: \"240\")","word.ind(left: \"1mm\", right: \"1.5pi\")");
    let fence=format!("```readmd-math v1\r\n{formula}\r\n```\r\n");
    let markdown=format!("\u{feff}单位前缀😀\r\n\r\n{fence}\r\n{fence}\r\n{fence}尾部α\r\n");
    let ranges=crate::math_dsl::markdown_ranges(&markdown).unwrap();assert_eq!(ranges.len(),3);let target=&ranges[1];
    let doc=parse(&markdown[target.body_from..target.body_to],"dsl",true).unwrap();
    let left=change(&doc,"ind","w:left",Some("0.1cm")).unwrap();
    assert_eq!(finite(&left),finite(&doc));assert_ne!(left.revision,doc.revision);
    let changed=change(&left,"ind","w:right",Some("18.125pt")).unwrap();
    assert_eq!(finite(&changed).left.unwrap().value.points(),360./127.);assert_eq!(finite(&changed).right.unwrap().value.points(),18.125);
    let tx=ContentTransaction{base_revision:transaction::revision(&markdown),origin:"formula".into(),edits:vec![TextEdit{
        from:pos(&markdown,target.body_from),to:pos(&markdown,target.body_to),insert:changed.source.clone()}]};
    let saved=transaction::apply(&markdown,&tx).unwrap();
    assert_eq!(&saved.content[..target.body_from],&markdown[..target.body_from]);assert!(saved.content.ends_with(&markdown[target.body_to..]));
    assert!(saved.content.starts_with('\u{feff}'));assert!(!saved.content.replace("\r\n","").contains('\n'));
    assert_eq!(saved.content.matches("word.ind(left: \"1mm\", right: \"1.5pi\")").count(),2);
    assert_eq!(saved.content.matches("word.ind(left: \"0.1cm\", right: \"18.125pt\")").count(),1);
    let inverse=ContentTransaction{base_revision:saved.revision.clone(),origin:"formula".into(),edits:saved.inverse};
    assert_eq!(transaction::apply(&saved.content,&inverse).unwrap().content,markdown);
    assert_eq!(transaction::apply(&saved.content,&tx).unwrap_err(),"transaction_conflict");
    assert_eq!(doc.source,markdown[target.body_from..target.body_to]);
}

#[test]
fn word_public_document_and_patch_json_shapes_remain_unchanged(){
    let doc=parse(&source(),"dsl",true).unwrap();let value=serde_json::to_value(&doc).unwrap();
    let mut keys=value.as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>();keys.sort();
    assert_eq!(keys,vec!["diagnostics","dialect","nodes","revision","root","source"]);
    let operation=MathPatch::SetAttribute{node_id:node_id(&doc,"ind"),name:"w:left".into(),value:None};
    let json=serde_json::to_value(&operation).unwrap();assert_eq!(json["operation"],"set_attribute");assert_eq!(json["name"],"w:left");assert!(json["value"].is_null());
}
