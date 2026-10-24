use super::*;

const SOURCE:&str="<!-- readmd-section:v1 {\"id\":\"first\"} -->\n\n# One\n\nOne[^a], repeated[^a], two[^b].\n\n# Another chapter in the same section\n\nThree[^c].\n\n<!-- readmd-section:v1 {\"id\":\"second\",\"break\":\"continuous\"} -->\n\n# New source section\n\nFour[^d] and old[^a].\n\n[^a]: Alpha.\n\n[^b]: Beta.\n\n[^c]: Gamma.\n\n[^d]: Delta.\n";
fn chapters(restart:&str,position:&str,split:&str)->Vec<Chapter>{
    let style=crate::export_styles::sanitize(&serde_json::json!({"footnotes":{"position":position,"restart":restart,"start":27,"numberFormat":"upperLetter"},"cover":{"enabled":false},"toc":{"enabled":false}}));
    render_chapters_with_options(&crate::md_ast::parse(SOURCE),split,&mut |_|None,&style)
}

#[test]
fn chapter_splitting_never_changes_continuous_or_source_section_note_labels() {
    for position in ["page","document"] {for split in ["none","h1","h2"]{
        let section=chapters("section",position,split);let all=section.iter().map(|chapter|chapter.body.as_str()).collect::<String>();
        assert_eq!(all.matches("<sup>AA</sup>").count(),4); // A three times, D once.
        assert_eq!(all.matches("<sup>BB</sup>").count(),1);
        assert_eq!(all.matches("<sup>CC</sup>").count(),1);
        assert!(!all.contains("<sup>DD</sup>"));
        let continuous=chapters("continuous",position,split).into_iter().map(|chapter|chapter.body).collect::<String>();
        assert_eq!(continuous.matches("<sup>AA</sup>").count(),3);
        assert_eq!(continuous.matches("<sup>DD</sup>").count(),1);
        // Page restart cannot be inferred from chapter splitting in an EPUB.
        let page=chapters("page",position,split).into_iter().map(|chapter|chapter.body).collect::<String>();
        assert_eq!(page,continuous);
    }}
}

#[test]
fn restart_changes_labels_without_changing_epub_note_targets_or_backlink_occurrences() {
    fn links(chapters:&[Chapter])->Vec<String>{
        chapters.iter().flat_map(|chapter|chapter.body.split('<')).filter(|part|part.starts_with("a epub:type=")||part.starts_with("aside epub:type=")||part.starts_with("li epub:type="))
            .map(|part|part.split('>').next().unwrap().into()).collect()
    }
    for position in ["page","document"]{
        let baseline=chapters("continuous",position,"h1");let section=chapters("section",position,"h1");
        assert_eq!(links(&baseline),links(&section));
        let doc=crate::md_ast::parse(&SOURCE.replace('\n',"\r\n"));
        let style=serde_json::json!({"footnotes":{"position":position,"restart":"section","start":27,"numberFormat":"upperLetter"}});
        let reopened=render_chapters_with_options(&doc,"h1",&mut |_|None,&style);
        assert_eq!(section.iter().map(|chapter|&chapter.body).collect::<Vec<_>>(),reopened.iter().map(|chapter|&chapter.body).collect::<Vec<_>>());
        if position=="document"{
            let notes=&section.last().unwrap().body;
            assert_eq!(notes.matches("Alpha.").count(),1);
            assert!(notes.contains("chapter_1.xhtml#fnref-1")&&notes.contains("chapter_3.xhtml#fnref-1-3"));
        }
    }
}

#[test]
fn separators_and_long_note_paragraph_flow_are_explicit_and_locally_overridable() {
    let doc=crate::md_ast::parse("Text[^a].\n\n[^a]: First.\n\n    Explicit split.<!-- readmd:layout v1 {\"id\":\"note-flow-explicit\",\"paragraph\":{\"keepLines\":false}} -->\n\n    Third.\n");
    for (policy,want) in [("split","auto"),("keepParagraphs","avoid")]{
        let options=serde_json::json!({"footnotes":{"separator":"none","longNote":policy}});
        let css=profile_css(&options);
        assert!(css.contains(&format!(".footnotes p{{break-inside:{want};page-break-inside:{want}}}")));
        let output=render_chapters_with_options(&doc,"none",&mut |_|None,&options);
        assert!(!output[0].body.contains("<hr/>"));
        assert!(output[0].body.contains("Explicit split.")&&output[0].body.contains("break-inside:auto"));
    }
    assert!(!profile_css(&serde_json::json!({})).contains(".footnotes p{break-inside:"));
}

#[test]
fn letter_note_boundaries_use_word_labels_without_changing_occurrence_identity() {
    let doc=crate::md_ast::parse("Body[^a], repeated[^a], then[^b].\n\n[^a]: A.\n\n[^b]: B.\n");
    for (start,first,next) in [(27,"AA","BB"),(52,"ZZ","AAA")] {for position in ["page","document"] {
        let style=serde_json::json!({"footnotes":{"position":position,"start":start,"numberFormat":"upperLetter"}});
        let all=render_chapters_with_options(&doc,"none",&mut |_|None,&style).into_iter().map(|chapter|chapter.body).collect::<String>();
        assert_eq!(all.matches(&format!("<sup>{first}</sup>")).count(),2);
        assert_eq!(all.matches(&format!("<sup>{next}</sup>")).count(),1);
        assert!(all.contains("id=\"fn-1\"")&&all.contains("id=\"fn-2\"")&&all.contains("#fnref-1-2\""));
    }}
}
