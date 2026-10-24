//! Source-bound document defaults for otherwise unconfigured ordinary LaTeX.
//! The author AST and MathDocument remain the source of truth. Only an ephemeral
//! rendering copy receives dispDef=false and the four admitted display spacings.
use crate::{document_transaction as transaction, math_defaults::{DefaultsError, EffectiveDefaults, MathDefaultsState}, math_document::{self, MathDocument, XmlNode, XmlPart, MATH_NS}, md_ast::{Block, Document, Inline}};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{borrow::Cow, io::{self, Write}};

const MAX_DEPTH: usize = 128;
const MAX_AST_NODES: usize = transaction::MAX_OPERATIONS * 16;
const MAX_RENDER_XML_BYTES: usize = 1024 * 1024;

fn source(code: impl Into<String>) -> DefaultsError { DefaultsError::Source(code.into()) }
fn check_cancel() -> Result<(), DefaultsError> { if crate::speech::cancelled() { Err(source("cancelled")) } else { Ok(()) } }
fn conflict() -> DefaultsError { source("transaction_conflict") }

#[derive(Debug)]
struct Occurrence {
    ordinary: bool,
    display: bool,
    latex_digest: String,
    native_root_digest: Option<String>,
}

#[derive(Debug)]
struct Policy { math_pr: XmlNode, effective: EffectiveDefaults }

/// Immutable, non-serialized analysis of one author AST. Projected OfficeMath
/// nodes never acquire author provenance: this plan retains their original role.
#[derive(Debug)]
pub(crate) struct DocumentDefaultsPlan {
    fingerprint: String,
    occurrences: Vec<Occurrence>,
    policy: Option<Policy>,
}

struct Formula<'a> { latex: &'a str, omml: Option<&'a str>, display: bool }
struct Inventory<'a> { fingerprint: String, formulas: Vec<Formula<'a>>, bytes: usize }

/// A streaming fingerprint, not Debug/JSON cloning of an entire document.
struct Scan<'a> {
    digest: Sha256,
    bytes: usize,
    nodes: usize,
    io_error: Option<&'static str>,
    formulas: Vec<Formula<'a>>,
}
impl<'a> Scan<'a> {
    fn new() -> Self { Self { digest: Sha256::new(), bytes: 0, nodes: 0, io_error: None, formulas: Vec::new() } }
    fn tag(&mut self, tag: &str) { self.digest.update((tag.len() as u64).to_le_bytes()); self.digest.update(tag.as_bytes()); }
    fn absorb(&mut self, bytes: &[u8]) -> Result<(), &'static str> {
        self.bytes = self.bytes.checked_add(bytes.len()).filter(|n| *n <= transaction::MAX_DOCUMENT_BYTES).ok_or("math_budget")?;
        for chunk in bytes.chunks(16384) { if crate::speech::cancelled() { return Err("cancelled"); } self.digest.update(chunk); }
        Ok(())
    }
    fn field(&mut self, value: &str) -> Result<(), DefaultsError> { self.digest.update((value.len() as u64).to_le_bytes()); self.absorb(value.as_bytes()).map_err(source) }
    fn step(&mut self, depth: usize) -> Result<(), DefaultsError> {
        check_cancel()?;
        self.nodes = self.nodes.checked_add(1).ok_or_else(|| source("math_budget"))?;
        if depth > MAX_DEPTH || self.nodes > MAX_AST_NODES { return Err(source("math_budget")); }
        Ok(())
    }
    fn json<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), DefaultsError> {
        self.tag("json"); self.io_error = None;
        if serde_json::to_writer(&mut *self, value).is_err() { return Err(source(self.io_error.take().unwrap_or("math_budget"))); }
        self.tag("/json"); Ok(())
    }
    fn value_depth(&mut self, value: &serde_json::Value, depth: usize) -> Result<(), DefaultsError> {
        self.step(depth)?;
        match value { serde_json::Value::Array(values) => for child in values { self.value_depth(child, depth + 1)?; }, serde_json::Value::Object(values) => for child in values.values() { self.value_depth(child, depth + 1)?; }, _ => () }
        Ok(())
    }
    fn formula(&mut self, latex: &'a str, omml: Option<&'a str>, display: bool) -> Result<(), DefaultsError> {
        if self.formulas.len() >= transaction::MAX_OPERATIONS { return Err(source("math_budget")); }
        self.tag(if omml.is_some() { "office-math" } else { "ordinary-latex" }); self.tag(if display { "display" } else { "inline" }); self.field(latex)?;
        if let Some(xml) = omml { self.field(xml)?; }
        self.formulas.push(Formula { latex, omml, display }); Ok(())
    }
    fn inlines(&mut self, values: &'a [Inline], depth: usize) -> Result<(), DefaultsError> {
        self.tag("inlines");
        for value in values {
            self.step(depth)?;
            match value {
                Inline::Text(s) => { self.tag("text"); self.field(s)?; }, Inline::Code(s) => { self.tag("code"); self.field(s)?; },
                Inline::Strong(v) => { self.tag("strong"); self.inlines(v, depth + 1)?; }, Inline::Emph(v) => { self.tag("emph"); self.inlines(v, depth + 1)?; }, Inline::Strike(v) => { self.tag("strike"); self.inlines(v, depth + 1)?; },
                Inline::Styled { format, children } => { self.tag("styled"); self.json(format)?; self.inlines(children, depth + 1)?; },
                Inline::AuthorStyled { style, children } => { self.tag("author-styled"); self.json(style)?; self.inlines(children, depth + 1)?; },
                Inline::Display { display, children } => { self.tag("display"); self.json(display)?; self.inlines(children, depth + 1)?; },
                Inline::Citation { source, children } => { self.tag("citation"); self.json(source)?; self.inlines(children, depth + 1)?; },
                Inline::Link { href, title, children } => { self.tag("link"); self.field(href)?; self.field(title)?; self.inlines(children, depth + 1)?; },
                Inline::Image { src, alt, title } => { self.tag("image"); self.field(src)?; self.field(alt)?; self.field(title)?; },
                Inline::Math(latex) => self.formula(latex, None, false)?, Inline::OfficeMath { latex, omml } => self.formula(latex, Some(omml), false)?,
                Inline::SoftBreak => self.tag("soft-break"), Inline::HardBreak => self.tag("hard-break"),
                Inline::FootnoteRef(s) => { self.tag("footnote-ref"); self.field(s)?; }, Inline::Html(s) => { self.tag("html"); self.field(s)?; },
            }
            self.tag("/inline");
        }
        self.tag("/inlines"); Ok(())
    }
    fn rich(&mut self, values: &[crate::publication_rich::RichNode], depth: usize) -> Result<(), DefaultsError> {
        for value in values { self.step(depth)?; match value {
            crate::publication_rich::RichNode::Text { text, format } => { self.tag("rich-text"); self.field(text)?; self.json(format)?; },
            crate::publication_rich::RichNode::Link { href, children } => { self.tag("rich-link"); self.field(href)?; self.rich(children, depth + 1)?; },
            crate::publication_rich::RichNode::Group { children, display, entry_index } => { self.tag("rich-group"); self.json(&(display, entry_index))?; self.rich(children, depth + 1)?; },
        } self.tag("/rich"); } Ok(())
    }
    fn blocks(&mut self, values: &'a [Block], depth: usize) -> Result<(), DefaultsError> {
        self.tag("blocks");
        for value in values {
            self.step(depth)?;
            match value {
                Block::Styled { attributes, block } => { self.tag("styled"); self.value_depth(attributes, depth + 1)?; self.json(attributes)?; self.blocks(std::slice::from_ref(block.as_ref()), depth + 1)?; },
                Block::Heading { level, id, inlines, numbering } => { self.tag("heading"); self.json(&(level, numbering))?; self.field(id)?; self.inlines(inlines, depth + 1)?; },
                Block::Caption { label, inlines } => { self.tag("caption"); self.json(label)?; self.inlines(inlines, depth + 1)?; },
                Block::Paragraph(v) => { self.tag("paragraph"); self.inlines(v, depth + 1)?; },
                Block::List { ordered, start, items } => { self.tag("list"); self.json(&(ordered, start))?; for item in items { self.step(depth + 1)?; self.json(&item.task)?; self.blocks(&item.blocks, depth + 1)?; } },
                Block::Quote(v) => { self.tag("quote"); self.blocks(v, depth + 1)?; },
                Block::Code { lang, text } => { self.tag("code"); self.field(lang)?; self.field(text)?; },
                Block::Math(latex) => self.formula(latex, None, true)?, Block::OfficeMath { latex, omml } => self.formula(latex, Some(omml), true)?,
                Block::Table { aligns, header, rows } => { self.tag("table"); for align in aligns { self.tag(align.as_str()); } self.tag("header"); for cell in header { self.inlines(cell, depth + 1)?; } self.tag("rows"); for row in rows { self.tag("row"); for cell in row { self.inlines(cell, depth + 1)?; } } },
                Block::Hr => self.tag("hr"), Block::PageBreak => self.tag("page-break"),
                Block::SectionStart(section) => { self.tag("section"); self.json(section)?; },
                Block::Html(s) => { self.tag("html"); self.field(s)?; },
                Block::FootnoteDef { label, blocks } => { self.tag("footnote"); self.field(label)?; self.blocks(blocks, depth + 1)?; },
                Block::Bibliography(bib) => { self.tag("bibliography"); self.field(&bib.title)?; self.json(&(bib.hanging_indent, bib.line_spacing, bib.entry_spacing, &bib.source_span))?; for item in &bib.items { self.step(depth + 1)?; self.field(&item.key)?; self.rich(&item.rich, depth + 1)?; self.tag("first-field"); self.rich(&item.first_field, depth + 1)?; } },
            }
            self.tag("/block");
        }
        self.tag("/blocks"); Ok(())
    }
}
impl Write for Scan<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> { match self.absorb(bytes) { Ok(()) => Ok(bytes.len()), Err(code) => { self.io_error = Some(code); Err(io::Error::new(io::ErrorKind::Other, code)) } } }
    fn flush(&mut self) -> io::Result<()> { Ok(()) }
}
// Do not impose mathematical clone/fingerprint budgets on documents containing
// no math. Iterators retain only the nesting spine, even for a million leaves.
fn has_math(doc: &Document) -> Result<bool, DefaultsError> {
    enum Work<'a> { Blocks(std::slice::Iter<'a, Block>), Inlines(std::slice::Iter<'a, Inline>), Items(std::slice::Iter<'a, crate::md_ast::ListItem>), Cells(std::slice::Iter<'a, Vec<Inline>>), Rows(std::slice::Iter<'a, Vec<Vec<Inline>>>) }
    enum Item<'a> { Block(&'a Block), Inline(&'a Inline), List(&'a crate::md_ast::ListItem), Cell(&'a Vec<Inline>), Row(&'a Vec<Vec<Inline>>) }
    let mut pending = vec![Work::Blocks(doc.blocks.iter())];
    while let Some(work) = pending.last_mut() {
        check_cancel()?;
        let item = match work { Work::Blocks(v) => v.next().map(Item::Block), Work::Inlines(v) => v.next().map(Item::Inline), Work::Items(v) => v.next().map(Item::List), Work::Cells(v) => v.next().map(Item::Cell), Work::Rows(v) => v.next().map(Item::Row) };
        let Some(item) = item else { pending.pop(); continue; };
        match item {
            Item::Block(Block::Math(_) | Block::OfficeMath { .. }) | Item::Inline(Inline::Math(_) | Inline::OfficeMath { .. }) => return Ok(true),
            Item::Block(Block::Styled { block, .. }) => pending.push(Work::Blocks(std::slice::from_ref(block.as_ref()).iter())),
            Item::Block(Block::Heading { inlines, .. } | Block::Caption { inlines, .. } | Block::Paragraph(inlines)) => pending.push(Work::Inlines(inlines.iter())),
            Item::Block(Block::Quote(v) | Block::FootnoteDef { blocks: v, .. }) => pending.push(Work::Blocks(v.iter())),
            Item::Block(Block::List { items, .. }) => pending.push(Work::Items(items.iter())),
            Item::Block(Block::Table { header, rows, .. }) => { pending.push(Work::Rows(rows.iter())); pending.push(Work::Cells(header.iter())); },
            Item::Inline(Inline::Strong(v) | Inline::Emph(v) | Inline::Strike(v) | Inline::Styled { children: v, .. } | Inline::AuthorStyled { children: v, .. } | Inline::Display { children: v, .. } | Inline::Citation { children: v, .. } | Inline::Link { children: v, .. }) => pending.push(Work::Inlines(v.iter())),
            Item::List(item) => pending.push(Work::Blocks(item.blocks.iter())), Item::Cell(cell) => pending.push(Work::Inlines(cell.iter())), Item::Row(row) => pending.push(Work::Cells(row.iter())), _ => (),
        }
    }
    Ok(false)
}
fn inventory(doc: &Document) -> Result<Inventory<'_>, DefaultsError> {
    if !has_math(doc)? { return Ok(Inventory { fingerprint: transaction::revision("document-math-defaults-v1:no-math"), formulas: Vec::new(), bytes: 0 }); }
    let mut scan = Scan::new(); scan.step(0)?; scan.tag("document-math-defaults-v1");
    if let Some(front) = &doc.front_matter { scan.tag("front"); scan.field(front)?; } else { scan.tag("no-front"); }
    scan.blocks(&doc.blocks, 1)?;
    Ok(Inventory { fingerprint: format!("{:x}", scan.digest.finalize()), formulas: scan.formulas, bytes: scan.bytes })
}

fn make_policy(spacings: [u16; 4]) -> Result<Policy, DefaultsError> {
    let [pre, post, inter, intra] = spacings;
    let root = math_document::parse_xml(&format!("<m:mathPr xmlns:m=\"{MATH_NS}\"><m:dispDef m:val=\"0\"/><m:preSp m:val=\"{pre}\"/><m:postSp m:val=\"{post}\"/><m:interSp m:val=\"{inter}\"/><m:intraSp m:val=\"{intra}\"/></m:mathPr>")).map_err(source)?;
    let effective = MathDefaultsState::from_xml(&root).map_err(source)?.effective;
    let mut math_pr = root.children().next().ok_or_else(|| source("math_invalid_xml"))?.clone();
    fn ids(node: &mut XmlNode, id: String) { node.id = id; let mut index = 0; for part in &mut node.content { if let XmlPart::Element(child) = part { ids(child, format!("{}.{}", node.id, index)); index += 1; } } }
    ids(&mut math_pr, "rd-document-defaults".into()); Ok(Policy { math_pr, effective })
}

impl DocumentDefaultsPlan {
    pub(crate) fn analyze(author: &Document) -> Result<Self, DefaultsError> {
        let scan = inventory(author)?; let mut occurrences = Vec::with_capacity(scan.formulas.len()); let mut states = Vec::with_capacity(scan.formulas.len());
        for formula in &scan.formulas {
            check_cancel()?;
            let (state, native_root_digest) = if let Some(xml) = formula.omml {
                let root = math_document::parse_xml(xml).map_err(source)?;
                (Some(MathDefaultsState::from_xml(&root).map_err(source)?), Some(transaction::revision(&root.xml())))
            } else { (None, None) };
            occurrences.push(Occurrence { ordinary: formula.omml.is_none(), display: formula.display, latex_digest: transaction::revision(formula.latex), native_root_digest }); states.push(state);
        }
        let has_false = states.iter().flatten().any(|state| !state.effective.display_defaults);
        // Inline providers impose global wrapping mode, never display spacings.
        let spacings = scan.formulas.iter().zip(&states).find_map(|(formula, state)| state.as_ref().filter(|_| formula.display).map(|state| { let e = &state.effective; [e.pre_spacing, e.post_spacing, e.inter_spacing, e.intra_spacing] })).unwrap_or([0; 4]);
        let policy = if has_false { Some(make_policy(spacings)?) } else { None };
        let ordinary = match &policy { Some(policy) => policy.effective.clone(), None => MathDefaultsState::from_xml(&math_document::parse_xml("<m:oMath/>").map_err(source)?).map_err(source)?.effective };
        let mut global: Option<(&EffectiveDefaults, usize)> = None; let mut display: Option<(&EffectiveDefaults, usize)> = None;
        for (index, (formula, state)) in scan.formulas.iter().zip(&states).enumerate() {
            check_cancel()?; let effective = state.as_ref().map(|state| &state.effective).unwrap_or(&ordinary); let formula_index = index + 1;
            if let Some((first, first_formula)) = global { if let Some(property) = first.global_difference(effective) { return Err(DefaultsError::Conflict { property, first_formula, formula: formula_index }); } }
            if formula.display { if let Some((first, first_formula)) = display { if let Some(property) = first.display_difference(effective) { return Err(DefaultsError::Conflict { property, first_formula, formula: formula_index }); } } display.get_or_insert((effective, formula_index)); }
            global.get_or_insert((effective, formula_index));
        }
        Ok(Self { fingerprint: scan.fingerprint, occurrences, policy })
    }
    pub(crate) fn fingerprint(&self) -> &str { &self.fingerprint }
    pub(crate) fn formula_count(&self) -> usize { self.occurrences.len() }
    pub(crate) fn is_ordinary(&self, occurrence: usize) -> bool { self.occurrences.get(occurrence).is_some_and(|entry| entry.ordinary) }
    pub(crate) fn is_active(&self) -> bool { self.policy.is_some() && self.occurrences.iter().any(|entry| entry.ordinary) }
    pub(crate) fn validate_doc(&self, author: &Document) -> Result<(), DefaultsError> { if inventory(author)?.fingerprint == self.fingerprint { Ok(()) } else { Err(conflict()) } }

    /// Source and ordinal binding is checked even for inactive/native copies.
    pub(crate) fn render_formula(&self, author: &MathDocument, occurrence: usize, display: bool) -> Result<MathDocument, DefaultsError> {
        check_cancel()?; let entry = self.occurrences.get(occurrence).ok_or_else(conflict)?;
        if entry.display != display || author.revision != transaction::revision(&author.source) { return Err(conflict()); }
        if entry.ordinary && (author.dialect != "latex" || transaction::revision(&author.source) != entry.latex_digest) || !entry.ordinary && author.dialect == "latex" { return Err(conflict()); }
        let expected = math_document::parse(&author.source, &author.dialect, display).map_err(source)?;
        if author.root != expected.root || !same_nodes(author, &expected) { return Err(conflict()); }
        if let Some(expected_digest) = &entry.native_root_digest { if transaction::revision(&author.root.xml()) != *expected_digest { return Err(conflict()); } }
        drop(expected);
        let mut rendered = author.clone();
        if entry.ordinary { if let Some(policy) = &self.policy { rendered.root.content.insert(0, XmlPart::Element(policy.math_pr.clone())); if rendered.root.xml().len() > MAX_RENDER_XML_BYTES { return Err(source("math_budget")); } } }
        check_cancel()?; Ok(rendered)
    }
    pub(crate) fn render_document<'a>(&self, author: &'a Document) -> Result<Cow<'a, Document>, DefaultsError> {
        let scan = inventory(author)?; if scan.fingerprint != self.fingerprint { return Err(conflict()); }
        if !self.is_active() { return Ok(Cow::Borrowed(author)); }
        let mut replacement = Vec::with_capacity(scan.formulas.len()); let mut projected_bytes = scan.bytes;
        for (index, formula) in scan.formulas.iter().enumerate() {
            check_cancel()?;
            if formula.omml.is_some() { replacement.push(None); continue; }
            let original = math_document::parse(formula.latex, "latex", formula.display).map_err(source)?;
            let rendered = self.render_formula(&original, index, formula.display)?; let xml = rendered.root.xml();
            projected_bytes = projected_bytes.checked_add(xml.len()).filter(|n| *n <= transaction::MAX_DOCUMENT_BYTES).ok_or_else(|| source("math_budget"))?;
            replacement.push(Some(xml));
        }
        check_cancel()?; let mut rendered = author.clone(); let mut occurrence = 0; rewrite_blocks(&mut rendered.blocks, &mut replacement, &mut occurrence)?;
        if occurrence != self.occurrences.len() { return Err(conflict()); } Ok(Cow::Owned(rendered))
    }
}

fn same_nodes(a: &MathDocument, b: &MathDocument) -> bool {
    a.nodes.len() == b.nodes.len() && a.nodes.iter().zip(&b.nodes).all(|(a,b)| a.id == b.id && a.kind == b.kind && a.from == b.from && a.to == b.to && a.tex == b.tex && a.slots.len() == b.slots.len() && a.slots.iter().zip(&b.slots).all(|(a,b)| a.role == b.role && a.from == b.from && a.to == b.to))
        && a.diagnostics.len() == b.diagnostics.len() && a.diagnostics.iter().zip(&b.diagnostics).all(|(a,b)| a.code == b.code && a.from == b.from && a.to == b.to)
}
fn rewrite_inlines(values: &mut [Inline], replacements: &mut [Option<String>], occurrence: &mut usize) -> Result<(), DefaultsError> {
    for value in values { check_cancel()?; match value {
        Inline::Math(latex) => { let xml = replacements.get_mut(*occurrence).ok_or_else(conflict)?.take(); *occurrence += 1; if let Some(omml) = xml { *value = Inline::OfficeMath { latex: std::mem::take(latex), omml }; } },
        Inline::OfficeMath { .. } => { if replacements.get(*occurrence).ok_or_else(conflict)?.is_some() { return Err(conflict()); } *occurrence += 1; },
        Inline::Strong(v) | Inline::Emph(v) | Inline::Strike(v) | Inline::Styled { children: v, .. } | Inline::AuthorStyled { children: v, .. } | Inline::Display { children: v, .. } | Inline::Citation { children: v, .. } | Inline::Link { children: v, .. } => rewrite_inlines(v, replacements, occurrence)?, _ => (),
    } } Ok(())
}
fn rewrite_blocks(values: &mut [Block], replacements: &mut [Option<String>], occurrence: &mut usize) -> Result<(), DefaultsError> {
    for value in values { check_cancel()?; match value {
        Block::Math(latex) => { let xml = replacements.get_mut(*occurrence).ok_or_else(conflict)?.take(); *occurrence += 1; if let Some(omml) = xml { *value = Block::OfficeMath { latex: std::mem::take(latex), omml }; } },
        Block::OfficeMath { .. } => { if replacements.get(*occurrence).ok_or_else(conflict)?.is_some() { return Err(conflict()); } *occurrence += 1; },
        Block::Styled { block, .. } => rewrite_blocks(std::slice::from_mut(block.as_mut()), replacements, occurrence)?,
        Block::Heading { inlines, .. } | Block::Caption { inlines, .. } | Block::Paragraph(inlines) => rewrite_inlines(inlines, replacements, occurrence)?,
        Block::Quote(v) | Block::FootnoteDef { blocks: v, .. } => rewrite_blocks(v, replacements, occurrence)?,
        Block::List { items, .. } => for item in items { rewrite_blocks(&mut item.blocks, replacements, occurrence)?; },
        Block::Table { header, rows, .. } => { for cell in header { rewrite_inlines(cell, replacements, occurrence)?; } for row in rows { for cell in row { rewrite_inlines(cell, replacements, occurrence)?; } } }, _ => (),
    } } Ok(())
}

#[cfg(test)]
#[path = "math_document_defaults_tests.rs"]
mod tests;
