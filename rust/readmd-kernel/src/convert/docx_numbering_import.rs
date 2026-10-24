//! Import current native Word numbering. Source information carries identity
//! and sparse-presence evidence only; it never supplies a previous rule value.
use super::{docx_notes, DocxRel, XmlNode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};

pub(crate) const INVALID: &str = "docx_numbering_invalid";
pub(crate) const BUDGET: &str = "docx_numbering_budget";
pub(crate) const UNREPRESENTABLE: &str = "docx_numbering_unrepresentable";
pub(crate) const CONFLICT: &str = "docx_numbering_import_conflict";
pub(crate) const SOURCE_PART: &str = "customXml/readmd-numbering-source.xml";
const SOURCE_REL: &str = "urn:readmd:relationships:numbering-source:v1";
const SOURCE_NS: &str = "urn:readmd:numbering-source:v1";
const OFFICE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const STRICT_REL: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships";
const MAX_DEFINITIONS: usize = 4096;
const MAX_PARAGRAPHS: usize = 65536;
const MAX_FIELDS: usize = 65536;
// Match the shared passive field reader's accepted nesting boundary. This
// semantic scan must not reject an otherwise ordinary field at depths 65..128.
const MAX_FIELD_DEPTH: usize = 128;
const MAX_SOURCE_INFO: usize = 1024 * 1024;

fn invalid(reason: &str) -> String { format!("{INVALID}: {reason}") }
fn unsupported(reason: &str) -> String { format!("{UNREPRESENTABLE}: {reason}") }
fn node_key(node: &XmlNode) -> usize { node as *const XmlNode as usize }
fn child<'a>(node: &'a XmlNode, name: &str) -> Result<Option<&'a XmlNode>, String> {
    let mut values = node.children.iter().filter(|n| n.name == name);
    let value = values.next();
    if values.next().is_some() { return Err(invalid("duplicate_property")); }
    Ok(value)
}
fn val<'a>(node: &'a XmlNode, name: &str) -> Result<Option<&'a str>, String> {
    child(node, name)?.map(|n| n.attr("w:val").ok_or_else(|| invalid("missing_value"))).transpose()
}
fn uint(value: &str) -> Result<u32, String> {
    if value.is_empty() || value.len() > 10 || !value.bytes().all(|c| c.is_ascii_digit()) {
        return Err(invalid("unsigned_integer"));
    }
    value.parse().map_err(|_| invalid("unsigned_integer"))
}
fn on_off(node: &XmlNode) -> Result<bool, String> {
    match node.attr("w:val").unwrap_or("true") {
        "1" | "true" | "on" => Ok(true), "0" | "false" | "off" => Ok(false),
        _ => Err(invalid("on_off")),
    }
}
fn printable(value: &str, limit: usize) -> bool {
    value.chars().count() <= limit && !value.chars().any(char::is_control)
}
fn digest(value: &[u8]) -> String {
    Sha256::digest(value).iter().map(|byte| format!("{byte:02x}")).collect()
}
fn relation(kind: &str, suffix: &str) -> bool {
    kind == format!("{OFFICE_REL}/{suffix}") || kind == format!("{STRICT_REL}/{suffix}")
}
fn related_part(
    entries: &[(String, Vec<u8>)], part: &str, rels: &[DocxRel], suffix: &str,
) -> Result<Option<XmlNode>, String> {
    let mut matches = rels.iter().filter(|rel| relation(&rel.kind, suffix));
    let Some(rel) = matches.next() else { return Ok(None); };
    if matches.next().is_some() || !matches!(rel.mode.as_str(), "" | "Internal") {
        return Err(invalid("relationship"));
    }
    let base = part.rsplit_once('/').map(|(base, _)| base).unwrap_or("");
    let name = docx_notes::member(base, &rel.raw).map_err(|_| invalid("relationship_target"))?;
    let bytes = docx_notes::unique(entries, &name).map_err(|_| invalid("relationship_member"))?;
    docx_notes::xml(bytes).map(Some).map_err(|_| invalid("xml"))
}

/// Complete native rule values, before the source model's representability
/// checks. Unknown formats/custom pictures remain typed native evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NativeLevel {
    pub level: u8,
    pub format: String,
    pub custom_format: Option<String>,
    pub start: u32,
    pub restart_after: u8,
    pub text: String,
    pub null_text: bool,
    pub suffix: String,
    pub is_legal: bool,
    pub paragraph_style: Option<String>,
    pub picture_bullet: bool,
}
impl NativeLevel {
    fn parse(node: &XmlNode) -> Result<Self, String> {
        let level = uint(node.attr("w:ilvl").ok_or_else(|| invalid("level_index"))?)?;
        if level > 8 { return Err(invalid("level_index")); }
        let level = level as u8;
        let number_format = child(node, "w:numFmt")?;
        let format = number_format.map(|n| n.attr("w:val").unwrap_or("decimal")).unwrap_or("decimal");
        let custom_format = number_format.and_then(|n| n.attr("w:format")).map(str::to_string);
        let start = val(node, "w:start")?.map(uint).transpose()?.unwrap_or(0);
        let restart = val(node, "w:lvlRestart")?.map(uint).transpose()?;
        // The native value is one based; an invalid later-level reference is
        // ignored, yielding the standard preceding-level default.
        let restart_after = restart.filter(|value| *value <= level as u32).unwrap_or(level as u32) as u8;
        let level_text = child(node, "w:lvlText")?;
        let null_text = level_text.map(|n| {
            match n.attr("w:null") {
                None | Some("0" | "false" | "off") => Ok(false),
                Some("1" | "true" | "on") => Ok(true), _ => Err(invalid("level_text_null")),
            }
        }).transpose()?.unwrap_or(false);
        let text = if null_text { "" } else { level_text.and_then(|n| n.attr("w:val")).unwrap_or("") };
        if !printable(text, 4096) { return Err(BUDGET.into()); }
        let suffix = val(node, "w:suff")?.unwrap_or("tab");
        if !matches!(suffix, "tab" | "space" | "nothing") { return Err(invalid("level_suffix")); }
        Ok(Self {
            level, format: format.into(), custom_format, start, restart_after,
            text: text.into(), null_text, suffix: suffix.into(),
            is_legal: child(node, "w:isLgl")?.map(on_off).transpose()?.unwrap_or(false),
            paragraph_style: val(node, "w:pStyle")?.map(str::to_string),
            picture_bullet: child(node, "w:lvlPicBulletId")?.is_some(),
        })
    }
    fn source_rule(&self) -> Result<Value, String> {
        if self.level > 5 || self.start as usize > crate::cross_references::MAX_NUMBER
            || self.custom_format.is_some() || self.picture_bullet
            || !crate::publication_layout::NUMBER_FORMATS.contains(&self.format.as_str())
            || !printable(&self.text, 256) {
            return Err(unsupported("heading_level_format"));
        }
        Ok(json!({"format":self.format,"start":self.start,"restartAfter":self.restart_after,
            "text":self.text,"suffix":self.suffix,"isLegal":self.is_legal}))
    }
}

#[derive(Debug, Clone, Default)]
struct NativeAbstract {
    levels: BTreeMap<u8, NativeLevel>,
    number_style_link: Option<String>,
}
#[derive(Debug, Clone, Default)]
struct NativeInstance {
    abstract_id: u32,
    levels: BTreeMap<u8, NativeLevel>,
    starts: BTreeMap<u8, u32>,
}
#[derive(Debug, Clone, Default)]
pub(crate) struct NativeNumbering {
    abstracts: BTreeMap<u32, NativeAbstract>,
    instances: BTreeMap<u32, NativeInstance>,
}
impl NativeNumbering {
    pub(crate) fn parse(root: &XmlNode) -> Result<Self, String> {
        if root.name != "w:numbering" { return Err(invalid("numbering_root")); }
        let mut out = Self::default();
        for node in &root.children {
            match node.name.as_str() {
                "w:abstractNum" => {
                    if out.abstracts.len() >= MAX_DEFINITIONS { return Err(BUDGET.into()); }
                    let id = uint(node.attr("w:abstractNumId").ok_or_else(|| invalid("abstract_id"))?)?;
                    let mut value = NativeAbstract {
                        number_style_link: val(node, "w:numStyleLink")?.map(str::to_string), ..Default::default()
                    };
                    for level in node.children.iter().filter(|n| n.name == "w:lvl") {
                        let level = NativeLevel::parse(level)?;
                        if value.levels.insert(level.level, level).is_some() { return Err(invalid("duplicate_level")); }
                    }
                    if out.abstracts.insert(id, value).is_some() { return Err(invalid("duplicate_abstract")); }
                }
                "w:num" => {
                    if out.instances.len() >= MAX_DEFINITIONS { return Err(BUDGET.into()); }
                    let id = uint(node.attr("w:numId").ok_or_else(|| invalid("num_id"))?)?;
                    if id == 0 { return Err(invalid("reserved_num_id")); }
                    let abstract_id = uint(val(node, "w:abstractNumId")?.ok_or_else(|| invalid("abstract_reference"))?)?;
                    let mut value = NativeInstance { abstract_id, ..Default::default() };
                    let mut seen = HashSet::new();
                    for level in node.children.iter().filter(|n| n.name == "w:lvlOverride") {
                        let index = uint(level.attr("w:ilvl").ok_or_else(|| invalid("override_index"))?)?;
                        if index > 8 || !seen.insert(index) { return Err(invalid("override_index")); }
                        let index = index as u8;
                        if let Some(rule) = child(level, "w:lvl")? {
                            // A child lvl is a complete rule, not a sparse
                            // merge with the abstract rule's missing values.
                            let mut rule = NativeLevel::parse(rule)?;
                            if rule.level != index { return Err(invalid("override_level_mismatch")); }
                            // Word ignores lvlRestart inside lvlOverride.
                            rule.restart_after = index;
                            value.levels.insert(index, rule);
                        }
                        if let Some(start) = val(level, "w:startOverride")? { value.starts.insert(index, uint(start)?); }
                    }
                    if out.instances.insert(id, value).is_some() { return Err(invalid("duplicate_num")); }
                }
                _ => (),
            }
        }
        for value in out.instances.values() {
            if !out.abstracts.contains_key(&value.abstract_id) { return Err(invalid("missing_abstract")); }
        }
        Ok(out)
    }
    fn levels(&self, id: u32, styles: &Styles, stack: &mut HashSet<u32>) -> Result<BTreeMap<u8, NativeLevel>, String> {
        if !stack.insert(id) || stack.len() > 64 { return Err(invalid("number_style_cycle")); }
        let instance = self.instances.get(&id).ok_or_else(|| invalid("missing_num"))?;
        let abstract_rule = self.abstracts.get(&instance.abstract_id).ok_or_else(|| invalid("missing_abstract"))?;
        let mut levels = if let Some(style) = &abstract_rule.number_style_link {
            let properties = styles.style_properties(style)?;
            let id = properties.num_id.filter(|id| *id != 0).ok_or_else(|| invalid("number_style_link"))?;
            self.levels(id, styles, stack)?
        } else { abstract_rule.levels.clone() };
        for (index, rule) in &instance.levels {
            let mut rule = rule.clone();
            // The override changes the rule but the restart policy remains
            // the base policy on Word, including an explicit never-restart.
            if let Some(base) = levels.get(index) { rule.restart_after = base.restart_after; }
            levels.insert(*index, rule);
        }
        for (index, start) in &instance.starts {
            levels.get_mut(index).ok_or_else(|| invalid("override_missing_level"))?.start = *start;
        }
        stack.remove(&id);
        Ok(levels)
    }
}

#[derive(Debug, Clone, Default)]
struct Properties { num_id: Option<u32>, ilvl: Option<u8>, outline: Option<u8> }
impl Properties {
    fn parse(ppr: Option<&XmlNode>) -> Result<Self, String> {
        let Some(ppr) = ppr else { return Ok(Self::default()); };
        let numbering = child(ppr, "w:numPr")?;
        let num_id = numbering.map(|n| val(n, "w:numId")).transpose()?.flatten().map(uint).transpose()?;
        let ilvl = numbering.map(|n| val(n, "w:ilvl")).transpose()?.flatten().map(uint).transpose()?;
        let outline = val(ppr, "w:outlineLvl")?.map(uint).transpose()?;
        if ilvl.is_some_and(|n| n > 8) || outline.is_some_and(|n| n > 9) { return Err(invalid("paragraph_level")); }
        Ok(Self { num_id, ilvl: ilvl.map(|n| n as u8), outline: outline.map(|n| n as u8) })
    }
    fn overlay(&mut self, other: &Self) {
        if other.num_id.is_some() { self.num_id = other.num_id; }
        if other.ilvl.is_some() { self.ilvl = other.ilvl; }
        if other.outline.is_some() { self.outline = other.outline; }
    }
}
#[derive(Debug, Clone)]
struct Style { name: String, based_on: Option<String>, properties: Properties }
#[derive(Debug, Clone, Default)]
struct Styles {
    entries: HashMap<String, Style>, default_id: Option<String>, defaults: Properties,
    resolved: HashMap<String, Properties>,
}
impl Styles {
    fn parse(root: &XmlNode) -> Result<Self, String> {
        if root.name != "w:styles" { return Err(invalid("styles_root")); }
        let mut out = Self::default();
        if let Some(defaults) = child(root, "w:docDefaults")? {
            if let Some(ppr) = child(defaults, "w:pPrDefault")? { out.defaults = Properties::parse(child(ppr, "w:pPr")?)?; }
        }
        for node in root.children.iter().filter(|n| n.name == "w:style") {
            if out.entries.len() >= MAX_DEFINITIONS { return Err(BUDGET.into()); }
            let kind = node.attr("w:type").unwrap_or("paragraph");
            if !matches!(kind, "paragraph" | "numbering") { continue; }
            let id = node.attr("w:styleId").filter(|id| !id.is_empty()).ok_or_else(|| invalid("style_id"))?;
            let default = match node.attr("w:default") {
                None | Some("0" | "false" | "off") => false,
                Some("1" | "true" | "on") => true, _ => return Err(invalid("style_default")),
            };
            if default && kind == "paragraph" && out.default_id.replace(id.into()).is_some() { return Err(invalid("style_default")); }
            let style = Style { name: val(node, "w:name")?.unwrap_or(id).into(),
                based_on: val(node, "w:basedOn")?.map(str::to_string),
                properties: Properties::parse(child(node, "w:pPr")?)? };
            if out.entries.insert(id.into(), style).is_some() { return Err(invalid("duplicate_style")); }
        }
        // Resolve each chain once at load, including unused cycles. No style
        // can force an unbounded recursive per-paragraph walk.
        let ids=out.entries.keys().cloned().collect::<Vec<_>>();
        for id in ids { let properties=out.style_properties(&id)?;out.resolved.insert(id,properties); }
        Ok(out)
    }
    fn chain(&self, id: &str) -> Result<Vec<&Style>, String> {
        let mut chain = Vec::new(); let mut seen = HashSet::new(); let mut id = Some(id);
        while let Some(value) = id {
            if !seen.insert(value) || chain.len() >= 64 { return Err(invalid("style_cycle")); }
            let Some(style) = self.entries.get(value) else {
                if chain.is_empty() { return Ok(chain); }
                return Err(invalid("style_base_missing"));
            };
            chain.push(style); id = style.based_on.as_deref();
        }
        Ok(chain)
    }
    fn style_properties(&self, id: &str) -> Result<Properties, String> {
        if let Some(properties)=self.resolved.get(id){return Ok(properties.clone());}
        let mut properties = self.defaults.clone();
        for style in self.chain(id)?.into_iter().rev() { properties.overlay(&style.properties); }
        Ok(properties)
    }
    fn paragraph(&self, node: &XmlNode) -> Result<(Properties, Option<String>, Option<u8>), String> {
        let ppr = child(node, "w:pPr")?;
        let id = ppr.map(|n| val(n, "w:pStyle")).transpose()?.flatten().map(str::to_string).or_else(|| self.default_id.clone());
        let mut properties = id.as_ref().map(|id| self.style_properties(id)).transpose()?.unwrap_or_else(|| self.defaults.clone());
        properties.overlay(&Properties::parse(ppr)?);
        let mut heading = properties.outline.filter(|level| *level < 9).map(|level| level + 1);
        if properties.outline.is_none() {
            if let Some(id) = &id {
                for style in self.chain(id)? {
                    let normalized = style.name.to_ascii_lowercase().replace(' ', "");
                    if let Some(level) = normalized.strip_prefix("heading").and_then(|n| n.parse::<u8>().ok()).filter(|n| (1..=9).contains(n)) { heading = Some(level); break; }
                }
                if heading.is_none() {
                    heading = id.to_ascii_lowercase().strip_prefix("heading").and_then(|n| n.parse::<u8>().ok()).filter(|n| (1..=9).contains(n));
                }
            }
        }
        Ok((properties, id, heading))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InlineEdit {
    /// Replacement for actual w:t text. Keep the existing run's formatting
    /// and apply literal escaping to this string.
    ReplaceText(String),
    /// Generated Markdown syntax, inserted once and never literal escaped.
    Source(String),
    Skip,
}
#[derive(Debug, Clone)]
pub(crate) struct ImportedHeading {
    pub level: u8, pub id: String, pub native_num_id: Option<u32>,
    pub unnumbered: bool,
    /// Only populated when a diagnosed non-global outline is rendered as
    /// current visible text. It must not be combined with automatic numbering.
    pub literal_prefix: String,
}
#[derive(Debug, Clone)]
pub(crate) struct ImportedCaption { pub id: String, pub kind: String, pub automatic: bool }

#[derive(Debug, Default)]
pub(crate) struct ImportPlan {
    pub(crate) config: Option<Value>,
    pub(crate) native_levels: BTreeMap<u32, Vec<NativeLevel>>,
    pub(crate) diagnostics: Vec<Value>,
    headings: HashMap<usize, ImportedHeading>,
    captions: HashMap<usize, ImportedCaption>,
    edits: HashMap<usize, InlineEdit>,
    anchors: HashMap<String, String>,
    signatures: BTreeMap<String, String>,
    caption_boundaries: BTreeMap<String, CaptionBoundaries>,
    reference_texts: HashMap<String, String>,
    cross_paragraph_sequences: HashSet<String>,
    preserved_tables: HashSet<usize>,
}

#[derive(Debug, Clone)]
struct Field {
    key: usize, instruction: String, result: String, nodes: Vec<usize>, nested: Vec<Field>,
    text_result: bool,
}
#[derive(Debug, Clone)]
struct Atom { key: usize, text: String, field: Option<Field>, non_text: bool }
#[derive(Debug, Clone)]
struct Bookmark { name: String, start: usize, end: usize }
#[derive(Debug, Clone)]
struct Paragraph<'a> {
    node: &'a XmlNode, ordinal: usize, top_level: bool,
    properties: Properties, style: Option<String>, heading: Option<u8>,
    atoms: Vec<Atom>, bookmarks: Vec<Bookmark>, visible: String,
    fields_complete: bool, preceding_object: Option<String>, preceding_object_key: Option<usize>,
}
enum Flat<'a> { Node(&'a XmlNode), Simple(&'a XmlNode) }
fn flat_inline<'a>(node: &'a XmlNode, out: &mut Vec<Flat<'a>>) {
    match node.name.as_str() {
        "w:del" | "w:moveFrom" | "w:pPr" | "w:rPr" | "w:sdtPr" => (),
        "w:fldSimple" => out.push(Flat::Simple(node)),
        "w:hyperlink" => {out.push(Flat::Node(node));for child in &node.children{flat_inline(child,out);}},
        "w:sdt" if super::docx_citations::is_control(node) => out.push(Flat::Node(node)),
        "w:t" | "w:instrText" | "w:delInstrText" | "w:fldChar"
        | "w:bookmarkStart" | "w:bookmarkEnd" | "w:tab" | "w:br" | "w:cr"
        | "w:drawing" | "w:pict" | "m:oMath" | "m:oMathPara"
        | "w:footnoteReference" | "w:endnoteReference" => out.push(Flat::Node(node)),
        _ => for child in &node.children { flat_inline(child, out); },
    }
}
fn leaf_text(node: &XmlNode) -> String {
    match node.name.as_str() { "w:t" => node.text.clone(), "w:tab" => "\t".into(),
        "w:br" | "w:cr" => "\n".into(), _ => String::new() }
}
fn field_char(node: &XmlNode) -> Option<&str> {
    (node.name == "w:fldChar").then(|| node.attr("w:fldCharType")).flatten()
}
fn non_text_node(node:&XmlNode)->bool {
    matches!(node.name.as_str(),"w:drawing"|"w:pict"|"m:oMath"|"m:oMathPara"|"w:footnoteReference"|"w:endnoteReference"|"w:hyperlink")
        ||super::docx_citations::is_control(node)
}
fn all_keys(node: &XmlNode, out: &mut Vec<usize>) {
    out.push(node_key(node)); for child in &node.children { all_keys(child, out); }
}
fn simple_field(node: &XmlNode, count: &mut usize, depth: usize) -> Result<Field, String> {
    *count += 1;
    if *count > MAX_FIELDS || depth >= MAX_FIELD_DEPTH { return Err(BUDGET.into()); }
    let instruction = node.attr("w:instr").ok_or_else(|| invalid("simple_field_instruction"))?;
    if instruction.len() > 4096 { return Err(BUDGET.into()); }
    let mut flat = Vec::new(); for child in &node.children { flat_inline(child, &mut flat); }
    let mut at = 0; let mut result = String::new(); let mut nested = Vec::new();let mut text_result=true;
    while at < flat.len() {
        match &flat[at] {
            Flat::Simple(child) => {
                let field = simple_field(child, count, depth + 1)?;
                result.push_str(&field.result); nested.push(field);text_result=false; at += 1;
            }
            Flat::Node(child) if field_char(child) == Some("begin") => {
                let field = complex_field(&flat, &mut at, count, depth + 1)?;
                result.push_str(&field.result); nested.push(field);text_result=false;
            }
            Flat::Node(child) => { result.push_str(&leaf_text(child));text_result &= !non_text_node(child); at += 1; }
        }
    }
    let mut nodes = Vec::new(); all_keys(node, &mut nodes);
    Ok(Field { key: node_key(node), instruction: instruction.into(), result, nodes, nested,text_result })
}
fn complex_field(flat: &[Flat<'_>], at: &mut usize, count: &mut usize, depth: usize) -> Result<Field, String> {
    *count += 1;
    if *count > MAX_FIELDS || depth >= MAX_FIELD_DEPTH { return Err(BUDGET.into()); }
    let Flat::Node(begin) = &flat[*at] else { return Err(invalid("field_begin")); };
    let key = node_key(begin); let mut nodes = vec![key]; *at += 1;
    let mut instruction = String::new(); let mut result = String::new(); let mut nested = Vec::new();
    let mut separated = false;let mut text_result=true;
    while *at < flat.len() {
        let field = match &flat[*at] {
            Flat::Simple(node) => { let field = simple_field(node, count, depth + 1)?; *at += 1; Some(field) }
            Flat::Node(node) if field_char(node) == Some("begin") => Some(complex_field(flat, at, count, depth + 1)?),
            Flat::Node(node) if field_char(node) == Some("separate") => {
                if separated { return Err(invalid("field_second_separator")); }
                separated = true; nodes.push(node_key(node)); *at += 1; None
            }
            Flat::Node(node) if field_char(node) == Some("end") => {
                nodes.push(node_key(node)); *at += 1;
                if !separated { return Err(invalid("field_without_result")); }
                return Ok(Field { key, instruction, result, nodes, nested,text_result });
            }
            Flat::Node(node) => {
                nodes.push(node_key(node));
                if separated { result.push_str(&leaf_text(node));text_result &= !non_text_node(node); }
                else if node.name == "w:instrText" { instruction.push_str(&node.text); }
                *at += 1; None
            }
        };
        if let Some(field) = field {
            if separated { result.push_str(&field.result);text_result=false; }
            else { instruction.push_str(&format!(" {{#{}}} ", nested.len())); }
            nodes.extend(&field.nodes); nested.push(field);
        }
        if instruction.len() > 4096 { return Err(BUDGET.into()); }
    }
    Err(invalid("field_crosses_paragraph"))
}
fn paragraph_content(node: &XmlNode, count: &mut usize) -> Result<(Vec<Atom>, Vec<Bookmark>, bool), String> {
    let mut flat = Vec::new(); for child in &node.children { flat_inline(child, &mut flat); }
    let mut out = Vec::new(); let mut bookmarks = Vec::new();
    let mut open: HashMap<String, (String, usize)> = HashMap::new(); let mut names = HashSet::new();
    let mut at = 0; let mut offset = 0;
    while at < flat.len() {
        let atom = match &flat[at] {
            Flat::Simple(node) => {
                let field = simple_field(node, count, 0)?; at += 1;
                Atom { key: field.key, text: field.result.clone(), field: Some(field),non_text:false }
            }
            Flat::Node(node) if field_char(node) == Some("begin") => {
                match complex_field(&flat, &mut at, count, 0) {
                    Ok(field) => Atom { key: field.key, text: field.result.clone(), field: Some(field),non_text:false },
                    Err(error) if error == BUDGET => return Err(error),
                    // The existing passive converter owns incomplete and
                    // cross-paragraph fields. Never reinterpret part of one.
                    Err(_) => return Ok((Vec::new(), Vec::new(), false)),
                }
            }
            Flat::Node(node) if node.name == "w:bookmarkStart" => {
                let id = node.attr("w:id").ok_or_else(|| invalid("bookmark_id"))?; uint(id)?;
                let name = node.attr("w:name").filter(|s| printable(s, 128) && !s.is_empty()).ok_or_else(|| invalid("bookmark_name"))?;
                if open.insert(id.into(), (name.into(), offset)).is_some() || !names.insert(name.to_ascii_lowercase()) {
                    return Err(invalid("duplicate_bookmark"));
                }
                at += 1; continue;
            }
            Flat::Node(node) if node.name == "w:bookmarkEnd" => {
                let id = node.attr("w:id").ok_or_else(|| invalid("bookmark_id"))?;
                if let Some((name, start)) = open.remove(id) { bookmarks.push(Bookmark { name, start, end: offset }); }
                at += 1; continue;
            }
            Flat::Node(node) if field_char(node).is_some() => return Ok((Vec::new(), Vec::new(), false)),
            Flat::Node(node) => { at += 1; Atom { key: node_key(node), text: leaf_text(node), field: None,non_text:non_text_node(node) } }
        };
        offset += atom.text.len(); out.push(atom);
    }
    Ok((out, bookmarks, true))
}
/// Counter fields may cross paragraphs even though an editable caption
/// cannot. Scan one main-story stack to prevent an invisible consumer from
/// being omitted merely because its visible cache happens to fit our start.
fn cross_paragraph_sequences(document:&XmlNode)->Result<HashSet<String>,String> {
    struct Open { paragraph:usize,instruction:String,result:bool,malformed:bool }
    fn record(field:&Open,out:&mut HashSet<String>){
        let values=tokens(&field.instruction).unwrap_or_default();
        if values.first().is_some_and(|value|value.eq_ignore_ascii_case("SEQ")){
            if let Some(id)=values.get(1).filter(|id|crate::cross_references::valid_sequence(id)){out.insert(id.to_lowercase());}
        }
    }
    fn walk(node:&XmlNode,paragraph:usize,stack:&mut Vec<Open>,out:&mut HashSet<String>,count:&mut usize)->Result<(),String>{
        if matches!(node.name.as_str(),"w:del"|"w:moveFrom"|"w:pPr"|"w:rPr"|"w:sdtPr"|"w:txbxContent"|"w:footnote"|"w:endnote"){return Ok(());}
        let paragraph=if node.name=="w:p"{node_key(node)}else{paragraph};
        if node.name=="w:fldSimple"&&node.descendants().iter().any(|child|matches!(child.name.as_str(),"w:p"|"w:tbl")){
            return Err(invalid("block_inside_simple_field"));
        }
        if let Some(kind)=field_char(node){
            *count+=1;if *count>MAX_FIELDS*3{return Err(BUDGET.into());}
            match kind{
                "begin"=>{
                    if stack.len()>=MAX_FIELD_DEPTH{return Err(BUDGET.into());}
                    stack.push(Open{paragraph,instruction:String::new(),result:false,malformed:false});
                },
                "separate"=>if let Some(field)=stack.last_mut(){field.malformed|=field.result;field.result=true;},
                "end"=>if let Some(field)=stack.pop(){if field.paragraph!=paragraph||!field.result||field.malformed{record(&field,out);}},
                _=>if let Some(field)=stack.last_mut(){field.malformed=true;},
            }
        }else if node.name=="w:instrText"{
            if let Some(field)=stack.last_mut().filter(|field|!field.result){
                field.instruction.push_str(&node.text);if field.instruction.len()>4096{return Err(BUDGET.into());}
            }
        }
        for child in &node.children{walk(child,paragraph,stack,out,count)?;}Ok(())
    }
    let mut out=HashSet::new();let mut stack=Vec::new();walk(document,0,&mut stack,&mut out,&mut 0)?;
    for field in stack{record(&field,&mut out);}Ok(out)
}
fn object_kind(node: &XmlNode) -> Option<&'static str> {
    if node.name == "w:tbl" { return Some("table"); }
    if node.name != "w:p" { return None; }
    let nodes = node.descendants();
    let has_text = nodes.iter().any(|n| n.name == "w:t" && !n.text.trim().is_empty());
    if !has_text && nodes.iter().any(|n| n.name == "m:oMathPara") { return Some("equation"); }
    if !has_text && nodes.iter().any(|n| matches!(n.name.as_str(), "w:drawing" | "w:pict")) { return Some("figure"); }
    None
}
fn collect_paragraphs<'a>(root: &'a XmlNode, styles: &Styles) -> Result<Vec<Paragraph<'a>>, String> {
    fn walk<'a>(node: &'a XmlNode, nested: bool, previous: &mut Option<(String,usize)>,
        styles: &Styles, out: &mut Vec<Paragraph<'a>>, fields: &mut usize) -> Result<(), String> {
        if matches!(node.name.as_str(), "w:del" | "w:moveFrom" | "w:sdtPr" | "w:rPr" | "w:pPr") { return Ok(()); }
        if node.name == "w:p" {
            if out.len() >= MAX_PARAGRAPHS { return Err(BUDGET.into()); }
            let (properties, style, heading) = styles.paragraph(node)?;
            let (atoms, bookmarks, fields_complete) = paragraph_content(node, fields)?;
            let visible = atoms.iter().map(|atom| atom.text.as_str()).collect::<String>();
            // Word requires a paragraph after a table, and our writer emits
            // an empty structural spacer there. It does not separate the
            // following caption from its object. The effective default Normal
            // style does not make it a styled content paragraph. An explicit
            // style, field, bookmark, native list number, heading, section
            // boundary, or any actual content still ends the link.
            let structural_properties = child(node,"w:pPr")?.is_none_or(|ppr|
                !ppr.children.iter().any(|property|matches!(property.name.as_str(),"w:pStyle"|"w:sectPr")));
            let spacer = fields_complete && structural_properties && heading.is_none() && properties.num_id.is_none()
                && bookmarks.is_empty() && atoms.iter().all(|atom|
                    atom.field.is_none() && !atom.non_text && atom.text.is_empty());
            out.push(Paragraph { node, ordinal: out.len(), top_level: !nested, properties, style, heading,
                atoms, bookmarks, visible, fields_complete,
                preceding_object: if nested { None } else { previous.as_ref().map(|(kind,_)|kind.clone()) },
                preceding_object_key: if nested { None } else { previous.as_ref().map(|(_,key)|*key) } });
            if !nested && !spacer { *previous = object_kind(node).map(|kind|(kind.into(),node_key(node))); }
            return Ok(());
        }
        if node.name == "w:tbl" {
            for child in &node.children { walk(child, true, &mut None, styles, out, fields)?; }
            if !nested { *previous = Some(("table".into(),node_key(node))); } return Ok(());
        }
        let nested = nested || matches!(node.name.as_str(), "w:txbxContent" | "w:footnote" | "w:endnote");
        for child in &node.children { walk(child, nested, previous, styles, out, fields)?; }
        Ok(())
    }
    let mut out = Vec::new(); walk(root, false, &mut None, styles, &mut out, &mut 0)?; Ok(out)
}
fn tokens(instruction: &str) -> Option<Vec<String>> {
    let mut out = Vec::new(); let mut chars = instruction.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch.is_whitespace() { continue; }
        let mut value = String::new();
        if ch == '"' {
            let mut ended = false;
            for ch in chars.by_ref() { if ch == '"' { ended = true; break; } value.push(ch); }
            if !ended || chars.peek().is_some_and(|ch| !ch.is_whitespace()) { return None; }
        } else {
            value.push(ch);
            while chars.peek().is_some_and(|ch| !ch.is_whitespace()) { value.push(chars.next()?); }
        }
        out.push(value); if out.len() > 256 { return None; }
    }
    Some(out)
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct Sequence {
    id: String, format: String, chapter: Option<u8>, reset: Option<u32>,
    repeat: bool, hidden: bool, offset: i64,
}
fn field_format(value: &str) -> Option<&'static str> {
    match value { "ARABIC" | "Arabic" | "arabic" => Some("decimal"),
        "roman" => Some("lowerRoman"), "ROMAN" => Some("upperRoman"),
        "alphabetic" => Some("lowerLetter"), "ALPHABETIC" => Some("upperLetter"), _ => None }
}
fn sequence(field: &Field) -> Option<Sequence> {
    let values = tokens(&field.instruction)?;
    if values.first()?.eq_ignore_ascii_case("SEQ") {
        if !field.nested.is_empty(){return None;}
        let id = values.get(1)?;
        if !crate::cross_references::valid_sequence(id) { return None; }
        let mut out = Sequence { id: id.into(), format: "decimal".into(), chapter: None,
            reset: None, repeat: false, hidden: false, offset: 0 };
        let mut at = 2; let mut switches = HashSet::new();
        while at < values.len() {
            let switch = values[at].to_ascii_lowercase();
            if switch != "\\*" && !switches.insert(switch.clone()) { return None; }
            at += 1;
            match switch.as_str() {
                "\\n" => (),
                "\\r" => { out.reset = Some(uint(values.get(at)?).ok()?); at += 1; }
                "\\s" => { let level = uint(values.get(at)?).ok()?; if !(1..=9).contains(&level) { return None; } out.chapter = Some(level as u8); at += 1; }
                "\\c" => out.repeat = true, "\\h" => out.hidden = true,
                "\\*" => {
                    let value = values.get(at)?; at += 1;
                    if !matches!(value.to_ascii_uppercase().as_str(), "MERGEFORMAT" | "CHARFORMAT") { out.format = field_format(value)?.into(); }
                }
                _ => return None,
            }
        }
        return Some(out);
    }
    // Evaluate only a single updateable native SEQ plus/minus a constant.
    if values.len() >= 4 && values[0] == "=" && values[1] == "{#0}" && field.nested.len() == 1 {
        let mut out = sequence(&field.nested[0])?;
        if out.offset != 0 || !matches!(values[2].as_str(), "+" | "-") { return None; }
        let offset = uint(&values[3]).ok()? as i64;
        out.offset = if values[2] == "-" { -offset } else { offset };
        let mut at = 4;
        while at < values.len() {
            if values[at] != "\\*" { return None; }
            let value = values.get(at + 1)?;
            if !matches!(value.to_ascii_uppercase().as_str(), "MERGEFORMAT" | "CHARFORMAT") { out.format = field_format(value)?.into(); }
            at += 2;
        }
        return Some(out);
    }
    None
}
fn style_reference(field: &Field, styles: &Styles) -> Option<u8> {
    let values = tokens(&field.instruction)?;
    if !values.first()?.eq_ignore_ascii_case("STYLEREF") { return None; }
    let target = values.get(1)?;
    let level = target.parse::<u8>().ok().filter(|n| (1..=9).contains(n)).or_else(|| {
        styles.entries.iter().find(|(id, style)| *id == target || style.name.eq_ignore_ascii_case(target))
            .and_then(|(id, _)| styles.style_properties(id).ok()?.outline).filter(|n| *n < 9).map(|n| n + 1)
    })?;
    if values.len() == 3 && values[2].eq_ignore_ascii_case("\\s") { Some(level) } else { None }
}
fn reference(field: &Field) -> Option<String> {
    let values = tokens(&field.instruction)?;
    if !values.first()?.eq_ignore_ascii_case("REF") || values.len() < 2 { return None; }
    let target = &values[1];
    if target.is_empty() || target.bytes().any(|c| !(c.is_ascii_alphanumeric() || c == b'_')) { return None; }
    let mut at = 2;
    while at < values.len() {
        match values[at].to_ascii_lowercase().as_str() {
            "\\h" => at += 1,
            "\\*" if values.get(at + 1).is_some_and(|v| matches!(v.to_ascii_uppercase().as_str(), "MERGEFORMAT" | "CHARFORMAT")) => at += 2,
            _ => return None,
        }
    }
    Some(target.clone())
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CaptionBoundaries { prefix_utf16: usize, label_separator_utf16: usize, caption_separator_utf16: usize }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceIdentity { source_id: String, bookmark: String, role: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceInfo {
    version: u8, shape: Value, identities: Vec<SourceIdentity>,
    signatures: BTreeMap<String, String>, boundaries: BTreeMap<String, CaptionBoundaries>,
}

fn source_info(
    entries: &[(String, Vec<u8>)], part: &str, rels: &[DocxRel],
) -> Result<Option<SourceInfo>, String> {
    let mut matches = rels.iter().filter(|rel| rel.kind == SOURCE_REL);
    let Some(rel) = matches.next() else { return Ok(None); };
    if matches.next().is_some() || !matches!(rel.mode.as_str(), "" | "Internal") {
        return Err(invalid("source_relationship"));
    }
    let base = part.rsplit_once('/').map(|(base, _)| base).unwrap_or("");
    let path = docx_notes::member(base, &rel.raw).map_err(|_| invalid("source_relationship_target"))?;
    let bytes = docx_notes::unique(entries, &path).map_err(|_| invalid("source_member"))?;
    if bytes.len() > MAX_SOURCE_INFO { return Err(BUDGET.into()); }
    let root = docx_notes::xml(bytes).map_err(|_| invalid("source_xml"))?;
    if root.name != "numberingSource" || root.attr("xmlns") != Some(SOURCE_NS) || !root.children.is_empty() {
        return Err(invalid("source_root"));
    }
    let value: SourceInfo = serde_json::from_str(&root.text).map_err(|_| invalid("source_schema"))?;
    if value.version != 1 || value.identities.len() > MAX_PARAGRAPHS
        || value.signatures.len() > MAX_PARAGRAPHS || value.boundaries.len() > MAX_PARAGRAPHS {
        return Err(invalid("source_schema"));
    }
    let mut ids = HashSet::new(); let mut bookmarks = HashSet::new();
    for identity in &value.identities {
        if !safe_source_id(&identity.source_id, &identity.role)
            || !printable(&identity.bookmark, 128)
            || !matches!(identity.role.as_str(), "heading" | "equation" | "figure" | "table")
            || !ids.insert(identity.source_id.as_str()) || !bookmarks.insert(identity.bookmark.to_ascii_lowercase()) {
            return Err(invalid("source_identity"));
        }
    }
    if value.signatures.values().any(|s| s.len() != 64 || !s.bytes().all(|c| c.is_ascii_hexdigit())) {
        return Err(invalid("source_signature"));
    }
    Ok(Some(value))
}
fn safe_source_id(id: &str, role: &str) -> bool {
    let limit = if role == "heading" { 512 } else { 128 };
    if id.is_empty() || id.chars().count() > limit || !id.chars().all(|c| c.is_alphanumeric() || "_:.-".contains(c)) {
        return false;
    }
    role == "heading" || id.starts_with(match role { "equation" => "eq:", "figure" => "fig:", "table" => "tbl:", _ => "\0" })
}
fn paragraph_identity(
    paragraph: &Paragraph<'_>, role: &str, range: std::ops::Range<usize>,
    info: Option<&SourceInfo>,
) -> (String, Option<String>) {
    let candidates: Vec<_> = paragraph.bookmarks.iter().filter(|bookmark| bookmark.start <= range.start
        && bookmark.end >= range.end && bookmark.name != "_GoBack").collect();
    if let Some(info) = info {
        for identity in &info.identities {
            if identity.role == role && candidates.iter().any(|b| b.name == identity.bookmark) {
                return (identity.source_id.clone(), Some(identity.bookmark.clone()));
            }
        }
    }
    let native = candidates.iter().find(|b| b.name.starts_with("_h"))
        .or_else(|| candidates.iter().find(|b| !b.name.starts_with("_Toc"))).copied();
    let prefix = match role { "equation" => "eq:", "figure" => "fig:", "table" => "tbl:", _ => "" };
    if let Some(bookmark) = native {
        let hash = digest(bookmark.name.as_bytes());
        return (format!("{prefix}docx-{}", &hash[..24]), Some(bookmark.name.clone()));
    }
    (format!("{prefix}docx-{}-{}", if role == "heading" { "heading" } else { "caption" }, paragraph.ordinal + 1), None)
}
fn source_presence(value: &Value) -> Value {
    match value {
        Value::Object(values) => Value::Object(values.iter().map(|(key, value)| (key.clone(), source_presence(value))).collect()),
        Value::Array(values) => Value::Array(values.iter().map(source_presence).collect()),
        Value::Null => Value::Null,
        _ => Value::Bool(true),
    }
}
fn apply_presence(value: &Value, shape: &Value) -> Option<Value> {
    match (value, shape) {
        (_, Value::Null) => Some(Value::Null),
        (Value::Object(value), Value::Object(shape)) => Some(Value::Object(shape.iter().filter_map(|(key, shape)| {
            value.get(key).and_then(|value| apply_presence(value, shape)).map(|value| (key.clone(), value))
        }).collect())),
        (Value::Array(value), Value::Array(shape)) if value.len() == shape.len() => Some(Value::Array(
            value.iter().zip(shape).map(|(value, shape)| apply_presence(value, shape)).collect::<Option<Vec<_>>>()?)),
        (_, Value::Bool(true)) => Some(value.clone()), _ => None,
    }
}
fn preserve_presence(native:&Value,shape:&Value)->Value {
    let target=canonical_config(native);
    if let Some(candidate)=apply_presence(native,shape) {
        if canonical_config(&candidate)==target{return candidate;}
    }
    // A native edit may make one previously omitted default significant.
    // Keep that new value while restoring unrelated omissions leaf by leaf.
    fn actions(value:&Value,shape:Option<&Value>,path:&str,out:&mut Vec<(String,bool)>) {
        if shape.is_some_and(Value::is_null){out.push((path.into(),true));return;}
        match value {
            Value::Object(values)=>for(key,value)in values {
                let child=format!("{path}/{}",key.replace('~',"~0").replace('/',"~1"));
                let present=shape.and_then(Value::as_object).and_then(|shape|shape.get(key));
                actions(value,present,&child,out);
                if present.is_none(){out.push((child,false));}
            },
            Value::Array(values)=>for(index,value)in values.iter().enumerate(){
                actions(value,shape.and_then(Value::as_array).and_then(|shape|shape.get(index)),&format!("{path}/{index}"),out);
            },
            _=>(),
        }
    }
    let mut edits=Vec::new();actions(native,Some(shape),"",&mut edits);let mut out=native.clone();
    for(path,null)in edits {
        let Some((owner,key))=path.rsplit_once('/')else{continue;};
        let mut candidate=out.clone();
        if null {
            let Some(value)=candidate.pointer_mut(&path)else{continue;};*value=Value::Null;
        }else{
            let Some(value)=candidate.pointer_mut(owner).and_then(Value::as_object_mut)else{continue;};
            value.remove(&key.replace("~1","/").replace("~0","~"));
        }
        if canonical_config(&candidate)==target{out=candidate;}
    }
    out
}
/// Compare effective native rules after omitted/default shorthand has been
/// resolved. No source-info value participates in this normalization.
fn canonical_config(value: &Value) -> Option<Value> {
    let config: crate::cross_references::NumberingConfig = serde_json::from_value(value.clone()).ok()?;
    crate::cross_references::validate_numbering_config(&config).ok()?;
    let value = serde_json::to_value(config).ok()?;
    let mut out = json!({"captions":value["captions"]});
    if let Some(headings) = value.get("headings").filter(|v| !v.is_null()) {
        let separator = headings["separator"].as_str().unwrap_or(".");
        let suffix = headings["suffix"].as_str().unwrap_or("space");
        let levels = headings["levels"].as_array()?;
        out["headings"] = json!({"levels":levels.iter().enumerate().map(|(index, level)| {
            json!({"format":level["format"].as_str().unwrap_or("decimal"),"start":level["start"].as_u64().unwrap_or(1),
                "restartAfter":level["restartAfter"].as_u64().unwrap_or(index as u64),
                "text":level["text"].as_str().map(str::to_string).unwrap_or_else(||(1..=index+1).map(|n|format!("%{n}")).collect::<Vec<_>>().join(separator)),
                "suffix":level["suffix"].as_str().unwrap_or(suffix),"isLegal":level["isLegal"].as_bool().unwrap_or(false)})
        }).collect::<Vec<_>>()});
    }
    // Import uses the default English source context. An omitted caption
    // prefix is safe only if it still produces the same current native text.
    if let Some(captions) = out["captions"].as_object_mut() {
        for (kind, rule) in captions {
            if rule["prefix"].is_null() {
                rule["prefix"] = Value::String(match kind.as_str() { "equation" => "Equation", "figure" => "Figure", _ => "Table" }.into());
            }
            if rule["sequence"].is_null() || rule.get("sequence").is_none() {
                rule["sequence"] = Value::String(match kind.as_str() { "equation" => "ReadMDEquation", "figure" => "ReadMDFigure", _ => "ReadMDTable" }.into());
            }
        }
    }
    Some(out)
}
fn comment(value: &Value) -> Result<String, String> {
    if canonical_config(value).is_none() { return Err(unsupported("source_rules")); }
    let raw = serde_json::to_string(value).map_err(|_| invalid("source_json"))?
        .replace('<', "\\u003c").replace('>', "\\u003e").replace('&', "\\u0026");
    let value = format!("{}v1 {raw} -->", crate::cross_references::NUMBERING_PREFIX);
    if value.len() > crate::cross_references::MAX_NUMBERING_BYTES { return Err(BUDGET.into()); }
    Ok(value)
}
impl ImportPlan {
    pub(super) fn load(entries: &[(String, Vec<u8>)], document_part: &str, document: &XmlNode, rels: &[DocxRel]) -> Result<Self, String> {
        if document.name != "w:document" { return Err(invalid("document_root")); }
        let mut out = Self::default();
        let info = match source_info(entries, document_part, rels) {
            Ok(info) => info,
            Err(error) => { out.diagnostics.push(json!({"code":CONFLICT,"reason":"source_info_invalid","detail":error,"visible_content_preserved":true})); None }
        };
        let styles = related_part(entries, document_part, rels, "styles")?.as_ref().map(Styles::parse).transpose()?.unwrap_or_default();
        let numbering = related_part(entries, document_part, rels, "numbering")?.as_ref().map(NativeNumbering::parse).transpose()?.unwrap_or_default();
        let paragraphs = collect_paragraphs(document, &styles)?;
        out.cross_paragraph_sequences=cross_paragraph_sequences(document)?;
        let mut bookmark_names = HashSet::new();
        for paragraph in &paragraphs {
            for bookmark in &paragraph.bookmarks {
                if !bookmark_names.insert(bookmark.name.to_ascii_lowercase()) { return Err(invalid("duplicate_bookmark")); }
            }
        }
        let mut active = HashSet::new();
        for paragraph in &paragraphs {
            if paragraph.top_level && paragraph.heading.is_some() {
                if let Some(id) = paragraph.properties.num_id.filter(|id| *id != 0) { active.insert(id); }
            }
        }
        if active.len() > 1 { return Err(unsupported("multiple_outlines")); }
        let mut heading_rules = None;
        if let Some(&id) = active.iter().next() {
            let levels = numbering.levels(id, &styles, &mut HashSet::new())?;
            let max_used = paragraphs.iter().filter(|p| p.properties.num_id == Some(id))
                .map(|p| p.heading.unwrap_or(0)).max().unwrap_or(0);
            if max_used == 0 || max_used > 6 { return Err(unsupported("heading_depth")); }
            let max_defined = levels.keys().filter(|index| **index <= 5).max().copied().unwrap_or(0) + 1;
            let depth = max_used.max(max_defined);
            let mut source_levels = Vec::new();
            for level in 0..depth {
                source_levels.push(levels.get(&level).ok_or_else(|| unsupported("missing_heading_level"))?.source_rule()?);
            }
            let native = levels.values().cloned().collect::<Vec<_>>();
            out.signatures.insert("headings".into(), digest(serde_json::to_string(&native).map_err(|_| invalid("native_json"))?.as_bytes()));
            out.native_levels.insert(id, native);
            for paragraph in &paragraphs {
                if paragraph.properties.num_id != Some(id) { continue; }
                if !paragraph.top_level || paragraph.heading.is_none() { return Err(unsupported("outline_shared_with_list")); }
                let heading = paragraph.heading.unwrap();
                let direct=Properties::parse(child(paragraph.node,"w:pPr")?)?;
                let mut ilvl = direct.ilvl;
                if ilvl.is_none() {
                    if let Some(style) = &paragraph.style {
                        let chain=styles.chain(style)?;
                        let matches = levels.values().filter(|rule| rule.paragraph_style.as_ref().is_some_and(|target|target==style
                            ||styles.entries.get(target).is_some_and(|target|chain.iter().any(|style|std::ptr::eq(*style,target))))).map(|rule| rule.level).collect::<Vec<_>>();
                        if matches.len() > 1 { return Err(invalid("ambiguous_style_level")); }
                        ilvl = matches.first().copied();
                    }
                }
                if ilvl.or(paragraph.properties.ilvl).unwrap_or(0) + 1 != heading { return Err(unsupported("outline_level_differs_from_heading")); }
            }
            let suffix=levels.get(&0).map(|level|level.suffix.as_str()).unwrap_or("space");
            let suffix=if levels.values().filter(|level|level.level<depth).all(|level|level.suffix==suffix){suffix}else{"space"};
            let separator=levels.get(&1).and_then(|rule|rule.text.strip_prefix("%1").and_then(|text|text.strip_suffix("%2")))
                .filter(|separator|printable(separator,16)&&!separator.contains('%'))
                .filter(|separator|levels.values().filter(|rule|rule.level<depth).all(|rule|rule.text==(1..=rule.level+1).map(|level|format!("%{level}")).collect::<Vec<_>>().join(separator)))
                .unwrap_or(".");
            heading_rules = Some(json!({"levels":source_levels,"separator":separator,"suffix":suffix}));
        }
        let mut ids = HashSet::new();
        for paragraph in &paragraphs {
            let title = paragraph.style.as_ref().and_then(|id| styles.entries.get(id)).is_some_and(|s|s.name.eq_ignore_ascii_case("title"));
            let Some(level) = paragraph.heading.or_else(|| (title && heading_rules.is_some()).then_some(1)) else { continue; };
            if !paragraph.top_level || level > 6 { continue; }
            let (id, bookmark) = paragraph_identity(paragraph, "heading", 0..paragraph.visible.len(), info.as_ref());
            if !ids.insert(id.clone()) { return Err(invalid("duplicate_source_identity")); }
            if let Some(bookmark) = bookmark { out.anchors.insert(bookmark, id.clone()); }
            out.headings.insert(node_key(paragraph.node), ImportedHeading { level, id, native_num_id: paragraph.properties.num_id.filter(|id|*id!=0),
                unnumbered: paragraph.properties.num_id==Some(0)||heading_rules.is_some() && paragraph.properties.num_id.is_none(), literal_prefix: String::new() });
        }
        let captions = out.import_captions(&paragraphs, &styles, info.as_ref(), heading_rules.as_ref())?;
        if heading_rules.is_some() || !captions.is_empty() {
            let mut config = json!({"captions":captions});
            if let Some(rules) = heading_rules { config["headings"] = rules; }
            if let Some(info) = &info {
                if info.shape.is_null() {
                    let legacy=json!({"captions":config["captions"].as_object().unwrap().keys().map(|kind|(kind.clone(),json!({}))).collect::<serde_json::Map<String,Value>>()});
                    if config.get("headings").is_none()&&canonical_config(&legacy)==canonical_config(&config){
                        out.import_references(&paragraphs)?;return Ok(out);
                    }
                }else{
                    if info.shape.get("captions").and_then(Value::as_object).is_some_and(|shape|shape.keys().any(|kind|config["captions"].get(kind).is_none())){
                        out.diagnostics.push(json!({"code":CONFLICT,"reason":"configuration_without_native_instance","visible_content_preserved":true}));
                    }
                    config=preserve_presence(&config,&info.shape);
                }
            }
            comment(&config)?; out.config = Some(config);
        }
        out.import_references(&paragraphs)?;
        Ok(out)
    }
    pub(super) fn heading(&self, paragraph: &XmlNode) -> Option<&ImportedHeading> { self.headings.get(&node_key(paragraph)) }
    pub(super) fn caption(&self, paragraph: &XmlNode) -> Option<&ImportedCaption> { self.captions.get(&node_key(paragraph)) }
    pub(super) fn inline_edit(&self, node: &XmlNode) -> Option<&InlineEdit> { self.edits.get(&node_key(node)) }
    /// A captioned native table must stay a table. Projecting it as a form
    /// list would leave the restored label without its actual table object.
    pub(super) fn preserve_table(&self, table: &XmlNode) -> bool { self.preserved_tables.contains(&node_key(table)) }
    pub(super) fn anchor(&self, bookmark: &str) -> Option<&str> { self.anchors.get(bookmark).map(String::as_str) }
    pub(super) fn literal_mode(&self) -> bool { self.config.is_some() || !self.headings.is_empty() || !self.captions.is_empty() }
    pub(super) fn finish(&self, mut markdown: String) -> Result<String, String> {
        if let Some(config) = &self.config {
            let declaration = comment(config)?;
            let body = crate::md_ast::source_without_front_matter(&markdown);
            let at = markdown.len() - body.len();
            let leading = if at != 0 && !markdown[..at].ends_with('\n') { "\n" } else { "" };
            markdown.insert_str(at, &format!("{leading}{declaration}\n\n"));
        }
        if !self.diagnostics.is_empty() {
            let data = serde_json::to_string(&self.diagnostics).map_err(|_| invalid("diagnostics"))?
                .replace('<', "\\u003c").replace('>', "\\u003e").replace('&', "\\u0026");
            markdown.push_str(&format!("\n\n<!--readmd-numbering-import:v1 {data}-->\n"));
        }
        Ok(markdown)
    }
}

#[derive(Debug, Clone)]
struct Chapter { paragraph: usize, number: String }
#[derive(Debug, Clone)]
struct CounterValue { value: u32, chapter: Option<usize> }
#[derive(Debug, Clone)]
struct CaptionCandidate {
    paragraph: usize, kind: String, id: String, bookmark: Option<String>,
    config: Value, label_text: String, remove_to: usize,
    boundaries: CaptionBoundaries, signature: String, valid: bool,
}
fn byte_at_utf16(text: &str, units: usize) -> Option<usize> {
    if units == 0 { return Some(0); }
    let mut count = 0;
    for (at, ch) in text.char_indices() {
        count += ch.len_utf16();
        if count == units { return Some(at + ch.len_utf8()); }
        if count > units { return None; }
    }
    None
}
fn caption_signature(sequence: &Sequence, chapter: Option<&Field>, prefix: &str, caption_separator: &str) -> String {
    digest(serde_json::to_string(&json!({"sequence":sequence.id,"format":sequence.format,
        "chapter":sequence.chapter,"reset":sequence.reset,"offset":sequence.offset,
        "chapterField":chapter.map(|field|field.instruction.split_whitespace().collect::<Vec<_>>()),
        "prefix":prefix,"captionSeparator":caption_separator})).unwrap().as_bytes())
}
fn split_caption<'a>(
    before: &'a str, after: &'a str, sequence: &Sequence, chapter: Option<&Field>,
    bookmark: Option<&str>, info: Option<&SourceInfo>,
) -> (&'a str, &'a str, &'a str, CaptionBoundaries, String) {
    if let Some((info, bookmark)) = info.zip(bookmark) {
        if let Some(boundaries) = info.boundaries.get(bookmark) {
            if let (Some(prefix), Some(total), Some(after_at)) = (
                byte_at_utf16(before, boundaries.prefix_utf16),
                byte_at_utf16(before, boundaries.prefix_utf16 + boundaries.label_separator_utf16),
                byte_at_utf16(after, boundaries.caption_separator_utf16),
            ) {
                if total == before.len() {
                    let signature = caption_signature(sequence, chapter, before, &after[..after_at]);
                    if info.signatures.get(&format!("caption:{bookmark}")) == Some(&signature) {
                        return (&before[..prefix], &before[prefix..], &after[..after_at], boundaries.clone(), signature);
                    }
                }
            }
        }
    }
    let prefix_end = before.trim_end_matches(char::is_whitespace).len();
    let separator_end = if after.starts_with(" — ") { " — ".len() }
        else if after.starts_with(": ") { 2 }
        else { after.len() - after.trim_start_matches(char::is_whitespace).len() };
    let boundaries = CaptionBoundaries {
        prefix_utf16: before[..prefix_end].encode_utf16().count(),
        label_separator_utf16: before[prefix_end..].encode_utf16().count(),
        caption_separator_utf16: after[..separator_end].encode_utf16().count(),
    };
    let signature = caption_signature(sequence, chapter, before, &after[..separator_end]);
    (&before[..prefix_end], &before[prefix_end..], &after[..separator_end], boundaries, signature)
}
fn overwrite_field(edits: &mut HashMap<usize, InlineEdit>, field: &Field, replacement: Option<String>) {
    for key in &field.nodes { edits.insert(*key, InlineEdit::Skip); }
    if let Some(replacement) = replacement { edits.insert(field.key, InlineEdit::Source(replacement)); }
}
fn remove_prefix(edits: &mut HashMap<usize, InlineEdit>, paragraph: &Paragraph<'_>, to: usize) -> Result<(), String> {
    let mut offset = 0;
    for atom in &paragraph.atoms {
        if offset >= to { break; }
        if atom.non_text{return Err(unsupported("caption_prefix_or_separator_object"));}
        let end = offset + atom.text.len();
        if let Some(field) = &atom.field {
            if end > to { return Err(invalid("partial_field_edit")); }
            overwrite_field(edits, field, None);
        } else if end <= to {if !atom.text.is_empty(){edits.insert(atom.key, InlineEdit::Skip);}}
        else {
            let at = to - offset;
            let value = atom.text.get(at..).ok_or_else(|| invalid("text_boundary"))?;
            edits.insert(atom.key, InlineEdit::ReplaceText(value.into()));
        }
        offset = end;
    }
    Ok(())
}
impl ImportPlan {
    fn heading_chapters(&self, paragraphs: &[Paragraph<'_>], rules: Option<&Value>) -> Result<HashMap<usize, String>, String> {
        let Some(rules) = rules else { return Ok(HashMap::new()); };
        let mut source = comment(&json!({"headings":rules}))?; source.push_str("\n\n");
        let mut keys = Vec::new();
        for paragraph in paragraphs {
            if let Some(heading) = self.headings.get(&node_key(paragraph.node)) {
                source.push_str(&format!("{} Native {{#{} {}}}\n\n", "#".repeat(heading.level as usize), heading.id,
                    if heading.unnumbered { ".unnumbered" } else { "" }));
                keys.push(node_key(paragraph.node));
            }
        }
        let plan = crate::cross_references::analyze_numbering(&source).map_err(|_|unsupported("heading_counter_hierarchy"))?
            .ok_or_else(|| invalid("heading_plan"))?;
        Ok(keys.into_iter().zip(plan.headings).filter_map(|(key, heading)| heading.numbering.map(|numbering|(key,numbering.text))).collect())
    }
    fn import_captions(
        &mut self, paragraphs: &[Paragraph<'_>], styles: &Styles, info: Option<&SourceInfo>, heading_rules: Option<&Value>,
    ) -> Result<serde_json::Map<String, Value>, String> {
        let heading_numbers = self.heading_chapters(paragraphs, heading_rules)?;
        let mut chapters: Vec<Option<Chapter>> = vec![None; 6];
        let mut counters: HashMap<String, CounterValue> = HashMap::new();
        let mut values: HashMap<usize, (Sequence, u32, Option<Chapter>)> = HashMap::new();
        let mut consumers: HashMap<String, HashSet<usize>> = HashMap::new();
        let mut opaque_sequences=self.cross_paragraph_sequences.clone();
        fn evaluate(
            field: &Field, chapters: &[Option<Chapter>], counters: &mut HashMap<String, CounterValue>,
            values: &mut HashMap<usize, (Sequence, u32, Option<Chapter>)>, consumers: &mut HashMap<String, HashSet<usize>>,
        ) -> Result<(), String> {
            if let Some(sequence) = sequence(field) {
                let key = sequence.id.to_lowercase();
                let chapter = sequence.chapter.and_then(|level| chapters.get(level as usize - 1)).cloned().flatten();
                let chapter_id = chapter.as_ref().map(|chapter|chapter.paragraph);
                let previous = counters.get(&key);
                let value = if let Some(reset) = sequence.reset { reset }
                    else if sequence.repeat { previous.map(|state|state.value).unwrap_or(0) }
                    else if previous.is_none_or(|state|sequence.chapter.is_some() && state.chapter != chapter_id) { 1 }
                    else { previous.unwrap().value.checked_add(1).ok_or(BUDGET)? };
                counters.insert(key.clone(), CounterValue { value, chapter: chapter_id });
                let displayed = i64::from(value).checked_add(sequence.offset).filter(|n|*n>=0 && *n<=crate::cross_references::MAX_NUMBER as i64).ok_or_else(||unsupported("sequence_value"))? as u32;
                consumers.entry(key).or_default().insert(field.key);
                values.insert(field.key, (sequence, displayed, chapter));
            } else {
                for nested in &field.nested { evaluate(nested, chapters, counters, values, consumers)?; }
            }
            Ok(())
        }
        for paragraph in paragraphs {
            if paragraph.top_level {
                if let Some(heading) = self.headings.get(&node_key(paragraph.node)) {
                    let index = heading.level as usize - 1;
                    for chapter in &mut chapters[index..] { *chapter = None; }
                    if let Some(number) = heading_numbers.get(&node_key(paragraph.node)) {
                        chapters[index] = Some(Chapter { paragraph: paragraph.ordinal, number: number.clone() });
                    }
                }
            }
            for field in paragraph.atoms.iter().filter_map(|atom|atom.field.as_ref()) {
                if sequence(field).is_none() {
                    let values=tokens(&field.instruction).unwrap_or_default();
                    if values.first().is_some_and(|value|value.eq_ignore_ascii_case("SEQ")) {
                        if let Some(id)=values.get(1).filter(|id|crate::cross_references::valid_sequence(id)){opaque_sequences.insert(id.to_lowercase());}
                        self.diagnostics.push(json!({"code":CONFLICT,"reason":"unsupported_sequence_field","paragraph":paragraph.ordinal,"visible_content_preserved":true}));
                    }
                }
                evaluate(field, &chapters, &mut counters, &mut values, &mut consumers)?;
            }
        }
        let mut candidates = Vec::new();
        let mut ordinals: HashMap<(String,Option<usize>),u32> = HashMap::new();
        let mut owned: HashSet<usize> = HashSet::new();
        for paragraph in paragraphs {
            let sequence_atoms = paragraph.atoms.iter().enumerate().filter(|(_,atom)| values.contains_key(&atom.key)).collect::<Vec<_>>();
            if sequence_atoms.is_empty() { continue; }
            let Some(kind) = paragraph.preceding_object.as_deref().filter(|_|paragraph.top_level && paragraph.fields_complete) else { continue; };
            if sequence_atoms.len() != 1 { return Err(unsupported("multiple_caption_sequences")); }
            let (index, atom) = sequence_atoms[0];
            let (sequence, value, chapter) = values.get(&atom.key).unwrap();
            if sequence.repeat || sequence.hidden || sequence.chapter.is_some_and(|level|level>6) {
                return Err(unsupported("caption_sequence_switch"));
            }
            let field = atom.field.as_ref().unwrap();
            let chapter_atom = index.checked_sub(1).and_then(|mut index| {
                while paragraph.atoms[index].field.is_none() {
                    if index == 0 { return None; } index -= 1;
                }
                paragraph.atoms[index].field.as_ref().and_then(|field|style_reference(field,styles).map(|level|(index,field,level)))
            });
            if sequence.chapter != chapter_atom.map(|(_,_,level)|level)
                || sequence.chapter.is_some() && (heading_rules.is_none() || chapter.is_none()) {
                return Err(unsupported("caption_chapter_field"));
            }
            let first = chapter_atom.map(|(index,_,_)|index).unwrap_or(index);
            if paragraph.atoms[..first].iter().any(|atom|atom.field.is_some()) { return Err(unsupported("caption_prefix_field")); }
            let number_start = paragraph.atoms[..first].iter().map(|atom|atom.text.len()).sum::<usize>();
            let number_end = paragraph.atoms[..=index].iter().map(|atom|atom.text.len()).sum::<usize>();
            let before = &paragraph.visible[..number_start];
            let after = &paragraph.visible[number_end..];
            let (id, bookmark) = paragraph_identity(paragraph, kind, number_start..number_end, info);
            let (prefix, label_separator, caption_separator, boundaries, signature) =
                split_caption(before, after, sequence, chapter_atom.map(|(_,field,_)|field), bookmark.as_deref(), info);
            let separator = if let Some((chapter_index,_,_)) = chapter_atom {
                paragraph.atoms[chapter_index+1..index].iter().map(|atom|atom.text.as_str()).collect::<String>()
            } else { ".".into() };
            let ordinal = ordinals.entry((kind.into(), chapter.as_ref().map(|chapter|chapter.paragraph))).or_default(); *ordinal += 1;
            let start = value.checked_add(1).and_then(|value|value.checked_sub(*ordinal)).ok_or_else(||unsupported("caption_restart"))?;
            if !printable(prefix,128) || !printable(label_separator,16) || !printable(caption_separator,32) || !printable(&separator,16) {
                return Err(unsupported("caption_literal_segments"));
            }
            let mut valid = field.text_result&&field.result == crate::cross_references::number_text(&sequence.format,*value as usize);
            if let Some((_,field,_)) = chapter_atom {
                valid &= field.text_result&&chapter.as_ref().is_some_and(|chapter|field.result==chapter.number);
            }
            let label_text = paragraph.visible[..number_end].to_string();
            let config = json!({"scope":if sequence.chapter.is_some(){"chapter"}else{"document"},
                "chapterLevel":sequence.chapter.unwrap_or(1),"format":sequence.format,"start":start,"prefix":prefix,
                "labelSeparator":label_separator,"separator":separator,"captionSeparator":caption_separator,"sequence":sequence.id});
            if kind=="table" {
                self.preserved_tables.insert(paragraph.preceding_object_key.ok_or_else(||invalid("caption_object_identity"))?);
            }
            owned.insert(atom.key);
            candidates.push(CaptionCandidate { paragraph: node_key(paragraph.node), kind: kind.into(), id, bookmark,
                config, label_text, remove_to: number_end + caption_separator.len(), boundaries, signature, valid });
        }
        let mut configs = serde_json::Map::new(); let mut conflicting = HashSet::new();
        for candidate in &candidates {
            let sequence = candidate.config["sequence"].as_str().unwrap().to_lowercase();
            if opaque_sequences.contains(&sequence){return Err(unsupported("opaque_sequence_consumes_caption_counter"));}
            if consumers.get(&sequence).is_some_and(|fields|fields.iter().any(|key|!owned.contains(key))) {
                return Err(unsupported("caption_sequence_shared_with_other_content"));
            }
            if let Some(previous) = configs.get(&candidate.kind) {
                if previous != &candidate.config { return Err(unsupported("nonuniform_caption_rules")); }
            } else { configs.insert(candidate.kind.clone(),candidate.config.clone()); }
            if !candidate.valid { conflicting.insert(candidate.kind.clone()); }
        }
        let mut ids: HashSet<String> = self.headings.values().map(|heading|heading.id.clone()).collect();
        for candidate in candidates {
            if !ids.insert(candidate.id.clone()) { return Err(invalid("duplicate_source_identity")); }
            let automatic = !conflicting.contains(&candidate.kind);
            if automatic {
                let paragraph = paragraphs.iter().find(|p|node_key(p.node)==candidate.paragraph).unwrap();
                remove_prefix(&mut self.edits, paragraph, candidate.remove_to)?;
                self.reference_texts.insert(candidate.id.clone(),candidate.label_text);
            } else {
                self.diagnostics.push(json!({"code":CONFLICT,"reason":"caption_result_modified","kind":candidate.kind,"source_id":candidate.id,"visible_content_preserved":true}));
            }
            if let Some(bookmark) = candidate.bookmark {
                self.anchors.insert(bookmark.clone(),candidate.id.clone());
                if automatic {
                    self.signatures.insert(format!("caption:{bookmark}"),candidate.signature);
                    self.caption_boundaries.insert(bookmark,candidate.boundaries);
                }
            }
            self.captions.insert(candidate.paragraph,ImportedCaption { id:candidate.id,kind:candidate.kind,automatic });
        }
        for kind in conflicting { configs.remove(&kind); }
        Ok(configs)
    }
    fn import_references(&mut self, paragraphs: &[Paragraph<'_>]) -> Result<(),String> {
        for paragraph in paragraphs {
            for field in paragraph.atoms.iter().filter_map(|atom|atom.field.as_ref()) {
                if self.edits.contains_key(&field.key) { continue; }
                let Some(bookmark) = reference(field) else {
                    if tokens(&field.instruction).and_then(|values|values.first().cloned()).is_some_and(|kind|kind.eq_ignore_ascii_case("REF")){
                        self.diagnostics.push(json!({"code":CONFLICT,"reason":"unsupported_reference_field","paragraph":paragraph.ordinal,"visible_content_preserved":true}));
                    }
                    continue;
                };
                let Some(id) = self.anchors.get(&bookmark) else { continue; };
                if !field.text_result{
                    self.diagnostics.push(json!({"code":CONFLICT,"reason":"reference_result_contains_objects","source_id":id,"visible_content_preserved":true}));
                    continue;
                }
                let replacement = if self.reference_texts.get(id) == Some(&field.result) {
                    format!(":ref[{id}]")
                } else {
                    self.diagnostics.push(json!({"code":CONFLICT,"reason":if self.reference_texts.contains_key(id){"reference_result_modified"}else{"reference_preserved_as_link"},
                        "source_id":id,"visible_content_preserved":true}));
                    format!("[{}](#{id})",super::docx_citations::literal_text(&field.result))
                };
                overwrite_field(&mut self.edits,field,Some(replacement));
            }
        }
        Ok(())
    }
}

fn append_part_xml(parts: &mut [(String,Vec<u8>)], name: &str, close: &str, addition: &str) -> Result<(),String> {
    let indices = parts.iter().enumerate().filter(|(_,part)|part.0==name).map(|(index,_)|index).collect::<Vec<_>>();
    if indices.len()!=1 { return Err(invalid("source_package_member")); }
    let bytes=&mut parts[indices[0]].1;
    let text=std::str::from_utf8(bytes).map_err(|_|invalid("source_package_xml"))?;
    let at=text.rfind(close).ok_or_else(||invalid("source_package_xml"))?;
    let mut updated=text.to_string();updated.insert_str(at,addition);*bytes=updated.into_bytes();Ok(())
}
fn xml_escape(text:&str)->String {
    text.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;").replace('"',"&quot;")
}
fn main_part(entries:&[(String,Vec<u8>)])->Result<String,String> {
    if !entries.iter().any(|(name,_)|name=="_rels/.rels") {
        return if entries.iter().any(|(name,_)|name=="word/document.xml"){Ok("word/document.xml".into())}else{Err(invalid("main_part"))};
    }
    let root=docx_notes::xml(docx_notes::unique(entries,"_rels/.rels").map_err(|_|invalid("main_relationships"))?).map_err(|_|invalid("main_relationships"))?;
    if root.name!="pkg:Relationships"{return Err(invalid("main_relationships"));}
    let mut related=root.children.iter().filter(|n|n.name=="pkg:Relationship"&&n.attr("Type").is_some_and(|kind|relation(kind,"officeDocument")));
    let rel=related.next().ok_or_else(||invalid("main_relationship"))?;
    if related.next().is_some()||!matches!(rel.attr("TargetMode").unwrap_or(""),""|"Internal"){return Err(invalid("main_relationship"));}
    docx_notes::member("",rel.attr("Target").ok_or_else(||invalid("main_relationship"))?).map_err(|_|invalid("main_target"))
}
/// Called only after the writer has emitted the final native document. The
/// part contains no old heading/caption value or author body. Its value-free
/// sparse mask can remove a field only if current native semantics stay equal.
pub(crate) fn attach_source_info(parts:&mut Vec<(String,Vec<u8>)>,source:&str)->Result<(),String> {
    let source_plan=crate::cross_references::analyze_numbering(source).map_err(str::to_string)?;
    let projection=crate::cross_references::project(source);
    let labels=projection.content.lines().filter_map(crate::cross_references::read_marker).collect::<Vec<_>>();
    if source_plan.is_none()&&labels.is_empty(){return Ok(());}
    if parts.iter().any(|(name,_)|name==SOURCE_PART){return Err(invalid("duplicate_source_part"));}
    let document_part=main_part(parts)?;
    let document=docx_notes::xml(docx_notes::unique(parts,&document_part).map_err(|_|invalid("document_member"))?).map_err(|_|invalid("document_xml"))?;
    let rels=docx_notes::relationships(parts,&document_part).map_err(|_|invalid("document_relationships"))?;
    let native=ImportPlan::load(parts,&document_part,&document,&rels)?;
    let styles=related_part(parts,&document_part,&rels,"styles")?.as_ref().map(Styles::parse).transpose()?.unwrap_or_default();
    let paragraphs=collect_paragraphs(&document,&styles)?;
    let mut identities=Vec::new();
    let mut signatures=native.signatures.clone();
    let mut boundaries=native.caption_boundaries.clone();
    if let Some(plan)=&source_plan {
        for heading in &plan.headings {
            let bookmark=crate::docx_writer::bookmark_name(&heading.id);
            if native.anchors.contains_key(&bookmark) {
                identities.push(SourceIdentity{source_id:heading.id.clone(),bookmark,role:"heading".into()});
            }
        }
    }
    for label in labels {
        let bookmark=crate::docx_writer::bookmark_name(&label.id);
        let Some(paragraph)=paragraphs.iter().find(|p|p.bookmarks.iter().any(|b|b.name==bookmark))else{continue;};
        if !native.anchors.contains_key(&bookmark){continue;}
        identities.push(SourceIdentity{source_id:label.id.clone(),bookmark:bookmark.clone(),role:label.kind.clone()});
        let sequence_atom=paragraph.atoms.iter().enumerate().find_map(|(index,atom)|atom.field.as_ref().and_then(|field|sequence(field).map(|seq|(index,field,seq))));
        let Some((index,_,sequence))=sequence_atom else{continue;};
        let chapter=paragraph.atoms[..index].iter().filter_map(|atom|atom.field.as_ref()).find(|field|style_reference(field,&styles).is_some());
        let first=chapter.map(|field|paragraph.atoms.iter().position(|atom|atom.key==field.key).unwrap()).unwrap_or(index);
        let before=paragraph.atoms[..first].iter().map(|atom|atom.text.as_str()).collect::<String>();
        let number_end=paragraph.atoms[..=index].iter().map(|atom|atom.text.len()).sum::<usize>();
        let after=&paragraph.visible[number_end..];
        let label_separator=label.numbering.as_ref().map(|n|n.label_separator.as_str()).unwrap_or(" ");
        let caption_separator=label.caption_separator();
        if before!=format!("{}{label_separator}",label.prefix)||!after.starts_with(caption_separator)&&!after.is_empty(){return Err(invalid("writer_caption_binding"));}
        let actual_caption_separator=if after.is_empty(){""}else{caption_separator};
        let boundary=CaptionBoundaries{prefix_utf16:label.prefix.encode_utf16().count(),label_separator_utf16:label_separator.encode_utf16().count(),caption_separator_utf16:actual_caption_separator.encode_utf16().count()};
        signatures.insert(format!("caption:{bookmark}"),caption_signature(&sequence,chapter,&before,actual_caption_separator));
        boundaries.insert(bookmark,boundary);
    }
    let shape=if let Some(plan)=source_plan {
        let raw=source[plan.bytes].trim().strip_prefix(crate::cross_references::NUMBERING_PREFIX).and_then(|raw|raw.strip_prefix("v1 ")).and_then(|raw|raw.strip_suffix("-->")).ok_or_else(||invalid("source_declaration"))?.trim();
        let value:Value=serde_json::from_str(raw).map_err(|_|invalid("source_declaration"))?;source_presence(&value)
    }else{Value::Null};
    let info=SourceInfo{version:1,shape,identities,signatures,boundaries};
    let data=serde_json::to_string(&info).map_err(|_|invalid("source_json"))?;
    if data.len()>MAX_SOURCE_INFO{return Err(BUDGET.into());}
    let (base,file)=document_part.rsplit_once('/').unwrap_or(("",document_part.as_str()));
    let rel_part=if base.is_empty(){format!("_rels/{file}.rels")}else{format!("{base}/_rels/{file}.rels")};
    let rel_root=docx_notes::xml(docx_notes::unique(parts,&rel_part).map_err(|_|invalid("source_relationships"))?).map_err(|_|invalid("source_relationships"))?;
    if rel_root.name!="pkg:Relationships"||rels.iter().any(|rel|rel.id=="readmdNumberingSource"||rel.kind==SOURCE_REL){return Err(invalid("source_relationships"));}
    let types=docx_notes::xml(docx_notes::unique(parts,"[Content_Types].xml").map_err(|_|invalid("source_content_types"))?).map_err(|_|invalid("source_content_types"))?;
    if types.name!="Types"||types.attr("xmlns")!=Some("http://schemas.openxmlformats.org/package/2006/content-types")
        ||types.children.iter().any(|n|n.attr("PartName")==Some(&format!("/{SOURCE_PART}"))){return Err(invalid("source_content_types"));}
    let target=format!("{}{SOURCE_PART}","../".repeat(base.split('/').filter(|part|!part.is_empty()).count()));
    append_part_xml(parts,&rel_part,"</Relationships>",&format!("<Relationship Id=\"readmdNumberingSource\" Type=\"{SOURCE_REL}\" Target=\"{}\"/>",xml_escape(&target)))?;
    append_part_xml(parts,"[Content_Types].xml","</Types>",&format!("<Override PartName=\"/{SOURCE_PART}\" ContentType=\"application/vnd.readmd.numbering-source+xml\"/>"))?;
    parts.push((SOURCE_PART.into(),format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><numberingSource xmlns=\"{SOURCE_NS}\">{}</numberingSource>",xml_escape(&data)).into_bytes()));
    Ok(())
}

#[cfg(test)]
#[path = "docx_numbering_import_tests.rs"]
mod tests;
