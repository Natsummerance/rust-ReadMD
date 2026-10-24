use super::*;

const SOURCE:&str="<!-- readmd-section:v1 {\"id\":\"first\"} -->\n\n# One\n\nOne[^a], again[^a], other[^b].\n\n# Heading alone does not restart\n\nThird[^c].\n\n<!-- readmd-section:v1 {\"id\":\"second\",\"break\":\"continuous\"} -->\n\nSecond source section[^d], old[^a].\n\n[^a]: Alpha.\n\n[^b]: Beta.\n\n[^c]: Gamma.\n\n[^d]: Delta.\n";
fn output(options:Value)->Source {
    let directory=tempfile::tempdir().unwrap();
    render(&crate::md_ast::parse(SOURCE),&options,"",directory.path(),"Notes").unwrap()
}

#[test]
fn page_restart_projects_a_physical_page_header_and_section_restart_only_updates_at_source_boundaries() {
    let page=output(serde_json::json!({"footnotes":{"restart":"page","start":27}})).text;
    assert!(page.contains("header:context { counter(footnote).update(26) },footer:none"));
    assert_eq!(page.matches("#counter(footnote).update(26)").count(),1);
    let section=output(serde_json::json!({"footnotes":{"restart":"section","start":27}})).text;
    assert!(!section.contains("header:context { counter(footnote)"));
    assert_eq!(section.matches("#counter(footnote).update(26)").count(),3); // Initial default, then two source sections.
    for source in [&page,&section]{
        assert_eq!(source.matches("#footnote[").count(),4);
        assert_eq!(source.matches("#footnote(label(").count(),2);
        assert_eq!(source.matches("#text(\"Alpha.\")").count(),1);
    }
    // These are source projection checks. Real page numbering and geometry
    // are separately verified through the pinned worker and PDF readback.
}

#[test]
fn native_note_geometry_is_checked_after_layout_without_changing_paragraphs_or_repeated_identities() {
    let rendered=output(serde_json::json!({"footnotes":{"restart":"page","start":27}}));
    let context=rendered.geometry.as_ref().unwrap();
    assert_eq!(context.note_count,4);assert!(context.paragraphs.is_empty());
    assert_eq!(rendered.text.matches("query(footnote.entry).enumerate()").count(),1);
    assert!(rendered.text.rfind("query(footnote.entry)").unwrap()>rendered.text.rfind("#footnote[").unwrap());
    assert!(rendered.text.contains("actual=entry.location().page()"));
    assert!(rendered.text.contains("expected=entry.note.location().page()"));
    assert!(rendered.text.contains("if actual != expected"));
    assert!(rendered.text.contains(&string(&context.marker)));
    assert!(!rendered.text.contains("#let readmd-typed-paragraph"));
    assert!(rendered.text.contains("costs:(widow:100%,orphan:100%)"));
    assert!(rendered.text.contains("#par[#text(\"One\")"));
    assert_eq!(rendered.text.matches("#footnote[").count(),4);
    assert_eq!(rendered.text.matches("#footnote(label(").count(),2);
    let end=output(serde_json::json!({"footnotes":{"position":"document"}}));
    assert!(end.geometry.is_none());assert!(!end.text.contains("query(footnote.entry)"));
    let dir=tempfile::tempdir().unwrap();
    let plain=render(&crate::md_ast::parse("Legacy paragraph without notes."),&serde_json::json!({}),"",dir.path(),"Plain").unwrap();
    assert!(plain.geometry.is_none());assert!(!plain.text.contains("query(footnote.entry)"));
}

fn note_details()->Value {serde_json::json!({"coordinateSpace":"pdfFootnoteStartPage","noteIndex":1,"definitionPage":2,"referencePage":3})}
fn note_diagnostic(context:&GeometryContext,source:&std::path::Path,value:Value)->String {
    format!("{}:13:1: error: panicked with: {}{}",source.display(),context.marker,value)
}

#[test]
fn note_geometry_promotes_only_this_render_native_note_index_and_exact_worker_diagnostic() {
    let dir=tempfile::tempdir().unwrap();let source=dir.path().join("document.typ");
    let mut context=GeometryContext::new();context.note_guard(2);
    for (definition,reference) in [(2,3),(4,3)] {
        let mut details=note_details();details["definitionPage"]=serde_json::json!(definition);details["referencePage"]=serde_json::json!(reference);
        assert_eq!(context.diagnostic(&note_diagnostic(&context,&source,details.clone()),&source,dir.path()),Some(format!("publication_note_layout: {details}")));
    }
    for (key,value) in [("coordinateSpace",serde_json::json!("pdfParagraphLine")),("noteIndex",serde_json::json!(0)),
        ("noteIndex",serde_json::json!(3)),("noteIndex",serde_json::json!(1.5)),("noteIndex",serde_json::json!("1")),
        ("definitionPage",serde_json::json!(0)),("referencePage",serde_json::json!(100_001)),
        ("definitionPage",serde_json::json!(3)),("definitionPage",serde_json::json!(-1)),
        ("referencePage",serde_json::json!(3.5)),("extra",serde_json::json!(true))] {
        let mut details=note_details();details[key]=value;
        assert!(context.diagnostic(&note_diagnostic(&context,&source,details),&source,dir.path()).is_none(),"{key}");
    }
    let valid=note_diagnostic(&context,&source,note_details());
    for invalid in [valid.replace(&context.marker,"readmd-p04-other:"),format!("source excerpt: {valid}"),
        valid.replace("document.typ:13:1:","other.typ:13:1:"),valid.replace("13:1:","0:1:"),
        valid.replace("13:1:","13:0:"),valid.clone()+" trailing text",
        valid.replace("\"noteIndex\":1","\"noteIndex\":1,\"noteIndex\":1"),
        format!("{valid}\nerror: compilation failed"),format!("{valid}\n{valid}")] {
        assert!(context.diagnostic(&invalid,&source,dir.path()).is_none(),"{invalid}");
    }
    assert!(context.diagnostic(&valid,&source,&dir.path().join("other-worker")).is_none());
    context.note_count=0;assert!(context.diagnostic(&valid,&source,dir.path()).is_none());
    context.note_count=crate::publication_blocks::MAX_NODES+1;assert!(context.diagnostic(&valid,&source,dir.path()).is_none());
}

#[test]
fn native_note_stderr_packet_retains_the_actual_first_entry_page_failure() {
    let dir=tempfile::tempdir().unwrap();let source=dir.path().join("document.typ");
    let mut context=GeometryContext::new();context.marker="readmd-p04-8d1d3a8c1fc14747b323c5038f53ff57:".into();context.note_guard(2);
    let native=include_str!("../tests/fixtures/typst-note-native-absolute.stderr");
    // Keep the native packet verbatim; remap only its owned absolute source
    // path so this diagnostic fixture is portable across test workspaces.
    let native_path=native.split_once(":12:1: error:").unwrap().0;
    let stderr=native.replace(native_path,source.to_str().unwrap());
    let error=context.diagnostic(&stderr,&source,dir.path()).unwrap();
    let (code,details)=error.split_once(": ").unwrap();assert_eq!(code,"publication_note_layout");
    let details:Value=serde_json::from_str(details).unwrap();assert_eq!(details["noteIndex"],1);
    assert_eq!(details["definitionPage"],1);assert_eq!(details["referencePage"],2);
    let relative=stderr.replace(source.to_str().unwrap(),"document.typ");
    assert_eq!(context.diagnostic(&relative,&source,dir.path()),Some(error));
    assert!(context.diagnostic(&relative.replace("document.typ:","./document.typ:"),&source,dir.path()).is_none());
}

#[test]
fn source_section_endnotes_keep_their_first_reference_labels_and_all_return_links() {
    let rendered=output(serde_json::json!({"footnotes":{"position":"document","restart":"section","start":27,"numberFormat":"upperLetter"}}));
    let source=rendered.text;
    assert_eq!(source.matches(&format!("#numbering({},27)",number_format("upperLetter"))).count(),6); // A references/entry plus D reference/entry.
    assert_eq!(source.matches(&format!("#numbering({},28)",number_format("upperLetter"))).count(),2);
    assert_eq!(source.matches(&format!("#numbering({},29)",number_format("upperLetter"))).count(),2);
    assert!(!source.contains(&format!("#numbering({},30)",number_format("upperLetter"))));
    for (kind,value) in [("note-ref","a:1"),("note-ref","a:2"),("note-ref","a:3"),("note","a"),("note","d")]{
        assert!(source.contains(&format!("#label({})",string(&private_anchor(kind,value)))));
    }
    assert!(!source.contains("#footnote["));
}

#[test]
fn author_anchors_cannot_shadow_notes_returns_or_source_sections() {
    let source="<!-- readmd-section:v1 {\"id\":\"first\"} -->\n\n# Author note {#note:a}\n\nOne[^a], repeated[^a], [heading](#note:a).\n\n## Author return {#note-ref:a:1}\n\n[return heading](#note-ref:a:1).\n\n## Author section {#readmd-section-first}\n\n[section heading](#readmd-section-first).\n\n[^a]: Unique note.\n";
    for position in ["page","document"] {
        let dir=tempfile::tempdir().unwrap();
        let result=render(&crate::md_ast::parse(source),&serde_json::json!({"footnotes":{"position":position}}),"",dir.path(),"Anchors").unwrap().text;
        for id in ["note:a","note-ref:a:1","readmd-section-first"] {
            let label=string(&anchor(id));
            assert_eq!(result.matches(&format!("#label({label})")).count(),1);
            let target=if id=="readmd-section-first" {string(&private_anchor("section","first"))}else{label};
            assert!(result.contains(&format!("#link(label({target}))")));
        }
        assert_eq!(result.matches(&format!("#label({})",string(&private_anchor("note","a")))).count(),1);
        assert!(result.contains(&format!("#label({})",string(&private_anchor("section","first")))));
        assert_eq!(result.matches("#text(\"Unique note.\")").count(),1);
        for (kind,value) in [("note","a"),("note-ref","a:1"),("section","first")] {
            for author in ["note:a","note-ref:a:1","readmd-section-first", "readmd-internal-note-61", "α"] {
                assert_ne!(anchor(author),private_anchor(kind,value));
            }
        }
    }
}

#[test]
fn undeclared_section_fragments_still_target_author_headings() {
    let dir=tempfile::tempdir().unwrap();
    let source=render(&crate::md_ast::parse("# Regular {#readmd-section-first}\n\n[Ordinary](#readmd-section-first)."),&serde_json::json!({}),"",dir.path(),"Heading").unwrap().text;
    let label=string(&anchor("readmd-section-first"));
    assert!(source.contains(&format!("#label({label})")));
    assert!(source.contains(&format!("#link(label({label}))")));
}

#[test]
fn typst_reports_unavailable_continuation_stories_and_endnote_page_restart_without_pretending_to_apply_them() {
    let rendered=output(serde_json::json!({"footnotes":{"position":"document","restart":"page","continuationNotice":"Continued #read(\"secret\")"}}));
    assert!(rendered.warnings.contains(&"publication_note_page_restart_unsupported".into()));
    assert!(rendered.warnings.contains(&"publication_note_continuation_unsupported".into()));
    assert!(!rendered.text.contains("header:context { counter(footnote)"));
    assert!(!rendered.text.contains("Continued #read"));
    let rendered=output(serde_json::json!({"footnotes":{"separator":"none","continuationSeparator":"line","longNote":"keepParagraphs"}}));
    assert!(rendered.warnings.contains(&"publication_note_continuation_unsupported".into()));
    assert!(rendered.text.contains("#set footnote.entry(separator:none,gap:0.6em)"));
    assert!(rendered.text.contains("#block(breakable:false)[#par[#text(\"Alpha.\")"));
}
