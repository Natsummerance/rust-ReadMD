//! Stable author-controlled labels, shared by the reader and every exporter.
//! Numbering is a projection: the Markdown source and label identity never
//! change when an object moves. Code, links, metadata and math stay literal.
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::{BTreeMap, HashMap}, ops::Range, sync::OnceLock};

pub const NUMBERING_PREFIX: &str = "<!-- readmd-numbering:";
pub const MAX_NUMBERING_BYTES: usize = 16384;
pub const MAX_NUMBER: usize = 10000;
const NUMBERING_INVALID: &str = "publication_numbering_invalid";
const NUMBERING_BUDGET: &str = "publication_numbering_budget";
const NUMBERING_HIERARCHY: &str = "publication_numbering_hierarchy";
const NUMBERING_CHAPTER_MISSING: &str = "publication_numbering_chapter_missing";

fn decimal() -> String { "decimal".into() }
fn one() -> usize { 1 }
fn chapter_one() -> u8 { 1 }
fn period() -> String { ".".into() }
fn space() -> String { " ".into() }
fn space_suffix() -> String { "space".into() }
fn document_scope() -> String { "document".into() }
fn caption_separator() -> String { " — ".into() }
fn false_value(value: &bool) -> bool { !*value }

/// Only explicitly configured levels are numbered. A missing ancestor is an
/// error, rather than an invented heading that cannot exist in a Word outline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HeadingRule {
    #[serde(default = "decimal")]
    pub format: String,
    #[serde(default = "one")]
    pub start: usize,
    /// None: preceding level (H1 never restarts). Zero: never restart.
    #[serde(default)]
    pub restart_after: Option<u8>,
    /// Native Word level text. None retains the legacy separator shorthand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// A per-level override of HeadingRules::suffix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    /// Word's isLgl: format every referenced counter as decimal.
    #[serde(default, skip_serializing_if = "false_value")]
    pub is_legal: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HeadingRules {
    pub levels: Vec<HeadingRule>,
    #[serde(default = "period")]
    pub separator: String,
    #[serde(default = "space_suffix")]
    pub suffix: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CaptionRule {
    #[serde(default = "document_scope")]
    pub scope: String,
    #[serde(default = "chapter_one")]
    pub chapter_level: u8,
    #[serde(default = "decimal")]
    pub format: String,
    #[serde(default = "one")]
    pub start: usize,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default = "space")]
    pub label_separator: String,
    #[serde(default = "period")]
    pub separator: String,
    #[serde(default = "caption_separator")]
    pub caption_separator: String,
    /// Preserve an imported native SEQ identifier without interpreting code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct NumberingConfig {
    #[serde(default)]
    pub headings: Option<HeadingRules>,
    #[serde(default)]
    pub captions: BTreeMap<String, CaptionRule>,
}

/// Effective rules map directly to Word's numFmt/start/lvlRestart/lvlText.
/// A display prefix is deliberately absent: no writer may fake a numbered
/// heading by writing its projected number into the author's heading runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedHeadingRule {
    pub format: String,
    pub start: usize,
    pub restart_after: u8,
    pub text: String,
    pub suffix: String,
    pub is_legal: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeadingNumbering {
    pub numbers: Vec<usize>,
    /// Number only, without the heading's suffix or author text.
    pub text: String,
    /// Actual separator after the number: space, tab or empty.
    pub suffix: String,
    pub rules: Vec<ResolvedHeadingRule>,
}
impl HeadingNumbering {
    pub fn full_text(&self) -> String { format!("{}{}", self.text, self.suffix) }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChapterNumbering {
    pub level: u8,
    pub id: String,
    pub number: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptionNumbering {
    pub format: String,
    pub start: usize,
    /// One-based native SEQ value, before applying a non-one start offset.
    pub ordinal: usize,
    pub chapter: Option<ChapterNumbering>,
    pub separator: String,
    pub label_separator: String,
    pub caption_separator: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NumberedHeading {
    /// Zero-based ordinal among real top-level headings, including levels
    /// outside this configuration. Nested headings never consume a number.
    pub ordinal: usize,
    pub level: u8,
    pub id: String,
    pub from: usize,
    pub to: usize,
    pub numbering: Option<HeadingNumbering>,
    /// An explicit source heading class opts this one native paragraph out.
    pub unnumbered: bool,
    #[serde(skip)]
    pub bytes: Range<usize>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NumberingPlan {
    pub config: NumberingConfig,
    pub from: usize,
    pub to: usize,
    pub headings: Vec<NumberedHeading>,
    #[serde(skip)]
    pub bytes: Range<usize>,
}

fn printable(text: &str, max: usize) -> bool {
    text.chars().count() <= max && !text.chars().any(char::is_control)
}
fn known_format(format: &str) -> bool { crate::publication_layout::NUMBER_FORMATS.contains(&format) }
pub fn valid_sequence(sequence: &str) -> bool {
    sequence.encode_utf16().count() <= 40
        && sequence.chars().next().is_some_and(char::is_alphabetic)
        && sequence.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// Word treats only percent followed by 1..9 as a level reference. A reference
/// to a deeper level is ignored; other percent characters remain literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadingTextPart<'a> { Literal(&'a str), Counter(usize) }
pub fn heading_text_parts(text: &str, level: usize) -> Vec<HeadingTextPart<'_>> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut at = 0;
    let bytes = text.as_bytes();
    while at + 1 < bytes.len() {
        if bytes[at] == b'%' && (b'1'..=b'9').contains(&bytes[at + 1]) {
            if start < at { out.push(HeadingTextPart::Literal(&text[start..at])); }
            let index = (bytes[at + 1] - b'1') as usize;
            if index < level { out.push(HeadingTextPart::Counter(index)); }
            at += 2; start = at;
        } else { at += 1; }
    }
    if start < text.len() { out.push(HeadingTextPart::Literal(&text[start..])); }
    out
}
pub fn number_text(format: &str, number: usize) -> String {
    if number == 0 {
        // A zero decimal counter is meaningful. Alphabetic and Roman formats
        // have no zero glyph; never clamp it to the first positive number.
        return if format == "decimal" { "0".into() } else { String::new() };
    }
    crate::publication_layout::note_number(format, number)
}
pub fn heading_number_text(rule: &ResolvedHeadingRule, rules: &[ResolvedHeadingRule], numbers: &[usize], level: usize) -> String {
    heading_text_parts(&rule.text, level).into_iter().map(|part| match part {
        HeadingTextPart::Literal(text) => text.to_string(),
        HeadingTextPart::Counter(index) => number_text(if rule.is_legal { "decimal" } else { &rules[index].format }, numbers[index]),
    }).collect()
}
pub fn word_number_format(format: &str) -> &'static str {
    match format {
        "lowerRoman" => "roman", "upperRoman" => "ROMAN",
        "lowerLetter" => "alphabetic", "upperLetter" => "ALPHABETIC", _ => "ARABIC",
    }
}
pub fn validate_numbering_config(config: &NumberingConfig) -> Result<(), &'static str> {
    if config.headings.is_none() && config.captions.is_empty() { return Err(NUMBERING_INVALID); }
    if let Some(headings) = &config.headings {
        if headings.levels.is_empty() || headings.levels.len() > 6
            || !printable(&headings.separator, 16) || headings.separator.contains('%')
            || !["space", "tab", "nothing"].contains(&headings.suffix.as_str()) { return Err(NUMBERING_INVALID); }
        for (index, rule) in headings.levels.iter().enumerate() {
            if !known_format(&rule.format) || rule.start > MAX_NUMBER
                || rule.restart_after.is_some_and(|n| n as usize > index)
                || rule.text.as_ref().is_some_and(|text| !printable(text, 256))
                || rule.suffix.as_ref().is_some_and(|suffix| !["space", "tab", "nothing"].contains(&suffix.as_str())) { return Err(NUMBERING_INVALID); }
        }
    }
    let mut sequences = std::collections::HashSet::new();
    for (kind, rule) in &config.captions {
        if !["equation", "figure", "table"].contains(&kind.as_str())
            || !["document", "chapter"].contains(&rule.scope.as_str())
            || !known_format(&rule.format) || rule.start > MAX_NUMBER
            || !(1..=6).contains(&rule.chapter_level)
            || !printable(&rule.separator, 16) || !printable(&rule.label_separator, 16)
            || !printable(&rule.caption_separator, 32)
            || rule.prefix.as_ref().is_some_and(|text| !printable(text, 128))
            || rule.sequence.as_ref().is_some_and(|sequence| !valid_sequence(sequence)) { return Err(NUMBERING_INVALID); }
        if rule.scope == "chapter" && config.headings.as_ref().is_none_or(|h| h.levels.len() < rule.chapter_level as usize) {
            return Err(NUMBERING_INVALID);
        }
        let sequence = rule.sequence.as_deref().unwrap_or(match kind.as_str() { "equation" => "ReadMDEquation", "figure" => "ReadMDFigure", _ => "ReadMDTable" });
        if !sequences.insert(sequence.to_lowercase()) { return Err(NUMBERING_INVALID); }
    }
    for (kind,sequence) in [("equation","ReadMDEquation"),("figure","ReadMDFigure"),("table","ReadMDTable")] {
        if !config.captions.contains_key(kind)&&!sequences.insert(sequence.to_lowercase()){return Err(NUMBERING_INVALID);}
    }
    Ok(())
}

pub fn read_numbering_comment(raw: &str) -> Result<Option<NumberingConfig>, &'static str> {
    let Some(payload) = raw.trim().strip_prefix(NUMBERING_PREFIX) else { return Ok(None); };
    if raw.len() > MAX_NUMBERING_BYTES { return Err(NUMBERING_BUDGET); }
    let payload = payload.strip_prefix("v1 ").and_then(|s| s.strip_suffix("-->")).ok_or(NUMBERING_INVALID)?.trim();
    let config: NumberingConfig = serde_json::from_str(payload).map_err(|_| NUMBERING_INVALID)?;
    validate_numbering_config(&config)?;
    Ok(Some(config))
}
pub fn numbering_comment(config: &NumberingConfig) -> Result<String, &'static str> {
    validate_numbering_config(config)?;
    let payload = serde_json::to_string(config).map_err(|_| NUMBERING_INVALID)?
        .replace('<', "\\u003c").replace('>', "\\u003e").replace('&', "\\u0026");
    let comment = format!("{NUMBERING_PREFIX}v1 {payload} -->");
    if comment.len() > MAX_NUMBERING_BYTES { return Err(NUMBERING_BUDGET); }
    Ok(comment)
}

/// Source presence is separate from the resolved typed plan.
fn source_config_value(source:&str,plan:&NumberingPlan)->Result<Value,&'static str> {
    let payload=source[plan.bytes.clone()].trim().strip_prefix(NUMBERING_PREFIX)
        .and_then(|raw|raw.strip_prefix("v1 ")).and_then(|raw|raw.strip_suffix("-->")).ok_or(NUMBERING_INVALID)?.trim();
    serde_json::from_str(payload).map_err(|_|NUMBERING_INVALID)
}
fn numbering_value_comment(value:&Value)->Result<String,&'static str> {
    let config:NumberingConfig=serde_json::from_value(value.clone()).map_err(|_|NUMBERING_INVALID)?;
    validate_numbering_config(&config)?;
    let payload=serde_json::to_string(value).map_err(|_|NUMBERING_INVALID)?
        .replace('<',"\\u003c").replace('>',"\\u003e").replace('&',"\\u0026");
    let comment=format!("{NUMBERING_PREFIX}v1 {payload} -->");
    if comment.len()>MAX_NUMBERING_BYTES{return Err(NUMBERING_BUDGET);}Ok(comment)
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level { HeadingLevel::H1 => 1, HeadingLevel::H2 => 2, HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4, HeadingLevel::H5 => 5, HeadingLevel::H6 => 6 }
}

/// Parse declarations from actual comment nodes, and identity from original
/// heading events. Code, metadata and nested examples cannot declare counters.
pub fn analyze_numbering(source: &str) -> Result<Option<NumberingPlan>, &'static str> {
    if !source.contains(NUMBERING_PREFIX) { return Ok(None); }
    if source.len() > crate::document_transaction::MAX_DOCUMENT_BYTES { return Err(NUMBERING_BUDGET); }
    let bom = usize::from(source.starts_with('\u{feff}')) * 3;
    let mut depth = 0usize;
    let mut body_started = false;
    let mut html: Option<(usize, usize, String)> = None;
    let mut declaration: Option<(NumberingConfig, Range<usize>)> = None;
    let mut heading: Option<(usize, u8, Option<String>, String, bool, bool)> = None;
    let mut slugs = crate::md_ast::HeadingIds::default();
    let mut headings = Vec::new();
    for (event, range) in Parser::new_ext(&source[bom..], options()).into_offset_iter() {
        let range = range.start + bom..range.end + bom;
        match event {
            Event::Start(Tag::HtmlBlock) => { html = Some((range.start, depth, String::new())); depth += 1; },
            Event::End(TagEnd::HtmlBlock) => {
                depth = depth.saturating_sub(1);
                if let Some((start, nesting, raw)) = html.take() {
                    if let Some(config) = read_numbering_comment(&raw)? {
                        if nesting != 0 || body_started || declaration.is_some() { return Err(NUMBERING_INVALID); }
                        declaration = Some((config, start..range.end));
                    } else if nesting == 0 && !raw.trim_start().starts_with("<!--") { body_started = true; }
                }
            },
            Event::Html(text) => {
                if let Some((_, _, raw)) = &mut html { raw.push_str(&text); }
                else if let Some(config) = read_numbering_comment(&text)? {
                    if depth != 0 || body_started || declaration.is_some() { return Err(NUMBERING_INVALID); }
                    declaration = Some((config, range));
                } else if depth == 0 && !text.trim_start().starts_with("<!--") { body_started = true; }
            },
            Event::InlineHtml(text) if text.trim().starts_with(NUMBERING_PREFIX) => return Err(NUMBERING_INVALID),
            Event::Start(Tag::Heading { level, id, classes, .. }) => {
                let top = depth == 0;
                if top { body_started = true; }
                heading = Some((range.start, heading_level(level), id.map(|id| id.to_string()), String::new(), top, classes.iter().any(|class|class.as_ref()=="unnumbered")));
                depth += 1;
            },
            Event::End(TagEnd::Heading(_)) => {
                depth = depth.saturating_sub(1);
                if let Some((start, level, explicit, title, top, unnumbered)) = heading.take() {
                    let id = slugs.allocate(&title, explicit.as_deref());
                    if top && declaration.is_some() {
                        if headings.len() >= MAX_NUMBER { return Err(NUMBERING_BUDGET); }
                        if !printable(&id, 512) { return Err(NUMBERING_INVALID); }
                        headings.push(NumberedHeading { ordinal: headings.len(), level, id, from: 0, to: 0,
                            numbering: None, unnumbered, bytes: start..range.end });
                    }
                }
            },
            Event::Start(tag) => {
                if depth == 0 && !matches!(tag, Tag::MetadataBlock(_)) { body_started = true; }
                depth += 1;
            },
            Event::End(_) => depth = depth.saturating_sub(1),
            Event::Text(text) | Event::Code(text) | Event::InlineMath(text) | Event::DisplayMath(text) => {
                if let Some((_, _, _, title, _, _)) = &mut heading { title.push_str(&text); }
                if depth == 0 { body_started = true; }
            },
            Event::SoftBreak | Event::HardBreak => if let Some((_, _, _, title, _, _)) = &mut heading { title.push(' '); },
            Event::FootnoteReference(label) => if let Some((_, _, _, title, _, _)) = &mut heading { title.push('['); title.push_str(&label); title.push(']'); },
            Event::Rule if depth == 0 => body_started = true,
            _ => (),
        }
    }
    let Some((config, bytes)) = declaration else { return Ok(None); };
    let positions: std::collections::HashSet<_> = headings.iter().flat_map(|h| [h.bytes.start, h.bytes.end])
        .chain([bytes.start, bytes.end]).collect();
    let mut offsets = HashMap::new();
    let mut units = 0;
    for (at, ch) in source.char_indices() {
        if positions.contains(&at) { offsets.insert(at, units); }
        units += ch.len_utf16();
    }
    offsets.insert(source.len(), units);
    if let Some(rules) = &config.headings {
        let resolved: Vec<_> = rules.levels.iter().enumerate().map(|(index, rule)| ResolvedHeadingRule {
            format: rule.format.clone(), start: rule.start, restart_after: rule.restart_after.unwrap_or(index as u8),
            text: rule.text.clone().unwrap_or_else(|| (1..=index + 1).map(|n| format!("%{n}")).collect::<Vec<_>>().join(&rules.separator)),
            suffix: rule.suffix.clone().unwrap_or_else(|| rules.suffix.clone()), is_legal: rule.is_legal,
        }).collect();
        let mut counters = [None::<usize>; 6];
        for heading in &mut headings {
            if heading.unnumbered { continue; }
            let index = heading.level as usize - 1;
            if index >= resolved.len() { continue; }
            if counters[..index].iter().any(Option::is_none) { return Err(NUMBERING_HIERARCHY); }
            let number = if let Some(previous) = counters[index] { previous.checked_add(1).ok_or(NUMBERING_BUDGET)? } else { resolved[index].start };
            if number > MAX_NUMBER { return Err(NUMBERING_BUDGET); }
            counters[index] = Some(number);
            for child in index + 1..resolved.len() {
                if resolved[child].restart_after >= heading.level { counters[child] = None; }
            }
            let numbers: Vec<_> = counters[..=index].iter().map(|number| number.unwrap()).collect();
            let text = heading_number_text(&resolved[index], &resolved, &numbers, index + 1);
            if text.chars().count() > 4096 { return Err(NUMBERING_BUDGET); }
            heading.numbering = Some(HeadingNumbering { numbers, text,
                suffix: match resolved[index].suffix.as_str() { "tab" => "\t", "nothing" => "", _ => " " }.into(), rules: resolved.clone() });
        }
    }
    for heading in &mut headings {
        heading.from = *offsets.get(&heading.bytes.start).ok_or(NUMBERING_INVALID)?;
        heading.to = *offsets.get(&heading.bytes.end).ok_or(NUMBERING_INVALID)?;
    }
    Ok(Some(NumberingPlan { config, from: *offsets.get(&bytes.start).ok_or(NUMBERING_INVALID)?,
        to: *offsets.get(&bytes.end).ok_or(NUMBERING_INVALID)?, headings, bytes }))
}

pub fn validate_source(source: &str) -> Result<(), &'static str> {
    if analyze_numbering(source)?.is_some() {
        let projected = project(source);
        if projected.diagnostics.iter().any(|d| d["code"] == NUMBERING_CHAPTER_MISSING) { return Err(NUMBERING_CHAPTER_MISSING); }
        if projected.diagnostics.iter().any(|d| d["code"] == NUMBERING_BUDGET) { return Err(NUMBERING_BUDGET); }
    }
    Ok(())
}

/// The source edit is separate from the transient projection. Callers apply it
/// with the common revision-checked transaction and keep its exact inverse.
pub fn numbering_edit(source: &str, config: Option<&NumberingConfig>) -> Result<Option<crate::document_transaction::TextEdit>, &'static str> {
    let existing = analyze_numbering(source)?;
    if let Some(config) = config { validate_numbering_config(config)?; }
    if existing.as_ref().map(|p| &p.config) == config { return Ok(None); }
    let comment = config.map(numbering_comment).transpose()?;
    numbering_text_edit(source,existing,comment)
}
fn numbering_value_edit(source:&str,config:Option<&Value>)->Result<Option<crate::document_transaction::TextEdit>,&'static str> {
    let existing=analyze_numbering(source)?;
    let current=existing.as_ref().map(|plan|source_config_value(source,plan)).transpose()?;
    if current.as_ref()==config{return Ok(None);}
    let comment=config.map(numbering_value_comment).transpose()?;
    numbering_text_edit(source,existing,comment)
}
fn numbering_text_edit(source:&str,existing:Option<NumberingPlan>,comment:Option<String>)->Result<Option<crate::document_transaction::TextEdit>,&'static str> {
    let eol = if source.contains("\r\n") { "\r\n" } else { "\n" };
    let edit = if let Some(existing) = existing {
        let raw=&source[existing.bytes.clone()];
        let trailing=raw[raw.trim_end_matches(['\r','\n']).len()..].to_string();
        crate::document_transaction::TextEdit { from: existing.from, to: existing.to,
            insert: comment.map(|c| format!("{c}{trailing}")).unwrap_or(trailing) }
    } else {
        let without = crate::md_ast::source_without_front_matter(source);
        let byte = source.len() - without.len();
        let at = source[..byte].encode_utf16().count();
        let leading = if byte > 0 && !source[..byte].ends_with('\n') && !source[..byte].ends_with('\u{feff}') { eol } else { "" };
        crate::document_transaction::TextEdit { from: at, to: at, insert: format!("{leading}{}{eol}{eol}", comment.ok_or(NUMBERING_INVALID)?) }
    };
    let tx = crate::document_transaction::ContentTransaction { base_revision: crate::document_transaction::revision(source), origin: "editor".into(), edits: vec![edit.clone()] };
    let changed = crate::document_transaction::apply(source, &tx)?;
    validate_source(&changed.content)?;
    Ok(Some(edit))
}

pub fn strip_numbering_for_view(source: &str) -> Result<String, &'static str> {
    let Some(plan) = analyze_numbering(source)? else { return Ok(source.into()); };
    let mut out = source[..plan.bytes.start].to_string();
    out.extend(source[plan.bytes.clone()].chars().filter(|c| matches!(c, '\r' | '\n')));
    out.push_str(&source[plan.bytes.end..]);
    Ok(out)
}

/// Physical Word tab stops have no identical counterpart in flowing formats.
/// The writers retain a readable gap and report this specific loss once.
pub fn format_warnings(source: &str, format: &str) -> Vec<String> {
    if format != "docx" && analyze_numbering(source).ok().flatten()
        .and_then(|p|p.config.headings).is_some_and(|h|h.levels.iter().any(|rule|rule.suffix.as_deref().unwrap_or(&h.suffix)=="tab")) {
        vec!["publication_numbering_tab_unsupported".into()]
    } else { Vec::new() }
}

/// One in-memory service for the desktop and MCP. Configuration is content,
/// not an export preference; callers commit this transaction through the
/// existing editor/save path and keep the inverse for ordinary undo.
pub fn numbering_request_schema() -> Value {
    json!({"type":"object","additionalProperties":false,
        "properties":{"action":{"type":"string","enum":["get","apply"]},
            "content":{"type":"string"},"base_revision":{"type":"string"},
            "config":{"anyOf":[{"type":"object"},{"type":"null"}]}},
        "required":["content"]})
}
pub fn numbering_request(value: &Value) -> Result<Value, &'static str> {
    use crate::document_transaction::{apply, revision, ContentTransaction};
    let object=value.as_object().ok_or("invalid_json")?;
    if object.keys().any(|key|!matches!(key.as_str(),"action"|"content"|"base_revision"|"config")) { return Err("invalid_json"); }
    let source = value["content"].as_str().ok_or("invalid_json")?;
    if source.len() > crate::document_transaction::MAX_DOCUMENT_BYTES { return Err(NUMBERING_BUDGET); }
    let action=match value.get("action") {None=>"get",Some(action)=>action.as_str().ok_or("invalid_json")?};
    match action {
        "get" => {
            validate_source(source)?;
            let plan = analyze_numbering(source)?;
            let config=plan.as_ref().map(|plan|source_config_value(source,plan)).transpose()?;
            Ok(json!({"ok":true,"revision":revision(source),
                "config":config,"plan":plan}))
        },
        "apply" => {
            let expected = value["base_revision"].as_str().ok_or("invalid_json")?;
            if expected != revision(source) { return Err("transaction_conflict"); }
            let raw = value.get("config").ok_or("invalid_json")?;
            let config=if raw.is_null(){None}else{Some(raw)};
            let transaction = ContentTransaction {base_revision: expected.into(), origin:"editor".into(),
                edits: numbering_value_edit(source, config)?.into_iter().collect()};
            let result = apply(source, &transaction)?;
            validate_source(&result.content)?;
            let plan = analyze_numbering(&result.content)?;
            let config=plan.as_ref().map(|plan|source_config_value(&result.content,plan)).transpose()?;
            Ok(json!({"ok":true,"transaction":transaction,"result":result,
                "config":config,"plan":plan}))
        },
        _ => Err("invalid_json"),
    }
}

/// Run after original heading identities and source-bound local styles have
/// been attached. Nested content never consumes a top-level outline number.
pub fn attach_numbering(blocks: &mut [crate::md_ast::Block], source: &str) -> Result<(), &'static str> {
    let Some(plan) = analyze_numbering(source)? else { return Ok(()); };
    use crate::md_ast::Block;
    fn heading(block: &mut Block) -> Option<(&mut String, &mut Option<HeadingNumbering>)> {
        match block {
            Block::Heading { id, numbering, .. } => Some((id, numbering)),
            Block::Styled { block, .. } => heading(block),
            _ => None,
        }
    }
    let mut ordinal = 0usize;
    for block in blocks {
        if let Some((id, numbering)) = heading(block) {
            let original = plan.headings.get(ordinal).ok_or(NUMBERING_HIERARCHY)?;
            *id = original.id.clone();
            *numbering = original.numbering.clone();
            ordinal += 1;
        }
    }
    if ordinal != plan.headings.len() { return Err(NUMBERING_HIERARCHY); }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Label {
    pub id: String,
    pub kind: String,
    pub number: usize,
    pub prefix: String,
    /// Omitted on legacy labels, including their serialized caption marker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub numbering: Option<CaptionNumbering>,
}
impl Label {
    pub fn number_text(&self) -> String {
        let Some(numbering) = &self.numbering else { return self.number.to_string(); };
        let number = number_text(&numbering.format, self.number);
        numbering.chapter.as_ref().map(|chapter| format!("{}{}{number}", chapter.number, numbering.separator)).unwrap_or(number)
    }
    pub fn text(&self) -> String {
        let separator = self.numbering.as_ref().map(|n| n.label_separator.as_str()).unwrap_or(" ");
        format!("{}{separator}{}", self.prefix, self.number_text())
    }
    pub fn caption_separator(&self) -> &str {
        self.numbering.as_ref().map(|n| n.caption_separator.as_str()).unwrap_or(" — ")
    }
    pub fn sequence(&self) -> &str {
        self.numbering.as_ref().and_then(|numbering|numbering.sequence.as_deref()).unwrap_or_else(|| match self.kind.as_str() { "equation" => "ReadMDEquation", "figure" => "ReadMDFigure", _ => "ReadMDTable" })
    }
}
pub struct Projection {
    pub content: String,
    pub labels: Vec<Value>,
    pub diagnostics: Vec<Value>,
    pub numbering: Option<NumberingPlan>,
}
fn pattern() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r":(label|ref)\[([\p{L}\p{N}_:.-]{1,128})\]").unwrap())
}
fn kind(id: &str) -> Option<&'static str> {
    match id.split_once(':')?.0 { "eq" => Some("equation"), "fig" => Some("figure"), "tbl" => Some("table"), _ => None }
}
fn language(source: &str) -> &str {
    let first = source.trim_start_matches('\u{feff}');
    if !first.starts_with("---\n") && !first.starts_with("---\r\n") { return "en"; }
    for line in first.lines().skip(1).take(128) {
        if line == "---" { break; }
        if let Some(("lang", value)) = line.split_once(':') { return value.trim().trim_matches(['\'', '"']); }
    }
    "en"
}
fn prefix(locale: &str, kind: &str) -> String {
    static LABELS: OnceLock<Value> = OnceLock::new();
    let labels = LABELS.get_or_init(|| serde_json::from_str(include_str!("../../../assets/data/publication-labels.json")).expect("publication labels"));
    let locale = locale.replace('_', "-");
    let texts = labels.get(&locale).or_else(|| labels.get(locale.split('-').next().unwrap_or("en"))).unwrap_or(&labels["en"]);
    texts[kind].as_str().unwrap_or(kind).into()
}
fn escaped(source: &str, at: usize) -> bool {
    source[..at].bytes().rev().take_while(|b| *b == b'\\').count() % 2 == 1
}
fn options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_MATH | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_HEADING_ATTRIBUTES | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS
}

/// This marker exists only in the transient projection, never in saved source.
pub fn marker(label: &Label) -> String {
    let payload = serde_json::to_string(label).expect("label").replace('<', "\\u003c").replace('>', "\\u003e").replace('&', "\\u0026");
    format!("<!--readmd:caption:v1 {payload}-->")
}
pub fn read_marker(text: &str) -> Option<Label> {
    let text = text.trim().strip_prefix("<!--readmd:caption:v1 ")?.strip_suffix("-->")?;
    if text.len() > MAX_NUMBERING_BYTES + 4096 { return None; }
    let label: Label = serde_json::from_str(text).ok()?;
    if label.number > MAX_NUMBER || label.number == 0 && label.numbering.is_none() || kind(&label.id) != Some(label.kind.as_str())
        || !pattern().is_match(&format!(":label[{}]", label.id))
        || label.id.chars().any(|c| !(c.is_alphanumeric() || "_:.-".contains(c)))
        || label.prefix.len() > 512 || !printable(&label.prefix, 128) { return None; }
    if let Some(numbering) = &label.numbering {
        if !known_format(&numbering.format) || numbering.start > MAX_NUMBER
            || !(1..=MAX_NUMBER).contains(&numbering.ordinal)
            || numbering.start.checked_add(numbering.ordinal - 1) != Some(label.number)
            || !printable(&numbering.separator, 16) || !printable(&numbering.label_separator, 16)
            || !printable(&numbering.caption_separator, 32)
            || numbering.sequence.as_ref().is_some_and(|sequence|!valid_sequence(sequence))
            || numbering.chapter.as_ref().is_some_and(|chapter| !(1..=6).contains(&chapter.level)
                || chapter.id.is_empty() || !printable(&chapter.id, 512) || !printable(&chapter.number, 4096)) { return None; }
    }
    Some(label)
}

fn chapter_at(plan: &NumberingPlan, level: u8, at: usize) -> Option<ChapterNumbering> {
    let mut current = None;
    for heading in &plan.headings {
        if heading.bytes.start >= at { break; }
        if heading.level < level { current = None; }
        else if heading.level == level {
            current = heading.numbering.as_ref().map(|number| ChapterNumbering { level, id: heading.id.clone(), number: number.text.clone() });
        }
    }
    current
}

fn markdown_literal(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c == '&' { out.push_str("&amp;"); }
        else if matches!(c, '[' | ']') { out.push_str(if c == '[' { "&#91;" } else { "&#93;" }); }
        else { if "\\`*_{}<>!#|$".contains(c) { out.push('\\'); } out.push(c); }
    }
    out
}

fn is_math_capsule_comment(text: &str) -> bool {
    let Some(payload) = text.trim().strip_prefix("<!--readmd:math:v1:").and_then(|text| text.strip_suffix("-->")) else { return false; };
    !payload.trim().is_empty() && !payload.contains("<!--") && !payload.contains("-->")
}

fn display_math_paragraph(source: &str, whole: Range<usize>, math: &Range<usize>, metadata: &[Range<usize>]) -> bool {
    // Imported Office math retains its native properties in a trailing HTML
    // comment. It is not visible prose and must not detach a real display
    // equation from its caption. The parser, not a text-prefix deletion, owns
    // these comment ranges; every remaining byte must be only whitespace.
    if metadata.len() > 1 { return false; }
    let mut parts = vec![math.clone()];
    parts.extend(metadata.iter().cloned());
    parts.sort_by_key(|part| part.start);
    let mut at = whole.start;
    for part in parts {
        if part.start < at || part.end > whole.end || !source[at..part.start].trim().is_empty() { return false; }
        at = part.end;
    }
    source[at..whole.end].trim().is_empty()
}

pub fn project(source: &str) -> Projection {
    let mut result = Projection { content: source.into(), labels: vec![], diagnostics: vec![], numbering: None };
    match analyze_numbering(source) {
        Ok(plan) => result.numbering = plan,
        Err(code) => {
            result.diagnostics.push(json!({"code":code,"key":"numbering","from":0,"to":0}));
            return result;
        },
    }
    if !source.contains(":label[") && !source.contains(":ref[") { return result; }
    let mut depth = 0usize;
    let mut block: Option<(usize, String)> = None;
    let mut block_math: Vec<Range<usize>> = Vec::new();
    let mut block_math_metadata: Vec<Range<usize>> = Vec::new();
    let mut blocks: Vec<(Range<usize>, String)> = vec![];
    let mut protected = 0usize;
    let mut runs: Vec<Range<usize>> = vec![];
    let mut heading_ids = std::collections::HashSet::new();
    for (event, range) in Parser::new_ext(source, options()).into_offset_iter() {
        match &event {
            Event::Start(tag) => {
                if let Tag::Heading {id:Some(id),..} = tag { heading_ids.insert(id.to_string()); }
                if depth == 0 {
                    block_math.clear();
                    block_math_metadata.clear();
                    let name = match tag {
                        Tag::Paragraph => "paragraph",
                        Tag::Table(_) => "table",
                        Tag::CodeBlock(CodeBlockKind::Fenced(info)) if info.trim() == "readmd-math v1" => "equation",
                        _ => "other",
                    };
                    let comment=&source[range.clone()];
                    let bridge=matches!(tag,Tag::HtmlBlock)&&(is_math_capsule_comment(comment)||crate::publication_pagination::is_marker(comment)||crate::publication_blocks::read_style(comment).is_ok_and(|s|s.is_some()));
                    block = if bridge{None}else{Some((range.start, name.into()))};
                }
                if matches!(tag, Tag::CodeBlock(_) | Tag::Link {..} | Tag::Image {..} | Tag::MetadataBlock(_)) { protected += 1; }
                if matches!(tag, Tag::Image {..}) {
                    if let Some((_, name)) = &mut block { if name == "paragraph" { *name = "figure".into(); } }
                }
                depth += 1;
            }
            Event::End(tag) => {
                if matches!(tag, TagEnd::CodeBlock | TagEnd::Link | TagEnd::Image | TagEnd::MetadataBlock(_)) { protected = protected.saturating_sub(1); }
                depth = depth.saturating_sub(1);
                if depth == 0 { if let Some((start, mut name)) = block.take() {
                    // Inline math and prose containing a display fragment do
                    // not turn a paragraph into a display-equation object.
                    if name == "paragraph" && block_math.len() == 1
                        && display_math_paragraph(source,start..range.end,&block_math[0],&block_math_metadata) {
                        name = "equation".into();
                    }
                    blocks.push((start..range.end, name));
                } }
            }
            Event::DisplayMath(_) if depth == 0 => blocks.push((range.clone(), "equation".into())),
            Event::DisplayMath(_) => block_math.push(range.clone()),
            Event::InlineHtml(html) | Event::Html(html) if depth > 0
                && is_math_capsule_comment(html) => block_math_metadata.push(range.clone()),
            Event::Text(_) if protected == 0 => {
                if let Some(last) = runs.last_mut().filter(|last| last.end == range.start) { last.end = range.end; }
                else { runs.push(range.clone()); }
            }
            Event::Html(s) if depth == 0 && !is_math_capsule_comment(s) && !crate::publication_pagination::is_marker(&s) && !crate::publication_blocks::read_style(&s).is_ok_and(|s|s.is_some()) => blocks.push((range.clone(), "other".into())),
            _ => (),
        }
    }
    let mut declarations = vec![];
    let mut counts: HashMap<String, usize> = HashMap::new();
    for (index, (range, block_kind)) in blocks.iter().enumerate() {
        if block_kind != "paragraph" { continue; }
        let fragment = &source[range.clone()];
        let Some(found) = pattern().captures(fragment.trim()) else { continue; };
        if &found[1] != "label" || found.get(0).unwrap().start() != 0 || escaped(source, range.start) { continue; }
        let id = found[2].to_string();
        let caption = fragment.trim()[found.get(0).unwrap().end()..].trim();
        let valid = kind(&id).is_some_and(|k| index > 0 && blocks[index - 1].1 == k) && !caption.contains(['\r','\n']) && !heading_ids.contains(&id);
        *counts.entry(id.clone()).or_default() += 1;
        declarations.push((range.clone(), id, caption.to_string(), valid));
    }
    let mut changes: BTreeMap<usize, (usize, String)> = BTreeMap::new();
    let mut labels: HashMap<String, Label> = HashMap::new();
    let mut numbers: HashMap<String, usize> = HashMap::new();
    for (range, id, caption, valid) in &declarations {
        let code = if counts[id] != 1 { Some("academic_duplicate_label") } else if !valid { Some("academic_invalid_label") } else { None };
        if let Some(code) = code {
            result.diagnostics.push(json!({"code":code,"key":id,"from":source[..range.start].encode_utf16().count(),"to":source[..range.end].encode_utf16().count()}));
            continue;
        }
        let kind = kind(id).unwrap();
        let rule = result.numbering.as_ref().and_then(|plan| plan.config.captions.get(kind));
        let chapter = match (rule, result.numbering.as_ref()) {
            (Some(rule), Some(plan)) if rule.scope == "chapter" => {
                let chapter = chapter_at(plan, rule.chapter_level, range.start);
                if chapter.is_none() {
                    result.diagnostics.push(json!({"code":NUMBERING_CHAPTER_MISSING,"key":id,"from":source[..range.start].encode_utf16().count(),"to":source[..range.end].encode_utf16().count()}));
                    continue;
                }
                chapter
            },
            _ => None,
        };
        let counter = format!("{kind}\0{}", chapter.as_ref().map(|c| c.id.as_str()).unwrap_or(""));
        let ordinal = numbers.entry(counter).or_default(); *ordinal += 1;
        let start = rule.map(|rule| rule.start).unwrap_or(1);
        let Some(number) = start.checked_add(*ordinal - 1).filter(|number| *number <= MAX_NUMBER) else {
            if result.numbering.is_some() { result.diagnostics.push(json!({"code":NUMBERING_BUDGET,"key":id,"from":source[..range.start].encode_utf16().count(),"to":source[..range.end].encode_utf16().count()})); }
            continue;
        };
        let numbering = rule.map(|rule| CaptionNumbering { format: rule.format.clone(), start, ordinal: *ordinal, chapter,
            separator: rule.separator.clone(), label_separator: rule.label_separator.clone(), caption_separator: rule.caption_separator.clone(), sequence: rule.sequence.clone() });
        let label = Label { id: id.clone(), kind: kind.into(), number,
            prefix: rule.and_then(|rule| rule.prefix.clone()).unwrap_or_else(|| prefix(language(source), kind)), numbering };
        let text = label.text();
        let projected_text = if label.numbering.is_some() { markdown_literal(&text) } else { text.clone() };
        let suffix = if caption.is_empty() { String::new() } else {
            let separator = if label.numbering.is_some() { markdown_literal(label.caption_separator()) } else { label.caption_separator().into() };
            format!("{separator}{caption}")
        };
        let eol = if source.contains("\r\n") { "\r\n" } else { "\n" };
        let value = format!("{}{eol}{eol}<span id=\"{id}\" class=\"publication-caption\"></span>{projected_text}{suffix}{eol}", marker(&label));
        changes.insert(range.start, (range.end, value));
        let mut data = json!({"id":id,"kind":kind,"number":number,"text":text,"caption":caption,"from":source[..range.start].encode_utf16().count(),"to":source[..range.end].encode_utf16().count()});
        if let Some(numbering) = &label.numbering { data["numbering"] = json!(numbering); }
        result.labels.push(data);
        labels.insert(id.clone(), label);
    }
    for run in runs {
        for found in pattern().captures_iter(&source[run.clone()]) {
            if &found[1] != "ref" { continue; }
            let all = found.get(0).unwrap();
            let start = run.start + all.start(); let end = run.start + all.end();
            if escaped(source, start) || declarations.iter().any(|(r,_,_,_)| r.contains(&start)) { continue; }
            let id = &found[2];
            if let Some(label) = labels.get(id) {
                let text = if label.numbering.is_some() { markdown_literal(&label.text()) } else { label.text() };
                changes.insert(start, (end, format!("[{text}](#{id})")));
            } else {
                result.diagnostics.push(json!({"code":"academic_missing_reference","key":id,"from":source[..start].encode_utf16().count(),"to":source[..end].encode_utf16().count()}));
            }
            if changes.len() >= 20000 { break; }
        }
    }
    for (start, (end, text)) in changes.into_iter().rev() { result.content.replace_range(start..end, &text); }
    result
}

// These wrappers can occur only after the trusted author-size projector has
// split a caption label by script. Recognizing their bounded syntax preserves
// typed caption structure; CSS authority still requires the separate nonce IR.
fn projected_author_run_open(raw:&str)->bool{
    let Some(rest)=raw.strip_prefix("<span data-readmd-author-run=\"")else{return false};
    let Some((key,tail))=rest.split_once('"')else{return false};
    let Some((nonce,index))=key.split_once('-')else{return false};
    if nonce.len()!=32||!nonce.bytes().all(|byte|byte.is_ascii_hexdigit())||index.is_empty()||index.len()>8||!index.bytes().all(|byte|byte.is_ascii_digit()){return false;}
    if tail==">"{return true;}
    tail.strip_prefix(" style=\"font-size:").and_then(|value|value.strip_suffix("pt\">"))
        .and_then(|value|value.parse::<f64>().ok()).is_some_and(|value|value.is_finite()&&value>0.&&value<=crate::publication_text_runs::MAX_RENDER_PT)
}
fn caption_title(label:&Label,inlines:&[crate::md_ast::Inline])->Option<Vec<crate::md_ast::Inline>>{
    use crate::md_ast::Inline;
    let anchor=format!("<span id=\"{}\" class=\"publication-caption\">",label.id);
    let mut at=match inlines.first(){
        Some(Inline::Html(value))if value==&format!("{anchor}</span>")=>1,
        Some(Inline::Html(value))if value==&anchor&&matches!(inlines.get(1),Some(Inline::Html(close))if close=="</span>")=>2,
        _=>return None,
    };
    let prefix=label.text();let mut consumed=0;let mut open=Vec::<String>::new();
    while consumed<prefix.len(){
        match inlines.get(at)?{
            Inline::Html(value)if projected_author_run_open(value)=>{open.push(value.clone());at+=1;},
            Inline::Html(value)if value=="</span>"&&!open.is_empty()=>{open.pop();at+=1;},
            Inline::Text(value)=>{
                let remaining=&prefix[consumed..];
                if remaining.starts_with(value.as_str()){consumed+=value.len();at+=1;}
                else if value.starts_with(remaining){
                    let mut title=open.into_iter().map(Inline::Html).collect::<Vec<_>>();
                    title.push(Inline::Text(value[remaining.len()..].into()));title.extend_from_slice(&inlines[at+1..]);return Some(title);
                }else{return None;}
            },_=>return None,
        }
    }
    // Restore only wrappers still open where the label ended. The remaining
    // title retains their original closers, including an empty run when a
    // script segment ended at the exact prefix boundary.
    let mut title=open.into_iter().map(Inline::Html).collect::<Vec<_>>();title.extend_from_slice(&inlines[at..]);Some(title)
}

/// Typed captions keep field generation out of the frontend projection.
pub fn attach(blocks: &mut Vec<crate::md_ast::Block>) {
    use crate::md_ast::Block;
    for block in blocks.iter_mut() {
        match block {
            Block::Quote(inner) | Block::FootnoteDef {blocks:inner,..} => attach(inner),
            Block::List {items,..} => for item in items { attach(&mut item.blocks); },
            _ => (),
        }
    }
    let mut index = 0;
    while index + 1 < blocks.len() {
        let Some(label) = (match &blocks[index] { Block::Html(s) => read_marker(s), _ => None }) else { index += 1; continue; };
        let Block::Paragraph(inlines) = &blocks[index + 1] else { index += 1; continue; };
        let Some(inlines)=caption_title(&label,inlines)else{index+=1;continue;};
        blocks.splice(index..index + 2, [Block::Caption { label, inlines }]);
        index += 1;
    }
}

#[cfg(test)]
#[path = "cross_references_numbering_tests.rs"]
mod numbering_tests;
#[cfg(test)]
#[path = "publication_numbering_api_tests.rs"]
mod numbering_api_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_labels_number_and_reorder_without_rewriting_source_or_examples() {
        let source = "---\nlang: zh-CN\n---\n参见 :ref[eq:second]。\n\n$$a$$\n\n:label[eq:first] 原式\n\n```readmd-math v1\nx^2\n```\n\n:label[eq:second] **新式**\n\n` :ref[eq:second] `\n\n```md\n:label[eq:hidden]\n:ref[eq:first]\n```\n\n\\:ref[eq:first]\n";
        let projected = project(source);
        assert_eq!(projected.labels.len(), 2); assert!(projected.diagnostics.is_empty());
        assert!(projected.content.contains("[公式 2](#eq:second)"));
        assert!(projected.content.contains("` :ref[eq:second] `") && projected.content.contains("\\:ref[eq:first]"));
        assert!(projected.content.contains("```md\n:label[eq:hidden]\n:ref[eq:first]\n```"));
        let reordered = source.replace("$$a$$\n\n:label[eq:first] 原式\n\n", "");
        assert!(project(&reordered).content.contains("[公式 1](#eq:second)"));
        let doc = crate::md_ast::parse(source);
        assert_eq!(doc.blocks.iter().filter(|b| matches!(b, crate::md_ast::Block::Caption {..})).count(), 2);
    }
    #[test]
    fn invalid_or_duplicate_labels_never_make_ambiguous_destinations() {
        let source = "$$x$$\n\n:label[eq:duplicate]\n\n$$y$$\n\n:label[eq:duplicate]\n\n:label[fig:orphan]\n\n:ref[eq:duplicate] :ref[fig:missing]\n";
        let p = project(source); assert!(p.labels.is_empty()); assert_eq!(p.diagnostics.len(), 5);
        assert_eq!(p.content, source); assert!(!p.content.contains("<span"));
        let collision = "# Heading {#eq:energy}\n\n$$x$$\n\n:label[eq:energy]\n\n:ref[eq:energy]";
        let p = project(collision); assert!(p.labels.is_empty()); assert_eq!(p.content, collision);
        assert_eq!(p.diagnostics[0]["code"], "academic_invalid_label");
    }
    #[test]
    fn native_writers_emit_updateable_word_fields_pdf_destinations_and_epub_chapter_links() {
        let source = "# First\n\nSee :ref[eq:energy] and :ref[tbl:evidence].\n\n# Second\n\n$$E=mc^2$$\n\n:label[eq:energy] Energy\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\n:label[tbl:evidence] **Evidence**\n";
        let doc = crate::md_ast::parse(source);
        let style = crate::export_styles::sanitize(&json!({"typography":{"firstLineIndent":8}}));
        let package = crate::docx_writer::build(&doc,&style,"test",&|_|None);
        let xml = |name: &str| String::from_utf8(package.parts.iter().find(|(n,_)|n==name).unwrap().1.clone()).unwrap();
        let document = xml("word/document.xml");
        assert!(document.contains(" SEQ ReadMDEquation ") && document.contains(" SEQ ReadMDTable "));
        assert!(document.contains(&format!(" REF {} ", crate::docx_writer::bookmark_name("eq:energy"))));
        assert!(document.contains("w:dirty=\"true\"") && document.contains("w:val=\"BodyText\""));
        assert!(xml("word/styles.xml").contains("w:firstLine=\"454\""));
        assert!(xml("word/settings.xml").contains("<w:updateFields w:val=\"true\"/>"));
        let dir=tempfile::tempdir().unwrap(); let mut warnings=vec![];
        let pdf=crate::pdf_render::render_bytes(&crate::md_ast::to_render_json(&doc.blocks),&style,dir.path().to_str().unwrap(),&|_|None,&mut warnings).unwrap();
        let parsed=lopdf::Document::load_mem(&pdf).unwrap(); let names=parsed.catalog().unwrap().get(b"Names").unwrap().as_dict().unwrap().get(b"Dests").unwrap().as_dict().unwrap().get(b"Names").unwrap().as_array().unwrap();
        assert!(names.iter().any(|v|v.as_str().ok()==Some(b"eq:energy")));
        assert!(parsed.objects.values().filter_map(|v|v.as_dict().ok()).any(|v|v.get(b"Subtype").ok().and_then(|v|v.as_name().ok())==Some(b"Link")&&v.get(b"Dest").is_ok()));
        let chapters=crate::epub_writer::render_chapters(&doc,"h1",&mut |_|None);
        let second_id=regex::Regex::new(r#"id="([^"]+)"[^>]*>Equation 1"#).unwrap().captures(&chapters[1].body).unwrap()[1].to_string();
        assert!(chapters[0].body.contains(&format!("chapter_2.xhtml#{second_id}")));
        assert!(chapters[1].body.contains("<strong>Evidence</strong>"));
        let tex=crate::latex_writer::render_body(&doc,&|_|None);
        assert!(tex.contains(&format!("\\label{{{}}}",crate::latex_writer::label_key("eq:energy"))));
        assert!(tex.contains(&format!("\\hyperref[{}]",crate::latex_writer::label_key("eq:energy"))));
    }
}
