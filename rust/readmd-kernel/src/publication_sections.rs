//! Opt-in, source-owned publication sections. A declaration starts the following
//! top-level body; it never becomes visible text or changes the source document.
use crate::md_ast::{Block, Document};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashSet, ops::Range};

pub const PREFIX: &str = "<!-- readmd-section:";
pub const MAX_SECTIONS: usize = 128;
pub const MAX_DECLARATION_BYTES: usize = 8192;
const INVALID: &str = "publication_section_invalid";
const BUDGET: &str = "publication_section_budget";
const GEOMETRY: &str = "publication_section_geometry";
const CONTINUOUS_PAGE: &str = "publication_section_continuous_page";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SectionBreak {
    Continuous,
    #[default]
    NextPage,
    OddPage,
    EvenPage,
}
impl SectionBreak {
    pub fn word_value(self) -> &'static str {
        match self {
            Self::Continuous => "continuous",
            Self::NextPage => "nextPage",
            Self::OddPage => "oddPage",
            Self::EvenPage => "evenPage",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct PagePatch {
    pub size: Option<String>,
    pub orientation: Option<String>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub margin_top: Option<f64>,
    pub margin_right: Option<f64>,
    pub margin_bottom: Option<f64>,
    pub margin_left: Option<f64>,
    pub gutter: Option<f64>,
    pub columns: Option<u32>,
    pub column_gap: Option<f64>,
    pub different_first_page: Option<bool>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct NumberingPatch {
    pub format: Option<String>,
    pub start: Option<u32>,
    pub enabled: Option<bool>,
    pub show_total: Option<bool>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct RunningPatch {
    pub link_previous: Option<bool>,
    pub text: Option<String>,
    pub even_text: Option<String>,
    pub first_text: Option<String>,
    pub align: Option<String>,
    pub distance: Option<f64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Section {
    pub id: String,
    #[serde(default, rename = "break")]
    pub break_kind: SectionBreak,
    #[serde(default)]
    pub page: PagePatch,
    #[serde(default)]
    pub numbering: NumberingPatch,
    #[serde(default)]
    pub header: RunningPatch,
    #[serde(default)]
    pub footer: RunningPatch,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footnotes: Option<crate::publication_note_policies::SectionNotePolicies>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedSection {
    pub id: String,
    pub break_kind: SectionBreak,
    pub style: Value,
    /// Original sparse source fields. Equal effective values remain explicit
    /// overrides when the document-wide note policy is edited later.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note_overrides: Option<crate::publication_note_policies::SectionNotePolicies>,
    /// Indices in the ORIGINAL top-level Document.blocks, excluding the marker.
    pub block_range: Range<usize>,
    /// None means continue numbering, never inherit a prior restart.
    pub page_start: Option<u32>,
    pub header_link_previous: bool,
    pub footer_link_previous: bool,
    pub explicit: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct Declaration {
    pub section: Section,
    pub from: usize,
    pub to: usize,
    #[serde(skip)]
    pub bytes: Range<usize>,
}

fn number(value: Option<f64>, low: f64, high: f64) -> bool {
    value.is_none_or(|n| n.is_finite() && n >= low && n <= high)
}
fn running_valid(p: &RunningPatch) -> bool {
    [p.text.as_ref(), p.even_text.as_ref(), p.first_text.as_ref()]
        .into_iter()
        .flatten()
        .all(|t| t.chars().count() <= 500 && !t.chars().any(char::is_control))
        && p.align
            .as_deref()
            .is_none_or(|s| ["left", "center", "right"].contains(&s))
        && number(p.distance, 0., 50.)
        && (!(p.text.is_some()
            || p.even_text.is_some()
            || p.first_text.is_some()
            || p.align.is_some())
            || p.link_previous == Some(false))
}
pub fn validate_section(s: &Section) -> Result<(), &'static str> {
    if s.footnotes.as_ref().is_some_and(|notes|notes.validate().is_err()) { return Err(INVALID); }
    if s.id.is_empty()
        || s.id.len() > 64
        || !s.id.as_bytes()[0].is_ascii_alphabetic()
        || !s
            .id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        || !running_valid(&s.header)
        || !running_valid(&s.footer)
        || s.numbering
            .format
            .as_deref()
            .is_some_and(|v| !crate::publication_layout::NUMBER_FORMATS.contains(&v))
        || s.numbering.start.is_some_and(|v| !(1..=9999).contains(&v))
    {
        return Err(INVALID);
    }
    let p = &s.page;
    if p.size
        .as_deref()
        .is_some_and(|v| !crate::export_styles::PAGE_SIZES.contains(&v))
        || p.orientation
            .as_deref()
            .is_some_and(|v| !["portrait", "landscape"].contains(&v))
        || p.columns.is_some_and(|v| !(1..=3).contains(&v))
        || !number(p.column_gap, 1., 30.)
        || !number(p.gutter, 0., 40.)
        || [p.margin_top, p.margin_right, p.margin_bottom, p.margin_left]
            .iter()
            .any(|v| !number(*v, 0., 60.))
        || !number(p.width, 80., 600.)
        || !number(p.height, 80., 600.)
    {
        return Err(GEOMETRY);
    }
    if (p.width.is_some() || p.height.is_some()) && p.size.as_deref() != Some("Custom")
        || p.size.as_deref() == Some("Custom") && (p.width.is_none() || p.height.is_none())
    {
        return Err(GEOMETRY);
    }
    Ok(())
}
pub fn read_comment(raw: &str) -> Result<Option<Section>, &'static str> {
    let Some(rest) = raw.trim().strip_prefix(PREFIX) else {
        return Ok(None);
    };
    if raw.len() > MAX_DECLARATION_BYTES {
        return Err(BUDGET);
    }
    let payload = rest
        .strip_prefix("v1 ")
        .and_then(|v| v.strip_suffix("-->"))
        .ok_or(INVALID)?
        .trim();
    let s: Section = serde_json::from_str(payload).map_err(|_| INVALID)?;
    validate_section(&s)?;
    Ok(Some(s))
}
pub fn comment(s: &Section) -> Result<String, &'static str> {
    validate_section(s)?;
    fn sparse(v: &mut Value) {
        if let Some(o) = v.as_object_mut() {
            for child in o.values_mut() {
                sparse(child);
            }
            o.retain(|_, v| !v.is_null() && !v.as_object().is_some_and(|o| o.is_empty()));
        }
    }
    let mut value = serde_json::to_value(s).map_err(|_| INVALID)?;
    sparse(&mut value);
    let text = serde_json::to_string(&value)
        .map_err(|_| INVALID)?
        .replace('<', "\\u003c")
        .replace('>', "\\u003e");
    let out = format!("{PREFIX}v1 {text} -->");
    if out.len() > MAX_DECLARATION_BYTES {
        return Err(BUDGET);
    }
    Ok(out)
}

/// Parse only actual Markdown HTML-comment nodes. Literal fenced/indented code
/// and metadata remain literal; a section inside a list/quote/paragraph is invalid.
pub fn analyze(source: &str) -> Result<Vec<Declaration>, &'static str> {
    if !source.contains(PREFIX) {
        return Ok(Vec::new());
    }
    if source.len() > crate::document_transaction::MAX_DOCUMENT_BYTES {
        return Err(BUDGET);
    }
    let bom = usize::from(source.starts_with('\u{feff}')) * 3;
    let mut depth = 0usize;
    let mut html: Option<(usize, usize, String)> = None;
    let mut found: Vec<(Section, Range<usize>)> = Vec::new();
    for (event, range) in
        Parser::new_ext(&source[bom..], crate::md_ast::options()).into_offset_iter()
    {
        match event {
            Event::Start(Tag::HtmlBlock) => html = Some((range.start + bom, depth, String::new())),
            Event::End(TagEnd::HtmlBlock) => {
                if let Some((start, nesting, raw)) = html.take() {
                    if let Some(s) = read_comment(&raw)? {
                        if nesting != 0 {
                            return Err(INVALID);
                        }
                        found.push((s, start..range.end + bom));
                    }
                }
            }
            Event::Html(text) => {
                if let Some((_, _, raw)) = &mut html {
                    raw.push_str(&text);
                } else if let Some(s) = read_comment(&text)? {
                    if depth != 0 {
                        return Err(INVALID);
                    }
                    found.push((s, range.start + bom..range.end + bom));
                }
            }
            Event::InlineHtml(text) if text.trim().starts_with(PREFIX) => return Err(INVALID),
            Event::Start(_) => depth += 1,
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => (),
        }
        if found.len() > MAX_SECTIONS {
            return Err(BUDGET);
        }
    }
    let mut ids = HashSet::new();
    for (s, _) in &found {
        if !ids.insert(&s.id) {
            return Err(INVALID);
        }
    }
    // One linear conversion; Unicode source offsets are never used as bytes.
    let positions: HashSet<_> = found.iter().flat_map(|(_, r)| [r.start, r.end]).collect();
    let mut offsets = std::collections::HashMap::new();
    let mut units = 0;
    for (at, ch) in source.char_indices() {
        if positions.contains(&at) {
            offsets.insert(at, units);
        }
        units += ch.len_utf16();
    }
    offsets.insert(source.len(), units);
    found
        .into_iter()
        .map(|(section, bytes)| {
            Ok(Declaration {
                section,
                from: *offsets.get(&bytes.start).ok_or(INVALID)?,
                to: *offsets.get(&bytes.end).ok_or(INVALID)?,
                bytes,
            })
        })
        .collect()
}
pub fn validate_source(source: &str) -> Result<(), &'static str> {
    analyze(source).map(|_| ())
}
pub fn strip_for_view(source: &str) -> Result<String, &'static str> {
    let declarations = analyze(source)?;
    let mut out = String::with_capacity(source.len());
    let mut end = 0;
    for d in declarations {
        out.push_str(&source[end..d.bytes.start]);
        out.extend(
            source[d.bytes.clone()]
                .chars()
                .filter(|c| matches!(c, '\r' | '\n')),
        );
        end = d.bytes.end;
    }
    out.push_str(&source[end..]);
    Ok(out)
}
/// Convert valid comments without making infallible Markdown parsing panic.
/// Malformed reserved comments stay hidden HTML and are rejected by validation.
pub fn attach(blocks: &mut [Block]) {
    for b in blocks {
        match b {
            Block::Html(raw) => {
                if let Ok(Some(s)) = read_comment(raw) {
                    *b = Block::SectionStart(s);
                }
            }
            Block::Styled { block, .. } => attach(std::slice::from_mut(block.as_mut())),
            Block::Quote(inner) | Block::FootnoteDef { blocks: inner, .. } => attach(inner),
            Block::List { items, .. } => {
                for item in items {
                    attach(&mut item.blocks);
                }
            }
            _ => (),
        }
    }
}
pub fn has_sections(doc: &Document) -> bool {
    doc.blocks
        .iter()
        .any(|b| matches!(b, Block::SectionStart(_)))
}
pub fn validate_document(doc: &Document) -> Result<(), &'static str> {
    fn walk(
        blocks: &[Block],
        nested: bool,
        depth: usize,
        count: &mut usize,
        ids: &mut HashSet<String>,
    ) -> Result<(), &'static str> {
        if depth > 32 && !ids.is_empty() {
            return Err(BUDGET);
        }
        for b in blocks {
            *count += 1;
            if *count > 32768 && !ids.is_empty() {
                return Err(BUDGET);
            }
            match b {
                Block::SectionStart(s) => {
                    validate_section(s)?;
                    if nested || !ids.insert(s.id.clone()) {
                        return Err(INVALID);
                    }
                    if ids.len() > MAX_SECTIONS {
                        return Err(BUDGET);
                    }
                }
                Block::Html(raw) if raw.trim().starts_with(PREFIX) => {
                    // The forgiving AST keeps malformed declarations as HTML.
                    // Preserve the original geometry/budget/type diagnostic
                    // before rejecting a misplaced otherwise-valid marker.
                    read_comment(raw)?;
                    return Err(INVALID);
                },
                Block::Styled { block, .. } => walk(
                    std::slice::from_ref(block.as_ref()),
                    true,
                    depth + 1,
                    count,
                    ids,
                )?,
                Block::Quote(inner) | Block::FootnoteDef { blocks: inner, .. } => {
                    walk(inner, true, depth + 1, count, ids)?
                }
                Block::List { items, .. } => {
                    for item in items {
                        walk(&item.blocks, true, depth + 1, count, ids)?;
                    }
                }
                Block::Paragraph(_)
                | Block::Heading { .. }
                | Block::Caption { .. }
                | Block::Table { .. } => {
                    let mut invalid = false;
                    crate::md_ast::walk_inlines(std::slice::from_ref(b), &mut |i| {
                        if matches!(i, crate::md_ast::Inline::Html(raw) if raw.trim().starts_with(PREFIX))
                        {
                            invalid = true;
                        }
                    });
                    if invalid {
                        return Err(INVALID);
                    }
                }
                _ => (),
            }
        }
        Ok(())
    }
    let mut count = 0;
    let mut ids = HashSet::new();
    walk(&doc.blocks, false, 0, &mut count, &mut ids)?;
    if !ids.is_empty() && count > 32768 {
        return Err(BUDGET);
    }
    Ok(())
}
fn set(style: &mut Value, group: &str, key: &str, value: Value) {
    if !style.is_object() {
        *style = json!({});
    }
    if !style[group].is_object() {
        style[group] = json!({});
    }
    style[group][key] = value;
}
fn apply_patch(style: &mut Value, s: &Section) {
    let page = serde_json::to_value(&s.page).expect("finite validated page values");
    for (key, v) in page.as_object().expect("page object") {
        if !v.is_null() {
            set(style, "page", key, v.clone());
        }
    }
    for (key, v) in [
        (
            "numberFormat",
            s.numbering.format.as_ref().map(|v| json!(v)),
        ),
        ("pageNumbers", s.numbering.enabled.map(|v| json!(v))),
        ("showTotal", s.numbering.show_total.map(|v| json!(v))),
    ] {
        if let Some(v) = v {
            set(style, "footer", key, v);
        }
    }
    for (group, patch) in [("header", &s.header), ("footer", &s.footer)] {
        if patch.link_previous == Some(false) {
            // An independent, omitted mode is blank, never the preceding text.
            for key in ["text", "evenText", "firstText"] {
                set(style, group, key, json!(""));
            }
        }
        let values = serde_json::to_value(patch).expect("finite running values");
        for (key, v) in values.as_object().expect("running object") {
            if key != "linkPrevious" && !v.is_null() {
                set(style, group, key, v.clone());
            }
        }
    }
}
fn geometry(style: &Value) -> Result<(), &'static str> {
    let (w, h) = crate::export_styles::page_dimensions(style);
    let n = |group: &str, key: &str, d: f64| style[group][key].as_f64().unwrap_or(d);
    let (left, right, top, bottom) = (
        n("page", "marginLeft", 18.),
        n("page", "marginRight", 18.),
        n("page", "marginTop", 20.),
        n("page", "marginBottom", 20.),
    );
    let columns = n("page", "columns", 1.);
    let width =
        (w - left - right - n("page", "gutter", 0.) - (columns - 1.) * n("page", "columnGap", 8.))
            / columns;
    if ![w, h, width, left, right, top, bottom]
        .iter()
        .all(|v| v.is_finite())
        || width < 20.
        || h - top - bottom < 20.
    {
        return Err(GEOMETRY);
    }
    let orientation = style["page"]["orientation"].as_str().unwrap_or("portrait");
    if orientation == "portrait" && w > h || orientation == "landscape" && w < h {
        return Err(GEOMETRY);
    }
    crate::publication_layout::Layout::from_style(style)
        .validate(style)
        .map_err(|_| GEOMETRY)
}
fn page_signature(style: &Value) -> (f64, f64, [f64; 5], String) {
    let (w, h) = crate::export_styles::page_dimensions(style);
    (
        w,
        h,
        [
            style["page"]["marginTop"].as_f64().unwrap_or(20.),
            style["page"]["marginRight"].as_f64().unwrap_or(18.),
            style["page"]["marginBottom"].as_f64().unwrap_or(20.),
            style["page"]["marginLeft"].as_f64().unwrap_or(18.),
            style["page"]["gutter"].as_f64().unwrap_or(0.),
        ],
        style["page"]["orientation"]
            .as_str()
            .unwrap_or("portrait")
            .into(),
    )
}
pub fn resolve(doc: &Document, global: &Value) -> Result<Vec<ResolvedSection>, &'static str> {
    validate_document(doc)?;
    let mut sections = Vec::new();
    let mut style = global.clone();
    let mut begin = 0;
    let has_section_notes=crate::publication_note_policies::has_section_policies(doc);
    let inherit_notes=crate::publication_note_policies::SectionNotePolicies::default();
    let mut current = ResolvedSection {
        id: "_document".into(),
        break_kind: SectionBreak::NextPage,
        style: style.clone(),
        note_overrides: None,
        block_range: 0..0,
        page_start: Some(crate::publication_layout::Layout::from_style(global).page_start),
        header_link_previous: false,
        footer_link_previous: false,
        explicit: false,
    };
    for (index, b) in doc.blocks.iter().enumerate() {
        if let Block::SectionStart(s) = b {
            // Leading definitions/hidden HTML belong to the document, not an empty
            // extra printed section. Definitions remain in the original AST.
            let visible_prefix = doc.blocks[begin..index]
                .iter()
                .any(|b| !matches!(b, Block::FootnoteDef { .. } | Block::Html(_)));
            if current.explicit || visible_prefix {
                current.block_range = begin..index;
                sections.push(current.clone());
            }
            let previous_page = page_signature(&style);
            apply_patch(&mut style, s);
            // Native section notes inherit from document defaults, rather
            // than from an unrelated override in the preceding section.
            if has_section_notes {
                let resolved=crate::publication_note_policies::resolve_section(global,s.footnotes.as_ref().unwrap_or(&inherit_notes)).map_err(|error|error.code)?;
                if let Some(notes)=resolved.get("footnotes"){style["footnotes"]=notes.clone();}
                else if let Some(object)=style.as_object_mut(){object.remove("footnotes");}
            }
            geometry(&style)?;
            if s.header.first_text.is_some() || s.footer.first_text.is_some() {
                if !crate::publication_layout::Layout::from_style(&style).hide_first {
                    return Err(INVALID);
                }
            }
            if s.header.even_text.is_some() || s.footer.even_text.is_some() {
                if !crate::publication_layout::Layout::from_style(&style).different_odd_even {
                    return Err(INVALID);
                }
            }
            if (s.numbering.enabled.is_some() || s.numbering.show_total.is_some())
                && s.footer.link_previous == Some(true)
            {
                return Err(INVALID);
            }
            if !sections.is_empty()
                && s.break_kind == SectionBreak::Continuous
                && page_signature(&style) != previous_page
            {
                return Err(CONTINUOUS_PAGE);
            }
            let first = sections.is_empty();
            current = ResolvedSection {
                id: s.id.clone(),
                break_kind: s.break_kind,
                style: style.clone(),
                note_overrides: s.footnotes.clone(),
                block_range: index + 1..index + 1,
                page_start: s.numbering.start.or_else(|| {
                    first
                        .then_some(crate::publication_layout::Layout::from_style(global).page_start)
                }),
                header_link_previous: !first && s.header.link_previous.unwrap_or(true),
                footer_link_previous: !first
                    && s.footer.link_previous.unwrap_or(
                        s.numbering.enabled.is_none() && s.numbering.show_total.is_none(),
                    ),
                explicit: true,
            };
            begin = index + 1;
        }
    }
    current.block_range = begin..doc.blocks.len();
    sections.push(current);
    Ok(sections)
}

#[cfg(test)]
#[path = "publication_sections_tests.rs"]
mod tests;
