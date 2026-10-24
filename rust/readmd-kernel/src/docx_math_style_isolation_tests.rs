//! Actual package styles and an independent generated-style cascade.
//! This proves structural/effective declarations, not Word application layout.
use crate::{docx_writer::{build,DocxPackage},math_document::{XmlNode,XmlPart,WORD_NS,MATH_NS}};
use serde_json::{json,Value};
use std::collections::{BTreeMap,BTreeSet};

fn word(node:&XmlNode,local:&str)->bool{node.namespace==WORD_NS&&node.local()==local}
fn child<'a>(node:&'a XmlNode,local:&str)->Option<&'a XmlNode>{node.children().find(|node|word(node,local))}
fn attr<'a>(node:&'a XmlNode,local:&str)->Option<&'a str>{
    node.attributes.iter().find(|(key,_)|key.rsplit(':').next()==Some(local)
        &&node.attribute_namespaces.get(*key).is_some_and(|uri|uri==WORD_NS)).map(|(_,value)|value.as_str())
}
fn part<'a>(package:&'a DocxPackage,name:&str)->&'a str{
    std::str::from_utf8(&package.parts.iter().find(|(path,_)|path==name).expect(name).1).unwrap()
}
fn tree(xml:&str)->XmlNode{crate::math_document::parse_xml(xml).unwrap()}
fn styles(root:&XmlNode)->&XmlNode{child(root,"styles").unwrap()}
fn style<'a>(root:&'a XmlNode,id:&str)->&'a XmlNode{
    styles(root).children().find(|node|word(node,"style")&&attr(node,"styleId")==Some(id)).unwrap()
}
fn defaults<'a>(root:&'a XmlNode,kind:&str,property:&str)->Option<&'a XmlNode>{
    child(styles(root),"docDefaults").and_then(|node|child(node,kind)).and_then(|node|child(node,property))
}
fn canonical(node:&XmlNode)->Value{
    let attributes=node.attributes.iter().filter(|(key,_)|key.as_str()!="xmlns"&&!key.starts_with("xmlns:"))
        .map(|(key,value)|(format!("{}:{}",node.attribute_namespaces.get(key).map(String::as_str).unwrap_or(""),key.rsplit(':').next().unwrap()),value))
        .collect::<BTreeMap<_,_>>();
    json!([node.namespace,node.local(),attributes,node.content.iter().map(|part|match part{
        XmlPart::Element(node)=>canonical(node),XmlPart::Text(text)=>json!({"text":text}),
        XmlPart::Comment(text)=>json!({"comment":text}),XmlPart::Instruction(text)=>json!({"instruction":text}),
    }).collect::<Vec<_>>()])
}

// Generated spacing/ind declarations merge by individual attribute. Other
// properties retain whole source subtrees; their declaration chains are also
// compared directly below, so no general Word pPr cascade is claimed here.
type Properties=BTreeMap<String,Value>;
fn overlay(properties:&mut Properties,ppr:Option<&XmlNode>)->Result<(),String>{
    let Some(ppr)=ppr else{return Ok(())};let mut seen=BTreeSet::new();
    for node in ppr.children(){
        if node.namespace!=WORD_NS||!seen.insert(node.local()){return Err("non-Word/duplicate generated pPr".into());}
        if node.local()=="pStyle"{continue;}
        if matches!(node.local(),"spacing"|"ind"){
            if node.children().next().is_some()||!node.text().trim().is_empty(){return Err("unexpected compound pPr source".into());}
            let previous=properties.entry(node.local().into()).or_insert_with(||json!({}));
            let map=previous.as_object_mut().ok_or("property kind mismatch")?;
            for(key,value)in &node.attributes{
                if key=="xmlns"||key.starts_with("xmlns:"){continue;}
                if !node.attribute_namespaces.get(key).is_some_and(|uri|uri==WORD_NS){return Err("foreign generated scalar attribute".into());}
                map.insert(key.rsplit(':').next().unwrap().into(),json!(value));
            }
        }else{properties.insert(node.local().into(),canonical(node));}
    }Ok(())
}
fn chain<'a>(root:&'a XmlNode,id:&str)->Result<Vec<&'a XmlNode>,String>{
    let mut out=Vec::new();let mut seen=BTreeSet::new();let mut id=Some(id.to_string());
    while let Some(current)=id{
        if out.len()>=128||!seen.insert(current.clone()){return Err("style chain cycle/budget".into());}
        let node=styles(root).children().find(|node|word(node,"style")&&attr(node,"styleId")==Some(current.as_str())).ok_or("missing style")?;
        if attr(node,"type")!=Some("paragraph"){return Err("non-paragraph chain".into());}
        id=child(node,"basedOn").map(|node|attr(node,"val").ok_or("missing basedOn val").map(str::to_string)).transpose()?;
        out.push(node);
    }out.reverse();Ok(out)
}
fn effective(root:&XmlNode,id:&str)->Result<Properties,String>{
    let mut out=Properties::new();overlay(&mut out,defaults(root,"pPrDefault","pPr"))?;
    for node in chain(root,id)?{overlay(&mut out,child(node,"pPr"))?;}Ok(out)
}
fn ids(root:&XmlNode,kind:&str)->BTreeSet<String>{
    styles(root).children().filter(|node|word(node,"style")&&attr(node,"type")==Some(kind))
        .map(|node|attr(node,"styleId").unwrap().to_string()).collect()
}
fn ordinary_equivalent(old:&XmlNode,new:&XmlNode)->Result<(),String>{
    let old_ids=ids(old,"paragraph");let mut new_ids=ids(new,"paragraph");new_ids.remove("ReadMDMath");
    if old_ids!=new_ids{return Err("ordinary paragraph catalogue changed".into());}
    if defaults(old,"rPrDefault","rPr").map(canonical)!=defaults(new,"rPrDefault","rPr").map(canonical){return Err("run docDefaults changed".into());}
    for id in &old_ids{
        let old_style=style(old,id);let new_style=style(new,id);
        let mut old_metadata=old_style.clone();old_metadata.content.retain(|part|!matches!(part,XmlPart::Element(node)if word(node,"pPr")));
        let mut new_metadata=new_style.clone();new_metadata.content.retain(|part|!matches!(part,XmlPart::Element(node)if word(node,"pPr")));
        if canonical(&old_metadata)!=canonical(&new_metadata){return Err(format!("non-pPr metadata/run changed: {id}"));}
        if id!="Normal"{
            if child(old_style,"pPr").map(canonical)!=child(new_style,"pPr").map(canonical){return Err(format!("ordinary direct pPr changed: {id}"));}
            if !chain(new,id)?.iter().any(|node|attr(node,"styleId")==Some("Normal")){return Err(format!("ordinary chain misses Normal: {id}"));}
        }
        if effective(old,id)?!=effective(new,id)?{return Err(format!("effective ordinary pPr changed: {id}"));}
    }
    if ids(old,"character")!=ids(new,"character"){return Err("character catalogue changed".into());}
    for id in ids(old,"character"){
        if canonical(style(old,&id))!=canonical(style(new,&id)){return Err(format!("character style changed: {id}"));}
    }Ok(())
}
fn paragraphs<'a>(node:&'a XmlNode,out:&mut Vec<&'a XmlNode>){
    if word(node,"p"){out.push(node);}for node in node.children(){paragraphs(node,out);}
}
fn actual_effective(root:&XmlNode,paragraph:&XmlNode)->Properties{
    let ppr=child(paragraph,"pPr");let id=ppr.and_then(|node|child(node,"pStyle")).and_then(|node|attr(node,"val")).unwrap_or("Normal");
    let mut out=effective(root,id).unwrap();overlay(&mut out,ppr).unwrap();out
}
fn assert_math_isolated(root:&XmlNode){
    let math=style(root,"ReadMDMath");assert!(child(math,"basedOn").is_none());
    assert!(!matches!(attr(math,"default"),Some("1"|"true"|"on")));
    assert!(effective(root,"ReadMDMath").unwrap().is_empty(),"math style inherited body pPr");
    assert!(defaults(root,"pPrDefault","pPr").is_none_or(|ppr|ppr.children().next().is_none()));
    let normal=child(style(root,"Normal"),"pPr").unwrap();
    let names=normal.children().map(XmlNode::local).collect::<Vec<_>>();
    assert_eq!(names.iter().filter(|name|**name=="spacing").count(),1);
    let at=|name:&str|names.iter().position(|value|*value==name).unwrap();
    assert!(at("widowControl")<at("spacing")&&at("spacing")<at("jc"),"{names:?}");
}
fn mutate_normal_line(root:&mut XmlNode){
    fn walk(node:&mut XmlNode)->bool{
        if word(node,"style")&&attr(node,"styleId")==Some("Normal"){
            for part in &mut node.content{if let XmlPart::Element(ppr)=part{if word(ppr,"pPr"){
                for part in &mut ppr.content{if let XmlPart::Element(spacing)=part{if word(spacing,"spacing"){
                    spacing.attributes.insert("w:line".into(),"1".into());return true;
                }}}
            }}}return false;
        }
        for part in &mut node.content{if let XmlPart::Element(child)=part{if walk(child){return true;}}}false
    }assert!(walk(root));
}
fn source(numbered:bool)->String{
    let prefix=if numbered{
        let config:crate::cross_references::NumberingConfig=serde_json::from_value(json!({"headings":{"levels":[{},{},{},{},{},{}]}})).unwrap();
        format!("{}\n\n",crate::cross_references::numbering_comment(&config).unwrap())
    }else{String::new()};
    format!("{prefix}# Ordinary heading\n\nOrdinary body with [link](https://example.test/ordinary) and notes[^f][^e].\n\n> Quote paragraph.\n\n- List paragraph.\n\n| Header |\n| --- |\n| Cell |\n\n```text\nCode paragraph.\n```\n\n[^f]: Footnote paragraph.\n\n[^e]: Endnote paragraph.\n")
}
fn package(doc:&crate::md_ast::Document,options:&Value)->DocxPackage{
    let calls=std::cell::Cell::new(0);let loader=|_:&str|{calls.set(calls.get()+1);None};
    let package=build(doc,options,"Style isolation",&loader);
    assert!(!package.parts.is_empty(),"{:?}",package.warns);assert!(package.warns.is_empty(),"{:?}",package.warns);assert_eq!(calls.get(),0);package
}

#[test]
fn display_math_style_isolates_body_defaults_and_keeps_every_ordinary_style_effective(){
    for numbered in [false,true]{for typography in [
        json!({}),
        json!({"size":13.5,"spacing":7.25,"lineHeight":1.3,"align":"justify","firstLineIndent":3.2,"widowControl":false,"keepLines":true}),
        json!({"size":12.,"spacing":9.,"lineSpacing":{"mode":"exact","pt":22.5},"align":"right","widowControl":true}),
    ]{
        let options=json!({"typography":typography,"footnotes":{"kinds":{"footnote":{"size":9.5},"endnote":{"size":10.5}},"kindByLabel":{"f":"footnote","e":"endnote"}}});
        let input=source(numbered);let doc=crate::md_ast::parse(&input);let old=package(&doc,&options);let old_tree=tree(part(&old,"word/styles.xml"));
        assert!(!ids(&old_tree,"paragraph").contains("ReadMDMath"));
        assert!(defaults(&old_tree,"pPrDefault","pPr").and_then(|ppr|child(ppr,"spacing")).is_some());
        let required=["Normal","BodyText","Heading1","Heading6","TOCHeading","TOC6","Title","Subtitle","Code","Quote","ListParagraph","TableText","Caption","FootnoteText","EndnoteText","Header","Footer"];
        for id in required{assert!(ids(&old_tree,"paragraph").contains(id),"missing catalog coverage: {id}");}
        assert_eq!(ids(&old_tree,"paragraph").contains("ReadMDExampleHeading6"),numbered);
        let mut with_inline=doc.clone();
        let Some(crate::md_ast::Block::Paragraph(values))=with_inline.blocks.iter_mut().find(|block|matches!(block,crate::md_ast::Block::Paragraph(_)))else{panic!("missing ordinary paragraph")};
        values.push(crate::md_ast::Inline::Math("x".into()));
        let inline=package(&with_inline,&options);assert_eq!(canonical(&old_tree),canonical(&tree(part(&inline,"word/styles.xml"))),"inline-only formula changed paragraph style mode");
        for office in [false,true]{
            let mut with_math=doc.clone();
            if office{
                let xml=format!(r#"<w:pPr xmlns:w="{WORD_NS}"><w:ind w:left="0" w:right="240"/><w:jc w:val="right"/></w:pPr><m:mathPr xmlns:m="{MATH_NS}"><m:dispDef m:val="0"/></m:mathPr><m:oMathPara xmlns:m="{MATH_NS}"><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath></m:oMathPara>"#);
                with_math.blocks.push(crate::md_ast::Block::OfficeMath{latex:"x".into(),omml:xml});
            }else{with_math.blocks.push(crate::md_ast::Block::Math("x^2".into()));}
            let new=package(&with_math,&options);let new_tree=tree(part(&new,"word/styles.xml"));
            ordinary_equivalent(&old_tree,&new_tree).unwrap();assert_math_isolated(&new_tree);
            // Detect an intentionally wrong moved line value. This oracle
            // must not accept merely that spacing now lives under Normal.
            let mut broken=new_tree.clone();mutate_normal_line(&mut broken);assert!(ordinary_equivalent(&old_tree,&broken).is_err());
            for(path,bytes)in &old.parts{
                if path!="word/document.xml"&&!path.starts_with("word/header")&&!path.starts_with("word/footer")&&!matches!(path.as_str(),"word/footnotes.xml"|"word/endnotes.xml"){continue;}
                let before=tree(std::str::from_utf8(bytes).unwrap());let after=tree(part(&new,path));let(mut old_pars,mut new_pars)=(Vec::new(),Vec::new());paragraphs(&before,&mut old_pars);paragraphs(&after,&mut new_pars);
                assert_eq!(new_pars.len(),old_pars.len()+if path=="word/document.xml"{1}else{0});
                for(old_par,new_par)in old_pars.iter().zip(&new_pars){
                    assert_eq!(child(old_par,"pPr").map(canonical),child(new_par,"pPr").map(canonical),"direct paragraph changed: {path}");
                    assert_eq!(actual_effective(&old_tree,old_par),actual_effective(&new_tree,new_par),"actual paragraph changed: {path}");
                }
            }
            let body=tree(part(&new,"word/document.xml"));let mut pars=Vec::new();paragraphs(&body,&mut pars);let formula=pars.last().unwrap();
            let ppr=child(formula,"pPr").unwrap();assert_eq!(attr(child(ppr,"pStyle").unwrap(),"val"),Some("ReadMDMath"));
            assert!(formula.children().any(|node|node.namespace==MATH_NS&&matches!(node.local(),"oMathPara"|"oMath")));
            let actual=actual_effective(&new_tree,formula);assert!(!actual.contains_key("spacing")&&!actual.contains_key("widowControl"));
            if office{
                assert_eq!(attr(child(ppr,"jc").unwrap(),"val"),Some("right"));let ind=child(ppr,"ind").unwrap();assert_eq!(attr(ind,"left"),Some("0"));assert_eq!(attr(ind,"right"),Some("240"));
            }
        }
    }}
}
