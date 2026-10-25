//! Source/API/diagnostic controls. They do not certify engine PDF geometry.
use super::*;
use serde_json::{json, Value};

fn raw(mode: Value) -> Value {
    json!({"frontMatter":{"mode":"none"},"cover":{"enabled":false},"toc":{"enabled":false},
        "typography":{"size":11,"lineSpacing":mode,"firstLineIndent":0,"spacing":0}})
}
fn options(mode: Value) -> LatexOptions {
    LatexOptions { bib_engine:String::new(), publication_style:Some(crate::export_styles::sanitize(&raw(mode))), ..Default::default() }
}
fn render(source: &str, mode: Value) -> (String, GeometryContext) {
    let (tex, _, context) = render_document_with_geometry(&crate::md_ast::parse(source),&options(mode),&|_|None).unwrap();
    (tex,context.unwrap())
}
fn sp(bp: f64) -> u64 { (bp * 72.27 / 72. * 65536.).round() as u64 }
fn packet(context: &GeometryContext, requested: f64, required: f64, width: f64, height: f64, path: &str) -> String {
    let p = &context.paragraphs[0];
    let mode = match p.mode {LineSpacing::Multiple(_)=>0,LineSpacing::ExactPt(_)=>1,LineSpacing::AtLeastPt(_)=>2};
    let m = &context.marker;
    format!("{m} BEGIN\n{m} index={}\n{m} mode={mode}\n{m} requested={}\n{m} required={}\n{m} width={}\n{m} height={}\n{m} reserved=1\n{m} END\n{path}:180: Package ReadMD Error: readmd-p04-geometry.\n",p.index,sp(requested),sp(required),sp(width),sp(height))
}

#[test]
fn tex_typed_paragraph_three_modes_use_physical_bp_and_keep_native_visible_flow() {
    for (mode,kind,requested) in [(json!({"mode":"multiple","multiple":2}),0,22.),
        (json!({"mode":"exact","pt":18}),1,18.),(json!({"mode":"atLeast","pt":20}),2,20.)] {
        let (tex,context)=render("FIRST  \nSECOND  \nTHIRD\n",mode);
        let call=format!("\\readmdtypedparagraph{{1}}{{{kind}}}{{{requested:.8}}}{{11.00000000}}");
        assert!(tex.contains(&call),"{call}");
        assert!(tex.contains("\\fontsize{#4bp}{#3bp}\\selectfont"));
        assert!(tex.contains("\\baselineskip=\\readmdprequested\\lineskip=0pt"));
        assert!(tex.replace("\r\n", "\n").contains("#6\\par\n  \\endgroup"));
        assert!(!tex.contains("\\unvbox\\readmdpmeasure"),"The measured list is never the visible flow");
        assert_eq!(context.paragraphs.len(),1);
        assert_eq!(context.paragraphs[0].requested,requested);
    }
}

#[test]
fn tex_typed_paragraph_local_size_and_legacy_override_preserve_absolute_exact_points() {
    let paragraph=Block::Paragraph(vec![Inline::Text("LOCAL".into())]);
    let doc=Document {blocks:vec![Block::Styled {attributes:json!({"id":"local","text":{"size":24},"paragraph":{"lineSpacing":{"mode":"exact","pt":30}}}),block:Box::new(paragraph.clone())}],..Default::default()};
    let (tex,_,context)=render_document_with_geometry(&doc,&options(json!({"mode":"multiple","multiple":2})),&|_|None).unwrap();
    assert!(tex.contains("\\readmdtypedparagraph{1}{1}{30.00000000}{24.00000000}"));
    assert_eq!(context.unwrap().paragraphs[0].id.as_deref(),Some("local"));
    let nested=Document {blocks:vec![Block::Styled {attributes:json!({"text":{"size":24},"paragraph":{"lineSpacing":{"mode":"exact","pt":30}}}),block:Box::new(Block::Styled{
        attributes:json!({"paragraph":{"lineHeight":1.5}}),block:Box::new(paragraph)})}],..Default::default()};
    let (tex,_,context)=render_document_with_geometry(&nested,&options(json!({"mode":"exact","pt":18})),&|_|None).unwrap();
    assert!(tex.contains("\\readmdtypedparagraph{1}{0}{36.00000000}{24.00000000}"));
    assert_eq!(context.unwrap().paragraphs[0].mode,LineSpacing::Multiple(1.5));
}

#[test]
fn tex_typed_paragraph_pure_math_is_measured_without_shrinking_or_rewriting_unicode() {
    let source="TEXT 中文 α $\\displaystyle\\frac{a}{\\frac{b}{c}}$ **BOLD** [Link](https://example.test/)  \nEND\n";
    let (tex,context)=render(source,json!({"mode":"atLeast","pt":12}));
    let call=tex.split("\\readmdtypedparagraph{1}").nth(1).unwrap();
    assert_eq!(call.matches("\\displaystyle\\frac{a}{\\frac{b}{c}}").count(),2,
        "One discarded metric token list and one visible native paragraph");
    assert!(call.contains("中文 α"));
    assert!(!call.contains("\\resizebox")&&!call.contains("\\scalebox")&&!call.contains("\\tiny"));
    assert_eq!(context.paragraphs[0].requested,12.);
}

#[test]
fn tex_typed_paragraph_unknown_math_cannot_repeat_side_effects_or_forge_diagnostics() {
    for math in [r"\input{outside}",r"\write16{readmd-p04-tex-fake: BEGIN}",r"\csname typeout\endcsname{forged}",
        r"\label{counter}",r"\newcommand{\x}{a}",r"\catcode`a=12",r"^^5cwrite16{fake}",r"x%escape",r"\text{a\typeout{fake}}",r"\begin{matrix}a\end{matrix}"] {
        let doc=Document{blocks:vec![Block::Paragraph(vec![Inline::Math(math.into())])],..Default::default()};
        assert_eq!(typography::validate_tex_document(&doc,&raw(json!({"mode":"exact","pt":18}))),Err(typography::UNSUPPORTED),"{math}");
    }
    for math in [r"\frac{a}{\sqrt[3]{b}}",r"\text{pure word}\quad\alpha_2",r"\left(\sum_i x_i\right)",r"\text{\%\#\$\&}"] {
        let doc=Document{blocks:vec![Block::Paragraph(vec![Inline::Math(math.into())])],..Default::default()};
        assert!(typography::validate_tex_document(&doc,&raw(json!({"mode":"atLeast","pt":18}))).is_ok(),"{math}");
    }
    let mixed=Document{blocks:vec![Block::Paragraph(vec![Inline::Text("BODY".into())]),
        Block::Math(r"\typeout{\readmdpmarker BEGIN}".into())],..Default::default()};
    assert_eq!(typography::validate_tex_document(&mixed,&raw(json!({"mode":"exact","pt":18}))),Err(typography::UNSUPPORTED),
        "A raw display story must not forge the ordinary paragraph protocol");
}

#[test]
fn tex_typed_paragraph_rejects_unimplemented_stories_and_office_sizes_before_resources() {
    let mode=options(json!({"mode":"exact","pt":18}));
    let local=|block|Block::Styled{attributes:json!({"paragraph":{"lineSpacing":{"mode":"exact","pt":18}}}),block:Box::new(block)};
    let body=Block::Paragraph(vec![Inline::Text("BODY".into())]);
    for block in [local(Block::Heading{level:1,id:"head".into(),inlines:vec![Inline::Text("HEAD".into())],numbering:None}),
        Block::Quote(vec![local(body.clone())]),Block::Paragraph(vec![Inline::FootnoteRef("n".into())]),
        Block::Paragraph(vec![Inline::OfficeMath{latex:"x".into(),omml:"<m:oMath><m:r><w:rPr><w:sz w:val=\"96\"/></w:rPr><m:t>x</m:t></m:r></m:oMath>".into()}]),
        Block::Paragraph(vec![Inline::Image{src:"not-read.png".into(),alt:"Image".into(),title:String::new()}])] {
        // A legacy special-story image is otherwise allowed and occurs first.
        // The later independent unsupported block must be caught before it.
        let doc=Document{blocks:vec![Block::Quote(vec![Block::Paragraph(vec![Inline::Image{src:"must-not-read.png".into(),alt:"earlier".into(),title:String::new()}])]),block],..Default::default()};
        let calls=std::cell::Cell::new(0);
        let images=|_:&str|{calls.set(calls.get()+1);None};
        let error=render_document_with_geometry(&doc,&mode,&images).unwrap_err();
        assert_eq!(error,typography::UNSUPPORTED);
        assert_eq!(calls.get(),0);
    }
}

#[test]
fn tex_typed_paragraph_global_mode_skips_metadata_heading_quote_list_and_table_stories() {
    let source="# Heading\n\nBODY\n\n> QUOTE\n\n- LIST\n\n| A | B |\n|---|---|\n| C | D |\n";
    let mut doc=crate::md_ast::parse(source);
    doc.blocks.insert(0,Block::Paragraph(vec![Inline::Text("ABSTRACT".into())]));
    let mut opts=options(json!({"mode":"exact","pt":18}));opts.publication_front_block_count=1;
    let (tex,_,context)=render_document_with_geometry(&doc,&opts,&|_|None).unwrap();
    assert_eq!(context.unwrap().paragraphs.len(),1);
    assert_eq!(tex.matches("\\readmdtypedparagraph{1}").count(),1);
    let body=tex.split_once("\\begin{document}").unwrap().1;
    for word in ["ABSTRACT","QUOTE","LIST"] {assert_eq!(body.matches(word).count(),1,"{word}");}
    assert!(body.contains("\\begin{quote}")&&body.contains("\\begin{itemize}")&&body.contains("\\begin{tabularx}"));
}

#[test]
fn tex_typed_paragraph_section_context_uses_actual_native_width_and_textheight() {
    let source="<!-- readmd-section:v1 {\"id\":\"narrow\",\"page\":{\"size\":\"A5\",\"orientation\":\"landscape\",\"columns\":2}} -->\n\nNARROW\n";
    let doc=crate::md_ast::parse(source);let mut opts=options(json!({"mode":"exact","pt":18}));
    opts.publication_sections=crate::publication_sections::resolve(&doc,opts.publication_style.as_ref().unwrap()).unwrap();
    let (tex,_,context)=render_document_with_geometry(&doc,&opts,&|_|None).unwrap();
    assert_eq!(context.unwrap().paragraphs.len(),1);
    assert!(tex.contains("layoutwidth=210mm,layoutheight=148mm"));
    assert!(tex.contains("\\readmdpwidth=\\linewidth")&&tex.contains("\\readmdpavailable=\\textheight"));
    let visible=tex.split_once("\\begin{document}").unwrap().1;
    assert!(visible.find("\\newgeometry").unwrap()<visible.find("\\readmdtypedparagraph{1}").unwrap());
    assert!(visible.find("\\begin{multicols}{2}").unwrap()<visible.find("\\readmdtypedparagraph{1}").unwrap());
}

#[test]
fn tex_typed_paragraph_absent_and_null_modes_keep_legacy_output_and_no_metric_resources() {
    let doc=crate::md_ast::parse("# Old heading\n\nOld body $x^2$.\n");
    let value=json!({"frontMatter":{"mode":"none"},"typography":{"size":11,"lineHeight":1.55}});
    let mut opts=LatexOptions{publication_style:Some(crate::export_styles::sanitize(&value)),..Default::default()};
    let a=render_document_with_geometry(&doc,&opts,&|_|None).unwrap();
    opts.publication_style.as_mut().unwrap()["typography"]["lineSpacing"]=Value::Null;
    let b=render_document_with_geometry(&doc,&opts,&|_|None).unwrap();
    assert_eq!(a.0,b.0);
    assert!(!a.1&&a.2.is_none()&&b.2.is_none());
    assert!(!a.0.contains("readmdtypedparagraph")&&!a.0.contains("readmd-p04-tex-"));
    assert!(a.0.contains("\\fontsize{11}{17.05}\\selectfont"));
}

#[test]
fn tex_typed_paragraph_worker_alias_is_canonical_but_untrusted_log_paths_are_not_resolved() {
    let (source, mut context) = render("BODY", json!({"mode":"exact","pt":18}));
    let worker = tempfile::tempdir().unwrap();
    let alias = worker.path().join(".");
    context.bind_worker_directory(&alias).unwrap();
    let canonical = dunce::canonicalize(worker.path()).unwrap();
    for directory in [alias.as_path(), worker.path(), canonical.as_path()] {
        let log = packet(&context, 18., 30., 493., 340.,
            &directory.join("document.tex").to_string_lossy());
        assert!(context.diagnostic(&log, &source, &alias).is_some(), "{log}");
    }
    let foreign = tempfile::tempdir().unwrap();
    let log = packet(&context, 18., 30., 493., 340.,
        &foreign.path().join("document.tex").to_string_lossy());
    assert!(context.diagnostic(&log, &source, worker.path()).is_none());
    let valid = packet(&context, 18., 30., 493., 340., "document.tex");
    assert!(context.diagnostic(&valid, &source, foreign.path()).is_none());
}

#[test]
fn tex_typed_paragraph_trusted_worker_spellings_remove_curdir_but_keep_parentdir() {
    // Synthetic trusted aliases make the lexical regression deterministic even
    // when the host temp path happens to equal its canonical spelling.
    let bare = Path::new(if cfg!(windows) { r"C:\ReadMD~1\worker" } else { "/var/readmd-alias/worker" });
    let alias = bare.join(".");
    let paths = trusted_worker_source_paths(&alias);
    // Path equality ignores CurDir itself; compare exact OS spellings instead.
    assert_eq!(paths[0].as_os_str(), alias.join("document.tex").as_os_str());
    assert_eq!(paths[1].as_os_str(), bare.join("document.tex").as_os_str());
    assert_ne!(paths[0].as_os_str(), paths[1].as_os_str());

    let parent_alias = bare.join("child").join("..").join(".");
    let paths = trusted_worker_source_paths(&parent_alias);
    let expected = parent_alias.join("document.tex");
    assert_eq!(paths[1].as_os_str(), expected.as_os_str());
    assert_ne!(paths[1].as_os_str(), bare.join("document.tex").as_os_str());
}

#[cfg(unix)]
#[test]
fn tex_typed_paragraph_bound_symlink_alias_accepts_its_bare_spelling_only() {
    let fixture = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(fixture.path()).unwrap();
    let worker = root.join("worker");
    std::fs::create_dir(&worker).unwrap();
    let alias = root.join("bound-alias");
    std::os::unix::fs::symlink(&worker, &alias).unwrap();
    let (source, mut context) = render("BODY", json!({"mode":"exact","pt":18}));
    std::fs::write(worker.join("document.tex"), &source).unwrap();
    let bound = alias.join(".");
    context.bind_worker_directory(&bound).unwrap();
    for directory in [worker.as_path(), alias.as_path(), bound.as_path()] {
        let log = packet(&context, 18., 30., 493., 340., &directory.join("document.tex").to_string_lossy());
        assert!(context.diagnostic(&log, &source, &bound).is_some(), "{log}");
    }
    let unbound = root.join("unbound-alias");
    std::os::unix::fs::symlink(&worker, &unbound).unwrap();
    let log = packet(&context, 18., 30., 493., 340., &unbound.join("document.tex").to_string_lossy());
    assert!(context.diagnostic(&log, &source, &bound).is_none(), "A log alias is never resolved to the bound worker");
}

#[test]
fn tex_typed_paragraph_equivalent_and_network_log_paths_remain_untrusted() {
    let (source, mut context) = render("BODY", json!({"mode":"exact","pt":18}));
    let worker = tempfile::tempdir().unwrap();
    context.bind_worker_directory(worker.path()).unwrap();
    std::fs::write(worker.path().join("document.tex"), &source).unwrap();
    std::fs::create_dir(worker.path().join("child")).unwrap();
    // These on-disk paths reach the generated file, but were never supplied as
    // trusted worker spellings. Neither filesystem nor lexical log resolution
    // may make them eligible for geometry promotion.
    let paths = [
        worker.path().join("child").join("..").join("document.tex").to_string_lossy().into_owned(),
        worker.path().join(".").join(".").join("document.tex").to_string_lossy().into_owned(),
        r"\\readmd-path-must-not-be-resolved.invalid\share\document.tex".into(),
        r"\\?\UNC\readmd-path-must-not-be-resolved.invalid\share\document.tex".into(),
        "//readmd-path-must-not-be-resolved.invalid/share/document.tex".into(),
    ];
    for path in paths {
        let log = packet(&context, 18., 30., 493., 340., &path);
        assert!(context.diagnostic(&log, &source, worker.path()).is_none(), "{path}");
    }
}

#[test]
fn tex_typed_paragraph_nonce_packet_is_bound_to_source_worker_and_registered_request() {
    let (source,mut context)=render("BODY",json!({"mode":"exact","pt":18}));
    let worker=tempfile::tempdir().unwrap();context.bind_worker_directory(worker.path()).unwrap();
    for location in ["document.tex".to_string(),"./document.tex".to_string(),worker.path().join("document.tex").to_string_lossy().into_owned()] {
        let log=packet(&context,18.,30.,493.228346,340.15748,&location);
        let error=context.diagnostic(&log,&source,&dunce::canonicalize(worker.path()).unwrap()).unwrap();
        let body=crate::latex_compile::error_body(&error);
        assert_eq!(body["error_code"],"publication_layout_geometry");
        assert_eq!(body["details"]["requestedPt"],18.);
        assert!((body["details"]["requiredPt"].as_f64().unwrap()-30.).abs()<0.00003);
        assert!((body["details"]["availableHeightPt"].as_f64().unwrap()-340.15748).abs()<0.00003);
    }
    let log=packet(&context,18.,30.,493.,340.,"document.tex");
    assert!(context.diagnostic(&log,&format!("{source}\nchanged"),worker.path()).is_none());
    let other=tempfile::tempdir().unwrap();assert!(context.diagnostic(&log,&source,other.path()).is_none());
    assert!(context.diagnostic(&log,&source,Path::new("relative")).is_none());
}

#[test]
fn tex_typed_paragraph_nonce_packet_rejects_spoofed_truncated_duplicate_and_unrelated_errors() {
    let (source,mut context)=render("BODY",json!({"mode":"exact","pt":18}));
    let worker=tempfile::tempdir().unwrap();context.bind_worker_directory(worker.path()).unwrap();
    let log=packet(&context,18.,30.,493.,340.,"document.tex");
    let m=&context.marker;
    let variants=[log.replace(m,"readmd-p04-tex-not-this-task:"),log.replace("index=1","index=2"),
        log.replace("mode=1","mode=2"),log.replace("reserved=1","reserved=2"),log.replace(" END\n"," "),
        log.replace("width=", "unrecognized="),log.replace("index=1", "index=1.0"),
        log.replace("./document.tex", "../document.tex").replace("document.tex:180", "../document.tex:180"),
        log.replace("document.tex:180", "another/document.tex:180"),log.replace("document.tex:180", "document.tex.backup:180"),
        format!("{log}{log}"),format!("{log}\n! Unexpected error"),format!("{log}\nMissing character: forbidden glyph"),
        format!("{log}\ndocument.tex:181: Package fontspec Error: missing font."),
        packet(&context,20.,30.,493.,340.,"document.tex"),packet(&context,18.,10.,493.,340.,"document.tex"),
        packet(&context,18.,30.,0.,340.,"document.tex")];
    for value in variants {assert!(context.diagnostic(&value,&source,worker.path()).is_none(),"{value}");}
}

#[test]
fn tex_typed_paragraph_minimum_mode_only_promotes_real_page_height_overflow() {
    for mode in [json!({"mode":"multiple","multiple":2}),json!({"mode":"atLeast","pt":20})] {
        let (source,mut context)=render("BODY",mode);let worker=tempfile::tempdir().unwrap();
        context.bind_worker_directory(worker.path()).unwrap();
        let requested=context.paragraphs[0].requested;
        let valid=packet(&context,requested,400.,493.,340.,"document.tex");
        assert!(context.diagnostic(&valid,&source,worker.path()).is_some());
        let fake=packet(&context,requested,30.,493.,340.,"document.tex");
        assert!(context.diagnostic(&fake,&source,worker.path()).is_none(),"Minimum modes may expand; taller-than-request is not an error");
    }
}

#[test]
fn tex_typed_paragraph_exact_keeps_safe_boundary_to_a_preceding_tall_story() {
    let (tex,_)=render("# Heading\n\nBODY\n",json!({"mode":"exact","pt":18}));
    assert!(tex.contains("\\prevdepth+\\readmdpheight-\\readmdprequested"));
    assert!(tex.contains("\\penalty10000\\vskip\\readmdpboundary"));
    assert!(tex.contains("\\lineskiplimit=-\\maxdimen"));
    assert!(tex.replace("\r\n", "\n").contains("#6\\par\n  \\endgroup"),"The original paragraph and heading hooks are active in the visible flow");
}

#[test]
fn tex_typed_paragraph_real_tex_export_preserves_source_sparse_input_and_reports_only_actual_losses() {
    let dir=tempfile::tempdir().unwrap();let target=dir.path().join("Actual.tex");let input=dir.path().join("Source.md");
    let original="\u{feff}<!-- readmd:layout v1 {\"id\":\"body\",\"text\":{\"font\":\"TeX Gyre Termes\",\"size\":24},\"paragraph\":{\"lineSpacing\":{\"mode\":\"exact\",\"pt\":30}}} -->\r\n\r\nBODY  \r\nEND\r\n";
    std::fs::write(&input,original).unwrap();let value=json!({"frontMatter":{"mode":"none"}});let before=value.clone();
    let result=crate::mdexport::export_tex(original,dir.path().to_str().unwrap(),target.to_str().unwrap(),&value,"Synthetic").unwrap();
    assert!(result.ok);let tex=std::fs::read_to_string(&target).unwrap();
    assert!(tex.contains("\\readmdtypedparagraph{1}{1}{30.00000000}{24.00000000}"));
    assert!(!tex.contains("readmd:layout"));assert!(!tex.contains("_publicationLineSpacingLegacy"));
    assert_eq!(std::fs::read(input).unwrap(),original.as_bytes());assert_eq!(value,before);
    assert!(!result.warns.unwrap().iter().any(|w|w=="publication_local_style_unsupported"));
    assert!(!dir.path().join("Actual.assets").exists(),"Pure text added no staged font/image resource");
}

#[test]
fn tex_typed_paragraph_internal_legacy_context_survives_sparse_preset_override() {
    let dir=tempfile::tempdir().unwrap();let target=dir.path().join("Actual.tex");
    let resolved=typography::ResolvedOptions::from_request(&json!({"typography":{"lineHeight":1.55}})).unwrap()
        .merge(&raw(json!({"mode":"exact","pt":18}))).unwrap();
    let result=crate::mdexport::export_document_with_resolved_options("tex","BODY\n\n> QUOTE\n","",target.to_str().unwrap(),&resolved,
        "Synthetic",dir.path(),dir.path()).unwrap();assert!(result.ok);
    let tex=std::fs::read_to_string(target).unwrap();
    assert!(tex.contains("\\fontsize{11}{17.05}\\selectfont"));
    assert!(tex.contains("\\readmdtypedparagraph{1}{1}{18.00000000}{11.00000000}"));
    assert_eq!(tex.matches("\\readmdtypedparagraph{1}").count(),1);
    assert!(!tex.contains("_publicationLineSpacingLegacy"));
    assert!(resolved.values()["typography"].get("lineHeight").is_none());
}

fn app(root:&Path)->std::sync::Arc<crate::App> {
    let assets=root.join("assets");std::fs::create_dir_all(&assets).unwrap();
    std::sync::Arc::new(crate::App::bootstrap(crate::paths::AppPaths::with_dirs(&root.join("profile"),root,&assets)).unwrap())
}

#[test]
fn tex_typed_paragraph_supported_body_reaches_both_real_engine_gates_and_preserves_old_files() {
    let dir=tempfile::tempdir().unwrap();let app=app(dir.path());let source="FIRST  \nSECOND\n";
    for engine in ["xelatex","pdflatex"] {
        let mut value=raw(json!({"mode":"exact","pt":18}));value["pdf"]=json!({"engine":engine});
        assert_eq!(typography::validate_export("pdf",source,&value),Ok(()));
        assert_eq!(typography::validate_pdf_preview("pdf",source,&value),Ok(()));
        let target=dir.path().join(format!("Protected-{engine}.pdf"));std::fs::write(&target,b"KEEP PDF").unwrap();
        let direct=crate::latex_compile::export_pdf_with_data(source,"",target.to_str().unwrap(),&value,"Synthetic",&app.paths.data_dir,&app.paths.assets_dir).unwrap_err();
        assert_eq!(direct,"latex_engine_unavailable");
        for route in ["/api/export","/api/export/preview"] {
            let (status,body)=crate::server::call_in_process(&app,"POST",route,Some(&json!({
                "format":"pdf","content":source,"out_path":target,"options":value,"suggestedName":"Synthetic"
            })));
            assert_eq!(status,200,"{body}");assert_eq!(body["error_code"],"latex_engine_unavailable","{body}");
            assert!(body.get("key").is_none());
        }
        let result=crate::mcp::call_tool(&app,"readmd_export_document",&json!({
            "markdown_content":source,"output_format":"pdf","output_path":target,"options":value,"confirm":true,"overwrite":true
        }));
        assert_eq!(result["isError"],true,"{result}");assert_eq!(result["structuredContent"]["error_code"],"latex_engine_unavailable","{result}");
        assert_eq!(std::fs::read(target).unwrap(),b"KEEP PDF");
    }
}

#[test]
fn tex_typed_paragraph_invalid_or_unsupported_input_refuses_before_any_resource_or_target_change() {
    let dir=tempfile::tempdir().unwrap();let target=dir.path().join("Protected.tex");std::fs::write(&target,b"KEEP TEX").unwrap();
    let heading="<!-- readmd:layout v1 {\"paragraph\":{\"lineSpacing\":{\"mode\":\"exact\",\"pt\":18}}} -->\n\n# Heading\n";
    let body="![Image](not-read.png)\n";
    for (source,value,error) in [(heading,json!({}),typography::UNSUPPORTED),
        (body,raw(json!({"mode":"exact","pt":18})),typography::UNSUPPORTED),
        (body,json!({"typography":{"lineSpacing":{"mode":"exact","pt":18},"lineHeight":1.5}}),typography::INVALID)] {
        let before=value.clone();
        assert_eq!(crate::mdexport::export_tex(source,dir.path().to_str().unwrap(),target.to_str().unwrap(),&value,"Synthetic").unwrap_err(),error);
        assert_eq!(std::fs::read(&target).unwrap(),b"KEEP TEX");assert_eq!(value,before);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),1);
    }
    for format in ["html","epub"] {
        assert_eq!(typography::validate_export(format,"BODY",&raw(json!({"mode":"exact","pt":18}))),Err(typography::UNSUPPORTED));
    }
}


#[test]
fn tex_typed_paragraph_geometry_producer_emits_an_explicit_space_token() {
    let packets: Vec<_> = MACROS.lines().filter(|line| line.contains(r"\typeout{\readmdpmarker")).collect();
    assert_eq!(packets.len(), 9, "BEGIN, seven ordered fields and END are emitted");
    for line in packets {
        assert!(line.trim_start().starts_with(r"\typeout{\readmdpmarker\space "), "{line}");
        assert!(!line.contains(r"\typeout{\readmdpmarker "), "A control-word delimiter space is not an output token");
    }
    let (source, mut context) = render("FIRST  \nSECOND $\\dfrac{X}{\\dfrac{Y}{Z}}$  \nTHIRD", json!({"mode":"exact","pt":18}));
    let worker = tempfile::tempdir().unwrap();
    context.bind_worker_directory(worker.path()).unwrap();
    let log = packet(&context, 18., 30., 493., 340., "./document.tex")
        .replace(":180: Package", ":208: Package");
    let error = context.diagnostic(&log, &source, worker.path()).unwrap();
    assert_eq!(crate::latex_compile::error_body(&error)["error_code"], "publication_layout_geometry");
}

#[test]
fn tex_typed_paragraph_geometry_parser_keeps_exact_frame_separators() {
    let (source, mut context) = render("BODY", json!({"mode":"exact","pt":18}));
    let worker = tempfile::tempdir().unwrap();
    context.bind_worker_directory(worker.path()).unwrap();
    let log = packet(&context, 18., 30., 493., 340., "./document.tex");
    let marker = &context.marker;
    for invalid in [
        log.replace(&format!("{marker} "), marker),
        log.replace(&format!("{marker} "), &format!("{marker}  ")),
        log.replace(&format!("{marker} "), &format!("{marker}\t")),
    ] {
        assert!(context.diagnostic(&invalid, &source, worker.path()).is_none(), "{invalid}");
    }
}


#[test]
fn tex_typed_paragraph_real_managed_xetex_packet_and_halt_tail_promote_geometry() {
    let (source, mut context) = render("\u{feff}FIRST  \r\nSECOND $\\dfrac{X}{\\dfrac{Y}{Z}}$  \r\nTHIRD\r\n", json!({"mode":"exact","pt":18}));
    let worker = tempfile::tempdir().unwrap();
    context.bind_worker_directory(worker.path()).unwrap();
    let raw = include_str!("fixtures/publication-tex-geometry-xetex.stderr.txt");
    assert_eq!(raw.matches("<READMD_CONTEXT_MARKER>").count(), 9);
    let log = raw.replace("<READMD_CONTEXT_MARKER>", &context.marker);
    let error = context.diagnostic(&log, &source, worker.path()).unwrap();
    let body = crate::latex_compile::error_body(&error);
    assert_eq!(body["error_code"], "publication_layout_geometry");
    assert_eq!(body["details"]["blockIndex"], 1);
    assert_eq!(body["details"]["requestedPt"], 18.);
    let bp = |value: f64| value / 65536. * 72. / 72.27;
    assert!((body["details"]["requiredPt"].as_f64().unwrap()-bp(2385120.)).abs()<0.000001);
    assert!((body["details"]["availableWidthPt"].as_f64().unwrap()-bp(32445430.)).abs()<0.000001);
    assert!((body["details"]["availableHeightPt"].as_f64().unwrap()-bp(47922272.)).abs()<0.000001);
}

#[test]
fn tex_typed_paragraph_real_packet_still_requires_exact_nonce_source_and_worker() {
    let (source, mut context) = render("BODY", json!({"mode":"exact","pt":18}));
    let worker = tempfile::tempdir().unwrap();
    context.bind_worker_directory(worker.path()).unwrap();
    let raw = include_str!("fixtures/publication-tex-geometry-xetex.stderr.txt");
    assert!(context.diagnostic(raw, &source, worker.path()).is_none());
    let log = raw.replace("<READMD_CONTEXT_MARKER>", &context.marker);
    assert!(context.diagnostic(&log, &source, worker.path()).is_some());
    assert!(context.diagnostic(&log, &format!("{source}\nchanged"), worker.path()).is_none());
    let other = tempfile::tempdir().unwrap();
    assert!(context.diagnostic(&log, &source, other.path()).is_none());
    for invalid in [
        log.replace("requested=1184071", "requested=1184072.0"),
        log.replace("mode=1", "mode=2"),
        log.replace("./document.tex:212", "../document.tex:212"),
        format!("{log}\n! An unrelated engine error"),
        format!("{log}\nMissing character: an unrelated missing glyph"),
    ] {
        assert!(context.diagnostic(&invalid, &source, worker.path()).is_none(), "{invalid}");
    }
}
