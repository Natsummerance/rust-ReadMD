//! Embedded TrueType fonts for the native PDF writer.
//!
//! Ordinary regular/bold proportional text can use the selected managed face
//! (regular text also has a bundled core fallback). Non-WinAnsi text keeps its
//! existing per-glyph ladder. The private TrueType font is *subset* into the PDF: only the
//! glyphs a document uses are copied (composite components included), renumbered
//! from 0, and written as `/CIDFontType2` + `/Identity-H` with an exact `/W`
//! array and a `/ToUnicode` CMap, so the PDF looks the same on every machine
//! and copy/search return the original text.
//!
//! Font files are loaded lazily, including a selected CJK face when ordinary
//! Latin text requests that family. Only
//! glyf-outlined fonts that permit embedding and subsetting are used; when none
//! is available the caller falls back to the reader-supplied `STSong-Light`
//! and reports it.
//!
//! State lives in a thread-local context bracketed by [`begin`] / [`finish`],
//! because measurement happens deep inside free functions of the layout code.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use ttf_parser::{GlyphId, Tag};

// Original DejaVu 2.37 release bytes, accompanied by the complete upstream
// license in assets/fonts. This is a portable core resource, never a host font.
const CORE_FALLBACK_BYTES: &[u8] = crate::publication_fonts::CORE_TEXT_FALLBACK;
// CIDToGIDMap Identity requires each Unicode alias to have a distinct subset
// glyph. This includes composite closure, not only directly drawn glyphs.
const MAX_SUBSET_GLYPHS: usize = 65_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuiltinFont {
    Math,
    Fallback,
}

/// Where a candidate font lives (`index` = face inside a `.ttc`).
#[derive(Debug, Clone, PartialEq)]
pub struct FontSource {
    pub path: PathBuf,
    pub index: u32,
    builtin: Option<BuiltinFont>,
}

impl FontSource {
    fn new(p: impl Into<PathBuf>, index: u32) -> Self {
        FontSource { path: p.into(), index, builtin:None }
    }
}

/// Regular and bold files from the active profile's verified font pack.
pub fn cjk_candidates(family: &str) -> (Vec<FontSource>, Vec<FontSource>) {
    let Some(resources)=crate::publication_fonts::current()else{return (vec![],vec![]);};
    let regular=if crate::publication_fonts::family(family)=="ReadMD Serif SC"{resources.serif.clone()}else{resources.sans.clone()};
    let bold=regular.with_file_name(regular.file_name().unwrap_or_default().to_string_lossy().replace("-Regular","-Bold"));
    (vec![FontSource::new(regular,0)],vec![FontSource::new(bold,0)])
}

/// Core symbols require neither an installed font pack nor a host font.
pub fn symbol_candidates() -> Vec<FontSource> {
    vec![FontSource{path:PathBuf::new(),index:0,builtin:Some(BuiltinFont::Math)}]
}

fn core_fallback_candidates() -> Vec<FontSource> {
    vec![FontSource{path:PathBuf::new(),index:0,builtin:Some(BuiltinFont::Fallback)}]
}

// ------------------------------------------------------------------ slots

struct Loaded {
    data: Arc<Vec<u8>>,
    index: u32,
    upem: u16,
    ascent: i16,
    descent: i16,
    cap: i16,
    bbox: (i16, i16, i16, i16),
    ps_name: String,
    cmap: HashMap<char, Option<(u16, u16)>>, // char -> (old gid, advance)
    // First subset glyph for each original: composite references use this map.
    new_of_old: HashMap<u16, u16>,
    // Different Unicode scalars may share an original glyph. Their CIDs must
    // remain distinct so ToUnicode works without relying on ActualText.
    cid_by_text: HashMap<(u16, String), u16>,
    allocation_failed: bool,
    olds: Vec<u16>,
    uni: Vec<String>,
}

enum SlotState {
    Pending(Vec<FontSource>),
    Ready(Box<Loaded>),
    Unavailable,
}

struct Slot {
    state: SlotState,
}

impl Slot {
    fn new(cands: Vec<FontSource>) -> Slot {
        Slot { state: if cands.is_empty() { SlotState::Unavailable } else { SlotState::Pending(cands) } }
    }

    fn loaded(&mut self) -> Option<&mut Loaded> {
        if let SlotState::Pending(c) = &self.state {
            let cands = c.clone();
            self.state = cands.iter().find_map(load).map(|l| SlotState::Ready(Box::new(l))).unwrap_or(SlotState::Unavailable);
        }
        match &mut self.state {
            SlotState::Ready(l) => Some(l),
            _ => None,
        }
    }
}

fn load(src: &FontSource) -> Option<Loaded> {
    let data = match src.builtin {
        Some(BuiltinFont::Math) => crate::publication_fonts::MATH.to_vec(),
        Some(BuiltinFont::Fallback) => CORE_FALLBACK_BYTES.to_vec(),
        None => std::fs::read(&src.path).ok()?,
    };
    let data = Arc::new(data);
    let face = ttf_parser::Face::parse(&data, src.index).ok()?;
    let raw = face.raw_face();
    if raw.table(Tag::from_bytes(b"glyf")).is_none() || raw.table(Tag::from_bytes(b"loca")).is_none() {
        return None;
    }
    if matches!(face.permissions(), Some(ttf_parser::Permissions::Restricted)) || !face.is_subsetting_allowed() {
        return None;
    }
    let ps_name = face
        .names()
        .into_iter()
        .filter(|n| n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME)
        .find_map(|n| n.to_string())
        .unwrap_or_else(|| "ReadMDFont".into());
    let ps_name: String = ps_name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(48).collect();
    let bb = face.global_bounding_box();
    let upem = face.units_per_em();
    let mut l = Loaded {
        index: src.index,
        upem,
        ascent: face.ascender(),
        descent: face.descender(),
        cap: face.capital_height().unwrap_or(face.ascender()),
        bbox: (bb.x_min, bb.y_min, bb.x_max, bb.y_max),
        ps_name: if ps_name.is_empty() { "ReadMDFont".into() } else { ps_name },
        cmap: HashMap::new(),
        new_of_old: HashMap::new(),
        cid_by_text: HashMap::new(),
        allocation_failed: false,
        olds: Vec::new(),
        uni: Vec::new(),
        data: data.clone(),
    };
    // .notdef keeps id 0.
    l.new_of_old.insert(0, 0);
    l.olds.push(0);
    l.uni.push(String::new());
    Some(l)
}

impl Loaded {
    fn lookup(&mut self, c: char) -> Option<(u16, u16)> {
        if let Some(hit) = self.cmap.get(&c) {
            return *hit;
        }
        let face = ttf_parser::Face::parse(&self.data, self.index).ok();
        let got = face.and_then(|f| {
            let g = f.glyph_index(c)?;
            if g.0 == 0 {
                return None;
            }
            Some((g.0, f.glyph_hor_advance(g).unwrap_or(self.upem)))
        });
        self.cmap.insert(c, got);
        got
    }

    fn use_gid(&mut self, old: u16, text: &str) -> Option<u16> {
        if !text.is_empty() {
            if let Some(&cid) = self.cid_by_text.get(&(old, text.to_string())) {
                return Some(cid);
            }
        }
        if let Some(&cid) = self.new_of_old.get(&old) {
            if text.is_empty() { return Some(cid); }
            // A component can be registered before its first visible use.
            if self.uni.get(cid as usize)?.is_empty() {
                self.uni[cid as usize] = text.to_string();
                self.cid_by_text.insert((old, text.to_string()), cid);
                return Some(cid);
            }
        }
        if self.olds.len() >= MAX_SUBSET_GLYPHS || self.olds.len() != self.uni.len() {
            self.allocation_failed = true;
            return None;
        }
        let cid = u16::try_from(self.olds.len()).ok()?;
        // Never change the canonical target when allocating a Unicode alias.
        self.new_of_old.entry(old).or_insert(cid);
        self.olds.push(old);
        self.uni.push(text.to_string());
        if !text.is_empty() { self.cid_by_text.insert((old, text.to_string()), cid); }
        Some(cid)
    }

    fn scale(&self, v: f64) -> f64 {
        v * 1000.0 / self.upem.max(1) as f64
    }
}

// ------------------------------------------------------------------ context

/// Faces handed to the PDF layer: `Face::Embedded(id)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceInfo {
    pub slot: usize,
    /// Drawn with fill+stroke (no real bold file on this machine).
    pub fake_bold: bool,
}

struct Ctx {
    slots: Vec<Slot>, // 0 = regular, 1 = bold, 2 = math symbols, 3 = core fallback
    faces: Vec<FaceInfo>,
    missing: BTreeSet<char>,
    fallback_used: bool,
    unembedded_proportional: bool,
    subset_allocation_failed: bool,
}

const REGULAR: usize = 0;
const BOLD: usize = 1;
const SYMBOL: usize = 2;
const CORE_FALLBACK: usize = 3;

thread_local! {
    static CTX: RefCell<Option<Ctx>> = const { RefCell::new(None) };
}

/// Start an export using `family` (the style's `typography.font`).
pub fn begin(family: &str) {
    let (reg, bold) = cjk_candidates(family);
    begin_with_fallback(reg, bold, symbol_candidates(), core_fallback_candidates());
}

/// [`begin`] with explicit candidates (tests, custom font settings).
pub fn begin_with(regular: Vec<FontSource>, bold: Vec<FontSource>, symbols: Vec<FontSource>) {
    begin_with_fallback(regular, bold, symbols, Vec::new());
}

fn begin_with_fallback(regular: Vec<FontSource>, bold: Vec<FontSource>, symbols: Vec<FontSource>, fallback: Vec<FontSource>) {
    CTX.with(|c| {
        *c.borrow_mut() = Some(Ctx {
            slots: vec![Slot::new(regular), Slot::new(bold), Slot::new(symbols), Slot::new(fallback)],
            faces: Vec::new(),
            missing: BTreeSet::new(),
            fallback_used: false,
            unembedded_proportional: false,
            subset_allocation_failed: false,
        })
    });
}

fn with_ctx<R>(f: impl FnOnce(&mut Ctx) -> R) -> Option<R> {
    CTX.with(|c| c.borrow_mut().as_mut().map(f))
}

impl Ctx {
    /// Slot order to try for one character.
    fn order(&self, bold: bool) -> &'static [(usize, bool)] {
        if bold {
            &[(BOLD, false), (REGULAR, true), (SYMBOL, false), (CORE_FALLBACK, false)]
        } else {
            &[(REGULAR, false), (SYMBOL, false), (CORE_FALLBACK, false)]
        }
    }

    fn resolve(&mut self, c: char, bold: bool) -> Option<(usize, bool, u16, f64)> {
        for &(slot, fake) in self.order(bold) {
            if let Some(l) = self.slots[slot].loaded() {
                if let Some((gid, adv)) = l.lookup(c) {
                    let fake = fake || (bold && (slot == SYMBOL || slot == CORE_FALLBACK));
                    return Some((slot, fake, gid, l.scale(adv as f64)));
                }
            }
        }
        None
    }

    /// WinAnsi R/B proportional text. Never substitute a regular program for
    /// bold: the existing PDF serializer does not implement fake_bold. A miss
    /// stays on its original Base14 face with an explicit substitution warning.
    fn resolve_proportional(&mut self, c: char, bold: bool) -> Option<(usize, u16, f64)> {
        let order: &[usize] = if bold { &[BOLD] } else { &[REGULAR, CORE_FALLBACK] };
        for &slot in order {
            if let Some(font) = self.slots[slot].loaded() {
                if let Some((gid, advance)) = font.lookup(c) {
                    // /W is emitted in integer 1/1000-em units. Layout and
                    // adjacent text matrices must use that exact displacement.
                    return Some((slot, gid, font.scale(advance as f64).round()));
                }
            }
        }
        None
    }

    fn face_id(&mut self, slot: usize, fake_bold: bool) -> u8 {
        let info = FaceInfo { slot, fake_bold };
        match self.faces.iter().position(|f| *f == info) {
            Some(i) => i as u8,
            None => {
                self.faces.push(info);
                (self.faces.len() - 1) as u8
            }
        }
    }
}

/// Advance of `c` at `size`, or `None` when no embedded font has it (or no
/// export is in progress).
pub fn advance(c: char, bold: bool, size: f64) -> Option<f64> {
    with_ctx(|ctx| ctx.resolve(c, bold).map(|(_, _, _, w)| w * size / 1000.0)).flatten()
}

/// Real embedded outline bounds, in physical points above/below baseline.
/// Resolution shares encode/advance's verified face and does not register or
/// substitute glyphs. Missing glyphs retain the normal writer error path.
pub(crate) fn vertical_bounds(c:char,bold:bool,size:f64)->Option<(f64,f64)>{
    with_ctx(|ctx|{
        let (slot,_,gid,_)=ctx.resolve(c,bold)?;
        let font=ctx.slots[slot].loaded()?;
        let face=ttf_parser::Face::parse(&font.data,font.index).ok()?;
        let bounds=face.glyph_bounding_box(ttf_parser::GlyphId(gid))?;
        let scale=size/font.upem as f64;
        Some((bounds.y_max as f64*scale,-(bounds.y_min as f64)*scale))
    }).flatten()
}

/// Advance of a WinAnsi proportional glyph in the exact selected program.
/// None preserves the original Base14 metric/face; no context is synthesized.
pub(crate) fn proportional_advance(c: char, bold: bool, size: f64) -> Option<f64> {
    with_ctx(|ctx| ctx.resolve_proportional(c, bold)
        .map(|(_, _, advance)| advance * size / 1000.)).flatten()
}

/// Font strut and real outline from the same resolver used by /W and encoding.
/// Space has its real font strut, without claiming it has a visible outline.
pub(crate) fn proportional_vertical_bounds(c: char, bold: bool, size: f64) -> Option<(f64, f64)> {
    with_ctx(|ctx| {
        let (slot, gid, _) = ctx.resolve_proportional(c, bold)?;
        let font = ctx.slots[slot].loaded()?;
        let face = ttf_parser::Face::parse(&font.data, font.index).ok()?;
        let scale = size / font.upem as f64;
        let mut upper = (font.ascent as f64 * scale).max(0.);
        let mut lower = (-font.descent as f64 * scale).max(0.);
        if let Some(ink) = face.glyph_bounding_box(GlyphId(gid)) {
            upper = upper.max(ink.y_max as f64 * scale);
            lower = lower.max(-ink.y_min as f64 * scale);
        }
        Some((upper, lower))
    }).flatten()
}

/// A missing owned proportional face remains explicit, so its caller can keep
/// the original Base14 program/metric rather than drawing a false bold face.
pub(crate) enum ProportionalPiece {
    Embedded(Piece),
    Base14(String),
}

/// Called only on WinAnsi regular/bold proportional runs. Non-WinAnsi, italic,
/// bold-italic and monospace retain their existing independent paths.
pub(crate) fn encode_proportional(text: &str, bold: bool, size: f64) -> Option<Vec<ProportionalPiece>> {
    with_ctx(|ctx| {
        let mut out = Vec::new();
        for c in text.chars() {
            if let Some((slot, gid, advance)) = ctx.resolve_proportional(c, bold) {
                let unicode = c.to_string();
                let cid = ctx.slots[slot].loaded().and_then(|font| font.use_gid(gid, &unicode));
                let Some(cid) = cid else {
                    // Nothing from this intermediate layout can be committed:
                    // finish() returns the recorded subset budget failure.
                    ctx.subset_allocation_failed = true;
                    continue;
                };
                let face = Some(ctx.face_id(slot, false));
                let bytes = cid.to_be_bytes();
                let advance = advance * size / 1000.;
                match out.last_mut() {
                    Some(ProportionalPiece::Embedded(last)) if last.face == face => {
                        last.text.push(c); last.bytes.extend_from_slice(&bytes); last.advance += advance;
                    }
                    _ => out.push(ProportionalPiece::Embedded(Piece {
                        face, text: unicode, bytes: bytes.to_vec(), advance,
                    })),
                }
            } else {
                if !c.is_whitespace() && !c.is_control() { ctx.unembedded_proportional = true; }
                match out.last_mut() {
                    Some(ProportionalPiece::Base14(last)) => last.push(c),
                    _ => out.push(ProportionalPiece::Base14(c.to_string())),
                }
            }
        }
        out
    })
}

/// One drawable piece of a non-WinAnsi run.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    /// `Some(face id)` for an embedded face; `None` = fall back to STSong-Light.
    pub face: Option<u8>,
    pub text: String,
    /// Content-stream bytes: 2-byte glyph ids, or UTF-16BE for the fallback.
    pub bytes: Vec<u8>,
    pub advance: f64,
}

/// Split `text` into embedded-font pieces and register the glyphs as used.
pub fn encode(text: &str, bold: bool, size: f64) -> Vec<Piece> {
    let got = with_ctx(|ctx| {
        let mut out: Vec<Piece> = Vec::new();
        for c in text.chars() {
            let (face, bytes, adv) = match ctx.resolve(c, bold) {
                Some((slot, fake, gid, w)) => {
                    let s = c.to_string();
                    let new = ctx.slots[slot].loaded().and_then(|font| font.use_gid(gid, &s));
                    let Some(new) = new else {
                        ctx.subset_allocation_failed = true;
                        continue;
                    };
                    (Some(ctx.face_id(slot, fake)), new.to_be_bytes().to_vec(), w * size / 1000.0)
                }
                None => {
                    if !c.is_whitespace() && !c.is_control() {
                        ctx.missing.insert(c);
                    }
                    ctx.fallback_used = true;
                    let mut b = Vec::new();
                    let mut buf = [0u16; 2];
                    for u in c.encode_utf16(&mut buf) {
                        b.extend_from_slice(&u.to_be_bytes());
                    }
                    (None, b, size)
                }
            };
            match out.last_mut() {
                Some(p) if p.face == face => {
                    p.text.push(c);
                    p.bytes.extend_from_slice(&bytes);
                    p.advance += adv;
                }
                _ => out.push(Piece { face, text: c.to_string(), bytes, advance: adv }),
            }
        }
        out
    });
    got.unwrap_or_else(|| {
        let mut b = Vec::new();
        for u in text.encode_utf16() {
            b.extend_from_slice(&u.to_be_bytes());
        }
        vec![Piece { face: None, text: text.to_string(), bytes: b, advance: text.chars().count() as f64 * size }]
    })
}

// ------------------------------------------------------------------ output

/// A finished subset, ready for the PDF writer.
#[derive(Clone)]
pub struct SubsetFont {
    pub base_name: String,
    /// The subset `.ttf` (uncompressed).
    pub file: Vec<u8>,
    /// Advance per glyph id, in 1/1000 em.
    pub widths: Vec<i64>,
    /// Text per glyph id for `/ToUnicode` (empty = no mapping).
    pub unicode: Vec<String>,
    pub ascent: i64,
    pub descent: i64,
    pub cap_height: i64,
    pub bbox: [i64; 4],
}

impl std::fmt::Debug for SubsetFont {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SubsetFont({}, {} glyphs, {} bytes)", self.base_name, self.widths.len(), self.file.len())
    }
}

#[derive(Debug, Clone, Default)]
pub struct EmbeddedSet {
    /// Per slot that produced glyphs (`None` for unused slots).
    pub fonts: Vec<Option<SubsetFont>>,
    pub faces: Vec<FaceInfo>,
}

/// End the export: build the subsets and return them with any warnings.
pub fn finish() -> Result<(EmbeddedSet, Vec<String>), &'static str> {
    let ctx = CTX.with(|c| c.borrow_mut().take());
    let Some(mut ctx) = ctx else { return Ok((EmbeddedSet::default(), Vec::new())) };
    if ctx.subset_allocation_failed { return Err("transaction_budget"); }
    let mut warns = Vec::new();
    if ctx.unembedded_proportional { warns.push("publication_font_substituted".into()); }
    let mut fonts = Vec::new();
    for slot in ctx.slots.iter_mut() {
        let sub = match &mut slot.state {
            SlotState::Ready(l) if l.allocation_failed => return Err("transaction_budget"),
            SlotState::Ready(l) if l.olds.len() > 1 => {
                let subset = subset(l);
                if l.allocation_failed { return Err("transaction_budget"); }
                Some(subset.ok_or(crate::api_codes::PUBLICATION_FONTS_UNAVAILABLE)?)
            },
            _ => None,
        };
        fonts.push(sub);
    }
    let regular_ok = matches!(ctx.slots[REGULAR].state, SlotState::Ready(_));
    if ctx.fallback_used && !regular_ok {
        warns.push("未找到可嵌入的 CJK 字体，非拉丁文字使用阅读器内置字体（STSong-Light）显示，其他电脑上可能不一致".to_string());
    } else if !ctx.missing.is_empty() {
        let list: String = ctx.missing.iter().take(24).collect();
        let more = if ctx.missing.len() > 24 { "…" } else { "" };
        warns.push(format!("嵌入字体缺少以下字符，已使用阅读器内置字体：{list}{more}"));
    }
    Ok((EmbeddedSet { fonts, faces: ctx.faces }, warns))
}

fn rd16(b: &[u8], o: usize) -> Option<u16> {
    b.get(o..o + 2).map(|s| u16::from_be_bytes([s[0], s[1]]))
}

fn rd32(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

/// Component glyph ids of a composite glyph and the byte offsets holding them.
fn components(g: &[u8]) -> Option<Vec<(usize, u16)>> {
    let mut out = Vec::new();
    if g.is_empty() { return Some(out); }
    if g.len() < 10 { return None; }
    if (rd16(g, 0)? as i16) >= 0 { return Some(out); }
    let mut pos = 10usize;
    loop {
        let flags = rd16(g, pos)?;
        let gid = rd16(g, pos + 2)?;
        out.push((pos + 2, gid));
        pos += 4 + if flags & 0x0001 != 0 { 4 } else { 2 };
        if flags & 0x0008 != 0 { pos += 2; }
        else if flags & 0x0040 != 0 { pos += 4; }
        else if flags & 0x0080 != 0 { pos += 8; }
        if pos > g.len() { return None; }
        if flags & 0x0020 == 0 { return Some(out); }
        // Preserve the existing per-glyph bound, but never accept a truncated
        // component list as a complete subset.
        if out.len() >= 64 { return None; }
    }
}

fn subset(l: &mut Loaded) -> Option<SubsetFont> {
    let data = l.data.clone();
    let face = ttf_parser::Face::parse(&data, l.index).ok()?;
    let raw = face.raw_face();
    let table = |t: &[u8; 4]| raw.table(Tag::from_bytes(t));
    let head = table(b"head")?;
    let loca = table(b"loca")?;
    let glyf = table(b"glyf")?;
    let hhea = table(b"hhea")?;
    let maxp = table(b"maxp")?;
    let long_loca = rd16(head, 50)? == 1;
    let n_orig = face.number_of_glyphs() as usize;
    let glyph_range = |gid: u16| -> Option<(usize, usize)> {
        let i = gid as usize;
        if i >= n_orig {
            return None;
        }
        let (a, b) = if long_loca {
            (rd32(loca, i * 4)? as usize, rd32(loca, i * 4 + 4)? as usize)
        } else {
            (rd16(loca, i * 2)? as usize * 2, rd16(loca, i * 2 + 2)? as usize * 2)
        };
        (a <= b && b <= glyf.len()).then_some((a, b))
    };

    // Pull in composite components (they get ids after the drawn glyphs).
    let mut i = 0usize;
    while i < l.olds.len() {
        let (a, b) = glyph_range(l.olds[i])?;
        for (_, comp) in components(&glyf[a..b])? {
            glyph_range(comp)?;
            l.use_gid(comp, "")?;
        }
        i += 1;
    }

    let n = l.olds.len();
    if n == 0 || n > MAX_SUBSET_GLYPHS || n != l.uni.len() { return None; }
    let n_u16 = u16::try_from(n).ok()?;
    let mut new_glyf: Vec<u8> = Vec::new();
    let mut new_loca: Vec<u8> = Vec::with_capacity((n + 1) * 4);
    let mut hmtx: Vec<u8> = Vec::with_capacity(n * 4);
    let mut widths = Vec::with_capacity(n);
    for &old in &l.olds {
        new_loca.extend_from_slice(&(new_glyf.len() as u32).to_be_bytes());
        let (a, b) = glyph_range(old)?;
        let mut g = glyf[a..b].to_vec();
        for (off, comp) in components(&g)? {
            let nid = *l.new_of_old.get(&comp)?;
            if l.olds.get(nid as usize).copied() != Some(comp) { return None; }
            g.get_mut(off..off + 2)?.copy_from_slice(&nid.to_be_bytes());
        }
        new_glyf.extend_from_slice(&g);
        while new_glyf.len() % 4 != 0 { new_glyf.push(0); }
        let adv = face.glyph_hor_advance(GlyphId(old)).unwrap_or(0);
        let lsb = face.glyph_hor_side_bearing(GlyphId(old)).unwrap_or(0);
        hmtx.extend_from_slice(&adv.to_be_bytes());
        hmtx.extend_from_slice(&lsb.to_be_bytes());
        widths.push(l.scale(adv as f64).round() as i64);
    }
    new_loca.extend_from_slice(&(new_glyf.len() as u32).to_be_bytes());

    let mut head2 = head.to_vec();
    head2.get_mut(8..12)?.copy_from_slice(&[0; 4]);
    head2.get_mut(50..52)?.copy_from_slice(&1u16.to_be_bytes());
    let mut hhea2 = hhea.to_vec();
    hhea2.get_mut(34..36)?.copy_from_slice(&n_u16.to_be_bytes());
    let mut maxp2 = maxp.to_vec();
    maxp2.get_mut(4..6)?.copy_from_slice(&n_u16.to_be_bytes());
    // post 3.0: no glyph names.
    let mut post = vec![0u8; 32];
    post[0..4].copy_from_slice(&0x0003_0000u32.to_be_bytes());
    if let Some(p) = table(b"post") {
        if p.len() >= 32 {
            post[4..32].copy_from_slice(&p[4..32]);
        }
    }

    let mut tables: Vec<([u8; 4], Vec<u8>)> = vec![
        (*b"glyf", new_glyf),
        (*b"head", head2),
        (*b"hhea", hhea2),
        (*b"hmtx", hmtx),
        (*b"loca", new_loca),
        (*b"maxp", maxp2),
        (*b"post", post),
    ];
    for t in [b"cvt ", b"fpgm", b"prep"] {
        if let Some(d) = table(t) {
            tables.push((*t, d.to_vec()));
        }
    }
    tables.sort_by(|a, b| a.0.cmp(&b.0));
    let file = write_sfnt(&tables);

    // Deterministic 6-letter subset tag from the glyph set.
    let mut h: u32 = 0x811c_9dc5;
    for g in &l.olds {
        for b in g.to_be_bytes() {
            h ^= b as u32;
            h = h.wrapping_mul(0x0100_0193);
        }
    }
    let tag: String = (0..6).map(|k| (b'A' + ((h >> (k * 5)) % 26) as u8) as char).collect();
    let s = |v: i16| l.scale(v as f64).round() as i64;
    Some(SubsetFont {
        base_name: format!("{tag}+{}", l.ps_name),
        file,
        widths,
        unicode: l.uni.clone(),
        ascent: s(l.ascent),
        descent: s(l.descent),
        cap_height: s(l.cap),
        bbox: [s(l.bbox.0), s(l.bbox.1), s(l.bbox.2), s(l.bbox.3)],
    })
}

fn checksum(d: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    for ch in d.chunks(4) {
        let mut w = [0u8; 4];
        w[..ch.len()].copy_from_slice(ch);
        sum = sum.wrapping_add(u32::from_be_bytes(w));
    }
    sum
}

fn write_sfnt(tables: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let n = tables.len() as u16;
    let mut pow = 1u16;
    let mut log = 0u16;
    while pow * 2 <= n {
        pow *= 2;
        log += 1;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    out.extend_from_slice(&n.to_be_bytes());
    out.extend_from_slice(&(pow * 16).to_be_bytes());
    out.extend_from_slice(&log.to_be_bytes());
    out.extend_from_slice(&(n * 16 - pow * 16).to_be_bytes());
    let mut offset = 12 + 16 * tables.len();
    let mut body = Vec::new();
    let mut head_at = None;
    for (tag, d) in tables {
        out.extend_from_slice(tag);
        out.extend_from_slice(&checksum(d).to_be_bytes());
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&(d.len() as u32).to_be_bytes());
        if tag == b"head" {
            head_at = Some(offset);
        }
        body.extend_from_slice(d);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        offset = 12 + 16 * tables.len() + body.len();
    }
    out.extend_from_slice(&body);
    if let Some(h) = head_at {
        let adj = 0xB1B0_AFBAu32.wrapping_sub(checksum(&out));
        out[h + 8..h + 12].copy_from_slice(&adj.to_be_bytes());
    }
    out
}

/// `/ToUnicode` CMap for glyph ids `0..unicode.len()`.
pub fn to_unicode_cmap(unicode: &[String]) -> String {
    let entries: Vec<(usize, &String)> = unicode.iter().enumerate().filter(|(_, s)| !s.is_empty()).collect();
    let mut s = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    for chunk in entries.chunks(100) {
        s.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (gid, text) in chunk {
            let hex: String = text.encode_utf16().map(|u| format!("{u:04X}")).collect();
            s.push_str(&format!("<{gid:04X}> <{hex}>\n"));
        }
        s.push_str("endbfchar\n");
    }
    s.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    s
}

/// `/W` array: every id in one run.
pub fn w_array(widths: &[i64]) -> String {
    let body: Vec<String> = widths.iter().map(|w| w.to_string()).collect();
    format!("[ 0 [ {} ] ]", body.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core_font() -> FontSource { symbol_candidates().remove(0) }

    #[test]
    fn sfnt_checksum_adjustment_makes_file_sum_magic() {
        let tables = vec![(*b"head", vec![0u8; 54]), (*b"maxp", vec![0, 1, 0, 0, 0, 3])];
        let f = write_sfnt(&tables);
        assert_eq!(checksum(&f), 0xB1B0_AFBA);
        assert_eq!(rd16(&f, 4), Some(2));
    }

    #[test]
    fn composite_components_are_found() {
        // numberOfContours = -1, bbox, one component (flags ARG_WORDS, gid 7).
        let mut g = vec![0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0];
        g.extend_from_slice(&[0x00, 0x01, 0x00, 0x07, 0, 0, 0, 0]);
        assert_eq!(components(&g), Some(vec![(12, 7)]));
        assert_eq!(components(&[0, 1, 0, 0, 0, 0, 0, 0, 0, 0]), Some(vec![]));
    }

    #[test]
    fn to_unicode_lists_mapped_glyphs() {
        let c = to_unicode_cmap(&["".into(), "中".into(), "𝑥".into()]);
        assert!(c.contains("<0001> <4E2D>"), "{c}");
        assert!(c.contains("<0002> <D835DC65>"), "{c}");
        assert!(!c.contains("<0000>  "));
    }

    #[test]
    fn without_context_encode_falls_back() {
        let p = encode("中文", false, 10.0);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].face, None);
        assert_eq!(p[0].bytes, vec![0x4e, 0x2d, 0x65, 0x87]);
        assert_eq!(advance('中', false, 10.0), None);
    }

    #[test]
    fn subset_round_trips_through_ttf_parser() {
        let src = core_font();
        begin_with(vec![src], vec![], vec![]);
        let pieces = encode("αβ A→", false, 12.0);
        assert!(pieces.iter().all(|p| p.face.is_some()), "{pieces:?}");
        assert!(advance('α', false, 10.0).unwrap() > 4.0);
        let (set, warns) = finish().unwrap();
        assert!(warns.is_empty(), "{warns:?}");
        let f = set.fonts[0].as_ref().expect("regular subset");
        assert!(f.file.len() < 200_000, "subset should be small: {}", f.file.len());
        let parsed = ttf_parser::Face::parse(&f.file, 0).expect("subset parses");
        assert_eq!(parsed.number_of_glyphs() as usize, f.widths.len());
        assert!(f.unicode.iter().any(|u| u == "α"));
        assert!(f.base_name.len() > 7 && f.base_name.as_bytes()[6] == b'+');
        // The glyph for α still has an outline after renumbering.
        let id = f.unicode.iter().position(|u| u == "α").unwrap() as u16;
        struct Count(usize);
        impl ttf_parser::OutlineBuilder for Count {
            fn move_to(&mut self, _: f32, _: f32) { self.0 += 1 }
            fn line_to(&mut self, _: f32, _: f32) { self.0 += 1 }
            fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) { self.0 += 1 }
            fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) { self.0 += 1 }
            fn close(&mut self) {}
        }
        let mut c = Count(0);
        assert!(parsed.outline_glyph(GlyphId(id), &mut c).is_some());
        assert!(c.0 > 4);
    }

    #[test]
    fn missing_everything_warns_once() {
        begin_with(vec![], vec![], vec![]);
        let p = encode("中", false, 10.0);
        assert_eq!(p[0].face, None);
        let (_, warns) = finish().unwrap();
        assert_eq!(warns.len(), 1);
        assert!(warns[0].contains("未找到可嵌入的 CJK 字体"));
    }

    #[test]
    fn core_fallback_astral_font_matches_independent_release_oracle() {
        use sha2::Digest;
        assert_eq!(CORE_FALLBACK_BYTES.len(), 757_076);
        assert_eq!(format!("{:x}", sha2::Sha256::digest(CORE_FALLBACK_BYTES)),
            "7da195a74c55bef988d0d48f9508bd5d849425c1770dba5d7bfc6ce9ed848954");
        let face = ttf_parser::Face::parse(CORE_FALLBACK_BYTES, 0).unwrap();
        assert_eq!(face.units_per_em(), 2048);
        assert_ne!(face.permissions(), Some(ttf_parser::Permissions::Restricted));
        assert!(face.is_subsetting_allowed());
        let gid = face.glyph_index('😀').unwrap();
        // FontTools 4.63.0 on the original 2.37 TTF, not renderer-derived values.
        assert_eq!(gid.0, 5857);
        assert_eq!(face.glyph_hor_advance(gid), Some(2135));
        let ink = face.glyph_bounding_box(gid).unwrap();
        assert_eq!((ink.x_min, ink.y_min, ink.x_max, ink.y_max), (170, -150, 1965, 1646));
    }

    #[test]
    fn core_fallback_is_per_glyph_after_stix_and_subsets_real_astral_outline() {
        begin("Arial");
        assert!((advance('😀', false, 11.0).unwrap() - 11.46728515625).abs() < 1e-10);
        let pieces = encode("𝑥😀", false, 11.0);
        assert_eq!(pieces.len(), 2);
        assert!(pieces.iter().all(|piece| piece.face.is_some()));
        let (set, warnings) = finish().unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(set.faces[pieces[0].face.unwrap() as usize].slot, SYMBOL);
        assert_eq!(set.faces[pieces[1].face.unwrap() as usize].slot, CORE_FALLBACK);
        let font = set.fonts[CORE_FALLBACK].as_ref().unwrap();
        let gid = font.unicode.iter().position(|text| text == "😀").unwrap() as u16;
        let subset = ttf_parser::Face::parse(&font.file, 0).unwrap();
        let ink = subset.glyph_bounding_box(GlyphId(gid)).unwrap();
        assert_eq!((ink.x_min, ink.y_min, ink.x_max, ink.y_max), (170, -150, 1965, 1646));
        assert_eq!(subset.glyph_hor_advance(GlyphId(gid)), Some(2135));
        assert!(font.base_name.ends_with("+DejaVuSans"), "{}", font.base_name);
        assert!(to_unicode_cmap(&font.unicode).contains(&format!("<{gid:04X}> <D83DDE00>")));
    }

    #[test]
    fn core_fallback_bold_keeps_real_metrics_and_unknown_scalar_stays_missing() {
        begin("Arial");
        assert!((advance('😀', true, 11.0).unwrap() - 11.46728515625).abs() < 1e-10);
        assert_eq!(advance('\u{10ffff}', true, 11.0), None);
        let pieces = encode("😀", true, 11.0);
        let (set, warnings) = finish().unwrap();
        let info = set.faces[pieces[0].face.unwrap() as usize];
        assert_eq!(info.slot, CORE_FALLBACK);
        assert!(info.fake_bold);
        assert!(warnings.is_empty(), "{warnings:?}");
    }
}

#[cfg(test)]
mod p15_owned_proportional_tests {
    use super::*;

    #[test]
    fn p15_core_latin_metrics_match_independent_fonttools_oracle_and_pdf_w() {
        begin_with_fallback(vec![], vec![], vec![], core_fallback_candidates());
        // Original DejaVu 2.37: upem2048, A advance1401, i569, space651.
        // PDF /W quantizes to integer 1/1000em, exactly as the measured run.
        for (scalar, width) in [('A',684.),('i',278.),(' ',318.)] {
            assert_eq!(proportional_advance(scalar,false,11.),Some(width*11./1000.));
        }
        let pieces=encode_proportional("Ai ",false,11.).unwrap();
        let [ProportionalPiece::Embedded(piece)]=pieces.as_slice() else {panic!("owned regular program required")};
        assert!((piece.advance-14.08).abs()<1e-10);
        let (fonts,warnings)=finish().unwrap(); assert!(warnings.is_empty());
        let face=fonts.faces[piece.face.unwrap() as usize];assert!(!face.fake_bold);
        let font=fonts.fonts[face.slot].as_ref().unwrap();
        let widths=piece.bytes.chunks_exact(2).map(|b|font.widths[u16::from_be_bytes([b[0],b[1]]) as usize]).collect::<Vec<_>>();
        assert_eq!(widths,vec![684,278,318]);
        assert!(font.base_name.ends_with("+DejaVuSans"));
    }

    #[test]
    fn p15_owned_latin_subsets_original_outlines_composites_and_unicode() {
        begin_with_fallback(vec![],vec![],vec![],core_fallback_candidates());
        let pieces=encode_proportional("AÉÇ€",false,11.).unwrap();
        let [ProportionalPiece::Embedded(piece)]=pieces.as_slice() else {panic!("owned regular")};
        let (fonts,warnings)=finish().unwrap();assert!(warnings.is_empty());
        let font=fonts.fonts[fonts.faces[piece.face.unwrap() as usize].slot].as_ref().unwrap();
        let subset=ttf_parser::Face::parse(&font.file,0).unwrap();
        let source=ttf_parser::Face::parse(CORE_FALLBACK_BYTES,0).unwrap();
        for scalar in "AÉÇ€".chars() {
            let gid=font.unicode.iter().position(|u|u==&scalar.to_string()).unwrap() as u16;
            let original=source.glyph_index(scalar).unwrap();
            assert_eq!(subset.glyph_bounding_box(GlyphId(gid)),source.glyph_bounding_box(original));
            assert_eq!(subset.glyph_hor_advance(GlyphId(gid)),source.glyph_hor_advance(original));
            assert!(subset.glyph_bounding_box(GlyphId(gid)).is_some());
            assert!(to_unicode_cmap(&font.unicode).contains(&format!("<{gid:04X}> <{:04X}>",scalar as u32)));
        }
    }

    #[test]
    fn p15_selected_owned_regular_has_priority_over_core_and_symbols() {
        // Explicit candidates test resolver priority. This is not an audit of
        // the managed pack's family/weight; its original bytes are separately locked.
        begin_with_fallback(symbol_candidates(),vec![],symbol_candidates(),core_fallback_candidates());
        let pieces=encode_proportional("A",false,11.).unwrap();
        let [ProportionalPiece::Embedded(piece)]=pieces.as_slice() else {panic!("selected regular")};
        let (fonts,warnings)=finish().unwrap();assert!(warnings.is_empty());
        assert_eq!(fonts.faces[piece.face.unwrap() as usize].slot,REGULAR);
        assert!(fonts.fonts[REGULAR].as_ref().unwrap().base_name.contains("STIX"));
        assert!(fonts.fonts[CORE_FALLBACK].is_none());
    }

    #[test]
    fn p15_bold_without_owned_bold_never_borrows_regular_or_core() {
        begin_with_fallback(symbol_candidates(),vec![],vec![],core_fallback_candidates());
        assert_eq!(proportional_advance('A',true,11.),None);
        assert_eq!(proportional_vertical_bounds('A',true,11.),None);
        let pieces=encode_proportional("Bold",true,11.).unwrap();
        let [ProportionalPiece::Base14(text)]=pieces.as_slice() else {panic!("preserve original bold")};
        assert_eq!(text,"Bold");
        let (fonts,warnings)=finish().unwrap();assert!(fonts.faces.is_empty());
        assert_eq!(warnings,vec!["publication_font_substituted"]);
    }

    #[test]
    fn p15_proportional_miss_does_not_borrow_the_math_program() {
        begin_with(vec![],vec![],symbol_candidates());
        assert_eq!(proportional_advance('A',false,11.),None);
        let pieces=encode_proportional("Aa",false,11.).unwrap();
        assert!(matches!(pieces.as_slice(),[ProportionalPiece::Base14(text)] if text=="Aa"));
        let (fonts,warnings)=finish().unwrap();assert!(fonts.faces.is_empty());
        assert_eq!(warnings,vec!["publication_font_substituted"]);
    }

    #[test]
    fn p15_space_font_strut_is_not_a_claim_of_visible_space_ink() {
        begin_with_fallback(vec![],vec![],vec![],core_fallback_candidates());
        let (upper,lower)=proportional_vertical_bounds(' ',false,11.).unwrap();
        assert!((upper-1901.*11./2048.).abs()<1e-10);
        assert!((lower-483.*11./2048.).abs()<1e-10);
        let font=ttf_parser::Face::parse(CORE_FALLBACK_BYTES,0).unwrap();
        assert_eq!(font.glyph_bounding_box(font.glyph_index(' ').unwrap()),None);
        let (set,warnings)=finish().unwrap();assert!(set.fonts.iter().all(Option::is_none));assert!(warnings.is_empty());
    }

    #[test]
    fn p15_proportional_context_ends_and_measurement_does_not_register_glyphs() {
        begin_with_fallback(vec![],vec![],vec![],core_fallback_candidates());
        assert!(proportional_advance('A',false,11.).is_some());
        let (fonts,warnings)=finish().unwrap();assert!(fonts.faces.is_empty());
        assert!(fonts.fonts.iter().all(Option::is_none));assert!(warnings.is_empty());
        assert_eq!(proportional_advance('A',false,11.),None);
        assert_eq!(proportional_vertical_bounds('A',false,11.),None);
        assert!(encode_proportional("A",false,11.).is_none());
    }
}

#[cfg(test)]
mod p15_unicode_cid_tests {
    use super::*;

    fn core() -> Loaded { load(&core_fallback_candidates().remove(0)).unwrap() }

    // This isolates CID identity, not cmap selection: the locked managed fonts'
    // actual middle-dot/bullet aliases have a separate FontTools audit and PDF QA.
    #[test]
    fn p15_unicode_aliases_get_distinct_cids_in_both_orders_without_changing_components() {
        for texts in [["·", "•"], ["•", "·"]] {
            let mut font = core();
            let (old, _) = font.lookup('·').unwrap();
            let first = font.use_gid(old, texts[0]).unwrap();
            let second = font.use_gid(old, texts[1]).unwrap();
            assert_ne!(first, second);
            assert_eq!(font.new_of_old.get(&old), Some(&first));
            assert_eq!(font.use_gid(old, ""), Some(first));
            assert_eq!(font.use_gid(old, texts[0]), Some(first));
            assert_eq!(font.use_gid(old, texts[1]), Some(second));
            let subset = subset(&mut font).unwrap();
            let parsed = ttf_parser::Face::parse(&subset.file, 0).unwrap();
            let original = ttf_parser::Face::parse(CORE_FALLBACK_BYTES, 0).unwrap();
            for (cid, text) in [(first, texts[0]), (second, texts[1])] {
                assert_eq!(subset.unicode[cid as usize], text);
                assert_eq!(parsed.glyph_bounding_box(GlyphId(cid)), original.glyph_bounding_box(GlyphId(old)));
                assert_eq!(parsed.glyph_hor_advance(GlyphId(cid)), original.glyph_hor_advance(GlyphId(old)));
                assert!(to_unicode_cmap(&subset.unicode).contains(&format!("<{cid:04X}> <{:04X}>", text.chars().next().unwrap() as u32)));
            }
            assert_eq!(parsed.number_of_glyphs() as usize, subset.widths.len());
            assert_eq!(subset.widths.len(), subset.unicode.len());
        }
    }

    #[test]
    fn p15_component_first_visible_use_reuses_canonical_but_alias_does_not_replace_it() {
        let mut font = core();
        let old = font.lookup('A').unwrap().0;
        let component = font.use_gid(old, "").unwrap();
        assert_eq!(font.use_gid(old, "A"), Some(component));
        let alias = font.use_gid(old, "Ａ").unwrap();
        assert_ne!(alias, component);
        let count = font.olds.len();
        assert_eq!(font.use_gid(old, ""), Some(component));
        assert_eq!(font.use_gid(old, "A"), Some(component));
        assert_eq!(font.use_gid(old, "Ａ"), Some(alias));
        assert_eq!(font.olds.len(), count);
        assert_eq!(font.uni[component as usize], "A");
        assert_eq!(font.uni[alias as usize], "Ａ");
    }

    #[test]
    fn p15_nested_composites_keep_complete_canonical_component_closure() {
        let mut font = core();
        // Original DejaVu U+01FB contains aring, itself a composite.
        let old = font.lookup('ǻ').unwrap().0;
        assert_eq!(old, 445); // Independent original FontTools audit.
        let cid = font.use_gid(old, "ǻ").unwrap();
        let alias = font.use_gid(old, "unit-alias").unwrap();
        let original = ttf_parser::Face::parse(CORE_FALLBACK_BYTES, 0).unwrap();
        let subset = subset(&mut font).unwrap();
        let parsed = ttf_parser::Face::parse(&subset.file, 0).unwrap();
        assert!(font.olds.len() > 5, "nested components must be pulled in");
        assert_eq!(font.new_of_old.get(&old), Some(&cid));
        for id in [cid, alias] {
            assert_eq!(parsed.glyph_bounding_box(GlyphId(id)), original.glyph_bounding_box(GlyphId(old)));
            assert_eq!(parsed.glyph_hor_advance(GlyphId(id)), original.glyph_hor_advance(GlyphId(old)));
        }
        assert_eq!(parsed.number_of_glyphs() as usize, subset.unicode.len());
        assert_eq!(subset.widths.len(), subset.unicode.len());
        for (old, canonical) in &font.new_of_old {
            assert_eq!(font.olds[*canonical as usize], *old);
        }
    }

    #[test]
    fn p15_cid_capacity_refuses_new_alias_without_wrapping_or_mutation() {
        let mut font = core();
        let old = font.lookup('A').unwrap().0;
        let cid = font.use_gid(old, "A").unwrap();
        font.olds.resize(MAX_SUBSET_GLYPHS, 0);
        font.uni.resize(MAX_SUBSET_GLYPHS, String::new());
        assert_eq!(font.use_gid(old, "A"), Some(cid), "existing CID remains usable at capacity");
        assert_eq!(font.use_gid(old, "Ａ"), None);
        assert_eq!(font.olds.len(), MAX_SUBSET_GLYPHS);
        assert_eq!(font.uni.len(), MAX_SUBSET_GLYPHS);
        assert!(font.allocation_failed);
        assert_eq!(font.new_of_old.get(&old), Some(&cid));
    }

    #[test]
    fn p15_component_capacity_failure_rejects_subset_instead_of_using_notdef() {
        let mut font = core();
        let old = font.lookup('ǻ').unwrap().0;
        font.use_gid(old, "ǻ").unwrap();
        font.olds.resize(MAX_SUBSET_GLYPHS, 0);
        font.uni.resize(MAX_SUBSET_GLYPHS, String::new());
        assert!(subset(&mut font).is_none());
        assert!(font.allocation_failed);
        assert_eq!(font.olds.len(), MAX_SUBSET_GLYPHS);
    }

    #[test]
    fn p15_capacity_is_an_error_at_finish_and_clears_the_export_context() {
        for proportional in [true, false] {
            begin_with_fallback(vec![], vec![], vec![], core_fallback_candidates());
            with_ctx(|ctx| {
                let font = ctx.slots[CORE_FALLBACK].loaded().unwrap();
                font.olds.resize(MAX_SUBSET_GLYPHS, 0);
                font.uni.resize(MAX_SUBSET_GLYPHS, String::new());
            }).unwrap();
            if proportional { assert!(encode_proportional("A", false, 11.).unwrap().is_empty()); }
            else { assert!(encode("α", false, 11.).is_empty()); }
            assert_eq!(finish().unwrap_err(), "transaction_budget");
            assert_eq!(proportional_advance('A', false, 11.), None);
            assert!(finish().unwrap().0.faces.is_empty());
        }
    }

    #[test]
    fn p15_invalid_component_mapping_cannot_silently_become_glyph_zero() {
        let mut font = core();
        let old = font.lookup('ǻ').unwrap().0;
        font.use_gid(old, "ǻ").unwrap();
        let original = ttf_parser::Face::parse(CORE_FALLBACK_BYTES, 0).unwrap();
        let raw = original.raw_face();
        let loca = raw.table(Tag::from_bytes(b"loca")).unwrap();
        let glyf = raw.table(Tag::from_bytes(b"glyf")).unwrap();
        let a = rd32(loca, old as usize * 4).unwrap() as usize;
        let b = rd32(loca, old as usize * 4 + 4).unwrap() as usize;
        let comp = components(&glyf[a..b]).unwrap()[0].1;
        font.new_of_old.insert(comp, 0);
        assert!(subset(&mut font).is_none());
        assert!(!font.allocation_failed);
    }

    #[test]
    fn p15_invalid_original_glyph_is_a_finish_error_not_an_empty_font() {
        begin_with_fallback(vec![], vec![], vec![], core_fallback_candidates());
        with_ctx(|ctx| { ctx.slots[CORE_FALLBACK].loaded().unwrap().use_gid(u16::MAX, "A").unwrap(); }).unwrap();
        assert_eq!(finish().unwrap_err(), crate::api_codes::PUBLICATION_FONTS_UNAVAILABLE);
        assert!(finish().unwrap().0.faces.is_empty());
    }

    #[test]
    fn p15_truncated_or_over_budget_component_lists_are_rejected() {
        assert_eq!(components(&[]), Some(vec![]));
        assert_eq!(components(&[0; 9]), None);
        let mut truncated = vec![0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0];
        truncated.extend_from_slice(&[0, 1, 0, 7, 0, 0]);
        assert_eq!(components(&truncated), None, "word arguments require four bytes");
        let mut too_many = vec![0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0];
        for _ in 0..65 { too_many.extend_from_slice(&[0, 0x20, 0, 7, 0, 0]); }
        assert_eq!(components(&too_many), None);
    }
}
