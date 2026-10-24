//! Finite Word run leaf edits use the existing source-bound scalar transaction.
use super::*;

fn source()->String{
    concat!(
        "// author 😀 é\r\nrow(\r\n",
        "  word.styles(\r\n",
        "    word.style(word.rPr(word.b(), word.i(val: \"0\"), word.color(val: \"112233\"), word.sz(val: \"24\")), type: \"paragraph\", styleId: \"Base\"),\r\n",
        "    word.style(word.basedOn(val: \"Base\"), type: \"paragraph\", styleId: \"Equation\"),\r\n",
        "    word.style(word.rPr(word.b(), word.i(), word.color(val: \"336699\"), word.sz(val: \"30\")), type: \"character\", styleId: \"Accent\"),\r\n",
        "    word.style(word.rPr(word.i(), word.color(val: \"aAbBcC\"), word.sz(val: \"024\")), type: \"character\", styleId: \"Unused\")\r\n",
        "  ),\r\n",
        "  word.pPr(word.pStyle(val: \"Equation\"), word.ind(left: \"0\", right: \"00360\"), word.jc(val: \"left\")),\r\n",
        "  office.mathPr(office.dispDef(val: \"0\")),\r\n",
        "  office.oMathPara(office.oMath(office.f(\r\n",
        "    office.fPr(office.ctrlPr(word.rPr(word.rStyle(val: \"Accent\"), word.i(val: \"off\")))),\r\n",
        "    office.num(office.r(word.rPr(word.rStyle(val: \"Accent\"), word.b(val: ( // scalar open 😀\r\n",
        "      \"off\" // scalar tail α\r\n",
        "    )), word.color(val: \"AA5500\"), word.sz(val: \"024\")), office.t(\"a\"))),\r\n",
        "    office.den(office.r(word.rPr(word.rStyle(val: \"Accent\")), office.t(\"b\")))\r\n",
        "  )))\r\n)"
    ).into()
}

fn leaves<'a>(root:&'a XmlNode,local:&str)->Vec<&'a XmlNode>{
    fn visit<'a>(node:&'a XmlNode,local:&str,out:&mut Vec<&'a XmlNode>){
        if node.namespace==WORD_NS&&node.local()==local{out.push(node);}
        for child in node.children(){visit(child,local,out);}
    }
    let mut out=Vec::new();visit(root,local,&mut out);out
}
fn site(doc:&MathDocument,local:&str,owner:&str)->String{
    leaves(&doc.root,local).into_iter().find(|node|{
        let path=word_node_path(&doc.root,&node.id).unwrap();
        path.len()>=3&&path[path.len()-3].namespace==MATH_NS&&path[path.len()-3].local()==owner
    }).unwrap().id.clone()
}
fn style_leaf(doc:&MathDocument,local:&str,style:&str)->String{
    leaves(&doc.root,local).into_iter().find(|node|{
        let path=word_node_path(&doc.root,&node.id).unwrap();
        path.len()>=3&&path[path.len()-3].namespace==WORD_NS&&path[path.len()-3].local()=="style"
            &&word_attribute_value(path[path.len()-3],"styleId")==Some(style)
    }).unwrap().id.clone()
}
fn change(doc:&MathDocument,id:&str,name:&str,value:Option<&str>)->Result<MathDocument,&'static str>{
    patch(&doc.source,&doc.dialect,true,&doc.revision,&MathPatch::SetAttribute{
        node_id:id.into(),name:name.into(),value:value.map(str::to_string)})
}
fn only_value(before:&MathDocument,after:&MathDocument,id:&str,value:Option<&str>){
    let mut expected=before.root.clone();let node=expected.find_mut(id).unwrap();
    let key=node.attributes.keys().find(|key|key.rsplit(':').next()==Some("val")
        &&node.attribute_namespaces.get(*key).is_some_and(|uri|uri==WORD_NS)).cloned();
    if let Some(key)=key{node.attributes.remove(&key);node.attribute_namespaces.remove(&key);}
    if let Some(value)=value{node.attributes.insert("w:val".into(),value.into());node.attribute_namespaces.insert("w:val".into(),WORD_NS.into());}
    assert_eq!(xml_semantics(&after.root),xml_semantics(&expected));
    assert_eq!(after.revision,transaction::revision(&after.source));
    assert_eq!(before.root.find(id).unwrap().id,after.root.find(id).unwrap().id);
}

#[test]
fn native_run_direct_scalar_edit_keeps_exact_source_trivia_and_unselected_declarations(){
    let input=source();let original=parse(&input,"dsl",true).unwrap();let id=site(&original,"b","r");
    let mapped=original.nodes.iter().find(|node|node.id==id).unwrap();
    let from=transaction::byte_offset(&input,mapped.from).unwrap();let to=transaction::byte_offset(&input,mapped.to).unwrap();
    assert_eq!(&input[from..to],mapped.tex);
    let mut expected=input.clone();expected.replace_range(from..to,&mapped.tex.replacen("\"off\"","\"on\"",1));
    let changed=change(&original,&id,"w:val",Some("on")).unwrap();
    assert_eq!(changed.source,expected);only_value(&original,&changed,&id,Some("on"));
    for keep in ["// author 😀 é\r\n","// scalar open 😀\r\n","// scalar tail α\r\n","right: \"00360\"","word.sz(val: \"024\")"]{assert!(changed.source.contains(keep));}
    assert_eq!(original.source,input);
}

#[test]
fn native_run_switches_keep_empty_off_and_all_absolute_lexical_states(){
    let original=parse(&source(),"dsl",true).unwrap();
    for (local,owner) in [("b","r"),("i","ctrlPr")]{
        let id=site(&original,local,owner);
        for value in ["1","true","on","0","false","off"]{
            let changed=change(&original,&id,"w:val",Some(value)).unwrap();
            only_value(&original,&changed,&id,Some(value));assert_eq!(word_attribute_value(changed.root.find(&id).unwrap(),"val"),Some(value));
        }
        let empty=change(&original,&id,"w:val",None).unwrap();only_value(&original,&empty,&id,None);
        assert!(empty.root.find(&id).unwrap().attributes.keys().all(|key|key=="xmlns"||key.starts_with("xmlns:")));
        let unchanged=change(&empty,&id,"w:val",None).unwrap();assert_eq!(unchanged.source,empty.source);
        let restored=change(&empty,&id,"w:val",Some("off")).unwrap();only_value(&empty,&restored,&id,Some("off"));
        assert!(restored.source.contains(&format!("word.{local}(")));
        if local=="b"{assert!(empty.source.contains("// scalar tail α\r\n"));}
    }
}

#[test]
fn native_run_paragraph_character_and_unselected_style_axes_are_source_editable(){
    let original=parse(&source(),"dsl",true).unwrap();
    for (style,local,value) in [("Base","b","false"),("Base","color","1a2B3c"),("Accent","i","off"),("Accent","sz","+0030"),("Unused","color","ABCDEF"),("Unused","sz","3276")]{
        let id=style_leaf(&original,local,style);let changed=change(&original,&id,"w:val",Some(value)).unwrap();
        only_value(&original,&changed,&id,Some(value));assert_eq!(word_attribute_value(changed.root.find(&id).unwrap(),"val"),Some(value));
    }
    let id=style_leaf(&original,"b","Base");let empty=change(&original,&id,"w:val",None).unwrap();assert_eq!(empty.source,original.source);
    let id=style_leaf(&original,"b","Accent");let off=change(&original,&id,"w:val",Some("off")).unwrap();
    let parsed=crate::omml::parse(&off.root.xml()).unwrap();
    fn first_run(node:&crate::omml::Node)->Option<&crate::omml::Node>{if node.is_math()&&node.name=="r"{Some(node)}else{node.children.iter().find_map(first_run)}}
    // The numerator still has an absolute direct off; a style edit does not
    // erase that declaration or turn a style false into a direct off.
    assert!(!crate::omml::run_style(first_run(&parsed).unwrap()).bold);
}

#[test]
fn native_run_size_and_rgb_are_finite_and_required_while_other_word_attributes_stay_closed(){
    let original=parse(&source(),"dsl",true).unwrap();
    for (local,values) in [("sz",vec!["1","+24","00030","3276"]),("color",vec!["000000","aAbBcC","FFFFFF"]) ]{
        let id=site(&original,local,"r");for value in values{let changed=change(&original,&id,"w:val",Some(value)).unwrap();only_value(&original,&changed,&id,Some(value));}
        assert_eq!(change(&original,&id,"w:val",None).unwrap_err(),"math_invalid_property");
    }
    for (local,values) in [("b",vec!["","TRUE","yes","2"]),("i",vec!["maybe","-0"]),("sz",vec!["0","3277","-1","12.5","1e2","24pt","4294967296"]),("color",vec!["auto","#112233","11223","11223344","gg0000","112233\n"]) ]{
        let owner=if local=="i"{"ctrlPr"}else{"r"};let id=site(&original,local,owner);
        for value in values{assert_eq!(change(&original,&id,"w:val",Some(value)).unwrap_err(),"math_invalid_property","{local}={value}");}
    }
    let id=site(&original,"b","r");
    for name in ["val","m:val","foreign:val","w:color","w:sz","w:themeColor","w:styleId"]{assert_eq!(change(&original,&id,name,Some("on")).unwrap_err(),"math_invalid_property");}
    assert_eq!(change(&original,&id,"w:val",Some(&"x".repeat(4097))).unwrap_err(),"math_invalid_property");
}

#[test]
fn native_run_xml_aliases_preserve_resolved_uri_and_do_not_rebind_an_ancestor(){
    let xml=format!(r#"<m:oMath xmlns:m="{MATH_NS}" xmlns:q="{WORD_NS}" xmlns:p="{WORD_NS}"><m:r><q:rPr><q:b p:val="off"><!--keep--></q:b></q:rPr><m:t>x</m:t></m:r></m:oMath>"#);
    let original=parse(&xml,"omml",true).unwrap();let id=site(&original,"b","r");
    let changed=change(&original,&id,"w:val",Some("on")).unwrap();let node=changed.root.find(&id).unwrap();
    assert_eq!(node.name,"q:b");assert_eq!(node.attributes.get("p:val").map(String::as_str),Some("on"));assert!(!node.attributes.contains_key("w:val"));
    assert_eq!(node.attribute_namespaces.get("p:val").map(String::as_str),Some(WORD_NS));assert!(changed.source.contains("<!--keep-->"));
    let cleared=change(&changed,&id,"p:val",None).unwrap();assert!(word_attribute_value(cleared.root.find(&id).unwrap(),"val").is_none());
    let xml=format!(r#"<m:oMath xmlns:m="{MATH_NS}" xmlns:w="urn:foreign"><m:r><rPr xmlns="{WORD_NS}"><b/></rPr><m:t>x</m:t></m:r></m:oMath>"#);
    let original=parse(&xml,"omml",true).unwrap();let id=site(&original,"b","r");let changed=change(&original,&id,"w:val",Some("off")).unwrap();
    let math=changed.root.children().next().unwrap();assert_eq!(math.attributes.get("xmlns:w").map(String::as_str),Some("urn:foreign"));
    let node=changed.root.find(&id).unwrap();assert_eq!(node.attributes.get("xmlns:w").map(String::as_str),Some(WORD_NS));assert_eq!(word_attribute_value(node,"val"),Some("off"));
}

#[test]
fn native_run_edit_validates_aliased_math_attributes_without_changing_author_namespaces(){
    let xml=format!(r#"<a:mathPr xmlns:a="{MATH_NS}"><a:mathFont a:val="STIX Two Math"/></a:mathPr><a:oMath xmlns:a="{MATH_NS}" xmlns:w="{WORD_NS}"><a:r><w:rPr><w:b w:val="off"/></w:rPr><a:t>x</a:t></a:r></a:oMath>"#);
    let original=parse(&xml,"omml",true).unwrap();let before=original.root.clone();let id=site(&original,"b","r");
    let changed=change(&original,&id,"w:val",Some("on")).unwrap();only_value(&original,&changed,&id,Some("on"));
    let property=changed.root.children().find(|node|node.namespace==MATH_NS&&node.local()=="mathPr").unwrap().children().next().unwrap();
    assert_eq!(property.name,"a:mathFont");assert_eq!(property.attributes.get("a:val").map(String::as_str),Some("STIX Two Math"));
    assert_eq!(property.attribute_namespaces.get("a:val").map(String::as_str),Some(MATH_NS));assert!(!property.attributes.contains_key("m:val"));
    assert_eq!(original.root,before);assert_eq!(original.source,xml);
}

#[test]
fn native_run_edit_rejects_duplicate_math_aliases_and_foreign_canonical_attribute_spoofing(){
    for font in [
        "<a:mathFont a:val=\"STIX Two Math\" q:val=\"STIX Two Math\"/>",
        "<a:mathFont m:val=\"STIX Two Math\"/>",
        "<a:mathFont f:val=\"STIX Two Math\"/>",
        "<a:mathFont a:val=\"STIX Two Math\" m:val=\"Foreign must not shadow\"/>",
        "<a:mathFont a:val=\"0123456789012345678901234567890123\"/>",
    ]{
        let xml=format!(r#"<a:mathPr xmlns:a="{MATH_NS}" xmlns:q="{MATH_NS}" xmlns:m="urn:foreign" xmlns:f="urn:foreign">{font}</a:mathPr><a:oMath xmlns:a="{MATH_NS}" xmlns:w="{WORD_NS}"><a:r><w:rPr><w:b w:val="off"/></w:rPr><a:t>x</a:t></a:r></a:oMath>"#);
        let original=parse(&xml,"omml",true).unwrap();let before=original.root.clone();let id=site(&original,"b","r");
        assert_eq!(change(&original,&id,"w:val",Some("on")).unwrap_err(),"math_invalid_property","{font}");
        assert_eq!(original.root,before);assert_eq!(original.source,xml);
    }
}

#[test]
fn native_run_edits_reject_wrong_owner_and_do_not_repair_invalid_original_shapes(){
    let math=format!(r#"<m:oMath xmlns:m="{MATH_NS}"><m:r><m:t>x</m:t></m:r></m:oMath>"#);
    for prefix in [
        format!(r#"<w:rPr xmlns:w="{WORD_NS}"><w:b/></w:rPr>"#),
        format!(r#"<w:pPr xmlns:w="{WORD_NS}"><w:rPr><w:b/></w:rPr></w:pPr>"#),
        format!(r#"<w:styles xmlns:w="{WORD_NS}"><w:docDefaults><w:rPrDefault><w:rPr><w:b/></w:rPr></w:rPrDefault></w:docDefaults></w:styles>"#),
        format!(r#"<w:styles xmlns:w="{WORD_NS}"><w:style w:type="table" w:styleId="T"><w:rPr><w:b/></w:rPr></w:style></w:styles>"#),
    ]{
        let original=parse(&(prefix+&math),"omml",true).unwrap();let id=leaves(&original.root,"b")[0].id.clone();
        assert_eq!(change(&original,&id,"w:val",Some("on")).unwrap_err(),"math_invalid_property");
    }
    for pr in [
        "<w:rPr><w:b w:val=\"maybe\"/></w:rPr>",
        "<w:rPr><w:b/><w:b/></w:rPr>",
        "<w:rPr><w:b><w:i/></w:b></w:rPr>",
        "<w:rPr><w:b>ink</w:b></w:rPr>",
        "<w:rPr><w:b w:val=\"off\" q:val=\"on\"/></w:rPr>",
        "<w:rPr><w:b f:val=\"off\"/></w:rPr>",
        "<w:rPr w:val=\"bad\"><w:b/></w:rPr>",
        "<w:rPr><w:b/></w:rPr><w:rPr/>",
        "<w:rPr><w:b/><w:color w:val=\"112233\" w:themeColor=\"accent1\"/></w:rPr>",
    ]{
        let xml=format!(r#"<m:oMath xmlns:m="{MATH_NS}" xmlns:w="{WORD_NS}" xmlns:q="{WORD_NS}" xmlns:f="urn:foreign"><m:r>{pr}<m:t>x</m:t></m:r></m:oMath>"#);
        let original=parse(&xml,"omml",true).unwrap();let id=leaves(&original.root,"b")[0].id.clone();
        assert_eq!(change(&original,&id,"w:val",Some("on")).unwrap_err(),"math_invalid_property","{pr}");assert_eq!(original.source,xml);
    }
    for xml in [
        format!(r#"<m:oMath xmlns:m="{MATH_NS}" xmlns:w="{WORD_NS}"><m:r><w:b/><m:t>x</m:t></m:r></m:oMath>"#),
        format!(r#"<m:oMath xmlns:m="{MATH_NS}" xmlns:w="{WORD_NS}"><m:ctrlPr><w:rPr><w:b/></w:rPr></m:ctrlPr></m:oMath>"#),
        format!(r#"<m:oMath xmlns:m="{MATH_NS}" xmlns:w="{WORD_NS}" xmlns:f="urn:foreign"><f:wrapper><m:r><w:rPr><w:b/></w:rPr><m:t>x</m:t></m:r></f:wrapper></m:oMath>"#),
    ]{
        let original=parse(&xml,"omml",true).unwrap();let id=leaves(&original.root,"b")[0].id.clone();assert_eq!(change(&original,&id,"w:val",Some("on")).unwrap_err(),"math_invalid_property");
    }
}

#[test]
fn native_run_unselected_style_invalid_ancestry_cannot_be_repaired_by_a_leaf_edit(){
    for reference in ["Missing","Unused","Base"]{
        let input=source().replace("word.style(word.rPr(word.i(), word.color(val: \"aAbBcC\"), word.sz(val: \"024\")), type: \"character\", styleId: \"Unused\")",
            &format!("word.style(word.basedOn(val: \"{reference}\"), word.rPr(word.i(), word.color(val: \"aAbBcC\"), word.sz(val: \"024\")), type: \"character\", styleId: \"Unused\")"));
        let original=parse(&input,"dsl",true).unwrap();let id=style_leaf(&original,"color","Unused");
        assert_eq!(change(&original,&id,"w:val",Some("FFFFFF")).unwrap_err(),"math_invalid_property");assert_eq!(original.source,input);
    }
}

#[test]
fn native_run_second_equal_formula_uses_exact_utf16_crlf_range_and_inverse(){
    let formula=source();let fence=format!("```readmd-math v1\r\n{formula}\r\n```\r\n");
    let markdown=format!("\u{feff}原文😀\r\n\r\n{fence}\r\n{fence}\r\n{fence}结尾𝛂\r\n");
    let ranges=crate::math_dsl::markdown_ranges(&markdown).unwrap();assert_eq!(ranges.len(),3);let target=&ranges[1];
    let original=parse(&markdown[target.body_from..target.body_to],"dsl",true).unwrap();let id=site(&original,"b","r");
    let changed=change(&original,&id,"w:val",Some("on")).unwrap();only_value(&original,&changed,&id,Some("on"));
    let tx=ContentTransaction{base_revision:transaction::revision(&markdown),origin:"formula".into(),edits:vec![TextEdit{
        from:pos(&markdown,target.body_from),to:pos(&markdown,target.body_to),insert:changed.source.clone()}]};
    assert!(tx.edits[0].from>markdown[..target.body_from].chars().count());
    let result=transaction::apply(&markdown,&tx).unwrap();assert_eq!(&result.content[..target.body_from],&markdown[..target.body_from]);assert!(result.content.ends_with(&markdown[target.body_to..]));
    assert!(!result.content.replace("\r\n","").contains('\n'));assert_eq!(crate::math_dsl::markdown_ranges(&result.content).unwrap().len(),3);
    let inverse=ContentTransaction{base_revision:result.revision.clone(),origin:"formula".into(),edits:result.inverse};
    assert_eq!(transaction::apply(&result.content,&inverse).unwrap().content,markdown);
    assert_eq!(transaction::apply(&result.content,&tx).unwrap_err(),"transaction_conflict");
    assert_eq!(patch(&changed.source,"dsl",true,&original.revision,&MathPatch::SetAttribute{node_id:id,name:"w:val".into(),value:Some("off".into())}).unwrap_err(),"transaction_conflict");
}

#[test]
fn native_run_local_edits_keep_the_dsl_budget_and_existing_paragraph_editor(){
    let input=format!("// {}😀\r\n{}","x".repeat(17000),source());let original=parse(&input,"dsl",true).unwrap();
    let id=site(&original,"b","r");let changed=change(&original,&id,"w:val",Some("on")).unwrap();assert!(changed.source.starts_with(&input[..input.find("row(").unwrap()]));
    let id=leaves(&changed.root,"ind")[0].id.clone();let indent=change(&changed,&id,"w:left",Some("1mm")).unwrap();
    assert_eq!(word_attribute_value(indent.root.find(&id).unwrap(),"left"),Some("1mm"));
    assert_eq!(word_attribute_value(leaves(&indent.root,"ind")[0],"right"),Some("00360"));
    let id=leaves(&indent.root,"jc")[0].id.clone();let right=change(&indent,&id,"w:val",Some("right")).unwrap();
    assert_eq!(word_attribute_value(right.root.find(&id).unwrap(),"val"),Some("right"));assert!(right.source.contains("// scalar tail α\r\n"));
}
