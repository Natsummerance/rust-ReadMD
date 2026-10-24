//! Proofs for reusing actual package globals. Nothing here emits a default,
//! changes the template, or substitutes a local style for a global declaration.
use super::*;

// Formatting trivia and namespace prefixes do not affect this compatibility
// proof. The independent full source digest still binds all original bytes,
// comments, missing/empty containers and lexical declarations to the author.
fn signature(root: &XmlNode) -> Result<String> {
    fn field(hash: &mut Sha256, value: &str) { hash.update((value.len() as u64).to_le_bytes()); hash.update(value.as_bytes()); }
    fn walk(node: &XmlNode, hash: &mut Sha256) -> Result<()> {
        if crate::speech::cancelled() { return Err("cancelled"); }
        field(hash, &node.namespace); field(hash, node.local());
        let mut attributes = BTreeMap::new();
        for (key, value) in &node.attributes {
            if key == "xmlns" || key.starts_with("xmlns:") { continue; }
            let uri = node.attribute_namespaces.get(key).map(String::as_str).unwrap_or("");
            if attributes.insert((uri, key.rsplit(':').next().unwrap_or(key)), value).is_some() { return Err("math_invalid_property"); }
        }
        for ((uri, name), value) in attributes { field(hash, "attribute"); field(hash, uri); field(hash, name); field(hash, value); }
        field(hash, "/attributes");
        for part in &node.content { match part {
            XmlPart::Element(child) => walk(child, hash)?,
            XmlPart::Text(text) if !text.trim().is_empty() => { field(hash, "text"); field(hash, text); },
            _ => (),
        }}
        field(hash, "/element"); Ok(())
    }
    let mut hash = Sha256::new(); walk(root, &mut hash)?; Ok(format!("{:x}", hash.finalize()))
}

fn theme_dependent(root: &XmlNode) -> bool {
    root.namespace == crate::math_document::DRAWING_NS || root.attributes.keys().any(|key| {
        root.attribute_namespaces.get(key).is_some_and(|uri| uri == WORD_NS) && {
            let local = key.rsplit(':').next().unwrap_or(key);
            local.to_ascii_lowercase().contains("theme")
        }
    }) || root.children().any(theme_dependent)
}

pub(super) fn global_context(styles: &WordMathStyles) -> Result<GlobalRunContext> {
    let wrapper = styles.root.as_ref().map(|root| checked_single(root, "docDefaults")).transpose()?.flatten()
        .map(|root| checked_single(root, "rPrDefault")).transpose()?.flatten();
    let Some(wrapper) = wrapper else { return Ok(GlobalRunContext { state: GlobalRunState::Missing, signature: None, theme_dependent: false }); };
    tree_budget(wrapper)?;
    let pr = checked_single(wrapper, "rPr")?;
    let meaningful = |node: &XmlNode| node.children().next().is_some()
        || node.attributes.keys().any(|key| key != "xmlns" && !key.starts_with("xmlns:"))
        || node.content.iter().any(|part| matches!(part, XmlPart::Text(text) if !text.trim().is_empty()));
    let explicit = wrapper.attributes.keys().any(|key| key != "xmlns" && !key.starts_with("xmlns:"))
        || wrapper.content.iter().any(|part| matches!(part, XmlPart::Text(text) if !text.trim().is_empty()))
        || wrapper.children().any(|node| !word(node, "rPr")) || pr.is_some_and(meaningful);
    Ok(GlobalRunContext { state: if explicit { GlobalRunState::Explicit } else { GlobalRunState::Empty },
        signature: Some(signature(wrapper)?), theme_dependent: theme_dependent(wrapper) })
}

pub(super) fn automatic_context(styles: &WordMathStyles) -> Result<Option<AutomaticCharacterContext>> {
    let Some(id) = &styles.character_default else { return Ok(None); };
    let mut hash = Sha256::new(); let mut dependencies = false;
    for node in styles.chain(id, "character")? {
        tree_budget(node)?;
        // Compare the complete selected definition, not just four current
        // effective values or a familiar ID with a different basedOn ancestor.
        hash.update(signature(node)?.as_bytes());
        dependencies |= theme_dependent(node);
    }
    Ok(Some(AutomaticCharacterContext { selected_id: id.clone(), signature: format!("{:x}", hash.finalize()), theme_dependent: dependencies }))
}

pub(super) fn globals_compatible(source: &GlobalRunContext, target: &GlobalRunContext) -> bool {
    // Missing/Empty retain the C68 counterfactual four-axis policy. Only a
    // claimed nonempty source global needs this stronger exact-context proof.
    source.state != GlobalRunState::Explicit || source == target && !source.theme_dependent
}

pub(super) fn automatic_compatible(source: &AutomaticCharacterContext, target: Option<&AutomaticCharacterContext>) -> bool {
    target.is_some_and(|target| source == target && !source.theme_dependent)
}

pub(super) fn properties_signature(properties: &Properties) -> Result<BTreeMap<String, String>> {
    properties.iter().map(|(key, value)| Ok((key.clone(), signature(value)?))).collect()
}

// Complete-property proofs must not turn a bounded style catalogue into an
// unbounded per-run clone. Charge the existing byte budget before each new
// full-property clone, including the inherited half of a partial font/lang.
pub(super) fn clone_properties(properties: &Properties, work: &mut usize) -> Result<Properties> {
    for node in properties.values() { charge(work, tree_budget(node)?)?; }
    Ok(properties.clone())
}

pub(super) fn apply_properties(properties: &mut Properties, pr: Option<&XmlNode>, toggle: bool, work: &mut usize) -> Result<()> {
    if let Some(pr) = pr {
        charge(work, tree_budget(pr)?)?;
        for node in pr.children().filter(|node| node.namespace == WORD_NS && matches!(node.local(), "rFonts" | "lang")) {
            if let Some(previous) = properties.get(node.local()) { charge(work, tree_budget(previous)?)?; }
        }
    }
    apply(properties, pr, toggle)
}
