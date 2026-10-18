//! Real OPC/WordprocessingML and entry-point refusal controls.
//! Draft only until root compiles/runs; native Word shaping remains unverified.
use super::*;
use crate::math_document::{parse_xml, XmlNode};
use serde_json::json;
use std::collections::BTreeMap;

fn package(source: &str, options: &Value) -> BTreeMap<String, Vec<u8>> {
    let result = crate::docx_writer::build(&crate::md_ast::parse(source), options, "Tracking synthetic", &|_| None);
    assert!(!result.parts.is_empty(), "{:?}", result.warns);
    let entries = result.parts.into_iter().map(|(path, data)| crate::mdexport::ZipEntry { path, data, compress: true }).collect::<Vec<_>>();
    let bytes = crate::mdexport::write_zip_at(&entries, 0, 0).unwrap();
    crate::publication_typography_docx_tests::unzip_bytes(&bytes)
}
fn xml(parts: &BTreeMap<String, Vec<u8>>, name: &str) -> XmlNode {
    parse_xml(std::str::from_utf8(&parts[name]).unwrap()).unwrap()
}
fn elements<'a>(node: &'a XmlNode, name: &str) -> Vec<&'a XmlNode> {
    let mut found = Vec::new();
    if node.name == name { found.push(node); }
    for child in node.children() { found.extend(elements(child, name)); }
    found
}
fn paragraph<'a>(root: &'a XmlNode, marker: &str) -> &'a XmlNode {
    let found = elements(root, "w:p").into_iter().filter(|p| p.text().contains(marker)).collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "{marker}"); found[0]
}
fn run_spacing(run: &XmlNode) -> Option<i32> {
    let values = run.children().filter(|n| n.name == "w:rPr")
        .flat_map(|n| n.children()).filter(|n| n.name == "w:spacing").collect::<Vec<_>>();
    assert!(values.len() <= 1, "Duplicate tracking override: {}", run.xml());
    values.first().map(|value| value.attributes["w:val"].parse::<i32>().unwrap())
}
fn declaration(id: &str, tracking: f64) -> String {
    format!("<!-- readmd:layout v1 {} -->\n\n", json!({"id":id,"text":{"trackingPt":tracking}}))
}

#[test]
fn p04_tracking_docx_global_targets_only_ordinary_runs_without_contaminating_styles_or_math() {
    let source = "# HEADING\n\nBODY **BOLD** *ITALIC* [LINK](https://example.invalid/qa) `CODE` $x^2$ NOTE[^n].\n\n> QUOTE\n\n- LIST\n\n| HEADER | NEXT |\n|---|---|\n| CELL | LAST |\n\n[^n]: FOOTNOTE\n";
    let old = json!({"frontMatter":{"mode":"none"},"typography":{"spacing":9}});
    let mut tracked = old.clone(); tracked["typography"]["trackingPt"] = json!(1.03);
    let baseline = package(source, &old); let actual = package(source, &tracked);
    for part in ["word/styles.xml", "word/settings.xml", "word/numbering.xml", "word/footnotes.xml"] {
        assert_eq!(actual[part], baseline[part], "Global tracking contaminated {part}");
    }
    let document = xml(&actual, "word/document.xml"); let body = paragraph(&document, "BODY");
    for run in elements(body, "w:r").into_iter().filter(|r| r.children().any(|n| n.name == "w:t")) {
        assert_eq!(run_spacing(run), if run.text() == "CODE" { None } else { Some(21) }, "{}", run.xml());
    }
    for marker in ["HEADING", "QUOTE", "LIST", "HEADER", "CELL"] {
        assert!(elements(paragraph(&document, marker), "w:r").into_iter().all(|run| run_spacing(run).is_none()), "{marker}");
    }
    let before_math = elements(&xml(&baseline,"word/document.xml"), "m:oMath").into_iter().map(XmlNode::xml).collect::<Vec<_>>();
    let actual_math = elements(&document, "m:oMath").into_iter().map(XmlNode::xml).collect::<Vec<_>>();
    assert!(!actual_math.is_empty()); assert_eq!(actual_math, before_math);
    assert_eq!(elements(&document,"w:t").into_iter().map(XmlNode::text).collect::<String>(),
               elements(&xml(&baseline,"word/document.xml"),"w:t").into_iter().map(XmlNode::text).collect::<String>());
}

#[test]
fn p04_tracking_docx_local_native_signed_twips_and_explicit_zero_preserve_unicode_math_and_rich_runs() {
    let source = [declaration("negative", -0.525), "NEGATIVE 中文 e\u{301} 👩\u{200d}💻 **BOLD** *ITALIC* [LINK](https://example.invalid) `CODE` $a+b$.\n\n".into(),
                  declaration("zero", 0.), "ZERO inherited extra spacing is cleared.\n\nPARENT restored after the scoped paragraphs.\n".into()].concat();
    let plain = source.replace("\"trackingPt\":-0.525", "\"trackingPt\":0.0");
    let options = json!({"frontMatter":{"mode":"none"},"typography":{"trackingPt":1.25}});
    let actual = package(&source, &options); let control = package(&plain, &options);
    let document = xml(&actual, "word/document.xml");
    for (marker, value) in [("NEGATIVE", -11),("ZERO", 0)] {
        let p = paragraph(&document, marker);
        assert!(elements(p,"w:r").into_iter().filter(|r| r.children().any(|n| n.name=="w:t"))
                .all(|run| run_spacing(run)==if run.text()=="CODE" {None} else {Some(value)}), "{}", p.xml());
    }
    assert!(elements(paragraph(&document,"PARENT"),"w:r").into_iter()
            .filter(|run|run.children().any(|n|n.name=="w:t")).all(|run|run_spacing(run)==Some(25)));
    let p = paragraph(&document,"NEGATIVE");
    assert!(p.text().contains("中文 e\u{301} 👩\u{200d}💻"));
    assert!(!elements(p,"w:b").is_empty() && !elements(p,"w:i").is_empty());
    assert!(!elements(p,"w:hyperlink").is_empty());
    assert_eq!(elements(&document,"m:oMath").into_iter().map(XmlNode::xml).collect::<Vec<_>>(),
               elements(&xml(&control,"word/document.xml"),"m:oMath").into_iter().map(XmlNode::xml).collect::<Vec<_>>());
    assert_eq!(actual["word/styles.xml"],control["word/styles.xml"]);
    assert!(!document.xml().contains("readmd:layout") && !document.xml().contains("trackingPt"));
}

#[test]
fn p04_tracking_docx_absent_and_null_preserve_all_stable_legacy_parts() {
    let source = "# OLD\n\nBODY $x^2$.\n\n> QUOTE\n\nNOTE[^n].\n\n[^n]: FOOTNOTE\n";
    let old = json!({"frontMatter":{"mode":"none"},"typography":{"lineHeight":1.75,"spacing":9}});
    let mut inherited = old.clone(); inherited["typography"]["trackingPt"] = Value::Null;
    let a = package(source,&old); let b = package(source,&inherited);
    for name in ["word/document.xml","word/styles.xml","word/settings.xml","word/numbering.xml","word/footnotes.xml"] {
        assert_eq!(a[name],b[name],"Missing/null tracking changed legacy {name}");
    }
}

#[test]
fn p04_tracking_bad_raw_values_refuse_before_any_image_or_resource_loader() {
    use std::cell::Cell;
    let calls=Cell::new(0); let load=|_:&str|{calls.set(calls.get()+1);None};
    let doc=crate::md_ast::parse("![synthetic](missing-owned.png)");
    for value in [json!("1"),json!(true),json!(-5.001),json!(20.001)] {
        let result=crate::docx_writer::build(&doc,&json!({"typography":{"trackingPt":value}}),"Synthetic",&load);
        assert!(result.parts.is_empty());assert_eq!(result.warns,vec![INVALID.to_string()]);
    }
    assert_eq!(calls.get(),0);
}

#[test]
fn p04_tracking_direct_unimplemented_formats_refuse_before_resources_staging_and_old_output_mutation() {
    let root=tempfile::tempdir().unwrap(); let data=root.path().join("profile-must-not-exist"); let assets=root.path().join("assets-must-not-exist");
    for (source, options) in [("Body with [@missing].".to_owned(),json!({"typography":{"trackingPt":0}})),
                              (declaration("local",1.)+"Body with [@missing].",json!({}))] {
        for (format,engine) in [("html","rust"),("epub","rust"),("tex","rust"),("pdf","rust"),("pdf","typst"),("pdf","xelatex")] {
            let target=root.path().join(format!("protected-{format}-{engine}.{format}"));std::fs::write(&target,b"KEEP OWNED ORIGINAL").unwrap();
            let mut options=options.clone();options["pdf"]=json!({"engine":engine});options["bibliography"]=json!("not-created-references.json");
            let error=crate::mdexport::export_document_with_data(format,&source,root.path().to_str().unwrap(),target.to_str().unwrap(),&options,"Synthetic",&assets,&data).unwrap_err();
            assert_eq!(error,UNSUPPORTED,"{format}/{engine}");assert_eq!(std::fs::read(target).unwrap(),b"KEEP OWNED ORIGINAL");
        }
    }
    assert!(!data.exists() && !assets.exists());
    assert!(!std::fs::read_dir(root.path()).unwrap().any(|item|item.unwrap().file_name().to_string_lossy().contains("readmd-part")));
}

#[test]
fn p04_tracking_raw_pdf_json_cannot_forge_a_consumer_or_omit_preflight() {
    use std::cell::Cell;
    let root=tempfile::tempdir().unwrap();let target=root.path().join("protected.pdf");std::fs::write(&target,b"KEEP PDF").unwrap();
    let calls=Cell::new(0);let resolve=|_:&str|{calls.set(calls.get()+1);None};
    for (blocks,style) in [
        (json!([{"type":"paragraph","text":"Body"}]),json!({"pdf":{"engine":"docx"},"typography":{"trackingPt":1}})),
        (json!([{"type":"paragraph","text":"Body","publicationContext":"body","publicationStyle":{"id":"local","text":{"trackingPt":0}}}]),json!({})),
    ] {
        let mut warnings=Vec::new();let result=crate::pdf_render::render(&blocks,target.to_str().unwrap(),&style,"",&resolve,&mut warnings);
        assert_eq!(result.unwrap_err().message,UNSUPPORTED);assert_eq!(std::fs::read(&target).unwrap(),b"KEEP PDF");
    }
    assert_eq!(calls.get(),0);
}

#[test]
fn p04_tracking_scoped_runs_keep_csl_native_format_and_restore_the_parent_after_nested_color() {
    use crate::md_ast::{Block, Inline};
    let mut document=crate::md_ast::parse("Parent.");
    let citation=Inline::Citation{source:crate::publication_rich::SourceCitation{
        span:crate::publication_rich::SourceSpan::new("[@qa]",0..5).unwrap(),items:Vec::new(),
        context:crate::academic::CiteContext::default()},children:vec![Inline::Strong(vec![Inline::Text("CITATION".into())])]};
    let decorations=||Inline::Strike(vec![Inline::Styled{format:crate::publication_rich::RichFormat{
        font_variant:crate::publication_rich::RichFontVariant::SmallCaps,text_decoration:crate::publication_rich::RichDecoration::Underline,
        vertical_align:crate::publication_rich::RichVerticalAlign::Sup,..Default::default()},children:vec![Inline::Text("DECORATED".into())]}]);
    document.blocks=vec![Block::Styled{attributes:json!({"text":{"trackingPt":-0.5}}),block:Box::new(
        Block::Styled{attributes:json!({"text":{"color":"#334455"}}),block:Box::new(Block::Paragraph(vec![
            Inline::Text("LOCAL ".into()),citation,Inline::Text(" ".into()),Inline::Code("CODE".into()),Inline::Math("x^2".into())
        ]))})},Block::Paragraph(vec![Inline::Text("PARENT".into())]),Block::Paragraph(vec![decorations()])];
    let package=crate::docx_writer::build(&document,&json!({"frontMatter":{"mode":"none"},"typography":{"trackingPt":1}}),"Synthetic",&|_|None);
    assert!(!package.parts.is_empty(),"{:?}",package.warns);
    let xml=package.parts.iter().find(|(name,_)|name=="word/document.xml").unwrap();
    let xml=parse_xml(std::str::from_utf8(&xml.1).unwrap()).unwrap();
    for run in elements(paragraph(&xml,"LOCAL"),"w:r").into_iter().filter(|run|run.children().any(|n|n.name=="w:t")) {
        assert_eq!(run_spacing(run),if matches!(run.text().as_str(),"CITATION"|"CODE"){None}else{Some(-10)});
    }
    assert!(elements(paragraph(&xml,"PARENT"),"w:r").into_iter().all(|run|run_spacing(run)==Some(20)));
    assert!(!elements(&xml,"m:oMath").is_empty());
    let decorated=elements(paragraph(&xml,"DECORATED"),"w:r")[0];assert_eq!(run_spacing(decorated),Some(20));
    let properties=decorated.children().find(|node|node.name=="w:rPr").unwrap();
    assert_eq!(properties.children().map(|node|node.local()).collect::<Vec<_>>(),vec!["smallCaps","strike","spacing","u","vertAlign"]);
}

#[test]
fn p04_tracking_valid_shared_preset_saves_for_docx_even_when_last_format_is_unsupported() {
    let root=tempfile::tempdir().unwrap();let assets=root.path().join("assets");std::fs::create_dir_all(&assets).unwrap();
    let app=std::sync::Arc::new(crate::App::bootstrap(crate::paths::AppPaths::with_dirs(&root.path().join("profile"),root.path(),&assets)).unwrap());
    let valid=json!({"custom":{"Tracking":{"typography":{"trackingPt":0.025}}},"last":{"fmt":"html","options":{"typography":{"trackingPt":0}}}});
    let (status,body)=crate::server::call_in_process(&app,"POST","/api/export/presets",Some(&valid));assert_eq!(status,200,"{body}");assert_eq!(body["ok"],true);
    let persisted=app.paths.data_dir.join("export_presets.json");let before=std::fs::read(&persisted).unwrap();
    let (_,body)=crate::server::call_in_process(&app,"GET","/api/export/presets",None);
    assert_eq!(body["custom"]["Tracking"]["typography"]["trackingPt"],0.025);assert_eq!(body["last"]["options"]["typography"]["trackingPt"],0);
    for invalid in [json!({"custom":{"Tracking":{"typography":{"trackingPt":"1"}}}}),
                    json!({"last":{"fmt":"docx","options":{"typography":{"trackingPt":20.001}}}})] {
        let (status,body)=crate::server::call_in_process(&app,"POST","/api/export/presets",Some(&invalid));
        assert_eq!(status,422,"{body}");assert_eq!(body["error_code"],INVALID);assert_eq!(body["field"],"typography.trackingPt");
        assert_eq!(std::fs::read(&persisted).unwrap(),before);
    }
}

#[test]
fn p04_tracking_prepared_paper_and_book_front_matter_never_inherit_body_tracking() {
    for mode in ["paper","book"] {
        let baseline=json!({"frontMatter":{"mode":mode},"meta":{"title":"FRONT TITLE","subtitle":"FRONT SUBTITLE",
            "authors":[{"id":"a","name":"FRONT AUTHOR","affiliationIds":["org"]}],
            "affiliations":[{"id":"org","name":"FRONT LAB"}],"abstracts":[{"language":"en","text":"FRONT ABSTRACT"}],
            "keywords":["FRONT KEYWORD"],"translators":[{"id":"t","name":"FRONT TRANSLATOR"}],
            "publisher":"FRONT PUBLISHER","series":{"title":"FRONT SERIES","number":"2"}}});
        let mut tracked=baseline.clone();tracked["typography"]=json!({"trackingPt":1});
        let before=package("BODY only.\n",&baseline);let actual=package("BODY only.\n",&tracked);
        let document=xml(&actual,"word/document.xml");let original=xml(&before,"word/document.xml");
        assert!(document.text().contains("FRONT TITLE")&&document.text().contains("FRONT AUTHOR"));
        if mode=="paper"{assert!(document.text().contains("FRONT ABSTRACT"));}
        else {assert!(document.text().contains("FRONT TRANSLATOR")&&document.text().contains("FRONT SERIES"));}
        let mut fronts=0;
        for p in elements(&document,"w:p") {
            let runs=elements(p,"w:r");
            if p.text().contains("BODY only.") {
                assert!(runs.into_iter().filter(|r|r.children().any(|n|n.name=="w:t")).all(|run|run_spacing(run)==Some(20)));
            } else if !p.text().trim().is_empty() {
                fronts+=1;assert!(runs.into_iter().all(|run|run_spacing(run).is_none()),"{mode}: {}",p.xml());
            }
        }
        assert!(fronts>=4);assert_eq!(document.text(),original.text());
        assert_eq!(actual["word/styles.xml"],before["word/styles.xml"]);
    }
}

#[test]
fn p04_tracking_direct_tex_document_refuses_without_any_line_spacing_request() {
    let ordinary=crate::md_ast::parse("BODY only.");
    let local=crate::md_ast::parse(&(declaration("tracked",0.)+"BODY only."));
    for (doc,options) in [(&ordinary,json!({"typography":{"trackingPt":1}})),(&local,json!({}))] {
        let options=crate::latex_writer::LatexOptions{publication_style:Some(options),..Default::default()};
        assert_eq!(crate::latex_writer::validate_paragraphs(doc,&options).unwrap_err(),UNSUPPORTED);
        let result=crate::latex_writer::render_document_with_geometry(doc,&options,&|_|None);
        assert_eq!(result.err().unwrap(),UNSUPPORTED);
        let legacy=crate::latex_writer::render_document(doc,&options,&|_|None);
        assert!(legacy.contains("\\PackageError{ReadMD}")&&legacy.contains(&crate::latex_writer::escape_text(UNSUPPORTED)));
    }
}
