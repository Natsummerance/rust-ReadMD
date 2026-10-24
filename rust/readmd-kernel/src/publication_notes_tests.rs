use super::*;

const SOURCE: &str = "<!-- readmd-section:v1 {\"id\":\"alpha\"} -->\n\n# First\n\nOne[^a], repeat[^a], other[^b].\n\n# Another heading\n\nStill first section[^c].\n\n<!-- readmd-section:v1 {\"id\":\"beta\",\"break\":\"continuous\"} -->\n\nSame page is allowed[^d], repeat old[^a], another[^e].\n\n[^e]: Last.\n\n[^c]: Middle.\n\n[^a]: First.\n\n[^b]: Second.\n\n[^d]: Later.\n";

#[test]
fn note_controls_are_optional_bounded_and_do_not_materialize_in_old_presets() {
    let old=crate::export_styles::sanitize(&json!({}));
    assert!(old.get("footnotes").is_none());
    let style=crate::export_styles::sanitize(&json!({"footnotes":{
        "restart":"chapter","separator":false,"continuationSeparator":"short",
        "continuationNotice":format!("A\n{}", "😀".repeat(121)),"longNote":"keepWholeNote"
    }}));
    assert_eq!(style["footnotes"]["restart"],"continuous");
    assert_eq!(style["footnotes"]["separator"],"line");
    assert_eq!(style["footnotes"]["continuationSeparator"],"line");
    assert_eq!(style["footnotes"]["longNote"],"split");
    let notice=style["footnotes"]["continuationNotice"].as_str().unwrap();
    assert_eq!(notice.chars().count(),120);
    assert!(!notice.contains('\n'));
    let layout=Layout::from_style(&style);
    assert_eq!(layout.note_restart,NoteRestart::Continuous);
    assert!(!layout.note_keep_paragraphs);
}

#[test]
fn first_reference_numbers_use_source_sections_without_heading_or_definition_order_resets() {
    let doc=crate::md_ast::parse(SOURCE);
    for (restart,expected) in [
        ("continuous",[27,28,29,30,31]),("section",[27,28,29,27,28]),("page",[27,28,29,30,31])
    ] {
        let style=json!({"footnotes":{"restart":restart,"start":27}});
        let numbers=note_numbers(&doc,&style);
        assert_eq!(["a","b","c","d","e"].map(|label|numbers[label]),expected,"{restart}");
        assert_eq!(numbers.len(),5);
        assert_eq!(note_numbers(&crate::md_ast::parse(&SOURCE.replace('\n',"\r\n")),&style),numbers);
    }
}

#[test]
fn note_to_note_reference_plans_are_bounded_and_keep_first_main_story_identity() {
    let doc=crate::md_ast::parse("First[^a], second[^c].\n\n[^a]: Nested[^b].\n\n[^b]: Cycle[^a].\n\n[^c]: Last main reference.\n");
    let numbers=note_numbers(&doc,&json!({}));
    assert_eq!((numbers["a"],numbers["c"],numbers["b"]),(1,2,3));
    assert_eq!(numbers.len(),3);
}

#[test]
fn physical_page_and_continuation_capabilities_never_substitute_chapter_behavior() {
    let page=json!({"footnotes":{"restart":"page"}});
    assert!(warnings("docx",&page).is_empty());
    assert!(warnings("epub",&page).contains(&"publication_note_page_restart_unsupported".into()));
    assert!(warnings("html",&page).contains(&"publication_note_page_restart_unsupported".into()));
    let typst=json!({"pdf":{"engine":"typst"},"footnotes":{"restart":"page"}});
    assert!(warnings("pdf",&typst).is_empty());
    let end=json!({"pdf":{"engine":"typst"},"footnotes":{"position":"document","restart":"page"}});
    assert_eq!(note_warnings("pdf",&end),vec!["publication_note_page_restart_unsupported"]);
    let section=json!({"footnotes":{"restart":"section"}});
    assert!(warnings("epub",&section).is_empty());
    assert!(warnings("html",&section).contains(&"publication_note_section_restart_unsupported".into()));
    let different=json!({"pdf":{"engine":"typst"},"footnotes":{"separator":"none","continuationSeparator":"line","continuationNotice":"Continued"}});
    assert_eq!(note_warnings("pdf",&different),vec!["publication_note_continuation_unsupported"]);
    assert_eq!(note_warnings("epub",&different),vec!["publication_note_continuation_unsupported"]);
    assert!(note_warnings("docx",&different).is_empty());
}

const EPUB_WARNING_SOURCE:&str="<!-- readmd-section:v1 {\"id\":\"warning-first\"} -->\n\n# Warning notes\n\nSOURCEFIRST[^a].\n\n<!-- readmd-section:v1 {\"id\":\"warning-second\",\"break\":\"continuous\"} -->\n\nSOURCESECOND[^b].\n\n[^a]: NATIVE_NOTE_A retained evidence.\n\n[^b]: NATIVE_NOTE_B retained evidence.\n";
fn epub_warning_options(restart:&str,position:&str)->Value{
    json!({"cover":{"enabled":false},"toc":{"enabled":false},"frontMatter":{"mode":"none"},
        "footer":{"pageNumbers":false},"page":{"columns":2},"epub":{"splitLevel":"none"},
        "footnotes":{"position":position,"restart":restart,"continuationSeparator":"line","continuationNotice":"Continued evidence"}})
}
fn assert_epub_note_warnings(warnings:&[String],restart:&str){
    assert_eq!(warnings.iter().filter(|value|value.as_str()=="publication_note_continuation_unsupported").count(),1,"{warnings:?}");
    assert_eq!(warnings.iter().filter(|value|value.as_str()=="publication_note_page_restart_unsupported").count(),usize::from(restart=="page"),"{warnings:?}");
    assert!(!warnings.iter().any(|value|value=="publication_note_section_restart_unsupported"),"{warnings:?}");
    // Sections and the requested columns independently produce this message.
    // The first existing warning stays first; only exact duplicates disappear.
    assert_eq!(warnings.first().map(String::as_str),Some("publication_layout_paged_unsupported"));
    assert_eq!(warnings.iter().filter(|value|value.as_str()=="publication_layout_paged_unsupported").count(),1);
    assert_eq!(warnings.iter().collect::<std::collections::HashSet<_>>().len(),warnings.len());
}

#[test]
fn all_epub_builders_and_export_dispatch_preserve_precise_note_losses_once(){
    let directory=tempfile::tempdir().unwrap();
    let base=directory.path().to_str().unwrap();let assets=directory.path().join("assets");let data=directory.path().join("profile");
    for restart in ["continuous","section","page"]{for position in ["page","document"]{
        let options=epub_warning_options(restart,position);
        let (entries,warnings)=crate::mdexport::epub_build_entries(&json!(EPUB_WARNING_SOURCE),base,&options,"Notes","Author","en-US","urn:uuid:note-warning-test","2026-10-20T00:00:00Z").unwrap();
        assert_epub_note_warnings(&warnings,restart);
        let text=entries.iter().filter(|entry|entry.path.ends_with(".xhtml")).map(|entry|String::from_utf8_lossy(&entry.data)).collect::<Vec<_>>().join("\n");
        assert!(text.contains("NATIVE_NOTE_A")&&text.contains("NATIVE_NOTE_B")&&text.contains("epub:type=\"noteref\""));
        let (bytes,byte_warnings)=crate::mdexport::epub_build_bytes(&json!(EPUB_WARNING_SOURCE),base,&options,"Notes","Author","en-US","urn:uuid:note-warning-test","2026-10-20T00:00:00Z").unwrap();
        assert!(bytes.starts_with(b"PK\x03\x04"));assert_eq!(byte_warnings,warnings);
    }}
    let options=epub_warning_options("page","page");
    for unified in [false,true]{
        let output=directory.path().join(if unified{"unified.epub"}else{"direct.epub"});
        let result=if unified{
            crate::mdexport::export_document_with_data("epub",EPUB_WARNING_SOURCE,base,output.to_str().unwrap(),&options,"Notes",&assets,&data)
        }else{crate::mdexport::export_epub(EPUB_WARNING_SOURCE,base,output.to_str().unwrap(),&options,"Notes")}.unwrap();
        assert!(result.ok);assert!(std::fs::read(output).unwrap().starts_with(b"PK\x03\x04"));
        assert_epub_note_warnings(result.warns.as_ref().unwrap(),"page");
    }
}

#[test]
fn dedicated_mcp_epub_returns_actual_note_warnings_and_plain_exports_add_none(){
    let directory=tempfile::tempdir().unwrap();let data=directory.path().join("profile");let assets=directory.path().join("assets");
    let app=std::sync::Arc::new(crate::App::bootstrap(crate::paths::AppPaths::with_dirs(&data,directory.path(),&assets)).unwrap());
    for restart in ["section","page"]{
        let output=directory.path().join(format!("mcp-{restart}.epub"));
        let result=crate::mcp::call_tool(&app,"readmd_export_epub",&json!({"markdown_content":EPUB_WARNING_SOURCE,
            "output_path":output,"base_dir":directory.path(),"title":"Notes","language":"en-US","confirm":true,"options":epub_warning_options(restart,"page")}));
        assert_ne!(result["isError"],true,"{result}");
        let body=&result["structuredContent"];assert_eq!(body["ok"],true,"{result}");
        let warnings=body["warnings"].as_array().unwrap().iter().map(|value|value.as_str().unwrap().to_string()).collect::<Vec<_>>();
        assert_epub_note_warnings(&warnings,restart);
        let readable:Value=serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(readable["warnings"],body["warnings"]);assert!(body.get("warns").is_none());
        assert!(std::fs::read(output).unwrap().starts_with(b"PK\x03\x04"));
    }
    let output=directory.path().join("plain.epub");
    let result=crate::mcp::call_tool(&app,"readmd_export_epub",&json!({"markdown_content":"Plain body without notes.",
        "output_path":output,"base_dir":directory.path(),"title":"Plain","language":"en-US","confirm":true,
        "options":{"cover":{"enabled":false},"frontMatter":{"mode":"none"},"toc":{"enabled":false},"footer":{"pageNumbers":false}}}));
    assert_eq!(result["structuredContent"]["ok"],true,"{result}");assert_eq!(result["structuredContent"]["warnings"],json!([]));
    assert!(std::fs::read(output).unwrap().starts_with(b"PK\x03\x04"));
}
