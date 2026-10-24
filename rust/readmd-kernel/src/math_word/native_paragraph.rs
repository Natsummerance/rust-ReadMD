//! DOCX-only paragraph definitions. This module never materializes source,
//! changes template styles, reads files, or infers declarations from values.
use super::{attr, child, word, WordMathStyles};
use crate::math_document::{XmlNode, XmlPart, WORD_NS, XML_NS};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

type Result<T> = std::result::Result<T, &'static str>;
const MAX_SOURCES: usize = 4096;
const MAX_DEFINITIONS: usize = 4096;
const MAX_CHAIN: usize = 128;
const MAX_BYTES: usize = 16 * 1024 * 1024;

#[path = "native_runs.rs"]
mod native_runs;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NativeParagraphDefaultsKind { Missing, Empty, Explicit }

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct NativeParagraphAlias {
    pub source_id: String,
    pub native_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct NativeParagraphLoss {
    pub code: &'static str,
    pub source_defaults: NativeParagraphDefaultsKind,
    pub template_defaults: NativeParagraphDefaultsKind,
    pub selected_style_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeParagraphProjection {
    /// Contents of the single outer w:pPr; the caller supplies that wrapper.
    pub ppr_xml: String,
    pub selected_style_id: String,
    pub aliases: Vec<NativeParagraphAlias>,
    pub selection_was_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NativeParagraphResolution {
    Native(NativeParagraphProjection),
    Materialized(NativeParagraphLoss),
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq)]
struct Definition {
    source_id: String,
    kind: &'static str,
    parent: Option<String>,
    ppr: Option<XmlNode>,
    rpr: Option<XmlNode>,
    key: String,
}

/// Construction is sealed by capture(). Original containers remain available
/// here; only an independently emitted paragraph subset reaches the registry.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NativeParagraphSource {
    direct: Option<XmlNode>,
    chain: Vec<Definition>,
    characters: Vec<Definition>,
    runs: native_runs::RunSources,
    defaults: Option<XmlNode>,
    selection_was_default: bool,
    paragraph_mark: bool,
    key: String,
    retained_bytes: usize,
}
impl NativeParagraphSource {
    /// Same bounded descriptor charge used by prepare; excludes formula runs.
    pub(crate) fn description_bytes(&self) -> usize {
        self.retained_bytes + self.key.len() + self.chain.iter().chain(&self.characters).map(|definition| definition.key.len()).sum::<usize>()
    }
    fn defaults_kind(&self) -> NativeParagraphDefaultsKind { defaults_kind(self.defaults.as_ref()) }
}

fn charge(total: &mut usize, bytes: usize) -> Result<()> {
    if crate::speech::cancelled() { return Err("cancelled"); }
    *total = total.checked_add(bytes).ok_or("math_budget")?;
    if *total > MAX_BYTES { return Err("math_budget"); }
    Ok(())
}

fn tree_budget(root: &XmlNode) -> Result<usize> {
    let mut stack = vec![(root, 0usize)];
    let (mut nodes, mut bytes) = (0usize, 0usize);
    while let Some((node, depth)) = stack.pop() {
        if crate::speech::cancelled() { return Err("cancelled"); }
        nodes += 1;
        if nodes > 12000 || depth > 128 { return Err("math_budget"); }
        if !(depth == 0 && node.name == "#root") && !crate::math_document::xml_qname(&node.name) {
            return Err("math_invalid_property");
        }
        charge(&mut bytes, 64 + node.id.len() + node.name.len() + node.namespace.len())?;
        for (key, value) in &node.attributes {
            if !crate::math_document::xml_qname(key) || !xml_characters(value) { return Err("math_invalid_property"); }
            charge(&mut bytes, 16 + key.len() + value.len())?;
        }
        for (key, value) in &node.attribute_namespaces { charge(&mut bytes, 16 + key.len() + value.len())?; }
        for part in &node.content { match part {
            XmlPart::Element(child) => stack.push((child, depth + 1)),
            XmlPart::Text(value) | XmlPart::Comment(value) | XmlPart::Instruction(value) => charge(&mut bytes, 16 + value.len())?,
        }}
    }
    Ok(bytes)
}

fn xml_characters(value: &str) -> bool {
    value.chars().all(|c| matches!(c, '\t' | '\r' | '\n') || (c >= ' ' && !matches!(c, '\u{fffe}' | '\u{ffff}')))
}

fn attribute(value: &str) -> String {
    let mut out = String::new();
    for c in value.chars() { match c {
        '&' => out.push_str("&amp;"), '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"), '"' => out.push_str("&quot;"),
        '\t' => out.push_str("&#9;"), '\r' => out.push_str("&#13;"), '\n' => out.push_str("&#10;"),
        _ => out.push(c),
    }}
    out
}

// A namespace-aware attribute emitter, not XmlNode.xml() followed by reparsing.
// Whitespace/comments outside declarations are not style metadata fidelity.
fn emit(node: &XmlNode) -> Result<String> {
    if node.namespace != WORD_NS { return Err("math_invalid_property"); }
    let mut out = format!("<w:{} xmlns:w=\"{}\"", node.local(), WORD_NS);
    let mut attributes = BTreeMap::new();
    for (key, value) in &node.attributes {
        if key == "xmlns" || key.starts_with("xmlns:") { continue; }
        let uri = node.attribute_namespaces.get(key).map(String::as_str).unwrap_or("");
        let prefix = match uri { WORD_NS => "w", XML_NS => "xml", _ => return Err("math_invalid_property") };
        let name = format!("{prefix}:{}", key.rsplit(':').next().unwrap_or(key));
        if attributes.insert(name, value).is_some() { return Err("math_invalid_property"); }
    }
    for (key, value) in attributes { out.push_str(&format!(" {key}=\"{}\"", attribute(value))); }
    let children = node.children().filter(|child| !word(child, "rPr")).collect::<Vec<_>>();
    if node.content.iter().any(|part| matches!(part, XmlPart::Text(value) if !value.trim().is_empty())) {
        return Err("math_invalid_property");
    }
    if children.is_empty() { out.push_str("/>"); } else {
        out.push('>');
        for child in children { out.push_str(&emit(child)?); if out.len() > MAX_BYTES { return Err("math_budget"); } }
        out.push_str(&format!("</w:{}>", node.local()));
    }
    if out.len() > MAX_BYTES { return Err("math_budget"); }
    Ok(out)
}

fn ppr_xml(node: Option<&XmlNode>) -> Result<Option<String>> { node.map(emit).transpose() }

fn defaults_node(styles: Option<&XmlNode>) -> Option<XmlNode> {
    styles.and_then(|s| child(s, "docDefaults")).and_then(|s| child(s, "pPrDefault")).cloned()
}
fn defaults_kind(node: Option<&XmlNode>) -> NativeParagraphDefaultsKind {
    match node {
        None => NativeParagraphDefaultsKind::Missing,
        Some(node) if child(node, "pPr").is_none_or(|pr| pr.children().next().is_none()) => NativeParagraphDefaultsKind::Empty,
        Some(_) => NativeParagraphDefaultsKind::Explicit,
    }
}
fn json(value: &impl Serialize) -> Result<String> { serde_json::to_string(value).map_err(|_| "math_invalid_property") }

/// Validate first, capture raw declarations second. No malformed chain or
/// unsupported effective paragraph can be converted into a successful fallback.
pub(crate) fn capture(root: &XmlNode) -> Result<Option<NativeParagraphSource>> {
    tree_budget(root)?;
    let context = super::word_paragraph_context(root)?;
    context.finite_display(true).map_err(|_| "math_unsupported_extension")?;
    let styles_node = child(root, "styles");
    let direct = child(root, "pPr").cloned();
    let styles = styles_node.map(WordMathStyles::from_node).transpose()?.unwrap_or_default();
    let selected = styles.paragraph_id(direct.as_ref());
    let selection_was_default = direct.as_ref().and_then(|p| child(p, "pStyle")).is_none() && selected.is_some();
    let defaults = defaults_node(styles_node);
    let mut chain = Vec::new();
    let mut parent: Option<String> = None;
    let mut budget = 0usize;
    let mut retained_bytes = 0usize;
    // The normalized key excludes comments and unregistered mark rPr. Charge
    // their retained source containers too, before precollection can clone many
    // such descriptions. Run descriptors retain bounded direct properties and
    // digests, never another complete formula or style catalogue.
    for node in direct.iter().chain(defaults.iter()) { charge(&mut retained_bytes, tree_budget(node)?)?; }
    if let Some(id) = selected { for style in styles.chain(id, "paragraph")? {
        if chain.len() >= MAX_CHAIN { return Err("math_budget"); }
        let id = attr(style, "styleId").ok_or("math_invalid_property")?.to_string();
        let ppr = child(style, "pPr").cloned();
        if let Some(node) = &ppr { charge(&mut retained_bytes, tree_budget(node)?)?; }
        let rpr = native_runs::four_properties(child(style, "rPr"))?;
        if let Some(node) = &rpr { charge(&mut retained_bytes, tree_budget(node)?)?; }
        // Parent is embedded JSON, not a quoted recursive string: depth must
        // grow linearly, never exponentially through repeated backslash escapes.
        let key = format!("{{\"id\":{},\"type\":\"paragraph\",\"ppr\":{},\"rpr\":{},\"parent\":{}}}", json(&id)?, json(&ppr_xml(ppr.as_ref())?)?, json(&ppr_xml(rpr.as_ref())?)?, parent.as_deref().unwrap_or("null"));
        charge(&mut budget, key.len())?;
        chain.push(Definition { source_id: id, kind: "paragraph", parent: parent.clone(), ppr, rpr, key: key.clone() });
        parent = Some(key);
    }}
    let (runs, characters) = native_runs::capture_runs(root, &styles, direct.as_ref(), &mut retained_bytes)?;
    if direct.is_none() && chain.is_empty() && defaults.is_none() && !runs.associated { return Ok(None); }
    let paragraph_mark = context.layers.iter().any(|layer| matches!(&layer.declaration,
        super::paragraph::WordParagraphDeclaration::Explicit(properties) if properties.paragraph_mark_properties));
    let key = format!("{{\"direct\":{},\"defaults\":{},\"chain\":{},\"defaultSelection\":{},\"mark\":{},\"source\":{}}}",
        json(&ppr_xml(direct.as_ref())?)?, json(&ppr_xml(defaults.as_ref())?)?, parent.as_deref().unwrap_or("null"), selection_was_default, paragraph_mark, json(&runs.source_digest)?);
    charge(&mut budget, key.len())?;
    charge(&mut budget, retained_bytes)?;
    Ok(Some(NativeParagraphSource { direct, chain, characters, runs, defaults, selection_was_default, paragraph_mark, key, retained_bytes }))
}

#[derive(Debug)]
pub(crate) struct NativeParagraphRegistry {
    definitions: BTreeMap<String, Definition>,
    ids: BTreeMap<String, String>,
    names: BTreeMap<String, String>,
    prepared: BTreeSet<String>,
    // Each resolve result may still be referenced by a paragraph. A successful
    // run projection replaces exactly one such use, not every use of a shared
    // definition or every earlier resolve of the same source.
    legacy_uses: BTreeMap<String, usize>,
    legacy_refs: BTreeMap<String, usize>,
    run_ids: BTreeMap<String, String>,
    run_names: BTreeMap<String, String>,
    run_used: BTreeSet<String>,
    template: WordMathStyles,
    template_run_context: native_runs::GlobalRunContext,
    resolved_hosts: BTreeMap<String, String>,
    template_defaults: NativeParagraphDefaultsKind,
}

impl NativeParagraphRegistry {
    /// Prepare all definitions before allocating aliases. No input-order wins,
    /// no trusted prefix, and every real template ID/name is reserved.
    pub(crate) fn prepare(template_styles: &str, sources: &[NativeParagraphSource]) -> Result<Self> {
        if sources.len() > MAX_SOURCES || template_styles.len() > MAX_BYTES { return Err("math_budget"); }
        let parsed = crate::math_document::parse_xml(template_styles)?;
        let roots = parsed.children().collect::<Vec<_>>();
        if roots.len() != 1 || !word(roots[0], "styles") { return Err("math_invalid_property"); }
        let template = WordMathStyles::from_node(roots[0])?;
        if template.styles.len() > MAX_DEFINITIONS { return Err("math_budget"); }
        tree_budget(&parsed)?;
        // Validate the defaults' actual namespace/particle shape without asking
        // the limited renderer to consume the template's ordinary style pPr.
        super::word_paragraph_context(&parsed)?;
        let template_defaults = defaults_kind(defaults_node(Some(roots[0])).as_ref());
        let template_run_context = native_runs::global_context(&template)?;
        let mut reserved = template.styles.keys().map(|id| id.to_lowercase()).collect::<BTreeSet<_>>();
        let mut reserved_names = template.styles.values().filter_map(|s| child(s, "name").and_then(|n| attr(n, "val")))
            .map(str::to_lowercase).collect::<BTreeSet<_>>();
        let mut definitions = BTreeMap::new();
        let mut families: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut prepared = BTreeSet::new();
        let mut needed_run_views = BTreeSet::new();
        let mut bytes = 0usize;
        for source in sources {
            charge(&mut bytes, source.description_bytes())?;
            prepared.insert(source.key.clone());
            if source.runs.associated { needed_run_views.extend(source.chain.iter().map(|definition| definition.key.clone())); }
            for definition in source.chain.iter().chain(&source.characters) {
                families.entry(definition.source_id.to_lowercase()).or_default().insert(definition.key.clone());
                if !definitions.contains_key(&definition.key) {
                    if definitions.len() >= MAX_DEFINITIONS { return Err("math_budget"); }
                    definitions.insert(definition.key.clone(), definition.clone());
                }
            }
        }
        let template_ids = reserved.clone();
        reserved.extend(definitions.values().map(|definition| definition.source_id.to_lowercase()));
        let mut ids = BTreeMap::new();
        for (key, definition) in &definitions {
            if definition.parent.as_ref().is_some_and(|parent| !definitions.contains_key(parent)) { return Err("math_invalid_property"); }
            let original = &definition.source_id;
            let id = if !template_ids.contains(&original.to_lowercase()) && families[&original.to_lowercase()].len() == 1 {
                original.clone()
            } else {
                let digest = format!("{:x}", Sha256::digest(key.as_bytes()));
                let stem = format!("ReadMDMathP_{digest}");
                let mut id = stem.clone();
                let mut count = 0usize;
                while reserved.contains(&id.to_lowercase()) {
                    count += 1;
                    if count > MAX_DEFINITIONS { return Err("math_budget"); }
                    id = format!("{stem}_{count}");
                }
                reserved.insert(id.to_lowercase()); id
            };
            ids.insert(key.clone(), id);
        }
        let mut names = BTreeMap::new();
        for (key, id) in &ids {
            let mut name = id.clone();
            let mut count = 0usize;
            while reserved_names.contains(&name.to_lowercase()) {
                count += 1;
                if count > MAX_DEFINITIONS { return Err("math_budget"); }
                name = format!("{id} ({count})");
            }
            reserved_names.insert(name.to_lowercase()); names.insert(key.clone(), name);
        }
        let mut run_ids = BTreeMap::new();
        let mut run_names = BTreeMap::new();
        for (key, definition) in &definitions {
            if definition.kind != "paragraph" || !needed_run_views.contains(key) { continue; }
            let digest = format!("{:x}", Sha256::digest(format!("native-run-view-v1:{key}").as_bytes()));
            let stem = format!("ReadMDMathR_{digest}");
            let mut id = stem.clone(); let mut count = 0usize;
            while reserved.contains(&id.to_lowercase()) { count += 1; if count > MAX_DEFINITIONS { return Err("math_budget"); } id = format!("{stem}_{count}"); }
            reserved.insert(id.to_lowercase());
            let mut name = id.clone(); let mut count = 0usize;
            while reserved_names.contains(&name.to_lowercase()) { count += 1; if count > MAX_DEFINITIONS { return Err("math_budget"); } name = format!("{id} ({count})"); }
            reserved_names.insert(name.to_lowercase());
            run_ids.insert(key.clone(), id); run_names.insert(key.clone(), name);
        }
        if template.styles.len().checked_add(definitions.len()).and_then(|count| count.checked_add(run_ids.len())).is_none_or(|count| count > MAX_DEFINITIONS) {
            return Err("math_budget");
        }
        Ok(Self { definitions, ids, names, prepared, legacy_uses: BTreeMap::new(), legacy_refs: BTreeMap::new(), run_ids, run_names,
            run_used: BTreeSet::new(), template, template_run_context, resolved_hosts: BTreeMap::new(), template_defaults })
    }

    fn retain_legacy_use(&mut self, source: &NativeParagraphSource) -> Result<()> {
        let uses = self.legacy_uses.get(&source.key).copied().unwrap_or(0).checked_add(1).ok_or("math_budget")?;
        // Check every increment before publishing any part of this use.
        for definition in &source.chain {
            self.legacy_refs.get(&definition.key).copied().unwrap_or(0).checked_add(1).ok_or("math_budget")?;
        }
        self.legacy_uses.insert(source.key.clone(), uses);
        for definition in &source.chain { *self.legacy_refs.entry(definition.key.clone()).or_default() += 1; }
        Ok(())
    }

    fn release_legacy_use(&mut self, source: &NativeParagraphSource) -> Result<()> {
        let Some(uses) = self.legacy_uses.get(&source.key).copied() else { return Ok(()); };
        if uses == 0 || source.chain.iter().any(|definition| self.legacy_refs.get(&definition.key).is_none_or(|refs| *refs == 0)) {
            return Err("math_invalid_source");
        }
        // The exact source owns one reference to every definition in its chain.
        // Other sources and earlier standalone resolve calls retain theirs.
        for definition in &source.chain {
            let refs = self.legacy_refs.get_mut(&definition.key).expect("checked legacy reference");
            *refs -= 1;
            if *refs == 0 { self.legacy_refs.remove(&definition.key); }
        }
        if uses == 1 { self.legacy_uses.remove(&source.key); }
        else { self.legacy_uses.insert(source.key.clone(), uses - 1); }
        Ok(())
    }

    pub(crate) fn resolve(&mut self, source: &NativeParagraphSource) -> Result<NativeParagraphResolution> {
        if !self.prepared.contains(&source.key) { return Err("math_invalid_source"); }
        let loss = |code| NativeParagraphResolution::Materialized(NativeParagraphLoss {
            code, source_defaults: source.defaults_kind(), template_defaults: self.template_defaults,
            selected_style_id: source.chain.last().map(|style| style.source_id.clone()),
        });
        // Nonempty globals need a separate whole-package compatibility proof;
        // this registry neither changes defaults nor manufactures that proof.
        if source.defaults_kind() == NativeParagraphDefaultsKind::Explicit || self.template_defaults == NativeParagraphDefaultsKind::Explicit {
            return Ok(loss("incompatible-paragraph-doc-defaults"));
        }
        let Some(selected) = source.chain.last() else { return Ok(NativeParagraphResolution::NotApplicable); };
        if source.paragraph_mark { return Ok(loss("paragraph-mark-properties-not-registered")); }
        let id = self.ids.get(&selected.key).ok_or("math_invalid_source")?.clone();
        let mut ppr_xml = format!("<w:pStyle xmlns:w=\"{WORD_NS}\" w:val=\"{}\"/>", attribute(&id));
        if let Some(pr) = &source.direct { for node in pr.children().filter(|node| !word(node, "pStyle")) {
            ppr_xml.push_str(&emit(node)?);
        }}
        let mut aliases = Vec::new();
        for definition in &source.chain {
            let native_id = self.ids.get(&definition.key).ok_or("math_invalid_source")?;
            if native_id != &definition.source_id { aliases.push(NativeParagraphAlias { source_id: definition.source_id.clone(), native_id: native_id.clone() }); }
        }
        self.retain_legacy_use(source)?;
        self.resolved_hosts.insert(source.key.clone(), id.clone());
        Ok(NativeParagraphResolution::Native(NativeParagraphProjection {
            ppr_xml, selected_style_id: id, aliases, selection_was_default: source.selection_was_default,
        }))
    }

    /// Legacy definitions with outstanding resolve uses and proven run views.
    /// Replaced legacy hosts do not publish otherwise unused definitions.
    pub(crate) fn style_xml(&self) -> String {
        let mut ordered = self.legacy_refs.keys().map(|key| (&self.ids[key], key)).collect::<Vec<_>>();
        ordered.sort();
        let mut out = String::new();
        for (id, key) in ordered {
            let definition = &self.definitions[key];
            out.push_str(&format!("<w:style xmlns:w=\"{WORD_NS}\" w:type=\"paragraph\" w:styleId=\"{}\"><w:name w:val=\"{}\"/>", attribute(id), attribute(&self.names[key])));
            if let Some(parent) = &definition.parent { out.push_str(&format!("<w:basedOn w:val=\"{}\"/>", attribute(&self.ids[parent]))); }
            if let Some(ppr) = &definition.ppr { out.push_str(&emit(ppr).expect("capture validated registered pPr")); }
            out.push_str("</w:style>");
        }
        let mut ordered = self.run_used.iter().map(|key| {
            let definition = &self.definitions[key];
            let id = if definition.kind == "paragraph" { &self.run_ids[key] } else { &self.ids[key] };
            (id, key)
        }).collect::<Vec<_>>();
        ordered.sort();
        for (id, key) in ordered {
            let definition = &self.definitions[key];
            let name = if definition.kind == "paragraph" { &self.run_names[key] } else { &self.names[key] };
            out.push_str(&format!("<w:style xmlns:w=\"{WORD_NS}\" w:type=\"{}\" w:styleId=\"{}\"><w:name w:val=\"{}\"/>", definition.kind, attribute(id), attribute(name)));
            if let Some(parent) = &definition.parent {
                let parent_id = if definition.kind == "paragraph" { &self.run_ids[parent] } else { &self.ids[parent] };
                out.push_str(&format!("<w:basedOn w:val=\"{}\"/>", attribute(parent_id)));
            }
            if let Some(ppr) = &definition.ppr { out.push_str(&emit(ppr).expect("validated run view paragraph properties")); }
            if let Some(rpr) = &definition.rpr { out.push_str(&emit(rpr).expect("validated registered run properties")); }
            out.push_str("</w:style>");
        }
        out
    }
}

#[cfg(test)]
#[path = "native_paragraph_tests.rs"]
mod tests;
