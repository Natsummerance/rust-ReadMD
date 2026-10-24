use super::*;
const W:&str="http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const PKG:&str="http://schemas.openxmlformats.org/package/2006/relationships";

fn specials_xml(kind:&str)->String{format!("<w:{kind} w:id=\"41\" w:type=\"separator\"><w:p><w:r><w:separator/></w:r></w:p></w:{kind}><w:{kind} w:id=\"42\" w:type=\"continuationSeparator\"><w:p/></w:{kind}><w:{kind} w:id=\"43\" w:type=\"continuationNotice\"><w:p><w:r><w:t>Current native notice</w:t></w:r></w:p></w:{kind}>")}
fn refs(kind:&str)->String{format!("<w:{kind} w:id=\"41\"/><w:{kind} w:id=\"42\"/><w:{kind} w:id=\"43\"/>")}
fn fixture()->Vec<(String,Vec<u8>)>{
    let document=format!("<w:document xmlns:w=\"{W}\"><w:body><w:p><w:r><w:t>Body before</w:t><w:footnoteReference w:id=\"7\"/><w:endnoteReference w:id=\"7\"/><w:footnoteReference w:id=\"7\"/></w:r></w:p><w:sectPr><w:footnotePr><w:pos w:val=\"beneathText\"/></w:footnotePr></w:sectPr></w:body></w:document>");
    let rels=format!("<Relationships xmlns=\"{PKG}\"><Relationship Id=\"f\" Type=\"{REL}/footnotes\" Target=\"footnotes.xml\"/><Relationship Id=\"e\" Type=\"{REL}/endnotes\" Target=\"endnotes.xml\"/><Relationship Id=\"s\" Type=\"{REL}/settings\" Target=\"settings.xml\"/><Relationship Id=\"st\" Type=\"{REL}/styles\" Target=\"styles.xml\"/></Relationships>");
    let settings=format!("<w:settings xmlns:w=\"{W}\"><w:footnotePr><w:pos w:val=\"pageBottom\"/><w:numFmt w:val=\"lowerLetter\"/><w:numStart w:val=\"0\"/><w:numRestart w:val=\"eachPage\"/>{}</w:footnotePr><w:endnotePr><w:pos w:val=\"docEnd\"/><w:numFmt w:val=\"upperRoman\"/><w:numStart w:val=\"65535\"/><w:numRestart w:val=\"eachSect\"/>{}</w:endnotePr></w:settings>",refs("footnote"),refs("endnote"));
    let styles=format!("<w:styles xmlns:w=\"{W}\"><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val=\"22\"/><w:szCs w:val=\"22\"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type=\"paragraph\" w:styleId=\"NoteBase\"><w:pPr><w:spacing w:line=\"241\" w:lineRule=\"auto\"/></w:pPr></w:style><w:style w:type=\"paragraph\" w:styleId=\"FootnoteText\"><w:basedOn w:val=\"NoteBase\"/><w:pPr><w:spacing w:line=\"285\" w:lineRule=\"exact\"/></w:pPr><w:rPr><w:sz w:val=\"17\"/><w:szCs w:val=\"18\"/></w:rPr></w:style><w:style w:type=\"paragraph\" w:styleId=\"EndnoteText\"><w:basedOn w:val=\"NoteBase\"/><w:pPr><w:spacing w:line=\"315\" w:lineRule=\"atLeast\"/></w:pPr><w:rPr><w:sz w:val=\"60\"/><w:szCs w:val=\"62\"/></w:rPr></w:style></w:styles>");
    let story=|kind:&str,style:&str,text:&str|format!("<w:{kind}s xmlns:w=\"{W}\">{}<w:{kind} w:id=\"7\"><w:p><w:pPr><w:pStyle w:val=\"{style}\"/></w:pPr><w:r><w:{}Ref/></w:r><w:r><w:t>{text}</w:t></w:r></w:p></w:{kind}></w:{kind}s>",specials_xml(kind),kind);
    [("word/document.xml",document),("word/_rels/document.xml.rels",rels),("word/settings.xml",settings),("word/styles.xml",styles),("word/footnotes.xml",story("footnote","FootnoteText","Foot body")),("word/endnotes.xml",story("endnote","EndnoteText","End body"))].into_iter().map(|(name,value)|(name.into(),value.into_bytes())).collect()
}
fn import(entries:&[(String,Vec<u8>)])->Result<ImportedNoteSettings>{
    let root=docx_notes::xml(docx_notes::unique(entries,"word/document.xml").unwrap()).unwrap();
    let body=root.children.iter().find(|n|n.name=="w:body").unwrap();
    let rels=docx_notes::relationships(entries,"word/document.xml").unwrap();
    let notes=docx_notes::Notes::load(entries,body,&rels).unwrap();parse(entries,body,&rels,&notes)
}
fn change(entries:&mut[(String,Vec<u8>)],part:&str,from:&str,to:&str){
    let entry=entries.iter_mut().find(|(name,_)|name==part).unwrap();let value=String::from_utf8(entry.1.clone()).unwrap();assert!(value.contains(from),"{part}: {from}");entry.1=value.replacen(from,to,1).into_bytes();
}

#[test]
fn transitional_package_relationships_use_the_normative_office_2006_uri(){
    let mut entries=fixture();
    // Deliberately independent of the parser's REL constant: a shared typo
    // must not make the fixture and the relationship lookup agree incorrectly.
    entries.iter_mut().find(|(name,_)|name=="word/_rels/document.xml.rels").unwrap().1=br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="f" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/><Relationship Id="e" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes" Target="endnotes.xml"/><Relationship Id="s" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/><Relationship Id="st" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#.to_vec();
    let imported=import(&entries).unwrap();assert_eq!(imported.stats.parsed_parts,2);assert_eq!(imported.stats.borrowed_note_parts,2);
    assert_eq!(imported.policies.policy(NoteKind::Footnote).start,Some(0));assert_eq!(imported.policies.policy(NoteKind::Endnote).size,Some(30.));
}

#[test]
fn native_mixed_settings_same_id_repeated_refs_and_special_ids_are_independent(){
    let entries=fixture();let original=entries.clone();let result=import(&entries).unwrap();
    let foot=result.policies.policy(NoteKind::Footnote);let end=result.policies.policy(NoteKind::Endnote);
    assert_eq!(foot.number_format.as_deref(),Some("lowerLetter"));assert_eq!(end.number_format.as_deref(),Some("upperRoman"));
    assert_eq!(foot.start,Some(0));assert_eq!(end.start,Some(65535));
    assert_eq!(foot.restart,Some(NoteRestart::Page));assert_eq!(end.restart,Some(NoteRestart::Section));
    assert_eq!(foot.placement,Some(NotePlacement::BeneathText));assert_eq!(end.placement,Some(NotePlacement::DocEnd));
    assert_eq!(foot.size,Some(8.5));assert_eq!(foot.size_cs,Some(9.));assert_eq!(end.size,Some(30.));assert_eq!(end.size_cs,Some(31.));
    assert_eq!(foot.line_spacing,Some(LineSpacing::ExactPt(14.25)));assert_eq!(end.line_spacing,Some(LineSpacing::AtLeastPt(15.75)));
    assert_eq!(foot.separator,Some(NoteSeparator::Line));assert_eq!(foot.continuation_separator,Some(NoteSeparator::None));
    assert_eq!(foot.continuation_notice.as_deref(),Some("Current native notice"));
    assert_eq!(result.policies.kind_by_label.len(),2);assert_eq!(result.policies.kind_for_label("word-footnote-7"),NoteKind::Footnote);assert_eq!(result.policies.kind_for_label("word-endnote-7"),NoteKind::Endnote);
    assert!(result.sections.is_empty());assert!(result.local.is_empty());result.require_global_only().unwrap();
    assert_eq!(result.stats.borrowed_note_parts,2);assert_eq!(result.stats.parsed_parts,2);assert_eq!(result.stats.resolved_styles,3);
    assert_eq!(entries,original,"conversion never writes or mutates original package bytes");
}

#[test]
fn omitted_endnote_format_uses_word_lower_roman_not_legacy_decimal(){
    let mut entries=fixture();change(&mut entries,"word/settings.xml","<w:numFmt w:val=\"upperRoman\"/>","");
    let parsed=import(&entries).unwrap();assert_eq!(parsed.policies.policy(NoteKind::Endnote).number_format.as_deref(),Some("lowerRoman"));
    assert_eq!(parsed.to_options()["footnotes"]["kinds"]["endnote"]["start"],65535);
}

#[test]
fn native_current_changes_and_clears_are_read_without_historical_metadata(){
    let mut entries=fixture();change(&mut entries,"word/settings.xml","<w:numStart w:val=\"0\"/>","<w:numStart w:val=\"12000\"/>");
    change(&mut entries,"word/styles.xml","<w:sz w:val=\"17\"/>","<w:sz w:val=\"60\"/>");
    change(&mut entries,"word/settings.xml","<w:numRestart w:val=\"eachPage\"/>","");
    let parsed=import(&entries).unwrap();let foot=parsed.policies.policy(NoteKind::Footnote);
    assert_eq!(foot.start,Some(12000));assert_eq!(foot.size,Some(30.));assert_eq!(foot.restart,Some(NoteRestart::Continuous));
}

#[test]
fn native_relationship_paths_and_strict_namespace_aliases_are_followed(){
    let mut entries=fixture();
    for(part,target)in[("word/settings.xml","word/config/current.xml"),("word/styles.xml","word/theme/note-styles.xml"),("word/footnotes.xml","word/notes/foot.xml"),("word/endnotes.xml","word/notes/end.xml")]{
        entries.iter_mut().find(|(name,_)|name==part).unwrap().0=target.into();
    }
    change(&mut entries,"word/_rels/document.xml.rels","Target=\"settings.xml\"","Target=\"config/current.xml\"");
    change(&mut entries,"word/_rels/document.xml.rels","Target=\"styles.xml\"","Target=\"theme/note-styles.xml\"");
    change(&mut entries,"word/_rels/document.xml.rels","Target=\"footnotes.xml\"","Target=\"notes/foot.xml\"");
    change(&mut entries,"word/_rels/document.xml.rels","Target=\"endnotes.xml\"","Target=\"/word/notes/end.xml\"");
    for(_,bytes)in &mut entries{let value=String::from_utf8(bytes.clone()).unwrap();*bytes=value.replace(W,"http://purl.oclc.org/ooxml/wordprocessingml/main").replace(REL,STRICT_REL).replace("w:","alias:").replace("xmlns:w=","xmlns:alias=").into_bytes();}
    let parsed=import(&entries).unwrap();assert_eq!(parsed.policies.policy(NoteKind::Footnote).size,Some(8.5));assert_eq!(parsed.stats.borrowed_note_parts,2);
}

#[test]
fn differing_native_sections_keep_real_boundaries_and_ignored_endnote_pos_is_not_promoted(){
    let mut entries=fixture();
    change(&mut entries,"word/document.xml","<w:p><w:r>","<w:p><w:pPr><w:sectPr><w:footnotePr><w:pos w:val=\"pageBottom\"/><w:numFmt w:val=\"upperRoman\"/></w:footnotePr><w:endnotePr><w:pos w:val=\"sectEnd\"/></w:endnotePr></w:sectPr></w:pPr><w:r>");
    change(&mut entries,"word/document.xml","</w:p><w:sectPr>","</w:p><w:p><w:r><w:t>Second section</w:t></w:r></w:p><w:sectPr>");
    let parsed=import(&entries).unwrap();assert_eq!(parsed.sections.len(),2);
    assert_eq!(parsed.sections[0].body_range,0..1);assert_eq!(parsed.sections[1].body_range,1..2);
    assert_eq!(parsed.sections[0].footnote.placement,Some(NotePlacement::PageBottom));assert_eq!(parsed.sections[1].footnote.placement,Some(NotePlacement::BeneathText));
    assert_eq!(parsed.sections[0].footnote.number_format.as_deref(),Some("upperRoman"));
    assert_eq!(parsed.policies.policy(NoteKind::Endnote).placement,Some(NotePlacement::DocEnd));assert!(parsed.sections[0].endnote.placement.is_none());
    assert_eq!(parsed.require_global_only().unwrap_err().code,UNSUPPORTED);
    assert_eq!(parsed.sections[1].to_value()["kinds"]["footnote"]["placement"],"beneathText");
}

#[test]
fn mixed_paragraph_metrics_and_keep_next_remain_source_local_patches(){
    let mut entries=fixture();
    change(&mut entries,"word/styles.xml","<w:spacing w:line=\"285\" w:lineRule=\"exact\"/>","<w:spacing w:line=\"285\" w:lineRule=\"exact\"/><w:keepLines/>");
    change(&mut entries,"word/footnotes.xml","<w:t>Foot body</w:t></w:r></w:p>","<w:t>Foot body</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val=\"FootnoteText\"/><w:spacing w:line=\"305\" w:lineRule=\"atLeast\"/><w:keepLines w:val=\"0\"/><w:keepNext/><w:widowControl w:val=\"false\"/></w:pPr><w:r><w:rPr><w:sz w:val=\"40\"/></w:rPr><w:t>Second note paragraph</w:t></w:r></w:p>");
    let parsed=import(&entries).unwrap();assert_eq!(parsed.policies.policy(NoteKind::Footnote).long_note,Some(LongNote::KeepParagraphs));
    let local=parsed.local.iter().find(|p|p.kind==NoteKind::Footnote).unwrap();
    assert_eq!(local.native_id,7);assert_eq!(local.label,"word-footnote-7");assert_eq!(local.paragraph_path,vec![1]);
    assert_eq!(local.style["text"]["size"].as_f64(),Some(20.));assert_eq!(local.style["paragraph"]["lineSpacing"],json!({"mode":"atLeast","pt":15.25}));
    assert_eq!(local.style["paragraph"]["keepLines"],false);assert_eq!(local.style["paragraph"]["keepNext"],true);assert_eq!(local.style["paragraph"]["widowControl"],false);
    assert_eq!(parsed.require_global_only().unwrap_err().code,UNSUPPORTED);
}

#[test]
fn styles_are_cached_once_and_repeated_references_do_not_multiply_note_work(){
    let mut entries=fixture();let mut body=String::new();let mut definitions=specials_xml("footnote");
    for id in 1..=80{
        body.push_str(&format!("<w:r><w:footnoteReference w:id=\"{id}\"/><w:footnoteReference w:id=\"{id}\"/></w:r>"));
        definitions.push_str(&format!("<w:footnote w:id=\"{id}\"><w:p><w:pPr><w:pStyle w:val=\"FootnoteText\"/></w:pPr><w:r><w:t>First</w:t></w:r><w:r><w:t> second</w:t></w:r></w:p></w:footnote>"));
    }
    entries.iter_mut().find(|(name,_)|name=="word/document.xml").unwrap().1=format!("<w:document xmlns:w=\"{W}\"><w:body><w:p>{body}</w:p><w:sectPr/></w:body></w:document>").into_bytes();
    entries.iter_mut().find(|(name,_)|name=="word/footnotes.xml").unwrap().1=format!("<w:footnotes xmlns:w=\"{W}\">{definitions}</w:footnotes>").into_bytes();
    // Keep special IDs distinct from the 80 ordinary IDs.
    for part in ["word/footnotes.xml","word/settings.xml"]{for id in [41,42,43]{let value=String::from_utf8(entries.iter().find(|(name,_)|name==part).unwrap().1.clone()).unwrap();let old=format!("w:id=\"{id}\"");let new=format!("w:id=\"{}\"",id+1000);entries.iter_mut().find(|(name,_)|name==part).unwrap().1=value.replacen(&old,&new,1).into_bytes();}}
    let parsed=import(&entries).unwrap();assert_eq!(parsed.stats.paragraphs,80);assert_eq!(parsed.stats.runs,160);assert_eq!(parsed.stats.resolved_styles,3);
    assert_eq!(parsed.stats.borrowed_note_parts,1);assert_eq!(parsed.stats.parsed_parts,3,"settings/styles and one unused endnote story, each once");
    assert_eq!(parsed.policies.kind_by_label.len(),80);
}

#[test]
fn verified_suppressed_citation_note_does_not_leave_a_dangling_author_binding(){
    let entries=fixture();let root=docx_notes::xml(&entries.iter().find(|(n,_)|n=="word/document.xml").unwrap().1).unwrap();let body=root.children.iter().find(|n|n.name=="w:body").unwrap();
    let rels=docx_notes::relationships(&entries,"word/document.xml").unwrap();let notes=docx_notes::Notes::load(&entries,body,&rels).unwrap();
    // The actual citation verifier only suppresses singly referenced notes.
    let mut entries=entries.clone();change(&mut entries,"word/document.xml","<w:footnoteReference w:id=\"7\"/>","");
    let root=docx_notes::xml(&entries.iter().find(|(n,_)|n=="word/document.xml").unwrap().1).unwrap();let body=root.children.iter().find(|n|n.name=="w:body").unwrap();let notes_single=docx_notes::Notes::load(&entries,body,&rels).unwrap();
    assert!(notes.suppress_generated(false,7).is_err());notes_single.suppress_generated(false,7).unwrap();
    let parsed=parse(&entries,body,&rels,&notes_single).unwrap();assert!(!parsed.policies.kind_by_label.contains_key("word-footnote-7"));assert_eq!(parsed.policies.kind_by_label.get("word-endnote-7"),Some(&NoteKind::Endnote));
    assert_eq!(parsed.policies.policy(NoteKind::Footnote).size,Some(8.5));
}

#[test]
fn mixed_suppressed_generated_endnote_refuses_without_guessing_source_kind(){
    let entries=fixture();let before=entries.clone();
    let root=docx_notes::xml(&entries.iter().find(|(n,_)|n=="word/document.xml").unwrap().1).unwrap();let body=root.children.iter().find(|n|n.name=="w:body").unwrap();
    let rels=docx_notes::relationships(&entries,"word/document.xml").unwrap();let notes=docx_notes::Notes::load(&entries,body,&rels).unwrap();notes.suppress_generated(true,7).unwrap();
    let error=parse(&entries,body,&rels,&notes).unwrap_err();assert_eq!(error.code,UNSUPPORTED);assert_eq!(error.part,"word/endnotes.xml");assert!(error.path.contains("[@w:id='7']"));assert!(error.detail.contains("source citation kind association"));assert_eq!(entries,before);
}

#[test]
fn selected_complex_specials_foreign_properties_and_unknown_enums_refuse_without_rewriting(){
    let original=fixture();
    for(part,from,to,code)in[
        ("word/footnotes.xml","<w:t>Current native notice</w:t>","<w:rPr><w:b/></w:rPr><w:t>Current native notice</w:t>",UNSUPPORTED),
        ("word/settings.xml","<w:numFmt w:val=\"lowerLetter\"/>","<q:numFmt xmlns:q=\"urn:foreign\" w:val=\"lowerLetter\"/>",UNSUPPORTED),
        ("word/settings.xml","w:val=\"lowerLetter\"","w:val=\"inventedFormat\"",INVALID),
        ("word/settings.xml","w:val=\"65535\"","w:val=\"65536\"",INVALID),
        ("word/_rels/document.xml.rels","Target=\"settings.xml\"","Target=\"https://host.invalid/settings.xml\" TargetMode=\"External\"",INVALID),
    ]{
        let mut entries=original.clone();change(&mut entries,part,from,to);let before=entries.clone();let failure=import(&entries).unwrap_err();assert_eq!(failure.code,code,"{failure}");assert!(!failure.path.is_empty());assert_eq!(entries,before);
    }
    let mut chicago=original.clone();change(&mut chicago,"word/settings.xml","w:val=\"lowerLetter\"","w:val=\"chicago\"");assert_eq!(import(&chicago).unwrap().policies.policy(NoteKind::Footnote).number_format.as_deref(),Some("chicago"));
}

#[test]
fn style_cycles_and_selected_special_ambiguity_fail_deterministically(){
    let mut cycle=fixture();change(&mut cycle,"word/styles.xml","w:styleId=\"NoteBase\">","w:styleId=\"NoteBase\"><w:basedOn w:val=\"FootnoteText\"/>");assert_eq!(import(&cycle).unwrap_err().code,INVALID);
    let mut duplicate=fixture();change(&mut duplicate,"word/settings.xml","<w:footnote w:id=\"42\"/>","<w:footnote w:id=\"41\"/>");assert_eq!(import(&duplicate).unwrap_err().code,INVALID);
    let mut missing=fixture();change(&mut missing,"word/settings.xml","<w:footnote w:id=\"43\"/>","<w:footnote w:id=\"999\"/>");assert_eq!(import(&missing).unwrap_err().code,INVALID);
}

#[test]
fn plain_docx_builder_defaults_do_not_materialize_a_source_note_profile(){
    let doc=crate::md_ast::parse("Ordinary text without any notes.");
    let built=crate::docx_writer::build(&doc,&crate::export_styles::sanitize(&json!({})),"plain.md",&|_|None);
    let imported=import(&built.parts).unwrap();assert_eq!(imported.to_options(),json!({}));assert!(imported.sections.is_empty());assert!(imported.local.is_empty());
}

#[test]
fn actual_conversion_attaches_local_properties_to_native_nodes_with_repeated_text(){
    let mut entries=fixture();
    change(&mut entries,"word/footnotes.xml","<w:pStyle w:val=\"FootnoteText\"/>","<w:pStyle w:val=\"FootnoteText\"/><w:keepNext/>");
    change(&mut entries,"word/endnotes.xml","<w:pStyle w:val=\"EndnoteText\"/>","<w:pStyle w:val=\"EndnoteText\"/><w:keepNext w:val=\"false\"/>");
    change(&mut entries,"word/footnotes.xml","Foot body","Identical author text");change(&mut entries,"word/endnotes.xml","End body","Identical author text");
    let settings=import(&entries).unwrap();assert_eq!(settings.local.len(),2);settings.require_no_sections().unwrap();
    let source=super::super::docx_convert(&entries,std::path::Path::new("native-notes.docx"),false).unwrap();
    assert!(source.contains("[^word-footnote-7]"));assert!(source.contains("[^word-endnote-7]"));
    let model=crate::publication_blocks::analyze(&source).unwrap();
    let styled=model.nodes.iter().filter(|n|n.style["paragraph"].get("keepNext").is_some()).collect::<Vec<_>>();
    assert_eq!(styled.len(),2);assert_eq!(styled.iter().filter(|n|n.style["paragraph"]["keepNext"]==true).count(),1);assert_eq!(styled.iter().filter(|n|n.style["paragraph"]["keepNext"]==false).count(),1);
    for node in &styled{assert!(node.id.len()<=80);assert!(node.title.contains("Identical author text"));}
    let again=super::super::docx_convert(&entries,std::path::Path::new("native-notes.docx"),false).unwrap();assert_eq!(source,again);
}

#[test]
fn missing_direct_style_uses_native_defaults_but_broken_inheritance_still_fails(){
    let mut entries=fixture();change(&mut entries,"word/footnotes.xml","w:val=\"FootnoteText\"","w:val=\"MissingParagraph\"");
    let parsed=import(&entries).unwrap();assert_eq!(parsed.policies.policy(NoteKind::Footnote).size,Some(11.));assert_eq!(parsed.policies.policy(NoteKind::Footnote).size_cs,Some(11.));
    let mut broken=fixture();change(&mut broken,"word/styles.xml","<w:basedOn w:val=\"NoteBase\"/>","<w:basedOn w:val=\"MissingBase\"/>");
    let error=import(&broken).unwrap_err();assert_eq!(error.code,INVALID);assert_eq!(error.path,"MissingBase");
}

#[test]
fn structural_code_label_and_table_fonts_do_not_redefine_the_note_body_template(){
    let mut entries=fixture();
    change(&mut entries,"word/styles.xml","</w:styles>","<w:style w:type=\"paragraph\" w:styleId=\"Code\"><w:name w:val=\"Code\"/><w:rPr><w:sz w:val=\"16\"/><w:szCs w:val=\"16\"/></w:rPr></w:style><w:style w:type=\"paragraph\" w:styleId=\"TableText\"><w:rPr><w:sz w:val=\"28\"/><w:szCs w:val=\"28\"/></w:rPr></w:style></w:styles>");
    let structures="<w:p><w:pPr><w:pStyle w:val=\"Code\"/></w:pPr><w:r><w:rPr><w:color w:val=\"8A8F98\"/><w:sz w:val=\"15\"/></w:rPr><w:t>rust</w:t></w:r><w:r><w:br/></w:r><w:r><w:t>let evidence = 98;</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:pPr><w:pStyle w:val=\"TableText\"/></w:pPr><w:r><w:t>Cell evidence</w:t></w:r></w:p></w:tc></w:tr></w:tbl>";
    change(&mut entries,"word/footnotes.xml","</w:footnote></w:footnotes>",&format!("{structures}</w:footnote></w:footnotes>"));
    let parsed=import(&entries).unwrap();assert_eq!(parsed.policies.policy(NoteKind::Footnote).size,Some(8.5));assert_eq!(parsed.policies.policy(NoteKind::Footnote).line_spacing,Some(LineSpacing::ExactPt(14.25)));
    assert!(parsed.local.is_empty(),"independent structural fonts are not per-paragraph note-template overrides");
    let mut changed=entries.clone();change(&mut changed,"word/footnotes.xml","w:val=\"8A8F98\"","w:val=\"112233\"");
    let error=import(&changed).unwrap_err();assert_eq!(error.code,UNSUPPORTED);assert!(error.detail.contains("mixed run font sizes"));
}

#[test]
fn genuinely_mixed_author_run_sizes_are_not_exempted_by_the_code_label_rule(){
    let mut entries=fixture();change(&mut entries,"word/footnotes.xml","<w:t>Foot body</w:t></w:r>","<w:t>Foot body</w:t></w:r><w:r><w:rPr><w:sz w:val=\"40\"/></w:rPr><w:t>Changed author size</w:t></w:r>");
    let error=import(&entries).unwrap_err();assert_eq!(error.code,UNSUPPORTED);assert!(error.detail.contains("mixed run font sizes"));
}

#[test]
fn default_normal_widow_true_is_sparse_while_note_and_direct_overrides_remain_local(){
    let mut entries=fixture();
    change(&mut entries,"word/styles.xml","<w:style w:type=\"paragraph\" w:styleId=\"NoteBase\">","<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:pPr><w:widowControl/></w:pPr></w:style><w:style w:type=\"paragraph\" w:styleId=\"NoteBase\"><w:basedOn w:val=\"Normal\"/>");
    assert!(import(&entries).unwrap().local.is_empty());
    let mut disabled=entries.clone();change(&mut disabled,"word/styles.xml","<w:widowControl/>","<w:widowControl w:val=\"false\"/>");
    let parsed=import(&disabled).unwrap();assert_eq!(parsed.local.len(),2);assert!(parsed.local.iter().all(|p|p.style["paragraph"]["widowControl"]==false));
    let mut specific=entries.clone();change(&mut specific,"word/styles.xml","<w:spacing w:line=\"241\" w:lineRule=\"auto\"/>","<w:spacing w:line=\"241\" w:lineRule=\"auto\"/><w:widowControl/>");
    let parsed=import(&specific).unwrap();assert_eq!(parsed.local.len(),2);assert!(parsed.local.iter().all(|p|p.style["paragraph"]["widowControl"]==true));
    let mut direct=entries.clone();change(&mut direct,"word/footnotes.xml","<w:pStyle w:val=\"FootnoteText\"/>","<w:pStyle w:val=\"FootnoteText\"/><w:widowControl/>");
    let parsed=import(&direct).unwrap();assert_eq!(parsed.local.len(),1);assert_eq!(parsed.local[0].kind,NoteKind::Footnote);assert_eq!(parsed.local[0].style["paragraph"]["widowControl"],true);
}

#[test]
fn equal_local_script_sizes_use_one_existing_size_field_but_distinct_cs_stays_guarded(){
    let mut entries=fixture();
    change(&mut entries,"word/styles.xml","</w:rPrDefault></w:docDefaults>","</w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:line=\"384\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>");
    change(&mut entries,"word/footnotes.xml","</w:footnote></w:footnotes>","<w:p><w:r><w:t>Added author paragraph</w:t></w:r></w:p></w:footnote></w:footnotes>");
    let parsed=import(&entries).unwrap();let local=parsed.local.iter().find(|p|p.kind==NoteKind::Footnote&&p.paragraph_path==vec![1]).unwrap();
    assert_eq!(local.style["text"]["size"].as_f64(),Some(11.));assert!(local.style["text"].get("sizeCs").is_none());assert_eq!(local.style["paragraph"]["lineSpacing"],json!({"mode":"multiple","multiple":1.6}));
    crate::publication_blocks::validate_style(&local.style).unwrap();
    let source=super::super::docx_convert(&entries,std::path::Path::new("added-author-note.docx"),false).unwrap();assert!(source.contains("Added author paragraph"));
    let resolved=crate::publication_profile::resolve(&source,&json!({})).unwrap();let again=crate::docx_writer::build(&crate::md_ast::parse(&source),resolved.values(),"again.md",&|_|None);assert!(again.warns.is_empty(),"{:?}",again.warns);
    let xml=std::str::from_utf8(&again.parts.iter().find(|(n,_)|n=="word/footnotes.xml").unwrap().1).unwrap();assert!(xml.contains("Added author paragraph"));assert!(xml.contains("<w:sz w:val=\"22\"/>")&&xml.contains("<w:szCs w:val=\"22\"/>"));
    let mut distinct=entries.clone();change(&mut distinct,"word/footnotes.xml","<w:r><w:t>Added author paragraph</w:t>","<w:r><w:rPr><w:szCs w:val=\"24\"/></w:rPr><w:t>Added author paragraph</w:t>");let before=distinct.clone();
    let parsed=import(&distinct).unwrap();assert_eq!(parsed.local.iter().find(|p|p.paragraph_path==vec![1]).unwrap().style["text"]["sizeCs"].as_f64(),Some(12.));
    assert!(super::super::docx_convert(&distinct,std::path::Path::new("distinct-local-cs.docx"),false).unwrap_err().starts_with(UNSUPPORTED));assert_eq!(distinct,before);
}

#[test]
fn actual_builder_table_spacer_does_not_create_a_phantom_local_block_or_hide_explicit_flow(){
    let source="Body[^n].\n\n[^n]: Author paragraph.\n\n    | Item | Value |\n    |---|---|\n    | Matrix | 98 |\n\n    ```rust\n    let evidence = 98;\n    ```\n\n    $x^2$\n";
    let built=crate::docx_writer::build(&crate::md_ast::parse(source),&json!({}),"spacer.md",&|_|None);assert!(built.warns.is_empty());
    let (part,bytes)=built.parts.iter().find(|(n,_)|n=="word/footnotes.xml").unwrap();let root=docx_notes::xml(bytes).unwrap();let normal=root.children.iter().find(|n|n.name=="w:footnote"&&n.attr("w:type").is_none()).unwrap();
    let table=normal.children.iter().position(|n|n.name=="w:tbl").unwrap();let spacer=table+1;assert_eq!(normal.children[spacer].name,"w:p");assert!(!paragraph_has_author_content(&normal.children[spacer]));
    let parsed=import(&built.parts).unwrap();assert_eq!(parsed.policies.policy(NoteKind::Footnote).line_spacing,Some(LineSpacing::Multiple(1.2)));assert!(!parsed.local.iter().any(|p|p.paragraph_path==vec![spacer]));
    let converted=super::super::docx_convert(&built.parts,std::path::Path::new("spacer.docx"),false).unwrap();for text in ["Author paragraph","Matrix","let evidence = 98;","x"]{assert!(converted.contains(text),"{converted}");}
    let defs=crate::md_ast::parse(&converted).blocks.into_iter().find_map(|b|match b{crate::md_ast::Block::FootnoteDef{blocks,..}=>Some(blocks),_=>None}).unwrap();assert!(defs.iter().any(|b|matches!(b.unstyled(),crate::md_ast::Block::Table{..})));assert!(defs.iter().any(|b|matches!(b.unstyled(),crate::md_ast::Block::Code{lang,text}if lang=="rust"&&text.trim_end()=="let evidence = 98;")));
    let mut explicit=built.parts.clone();let mut edited=root.clone();let normal=edited.children.iter_mut().find(|n|n.name=="w:footnote"&&n.attr("w:type").is_none()).unwrap();let pr=normal.children[spacer].children.iter_mut().find(|n|n.name=="w:pPr").unwrap();pr.children.push(docx_notes::xml(format!("<w:keepNext xmlns:w=\"{W}\"/>").as_bytes()).unwrap());
    explicit.iter_mut().find(|(name,_)|name==part).unwrap().1=super::super::docx_xml_fragment(&edited,&[]).into_bytes();let before=explicit.clone();let parsed=import(&explicit).unwrap();assert_eq!(parsed.local.iter().find(|p|p.paragraph_path==vec![spacer]).unwrap().style["paragraph"]["keepNext"],true);
    assert!(super::super::docx_convert(&explicit,std::path::Path::new("spacer-flow.docx"),false).unwrap_err().starts_with(UNSUPPORTED));assert_eq!(explicit,before,"an unrepresentable explicit empty-paragraph override still protects the original input");
}

#[test]
fn actual_section_restart_restores_native_scopes_source_ids_and_section_links(){
    let source="<!-- readmd-section:v1 {\"id\":\"notes-first\"} -->\n\n# Same title\n\nFirst[^a], second[^b], [Jump](#readmd-section-notes-second).\n\n<!-- readmd-section:v1 {\"id\":\"notes-second\",\"break\":\"continuous\"} -->\n\n# Same title\n\nLater[^c], repeated old[^a].\n\n[^a]: First evidence.\n\n[^b]: Second evidence.\n\n[^c]: Later evidence.\n";
    let options=json!({"frontMatter":{"mode":"none"},"footnotes":{"size":9,"lineHeight":1.2,"numberFormat":"upperLetter","start":27,"restart":"section"}});
    let built=crate::docx_writer::build(&crate::md_ast::parse(source),&options,"section-notes.md",&|_|None);assert!(built.warns.is_empty());let settings=import(&built.parts).unwrap();assert_eq!(settings.sections.len(),2);assert!(settings.sections.iter().all(|s|s.footnote.is_empty()&&s.endnote.is_empty()));
    let root=docx_notes::xml(&built.parts.iter().find(|(n,_)|n=="word/document.xml").unwrap().1).unwrap();let body=root.children.iter().find(|n|n.name=="w:body").unwrap();let plan=settings.plan_source_sections(body).unwrap();
    assert_eq!(plan.declarations.iter().map(|d|d.section.id.as_str()).collect::<Vec<_>>(),vec!["notes-first","notes-second"]);assert_eq!(plan.declarations[0].section.break_kind,crate::publication_sections::SectionBreak::NextPage);assert_eq!(plan.declarations[1].section.break_kind,crate::publication_sections::SectionBreak::Continuous);
    let children=super::super::docx_word_children(body);for declaration in &plan.declarations{
        let bookmark=crate::docx_writer::section_bookmark_name(&declaration.section.id);let child=children.iter().find(|node|node.descendants().iter().any(|n|n.name=="w:bookmarkStart"&&n.attr("w:name")==Some(bookmark.as_str()))).unwrap();assert_eq!(declaration.before_node,Some(*child as *const XmlNode as usize));
    }
    let second_bookmark=crate::docx_writer::section_bookmark_name("notes-second");assert_eq!(plan.anchors[&second_bookmark],"readmd-section-notes-second");assert_eq!(source_section_anchors(body).unwrap(),plan.anchors);
    let converted=super::super::docx_convert(&built.parts,std::path::Path::new("section-notes.docx"),false).unwrap();let declarations=crate::publication_sections::analyze(&converted).unwrap();assert_eq!(declarations.iter().map(|d|d.section.id.as_str()).collect::<Vec<_>>(),vec!["notes-first","notes-second"]);
    let doc=crate::md_ast::parse(&converted);let mut links=Vec::new();crate::md_ast::walk_inlines(&doc.blocks,&mut|inline|if let crate::md_ast::Inline::Link{href,children,..}=inline{links.push((href.clone(),crate::md_ast::inline_plain(children)));});assert_eq!(links,vec![("#readmd-section-notes-second".into(),"Jump".into())],"section identity must survive either legal Markdown link serialization: {converted}");
    let resolved=crate::publication_profile::resolve(&converted,&json!({})).unwrap();let flat=crate::publication_note_policies::project_legacy(&doc,resolved.values()).unwrap();let numbers=crate::publication_layout::note_numbers(&doc,&flat);assert_eq!(numbers["word-footnote-1"],27);assert_eq!(numbers["word-footnote-2"],28);assert_eq!(numbers["word-footnote-3"],27,"the actual second native section must restart independently of the duplicate heading text");
    let again=crate::docx_writer::build(&doc,resolved.values(),"again.md",&|_|None);assert!(again.warns.is_empty(),"{:?}",again.warns);let xml=std::str::from_utf8(&again.parts.iter().find(|(n,_)|n=="word/document.xml").unwrap().1).unwrap();assert_eq!(xml.matches("<w:sectPr>").count(),2);assert!(xml.contains("<w:type w:val=\"continuous\"/>"));assert!(xml.contains(&format!("w:anchor=\"{second_bookmark}\"")));
}

#[test]
fn cleared_inactive_note_defaults_keep_code_empty_sections_and_live_section_links(){
    let source="<!-- readmd-section:v1 {\"id\":\"notes-first\"} -->\n\n# Same title\n\n[Jump](#readmd-section-empty-middle), [End](#readmd-section-notes-last).\n\n```rust\nlet native = 42;\n```\n\n<!-- readmd-section:v1 {\"id\":\"empty-middle\",\"break\":\"continuous\"} -->\n\n<!-- readmd-section:v1 {\"id\":\"notes-last\"} -->\n\n# Same title\n\nLast section.\n";
    let built=crate::docx_writer::build(&crate::md_ast::parse(source),&json!({"frontMatter":{"mode":"none"}}),"no-notes.md",&|_|None);assert!(built.warns.is_empty(),"{:?}",built.warns);
    let mut entries=built.parts;let settings=entries.iter_mut().find(|(name,_)|name=="word/settings.xml").unwrap();let mut root=docx_notes::xml(&settings.1).unwrap();let before=root.children.len();root.children.retain(|node|!matches!(node.name.as_str(),"w:footnotePr"|"w:endnotePr"));assert!(root.children.len()<before,"the regression must clear actual writer defaults");settings.1=super::super::docx_xml_fragment(&root,&[]).into_bytes();
    let original=entries.clone();let imported=import(&entries).unwrap();assert_eq!(imported.to_options(),json!({}));assert_eq!(imported.sections.len(),3);assert!(imported.local.is_empty());
    let mut converted=super::super::docx_convert(&entries,std::path::Path::new("no-notes-cleared.docx"),false).unwrap();assert_eq!(entries,original,"clearing inactive settings never authorizes mutating the input");
    for pass in 0..2{
        let declarations=crate::publication_sections::analyze(&converted).unwrap();assert_eq!(declarations.iter().map(|d|d.section.id.as_str()).collect::<Vec<_>>(),vec!["notes-first","empty-middle","notes-last"]);assert_eq!(declarations[1].section.break_kind,crate::publication_sections::SectionBreak::Continuous);
        let doc=crate::md_ast::parse(&converted);let mut links=Vec::new();crate::md_ast::walk_inlines(&doc.blocks,&mut|inline|if let crate::md_ast::Inline::Link{href,children,..}=inline{links.push((href.clone(),crate::md_ast::inline_plain(children)));});assert_eq!(links,vec![("#readmd-section-empty-middle".into(),"Jump".into()),("#readmd-section-notes-last".into(),"End".into())],"pass {pass}: {converted}");
        let resolved=crate::publication_profile::resolve(&converted,&json!({})).unwrap();assert!(resolved.values().get("footnotes").is_none(),"inactive note defaults must not become source-owned metadata");let sections=crate::publication_sections::resolve(&doc,resolved.values()).unwrap();assert_eq!(sections.len(),3);assert!(sections[1].block_range.is_empty(),"the empty native section must remain empty");
        let code=doc.blocks.iter().enumerate().filter_map(|(index,block)|match block.unstyled(){crate::md_ast::Block::Code{lang,text}=>Some((lang.clone(),text.clone(),sections.iter().find(|section|section.block_range.contains(&index)).map(|section|section.id.clone()))),_=>None}).collect::<Vec<_>>();assert_eq!(code,vec![("rust".into(),"let native = 42;".into(),Some("notes-first".into()))],"the exact code and its native source section must survive pass {pass}: {converted}");
        let again=crate::docx_writer::build(&doc,resolved.values(),"no-notes-again.md",&|_|None);assert!(again.warns.is_empty(),"{:?}",again.warns);let document=std::str::from_utf8(&again.parts.iter().find(|(name,_)|name=="word/document.xml").unwrap().1).unwrap();for id in ["empty-middle","notes-last"]{let name=crate::docx_writer::section_bookmark_name(id);assert!(document.contains(&format!("w:name=\"{name}\"")),"missing live native destination {id}");assert!(document.contains(&format!("w:anchor=\"{name}\"")),"link must use that exact native destination {id}");}
        converted=super::super::docx_convert(&again.parts,std::path::Path::new("no-notes-again.docx"),false).unwrap();
    }
}

#[test]
fn equivalent_native_section_numbering_keeps_explicit_presence_and_accepts_real_overrides(){
    let mut entries=fixture();change(&mut entries,"word/document.xml","</w:p><w:sectPr>","</w:p><w:p><w:pPr><w:sectPr><w:footnotePr><w:pos w:val=\"beneathText\"/><w:numFmt w:val=\"lowerLetter\"/><w:numStart w:val=\"0\"/><w:numRestart w:val=\"eachPage\"/></w:footnotePr></w:sectPr></w:pPr></w:p><w:p><w:r><w:t>Actual second section</w:t></w:r></w:p><w:sectPr>");
    let settings=import(&entries).unwrap();assert_eq!(settings.sections.len(),2);assert_eq!(settings.sections[0].footnote.number_format.as_deref(),Some("lowerLetter"));assert_eq!(settings.sections[0].footnote.start,Some(0));assert_eq!(settings.sections[0].footnote.restart,Some(NoteRestart::Page));assert!(settings.sections[1].footnote.is_empty());assert_eq!(settings.policies.policy(NoteKind::Footnote).start,Some(0));assert_eq!(settings.policies.policy(NoteKind::Footnote).placement,Some(NotePlacement::BeneathText));
    let root=docx_notes::xml(&entries.iter().find(|(n,_)|n=="word/document.xml").unwrap().1).unwrap();let body=root.children.iter().find(|n|n.name=="w:body").unwrap();assert_eq!(settings.plan_source_sections(body).unwrap().declarations.len(),2);
    let mut different=entries.clone();change(&mut different,"word/document.xml","<w:numStart w:val=\"0\"/>","<w:numStart w:val=\"3\"/>");let original=different.clone();let settings=import(&different).unwrap();let root=docx_notes::xml(&different.iter().find(|(n,_)|n=="word/document.xml").unwrap().1).unwrap();let body=root.children.iter().find(|n|n.name=="w:body").unwrap();let plan=settings.plan_source_sections(body).unwrap();assert_eq!(plan.declarations[0].section.footnotes.as_ref().unwrap().footnote_numbering.start,Some(3));assert!(plan.declarations[1].section.footnotes.is_none());assert_eq!(settings.policies.policy(NoteKind::Footnote).start,Some(0));assert_eq!(different,original);
}

#[test]
fn truncated_section_identity_gets_a_short_recoverable_id_without_future_drift(){
    let id="A-very-long-original-section-identifier-beyond-the-native-stem";assert!(recover_section_id(&crate::docx_writer::section_bookmark_name(id)).is_none());
    let source=format!("<!-- readmd-section:v1 {{\"id\":\"{id}\"}} -->\n\nSame text[^a].\n\n[^a]: Evidence.\n");let built=crate::docx_writer::build(&crate::md_ast::parse(&source),&json!({"frontMatter":{"mode":"none"},"footnotes":{"restart":"continuous"}}),"long-section.md",&|_|None);assert!(built.warns.is_empty());
    let settings=import(&built.parts).unwrap();let root=docx_notes::xml(&built.parts.iter().find(|(n,_)|n=="word/document.xml").unwrap().1).unwrap();let body=root.children.iter().find(|n|n.name=="w:body").unwrap();let plan=settings.plan_source_sections(body).unwrap();assert_eq!(plan.declarations.len(),1);let generated=&plan.declarations[0].section.id;assert!(generated.len()<=11);assert_eq!(recover_section_id(&crate::docx_writer::section_bookmark_name(generated)).as_deref(),Some(generated.as_str()));
    let converted=super::super::docx_convert(&built.parts,std::path::Path::new("long-section.docx"),false).unwrap();let resolved=crate::publication_profile::resolve(&converted,&json!({})).unwrap();let again=crate::docx_writer::build(&crate::md_ast::parse(&converted),resolved.values(),"again.md",&|_|None);assert!(again.warns.is_empty());let twice=super::super::docx_convert(&again.parts,std::path::Path::new("long-section-again.docx"),false).unwrap();assert_eq!(crate::publication_sections::analyze(&twice).unwrap()[0].section.id,*generated);
}

fn native_position_sections()->Vec<(String,Vec<u8>)>{
    let mut entries=fixture();let first=crate::docx_writer::section_bookmark_name("native-first");let second=crate::docx_writer::section_bookmark_name("native-last");
    let document=format!("<w:document xmlns:w=\"{W}\"><w:body><w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:bookmarkStart w:id=\"1\" w:name=\"{first}\"/><w:r><w:t>Same title</w:t></w:r><w:bookmarkEnd w:id=\"1\"/></w:p><w:p><w:r><w:t>First</w:t><w:footnoteReference w:id=\"7\"/><w:endnoteReference w:id=\"7\"/><w:footnoteReference w:id=\"7\"/></w:r><w:hyperlink w:anchor=\"{second}\"><w:r><w:t>Jump</w:t></w:r></w:hyperlink></w:p><w:p><w:pPr><w:sectPr><w:footnotePr><w:pos w:val=\"beneathText\"/></w:footnotePr><w:endnotePr><w:pos w:val=\"sectEnd\"/></w:endnotePr><w:type w:val=\"nextPage\"/></w:sectPr></w:pPr></w:p><w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:bookmarkStart w:id=\"2\" w:name=\"{second}\"/><w:r><w:t>Same title</w:t></w:r><w:bookmarkEnd w:id=\"2\"/></w:p><w:p><w:r><w:t>Later</w:t><w:footnoteReference w:id=\"7\"/><w:endnoteReference w:id=\"7\"/></w:r></w:p><w:sectPr><w:footnotePr><w:pos w:val=\"pageBottom\"/></w:footnotePr><w:type w:val=\"continuous\"/></w:sectPr></w:body></w:document>");
    entries.iter_mut().find(|(name,_)|name=="word/document.xml").unwrap().1=document.into_bytes();entries
}
fn native_section_positions(entries:&[(String,Vec<u8>)])->Vec<NotePlacement>{
    let root=docx_notes::xml(&entries.iter().find(|(name,_)|name=="word/document.xml").unwrap().1).unwrap();root.descendants().iter().filter(|node|node.name=="w:sectPr").map(|node|match node.children.iter().find(|node|node.name=="w:footnotePr").and_then(|node|node.children.iter().find(|node|node.name=="w:pos")).and_then(|node|node.attr("w:val")){Some("beneathText")=>NotePlacement::BeneathText,None|Some("pageBottom")=>NotePlacement::PageBottom,other=>panic!("unexpected native placement {other:?}")}).collect()
}

#[test]
fn native_different_section_positions_restore_current_values_and_preserve_mixed_note_identities(){
    let entries=native_position_sections();let before=entries.clone();assert!(!entries.iter().any(|(name,_)|name==crate::convert::docx_numbering_import::SOURCE_PART));let imported=import(&entries).unwrap();
    assert_eq!(imported.sections.len(),2);assert_eq!(imported.policies.policy(NoteKind::Footnote).placement,Some(NotePlacement::PageBottom));assert_eq!(imported.policies.policy(NoteKind::Endnote).placement,Some(NotePlacement::DocEnd));assert!(imported.sections[0].endnote.placement.is_none());
    let root=docx_notes::xml(&entries.iter().find(|(name,_)|name=="word/document.xml").unwrap().1).unwrap();let body=root.children.iter().find(|node|node.name=="w:body").unwrap();let plan=imported.plan_source_sections(body).unwrap();assert_eq!(plan.declarations.iter().map(|declaration|declaration.section.footnotes.as_ref().unwrap().footnote_placement).collect::<Vec<_>>(),vec![Some(NotePlacement::BeneathText),Some(NotePlacement::PageBottom)]);assert_eq!(plan.declarations.iter().map(|declaration|declaration.section.id.as_str()).collect::<Vec<_>>(),vec!["native-first","native-last"]);
    let mut source=super::super::docx_convert(&entries,std::path::Path::new("native-position.docx"),true).unwrap();assert_eq!(entries,before);
    for pass in 0..2{
        let declarations=crate::publication_sections::analyze(&source).unwrap();assert_eq!(declarations.iter().map(|declaration|declaration.section.id.as_str()).collect::<Vec<_>>(),vec!["native-first","native-last"]);assert_eq!(declarations.iter().map(|declaration|declaration.section.footnotes.as_ref().unwrap().footnote_placement).collect::<Vec<_>>(),vec![Some(NotePlacement::BeneathText),Some(NotePlacement::PageBottom)]);
        let doc=crate::md_ast::parse(&source);let mut jump=Vec::new();crate::md_ast::walk_inlines(&doc.blocks,&mut|inline|if let crate::md_ast::Inline::Link{href,children,..}=inline{if crate::md_ast::inline_plain(children)=="Jump"{jump.push(href.clone());}});assert_eq!(jump,vec!["#readmd-section-native-last"]);
        let mut references=Vec::new();crate::md_ast::walk_inlines(&doc.blocks,&mut|inline|if let crate::md_ast::Inline::FootnoteRef(label)=inline{references.push(label.clone());});assert_eq!(references.len(),5,"pass {pass}: references {references:?}; source {source}");
        let resolved=crate::publication_profile::resolve(&source,&json!({})).unwrap();let output=crate::docx_writer::build(&doc,resolved.values(),"positions-again.md",&|_|None);assert!(output.warns.is_empty(),"pass {pass}: {:?}",output.warns);assert_eq!(native_section_positions(&output.parts),vec![NotePlacement::BeneathText,NotePlacement::PageBottom]);
        let document=std::str::from_utf8(&output.parts.iter().find(|(name,_)|name=="word/document.xml").unwrap().1).unwrap();assert_eq!(document.matches("<w:footnoteReference w:id=\"1\"").count(),3);assert_eq!(document.matches("<w:endnoteReference w:id=\"1\"").count(),2);let second=crate::docx_writer::section_bookmark_name("native-last");assert!(document.contains(&format!("w:anchor=\"{second}\"")));assert!(document.contains(&format!("w:name=\"{second}\"")));
        let returned=import(&output.parts).unwrap();assert_eq!(returned.policies.policy(NoteKind::Endnote).placement,Some(NotePlacement::DocEnd));assert_eq!(returned.policies.kind_by_label.len(),2);assert_eq!(returned.policies.kind_by_label.values().copied().collect::<HashSet<_>>(),HashSet::from(NoteKind::ALL));source=super::super::docx_convert(&output.parts,std::path::Path::new("positions-again.docx"),true).unwrap();
    }
}

#[test]
fn current_native_position_changes_do_not_reuse_an_old_section_override(){
    let mut entries=native_position_sections();change(&mut entries,"word/document.xml","<w:pos w:val=\"beneathText\"/>","<w:pos w:val=\"pageBottom\"/>");let before=entries.clone();
    assert_eq!(native_section_positions(&entries),vec![NotePlacement::PageBottom,NotePlacement::PageBottom]);let source=super::super::docx_convert(&entries,std::path::Path::new("current-position.docx"),true).unwrap();let scopes=crate::publication_sections::analyze(&source).unwrap();assert_eq!(scopes.iter().map(|scope|scope.section.id.as_str()).collect::<Vec<_>>(),vec!["native-first","native-last"]);let doc=crate::md_ast::parse(&source);let options=crate::publication_profile::resolve(&source,&json!({})).unwrap();let output=crate::docx_writer::build(&doc,options.values(),"current-position.md",&|_|None);assert!(output.warns.is_empty(),"{:?}",output.warns);assert_eq!(native_section_positions(&output.parts),vec![NotePlacement::PageBottom,NotePlacement::PageBottom]);assert_eq!(entries,before);
    let mut partial=native_position_sections();change(&mut partial,"word/document.xml","<w:pos w:val=\"pageBottom\"/>","");let source=super::super::docx_convert(&partial,std::path::Path::new("omitted-position.docx"),true).unwrap();let doc=crate::md_ast::parse(&source);let options=crate::publication_profile::resolve(&source,&json!({})).unwrap();let output=crate::docx_writer::build(&doc,options.values(),"omitted-position.md",&|_|None);assert!(output.warns.is_empty());assert_eq!(native_section_positions(&output.parts),vec![NotePlacement::BeneathText,NotePlacement::PageBottom],"an omitted final native pos must use pageBottom instead of the preceding section's beneathText");
}

#[test]
fn unsupported_native_section_leaves_are_not_swallowed_by_placement_support(){
    for(field,xml)in[("sz","<w:sz w:val=\"20\"/>"),("spacing","<w:spacing w:line=\"240\"/>"),("separator","<w:separator/>")]{
        let mut entries=native_position_sections();change(&mut entries,"word/document.xml","<w:pos w:val=\"beneathText\"/>",&format!("<w:pos w:val=\"beneathText\"/>{xml}"));let before=entries.clone();let error=super::super::docx_convert(&entries,std::path::Path::new("unsupported-section.docx"),true).unwrap_err();assert!(error.starts_with(UNSUPPORTED),"{error}");assert!(error.contains(&format!("/w:footnotePr/w:{field}")),"{error}");assert_eq!(entries,before);
    }
    for xml in ["<w:suppressEndnotes/>","<w:suppressEndnotes w:val=\"false\"/>"]{let mut entries=native_position_sections();change(&mut entries,"word/document.xml","<w:type w:val=\"nextPage\"/>",&format!("{xml}<w:type w:val=\"nextPage\"/>"));let error=super::super::docx_convert(&entries,std::path::Path::new("suppressed-section.docx"),true).unwrap_err();assert!(error.starts_with(UNSUPPORTED)&&error.contains("/w:suppressEndnotes"),"{error}");}
}

#[test]
fn an_unused_explicit_native_section_position_is_readable_without_inventing_global_note_defaults(){
    let built=crate::docx_writer::build(&crate::md_ast::parse("Ordinary text, no note references."),&json!({"frontMatter":{"mode":"none"}}),"no-notes.md",&|_|None);assert!(built.warns.is_empty());let mut entries=built.parts;
    let settings=entries.iter_mut().find(|(name,_)|name=="word/settings.xml").unwrap();let mut root=docx_notes::xml(&settings.1).unwrap();root.children.retain(|node|!matches!(node.name.as_str(),"w:footnotePr"|"w:endnotePr"));settings.1=super::super::docx_xml_fragment(&root,&[]).into_bytes();change(&mut entries,"word/document.xml","<w:sectPr>","<w:sectPr><w:footnotePr><w:pos w:val=\"beneathText\"/></w:footnotePr>");
    let imported=import(&entries).unwrap();assert_eq!(imported.to_options(),json!({}));assert_eq!(imported.sections.len(),1);let source=super::super::docx_convert(&entries,std::path::Path::new("unused-position.docx"),true).unwrap();let profile=crate::publication_profile::from_source(&source).unwrap();
    // The body actually consumes its native 11pt baseline. Persist it so an
    // omitted MCP preset (minimal: 10.5pt) cannot change the reopened body;
    // the unused note placement must still invent no global note policy.
    assert_eq!(profile.options.as_object().unwrap().len(),1);assert_eq!(profile.options["typography"]["size"].as_f64(),Some(11.));assert_eq!(profile.options["typography"].as_object().unwrap().len(),1);assert!(profile.options.get("footnotes").is_none());assert!(source.contains("Ordinary text, no note references."));let declarations=crate::publication_sections::analyze(&source).unwrap();assert_eq!(declarations.len(),1);assert_eq!(declarations[0].section.footnotes.as_ref().unwrap().footnote_placement,Some(NotePlacement::BeneathText));
    let output=crate::docx_writer::build(&crate::md_ast::parse(&source),&json!({"frontMatter":{"mode":"none"}}),"unused-position.md",&|_|None);assert!(output.warns.is_empty());assert_eq!(native_section_positions(&output.parts),vec![NotePlacement::BeneathText]);
    let mut numbered=entries.clone();change(&mut numbered,"word/document.xml","<w:pos w:val=\"beneathText\"/>","<w:pos w:val=\"beneathText\"/><w:numStart w:val=\"3\"/>");let before=numbered.clone();let source=super::super::docx_convert(&numbered,std::path::Path::new("unused-numbered.docx"),true).unwrap();let sections=crate::publication_sections::analyze(&source).unwrap();assert_eq!(sections[0].section.footnotes.as_ref().unwrap().footnote_numbering.start,Some(3));assert_eq!(numbered,before);
}

// Inspect actual native leaves independently of parse_properties and the
// source-policy serializer. The snapshot records field presence, not merely
// effective equality, and checks the fixed schema's sequence as it reads.
fn section_numbering_snapshot(entries:&[(String,Vec<u8>)])->Vec<Value>{
    let document=docx_notes::xml(docx_notes::unique(entries,"word/document.xml").unwrap()).unwrap();
    document.descendants().into_iter().filter(|node|node.name=="w:sectPr").map(|section|{
        let mut kinds=Map::new();let mut prior_kind=0;
        for property in section.children.iter().filter(|node|matches!(node.name.as_str(),"w:footnotePr"|"w:endnotePr")){
            let (kind,rank)=if property.name=="w:footnotePr"{("footnote",1)}else{("endnote",2)};assert!(rank>prior_kind);prior_kind=rank;
            let mut leaves=Map::new();let mut prior_leaf=0;
            for leaf in &property.children{
                let order=match leaf.name.as_str(){"w:pos"=>1,"w:numFmt"=>2,"w:numStart"=>3,"w:numRestart"=>4,other=>panic!("unexpected section note child {other}")};
                assert!(order>prior_leaf,"native note child sequence at {}",leaf.name);prior_leaf=order;
                if order==1{continue;}
                let mut attrs=Map::new();attrs.insert("val".into(),json!(leaf.attr("w:val").expect("required native val")));
                if let Some(pattern)=leaf.attr("w:format"){attrs.insert("format".into(),json!(pattern));}
                leaves.insert(leaf.local.clone(),Value::Object(attrs));
            }
            if !leaves.is_empty(){kinds.insert(kind.into(),Value::Object(leaves));}
        }Value::Object(kinds)
    }).collect()
}

fn native_numbering_sections()->Vec<(String,Vec<u8>)>{
    let mut entries=fixture();
    for(from,to)in[("<w:numStart w:val=\"0\"/>","<w:numStart w:val=\"3\"/>"),("<w:numRestart w:val=\"eachPage\"/>","<w:numRestart w:val=\"continuous\"/>"),("<w:numFmt w:val=\"upperRoman\"/>","<w:numFmt w:val=\"lowerRoman\"/>"),("<w:numStart w:val=\"65535\"/>","<w:numStart w:val=\"1\"/>"),("<w:numRestart w:val=\"eachSect\"/>","<w:numRestart w:val=\"continuous\"/>")]{change(&mut entries,"word/settings.xml",from,to);}
    // The canonical Word name has only twelve source-id stem characters after
    // its fixed section prefix. Keep each intended identity fully present;
    // the focused boundary test below covers the intentionally truncated case.
    let first=crate::docx_writer::section_bookmark_name("native-first");let middle=crate::docx_writer::section_bookmark_name("native-mid");let last=crate::docx_writer::section_bookmark_name("native-last");
    let heading=|id:usize,name:&str|format!("<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:bookmarkStart w:id=\"{id}\" w:name=\"{name}\"/><w:r><w:t>Same title</w:t></w:r><w:bookmarkEnd w:id=\"{id}\"/></w:p>");
    let first_heading=heading(1,&first);let middle_heading=heading(2,&middle);let last_heading=heading(3,&last);
    let document=format!(concat!("<w:document xmlns:w=\"{W}\"><w:body>{first_heading}",
        "<w:p><w:r><w:t>First</w:t><w:footnoteReference w:id=\"7\"/><w:endnoteReference w:id=\"7\"/><w:footnoteReference w:id=\"7\"/></w:r><w:hyperlink w:anchor=\"{last}\"><w:r><w:t>Jump</w:t></w:r></w:hyperlink></w:p>",
        "<w:p><w:pPr><w:sectPr><w:footnotePr><w:numFmt w:val=\"upperRoman\"/><w:numStart w:val=\"7\"/><w:numRestart w:val=\"eachSect\"/></w:footnotePr><w:endnotePr><w:numFmt w:val=\"upperLetter\"/><w:numStart w:val=\"27\"/><w:numRestart w:val=\"eachSect\"/></w:endnotePr><w:type w:val=\"nextPage\"/></w:sectPr></w:pPr></w:p>{middle_heading}",
        "<w:p><w:r><w:t>Middle</w:t><w:footnoteReference w:id=\"11\"/><w:endnoteReference w:id=\"11\"/><w:footnoteReference w:id=\"7\"/><w:endnoteReference w:id=\"7\"/></w:r></w:p>",
        "<w:p><w:pPr><w:sectPr><w:footnotePr><w:numFmt w:val=\"decimal\"/><w:numStart w:val=\"0\"/><w:numRestart w:val=\"continuous\"/></w:footnotePr><w:endnotePr><w:numStart w:val=\"0\"/></w:endnotePr><w:type w:val=\"continuous\"/></w:sectPr></w:pPr></w:p>{last_heading}",
        "<w:p><w:r><w:t>Last</w:t><w:footnoteReference w:id=\"7\"/><w:endnoteReference w:id=\"7\"/></w:r></w:p><w:sectPr><w:type w:val=\"nextPage\"/></w:sectPr></w:body></w:document>"),W=W,first_heading=first_heading,middle_heading=middle_heading,last_heading=last_heading,last=last);
    entries.iter_mut().find(|(name,_)|name=="word/document.xml").unwrap().1=document.into_bytes();
    for(kind,style,body)in[("footnote","FootnoteText","Later foot evidence"),("endnote","EndnoteText","Later end evidence")]{change(&mut entries,&format!("word/{kind}s.xml"),&format!("</w:{kind}s>"),&format!("<w:{kind} w:id=\"11\"><w:p><w:pPr><w:pStyle w:val=\"{style}\"/></w:pPr><w:r><w:t>{body}</w:t></w:r></w:p></w:{kind}></w:{kind}s>"));}
    entries
}

#[test]
fn native_three_section_numbering_roundtrips_sparse_leaves_and_repeated_story_identities(){
    let entries=native_numbering_sections();let original=entries.clone();let expected=section_numbering_snapshot(&entries);assert_eq!(expected.len(),3);assert_eq!(expected[2],json!({}));
    assert_eq!(expected[0]["footnote"],json!({"numFmt":{"val":"upperRoman"},"numStart":{"val":"7"},"numRestart":{"val":"eachSect"}}));
    assert_eq!(expected[1]["endnote"],json!({"numStart":{"val":"0"}}));
    let imported=import(&entries).unwrap();assert_eq!(imported.policies.policy(NoteKind::Footnote).start,Some(3));assert_eq!(imported.policies.policy(NoteKind::Endnote).start,Some(1));assert_eq!(imported.policies.kind_by_label.len(),4);
    let root=docx_notes::xml(docx_notes::unique(&entries,"word/document.xml").unwrap()).unwrap();let body=root.children.iter().find(|node|node.name=="w:body").unwrap();let plan=imported.plan_source_sections(body).unwrap();assert_eq!(plan.declarations.iter().map(|declaration|declaration.section.id.as_str()).collect::<Vec<_>>(),vec!["native-first","native-mid","native-last"],"the native fixture must carry every intended canonical identity before conversion");
    let mut source=super::super::docx_convert(&entries,std::path::Path::new("native-numbering.docx"),false).unwrap();
    for pass in 0..2{
        let declarations=crate::publication_sections::analyze(&source).unwrap();assert_eq!(declarations.iter().map(|value|value.section.id.as_str()).collect::<Vec<_>>(),vec!["native-first","native-mid","native-last"]);
        let first=declarations[0].section.footnotes.as_ref().unwrap();assert_eq!(first.footnote_numbering.start,Some(7));assert_eq!(first.endnote_numbering.start,Some(27));
        let middle=declarations[1].section.footnotes.as_ref().unwrap();assert_eq!(middle.footnote_numbering.number_format.as_deref(),Some("decimal"));assert_eq!(middle.footnote_numbering.start,Some(0));assert_eq!(middle.footnote_numbering.restart,Some(NoteRestart::Continuous));assert_eq!(middle.endnote_numbering.start,Some(0));assert!(middle.endnote_numbering.number_format.is_none());assert!(declarations[2].section.footnotes.is_none());
        let options=crate::publication_profile::resolve(&source,&json!({})).unwrap();let doc=crate::md_ast::parse(&source);let scopes=crate::publication_sections::resolve(&doc,options.values()).unwrap();
        let policy=|index:usize,kind:NoteKind|policy::parse_layer(&scopes[index].style).unwrap().policy(kind);assert_eq!(policy(0,NoteKind::Footnote).start,Some(7));assert_eq!(policy(1,NoteKind::Footnote).start,Some(0));assert_eq!(policy(2,NoteKind::Footnote).start,Some(3));assert_eq!(policy(1,NoteKind::Endnote).number_format.as_deref(),Some("lowerRoman"));
        let output=crate::docx_writer::build(&doc,options.values(),"native-numbering.md",&|_|None);assert!(output.warns.is_empty(),"pass {pass}: {:?}",output.warns);assert_eq!(section_numbering_snapshot(&output.parts),expected);
        let native=std::str::from_utf8(docx_notes::unique(&output.parts,"word/document.xml").unwrap()).unwrap();assert_eq!(native.matches("<w:footnoteReference w:id=\"1\"").count(),4);assert_eq!(native.matches("<w:footnoteReference w:id=\"2\"").count(),1);assert_eq!(native.matches("<w:endnoteReference w:id=\"1\"").count(),3);assert_eq!(native.matches("<w:endnoteReference w:id=\"2\"").count(),1);assert!(native.contains(&format!("w:anchor=\"{}\"",crate::docx_writer::section_bookmark_name("native-last"))));
        let returned=import(&output.parts).unwrap();assert_eq!(returned.policies.kind_by_label.len(),4);assert_eq!(returned.policies.policy(NoteKind::Footnote).start,Some(3));assert_eq!(returned.policies.policy(NoteKind::Endnote).start,Some(1));
        source=super::super::docx_convert(&output.parts,std::path::Path::new("native-numbering-again.docx"),false).unwrap();assert!(source.contains("Later foot evidence")&&source.contains("Later end evidence"));
    }assert_eq!(entries,original);
}

#[test]
fn native_section_identity_never_guesses_a_source_id_missing_from_a_truncated_bookmark(){
    let complete="native-first";assert_eq!(complete.len(),12);let complete_name=crate::docx_writer::section_bookmark_name(complete);assert_eq!(recover_section_id(&complete_name).as_deref(),Some(complete));
    let truncated="native-middle";assert_eq!(truncated.len(),13);let truncated_name=crate::docx_writer::section_bookmark_name(truncated);assert!(truncated_name.starts_with("_ssection3anative_middl_"));assert!(recover_section_id(&truncated_name).is_none(),"the missing final source character cannot be reconstructed from the hash suffix");
    let paragraph=|name:&str|docx_notes::xml(format!("<w:p xmlns:w=\"{W}\"><w:bookmarkStart w:id=\"1\" w:name=\"{name}\"/><w:r><w:t>Same title</w:t></w:r><w:bookmarkEnd w:id=\"1\"/></w:p>").as_bytes()).unwrap();
    let short=source_section_id(Some(&paragraph(&truncated_name)),1,&mut HashSet::new()).unwrap();assert_ne!(short,truncated);assert!(short.len()<=11);let short_name=crate::docx_writer::section_bookmark_name(&short);assert_eq!(recover_section_id(&short_name).as_deref(),Some(short.as_str()));assert_eq!(source_section_id(Some(&paragraph(&short_name)),1,&mut HashSet::new()).unwrap(),short,"the first recovery assigns a fully recoverable native identity that does not drift on later imports");
}

#[test]
fn every_fixed_native_format_u16_start_and_restart_roundtrips_in_each_story(){
    assert_eq!(policy::WORD_NUMBER_FORMATS.len(),63);let starts=[0,1,27,12000,65535];let restarts=["continuous","eachSect","eachPage"];
    for(index,format)in policy::WORD_NUMBER_FORMATS.iter().enumerate(){
        let mut entries=native_position_sections();let start=starts[index%starts.len()];let restart=restarts[index%restarts.len()];let pattern=if *format=="custom"{" w:format=\"[A]&amp;&quot;\""}else{""};
        let leaves=format!("<w:numFmt w:val=\"{format}\"{pattern}/><w:numStart w:val=\"{start}\"/><w:numRestart w:val=\"{restart}\"/>");
        change(&mut entries,"word/document.xml","<w:pos w:val=\"beneathText\"/>",&format!("<w:pos w:val=\"beneathText\"/>{leaves}"));change(&mut entries,"word/document.xml","<w:pos w:val=\"sectEnd\"/>",&format!("<w:pos w:val=\"sectEnd\"/>{leaves}"));let original=entries.clone();let expected=section_numbering_snapshot(&entries);
        let source=super::super::docx_convert(&entries,std::path::Path::new("native-format.docx"),false).unwrap();let options=crate::publication_profile::resolve(&source,&json!({})).unwrap();let output=crate::docx_writer::build(&crate::md_ast::parse(&source),options.values(),"native-format.md",&|_|None);assert!(output.warns.is_empty(),"{format}: {:?}",output.warns);assert_eq!(section_numbering_snapshot(&output.parts),expected,"{format}");
        let reimported=super::super::docx_convert(&output.parts,std::path::Path::new("native-format-returned.docx"),false).unwrap();let sections=crate::publication_sections::analyze(&reimported).unwrap();let settings=sections[0].section.footnotes.as_ref().unwrap();for numbering in [&settings.footnote_numbering,&settings.endnote_numbering]{assert_eq!(numbering.number_format.as_deref(),Some(*format));assert_eq!(numbering.start,Some(start));}assert_eq!(entries,original);
    }
}

#[test]
fn section_writer_clears_inherited_pattern_and_materializes_required_val_for_pattern_only(){
    let declarations=[json!({"id":"pattern-inherit"}),json!({"id":"pattern-clear","footnotes":{"kinds":{"footnote":{"numberFormat":"decimal"},"endnote":{"start":0}}}}),json!({"id":"pattern-only","footnotes":{"kinds":{"footnote":{"numberPattern":"[A]&\""},"endnote":{"numberPattern":"[i]&\""}}}}),json!({"id":"restart-only","footnotes":{"kinds":{"footnote":{"restart":"page"},"endnote":{"restart":"page"}}}})];
    let mut source=String::new();for(index,declaration)in declarations.iter().enumerate(){source.push_str(&format!("<!-- readmd-section:v1 {declaration} -->\n\nSection {index}.\n\n"));}
    let options=json!({"frontMatter":{"mode":"none"},"footnotes":{"kinds":{"footnote":{"numberFormat":"custom","numberPattern":"GLOBAL-A","start":3,"restart":"continuous"},"endnote":{"numberFormat":"lowerRoman","numberPattern":"GLOBAL-i","start":1,"restart":"continuous"}}}});
    let output=crate::docx_writer::build(&crate::md_ast::parse(&source),&options,"patterns.md",&|_|None);assert!(output.warns.is_empty(),"{:?}",output.warns);let native=section_numbering_snapshot(&output.parts);assert_eq!(native.len(),4);
    assert_eq!(native[0],json!({}));assert_eq!(native[1],json!({"footnote":{"numFmt":{"val":"decimal"}},"endnote":{"numStart":{"val":"0"}}}));assert_eq!(native[2],json!({"footnote":{"numFmt":{"val":"custom","format":"[A]&\""}},"endnote":{"numFmt":{"val":"lowerRoman","format":"[i]&\""}}}));assert_eq!(native[3],json!({"footnote":{"numRestart":{"val":"eachPage"}},"endnote":{"numRestart":{"val":"eachPage"}}}));
    let converted=super::super::docx_convert(&output.parts,std::path::Path::new("patterns.docx"),false).unwrap();let resolved=crate::publication_profile::resolve(&converted,&json!({})).unwrap();let sections=crate::publication_sections::resolve(&crate::md_ast::parse(&converted),resolved.values()).unwrap();assert_eq!(policy::parse_layer(&sections[1].style).unwrap().policy(NoteKind::Footnote).number_pattern,None);assert_eq!(policy::parse_layer(&sections[0].style).unwrap().policy(NoteKind::Footnote).number_pattern.as_deref(),Some("GLOBAL-A"));
    let again=crate::docx_writer::build(&crate::md_ast::parse(&converted),resolved.values(),"patterns-returned.md",&|_|None);assert!(again.warns.is_empty(),"{:?}",again.warns);assert_eq!(section_numbering_snapshot(&again.parts),native);
}

#[test]
fn current_native_section_numbering_changes_and_clears_preserve_auxiliary_bytes(){
    let source="<!-- readmd-numbering:v1 {\"headings\":{\"levels\":[{}]}} -->\n\n<!-- readmd-section:v1 {\"id\":\"bound-first\",\"footnotes\":{\"kinds\":{\"footnote\":{\"numberFormat\":\"upperRoman\",\"start\":7,\"restart\":\"section\"},\"endnote\":{\"numberFormat\":\"upperLetter\",\"start\":27,\"restart\":\"section\"}}}} -->\n\n# Same title {#bound-title}\n\nFirst[^a] and end[^e].\n\n<!-- readmd-section:v1 {\"id\":\"bound-last\"} -->\n\nLast and repeated[^a].\n\n[^a]: Foot evidence.\n\n[^e]: End evidence.\n";
    let global=json!({"frontMatter":{"mode":"none"},"footnotes":{"kindByLabel":{"a":"footnote","e":"endnote"},"kinds":{"footnote":{"numberFormat":"lowerLetter","start":3,"restart":"continuous"},"endnote":{"numberFormat":"lowerRoman","start":1,"restart":"continuous"}}}});
    let output=crate::docx_writer::build(&crate::md_ast::parse(source),&global,"bound.md",&|_|None);assert!(output.warns.is_empty(),"{:?}",output.warns);let mut entries=output.parts;super::super::docx_numbering_import::attach_source_info(&mut entries,source).unwrap();assert!(entries.iter().any(|(name,_)|name==super::super::docx_numbering_import::SOURCE_PART));let auxiliary=entries.iter().filter(|(name,_)|name!="word/document.xml").cloned().collect::<Vec<_>>();
    for(from,to)in[("<w:numFmt w:val=\"upperRoman\"/>","<w:numFmt w:val=\"custom\" w:format=\"CURRENT\"/>"),("<w:numStart w:val=\"7\"/>","<w:numStart w:val=\"12000\"/>"),("<w:numRestart w:val=\"eachSect\"/>","<w:numRestart w:val=\"eachPage\"/>")]{change(&mut entries,"word/document.xml",from,to);}
    let expected=section_numbering_snapshot(&entries);let converted=super::super::docx_convert(&entries,std::path::Path::new("bound-current.docx"),false).unwrap();let resolved=crate::publication_profile::resolve(&converted,&json!({})).unwrap();let output=crate::docx_writer::build(&crate::md_ast::parse(&converted),resolved.values(),"bound-current.md",&|_|None);assert!(output.warns.is_empty(),"{:?}",output.warns);assert_eq!(section_numbering_snapshot(&output.parts),expected);assert_eq!(entries.iter().filter(|(name,_)|name!="word/document.xml").cloned().collect::<Vec<_>>(),auxiliary);
    for leaf in ["<w:numFmt w:val=\"custom\" w:format=\"CURRENT\"/>","<w:numStart w:val=\"12000\"/>","<w:numRestart w:val=\"eachPage\"/>","<w:numFmt w:val=\"upperLetter\"/>","<w:numStart w:val=\"27\"/>","<w:numRestart w:val=\"eachSect\"/>"]{change(&mut entries,"word/document.xml",leaf,"");}
    let converted=super::super::docx_convert(&entries,std::path::Path::new("bound-clear-local.docx"),false).unwrap();let declarations=crate::publication_sections::analyze(&converted).unwrap();assert!(declarations.iter().all(|declaration|declaration.section.footnotes.is_none()));let options=crate::publication_profile::resolve(&converted,&json!({})).unwrap();let policies=policy::parse_layer(options.values()).unwrap();assert_eq!(policies.policy(NoteKind::Footnote).start,Some(3));assert_eq!(policies.policy(NoteKind::Endnote).number_format.as_deref(),Some("lowerRoman"));assert_eq!(entries.iter().filter(|(name,_)|name!="word/document.xml").cloned().collect::<Vec<_>>(),auxiliary);
    for leaf in ["<w:numFmt w:val=\"lowerLetter\"/>","<w:numStart w:val=\"3\"/>","<w:numFmt w:val=\"lowerRoman\"/>","<w:numStart w:val=\"1\"/>"]{change(&mut entries,"word/settings.xml",leaf,"");}for _ in 0..2{change(&mut entries,"word/settings.xml","<w:numRestart w:val=\"continuous\"/>","");}
    let converted=super::super::docx_convert(&entries,std::path::Path::new("bound-clear-all.docx"),false).unwrap();let options=crate::publication_profile::resolve(&converted,&json!({})).unwrap();let policies=policy::parse_layer(options.values()).unwrap();for(kind,format)in[(NoteKind::Footnote,"decimal"),(NoteKind::Endnote,"lowerRoman")]{let policy=policies.policy(kind);assert_eq!(policy.number_format.as_deref(),Some(format));assert_eq!(policy.start,Some(1));assert_eq!(policy.restart,Some(NoteRestart::Continuous));}assert!(crate::publication_sections::analyze(&converted).unwrap().iter().all(|declaration|declaration.section.footnotes.is_none()));
}

#[test]
fn native_section_numbering_rejects_invalid_schema_leaves_before_source_or_output(){
    for(xml,code,path)in[
        ("<w:numFmt/>",INVALID,"numFmt"),("<w:numFmt w:format=\"A\"/>",INVALID,"numFmt"),("<w:numFmt w:val=\"invented\"/>",INVALID,"numFmt"),("<w:numFmt w:val=\"decimal\"/><w:numFmt w:val=\"decimal\"/>",INVALID,"numFmt"),
        ("<w:numStart/>",INVALID,"numStart"),("<w:numStart w:val=\"-1\"/>",INVALID,"numStart"),("<w:numStart w:val=\"65536\"/>",INVALID,"numStart"),("<w:numStart w:val=\"1.5\"/>",INVALID,"numStart"),("<w:numStart w:val=\"true\"/>",INVALID,"numStart"),
        ("<w:numRestart/>",INVALID,"numRestart"),("<w:numRestart w:val=\"chapter\"/>",INVALID,"numRestart"),("<w:numRestart w:val=\"eachSect\"/><w:numStart w:val=\"7\"/>",INVALID,"numStart"),
        ("<q:numFmt xmlns:q=\"urn:foreign\" w:val=\"decimal\"/>",UNSUPPORTED,"numFmt"),("<w:numStart w:val=\"7\" q:value=\"7\" xmlns:q=\"urn:foreign\"/>",UNSUPPORTED,"numStart"),("<w:footnote w:id=\"-1\"/>",UNSUPPORTED,"footnote"),
    ]{
        let mut entries=native_position_sections();change(&mut entries,"word/document.xml","<w:pos w:val=\"beneathText\"/>",&format!("<w:pos w:val=\"beneathText\"/>{xml}"));let original=entries.clone();let error=import(&entries).unwrap_err();assert_eq!(error.code,code,"{xml}: {error}");assert!(error.path.contains(path),"{xml}: {error}");assert_eq!(entries,original);
    }
}

#[test]
fn unused_explicit_native_numbering_defaults_remain_section_owned(){
    let output=crate::docx_writer::build(&crate::md_ast::parse("No note references."),&json!({"frontMatter":{"mode":"none"}}),"unused-defaults.md",&|_|None);assert!(output.warns.is_empty());let mut entries=output.parts;
    let leaves="<w:footnotePr><w:numFmt w:val=\"decimal\"/><w:numStart w:val=\"1\"/><w:numRestart w:val=\"continuous\"/></w:footnotePr><w:endnotePr><w:numFmt w:val=\"lowerRoman\"/><w:numStart w:val=\"1\"/><w:numRestart w:val=\"continuous\"/></w:endnotePr>";change(&mut entries,"word/document.xml","<w:sectPr>",&format!("<w:sectPr>{leaves}"));let expected=section_numbering_snapshot(&entries);
    let imported=import(&entries).unwrap();assert_eq!(imported.to_options(),json!({}));assert_eq!(imported.sections.len(),1);assert_eq!(imported.sections[0].footnote.start,Some(1));assert_eq!(imported.sections[0].endnote.start,Some(1));let source=super::super::docx_convert(&entries,std::path::Path::new("unused-defaults.docx"),false).unwrap();assert!(crate::publication_profile::from_source(&source).unwrap().options.get("footnotes").is_none());let options=crate::publication_profile::resolve(&source,&json!({})).unwrap();let output=crate::docx_writer::build(&crate::md_ast::parse(&source),options.values(),"unused-defaults.md",&|_|None);assert!(output.warns.is_empty(),"{:?}",output.warns);assert_eq!(section_numbering_snapshot(&output.parts),expected);
}
