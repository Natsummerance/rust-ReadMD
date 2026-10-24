//! A native run view is activated only after all four axes of every run have
//! been checked against the real target defaults and host. The legacy pPr-only
//! view remains available; no declaration is invented to force compatibility.
use super::*;
use super::super::{apply, bind_used_namespaces, named, Properties};
use crate::math_document::MATH_NS;

const AXES: [&str; 4] = ["b", "i", "sz", "color"];

#[path = "native_run_context.rs"]
mod context;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GlobalRunState { Missing, Empty, Explicit }

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GlobalRunContext {
    state: GlobalRunState,
    signature: Option<String>,
    theme_dependent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AutomaticCharacterContext {
    selected_id: String,
    signature: String,
    theme_dependent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Axis { Missing, Toggle(bool), Size(u32), Color([u8; 3]), Unsupported(String) }
type Four = [Axis; 4];

#[derive(Debug, Clone, PartialEq)]
struct RunSource {
    role: String,
    fingerprint: String,
    direct: Option<XmlNode>,
    character: Option<String>,
    automatic: bool,
    effective: Four,
    effective_properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq)]
struct NormalizationOwner {
    kind: &'static str,
    ordinal: usize,
    first_run: usize,
    end_run: usize,
    declaration: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct RunSources {
    pub(super) associated: bool,
    pub(super) source_digest: String,
    structure_digest: String,
    owners: Vec<NormalizationOwner>,
    runs: Vec<RunSource>,
    blockers: BTreeSet<&'static str>,
    losses: BTreeSet<&'static str>,
    global: GlobalRunContext,
    automatic: Option<AutomaticCharacterContext>,
}

pub(super) fn global_context(styles: &WordMathStyles) -> Result<GlobalRunContext> { context::global_context(styles) }

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeRunProjection {
    pub content: String,
    pub native_runs: usize,
    pub materialized_runs: usize,
    pub loss_reasons: Vec<&'static str>,
    /// Present only for a proven run-enabled paragraph view or known template
    /// host. ppr_xml replaces the legacy pPr contents only when supplied.
    pub host_style_id: Option<String>,
    pub ppr_xml: Option<String>,
}

fn checked_single<'a>(node: &'a XmlNode, local: &str) -> Result<Option<&'a XmlNode>> {
    let mut matches = node.children().filter(|node| word(node, local));
    let value = matches.next();
    if matches.next().is_some() { return Err("math_invalid_property"); }
    Ok(value)
}

fn scalar_shape(node: &XmlNode) -> bool {
    node.children().next().is_none() && node.content.iter().all(|part|
        matches!(part, XmlPart::Text(value) if value.trim().is_empty()))
        && node.attributes.keys().filter(|key| key.as_str() != "xmlns" && !key.starts_with("xmlns:")).all(|key|
            node.attribute_namespaces.get(key).is_some_and(|uri| uri == WORD_NS)
                && key.rsplit(':').next() == Some("val"))
}

fn axis(node: Option<&XmlNode>, index: usize) -> Result<Axis> {
    let Some(node) = node else { return Ok(Axis::Missing); };
    if !scalar_shape(node) { return Ok(Axis::Unsupported(digest(node, false)?)); }
    Ok(match index {
        0 | 1 => match attr(node, "val").unwrap_or("true") {
            "1" | "true" | "on" => Axis::Toggle(true),
            "0" | "false" | "off" => Axis::Toggle(false),
            _ => return Err("math_invalid_property"),
        },
        2 => match attr(node, "val").and_then(|value| value.parse::<u32>().ok()).filter(|value| (1..=3276).contains(value)) {
            Some(value) => Axis::Size(value), _ => Axis::Unsupported(digest(node, false)?),
        },
        _ => match attr(node, "val").filter(|value| value.len() == 6 && value.bytes().all(|c| c.is_ascii_hexdigit())) {
            Some(value) => Axis::Color([u8::from_str_radix(&value[..2], 16).unwrap(), u8::from_str_radix(&value[2..4], 16).unwrap(), u8::from_str_radix(&value[4..], 16).unwrap()]),
            None => Axis::Unsupported(digest(node, false)?),
        },
    })
}

fn four(properties: &Properties) -> Result<Four> {
    Ok([axis(properties.get("b"), 0)?, axis(properties.get("i"), 1)?,
        axis(properties.get("sz"), 2)?, axis(properties.get("color"), 3)?])
}

/// Retain the actual four-axis declarations, including non-admitted values in
/// the source descriptor/key. Such values are never emitted as a native view.
pub(super) fn four_properties(pr: Option<&XmlNode>) -> Result<Option<XmlNode>> {
    let Some(pr) = pr else { return Ok(None); };
    tree_budget(pr)?;
    let mut result = named("rPr");
    let mut seen = BTreeSet::new();
    for node in pr.children().filter(|node| node.namespace == WORD_NS && AXES.contains(&node.local())) {
        if !seen.insert(node.local()) { return Err("math_invalid_property"); }
        let mut node = node.clone(); bind_used_namespaces(&mut node);
        result.content.push(XmlPart::Element(node));
    }
    // CT_RPr has a fixed order; raw source itself remains untouched.
    result.content.sort_by_key(|part| match part { XmlPart::Element(node) => match node.local() { "b" => 0, "i" => 1, "color" => 2, _ => 3 }, _ => 4 });
    Ok(Some(result))
}

fn apply_four(properties: &mut Properties, pr: Option<&XmlNode>, toggle: bool) -> Result<()> {
    let filtered = four_properties(pr)?;
    apply(properties, filtered.as_ref(), toggle)
}

fn note_properties(pr: Option<&XmlNode>, blockers: &mut BTreeSet<&'static str>, losses: &mut BTreeSet<&'static str>) -> Result<()> {
    let Some(pr) = pr else { return Ok(()); };
    let mut seen = BTreeSet::new();
    for node in pr.children() {
        if node.namespace != WORD_NS || !AXES.contains(&node.local()) {
            if !word(node, "rStyle") { losses.insert("other-run-properties-materialized"); }
            continue;
        }
        if !seen.insert(node.local()) { return Err("math_invalid_property"); }
        let index = AXES.iter().position(|key| *key == node.local()).unwrap();
        if matches!(axis(Some(node), index)?, Axis::Unsupported(_)) {
            blockers.insert(match node.local() { "sz" => "unsupported-native-run-size", "color" => "nonliteral-native-run-color", _ => "unsupported-native-run-toggle" });
        }
    }
    Ok(())
}

fn declares_four(pr: Option<&XmlNode>) -> bool {
    pr.is_some_and(|pr| pr.children().any(|node| node.namespace == WORD_NS && AXES.contains(&node.local())))
}

// Digests are namespace-aware and stream text; they exclude internal node IDs
// and redundant namespace spelling. Direct Word rPr is the only mutable slot.
fn digest(root: &XmlNode, omit_direct_rpr: bool) -> Result<String> {
    fn field(hash: &mut Sha256, value: &str) { hash.update((value.len() as u64).to_le_bytes()); hash.update(value.as_bytes()); }
    fn walk(node: &XmlNode, hash: &mut Sha256, omit: bool) -> Result<()> {
        if crate::speech::cancelled() { return Err("cancelled"); }
        field(hash, "element"); field(hash, &node.namespace); field(hash, node.local());
        let mut attributes = BTreeMap::new();
        for (key, value) in &node.attributes {
            if key == "xmlns" || key.starts_with("xmlns:") { continue; }
            let uri = node.attribute_namespaces.get(key).map(String::as_str).unwrap_or("");
            if attributes.insert((uri, key.rsplit(':').next().unwrap_or(key)), value).is_some() { return Err("math_invalid_property"); }
        }
        for ((uri, local), value) in attributes { field(hash, "attribute"); field(hash, uri); field(hash, local); field(hash, value); }
        field(hash, "/attributes");
        for part in &node.content { match part {
            XmlPart::Element(child) if omit && word(child, "rPr") => (),
            XmlPart::Element(child) => walk(child, hash, false)?,
            XmlPart::Text(value) => { field(hash, "text"); field(hash, value); },
            XmlPart::Comment(value) => { field(hash, "comment"); field(hash, value); },
            XmlPart::Instruction(value) => { field(hash, "instruction"); field(hash, value); },
        }}
        field(hash, "/element"); Ok(())
    }
    let mut hash = Sha256::new(); walk(root, &mut hash, omit_direct_rpr)?; Ok(format!("{:x}", hash.finalize()))
}

// DOCX may group root equations into oMathPara and insert only paragraph jc /
// operator limLoc. Bind everything else, including argument-container roles,
// so identical repeated text cannot make a swapped or dropped object match.
fn structure_digest(root: &XmlNode) -> Result<String> {
    fn field(hash: &mut Sha256, value: &str) { hash.update((value.len() as u64).to_le_bytes()); hash.update(value.as_bytes()); }
    fn walk(node: &XmlNode, parent: &str, hash: &mut Sha256) -> Result<()> {
        if crate::speech::cancelled() { return Err("cancelled"); }
        if super::super::is_context(node) || word(node, "rPr")
            || node.namespace == MATH_NS && (node.local() == "mathPr"
                || node.local() == "jc" && parent == "oMathParaPr"
                || node.local() == "limLoc" && parent == "naryPr") { return Ok(()); }
        let transparent = node.name == "#root" || node.namespace == MATH_NS && matches!(node.local(), "oMathPara" | "oMathParaPr" | "naryPr");
        let mut attrs = BTreeMap::new();
        for (key, value) in &node.attributes {
            if key == "xmlns" || key.starts_with("xmlns:") { continue; }
            let uri = node.attribute_namespaces.get(key).map(String::as_str).unwrap_or("");
            if attrs.insert((uri, key.rsplit(':').next().unwrap_or(key)), value).is_some() { return Err("math_invalid_property"); }
        }
        if !transparent || !attrs.is_empty() {
            field(hash, if transparent { "wrapper-attributes" } else { "element" }); field(hash, &node.namespace); field(hash, node.local());
            for ((uri, local), value) in attrs { field(hash, uri); field(hash, local); field(hash, value); } field(hash, "/attributes");
        }
        for part in &node.content { match part {
            XmlPart::Element(child) => walk(child, node.local(), hash)?,
            XmlPart::Text(text) if !text.trim().is_empty() || node.namespace == MATH_NS && node.local() == "t" => { field(hash, "text"); field(hash, text); },
            XmlPart::Comment(text) => { field(hash, "comment"); field(hash, text); },
            XmlPart::Instruction(text) => { field(hash, "instruction"); field(hash, text); },
            _ => (),
        }}
        if !transparent { field(hash, "/element"); } Ok(())
    }
    let mut hash = Sha256::new(); walk(root, "", &mut hash)?; Ok(format!("{:x}", hash.finalize()))
}

fn normalization_owners(root: &XmlNode, source: bool) -> Result<Vec<NormalizationOwner>> {
    fn math_child<'a>(node: &'a XmlNode, local: &str) -> Result<Option<&'a XmlNode>> {
        let mut values = node.children().filter(|node| node.namespace == MATH_NS && node.local() == local);
        let first = values.next(); if values.next().is_some() { return Err("math_invalid_property"); } Ok(first)
    }
    fn walk(node: &XmlNode, source: bool, runs: &mut usize, nary: &mut usize, out: &mut Vec<NormalizationOwner>) -> Result<()> {
        if crate::speech::cancelled() { return Err("cancelled"); }
        if super::super::is_context(node) || word(node, "rPr") || node.namespace == MATH_NS && node.local() == "mathPr" { return Ok(()); }
        let first_run = *runs;
        if node.namespace == MATH_NS && matches!(node.local(), "r" | "ctrlPr") { *runs += 1; }
        let owner = if node.namespace == MATH_NS && node.local() == "nary" {
            let ordinal = *nary; *nary += 1;
            Some(("nary", ordinal, math_child(node, "naryPr")?.map(|pr| math_child(pr, "limLoc")).transpose()?.flatten()))
        } else if node.namespace == MATH_NS && node.local() == "oMathPara" {
            Some(("paragraph", 0, math_child(node, "oMathParaPr")?.map(|pr| math_child(pr, "jc")).transpose()?.flatten()))
        } else { None };
        for child in node.children() { walk(child, source, runs, nary, out)?; }
        if let Some((kind, ordinal, declaration)) = owner {
            let declaration = declaration.map(|declaration| {
                // The old normalizer makes an explicit empty paragraph jc's
                // documented centerGroup default native; other existing values
                // (including an empty limLoc) must remain exactly unchanged.
                if source && kind == "paragraph" && !declaration.attributes.keys().any(|key| key.rsplit(':').next() == Some("val") && declaration.attribute_namespaces.get(key).is_some_and(|uri| uri == MATH_NS)) {
                    let mut value = declaration.clone();
                    value.attributes.insert("xmlns:readmdRunCheck".into(), MATH_NS.into()); value.attributes.insert("readmdRunCheck:val".into(), "centerGroup".into());
                    value.attribute_namespaces.insert("readmdRunCheck:val".into(), MATH_NS.into()); digest(&value, false)
                } else { digest(declaration, false) }
            }).transpose()?;
            out.push(NormalizationOwner { kind, ordinal, first_run, end_run: *runs, declaration });
        }
        Ok(())
    }
    let mut out = Vec::new(); walk(root, source, &mut 0, &mut 0, &mut out)?; Ok(out)
}

fn visit_runs(root: &XmlNode, visit: &mut impl FnMut(&XmlNode) -> Result<()>) -> Result<()> {
    if crate::speech::cancelled() { return Err("cancelled"); }
    if super::super::is_context(root) || word(root, "rPr") { return Ok(()); }
    if root.namespace == MATH_NS && matches!(root.local(), "r" | "ctrlPr") { visit(root)?; }
    for child in root.children() { visit_runs(child, visit)?; }
    Ok(())
}

pub(super) fn capture_runs(root: &XmlNode, styles: &WordMathStyles, ppr: Option<&XmlNode>, retained: &mut usize) -> Result<(RunSources, Vec<Definition>)> {
    let mut blockers = BTreeSet::new(); let mut losses = BTreeSet::new();
    let defaults = styles.root.as_ref().and_then(|s| child(s, "docDefaults")).and_then(|d| child(d, "rPrDefault")).and_then(|d| child(d, "rPr"));
    let global = global_context(styles)?;
    charge(retained, 192 + global.signature.as_ref().map_or(0, String::len))?;
    note_properties(defaults, &mut blockers, &mut losses)?;
    let mut associated = declares_four(defaults) || global.state == GlobalRunState::Explicit;
    let mut work = 0usize;
    let mut base = Properties::new(); context::apply_properties(&mut base, defaults, false, &mut work)?;
    if let Some(id) = styles.paragraph_id(ppr) { for style in styles.chain(id, "paragraph")? {
        let pr = checked_single(style, "rPr")?;
        associated |= declares_four(pr);
        note_properties(pr, &mut blockers, &mut losses)?; context::apply_properties(&mut base, pr, true, &mut work)?;
    }}
    let theme = crate::math_theme::ThemeColors::from_context(root)?;
    let fonts = crate::math_font_theme::ThemeFonts::from_context(root)?;
    let mut runs = Vec::new(); let mut definitions = BTreeMap::new(); let mut automatic = None;
    visit_runs(root, &mut |node| {
        if runs.len() >= 12000 { return Err("math_budget"); }
        let direct = checked_single(node, "rPr")?;
        if let Some(pr) = direct { charge(retained, tree_budget(pr)?)?; }
        associated |= declares_four(direct);
        note_properties(direct, &mut blockers, &mut losses)?;
        let reference = direct.map(|pr| checked_single(pr, "rStyle")).transpose()?.flatten();
        if let Some(reference) = reference {
            if !scalar_shape(reference) || attr(reference, "val").is_none_or(str::is_empty) { return Err("math_invalid_property"); }
        }
        let explicit = reference.and_then(|reference| attr(reference, "val"));
        associated |= explicit.is_some();
        let character = explicit.or(styles.character_default.as_deref());
        let defaulted = explicit.is_none() && character.is_some();
        if defaulted {
            associated = true;
            if automatic.is_none() {
                automatic = context::automatic_context(styles)?;
                charge(retained, automatic.as_ref().map_or(0, |value| value.selected_id.len() + value.signature.len() + 128))?;
            }
        }
        let mut effective = context::clone_properties(&base, &mut work)?;
        let mut parent: Option<String> = None;
        if let Some(id) = character { for style in styles.chain(id, "character")? {
            let pr = checked_single(style, "rPr")?;
            associated |= declares_four(pr);
            note_properties(pr, &mut blockers, &mut losses)?; context::apply_properties(&mut effective, pr, true, &mut work)?;
            if explicit.is_some() {
                let rpr = four_properties(pr)?;
                let id = attr(style, "styleId").ok_or("math_invalid_property")?.to_string();
                let key = format!("{{\"id\":{},\"type\":\"character\",\"rpr\":{},\"parent\":{}}}", json(&id)?, json(&ppr_xml(rpr.as_ref())?)?, parent.as_deref().unwrap_or("null"));
                if !definitions.contains_key(&key) {
                    charge(retained, key.len())?;
                    if let Some(pr) = &rpr { charge(retained, tree_budget(pr)?)?; }
                    definitions.insert(key.clone(), Definition { source_id: id, kind: "character", parent: parent.clone(), ppr: None, rpr, key: key.clone() });
                }
                parent = Some(key);
            }
        }}
        context::apply_properties(&mut effective, direct, false, &mut work)?;
        if let Some(color) = effective.get_mut("color") { theme.resolve_run_color(color)?; }
        if let Some(rfonts) = effective.get_mut("rFonts") { fonts.resolve(rfonts)?; }
        let effective_properties = context::properties_signature(&effective)?;
        charge(retained, effective_properties.iter().map(|(key, value)| key.len() + value.len() + 64).sum())?;
        let mut direct = direct.cloned(); if let Some(pr) = &mut direct { bind_used_namespaces(pr); }
        let fingerprint = digest(node, true)?;
        charge(retained, fingerprint.len() + 256)?;
        runs.push(RunSource { role: node.local().into(), fingerprint, direct, character: parent, automatic: defaulted, effective: four(&effective)?, effective_properties });
        Ok(())
    })?;
    let source_digest = digest(root, false)?;
    let structure_digest = structure_digest(root)?;
    let owners = normalization_owners(root, true)?;
    charge(retained, owners.len().checked_mul(128).ok_or("math_budget")?)?;
    charge(retained, source_digest.len() + structure_digest.len())?;
    Ok((RunSources { associated, source_digest, structure_digest, owners, runs, blockers, losses, global, automatic }, definitions.into_values().collect()))
}

fn actual_four(node: &XmlNode) -> Result<Four> {
    let mut properties = Properties::new(); apply_four(&mut properties, checked_single(node, "rPr")?, false)?; four(&properties)
}

fn actual_properties(node: &XmlNode) -> Result<BTreeMap<String, String>> {
    let mut properties = Properties::new(); apply(&mut properties, checked_single(node, "rPr")?, false)?;
    context::properties_signature(&properties)
}

fn direct_projection(run: &RunSource, materialized: Option<&XmlNode>, character_id: Option<&str>, inherited: Option<&Properties>, work: &mut usize) -> Result<Option<XmlNode>> {
    let mut pr = materialized.cloned().unwrap_or_else(|| named("rPr"));
    pr.content.retain(|part| !matches!(part, XmlPart::Element(node) if node.namespace == WORD_NS && (AXES.contains(&node.local()) || node.local() == "rStyle")));
    let mut properties = Properties::new();
    if let Some(original) = &run.direct { for node in original.children().filter(|node| node.namespace == WORD_NS && AXES.contains(&node.local())) {
        properties.insert(node.local().into(), node.clone());
    }}
    for node in properties.into_values() { pr.content.push(XmlPart::Element(node)); }
    if let Some(id) = character_id {
        let mut reference = named("rStyle"); reference.attributes.insert("w:val".into(), id.into()); reference.attribute_namespaces.insert("w:val".into(), WORD_NS.into());
        pr.content.insert(0, XmlPart::Element(reference));
    }
    if let Some(inherited) = inherited {
        let mut proposed = context::clone_properties(inherited, work)?;
        context::apply_properties(&mut proposed, run.direct.as_ref(), false, work)?;
        let signatures = context::properties_signature(&proposed)?;
        let mut original = BTreeMap::new();
        if let Some(direct) = &run.direct {
            for node in direct.children().filter(|node| node.namespace == WORD_NS && !AXES.contains(&node.local()) && node.local() != "rStyle") {
                if original.insert(node.local().to_string(), node).is_some() { return Err("math_invalid_property"); }
            }
        }
        // Restore the original property (including partial rFonts/lang) only
        // when the real inherited cascade closes exactly. An authored direct
        // node is never guessed to be inherited merely because values match.
        let mut restored = BTreeSet::new();
        pr.content.retain(|part| match part {
            XmlPart::Element(node) if node.namespace == WORD_NS && !AXES.contains(&node.local()) && node.local() != "rStyle"
                && signatures.get(node.local()) == run.effective_properties.get(node.local()) => { restored.insert(node.local().to_string()); false },
            _ => true,
        });
        for key in restored { if let Some(node) = original.get(&key) { pr.content.push(XmlPart::Element((*node).clone())); } }
    }
    const ORDER: &[&str] = &["rStyle","rFonts","b","bCs","i","iCs","caps","smallCaps","strike","dstrike","outline","shadow","emboss","imprint","noProof","snapToGrid","vanish","webHidden","color","spacing","w","kern","position","sz","szCs","highlight","u","effect","bdr","shd","fitText","vertAlign","rtl","cs","em","lang","eastAsianLayout","specVanish","oMath","rPrChange"];
    pr.content.sort_by_key(|part| match part { XmlPart::Element(node) if node.namespace == WORD_NS => ORDER.iter().position(|key| *key == node.local()).unwrap_or(ORDER.len()), _ => ORDER.len() + 1 });
    if run.direct.is_none() && pr.content.is_empty() { Ok(None) } else { Ok(Some(pr)) }
}

fn replace_pr(node: &mut XmlNode, pr: Option<XmlNode>) {
    let original = node.content.iter().position(|part| matches!(part, XmlPart::Element(child) if word(child, "rPr")));
    node.content.retain(|part| !matches!(part, XmlPart::Element(child) if word(child, "rPr")));
    if let Some(pr) = pr {
        let at = original.unwrap_or_else(|| node.content.iter().position(|part| matches!(part, XmlPart::Element(child) if child.namespace == MATH_NS && matches!(child.local(), "t" | "break"))).unwrap_or(node.content.len()));
        node.content.insert(at, XmlPart::Element(pr));
    }
}

fn emit_content(root: &XmlNode) -> Result<String> {
    fn push(out: &mut String, text: &str) -> Result<()> { if out.len().checked_add(text.len()).is_none_or(|n| n > MAX_BYTES) { return Err("math_budget"); } out.push_str(text); Ok(()) }
    fn emit(node: &XmlNode, out: &mut String) -> Result<()> {
        if crate::speech::cancelled() { return Err("cancelled"); }
        if node.name != "#root" {
            push(out, "<")?; push(out, &node.name)?;
            for (key, value) in &node.attributes { push(out, " ")?; push(out, key)?; push(out, "=\"")?; push(out, &attribute(value))?; push(out, "\"")?; }
            if node.content.is_empty() { push(out, "/>")?; return Ok(()); } push(out, ">")?;
        }
        for part in &node.content { match part {
            XmlPart::Element(child) => emit(child, out)?,
            XmlPart::Text(text) => push(out, &text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('\r', "&#13;"))?,
            XmlPart::Comment(text) => { push(out, "<!--")?; push(out, text)?; push(out, "-->")?; },
            XmlPart::Instruction(text) => { push(out, "<?")?; push(out, text)?; push(out, "?>")?; },
        }}
        if node.name != "#root" { push(out, "</")?; push(out, &node.name)?; push(out, ">")?; } Ok(())
    }
    let mut out = String::new(); emit(root, &mut out)?; Ok(out)
}

impl NativeParagraphRegistry {
    fn inherited_run_properties(&self, base: &Properties, run: &RunSource, full: bool, work: &mut usize) -> Result<Properties> {
        let mut inherited = if full { context::clone_properties(base, work)? } else { base.clone() };
        let mut chain = Vec::new(); let mut key = run.character.as_deref();
        while let Some(current) = key {
            if chain.len() >= MAX_CHAIN { return Err("math_budget"); }
            let definition = self.definitions.get(current).ok_or("math_invalid_source")?;
            if definition.kind != "character" { return Err("math_invalid_source"); }
            chain.push(definition); key = definition.parent.as_deref();
        }
        for definition in chain.into_iter().rev() {
            if full { context::apply_properties(&mut inherited, definition.rpr.as_ref(), true, work)?; }
            else { apply_four(&mut inherited, definition.rpr.as_ref(), true)?; }
        }
        if run.character.is_none() {
            if let Some(id) = &self.template.character_default {
                for style in self.template.chain(id, "character")? {
                    if full { context::apply_properties(&mut inherited, child(style, "rPr"), true, work)?; }
                    else { apply_four(&mut inherited, child(style, "rPr"), true)?; }
                }
            }
        }
        Ok(inherited)
    }

    pub(crate) fn project_runs(&mut self, source: &NativeParagraphSource, materialized_content: &str, host_style: Option<&str>) -> Result<NativeRunProjection> {
        // Nested consumers deliberately have no registered host. Their sealed
        // source may be absent from the top-level registry; validate its exact
        // materialized binding, then keep the old output without registration.
        if host_style.is_some() && !self.prepared.contains(&source.key) { return Err("math_invalid_source"); }
        if materialized_content.len() > MAX_BYTES { return Err("math_budget"); }
        let mut content = crate::math_document::parse_xml(materialized_content)?;
        tree_budget(&content)?;
        if structure_digest(&content)? != source.runs.structure_digest { return Err("math_invalid_source"); }
        let owners = normalization_owners(&content, false)?;
        for original in source.runs.owners.iter().filter(|owner| owner.declaration.is_some()) {
            let mut matches = owners.iter().filter(|owner| owner.kind == original.kind &&
                if original.kind == "nary" { owner.ordinal == original.ordinal }
                else { owner.first_run == original.first_run && owner.end_run == original.end_run });
            let current = matches.next().ok_or("math_invalid_source")?;
            if matches.next().is_some() || current.declaration != original.declaration { return Err("math_invalid_source"); }
        }
        let mut index = 0usize;
        visit_runs(&content, &mut |node| {
            let expected = source.runs.runs.get(index).ok_or("math_invalid_source")?; index += 1;
            if expected.role != node.local() || expected.fingerprint != digest(node, true)? || expected.effective != actual_four(node)?
                || expected.effective_properties != actual_properties(node)? { return Err("math_invalid_source"); }
            Ok(())
        })?;
        if index != source.runs.runs.len() { return Err("math_invalid_source"); }
        // A paragraph-only source is not a failed native-run request. Keep its
        // established provenance and do not manufacture a new run-loss warning.
        if !source.runs.associated {
            return Ok(NativeRunProjection { content: materialized_content.into(), native_runs: 0,
                materialized_runs: 0, loss_reasons: Vec::new(), host_style_id: None, ppr_xml: None });
        }
        if host_style.is_none() {
            let mut losses = source.runs.losses.clone(); losses.extend(source.runs.blockers.iter().copied());
            if source.runs.global.state == GlobalRunState::Explicit { losses.insert("run-document-defaults-not-registered"); }
            if source.runs.automatic.is_some() { losses.insert("automatic-character-default-not-registered"); }
            losses.insert("unproven-native-run-host");
            return Ok(NativeRunProjection { content: materialized_content.into(), native_runs: 0,
                materialized_runs: source.runs.runs.len(), loss_reasons: losses.into_iter().collect(),
                host_style_id: None, ppr_xml: None });
        }
        let mut losses = source.runs.losses.clone();
        let mut blockers = source.runs.blockers.clone();
        let globals_reused = source.runs.global.state == GlobalRunState::Explicit
            && context::globals_compatible(&source.runs.global, &self.template_run_context);
        if !context::globals_compatible(&source.runs.global, &self.template_run_context) { blockers.insert("run-document-defaults-not-registered"); }
        let automatic_reused = if let Some(automatic) = &source.runs.automatic {
            let target = context::automatic_context(&self.template)?;
            let compatible = context::automatic_compatible(automatic, target.as_ref());
            if !compatible { blockers.insert("automatic-character-default-not-registered"); }
            compatible
        } else { false };
        let full_context = globals_reused || automatic_reused;
        let mut work = 0usize;
        let mut base = Properties::new(); let mut selected = None;
        if let Some(host) = host_style {
            let resolved = self.resolved_hosts.get(&source.key).is_some_and(|id| id == host);
            if resolved {
                let leaf = source.chain.last().ok_or("math_invalid_source")?;
                selected = Some(self.run_ids.get(&leaf.key).ok_or("math_invalid_source")?.clone());
            } else if host != "ReadMDMath" { return Err("math_invalid_source"); }
            let defaults = self.template.root.as_ref().and_then(|s| child(s, "docDefaults")).and_then(|d| child(d, "rPrDefault")).and_then(|d| child(d, "rPr"));
            if full_context { context::apply_properties(&mut base, defaults, false, &mut work)?; }
            else { apply_four(&mut base, defaults, false)?; }
            if resolved {
                for definition in &source.chain {
                    if full_context { context::apply_properties(&mut base, definition.rpr.as_ref(), true, &mut work)?; }
                    else { apply_four(&mut base, definition.rpr.as_ref(), true)?; }
                }
            } else { for style in self.template.chain(host, "paragraph")? {
                if full_context { context::apply_properties(&mut base, child(style, "rPr"), true, &mut work)?; }
                else { apply_four(&mut base, child(style, "rPr"), true)?; }
            } }
        } else { blockers.insert("unproven-native-run-host"); }
        let mut projections = Vec::new();
        let mut used = BTreeSet::new();
        for run in &source.runs.runs {
            let mut effective = self.inherited_run_properties(&base, run, full_context, &mut work)?; let mut key = run.character.as_deref();
            while let Some(current) = key {
                let definition = self.definitions.get(current).ok_or("math_invalid_source")?;
                used.insert(definition.key.clone()); key = definition.parent.as_deref();
            }
            apply_four(&mut effective, run.direct.as_ref(), false)?;
            if four(&effective)? != run.effective { blockers.insert("incompatible-native-run-context"); }
        }
        if source.runs.runs.is_empty() { blockers.insert("no-native-run-sites"); }
        if !blockers.is_empty() {
            losses.extend(blockers);
            return Ok(NativeRunProjection { content: materialized_content.into(), native_runs: 0, materialized_runs: source.runs.runs.len(), loss_reasons: losses.into_iter().collect(), host_style_id: None, ppr_xml: None });
        }
        // Complete role/count/fingerprint and counterfactual checks finished
        // before any rPr is changed or a run style is made visible to the writer.
        let mut index = 0usize;
        visit_runs(&content, &mut |node| {
            let run = &source.runs.runs[index]; index += 1;
            let id = run.character.as_ref().map(|key| self.ids.get(key).map(String::as_str).ok_or("math_invalid_source")).transpose()?;
            let inherited = if globals_reused || run.automatic && automatic_reused { Some(self.inherited_run_properties(&base, run, full_context, &mut work)?) } else { None };
            projections.push(direct_projection(run, checked_single(node, "rPr")?, id, inherited.as_ref(), &mut work)?); Ok(())
        })?;
        fn replace(node: &mut XmlNode, projections: &mut std::vec::IntoIter<Option<XmlNode>>) -> Result<()> {
            if super::super::is_context(node) || word(node, "rPr") { return Ok(()); }
            if node.namespace == MATH_NS && matches!(node.local(), "r" | "ctrlPr") { replace_pr(node, projections.next().ok_or("math_invalid_source")?); }
            for part in &mut node.content { if let XmlPart::Element(child) = part { replace(child, projections)?; } } Ok(())
        }
        let mut projections = projections.into_iter(); replace(&mut content, &mut projections)?;
        if projections.next().is_some() { return Err("math_invalid_source"); }
        // Recompute from the exact sparse rPr that will be serialized, not the
        // proposal alone. Non-four-axis effective direct nodes remain in place.
        let mut index = 0usize; let mut full_context_mismatch = false;
        visit_runs(&content, &mut |node| {
            let run = &source.runs.runs[index]; index += 1;
            let mut effective = self.inherited_run_properties(&base, run, full_context, &mut work)?;
            if full_context { context::apply_properties(&mut effective, checked_single(node, "rPr")?, false, &mut work)?; }
            else { apply_four(&mut effective, checked_single(node, "rPr")?, false)?; }
            if four(&effective)? != run.effective || digest(node, true)? != run.fingerprint { return Err("math_invalid_source"); }
            if full_context && context::properties_signature(&effective)? != run.effective_properties { full_context_mismatch = true; }
            Ok(())
        })?;
        if full_context_mismatch {
            losses.insert("incompatible-native-run-context");
            return Ok(NativeRunProjection { content: materialized_content.into(), native_runs: 0,
                materialized_runs: source.runs.runs.len(), loss_reasons: losses.into_iter().collect(), host_style_id: None, ppr_xml: None });
        }
        let content = emit_content(&content)?;
        let ppr_xml = if let Some(id) = &selected {
            let mut xml = format!("<w:pStyle xmlns:w=\"{WORD_NS}\" w:val=\"{}\"/>", attribute(id));
            if let Some(pr) = &source.direct { for node in pr.children().filter(|node| !word(node, "pStyle")) { xml.push_str(&super::emit(node)?); } }
            Some(xml)
        } else { None };
        // All possible failures precede activation. Neither a fallback nor an
        // unknown host can publish otherwise unused character/run definitions.
        if selected.is_some() {
            self.release_legacy_use(source)?;
            used.extend(source.chain.iter().map(|definition| definition.key.clone()));
        }
        self.run_used.extend(used);
        Ok(NativeRunProjection { content, native_runs: source.runs.runs.len(), materialized_runs: 0,
            loss_reasons: losses.into_iter().collect(), host_style_id: selected.or_else(|| host_style.map(str::to_string)), ppr_xml })
    }
}

#[cfg(test)]
#[path = "native_runs_tests.rs"]
mod tests;
