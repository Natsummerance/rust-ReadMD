//! Original fixtures and real packaging checks; no independent-reader certification.
use super::*;
fn marker(id:&str,page:&str,role:Option<&str>)->String{
    let mut value=json!({"id":id,"page":page});if let Some(role)=role{value["landmark"]=json!(role);}
    format!("<!-- readmd:epub v1 {value} -->")
}
fn plain()->Value{json!({"cover":{"enabled":false},"frontMatter":{"mode":"none"},"epub":{"splitLevel":"h1"}})}
fn rendered(source:&str,options:&Value)->(Request,Vec<crate::epub_writer::Chapter>){
    let request=preflight_export("epub",source,options).unwrap();let doc=crate::md_ast::parse(source);
    let chapters=crate::epub_writer::render_chapters_with_navigation(&doc,"h1",&mut |_|None,&crate::export_styles::default_style(),0,&request).unwrap();
    (request,chapters)
}
fn entries(source:&str,options:&Value)->Vec<crate::mdexport::ZipEntry>{
    crate::mdexport::epub_build_entries(&json!(source),"",options,"Synthetic navigation","","en-US","00000000-0000-4000-8000-000000000013","2026-10-16T00:00:00Z").unwrap().0
}
fn part<'a>(entries:&'a [crate::mdexport::ZipEntry],name:&str)->&'a str{
    std::str::from_utf8(&entries.iter().find(|entry|entry.path==name).unwrap().data).unwrap()
}
fn chapter(body:&str)->crate::epub_writer::Chapter{
    crate::epub_writer::Chapter{title:"Synthetic".into(),body:body.into(),headings:Vec::new(),has_math:false,math_fonts:Default::default(),math_font_warnings:false}
}
fn types(xml:&str)->Vec<String>{
    use quick_xml::{events::Event,Reader,XmlVersion};
    let mut reader=Reader::from_str(xml);let mut values=Vec::new();
    loop{match reader.read_event().unwrap(){Event::Start(e)|Event::Empty(e)=>{
        for a in e.attributes(){let a=a.unwrap();if a.key.as_ref()=="epub:type"{values.push(a.normalized_value(XmlVersion::Implicit1_0).unwrap().into_owned());}}
    },Event::Eof=>break,_=>(),}}values
}

#[test]
fn epub_navigation_only_parsed_comments_have_exact_utf16_ranges(){
    let real=marker("first","i",Some("preface"));let literal=marker("literal","99",None);
    let source=format!("\u{feff}# 中文😀\r\n\r\n{real}\r\n\r\nBODY\r\n\r\n\u{60}\u{60}\u{60}html\r\n{literal}\r\n\u{60}\u{60}\u{60}\r\n\r\n\u{60}{literal}\u{60}\r\n\r\n{d}{literal}{d}\r\n",d='$');
    let request=from_source(&source,&Value::Null).unwrap();assert_eq!(request.anchors.len(),1);
    let at=source.find(&real).unwrap();assert_eq!(request.anchors[0].from,source[..at].encode_utf16().count());
    assert_eq!(request.anchors[0].to,source[..at+real.len()].encode_utf16().count());assert_eq!(request.anchors[0].id,"first");
}

#[test]
fn epub_navigation_lf_crlf_bom_emoji_and_same_line_comments_own_exact_ranges(){
    for bom in ["","\u{feff}"]{for newline in ["\n","\r\n"]{
        let first=marker("first","i",None);let second=marker("second","2😀",None);
        for line in [format!("{first}"),format!("  {first} {second}"),format!("{first} **BODY** {second} END"),format!("START {first} {second} END")]{
            let source=format!("{bom}# 中文😀{newline}{newline}{line}{newline}{newline}BODY{newline}");
            let request=from_source(&source,&plain()).unwrap();
            let expected=if line.contains(&second){vec![&first,&second]}else{vec![&first]};assert_eq!(request.anchors.len(),expected.len());
            for (anchor,raw) in request.anchors.iter().zip(expected){let at=source.find(raw).unwrap();assert_eq!((anchor.from,anchor.to),(source[..at].encode_utf16().count(),source[..at+raw.len()].encode_utf16().count()),"{source:?}");}
        }
        let duplicate=format!("{bom}# 中文😀{newline}{newline}{first} BODY {first}{newline}");
        let error=from_source(&duplicate,&plain()).unwrap_err();let at=duplicate.rfind(&first).unwrap();
        assert_eq!(error.code,REFERENCE);assert_eq!((error.from,error.to),(duplicate[..at].encode_utf16().count(),duplicate[..at+first.len()].encode_utf16().count()));
        let invalid="<!-- readmd:epub v2 {} -->";let source=format!("{bom}# 中文😀{newline}{newline}{first} BODY {invalid}{newline}");
        let error=from_source(&source,&plain()).unwrap_err();let at=source.find(invalid).unwrap();
        assert_eq!(error.code,INVALID);assert_eq!((error.from,error.to),(source[..at].encode_utf16().count(),source[..at+invalid.len()].encode_utf16().count()));
    }}
}

#[test]
fn epub_navigation_front_matter_ranges_address_original_crlf_and_bom(){
    for bom in ["","\u{feff}"]{for newline in ["\n","\r\n"]{
        for value in ["    pageList: true","    landmarks:\n      - type: index\n        target: missing"]{
            let body=format!("title: \"中文😀\"\nepub:\n  navigation:\n{value}\n").replace('\n',newline);
            let source=format!("{bom}---{newline}{body}---{newline}{newline}# One{newline}{newline}BODY{newline}");
            // Compute the expected node range from the original bytes, not
            // pulldown's normalized MetadataBlock Text payload.
            let (_,from,to)=crate::publication_metadata::bounded_front_matter_member(&body,&["epub","navigation"]).unwrap().unwrap();
            let start=source.find(&body).unwrap();let offset=source[..start].encode_utf16().count();let error=from_source(&source,&plain()).unwrap_err();
            let (first,last)=if value.contains("missing"){("landmarks:","missing")}else{("pageList:","true")};
            assert_eq!((offset+from,offset+to),(source[..source.find(first).unwrap()].encode_utf16().count(),source[..source.find(last).unwrap()+last.len()].encode_utf16().count()));
            assert_eq!(error.code,if value.contains("missing"){REFERENCE}else{INVALID});assert_eq!((error.from,error.to),(offset+from,offset+to),"{source:?}");
        }
        let body=format!("title: \"中文😀\"\nepub:\n  navigation: [\n").replace('\n',newline);
        let source=format!("{bom}---{newline}{body}---{newline}{newline}BODY{newline}");let yaml=crate::publication_metadata::bounded_front_matter_member(&body,&["epub","navigation"]).unwrap_err();
        let error=from_source(&source,&plain()).unwrap_err();let offset=source[..source.find(&body).unwrap()].encode_utf16().count();
        assert_eq!(error.code,INVALID);assert_eq!((error.from,error.to),(offset+yaml.from,offset+yaml.to));
    }}
}

#[test]
fn epub_navigation_mixed_carrier_preserves_rich_body_across_exports(){
    let first=marker("first","1",None);let second=marker("second","2",None);let literal=marker("literal","99",None);
    let body=format!("**BOLD_SENTINEL** [LINK_SENTINEL](https://example.invalid/readmd) $x+1$ \u{60}CODE_SENTINEL\u{60} {second} END_SENTINEL");
    let source=format!("# One\n\n{first}{body}\n\n\u{60}\u{60}\u{60}html\n{literal}\n\u{60}\u{60}\u{60}\n");
    let baseline=format!("# One\n\n{body}\n\n\u{60}\u{60}\u{60}html\n{literal}\n\u{60}\u{60}\u{60}\n");
    let request=from_source(&source,&plain()).unwrap();assert_eq!(request.anchors.iter().map(|anchor|anchor.id.as_str()).collect::<Vec<_>>(),vec!["first","second"]);
    let doc=crate::md_ast::parse(&source);let control=crate::md_ast::parse(&baseline);
    let mut semantics=BTreeSet::new();crate::md_ast::walk_inlines(&doc.blocks,&mut |inline|{match inline{
        Inline::Strong(children) if crate::md_ast::inline_plain(children)=="BOLD_SENTINEL"=>{semantics.insert("bold");},
        Inline::Link{href,children,..} if href=="https://example.invalid/readmd"&&crate::md_ast::inline_plain(children)=="LINK_SENTINEL"=>{semantics.insert("link");},
        Inline::Math(latex)|Inline::OfficeMath{latex,..} if latex=="x+1"=>{semantics.insert("math");},
        Inline::Code(text) if text=="CODE_SENTINEL"=>{semantics.insert("code");},_=>(),
    }});assert_eq!(semantics,BTreeSet::from(["bold","link","math","code"]));
    assert_eq!(crate::md_ast::to_render_json(&doc.blocks),crate::md_ast::to_render_json(&control.blocks));
    assert_eq!(crate::latex_writer::render_body(&doc,&mut |_|None),crate::latex_writer::render_body(&control,&mut |_|None));
    let style=crate::export_styles::default_style();let package=crate::docx_writer::build(&doc,&style,"Synthetic",&|_|None);let control=crate::docx_writer::build(&control,&style,"Synthetic",&|_|None);
    let document=|package:crate::docx_writer::DocxPackage|package.parts.into_iter().find(|(name,_)|name=="word/document.xml").unwrap().1;
    assert_eq!(document(package),document(control));
    let entries=entries(&source,&plain());let chapter=part(&entries,"OEBPS/chapter_1.xhtml");
    for token in ["BOLD_SENTINEL","LINK_SENTINEL","CODE_SENTINEL","END_SENTINEL"]{assert_eq!(chapter.matches(token).count(),1);}
    assert!(chapter.contains("<strong>BOLD_SENTINEL</strong>"));assert!(chapter.contains("href=\"https://example.invalid/readmd\""));assert!(chapter.contains("<math"));
    let duplicate=format!("# One\n\n{first} **VISIBLE** {first} END");assert_eq!(from_source(&duplicate,&plain()).unwrap_err().code,REFERENCE);
}

#[test]
fn epub_navigation_strict_versions_fields_ids_stories_and_budget(){
    for value in ["<!-- readmd:epub v2 {} -->","<!-- readmd:epub v1 {\"id\":\"x\",\"page\":\"1\",\"pageBreakSource\":\"invented\"} -->","<!-- readmd:epub v1 {\"id\":\"bodymatter\",\"page\":\"1\"} -->"]{assert_eq!(parse_html(value).unwrap_err().code,INVALID);}
    let page=marker("restricted","1",None);
    for source in [format!("> {page}\n"),format!("- {page}\n"),format!("A[^n]\n\n[^n]: {page}\n"),format!("| A |\n|---|\n| {page} |\n")]{
        let error=from_source(&source,&Value::Null).unwrap_err();assert_eq!(error.code,INVALID,"{source}");assert!(error.to>error.from);
    }
    let duplicate=format!("{page}\n\nBODY\n\n{page}");assert_eq!(from_source(&duplicate,&Value::Null).unwrap_err().code,REFERENCE);
    let huge=format!("<!-- readmd:epub v1 {{\"id\":\"x\",\"page\":\"{}\"}} -->","x".repeat(MAX_DECLARATION_BYTES));assert_eq!(parse_html(&huge).unwrap_err().code,BUDGET);
    for value in [json!(false),json!({"pageList":true}),json!({"pageBreakSource":"unknown"}),json!({"paginationSource":{}}),json!({"landmarks":[{"type":"nonsense","target":"x"}]}),json!({"labels":{"unknown":"bad"}})]{assert_eq!(validate_options(&json!({"epub":{"navigation":value}})).unwrap_err().code,INVALID);}
    let schema=request_schema();assert_eq!(schema["additionalProperties"],false);assert!(schema["properties"].get("pageBreakSource").is_none());
}

#[test]
fn epub_navigation_bounded_yaml_and_sparse_null_inherit_explicit_false_clears(){
    let source=format!("---\nepub:\n  navigation:\n    landmarks: false\n    pageList: auto\n    paginationSource:\n      identifier: \"urn:isbn:0013\"\n    labels:\n      pageList: \"Reference pages\"\n---\n\n# One\n\n{} BODY\n",marker("first","1",None));
    let request=from_source(&source,&json!({"epub":{"navigation":{"landmarks":null,"paginationSource":null}}})).unwrap();
    assert_eq!(request.landmarks,LandmarkMode::Disabled);assert!(request.page_list);assert_eq!(request.pagination_source.unwrap().identifier.as_deref(),Some("urn:isbn:0013"));assert_eq!(request.labels["pageList"],"Reference pages");
    let cleared=from_source(&source,&json!({"epub":{"navigation":{"landmarks":[],"pageList":false,"paginationSource":false}}})).unwrap();assert_eq!(cleared.landmarks,LandmarkMode::Explicit(Vec::new()));assert!(!cleared.page_list);assert!(cleared.pagination_source.is_none());
    for front in ["epub: &a {navigation: *a}","epub: !!str value"]{assert_eq!(from_source(&format!("---\n{front}\n---\n\nBODY"),&Value::Null).unwrap_err().code,INVALID);}
    assert!(from_source("---\nepub:\n  navigation:\n    landmarks: \"false\"\n---\nBODY",&Value::Null).is_err());
}

#[test]
fn epub_navigation_preset_arrays_replace_and_null_inherits_without_mutation(){
    let base=json!({"epub":{"navigation":{"landmarks":[{"type":"preface","target":"old"}],"pageList":false,"paginationSource":{"identifier":"0013"}}}});
    let over=json!({"epub":{"navigation":{"landmarks":[{"type":"index","target":"new"}],"pageList":null}}});let original=(base.clone(),over.clone());
    let merged=crate::publication_metadata::merge_options(&base,&over).unwrap();assert_eq!(merged["epub"]["navigation"]["landmarks"],over["epub"]["navigation"]["landmarks"]);
    assert_eq!(merged["epub"]["navigation"]["pageList"],false);assert_eq!(merged["epub"]["navigation"]["paginationSource"]["identifier"],"0013");assert_eq!((base,over),original);
}

#[test]
fn epub_navigation_cross_chapter_order_inline_targets_and_final_marker_survive(){
    let source=format!("{}\n\n# Preface\n\nFRONT\n\n{}\n\n# Main\n\nMAIN {} MID\n\n# End\n\n[Return](#readmd-epub-bodypage)\n\n{}\n",marker("front","i",Some("preface")),marker("main","1",Some("bodymatter")),marker("bodypage","2",None),marker("last","3",Some("index")));
    let(request,chapters)=rendered(&source,&plain());assert_eq!(chapters.len(),3);let resolved=resolve(&request,&chapters,false).unwrap();
    assert_eq!(resolved.pages.iter().map(|link|(link.label.as_str(),link.href.as_str())).collect::<Vec<_>>(),vec![("i","chapter_1.xhtml#h-readmd-epub-front"),("1","chapter_2.xhtml#h-readmd-epub-main"),("2","chapter_2.xhtml#h-readmd-epub-bodypage"),("3","chapter_3.xhtml#h-readmd-epub-last")]);
    assert!(chapters[2].body.contains("href=\"chapter_2.xhtml#h-readmd-epub-bodypage\""));
    assert_eq!(chapters.iter().map(|c|c.body.matches("data-readmd-epub-anchor=\"bodymatter\"").count()).sum::<usize>(),1);
    assert!(chapters[0].body.contains("role=\"doc-pagebreak\" aria-label=\"i\""));assert!(!chapters.iter().any(|c|c.body.contains("readmd:epub v1")));
}

#[test]
fn epub_navigation_final_xml_binding_rejects_missing_duplicate_and_heading_masquerade(){
    let source=format!("# One\n\n{}\n\nBODY",marker("first","1",None));let(request,chapters)=rendered(&source,&plain());assert!(resolve(&request,&chapters,false).is_ok());
    for body in ["<h1 id=\"h-readmd-epub-first\">Same-name heading</h1>","<span id=\"h-readmd-epub-first\" data-readmd-epub-anchor=\"bodymatter\"></span>","<p>h-readmd-epub-first</p>"]{assert_eq!(resolve(&request,&[chapter(body)],false).unwrap_err().code,REFERENCE);}
    assert_eq!(resolve(&request,&[chapter(&(chapters[0].body.clone()+&chapters[0].body))],false).unwrap_err().code,REFERENCE);
    assert_eq!(preflight_export("epub",&marker("alone","1",None),&plain()).unwrap_err().code,REFERENCE);
}

#[test]
fn epub_navigation_explicit_landmarks_content_locale_and_virtual_targets_are_honest(){
    let source=format!("# Preface\n\n{}\n\nBODY",marker("target","i",Some("preface")));
    let options=json!({"epub":{"navigation":{"landmarks":[{"type":"preface","target":"target","label":"Read & start"},{"type":"toc","target":"@toc"}]}}});
    let(mut request,chapters)=rendered(&source,&options);let resolved=resolve(&request,&chapters,false).unwrap();assert_eq!(resolved.landmarks.len(),2);assert_eq!(resolved.landmarks[0].label,"Read & start");assert_eq!(resolved.landmarks[1].href,"nav.xhtml#toc");
    for target in ["missing","@cover"]{let options=json!({"epub":{"navigation":{"landmarks":[{"type":"cover","target":target}]}}});assert_eq!(preflight_export("epub",&source,&options).unwrap_err().code,REFERENCE);}
    request.labels.insert("pageList".into(),"Author's pages".into());assert_eq!(localize(&mut request,"zh-Hant-HK"),None);assert_eq!(request.labels["pageList"],"Author's pages");assert_eq!(request.labels["landmarks"],"導覽地標");
    let mut unknown=Request::default();assert_eq!(localize(&mut unknown,"zu-ZA"),Some("publication_metadata_label_fallback"));assert_eq!(unknown.labels["landmarks"],"Landmarks");
}

#[test]
fn epub_navigation_real_package_keeps_toc_ncx_and_complete_body(){
    let source=format!("{}\n\n# One\n\nALPHA {} OMEGA\n\n# Two\n\nSECOND\n",marker("first","i",Some("preface")),marker("mid","1",None));
    let entries=entries(&source,&plain());let nav=part(&entries,"OEBPS/nav.xhtml");let kinds=types(nav);
    assert_eq!(kinds.iter().filter(|kind|kind.as_str()=="toc").count(),2);assert_eq!(kinds.iter().filter(|kind|kind.as_str()=="landmarks").count(),1);assert_eq!(kinds.iter().filter(|kind|kind.as_str()=="page-list").count(),1);
    assert!(nav.contains("chapter_1.xhtml#h-readmd-epub-mid\">1</a>"));assert!(part(&entries,"OEBPS/toc.ncx").contains("chapter_2.xhtml#h-two"));
    let body=entries.iter().filter(|e|e.path.starts_with("OEBPS/chapter_")).map(|e|String::from_utf8_lossy(&e.data)).collect::<String>();
    for token in ["ALPHA","OMEGA","SECOND"]{assert_eq!(body.matches(token).count(),1);}assert!(!body.contains("readmd:epub v1"));
}

#[test]
fn epub_navigation_final_p01_opf_merge_keeps_people_ids_rights_and_source_of(){
    let source=format!("---\ntitle: Shared navigation book\nauthors:\n  - id: jane\n    name: \"Doe, Jane\"\n    affiliationIds: [north]\naffiliations:\n  - id: north\n    name: \"North, Synthetic\"\nrights: Original rights\nidentifiers:\n  - scheme: isbn\n    value: \"978-0-00-000000-2\"\n    primary: true\nlanguage: en-US\nepub:\n  navigation:\n    paginationSource:\n      identifier: \"urn:isbn:978-0-00-000000-2\"\n      description: \"Synthetic printed edition, 2026\"\n---\n\n# One\n\n{} BODY\n",marker("first","1",None));
    let entries=entries(&source,&plain());let opf=part(&entries,"OEBPS/content.opf");
    for token in ["Shared navigation book","Doe, Jane","North, Synthetic","Original rights","978-0-00-000000-2","readmd:metadata","urn:isbn:978-0-00-000000-2","Synthetic printed edition, 2026"]{assert!(opf.contains(token),"Missing P01/P13 value: {token}");}
    assert_eq!(opf.matches("id=\"readmd-pagination-source\"").count(),1);assert!(opf.contains("refines=\"#readmd-pagination-source\" property=\"source-of\">pagination</meta>"));assert!(!opf.contains("pageBreakSource"));
}

#[test]
fn epub_navigation_disabled_nav_keeps_semantic_page_anchors_and_omits_unused_source(){
    let source=format!("# One\n\n{} BODY",marker("first","1",Some("preface")));
    let options=json!({"frontMatter":{"mode":"none"},"epub":{"navigation":{"landmarks":false,"pageList":false,"paginationSource":{"identifier":"0013"}}}});
    let entries=entries(&source,&options);assert!(!types(part(&entries,"OEBPS/nav.xhtml")).iter().any(|kind|matches!(kind.as_str(),"page-list"|"landmarks")));
    assert!(!part(&entries,"OEBPS/content.opf").contains("readmd-pagination-source"));assert!(part(&entries,"OEBPS/chapter_1.xhtml").contains("role=\"doc-pagebreak\""));
}

#[test]
fn epub_navigation_non_epub_projection_does_not_copy_metadata_or_lose_body(){
    let source=format!("# One\n\nBEFORE {} AFTER\n\n{}\n\nEND",marker("inline","1",None),marker("block","2",Some("index")));let original=source.clone();
    let doc=crate::md_ast::parse(&source);assert!(preflight_export("docx",&source,&plain()).is_ok());assert!(!crate::md_ast::to_render_json(&doc.blocks).to_string().contains("readmd:epub"));
    let tex=crate::latex_writer::render_body(&doc,&mut |_|None);assert!(!tex.contains("readmd:epub"));for token in ["BEFORE","AFTER","END"]{assert_eq!(tex.matches(token).count(),1);}assert_eq!(source,original);
}

#[test]
fn epub_navigation_actual_export_rejects_before_staging_and_reopen_preserves_bom_crlf(){
    let dir=tempfile::tempdir().unwrap();let target=dir.path().join("Protected.epub");let input=dir.path().join("Book.md");
    let source=format!("\u{feff}---\r\nunknown: \"a,b: preserved\" # exact trivia\r\n---\r\n\r\n# 中文😀\r\n\r\n{} BODY\r\n",marker("first","0042",None));
    std::fs::write(&input,&source).unwrap();std::fs::write(&target,b"KEEP ORIGINAL EPUB").unwrap();
    let options=json!({"epub":{"navigation":{"landmarks":[{"type":"index","target":"missing"}]}}});let before=options.clone();
    let error=crate::mdexport::export_epub(&source,dir.path().to_str().unwrap(),target.to_str().unwrap(),&options,"Synthetic").unwrap_err();
    assert_eq!(error,REFERENCE);assert_eq!(std::fs::read(&target).unwrap(),b"KEEP ORIGINAL EPUB");assert_eq!(std::fs::read(&input).unwrap(),source.as_bytes());assert_eq!(options,before);assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),2);
    let first=from_source(&std::fs::read_to_string(&input).unwrap(),&plain()).unwrap();let _actual_epub=entries(&source,&plain());
    assert_eq!(first,from_source(&std::fs::read_to_string(&input).unwrap(),&plain()).unwrap());assert_eq!(std::fs::read(input).unwrap(),source.as_bytes());assert_eq!(first.anchors[0].page.as_deref(),Some("0042"));
}

fn new_app(root:&std::path::Path)->std::sync::Arc<crate::App>{
    let assets=root.join("assets");std::fs::create_dir_all(&assets).unwrap();
    std::sync::Arc::new(crate::App::bootstrap(crate::paths::AppPaths::with_dirs(&root.join("profile"),root,&assets)).unwrap())
}

#[test]
fn epub_navigation_http_preview_legacy_and_mcp_keep_target_and_source_ranges(){
    let dir=tempfile::tempdir().unwrap();let app=new_app(dir.path());let target=dir.path().join("Protected.epub");
    std::fs::write(&target,b"KEEP EPUB").unwrap();
    let page=marker("same","1",None);let source=format!("\u{feff}# 中文😀\r\n\r\n{page}\r\n\r\nBODY\r\n\r\n{page}\r\n");
    let expected=preflight_export("epub",&source,&plain()).unwrap_err();
    for route in ["/api/export","/api/export/preview","/api/export/epub"]{
        let (status,body)=crate::server::call_in_process(&app,"POST",route,Some(&json!({"format":"epub","content":source,"options":plain(),"out_path":target,"suggestedName":"Synthetic","confirm":true})));
        assert_eq!(status,200,"{body}");assert_eq!(body["error_code"],REFERENCE,"{body}");
        assert_eq!(body["from"],expected.from);assert_eq!(body["to"],expected.to);assert_eq!(body["units"],"UTF-16");
        assert!(body.get("pdf").is_none());assert!(body.get("key").is_none());assert_eq!(std::fs::read(&target).unwrap(),b"KEEP EPUB");
    }
    let result=crate::mcp::call_tool(&app,"readmd_export_epub",&json!({"markdown_content":source,"output_path":target,"options":plain(),"base_dir":dir.path(),"confirm":true,"overwrite":true}));
    assert_eq!(result["isError"],true,"{result}");assert_eq!(result["structuredContent"]["error_code"],REFERENCE);
    assert_eq!(result["structuredContent"]["from"],expected.from);assert_eq!(result["structuredContent"]["to"],expected.to);assert_eq!(std::fs::read(target).unwrap(),b"KEEP EPUB");
}

#[test]
fn epub_navigation_presets_survive_app_reopen_and_invalid_schema_does_not_replace_them(){
    let dir=tempfile::tempdir().unwrap();let app=new_app(dir.path());
    let options=json!({"epub":{"navigation":{"landmarks":[{"type":"index","target":"deferred-source-id"}],"pageList":false,"paginationSource":{"identifier":"0013"}}}});
    let (status,body)=crate::server::call_in_process(&app,"POST","/api/export/presets",Some(&json!({"custom":{"Navigation":options}})));assert_eq!(status,200,"{body}");assert_eq!(body["ok"],true);
    let preset=app.paths.data_dir.join("export_presets.json");let original=std::fs::read(&preset).unwrap();drop(app);
    let reopened=new_app(dir.path());let(status,body)=crate::server::call_in_process(&reopened,"GET","/api/export/presets",None);
    assert_eq!(status,200);assert_eq!(body["custom"]["Navigation"]["epub"]["navigation"],options["epub"]["navigation"]);
    let(status,body)=crate::server::call_in_process(&reopened,"POST","/api/export/presets",Some(&json!({"custom":{"Navigation":{"epub":{"navigation":{"pageBreakSource":"invented"}}}}})));
    assert_eq!(status,422,"{body}");assert_eq!(body["error_code"],INVALID);assert_eq!(std::fs::read(preset).unwrap(),original);
}

#[test]
fn epub_navigation_p01_title_transaction_preserves_unknown_yaml_and_marker_bytes(){
    let source=format!("\u{feff}---\r\ntitle: Old\r\nepub:\r\n  navigation:\r\n    pageList: false # preserved exactly\r\nunknown: {{value: \"a,b: unchanged\"}}\r\n---\r\n\r\n# One\r\n\r\n{} BODY\r\n",marker("first","0042",None));
    let before=from_source(&source,&Value::Null).unwrap();
    let transaction=crate::publication_metadata::plan_patch(&source,&crate::document_transaction::revision(&source),&json!({"title":"New"})).unwrap();
    // P01 serializes changed string scalars as JSON-quoted YAML. The title
    // edit owns only the old value; all other source bytes must be untouched.
    let title=source.find("Old").unwrap();let from=source[..title].encode_utf16().count();
    assert_eq!(transaction.edits,vec![crate::document_transaction::TextEdit{from,to:from+3,insert:"\"New\"".into()}]);
    let changed=crate::document_transaction::apply(&source,&transaction).unwrap();
    assert_eq!(changed.content,source.replacen("title: Old","title: \"New\"",1));
    assert_eq!(crate::publication_metadata::from_source(&changed.content,&Value::Null,"Synthetic.md").unwrap().title,"New");
    let after=from_source(&changed.content,&Value::Null).unwrap();let mut relocated=before.clone();
    // The two inserted quote characters shift each later source-owned marker.
    for anchor in &mut relocated.anchors{anchor.from+=2;anchor.to+=2;}assert_eq!(after,relocated);
    let raw=marker("first","0042",None);let at=changed.content.find(&raw).unwrap();
    assert_eq!((after.anchors[0].from,after.anchors[0].to),(changed.content[..at].encode_utf16().count(),changed.content[..at+raw.len()].encode_utf16().count()));
    let inverse=crate::document_transaction::ContentTransaction{base_revision:changed.revision,origin:"editor".into(),edits:changed.inverse};
    assert_eq!(crate::document_transaction::apply(&changed.content,&inverse).unwrap().content,source);
}

#[test]
fn epub_navigation_fixed_original_book_fixture_matches_actual_nav_xml(){
    use crate::math_document::XmlNode;
    fn find<'a>(node:&'a XmlNode,kind:&str)->Option<&'a XmlNode>{
        if node.local()=="nav"&&node.attributes.get("epub:type").is_some_and(|value|value==kind){return Some(node);}
        node.children().find_map(|child|find(child,kind))
    }
    fn links(node:&XmlNode)->Vec<Value>{
        if node.local()=="a"{
            let mut value=json!({"href":node.attributes["href"],"label":node.text()});
            if let Some(kind)=node.attributes.get("epub:type"){value["type"]=json!(kind);}
            return vec![value];
        }
        node.children().flat_map(links).collect()
    }
    let source=include_str!("fixtures/publication-navigation-book.md");
    let expected:Value=serde_json::from_str(include_str!("fixtures/publication-navigation-book.expected.json")).unwrap();
    let entries=entries(source,&expected["options"]);let nav=part(&entries,"OEBPS/nav.xhtml");
    assert_eq!(nav.matches("<!DOCTYPE html>").count(),1);
    let node=crate::math_document::parse_xml(&nav.replacen("<!DOCTYPE html>","",1)).unwrap();
    assert_eq!(links(find(&node,"page-list").unwrap()),*expected["pages"].as_array().unwrap());
    assert_eq!(links(find(&node,"landmarks").unwrap()),*expected["landmarks"].as_array().unwrap());
    let chapters=entries.iter().filter(|entry|entry.path.starts_with("OEBPS/chapter_")).collect::<Vec<_>>();
    assert_eq!(chapters.len() as u64,expected["chapters"].as_u64().unwrap());
    let body=chapters.iter().map(|entry|String::from_utf8_lossy(&entry.data)).collect::<String>();
    for token in expected["bodyTokens"].as_array().unwrap(){assert_eq!(body.matches(token.as_str().unwrap()).count(),1);}
    assert!(part(&entries,"OEBPS/content.opf").contains(expected["sourceOfPagination"].as_str().unwrap()));
    assert!(body.contains("href=\"chapter_1.xhtml#h-readmd-epub-front\""));
}

#[test]
fn epub_navigation_landmark_type_cannot_repeat_in_one_resource_but_can_across_chapters(){
    let first=marker("first","i",Some("preface"));let second=marker("second","ii",Some("preface"));
    let same=format!("# One\n\n{first} A\n\n{second} B");
    let(request,chapters)=rendered(&same,&plain());assert_eq!(resolve(&request,&chapters,false).unwrap_err().code,REFERENCE);
    let distinct=format!("# One\n\n{first} A\n\n# Two\n\n{second} B");
    let(request,chapters)=rendered(&distinct,&plain());assert!(resolve(&request,&chapters,false).is_ok());
}
