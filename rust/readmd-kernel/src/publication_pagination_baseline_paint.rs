//! Qualified generated-PDF paint proof. Copy text, font names and annotation
//! rectangles do not identify a source or establish visible glyph bounds.
use super::*;
use lopdf::{content::Content as PdfContent, Stream};
use sha2::{Digest, Sha256};
use std::{io::Read, sync::Arc};
#[path = "publication_pagination_baseline_paint_ops.rs"]
mod ops;

const EPS: f64 = 0.001;
const MAX_OPS: usize = 500_000;
const MAX_EXPANDED: usize = 64 * 1024 * 1024;
const MAX_STREAM: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
struct Matrix([f64; 6]);
impl Matrix {
    fn identity() -> Self {
        Self([1., 0., 0., 1., 0., 0.])
    }
    fn point(self, x: f64, y: f64) -> (f64, f64) {
        let m = self.0;
        (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
    }
    /// self after rhs, using PDF's column-vector equivalent convention.
    fn concat(self, rhs: Self) -> Self {
        let a = self.0;
        let b = rhs.0;
        Self([
            a[0] * b[0] + a[2] * b[1],
            a[1] * b[0] + a[3] * b[1],
            a[0] * b[2] + a[2] * b[3],
            a[1] * b[2] + a[3] * b[3],
            a[0] * b[4] + a[2] * b[5] + a[4],
            a[1] * b[4] + a[3] * b[5] + a[5],
        ])
    }
    fn translate(self, x: f64, y: f64) -> Self {
        self.concat(Self([1., 0., 0., 1., x, y]))
    }
    fn scale_bound(self) -> f64 {
        let m = self.0;
        (m[0] * m[0] + m[1] * m[1] + m[2] * m[2] + m[3] * m[3]).sqrt()
    }
    fn finite(self) -> bool {
        self.0.iter().all(|v| v.is_finite() && v.abs() <= 1e9)
    }
    fn axis_aligned(self) -> bool {
        self.0[1].abs() < 1e-9 && self.0[2].abs() < 1e-9
    }
}
#[derive(Clone, Copy, Debug)]
struct Bounds([f64; 4]);
impl Bounds {
    fn point(x: f64, y: f64) -> Self {
        Self([x, y, x, y])
    }
    fn add(&mut self, x: f64, y: f64) {
        self.0[0] = self.0[0].min(x);
        self.0[1] = self.0[1].min(y);
        self.0[2] = self.0[2].max(x);
        self.0[3] = self.0[3].max(y);
    }
    fn union(self, b: Self) -> Self {
        let mut a = self;
        a.add(b.0[0], b.0[1]);
        a.add(b.0[2], b.0[3]);
        a
    }
    fn transformed(self, m: Matrix) -> Self {
        let p = m.point(self.0[0], self.0[1]);
        let mut b = Self::point(p.0, p.1);
        for (x, y) in [
            (self.0[0], self.0[3]),
            (self.0[2], self.0[1]),
            (self.0[2], self.0[3]),
        ] {
            let p = m.point(x, y);
            b.add(p.0, p.1);
        }
        b
    }
    fn expand(self, r: f64) -> Self {
        Self([self.0[0] - r, self.0[1] - r, self.0[2] + r, self.0[3] + r])
    }
    fn inside(self, b: Self) -> bool {
        self.0[0] >= b.0[0] - EPS
            && self.0[1] >= b.0[1] - EPS
            && self.0[2] <= b.0[2] + EPS
            && self.0[3] <= b.0[3] + EPS
    }
    fn finite(self) -> bool {
        self.0.iter().all(|v| v.is_finite() && v.abs() <= 1e9)
    }
    fn intersection(self, b: Self) -> Result<Self, String> {
        let c = Self([
            self.0[0].max(b.0[0]),
            self.0[1].max(b.0[1]),
            self.0[2].min(b.0[2]),
            self.0[3].min(b.0[3]),
        ]);
        if c.0[0] > c.0[2] || c.0[1] > c.0[3] {
            Err(UNSUPPORTED.into())
        } else {
            Ok(c)
        }
    }
    fn overlaps(self, b: Self) -> bool {
        self.0[2].min(b.0[2]) - self.0[0].max(b.0[0]) > EPS
            && self.0[3].min(b.0[3]) - self.0[1].max(b.0[1]) > EPS
    }
}
#[derive(Default)]
struct Outline {
    bounds: Option<Bounds>,
    bad: bool,
    points: usize,
}
impl Outline {
    fn point(&mut self, x: f32, y: f32) {
        self.points = self.points.saturating_add(1);
        if self.points > 131072 || !x.is_finite() || !y.is_finite() {
            self.bad = true;
            return;
        }
        if let Some(b) = &mut self.bounds {
            b.add(x as f64, y as f64);
        } else {
            self.bounds = Some(Bounds::point(x as f64, y as f64));
        }
    }
}
impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.point(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.point(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.point(x1, y1);
        self.point(x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.point(x1, y1);
        self.point(x2, y2);
        self.point(x, y);
    }
    fn close(&mut self) {}
}

fn number(v: &Object) -> Result<f64, String> {
    super::super::number(v)
        .filter(|n| n.is_finite() && n.abs() <= 1e9)
        .ok_or_else(|| INVALID.into())
}
fn numbers(v: &[Object], n: usize) -> Result<Vec<f64>, String> {
    if v.len() != n {
        return Err(INVALID.into());
    }
    v.iter().map(number).collect()
}
fn matrix(v: &[Object]) -> Result<Matrix, String> {
    let a = numbers(v, 6)?;
    let m = Matrix([a[0], a[1], a[2], a[3], a[4], a[5]]);
    if !m.finite() || (m.0[0] * m.0[3] - m.0[1] * m.0[2]).abs() < 1e-12 {
        return Err(UNSUPPORTED.into());
    }
    Ok(m)
}
fn direct<'a>(doc: &'a Document, v: &'a Object) -> Result<&'a Object, String> {
    graph::direct(doc, v)
}
fn dictionary<'a>(doc: &'a Document, v: &'a Object) -> Result<&'a Dictionary, String> {
    direct(doc, v)?.as_dict().map_err(|_| INVALID.into())
}
fn resource<'a>(
    doc: &'a Document,
    res: &'a Dictionary,
    kind: &[u8],
    name: &[u8],
) -> Result<&'a Object, String> {
    dictionary(doc, res.get(kind).map_err(|_| INVALID)?)?
        .get(name)
        .map_err(|_| INVALID.into())
}
fn bounds(doc: &Document, v: &Object) -> Result<Bounds, String> {
    let a = numbers(direct(doc, v)?.as_array().map_err(|_| INVALID)?, 4)?;
    if a[0] >= a[2] || a[1] >= a[3] {
        return Err(INVALID.into());
    }
    Ok(Bounds([a[0], a[1], a[2], a[3]]))
}

#[derive(Clone)]
struct Site {
    source: Option<String>,
    rise: f64,
    protected: Option<ObjectId>,
}
#[derive(Clone, Copy)]
enum Mark {
    Content(ObjectId, i64),
    Artifact,
}
#[derive(Clone)]
struct State {
    ctm: Matrix,
    tm: Matrix,
    line: Matrix,
    font: Option<usize>,
    size: f64,
    char_space: f64,
    hscale: f64,
    leading: f64,
    rise: f64,
    width: f64,
    miter: f64,
    cap: i64,
    join: i64,
    clip: Bounds,
    fill_alpha: f64,
    stroke_alpha: f64,
}
impl State {
    fn new(media: [f64; 4]) -> Self {
        Self {
            ctm: Matrix::identity(),
            tm: Matrix::identity(),
            line: Matrix::identity(),
            font: None,
            size: 0.,
            char_space: 0.,
            hscale: 1.,
            leading: 0.,
            rise: 0.,
            width: 1.,
            miter: 10.,
            cap: 0,
            join: 0,
            clip: Bounds(media),
            fill_alpha: 1.,
            stroke_alpha: 1.,
        }
    }
}
struct Font {
    data: Vec<u8>,
    mapping: Option<Vec<u8>>,
    widths: BTreeMap<u16, f64>,
    default_width: f64,
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Group {
    page: ObjectId,
    source: Option<String>,
    protection: Option<ObjectId>,
    line: i64,
    mark: Option<i64>,
    glyph: Option<usize>,
}
struct Ink {
    bounds: Bounds,
    changed: bool,
    source: Option<String>,
}
struct Coverage {
    key: (ObjectId, i64),
    from: (f64, f64),
    to: (f64, f64),
    has_ink: bool,
}
pub(super) struct PaintProof {
    snapshot: [u8; 32],
    source_ink: BTreeMap<(ObjectId, String), Bounds>,
}
impl PaintProof {
    pub(super) fn append_source_hits(
        &self,
        input: &Instrumented,
        pages: &[Page],
        hits: &mut Vec<Hit>,
        budget: &mut Budget,
    ) -> Result<(), String> {
        if self.source_ink.len() > MAX_LINKS.saturating_sub(hits.len()) {
            return Err(LIMIT.into());
        }
        let known = input.nodes.iter().map(|node| node.id.as_str()).collect::<BTreeSet<_>>();
        let by_page = pages.iter().enumerate().map(|(index, page)| (page.id, (index as u32 + 1, page.media)))
            .collect::<BTreeMap<_, _>>();
        for ((page_id, source), bounds) in &self.source_ink {
            budget.step(0)?;
            if !known.contains(source.as_str()) {
                return Err(INVALID.into());
            }
            let (page, media) = by_page.get(page_id).ok_or(INVALID)?;
            hits.push(super::hit(bounds.0, *media, source, *page).ok_or(INVALID)?);
        }
        Ok(())
    }

    pub(super) fn check_unchanged(
        &self,
        doc: &Document,
        pages: &[Page],
        budget: &mut Budget,
    ) -> Result<(), String> {
        if snapshot(doc, pages, budget)? != self.snapshot {
            return Err(INVALID.into());
        }
        Ok(())
    }
}

struct Reader<'a, 'b> {
    doc: &'a Document,
    budget: &'b mut Budget,
    sites: BTreeMap<(ObjectId, i64), Site>,
    seen: BTreeSet<(ObjectId, i64)>,
    painted: BTreeSet<(ObjectId, i64)>,
    fonts: BTreeMap<usize, Arc<Font>>,
    outlines: BTreeMap<(usize, u16), Option<Bounds>>,
    expanded: usize,
    operations: usize,
    forms: BTreeSet<usize>,
    ink: BTreeMap<Group, Ink>,
    ordinary: BTreeSet<String>,
    protected: BTreeSet<ObjectId>,
    coverage: Vec<Coverage>,
    page_numbers: BTreeMap<ObjectId, u32>,
    page_media: BTreeMap<ObjectId, Bounds>,
}
impl<'a, 'b> Reader<'a, 'b> {
    fn stream(&mut self, s: &Stream) -> Result<Vec<u8>, String> {
        self.budget.step(0)?;
        if s.content.len() > MAX_STREAM {
            return Err(LIMIT.into());
        }
        let filters = match s.dict.get(b"Filter") {
            Err(_) => Vec::new(),
            Ok(v) => match direct(self.doc, v)? {
                Object::Name(n) => vec![n.as_slice()],
                Object::Array(a) => a
                    .iter()
                    .map(|n| n.as_name().map_err(|_| INVALID.into()))
                    .collect::<Result<Vec<_>, String>>()?,
                _ => return Err(INVALID.into()),
            },
        };
        let remaining = MAX_EXPANDED
            .checked_sub(self.expanded)
            .ok_or(LIMIT)?
            .min(MAX_STREAM);
        let data = if filters.is_empty() {
            s.content.clone()
        } else if filters == vec![b"FlateDecode".as_slice()] {
            if s.dict.has(b"DecodeParms") {
                return Err(UNSUPPORTED.into());
            }
            let mut d = Vec::new();
            flate2::read::ZlibDecoder::new(s.content.as_slice())
                .take(remaining as u64 + 1)
                .read_to_end(&mut d)
                .map_err(|_| INVALID)?;
            d
        } else {
            return Err(UNSUPPORTED.into());
        };
        self.expanded = self.expanded.checked_add(data.len()).ok_or(LIMIT)?;
        if data.len() > MAX_STREAM || self.expanded > MAX_EXPANDED {
            return Err(LIMIT.into());
        }
        Ok(data)
    }
    fn contents(&mut self, v: &Object, out: &mut Vec<u8>, depth: usize) -> Result<(), String> {
        self.budget.step(depth)?;
        match direct(self.doc, v)? {
            Object::Stream(s) => {
                let data = self.stream(s)?;
                if out.len().saturating_add(data.len() + 1) > MAX_STREAM {
                    return Err(LIMIT.into());
                }
                out.extend(data);
                out.push(b'\n');
            }
            Object::Array(a) => {
                for v in a {
                    self.contents(v, out, depth + 1)?;
                }
            }
            Object::Null => (),
            _ => return Err(INVALID.into()),
        }
        Ok(())
    }
    fn font(&mut self, value: &'a Object) -> Result<usize, String> {
        let object = direct(self.doc, value)?;
        let key = object as *const Object as usize;
        if self.fonts.contains_key(&key) {
            return Ok(key);
        }
        if self.fonts.len() >= 256 {
            return Err(LIMIT.into());
        }
        let d = object.as_dict().map_err(|_| INVALID)?;
        if d.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"Type0")
            || d.get(b"Encoding").and_then(Object::as_name).ok() != Some(b"Identity-H")
        {
            return Err(UNSUPPORTED.into());
        }
        let descendants = direct(self.doc, d.get(b"DescendantFonts").map_err(|_| INVALID)?)?
            .as_array()
            .map_err(|_| INVALID)?;
        if descendants.len() != 1 {
            return Err(INVALID.into());
        }
        let cid = dictionary(self.doc, &descendants[0])?;
        if cid.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"CIDFontType2") {
            return Err(UNSUPPORTED.into());
        }
        let descriptor = dictionary(self.doc, cid.get(b"FontDescriptor").map_err(|_| INVALID)?)?;
        let program = direct(
            self.doc,
            descriptor.get(b"FontFile2").map_err(|_| UNSUPPORTED)?,
        )?
        .as_stream()
        .map_err(|_| INVALID)?;
        let data = self.stream(program)?;
        let face = ttf_parser::Face::parse(&data, 0).map_err(|_| INVALID)?;
        if face.tables().glyf.is_none() || face.units_per_em() == 0 || face.number_of_glyphs() == 0
        {
            return Err(UNSUPPORTED.into());
        }
        let mapping = match direct(self.doc, cid.get(b"CIDToGIDMap").map_err(|_| UNSUPPORTED)?)? {
            Object::Name(n) if n == b"Identity" => None,
            Object::Stream(s) => {
                let data = self.stream(s)?;
                if data.len() > 131072 || data.len() % 2 != 0 {
                    return Err(INVALID.into());
                }
                Some(data)
            }
            _ => return Err(UNSUPPORTED.into()),
        };
        let default_width = cid
            .get(b"DW")
            .ok()
            .map(number)
            .transpose()?
            .unwrap_or(1000.);
        if !(0. ..=10000.).contains(&default_width) {
            return Err(INVALID.into());
        }
        let mut widths = BTreeMap::new();
        if let Ok(v) = cid.get(b"W") {
            let values = direct(self.doc, v)?.as_array().map_err(|_| INVALID)?;
            let mut i = 0;
            while i < values.len() {
                self.budget.step(0)?;
                let start = values[i].as_i64().map_err(|_| INVALID)?;
                if !(0..=65535).contains(&start) {
                    return Err(INVALID.into());
                }
                i += 1;
                let next = values.get(i).ok_or(INVALID)?;
                if let Object::Array(entries) = direct(self.doc, next)? {
                    for (offset, width) in entries.iter().enumerate() {
                        let at = start as usize + offset;
                        if at > 65535 {
                            return Err(INVALID.into());
                        }
                        let w = number(width)?;
                        if !(0. ..=10000.).contains(&w) || widths.insert(at as u16, w).is_some() {
                            return Err(INVALID.into());
                        }
                    }
                    i += 1;
                } else {
                    let end = next.as_i64().map_err(|_| INVALID)?;
                    i += 1;
                    let w = number(values.get(i).ok_or(INVALID)?)?;
                    i += 1;
                    if end < start || end > 65535 || !(0. ..=10000.).contains(&w) {
                        return Err(INVALID.into());
                    }
                    for at in start..=end {
                        self.budget.step(0)?;
                        if widths.insert(at as u16, w).is_some() {
                            return Err(INVALID.into());
                        }
                    }
                }
            }
        }
        self.fonts.insert(
            key,
            Arc::new(Font {
                data,
                mapping,
                widths,
                default_width,
            }),
        );
        Ok(key)
    }
    fn emit(
        &mut self,
        page: ObjectId,
        state: &State,
        marks: &[Mark],
        rect: Bounds,
        origin: Option<(f64, f64)>,
        is_text: bool,
    ) -> Result<(), String> {
        self.budget.step(0)?;
        if !rect.finite() {
            return Err(INVALID.into());
        }
        let (site, key) = match marks.last().copied().ok_or(UNSUPPORTED)? {
            Mark::Content(p, m) => {
                let s = self.sites.get(&(p, m)).ok_or(INVALID)?.clone();
                self.painted.insert((p, m));
                (s, Some((p, m)))
            }
            Mark::Artifact => (
                Site {
                    source: None,
                    rise: 0.,
                    protected: None,
                },
                None,
            ),
        };
        if !rect.inside(state.clip) {
            let reason = if rect.inside(*self.page_media.get(&page).ok_or(INVALID)?) {
                "clipped-ink"
            } else {
                "paper-edge"
            };
            return Err(geometry(
                site.source.as_deref(),
                page,
                *self.page_numbers.get(&page).ok_or(INVALID)?,
                reason,
                rect,
                None,
            ));
        }
        let changed = site.rise != 0. && site.protected.is_none();
        if changed && !is_text {
            return Err(UNSUPPORTED.into());
        }
        if changed {
            self.ordinary.insert(site.source.clone().ok_or(INVALID)?);
        }
        if let Some(p) = site.protected {
            self.protected.insert(p);
        }
        let line = if let Some((_, y)) = origin {
            ((y - if site.protected.is_none() {
                site.rise
            } else {
                0.
            }) / EPS)
                .round() as i64
        } else {
            key.map_or(-1, |(_, m)| m)
        };
        // Keep separate glyph envelopes across CODE/Math gaps. A line-wide
        // union would falsely treat empty inter-glyph space as painted ink.
        let group = Group {
            page,
            source: site.source.clone(),
            protection: site.protected,
            line,
            mark: key.map(|(_, m)| m),
            glyph: is_text.then_some(self.ink.len()),
        };
        self.ink
            .entry(group)
            .and_modify(|v| v.bounds = v.bounds.union(rect))
            .or_insert(Ink {
                bounds: rect,
                changed,
                source: site.source,
            });
        Ok(())
    }
    fn text(
        &mut self,
        page: ObjectId,
        state: &mut State,
        marks: &[Mark],
        v: &Object,
    ) -> Result<(), String> {
        if state.fill_alpha == 0. {
            return Err(UNSUPPORTED.into());
        }
        let bytes = v.as_str().map_err(|_| INVALID)?;
        if bytes.len() % 2 != 0 {
            return Err(INVALID.into());
        }
        let font_key = state.font.ok_or(INVALID)?;
        let font = self.fonts.get(&font_key).cloned().ok_or(INVALID)?;
        let face = ttf_parser::Face::parse(&font.data, 0).map_err(|_| INVALID)?;
        let unit = f64::from(face.units_per_em());
        for pair in bytes.chunks_exact(2) {
            self.budget.step(0)?;
            let cid = u16::from_be_bytes([pair[0], pair[1]]);
            let gid = match &font.mapping {
                None => cid,
                Some(map) => {
                    let at = cid as usize * 2;
                    let b = map.get(at..at + 2).ok_or(INVALID)?;
                    u16::from_be_bytes([b[0], b[1]])
                }
            };
            if gid == 0 || gid >= face.number_of_glyphs() {
                return Err(INVALID.into());
            }
            let glyph = ttf_parser::GlyphId(gid);
            let advance = face.glyph_hor_advance(glyph).ok_or(INVALID)? as f64 / unit * 1000.;
            let width = font.widths.get(&cid).copied().unwrap_or(font.default_width);
            if (advance - width).abs() > 0.002 {
                return Err(UNSUPPORTED.into());
            }
            let outline = if let Some(bounds) = self.outlines.get(&(font_key, gid)) {
                *bounds
            } else {
                let mut outline = Outline::default();
                let result = face.outline_glyph(glyph, &mut outline);
                if outline.bad || outline.bounds.is_some() && result.is_none() {
                    return Err(INVALID.into());
                }
                self.budget.0 = self.budget.0.checked_sub(outline.points).ok_or(LIMIT)?;
                self.outlines.insert((font_key, gid), outline.bounds);
                outline.bounds
            };
            let text = state.ctm.concat(state.tm);
            if !text.finite() {
                return Err(INVALID.into());
            }
            if let Some(Mark::Content(p, m)) = marks.last() {
                let site = self.sites.get(&(*p, *m)).ok_or(INVALID)?;
                if site.rise != 0. && site.protected.is_none() && !text.axis_aligned() {
                    return Err(UNSUPPORTED.into());
                }
            }
            // Identity-H has only two-byte codes: PDF Tw applies to a
            // single-byte code 32, never to a two-byte CID whose value is 32.
            let distance = (width / 1000. * state.size + state.char_space) * state.hscale;
            match marks.last().copied().ok_or(UNSUPPORTED)? {
                Mark::Content(p, m) => {
                    if self.coverage.len() >= 2_000_000 {
                        return Err(LIMIT.into());
                    }
                    self.coverage.push(Coverage {
                        key: (p, m),
                        from: text.point(0., state.rise),
                        to: text.point(distance, state.rise),
                        has_ink: outline.is_some(),
                    });
                }
                Mark::Artifact => (),
            }
            if let Some(rect) = outline {
                let glyph_matrix = text.concat(Matrix([
                    state.size * state.hscale / unit,
                    0.,
                    0.,
                    state.size / unit,
                    0.,
                    state.rise,
                ]));
                let origin = text.point(0., state.rise);
                self.emit(
                    page,
                    state,
                    marks,
                    rect.transformed(glyph_matrix),
                    Some(origin),
                    true,
                )?;
            }
            state.tm = state.tm.translate(distance, 0.);
            if !state.tm.finite() {
                return Err(INVALID.into());
            }
        }
        Ok(())
    }
}

fn geometry(
    source: Option<&str>,
    page: ObjectId,
    page_number: u32,
    reason: &str,
    ink: Bounds,
    other: Option<&str>,
) -> String {
    format!(
        "publication_layout_geometry: {}",
        serde_json::json!({"coordinateSpace":"pdfBaselineInk","blockId":source,"pageObject":page.0,"page":page_number,"reason":reason,"ink":ink.0,"otherBlockId":other})
    )
}
fn unique_fragment(
    fragments: &[&Annotation],
    page: ObjectId,
    from: (f64, f64),
    to: (f64, f64),
    rise: f64,
    budget: &mut Budget,
) -> Result<usize, String> {
    let mut found = None;
    for (i, f) in fragments.iter().enumerate() {
        budget.step(0)?;
        if f.page_id != page {
            continue;
        }
        let inside = |p: (f64, f64)| {
            p.0 >= f.rect[0] - EPS
                && p.0 <= f.rect[2] + EPS
                && p.1 - rise >= f.rect[1] - EPS
                && p.1 - rise <= f.rect[3] + EPS
        };
        if inside(from) && inside(to) {
            if found.replace(i).is_some() {
                return Err(INVALID.into());
            }
        }
    }
    found.ok_or_else(|| INVALID.into())
}

pub(super) fn verify(
    doc: &Document,
    pages: &[Page],
    graph: &Graph,
    proof: &Proof,
    requests: &BTreeMap<String, &BaselineLinkRequest>,
    annotations: &[Annotation],
    budget: &mut Budget,
) -> Result<PaintProof, String> {
    let inventory = graph.paint_inventory()?;
    let mut sites = BTreeMap::new();
    for (key, tag) in inventory {
        budget.step(0)?;
        let source = super::owner(graph, &proof.wrappers, tag)?;
        let mut protected = None;
        let mut at = tag;
        let mut ordinary_structure = true;
        let mut inside_source = true;
        for _ in 0..MAX_DEPTH {
            if proof.protected.contains_key(&at) && protected.replace(at).is_some() {
                return Err(INVALID.into());
            }
            if at == graph.root {
                break;
            }
            let entry = graph.tags.get(&at).ok_or(INVALID)?;
            if proof.wrappers.contains_key(&at) {
                inside_source = false;
            }
            if inside_source
                && !matches!(
                    entry.role.as_slice(),
                    b"Span" | b"P" | b"Strong" | b"Em" | b"Link" | b"Document"
                )
            {
                ordinary_structure = false;
            }
            at = entry.parent;
        }
        if source.as_ref().is_some_and(|s| requests.contains_key(s))
            && protected.is_none()
            && !ordinary_structure
        {
            return Err(UNSUPPORTED.into());
        }
        let rise = source
            .as_ref()
            .and_then(|s| requests.get(s))
            .map_or(0., |r| r.rise_pt);
        sites.insert(
            key,
            Site {
                source,
                rise,
                protected,
            },
        );
    }
    let page_numbers = pages
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id, i as u32 + 1))
        .collect();
    let page_media = pages.iter().map(|p| (p.id, Bounds(p.media))).collect();
    let mut reader = Reader {
        doc,
        budget,
        sites,
        seen: BTreeSet::new(),
        painted: BTreeSet::new(),
        fonts: BTreeMap::new(),
        outlines: BTreeMap::new(),
        expanded: 0,
        operations: 0,
        forms: BTreeSet::new(),
        ink: BTreeMap::new(),
        ordinary: BTreeSet::new(),
        protected: BTreeSet::new(),
        coverage: Vec::new(),
        page_numbers,
        page_media,
    };
    for page in pages {
        let d = doc.get_dictionary(page.id).map_err(|_| INVALID)?;
        let resources = page_resources(doc, page.id)?;
        let mut bytes = Vec::new();
        if let Ok(v) = d.get(b"Contents") {
            reader.contents(v, &mut bytes, 0)?;
        }
        reader.paint(
            page.id,
            &bytes,
            resources,
            State::new(page.media),
            Vec::new(),
            0,
            false,
        )?;
    }
    if reader.seen.len() != reader.sites.len()
        || reader.sites.keys().any(|k| !reader.seen.contains(k))
    {
        return Err(INVALID.into());
    }
    for source in requests.keys() {
        if !reader.ordinary.contains(source) {
            return Err(UNSUPPORTED.into());
        }
    }
    let mut coverage_by_key = BTreeMap::<_, Vec<&Coverage>>::new();
    for glyph in &reader.coverage {
        reader.budget.step(0)?;
        coverage_by_key.entry(glyph.key).or_default().push(glyph);
    }
    let mut protected_keys = BTreeMap::<ObjectId, Vec<(ObjectId, i64)>>::new();
    for (key, site) in &reader.sites {
        if let Some(tag) = site.protected {
            protected_keys.entry(tag).or_default().push(*key);
        }
    }
    let mut fragments_by_tag = BTreeMap::<ObjectId, Vec<&Annotation>>::new();
    for a in annotations {
        reader.budget.step(0)?;
        let tag = graph.objrs.get(&a.id).ok_or(INVALID)?.tag;
        // A source witness can be attached to a descendant real/protected
        // Link at a page break. It is not a real click fragment or that
        // protected object's own witness; identity is already proven above.
        if let Some(witness) = &a.temporary {
            if proof.protected.get(&tag) != Some(witness) { continue; }
        }
        fragments_by_tag
            .entry(tag)
            .or_default()
            .push(a);
    }
    let mut ink_by_protection = BTreeMap::<ObjectId, Vec<(&Group, &Ink)>>::new();
    for (group, ink) in &reader.ink {
        if let Some(tag) = group.protection {
            if group.mark.is_some() {
                ink_by_protection.entry(tag).or_default().push((group, ink));
            }
        }
    }
    for (tag, witness) in &proof.protected {
        if !reader.protected.contains(tag) {
            return Err(UNSUPPORTED.into());
        }
        let keys = protected_keys.get(tag).ok_or(INVALID)?;
        let fragments = fragments_by_tag.get(tag).ok_or(INVALID)?;
        if fragments
            .iter()
            .any(|a| a.temporary.as_ref() != Some(witness))
        {
            return Err(INVALID.into());
        }
        let content_pages = keys
            .iter()
            .filter(|k| reader.painted.contains(k))
            .map(|(p, _)| *p)
            .collect::<BTreeSet<_>>();
        let annotation_pages = fragments.iter().map(|a| a.page_id).collect::<BTreeSet<_>>();
        if content_pages != annotation_pages {
            return Err(INVALID.into());
        }
        let mut covered = vec![false; fragments.len()];
        for key in keys {
            for glyph in coverage_by_key.get(key).into_iter().flatten() {
                reader.budget.step(0)?;
                // Empty outlines retain validated CID/GID, width and source
                // coverage, but their logical advance is not clickable ink.
                if !glyph.has_ink { continue; }
                covered[unique_fragment(
                    fragments,
                    glyph.key.0,
                    glyph.from,
                    glyph.to,
                    0.,
                    reader.budget,
                )?] = true;
            }
        }
        // Vector equations contain paths rather than text-show operands.
        for (group, ink) in ink_by_protection.get(tag).into_iter().flatten() {
            reader.budget.step(0)?;
            let r = ink.bounds.0;
            covered[unique_fragment(
                fragments,
                group.page,
                (r[0], r[1]),
                (r[2], r[3]),
                0.,
                reader.budget,
            )?] = true;
        }
        if covered.iter().any(|yes| !*yes) {
            return Err(INVALID.into());
        }
    }
    // Native Link ownership is already established by OBJR/ParentTree and
    // the unique source ancestor. Rectangles below prove click coverage only;
    // neither a URL nor rectangle geometry is allowed to select an owner.
    for (tag, entry) in &graph.tags {
        if entry.role != b"Link"
            || proof.wrappers.contains_key(tag)
            || proof.protected.contains_key(tag)
        {
            continue;
        }
        let Some(source) = super::owner(graph, &proof.wrappers, *tag)? else {
            continue;
        };
        let Some(request) = requests.get(&source) else {
            continue;
        };
        let keys = graph
            .link_content_inventory(*tag, &proof.temporary_annotations, reader.budget)?
            .into_iter()
            .collect::<BTreeSet<_>>();
        let fragments = fragments_by_tag.get(tag).ok_or(INVALID)?;
        if fragments.is_empty() {
            return Err(INVALID.into());
        }
        let mut covered = vec![false; fragments.len()];
        let mut content_seen = BTreeSet::new();
        for key in &keys {
            for glyph in coverage_by_key.get(key).ok_or(INVALID)? {
                reader.budget.step(0)?;
                content_seen.insert(glyph.key);
                // Typst retains the trailing blank's advance after a wrapped
                // link. Each real fragment still needs visible coverage.
                if !glyph.has_ink { continue; }
                covered[unique_fragment(
                    fragments,
                    glyph.key.0,
                    glyph.from,
                    glyph.to,
                    request.rise_pt,
                    reader.budget,
                ).map_err(|error|{#[cfg(test)]eprintln!("[Typst link coverage] tag={tag:?} key={:?} from={:?} to={:?} rise={} fragments={:?}",glyph.key,glyph.from,glyph.to,request.rise_pt,fragments.iter().map(|a|(a.page_id,a.rect)).collect::<Vec<_>>());error})?] = true;
            }
        }
        if content_seen != keys || covered.iter().any(|yes| !*yes) {
            return Err(INVALID.into());
        }
    }
    let media = pages
        .iter()
        .map(|p| (p.id, Bounds(p.media)))
        .collect::<BTreeMap<_, _>>();
    let mut ordered = reader.ink.iter().collect::<Vec<_>>();
    ordered.sort_by(|(a, x), (b, y)| {
        a.page
            .cmp(&b.page)
            .then_with(|| x.bounds.0[1].total_cmp(&y.bounds.0[1]))
    });
    let mut active = Vec::<(&Group, &Ink)>::new();
    for (group, ink) in ordered {
        reader.budget.step(0)?;
        if !ink.bounds.inside(*media.get(&group.page).ok_or(INVALID)?) {
            return Err(geometry(
                ink.source.as_deref(),
                group.page,
                *reader.page_numbers.get(&group.page).ok_or(INVALID)?,
                "paper-edge",
                ink.bounds,
                None,
            ));
        }
        active.retain(|(g, v)| g.page == group.page && v.bounds.0[3] > ink.bounds.0[1] + EPS);
        for (previous_group, previous) in &active {
            reader.budget.step(0)?;
            // Kerning and combining glyphs within one actual MCID text run
            // share its baseline. Distinct runs still get collision checks.
            let same_text_line = group.glyph.is_some()
                && previous_group.glyph.is_some()
                && group.source.is_some()
                && group.source == previous_group.source
                && group.protection.is_none()
                && previous_group.protection.is_none()
                && group.line == previous_group.line
                && group.mark == previous_group.mark;
            if !same_text_line
                && (ink.changed || previous.changed)
                && ink.bounds.overlaps(previous.bounds)
            {
                return Err(geometry(
                    ink.source.as_deref(),
                    group.page,
                    *reader.page_numbers.get(&group.page).ok_or(INVALID)?,
                    "ink-collision",
                    ink.bounds,
                    previous.source.as_deref(),
                ));
            }
        }
        active.push((group, ink));
    }
    // Only proven visible paint contributes. Protection keeps the parent
    // source identity; artifact/unknown-source paint cannot publish a hit.
    // A source spanning pages must never merge their coordinate frames.
    let mut source_ink = BTreeMap::<(ObjectId, String), Bounds>::new();
    for (group, ink) in &reader.ink {
        reader.budget.step(0)?;
        if let Some(source) = &group.source {
            source_ink.entry((group.page, source.clone()))
                .and_modify(|bounds| *bounds = bounds.union(ink.bounds))
                .or_insert(ink.bounds);
            if source_ink.len() > MAX_LINKS {
                return Err(LIMIT.into());
            }
        }
    }
    Ok(PaintProof {
        snapshot: snapshot(doc, pages, reader.budget)?,
        source_ink,
    })
}

fn page_resources(doc: &Document, mut id: ObjectId) -> Result<&Dictionary, String> {
    let mut seen = BTreeSet::new();
    for _ in 0..MAX_DEPTH {
        if !seen.insert(id) {
            return Err(INVALID.into());
        }
        let d = doc.get_dictionary(id).map_err(|_| INVALID)?;
        if let Ok(v) = d.get(b"Resources") {
            return dictionary(doc, v);
        }
        id = d
            .get(b"Parent")
            .and_then(Object::as_reference)
            .map_err(|_| INVALID)?;
    }
    Err(LIMIT.into())
}

fn snapshot(doc: &Document, pages: &[Page], budget: &mut Budget) -> Result<[u8; 32], String> {
    fn hash(
        doc: &Document,
        v: &Object,
        h: &mut Sha256,
        seen: &mut BTreeSet<ObjectId>,
        depth: usize,
        budget: &mut Budget,
    ) -> Result<(), String> {
        budget.step(depth)?;
        match v {
            Object::Reference(id) => {
                h.update(b"ref");
                h.update(id.0.to_be_bytes());
                h.update(id.1.to_be_bytes());
                if seen.insert(*id) {
                    hash(
                        doc,
                        doc.objects.get(id).ok_or(INVALID)?,
                        h,
                        seen,
                        depth + 1,
                        budget,
                    )?;
                }
            }
            Object::Null => h.update(b"null"),
            Object::Boolean(b) => h.update(if *b {
                b"true".as_slice()
            } else {
                b"false".as_slice()
            }),
            Object::Integer(_) | Object::Real(_) => {
                h.update(b"number");
                let n = number(v)?;
                h.update((if n == 0. { 0. } else { n }).to_bits().to_be_bytes());
            }
            Object::Name(b) | Object::String(b, _) => {
                h.update(if matches!(v, Object::Name(_)) {
                    b"name".as_slice()
                } else {
                    b"string".as_slice()
                });
                h.update((b.len() as u64).to_be_bytes());
                h.update(b);
            }
            Object::Array(values) => {
                h.update(b"array");
                h.update((values.len() as u64).to_be_bytes());
                for v in values {
                    hash(doc, v, h, seen, depth + 1, budget)?;
                }
            }
            Object::Dictionary(d) => {
                h.update(b"dict");
                let mut entries = d.iter().collect::<Vec<_>>();
                entries.sort_by_key(|(k, _)| *k);
                for (k, v) in entries {
                    h.update((k.len() as u64).to_be_bytes());
                    h.update(k);
                    hash(doc, v, h, seen, depth + 1, budget)?;
                }
            }
            Object::Stream(s) => {
                hash(
                    doc,
                    &Object::Dictionary(s.dict.clone()),
                    h,
                    seen,
                    depth + 1,
                    budget,
                )?;
                h.update(b"stream");
                h.update((s.content.len() as u64).to_be_bytes());
                h.update(&s.content);
            }
        }
        Ok(())
    }
    let mut h = Sha256::new();
    let mut seen = BTreeSet::new();
    for page in pages {
        h.update(b"page");
        for n in page.media {
            h.update(n.to_bits().to_be_bytes());
        }
        let d = doc.get_dictionary(page.id).map_err(|_| INVALID)?;
        hash(
            doc,
            &Object::Dictionary(page_resources(doc, page.id)?.clone()),
            &mut h,
            &mut seen,
            0,
            budget,
        )?;
        if let Ok(v) = d.get(b"Contents") {
            hash(doc, v, &mut h, &mut seen, 0, budget)?;
        }
    }
    Ok(h.finalize().into())
}
