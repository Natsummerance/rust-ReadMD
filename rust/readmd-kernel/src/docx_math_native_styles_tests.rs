//! Generated OPC parts and an independent paragraph inheritance oracle.
//! Application layout and the full Word style schema remain separate gates.
use super::*;
use crate::math_document::{XmlNode, XmlPart, WORD_NS, MATH_NS};
use std::collections::BTreeMap;

fn word(node:&XmlNode,name:&str)->bool{node.namespace==WORD_NS&&node.local()==name}
fn child<'a>(node:&'a XmlNode,name:&str)->Option<&'a XmlNode>{node.children().find(|node|word(node,name))}
fn attr<'a>(node:&'a XmlNode,name:&str)->Option<&'a str>{node.attributes.iter().find_map(|(key,value)|
    (key.rsplit(':').next()==Some(name)&&node.attribute_namespaces.get(key).is_some_and(|uri|uri==WORD_NS)).then_some(value.as_str()))}
fn part<'a>(package:&'a DocxPackage,name:&str)->&'a str{std::str::from_utf8(&package.parts.iter().find(|(path,_)|path==name).expect(name).1).unwrap()}
fn tree(xml:&str)->XmlNode{crate::math_document::parse_xml(xml).unwrap()}
fn descendants<'a>(node:&'a XmlNode,name:&str)->Vec<&'a XmlNode>{
    let mut out=Vec::new();if word(node,name){out.push(node);}for node in node.children(){out.extend(descendants(node,name));}out
}
fn style<'a>(root:&'a XmlNode,id:&str)->&'a XmlNode{descendants(root,"style").into_iter().find(|node|attr(node,"styleId")==Some(id)).unwrap()}
fn formula_paragraphs(root:&XmlNode)->Vec<&XmlNode>{descendants(root,"p").into_iter().filter(|node|
    node.children().any(|node|node.namespace==MATH_NS&&node.local()=="oMathPara")).collect()}
fn selected(paragraph:&XmlNode)->&str{attr(child(child(paragraph,"pPr").unwrap(),"pStyle").unwrap(),"val").unwrap()}
fn overlay(out:&mut BTreeMap<String,String>,ppr:Option<&XmlNode>){
    if let Some(ppr)=ppr{for node in ppr.children(){
        if word(node,"jc"){out.insert("jc".into(),attr(node,"val").unwrap().into());}
        if word(node,"ind"){for axis in ["left","right"]{if let Some(value)=attr(node,axis){out.insert(axis.into(),value.into());}}}
    }}
}
fn effective(styles:&XmlNode,paragraph:&XmlNode)->BTreeMap<String,String>{
    let catalogue=child(styles,"styles").unwrap();let mut out=BTreeMap::new();
    overlay(&mut out,child(catalogue,"docDefaults").and_then(|node|child(node,"pPrDefault")).and_then(|node|child(node,"pPr")));
    let mut chain=Vec::new();let mut seen=BTreeSet::new();let mut id=Some(selected(paragraph));
    while let Some(current)=id{
        assert!(chain.len()<128&&seen.insert(current));let current_style=style(styles,current);
        id=child(current_style,"basedOn").map(|node|attr(node,"val").unwrap());chain.push(current_style);
    }
    for current in chain.into_iter().rev(){overlay(&mut out,child(current,"pPr"));}
    overlay(&mut out,child(paragraph,"pPr"));out
}
fn source(base_id:&str,equation_id:&str,left:&str,direct:&str,defaults:&str,label:&str,default_style:bool)->String{
    format!(r#"<w:styles xmlns:w="{WORD_NS}">{defaults}<w:style w:type="paragraph" w:styleId="{base_id}"><w:name w:val="Base equations"/><w:pPr><w:ind w:left="{left}" w:right="1cm"/><w:jc w:val="right"/></w:pPr></w:style><w:style w:type="paragraph" w:styleId="{equation_id}"{default_attribute}><w:name w:val="Equations"/><w:basedOn w:val="{base_id}"/><w:pPr><w:ind w:right="18.125pt"/></w:pPr></w:style></w:styles><w:pPr xmlns:w="{WORD_NS}">{selector}{direct}</w:pPr><m:mathPr xmlns:m="{MATH_NS}"><m:dispDef m:val="0"/></m:mathPr><m:oMathPara xmlns:m="{MATH_NS}"><m:oMath><m:r><m:t>{label}</m:t></m:r></m:oMath></m:oMathPara>"#,
        default_attribute=if default_style{" w:default=\"1\""}else{""},
        selector=if default_style{String::new()}else{format!("<w:pStyle w:val=\"{equation_id}\"/>")})
}
fn office(xml:String)->Block{Block::OfficeMath{latex:"x".into(),omml:xml}}
fn package(blocks:Vec<Block>)->DocxPackage{
    let document=Document{front_matter:None,blocks};let options=crate::export_styles::sanitize(&serde_json::json!({}));
    let package=build(&document,&options,"Native style test",&|_|None);assert!(!package.parts.is_empty(),"{:?}",package.warns);package
}
fn base_blocks()->Vec<Block>{
    let mut doc=crate::md_ast::parse("# Ordinary heading\n\nOrdinary text with **bold** and a [link](#ordinary-heading).\n\n| A | B |\n|---|---|\n| a | b |\n\n> Ordinary quote.\n\nText with note.[^n]\n\n[^n]: Ordinary note.\n");
    // No associated Word context, but the same document-wide math policy.
    // Explicit, unassociated OfficeMath remains a strict native declaration;
    // only carrier-free ordinary LaTeX can inherit the admitted document policy.
    doc.blocks.push(office(format!(r#"<m:mathPr xmlns:m="{MATH_NS}"><m:dispDef m:val="0"/></m:mathPr><m:oMathPara xmlns:m="{MATH_NS}"><m:oMath><m:r><m:t>unassociated</m:t></m:r></m:oMath></m:oMathPara>"#)));doc.blocks
}

#[test]
fn native_chain_is_real_sparse_and_ordinary_catalogue_and_paragraphs_stay_unchanged(){
    let baseline=package(base_blocks());let mut blocks=base_blocks();
    blocks.push(office(source("Base","Equation","1mm","<w:ind w:left=\"0\"/>","","x",false)));
    let output=package(blocks);assert!(!output.warns.iter().any(|code|code=="math_paragraph_style_materialized"));
    let old_styles=tree(part(&baseline,"word/styles.xml"));let styles=tree(part(&output,"word/styles.xml"));
    for old in descendants(&old_styles,"style"){
        assert_eq!(crate::math_document::xml_semantics(old),crate::math_document::xml_semantics(style(&styles,attr(old,"styleId").unwrap())));
    }
    assert_eq!(crate::math_document::xml_semantics(child(child(&old_styles,"styles").unwrap(),"docDefaults").unwrap()),crate::math_document::xml_semantics(child(child(&styles,"styles").unwrap(),"docDefaults").unwrap()));
    for path in ["word/document.xml","word/footnotes.xml"]{
        let old_body=tree(part(&baseline,path));let body=tree(part(&output,path));
        let old=descendants(&old_body,"p");let new=descendants(&body,"p");
        assert_eq!(new.len(),old.len()+usize::from(path=="word/document.xml"));
        for (before,after) in old.iter().zip(&new){assert_eq!(crate::math_document::xml_semantics(before),crate::math_document::xml_semantics(after));}
    }
    let body=tree(part(&output,"word/document.xml"));let formula=*formula_paragraphs(&body).last().unwrap();let ppr=child(formula,"pPr").unwrap();
    assert_eq!(selected(formula),"Equation");assert_eq!(ppr.children().map(XmlNode::local).collect::<Vec<_>>(),["pStyle","ind"]);
    assert_eq!(attr(child(ppr,"ind").unwrap(),"left"),Some("0"));assert!(child(ppr,"jc").is_none());
    let axes=effective(&styles,formula);assert_eq!(axes.get("left").map(String::as_str),Some("0"));assert_eq!(axes.get("right").map(String::as_str),Some("18.125pt"));assert_eq!(axes.get("jc").map(String::as_str),Some("right"));
    let mut removed=formula.clone();if let XmlPart::Element(ppr)=removed.content.iter_mut().find(|part|matches!(part,XmlPart::Element(node)if word(node,"pPr"))).unwrap(){ppr.content.retain(|part|!matches!(part,XmlPart::Element(node)if word(node,"ind")));}
    assert_eq!(effective(&styles,&removed).get("left").map(String::as_str),Some("1mm"));
}

#[test]
fn conflicting_ids_and_ancestry_have_stable_independent_native_bindings_in_both_orders(){
    let a=source("Base","Equation","1mm","","","A",false);
    let b=source("Base","Equation","0.5in","","","B",false);
    let mut mapping=None;
    for pair in [[a.clone(),b.clone()],[b.clone(),a.clone()]]{
        let output=package(pair.into_iter().map(office).collect());let styles=tree(part(&output,"word/styles.xml"));let body=tree(part(&output,"word/document.xml"));let mut actual=BTreeMap::new();
        for paragraph in formula_paragraphs(&body){let label=paragraph.text();let selected=selected(paragraph);assert_ne!(selected,"Equation");let axes=effective(&styles,paragraph);
            assert_eq!(axes.get("left").map(String::as_str),Some(if label=="A"{"1mm"}else{"0.5in"}));actual.insert(label,selected.to_owned());}
        assert_eq!(actual.len(),2);assert_ne!(actual["A"],actual["B"]);if let Some(expected)=&mapping{assert_eq!(&actual,expected);}else{mapping=Some(actual);}
    }
}

#[test]
fn template_ids_are_never_reused_for_foreign_definitions_or_primary_defaults(){
    let baseline=package(base_blocks());let mut blocks=base_blocks();blocks.push(office(source("Normal","ReadMDMath","2pc","","","x",true)));
    let output=package(blocks);let old=tree(part(&baseline,"word/styles.xml"));let styles=tree(part(&output,"word/styles.xml"));
    for id in ["Normal","ReadMDMath","BodyText"]{assert_eq!(crate::math_document::xml_semantics(style(&old,id)),crate::math_document::xml_semantics(style(&styles,id)));}
    let body=tree(part(&output,"word/document.xml"));let formula=*formula_paragraphs(&body).last().unwrap();assert_ne!(selected(formula),"ReadMDMath");
    assert_eq!(effective(&styles,formula).get("left").map(String::as_str),Some("2pc"));
    assert_eq!(descendants(&styles,"style").into_iter().filter(|node|matches!(attr(node,"default"),Some("1"|"true"|"on"))).count(),1);
    let reference=child(style(&styles,selected(formula)),"basedOn").unwrap();assert_ne!(attr(reference,"val"),Some("Normal"));
}

#[test]
fn nonempty_global_defaults_fall_back_with_warning_without_touching_generated_defaults(){
    let defaults="<w:docDefaults><w:pPrDefault><w:pPr><w:ind w:left=\"1mm\"/></w:pPr></w:pPrDefault></w:docDefaults>";
    let baseline=package(base_blocks());let mut blocks=base_blocks();blocks.push(office(source("Base","Equation","2pc","",defaults,"x",false)));
    let output=package(blocks);assert_eq!(output.warns.iter().filter(|code|code.as_str()=="math_paragraph_style_materialized").count(),1);
    let old=tree(part(&baseline,"word/styles.xml"));let styles=tree(part(&output,"word/styles.xml"));assert_eq!(crate::math_document::xml_semantics(&old),crate::math_document::xml_semantics(&styles));
    let body=tree(part(&output,"word/document.xml"));let formula=*formula_paragraphs(&body).last().unwrap();assert_eq!(selected(formula),"ReadMDMath");assert_eq!(effective(&styles,formula).get("left").map(String::as_str),Some("2pc"));
}

#[test]
fn native_style_output_keeps_integer_attribute_tabs_as_numeric_entities(){
    let output=package(vec![office(source("Base","Equation"," +000720&#9;","<w:ind w:right=\"  +00360&#10;\"/>","","x",false))]);
    let raw_styles=part(&output,"word/styles.xml");let raw_body=part(&output,"word/document.xml");assert!(raw_styles.contains("&#9;"));assert!(raw_body.contains("&#10;"));
    let styles=tree(raw_styles);let body=tree(raw_body);let formula=formula_paragraphs(&body)[0];let axes=effective(&styles,formula);
    assert_eq!(axes.get("left").map(String::as_str),Some(" +000720\t"));assert_eq!(axes.get("right").map(String::as_str),Some("  +00360\n"));
}

#[test]
fn native_registry_bare_latex_inherits_only_admitted_math_defaults_without_word_style(){
    let native=office(source("Base","Equation","1mm","","","n",false));
    for reverse in [false,true]{
        let mut blocks=vec![Block::Math("x^2".into()),native.clone()];if reverse{blocks.reverse();}
        let doc=Document{front_matter:None,blocks};let before=doc.clone();
        let options=crate::export_styles::sanitize(&serde_json::json!({}));let output=build(&doc,&options,"Mixed global defaults",&|_|None);
        assert!(!output.parts.is_empty(),"{:?}",output.warns);assert_eq!(doc,before);
        let body=tree(part(&output,"word/document.xml"));let paragraphs=formula_paragraphs(&body);
        assert_eq!(paragraphs.len(),2);let plain=paragraphs[usize::from(reverse)];let imported=paragraphs[usize::from(!reverse)];
        assert_eq!(selected(plain),"ReadMDMath");assert!(child(child(plain,"pPr").unwrap(),"ind").is_none());
        assert_eq!(selected(imported),"Equation");
        let styles=tree(part(&output,"word/styles.xml"));assert_eq!(effective(&styles,imported).get("left").map(String::as_str),Some("1mm"));
    }
}

#[test]
fn native_registry_still_rejects_explicit_incompatible_office_defaults(){
    let explicit=office(format!(r#"<m:mathPr xmlns:m="{MATH_NS}"><m:dispDef/></m:mathPr><m:oMathPara xmlns:m="{MATH_NS}"><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath></m:oMathPara>"#));
    for reverse in [false,true]{
        let mut blocks=vec![explicit.clone(),office(source("Base","Equation","1mm","","","n",false))];if reverse{blocks.reverse();}
        let doc=Document{front_matter:None,blocks};let options=crate::export_styles::sanitize(&serde_json::json!({}));
        let output=build(&doc,&options,"Explicit conflict",&|_|panic!("No resources before defaults preflight"));
        assert!(output.parts.is_empty());assert!(output.warns[0].starts_with("math_document_defaults_conflict: "));assert!(output.warns[0].contains("dispDef"));
    }
}
