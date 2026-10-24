//! Source-owned DOCX citation bindings. Only citation fragments and their real
//! bibliography/CSL inputs travel with the package; the document body does not.
use crate::{
    academic::Cite,
    publication_rich::{SourceCitation, SourceSpan},
};
use pulldown_cmark::{Event as MdEvent, Options, Parser, Tag, TagEnd};
use quick_xml::{events::Event, name::ResolveResult, NsReader, XmlVersion};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    io::Read,
    path::Path,
};

pub const INVALID: &str = "docx_citation_invalid";
pub const CONFLICT: &str = "docx_citation_conflict";
pub const RESOURCE_INVALID: &str = "docx_citation_resources_invalid";
pub const MAX_RESOURCES: usize = 2 * 1024 * 1024;
pub const NS: &str = "urn:readmd:citations:1";
pub const PART: &str = "customXml/readmd-citations.xml";
pub const TAG: &str = "readmd:citation:v1:";
pub const BIB_TAG: &str = "readmd:bibliography:v1:";
pub const RENDER_CONTEXT_VERSION: u32 = 2;
const CAPSULE: &str = "<!--readmd:citation-resources:v1\n";
const CAPSULE_FAMILY: &str = "<!--readmd:citation-resources:";
const IMPORT_ANNOTATION: &str = "<!--readmd:citation-import:v1\n";
const IMPORT_FAMILY: &str = "<!--readmd:citation-import:";
const W: &str = crate::math_document::WORD_NS;
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PKG: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

fn digest(text: &str) -> String {
    crate::document_transaction::revision(text)
}
fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn valid_text(value: &str) -> bool {
    !value.chars().any(|c| {
        !matches!(c, '\t' | '\r' | '\n') && (c.is_control() || matches!(c, '\u{fffe}' | '\u{ffff}'))
    })
}
fn err() -> String {
    INVALID.into()
}
fn resource_err() -> String {
    RESOURCE_INVALID.into()
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ImportDiagnostic {
    code: String,
    reason: String,
    visible_content_preserved: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    binding: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    part: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    original_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    current_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    origin_span: Option<SourceSpan>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportAnnotation {
    version: u32,
    diagnostics: Vec<ImportDiagnostic>,
}
impl ImportDiagnostic {
    fn valid(&self) -> bool {
        matches!(self.code.as_str(), INVALID | CONFLICT | RESOURCE_INVALID)
            && !self.reason.is_empty()
            && self.reason.len() <= 128
            && self
                .reason
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            && self.visible_content_preserved
            && self
                .binding
                .as_ref()
                .is_none_or(|v| v.len() <= 256 && valid_text(v))
            && self.part.as_ref().is_none_or(|v| {
                matches!(
                    v.as_str(),
                    "word/document.xml" | "word/footnotes.xml" | "word/endnotes.xml"
                )
            })
            && [&self.original_source, &self.current_source]
                .iter()
                .all(|v| v.as_ref().is_none_or(|v| v.len() <= 65536 && valid_text(v)))
            && self
                .source_revision
                .as_ref()
                .is_none_or(|v| v.is_empty() || valid_digest(v))
            && self.origin_span.as_ref().is_none_or(|span| {
                span.source.len() <= 65536
                    && valid_text(&span.source)
                    && span.byte_to.checked_sub(span.byte_from) == Some(span.source.len())
                    && span.to.checked_sub(span.from) == Some(span.source.encode_utf16().count())
            })
    }
}

/// Pulldown emits a block comment as one HTML event per line. Read the
/// original range of the whole HTML block instead, without interpreting
/// comments inside raw HTML examples or Markdown containers as metadata.
pub(crate) fn source_comments(source: &str, family: &str, limit: usize) -> Vec<std::ops::Range<usize>> {
    fn collect(
        source: &str,
        range: std::ops::Range<usize>,
        family: &str,
        limit: usize,
        output: &mut Vec<std::ops::Range<usize>>,
    ) {
        let mut cursor = range.start;
        while cursor < range.end && output.len() < limit {
            let remaining = &source[cursor..range.end];
            cursor += remaining.len() - remaining.trim_start().len();
            let remaining = &source[cursor..range.end];
            if !remaining.starts_with("<!--") {
                break;
            }
            let end = remaining
                .find("-->")
                .map(|offset| cursor + offset + 3)
                .unwrap_or(range.end);
            if remaining.starts_with(family) {
                output.push(cursor..end);
            }
            cursor = end;
        }
    }
    let options = Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_MATH;
    let mut output = Vec::new();
    let mut nested = 0usize;
    let mut html_block = false;
    let mut html_stack = Vec::<String>::new();
    for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
        match event {
            MdEvent::Start(
                Tag::BlockQuote(_)
                | Tag::List(_)
                | Tag::Item
                | Tag::FootnoteDefinition(_)
                | Tag::CodeBlock(_)
                | Tag::MetadataBlock(_),
            ) => nested += 1,
            MdEvent::End(
                TagEnd::BlockQuote(_)
                | TagEnd::List(_)
                | TagEnd::Item
                | TagEnd::FootnoteDefinition
                | TagEnd::CodeBlock
                | TagEnd::MetadataBlock(_),
            ) => nested = nested.saturating_sub(1),
            MdEvent::Start(Tag::HtmlBlock) => {
                html_block = true;
                if nested == 0 && html_stack.is_empty() {
                    collect(source, range, family, limit, &mut output);
                }
            }
            MdEvent::End(TagEnd::HtmlBlock) => html_block = false,
            MdEvent::InlineHtml(text) if !html_block && nested == 0 => {
                let text = text.trim();
                if text.starts_with("<!--") {
                    if html_stack.is_empty() {
                        collect(source, range, family, limit, &mut output);
                    }
                    continue;
                }
                let closing = text.starts_with("</");
                let tag = text
                    .trim_start_matches('<')
                    .trim_start_matches('/')
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                    .collect::<String>()
                    .to_ascii_lowercase();
                if tag.is_empty() {
                    continue;
                }
                if closing {
                    if let Some(index) = html_stack.iter().rposition(|name| name == &tag) {
                        html_stack.truncate(index);
                    }
                } else if !text.ends_with("/>")
                    && !matches!(
                        tag.as_str(),
                        "area"
                            | "base"
                            | "br"
                            | "col"
                            | "embed"
                            | "hr"
                            | "img"
                            | "input"
                            | "link"
                            | "meta"
                            | "param"
                            | "source"
                            | "track"
                            | "wbr"
                    )
                {
                    html_stack.push(tag);
                }
            }
            _ => (),
        }
        if output.len() >= limit {
            break;
        }
    }
    output
}

fn comment_payload<'a>(text: &'a str, header: &str) -> Option<&'a str> {
    let raw = text.strip_prefix(header.strip_suffix('\n')?)?;
    let raw = raw
        .strip_prefix("\r\n")
        .or_else(|| raw.strip_prefix('\n'))?;
    raw.strip_suffix("-->")
        .filter(|raw| !raw.contains("<!--") && !raw.contains("-->"))
}

/// Read bounded, source-preserving import diagnostics from actual top-level
/// Markdown comments. Prose/code/examples never become recovery instructions.
/// Malformed annotations remain in the source and yield a localized warning.
pub fn import_diagnostics(source: &str) -> Vec<Value> {
    fn invalid_annotation(source: &str, range: std::ops::Range<usize>, reason: &str) -> Value {
        json!({"code":INVALID,"reason":reason,"visible_content_preserved":true,"source_preserved":true,
            "from":source[..range.start].encode_utf16().count(),"to":source[..range.end].encode_utf16().count()})
    }
    let mut output = Vec::new();
    let mut bytes = 0usize;
    for (index, range) in source_comments(source, IMPORT_FAMILY, 9)
        .into_iter()
        .enumerate()
    {
        let text = &source[range.clone()];
        bytes = bytes.saturating_add(text.len());
        if index >= 8 || bytes > MAX_RESOURCES {
            output.push(invalid_annotation(
                source,
                range,
                "citation_import_annotation_budget",
            ));
            break;
        }
        let parsed = comment_payload(text, IMPORT_ANNOTATION)
            .and_then(|raw| serde_json::from_str::<ImportAnnotation>(raw).ok())
            .filter(|annotation| {
                annotation.version == 1
                    && annotation.diagnostics.len() <= 4096
                    && annotation.diagnostics.iter().all(ImportDiagnostic::valid)
            });
        match parsed {
            Some(annotation)
                if output.len().saturating_add(annotation.diagnostics.len()) <= 4096 =>
            {
                output.extend(
                    annotation
                        .diagnostics
                        .into_iter()
                        .filter_map(|value| serde_json::to_value(value).ok()),
                );
            }
            _ => output.push(invalid_annotation(
                source,
                range,
                "citation_import_annotation_invalid",
            )),
        }
    }
    output
}
fn esc(value: &str) -> String {
    crate::docx_writer::xml_escape(value).replace('\r', "&#13;")
}
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 1024
        && !name.starts_with('/')
        && !name.contains(['\\', ':', '?', '#', '%', '\0'])
        && valid_text(name)
        && name.split('/').all(|part| {
            !part.is_empty() && !matches!(part, "." | "..") && !part.ends_with(['.', ' '])
        })
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub name: String,
    pub kind: String,
    pub sha256: String,
    pub text: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Resources {
    pub version: u32,
    pub bibliography: Vec<String>,
    pub files: Vec<Resource>,
    pub style: String,
    pub locale: String,
    pub locale_xml: Option<String>,
    pub manual_bibliography: bool,
}
impl Resources {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || self.bibliography.is_empty()
            || self.bibliography.len() > 8
            || self.files.len() > 9
            || self.locale.len() > 128
            || !valid_text(&self.locale)
            || self.locale.is_empty()
            || !valid_name(&self.style)
            || self
                .locale_xml
                .as_ref()
                .is_some_and(|x| x.len() > 256 * 1024 || !valid_text(x))
        {
            return Err(resource_err());
        }
        let mut names = HashSet::new();
        for file in &self.files {
            let ext = Path::new(&file.name)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !valid_name(&file.name)
                || !names.insert(file.name.to_lowercase())
                || !valid_text(&file.text)
                || !valid_digest(&file.sha256)
                || digest(&file.text) != file.sha256
                || !matches!(
                    (file.kind.as_str(), ext.as_str()),
                    ("bibtex", "bib") | ("csl-json", "json") | ("csl", "csl")
                )
            {
                return Err(resource_err());
            }
        }
        let mut bibliography = HashSet::new();
        let mut kind = None;
        for name in &self.bibliography {
            let file = self
                .files
                .iter()
                .find(|f| f.name == *name)
                .ok_or_else(resource_err)?;
            if !bibliography.insert(name)
                || !matches!(file.kind.as_str(), "bibtex" | "csl-json")
                || kind.is_some_and(|k| k != file.kind.as_str())
            {
                return Err(resource_err());
            }
            kind = Some(file.kind.as_str());
        }
        if self.style.to_ascii_lowercase().ends_with(".csl")
            && !self
                .files
                .iter()
                .any(|f| f.name == self.style && f.kind == "csl")
        {
            return Err(resource_err());
        }
        if self.files.iter().any(|f| {
            !self.bibliography.contains(&f.name) && !(f.kind == "csl" && f.name == self.style)
        }) {
            return Err(resource_err());
        }
        let wire = serde_json::to_string_pretty(self)
            .map_err(|_| resource_err())?
            .replace('&', "\\u0026")
            .replace('<', "\\u003c")
            .replace('>', "\\u003e");
        if wire.len() + CAPSULE.len() + 5 > MAX_RESOURCES {
            return Err(resource_err());
        }
        Ok(())
    }
    pub fn read(&self, name: &str) -> Option<&str> {
        let name = relative_alias(name)?;
        self.files
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(&name))
            .map(|f| f.text.as_str())
    }
    pub fn resource_revision(&self) -> Result<String, String> {
        self.validate()?;
        Ok(digest(
            &serde_json::to_string(self).map_err(|_| resource_err())?,
        ))
    }
    pub fn markdown_capsule(&self) -> Result<String, String> {
        self.validate()?;
        // JSON escapes preserve these legal resource characters without letting
        // a CSL value or JSON title close, nest or execute the HTML comment.
        let json = serde_json::to_string_pretty(self)
            .map_err(|_| resource_err())?
            .replace('&', "\\u0026")
            .replace('<', "\\u003c")
            .replace('>', "\\u003e");
        let text = format!("{CAPSULE}{json}\n-->\n");
        if text.len() > MAX_RESOURCES {
            return Err(resource_err());
        }
        Ok(text)
    }
    pub fn from_markdown(source: &str) -> Result<Option<Self>, String> {
        let mut result = None;
        for range in source_comments(source, CAPSULE_FAMILY, 2) {
            let text = &source[range];
            if result.is_some() || text.len() > MAX_RESOURCES {
                return Err(resource_err());
            }
            let raw = comment_payload(text, CAPSULE).ok_or_else(resource_err)?;
            let resources: Self = serde_json::from_str(raw).map_err(|_| resource_err())?;
            resources.validate()?;
            result = Some(resources);
        }
        Ok(result)
    }
    pub fn capture(source: &str, base: &str, options: &Value) -> Result<Option<Self>, String> {
        let embedded = Self::from_markdown(source)?;
        let self_contained = embedded.is_some();
        let names = if let Some(resources) = &embedded {
            resources.bibliography.clone()
        } else {
            crate::publication_metadata::bibliography_sources(source).map_err(|_| resource_err())?
        };
        if names.is_empty() {
            return Ok(None);
        }
        let mut out = embedded.unwrap_or(Self {
            version: 1,
            bibliography: Vec::new(),
            files: Vec::new(),
            style: String::new(),
            locale: "en-US".into(),
            locale_xml: None,
            manual_bibliography: false,
        });
        let root = if names.iter().any(|name| out.read(name).is_none()) {
            Some(Path::new(base).canonicalize().map_err(|_| resource_err())?)
        } else {
            None
        };
        let mut budget = out.files.iter().map(|f| f.text.len()).sum::<usize>();
        let mut canonical_names = Vec::new();
        for name in names {
            if out.read(&name).is_some() {
                canonical_names.push(name);
                continue;
            }
            let (name, text) = capture_file(
                root.as_deref().ok_or_else(resource_err)?,
                &name,
                &mut budget,
            )?;
            let kind = if name.to_ascii_lowercase().ends_with(".json") {
                "csl-json"
            } else {
                "bibtex"
            };
            out.files.push(Resource {
                name: name.clone(),
                kind: kind.into(),
                sha256: digest(&text),
                text,
            });
            canonical_names.push(name);
        }
        out.bibliography = canonical_names;
        let doc = crate::md_ast::parse(source);
        let requested = options
            .pointer("/citations/style")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| doc.meta("csl"))
            .or_else(|| (!out.style.is_empty()).then(|| out.style.clone()))
            .unwrap_or_else(|| "american-psychological-association".into());
        if requested.to_ascii_lowercase().ends_with(".csl") && out.read(&requested).is_none() {
            if self_contained {
                return Err(resource_err());
            }
            let root = Path::new(base).canonicalize().map_err(|_| resource_err())?;
            let (name, text) = capture_file(&root, &requested, &mut budget)?;
            out.files.retain(|f| f.kind != "csl");
            out.files.push(Resource {
                name: name.clone(),
                kind: "csl".into(),
                sha256: digest(&text),
                text,
            });
            out.style = name;
        } else {
            out.style = if requested.to_ascii_lowercase().ends_with(".csl") {
                let alias = relative_alias(&requested).ok_or_else(resource_err)?;
                out.files
                    .iter()
                    .find(|f| f.name.eq_ignore_ascii_case(&alias))
                    .map(|f| f.name.clone())
                    .ok_or_else(resource_err)?
            } else {
                requested
            };
            out.files.retain(|f| f.kind != "csl" || f.name == out.style);
        }
        if let Some(locale) = crate::publication_metadata::from_source(source, options, "")
            .map_err(|_| resource_err())?
            .language
        {
            out.locale = locale;
        }
        if let Some(value) = options
            .pointer("/citations/locale_xml")
            .filter(|v| !v.is_null())
        {
            out.locale_xml = Some(value.as_str().ok_or_else(resource_err)?.into());
        }
        out.validate()?;
        out.markdown_capsule()?;
        Ok(Some(out))
    }
}
fn relative_alias(name: &str) -> Option<String> {
    if name.starts_with(['/', '\\']) || name.contains([':', '?', '#', '%', '\0']) {
        return None;
    }
    let normalized = name.replace('\\', "/");
    let mut parts = Vec::new();
    for part in normalized.split('/') {
        match part {
            "" | "." => (),
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    let alias = parts.join("/");
    valid_name(&alias).then_some(alias)
}
fn capture_file(root: &Path, name: &str, budget: &mut usize) -> Result<(String, String), String> {
    let path = root.join(name).canonicalize().map_err(|_| resource_err())?;
    let rel = path.strip_prefix(root).map_err(|_| resource_err())?;
    let safe = rel
        .components()
        .map(|c| c.as_os_str().to_str().ok_or_else(resource_err))
        .collect::<Result<Vec<_>, _>>()?
        .join("/");
    if !valid_name(&safe) || !path.is_file() {
        return Err(resource_err());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|_| resource_err())?
        .take((MAX_RESOURCES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| resource_err())?;
    *budget = budget.saturating_add(bytes.len());
    if *budget > MAX_RESOURCES {
        return Err(resource_err());
    }
    Ok((safe, crate::text_encoding::detect_and_decode(&bytes).0))
}

#[derive(Clone, Debug)]
pub struct Record {
    pub id: String,
    pub kind: String,
    pub part: String,
    pub source: String,
    pub original_source: String,
    pub render_sha256: String,
    pub render_context_version: u32,
    pub binding: String,
    pub span: Option<SourceSpan>,
    pub items: Vec<Cite>,
    pub generated: bool,
    pub note: Option<NoteBinding>,
}
#[derive(Clone, Debug)]
pub struct NoteBinding {
    pub end: bool,
    pub id: i32,
    pub story_sha256: String,
}
impl Record {
    fn binding(&self) -> String {
        digest(&format!(
            "ReadMD citation binding {}\0{}\0{}\0{}\0{}\0{}\0{}\0{:?}",
            self.render_context_version,
            self.id,
            self.kind,
            self.part,
            self.original_source,
            self.render_sha256,
            self.generated,
            self.note
                .as_ref()
                .map(|n| (n.end, n.id, n.story_sha256.as_str()))
        ))
    }
    pub fn tag(&self) -> String {
        format!(
            "{}{}",
            if self.kind == "bibliography" {
                BIB_TAG
            } else {
                TAG
            },
            self.id
        )
    }
    pub fn valid(&self) -> bool {
        uuid::Uuid::parse_str(&self.id).is_ok()
            && matches!(self.kind.as_str(), "citation" | "bibliography")
            && matches!(
                self.part.as_str(),
                "word/document.xml" | "word/footnotes.xml" | "word/endnotes.xml"
            )
            && self.source.len() <= 65536
            && self.original_source.len() <= 65536
            && valid_text(&self.source)
            && valid_text(&self.original_source)
            && valid_digest(&self.render_sha256)
            && matches!(self.render_context_version, 1 | RENDER_CONTEXT_VERSION)
            && self.binding == self.binding()
            && self.span.as_ref().is_none_or(|span| {
                span.byte_to.checked_sub(span.byte_from) == Some(span.source.len())
                    && span.to.checked_sub(span.from) == Some(span.source.encode_utf16().count())
                    && (self.kind != "citation" || span.source == self.original_source)
            })
    }
}
#[derive(Clone, Debug)]
pub struct Manifest {
    pub document_id: String,
    pub source_revision: String,
    pub resources: Option<Resources>,
    pub records: Vec<Record>,
}
#[derive(Default)]
pub struct Bindings {
    records: Vec<Record>,
}
impl Bindings {
    pub fn citation(
        &mut self,
        source: &SourceCitation,
        rendered: &str,
        part: &str,
        generated: bool,
        note: Option<(bool, i32)>,
    ) -> Result<String, String> {
        self.register(
            "citation",
            &source.span.source,
            Some(source.span.clone()),
            source.items.iter().map(|i| i.cite.clone()).collect(),
            rendered,
            part,
            generated,
            note,
        )
    }
    pub fn bibliography(
        &mut self,
        source: Option<&SourceSpan>,
        rendered: &str,
    ) -> Result<String, String> {
        self.register(
            "bibliography",
            "<!--readmd:bibliography-->",
            source.cloned(),
            Vec::new(),
            rendered,
            "word/document.xml",
            false,
            None,
        )
    }
    fn register(
        &mut self,
        kind: &str,
        source: &str,
        span: Option<SourceSpan>,
        items: Vec<Cite>,
        rendered: &str,
        part: &str,
        generated: bool,
        note: Option<(bool, i32)>,
    ) -> Result<String, String> {
        if self.records.len() >= 4096 {
            return Err(err());
        }
        let mut record = Record {
            id: uuid::Uuid::new_v4().to_string(),
            kind: kind.into(),
            part: part.into(),
            source: source.into(),
            original_source: source.into(),
            render_sha256: render_signature(rendered)?,
            render_context_version: RENDER_CONTEXT_VERSION,
            binding: String::new(),
            span,
            items,
            generated,
            note: note.map(|(end, id)| NoteBinding {
                end,
                id,
                story_sha256: String::new(),
            }),
        };
        record.binding = record.binding();
        if !record.valid() {
            return Err(err());
        }
        let xml = format!("<w:sdt><w:sdtPr><w:alias w:val=\"ReadMD {}\"/><w:tag w:val=\"{}\"/><w:id w:val=\"{}\"/></w:sdtPr><w:sdtContent>{rendered}</w:sdtContent></w:sdt>",kind,record.tag(),1_000_000+self.records.len());
        self.records.push(record);
        Ok(xml)
    }
    pub fn attach(mut self, parts: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
        if self.records.is_empty() {
            return Ok(());
        }
        // Local paragraph and note defaults can add run properties after the
        // citation writer. Bind the final emitted controls, not an earlier view.
        {
            let signatures = crate::convert::docx_citations::PackageSignatures::new(parts);
            let final_renders = signatures.bindings()?;
            for record in &mut self.records {
                record.render_sha256 = final_renders.get(&record.tag()).cloned().ok_or_else(err)?;
                if let Some(note) = &mut record.note {
                    note.story_sha256 = signatures.note(note.end, note.id)?;
                }
                record.binding = record.binding();
            }
        }
        let document_id = uuid::Uuid::new_v4().to_string();
        let manifest = Manifest {
            document_id: document_id.clone(),
            source_revision: String::new(),
            resources: None,
            records: self.records,
        };
        parts.push((PART.into(), manifest.xml()?.into_bytes()));
        let item = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><ds:datastoreItem ds:itemID=\"{{{document_id}}}\" xmlns:ds=\"http://schemas.openxmlformats.org/officeDocument/2006/customXml\"><ds:schemaRefs><ds:schemaRef ds:uri=\"{NS}\"/></ds:schemaRefs></ds:datastoreItem>");
        parts.push((
            "customXml/readmd-citations-props.xml".into(),
            item.into_bytes(),
        ));
        parts.push(("customXml/_rels/readmd-citations.xml.rels".into(),format!("<Relationships xmlns=\"{PKG}\"><Relationship Id=\"readmdCitationProperties\" Type=\"{R}/customXmlProps\" Target=\"readmd-citations-props.xml\"/></Relationships>").into_bytes()));
        append_xml(parts,"word/_rels/document.xml.rels","</Relationships>",&format!("<Relationship Id=\"readmdCitationSources\" Type=\"{R}/customXml\" Target=\"../{PART}\"/>"))?;
        append_xml(parts,"[Content_Types].xml","</Types>","<Override PartName=\"/customXml/readmd-citations-props.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.customXmlProperties+xml\"/>")?;
        Ok(())
    }
}
fn append_xml(
    parts: &mut [(String, Vec<u8>)],
    name: &str,
    closing: &str,
    insert: &str,
) -> Result<(), String> {
    let part = parts.iter_mut().find(|(n, _)| n == name).ok_or_else(err)?;
    let xml = std::str::from_utf8(&part.1).map_err(|_| err())?;
    let at = xml.rfind(closing).ok_or_else(err)?;
    let mut xml = xml.to_string();
    xml.insert_str(at, insert);
    part.1 = xml.into_bytes();
    Ok(())
}

/// Attach the original revision and actual resource inputs after the writer has
/// emitted local bindings. This does not store any other source-document text.
pub fn attach_context(
    parts: &mut Vec<(String, Vec<u8>)>,
    source: &str,
    base: &str,
    options: &Value,
) -> Result<(), String> {
    let Some(index) = parts.iter().position(|(name, _)| name == PART) else {
        return Ok(());
    };
    let mut manifest = Manifest::parse(&parts[index].1)?;
    for record in &manifest.records {
        if let Some(span) = &record.span {
            if source.get(span.byte_from..span.byte_to) != Some(span.source.as_str())
                || source[..span.byte_from].encode_utf16().count() != span.from
                || source[..span.byte_to].encode_utf16().count() != span.to
            {
                return Err(err());
            }
        }
    }
    manifest.source_revision = digest(source);
    manifest.resources = Resources::capture(source, base, options)?;
    parts[index].1 = manifest.xml()?.into_bytes();
    Ok(())
}

/// A conservative semantic digest ignores namespace aliases, rsid/proofing and
/// run boundaries while retaining text, run properties, links and note markers.
pub fn render_signature(fragment: &str) -> Result<String, String> {
    use crate::math_document::{XmlNode, XmlPart};
    let root = crate::math_document::parse_xml(&format!(
        "<w:sdtContent xmlns:w=\"{W}\" xmlns:r=\"{R}\">{fragment}</w:sdtContent>"
    ))
    .map_err(|_| err())?;
    fn semantic(node: &XmlNode) -> Value {
        let attrs: BTreeMap<_, _> = node
            .attributes
            .iter()
            .filter(|(key, value)| {
                let namespace=node.attribute_namespaces.get(*key).map(String::as_str).unwrap_or("");
                let local=key.rsplit(':').next().unwrap_or(key);
                let revision=node.namespace==W&&matches!(node.local(),"p"|"r"|"sectPr")&&namespace==W
                    &&matches!(local,"rsid"|"rsidR"|"rsidRPr"|"rsidDel"|"rsidP"|"rsidRDefault"|"rsidSect");
                let space=node.namespace==W&&node.local()=="t"&&namespace==crate::math_document::XML_NS
                    &&local=="space"&&matches!(value.as_str(),"default"|"preserve");
                !(key.as_str()=="xmlns"||key.starts_with("xmlns:")||revision||space)
            })
            .map(|(key, value)| {
                (
                    format!(
                        "{}:{}",
                        node.attribute_namespaces
                            .get(key)
                            .map(String::as_str)
                            .unwrap_or(""),
                        key.rsplit(':').next().unwrap_or(key)
                    ),
                    value.clone(),
                )
            })
            .collect();
        let mut children = Vec::<Value>::new();
        for part in &node.content {
            match part {
                XmlPart::Element(child)
                    if child.namespace == W
                        && matches!(
                            child.local(),
                            "proofErr" | "bookmarkStart" | "bookmarkEnd" | "lastRenderedPageBreak"
                        ) =>
                {
                    ()
                }
                XmlPart::Element(child) => children.push(semantic(child)),
                XmlPart::Text(text) if node.local() == "t" || !text.trim().is_empty() => {
                    children.push(json!({"text":text}))
                }
                _ => (),
            }
        }
        // Equivalent adjacent Word text runs may be split or joined by an
        // editor. Keep their property trees while joining the text atoms.
        let mut normalized = Vec::<Value>::new();
        for child in children {
            if child.get(0).and_then(Value::as_str) == Some(W)
                && child.get(1).and_then(Value::as_str) == Some("r")
                && child[2].as_object().is_some_and(|attrs|attrs.is_empty())
            {
                let run = child[3].as_array().cloned().unwrap_or_default();
                let properties = run
                    .iter()
                    .filter(|v| v.get(0).and_then(Value::as_str)==Some(W)&&v.get(1).and_then(Value::as_str)==Some("rPr"))
                    .cloned()
                    .collect::<Vec<_>>();
                let content = run
                    .into_iter()
                    .filter(|v| !(v.get(0).and_then(Value::as_str)==Some(W)&&v.get(1).and_then(Value::as_str)==Some("rPr")))
                    .collect::<Vec<_>>();
                let mut text = String::new();
                let only_text = content
                    .iter()
                    .all(|v| v.get(0).and_then(Value::as_str)==Some(W)&&v.get(1).and_then(Value::as_str)==Some("t")
                        &&v[2].as_object().is_some_and(|attrs|attrs.is_empty())
                        &&v[3].as_array().is_some_and(|atoms|atoms.iter().all(|atom|atom.as_object().is_some_and(|value|value.len()==1&&value.get("text").is_some_and(Value::is_string)))));
                if only_text {
                    for value in &content {
                        if let Some(nodes) = value[3].as_array() {
                            for value in nodes {
                                if let Some(t) = value["text"].as_str() {
                                    text.push_str(t);
                                }
                            }
                        }
                    }
                    if let Some(last) = normalized.last_mut().filter(|v| {
                        v["run_properties"] == json!(properties) && v.get("run_text").is_some()
                    }) {
                        let mut value = last["run_text"].as_str().unwrap_or("").to_string();
                        value.push_str(&text);
                        last["run_text"] = json!(value);
                    } else {
                        normalized.push(json!({"run_properties":properties,"run_text":text}));
                    }
                    continue;
                }
            }
            normalized.push(child);
        }
        json!([node.namespace, node.local(), attrs, normalized])
    }
    let root = root.children().next().ok_or_else(err)?;
    Ok(digest(&semantic(root).to_string()))
}

/// Bind the visible fragment to only its directly referenced OPC relationships.
/// Resolution is entirely against package members, never the host or network.
/// The shared context caches relationship trees and binary hashes across SDTs.
pub(crate) struct ContextSignatures<'a> {
    entries: &'a [(String, Vec<u8>)],
    relationships: RefCell<BTreeMap<String, Result<crate::math_document::XmlNode, String>>>,
    payloads: RefCell<BTreeMap<String, Result<String, String>>>,
    payload_bytes: Cell<usize>,
    payload_limit: usize,
}
impl<'a> ContextSignatures<'a> {
    pub(crate) fn new(entries: &'a [(String, Vec<u8>)]) -> Self {
        Self { entries, relationships: RefCell::new(BTreeMap::new()), payloads: RefCell::new(BTreeMap::new()), payload_bytes: Cell::new(0), payload_limit:64*1024*1024 }
    }
    #[cfg(test)]
    pub(crate) fn with_payload_limit(entries:&'a [(String,Vec<u8>)],limit:usize)->Self {
        Self{payload_limit:limit,..Self::new(entries)}
    }
    #[cfg(test)]
    pub(crate) fn payload_work(&self)->(usize,usize) {
        (self.payloads.borrow().values().filter(|value|value.is_ok()).count(),self.payload_bytes.get())
    }
    fn member(&self, name: &str) -> Result<&'a [u8], String> {
        let mut found = self.entries.iter().filter(|(path, _)| path == name);
        let value = found.next().ok_or_else(err)?.1.as_slice();
        if found.next().is_some() { return Err(err()); }
        Ok(value)
    }
    fn payload(&self, name: &str) -> Result<String, String> {
        if let Some(value) = self.payloads.borrow().get(name) { return value.clone(); }
        let value = (|| {
            let bytes = self.member(name)?;
            let total = self.payload_bytes.get().checked_add(bytes.len()).ok_or_else(err)?;
            if total > self.payload_limit { return Err(err()); }
            self.payload_bytes.set(total);
            use sha2::{Digest, Sha256};
            Ok(format!("{:x}", Sha256::digest(bytes)))
        })();
        self.payloads.borrow_mut().insert(name.into(), value.clone());
        value
    }
    pub(crate) fn signature(&self, part: &str, fragment: &str) -> Result<String, String> {
        if !matches!(part, "word/document.xml" | "word/footnotes.xml" | "word/endnotes.xml") { return Err(err()); }
        let root = crate::math_document::parse_xml(&format!("<w:sdtContent xmlns:w=\"{W}\" xmlns:r=\"{R}\">{fragment}</w:sdtContent>")).map_err(|_| err())?;
        fn references(node: &crate::math_document::XmlNode, ids: &mut BTreeSet<String>) -> Result<(), String> {
            let mut expanded = HashSet::new();
            for (key, value) in &node.attributes {
                if key == "xmlns" || key.starts_with("xmlns:") { continue; }
                let namespace = node.attribute_namespaces.get(key).map(String::as_str).unwrap_or("");
                let local = key.rsplit(':').next().unwrap_or(key);
                if !expanded.insert((namespace, local)) { return Err(err()); }
                if matches!(namespace, R | "http://purl.oclc.org/ooxml/officeDocument/relationships") && matches!(local, "id" | "embed" | "link") {
                    if value.is_empty() || value.len() > 1024 || !valid_text(value) { return Err(err()); }
                    ids.insert(value.clone());
                    if ids.len() > 4096 { return Err(err()); }
                }
            }
            for child in node.children() { references(child, ids)?; }
            Ok(())
        }
        let mut ids = BTreeSet::new();
        references(&root, &mut ids)?;
        let mut dependencies = Vec::<Value>::new();
        if !ids.is_empty() {
            let (base, file) = part.rsplit_once('/').ok_or_else(err)?;
            let rel_part = format!("{base}/_rels/{file}.rels");
            if !self.relationships.borrow().contains_key(&rel_part) {
                let parsed = (|| {
                    let bytes = self.member(&rel_part)?;
                    let xml = std::str::from_utf8(bytes).map_err(|_| err())?;
                    let tree = crate::math_document::parse_xml(xml).map_err(|_| err())?;
                    if tree.children().count() != 1 || tree.children().next().is_none_or(|node| node.local() != "Relationships" || node.namespace != PKG) { return Err(err()); }
                    Ok(tree)
                })();
                self.relationships.borrow_mut().insert(rel_part.clone(), parsed);
            }
            let relationships = self.relationships.borrow();
            let tree = relationships.get(&rel_part).ok_or_else(err)?.as_ref().map_err(Clone::clone)?;
            let relationships = tree.children().next().ok_or_else(err)?;
            for id in ids {
                let mut matching = relationships.children().filter(|node| node.namespace == PKG && node.local() == "Relationship" && node.attributes.get("Id") == Some(&id));
                let relation = matching.next().ok_or_else(err)?;
                if matching.next().is_some() || relation.children().next().is_some() || !relation.text().trim().is_empty()
                    || relation.attributes.keys().any(|key| key != "xmlns" && !key.starts_with("xmlns:") && !matches!(key.as_str(), "Id" | "Type" | "Target" | "TargetMode")) { return Err(err()); }
                let attr = |key| relation.attributes.get(key).filter(|value| !value.is_empty() && value.len() <= 65536 && valid_text(value)).map(String::as_str).ok_or_else(err);
                let kind = attr("Type")?;
                let target = attr("Target")?;
                let mode = match relation.attributes.get("TargetMode").map(String::as_str) {
                    None | Some("") | Some("Internal") => "Internal",
                    Some("External") => "External",
                    _ => return Err(err()),
                };
                let payload = if mode == "Internal" {
                    let member = context_member(base, target)?;
                    json!([member, self.payload(&member)?])
                } else { Value::Null };
                dependencies.push(json!([id, kind, target, mode, payload]));
            }
        }
        Ok(digest(&json!(["ReadMD citation render context", RENDER_CONTEXT_VERSION, part, render_signature(fragment)?, dependencies]).to_string()))
    }
}
#[cfg(test)]
pub(crate) fn context_signature(entries: &[(String, Vec<u8>)], part: &str, fragment: &str) -> Result<String, String> {
    ContextSignatures::new(entries).signature(part, fragment)
}
fn context_member(base: &str, target: &str) -> Result<String, String> {
    let bytes = target.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'%' && (index + 2 >= bytes.len() || !bytes[index + 1..index + 3].iter().all(u8::is_ascii_hexdigit)) { return Err(err()); }
    }
    let decoded = percent_encoding::percent_decode_str(target).decode_utf8().map_err(|_| err())?;
    if decoded.is_empty() || decoded.starts_with("//") || decoded.contains(['\\', ':', '?', '#']) || decoded.chars().any(char::is_control) { return Err(err()); }
    let mut parts = if decoded.starts_with('/') { Vec::new() } else { base.split('/').filter(|part| !part.is_empty()).collect::<Vec<_>>() };
    for part in decoded.split('/') {
        match part { "" | "." => (), ".." => { parts.pop().ok_or_else(err)?; }, _ => parts.push(part) }
    }
    if parts.is_empty() { return Err(err()); }
    Ok(parts.join("/"))
}

#[derive(Default)]
struct Node {
    name: String,
    attrs: BTreeMap<String, String>,
    text: String,
    children: Vec<Node>,
}
impl Node {
    fn attr(&self, key: &str) -> Result<&str, String> {
        self.attrs.get(key).map(String::as_str).ok_or_else(err)
    }
    fn child(&self, key: &str) -> Result<&Node, String> {
        let mut children = self.children.iter().filter(|n| n.name == key);
        let first = children.next().ok_or_else(err)?;
        if children.next().is_some() {
            return Err(err());
        }
        Ok(first)
    }
}
fn parse_manifest(bytes: &[u8]) -> Result<Node, String> {
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(err());
    }
    let xml = std::str::from_utf8(bytes).map_err(|_| err())?;
    if !valid_text(xml) {
        return Err(err());
    }
    let mut reader = NsReader::from_str(xml);
    reader.config_mut().check_end_names = true;
    reader.config_mut().check_comments = true;
    let mut stack = Vec::<Node>::new();
    let mut roots = Vec::new();
    let mut count = 0;
    loop {
        match reader.read_event().map_err(|_| err())? {
            event @ (Event::Start(_) | Event::Empty(_)) => {
                let element = match &event {
                    Event::Start(e) | Event::Empty(e) => e,
                    _ => unreachable!(),
                };
                let (ns, local) = reader.resolver().resolve_element(element.name());
                if !matches!(ns,ResolveResult::Bound(ref n) if n.as_ref()==NS) {
                    return Err(err());
                }
                count += 1;
                if count > 100_000 || stack.len() > 12 {
                    return Err(err());
                }
                let mut node = Node {
                    name: local.as_ref().to_string(),
                    ..Default::default()
                };
                for attr in element.attributes() {
                    let attr = attr.map_err(|_| err())?;
                    let name = attr.key.as_ref();
                    if name == "xmlns" || name.starts_with("xmlns:") {
                        continue;
                    }
                    if name.contains(':') {
                        return Err(err());
                    }
                    let value = attr
                        .normalized_value(XmlVersion::Implicit1_0)
                        .map_err(|_| err())?
                        .into_owned();
                    if node.attrs.insert(name.to_string(), value).is_some() {
                        return Err(err());
                    }
                }
                if matches!(event, Event::Start(_)) {
                    stack.push(node);
                } else if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else {
                    roots.push(node);
                }
            }
            Event::End(_) => {
                let node = stack.pop().ok_or_else(err)?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else {
                    roots.push(node);
                }
            }
            Event::Text(text) => {
                let text = text.xml_content(XmlVersion::Implicit1_0);
                if let Some(parent) = stack.last_mut() {
                    parent.text.push_str(&text);
                } else if !text.trim().is_empty() {
                    return Err(err());
                }
            }
            Event::CData(text) => stack
                .last_mut()
                .ok_or_else(err)?
                .text
                .push_str(&text.xml_content(XmlVersion::Implicit1_0)),
            Event::GeneralRef(text) => {
                let value = format!("&{};", text.as_ref());
                let value = quick_xml::escape::unescape(&value).map_err(|_| err())?;
                stack.last_mut().ok_or_else(err)?.text.push_str(&value);
            }
            Event::DocType(_) => return Err(err()),
            Event::Eof => break,
            _ => (),
        }
    }
    if !stack.is_empty() || roots.len() != 1 {
        return Err(err());
    }
    Ok(roots.pop().unwrap())
}
impl Manifest {
    pub fn xml(&self) -> Result<String, String> {
        if uuid::Uuid::parse_str(&self.document_id).is_err()
            || (!self.source_revision.is_empty() && !valid_digest(&self.source_revision))
            || self.records.len() > 4096
        {
            return Err(err());
        }
        let mut xml=format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><rc:citations xmlns:rc=\"{NS}\" version=\"1\" document_id=\"{}\" source_revision=\"{}\">",self.document_id,self.source_revision);
        if let Some(resources) = &self.resources {
            resources.validate()?;
            xml.push_str(&format!(
                "<rc:resources>{}</rc:resources>",
                esc(&serde_json::to_string_pretty(resources).map_err(|_| resource_err())?)
            ));
        }
        for record in &self.records {
            if !record.valid() {
                return Err(err());
            }
            xml.push_str(&format!("<rc:record id=\"{}\" kind=\"{}\" part=\"{}\" render_sha256=\"{}\" render_context_version=\"{}\" binding=\"{}\" generated=\"{}\"><rc:source>{}</rc:source><rc:original_source>{}</rc:original_source>",record.id,record.kind,record.part,record.render_sha256,record.render_context_version,record.binding,record.generated,esc(&record.source),esc(&record.original_source)));
            if let Some(note) = &record.note {
                xml.push_str(&format!(
                    "<rc:note end=\"{}\" id=\"{}\" story_sha256=\"{}\"/>",
                    note.end, note.id, note.story_sha256
                ));
            }
            if let Some(span) = &record.span {
                xml.push_str(&format!(
                    "<rc:span byte_from=\"{}\" byte_to=\"{}\" from=\"{}\" to=\"{}\">{}</rc:span>",
                    span.byte_from,
                    span.byte_to,
                    span.from,
                    span.to,
                    esc(&span.source)
                ));
            }
            for cite in &record.items {
                xml.push_str(&format!(
                    "<rc:item key=\"{}\" mode=\"{}\">",
                    esc(&cite.key),
                    serde_json::to_value(cite.mode)
                        .map_err(|_| err())?
                        .as_str()
                        .ok_or_else(err)?
                ));
                for (name, value) in [
                    ("locator", &cite.locator),
                    ("kind", &cite.kind),
                    ("prefix", &cite.prefix),
                    ("suffix", &cite.suffix),
                ] {
                    if let Some(value) = value {
                        xml.push_str(&format!("<rc:{name}>{}</rc:{name}>", esc(value)));
                    }
                }
                xml.push_str("</rc:item>");
            }
            xml.push_str("</rc:record>");
        }
        xml.push_str("</rc:citations>");
        if xml.len() > 8 * 1024 * 1024 {
            return Err(err());
        }
        Ok(xml)
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let root = parse_manifest(bytes)?;
        if root.name != "citations" || root.attr("version")? != "1" {
            return Err(err());
        }
        let mut out = Self {
            document_id: root.attr("document_id")?.into(),
            source_revision: root.attr("source_revision")?.into(),
            resources: None,
            records: Vec::new(),
        };
        let mut ids = HashSet::new();
        for node in &root.children {
            match node.name.as_str() {
                "resources" => {
                    if out.resources.is_some() || !node.children.is_empty() {
                        return Err(resource_err());
                    }
                    let resources: Resources =
                        serde_json::from_str(&node.text).map_err(|_| resource_err())?;
                    resources.validate()?;
                    out.resources = Some(resources);
                }
                "record" => {
                    let original = node.child("original_source")?.text.clone();
                    let mut record = Record {
                        id: node.attr("id")?.into(),
                        kind: node.attr("kind")?.into(),
                        part: node.attr("part")?.into(),
                        source: node.child("source")?.text.clone(),
                        original_source: original,
                        render_sha256: node.attr("render_sha256")?.into(),
                        render_context_version: match node.attrs.get("render_context_version").map(String::as_str) {
                            None | Some("1") => 1,
                            Some("2") => RENDER_CONTEXT_VERSION,
                            _ => return Err(err()),
                        },
                        binding: node.attr("binding")?.into(),
                        span: None,
                        items: Vec::new(),
                        generated: match node.attr("generated")? {
                            "true" => true,
                            "false" => false,
                            _ => return Err(err()),
                        },
                        note: None,
                    };
                    for field in &node.children {
                        match field.name.as_str() {
                            "source" | "original_source" => {
                                if !field.children.is_empty() {
                                    return Err(err());
                                }
                            }
                            "span" => {
                                if record.span.is_some() {
                                    return Err(err());
                                }
                                let n = |key| field.attr(key)?.parse::<usize>().map_err(|_| err());
                                record.span = Some(SourceSpan {
                                    byte_from: n("byte_from")?,
                                    byte_to: n("byte_to")?,
                                    from: n("from")?,
                                    to: n("to")?,
                                    source: field.text.clone(),
                                });
                            }
                            "note" => {
                                if record.note.is_some() {
                                    return Err(err());
                                }
                                let end = match field.attr("end")? {
                                    "true" => true,
                                    "false" => false,
                                    _ => return Err(err()),
                                };
                                let id = field.attr("id")?.parse().map_err(|_| err())?;
                                let story_sha256 = field.attr("story_sha256")?.to_string();
                                if !valid_digest(&story_sha256) {
                                    return Err(err());
                                }
                                record.note = Some(NoteBinding {
                                    end,
                                    id,
                                    story_sha256,
                                });
                            }
                            "item" => {
                                let mut cite = Cite::new(field.attr("key")?);
                                cite.mode = serde_json::from_value(json!(field.attr("mode")?))
                                    .map_err(|_| err())?;
                                let mut seen = HashSet::new();
                                for value in &field.children {
                                    if !seen.insert(&value.name) || !value.children.is_empty() {
                                        return Err(err());
                                    }
                                    let target = match value.name.as_str() {
                                        "locator" => &mut cite.locator,
                                        "kind" => &mut cite.kind,
                                        "prefix" => &mut cite.prefix,
                                        "suffix" => &mut cite.suffix,
                                        _ => return Err(err()),
                                    };
                                    *target = Some(value.text.clone());
                                }
                                cite.validate().map_err(|_| err())?;
                                record.items.push(cite);
                            }
                            _ => return Err(err()),
                        }
                    }
                    if !record.valid() || !ids.insert(record.id.clone()) {
                        return Err(err());
                    }
                    out.records.push(record);
                }
                _ => return Err(err()),
            }
        }
        if uuid::Uuid::parse_str(&out.document_id).is_err()
            || (!out.source_revision.is_empty() && !valid_digest(&out.source_revision))
            || out.records.len() > 4096
        {
            return Err(err());
        }
        Ok(out)
    }
}

#[cfg(test)]
mod annotation_tests {
    use super::*;
    fn annotation(code: &str) -> String {
        let payload = json!({"version":1,"diagnostics":[{"code":code,"reason":"citation_render_modified","visible_content_preserved":true,"original_source":"[see @42, p. 12 **tail**]","current_source":"[@42, p. 99]"}]});
        format!(
            "{IMPORT_ANNOTATION}{}\n-->",
            serde_json::to_string_pretty(&payload).unwrap()
        )
    }
    #[test]
    fn actual_import_comments_surface_all_three_localized_warning_codes_and_keep_source() {
        let source = format!(
            "KEEP_BODY 中文😀\n\n{}\n\n{}\n\n{}\n",
            annotation(INVALID),
            annotation(CONFLICT),
            annotation(RESOURCE_INVALID)
        );
        let diagnostics = import_diagnostics(&source);
        assert_eq!(diagnostics.len(), 3);
        assert_eq!(
            diagnostics
                .iter()
                .map(|d| d["code"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec![INVALID, CONFLICT, RESOURCE_INVALID]
        );
        let prepared = crate::publication::prepare(&source, "not-a-resource-directory").unwrap();
        assert_eq!(prepared.content, source);
        assert_eq!(prepared.warnings, vec![INVALID, CONFLICT, RESOURCE_INVALID]);
        assert_eq!(prepared.data["diagnostics"].as_array().unwrap().len(), 3);
        assert_eq!(prepared.data["active"], true);
        assert!(prepared.data["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .all(|d| d["revision"] == digest(&source)));
    }
    #[test]
    fn code_frontmatter_and_nested_examples_never_create_import_warnings() {
        let value = annotation(CONFLICT);
        for source in [
            format!("````json\n{value}\n````"),
            format!("    {}", value.replace('\n', "\n    ")),
            format!("> {}", value.replace('\n', "\n> ")),
            format!("- example\n\n    {}", value.replace('\n', "\n    ")),
            format!("<pre>\n{value}\n</pre>"),
            format!(
                "---\nexample: |\n    {}\n---\nBody",
                value.replace('\n', "\n    ")
            ),
        ] {
            assert!(import_diagnostics(&source).is_empty(), "{source}");
        }
    }
    #[test]
    fn malformed_unknown_and_over_budget_annotations_warn_without_changing_body() {
        for value in [
            "<!--readmd:citation-import:v999\n{}\n-->".into(),
            "<!--readmd:citation-import:v1\nnot json\n-->".into(),
            annotation("untrusted_injected_code"),
            annotation(CONFLICT).replace("citation_render_modified", "<unsafe>"),
            annotation(CONFLICT).replace("[@42, p. 99]", "bad\\u0000source"),
            format!("{IMPORT_ANNOTATION}{}\n-->", " ".repeat(MAX_RESOURCES)),
        ] {
            let source = format!("KEEP_BODY\n\n{value}\n");
            let diagnostics = import_diagnostics(&source);
            assert_eq!(diagnostics.len(), 1);
            assert_eq!(diagnostics[0]["code"], INVALID);
            let prepared = crate::publication::prepare(&source, "unused").unwrap();
            assert_eq!(prepared.content, source);
            assert_eq!(prepared.warnings, vec![INVALID]);
        }
        let many = (0..9)
            .map(|_| annotation(CONFLICT))
            .collect::<Vec<_>>()
            .join("\n\n");
        let diagnostics = import_diagnostics(&many);
        assert_eq!(diagnostics.len(), 9);
        assert_eq!(
            diagnostics.last().unwrap()["reason"],
            "citation_import_annotation_budget"
        );
    }
}
