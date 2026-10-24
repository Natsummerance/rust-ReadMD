//! Caption author sizes are consumed; explicit Word RTL still fails closed.
//! Real in-process entrypoints and writers; no external engine is simulated.
use crate::md_ast::{Block, Document, Inline};
use crate::publication_inline_text::UNSUPPORTED;
use serde_json::{json, Value};
use std::{cell::Cell, fs, path::Path, sync::Arc};

const CAPTION: &str = "CaptionGuard";
const ORIGINAL: &[u8] = b"KEEP EXISTING CAPTION OUTPUT";

#[test]
fn caption_shaping_preflight_precedes_missing_citations_staging_and_actual_api_publication(){
    let root=tempfile::tempdir().unwrap();let assets=root.path().join("not-created-assets");let data=root.path().join("not-created-data");
    let source=source(Some(json!({"size":30,"sizeCs":10}))).replace(CAPTION,"العربية")+"\nMissing [@missing-caption-shaping].\n\n![probe](missing-probe.png)\n";
    let mut options=options("rust");options["bibliography"]=json!("missing-caption-shaping.json");
    let target=root.path().join("protected.pdf");fs::write(&target,ORIGINAL).unwrap();
    for error in [
        crate::mdexport::export_document_with_data("pdf",&source,root.path().to_str().unwrap(),target.to_str().unwrap(),&options,"Caption",&assets,&data).unwrap_err(),
        crate::mdexport::export_pdf(&source,root.path().to_str().unwrap(),target.to_str().unwrap(),&options,"Caption",&assets).unwrap_err(),
        crate::export_preview::prepare_with_data("pdf",&source,root.path().to_str().unwrap(),&options,"Caption",&assets,&data).err().unwrap(),
    ]{
        assert!(error.starts_with("author_inline_text_pdf_shaping_unsupported: "),"{error}");
        let details:Value=serde_json::from_str(error.split_once(": ").unwrap().1).unwrap();
        assert_eq!(details["consumer"],"rustPdf");assert_eq!(details["coordinateSpace"],"textUtf8Bytes");
        assert_eq!(fs::read(&target).unwrap(),ORIGINAL);assert!(!assets.exists()&&!data.exists());
    }
    let app=app(root.path());
    let mut request=json!({"format":"pdf","content":source,"baseDir":root.path(),"out_path":target,"options":options});
    for endpoint in ["/api/export","/api/export/preview"]{
        let(status,body)=crate::server::call_in_process(&app,"POST",endpoint,Some(&request));assert_eq!(status,200,"{body}");
        assert_eq!(body["ok"],false,"{body}");assert_eq!(body["error_code"],"author_inline_text_pdf_shaping_unsupported","{body}");
        assert!(body.get("key").is_none());assert_eq!(fs::read(&target).unwrap(),ORIGINAL);
        request["suggestedName"]=json!("Caption");
    }
    let args=json!({"output_format":"pdf","markdown_content":source,"base_dir":root.path(),"output_path":target,"options":options,"confirm":true});
    let missing=crate::mcp::call_tool(&app,"readmd_export_document",&args);
    assert_eq!(missing["isError"],true);assert_eq!(missing["structuredContent"]["error_code"],"output_exists");
    let mut args=args;args["overwrite"]=json!(true);
    let refused=crate::mcp::call_tool(&app,"readmd_export_document",&args);
    assert_eq!(refused["isError"],true,"{refused}");
    assert_eq!(refused["structuredContent"]["error_code"],"author_inline_text_pdf_shaping_unsupported","{refused}");
    assert_eq!(fs::read(&target).unwrap(),ORIGINAL);
    let mut managed=options.clone();managed["pdf"]["engine"]=json!("typst");
    assert_eq!(crate::mdexport::validate_caption_author_export("pdf",&source,&managed),Ok(()),"a built-in PDF limitation must not reject a capable engine");
}

fn source(author: Option<Value>) -> String {
    let directive = author.map(|author| format!("{}\n\n", crate::publication_blocks::comment(
        &json!({"id":"caption-author","text":{"author":author}})).unwrap())).unwrap_or_default();
    format!("---\nlang: en-US\n---\n\n| A | B |\n| --- | --- |\n| CellGuardA | CellGuardB |\n\n{directive}:label[tbl:caption-guard] **{CAPTION}**\n\nFollowingGuard.\n")
}

fn unsupported_direction() -> String { source(Some(json!({"size":11,"rtl":true}))) }

fn options(engine: &str) -> Value {
    json!({"frontMatter":{"mode":"none"},"cover":{"enabled":false},"toc":{"enabled":false},
        "footer":{"pageNumbers":false},"typography":{"font":"Helvetica","size":11},
        "tex":{"useCtex":false,"bibEngine":"","renderBibliography":false},"pdf":{"engine":engine}})
}

fn app(root: &Path) -> Arc<crate::App> {
    let assets = root.join("assets"); fs::create_dir_all(&assets).unwrap();
    Arc::new(crate::App::bootstrap(crate::paths::AppPaths::with_dirs(&root.join("profile"),root,&assets)).unwrap())
}

fn document(raw: &str) -> Document {
    crate::publication_blocks::validate_source(raw).unwrap();
    let doc = crate::md_ast::parse(raw);
    assert_eq!(doc.blocks.iter().filter(|block| matches!(block.unstyled(),Block::Table {..})).count(),1);
    let captions = doc.blocks.iter().filter(|block| matches!(block.unstyled(),Block::Caption {..})).collect::<Vec<_>>();
    assert_eq!(captions.len(),1,"Fixture must produce a real typed caption: {:?}",doc.blocks);
    let Block::Caption {label,inlines} = captions[0].unstyled() else { unreachable!() };
    assert_eq!(label.id,"tbl:caption-guard");
    assert!(crate::md_ast::inline_plain(inlines).contains(CAPTION));
    doc
}

fn image_before_caption(mut doc: Document) -> Document {
    doc.blocks.insert(0,Block::Paragraph(vec![Inline::Image {src:"resource-probe.png".into(),alt:"ResourceProbe".into(),title:String::new()}]));
    doc
}

fn typed(body: &Value) {
    assert_eq!(body["ok"],false,"{body}");
    assert_eq!(body["error_code"],UNSUPPORTED,"{body}");
}

fn typed_mcp(result: &Value) {
    assert_eq!(result["isError"],true,"{result}"); typed(&result["structuredContent"]);
    let text: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text,result["structuredContent"]);
}

#[test]
fn caption_author_source_is_a_real_typed_caption_and_docx_support_is_explicit() {
    let raw = source(Some(json!({"size":11}))); let doc = document(&raw);
    let caption = doc.blocks.iter().find(|block| matches!(block.unstyled(),Block::Caption {..})).unwrap();
    let Block::Styled {attributes,..} = caption else { panic!("Author directive was lost before the caption") };
    assert_eq!(attributes["text"]["author"],json!({"size":11}));
    assert_eq!(crate::publication_blocks::validate_author_consumer(&raw,true),Ok(()));
    assert_eq!(crate::publication_blocks::validate_author_document(&doc,true),Ok(()));
    assert_eq!(crate::publication_blocks::validate_author_consumer(&raw,false),Err(UNSUPPORTED));
    assert_eq!(crate::publication_blocks::validate_author_document(&doc,false),Err(UNSUPPORTED));
    assert_eq!(crate::publication_blocks::validate_author_render_document(&doc),Ok(()));
    assert_eq!(crate::publication_blocks::validate_author_render_consumer(&raw),Ok(()));
    let direction=unsupported_direction();let direction_doc=document(&direction);
    assert_eq!(crate::publication_blocks::validate_author_document(&direction_doc,true),Ok(()));
    assert_eq!(crate::publication_blocks::validate_author_render_document(&direction_doc),Err(UNSUPPORTED));
    assert_eq!(crate::publication_blocks::validate_author_render_consumer(&direction),Err(UNSUPPORTED));
}

#[test]
fn caption_author_shared_exports_refuse_before_dependencies_and_preserve_existing_targets() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("not-created-profile"); let assets = root.path().join("not-created-assets");
    let raw = format!("{}\nMissing citation [@caption-resource-probe].\n\n![probe](resource-probe.png)\n",unsupported_direction());
    for (format,engine) in [("html","rust"),("epub","rust"),("pdf","rust"),("pdf","typst"),("pdf","xelatex"),("tex","xelatex")] {
        let target = root.path().join(format!("protected-{format}-{engine}.{format}")); fs::write(&target,ORIGINAL).unwrap();
        let mut value = options(engine); value["bibliography"] = json!("missing-caption-references.json");
        let error = crate::mdexport::export_document_with_data(format,&raw,root.path().to_str().unwrap(),target.to_str().unwrap(),&value,"Caption Guard",&assets,&data).unwrap_err();
        assert_eq!(error,UNSUPPORTED,"{format}/{engine}: dependency/engine resolution preceded capability refusal");
        assert_eq!(fs::read(&target).unwrap(),ORIGINAL);
        assert!(!target.with_extension("assets").exists());
    }
    assert!(!data.exists() && !assets.exists());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(),6,"Refused export staged an extra resource");
}

#[test]
fn caption_author_public_format_entrypoints_cannot_bypass_the_shared_guard() {
    let root = tempfile::tempdir().unwrap(); let target = root.path().join("protected.output");fs::write(&target,ORIGINAL).unwrap();
    let raw = unsupported_direction(); let value = options("rust"); let output = target.to_str().unwrap();
    let errors = [
        crate::mdexport::export_html(&raw,"",output,&value,"Caption Guard",root.path()).unwrap_err(),
        crate::mdexport::export_epub(&raw,"",output,&value,"Caption Guard").unwrap_err(),
        crate::mdexport::export_tex(&raw,"",output,&value,"Caption Guard").unwrap_err(),
        crate::mdexport::export_pdf(&raw,"",output,&value,"Caption Guard",root.path()).unwrap_err(),
        crate::typst_compile::export_pdf(&raw,"",output,&options("typst"),"Caption Guard",root.path(),root.path()).unwrap_err(),
        crate::latex_compile::export_pdf_with_data(&raw,"",output,&options("xelatex"),"Caption Guard",root.path(),root.path()).unwrap_err(),
    ];
    assert!(errors.iter().all(|error| error==UNSUPPORTED),"{errors:?}");
    assert_eq!(fs::read(&target).unwrap(),ORIGINAL);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(),1);
}

#[test]
fn caption_author_http_and_mcp_share_typed_refusal_and_original_target_bytes() {
    let root = tempfile::tempdir().unwrap(); let app = app(root.path()); let raw = unsupported_direction();
    let source_path = root.path().join("original.md");fs::write(&source_path,raw.as_bytes()).unwrap();
    for (format,engine) in [("html","rust"),("epub","rust"),("pdf","rust"),("pdf","typst"),("pdf","xelatex"),("tex","xelatex")] {
        let target = root.path().join(format!("http-mcp-{format}-{engine}.{format}"));fs::write(&target,ORIGINAL).unwrap();
        let value = options(engine);
        let (status,body) = crate::server::call_in_process(&app,"POST","/api/export",Some(&json!({"format":format,"content":raw,"baseDir":root.path(),"out_path":target,"options":value})));
        assert_eq!(status,200,"{body}");typed(&body);assert_eq!(fs::read(&target).unwrap(),ORIGINAL);
        let (tool,request) = if format=="epub" {
            ("readmd_export_epub",json!({"markdown_content":raw,"output_path":target,"base_dir":root.path(),"options":value,"title":"Caption Guard","language":"en-US","overwrite":true,"confirm":true}))
        } else {
            ("readmd_export_document",json!({"markdown_content":raw,"output_format":format,"output_path":target,"base_dir":root.path(),"options":value,"overwrite":true,"confirm":true}))
        };
        typed_mcp(&crate::mcp::call_tool(&app,tool,&request));
        assert_eq!(fs::read(&target).unwrap(),ORIGINAL);
    }
    assert_eq!(fs::read(&source_path).unwrap(),raw.as_bytes());
}

#[test]
fn caption_author_direct_ast_writers_refuse_before_image_callbacks_or_staging() {
    let root = tempfile::tempdir().unwrap(); let doc = image_before_caption(document(&unsupported_direction()));
    let value = options("rust"); let style = crate::export_styles::sanitize(&value);
    let tex_calls = Cell::new(0usize);
    let tex_options = crate::latex_writer::LatexOptions {publication_style:Some(style.clone()),bib_engine:String::new(),..Default::default()};
    let tex = crate::latex_writer::render_document_with_geometry(&doc,&tex_options,&|_| {tex_calls.set(tex_calls.get()+1);None});
    assert_eq!(tex.err().as_deref(),Some(UNSUPPORTED));assert_eq!(tex_calls.get(),0);
    let epub_calls = Cell::new(0usize);let request = crate::publication_navigation::Request::default();
    let epub = crate::epub_writer::render_chapters_with_navigation(&doc,"none",&mut |_| {epub_calls.set(epub_calls.get()+1);None},&style,0,&request);
    assert_eq!(epub.err().unwrap().code,UNSUPPORTED);assert_eq!(epub_calls.get(),0);
    let staging = root.path().join("not-created-typst-staging");
    assert_eq!(crate::typst_writer::render(&doc,&value,"",&staging,"Caption Guard").err().as_deref(),Some(UNSUPPORTED));
    assert!(!staging.exists());assert_eq!(fs::read_dir(root.path()).unwrap().count(),0);
}

#[test]
fn caption_author_direct_pdf_json_refuses_object_and_layered_styles_before_callback_and_commit() {
    let root = tempfile::tempdir().unwrap();let target = root.path().join("protected.pdf");fs::write(&target,ORIGINAL).unwrap();
    let doc = image_before_caption(document(&unsupported_direction()));let raw = crate::md_ast::to_publication_render_json(&doc.blocks);
    assert_eq!(raw.as_array().unwrap().iter().find(|block|block["type"]=="caption").unwrap()["publicationStyle"]["text"]["author"],json!({"size":11,"rtl":true}));
    for layered in [false,true] {
        let mut blocks = raw.clone();
        if layered {let caption = blocks.as_array_mut().unwrap().iter_mut().find(|block|block["type"]=="caption").unwrap();caption["publicationStyle"]=json!([caption["publicationStyle"].clone()]);}
        let calls = Cell::new(0usize);let mut warnings = Vec::new();
        let error = crate::pdf_render::render(&blocks,target.to_str().unwrap(),&crate::export_styles::sanitize(&options("rust")),root.path().to_str().unwrap(),&|_| {calls.set(calls.get()+1);None},&mut warnings).unwrap_err();
        assert_eq!(error.message,UNSUPPORTED);assert_eq!(calls.get(),0);assert!(warnings.is_empty());
        assert_eq!(fs::read(&target).unwrap(),ORIGINAL);
    }
}

#[test]
fn caption_author_reader_and_preview_guards_do_not_redefine_the_mcp_prepare_contract() {
    let root = tempfile::tempdir().unwrap(); let raw = unsupported_direction();let data = root.path().join("not-created-profile");let assets = root.path().join("not-created-assets");
    assert_eq!(crate::publication_typography::validate_reader_view(&raw,&json!({})),Err(UNSUPPORTED));
    assert_eq!(crate::publication_blocks::view(&raw).unwrap_err(),UNSUPPORTED);
    for (format,engine) in [("pdf","rust"),("pdf","typst"),("pdf","xelatex"),("docx","rust")] {
        let error = crate::export_preview::prepare_with_data(format,&raw,"",&options(engine),"Caption Guard",&assets,&data).err().unwrap();
        assert_eq!(error,UNSUPPORTED,"{format}/{engine}");
    }
    assert_eq!(crate::epub_preview::prepare(&raw,"",&options("rust"),"Caption Guard").unwrap_err(),UNSUPPORTED);
    assert!(!data.exists() && !assets.exists());assert_eq!(fs::read_dir(root.path()).unwrap().count(),0);
    let app = app(root.path());
    let source_path = root.path().join("reader-source.md");fs::write(&source_path,raw.as_bytes()).unwrap();
    let old_target = root.path().join("reader-protected.pdf");fs::write(&old_target,ORIGINAL).unwrap();
    let preview_options = crate::publication_profile::resolve_for_format("pdf",&raw,&options("rust")).unwrap();
    let preview_key = crate::export_preview::key_with_data("pdf",&raw,"",preview_options.values(),"Caption Guard",&app.paths.data_dir,&app.paths.assets_dir);
    assert!(crate::export_preview::cached(&preview_key).is_none());
    let (status,body) = crate::server::call_in_process(&app,"POST","/api/academic/document",Some(&json!({"content":raw,"baseDir":root.path(),"options":{},"view":true})));
    assert_eq!(status,422,"{body}");typed(&body);
    // Default MCP preparation remains source-preserving. The explicit reader
    // view is now shared with HTTP; missing scope, wrong types and unknown
    // options must still fail schema validation before publication work.
    for request in [json!({"content":raw,"view":true}),json!({"content":raw,"options":{}}),json!({"content":raw,"baseDir":root.path(),"options":{},"view":"true"}),json!({"content":raw,"baseDir":root.path(),"view":true,"pagination":"all"})] {
        let result = crate::mcp::call_tool(&app,"readmd_academic_document",&request);
        assert_eq!(result["isError"],true,"{result}");
        assert_eq!(result["structuredContent"]["ok"],false);
        assert_eq!(result["structuredContent"]["error_code"],"invalid_arguments");
        let text: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(text,result["structuredContent"]);assert!(text.get("key").is_none());
        assert_eq!(fs::read(&source_path).unwrap(),raw.as_bytes());assert_eq!(fs::read(&old_target).unwrap(),ORIGINAL);
        assert!(crate::export_preview::cached(&preview_key).is_none());
    }
    let projected = crate::mcp::call_tool(&app,"readmd_academic_document",&json!({"content":raw,"baseDir":root.path(),"options":{},"view":true}));
    assert_eq!(projected["isError"],true,"{projected}");
    let projected_text: Value = serde_json::from_str(projected["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(projected_text,projected["structuredContent"]);typed(&projected_text);
    assert_eq!(fs::read(&source_path).unwrap(),raw.as_bytes());assert_eq!(fs::read(&old_target).unwrap(),ORIGINAL);
    assert!(crate::export_preview::cached(&preview_key).is_none());
    let result = crate::mcp::call_tool(&app,"readmd_academic_document",&json!({"content":raw,"baseDir":root.path(),"options":{}}));
    assert_ne!(result["isError"],true,"{result}");
    let text: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();assert_eq!(text,result["structuredContent"]);
    let projection: crate::publication_rich::Projection = serde_json::from_value(text["projection"].clone()).unwrap();
    projection.verify_source(&raw).unwrap();
    let source_doc = crate::md_ast::parse_prepared(&raw,&projection).unwrap();
    let prepared = text["content"].as_str().unwrap();let prepared_doc = crate::md_ast::parse(prepared);
    // Preparation legitimately expands :label into a caption marker/anchor.
    // Compare the complete typed documents, including table cells, rich title
    // and following paragraphs, instead of treating display bytes as source.
    assert_eq!(prepared_doc,source_doc,"MCP display projection changed author content or its typed caption");
    let original_layout = crate::publication_blocks::analyze(&raw).unwrap().nodes.into_iter().filter(|node|node.style.pointer("/text/author").is_some()).collect::<Vec<_>>();
    assert_eq!(original_layout.len(),1);
    let original_declaration = original_layout[0].directive.clone().unwrap();
    let displayed_declarations = pulldown_cmark::Parser::new_ext(prepared,crate::md_ast::options()).into_offset_iter().filter_map(|(event,range)| {
        match event {
            pulldown_cmark::Event::Html(html)|pulldown_cmark::Event::InlineHtml(html)
                if crate::publication_blocks::read_style(html.trim()).is_ok_and(|style|style.is_some()) => Some(prepared[range].trim()),
            _ => None,
        }
    }).collect::<Vec<_>>();
    assert_eq!(displayed_declarations,vec![&raw[original_declaration]],"MCP preparation rewrote or duplicated the controlled layout declaration");
    let [table,caption,following] = source_doc.blocks.as_slice() else {panic!("MCP preparation changed the complete table/caption/following-body structure")};
    let Block::Table {header,rows,..} = table else {panic!("MCP preparation dropped the real table")};
    assert_eq!(header.iter().map(|cell|crate::md_ast::inline_plain(cell)).collect::<Vec<_>>(),vec!["A","B"]);
    assert_eq!(rows.iter().map(|row|row.iter().map(|cell|crate::md_ast::inline_plain(cell)).collect::<Vec<_>>()).collect::<Vec<_>>(),vec![vec!["CellGuardA","CellGuardB"]]);
    let Block::Styled {attributes,block} = caption else {panic!("MCP preparation dropped the source-owned author style")};
    assert_eq!(attributes["text"]["author"],json!({"size":11,"rtl":true}));
    let Block::Caption {label,inlines} = block.as_ref() else {panic!("MCP preparation dropped the typed caption")};
    assert_eq!(label.id,"tbl:caption-guard");assert_eq!(crate::md_ast::inline_plain(inlines),format!(" — {CAPTION}"));
    assert!(inlines.iter().any(|inline|matches!(inline,Inline::Strong(children)if crate::md_ast::inline_plain(children)==CAPTION)));
    assert_eq!(following,&Block::Paragraph(vec![Inline::Text("FollowingGuard.".into())]));
    // Retained explicit Word RTL is still unsupported by a real reader; this
    // successful non-rendering preparation is not counted as reader support.
    assert_eq!(crate::publication_blocks::validate_author_render_document(&prepared_doc),Err(UNSUPPORTED));
    assert_eq!(fs::read(&source_path).unwrap(),raw.as_bytes());assert_eq!(fs::read(&old_target).unwrap(),ORIGINAL);
    assert!(crate::export_preview::cached(&preview_key).is_none());
    for (format,engine) in [("pdf","rust"),("pdf","typst"),("pdf","xelatex"),("docx","rust"),("epub","rust")] {
        let (status,body) = crate::server::call_in_process(&app,"POST","/api/export/preview",Some(&json!({"format":format,"content":raw,"options":options(engine),"suggestedName":"Caption Guard"})));
        assert_eq!(status,200,"{body}");typed(&body);assert!(body.get("key").is_none());
    }
    assert!(crate::export_preview::cached(&preview_key).is_none());
    assert_eq!(fs::read(&source_path).unwrap(),raw.as_bytes());assert_eq!(fs::read(&old_target).unwrap(),ORIGINAL);
}

#[test]
fn ordinary_and_empty_author_captions_keep_real_source_ast_json_and_writer_entrypoints() {
    let root = tempfile::tempdir().unwrap();let value = options("rust");let style = crate::export_styles::sanitize(&value);
    for author in [None,Some(json!({}))] {
        let raw = source(author);let doc = document(&raw);
        assert_eq!(crate::publication_blocks::validate_author_consumer(&raw,false),Ok(()));
        assert_eq!(crate::publication_blocks::validate_author_document(&doc,false),Ok(()));
        for (format,engine) in [("html","rust"),("epub","rust"),("pdf","rust"),("pdf","typst"),("pdf","xelatex"),("tex","xelatex")] {
            assert_eq!(crate::publication_typography::validate_export(format,&raw,&options(engine)),Ok(()),"{format}/{engine}");
            assert!(crate::mdexport::prepare_export_source(format,&raw,"",&options(engine)).is_ok());
        }
        assert!(crate::publication_typography::validate_reader_view(&raw,&value).is_ok());
        assert!(crate::publication_blocks::view(&raw).is_ok());
        assert!(crate::publication_typography::validate_preview(&raw,&value).is_ok());
        let typst = crate::typst_writer::render(&doc,&value,"",root.path(),"Caption Guard").unwrap();assert!(typst.text.contains(CAPTION));
        let tex_options = crate::latex_writer::LatexOptions {publication_style:Some(style.clone()),bib_engine:String::new(),..Default::default()};
        let tex = crate::latex_writer::render_document_with_geometry(&doc,&tex_options,&|_|None).unwrap().0;assert!(tex.contains(CAPTION));
        let chapters = crate::epub_writer::render_chapters_with_navigation(&doc,"none",&mut |_|None,&style,0,&crate::publication_navigation::Request::default()).unwrap();
        assert!(chapters.iter().any(|chapter|chapter.body.contains(CAPTION)&&chapter.body.contains("CellGuardA")));
        let blocks = crate::md_ast::to_publication_render_json(&doc.blocks);
        let bytes = crate::pdf_render::render_bytes(&blocks,&style,"",&|_|None,&mut Vec::new()).unwrap();
        let pdf = lopdf::Document::load_mem(&bytes).unwrap();let text = pdf.extract_text(&[1]).unwrap();
        assert!(text.contains(CAPTION)&&text.contains("CellGuardA"),"{text}");
    }
}

#[test]
fn ordinary_caption_real_supported_exports_do_not_gain_an_author_refusal() {
    let root = tempfile::tempdir().unwrap();let raw = source(None);
    for format in ["html","epub","pdf","tex"] {
        let target = root.path().join(format!("ordinary.{format}"));
        let result = crate::mdexport::export_document_with_data(format,&raw,root.path().to_str().unwrap(),target.to_str().unwrap(),&options("rust"),"Caption Guard",&root.path().join("assets"),&root.path().join("profile")).unwrap();
        assert!(result.ok);let bytes = fs::read(&target).unwrap();assert!(!bytes.is_empty());
        if format=="pdf" {let pdf = lopdf::Document::load_mem(&bytes).unwrap();assert!(pdf.extract_text(&[1]).unwrap().contains(CAPTION));}
        if format=="epub" {assert!(bytes.starts_with(b"PK"));}
        if format=="tex" {assert!(String::from_utf8(bytes).unwrap().contains(CAPTION));}
    }
}

#[test]
fn authored_caption_real_builtin_exports_keep_both_sizes_and_complete_body_content(){
    fn has_sized_text(node:&crate::math_document::XmlNode,points:u32,needle:&str)->bool{
        node.attributes.get("style").is_some_and(|style|style==&format!("font-size:{points}pt"))&&node.text().contains(needle)
            ||node.children().any(|child|has_sized_text(child,points,needle))
    }
    let root=tempfile::tempdir().unwrap();
    let raw=source(Some(json!({"size":30,"sizeCs":10,"cs":false,"rtl":false}))).replace(&format!("**{CAPTION}**"),&format!("**{CAPTION}** <span data-readmd-text=\"v1\" data-cs=\"true\">CaptionCs</span>"));
    let original=root.path().join("author-source.md");fs::write(&original,raw.as_bytes()).unwrap();
    for format in ["html","epub","pdf","tex"]{
        let target=root.path().join(format!("authored.{format}"));let result=crate::mdexport::export_document_with_data(format,&raw,root.path().to_str().unwrap(),target.to_str().unwrap(),&options("rust"),"Caption Guard",&root.path().join("assets"),&root.path().join("profile")).unwrap();
        assert!(result.ok);let bytes=fs::read(&target).unwrap();assert!(!bytes.is_empty());
        match format{
            "html"=>{
                let html=String::from_utf8(bytes).unwrap();let marker="<script type=\"application/json\" id=\"md-source\">";let from=html.find(marker).unwrap()+marker.len();
                let markdown:String=serde_json::from_str(html[from..].split("</script>").next().unwrap()).unwrap();let mut body=String::new();pulldown_cmark::html::push_html(&mut body,pulldown_cmark::Parser::new_ext(&markdown,crate::md_ast::options()));
                let dom=crate::math_document::parse_xml(&format!("<body>{body}</body>")).unwrap();
                assert!(has_sized_text(&dom,30,"Table 1")&&has_sized_text(&dom,30,CAPTION)&&has_sized_text(&dom,10,"CaptionCs"),"{body}");
                assert!(dom.text().contains("CellGuardA")&&dom.text().contains("FollowingGuard."));assert!(html.contains("window.ReadMDAuthorText.apply(root,"));
            },
            "epub"=>{
                let parts=crate::publication_typography_docx_tests::unzip_bytes(&bytes);let mut body=None;
                for(name,data)in parts{if !name.ends_with(".xhtml"){continue;}let text=String::from_utf8(data).unwrap();if text.contains("CellGuardA")&&text.contains(CAPTION){assert!(body.replace(text).is_none(),"Caption body duplicated across chapter resources");}}
                let body=body.expect("Actual EPUB chapter body missing");let from=body.find("<body").unwrap();let from=from+body[from..].find('>').unwrap()+1;let fragment=body[from..].split("</body>").next().unwrap();
                let dom=crate::math_document::parse_xml(&format!("<body xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\">{fragment}</body>")).unwrap();
                assert!(has_sized_text(&dom,30,"Table 1")&&has_sized_text(&dom,30,CAPTION)&&has_sized_text(&dom,10,"CaptionCs"),"{body}");assert!(dom.text().contains("FollowingGuard."));
            },
            "pdf"=>{
                let pdf=lopdf::Document::load_mem(&bytes).unwrap();let pages=pdf.get_pages().keys().copied().collect::<Vec<_>>();let text=pdf.extract_text(&pages).unwrap();for needle in ["Table 1",CAPTION,"CaptionCs","CellGuardA","FollowingGuard."]{assert!(text.contains(needle),"{text}");}
                let mut sizes=Vec::new();for id in pdf.get_pages().values(){let content=lopdf::content::Content::decode(&pdf.get_page_content(*id).unwrap()).unwrap();for operation in content.operations{if operation.operator=="Tf"{let size=match &operation.operands[1]{lopdf::Object::Integer(value)=>*value as f64,lopdf::Object::Real(value)=>*value as f64,_=>panic!("PDF font size must be numeric")};sizes.push(size);}}}
                for points in [30.,10.]{assert!(sizes.iter().any(|size|(*size-points).abs()<0.001),"missing actual PDF Tf {points}: {sizes:?}");}
            },
            "tex"=>{let text=String::from_utf8(bytes).unwrap();assert!(text.contains("\\fontsize{30bp}{\\baselineskip}\\selectfont{}Table 1"),"{text}");assert!(text.contains("\\fontsize{10bp}{\\baselineskip}\\selectfont{}CaptionCs"));for needle in [CAPTION,"CellGuardA","FollowingGuard."]{assert!(text.contains(needle));}},
            _=>unreachable!(),
        }
        assert_eq!(fs::read(&original).unwrap(),raw.as_bytes());
    }
}

#[test]
fn ordinary_documents_over_the_author_declaration_budget_do_not_gain_a_new_budget_error() {
    let doc = Document {blocks:vec![Block::Paragraph(vec![Inline::Text("ordinary".into())]);crate::publication_blocks::MAX_NODES+1],..Default::default()};
    assert_eq!(crate::publication_blocks::validate_author_document(&doc,false),Ok(()));
    assert_eq!(crate::publication_blocks::validate_author_document(&doc,true),Ok(()));
    assert_eq!(crate::publication_blocks::validate_author_render_document(&doc),Ok(()));
}
