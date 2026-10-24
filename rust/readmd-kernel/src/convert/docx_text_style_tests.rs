use super::*;
use crate::md_ast::{self, Block, Document, Inline};
use serde_json::{json, Value};
use std::path::Path;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const P: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

fn package(body: &str, styles: &str, footnotes: &str, endnotes: &str, extra_rels: &str) -> Vec<(String, Vec<u8>)> {
    let mut parts = vec![
        ("[Content_Types].xml".into(), br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/></Types>"#.to_vec()),
        ("_rels/.rels".into(), format!(r#"<Relationships xmlns="{P}"><Relationship Id="main" Type="{R}/officeDocument" Target="word/document.xml"/></Relationships>"#).into_bytes()),
        ("word/document.xml".into(), format!(r#"<w:document xmlns:w="{W}" xmlns:r="{R}"><w:body>{body}</w:body></w:document>"#).into_bytes()),
    ];
    let mut rels=extra_rels.to_string();
    for (name,content) in [("styles",styles),("footnotes",footnotes),("endnotes",endnotes)] {
        if content.is_empty(){continue;}
        rels.push_str(&format!(r#"<Relationship Id="{name}" Type="{R}/{name}" Target="{name}.xml"/>"#));
        parts.push((format!("word/{name}.xml"),format!(r#"<w:{name} xmlns:w="{W}" xmlns:r="{R}">{content}</w:{name}>"#).into_bytes()));
    }
    parts.push(("word/_rels/document.xml.rels".into(),format!(r#"<Relationships xmlns="{P}">{rels}</Relationships>"#).into_bytes()));
    parts
}
fn import(parts:&[(String,Vec<u8>)])->std::result::Result<String,String>{
    super::super::docx_convert(parts,Path::new("native-author.docx"),false)
}
fn output(source:&str,request:&Value)->crate::docx_writer::DocxPackage{
    let options=crate::publication_profile::resolve_for_format("docx",source,request).unwrap();
    let style=crate::export_styles::sanitize_options(Some(options.values()));
    let (doc,warnings)=crate::mdexport::publication_document(source,"",options.values()).unwrap();
    assert!(warnings.is_empty(),"{warnings:?}");
    let package=crate::docx_writer::build_with_lines(&doc,&style,"Author.md",&|_|None,options.line_context());
    assert!(!package.parts.is_empty(),"{:?}",package.warns);
    assert!(package.warns.is_empty(),"{:?}",package.warns);
    assert!(package.parts.iter().all(|(_,bytes)|!String::from_utf8_lossy(bytes).contains(crate::docx_local_style::AUTHOR_AXES_ATTRIBUTE)),"private writer authority leaked into package");
    package
}
fn output_minimal(source:&str)->crate::docx_writer::DocxPackage{
    let fallback=crate::export_styles::preset_style("minimal");
    let resolved=crate::publication_profile::resolve_with_preset(source,&fallback,None,&json!({})).unwrap();
    output(source,resolved.values())
}
fn root(parts:&[(String,Vec<u8>)],name:&str)->XmlNode{
    docx_notes::xml(docx_notes::unique(parts,name).unwrap()).unwrap()
}
fn child<'a>(node:&'a XmlNode,name:&str)->Option<&'a XmlNode>{node.children.iter().find(|node|node.name==name)}
fn all_runs(node:&XmlNode)->Vec<&XmlNode>{std::iter::once(node).chain(node.descendants()).filter(|node|node.name=="w:r").collect()}
fn size(run:&XmlNode,name:&str)->Option<u32>{child(run,"w:rPr").and_then(|pr|child(pr,name)).and_then(|node|node.attr("w:val")).map(|raw|raw.parse().unwrap())}
fn flag(run:&XmlNode,name:&str)->Option<bool>{child(run,"w:rPr").and_then(|pr|child(pr,name)).map(|node|!matches!(node.attr("w:val"),Some("0"|"off"|"false")))}
fn text(run:&XmlNode)->String{super::super::docx_run_text(run)}
fn run<'a>(root:&'a XmlNode,label:&str)->&'a XmlNode{
    let runs=all_runs(root).into_iter().filter(|node|text(node)==label).collect::<Vec<_>>();assert_eq!(runs.len(),1,"{label}: {} runs",runs.len());runs[0]
}
fn pair(root:&XmlNode,label:&str)->(Option<u32>,Option<u32>){let run=run(root,label);(size(run,"w:sz"),size(run,"w:szCs"))}
fn effective_run(parts:&[(String,Vec<u8>)],label:&str)->notes::Metrics{
    let document=root(parts,"word/document.xml");let body=child(&document,"w:body").unwrap();let rels=docx_notes::relationships(parts,"word/document.xml").unwrap();let notes=docx_notes::Notes::load(parts,body,&rels).unwrap();let index=RunStyleIndex::build(parts,&rels,body,&notes).unwrap();index.run("word/document.xml",run(&document,label)).unwrap()
}
fn default_pair(parts:&[(String,Vec<u8>)])->(Option<u32>,Option<u32>){
    let styles=root(parts,"word/styles.xml");let properties=child(child(child(&styles,"w:docDefaults").unwrap(),"w:rPrDefault").unwrap(),"w:rPr").unwrap();
    let value=|name|child(properties,name).and_then(|node|node.attr("w:val")).map(|raw|raw.parse().unwrap());(value("w:sz"),value("w:szCs"))
}
fn styled(value:Value,children:Vec<Inline>)->Inline{Inline::AuthorStyled{style:AuthorTextStyle::from_value(&value).unwrap(),children}}
fn word(entries:&[(String,Vec<u8>)])->String{String::from_utf8(docx_notes::unique(entries,"word/document.xml").unwrap().to_vec()).unwrap()}
fn alter(parts:&mut[(String,Vec<u8>)],name:&str,from:&str,to:&str){let data=&mut parts.iter_mut().find(|(part,_)|part==name).unwrap().1;let old=String::from_utf8(data.clone()).unwrap();assert!(old.contains(from));*data=old.replacen(from,to,1).into_bytes();}

#[derive(Debug)]
struct WordField{code:String,cache:String,nested:Vec<WordField>}
fn word_fields(root:&XmlNode)->Vec<WordField>{
    struct Open{field:WordField,separated:bool}
    fn finish(field:WordField,stack:&mut[Open],out:&mut Vec<WordField>){
        if let Some(parent)=stack.last_mut(){if parent.separated{parent.field.cache.push_str(&field.cache);}else{parent.field.code.push_str(" {FIELD} ");}parent.field.nested.push(field);}else{out.push(field);}
    }
    fn walk(node:&XmlNode,stack:&mut Vec<Open>,out:&mut Vec<WordField>){
        if node.name=="w:fldSimple"{
            let depth=stack.len();stack.push(Open{field:WordField{code:node.attr("w:instr").expect("simple field instruction").into(),cache:String::new(),nested:Vec::new()},separated:true});
            for child in &node.children{walk(child,stack,out);}assert_eq!(stack.len(),depth+1,"unbalanced nested simple field");let open=stack.pop().unwrap();finish(open.field,stack,out);return;
        }
        if node.name=="w:fldChar"{match node.attr("w:fldCharType").unwrap(){
            "begin"=>stack.push(Open{field:WordField{code:String::new(),cache:String::new(),nested:Vec::new()},separated:false}),
            "separate"=>{let open=stack.last_mut().expect("field separator without begin");assert!(!open.separated,"duplicate field separator");open.separated=true;},
            "end"=>{let open=stack.pop().expect("field end without begin");assert!(open.separated,"field without result boundary");finish(open.field,stack,out);},
            other=>panic!("unknown field character {other}"),
        }return;}
        if node.name=="w:instrText"{let open=stack.last_mut().expect("instruction outside field");assert!(!open.separated,"instruction in field result");open.field.code.push_str(&node.text);}
        if node.name=="w:t"{if let Some(open)=stack.last_mut(){if open.separated{open.field.cache.push_str(&node.text);}}}
        for child in &node.children{walk(child,stack,out);}
    }
    let mut out=Vec::new();let mut stack=Vec::new();walk(root,&mut stack,&mut out);assert!(stack.is_empty(),"unterminated Word field");out
}
fn automatic_table_id(source:&str)->String{
    let doc=md_ast::parse_prepared(source,&crate::publication_rich::Projection::new(source)).unwrap();
    let captions=doc.blocks.iter().filter_map(|block|if let Block::Caption{label,..}=block.unstyled(){Some(label)}else{None}).collect::<Vec<_>>();assert_eq!(captions.len(),1,"{source}");let label=captions[0];assert_eq!(label.kind,"table");assert_eq!(label.sequence(),"Table");assert_eq!(label.number,4);let numbering=label.numbering.as_ref().expect("automatic caption rule");assert_eq!((numbering.start,numbering.ordinal),(4,1));label.id.clone()
}
struct VisibleRun<'a>{run:&'a XmlNode,text:String}
fn visible_runs<'a>(nodes:&'a[XmlNode])->std::result::Result<Vec<VisibleRun<'a>>,String>{
    fn walk<'a>(node:&'a XmlNode,run:Option<&'a XmlNode>,fields:&mut Vec<bool>,out:&mut Vec<VisibleRun<'a>>)->std::result::Result<(),String>{
        if node.name=="w:fldSimple"{let depth=fields.len();fields.push(true);for child in &node.children{walk(child,run,fields,out)?;}if fields.len()!=depth+1{return Err("unbalanced simple field".into());}fields.pop();return Ok(());}
        if node.name=="w:fldChar"{match node.attr("w:fldCharType"){
            Some("begin")=>fields.push(false),Some("separate")=>{let result=fields.last_mut().ok_or("field separator without begin")?;if *result{return Err("duplicate field separator".into());}*result=true;},
            Some("end")=>{if fields.pop()!=Some(true){return Err("field end without separated result".into());}},_=>return Err("unknown field marker".into()),
        }return Ok(());}
        let run=if node.name=="w:r"{Some(node)}else{run};
        let value=match node.name.as_str(){"w:t"=>Some(node.text.as_str()),"w:tab"=>Some("\t"),"w:br"|"w:cr"=>Some("\n"),_=>None};
        if let Some(value)=value.filter(|value|!value.is_empty()&&fields.iter().all(|result|*result)){
            let run=run.ok_or("visible caption content outside a run")?;
            if let Some(last)=out.last_mut().filter(|last|std::ptr::eq(last.run,run)){last.text.push_str(value);}else{out.push(VisibleRun{run,text:value.into()});}
        }
        for child in &node.children{walk(child,run,fields,out)?;}Ok(())
    }
    let mut out=Vec::new();let mut fields=Vec::new();for node in nodes{walk(node,None,&mut fields,&mut out)?;}if !fields.is_empty(){return Err("unterminated caption field".into());}Ok(out)
}
struct CaptionRuns<'a>{start:usize,begin_field:usize,end_field:usize,end:usize,label:Vec<VisibleRun<'a>>,tail:Vec<VisibleRun<'a>>}
fn caption_runs<'a>(xml:&'a XmlNode,id:&str,label_text:&str,tail_text:&str)->std::result::Result<CaptionRuns<'a>,String>{
    let bookmark=crate::docx_writer::bookmark_name(id);
    let starts=xml.descendants().into_iter().filter(|node|node.name=="w:bookmarkStart"&&node.attr("w:name")==Some(bookmark.as_str())).collect::<Vec<_>>();
    if starts.len()!=1{return Err("caption bookmark must be unique".into());}let bookmark_id=starts[0].attr("w:id").ok_or("caption bookmark has no id")?;
    if xml.descendants().iter().filter(|node|node.name=="w:bookmarkEnd"&&node.attr("w:id")==Some(bookmark_id)).count()!=1{return Err("caption bookmark end must be unique".into());}
    let caption=xml.descendants().into_iter().find(|node|node.name=="w:p"&&node.children.iter().any(|child|child.name=="w:bookmarkStart"&&child.attr("w:name")==Some(bookmark.as_str()))).ok_or("missing caption paragraph")?;
    let start=caption.children.iter().position(|node|node.name=="w:bookmarkStart"&&node.attr("w:name")==Some(bookmark.as_str())).ok_or("missing caption start")?;
    let end=caption.children.iter().position(|node|node.name=="w:bookmarkEnd"&&node.attr("w:id")==Some(bookmark_id)).ok_or("caption bookmark crosses paragraph")?;
    let field=|node:&XmlNode,kind:&str|node.name=="w:fldSimple"||node.descendants().iter().any(|child|child.name=="w:fldChar"&&child.attr("w:fldCharType")==Some(kind));
    let begin_field=caption.children.iter().position(|node|field(node,"begin")).ok_or("missing caption field begin")?;
    let end_field=caption.children.iter().rposition(|node|field(node,"end")).ok_or("missing caption field end")?;
    if !(start<begin_field&&begin_field<=end_field&&end_field<end){return Err("caption field must lie inside label bookmark".into());}
    let label=visible_runs(&caption.children[start+1..end])?;let tail=visible_runs(&caption.children[end+1..])?;
    if label.iter().map(|part|part.text.as_str()).collect::<String>()!=label_text{return Err("caption label visible text differs".into());}
    if tail.iter().map(|part|part.text.as_str()).collect::<String>()!=tail_text{return Err("caption tail visible text differs".into());}
    Ok(CaptionRuns{start,begin_field,end_field,end,label,tail})
}
type AuthorAxes=(Option<u32>,Option<u32>,Option<bool>,Option<bool>);
fn axes(metrics:&notes::Metrics)->AuthorAxes{(metrics.size,metrics.size_cs,metrics.cs,metrics.rtl)}
fn assert_caption_run_axes(parts:&[(String,Vec<u8>)],id:&str,label_text:&str,tail_text:&str,caption_axes:AuthorAxes,reference_axes:Option<AuthorAxes>){
    let document=root(parts,"word/document.xml");let body=child(&document,"w:body").unwrap();let rels=docx_notes::relationships(parts,"word/document.xml").unwrap();let notes=docx_notes::Notes::load(parts,body,&rels).unwrap();let index=RunStyleIndex::build(parts,&rels,body,&notes).unwrap();
    let runs=caption_runs(&document,id,label_text,tail_text).unwrap_or_else(|reason|panic!("{reason}: {}",word(parts)));
    for (scope,contributions) in [("caption label",runs.label),("caption tail",runs.tail)]{assert!(!contributions.is_empty(),"{scope}");for part in contributions{let actual=index.run("word/document.xml",part.run).unwrap();assert_eq!(axes(&actual),caption_axes,"{scope} contributor {:?}",part.text);}}
    if let Some(expected)=reference_axes{let bookmark=crate::docx_writer::bookmark_name(id);let references=document.descendants().into_iter().filter(|node|node.name=="w:fldSimple"&&node.attr("w:instr").is_some_and(|code|code.split_whitespace().collect::<Vec<_>>()==["REF",bookmark.as_str(),"\\h"])).collect::<Vec<_>>();assert_eq!(references.len(),1,"the same target must have one live writer REF");let parts=visible_runs(std::slice::from_ref(references[0])).unwrap();assert_eq!(parts.iter().map(|part|part.text.as_str()).collect::<String>(),label_text);assert!(!parts.is_empty());for part in parts{let actual=index.run("word/document.xml",part.run).unwrap();assert_eq!(axes(&actual),expected,"REF contributor {:?}",part.text);}}
}
fn assert_table_offset_field_identity(xml:&XmlNode,id:&str){
    let fields=word_fields(xml);assert_eq!(fields.len(),2,"{fields:?}");
    let number=fields.iter().find(|field|field.code.trim_start().starts_with('=')).expect("live outer formula field");assert_eq!(number.code.split_whitespace().collect::<Vec<_>>(),["=","{FIELD}","+","3","\\*","ARABIC"]);assert_eq!(number.cache,"4");assert_eq!(number.nested.len(),1);let sequence=&number.nested[0];assert_eq!(sequence.code.split_whitespace().collect::<Vec<_>>(),["SEQ","Table"]);assert_eq!(sequence.cache,"1");assert!(sequence.nested.is_empty());
    let bookmark=crate::docx_writer::bookmark_name(id);let reference=fields.iter().find(|field|field.code.split_whitespace().next()==Some("REF")).expect("live REF field");assert_eq!(reference.code.split_whitespace().collect::<Vec<_>>(),["REF",bookmark.as_str(),"\\h"]);assert_eq!(reference.cache,"Table 4");assert!(reference.nested.is_empty());
    let runs=caption_runs(xml,id,"Table 4"," — NativeTitle").unwrap_or_else(|reason|panic!("{reason}"));
    assert!(runs.start<runs.begin_field&&runs.begin_field<runs.end_field&&runs.end_field<runs.end,"REF bookmark must span the live label field and exclude the complete editable title");
}

#[test]
fn native_repeated_text_binds_actual_runs_and_independent_sizes_across_two_exports(){
    let body=r#"<w:p><w:r><w:rPr><w:sz w:val="60"/><w:szCs w:val="18"/><w:cs/><w:rtl w:val="0"/></w:rPr><w:t>Same</w:t></w:r><w:r><w:rPr><w:sz w:val="20"/><w:szCs w:val="41"/><w:cs w:val="0"/><w:rtl/></w:rPr><w:t>Same</w:t></w:r></w:p>"#;
    let parts=package(body,"","","","");let before=parts.clone();let source=import(&parts).unwrap();assert_eq!(parts,before);
    let source_nodes=crate::publication_inline_text::get(&source).unwrap().nodes.into_iter().filter(|node|node.kind=="span").collect::<Vec<_>>();assert_eq!(source_nodes.len(),2);assert_eq!(source_nodes[0].style.size,CascadeSize::Points(HalfPoints::new(60).unwrap()));assert_eq!(source_nodes[1].style.size_cs,CascadeSize::Points(HalfPoints::new(41).unwrap()));
    assert!(source.contains("data-size-pt=\"30\""));assert!(source.contains("data-size-cs-pt=\"20.5\""));
    let first=output(&source,&json!({}));let reopened=import(&first.parts).unwrap();let second=output(&reopened,&json!({}));
    for package in [&first,&second]{let xml=root(&package.parts,"word/document.xml");let repeated=all_runs(&xml).into_iter().filter(|r|text(r)=="Same").collect::<Vec<_>>();assert_eq!(repeated.len(),2);
        assert_eq!((size(repeated[0],"w:sz"),size(repeated[0],"w:szCs"),flag(repeated[0],"w:cs"),flag(repeated[0],"w:rtl")),(Some(60),Some(18),Some(true),Some(false)));
        assert_eq!((size(repeated[1],"w:sz"),size(repeated[1],"w:szCs"),flag(repeated[1],"w:cs"),flag(repeated[1],"w:rtl")),(Some(20),Some(41),Some(false),Some(true)));
    }
    // Source editing is through the shared typed style model, never an XML payload.
    assert!(!source.contains("<w:r"));
}

#[test]
fn inherited_document_paragraph_character_and_direct_values_use_one_cache(){
    let styles=r#"<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/><w:szCs w:val="24"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:styleId="Base"><w:rPr><w:sz w:val="26"/><w:cs/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Body"><w:basedOn w:val="Base"/><w:rPr><w:szCs w:val="30"/></w:rPr></w:style><w:style w:type="character" w:styleId="CharBase"><w:rPr><w:sz w:val="40"/></w:rPr></w:style><w:style w:type="character" w:styleId="Char"><w:basedOn w:val="CharBase"/><w:rPr><w:szCs w:val="44"/></w:rPr></w:style>"#;
    let body=r#"<w:p><w:pPr><w:pStyle w:val="Body"/></w:pPr><w:r><w:t>Inherited</w:t></w:r><w:r><w:rPr><w:rStyle w:val="Char"/></w:rPr><w:t>Character</w:t></w:r><w:r><w:rPr><w:rStyle w:val="Char"/><w:sz w:val="61"/><w:cs w:val="0"/></w:rPr><w:t>Direct</w:t></w:r><w:r><w:t>InheritedAgain</w:t></w:r></w:p>"#;
    let parts=package(body,styles,"","","");let document=root(&parts,"word/document.xml");let rels=docx_notes::relationships(&parts,"word/document.xml").unwrap();let body=child(&document,"w:body").unwrap();let notes=docx_notes::Notes::load(&parts,body,&rels).unwrap();
    let index=RunStyleIndex::build(&parts,&rels,body,&notes).unwrap();assert_eq!(index.stats.parsed_parts,1);assert_eq!(index.stats.resolved_styles,4);
    let source=import(&parts).unwrap();let out=output(&source,&json!({}));let xml=root(&out.parts,"word/document.xml");
    assert_eq!(pair(&xml,"Inherited"),(Some(26),Some(30)));assert_eq!(pair(&xml,"InheritedAgain"),(Some(26),Some(30)));assert_eq!(pair(&xml,"Character"),(Some(40),Some(44)));assert_eq!(pair(&xml,"Direct"),(Some(61),Some(44)));
    assert_eq!(flag(run(&xml,"Inherited"),"w:cs"),Some(true));assert_eq!(flag(run(&xml,"Direct"),"w:cs"),Some(false));
}

#[test]
fn equivalent_body_defaults_stay_plain_but_heading_paragraph_and_direct_sizes_stay_explicit(){
    let styles=r#"<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/><w:szCs w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:styleId="LargeBody"><w:name w:val="Body"/><w:rPr><w:sz w:val="26"/><w:szCs w:val="30"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="SmallHeading"><w:name w:val="heading 1"/></w:style><w:style w:type="paragraph" w:styleId="DefaultHeading"><w:name w:val="heading 1"/><w:rPr><w:sz w:val="40"/><w:szCs w:val="40"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/></w:style>"#;
    let body=r#"<w:p><w:r><w:t>BEFORE</w:t></w:r><w:r><w:t xml:space="preserve"> TAIL</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="LargeBody"/></w:pPr><w:r><w:t>Paragraph</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="SmallHeading"/></w:pPr><w:r><w:t>Heading</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="DefaultHeading"/></w:pPr><w:r><w:t>DefaultHeading</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="Title"/></w:pPr><w:r><w:t>Title</w:t></w:r></w:p><w:p><w:r><w:rPr><w:sz w:val="22"/></w:rPr><w:t>ExplicitDefault</w:t></w:r></w:p><w:p><w:r><w:t>*[@42]*</w:t></w:r></w:p>"#;
    let source=import(&package(body,styles,"","","")).unwrap();assert_eq!(crate::publication_profile::from_source(&source).unwrap().options["typography"]["size"],11.);assert!(md_ast::source_without_front_matter(&source).trim_start_matches(['\r','\n']).starts_with("BEFORE TAIL\n\n"),"{source}");assert!(source.contains("# DefaultHeading\n"),"{source}");assert!(source.contains("data-size-pt=\"11\">ExplicitDefault</span>"),"an actual direct declaration remains explicit: {source}");
    let out=output(&source,&json!({}));let xml=root(&out.parts,"word/document.xml");assert_eq!(pair(&xml,"Paragraph"),(Some(26),Some(30)));assert_eq!(pair(&xml,"Heading"),(Some(22),Some(22)));assert_eq!(pair(&xml,"Title"),(Some(22),Some(22)));assert_eq!(pair(&xml,"DefaultHeading"),(None,None));assert_eq!(pair(&xml,"ExplicitDefault"),(Some(22),None));assert_eq!(text(run(&xml,"*[@42]*")),"*[@42]*","native literal syntax must not create a citation or emphasis");
    let styles=root(&out.parts,"word/styles.xml");let heading=styles.children.iter().find(|node|node.name=="w:style"&&node.attr("w:styleId")==Some("Heading1")).unwrap();let properties=child(heading,"w:rPr").unwrap();assert_eq!(child(properties,"w:sz").unwrap().attr("w:val"),Some("40"));assert_eq!(child(properties,"w:szCs").unwrap().attr("w:val"),Some("40"));
}

#[test]
fn native_note_default_postprocessing_does_not_materialize_author_overrides_or_reject_code(){
    let source="Body[^n].\n\n[^n]: First **strong** note.\n\n    > Quoted evidence.\n\n    | Item | Value |\n    |---|---|\n    | Matrix | 98 |\n\n    ```rust\n    let evidence = 98;\n    ```\n";
    let original=output(source,&json!({}));let imported=import(&original.parts).unwrap();for expected in ["First **strong** note.","> Quoted evidence.","let evidence = 98;"]{assert!(imported.contains(expected),"missing {expected}: {imported}");}assert!(!imported.contains("data-readmd-text"),"equivalent note-template direct sizes are not author overrides: {imported}");
    let out=output(&imported,&json!({}));let note=root(&out.parts,"word/footnotes.xml");assert_eq!(pair(&note,"Quoted evidence."),(Some(18),Some(18)),"ordinary note text uses its 9pt baseline");assert_eq!(pair(&note,"Matrix"),(Some(20),Some(20)),"the writer's explicit 10pt table style wins over note defaults");
    let code=note.descendants().into_iter().find(|node|node.name=="w:p"&&super::super::docx_pstyle_raw(node).as_deref()==Some("Code")).unwrap();let metadata=notes::code_label_runs(code,true).unwrap();let runs=all_runs(code).into_iter().filter(|node|{let id=*node as *const XmlNode as usize;id!=metadata.0&&id!=metadata.1&&!text(node).is_empty()}).collect::<Vec<_>>();assert!(runs.iter().map(|node|text(node)).collect::<String>().contains("let evidence = 98;"));for run in runs{assert_eq!((size(run,"w:sz"),size(run,"w:szCs")),(Some(18),Some(18)),"each actual highlighted code token uses the note baseline");}
}

#[test]
fn current_native_edits_and_clears_win_and_namespace_aliases_are_equivalent(){
    let styles=r#"<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/><w:szCs w:val="24"/></w:rPr></w:rPrDefault></w:docDefaults>"#;
    let body=r#"<w:p><w:r><w:rPr><w:sz w:val="60"/><w:szCs w:val="18"/><w:cs/></w:rPr><w:t>Edited</w:t></w:r></w:p>"#;
    let mut parts=package(body,styles,"","","");
    alter(&mut parts,"word/document.xml",r#"<w:sz w:val="60"/>"#,"");alter(&mut parts,"word/document.xml",r#"<w:szCs w:val="18"/>"#,r#"<w:szCs w:val="33"/>"#);alter(&mut parts,"word/document.xml","<w:cs/>",r#"<w:cs w:val="0"/>"#);
    let ordinary=import(&parts).unwrap();
    for(_,bytes)in &mut parts{*bytes=String::from_utf8(bytes.clone()).unwrap().replace(W,"http://purl.oclc.org/ooxml/wordprocessingml/main").replace("w:","alias:").replace("xmlns:w=","xmlns:alias=").into_bytes();}
    let aliased=import(&parts).unwrap();let first=output(&ordinary,&json!({}));let second=output(&aliased,&json!({}));
    // A cleared direct western size still has a known native docDefaults22
    // value. It must be explicit when asymmetric defaults cannot become one
    // source typography.size; a bare writer22 comparison cannot prove that
    // MCP's minimal21 fallback would reproduce it.
    for package in [first,second]{let xml=root(&package.parts,"word/document.xml");assert_eq!(pair(&xml,"Edited"),(Some(22),Some(33)));assert_eq!(default_pair(&package.parts),(Some(22),Some(22)));assert_eq!(flag(run(&xml,"Edited"),"w:cs"),Some(false));}
    let minimal=output_minimal(&ordinary);assert_eq!(default_pair(&minimal.parts),(Some(21),Some(21)));assert_eq!(pair(&root(&minimal.parts,"word/document.xml"),"Edited"),(Some(22),Some(33)));
}

#[test]
fn main_namespace_is_resolved_when_all_sizes_are_inherited_from_styles_only(){
    let mut parts=package(r#"<w:p><w:r><w:t>OnlyInherited</w:t></w:r></w:p>"#,r#"<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="61"/><w:szCs w:val="19"/><w:cs/><w:rtl w:val="0"/></w:rPr></w:rPrDefault></w:docDefaults>"#,"","","");
    for(_,bytes)in &mut parts{*bytes=String::from_utf8(bytes.clone()).unwrap().replace("w:","native:").replace("xmlns:w=","xmlns:native=").into_bytes();}
    let source=import(&parts).unwrap();let package=output(&source,&json!({}));let xml=root(&package.parts,"word/document.xml");assert_eq!(pair(&xml,"OnlyInherited"),(Some(61),Some(19)));assert_eq!(flag(run(&xml,"OnlyInherited"),"w:cs"),Some(true));assert_eq!(flag(run(&xml,"OnlyInherited"),"w:rtl"),Some(false));
    let mut wrong=parts.clone();alter(&mut wrong,"word/document.xml",W,"urn:fake-word");assert_eq!(import(&wrong).unwrap_err(),docx_notes::INVALID);
    let plain=vec![("word/document.xml".into(),br#"<w:document><w:body><w:p><w:r><w:t>xmlns:native=&quot;not a declaration&quot;</w:t></w:r></w:p></w:body></w:document>"#.to_vec())];
    assert!(import(&plain).unwrap().contains("not a declaration"),"legacy fragments are not reclassified by authored text");
}

#[test]
fn native_heading_table_hyperlink_emphasis_code_and_whitespace_keep_run_properties(){
    let styles=r#"<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/></w:style>"#;
    let body=format!(r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:rPr><w:sz w:val="67"/></w:rPr><w:t>Heading</w:t></w:r></w:p><w:p><w:r><w:rPr><w:sz w:val="60"/><w:b/><w:i/></w:rPr><w:t>StrongItalic</w:t></w:r><w:hyperlink r:id="url"><w:r><w:rPr><w:szCs w:val="17"/><w:cs/><w:rtl w:val="0"/></w:rPr><w:t>Linked</w:t></w:r></w:hyperlink><w:r><w:rPr><w:rFonts w:ascii="Consolas"/><w:sz w:val="23"/></w:rPr><w:t>literal *code*</w:t></w:r><w:r><w:rPr><w:sz w:val="31"/></w:rPr><w:t xml:space="preserve">  Edge  </w:t></w:r><w:r><w:rPr><w:sz w:val="32"/></w:rPr><w:t xml:space="preserve">   </w:t></w:r><w:r><w:rPr><w:sz w:val="33"/></w:rPr><w:t>BeforeBreak</w:t><w:br/><w:t>AfterBreak</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:rPr><w:sz w:val="37"/><w:szCs w:val="19"/></w:rPr><w:t>Cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#);
    let parts=package(&body,styles,"","",&format!(r#"<Relationship Id="url" Type="{R}/hyperlink" Target="https://example.test/native" TargetMode="External"/>"#));
    let source=import(&parts).unwrap();let package=output(&source,&json!({}));let xml=root(&package.parts,"word/document.xml");
    for(label,expected)in[("Heading",(Some(67),None)),("StrongItalic",(Some(60),None)),("Linked",(None,Some(17))),("literal *code*",(Some(23),None)),("  Edge  ",(Some(31),None)),("   ",(Some(32),None)),("Cell",(Some(37),Some(19)))]{
        let actual=pair(&xml,label);assert_eq!(actual.0,expected.0,"{label}: western");if expected.1.is_some(){assert_eq!(actual.1,expected.1,"{label}: complex");}
    }
    let strong=run(&xml,"StrongItalic");assert_eq!(flag(strong,"w:b"),Some(true));assert_eq!(flag(strong,"w:i"),Some(true));assert_eq!(flag(run(&xml,"Linked"),"w:cs"),Some(true));assert_eq!(flag(run(&xml,"Linked"),"w:rtl"),Some(false));
    assert!(xml.descendants().iter().any(|node|node.name=="w:hyperlink"&&node.attr("r:id").is_some()));
    assert!(xml.descendants().iter().any(|node|node.name=="w:tbl"));
    let break_runs=all_runs(&xml).into_iter().filter(|r|text(r).contains("Break")||r.children.iter().any(|n|n.name=="w:br")).collect::<Vec<_>>();assert!(!break_runs.is_empty());assert!(break_runs.iter().all(|r|size(r,"w:sz")==Some(33)));
    let reopened=import(&package.parts).unwrap();let second=output(&reopened,&json!({}));let again=root(&second.parts,"word/document.xml");assert_eq!(pair(&again,"Cell"),(Some(37),Some(19)));assert_eq!(text(run(&again,"  Edge  ")),"  Edge  ");
}

#[test]
fn notes_share_native_cache_but_keep_stories_kind_bases_and_mixed_runs_independent(){
    let styles=r#"<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/><w:szCs w:val="24"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:styleId="FootnoteText"><w:rPr><w:sz w:val="17"/><w:szCs w:val="18"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="EndnoteText"><w:rPr><w:sz w:val="25"/><w:szCs w:val="26"/></w:rPr></w:style>"#;
    let body=r#"<w:p><w:r><w:t>Body</w:t></w:r><w:r><w:rPr><w:sz w:val="57"/><w:szCs w:val="31"/></w:rPr><w:footnoteReference w:id="7"/></w:r><w:r><w:endnoteReference w:id="7"/><w:footnoteReference w:id="7"/></w:r></w:p>"#;
    let foot=r#"<w:footnote w:id="7"><w:p><w:pPr><w:pStyle w:val="FootnoteText"/></w:pPr><w:r><w:footnoteRef/></w:r><w:r><w:t>FootBase</w:t></w:r><w:r><w:rPr><w:sz w:val="60"/><w:szCs w:val="21"/><w:cs/></w:rPr><w:t>FootLarge</w:t></w:r><w:r><w:t>FootAfter</w:t></w:r></w:p></w:footnote>"#;
    let end=r#"<w:endnote w:id="7"><w:p><w:pPr><w:pStyle w:val="EndnoteText"/></w:pPr><w:r><w:endnoteRef/></w:r><w:r><w:t>EndBase</w:t></w:r><w:r><w:rPr><w:sz w:val="33"/><w:rtl w:val="0"/></w:rPr><w:t>EndDifferent</w:t></w:r></w:p></w:endnote>"#;
    let parts=package(body,styles,foot,end,"");let document=root(&parts,"word/document.xml");let body_node=child(&document,"w:body").unwrap();let rels=docx_notes::relationships(&parts,"word/document.xml").unwrap();let notes=docx_notes::Notes::load(&parts,body_node,&rels).unwrap();
    let index=RunStyleIndex::build(&parts,&rels,body_node,&notes).unwrap();let settings=notes::parse_with_text_styles(&parts,body_node,&rels,&notes,&index).unwrap();assert_eq!(settings.stats.parsed_parts,1);assert_eq!(settings.stats.borrowed_note_parts,2);assert_eq!(settings.stats.resolved_styles,2);
    let source=import(&parts).unwrap();let profile=crate::publication_profile::from_source(&source).unwrap();assert_eq!(profile.options["footnotes"]["kinds"]["footnote"]["size"],8.5);assert_eq!(profile.options["footnotes"]["kinds"]["footnote"]["sizeCs"],9.);assert_eq!(profile.options["footnotes"]["kinds"]["endnote"]["size"],12.5);
    let first=output(&source,&json!({}));let reopened=import(&first.parts).unwrap();let second=output(&reopened,&json!({}));
    for package in [first,second]{let foot=root(&package.parts,"word/footnotes.xml");let end=root(&package.parts,"word/endnotes.xml");assert_eq!(pair(&foot,"FootBase"),(Some(17),Some(18)));assert_eq!(pair(&foot,"FootAfter"),(Some(17),Some(18)));assert_eq!(pair(&foot,"FootLarge"),(Some(60),Some(21)));assert_eq!(flag(run(&foot,"FootLarge"),"w:cs"),Some(true));assert_eq!(pair(&end,"EndBase"),(Some(25),Some(26)));assert_eq!(pair(&end,"EndDifferent"),(Some(33),Some(26)));assert_eq!(flag(run(&end,"EndDifferent"),"w:rtl"),Some(false));
        let main=root(&package.parts,"word/document.xml");let references=all_runs(&main).into_iter().filter(|r|child(r,"w:footnoteReference").is_some()).collect::<Vec<_>>();assert_eq!(references.len(),2);assert_eq!((size(references[0],"w:sz"),size(references[0],"w:szCs")),(Some(57),Some(31)));assert_eq!((size(references[1],"w:sz"),size(references[1],"w:szCs")),(Some(22),Some(24)),"both known asymmetric native axes must survive a different export fallback");assert_eq!(default_pair(&package.parts),(Some(22),Some(22)));
    }
}

#[test]
fn inherited_note_size_keeps_real_leading_and_trailing_spaces_without_redundant_author_override(){
    let styles=r#"<w:style w:type="paragraph" w:styleId="FootnoteText"><w:rPr><w:sz w:val="17"/><w:szCs w:val="19"/></w:rPr></w:style>"#;
    let body=r#"<w:p><w:r><w:t>Body</w:t><w:footnoteReference w:id="1"/></w:r></w:p>"#;
    let foot=r#"<w:footnote w:id="1"><w:p><w:pPr><w:pStyle w:val="FootnoteText"/></w:pPr><w:r><w:footnoteRef/></w:r><w:r><w:t xml:space="preserve">  Inherited edge  </w:t></w:r></w:p></w:footnote>"#;
    let source=import(&package(body,styles,foot,"","")).unwrap();assert!(source.contains("&#32;&#32;Inherited edge&#32;&#32;"),"{source}");
    let package=output(&source,&json!({}));let notes=root(&package.parts,"word/footnotes.xml");assert_eq!(pair(&notes,"  Inherited edge  "),(Some(17),Some(19)));
}

#[test]
fn author_axes_inherit_and_blockbase_do_not_copy_into_each_other_or_escape_block_styles(){
    let children=vec![styled(json!({"size":30,"sizeCs":10}),vec![Inline::Text("Outer".into()),styled(json!({"size":"base"}),vec![Inline::Text("ResetWestern".into())]),styled(json!({"sizeCs":"base"}),vec![Inline::Text("ResetComplex".into())])]),styled(json!({"size":25}),vec![Inline::Text("WesternOnly".into())]),styled(json!({"sizeCs":7.5}),vec![Inline::Text("ComplexOnly".into())])];
    let doc=Document{front_matter:None,blocks:vec![Block::Styled{attributes:json!({"id":"base-test","text":{"size":18}}),block:Box::new(Block::Paragraph(children))},Block::Paragraph(vec![Inline::Text("After".into())])]};
    let package=crate::docx_writer::build(&doc,&json!({}),"Base.md",&|_|None);assert!(package.warns.is_empty(),"{:?}",package.warns);let xml=root(&package.parts,"word/document.xml");
    for(label,w,cs)in[("Outer",60,20),("ResetWestern",36,20),("ResetComplex",60,36),("WesternOnly",50,36),("ComplexOnly",36,15)]{assert_eq!(pair(&xml,label),(Some(w),Some(cs)),"{label}");}
    assert_eq!(pair(&xml,"After"),(None,None));assert!(!word(&package.parts).contains(crate::docx_local_style::AUTHOR_AXES_ATTRIBUTE));
}

#[test]
fn western_only_complex_only_and_false_routes_are_written_without_aliases(){
    let doc=Document{front_matter:None,blocks:vec![Block::Paragraph(vec![styled(json!({"size":30}),vec![Inline::Text("Western".into())]),styled(json!({"sizeCs":9,"cs":true,"rtl":false}),vec![Inline::Text("Latin uses CS".into())]),styled(json!({"cs":false,"rtl":true}),vec![Inline::Text("ForcedRoute".into())])])]};
    let package=crate::docx_writer::build(&doc,&json!({}),"Axes.md",&|_|None);assert!(package.warns.is_empty(),"{:?}",package.warns);let xml=root(&package.parts,"word/document.xml");assert_eq!(pair(&xml,"Western"),(Some(60),None));assert_eq!(pair(&xml,"Latin uses CS"),(None,Some(18)));assert_eq!(flag(run(&xml,"Latin uses CS"),"w:cs"),Some(true));assert_eq!(flag(run(&xml,"Latin uses CS"),"w:rtl"),Some(false));assert_eq!(pair(&xml,"ForcedRoute"),(None,None));assert_eq!(flag(run(&xml,"ForcedRoute"),"w:cs"),Some(false));assert_eq!(flag(run(&xml,"ForcedRoute"),"w:rtl"),Some(true));
}

#[test]
fn blockbase_uses_heading_table_and_independent_note_policy_bases(){
    let source="# <span data-readmd-text=\"v1\" data-size-pt=\"30\"><span data-readmd-text=\"v1\" data-size-pt=\"base\" data-size-cs-pt=\"base\">HeadingBase</span></span>\n\n| Cell |\n|---|\n| <span data-readmd-text=\"v1\" data-size-pt=\"base\" data-size-cs-pt=\"base\">TableBase</span> |\n\nBody[^n].\n\n[^n]: <span data-readmd-text=\"v1\" data-size-pt=\"30\"><span data-readmd-text=\"v1\" data-size-pt=\"base\" data-size-cs-pt=\"base\">NoteBase</span></span>\n";
    let package=output(source,&json!({"headings":{"h1":{"size":23}},"table":{"cellSize":12},"footnotes":{"kinds":{"footnote":{"size":8.5,"sizeCs":9.5}}}}));let xml=root(&package.parts,"word/document.xml");assert_eq!(pair(&xml,"HeadingBase"),(Some(46),Some(46)));assert_eq!(pair(&xml,"TableBase"),(Some(24),Some(24)));let notes=root(&package.parts,"word/footnotes.xml");assert_eq!(pair(&notes,"NoteBase"),(Some(17),Some(19)));
}

#[test]
fn author_markers_are_removed_only_after_real_section_offsets_are_consumed(){
    let first=crate::publication_sections::comment(&serde_json::from_value::<crate::publication_sections::Section>(json!({"id":"first"})).unwrap()).unwrap();
    let second=crate::publication_sections::comment(&serde_json::from_value::<crate::publication_sections::Section>(json!({"id":"second","break":"continuous"})).unwrap()).unwrap();
    let source=format!("{first}\n\n<span data-readmd-text=\"v1\" data-size-pt=\"30\">First</span>\n\n{second}\n\n<span data-readmd-text=\"v1\" data-size-cs-pt=\"9\">Second</span>\n");
    let package=output(&source,&json!({}));let xml=root(&package.parts,"word/document.xml");assert_eq!(pair(&xml,"First"),(Some(60),None));assert_eq!(pair(&xml,"Second"),(None,Some(18)));let body=child(&xml,"w:body").unwrap();let sections=body.children.iter().enumerate().filter(|(_,p)|child(p,"w:pPr").is_some_and(|pr|child(pr,"w:sectPr").is_some())).map(|(i,_)|i).collect::<Vec<_>>();assert_eq!(sections.len(),1);let first=body.children.iter().position(|p|p.descendants().iter().any(|r|r.name=="w:t"&&r.text=="First")).unwrap();let second=body.children.iter().position(|p|p.descendants().iter().any(|r|r.name=="w:t"&&r.text=="Second")).unwrap();assert!(first<sections[0]&&sections[0]<second);assert!(!word(&package.parts).contains(crate::docx_local_style::AUTHOR_AXES_ATTRIBUTE));
}

#[test]
fn malformed_native_sizes_routing_and_style_cycles_are_typed_before_source_output(){
    for property in [r#"<w:sz w:val="0"/>"#,r#"<w:sz w:val="2147483648"/>"#,r#"<w:sz w:val="30.5"/>"#,r#"<w:sz val="60"/>"#,r#"<w:sz w:val="60"/><w:sz w:val="20"/>"#,r#"<w:sz xmlns:q="urn:fake" q:val="60"/>"#,r#"<w:cs w:val="maybe"/>"#,r#"<w:rtl w:val="2"/>"#]{let body=format!("<w:p><w:r><w:rPr>{property}</w:rPr><w:t>Native</w:t></w:r></w:p>");let parts=package(&body,"","","","");let before=parts.clone();let err=import(&parts).unwrap_err();assert!(err.starts_with(INVALID)||err.starts_with(UNSUPPORTED),"{property}: {err}");assert!(err.contains("word/document.xml"));assert_eq!(parts,before);}
    let cycle=r#"<w:style w:type="paragraph" w:styleId="A"><w:basedOn w:val="B"/></w:style><w:style w:type="paragraph" w:styleId="B"><w:basedOn w:val="A"/></w:style>"#;let err=import(&package(r#"<w:p><w:pPr><w:pStyle w:val="A"/></w:pPr><w:r><w:t>Cycle</w:t></w:r></w:p>"#,cycle,"","","")).unwrap_err();assert!(err.starts_with(INVALID),"{err}");assert!(err.contains("cyclic"));
}

#[test]
fn precise_code_and_field_guards_do_not_claim_unrepresented_sized_runs(){
    let code_style=r#"<w:style w:type="paragraph" w:styleId="Code"><w:name w:val="Code"/></w:style>"#;
    let code=r#"<w:p><w:pPr><w:pStyle w:val="Code"/></w:pPr><w:r><w:rPr><w:sz w:val="60"/></w:rPr><w:t>Code text</w:t></w:r></w:p>"#;
    let inline=r#"<w:p><w:r><w:rPr><w:rFonts w:ascii="Consolas"/><w:sz w:val="60"/></w:rPr><w:t>Code</w:t><w:br/><w:t>line</w:t></w:r></w:p>"#;
    let simple=r#"<w:p><w:fldSimple w:instr=" DATE "><w:r><w:rPr><w:sz w:val="60"/></w:rPr><w:t>Current date</w:t></w:r></w:fldSimple></w:p>"#;
    let complex=r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/><w:instrText> DATE </w:instrText><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:rPr><w:szCs w:val="60"/></w:rPr><w:t>Current date</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#;
    for body in [code,inline,simple,complex]{let err=import(&package(body,code_style,"","","")).unwrap_err();assert!(err.starts_with(UNSUPPORTED),"{err}");assert!(err.contains("word/document.xml"));}
    let plain=r#"<w:p><w:fldSimple w:instr=" DATE "><w:r><w:t>Current date</w:t></w:r></w:fldSimple></w:p>"#;assert!(import(&package(plain,"","","","")).unwrap().contains("Current date"));
    let generated=crate::docx_writer::build(&md_ast::parse("```rust\nlet x = 1;\n```"),&json!({}),"Code.md",&|_|None);assert!(generated.warns.is_empty());let source=import(&generated.parts).unwrap();assert!(source.contains("```rust\nlet x = 1;"),"{source}");
}

#[test]
fn unsupported_author_math_never_returns_partial_docx_parts(){
    for inline in [Inline::Math("x".into()),Inline::OfficeMath{latex:"x".into(),omml:r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:r><m:t>x</m:t></m:r></m:oMath>"#.into()}]{let doc=Document{front_matter:None,blocks:vec![Block::Paragraph(vec![Inline::Text("Before".into()),styled(json!({"size":30}),vec![inline]),Inline::Text("After".into())])]};let package=crate::docx_writer::build(&doc,&json!({}),"Guard.md",&|_|None);assert!(package.parts.is_empty());assert_eq!(package.warns.first().map(String::as_str),Some(UNSUPPORTED));}
}

#[test]
fn private_axis_authority_rejects_forgery_and_restores_duplicate_run_positions(){
    let fragment=format!(r#"<w:p><w:r {}="private:1"><w:rPr><w:sz w:val="60"/></w:rPr><w:t>Same</w:t></w:r><w:r {}="private:2"><w:rPr><w:szCs w:val="18"/></w:rPr><w:t>Same</w:t></w:r></w:p>"#,crate::docx_local_style::AUTHOR_AXES_ATTRIBUTE,crate::docx_local_style::AUTHOR_AXES_ATTRIBUTE);
    let protected=crate::docx_local_style::preserve_author_axes(&fragment,"private",|source|Ok(source.replace(r#"<w:sz w:val="60"/>"#,r#"<w:sz w:val="36"/><w:szCs w:val="36"/>"#).replace(r#"<w:szCs w:val="18"/>"#,r#"<w:sz w:val="36"/><w:szCs w:val="36"/>"#))).unwrap();
    let clean=crate::docx_local_style::strip_author_axes(&protected,"private").unwrap();let xml=docx_notes::xml(format!(r#"<w:document xmlns:w="{W}"><w:body>{clean}</w:body></w:document>"#).as_bytes()).unwrap();let runs=all_runs(&xml);assert_eq!((size(runs[0],"w:sz"),size(runs[0],"w:szCs")),(Some(60),Some(36)));assert_eq!((size(runs[1],"w:sz"),size(runs[1],"w:szCs")),(Some(36),Some(18)));assert!(!clean.contains(crate::docx_local_style::AUTHOR_AXES_ATTRIBUTE));
    assert_eq!(crate::docx_local_style::preserve_author_axes(&fragment,"forged",|s|Ok(s.into())),Err(INVALID));assert_eq!(crate::docx_local_style::strip_author_axes(&fragment,"forged"),Err(INVALID));assert_eq!(crate::docx_local_style::preserve_author_axes(&fragment,"private",|_|Ok("<w:p/>".into())),Err(INVALID));
}

fn caption_package(base:&str,title_properties:&str,reference_style:&str)->Vec<(String,Vec<u8>)>{
    let styles=format!(r#"<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/><w:szCs w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:styleId="NativeCaptionBase"><w:rPr><w:sz w:val="18"/>{base}</w:rPr></w:style><w:style w:type="paragraph" w:styleId="NativeCaption"><w:basedOn w:val="NativeCaptionBase"/></w:style><w:style w:type="paragraph" w:styleId="BigRef"><w:rPr><w:sz w:val="60"/></w:rPr></w:style>"#);
    let body=format!(r#"<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:pPr><w:pStyle w:val="NativeCaption"/></w:pPr><w:bookmarkStart w:id="91" w:name="cap_table"/><w:r><w:t xml:space="preserve">Table </w:t></w:r><w:fldSimple w:instr=" SEQ Table \r 4 \* ARABIC "><w:r><w:t>4</w:t></w:r></w:fldSimple><w:bookmarkEnd w:id="91"/><w:r><w:t xml:space="preserve"> — </w:t></w:r><w:r><w:rPr>{title_properties}</w:rPr><w:t>NativeTitle</w:t></w:r></w:p><w:p>{reference_style}<w:fldSimple w:instr=" REF cap_table \h "><w:r><w:t>Table 4</w:t></w:r></w:fldSimple></w:p>"#);
    package(&body,&styles,"","","")
}

#[test]
fn uniform_native_caption_and_ref_author_axes_rebuild_current_sizes_and_routes(){
    for property in [r#"<w:sz w:val="60"/>"#,r#"<w:szCs w:val="60"/>"#,r#"<w:cs/>"#,r#"<w:rtl/>"#]{
        // The new caption author model now represents these uniform current
        // native values. Check the real fields and sizes instead of rejecting
        // a value merely because the previous source model lacked it.
        let mut parts=caption_package(property,"","");
        if property.starts_with("<w:sz "){alter(&mut parts,"word/styles.xml",r#"<w:sz w:val="18"/><w:sz w:val="60"/>"#,property);}
        let before=parts.clone();let native=effective_run(&parts,"4");let native_ref=effective_run(&parts,"Table 4");let source=import(&parts).unwrap();assert_eq!(parts,before);let id=automatic_table_id(&source);
        let mut first=output_minimal(&source);assert_table_offset_field_identity(&root(&first.parts,"word/document.xml"),&id);
        super::super::docx_numbering_import::attach_source_info(&mut first.parts,&source).unwrap();let reopened=import(&first.parts).unwrap();assert_eq!(automatic_table_id(&reopened),id);let second=output_minimal(&reopened);
        for package in [first,second]{let actual=effective_run(&package.parts,"4");assert_eq!((actual.size,actual.size_cs,actual.cs,actual.rtl),(native.size,native.size_cs,native.cs,native.rtl),"{property}");assert_table_offset_field_identity(&root(&package.parts,"word/document.xml"),&id);assert_caption_run_axes(&package.parts,&id,"Table 4"," — NativeTitle",axes(&native),Some(axes(&native_ref)));}
    }
    let parts=caption_package("","",r#"<w:pPr><w:pStyle w:val="BigRef"/></w:pPr>"#);let source=import(&parts).unwrap();let id=automatic_table_id(&source);let package=output_minimal(&source);assert_table_offset_field_identity(&root(&package.parts,"word/document.xml"),&id);assert_eq!(effective_run(&package.parts,"Table 4").size,Some(60));assert_caption_run_axes(&package.parts,&id,"Table 4"," — NativeTitle",(Some(18),Some(22),None,None),Some((Some(60),Some(22),None,None)));
    let mut complex=parts.clone();alter(&mut complex,"word/document.xml",r#"<w:fldSimple w:instr=" REF cap_table \h "><w:r><w:t>Table 4</w:t></w:r></w:fldSimple>"#,r#"<w:r><w:fldChar w:fldCharType="begin"/><w:instrText> REF cap_table \h </w:instrText><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>Table 4</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#);let source=import(&complex).unwrap();let id=automatic_table_id(&source);let package=output_minimal(&source);assert_table_offset_field_identity(&root(&package.parts,"word/document.xml"),&id);assert_eq!(effective_run(&package.parts,"Table 4").size,Some(60));assert_caption_run_axes(&package.parts,&id,"Table 4"," — NativeTitle",(Some(18),Some(22),None,None),Some((Some(60),Some(22),None,None)));
}

#[test]
fn native_caption_field_baseline_matches_actual_word_styles_and_title_override_is_editable(){
    let mut parts=caption_package(r#"<w:cs w:val="0"/><w:rtl w:val="0"/>"#,r#"<w:sz w:val="60"/><w:szCs w:val="19"/>"#,"");
    alter(&mut parts,"word/document.xml",r#"<w:r><w:t>4</w:t></w:r>"#,r#"<w:r><w:rPr><w:sz w:val="18"/><w:szCs w:val="22"/></w:rPr><w:t>4</w:t></w:r>"#);alter(&mut parts,"word/document.xml",r#"<w:r><w:t>Table 4</w:t></w:r>"#,r#"<w:r><w:rPr><w:sz w:val="22"/><w:szCs w:val="22"/></w:rPr><w:t>Table 4</w:t></w:r>"#);
    assert!(!parts.iter().any(|(name,_)|name==super::super::docx_numbering_import::SOURCE_PART),"first input must remain genuine native-only fields");let source=import(&parts).unwrap();let id=automatic_table_id(&source);assert!(source.contains(&format!(":ref[{id}]")));let mut first=output(&source,&json!({}));let xml=root(&first.parts,"word/document.xml");assert_eq!(pair(&xml,"NativeTitle"),(Some(60),Some(19)));assert_table_offset_field_identity(&xml,&id);
    let styles=root(&first.parts,"word/styles.xml");let caption=styles.children.iter().find(|node|node.name=="w:style"&&node.attr("w:styleId")==Some("Caption")).unwrap();let pr=child(caption,"w:rPr").unwrap();assert_eq!(child(pr,"w:sz").unwrap().attr("w:val"),Some("18"));assert!(child(pr,"w:szCs").is_none(),"CS actually inherits document default; do not assume Caption uses 9pt on both axes");let defaults=child(child(child(&styles,"w:docDefaults").unwrap(),"w:rPrDefault").unwrap(),"w:rPr").unwrap();assert_eq!(child(defaults,"w:szCs").unwrap().attr("w:val"),Some("22"));
    // Match the real exporter only for identity/explicitness binding. All
    // native field values above have already been checked without this part.
    super::super::docx_numbering_import::attach_source_info(&mut first.parts,&source).unwrap();let reopened=import(&first.parts).unwrap();assert_eq!(automatic_table_id(&reopened),id);assert!(reopened.contains(&format!(":ref[{id}]")));let mut second=output(&reopened,&json!({}));let xml=root(&second.parts,"word/document.xml");assert_eq!(pair(&xml,"NativeTitle"),(Some(60),Some(19)));assert_table_offset_field_identity(&xml,&id);super::super::docx_numbering_import::attach_source_info(&mut second.parts,&reopened).unwrap();let third=import(&second.parts).unwrap();assert_eq!(automatic_table_id(&third),id);
}

#[test]
fn consumed_common_native_body_size_is_source_owned_even_at_eleven_points(){
    for half_points in [21,22,28]{
        let styles=format!(r#"<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="{half_points}"/><w:szCs w:val="{half_points}"/></w:rPr></w:rPrDefault></w:docDefaults>"#);
        let parts=package(r#"<w:p><w:r><w:t>CommonNativeBody</w:t></w:r></w:p>"#,&styles,"","","");let before=parts.clone();
        let source=import(&parts).unwrap();assert_eq!(parts,before);assert!(!source.contains("data-readmd-text"));
        assert_eq!(crate::publication_profile::from_source(&source).unwrap().options["typography"]["size"],half_points as f64/2.);
        let first=output_minimal(&source);let reopened=import(&first.parts).unwrap();let second=output_minimal(&reopened);
        for package in [first,second]{
            assert_eq!(default_pair(&package.parts),(Some(half_points),Some(half_points)),"an empty request over minimal10.5 must retain the native source baseline");
            assert_eq!(pair(&root(&package.parts,"word/document.xml"),"CommonNativeBody"),(None,None));
        }
    }
}

#[test]
fn native_minimal_caption_and_ref_fields_keep_common_twenty_one_half_point_baseline(){
    let mut parts=caption_package("","","");alter(&mut parts,"word/styles.xml",r#"<w:sz w:val="22"/><w:szCs w:val="22"/>"#,r#"<w:sz w:val="21"/><w:szCs w:val="21"/>"#);
    let before=parts.clone();let source=import(&parts).unwrap();assert_eq!(parts,before);
    assert_eq!(crate::publication_profile::from_source(&source).unwrap().options["typography"]["size"],10.5);
    let id=automatic_table_id(&source);assert!(source.contains(&format!(":ref[{id}]")));
    let mut first=output_minimal(&source);assert_eq!(default_pair(&first.parts),(Some(21),Some(21)));assert_table_offset_field_identity(&root(&first.parts,"word/document.xml"),&id);
    assert_caption_run_axes(&first.parts,&id,"Table 4"," — NativeTitle",(Some(18),Some(21),None,None),Some((Some(21),Some(21),None,None)));
    super::super::docx_numbering_import::attach_source_info(&mut first.parts,&source).unwrap();
    let reopened=import(&first.parts).unwrap();assert_eq!(automatic_table_id(&reopened),id);let second=output_minimal(&reopened);
    assert_eq!(default_pair(&second.parts),(Some(21),Some(21)));assert_table_offset_field_identity(&root(&second.parts,"word/document.xml"),&id);
    assert_caption_run_axes(&second.parts,&id,"Table 4"," — NativeTitle",(Some(18),Some(21),None,None),Some((Some(21),Some(21),None,None)));
    for properties in [r#"<w:sz w:val="60"/>"#,r#"<w:szCs w:val="60"/>"#,r#"<w:cs/>"#,r#"<w:rtl/>"#]{
        let mut edited=parts.clone();alter(&mut edited,"word/document.xml",r#"<w:r><w:t>4</w:t></w:r>"#,&format!(r#"<w:r><w:rPr>{properties}</w:rPr><w:t>4</w:t></w:r>"#));
        let before=edited.clone();let error=import(&edited).unwrap_err();assert!(error.starts_with(UNSUPPORTED),"{properties}: {error}");assert!(error.contains("field"));assert_eq!(edited,before);
    }
}

#[test]
fn independent_or_large_native_semantic_axes_rebuild_without_global_promotion(){
    for defaults in [r#"<w:sz w:val="22"/><w:szCs w:val="24"/>"#,r#"<w:sz w:val="14"/><w:szCs w:val="14"/>"#,r#"<w:sz w:val="42"/><w:szCs w:val="42"/>"#,r#"<w:sz w:val="22"/>"#]{
        let mut parts=caption_package("","","");alter(&mut parts,"word/styles.xml",r#"<w:sz w:val="22"/><w:szCs w:val="22"/>"#,defaults);
        let before=parts.clone();let native_caption=effective_run(&parts,"4");let native_ref=effective_run(&parts,"Table 4");let source=import(&parts).unwrap();assert_eq!(parts,before);assert!(crate::publication_profile::from_source(&source).unwrap().options.get("typography").is_none());let id=automatic_table_id(&source);
        let mut first=output_minimal(&source);super::super::docx_numbering_import::attach_source_info(&mut first.parts,&source).unwrap();let reopened=import(&first.parts).unwrap();let second=output_minimal(&reopened);
        for package in [first,second]{assert_table_offset_field_identity(&root(&package.parts,"word/document.xml"),&id);for(label,native)in[("4",&native_caption),("Table 4",&native_ref)]{let actual=effective_run(&package.parts,label);if native.size.is_some(){assert_eq!(actual.size,native.size,"{defaults}: {label}");}if native.size_cs.is_some(){assert_eq!(actual.size_cs,native.size_cs,"{defaults}: {label}");}}}
    }
}

#[test]
fn known_asymmetric_or_single_default_axes_stay_explicit_under_minimal_fallback(){
    for (defaults,expected) in [(r#"<w:sz w:val="22"/><w:szCs w:val="24"/>"#,(Some(22),Some(24))),(r#"<w:sz w:val="22"/>"#,(Some(22),None)),(r#"<w:sz w:val="61"/><w:szCs w:val="61"/>"#,(Some(61),Some(61)))]{
        let styles=format!("<w:docDefaults><w:rPrDefault><w:rPr>{defaults}</w:rPr></w:rPrDefault></w:docDefaults>");
        let source=import(&package(r#"<w:p><w:r><w:t>KnownNativeAxes</w:t></w:r></w:p>"#,&styles,"","","")).unwrap();
        assert!(crate::publication_profile::from_source(&source).unwrap().options.get("typography").is_none());
        let first=output_minimal(&source);assert_eq!(default_pair(&first.parts),(Some(21),Some(21)));assert_eq!(pair(&root(&first.parts,"word/document.xml"),"KnownNativeAxes"),expected);
        if expected.1.is_none(){assert!(!source.contains("data-size-cs-pt"),"a missing native axis remains unknown");}
        else{let reopened=import(&first.parts).unwrap();let second=output_minimal(&reopened);assert_eq!(pair(&root(&second.parts,"word/document.xml"),"KnownNativeAxes"),expected);}
    }
}

fn ordinary_code_styles(extra:&str)->String{
    format!(r#"<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:styleId="CodeBlock"><w:name w:val="Code Block"/><w:rPr><w:rFonts w:ascii="Consolas" w:hAnsi="Consolas"/></w:rPr></w:style>{extra}"#)
}
fn ordinary_code_paragraph(contents:&str)->String{format!(r#"<w:p><w:pPr><w:pStyle w:val="CodeBlock"/></w:pPr>{contents}</w:p>"#)}

#[test]
fn ordinary_docdefaults_only_code_keeps_native_section_terminator_outside_fence(){
    let body=r#"<w:p><w:r><w:t>Before native code</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="CodeBlock"/><w:sectPr><w:type w:val="continuous"/><w:footnotePr><w:pos w:val="beneathText"/></w:footnotePr></w:sectPr></w:pPr><w:r><w:t>SECTION_A_CODE_END</w:t></w:r></w:p><w:p><w:r><w:t>After native code</w:t></w:r></w:p><w:sectPr><w:footnotePr><w:pos w:val="pageBottom"/></w:footnotePr></w:sectPr>"#;
    let parts=package(body,&ordinary_code_styles(""),"","","");let before=parts.clone();let source=import(&parts).unwrap();assert_eq!(parts,before);
    let declarations=crate::publication_sections::analyze(&source).unwrap();assert_eq!(declarations.len(),2);
    assert!(source[..declarations[1].bytes.start].trim_end().ends_with("```"),"the native boundary must follow the closed code fence: {source}");
    assert_eq!(declarations[0].section.footnotes.as_ref().unwrap().footnote_placement,Some(crate::publication_note_policies::NotePlacement::BeneathText));
    assert_eq!(declarations[1].section.footnotes.as_ref().unwrap().footnote_placement,Some(crate::publication_note_policies::NotePlacement::PageBottom));
    let doc=md_ast::parse(&source);assert_eq!(doc.blocks.iter().filter_map(|block|if let Block::Code{text,..}=block.unstyled(){Some(text.as_str())}else{None}).collect::<Vec<_>>(),["SECTION_A_CODE_END"]);
    let first=output(&source,&json!({}));let xml=root(&first.parts,"word/document.xml");assert_eq!(xml.descendants().iter().filter(|node|node.name=="w:sectPr").count(),2);
    let reopened=import(&first.parts).unwrap();let scopes=crate::publication_sections::analyze(&reopened).unwrap();assert_eq!(scopes.len(),2);assert_eq!(scopes[0].section.footnotes.as_ref().unwrap().footnote_placement,Some(crate::publication_note_policies::NotePlacement::BeneathText));assert_eq!(scopes[1].section.footnotes.as_ref().unwrap().footnote_placement,Some(crate::publication_note_policies::NotePlacement::PageBottom));assert!(md_ast::parse(&reopened).blocks.iter().any(|block|matches!(block.unstyled(),Block::Code{text,..}if text=="SECTION_A_CODE_END")));
    // This restores legacy ordinary code text/section conversion. It does not
    // claim an editable per-code-block native11pt typography model.
}

#[test]
fn code_legacy_compatibility_requires_one_uniform_group_without_local_author_axes(){
    let plain=ordinary_code_paragraph(r#"<w:r><w:t>FirstCodeLine</w:t></w:r>"#);
    let two=format!("{plain}{}",ordinary_code_paragraph(r#"<w:r><w:t>SecondCodeLine</w:t></w:r>"#));
    let source=import(&package(&two,&ordinary_code_styles(""),"","","")).unwrap();let doc=md_ast::parse(&source);assert_eq!(doc.blocks.iter().filter_map(|block|if let Block::Code{text,..}=block.unstyled(){Some(text.as_str())}else{None}).collect::<Vec<_>>(),["FirstCodeLine\nSecondCodeLine"]);
    let mut cases=Vec::new();
    for properties in [r#"<w:sz w:val="60"/>"#,r#"<w:sz w:val="22"/>"#,r#"<w:szCs w:val="60"/>"#,r#"<w:cs/>"#,r#"<w:rtl/>"#,r#"<w:cs w:val="0"/>"#]{cases.push((ordinary_code_paragraph(&format!(r#"<w:r><w:rPr>{properties}</w:rPr><w:t>LocalCode</w:t></w:r>"#)),String::new()));}
    cases.push((ordinary_code_paragraph(r#"<w:r><w:t>Inherited</w:t></w:r><w:r><w:rPr><w:sz w:val="19"/><w:szCs w:val="19"/></w:rPr><w:t>CanonicalButMixed</w:t></w:r>"#),String::new()));
    cases.push((ordinary_code_paragraph(r#"<w:r><w:rPr><w:rStyle w:val="CodeChar"/></w:rPr><w:t>CharacterCode</w:t></w:r>"#),r#"<w:style w:type="character" w:styleId="CodeChar"><w:rPr><w:sz w:val="60"/></w:rPr></w:style>"#.into()));
    cases.push((format!("{plain}<w:p><w:pPr><w:pStyle w:val=\"Code\"/></w:pPr><w:r><w:t>AdjacentLocalCode</w:t></w:r></w:p>"),r#"<w:style w:type="paragraph" w:styleId="Code"><w:name w:val="Code"/><w:rPr><w:sz w:val="19"/><w:szCs w:val="19"/></w:rPr></w:style>"#.into()));
    cases.push((r#"<w:p><w:pPr><w:pStyle w:val="CodeInherited"/></w:pPr><w:r><w:t>ParagraphLocalCode</w:t></w:r></w:p>"#.into(),r#"<w:style w:type="paragraph" w:styleId="CodeBase"><w:rPr><w:sz w:val="60"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="CodeInherited"><w:name w:val="Code"/><w:basedOn w:val="CodeBase"/></w:style>"#.into()));
    for (body,extra) in cases{let parts=package(&body,&ordinary_code_styles(&extra),"","","");let before=parts.clone();let error=import(&parts).unwrap_err();assert!(error.starts_with(UNSUPPORTED),"{body}: {error}");assert!(error.contains("fenced code block"));assert_eq!(parts,before);}
    let mut routed=package(&plain,&ordinary_code_styles(""),"","","");alter(&mut routed,"word/styles.xml",r#"<w:sz w:val="22"/>"#,r#"<w:sz w:val="22"/><w:rtl/>"#);assert!(import(&routed).unwrap_err().starts_with(UNSUPPORTED));
}

#[test]
fn actual_native_missing_caption_styles_preserve_single_known_axis_and_live_fields_twice(){
    for bytes in [include_bytes!("../../tests/fixtures/author-native-caption-numbering.docx").as_slice(),include_bytes!("../../tests/fixtures/author-native-caption-note-mixed.docx").as_slice(),include_bytes!("../../tests/fixtures/author-native-caption-mixed.docx").as_slice()]{
        let parts=super::super::parse_zip_entries(bytes).unwrap();assert!(!parts.iter().any(|(name,_)|name==super::super::docx_numbering_import::SOURCE_PART));
        let native=effective_run(&parts,"4");assert_eq!((native.size,native.size_cs),(Some(22),None));let before=parts.clone();let source=import(&parts).unwrap();assert_eq!(parts,before);let id=automatic_table_id(&source);
        let model=crate::publication_blocks::analyze(&source).unwrap();assert!(model.diagnostics.is_empty(),"{:?}",model.diagnostics);let caption=model.nodes.iter().find(|node|node.kind=="caption").unwrap();assert_eq!(caption.style["text"]["author"],json!({"size":11}));
        let mut first=output_minimal(&source);super::super::docx_numbering_import::attach_source_info(&mut first.parts,&source).unwrap();let reopened=import(&first.parts).unwrap();assert_eq!(automatic_table_id(&reopened),id);let second=output_minimal(&reopened);
        for package in [first,second]{let xml=root(&package.parts,"word/document.xml");let fields=word_fields(&xml);assert_eq!(fields.len(),2);assert!(fields.iter().any(|field|field.nested.iter().any(|nested|nested.code.split_whitespace().collect::<Vec<_>>()==["SEQ","Table"])));assert!(fields.iter().any(|field|field.code.split_whitespace().next()==Some("REF")&&field.cache=="Table 4"));
            for label in ["4","Table 4"]{assert_eq!(effective_run(&package.parts,label).size,Some(22));assert_eq!(size(run(&xml,label),"w:szCs"),None,"no missing native CS axis is invented as an author declaration");}
            assert_eq!(default_pair(&package.parts),(Some(21),Some(21)),"the actual minimal fallback stays21; the known22 is carried by author source");
        }
    }
}

#[test]
fn direct_and_character_style_uniform_caption_sizes_are_editable_but_mixed_field_results_are_not(){
    for property in [r#"<w:sz w:val="60"/>"#,r#"<w:rStyle w:val="NativeAuthorChar"/>"#]{
        let mut parts=caption_package("",r#"<w:sz w:val="60"/>"#,"");
        alter(&mut parts,"word/styles.xml","</w:styles>",r#"<w:style w:type="character" w:styleId="NativeAuthorChar"><w:rPr><w:sz w:val="60"/></w:rPr></w:style></w:styles>"#);
        let document=parts.iter_mut().find(|(name,_)|name=="word/document.xml").unwrap();let original=String::from_utf8(document.1.clone()).unwrap();document.1=original.replace("<w:r><w:t",&format!("<w:r><w:rPr>{property}</w:rPr><w:t")).into_bytes();
        let source=import(&parts).unwrap();let id=automatic_table_id(&source);let mut first=output_minimal(&source);super::super::docx_numbering_import::attach_source_info(&mut first.parts,&source).unwrap();let reopened=import(&first.parts).unwrap();let second=output_minimal(&reopened);
        for package in [first,second]{assert_table_offset_field_identity(&root(&package.parts,"word/document.xml"),&id);assert_eq!(effective_run(&package.parts,"4").size,Some(60));assert_eq!(effective_run(&package.parts,"Table 4").size,Some(60));}
    }
    let mut mixed=caption_package("","","");alter(&mut mixed,"word/document.xml",r#"<w:r><w:t>Table 4</w:t></w:r>"#,r#"<w:r><w:t xml:space="preserve">Table </w:t></w:r><w:r><w:rPr><w:sz w:val="60"/></w:rPr><w:t>4</w:t></w:r>"#);let before=mixed.clone();let error=import(&mixed).unwrap_err();assert!(error.starts_with(UNSUPPORTED)&&error.contains("mixed native semantic field"),"{error}");assert_eq!(mixed,before);
}

#[test]
fn caption_block_author_axes_base_and_flags_keep_explicit_title_overrides(){
    let source="| Cell |\n|---|\n| Value |\n\n<!-- readmd:layout v1 {\"id\":\"caption-axes\",\"text\":{\"author\":{\"size\":30,\"sizeCs\":10,\"cs\":true,\"rtl\":true}}} -->\n\n:label[tbl:axes] <span data-readmd-text=\"v1\" data-size-pt=\"20\" data-size-cs-pt=\"12\" data-cs=\"false\" data-rtl=\"false\">TitleOverride</span> <span data-readmd-text=\"v1\" data-cs=\"false\" data-rtl=\"false\">FlagsOnly</span>\n";
    let package=output(source,&json!({}));let xml=root(&package.parts,"word/document.xml");assert_eq!(pair(&xml,"1"),(Some(60),Some(20)));assert_eq!(flag(run(&xml,"1"),"w:cs"),Some(true));assert_eq!(flag(run(&xml,"1"),"w:rtl"),Some(true));assert_eq!(pair(&xml,"TitleOverride"),(Some(40),Some(24)));for title in ["TitleOverride","FlagsOnly"]{assert_eq!(flag(run(&xml,title),"w:cs"),Some(false));assert_eq!(flag(run(&xml,title),"w:rtl"),Some(false));}
    assert_eq!(pair(&xml,"FlagsOnly"),(Some(60),Some(20)));assert!(!word(&package.parts).contains(crate::docx_local_style::AUTHOR_AXES_ATTRIBUTE));
    for (legacy,expected) in [("",(18,22)),("\"size\":18,",(36,36))]{
        let source=format!("| C |\n|---|\n| V |\n\n<!-- readmd:layout v1 {{\"id\":\"caption-base\",\"text\":{{{legacy}\"author\":{{\"size\":\"base\",\"sizeCs\":\"base\",\"cs\":false,\"rtl\":false}}}}}} -->\n\n:label[tbl:base] BaseTitle\n");let package=output(&source,&json!({}));assert_caption_run_axes(&package.parts,"tbl:base","Table 1"," — BaseTitle",(Some(expected.0),Some(expected.1),Some(false),Some(false)),None);
    }
    let malformed=format!(r#"<w:p><w:r {}="private:4"><w:rPr><w:sz w:val="60"/></w:rPr><w:t>Forgery</w:t></w:r></w:p>"#,crate::docx_local_style::AUTHOR_AXES_ATTRIBUTE);assert_eq!(crate::docx_local_style::preserve_author_axes(&malformed,"private",|source|Ok(source.into())),Err(INVALID));assert_eq!(crate::docx_local_style::preserve_author_axes(&malformed,"wrong",|source|Ok(source.into())),Err(INVALID));
}

#[test]
fn caption_inline_base_resets_to_effective_author_pair_before_native_run_output(){
    for (legacy,author,expected) in [
        ("",json!({"size":30,"sizeCs":10}),(60,20)),
        ("\"size\":17.5,",json!({"size":30}),(60,35)),
    ]{
        let source=format!("| C |\n|---|\n| V |\n\n<!-- readmd:layout v1 {{\"id\":\"authored-reset\",\"text\":{{{legacy}\"author\":{author}}}}} -->\n\n:label[tbl:authored-reset] <span data-readmd-text=\"v1\" data-size-pt=\"45\" data-size-cs-pt=\"18\">INNER <span data-readmd-text=\"v1\" data-size-pt=\"base\" data-size-cs-pt=\"base\">RESET</span></span>\n");
        let package=output(&source,&json!({}));let xml=root(&package.parts,"word/document.xml");
        assert_eq!(pair(&xml,"INNER "),(Some(90),Some(36)));
        assert_eq!(pair(&xml,"RESET"),(Some(expected.0),Some(expected.1)));
        assert_eq!(pair(&xml,"1"),(Some(expected.0),Some(expected.1)));
        assert!(!word(&package.parts).contains(crate::docx_local_style::AUTHOR_AXES_ATTRIBUTE));
    }
}

#[test]
fn semantic_note_controls_use_actual_note_base_but_keep_distinct_current_author_axes(){
    let styles=r#"<w:style w:type="paragraph" w:styleId="FootnoteText"><w:rPr><w:sz w:val="20"/><w:szCs w:val="20"/></w:rPr></w:style>"#;
    let body=r#"<w:p><w:r><w:t>Body</w:t><w:footnoteReference w:id="1"/></w:r></w:p>"#;
    for (property,changed) in [("",false),(r#"<w:sz w:val="60"/>"#,true)]{
        let foot=format!(r#"<w:footnote w:id="1"><w:p><w:pPr><w:pStyle w:val="FootnoteText"/></w:pPr><w:sdt><w:sdtContent><w:r><w:rPr>{property}</w:rPr><w:t>Native note control</w:t></w:r></w:sdtContent></w:sdt></w:p></w:footnote>"#);
        let parts=package(body,styles,&foot,"","");let document=root(&parts,"word/document.xml");let body=child(&document,"w:body").unwrap();let rels=docx_notes::relationships(&parts,"word/document.xml").unwrap();let notes=docx_notes::Notes::load(&parts,body,&rels).unwrap();let index=RunStyleIndex::build(&parts,&rels,body,&notes).unwrap();let stories=notes.settings_stories();let story=stories.iter().find(|story|!story.end).unwrap();let control=story.root.descendants().into_iter().find(|node|node.name=="w:sdt").unwrap();
        // This unit checks the baseline contract of the already-identified
        // semantic-control hook; citation identity is covered by its own suite.
        let source="[@42]";let result=index.semantic_source(story.part,control,None,source).unwrap();
        if changed{let expected=AuthorTextStyle::from_value(&json!({"size":30})).unwrap().wrap_source(source);assert_eq!(result,expected,"the distinct western axis remains editable; CS still uses the actual note10pt base");}
        else{assert_eq!(result,source,"an unchanged note control must not gain an unnecessary HTML span");let expected=AuthorTextStyle::from_value(&json!({"size":10,"sizeCs":10})).unwrap().wrap_source(source);assert_eq!(index.semantic_source(story.part,control,Some((18,18)),source).unwrap(),expected,"a caller-supplied baseline takes precedence over the note template");}
    }
}

fn oracle_caption_xml(tail:&str)->String{
    let bookmark=crate::docx_writer::bookmark_name("tbl:oracle");
    format!(r#"<w:document xmlns:w="{W}"><w:body><w:p><w:bookmarkStart w:id="73" w:name="{bookmark}"/><w:r><w:t xml:space="preserve">Table </w:t></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> = </w:instrText></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> SEQ Table </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r><w:r><w:instrText> + 3 \* ARABIC </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>4</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r><w:bookmarkEnd w:id="73"/>{tail}</w:p><w:p><w:fldSimple w:instr=" REF {bookmark} \h "><w:r><w:t>Table 4</w:t></w:r></w:fldSimple></w:p></w:body></w:document>"#)
}

#[test]
fn caption_oracle_accepts_merged_and_split_runs_but_rejects_missing_duplicate_and_moved_titles(){
    let merged=r#"<w:r><w:t xml:space="preserve"> — NativeTitle</w:t></w:r>"#;
    let split=r#"<w:r><w:t xml:space="preserve"> — </w:t></w:r><w:r><w:t>Native</w:t></w:r><w:r><w:t>Title</w:t></w:r>"#;
    for (tail,count) in [(merged,1),(split,3)]{let xml=docx_notes::xml(oracle_caption_xml(tail).as_bytes()).unwrap();assert_table_offset_field_identity(&xml,"tbl:oracle");let runs=caption_runs(&xml,"tbl:oracle","Table 4"," — NativeTitle").unwrap();assert_eq!(runs.tail.len(),count);assert_eq!(runs.label.iter().map(|part|part.text.as_str()).collect::<String>(),"Table 4","the nested SEQ instruction cache1 must not appear as visible outer result14");}
    let valid=oracle_caption_xml(merged);let missing=oracle_caption_xml("");let duplicate=oracle_caption_xml(&merged.repeat(2));let moved=missing.replace(r#"<w:bookmarkEnd w:id="73"/>"#,&format!(r#"{merged}<w:bookmarkEnd w:id="73"/>"#));let before=missing.replace(r#"<w:bookmarkStart w:id="73""#,&format!(r#"{merged}<w:bookmarkStart w:id="73""#));let wrong=valid.replace("<w:t>4</w:t>","<w:t>9</w:t>");
    for (kind,source) in [("missing",missing),("duplicate",duplicate),("inside bookmark",moved),("before bookmark",before),("wrong current cache",wrong)]{let xml=docx_notes::xml(source.as_bytes()).unwrap();assert!(caption_runs(&xml,"tbl:oracle","Table 4"," — NativeTitle").is_err(),"{kind} must fail the complete native-region oracle");}
}

#[test]
fn caption_contributor_oracle_rejects_single_axis_or_false_route_changes_without_text_changes(){
    let source="| C |\n|---|\n| V |\n\n<!-- readmd:layout v1 {\"id\":\"format-control\",\"text\":{\"author\":{\"size\":11,\"sizeCs\":12,\"cs\":false,\"rtl\":false}}} -->\n\n:label[tbl:format] BaseTitle\n";
    let package=output(source,&json!({}));let expected=(Some(22),Some(24),Some(false),Some(false));assert_caption_run_axes(&package.parts,"tbl:format","Table 1"," — BaseTitle",expected,None);
    for (tag,value) in [("w:sz","23"),("w:szCs","25"),("w:cs","1"),("w:rtl","1")]{
        let mut document=root(&package.parts,"word/document.xml");let target={let runs=caption_runs(&document,"tbl:format","Table 1"," — BaseTitle").unwrap();runs.tail[0].run as *const XmlNode as usize};
        fn alter_node(node:&mut XmlNode,target:usize,tag:&str,value:&str)->bool{
            if node as *const XmlNode as usize==target{let properties=node.children.iter_mut().find(|node|node.name=="w:rPr").unwrap();let property=properties.children.iter_mut().find(|node|node.name==tag).unwrap();property.attrs.iter_mut().find(|(key,_)|key=="w:val").unwrap().1=value.into();return true;}
            node.children.iter_mut().any(|node|alter_node(node,target,tag,value))
        }
        assert!(alter_node(&mut document,target,tag,value));assert!(caption_runs(&document,"tbl:format","Table 1"," — BaseTitle").is_ok(),"text and bookmark regions remain exact");
        let mut changed=package.parts.clone();changed.iter_mut().find(|(name,_)|name=="word/document.xml").unwrap().1=super::super::docx_xml_fragment(&document,&[]).into_bytes();
        assert!(std::panic::catch_unwind(||assert_caption_run_axes(&changed,"tbl:format","Table 1"," — BaseTitle",expected,None)).is_err(),"{tag} alone must be rejected");
    }
}


#[test]
fn generated_note_recovery_checks_every_author_axis_and_explicit_routing_in_both_stories(){
    for end in [false,true]{
        let (kind,style,marker)=if end{("endnote","EndnoteText","endnoteRef")}else{("footnote","FootnoteText","footnoteRef")};
        let styles=format!(r#"<w:style w:type="paragraph" w:styleId="{style}"><w:rPr><w:sz w:val="60"/><w:szCs w:val="24"/></w:rPr></w:style>"#);
        for (properties,expected)in [("",true),(r#"<w:sz w:val="60"/><w:szCs w:val="24"/>"#,true),(r#"<w:sz w:val="20"/>"#,false),(r#"<w:szCs w:val="20"/>"#,false),(r#"<w:cs w:val="false"/>"#,false),(r#"<w:rtl w:val="false"/>"#,false),(r#"<w:cs/>"#,false),(r#"<w:rtl/>"#,false)]{
            let story=format!(r#"<w:{kind} w:id="1"><w:p><w:pPr><w:pStyle w:val="{style}"/></w:pPr><w:r><w:{marker}/></w:r><w:r><w:t xml:space="preserve"> </w:t></w:r><w:r><w:t>Same</w:t></w:r><w:r><w:rPr>{properties}</w:rPr><w:t>Same</w:t></w:r></w:p></w:{kind}>"#);
            let body=format!(r#"<w:p><w:r><w:t>Body</w:t><w:{kind}Reference w:id="1"/></w:r></w:p>"#);let parts=package(&body,&styles,if end{""}else{&story},if end{&story}else{""},"");
            let document=root(&parts,"word/document.xml");let body=child(&document,"w:body").unwrap();let rels=docx_notes::relationships(&parts,"word/document.xml").unwrap();let notes=docx_notes::Notes::load(&parts,body,&rels).unwrap();let index=RunStyleIndex::build(&parts,&rels,body,&notes).unwrap();let (part,note)=notes.settings_note(end,1).unwrap();let work=index.stats.work;
            assert_eq!(index.generated_note_author_reproducible(part,note).unwrap(),expected,"{kind}: {properties}");assert_eq!(index.stats.work,work,"the restore gate must reuse the existing index instead of reparsing styles");assert!(notes.settings_note(!end,1).is_none(),"same numeric id never crosses stories");
        }
    }
}

#[test]
fn generated_note_marker_and_gap_must_be_reproducible_instead_of_discarding_their_format(){
    for (marker_properties,gap_properties)in [(r#"<w:sz w:val="20"/>"#,""),("",r#"<w:szCs w:val="20"/>"#),("",r#"<w:cs w:val="false"/>"#),("",r#"<w:rtl/>"#)]{
        let styles=r#"<w:style w:type="paragraph" w:styleId="FootnoteText"><w:rPr><w:sz w:val="60"/><w:szCs w:val="24"/></w:rPr></w:style>"#;
        let story=format!(r#"<w:footnote w:id="1"><w:p><w:pPr><w:pStyle w:val="FootnoteText"/></w:pPr><w:r><w:rPr>{marker_properties}</w:rPr><w:footnoteRef/></w:r><w:r><w:rPr>{gap_properties}</w:rPr><w:t xml:space="preserve"> </w:t></w:r><w:r><w:t>Current body</w:t></w:r></w:p></w:footnote>"#);
        let parts=package(r#"<w:p><w:r><w:footnoteReference w:id="1"/></w:r></w:p>"#,styles,&story,"","");let document=root(&parts,"word/document.xml");let body=child(&document,"w:body").unwrap();let rels=docx_notes::relationships(&parts,"word/document.xml").unwrap();let notes=docx_notes::Notes::load(&parts,body,&rels).unwrap();let index=RunStyleIndex::build(&parts,&rels,body,&notes).unwrap();let (part,note)=notes.settings_note(false,1).unwrap();let error=index.generated_note_author_reproducible(part,note).unwrap_err();assert_eq!(error.code,UNSUPPORTED);assert_eq!(error.part,"word/footnotes.xml");assert!(error.detail.contains("marker or gap"),"{error}");
    }
}

#[test]
fn authored_generated_note_qualification_uses_uniform_native_axes_and_actual_control_identity(){
    for end in [false,true]{
        let (kind,style,marker)=if end{("endnote","EndnoteText","endnoteRef")}else{("footnote","FootnoteText","footnoteRef")};
        let styles=format!(r#"<w:style w:type="paragraph" w:styleId="{style}"><w:rPr><w:sz w:val="60"/><w:szCs w:val="24"/></w:rPr></w:style>"#);
        let properties=r#"<w:sz w:val="20"/><w:szCs w:val="22"/><w:cs w:val="0"/><w:rtl w:val="false"/>"#;
        let story=format!(r#"<w:{kind} w:id="1"><w:p><w:pPr><w:pStyle w:val="{style}"/></w:pPr><w:bookmarkStart w:id="8" w:name="native"/><w:r><w:{marker}/></w:r><w:bookmarkEnd w:id="8"/><w:r><w:t xml:space="preserve"> </w:t></w:r><w:sdt><w:sdtPr/><w:sdtContent><w:r><w:rPr>{properties}</w:rPr><w:t>Same</w:t></w:r><w:hyperlink r:id="live"><w:r><w:rPr>{properties}</w:rPr><w:t>Same</w:t></w:r></w:hyperlink></w:sdtContent></w:sdt></w:p></w:{kind}>"#);
        let body=format!(r#"<w:p><w:r><w:{kind}Reference w:id="1"/></w:r></w:p>"#);
        let parts=package(&body,&styles,if end{""}else{&story},if end{&story}else{""},"");
        let document=root(&parts,"word/document.xml");let body=child(&document,"w:body").unwrap();let rels=docx_notes::relationships(&parts,"word/document.xml").unwrap();let notes=docx_notes::Notes::load(&parts,body,&rels).unwrap();let index=RunStyleIndex::build(&parts,&rels,body,&notes).unwrap();let (part,note)=notes.settings_note(end,1).unwrap();let control=note.descendants().into_iter().find(|node|node.name=="w:sdt").unwrap();let before=index.stats.work;
        assert!(!index.generated_note_author_reproducible(part,note).unwrap());
        let style=index.generated_note_authored_style(part,note,control).unwrap().unwrap();
        assert_eq!(style.to_value(),json!({"size":10,"sizeCs":11,"cs":false,"rtl":false}));
        assert_eq!(index.semantic_source(part,control,None,"[@42]").unwrap(),style.wrap_source("[@42]"));
        let cloned_control=(*control).clone();assert!(index.generated_note_authored_style(part,note,&cloned_control).unwrap().is_none(),"an equal clone is not the indexed native control");
        assert!(index.generated_note_authored_style(if end{"word/footnotes.xml"}else{"word/endnotes.xml"},note,control).is_err(),"equal numeric ids never borrow another story's index");
        assert_eq!(index.stats.work,before,"the gate must not parse styles or rebuild the native index");
    }
}

#[test]
fn authored_generated_note_qualification_rejects_mixed_axes_extra_content_and_unknown_fields(){
    let styles=r#"<w:style w:type="paragraph" w:styleId="FootnoteText"><w:rPr><w:sz w:val="60"/><w:szCs w:val="24"/></w:rPr></w:style>"#;
    let one=r#"<w:r><w:rPr><w:sz w:val="20"/></w:rPr><w:t>Same</w:t></w:r>"#;
    let different=r#"<w:r><w:rPr><w:sz w:val="22"/></w:rPr><w:t>Same</w:t></w:r>"#;
    for (content,tail,gap) in [
        (format!("{one}{different}"),"",r#"<w:r><w:t xml:space="preserve"> </w:t></w:r>"#),
        (one.to_string(),"<w:r><w:t>outside control</w:t></w:r>",r#"<w:r><w:t xml:space="preserve"> </w:t></w:r>"#),
        (format!(r#"<w:fldSimple w:instr=" DATE ">{one}</w:fldSimple>"#),"",r#"<w:r><w:t xml:space="preserve"> </w:t></w:r>"#),
        (one.to_string(),"",r#"<w:r><w:rPr/><w:t xml:space="preserve"> </w:t></w:r>"#),
    ]{
        let story=format!(r#"<w:footnote w:id="1"><w:p><w:pPr><w:pStyle w:val="FootnoteText"/></w:pPr><w:r><w:footnoteRef/></w:r>{gap}<w:sdt><w:sdtPr/><w:sdtContent>{content}</w:sdtContent></w:sdt>{tail}</w:p></w:footnote>"#);
        let parts=package(r#"<w:p><w:r><w:footnoteReference w:id="1"/></w:r></w:p>"#,styles,&story,"","");
        let document=root(&parts,"word/document.xml");let body=child(&document,"w:body").unwrap();let rels=docx_notes::relationships(&parts,"word/document.xml").unwrap();let notes=docx_notes::Notes::load(&parts,body,&rels).unwrap();let index=RunStyleIndex::build(&parts,&rels,body,&notes).unwrap();let (part,note)=notes.settings_note(false,1).unwrap();let control=note.descendants().into_iter().find(|node|node.name=="w:sdt").unwrap();
        assert!(index.generated_note_authored_style(part,note,control).unwrap().is_none(),"{content} / {tail} / {gap}");
    }
}
