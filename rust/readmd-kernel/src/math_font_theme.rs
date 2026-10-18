//! Readable Office theme font context. Effective copies resolve theme tokens;
//! source attributes and typeface names remain intact for round-trip editing.
use crate::math_document::{self, XmlNode, XmlPart, DRAWING_NS, WORD_NS};
use std::collections::BTreeSet;
type Result<T> = std::result::Result<T, &'static str>;
fn drawing(n: &XmlNode, name: &str) -> bool {
    n.namespace == DRAWING_NS && n.local() == name
}
fn word(n: &XmlNode, name: &str) -> bool {
    n.namespace == WORD_NS && n.local() == name
}
fn attr<'a>(n: &'a XmlNode, key: &str) -> Option<&'a str> {
    n.attributes
        .iter()
        .find(|(k, _)| {
            k.rsplit(':').next() == Some(key)
                && n.attribute_namespaces
                    .get(*k)
                    .is_some_and(|uri| uri == WORD_NS)
        })
        .map(|(_, v)| v.as_str())
}
fn single<'a>(mut nodes: impl Iterator<Item = &'a XmlNode>) -> Result<Option<&'a XmlNode>> {
    let first = nodes.next();
    if nodes.next().is_some() {
        Err("math_invalid_property")
    } else {
        Ok(first)
    }
}
pub(crate) fn is_context(n: &XmlNode) -> bool {
    drawing(n, "fontScheme") || word(n, "themeFontLang")
}
fn theme_token(token: &str) -> Result<(&str, &str, &str)> {
    Ok(match token {
        "majorAscii" | "majorHAnsi" => ("majorFont", "latin", "val"),
        "minorAscii" | "minorHAnsi" => ("minorFont", "latin", "val"),
        "majorEastAsia" => ("majorFont", "ea", "eastAsia"),
        "minorEastAsia" => ("minorFont", "ea", "eastAsia"),
        "majorBidi" => ("majorFont", "cs", "bidi"),
        "minorBidi" => ("minorFont", "cs", "bidi"),
        _ => return Err("math_invalid_property"),
    })
}
/// ISO 15924 tags used by DrawingML supplemental fonts. Explicit BCP47 script
/// subtags take priority; unrecognized languages retain the regional default.
fn script(language: &str) -> Option<&str> {
    for part in language.split('-').skip(1) {
        if part.len() == 4 && part.bytes().all(|b| b.is_ascii_alphabetic()) {
            return Some(match part {
                "Hans" => "Hans",
                "Hant" => "Hant",
                other => other,
            });
        }
    }
    let mut parts = language.split('-');
    let primary = parts.next()?;
    Some(match primary.to_ascii_lowercase().as_str() {
        "zh" => {
            if parts.any(|p| ["TW", "HK", "MO"].iter().any(|r| p.eq_ignore_ascii_case(r))) {
                "Hant"
            } else {
                "Hans"
            }
        }
        "ja" => "Jpan",
        "ko" => "Hang",
        "ar" | "fa" | "ur" | "ps" => "Arab",
        "he" | "yi" => "Hebr",
        "syr" => "Syrc",
        "dv" => "Thaa",
        "ru" | "uk" | "bg" | "mk" | "be" | "kk" | "ky" | "tg" | "mn" => "Cyrl",
        "el" => "Grek",
        "hy" => "Armn",
        "ka" => "Geor",
        "hi" | "mr" | "ne" | "sa" | "kok" => "Deva",
        "bn" | "as" => "Beng",
        "pa" => "Guru",
        "gu" => "Gujr",
        "or" => "Orya",
        "ta" => "Taml",
        "te" => "Telu",
        "kn" => "Knda",
        "ml" => "Mlym",
        "si" => "Sinh",
        "th" => "Thai",
        "lo" => "Laoo",
        "km" => "Khmr",
        "my" => "Mymr",
        "bo" | "dz" => "Tibt",
        "am" | "ti" => "Ethi",
        "chr" => "Cher",
        "iu" => "Cans",
        "ii" => "Yiii",
        "en" | "fr" | "de" | "es" | "pt" | "it" | "nl" | "sv" | "da" | "no" | "fi" | "pl"
        | "cs" | "sk" | "hu" | "ro" | "tr" | "vi" | "id" | "ms" | "sw" | "af" | "sq" | "eu"
        | "ca" | "et" | "gl" | "hr" | "is" | "lt" | "lv" | "sl" | "cy" | "ga" => "Latn",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math_document::MATH_NS;

    fn theme() -> String {
        format!(
            r#"<a:theme xmlns:a="{DRAWING_NS}"><a:themeElements><a:fontScheme name="Office">
            <a:majorFont><a:latin typeface="Major Latin"/><a:ea typeface=""/><a:cs typeface="Major Complex"/>
                <a:font script="Jpan" typeface="Major Japanese"/><a:font script="Hans" typeface="Major Chinese"/>
                <a:font script="Hant" typeface="Major Traditional"/><a:font script="Arab" typeface="Major Arabic"/></a:majorFont>
            <a:minorFont><a:latin typeface="Minor Latin"/><a:ea typeface="Minor East Asia"/><a:cs typeface="Minor Complex"/>
                <a:font script="Jpan" typeface="Minor Japanese"/></a:minorFont>
            </a:fontScheme></a:themeElements></a:theme>"#
        )
    }
    fn settings(attributes: &str) -> String {
        format!(r#"<w:settings xmlns:w="{WORD_NS}"><w:themeFontLang {attributes}/></w:settings>"#)
    }
    fn rfonts(attributes: &str) -> XmlNode {
        math_document::parse_xml(&format!(r#"<q:rFonts xmlns:q="{WORD_NS}" {attributes}/>"#))
            .unwrap()
            .children()
            .next()
            .unwrap()
            .clone()
    }
    fn run(root: &crate::omml::Node) -> &crate::omml::Node {
        if root.is_math() && root.name == "r" {
            return root;
        }
        root.children
            .iter()
            .find_map(|c| {
                fn find(n: &crate::omml::Node) -> Option<&crate::omml::Node> {
                    if n.is_math() && n.name == "r" {
                        Some(n)
                    } else {
                        n.children.iter().find_map(find)
                    }
                }
                find(c)
            })
            .unwrap()
    }

    #[test]
    fn math_fonts_theme_language_selects_supplemental_family_for_all_eight_tokens() {
        let languages = settings(r#"w:val="ja-JP" w:eastAsia="zh-TW" w:bidi="ar-SA""#);
        let fonts = ThemeFonts::from_parts(Some(&theme()), Some(&languages)).unwrap();
        for (token, expected) in [
            ("majorAscii", "Major Japanese"),
            ("majorHAnsi", "Major Japanese"),
            ("minorAscii", "Minor Japanese"),
            ("minorHAnsi", "Minor Japanese"),
            ("majorEastAsia", "Major Traditional"),
            ("minorEastAsia", "Minor East Asia"),
            ("majorBidi", "Major Arabic"),
            ("minorBidi", "Minor Complex"),
        ] {
            // The token determines its family/region; the containing slot does
            // not silently reinterpret a legal cross-slot theme reference.
            let mut n = rfonts(&format!(
                r#"q:asciiTheme="{token}" q:ascii="Producer Fallback""#
            ));
            fonts.resolve(&mut n).unwrap();
            assert_eq!(attr(&n, "ascii"), Some(expected), "{token}");
            assert!(attr(&n, "asciiTheme").is_none());
        }
        assert_eq!(script("ZH-hant-cn"), Some("hant"));
        assert_eq!(script("zh-hk"), Some("Hant"));
        assert_eq!(script("und-Arab"), Some("Arab"));
        assert_eq!(script("und"), None);
    }

    #[test]
    fn math_fonts_empty_or_absent_theme_retains_producer_fallback_without_host_lookup() {
        for fonts in [
            ThemeFonts::default(),
            ThemeFonts::from_parts(Some(&theme()), None).unwrap(),
        ] {
            let mut n = rfonts(r#"q:eastAsiaTheme="majorEastAsia" q:eastAsia="Producer Fallback""#);
            fonts.resolve(&mut n).unwrap();
            assert_eq!(attr(&n, "eastAsia"), Some("Producer Fallback"));
            assert_eq!(attr(&n, "eastAsiaTheme"), Some("majorEastAsia"));
        }
        let fonts = ThemeFonts::from_parts(Some(&theme()), None).unwrap();
        let mut n = rfonts(r#"q:hAnsiTheme="minorHAnsi""#);
        fonts.resolve(&mut n).unwrap();
        assert_eq!(attr(&n, "hAnsi"), Some("Minor Latin"));
    }

    #[test]
    fn math_fonts_capture_is_minimal_readable_and_round_trips_without_mutating_source() {
        let languages = settings(r#"w:val="ja-JP" w:eastAsia="zh-CN""#);
        let fonts = ThemeFonts::from_parts(Some(&theme()), Some(&languages)).unwrap();
        let n = rfonts(r#"q:asciiTheme="majorAscii" q:eastAsiaTheme="majorEastAsia""#);
        let context = fonts.capture(&[&n]).unwrap();
        assert_eq!(context.len(), 2);
        assert_eq!(
            context[0]
                .children()
                .map(XmlNode::local)
                .collect::<Vec<_>>(),
            ["majorFont"]
        );
        assert!(fonts
            .capture(&[&rfonts(r#"q:ascii="Plain""#)])
            .unwrap()
            .is_empty());
        let xml = format!(
            r#"{}{}<m:oMath xmlns:m="{MATH_NS}" xmlns:w="{WORD_NS}"><m:r><m:rPr><m:nor/></m:rPr><w:rPr><w:rFonts w:ascii="Old" w:asciiTheme="majorAscii" w:eastAsiaTheme="majorEastAsia"/></w:rPr><m:t>A中</m:t></m:r></m:oMath>"#,
            context[0].xml(),
            context[1].xml()
        );
        let source = math_document::parse_xml(&xml).unwrap();
        let readable = crate::math_dsl::decompile(&source).unwrap();
        assert!(readable.contains("drawing.fontScheme("));
        assert!(readable.contains("word.themeFontLang("));
        assert!(readable.contains("asciiTheme: \"majorAscii\""));
        let (compiled, _) = crate::math_dsl::compile(&readable, false).unwrap();
        assert_eq!(
            math_document::xml_semantics(&source),
            math_document::xml_semantics(&compiled)
        );
        let before = compiled.xml();
        let effective = crate::omml::parse(&before).unwrap();
        let slots = crate::omml::run_fonts(run(&effective));
        assert_eq!(slots.family('A'), Some("Major Japanese"));
        assert_eq!(slots.family('中'), Some("Major Chinese"));
        let (output, _) = crate::omml::docx_math_fragment(&before, false).unwrap();
        assert!(!output.contains("fontScheme") && !output.contains("themeFontLang"));
        assert!(!output.contains("asciiTheme") && !output.contains("eastAsiaTheme"));
        assert!(output.contains("Major Japanese") && output.contains("Major Chinese"));
        assert_eq!(compiled.xml(), before);
    }

    #[test]
    fn math_fonts_same_level_theme_wins_independent_of_prefix_but_higher_direct_wins() {
        let scheme = ThemeFonts::from_parts(Some(&theme()), None)
            .unwrap()
            .scheme
            .unwrap()
            .xml();
        for direct in [
            r#"a:asciiTheme="minorHAnsi" z:ascii="Same Level""#,
            r#"z:asciiTheme="minorHAnsi" a:ascii="Same Level""#,
        ] {
            let xml = format!(
                r#"{scheme}<w:styles xmlns:w="{WORD_NS}"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Inherited" w:eastAsia="Inherited East"/></w:rPr></w:rPrDefault></w:docDefaults></w:styles>
                <m:oMath xmlns:m="{MATH_NS}" xmlns:w="{WORD_NS}" xmlns:a="{WORD_NS}" xmlns:z="{WORD_NS}"><m:r><m:rPr><m:nor/></m:rPr><w:rPr><w:rFonts {direct}/></w:rPr><m:t>A中</m:t></m:r></m:oMath>"#
            );
            let parsed = crate::omml::parse(&xml).unwrap();
            let slots = crate::omml::run_fonts(run(&parsed));
            assert_eq!(slots.family('A'), Some("Minor Latin"));
            assert_eq!(slots.family('中'), Some("Inherited East"));
        }
        let xml = format!(
            r#"{scheme}<w:styles xmlns:w="{WORD_NS}"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:asciiTheme="minorHAnsi"/></w:rPr></w:rPrDefault></w:docDefaults></w:styles><m:oMath xmlns:m="{MATH_NS}" xmlns:w="{WORD_NS}"><m:r><w:rPr><w:rFonts w:ascii="Higher Direct"/></w:rPr><m:t>A</m:t></m:r></m:oMath>"#
        );
        let parsed = crate::omml::parse(&xml).unwrap();
        assert_eq!(
            crate::omml::run_fonts(run(&parsed)).family('A'),
            Some("Higher Direct")
        );
    }

    #[test]
    fn math_fonts_ambiguous_invalid_or_nested_context_fails_explicitly() {
        for changed in [
            theme().replace(
                "</a:majorFont>",
                "<a:latin typeface=\"Duplicate\"/></a:majorFont>",
            ),
            theme().replace(
                "</a:majorFont>",
                "<a:font script=\"jpan\" typeface=\"Duplicate\"/></a:majorFont>",
            ),
            theme().replace("script=\"Hans\"", "script=\"NotAScript\""),
            theme().replace(" typeface=\"Major Latin\"", ""),
        ] {
            assert!(ThemeFonts::from_parts(Some(&changed), None).is_err());
        }
        for attributes in [r#"q:asciiTheme="majorWrong""#, r#"q:hint="incorrect""#] {
            let xml = format!(
                r#"<m:oMath xmlns:m="{MATH_NS}"><m:r><w:rPr xmlns:w="{WORD_NS}">{}</w:rPr><m:t>x</m:t></m:r></m:oMath>"#,
                rfonts(attributes).xml()
            );
            assert!(ThemeFonts::from_context(&math_document::parse_xml(&xml).unwrap()).is_err());
        }
        for source in [
            "frac(drawing.fontScheme(), x)",
            "row(word.themeFontLang(), word.themeFontLang(), office.oMath(x))",
        ] {
            assert!(crate::math_dsl::compile(source, false).is_err(), "{source}");
        }
    }

    #[test]
    fn math_fonts_word_character_slots_cover_hint_language_and_complex_overrides() {
        let plain = crate::omml::RunFonts {
            ascii: Some("ASCII"),
            high_ansi: Some("High"),
            east_asian: Some("East"),
            complex: Some("Complex"),
            ..Default::default()
        };
        for c in [
            'A', '\u{0590}', '\u{06FF}', '\u{0780}', '\u{FB1D}', '\u{FEFE}',
        ] {
            assert_eq!(plain.family(c), Some("ASCII"));
        }
        for c in ['α', 'é', '\u{1E00}', '\u{31A0}', '\u{4DC0}', '\u{FEFF}'] {
            assert_eq!(plain.family(c), Some("High"));
        }
        for c in [
            '中',
            '\u{1100}',
            '\u{3000}',
            '\u{3200}',
            '\u{4DBF}',
            '\u{FE30}',
            '\u{FFEF}',
            '\u{20000}',
        ] {
            assert_eq!(plain.family(c), Some("East"));
        }
        let hinted = crate::omml::RunFonts {
            east_hint: true,
            ..plain
        };
        assert_eq!(hinted.family('α'), Some("East"));
        assert_eq!(hinted.family('é'), Some("High"));
        assert_eq!(
            crate::omml::RunFonts {
                chinese: true,
                ..hinted
            }
            .family('é'),
            Some("East")
        );
        assert_eq!(
            crate::omml::RunFonts {
                complex_run: true,
                ..plain
            }
            .family('A'),
            Some("Complex")
        );
        let special = crate::omml::RunFonts {
            ascii: Some("Same"),
            high_ansi: Some("Same"),
            east_asian: Some("Times New Roman"),
            ..plain
        };
        assert_eq!(special.family('中'), Some("Same"));
        let xml = format!(
            r#"<m:oMath xmlns:m="{MATH_NS}" xmlns:w="{WORD_NS}" xmlns:f="urn:foreign"><m:r><w:rPr><f:rFonts f:ascii="Foreign"/><w:rFonts w:ascii="ASCII" w:hAnsi="High" w:eastAsia="East" w:cs="Complex" w:hint="eastAsia"/><w:lang w:eastAsia="ZH-cn"/><w:cs w:val="false"/><w:rtl w:val="0"/></w:rPr><m:t>é</m:t></m:r></m:oMath>"#
        );
        let parsed = crate::omml::parse(&xml).unwrap();
        assert_eq!(
            crate::omml::run_fonts(run(&parsed)).family('é'),
            Some("East")
        );
        let parsed = crate::omml::parse(&xml.replace("w:val=\"0\"", "w:val=\"1\"")).unwrap();
        assert_eq!(
            crate::omml::run_fonts(run(&parsed)).family('é'),
            Some("Complex")
        );
    }

    #[test]
    fn math_fonts_missing_family_and_glyph_are_reported_without_reading_host_fonts() {
        let xml = format!(
            r#"<m:oMath xmlns:m="{MATH_NS}" xmlns:w="{WORD_NS}"><m:r><m:rPr><m:nor/></m:rPr><w:rPr><w:rFonts w:ascii="Unavailable Fixture Font"/></w:rPr><m:t>A&#x10FFFF;</m:t></m:r></m:oMath>"#
        );
        let parsed = crate::omml::parse(&xml).unwrap();
        let warnings = crate::math_layout::font_warnings(&parsed);
        assert!(warnings.iter().any(|w| w["code"] == "font_fallback"
            && w["font"] == "Unavailable Fixture Font"
            && w["actual"] == "STIX Two Math"));
        assert!(warnings.iter().any(|w| w["code"] == "glyph_missing"
            && w["chars"]
                .as_str()
                .is_some_and(|s| s.contains('\u{10FFFF}'))));
        assert!(!crate::math_layout::needs_managed_fonts(&parsed));
        let mathml=crate::omml::to_mathml(&parsed,false,"");
        assert!(mathml.contains("font-family:'STIX Two Math'"));
        assert!(!mathml.contains("Unavailable Fixture Font"));
    }
}
#[derive(Default)]
pub(crate) struct ThemeFonts {
    scheme: Option<XmlNode>,
    languages: Option<XmlNode>,
}
impl ThemeFonts {
    pub(crate) fn from_parts(theme: Option<&str>, settings: Option<&str>) -> Result<Self> {
        let scheme = if let Some(source) = theme {
            let root = math_document::parse_xml(source)?;
            let t = single(root.children())?
                .filter(|n| drawing(n, "theme"))
                .ok_or("math_invalid_xml")?;
            let elements = single(t.children().filter(|n| drawing(n, "themeElements")))?
                .ok_or("math_invalid_xml")?;
            single(elements.children().filter(|n| drawing(n, "fontScheme")))?.cloned()
        } else {
            None
        };
        let languages = if let Some(source) = settings {
            let root = math_document::parse_xml(source)?;
            let s = single(root.children())?
                .filter(|n| word(n, "settings"))
                .ok_or("math_invalid_xml")?;
            single(s.children().filter(|n| word(n, "themeFontLang")))?.cloned()
        } else {
            None
        };
        Self::new(scheme, languages)
    }
    pub(crate) fn from_context(root: &XmlNode) -> Result<Self> {
        fn placement(n: &XmlNode, depth: usize) -> Result<()> {
            if depth > 1 && is_context(n) {
                return Err("math_invalid_property");
            }
            if word(n, "rFonts") {
                for key in ["asciiTheme", "hAnsiTheme", "eastAsiaTheme", "cstheme"] {
                    if let Some(token) = attr(n, key) {
                        theme_token(token)?;
                    }
                }
                if attr(n, "hint").is_some_and(|v| !matches!(v, "default" | "eastAsia" | "cs")) {
                    return Err("math_invalid_property");
                }
            }
            for c in n.children() {
                placement(c, depth + 1)?;
            }
            Ok(())
        }
        placement(root, 0)?;
        Self::new(
            single(root.children().filter(|n| drawing(n, "fontScheme")))?.cloned(),
            single(root.children().filter(|n| word(n, "themeFontLang")))?.cloned(),
        )
    }
    fn new(mut scheme: Option<XmlNode>, mut languages: Option<XmlNode>) -> Result<Self> {
        if let Some(s) = &mut scheme {
            let mut families = BTreeSet::new();
            for f in s.children().filter(|n| n.namespace == DRAWING_NS) {
                if matches!(f.local(), "majorFont" | "minorFont") {
                    if !families.insert(f.local()) {
                        return Err("math_invalid_property");
                    }
                    let mut regional = BTreeSet::new();
                    let mut supplemental = BTreeSet::new();
                    for c in f.children().filter(|n| n.namespace == DRAWING_NS) {
                        if matches!(c.local(), "latin" | "ea" | "cs") && !regional.insert(c.local())
                        {
                            return Err("math_invalid_property");
                        }
                        if c.local() == "font" {
                            let tag = c.attributes.get("script").ok_or("math_invalid_property")?;
                            if tag.len() != 4
                                || !tag.bytes().all(|b| b.is_ascii_alphabetic())
                                || !supplemental.insert(tag.to_ascii_lowercase())
                            {
                                return Err("math_invalid_property");
                            }
                        }
                        if matches!(c.local(), "latin" | "ea" | "cs" | "font")
                            && !c.attributes.contains_key("typeface")
                        {
                            return Err("math_invalid_property");
                        }
                    }
                }
            }
            fn clean(n: &mut XmlNode) {
                n.content
                    .retain(|p| !matches!(p,XmlPart::Text(t) if t.trim().is_empty()));
                for p in &mut n.content {
                    if let XmlPart::Element(c) = p {
                        clean(c);
                    }
                }
            }
            clean(s);
            crate::math_word::bind_used_namespaces(s);
        }
        if let Some(l) = &mut languages {
            crate::math_word::bind_used_namespaces(l);
        }
        Ok(Self { scheme, languages })
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.scheme.is_none() && self.languages.is_none()
    }
    pub(crate) fn capture(&self, nodes: &[&XmlNode]) -> Result<Vec<XmlNode>> {
        fn refs(n: &XmlNode, out: &mut BTreeSet<String>) -> Result<()> {
            if word(n, "rFonts") {
                for key in ["asciiTheme", "hAnsiTheme", "eastAsiaTheme", "cstheme"] {
                    if let Some(token) = attr(n, key) {
                        out.insert(theme_token(token)?.0.into());
                    }
                }
            }
            for c in n.children() {
                refs(c, out)?;
            }
            Ok(())
        }
        let mut used = BTreeSet::new();
        for n in nodes {
            refs(n, &mut used)?;
        }
        if used.is_empty() {
            return Ok(vec![]);
        }
        let mut out = vec![];
        if let Some(s) = &self.scheme {
            let mut selected = s.clone();
            selected.content.retain(|p|matches!(p,XmlPart::Element(n) if n.namespace==DRAWING_NS&&used.contains(n.local())));
            out.push(selected);
        }
        if let Some(l) = &self.languages {
            out.push(l.clone());
        }
        Ok(out)
    }
    pub(crate) fn resolve(&self, fonts: &mut XmlNode) -> Result<()> {
        for (key, target) in [
            ("asciiTheme", "ascii"),
            ("hAnsiTheme", "hAnsi"),
            ("eastAsiaTheme", "eastAsia"),
            ("cstheme", "cs"),
        ] {
            let Some(token) = attr(fonts, key) else {
                continue;
            };
            let (family, region, language) = theme_token(token)?;
            let Some(scheme) = &self.scheme else {
                continue;
            };
            let f = single(scheme.children().filter(|n| drawing(n, family)))?
                .ok_or("math_invalid_property")?;
            let preferred = self
                .languages
                .as_ref()
                .and_then(|l| attr(l, language))
                .and_then(script)
                .and_then(|tag| {
                    f.children().find(|n| {
                        drawing(n, "font")
                            && n.attributes
                                .get("script")
                                .is_some_and(|v| v.eq_ignore_ascii_case(tag))
                    })
                });
            let face = preferred
                .or_else(|| f.children().find(|n| drawing(n, region)))
                .and_then(|n| n.attributes.get("typeface"))
                .filter(|s| !s.is_empty())
                .cloned();
            // An empty regional font has no family to materialize. Retain the
            // producer's fallback and token instead of inventing a typeface.
            let Some(face) = face else {
                continue;
            };
            let keys = fonts
                .attributes
                .keys()
                .filter(|k| {
                    fonts
                        .attribute_namespaces
                        .get(*k)
                        .is_some_and(|uri| uri == WORD_NS)
                        && matches!(k.rsplit(':').next(),Some(v) if v==key||v==target)
                })
                .cloned()
                .collect::<Vec<_>>();
            for k in keys {
                fonts.attributes.remove(&k);
                fonts.attribute_namespaces.remove(&k);
            }
            let mut prefix = "readmdFont".to_string();
            while fonts
                .attributes
                .get(&format!("xmlns:{prefix}"))
                .is_some_and(|uri| uri != WORD_NS)
            {
                prefix.push('_');
            }
            fonts
                .attributes
                .insert(format!("xmlns:{prefix}"), WORD_NS.into());
            let key = format!("{prefix}:{target}");
            fonts.attributes.insert(key.clone(), face);
            fonts.attribute_namespaces.insert(key, WORD_NS.into());
        }
        Ok(())
    }
}
