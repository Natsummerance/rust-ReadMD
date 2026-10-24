//! Shared API integration. Native process/filesystem evidence is kept apart.
use serde_json::{json,Value};
fn app()->(tempfile::TempDir,std::sync::Arc<crate::App>){
    let dir=tempfile::tempdir().unwrap();let assets=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let app=std::sync::Arc::new(crate::App::bootstrap(crate::paths::AppPaths::with_dirs(&dir.path().join("profile"),dir.path(),&assets)).unwrap());(dir,app)
}
#[test]
fn author_inline_http_transaction_is_revision_bound_reversible_and_side_effect_free(){
    let(dir,app)=app();let source="\u{feff}# T\r\n\r\n😀 KEEP selected KEEP\r\n";
    let(_,read)=crate::server::call_in_process(&app,"POST","/api/publication/inline-text",Some(&json!({"action":"get","content":source})));
    let at=source.find("selected").unwrap();let from=source[..at].encode_utf16().count();
    let request=json!({"action":"apply","content":source,"base_revision":read["revision"],"from":from,"to":from+8,"patch":{"size":30,"sizeCs":10}});
    let(status,edit)=crate::server::call_in_process(&app,"POST","/api/publication/inline-text",Some(&request));assert_eq!(status,200,"{edit}");assert_eq!(edit["result"]["changed"],true);
    let output=edit["result"]["content"].as_str().unwrap();assert!(output.starts_with('\u{feff}')&&output.ends_with(" KEEP\r\n"));
    let inverse=crate::document_transaction::ContentTransaction{base_revision:edit["result"]["revision"].as_str().unwrap().into(),origin:"editor".into(),edits:serde_json::from_value(edit["result"]["inverse"].clone()).unwrap()};assert_eq!(crate::document_transaction::apply(output,&inverse).unwrap().content,source);
    let mut stale=request.clone();stale["content"]=json!(output);let(status,error)=crate::server::call_in_process(&app,"POST","/api/publication/inline-text",Some(&stale));assert_eq!(status,409);assert_eq!(error["error_code"],"transaction_conflict");
    assert!(!dir.path().join("readmd-assets").exists());
}
#[test]
fn author_inline_desktop_view_uses_numeric_projection_not_source_css(){
    let(_dir,app)=app();let source="<span data-readmd-text=\"v1\" data-size-pt=\"30\" data-size-cs-pt=\"10\">**A** 中文 العربية</span>\n";
    let(status,body)=crate::server::call_in_process(&app,"POST","/api/academic/document",Some(&json!({"content":source,"view":true})));
    assert_eq!(status,200,"{body}");assert!(body["author_text"]["runs"].as_array().unwrap().iter().any(|run|run["size"]==10.));
    assert!(body["content"].as_str().unwrap().contains("data-readmd-author-run="));assert!(body["author_text"].get("content").is_none());
    let invalid=source.replace("data-size-pt=\"30\"","data-size-pt=\"30;position:fixed\"");
    let(status,body)=crate::server::call_in_process(&app,"POST","/api/academic/document",Some(&json!({"content":invalid,"view":true})));assert_eq!(status,422);assert_eq!(body["error_code"],crate::publication_inline_text::INVALID);
}
#[test]
fn author_inline_explicit_word_rtl_is_not_falsely_projected_as_css_bidi(){
    let source="A <span data-readmd-text=\"v1\" data-size-cs-pt=\"10\" data-rtl=\"true\">123</span>";
    assert_eq!(crate::publication_inline_view::project(source,&json!({})).unwrap_err(),crate::publication_inline_text::UNSUPPORTED);
    assert!(crate::publication_inline_text::get(source).is_ok());
}
#[test]
fn author_inline_formula_source_is_readable_clearable_and_rejected_by_the_view(){
    let(dir,app)=app();
    for formula in ["$x$",":math[x^2]"]{
        let source=format!("A <span data-readmd-text=\"v1\" data-size-pt=\"30\">{formula}</span>");let original=source.clone();
        let(status,read)=crate::server::call_in_process(&app,"POST","/api/publication/inline-text",Some(&json!({"action":"get","content":source})));assert_eq!(status,200,"{read}");
        assert_eq!(read["revision"],crate::document_transaction::revision(&source));
        let span=read["nodes"].as_array().unwrap().iter().find(|node|node["kind"]=="span").unwrap();
        let(status,rejected)=crate::server::call_in_process(&app,"POST","/api/academic/document",Some(&json!({"content":source,"view":true})));assert_eq!(status,422,"{rejected}");assert_eq!(rejected["error_code"],crate::publication_inline_text::UNSUPPORTED);
        let(status,clear)=crate::server::call_in_process(&app,"POST","/api/publication/inline-text",Some(&json!({"action":"clear","content":source,"base_revision":read["revision"],"from":span["from"],"to":span["to"],"fields":["size"]})));assert_eq!(status,200,"{clear}");assert_eq!(clear["result"]["content"],format!("A {formula}"));assert_eq!(clear["result"]["changed"],true);
        let restored=clear["result"]["content"].as_str().unwrap();let(status,view)=crate::server::call_in_process(&app,"POST","/api/academic/document",Some(&json!({"content":restored,"view":true})));assert_eq!(status,200,"{view}");
        let inverse=crate::document_transaction::ContentTransaction{base_revision:clear["result"]["revision"].as_str().unwrap().into(),origin:"editor".into(),edits:serde_json::from_value(clear["result"]["inverse"].clone()).unwrap()};assert_eq!(crate::document_transaction::apply(restored,&inverse).unwrap().content,source);
        assert_eq!(source,original);
    }
    let source="<span data-readmd-text=\"v1\" data-size-pt=\"30\">:math[x^2]</span>";
    let source_path=dir.path().join("first-run-formula.md");std::fs::write(&source_path,source.as_bytes()).unwrap();
    let(status,read)=crate::server::call_in_process(&app,"POST","/api/publication/inline-text",Some(&json!({"action":"get","content":source})));assert_eq!(status,200,"{read}");
    let span=read["nodes"].as_array().unwrap().iter().find(|node|node["kind"]=="span").unwrap();
    let(status,clear)=crate::server::call_in_process(&app,"POST","/api/publication/inline-text",Some(&json!({"action":"clear","content":source,"base_revision":read["revision"],"from":span["from"],"to":span["to"],"fields":["size"]})));assert_eq!(status,200,"{clear}");assert_eq!(clear["ok"],true);assert_eq!(clear["result"]["content"],":math[x^2]");assert_eq!(clear["result"]["changed"],true);assert_eq!(clear["transaction"]["base_revision"],read["revision"]);
    assert_eq!(clear["result"]["revision"],crate::document_transaction::revision(":math[x^2]"));
    let inverse=crate::document_transaction::ContentTransaction{base_revision:clear["result"]["revision"].as_str().unwrap().into(),origin:"editor".into(),edits:serde_json::from_value(clear["result"]["inverse"].clone()).unwrap()};assert_eq!(crate::document_transaction::apply(":math[x^2]",&inverse).unwrap().content,source);
    let(status,reopened)=crate::server::call_in_process(&app,"POST","/api/publication/inline-text",Some(&json!({"action":"get","content":clear["result"]["content"]})));assert_eq!(status,200,"{reopened}");assert_eq!(reopened["revision"],clear["result"]["revision"]);assert!(!reopened["nodes"].as_array().unwrap().iter().any(|node|node["kind"]=="span"));
    let(status,view)=crate::server::call_in_process(&app,"POST","/api/academic/document",Some(&json!({"content":clear["result"]["content"],"view":true})));assert_eq!(status,200,"{view}");assert_eq!(view["content"],":math[x^2]");
    let(status,current)=crate::server::call_in_process(&app,"POST","/api/publication/inline-text",Some(&json!({"action":"get","content":source})));assert_eq!(status,200,"{current}");assert_eq!(current,read);
    assert_eq!(std::fs::read(&source_path).unwrap(),source.as_bytes());
}
#[test]
fn author_inline_mcp_and_desktop_share_schema_operations_and_no_write_annotation(){
    let(_dir,app)=app();let tool=crate::mcp::tools().as_array().unwrap().iter().find(|tool|tool["name"]=="readmd_publication_inline_text").unwrap().clone();
    assert_eq!(tool["annotations"]["readOnlyHint"],true);assert_eq!(tool["annotations"]["openWorldHint"],false);
    let request=json!({"action":"get","content":"A text"});
    let reply=crate::mcp::call_tool(&app,"readmd_publication_inline_text",&request);assert_ne!(reply["isError"],true,"{reply}");
    let content:Value=serde_json::from_str(reply["content"][0]["text"].as_str().unwrap()).unwrap();assert_eq!(content["revision"],crate::document_transaction::revision("A text"));
    let invalid=crate::mcp::call_tool(&app,"readmd_publication_inline_text",&json!({"action":"apply","content":"text","from":0,"to":4,"patch":{"size":"30px"}}));assert_eq!(invalid["isError"],true,"{invalid}");
}
#[test]
fn author_inline_shaping_details_are_bounded_and_unknown_objects_stay_plain(){
    let valid=json!({"consumer":"rustPdf","coordinateSpace":"textUtf8Bytes","from":0,"to":8});
    let code=crate::api_codes::AUTHOR_INLINE_TEXT_PDF_SHAPING_UNSUPPORTED;
    assert_eq!(crate::latex_compile::error_body(&format!("{code}: {valid}"))["details"],valid);
    for bad in [json!({"consumer":"rustPdf","coordinateSpace":"sourceUtf16","from":0,"to":8}),json!({"consumer":"rustPdf","coordinateSpace":"textUtf8Bytes","from":8,"to":0}),json!({"consumer":"rustPdf","coordinateSpace":"textUtf8Bytes","from":0,"to":8,"file":"private"})]{
        assert!(crate::latex_compile::error_body(&format!("{code}: {bad}"))["details"].is_string());
    }
}

fn caption_view_source(author:Value)->String{
    let layout=crate::publication_blocks::comment(&json!({"id":"caption-http","text":{"author":author}})).unwrap();
    format!("---\nlang: en-US\n---\n\n| Column |\n|---|\n| ActualCell |\n\n{layout}\n\n:label[tbl:http] **ActualCaption** العربية\n\nActualCaption following.[^n]\n\n[^n]: ActualNote\n")
}
fn descendants<'a>(node:&'a crate::math_document::XmlNode,out:&mut Vec<&'a crate::math_document::XmlNode>){out.push(node);for child in node.children(){descendants(child,out);}}
fn caption_dom<'a>(root:&'a crate::math_document::XmlNode)->&'a crate::math_document::XmlNode{
    let mut nodes=Vec::new();descendants(root,&mut nodes);nodes.into_iter().find(|node|node.local()=="p"&&node.children().any(|child|child.attributes.get("id").map(String::as_str)==Some("tbl:http"))).expect("the actual caption anchor must own its title paragraph")
}
fn assert_caption_author_runs(caption:&crate::math_document::XmlNode)->Vec<String>{
    assert_eq!(caption.text(),"Table 1 — ActualCaption العربية");
    let mut nodes=Vec::new();descendants(caption,&mut nodes);let runs=nodes.into_iter().filter(|node|node.attributes.contains_key("data-readmd-author-run")).collect::<Vec<_>>();
    // The strong title ends one Markdown text event. The next event starts
    // with a space before Arabic, so the shared itemizer correctly assigns
    // that leading neutral to the following complex-script text. Compare
    // every complete DOM run, including that original space, without trim.
    let actual=runs.iter().map(|node|(node.text(),node.attributes.get("style").map(String::as_str))).collect::<Vec<_>>();
    assert_eq!(actual,vec![("Table 1 — ".into(),Some("font-size:30pt")),("ActualCaption".into(),Some("font-size:30pt")),(" العربية".into(),Some("font-size:10pt"))]);
    runs.iter().map(|node|node.attributes["data-readmd-author-run"].clone()).collect()
}
#[test]
fn caption_only_http_view_and_html_export_consume_real_label_and_title_runs(){
    let(dir,app)=app();let source=caption_view_source(json!({"size":30,"sizeCs":10,"cs":false,"rtl":false}));assert!(!crate::publication_inline_text::has_owned_syntax(&source));let source_path=dir.path().join("caption-owned.md");std::fs::write(&source_path,source.as_bytes()).unwrap();
    let(status,body)=crate::server::call_in_process(&app,"POST","/api/academic/document",Some(&json!({"content":source,"view":true,"options":{}})));assert_eq!(status,200,"{body}");assert_eq!(body["author_text"]["revision"],crate::document_transaction::revision(&source),"author IR must bind the request source, independently of generated display bytes");let projection:crate::publication_rich::Projection=serde_json::from_value(body["projection"].clone()).unwrap();projection.verify_source(&source).unwrap();let runs=body["author_text"]["runs"].as_array().unwrap();assert!(runs.iter().any(|run|run["size"]==30.));assert!(runs.iter().any(|run|run["size"]==10.));assert!(body["author_text"].get("content").is_none());
    let html=crate::html_footnotes::render(body["content"].as_str().unwrap()).unwrap().unwrap();let dom=crate::math_document::parse_xml(&html).unwrap();let keys=assert_caption_author_runs(caption_dom(&dom));assert_eq!(runs.iter().map(|run|run["key"].as_str().unwrap()).collect::<Vec<_>>(),keys.iter().map(String::as_str).collect::<Vec<_>>());assert_eq!(runs.iter().map(|run|(run["size"].as_f64(),run["complex"].as_bool(),run["base"].is_null())).collect::<Vec<_>>(),vec![(Some(30.),Some(false),true),(Some(30.),Some(false),true),(Some(10.),Some(true),true)]);
    for run in runs{let key=run["key"].as_str().unwrap();let mut actual=Vec::new();descendants(&dom,&mut actual);assert_eq!(actual.iter().filter(|node|node.attributes.get("data-readmd-author-run").map(String::as_str)==Some(key)).count(),1);}
    let target=dir.path().join("caption-output.html");crate::mdexport::export_html(&source,dir.path().to_str().unwrap(),target.to_str().unwrap(),&json!({"frontMatter":{"mode":"none"}}),"Caption",&app.paths.assets_dir).unwrap();let exported=std::fs::read_to_string(&target).unwrap();let payload=exported.split_once("id=\"readmd-footnote-html\">").unwrap().1.split_once("</script>").unwrap().0;let actual:String=serde_json::from_str(payload).unwrap();let dom=crate::math_document::parse_xml(&actual).unwrap();assert_caption_author_runs(caption_dom(&dom));assert!(exported.contains("window.ReadMDAuthorText.apply(root,"));assert_eq!(std::fs::read(&source_path).unwrap(),source.as_bytes());
    let prepared=crate::mcp::call_tool(&app,"readmd_academic_document",&json!({"content":source,"baseDir":dir.path(),"options":{}}));assert_ne!(prepared["isError"],true,"{prepared}");let data=&prepared["structuredContent"];assert!(data.get("author_text").is_none());let projection:crate::publication_rich::Projection=serde_json::from_value(data["projection"].clone()).unwrap();assert_eq!(crate::md_ast::parse_prepared(&source,&projection).unwrap(),crate::md_ast::parse(data["content"].as_str().unwrap()));assert_eq!(std::fs::read(&source_path).unwrap(),source.as_bytes());
}

#[test]
fn caption_reader_and_html_preflight_reject_rtl_and_math_without_replacing_outputs(){
    let(dir,app)=app();let target=dir.path().join("caption-protected.html");std::fs::write(&target,b"KEEP_CAPTION").unwrap();
    for source in [caption_view_source(json!({"size":30,"rtl":true})),caption_view_source(json!({"size":30})).replace("**ActualCaption** العربية","**ActualCaption** $x$")]{
        let(status,body)=crate::server::call_in_process(&app,"POST","/api/academic/document",Some(&json!({"content":source,"view":true})));assert_eq!(status,422,"{body}");assert_eq!(body["error_code"],crate::publication_inline_text::UNSUPPORTED);
        assert_eq!(crate::mdexport::export_html(&source,"",target.to_str().unwrap(),&json!({}),"Caption",&app.paths.assets_dir).unwrap_err(),crate::publication_inline_text::UNSUPPORTED);assert_eq!(std::fs::read(&target).unwrap(),b"KEEP_CAPTION");
    }
}

#[test]
fn html_caption_author_warning_clears_only_the_fields_actually_consumed(){
    let(dir,app)=app();let source=caption_view_source(json!({"size":30,"sizeCs":10}));
    for(index,patch,expected)in[(0,json!({}),false),(1,json!({"text":{"size":17.5}}),false),(2,json!({"text":{"color":"#123456"}}),true),(3,json!({"text":{"font":"Helvetica"}}),true),(4,json!({"paragraph":{"align":"right"}}),true)].into_iter(){
        let document=crate::publication_blocks::analyze(&source).unwrap();let changed=crate::publication_blocks::apply(&source,&document.revision,"caption-http",&patch).unwrap();let current=changed["result"]["content"].as_str().unwrap();let target=dir.path().join(format!("caption-warning-{index}.html"));let result=crate::mdexport::export_html(current,"",target.to_str().unwrap(),&json!({}),"Caption",&app.paths.assets_dir).unwrap();let warnings=result.warns.unwrap_or_default();assert_eq!(warnings.iter().any(|warning|warning=="publication_local_style_unsupported"),expected,"{patch}: {warnings:?}");
    }
}

#[test]
fn caption_only_standalone_html_keeps_the_real_markdown_payload_and_nonce_runs(){
    let(dir,app)=app();let layout=crate::publication_blocks::comment(&json!({"id":"caption-standalone","text":{"author":{"size":30,"sizeCs":10}}})).unwrap();
    let source=format!("---\nlang: en-US\n---\n\n| Column |\n|---|\n| ActualCell |\n\n{layout}\n\n:label[tbl:http] **ActualCaption** العربية\n");let original=source.clone();let target=dir.path().join("standalone-caption.html");crate::mdexport::export_html(&source,"",target.to_str().unwrap(),&json!({}),"Caption",&app.paths.assets_dir).unwrap();let output=std::fs::read_to_string(target).unwrap();assert!(!output.contains("id=\"readmd-footnote-html\""));
    let payload=output.split_once("id=\"md-source\">").unwrap().1.split_once("</script>").unwrap().0;let markdown:String=serde_json::from_str(payload).unwrap();let mut html=String::new();pulldown_cmark::html::push_html(&mut html,crate::publication_inline_text::markdown_events(&markdown).unwrap().into_iter().map(|(event,_)|event));let dom=crate::math_document::parse_xml(&html).unwrap();assert_caption_author_runs(caption_dom(&dom));assert!(output.contains("window.ReadMDAuthorText.apply(root,"));assert_eq!(source,original);
}
