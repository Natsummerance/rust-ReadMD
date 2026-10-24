//! DOCX note stories: identity, strict package relationships and shared rendering.
//! The Markdown labels keep footnotes and endnotes distinct even when IDs collide.
use super::*;
use quick_xml::{events::{Event, BytesStart}, name::ResolveResult, NsReader, XmlVersion};
use std::collections::HashSet;
use std::cell::RefCell;

pub(super) const INVALID: &str = crate::api_codes::DOCX_NOTES_INVALID;
const WORD: &str = crate::math_document::WORD_NS;
const REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const OFFICE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const WORD_DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const PICTURE: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
const MAX_PART: usize = 32 * 1024 * 1024;
const MAX_NODES: usize = 400_000;

fn err() -> String { INVALID.into() }

fn valid_xml_text(text: &str) -> Result<(),String> {
    if text.chars().any(|c| !matches!(c,'\t'|'\r'|'\n') && ((c as u32)<32 || matches!(c,'\u{fffe}'|'\u{ffff}'))) { return Err(err()); }
    Ok(())
}

fn qualified(namespace: ResolveResult<'_>, name: &str) -> Result<String, String> {
    let local = local_name_of(name);
    let uri = match namespace {
        ResolveResult::Bound(uri) => uri.as_ref().to_string(),
        ResolveResult::Unbound => String::new(),
        ResolveResult::Unknown(_) => return Err(err()),
    };
    let prefix = match uri.as_str() {
        WORD | "http://purl.oclc.org/ooxml/wordprocessingml/main" => "w",
        crate::math_document::MATH_NS | "http://purl.oclc.org/ooxml/officeDocument/math" => "m",
        OFFICE_REL | "http://purl.oclc.org/ooxml/officeDocument/relationships" => "r",
        DRAWING | "http://purl.oclc.org/ooxml/drawingml/main" => "a",
        WORD_DRAWING => "wp",
        PICTURE => "pic",
        REL => "pkg",
        "http://www.w3.org/XML/1998/namespace" => "xml",
        "" => return Ok(name.to_string()),
        _ => return Ok(if name.split_once(':').is_some_and(|(prefix,_)|matches!(prefix,"w"|"m"|"r"|"a"|"wp"|"pic"|"pkg")) {format!("foreign:{local}")}else{name.to_string()}),
    };
    Ok(format!("{prefix}:{local}"))
}

fn start(reader: &NsReader<&[u8]>, event: &BytesStart<'_>, namespace: ResolveResult<'_>) -> Result<XmlNode, String> {
    let name = qualified(namespace, event.name().as_ref())?;
    let mut attrs = Vec::new();
    let mut keys = HashSet::new();
    for attribute in event.attributes() {
        let attribute = attribute.map_err(|_| err())?;
        let raw = attribute.key.as_ref();
        let key = if raw == "xmlns" || raw.starts_with("xmlns:") { raw.to_string() } else {
            let (ns, _) = reader.resolver().resolve_attribute(attribute.key);
            qualified(ns, attribute.key.as_ref())?
        };
        if !keys.insert(key.clone()) { return Err(err()); }
        let value=attribute.normalized_value(XmlVersion::Implicit1_0).map_err(|_| err())?.into_owned();
        valid_xml_text(&value)?;
        attrs.push((key,value));
    }
    // A spoofed note element/attribute must not be accepted by legacy local-name helpers.
    if matches!(local_name_of(&name), "footnoteReference" | "endnoteReference" | "footnote" | "endnote" | "footnotes" | "endnotes") && !name.starts_with("w:") {
        return Err(err());
    }
    Ok(XmlNode { local: local_name_of(&name).into(), name, attrs, children: Vec::new(), text: String::new(), tail: String::new() })
}

fn append(node: &mut XmlNode, text: &str) {
    if let Some(child) = node.children.last_mut() { child.tail.push_str(text); } else { node.text.push_str(text); }
}

/// Namespace-aware, bounded parsing without the math parser's 1 MiB formula budget.
pub(super) fn xml(data: &[u8]) -> Result<XmlNode, String> {
    if data.len() > MAX_PART { return Err(err()); }
    let source = std::str::from_utf8(data).map_err(|_| err())?;
    valid_xml_text(source)?;
    let mut reader = NsReader::from_str(source);
    reader.config_mut().check_end_names = true;
    reader.config_mut().check_comments = true;
    let mut roots = Vec::new();
    let mut stack: Vec<XmlNode> = Vec::new();
    let mut nodes = 0usize;
    loop {
        let event = reader.read_event().map_err(|_| err())?;
        match event {
            Event::Start(ref element) | Event::Empty(ref element) => {
                nodes += 1;
                if nodes > MAX_NODES || stack.len() >= 128 { return Err(err()); }
                let (ns, _) = reader.resolver().resolve_element(element.name());
                let node = start(&reader, element, ns)?;
                if matches!(event, Event::Start(_)) { stack.push(node); }
                else if let Some(parent) = stack.last_mut() { parent.children.push(node); }
                else { roots.push(node); }
            }
            Event::End(_) => {
                let node = stack.pop().ok_or_else(err)?;
                if let Some(parent) = stack.last_mut() { parent.children.push(node); } else { roots.push(node); }
            }
            Event::Text(t) => {
                let text = t.xml_content(XmlVersion::Implicit1_0);
                if let Some(node) = stack.last_mut() { append(node, &text); }
                else if !text.trim().is_empty() { return Err(err()); }
            }
            Event::CData(t) => { append(stack.last_mut().ok_or_else(err)?, &t.xml_content(XmlVersion::Implicit1_0)); }
            Event::GeneralRef(t) => {
                let encoded = format!("&{};", t.as_ref());
                let text = quick_xml::escape::unescape(&encoded).map_err(|_| err())?;
                valid_xml_text(&text)?;
                append(stack.last_mut().ok_or_else(err)?, &text);
            }
            Event::DocType(_) => return Err(err()),
            Event::Eof => break,
            _ => (),
        }
    }
    if !stack.is_empty() || roots.len() != 1 { return Err(err()); }
    let mut root = roots.pop().unwrap();
    // Canonical names let the shared converter handle arbitrary source prefixes.
    for (prefix, namespace) in [("w",WORD),("m",crate::math_document::MATH_NS),("r",OFFICE_REL),("a",DRAWING),("wp",WORD_DRAWING),("pic",PICTURE)] {
        root.attrs.retain(|(key,_)| key != &format!("xmlns:{prefix}"));
        root.attrs.push((format!("xmlns:{prefix}"),namespace.into()));
    }
    Ok(root)
}

pub(super) fn member(base: &str, target: &str) -> Result<String, String> {
    let decoded = percent_encoding::percent_decode_str(target).decode_utf8().map_err(|_| err())?;
    if decoded.is_empty() || decoded.contains(['\\', ':', '?', '#', '\0']) { return Err(err()); }
    let mut parts: Vec<&str> = if decoded.starts_with('/') { Vec::new() } else { base.split('/').filter(|p| !p.is_empty()).collect() };
    for item in decoded.split('/') {
        match item { "" | "." => (), ".." => { parts.pop().ok_or_else(err)?; }, _ => parts.push(item) }
    }
    if parts.is_empty() { return Err(err()); }
    Ok(parts.join("/"))
}

pub(super) fn unique<'a>(entries: &'a [(String,Vec<u8>)], name: &str) -> Result<&'a [u8],String> {
    let mut found = entries.iter().filter(|(path,_)| path == name);
    let bytes = found.next().ok_or_else(err)?.1.as_slice();
    if found.next().is_some() { return Err(err()); }
    Ok(bytes)
}

pub(super) fn relationships(entries: &[(String,Vec<u8>)], part: &str) -> Result<Vec<DocxRel>,String> {
    let (base,file) = part.rsplit_once('/').unwrap_or(("",part));
    let path = format!("{base}/_rels/{file}.rels");
    if !entries.iter().any(|(name,_)| name == &path) { return Ok(Vec::new()); }
    let root = xml(unique(entries,&path)?)?;
    if root.name != "pkg:Relationships" { return Err(err()); }
    let mut ids = HashSet::new();
    let mut out = Vec::new();
    for rel in &root.children {
        if rel.name != "pkg:Relationship" { return Err(err()); }
        let id = rel.attr("Id").filter(|v|!v.is_empty()).ok_or_else(err)?;
        let raw = rel.attr("Target").filter(|v|!v.is_empty()).ok_or_else(err)?;
        let kind = rel.attr("Type").filter(|v|!v.is_empty()).ok_or_else(err)?;
        if !ids.insert(id.to_string()) { return Err(err()); }
        let mode=rel.attr("TargetMode").unwrap_or("");
        if !matches!(mode,""|"Internal"|"External") { return Err(err()); }
        out.push(DocxRel {id:id.into(),raw:raw.into(),kind:kind.into(),mode:mode.into()});
    }
    Ok(out)
}

fn key(node: &XmlNode) -> Result<Option<(bool,i32)>,String> {
    let end = match node.name.as_str() { "w:footnoteReference" => false, "w:endnoteReference" => true, _ => return Ok(None) };
    let id = node.attr("w:id").ok_or_else(err)?.parse().map_err(|_| err())?;
    Ok(Some((end,id)))
}

fn references(node: &XmlNode, out: &mut Vec<(bool,i32)>) -> Result<(),String> {
    fn visit(node: &XmlNode, out: &mut Vec<(bool,i32)>, seen: &mut HashSet<(bool,i32)>) -> Result<(),String> {
        if matches!(node.name.as_str(),"w:del" | "w:sdtPr" | "w:moveFrom") { return Ok(()); }
        if let Some(id) = key(node)? { if seen.insert(id) { out.push(id); } }
        for child in &node.children { visit(child,out,seen)?; }
        Ok(())
    }
    visit(node,out,&mut HashSet::new())
}

pub(super) fn label(end: bool, id: i32) -> String { format!("word-{}-{id}",if end {"endnote"} else {"footnote"}) }

struct Story { end: bool, base: String, part: String, root: XmlNode, rels: Vec<DocxRel>, definitions: HashMap<i32,usize> }
/// Borrow an already validated story. Settings import must not parse the same
/// part again for every definition, or infer its kind from a Markdown label.
pub(super) struct NoteStoryRef<'a> {
    pub end: bool,
    pub part: &'a str,
    pub root: &'a XmlNode,
    pub definitions: &'a HashMap<i32,usize>,
}
#[derive(Default)]
pub(super) struct Notes {
    stories: Vec<Story>, order: Vec<(bool,i32)>, identities: HashSet<(bool,i32)>,
    expected: HashMap<(bool,i32),usize>, rendered: RefCell<HashMap<(bool,i32),usize>>,
    suppressed_generated: RefCell<HashSet<(bool,i32)>>,
}
impl Notes {
    pub(super) fn settings_stories(&self) -> Vec<NoteStoryRef<'_>> {
        self.stories.iter().map(|story|NoteStoryRef{end:story.end,part:&story.part,root:&story.root,definitions:&story.definitions}).collect()
    }
    /// Borrow the same validated note node used by the run/style index. No
    /// reparsing, text matching, label-prefix inference or story cloning.
    pub(super) fn settings_note(&self,end:bool,id:i32)->Option<(&str,&XmlNode)>{
        let story=self.stories.iter().find(|story|story.end==end)?;
        let node=story.root.children.get(*story.definitions.get(&id)?)?;
        Some((&story.part,node))
    }
    /// Call after citation controls have been rendered: verified generated
    /// notes have no Markdown definition and must not leave a stale binding.
    pub(super) fn settings_bindings(&self) -> Vec<(bool,i32,String)> {
        let suppressed=self.suppressed_generated.borrow();
        self.order.iter().filter(|identity|!suppressed.contains(identity)).map(|(end,id)|(*end,*id,label(*end,*id))).collect()
    }
    pub(super) fn settings_used(&self) -> &[(bool,i32)] { &self.order }

    pub(super) fn load(entries: &[(String,Vec<u8>)], body: &XmlNode, rels: &[DocxRel]) -> Result<Self,String> {
        let mut out = Self::default();
        references(body,&mut out.order)?;
        fn count(node: &XmlNode, counts: &mut HashMap<(bool,i32),usize>) -> Result<(),String> {
            if matches!(node.name.as_str(),"w:del"|"w:sdtPr"|"w:moveFrom") { return Ok(()); }
            if let Some(id)=key(node)? { *counts.entry(id).or_default()+=1; }
            for child in &node.children { count(child,counts)?; }
            Ok(())
        }
        count(body,&mut out.expected)?;
        out.identities.extend(out.order.iter().copied());
        if out.order.is_empty() { return Ok(out); }
        for end in [false,true] {
            if !out.order.iter().any(|(kind,_)| *kind == end) { continue; }
            let kind = if end {"endnotes"} else {"footnotes"};
            let mut related = rels.iter().filter(|r| r.kind == format!("{OFFICE_REL}/{kind}") || r.kind == format!("http://purl.oclc.org/ooxml/officeDocument/relationships/{kind}"));
            let rel = related.next().ok_or_else(err)?;
            if related.next().is_some() || rel.mode.eq_ignore_ascii_case("External") { return Err(err()); }
            let part = member("word",&rel.raw)?;
            let root = xml(unique(entries,&part)?)?;
            if root.name != format!("w:{kind}") { return Err(err()); }
            let mut definitions = HashMap::new();
            let mut all_ids = HashSet::new();
            for (index,note) in root.children.iter().enumerate() {
                if note.name != format!("w:{}",if end {"endnote"} else {"footnote"}) { return Err(err()); }
                let id: i32 = note.attr("w:id").ok_or_else(err)?.parse().map_err(|_| err())?;
                if !all_ids.insert(id) { return Err(err()); }
                let mut nested = Vec::new(); references(note,&mut nested)?;
                if !nested.is_empty() { return Err(crate::api_codes::DOCX_NESTED_FOOTNOTE.into()); }
                match note.attr("w:type").unwrap_or("normal") {
                    "normal" => { definitions.insert(id,index); },
                    "separator" | "continuationSeparator" | "continuationNotice" => (),
                    _ => return Err(err()),
                }
            }
            if out.order.iter().any(|(kind,id)| *kind == end && !definitions.contains_key(id)) { return Err(err()); }
            out.stories.push(Story {end,base:part.rsplit_once('/').map(|(base,_)|base).unwrap_or("").into(),rels:relationships(entries,&part)?,part,root,definitions});
        }
        Ok(out)
    }

    pub(super) fn has_math(&self) -> bool {
        self.order.iter().any(|(end,id)| {
            let story = self.stories.iter().find(|s|s.end==*end).unwrap();
            story.root.children[story.definitions[id]].descendants().iter().any(|n| matches!(n.name.as_str(),"m:oMath"|"m:oMathPara"))
        })
    }

    pub(super) fn reference(&self, node: &XmlNode) -> Result<Option<String>,String> {
        let Some((end,id)) = key(node)? else { return Ok(None); };
        if !self.identities.contains(&(end,id)) { return Err(err()); }
        *self.rendered.borrow_mut().entry((end,id)).or_default()+=1;
        Ok(Some(format!("[^{}]",label(end,id))))
    }

    pub(super) fn ensure_rendered(&self) -> Result<(),String> {
        // Unknown containing structures must fail instead of emitting a note
        // definition after silently discarding its reference in the body.
        let suppressed=self.suppressed_generated.borrow();
        let expected=self.expected.iter().filter(|(key,_)|!suppressed.contains(key)).map(|(key,value)|(*key,*value)).collect::<HashMap<_,_>>();
        if *self.rendered.borrow() != expected { return Err(err()); }
        Ok(())
    }

    /// Only a verified source-owned anchor may remove its unchanged generated
    /// note. Ordinary author notes and shared/copied references cannot enter it.
    pub(super) fn suppress_generated(&self,end:bool,id:i32)->Result<(),String>{
        if self.expected.get(&(end,id))!=Some(&1)||self.rendered.borrow().contains_key(&(end,id)){return Err(err());}
        if !self.suppressed_generated.borrow_mut().insert((end,id)){return Err(err());}Ok(())
    }

    pub(super) fn render(&self, pkg: &DocxPackage, styles: &DocxStyles) -> Result<String,String> {
        self.render_inner(pkg,styles,None)
    }
    pub(super) fn render_with_settings(&self,pkg:&DocxPackage,styles:&DocxStyles,settings:&super::docx_note_settings::ImportedNoteSettings)->Result<String,String>{
        self.render_inner(pkg,styles,Some(settings))
    }
    fn render_inner(&self,pkg:&DocxPackage,styles:&DocxStyles,settings:Option<&super::docx_note_settings::ImportedNoteSettings>)->Result<String,String>{
        let mut patches=HashMap::<(bool,i32),Vec<&super::docx_note_settings::NoteLocalPatch>>::new();
        if let Some(settings)=settings{for patch in &settings.local{patches.entry((patch.kind.is_end(),patch.native_id)).or_default().push(patch);}}
        let mut notes = Vec::new();
        for (end,id) in &self.order {
            if self.suppressed_generated.borrow().contains(&(*end,*id)){continue;}
            let story = self.stories.iter().find(|s|s.end==*end).unwrap();
            let note = &story.root.children[story.definitions[id]];
            let settings_error=|code,detail:&str|super::docx_note_settings::Error{code,part:story.part.clone(),path:format!("{}[@w:id='{id}']",if *end{"endnote"}else{"footnote"}),detail:detail.into()}.to_string();
            let mut local_styles=HashMap::<usize,String>::new();
            if let Some(patches)=patches.remove(&(*end,*id)){
                for patch in patches{
                    if patch.label!=label(*end,*id){return Err(settings_error(super::docx_note_settings::INVALID,"note binding changed"));}
                    let mut node=note;
                    for index in &patch.paragraph_path{node=node.children.get(*index).ok_or_else(||settings_error(super::docx_note_settings::INVALID,"note paragraph moved"))?;}
                    if node.name!="w:p"{return Err(settings_error(super::docx_note_settings::INVALID,"note paragraph moved"));}
                    let comment=crate::publication_blocks::comment(&patch.style).map_err(|_|settings_error(super::docx_note_settings::UNSUPPORTED,"native note paragraph style cannot be represented by source local layout"))?;
                    if local_styles.insert(node as *const XmlNode as usize,comment).is_some(){return Err(settings_error(super::docx_note_settings::INVALID,"duplicate note paragraph binding"));}
                }
            }
            let local = DocxPackage {entries:pkg.entries,rels:story.rels.clone(),source:pkg.source,base_dir:story.base.clone(),part:story.part.clone(),strict_story:true,notes:self,numbering:pkg.numbering,heading_import:pkg.heading_import,section_anchors:pkg.section_anchors,citations:pkg.citations,fields:DocxFields::new(note)?,math_defaults:pkg.math_defaults,math_styles:pkg.math_styles,math_theme:pkg.math_theme,math_fonts:pkg.math_fonts,text_styles:pkg.text_styles,namespaces:story.root.attrs.iter().filter(|(k,_)|k=="xmlns"||k.starts_with("xmlns:")).cloned().collect()};
            let (mut lines,mut code,mut lang,mut ordered) = (Vec::new(),Vec::new(),String::new(),0);
            for child in docx_word_children(note) {
                match child.name.as_str() {
                    "w:p" => {
                        if let Some(comment)=local_styles.remove(&(child as *const XmlNode as usize)){
                            docx_flush_code(&mut lines,&mut code,&mut lang);
                            let mut paragraph=Vec::new();
                            docx_handle_para(child,&local,styles,&mut paragraph,&mut code,&mut lang,&mut ordered)?;
                            docx_flush_code(&mut paragraph,&mut code,&mut lang);
                            if paragraph.iter().all(|line|line.trim().is_empty()){return Err(settings_error(super::docx_note_settings::UNSUPPORTED,"empty styled note paragraph has no source block"));}
                            // Inline directives are the existing local-layout
                            // representation for nested list/quote paragraphs.
                            // A separate block directive would split the list.
                            let nested=docx_para_list_info(child,styles).0||docx_style_for_para(child,styles).is_some_and(|s|s.name.to_lowercase().contains("quote"))||docx_pstyle_raw(child).as_deref()==Some("Quote");
                            if nested{
                                let at=paragraph.iter().rposition(|line|!line.trim().is_empty()).unwrap();paragraph[at].push(' ');paragraph[at].push_str(&comment);
                            }else{lines.push(comment);lines.push(String::new());}
                            lines.extend(paragraph);
                        }else{docx_handle_para(child,&local,styles,&mut lines,&mut code,&mut lang,&mut ordered)?;}
                    },
                    "w:tbl" => { docx_flush_code(&mut lines,&mut code,&mut lang); lines.push(docx_table_to_md(child,&local,false)?); lines.push(String::new()); },
                    _ => (),
                }
            }
            if !local_styles.is_empty(){return Err(settings_error(super::docx_note_settings::UNSUPPORTED,"note paragraph inside an unsupported source container"));}
            docx_flush_code(&mut lines,&mut code,&mut lang);
            let content = lines.join("\n").trim().to_string();
            // A legal empty note still needs a definition; it must not become an unresolved ref.
            let mut content = content.lines();
            let mut definition = format!("[^{}]: {}",label(*end,*id),content.next().unwrap_or(""));
            for line in content { definition.push('\n'); if !line.is_empty(){definition.push_str("    ");definition.push_str(line);} }
            notes.push(definition);
        }
        if !patches.is_empty(){return Err(super::docx_note_settings::Error{code:super::docx_note_settings::INVALID,part:"notes".into(),path:"local".into(),detail:"unconsumed note paragraph binding".into()}.to_string());}
        let result=notes.join("\n\n");
        if settings.is_some(){crate::publication_blocks::validate_source(&result).map_err(|_|super::docx_note_settings::Error{code:super::docx_note_settings::UNSUPPORTED,part:"notes".into(),path:"local".into(),detail:"native paragraph style has no matching source layout block".into()}.to_string())?;}
        Ok(result)
    }
}

#[derive(Default)]
pub(super) struct Numbering { formats: HashMap<(String,usize),(bool,i64)> }
impl Numbering {
    pub(super) fn load(entries: &[(String,Vec<u8>)], rels: &[DocxRel]) -> Result<Self,String> {
        let Some(rel) = rels.iter().find(|r|r.kind.ends_with("/numbering") && r.mode!="External") else { return Ok(Self::default()); };
        let path = member("word",&rel.raw)?;
        let root = xml(unique(entries,&path)?)?;
        let mut levels = HashMap::new();
        for abstract_num in el_children(&root,"w:abstractNum") {
            let Some(id) = abstract_num.attr("w:abstractNumId") else { continue; };
            for level in el_children(abstract_num,"w:lvl") {
                let index = level.attr("w:ilvl").and_then(|s|s.parse::<usize>().ok()).unwrap_or(0);
                let format = el_child(level,"w:numFmt").and_then(|n|n.attr("w:val")).unwrap_or("bullet");
                let start = el_child(level,"w:start").and_then(|n|n.attr("w:val")).and_then(|s|s.parse().ok()).unwrap_or(1);
                levels.insert((id.to_string(),index),(format!="bullet"&&format!="none",start));
            }
        }
        let mut out = Self::default();
        for num in el_children(&root,"w:num") {
            let Some(id) = num.attr("w:numId") else { continue; };
            let Some(base) = el_child(num,"w:abstractNumId").and_then(|n|n.attr("w:val")) else { continue; };
            for ((abstract_id,level),(ordered,start)) in &levels {
                if abstract_id!=base {continue;}
                let start = el_children(num,"w:lvlOverride").into_iter().find(|n|n.attr("w:ilvl").and_then(|v|v.parse::<usize>().ok())==Some(*level)).and_then(|n|el_child(n,"w:startOverride")).and_then(|n|n.attr("w:val")).and_then(|v|v.parse().ok()).unwrap_or(*start);
                out.formats.insert((id.to_string(),*level),(*ordered,start));
            }
        }
        Ok(out)
    }
    pub(super) fn level(&self, p: &XmlNode, level: usize) -> Option<(bool,i64)> {
        let id = el_child(p,"w:pPr").and_then(|n|el_child(n,"w:numPr")).and_then(|n|el_child(n,"w:numId")).and_then(|n|n.attr("w:val"))?;
        self.formats.get(&(id.to_string(),level)).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(body: &str, footnotes: &str, endnotes: &str) -> Vec<(String,Vec<u8>)> {
        let document = format!(r#"<w:document xmlns:w="{WORD}"><w:body>{body}</w:body></w:document>"#);
        let mut relationships = format!(r#"<Relationships xmlns="{REL}">"#);
        let mut entries = vec![("word/document.xml".into(),document.into_bytes())];
        for (name,content) in [("footnotes",footnotes),("endnotes",endnotes)] {
            if content.is_empty() {continue;}
            relationships.push_str(&format!(r#"<Relationship Id="{name}" Type="{OFFICE_REL}/{name}" Target="{name}.xml"/>"#));
            entries.push((format!("word/{name}.xml"),format!(r#"<w:{name} xmlns:w="{WORD}">{content}</w:{name}>"#).into_bytes()));
        }
        relationships.push_str("</Relationships>");
        entries.push(("word/_rels/document.xml.rels".into(),relationships.into_bytes()));
        entries
    }
    fn note(end: bool, id: &str, content: &str) -> String {
        let tag = if end {"endnote"}else{"footnote"};
        format!(r#"<w:{tag} w:id="{id}"><w:p><w:r><w:{tag}Ref/><w:t>{content}</w:t></w:r></w:p></w:{tag}>"#)
    }
    fn convert(entries: &[(String,Vec<u8>)]) -> Result<String,String> {
        docx_convert(entries,Path::new("Synthetic-note-study.docx"),true)
    }
    fn replace(entries: &mut [(String,Vec<u8>)], part: &str, from: &str, to: &str) {
        let bytes = &mut entries.iter_mut().find(|(name,_)|name==part).unwrap().1;
        *bytes = String::from_utf8(bytes.clone()).unwrap().replace(from,to).into_bytes();
    }
    const BODY: &str = r#"<w:p><w:r><w:t>Before</w:t><w:footnoteReference w:id="1"/><w:t>Middle</w:t><w:endnoteReference w:id="1"/><w:t>After</w:t><w:footnoteReference w:id="1"/></w:r></w:p>"#;

    #[test]
    fn docx_notes_keep_run_order_shared_references_and_distinct_story_ids() {
        let entries = package(BODY,&note(false,"1","Foot evidence"),&note(true,"1","End evidence"));
        let before = entries.clone();
        let markdown = convert(&entries).unwrap();
        let parsed=crate::md_ast::parse(&markdown);let expected=crate::md_ast::parse("Before[^word-footnote-1]Middle[^word-endnote-1]After[^word-footnote-1]\n\n[^word-footnote-1]: Foot evidence\n\n[^word-endnote-1]: End evidence\n");
        assert_eq!(parsed.blocks.first(),expected.blocks.first(),"{markdown}");
        let profile=crate::publication_profile::from_source(&markdown).unwrap();assert!(profile.diagnostics.is_empty());assert_eq!(profile.options["footnotes"]["kindByLabel"]["word-endnote-1"],"endnote");
        assert_eq!(markdown.matches("[^word-footnote-1]:").count(),1);
        assert_eq!(markdown.matches("[^word-endnote-1]:").count(),1);
        assert!(markdown.contains("[^word-footnote-1]: Foot evidence")&&markdown.contains("[^word-endnote-1]: End evidence"));
        assert_eq!(entries,before);
    }

    #[test]
    fn adjacent_note_references_and_a_link_remain_distinct_without_added_visible_spaces() {
        let body=format!(r#"<w:p><w:r><w:t>Before</w:t><w:footnoteReference w:id="1"/><w:endnoteReference w:id="1"/><w:footnoteReference w:id="1"/></w:r><w:hyperlink xmlns:r="{OFFICE_REL}" r:id="link"><w:r><w:t>Jump</w:t></w:r></w:hyperlink></w:p>"#);
        let mut entries=package(&body,&note(false,"1","Foot evidence"),&note(true,"1","End evidence"));
        replace(&mut entries,"word/_rels/document.xml.rels","</Relationships>",&format!(r#"<Relationship Id="link" Type="{OFFICE_REL}/hyperlink" Target="https://example.test/note-target" TargetMode="External"/></Relationships>"#));
        let source=convert(&entries).unwrap();let document=crate::md_ast::parse(&source);
        let mut references=Vec::new();let mut links=Vec::new();
        crate::md_ast::walk_inlines(&document.blocks,&mut|inline|match inline{
            crate::md_ast::Inline::FootnoteRef(label)=>references.push(label.clone()),
            crate::md_ast::Inline::Link{href,children,..}=>links.push((href.clone(),crate::md_ast::inline_plain(children))),_=>(),
        });
        assert_eq!(references,["word-footnote-1","word-endnote-1","word-footnote-1"],"{source}");
        assert_eq!(links,[("https://example.test/note-target".into(),"Jump".into())]);
        let crate::md_ast::Block::Paragraph(inlines)=&document.blocks[0] else{panic!("{source}")};
        assert_eq!(crate::md_ast::inline_plain(inlines),"Before[word-footnote-1][word-endnote-1][word-footnote-1]Jump");
        let options=crate::publication_profile::resolve(&source,&serde_json::json!({})).unwrap();
        let output=crate::docx_writer::build(&document,options.values(),"notes-again.md",&|_|None);
        assert!(output.warns.is_empty(),"{:?}",output.warns);
        let native=std::str::from_utf8(&output.parts.iter().find(|(name,_)|name=="word/document.xml").unwrap().1).unwrap();
        assert_eq!(native.matches("<w:footnoteReference w:id=\"1\"").count(),2);
        assert_eq!(native.matches("<w:endnoteReference w:id=\"1\"").count(),1);
        assert!(native.contains("<w:hyperlink r:id="));
        let relationships=std::str::from_utf8(&output.parts.iter().find(|(name,_)|name=="word/_rels/document.xml.rels").unwrap().1).unwrap();
        assert!(relationships.contains("Target=\"https://example.test/note-target\" TargetMode=\"External\""));
    }

    #[test]
    fn docx_notes_handle_namespace_aliases_and_strict_wordprocessing_namespaces() {
        let mut entries = package(BODY,&note(false,"1","Foot evidence"),&note(true,"1","End evidence"));
        for (_,bytes) in entries.iter_mut().filter(|(name,_)|!name.ends_with(".rels")) {
            *bytes = String::from_utf8(bytes.clone()).unwrap().replace("w:","doc:").replace("xmlns:w=","xmlns:doc=").into_bytes();
        }
        assert!(convert(&entries).unwrap().contains("Foot evidence"));
        for (_,bytes) in &mut entries {
            *bytes = String::from_utf8(bytes.clone()).unwrap().replace(WORD,"http://purl.oclc.org/ooxml/wordprocessingml/main").replace(OFFICE_REL,"http://purl.oclc.org/ooxml/officeDocument/relationships").into_bytes();
        }
        assert!(convert(&entries).unwrap().contains("End evidence"));
    }

    #[test]
    fn docx_notes_skip_separator_orphan_and_deleted_content_but_keep_normal_zero_id() {
        let body = r#"<w:p><w:r><w:t>Claim</w:t><w:footnoteReference w:id="0"/></w:r><w:del><w:r><w:footnoteReference w:id="99"/></w:r></w:del></w:p>"#;
        let defs = format!(r#"<w:footnote w:id="-1" w:type="separator"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>{}{}"#,note(false,"0","Normal zero"),note(false,"7","Orphan private comment"));
        let markdown = convert(&package(body,&defs,"")).unwrap();
        assert!(markdown.contains("Normal zero")&&!markdown.contains("Orphan private comment")&&!markdown.contains("99"));
        let bad = body.replace("w:id=\"0\"","w:id=\"-1\"");
        assert_eq!(convert(&package(&bad,&defs,"")).unwrap_err(),INVALID);
    }

    #[test]
    fn docx_notes_reject_missing_duplicate_and_nested_definitions() {
        let mut cases = Vec::new();
        cases.push(package(BODY,"",&note(true,"1","End")));
        cases.push(package(BODY,&note(false,"2","Wrong ID"),&note(true,"1","End")));
        cases.push(package(BODY,&format!("{}{}",note(false,"1","A"),note(false,"01","B")),&note(true,"1","End")));
        let mut missing_part = package(BODY,&note(false,"1","Foot"),&note(true,"1","End"));
        missing_part.retain(|(name,_)|name!="word/footnotes.xml"); cases.push(missing_part);
        let mut duplicate_part = package(BODY,&note(false,"1","Foot"),&note(true,"1","End"));
        duplicate_part.push(duplicate_part.iter().find(|(name,_)|name=="word/footnotes.xml").unwrap().clone()); cases.push(duplicate_part);
        for entries in cases { assert_eq!(convert(&entries).unwrap_err(),INVALID); }
        let nested = note(false,"1",r#"Nested</w:t><w:footnoteReference w:id="2"/><w:t>reference"#);
        assert_eq!(convert(&package(BODY,&nested,&note(true,"1","End"))).unwrap_err(),crate::api_codes::DOCX_NESTED_FOOTNOTE);
    }

    #[test]
    fn docx_notes_reject_namespace_spoofing_and_malformed_xml() {
        let original = package(BODY,&note(false,"1","Foot"),&note(true,"1","End"));
        for (part,from,to) in [
            ("word/footnotes.xml",WORD,"urn:not-word"),
            ("word/document.xml",WORD,"urn:not-word"),
            ("word/footnotes.xml","w:id=\"1\"","id=\"1\""),
            ("word/footnotes.xml","</w:footnotes>",""),
            ("word/footnotes.xml","Foot","&unknown;"),
            ("word/footnotes.xml","<w:footnotes","<!DOCTYPE anything><w:footnotes"),
        ] {
            let mut entries=original.clone(); replace(&mut entries,part,from,to);
            assert_eq!(convert(&entries).unwrap_err(),INVALID,"{part}: {from}");
        }
    }

    #[test]
    fn docx_notes_reject_external_escaping_and_ambiguous_relationships() {
        let original = package(BODY,&note(false,"1","Foot"),&note(true,"1","End"));
        for target in ["../../../secret.xml","%2e%2e/%2e%2e/secret.xml","file:///private.xml","https://host/notes.xml","footnotes.xml#fragment","footnotes.xml?query"] {
            let mut entries=original.clone();replace(&mut entries,"word/_rels/document.xml.rels","Target=\"footnotes.xml\"",&format!("Target=\"{target}\""));
            assert_eq!(convert(&entries).unwrap_err(),INVALID,"{target}");
        }
        let mut external=original.clone();replace(&mut external,"word/_rels/document.xml.rels","Target=\"footnotes.xml\"","Target=\"footnotes.xml\" TargetMode=\"External\"");
        assert_eq!(convert(&external).unwrap_err(),INVALID);
        let mut duplicate=original.clone();replace(&mut duplicate,"word/_rels/document.xml.rels","Id=\"endnotes\"","Id=\"footnotes\"");
        assert_eq!(convert(&duplicate).unwrap_err(),INVALID);
    }

    #[test]
    fn docx_notes_resolve_nondefault_story_local_images_and_stage_until_explicit_save() {
        let data = tempfile::tempdir().unwrap(); let documents=tempfile::tempdir().unwrap();
        let body=r#"<w:p><w:r><w:t>Claim</w:t><w:footnoteReference w:id="1"/></w:r></w:p>"#;
        let content=format!(r#"<w:footnote w:id="1"><w:p><w:r><w:footnoteRef/><w:t>Evidence </w:t></w:r><w:hyperlink r:id="link"><w:r><w:t>source</w:t></w:r></w:hyperlink><w:r><w:t> before image </w:t><w:drawing><a:blip r:embed="image"/></w:drawing><w:t> after image</w:t></w:r></w:p></w:footnote>"#);
        let mut entries=package(body,&content,"");
        replace(&mut entries,"word/_rels/document.xml.rels","Target=\"footnotes.xml\"","Target=\"/word/notes/evidence.xml\"");
        replace(&mut entries,"word/footnotes.xml","xmlns:w=",&format!("xmlns:r=\"{OFFICE_REL}\" xmlns:a=\"{DRAWING}\" xmlns:w="));
        entries.iter_mut().find(|(name,_)|name=="word/footnotes.xml").unwrap().0="word/notes/evidence.xml".into();
        entries.push(("word/notes/_rels/evidence.xml.rels".into(),format!(r#"<Relationships xmlns="{REL}"><Relationship Id="link" Type="{OFFICE_REL}/hyperlink" Target="https://example.test/evidence?a=1&amp;b=2" TargetMode="External"/><Relationship Id="image" Type="{OFFICE_REL}/image" Target="../media/evidence.png"/></Relationships>"#).into_bytes()));
        let mut png=Cursor::new(Vec::new()); image::RgbaImage::from_pixel(4,4,image::Rgba([40,80,200,255])).write_to(&mut png,image::ImageFormat::Png).unwrap();
        entries.push(("word/media/evidence.png".into(),png.into_inner()));
        let source=documents.path().join("Evidence.docx");
        let (converted,assets,error)=crate::document_assets::with_staging(data.path(),||docx_convert(&entries,&source,true));
        assert!(error.is_none());let markdown=converted.unwrap();assert_eq!(assets.len(),1);
        assert!(markdown.contains("[source](https://example.test/evidence?a=1&b=2)"));
        assert!(markdown.find("before image").unwrap()<markdown.find("![image]").unwrap()&&markdown.find("![image]").unwrap()<markdown.find("after image").unwrap());
        assert_eq!(fs::read_dir(documents.path()).unwrap().count(),0);
        let output=documents.path().join("Evidence.md");
        let mut saved=crate::document_assets::materialize(&markdown,&output,&assets).unwrap();
        fs::write(&output,&saved.text).unwrap();saved.commit();
        let reopened=fs::read_to_string(output).unwrap();assert!(reopened.contains("readmd-assets/")&&!reopened.contains("/raw?p="));
        assert_eq!(fs::read_dir(documents.path().join("readmd-assets")).unwrap().count(),1);
    }

    #[test]
    fn docx_notes_rich_content_and_math_only_in_notes_survive_import_and_word_reexport() {
        let source="Body[^n]. Repeated[^n].\n\n[^n]: First **strong** note and [source](https://example.test).\n\n    > Quoted evidence.\n\n    3. Ordered evidence.\n    4. Second item.\n\n    | Measure | Value |\n    |---|---:|\n    | Accuracy | 98 |\n\n    ```rust\n    let evidence = 98;\n    ```\n\n    $x^2$\n";
        for position in ["page","document"] {
            let options=serde_json::json!({"footnotes":{"position":position}});
            let original=crate::docx_writer::build(&crate::md_ast::parse(source),&options,"Notes.md",&|_|None);
            assert!(original.warns.is_empty());
            let markdown=convert(&original.parts).unwrap();
            for text in ["**strong**","[source](https://example.test)","> Quoted evidence.","3. Ordered evidence.","4. Second item.","Measure","let evidence = 98;"] {
                assert!(markdown.contains(text),"missing {text}: {markdown}");
            }
            let defs=crate::md_ast::parse(&markdown).blocks.into_iter().filter_map(|b|if let crate::md_ast::Block::FootnoteDef{blocks,..}=b{Some(blocks)}else{None}).collect::<Vec<_>>();
            assert_eq!(defs.len(),1);assert!(defs[0].iter().any(|b|matches!(b,crate::md_ast::Block::Table{..})));assert!(defs[0].iter().any(|b|matches!(b,crate::md_ast::Block::Quote(_))));
            assert!(defs[0].iter().any(|b|matches!(b,crate::md_ast::Block::Code{lang,text} if lang=="rust" && text.trim_end()=="let evidence = 98;")),"{markdown}");
            let again=crate::docx_writer::build(&crate::md_ast::parse(&markdown),&options,"Again.md",&|_|None);
            assert!(again.warns.is_empty());
            let part=if position=="document"{"word/endnotes.xml"}else{"word/footnotes.xml"};
            let xml=String::from_utf8_lossy(find_zip_entry(&again.parts,part).unwrap());
            for text in ["Quoted evidence.","Ordered evidence.","<w:tbl>","<m:oMath","evidence"]{assert!(xml.contains(text),"{text}");}
            assert_eq!(convert(&again.parts).unwrap().matches("[^word-").count(),3);
        }
    }

    #[test]
    fn docx_notes_keep_empty_and_large_legal_notes() {
        let body=r#"<w:p><w:r><w:t>Claim</w:t><w:footnoteReference w:id="1"/></w:r></w:p>"#;
        let empty=convert(&package(body,&note(false,"1",""),"")).unwrap();
        assert!(crate::md_ast::parse(&empty).blocks.iter().any(|b|matches!(b,crate::md_ast::Block::FootnoteDef{..})),"{empty}");
        let text="Evidence ".repeat(140_000);
        let large=convert(&package(body,&note(false,"1",&text),"")).unwrap();
        assert_eq!(large.matches("Evidence").count(),140_000);
    }

    #[test]
    fn docx_notes_preserve_code_indentation_and_literal_backtick_fences() {
        let body=r#"<w:p><w:r><w:t>Code</w:t><w:footnoteReference w:id="1"/></w:r></w:p>"#;
        let definition=r#"<w:footnote w:id="1"><w:p><w:r><w:footnoteRef/></w:r></w:p><w:p><w:pPr><w:pStyle w:val="Code"/></w:pPr><w:r><w:t xml:space="preserve">    indented</w:t><w:br/><w:t>```</w:t></w:r></w:p></w:footnote>"#;
        let markdown=convert(&package(body,definition,"")).unwrap();
        let document=crate::md_ast::parse(&markdown);
        let blocks=document.blocks.iter().find_map(|b|if let crate::md_ast::Block::FootnoteDef{blocks,..}=b{Some(blocks)}else{None}).unwrap();
        assert!(blocks.iter().any(|b|matches!(b,crate::md_ast::Block::Code{text,..} if text.trim_end_matches('\n')=="    indented\n```")),"{markdown}");
    }

    #[test]
    fn docx_notes_keep_field_and_accepted_move_refs_and_reject_lost_references() {
        let body=r#"<w:p><w:fldSimple w:instr="REF example"><w:r><w:t>Field</w:t><w:footnoteReference w:id="1"/></w:r></w:fldSimple><w:moveTo><w:r><w:t>Moved</w:t><w:footnoteReference w:id="1"/></w:r></w:moveTo><w:moveFrom><w:r><w:footnoteReference w:id="99"/></w:r></w:moveFrom></w:p>"#;
        let entries=package(body,&note(false,"1","Evidence"),"");
        let markdown=convert(&entries).unwrap();
        let parsed=crate::md_ast::parse(&markdown);let expected=crate::md_ast::parse("Field[^word-footnote-1]Moved[^word-footnote-1]\n\n[^word-footnote-1]: Evidence\n");
        assert_eq!(parsed.blocks.first(),expected.blocks.first(),"{markdown}");
        let profile=crate::publication_profile::from_source(&markdown).unwrap();assert!(profile.diagnostics.is_empty());assert_eq!(profile.options["footnotes"]["kindByLabel"]["word-footnote-1"],"footnote");
        assert!(!markdown.contains("99"));
        let hidden=r#"<w:p><w:unsupported><w:r><w:footnoteReference w:id="1"/></w:r></w:unsupported></w:p>"#;
        assert_eq!(convert(&package(hidden,&note(false,"1","Evidence"),"")).unwrap_err(),INVALID);
    }

    #[test]
    fn docx_notes_invalid_public_conversion_never_uses_lossy_fallback() {
        let dir=tempfile::tempdir().unwrap();
        for (name,definition,expected) in [
            ("Missing",note(false,"2","Other"),INVALID),
            ("Nested",note(false,"1",r#"Nested</w:t><w:footnoteReference w:id="2"/><w:t>"#),crate::api_codes::DOCX_NESTED_FOOTNOTE),
        ] {
            let body=r#"<w:p><w:r><w:t>Claim</w:t><w:footnoteReference w:id="1"/></w:r></w:p>"#;
            let parts=package(body,&definition,"").into_iter().map(|(path,data)|crate::mdexport::ZipEntry{path,data,compress:false}).collect::<Vec<_>>();
            let bytes=crate::mdexport::write_zip(&parts).unwrap();
            let file=dir.path().join(format!("{name}.docx"));fs::write(&file,&bytes).unwrap();
            let result=convert_triple(file.to_str().unwrap(),true);
            assert_eq!(result.error.as_deref(),Some(expected));assert!(result.text.is_empty());
            assert_eq!(fs::read(&file).unwrap(),bytes);assert_eq!(fs::read_dir(dir.path()).unwrap().count(),if name=="Missing"{1}else{2});
        }
    }

    #[test]
    fn docx_notes_reject_encoded_xml_controls_and_invalid_target_modes() {
        for encoded in ["&#0;","&#1;","&#x1f;","&#xfffe;","&#xffff;"] {
            assert_eq!(xml(format!("<w:t xmlns:w=\"{WORD}\">{encoded}</w:t>").as_bytes()).unwrap_err(),INVALID);
            assert_eq!(xml(format!("<w:t xmlns:w=\"{WORD}\" w:val=\"{encoded}\"/>").as_bytes()).unwrap_err(),INVALID);
        }
        assert_eq!(xml(format!("<w:t xmlns:w=\"{WORD}\">&#9;&#10;&#13;α</w:t>").as_bytes()).unwrap().text,"\t\n\rα");
        let mut entries=package(BODY,&note(false,"1","Evidence"),&note(true,"1","Other"));
        replace(&mut entries,"word/_rels/document.xml.rels","Target=\"footnotes.xml\"","Target=\"footnotes.xml\" TargetMode=\"Unknown\"");
        assert_eq!(convert(&entries).unwrap_err(),INVALID);
    }
}
