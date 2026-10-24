//! Exact author-source occurrences for the shared document defaults plan.
//! Neither an expression's text nor a client supplied mathPr is an identity.
use crate::{document_transaction as tx, math_document::{self, MathDocument},
    math_document_defaults::DocumentDefaultsPlan, md_ast::{self, Block, Inline}};
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};
use serde::Serialize;
use std::collections::BTreeMap;

const INVALID: &str = "transaction_invalid_range";

#[derive(Debug)]
pub(crate) struct SourceAnalysis {
    pub author: md_ast::Document,
    pub plan: DocumentDefaultsPlan,
    pub occurrences: Vec<Occurrence>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Occurrence {
    pub ordinal: usize,
    #[serde(skip)] pub byte_from: usize,
    #[serde(skip)] pub byte_to: usize,
    #[serde(skip)] pub body_byte_from: usize,
    #[serde(skip)] pub body_byte_to: usize,
    pub from: usize,
    pub to: usize,
    pub body_from: usize,
    pub body_to: usize,
    pub source: String,
    pub dialect: String,
    pub display: bool,
    pub patchable: bool,
    pub source_revision: String,
    pub metadata: Option<String>,
    #[serde(skip)] pub document: MathDocument,
}

impl Occurrence {
    pub(crate) fn render(&self, analysis: &SourceAnalysis) -> Result<MathDocument, String> {
        analysis.plan.render_formula(&self.document, self.ordinal, self.display).map_err(|e| e.message())
    }
}

struct Expected<'a> { source: &'a str, omml: Option<&'a str>, display: bool }
fn inlines<'a>(values: &'a [Inline], out: &mut Vec<Expected<'a>>) {
    for value in values {
        match value {
            Inline::Math(source) => out.push(Expected {source, omml: None, display: false}),
            Inline::OfficeMath {latex, omml} => out.push(Expected {source: latex, omml: Some(omml), display: false}),
            Inline::Strong(v) | Inline::Emph(v) | Inline::Strike(v)
            | Inline::Styled {children: v, ..} | Inline::AuthorStyled {children: v, ..}
            | Inline::Display {children: v, ..} | Inline::Citation {children: v, ..}
            | Inline::Link {children: v, ..} => inlines(v, out),
            _ => (),
        }
    }
}
fn blocks<'a>(values: &'a [Block], out: &mut Vec<Expected<'a>>) {
    for value in values {
        match value {
            Block::Math(source) => out.push(Expected {source, omml: None, display: true}),
            Block::OfficeMath {latex, omml} => out.push(Expected {source: latex, omml: Some(omml), display: true}),
            Block::Styled {block, ..} => blocks(std::slice::from_ref(block.as_ref()), out),
            Block::Heading {inlines: v, ..} | Block::Caption {inlines: v, ..} | Block::Paragraph(v) => inlines(v, out),
            Block::Quote(v) | Block::FootnoteDef {blocks: v, ..} => blocks(v, out),
            Block::List {items, ..} => for item in items { blocks(&item.blocks, out); },
            Block::Table {header, rows, ..} => {
                for cell in header { inlines(cell, out); }
                for row in rows { for cell in row { inlines(cell, out); } }
            },
            // Generated bibliography math has no author-source occurrence.
            // A nonempty unmatched inventory is rejected below.
            _ => (),
        }
    }
}

struct Raw { from: usize, to: usize, body_from: usize, body_to: usize, source: String, dsl: bool, display: bool, patchable: bool }
fn inventory(source: &str) -> Result<Vec<Raw>, String> {
    let mut dsls = crate::math_dsl::markdown_ranges(source).map_err(str::to_owned)?;
    dsls.sort_by_key(|r| (r.from, r.to));
    if dsls.windows(2).any(|r| r[0].to > r[1].from) { return Err(INVALID.into()); }
    let mut raw = dsls.iter().map(|r| Raw {from: r.from, to: r.to, body_from: r.body_from,
        body_to: r.body_to, source: source[r.body_from..r.body_to].into(), dsl: true, display: r.display, patchable: true}).collect::<Vec<_>>();
    let mut fence: Option<(usize, String, Option<usize>, usize)> = None;
    for (event, range) in Parser::new_ext(source, md_ast::options()).into_offset_iter() {
        match &event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) if info.trim() == "readmd-math v1" => {
                let next=dsls.partition_point(|r|r.to<=range.start);
                if !dsls.get(next).is_some_and(|r|range.start<r.to&&range.end>r.from) {
                    fence=Some((range.start,String::new(),None,range.start));
                }
            },
            Event::Text(text) => if let Some((_,body,first,last))=&mut fence {
                if body.len().checked_add(text.len()).is_none_or(|len|len>tx::MAX_DOCUMENT_BYTES) {return Err("math_budget".into());}
                body.push_str(text); first.get_or_insert(range.start); *last=range.end;
            },
            Event::End(TagEnd::CodeBlock) => if let Some((from,body,first,last))=fence.take() {
                let body_from=first.ok_or("math_invalid_source")?;
                let body_to=source[..last].trim_end_matches(['\r','\n']).len();
                if body_from>body_to || body_to>range.end {return Err(INVALID.into());}
                raw.push(Raw {from,to:range.end,body_from,body_to,source:body.trim_end_matches(['\r','\n']).into(),dsl:true,display:true,patchable:false});
            },
            _=>(),
        }
        let (body, display) = match event {
            Event::InlineMath(body) => (body, false), Event::DisplayMath(body) => (body, true), _ => continue,
        };
        let next = dsls.partition_point(|r| r.to <= range.start);
        if dsls.get(next).is_some_and(|r| range.end > r.from) { continue; }
        let text = source.get(range.clone()).ok_or(INVALID)?;
        let delimiter = if text.starts_with("$$") && text.ends_with("$$") { 2 }
            else if text.starts_with('$') && text.ends_with('$') { 1 }
            else { return Err(INVALID.into()); };
        if text.len() < 2 * delimiter { return Err(INVALID.into()); }
        raw.push(Raw {from: range.start, to: range.end, body_from: range.start + delimiter,
            body_to: range.end - delimiter, source: body.into_string(), dsl: false, display, patchable:true});
        if raw.len() > tx::MAX_OPERATIONS { return Err("math_budget".into()); }
    }
    raw.sort_by_key(|r| (r.from, r.to));
    if raw.len() > tx::MAX_OPERATIONS || raw.windows(2).any(|r| r[0].to > r[1].from) {
        return Err("math_budget".into());
    }
    Ok(raw)
}

fn positions(source: &str, raw: &[Raw]) -> Result<BTreeMap<usize, usize>, String> {
    if raw.is_empty() { return Ok(BTreeMap::new()); }
    let mut endpoints = raw.iter().flat_map(|r| [r.from, r.to, r.body_from, r.body_to]).collect::<Vec<_>>();
    endpoints.sort_unstable(); endpoints.dedup();
    let mut out = BTreeMap::new(); let mut next = 0; let mut utf16 = 0;
    for (byte, ch) in source.char_indices() {
        if endpoints.get(next) == Some(&byte) { out.insert(byte, utf16); next += 1; }
        if endpoints.get(next).is_some_and(|end| *end < byte) { return Err(INVALID.into()); }
        utf16 += ch.len_utf16();
    }
    if endpoints.get(next) == Some(&source.len()) { out.insert(source.len(), utf16); next += 1; }
    if next != endpoints.len() { return Err(INVALID.into()); }
    Ok(out)
}

fn adjacent_metadata(source: &str, after: usize) -> Result<Option<String>, String> {
    let tail = &source[after..];
    let tail = tail.trim_start_matches(char::is_whitespace);
    if !tail.starts_with("<!--readmd:math:") { return Ok(None); }
    let end = tail.find("-->").ok_or("math_metadata_invalid")? + 3;
    Ok(Some(tail[..end].into()))
}

pub(crate) fn analyze_source(content: &str) -> Result<SourceAnalysis, String> {
    if content.len() > tx::MAX_DOCUMENT_BYTES { return Err("math_budget".into()); }
    // Fail closed on unsupported/invalid DSL instead of md_ast's tolerant read
    // fallback turning it into an ordinary, inheriting expression.
    if !crate::math_dsl::version_diagnostics(content).is_empty() { return Err("math_unsupported_version".into()); }
    crate::math_dsl::expand_markdown(content).map_err(str::to_owned)?;
    let author = md_ast::parse(content);
    let plan = DocumentDefaultsPlan::analyze(&author).map_err(|e| e.message())?;
    let mut expected = Vec::new(); blocks(&author.blocks, &mut expected);
    let raw = inventory(content)?;
    if raw.len() != expected.len() || raw.len() != plan.formula_count() { return Err(INVALID.into()); }
    let positions = positions(content, &raw)?;
    let mut occurrences = Vec::with_capacity(raw.len());
    for (ordinal, (raw, expected)) in raw.into_iter().zip(expected).enumerate() {
        let metadata = adjacent_metadata(content, raw.to)?;
        let (source, dialect, mut document) = if raw.dsl {
            if !expected.display.eq(&raw.display) || expected.omml.is_none() { return Err(INVALID.into()); }
            let document = if let Some(comment) = metadata.as_deref() {
                let resolved=crate::math_extension::resolve(&raw.source, comment, None, raw.display).map_err(str::to_owned)?;
                let restored=math_document::parse(&resolved.xml,"omml",raw.display).map_err(str::to_owned)?;
                if math_document::xml_semantics(&restored.root)!=math_document::xml_semantics(&resolved.document.root) {
                    return Err("math_unsupported_extension".into());
                }
                resolved.document
            } else { math_document::parse(&raw.source, "dsl", raw.display).map_err(str::to_owned)? };
            (raw.source.clone(), "dsl", document)
        } else {
            // md_ast trims display blocks, but preserves inline source. Use
            // that exact parser projection; the raw source ranges stay intact.
            let source = if expected.display { raw.source.trim() } else { &raw.source };
            if source != expected.source || expected.display && !raw.display { return Err(INVALID.into()); }
            let document = if let Some(comment) = metadata.as_deref() {
                let (native, current) = math_document::read_metadata(comment, source).map_err(str::to_owned)?;
                if !current { return Err("math_metadata_stale".into()); }
                if expected.omml != Some(native.omml.as_str()) { return Err(INVALID.into()); }
                math_document::parse(&native.omml, "omml", expected.display).map_err(str::to_owned)?
            } else {
                if expected.omml.is_some() { return Err(INVALID.into()); }
                math_document::parse(source, "latex", expected.display).map_err(str::to_owned)?
            };
            (source.into(), "latex", document)
        };
        if let Some(xml) = expected.omml {
            let native = math_document::parse(xml, "omml", expected.display).map_err(str::to_owned)?;
            if math_document::xml_semantics(&native.root) != math_document::xml_semantics(&document.root) { return Err(INVALID.into()); }
        } else if !plan.is_ordinary(ordinal) { return Err(INVALID.into()); }
        // This also proves the ordinal belongs to the complete author AST and
        // rejects altered models before the context is retained or published.
        plan.render_formula(&document, ordinal, expected.display).map_err(|e| e.message())?;
        let utf16 = |byte| positions[&byte];
        // Explicit assignment prevents a later renderer's derived revision
        // from becoming the author/model revision retained by the context.
        document.revision = tx::revision(&document.source);
        occurrences.push(Occurrence {ordinal, byte_from: raw.from, byte_to: raw.to,
            body_byte_from: raw.body_from, body_byte_to: raw.body_to,
            from: utf16(raw.from), to: utf16(raw.to), body_from: utf16(raw.body_from), body_to: utf16(raw.body_to),
            source_revision: tx::revision(&source), source, dialect: dialect.into(), display: expected.display,
            metadata, patchable:raw.patchable, document});
    }
    Ok(SourceAnalysis {author, plan, occurrences})
}

#[cfg(test)]
#[path = "math_source_context_tests.rs"]
mod tests;
