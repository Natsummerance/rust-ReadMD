//! Bounded publication metadata and source-preserving editor transactions.
//! No file I/O, constructors, network, citation processing or PDF rendering.
use crate::document_transaction::{self as transaction, ContentTransaction, TextEdit};
use crate::md_ast::{Block, Inline};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use yaml_rust2::{
    parser::{Event, Parser},
    scanner::TScalarStyle,
};

pub const MAX_FRONT_MATTER: usize = 64 * 1024;
const MAX_DEPTH: usize = 16;
const MAX_NODES: usize = 4096;
const FIELDS: &[&str] = &[
    "title",
    "subtitle",
    "authors",
    "affiliations",
    "translators",
    "abstracts",
    "keywords",
    "funding",
    "publisher",
    "date",
    "rights",
    "edition",
    "series",
    "identifiers",
    "language",
    "subject",
];
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MetadataError {
    pub code: String,
    pub path: String,
    pub from: usize,
    pub to: usize,
}
impl std::fmt::Display for MetadataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.path)
    }
}
impl std::error::Error for MetadataError {}
impl MetadataError {
    pub fn response_body(&self) -> Value {
        json!({"ok":false,"error_code":self.code,"path":self.path,"field":self.path,"from":self.from,"to":self.to})
    }
}
type Result<T> = std::result::Result<T, MetadataError>;
fn err(code: &str, path: &str) -> MetadataError {
    MetadataError {
        code: code.into(),
        path: path.into(),
        from: 0,
        to: 0,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Person {
    pub id: String,
    pub name: String,
    pub given: Option<String>,
    pub family: Option<String>,
    pub file_as: Option<String>,
    pub orcid: Option<String>,
    pub corresponding: bool,
    pub affiliation_ids: Vec<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Institution {
    pub id: String,
    pub name: String,
    pub department: Option<String>,
    pub address: Option<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Funding {
    pub funder: String,
    pub award: Option<String>,
    pub recipient_ids: Vec<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Identifier {
    pub scheme: String,
    pub value: String,
    pub primary: bool,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Abstract {
    pub text: String,
    pub language: Option<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Series {
    pub title: String,
    pub number: Option<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct PublicationMetadata {
    pub title: String,
    pub subtitle: Option<String>,
    pub authors: Vec<Person>,
    pub affiliations: Vec<Institution>,
    pub translators: Vec<Person>,
    pub abstracts: Vec<Abstract>,
    pub keywords: Vec<String>,
    pub funding: Vec<Funding>,
    pub publisher: Option<String>,
    pub date: Option<String>,
    pub rights: Option<String>,
    pub edition: Option<String>,
    pub series: Option<Series>,
    pub identifiers: Vec<Identifier>,
    pub language: Option<String>,
    pub subject: Option<String>,
    pub declared: bool,
    pub present_fields: BTreeSet<String>,
    pub cleared_fields: BTreeSet<String>,
    pub warning_items: Vec<Value>,
}
impl PublicationMetadata {
    pub fn author_text(&self) -> String {
        self.authors
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
            .join("; ")
    }
    pub fn translator_text(&self) -> String {
        self.translators
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
            .join("; ")
    }
    pub fn primary_identifier(&self) -> Option<&Identifier> {
        self.identifiers
            .iter()
            .find(|i| i.primary)
            .or_else(|| self.identifiers.first())
    }
}

#[derive(Debug, Clone)]
struct Entry {
    key: String,
    key_start: usize,
    node: Node,
}
#[derive(Debug, Clone)]
enum Kind {
    Scalar,
    Array(Vec<Node>),
    Object(Vec<Entry>),
}
#[derive(Debug, Clone)]
struct Node {
    value: Value,
    start: usize,
    end: usize,
    flow: bool,
    kind: Kind,
}
struct Front<'a> {
    source: &'a str,
    body: &'a str,
    body_from: usize,
    closing: usize,
    end: usize,
    eol: &'a str,
}
fn front(source: &str) -> Result<Option<Front<'_>>> {
    if source.len() > transaction::MAX_DOCUMENT_BYTES {
        return Err(err("publication_metadata_budget", "content"));
    }
    let bom = if source.starts_with('\u{feff}') { 3 } else { 0 };
    let text = &source[bom..];
    let first = text.split_inclusive('\n').next().unwrap_or("");
    if first.trim_end_matches(['\r', '\n', ' ', '\t']) != "---" || !first.ends_with('\n') {
        return Ok(None);
    }
    let start = bom + first.len();
    let mut offset = start;
    for line in source[start..].split_inclusive('\n') {
        if matches!(
            line.trim_end_matches(['\r', '\n', ' ', '\t']),
            "---" | "..."
        ) {
            if offset - start > MAX_FRONT_MATTER {
                return Err(err("publication_metadata_budget", "frontMatter"));
            }
            return Ok(Some(Front {
                source,
                body: &source[start..offset],
                body_from: start,
                closing: offset,
                end: offset + line.len(),
                eol: if first.ends_with("\r\n") {
                    "\r\n"
                } else {
                    "\n"
                },
            }));
        }
        offset += line.len();
    }
    Ok(None)
}
pub(crate) fn bibliography_sources(source: &str) -> Result<Vec<String>> {
    let Some(front) = front(source)? else {
        return Ok(Vec::new());
    };
    let root = parse_yaml(front.body)?;
    let value = root.value.get("bibliography").unwrap_or(&Value::Null);
    if value.is_null() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for (index, value) in items(value, "bibliography", 8)?.into_iter().enumerate() {
        if let Some(value) = text(value, &format!("bibliography[{index}]"))? {
            if !value.is_empty() {
                result.push(value);
            }
        }
    }
    Ok(result)
}
fn utf16(source: &str, byte: usize) -> usize {
    source[..byte.min(source.len())].encode_utf16().count()
}
fn canonical(key: &str) -> &str {
    match key {
        "author" => "authors",
        "institution" | "institutions" | "affiliation" => "affiliations",
        "translator" => "translators",
        "abstract" => "abstracts",
        "isbn" | "identifier" => "identifiers",
        "lang" => "language",
        _ => key,
    }
}
fn metadata_key(key: &str) -> bool {
    FIELDS.contains(&canonical(key))
}
fn value_text(v: &Value, path: &str, max: usize) -> Result<Option<String>> {
    if v.is_null() {
        return Ok(None);
    }
    let Some(s) = v.as_str() else {
        return Err(err("publication_metadata_type", path));
    };
    if s.len() > max
        || s.chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err(err("publication_metadata_budget", path));
    }
    Ok(if s.is_empty() { None } else { Some(s.into()) })
}
fn text(v: &Value, path: &str) -> Result<Option<String>> {
    value_text(v, path, 4096)
}
fn required(v: &Value, path: &str) -> Result<String> {
    text(v, path)?
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| err("publication_metadata_required", path))
}
fn boolean(v: &Value, path: &str) -> Result<bool> {
    if v.is_null() {
        return Ok(false);
    }
    v.as_bool()
        .or_else(|| {
            v.as_str().and_then(|s| match s {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            })
        })
        .ok_or_else(|| err("publication_metadata_type", path))
}
fn items<'a>(v: &'a Value, path: &str, max: usize) -> Result<Vec<&'a Value>> {
    let values = if v.is_null() {
        Vec::new()
    } else if let Some(a) = v.as_array() {
        a.iter().collect()
    } else {
        vec![v]
    };
    if values.len() > max {
        return Err(err("publication_metadata_budget", path));
    }
    Ok(values)
}
fn strings(v: &Value, path: &str, max: usize) -> Result<Vec<String>> {
    items(v, path, max)?
        .into_iter()
        .enumerate()
        .map(|(i, v)| required(v, &format!("{path}[{i}]")))
        .collect()
}
fn stable_id(role: &str, name: &str, occurrence: usize) -> String {
    let digest = Sha256::digest(format!("{role}\0{name}\0{occurrence}").as_bytes());
    format!(
        "{role}-{}",
        digest[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}

// YAML events retain lexical strings (including identifiers with leading
// zeros). Only explicit plain null is typed here; booleans are checked at the
// few fields that accept them. Nothing is constructed or expanded.
/// Safe read-only extension access, reusing the existing YAML budgets, alias/
/// tag rejection and lexical identifier preservation. Only unquoted literal
/// booleans are materialized for typed extension requests.
pub(crate) fn bounded_front_matter_member(body:&str,keys:&[&str])->Result<Option<(Value,usize,usize)>> {
    fn value(node:&Node,body:&str)->Value {
        match &node.kind {
            Kind::Object(entries)=>Value::Object(entries.iter().map(|e|(e.key.clone(),value(&e.node,body))).collect()),
            Kind::Array(nodes)=>Value::Array(nodes.iter().map(|n|value(n,body)).collect()),
            Kind::Scalar=>match body[node.start..node.end].trim(){"true"=>Value::Bool(true),"false"=>Value::Bool(false),_=>node.value.clone()},
        }
    }
    let root=parse_yaml(body)?;let mut node=&root;
    for key in keys {let Kind::Object(entries)=&node.kind else{return Ok(None);};let Some(entry)=entries.iter().find(|e|e.key==*key)else{return Ok(None);};node=&entry.node;}
    // An implicit YAML MappingStart points at the first key's colon. The
    // lexical object value starts at that key; explicit flow/empty mappings
    // retain their opening delimiter, and scalar/array spans stay unchanged.
    let start=match &node.kind{Kind::Object(entries)=>entries.first().map_or(node.start,|entry|node.start.min(entry.key_start)),_=>node.start};
    Ok(Some((value(node,body),utf16(body,start),utf16(body,node.end))))
}

fn parse_yaml(body: &str) -> Result<Node> {
    if body.len() > MAX_FRONT_MATTER {
        return Err(err("publication_metadata_budget", "frontMatter"));
    }
    // 0.13's scanner increments index per char, except its block-scalar fast
    // path increments bytes. Line/column marks at token starts avoid that
    // mixed-unit index (columns are reset after every line break).
    let lines = std::iter::once(0)
        .chain(body.match_indices('\n').map(|(i, _)| i + 1))
        .collect::<Vec<_>>();
    let position = |mark: &yaml_rust2::scanner::Marker| {
        let start = lines
            .get(mark.line().saturating_sub(1))
            .copied()
            .unwrap_or(body.len());
        let line = body[start..].split('\n').next().unwrap_or("");
        start
            + line
                .char_indices()
                .nth(mark.col())
                .map(|(i, _)| i)
                .unwrap_or(line.len())
    };
    let mut parser = Parser::new_from_str(body);
    let mut events = Vec::new();
    let mut documents = 0;
    loop {
        let (event, mark) = parser.next_token().map_err(|e| {
            let mut error = err("publication_metadata_yaml", "frontMatter");
            error.from = utf16(body, position(e.marker()));
            error.to = error.from;
            error
        })?;
        if events.len() >= MAX_NODES {
            return Err(err("publication_metadata_budget", "frontMatter"));
        }
        let at = position(&mark);
        match &event {
            Event::DocumentStart => {
                documents += 1;
                if documents > 1 {
                    return Err(err("publication_metadata_yaml", "frontMatter"));
                }
            }
            Event::Alias(_) => return Err(err("publication_metadata_yaml_feature", "frontMatter")),
            Event::Scalar(_, _, anchor, tag)
            | Event::SequenceStart(anchor, tag)
            | Event::MappingStart(anchor, tag)
                if *anchor != 0 || tag.is_some() =>
            {
                return Err(err("publication_metadata_yaml_feature", "frontMatter"))
            }
            _ => {}
        }
        let done = matches!(event, Event::StreamEnd);
        events.push((event, at));
        if done {
            break;
        }
    }
    fn node(
        events: &[(Event, usize)],
        cursor: &mut usize,
        body: &str,
        depth: usize,
        in_flow: bool,
    ) -> Result<Node> {
        if depth > MAX_DEPTH {
            return Err(err("publication_metadata_budget", "frontMatter"));
        }
        while matches!(
            events.get(*cursor).map(|e| &e.0),
            Some(Event::StreamStart | Event::DocumentStart)
        ) {
            *cursor += 1;
        }
        let (event, start) = events
            .get(*cursor)
            .ok_or_else(|| err("publication_metadata_yaml", "frontMatter"))?;
        let start = *start;
        *cursor += 1;
        match event {
            Event::Scalar(s, style, _, _) => {
                let end = scalar_end(
                    body,
                    start,
                    events.get(*cursor).map(|e| e.1).unwrap_or(body.len()),
                    *style,
                    in_flow,
                );
                Ok(Node {
                    value: if *style == TScalarStyle::Plain
                        && matches!(s.as_str(), "null" | "Null" | "NULL" | "~")
                    {
                        Value::Null
                    } else {
                        json!(s)
                    },
                    start,
                    end,
                    flow: in_flow,
                    kind: Kind::Scalar,
                })
            }
            Event::SequenceStart(_, _) => {
                let flow = body.as_bytes().get(start) == Some(&b'[');
                let mut children = Vec::new();
                while !matches!(events.get(*cursor).map(|e| &e.0), Some(Event::SequenceEnd)) {
                    children.push(node(events, cursor, body, depth + 1, flow)?);
                }
                let boundary = events[*cursor].1;
                *cursor += 1;
                let end = if flow {
                    boundary + 1
                } else {
                    children.last().map(|n| n.end).unwrap_or(start)
                };
                Ok(Node {
                    value: Value::Array(children.iter().map(|n| n.value.clone()).collect()),
                    start,
                    end: end.min(body.len()),
                    flow,
                    kind: Kind::Array(children),
                })
            }
            Event::MappingStart(_, _) => {
                let flow = body.as_bytes().get(start) == Some(&b'{');
                let mut entries = Vec::new();
                let mut values = Map::new();
                while !matches!(events.get(*cursor).map(|e| &e.0), Some(Event::MappingEnd)) {
                    let key = node(events, cursor, body, depth + 1, flow)?;
                    let name = key
                        .value
                        .as_str()
                        .ok_or_else(|| err("publication_metadata_yaml", "frontMatter"))?
                        .to_owned();
                    let value = node(events, cursor, body, depth + 1, flow)?;
                    if values.insert(name.clone(), value.value.clone()).is_some() {
                        return Err(err("publication_metadata_duplicate", &name));
                    }
                    entries.push(Entry {
                        key: name,
                        key_start: key.start,
                        node: value,
                    });
                }
                let boundary = events[*cursor].1;
                *cursor += 1;
                let end = if flow {
                    boundary + 1
                } else {
                    entries.last().map(|e| e.node.end).unwrap_or(start)
                };
                Ok(Node {
                    value: Value::Object(values),
                    start,
                    end: end.min(body.len()),
                    flow,
                    kind: Kind::Object(entries),
                })
            }
            _ => Err(err("publication_metadata_yaml", "frontMatter")),
        }
    }
    if body
        .lines()
        .all(|line| line.trim().is_empty() || line.trim_start().starts_with('#'))
    {
        return Ok(Node {
            value: json!({}),
            start: 0,
            end: 0,
            flow: false,
            kind: Kind::Object(Vec::new()),
        });
    }
    let mut cursor = 0;
    let root = node(&events, &mut cursor, body, 0, false)?;
    if !root.value.is_object() {
        return Err(err("publication_metadata_type", "frontMatter"));
    }
    Ok(root)
}
fn scalar_end(body: &str, start: usize, boundary: usize, style: TScalarStyle, flow: bool) -> usize {
    let mut limit = boundary.max(start).min(body.len());
    let bytes = body.as_bytes();
    if !flow {
        let line = line_start(body, limit);
        let prefix = &body[line..limit];
        if line > start && (prefix.trim().is_empty() || prefix.trim() == "-") {
            limit = line;
        }
    }
    if matches!(
        style,
        TScalarStyle::SingleQuoted | TScalarStyle::DoubleQuoted
    ) {
        let quote = if style == TScalarStyle::SingleQuoted {
            b'\''
        } else {
            b'"'
        };
        let mut i = start + 1;
        while i < body.len() {
            if quote == b'"' && bytes[i] == b'\\' {
                i += 2;
                continue;
            }
            if bytes[i] == quote {
                if quote == b'\'' && bytes.get(i + 1) == Some(&quote) {
                    i += 2;
                    continue;
                }
                return i + 1;
            }
            i += 1;
        }
        return limit;
    }
    let mut end = start;
    let mut line_start = start;
    for line in body[start..limit].split_inclusive('\n') {
        let mut n = line.trim_end_matches(['\r', '\n']).len();
        if !matches!(style, TScalarStyle::Literal | TScalarStyle::Folded) {
            for (i, c) in line.char_indices() {
                if (c == '#' && (i == 0 || line[..i].ends_with(char::is_whitespace)))
                    || (flow && matches!(c, ',' | ']' | '}'))
                {
                    n = n.min(i);
                    break;
                }
            }
        }
        let meaningful = line[..n].trim_end();
        if !meaningful.is_empty() {
            end = line_start + meaningful.len();
        }
        line_start += line.len();
    }
    end
}

fn field<'a>(object: &'a Value, key: &str) -> &'a Value {
    object.get(key).unwrap_or(&Value::Null)
}
fn id(value: &Value, path: &str, fallback: String) -> Result<String> {
    let value = text(value, path)?.unwrap_or(fallback);
    if value.is_empty() || value.len() > 128 || value.chars().any(char::is_whitespace) {
        return Err(err("publication_metadata_id", path));
    }
    Ok(value)
}
fn orcid_valid(value: &str) -> bool {
    let value = value
        .strip_prefix("https://orcid.org/")
        .or_else(|| value.strip_prefix("http://orcid.org/"))
        .unwrap_or(value);
    let bytes = value.as_bytes();
    if bytes.len() != 19 || [4, 9, 14].into_iter().any(|i| bytes[i] != b'-') {
        return false;
    }
    let digits: Vec<_> = bytes.iter().copied().filter(|b| *b != b'-').collect();
    if !digits[..15].iter().all(u8::is_ascii_digit) {
        return false;
    }
    let total = digits[..15]
        .iter()
        .fold(0u32, |total, b| (total + u32::from(*b - b'0')) * 2);
    let check = (12 - total % 11) % 11;
    digits[15]
        == if check == 10 {
            b'X'
        } else {
            b'0' + check as u8
        }
}
fn isbn_valid(value: &str) -> bool {
    // Separators are ignored for checking only; the original string is kept.
    let value = value
        .strip_prefix("ISBN ")
        .or_else(|| value.strip_prefix("ISBN-"))
        .unwrap_or(value);
    let digits: Vec<_> = value
        .bytes()
        .filter(|b| !matches!(*b, b' ' | b'-'))
        .collect();
    match digits.len() {
        10 => {
            digits[..9].iter().all(u8::is_ascii_digit)
                && matches!(digits[9], b'0'..=b'9' | b'X')
                && digits
                    .iter()
                    .enumerate()
                    .map(|(i, b)| {
                        (10 - i) as u32 * if *b == b'X' { 10 } else { u32::from(*b - b'0') }
                    })
                    .sum::<u32>()
                    % 11
                    == 0
        }
        13 => {
            digits.iter().all(u8::is_ascii_digit)
                && digits
                    .iter()
                    .enumerate()
                    .map(|(i, b)| u32::from(*b - b'0') * if i % 2 == 0 { 1 } else { 3 })
                    .sum::<u32>()
                    % 10
                    == 0
        }
        _ => false,
    }
}
fn people(value: &Value, role: &str) -> Result<Vec<Person>> {
    let mut names = BTreeMap::<String, usize>::new();
    items(value, role, 32)?
        .into_iter()
        .enumerate()
        .map(|(i, v)| {
            let path = format!("{role}[{i}]");
            if !v.is_string() && !v.is_object() {
                return Err(err("publication_metadata_type", &path));
            }
            let name = required(
                if v.is_string() { v } else { field(v, "name") },
                &format!("{path}.name"),
            )?;
            let occurrence = names.entry(name.clone()).or_default();
            let fallback = stable_id(role.trim_end_matches('s'), &name, *occurrence);
            *occurrence += 1;
            let orcid = text(field(v, "orcid"), &format!("{path}.orcid"))?;
            if orcid.as_deref().is_some_and(|s| !orcid_valid(s)) {
                return Err(err("publication_metadata_orcid", &format!("{path}.orcid")));
            }
            Ok(Person {
                id: id(field(v, "id"), &format!("{path}.id"), fallback)?,
                name,
                given: text(field(v, "given"), &format!("{path}.given"))?,
                family: text(field(v, "family"), &format!("{path}.family"))?,
                file_as: text(field(v, "fileAs"), &format!("{path}.fileAs"))?,
                orcid,
                corresponding: boolean(
                    field(v, "corresponding"),
                    &format!("{path}.corresponding"),
                )?,
                affiliation_ids: strings(
                    field(v, "affiliationIds"),
                    &format!("{path}.affiliationIds"),
                    32,
                )?,
            })
        })
        .collect()
}
fn institutions(value: &Value) -> Result<Vec<Institution>> {
    let mut names = BTreeMap::<String, usize>::new();
    items(value, "affiliations", 32)?
        .into_iter()
        .enumerate()
        .map(|(i, v)| {
            let p = format!("affiliations[{i}]");
            if !v.is_string() && !v.is_object() {
                return Err(err("publication_metadata_type", &p));
            }
            let name = required(
                if v.is_string() { v } else { field(v, "name") },
                &format!("{p}.name"),
            )?;
            let occurrence = names.entry(name.clone()).or_default();
            let fallback = stable_id("institution", &name, *occurrence);
            *occurrence += 1;
            Ok(Institution {
                id: id(field(v, "id"), &format!("{p}.id"), fallback)?,
                name,
                department: text(field(v, "department"), &format!("{p}.department"))?,
                address: text(field(v, "address"), &format!("{p}.address"))?,
            })
        })
        .collect()
}
fn abstracts(value: &Value) -> Result<Vec<Abstract>> {
    // A language keyed map is accepted in addition to scalar/object/list.
    if let Some(map) = value.as_object().filter(|map| !map.contains_key("text")) {
        if map.len() > 8 {
            return Err(err("publication_metadata_budget", "abstracts"));
        }
        return map
            .iter()
            .map(|(language, value)| {
                Ok(Abstract {
                    text: value_text(value, &format!("abstracts.{language}"), 16 * 1024)?
                        .ok_or_else(|| {
                            err(
                                "publication_metadata_required",
                                &format!("abstracts.{language}"),
                            )
                        })?,
                    language: Some(
                        crate::md_ast::publication_language_tag(language).ok_or_else(|| {
                            err(
                                "publication_metadata_language",
                                &format!("abstracts.{language}"),
                            )
                        })?,
                    ),
                })
            })
            .collect();
    }
    items(value, "abstracts", 8)?
        .into_iter()
        .enumerate()
        .map(|(i, v)| {
            let p = format!("abstracts[{i}]");
            if !v.is_string() && !v.is_object() {
                return Err(err("publication_metadata_type", &p));
            }
            let text = value_text(
                if v.is_string() { v } else { field(v, "text") },
                &format!("{p}.text"),
                16 * 1024,
            )?
            .filter(|s| !s.is_empty())
            .ok_or_else(|| err("publication_metadata_required", &format!("{p}.text")))?;
            let language = text_language(field(v, "language"), &format!("{p}.language"))?;
            Ok(Abstract { text, language })
        })
        .collect()
}
fn text_language(value: &Value, path: &str) -> Result<Option<String>> {
    text(value, path)?
        .map(|s| {
            crate::md_ast::publication_language_tag(&s)
                .ok_or_else(|| err("publication_metadata_language", path))
        })
        .transpose()
}
fn funds(value: &Value) -> Result<Vec<Funding>> {
    items(value, "funding", 16)?
        .into_iter()
        .enumerate()
        .map(|(i, v)| {
            let p = format!("funding[{i}]");
            if !v.is_string() && !v.is_object() {
                return Err(err("publication_metadata_type", &p));
            }
            Ok(Funding {
                funder: required(
                    if v.is_string() { v } else { field(v, "funder") },
                    &format!("{p}.funder"),
                )?,
                award: text(field(v, "award"), &format!("{p}.award"))?,
                recipient_ids: strings(field(v, "recipientIds"), &format!("{p}.recipientIds"), 32)?,
            })
        })
        .collect()
}
fn identifiers(value: &Value, isbn: bool) -> Result<Vec<Identifier>> {
    let ids = items(value, "identifiers", 16)?
        .into_iter()
        .enumerate()
        .map(|(i, v)| {
            let p = format!("identifiers[{i}]");
            let (scheme, value, primary) = if v.is_string() {
                (
                    if isbn { "isbn" } else { "uri" }.to_owned(),
                    required(v, &format!("{p}.value"))?,
                    false,
                )
            } else if v.is_object() {
                (
                    required(field(v, "scheme"), &format!("{p}.scheme"))?,
                    required(field(v, "value"), &format!("{p}.value"))?,
                    boolean(field(v, "primary"), &format!("{p}.primary"))?,
                )
            } else {
                return Err(err("publication_metadata_type", &p));
            };
            if scheme.eq_ignore_ascii_case("orcid") {
                return Err(err(
                    "publication_metadata_identifier_role",
                    &format!("{p}.scheme"),
                ));
            }
            if scheme.eq_ignore_ascii_case("isbn") && !isbn_valid(&value) {
                return Err(err("publication_metadata_isbn", &format!("{p}.value")));
            }
            Ok(Identifier {
                scheme,
                value,
                primary,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    if ids.iter().filter(|i| i.primary).count() > 1 {
        return Err(err("publication_metadata_primary", "identifiers"));
    }
    Ok(ids)
}
fn normalize_field(key: &str, value: &Value, original: &str) -> Result<Value> {
    Ok(match key {
        "authors" | "translators" => json!(people(value, key)?),
        "affiliations" => json!(institutions(value)?),
        "abstracts" => json!(abstracts(value)?),
        "keywords" => json!(strings(value, key, 64)?),
        "funding" => json!(funds(value)?),
        "identifiers" => json!(identifiers(value, original == "isbn")?),
        "series" => {
            if value.is_null() {
                Value::Null
            } else if value.is_string() {
                json!(Series {
                    title: required(value, "series.title")?,
                    number: None
                })
            } else if value.is_object() {
                json!(Series {
                    title: required(field(value, "title"), "series.title")?,
                    number: text(field(value, "number"), "series.number")?
                })
            } else {
                return Err(err("publication_metadata_type", "series"));
            }
        }
        "language" => json!(text_language(value, key)?),
        _ => json!(text(value, key)?),
    })
}
fn source_values(root: &Node) -> Result<Map<String, Value>> {
    let mut result = Map::new();
    let Kind::Object(entries) = &root.kind else {
        return Err(err("publication_metadata_type", "frontMatter"));
    };
    for entry in entries
        .iter()
        .filter(|e| metadata_key(&e.key) && !matches!(e.key.as_str(), "lang" | "language"))
    {
        let key = canonical(&entry.key);
        let value = normalize_field(key, &entry.node.value, &entry.key)?;
        if result.get(key).is_some_and(|old| old != &value) {
            return Err(err("publication_metadata_alias_conflict", key));
        }
        result.insert(key.into(), value);
    }
    // Keep P02's lang/language precedence and invalid-tag fallback.
    for key in ["lang", "language"] {
        if let Some(entry) = entries.iter().find(|e| e.key == key) {
            let raw = text(&entry.node.value, "language")?;
            if raw.is_none() {
                result.entry("language").or_insert(Value::Null);
            } else if let Some(tag) = raw
                .as_deref()
                .and_then(crate::md_ast::publication_language_tag)
            {
                result.insert("language".into(), json!(tag));
                break;
            }
        }
    }
    Ok(result)
}
fn overlay_values(layer: &Value) -> Result<(Map<String, Value>, BTreeSet<String>)> {
    let Some(meta) = layer.get("meta") else {
        return Ok((Map::new(), BTreeSet::new()));
    };
    if meta.is_null() {
        return Ok((Map::new(), BTreeSet::new()));
    }
    let Some(object) = meta.as_object() else {
        return Err(err("publication_metadata_type", "meta"));
    };
    let mut clears = BTreeSet::new();
    for key in strings(field(meta, "clear"), "meta.clear", FIELDS.len())? {
        if !FIELDS.contains(&key.as_str()) {
            return Err(err(
                "publication_metadata_field",
                &format!("meta.clear.{key}"),
            ));
        }
        clears.insert(key);
    }
    let mut values = Map::new();
    for (raw, value) in object {
        if !metadata_key(raw)
            || matches!(raw.as_str(), "language" | "lang")
            || value.is_null()
            || matches!(raw.as_str(), "title" | "author" | "subject") && value.as_str() == Some("")
        {
            continue;
        }
        let key = canonical(raw);
        let value = normalize_field(key, value, raw)?;
        if values.get(key).is_some_and(|v| v != &value) {
            return Err(err(
                "publication_metadata_alias_conflict",
                &format!("meta.{key}"),
            ));
        }
        values.insert(key.into(), value);
    }
    for raw in ["language", "lang"] {
        if let Some(value) = object.get(raw).filter(|v| !v.is_null()) {
            if let Some(tag) = text(value, "meta.language")?
                .as_deref()
                .and_then(crate::md_ast::publication_language_tag)
            {
                values.insert("language".into(), json!(tag));
                break;
            }
        }
    }
    if let Some(key) = clears.iter().find(|key| values.contains_key(key.as_str())) {
        return Err(err(
            "publication_metadata_clear_conflict",
            &format!("meta.{key}"),
        ));
    }
    Ok((values, std::mem::take(&mut clears)))
}
fn is_empty(value: &Value) -> bool {
    value.is_null() || value.as_str() == Some("") || value.as_array().is_some_and(Vec::is_empty)
}
fn metadata(
    values: Map<String, Value>,
    present: BTreeSet<String>,
    cleared: BTreeSet<String>,
    source_name: &str,
    complete: bool,
) -> Result<PublicationMetadata> {
    let value = Value::Object(values);
    let fallback = source_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(source_name);
    let fallback = fallback
        .rsplit_once('.')
        .map(|(name, _)| name)
        .unwrap_or(fallback);
    let mut result = PublicationMetadata {
        title: text(field(&value, "title"), "title")?.unwrap_or_else(|| {
            if fallback.is_empty() {
                "Document".into()
            } else {
                fallback.chars().take(4096).collect()
            }
        }),
        subtitle: text(field(&value, "subtitle"), "subtitle")?,
        authors: people(field(&value, "authors"), "authors")?,
        affiliations: institutions(field(&value, "affiliations"))?,
        translators: people(field(&value, "translators"), "translators")?,
        abstracts: abstracts(field(&value, "abstracts"))?,
        keywords: strings(field(&value, "keywords"), "keywords", 64)?,
        funding: funds(field(&value, "funding"))?,
        publisher: text(field(&value, "publisher"), "publisher")?,
        date: text(field(&value, "date"), "date")?,
        rights: text(field(&value, "rights"), "rights")?,
        edition: text(field(&value, "edition"), "edition")?,
        series: if field(&value, "series").is_null() {
            None
        } else {
            Some(Series {
                title: required(field(field(&value, "series"), "title"), "series.title")?,
                number: text(field(field(&value, "series"), "number"), "series.number")?,
            })
        },
        identifiers: identifiers(field(&value, "identifiers"), false)?,
        language: text_language(field(&value, "language"), "language")?,
        subject: text(field(&value, "subject"), "subject")?,
        declared: !present.is_empty(),
        present_fields: present,
        cleared_fields: cleared,
        warning_items: Vec::new(),
    };
    let mut person_ids = BTreeSet::new();
    for (role, people) in [
        ("authors", &result.authors),
        ("translators", &result.translators),
    ] {
        for (i, p) in people.iter().enumerate() {
            if !person_ids.insert(&p.id) {
                return Err(err(
                    "publication_metadata_duplicate_id",
                    &format!("{role}[{i}].id"),
                ));
            }
        }
    }
    let mut institution_ids = BTreeSet::new();
    for (i, a) in result.affiliations.iter().enumerate() {
        if !institution_ids.insert(&a.id) {
            return Err(err(
                "publication_metadata_duplicate_id",
                &format!("affiliations[{i}].id"),
            ));
        }
    }
    for (role, people) in [
        ("authors", &result.authors),
        ("translators", &result.translators),
    ] {
        for (i, p) in people.iter().enumerate() {
            let mut seen = BTreeSet::new();
            for (j, id) in p.affiliation_ids.iter().enumerate() {
                if ((complete || result.present_fields.contains("affiliations")) && !institution_ids.contains(id)) || !seen.insert(id) {
                    return Err(err(
                        "publication_metadata_reference",
                        &format!("{role}[{i}].affiliationIds[{j}]"),
                    ));
                }
            }
        }
    }
    let authors: BTreeSet<_> = result.authors.iter().map(|p| &p.id).collect();
    for (i, f) in result.funding.iter().enumerate() {
        let mut seen = BTreeSet::new();
        for (j, id) in f.recipient_ids.iter().enumerate() {
            if ((complete || result.present_fields.contains("authors")) && !authors.contains(id)) || !seen.insert(id) {
                return Err(err(
                    "publication_metadata_reference",
                    &format!("funding[{i}].recipientIds[{j}]"),
                ));
            }
        }
    }
    if result.declared && !result.present_fields.contains("title") {
        result
            .warning_items
            .push(json!({"code":"publication_metadata_title_fallback","path":"title"}));
    }
    Ok(result)
}
/// A saved preset is sparse: references to inherited people/institutions are
/// validated when it is applied to a document. Explicitly supplied entities,
/// duplicate IDs, malformed values and contradictory clear states are checked now.
pub fn validate_options(options: &Value) -> Result<()> {
    let (values, clears)=overlay_values(options)?;
    let present=values.keys().cloned().chain(clears.iter().cloned()).collect();
    metadata(values,present,clears,"",false).map(|_|())
}
/// Resolve source -> preset -> request. Format-specific callers supply their
/// compatibility metadata as the final layer; this function never leaks EPUB
/// overrides into another format.
pub fn resolve_layers(
    front_matter: Option<&str>,
    layers: &[Value],
    source_name: &str,
) -> Result<PublicationMetadata> {
    let root = front_matter.map(parse_yaml).transpose()?;
    let mut values = if let Some(root) = &root {
        source_values(root).map_err(|e| locate(e, root, front_matter.unwrap_or(""), 0))?
    } else {
        Map::new()
    };
    let mut present: BTreeSet<_> = values.keys().cloned().collect();
    let mut cleared: BTreeSet<_> = values
        .iter()
        .filter(|(_, v)| is_empty(v))
        .map(|(k, _)| k.clone())
        .collect();
    let mut overridden = BTreeSet::new();
    for layer in layers {
        let (over, clear) = overlay_values(layer)?;
        for key in clear {
            overridden.insert(key.clone());
            values.insert(key.clone(), Value::Null);
            present.insert(key.clone());
            cleared.insert(key);
        }
        for (key, value) in over {
            overridden.insert(key.clone());
            present.insert(key.clone());
            if is_empty(&value) {
                cleared.insert(key.clone());
            } else {
                cleared.remove(&key);
            }
            values.insert(key, value);
        }
    }
    let result = metadata(values, present, cleared, source_name, true).map(|mut metadata| {
        let mut options = json!({});
        for layer in layers {
            if let Some(front) = layer.get("frontMatter") {
                options["frontMatter"] = crate::export_styles::deep_merge(
                    options.get("frontMatter").unwrap_or(&json!({})),
                    front,
                );
            }
        }
        append_label_warnings(&mut metadata, &options);
        attach_warning_ranges(
            &mut metadata.warning_items,
            root.as_ref(),
            front_matter.unwrap_or(""),
            &overridden,
        );
        metadata
    });
    result.map_err(|e| {
        let top=path_components(&e.path).into_iter().next().unwrap_or_default();
        if overridden.contains(&top) {
            e
        } else if let Some(root) = &root {
            locate(e, root, front_matter.unwrap_or(""), 0)
        } else {
            e
        }
    })
}
pub fn resolve(
    front_matter: Option<&str>,
    options: &Value,
    source_name: &str,
) -> Result<PublicationMetadata> {
    resolve_layers(front_matter, std::slice::from_ref(options), source_name)
}
pub fn from_source(
    content: &str,
    options: &Value,
    source_name: &str,
) -> Result<PublicationMetadata> {
    let fm = front(content)?;
    resolve(fm.as_ref().map(|f| f.body), options, source_name)
        .map(|mut metadata| {
            if let Some(f) = &fm {
                let offset = utf16(content, f.body_from);
                for warning in &mut metadata.warning_items {
                    if warning["coordinateSpace"] == "frontMatter" {
                        warning["from"] =
                            json!(warning["from"].as_u64().unwrap_or(0) + offset as u64);
                        warning["to"] = json!(warning["to"].as_u64().unwrap_or(0) + offset as u64);
                        warning["coordinateSpace"] = json!("source");
                    }
                }
            }
            metadata
        })
        .map_err(|mut e| {
            if let Some(f) = fm.filter(|_|e.from!=0||e.to!=0) {
                e.from += utf16(content, f.body_from);
                e.to += utf16(content, f.body_from);
            }
            e
        })
}
/// Preserve clear provenance while merging presets. Other options keep the
/// existing export merge contract, including its handling of JSON null.
pub fn merge_options(base: &Value, over: &Value) -> Result<Value> {
    for options in [base,over]{crate::publication_navigation::validate_options(options).map_err(|e|MetadataError{code:e.code.into(),path:e.path,from:e.from,to:e.to})?;}
    let mut navigation_over=over.clone();crate::publication_navigation::inherit_null_overlay(&mut navigation_over);let over=&navigation_over;
    let (base, over) = crate::publication_typography::merge_layers(base, over).map_err(|code| err(code, crate::publication_typography::merge_error_field(base, over)))?;
    let base = &base; let over = &over;
    let (mut values, mut clears) = overlay_values(base)?;
    let (next, next_clear) = overlay_values(over)?;
    for key in next_clear {
        values.remove(&key);
        clears.insert(key);
    }
    for (key, value) in next {
        clears.remove(&key);
        values.insert(key, value);
    }
    let mut result = crate::export_styles::deep_merge(base, over);
    let mut meta = result
        .get("meta")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for key in meta
        .keys()
        .filter(|k| metadata_key(k) || k.as_str() == "clear")
        .cloned()
        .collect::<Vec<_>>()
    {
        meta.remove(&key);
    }
    meta.extend(values);
    if !clears.is_empty() {
        meta.insert("clear".into(), json!(clears));
    }
    if base.get("meta").is_some() || over.get("meta").is_some() {
        if !result.is_object() {
            result = json!({});
        }
        result["meta"] = Value::Object(meta);
    }
    Ok(result)
}

fn path_components(path: &str) -> Vec<String> {
    path.trim_start_matches("meta.")
        .replace('[', ".")
        .replace(']', "")
        .split('.')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}
fn node_for_path<'a>(root: &'a Node, path: &str) -> Option<&'a Node> {
    let mut node = root;
    for (index, component) in path_components(path).iter().enumerate() {
        match &node.kind {
            Kind::Object(entries) => {
                let entry = entries.iter().find(|e| {
                    if index == 0 {
                        canonical(&e.key) == component
                    } else {
                        e.key == *component
                    }
                });
                if let Some(entry) = entry {
                    node = &entry.node;
                } else {
                    break;
                }
            }
            Kind::Array(items) => {
                if let Some(item) = component.parse::<usize>().ok().and_then(|i| items.get(i)) {
                    node = item;
                } else {
                    break;
                }
            }
            _ => break,
        }
    }
    Some(node)
}
fn locate(mut error: MetadataError, root: &Node, source: &str, offset: usize) -> MetadataError {
    if error.from == 0 && error.to == 0 {
        if let Some(node) = node_for_path(root, &error.path) {
            error.from = utf16(source, offset + node.start);
            error.to = utf16(source, offset + node.end);
        }
    }
    error
}

fn attach_warning_ranges(
    items: &mut [Value],
    root: Option<&Node>,
    source: &str,
    overridden: &BTreeSet<String>,
) {
    for warning in items {
        let path = warning["path"].as_str().unwrap_or("").to_string();
        let top = path_components(&path)
            .into_iter()
            .next()
            .unwrap_or_default();
        let in_source = !overridden.contains(&top) && root.is_some_and(|root| {
            matches!(&root.kind, Kind::Object(entries) if entries.iter().any(|entry|canonical(&entry.key)==top))
        });
        let range = if in_source {
            node_for_path(root.unwrap(), &path)
                .map(|node| (utf16(source, node.start), utf16(source, node.end)))
        } else {
            None
        };
        let (from, to) = range.unwrap_or((0, 0));
        warning["from"] = json!(from);
        warning["to"] = json!(to);
        warning["units"] = json!("UTF-16");
        warning["coordinateSpace"] = json!(if in_source {
            "frontMatter"
        } else if overridden.contains(&top) {
            "options"
        } else {
            "fallback"
        });
    }
}

/// Format diagnostics retain field positions while export overrides are
/// explicitly located in options, never attributed to an inherited source value.
pub(crate) fn annotate_warning_ranges(
    items: &mut [Value],
    front_matter: Option<&str>,
    options: &Value,
) -> Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    let root = front_matter.map(parse_yaml).transpose()?;
    let (over, clear) = overlay_values(options)?;
    let overridden = over.into_iter().map(|(key, _)| key).chain(clear).collect();
    attach_warning_ranges(
        items,
        root.as_ref(),
        front_matter.unwrap_or(""),
        &overridden,
    );
    Ok(())
}

/// Typed, literal paragraphs; metadata text is never parsed as Markdown/HTML
/// or emitted as an executable TeX/Typst fragment. Paragraphs avoid TOC IDs.
pub fn front_matter_blocks(meta: &PublicationMetadata, options: &Value) -> Vec<Block> {
    let mode = options
        .pointer("/frontMatter/mode")
        .and_then(Value::as_str)
        .unwrap_or("none");
    if !matches!(mode, "paper" | "book") {
        return Vec::new();
    }
    let mut blocks = Vec::new();
    let mut paragraph = |value: String, bold: bool| {
        if !value.is_empty() {
            let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
            let mut inlines = Vec::new();
            for (i, line) in normalized.split('\n').enumerate() {
                if i > 0 {
                    inlines.push(Inline::HardBreak);
                }
                if !line.is_empty() {
                    inlines.push(Inline::Text(line.into()));
                }
            }
            blocks.push(Block::Paragraph(if bold {
                vec![Inline::Strong(inlines)]
            } else {
                inlines
            }));
        }
    };
    paragraph(meta.title.clone(), true);
    if let Some(s) = &meta.subtitle {
        paragraph(s.clone(), false);
    }
    for person in &meta.authors {
        let mut line = person.name.clone();
        let marks = person
            .affiliation_ids
            .iter()
            .filter_map(|id| {
                meta.affiliations
                    .iter()
                    .position(|a| &a.id == id)
                    .map(|i| (i + 1).to_string())
            })
            .collect::<Vec<_>>();
        if !marks.is_empty() {
            line.push_str(&format!(" [{}]", marks.join(", ")));
        }
        if person.corresponding {
            line.push_str(&format!(
                " · {}",
                content_label(meta, options, "corresponding", None)
            ));
        }
        if let Some(orcid) = &person.orcid {
            line.push_str(&format!(" · ORCID: {orcid}"));
        }
        paragraph(line, false);
    }
    for (i, a) in meta.affiliations.iter().enumerate() {
        let mut line = format!("[{}] {}", i + 1, a.name);
        for s in [&a.department, &a.address].into_iter().flatten() {
            line.push_str(&format!(" · {s}"));
        }
        paragraph(line, false);
    }
    if mode == "paper" {
        for abs in &meta.abstracts {
            paragraph(
                content_label(meta, options, "abstract", abs.language.as_deref()),
                true,
            );
            paragraph(abs.text.clone(), false);
        }
        if !meta.keywords.is_empty() {
            paragraph(
                format!(
                    "{}: {}",
                    content_label(meta, options, "keywords", None),
                    meta.keywords.join("; ")
                ),
                false,
            );
        }
        for funding in &meta.funding {
            let mut value = format!(
                "{}: {}",
                content_label(meta, options, "funding", None),
                funding.funder
            );
            if let Some(award) = &funding.award {
                value.push_str(&format!(" ({award})"));
            }
            let recipients = funding
                .recipient_ids
                .iter()
                .filter_map(|id| {
                    meta.authors
                        .iter()
                        .find(|p| &p.id == id)
                        .map(|p| p.name.as_str())
                })
                .collect::<Vec<_>>();
            if !recipients.is_empty() {
                value.push_str(&format!(" — {}", recipients.join("; ")));
            }
            paragraph(value, false);
        }
    } else {
        for p in &meta.translators {
            paragraph(
                format!(
                    "{}: {}",
                    content_label(meta, options, "translator", None),
                    p.name
                ),
                false,
            );
        }
        for (key, value) in [
            ("publisher", &meta.publisher),
            ("date", &meta.date),
            ("edition", &meta.edition),
            ("rights", &meta.rights),
        ] {
            if let Some(value) = value {
                paragraph(
                    format!("{}: {value}", content_label(meta, options, key, None)),
                    false,
                );
            }
        }
        if let Some(series) = &meta.series {
            paragraph(
                format!(
                    "{}: {}{}",
                    content_label(meta, options, "series", None),
                    series.title,
                    series
                        .number
                        .as_ref()
                        .map(|s| format!(" · {s}"))
                        .unwrap_or_default()
                ),
                false,
            );
        }
        for id in &meta.identifiers {
            paragraph(format!("{}: {}", id.scheme, id.value), false);
        }
    }
    blocks
}
fn content_labels(language: &str) -> Option<&'static Value> {
    static LABELS: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    let asset = LABELS.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../assets/data/publication-metadata-i18n.json"
        ))
        .unwrap_or(Value::Null)
    });
    select_content_labels(asset.get("contentLabels")?,language)
}
/// Shared locale selection for independently typed content labels.
pub(crate) fn select_content_labels<'a>(labels:&'a Value,language:&str)->Option<&'a Value> {
    let tag = language.replace('_', "-");
    let find = |locale: &str| {
        labels
            .as_object()?
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(locale))
            .map(|(_, value)| value)
    };
    if let Some(exact) = find(&tag) {
        return Some(exact);
    }
    let subtags = tag.split('-').collect::<Vec<_>>();
    let primary = subtags.first().copied().unwrap_or("en");
    if primary.eq_ignore_ascii_case("zh") {
        // An explicit script controls the writing system. Traditional HK/MO
        // selects its own vocabulary; a missing script follows the region.
        let has = |value: &str| {
            subtags
                .iter()
                .skip(1)
                .any(|part| part.eq_ignore_ascii_case(value))
        };
        let locale = if has("Hans") {
            "zh-CN"
        } else if has("HK") || has("MO") {
            "zh-HK"
        } else if has("Hant") || has("TW") {
            "zh-TW"
        } else {
            "zh-CN"
        };
        return find(locale);
    }
    find(primary)
}
fn label_supported(language: &str) -> bool {
    if content_labels(language).is_some() {
        return true;
    }
    matches!(
        language.split('-').next().unwrap_or("en"),
        "en" | "zh" | "ja" | "de" | "fr" | "es" | "ar"
    )
}
fn append_label_warnings(meta: &mut PublicationMetadata, options: &Value) {
    let mode = options
        .pointer("/frontMatter/mode")
        .and_then(Value::as_str)
        .unwrap_or("none");
    if !matches!(mode, "paper" | "book") {
        return;
    }
    let mut fallback = BTreeMap::<(String, String), BTreeSet<String>>::new();
    let mut check = |key: &str, language: Option<&str>, path: &str| {
        if options
            .pointer("/frontMatter/labels")
            .and_then(|v| v.get(key))
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty() && s.len() <= 4096)
        {
            return;
        }
        let language = language.or(meta.language.as_deref()).unwrap_or("en");
        if !label_supported(language) {
            fallback
                .entry((path.into(), language.into()))
                .or_default()
                .insert(key.into());
        }
    };
    if meta.authors.iter().any(|p| p.corresponding) {
        check("corresponding", None, "language");
    }
    if mode == "paper" {
        for (i, abstract_) in meta.abstracts.iter().enumerate() {
            check(
                "abstract",
                abstract_.language.as_deref(),
                &if abstract_.language.is_some() {
                    format!("abstracts[{i}].language")
                } else {
                    "language".into()
                },
            );
        }
        if !meta.keywords.is_empty() {
            check("keywords", None, "language");
        }
        if !meta.funding.is_empty() {
            check("funding", None, "language");
        }
    } else {
        if !meta.translators.is_empty() {
            check("translator", None, "language");
        }
        for (key, value) in [
            ("publisher", &meta.publisher),
            ("date", &meta.date),
            ("edition", &meta.edition),
            ("rights", &meta.rights),
        ] {
            if value.is_some() {
                check(key, None, "language");
            }
        }
        if meta.series.is_some() {
            check("series", None, "language");
        }
    }
    meta.warning_items.extend(fallback.into_iter().map(|((path,language),labels)|json!({"code":"publication_metadata_label_fallback","path":path,"language":language,"fallback":"en","labels":labels})));
}
fn content_label(
    meta: &PublicationMetadata,
    options: &Value,
    key: &str,
    language: Option<&str>,
) -> String {
    if let Some(value) = options
        .pointer("/frontMatter/labels")
        .and_then(|v| v.get(key))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 4096)
    {
        return value.into();
    }
    let declared = language.or(meta.language.as_deref()).unwrap_or("en");
    if let Some(value) = content_labels(declared)
        .and_then(|labels| labels.get(key))
        .and_then(Value::as_str)
    {
        return value.into();
    }
    let lang = language
        .or(meta.language.as_deref())
        .unwrap_or("en")
        .split('-')
        .next()
        .unwrap_or("en");
    let labels = match lang {
        "zh" => [
            "摘要",
            "关键词",
            "基金",
            "通讯作者",
            "译者",
            "出版社",
            "日期",
            "版本",
            "版权",
            "丛书",
        ],
        "ja" => [
            "要旨",
            "キーワード",
            "研究助成",
            "責任著者",
            "翻訳者",
            "出版社",
            "日付",
            "版",
            "著作権",
            "シリーズ",
        ],
        "de" => [
            "Zusammenfassung",
            "Schlüsselwörter",
            "Förderung",
            "Korrespondenzautor",
            "Übersetzer",
            "Verlag",
            "Datum",
            "Auflage",
            "Rechte",
            "Reihe",
        ],
        "fr" => [
            "Résumé",
            "Mots-clés",
            "Financement",
            "Auteur correspondant",
            "Traducteur",
            "Éditeur",
            "Date",
            "Édition",
            "Droits",
            "Collection",
        ],
        "es" => [
            "Resumen",
            "Palabras clave",
            "Financiación",
            "Autor de correspondencia",
            "Traductor",
            "Editorial",
            "Fecha",
            "Edición",
            "Derechos",
            "Serie",
        ],
        "ar" => [
            "الملخص",
            "الكلمات المفتاحية",
            "التمويل",
            "المؤلف المسؤول",
            "المترجم",
            "الناشر",
            "التاريخ",
            "الطبعة",
            "الحقوق",
            "السلسلة",
        ],
        _ => [
            "Abstract",
            "Keywords",
            "Funding",
            "Corresponding author",
            "Translator",
            "Publisher",
            "Date",
            "Edition",
            "Rights",
            "Series",
        ],
    };
    let index = [
        "abstract",
        "keywords",
        "funding",
        "corresponding",
        "translator",
        "publisher",
        "date",
        "edition",
        "rights",
        "series",
    ]
    .iter()
    .position(|k| *k == key)
    .unwrap_or(0);
    labels[index].into()
}

#[derive(Debug, Clone)]
struct ByteEdit {
    from: usize,
    to: usize,
    insert: String,
}
fn line_start(source: &str, at: usize) -> usize {
    source[..at.min(source.len())]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0)
}
fn line_end(source: &str, at: usize) -> usize {
    source[at.min(source.len())..]
        .find('\n')
        .map(|i| at + i + 1)
        .unwrap_or(source.len())
}
fn yaml(value: &Value) -> String {
    serde_json::to_string(value).expect("JSON value serialization")
}
fn apply_bytes(source: &str, edits: &[ByteEdit], from: usize, to: usize) -> Result<String> {
    let mut edits = edits.iter().collect::<Vec<_>>();
    edits.sort_by_key(|e| (e.from, e.to));
    let mut output = String::new();
    let mut cursor = from;
    for e in edits {
        if e.from < cursor || e.to > to || e.to < e.from {
            return Err(err("transaction_overlapping_edits", "patch"));
        }
        output.push_str(&source[cursor..e.from]);
        output.push_str(&e.insert);
        cursor = e.to;
    }
    output.push_str(&source[cursor..to]);
    Ok(output)
}
fn merged_unknown(source: &Value, value: &Value) -> Value {
    if let (Some(source), Some(value)) = (source.as_object(), value.as_object()) {
        let mut merged = source.clone();
        for (key, value) in value {
            merged.insert(
                key.clone(),
                merged_unknown(merged.get(key).unwrap_or(&Value::Null), value),
            );
        }
        Value::Object(merged)
    } else {
        value.clone()
    }
}
fn without_separator(gap: &str) -> String {
    let mut comment = false;
    let mut removed = false;
    gap.chars()
        .filter(|c| {
            if matches!(*c, '\r' | '\n') {
                comment = false;
            } else if *c == '#' {
                comment = true;
            }
            if *c == ',' && !comment && !removed {
                removed = true;
                false
            } else {
                true
            }
        })
        .collect()
}
fn array_matches(old: &Value, new: &Value) -> bool {
    if old == new {
        return true;
    }
    for key in ["id", "funder", "language", "scheme"] {
        if let (Some(a), Some(b)) = (old.get(key), new.get(key)) {
            if !a.is_null() && a == b {
                return true;
            }
        }
    }
    false
}
fn object_insert(
    source: &str,
    node: &Node,
    values: &[(String, Value)],
    eol: &str,
    edits: &mut Vec<ByteEdit>,
) {
    if values.is_empty() {
        return;
    }
    if node.flow {
        let at = node.end.saturating_sub(1);
        let prefix = if node.value.as_object().is_some_and(|m| !m.is_empty()) {
            ", "
        } else {
            ""
        };
        edits.push(ByteEdit {
            from: at,
            to: at,
            insert: format!(
                "{prefix}{}",
                values
                    .iter()
                    .map(|(k, v)| format!("{}: {}", yaml(&json!(k)), yaml(v)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
        return;
    }
    let Kind::Object(entries) = &node.kind else {
        return;
    };
    let column = entries
        .first()
        .map(|e| e.key_start - line_start(source, e.key_start))
        .unwrap_or(0);
    let at = line_end(source, node.end);
    let prefix = if at > 0 && !source[..at].ends_with('\n') {
        eol
    } else {
        ""
    };
    edits.push(ByteEdit {
        from: at,
        to: at,
        insert: format!(
            "{prefix}{}",
            values
                .iter()
                .map(|(k, v)| format!(
                    "{}{}: {}{eol}",
                    " ".repeat(column),
                    yaml(&json!(k)),
                    yaml(v)
                ))
                .collect::<String>()
        ),
    });
}
fn patch_node(
    source: &str,
    node: &Node,
    old: &Value,
    new: &Value,
    eol: &str,
    force_ids: bool,
    edits: &mut Vec<ByteEdit>,
) -> Result<()> {
    if old == new && (!force_ids || !new.is_object() && !new.is_array()) {
        return Ok(());
    }
    match (&node.kind, new) {
        (Kind::Object(entries), Value::Object(values)) => {
            let mut additions = Vec::new();
            for (key, value) in values {
                if let Some(entry) = entries.iter().find(|e| &e.key == key) {
                    patch_node(
                        source,
                        &entry.node,
                        old.get(key).unwrap_or(&entry.node.value),
                        value,
                        eol,
                        force_ids,
                        edits,
                    )?;
                } else if !is_empty(value)
                    && value != &json!(false)
                    && (old.get(key) != Some(value) || force_ids && key == "id")
                {
                    additions.push((key.clone(), value.clone()));
                }
            }
            object_insert(source, node, &additions, eol, edits);
        }
        (Kind::Array(children), Value::Array(values)) => {
            let normalized = old.as_array().map(Vec::as_slice).unwrap_or(&[]);
            let mut used = BTreeSet::new();
            let mut matches = Vec::new();
            for value in values {
                let found = normalized
                    .iter()
                    .enumerate()
                    .find(|(i, old)| !used.contains(i) && *old == value)
                    .or_else(|| {
                        normalized
                            .iter()
                            .enumerate()
                            .find(|(i, old)| !used.contains(i) && array_matches(old, value))
                    })
                    .map(|(i, _)| i);
                if let Some(i) = found {
                    used.insert(i);
                }
                matches.push(found);
            }
            let same_order = children.len() == values.len()
                && matches
                    .iter()
                    .enumerate()
                    .all(|(i, found)| *found == Some(i));
            if same_order {
                for (i, child) in children.iter().enumerate() {
                    patch_node(
                        source,
                        child,
                        normalized.get(i).unwrap_or(&child.value),
                        &values[i],
                        eol,
                        force_ids,
                        edits,
                    )?;
                }
                return Ok(());
            }
            if node.flow {
                // Preserve the lexical representation of each retained flow
                // item; punctuation between rows is owned by the array.
                let mut parts = Vec::new();
                for (i, value) in values.iter().enumerate() {
                    if let Some(index) = matches[i] {
                        let child = &children[index];
                        let mut changes = Vec::new();
                        patch_node(
                            source,
                            child,
                            &normalized[index],
                            value,
                            eol,
                            force_ids,
                            &mut changes,
                        )?;
                        parts.push(apply_bytes(source, &changes, child.start, child.end)?);
                    } else {
                        parts.push(yaml(value));
                    }
                }
                // yaml-rust2 rejects comment-only multiline flow sequences.
                // Write a real empty value first, then keep every original
                // comment/whitespace byte as trailing mapping trivia.
                let mut output = String::from(if parts.is_empty() { "[]" } else { "[" });
                if let (Some(first), Some(last)) = (children.first(), children.last()) {
                    output.push_str(&source[node.start + 1..first.start]);
                    let gaps = children
                        .windows(2)
                        .map(|w| &source[w[0].end..w[1].start])
                        .collect::<Vec<_>>();
                    for (i, part) in parts.iter().enumerate() {
                        if i > 0 {
                            output.push_str(gaps.get(i - 1).copied().unwrap_or(", "));
                        }
                        output.push_str(part);
                    }
                    // Deletion may remove separators, never unrelated comment
                    // bytes. Equal-length reorders retain every gap verbatim.
                    for gap in gaps.iter().skip(parts.len().saturating_sub(1)) {
                        output.push_str(&without_separator(gap));
                    }
                    let suffix = &source[last.end..node.end - 1];
                    output.push_str(&if parts.is_empty() {
                        without_separator(suffix)
                    } else {
                        suffix.into()
                    });
                } else {
                    output.push_str(&parts.join(", "));
                }
                if !parts.is_empty() {
                    output.push(']');
                }
                edits.push(ByteEdit {
                    from: node.start,
                    to: node.end,
                    insert: output,
                });
                return Ok(());
            }
            if children.is_empty() {
                edits.push(ByteEdit {
                    from: node.start,
                    to: node.end,
                    insert: yaml(new),
                });
                return Ok(());
            }
            let first_line = line_start(source, node.start);
            let prefix = &source[first_line..node.start];
            let indent = if prefix.chars().all(char::is_whitespace) {
                prefix.len()
            } else {
                source[first_line..]
                    .chars()
                    .take_while(|c| *c == ' ')
                    .count()
            };
            // Find each dash from token positions, including '-\n  name:'.
            let mut starts = Vec::new();
            for child in children {
                let mut found = None;
                let limit = line_start(source, child.start);
                let mut at = first_line;
                while at <= limit {
                    let end = line_end(source, at);
                    let line = &source[at..end];
                    if line.as_bytes().get(indent) == Some(&b'-')
                        && line
                            .as_bytes()
                            .get(indent + 1)
                            .is_none_or(u8::is_ascii_whitespace)
                    {
                        found = Some(at);
                    }
                    if end <= at {
                        break;
                    }
                    at = end;
                }
                starts
                    .push(found.ok_or_else(|| err("publication_metadata_patch_syntax", "patch"))?);
            }
            if starts.windows(2).any(|w| w[0] >= w[1]) {
                return Err(err("publication_metadata_patch_syntax", "patch"));
            }
            // Ownership is deliberately lexical: a contiguous full-line
            // comment at exactly the dash indentation belongs to that next
            // row. Blank lines, other indentation and any value/key stop the
            // scan. Never cross the preceding dash (including '-\n name:').
            // Compare indentation rather than the prior node's end marker:
            // block-scalar trivia can extend that marker beyond its own value.
            let dash_starts = starts.clone();
            for (index, start) in starts.iter_mut().enumerate() {
                let floor = if index == 0 {
                    0
                } else {
                    line_end(source, dash_starts[index - 1])
                };
                while *start > floor {
                    let previous = line_start(source, *start - 1);
                    let line = &source[previous..*start];
                    if previous < floor
                        || line.as_bytes().get(indent) != Some(&b'#')
                        || !line.as_bytes()[..indent].iter().all(|b| *b == b' ')
                    {
                        break;
                    }
                    *start = previous;
                }
            }
            let end = line_end(source, node.end);
            let from = starts[0];
            let mut output = String::new();
            if values.is_empty() {
                // Keep the field's key/comment and an explicit empty list.
                edits.push(ByteEdit {
                    from: node.start,
                    to: node.end,
                    insert: "[]".into(),
                });
                return Ok(());
            }
            for (i, value) in values.iter().enumerate() {
                if let Some(index) = matches[i] {
                    let child = &children[index];
                    let to = starts.get(index + 1).copied().unwrap_or(end);
                    let mut changes = Vec::new();
                    patch_node(
                        source,
                        child,
                        &normalized[index],
                        value,
                        eol,
                        force_ids,
                        &mut changes,
                    )?;
                    let mut row = apply_bytes(source, &changes, starts[index], to)?;
                    if !row.ends_with('\n') {
                        row.push_str(eol);
                    }
                    output.push_str(&row);
                } else {
                    output.push_str(&format!("{}- {}{eol}", " ".repeat(indent), yaml(value)));
                }
            }
            edits.push(ByteEdit {
                from,
                to: end,
                insert: output,
            });
        }
        _ => {
            let value = merged_unknown(&node.value, new);
            edits.push(ByteEdit {
                from: node.start,
                to: node.end,
                insert: yaml(&value),
            });
        }
    }
    Ok(())
}
fn ranges(root: &Node, source: &str, offset: usize) -> Value {
    fn walk(node: &Node, path: &str, source: &str, offset: usize, out: &mut Map<String, Value>) {
        out.insert(
            path.into(),
            json!({"from":utf16(source,offset+node.start),"to":utf16(source,offset+node.end)}),
        );
        match &node.kind {
            Kind::Object(entries) => {
                for e in entries {
                    walk(&e.node, &format!("{path}.{}", e.key), source, offset, out)
                }
            }
            Kind::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    walk(item, &format!("{path}[{i}]"), source, offset, out)
                }
            }
            _ => {}
        }
    }
    let mut out = Map::new();
    if let Kind::Object(entries) = &root.kind {
        for entry in entries.iter().filter(|e| metadata_key(&e.key)) {
            walk(&entry.node, canonical(&entry.key), source, offset, &mut out);
        }
    }
    Value::Object(out)
}
/// Plan and validate an invertible source transaction; never save a file.
pub fn plan_patch(content: &str, base_revision: &str, patch: &Value) -> Result<ContentTransaction> {
    if base_revision != transaction::revision(content) {
        return Err(err("transaction_conflict", "base_revision"));
    }
    let Some(patch) = patch.as_object() else {
        return Err(err("publication_metadata_type", "patch"));
    };
    let fm = front(content)?;
    let empty = Node {
        value: json!({}),
        start: 0,
        end: 0,
        flow: false,
        kind: Kind::Object(Vec::new()),
    };
    let root = if let Some(fm) = &fm {
        parse_yaml(fm.body)?
    } else {
        empty
    };
    let values = source_values(&root).map_err(|e| {
        if let Some(fm) = &fm {
            locate(e, &root, content, fm.body_from)
        } else {
            e
        }
    })?;
    let mut requested = Map::new();
    for (key, value) in patch {
        if !metadata_key(key) && !matches!(key.as_str(), "bibliography" | "csl" | "presentation") {
            return Err(err("publication_metadata_field", &format!("patch.{key}")));
        }
        let canonical = canonical(key);
        let normalized = if metadata_key(key) {
            normalize_field(canonical, value, key)?
        } else {
            value.clone()
        };
        if requested.get(canonical).is_some_and(|v| v != &normalized) {
            return Err(err("publication_metadata_alias_conflict", canonical));
        }
        requested.insert(canonical.into(), normalized);
    }
    // References are stable source IDs, rather than list positions. When a
    // generated read-only ID becomes referenced, materialize only those IDs.
    let changed = requested.iter().any(|(key, value)| {
        values
            .get(key)
            .or_else(|| root.value.get(key))
            .unwrap_or(&Value::Null)
            != value
    });
    let mut forced = BTreeSet::new();
    let funding = requested.get("funding").or_else(|| values.get("funding"));
    let authors = requested.get("authors").or_else(|| values.get("authors"));
    let need_authors = changed
        && funding.and_then(Value::as_array).is_some_and(|rows| {
            rows.iter().any(|r| {
                r.get("recipientIds")
                    .and_then(Value::as_array)
                    .is_some_and(|a| !a.is_empty())
            })
        });
    if need_authors {
        if let Some(authors) = authors.cloned() {
            requested.entry("authors").or_insert(authors);
            forced.insert("authors");
        }
    }
    let need_affiliations = changed
        && ["authors", "translators"].iter().any(|k| {
            requested
                .get(*k)
                .or_else(|| values.get(*k))
                .and_then(Value::as_array)
                .is_some_and(|rows| {
                    rows.iter().any(|r| {
                        r.get("affiliationIds")
                            .and_then(Value::as_array)
                            .is_some_and(|a| !a.is_empty())
                    })
                })
        });
    if need_affiliations {
        if let Some(value) = values.get("affiliations") {
            requested.entry("affiliations").or_insert(value.clone());
            forced.insert("affiliations");
        }
    }
    let mut byte_edits = Vec::new();
    if let Some(fm) = &fm {
        let Kind::Object(entries) = &root.kind else {
            return Err(err("publication_metadata_type", "frontMatter"));
        };
        let mut additions = Vec::new();
        for (key, value) in &requested {
            let matches = entries
                .iter()
                .filter(|e| canonical(&e.key) == key)
                .collect::<Vec<_>>();
            if matches.is_empty() {
                if !is_empty(value) {
                    additions.push((key.clone(), value.clone()));
                }
                continue;
            }
            let current = values
                .get(key)
                .or_else(|| root.value.get(key))
                .unwrap_or(&Value::Null);
            if current == value && !forced.contains(key.as_str()) {
                continue;
            }
            for entry in matches {
                patch_node(
                    fm.body,
                    &entry.node,
                    current,
                    value,
                    fm.eol,
                    forced.contains(key.as_str()),
                    &mut byte_edits,
                )?;
            }
        }
        if !additions.is_empty() {
            if root.flow {
                object_insert(fm.body, &root, &additions, fm.eol, &mut byte_edits);
            } else {
                byte_edits.push(ByteEdit {
                    from: fm.body.len(),
                    to: fm.body.len(),
                    insert: additions
                        .iter()
                        .map(|(k, v)| format!("{}: {}{}", yaml(&json!(k)), yaml(v), fm.eol))
                        .collect(),
                });
            }
        }
        for edit in &mut byte_edits {
            edit.from += fm.body_from;
            edit.to += fm.body_from;
        }
    } else if requested.values().any(|v| !is_empty(v)) {
        let eol = if content.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let at = if content.starts_with('\u{feff}') {
            3
        } else {
            0
        };
        let body = requested
            .iter()
            .filter(|(_, v)| !is_empty(v))
            .map(|(k, v)| format!("{}: {}{eol}", yaml(&json!(k)), yaml(v)))
            .collect::<String>();
        byte_edits.push(ByteEdit {
            from: at,
            to: at,
            insert: format!("---{eol}{body}---{eol}"),
        });
    }
    // Coalesce independent insertions at a shared mapping boundary, while
    // keeping every replacement's original source interval intact.
    byte_edits.sort_by_key(|e| (e.from, e.to));
    let mut combined: Vec<ByteEdit> = Vec::new();
    for edit in byte_edits {
        if let Some(last) = combined
            .last_mut()
            .filter(|last| last.from == edit.from && last.to == edit.to && edit.from == edit.to)
        {
            last.insert.push_str(&edit.insert);
        } else {
            combined.push(edit);
        }
    }
    let tx = ContentTransaction {
        base_revision: base_revision.into(),
        origin: "editor".into(),
        edits: combined
            .into_iter()
            .map(|e| TextEdit {
                from: utf16(content, e.from),
                to: utf16(content, e.to),
                insert: e.insert,
            })
            .collect(),
    };
    let applied = transaction::apply(content, &tx).map_err(|code| err(code, "patch"))?;
    from_source(&applied.content, &json!({}), "").map_err(|e| {
        if let Some(fm) = &fm {
            locate(
                MetadataError {
                    from: 0,
                    to: 0,
                    ..e
                },
                &root,
                content,
                fm.body_from,
            )
        } else {
            e
        }
    })?;
    Ok(tx)
}
pub fn handle(request: &Value) -> Result<Value> {
    let content = request
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| err("publication_metadata_type", "content"))?;
    let source_name = request
        .get("source_name")
        .or_else(|| request.get("sourceName"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let options = request.get("options").unwrap_or(&Value::Null);
    if request
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("get")
        == "patch"
    {
        let revision = request
            .get("base_revision")
            .or_else(|| request.get("baseRevision"))
            .and_then(Value::as_str)
            .ok_or_else(|| err("transaction_conflict", "base_revision"))?;
        let tx = plan_patch(
            content,
            revision,
            request.get("patch").unwrap_or(&Value::Null),
        )?;
        let applied = transaction::apply(content, &tx).map_err(|code| err(code, "patch"))?;
        let mut response = handle(
            &json!({"action":"get","content":applied.content,"options":options,"source_name":source_name}),
        )?;
        response["transaction"] = json!(tx);
        response["content"] = json!(applied.content);
        response["inverse"] = json!(applied.inverse);
        response["changed"] = json!(applied.changed);
        return Ok(response);
    }
    if request
        .get("action")
        .and_then(Value::as_str)
        .is_some_and(|s| s != "get")
    {
        return Err(err("publication_metadata_action", "action"));
    }
    let fm = front(content)?;
    let metadata = from_source(content, options, source_name)?;
    let (values, field_ranges, legacy) = if let Some(fm) = &fm {
        let root = parse_yaml(fm.body)?;
        let values = source_values(&root).map_err(|e| locate(e, &root, content, fm.body_from))?;
        let mut legacy = Map::new();
        for key in ["bibliography", "csl", "presentation", "lang"] {
            if let Some(value) = root.value.get(key) {
                legacy.insert(key.into(), value.clone());
            }
        }
        (
            Value::Object(values),
            ranges(&root, content, fm.body_from),
            Value::Object(legacy),
        )
    } else {
        (json!({}), json!({}), json!({}))
    };
    Ok(
        json!({"ok":true,"metadata":metadata,"sourceValues":values,"ranges":field_ranges,"legacyValues":legacy,"revision":transaction::revision(content)}),
    )
}
pub fn schema() -> Value {
    let string = json!({"type":["string","null"],"maxLength":4096});
    let refs = json!({"type":"array","maxItems":32,"items":{"type":"string"}});
    let person = json!({"type":"object","required":["name"],"properties":{"id":{"type":"string","maxLength":128},"name":string,"given":string,"family":string,"fileAs":string,"orcid":string,"corresponding":{"type":"boolean"},"affiliationIds":refs}});
    let mut properties = Map::new();
    for key in FIELDS {
        properties.insert((*key).into(), string.clone());
    }
    properties.insert(
        "authors".into(),
        json!({"type":"array","maxItems":32,"items":person}),
    );
    properties.insert(
        "translators".into(),
        json!({"type":"array","maxItems":32,"items":person}),
    );
    properties.insert("affiliations".into(),json!({"type":"array","maxItems":32,"items":{"type":"object","required":["id","name"],"properties":{"id":string,"name":string,"department":string,"address":string}}}));
    properties.insert("abstracts".into(),json!({"type":"array","maxItems":8,"items":{"type":"object","required":["text"],"properties":{"text":{"type":"string","maxLength":16384},"language":string}}}));
    properties.insert(
        "keywords".into(),
        json!({"type":"array","maxItems":64,"items":string}),
    );
    properties.insert("funding".into(),json!({"type":"array","maxItems":16,"items":{"type":"object","required":["funder"],"properties":{"funder":string,"award":string,"recipientIds":refs}}}));
    properties.insert("series".into(),json!({"type":["object","null"],"properties":{"title":string,"number":string},"required":["title"]}));
    properties.insert("identifiers".into(),json!({"type":"array","maxItems":16,"items":{"type":"object","required":["scheme","value"],"properties":{"scheme":string,"value":string,"primary":{"type":"boolean"}}}}));
    properties.insert(
        "clear".into(),
        json!({"type":"array","maxItems":FIELDS.len(),"uniqueItems":true,"items":{"enum":FIELDS}}),
    );
    for property in properties.values_mut() {
        if property["type"]=="array" {property["type"]=json!(["array","null"]);}
    }
    let mut patch_properties = properties.clone();
    patch_properties.remove("clear");
    patch_properties.insert(
        "bibliography".into(),
        json!({"anyOf":[string,{"type":"array","items":string}]}),
    );
    patch_properties.insert("csl".into(), string.clone());
    patch_properties.insert("presentation".into(), json!({"type":"object"}));
    json!({"version":1,"budgets":{"frontMatterBytes":MAX_FRONT_MATTER,"depth":MAX_DEPTH,"nodes":MAX_NODES},"meta":{"type":"object","properties":properties},"frontMatter":{"type":"object","properties":{"mode":{"enum":["none","paper","book"]},"labels":{"type":"object","additionalProperties":string}}},"patch":{"type":"object","properties":patch_properties},"units":"UTF-16","nullOverlay":"inherit","scalarClear":"meta.clear","arrayClear":"empty array"})
}
/// Actual callable contract; the descriptive model schema above is not itself
/// a JSON Schema for a request and must never gate all MCP calls as invalid.
pub fn request_schema() -> Value {
    let mut model=schema();
    model["meta"]["type"]=json!(["object","null"]);
    json!({"type":"object","required":["content"],"additionalProperties":false,"properties":{
        "action":{"type":"string","enum":["get","patch"]},
        "content":{"type":"string"},
        "source_name":{"type":"string","maxLength":4096},
        "sourceName":{"type":"string","maxLength":4096},
        "base_revision":{"type":"string","maxLength":64},
        "baseRevision":{"type":"string","maxLength":64},
        "options":{"type":["object","null"],"properties":{"meta":model["meta"],"frontMatter":model["frontMatter"]}},
        "patch":model["patch"]
    }})
}

#[cfg(test)]
#[path = "publication_metadata_tests.rs"]
mod tests;
