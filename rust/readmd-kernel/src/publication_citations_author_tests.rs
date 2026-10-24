use super::*;
use crate::math_document::{XmlNode,WORD_NS};
use crate::md_ast::{self,Inline};
use serde_json::json;

const OPEN:&str="<span data-readmd-text=\"v1\" data-size-pt=\"30\" data-size-cs-pt=\"19\">";

fn assert_source_ranges(source:&str,citations:&[SourceCitation]){
    for citation in citations{
        for span in std::iter::once(&citation.span).chain(citation.items.iter().flat_map(|item|std::iter::once(&item.span).chain(item.fields.values()))){
            assert_eq!(&source[span.byte_from..span.byte_to],span.source);
            assert_eq!(crate::document_transaction::byte_offset(source,span.from).unwrap(),span.byte_from);
            assert_eq!(crate::document_transaction::byte_offset(source,span.to).unwrap(),span.byte_to);
        }
    }
}

#[test]
fn validated_author_edges_keep_body_first_nested_and_duplicate_citations_source_bound(){
    for prefix in ["","\u{feff}"]{
        for body in [
            format!("😀 Body {OPEN}[see @42, p. 12 **tail**]</span>."),
            format!("{OPEN}[@42]</span> Σ tail."),
            format!("<SPAN DATA-READMD-TEXT=\"v1\" DATA-SIZE-PT=\"30\">Outer <span data-readmd-text=\"v1\" data-size-cs-pt=\"19\">[@42]</span> Σ</SPAN>"),
            format!("{OPEN}[@42] and [@42]</span>"),
        ]{
            let source=format!("{prefix}{body}");
            let scanned=scan(&source,"en-US").unwrap();
            assert!(scanned.diagnostics.is_empty(),"{:?}",scanned.diagnostics);
            let expected=if body.contains(" and "){2}else{1};
            assert_eq!(scanned.citations.len(),expected,"{source}");
            assert!(scanned.citations.iter().all(|citation|citation.items[0].cite.key=="42"));
            assert_source_ranges(&source,&scanned.citations);
            if expected==2{assert_ne!(scanned.citations[0].span.byte_from,scanned.citations[1].span.byte_from);}
        }
    }
}

#[test]
fn validated_author_edges_do_not_activate_ordinary_html_code_math_images_or_links(){
    let source=format!(r#"{OPEN}[@live]</span>

<span class="ordinary">[@ordinary]</span>

{OPEN}<em>[@html_inside]</em> `[@code_inside]`</span>

<code>{OPEN}[@raw_code]</span></code>

`<span data-readmd-text="v1" data-size-pt="invalid">[@code_marker]</span>`

```md
{OPEN}[@fenced]</span>
```

![literal @image](/owned.png) [@linked](https://example.test/@path) $[@math]$ \@escaped
"#);
    let scanned=scan(&source,"en-US").unwrap();
    assert!(scanned.diagnostics.is_empty(),"{:?}",scanned.diagnostics);
    assert_eq!(scanned.citations.iter().map(|citation|citation.items[0].cite.key.as_str()).collect::<Vec<_>>(),vec!["live"]);
    assert_source_ranges(&source,&scanned.citations);
}

#[test]
fn malformed_owned_source_is_typed_in_scan_and_reading_order(){
    for source in [
        "A <span data-readmd-text=\"v1\" data-size-pt=\"30\">[@42]",
        "A <span data-readmd-text=\"v1\" data-size-pt=\"0\">[@42]</span>",
        "A <span data-readmd-text=\"v1\" onclick=\"bad()\">[@42]</span>",
    ]{
        assert_eq!(scan(source,"en-US").err().unwrap().code,crate::publication_inline_text::INVALID);
        assert_eq!(reading_order(source,Vec::new(),false,&HashSet::new()).err().unwrap().code,crate::publication_inline_text::INVALID);
    }
}

#[test]
fn ordinary_scanning_retains_legacy_events_beyond_the_author_event_budget(){
    let source=format!("{} [@42]","*ordinary* ".repeat(crate::publication_inline_text::MAX_NODES*5));
    assert!(!crate::publication_inline_text::has_owned_syntax(&source));
    let limit=crate::publication_inline_text::MAX_NODES*16;
    let(events,edges)=source_events(&source).unwrap();
    assert!(edges.is_empty());
    assert_eq!(events.take(limit+1).count(),limit+1,"ordinary source must not gain the author parser's event limit");
    let scanned=scan(&source,"en-US").unwrap();assert_eq!(scanned.citations.len(),1);
    assert_source_ranges(&source,&scanned.citations);
}

fn fixture(body:&str)->(tempfile::TempDir,String){
    let dir=tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("refs.json"),r#"[{"id":42,"type":"book","title":"FIRST_TITLE"},{"id":"same","type":"book","title":"SECOND_TITLE"}]"#).unwrap();
    std::fs::write(dir.path().join("house.csl"),r#"<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text"><info><title>Owned citation scope</title><id>urn:readmd:test:owned-citation</id></info><citation><layout><text variable="title"/></layout></citation></style>"#).unwrap();
    (dir,format!("---\nbibliography: refs.json\ncsl: house.csl\nlang: en-US\n---\n{body}\n"))
}

fn word_nodes<'a>(node:&'a XmlNode,local:&str)->Vec<&'a XmlNode>{
    fn collect<'a>(node:&'a XmlNode,local:&str,out:&mut Vec<&'a XmlNode>){
        if node.namespace==WORD_NS&&node.local()==local{out.push(node);}
        for child in node.children(){collect(child,local,out);}
    }
    let mut out=Vec::new();collect(node,local,&mut out);out
}
fn property<'a>(run:&'a XmlNode,local:&str)->Option<&'a str>{
    run.children().find(|node|node.namespace==WORD_NS&&node.local()=="rPr")?
        .children().find(|node|node.namespace==WORD_NS&&node.local()==local)?
        .attributes.get("w:val").map(String::as_str)
}
fn native_citations(source:&str,base:&str)->(crate::docx_writer::DocxPackage,crate::publication_docx_citations::Manifest){
    let prepared=crate::publication::prepare(source,base).unwrap();
    let scanned=scan(source,"en-US").unwrap();
    assert_eq!(prepared.projection.citations.len(),scanned.citations.len());
    let document=md_ast::parse_prepared(source,&prepared.projection).unwrap();
    let mut sources=Vec::new();
    md_ast::walk_inlines(&document.blocks,&mut|inline|if let Inline::Citation{source,..}=inline{sources.push(source.clone());});
    assert_eq!(sources.len(),scanned.citations.len(),"projection must create a real editable citation for every scanned source");
    assert_source_ranges(source,&sources);
    let options=json!({"frontMatter":{"mode":"none"}});
    let mut package=crate::docx_writer::build(&document,&options,"OwnedCitation.md",&|_|None);
    assert!(!package.parts.is_empty(),"{:?}",package.warns);
    crate::publication_docx_citations::attach_context(&mut package.parts,source,base,&options).unwrap();
    assert!(package.parts.iter().all(|(_,bytes)|!String::from_utf8_lossy(bytes).contains(crate::docx_local_style::AUTHOR_AXES_ATTRIBUTE)));
    let manifest=crate::publication_docx_citations::Manifest::parse(&package.parts.iter().find(|(part,_)|part==crate::publication_docx_citations::PART).unwrap().1).unwrap();
    assert_eq!(manifest.source_revision,crate::document_transaction::revision(source));
    let records=manifest.records.iter().filter(|record|record.kind=="citation").collect::<Vec<_>>();
    assert_eq!(records.len(),sources.len());
    assert_eq!(records.iter().map(|record|&record.id).collect::<HashSet<_>>().len(),records.len());
    for record in records{
        assert!(sources.iter().any(|source|Some(&source.span)==record.span.as_ref()&&source.span.source==record.original_source));
    }
    (package,manifest)
}
fn assert_native_control(package:&crate::docx_writer::DocxPackage,record:&crate::publication_docx_citations::Record,size:u32,size_cs:u32,false_routes:bool){
    let bytes=&package.parts.iter().find(|(part,_)|part==&record.part).unwrap().1;
    let root=crate::math_document::parse_xml(std::str::from_utf8(bytes).unwrap()).unwrap();
    let controls=word_nodes(&root,"sdt").into_iter().filter(|node|word_nodes(node,"tag").into_iter().any(|tag|tag.attributes.get("w:val")==Some(&record.tag()))).collect::<Vec<_>>();
    assert_eq!(controls.len(),1,"each citation identity must bind exactly one native content control");
    let texts=word_nodes(controls[0],"t").into_iter().map(XmlNode::text).collect::<String>();
    assert_eq!(texts,if record.items[0].key=="42"{"FIRST_TITLE"}else{"SECOND_TITLE"});
    let runs=word_nodes(controls[0],"r").into_iter().filter(|run|word_nodes(run,"t").into_iter().any(|text|!text.text().is_empty())).collect::<Vec<_>>();
    assert!(!runs.is_empty());
    for run in runs{
        assert_eq!(property(run,"sz").unwrap().parse::<u32>().unwrap(),size);
        assert_eq!(property(run,"szCs").unwrap().parse::<u32>().unwrap(),size_cs);
        if false_routes{assert_eq!(property(run,"cs"),Some("0"));assert_eq!(property(run,"rtl"),Some("0"));}
    }
}

#[test]
fn prepared_author_citations_write_real_docx_controls_with_current_sizes_and_source_identity(){
    for(prefix,body,nested)in[
        ("",format!("Body {OPEN}[@42]</span>."),false),
        ("\u{feff}",format!("{OPEN}[@42]</span> 😀 Σ tail."),false),
        ("",String::from("<span data-readmd-text=\"v1\" data-size-pt=\"30\">Outer <span data-readmd-text=\"v1\" data-size-cs-pt=\"19\" data-cs=\"false\" data-rtl=\"false\">[@42]</span> tail</span>"),true),
    ]{
        let(dir,source)=fixture(&body);let source=format!("{prefix}{source}");
        let(package,manifest)=native_citations(&source,dir.path().to_str().unwrap());
        let records=manifest.records.iter().filter(|record|record.kind=="citation").collect::<Vec<_>>();
        assert_eq!(records.len(),1);
        assert_native_control(&package,records[0],60,38,nested);
    }
}

#[test]
fn author_scopes_preserve_manual_note_reading_order_duplicate_citations_and_native_note_identity(){
    let body=format!("{OPEN}Manual[^n] again[^n].</span> Then {OPEN}[@42] and [@42]</span>.\n\n[^n]: <span data-readmd-text=\"v1\" data-size-pt=\"15\" data-size-cs-pt=\"19\">Authored [@same].</span>");
    let(dir,source)=fixture(&body);let base=dir.path().to_str().unwrap();
    let scanned=scan(&source,"en-US").unwrap();assert_eq!(scanned.citations.len(),3);
    let order=reading_order(&source,scanned.citations,true,&HashSet::from(["42".into(),"same".into()])).unwrap();
    assert_eq!(order.iter().map(|citation|citation.items[0].cite.key.as_str()).collect::<Vec<_>>(),vec!["same","42","42"]);
    assert!(order[0].context.in_note);assert_eq!(order[0].context.note_label.as_deref(),Some("n"));
    assert_eq!(order.iter().map(|citation|citation.context.note_number).collect::<Vec<_>>(),vec![Some(1),Some(2),Some(3)]);
    let(package,manifest)=native_citations(&source,base);
    let records=manifest.records.iter().filter(|record|record.kind=="citation").collect::<Vec<_>>();assert_eq!(records.len(),3);
    for record in records{
        let note=record.part=="word/footnotes.xml";
        assert_native_control(&package,record,if note{30}else{60},38,false);
        assert_eq!(record.original_source,if note{"[@same]"}else{"[@42]"});
    }
    let body=crate::math_document::parse_xml(std::str::from_utf8(&package.parts.iter().find(|(part,_)|part=="word/document.xml").unwrap().1).unwrap()).unwrap();
    let refs=word_nodes(&body,"footnoteReference");assert_eq!(refs.len(),2);
    assert_eq!(refs[0].attributes.get("w:id"),refs[1].attributes.get("w:id"));
    let notes=crate::math_document::parse_xml(std::str::from_utf8(&package.parts.iter().find(|(part,_)|part=="word/footnotes.xml").unwrap().1).unwrap()).unwrap();
    assert_eq!(word_nodes(&notes,"footnote").into_iter().filter(|note|!note.attributes.contains_key("w:type")).count(),1);
}

#[test]
fn preparing_owned_citations_retains_literal_exclusions_and_rejects_malformed_source(){
    let body=format!("{OPEN}[@42]</span> <span>[@same]</span> `[@same]` ![literal @same](/owned.png) [@same](https://example.test/) ");
    let(dir,source)=fixture(&body);
    let prepared=crate::publication::prepare(&source,dir.path().to_str().unwrap()).unwrap();
    assert_eq!(prepared.projection.citations.len(),1);
    assert_eq!(prepared.projection.citations[0].source.items[0].cite.key,"42");
    let document=md_ast::parse_prepared(&source,&prepared.projection).unwrap();
    let mut citations=0;md_ast::walk_inlines(&document.blocks,&mut|inline|if matches!(inline,Inline::Citation{..}){citations+=1;});assert_eq!(citations,1);
    let malformed=source.replace("data-size-pt=\"30\"","data-size-pt=\"0\"");
    assert_eq!(crate::publication::prepare(&malformed,dir.path().to_str().unwrap()).err().unwrap().code,crate::publication_inline_text::INVALID);
}
