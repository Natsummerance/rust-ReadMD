//! Vector formula layout for the PDF writer.
//!
//! LaTeX → OMML ([`crate::latex2omml`]) → node tree ([`crate::omml`]) → boxes
//! laid out with the OpenType `MATH` constants of the bundled STIX Two Math font
//! → glyph outlines as PDF
//! path operators.  Formulas are therefore drawn as vector art: sharp at any
//! zoom and independent of fonts installed on the reader's machine. The PDF
//! writer adds a font-backed, unpainted text layer for copying/searching the
//! formula's LaTeX projection; ActualText around paths alone is insufficient.
//!
//! Characters the math font lacks (CJK inside `\text{}`) are taken from a
//! managed optional text font. When required glyphs are absent or a formula fails
//! to parse, the caller falls back to text and reports it.

use crate::omml::{self, Node, MathStyle as Style};
use crate::math_equation_array::{self as equation_array, ColumnMeasure, EquationArrayDiagnostic, MarkerKind};
use std::collections::{HashMap,BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use ttf_parser::{GlyphId, OutlineBuilder};

/// Positions and sizes below are in em units of the formula's base size
/// (1.0 = font size) until [`Formula::draw`] scales them to points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Paint {
    Unresolved,
    Auto,
    Rgb([u8; 3]),
}
impl Paint {
    fn from_color(color: Option<[u8; 3]>) -> Self { color.map(Self::Rgb).unwrap_or(Self::Auto) }
    fn svg(self, fallback: &str) -> String {
        match self { Self::Rgb([r,g,b]) => format!("#{r:02X}{g:02X}{b:02X}"), _ => fallback.into() }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Item {
    Glyph { font: u8, gid: u16, x: f64, y: f64, scale: f64, shear:f64, x_scale:f64, color: Paint },
    Rule { x: f64, y: f64, w: f64, h: f64, color: Paint },
    Line { x1:f64, y1:f64, x2:f64, y2:f64, thickness:f64, color: Paint },
}
impl Item {
    fn paint(&self) -> Paint { match *self { Self::Glyph{color,..}|Self::Rule{color,..}|Self::Line{color,..} => color } }
    fn resolve_paint(&mut self, paint: Paint) {
        let (Self::Glyph{color,..}|Self::Rule{color,..}|Self::Line{color,..}) = self;
        if *color == Paint::Unresolved { *color = paint; }
    }
}

/// Source-bound selection regions are independent of painted paths. Their
/// item ranges also let clients measure the actual ink of an operator.
#[derive(Debug, Clone, PartialEq)]
struct Region {
    id: String, kind: String, part: bool,
    x: f64, y: f64, w: f64, h: f64, d: f64,
    from: usize, to: usize,
    character: Option<char>,
    /// UTF-16 range within the owning Office run's text, never a glyph index.
    text_range: Option<(usize, usize)>,
}

/// Balanced interval index avoids rescanning every source region for every
/// wrapped line (a single Office run may own thousands of visible fragments).
struct RegionIndex<'a>{regions:Vec<&'a Region>,max_to:Vec<usize>}
impl<'a> RegionIndex<'a>{
    fn new(regions:&'a[Region])->Self{
        let mut regions=regions.iter().collect::<Vec<_>>();regions.sort_by_key(|r|(r.from,r.to));
        let mut index=Self{max_to:vec![0;regions.len().saturating_mul(4).max(1)],regions};
        if !index.regions.is_empty(){index.build(0,0,index.regions.len());}index
    }
    fn build(&mut self,node:usize,lo:usize,hi:usize)->usize{
        let value=if hi-lo==1{self.regions[lo].to.saturating_add(1)}else{let mid=(lo+hi)/2;self.build(node*2+1,lo,mid).max(self.build(node*2+2,mid,hi))};
        self.max_to[node]=value;value
    }
    fn overlapping(&self,from:usize,to:usize)->Vec<&'a Region>{
        fn visit<'a>(s:&RegionIndex<'a>,node:usize,lo:usize,hi:usize,from:usize,to:usize,out:&mut Vec<&'a Region>){
            if s.max_to[node]<=from||s.regions[lo].from>to{return;}
            if hi-lo==1{let r=s.regions[lo];if r.from<to&&r.to>from||r.from==r.to&&r.from>=from&&r.from<=to{out.push(r);}return;}
            let mid=(lo+hi)/2;visit(s,node*2+1,lo,mid,from,to,out);visit(s,node*2+2,mid,hi,from,to,out);
        }
        let mut out=Vec::new();if !self.regions.is_empty(){visit(self,0,0,self.regions.len(),from,to,&mut out);}out
    }
}

#[derive(Debug, Clone, Default)]
struct MBox {
    w: f64,
    /// Height above the baseline.
    h: f64,
    /// Depth below the baseline (positive).
    d: f64,
    items: Vec<Item>,
    regions: Vec<Region>,
    /// Italic correction of the last glyph (for superscripts).
    italic: f64,
    class: Option<omml::Class>,
    /// A single large operator (for limits placement).
    large_op: bool,
    /// Effective em at this atom. Keeps inter-atom spacing at the actual Word
    /// size without scaling child operands or explicit sizes a second time.
    em: Option<f64>,
    /// Normalized first/last semantic classes of this atom in its parent row.
    spacing: Option<(Option<omml::Class>, Option<omml::Class>)>,
}

#[derive(Clone, Copy)]
enum KernCorner { TopRight, TopLeft, BottomRight, BottomLeft }

impl MBox {
    fn shifted(mut self, dx: f64, dy: f64) -> MBox {
        for region in &mut self.regions { region.x+=dx;region.y+=dy; }
        for it in &mut self.items {
            match it {
                Item::Line {x1,y1,x2,y2,..} => {*x1+=dx;*x2+=dx;*y1+=dy;*y2+=dy;},
                Item::Glyph { x, y, .. } | Item::Rule { x, y, .. } => {
                    *x += dx;
                    *y += dy;
                }
            }
        }
        self
    }

    fn append(&mut self, other: MBox, dx: f64, dy: f64) {
        self.h = self.h.max(other.h + dy);
        self.d = self.d.max(other.d - dy);
        let mut o = other.shifted(dx, dy);
        let offset=self.items.len();
        for region in &mut o.regions { region.from+=offset;region.to+=offset; }
        self.items.extend(o.items);
        self.regions.extend(o.regions);
    }

    fn mark(mut self, n: &Node) -> Self {
        if let Some(id)=n.attr("readmdId").filter(|id|id.starts_with("rd-formula-token-omml-")&&id.bytes().all(|c|c.is_ascii_alphanumeric()||c==b'-')) {
            if !self.regions.iter().any(|r|r.id==id) {
                self.regions.push(Region{id:id.into(),kind:n.name.clone(),part:false,x:0.0,y:0.0,w:self.w,h:self.h,d:self.d,from:0,to:self.items.len(),character:None,text_range:None});
            }
        }
        self
    }

    fn part(mut self, kind: &str) -> Self {
        self.regions.push(Region{id:String::new(),kind:kind.into(),part:true,x:0.0,y:0.0,w:self.w,h:self.h,d:self.d,from:0,to:self.items.len(),character:None,text_range:None});self
    }

    fn text_fragment(mut self, owner: Option<&str>, from: usize, to: usize) -> Self {
        if let Some(owner)=owner {
            let (h,d)=if self.h+self.d==0.0{(0.8,0.2)}else{(self.h,self.d)};
            self.regions.push(Region{id:format!("{owner}-text-{from}"),kind:"run-text".into(),part:false,x:0.0,y:0.0,w:self.w,h,d,from:0,to:self.items.len(),character:None,text_range:Some((from,to))});
        }
        self
    }

    fn character(mut self, kind: &str, character:char) -> Self {
        self=self.part(kind);self.regions.last_mut().unwrap().character=Some(character);self
    }

    fn equation_marker(mut self, kind: MarkerKind, owner: Option<&str>, text_range: Option<(usize, usize)>) -> Self {
        self.regions.push(Region { id: owner.unwrap_or("").into(),
            kind: match kind { MarkerKind::Alignment => "eqarr-marker-align", MarkerKind::Separator => "eqarr-marker-separator" }.into(),
            part: true, x: 0.0, y: 0.0, w: 0.0, h: 0.0, d: 0.0, from: 0, to: 0, character: None, text_range });
        self
    }

    fn empty() -> MBox {
        MBox::default()
    }
}

// ------------------------------------------------------------------ fonts

#[derive(Clone, PartialEq)]
struct MathFont {
    data: Arc<Vec<u8>>,
    index: u32,
    upem: f64,
    family: String,
    bold: bool,
}

impl MathFont {
    fn face(&self) -> Option<ttf_parser::Face<'_>> {
        ttf_parser::Face::parse(&self.data, self.index).ok()
    }
}

/// Font 0 is the math font, font 1 the text fallback for anything it lacks.
#[derive(Debug, Clone, PartialEq)]
struct Fonts {
    fonts: Vec<MathFont>,
    c: Constants,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Constants {
    axis: f64,
    rule: f64,
    num_up: f64,
    num_up_d: f64,
    den_down: f64,
    den_down_d: f64,
    num_gap: f64,
    num_gap_d: f64,
    den_gap: f64,
    den_gap_d: f64,
    stack_up: f64,
    stack_up_d: f64,
    stack_down: f64,
    stack_down_d: f64,
    stack_gap: f64,
    stack_gap_d: f64,
    skew_hgap: f64,
    skew_vgap: f64,
    sup_up: f64,
    sup_up_cramped: f64,
    sup_bottom_min: f64,
    sub_down: f64,
    sub_top_max: f64,
    sub_sup_gap: f64,
    sup_bottom_max_with_sub: f64,
    sup_drop_max: f64,
    sub_drop_min: f64,
    space_after_script: f64,
    rad_gap: f64,
    rad_gap_d: f64,
    rad_rule: f64,
    rad_extra: f64,
    rad_before: f64,
    rad_after: f64,
    rad_degree_raise: f64,
    upper_gap: f64,
    upper_rise: f64,
    lower_gap: f64,
    lower_drop: f64,
    script_scale: f64,
    script_script_scale: f64,
    display_op_min: f64,
    overbar_gap: f64,
    overbar_rule: f64,
    overbar_extra: f64,
    underbar_gap: f64,
    underbar_rule: f64,
    underbar_extra: f64,
    accent_base: f64,
    delim_min: f64,
}

impl Constants {
    /// TeX-like defaults, used for any value a font leaves out.
    fn defaults() -> Constants {
        Constants {
            axis: 0.25,
            rule: 0.05,
            num_up: 0.39,
            num_up_d: 0.68,
            den_down: 0.34,
            den_down_d: 0.69,
            num_gap: 0.05,
            num_gap_d: 0.15,
            den_gap: 0.05,
            den_gap_d: 0.15,
            stack_up: 0.39,
            stack_up_d: 0.68,
            stack_down: 0.34,
            stack_down_d: 0.69,
            stack_gap: 0.15,
            stack_gap_d: 0.35,
            skew_hgap: 0.3,
            skew_vgap: 0.1,
            sup_up: 0.36,
            sup_up_cramped: 0.29,
            sup_bottom_min: 0.11,
            sub_down: 0.2,
            sub_top_max: 0.35,
            sub_sup_gap: 0.2,
            sup_bottom_max_with_sub: 0.38,
            sup_drop_max: 0.39,
            sub_drop_min: 0.05,
            space_after_script: 0.05,
            rad_gap: 0.06,
            rad_gap_d: 0.15,
            rad_rule: 0.05,
            rad_extra: 0.05,
            rad_before: 5.0 / 18.0,
            rad_after: -10.0 / 18.0,
            rad_degree_raise: 0.6,
            upper_gap: 0.11,
            upper_rise: 0.11,
            lower_gap: 0.17,
            lower_drop: 0.6,
            script_scale: 0.7,
            script_script_scale: 0.5,
            display_op_min: 1.3,
            overbar_gap: 0.15,
            overbar_rule: 0.05,
            overbar_extra: 0.05,
            underbar_gap: 0.15,
            underbar_rule: 0.05,
            underbar_extra: 0.05,
            accent_base: 0.45,
            delim_min: 1.3,
        }
    }
}

fn load_constants(f: &MathFont) -> Constants {
    let mut c = Constants::defaults();
    let Some(face) = f.face() else { return c };
    if let Some(h) = face.x_height().filter(|h| *h > 0) { c.accent_base = f64::from(h) / f.upem; }
    let Some(k) = face.tables().math.and_then(|m| m.constants) else { return c };
    let e = |v: ttf_parser::math::MathValue| v.value as f64 / f.upem;
    c.axis = e(k.axis_height());
    c.rule = e(k.fraction_rule_thickness());
    c.num_up = e(k.fraction_numerator_shift_up());
    c.num_up_d = e(k.fraction_numerator_display_style_shift_up());
    c.den_down = e(k.fraction_denominator_shift_down());
    c.den_down_d = e(k.fraction_denominator_display_style_shift_down());
    c.num_gap = e(k.fraction_numerator_gap_min());
    c.num_gap_d = e(k.fraction_num_display_style_gap_min());
    c.den_gap = e(k.fraction_denominator_gap_min());
    c.den_gap_d = e(k.fraction_denom_display_style_gap_min());
    c.stack_up = e(k.stack_top_shift_up());
    c.stack_up_d = e(k.stack_top_display_style_shift_up());
    c.stack_down = e(k.stack_bottom_shift_down());
    c.stack_down_d = e(k.stack_bottom_display_style_shift_down());
    c.stack_gap = e(k.stack_gap_min());
    c.stack_gap_d = e(k.stack_display_style_gap_min());
    c.skew_hgap = e(k.skewed_fraction_horizontal_gap());
    c.skew_vgap = e(k.skewed_fraction_vertical_gap());
    c.sup_up = e(k.superscript_shift_up());
    c.sup_up_cramped = e(k.superscript_shift_up_cramped());
    c.sup_bottom_min = e(k.superscript_bottom_min());
    c.sub_down = e(k.subscript_shift_down());
    c.sub_top_max = e(k.subscript_top_max());
    c.sub_sup_gap = e(k.sub_superscript_gap_min());
    c.sup_bottom_max_with_sub = e(k.superscript_bottom_max_with_subscript());
    c.sup_drop_max = e(k.superscript_baseline_drop_max());
    c.sub_drop_min = e(k.subscript_baseline_drop_min());
    c.space_after_script = e(k.space_after_script());
    c.rad_gap = e(k.radical_vertical_gap());
    c.rad_gap_d = e(k.radical_display_style_vertical_gap());
    c.rad_rule = e(k.radical_rule_thickness());
    c.rad_extra = e(k.radical_extra_ascender());
    c.rad_before = e(k.radical_kern_before_degree());
    c.rad_after = e(k.radical_kern_after_degree());
    // Zero is a valid explicit percentage, not a missing font value.
    c.rad_degree_raise = f64::from(k.radical_degree_bottom_raise_percent()) / 100.0;
    c.upper_gap = e(k.upper_limit_gap_min());
    c.upper_rise = e(k.upper_limit_baseline_rise_min());
    c.lower_gap = e(k.lower_limit_gap_min());
    c.lower_drop = e(k.lower_limit_baseline_drop_min());
    c.overbar_gap = e(k.overbar_vertical_gap());
    c.overbar_rule = e(k.overbar_rule_thickness());
    c.overbar_extra = e(k.overbar_extra_ascender());
    c.underbar_gap = e(k.underbar_vertical_gap());
    c.underbar_rule = e(k.underbar_rule_thickness());
    c.underbar_extra = e(k.underbar_extra_descender());
    c.accent_base = e(k.accent_base_height());
    let pct = |p: i16, d: f64| if p > 0 { p as f64 / 100.0 } else { d };
    c.script_scale = pct(k.script_percent_scale_down(), 0.7);
    c.script_script_scale = pct(k.script_script_percent_scale_down(), 0.5);
    let m = k.display_operator_min_height() as f64 / f.upem;
    if m > 0.0 {
        c.display_op_min = m;
    }
    let dm = k.delimited_sub_formula_min_height() as f64 / f.upem;
    if dm > 0.0 {
        c.delim_min = dm;
    }
    c
}

fn open(path: &PathBuf, index: u32, need_math: bool) -> Option<MathFont> {
    let data = Arc::new(std::fs::read(path).ok()?);
    let face = ttf_parser::Face::parse(&data, index).ok()?;
    if need_math && face.tables().math.is_none() {
        return None;
    }
    if matches!(face.permissions(), Some(ttf_parser::Permissions::Restricted)) {
        return None;
    }
    let upem = face.units_per_em() as f64;
    let family=face.names().into_iter().filter(|n|n.name_id==ttf_parser::name_id::FAMILY).find_map(|n|n.to_string()).unwrap_or_default();
    drop(face);
    Some(MathFont { data, index, upem, family, bold:false })
}

fn fonts(need_text:bool) -> Option<Arc<Fonts>> {
    static CORE:OnceLock<Option<Arc<Fonts>>>=OnceLock::new();
    let core=CORE.get_or_init(||{
        let data=Arc::new(crate::publication_fonts::MATH.to_vec());
        let face=ttf_parser::Face::parse(&data,0).ok()?;
        let upem=face.units_per_em() as f64;
        let math=MathFont{data,index:0,upem,family:"STIX Two Math".into(),bold:false};let c=load_constants(&math);
        Some(Arc::new(Fonts{fonts:vec![math],c}))
    }).as_ref()?.clone();
    if !need_text{return Some(core);}
    let Some(resources)=crate::publication_fonts::current()else{return Some(core);};
    // Keep the parsed fallback only for the active export thread. A newly
    // installed or repaired pack changes its private identity immediately.
    thread_local!{static TEXT:std::cell::RefCell<Option<(PathBuf,String,Arc<Fonts>)>>=const{std::cell::RefCell::new(None)};}
    TEXT.with(|cache|{
        if let Some((path,revision,fonts))=&*cache.borrow(){if path==&resources.sans&&revision==&resources.fingerprint{return Some(fonts.clone());}}
        let mut selected=(*core).clone();
        if let Some(text)=open(&resources.sans,0,false){selected.fonts.push(text);}else{return Some(core);}
        let fonts=Arc::new(selected);*cache.borrow_mut()=Some((resources.sans.clone(),resources.fingerprint.clone(),fonts.clone()));Some(fonts)
    })
}

fn requested_fonts(root:&Node)->BTreeSet<(String,bool)>{
    fn visit(n:&Node,out:&mut BTreeSet<(String,bool)>){if n.is_math()&&n.name=="r"{let style=omml::run_style(n);if style.normal_text{let slots=omml::run_fonts(n);for c in n.run_text().chars(){if let Some(f)=slots.family(c){out.insert((crate::publication_fonts::family(f).into(),style.bold));}}}}for c in &n.children{visit(c,out);}}
    let mut out=BTreeSet::new();visit(root,&mut out);out
}
pub(crate) fn mathml_fonts(root:&Node)->BTreeSet<(String,bool)>{
    fn visit(n:&Node,out:&mut BTreeSet<(String,bool)>){
        if n.is_math()&&n.name=="r"{let style=omml::run_style(n);if style.normal_text{let slots=omml::run_fonts(n);for c in n.run_text().chars(){
            if let Some(f @ ("ReadMD Sans SC"|"ReadMD Serif SC"))=crate::publication_fonts::math_text_family(slots.family(c),c){out.insert((f.into(),style.bold));}
        }}}for c in &n.children{visit(c,out);}
    }
    let mut fonts=BTreeSet::new();visit(root,&mut fonts);fonts
}
fn fonts_for(root:&Node,need_text:bool)->Option<Arc<Fonts>>{
    let mut selected=fonts(need_text)?;let requested=requested_fonts(root);
    let Some(resources)=crate::publication_fonts::current()else{return Some(selected);};
    if requested.is_empty(){return Some(selected);}
    thread_local!{static FACES:std::cell::RefCell<HashMap<(PathBuf,String),MathFont>>=std::cell::RefCell::new(HashMap::new());}
    for (family,bold) in requested{
        if !matches!(family.as_str(),"ReadMD Sans SC"|"ReadMD Serif SC")||selected.fonts.iter().any(|f|f.family==family&&f.bold==bold){continue;}
        let mut path=if family=="ReadMD Serif SC"{resources.serif.clone()}else{resources.sans.clone()};
        if bold{path=path.with_file_name(path.file_name()?.to_string_lossy().replace("-Regular","-Bold"));}
        let key=(path.clone(),resources.fingerprint.clone());let font=FACES.with(|cache|{
            if let Some(f)=cache.borrow().get(&key){return Some(f.clone());}
            let mut font=open(&path,0,false)?;font.bold=bold;
            let mut cache=cache.borrow_mut();if cache.len()>=8{cache.clear();}cache.insert(key,font.clone());Some(font)
        });
        if let Some(font)=font{Arc::make_mut(&mut selected).fonts.push(font);}
    }Some(selected)
}

/// Managed resources are requested only when a formula actually uses them.
/// Core Greek/Latin formulas remain independent of optional fonts and locks.
pub(crate) fn needs_managed_fonts(root:&Node)->bool{
    fn cjk(n:&Node)->bool{crate::publication_fonts::needs_cjk(&n.text)||n.children.iter().any(cjk)}
    cjk(root)||requested_fonts(root).iter().any(|(f,_)|matches!(f.as_str(),"ReadMD Sans SC"|"ReadMD Serif SC"))
}

pub(crate) fn font_warnings(root:&Node)->Vec<serde_json::Value>{
    fn visit(n:&Node,out:&mut BTreeSet<(String,String)>){
        if n.is_math()&&n.name=="r"&&omml::run_style(n).normal_text{let slots=omml::run_fonts(n);for c in n.run_text().chars(){if let Some(requested)=slots.family(c){let family=crate::publication_fonts::family(requested);let available=family=="STIX Two Math"||crate::publication_fonts::current().is_some()&&matches!(family,"ReadMD Sans SC"|"ReadMD Serif SC");if !available||family!=requested{out.insert((requested.into(),if available{family.into()}else{"STIX Two Math".into()}));}}}}
        if n.is_math()&&n.name=="mathFont"{if let Some(f)=n.attr("val").filter(|v|*v!="STIX Two Math"){out.insert((f.into(),"STIX Two Math".into()));}}
        for c in &n.children{visit(c,out);}
    }
    let mut out=BTreeSet::new();visit(root,&mut out);let mut warnings=out.into_iter().map(|(font,actual)|serde_json::json!({"code":"font_fallback","font":font,"actual":actual})).collect::<Vec<_>>();
    if let Some(fonts)=fonts_for(root,needs_managed_fonts(root)){
        let faces=fonts.fonts.iter().filter_map(MathFont::face).collect::<Vec<_>>();let mut missing=BTreeSet::new();
        fn glyphs(n:&Node,faces:&[ttf_parser::Face<'_>],missing:&mut BTreeSet<char>){if n.is_math()&&n.name=="r"{for c in n.run_text().chars().filter(|c|!c.is_whitespace()&&!c.is_control()){if !faces.iter().any(|f|f.glyph_index(c).is_some_and(|g|g.0!=0)){missing.insert(c);}}}for c in &n.children{glyphs(c,faces,missing);}}
        glyphs(root,&faces,&mut missing);if !missing.is_empty(){warnings.push(serde_json::json!({"code":"glyph_missing","chars":missing.into_iter().take(48).collect::<String>()}));}
    }warnings
}
impl std::fmt::Debug for MathFont {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{f.debug_struct("MathFont").field("bytes",&self.data.len()).field("index",&self.index).field("upem",&self.upem).finish()}
}

/// Vector math comes with ReadMD and never searches host font directories.
pub fn available() -> bool { fonts(false).is_some() }

/// Literal whitespace uses the core font's actual logical advance, not an
/// operator-name thin-space policy. No STIX numeric value is a product constant.
pub(crate) fn literal_space_advance(character: char) -> Option<f64> {
    if !matches!(character, ' ' | '\u{2009}' | '\u{2005}' | '\u{2003}') { return None; }
    let fonts = fonts(false)?;
    let face = fonts.fonts[0].face()?;
    let glyph = face.glyph_index(character).filter(|glyph| glyph.0 != 0)?;
    Some(f64::from(face.glyph_hor_advance(glyph)?) / fonts.fonts[0].upem)
}

// ------------------------------------------------------------------ layout

impl Style {
    fn scale(&self, c: &Constants) -> f64 {
        match self.level {
            0 => 1.0,
            1 => c.script_scale,
            _ => c.script_script_scale,
        }
    }
}

/// Use the same bundled OpenType MATH script factors in MathML as in vectors.
pub(crate) fn script_scale(level:u8) -> f64 {
    fonts(false).map(|f|Style{display:false,level,cramped:false}.scale(&f.c)).unwrap_or(1.0)
}

struct Ctx<'f> {
    f: &'f Fonts,
    base_points:f64,
    available_width:Option<f64>,
    /// Immediate content slot, in base em; never an ambient page-width lookup.
    local_width: Option<f64>,
    measuring: bool,
    equation_diagnostics: Vec<EquationArrayDiagnostic>,
    faces: Vec<ttf_parser::Face<'f>>,
    /// (font, char) → (gid, advance em, h, d, italic)
    cache: HashMap<(u8, char), Option<(u16, f64, f64, f64, f64)>>,
    missing: bool,
    spacing_error: Option<&'static str>,
}

/// Math alphanumerics for `\mathbb` / `\mathcal` / bold / italic letters.
fn math_alnum(c: char, st: &omml::RunStyle, italic: bool) -> char {
    let off = |base: u32, c: char, from: char| char::from_u32(base + (c as u32 - from as u32));
    let exceptions = |c: char| -> Option<char> {
        Some(match c {
            'h' if italic => 'ℎ',
            _ => return None,
        })
    };
    let alphabet=|upper,lower,digit|match c{
        'A'..='Z'=>off(upper,c,'A'),'a'..='z'=>off(lower,c,'a'),
        '0'..='9' if digit!=0=>off(digit,c,'0'),_=>None,
    };
    let greek=|upper:u32,lower:u32|match c{
        'Α'..='Ρ'|'Σ'..='Ω'=>off(upper,c,'Α'),'α'..='ω'=>off(lower,c,'α'),
        'ϴ'=>char::from_u32(upper+17),'∇'=>char::from_u32(upper+25),
        '∂'=>char::from_u32(lower+25),'ϵ'=>char::from_u32(lower+26),'ϑ'=>char::from_u32(lower+27),
        'ϰ'=>char::from_u32(lower+28),'ϕ'=>char::from_u32(lower+29),'ϱ'=>char::from_u32(lower+30),'ϖ'=>char::from_u32(lower+31),
        _=>None,
    };
    if st.monospace{return alphabet(0x1D670,0x1D68A,0x1D7F6).unwrap_or(c);}
    if st.sans_serif{
        let(upper,lower,digit)=match(st.bold,italic){
            (true,true)=>(0x1D63C,0x1D656,0x1D7EC),(true,false)=>(0x1D5D4,0x1D5EE,0x1D7EC),
            (false,true)=>(0x1D608,0x1D622,0x1D7E2),_ =>(0x1D5A0,0x1D5BA,0x1D7E2),
        };
        return alphabet(upper,lower,digit).or_else(||if st.bold{if italic{greek(0x1D790,0x1D7AA)}else{greek(0x1D756,0x1D770)}}else{None}).unwrap_or(c);
    }
    if st.fraktur{
        if st.bold{return alphabet(0x1D56C,0x1D586,0).unwrap_or(c);}
        let special=match c{'C'=>Some('ℭ'),'H'=>Some('ℌ'),'I'=>Some('ℑ'),'R'=>Some('ℜ'),'Z'=>Some('ℨ'),_=>None};
        return special.or_else(||alphabet(0x1D504,0x1D51E,0)).unwrap_or(c);
    }
    if st.double_struck {
        let special = match c {
            'C' => Some('ℂ'),
            'H' => Some('ℍ'),
            'N' => Some('ℕ'),
            'P' => Some('ℙ'),
            'Q' => Some('ℚ'),
            'R' => Some('ℝ'),
            'Z' => Some('ℤ'),
            _ => None,
        };
        if let Some(s) = special {
            return s;
        }
        return match c {
            'A'..='Z' => off(0x1D538, c, 'A'),
            'a'..='z' => off(0x1D552, c, 'a'),
            '0'..='9' => off(0x1D7D8, c, '0'),
            _ => None,
        }
        .unwrap_or(c);
    }
    if st.script {
        if st.bold{return alphabet(0x1D4D0,0x1D4EA,0).unwrap_or(c);}
        let special = match c {
            'B' => Some('ℬ'),
            'E' => Some('ℰ'),
            'F' => Some('ℱ'),
            'H' => Some('ℋ'),
            'I' => Some('ℐ'),
            'L' => Some('ℒ'),
            'M' => Some('ℳ'),
            'R' => Some('ℛ'),
            'e' => Some('ℯ'),
            'g' => Some('ℊ'),
            'o' => Some('ℴ'),
            _ => None,
        };
        if let Some(s) = special {
            return s;
        }
        return match c {
            'A'..='Z' => off(0x1D49C, c, 'A'),
            'a'..='z' => off(0x1D4B6, c, 'a'),
            _ => None,
        }
        .unwrap_or(c);
    }
    match (st.bold, italic) {
        (true, true) => match c {
            'A'..='Z' => off(0x1D468, c, 'A'),
            'a'..='z' => off(0x1D482, c, 'a'),
            _ => None,
        }.or_else(||greek(0x1D71C,0x1D736)),
        (true, false) => match c {
            'A'..='Z' => off(0x1D400, c, 'A'),
            'a'..='z' => off(0x1D41A, c, 'a'),
            '0'..='9' => off(0x1D7CE, c, '0'),
            _ => None,
        }.or_else(||greek(0x1D6A8,0x1D6C2)),
        (false, true) => exceptions(c).or(match c {
            'A'..='Z' => off(0x1D434, c, 'A'),
            'a'..='z' => off(0x1D44E, c, 'a'),
            _ => None,
        }).or_else(||greek(0x1D6E2,0x1D6FC)),
        (false, false) => None,
    }
    .unwrap_or(c)
}

impl<'f> Ctx<'f> {
    fn new(f: &'f Fonts, base_points:f64) -> Ctx<'f> {
        let faces = f.fonts.iter().filter_map(|m| m.face()).collect();
        Ctx { f, base_points, available_width:None, local_width:None, measuring:false,
            equation_diagnostics:Vec::new(), faces, cache: HashMap::new(), missing: false, spacing_error: None }
    }

    fn c(&self) -> &Constants {
        &self.f.c
    }

    /// Actual painted bounds, including positive combining-mark yMin and
    /// horizontal glyph-origin adjustments. Box ascent/depth can include
    /// reserved white space and cannot be substituted for these ink edges.
    fn box_ink_bounds(&self, b: &MBox) -> Option<(f64, f64, f64, f64)> {
        self.box_bounds(b, false)
    }

    /// Phantom visibility and its explicit zero-dimension switches are
    /// independent. Its effective box replaces its children's painted bounds
    /// for surrounding structure placement, including mixed visible rows.
    fn box_content_bounds(&self, b: &MBox) -> Option<(f64, f64, f64, f64)> {
        self.box_bounds(b, true)
    }

    fn box_bounds(&self, b: &MBox, content: bool) -> Option<(f64, f64, f64, f64)> {
        let mut bounds: Option<(f64, f64, f64, f64)> = None;
        let mut masks = Vec::new();
        if content {
            for r in b.regions.iter().filter(|r| r.part && r.kind == "phantom-layout") {
                let rect = (r.x, r.y-r.d, r.x+r.w, r.y+r.h);
                bounds = Some(match bounds {
                    Some((a,b,c,d)) => (a.min(rect.0), b.min(rect.1), c.max(rect.2), d.max(rect.3)), None => rect,
                });
                if r.from < r.to { masks.push((r.from, r.to)); }
            }
            masks.sort_unstable();
        }
        let (mut next, mut skip_to) = (0, 0);
        for (index, item) in b.items.iter().enumerate() {
            while next < masks.len() && masks[next].0 <= index { skip_to = skip_to.max(masks[next].1); next += 1; }
            if index < skip_to { continue; }
            let rect = match *item {
                Item::Glyph { font, gid, x, y, scale, shear, x_scale, .. } => {
                    let Some(face) = self.faces.get(font as usize) else { continue; };
                    let Some(bb) = face.glyph_bounding_box(GlyphId(gid)) else { continue; };
                    let k = scale / self.f.fonts[font as usize].upem;
                    let (a, z) = (shear * f64::from(bb.y_min), shear * f64::from(bb.y_max));
                    (x + (f64::from(bb.x_min) + a.min(z)) * k * x_scale, y + f64::from(bb.y_min) * k,
                     x + (f64::from(bb.x_max) + a.max(z)) * k * x_scale, y + f64::from(bb.y_max) * k)
                },
                Item::Rule { x, y, w, h, .. } => (x, y, x + w, y + h),
                Item::Line { x1, y1, x2, y2, thickness, .. } => {
                    let pad = thickness / 2.0;
                    (x1.min(x2) - pad, y1.min(y2) - pad, x1.max(x2) + pad, y1.max(y2) + pad)
                },
            };
            bounds = Some(match bounds {
                Some((a,b,c,d)) => (a.min(rect.0), b.min(rect.1), c.max(rect.2), d.max(rect.3)), None => rect,
            });
        }
        bounds
    }

    /// Attach the actual selected variant, not the original Unicode glyph.
    /// Assemblies and compound bases use a geometric-centre fallback. A
    /// synthetic slant has no font-designed attachment point of its own.
    fn top_accent_attachment(&self, b: &MBox) -> f64 {
        // Hidden content still makes this a compound base. Counting only
        // painted glyphs would move an accent when phantom.show changes.
        if b.regions.iter().any(|r| r.part && r.kind == "phantom-layout") { return b.w / 2.0; }
        if let [Item::Glyph { font, gid, x, scale, shear, x_scale, .. }] = b.items.as_slice() {
            if *shear == 0.0 {
                if let Some(face) = self.faces.get(*font as usize) {
                    let glyph = GlyphId(*gid);
                    let attachment = face.tables().math.and_then(|m|m.glyph_info)
                        .and_then(|g|g.top_accent_attachments).and_then(|a|a.get(glyph))
                        .map(|a|f64::from(a.value))
                        .unwrap_or_else(||f64::from(face.glyph_hor_advance(glyph).unwrap_or(0)) / 2.0);
                    return x + attachment * scale * x_scale / self.f.fonts[*font as usize].upem;
                }
            }
        }
        b.w / 2.0
    }

    /// OpenType MATH corner values are relative to the individual glyph's
    /// baseline and size. Boxes and synthetic slants use the permitted zero
    /// corner fallback rather than applying an unrelated glyph's contour.
    fn corner_kern(&self, b: &MBox, corner: KernCorner, height: f64) -> f64 {
        let [Item::Glyph { font, gid, y, scale, shear, x_scale, .. }] = b.items.as_slice() else { return 0.0; };
        if *shear != 0.0 || *x_scale != 1.0 || !scale.is_finite() || *scale <= 0.0 || !height.is_finite() { return 0.0; }
        let Some(face) = self.faces.get(*font as usize) else { return 0.0; };
        let Some(info) = face.tables().math.and_then(|m|m.glyph_info).and_then(|g|g.kern_infos).and_then(|k|k.get(GlyphId(*gid))) else { return 0.0; };
        let table = match corner { KernCorner::TopRight=>info.top_right, KernCorner::TopLeft=>info.top_left, KernCorner::BottomRight=>info.bottom_right, KernCorner::BottomLeft=>info.bottom_left };
        let Some(table) = table else { return 0.0; };
        let upem = face.units_per_em() as f64;
        let height = (height - y) * upem / scale;
        // Equality selects the following interval. The tiny design-unit
        // tolerance compensates only for baseline/scale roundoff at an edge.
        let (mut lo, mut hi) = (0, table.count());
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let Some(edge) = table.height(mid) else { return 0.0; };
            if edge.value as f64 <= height + 1e-9 { lo = mid + 1; } else { hi = mid; }
        }
        table.kern(lo).map(|k|k.value as f64 * scale / upem).unwrap_or(0.0)
    }

    fn metrics(&mut self, font: u8, c: char) -> Option<(u16, f64, f64, f64, f64)> {
        if let Some(hit) = self.cache.get(&(font, c)) {
            return *hit;
        }
        let face = self.faces.get(font as usize)?;
        let upem = self.f.fonts[font as usize].upem;
        let got = face.glyph_index(c).filter(|g| g.0 != 0).map(|g| {
            let adv = face.glyph_hor_advance(g).unwrap_or(0) as f64 / upem;
            let (h, d) = face
                .glyph_bounding_box(g)
                .map(|b| (b.y_max as f64 / upem, -(b.y_min as f64) / upem))
                .unwrap_or((0.0, 0.0));
            let italic = if font == 0 {
                face.tables()
                    .math
                    .and_then(|m| m.glyph_info)
                    .and_then(|gi| gi.italic_corrections)
                    .and_then(|ic| ic.get(g))
                    .map(|v| v.value as f64 / upem)
                    .unwrap_or(0.0)
            } else {
                0.0
            };
            (g.0, adv, h, d, italic)
        });
        self.cache.insert((font, c), got);
        got
    }

    fn glyph_box(&mut self, c: char, scale: f64) -> MBox {
        self.glyph_box_preferred(c,scale,None)
    }
    fn glyph_box_preferred(&mut self,c:char,scale:f64,preferred:Option<u8>)->MBox{
        for font in preferred.into_iter().chain((0..self.faces.len() as u8).filter(|f|Some(*f)!=preferred)) {
            if let Some((gid, adv, h, d, it)) = self.metrics(font, c) {
                return MBox {
                    w: adv * scale,
                    h: h.max(0.0) * scale,
                    d: d.max(0.0) * scale,
                    items: vec![Item::Glyph { font, gid, x: 0.0, y: 0.0, scale, shear:0.0, x_scale:1.0, color: Paint::Unresolved }],
                    regions: vec![],
                    italic: it * scale,
                    class: Some(omml::classify(c)),
                    large_op: false,
                    em: Some(scale),
                    spacing: None,
                };
            }
        }
        self.missing = true;
        // Unknown glyph: an em-quad placeholder keeps spacing sane.
        MBox { w: 0.5 * scale, h: 0.7 * scale, d: 0.0, ..MBox::default() }
    }

    /// Glyph id variant that reaches `target` (em, at scale 1) vertically.
    fn vertical_variant(&mut self, c: char, target: f64) -> Option<(u16, f64, f64, f64)> {
        let face = self.faces.first()?;
        let upem = self.f.fonts[0].upem;
        let g = face.glyph_index(c)?;
        let vars = face.tables().math?.variants?;
        let cons = vars.vertical_constructions.get(g)?;
        let mut best = None;
        for v in cons.variants {
            let gid = v.variant_glyph;
            let bb = face.glyph_bounding_box(gid)?;
            let total = (bb.y_max - bb.y_min) as f64 / upem;
            let adv = face.glyph_hor_advance(gid).unwrap_or(0) as f64 / upem;
            best = Some((gid.0, adv, bb.y_max as f64 / upem, -(bb.y_min as f64) / upem));
            if total >= target {
                break;
            }
        }
        best
    }

    /// Build a vertical glyph assembly (top/extender/middle/bottom parts) at
    /// least `target` em tall, when even the largest size variant is short.
    fn vertical_assembly(&mut self, c: char, target: f64, scale: f64) -> Option<MBox> {
        let face = self.faces.first()?;
        let upem = self.f.fonts[0].upem;
        let g = face.glyph_index(c)?;
        let vars = face.tables().math?.variants?;
        let asm = vars.vertical_constructions.get(g)?.assembly?;
        let parts: Vec<_> = asm.parts.into_iter().collect();
        if parts.is_empty() || !parts.iter().any(|p| p.part_flags.extender()) {
            return None;
        }
        let overlap = vars.min_connector_overlap as f64 / upem;
        let adv = |p: &ttf_parser::math::GlyphPart| p.full_advance as f64 / upem;
        let fixed: f64 = parts.iter().filter(|p| !p.part_flags.extender()).map(|p| adv(p) - overlap).sum();
        let ext: f64 = parts.iter().filter(|p| p.part_flags.extender()).map(|p| adv(p) - overlap).sum();
        let reps = if ext <= 0.0 { 0 } else { (((target / scale) - fixed - overlap) / ext).ceil().clamp(0.0, 200.0) as usize };
        let mut out = MBox::empty();
        let mut pos = 0.0f64;
        let mut width = 0.0f64;
        for p in &parts {
            let n = if p.part_flags.extender() { reps } else { 1 };
            for _ in 0..n {
                let bb_min = face.glyph_bounding_box(p.glyph_id).map(|b| b.y_min as f64 / upem).unwrap_or(0.0);
                let w = face.glyph_hor_advance(p.glyph_id).unwrap_or(0) as f64 / upem;
                width = width.max(w);
                out.items.push(Item::Glyph { font: 0, gid: p.glyph_id.0, x: 0.0, y: (pos - bb_min) * scale, scale, shear:0.0, x_scale:1.0, color: Paint::Unresolved });
                pos += adv(p) - overlap;
            }
        }
        let total = (pos + overlap) * scale;
        out.w = width * scale;
        out.h = total;
        out.d = 0.0;
        out.class = Some(omml::Class::Ord);
        Some(out)
    }

    fn sized_glyph(&mut self, c: char, target: f64, scale: f64) -> MBox {
        if let Some((_, _, h, d)) = self.vertical_variant(c, target / scale) {
            if (h + d) * scale < target * 0.98 {
                if let Some(b) = self.vertical_assembly(c, target, scale) {
                    return b;
                }
            }
        }
        match self.vertical_variant(c, target / scale) {
            Some((gid, adv, h, d)) => MBox {
                w: adv * scale,
                h: h * scale,
                d: d * scale,
                items: vec![Item::Glyph { font: 0, gid, x: 0.0, y: 0.0, scale, shear:0.0, x_scale:1.0, color: Paint::Unresolved }],
                regions: vec![],
                italic: 0.0,
                class: Some(omml::classify(c)),
                large_op: false,
                em: Some(scale),
                spacing: None,
            },
            None => self.glyph_box(c, scale),
        }
    }

    /// Use the font's horizontal constructions, rather than enlarging a
    /// bracket uniformly (which also makes its strokes and height enormous).
    fn horizontal_glyph(&mut self, c: char, target: f64, scale: f64) -> MBox {
        let constructed = (|| {
            let face = self.faces.first()?;
            let upem = self.f.fonts[0].upem;
            let vars = face.tables().math?.variants?;
            let cons = vars.horizontal_constructions.get(face.glyph_index(c)?)?;
            let mut best = None;
            for variant in cons.variants {
                let bb = face.glyph_bounding_box(variant.variant_glyph)?;
                let width = (bb.x_max as f64 - bb.x_min as f64) / upem * scale;
                let mut b = MBox::empty(); b.w = width;
                b.h = (bb.y_max as f64 / upem * scale).max(0.0);
                b.d = (-f64::from(bb.y_min) / upem * scale).max(0.0);
                b.items.push(Item::Glyph { font:0, gid:variant.variant_glyph.0,
                    x:-f64::from(bb.x_min)/upem*scale, y:0.0, scale, shear:0.0, x_scale:1.0, color:Paint::Unresolved });
                best = Some(b);
                if width >= target { return best; }
            }
            let Some(asm) = cons.assembly else { return best; };
            let parts: Vec<_> = asm.parts.into_iter().collect();
            if !parts.iter().any(|p|p.part_flags.extender()) { return best; }
            let overlap = vars.min_connector_overlap;
            // Bound malformed fonts and exceptionally wide author content.
            for repeats in 1..=200 {
                let sequence: Vec<_> = parts.iter().flat_map(|p|
                    std::iter::repeat_n(p, if p.part_flags.extender(){repeats}else{1})).collect();
                if sequence.is_empty() || sequence.len()>2048 { return best; }
                if sequence.windows(2).any(|p|p[0].end_connector_length.min(p[1].start_connector_length)<overlap) { return best; }
                let advance = sequence.iter().map(|p|u64::from(p.full_advance)).sum::<u64>()
                    .saturating_sub(u64::from(overlap)*(sequence.len()-1) as u64);
                if (advance as f64 / upem * scale) < target { continue; }
                let mut b = MBox::empty(); let mut x = 0.0;
                for p in sequence {
                    let bb = face.glyph_bounding_box(p.glyph_id)?;
                    b.h = b.h.max(f64::from(bb.y_max)/upem*scale);
                    b.d = b.d.max(-f64::from(bb.y_min)/upem*scale);
                    b.items.push(Item::Glyph { font:0, gid:p.glyph_id.0, x:x*scale,
                        y:0.0, scale, shear:0.0, x_scale:1.0, color:Paint::Unresolved });
                    x += (f64::from(p.full_advance)-f64::from(overlap))/upem;
                }
                b.w=advance as f64/upem*scale;
                return Some(b);
            }
            best
        })();
        constructed.unwrap_or_else(||self.glyph_box(c,scale))
    }

    /// Office delimiters either stay at natural size, grow symmetrically about
    /// the math axis, or match the content's asymmetric height and depth.
    fn delimiter(&mut self, c: char, h: f64, d: f64, s: f64, grow: bool, symmetric: bool) -> MBox {
        if !grow { return self.glyph_box(c, s); }
        let axis = self.c().axis * s;
        let half = (h - axis).max(d + axis);
        let target = if symmetric { 2.0 * half } else { h + d };
        let mut b = if target > 1.05 * s { self.sized_glyph(c, target, s) } else { self.glyph_box(c, s) };
        let mid = (b.h - b.d) / 2.0;
        let dy = if symmetric { axis - mid } else { (h - d) / 2.0 - mid };
        b = b.shifted(0.0, dy);
        let (h2, d2) = (b.h + dy, b.d - dy);
        b.h = h2;
        b.d = d2;
        b
    }

    fn row_parts(&mut self, nodes: &[Node], st: Style, context: omml::SpacingContext) -> Vec<MBox> {
        // Intrinsic pass is suppressed recursively. Only subtrees containing
        // arrays need a second placement, so ordinary formula baselines are
        // byte-for-byte unchanged and deep nesting is not exponential.
        if !self.measuring && self.local_width.is_some() && nodes.iter().any(equation_array::contains_array) {
            let width = self.local_width.unwrap();
            let natural = self.intrinsic(|ctx| ctx.row_parts(nodes, st, context));
            let parts = nodes.iter().filter(|n| !n.is_props()).collect::<Vec<_>>();
            let total = self.hlist(natural.clone(), st).w;
            let count = parts.iter().filter(|n| equation_array::stretches(n)).count();
            let extra = (width - total).max(0.0) / count.max(1) as f64;
            let plans = omml::spacing_plans(nodes, context);
            return nodes.iter().zip(plans).filter(|(n, _)| !n.is_props()).zip(natural).map(|((n, plan), natural)| {
                let slot = if total > width { (width - (total - natural.w)).max(0.0) }
                    else { natural.w + if equation_array::stretches(n) { extra } else { 0.0 } };
                let mut part = self.with_width(Some(slot), |ctx| ctx.node_context(n, st, plan.context));
                part.spacing = Some((plan.first, plan.last)); part
            }).collect();
        }
        let plans = omml::spacing_plans(nodes, context);
        nodes.iter().zip(plans).filter(|(n, _)| !n.is_props()).map(|(n, plan)| {
            let mut part = self.node_context(n, st, plan.context);
            part.spacing = Some((plan.first, plan.last));
            part
        }).collect()
    }

    fn row_context(&mut self, nodes: &[Node], st: Style, context: omml::SpacingContext) -> MBox {
        let parts = self.row_parts(nodes, st, context);
        self.hlist(parts, st)
    }

    fn row(&mut self, nodes: &[Node], st: Style) -> MBox {
        self.row_context(nodes, st, omml::SpacingContext::default())
    }

    fn with_width<T>(&mut self, width: Option<f64>, f: impl FnOnce(&mut Self) -> T) -> T {
        let previous = std::mem::replace(&mut self.local_width, width);
        let value = f(self); self.local_width = previous; value
    }

    fn intrinsic<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let previous = std::mem::replace(&mut self.measuring, true);
        let diagnostics = self.equation_diagnostics.len();
        let value = self.with_width(None, f);
        self.equation_diagnostics.truncate(diagnostics); self.measuring = previous; value
    }

    fn argument_width(&mut self, n: &Node, st: Style, width: Option<f64>) -> MBox {
        self.with_width(width, |ctx| ctx.argument(n, st))
    }

    fn horizontal_slots(natural: &[f64], stretch: &[bool], content: f64, available: f64) -> Vec<f64> {
        let count=stretch.iter().filter(|v|**v).count().max(1) as f64;
        natural.iter().zip(stretch).map(|(width,stretch)| {
            if content>available { (available-(content-width)).max(0.0) }
            else { width+if *stretch{(available-content)/count}else{0.0} }
        }).collect()
    }

    fn display_line(&mut self, nodes: &[Node], st: Style) -> MBox {
        let parts = self.row_parts(nodes, st, omml::SpacingContext::default());
        let parts = nodes.iter().filter(|n| !n.is_props()).zip(parts).map(|(n, part)| self.flow_part(n, part)).collect();
        self.hlist(parts, st)
    }

    fn flow_part(&self, n: &Node, mut part: MBox) -> MBox {
        if n.is_math() && n.name == "r" {
            let mut characters = false;
            for r in &mut part.regions { if r.part { match r.kind.as_str() {
                "alignment-operator" => { r.kind = "paragraph-operator".into(); characters = true; },
                "flow-character" => { r.kind = "paragraph-character".into(); characters = true; },
                _ => (),
            } } }
            if !characters { part = part.part("paragraph-atom"); }
        } else if n.is_math() && n.name == "box" && !n.enabled("boxPr", "noBreak", false) {
            if n.enabled("boxPr", "opEmu", false) && !part.regions.iter().any(|r| r.part && r.kind == "paragraph-operator") {
                part.regions.retain(|r| !(r.part && matches!(r.kind.as_str(), "paragraph-character" | "paragraph-atom")));
                part = part.part("paragraph-operator");
            } else if !part.regions.iter().any(|r| r.part && matches!(r.kind.as_str(), "paragraph-character" | "paragraph-operator" | "paragraph-atom")) { part = part.part("paragraph-atom"); }
        } else if n.is_math() && n.name == "box" && n.enabled("boxPr", "opEmu", false) {
            part = part.part("paragraph-operator");
        } else { part = part.part("paragraph-atom"); }
        part
    }
    /// Cut only at top-level atom boundaries. Nested structures remain whole;
    /// source-owner rectangles are clipped to each resulting visual fragment.
    fn slice_display(&self,b:&MBox,index:&RegionIndex<'_>,from:usize,to:usize,left:f64,right:f64)->MBox{
        let mut out=MBox{w:(right-left).max(0.0),items:b.items[from..to].to_vec(),..MBox::empty()};
        for item in &out.items{
            let(h,d)=match *item{
                Item::Glyph{font,gid,y,scale,..}=>self.faces[font as usize].glyph_bounding_box(GlyphId(gid)).map(|r|{
                    let k=scale/self.f.fonts[font as usize].upem;(y+f64::from(r.y_max)*k,-y-f64::from(r.y_min)*k)
                }).unwrap_or((0.0,0.0)),
                Item::Rule{y,h,..}=>(y+h,-y),Item::Line{y1,y2,thickness,..}=>(y1.max(y2)+thickness/2.0,-y1.min(y2)+thickness/2.0),
            };out.h=out.h.max(h);out.d=out.d.max(d);
        }
        let selected=index.overlapping(from,to);
        for r in &selected{
            if r.kind=="paragraph-atom"&&r.x>=left&&r.x+r.w<=right{out.h=out.h.max(r.h+r.y);out.d=out.d.max(r.d-r.y);}
        }
        for r in selected{if r.from<to&&r.to>from||r.from==r.to&&r.x>=left&&r.x+r.w<=right{
            let mut r=r.clone();r.from=r.from.clamp(from,to)-from;r.to=r.to.clamp(from,to)-from;
            let end=(r.x+r.w).min(right);r.x=r.x.max(left);r.w=(end-r.x).max(0.0);
            r.h=r.h.min((out.h-r.y).max(0.0));r.d=r.d.min((out.d+r.y).max(0.0));out.regions.push(r);
        }}
        out.shifted(-left,0.0)
    }

    fn repeated_sign(&mut self,mut b:MBox,character:Option<char>,sign:char)->MBox{
        if !matches!(character,Some('-'|'−'))||sign=='-'{return b;}
        let scale=b.items.iter().find_map(|i|if let Item::Glyph{scale,..}=i{Some(*scale)}else{None}).unwrap_or(1.0);
        let mut glyph=self.glyph_box('+',scale);
        for r in &mut b.regions{r.x=0.0;r.w=glyph.w;r.h=glyph.h;r.d=glyph.d;r.from=0;r.to=glyph.items.len();if r.character.is_some(){r.character=Some('+');}}
        glyph.regions=b.regions;glyph
    }

    fn wrap_display_line(&mut self,b:MBox,width:Option<f64>,first_x:f64,wrap:omml::MathWrapStyle,alignment:Option<omml::MathJustification>)->Vec<(MBox,f64)>{
        let Some(width)=width.filter(|width|b.w+first_x>*width+1e-8)else{return vec![(b,first_x)];};
        let atoms=b.regions.iter().filter(|r|r.part&&matches!(r.kind.as_str(),"paragraph-character"|"paragraph-operator"|"paragraph-atom")).collect::<Vec<_>>();
        let mut candidates=Vec::new();let(mut previous,mut depth)=(None,0usize);
        for (index,r) in atoms.iter().enumerate(){
            let class=r.character.map(omml::classify).unwrap_or(omml::Class::Ord);
            let binary=r.kind=="paragraph-operator"&&(class!=omml::Class::Bin||matches!(previous,Some(omml::Class::Ord|omml::Class::Close)));
            if depth==0&&binary&&index>0&&index+1<atoms.len(){candidates.push((*r,atoms[index+1].x));}
            match class{omml::Class::Open=>depth+=1,omml::Class::Close=>depth=depth.saturating_sub(1),_=>()}
            previous=Some(class);
        }
        if candidates.is_empty(){return vec![(b,first_x)];}
        let index=RegionIndex::new(&b.regions);
        let indent=if wrap.right{0.0}else{let n=wrap.indent/self.base_points;if n<width{n}else{0.0}};
        let(mut from,mut left,mut prefix,mut result)=(0usize,0.0,MBox::empty(),Vec::new());
        loop{
            let base=if result.is_empty(){first_x}else{indent};let capacity=(width-base).max(0.0);
            if prefix.w+b.w-left<=capacity+1e-8{
                let tail=self.slice_display(&b,&index,from,b.items.len(),left,b.w);let mut line=prefix;let x=line.w;line.w+=tail.w;line.append(tail,x,0.0);
                let x=if !result.is_empty()&&wrap.right{(width-line.w).max(0.0)}else{base};result.push((line,x));break;
            }
            let mut chosen=None;
            let candidate_start=candidates.partition_point(|(r,_)|r.from<=from);
            for &(r,next) in &candidates[candidate_start..]{
                if r.from<=from||r.to>=b.items.len(){continue;}
                let end=r.x+if wrap.binary==omml::MathBinaryBreak::Before{0.0}else{r.w};
                if prefix.w+end-left<=capacity+1e-8{chosen=Some((r,next));}
                else{if chosen.is_none(){chosen=Some((r,next));}break;}
            }
            let Some((cut,next))=chosen else{
                let tail=self.slice_display(&b,&index,from,b.items.len(),left,b.w);let mut line=prefix;let x=line.w;line.w+=tail.w;line.append(tail,x,0.0);result.push((line,base));break;
            };
            let body=self.slice_display(&b,&index,from,cut.from,left,cut.x);let mut line=prefix;let x=line.w;line.w+=body.w;line.append(body,x,0.0);
            if wrap.binary!=omml::MathBinaryBreak::Before{
                let operator=self.slice_display(&b,&index,cut.from,cut.to,cut.x,cut.x+cut.w);
                let operator=if wrap.binary==omml::MathBinaryBreak::Repeat{self.repeated_sign(operator,cut.character,wrap.subtraction.chars().next().unwrap_or('-'))}else{operator};
                let x=line.w;line.w+=operator.w;line.append(operator,x,0.0);
            }
            let x=if result.is_empty(){alignment.filter(|a|*a!=omml::MathJustification::CenterGroup).map_or(base,|a|a.offset(width,line.w))}else if wrap.right{(width-line.w).max(0.0)}else{base};result.push((line,x));
            prefix=MBox::empty();
            match wrap.binary{
                omml::MathBinaryBreak::Before=>{from=cut.from;left=cut.x;},
                omml::MathBinaryBreak::After=>{from=cut.to;left=next;},
                omml::MathBinaryBreak::Repeat=>{from=cut.to;left=cut.x+cut.w;let operator=self.slice_display(&b,&index,cut.from,cut.to,cut.x,cut.x+cut.w);prefix=self.repeated_sign(operator,cut.character,wrap.subtraction.chars().nth(1).unwrap_or('-'));},
            }
        }
        if result.len()>1{for(line,_)in &mut result{*line=std::mem::take(line).part("auto-wrap-line");}}result
    }

    fn display_equation(&mut self, n: &Node, st: Style, width:Option<f64>, wrap:omml::MathWrapStyle, alignment:omml::MathJustification) -> MBox {
        let lines=omml::manual_math_lines(&n.children);
        let mut out=MBox::empty();let mut previous_operators=Vec::new();
        let (mut previous_start,mut previous_depth,mut y,mut count,mut wrapped)=(0.0,0.0,0.0,0usize,false);
        let face=&self.faces[0];let leading=f64::from(i32::from(face.ascender())-i32::from(face.descender())+i32::from(face.line_gap()))/self.f.fonts[0].upem*st.scale(self.c());
        for (nodes,manual) in lines{
            let natural=self.with_width(width, |ctx| ctx.display_line(nodes,st));
            let start=manual.and_then(|v|v.checked_sub(1)).and_then(|i|previous_operators.get(i)).copied().unwrap_or(previous_start);
            let lines=self.wrap_display_line(natural,width,start,wrap,(count==0).then_some(alignment));wrapped|=lines.len()>1;
            for (line,x) in lines{
                if count>0{y-=(previous_depth+line.h+0.2*st.scale(self.c())).max(leading);}
                previous_operators=line.regions.iter().filter(|r|r.part&&r.kind=="paragraph-operator").map(|r|r.x+x).collect();
                previous_start=x;previous_depth=line.d;out.w=out.w.max(x+line.w);out.append(line,x,y);count+=1;
            }
        }
        if wrapped{out.w=out.w.max(width.unwrap_or(0.0));}
        out.mark(n)
    }

    fn paragraph_alignment(&mut self, equations: &[&Node], st: Style, width: Option<f64>) -> Option<Vec<MBox>> {
        let plans=equations.iter().map(|n|equation_array::plan_row(n,false)).collect::<Vec<_>>();
        if !plans.iter().any(|p|p.has_markers||!p.diagnostics.is_empty()) {return None;}
        if plans.iter().any(|p|!p.diagnostics.is_empty()) {
            if !self.measuring {self.equation_diagnostics.extend(plans.iter().flat_map(|p|p.diagnostics.clone()));}
            return None;
        }
        if equations.iter().any(|n|omml::manual_math_lines(&n.children).len()>1) {
            if !self.measuring {self.equation_diagnostics.push(EquationArrayDiagnostic::at_node("math_equation_array_manual_alignment_scope",equations[0],None));}
            return None;
        }
        let columns=plans.iter().map(equation_array::node_columns).collect::<Vec<_>>();
        let natural=self.intrinsic(|ctx|columns.iter().map(|row|row.iter().map(|c| {
            let left=ctx.equation_atoms(&c.before,st,None);let right=ctx.equation_atoms(&c.after,st,None);
            ColumnMeasure{width:left.w+right.w,anchor:c.aligned.then_some(left.w)}
        }).collect::<Vec<_>>()).collect::<Vec<_>>());
        let solution=equation_array::solve(&natural,equation_array::Flags::default(),width).ok()?;
        if solution.overflow {
            if !self.measuring {
                let mut issue=EquationArrayDiagnostic::at_node("math_equation_array_paragraph_alignment_overflow",equations[0],None);
                issue.required_points=Some(solution.natural_width*self.base_points);issue.available_points=width.map(|w|w*self.base_points);
                self.equation_diagnostics.push(issue);
            }
            return None;
        }
        Some(columns.iter().zip(equations).map(|(row,source)| {
            let c=&row[0];let position=solution.columns[0];
            let left=self.equation_atoms(&c.before,st,Some(position.anchor));
            let right=self.equation_atoms(&c.after,st,Some(position.width-position.anchor));
            let mut out=MBox::empty();out.w=solution.width;
            let x=if c.aligned{position.anchor-left.w}else{0.0};
            out.append(left,x,0.0);out.append(right,position.anchor,0.0);
            if c.aligned {out.regions.push(Region{id:String::new(),kind:"paragraph-explicit-anchor".into(),part:true,x:position.anchor,y:0.0,w:0.0,h:out.h,d:out.d,from:0,to:0,character:None,text_range:None});}
            out.mark(source)
        }).collect())
    }

    fn display_paragraph(&mut self, root: &Node, n: &Node, st: Style) -> MBox {
        let style=omml::math_paragraph_style(root,Some(n));
        let(left,right)=style.margins(self.available_width);let width=self.available_width.map(|v|((v-left-right)/self.base_points).max(0.0));let wrap=omml::math_wrap_style(root);
        let equations:Vec<_>=n.children.iter().filter(|n|n.is_math()&&n.name=="oMath").collect();
        let mut out=if equations.is_empty(){self.display_equation(n,st,width,wrap,style.alignment)}else{
            let boxes=self.paragraph_alignment(&equations,st,width).unwrap_or_else(||equations.into_iter().map(|n|self.display_equation(n,st,width,wrap,style.alignment)).collect());
            stack_math_boxes(boxes,style.alignment,style.inter/self.base_points)
        };
        out.h+=style.before/self.base_points;out.d+=style.after/self.base_points;
        if style.explicit_margins {
            let(left,right)=style.margins(self.available_width);let(left,right)=(left/self.base_points,right/self.base_points);
            let available=self.available_width.map(|v|v/self.base_points);
            let inner=available.map(|v|(v-left-right).max(0.0)).unwrap_or(out.w);
            let x=left+style.alignment.offset(inner,out.w);let width=available.unwrap_or(left+out.w+right).max(left+out.w+right);
            let mut frame=MBox::empty();frame.w=width;frame.append(out,x,0.0);out=frame;
        }
        out.mark(n)
    }

    fn display_root(&mut self, root: &Node, st: Style) -> MBox {
        let style=omml::math_paragraph_style(root,None);
        let paragraphs:Vec<_>=root.children.iter().filter(|n|n.is_math()&&n.name=="oMathPara").collect();
        if paragraphs.is_empty(){return self.display_paragraph(root,root,st);}
        // Standard fragments may include adjacent paragraphs. An inline oMath
        // fragment between them is its own implicit display paragraph.
        let mut boxes=Vec::new();
        for n in root.children.iter().filter(|n|!n.is_props()){
            if n.is_math()&&n.name=="oMathPara"{boxes.push(self.display_paragraph(root,n,st));}
            else{let(left,right)=style.margins(self.available_width);let width=self.available_width.map(|v|((v-left-right)/self.base_points).max(0.0));let mut b=self.display_equation(n,st,width,omml::math_wrap_style(root),style.alignment);b.h+=style.before/self.base_points;b.d+=style.after/self.base_points;boxes.push(b);}
        }
        stack_math_boxes(boxes,style.alignment,style.intra/self.base_points)
    }

    /// Concatenate with shared semantic spacing. Transparent phantom contents
    /// have already been normalized with their surrounding row's context.
    fn hlist(&mut self, parts: Vec<MBox>, st: Style) -> MBox {
        let s = st.scale(self.c());
        let mut out = MBox::empty();
        out.em = parts.iter().find_map(|p| p.em);
        let mut previous_em = None;
        let mut previous = None;
        let count = parts.len();
        for (index, p) in parts.into_iter().enumerate() {
            let (first, last) = p.spacing.unwrap_or_else(|| {
                let mut class = p.class;
                if class == Some(omml::Class::Bin) && (previous.is_none() || matches!(previous, Some(omml::Class::Bin | omml::Class::Rel | omml::Class::Open | omml::Class::Punct)) || index + 1 == count) { class = Some(omml::Class::Ord); }
                (class, class)
            });
            let gap = omml::spacing_gap(previous, first, st.level > 0);
            let x = out.w + gap * p.em.or(previous_em).unwrap_or(s);
            previous_em = p.em.or(previous_em);
            out.w = x + p.w;
            out.italic = p.italic;
            out.append(p, x, 0.0);
            if last.is_some() { previous = last; }
        }
        out.class = None;
        out
    }
    fn child_row(&mut self, n: &Node, name: &str, st: Style) -> MBox {
        match n.child(name) {
            Some(c) => {
                let width = if matches!(name, "sub" | "sup" | "deg" | "lim") { None } else { self.local_width };
                self.argument_width(c, st, width)
            },
            None => MBox::empty(),
        }
    }

    fn argument(&mut self, n: &Node, st: Style) -> MBox {
        self.row(&n.children, st.argument(n)).mark(n)
    }

    fn run(&mut self, r: &Node, st: Style) -> MBox {
        let text = r.run_text();
        let rs = omml::run_style(r);
        let literal = omml::literal_run(r);
        let font_slots=omml::run_fonts(r);
        let s = st.scale(self.c()) * rs.half_points.map(|v|f64::from(v)/2.0/self.base_points).unwrap_or(1.0);
        let word = !literal && r.equation_word.unwrap_or_else(||omml::is_word(&text))&&!rs.explicit_style;
        let mut parts = Vec::new();
        let owner=r.equation_source.as_ref().map(|(id,_)|id.as_str()).or_else(||r.attr("readmdId").filter(|_|text.chars().nth(1).is_some()))
            .filter(|id|id.starts_with("rd-formula-token-omml-")&&id.bytes().all(|c|c.is_ascii_alphanumeric()||c==b'-'));
        let mut offset=r.equation_source.as_ref().map_or(0,|(_,start)|*start);
        let mut markers = r.equation_markers.iter().peekable();
        let mut marker_parts = 0;
        for c in text.chars() {
            let from=offset;offset+=c.len_utf16();
            let mut consumes = false;
            while markers.peek().is_some_and(|m| m.offset == from) {
                let marker = markers.next().unwrap();
                let range = (from, if marker.consumes_character { offset } else { from });
                parts.push(MBox::empty().equation_marker(marker.kind, r.attr("readmdId"), Some(range)));
                marker_parts += 1; consumes |= marker.consumes_character;
            }
            if consumes { continue; }
            if literal && !rs.normal_text {
                if let Some(space) = literal_space_advance(c) {
                    parts.push(MBox { w: space*s, ..MBox::empty() }.text_fragment(owner,from,offset));
                    continue;
                }
            }
            if c == ' ' {
                // Operator names carry a trailing space: thin space.
                let space=if rs.normal_text{font_slots.family(c).map(crate::publication_fonts::family).and_then(|family|self.f.fonts.iter().position(|f|f.family==family&&f.bold==rs.bold)).and_then(|i|self.metrics(i as u8,c)).map(|m|m.1).unwrap_or(0.25)}else{1.0/6.0};
                parts.push(MBox { w: space*s, ..MBox::empty() }.text_fragment(owner,from,offset));
                continue;
            }
            if c == '\u{2009}' || c == '\u{2005}' {
                parts.push(MBox { w: 0.17 * s, ..MBox::empty() }.text_fragment(owner,from,offset));
                continue;
            }
            if c == '\u{2003}' {
                parts.push(MBox { w: 1.0 * s, ..MBox::empty() }.text_fragment(owner,from,offset));
                continue;
            }
            let italic = !rs.normal_text && !rs.upright && !word && (c.is_ascii_alphabetic() || ('α'..='ω').contains(&c) || rs.explicit_style&&c.is_alphabetic());
            let preferred=if rs.normal_text{font_slots.family(c).map(crate::publication_fonts::family).and_then(|family|self.f.fonts.iter().position(|f|f.family==family&&f.bold==rs.bold)).filter(|i|*i!=0||!rs.word_italic.unwrap_or(false)).map(|i|i as u8)}else{None};
            let mapped = if rs.normal_text&&preferred.is_none(){math_alnum(c,&omml::RunStyle{bold:rs.bold,..Default::default()},rs.word_italic.unwrap_or(false))}else if rs.normal_text{c}else{math_alnum(c, &rs, italic)};
            let mut b = self.glyph_box_preferred(mapped, s,preferred);
            if rs.normal_text&&rs.word_italic.unwrap_or(false)&&preferred.is_some(){for item in &mut b.items{if let Item::Glyph{shear,..}=item{*shear=0.2;}}b.italic+=0.2*b.h;}
            if b.items.is_empty() && mapped != c {
                b = self.glyph_box(c, s);
            }
            if c == '-' && !rs.normal_text && !literal {
                b = self.glyph_box('−', s);
            }
            for item in &mut b.items { item.resolve_paint(Paint::from_color(rs.color)); }
            if !rs.normal_text{b=b.character(if !literal && matches!(omml::classify(c),omml::Class::Bin|omml::Class::Rel){"alignment-operator"}else{"flow-character"},c);}
            parts.push(b.text_fragment(owner,from,offset));
        }
        for marker in markers {
            parts.push(MBox::empty().equation_marker(marker.kind, r.attr("readmdId"), Some((offset, offset))));
            marker_parts += 1;
        }
        let mut out = MBox::empty();
        let cls = if parts.len() - marker_parts == 1 { parts.iter().find(|p| !p.items.is_empty() || p.w != 0.0).and_then(|p|p.class) } else { Some(omml::Class::Ord) };
        for p in parts {
            let x = out.w;
            out.w += p.w;
            out.italic = p.italic;
            out.append(p, x, 0.0);
        }
        out.class = if rs.normal_text || literal { Some(omml::Class::Ord) } else { cls };
        out.em = Some(s);
        out
    }

    fn control_scale(&self, n: &Node, st: Style) -> f64 {
        st.scale(self.c()) * omml::control_style(n).half_points.map(|v|f64::from(v)/2.0/self.base_points).unwrap_or(1.0)
    }

    fn scripts(&mut self, n: &Node, base: MBox, sub: Option<MBox>, sup: Option<MBox>, st: Style) -> MBox {
        let c = *self.c();
        let s = self.control_scale(n, st);
        let single = base.items.len() <= 1;
        let mut out = base.clone();
        let italic = base.italic;
        out.class = base.class;
        let (mut u, mut v) = (0.0f64, 0.0f64);
        if let Some(p) = &sup {
            u = if st.cramped { c.sup_up_cramped } else { c.sup_up } * s;
            if !single {
                u = u.max(base.h - c.sup_drop_max * s);
            }
            u = u.max(p.d + c.sup_bottom_min * s);
        }
        if let Some(b) = &sub {
            v = c.sub_down * s;
            if !single {
                v = v.max(base.d + c.sub_drop_min * s);
            }
            v = v.max(b.h - c.sub_top_max * s);
        }
        if let (Some(p), Some(b)) = (&sup, &sub) {
            let gap = (u - p.d) - (b.h - v);
            if gap < c.sub_sup_gap * s {
                let deficit = c.sub_sup_gap * s - gap;
                // Use the font's permitted upward room first; only the
                // remaining separation moves the subscript farther down.
                let raise = (c.sup_bottom_max_with_sub * s - (u - p.d)).max(0.0).min(deficit);
                u += raise;
                v += deficit - raise;
            }
        }
        let x0 = base.w;
        // At each overlap edge, combine the two facing corners. The smaller
        // sum is the OpenType MATH correction (which can be negative).
        let sup_kern = sup.as_ref().map(|p| {
            (self.corner_kern(&base,KernCorner::TopRight,u-p.d) + self.corner_kern(p,KernCorner::BottomLeft,-p.d))
                .min(self.corner_kern(&base,KernCorner::TopRight,base.h) + self.corner_kern(p,KernCorner::BottomLeft,base.h-u))
        }).unwrap_or(0.0);
        let sub_kern = sub.as_ref().map(|b| {
            (self.corner_kern(&base,KernCorner::BottomRight,b.h-v) + self.corner_kern(b,KernCorner::TopLeft,b.h))
                .min(self.corner_kern(&base,KernCorner::BottomRight,-base.d) + self.corner_kern(b,KernCorner::TopLeft,v-base.d))
        }).unwrap_or(0.0);
        let natural_sup_x = x0 + italic + sup_kern;
        let natural_sub_x = x0 + sub_kern;
        // Explicit Office alignment puts both columns at the outermost start,
        // overriding the contour adjustment without pulling one into the base.
        let aligned = n.enabled("sSubSupPr", "alnScr", false);
        let sup_x = if aligned { natural_sup_x.max(natural_sub_x) } else { natural_sup_x };
        let sub_x = if aligned { sup_x } else { natural_sub_x };
        let mut w = x0;
        if let Some(p) = sup {
            let pw = p.w;
            out.append(p, sup_x, u);
            w = w.max(sup_x + pw);
        }
        if let Some(b) = sub {
            let bw = b.w;
            out.append(b, sub_x, -v);
            w = w.max(sub_x + bw);
        }
        out.w = w + c.space_after_script * s;
        out.italic = 0.0;
        out
    }

    fn fraction(&mut self, n: &Node, st: Style) -> MBox {
        let c = *self.c();
        let s = self.control_scale(n, st);
        let kind = n.prop("fPr", "type").unwrap_or("bar");
        let small = n.math_defaults.small_fraction.unwrap_or(false);
        let constrained=!self.measuring&&self.local_width.is_some()&&equation_array::contains_array(n);
        let mut num=if constrained{self.intrinsic(|ctx|ctx.child_row(n,"num",st.frac_part(false,small)))}else{self.child_row(n,"num",st.frac_part(false,small))};
        let mut den=if constrained{self.intrinsic(|ctx|ctx.child_row(n,"den",st.frac_part(true,small)))}else{self.child_row(n,"den",st.frac_part(true,small))};
        if constrained {
            let outer=self.intrinsic(|ctx|ctx.fraction(n,st));
            let stacked=!matches!(kind,"lin"|"skw");
            let content=if stacked{num.w.max(den.w)}else{num.w+den.w};
            let available=(self.local_width.unwrap()-(outer.w-content)).max(0.0);
            let slots=if stacked{vec![available;2]}else{Self::horizontal_slots(&[num.w,den.w],
                &[n.child("num").is_some_and(equation_array::stretches),n.child("den").is_some_and(equation_array::stretches)],content,available)};
            num=self.with_width(Some(slots[0]),|ctx|ctx.child_row(n,"num",st.frac_part(false,small)));
            den=self.with_width(Some(slots[1]),|ctx|ctx.child_row(n,"den",st.frac_part(true,small)));
        }
        if kind == "lin" {
            let slash=self.glyph_box('/',s);
            let (nw,dw,sw)=(num.w,den.w,slash.w);let mut out=MBox::empty();
            out.append(num,0.0,0.0);out.append(slash,nw+0.08*s,0.0);out.append(den,nw+sw+0.16*s,0.0);
            out.w=nw+dw+sw+0.16*s;out.class=Some(omml::Class::Ord);return out;
        }
        if kind == "skw" {
            let axis = c.axis * s;
            let horizontal = c.skew_hgap.max(0.0) * s;
            let vertical = c.skew_vgap.max(0.0) * s;
            // The two font gaps separate the operands, not two extra margins
            // around the slash. Put their facing ink edges about the axis.
            let up = axis + vertical / 2.0 + num.d;
            let down = den.h - axis + vertical / 2.0;
            let target = 2.0 * (num.h + num.d + vertical / 2.0)
                .max(den.h + den.d + vertical / 2.0);
            let mut slash = self.sized_glyph('/', target, s);
            let actual = slash.h + slash.d;
            if actual > 0.0 && actual < target {
                // Some fonts have finitely many slash variants and no
                // assembly. Uniformly enlarge the largest available outline
                // instead of leaving a short slash inside a tall fraction.
                let factor = target / actual;
                for item in &mut slash.items {
                    if let Item::Glyph { x, y, scale, .. } = item { *x *= factor; *y *= factor; *scale *= factor; }
                }
                slash.w *= factor; slash.h *= factor; slash.d *= factor;
            }
            let slash_y = axis - (slash.h - slash.d) / 2.0;
            let slash_x = num.w + horizontal / 2.0 - slash.w / 2.0;
            // A wide slash may extend beyond a very short/empty operand.
            // Reserve its advance while retaining the exact operand gap.
            let left = slash_x.min(0.0);
            let width = (num.w + horizontal + den.w).max(slash_x + slash.w) - left;
            let den_x = num.w + horizontal - left;
            let mut out = MBox::empty();
            out.append(num, -left, up);
            out.append(slash.part("fraction-slash"), slash_x - left, slash_y);
            out.append(den, den_x, -down);
            out.w = width; out.class = Some(omml::Class::Ord); return out;
        }
        let pad = 0.12 * s;
        let w = num.w.max(den.w) + 2.0 * pad;
        if kind == "noBar" {
            let (mut up, mut down, gap) = if st.display {
                (c.stack_up_d * s, c.stack_down_d * s, c.stack_gap_d * s)
            } else { (c.stack_up * s, c.stack_down * s, c.stack_gap * s) };
            let deficit = (gap - (up - num.d + down - den.h)).max(0.0);
            up += deficit / 2.0; down += deficit / 2.0;
            let (nw, dw) = (num.w, den.w);
            let mut out = MBox::empty();
            out.append(num, (w - nw) / 2.0, up);
            out.append(den, (w - dw) / 2.0, -down);
            out.w = w; out.class = Some(omml::Class::Ord); return out;
        }
        let t = c.rule * s;
        let axis = c.axis * s;
        let (mut up, mut down) = if st.display { (c.num_up_d * s, c.den_down_d * s) } else { (c.num_up * s, c.den_down * s) };
        let (ng, dg) = if st.display { (c.num_gap_d * s, c.den_gap_d * s) } else { (c.num_gap * s, c.den_gap * s) };
        up = up.max(axis + t / 2.0 + ng + num.d);
        down = down.max(dg + t / 2.0 - axis + den.h);
        let mut out = MBox::empty();
        let (nw, dw) = (num.w, den.w);
        out.append(num, (w - nw) / 2.0, up);
        out.append(den, (w - dw) / 2.0, -down);
        out.items.push(Item::Rule { x: pad * 0.5, y: axis - t / 2.0, w: w - pad, h: t, color: Paint::Unresolved });
        out.w = w;
        out.h = out.h.max(axis + t);
        out.class = Some(omml::Class::Ord);
        out
    }

    fn radical(&mut self, n: &Node, st: Style) -> MBox {
        let c = *self.c();
        let s = self.control_scale(n, st);
        let constrained=!self.measuring&&self.local_width.is_some()&&n.child("e").is_some_and(equation_array::contains_array);
        let mut body=if constrained{self.intrinsic(|ctx|ctx.child_row(n,"e",Style{cramped:true,..st}))}
            else{self.child_row(n,"e",Style{cramped:true,..st})};
        if constrained {
            let outer=self.intrinsic(|ctx|ctx.radical(n,st));
            let width=(self.local_width.unwrap()-(outer.w-body.w)).max(0.0);
            body=self.with_width(Some(width),|ctx|ctx.child_row(n,"e",Style{cramped:true,..st}));
        }
        let gap = if st.display { c.rad_gap_d } else { c.rad_gap } * s;
        let t = c.rad_rule * s;
        let (_, bottom, _, body_top) = self.box_content_bounds(&body).unwrap_or((0.0, -body.d, body.w, body.h));
        let need = body_top - bottom + gap + t;
        let mut sign = self.sized_glyph('√', need, s);
        // Put the actual top of the sign at the body's ink + gap + rule.
        let top = body_top + gap + t;
        let (_, sign_bottom, _, sign_top) = self.box_ink_bounds(&sign).unwrap_or((0.0, -sign.d, sign.w, sign.h));
        let dy = top - sign_top;
        sign = sign.shifted(0.0, dy);
        let (sh, sd) = (top, -sign_bottom - dy);
        sign.h = sh; sign.d = sd;
        let mut out = MBox::empty();
        let hide = n.enabled("radPr", "degHide",false);
        let deg = n.child("deg").filter(|d| !hide &&
            !d.children.iter().filter(|c| !c.is_props()).all(omml::empty_argument));
        let mut x = 0.0;
        if let Some(dn) = deg {
            let db = self.argument_width(dn, Style { display: false, level: 2, cramped: false },None);
            // Position the degree's bottom, not its baseline. Negative
            // after-kern must not move a short degree left of the radical.
            let degree_bottom = self.box_content_bounds(&db).map(|b| b.1).unwrap_or(-db.d);
            let raise = -sd + (sh + sd) * c.rad_degree_raise - degree_bottom;
            let dw = db.w;
            let before = (c.rad_before * s).max(0.0);
            let after = (c.rad_after * s).max(-dw);
            out.append(db.part("radical-degree"), before, raise);
            x = before + dw + after;
        }
        let sw = sign.w;
        out.append(sign.part("radical-sign"), x, 0.0);
        out.h = out.h.max(sh + c.rad_extra * s);
        out.d = out.d.max(sd);
        let bx = x + sw;
        let bw = body.w;
        out.append(body, bx, 0.0);
        out.items.push(Item::Rule { x: bx, y: top - t, w: bw + 0.05 * s, h: t, color: Paint::Unresolved });
        out.w = bx + bw + 0.1 * s;
        out.class = Some(omml::Class::Ord);
        out
    }

    fn nary(&mut self, n: &Node, st: Style) -> MBox {
        let c = *self.c();
        let s = self.control_scale(n, st);
        let chr = n.character("naryPr", "chr", "∫").chars().next();
        let constrained=!self.measuring&&self.local_width.is_some()&&n.child("e").is_some_and(equation_array::contains_array);
        let mut body=if constrained{self.intrinsic(|ctx|ctx.child_row(n,"e",st))}else{self.child_row(n,"e",st)};
        if constrained {
            let outer=self.intrinsic(|ctx|ctx.nary(n,st));
            let width=(self.local_width.unwrap()-(outer.w-body.w)).max(0.0);
            body=self.with_width(Some(width),|ctx|ctx.child_row(n,"e",st));
        }
        let mut op = chr.map(|chr| {
            let minimum = if st.display { c.display_op_min * s } else { 0.0 };
            let target = if n.enabled("naryPr", "grow", false) { minimum.max(body.h + body.d) } else { minimum };
            if target > 0.0 { self.sized_glyph(chr, target, s) } else { self.glyph_box(chr, s) }
        }).unwrap_or_else(MBox::empty);
        // Centre on the math axis.
        let mid = (op.h - op.d) / 2.0;
        let dy = if chr.is_some() { c.axis * s - mid } else { 0.0 };
        op = op.shifted(0.0, dy);
        op.h += dy;
        op.d -= dy;
        op.large_op = true;
        op=op.part("operator");
        let sub = n.child("sub").filter(|x| !x.children.is_empty()&&!n.enabled("naryPr","subHide",false)).map(|x| self.argument_width(x, st.sub(),None));
        let sup = n.child("sup").filter(|x| !x.children.is_empty()&&!n.enabled("naryPr","supHide",false)).map(|x| self.argument_width(x, st.sup(),None));
        let limits = omml::nary_limits(n, st.display);
        let mut head = if sub.is_none() && sup.is_none() {
            // With no bounds there is no script spacing. In particular, an
            // explicitly empty operator must not leave an invisible gap.
            op
        } else if limits {
            let w = op.w.max(sub.as_ref().map(|b| b.w).unwrap_or(0.0)).max(sup.as_ref().map(|b| b.w).unwrap_or(0.0));
            let mut out = MBox::empty();
            let (oh, od, ow) = (op.h, op.d, op.w);
            out.append(op, (w - ow) / 2.0, 0.0);
            if let Some(p) = sup {
                let y = (oh + c.upper_gap * s + p.d).max(oh + c.upper_rise * s);
                let pw = p.w;
                out.append(p, (w - pw) / 2.0, y);
            }
            if let Some(b) = sub {
                let y = (od + c.lower_gap * s + b.h).max(od + c.lower_drop * s * 0.5);
                let bw = b.w;
                out.append(b, (w - bw) / 2.0, -y);
            }
            out.w = w;
            out
        } else {
            let italic = op.italic;
            let mut b = self.scripts(n, op, sub, sup, st);
            b.w += italic * 0.5;
            b
        };
        if head.w > 0.0 { head.w += 0.1 * s; }
        let hw = head.w;
        head.w = hw + body.w;
        head.append(body, hw, 0.0);
        head.class = Some(omml::Class::Ord);
        head
    }

    fn delimited(&mut self, n: &Node, st: Style) -> MBox {
        let s = self.control_scale(n, st);
        let beg = omml::delimiter_char(n.character("dPr", "begChr", "("));
        let end = omml::delimiter_char(n.character("dPr", "endChr", ")"));
        let sep = omml::delimiter_char(n.character("dPr", "sepChr", "│"));
        let grow = n.enabled("dPr", "grow", true);
        let symmetric = n.prop("dPr", "shp") != Some("match");
        let sources=n.children.iter().filter(|c|c.name=="e"&&c.is_math()).collect::<Vec<_>>();
        let constrained=!self.measuring&&self.local_width.is_some()&&sources.iter().any(|e|equation_array::contains_array(e));
        let mut arguments:Vec<MBox>=if constrained {self.intrinsic(|ctx|sources.iter().map(|e|ctx.argument(e,st)).collect())}
            else{sources.iter().map(|e|self.argument(e,st)).collect()};
        if constrained {
            let outer=self.intrinsic(|ctx|ctx.delimited(n,st));
            let widths=arguments.iter().map(|b|b.w).collect::<Vec<_>>();let content=widths.iter().sum::<f64>();
            let available=(self.local_width.unwrap()-(outer.w-content)).max(0.0);
            let slots=Self::horizontal_slots(&widths,&sources.iter().map(|n|equation_array::stretches(n)).collect::<Vec<_>>(),content,available);
            arguments=sources.iter().zip(slots).map(|(e,w)|self.argument_width(e,st,Some(w))).collect();
        }
        let h = arguments.iter().map(|b| b.h).fold(0.7*s, f64::max);
        let d = arguments.iter().map(|b| b.d).fold(0.2*s, f64::max);
        let mut inner_parts = Vec::new();
        for (i, argument) in arguments.into_iter().enumerate() {
            if i > 0 {
                if let Some(sep) = sep.chars().next() { inner_parts.push(self.delimiter(sep, h, d, s, grow, symmetric)); }
            }
            inner_parts.push(argument);
        }
        let inner = self.hlist(inner_parts, st);
        let mut out = MBox::empty();
        let mut x = 0.0;
        if let Some(ch) = beg.chars().next() {
            let b = self.delimiter(ch, h, d, s, grow, symmetric).part("begin-fence");
            let bw = b.w;
            out.append(b, 0.0, 0.0);
            x = bw;
        }
        let iw = inner.w;
        out.append(inner, x, 0.0);
        x += iw;
        if let Some(ch) = end.chars().next() {
            let b = self.delimiter(ch, h, d, s, grow, symmetric).part("end-fence");
            let bw = b.w;
            out.append(b, x, 0.0);
            x += bw;
        }
        out.w = x;
        out.class = Some(omml::Class::Ord);
        out
    }

    /// Render semantic fragments with the original row's boundary glue and
    /// lexical style. A split operator/function does not acquire a new class.
    fn equation_atoms(&mut self, nodes: &[Node], st: Style, width: Option<f64>) -> MBox {
        let natural = if width.is_some() && !self.measuring && nodes.iter().any(equation_array::contains_array) {
            Some(self.intrinsic(|ctx| nodes.iter().map(|n| {
                let part = ctx.node_context(n, st, n.equation_context.unwrap_or_default());
                let gap = n.equation_glue.map_or(0.0, |(a,b)|omml::spacing_gap(a,b,st.level>0)) * part.em.unwrap_or_else(||st.scale(ctx.c()));
                (part.w, gap)
            }).collect::<Vec<_>>()))
        } else { None };
        let total = natural.as_ref().map_or(0.0,|v|v.iter().map(|(w,g)|w+g).sum::<f64>());
        let count = nodes.iter().filter(|n|equation_array::stretches(n)).count().max(1);
        let mut out = MBox::empty();
        for (i,n) in nodes.iter().enumerate() {
            let slot = width.map(|width| if let Some(natural)=&natural {
                let (w,_) = natural[i];
                if total>width { (width-(total-w)).max(0.0) }
                else { w + if equation_array::stretches(n) {(width-total)/count as f64} else {0.0} }
            } else { width });
            let part = self.with_width(slot, |ctx|ctx.node_context(n,st,n.equation_context.unwrap_or_default()));
            let gap = n.equation_glue.map_or(0.0, |(a,b)|omml::spacing_gap(a,b,st.level>0)) * part.em.unwrap_or_else(||st.scale(self.c()));
            let x=out.w+gap; out.w=x+part.w; out.italic=part.italic; out.append(part,x,0.0);
        }
        out
    }

    fn equation_array(&mut self, n: &Node, st: Style) -> MBox {
        let rows = n.children.iter().filter(|c|c.is_math()&&c.name=="e").collect::<Vec<_>>();
        let plans = rows.iter().map(|r|equation_array::plan_row(r,true)).collect::<Vec<_>>();
        if plans.iter().map(|p|p.marker_count).sum::<usize>()>equation_array::MAX_MARKERS {
            if !self.measuring {self.equation_diagnostics.push(EquationArrayDiagnostic::at_node("math_equation_array_marker_budget",n,None));}
            return self.with_width(None,|ctx|ctx.table(rows.iter().map(|r|vec![*r]).collect(),st,true,Some(n)));
        }
        if plans.iter().any(|p|!p.diagnostics.is_empty()) {
            if !self.measuring { self.equation_diagnostics.extend(plans.iter().flat_map(|p|p.diagnostics.clone())); }
            return self.with_width(None, |ctx|ctx.table(rows.iter().map(|r|vec![*r]).collect(),st,true,Some(n)));
        }
        if !plans.iter().any(|p|p.has_markers) {
            return self.with_width(None, |ctx|ctx.table(rows.iter().map(|r|vec![*r]).collect(),st,true,Some(n)));
        }
        let columns = plans.iter().map(equation_array::node_columns).collect::<Vec<_>>();
        let row_styles = rows.iter().map(|r|Style{display:false,..st}.argument(r)).collect::<Vec<_>>();
        let natural_boxes = self.intrinsic(|ctx| columns.iter().zip(&row_styles).map(|(row,style)|row.iter().map(|c| {
            let left=ctx.equation_atoms(&c.before,*style,None);
            let right=ctx.equation_atoms(&c.after,*style,None);
            (left,right)
        }).collect::<Vec<_>>()).collect::<Vec<_>>());
        let natural=natural_boxes.iter().zip(&columns).map(|(row,columns)|row.iter().zip(columns).map(|((left,right),column)|
            ColumnMeasure{width:left.w+right.w,anchor:column.aligned.then_some(left.w)}).collect::<Vec<_>>()).collect::<Vec<_>>();
        let width=self.local_width;
        let solution=match equation_array::solve(&natural,equation_array::Flags::read(n),width) {
            Ok(solution)=>solution,
            Err(code)=>{
                if !self.measuring {self.equation_diagnostics.push(EquationArrayDiagnostic::at_node(code,n,None));}
                return self.with_width(None,|ctx|ctx.table(rows.iter().map(|r|vec![*r]).collect(),st,true,Some(n)));
            }
        };
        if !self.measuring {
            if solution.overflow {
                let mut issue=EquationArrayDiagnostic::at_node("math_equation_array_overflow",n,None);
                issue.required_points=Some(solution.natural_width*self.base_points);
                issue.available_points=width.map(|v|v*self.base_points); self.equation_diagnostics.push(issue);
            }
            if equation_array::Flags::read(n).maximum && width.is_none() {
                self.equation_diagnostics.push(EquationArrayDiagnostic::at_node("math_equation_array_width_unavailable",n,None));
            }
            if solution.single_column_distribution {
                self.equation_diagnostics.push(EquationArrayDiagnostic::at_node("math_equation_array_single_column_distribution",n,None));
            }
        }
        let rendered = columns.iter().zip(&row_styles).enumerate().map(|(i,(row,style))| {
            let mut out=MBox::empty();out.w=solution.width;
            for (j,c) in row.iter().enumerate() {
                let position=solution.columns[j];
                let left_width=position.anchor-position.origin;
                // An intrinsic ancestor must not re-enter a second placement
                // for every nested array (which would double work per level).
                let (left,right)=if self.measuring {natural_boxes[i][j].clone()}else{
                    (self.equation_atoms(&c.before,*style,Some(if c.aligned{left_width}else{position.width})),
                     self.equation_atoms(&c.after,*style,Some((position.width-left_width).max(0.0))))
                };
                let x=if c.aligned{position.anchor-left.w}else{position.origin};
                out.append(left,x,0.0);out.append(right,position.anchor,0.0);
                out.regions.push(Region{id:String::new(),kind:"equation-array-column".into(),part:true,x:position.origin,y:0.0,w:position.width,h:out.h,d:out.d,from:0,to:out.items.len(),character:None,text_range:None});
                if c.aligned {out.regions.push(Region{id:String::new(),kind:"equation-array-anchor".into(),part:true,x:position.anchor,y:0.0,w:0.0,h:out.h,d:out.d,from:0,to:0,character:None,text_range:None});}
            }
            out
        }).collect::<Vec<_>>();
        let s=self.control_scale(n,st);let gap=omml::table_row_gap(n).in_em(self.base_points,s);
        let heights=rendered.iter().map(|r|(r.h.max(0.7*s),r.d.max(0.25*s))).collect::<Vec<_>>();
        let total=heights.iter().map(|(h,d)|h+d).sum::<f64>()+gap*heights.len().saturating_sub(1) as f64;
        let mut y=match omml::table_base_justification(n){Some("top")=>heights.first().map_or(0.0,|r|r.0),Some("bot")=>total-heights.last().map_or(0.0,|r|r.1),_=>total/2.0+self.c().axis*s};
        let mut out=MBox::empty();out.w=solution.width;out.class=Some(omml::Class::Ord);
        for ((row,(h,d)),source) in rendered.into_iter().zip(heights).zip(rows) {
            y-=h;out.append(row.mark(source),0.0,y);y-=d+gap;
        }
        out
    }

    fn table_cell(&mut self, c: &Node, st: Style, s: f64, left: bool, placeholders: bool) -> MBox {
            if !left && omml::empty_argument(c) {
                let scale = s * st.argument(c).scale(self.c()) / st.scale(self.c());
                let (w, h, d, stroke) = omml::MATRIX_PLACEHOLDER;
                let mut b = MBox { w:w*scale, h:h*scale, d:d*scale, class:Some(omml::Class::Ord), ..MBox::empty() };
                if placeholders {
                    let inset = stroke * scale / 2.0;
                    for (x1,y1,x2,y2) in [(inset,-b.d+inset,b.w-inset,-b.d+inset),(inset,b.h-inset,b.w-inset,b.h-inset),(inset,-b.d+inset,inset,b.h-inset),(b.w-inset,-b.d+inset,b.w-inset,b.h-inset)] {
                        b.items.push(Item::Line { x1,y1,x2,y2,thickness:stroke*scale,color:Paint::Unresolved });
                    }
                }
                b.mark(c)
            } else { self.argument(c, Style { display:false, ..st }) }
    }

    fn table(&mut self, rows: Vec<Vec<&Node>>, st: Style, left: bool, properties:Option<&Node>) -> MBox {
        let s = properties.map(|n|self.control_scale(n,st)).unwrap_or_else(||st.scale(self.c()));
        let placeholders = !left && properties.is_some_and(omml::matrix_placeholders_visible);
        let constrained=!self.measuring&&self.local_width.is_some()&&rows.iter().flatten().any(|c|equation_array::contains_array(c));
        let mut cells:Vec<Vec<MBox>>=if constrained {
            self.intrinsic(|ctx|rows.iter().map(|r|r.iter().map(|c|ctx.table_cell(c,st,s,left,placeholders)).collect()).collect())
        } else {rows.iter().map(|r|r.iter().map(|c|self.table_cell(c,st,s,left,placeholders)).collect()).collect()};
        let ncol = cells.iter().map(|r| r.len()).max().unwrap_or(0);
        let alignments=if left {vec!["left";ncol]} else {properties.map(|n|omml::matrix_column_alignments(n,ncol)).unwrap_or_else(||vec!["center";ncol])};
        let minimum = if left { 0.0 } else { properties.map(|n|omml::matrix_column_minimum(n).in_em(self.base_points,s)).unwrap_or(0.0) };
        let mut colw = vec![minimum; ncol];
        for r in &cells {
            for (i, c) in r.iter().enumerate() {
                colw[i] = colw[i].max(c.w);
            }
        }
        let col_gap = if left { 0.0 } else { properties.map(|n|omml::matrix_column_gap(n).in_em(self.base_points,s)).unwrap_or(0.8*s) };
        if constrained {
            let stretch=(0..ncol).map(|j|rows.iter().any(|r|r.get(j).is_some_and(|n|equation_array::stretches(n)))).collect::<Vec<_>>();
            let content=colw.iter().sum::<f64>();
            let available=(self.local_width.unwrap()-col_gap*ncol.saturating_sub(1) as f64).max(0.0);
            let slots=Self::horizontal_slots(&colw,&stretch,content,available);
            cells=rows.iter().map(|r|r.iter().enumerate().map(|(j,c)|self.with_width(Some(slots[j]),|ctx|ctx.table_cell(c,st,s,left,placeholders))).collect()).collect();
            colw.fill(minimum);
            for row in &cells {for (j,cell) in row.iter().enumerate(){colw[j]=colw[j].max(cell.w);}}
        }
        let row_gap = properties.map(|n|omml::table_row_gap(n).in_em(self.base_points,s)).unwrap_or(s);
        let heights: Vec<(f64, f64)> = cells
            .iter()
            .map(|r| {
                (
                    r.iter().map(|c| c.h).fold(0.7 * s, f64::max),
                    r.iter().map(|c| c.d).fold(0.25 * s, f64::max),
                )
            })
            .collect();
        let total: f64 = heights.iter().map(|(h, d)| h + d).sum::<f64>() + row_gap * (heights.len().saturating_sub(1)) as f64;
        // Centre the whole table on the axis.
        let mut y = total / 2.0 + self.c().axis * s;
        match properties.and_then(omml::table_base_justification){
            Some("top")=>y=heights.first().map(|(h,_)|*h).unwrap_or(0.0),
            Some("bot")=>y=total-heights.last().map(|(_,d)|*d).unwrap_or(0.0),
            _=>{},
        }
        let mut out = MBox::empty();
        for (ri, r) in cells.into_iter().enumerate() {
            let (h, d) = heights[ri];
            y -= h;
            let mut x = 0.0;
            for (ci, c) in r.into_iter().enumerate() {
                let cw = c.w;
                let dx = match alignments[ci]{"left"=>0.0,"right"=>colw[ci]-cw,_=>(colw[ci]-cw)/2.0};
                out.append(c, x + dx, y);
                x += colw[ci] + col_gap;
            }
            y -= d + row_gap;
        }
        out.w = colw.iter().sum::<f64>() + col_gap * (ncol.saturating_sub(1)) as f64;
        out.class = Some(omml::Class::Ord);
        out
    }

    fn accent(&mut self, n: &Node, st: Style) -> MBox {
        let s = self.control_scale(n, st);
        let chr = n.character("accPr", "chr", "^");
        let base = self.child_row(n, "e", Style { cramped: true, ..st });
        if chr.is_empty() { return base; }
        let ch = match chr {
            "^" | "\u{302}" => '\u{302}',
            "~" | "\u{303}" => '\u{303}',
            "\u{2192}" | "\u{20D7}" => '\u{20D7}',
            "¯" | "\u{305}" | "\u{304}" => '\u{304}',
            "\u{307}" | "˙" => '\u{307}',
            "\u{308}" | "¨" => '\u{308}',
            other => other.chars().next().unwrap_or('^'),
        };
        let mut acc = self.horizontal_glyph(ch, base.w, s);
        if acc.items.is_empty() {
            acc = self.glyph_box(if ch == '\u{20D7}' { '→' } else { '^' }, s * 0.8);
        }
        // Some fonts have a finite accent variant list but no assembly.
        // Stretch its largest designed outline only horizontally, preserving
        // the designed height. Non-stretchy accents (dots) keep natural size.
        let stretchable = self.faces.first().and_then(|f| f.tables().math.and_then(|m|m.variants)
            .and_then(|v|f.glyph_index(ch).and_then(|g|v.horizontal_constructions.get(g)))).is_some();
        if stretchable {
            if let Some((left, _, right, _)) = self.box_ink_bounds(&acc) {
                let width = right - left;
                if width > 0.0 && width < base.w {
                    let factor = base.w / width;
                    for item in &mut acc.items {
                        if let Item::Glyph { x, x_scale, .. } = item { *x *= factor; *x_scale *= factor; }
                    }
                    acc.w *= factor; acc.italic *= factor;
                }
            }
        }
        let mut out = MBox::empty();
        let bw = base.w;
        let (base_left, _, base_right, base_top) = self.box_content_bounds(&base).unwrap_or((0.0, -base.d, bw, base.h));
        let (left, acc_bottom, right, _) = self.box_ink_bounds(&acc).unwrap_or((0.0, -acc.d, acc.w, acc.h));
        let dx = self.top_accent_attachment(&base) - self.top_accent_attachment(&acc);
        // Font-designed combining accents share the base baseline until the
        // base exceeds AccentBaseHeight. Other glyphs still cannot collide.
        let y = (base_top - self.c().accent_base * s).max(0.0).max(base_top - acc_bottom);
        let start = (dx + left).min(base_left).min(0.0);
        let end = bw.max(base_right).max(dx + right);
        out.append(base, -start, 0.0);
        out.append(acc.part("accent-glyph"), dx - start, y);
        out.w = end - start;
        out.class = Some(omml::Class::Ord);
        out
    }

    fn bar(&mut self, n: &Node, st: Style) -> MBox {
        let c = *self.c();
        let s = self.control_scale(n, st);
        let bottom = n.prop("barPr", "pos") == Some("bot");
        let base = self.child_row(n, "e", st);
        let t = if bottom { c.underbar_rule } else { c.overbar_rule } * s;
        let mut out = MBox::empty();
        let bw = base.w;
        let (_, low, _, high) = self.box_content_bounds(&base).unwrap_or((0.0, -base.d, bw, base.h));
        out.append(base, 0.0, 0.0);
        if bottom {
            let y = low - c.underbar_gap * s - t;
            out.items.push(Item::Rule { x: 0.0, y, w: bw, h: t, color: Paint::Unresolved });
            out.d = out.d.max(-y + c.underbar_extra * s);
        } else {
            let y = high + c.overbar_gap * s;
            out.items.push(Item::Rule { x: 0.0, y, w: bw, h: t, color: Paint::Unresolved });
            out.h = out.h.max(y + t + c.overbar_extra * s);
        }
        out.w = bw;
        out.class = Some(omml::Class::Ord);
        out
    }

    fn limit(&mut self, n: &Node, st: Style, lower: bool) -> MBox {
        let c = *self.c();
        let s = self.control_scale(n, st);
        let base = self.child_row(n, "e", st);
        let lim = self.child_row(n, "lim", st.sub());
        // limLow/limUpp are explicit below/above structures, including inline
        // math. Only n-ary limit location is allowed to choose side scripts.
        let w = base.w.max(lim.w);
        let mut out = MBox::empty();
        let (bw, bh, bd) = (base.w, base.h, base.d);
        out.append(base, (w - bw) / 2.0, 0.0);
        let lw = lim.w;
        if lower {
            let y = bd + c.lower_gap * s + lim.h;
            out.append(lim, (w - lw) / 2.0, -y);
        } else {
            let y = bh + c.upper_gap * s + lim.d;
            out.append(lim, (w - lw) / 2.0, y);
        }
        out.w = w;
        out.class = Some(omml::Class::Ord);
        out
    }

    fn node(&mut self, n: &Node, st: Style) -> MBox {
        self.node_context(n, st, omml::SpacingContext::default())
    }

    fn node_context(&mut self, n: &Node, st: Style, context: omml::SpacingContext) -> MBox {
        if n.namespace==crate::math_spacing::MATHML_NS&&n.name=="mspace"{
            let space=crate::math_spacing::Spacing::from_attributes(n.attrs.iter().map(|(key,value)|(key.as_str(),value.as_str()))).expect("validated MathML space");
            let face=&self.faces[0];let upem=self.f.fonts[0].upem;
            let x_height=face.x_height().filter(|value|*value>0).map(|value|f64::from(value)/upem)
                .or_else(||face.glyph_index('x').and_then(|gid|face.glyph_bounding_box(gid)).filter(|bounds|bounds.y_max>0).map(|bounds|f64::from(bounds.y_max)/upem)).unwrap_or(0.5);
            let scale=st.scale(self.c());
            let (w,h,d)=match space.dimensions(self.base_points*scale,x_height){Ok(value)=>value,Err(error)=>{self.spacing_error=Some(error);self.missing=true;return MBox::empty();}};
            // A spacelike node reserves geometry but supplies neither an
            // operator class nor the surrounding row's font-size context.
            return MBox{w:w/self.base_points,h:h/self.base_points,d:d/self.base_points,..MBox::default()}.mark(n);
        }
        if !n.is_math() { return self.row_context(&n.children, st, context); }
        let mut result=match n.name.as_str() {
            "r" => self.run(n, st),
            "f" => self.fraction(n, st),
            "sSup" | "sSub" | "sSubSup" => {
                let sub=matches!(n.name.as_str(),"sSub"|"sSubSup").then(||self.child_row(n,"sub",st.sub()));
                let sup=matches!(n.name.as_str(),"sSup"|"sSubSup").then(||self.child_row(n,"sup",st.sup()));
                let constrained=!self.measuring&&self.local_width.is_some()&&n.child("e").is_some_and(equation_array::contains_array);
                let mut base=if constrained{self.intrinsic(|ctx|ctx.child_row(n,"e",st))}else{self.child_row(n,"e",st)};
                if constrained {
                    let natural=self.scripts(n,base.clone(),sub.clone(),sup.clone(),st);
                    let width=(self.local_width.unwrap()-(natural.w-base.w)).max(0.0);
                    base=self.with_width(Some(width),|ctx|ctx.child_row(n,"e",st));
                }
                self.scripts(n,base,sub,sup,st)
            }
            "rad" => self.radical(n, st),
            "nary" => self.nary(n, st),
            "d" => self.delimited(n, st),
            "m" => {
                let rows = n.children.iter().filter(|c| c.name == "mr" && c.is_math()).map(|r| r.children.iter().filter(|c| c.name == "e" && c.is_math()).collect()).collect();
                self.table(rows, st, false,Some(n))
            }
            "eqArr" => {
                self.equation_array(n,st)
            }
            "acc" => self.accent(n, st),
            "bar" => self.bar(n, st),
            "limLow" => self.limit(n, st, true),
            "limUpp" => self.limit(n, st, false),
            "borderBox" => {
                let s = self.control_scale(n, st);
                let t = self.c().rule * s;
                let pad = 0.15 * s;
                let inner = self.with_width(self.local_width.map(|w|(w-2.0*pad).max(0.0)),|ctx|ctx.child_row(n,"e",st));
                let (w, h, d) = (inner.w + 2.0 * pad, inner.h + pad, inner.d + pad);
                let mut out = MBox::empty();
                out.append(inner, pad, 0.0);
                if !n.enabled("borderBoxPr","hideBot",false){out.items.push(Item::Rule { x: 0.0, y: -d, w, h: t, color: Paint::Unresolved });}
                if !n.enabled("borderBoxPr","hideTop",false){out.items.push(Item::Rule { x: 0.0, y: h - t, w, h: t, color: Paint::Unresolved });}
                if !n.enabled("borderBoxPr","hideLeft",false){out.items.push(Item::Rule { x: 0.0, y: -d, w: t, h: h + d, color: Paint::Unresolved });}
                if !n.enabled("borderBoxPr","hideRight",false){out.items.push(Item::Rule { x: w - t, y: -d, w: t, h: h + d, color: Paint::Unresolved });}
                for (key,x1,y1,x2,y2) in [("strikeH",0.0,(h-d)/2.0,w,(h-d)/2.0),("strikeV",w/2.0,-d,w/2.0,h),("strikeBLTR",0.0,-d,w,h),("strikeTLBR",0.0,h,w,-d)]{
                    if n.enabled("borderBoxPr",key,false){out.items.push(Item::Line{x1,y1,x2,y2,thickness:t,color:Paint::Unresolved});}
                }
                out.w = w;
                out.h = out.h.max(h);
                out.d = out.d.max(d);
                out.class = Some(omml::Class::Ord);
                out
            }
            "box" => {
                if let Some(e) = n.child("e") {
                    let child_style = st.argument(e);
                    let mut parts = self.row_parts(&e.children, child_style, omml::SpacingContext::default());
                    if !n.enabled("boxPr", "noBreak", false) {
                        parts = e.children.iter().filter(|n| !n.is_props()).zip(parts).map(|(child, part)| self.flow_part(child, part)).collect();
                    }
                    self.hlist(parts, child_style).mark(e)
                } else { MBox::empty() }
            },
            "func" => {
                let mut out=self.with_width(None,|ctx|ctx.child_row(n,"fName",st));
                let x=out.w+0.12*self.control_scale(n,st);
                let e=self.with_width(self.local_width.map(|w|(w-x).max(0.0)),|ctx|ctx.child_row(n,"e",st));
                out.w=x+e.w;out.append(e,x,0.0);out
            },
            "sPre" => {
                let sub=self.child_row(n,"sub",st.sub());let sup=self.child_row(n,"sup",st.sup());
                let prefix_width=if sub.w.max(sup.w)==0.0&&sub.h+sub.d+sup.h+sup.d==0.0{0.0}else{sub.w.max(sup.w)+self.c().space_after_script*self.control_scale(n,st)};
                let base=self.with_width(self.local_width.map(|w|(w-prefix_width).max(0.0)),|ctx|ctx.child_row(n,"e",st));
                let width=sub.w.max(sup.w);
                if width==0.0&&sub.h+sub.d+sup.h+sup.d==0.0 { base } else {
                    // Prefix indices are right aligned towards the base. Keep
                    // the normal script gap instead of overlapping its advance.
                    let pad=|b:MBox|{let dx=width-b.w;let mut b=b.shifted(dx,0.0);b.w=width;b};
                    let prefix=self.scripts(n,MBox::empty(),Some(pad(sub)),Some(pad(sup)),st);
                    let offset=prefix.w;let mut out=MBox::empty();out.w=offset+base.w;
                    out.append(prefix,0.0,0.0);out.append(base,offset,0.0);out
                }
            },
            "phant" => {
                let inherited = if n.enabled("phantPr", "transp", false) { context } else { omml::SpacingContext::default() };
                let mut b = n.child("e").map(|e| self.row_context(&e.children, st.argument(e), inherited).mark(e)).unwrap_or_default();
                if !n.enabled("phantPr", "show", false) { b.items.clear(); b.regions.clear(); }
                if n.enabled("phantPr", "zeroWid", false) { b.w = 0.0; }
                if n.enabled("phantPr", "zeroAsc", false) { b.h = 0.0; }
                if n.enabled("phantPr", "zeroDesc", false) { b.d = 0.0; }
                // The outer phantom owns the entire effective box. Nested
                // masks must not override its explicit zero switches.
                b.regions.retain(|r| !(r.part && r.kind == "phantom-layout"));
                b.part("phantom-layout")
            },
            "groupChr" => {
                let (bottom,sign_baseline,chr)=omml::group_character(n);
                let base=self.child_row(n,"e",if sign_baseline{st.sub()}else{st});
                if chr.is_empty() { base } else {
                    let s=self.control_scale(n,st);
                    let sign=self.horizontal_glyph(chr.chars().next().unwrap(),base.w,s);
                    let (bw,sw)=(base.w,sign.w);let width=bw.max(sw);
                    let y=if bottom{-base.d-0.12*s-sign.h}else{base.h+0.12*s+sign.d};
                    let mut out=MBox::empty();out.w=width;
                    out.append(base,(width-bw)/2.0,if sign_baseline{-y}else{0.0});
                    out.append(sign,(width-sw)/2.0,if sign_baseline{0.0}else{y});out
                }
            },
            _ => self.argument(n, st),
        };
        // Flow markers may cross ordinary breakable boxes only. A fraction,
        // script, phantom or noBreak box is an atomic object in its parent row.
        if n.name != "box" || n.enabled("boxPr", "noBreak", false) {
            result.regions.retain(|r| !(r.part && matches!(r.kind.as_str(), "paragraph-character" | "paragraph-operator" | "paragraph-atom")));
        }
        let format = omml::control_style(n);
        for item in &mut result.items { item.resolve_paint(Paint::from_color(format.color)); }
        result.em = Some(self.control_scale(n, st));
        if n.name != "r" {
            for marker in n.equation_markers.iter().rev() {
                let mut start=MBox::empty().equation_marker(marker.kind,n.attr("readmdId"),None);
                let width=result.w;start.append(result,0.0,0.0);start.w=width;result=start;
            }
        }
        result.mark(n)
    }
}

// ------------------------------------------------------------------ output

/// A laid-out formula, in points once [`Formula::at_size`] is applied.
#[derive(Debug, Clone, PartialEq)]
pub struct Formula {
    /// Width / height above baseline / depth below baseline, in points.
    pub width: f64,
    pub height: f64,
    pub depth: f64,
    items: Vec<Item>,
    regions: Vec<Region>,
    size: f64,
    fonts: Arc<Fonts>,
    pub latex: String,
    pub alignment: Option<omml::MathJustification>,
    /// A frozen layout input, needed when a surrounding paragraph supplies a
    /// different inherited size. Scaling outlines would also scale explicit
    /// Office point sizes, which must instead remain physically unchanged.
    source_xml: Option<Arc<str>>,
    display: bool,
    available_width: Option<f64>,
    equation_diagnostics: Vec<EquationArrayDiagnostic>,
}

fn stack_math_boxes(boxes: Vec<MBox>, alignment: omml::MathJustification, gap: f64) -> MBox {
    let width=boxes.iter().map(|b|b.w).fold(0.0,f64::max);
    let mut out=MBox::empty();out.w=width;
    let (mut y,mut previous_depth)=(0.0,0.0);
    for (index,b) in boxes.into_iter().enumerate(){
        if index>0{y-=previous_depth+b.h+gap;}previous_depth=b.d;
        let x=if alignment==omml::MathJustification::CenterGroup{0.0}else{alignment.offset(width,b.w)};
        out.append(b,x,y);
    }
    out
}

/// Lay out `latex` at `size` points.  `None` when there is no math font, the
/// formula does not parse, or glyphs are missing (the caller shows text).
pub fn layout(latex: &str, display: bool, size: f64) -> Option<Formula> {
    layout_source(latex,None,display,size)
}

pub fn layout_source(latex: &str, xml: Option<&str>, display: bool, size: f64) -> Option<Formula> {
    layout_source_constrained(latex,xml,display,size,None)
}

/// Finite Core-MathML projection measurements, in physical points. The
/// browser retains its own table baselines; only horizontal allocation is
/// shared with the vector solver. This first profile deliberately excludes
/// structured atoms and non-core faces whose native advance is not certified.
pub(crate) struct MathmlEquationArrayGeometry {
    pub solution: equation_array::Solution,
    pub cells: Vec<Vec<[f64; 2]>>,
}

pub(crate) fn mathml_equation_array_geometry(
    array: &Node,
    size: f64,
    width: f64,
) -> Result<Option<MathmlEquationArrayGeometry>, &'static str> {
    if !size.is_finite() || !(4.0..=512.0).contains(&size)
        || !width.is_finite() || !(0.0..=20000.0).contains(&width)
    {
        return Err("math_invalid_property");
    }
    if !array.is_math() || array.name != "eqArr" || needs_managed_fonts(array) {
        return Ok(None);
    }
    fn supported(n: &Node, sized_array: bool) -> bool {
        if n.namespace == crate::math_spacing::MATHML_NS && n.name == "mspace" {
            // A formatted outer control can change a native space's inherited
            // em without changing its Office siblings' physical run resets.
            return !sized_array && !n.attrs.iter().any(|(key, _)| key == "mathsize");
        }
        let source = n.equation_origin.as_deref().unwrap_or(n);
        if !n.is_math() || n.name != "r" || omml::literal_run(n) || omml::manual_break(source).is_some() {
            return false;
        }
        let style = omml::run_style(n);
        !style.normal_text && !style.bold && !style.monospace && !style.sans_serif
            && !style.fraktur && !style.double_struck && !style.script
    }
    let rows = array.children.iter().filter(|n| n.is_math() && n.name == "e").collect::<Vec<_>>();
    let plans = rows.iter().map(|n| equation_array::plan_row(n, true)).collect::<Vec<_>>();
    if plans.is_empty() || plans.iter().any(|p| !p.diagnostics.is_empty())
        || plans.iter().map(|p| p.marker_count).sum::<usize>() > equation_array::MAX_MARKERS
        || rows.iter().any(|n| omml::argument_size(n) != 0)
    {
        return Ok(None);
    }
    let columns = plans.iter().map(equation_array::node_columns).collect::<Vec<_>>();
    let count = columns.iter().map(Vec::len).max().unwrap_or(0);
    let sized_array = omml::control_style(array).half_points.is_some();
    if count < 2 || columns.iter().any(|row| row.len() != count || row.iter().any(|column| {
        !column.aligned || column.before.iter().chain(&column.after).any(|n| !supported(n, sized_array))
    })) {
        return Ok(None);
    }
    let Some(fonts) = fonts(false) else { return Ok(None); };
    let mut ctx = Ctx::new(&fonts, size);
    let st = Style { display: false, level: 0, cramped: false };
    let cells = ctx.intrinsic(|ctx| columns.iter().map(|row| row.iter().map(|column| {
        [ctx.equation_atoms(&column.before, st, None).w * size,
         ctx.equation_atoms(&column.after, st, None).w * size]
    }).collect::<Vec<_>>()).collect::<Vec<_>>());
    if let Some(error) = ctx.spacing_error { return Err(error); }
    if ctx.missing { return Ok(None); }
    let measures = cells.iter().map(|row| row.iter().map(|[left, right]| {
        ColumnMeasure { width: left + right, anchor: Some(*left) }
    }).collect::<Vec<_>>()).collect::<Vec<_>>();
    let solution = equation_array::solve(&measures, equation_array::Flags::read(array), Some(width))?;
    Ok(Some(MathmlEquationArrayGeometry { solution, cells }))
}

/// Reuse the actual style/font walk for dimension budgets. Do not guess a
/// script's effective em, or infer a typed budget error from a generic None.
pub(crate) fn spacing_budget(xml:&str,display:bool,size:f64)->Result<(),&'static str>{
    if !xml.contains(crate::math_spacing::MATHML_NS){return Ok(());}
    if !size.is_finite()||size<=0.{return Err("math_invalid_property");}
    let root=omml::parse(xml).map_err(|_|"math_invalid_xml")?;
    let fonts=fonts_for(&root,crate::publication_fonts::needs_cjk(xml)).ok_or("math_unsupported_extension")?;
    spacing_budget_with_fonts(&root,&fonts,display,size)
}

fn spacing_budget_with_fonts(root:&Node,fonts:&Fonts,display:bool,size:f64)->Result<(),&'static str>{
    if !size.is_finite()||size<=0.{return Err("math_invalid_property");}
    let mut ctx=Ctx::new(fonts,size);let st=Style{display,level:0,cramped:false};
    if display{ctx.display_root(&root,st);}else{ctx.row(&root.children,st);}
    ctx.spacing_error.map_or(Ok(()),Err)
}

/// Width is physical points, shared by native previews and output columns.
/// Omitting it preserves the compatibility API's intrinsic-size behavior.
pub fn layout_source_constrained(latex: &str, xml: Option<&str>, display: bool, size: f64, width:Option<f64>) -> Option<Formula> {
    layout_source_using_fonts(latex, xml, display, size, width, None)
}

fn layout_source_using_fonts(latex: &str, xml: Option<&str>, display: bool, size: f64, width:Option<f64>, selected:Option<Arc<Fonts>>) -> Option<Formula> {
    if !size.is_finite() || size<=0.0 {return None;}
    if width.is_some_and(|v|!v.is_finite()||v<=0.0||v>20000.0){return None;}
    // Relative LaTeX kern atoms need the same frozen source/font validator as
    // imported mspace when a heading/table later supplies its actual size.
    let generated = if xml.is_none() { crate::math_spacing::latex_spacing_xml(latex,display).ok()? } else { None };
    let xml = xml.or(generated.as_deref());
    let root = match xml {Some(xml)=>omml::parse(xml).ok()?,None=>omml::from_latex(latex,display).ok()?};
    let f = selected.or_else(|| fonts_for(&root,crate::publication_fonts::needs_cjk(xml.unwrap_or(latex))))?;
    let mut ctx = Ctx::new(&f,size);
    ctx.available_width=if display{width}else{None};
    let st = Style { display, level: 0, cramped: false };
    let b = if display {ctx.display_root(&root,st)}else{ctx.row(&root.children,st)};
    let paragraph=root.children.iter().find(|n|n.is_math()&&n.name=="oMathPara");
    let paragraph_style=omml::math_paragraph_style(&root,paragraph);
    let alignment=(display&&paragraph_style.explicit_alignment).then_some(paragraph_style.alignment);
    // An invisible phantom still reserves real dimensions. Falling back to
    // source text here would expose content explicitly hidden by the author.
    if ctx.missing || ctx.spacing_error.is_some() || (xml.is_none() && latex.trim().is_empty()) {
        return None;
    }
    let equation_diagnostics=ctx.equation_diagnostics;
    Some(Formula { width: b.w * size, height: b.h * size, depth: b.d * size, items: b.items, regions:b.regions, size, fonts:f, latex: latex.to_string(), alignment, source_xml:xml.map(Arc::from), display, available_width:width, equation_diagnostics })
}

struct PathOut<'a> {
    s: &'a mut String,
    k: f64,
    x: f64,
    y: f64,
    shear:f64,
    x_scale:f64,
}

#[cfg(test)]
mod spacing_frozen_font_tests{
    use super::*;
    fn spacing(source:&str,size:f64)->Formula{
        let doc=crate::math_document::parse(source,"mathml",false).unwrap();layout_source("",Some(&doc.root.xml()),false,size).unwrap()
    }
    #[test]fn mathml_space_pdf_retarget_keeps_frozen_fonts_and_physical_sizes(){
        let relative=spacing("<math><mspace width='1em'/></math>",12.);
        let changed=relative.spacing_at_size(24.).unwrap().unwrap();assert_eq!(relative.width,12.);assert_eq!(changed.width,24.);
        assert!(Arc::ptr_eq(&relative.fonts,&changed.fonts));assert_eq!(relative.source_xml,changed.source_xml);
        let physical=spacing("<math><mspace width='1em' mathsize='18pt'/></math>",12.);
        let changed=physical.spacing_at_size(24.).unwrap().unwrap();assert_eq!(changed.width,18.);assert!(Arc::ptr_eq(&physical.fonts,&changed.fonts));
    }
    #[test]fn mathml_space_pdf_retarget_reports_budget_and_leaves_plain_formula_compatibility(){
        let relative=spacing("<math><mspace width='1000em'/></math>",12.);assert_eq!(relative.width,12000.);
        assert_eq!(relative.spacing_at_size(24.).unwrap_err(),"math_budget");assert_eq!(relative.width,12000.);
        for size in [0.,-1.,f64::NAN,f64::INFINITY]{assert_eq!(relative.spacing_at_size(size).unwrap_err(),"math_invalid_property");}
        let ordinary=layout("x^2",false,12.).unwrap();assert!(ordinary.spacing_at_size(f64::NAN).unwrap().is_none());assert!(ordinary.at_size(f64::NAN).is_none());
    }
}

impl PathOut<'_> {
    fn p(&self, x: f32, y: f32) -> (String, String) {
        (num(self.x + (x as f64+self.shear*y as f64) * self.k * self.x_scale), num(self.y + y as f64 * self.k))
    }
}

fn num(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    let s = format!("{r:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    if s == "-0" || s.is_empty() {
        "0".into()
    } else {
        s
    }
}

impl OutlineBuilder for PathOut<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let (a, b) = self.p(x, y);
        self.s.push_str(&format!("{a} {b} m\n"));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let (a, b) = self.p(x, y);
        self.s.push_str(&format!("{a} {b} l\n"));
    }
    fn quad_to(&mut self, _x1: f32, _y1: f32, x: f32, y: f32) {
        // Only reached through `QuadFix`, which converts quadratics exactly.
        self.line_to(x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (a, b) = self.p(x1, y1);
        let (c, d) = self.p(x2, y2);
        let (e, f) = self.p(x, y);
        self.s.push_str(&format!("{a} {b} {c} {d} {e} {f} c\n"));
    }
    fn close(&mut self) {
        self.s.push_str("h\n");
    }
}

/// Exact quadratic→cubic conversion needs the start point; this builder keeps it.
struct QuadFix<'a> {
    inner: PathOut<'a>,
    cur: (f32, f32),
    start: (f32, f32),
}

impl OutlineBuilder for QuadFix<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.cur = (x, y);
        self.start = (x, y);
        self.inner.move_to(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.cur = (x, y);
        self.inner.line_to(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (x0, y0) = self.cur;
        let c1 = (x0 + 2.0 / 3.0 * (x1 - x0), y0 + 2.0 / 3.0 * (y1 - y0));
        let c2 = (x + 2.0 / 3.0 * (x1 - x), y + 2.0 / 3.0 * (y1 - y));
        self.inner.curve_to(c1.0, c1.1, c2.0, c2.1, x, y);
        self.cur = (x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.cur = (x, y);
        self.inner.curve_to(x1, y1, x2, y2, x, y);
    }
    fn close(&mut self) {
        self.cur = self.start;
        self.inner.close();
    }
}

impl Formula {
    /// Layout limitations and overflow are separate from source preservation.
    pub fn equation_array_diagnostics(&self) -> &[EquationArrayDiagnostic] { &self.equation_diagnostics }
    /// Re-layout at the actual inherited point size using the same source and
    /// font snapshot. Absolute Office sizes/gaps are resolved anew, never
    /// uniformly enlarged along with relative mathematical content.
    pub fn at_size(&self, size: f64) -> Option<Formula> {
        if !size.is_finite() || size <= 0.0 { return None; }
        if size == self.size { return Some(self.clone()); }
        layout_source_using_fonts(&self.latex, self.source_xml.as_deref(), self.display, size, self.available_width, Some(self.fonts.clone()))
    }

    /// Query the frozen source for page-frame validation without changing fonts.
    pub(crate) fn has_spacing(&self)->Result<bool,&'static str>{
        let Some(xml)=self.source_xml.as_deref().filter(|xml|xml.contains(crate::math_spacing::MATHML_NS))else{return Ok(false);};
        Ok(crate::math_spacing::contains_spacing(&crate::math_document::parse_xml(xml)?))
    }

    /// Retarget only a real spacing formula through the typed path. None
    /// means ordinary Office/TeX math and preserves its legacy compatibility.
    /// Both the budget walk and re-layout use this formula's frozen fonts.
    pub(crate) fn spacing_at_size(&self,size:f64)->Result<Option<Formula>,&'static str>{
        let Some(xml)=self.source_xml.as_deref().filter(|xml|xml.contains(crate::math_spacing::MATHML_NS))else{return Ok(None);};
        let source=crate::math_document::parse_xml(xml)?;
        if !crate::math_spacing::contains_spacing(&source){return Ok(None);}
        let root=omml::parse(xml).map_err(|_|"math_invalid_xml")?;
        spacing_budget_with_fonts(&root,&self.fonts,self.display,size)?;
        let result=if size==self.size{self.clone()}else{
            layout_source_using_fonts(&self.latex,Some(xml),self.display,size,self.available_width,Some(self.fonts.clone())).ok_or("math_unsupported_extension")?
        };Ok(Some(result))
    }

    /// Bounds of painted glyphs, rules and strokes in em coordinates. Logical
    /// advance widths are not sufficient: italic overhangs and assembled
    /// delimiters can extend beyond them.
    fn ink_bounds(&self) -> Option<(f64, f64, f64, f64)> {
        let faces: Vec<_> = self.fonts.fonts.iter().map(|font| font.face()).collect();
        let mut bounds: Option<(f64,f64,f64,f64)> = None;
        for item in &self.items {
            let rect = match *item {
                Item::Glyph{font,gid,x,y,scale,shear,x_scale,..} => {
                    let Some(Some(face))=faces.get(font as usize) else {continue};
                    let Some(b)=face.glyph_bounding_box(GlyphId(gid)) else {continue};
                    let k=scale/self.fonts.fonts[font as usize].upem;
                    let slant_a=shear*f64::from(b.y_min);
                    let slant_b=shear*f64::from(b.y_max);
                    (x+(f64::from(b.x_min)+slant_a.min(slant_b))*k*x_scale,y+f64::from(b.y_min)*k,
                        x+(f64::from(b.x_max)+slant_a.max(slant_b))*k*x_scale,y+f64::from(b.y_max)*k)
                },
                Item::Rule{x,y,w,h,..} => (x,y,x+w,y+h),
                Item::Line{x1,y1,x2,y2,thickness,..} => {
                    let pad=thickness/2.0;(x1.min(x2)-pad,y1.min(y2)-pad,x1.max(x2)+pad,y1.max(y2)+pad)
                },
            };
            bounds=Some(match bounds {Some((a,b,c,d))=>(a.min(rect.0),b.min(rect.1),c.max(rect.2),d.max(rect.3)),None=>rect});
        }
        bounds
    }

    /// A reading/editing preview using exactly the PDF glyphs and positions.
    /// Actual ink and transparent source-bound selection regions stay separate;
    /// native tests can measure painted geometry rather than selection boxes.
    pub fn interactive_svg(&self) -> String {
        self.normalized_svg(true)
    }

    /// Static HTML uses the same ink bounds and glyph positions, without
    /// editor selection IDs that would collide across repeated formulas.
    pub(crate) fn publication_svg(&self) -> String {
        self.normalized_svg(false)
    }

    fn normalized_svg(&self, interactive: bool) -> String {
        // Keep layout in real points (Office absolute spacing depends on it),
        // then normalize only drawing coordinates to 1000 units/em for editor
        // hit targets. Laying out at 1000pt would shrink absolute gaps.
        let mut drawing=self.clone();let scale=1000.0/self.size;
        drawing.width*=scale;drawing.height*=scale;drawing.depth*=scale;drawing.size=1000.0;
        drawing.interactive_svg_units(interactive)
    }

    fn interactive_svg_units(&self, interactive: bool) -> String {
        let (mut left,mut bottom,mut right,mut top)=(0.0f64,-self.depth/self.size,self.width/self.size,self.height/self.size);
        if let Some((a,b,c,d))=self.ink_bounds(){left=left.min(a);bottom=bottom.min(b);right=right.max(c);top=top.max(d);}
        // A small antialiasing allowance; it cannot compensate for incorrect
        // operator placement, which is checked separately against operand ink.
        let pad=0.01;left-=pad;bottom-=pad;right+=pad;top+=pad;
        let width=(right-left).max(0.01);let height=(top-bottom).max(0.01);
        let alignment=self.alignment.map(|a|format!(r#" data-math-justification="{}""#,a.as_str())).unwrap_or_default();
        let mut out=format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}em" height="{}em" viewBox="{} {} {} {}" style="vertical-align:{}em" aria-hidden="true"{alignment}><g data-math-ink="true" fill="currentColor" transform="scale(1 -1)">"#,
            num(width),num(height),num(left*self.size),num(-top*self.size),num(width*self.size),num(height*self.size),num(bottom));
        let mut starts=vec![Vec::new();self.items.len()+1];
        for region in &self.regions {if region.from<region.to&&region.to<=self.items.len()&&!matches!(region.kind.as_str(),"flow-character"|"paragraph-character"|"paragraph-atom"){starts[region.from].push(region);}}
        let mut active:Vec<&Region>=vec![];
        for (index,item) in self.items.iter().enumerate(){
            active.retain(|r|r.to>index);active.extend(starts[index].iter().copied());
            let owners=active.iter().filter(|r|!r.part).map(|r|r.id.as_str()).collect::<Vec<_>>().join(" ");
            let parts=active.iter().filter(|r|r.part).map(|r|r.kind.as_str()).collect::<Vec<_>>().join(" ");
            let baseline=active.iter().find(|r|r.part&&r.kind=="auto-wrap-line").map(|r|format!(r#" data-math-line-baseline="{}""#,num(r.y*self.size))).unwrap_or_default();
            out.push_str(&format!(r#"<g data-math-owners="{owners}" data-math-parts="{parts}"{baseline}>"#));
            self.svg_item(&mut out,item,"currentColor");out.push_str("</g>");
        }
        if !interactive {out.push_str("</g></svg>");return out;}
        out.push_str("</g><g data-math-selection-regions=\"true\">");
        let mut regions:Vec<_>=self.regions.iter().filter(|r|!r.part).collect();
        // Containers first, leaf regions last, so a click on a character does
        // not get intercepted by the containing numerator or matrix slot.
        regions.sort_by(|a,b|((b.w*(b.h+b.d)).total_cmp(&(a.w*(a.h+a.d)))).then_with(||a.id.len().cmp(&b.id.len())));
        let mut groups:Vec<Vec<&Region>>=Vec::new();let mut group_ids=HashMap::new();
        for r in regions{let index=*group_ids.entry(r.id.as_str()).or_insert_with(||{groups.push(Vec::new());groups.len()-1});groups[index].push(r);}
        for group in groups{
            let r=group[0];let kind=r.kind.replace('&',"&amp;").replace('"',"&quot;").replace('<',"&lt;");
            let text=r.text_range.map(|(from,to)|format!(r#" data-math-text-from="{from}" data-math-text-to="{to}""#)).unwrap_or_default();
            out.push_str(&format!(r#"<g id="{}" data-math-node="{kind}"{text}>"#,r.id));
            for r in group{
                let(w,h)=if r.w==0.0&&(r.h+r.d)==0.0{(0.35,1.0)}else{(r.w,r.h+r.d)};
                let y=if r.h+r.d==0.0{-r.y-0.8}else{-r.y-r.h};
                out.push_str(&format!(r#"<rect x="{}" y="{}" width="{}" height="{}" fill="transparent" pointer-events="none"/>"#,num(r.x*self.size),num(y*self.size),num(w*self.size),num(h*self.size)));
            }out.push_str("</g>");
        }
        out.push_str("</g></svg>");out
    }

    fn svg_item(&self,out:&mut String,item:&Item,color:&str){
        struct SvgPath {data:String}
        impl OutlineBuilder for SvgPath{
            fn move_to(&mut self,x:f32,y:f32){self.data.push_str(&format!("M{x} {y}"));}
            fn line_to(&mut self,x:f32,y:f32){self.data.push_str(&format!("L{x} {y}"));}
            fn quad_to(&mut self,x1:f32,y1:f32,x:f32,y:f32){self.data.push_str(&format!("Q{x1} {y1} {x} {y}"));}
            fn curve_to(&mut self,x1:f32,y1:f32,x2:f32,y2:f32,x:f32,y:f32){self.data.push_str(&format!("C{x1} {y1} {x2} {y2} {x} {y}"));}
            fn close(&mut self){self.data.push('Z');}
        }
        match *item{
            Item::Glyph{font,gid,x,y,scale,shear,x_scale,color:own}=>{
                let Some(font)=self.fonts.fonts.get(font as usize) else{return};let Some(face)=font.face()else{return};
                let mut outline=SvgPath{data:String::new()};if face.outline_glyph(GlyphId(gid),&mut outline).is_some(){
                    let k=self.size*scale/font.upem;
                    let fill=if let Paint::Rgb(_)=own{format!(" fill=\"{}\"",own.svg(color))}else{String::new()};
                    let skew=if shear!=0.0{format!(" matrix(1 0 {shear} 1 0 0)")}else{String::new()};
                    let transform = if x_scale == 1.0 { format!("scale({k})") } else { format!("scale({} {k})", k*x_scale) };
                    out.push_str(&format!(r#"<path transform="translate({} {}) {transform}{skew}" d="{}"{fill}/>"#,x*self.size,y*self.size,outline.data));
                }
            },
            Item::Rule{x,y,w,h,color:own}=>{
                let fill=if let Paint::Rgb(_)=own{format!(" fill=\"{}\"",own.svg(color))}else{String::new()};
                out.push_str(&format!(r#"<rect x="{}" y="{}" width="{}" height="{}"{fill}/>"#,x*self.size,y*self.size,w*self.size,h*self.size));
            },
            Item::Line{x1,y1,x2,y2,thickness,color:own}=>out.push_str(&format!(r#"<path d="M{} {}L{} {}" fill="none" stroke="{}" stroke-width="{}"/>"#,x1*self.size,y1*self.size,x2*self.size,y2*self.size,own.svg(color),thickness*self.size)),
        }
    }

    /// Interchange text for the PDF semantic layer. A wholly invisible phantom
    /// has no copy target. Preserve mathematical characters, but normalize
    /// source line breaks/control characters to spaces in the one-line layer.
    pub fn copy_text(&self) -> String {
        if self.items.is_empty() {
            return String::new();
        }
        self.latex.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
    }

    /// The same glyph positions and outlines used by the PDF backend, expressed
    /// as a self-contained vector image for managed publication engines.
    pub fn svg(&self, color: &str) -> String {
        let color=if color.len()==7 && color.starts_with('#') && color[1..].bytes().all(|b|b.is_ascii_hexdigit()){color}else{"#262626"};
        let width=self.width.max(0.01);let height=(self.height+self.depth).max(0.01);
        let mut out=format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}pt" height="{height}pt" viewBox="0 0 {width} {height}"><g fill="{color}" transform="translate(0 {}) scale(1 -1)">"#,self.height);
        for item in &self.items{self.svg_item(&mut out,item,color);}
        out.push_str("</g></svg>");out
    }
    /// PDF content-stream operators drawing the formula with its baseline at
    /// (`x`, `y`) in `rgb` (already formatted `r g b`).
    pub fn draw(&self, x: f64, y: f64, rgb: &str) -> String {
        let f = &self.fonts;
        let faces: Vec<_> = f.fonts.iter().map(|m| m.face()).collect();
        let mut s = String::new();
        s.push_str(&format!("q\n{rgb} rg\n{rgb} RG\n"));
        for it in &self.items {
            if let Paint::Rgb([r,g,b])=it.paint(){
                let color=format!("{} {} {}",num(f64::from(r)/255.0),num(f64::from(g)/255.0),num(f64::from(b)/255.0));
                s.push_str(&format!("q {color} rg {color} RG\n"));
            }
            match *it {
                Item::Glyph { font, gid, x: gx, y: gy, scale, shear, x_scale, .. } => {
                    let Some(Some(face)) = faces.get(font as usize) else { if matches!(it.paint(),Paint::Rgb(_)){s.push_str("Q\n");} continue };
                    let k = self.size * scale / f.fonts[font as usize].upem;
                    let mut b = QuadFix {
                        inner: PathOut { s: &mut s, k, x: x + gx * self.size, y: y + gy * self.size, shear, x_scale },
                        cur: (0.0, 0.0),
                        start: (0.0, 0.0),
                    };
                    if face.outline_glyph(GlyphId(gid), &mut b).is_some() {
                        s.push_str("f\n");
                    }
                }
                Item::Rule { x: rx, y: ry, w, h, .. } => {
                    s.push_str(&format!(
                        "{} {} {} {} re f\n",
                        num(x + rx * self.size),
                        num(y + ry * self.size),
                        num(w * self.size),
                        num(h * self.size)
                    ));
                }
                Item::Line {x1,y1,x2,y2,thickness,..}=>{
                    s.push_str(&format!("{} w {} {} m {} {} l S\n",num(thickness*self.size),num(x+x1*self.size),num(y+y1*self.size),num(x+x2*self.size),num(y+y2*self.size)));
                },
            }
            if matches!(it.paint(),Paint::Rgb(_)){s.push_str("Q\n");}
        }
        s.push_str("Q\n");
        s
    }
}

#[cfg(test)]
#[path = "math_layout_font_geometry_tests.rs"]
mod font_geometry_tests;

#[cfg(test)]
#[path = "math_layout_equation_array_tests.rs"]
mod equation_array_tests;

#[cfg(test)]
#[path = "math_literal_semantics_tests.rs"]
mod literal_semantics_tests;

#[cfg(test)]
#[path = "math_operator_emulator_tests.rs"]
mod operator_emulator_tests;

#[cfg(test)]
mod tests {

    #[test]
    fn math_differential_spacing_changes_geometry_without_changing_ink() {
        let source = |prop: &str| format!("<m:oMath><m:r><m:t>x</m:t></m:r><m:box><m:boxPr>{prop}</m:boxPr><m:e><m:r><m:t>dx</m:t></m:r></m:e></m:box></m:oMath>");
        for size in [12., 24.] {
            let off = layout_source("", Some(&source("<m:diff m:val=\"0\"/>")), true, size).unwrap();
            for prop in ["<m:diff/>", "<m:diff m:val=\"true\"/>"] {
                let on = layout_source("", Some(&source(prop)), true, size).unwrap();
                assert!((on.width - off.width - size / 6.).abs() < 1e-8);
                let glyphs = |f: &Formula| f.items.iter().filter_map(|i| if let Item::Glyph { font, gid, scale, .. } = i { Some((*font, *gid, *scale)) } else { None }).collect::<Vec<_>>();
                assert_eq!(glyphs(&on), glyphs(&off));
                let positions = |f: &Formula| f.items.iter().filter_map(|i| if let Item::Glyph { x, .. } = i { Some(*x * f.size) } else { None }).collect::<Vec<_>>();
                assert_eq!(positions(&on)[0], positions(&off)[0]);
                assert!((positions(&on)[1] - positions(&off)[1] - size / 6.).abs() < 1e-8);
            }
        }
    }

    #[test]
    fn math_phantom_transparency_preserves_context_not_opacity() {
        let run = |t: &str| format!("<m:r><m:t>{t}</m:t></m:r>");
        let inside = format!("{}{}{}", run("+"), run("y"), run("+"));
        let plain = format!("<m:oMath>{}{}{}</m:oMath>", run("x"), inside, run("z"));
        let baseline = layout_source("", Some(&plain), true, 12.).unwrap();
        let phantom = |transp: &str, show: &str, body: &str, zero: &str| format!("<m:phant><m:phantPr>{transp}{show}{zero}</m:phantPr><m:e>{body}</m:e></m:phant>");
        for nested in [false, true] {
            let body = if nested { phantom("<m:transp/>", "<m:show/>", &inside, "") } else { inside.clone() };
            for show in ["<m:show/>", "<m:show m:val=\"0\"/>"] {
                let content = phantom("<m:transp/>", show, &body, "");
                let source = format!("<m:oMath>{}{content}{}</m:oMath>", run("x"), run("z"));
                let actual = layout_source("", Some(&source), true, 12.).unwrap();
                assert!((actual.width - baseline.width).abs() < 1e-8, "nested={nested}, show={show}");
                assert_eq!(actual.items.len(), if show == "<m:show/>" { baseline.items.len() } else { 2 });
            }
        }
        for op in ["+", "="] {
            let source = |transp: &str, zero: &str| format!("<m:oMath>{}{}{}</m:oMath>", run("x"), phantom(transp, "<m:show/>", &run(op), zero), run("z"));
            let opaque = layout_source("", Some(&source("<m:transp m:val=\"false\"/>", "")), true, 12.).unwrap();
            let transparent = layout_source("", Some(&source("<m:transp/>", "")), true, 12.).unwrap();
            let expected = if op == "+" { 2. * 4. / 18. * 12. } else { 2. * 5. / 18. * 12. };
            assert!((transparent.width - opaque.width - expected).abs() < 1e-8);
            let zero = layout_source("", Some(&source("<m:transp/>", "<m:zeroWid/><m:zeroAsc/><m:zeroDesc/>")), true, 12.).unwrap();
            let bare = layout_source("", Some(&format!("<m:oMath>{}{}</m:oMath>", run("x"), run("z"))), true, 12.).unwrap();
            assert!((zero.width - bare.width - expected).abs() < 1e-8);
        }
        // At row start the same transparent '+' is unary, with no binary gap.
        let leading = format!("<m:oMath>{}{}</m:oMath>", phantom("<m:transp/>", "<m:show/>", &run("+"), ""), run("z"));
        let unwrapped = format!("<m:oMath>{}{}</m:oMath>", run("+"), run("z"));
        assert_eq!(layout_source("", Some(&leading), true, 12.).unwrap().width, layout_source("", Some(&unwrapped), true, 12.).unwrap().width);
    }

    #[test]
    fn math_phantom_differential_and_hidden_source_regions_remain_separate() {
        let diff = "<m:box><m:boxPr><m:diff/></m:boxPr><m:e><m:r readmdId=\"rd-formula-token-omml-0-7\"><m:t>dx</m:t></m:r></m:e></m:box>";
        let source = |props: &str| format!("<m:oMath><m:r><m:t>x</m:t></m:r><m:phant readmdId=\"rd-formula-token-omml-0-3\"><m:phantPr>{props}</m:phantPr><m:e>{diff}</m:e></m:phant></m:oMath>");
        let opaque = layout_source("", Some(&source("")), true, 12.).unwrap();
        let transparent = layout_source("", Some(&source("<m:transp/>")), true, 12.).unwrap();
        assert!((transparent.width - opaque.width - 2.).abs() < 1e-8);
        assert_eq!(transparent.items.len(), 1);
        assert!(!transparent.interactive_svg().contains("rd-formula-token-omml-0-7"));
        assert!(transparent.interactive_svg().contains("rd-formula-token-omml-0-3"));
    }

    #[test]
    fn math_script_phantom_spacing_uses_the_actual_script_level_in_both_outputs() {
        let run = |t: &str| format!("<m:r><m:t>{t}</m:t></m:r>");
        let script = |body: &str| format!("<m:oMath><m:sSup><m:e>{}</m:e><m:sup>{body}</m:sup></m:sSup></m:oMath>", run("x"));
        for operator in ["+", "="] {
            let plain = script(&format!("{}{}{}", run("a"), run(operator), run("b")));
            let wrapped = script(&format!("{}<m:phant><m:phantPr><m:show/><m:transp/></m:phantPr><m:e>{}</m:e></m:phant>{}", run("a"), run(operator), run("b")));
            let a = layout_source("", Some(&plain), true, 12.).unwrap();
            let b = layout_source("", Some(&wrapped), true, 12.).unwrap();
            assert!((a.width - b.width).abs() < 1e-8);
            let mathml = omml::to_mathml(&omml::parse(&wrapped).unwrap(), true, "");
            assert!(!mathml.contains("<mspace"), "{operator}: {mathml}");
            assert!(!mathml.contains("mathsize="), "Spacing alone must not force absolute font sizes");
        }
        let differential = |on: bool| script(&format!("{}<m:box><m:boxPr><m:diff m:val=\"{on}\"/></m:boxPr><m:e>{}</m:e></m:box>", run("a"), run("dx")));
        let off = layout_source("", Some(&differential(false)), true, 12.).unwrap();
        let on = layout_source("", Some(&differential(true)), true, 12.).unwrap();
        assert!((on.width - off.width - 12. * script_scale(1) / 6.).abs() < 1e-8);
        let mathml = omml::to_mathml(&omml::parse(&differential(true)).unwrap(), true, "");
        assert_eq!(mathml.matches("width=\"0.16666667em\"").count(), 1);
    }
    #[test]
    fn math_box_explicit_no_break_false_enables_internal_wrapping() {
        let source = |props: &str, nested: bool| {
            let box_xml = format!("<m:box><m:boxPr>{props}</m:boxPr><m:e><m:r><m:t>a+b+c+d+e+f+g+h+i+j+k+l+m+n+o+p</m:t></m:r></m:e></m:box>");
            let body = if nested { format!("<m:f><m:num>{box_xml}</m:num><m:den><m:r><m:t>1</m:t></m:r></m:den></m:f>") } else { box_xml };
            format!("<m:mathPr><m:wrapIndent m:val=\"0\"/></m:mathPr><m:oMath>{body}</m:oMath>")
        };
        for prop in ["<m:noBreak m:val=\"0\"/>", "<m:noBreak m:val=\"false\"/>", ""] {
            let f = layout_source_constrained("", Some(&source(prop, false)), true, 12., Some(90.)).unwrap();
            assert!(f.regions.iter().filter(|r| r.kind == "auto-wrap-line").count() > 1, "{prop}");
            assert!(f.width <= 90. + 1e-8);
            assert_eq!(f.items.len(), 31);
        }
        for (prop, nested) in [("<m:noBreak/>", false), ("<m:noBreak m:val=\"true\"/>", false), ("<m:noBreak m:val=\"0\"/>", true)] {
            let f = layout_source_constrained("", Some(&source(prop, nested)), true, 12., Some(90.)).unwrap();
            assert!(!f.regions.iter().any(|r| r.kind == "auto-wrap-line"));
        }
    }
    fn kern_oracle() -> serde_json::Value {
        serde_json::from_str(include_str!("../tests/fixtures/math-kern-stix.json")).unwrap()
    }

    #[test]
    fn compound_scripts_match_independent_vertical_font_oracle() {
        let oracle=kern_oracle();let cases=oracle["verticalCases"].as_array().unwrap();assert_eq!(cases.len(),256);
        let f=fonts(false).unwrap();let c=load_constants(&f.fonts[0]);assert_eq!(c.sup_bottom_max_with_sub,oracle["superscriptBottomMaxWithSubscript"].as_f64().unwrap());
        assert!(cases.iter().any(|r|r["raised"].as_f64().unwrap()>0.&&r["lowered"].as_f64().unwrap()>0.));assert!(cases.iter().any(|r|r["raised"]==0.));
        for case in cases {for display in [false,true]{for size in [12.,24.]{
            let xml=format!("<m:oMath><m:sSubSup><m:e><m:r><m:t>{}</m:t></m:r></m:e><m:sub><m:r><m:t>{}</m:t></m:r></m:sub><m:sup><m:r><m:t>{}</m:t></m:r></m:sup></m:sSubSup></m:oMath>",case["base"].as_str().unwrap(),case["sub"].as_str().unwrap(),case["sup"].as_str().unwrap());
            let drawing=layout_source("",Some(&xml),display,size).unwrap();let ys=drawing.items.iter().filter_map(|i|if let Item::Glyph{y,..}=i{Some(*y)}else{None}).collect::<Vec<_>>();assert_eq!(ys.len(),3);
            assert!((ys[1]-case["supY"].as_f64().unwrap()).abs()<1e-8,"sup {case}: {ys:?}");assert!((ys[2]-case["subY"].as_f64().unwrap()).abs()<1e-8,"sub {case}: {ys:?}");
            let ink_gap=(ys[1]-case["supDepth"].as_f64().unwrap())-(case["subHeight"].as_f64().unwrap()+ys[2]);assert!(ink_gap+1e-8>=case["requiredGap"].as_f64().unwrap());
        }}}
    }

    #[test]
    fn isolated_scripts_keep_their_original_font_shifts() {
        let oracle=kern_oracle();let sample=&oracle["verticalCases"][0];
        for (kind,slot,text,y) in [("sSup","sup",sample["sup"].as_str().unwrap(),sample["supY"].as_f64().unwrap()-sample["raised"].as_f64().unwrap()),("sSub","sub",sample["sub"].as_str().unwrap(),sample["subY"].as_f64().unwrap()+sample["lowered"].as_f64().unwrap())]{
            let xml=format!("<m:oMath><m:{kind}><m:e><m:r><m:t>𝑓</m:t></m:r></m:e><m:{slot}><m:r><m:t>{text}</m:t></m:r></m:{slot}></m:{kind}></m:oMath>");
            for display in [false,true]{let drawing=layout_source("",Some(&xml),display,12.).unwrap();let ys=drawing.items.iter().filter_map(|i|if let Item::Glyph{y,..}=i{Some(*y)}else{None}).collect::<Vec<_>>();assert_eq!(ys.len(),2);assert!((ys[1]-y).abs()<1e-8);}
        }
    }

    #[test]
    fn math_corner_kerns_match_independent_font_tables_at_all_interval_edges() {
        use sha2::{Digest,Sha256};
        let oracle=kern_oracle();let f=fonts(false).unwrap();let ctx=Ctx::new(&f,12.);
        assert_eq!(format!("{:x}",Sha256::digest(&*f.fonts[0].data)),oracle["fontSha256"].as_str().unwrap());
        let upem=oracle["unitsPerEm"].as_f64().unwrap();assert_eq!(oracle["coveredGlyphs"],233);
        assert_eq!(oracle["cases"].as_array().unwrap().len(),4411);
        let corners=[KernCorner::TopRight,KernCorner::TopLeft,KernCorner::BottomRight,KernCorner::BottomLeft];
        for case in oracle["cases"].as_array().unwrap() {
            let gid=case[0].as_u64().unwrap() as u16;let corner=corners[case[1].as_u64().unwrap() as usize];
            for scale in [0.5,0.7,1.,2.] { for y in [0.,0.23] {
                let b=MBox{items:vec![Item::Glyph{font:0,gid,x:0.,y,scale,shear:0.,x_scale:1.,color:Paint::Auto}],..MBox::default()};
                let height=case[2].as_f64().unwrap()*scale/upem+y;let expected=case[3].as_f64().unwrap()*scale/upem;
                assert!((ctx.corner_kern(&b,corner,height)-expected).abs()<1e-12,"gid {gid}, scale {scale}, y {y}, case {case}");
            }}
        }
    }

    #[test]
    fn math_corner_kerns_place_real_scripts_and_alignment_uses_outer_column() {
        let oracle=kern_oracle();
        for pair in oracle["pairs"].as_array().unwrap() {
            let run=|key:&str|format!("<m:r><m:t>{}</m:t></m:r>",pair[key].as_str().unwrap());
            let xml=|property:&str|format!("<m:oMath><m:sSubSup>{property}<m:e>{}</m:e><m:sub>{}</m:sub><m:sup>{}</m:sup></m:sSubSup></m:oMath>",run("base"),run("sub"),run("sup"));
            for size in [12.,24.] { for display in [true,false] {
                let f=layout_source("",Some(&xml("")),display,size).unwrap();
                let positions=|f:&Formula|f.items.iter().filter_map(|i|if let Item::Glyph{x,y,..}=i{Some((*x,*y))}else{None}).collect::<Vec<_>>();
                let pos=positions(&f);assert_eq!(pos.len(),3);
                for (actual,key) in [(pos[1].0,"supX"),(pos[1].1,"supY"),(pos[2].0,"subX"),(pos[2].1,"subY")] {
                    assert!((actual-pair[key].as_f64().unwrap()).abs()<1e-10,"{pair}, {key}, got {actual}");
                }
                for value in ["0","false","off"] {assert_eq!(layout_source("",Some(&xml(&format!(r#"<m:sSubSupPr><m:alnScr m:val="{value}"/></m:sSubSupPr>"#))),display,size).unwrap().items,f.items);}
                let aligned=layout_source("",Some(&xml("<m:sSubSupPr><m:alnScr/></m:sSubSupPr>")),display,size).unwrap();
                let on=positions(&aligned);let expected=pos[1].0.max(pos[2].0);assert_eq!(on[1].0,expected);assert_eq!(on[2].0,expected);assert_eq!(on[1].1,pos[1].1);assert_eq!(on[2].1,pos[2].1);
            }}
        }
    }

    #[test]
    fn math_corner_kerns_do_not_apply_a_glyph_table_to_boxes_or_synthetic_slants() {
        let f=fonts(false).unwrap();let mut ctx=Ctx::new(&f,12.);let glyph=ctx.glyph_box('𝑓',1.);
        assert!(ctx.corner_kern(&glyph,KernCorner::BottomRight,-0.2)<0.);
        let mut composite=glyph.clone();composite.append(glyph.clone(),glyph.w,0.);
        assert_eq!(ctx.corner_kern(&composite,KernCorner::BottomRight,-0.2),0.);
        let mut slanted=glyph.clone();if let Item::Glyph{shear,..}=&mut slanted.items[0]{*shear=0.2;}
        assert_eq!(ctx.corner_kern(&slanted,KernCorner::BottomRight,-0.2),0.);
        let mut missing=glyph.clone();if let Item::Glyph{font,..}=&mut missing.items[0]{*font=254;}
        assert_eq!(ctx.corner_kern(&missing,KernCorner::BottomRight,-0.2),0.);
        let uncovered=ctx.glyph_box('2',1.);
        assert_eq!(ctx.corner_kern(&uncovered,KernCorner::BottomRight,-0.2),0.);
    }

    #[test]
    fn math_text_fragments_retain_utf16_offsets_without_changing_ink() {
        let body="<m:r><m:rPr><m:nor/></m:rPr><m:t>A𝛂 é</m:t></m:r>";
        let xml=format!("<m:oMath>{body}</m:oMath>");
        let annotated=xml.replace("<m:r>","<m:r readmdId=\"rd-formula-token-omml-x-0-0\">");
        let plain=super::layout_source("",Some(&xml),false,12.).unwrap();let edited=super::layout_source("",Some(&annotated),false,12.).unwrap();
        assert_eq!(plain.items,edited.items);assert_eq!(plain.draw(0.,0.,"0 0 0"),edited.draw(0.,0.,"0 0 0"));
        let ranges=edited.regions.iter().filter_map(|r|r.text_range).collect::<Vec<_>>();assert_eq!(ranges,[(0,1),(1,3),(3,4),(4,5),(5,6)]);
        let svg=edited.interactive_svg();assert!(svg.contains("data-math-text-from=\"1\" data-math-text-to=\"3\""));assert!(svg.contains("data-math-node=\"run-text\""));
    }

    use super::*;

    #[test]
    fn explicit_limits_remain_above_or_below_in_inline_math() {
        for (kind,below) in [("limLow",true),("limUpp",false)] {
            let xml=format!("<m:oMath><m:{kind}><m:e><m:r><m:t>=</m:t></m:r></m:e><m:lim><m:r><m:t>ab</m:t></m:r></m:lim></m:{kind}></m:oMath>");
            let inline=layout_source("",Some(&xml),false,12.).unwrap();let display=layout_source("",Some(&xml),true,12.).unwrap();
            assert_eq!(inline.items,display.items);
            let positions=inline.items.iter().filter_map(|i|if let Item::Glyph{x,y,scale,..}=i{Some((*x,*y,*scale))}else{None}).collect::<Vec<_>>();assert_eq!(positions.len(),3);assert_eq!(positions[0].1,0.0);
            assert_eq!(positions[1].1<0.0,below);assert_eq!(positions[2].1,positions[1].1);assert!(positions[1].2<positions[0].2);
            let width=|text|layout_source("",Some(&format!("<m:oMath><m:r><m:t>{text}</m:t></m:r></m:oMath>")),false,12.).unwrap().width/12.;
            assert!((positions[0].0+width("=")/2.0-positions[1].0-width("ab")*positions[1].2/2.0).abs()<1e-8);
        }
    }

    #[test]
    fn prescript_columns_align_towards_the_base_and_keep_the_script_gap() {
        let plain=|text:&str|format!("<m:r><m:t>{text}</m:t></m:r>");
        for (sub,sup) in [("1234","7"),("2","3456")] {for display in [true,false]{for size in [12.,24.]{
            let xml=format!("<m:oMath><m:sPre><m:sub>{}</m:sub><m:sup>{}</m:sup><m:e>{}</m:e></m:sPre></m:oMath>",plain(sub),plain(sup),plain("x"));
            let f=layout_source("",Some(&xml),display,size).unwrap();let glyphs=f.items.iter().filter_map(|i|if let Item::Glyph{x,scale,..}=i{Some((*x,*scale))}else{None}).collect::<Vec<_>>();
            let width=|text:&str|layout_source("",Some(&format!("<m:oMath>{}</m:oMath>",plain(text))),display,size).unwrap().width/size;
            let upper_right=glyphs[0].0+width(sup)*glyphs[0].1;let lower_right=glyphs[sup.len()].0+width(sub)*glyphs[sup.len()].1;
            assert!((upper_right-lower_right).abs()<1e-8);assert!(glyphs.last().unwrap().0>upper_right);assert!(f.ink_bounds().is_some());
        }}}
        let empty=format!("<m:oMath><m:sPre><m:sub/><m:sup/><m:e>{}</m:e></m:sPre></m:oMath>",plain("x"));
        let f=layout_source("",Some(&empty),false,12.).unwrap();let b=layout_source("",Some(&format!("<m:oMath>{}</m:oMath>",plain("x"))),false,12.).unwrap();assert_eq!(f.items,b.items);assert_eq!(f.width,b.width);
    }

    #[test]
    fn office_script_alignment_changes_actual_columns_and_respects_all_switch_values() {
        let xml=|property:&str|format!("<m:oMath><m:sSubSup>{property}<m:e><m:r><m:t>f</m:t></m:r></m:e><m:sub><m:r><m:t>2</m:t></m:r></m:sub><m:sup><m:r><m:t>3</m:t></m:r></m:sup></m:sSubSup></m:oMath>");
        for display in [true,false] {
            let default=layout_source("",Some(&xml("")),display,12.).unwrap();
            let positions=|f:&Formula|f.items.iter().filter_map(|i|if let Item::Glyph{x,y,..}=i{Some((*x,*y))}else{None}).collect::<Vec<_>>();
            let off=positions(&default);assert!((off[1].0-off[2].0).abs()>0.001,"italic base needs distinct default script starts");
            for value in ["0","false","off"]{let f=layout_source("",Some(&xml(&format!(r#"<m:sSubSupPr><m:alnScr m:val="{value}"/></m:sSubSupPr>"#))),display,12.).unwrap();assert_eq!(f.items,default.items);}
            for prop in ["<m:alnScr/>",r#"<m:alnScr m:val="1"/>"#,r#"<m:alnScr m:val="true"/>"#,r#"<m:alnScr m:val="on"/>"#] {
                let f=layout_source("",Some(&xml(&format!("<m:sSubSupPr>{prop}</m:sSubSupPr>"))),display,12.).unwrap();let on=positions(&f);assert_eq!(on[1].0,on[2].0);assert_eq!(on[0],off[0]);assert_eq!(on[1],off[1]);assert_eq!(on[2].1,off[2].1);
            }
        }
    }

    #[test]
    fn group_characters_resolve_office_defaults_empty_values_and_both_baselines() {
        let xml=|properties:&str|format!("<m:oMath><m:groupChr>{properties}<m:e><m:r><m:t>abcde</m:t></m:r></m:e></m:groupChr></m:oMath>");
        let default=layout_source("",Some(&xml("")),true,12.).unwrap();
        let explicit=layout_source("",Some(&xml(r#"<m:groupChrPr><m:chr m:val="⏟"/><m:pos m:val="bot"/><m:vertJc m:val="top"/></m:groupChrPr>"#)),true,12.).unwrap();
        assert_eq!(default.items,explicit.items);
        let plain=layout_source("",Some("<m:oMath><m:r><m:t>abcde</m:t></m:r></m:oMath>"),true,12.).unwrap();
        let empty=layout_source("",Some(&xml("<m:groupChrPr><m:chr/></m:groupChrPr>")),true,12.).unwrap();
        assert_eq!(empty.items,plain.items);
        assert_eq!((empty.width,empty.height,empty.depth),(plain.width,plain.height,plain.depth));
        for (pos,chr) in [("top","⏞"),("bot","⏟")] {
            for jc in ["top","bot"] {
                let properties=format!(r#"<m:groupChrPr><m:chr m:val="{chr}"/><m:pos m:val="{pos}"/><m:vertJc m:val="{jc}"/></m:groupChrPr>"#);
                let f=layout_source("",Some(&xml(&properties)),true,12.).unwrap();
                let base=f.items.iter().find_map(|i|if let Item::Glyph{y,scale,..}=i{Some((*y,*scale))}else{None}).unwrap();
                if pos==jc {assert!(base.0.abs()>0.1);assert!(base.1<1.0);}else{assert_eq!(base,(0.,1.));}
                let bbox=f.ink_bounds().unwrap();assert!(bbox.0.is_finite()&&bbox.3.is_finite());
            }
        }
        let absent=layout_source("",Some(&xml(r#"<m:groupChrPr><m:pos m:val="bot"/></m:groupChrPr>"#)),true,12.).unwrap();
        let empty_value=layout_source("",Some(&xml(r#"<m:groupChrPr><m:pos m:val="bot"/><m:vertJc/></m:groupChrPr>"#)),true,12.).unwrap();
        assert_ne!(absent.items,empty_value.items);
    }
    #[test]
    fn horizontal_group_assemblies_follow_long_operands_without_thickening_strokes() {
        for chr in ["⏟","⏞"] {
            let body="abcdefghij".repeat(4);
            let xml=format!(r#"<m:oMath><m:groupChr><m:groupChrPr><m:chr m:val="{chr}"/><m:pos m:val="bot"/></m:groupChrPr><m:e><m:r><m:t>{body}</m:t></m:r></m:e></m:groupChr></m:oMath>"#);
            let f=layout_source("",Some(&xml),true,12.).unwrap();
            let sign=&f.items[body.len()..];assert!(sign.len()>3,"long signs use actual assemblies");
            let xs=sign.iter().filter_map(|i|if let Item::Glyph{x,scale,..}=i{assert_eq!(*scale,1.0);Some(*x)}else{None}).collect::<Vec<_>>();
            assert!(xs.last().unwrap()-xs[0]>f.width/f.size*0.8);
            // Short glyph variants legitimately have different outlines from
            // assemblies. Compare two actual assemblies instead.
            let longer=layout_source("",Some(&xml.replace(&body,&body.repeat(2))),true,12.).unwrap();
            assert!(longer.width>f.width*1.9);
            assert_eq!((f.height,f.depth),(longer.height,longer.depth));
        }
        let plain=layout_source("",Some("<m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>"),true,12.).unwrap();
        let empty=layout_source("",Some("<m:oMath><m:acc><m:accPr><m:chr/></m:accPr><m:e><m:r><m:t>x</m:t></m:r></m:e></m:acc></m:oMath>"),true,12.).unwrap();
        assert_eq!(empty.items,plain.items);
    }

    fn word_control_fixture(body: &str) -> String {
        format!(r#"<w:styles><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="32"/><w:color w:val="336699"/></w:rPr></w:rPrDefault></w:docDefaults></w:styles><m:oMath>{body}</m:oMath>"#)
    }

    #[test]
    fn math_word_inherited_control_size_is_physical_across_structures_and_scripts() {
        let run = |s: &str| format!("<m:r><m:t>{s}</m:t></m:r>");
        let a=run("a");let b=run("b");
        let bodies=[
            format!("{a}{}{}",run("+"),b),
            format!("<m:f><m:num>{a}</m:num><m:den>{b}</m:den></m:f>"),
            format!("<m:rad><m:radPr><m:degHide/></m:radPr><m:e>{a}</m:e></m:rad>"),
            format!("<m:nary><m:naryPr><m:chr m:val=\"∑\"/></m:naryPr><m:sub>{a}</m:sub><m:sup>{b}</m:sup><m:e>{a}</m:e></m:nary>"),
            format!("<m:d><m:dPr><m:grow m:val=\"0\"/></m:dPr><m:e>{a}</m:e></m:d>"),
            format!("<m:bar><m:e>{a}</m:e></m:bar>"),
            format!("<m:borderBox><m:borderBoxPr><m:strikeBLTR/></m:borderBoxPr><m:e>{a}</m:e></m:borderBox>"),
            format!("<m:sSubSup><m:e>{a}</m:e><m:sub>{b}</m:sub><m:sup>{b}</m:sup></m:sSubSup>"),
            format!("<m:m><m:mr><m:e>{a}</m:e><m:e>{b}</m:e></m:mr><m:mr><m:e>{b}</m:e><m:e>{a}</m:e></m:mr></m:m>"),
        ];
        for body in bodies { for display in [false,true] {
            let xml=word_control_fixture(&body);
            let a=layout_source("",Some(&xml),display,12.).unwrap();let b=layout_source("",Some(&xml),display,24.).unwrap();
            for (x,y) in [(a.width,b.width),(a.height,b.height),(a.depth,b.depth)] { assert!((x-y).abs()<1e-8,"{body}: {x} != {y}"); }
            assert!(a.items.iter().all(|i|i.paint()==Paint::Rgb([0x33,0x66,0x99])),"{body}");
            assert_eq!(a.items.len(),b.items.len());
            for (x,y) in a.items.iter().zip(&b.items) {
                let measure=|i:&Item,size:f64|match *i {
                    Item::Glyph{x,y,scale,..}=>vec![x*size,y*size,scale*size],
                    Item::Rule{x,y,w,h,..}=>vec![x*size,y*size,w*size,h*size],
                    Item::Line{x1,y1,x2,y2,thickness,..}=>vec![x1*size,y1*size,x2*size,y2*size,thickness*size],
                };
                assert!(measure(x,a.size).iter().zip(measure(y,b.size)).all(|(x,y)|(x-y).abs()<1e-8),"{body}");
            }
        }}
    }

    #[test]
    fn math_word_explicit_control_format_does_not_recolor_or_resize_operands() {
        let run=r#"<m:r><w:rPr><w:sz w:val="24"/><w:color w:val="auto"/></w:rPr><m:t>x</m:t></m:r>"#;
        for (kind,pr,body) in [
            ("f","fPr",format!("<m:num>{run}</m:num><m:den>{run}</m:den>")),
            ("rad","radPr",format!("<m:e>{run}</m:e>")),
            ("nary","naryPr",format!("<m:e>{run}</m:e>")),
            ("d","dPr",format!("<m:e>{run}</m:e>")),
            ("bar","barPr",format!("<m:e>{run}</m:e>")),
            ("acc","accPr",format!("<m:e>{run}</m:e>")),
            ("groupChr","groupChrPr",format!("<m:e>{run}</m:e>")),
            ("borderBox","borderBoxPr",format!("<m:e>{run}</m:e>")),
        ] {
            let xml=|sz|word_control_fixture(&format!(r#"<m:{kind}><m:{pr}><m:ctrlPr><w:rPr><w:sz w:val="{sz}"/><w:color w:val="CC3366"/></w:rPr></m:ctrlPr></m:{pr}>{body}</m:{kind}>"#));
            let small=layout_source("",Some(&xml(24)),true,12.).unwrap();let large=layout_source("",Some(&xml(48)),true,12.).unwrap();
            let operands=|f:&Formula|f.items.iter().filter_map(|i|match i {Item::Glyph{gid,scale,color:Paint::Auto,..}=>Some((*gid,*scale)),_=>None}).collect::<Vec<_>>();
            assert!(!operands(&small).is_empty(),"{kind}");assert_eq!(operands(&small),operands(&large),"{kind}");
            assert!(large.items.iter().any(|i|i.paint()==Paint::Rgb([0xCC,0x33,0x66])),"{kind}");
            assert!(large.items.iter().all(|i|i.paint()!=Paint::Unresolved));
            let svg=large.svg("#EEEEEE");assert!(svg.contains("#CC3366"));
            let pdf=large.draw(0.,0.,"0.9 0.9 0.9");assert!(pdf.contains("0.8 0.2 0.4 rg 0.8 0.2 0.4 RG"));
            assert_eq!(pdf.split_whitespace().filter(|s|*s=="q").count(),pdf.split_whitespace().filter(|s|*s=="Q").count());
        }
    }

    #[test]
    fn math_word_absent_empty_and_auto_control_format_remain_distinct() {
        let numerator=r#"<m:num><m:r><w:rPr><w:color w:val="CC3366"/><w:sz w:val="48"/></w:rPr><m:t>x</m:t></m:r></m:num>"#;
        let denominator="<m:den><m:r><m:t>y</m:t></m:r></m:den>";
        let make=|pr|word_control_fixture(&format!("<m:f>{pr}{numerator}{denominator}</m:f>"));
        let a=layout_source("",Some(&make("")),true,12.).unwrap();
        let b=layout_source("",Some(&make("<m:fPr><m:ctrlPr/></m:fPr>")),true,12.).unwrap();
        let c=layout_source("",Some(&make("<m:fPr><m:ctrlPr><w:rPr><w:color w:val=\"auto\"/></w:rPr></m:ctrlPr></m:fPr>")),true,12.).unwrap();
        let rule=|f:&Formula|f.items.iter().find_map(|i|if let Item::Rule{h,color,..}=i{Some((*h,*color))}else{None}).unwrap();
        assert_eq!(rule(&a).1,Paint::Rgb([0xCC,0x33,0x66]));assert_eq!(rule(&b).1,Paint::Rgb([0x33,0x66,0x99]));assert_eq!(rule(&c).1,Paint::Auto);
        assert!((rule(&a).0/rule(&b).0-1.5).abs()<1e-8);assert_eq!(rule(&b).0,rule(&c).0);
        let svg=c.interactive_svg();assert!(svg.contains("fill=\"#CC3366\""));assert!(!svg.contains("stroke=\"#336699\""));
    }

    #[test]
    fn math_word_tiny_explicit_rules_are_not_thickened_by_the_host_base_size() {
        let xml=r#"<m:oMath><m:f><m:fPr><m:ctrlPr><w:rPr><w:sz w:val="2"/></w:rPr></m:ctrlPr></m:fPr><m:num><m:r><m:t>x</m:t></m:r></m:num><m:den><m:r><m:t>y</m:t></m:r></m:den></m:f></m:oMath>"#;
        let a=layout_source("",Some(xml),true,12.).unwrap();let b=layout_source("",Some(xml),true,24.).unwrap();
        let height=|f:&Formula|f.items.iter().find_map(|i|if let Item::Rule{h,..}=i{Some(*h*f.size)}else{None}).unwrap();
        assert!((height(&a)-height(&b)).abs()<1e-10);assert!(height(&a)<0.02*12.);
        for f in [&a,&b] {
            assert!(f.svg("#262626").contains(&format!("height=\"{}\"",height(f))));
            assert!(f.draw(0.,0.,"0 0 0").contains(&format!("{} re f",num(height(f)))));
        }
    }

    #[test]
    fn math_word_run_font_size_color_and_normal_text_reach_both_vector_outputs() {
        let xml="<m:oMath><m:r><m:rPr><m:nor/></m:rPr><w:rPr><w:b/><w:i/><w:sz w:val=\"36\"/><w:color w:val=\"336699\"/></w:rPr><m:t>Abα</m:t></m:r></m:oMath>";
        let small=layout_source("Abα",Some(xml),false,12.).unwrap();let large=layout_source("Abα",Some(xml),false,24.).unwrap();
        assert!((small.width-large.width).abs()<0.0001);assert!((small.height-large.height).abs()<0.0001);
        let normal=layout_source("Abα",Some("<m:oMath><m:r><m:rPr><m:nor/></m:rPr><w:rPr><w:sz w:val=\"36\"/></w:rPr><m:t>Abα</m:t></m:r></m:oMath>"),false,12.).unwrap();
        let glyphs=|f:&Formula|f.items.iter().filter_map(|i|if let Item::Glyph{gid,..}=i{Some(*gid)}else{None}).collect::<Vec<_>>();
        assert_ne!(glyphs(&small),glyphs(&normal));
        assert!(small.items.iter().all(|i|matches!(i,Item::Glyph{color:Paint::Rgb([0x33,0x66,0x99]),..})));
        assert!(small.interactive_svg().contains("fill=\"#336699\""));assert!(small.svg("#000000").contains("fill=\"#336699\""));
        let pdf=small.draw(20.,100.,"0 0 0");assert_eq!(pdf.matches("q 0.2 0.4 0.6 rg").count(),3);assert_eq!(pdf.matches("Q\n").count(),4);
        let mathml=omml::to_mathml(&omml::parse(xml).unwrap(),false,"");assert!(mathml.contains("mathsize=\"18pt\"")&&mathml.contains("mathcolor=\"#336699\"")&&mathml.contains("<mtext mathvariant=\"bold-italic\">Abα</mtext>"));
    }

    #[test]fn mathml_style_cascade_changes_actual_ink_and_preserves_control_appearance(){
        let source=r##"<math><mstyle mathcolor="#336699" mathsize="24px"><mfrac><mi mathcolor="red" mathsize="12pt">x</mi><mi>y</mi></mfrac></mstyle></math>"##;
        let(xml,diagnostics)=crate::math_formats::from_mathml(source).unwrap();assert!(diagnostics.is_empty());
        let small=layout_source("",Some(&xml),true,12.).unwrap();let large=layout_source("",Some(&xml),true,24.).unwrap();
        assert!((small.width-large.width).abs()<0.0001);assert!((small.height-large.height).abs()<0.0001);
        assert!(small.items.iter().any(|i|matches!(i,Item::Rule{color:Paint::Rgb([0x33,0x66,0x99]),..})));
        assert!(small.items.iter().any(|i|matches!(i,Item::Glyph{color:Paint::Rgb([255,0,0]),..})));
        assert!(small.items.iter().any(|i|matches!(i,Item::Glyph{color:Paint::Rgb([0x33,0x66,0x99]),..})));
        assert!(small.svg("#000000").contains("#336699"));assert!(small.svg("#000000").contains("#FF0000"));
        let mathml=crate::omml::to_mathml(&crate::omml::parse(&xml).unwrap(),true,"");assert!(mathml.contains("mathsize=\"18pt\"")&&mathml.contains("mathsize=\"12pt\""));
    }

    #[test]
    fn math_word_inherited_script_size_and_explicit_math_style_remain_independent() {
        let xml=format!("<w:styles xmlns:w=\"{}\"><w:docDefaults><w:rPrDefault><w:rPr><w:b/><w:i/><w:sz w:val=\"24\"/></w:rPr></w:rPrDefault></w:docDefaults></w:styles><m:oMath><m:sSup><m:e><m:r><m:rPr><m:sty m:val=\"p\"/></m:rPr><m:t>x</m:t></m:r></m:e><m:sup><m:r><m:rPr><m:sty m:val=\"p\"/></m:rPr><m:t>x</m:t></m:r></m:sup></m:sSup></m:oMath>",crate::math_document::WORD_NS);
        let a=layout_source("x^x",Some(&xml),false,12.).unwrap();let b=layout_source("x^x",Some(&xml),false,24.).unwrap();
        let glyphs=|f:&Formula|f.items.iter().filter_map(|i|if let Item::Glyph{gid,scale,..}=i{Some((*gid,*scale*f.size))}else{None}).collect::<Vec<_>>();
        let ga=glyphs(&a);let gb=glyphs(&b);assert_eq!(ga,gb);assert_eq!(ga[0].0,ga[1].0);assert!(ga[1].1<ga[0].1);
        let normal=layout("\\mathrm{x}",false,12.).unwrap();assert_eq!(ga[0].0,glyphs(&normal)[0].0); // explicit m:sty overrides inherited Word bold/italic
    }

    #[test]
    fn math_automatic_wrapping_preserves_operands_size_and_all_operator_policies(){
        let text="a−b−c−d−e−f−g−h−i−j−k−l−m−n−o−p−q−r";
        for size in [12.,24.]{for mode in ["before","after","repeat"]{for signs in ["--","-+","+-"]{
            let xml=format!(r#"<m:mathPr><m:brkBin m:val="{mode}"/><m:brkBinSub m:val="{signs}"/><m:lMargin m:val="240"/><m:rMargin m:val="240"/><m:wrapIndent m:val="240"/></m:mathPr><m:oMath><m:r><m:t>{text}</m:t></m:r></m:oMath>"#);
            let flat=layout_source("",Some(&xml),true,size).unwrap();let f=layout_source_constrained("",Some(&xml),true,size,Some(120.)).unwrap();
            let rows=f.regions.iter().filter(|r|r.part&&r.kind=="auto-wrap-line").collect::<Vec<_>>();assert!(rows.len()>1,"{mode}/{signs}/{size}");assert!(f.width<=120.+1e-8);assert!(f.height+f.depth>flat.height+flat.depth);
            let glyphs=|f:&Formula|f.items.iter().filter_map(|i|if let Item::Glyph{font,gid,scale,..}=i{Some((*font,*gid,*scale))}else{None}).collect::<Vec<_>>();
            let original=glyphs(&flat);let actual=glyphs(&f);assert!(actual.iter().all(|g|g.2==1.0));
            if mode!="repeat"{assert_eq!(actual,original);}else{assert_eq!(actual.len(),original.len()+rows.len()-1);}
            for row in rows.iter().skip(1){assert!((row.x*size-24.).abs()<1e-8);}
            if mode=="repeat"{
                let face=f.fonts.fonts[0].face().unwrap();let gid=|c|face.glyph_index(if c=='-'{ '−' }else{c}).unwrap().0;
                for pair in rows.windows(2){let last=f.items[pair[0].from..pair[0].to].iter().rev().find_map(|i|if let Item::Glyph{gid,..}=i{Some(*gid)}else{None}).unwrap();let first=f.items[pair[1].from..pair[1].to].iter().find_map(|i|if let Item::Glyph{gid,..}=i{Some(*gid)}else{None}).unwrap();assert_eq!(last,gid(signs.chars().next().unwrap()));assert_eq!(first,gid(signs.chars().nth(1).unwrap()));}
            }
        }}}
        let body="<m:oMath><m:r><m:t>a+b+c+d+e+f+g+h+i+j+k+l+m+n+o+p+q+r</m:t></m:r></m:oMath>";
        for setting in ["<m:wrapRight/>",r#"<m:wrapRight m:val="1"/>"#]{let xml=format!("<m:mathPr>{setting}</m:mathPr>{body}");let f=layout_source_constrained("",Some(&xml),true,12.,Some(120.)).unwrap();let rows=f.regions.iter().filter(|r|r.kind=="auto-wrap-line").collect::<Vec<_>>();assert!(rows.len()>1);for row in rows.iter().skip(1){assert!(((row.x+row.w)*12.-120.).abs()<1e-8);}}
    }

    #[test]
    fn math_wrapping_keeps_unary_text_brackets_and_no_break_structures_intact(){
        for run in ["<m:r><m:t>-a</m:t></m:r>","<m:r><m:t>(a+a+a+a+a+a+a+a)</m:t></m:r>","<m:r><m:rPr><m:nor/></m:rPr><m:t>a+b+c+d+e+f</m:t></m:r>"]{
            let xml=format!("<m:oMath>{run}</m:oMath>");let a=layout_source("",Some(&xml),true,12.).unwrap();let b=layout_source_constrained("",Some(&xml),true,12.,Some(10.)).unwrap();assert_eq!(a.items,b.items);assert!(!b.regions.iter().any(|r|r.kind=="auto-wrap-line"));
        }
        let xml="<m:mathPr><m:wrapIndent m:val=\"0\"/></m:mathPr><m:oMath><m:box><m:boxPr><m:noBreak/></m:boxPr><m:e><m:r><m:t>a+a+a+a+a+a+a+a</m:t></m:r></m:e></m:box><m:r><m:t>+</m:t></m:r><m:r><m:t>z</m:t></m:r></m:oMath>";
        let f=layout_source_constrained("",Some(xml),true,12.,Some(60.)).unwrap();let positions=f.items.iter().filter_map(|i|if let Item::Glyph{y,..}=i{Some(*y)}else{None}).collect::<Vec<_>>();assert!(positions[..15].iter().all(|y|*y==positions[0]));assert!(positions[15]<positions[0]);
    }

    #[test]
    fn math_wrapped_source_fragments_have_one_dom_identity_and_retain_readable_source(){
        let source="row(office.mathPr(office.brkBin(val: repeat), office.wrapIndent()), office.oMath(office.r(office.t(\"a+b+c+d+e+f+g+h+i+j+k+l+m+n+o+p\"))))";
        let doc=crate::math_document::parse(source,"dsl",true).unwrap();let svg=crate::math_document::vector_preview_constrained(&doc,true,12.,Some(180.)).unwrap();assert!(svg.contains("auto-wrap-line"));
        let mut reader=quick_xml::Reader::from_str(&svg);let mut ids=std::collections::BTreeSet::new();let mut rects=0;let mut maximum_rects=0;
        loop{match reader.read_event().unwrap(){quick_xml::events::Event::Start(e)=>{if e.local_name().as_ref()=="g"{for a in e.attributes(){let a=a.unwrap();if a.key.as_ref()=="id"{assert!(ids.insert(a.value.to_string()));rects=0;}}}},quick_xml::events::Event::Empty(e)if e.local_name().as_ref()=="rect"=>{rects+=1;maximum_rects=maximum_rects.max(rects);},quick_xml::events::Event::Eof=>break,_=>()}}
        assert!(maximum_rects>1);assert_eq!(doc.source,source);let(_,settings)=omml::docx_math_fragment(&doc.root.xml(),true).unwrap();assert!(settings.unwrap().xml().contains("val=\"1440\""));
    }

    #[test]
    fn math_large_single_run_wraps_without_losing_any_of_twenty_thousand_glyphs(){
        let text=std::iter::repeat_n("x",10000).collect::<Vec<_>>().join("+");let xml=format!("<m:mathPr><m:wrapIndent m:val=\"0\"/></m:mathPr><m:oMath><m:r><m:t>{text}</m:t></m:r></m:oMath>");
        let f=layout_source_constrained("",Some(&xml),true,12.,Some(120.)).unwrap();assert_eq!(f.items.len(),19999);assert!(f.width<=120.+1e-8);assert!(f.regions.iter().filter(|r|r.kind=="auto-wrap-line").count()>500);
    }

    #[test]fn math_margins_use_physical_points_and_the_real_column_width(){
        let equation="<m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>";
        let source=|props:&str|format!("<m:mathPr>{props}</m:mathPr>{equation}");
        let glyph_x=|f:&Formula|f.items.iter().find_map(|i|if let Item::Glyph{x,..}=i{Some(x*f.size)}else{None}).unwrap();
        for size in [12.,24.]{
            let render=|props:&str|layout_source_constrained("",Some(&source(props)),true,size,Some(300.)).unwrap();
            let left=render(r#"<m:lMargin m:val="240"/><m:rMargin m:val="480"/><m:defJc m:val="left"/>"#);
            assert_eq!(left.width,300.);assert!((glyph_x(&left)-12.).abs()<1e-8);
            let right=render(r#"<m:lMargin m:val="240"/><m:rMargin m:val="480"/><m:defJc m:val="right"/>"#);
            let plain=layout_source("",Some(equation),true,size).unwrap();
            assert!((glyph_x(&right)-(300.-24.-plain.width)).abs()<1e-8);
            let off=render(r#"<m:dispDef m:val="0"/><m:lMargin/><m:rMargin/>"#);assert_eq!(off.items,plain.items);assert_eq!(off.width,plain.width);
            let absent=render("");assert_eq!(absent.width,plain.width);
            let val_less=render(r#"<m:lMargin/><m:rMargin/><m:defJc m:val="left"/>"#);assert!((glyph_x(&val_less)-72.).abs()<1e-8);
            let oversized=render(r#"<m:lMargin m:val="6000"/><m:rMargin m:val="6020"/><m:defJc m:val="left"/>"#);assert!(glyph_x(&oversized).abs()<1e-8);
        }
        for width in [f64::NAN,0.,-1.,20001.]{assert!(layout_source_constrained("x",None,true,12.,Some(width)).is_none());}
        let inline=layout_source_constrained("",Some(&source("<m:lMargin/><m:rMargin/>")),false,12.,Some(300.)).unwrap();let plain=layout_source("x",None,false,12.).unwrap();assert_eq!(inline.items,plain.items);
    }

    #[test]
    fn math_paragraphs_stack_real_equations_and_preserve_point_spacing_at_each_font_size() {
        let equation=|s:&str|format!("<m:oMath><m:r><m:t>{s}</m:t></m:r></m:oMath>");
        let xml=|props:&str,align:&str|format!(r#"<m:mathPr>{props}<m:defJc m:val="left"/></m:mathPr><m:oMathPara><m:oMathParaPr><m:jc m:val="{align}"/></m:oMathParaPr>{}{}</m:oMathPara>"#,equation("abcdef"),equation("x"));
        let glyphs=|f:&Formula|f.items.iter().filter_map(|i|if let Item::Glyph{x,y,..}=i{Some((*x*f.size,*y*f.size))}else{None}).collect::<Vec<_>>();
        for size in [12.,24.]{
            let render=|props,align|layout_source("",Some(&xml(props,align)),true,size).unwrap();
            let (group,center,right)=(render("","centerGroup"),render("","center"),render("","right"));
            assert_eq!(glyphs(&group)[0].0,glyphs(&group)[6].0);
            assert!(glyphs(&center)[6].0>glyphs(&group)[6].0);assert!(glyphs(&right)[6].0>glyphs(&center)[6].0);
            assert_eq!(right.alignment,Some(omml::MathJustification::Right));assert!(right.interactive_svg().contains("data-math-justification=\"right\""));
            let spaced=render(r#"<m:preSp m:val="120"/><m:postSp m:val="240"/><m:interSp m:val="360"/>"#,"centerGroup");
            assert!((spaced.height-group.height-6.).abs()<1e-8);
            assert!((spaced.depth-group.depth-12.-18.).abs()<1e-8);
            assert!((glyphs(&group)[6].1-glyphs(&spaced)[6].1-18.).abs()<1e-8);
            let disabled=render(r#"<m:dispDef m:val="0"/><m:preSp m:val="120"/><m:postSp m:val="240"/><m:interSp m:val="360"/>"#,"centerGroup");assert_eq!(disabled.items,group.items);
        }
        let paragraphs=format!("<m:oMathPara>{}</m:oMathPara><m:oMathPara>{}</m:oMathPara>",equation("a"),equation("b"));
        let normal=layout_source("",Some(&paragraphs),true,12.).unwrap();
        let spaced=layout_source("",Some(&format!(r#"<m:mathPr><m:intraSp m:val="240"/></m:mathPr>{paragraphs}"#)),true,12.).unwrap();
        assert!((spaced.depth-normal.depth-12.).abs()<1e-8);
    }

    #[test]
    fn math_manual_break_alignment_uses_the_previous_lines_actual_operator_positions() {
        let r=|text:&str|format!("<m:r><m:t>{text}</m:t></m:r>");
        let prefix=["a","+","b","=","c"].map(r).join("");
        for index in [0,1,2]{
            let property=if index==0{"<m:brk/>".into()}else{format!(r#"<m:brk m:alnAt="{index}"/>"#)};
            let xml=format!("<m:oMath>{prefix}<m:r><m:rPr>{property}</m:rPr><m:t>=</m:t></m:r>{}</m:oMath>",r("d"));
            let f=layout_source("",Some(&xml),true,12.).unwrap();
            let positions=f.items.iter().filter_map(|i|if let Item::Glyph{x,y,..}=i{Some((*x,*y))}else{None}).collect::<Vec<_>>();
            assert_eq!(positions.len(),7);assert!(positions[5].1<positions[0].1);
            assert!((positions[5].0-positions[match index{1=>1,2=>3,_=>0}].0).abs()<1e-8);
            let flat=layout_source("",Some(&format!("<m:oMath>{prefix}{}{}</m:oMath>",r("="),r("d"))),true,12.).unwrap();assert!(f.height+f.depth>flat.height+flat.depth);assert!(f.width<flat.width);
        }
    }

    fn fraction_font_oracle() -> serde_json::Value {
        use sha2::Digest;
        let oracle: serde_json::Value = serde_json::from_str(include_str!("../tests/fixtures/math-font-geometry-stix.json")).unwrap();
        let f = fonts(false).unwrap();
        assert_eq!(oracle["fontSha256"].as_str().unwrap(), format!("{:x}", sha2::Sha256::digest(&f.fonts[0].data[..])));
        assert_eq!(oracle["unitsPerEm"].as_f64().unwrap(), f.fonts[0].upem);
        oracle
    }

    fn fraction_sample(kind: &str, num: &str, den: &str, control: &str, defaults: &str) -> String {
        format!("<m:mathPr>{defaults}</m:mathPr><m:oMath><m:f readmdId=\"rd-formula-token-omml-f\"><m:fPr><m:type m:val=\"{kind}\"/>{control}</m:fPr><m:num readmdId=\"rd-formula-token-omml-f-num\">{num}</m:num><m:den readmdId=\"rd-formula-token-omml-f-den\">{den}</m:den></m:f></m:oMath>")
    }

    #[test]
    fn math_fraction_constants_match_independent_fonttools_oracle() {
        let oracle = fraction_font_oracle();
        let upem = oracle["unitsPerEm"].as_f64().unwrap();
        let c = fonts(false).unwrap().c;
        for (name, value) in [
            ("StackTopShiftUp", c.stack_up), ("StackTopDisplayStyleShiftUp", c.stack_up_d),
            ("StackBottomShiftDown", c.stack_down), ("StackBottomDisplayStyleShiftDown", c.stack_down_d),
            ("StackGapMin", c.stack_gap), ("StackDisplayStyleGapMin", c.stack_gap_d),
            ("SkewedFractionHorizontalGap", c.skew_hgap), ("SkewedFractionVerticalGap", c.skew_vgap),
        ] { assert!((value - oracle["constants"][name].as_f64().unwrap() / upem).abs() < 1e-12, "{name}"); }
        let fallback = Constants::defaults();
        assert!(fallback.stack_gap >= 3.0 * fallback.rule - 1e-12);
        assert!(fallback.stack_gap_d >= 7.0 * fallback.rule - 1e-12);
        assert!(fallback.skew_hgap > 0.0 && fallback.skew_vgap > 0.0);
    }

    #[test]
    fn math_no_bar_fraction_uses_stack_shifts_and_ink_gap_at_each_size() {
        let oracle = fraction_font_oracle();
        let upem = oracle["unitsPerEm"].as_f64().unwrap();
        let value = |key: &str| oracle["constants"][key].as_f64().unwrap() / upem;
        let metric = |character: &str, edge: usize| {
            let name = oracle["characters"][character].as_str().unwrap();
            oracle["glyphs"][name]["inkBounds"][edge].as_f64().unwrap() / upem
        };
        for display in [false, true] { for size in [8.0, 12.0, 24.0] { for small in [false, true] { for tall in [false, true] { for explicit_control in [false, true] {
            let run = |text| if tall { format!("<m:r><w:rPr><w:sz w:val=\"96\"/></w:rPr><m:t>{text}</m:t></m:r>") }
                else { format!("<m:r><m:t>{text}</m:t></m:r>") };
            // An absent ctrlPr uses the first run's Word format. An explicit
            // control below keeps ambient-sized Stack constants even when
            // its independently formatted operands are 48pt tall.
            let control = if explicit_control { format!("<m:ctrlPr><w:rPr><w:sz w:val=\"{}\"/></w:rPr></m:ctrlPr>", size * 2.0) } else { String::new() };
            let expected_control = if explicit_control { Some(size) } else if tall { Some(48.0) } else { None };
            let control_points = expected_control.unwrap_or(size);
            let control_scale = control_points / size;
            let xml = fraction_sample("noBar", &run("a"), &run("b"), &control, if small { "<m:smallFrac/>" } else { "" });
            let tree = omml::parse(&xml).unwrap();
            let fraction = tree.child("oMath").unwrap().child("f").unwrap();
            let actual_control = omml::control_style(fraction).half_points.map(|v| f64::from(v) / 2.0);
            let case = format!("display={display}, base={size}pt, small={small}, tall={tall}, explicit_control={explicit_control}, expected_control={expected_control:?}, actual_control={actual_control:?}");
            assert_eq!(actual_control, expected_control, "{case}");
            let f = layout_source("", Some(&xml), display, size).unwrap();
            let glyphs = f.items.iter().filter_map(|item| if let Item::Glyph { y, scale, .. } = item { Some((*y, *scale)) } else { None }).collect::<Vec<_>>();
            let case = format!("{case}, glyphs(y,scale)={glyphs:?}");
            assert_eq!(glyphs.len(), 2, "{case}"); assert!(!f.items.iter().any(|item| matches!(item, Item::Rule { .. } | Item::Line { .. })), "{case}");
            let scale = (if !display || small { value("ScriptPercentScaleDown") * upem / 100.0 } else { 1.0 }) * if tall { 48.0 / size } else { 1.0 };
            assert!((glyphs[0].1 - scale).abs() < 1e-12 && (glyphs[1].1 - scale).abs() < 1e-12, "{case}: expected operand scale={scale}");
            let (top, bottom, gap) = if display { (value("StackTopDisplayStyleShiftUp"), value("StackBottomDisplayStyleShiftDown"), value("StackDisplayStyleGapMin")) }
                else { (value("StackTopShiftUp"), value("StackBottomShiftDown"), value("StackGapMin")) };
            let (top, bottom, gap) = (top * control_scale, bottom * control_scale, gap * control_scale);
            let actual_gap = glyphs[0].0 + metric("𝑎", 1) * scale - glyphs[1].0 - metric("𝑏", 3) * scale;
            let case = format!("{case}, stack(top,down,gap)=({top},{bottom},{gap}), actual_gap={actual_gap}");
            assert!(glyphs[0].0 >= top - 1e-10 && -glyphs[1].0 >= bottom - 1e-10, "{case}");
            assert!(actual_gap >= gap - 1e-10, "{case}");
            let extra_top = glyphs[0].0 - top; let extra_bottom = -glyphs[1].0 - bottom;
            assert!((extra_top - extra_bottom).abs() < 1e-10, "{case}: shared corrections differ ({extra_top}, {extra_bottom})");
            if extra_top > 1e-10 { assert!((actual_gap - gap).abs() < 1e-10, "{case}: corrected ink gap must equal the font minimum"); }
        } } } } }
    }

    #[test]
    fn math_skewed_fraction_uses_font_gaps_and_axis_centered_slash() {
        let oracle = fraction_font_oracle();
        let upem = oracle["unitsPerEm"].as_f64().unwrap();
        let value = |key: &str| oracle["constants"][key].as_f64().unwrap() / upem;
        let glyph = |character: &str| &oracle["glyphs"][oracle["characters"][character].as_str().unwrap()];
        let run = |text| format!("<m:r><m:t>{text}</m:t></m:r>");
        for display in [false, true] { for size in [8.0, 12.0, 24.0] { for small in [false, true] { for control in [None, Some(24.0)] {
            let properties = control.map(|points| format!("<m:ctrlPr><w:rPr><w:sz w:val=\"{}\"/></w:rPr></m:ctrlPr>", points * 2.0)).unwrap_or_default();
            let xml = fraction_sample("skw", &run("a"), &run("b"), &properties, if small { "<m:smallFrac/>" } else { "" });
            let f = layout_source("", Some(&xml), display, size).unwrap();
            let operands = f.items.iter().filter_map(|item| if let Item::Glyph { gid, x, y, scale, .. } = item {
                if u64::from(*gid) == glyph("𝑎")["gid"].as_u64().unwrap() || u64::from(*gid) == glyph("𝑏")["gid"].as_u64().unwrap() { Some((*x, *y, *scale)) } else { None }
            } else { None }).collect::<Vec<_>>();
            assert_eq!(operands.len(), 2);
            let s = control.unwrap_or(size) / size;
            let depth = -glyph("𝑎")["inkBounds"][1].as_f64().unwrap() / upem * operands[0].2;
            let height = glyph("𝑏")["inkBounds"][3].as_f64().unwrap() / upem * operands[1].2;
            assert!((operands[0].1 - depth - (value("AxisHeight") + value("SkewedFractionVerticalGap") / 2.0) * s).abs() < 1e-10);
            assert!((operands[1].1 + height - (value("AxisHeight") - value("SkewedFractionVerticalGap") / 2.0) * s).abs() < 1e-10);
            let advance = glyph("𝑎")["advance"].as_f64().unwrap() / upem * operands[0].2;
            assert!((operands[1].0 - operands[0].0 - advance - value("SkewedFractionHorizontalGap") * s).abs() < 1e-10);
            let region = f.regions.iter().find(|r| r.part && r.kind == "fraction-slash").unwrap();
            let mut slash = f.clone(); slash.items = f.items[region.from..region.to].to_vec();
            let (_, lower, _, upper) = slash.ink_bounds().unwrap();
            assert!(((upper + lower) / 2.0 - value("AxisHeight") * s).abs() < 1e-10);
            let top = operands[0].1 + glyph("𝑎")["inkBounds"][3].as_f64().unwrap() / upem * operands[0].2;
            let bottom = operands[1].1 + glyph("𝑏")["inkBounds"][1].as_f64().unwrap() / upem * operands[1].2;
            assert!(upper >= top - 1e-10 && lower <= bottom + 1e-10);
        } } } }
    }

    #[test]
    fn math_fraction_nested_slashes_preserve_ink_and_source_argument_regions() {
        let run = |text| format!("<m:r><m:t>{text}</m:t></m:r>");
        let inner = format!("<m:f><m:fPr><m:type m:val=\"skw\"/></m:fPr><m:num><m:f><m:num>{}</m:num><m:den>{}</m:den></m:f></m:num><m:den>{}</m:den></m:f>", run("a"), run("b"), run("b"));
        for kind in ["bar", "noBar", "skw", "lin"] { for display in [false, true] { for size in [8.0, 24.0] { for empty in [false, true] {
            let xml = fraction_sample(kind, &inner, if empty { "" } else { &inner }, "", "");
            let f = layout_source("", Some(&xml), display, size).unwrap();
            assert!(f.width.is_finite() && f.height.is_finite() && f.depth.is_finite());
            let (left, bottom, right, top) = f.ink_bounds().unwrap();
            assert!(left >= -1e-10 && right <= f.width / size + 1e-10);
            assert!(top <= f.height / size + 1e-10 && bottom >= -f.depth / size - 1e-10);
            for id in ["rd-formula-token-omml-f-num", "rd-formula-token-omml-f-den"] {
                let r = f.regions.iter().find(|r| r.id == id).unwrap();
                assert!(r.from <= r.to && r.to <= f.items.len());
                assert!(r.x.is_finite() && r.y.is_finite() && r.w.is_finite() && r.h.is_finite() && r.d.is_finite());
                let mut part = f.clone(); part.items = f.items[r.from..r.to].to_vec();
                if let Some((_, low, _, high)) = part.ink_bounds() { assert!(high <= r.y + r.h + 1e-10 && low >= r.y - r.d - 1e-10); }
                assert!(f.interactive_svg().contains(&format!("id=\"{id}\"")));
            }
            assert!(f.interactive_svg().contains("fraction-slash"));
            assert!(!f.draw(0.0, 0.0, "0 0 0").is_empty());
        } } } }
        // Force the no-assembly fallback beyond this font's largest slash.
        let huge = "<m:r><w:rPr><w:sz w:val=\"240\"/></w:rPr><m:t>b</m:t></m:r>";
        let f = layout_source("", Some(&fraction_sample("skw", huge, huge, "", "")), true, 12.0).unwrap();
        let slash = f.regions.iter().find(|r| r.part && r.kind == "fraction-slash").unwrap();
        assert!(f.items[slash.from..slash.to].iter().any(|i| matches!(i, Item::Glyph { scale, .. } if *scale > 1.0)));
        let oracle = fraction_font_oracle();
        let b = &oracle["glyphs"][oracle["characters"]["𝑏"].as_str().unwrap()];
        let body_height = (b["inkBounds"][3].as_f64().unwrap() - b["inkBounds"][1].as_f64().unwrap()) / oracle["unitsPerEm"].as_f64().unwrap() * 10.0;
        assert!(slash.h + slash.d >= 2.0 * body_height);
        let (left, bottom, right, top) = f.ink_bounds().unwrap();
        assert!(left >= -1e-10 && right <= f.width / f.size + 1e-10);
        assert!(top <= f.height / f.size + 1e-10 && bottom >= -f.depth / f.size - 1e-10);
    }

    #[test]
    fn math_fraction_control_and_operand_sizes_remain_independent() {
        let oracle = fraction_font_oracle();
        let script = oracle["constants"]["ScriptPercentScaleDown"].as_f64().unwrap() / 100.0;
        let run = |text| format!("<m:r><w:rPr><w:sz w:val=\"32\"/></w:rPr><m:t>{text}</m:t></m:r>");
        let control = "<m:ctrlPr><w:rPr><w:sz w:val=\"48\"/><w:color w:val=\"CC3366\"/></w:rPr></m:ctrlPr>";
        let glyphs = ["𝑎", "𝑏"].map(|text| oracle["glyphs"][oracle["characters"][text].as_str().unwrap()]["gid"].as_u64().unwrap());
        for kind in ["bar", "noBar", "skw", "lin"] { for display in [false, true] { for small in [false, true] {
            let xml = fraction_sample(kind, &run("a"), &run("b"), control, if small { "<m:smallFrac/>" } else { "" });
            let a = layout_source("", Some(&xml), display, 8.0).unwrap();
            let b = layout_source("", Some(&xml), display, 24.0).unwrap();
            for (x, y) in [(a.width, b.width), (a.height, b.height), (a.depth, b.depth)] { assert!((x - y).abs() < 1e-9, "Absolute Office sizes must not change with ambient size: {kind}"); }
            for f in [&a, &b] {
                let operands = f.items.iter().filter_map(|item| if let Item::Glyph { gid, scale, .. } = item { glyphs.contains(&u64::from(*gid)).then_some(scale * f.size) } else { None }).collect::<Vec<_>>();
                assert_eq!(operands.len(), 2);
                let expected = 16.0 * if !display || small { script } else { 1.0 };
                assert!(operands.iter().all(|points| (*points - expected).abs() < 1e-9));
                if kind == "bar" {
                    let thickness = f.items.iter().find_map(|item| if let Item::Rule { h, .. } = item { Some(h * f.size) } else { None }).unwrap();
                    assert!((thickness - oracle["constants"]["FractionRuleThickness"].as_f64().unwrap() / oracle["unitsPerEm"].as_f64().unwrap() * 24.0).abs() < 1e-9);
                }
            }
            assert_eq!(a.draw(0.0, 0.0, "0 0 0"), b.draw(0.0, 0.0, "0 0 0"));
        } } }
    }

    #[test]
    fn math_small_fraction_changes_argument_glyphs_without_scaling_the_rule() {
        for kind in ["bar", "noBar", "skw", "lin"] {
            let xml = |setting: &str| format!(r#"<m:mathPr>{setting}</m:mathPr><m:oMath><m:f><m:fPr><m:type m:val="{kind}"/></m:fPr><m:num><m:r><m:t>a</m:t></m:r></m:num><m:den><m:r><m:t>b</m:t></m:r></m:den></m:f></m:oMath>"#);
            let render = |setting: &str, display| layout_source("", Some(&xml(setting)), display, 12.0).unwrap();
            let normal = render("", true);
            let small = render("<m:smallFrac/>", true);
            let scales = |f: &Formula| f.items.iter().filter_map(|i| if let Item::Glyph { scale, .. } = i { Some(*scale) } else { None }).collect::<Vec<_>>();
            let a = scales(&normal); let b = scales(&small);
            assert_eq!(a.len(), b.len());
            assert!(b.first().unwrap() < a.first().unwrap(), "{kind}");
            assert!(b.last().unwrap() < a.last().unwrap(), "{kind}");
            if matches!(kind, "skw" | "lin") { assert_eq!(a[1], b[1]); }
            let rules = |f: &Formula| f.items.iter().filter_map(|i| if let Item::Rule { h, .. } = i { Some(*h) } else { None }).collect::<Vec<_>>();
            assert_eq!(rules(&normal), rules(&small));
            assert!(small.width < normal.width, "{kind}");
            for off in [r#"<m:smallFrac m:val="0"/>"#, r#"<m:smallFrac m:val="false"/>"#, r#"<m:smallFrac m:val="off"/>"#] {
                assert_eq!(render(off, true).items, normal.items);
            }
            for on in [r#"<m:smallFrac m:val="1"/>"#, r#"<m:smallFrac m:val="true"/>"#, r#"<m:smallFrac m:val="on"/>"#] {
                assert_eq!(render(on, true).items, small.items);
            }
            assert_eq!(render("", false).items, render("<m:smallFrac/>", false).items);
        }
    }

    #[test]
    fn math_document_defaults_match_explicit_limits_in_real_vector_output() {
        let ns = crate::math_document::MATH_NS;
        for character in ["∫", "∑"] {
            for above in [false, true] {
                let location = if above {"undOvr"} else {"subSup"};
                let property = if character == "∫" {"intLim"} else {"naryLim"};
                let formula = |local: &str| format!(r#"<m:oMath xmlns:m="{ns}"><m:nary><m:naryPr><m:chr m:val="{character}"/>{local}</m:naryPr><m:sub><m:r><m:t>0</m:t></m:r></m:sub><m:sup><m:r><m:t>1</m:t></m:r></m:sup><m:e><m:r><m:t>x</m:t></m:r></m:e></m:nary></m:oMath>"#);
                let inherited = format!(r#"<m:mathPr xmlns:m="{ns}"><m:{property} m:val="{location}"/></m:mathPr>{}"#, formula(""));
                let explicit = formula(&format!(r#"<m:limLoc m:val="{location}"/>"#));
                for display in [false, true] {
                    let a = layout_source("", Some(&inherited), display, 12.0).expect("bundled math font");
                    let b = layout_source("", Some(&explicit), display, 12.0).unwrap();
                    assert_eq!(a.items, b.items);
                    assert_eq!(a.draw(0.0, 0.0, "0 0 0"), b.draw(0.0, 0.0, "0 0 0"));
                }
            }
        }
    }

    #[test]
    fn explicit_inline_limits_and_display_defaults_change_real_vector_output() {
        let xml=|character:&str,properties:&str|format!(r#"<m:oMath><m:nary><m:naryPr><m:chr m:val="{character}"/>{properties}</m:naryPr><m:sub><m:r><m:t>0</m:t></m:r></m:sub><m:sup><m:r><m:t>1</m:t></m:r></m:sup><m:e><m:r><m:t>x</m:t></m:r></m:e></m:nary></m:oMath>"#);
        let render=|character,properties,display|layout_source("",Some(&xml(character,properties)),display,12.0).expect("bundled math font required");
        for display in [false,true] {
            let above=render("∑",r#"<m:limLoc m:val="undOvr"/>"#,display);
            let side=render("∑",r#"<m:limLoc m:val="subSup"/>"#,display);
            assert!(above.height+above.depth>side.height+side.depth);
            assert!(above.width<side.width);
            assert_ne!(above.draw(0.0,0.0,"0 0 0"),side.draw(0.0,0.0,"0 0 0"));
            let default=render("∑","",display);
            assert_eq!(default.items,if display {above.items} else {side.items});
            assert_eq!(render("∫","",display).items,render("∫",r#"<m:limLoc m:val="subSup"/>"#,display).items);
        }
    }

    #[test]
    fn row_gaps_change_painted_baselines_and_points_remain_absolute_in_both_table_types() {
        for (object,container,row_a,row_b) in [
            ("m","mPr","<m:mr><m:e><m:r><m:t>x</m:t></m:r></m:e></m:mr>","<m:mr><m:e><m:r><m:t>y</m:t></m:r></m:e></m:mr>"),
            ("eqArr","eqArrPr","<m:e><m:r><m:t>x</m:t></m:r></m:e>","<m:e><m:r><m:t>y</m:t></m:r></m:e>"),
        ] {
            let xml=|rule:u8,value:u16,base:&str|format!(r#"<m:oMath><m:{object}><m:{container}><m:baseJc m:val="{base}"/><m:rSpRule m:val="{rule}"/><m:rSp m:val="{value}"/></m:{container}>{row_a}{row_b}</m:{object}></m:oMath>"#);
            let render=|rule,value,size,base|layout_source("",Some(&xml(rule,value,base)),true,size).unwrap();
            let baselines=|f:&Formula|f.items.iter().filter_map(|i|if let Item::Glyph{y,..}=i{Some(*y*f.size)}else{None}).collect::<Vec<_>>();
            let distance=|rule,value,size|{let positions=baselines(&render(rule,value,size,"center"));positions[0]-positions[1]};
            let near=|a:f64,b:f64|assert!((a-b).abs()<1e-8,"{object}: {a} != {b}");
            for size in [12.0,24.0] {
                let zero=distance(3,0,size);
                near(distance(0,600,size)-zero,size);
                near(distance(1,600,size)-zero,1.5*size);
                near(distance(2,600,size)-zero,2.0*size);
                near(distance(3,12,size)-zero,12.0);
                near(distance(4,3,size)-zero,1.5*size);
                near(baselines(&render(3,12,size,"top"))[0],0.0);
                near(baselines(&render(3,12,size,"bot"))[1],0.0);
            }
        }
    }

    #[test]
    fn matrix_minimum_column_width_is_absolute_and_never_clips_wide_arguments() {
        let run=|text:&str|format!("<m:r><m:t>{text}</m:t></m:r>");
        let matrix=|width:u16,alignment:&str,text:&str|format!(r#"<m:m><m:mPr><m:cSp m:val="{width}"/><m:cGpRule m:val="3"/><m:cGp m:val="120"/><m:mcs><m:mc><m:mcPr><m:count m:val="2"/><m:mcJc m:val="{alignment}"/></m:mcPr></m:mc></m:mcs></m:mPr><m:mr><m:e>{}</m:e><m:e>{}</m:e></m:mr></m:m>"#,run(text),run(text));
        for size in [12.,24.] { for alignment in ["left","right","center"] {
            let render=|w,text|layout_source("",Some(&format!("<m:oMath>{}</m:oMath>",matrix(w,alignment,text))),true,size).unwrap();
            let f=render(720,"x");
            let xs=f.items.iter().filter_map(|i|if let Item::Glyph{x,..}=i{Some(x*size)}else{None}).collect::<Vec<_>>();
            assert_eq!(xs.len(),2);assert!((xs[1]-xs[0]-42.).abs()<1e-8,"{alignment}: {xs:?}");
            assert!((f.width-78.).abs()<1e-8);
            let wide=render(720,"abcdefghijklmnop");let natural=render(0,"abcdefghijklmnop");
            assert_eq!(wide.items,natural.items);assert_eq!(wide.width,natural.width);
            assert_ne!(f.draw(0.,0.,"0 0 0"),render(0,"x").draw(0.,0.,"0 0 0"));
            // A subscript matrix uses smaller glyphs, but its minimum column
            // width and gap remain physical rather than shrinking with them.
            let script=layout_source("",Some(&format!("<m:oMath><m:sSub><m:e>{}</m:e><m:sub>{}</m:sub></m:sSub></m:oMath>",run("z"),matrix(720,alignment,"x"))),true,size).unwrap();
            let xs=script.items.iter().filter_map(|i|if let Item::Glyph{x,..}=i{Some(x*size)}else{None}).collect::<Vec<_>>();
            assert!((xs[2]-xs[1]-42.).abs()<1e-8);
        }}
    }

    #[test]
    fn matrix_empty_slots_keep_source_regions_and_hide_only_placeholder_ink() {
        let render=|hide:&str,body:&str|layout_source("",Some(&format!(r#"<m:oMath><m:m><m:mPr>{hide}</m:mPr><m:mr><m:e readmdId="rd-formula-token-omml-0-1">{body}</m:e><m:e><m:r><m:t>x</m:t></m:r></m:e></m:mr></m:m></m:oMath>"#)),true,12.).unwrap();
        let shown=render("","");let hidden=render("<m:plcHide/>","");let explicit_off=render(r#"<m:plcHide m:val="0"/>"#,"");
        assert_eq!(shown.items.len(),5);assert_eq!(hidden.items.len(),1);assert_eq!(shown.items,explicit_off.items);
        assert_eq!((shown.width,shown.height,shown.depth),(hidden.width,hidden.height,hidden.depth));
        for f in [&shown,&hidden] {
            let r=f.regions.iter().find(|r|r.id=="rd-formula-token-omml-0-1").unwrap();
            assert_eq!(r.w,0.45);assert_eq!((r.h,r.d),(0.65,0.05));
            assert!(f.interactive_svg().contains("rd-formula-token-omml-0-1"));
        }
        assert_eq!(render("","<m:r><m:t> </m:t></m:r>").items.len(),1);
        assert_eq!(render("","<m:phant><m:e><m:r><m:t>y</m:t></m:r></m:e></m:phant>").items.len(),1);
        // An empty slot within a nested matrix is not itself another empty
        // outer cell; only the inner matrix draws one box.
        assert_eq!(render("","<m:m><m:mr><m:e/></m:mr></m:m>").items.len(),5);
    }

    #[test]
    fn interactive_preview_preserves_pdf_ink_and_absolute_spacing_with_source_regions(){
        let xml=r#"<m:oMath><m:d readmdId="rd-formula-token-omml-0-0"><m:dPr><m:shp m:val="match"/></m:dPr><m:e readmdId="rd-formula-token-omml-0-0-1"><m:sSup><m:e><m:r><m:t>x</m:t></m:r></m:e><m:sup><m:m><m:mr><m:e><m:r><m:t>1</m:t></m:r></m:e></m:mr><m:mr><m:e><m:r><m:t>2</m:t></m:r></m:e></m:mr><m:mr><m:e><m:r><m:t>3</m:t></m:r></m:e></m:mr></m:m></m:sup></m:sSup></m:e></m:d></m:oMath>"#;
        let marked=layout_source("",Some(xml),true,12.0).unwrap();
        let plain=layout_source("",Some(&xml.replace(" readmdId=\"rd-formula-token-omml-0-0\"", "").replace(" readmdId=\"rd-formula-token-omml-0-0-1\"", "")),true,12.0).unwrap();
        assert_eq!(marked.items,plain.items);assert_eq!(marked.draw(0.0,0.0,"0 0 0"),plain.draw(0.0,0.0,"0 0 0"));
        let svg=marked.interactive_svg();assert!(svg.contains("data-math-ink"));assert!(svg.contains("data-math-parts=\"begin-fence\""));assert!(svg.contains("id=\"rd-formula-token-omml-0-0-1\""));
        assert_eq!(svg.matches("<path ").count(),marked.svg("#262626").matches("<path ").count());
        let body=marked.regions.iter().find(|r|r.id=="rd-formula-token-omml-0-0-1").unwrap();
        assert!(body.to>body.from);assert!(marked.regions.iter().any(|r|r.kind=="begin-fence"&&r.to<=body.from));
        // Normalization is drawing-only: the real 12pt column gap remains
        // 1em at 12pt and 0.5em at 24pt, rather than being laid out at 1000pt.
        let gap=r#"<m:oMath><m:m><m:mPr><m:cGpRule m:val="3"/><m:cGp m:val="240"/></m:mPr><m:mr><m:e><m:r><m:t>x</m:t></m:r></m:e><m:e><m:r><m:t>y</m:t></m:r></m:e></m:mr></m:m></m:oMath>"#;
        let small=layout_source("",Some(gap),true,12.0).unwrap();let large=layout_source("",Some(gap),true,24.0).unwrap();
        assert!((small.width/12.0-large.width/24.0-0.5).abs()<1e-8);
        assert!(small.interactive_svg().contains("em\""));
    }

    #[test]
    fn delimiter_properties_change_real_vector_geometry_and_keep_empty_characters_empty() {
        let render=|properties:&str,body:&str| {
            let xml=format!("<m:oMath><m:d><m:dPr>{properties}</m:dPr><m:e>{body}</m:e></m:d></m:oMath>");
            layout_source("",Some(&xml),true,12.0).expect("bundled math font required")
        };
        let tall=r#"<m:sSup><m:e><m:r><m:t>x</m:t></m:r></m:e><m:sup><m:m><m:mr><m:e><m:r><m:t>1</m:t></m:r></m:e></m:mr><m:mr><m:e><m:r><m:t>2</m:t></m:r></m:e></m:mr><m:mr><m:e><m:r><m:t>3</m:t></m:r></m:e></m:mr></m:m></m:sup></m:sSup>"#;
        let centered=render("",tall);
        let matched=render(r#"<m:shp m:val="match"/>"#,tall);
        let fixed=render(r#"<m:grow m:val="0"/>"#,tall);
        assert_ne!(centered.items,matched.items);
        assert!(centered.width>fixed.width);
        assert_ne!(centered.draw(0.0,0.0,"0 0 0"),fixed.draw(0.0,0.0,"0 0 0"));
        let baseline=layout_source("x",None,true,12.0).unwrap();
        for empty in [r#"<m:begChr/><m:endChr/>"#,r#"<m:begChr m:val=""/><m:endChr m:val=""/>"#] {
            let absent=render(empty,"<m:r><m:t>x</m:t></m:r>");
            assert_eq!(absent.width,baseline.width);
            assert_eq!(absent.items,baseline.items);
        }
        let xml="<m:oMath><m:d><m:dPr><m:begChr/><m:sepChr/><m:endChr/></m:dPr><m:e><m:r><m:t>x</m:t></m:r></m:e><m:e><m:r><m:t>y</m:t></m:r></m:e></m:d></m:oMath>";
        let no_separator=layout_source("",Some(xml),true,12.0).unwrap();
        assert_eq!(no_separator.items.len(),2);
    }

    #[test]
    fn nary_growth_matches_tall_operands_without_changing_display_style_or_empty_operators() {
        let body="<m:m><m:mr><m:e><m:r><m:t>x</m:t></m:r></m:e></m:mr>".to_owned()+&"<m:mr><m:e><m:r><m:t>x</m:t></m:r></m:e></m:mr>".repeat(5)+"</m:m>";
        let render=|properties:&str| {
            let xml=format!("<m:oMath><m:nary><m:naryPr>{properties}</m:naryPr><m:sub><m:r><m:t>0</m:t></m:r></m:sub><m:sup><m:r><m:t>1</m:t></m:r></m:sup><m:e>{body}</m:e></m:nary></m:oMath>");
            layout_source("",Some(&xml),true,12.0).expect("bundled math font required")
        };
        let omitted=render("");let off=render(r#"<m:grow m:val="0"/>"#);let on=render("<m:grow/>");
        assert_eq!(omitted.items,off.items);
        assert!(on.height+on.depth>off.height+off.depth);
        assert_ne!(on.items,off.items);
        let empty="<m:oMath><m:nary><m:naryPr><m:chr/></m:naryPr><m:sub/><m:sup/><m:e><m:r><m:t>x</m:t></m:r></m:e></m:nary></m:oMath>";
        let no_operator=layout_source("",Some(empty),true,12.0).unwrap();
        let baseline=layout_source("x",None,true,12.0).unwrap();
        assert_eq!(no_operator.width,baseline.width);
        assert_eq!(no_operator.items,baseline.items);
    }

    #[test]
    fn matrix_gap_rules_change_actual_pdf_width_and_absolute_gaps_ignore_font_size(){
        let xml=|rule:u8,value:u16|format!(r#"<m:oMath><m:m><m:mPr><m:cGpRule m:val="{rule}"/><m:cGp m:val="{value}"/></m:mPr><m:mr><m:e><m:r><m:t>x</m:t></m:r></m:e><m:e><m:r><m:t>y</m:t></m:r></m:e></m:mr></m:m></m:oMath>"#);
        let width=|rule,value,size|layout_source("",Some(&xml(rule,value)),true,size).expect("math font required for actual gap verification").width;
        let near=|a:f64,b:f64|assert!((a-b).abs()<1e-8,"{a} != {b}");
        for size in [12.0,24.0] {
            let no_gap=width(3,0,size);
            near(width(0,600,size)-no_gap,size);
            near(width(1,600,size)-no_gap,1.5*size);
            near(width(2,600,size)-no_gap,2.0*size);
            near(width(4,6,size)-no_gap,3.0*size);
            near(width(3,240,size)-no_gap,12.0);
        }
        assert!(layout_source("x",None,true,0.0).is_none());
        assert!(layout_source("x",None,true,f64::NAN).is_none());
    }

    #[test]
    fn argument_size_changes_vector_glyph_scale_relative_to_the_script_slot() {
        let xml=|body:&str|format!(r#"<m:oMath xmlns:m="{}">{body}</m:oMath>"#,crate::math_document::MATH_NS);
        let box_at=|value:i8|xml(&format!(r#"<m:box><m:e><m:argPr><m:argSz m:val="{value}"/></m:argPr><m:r><m:t>x</m:t></m:r></m:e></m:box>"#));
        let render=|source:String|layout_source("",Some(&source),true,12.0).expect("math font required for argument-size verification");
        let small=render(box_at(-2));let medium=render(box_at(-1));let normal=render(box_at(0));
        assert!(small.width<medium.width&&medium.width<normal.width);
        // A positive adjustment in an existing script restores normal size;
        // it must not be interpreted as yet another reduction.
        let sup=|value:i8|xml(&format!(r#"<m:sSup><m:e/><m:sup><m:argPr><m:argSz m:val="{value}"/></m:argPr><m:r><m:t>x</m:t></m:r></m:sup></m:sSup>"#));
        let scale=|f:&Formula|f.items.iter().find_map(|i|if let Item::Glyph{scale,..}=i{Some(*scale)}else{None}).unwrap();
        assert_eq!(scale(&render(sup(1))),scale(&normal));
        assert!(scale(&render(sup(0)))<scale(&normal));
        assert_eq!(scale(&render(sup(-2))),scale(&small));
    }

    #[test]
    fn matrix_column_and_baseline_properties_change_actual_vector_positions(){
        let make=|alignment:&str,base:&str|format!(r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:m><m:mPr><m:baseJc m:val="{base}"/><m:mcs><m:mc><m:mcPr><m:count m:val="1"/><m:mcJc m:val="{alignment}"/></m:mcPr></m:mc></m:mcs></m:mPr><m:mr><m:e><m:r><m:t>x</m:t></m:r></m:e></m:mr><m:mr><m:e><m:r><m:t>xxxx</m:t></m:r></m:e></m:mr></m:m></m:oMath>"#);
        let first=|f:&Formula|f.items.iter().find_map(|i|if let Item::Glyph{x,y,..}=i{Some((*x,*y))}else{None}).unwrap();
        let left=layout_source("",Some(&make("left","center")),true,12.0).expect("math font required for matrix verification");
        let center=layout_source("",Some(&make("center","center")),true,12.0).unwrap();
        let right=layout_source("",Some(&make("right","center")),true,12.0).unwrap();
        assert!(first(&left).0<first(&center).0&&first(&center).0<first(&right).0);
        assert_eq!(left.width,right.width);
        let top=layout_source("",Some(&make("left","top")),true,12.0).unwrap();
        let bot=layout_source("",Some(&make("left","bot")),true,12.0).unwrap();
        assert_eq!(first(&top).1,0.0);assert!(first(&bot).1>first(&center).1);
    }
    #[test]
    fn number_format_is_compact() {
        assert_eq!(num(1.0), "1");
        assert_eq!(num(-0.0001), "0");
        assert_eq!(num(12.34567), "12.346");
    }

    #[test]
    fn invisible_phantoms_never_fall_back_to_visible_source() {
        if !available(){return;}
        let xml=r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:phant><m:phantPr><m:show m:val="0"/><m:zeroWid m:val="1"/><m:zeroAsc m:val="1"/><m:zeroDesc m:val="1"/></m:phantPr><m:e><m:r><m:t>x</m:t></m:r></m:e></m:phant></m:oMath>"#;
        let invisible=layout_source("x",Some(xml),true,12.0).unwrap();assert!(invisible.items.is_empty());assert_eq!(invisible.width,0.0);
    }

    #[test]
    fn alphanumerics_map() {
        let plain = omml::RunStyle::default();
        assert_eq!(math_alnum('x', &plain, true), '𝑥');
        assert_eq!(math_alnum('h', &plain, true), 'ℎ');
        let bb = omml::RunStyle { double_struck: true, ..Default::default() };
        assert_eq!(math_alnum('R', &bb, false), 'ℝ');
        assert_eq!(math_alnum('A', &bb, false), '𝔸');
        let script=omml::RunStyle{script:true,..Default::default()};
        assert_eq!(math_alnum('g',&script,false),'ℊ');assert_eq!(math_alnum('z',&script,false),'𝓏');
        let fraktur=omml::RunStyle{fraktur:true,..Default::default()};
        assert_eq!(math_alnum('C',&fraktur,false),'ℭ');assert_eq!(math_alnum('a',&fraktur,false),'𝔞');
        let sans=omml::RunStyle{sans_serif:true,bold:true,..Default::default()};
        assert_eq!(math_alnum('A',&sans,true),'𝘼');assert_eq!(math_alnum('7',&sans,false),'𝟳');
        let mono=omml::RunStyle{monospace:true,..Default::default()};
        assert_eq!(math_alnum('a',&mono,false),'𝚊');assert_eq!(math_alnum('7',&mono,false),'𝟽');
        let bold=omml::RunStyle{bold:true,..Default::default()};
        assert_eq!(math_alnum('α',&bold,false),'𝛂');assert_eq!(math_alnum('ϕ',&bold,true),'𝝓');
        assert_eq!(math_alnum('∇',&bold,false),'𝛁');assert_eq!(math_alnum('∂',&bold,true),'𝝏');
    }

    #[test]
    fn formulas_lay_out_when_a_math_font_exists() {
        if !available() {
            return;
        }
        let frac = layout(r"\frac{a}{b}", true, 10.0).expect("fraction");
        assert!(frac.height > 5.0 && frac.depth > 3.0, "{frac:?}");
        let inline = layout(r"\frac{a}{b}", false, 10.0).unwrap();
        assert!(inline.height < frac.height, "display fractions are taller");
        let sup = layout("x^2", false, 10.0).unwrap();
        assert!(sup.height > layout("x", false, 10.0).unwrap().height);
        let big = layout(r"\sum_{i=1}^{n} i + \sqrt{x^2+y^2} = \int_0^1 f", true, 12.0).unwrap();
        assert!(big.width > 60.0);
        let ops = big.draw(10.0, 20.0, "0 0 0");
        assert!(ops.contains(" c\n") && ops.ends_with("Q\n"));
        assert_eq!(big.copy_text(), big.latex);
        let m = layout(r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}", true, 10.0).unwrap();
        assert!(m.height + m.depth > 20.0, "{m:?}");
        // CJK publication resources are optional. Their real installation and
        // searchable/vector output are exercised by the native pack workflow.
        assert!(layout(r"\text{中文} + x", false, 10.0).is_none());
    }

    #[test]
    fn math_fonts_oblique_ink_bounds_include_shear_in_svg_and_pdf_geometry() {
        let mut formula = layout("A", false, 12.0).unwrap();
        let before = formula.ink_bounds().unwrap();
        for item in &mut formula.items {
            if let Item::Glyph { shear, .. } = item { *shear = 0.2; }
        }
        let after = formula.ink_bounds().unwrap();
        assert_eq!(before.1, after.1);
        assert_eq!(before.3, after.3);
        assert!(after.2 > before.2);
        assert!((after.2 - before.2 - 0.2 * before.3).abs() < 1e-8);
        assert!(formula.interactive_svg().contains("matrix(1 0 0.2 1 0 0)"));
        assert_ne!(formula.draw(0.0, 0.0, "0 0 0"), layout("A", false, 12.0).unwrap().draw(0.0, 0.0, "0 0 0"));
    }

    #[test]
    fn never_panics_on_odd_input() {
        for s in ["", "{", "}", r"\frac", r"\sqrt", "^^", "_", r"\left(", r"\begin{matrix}", "x^{y^{z^{w}}}", r"\\\\"] {
            let _ = layout(s, true, 10.0);
            let _ = layout(s, false, 10.0);
        }
    }
}
