//! Source/AST bridge regressions. Fixed-engine PDF geometry is independently
//! calibrated in the owned focused CLI evidence and remains separate acceptance.
use super::*;
use serde_json::json;

fn style()->Value{crate::export_styles::sanitize(&json!({"frontMatter":{"mode":"none"},"cover":{"enabled":false},"toc":{"enabled":false},"typography":{"font":"Arial","size":11,"lineHeight":2,"spacing":3}}))}
fn paragraph(value:&str)->Block{Block::Paragraph(vec![Inline::Text(value.into())])}
fn heading(value:&str)->Block{Block::Heading{level:1,id:value.into(),inlines:vec![Inline::Text(value.into())],numbering:None}}
pub(super) fn writer<'a>(style:&'a Value,base:&'a str,dir:&'a Path,blocks:&[Block],nonce:Option<&'a str>)->Writer<'a>{
    let mut notes=HashMap::new();collect_notes(blocks,&mut notes);
    Writer{style,base,dir,notes,seen_notes:HashSet::new(),end_notes:Vec::new(),end_ids:HashMap::new(),end_refs:HashMap::new(),note_numbers:HashMap::new(),section_ids:HashSet::new(),note_depth:0,images:HashMap::new(),resources:0,bytes:0,warnings:Vec::new(),local:None,mapping_nonce:nonce,ordinary_body:true,local_line:None,local_id:None,paragraph_index:0,inline_object_bounds:None,geometry:None,front_blocks:HashSet::new(),author:None,author_base:ResolvedTextStyle::base(11.,11.),local_pair:None,baseline:None,local_baseline:None,source_id:None,baseline_links:Vec::new(),baseline_protected:Vec::new()}
}
fn section_source(count:u8,body:&str)->String{format!("<!-- readmd-section:v1 {{\"id\":\"main\",\"page\":{{\"columns\":{count}}}}} -->\n\n{body}\n\n<!-- readmd-section:v1 {{\"id\":\"full\",\"break\":\"continuous\",\"page\":{{\"columns\":1}}}} -->\n\nFull width tail.\n")}
fn call<'a>(source:&'a str,id:&str)->&'a str{
    source.lines().find(|line|line.contains("mm,count:")&&line.contains(&format!("id:{}",string(id)))).unwrap()
}

#[test]
fn column_units_keep_heading_and_explicit_keep_next_chain_together(){
    let style=style();let dir=tempfile::tempdir().unwrap();
    let blocks=vec![heading("Heading"),Block::Html("<!-- unchanged trivia -->".into()),Block::Styled{attributes:json!({"paragraph":{"keepNext":true}}),block:Box::new(paragraph("Kept body"))},paragraph("Following body"),paragraph("Independent tail")];
    let original=blocks.clone();let mut writer=writer(&style,"",dir.path(),&blocks,None);
    let (parts,safe)=group_column_parts(writer.rendered_blocks(&blocks).unwrap());
    assert!(safe);assert_eq!(parts.len(),2);assert!(parts[0].contains(&text("Heading")));assert!(parts[0].contains(&text("Kept body")));assert!(parts[0].contains(&text("Following body")));assert!(!parts[0].contains(&text("Independent tail")));assert_eq!(parts[1],format!("#par[{}]\n",text("Independent tail")));assert_eq!(blocks,original);
}

#[test]
fn explicit_heading_keep_next_false_overrides_default_and_preserves_each_block(){
    let style=style();let dir=tempfile::tempdir().unwrap();
    let blocks=vec![Block::Styled{attributes:json!({"paragraph":{"keepNext":false}}),block:Box::new(heading("Independent heading"))},paragraph("First body"),paragraph("Second body")];
    let mut writer=writer(&style,"",dir.path(),&blocks,None);let (parts,safe)=group_column_parts(writer.rendered_blocks(&blocks).unwrap());
    assert!(safe);assert_eq!(parts.len(),3);assert!(parts[0].contains(&text("Independent heading")));assert!(!parts[0].contains(&text("First body")));assert!(parts[1].contains(&text("First body")));assert!(parts[2].contains(&text("Second body")));
}

#[test]
fn pending_source_markers_stay_with_their_exact_heading_and_body_during_grouping(){
    let style=style();let dir=tempfile::tempdir().unwrap();let nonce="0123456789abcdef0123456789abcdef";
    let marker=|id:&str|Block::Html(format!("<!-- readmd:page-source {nonce} {id} -->"));
    let blocks=vec![marker("heading-node"),heading("Mapped heading"),Block::Html("<!-- ordinary comment -->".into()),marker("body-node"),paragraph("Mapped body"),marker("tail-node"),paragraph("Mapped tail")];
    let mut writer=writer(&style,"",dir.path(),&blocks,Some(nonce));let (parts,safe)=group_column_parts(writer.rendered_blocks(&blocks).unwrap());
    assert!(safe);assert_eq!(parts.len(),2);
    for (id,index) in [("heading-node",0),("body-node",0),("tail-node",1)]{let uri=string(&format!("{}{id}",crate::publication_pagination::prefix(nonce)));let actual=format!("link({uri})[");assert_eq!(parts[index].matches(&actual).count(),1);assert!(!parts[1-index].contains(&actual));}
    assert!(parts.iter().all(|part|!part.contains("readmd:page-source")));assert!(parts[0].find(&text("Mapped heading")).unwrap()<parts[0].find(&text("Mapped body")).unwrap());
}

#[test]
fn sections_render_math_and_reused_image_resources_once_without_touching_inputs(){
    let input=tempfile::tempdir().unwrap();let output=tempfile::tempdir().unwrap();let svg=r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="40"><rect width="100" height="40" fill="blue"/></svg>"#;
    fs::write(input.path().join("figure.svg"),svg).unwrap();let raw=section_source(2,"Formula $x+1$.\n\n![Figure](figure.svg)\n\n![Repeat](figure.svg)");let doc=crate::md_ast::parse(&raw);let original=doc.clone();
    let source=render(&doc,&style(),input.path().to_str().unwrap(),output.path(),"Evidence").unwrap();
    let files=fs::read_dir(output.path()).unwrap().map(|entry|entry.unwrap().file_name().to_string_lossy().into_owned()).collect::<Vec<_>>();assert_eq!(files.len(),2,"One formula resource and one reused image resource: {files:?}");assert_eq!(source.text.matches("#image(\"resource-1.svg\"").count(),1);assert_eq!(source.text.matches("#image(\"resource-2.svg\"").count(),2);assert_eq!(fs::read_to_string(input.path().join("figure.svg")).unwrap(),svg);assert_eq!(doc,original);assert_eq!(call(&source.text,"main").contains("safe:true"),true);
}

#[test]
fn repeated_notes_keep_one_body_one_native_reference_and_disable_column_measurement(){
    let dir=tempfile::tempdir().unwrap();let raw=section_source(2,"Body[^n].\n\nRepeat[^n].\n\n[^n]: Unique note evidence.");let doc=crate::md_ast::parse(&raw);
    let source=render(&doc,&style(),"",dir.path(),"Notes").unwrap();assert!(call(&source.text,"main").contains("safe:false"));assert_eq!(source.text.matches("#footnote[").count(),1);assert_eq!(source.text.matches("#footnote(label(").count(),1);assert_eq!(source.text.matches(&text("Unique note evidence.")).count(),1);assert!(source.warnings.iter().any(|warning|warning=="publication_continuous_columns_partial"));
}

#[test]
fn column_safety_rejects_nested_notes_explicit_breaks_and_complex_bibliographies(){
    let style=style();let nested=Block::Quote(vec![Block::List{ordered:false,start:1,items:vec![crate::md_ast::ListItem{task:None,blocks:vec![Block::Paragraph(vec![Inline::Strong(vec![Inline::FootnoteRef("n".into())])])]}]}]);
    assert!(!safe_column_block(&style,&nested));assert!(!safe_column_block(&style,&Block::PageBreak));assert!(!safe_column_block(&style,&Block::FootnoteDef{label:"n".into(),blocks:vec![paragraph("Note")]}));
    assert!(!safe_column_block(&style,&Block::Styled{attributes:json!({"paragraph":{"pageBreakBefore":true}}),block:Box::new(paragraph("Next page"))}));
    let mut heading_style=style.clone();heading_style["headings"]["h1"]["pageBreakBefore"]=json!(true);assert!(!safe_column_block(&heading_style,&heading("Forced")));
    let bib=crate::publication_rich::RichBibliography{title:"References".into(),items:vec![],hanging_indent:false,line_spacing:1,entry_spacing:0,source_span:None};assert!(!safe_column_block(&style,&Block::Bibliography(bib)));
    assert!(!safe_column_block(&style,&Block::Paragraph(vec![Inline::Display{display:crate::publication_rich::RichDisplay::Block,children:vec![Inline::Text("Nested display".into())]}])));
    assert!(!safe_column_block(&style,&Block::Html("<div>Unknown flow</div>".into())));
    assert!(safe_column_block(&style,&Block::Math("x+1".into())));assert!(safe_column_block(&style,&Block::Table{aligns:vec![],header:vec![vec![Inline::Text("A".into())]],rows:vec![vec![vec![Inline::Text("B".into())]]]}));
}

#[test]
fn helper_uses_each_current_section_paper_margins_gutter_and_global_paragraph_spacing(){
    let dir=tempfile::tempdir().unwrap();let raw="<!-- readmd-section:v1 {\"id\":\"portrait\",\"page\":{\"columns\":2,\"gutter\":3}} -->\n\n# Portrait\n\nA\n\nB\n\n<!-- readmd-section:v1 {\"id\":\"landscape\",\"page\":{\"orientation\":\"landscape\",\"columns\":2}} -->\n\nC\n\nD\n";
    let options=json!({"frontMatter":{"mode":"none"},"cover":{"enabled":false},"page":{"marginLeft":19,"marginRight":21,"marginBottom":22},"typography":{"size":11,"lineHeight":2,"spacing":3}});let source=render(&crate::md_ast::parse(raw),&options,"",dir.path(),"Paper").unwrap().text;
    assert!(call(&source,"portrait").contains("167mm,275mm,count:2"));assert!(call(&source,"landscape").contains("254mm,188mm,count:2"));assert!(call(&source,"portrait").contains("after:14pt"));assert_eq!(source.matches("#let readmd-section-columns(").count(),1);
    assert!(source.contains("top-edge:0.8em,bottom-edge:-0.2em"));assert!(source.contains("#set par(leading:11pt,spacing:14pt"));assert!(source.contains("let padding=query(<readmd-publication-parity-before>)"));assert!(source.contains("if not padding and starts.len() > 0"));
}

#[test]
fn ordinary_documents_retain_legacy_column_source_and_no_partial_warning(){
    let dir=tempfile::tempdir().unwrap();let options=json!({"page":{"columns":2,"columnGap":7},"cover":{"enabled":false},"frontMatter":{"mode":"none"}});
    let plain=render(&crate::md_ast::parse("# Legacy\n\nBody.\n"),&options,"",dir.path(),"Legacy").unwrap();let comment=render(&crate::md_ast::parse("<!-- Ordinary comment -->\n\n# Legacy\n\nBody.\n"),&options,"",dir.path(),"Legacy").unwrap();
    assert_eq!(plain.text,comment.text);assert!(plain.text.contains("#columns(2,gutter:7mm)"));assert!(!plain.text.contains("readmd-section-columns"));assert!(!plain.warnings.iter().any(|warning|warning=="publication_continuous_columns_partial"));
}

#[test]
fn fallback_counts_and_all_original_rows_reach_the_helper_without_truncation(){
    let dir=tempfile::tempdir().unwrap();let body=(0..513).map(|i|format!("TOKEN{i}\n\n")).collect::<String>();let source=render(&crate::md_ast::parse(&section_source(3,&body)),&style(),"",dir.path(),"Large").unwrap();
    assert!(call(&source.text,"main").contains("count:3"));for i in 0..513{assert_eq!(source.text.matches(&text(&format!("TOKEN{i}"))).count(),1);}assert!(source.text.contains("parts.len() <= 512"));assert!(source.text.contains("else { native }"));
}

#[test]
fn continuous_column_limit_warning_is_deduplicated_and_does_not_claim_full_balance(){
    let dir=tempfile::tempdir().unwrap();let raw="<!-- readmd-section:v1 {\"id\":\"first\",\"page\":{\"columns\":2}} -->\n\nA\n\nB\n\n<!-- readmd-section:v1 {\"id\":\"second\",\"break\":\"continuous\"} -->\n\nC\n\nD\n\n<!-- readmd-section:v1 {\"id\":\"third\",\"break\":\"continuous\",\"page\":{\"columns\":1}} -->\n\nTail\n";
    let source=render(&crate::md_ast::parse(raw),&style(),"",dir.path(),"Flow").unwrap();assert_eq!(source.warnings.iter().filter(|warning|warning.as_str()=="publication_continuous_columns_partial").count(),1);assert_eq!(source.text.matches("#readmd-section-columns(").count(),2);
    let next_page=raw.replace("\"continuous\"","\"nextPage\"");let ordinary=render(&crate::md_ast::parse(&next_page),&style(),"",dir.path(),"Flow").unwrap();assert!(!ordinary.warnings.iter().any(|warning|warning=="publication_continuous_columns_partial"));
}

#[test]
fn mapped_section_identity_and_every_source_link_survive_the_helper_array(){
    let dir=tempfile::tempdir().unwrap();let raw=section_source(2,"# Mapped\n\nFirst original paragraph.\n\nSecond original paragraph.");let instrumented=crate::publication_pagination::instrument(&raw).unwrap();let doc=crate::md_ast::parse(&instrumented.content);
    let source=render_mapped(&doc,&style(),"",dir.path(),"Mapped",&instrumented.nonce).unwrap().text;
    for node in &instrumented.nodes{let uri=string(&format!("{}{}",crate::publication_pagination::prefix(&instrumented.nonce),node.id));assert_eq!(source.matches(&format!("link({uri})[")).count(),1,"Source node {} was lost or painted more than once",node.id);}
    for id in ["main","full"]{assert_eq!(source.matches(&format!("#label({})",string(&private_anchor("section",id)))).count(),1);}
    assert_eq!(source.matches("#metadata((index:").count(),2);assert!(!source.contains("readmd:page-source"));assert!(!source.contains("readmd-section:v1"));
}


#[test]
fn csl_cells_keep_split_styled_identifiers_breakable_in_nested_columns() {
    use crate::publication_rich::{RichBibliography, RichBibliographyItem, RichDisplay, RichNode};
    let group = |display, children| RichNode::Group { display: Some(display), entry_index: Some(7), children };
    let rich = vec![
        group(RichDisplay::LeftMargin, vec![
            group(RichDisplay::LeftMargin, vec![RichNode::text("NLEFT"), RichNode::Group {
                display: None, entry_index: None, children: vec![RichNode::text("100")],
            }]),
            group(RichDisplay::RightInline, vec![RichNode::text("LEFTBODY100")]),
        ]),
        group(RichDisplay::RightInline, vec![
            group(RichDisplay::LeftMargin, vec![RichNode::text("NRIGHT100")]),
            group(RichDisplay::RightInline, vec![RichNode::text("RIGHTBODY office original ENDB")]),
        ]),
    ];
    let bib = RichBibliography { title: "References".into(),
        items: vec![RichBibliographyItem { key: "b".into(), first_field: vec![], rich }],
        hanging_indent: false, line_spacing: 1, entry_spacing: 0, source_span: None };
    let doc = crate::md_ast::Document { blocks: vec![Block::Bibliography(bib), paragraph("After ordinary paragraph.")], ..Default::default() };
    let before = doc.clone(); let dir = tempfile::tempdir().unwrap();
    let source = render(&doc, &style(), "", dir.path(), "Gutter").unwrap().text;
    let helper = include_str!("publication_typst_cell.typ");
    assert_eq!(source.matches("#let readmd-csl-cell(").count(), 1);
    assert!(helper.contains("if measure(it).width <= available { box(it) }"));
    assert!(helper.contains("it.text.clusters().map(cluster => box(text(cluster))).join()"));
    assert!(source.contains("size.width*0.35") && source.contains("column-gutter:0.75em"));
    for value in ["NLEFT", "100", "LEFTBODY100", "NRIGHT100", "RIGHTBODY office original ENDB", "After ordinary paragraph."] {
        assert!(source.contains(&text(value)), "Actual rich leaf disappeared: {value}");
    }
    assert_eq!(doc, before);
    assert!(fs::read_dir(dir.path()).unwrap().next().is_none());
}
