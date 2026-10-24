//! Native OPC run-style integration: an independent four-axis cascade checks
//! actual references and later native edits, not the ReadMD source payload.
use super::*;
use crate::math_document::{XmlNode,XmlPart,WORD_NS,MATH_NS};
use std::collections::{BTreeMap,BTreeSet};

fn w(node:&XmlNode,name:&str)->bool{node.namespace==WORD_NS&&node.local()==name}
fn child<'a>(node:&'a XmlNode,name:&str)->Option<&'a XmlNode>{node.children().find(|node|w(node,name))}
fn attr<'a>(node:&'a XmlNode,name:&str)->Option<&'a str>{node.attributes.iter().find_map(|(key,value)|
    (key.rsplit(':').next()==Some(name)&&node.attribute_namespaces.get(key).is_some_and(|uri|uri==WORD_NS)).then_some(value.as_str()))}
fn tree(xml:&str)->XmlNode{crate::math_document::parse_xml(xml).unwrap()}
fn nodes<'a>(node:&'a XmlNode,namespace:&str,name:&str)->Vec<&'a XmlNode>{
    let mut out=Vec::new();if node.namespace==namespace&&node.local()==name{out.push(node);}for node in node.children(){out.extend(nodes(node,namespace,name));}out
}
fn part<'a>(package:&'a DocxPackage,name:&str)->&'a str{std::str::from_utf8(&package.parts.iter().find(|(path,_)|path==name).expect(name).1).unwrap()}
fn style<'a>(styles:&'a XmlNode,id:&str)->&'a XmlNode{nodes(styles,WORD_NS,"style").into_iter().find(|node|attr(node,"styleId")==Some(id)).expect(id)}
fn chain<'a>(styles:&'a XmlNode,id:&str,kind:&str)->Vec<&'a XmlNode>{
    let mut out=Vec::new();let mut seen=BTreeSet::new();let mut id=Some(id);
    while let Some(current)=id{assert!(out.len()<128&&seen.insert(current));let node=style(styles,current);assert_eq!(attr(node,"type"),Some(kind));out.push(node);id=child(node,"basedOn").and_then(|n|attr(n,"val"));}
    out.reverse();out
}
#[derive(Debug,Clone,PartialEq,Eq,Default)]
struct Four{b:Option<bool>,i:Option<bool>,sz:Option<String>,color:Option<String>}
fn apply(out:&mut Four,pr:Option<&XmlNode>,toggle:bool){
    if let Some(pr)=pr{for node in pr.children(){
        if w(node,"b")||w(node,"i"){
            let value=match attr(node,"val").unwrap_or("1"){"1"|"true"|"on"=>true,"0"|"false"|"off"=>false,value=>panic!("Unexpected toggle {value}")};
            let target=if w(node,"b"){&mut out.b}else{&mut out.i};
            if toggle{if value{*target=Some(!target.unwrap_or(false));}}else{*target=Some(value);}
        }else if w(node,"sz"){out.sz=Some(attr(node,"val").unwrap().into());}
        else if w(node,"color"){out.color=Some(attr(node,"val").unwrap().to_ascii_uppercase());}
    }}
}
fn selected(p:&XmlNode)->&str{attr(child(child(p,"pPr").unwrap(),"pStyle").unwrap(),"val").unwrap()}
fn formula_paragraphs(body:&XmlNode)->Vec<&XmlNode>{nodes(body,WORD_NS,"p").into_iter().filter(|p|!nodes(p,MATH_NS,"oMathPara").is_empty()).collect()}
fn effective(styles:&XmlNode,p:&XmlNode,site:&XmlNode)->Four{
    let mut out=Four::default();let catalogue=child(styles,"styles").unwrap();
    apply(&mut out,child(catalogue,"docDefaults").and_then(|n|child(n,"rPrDefault")).and_then(|n|child(n,"rPr")),false);
    for node in chain(styles,selected(p),"paragraph"){apply(&mut out,child(node,"rPr"),true);}
    let direct=child(site,"rPr");
    if let Some(id)=direct.and_then(|n|child(n,"rStyle")).and_then(|n|attr(n,"val")){
        for node in chain(styles,id,"character"){apply(&mut out,child(node,"rPr"),true);}
    }
    apply(&mut out,direct,false);out
}
fn source(color:&str,size:&str,label:&str,defaults:&str)->String{
    format!(r#"<w:styles xmlns:w="{WORD_NS}">{defaults}<w:style w:type="paragraph" w:styleId="Base"><w:pPr><w:jc w:val="center"/></w:pPr><w:rPr><w:b/><w:i/><w:color w:val="112233"/><w:sz w:val="28"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Equation"><w:basedOn w:val="Base"/></w:style><w:style w:type="character" w:styleId="CharBase"><w:rPr><w:b/><w:i/></w:rPr></w:style><w:style w:type="character" w:styleId="Accent"><w:basedOn w:val="CharBase"/><w:rPr><w:b w:val="false"/><w:color w:val="{color}"/><w:sz w:val="{size}"/></w:rPr></w:style></w:styles><w:pPr xmlns:w="{WORD_NS}"><w:pStyle w:val="Equation"/></w:pPr><m:mathPr xmlns:m="{MATH_NS}"><m:dispDef m:val="0"/></m:mathPr><m:oMathPara xmlns:m="{MATH_NS}"><m:oMath><m:f><m:fPr><m:ctrlPr xmlns:w="{WORD_NS}"><w:rPr><w:rStyle w:val="Accent"/><w:rFonts w:ascii="STIXTwoMath" w:hAnsi="STIXTwoMath"/></w:rPr></m:ctrlPr></m:fPr><m:num><m:r xmlns:w="{WORD_NS}"><w:rPr><w:rStyle w:val="Accent"/><w:rFonts w:ascii="STIXTwoMath" w:hAnsi="STIXTwoMath"/></w:rPr><m:t>{label}</m:t></m:r></m:num><m:den><m:r xmlns:w="{WORD_NS}"><w:rPr><w:rStyle w:val="Accent"/><w:rFonts w:ascii="STIXTwoMath" w:hAnsi="STIXTwoMath"/><w:b w:val="off"/><w:color w:val="AA5500"/><w:sz w:val="24"/></w:rPr><m:t>b</m:t></m:r></m:den></m:f></m:oMath></m:oMathPara>"#)
}
fn office(xml:String)->Block{Block::OfficeMath{latex:"a/b".into(),omml:xml}}
fn package(blocks:Vec<Block>)->DocxPackage{
    let document=Document{front_matter:None,blocks};let before=document.clone();
    let options=crate::export_styles::sanitize(&serde_json::json!({}));let package=build(&document,&options,"Run styles",&|_|None);
    assert_eq!(document,before,"Export mutated the author model");assert!(!package.parts.is_empty(),"{:?}",package.warns);package
}
fn change_style(styles:&XmlNode,id:&str,axis:&str,value:&str)->XmlNode{
    let mut styles=styles.clone();fn visit(node:&mut XmlNode,id:&str,axis:&str,value:&str){
        if w(node,"style")&&attr(node,"styleId")==Some(id){
            let pr=node.content.iter_mut().find_map(|p|match p{XmlPart::Element(n)if w(n,"rPr")=>Some(n),_=>None}).unwrap();
            let property=pr.content.iter_mut().find_map(|p|match p{XmlPart::Element(n)if w(n,axis)=>Some(n),_=>None}).unwrap();
            let key=property.attributes.keys().find(|key|key.rsplit(':').next()==Some("val")).unwrap().clone();property.attributes.insert(key,value.into());return;
        }
        for p in &mut node.content{if let XmlPart::Element(n)=p{visit(n,id,axis,value);}}
    }visit(&mut styles,id,axis,value);styles
}

#[test]
fn native_run_references_are_sparse_and_current_style_edits_reach_only_inherited_axes(){
    let out=package(vec![office(source("336699","30","a",""))]);let styles=tree(part(&out,"word/styles.xml"));let body=tree(part(&out,"word/document.xml"));let p=formula_paragraphs(&body)[0];
    assert!(!out.warns.iter().any(|code|code==crate::api_codes::WARN_MATH_RUN_STYLE_MATERIALIZED));
    let runs=nodes(p,MATH_NS,"r");assert_eq!(runs.len(),2);let control=nodes(p,MATH_NS,"ctrlPr")[0];
    for site in [runs[0],control]{let pr=child(site,"rPr").unwrap();assert!(child(pr,"rStyle").is_some());for axis in ["b","i","sz","color"]{assert!(child(pr,axis).is_none(),"Inherited {axis} was shadowed by direct formatting");}}
    let inherited=Four{b:Some(false),i:Some(false),sz:Some("30".into()),color:Some("336699".into())};
    assert_eq!(effective(&styles,p,runs[0]),inherited);assert_eq!(effective(&styles,p,control),inherited);
    assert_eq!(effective(&styles,p,runs[1]),Four{b:Some(false),i:Some(false),sz:Some("24".into()),color:Some("AA5500".into())});
    let id=attr(child(child(runs[0],"rPr").unwrap(),"rStyle").unwrap(),"val").unwrap();
    let changed=change_style(&change_style(&styles,id,"color","A1B2C3"),id,"sz","36");
    assert_eq!(effective(&changed,p,runs[0]),Four{color:Some("A1B2C3".into()),sz:Some("36".into()),..inherited.clone()});
    assert_eq!(effective(&changed,p,control),effective(&changed,p,runs[0]));
    assert_eq!(effective(&changed,p,runs[1]),effective(&styles,p,runs[1]));
}

#[test]
fn deleting_direct_off_restores_style_toggle_inheritance_without_invented_defaults(){
    let xml=source("336699","30","a","").replace("<w:rPr><w:b/><w:i/></w:rPr>","<w:rPr><w:i/></w:rPr>");
    let out=package(vec![office(xml)]);let styles=tree(part(&out,"word/styles.xml"));let body=tree(part(&out,"word/document.xml"));let p=formula_paragraphs(&body)[0];let runs=nodes(p,MATH_NS,"r");
    assert_eq!(effective(&styles,p,runs[1]).b,Some(false));let mut cleared=runs[1].clone();
    let pr=cleared.content.iter_mut().find_map(|part|match part{XmlPart::Element(n)if w(n,"rPr")=>Some(n),_=>None}).unwrap();
    pr.content.retain(|part|!matches!(part,XmlPart::Element(n)if w(n,"b")));
    assert!(child(pr,"b").is_none());
    assert_eq!(effective(&styles,p,&cleared).b,Some(true));
}

#[test]
fn paragraph_and_character_collision_bindings_are_stable_when_source_order_reverses(){
    let a=source("336699","30","A","");let b=source("993366","26","B","");let mut expected=None;
    for pair in [[a.clone(),b.clone()],[b.clone(),a.clone()]]{
        let out=package(pair.into_iter().map(office).collect());let styles=tree(part(&out,"word/styles.xml"));let body=tree(part(&out,"word/document.xml"));let mut ids=BTreeSet::new();
        for node in nodes(&styles,WORD_NS,"style"){assert!(ids.insert(attr(node,"styleId").unwrap()));}
        let mut bindings=BTreeMap::new();for p in formula_paragraphs(&body){let run=nodes(p,MATH_NS,"r")[0];let label=run.text();let id=attr(child(child(run,"rPr").unwrap(),"rStyle").unwrap(),"val").unwrap();
            assert_eq!(effective(&styles,p,run).color.as_deref(),Some(if label=="A"{"336699"}else{"993366"}));
            bindings.insert(label,(selected(p).to_string(),id.to_string()));}
        assert_eq!(bindings.len(),2);assert_ne!(bindings["A"],bindings["B"]);if let Some(old)=&expected{assert_eq!(&bindings,old);}else{expected=Some(bindings);}
    }
}

#[test]
fn native_run_catalogue_does_not_change_ordinary_styles_body_or_unassociated_math(){
    let base=||{let mut blocks=crate::md_ast::parse("# Ordinary\n\nBody **bold**, *italic*, [link](#ordinary).\n\n> Quote.\n\n```text\ncode\n```\n").blocks;
        // Hold the already-approved document math policy fixed in both files.
        // This unassociated control has no Word paragraph or run provenance.
        blocks.push(office(format!(r#"<m:mathPr xmlns:m="{MATH_NS}"><m:dispDef m:val="0"/></m:mathPr><m:oMathPara xmlns:m="{MATH_NS}"><m:oMath><m:r><m:t>unassociated</m:t></m:r></m:oMath></m:oMathPara>"#)));
        blocks.push(Block::Math("x^2".into()));blocks};
    let before=package(base());let mut mixed=base();mixed.push(office(source("336699","30","a","")));let after=package(mixed);
    let old_styles=tree(part(&before,"word/styles.xml"));let styles=tree(part(&after,"word/styles.xml"));for node in nodes(&old_styles,WORD_NS,"style"){
        assert_eq!(crate::math_document::xml_semantics(node),crate::math_document::xml_semantics(style(&styles,attr(node,"styleId").unwrap())));}
    let old=tree(part(&before,"word/document.xml"));let new=tree(part(&after,"word/document.xml"));let old_ps=nodes(&old,WORD_NS,"p");let new_ps=nodes(&new,WORD_NS,"p");assert_eq!(new_ps.len(),old_ps.len()+1);
    for (a,b) in old_ps.iter().zip(&new_ps){assert_eq!(crate::math_document::xml_semantics(a),crate::math_document::xml_semantics(b));}
    let old_catalogue=child(&old_styles,"styles").unwrap();let catalogue=child(&styles,"styles").unwrap();
    assert_eq!(crate::math_document::xml_semantics(child(old_catalogue,"docDefaults").unwrap()),crate::math_document::xml_semantics(child(catalogue,"docDefaults").unwrap()));
    for path in ["word/settings.xml"]{assert_eq!(part(&before,path),part(&after,path));}
}

#[test]
fn unprepared_quote_hosts_keep_effective_run_values_and_do_not_publish_native_character_styles(){
    // A scoped Word pPr is already rejected by the nested-host preflight.
    // Use an admitted direct-run-only source to exercise this registry lane.
    let xml=format!(r#"<m:mathPr xmlns:m="{MATH_NS}"><m:dispDef m:val="0"/></m:mathPr><m:oMathPara xmlns:m="{MATH_NS}" xmlns:w="{WORD_NS}"><m:oMath><m:r><w:rPr><w:b w:val="off"/><w:i w:val="0"/><w:color w:val="336699"/><w:sz w:val="30"/></w:rPr><m:t>a</m:t></m:r></m:oMath></m:oMathPara>"#);
    let out=package(vec![Block::Quote(vec![office(xml)])]);
    assert_eq!(out.warns.iter().filter(|code|code.as_str()==crate::api_codes::WARN_MATH_RUN_STYLE_MATERIALIZED).count(),1);
    let styles=tree(part(&out,"word/styles.xml"));
    assert!(!nodes(&styles,WORD_NS,"style").iter().any(|node|attr(node,"styleId")==Some("Accent")));
    let body=tree(part(&out,"word/document.xml"));let p=formula_paragraphs(&body)[0];let first=nodes(p,MATH_NS,"r")[0];
    assert!(child(child(first,"rPr").unwrap(),"rStyle").is_none());
    assert_eq!(effective(&styles,p,first),Four{b:Some(false),i:Some(false),sz:Some("30".into()),color:Some("336699".into())});
}

#[test]
fn nonempty_source_run_defaults_keep_materialized_values_and_report_inheritance_loss(){
    let defaults="<w:docDefaults><w:rPrDefault><w:rPr><w:color w:val=\"FFFFFF\"/></w:rPr></w:rPrDefault></w:docDefaults>";
    let out=package(vec![office(source("336699","30","a",defaults))]);
    assert_eq!(out.warns.iter().filter(|code|code.as_str()==crate::api_codes::WARN_MATH_RUN_STYLE_MATERIALIZED).count(),1);
    let styles=tree(part(&out,"word/styles.xml"));let body=tree(part(&out,"word/document.xml"));let p=formula_paragraphs(&body)[0];
    assert_eq!(selected(p),"Equation");let first=nodes(p,MATH_NS,"r")[0];assert!(child(child(first,"rPr").unwrap(),"rStyle").is_none());
    assert_eq!(effective(&styles,p,first),Four{b:Some(false),i:Some(false),sz:Some("30".into()),color:Some("336699".into())});
}
