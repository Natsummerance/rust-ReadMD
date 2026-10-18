use crate::{
    md_ast::{self, Block, Inline},
    publication_rich::*,
};
use serde_json::json;
fn projection(source: &str) -> Projection {
    let mut result = Projection::new(source);
    result.citations = source
        .match_indices("@a")
        .map(|(at, _)| {
            let span = SourceSpan::new(source, at..at + 2).unwrap();
            ProjectedCitation {
                source: SourceCitation {
                    span: span.clone(),
                    items: vec![SourceItem {
                        cite: crate::academic::Cite::new("a"),
                        span,
                        fields: Default::default(),
                    }],
                    context: Default::default(),
                },
                rich: vec![RichNode::Text {
                    text: "Author".into(),
                    format: RichFormat {
                        font_variant: RichFontVariant::SmallCaps,
                        text_decoration: RichDecoration::Underline,
                        vertical_align: RichVerticalAlign::Sup,
                        ..Default::default()
                    },
                }],
                inline_rich: Vec::new(),
                note_rich: Vec::new(),
                note_label: None,
                rendered: true,
            }
        })
        .collect();
    result
}
#[test]
fn typed_slots_preserve_paragraphs_headings_lists_tables_and_unicode_ranges() {
    for source in [
        "@a after",
        "before @a after",
        "# @a after",
        "- @a after",
        "| C |\n|---|\n| @a after |",
        "😀\r\n\r\n@a and @a after",
    ] {
        let p = projection(source);
        let doc = md_ast::parse_prepared(source, &p).unwrap();
        let mut items = Vec::new();
        md_ast::walk_inlines(&doc.blocks, &mut |node| {
            if let Inline::Citation { source, .. } = node {
                items.push(source.span.clone())
            }
        });
        assert_eq!(items.len(), p.citations.len(), "{source}");
        for item in items {
            assert_eq!(&source[item.byte_from..item.byte_to], "@a");
            assert_eq!(source[..item.byte_from].encode_utf16().count(), item.from);
        }
        let json = md_ast::to_render_json(&doc.blocks).to_string();
        assert!(json.contains("Author"));
        assert!(json.contains("after"), "{source}: {json}");
        assert!(!json.contains("readmd-csl"));
        if source.starts_with('#') {
            assert!(matches!(&doc.blocks[0],Block::Heading{id,..} if id=="author-after"));
        }
    }
}
#[test]
fn projection_refuses_stale_overlapping_or_forged_slots() {
    let source = "@a after";
    let p = projection(source);
    assert!(md_ast::parse_prepared("@a changed", &p).is_err());
    let mut overlap = p.clone();
    overlap.citations.push(p.citations[0].clone());
    assert!(md_ast::parse_prepared(source, &overlap).is_err());
    let forged = format!("{}\n\n@a", p.token(ProjectionSlot::Citation(0)));
    assert!(md_ast::parse_prepared(&forged, &projection(&forged)).is_err());
}
#[test]
fn rich_citation_and_bibliography_reach_ooxml_and_epub_semantics() {
    let source = "@a after";
    let mut p = projection(source);
    p.bibliography = Some(RichBibliography {
        title: "References".into(),
        items: vec![RichBibliographyItem {
            key: "a".into(),
            rich: p.citations[0].rich.clone(),
            first_field: vec![RichNode::text("[1]")],
        }],
        hanging_indent: true,
        line_spacing: 2,
        entry_spacing: 1,
        source_span: None,
    });
    let doc = md_ast::parse_prepared(source, &p).unwrap();
    let package = crate::docx_writer::build(&doc, &json!({}), "Paper", &|_| None);
    let body = String::from_utf8(
        package
            .parts
            .iter()
            .find(|(n, _)| n == "word/document.xml")
            .unwrap()
            .1
            .clone(),
    )
    .unwrap();
    for expected in [
        "<w:smallCaps/>",
        "<w:u w:val=\"single\"/>",
        "<w:vertAlign w:val=\"superscript\"/>",
        "<w:tab/>",
        "w:line=\"480\"",
        "References",
        "after",
    ] {
        assert!(body.contains(expected), "missing {expected}: {body}");
    }
    let epub = crate::epub_writer::render_chapters(&doc, "none", &mut |_| None)
        .into_iter()
        .map(|chapter| chapter.body)
        .collect::<String>();
    for expected in [
        "font-variant:small-caps",
        "text-decoration:underline",
        "vertical-align:super",
        "References",
        "role=\"presentation\" class=\"bibliography-columns\"",
        "<td class=\"bibliography-first-field\"",
        "<td class=\"bibliography-content\"",
        "after",
    ] {
        assert!(epub.contains(expected), "missing {expected}: {epub}");
    }
}
#[test]
fn standalone_citation_commands_keep_context_or_explicitly_reject_loss() {
    let mut p = projection("@a after");
    p.citations[0].source.items[0].cite.prefix = Some("see [this] & more".into());
    p.citations[0].source.items[0].cite.locator = Some("17".into());
    p.citations[0].source.items[0].cite.suffix = Some("and passim".into());
    let doc = md_ast::parse_prepared("@a after", &p).unwrap();
    crate::latex_writer::validate_native(&doc, "biblatex").unwrap();
    let options = crate::latex_writer::LatexOptions {
        bib_engine: "biblatex".into(),
        ..Default::default()
    };
    let tex = crate::latex_writer::render_document(&doc, &options, &|_| None);
    assert!(
        tex.contains("\\autocite[{see [this] \\& more}][{17 and passim}]{a}"),
        "{tex}"
    );
    p.citations[0].source.items[0].cite.prefix = None;
    let bibtex_doc = md_ast::parse_prepared("@a after", &p).unwrap();
    crate::latex_writer::validate_native(&bibtex_doc, "bibtex").unwrap();
    let bibtex_options = crate::latex_writer::LatexOptions {
        bib_engine: "bibtex".into(),
        ..Default::default()
    };
    let bibtex = crate::latex_writer::render_document(&bibtex_doc, &bibtex_options, &|_| None);
    assert!(bibtex.contains("\\cite[{17 and passim}]{a}"), "{bibtex}");
    assert!(!bibtex.contains("\\cite[{}]"), "BibTeX accepts one postnote, not two: {bibtex}");
    p.citations[0].source.items[0].cite.mode = crate::academic::CiteMode::SuppressAuthor;
    let doc = md_ast::parse_prepared("@a after", &p).unwrap();
    assert_eq!(
        crate::latex_writer::validate_native(&doc, "natbib").unwrap_err(),
        "academic_tex_context"
    );
    crate::latex_writer::validate_native(&doc, "").unwrap();
}

#[test]
fn unrepresentable_native_tex_bibliography_preserves_destination_and_creates_no_resources(){
    let directory=tempfile::tempdir().unwrap();let target=directory.path().join("Paper.tex");std::fs::write(&target,"Previous complete output").unwrap();
    let source="---\nbibliography: refs.json\n---\nCitation [@a].";
    let result=crate::mdexport::export_tex(source,directory.path().to_str().unwrap(),target.to_str().unwrap(),&json!({"tex":{"bibEngine":"biblatex"}}),"Paper");
    assert_eq!(result.unwrap_err(),"academic_tex_context");assert_eq!(std::fs::read_to_string(target).unwrap(),"Previous complete output");assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(),1);
}

#[test]
fn export_metadata_diagnostics_keep_source_ranges_and_do_not_blame_inherited_values() {
    let source="\u{feff}---\r\ntitle: \"中文🧭\"\r\nauthors:\r\n  - name: \"Linh, Nguyen\"\r\n    orcid: \"0000-0000-0000-0000\"\r\n---\r\nBody unchanged\r\n";
    let error=crate::publication_metadata::from_source(source,&json!({}),"Paper").unwrap_err();
    assert_eq!(error.code,"publication_metadata_orcid");
    let body=crate::publication_metadata_formats::error_body(source,&json!({}),"Paper","docx",&error.code).unwrap();
    assert_eq!(body["path"],"authors[0].orcid");
    let utf16:Vec<u16>=source.encode_utf16().collect();
    let range=String::from_utf16(&utf16[body["from"].as_u64().unwrap() as usize..body["to"].as_u64().unwrap() as usize]).unwrap();
    assert!(range.contains("0000-0000-0000-0000"),"{range}");
    let valid=source.replace("0000-0000-0000-0000","0000-0002-1825-0097");
    let options=json!({"meta":{"authors":[{"name":"Export only","orcid":"0000-0000-0000-0000"}]}});
    let error=crate::publication_metadata::from_source(&valid,&options,"Paper").unwrap_err();
    assert_eq!((error.from,error.to),(0,0),"An invalid override must not point at a valid inherited author");
    let body=crate::publication_metadata_formats::error_body(&valid,&options,"Paper","epub",&error.code).unwrap();
    assert_eq!((body["from"].as_u64(),body["to"].as_u64()),(Some(0),Some(0)));
    assert!(crate::publication_metadata_formats::error_body(&valid,&json!({}),"Paper","pdf","publication_metadata_pdf").is_none(),"An engine failure must not be labelled as a source validation error");
}

#[test]
fn sparse_presets_keep_inherited_relationships_and_validate_explicit_entities() {
    let sparse=json!({"meta":{"authors":[{"id":"ada","name":"Ada revised","affiliationIds":["north"]}],"funding":[{"funder":"Example","recipientIds":["ada"]}],"clear":["subtitle"]}});
    crate::publication_metadata::validate_options(&sparse).unwrap();
    let source="---\nauthors: [{id: ada, name: Ada, affiliationIds: [north]}]\naffiliations: [{id: north, name: Institute}]\n---\nBody\n";
    let applied=crate::publication_metadata::from_source(source,&sparse,"Study").unwrap();
    assert_eq!(applied.authors[0].name,"Ada revised");
    assert_eq!(applied.affiliations[0].id,"north");
    assert!(crate::publication_metadata::from_source("Body",&sparse,"Study").is_err(),"Saving a sparse preset does not authorize dangling references at export");
    let mut contradictory=sparse.clone();contradictory["meta"]["affiliations"]=json!([]);
    assert_eq!(crate::publication_metadata::validate_options(&contradictory).unwrap_err().code,"publication_metadata_reference");
    let mut duplicate=sparse;duplicate["meta"]["authors"][0]["affiliationIds"]=json!(["north","north"]);
    assert_eq!(crate::publication_metadata::validate_options(&duplicate).unwrap_err().code,"publication_metadata_reference");
    crate::publication_metadata::validate_options(&json!({"meta":{"funding":[{"funder":"Example","recipientIds":["inherited-person"]}]}})).unwrap();
}

#[test]
fn rich_small_caps_use_portable_real_faces_without_changing_unicode_text() {
    let doc=md_ast::parse_prepared("@a after",&projection("@a after")).unwrap();
    let options=crate::latex_writer::LatexOptions{bib_engine:String::new(),..Default::default()};
    let tex=crate::latex_writer::render_document(&doc,&options,&|_|None);
    for face in ["regular","bold","italic","bolditalic"] {assert!(tex.contains(&format!("texgyretermes-{face}.otf")));}
    assert!(tex.contains("\\readmdsmallcaps{"));assert!(tex.contains("Author"));assert!(!tex.contains("AUTHOR"));
    let dir=tempfile::tempdir().unwrap();
    let typst=crate::typst_writer::render(&doc,&json!({}),"",dir.path(),"Paper").unwrap();
    assert!(typst.text.contains("\"Libertinus Serif\""));assert!(!typst.text.contains("Libertinus Sans"));assert!(typst.text.contains("#smallcaps["));
    assert!(typst.text.contains("\"Author\""));assert!(!typst.text.contains("\"AUTHOR\""));
    assert_eq!(typst.warnings.iter().filter(|warning|warning.as_str()=="font_fallback: Libertinus Serif").count(),1,"The absent Sans family must use the pinned real smcp face with an explicit, deduplicated diagnostic");
    let plain=crate::latex_writer::render_document(&md_ast::parse("Unchanged body"),&Default::default(),&|_|None);
    assert!(!plain.contains("readmdsmallcaps"),"Plain documents must not acquire optional font code");
}

#[test]
fn typed_rich_font_axes_preserve_source_and_use_pinned_typst_faces() {
    let source="中文 😀 @a after";
    let dir=tempfile::tempdir().unwrap();
    for (style,weight,variant) in [(RichFontStyle::Normal,RichFontWeight::Normal,RichFontVariant::Normal),(RichFontStyle::Italic,RichFontWeight::Normal,RichFontVariant::Normal),(RichFontStyle::Oblique,RichFontWeight::Normal,RichFontVariant::Normal),(RichFontStyle::Normal,RichFontWeight::Bold,RichFontVariant::Normal),(RichFontStyle::Italic,RichFontWeight::Bold,RichFontVariant::Normal),(RichFontStyle::Normal,RichFontWeight::Normal,RichFontVariant::SmallCaps),(RichFontStyle::Italic,RichFontWeight::Bold,RichFontVariant::SmallCaps)]{
        let mut p=projection(source);p.citations[0].rich=vec![RichNode::Text{text:"Abc 中文 😀".into(),format:RichFormat{font_style:style,font_weight:weight,font_variant:variant,..Default::default()}}];
        p.bibliography=Some(RichBibliography{title:"References".into(),items:vec![RichBibliographyItem{key:"a".into(),first_field:Vec::new(),rich:p.citations[0].rich.clone()}],hanging_indent:false,line_spacing:1,entry_spacing:0,source_span:None});
        let doc=md_ast::parse_prepared(source,&p).unwrap();let before=md_ast::to_render_json(&doc.blocks);
        let output=crate::typst_writer::render(&doc,&json!({"typography":{"font":"Libertinus Sans"},"cover":{"enabled":false}}),"",dir.path(),"Paper").unwrap();
        assert_eq!(md_ast::to_render_json(&doc.blocks),before,"Rendering must preserve typed source identities and styles");
        assert!(!output.text.contains("Libertinus Sans"));assert!(output.text.contains("font:(\"Libertinus Serif\",\"DejaVu Sans\")"));
        assert_eq!(output.text.matches("#text(\"Abc 中文 😀\")").count(),2,"Citation and bibliography both retain original Unicode");
        assert_eq!(output.text.matches("#smallcaps[").count(),if variant==RichFontVariant::SmallCaps{2}else{0});
        assert_eq!(output.warnings,vec!["font_fallback: Libertinus Serif"]);
        let span=&p.citations[0].source.span;assert_eq!(&source[span.byte_from..span.byte_to],"@a");
        assert_eq!(span.from,source[..span.byte_from].encode_utf16().count());
    }
}
