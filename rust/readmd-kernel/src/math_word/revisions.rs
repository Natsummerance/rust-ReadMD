//! Direct review records stay separate from the current formatting cascade.
use super::{word, Result, XmlNode, WORD_NS};
use std::collections::BTreeMap;

pub(crate) const WARN_REVISION_NOT_PRESERVED: &str = "math_revision_not_preserved";

/// A deliberately small preservation proof. The caller has already proved the
/// current run has no inherited/style/theme/font context to flatten. Its old
/// formatting must not introduce a style reference, theme token, nested review
/// record or foreign extension whose dependencies we cannot prove here.
pub(super) fn independent_history(change: &XmlNode) -> bool {
    fn concrete(node: &XmlNode) -> bool {
        node.namespace == WORD_NS
            && !matches!(node.local(), "rStyle" | "rPrChange")
            && node.attributes.keys().all(|key| {
                key == "xmlns" || key.starts_with("xmlns:")
                    || node.attribute_namespaces.get(key).is_some_and(|uri| uri == WORD_NS)
                        && !key.rsplit(':').next().unwrap_or(key).to_ascii_lowercase().contains("theme")
            })
            && node.children().all(concrete)
    }
    let mut children = change.children();
    let old = children.next();
    old.is_some_and(|old| word(old,"rPr") && concrete(old)) && children.next().is_none()
}

fn records(root: &XmlNode) -> Result<BTreeMap<String, usize>> {
    fn visit(node: &XmlNode, out: &mut BTreeMap<String, usize>) -> Result<()> {
        if crate::speech::cancelled() { return Err("cancelled"); }
        if word(node,"rPrChange") {
            // Namespace prefixes and declarations may change when a record is
            // moved, but author/date/old properties/comments must all survive.
            let key = serde_json::to_string(&crate::math_document::xml_semantics(node))
                .map_err(|_|"math_invalid_source")?;
            *out.entry(key).or_default() += 1;
            // An outer record includes its complete subtree once, avoiding
            // repeated copies for malformed nested review records.
        } else {
            for child in node.children() { visit(child,out)?; }
        }
        Ok(())
    }
    let mut out = BTreeMap::new(); visit(root,&mut out)?; Ok(out)
}

/// Compare the actual final DOCX formula fragment, after native run projection,
/// with its source. No warning is inferred merely from the presence of history.
pub(crate) fn revision_not_preserved(source: &str, projected: &str) -> Result<bool> {
    if !source.contains("rPrChange") { return Ok(false); }
    let original = records(&crate::math_document::parse_xml(source)?)?;
    if original.is_empty() { return Ok(false); }
    let actual = records(&crate::math_document::parse_xml(projected)?)?;
    Ok(original.into_iter().any(|(record,count)| actual.get(&record).copied().unwrap_or(0) < count))
}
