//! Standard nocite input reaches the real publication writers and portable
//! citation resources. These checks do not claim an interactive Word edit.
use super::*;
use crate::publication_docx_citations::{Manifest, Resources, PART};
use std::fs;

const BIB: &str = r#"@book{core,title={Memory research methods},author={Chen, Lin},year={2024},publisher={Example Academic}}
@book{extra,title={Attention and learning},author={Wang, Mei},year={2023},publisher={Example Academic}}
@book{other,title={Cognitive research handbook},author={Li, Yun},year={2022},publisher={Example Academic}}
"#;

fn fixture(nocite: &str, body: &str, style: Option<&str>) -> (tempfile::TempDir, String) {
    let root=tempfile::tempdir().unwrap();
    fs::write(root.path().join("refs.bib"),BIB).unwrap();
    let csl=if let Some(style)=style {fs::write(root.path().join("house.csl"),style).unwrap();"csl: house.csl\n"}else{""};
    let source=format!("---\ntitle: 文献综述样张\nbibliography: refs.bib\nlang: zh-CN\n{csl}{nocite}---\n\n{body}\n");
    (root,source)
}
fn keys(prepared: &crate::publication::Prepared) -> Vec<String> {
    prepared.data["bibliography"]["items"].as_array().unwrap().iter()
        .map(|item|item["key"].as_str().unwrap().to_string()).collect()
}

#[test]
fn nocite_reads_pandoc_scalars_yaml_blocks_lists_and_braced_unicode_keys() {
    for field in [
        "nocite: '@core, @extra'\n",
        "nocite: |\n  @core,\n  @extra\n",
        "nocite: >\n  @core,\n  @extra\n",
        "nocite: ['@core', '@extra', '@core']\n",
        "nocite:\n  - '@core'\n  - '[@extra]'\n",
    ] {
        let source=format!("\u{feff}---\n{field}---\nBody [@other].");
        let selected=nocite(&source).unwrap().unwrap();
        assert_eq!(selected.keys,["core","extra"],"{field}");
        assert_eq!(&source[selected.span.byte_from..selected.span.byte_to],selected.span.source);
        assert_eq!(crate::document_transaction::byte_offset(&source,selected.span.from).unwrap(),selected.span.byte_from);
        assert_eq!(scan(&source,"zh-CN").unwrap().citations.len(),1,"metadata never becomes a body citation");
    }
    let source="---\nnocite: '@{中文 文献/2026}, @*; `@fake @*` \\@escaped'\n---\n";
    assert_eq!(nocite(source).unwrap().unwrap().keys,["中文 文献/2026","*"]);
    for field in ["nocite: null\n","nocite: []\n","nocite: ''\n"] {
        assert!(nocite(&format!("---\n{field}---\n")).unwrap().unwrap().keys.is_empty());
    }
    for field in ["nocite: true\n","nocite: {key: '@core'}\n","nocite: ['@core', false]\n"] {
        let error=nocite(&format!("---\n{field}---\n")).err().unwrap();
        assert_eq!(error.code,"academic_context");
    }
}

#[test]
fn nocite_adds_selected_or_all_bibliography_items_without_body_citations() {
    for (field,expected) in [
        ("nocite: '@extra'\n",vec!["extra"]),
        ("nocite: '@*'\n",vec!["core","other","extra"]),
    ] {
        let(root,source)=fixture(field,"# 延伸阅读\n\n正文没有引用标记。",None);
        let prepared=crate::publication::prepare(&source,root.path().to_str().unwrap()).unwrap();
        assert!(prepared.projection.citations.is_empty());
        assert_eq!(prepared.data["citations"],json!([]));
        assert_eq!(keys(&prepared),expected);
        assert_eq!(prepared.projection.bibliography.as_ref().unwrap().items.len(),expected.len());
        assert!(prepared.warnings.is_empty(),"{:?}",prepared.warnings);
        assert!(prepared.content.contains("正文没有引用标记。"));
        assert!(!prepared.content.contains("[^readmd-citation-"));
    }
}

#[test]
fn nocite_keeps_real_note_order_ibid_and_numeric_bibliography_source_order() {
    let style=r#"<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="note"><info><title>Nocite note order</title><id>urn:readmd:test:nocite-notes</id></info><citation><layout suffix="."><choose><if position="ibid"><text term="ibid"/></if><else><text variable="title"/></else></choose></layout></citation><bibliography><layout><text variable="citation-number" suffix=". "/><text variable="title"/></layout></bibliography></style>"#;
    let(root,source)=fixture("nocite: '@*'\n","正文 [@extra]。再引 [@extra]。",Some(style));
    let base=root.path().to_str().unwrap();
    let prepared=crate::publication::prepare(&source,base).unwrap();
    let prior=crate::publication::prepare(&source.replace("nocite: '@*'\n",""),base).unwrap();
    assert_eq!(prepared.data["citations"],prior.data["citations"],"uncited items cannot create first/ibid events or notes");
    assert_eq!(prepared.data["citations"].as_array().unwrap().len(),2);
    assert_eq!(prepared.data["citations"][0]["note_number"],1);
    assert_eq!(prepared.data["citations"][1]["note_number"],2);
    assert_eq!(keys(&prepared),["extra","core","other"],"cited items precede wildcard additions in bibliography source order");
    for (index,item) in prepared.data["bibliography"]["items"].as_array().unwrap().iter().enumerate() {
        assert!(item["text"].as_str().unwrap().starts_with(&format!("{}. ",index+1)));
    }
}

#[test]
fn nocite_unknown_keys_are_diagnosed_at_the_yaml_field_without_creating_body_notes() {
    let(root,source)=fixture("nocite: |\n  @extra, @missing, @missing\n","正文 [@core]。",None);
    let prepared=crate::publication::prepare(&source,root.path().to_str().unwrap()).unwrap();
    assert_eq!(keys(&prepared),["core","extra"]);
    let diagnostics=prepared.data["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(),1);
    let diagnostic=&diagnostics[0];
    assert_eq!(diagnostic["code"],"academic_missing_reference");
    assert_eq!(diagnostic["field"],"nocite");assert_eq!(diagnostic["key"],"missing");
    let selected=nocite(&source).unwrap().unwrap();
    assert_eq!(diagnostic["from"],selected.span.from);assert_eq!(diagnostic["to"],selected.span.to);
    assert_eq!(prepared.projection.citations.len(),1);
}

#[test]
fn nocite_real_docx_html_epub_exports_and_docx_return_keep_extended_reading() {
    let(root,source)=fixture("nocite: |\n  @extra, @other\n","# 研究札记\n\n正文引用 [@core, pp. 12–14]。另两本作为延伸阅读列入文献表。",None);
    let base=root.path().to_str().unwrap();
    let options=json!({"frontMatter":{"mode":"none"},"cover":{"enabled":false},"toc":{"enabled":false}});
    let docx=root.path().join("nocite.docx");let epub=root.path().join("nocite.epub");let html=root.path().join("nocite.html");
    crate::mdexport::export_docx(&source,base,docx.to_str().unwrap(),&options,"nocite.md").unwrap();
    crate::mdexport::export_epub(&source,base,epub.to_str().unwrap(),&options,"nocite.md").unwrap();
    crate::mdexport::export_html(&source,base,html.to_str().unwrap(),&options,"nocite.md",root.path()).unwrap();
    let parts=crate::convert::parse_zip_entries(&fs::read(&docx).unwrap()).unwrap();
    let manifest=Manifest::parse(&parts.iter().find(|(name,_)|name==PART).unwrap().1).unwrap();
    assert_eq!(manifest.resources.as_ref().unwrap().nocite,["extra","other"]);
    assert_eq!(manifest.records.iter().filter(|record|record.kind=="citation").count(),1);
    let word=String::from_utf8_lossy(&parts.iter().find(|(name,_)|name=="word/document.xml").unwrap().1);
    let html_content=fs::read_to_string(&html).unwrap();
    let epub_parts=crate::convert::parse_zip_entries(&fs::read(&epub).unwrap()).unwrap();
    let epub_content=epub_parts.iter().filter(|(name,_)|name.ends_with(".xhtml"))
        .map(|(_,bytes)|String::from_utf8_lossy(bytes)).collect::<String>();
    for title in ["Memory research methods","Attention and learning","Cognitive research handbook"] {
        for (format,content) in [("DOCX",word.as_ref()),("HTML",html_content.as_str()),("EPUB",epub_content.as_str())] {
            assert!(content.contains(title),"{format} lost {title}");
        }
    }
    let returned=crate::convert::convert(docx.to_str().unwrap()).unwrap();
    let resources=Resources::from_markdown(&returned).unwrap().unwrap();
    assert_eq!(resources.nocite,["extra","other"]);
    let reopened=crate::publication::prepare(&returned,"").unwrap();
    assert_eq!(keys(&reopened),["core","other","extra"]);
    assert_eq!(reopened.projection.citations.len(),1);
    let patch=crate::publication_metadata::patch_source_member(&returned,&crate::document_transaction::revision(&returned),&["nocite"],&json!([])).unwrap();
    let cleared_source=crate::document_transaction::apply(&returned,&patch).unwrap().content;
    let cleared=crate::publication::prepare(&cleared_source,"").unwrap();
    assert_eq!(keys(&cleared),["core"],"an explicit empty selection clears imported nocite");
    // Old resources have no nocite property and keep their original behavior.
    let mut old=serde_json::to_value(&resources).unwrap();old.as_object_mut().unwrap().remove("nocite");
    assert!(serde_json::from_value::<Resources>(old).unwrap().nocite.is_empty());
    if let Ok(output)=std::env::var("READMD_NOCITE_SAMPLE_DIR") {
        let output=std::path::Path::new(&output);fs::create_dir_all(output).unwrap();
        fs::write(output.join("nocite.md"),&source).unwrap();fs::write(output.join("refs.bib"),BIB).unwrap();
        fs::write(output.join("nocite-returned.md"),returned).unwrap();
        for file in [docx,epub,html] {fs::copy(&file,output.join(file.file_name().unwrap())).unwrap();}
    }
}
