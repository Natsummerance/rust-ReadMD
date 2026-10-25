//! Source-bound publication blocks and readable, opt-in local layout attributes.
//! Operations return ordinary revision-checked edits; they never write a file.
use crate::document_transaction::{self as tx, ContentTransaction, TextEdit};
use pulldown_cmark::{CodeBlockKind, Event, Tag, TagEnd};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    ops::Range,
};

pub const PREFIX: &str = "<!-- readmd:layout ";
pub const MAX_NODES: usize = 32768;

#[derive(Debug, Clone, Serialize)]
pub struct Node {
    pub id: String,
    pub kind: String,
    pub from: usize,
    pub to: usize,
    pub digest: String,
    pub title: String,
    pub editable: bool,
    pub nested: bool,
    pub style: Value,
    #[serde(skip)]
    pub bytes: Range<usize>,
    #[serde(skip)]
    pub directive: Option<Range<usize>>,
    #[serde(skip)]
    pub inline_content: Range<usize>,
    #[serde(skip)]
    pub directive_inline: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct Document {
    pub revision: String,
    pub nodes: Vec<Node>,
    pub diagnostics: Vec<Value>,
}

/// Strict, bounded attributes: unknown keys are errors rather than silently lost.
pub fn validate_style(style: &Value) -> Result<(), &'static str> {
    let root = style.as_object().ok_or("publication_local_style_invalid")?;
    for (section, value) in root {
        if section == "id" {
            let id = value.as_str().ok_or("publication_local_style_invalid")?;
            if id.is_empty()
                || id.len() > 80
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            {
                return Err("publication_local_style_invalid");
            }
            continue;
        }
        let values = value.as_object().ok_or("publication_local_style_invalid")?;
        for (key, value) in values {
            let valid = match (section.as_str(), key.as_str()) {
                ("text", "author") => crate::publication_inline_text::AuthorTextStyle::from_value(value).is_ok(),
                ("text", "size") => bounded(value, 6., 72.),
                ("text", "trackingPt") => crate::publication_typography::tracking::parse(value).is_ok(),
                ("text", "baselineShiftPt") => crate::publication_typography::baseline::parse(value).is_ok(),
                ("text", "font") => value.as_str().is_some_and(|s| {
                    !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control)
                }),
                ("text", "color") => value.as_str().is_some_and(|s| {
                    s.len() == 7
                        && s.starts_with('#')
                        && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
                }),
                ("text", "bold" | "italic")
                | ("paragraph", "keepNext" | "keepLines" | "widowControl" | "pageBreakBefore") => {
                    value.is_boolean()
                }
                ("paragraph", "align") => value
                    .as_str()
                    .is_some_and(|s| ["left", "center", "right", "justify"].contains(&s)),
                ("paragraph", "before" | "after") => bounded(value, 0., 100.),
                ("paragraph", "left" | "right" | "firstLine" | "hanging") => {
                    bounded(value, 0., 60.)
                }
                ("paragraph", "lineHeight") => bounded(value, 1., 3.),
                ("paragraph", "lineSpacing") => crate::publication_typography::parse_line_spacing(value, crate::publication_typography::Scope::LocalBlock).is_ok(),
                ("paragraph", "shading" | "border") => crate::publication_typography::decoration::parse_key(key, value).is_ok(),
                _ => false,
            };
            if !valid {
                return Err("publication_local_style_invalid");
            }
        }
        if !matches!(section.as_str(), "text" | "paragraph") {
            return Err("publication_local_style_invalid");
        }
    }
    crate::publication_typography::validate_local(&style["paragraph"])?;
    if style["paragraph"]["firstLine"].as_f64().unwrap_or(0.) > 0.
        && style["paragraph"]["hanging"].as_f64().unwrap_or(0.) > 0.
    {
        return Err("publication_local_style_invalid");
    }
    Ok(())
}
fn bounded(v: &Value, min: f64, max: f64) -> bool {
    v.as_f64()
        .is_some_and(|n| n.is_finite() && n >= min && n <= max)
}
fn validate_target(kind: &str, style: &Value) -> Result<(), &'static str> {
    if style["text"].get("author").is_some()&&kind!="caption"{return Err(crate::publication_inline_text::INVALID);}
    // Paragraph decoration is an ordinary-body attribute: headings, captions
    // and math blocks never accept it (attach's own allowed-kind set).
    if kind != "paragraph"
        && (style["paragraph"].get("shading").is_some() || style["paragraph"].get("border").is_some())
    { return Err(crate::publication_typography::INVALID); }
    if kind == "math"
        && (style["text"]
            .as_object()
            .is_some_and(|o| o.keys().any(|k| !matches!(k.as_str(), "size" | "color")))
            || style["paragraph"].as_object().is_some_and(|o| {
                o.keys().any(|k| {
                    matches!(
                        k.as_str(),
                        "firstLine" | "hanging" | "lineHeight" | "lineSpacing" | "widowControl"
                    )
                })
            }))
    {
        return Err("publication_local_style_invalid");
    }
    Ok(())
}
pub fn read_style(comment: &str) -> Result<Option<Value>, &'static str> {
    let Some(rest) = comment.trim().strip_prefix(PREFIX) else {
        return Ok(None);
    };
    if comment.len() > 16384 {
        return Err("publication_local_style_invalid");
    }
    let rest = rest
        .strip_prefix("v1 ")
        .ok_or("publication_local_style_version")?;
    let text = rest
        .strip_suffix("-->")
        .ok_or("publication_local_style_invalid")?
        .trim();
    let style: Value = serde_json::from_str(text).map_err(|_| "publication_local_style_invalid")?;
    validate_style(&style)?;
    Ok(Some(style))
}
pub fn comment(style: &Value) -> Result<String, &'static str> {
    validate_style(style)?;
    // HTML comments cannot contain -->, including a user-selected font name.
    let text = serde_json::to_string(style)
        .map_err(|_| "publication_local_style_invalid")?
        .replace('<', "\\u003c")
        .replace('>', "\\u003e");
    Ok(format!("{PREFIX}v1 {text} -->"))
}
pub fn font_family(name: &str) -> &str {
    if name == "MicrosoftYaHei" {
        "Microsoft YaHei"
    } else {
        name
    }
}
fn line_start(source: &str, offset: usize) -> usize {
    source[..offset]
        .rfind('\n')
        .map_or(if source.starts_with('\u{feff}') { 3 } else { 0 }, |n| {
            n + 1
        })
}
fn utf16_offsets(source: &str, positions: &BTreeSet<usize>) -> BTreeMap<usize, usize> {
    let mut result = BTreeMap::new();
    let mut count = 0;
    for (at, ch) in source.char_indices() {
        if positions.contains(&at) {
            result.insert(at, count);
        }
        count += ch.len_utf16();
    }
    if positions.contains(&source.len()) {
        result.insert(source.len(), count);
    }
    result
}
fn node_identity(kind: &str, range: &Range<usize>, source: &str) -> String {
    format!(
        "block-{}",
        &tx::revision(&format!(
            "{kind}:{}:{}:{}",
            range.start,
            range.end,
            tx::revision(&source[range.clone()])
        ))[..24]
    )
}
fn inline_event(event: &Event<'_>) -> bool {
    matches!(
        event,
        Event::Text(_)
            | Event::Code(_)
            | Event::InlineMath(_)
            | Event::InlineHtml(_)
            | Event::FootnoteReference(_)
            | Event::SoftBreak
            | Event::HardBreak
            | Event::Start(
                Tag::Emphasis
                    | Tag::Strong
                    | Tag::Strikethrough
                    | Tag::Link { .. }
                    | Tag::Image { .. }
            )
            | Event::End(
                TagEnd::Emphasis
                    | TagEnd::Strong
                    | TagEnd::Strikethrough
                    | TagEnd::Link
                    | TagEnd::Image
            )
    )
}

pub fn analyze(source: &str) -> Result<Document, &'static str> {
    if source.len() > tx::MAX_DOCUMENT_BYTES {
        return Err("transaction_budget");
    }
    if crate::publication_inline_text::has_owned_syntax(source){
        analyze_events(source,crate::publication_inline_text::markdown_events(source)?)
    }else{
        let bom=usize::from(source.starts_with('\u{feff}'))*3;
        analyze_events(source,pulldown_cmark::Parser::new_ext(&source[bom..],crate::md_ast::options()).into_offset_iter().map(|(event,range)|(event,range.start+bom..range.end+bom)))
    }
}
fn analyze_events<'event>(source:&str,events:impl IntoIterator<Item=(Event<'event>,Range<usize>)>)->Result<Document,&'static str>{
    let mut nodes = Vec::new();
    let mut markers = Vec::new();
    let mut diagnostics = Vec::new();
    let mut container = 0usize;
    let mut paragraph = false;
    let mut active = None;
    let mut implicit = false;
    let mut blocked = 0usize;
    // Raw HTML blocks emit a separate Html event for each physical line.
    // A declaration-looking line inside <pre>/<script>/another raw block is
    // an example, not a live layout comment. Only an HTML block introduced by
    // the declaration itself may supply block markers.
    let mut html_block_markers = false;
    // A prepared caption consists of one strict typed marker followed by its
    // exact anchor paragraph. Keep that source range with the paragraph so a
    // preceding layout declaration still selects the same caption after
    // cross-reference projection. Visible title text never selects a target.
    let mut caption_marker:Option<(Range<usize>,String)>=None;
    // Mapping must use the exact content parser dialect. Options::all also
    // enables legacy footnotes, which consume later note paragraphs without
    // the normal structural events and make those paragraphs uneditable.
    for (event, range) in events {
        let inline = inline_event(&event);
        if implicit && !inline {
            active = None;
            implicit = false;
            paragraph = false;
        }
        if inline && active.is_none() && container > 0 && blocked == 0 {
            if nodes.len() >= MAX_NODES {
                return Err("transaction_budget");
            }
            nodes.push(Node {
                id: String::new(),
                kind: "paragraph".into(),
                from: 0,
                to: 0,
                digest: String::new(),
                title: String::new(),
                editable: true,
                nested: true,
                style: json!({}),
                bytes: range.clone(),
                directive: None,
                inline_content: range.end..range.start,
                directive_inline: false,
            });
            active = Some(nodes.len() - 1);
            implicit = true;
            paragraph = true;
        }
        // Inline event ranges exclude list/quote prefixes, heading closing
        // hashes and explicit heading IDs. Appending an invisible directive
        // here does not split a tight list or change its Markdown structure.
        if let Some(index) = active {
            if inline {
                let node: &mut Node = &mut nodes[index];
                node.inline_content.start = node.inline_content.start.min(range.start);
                if !matches!(&event, Event::SoftBreak | Event::HardBreak) {
                    node.inline_content.end = node.inline_content.end.max(range.end);
                }
                if implicit {
                    node.bytes.start = node.bytes.start.min(range.start);
                    node.bytes.end = node.bytes.end.max(range.end);
                }
            }
        }
        let mut caption_start=None;
        let kind = match &event {
            Event::Start(Tag::Paragraph) => {
                paragraph = true;
                let raw = source[range.clone()].trim();
                if let Some((marker,anchor))=caption_marker.take(){
                    if container==0&&blocked==0&&marker.end<=range.start
                        &&source[marker.end..range.start].trim().is_empty()&&raw.starts_with(&anchor){caption_start=Some(marker.start);}
                }
                Some(if caption_start.is_some()||raw.starts_with(":label[") {
                    "caption"
                } else if raw.starts_with("$$") && raw.ends_with("$$") {
                    "math"
                } else {
                    "paragraph"
                })
            }
            Event::Start(Tag::Heading { .. }) => Some("heading"),
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info)))
                if info.trim() == "readmd-math v1" =>
            {
                Some("math")
            }
            Event::DisplayMath(_) if !paragraph => Some("math"),
            _ => None,
        };
        if let Some(kind) = kind {
            if nodes.len() >= MAX_NODES {
                return Err("transaction_budget");
            }
            let raw = &source[range.clone()];
            let id = format!(
                "block-{}",
                &tx::revision(&format!(
                    "{kind}:{}:{}:{}",
                    range.start,
                    range.end,
                    tx::revision(raw)
                ))[..24]
            );
            let nested = container > 0;
            let editable = if nested {
                matches!(kind, "paragraph" | "heading")
            } else {
                source[line_start(source, range.start)..range.start]
                    .trim_matches('\u{feff}')
                    .trim()
                    .is_empty()
            };
            nodes.push(Node {
                id,
                kind: kind.into(),
                from: 0,
                to: 0,
                digest: tx::revision(raw),
                title: raw
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .chars()
                    .take(100)
                    .collect(),
                editable,
                nested,
                style: json!({}),
                bytes: caption_start.unwrap_or(range.start)..range.end,
                directive: None,
                inline_content: range.end..range.start,
                directive_inline: false,
            });
            if matches!(kind, "paragraph" | "heading" | "caption") {
                active = Some(nodes.len() - 1);
            }
        }
        match event {
            Event::Start(Tag::HtmlBlock) => {
                let raw=source[range.clone()].trim();
                html_block_markers=raw.starts_with(PREFIX)||crate::cross_references::read_marker(raw).is_some();
                blocked+=1;
            }
            Event::End(TagEnd::HtmlBlock) => {
                html_block_markers=false;
                blocked=blocked.saturating_sub(1);
            }
            Event::Start(Tag::CodeBlock(_) | Tag::Table(_)) => blocked += 1,
            Event::End(TagEnd::CodeBlock | TagEnd::Table) => {
                blocked = blocked.saturating_sub(1)
            }
            Event::Start(
                Tag::BlockQuote(_) | Tag::List(_) | Tag::Item | Tag::FootnoteDefinition(_),
            ) => {
                container += 1;
            }
            Event::End(
                TagEnd::BlockQuote(_) | TagEnd::List(_) | TagEnd::Item | TagEnd::FootnoteDefinition,
            ) => {
                container = container.saturating_sub(1);
            }
            Event::End(TagEnd::Paragraph) => {
                paragraph = false;
                active = None;
            }
            Event::End(TagEnd::Heading(_)) => active = None,
            Event::Html(html) if html_block_markers&&container==0&&blocked<=1&&crate::cross_references::read_marker(&html).is_some() => {
                let label=crate::cross_references::read_marker(&html).expect("checked typed caption marker");
                let trimmed=html.trim();let start=range.start+html.len()-html.trim_start().len();
                caption_marker=Some((start..start+trimmed.len(),format!("<span id=\"{}\" class=\"publication-caption\"></span>",label.id)));
            }
            Event::Html(html) if html_block_markers&&html.trim_start().starts_with(PREFIX) => {
                let trimmed = html.trim();
                let start = range.start + html.len() - html.trim_start().len();
                let end = start + trimmed.len();
                if markers.len() >= MAX_NODES {
                    return Err("transaction_budget");
                }
                markers.push((start..end, read_style(trimmed), None));
            }
            Event::InlineHtml(html) if html.trim_start().starts_with(PREFIX) => {
                let trimmed = html.trim();
                let start = range.start + html.len() - html.trim_start().len();
                if markers.len() >= MAX_NODES {
                    return Err("transaction_budget");
                }
                markers.push((start..start + trimmed.len(), read_style(trimmed), active));
            }
            _ => (),
        }
    }
    for node in &mut nodes {
        node.digest = tx::revision(&source[node.bytes.clone()]);
        node.id = node_identity(&node.kind, &node.bytes, source);
        node.title = source[node.bytes.clone()]
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .chars()
            .take(100)
            .collect();
    }
    let mut used = HashSet::new();
    for (range, style, inline) in markers {
        let at = inline.unwrap_or_else(|| nodes.partition_point(|n| n.bytes.start < range.end));
        let found = nodes.get_mut(at).filter(|n| {
            if inline.is_some() {
                n.inline_content.end == range.end
            } else {
                source[range.end..n.bytes.start].trim().is_empty()
            }
        });
        let error = match style {
            Ok(Some(mut style)) => {
                if let Some(node) = found {
                    if !node.editable || node.directive.is_some() {
                        Some("publication_local_style_orphan")
                    } else if let Err(code) = validate_target(&node.kind, &style) {
                        Some(code)
                    } else {
                        if let Some(id) = style["id"].as_str() {
                            node.id = id.into();
                        } else {
                            style["id"] = json!(node.id);
                        }
                        if !used.insert(node.id.clone()) {
                            Some("publication_local_style_duplicate")
                        } else {
                            node.style = style;
                            node.directive = Some(range.clone());
                            node.directive_inline = inline.is_some();
                            if inline.is_some() {
                                let mut raw = source[node.bytes.clone()].to_string();
                                raw.replace_range(
                                    range.start - node.bytes.start..range.end - node.bytes.start,
                                    "",
                                );
                                node.title = raw
                                    .lines()
                                    .next()
                                    .unwrap_or("")
                                    .trim()
                                    .chars()
                                    .take(100)
                                    .collect();
                            }
                            None
                        }
                    }
                } else {
                    Some("publication_local_style_orphan")
                }
            }
            Err(code) => Some(code),
            Ok(None) => None,
        };
        if let Some(code) = error {
            diagnostics.push(json!({"code":code,"bytesFrom":range.start,"bytesTo":range.end}));
        }
    }
    let positions = nodes
        .iter()
        .flat_map(|n| [n.bytes.start, n.bytes.end])
        .chain(diagnostics.iter().flat_map(|d| {
            [
                d["bytesFrom"].as_u64().unwrap() as usize,
                d["bytesTo"].as_u64().unwrap() as usize,
            ]
        }))
        .collect();
    let offsets = utf16_offsets(source, &positions);
    for node in &mut nodes {
        if node.nested && node.inline_content.start >= node.inline_content.end {
            node.editable = false;
        }
        node.from = offsets[&node.bytes.start];
        node.to = offsets[&node.bytes.end];
    }
    for d in &mut diagnostics {
        let from = d["bytesFrom"].as_u64().unwrap() as usize;
        let to = d["bytesTo"].as_u64().unwrap() as usize;
        *d = json!({"code":d["code"],"from":offsets[&from],"to":offsets[&to]});
    }
    Ok(Document {
        revision: tx::revision(source),
        nodes,
        diagnostics,
    })
}

pub fn apply(source: &str, expected: &str, id: &str, patch: &Value) -> Result<Value, &'static str> {
    if expected != tx::revision(source) {
        return Err("transaction_conflict");
    }
    let document = analyze(source)?;
    if !document.diagnostics.is_empty() {
        return Err("publication_local_style_invalid");
    }
    let node = document
        .nodes
        .iter()
        .find(|n| n.id == id)
        .ok_or("publication_local_style_target")?;
    if !node.editable {
        return Err("publication_local_style_target");
    }
    let patch = patch.as_object().ok_or("publication_local_style_invalid")?;
    let mut style = node.style.clone();
    for (section, value) in patch {
        if !matches!(section.as_str(), "text" | "paragraph") {
            return Err("publication_local_style_invalid");
        }
        if value.is_null() {
            style.as_object_mut().unwrap().remove(section);
            continue;
        }
        let values = value.as_object().ok_or("publication_local_style_invalid")?;
        if !style[section].is_object() {
            style[section] = json!({});
        }
        for (key, value) in values {
            if value.is_null() {
                style[section].as_object_mut().unwrap().remove(key);
            } else {
                style[section][key] = value.clone();
            }
        }
        if style[section].as_object().unwrap().is_empty() {
            style.as_object_mut().unwrap().remove(section);
        }
    }
    validate_style(&style)?;
    validate_target(&node.kind, &style)?;
    let empty = style.as_object().unwrap().keys().all(|s| s == "id");
    let nl = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let (bytes, insert) = if let Some(range) = &node.directive {
        if empty {
            (
                if node.directive_inline {
                    range.clone()
                } else {
                    line_start(source, range.start)..line_start(source, node.bytes.start)
                },
                String::new(),
            )
        } else {
            (range.clone(), comment(&style)?)
        }
    } else if empty {
        (node.bytes.start..node.bytes.start, String::new())
    } else {
        style["id"] = json!(node.id);
        if node.nested {
            let at = node.inline_content.end;
            (at..at, comment(&style)?)
        } else {
            let at = line_start(source, node.bytes.start);
            (at..at, format!("{}{nl}{nl}", comment(&style)?))
        }
    };
    let edit = TextEdit {
        from: source[..bytes.start].encode_utf16().count(),
        to: source[..bytes.end].encode_utf16().count(),
        insert,
    };
    let transaction = ContentTransaction {
        base_revision: expected.into(),
        origin: "editor".into(),
        edits: vec![edit],
    };
    let result = tx::apply(source, &transaction)?;
    Ok(json!({"ok":true,"transaction":transaction,"result":result}))
}

/// Attribute comments are not ordinary content. Invalid ones block export
/// before committing an output and remain intact for the author's correction.
pub fn validate_source(source: &str) -> Result<(), &'static str> {
    if source.contains(PREFIX) {
        let model = analyze(source)?;
        if let Some(d) = model.diagnostics.first() {
            return Err(match d["code"].as_str() {
                Some("publication_local_style_version") => "publication_local_style_version",
                Some("publication_local_style_orphan") => "publication_local_style_orphan",
                Some("publication_local_style_duplicate") => "publication_local_style_duplicate",
                _ => "publication_local_style_invalid",
            });
        }
    }
    Ok(())
}
/// Native caption fields use the same independent author axes as inline text.
/// Existing text.size retains its older two-axis block semantics.
pub(crate) fn author_style(attributes:&Value)->Result<crate::publication_inline_text::AuthorTextStyle,&'static str>{
    attributes["text"].get("author").map(crate::publication_inline_text::AuthorTextStyle::from_value).transpose().map(|style|style.unwrap_or_default())
}
pub(crate) fn validate_author_document(doc:&crate::md_ast::Document,supported:bool)->Result<(),&'static str>{
    use crate::md_ast::{Block,Inline};
    let mut pending=doc.blocks.iter().collect::<Vec<_>>();let mut count=0;
    while let Some(block)=pending.pop(){
        if crate::speech::cancelled(){return Err("cancelled");}
        match block{
            Block::Styled{attributes,block}=>{
                if attributes["text"].get("author").is_some(){
                    count+=1;if count>MAX_NODES{return Err(crate::publication_inline_text::BUDGET);}
                    validate_style(attributes)?;
                    let Block::Caption{inlines,..}=block.unstyled()else{return Err(crate::publication_inline_text::INVALID);};
                    let author=author_style(attributes)?;
                    if !author.is_empty(){
                        if !supported{return Err(crate::publication_inline_text::UNSUPPORTED);}
                        let mut values=inlines.iter().collect::<Vec<_>>();
                        while let Some(value)=values.pop(){
                            if crate::speech::cancelled(){return Err("cancelled");}count+=1;if count>MAX_NODES{return Err(crate::publication_inline_text::BUDGET);}
                            match value{
                            Inline::Math(_)|Inline::OfficeMath{..}=>return Err(crate::publication_inline_text::UNSUPPORTED),
                            Inline::Strong(children)|Inline::Emph(children)|Inline::Strike(children)|Inline::Styled{children,..}|Inline::AuthorStyled{children,..}|Inline::Display{children,..}|Inline::Citation{children,..}|Inline::Link{children,..}=>values.extend(children),_=>(),
                        }}
                    }
                }
                pending.push(block);
            },
            Block::Quote(blocks)|Block::FootnoteDef{blocks,..}=>pending.extend(blocks),
            Block::List{items,..}=>for item in items{pending.extend(&item.blocks);},_=>(),
        }
    }Ok(())
}
pub(crate) fn validate_author_consumer(source:&str,supported:bool)->Result<(),&'static str>{
    if !source.contains(PREFIX){return Ok(());}
    validate_source(source)?;
    let model=analyze(source)?;
    if model.nodes.iter().any(|node|node.style["text"].get("author").is_some()){
        validate_author_document(&crate::md_ast::parse(source),supported)?;
    }Ok(())
}

/// Flow renderers consume the two size axes, but Word's explicit RTL run
/// behavior still has no equivalent. Check this before resource callbacks or
/// staging; native DOCX can continue using validate_author_document directly.
pub(crate) fn validate_author_render_document(doc:&crate::md_ast::Document)->Result<(),&'static str>{
    use crate::md_ast::Block;
    validate_author_document(doc,true)?;
    let mut pending=doc.blocks.iter().collect::<Vec<_>>();
    while let Some(block)=pending.pop(){
        if crate::speech::cancelled(){return Err("cancelled");}
        match block{
            Block::Styled{attributes,block}=>{
                if author_style(attributes)?.rtl==Some(true){return Err(crate::publication_inline_text::UNSUPPORTED);}
                pending.push(block);
            },
            Block::Quote(blocks)|Block::FootnoteDef{blocks,..}=>pending.extend(blocks),
            Block::List{items,..}=>for item in items{pending.extend(&item.blocks);},_=>(),
        }
    }Ok(())
}
pub(crate) fn validate_author_render_consumer(source:&str)->Result<(),&'static str>{
    if !source.contains(PREFIX){return Ok(());}
    validate_source(source)?;
    if analyze(source)?.nodes.iter().any(|node|node.style["text"].get("author").is_some()){
        validate_author_render_document(&crate::md_ast::parse(source))?;
    }Ok(())
}

#[cfg(test)]
#[path="publication_blocks_author_tests.rs"]
mod author_tests;

/// The existing reading surface receives only explicit overrides. Reading
/// typography and theme remain intact when a publication attribute is absent.
pub fn view(source: &str) -> Result<(String, Value), &'static str> {
    crate::publication_typography::validate_reader_view(source, &json!({}))?;
    let document = analyze(source)?;
    let mut output = source.to_string();
    let mut rules = serde_json::Map::new();
    for node in document.nodes.iter().rev() {
        if let Some(range) = &node.directive {
            let mut css = serde_json::Map::new();
            let text = &node.style["text"];
            let p = &node.style["paragraph"];
            if let Some(decoration) = crate::publication_typography::decoration::parse(p)? {
                css.extend(decoration.reader_css());
            }
            if let Some(value) = crate::publication_typography::baseline::local(text)? {
                css.insert("--readmd-baseline-shift".into(), json!(format!("{}pt", value.requested_pt())));
            }
            for (key, property, unit) in [
                ("size", "font-size", "pt"),
                ("trackingPt", "letter-spacing", "pt"),
                ("color", "color", ""),
                ("font", "font-family", ""),
            ] {
                if let Some(value) = text.get(key) {
                    let value = if key == "font" {
                        format!(
                            "'{}'",
                            font_family(value.as_str().unwrap()).replace(['\'', '\\'], " ")
                        )
                    } else if let Some(n) = value.as_f64() {
                        format!("{n}{unit}")
                    } else {
                        value.as_str().unwrap().into()
                    };
                    css.insert(property.into(), json!(value));
                }
            }
            for (key, property, on, off) in [
                ("bold", "font-weight", "bold", "normal"),
                ("italic", "font-style", "italic", "normal"),
            ] {
                if let Some(v) = text[key].as_bool() {
                    css.insert(property.into(), json!(if v { on } else { off }));
                }
            }
            for (key, property, unit) in [
                ("align", "text-align", ""),
                ("before", "margin-top", "pt"),
                ("after", "margin-bottom", "pt"),
                ("left", "margin-left", "mm"),
                ("right", "margin-right", "mm"),
                ("lineHeight", "line-height", ""),
            ] {
                if let Some(value) = p.get(key) {
                    css.insert(
                        property.into(),
                        json!(if let Some(n) = value.as_f64() {
                            format!("{n}{unit}")
                        } else {
                            value.as_str().unwrap().into()
                        }),
                    );
                }
            }
            if p.get("firstLine").is_some() || p.get("hanging").is_some() {
                css.insert(
                    "text-indent".into(),
                    json!(format!(
                        "{}mm",
                        p["firstLine"].as_f64().unwrap_or(0.) - p["hanging"].as_f64().unwrap_or(0.)
                    )),
                );
            }
            let key = format!(
                "{}-{}",
                node.id,
                &tx::revision(&source[range.clone()])[..12]
            );
            rules.insert(key.clone(), Value::Object(css));
            if node.directive_inline {
                output.replace_range(range.clone(), "</span>");
                output.insert_str(
                    node.inline_content.start,
                    &format!(
                        "<span data-readmd-layout=\"{key}\" data-readmd-layout-inline=\"true\">"
                    ),
                );
            } else {
                output.replace_range(
                    range.clone(),
                    &format!("<div data-readmd-layout=\"{key}\"></div>"),
                );
            }
        }
    }
    Ok((output, Value::Object(rules)))
}

pub fn validate_geometry(source: &str, style: &Value) -> Result<(), &'static str> {
    if !source.contains(PREFIX) {
        return Ok(());
    }
    let width = crate::publication_layout::Layout::from_style(style).column_width(style);
    for node in analyze(source)?.nodes {
        if node.directive.is_some()
            && width
                - node.style["paragraph"]["left"].as_f64().unwrap_or(0.)
                - node.style["paragraph"]["right"].as_f64().unwrap_or(0.)
                < 20.
        {
            return Err("publication_layout_geometry");
        }
    }
    Ok(())
}

pub fn has_overrides(source: &str) -> bool {
    source.contains(PREFIX)
        && analyze(source).is_ok_and(|d| {
            d.nodes.iter().any(|n| {
                n.directive.is_some()
                    && n.style
                        .as_object()
                        .is_some_and(|o| o.keys().any(|k| k != "id"))
            })
        })
}

pub fn attach(blocks: &mut Vec<crate::md_ast::Block>) {
    use crate::md_ast::Block;
    for block in blocks.iter_mut() {
        match block {
            Block::Quote(inner) | Block::FootnoteDef { blocks: inner, .. } => attach(inner),
            Block::List { items, .. } => {
                for item in items {
                    attach(&mut item.blocks)
                }
            }
            _ => (),
        }
        // Trailing inline directives preserve a nested paragraph's list and
        // quote membership. They are attributes, never visible export text.
        let values = match block {
            Block::Paragraph(v) | Block::Heading { inlines: v, .. } => Some(v),
            _ => None,
        };
        if let Some(values) = values {
            let index = values.iter().rposition(
                |v| matches!(v,crate::md_ast::Inline::Html(s)if s.trim().starts_with(PREFIX)),
            );
            if let Some(index) = index {
                let attributes = if let crate::md_ast::Inline::Html(s) = &values[index] {
                    read_style(s).ok().flatten()
                } else {
                    None
                };
                if let Some(attributes) = attributes {
                    values.remove(index);
                    let inner = std::mem::replace(block, Block::Html(String::new()));
                    *block = Block::Styled {
                        attributes,
                        block: Box::new(inner),
                    };
                }
            }
        }
    }
    let mut at = 0;
    while at + 1 < blocks.len() {
        let style = match &blocks[at] {
            Block::Html(s) => read_style(s).ok().flatten(),
            _ => None,
        };
        if let Some(attributes) = style {
            if matches!(
                blocks[at + 1],
                Block::Heading { .. }
                    | Block::Paragraph(_)
                    | Block::Caption { .. }
                    | Block::Math(_)
                    | Block::OfficeMath { .. }
            ) {
                let block = Box::new(blocks.remove(at + 1));
                blocks[at] = Block::Styled { attributes, block };
            }
        }
        at += 1;
    }
}

#[derive(Debug, Clone)]
pub struct Resolved {
    pub size: f64,
    pub font: String,
    pub color: String,
    pub bold: bool,
    pub italic: bool,
    pub(crate) baseline_shift: Option<crate::publication_typography::baseline::BaselineShiftPt>,
    pub align: String,
    pub before: f64,
    pub after: f64,
    pub left: f64,
    pub right: f64,
    pub first: f64,
    pub hanging: f64,
    pub line_height: f64,
    pub keep_next: bool,
    pub keep_lines: bool,
    pub widow: bool,
    pub page_break: bool,
}
impl Resolved {
    pub fn new(global: &Value, attributes: &Value, block: &crate::md_ast::Block) -> Self {
        Self::for_context(global, attributes, block, false)
    }
    pub fn for_context(global: &Value, attributes: &Value, block: &crate::md_ast::Block, in_note: bool) -> Self {
        Self::for_context_with_lines(global, attributes, block, in_note, crate::publication_typography::LineContext::default())
    }
    pub(crate) fn for_context_with_lines(global: &Value, attributes: &Value, block: &crate::md_ast::Block, in_note: bool, lines: crate::publication_typography::LineContext) -> Self {
        use crate::md_ast::Block;
        let body = &global["typography"];
        let heading = if let Block::Heading { level, .. } = block {
            Some(&global["headings"][format!("h{level}")])
        } else {
            None
        };
        let caption = matches!(block, Block::Caption { .. });
        let math = matches!(block, Block::Math(_) | Block::OfficeMath { .. });
        let text = &attributes["text"];
        let p = &attributes["paragraph"];
        let base = heading.unwrap_or(body);
        let number = |v: &Value, k: &str, d: f64| v[k].as_f64().unwrap_or(d);
        Self {
            size: number(
                text,
                "size",
                if caption {
                    10.
                } else {
                    if in_note && heading.is_none() { number(&global["footnotes"], "size", 9.) } else { number(base, "size", 11.) }
                },
            ),
            font: text["font"]
                .as_str()
                .or_else(|| body["font"].as_str())
                .unwrap_or("MicrosoftYaHei")
                .into(),
            color: text["color"]
                .as_str()
                .or_else(|| base["color"].as_str())
                .unwrap_or("#262626")
                .into(),
            bold: text["bold"]
                .as_bool()
                .unwrap_or(heading.is_some() && base["bold"].as_bool().unwrap_or(true)),
            italic: text["italic"].as_bool().unwrap_or(false),
            baseline_shift: crate::publication_typography::baseline::resolve(
                crate::publication_typography::baseline::global(global).ok().flatten(), text,
                !in_note && matches!(block, Block::Paragraph(_))).ok().flatten(),
            align: p["align"]
                .as_str()
                .unwrap_or(if caption || math {
                    "center"
                } else {
                    base["align"].as_str().unwrap_or("left")
                })
                .into(),
            before: number(
                p,
                "before",
                if caption {
                    4.
                } else {
                    number(base, "before", 0.)
                },
            ),
            after: number(
                p,
                "after",
                if caption {
                    8.
                } else {
                    heading.map_or(if in_note { 0. } else { number(body, "spacing", 6.) }, |h| number(h, "after", 8.))
                },
            ),
            left: number(p, "left", 0.),
            right: number(p, "right", 0.),
            first: number(
                p,
                "firstLine",
                if in_note || heading.is_some() || caption || math || number(p, "hanging", 0.) > 0. {
                    0.
                } else {
                    number(body, "firstLineIndent", 0.)
                },
            ),
            hanging: number(p, "hanging", 0.),
            line_height: number(p, "lineHeight", if in_note { number(&global["footnotes"], "lineHeight", 1.2) } else { number(body, "lineHeight", lines.legacy_multiple()) }),
            keep_next: p["keepNext"]
                .as_bool()
                .unwrap_or(heading.is_some() && base["keepNext"].as_bool().unwrap_or(true)),
            keep_lines: p["keepLines"].as_bool().unwrap_or(if heading.is_some() {
                base["keepLines"].as_bool().unwrap_or(true)
            } else {
                if in_note { crate::publication_layout::Layout::from_style(global).note_keep_paragraphs }
                else { body["keepLines"].as_bool().unwrap_or(false) }
            }),
            widow: p["widowControl"]
                .as_bool()
                .unwrap_or(body["widowControl"].as_bool().unwrap_or(true)),
            page_break: p["pageBreakBefore"]
                .as_bool()
                .unwrap_or(heading.is_some() && base["pageBreakBefore"].as_bool().unwrap_or(false)),
        }
    }
    pub fn css(&self) -> String {
        format!("font-family:'{}';font-size:{}pt;color:{};font-weight:{};font-style:{};text-align:{};line-height:{};margin:{}pt {}mm {}pt {}mm;text-indent:{}mm;break-before:{};break-after:{};break-inside:{};orphans:{};widows:{};",font_family(&self.font).replace(['\'','\\']," "),self.size,self.color,if self.bold{"bold"}else{"normal"},if self.italic{"italic"}else{"normal"},self.align,self.line_height,self.before,self.right,self.after,self.left,self.first-self.hanging,if self.page_break{"page"}else{"auto"},if self.keep_next{"avoid"}else{"auto"},if self.keep_lines{"avoid"}else{"auto"},if self.widow{2}else{1},if self.widow{2}else{1})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_paragraphs_are_located_by_range_not_first_string_match() {
        let s = "# Title\n\nSame paragraph.\n\nSame paragraph.\n";
        let doc = analyze(s).unwrap();
        let id = &doc.nodes[2].id;
        let r = apply(
            s,
            &doc.revision,
            id,
            &json!({"paragraph":{"align":"right"},"text":{"size":14}}),
        )
        .unwrap();
        let out = r["result"]["content"].as_str().unwrap();
        assert!(out.starts_with("# Title\n\nSame paragraph.\n\n<!-- readmd:layout"));
        let parsed = analyze(out).unwrap();
        assert_eq!(parsed.nodes[2].id, *id);
        assert_eq!(parsed.nodes[2].style["text"]["size"], 14);
        assert!(parsed.nodes[1].style.as_object().unwrap().is_empty());
        let back = tx::apply(
            out,
            &ContentTransaction {
                base_revision: tx::revision(out),
                origin: "editor".into(),
                edits: serde_json::from_value(r["result"]["inverse"].clone()).unwrap(),
            },
        )
        .unwrap();
        assert_eq!(back.content, s);
    }
    #[test]
    fn repeated_footnote_paragraphs_keep_source_identity_crlf_and_reset() {
        let source = "\u{feff}Body[^n].\r\n\r\n[^n]: Repeated 🚀 evidence.\r\n\r\n    Repeated 🚀 evidence.\r\n";
        let model = analyze(source).unwrap();
        let nodes: Vec<_> = model.nodes.iter().filter(|n| n.title.contains("Repeated")).collect();
        assert_eq!(nodes.len(), 2);
        assert!(nodes.iter().all(|n| n.editable && n.nested));
        let result = apply(source, &model.revision, &nodes[1].id, &json!({"text":{"size":17},"paragraph":{"align":"right"}})).unwrap();
        let edited = result["result"]["content"].as_str().unwrap();
        assert!(edited.contains("[^n]: Repeated 🚀 evidence.\r\n\r\n    Repeated 🚀 evidence.<!-- readmd:layout"));
        validate_source(edited).unwrap();
        assert_eq!(apply(edited, &model.revision, &nodes[1].id, &json!({})).unwrap_err(), "transaction_conflict");
        let current = analyze(edited).unwrap();
        let selected = current.nodes.iter().find(|n| n.style["text"]["size"] == 17).unwrap();
        let reset = apply(edited, &current.revision, &selected.id, &json!({"text":null,"paragraph":null})).unwrap();
        assert_eq!(reset["result"]["content"], source);
        let context = Resolved::for_context(&json!({"typography":{"size":12,"lineHeight":1.8,"firstLineIndent":5,"spacing":8},"footnotes":{"size":8,"lineHeight":1.2}}), &json!({"paragraph":{"align":"right"}}), &crate::md_ast::Block::Paragraph(vec![]), true);
        assert_eq!((context.size, context.line_height, context.first, context.after), (8.,1.2,0.,0.));
    }

    #[test]
    fn bom_crlf_and_surrogates_keep_exact_ranges_and_roundtrip() {
        let s = "\u{feff}# 𝛂\r\n\r\n甲🙂 beta\r\n\r\nTail.\r\n";
        let d = analyze(s).unwrap();
        let n = &d.nodes[1];
        assert_eq!(
            &s[tx::byte_offset(s, n.from).unwrap()..tx::byte_offset(s, n.to).unwrap()],
            &s[n.bytes.clone()]
        );
        let r = apply(s, &d.revision, &n.id, &json!({"text":{"bold":true}})).unwrap();
        let out = r["result"]["content"].as_str().unwrap();
        assert!(!out.replace("\r\n", "").contains('\n'));
        let d = analyze(out).unwrap();
        let reset = apply(out, &d.revision, &d.nodes[1].id, &json!({"text":null})).unwrap();
        assert_eq!(reset["result"]["content"], s);
    }
    #[test]
    fn conflict_and_unbounded_or_unknown_properties_are_rejected() {
        let s = "Text\n";
        let d = analyze(s).unwrap();
        assert_eq!(
            apply(s, "stale", &d.nodes[0].id, &json!({})).unwrap_err(),
            "transaction_conflict"
        );
        for bad in [
            json!({"text":{"size":999}}),
            json!({"page":{"columns":2}}),
            json!({"paragraph":{"firstLine":2,"hanging":3}}),
            json!({"text":{"font":"A\nB"}}),
        ] {
            assert!(apply(s, &d.revision, &d.nodes[0].id, &bad).is_err());
        }
    }
    #[test]
    fn code_examples_do_not_become_attributes_and_orphans_are_diagnosed() {
        let s = "```html\n<!-- readmd:layout v1 {} -->\n```\n\nText.\n";
        assert!(analyze(s).unwrap().diagnostics.is_empty());
        validate_source(s).unwrap();
        for s in [
            "<!-- readmd:layout v2 {} -->\n\nText.\n",
            "<!-- readmd:layout v1 {} -->\n\n```js\nx\n```\n",
        ] {
            assert_eq!(analyze(s).unwrap().diagnostics.len(), 1);
            assert!(validate_source(s).is_err());
        }
    }
    #[test]
    fn merge_is_idempotent_reset_removes_only_opted_in_metadata() {
        let s = "One.\n\nTwo.\n";
        let d = analyze(s).unwrap();
        let patch = json!({"paragraph":{"left":8,"align":"center"}});
        let r = apply(s, &d.revision, &d.nodes[1].id, &patch).unwrap();
        let out = r["result"]["content"].as_str().unwrap();
        let d = analyze(out).unwrap();
        let same = apply(out, &d.revision, &d.nodes[1].id, &patch).unwrap();
        assert_eq!(same["result"]["changed"], false);
        let reset = apply(out, &d.revision, &d.nodes[1].id, &json!({"paragraph":null})).unwrap();
        assert_eq!(reset["result"]["content"], s);
    }
    #[test]
    fn nested_paragraphs_headings_and_notes_are_editable_but_math_remains_explicit() {
        let d = analyze("> A quote.\n\n- A list item.\n\n> ## A heading\n\n> ```readmd-math v1\n> a / b\n> ```\n\n[^n]: A footnote.\n").unwrap();
        assert!(d
            .nodes
            .iter()
            .filter(|n| n.kind != "math")
            .all(|n| n.editable && n.nested));
        assert!(d
            .nodes
            .iter()
            .filter(|n| n.kind == "math")
            .all(|n| !n.editable));
    }
    fn without_styles(blocks: &mut [crate::md_ast::Block]) {
        use crate::md_ast::Block;
        fn canonical_inlines(values: &mut Vec<crate::md_ast::Inline>) {
            use crate::md_ast::Inline;
            // pulldown-cmark may emit an empty text token before closing
            // heading hashes. It has no content or formatting semantics.
            values.retain(|v| !matches!(v,Inline::Text(s) if s.is_empty()));
            for value in values {
                match value {
                    Inline::Strong(v) | Inline::Emph(v) | Inline::Strike(v)
                        | Inline::Styled { children: v, .. } | Inline::Display { children: v, .. } | Inline::Citation { children: v, .. } => canonical_inlines(v),
                    Inline::Link { children, .. } => canonical_inlines(children),
                    _ => (),
                }
            }
        }
        for block in blocks {
            while matches!(block, Block::Styled { .. }) {
                let old = std::mem::replace(block, Block::Html(String::new()));
                if let Block::Styled { block: inner, .. } = old {
                    *block = *inner;
                }
            }
            match block {
                Block::Paragraph(v) | Block::Heading { inlines: v, .. } => canonical_inlines(v),
                Block::Quote(v) | Block::FootnoteDef { blocks: v, .. } => without_styles(v),
                Block::List { items, .. } => {
                    for item in items {
                        without_styles(&mut item.blocks)
                    }
                }
                _ => (),
            }
        }
    }
    #[test]
    fn nested_inline_layout_preserves_tight_lists_prefixes_heading_ids_and_exact_reset() {
        let source="\u{feff}> ## Nested **heading** ## {#anchor}\r\n>\r\n> repeated🙂\r\n>\r\n> - repeated🙂\r\n>   - inner [link](#anchor)\r\n>\r\n> repeated🙂\r\n\r\n3. ordered text\r\n4. next text\r\n\r\n- [x] Task **done**\r\n";
        let document = analyze(source).unwrap();
        let mut original = crate::md_ast::parse(source);
        without_styles(&mut original.blocks);
        assert!(document.nodes.len() >= 8);
        for node in document.nodes.iter().filter(|n| n.editable) {
            assert!(node.nested);
            let result = apply(
                source,
                &document.revision,
                &node.id,
                &json!({"text":{"size":17,"color":"#234567"},"paragraph":{"align":"right"}}),
            )
            .unwrap();
            let updated = result["result"]["content"].as_str().unwrap();
            assert_eq!(updated.matches(PREFIX).count(), 1);
            assert!(!updated.replace("\r\n", "").contains('\n'));
            let parsed = analyze(updated).unwrap();
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let changed = parsed.nodes.iter().find(|n| n.id == node.id).unwrap();
            assert!(changed.directive_inline);
            assert_eq!(changed.title, node.title);
            assert_eq!(changed.style["text"]["size"], 17);
            let mut updated_ast = crate::md_ast::parse(updated);
            without_styles(&mut updated_ast.blocks);
            assert_eq!(updated_ast, original, "{}", node.title);
            let reset = apply(
                updated,
                &parsed.revision,
                &node.id,
                &json!({"text":null,"paragraph":null}),
            )
            .unwrap();
            assert_eq!(reset["result"]["content"], source);
            assert_eq!(
                apply(
                    updated,
                    &parsed.revision,
                    &node.id,
                    &json!({"text":{"size":17,"color":"#234567"},"paragraph":{"align":"right"}})
                )
                .unwrap()["result"]["changed"],
                false
            );
        }
    }
    #[test]
    fn nested_directives_cannot_escape_their_paragraph_or_style_code_examples() {
        let source="> SAME\n>\n> SAME\n\n`<!-- readmd:layout v1 {} -->`\n\n```markdown\n- literal<!-- readmd:layout v1 {} -->\n```\n";
        let d = analyze(source).unwrap();
        assert!(d.diagnostics.is_empty());
        let n = &d.nodes[1];
        let result = apply(source, &d.revision, &n.id, &json!({"text":{"bold":true}})).unwrap();
        let updated = result["result"]["content"].as_str().unwrap();
        assert!(updated.starts_with("> SAME\n>\n> SAME<!-- readmd:layout "));
        let (view, rules) = view(updated).unwrap();
        assert_eq!(rules.as_object().unwrap().len(), 1);
        assert!(view.starts_with("> SAME\n>\n> <span data-readmd-layout="));
        assert!(view.contains("`<!-- readmd:layout v1 {} -->`"));
        let bad = "> Prefix<!-- readmd:layout v1 {} --> suffix\n";
        assert_eq!(
            analyze(bad).unwrap().diagnostics[0]["code"],
            "publication_local_style_orphan"
        );
        assert_eq!(
            apply(updated, "stale", &n.id, &json!({})).unwrap_err(),
            "transaction_conflict"
        );
    }
    #[test]
    fn nested_layout_reaches_native_word_epub_and_typst_without_changing_list_structure() {
        let source="3. First\n4. Selected **strong** [link](https://example.invalid)\n\n> Quoted paragraph\n";
        let model = analyze(source).unwrap();
        let selected = model
            .nodes
            .iter()
            .find(|n| n.title.starts_with("Selected"))
            .unwrap();
        let result = apply(
            source,
            &model.revision,
            &selected.id,
            &json!({"text":{"size":18,"color":"#234567"},"paragraph":{"align":"right"}}),
        )
        .unwrap();
        let updated = result["result"]["content"].as_str().unwrap();
        let doc = crate::md_ast::parse(updated);
        let style = crate::export_styles::sanitize(&json!({}));
        let word = crate::docx_writer::build(&doc, &style, "nested", &|_| None);
        assert!(word.warns.is_empty(), "{:?}", word.warns);
        let xml = String::from_utf8(
            word.parts
                .iter()
                .find(|(p, _)| p == "word/document.xml")
                .unwrap()
                .1
                .clone(),
        )
        .unwrap();
        let paragraph = xml.split("<w:p>").find(|s| s.contains("Selected")).unwrap();
        assert!(paragraph.contains("w:sz w:val=\"36\""));
        assert!(paragraph.contains("w:color w:val=\"234567\""));
        assert!(paragraph.contains("w:numPr"));
        assert!(paragraph.contains("w:hyperlink"));
        assert!(!xml.contains(PREFIX));
        let epub =
            crate::epub_writer::render_chapters_with_options(&doc, "h1", &mut |_| None, &style);
        assert!(epub[0].body.contains("<ol"));
        assert!(epub[0].body.contains("font-size:18pt"));
        assert!(!epub[0].body.contains(PREFIX));
        let directory = tempfile::tempdir().unwrap();
        let typst =
            crate::typst_writer::render(&doc, &style, "", directory.path(), "nested").unwrap();
        assert!(typst.text.contains("#enum(start:3,"));
        assert!(typst.text.contains("size:18pt"));
        assert!(!typst.text.contains(PREFIX));
    }
    #[test]
    fn duplicate_persistent_ids_block_ambiguous_targets() {
        let s="<!-- readmd:layout v1 {\"id\":\"same\"} -->\n\nOne.\n\n<!-- readmd:layout v1 {\"id\":\"same\"} -->\n\nTwo.\n";
        assert_eq!(
            analyze(s).unwrap().diagnostics[0]["code"],
            "publication_local_style_duplicate"
        );
    }
    #[test]
    fn reader_projection_is_partial_and_keeps_revision_specific_styles() {
        let s = "  Indented paragraph.\n";
        let d = analyze(s).unwrap();
        let applied = apply(
            s,
            &d.revision,
            &d.nodes[0].id,
            &json!({"paragraph":{"align":"right"}}),
        )
        .unwrap();
        let out = applied["result"]["content"].as_str().unwrap();
        let (view, rules) = view(out).unwrap();
        assert!(view.contains("<div data-readmd-layout="));
        assert!(!view.contains(PREFIX));
        let rule = rules.as_object().unwrap().values().next().unwrap();
        assert_eq!(rule, &json!({"text-align":"right"}));
        let d = analyze(out).unwrap();
        let reset = apply(out, &d.revision, &d.nodes[0].id, &json!({"paragraph":null})).unwrap();
        assert_eq!(reset["result"]["content"], s);
    }
    #[test]
    fn styled_blocks_keep_native_word_attributes_content_links_and_book_structure() {
        let source="<!-- readmd:layout v1 {\"text\":{\"size\":20},\"paragraph\":{\"align\":\"right\"}} -->\n\n# Heading {#stable}\n\n<!-- readmd:layout v1 {\"text\":{\"size\":16,\"color\":\"#345678\",\"font\":\"Arial\"},\"paragraph\":{\"left\":10,\"pageBreakBefore\":true}} -->\n\nLOCAL **strong** [link](#stable)[^n]\n\nUNCHANGED\n\n[^n]: Note.\n";
        validate_source(source).unwrap();
        let doc = crate::md_ast::parse(source);
        let style = crate::export_styles::sanitize(&json!({}));
        assert_eq!(
            crate::docx_writer::doc_title(&doc, &style, "fallback"),
            "Heading"
        );
        let pkg = crate::docx_writer::build(&doc, &style, "test", &|_| None);
        assert!(pkg.warns.is_empty(), "{:?}", pkg.warns);
        let xml = String::from_utf8(
            pkg.parts
                .iter()
                .find(|(p, _)| p == "word/document.xml")
                .unwrap()
                .1
                .clone(),
        )
        .unwrap();
        let local = xml.split("<w:p>").find(|p| p.contains("LOCAL")).unwrap();
        assert!(local.contains("w:sz w:val=\"32\""));
        assert!(local.contains("w:color w:val=\"345678\""));
        assert!(local.contains("w:left=\"567\""));
        assert!(local.contains("w:pageBreakBefore w:val=\"1\""));
        assert!(local.contains("w:hyperlink"));
        assert!(local.contains("w:footnoteReference"));
        assert!(!xml
            .split("<w:p>")
            .find(|p| p.contains("UNCHANGED"))
            .unwrap()
            .contains("w:sz w:val=\"32\""));
        let mut images = |_: &str| None;
        let chapters =
            crate::epub_writer::render_chapters_with_options(&doc, "h1", &mut images, &style);
        assert_eq!(chapters.len(), 1);
        assert_eq!(chapters[0].title, "Heading");
        assert!(chapters[0].body.contains("font-size:16pt"));
        assert!(chapters[0].body.contains("margin:0pt 0mm 6pt 10mm"));
        assert!(chapters[0]
            .body
            .contains("href=\"chapter_1.xhtml#h-stable\""));
        let dir = tempfile::tempdir().unwrap();
        let rendered = crate::typst_writer::render(&doc, &style, "", dir.path(), "test").unwrap();
        assert!(rendered.text.contains("size:16pt"));
        assert!(rendered.text.contains("#pad(left:10mm"));
        assert!(rendered.text.contains("#pagebreak(weak:true)"));
    }
    #[test]
    fn invalid_layout_never_replaces_previous_output() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("prior.docx");
        std::fs::write(&target, b"previous").unwrap();
        let source = "<!-- readmd:layout v9 {} -->\n\nText\n";
        assert_eq!(
            crate::mdexport::export_docx(source, "", target.to_str().unwrap(), &json!({}), "test")
                .unwrap_err(),
            "publication_local_style_version"
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"previous");
        let source =
            "<!-- readmd:layout v1 {\"paragraph\":{\"left\":60,\"right\":60}} -->\n\nText\n";
        assert_eq!(
            validate_geometry(
                source,
                &crate::export_styles::sanitize(&json!({"page":{"size":"A5"}}))
            )
            .unwrap_err(),
            "publication_layout_geometry"
        );
    }
    #[test]
    fn equation_and_caption_layout_keep_semantics_and_reject_inapplicable_properties() {
        let source="```readmd-math v1\nfrac(a+b,c)\n```\n\n:label[eq:estimate] Estimate\n\nSee :ref[eq:estimate].\n";
        let d = analyze(source).unwrap();
        assert_eq!(d.nodes[0].kind, "math");
        assert_eq!(
            apply(
                source,
                &d.revision,
                &d.nodes[0].id,
                &json!({"text":{"font":"Arial"}})
            )
            .unwrap_err(),
            "publication_local_style_invalid"
        );
        let change = apply(
            source,
            &d.revision,
            &d.nodes[0].id,
            &json!({"text":{"size":20,"color":"#123456"},"paragraph":{"align":"left"}}),
        )
        .unwrap();
        let source = change["result"]["content"].as_str().unwrap();
        validate_source(source).unwrap();
        let prepared = crate::publication::prepare(source, "").unwrap();
        let doc = crate::md_ast::parse(&prepared.content);
        assert!(matches!(doc.blocks[0], crate::md_ast::Block::Styled { .. }));
        let style = crate::export_styles::sanitize(&json!({}));
        let word = crate::docx_writer::build(&doc, &style, "test", &|_| None);
        assert!(word.warns.is_empty(), "{:?}", word.warns);
        let xml = String::from_utf8(
            word.parts
                .iter()
                .find(|(p, _)| p == "word/document.xml")
                .unwrap()
                .1
                .clone(),
        )
        .unwrap();
        assert!(xml.contains("m:oMath"));
        assert!(xml.contains("w:sz w:val=\"40\""));
        assert!(xml.contains("w:color w:val=\"123456\""));
        assert!(xml.contains("Estimate"));
        let mut images = |_: &str| None;
        let book =
            crate::epub_writer::render_chapters_with_options(&doc, "h1", &mut images, &style);
        assert!(book[0].has_math);
        assert!(book[0].body.contains("<math"));
        assert!(book[0].body.contains("font-size:20pt"));
    }
}
