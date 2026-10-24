use super::*;

const W:&str="http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R:&str="http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const P:&str="http://schemas.openxmlformats.org/package/2006/relationships";
fn root(text:&str)->XmlNode { docx_notes::xml(text.as_bytes()).unwrap() }
fn package(body:&str,numbering:&str,styles:&str)->Vec<(String,Vec<u8>)> {
    let mut parts=vec![
        ("[Content_Types].xml".into(),br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/></Types>"#.to_vec()),
        ("_rels/.rels".into(),format!(r#"<Relationships xmlns="{P}"><Relationship Id="main" Type="{R}/officeDocument" Target="word/document.xml"/></Relationships>"#).into_bytes()),
        ("word/document.xml".into(),format!(r#"<w:document xmlns:w="{W}" xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><w:body>{body}</w:body></w:document>"#).into_bytes()),
    ];
    let mut relationships=String::new();
    if !numbering.is_empty() {
        parts.push(("word/config/counters.xml".into(),format!(r#"<w:numbering xmlns:w="{W}">{numbering}</w:numbering>"#).into_bytes()));
        relationships.push_str(&format!(r#"<Relationship Id="numbers" Type="{R}/numbering" Target="config/counters.xml"/>"#));
    }
    if !styles.is_empty() {
        parts.push(("word/config/styles.xml".into(),format!(r#"<w:styles xmlns:w="{W}">{styles}</w:styles>"#).into_bytes()));
        relationships.push_str(&format!(r#"<Relationship Id="styles" Type="{R}/styles" Target="config/styles.xml"/>"#));
    }
    parts.push(("word/_rels/document.xml.rels".into(),format!(r#"<Relationships xmlns="{P}">{relationships}</Relationships>"#).into_bytes()));
    parts
}
fn load(parts:&[(String,Vec<u8>)])->Result<(XmlNode,ImportPlan),String> {
    let document=root(std::str::from_utf8(docx_notes::unique(parts,"word/document.xml")?).unwrap());
    let rels=docx_notes::relationships(parts,"word/document.xml")?;
    let plan=ImportPlan::load(parts,"word/document.xml",&document,&rels)?;
    Ok((document,plan))
}
fn outline(levels:&str)->String {
    format!(r#"<w:abstractNum w:abstractNumId="9">{levels}</w:abstractNum><w:num w:numId="7"><w:abstractNumId w:val="9"/></w:num>"#)
}
fn heading(level:u8,id:u32,text:&str)->String {
    format!(r#"<w:p><w:pPr><w:pStyle w:val="Heading{level}"/><w:numPr><w:ilvl w:val="{}"/><w:numId w:val="{id}"/></w:numPr></w:pPr><w:bookmarkStart w:id="{level}" w:name="heading_{level}"/><w:r><w:t>{text}</w:t></w:r><w:bookmarkEnd w:id="{level}"/></w:p>"#,level-1)
}
fn level(index:u8,start:u32,format:&str,text:&str,suffix:&str)->String {
    format!(r#"<w:lvl w:ilvl="{index}"><w:start w:val="{start}"/><w:numFmt w:val="{format}"/><w:lvlText w:val="{text}"/><w:suff w:val="{suffix}"/><w:pStyle w:val="Heading{}"/></w:lvl>"#,index+1)
}
fn caption(cache:&str)->String {
    format!(r#"<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:bookmarkStart w:id="91" w:name="cap_table"/><w:r><w:t xml:space="preserve">Table </w:t></w:r><w:fldSimple w:instr=" SEQ Table \r 4 \* ARABIC "><w:r><w:t>{cache}</w:t></w:r></w:fldSimple><w:bookmarkEnd w:id="91"/><w:r><w:t xml:space="preserve"> — </w:t></w:r><w:r><w:t>Current title</w:t></w:r></w:p><w:p><w:fldSimple w:instr=" REF cap_table \h "><w:r><w:t>Table {cache}</w:t></w:r></w:fldSimple></w:p>"#)
}
fn two_column_table()->&'static str {
    r#"<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Measure</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Value</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>Item</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>42</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#
}
fn mutate(parts:&mut [(String,Vec<u8>)],part:&str,from:&str,to:&str) {
    let bytes=&mut parts.iter_mut().find(|(name,_)|name==part).unwrap().1;
    let before=String::from_utf8(bytes.clone()).unwrap();assert!(before.contains(from),"{from}");
    *bytes=before.replace(from,to).into_bytes();
}

#[test]
fn complete_native_rule_defaults_and_override_replacement_are_not_old_list_defaults() {
    let native=NativeNumbering::parse(&root(&format!(r#"<w:numbering xmlns:w="{W}"><w:abstractNum w:abstractNumId="1"><w:lvl w:ilvl="0"><w:start w:val="9"/><w:numFmt w:val="upperRoman"/><w:lvlText w:val="(%1)"/><w:suff w:val="space"/></w:lvl></w:abstractNum><w:num w:numId="8"><w:abstractNumId w:val="1"/><w:lvlOverride w:ilvl="0"><w:lvl w:ilvl="0"><w:lvlText w:val="[%1]"/></w:lvl><w:startOverride w:val="0"/></w:lvlOverride></w:num></w:numbering>"#))).unwrap();
    let rule=native.levels(8,&Styles::default(),&mut HashSet::new()).unwrap().remove(&0).unwrap();
    assert_eq!((rule.format.as_str(),rule.start,rule.suffix.as_str(),rule.text.as_str()),("decimal",0,"tab","[%1]"));
}
#[test]
fn native_six_level_text_suffix_islegal_zero_and_styles_are_independently_parsed() {
    let mut levels=String::new();let mut body=String::new();
    for index in 0..6 {
        let text=if index==0{"Part %1)"}else if index==1{"[%2/%1/%2]"}else{"%1:%2:%3 %9"};
        levels.push_str(&level(index,if index==0{0}else{28},if index==1{"upperLetter"}else{"decimal"},text,["tab","nothing","space"][index as usize%3]));
        body.push_str(&heading(index+1,7,&format!("Author {}",index+1)));
    }
    let parts=package(&body,&outline(&levels),"");
    let (document,plan)=load(&parts).unwrap();let config=plan.config.as_ref().unwrap();
    assert_eq!(config["headings"]["levels"].as_array().unwrap().len(),6);
    assert_eq!(config["headings"]["levels"][0]["start"],0);
    assert_eq!(config["headings"]["levels"][1]["text"],"[%2/%1/%2]");
    assert_eq!(config["headings"]["levels"][1]["suffix"],"nothing");
    let paragraph=document.descendants().into_iter().find(|n|n.name=="w:p").unwrap();
    let heading=plan.heading(paragraph).unwrap();assert_eq!(heading.level,1);assert!(heading.literal_prefix.is_empty());
    let output=plan.finish("# Current body\n".into()).unwrap();
    assert!(output.contains("Part %1)"));assert!(output.ends_with("# Current body\n"));assert!(!output.contains("Author 1"));
}
#[test]
fn inherited_style_numbering_and_explicit_native_off_keep_their_real_roles() {
    let styles=r#"<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:pPr><w:outlineLvl w:val="0"/><w:numPr><w:numId w:val="7"/></w:numPr></w:pPr></w:style><w:style w:type="paragraph" w:styleId="Derived"><w:name w:val="Local heading"/><w:basedOn w:val="Heading1"/></w:style>"#;
    let body=r#"<w:p><w:pPr><w:pStyle w:val="Derived"/></w:pPr><w:r><w:t>First</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="Derived"/><w:numPr><w:numId w:val="0"/></w:numPr></w:pPr><w:r><w:t>Manual</w:t></w:r></w:p>"#;
    let parts=package(body,&outline(&level(0,0,"decimal","%1","space")),styles);
    let(document,plan)=load(&parts).unwrap();let ps=document.descendants().into_iter().filter(|n|n.name=="w:p").collect::<Vec<_>>();
    assert!(!plan.heading(ps[0]).unwrap().unnumbered);assert!(plan.heading(ps[1]).unwrap().unnumbered);
}
#[test]
fn an_empty_native_numbered_heading_keeps_its_counter_and_identity_through_source_and_docx() {
    let empty=heading(1,7,"").replace("heading_1","native_empty").replace(r#"w:id="1""#,r#"w:id="41""#);
    let real=heading(1,7,"Real").replace("heading_1","native_real").replace(r#"w:id="1""#,r#"w:id="42""#);
    let parts=package(&format!("<w:p/>{empty}<w:p/>{real}<w:p/>"),&outline(&level(0,3,"upperRoman","%1","space")),"");
    assert!(!parts.iter().any(|(name,_)|name==SOURCE_PART));
    // Exact native-derived identities are fixed independently of the product
    // bookmark mapping. The first native paragraph has no visible author text.
    let ids=["docx-ac632ea8964a7b9fbc3fed19","docx-c96b06508c3b8b82a6b12ff4"];
    let expected=vec![(ids[0].to_string(),String::new(),"III".into()),(ids[1].to_string(),"Real".into(),"IV".into())];
    fn snapshot(source:&str)->Vec<(String,String,String)> {
        let projected=crate::cross_references::project(source);
        assert!(projected.diagnostics.is_empty(),"{:?}",projected.diagnostics);
        let document=crate::md_ast::parse(&projected.content);
        document.blocks.iter().filter_map(|block|match block.unstyled(){
            crate::md_ast::Block::Heading{level,id,inlines,numbering}=>{
                assert_eq!(*level,1);
                Some((id.clone(),crate::md_ast::inline_plain(inlines),numbering.as_ref().unwrap().text.clone()))
            },_=>None,
        }).collect()
    }
    let imported=super::super::docx_convert(&parts,std::path::Path::new("empty-numbered.docx"),false).unwrap();
    assert_eq!(snapshot(&imported),expected);
    let mut returned=crate::docx_writer::build(&crate::md_ast::parse(&imported),&json!({"frontMatter":{"mode":"none"}}),"empty-numbered.md",&|_|None);
    assert!(!returned.parts.is_empty(),"{:?}",returned.warns);
    attach_source_info(&mut returned.parts,&imported).unwrap();
    let(document,plan)=load(&returned.parts).unwrap();
    let paragraphs=document.descendants().into_iter().filter(|node|plan.heading(node).is_some()).collect::<Vec<_>>();
    assert_eq!(paragraphs.len(),2);
    let mut native_ids=Vec::new();
    for (index,paragraph) in paragraphs.iter().enumerate() {
        assert_eq!(plan.heading(paragraph).unwrap().id,ids[index]);
        assert_eq!(super::super::docx_para_plain(paragraph),if index==0{""}else{"Real"});
        let properties=Properties::parse(child(paragraph,"w:pPr").unwrap()).unwrap();
        native_ids.push(properties.num_id.unwrap());
        let bookmark=paragraph.children.iter().find(|node|node.name=="w:bookmarkStart").unwrap();
        assert_eq!(bookmark.attr("w:name"),Some(crate::docx_writer::bookmark_name(ids[index]).as_str()));
    }
    assert_ne!(native_ids[0],0);assert_eq!(native_ids[0],native_ids[1]);
    let numbering=NativeNumbering::parse(&root(std::str::from_utf8(docx_notes::unique(&returned.parts,"word/numbering.xml").unwrap()).unwrap())).unwrap();
    let rules=numbering.levels(native_ids[0],&Styles::default(),&mut HashSet::new()).unwrap();
    let rule=&rules[&0];assert_eq!((rule.format.as_str(),rule.start,rule.text.as_str()),("upperRoman",3,"%1"));
    let reopened=super::super::docx_convert(&returned.parts,std::path::Path::new("returned-empty-numbered.docx"),false).unwrap();
    assert_eq!(snapshot(&reopened),expected);

    let ordinary=package("<w:p/><w:p><w:r><w:t>Ordinary body</w:t></w:r></w:p><w:p/>","","");
    let imported=super::super::docx_convert(&ordinary,std::path::Path::new("ordinary-empty.docx"),false).unwrap();
    assert_eq!(imported,"Ordinary body");
    assert!(crate::cross_references::project(&imported).numbering.is_none());
    assert!(crate::md_ast::parse(&imported).blocks.iter().all(|block|!matches!(block.unstyled(),crate::md_ast::Block::Heading{..})));
}
#[test]
fn real_foreign_seq_reset_and_ref_import_as_editable_rules_and_identity_without_source_info() {
    let parts=package(&caption("4"),"","");
    let(document,plan)=load(&parts).unwrap();
    assert_eq!(plan.config.as_ref().unwrap()["captions"]["table"]["sequence"],"Table");
    assert_eq!(plan.config.as_ref().unwrap()["captions"]["table"]["start"],4);
    assert_eq!(plan.config.as_ref().unwrap()["captions"]["table"]["captionSeparator"]," — ");
    let paragraphs=document.descendants().into_iter().filter(|n|n.name=="w:p").collect::<Vec<_>>();
    let caption=plan.caption(paragraphs[1]).unwrap();assert!(caption.automatic);assert!(caption.id.starts_with("tbl:"));
    assert_eq!(plan.anchor("cap_table"),Some(caption.id.as_str()));
    let reference=paragraphs[2].children.iter().find(|n|n.name=="w:fldSimple").unwrap();
    assert_eq!(plan.inline_edit(reference),Some(&InlineEdit::Source(format!(":ref[{}]",caption.id))));
}
#[test]
fn captioned_two_column_tables_survive_default_form_projection_and_native_return() {
    let spacer=r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0"/></w:pPr></w:p>"#;
    let native_caption=caption("4");let tail=native_caption.split_once("</w:tbl>").unwrap().1;
    let native=package(&format!("{}{spacer}{tail}",two_column_table()),"","");
    assert!(!native.iter().any(|(name,_)|name==SOURCE_PART));
    let source="<!-- readmd-numbering:v1 {\"headings\":{\"levels\":[{\"format\":\"upperRoman\",\"start\":3}]},\"captions\":{\"table\":{\"scope\":\"chapter\",\"start\":4}}} -->\n\n# SETTINGS_ALPHA {#settings-alpha}\n\n| **Measure** | **Value** |\n|---|---|\n| Item | 42 |\n\n:label[tbl:settings] SETTINGS_TABLE\n\nSee :ref[tbl:settings].\n";
    let mut authored=crate::docx_writer::build(&crate::md_ast::parse(source),&json!({"frontMatter":{"mode":"none"}}),"settings.md",&|_|None);
    assert!(!authored.parts.is_empty(),"{:?}",authored.warns);
    attach_source_info(&mut authored.parts,source).unwrap();
    for (parts,id,text,sequence) in [(native,"tbl:docx-c001543f1a98d9dc458e4863","Table 4","Table"),
        (authored.parts,"tbl:settings","Table III.4","ReadMDTable")] {
        let(document,plan)=load(&parts).unwrap();
        let tables=document.descendants().into_iter().filter(|node|node.name=="w:tbl").collect::<Vec<_>>();
        assert_eq!(tables.len(),1);assert!(plan.preserve_table(tables[0]));
        let imported=super::super::docx_convert(&parts,std::path::Path::new("captioned-table.docx"),true).unwrap();
        assert!(imported.contains(&format!(":label[{id}]"))&&imported.contains(&format!(":ref[{id}]")));
        let projected=crate::cross_references::project(&imported);
        assert!(projected.diagnostics.is_empty(),"{:?}\n{imported}",projected.diagnostics);
        assert_eq!(projected.labels.len(),1);assert_eq!(projected.labels[0]["id"],id);assert_eq!(projected.labels[0]["text"],text);
        let parsed=crate::md_ast::parse(&imported);
        assert_eq!(parsed.blocks.iter().filter(|block|matches!(block.unstyled(),crate::md_ast::Block::Table{..})).count(),1);
        assert_eq!(parsed.blocks.iter().filter(|block|matches!(block.unstyled(),crate::md_ast::Block::Caption{..})).count(),1);
        let mut returned=crate::docx_writer::build(&parsed,&json!({"frontMatter":{"mode":"none"}}),"returned-table.md",&|_|None);
        assert!(!returned.parts.is_empty(),"{:?}",returned.warns);
        attach_source_info(&mut returned.parts,&imported).unwrap();
        let word=std::str::from_utf8(docx_notes::unique(&returned.parts,"word/document.xml").unwrap()).unwrap();
        assert!(word.contains(&format!(" SEQ {sequence}")));
        assert!(word.contains(&format!(" REF {} ",crate::docx_writer::bookmark_name(id))));
        let(_,plan)=load(&returned.parts).unwrap();
        assert!(plan.captions.values().any(|caption|caption.automatic&&caption.id==id));
        let reopened=super::super::docx_convert(&returned.parts,std::path::Path::new("returned-table.docx"),true).unwrap();
        let projected=crate::cross_references::project(&reopened);
        assert!(projected.diagnostics.is_empty(),"{:?}",projected.diagnostics);
        assert_eq!(projected.labels.len(),1);assert_eq!(projected.labels[0]["id"],id);assert_eq!(projected.labels[0]["text"],text);
        assert!(reopened.contains(&format!(":ref[{id}]")));
    }
}
#[test]
fn form_table_preservation_uses_exact_caption_object_nodes_and_respects_boundaries() {
    let caption=caption("4");let tail=caption.split_once("</w:tbl>").unwrap().1;
    let same_table=two_column_table();
    let parts=package(&format!("{same_table}<w:p/>{tail}{same_table}"),"","");
    let(document,plan)=load(&parts).unwrap();
    let tables=document.descendants().into_iter().filter(|node|node.name=="w:tbl").collect::<Vec<_>>();
    assert_eq!(tables.len(),2);assert!(plan.preserve_table(tables[0]));assert!(!plan.preserve_table(tables[1]));
    let imported=super::super::docx_convert(&parts,std::path::Path::new("similar-tables.docx"),true).unwrap();
    let projected=crate::cross_references::project(&imported);assert!(projected.diagnostics.is_empty(),"{:?}",projected.diagnostics);assert_eq!(projected.labels.len(),1);
    let parsed=crate::md_ast::parse(&imported);
    assert_eq!(parsed.blocks.iter().filter(|block|matches!(block.unstyled(),crate::md_ast::Block::Table{..})).count(),1);
    assert_eq!(parsed.blocks.iter().filter(|block|matches!(block.unstyled(),crate::md_ast::Block::List{..})).count(),1);
    for barrier in [r#"<w:p><w:r><w:t>Intervening body</w:t></w:r></w:p>"#,
        r#"<w:p><w:pPr><w:pStyle w:val="Code"/></w:pPr></w:p>"#,
        r#"<w:p><w:pPr><w:sectPr/></w:pPr></w:p>"#,
        r#"<w:p><w:fldSimple w:instr=" PAGE "><w:r><w:t></w:t></w:r></w:fldSimple></w:p>"#] {
        let parts=package(&format!("{same_table}{barrier}{tail}"),"","");
        let(document,plan)=load(&parts).unwrap();let table=document.descendants().into_iter().find(|node|node.name=="w:tbl").unwrap();
        assert!(!plan.preserve_table(table),"{barrier}");assert!(plan.captions.is_empty(),"{barrier}");
    }
    let ordinary=package(same_table,"","");
    let(document,plan)=load(&ordinary).unwrap();let table=document.descendants().into_iter().find(|node|node.name=="w:tbl").unwrap();assert!(!plan.preserve_table(table));
    let imported=super::super::docx_convert(&ordinary,std::path::Path::new("ordinary-form.docx"),true).unwrap();
    assert!(imported.starts_with("- **Measure**:"));
    assert!(crate::md_ast::parse(&imported).blocks.iter().all(|block|!matches!(block.unstyled(),crate::md_ast::Block::Table{..})));
    let ordinary_sequence=format!(r#"{same_table}<w:p><w:fldSimple w:instr=" SEQ Ordinary \* UNKNOWN "><w:r><w:t>CURRENT_ORDINARY</w:t></w:r></w:fldSimple></w:p>"#);
    let parts=package(&ordinary_sequence,"","");
    let(document,plan)=load(&parts).unwrap();let table=document.descendants().into_iter().find(|node|node.name=="w:tbl").unwrap();
    assert!(!plan.preserve_table(table));assert!(plan.captions.is_empty());
    assert!(plan.diagnostics.iter().any(|diagnostic|diagnostic["reason"]=="unsupported_sequence_field"));
    let imported=super::super::docx_convert(&parts,std::path::Path::new("ordinary-sequence.docx"),true).unwrap();
    assert!(imported.starts_with("- **Measure**:")&&imported.contains("CURRENT_ORDINARY"));
    assert!(!imported.contains(":label[")&&!imported.contains(":ref["));
}
#[test]
fn a_manual_caption_cache_preserves_its_two_column_table_without_reviving_automatic_semantics() {
    let caption=caption("MANUAL");let tail=caption.split_once("</w:tbl>").unwrap().1;
    let parts=package(&format!("{}<w:p/>{tail}",two_column_table()),"","");
    let(document,plan)=load(&parts).unwrap();let table=document.descendants().into_iter().find(|node|node.name=="w:tbl").unwrap();
    assert!(plan.preserve_table(table));assert!(plan.captions.values().all(|caption|!caption.automatic));
    let imported=super::super::docx_convert(&parts,std::path::Path::new("manual-table.docx"),true).unwrap();
    assert!(imported.contains("MANUAL")&&!imported.contains(":label[")&&!imported.contains(":ref["));
    assert!(crate::cross_references::project(&imported).labels.is_empty());
    assert_eq!(crate::md_ast::parse(&imported).blocks.iter().filter(|block|matches!(block.unstyled(),crate::md_ast::Block::Table{..})).count(),1);
    assert!(plan.diagnostics.iter().any(|diagnostic|diagnostic["reason"]=="caption_result_modified"));
}
#[test]
fn a_real_empty_table_spacer_keeps_caption_identity_but_content_fields_and_list_numbers_do_not() {
    let spacer=r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0"/></w:pPr></w:p>"#;
    let normal=r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>"#;
    let body=caption("4").replacen("</w:tbl>",&format!("</w:tbl>{spacer}"),1);
    for styles in ["",normal] {
        let(document,plan)=load(&package(&body,"",styles)).unwrap();
        let imported=document.descendants().into_iter().find_map(|p|plan.caption(p)).unwrap();
        assert!(imported.automatic);
        assert_eq!(plan.config.as_ref().unwrap()["captions"]["table"]["sequence"],"Table");
        assert_eq!(plan.anchor("cap_table"),Some(imported.id.as_str()));
        let reference=document.descendants().into_iter().find(|p|p.name=="w:fldSimple"&&p.attr("w:instr").is_some_and(|code|code.contains("REF"))).unwrap();
        assert_eq!(plan.inline_edit(reference),Some(&InlineEdit::Source(format!(":ref[{}]",imported.id))));
    }
    for barrier in [
        r#"<w:p><w:r><w:t>Intervening body</w:t></w:r></w:p>"#,
        r#"<w:p><w:fldSimple w:instr=" PAGE "><w:r><w:t></w:t></w:r></w:fldSimple></w:p>"#,
        r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="77"/></w:numPr></w:pPr></w:p>"#,
        r#"<w:p><w:bookmarkStart w:id="92" w:name="barrier"/><w:bookmarkEnd w:id="92"/></w:p>"#,
        r#"<w:p><w:pPr><w:pStyle w:val="Code"/></w:pPr></w:p>"#,
        r#"<w:p><w:pPr><w:sectPr/></w:pPr></w:p>"#,
    ] {
        let body=caption("4").replacen("</w:tbl>",&format!("</w:tbl>{barrier}"),1);
        let(_,plan)=load(&package(&body,"",normal)).unwrap();
        assert!(plan.captions.is_empty(),"{barrier}");
        assert!(plan.config.is_none(),"{barrier}");
    }
}
#[test]
fn changed_caption_cache_is_literal_and_diagnostic_instead_of_resurrecting_old_automatic_rules() {
    let parts=package(&caption("MANUAL"),"","");
    let(document,plan)=load(&parts).unwrap();
    assert!(plan.config.is_none());
    let caption=document.descendants().into_iter().find_map(|p|plan.caption(p)).unwrap();
    assert!(!caption.automatic);
    assert!(plan.diagnostics.iter().any(|d|d["reason"]=="caption_result_modified"));
    assert!(plan.finish("Current MANUAL body".into()).unwrap().contains("Current MANUAL body"));
    assert!(!plan.edits.values().any(|edit|matches!(edit,InlineEdit::Source(value) if value.starts_with(":ref["))));
}
#[test]
fn duplicate_native_ids_bad_relationships_unknown_formats_and_shared_outlines_are_not_guessed() {
    let valid=outline(&level(0,1,"decimal","%1","space"));
    let body=heading(1,7,"One");
    for invalid_numbering in [format!("{valid}<w:num w:numId=\"7\"><w:abstractNumId w:val=\"9\"/></w:num>"),outline(&level(0,1,"hex","%1","space"))] {
        assert!(load(&package(&body,&invalid_numbering,"")).is_err());
    }
    let mixed=format!("{body}<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"7\"/></w:numPr></w:pPr><w:r><w:t>Ordinary list</w:t></w:r></w:p>");
    assert!(load(&package(&mixed,&valid,"")).unwrap_err().contains("outline_shared_with_list"));
    let mut parts=package(&body,&valid,"");
    mutate(&mut parts,"word/_rels/document.xml.rels","Target=\"config/counters.xml\"","Target=\"https://invalid.test/data\" TargetMode=\"External\"");
    assert!(load(&parts).unwrap_err().contains("relationship"));
}
#[test]
fn source_information_has_no_old_values_and_native_edits_win_while_identity_survives() {
    let source="<!-- readmd-numbering:v1 {\"headings\":{\"levels\":[{\"start\":0},{\"format\":\"upperLetter\",\"start\":28,\"text\":\"[%2/%1]\",\"suffix\":\"nothing\"}]},\"captions\":{\"table\":{\"start\":4,\"sequence\":\"Table\"}}} -->\n\n# Original {#stable-heading}\n\n## Child {#stable-child}\n\n| A |\n|---|\n| B |\n\n:label[tbl:stable] Author caption\n\nSee :ref[tbl:stable].\n";
    let document=crate::md_ast::parse(source);
    let mut exported=crate::docx_writer::build(&document,&json!({"frontMatter":{"mode":"none"}}),"native-roundtrip.md",&|_|None);
    assert!(!exported.parts.is_empty(),"{:?}",exported.warns);
    attach_source_info(&mut exported.parts,source).unwrap();
    let data=String::from_utf8(docx_notes::unique(&exported.parts,SOURCE_PART).unwrap().to_vec()).unwrap();
    for forbidden in ["Original","Author caption","upperLetter","Table&quot;","stable-child text"] { assert!(!data.contains(forbidden),"{forbidden}"); }
    mutate(&mut exported.parts,"word/numbering.xml","<w:start w:val=\"0\"/>","<w:start w:val=\"3\"/>");
    mutate(&mut exported.parts,"word/document.xml",">Original<",">Edited current body<");
    let(_,plan)=load(&exported.parts).unwrap();
    assert_eq!(plan.config.as_ref().unwrap()["headings"]["levels"][0]["start"],3);
    assert!(plan.headings.values().any(|h|h.id=="stable-heading"));
    assert_eq!(plan.config.as_ref().unwrap()["captions"]["table"]["sequence"],"Table");
    assert!(plan.captions.values().any(|caption|caption.automatic&&caption.id=="tbl:stable"));
    let info=source_info(&exported.parts,"word/document.xml",&docx_notes::relationships(&exported.parts,"word/document.xml").unwrap()).unwrap().unwrap();
    assert!(info.identities.iter().any(|identity|identity.source_id=="tbl:stable"));
}
#[test]
fn imported_offset_equation_captions_reproject_current_native_results_and_keep_ids_without_overwriting_manual_cache() {
    let config=json!({"headings":{"levels":[{"format":"upperRoman","start":3,"restartAfter":0}]},
        "captions":{"equation":{"scope":"chapter","format":"upperLetter","start":26,"separator":"/"}}});
    let mut source=format!("<!-- readmd-numbering:v1 {config} -->\n\n# Gamma {{#gamma}}\n\n");
    for number in 26..=53 {source.push_str(&format!("$$x={number}$$\n\n:label[eq:offset-{number}] Offset {number}\n\n"));}
    source.push_str("# Delta {#delta}\n\n$$z=1$$\n\n:label[eq:offset-reset] Restart offset\n\nSee :ref[eq:offset-26].\n");
    let mut exported=crate::docx_writer::build(&crate::md_ast::parse(&source),&json!({"frontMatter":{"mode":"none"}}),"offset-equations.md",&|_|None);
    assert!(!exported.parts.is_empty(),"{:?}",exported.warns);
    attach_source_info(&mut exported.parts,&source).unwrap();
    let imported=super::super::docx_convert(&exported.parts,std::path::Path::new("offset-equations.docx"),false).unwrap();
    assert_eq!(imported.matches("<!--readmd:math:v1:").count(),29);
    let projected=crate::cross_references::project(&imported);
    assert!(projected.diagnostics.is_empty(),"{:?}",projected.diagnostics);
    let suffixes=["Z","AA","BB","CC","DD","EE","FF","GG","HH","II","JJ","KK","LL","MM","NN","OO","PP","QQ","RR","SS","TT","UU","VV","WW","XX","YY","ZZ","AAA"];
    let mut expected=suffixes.iter().enumerate().map(|(index,suffix)|
        (format!("eq:offset-{}",index+26),format!("Equation III/{suffix}"))).collect::<Vec<_>>();
    expected.push(("eq:offset-reset".into(),"Equation IV/Z".into()));
    assert_eq!(projected.labels.iter().map(|label|(label["id"].as_str().unwrap().to_string(),label["text"].as_str().unwrap().to_string())).collect::<Vec<_>>(),expected);
    assert_eq!(projected.numbering.as_ref().unwrap().config.captions["equation"].start,26);
    assert!(imported.contains(":ref[eq:offset-26]"));

    let source_info=docx_notes::unique(&exported.parts,SOURCE_PART).unwrap().to_vec();
    let bytes=&mut exported.parts.iter_mut().find(|(name,_)|name=="word/document.xml").unwrap().1;
    let word=String::from_utf8(bytes.clone()).unwrap();
    let before=r#"<w:t xml:space="preserve">Z</w:t>"#;
    assert!(word.contains(before));
    *bytes=word.replacen(before,r#"<w:t xml:space="preserve">MANUAL</w:t>"#,1).into_bytes();
    assert_eq!(docx_notes::unique(&exported.parts,SOURCE_PART).unwrap(),source_info.as_slice());
    let(_,plan)=load(&exported.parts).unwrap();
    assert_eq!(plan.captions.len(),29);
    assert!(plan.captions.values().all(|caption|!caption.automatic));
    assert!(plan.captions.values().any(|caption|caption.id=="eq:offset-26"));
    assert!(plan.diagnostics.iter().any(|diagnostic|diagnostic["reason"]=="caption_result_modified"));
    let mut diagnosed_ids=plan.diagnostics.iter().filter(|diagnostic|diagnostic["reason"]=="caption_result_modified"&&diagnostic["kind"]=="equation")
        .map(|diagnostic|diagnostic["source_id"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    let mut expected_ids=expected.iter().map(|(id,_)|id.clone()).collect::<Vec<_>>();
    diagnosed_ids.sort();expected_ids.sort();assert_eq!(diagnosed_ids,expected_ids);
    let manual=super::super::docx_convert(&exported.parts,std::path::Path::new("offset-equations.docx"),false).unwrap();
    assert!(manual.contains("MANUAL")&&manual.contains("id=\"eq:offset-26\""));
    assert!(!manual.contains(":label[eq:offset-26]")&&!manual.contains(":ref[eq:offset-26]"));
    // The first native SEQ still consumed ordinal 1. Conservatively keeping
    // the entire family literal prevents the remaining 28 captions from
    // silently becoming ordinals 1..28 in a newly shortened source sequence.
    let manual_projection=crate::cross_references::project(&manual);
    assert!(manual_projection.labels.is_empty());
    assert!(!manual_projection.numbering.as_ref().unwrap().config.captions.contains_key("equation"));
    assert!(!manual_projection.content.contains("<!--readmd:caption:v1"));
    let document=crate::md_ast::parse(&manual_projection.content);
    let actual_current=document.blocks.iter().filter_map(|block|match block.unstyled(){
        crate::md_ast::Block::Paragraph(inlines)=>Some(crate::md_ast::inline_plain(inlines)),_=>None,
    }).filter(|text|text.starts_with("Equation ")).collect::<Vec<_>>();
    let expected_current=expected.iter().enumerate().map(|(index,(_,label))|{
        let label=if index==0{"Equation III/MANUAL"}else{label.as_str()};
        let title=if index==28{"Restart offset".into()}else{format!("Offset {}",index+26)};
        format!("{label} — {title}")
    }).collect::<Vec<_>>();
    assert_eq!(actual_current,expected_current);
}
#[test]
fn no_related_source_settings_add_no_source_part_and_cleared_native_numbering_is_not_restored() {
    let source="# Plain {#plain}\n";
    let mut exported=crate::docx_writer::build(&crate::md_ast::parse(source),&json!({}),"plain.md",&|_|None);
    attach_source_info(&mut exported.parts,source).unwrap();assert!(!exported.parts.iter().any(|(name,_)|name==SOURCE_PART));
    let source="<!-- readmd-numbering:v1 {\"headings\":{\"levels\":[{}]}} -->\n\n# Numbered {#stable}\n";
    let mut exported=crate::docx_writer::build(&crate::md_ast::parse(source),&json!({}),"numbered.md",&|_|None);
    attach_source_info(&mut exported.parts,source).unwrap();
    mutate(&mut exported.parts,"word/document.xml","<w:numId w:val=\"2\"/>","<w:numId w:val=\"0\"/>");
    let(_,plan)=load(&exported.parts).unwrap();assert!(plan.config.is_none());assert!(plan.headings.values().any(|h|h.id=="stable"));
}
#[test]
fn native_namespace_aliases_and_nonstandard_relationship_paths_are_resolved_by_opc() {
    let mut parts=package(&heading(1,7,"One"),&outline(&level(0,0,"decimal","(%1)","nothing")),"");
    for(name,bytes)in &mut parts {
        if name=="word/document.xml"||name=="word/config/counters.xml" {
            *bytes=String::from_utf8(bytes.clone()).unwrap().replace("xmlns:w=","xmlns:n=").replace("w:","n:").into_bytes();
        }
    }
    let(_,plan)=load(&parts).unwrap();assert_eq!(plan.config.as_ref().unwrap()["headings"]["levels"][0]["text"],"(%1)");
    let document=root(std::str::from_utf8(docx_notes::unique(&parts,"word/document.xml").unwrap()).unwrap());
    let mut rels=docx_notes::relationships(&parts,"word/document.xml").unwrap();
    parts.iter_mut().find(|(name,_)|name=="word/config/counters.xml").unwrap().0="counters/active.xml".into();
    rels.iter_mut().find(|rel|relation(&rel.kind,"numbering")).unwrap().raw="../counters/active.xml".into();
    let plan=ImportPlan::load(&parts,"word/document.xml",&document,&rels).unwrap();assert!(plan.config.is_some());
}

#[test]
fn a_cross_paragraph_seq_reset_cannot_hide_a_real_counter_consumer_but_toc_does_not_block_import() {
    let cross=r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> SEQ Ta</w:instrText></w:r></w:p><w:p><w:r><w:instrText>ble \r 0 </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>0</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#;
    let body=format!("{cross}{}",caption("1").replace(r" \r 4", ""));
    assert!(load(&package(&body,"","")).unwrap_err().contains("opaque_sequence_consumes_caption_counter"));
    let toc=r#"<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> TOC \o "1-3" </w:instrText></w:r></w:p><w:p><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>Contents</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#;
    let(_,plan)=load(&package(&format!("{toc}{}",caption("4")),"","")).unwrap();
    assert_eq!(plan.config.as_ref().unwrap()["captions"]["table"]["start"],4);
}

#[test]
fn ordinary_fields_at_shared_depth_boundaries_remain_readable_without_numbering_projection() {
    fn complex(depth:usize)->String {
        let begin=r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> QUOTE </w:instrText></w:r>"#;
        let result=r#"<w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>PLAIN_FIELD_VISIBLE</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#;
        format!("<w:p>{}{}</w:p>",begin.repeat(depth),result.repeat(depth))
    }
    let mut simple=r#"<w:r><w:t>PLAIN_FIELD_VISIBLE</w:t></w:r>"#.to_string();
    for _ in 0..65 {simple=format!(r#"<w:fldSimple w:instr=" QUOTE ">{simple}</w:fldSimple>"#);}
    for body in [complex(65),complex(128),format!("<w:p>{simple}</w:p>")] {
        let parts=package(&body,"","");
        let(_,plan)=load(&parts).unwrap();
        assert!(plan.config.is_none()&&plan.headings.is_empty()&&plan.captions.is_empty());
        assert!(plan.edits.is_empty()&&plan.diagnostics.is_empty());
        let actual=super::super::docx_convert(&parts,std::path::Path::new("ordinary-fields.docx"),false).unwrap();
        assert_eq!(actual,"PLAIN_FIELD_VISIBLE");
        assert!(!actual.contains("readmd-numbering")&&!actual.contains(":label[")&&!actual.contains(":ref["));
    }
    let parts=package(&complex(129),"","");
    assert_eq!(super::super::docx_convert(&parts,std::path::Path::new("ordinary-fields.docx"),false).unwrap_err(),"docx_field_budget");
}

#[test]
fn objects_in_native_number_or_reference_results_are_never_erased_by_a_matching_text_cache() {
    let math=r#"<m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>"#;
    let body=caption("4").replacen("<w:t>4</w:t>",&format!("<w:t>4</w:t>{math}"),1);
    let(document,plan)=load(&package(&body,"","")).unwrap();
    let number=document.descendants().into_iter().find(|n|n.name=="w:fldSimple"&&n.attr("w:instr").is_some_and(|code|code.contains("SEQ"))).unwrap();
    assert!(plan.inline_edit(number).is_none());
    assert!(!plan.captions.values().next().unwrap().automatic);
    let body=caption("4").replace("<w:t>Table 4</w:t>",&format!("<w:t>Table 4</w:t>{math}"));
    let(document,plan)=load(&package(&body,"","")).unwrap();
    let reference=document.descendants().into_iter().find(|n|n.name=="w:fldSimple"&&n.attr("w:instr").is_some_and(|code|code.contains("REF"))).unwrap();
    assert!(plan.inline_edit(reference).is_none());
    assert!(plan.diagnostics.iter().any(|d|d["reason"]=="reference_result_contains_objects"));
}
