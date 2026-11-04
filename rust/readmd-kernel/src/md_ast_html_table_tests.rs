//! Independent cell/OOXML/entry-point safety expectations. Root runs these
//! when the shared Rust compilation slot is available; no native Office claim.
use super::*;
use serde_json::json;
const SIMPLE: &str = include_str!("../../../tests/fixtures/table-safety/simple.html.md");
const MERGED: &str = include_str!("../../../tests/fixtures/table-safety/merged.html.md");
const GFM: &str = include_str!("../../../tests/fixtures/table-safety/gfm.md");
const LITERAL: &str = include_str!("../../../tests/fixtures/table-safety/literal.md");
const UNSUPPORTED: &str = crate::api_codes::PUBLICATION_TABLE_UNSUPPORTED;
fn tables(blocks: &[Block]) -> Vec<&Block> { blocks.iter().filter(|block| matches!(block, Block::Table { .. })).collect() }
fn document_xml(document: &Document) -> String {
    let package = crate::docx_writer::build(document, &json!({"frontMatter":{"mode":"none"}}), "Owned table", &|_|None);
    assert!(!package.parts.is_empty(), "{:?}", package.warns);
    String::from_utf8(package.parts.into_iter().find(|(name,_)|name=="word/document.xml").unwrap().1).unwrap()
}
fn elements<'a>(root: &'a crate::math_document::XmlNode, name: &str) -> Vec<&'a crate::math_document::XmlNode> {
    let mut out=Vec::new(); if root.name==name { out.push(root); }
    for child in root.children() { out.extend(elements(child,name)); } out
}

#[test]
fn native_html_table_projects_exact_cells_entities_rich_inlines_and_breaks_without_parsing_literal_markdown() {
    let source = parse(SIMPLE); let before = source.clone(); let projected = native_table_document(&source).unwrap();
    assert_eq!(source,before); assert!(matches!(projected,std::borrow::Cow::Owned(_)));
    let actual=tables(&projected.blocks);assert_eq!(actual.len(),1);
    let Block::Table { aligns,header,rows }=actual[0] else{panic!("native table")};
    assert_eq!(aligns,&[Align::Left,Align::Right]);assert_eq!(header.iter().map(|cell|inline_plain(cell)).collect::<Vec<_>>(),["SIMPLE_HEAD_ALPHA","SIMPLE_HEAD_BETA"]);
    assert_eq!(rows.len(),2);assert_eq!(rows[0].len(),2);assert_eq!(rows[1].len(),2);
    assert_eq!(inline_plain(&rows[0][0]),"SIMPLE_CELL_ALPHA & literal *star* EM_ALPHA BR_ALPHA <x> LINK_ALPHA");
    assert_eq!(inline_plain(&rows[0][1]),"17.50");assert_eq!(inline_plain(&rows[1][0]),"SIMPLE_CELL_BETA");assert!(rows[1][1].is_empty());
    assert!(rows[0][0].iter().any(|inline|matches!(inline,Inline::Strong(children) if inline_plain(children)=="SIMPLE_CELL_ALPHA")));
    assert!(rows[0][0].iter().any(|inline|matches!(inline,Inline::Emph(children) if inline_plain(children)=="EM_ALPHA")));
    assert!(rows[0][0].iter().any(|inline|matches!(inline,Inline::Code(value) if value=="<x>")));
    assert_eq!(rows[0][0].iter().filter(|inline|matches!(inline,Inline::HardBreak)).count(),1);
    assert!(rows[0][0].iter().any(|inline|matches!(inline,Inline::Link{href,children,..} if href=="https://example.invalid/table"&&inline_plain(children)=="LINK_ALPHA")));
}

#[test]
fn native_html_table_reuses_real_docx_table_cells_formats_alignment_and_preserves_two_equal_tables() {
    let xml=document_xml(&parse(SIMPLE)); let root=crate::math_document::parse_xml(&xml).unwrap();
    let actual=elements(&root,"w:tbl");assert_eq!(actual.len(),1);assert_eq!(elements(actual[0],"w:tr").len(),3);
    let cells=elements(actual[0],"w:tc");assert_eq!(cells.len(),6);
    assert_eq!(cells.iter().map(|cell|cell.text()).collect::<Vec<_>>(),[
        "SIMPLE_HEAD_ALPHA","SIMPLE_HEAD_BETA","SIMPLE_CELL_ALPHA & literal *star* EM_ALPHABR_ALPHA <x> LINK_ALPHA","17.50","SIMPLE_CELL_BETA",""]);
    for index in [1,3,5] { assert_eq!(elements(cells[index],"w:jc")[0].attributes["w:val"],"right"); }
    assert!(!elements(cells[2],"w:b").is_empty());assert!(!elements(cells[2],"w:i").is_empty());assert_eq!(elements(cells[2],"w:br").len(),1);assert_eq!(elements(cells[2],"w:hyperlink").len(),1);
    let duplicate=document_xml(&parse(&format!("{SIMPLE}\n\n{SIMPLE}")));let root=crate::math_document::parse_xml(&duplicate).unwrap();assert_eq!(elements(&root,"w:tbl").len(),2);assert_eq!(elements(&root,"w:tc").len(),12);
}

#[test]
fn native_html_table_rust_pdf_uses_existing_native_table_json_and_retains_all_body_markers() {
    let source=parse(SIMPLE);let projected=native_table_document(&source).unwrap();let blocks=to_render_json(&projected.blocks);
    let table=blocks.as_array().unwrap().iter().find(|block|block["type"]=="table").unwrap();
    assert_eq!(table["header"].as_array().unwrap().len(),2);assert_eq!(table["rows"].as_array().unwrap().len(),2);assert_eq!(table["aligns"],json!(["left","right"]));
    let style=crate::export_styles::sanitize(&json!({"frontMatter":{"mode":"none"},"typography":{"font":"Arial","size":11}}));
    let mut warnings=Vec::new();let rendered=crate::pdf_render::build(&blocks,&style,"",&|_|None,&mut warnings).unwrap();
    let text=rendered.pages.iter().map(|page|page.text.as_str()).collect::<String>();
    for marker in ["SIMPLE_HEAD_ALPHA","SIMPLE_HEAD_BETA","SIMPLE_CELL_ALPHA","SIMPLE_CELL_BETA","EM_ALPHA","BR_ALPHA","LINK_ALPHA"] { assert!(text.contains(marker),"{marker}: {text}"); }
    // Read the actual PDF bytes through the independently implemented reader,
    // not just the writer's own layout projection. This runs on every CI OS
    // without adding Python or another host tool to the application contract.
    let directory=tempfile::tempdir().unwrap();let target=directory.path().join("ordinary-html-table.pdf");
    crate::mdexport::export_pdf(SIMPLE,directory.path().to_str().unwrap(),target.to_str().unwrap(),&style,"Owned table",&directory.path().join("assets")).unwrap();
    let decoded=pdf_extract::extract_text(&target).unwrap();let normalized=decoded.split_whitespace().collect::<String>();
    for marker in ["SIMPLE_HEAD_ALPHA","SIMPLE_HEAD_BETA","SIMPLE_CELL_ALPHA","SIMPLE_CELL_BETA","EM_ALPHA","BR_ALPHA","LINK_ALPHA"] {
        assert_eq!(normalized.matches(marker).count(),1,"physical PDF cell lost or duplicated: {marker}");
    }
}

#[test]
fn native_html_table_real_dom_distinguishes_unit_spans_entities_code_comments_and_non_table_html() {
    let unit=SIMPLE.replace("<td align=\"left\">SIMPLE_CELL_BETA","<td align=\"left\" colspan=\"1\" rowspan=\"1\">SIMPLE_CELL_BETA");assert!(native_table_document(&parse(&unit)).is_ok());
    let unicode="<table><tr><th>H &amp; &#x1F600;</th></tr><tr><td>&#20013;&#25991;&nbsp;&lt;tag&gt; *literal*</td></tr></table>";
    let doc=parse(unicode);let projected=native_table_document(&doc).unwrap();let Block::Table{header,rows,..}=&projected.blocks[0]else{panic!("table")};
    assert_eq!(inline_plain(&header[0]),"H & 😀");assert_eq!(inline_plain(&rows[0][0]),"中文\u{a0}<tag> *literal*");
    for source in [GFM,LITERAL,"<div title=\"<table>\">Ordinary HTML</div>","<!-- <table><tr><td>fake</td></tr></table> -->"] {
        let doc=parse(source);let before=doc.clone();let projected=native_table_document(&doc).unwrap();assert!(matches!(projected,std::borrow::Cow::Borrowed(_)));assert_eq!(doc,before);
    }
    for format in ["html","epub","tex","pptx"] { assert!(crate::publication_profile::preflight(format,MERGED,&json!({})).is_ok(),"{format}"); }
}

#[test]
fn native_html_table_unsupported_structure_and_critical_attributes_never_flatten() {
    let invalid=[MERGED.to_owned(),
        SIMPLE.replace("SIMPLE_CELL_BETA","<table><tr><th>NESTED</th></tr></table>"),
        SIMPLE.replace("<td align=\"right\"></td>",""),
        SIMPLE.replace("<td align=\"left\">SIMPLE_CELL_BETA","<td align=\"left\" style=\"color:red\">SIMPLE_CELL_BETA"),
        SIMPLE.replace("<th align=\"left\">","<th align=\"left\" rowspan=\"0\">"),
        SIMPLE.replace("<th align=\"left\">","<th align=\"left\" scope=\"row\">"),
        SIMPLE.replace("<td align=\"right\"></td>","<td align=\"left\"></td>"),
        "<table><tr><td>Headerless</td></tr></table>".into(),
        "<table><thead><tr><th>A</th></tr><tr><th>B</th></tr></thead></table>".into(),
        "<table></table>".into(),
        SIMPLE.replace("SIMPLE_CELL_BETA","\0"),
        "Inline before <table><tr><th>H</th></tr></table>".into(),
    ];
    for source in invalid {
        let doc=parse(&source);let before=doc.clone();assert_eq!(native_table_document(&doc).unwrap_err(),UNSUPPORTED,"{source}");assert_eq!(doc,before);
        for format in ["docx","pdf"] { assert_eq!(crate::publication_profile::preflight(format,&source,&json!({"pdf":{"engine":"rust"}})).unwrap_err().code,UNSUPPORTED); }
        let calls=std::cell::Cell::new(0);let package=crate::docx_writer::build(&doc,&json!({}),"Protected",&|_|{calls.set(calls.get()+1);None});assert!(package.parts.is_empty());assert_eq!(package.warns,[UNSUPPORTED]);assert_eq!(calls.get(),0);
    }
}

#[test]
fn native_html_table_public_export_refuses_before_fonts_resources_target_replacement_or_new_artifact() {
    let root=tempfile::tempdir().unwrap();let data=root.path().join("profile-must-not-exist");let assets=root.path().join("assets-must-not-exist");
    for format in ["docx","pdf"] {
        for exists in [false,true] {
            let target=root.path().join(format!("protected-{format}-{exists}.{format}"));if exists { std::fs::write(&target,b"OWNED ORIGINAL").unwrap(); }
            let error=crate::mdexport::export_document_with_data(format,MERGED,root.path().to_str().unwrap(),target.to_str().unwrap(),&json!({"pdf":{"engine":"rust"}}),"Owned",&assets,&data).unwrap_err();assert_eq!(error,UNSUPPORTED);
            if exists { assert_eq!(std::fs::read(&target).unwrap(),b"OWNED ORIGINAL"); } else { assert!(!target.exists()); }
        }
    }
    assert!(!data.exists()&&!assets.exists());
    let target=root.path().join("direct.pdf");std::fs::write(&target,b"OWNED DIRECT PDF").unwrap();
    let error=crate::mdexport::export_pdf(MERGED,"",target.to_str().unwrap(),&json!({"pdf":{"engine":"typst"}}),"Owned",&assets).unwrap_err();assert_eq!(error,UNSUPPORTED);assert_eq!(std::fs::read(target).unwrap(),b"OWNED DIRECT PDF");
    assert!(!std::fs::read_dir(root.path()).unwrap().any(|entry|entry.unwrap().file_name().to_string_lossy().contains("readmd-part")));
}

#[test]
fn native_html_table_html5_named_entities_preserve_characters_without_recursive_or_structural_decoding() {
    let source="<table><tr><th>H</th></tr><tr><td>&alpha; &NotEqualTilde; &amp;alpha; &lt;table&gt; <a href=\"https://example.invalid/?q=&quot;quoted&quot;&amp;x=1\" title=\"&alpha; &quot;title&quot;\">LINK</a></td></tr></table>";
    let doc=parse(source);let projected=native_table_document(&doc).unwrap();
    let Block::Table{rows,..}=&projected.blocks[0]else{panic!("table")};
    assert_eq!(inline_plain(&rows[0][0]),"α ≂\u{338} &alpha; <table> LINK");
    assert!(rows[0][0].iter().any(|value|matches!(value,Inline::Link{href,title,..} if href=="https://example.invalid/?q=\"quoted\"&x=1"&&title=="α \"title\"")));
    let xml=document_xml(&doc);let root=crate::math_document::parse_xml(&xml).unwrap();
    let cells=elements(&root,"w:tc");assert_eq!(cells.len(),2);
    assert_eq!(cells[1].text(),"α ≂\u{338} &alpha; <table> LINK");
    assert_eq!(elements(&root,"w:tbl").len(),1);assert_eq!(elements(&root,"w:hyperlink").len(),1);
}

#[test]
fn native_html_table_cancelled_oversized_input_stops_before_dom_construction() {
    let doc=Document{front_matter:None,blocks:vec![Block::Html(format!("<table><tr><th>{}</th></tr></table>","X".repeat(2*1024*1024)))]};
    let result=crate::speech::with_cancel(std::sync::Arc::new(||true),||native_table_document(&doc));
    assert_eq!(result.unwrap_err(),"cancelled");
}

#[test]
fn native_html_table_cancelled_wide_unknown_attributes_stop_before_quadratic_dom_deduplication() {
    let attributes=(0..12000).map(|index|format!(" a{index}=\"\"")).collect::<String>();
    let doc=Document{front_matter:None,blocks:vec![Block::Html(format!("<table{attributes}><tr><th>H</th></tr><tr><td>BODY</td></tr></table>"))]};
    let result=crate::speech::with_cancel(std::sync::Arc::new(||true),||native_table_document(&doc));
    assert_eq!(result.unwrap_err(),"cancelled");
}

#[test]
fn native_html_table_invalid_pdf_engine_refuses_before_snapshot_resource_freeze() {
    let directory=tempfile::tempdir().unwrap();let assets=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets").canonicalize().unwrap();
    let runtime=std::sync::Arc::new(crate::App::bootstrap(crate::paths::AppPaths::with_dirs(&directory.path().join("profile"),directory.path(),&assets)).unwrap());
    let source=directory.path().join("Owned.md");
    std::fs::write(&source,"<table><tr><th>H</th></tr><tr><td colspan=\"2\">BODY</td></tr></table>\n\n![Owned](owned.svg)\n").unwrap();
    std::fs::write(directory.path().join("owned.svg"),"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\"><rect width=\"1\" height=\"1\"/></svg>").unwrap();
    let route=format!("/api/file?p={}",percent_encoding::utf8_percent_encode(&source.to_string_lossy(),percent_encoding::NON_ALPHANUMERIC));
    assert_eq!(crate::server::call_in_process(&runtime,"GET",&route,None).0,200);
    let (status,opened)=crate::server::call_in_process(&runtime,"POST","/api/review",Some(&json!({"action":"open","path":source})));assert_eq!(status,200,"{opened}");
    let (status,result)=crate::server::call_in_process(&runtime,"POST","/api/export/snapshot",Some(&json!({"action":"prepare","document_id":opened["document_id"],"generation":opened["generation"],"review_revision":opened["review_revision"],"call_id":uuid::Uuid::new_v4().to_string(),"format":"pdf","options":{"pdf":{"engine":"bogus"}},"suggestedName":"Owned.md"})));
    assert_eq!((status,result["error_code"].as_str()),(422,Some("latex_invalid_engine")),"{result}");
    assert!(!runtime.paths.data_dir.join("export-snapshots").exists());
}

#[test]
fn native_html_table_dom_budgets_refuse_before_native_package_or_image_callbacks() {
    let attrs=" align=\"left\"".repeat(17);
    let nested=format!("{}BODY{}","<span>".repeat(65),"</span>".repeat(65));
    let nodes="<span>X</span>".repeat(50001);
    let long_token=format!("<a title=\"{}\" href=\"https://example.invalid/\">BODY</a>","X".repeat(4096));
    let cases=[
        format!("<table><tr><th{attrs}>H</th></tr><tr><td align=\"left\">BODY</td></tr></table>"),
        format!("<table><tr><th>H</th></tr><tr><td>{nested}</td></tr></table>"),
        format!("<table><tr><th>H</th></tr><tr><td>{nodes}</td></tr></table>"),
        format!("<table><tr><th>H</th></tr><tr><td>{long_token}</td></tr></table>"),
        format!("<table><tr><th>H</th></tr><tr><td>{}</td></tr></table>","X".repeat(2*1024*1024)),
        "<table><tr><th title=\"unterminated".into(),
    ];
    for html in cases {
        let doc=Document{front_matter:None,blocks:vec![Block::Html(html)]};
        assert_eq!(native_table_document(&doc).unwrap_err(),UNSUPPORTED);
        let calls=std::cell::Cell::new(0);let package=crate::docx_writer::build(&doc,&json!({}),"Owned",&|_|{calls.set(calls.get()+1);None});
        assert!(package.parts.is_empty());assert_eq!(package.warns,[UNSUPPORTED]);assert_eq!(calls.get(),0);
    }
}

#[test]
fn native_html_table_non_table_html_and_gfm_are_not_rejected_by_native_budgets_or_entity_rules() {
    let long=format!("<div title=\"<table> &unknown; {}\">Ordinary HTML</div>","X".repeat(2*1024*1024));
    for html in [long,"<!-- <table> &unknown; -->".into(),"<script>const example='<table> &unknown;';</script>".into(),"<textarea><table>&unknown;</textarea>".into()] {
        let doc=Document{front_matter:None,blocks:vec![Block::Html(html)]};let before=doc.clone();
        assert!(matches!(native_table_document(&doc).unwrap(),std::borrow::Cow::Borrowed(_)));assert_eq!(doc,before);
    }
    for source in [GFM,LITERAL,"| H |\n| --- |\n| &alpha; &amp;alpha; |\n"] {
        let doc=parse(source);assert!(matches!(native_table_document(&doc).unwrap(),std::borrow::Cow::Borrowed(_)));
    }
    let doc=parse("<table><tr><th>H</th></tr><tr><td>&alpha &unknown; &#x80;</td></tr></table>");
    assert_eq!(native_table_document(&doc).unwrap_err(),UNSUPPORTED);
}

#[test]
fn native_html_table_pdf_engine_enum_preserves_all_four_engines_and_early_target_protection() {
    for engine in [Value::Null,json!("rust"),json!("typst"),json!("xelatex"),json!("pdflatex")] {
        assert!(crate::publication_profile::preflight("pdf","Plain owned text",&json!({"pdf":{"engine":engine}})).is_ok());
    }
    let directory=tempfile::tempdir().unwrap();let target=directory.path().join("protected.pdf");std::fs::write(&target,b"OWNED ORIGINAL").unwrap();
    for engine in [json!("bogus"),json!(""),json!("Rust"),json!("native"),json!(false),json!(123)] {
        let options=json!({"pdf":{"engine":engine}});
        let error=crate::mdexport::export_document_with_data("pdf",MERGED,"",target.to_str().unwrap(),&options,"Owned",&directory.path().join("assets-must-not-exist"),&directory.path().join("profile-must-not-exist")).unwrap_err();
        assert_eq!(error,"latex_invalid_engine");assert_eq!(std::fs::read(&target).unwrap(),b"OWNED ORIGINAL");
        let error=crate::mdexport::export_pdf(MERGED,"",target.to_str().unwrap(),&options,"Owned",&directory.path().join("assets-must-not-exist")).unwrap_err();assert_eq!(error,"latex_invalid_engine");
    }
    assert!(!directory.path().join("profile-must-not-exist").exists());assert!(!directory.path().join("assets-must-not-exist").exists());
}

#[test]
fn native_html_table_same_html_block_comments_do_not_decode_or_reject_hidden_entities() {
    let source="<!-- &alpha &unknown; &alpha; --><table><tr><th>H</th></tr><tr><td>&beta; <a href=\"https://example.invalid/\" title=\"<!-- &alpha; -->\">LINK</a></td></tr></table>";
    for doc in [parse(source),Document{front_matter:None,blocks:vec![Block::Html(source.into())]}] {
        let projected=native_table_document(&doc).unwrap();
        let Block::Table{rows,..}=&projected.blocks[0]else{panic!("native table")};
        assert_eq!(inline_plain(&rows[0][0]),"β LINK");
        assert!(rows[0][0].iter().any(|value|matches!(value,Inline::Link{title,..} if title=="<!-- α -->")));
        let xml=document_xml(&doc);let root=crate::math_document::parse_xml(&xml).unwrap();
        assert_eq!(elements(&root,"w:tbl").len(),1);assert_eq!(elements(&root,"w:tc")[1].text(),"β LINK");
    }
    assert_eq!(native_table_document(&parse(&source.replace("&beta;","&unknown;"))).unwrap_err(),UNSUPPORTED);
}
