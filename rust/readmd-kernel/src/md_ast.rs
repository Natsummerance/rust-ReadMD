//! Shared, typed Markdown AST for the document exporters (DOCX, LaTeX, PDF).
//!
//! The older `mdexport::md_parse` is a line-oriented port of the retired Python
//! parser: it drops fence languages, table alignment, task markers, nesting and
//! ordered-list starts.  This module parses with `pulldown-cmark` (GFM tables,
//! strikethrough, task lists, footnotes, `$`/`$$` math, YAML front matter) into
//! an owned tree that every exporter walks, so all formats agree on structure.

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde_json::{json, Value};

/// Column alignment of a GFM table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    None,
    Left,
    Center,
    Right,
}

impl Align {
    pub fn as_str(self) -> &'static str {
        match self {
            Align::None => "",
            Align::Left => "left",
            Align::Center => "center",
            Align::Right => "right",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text(String),
    Code(String),
    Strong(Vec<Inline>),
    Emph(Vec<Inline>),
    Strike(Vec<Inline>),
    Styled { format: crate::publication_rich::RichFormat, children: Vec<Inline> },
    Display { display: crate::publication_rich::RichDisplay, children: Vec<Inline> },
    Citation { source: crate::publication_rich::SourceCitation, children: Vec<Inline> },
    Link { href: String, title: String, children: Vec<Inline> },
    Image { src: String, alt: String, title: String },
    Math(String),
    OfficeMath { latex: String, omml: String },
    SoftBreak,
    HardBreak,
    FootnoteRef(String),
    Html(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListItem {
    /// `Some(checked)` for a GFM task item.
    pub task: Option<bool>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Styled { attributes:Value, block:Box<Block> },
    Heading { level: u8, id: String, inlines: Vec<Inline> },
    Caption { label: crate::cross_references::Label, inlines: Vec<Inline> },
    Paragraph(Vec<Inline>),
    List { ordered: bool, start: u64, items: Vec<ListItem> },
    Quote(Vec<Block>),
    Code { lang: String, text: String },
    Math(String),
    OfficeMath { latex: String, omml: String },
    Table { aligns: Vec<Align>, header: Vec<Vec<Inline>>, rows: Vec<Vec<Vec<Inline>>> },
    Hr,
    PageBreak,
    /// A source-owned section boundary, never an inline or copied body string.
    SectionStart(crate::publication_sections::Section),
    Html(String),
    FootnoteDef { label: String, blocks: Vec<Block> },
    Bibliography(crate::publication_rich::RichBibliography),
}

/// A parsed document: blocks plus the raw YAML front matter (if any).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Document {
    pub front_matter: Option<String>,
    pub blocks: Vec<Block>,
}

impl Document {
    /// Publication language is independent of the application's UI language.
    /// Explicit export metadata overrides top-level author metadata.
    pub fn publication_language(&self, options: &Value) -> Option<String> {
        let scalar = |key: &str| self.front_matter.as_deref().unwrap_or("").lines()
            .filter(|line| !line.starts_with(char::is_whitespace))
            .find_map(|line| line.split_once(':').filter(|(k,_)| k.trim().eq_ignore_ascii_case(key))
                .map(|(_,v)| v.trim().trim_matches(['\'', '"']).to_string()));
        [options.pointer("/meta/language").and_then(Value::as_str).map(str::to_owned),
         options.pointer("/meta/lang").and_then(Value::as_str).map(str::to_owned),
         scalar("lang"), scalar("language")]
            .into_iter().flatten().find_map(|s| publication_language_tag(&s))
    }
    /// Scalar, inline list or indented YAML list, without executing YAML tags.
    pub fn meta_list(&self, key: &str) -> Vec<String> {
        if let Some(value) = self.meta(key) {
            return value.trim_matches(|c| c == '[' || c == ']').split(',')
                .map(|s| s.trim().trim_matches(|c| c == '\'' || c == '"').to_string())
                .filter(|s| !s.is_empty()).collect();
        }
        let mut found = false; let mut values = Vec::new();
        for line in self.front_matter.as_deref().unwrap_or("").lines() {
            if !found {
                found = line.split_once(':').is_some_and(|(k,v)| k.trim().eq_ignore_ascii_case(key) && v.trim().is_empty());
                continue;
            }
            if line.trim().is_empty() { continue; }
            if !line.starts_with(char::is_whitespace) { break; }
            if let Some(value) = line.trim().strip_prefix("- ") {
                values.push(value.trim().trim_matches(|c| c == '\'' || c == '"').to_string());
            }
        }
        values
    }
    /// `title:` from the front matter, trimmed of quotes.
    pub fn meta(&self, key: &str) -> Option<String> {
        let fm = self.front_matter.as_deref()?;
        for line in fm.lines() {
            if let Some((k, v)) = line.split_once(':') {
                if k.trim().eq_ignore_ascii_case(key) {
                    let v = v.trim().trim_matches(|c| c == '"' || c == '\'').trim();
                    if !v.is_empty() {
                        return Some(v.to_string());
                    }
                }
            }
        }
        None
    }
}

/// Bounded, injection-safe language declarations. This normalizes conventional
/// language/script/region casing, not IANA registration or deprecated aliases.
pub(crate) fn publication_language_tag(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > 255 { return None; }
    let normalized = raw.replace('_', "-");
    let mut parts:Vec<String> = normalized.split('-').map(str::to_ascii_lowercase).collect();
    if parts.iter().any(|p| p.is_empty() || p.len()>8 || !p.bytes().all(|b| b.is_ascii_alphanumeric())) { return None; }
    let first=&parts[0];
    if !first.bytes().all(|b| b.is_ascii_alphabetic()) || (first.len()<2 && !(parts.len()>1 && matches!(first.as_str(),"x"|"i"))) { return None; }
    const GRANDFATHERED:&[&str]=&["en-GB-oed","i-ami","i-bnn","i-default","i-enochian","i-hak","i-klingon","i-lux","i-mingo","i-navajo","i-pwn","i-tao","i-tay","i-tsu","sgn-BE-FR","sgn-BE-NL","sgn-CH-DE","art-lojban","cel-gaulish","no-bok","no-nyn","zh-guoyu","zh-hakka","zh-min","zh-min-nan","zh-xiang"];
    if let Some(tag)=GRANDFATHERED.iter().find(|t|t.eq_ignore_ascii_case(&normalized)){return Some((*tag).into());}
    if first=="i"{return None;}
    if first=="x"{return Some(parts.join("-"));}
    // Extension/private-use payloads must not be mistaken for script/region.
    if !matches!(first.as_str(),"x"|"i") {
        let mut i=1;
        if (2..=3).contains(&first.len()) { for _ in 0..3 { if parts.get(i).is_some_and(|p|p.len()==3&&p.bytes().all(|b|b.is_ascii_alphabetic())) {i+=1;} else {break;} } }
        if parts.get(i).is_some_and(|p|p.len()==4&&p.bytes().all(|b|b.is_ascii_alphabetic())) { parts[i][..1].make_ascii_uppercase();i+=1; }
        if parts.get(i).is_some_and(|p|(p.len()==2&&p.bytes().all(|b|b.is_ascii_alphabetic()))||(p.len()==3&&p.bytes().all(|b|b.is_ascii_digit()))) {parts[i].make_ascii_uppercase();i+=1;}
        let mut variants=std::collections::HashSet::new();
        while parts.get(i).is_some_and(|p|p.len()>=5||(p.len()==4&&p.as_bytes()[0].is_ascii_digit())) {
            if !variants.insert(parts[i].clone()){return None;}i+=1;
        }
        let mut extensions=std::collections::HashSet::new();
        while parts.get(i).is_some_and(|p|p.len()==1&&p!="x") {
            if !extensions.insert(parts[i].clone()){return None;}i+=1;let start=i;
            while parts.get(i).is_some_and(|p|p.len()>=2){i+=1;}
            if start==i{return None;}
        }
        if parts.get(i).is_some_and(|p|p=="x"){i+=1;if i==parts.len(){return None;}i=parts.len();}
        if i!=parts.len(){return None;}
    }
    Some(parts.join("-"))
}

pub(crate) fn publication_language_region(tag:&str)->Option<&str> {
    let parts:Vec<_>=tag.split('-').collect();let first=*parts.first()?;
    if matches!(first,"x"|"i"){return None;}
    let mut i=1;
    if first.len()<=3 {for _ in 0..3{if parts.get(i).is_some_and(|p|p.len()==3&&p.bytes().all(|b|b.is_ascii_alphabetic())){i+=1;}else{break;}}}
    if parts.get(i).is_some_and(|p|p.len()==4&&p.bytes().all(|b|b.is_ascii_alphabetic())){i+=1;}
    parts.get(i).copied().filter(|p|p.len()==2&&p.bytes().all(|b|b.is_ascii_alphabetic()))
}

/// Read only the opening metadata block; HTML export must not reparse or
/// compile every formula just to obtain the document language.
pub(crate) fn source_publication_language(source:&str,options:&Value)->Option<String> {
    let mut parser=Parser::new_ext(source.strip_prefix('\u{feff}').unwrap_or(source),Options::ENABLE_YAML_STYLE_METADATA_BLOCKS);
    let mut metadata=String::new();
    if matches!(parser.next(),Some(Event::Start(Tag::MetadataBlock(_)))) {
        for event in parser {match event {Event::Text(t)=>{if metadata.len()+t.len()>65536{break;}metadata.push_str(&t);},Event::End(TagEnd::MetadataBlock(_))=>break,_=>{}}}
    }
    Document{front_matter:Some(metadata),blocks:Vec::new()}.publication_language(options)
}

pub(crate) fn source_without_front_matter(source:&str)->&str {
    let source=source.strip_prefix('\u{feff}').unwrap_or(source);
    let mut parser=Parser::new_ext(source,Options::ENABLE_YAML_STYLE_METADATA_BLOCKS).into_offset_iter();
    if matches!(parser.next(),Some((Event::Start(Tag::MetadataBlock(_)),_))) {
        for (event,range) in parser {if matches!(event,Event::End(TagEnd::MetadataBlock(_))){return &source[range.end..];}}
    }
    source
}

const PAGEBREAKS: [&str; 3] = ["<!-- pagebreak -->", "<!-- page-break -->", "\\newpage"];

/// Private-use sentinels for a task marker that landed inside a paragraph.
const TASK_ON: &str = "\u{E000}x\u{E000}";
const TASK_OFF: &str = "\u{E000} \u{E000}";

pub(crate) fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_MATH
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
}

/// Parse Markdown source into a [`Document`].
pub fn parse(src: &str) -> Document {
    parse_internal(src, None).expect("ordinary Markdown parsing has no publication slots")
}

/// Only the source-bound projection may authorize ephemeral publication slots.
/// Resolve them during parsing, before heading identity and local styles are
/// computed. No processor markup or opaque slot survives into a writer.
pub fn parse_prepared(src: &str, projection: &crate::publication_rich::Projection) -> crate::ApiResult<Document> {
    let instrumented = projection.instrument(src)?;
    parse_internal(&instrumented, Some(projection))
}

fn parse_internal(src: &str, projection: Option<&crate::publication_rich::Projection>) -> crate::ApiResult<Document> {
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    let publication = crate::cross_references::project(src);
    let src = publication.content.as_str();
    let expanded = match crate::math_dsl::expand_markdown(src){
        Ok(expanded)=>expanded,
        Err(code) if projection.is_some()=>return Err(crate::ApiError::new(422,code)),
        Err(_)=>src.into(),
    };
    let src = expanded.as_str();
    let events: Vec<Event> = Parser::new_ext(src, options()).collect();
    let mut b = Builder { ev: events, pos: 0, front_matter: None, slugs: HeadingIds::default(), projection, consumed_slots: std::collections::HashSet::new(), slot_error: None, projected_notes: Vec::new() };
    let mut blocks = b.blocks_until(None);
    if let Some(code) = b.slot_error { return Err(crate::ApiError::new(422, code)); }
    if projection.is_some_and(|p| b.consumed_slots.len() != p.slot_count()) { return Err(crate::ApiError::new(422, "academic_projection_slots")); }
    blocks.append(&mut b.projected_notes);
    attach_office_math(&mut blocks);
    crate::cross_references::attach(&mut blocks);
    crate::publication_blocks::attach(&mut blocks);
    crate::publication_sections::attach(&mut blocks);
    Ok(Document { front_matter: b.front_matter, blocks })
}

pub fn rich_inlines(nodes: &[crate::publication_rich::RichNode]) -> Vec<Inline> {
    use crate::publication_rich::RichNode;
    let mut out = Vec::new();
    for node in nodes {
        match node {
            RichNode::Text { text, format } => out.push(Inline::Styled { format: format.clone(), children: vec![Inline::Text(text.clone())] }),
            RichNode::Link { href, children } => out.push(Inline::Link { href: href.clone(), title: String::new(), children: rich_inlines(children) }),
            RichNode::Group { children, display, .. } => {
                let children = rich_inlines(children);
                if let Some(display) = display { out.push(Inline::Display { display: *display, children }); }
                else { out.extend(children); }
            }
        }
    }
    out
}

fn attach_office_math(blocks: &mut Vec<Block>) {
    let mut index=0;
    while index<blocks.len() {
        match &mut blocks[index] {
            Block::Quote(inner)|Block::FootnoteDef {blocks:inner,..}=>attach_office_math(inner),
            Block::List {items,..}=>for item in items {attach_office_math(&mut item.blocks);},
            _=>(),
        }
        if index>0 {
            if let Block::Html(comment)=&blocks[index] {
                let latex=match &blocks[index-1] {Block::Math(m)=>Some(m.clone()),Block::Paragraph(v) if v.len()==1=>if let Inline::Math(m)=&v[0] {Some(m.clone())} else {None},_=>None};
                if let Some(latex)=latex {
                    if let Ok((metadata,true))=crate::math_document::read_metadata(comment.trim(),&latex) {
                        blocks[index-1]=Block::OfficeMath {latex,omml:metadata.omml};blocks.remove(index);continue;
                    }
                }
            }
        }
        index+=1;
    }
}
impl Block {
    pub fn unstyled(&self)->&Block{match self{Block::Styled{block,..}=>block.unstyled(),other=>other}}
}

struct Builder<'a> {
    ev: Vec<Event<'a>>,
    pos: usize,
    front_matter: Option<String>,
    slugs: HeadingIds,
    projection: Option<&'a crate::publication_rich::Projection>,
    consumed_slots: std::collections::HashSet<String>,
    slot_error: Option<&'static str>,
    projected_notes: Vec<Block>,
}

fn heading_level(l: HeadingLevel) -> u8 {
    match l {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn align_of(a: &Alignment) -> Align {
    match a {
        Alignment::None => Align::None,
        Alignment::Left => Align::Left,
        Alignment::Center => Align::Center,
        Alignment::Right => Align::Right,
    }
}

/// GitHub-style heading slug (lowercase, spaces → `-`, punctuation dropped;
/// CJK and other letters kept).
pub fn slugify(text: &str) -> String {
    let mut out = String::new();
    for c in text.trim().chars() {
        if c.is_alphanumeric() || c == '_' || c == '-' {
            out.extend(c.to_lowercase());
        } else if c.is_whitespace() {
            out.push('-');
        }
    }
    out
}

/// Shared identity allocation for exporters and workspace navigation.
/// Counters avoid rescanning all preceding headings for each duplicate.
#[derive(Default)]
pub(crate) struct HeadingIds {
    used: std::collections::HashSet<String>,
    next: std::collections::HashMap<String, usize>,
}
impl HeadingIds {
    pub(crate) fn allocate(&mut self, title: &str, explicit: Option<&str>) -> String {
        let base = explicit.filter(|s| !s.is_empty()).map(str::to_owned)
            .unwrap_or_else(|| slugify(title));
        let base = if base.is_empty() { "section".to_owned() } else { base };
        if self.used.insert(base.clone()) { return base; }
        let next = self.next.entry(base.clone()).or_insert(1);
        loop {
            let id = format!("{base}-{next}");
            *next += 1;
            if self.used.insert(id.clone()) { return id; }
        }
    }
}

impl<'a> Builder<'a> {
    fn publication_slot(&mut self, marker: &str) -> Option<crate::publication_rich::ProjectionSlot> {
        use crate::publication_rich::{Projection, ProjectionSlot};
        let projection = self.projection?;
        if !Projection::is_slot_marker(marker) { return None; }
        let Some(slot) = projection.slot(marker) else { self.slot_error = Some("academic_projection_slots"); return None; };
        let key = match slot { ProjectionSlot::Citation(index) => format!("c{index}"), ProjectionSlot::Bibliography => "b".into() };
        if !self.consumed_slots.insert(key) { self.slot_error = Some("academic_projection_slots"); return None; }
        Some(slot)
    }
    fn citation_inline(&mut self, index: usize) -> Inline {
        let citation = &self.projection.expect("owned slot").citations[index];
        if let Some(label) = &citation.note_label {
            let note = if citation.note_rich.is_empty() { &citation.rich } else { &citation.note_rich };
            let inline = Inline::Citation { source: citation.source.clone(), children: rich_inlines(note) };
            self.projected_notes.push(Block::FootnoteDef { label: label.clone(), blocks: vec![Block::Paragraph(vec![inline])] });
            if citation.inline_rich.is_empty() { Inline::FootnoteRef(label.clone()) }
            else { let mut children = rich_inlines(&citation.inline_rich); children.push(Inline::FootnoteRef(label.clone())); Inline::Citation { source: citation.source.clone(), children } }
        } else { Inline::Citation { source: citation.source.clone(), children: rich_inlines(&citation.rich) } }
    }
    fn peek(&self) -> Option<&Event<'a>> {
        self.ev.get(self.pos)
    }

    fn next(&mut self) -> Option<Event<'a>> {
        let e = self.ev.get(self.pos).cloned();
        if e.is_some() {
            self.pos += 1;
        }
        e
    }

    /// Collect blocks until the matching `end` tag (consumed) or end of input.
    /// Loose inline events (tight list items) become an implicit paragraph.
    fn blocks_until(&mut self, end: Option<TagEnd>) -> Vec<Block> {
        let mut out = Vec::new();
        let mut loose: Vec<Inline> = Vec::new();
        loop {
            let Some(e) = self.peek().cloned() else { break };
            if let (Event::End(t), Some(want)) = (&e, &end) {
                if t == want {
                    self.pos += 1;
                    break;
                }
            }
            match e {
                Event::Start(tag) if is_block_tag(&tag) => {
                    flush_loose(&mut loose, &mut out);
                    self.pos += 1;
                    if let Some(b) = self.block(tag) {
                        out.push(b);
                    }
                }
                Event::Rule => {
                    flush_loose(&mut loose, &mut out);
                    self.pos += 1;
                    out.push(Block::Hr);
                }
                Event::Html(h) => {
                    flush_loose(&mut loose, &mut out);
                    self.pos += 1;
                    let t = h.trim();
                    if let Some(slot) = self.publication_slot(t) {
                        use crate::publication_rich::ProjectionSlot;
                        out.push(match slot {
                            ProjectionSlot::Citation(index) => Block::Paragraph(vec![self.citation_inline(index)]),
                            ProjectionSlot::Bibliography => Block::Bibliography(self.projection.expect("owned slot").bibliography.clone().expect("owned bibliography")),
                        });
                    } else if PAGEBREAKS.contains(&t) {
                        out.push(Block::PageBreak);
                    } else if !t.is_empty() {
                        out.push(Block::Html(h.to_string()));
                    }
                }
                Event::DisplayMath(m) if loose.is_empty() => {
                    self.pos += 1;
                    out.push(Block::Math(m.trim().to_string()));
                }
                Event::End(_) => {
                    // Stray end (unbalanced): consume to guarantee progress.
                    self.pos += 1;
                    if end.is_none() {
                        continue;
                    }
                    break;
                }
                _ => {
                    if let Some(i) = self.inline_event() {
                        loose.push(i);
                    }
                }
            }
        }
        flush_loose(&mut loose, &mut out);
        out
    }

    fn block(&mut self, tag: Tag<'a>) -> Option<Block> {
        match tag {
            Tag::Paragraph => {
                // A paragraph holding nothing but one `$$…$$` is a display block.
                let mut j = self.pos;
                let mut maths = Vec::new();
                let mut only_math = true;
                while let Some(e) = self.ev.get(j) {
                    match e {
                        Event::End(TagEnd::Paragraph) => break,
                        Event::DisplayMath(m) => maths.push(m.to_string()),
                        Event::SoftBreak | Event::HardBreak => {}
                        Event::Text(t) if t.trim().is_empty() => {}
                        _ => {
                            only_math = false;
                            break;
                        }
                    }
                    j += 1;
                }
                if only_math && maths.len() == 1 {
                    self.pos = j + 1;
                    return Some(Block::Math(maths.remove(0).trim().to_string()));
                }
                let inl = self.inlines_until(TagEnd::Paragraph);
                Some(paragraph_or_special(inl))
            }
            Tag::Heading { level, id, .. } => {
                let lvl = heading_level(level);
                let inl = self.inlines_until(TagEnd::Heading(level));
                let plain = inline_plain(&inl);
                let id = self.slugs.allocate(&plain, id.as_deref());
                Some(Block::Heading { level: lvl, id, inlines: inl })
            }
            Tag::BlockQuote(k) => Some(Block::Quote(self.blocks_until(Some(TagEnd::BlockQuote(k))))),
            Tag::CodeBlock(kind) => {
                let math = matches!(&kind, CodeBlockKind::Fenced(info) if info.trim()=="readmd-math v1");
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => info.split_whitespace().next().unwrap_or("").to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                let mut text = String::new();
                while let Some(e) = self.next() {
                    match e {
                        Event::Text(t) => text.push_str(&t),
                        Event::End(TagEnd::CodeBlock) => break,
                        _ => {}
                    }
                }
                if text.ends_with('\n') {
                    text.pop();
                }
                if math {
                    if let Ok((root,_))=crate::math_dsl::compile(&text,true) {
                        return Some(Block::OfficeMath {latex:crate::math_formats::tex(&root),omml:root.xml()});
                    }
                }
                Some(Block::Code { lang, text })
            }
            Tag::HtmlBlock => {
                let mut html = String::new();
                while let Some(e) = self.next() {
                    match e {
                        Event::Html(h) | Event::Text(h) => html.push_str(&h),
                        Event::End(TagEnd::HtmlBlock) => break,
                        _ => {}
                    }
                }
                // A parsed, valid navigation comment may share this HTML
                // carrier with ordinary Markdown. Feed only that carrier back
                // through the existing builder so its body is not discarded.
                if let Some(events) = crate::publication_navigation::body_carrier_events(&html) {
                    let events: Vec<Event<'a>> = events;
                    self.ev.splice(self.pos..self.pos, events);
                    return None;
                }
                let t = html.trim();
                if let Some(slot) = self.publication_slot(t) {
                    use crate::publication_rich::ProjectionSlot;
                    Some(match slot {
                        ProjectionSlot::Citation(index) => Block::Paragraph(vec![self.citation_inline(index)]),
                        ProjectionSlot::Bibliography => Block::Bibliography(self.projection.expect("owned slot").bibliography.clone().expect("owned bibliography")),
                    })
                } else if PAGEBREAKS.contains(&t) {
                    Some(Block::PageBreak)
                } else if t.is_empty() {
                    None
                } else {
                    Some(Block::Html(html))
                }
            }
            Tag::List(start) => {
                let ordered = start.is_some();
                let mut items = Vec::new();
                loop {
                    match self.next() {
                        Some(Event::Start(Tag::Item)) => {
                            let mut task = None;
                            if let Some(Event::TaskListMarker(c)) = self.peek() {
                                task = Some(*c);
                                self.pos += 1;
                            }
                            let mut blocks = self.blocks_until(Some(TagEnd::Item));
                            // Loose task items carry the marker inside their first paragraph.
                            if task.is_none() {
                                if let Some(Block::Paragraph(inl)) = blocks.first_mut() {
                                    if let Some(Inline::Text(t)) = inl.first_mut() {
                                        if let Some(rest) = t.strip_prefix(TASK_ON) {
                                            task = Some(true);
                                            *t = rest.to_string();
                                        } else if let Some(rest) = t.strip_prefix(TASK_OFF) {
                                            task = Some(false);
                                            *t = rest.to_string();
                                        }
                                        if t.is_empty() {
                                            inl.remove(0);
                                        }
                                    }
                                }
                            }
                            if blocks.is_empty() {
                                blocks.push(Block::Paragraph(Vec::new()));
                            }
                            items.push(ListItem { task, blocks });
                        }
                        Some(Event::End(TagEnd::List(_))) | None => break,
                        _ => {}
                    }
                }
                Some(Block::List { ordered, start: start.unwrap_or(1), items })
            }
            Tag::Table(aligns) => {
                let aligns: Vec<Align> = aligns.iter().map(align_of).collect();
                let mut header = Vec::new();
                let mut rows = Vec::new();
                let mut cur_row: Vec<Vec<Inline>> = Vec::new();
                let mut in_head = false;
                loop {
                    match self.next() {
                        Some(Event::Start(Tag::TableHead)) => {
                            in_head = true;
                            cur_row.clear();
                        }
                        Some(Event::End(TagEnd::TableHead)) => {
                            header = std::mem::take(&mut cur_row);
                            in_head = false;
                        }
                        Some(Event::Start(Tag::TableRow)) => cur_row.clear(),
                        Some(Event::End(TagEnd::TableRow)) => {
                            if !in_head {
                                rows.push(std::mem::take(&mut cur_row));
                            }
                        }
                        Some(Event::Start(Tag::TableCell)) => {
                            cur_row.push(self.inlines_until(TagEnd::TableCell));
                        }
                        Some(Event::End(TagEnd::Table)) | None => break,
                        _ => {}
                    }
                }
                Some(Block::Table { aligns, header, rows })
            }
            Tag::FootnoteDefinition(label) => {
                let blocks = self.blocks_until(Some(TagEnd::FootnoteDefinition));
                Some(Block::FootnoteDef { label: label.to_string(), blocks })
            }
            Tag::MetadataBlock(kind) => {
                let mut text = String::new();
                while let Some(e) = self.next() {
                    match e {
                        Event::Text(t) => text.push_str(&t),
                        Event::End(TagEnd::MetadataBlock(k)) if k == kind => break,
                        _ => {}
                    }
                }
                if self.front_matter.is_none() {
                    self.front_matter = Some(text);
                }
                None
            }
            // Definition lists are not enabled; treat anything else as a container.
            other => {
                let end = other.to_end();
                let inner = self.blocks_until(Some(end));
                if inner.is_empty() {
                    None
                } else {
                    Some(Block::Quote(inner))
                }
            }
        }
    }

    /// Collect inline nodes until `end` (consumed).
    fn inlines_until(&mut self, end: TagEnd) -> Vec<Inline> {
        let mut out = Vec::new();
        loop {
            match self.peek() {
                None => break,
                Some(Event::End(t)) if *t == end => {
                    self.pos += 1;
                    break;
                }
                Some(Event::End(_)) => {
                    // Unbalanced: let the caller see it.
                    break;
                }
                _ => {
                    if let Some(i) = self.inline_event() {
                        if let Inline::Html(comment)=&i {
                            if let Some(Inline::Math(latex))=out.last() {
                                if let Ok((metadata,true))=crate::math_document::read_metadata(comment.trim(),latex) {
                                    let latex=latex.clone();out.pop();out.push(Inline::OfficeMath {latex,omml:metadata.omml});continue;
                                }
                            }
                        }
                        out.push(i);
                    } else if self.pos >= self.ev.len() {
                        break;
                    }
                }
            }
        }
        merge_text(out)
    }

    /// Consume one inline event (or one inline container) and return its node.
    fn inline_event(&mut self) -> Option<Inline> {
        let e = self.next()?;
        Some(match e {
            Event::Text(t) => Inline::Text(t.to_string()),
            Event::Code(t) => Inline::Code(t.to_string()),
            Event::InlineMath(m) => Inline::Math(m.to_string()),
            // Display math inside running text is kept inline (rare; `$$x$$` mid-line).
            Event::DisplayMath(m) => Inline::Math(m.to_string()),
            Event::SoftBreak => Inline::SoftBreak,
            Event::HardBreak => Inline::HardBreak,
            Event::FootnoteReference(l) => Inline::FootnoteRef(l.to_string()),
            Event::InlineHtml(h) | Event::Html(h) => {
                if let Some(slot) = self.publication_slot(&h) {
                    return match slot {
                        crate::publication_rich::ProjectionSlot::Citation(index) => Some(self.citation_inline(index)),
                        crate::publication_rich::ProjectionSlot::Bibliography => { self.slot_error = Some("academic_projection_slots"); None },
                    };
                }
                let t = h.trim().to_ascii_lowercase();
                if t == "<br>" || t == "<br/>" || t == "<br />" {
                    Inline::HardBreak
                } else {
                    Inline::Html(h.to_string())
                }
            }
            // Only reached for loose task items; `ListItem` parsing strips it again.
            Event::TaskListMarker(c) => Inline::Text(if c { TASK_ON.into() } else { TASK_OFF.into() }),
            Event::Start(Tag::Emphasis) => Inline::Emph(self.inlines_until(TagEnd::Emphasis)),
            Event::Start(Tag::Strong) => Inline::Strong(self.inlines_until(TagEnd::Strong)),
            Event::Start(Tag::Strikethrough) => Inline::Strike(self.inlines_until(TagEnd::Strikethrough)),
            Event::Start(Tag::Link { dest_url, title, .. }) => {
                let children = self.inlines_until(TagEnd::Link);
                Inline::Link { href: dest_url.to_string(), title: title.to_string(), children }
            }
            Event::Start(Tag::Image { dest_url, title, .. }) => {
                let alt = inline_plain(&self.inlines_until(TagEnd::Image));
                Inline::Image { src: dest_url.to_string(), alt, title: title.to_string() }
            }
            Event::Start(tag) => {
                // Superscript/subscript etc. are not enabled; flatten any other
                // inline container into its children.
                let end = tag.to_end();
                let inner = self.inlines_until(end);
                Inline::Emph(inner)
            }
            Event::Rule => Inline::Text(String::new()),
            Event::End(_) => return None,
        })
    }
}

fn is_block_tag(tag: &Tag) -> bool {
    matches!(
        tag,
        Tag::Paragraph
            | Tag::Heading { .. }
            | Tag::BlockQuote(_)
            | Tag::CodeBlock(_)
            | Tag::HtmlBlock
            | Tag::List(_)
            | Tag::Table(_)
            | Tag::FootnoteDefinition(_)
            | Tag::MetadataBlock(_)
            | Tag::DefinitionList
    )
}

fn flush_loose(loose: &mut Vec<Inline>, out: &mut Vec<Block>) {
    if loose.is_empty() {
        return;
    }
    let inl = merge_text(std::mem::take(loose));
    if inl.iter().all(|i| matches!(i, Inline::SoftBreak | Inline::HardBreak) || matches!(i, Inline::Text(t) if t.trim().is_empty())) {
        return;
    }
    out.push(paragraph_or_special(inl));
}

/// A paragraph that is only `$$…$$` becomes a math block; one that is only a
/// page-break marker becomes a page break.
fn paragraph_or_special(inl: Vec<Inline>) -> Block {
    let meaningful: Vec<&Inline> = inl
        .iter()
        .filter(|i| !matches!(i, Inline::SoftBreak) && !matches!(i, Inline::Text(t) if t.trim().is_empty()))
        .collect();
    if meaningful.len() == 1 {
        if let Inline::Text(t) = meaningful[0] {
            if PAGEBREAKS.contains(&t.trim()) {
                return Block::PageBreak;
            }
        }
        if let Inline::Html(t) = meaningful[0] {
            if PAGEBREAKS.contains(&t.trim()) {
                return Block::PageBreak;
            }
        }
    }
    Block::Paragraph(inl)
}

/// Join adjacent `Text` runs.
fn merge_text(v: Vec<Inline>) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::with_capacity(v.len());
    for i in v {
        if let Inline::Text(t) = &i {
            if let Some(Inline::Text(prev)) = out.last_mut() {
                prev.push_str(t);
                continue;
            }
        }
        out.push(i);
    }
    out
}

/// Plain text of an inline list (alt text, slugs, bookmarks).
pub fn inline_plain(v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(t) | Inline::Code(t) | Inline::Math(t) => s.push_str(t),
            Inline::OfficeMath {latex,..} => s.push_str(latex),
            Inline::Strong(c) | Inline::Emph(c) | Inline::Strike(c)
                | Inline::Styled { children: c, .. } | Inline::Display { children: c, .. } | Inline::Citation { children: c, .. } => s.push_str(&inline_plain(c)),
            Inline::Link { children, .. } => s.push_str(&inline_plain(children)),
            Inline::Image { alt, .. } => s.push_str(alt),
            Inline::SoftBreak | Inline::HardBreak => s.push(' '),
            Inline::FootnoteRef(l) => {
                s.push('[');
                s.push_str(l);
                s.push(']');
            }
            Inline::Html(_) => {}
        }
    }
    s
}

/// Call `f` for every inline node (recursively) in the document.
pub fn walk_inlines<'d>(blocks: &'d [Block], f: &mut dyn FnMut(&'d Inline)) {
    fn walk_list<'d>(v: &'d [Inline], f: &mut dyn FnMut(&'d Inline)) {
        for i in v {
            f(i);
            match i {
                Inline::Strong(c) | Inline::Emph(c) | Inline::Strike(c)
                    | Inline::Styled { children: c, .. } | Inline::Display { children: c, .. } | Inline::Citation { children: c, .. } => walk_list(c, f),
                Inline::Link { children, .. } => walk_list(children, f),
                _ => {}
            }
        }
    }
    for b in blocks {
        match b.unstyled() {
            Block::Heading { inlines, .. } | Block::Caption { inlines, .. } | Block::Paragraph(inlines) => walk_list(inlines, f),
            Block::List { items, .. } => {
                for it in items {
                    walk_inlines(&it.blocks, f);
                }
            }
            Block::Quote(inner) | Block::FootnoteDef { blocks: inner, .. } => walk_inlines(inner, f),
            Block::Table { header, rows, .. } => {
                for c in header {
                    walk_list(c, f);
                }
                for r in rows {
                    for c in r {
                        walk_list(c, f);
                    }
                }
            }
            _ => {}
        }
    }
}

// ------------------------------------------------------------ render JSON
//
// Projection onto the block/inline JSON shape `pdf_render::render` consumes.

fn inline_json(i: &Inline, out: &mut Vec<Value>) {
    match i {
        Inline::Text(t) => out.push(json!({"t": "text", "v": t})),
        Inline::Code(t) => out.push(json!({"t": "code", "v": t})),
        Inline::Strong(c) => out.push(json!({"t": "bold", "v": inlines_json(c)})),
        Inline::Emph(c) => out.push(json!({"t": "italic", "v": inlines_json(c)})),
        Inline::Strike(c) => out.push(json!({"t": "strike", "v": inlines_json(c)})),
        Inline::Styled { format, children } => out.push(json!({"t":"styled", "format":format, "v":inlines_json(children)})),
        Inline::Display { display, children } => out.push(json!({"t":"display", "display":display, "v":inlines_json(children)})),
        Inline::Citation { children, .. } => out.extend(inlines_json(children).as_array().cloned().unwrap_or_default()),
        Inline::Link { href, title, children } => {
            out.push(json!({"t": "link", "text": inlines_json(children), "href": href, "title": title}))
        }
        Inline::Image { src, alt, title } => out.push(json!({"t": "image", "alt": alt, "src": src, "title": title})),
        Inline::Math(m) => out.push(json!({"t": "math", "latex": m, "display": false, "fallback": true})),
        Inline::OfficeMath {latex,omml} => out.push(json!({"t":"math","latex":latex,"omml":omml,"display":false,"fallback":true})),
        Inline::SoftBreak => out.push(json!({"t": "text", "v": " "})),
        Inline::HardBreak => out.push(json!({"t": "br"})),
        Inline::FootnoteRef(l) => out.push(json!({"t": "text", "v": format!("[{l}]")})),
        Inline::Html(_) => {}
    }
}

pub fn inlines_json(v: &[Inline]) -> Value {
    let mut out = Vec::new();
    for i in v {
        inline_json(i, &mut out);
    }
    Value::Array(out)
}

/// CSS is constructed exclusively from the restricted format enums.
pub fn rich_format_css(format: &crate::publication_rich::RichFormat) -> String {
    use crate::publication_rich::*;
    let mut css = String::new();
    match format.font_style { RichFontStyle::Normal => {}, RichFontStyle::Italic => css.push_str("font-style:italic;"), RichFontStyle::Oblique => css.push_str("font-style:oblique;") }
    if format.font_variant == RichFontVariant::SmallCaps { css.push_str("font-variant:small-caps;"); }
    match format.font_weight { RichFontWeight::Normal => {}, RichFontWeight::Bold => css.push_str("font-weight:700;"), RichFontWeight::Light => css.push_str("font-weight:300;") }
    if format.text_decoration == RichDecoration::Underline { css.push_str("text-decoration:underline;"); }
    match format.vertical_align { RichVerticalAlign::Baseline => {}, RichVerticalAlign::Sup => css.push_str("vertical-align:super;font-size:smaller;"), RichVerticalAlign::Sub => css.push_str("vertical-align:sub;font-size:smaller;") }
    css
}
pub fn rich_format_classes(format: &crate::publication_rich::RichFormat) -> String {
    use crate::publication_rich::*;
    let mut classes=Vec::new();
    match format.font_style { RichFontStyle::Normal=>(),RichFontStyle::Italic=>classes.push("readmd-csl-italic"),RichFontStyle::Oblique=>classes.push("readmd-csl-oblique") }
    if format.font_variant==RichFontVariant::SmallCaps {classes.push("readmd-csl-smallcaps");}
    match format.font_weight {RichFontWeight::Normal=>(),RichFontWeight::Bold=>classes.push("readmd-csl-bold"),RichFontWeight::Light=>classes.push("readmd-csl-light")}
    if format.text_decoration==RichDecoration::Underline {classes.push("readmd-csl-underline");}
    match format.vertical_align {RichVerticalAlign::Baseline=>(),RichVerticalAlign::Sup=>classes.push("readmd-csl-sup"),RichVerticalAlign::Sub=>classes.push("readmd-csl-sub")}
    classes.join(" ")
}

/// Interpret a callout only at the start of a quote. Keep the shared source
/// tree untouched: marker-looking code and ordinary prose remain literal.
pub fn quote_callout(blocks: &[Block]) -> Option<(&'static str, Vec<Inline>, Vec<Block>)> {
    let Block::Paragraph(inlines) = blocks.first()? else { return None };
    if !matches!(inlines.first(), Some(Inline::Text(_))) { return None; }
    let line_end = inlines.iter().position(|i| matches!(i, Inline::SoftBreak | Inline::HardBreak)).unwrap_or(inlines.len());
    let first_line = inline_plain(&inlines[..line_end]);
    let start = first_line.trim_start();
    let marker_end = start.strip_prefix("[!")?.find(']')? + 2;
    let name = start[2..marker_end].to_ascii_lowercase();
    let kind = match name.as_str() {
        "note" | "info" | "todo" | "abstract" | "summary" | "tldr" => "note",
        "tip" | "hint" | "success" | "check" | "done" => "tip",
        "important" | "question" | "help" | "faq" | "example" => "important",
        "warning" | "attention" => "warning",
        "caution" | "danger" | "error" | "failure" | "fail" | "missing" | "bug" => "caution",
        _ => return None,
    };
    let mut rest = &start[marker_end + 1..];
    if rest.starts_with(['+', '-']) { rest = &rest[1..]; }
    rest = rest.trim_start_matches([' ', '\t']);
    let mut skip = first_line.len() - rest.len();
    let mut title = inlines[..line_end].to_vec();
    while skip > 0 {
        let Some(Inline::Text(text)) = title.first_mut() else { return None };
        let count = skip.min(text.len());
        if !text.is_char_boundary(count) { return None; }
        text.drain(..count); skip -= count;
        if text.is_empty() { title.remove(0); }
    }
    let mut body = Vec::new();
    if line_end < inlines.len() && line_end + 1 < inlines.len() {
        body.push(Block::Paragraph(inlines[line_end + 1..].to_vec()));
    }
    body.extend_from_slice(&blocks[1..]);
    Some((kind, title, body))
}

fn push_blocks_json(blocks: &[Block], level: usize, out: &mut Vec<Value>, context: Option<&'static str>) {
    for b in blocks {
        let first = out.len();
        match b {
            Block::Styled { attributes, block } => {
                push_blocks_json(std::slice::from_ref(block.as_ref()),level,out,context);
                let covered = ["text", "paragraph"].iter().any(|key| attributes[*key].as_object().is_some_and(|values| !values.is_empty()));
                if covered {
                    if let Some(value) = out.get_mut(first) {
                        let previous = value.get("publicationStyle").cloned();
                        value["publicationStyle"] = match previous {
                            None => attributes.clone(),
                            Some(Value::Array(mut layers)) => { layers.insert(0,attributes.clone()); Value::Array(layers) },
                            Some(previous) => json!([attributes,previous]),
                        };
                    }
                }
            },
            Block::SectionStart(section) => out.push(json!({"type":"section", "section":section})),
            Block::Heading { level: l, inlines, id } => {
                out.push(json!({"type": "heading", "level": *l as i64, "id": id, "text": inlines_json(inlines)}))
            }
            Block::Paragraph(inl) => out.push(json!({"type": "paragraph", "text": inlines_json(inl)})),
            Block::Caption { label, inlines } => {
                let mut full = vec![Inline::Text(label.text())]; full.extend_from_slice(inlines);
                out.push(json!({"type":"caption","id":label.id,"text":inlines_json(&full)}));
            }
            Block::List { ordered, start, items } => {
                // `pdf_render` draws a flat list of items; nesting is carried as
                // `level` (indent) and every item carries its own number.
                let mut flat = Vec::new();
                let mut tail = Vec::new();
                for (n, it) in items.iter().enumerate() {
                    let mut text = Vec::new();
                    let mut rest: Vec<&Block> = Vec::new();
                    for (bi, blk) in it.blocks.iter().enumerate() {
                        match blk {
                            Block::Paragraph(inl) if bi == 0 => {
                                if let Value::Array(a) = inlines_json(inl) {
                                    text = a;
                                }
                            }
                            other => rest.push(other),
                        }
                    }
                    flat.push(json!({
                        "text": text,
                        "task": it.task.is_some(),
                        "checked": it.task.unwrap_or(false),
                        "ordered": *ordered,
                        "number": *start + n as u64,
                        "level": level,
                    }));
                    for blk in rest {
                        if let Block::List { .. } = blk {
                            // Nested list: flush what we have, then the child.
                            let mut child = Vec::new();
                            push_blocks_json(std::slice::from_ref(blk), level + 1, &mut child, context.map(|_| "list"));
                            for c in child {
                                if c.get("type").and_then(|t| t.as_str()) == Some("list") {
                                    if let Some(Value::Array(ci)) = c.get("items") {
                                        flat.extend(ci.iter().cloned());
                                    }
                                } else {
                                    tail.push(c);
                                }
                            }
                        } else {
                            let mut child = Vec::new();
                            push_blocks_json(std::slice::from_ref(blk), level + 1, &mut child, context.map(|_| "list"));
                            // Non-list continuation blocks (code, quote…) break the
                            // flat list; emit the list so far, then them.
                            if !flat.is_empty() {
                                out.push(json!({"type": "list", "items": std::mem::take(&mut flat)}));
                            }
                            out.extend(child);
                        }
                    }
                }
                if !flat.is_empty() {
                    out.push(json!({"type": "list", "items": flat}));
                }
                out.extend(tail);
            }
            Block::Quote(inner) => {
                let mut v = Vec::new();
                if let Some((kind, title, body)) = quote_callout(inner) {
                    push_blocks_json(&body, level, &mut v, context.map(|_| "quote"));
                    out.push(json!({"type": "quote", "kind": kind, "title": inlines_json(&title), "blocks": v}));
                } else {
                    push_blocks_json(inner, level, &mut v, context.map(|_| "quote"));
                    out.push(json!({"type": "quote", "blocks": v}));
                }
            }
            Block::Code { lang, text } => out.push(json!({"type": "code", "lang": lang, "content": text})),
            Block::Math(m) => out.push(json!({"type": "math", "display": true, "latex": m, "fallback": true})),
            Block::OfficeMath {latex,omml} => out.push(json!({"type":"math","display":true,"latex":latex,"omml":omml,"fallback":true})),
            Block::Table { aligns, header, rows } => {
                let al: Vec<Value> = aligns
                    .iter()
                    .map(|a| if *a == Align::None { Value::Null } else { Value::String(a.as_str().into()) })
                    .collect();
                let has_align = aligns.iter().any(|a| *a != Align::None);
                let mut t = json!({
                    "type": "table",
                    "header": header.iter().map(|c| inlines_json(c)).collect::<Vec<_>>(),
                    "rows": rows.iter().map(|r| r.iter().map(|c| inlines_json(c)).collect::<Vec<_>>()).collect::<Vec<_>>(),
                });
                if has_align {
                    t["aligns"] = Value::Array(al);
                }
                out.push(t);
            }
            Block::Hr => out.push(json!({"type": "hr"})),
            Block::PageBreak => out.push(json!({"type": "pagebreak"})),
            Block::Bibliography(bibliography) => {
                out.push(json!({"type":"bibliography", "title":bibliography.title, "hangingIndent":bibliography.hanging_indent,
                    "lineSpacing":bibliography.line_spacing, "entrySpacing":bibliography.entry_spacing,
                    "complexLayout":bibliography.items.iter().any(|item|item.layout().map(|flow|crate::publication_rich::layout_requires_flow(&flow)).unwrap_or(true)),
                    "items":bibliography.items.iter().map(|item|{let layout=item.layout();json!({"key":item.key,"text":inlines_json(&rich_inlines(&item.rich)),"firstField":inlines_json(&rich_inlines(&item.first_field)),"layout":layout.as_ref().ok(),"layoutError":layout.as_ref().err().map(|error|&error.code)})}).collect::<Vec<_>>()}));
            }
            Block::Html(_) => {}
            Block::FootnoteDef { label, blocks } => {
                let mut inner = Vec::new();
                push_blocks_json(blocks, level, &mut inner, context.map(|_| "footnote"));
                // Rendered as a small paragraph `[label] …`.
                let mut first = true;
                for mut b in inner {
                    if first && b.get("type").and_then(|t| t.as_str()) == Some("paragraph") {
                        if let Some(Value::Array(a)) = b.get_mut("text") {
                            a.insert(0, json!({"t": "text", "v": format!("[{label}] ")}));
                        }
                        first = false;
                    }
                    out.push(b);
                }
            }
        }
        if let Some(context) = context {
            for value in &mut out[first..] {
                if value.get("publicationContext").is_none() { value["publicationContext"] = json!(context); }
            }
        }
    }
}

/// The `pdf_render` block JSON for a parsed document.
pub fn to_render_json(blocks: &[Block]) -> Value {
    let mut out = Vec::new();
    push_blocks_json(blocks, 0, &mut out, None);
    Value::Array(out)
}

/// New typed paragraph spacing needs story ownership that the legacy flat
/// adapter did not retain. Existing callers without a new request keep the
/// old JSON shape; this projection adds only non-visible context metadata.
pub(crate) fn to_publication_render_json(blocks: &[Block]) -> Value {
    let mut out = Vec::new(); push_blocks_json(blocks,0,&mut out,Some("body")); Value::Array(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn publication_language_tags_keep_script_region_and_extension_boundaries() {
        for (input,expected,region) in [("zh_hant_tw","zh-Hant-TW",Some("TW")),("DE-de","de-DE",Some("DE")),("es-419","es-419",None),("zh-cmn-hans-cn","zh-cmn-Hans-CN",Some("CN")),("en-u-ca-gregory-x-US","en-u-ca-gregory-x-us",None),("x-note-US","x-note-us",None),("i-klingon","i-klingon",None),("sl-rozaj-biske-1994","sl-rozaj-biske-1994",None)] {
            let tag=super::publication_language_tag(input).unwrap();assert_eq!(tag,expected);assert_eq!(super::publication_language_region(&tag),region);
        }
        for invalid in ["", "de--DE", "12-US", "en-a", "en-x", "en-u-ca-u-nu", "sl-rozaj-rozaj", "i-unknown", "de\" onclick=\"x", "fr\n<script>"] {assert!(super::publication_language_tag(invalid).is_none(),"{invalid}");}
    }
    #[test]
    fn publication_language_uses_only_opening_top_level_metadata_and_explicit_overrides() {
        let source="\u{feff}---\nother:\n  lang: ja-JP\nlanguage: de_DE\n---\n# Text\n\n```yaml\nlang: fr-FR\n```\n";
        for options in [serde_json::json!({}),serde_json::json!({"meta":{"language":""}})] {assert_eq!(super::parse(source).publication_language(&options).as_deref(),Some("de-DE"));assert_eq!(super::source_publication_language(source,&options).as_deref(),Some("de-DE"));}
        let override_=serde_json::json!({"meta":{"language":"fr_CA"}});assert_eq!(super::source_publication_language(source,&override_).as_deref(),Some("fr-CA"));
        assert_eq!(super::source_publication_language("# Body\n\nlang: de-DE",&serde_json::json!({})),None);
        assert_eq!(super::source_publication_language("---\nother:\n  lang: ja-JP\n---\nBody",&serde_json::json!({})),None);
        assert_eq!(super::source_publication_language("# Body",&override_).as_deref(),Some("fr-CA"));
    }
    use super::*;

    fn p(s: &str) -> Vec<Block> {
        parse(s).blocks
    }

    #[test]
    fn headings_get_unique_slugs() {
        let b = p("# Intro\n\n## Intro\n\n### 中文 标题\n");
        let ids: Vec<&str> = b
            .iter()
            .filter_map(|b| if let Block::Heading { id, .. } = b { Some(id.as_str()) } else { None })
            .collect();
        assert_eq!(ids, vec!["intro", "intro-1", "中文-标题"]);
    }

    #[test]
    fn callouts_preserve_types_titles_body_and_literal_examples() {
        let source = "> [!WARNING]- **Check first**\n> Keep $x^2$ and [the source](https://example.org).\n>\n> Second paragraph.\n\n> `[!NOTE]` is literal code.\n\n> [!UNKNOWN]\n> Keep this ordinary quote.\n";
        let document = parse(source);
        let json = to_render_json(&document.blocks);
        assert_eq!(json[0]["kind"], "warning");
        assert_eq!(json[0]["title"][0]["t"], "bold");
        assert_eq!(json[0]["blocks"].as_array().unwrap().len(), 2);
        assert!(json[0]["blocks"].to_string().contains("https://example.org"));
        assert!(json[0]["blocks"].to_string().contains("x^2"));
        assert!(json[1].get("kind").is_none());
        assert!(json[2].get("kind").is_none());
        let Block::Quote(original) = &document.blocks[0] else { panic!("quote lost") };
        let Block::Paragraph(inlines) = &original[0] else { panic!("paragraph lost") };
        assert!(inline_plain(inlines).starts_with("[!WARNING]- Check first"));
    }

    #[test]
    fn front_matter_is_stripped() {
        let d = parse("---\ntitle: \"Hello\"\nauthor: Me\n---\n\n# H\n");
        assert_eq!(d.meta("title").as_deref(), Some("Hello"));
        assert_eq!(d.meta("author").as_deref(), Some("Me"));
        assert!(matches!(d.blocks[0], Block::Heading { level: 1, .. }));
    }

    #[test]
    fn nested_ordered_task_lists() {
        let b = p("3. a\n4. b\n   - [x] done\n   - [ ] todo\n");
        match &b[0] {
            Block::List { ordered: true, start: 3, items } => {
                assert_eq!(items.len(), 2);
                match &items[1].blocks[1] {
                    Block::List { ordered: false, items: sub, .. } => {
                        assert_eq!(sub[0].task, Some(true));
                        assert_eq!(sub[1].task, Some(false));
                    }
                    other => panic!("{other:?}"),
                }
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn table_alignment_and_code_lang() {
        let b = p("| a | b | c |\n|:--|:-:|--:|\n| 1 | 2 | 3 |\n\n```rust\nfn x() {}\n```\n");
        match &b[0] {
            Block::Table { aligns, header, rows } => {
                assert_eq!(aligns, &vec![Align::Left, Align::Center, Align::Right]);
                assert_eq!(header.len(), 3);
                assert_eq!(rows.len(), 1);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(b[1], Block::Code { lang: "rust".into(), text: "fn x() {}".into() });
    }

    #[test]
    fn math_inline_and_display() {
        let b = p("Euler $e^{i\\pi}$ ok\n\n$$\n\\frac{a}{b}\n$$\n");
        match &b[0] {
            Block::Paragraph(inl) => assert!(inl.contains(&Inline::Math("e^{i\\pi}".into()))),
            other => panic!("{other:?}"),
        }
        assert_eq!(b[1], Block::Math("\\frac{a}{b}".into()));
    }

    #[test]
    fn pagebreak_markers() {
        let b = p("a\n\n<!-- pagebreak -->\n\nb\n\n\\newpage\n\nc\n");
        assert_eq!(b.iter().filter(|x| **x == Block::PageBreak).count(), 2);
    }

    #[test]
    fn render_json_carries_lang_aligns_task() {
        let d = parse("```py\nx\n```\n\n| a |\n|--:|\n| 1 |\n\n- [x] t\n");
        let j = to_render_json(&d.blocks);
        assert_eq!(j[0]["lang"], "py");
        assert_eq!(j[1]["aligns"][0], "right");
        assert_eq!(j[2]["items"][0]["task"], true);
        assert_eq!(j[2]["items"][0]["checked"], true);
    }

    #[test]
    fn never_panics_on_odd_input() {
        let samples = [
            "", "*", "**", "***a", "`", "$", "$$", "$$\n", "| a |\n", "- \n-", "> > >", "[x](", "![", "<!--",
            "中文**粗**`码`$x$", "- [ ]", "1.", "```\n", "---\n", "---\ntitle: x", "[^1]\n\n[^1]: n",
        ];
        for s in samples {
            let _ = to_render_json(&parse(s).blocks);
        }
    }
}
