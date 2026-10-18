//! Stable author-controlled labels, shared by the reader and every exporter.
//! Numbering is a projection: the Markdown source and label identity never
//! change when an object moves. Code, links, metadata and math stay literal.
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::{BTreeMap, HashMap}, ops::Range, sync::OnceLock};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Label {
    pub id: String,
    pub kind: String,
    pub number: usize,
    pub prefix: String,
}
impl Label {
    pub fn text(&self) -> String { format!("{} {}", self.prefix, self.number) }
    pub fn sequence(&self) -> &'static str {
        match self.kind.as_str() { "equation" => "ReadMDEquation", "figure" => "ReadMDFigure", _ => "ReadMDTable" }
    }
}
pub struct Projection {
    pub content: String,
    pub labels: Vec<Value>,
    pub diagnostics: Vec<Value>,
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
        | Options::ENABLE_HEADING_ATTRIBUTES | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
}

/// This marker exists only in the transient projection, never in saved source.
pub fn marker(label: &Label) -> String {
    format!("<!--readmd:caption:v1 {}-->", serde_json::to_string(label).expect("label"))
}
pub fn read_marker(text: &str) -> Option<Label> {
    let text = text.trim().strip_prefix("<!--readmd:caption:v1 ")?.strip_suffix("-->")?;
    if text.len() > 1024 { return None; }
    let label: Label = serde_json::from_str(text).ok()?;
    if label.number == 0 || label.number > 10000 || kind(&label.id) != Some(label.kind.as_str())
        || !pattern().is_match(&format!(":label[{}]", label.id))
        || label.id.chars().any(|c| !(c.is_alphanumeric() || "_:.-".contains(c)))
        || label.prefix.len() > 128 || label.prefix.contains(['\r', '\n']) { return None; }
    Some(label)
}

pub fn project(source: &str) -> Projection {
    let mut result = Projection { content: source.into(), labels: vec![], diagnostics: vec![] };
    if !source.contains(":label[") && !source.contains(":ref[") { return result; }
    let mut depth = 0usize;
    let mut block: Option<(usize, String)> = None;
    let mut blocks: Vec<(Range<usize>, String)> = vec![];
    let mut protected = 0usize;
    let mut runs: Vec<Range<usize>> = vec![];
    let mut heading_ids = std::collections::HashSet::new();
    for (event, range) in Parser::new_ext(source, options()).into_offset_iter() {
        match &event {
            Event::Start(tag) => {
                if let Tag::Heading {id:Some(id),..} = tag { heading_ids.insert(id.to_string()); }
                if depth == 0 {
                    let name = match tag {
                        Tag::Paragraph => "paragraph",
                        Tag::Table(_) => "table",
                        Tag::CodeBlock(CodeBlockKind::Fenced(info)) if info.trim() == "readmd-math v1" => "equation",
                        _ => "other",
                    };
                    let comment=&source[range.clone()];
                    let bridge=matches!(tag,Tag::HtmlBlock)&&(comment.trim_start().starts_with("<!--readmd:math:v1:")||crate::publication_pagination::is_marker(comment)||crate::publication_blocks::read_style(comment).is_ok_and(|s|s.is_some()));
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
                if depth == 0 { if let Some((start, name)) = block.take() { blocks.push((start..range.end, name)); } }
            }
            Event::DisplayMath(_) if depth == 0 => blocks.push((range.clone(), "equation".into())),
            Event::InlineMath(_) | Event::DisplayMath(_) => {
                if let Some((_, name)) = &mut block { if name == "paragraph" { *name = "equation".into(); } }
            }
            Event::Text(_) if protected == 0 => {
                if let Some(last) = runs.last_mut().filter(|last| last.end == range.start) { last.end = range.end; }
                else { runs.push(range.clone()); }
            }
            Event::Html(s) if depth == 0 && !s.trim_start().starts_with("<!--readmd:math:v1:") && !crate::publication_pagination::is_marker(&s) && !crate::publication_blocks::read_style(&s).is_ok_and(|s|s.is_some()) => blocks.push((range.clone(), "other".into())),
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
    let mut numbers: HashMap<&str, usize> = HashMap::new();
    for (range, id, caption, valid) in &declarations {
        let code = if counts[id] != 1 { Some("academic_duplicate_label") } else if !valid { Some("academic_invalid_label") } else { None };
        if let Some(code) = code {
            result.diagnostics.push(json!({"code":code,"key":id,"from":source[..range.start].encode_utf16().count(),"to":source[..range.end].encode_utf16().count()}));
            continue;
        }
        let kind = kind(id).unwrap();
        let number = numbers.entry(kind).or_default(); *number += 1;
        if *number > 10000 { continue; }
        let label = Label { id: id.clone(), kind: kind.into(), number: *number, prefix: prefix(language(source), kind) };
        let text = label.text();
        let suffix = if caption.is_empty() { String::new() } else { format!(" — {caption}") };
        let eol = if source.contains("\r\n") { "\r\n" } else { "\n" };
        let value = format!("{}{eol}{eol}<span id=\"{id}\" class=\"publication-caption\"></span>{text}{suffix}{eol}", marker(&label));
        changes.insert(range.start, (range.end, value));
        result.labels.push(json!({"id":id,"kind":kind,"number":number,"text":text,"caption":caption,"from":source[..range.start].encode_utf16().count(),"to":source[..range.end].encode_utf16().count()}));
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
                changes.insert(start, (end, format!("[{}](#{id})", label.text())));
            } else {
                result.diagnostics.push(json!({"code":"academic_missing_reference","key":id,"from":source[..start].encode_utf16().count(),"to":source[..end].encode_utf16().count()}));
            }
            if changes.len() >= 20000 { break; }
        }
    }
    for (start, (end, text)) in changes.into_iter().rev() { result.content.replace_range(start..end, &text); }
    result
}

/// Typed captions keep field generation out of the frontend projection.
pub fn attach(blocks: &mut Vec<crate::md_ast::Block>) {
    use crate::md_ast::{Block, Inline};
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
        let mut inlines = inlines.clone();
        while matches!(inlines.first(), Some(Inline::Html(_))) { inlines.remove(0); }
        let Some(Inline::Text(first)) = inlines.first_mut() else { index += 1; continue; };
        if !first.starts_with(&label.text()) { index += 1; continue; }
        first.drain(..label.text().len());
        blocks.splice(index..index + 2, [Block::Caption { label, inlines }]);
        index += 1;
    }
}

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
