//! Real OPC XML and pre-resource refusal, independently of real Word shaping.
use super::*;
use crate::math_document::{parse_xml,XmlNode};
use serde_json::json;
use std::collections::BTreeMap;

fn package(source:&str,options:&Value)->BTreeMap<String,Vec<u8>> {
    let result=crate::docx_writer::build(&crate::md_ast::parse(source),options,"Baseline QA",&|_|None);
    assert!(!result.parts.is_empty(),"{:?}",result.warns);
    let entries=result.parts.into_iter().map(|(path,data)|crate::mdexport::ZipEntry {path,data,compress:true}).collect::<Vec<_>>();
    crate::publication_typography_docx_tests::unzip_bytes(&crate::mdexport::write_zip_at(&entries,0,0).unwrap())
}
fn xml(parts:&BTreeMap<String,Vec<u8>>,name:&str)->XmlNode {parse_xml(std::str::from_utf8(&parts[name]).unwrap()).unwrap()}
fn elements<'a>(node:&'a XmlNode,name:&str)->Vec<&'a XmlNode> {
    let mut found=Vec::new();if node.name==name {found.push(node);}for child in node.children(){found.extend(elements(child,name));}found
}
fn paragraph<'a>(root:&'a XmlNode,marker:&str)->&'a XmlNode {
    let found=elements(root,"w:p").into_iter().filter(|p|p.text().starts_with(marker)).collect::<Vec<_>>();
    assert_eq!(found.len(),1,"{marker}");found[0]
}
fn position(run:&XmlNode)->Option<i32> {
    let values=run.children().filter(|n|n.name=="w:rPr").flat_map(|n|n.children()).filter(|n|n.name=="w:position").collect::<Vec<_>>();
    assert!(values.len()<=1);values.first().map(|n|n.attributes["w:val"].parse().unwrap())
}

#[test]
fn p04_baseline_docx_position_preserves_sizes_text_and_math_and_clears_with_explicit_zero() {
    let source="# HEADING\n\nBODY **BOLD** *ITALIC* [LINK](https://example.invalid) `CODE` $x^2$.\n\n<!-- readmd:layout v1 {\"id\":\"negative\",\"text\":{\"baselineShiftPt\":-2.5}} -->\n\nNEGATIVE glyphs.\n\n<!-- readmd:layout v1 {\"id\":\"zero\",\"text\":{\"baselineShiftPt\":0}} -->\n\nZERO reset.\n\nRESTORED parent.\n\n> QUOTE\n\n- LIST\n\n[^note]: FOOTNOTE\n";
    let old=json!({"frontMatter":{"mode":"none"},"typography":{"size":12,"spacing":9}});
    let mut shifted=old.clone();shifted["typography"]["baselineShiftPt"]=json!(3.24);
    let control=package(source,&old);let actual=package(source,&shifted);let doc=xml(&actual,"word/document.xml");
    for (marker,expected) in [("BODY",6),("NEGATIVE",-5),("ZERO",0),("RESTORED",6)] {
        for run in elements(paragraph(&doc,marker),"w:r").into_iter().filter(|r|r.children().any(|n|n.name=="w:t")) {
            assert_eq!(position(run),if run.text()=="CODE"{None}else{Some(expected)},"{}",run.xml());
        }
    }
    for marker in ["HEADING","QUOTE","LIST"] {assert!(elements(paragraph(&doc,marker),"w:r").iter().all(|r|position(r).is_none()));}
    let before=xml(&control,"word/document.xml");
    assert_eq!(elements(&doc,"m:oMath").iter().map(|n|n.xml()).collect::<Vec<_>>(),elements(&before,"m:oMath").iter().map(|n|n.xml()).collect::<Vec<_>>());
    for tag in ["w:t","w:sz","w:szCs"] {assert_eq!(elements(&doc,tag).iter().map(|n|n.xml()).collect::<Vec<_>>(),elements(&before,tag).iter().map(|n|n.xml()).collect::<Vec<_>>());}
    for name in ["word/styles.xml","word/settings.xml","word/numbering.xml"] {assert_eq!(actual[name],control[name]);}
    assert!(!doc.xml().contains("baselineShiftPt"));
}

#[test]
fn p04_baseline_docx_half_point_ties_are_signed_and_source_precision_survives() {
    for (requested,half) in [(-0.25,-1),(-0.249,0),(0.249,0),(0.25,1)] {
        let source=format!("<!-- readmd:layout v1 {} -->\n\nQUANTISED.\n",json!({"id":"q","text":{"baselineShiftPt":requested}}));
        let actual=package(&source,&json!({"frontMatter":{"mode":"none"}}));let doc=xml(&actual,"word/document.xml");
        assert_eq!(position(elements(paragraph(&doc,"QUANTISED"),"w:r")[0]),Some(half));
        assert_eq!(crate::publication_blocks::analyze(&source).unwrap().nodes[0].style["text"]["baselineShiftPt"],requested);
    }
}

#[test]
fn p04_baseline_docx_invalid_and_unverified_semantic_combination_fail_before_images() {
    use crate::md_ast::{Block,Inline};use std::cell::Cell;
    let calls=Cell::new(0);let loader=|_:&str|{calls.set(calls.get()+1);None};
    let mut doc=crate::md_ast::parse("![owned](not-loaded.png)");
    for value in [json!("3"),json!(true),json!(-72.001),json!(72.001)] {
        let result=crate::docx_writer::build(&doc,&json!({"typography":{"baselineShiftPt":value}}),"QA",&loader);
        assert!(result.parts.is_empty());assert_eq!(result.warns,vec![INVALID.to_string()]);
    }
    let mut format=crate::publication_rich::RichFormat::default();format.vertical_align=crate::publication_rich::RichVerticalAlign::Sup;
    doc.blocks.push(Block::Paragraph(vec![Inline::Styled{format,children:vec![Inline::Text("2".into())]}]));
    let result=crate::docx_writer::build(&doc,&json!({"typography":{"baselineShiftPt":3}}),"QA",&loader);
    assert!(result.parts.is_empty());assert_eq!(result.warns,vec![UNSUPPORTED.to_string()]);assert_eq!(calls.get(),0);
}

#[test]
fn p04_baseline_unsupported_exports_preserve_target_and_never_stage_resources() {
    let root=tempfile::tempdir().unwrap();let data=root.path().join("profile-not-created");let assets=root.path().join("assets-not-created");
    for (format,engine) in [("html","rust"),("epub","rust"),("tex","rust"),("pdf","typst"),("pdf","xelatex")] {
        let target=root.path().join(format!("protected-{format}-{engine}.{format}"));std::fs::write(&target,b"OWNED ORIGINAL").unwrap();
        let request=json!({"typography":{"baselineShiftPt":0},"pdf":{"engine":engine},"bibliography":"missing-owned.json"});
        let error=crate::mdexport::export_document_with_data(format,"Body [@missing].",root.path().to_str().unwrap(),target.to_str().unwrap(),&request,"QA",&assets,&data).unwrap_err();
        assert_eq!(error,UNSUPPORTED);assert_eq!(std::fs::read(&target).unwrap(),b"OWNED ORIGINAL");
    }
    assert!(!data.exists()&&!assets.exists());assert!(!std::fs::read_dir(root.path()).unwrap().any(|e|e.unwrap().file_name().to_string_lossy().contains("readmd-part")));
}
