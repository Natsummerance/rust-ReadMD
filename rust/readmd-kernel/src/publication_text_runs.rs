//! Size-axis itemization for author text. This is not a bidi or glyph shaper.
//! Word's sz/szCs axes are independent; cs/rtl can force the complex axis even
//! for Latin. Unicode script properties come from the already locked regex
//! tables. Common/inherited characters follow their surrounding text rather
//! than treating every non-ASCII character as complex script.
use crate::publication_inline_text::{ResolvedTextStyle, SizeOrigin, BUDGET, INVALID, UNSUPPORTED};
use regex::RegexSet;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeKind { Western, Complex }

#[derive(Debug, Clone, PartialEq)]
pub struct TextSegment {
    /// UTF-8 byte bounds in the unchanged input, never character indices.
    pub from: usize,
    pub to: usize,
    pub size: f64,
    pub origin: SizeOrigin,
    pub kind: SizeKind,
    /// Consumers without shaping must reject this text before creating output.
    pub requires_shaping: bool,
    /// Directional evidence only. A complex script is not necessarily RTL.
    pub rtl: bool,
}

const MAX_SEGMENTS: usize = 65_536;
/// Native values remain round-trippable in source/DOCX; raster/vector layout
/// has a separate physical safety bound and never silently clamps a value.
pub const MAX_RENDER_PT: f64 = 4096.;
fn properties() -> &'static RegexSet {
    static SET: OnceLock<RegexSet> = OnceLock::new();
    SET.get_or_init(|| RegexSet::new([
        // Western and East Asian script families use sz. The remaining strong
        // script families use the complex axis, as in office script classes.
        r"^[\p{Latin}\p{Greek}\p{Cyrillic}\p{Armenian}\p{Georgian}\p{Cherokee}\p{Coptic}\p{Gothic}\p{Ogham}\p{Old_Italic}\p{Runic}\p{Canadian_Aboriginal}\p{Braille}\p{Cypriot}\p{Osmanya}\p{Shavian}\p{Glagolitic}\p{Old_Hungarian}\p{Han}\p{Hangul}\p{Hiragana}\p{Katakana}\p{Bopomofo}\p{Yi}\p{Tangut}\p{Khitan_Small_Script}]$",
        r"^[\p{Common}\p{Inherited}\p{Cn}\p{Co}]$",
        r"^\p{M}$",
        r"^[\p{Hebrew}\p{Arabic}\p{Syriac}\p{Thaana}\p{Nko}\p{Samaritan}\p{Mandaic}\p{Adlam}\p{Hanifi_Rohingya}\p{Phoenician}\p{Imperial_Aramaic}\p{Avestan}\p{Inscriptional_Parthian}\p{Inscriptional_Pahlavi}\p{Psalter_Pahlavi}\p{Manichaean}\p{Nabataean}\p{Palmyrene}\p{Hatran}\p{Old_North_Arabian}\p{Old_South_Arabian}\p{Old_Hungarian}\p{Yezidi}\p{Sogdian}\p{Old_Sogdian}\p{Elymaic}\p{Chorasmian}]$",
    ]).expect("locked Unicode script properties"))
}

#[derive(Clone, Copy)]
struct Class { kind: Option<SizeKind>, shaping: bool, rtl: bool }
fn class(c: char) -> Class {
    if c.is_ascii() { return Class { kind: c.is_ascii_alphabetic().then_some(SizeKind::Western), shaping: false, rtl: false }; }
    let mut buffer = [0; 4];
    let found = properties().matches(c.encode_utf8(&mut buffer));
    let mark = found.matched(2);
    let rtl = found.matched(3);
    let controls = matches!(c, '\u{200c}' | '\u{200d}' | '\u{200e}' | '\u{200f}' | '\u{061c}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{fe00}'..='\u{fe0f}' | '\u{e0100}'..='\u{e01ef}' | '\u{1f3fb}'..='\u{1f3ff}');
    let kind = if found.matched(0) { Some(SizeKind::Western) }
        else if found.matched(1) || mark || controls { None }
        else { Some(SizeKind::Complex) };
    let jamo = matches!(c, '\u{1100}'..='\u{11ff}' | '\u{a960}'..='\u{a97f}' | '\u{d7b0}'..='\u{d7ff}');
    Class { kind, shaping: mark || controls || jamo || kind == Some(SizeKind::Complex) || rtl, rtl }
}

/// Exact, bounded logical-text segmentation. Marks and join controls remain
/// with their base. Neutral leading text follows the first strong character;
/// neutral text thereafter follows its preceding character. No normalization,
/// source rewrite, visual reordering or unsupported shaping is performed here.
pub fn split(text: &str, style: &ResolvedTextStyle) -> Result<Vec<TextSegment>, &'static str> {
    if ![style.size, style.size_cs].into_iter().all(|v| v.is_finite() && v > 0. && v <= i32::MAX as f64 / 2.) { return Err(INVALID); }
    if [style.size,style.size_cs].into_iter().any(|v|v>MAX_RENDER_PT){return Err(UNSUPPORTED);}
    if text.len() > crate::document_transaction::MAX_DOCUMENT_BYTES { return Err(BUDGET); }
    let force = style.cs == Some(true) || style.rtl == Some(true);
    let initial = if force { SizeKind::Complex } else { text.chars().find_map(|c| class(c).kind).unwrap_or(SizeKind::Western) };
    let mut prior = initial;
    let mut out: Vec<TextSegment> = Vec::new();
    for (from, c) in text.char_indices() {
        let info = class(c);
        let kind = if force { SizeKind::Complex } else { info.kind.unwrap_or(prior) };
        prior = kind;
        let (size, origin) = match kind { SizeKind::Western => (style.size, style.size_origin), SizeKind::Complex => (style.size_cs, style.size_cs_origin) };
        let rtl = style.rtl == Some(true) || info.rtl;
        let to = from + c.len_utf8();
        // Preserve a complete combining/joining cluster in one segment. Its
        // safety flag is accumulated, so an incapable consumer cannot render
        // just the harmless base and silently drop its combining character.
        if let Some(previous) = out.last_mut().filter(|p| p.kind == kind && p.size == size && p.origin == origin) {
            previous.to = to;
            previous.requires_shaping |= info.shaping;
            previous.rtl |= rtl;
        } else {
            if out.len() >= MAX_SEGMENTS { return Err(BUDGET); }
            out.push(TextSegment { from, to, size, origin, kind, requires_shaping: info.shaping, rtl });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_axes_preserve_multilingual_text_and_byte_ranges() {
        let source = "A中文 α Ж العربية עברית हिन्दी ไทย Z";
        let style = ResolvedTextStyle::base(30., 10.);
        let segments = split(source, &style).unwrap();
        assert_eq!(segments.iter().map(|s| &source[s.from..s.to]).collect::<String>(), source);
        for s in &segments { assert_eq!(s.size, if s.kind == SizeKind::Complex { 10. } else { 30. }); }
        assert!(segments.iter().any(|s| source[s.from..s.to].contains("中文") && s.kind == SizeKind::Western));
        assert!(segments.iter().any(|s| source[s.from..s.to].contains("العربية") && s.rtl && s.requires_shaping));
        assert!(segments.iter().any(|s| source[s.from..s.to].contains("ไทย") && s.kind == SizeKind::Complex));
    }
    #[test]
    fn forced_complex_latin_uses_cs_origin_but_is_not_fake_rtl_or_shaping() {
        let mut style = ResolvedTextStyle::base(30., 10.);
        style.size_origin = SizeOrigin::Author;
        style.size_cs_origin = SizeOrigin::BlockBase;
        style.cs = Some(true);
        let segments = split("Latin 123", &style).unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!((segments[0].size, segments[0].origin), (10., SizeOrigin::BlockBase));
        assert!(!segments[0].rtl && !segments[0].requires_shaping);
        style.rtl = Some(true);
        assert!(split("Latin", &style).unwrap()[0].rtl);
    }
    #[test]
    fn marks_joiners_variations_and_jamo_require_real_shaping() {
        for text in ["e\u{301}", "ع\u{200d}ب", "👩\u{200d}💻", "\u{1100}\u{1161}", "✋\u{1f3fb}"] {
            let segments = split(text, &ResolvedTextStyle::base(12., 9.)).unwrap();
            assert!(segments.iter().any(|s| s.requires_shaping), "{text}");
            assert_eq!(segments.iter().map(|s| &text[s.from..s.to]).collect::<String>(), text);
        }
        let segments = split("\u{301}ह", &ResolvedTextStyle::base(12., 9.)).unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].kind, SizeKind::Complex);
    }
    #[test]
    fn validation_is_bounded_and_does_not_clamp_or_normalize() {
        for size in [0., -1., f64::NAN, f64::INFINITY, i32::MAX as f64] {
            assert_eq!(split("x", &ResolvedTextStyle::base(size, 12.)), Err(INVALID));
        }
        assert!(split("", &ResolvedTextStyle::base(12., 12.)).unwrap().is_empty());
        assert_eq!(split("x", &ResolvedTextStyle::base(4096.5, 12.)),Err(UNSUPPORTED));
        let source = "Aع".repeat(MAX_SEGMENTS / 2 + 1);
        assert_eq!(split(&source, &ResolvedTextStyle::base(12., 10.)), Err(BUDGET));
    }
}
