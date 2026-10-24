//! Resolve only source-owned citation controls at their actual DOCX positions.
//! Unrecognized Word/Zotero/etc. fields retain the ordinary conversion path.
use super::*;
use crate::publication_docx_citations::{self as domain, Manifest, Record};
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
};

fn tag(node: &XmlNode) -> Option<&str> {
    if node.name != "w:sdt" {
        return None;
    }
    el_child(node, "w:sdtPr")
        .and_then(|p| el_child(p, "w:tag"))
        .and_then(|n| n.attr("w:val"))
}
pub(super) fn is_control(node: &XmlNode) -> bool {
    tag(node).is_some_and(|value| {
        value.starts_with("readmd:citation:") || value.starts_with("readmd:bibliography:")
    })
}
fn content(node: &XmlNode) -> Result<&XmlNode, String> {
    let mut found = node.children.iter().filter(|n| n.name == "w:sdtContent");
    let value = found.next().ok_or(domain::INVALID)?;
    if found.next().is_some() {
        return Err(domain::INVALID.into());
    }
    Ok(value)
}

/// Escape only actual Word text, before the converter adds its own rich
/// Markdown. Entities also keep publication directives and citation syntax
/// inert; an ampersand from Word is encoded once, never reinterpreted.
pub(super) fn literal_text(text: &str) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    let mut out = String::new();
    for (index, ch) in chars.iter().copied().enumerate() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '_' if index > 0
                && chars[index - 1].is_alphanumeric()
                && chars.get(index + 1).is_some_and(|c| c.is_alphanumeric()) =>
            {
                out.push(ch)
            }
            ch if ch.is_ascii_punctuation() => out.push_str(&format!("&#{};", ch as u32)),
            _ => out.push(ch),
        }
    }
    out
}
pub(super) fn literal_code(text: &str, block: bool) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat((longest + 1).max(if block { 3 } else { 1 }));
    if block {
        format!("{fence}\n{text}\n{fence}")
    } else {
        let pad = text.starts_with('`')
            || text.ends_with('`')
            || (text.starts_with(' ') && text.ends_with(' ') && !text.trim().is_empty());
        format!(
            "{fence}{}{text}{}{fence}",
            if pad { " " } else { "" },
            if pad { " " } else { "" }
        )
    }
}
pub(super) fn literal_link_target(url: &str) -> String {
    let mut out = String::new();
    for ch in url.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' | '>' | '\\' | ' ' | '\t' | '\r' | '\n' => {
                for byte in ch.to_string().bytes() {
                    out.push_str(&format!("%{byte:02X}"));
                }
            }
            _ => out.push(ch),
        }
    }
    format!("<{out}>")
}
fn namespaces(root: &XmlNode) -> Vec<(String, String)> {
    root.attrs
        .iter()
        .filter(|(k, _)| k == "xmlns" || k.starts_with("xmlns:"))
        .cloned()
        .collect()
}
fn fragment(node: &XmlNode, namespaces: &[(String, String)]) -> String {
    node.children
        .iter()
        .map(|n| docx_xml_fragment(n, namespaces))
        .collect()
}
fn collect<'a>(node: &'a XmlNode, out: &mut Vec<&'a XmlNode>) {
    if matches!(node.name.as_str(), "w:del" | "w:moveFrom" | "w:sdtPr") {
        return;
    }
    if is_control(node) {
        out.push(node);
    }
    for child in &node.children {
        collect(child, out);
    }
}
fn generated_reference_identity(node:&XmlNode)->Option<(bool,i32)>{
    fn boundary(node:&XmlNode)->bool{matches!(node.name.as_str(),"w:bookmarkStart"|"w:bookmarkEnd"|"w:proofErr"|"w:lastRenderedPageBreak")&&node.children.is_empty()&&node.text.trim().is_empty()}
    let content=content(node).ok()?;
    if !content.text.trim().is_empty()||content.children.iter().any(|node|!node.tail.trim().is_empty()){return None;}
    let children=content.children.iter().filter(|node|!boundary(node)).collect::<Vec<_>>();
    let [run]=children.as_slice()else{return None};
    if run.name!="w:r"||!run.text.trim().is_empty()||run.children.iter().any(|node|!node.tail.trim().is_empty())||run.children.iter().filter(|node|node.name=="w:rPr").count()>1{return None;}
    let children=run.children.iter().filter(|node|node.name!="w:rPr"&&!boundary(node)).collect::<Vec<_>>();
    let [reference]=children.as_slice()else{return None};
    let end=match reference.name.as_str(){"w:footnoteReference"=>false,"w:endnoteReference"=>true,_=>return None};
    if !reference.children.is_empty(){return None;}
    Some((end,reference.attr("w:id")?.parse().ok()?))
}
struct SignatureStory {
    root: XmlNode,
    namespaces: Vec<(String,String)>,
    notes: HashMap<i32,usize>,
}
/// One pass owns all relationship/payload budgets. Notes and their SDTs share
/// that context; even failed stories and note digests are memoized.
pub(crate) struct PackageSignatures<'a> {
    entries: &'a [(String,Vec<u8>)],
    context: domain::ContextSignatures<'a>,
    stories: RefCell<HashMap<String,Result<SignatureStory,String>>>,
    notes: RefCell<HashMap<(bool,i32),Result<String,String>>>,
    #[cfg(test)] story_parses: Cell<usize>,
    #[cfg(test)] note_digests: Cell<usize>,
}
impl<'a> PackageSignatures<'a> {
    pub(crate) fn new(entries:&'a [(String,Vec<u8>)])->Self {
        Self{entries,context:domain::ContextSignatures::new(entries),stories:RefCell::new(HashMap::new()),notes:RefCell::new(HashMap::new()),
            #[cfg(test)] story_parses:Cell::new(0),#[cfg(test)] note_digests:Cell::new(0)}
    }
    fn with_story<T>(&self,part:&str,operation:impl FnOnce(&SignatureStory)->Result<T,String>)->Result<T,String> {
        if !matches!(part,"word/document.xml"|"word/footnotes.xml"|"word/endnotes.xml"){return Err(domain::INVALID.into());}
        if !self.stories.borrow().contains_key(part) {
            let parsed=(|| {
                let bytes=docx_notes::unique(self.entries,part).map_err(|_|domain::INVALID.to_string())?;
                #[cfg(test)] self.story_parses.set(self.story_parses.get()+1);
                let root=docx_notes::xml(bytes)?;
                let expected=match part{"word/footnotes.xml"=>"w:footnotes","word/endnotes.xml"=>"w:endnotes",_=>"w:document"};
                if root.name!=expected{return Err(domain::INVALID.into());}
                let mut notes=HashMap::new();
                if part!="word/document.xml" {
                    let expected=if part=="word/endnotes.xml"{"w:endnote"}else{"w:footnote"};
                    for (index,note) in root.children.iter().enumerate() {
                        if note.name!=expected{return Err(domain::INVALID.into());}
                        let id=note.attr("w:id").and_then(|value|value.parse::<i32>().ok()).ok_or(domain::INVALID)?;
                        if notes.insert(id,index).is_some(){return Err(domain::INVALID.into());}
                    }
                }
                let namespaces=namespaces(&root);Ok(SignatureStory{root,namespaces,notes})
            })();
            self.stories.borrow_mut().insert(part.into(),parsed);
        }
        let stories=self.stories.borrow();
        operation(stories.get(part).ok_or(domain::INVALID)?.as_ref().map_err(Clone::clone)?)
    }
    pub(crate) fn bindings(&self)->Result<HashMap<String,String>,String> {
        let mut out=HashMap::new();
        for part in ["word/document.xml","word/footnotes.xml","word/endnotes.xml"] {
            if !self.entries.iter().any(|(name,_)|name==part){continue;}
            self.with_story(part,|story| {
                let mut controls=Vec::new();collect(&story.root,&mut controls);
                for node in controls {
                    let value=tag(node).ok_or(domain::INVALID)?.to_string();
                    let signature=self.context.signature(part,&fragment(content(node)?,&story.namespaces))?;
                    if out.insert(value,signature).is_some(){return Err(format!("{}: duplicate_binding:{part}",domain::INVALID));}
                }
                Ok(())
            })?;
        }
        Ok(out)
    }
    pub(crate) fn note(&self,end:bool,id:i32)->Result<String,String> {
        if let Some(value)=self.notes.borrow().get(&(end,id)){return value.clone();}
        let part=if end{"word/endnotes.xml"}else{"word/footnotes.xml"};
        let value=self.with_story(part,|story| {
            let index=*story.notes.get(&id).ok_or(domain::INVALID)?;
            #[cfg(test)] self.note_digests.set(self.note_digests.get()+1);
            self.context.signature(part,&docx_xml_fragment(&story.root.children[index],&story.namespaces))
        });
        self.notes.borrow_mut().insert((end,id),value.clone());value
    }
}
#[cfg(test)]
pub(crate) fn binding_signatures(entries:&[(String,Vec<u8>)])->Result<HashMap<String,String>,String> {
    PackageSignatures::new(entries).bindings()
}
#[cfg(test)]
pub(crate) fn note_signature(entries:&[(String,Vec<u8>)],end:bool,id:i32)->Result<String,String> {
    PackageSignatures::new(entries).note(end,id)
}

#[derive(Default)]
struct GeneratedAuthorFamily{
    incompatible:HashSet<(bool,i32)>,
    // In this finite mode every generated pair becomes an actual authored
    // note. Keeping only selected pairs would change the family's note order.
    authored:HashMap<(bool,i32),String>,
}
#[derive(Default)]
pub(super) struct Citations {
    manifest: Option<Manifest>,
    counts: HashMap<String, usize>,
    signatures: HashMap<String, String>,
    generated: HashMap<String, String>,
    reference_only: HashMap<String,(bool,i32)>,
    // One bounded decision for the whole source-owned citation family. A
    // manual fallback must not silently renumber its remaining citations or
    // remove its current bibliography, even if that bibliography comes first.
    generated_author_family: RefCell<Option<Result<GeneratedAuthorFamily,String>>>,
    diagnostics: RefCell<Vec<Value>>,
    manual_bibliography: Cell<bool>,
    seen: RefCell<HashSet<String>>,
    known: HashSet<String>,
    literal: Cell<bool>,
    #[cfg(test)] signature_work: (usize,usize,usize,usize),
}
impl Citations {
    pub(super) fn literal_mode(&self) -> bool {
        self.literal.get()
    }
    fn with_literal<T>(&self, operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        struct Restore<'a>(&'a Cell<bool>, bool);
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                self.0.set(self.1);
            }
        }
        let _restore = Restore(&self.literal, self.literal.replace(true));
        operation()
    }
    pub(super) fn load(entries: &[(String, Vec<u8>)], body: &XmlNode, rels: &[DocxRel]) -> Self {
        let mut out = Self::default();
        let related=rels.iter().filter(|r|r.kind==format!("http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml")&&r.mode!="External"&&docx_notes::member("word",&r.raw).ok().as_deref()==Some(domain::PART)).collect::<Vec<_>>();
        let parts = entries
            .iter()
            .filter(|(name, _)| name == domain::PART)
            .collect::<Vec<_>>();
        if related.is_empty() && parts.is_empty() {
            return out;
        }
        let loaded = if related.len() != 1 || parts.len() != 1 {
            Err(domain::INVALID.to_string())
        } else {
            Manifest::parse(&parts[0].1)
        };
        let manifest = match loaded {
            Ok(m) => m,
            Err(error) => {
                out.diagnostics.borrow_mut().push(json!({"code":error,"reason":"citation_source_package_invalid","visible_content_preserved":true}));
                return out;
            }
        };
        if let Some(resources) = &manifest.resources {
            let data = if resources
                .files
                .iter()
                .find(|f| resources.bibliography.contains(&f.name))
                .is_some_and(|f| f.kind == "csl-json")
            {
                let parsed = resources
                    .bibliography
                    .iter()
                    .map(|name| {
                        serde_json::from_str::<Vec<Value>>(resources.read(name).unwrap_or(""))
                    })
                    .collect::<Result<Vec<_>, _>>();
                parsed
                    .ok()
                    .and_then(|items| {
                        serde_json::to_string(&items.into_iter().flatten().collect::<Vec<_>>()).ok()
                    })
                    .map(|data| ("csl-json", data))
            } else {
                Some((
                    "bibtex",
                    resources
                        .bibliography
                        .iter()
                        .filter_map(|name| resources.read(name))
                        .collect::<Vec<_>>()
                        .join("\n"),
                ))
            };
            match data
                .and_then(|(format, data)| crate::academic::reference_keys(format, &data).ok())
            {
                Some(known) => out.known = known,
                None => {
                    out.diagnostics.borrow_mut().push(json!({"code":domain::RESOURCE_INVALID,"reason":"citation_bibliography_invalid","visible_content_preserved":true}));
                    return out;
                }
            }
        }
        out.manual_bibliography.set(
            manifest
                .resources
                .as_ref()
                .is_some_and(|r| r.manual_bibliography),
        );
        let signatures=PackageSignatures::new(entries);
        for path in ["word/document.xml","word/footnotes.xml","word/endnotes.xml"] {
            let _=signatures.with_story(path,|story| {
                let mut controls=Vec::new();collect(&story.root,&mut controls);
                for node in controls {
                    let Some(value)=tag(node) else{continue;};
                    *out.counts.entry(value.to_string()).or_default()+=1;
                    if let Ok(content)=content(node) {
                        if let Ok(signature)=signatures.context.signature(path,&fragment(content,&story.namespaces)) {
                            out.signatures.insert(format!("{path}\0{value}"),signature);
                        }
                    }
                }
                Ok(())
            });
        }
        let mut references = HashMap::<(bool, i32), usize>::new();
        for node in std::iter::once(body).chain(body.descendants().into_iter()) {
            if let Some(value)=tag(node){if let Some(identity)=generated_reference_identity(node){out.reference_only.insert(value.into(),identity);}}
            let end = match node.name.as_str() {
                "w:footnoteReference" => false,
                "w:endnoteReference" => true,
                _ => continue,
            };
            if let Some(id) = node.attr("w:id").and_then(|v| v.parse().ok()) {
                *references.entry((end, id)).or_default() += 1;
            }
        }
        out.manifest = Some(manifest);
        let manifest = out.manifest.as_ref().unwrap();
        for anchor in manifest
            .records
            .iter()
            .filter(|r| r.generated && r.part == "word/document.xml")
        {
            let Some(note) = &anchor.note else {
                continue;
            };
            if references.get(&(note.end, note.id)) != Some(&1) || !out.unchanged(anchor) {
                continue;
            }
            let anchors = manifest
                .records
                .iter()
                .filter(|r| {
                    r.generated
                        && r.part == "word/document.xml"
                        && r.note
                            .as_ref()
                            .is_some_and(|n| n.end == note.end && n.id == note.id)
                })
                .count();
            if anchors != 1 {
                continue;
            }
            let notes = manifest
                .records
                .iter()
                .filter(|r| {
                    r.generated
                        && r.part != "word/document.xml"
                        && r.note
                            .as_ref()
                            .is_some_and(|n| n.end == note.end && n.id == note.id)
                        && r.original_source == anchor.original_source
                })
                .collect::<Vec<_>>();
            if notes.len() != 1 || !out.unchanged(notes[0]) {
                continue;
            }
            let note_record = notes[0];
            let current = signatures.note(note.end, note.id).ok();
            if current.as_deref() != Some(note.story_sha256.as_str())
                || note_record
                    .note
                    .as_ref()
                    .is_none_or(|n| n.story_sha256 != note.story_sha256)
            {
                continue;
            }
            let chosen = if anchor.source == note_record.source {
                Some(anchor.source.as_str())
            } else if anchor.source == anchor.original_source {
                Some(note_record.source.as_str())
            } else if note_record.source == note_record.original_source {
                Some(anchor.source.as_str())
            } else {
                None
            };
            if let Some(source) = chosen.filter(|source| out.valid_source(source)) {
                out.generated.insert(anchor.tag(), source.into());
            }
        }
        #[cfg(test)] {
            let (payloads,bytes)=signatures.context.payload_work();
            out.signature_work=(signatures.story_parses.get(),signatures.note_digests.get(),payloads,bytes);
        }
        out
    }
    fn generated_author_family_reason(&self,record:&Record,pkg:&DocxPackage)->Result<Option<&'static str>,String>{
        if self.generated_author_family.borrow().is_none(){
            let result=(||->Result<GeneratedAuthorFamily,String>{
                let mut checked=HashSet::new();let mut incompatible=HashSet::new();
                if let Some(manifest)=&self.manifest{for anchor in manifest.records.iter().filter(|anchor|anchor.generated&&anchor.part=="word/document.xml"&&self.generated.contains_key(&anchor.tag())){
                    let Some(note)=&anchor.note else{continue};let identity=(note.end,note.id);if !checked.insert(identity){continue;}
                    let Some((part,node))=pkg.notes.settings_note(note.end,note.id)else{continue};
                    if !pkg.text_styles.generated_note_author_reproducible(part,node).map_err(|error|error.to_string())?{incompatible.insert(identity);}
                }}
                if incompatible.is_empty(){return Ok(GeneratedAuthorFamily::default());}
                // This new admission is all-or-nothing. Keep the old literal
                // family when any signature, pair, source mode or native
                // control cannot prove the authored-note representation.
                let mut authored=HashMap::new();let mut claimed=HashSet::new();let mut safe=true;
                if let Some(manifest)=&self.manifest{
                    safe&=manifest.resources.as_ref().is_some_and(|resources|!resources.manual_bibliography
                        &&crate::academic::authored_note_recovery_equivalent(&resources.style,
                            if resources.style.to_ascii_lowercase().ends_with(".csl"){resources.read(&resources.style)}else{None}).unwrap_or(false));
                    for current in &manifest.records{
                        safe&=self.unchanged(current);
                        if current.kind=="citation"{safe&=self.valid_source(&current.source);}
                    }
                    for anchor in manifest.records.iter().filter(|anchor|anchor.generated&&anchor.part=="word/document.xml"){
                        if crate::speech::cancelled(){return Err("cancelled".into());}
                        let Some(note)=&anchor.note else{safe=false;continue};let identity=(note.end,note.id);
                        let Some(source)=self.generated.get(&anchor.tag())else{safe=false;continue};
                        if self.reference_only.get(&anchor.tag())!=Some(&identity)
                            ||!self.normal_note_source(source)||!self.normal_note_source(&anchor.original_source){safe=false;continue;}
                        let part=if note.end{"word/endnotes.xml"}else{"word/footnotes.xml"};
                        let records=manifest.records.iter().filter(|current|current.generated&&current.kind=="citation"&&current.part==part
                            &&current.original_source==anchor.original_source&&current.note.as_ref().is_some_and(|value|(value.end,value.id)==identity)).collect::<Vec<_>>();
                        let [note_record]=records.as_slice()else{safe=false;continue};
                        let Some((actual_part,node))=pkg.notes.settings_note(note.end,note.id)else{safe=false;continue};
                        let mut controls=Vec::new();collect(node,&mut controls);
                        let controls=controls.into_iter().filter(|node|tag(node)==Some(note_record.tag().as_str())).collect::<Vec<_>>();
                        let [control]=controls.as_slice()else{safe=false;continue};
                        if pkg.text_styles.generated_note_authored_style(actual_part,node,control).map_err(|error|error.to_string())?.is_none(){safe=false;continue;}
                        if !claimed.insert(anchor.tag())||!claimed.insert(note_record.tag())||authored.insert(identity,source.clone()).is_some(){safe=false;}
                    }
                    safe&=claimed.len()==manifest.records.iter().filter(|record|record.generated).count();
                }else{safe=false;}
                if safe&&!authored.is_empty(){Ok(GeneratedAuthorFamily{incompatible:HashSet::new(),authored})}
                else{Ok(GeneratedAuthorFamily{incompatible,authored:HashMap::new()})}
            })();
            if result.as_ref().is_ok_and(|family|!family.incompatible.is_empty()){self.manual_bibliography.set(true);}
            *self.generated_author_family.borrow_mut()=Some(result);
        }
        let cached=self.generated_author_family.borrow();let family=cached.as_ref().expect("initialized generated author family").as_ref().map_err(Clone::clone)?;
        if family.incompatible.is_empty(){return Ok(None);}
        Ok(Some(if record.generated&&record.note.as_ref().is_some_and(|note|family.incompatible.contains(&(note.end,note.id))){"generated_citation_note_author_format_not_reproducible"}else{"citation_family_author_format_not_reproducible"}))
    }
    fn normal_note_source(&self,source:&str)->bool{
        if !source.starts_with('[')||!self.valid_source(source){return false;}
        let locale=self.manifest.as_ref().and_then(|manifest|manifest.resources.as_ref()).map(|resources|resources.locale.as_str()).unwrap_or("en-US");
        crate::publication::citations::scan(source,locale).is_ok_and(|scan|scan.citations[0].items.iter().all(|item|item.cite.mode==crate::academic::CiteMode::Normal))
    }
    fn unchanged(&self, record: &Record) -> bool {
        record.render_context_version == domain::RENDER_CONTEXT_VERSION
            && self.counts.get(&record.tag()) == Some(&1)
            && self
                .signatures
                .get(&format!("{}\0{}", record.part, record.tag()))
                == Some(&record.render_sha256)
    }
    fn valid_source(&self, source: &str) -> bool {
        let locale = self
            .manifest
            .as_ref()
            .and_then(|m| m.resources.as_ref())
            .map(|r| r.locale.as_str())
            .unwrap_or("en-US");
        let Ok(scan) = crate::publication::citations::scan(source, locale) else {
            return false;
        };
        scan.diagnostics.is_empty()
            && scan.citations.len() == 1
            && scan.citations[0].span.byte_from == 0
            && scan.citations[0].span.byte_to == source.len()
            && (self.manifest.as_ref().is_none_or(|m| m.resources.is_none())
                || scan.citations[0]
                    .items
                    .iter()
                    .all(|item| self.known.contains(&item.cite.key)))
    }
    fn record(&self, node: &XmlNode, pkg: &DocxPackage) -> Option<&Record> {
        self.record_any_story(node)
            .filter(|record| record.part == pkg.part)
    }
    fn record_any_story(&self, node: &XmlNode) -> Option<&Record> {
        let value = tag(node)?;
        self.manifest
            .as_ref()?
            .records
            .iter()
            .find(|record| record.tag() == value)
    }
    fn diagnose(&self, node: &XmlNode, pkg: &DocxPackage, reason: &str) {
        let value = tag(node).unwrap_or("unknown");
        if !self.seen.borrow_mut().insert(format!("{value}\0{reason}")) {
            return;
        }
        let mut diagnostic = json!({"code":domain::CONFLICT,"reason":reason,"binding":value,"part":pkg.part,"visible_content_preserved":true});
        if let Some(record) = self.record_any_story(node) {
            diagnostic["original_source"] = json!(record.original_source);
            diagnostic["current_source"] = json!(record.source);
            diagnostic["source_revision"] = json!(self.manifest.as_ref().unwrap().source_revision);
            if let Some(span) = &record.span {
                diagnostic["origin_span"] = json!(span);
            }
        }
        self.diagnostics.borrow_mut().push(diagnostic);
    }
    fn restore(&self, node: &XmlNode, pkg: &DocxPackage) -> Result<Option<String>,String> {
        if self
            .record_any_story(node)
            .is_some_and(|record| record.part != pkg.part)
        {
            self.diagnose(node, pkg, "citation_binding_story_changed");
            return Ok(None);
        }
        let Some(record) = self.record(node, pkg) else {
            self.diagnose(node, pkg, "citation_binding_missing_or_unsupported");
            return Ok(None);
        };
        if self.counts.get(&record.tag()) != Some(&1) {
            self.diagnose(node, pkg, "duplicate_citation_binding");
            return Ok(None);
        }
        if record.render_context_version != domain::RENDER_CONTEXT_VERSION {
            self.diagnose(node, pkg, "citation_render_signature_unsupported");
            return Ok(None);
        }
        if !self.unchanged(record) {
            self.diagnose(
                node,
                pkg,
                if record.source != record.original_source {
                    "citation_source_and_render_modified"
                } else {
                    "citation_render_modified"
                },
            );
            return Ok(None);
        }
        if let Some(reason)=self.generated_author_family_reason(record,pkg)?{
            self.diagnose(node,pkg,reason);
            return Ok(None);
        }
        if record.kind == "bibliography" {
            if self
                .manifest
                .as_ref()
                .is_some_and(|m| m.resources.is_some())
            {
                return Ok(Some("<!--readmd:bibliography-->".into()));
            }
            self.diagnose(node, pkg, "bibliography_resources_unavailable");
            return Ok(None);
        }
        if record.generated {
            let authored={
                let cached=self.generated_author_family.borrow();
                cached.as_ref().and_then(|result|result.as_ref().ok()).and_then(|family|record.note.as_ref().and_then(|note|family.authored.get(&(note.end,note.id)))).cloned()
            };
            if let Some(source)=authored{
                // Render the real anchor/reference and let Notes retain this
                // definition. Only its actual note SDT restores the active
                // key. The shared inline hook applies exactly one current
                // AuthorStyled span, and the next export truthfully records
                // generated=false / context.in_note=true.
                return Ok((record.part!="word/document.xml").then_some(source));
            }
            if let Some(source) = self.generated.get(&record.tag()) {
                if let Some(note) = &record.note {
                    if pkg.notes.suppress_generated(note.end, note.id).is_ok() {
                        return Ok(Some(source.clone()));
                    }
                }
            }
            self.diagnose(node, pkg, "generated_citation_note_modified_or_unbound");
            return Ok(None);
        }
        if !self.valid_source(&record.source) {
            self.diagnose(node, pkg, "citation_source_invalid_or_unknown_key");
            return Ok(None);
        }
        Ok(Some(record.source.clone()))
    }
    pub(super) fn inline(
        &self,
        node: &XmlNode,
        pkg: &DocxPackage,
        paragraph: &XmlNode,
    ) -> Result<String, String> {
        if !pkg.fields.visible() {
            return docx_para_inline_context(content(node)?, pkg, true, paragraph);
        }
        if let Some(source) = self.restore(node, pkg)? {
            return Ok(source);
        }
        self.with_literal(|| match content(node) {
            Ok(content) => docx_para_inline_context(content, pkg, true, paragraph),
            Err(_) => Ok(literal_text(&node.concat_text_of("t"))),
        })
    }
    pub(super) fn block(
        &self,
        node: &XmlNode,
        pkg: &DocxPackage,
        styles: &DocxStyles,
        form_tables: bool,
    ) -> Result<String, String> {
        if let Some(source) = self.restore(node, pkg)? {
            return Ok(source);
        }
        if tag(node).is_some_and(|v| v.starts_with("readmd:bibliography:")) {
            self.manual_bibliography.set(true);
        }
        let Ok(content) = content(node) else {
            return Ok(literal_text(&node.concat_text_of("t")));
        };
        self.with_literal(|| {
            let (mut lines, mut code, mut language, mut ordered) =
                (Vec::new(), Vec::new(), String::new(), 0);
            for child in docx_word_children(content) {
                match child.name.as_str() {
                    "w:p" => docx_handle_para(
                        child,
                        pkg,
                        styles,
                        &mut lines,
                        &mut code,
                        &mut language,
                        &mut ordered,
                    )?,
                    "w:tbl" => {
                        docx_flush_code(&mut lines, &mut code, &mut language);
                        lines.push(docx_table_to_md(child, pkg, form_tables)?);
                        lines.push(String::new());
                    }
                    "w:sdt" if is_control(child) => {
                        docx_flush_code(&mut lines, &mut code, &mut language);
                        lines.push(self.block(child, pkg, styles, form_tables)?);
                        lines.push(String::new());
                    }
                    _ => (),
                }
            }
            docx_flush_code(&mut lines, &mut code, &mut language);
            Ok(lines.join("\n").trim().to_string())
        })
    }
    pub(super) fn finish(&self, text: String) -> Result<String, String> {
        if let Some(manifest) = &self.manifest {
            for record in manifest
                .records
                .iter()
                .filter(|record| !self.counts.contains_key(&record.tag()))
            {
                if record.kind == "bibliography" {
                    self.manual_bibliography.set(true);
                }
                let reason = if record.kind == "bibliography" {
                    "bibliography_control_removed"
                } else {
                    "citation_control_removed"
                };
                if self
                    .seen
                    .borrow_mut()
                    .insert(format!("{}\0{reason}", record.tag()))
                {
                    let mut diagnostic = json!({"code":domain::CONFLICT,"reason":reason,"binding":record.tag(),"part":record.part,"visible_content_preserved":true,"original_source":record.original_source,"current_source":record.source,"source_revision":manifest.source_revision});
                    if let Some(span) = &record.span {
                        diagnostic["origin_span"] = json!(span);
                    }
                    self.diagnostics.borrow_mut().push(diagnostic);
                }
            }
        }
        let mut out = String::new();
        if let Some(resources) = self.manifest.as_ref().and_then(|m| m.resources.as_ref()) {
            let mut resources = resources.clone();
            resources.manual_bibliography = self.manual_bibliography.get();
            out.push_str(&resources.markdown_capsule()?);
            out.push('\n');
        }
        out.push_str(&text);
        if !self.diagnostics.borrow().is_empty() {
            let diagnostics = serde_json::to_string_pretty(
                &json!({"version":1,"diagnostics":*self.diagnostics.borrow()}),
            )
            .map_err(|_| domain::INVALID)?
            .replace('&', "\\u0026")
            .replace('<', "\\u003c")
            .replace('>', "\\u003e");
            out.push_str("\n\n<!--readmd:citation-import:v1\n");
            out.push_str(&diagnostics);
            out.push_str("\n-->\n");
        }
        Ok(out)
    }
}

#[cfg(test)]
#[path = "docx_citations_tests.rs"]
mod tests;
