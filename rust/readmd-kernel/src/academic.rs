//! One offline CSL processor for desktop, export, MCP and extension clients.
//! Bibliographies and styles are supplied as content, never arbitrary host paths.
use crate::{ApiError, ApiResult};
use hayagriva::citationberg::{taxonomy::Locator, LocaleCode, Style};
use hayagriva::{
    BibliographyDriver, BibliographyRequest, CitationItem, CitationRequest, ElemChild,
    ElemChildren, Formatted, LocatorPayload, SpecificLocator,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::str::FromStr;
use crate::publication_rich::{self, RichNode};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CiteMode { #[default] Normal, SuppressAuthor, AuthorOnly, Prose }

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct Cite {
    pub key: String,
    pub locator: Option<String>,
    pub kind: Option<String>,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    pub mode: CiteMode,
    pub prefix_rich: Option<Vec<RichNode>>,
    pub suffix_rich: Option<Vec<RichNode>>,
}
fn context_error(field: &str) -> ApiError {
    invalid("academic_context").noted("field", field)
}
impl Cite {
    pub fn new(key: impl Into<String>) -> Self { Self { key: key.into(), ..Default::default() } }
    fn from_value(value: &Value) -> ApiResult<Self> {
        let object = value.as_object().ok_or_else(|| context_error("cite"))?;
        const FIELDS: &[&str] = &["key", "id", "locator", "kind", "label", "prefix", "suffix", "mode", "prefix_rich", "suffix_rich", "suppress_author", "suppress-author", "author_only", "author-only"];
        if let Some(key) = object.keys().find(|key| !FIELDS.contains(&key.as_str())) { return Err(context_error(key)); }
        fn string(value: &Value, name: &str) -> ApiResult<Option<String>> {
            match value.get(name) { None | Some(Value::Null) => Ok(None), Some(Value::String(text)) => Ok(Some(text.clone())), _ => Err(context_error(name)) }
        }
        fn alias_string(value: &Value, first: &str, second: &str) -> ApiResult<Option<String>> {
            let a = string(value, first)?; let b = string(value, second)?;
            if a.is_some() && b.is_some() && a != b { return Err(context_error(second)); }
            Ok(a.or(b))
        }
        fn flag(value: &Value, first: &str, second: &str) -> ApiResult<Option<bool>> {
            let get = |name: &str| match value.get(name) { None => Ok(None), Some(Value::Bool(flag)) => Ok(Some(*flag)), _ => Err(context_error(name)) };
            let a = get(first)?; let b = get(second)?;
            if a.is_some() && b.is_some() && a != b { return Err(context_error(second)); }
            Ok(a.or(b))
        }
        let suppress = flag(value, "suppress_author", "suppress-author")?;
        let author = flag(value, "author_only", "author-only")?;
        if suppress == Some(true) && author == Some(true) { return Err(context_error("author_only")); }
        let mode = match value.get("mode") {
            None => if suppress == Some(true) { CiteMode::SuppressAuthor } else if author == Some(true) { CiteMode::AuthorOnly } else { CiteMode::Normal },
            Some(mode) => serde_json::from_value::<CiteMode>(mode.clone()).map_err(|_| context_error("mode"))?,
        };
        if suppress.is_some_and(|flag| flag != (mode == CiteMode::SuppressAuthor)) { return Err(context_error("suppress_author")); }
        if author.is_some_and(|flag| flag != (mode == CiteMode::AuthorOnly)) { return Err(context_error("author_only")); }
        let rich = |field: &str| -> ApiResult<Option<Vec<RichNode>>> {
            match value.get(field) {
                None | Some(Value::Null) => Ok(None),
                Some(nodes) => { let nodes:Vec<RichNode> = serde_json::from_value(nodes.clone()).map_err(|_| context_error(field))?; publication_rich::validate(&nodes).map_err(|error| error.noted("field",field))?; Ok(Some(nodes)) }
            }
        };
        let result = Self {
            key: alias_string(value, "key", "id")?.ok_or_else(|| context_error("key"))?,
            locator: string(value, "locator")?, kind: alias_string(value, "kind", "label")?,
            prefix: string(value, "prefix")?, suffix: string(value, "suffix")?, mode,
            prefix_rich: rich("prefix_rich")?, suffix_rich: rich("suffix_rich")?,
        };
        result.validate()?; Ok(result)
    }
    pub fn validate(&self) -> ApiResult<()> {
        if self.key.is_empty() || self.key.len() > 1024 { return Err(context_error("key")); }
        for (field, value) in [("prefix", &self.prefix), ("suffix", &self.suffix)] {
            if value.as_ref().is_some_and(|value| value.len() > 4096) { return Err(context_error(field)); }
        }
        locator(self)?;
        if let Some(nodes) = &self.prefix_rich { publication_rich::to_elements(nodes).map_err(|error|error.noted("field","prefix_rich"))?; }
        if let Some(nodes) = &self.suffix_rich { publication_rich::to_elements(nodes).map_err(|error|error.noted("field","suffix_rich"))?; }
        Ok(())
    }
    pub fn prefix_nodes(&self) -> Vec<RichNode> { self.prefix_rich.clone().unwrap_or_else(|| self.prefix.as_ref().filter(|s| !s.is_empty()).map(|s| vec![RichNode::text(s)]).unwrap_or_default()) }
    pub fn suffix_nodes(&self) -> Vec<RichNode> { self.suffix_rich.clone().unwrap_or_else(|| self.suffix.as_ref().filter(|s| !s.is_empty()).map(|s| vec![RichNode::text(s)]).unwrap_or_default()) }
}
impl<'de> Deserialize<'de> for Cite {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_value(&Value::deserialize(deserializer)?).map_err(|error| serde::de::Error::custom(error.code))
    }
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct CiteContext {
    pub note_number: Option<usize>,
    pub in_note: bool,
    pub note_label: Option<String>,
}
#[derive(Deserialize)]
pub struct Request {
    pub format: String,
    pub data: String,
    #[serde(default = "default_style")]
    pub style: String,
    #[serde(default)]
    pub csl: Option<String>,
    #[serde(default = "default_locale")]
    pub locale: String,
    /// Explicit offline locale data for the requested canonical language tag.
    #[serde(default)]
    pub locale_xml: Option<String>,
    pub clusters: Vec<Vec<Cite>>,
    /// Distinct pre-existing document notes before each citation cluster.
    #[serde(default)]
    pub preceding_notes: Vec<usize>,
    /// Real reading-order note identity. An empty vector retains the legacy
    /// preceding_notes contract; supplied contexts must parallel clusters.
    #[serde(default)]
    pub contexts: Vec<CiteContext>,
}

/// HTTP adapters use this entry to retain exact field errors before serde's
/// legacy invalid-json envelope would erase an item's validation location.
pub fn parse_request(value: Value) -> ApiResult<Request> {
    if let Some(clusters) = value.get("clusters").and_then(Value::as_array) {
        for (cluster, items) in clusters.iter().enumerate() {
            let items = items.as_array().ok_or_else(|| context_error(&format!("clusters[{cluster}]")))?;
            for (item, cite) in items.iter().enumerate() {
                Cite::from_value(cite).map_err(|error| {
                    let field = match &error.shape { crate::error::ErrorShape::ApiCode { extras } => extras.iter().find(|(key, _)| key == "field").and_then(|(_, v)| v.as_str()).unwrap_or("cite"), _ => "cite" };
                    let path=format!("clusters[{cluster}][{item}].{field}");error.noted("field",path)
                })?;
            }
        }
    }
    serde_json::from_value(value).map_err(|_| ApiError::bad_request("invalid_json"))
}
fn default_style() -> String {
    "american-psychological-association".into()
}
fn default_locale() -> String {
    "en-US".into()
}
fn invalid(code: &'static str) -> ApiError {
    ApiError::new(422, code)
}
fn escape_markdown(text: &str) -> String {
    let mut s = String::new();
    for c in text.chars() {
        // Escaped brackets/parentheses are TeX delimiters in our renderer.
        // Entities preserve literal CSL punctuation without creating math.
        if c == '[' { s.push_str("&#91;"); continue; }
        if c == ']' { s.push_str("&#93;"); continue; }
        if "\\`*_{}<>!#|$".contains(c) {
            s.push('\\');
        }
        s.push(if matches!(c, '\r' | '\n') { ' ' } else { c });
    }
    s
}
fn rich(children: &ElemChildren) -> (String, String, bool) {
    use hayagriva::citationberg::{
        FontStyle, FontVariant, FontWeight, TextDecoration, VerticalAlign,
    };
    fn text(t: &Formatted) -> (String, String, bool) {
        fn surround(value: String, marker: &str) -> String {
            let middle = value.trim();
            if middle.is_empty() { return value; }
            let start = value.len() - value.trim_start().len();
            let end = value.trim_end().len();
            format!("{}{marker}{}{marker}{}", &value[..start], &value[start..end], &value[end..])
        }
        let f = t.formatting;
        let mut css = String::new();
        let mut markdown = escape_markdown(&t.text);
        if f.font_style != FontStyle::Normal {
            css.push_str("font-style:italic;");
            markdown = surround(markdown, "*");
        }
        if f.font_weight == FontWeight::Bold {
            css.push_str("font-weight:bold;");
            markdown = surround(markdown, "**");
        }
        if f.font_weight == FontWeight::Light {
            css.push_str("font-weight:300;");
        }
        if f.font_variant == FontVariant::SmallCaps {
            css.push_str("font-variant:small-caps;");
        }
        if f.text_decoration == TextDecoration::Underline {
            css.push_str("text-decoration:underline;");
        }
        if f.vertical_align == VerticalAlign::Sup {
            css.push_str("vertical-align:super;font-size:smaller;");
        }
        if f.vertical_align == VerticalAlign::Sub {
            css.push_str("vertical-align:sub;font-size:smaller;");
        }
        let html = crate::mdexport::html_escape(&t.text, false);
        let html = if css.is_empty() {
            html
        } else {
            format!("<span style=\"{css}\">{html}</span>")
        };
        let exact = f.font_variant == FontVariant::Normal
            && f.text_decoration == TextDecoration::None
            && matches!(f.vertical_align, VerticalAlign::None | VerticalAlign::Baseline)
            && f.font_weight != FontWeight::Light;
        (html, markdown, exact)
    }
    let (mut html, mut markdown, mut exact) = (String::new(), String::new(), true);
    for child in &children.0 {
        let (h, m, e) = match child {
            ElemChild::Text(t) => text(t),
            ElemChild::Elem(e) => {
                let (html, markdown, exact) = rich(&e.children);
                // The compatibility Markdown field cannot carry recursive
                // CSL display. Production writers consume rich/layout instead.
                (html, markdown, exact && e.display.is_none())
            },
            ElemChild::Link { text: t, url } => {
                let (h, m, e) = text(t);
                if url.starts_with("https://") || url.starts_with("http://") {
                    let safe: String = url.chars().map(|c| {
                        if c.is_ascii_control() || " ()<>\\\"'`".contains(c) {
                            format!("%{:02X}", c as u32)
                        } else { c.to_string() }
                    }).collect();
                    (
                        format!(
                            "<a href=\"{}\">{h}</a>",
                            crate::mdexport::html_escape(url, true)
                        ),
                        format!("[{m}]({safe})"),
                        e,
                    )
                } else {
                    (h, m, e)
                }
            }
            ElemChild::Markup(m) => (
                crate::mdexport::html_escape(m, false),
                escape_markdown(m),
                false,
            ),
            ElemChild::Transparent { .. } => (String::new(), String::new(), false),
        };
        html.push_str(&h);
        markdown.push_str(&m);
        exact &= e;
    }
    (html, markdown, exact)
}
fn locator(c: &Cite) -> ApiResult<Option<SpecificLocator<'_>>> {
    let kind = Locator::from_str(c.kind.as_deref().unwrap_or("page")).map_err(|_| context_error("kind"))?;
    let Some(value) = c.locator.as_deref() else {
        return Ok(None);
    };
    if value.is_empty() || value.len() > 512 {
        return Err(invalid("academic_locator"));
    }
    Ok(Some(SpecificLocator(kind, LocatorPayload::Str(value))))
}
fn prose_parts(children: &ElemChildren) -> ApiResult<(Vec<RichNode>, Vec<RichNode>)> {
    let (mut author, mut remainder) = (Vec::new(), Vec::new());
    for child in &children.0 {
        if let ElemChild::Elem(elem) = child {
            match elem.meta.as_ref() {
                Some(hayagriva::ElemMeta::ProseAuthor) => author.extend(publication_rich::from_elements(&elem.children)?),
                Some(hayagriva::ElemMeta::ProseRemainder) => remainder.extend(publication_rich::from_elements(&elem.children)?),
                _ => { let (a, r) = prose_parts(&elem.children)?; author.extend(a); remainder.extend(r); }
            }
        }
    }
    Ok((author,remainder))
}
fn note_prose_parts(children: &ElemChildren, cites: &[Cite], delimiter: &str) -> ApiResult<(Vec<RichNode>, Vec<RichNode>)> {
    fn entries<'a>(children: &'a ElemChildren, out: &mut Vec<(usize,&'a ElemChildren)>) {
        for child in &children.0 { if let ElemChild::Elem(elem)=child {
            if let Some(hayagriva::ElemMeta::Entry(index))=&elem.meta { out.push((*index,&elem.children)); }
            else { entries(&elem.children,out); }
        }}
    }
    fn append(out: &mut Vec<RichNode>, nodes: Vec<RichNode>, delimiter: &str) {
        if nodes.is_empty() { return; }
        if !out.is_empty() && !delimiter.is_empty() { out.push(RichNode::text(delimiter)); }
        out.extend(nodes);
    }
    let mut parts=Vec::new();entries(children,&mut parts);
    let (mut inline,mut note)=(Vec::new(),Vec::new());
    for(index,elements)in parts {
        let cite=cites.get(index).ok_or_else(||invalid("academic_source_range"))?;
        match cite.mode {
            CiteMode::Prose=>{
                let(author,mut remainder)=prose_parts(elements)?;
                let mut author_nodes=cite.prefix_nodes();author_nodes.extend(author);remainder.extend(cite.suffix_nodes());
                append(&mut inline,author_nodes,delimiter);append(&mut note,remainder,delimiter);
            },
            CiteMode::AuthorOnly=>append(&mut inline,publication_rich::from_elements(elements)?,delimiter),
            CiteMode::Normal|CiteMode::SuppressAuthor=>append(&mut note,publication_rich::from_elements(elements)?,delimiter),
        }
    }
    Ok((inline,note))
}
fn within_authored_note(mut children: ElemChildren, cites: &[Cite]) -> ElemChildren {
    fn bracket(children: &mut ElemChildren) {
        children.0.insert(0,ElemChild::Text(Formatted{text:"(".into(),formatting:Default::default()}));
        children.0.push(ElemChild::Text(Formatted{text:")".into(),formatting:Default::default()}));
    }
    if cites.iter().all(|cite|matches!(cite.mode,CiteMode::Normal|CiteMode::SuppressAuthor)) { bracket(&mut children); }
    else {
        fn entries(children: &mut ElemChildren, cites: &[Cite]) {
            for child in &mut children.0 { if let ElemChild::Elem(elem)=child {
                if let Some(hayagriva::ElemMeta::Entry(index))=&elem.meta {
                    if cites.get(*index).is_some_and(|cite|matches!(cite.mode,CiteMode::Normal|CiteMode::SuppressAuthor)) { bracket(&mut elem.children); }
                } else { entries(&mut elem.children,cites); }
            }}
        }
        entries(&mut children,cites);
    }
    children
}
fn capitalize_note_term(children: &mut ElemChildren, cites: &[Cite]) {
    fn first(children: &mut ElemChildren, term: bool) -> bool {
        for child in &mut children.0 {
            match child {
                ElemChild::Text(value) => {
                    if let Some((at, ch)) = value.text.char_indices().find(|(_, ch)| ch.is_alphanumeric()) {
                        if term && ch.is_lowercase() { value.text.replace_range(at..at + ch.len_utf8(), &ch.to_uppercase().collect::<String>()); }
                        return true;
                    }
                }
                ElemChild::Elem(elem) => {
                    if first(&mut elem.children, term || elem.meta == Some(hayagriva::ElemMeta::Term)) { return true; }
                }
                ElemChild::Link { text, .. } => { if text.text.chars().any(char::is_alphanumeric) { return true; } }
                ElemChild::Markup(text) => { if text.chars().any(char::is_alphanumeric) { return true; } }
                ElemChild::Transparent { .. } => return true,
            }
        }
        false
    }
    fn remainder(children: &mut ElemChildren) -> bool {
        for child in &mut children.0 { if let ElemChild::Elem(elem) = child {
            if elem.meta == Some(hayagriva::ElemMeta::ProseRemainder) {
                if first(&mut elem.children, false) { return true; }
            } else if remainder(&mut elem.children) { return true; }
        }}
        false
    }
    fn entries(children: &mut ElemChildren, cites: &[Cite]) -> bool {
        for child in &mut children.0 {
            match child {
                ElemChild::Elem(elem) => {
                    if let Some(hayagriva::ElemMeta::Entry(index)) = elem.meta {
                        let found = match cites.get(index).map(|cite| cite.mode) {
                            Some(CiteMode::AuthorOnly) => false,
                            Some(CiteMode::Prose) => remainder(&mut elem.children),
                            _ => first(&mut elem.children, false),
                        };
                        if found { return true; }
                    } else if entries(&mut elem.children, cites) { return true; }
                }
                ElemChild::Text(value) => { if value.text.chars().any(char::is_alphanumeric) { return true; } }
                ElemChild::Link { text, .. } => { if text.text.chars().any(char::is_alphanumeric) { return true; } }
                ElemChild::Markup(text) => { if text.chars().any(char::is_alphanumeric) { return true; } }
                ElemChild::Transparent { .. } => return true,
            }
        }
        false
    }
    // Only a processor-owned leading term is changed. Authored affixes,
    // names, titles, explicit lowercase and a prose citation's inline author
    // retain their original text and formatting.
    entries(children, cites);
}
/// Keep the optional author layout outside citationberg's CSL 1.0.2 model.
/// The authored XML and the normal citation/bibliography are never rewritten.
fn load_intext(csl: &str) -> ApiResult<Option<hayagriva::InText>> {
    use crate::math_document::{parse_xml, XmlNode, XmlPart};
    use quick_xml::{events::Event, Reader};
    const CSL_NS: &str = "http://purl.org/net/xbiblio/csl";
    // A lexical mention in a comment/value is not an extension. Keep normal
    // styles on the existing path and apply the tree budget only to intext.
    if !csl.contains("intext") { return Ok(None); }
    let bad = || invalid("academic_style").noted("field", "csl.intext");
    let mut reader = Reader::from_str(csl);
    let mut found = false;
    loop {
        match reader.read_event().map_err(|_| bad())? {
            Event::Start(start) | Event::Empty(start) if start.local_name().as_ref() == "intext" => { found = true; break; }
            Event::Eof => break,
            _ => {}
        }
    }
    if !found { return Ok(None); }
    let xml = parse_xml(csl).map_err(|code| if code == "math_budget" { ApiError::new(413, "academic_budget") } else { bad() })?;
    fn count(node: &XmlNode) -> usize {
        usize::from(node.local() == "intext") + node.children().map(count).sum::<usize>()
    }
    let root = xml.children().next().ok_or_else(bad)?;
    if root.local() != "style" || root.namespace != CSL_NS || count(root) != 1 { return Err(bad()); }
    let intext = root.children().find(|node| node.local() == "intext" && node.namespace == CSL_NS).ok_or_else(bad)?;
    // Intext inherits citation sorting/state; it only supplies an author
    // layout and standard name options. Never silently discard other modes.
    const NAME_OPTIONS: &[&str] = &["and", "name-delimiter", "names-delimiter", "delimiter-precedes-et-al", "delimiter-precedes-last", "et-al-min", "et-al-use-first", "et-al-subsequent-min", "et-al-subsequent-use-first", "et-al-use-last", "name-form", "initialize", "initialize-with", "name-as-sort-order", "sort-separator"];
    if intext.attributes.keys().any(|key| key != "xmlns" && !key.starts_with("xmlns:") && !NAME_OPTIONS.contains(&key.as_str()))
        || intext.children().count() != 1
        || !intext.children().next().is_some_and(|node| node.local() == "layout" && node.namespace == CSL_NS)
        || intext.content.iter().any(|part| matches!(part, XmlPart::Text(text) if !text.trim().is_empty())) {
        return Err(bad());
    }
    // Reuse the locked parser and its layout/name model, with the original
    // style metadata/macros/locales still in scope. This temporary AST is not
    // returned, saved, used as cache identity or rendered as the normal style.
    let mut projected = root.clone();
    projected.content.retain(|part| !matches!(part, XmlPart::Element(node) if node.namespace == CSL_NS && matches!(node.local(), "citation" | "intext")));
    let mut citation = intext.clone();
    citation.name = citation.name.rsplit_once(':').map_or_else(|| "citation".into(), |(prefix, _)| format!("{prefix}:citation"));
    projected.content.push(XmlPart::Element(citation));
    let Style::Independent(parsed) = Style::from_xml(&projected.xml()).map_err(|_| bad())? else { return Err(bad()); };
    Ok(Some(hayagriva::InText { layout: parsed.citation.layout, name_options: parsed.citation.name_options }))
}
fn load_style(name: &str, csl: Option<&str>) -> ApiResult<(Style, Option<hayagriva::InText>)> {
    if let Some(csl) = csl {
        if csl.len() > 1024 * 1024 || csl.contains("<!DOCTYPE") || csl.contains("<!ENTITY") { return Err(invalid("academic_style")); }
        let style = Style::from_xml(csl).map_err(|_| invalid("academic_style"))?;
        Ok((style, load_intext(csl)?))
    } else {
        hayagriva::archive::ArchivedStyle::by_name(name)
            .or_else(|| hayagriva::archive::ArchivedStyle::by_id(name))
            .or_else(|| hayagriva::archive::ArchivedStyle::by_id(&format!("http://www.zotero.org/styles/{name}")))
            .map(|style| (style.get(), None)).ok_or_else(|| invalid("academic_style"))
    }
}
pub fn is_note_style(name: &str, csl: Option<&str>) -> ApiResult<bool> {
    let (Style::Independent(style), _) = load_style(name, csl)? else { return Err(invalid("academic_style_parent_required")); };
    Ok(style.settings.class == hayagriva::citationberg::StyleClass::Note)
}
pub struct ReferenceSummary { pub key: String, pub title: String }
fn csl_json_library(data: &str) -> ApiResult<(Vec<hayagriva::citationberg::json::Item>, Vec<Value>)> {
    if data.len() > 2 * 1024 * 1024 { return Err(ApiError::new(413, "academic_budget")); }
    let objects: Vec<serde_json::Map<String, Value>> = serde_json::from_str(data).map_err(|_| invalid("academic_bibliography"))?;
    if objects.len() > 5000 { return Err(ApiError::new(413, "academic_budget")); }
    let mut entries = Vec::with_capacity(objects.len());
    let mut diagnostics = Vec::new();
    for (index, mut object) in objects.into_iter().enumerate() {
        // citationberg's strongly typed values cannot represent arbitrary
        // processor extensions. Project only standard CSL variables into the
        // processor; the authored JSON remains untouched in Request::data.
        let unknown = object.keys().filter(|key| key.as_str() != "id" && key.as_str() != "type"
            && serde_json::from_value::<hayagriva::citationberg::taxonomy::Variable>(Value::String((*key).clone())).is_err()).cloned().collect::<Vec<_>>();
        for field in unknown {
            if diagnostics.len() == 8192 { return Err(ApiError::new(413, "academic_budget")); }
            object.remove(&field);
            diagnostics.push(json!({"code":"academic_diagnostic","field":format!("data[{index}].{field}"),"reason":"unsupported_csl_json_extension_ignored","source_preserved":true}));
        }
        entries.push(serde_json::from_value(Value::Object(object)).map_err(|_| invalid("academic_bibliography"))?);
    }
    Ok((entries, diagnostics))
}

pub(crate) fn custom_locale(xml: &str, requested: &str) -> ApiResult<hayagriva::citationberg::Locale> {
    use quick_xml::{events::Event, name::ResolveResult, reader::NsReader};
    if xml.len() > 256 * 1024 { return Err(ApiError::new(413, "academic_budget")); }
    let bad = || context_error("locale_xml");
    if xml.contains("<!DOCTYPE") || xml.contains("<!ENTITY") { return Err(bad()); }
    let mut reader = NsReader::from_str(xml);
    let mut depth = 0usize;
    let mut roots = 0usize;
    loop {
        let (namespace, event) = reader.read_resolved_event().map_err(|_| bad())?;
        match event {
            Event::Start(ref start) | Event::Empty(ref start) => {
                if depth == 0 {
                    roots += 1;
                    if roots != 1 || start.local_name().as_ref() != "locale"
                        || !matches!(namespace, ResolveResult::Bound(ref ns) if ns.as_ref() == "http://purl.org/net/xbiblio/csl") { return Err(bad()); }
                    let mut lang = None;
                    for attribute in start.attributes() {
                        let attribute = attribute.map_err(|_| bad())?;
                        if attribute.key.as_ref() == "xml:lang" {
                            let raw = attribute.normalized_value(quick_xml::XmlVersion::Implicit1_0).map_err(|_| bad())?;
                            lang = crate::md_ast::publication_language_tag(&raw);
                        }
                    }
                    if lang.as_deref() != Some(requested) { return Err(bad()); }
                }
                if matches!(event, Event::Start(_)) { depth += 1; }
                if depth > 64 { return Err(ApiError::new(413, "academic_budget")); }
            }
            Event::End(_) => { depth = depth.checked_sub(1).ok_or_else(bad)?; }
            Event::DocType(_) => return Err(bad()),
            Event::Text(ref value) if depth == 0 && !value.as_ref().bytes().all(|byte| byte.is_ascii_whitespace()) => return Err(bad()),
            Event::CData(_) | Event::GeneralRef(_) if depth == 0 => return Err(bad()),
            Event::Eof => break,
            _ => {}
        }
    }
    if roots != 1 || depth != 0 { return Err(bad()); }
    let file = hayagriva::citationberg::LocaleFile::from_xml(xml).map_err(|_| bad())?;
    if crate::md_ast::publication_language_tag(&file.lang.0).as_deref() != Some(requested) { return Err(bad()); }
    let mut locale: hayagriva::citationberg::Locale = file.into();
    locale.lang = Some(LocaleCode(requested.to_string()));
    Ok(locale)
}

pub fn reference_summaries(format: &str, data: &str) -> ApiResult<Vec<ReferenceSummary>> {
    if data.len() > 2 * 1024 * 1024 { return Err(ApiError::new(413, "academic_budget")); }
    let entries: Vec<ReferenceSummary> = match format {
        "bibtex" | "biblatex" => hayagriva::io::from_biblatex_str(data).map_err(|_| invalid("academic_bibliography"))?.iter().map(|entry|ReferenceSummary{key:entry.key().to_string(),title:entry.title().map(ToString::to_string).unwrap_or_default()}).collect(),
        "csl-json" => csl_json_library(data)?.0.iter().map(|entry|Ok(ReferenceSummary{key:entry.id().map(|id|id.into_owned()).ok_or_else(|| invalid("academic_reference_id"))?,title:entry.0.get("title").and_then(|value|value.to_str()).map(|title|title.into_owned()).unwrap_or_default()})).collect::<ApiResult<Vec<_>>>()?,
        _ => return Err(invalid("academic_bibliography_format")),
    };
    if entries.len() > 5000 { return Err(ApiError::new(413, "academic_budget")); }
    let mut seen=std::collections::HashSet::new();
    for entry in &entries {
        if entry.key.is_empty() || entry.key.len()>1024 { return Err(invalid("academic_reference_id")); }
        if !seen.insert(entry.key.as_str()) { return Err(invalid("academic_duplicate_reference")); }
    }
    Ok(entries)
}
pub fn reference_keys(format: &str, data: &str) -> ApiResult<std::collections::HashSet<String>> {
    Ok(reference_summaries(format,data)?.into_iter().map(|entry|entry.key).collect())
}
pub fn citations(request: &Request) -> ApiResult<Value> {
    if request.data.len() > 2 * 1024 * 1024
        || request.clusters.len() > 10000
        || request.clusters.iter().any(|c| c.len() > 100)
        || request.locale.len() > 255
        || request.locale_xml.as_ref().is_some_and(|xml| xml.len() > 256 * 1024)
        || (!request.preceding_notes.is_empty() && request.preceding_notes.len() != request.clusters.len())
        || request.preceding_notes.iter().any(|n| *n > 10000)
        || (!request.contexts.is_empty() && request.contexts.len() != request.clusters.len())
    {
        return Err(ApiError::new(413, "academic_budget"));
    }
    for (index, context) in request.contexts.iter().enumerate() {
        if context.note_number.is_some_and(|number| number == 0 || number > 10000) || (context.in_note && context.note_number.is_none()) {
            return Err(context_error(&format!("contexts[{index}].note_number")));
        }
    }
    for (cluster, cites) in request.clusters.iter().enumerate() {
        for (item, cite) in cites.iter().enumerate() { cite.validate().map_err(|error| {
            let field = match &error.shape { crate::error::ErrorShape::ApiCode { extras } => extras.iter().find(|(key,_)| key == "field").and_then(|(_,value)| value.as_str()).unwrap_or("cite"), _ => "cite" };
            let path=format!("clusters[{cluster}][{item}].{field}");error.noted("field",path)
        })?; }
    }
    let (style, intext) = load_style(&request.style, request.csl.as_deref())?;
    let Style::Independent(style) = style else {
        return Err(invalid("academic_style_parent_required"));
    };
    let requested_locale = crate::md_ast::publication_language_tag(&request.locale)
        .ok_or_else(|| ApiError::bad_request("invalid_json"))?;
    let mut locales = hayagriva::archive::locales();
    if let Some(xml) = &request.locale_xml {
        let mut custom = custom_locale(xml, &requested_locale)?;
        if let Some(base) = locales.iter().find(|locale| locale.lang.as_ref().is_some_and(|lang| lang.0 == requested_locale)) {
            // Sparse custom terms/dates override this language, while missing
            // definitions retain the locked locale's normal fallbacks.
            match (&mut custom.terms, &base.terms) {
                (Some(target), Some(source)) => {
                    let fallback=source.terms.iter().filter(|term| !target.terms.iter().any(|custom|
                        custom.name==term.name && custom.form==term.form && custom.match_==term.match_
                            && custom.gender==term.gender && custom.gender_form==term.gender_form)).cloned().collect::<Vec<_>>();
                    target.terms.extend(fallback);
                }
                (None, source) => custom.terms = source.clone(),
                _ => {}
            }
            custom.date.extend(base.date.clone());
            if let Some(base_options) = &base.style_options {
                if let Some(options) = &mut custom.style_options {
                    options.limit_day_ordinals_to_day_1 = options.limit_day_ordinals_to_day_1.or(base_options.limit_day_ordinals_to_day_1);
                    options.punctuation_in_quote = options.punctuation_in_quote.or(base_options.punctuation_in_quote);
                } else { custom.style_options = Some(base_options.clone()); }
            }
        }
        locales.retain(|locale| !locale.lang.as_ref().is_some_and(|lang| lang.0 == requested_locale));
        locales.insert(0, custom);
    }
    // CSL files commonly use language-region tags. Retain the full authored
    // tag (and custom style locales), but make an explicitly declared region
    // available before the processor falls back to the language default.
    // In particular zh-Hant-TW must not fall through to simplified zh-CN.
    let locale_alias = if !locales.iter().any(|l| l.lang.as_ref().is_some_and(|l|l.0 == requested_locale)) {
        crate::md_ast::publication_language_region(&requested_locale).and_then(|region| {
            let base = requested_locale.split('-').next()?;
            let candidate = format!("{base}-{region}");
            let mut alias = locales.iter().find(|l|l.lang.as_ref().is_some_and(|l|l.0 == candidate))?.clone();
            alias.lang = Some(LocaleCode(requested_locale.clone()));
            Some((candidate, alias))
        })
    } else { None };
    let alias_name = locale_alias.as_ref().map(|(name,_)|name.clone());
    if let Some((_,alias)) = locale_alias { locales.push(alias); }
    let locale = Some(LocaleCode(requested_locale.clone()));
    let mut diagnostics = Vec::new();
    // EntryLike is intentionally private in Hayagriva; expansion retains its
    // concrete BibLaTeX and CSL-JSON implementations without a parallel engine.
    macro_rules! render {($entries:expr)=>{{
        let entries=$entries;
        if entries.len()>5000{return Err(ApiError::new(413,"academic_budget"));}
        let mut driver=BibliographyDriver::new();
        let mut issued_notes=0usize;
        let mut slots=Vec::new();
        for(index,cluster)in request.clusters.iter().enumerate(){
            let mut items=Vec::new();
            for(item_index,cite)in cluster.iter().enumerate(){
                if let Some(entry)=entries.get(&cite.key){
                    let purpose=match cite.mode {
                        CiteMode::Normal=>None,
                        CiteMode::SuppressAuthor=>Some(hayagriva::CitePurpose::SuppressAuthor),
                        CiteMode::AuthorOnly=>Some(hayagriva::CitePurpose::Author),
                        CiteMode::Prose=>Some(hayagriva::CitePurpose::Prose),
                    };
                    items.push(CitationItem::new(*entry,locator(cite)?,None,false,purpose)
                        .with_affixes(publication_rich::to_elements(&cite.prefix_nodes())?,publication_rich::to_elements(&cite.suffix_nodes())?));
                }
                else{diagnostics.push(json!({"code":"academic_missing_reference","key":cite.key,"cluster":index,"item":item_index}));}
            }
            let complete=items.len()==cluster.len()&&!items.is_empty();
            // A partially resolved cluster is not a citation event. Feeding its
            // known subset to the driver would change later first/ibid state.
            if !complete {continue;}
            let prose_only=cluster.iter().all(|cite|cite.mode==CiteMode::AuthorOnly);
            let note=if let Some(context)=request.contexts.get(index){context.note_number}
                else if prose_only||style.settings.class!=hayagriva::citationberg::StyleClass::Note{None}else{issued_notes+=1;Some(request.preceding_notes.get(index).copied().unwrap_or(0)+issued_notes)};
            driver.citation(CitationRequest::new(items,&style,locale.clone(),&locales,note).with_intext(intext.as_ref()));
            slots.push(index);
        }
        let rendered=driver.finish(BibliographyRequest{style:&style,locale:locale.clone(),locale_files:&locales});
        let mut citations=vec![json!({"text":"","html":"","markdown":"","rich":[],"layout":[],"inline_layout":[],"note_layout":[],"markdown_format_exact":true,"note_number":null,"complete":false});request.clusters.len()];
        let mut output_bytes=0usize;
        let mut started_notes=std::collections::HashSet::new();
        for(index,mut item)in slots.into_iter().zip(rendered.citations){
            if style.settings.class==hayagriva::citationberg::StyleClass::Note
                && request.clusters[index].iter().any(|cite|cite.mode!=CiteMode::AuthorOnly)
                && item.note_number.is_some_and(|note| started_notes.insert(note))
                && !request.contexts.get(index).is_some_and(|context|context.in_note) {
                capitalize_note_term(&mut item.citation, &request.clusters[index]);
            }
            if style.settings.class==hayagriva::citationberg::StyleClass::Note
                && request.contexts.get(index).is_some_and(|context|context.in_note){
                item.citation=within_authored_note(item.citation,&request.clusters[index]);
            }
            if intext.is_some() {
                // An explicit but unprintable author layout is an error, not
                // permission to substitute the normal citation or drop an
                // author's component from a mixed cluster/prose citation.
                fn entry(children: &ElemChildren, index: usize) -> Option<&ElemChildren> {
                    children.0.iter().find_map(|child| if let ElemChild::Elem(elem) = child {
                        if elem.meta == Some(hayagriva::ElemMeta::Entry(index)) { Some(&elem.children) }
                        else { entry(&elem.children, index) }
                    } else { None })
                }
                for (item_index, cite) in request.clusters[index].iter().enumerate() {
                    if !matches!(cite.mode, CiteMode::AuthorOnly | CiteMode::Prose) { continue; }
                    let rendered = entry(&item.citation, item_index);
                    let author = if let Some(children) = rendered { prose_parts(children)?.0 } else { Vec::new() };
                    if publication_rich::plain(&author).trim().is_empty() {
                        return Err(invalid("academic_unprintable_reference").noted("field",format!("clusters[{index}][{item_index}]")));
                    }
                }
            }
            let nodes=publication_rich::from_elements(&item.citation)?;
            if request.clusters[index].iter().all(|cite|cite.mode==CiteMode::AuthorOnly)
                && publication_rich::plain(&nodes).trim().is_empty(){
                return Err(invalid("academic_unprintable_reference").noted("field",format!("clusters[{index}]")));
            }
            let (inline_nodes,note_nodes)=if style.settings.class==hayagriva::citationberg::StyleClass::Note
                && request.clusters[index].iter().any(|cite|matches!(cite.mode,CiteMode::Prose|CiteMode::AuthorOnly)){
                note_prose_parts(&item.citation,&request.clusters[index],style.citation.layout.delimiter.as_deref().unwrap_or(""))?
            }else{(Vec::new(),Vec::new())};
            let layout=publication_rich::layout(&nodes)?;
            let inline_layout=publication_rich::layout(&inline_nodes)?;
            let note_layout=publication_rich::layout(&note_nodes)?;
            for tree in [&nodes,&inline_nodes,&note_nodes]{output_bytes=output_bytes.saturating_add(serde_json::to_vec(tree).map_err(|_|invalid("academic_rich_unsupported"))?.len());}
            for flow in [&layout,&inline_layout,&note_layout]{output_bytes=output_bytes.saturating_add(serde_json::to_vec(flow).map_err(|_|invalid("academic_rich_unsupported"))?.len());}
            if output_bytes>publication_rich::MAX_RICH_BYTES{return Err(ApiError::new(413,"academic_rich_budget"));}
            let(_,markdown,exact)=rich(&item.citation);
            let html=publication_rich::html(&nodes)?;
            citations[index]=json!({"text":format!("{:#}",item.citation),"html":html,"markdown":markdown,"rich":nodes,"layout":layout,"inline_rich":inline_nodes,"note_rich":note_nodes,"inline_layout":inline_layout,"note_layout":note_layout,"markdown_format_exact":exact,"note_number":item.note_number,"complete":true});
        }
        let bibliography=if let Some(bib)=rendered.bibliography{
            let mut items=Vec::new();
            for item in bib.items{
                let nodes=publication_rich::from_elements(&item.content)?;
                let first_nodes=if let Some(first)=&item.first_field{publication_rich::from_elements(&ElemChildren(vec![first.clone()]))?}else{Vec::new()};
                let rich_item=publication_rich::RichBibliographyItem{key:item.key.clone(),rich:nodes,first_field:first_nodes};
                let layout=rich_item.layout()?;
                for tree in [&rich_item.rich,&rich_item.first_field]{output_bytes=output_bytes.saturating_add(serde_json::to_vec(tree).map_err(|_|invalid("academic_rich_unsupported"))?.len());}
                output_bytes=output_bytes.saturating_add(serde_json::to_vec(&layout).map_err(|_|invalid("academic_rich_unsupported"))?.len());
                if output_bytes>publication_rich::MAX_RICH_BYTES{return Err(ApiError::new(413,"academic_rich_budget"));}
                let(_,markdown,exact)=rich(&item.content);
                let html=publication_rich::html(&rich_item.rich)?;
                items.push(json!({"key":item.key,"text":format!("{:#}",item.content),"html":html,"markdown":markdown,"rich":rich_item.rich,"layout":layout,"markdown_format_exact":exact,"first_field":item.first_field.map(|f|format!("{f:#}")),"first_field_rich":rich_item.first_field}));
            }
            Some(json!({"hanging_indent":bib.hanging_indent,"line_spacing":bib.line_spacing.get(),"entry_spacing":bib.entry_spacing,"items":items}))
        }else{None};
        if output_bytes>publication_rich::MAX_RICH_BYTES{return Err(ApiError::new(413,"academic_rich_budget"));}
        Ok(json!({"ok":true,"locale":requested_locale,"locale_alias":alias_name,"citations":citations,"bibliography":bibliography,"diagnostics":diagnostics,"style_class":if style.settings.class==hayagriva::citationberg::StyleClass::Note{"note"}else{"in-text"}}))
    }}}
    match request.format.as_str() {
        "bibtex" | "biblatex" => {
            let library = hayagriva::io::from_biblatex_str(&request.data)
                .map_err(|_| invalid("academic_bibliography"))?;
            let mut entries = HashMap::new();
            for entry in library.iter() {
                if entry.title().is_none() {
                    diagnostics.push(json!({"code":"academic_missing_title","key":entry.key()}));
                }
                entries.insert(entry.key().to_string(), entry);
            }
            render!(entries)
        }
        "csl-json" => {
            let (library, ignored_extensions) = csl_json_library(&request.data)?;
            diagnostics.extend(ignored_extensions);
            let mut entries = HashMap::new();
            for entry in &library {
                let key = entry
                    .id()
                    .ok_or_else(|| invalid("academic_reference_id"))?
                    .to_string();
                if !entry.0.contains_key("title") {
                    diagnostics.push(json!({"code":"academic_missing_title","key":key}));
                }
                // CSL author is optional. Names may legally substitute title,
                // editor or another field; its absence alone is not a warning.
                if entries.insert(key, entry).is_some() {
                    return Err(invalid("academic_duplicate_reference"));
                }
            }
            render!(entries)
        }
        _ => Err(invalid("academic_bibliography_format")),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn request(format: &str, data: &str) -> Request {
        Request {
            format: format.into(),
            data: data.into(),
            style: default_style(),
            locale: default_locale(),
            locale_xml: None,
            csl: None,
            preceding_notes: vec![],
            contexts: vec![],
            clusters: vec![vec![Cite {
                key: "kant".into(),
                locator: Some("42–44".into()),
                kind: None,
                ..Default::default()
            }]],
        }
    }
    #[test]
    fn bibtex_and_csl_json_render_real_citations_locators_and_bibliography() {
        let bib = r#"@book{kant,title={Critique of Pure Reason},author={Kant, Immanuel},year={1781},publisher={Cambridge University Press}}"#;
        let json = r#"[{"id":"kant","type":"book","title":"Critique of Pure Reason","author":[{"family":"Kant","given":"Immanuel"}],"issued":{"date-parts":[[1781]]},"publisher":"Cambridge University Press"}]"#;
        for req in [request("bibtex", bib), request("csl-json", json)] {
            let output = citations(&req).unwrap();
            let cite = output["citations"][0]["text"].as_str().unwrap();
            assert!(
                cite.contains("Kant") && cite.contains("1781") && cite.contains("42–44"),
                "{cite}"
            );
            assert_eq!(output["bibliography"]["items"][0]["key"], "kant");
            assert!(output["bibliography"]["items"][0]["text"]
                .as_str()
                .unwrap()
                .contains("Critique of Pure Reason"));
            assert_eq!(output["diagnostics"], json!([]));
        }
    }
    #[test]
    fn missing_references_and_untrusted_styles_are_diagnosed() {
        let req = request("csl-json", "[]");
        assert_eq!(
            citations(&req).unwrap()["diagnostics"][0]["code"],
            "academic_missing_reference"
        );
        let mut req = req;
        req.csl = Some("<!DOCTYPE style><style/>".into());
        assert_eq!(citations(&req).unwrap_err().code, "academic_style");
    }
    #[test]
    fn csl_locale_normalizes_declared_region_and_keeps_style_overrides() {
        let mut req=request("csl-json",r#"[{"id":"kant","type":"book","title":"Evidence","issued":{"date-parts":[[2026,1,13]]}}]"#);
        req.csl=Some(r#"<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="in-text"><info><title>Locale probe</title><id>urn:readmd:test:locale</id><updated>2026-10-13T00:00:00Z</updated></info><citation><layout delimiter=" "><text term="page" suffix=" "/><date variable="issued"><date-part name="month" form="long"/></date></layout></citation></style>"#.into());
        for (locale,month) in [("de_DE","Januar"),("fr_CA","janvier"),("en-US","January")] {
            req.locale=locale.into();let result=citations(&req).unwrap();
            assert!(result["citations"][0]["text"].as_str().unwrap().contains(month),"{result}");
        }
        req.locale="zh-TW".into();let traditional=citations(&req).unwrap()["citations"][0]["text"].clone();
        req.locale="zh_Hant_TW".into();let result=citations(&req).unwrap();
        assert_eq!(result["locale"],"zh-Hant-TW");assert_eq!(result["locale_alias"],"zh-TW");assert_eq!(result["citations"][0]["text"],traditional);
        req.csl=Some(req.csl.as_ref().unwrap().replace("<citation>",r#"<locale xml:lang="zh-Hant-TW"><terms><term name="page">CUSTOM PAGE</term></terms></locale><citation>"#));
        assert!(citations(&req).unwrap()["citations"][0]["text"].as_str().unwrap().contains("CUSTOM PAGE"));
        req.locale="de\" onclick=\"unsafe".into();assert_eq!(citations(&req).unwrap_err().code,"invalid_json");
    }
    #[test]
    fn csl_output_preserves_emphasis_and_escapes_titles_links_and_markup() {
        let input = r#"[{"id":"kant","type":"book","title":"A <script>unsafe</script> study","author":[{"family":"Kant","given":"Immanuel"}],"issued":{"date-parts":[[1781]]}}]"#;
        let result = citations(&request("csl-json", input)).unwrap();
        let item = &result["bibliography"]["items"][0];
        assert!(!item["html"].as_str().unwrap().contains("<script>"));
        assert!(item["markdown"].as_str().unwrap().contains('*'));
        assert_eq!(item["markdown_format_exact"], true, "literal title text remains text after escaping");
        let plain=citations(&request("csl-json",&input.replace("A <script>unsafe</script> study","A safe study"))).unwrap();assert_eq!(plain["bibliography"]["items"][0]["markdown_format_exact"],true);
        let unsupported = ElemChildren(vec![ElemChild::Markup("#unsafe()".into())]);
        assert!(!rich(&unsupported).2, "Typst source markup cannot be represented as formatted Markdown");
        let link = ElemChildren(vec![ElemChild::Link { text: Formatted { text: " label ".into(), formatting: hayagriva::Formatting { font_style: hayagriva::citationberg::FontStyle::Italic, ..Default::default() } }, url: "https://example.test/\")<script>".into() }]);
        let (html, md, exact) = rich(&link);
        assert!(html.contains("&quot;") && !html.contains("<script>"));
        assert_eq!(md, "[ *label* ](https://example.test/%22%29%3Cscript%3E)");
        assert!(exact);
        assert_eq!(escape_markdown("(Kant, 1781) [1]"), "(Kant, 1781) &#91;1&#93;");
    }
    fn probe_style(class: &str, citation: &str) -> String {
        format!(r#"<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0" class="{class}"><info><title>ReadMD context probe</title><id>urn:readmd:context-probe</id><updated>2026-10-13T00:00:00Z</updated></info>{citation}</style>"#)
    }
    const PROBE_ENTRIES: &str = r#"[{"id":"a","type":"book","title":"Analytical Engines","author":[{"family":"Lovelace","given":"Ada"}],"issued":{"date-parts":[[1843]]}},{"id":"b","type":"book","title":"Computable Numbers","author":[{"family":"Turing","given":"Alan"}],"issued":{"date-parts":[[1936]]}},{"id":"c","type":"book","title":"Information Theory","author":[{"family":"Shannon","given":"Claude"}],"issued":{"date-parts":[[1948]]}}]"#;
    #[test]
    fn citation_context_model_preserves_explicit_flags_and_reports_exact_conflicts() {
        for (value, mode) in [
            (json!({"key":"a"}),CiteMode::Normal),
            (json!({"id":"a","suppress-author":true}),CiteMode::SuppressAuthor),
            (json!({"key":"a","author_only":true,"suppress_author":false}),CiteMode::AuthorOnly),
            (json!({"key":"a","mode":"prose","author_only":false}),CiteMode::Prose),
        ] { assert_eq!(Cite::from_value(&value).unwrap().mode,mode); }
        for (value,field) in [
            (json!({"key":"a","mode":"suppress-author","suppress_author":false}),"suppress_author"),
            (json!({"key":"a","author_only":true,"suppress-author":true}),"author_only"),
            (json!({"key":"a","author_only":true,"author-only":false}),"author-only"),
            (json!({"key":"a","kind":"page","label":"chapter"}),"label"),
            (json!({"key":"a","prefixx":"see "}),"prefixx"),
        ] {
            let error=parse_request(json!({"format":"csl-json","data":PROBE_ENTRIES,"clusters":[[value]]})).err().unwrap();
            assert_eq!(error.code,"academic_context");
            let crate::error::ErrorShape::ApiCode{extras}=error.shape else{panic!("missing field error")};
            assert!(extras.iter().any(|(key,value)|key=="field"&&value==&json!(format!("clusters[0][0].{field}"))),"{extras:?}");
        }
        let error=parse_request(json!({"format":"csl-json","data":PROBE_ENTRIES,"clusters":[[{"key":"a","prefix_rich":[{"type":"link","href":"javascript:bad()","children":[]}]}]]})).err().unwrap();
        assert_eq!(error.code,"academic_rich_link");
    }
    #[test]
    fn citation_locator_model_accepts_every_locked_kind_and_rejects_unknown_without_a_value() {
        for kind in ["act","appendix","article-locator","book","canon","chapter","column","elocation","equation","figure","folio","issue","line","note","opus","page","paragraph","part","rule","scene","section","sub verbo","sub-verbo","supplement","table","timestamp","title","title-locator","verse","volume"] {
            let cite=Cite{kind:Some(kind.into()),locator:Some("8–11".into()),..Cite::new("a")};
            assert!(cite.validate().is_ok(),"{kind}");assert!(locator(&cite).unwrap().is_some());
        }
        assert_eq!(Cite{kind:Some("not-a-locator".into()),..Cite::new("a")}.validate().unwrap_err().code,"academic_context");
    }
    #[test]
    fn citation_item_affixes_preserve_numeric_collapse_edges_and_keep_middle_explanations() {
        let mut req=request("csl-json",PROBE_ENTRIES);
        req.csl=Some(probe_style("in-text",r#"<citation collapse="citation-number"><layout prefix="[" suffix="]" delimiter=", "><text variable="citation-number"/></layout></citation>"#));
        let mut a=Cite::new("a");a.prefix=Some("compare ".into());let b=Cite::new("b");let mut c=Cite::new("c");c.suffix=Some(" also".into());
        req.clusters=vec![vec![a,b,c]];
        assert_eq!(citations(&req).unwrap()["citations"][0]["text"],"[compare 1–3 also]");
        req.clusters[0][0].prefix=None;req.clusters[0][2].suffix=None;req.clusters[0][1].prefix=Some("compare ".into());
        assert_eq!(citations(&req).unwrap()["citations"][0]["text"],"[1, compare 2, 3]");
        req.clusters[0][1].prefix=None;req.clusters[0][1].locator=Some("9".into());
        req.csl=Some(probe_style("in-text",r#"<citation collapse="citation-number"><layout prefix="[" suffix="]" delimiter=", "><group delimiter=":"><text variable="citation-number"/><text variable="locator"/></group></layout></citation>"#));
        assert_eq!(citations(&req).unwrap()["citations"][0]["text"],"[1, 2:9, 3]");
    }
    #[test]
    fn citation_modes_keep_prose_remainder_locators_and_anonymous_style_substitutes() {
        let mut req=request("csl-json",PROBE_ENTRIES);
        req.csl=Some(probe_style("in-text",r#"<citation><layout prefix="(" suffix=")" delimiter="; "><group delimiter=", "><names variable="author"><name form="short"/><substitute><names variable="editor"/><text variable="title"/></substitute></names><date variable="issued"><date-part name="year"/></date><text variable="title"/><text variable="locator"/></group></layout></citation>"#));
        req.clusters=[CiteMode::Normal,CiteMode::SuppressAuthor,CiteMode::AuthorOnly,CiteMode::Prose].into_iter().map(|mode|vec![Cite{mode,locator:Some("17".into()),..Cite::new("a")}]).collect();
        let result=citations(&req).unwrap();
        assert_eq!(result["citations"][0]["text"],"(Lovelace, 1843, Analytical Engines, 17)");
        assert_eq!(result["citations"][1]["text"],"(1843, Analytical Engines, 17)");
        assert_eq!(result["citations"][2]["text"],"Lovelace");
        assert_eq!(result["citations"][3]["text"],"Lovelace (1843, Analytical Engines, 17)");
        req.data=r#"[{"id":"a","type":"legislation","title":"Anonymous Statute","issued":{"date-parts":[[2024]]}}]"#.into();
        req.clusters=vec![vec![Cite{mode:CiteMode::AuthorOnly,..Cite::new("a")}]];
        let result=citations(&req).unwrap();assert_eq!(result["citations"][0]["text"],"Anonymous Statute");assert_eq!(result["diagnostics"],json!([]));
        req.data=r#"[{"id":"a","type":"book","title":"Anonymous Study","issued":{"date-parts":[[2024]]}}]"#.into();
        assert_eq!(citations(&req).unwrap()["diagnostics"],json!([]));
    }
    #[test]
    fn citation_driver_uses_physical_note_distance_and_ignores_partial_clusters() {
        let mut req=request("csl-json",PROBE_ENTRIES);
        req.csl=Some(probe_style("note",r#"<citation near-note-distance="5"><layout><names variable="author"><name form="short"/></names><choose><if position="first"><text value=" FIRST"/></if><else-if position="ibid"><text value=" IBID"/></else-if><else-if position="near-note"><text value=" NEAR"/></else-if><else><text value=" FAR"/></else></choose></layout></citation>"#));
        req.clusters=vec![vec![Cite::new("a")],vec![Cite::new("a")]];
        req.contexts=vec![CiteContext{note_number:Some(1),..Default::default()},CiteContext{note_number:Some(102),..Default::default()}];
        let far=citations(&req).unwrap();assert!(far["citations"][1]["text"].as_str().unwrap().ends_with(" FAR"),"{far}");
        req.contexts[1].note_number=Some(2);let clean=citations(&req).unwrap();assert!(clean["citations"][1]["text"].as_str().unwrap().ends_with(" IBID"));
        req.clusters.insert(1,vec![Cite::new("b"),Cite::new("missing")]);req.contexts.insert(1,CiteContext::default());
        let missing=citations(&req).unwrap();assert_eq!(missing["citations"][1]["complete"],false);assert_eq!(missing["citations"][2]["rich"],clean["citations"][1]["rich"]);
        req.clusters[1]=vec![Cite{mode:CiteMode::AuthorOnly,..Cite::new("b")}];
        assert_eq!(citations(&req).unwrap()["citations"][2]["rich"],clean["citations"][1]["rich"]);
        req.clusters=vec![vec![Cite{mode:CiteMode::Prose,locator:Some("17".into()),..Cite::new("a")}]];req.contexts=vec![CiteContext{note_number:Some(1),..Default::default()}];
        let prose=citations(&req).unwrap();
        assert!(prose["citations"][0]["inline_rich"].as_array().is_some_and(|nodes|!nodes.is_empty()));
        let inline:Vec<RichNode>=serde_json::from_value(prose["citations"][0]["inline_rich"].clone()).unwrap();let note:Vec<RichNode>=serde_json::from_value(prose["citations"][0]["note_rich"].clone()).unwrap();
        assert_eq!(publication_rich::plain(&inline),"Lovelace");assert!(!publication_rich::plain(&note).contains("Lovelace"));assert!(publication_rich::plain(&note).contains("FIRST"));
    }
    #[test]
    fn citation_ibid_requires_adjacent_notes_and_a_single_preceding_cite() {
        let mut req=request("csl-json",PROBE_ENTRIES);
        req.csl=Some(probe_style("note",r#"<citation near-note-distance="5"><layout delimiter="; "><names variable="author"><name form="short"/></names><choose><if position="first"><text value=" FIRST"/></if><else-if position="ibid-with-locator"><text value=" IBID-LOC"/></else-if><else-if position="ibid"><text value=" IBID"/></else-if><else-if position="near-note"><text value=" NEAR"/></else-if><else><text value=" FAR"/></else></choose></layout></citation>"#));
        req.clusters=vec![vec![Cite::new("a")],vec![Cite::new("a")]];
        for (previous,current,position) in [(1,1," IBID"),(1,2," IBID"),(1,3," NEAR"),(1,6," NEAR"),(1,7," FAR"),(1,102," FAR"),(102,1," FAR")] {
            req.contexts=vec![CiteContext{note_number:Some(previous),..Default::default()},CiteContext{note_number:Some(current),..Default::default()}];
            let output=citations(&req).unwrap();
            assert!(output["citations"][1]["text"].as_str().unwrap().ends_with(position),"notes {previous} → {current}: {output}");
        }
        req.clusters[1][0].locator=Some("17".into());
        req.contexts[0].note_number=Some(1);
        for (current,position) in [(2," IBID-LOC"),(3," NEAR"),(102," FAR")] {
            req.contexts[1].note_number=Some(current);
            let output=citations(&req).unwrap();
            assert!(output["citations"][1]["text"].as_str().unwrap().ends_with(position),"new locator at note {current}: {output}");
        }
        req.clusters=vec![vec![Cite::new("a"),Cite::new("b")],vec![Cite::new("b")]];
        req.contexts[1].note_number=Some(2);
        let multi=citations(&req).unwrap();
        assert!(multi["citations"][1]["text"].as_str().unwrap().ends_with(" NEAR"),"previous citation has two cites: {multi}");
        req.clusters=vec![vec![Cite::new("a"),Cite::new("a"),Cite{locator:Some("17".into()),..Cite::new("a")}]];
        req.contexts.truncate(1);
        assert_eq!(citations(&req).unwrap()["citations"][0]["text"],"Lovelace FIRST; Lovelace IBID; Lovelace IBID-LOC");
    }
    #[test]
    fn authored_note_context_brackets_normal_citations_without_bracketing_prose_or_author_only() {
        let mut req=request("csl-json",PROBE_ENTRIES);
        req.csl=Some(probe_style("note",r#"<citation><layout delimiter="; "><group delimiter=" | "><names variable="author"><name form="short"/></names><text variable="title"/><text variable="locator"/></group></layout></citation>"#));
        req.clusters=[CiteMode::Normal,CiteMode::SuppressAuthor,CiteMode::Prose,CiteMode::AuthorOnly].into_iter().map(|mode|vec![Cite{mode,locator:Some("17".into()),..Cite::new("a")}]).collect();
        req.contexts=vec![CiteContext{note_number:Some(1),in_note:true,note_label:Some("manual".into())};4];
        let output=citations(&req).unwrap();
        for(index,expected)in ["(Lovelace | Analytical Engines | 17)","(Analytical Engines | 17)","Lovelace Analytical Engines | 17","Lovelace"].into_iter().enumerate(){
            assert_eq!(output["citations"][index]["text"],expected);
            let tree:Vec<RichNode>=serde_json::from_value(output["citations"][index]["rich"].clone()).unwrap();assert_eq!(publication_rich::plain(&tree),expected);
        }
    }
    #[test]
    fn csl_json_numeric_ids_and_unknown_extensions_share_the_bounded_parser() {
        let data=json!([{"id":42,"type":"legal_case","title":"Synthetic v. Example","issued":{"date-parts":[[2026]]},"multi":{"_keys":{"title":{"fr":"Exemple"}}},"qa-extension":[true,{"extra":"kept in source"}]}]).to_string();
        let mut req=request("csl-json",&data);req.clusters=vec![vec![Cite::new("42")]];
        req.csl=Some(probe_style("in-text",r#"<citation><layout><text variable="title"/></layout></citation>"#));
        let output=citations(&req).unwrap();assert_eq!(output["citations"][0]["text"],"Synthetic v. Example");assert_eq!(req.data,data);
        let diagnostics=output["diagnostics"].as_array().unwrap();assert_eq!(diagnostics.len(),2);
        for field in ["data[0].multi","data[0].qa-extension"] {
            assert!(diagnostics.iter().any(|item|item["code"]=="academic_diagnostic"&&item["field"]==field&&item["source_preserved"]==true));
        }
        let entries=reference_summaries("csl-json",&data).unwrap();assert_eq!(entries[0].key,"42");assert_eq!(entries[0].title,"Synthetic v. Example");
        assert!(reference_keys("csl-json",&data).unwrap().contains("42"));
        req.data=r#"[{"id":42,"type":"book","title":{"invalid":"known standard field"}}]"#.into();
        assert_eq!(citations(&req).unwrap_err().code,"academic_bibliography");
    }
    const CUSTOM_QA_LOCALE:&str=r#"<locale xmlns="http://purl.org/net/xbiblio/csl" version="1.0" xml:lang="en-US"><terms><term name="translator" form="short"><single>qa-tr.</single><multiple>qa-trs.</multiple></term></terms></locale>"#;
    #[test]
    fn explicit_locale_xml_overrides_terms_keeps_fallbacks_and_checks_root_and_language() {
        let data=r#"[{"id":"t","type":"book","title":"Locale proof","translator":[{"family":"Reader","given":"Rae"}]}]"#;
        let mut req=request("csl-json",data);req.clusters=vec![vec![Cite{locator:Some("8".into()),..Cite::new("t")}]];
        req.csl=Some(probe_style("in-text",r#"<citation><layout><names variable="translator"><name form="short"/><label form="short" prefix=" "/></names><label variable="locator" form="short" prefix=" "/><text variable="locator" prefix=" "/></layout></citation>"#));
        let original=citations(&req).unwrap();assert!(!original["citations"][0]["text"].as_str().unwrap().contains("qa-tr."));
        req.locale_xml=Some(CUSTOM_QA_LOCALE.into());
        assert_eq!(citations(&req).unwrap()["citations"][0]["text"],"Reader qa-tr. p. 8");assert_eq!(req.data,data);
        let style=req.csl.clone().unwrap();
        req.csl=Some(style.replace("<citation>",r#"<locale xml:lang="en-US"><terms><term name="translator" form="short">house-tr.</term></terms></locale><citation>"#));
        assert_eq!(citations(&req).unwrap()["citations"][0]["text"],"Reader house-tr. p. 8");
        req.csl=Some(style);
        for xml in [CUSTOM_QA_LOCALE.replace("http://purl.org/net/xbiblio/csl","urn:wrong"),CUSTOM_QA_LOCALE.replace("en-US","fr-FR"),CUSTOM_QA_LOCALE.replace(" xml:lang=\"en-US\"",""),format!("{CUSTOM_QA_LOCALE}{CUSTOM_QA_LOCALE}"),format!("{CUSTOM_QA_LOCALE}trailing text"),format!("<!DOCTYPE locale [<!ENTITY x 'value'>]>{CUSTOM_QA_LOCALE}"),CUSTOM_QA_LOCALE.replace("<locale ","<style ").replace("</locale>","</style>")] {
            req.locale_xml=Some(xml);let error=citations(&req).unwrap_err();assert_eq!(error.code,"academic_context");
            assert!(matches!(error.shape, crate::error::ErrorShape::ApiCode { extras } if extras.iter().any(|(key,value)|key=="field"&&value=="locale_xml")));
        }
        req.locale_xml=Some(" ".repeat(256*1024+1));assert_eq!(citations(&req).unwrap_err().code,"academic_budget");
    }
    #[test]
    fn note_initial_terms_capitalize_without_changing_authored_text_or_formatting() {
        let mut req=request("csl-json",PROBE_ENTRIES);req.clusters=vec![vec![Cite::new("a")],vec![Cite::new("a")]];
        let style=probe_style("note",r#"<citation><layout><choose><if position="first"><text value="authored lower"/></if><else><text term="ibid" font-style="italic"/></else></choose></layout></citation>"#);req.csl=Some(style.clone());
        req.contexts=vec![CiteContext{note_number:Some(1),..Default::default()},CiteContext{note_number:Some(2),..Default::default()}];
        let output=citations(&req).unwrap();assert_eq!(output["citations"][0]["text"],"authored lower");assert_eq!(output["citations"][1]["text"],"Ibid.");
        let rich:Vec<RichNode>=serde_json::from_value(output["citations"][1]["rich"].clone()).unwrap();assert!(publication_rich::html(&rich).unwrap().contains("italic"));assert_eq!(publication_rich::plain(&rich),"Ibid.");
        req.contexts[1].note_number=Some(1);assert_eq!(citations(&req).unwrap()["citations"][1]["text"],"ibid.");
        req.contexts[1].note_number=Some(2);req.contexts[1].in_note=true;assert_eq!(citations(&req).unwrap()["citations"][1]["text"],"(ibid.)");
        req.contexts[1].in_note=false;req.clusters[1][0].prefix=Some("cf. ".into());assert_eq!(citations(&req).unwrap()["citations"][1]["text"],"cf. ibid.");
        req.clusters[1][0].prefix=None;req.csl=Some(style.replace("term=\"ibid\"","term=\"ibid\" text-case=\"lowercase\""));assert_eq!(citations(&req).unwrap()["citations"][1]["text"],"ibid.");
        req.csl=Some(probe_style("note",r#"<citation><layout><names variable="author"><name form="short"/></names><text term="ibid" prefix=" "/></layout></citation>"#));
        req.clusters=vec![vec![Cite{mode:CiteMode::Prose,prefix:Some("cf. ".into()),..Cite::new("a")}]];req.contexts.truncate(1);
        let output=citations(&req).unwrap();let inline:Vec<RichNode>=serde_json::from_value(output["citations"][0]["inline_rich"].clone()).unwrap();let note:Vec<RichNode>=serde_json::from_value(output["citations"][0]["note_rich"].clone()).unwrap();
        assert_eq!(publication_rich::plain(&inline),"cf. Lovelace");assert_eq!(publication_rich::plain(&note).trim(),"Ibid.");
    }
    #[test]
    fn same_note_near_position_includes_prior_items_in_the_same_cluster() {
        let mut req=request("csl-json",PROBE_ENTRIES);
        req.csl=Some(probe_style("note",r#"<citation near-note-distance="0"><layout delimiter="|"><choose><if position="ibid near-note" match="all"><text value="repeat:"/></if><else><text value="other:"/></else></choose><text variable="title"/></layout></citation>"#));
        req.clusters=vec![vec![Cite::new("a"),Cite::new("a")],vec![Cite::new("a")]];
        req.contexts=vec![CiteContext{note_number:Some(1),..Default::default()},CiteContext{note_number:Some(2),..Default::default()}];
        let output=citations(&req).unwrap();assert_eq!(output["citations"][0]["text"],"other:Analytical Engines|repeat:Analytical Engines");assert_eq!(output["citations"][1]["text"],"other:Analytical Engines");
        req.clusters[0].truncate(1);req.contexts[1].note_number=Some(1);assert_eq!(citations(&req).unwrap()["citations"][1]["text"],"repeat:Analytical Engines");
    }
    #[test]
    fn suppress_author_selects_the_real_primary_names_node_and_legal_substitutes() {
        let data=json!([
            {"id":"named","type":"book","title":"One","author":[{"family":"Aster"}],"editor":[{"family":"Beacon"}]},
            {"id":"edited","type":"book","title":"Two","editor":[{"family":"Cedar"}],"translator":[{"family":"Dahlia"}]},
            {"id":"anonymous","type":"book","title":"Anonymous work","translator":[{"family":"Elm"}]},
            {"id":"untitled","type":"book","translator":[{"family":"Fir"}]}
        ]).to_string();
        let mut req=request("csl-json",&data);req.clusters=["named","edited"].into_iter().map(|key|vec![Cite{mode:CiteMode::SuppressAuthor,..Cite::new(key)}]).collect();
        req.csl=Some(probe_style("in-text",r#"<citation><layout><group delimiter="+"><names variable="author"><name form="short"/></names><names variable="editor"><name form="short"/></names><names variable="translator"><name form="short"/></names></group></layout></citation>"#));
        let output=citations(&req).unwrap();assert_eq!(output["citations"][0]["text"],"Beacon");assert_eq!(output["citations"][1]["text"],"Dahlia");
        req.clusters=["edited","anonymous","untitled"].into_iter().map(|key|vec![Cite{mode:CiteMode::SuppressAuthor,..Cite::new(key)}]).collect();
        req.csl=Some(probe_style("in-text",r#"<citation><layout><group delimiter="+"><names variable="author"><name form="short"/><substitute><names variable="editor"/><text variable="title"/></substitute></names><names variable="translator"><name form="short"/></names></group></layout></citation>"#));
        let output=citations(&req).unwrap();for (index,expected) in ["Dahlia","Elm","Fir"].into_iter().enumerate(){assert_eq!(output["citations"][index]["text"],expected,"direct primary, item {index}");}
        // An empty author/substitute remains the same primary role when a
        // macro and a nested group prune it before the actual render call.
        req.csl=Some(probe_style("in-text",r#"<macro name="primary"><group><names variable="author"><name form="short"/><substitute><names variable="editor"/><text variable="title"/></substitute></names></group></macro><citation><layout><group delimiter="+"><text macro="primary"/><names variable="translator"><name form="short"/></names></group></layout></citation>"#));
        let output=citations(&req).unwrap();for (index,expected) in ["Dahlia","Elm","Fir"].into_iter().enumerate(){assert_eq!(output["citations"][index]["text"],expected,"nested macro primary, item {index}");}
        for cite in req.clusters.iter_mut().flatten(){cite.mode=CiteMode::Normal;}
        let output=citations(&req).unwrap();for (index,expected) in ["Cedar+Dahlia","Anonymous work+Elm","Fir"].into_iter().enumerate(){assert_eq!(output["citations"][index]["text"],expected,"normal mode, item {index}");}
        req.clusters=vec![vec![Cite{mode:CiteMode::SuppressAuthor,..Cite::new("edited")}]];
        req.csl=Some(probe_style("in-text",r#"<citation><layout><group delimiter="+"><choose><if type="article-journal"><names variable="author"><substitute><text variable="title"/></substitute></names></if><else><names variable="editor"><name form="short"/></names></else></choose><names variable="translator"><name form="short"/></names></group></layout></citation>"#));
        assert_eq!(citations(&req).unwrap()["citations"][0]["text"],"Dahlia","inactive author branch must not select a primary node");
    }
    #[test]
    fn year_suffix_ranges_keep_gaps_years_locators_and_affixes() {
        let mut data=(0..6).map(|index|json!({"id":format!("r{index}"),"type":"book","title":format!("Synthetic study {index}"),"author":[{"family":"Reed","given":"Robin"}],"issued":{"date-parts":[[if index==5{2027}else{2026}]]}})).collect::<Vec<_>>();
        let mut req=request("csl-json",&json!(data).to_string());
        req.csl=Some(probe_style("in-text",r#"<citation collapse="year-suffix-ranged" disambiguate-add-year-suffix="true" year-suffix-delimiter=","><layout delimiter=", "><group delimiter=" "><names variable="author"><name form="short"/></names><date variable="issued"><date-part name="year"/></date></group><text variable="year-suffix"/><text variable="locator" prefix="@"/></layout></citation>"#));
        let cluster=|indices:&[usize]|indices.iter().map(|index|Cite::new(format!("r{index}"))).collect::<Vec<_>>();
        req.clusters=vec![cluster(&[0,1,2,3,4,5]),cluster(&[0,2,3,4,5]),cluster(&[0,1]),cluster(&[0,1,2,3,4,5]),cluster(&[0,1,2,3,4,5])];
        req.clusters[3][2].locator=Some("8".into());req.clusters[4][0].prefix=Some("see ".into());req.clusters[4][4].suffix=Some(" note".into());
        let output=citations(&req).unwrap();
        for (index,expected) in ["Reed 2026a–e, 2027","Reed 2026a,c–e, 2027","Reed 2026a,b","Reed 2026a,b, 2026c@8, 2026d,e, 2027","see Reed 2026a–e note, Reed 2027"].into_iter().enumerate(){assert_eq!(output["citations"][index]["text"],expected,"cluster {index}");}
        // The first date may be event-date, rather than an assumed issued year.
        req.csl=req.csl.map(|style|style.replace("variable=\"issued\"","variable=\"event-date\""));
        for entry in &mut data { let year=entry.as_object_mut().unwrap().remove("issued").unwrap();entry["event-date"]=year; }
        req.data=json!(data).to_string();assert_eq!(citations(&req).unwrap()["citations"][0]["text"],"Reed 2026a–e, 2027");
        let mut other=data[5].clone();other["id"]=json!("r6");other["title"]=json!("Another annual study");data.push(other);req.data=json!(data).to_string();
        req.clusters=vec![cluster(&[5,6]),cluster(&[0,6])];
        assert_eq!(citations(&req).unwrap()["citations"][1]["text"],"Reed 2026, 2027b");
    }
}
