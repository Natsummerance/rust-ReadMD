//! Word style context carried beside, rather than inside, an Office equation.
//! The readable DSL retains the referenced style definitions. Rendering and
//! standalone DOCX projection resolve the same cascade without changing them.
use crate::math_document::{XmlNode, XmlPart, MATH_NS, WORD_NS};
use std::collections::{BTreeMap, BTreeSet};

#[path = "math_word/paragraph.rs"]
mod paragraph;
pub(crate) use paragraph::{word_paragraph_context, WordParagraphContext, WordFiniteParagraph, WordParagraphJustification};
#[path = "math_word/native_paragraph.rs"]
mod native_paragraph;
pub(crate) use native_paragraph::{capture as capture_native_paragraph, NativeParagraphSource,
    NativeParagraphRegistry, NativeParagraphResolution};
#[cfg(test)]
pub(crate) use paragraph::WordParagraphOrigin;

#[cfg(test)]
#[path = "math_word/capture_tests.rs"]
mod paragraph_capture_tests;

type Result<T> = std::result::Result<T, &'static str>;
fn word(n: &XmlNode, local: &str) -> bool { n.namespace == WORD_NS && n.local() == local }
fn child<'a>(n: &'a XmlNode, local: &str) -> Option<&'a XmlNode> { n.children().find(|n| word(n, local)) }
fn attr<'a>(n: &'a XmlNode, local: &str) -> Option<&'a str> {
    n.attributes.iter().find(|(key, _)| key.rsplit(':').next() == Some(local)
        && n.attribute_namespaces.get(*key).is_some_and(|uri| uri == WORD_NS)).map(|(_, v)| v.as_str())
}
fn unique_resolved_attributes(node: &XmlNode) -> Result<()> {
    // Check one bounded node before any attribute can choose a style identity.
    // Namespace declarations bind aliases; they are not ordinary attributes.
    let mut seen = BTreeSet::new();
    for key in node.attributes.keys() {
        if key == "xmlns" || key.starts_with("xmlns:") { continue; }
        let namespace = node.attribute_namespaces.get(key).map(String::as_str).unwrap_or("");
        let local = key.rsplit(':').next().unwrap_or(key);
        if !seen.insert((namespace, local)) { return Err("math_invalid_property"); }
    }
    Ok(())
}
fn enabled(n: &XmlNode) -> Result<bool> {
    match attr(n, "val").unwrap_or("true") {
        "1" | "true" | "on" => Ok(true), "0" | "false" | "off" => Ok(false),
        _ => Err("math_invalid_property"),
    }
}
pub(crate) fn bind_used_namespaces(n: &mut XmlNode) {
    // Resolved namespace URIs let a moved property bind only the prefixes it
    // actually uses. Copying every root declaration into every style/node can
    // amplify a small input to gigabytes; never do that on a render hot path.
    if let Some((prefix,_))=n.name.split_once(':') {
        if !n.namespace.is_empty() { n.attributes.insert(format!("xmlns:{prefix}"),n.namespace.clone()); }
    } else if !n.namespace.is_empty() { n.attributes.insert("xmlns".into(),n.namespace.clone()); }
    for (key,uri) in &n.attribute_namespaces {
        if let Some((prefix,_))=key.split_once(':') { if prefix!="xml"&&!uri.is_empty() { n.attributes.insert(format!("xmlns:{prefix}"),uri.clone()); } }
    }
    for part in &mut n.content { if let XmlPart::Element(child)=part { bind_used_namespaces(child); } }
}
fn named(local: &str) -> XmlNode {
    XmlNode { id: String::new(), name: format!("w:{local}"), namespace: WORD_NS.into(),
        attributes: [("xmlns:w".into(), WORD_NS.into())].into(), attribute_namespaces: BTreeMap::new(), content: vec![] }
}

#[derive(Default, Debug)]
pub(crate) struct WordMathStyles {
    root: Option<XmlNode>, styles: BTreeMap<String, XmlNode>,
    paragraph_default: Option<String>, character_default: Option<String>,
}
impl WordMathStyles {
    pub(crate) fn parse(source: &str) -> Result<Self> {
        let root = crate::math_document::parse_xml(source)?;
        let roots = root.children().collect::<Vec<_>>();
        if roots.len() != 1 || !word(roots[0], "styles") { return Err("math_invalid_xml"); }
        Self::from_node(roots[0])
    }
    fn from_node(root: &XmlNode) -> Result<Self> {
        unique_resolved_attributes(root)?;
        let mut out = Self { root: Some(root.clone()), ..Self::default() };
        for style in root.children().filter(|n| word(n, "style")) {
            unique_resolved_attributes(style)?;
            let Some(id) = attr(style, "styleId") else { return Err("math_invalid_property"); };
            if id.is_empty() || out.styles.contains_key(id) { return Err("math_invalid_property"); }
            let style = style.clone();
            if attr(&style, "default").is_some() && enabled_attribute(&style, "default")? {
                let target = match attr(&style, "type") {
                    Some("paragraph") => Some(&mut out.paragraph_default),
                    Some("character") => Some(&mut out.character_default), _ => None,
                };
                if let Some(target) = target { if target.replace(id.into()).is_some() { return Err("math_invalid_property"); } }
            }
            out.styles.insert(id.into(), style);
        }
        Ok(out)
    }
    fn chain<'a>(&'a self, id: &str, kind: &str) -> Result<Vec<&'a XmlNode>> {
        let mut chain = vec![]; let mut seen = BTreeSet::new(); let mut current = Some(id);
        while let Some(id) = current {
            if chain.len() >= 128 || !seen.insert(id) { return Err("math_invalid_property"); }
            let style = self.styles.get(id).ok_or("math_invalid_property")?;
            if attr(style, "type") != Some(kind) { return Err("math_invalid_property"); }
            let mut references = style.children().filter(|node| word(node, "basedOn"));
            let reference = references.next();
            if references.next().is_some() { return Err("math_invalid_property"); }
            if let Some(reference) = reference { unique_resolved_attributes(reference)?; }
            chain.push(style); current = reference.and_then(|node| attr(node, "val"));
        }
        chain.reverse(); Ok(chain)
    }
    fn paragraph_id<'a>(&'a self, ppr: Option<&'a XmlNode>) -> Option<&'a str> {
        ppr.and_then(|n| child(n, "pStyle")).and_then(|n| attr(n, "val")).or(self.paragraph_default.as_deref())
    }
    /// Keep only the definitions the equation uses, in original catalogue order.
    /// No disk writes, whole-document duplication or inherited XML black box.
    pub(crate) fn capture(&self, paragraph: Option<&XmlNode>, equation: &XmlNode) -> Result<Vec<XmlNode>> {
        let mut used = BTreeSet::new();
        let mut include = |id: &str, kind: &str| -> Result<()> {
            for style in self.chain(id, kind)? { used.insert(attr(style, "styleId").unwrap().to_string()); } Ok(())
        };
        if let Some(id) = self.paragraph_id(paragraph) { include(id, "paragraph")?; }
        if let Some(id) = &self.character_default { include(id, "character")?; }
        fn references(n: &XmlNode, out: &mut BTreeSet<String>) {
            if word(n, "rPr") { if let Some(id) = child(n, "rStyle").and_then(|n| attr(n, "val")) { out.insert(id.into()); } }
            for n in n.children() { references(n, out); }
        }
        let mut ids = BTreeSet::new(); references(equation, &mut ids);
        for id in &ids { include(id, "character")?; }
        let mut context = vec![];
        if let Some(root) = &self.root {
            let mut subset = root.clone();
            subset.content.retain(|part| matches!(part, XmlPart::Element(n) if word(n,"docDefaults")
                || word(n,"style") && attr(n,"styleId").is_some_and(|id|used.contains(id))));
            if !subset.content.is_empty() { context.push(subset); }
        }
        if let Some(paragraph) = paragraph {
            if !word(paragraph,"pPr") { return Err("math_invalid_property"); }
            context.push(paragraph.clone());
        }
        Ok(context)
    }
}
fn enabled_attribute(n: &XmlNode, name: &str) -> Result<bool> {
    match attr(n, name).unwrap_or("false") {
        "true" | "1" | "on" => Ok(true), "false" | "0" | "off" => Ok(false), _ => Err("math_invalid_property"),
    }
}

type Properties = BTreeMap<String, XmlNode>;
fn bind_attribute(n:&mut XmlNode, preferred:&str, namespace:&str)->String {
    if n.attributes.get(&format!("xmlns:{preferred}")).is_none_or(|uri|uri==namespace) {
        n.attributes.insert(format!("xmlns:{preferred}"),namespace.into());return preferred.into();
    }
    for i in 0..=n.attributes.len() {
        let prefix=format!("readmdAttribute{i}");
        if n.attributes.get(&format!("xmlns:{prefix}")).is_none_or(|uri|uri==namespace) {
            n.attributes.insert(format!("xmlns:{prefix}"),namespace.into());return prefix;
        }
    }
    unreachable!("more fresh names than existing attributes")
}
fn apply(properties: &mut Properties, pr: Option<&XmlNode>, toggle: bool) -> Result<()> {
    let Some(pr) = pr else { return Ok(()); };
    for n in pr.children().filter(|n| n.namespace == WORD_NS && n.local() != "rStyle") {
        let mut n = n.clone(); bind_used_namespaces(&mut n);
        let key = n.local().to_string();
        if matches!(key.as_str(), "b" | "bCs" | "i" | "iCs" | "caps" | "smallCaps" | "strike" | "dstrike" | "outline" | "shadow" | "emboss" | "imprint" | "vanish") {
            let value = enabled(&n)?;
            if toggle && !value { continue; }
            let value = if toggle { !properties.get(&key).map(enabled).transpose()?.unwrap_or(false) } else { value };
            // Only the effective copy is canonicalized; explicit author values
            // and omitted switches remain intact in the DSL context.
            n.attributes.retain(|k, _| k.rsplit(':').next() != Some("val") || n.attribute_namespaces.get(k).is_none_or(|uri|uri!=WORD_NS));
            let prefix=bind_attribute(&mut n,"w",WORD_NS);let key=format!("{prefix}:val");
            n.attributes.insert(key.clone(), if value {"1"} else {"0"}.into());
            n.attribute_namespaces.insert(key, WORD_NS.into());
        } else if matches!(key.as_str(), "rFonts" | "lang") {
            if let Some(previous) = properties.get(&key) {
                let mut merged = previous.clone();
                // Clear paired attributes from the inherited level first.
                // When both direct and theme values occur on this same level,
                // keep both: the theme has priority regardless of XML prefixes
                // or BTreeMap iteration order. Higher direct formatting still
                // overrides a lower-level theme for the corresponding slot.
                if key == "rFonts" {
                    for (direct, theme) in [("ascii", "asciiTheme"), ("hAnsi", "hAnsiTheme"), ("eastAsia", "eastAsiaTheme"), ("cs", "cstheme")] {
                        if attr(&n, direct).is_some() || attr(&n, theme).is_some() {
                            let old = merged.attributes.keys().filter(|k| {
                                merged.attribute_namespaces.get(*k).is_some_and(|uri| uri == WORD_NS)
                                    && matches!(k.rsplit(':').next(), Some(local) if local == direct || local == theme)
                            }).cloned().collect::<Vec<_>>();
                            for old in old { merged.attributes.remove(&old); merged.attribute_namespaces.remove(&old); }
                        }
                    }
                }
                for (k, v) in &n.attributes {
                    if k=="xmlns" || k.starts_with("xmlns:") { continue; }
                    let local = k.rsplit(':').next().unwrap_or(k);
                    let namespace = n.attribute_namespaces.get(k);
                    merged.attributes.retain(|old,_| !(old.rsplit(':').next()==Some(local) && merged.attribute_namespaces.get(old)==namespace));
                    let key=if let Some(uri)=namespace.filter(|uri|!uri.is_empty()) {
                        let prefix=bind_attribute(&mut merged,k.split_once(':').map(|p|p.0).unwrap_or("w"),uri);
                        format!("{prefix}:{local}")
                    } else { k.clone() };
                    merged.attributes.insert(key.clone(), v.clone());
                    if let Some(uri) = namespace { merged.attribute_namespaces.insert(key, uri.clone()); }
                }
                n = merged;
            }
        }
        properties.insert(key, n);
    }
    Ok(())
}
fn properties_node(properties: Properties) -> XmlNode {
    // CT_RPr requires a defined sequence even when the source used aliases.
    const ORDER: &[&str] = &["rFonts","b","bCs","i","iCs","caps","smallCaps","strike","dstrike","outline","shadow","emboss","imprint","noProof","snapToGrid","vanish","webHidden","color","spacing","w","kern","position","sz","szCs","highlight","u","effect","bdr","shd","fitText","vertAlign","rtl","cs","em","lang","eastAsianLayout","specVanish","oMath","rPrChange"];
    let mut children = properties.into_values().collect::<Vec<_>>();
    children.sort_by_key(|n| ORDER.iter().position(|v| *v == n.local()).unwrap_or(ORDER.len()));
    let mut pr = named("rPr"); pr.content = children.into_iter().map(XmlPart::Element).collect(); pr
}
fn is_context(n: &XmlNode) -> bool {
    word(n,"styles") || word(n,"pPr") || crate::math_theme::is_context(n)
        || crate::math_font_theme::is_context(n)
}
fn remove_context(root: &mut XmlNode) {
    root.content.retain(|part| !matches!(part,XmlPart::Element(n) if is_context(n)));
}

/// Effective render/export copy. The source tree and style IDs remain untouched.
/// Word paragraph-mark rPr is deliberately not inherited by paragraph text.
pub(crate) fn materialize(root: &mut XmlNode) -> Result<()> {
    let theme=crate::math_theme::ThemeColors::from_context(root)?;
    let fonts=crate::math_font_theme::ThemeFonts::from_context(root)?;
    let contexts = root.children().filter(|n| word(n,"styles")).collect::<Vec<_>>();
    if contexts.len()>1 || root.children().filter(|n|word(n,"pPr")).count()>1 { return Err("math_invalid_property"); }
    if contexts.is_empty() && theme.is_empty() && fonts.is_empty() {
        // Direct paragraph properties also arrive without a style catalogue.
        // Their effective values have already been projected by the caller;
        // never leak this context into a second native paragraph-properties node.
        remove_context(root);
        return Ok(());
    }
    let styles = contexts.first().map(|n|WordMathStyles::from_node(n)).transpose()?.unwrap_or_default();
    let mut base = Properties::new();
    let defaults = styles.root.as_ref().and_then(|r|child(r,"docDefaults")).and_then(|r|child(r,"rPrDefault")).and_then(|r|child(r,"rPr"));
    apply(&mut base, defaults, false)?;
    if let Some(id) = styles.paragraph_id(root.children().find(|n|word(n,"pPr"))) {
        for style in styles.chain(id,"paragraph")? { apply(&mut base,child(style,"rPr"),true)?; }
    }
    fn walk(n: &mut XmlNode, base: &Properties, styles: &WordMathStyles,theme:&crate::math_theme::ThemeColors,fonts:&crate::math_font_theme::ThemeFonts) -> Result<()> {
        if n.namespace == MATH_NS && matches!(n.local(),"r"|"ctrlPr") {
            let direct = child(n,"rPr").cloned();
            let mut effective = base.clone();
            let character = direct.as_ref().and_then(|r|child(r,"rStyle")).and_then(|r|attr(r,"val")).or(styles.character_default.as_deref());
            if let Some(id) = character { for style in styles.chain(id,"character")? { apply(&mut effective,child(style,"rPr"),true)?; } }
            apply(&mut effective,direct.as_ref(),false)?;
            if let Some(color)=effective.get_mut("color"){theme.resolve_run_color(color)?;}
            if let Some(rfonts)=effective.get_mut("rFonts"){fonts.resolve(rfonts)?;}
            if !effective.is_empty() {
                let direct_position = n.content.iter().position(|p|matches!(p,XmlPart::Element(c) if word(c,"rPr")));
                n.content.retain(|p|!matches!(p,XmlPart::Element(c) if word(c,"rPr")));
                // Replacing direct formatting must not reorder adjacent author
                // extensions, comments or local math properties.
                let at = direct_position.unwrap_or_else(||n.content.iter().position(|p|matches!(p,XmlPart::Element(c) if c.namespace==MATH_NS && matches!(c.local(),"t"|"break"))).unwrap_or(n.content.len()));
                n.content.insert(at,XmlPart::Element(properties_node(effective)));
            } else if let Some(pr) = n.content.iter_mut().find_map(|part| match part {
                XmlPart::Element(pr) if word(pr,"rPr") => Some(pr), _ => None,
            }) {
                // Even an empty character chain is still a source reference.
                // Its catalogue is removed below, so a materialized copy must
                // not retain that now-dangling rStyle. Keep the original empty
                // container, comments and foreign children in this case.
                pr.content.retain(|part|!matches!(part,XmlPart::Element(reference) if word(reference,"rStyle")));
            }
        }
        for p in &mut n.content { if let XmlPart::Element(c)=p { if !is_context(c) { walk(c,base,styles,theme,fonts)?; } } }
        Ok(())
    }
    walk(root,&base,&styles,&theme,&fonts)?;
    remove_context(root);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context(styles:&str, paragraph:&str, runs:&str)->String {
        format!("<w:styles xmlns:w=\"{WORD_NS}\">{styles}</w:styles><w:pPr xmlns:w=\"{WORD_NS}\">{paragraph}</w:pPr><m:oMath xmlns:m=\"{MATH_NS}\" xmlns:w=\"{WORD_NS}\">{runs}</m:oMath>")
    }
    fn runs(root:&crate::omml::Node)->Vec<&crate::omml::Node> {root.children.iter().find(|n|n.name=="oMath").unwrap().children.iter().filter(|n|n.name=="r").collect()}
    #[test]
    fn math_word_materialization_removes_only_root_context_even_without_styles() {
        let run = r#"<m:r><w:rPr><w:rFonts w:ascii="STIX Two Math"/><w:b w:val="0"/><w:color w:val="112233"/><w:sz w:val="24"/></w:rPr><f:rPr f:note="keep"/><m:t>x</m:t></m:r>"#;
        for context in ["", "<w:styles/>", "<w:clrSchemeMapping/>"] {
            let xml = format!(r#"{context}<w:pPr><w:ind w:left="720"/><w:jc w:val="right"/><w:rPr><w:sz w:val="90"/></w:rPr></w:pPr><f:pPr f:note="keep"/><f:styles/><m:mathPr><m:dispDef m:val="0"/></m:mathPr><m:oMath>{run}</m:oMath>"#);
            let xml = format!(r#"<root xmlns:m="{MATH_NS}" xmlns:w="{WORD_NS}" xmlns:f="urn:foreign">{xml}</root>"#);
            let parsed = crate::math_document::parse_xml(&xml).unwrap();
            let original = parsed.children().next().unwrap().clone();
            let before = original.xml();
            let mut effective = original.clone();
            materialize(&mut effective).unwrap();
            assert_eq!(original.xml(), before);
            assert!(!effective.children().any(is_context));
            assert_eq!(effective.children().filter(|n| n.namespace == "urn:foreign").count(), 2);
            assert!(effective.children().any(|n| n.namespace == MATH_NS && n.local() == "mathPr"));
            let before_math = original.children().find(|n| n.namespace == MATH_NS && n.local() == "oMath").unwrap();
            let after_math = effective.children().find(|n| n.namespace == MATH_NS && n.local() == "oMath").unwrap();
            assert_eq!(crate::math_document::xml_semantics(before_math), crate::math_document::xml_semantics(after_math));
            assert_eq!(crate::math_document::xml_semantics(&effective), crate::math_document::xml_semantics(&crate::math_document::parse_xml(&effective.xml()).unwrap().children().next().unwrap().clone()));
        }
    }
    #[test]
    fn math_word_materialization_rejects_duplicate_paragraph_context_before_fast_path() {
        for context in ["", "<w:styles/>", "<w:clrSchemeMapping/>"] {
            let xml = format!(r#"{context}<w:pPr/><v:pPr xmlns:v="{WORD_NS}"/><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>"#);
            let mut root = crate::math_document::parse_xml(&xml).unwrap();
            let before = root.xml();
            assert_eq!(materialize(&mut root), Err("math_invalid_property"));
            assert_eq!(root.xml(), before);
        }
    }
    #[test]
    fn math_word_style_cascade_toggles_and_direct_formatting_are_distinct() {
        let xml=context(r#"<w:docDefaults><w:rPrDefault><w:rPr><w:b/><w:i w:val="0"/><w:sz w:val="24"/></w:rPr></w:rPrDefault></w:docDefaults>
            <w:style w:type="paragraph" w:styleId="Base"><w:rPr><w:b/><w:i/></w:rPr></w:style>
            <w:style w:type="paragraph" w:styleId="Derived"><w:basedOn w:val="Base"/><w:rPr><w:b w:val="0"/><w:color w:val="336699"/></w:rPr></w:style>
            <w:style w:type="character" w:styleId="Accent"><w:rPr><w:b/></w:rPr></w:style>"#,
            r#"<w:pStyle w:val="Derived"/><w:rPr><w:sz w:val="90"/></w:rPr>"#,
            r#"<m:r><m:t>x</m:t></m:r><m:r><w:rPr><w:rStyle w:val="Accent"/><w:i w:val="0"/></w:rPr><m:t>y</m:t></m:r>
            <m:r><w:rPr><w:rStyle w:val="Accent"/><w:b w:val="0"/></w:rPr><m:t>z</m:t></m:r>"#).replace("\n            ","");
        let parsed=crate::omml::parse(&xml).unwrap();let styles=runs(&parsed).into_iter().map(crate::omml::run_style).collect::<Vec<_>>();
        assert!(!styles[0].bold && !styles[0].upright); // two true bold toggles cancel; false style is a no-op
        assert!(styles[1].bold && styles[1].upright); // character toggle, then absolute direct italic off
        assert!(!styles[2].bold && !styles[2].upright);
        assert!(styles.iter().all(|s|s.half_points==Some(24)&&s.color==Some([0x33,0x66,0x99]))); // paragraph-mark size does not leak
        let original=crate::math_document::parse_xml(&xml).unwrap();let readable=crate::math_migration::readable(&xml).unwrap();
        assert!(readable.contains("word.styles(")&&readable.contains("word.b(val: \"0\")"));
        let (compiled,_)=crate::math_dsl::compile(&readable,false).unwrap();
        assert_eq!(crate::math_document::xml_semantics(&original),crate::math_document::xml_semantics(&compiled));
        assert_eq!(crate::omml::parse(&compiled.xml()).unwrap(),parsed);
    }
    #[test]
    fn math_word_default_styles_and_default_character_replacement() {
        let xml=context(r#"<w:style w:type="paragraph" w:styleId="Normal" w:default="1"><w:rPr><w:b/></w:rPr></w:style>
            <w:style w:type="character" w:styleId="Default" w:default="true"><w:rPr><w:i/></w:rPr></w:style>
            <w:style w:type="character" w:styleId="Plain"><w:rPr><w:b/></w:rPr></w:style>"#,"",
            r#"<m:r><m:t>x</m:t></m:r><m:r><w:rPr><w:rStyle w:val="Plain"/></w:rPr><m:t>y</m:t></m:r>"#);
        let parsed=crate::omml::parse(&xml).unwrap();let r=runs(&parsed);
        assert!(crate::omml::run_style(r[0]).bold&&!crate::omml::run_style(r[0]).upright);
        assert!(!crate::omml::run_style(r[1]).bold&&crate::omml::run_style(r[1]).word_italic.is_none());
    }
    #[test]
    fn math_word_context_is_bounded_referenced_and_namespace_aware() {
        let styles=format!(r#"<v:styles xmlns:v="{WORD_NS}" xmlns:f="urn:unrelated"><v:docDefaults><v:rPrDefault><v:rPr><v:rFonts v:asciiTheme="minorHAnsi" v:eastAsia="Noto Serif CJK"/><v:b/></v:rPr></v:rPrDefault></v:docDefaults>
            <v:style v:type="paragraph" v:styleId="P" v:default="1"><v:rPr><v:rFonts v:ascii="STIX Two Math"/></v:rPr></v:style>
            <v:style v:type="character" v:styleId="Unused"><v:rPr><v:sz v:val="120"/></v:rPr></v:style></v:styles>"#);
        let styles=WordMathStyles::parse(&styles).unwrap();
        let equation=crate::math_document::parse_xml(&format!("<m:oMath xmlns:m=\"{MATH_NS}\"><m:r><m:t>x</m:t></m:r></m:oMath>")).unwrap();
        let context=styles.capture(None,&equation).unwrap();assert_eq!(context.len(),1);assert!(!context[0].xml().contains("Unused"));
        let xml=format!("{}{}",context[0].xml(),equation.xml());let mut effective=crate::math_document::parse_xml(&xml).unwrap();
        materialize(&mut effective).unwrap();let reparse=crate::math_document::parse_xml(&effective.xml()).unwrap();assert_eq!(crate::math_document::xml_semantics(&effective),crate::math_document::xml_semantics(&reparse));
        let parsed=crate::omml::parse(&xml).unwrap();let pr=runs(&parsed)[0].children.iter().find(|n|n.namespace==WORD_NS&&n.name=="rPr").unwrap();
        let fonts=pr.children.iter().find(|n|n.name=="rFonts").unwrap();assert_eq!(fonts.attr("ascii"),Some("STIX Two Math"));assert_eq!(fonts.attr("eastAsia"),Some("Noto Serif CJK"));assert!(fonts.attr("asciiTheme").is_none());
        assert!(crate::omml::run_style(runs(&parsed)[0]).bold);
        let foreign=format!("<m:oMath xmlns:m=\"{MATH_NS}\" xmlns:w=\"{WORD_NS}\" xmlns:f=\"urn:unrelated\"><m:r><w:rPr><f:b/><f:i/><f:sz f:val=\"80\"/></w:rPr><m:t>x</m:t></m:r></m:oMath>");
        assert_eq!(crate::omml::run_style(runs(&crate::omml::parse(&foreign).unwrap())[0]),crate::omml::RunStyle::default());
    }
    #[test]
    fn math_word_cycles_missing_styles_and_wrong_types_fail_without_fallback() {
        for definitions in [
            r#"<w:style w:type="paragraph" w:styleId="A"><w:basedOn w:val="A"/></w:style>"#,
            r#"<w:style w:type="paragraph" w:styleId="A"><w:basedOn w:val="B"/></w:style><w:style w:type="paragraph" w:styleId="B"><w:basedOn w:val="A"/></w:style>"#,
            r#"<w:style w:type="paragraph" w:styleId="A"><w:basedOn w:val="Missing"/></w:style>"#,
            r#"<w:style w:type="character" w:styleId="A"/>"#,
            r#"<w:style w:type="paragraph" w:styleId="A"/><w:style w:type="paragraph" w:styleId="A"/>"#,
        ] {
            let xml=context(definitions,r#"<w:pStyle w:val="A"/>"#,r#"<m:r><m:t>x</m:t></m:r>"#);
            assert_eq!(crate::omml::parse(&xml).unwrap_err(),"math_invalid_property");
            let source=crate::math_dsl::decompile(&crate::math_document::parse_xml(&xml).unwrap()).unwrap();
            assert_eq!(crate::math_dsl::compile(&source,false).unwrap_err(),"math_invalid_property");
        }
    }
    #[test]
    fn math_word_projection_does_not_rebind_colliding_external_attribute_prefixes() {
        let xml=format!(r#"<v:styles xmlns:v="{WORD_NS}" xmlns:w="urn:foreign"><v:docDefaults><v:rPrDefault><v:rPr><v:rFonts v:ascii="First" w:note="keep"/><v:b w:val="external"/></v:rPr></v:rPrDefault></v:docDefaults></v:styles>
            <m:oMath xmlns:m="{MATH_NS}" xmlns:q="{WORD_NS}" xmlns:v="urn:other"><m:r><q:rPr><q:rFonts q:hAnsi="Second" v:note="new"/></q:rPr><m:t>x</m:t></m:r></m:oMath>"#);
        let mut root=crate::math_document::parse_xml(&xml).unwrap();materialize(&mut root).unwrap();
        let reparsed=crate::math_document::parse_xml(&root.xml()).unwrap();assert_eq!(crate::math_document::xml_semantics(&root),crate::math_document::xml_semantics(&reparsed));
        let r=reparsed.children().find(|n|n.local()=="oMath").unwrap().children().find(|n|n.local()=="r").unwrap();
        let pr=child(r,"rPr").unwrap();let fonts=child(pr,"rFonts").unwrap();assert_eq!(attr(fonts,"ascii"),Some("First"));assert_eq!(attr(fonts,"hAnsi"),Some("Second"));
        let foreign=fonts.attributes.iter().filter_map(|(key,value)|fonts.attribute_namespaces.get(key).filter(|uri|uri.starts_with("urn:")).map(|uri|(uri.as_str(),value.as_str()))).collect::<BTreeMap<_,_>>();
        assert_eq!(foreign,BTreeMap::from([("urn:foreign","keep"),("urn:other","new")]));
        let bold=child(pr,"b").unwrap();assert_eq!(attr(bold,"val"),Some("1"));assert!(bold.attributes.iter().any(|(key,v)|v=="external"&&bold.attribute_namespaces.get(key).is_some_and(|uri|uri=="urn:foreign")));
    }
    #[test]
    fn math_word_unused_namespace_catalogue_does_not_expand_every_style_and_run() {
        let declarations=(0..256).map(|i|format!(" xmlns:unused{i}=\"urn:unused:{i}\"")).collect::<String>();
        let definitions=(0..128).map(|i|format!("<w:style w:type=\"paragraph\" w:styleId=\"P{i}\"><w:rPr><w:b/><w:sz w:val=\"24\"/></w:rPr></w:style>")).collect::<String>();
        let xml=format!("<w:styles xmlns:w=\"{WORD_NS}\"{declarations}>{definitions}</w:styles>");
        let styles=WordMathStyles::parse(&xml).unwrap();let ppr=crate::math_document::parse_xml("<w:pPr><w:pStyle w:val=\"P0\"/></w:pPr>").unwrap();
        let eq=crate::math_document::parse_xml(&format!("<m:oMath>{}</m:oMath>","<m:r><m:t>x</m:t></m:r>".repeat(100))).unwrap();
        let context=styles.capture(ppr.children().next(),&eq).unwrap();assert_eq!(context[0].children().filter(|n|word(n,"style")).count(),1);
        let mut root=crate::math_document::parse_xml(&format!("{}{}{}",context[0].xml(),context[1].xml(),eq.xml())).unwrap();materialize(&mut root).unwrap();
        let output=root.xml();assert!(!output.contains("unused"));assert!(output.len()<40000,"{}",output.len());
        assert_eq!(crate::math_document::xml_semantics(&root),crate::math_document::xml_semantics(&crate::math_document::parse_xml(&output).unwrap()));
    }
    #[test]
    fn math_word_export_is_self_contained_and_keeps_readable_context_unmodified() {
        let xml=context(r#"<w:style w:type="paragraph" w:styleId="P"><w:rPr><w:b/><w:sz w:val="36"/></w:rPr></w:style>
            <w:style w:type="character" w:styleId="C"><w:rPr><w:i/></w:rPr></w:style>"#,
            r#"<w:pStyle w:val="P"/>"#,r#"<m:r><w:rPr><w:rStyle w:val="C"/></w:rPr><m:t>x</m:t></m:r>"#);
        let doc=crate::math_document::parse(&xml,"omml",false).unwrap();let original=doc.root.xml();
        let (fragment,_)=crate::omml::docx_math_fragment(&original,false).unwrap();
        assert!(!fragment.contains("styles")&&!fragment.contains("pPr")&&!fragment.contains("rStyle"));
        assert!(fragment.contains("w:b")&&fragment.contains("w:i")&&fragment.contains("36"));
        let from=crate::omml::parse(&original).unwrap();let to=crate::omml::parse(&fragment).unwrap();
        assert_eq!(crate::omml::run_style(runs(&from)[0]),crate::omml::run_style(runs(&to)[0]));assert_eq!(doc.root.xml(),original);
    }
}
