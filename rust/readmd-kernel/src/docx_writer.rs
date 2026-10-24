//! Markdown → DOCX (WordprocessingML), built on [`crate::md_ast`].
//!
//! Applies the sanitized export style (`export_styles::sanitize_options`):
//! page size / orientation / margins, body typography, H1–H6, tables (header
//! colours, borders, banding, padding, width, column alignment), code, quotes,
//! links, rules, header / footer with page numbers, optional cover page and
//! TOC field.  Emits real Word numbering for ordered / bullet / nested lists,
//! task checkboxes, hyperlinks (external and to heading bookmarks), footnotes,
//! embedded images and OMML math.

use crate::md_ast::{inline_plain, Align, Block, Document, Inline};
use crate::publication_rich::{RichBibliography, RichBibliographyItem, RichLayoutNode, RichNode};
use crate::publication_note_policies::{self as note_policy, NoteKind, NotePolicies, NotePolicyPatch, NoteSeparator, LongNote};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap, HashSet};

/// Image loader: markdown `src` → `(extension, bytes)`, or `None` when the
/// image cannot be embedded (the loader records its own warning).
pub type ImageLoader<'a> = &'a dyn Fn(&str) -> Option<(String, Vec<u8>)>;

/// The files of a `.docx` package, ready to be zipped.
pub struct DocxPackage {
    pub parts: Vec<(String, Vec<u8>)>,
    pub warns: Vec<String>,
}

const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

#[cfg(test)]
#[path="docx_math_style_isolation_tests.rs"]
mod math_style_isolation_tests;
#[cfg(test)]
#[path="docx_math_native_styles_tests.rs"]
mod math_native_styles_tests;
#[cfg(test)]
#[path="docx_math_native_runs_tests.rs"]
mod math_native_runs_tests;
const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

// ------------------------------------------------------------------ helpers

/// XML-escape text and drop characters XML 1.0 forbids (Word refuses the file).
pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 => {}
            '\u{FFFE}' | '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

fn dig<'a>(v: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut cur = v;
    for p in path {
        cur = cur.get(*p)?;
    }
    Some(cur)
}

fn num(v: &Value, path: &[&str], default: f64) -> f64 {
    match dig(v, path) {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(default),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(default),
        _ => default,
    }
}

fn text<'a>(v: &'a Value, path: &[&str], default: &'a str) -> &'a str {
    dig(v, path).and_then(|x| x.as_str()).unwrap_or(default)
}

fn flag(v: &Value, path: &[&str], default: bool) -> bool {
    match dig(v, path) {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().map(|x| x != 0.0).unwrap_or(default),
        _ => default,
    }
}

/// `#1a2b3c` → `1A2B3C` (invalid → `fallback`).
fn color(v: &Value, path: &[&str], fallback: &str) -> String {
    let raw = text(v, path, fallback).trim().trim_start_matches('#');
    let ok = |s: &str| s.len() == 6 && s.chars().all(|c| c.is_ascii_hexdigit());
    if ok(raw) {
        raw.to_ascii_uppercase()
    } else if raw.len() == 3 && raw.chars().all(|c| c.is_ascii_hexdigit()) {
        raw.chars().flat_map(|c| [c, c]).collect::<String>().to_ascii_uppercase()
    } else {
        fallback.trim_start_matches('#').to_ascii_uppercase()
    }
}

/// Export-style font key → the family name Word knows.
pub fn font_family(key: &str) -> &str {
    match key {
        "MicrosoftYaHei" => "Microsoft YaHei",
        "SimHei" => "SimHei",
        "SimSun" => "SimSun",
        "KaiTi" => "KaiTi",
        "DengXian" => "DengXian",
        "Arial" => "Arial",
        "Consolas" => "Consolas",
        "Courier New" => "Courier New",
        other => other,
    }
}

fn jc(align: &str) -> &'static str {
    match align {
        "center" => "center",
        "right" => "right",
        "justify" => "both",
        _ => "left",
    }
}

/// Points → twentieths of a point (twips).
fn twips(pt: f64) -> i64 {
    (pt * 20.0).round() as i64
}

/// A portable label column based on the reviewed embedded fallback's advances.
/// Word may use a requested family with different metrics; preserve that family.
fn bibliography_label_tab_twips(items: &[crate::publication_rich::RichBibliographyItem], size: f64) -> i64 {
    let face = ttf_parser::Face::parse(crate::publication_fonts::CORE_TEXT_FALLBACK, 0).ok();
    let max_em = items.iter().flat_map(|item| {
        crate::publication_rich::plain(&item.first_field).split('\n').map(|line| {
            line.chars().map(|character| {
                face.as_ref().and_then(|font| font.glyph_index(character).filter(|glyph| glyph.0 != 0)
                    .and_then(|glyph| font.glyph_hor_advance(glyph))
                    .map(|advance| f64::from(advance) / f64::from(font.units_per_em())))
                    .unwrap_or(1.)
            }).sum::<f64>()
        }).collect::<Vec<_>>()
    }).fold(0., f64::max);
    ((max_em + 0.75) * size * 20.).ceil().max(360.) as i64
}

/// CSL supplies no physical column width. All labels contribute to a shared
/// preferred width; a nested container can use at most 35% for its label (the
/// 0.75em gutter is included). Text wraps; neither the text nor its font shrinks.
/// The reviewed fallback supplies portable advances, not a claim about Word's
/// installed version of the requested Office family.
fn bibliography_preferred_label_twips(layouts: &[Vec<RichLayoutNode>], size: f64) -> i64 {
    fn minimum_flow(nodes: &[RichLayoutNode], em: i64, gutter: i64) -> i64 {
        nodes.iter().map(|node| match node {
            RichLayoutNode::Inline { children } => if bibliography_visible_text(children) { em } else { 0 },
            RichLayoutNode::Group { children, .. } | RichLayoutNode::Link { children, .. } => minimum_flow(children, em, gutter),
            RichLayoutNode::Block { children, indent } => minimum_flow(children, em, gutter) + if *indent { em * 4 } else { 0 },
            RichLayoutNode::Columns { left, right } => {
                let left = minimum_flow(left, em, gutter).max(em) + gutter;
                ((left * 100 + 34) / 35).max(left + minimum_flow(right, em, gutter).max(em))
            }
        }).max().unwrap_or(0)
    }
    struct Measure { line: f64, maximum: f64 }
    impl Measure {
        fn end_line(&mut self) { self.maximum = self.maximum.max(self.line); self.line = 0.; }
        fn rich(&mut self, nodes: &[RichNode], face: Option<&ttf_parser::Face<'_>>) {
            for node in nodes {
                match node {
                    RichNode::Text { text, .. } => for character in text.chars() {
                        if character == '\n' { self.end_line(); } else {
                            self.line += face.and_then(|font| font.glyph_index(character).filter(|glyph| glyph.0 != 0)
                                .and_then(|glyph| font.glyph_hor_advance(glyph))
                                .map(|advance| f64::from(advance) / f64::from(font.units_per_em()))).unwrap_or(1.);
                        }
                    },
                    RichNode::Group { children, .. } | RichNode::Link { children, .. } => self.rich(children, face),
                }
            }
        }
        fn flow(&mut self, nodes: &[RichLayoutNode], face: Option<&ttf_parser::Face<'_>>) {
            for node in nodes {
                match node {
                    RichLayoutNode::Inline { children } => self.rich(children, face),
                    RichLayoutNode::Group { children, .. } | RichLayoutNode::Link { children, .. } => self.flow(children, face),
                    RichLayoutNode::Block { children, .. } => { self.end_line(); self.flow(children, face); self.end_line(); }
                    RichLayoutNode::Columns { left, right } => { self.end_line(); self.flow(left, face); self.flow(right, face); self.end_line(); }
                }
            }
        }
    }
    fn collect(nodes: &[RichLayoutNode], maximum: &mut f64, minimum: &mut i64, em: i64, gutter: i64, face: Option<&ttf_parser::Face<'_>>) {
        for node in nodes {
            match node {
                RichLayoutNode::Columns { left, right } => {
                    let mut measure = Measure { line: 0., maximum: 0. }; measure.flow(left, face); measure.end_line();
                    *maximum = (*maximum).max(measure.maximum);
                    // A label can itself contain columns/indent blocks. Its
                    // minimum structural width participates in the preference,
                    // so a short nested label is not rejected merely because
                    // measuring its two strings alone underestimated its grid.
                    *minimum = (*minimum).max(minimum_flow(left, em, gutter).max(em) + gutter);
                    collect(left, maximum, minimum, em, gutter, face); collect(right, maximum, minimum, em, gutter, face);
                }
                RichLayoutNode::Block { children, .. } | RichLayoutNode::Group { children, .. }
                    | RichLayoutNode::Link { children, .. } => collect(children, maximum, minimum, em, gutter, face),
                RichLayoutNode::Inline { .. } => (),
            }
        }
    }
    let face = ttf_parser::Face::parse(crate::publication_fonts::CORE_TEXT_FALLBACK, 0).ok();
    let em = twips(size); let gutter = twips(size * 0.75); let mut minimum = 0; let mut maximum = 0_f64;
    for layout in layouts { collect(layout, &mut maximum, &mut minimum, em, gutter, face.as_ref()); }
    (((maximum + 0.75) * size * 20.).ceil().max(360.) as i64).max(minimum)
}

fn bibliography_visible_text(nodes: &[RichNode]) -> bool {
    nodes.iter().any(|node| match node {
        RichNode::Text { text, .. } => text.chars().any(|c| !c.is_whitespace()),
        RichNode::Group { children, .. } | RichNode::Link { children, .. } => bibliography_visible_text(children),
    })
}

fn bibliography_inline_only(nodes: &[RichNode]) -> bool {
    nodes.iter().all(|node| match node {
        RichNode::Text { .. } => true,
        RichNode::Link { children, .. } => bibliography_inline_only(children),
        RichNode::Group { children, display, .. } => display.is_none() && bibliography_inline_only(children),
    })
}

fn bibliography_simple_item(item: &RichBibliographyItem, layout: &[RichLayoutNode]) -> bool {
    fn inline_flow(nodes: &[RichLayoutNode]) -> bool {
        nodes.iter().all(|node| match node {
            RichLayoutNode::Inline { .. } => true,
            RichLayoutNode::Group { children, .. } | RichLayoutNode::Link { children, .. } => inline_flow(children),
            RichLayoutNode::Block { .. } | RichLayoutNode::Columns { .. } => false,
        })
    }
    if bibliography_inline_only(&item.first_field) && bibliography_inline_only(&item.rich) { return true; }
    // Existing processor first_field values can retain the consumed outer
    // left-margin/right-inline markers. Their projected cells are still inline;
    // keep their original native tab route instead of unnecessarily adding a
    // layout table. A real block, second row or nested pair cannot enter here.
    !item.first_field.is_empty() && matches!(layout, [RichLayoutNode::Columns { left, right }] if inline_flow(left) && inline_flow(right))
}

fn bibliography_has_light(nodes: &[RichNode]) -> bool {
    nodes.iter().any(|node| match node {
        RichNode::Text { format, .. } => format.font_weight == crate::publication_rich::RichFontWeight::Light,
        RichNode::Link { children, .. } | RichNode::Group { children, .. } => bibliography_has_light(children),
    })
}

/// Renderable blocks rather than XML string surgery: Group and Link are
/// transparent to the current paragraph, while actual Block/Columns flush it.
enum BibliographyBlock {
    Paragraph { runs: String, indent: i64 },
    Columns { left: Vec<Self>, right: Vec<Self>, indent: i64 },
}

#[derive(Clone, Default)]
struct BibliographyOwners<'a> {
    link: Option<&'a str>,
    entry_indices: Vec<usize>,
}

struct BibliographyMetrics {
    preferred_label: i64,
    gutter: i64,
    minimum_text: i64,
    indent_step: i64,
    line: u64,
    hanging: bool,
}

struct BibliographyFailure {
    code: String,
    path: String,
}

impl BibliographyFailure {
    fn geometry(path: &str) -> Self { Self { code: "publication_layout_geometry".into(), path: path.into() } }
}

/// Millimetres → twips.
fn mm_twips(mm: f64) -> i64 {
    (mm * 56.692_913).round() as i64
}

/// Heading bookmark name: Word allows ≤ 40 chars, letters/digits/underscore.
pub fn bookmark_name(slug: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut out = String::from("_h");
    for c in slug.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if c == '-' || c == '_' {
            out.push('_');
        } else {
            out.push_str(&format!("{:x}", c as u32));
        }
    }
    // Encoding punctuation or Unicode into a Word name is not injective.
    // Include a stable suffix even for short ids (a-b / a_b, α / 3b1).
    out.truncate(23);
    out.push('_');
    out.push_str(&format!("{:x}", Sha256::digest(slug.as_bytes()))[..16]);
    out
}

/// Pixel size of PNG / JPEG / GIF / BMP bytes.
pub fn image_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() >= 24 && b.starts_with(b"\x89PNG\r\n\x1a\n") {
        let w = u32::from_be_bytes([b[16], b[17], b[18], b[19]]);
        let h = u32::from_be_bytes([b[20], b[21], b[22], b[23]]);
        return Some((w, h));
    }
    if b.len() >= 10 && (b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a")) {
        return Some((u16::from_le_bytes([b[6], b[7]]) as u32, u16::from_le_bytes([b[8], b[9]]) as u32));
    }
    if b.len() >= 26 && b.starts_with(b"BM") {
        let w = i32::from_le_bytes([b[18], b[19], b[20], b[21]]).unsigned_abs();
        let h = i32::from_le_bytes([b[22], b[23], b[24], b[25]]).unsigned_abs();
        return Some((w, h));
    }
    if b.len() >= 4 && b[0] == 0xFF && b[1] == 0xD8 {
        let mut i = 2usize;
        while i + 9 < b.len() {
            if b[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = b[i + 1];
            if marker == 0xD8 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) || marker == 0xFF {
                i += if marker == 0xFF { 1 } else { 2 };
                continue;
            }
            let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
            let is_sof = matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF);
            if is_sof {
                let h = u16::from_be_bytes([b[i + 5], b[i + 6]]) as u32;
                let w = u16::from_be_bytes([b[i + 7], b[i + 8]]) as u32;
                return Some((w, h));
            }
            if len < 2 {
                return None;
            }
            i += 2 + len;
        }
    }
    None
}

fn is_cjk(c: char) -> bool {
    crate::latex_writer::is_cjk(c)
}

// ------------------------------------------------------------------ writer

#[derive(Clone,Copy)]
pub(crate) enum AuthorTextContext{Body,Heading(u8),Caption,Table,Code}
/// Match the native styles below, including Caption's independent CS
/// inheritance. This is also the import guard's expected semantic field base.
pub(crate) fn author_text_base(style:&Value,context:AuthorTextContext)->(u32,u32){
    let hp=|size:f64|(size*2.).round().max(1.).min(i32::MAX as f64)as u32;
    let body=hp(num(style,&["typography","size"],11.));
    match context{
        AuthorTextContext::Caption=>(18,body),
        AuthorTextContext::Heading(level)=>{let level=level.clamp(1,6);let key=format!("h{level}");let size=hp(num(style,&["headings",&key,"size"],[20.,16.,14.,12.,11.,10.5][usize::from(level)-1]));(size,size)},
        AuthorTextContext::Table=>{let size=hp(num(style,&["table","cellSize"],10.));(size,size)},
        AuthorTextContext::Code=>{let size=hp(num(style,&["code","size"],9.5));(size,size)},
        AuthorTextContext::Body=>(body,body),
    }
}

#[derive(Clone, Default)]
struct Fmt {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    link: bool,
    color: Option<String>,
    size_hp: Option<i64>,
    author_size_hp: Option<u32>,
    author_size_cs_hp: Option<u32>,
    block_base_hp: Option<(u32,u32)>,
    author_token: Option<String>,
    cs: Option<bool>,
    rtl: Option<bool>,
    tracking_twips: Option<i32>,
    baseline_half_points: Option<i32>,
    rich: Option<crate::publication_rich::RichFormat>,
}

struct Rel {
    id: String,
    kind: &'static str,
    target: String,
    external: bool,
}

struct Ctx<'a> {
    style: &'a Value,
    author_token: String,
    author_block_base: Option<(u32,u32)>,
    author_local_base: Option<(u32,u32)>,
    line_context: crate::publication_typography::LineContext,
    body_tracking_twips: Option<i32>,
    body_baseline_half_points: Option<i32>,
    local_baseline_half_points: Option<i32>,
    tracking_front_blocks_remaining: usize,
    images: ImageLoader<'a>,
    body: String,
    rels: Vec<Rel>,
    media: Vec<(String, Vec<u8>)>,
    image_cache: HashMap<String, Option<(String, u32, u32, Option<String>, Option<f64>)>>,
    bullet_num: u32,
    heading_num: Option<u32>,
    nums: Vec<(u32, u32, u64)>, // (numId, abstractId, start)
    next_num: u32,
    bookmark_id: u32,
    labels: HashMap<String, String>,
    publication_section_ids: HashSet<String>,
    drawing_id: u32,
    footnote_defs: HashMap<String, Vec<Block>>,
    footnotes: Vec<(NoteKind,u32, String)>, // story identity, id, paragraph XML
    footnote_ids: HashMap<String, u32>,
    footnote_rels: HashMap<NoteKind,HashSet<String>>,
    note_policies: NotePolicies,
    note_counts: [u32;2],
    fn_stack: Vec<String>,
    warns: Vec<String>,
    math_defaults: crate::math_defaults::DocxDefaultsMerge,
    math_paragraph_registry: crate::math_word::NativeParagraphRegistry,
    has_math_paragraph_style:bool,
    math_failure: Option<String>,
    text_w_emu: i64,
    text_h_emu: i64,
    bibliography_control_id: u32,
    bibliography_failure: Option<BibliographyFailure>,
    citation_bindings: crate::publication_docx_citations::Bindings,
    citation_depth: usize,
}

impl<'a> Ctx<'a> {
    fn generated_citation_source(&self, label: &str) -> Option<crate::publication_rich::SourceCitation> {
        if !label.starts_with("readmd-citation-") { return None; }
        let definitions = self.footnote_defs.get(label)?;
        let [Block::Paragraph(values)] = definitions.as_slice() else { return None; };
        let [Inline::Citation { source, .. }] = values.as_slice() else { return None; };
        (!source.context.in_note).then(||source.clone())
    }
    fn citation_control(&mut self, source: &crate::publication_rich::SourceCitation, rendered: &str, generated_label: Option<&str>) -> String {
        let generated = generated_label.is_some() || self.fn_stack.last().is_some_and(|label|self.generated_citation_source(label).is_some());
        let label = generated_label.or_else(||self.fn_stack.last().map(String::as_str));
        let end=label.map(|label|self.note_policies.kind_for_label(label).is_end()).unwrap_or(crate::publication_layout::notes_at_end(self.style));
        let part=match self.active_note_kind(){Some(NoteKind::Endnote)=>"word/endnotes.xml",Some(NoteKind::Footnote)=>"word/footnotes.xml",None=>"word/document.xml"};
        let note = label.and_then(|label|self.footnote_ids.get(label)).map(|id|(end,*id as i32));
        match self.citation_bindings.citation(source, rendered, part, generated, note) {
            Ok(xml) => xml,
            Err(code) => { self.bibliography_failure = Some(BibliographyFailure {code,path:"citations".into()}); rendered.into() },
        }
    }
    fn active_note_kind(&self)->Option<NoteKind>{self.fn_stack.last().map(|label|self.note_policies.kind_for_label(label))}
    fn apply_local_layout(&self,fragment:&str,attributes:&Value,block:&Block,inherit_body:bool)->Result<String,&'static str>{
        let mut resolved=crate::publication_blocks::Resolved::for_context_with_lines(self.style,attributes,block,!self.fn_stack.is_empty(),self.line_context);
        if let Some(kind)=self.active_note_kind(){
            let policy=self.note_policies.paragraph_policy(kind);
            if attributes["text"].get("size").is_none(){if let Some(size)=policy.size{resolved.size=size;}}
            if attributes["paragraph"].get("lineHeight").is_none(){if let Some(crate::publication_typography::LineSpacing::Multiple(value))=policy.line_spacing{resolved.line_height=value;}}
            if attributes["paragraph"].get("keepLines").is_none(){if let Some(value)=policy.long_note{resolved.keep_lines=value==LongNote::KeepParagraphs;}}
            crate::docx_local_style::preserve_author_axes(fragment,&self.author_token,|fragment|crate::docx_local_style::note_local(fragment,&resolved,attributes,self.style,&policy))
        }else{crate::docx_local_style::preserve_author_axes(fragment,&self.author_token,|fragment|crate::docx_local_style::apply(fragment,&resolved,attributes,self.style,inherit_body))}
    }
    fn office_math(&mut self, xml: &str, display: bool) -> String {
        if self.math_failure.is_some() { return String::new(); }
        match self.math_defaults.accept(xml, display) {
            Ok(content) => content,
            Err(error) => { self.math_failure = Some(error.message()); String::new() }
        }
    }
    fn office_math_paragraph(&mut self,xml:&str,sc:&Scope)->(String,String){
        if self.math_failure.is_some(){return (String::new(),String::new());}
        match self.math_defaults.accept_native_projected(xml,true){
            Ok((mut content,paragraph,source))=>{
                // A local host may subsequently rewrite pPr. Keep those old
                // paths separate from the source-owned body style registry.
                let native_body_host=!sc.in_quote && sc.list_depth==0 && !sc.nested_math_host && self.fn_stack.is_empty();
                if native_body_host {
                    if let Some(source)=source.as_ref() {
                        match self.math_paragraph_registry.resolve(source) {
                            Ok(crate::math_word::NativeParagraphResolution::Native(native))=>{
                                let projected=match self.math_paragraph_registry.project_runs(source,&content,Some(&native.selected_style_id)){
                                    Ok(projected)=>projected,
                                    Err(code)=>{self.math_failure=Some(code.into());return (String::new(),String::new());},
                                };
                                if projected.materialized_runs>0 {
                                    let code=crate::api_codes::WARN_MATH_RUN_STYLE_MATERIALIZED;
                                    if !self.warns.iter().any(|warning|warning==code){self.warns.push(code.into());}
                                }
                                self.has_math_paragraph_style=true;
                                return (projected.ppr_xml.unwrap_or(native.ppr_xml),projected.content);
                            },
                            Ok(crate::math_word::NativeParagraphResolution::Materialized(_))=>{
                                let code=crate::api_codes::WARN_MATH_PARAGRAPH_STYLE_MATERIALIZED;
                                if !self.warns.iter().any(|warning|warning==code) {
                                    self.warns.push(code.into());
                                }
                            },
                            Ok(crate::math_word::NativeParagraphResolution::NotApplicable)=>(),
                            Err(code)=>{self.math_failure=Some(code.into());return (String::new(),String::new());},
                        }
                    }
                }
                if let Some(source)=source.as_ref(){
                    // The template host is known only in the body lane.
                    // Quotes, lists, notes and nested hosts may rewrite pPr;
                    // they retain materialized values and a precise warning.
                    let host=native_body_host.then_some("ReadMDMath");
                    match self.math_paragraph_registry.project_runs(source,&content,host){
                        Ok(projected)=>{
                            if projected.materialized_runs>0{
                                let code=crate::api_codes::WARN_MATH_RUN_STYLE_MATERIALIZED;
                                if !self.warns.iter().any(|warning|warning==code){self.warns.push(code.into());}
                            }
                            if let Some(ppr)=projected.ppr_xml{
                                self.has_math_paragraph_style=true;
                                return (ppr,projected.content);
                            }
                            content=projected.content;
                        },
                        Err(code)=>{self.math_failure=Some(code.into());return (String::new(),String::new());},
                    }
                }
                let mut ppr=self.scope_ppr(sc);
                if !sc.in_quote&&sc.list_depth==0 {self.has_math_paragraph_style=true;ppr="<w:pStyle w:val=\"ReadMDMath\"/>".into();}
                let alignment=paragraph.as_ref().and_then(|value|value.alignment.as_deref()).unwrap_or("center").to_string();
                if let Some(value)=paragraph {
                    if value.left.is_some()||value.right.is_some(){
                        // Scope contributes only these two generated fields;
                        // replace its indent, never append a duplicate w:ind.
                        ppr=if sc.in_quote{"<w:pStyle w:val=\"Quote\"/>".into()}else if sc.list_depth==0{"<w:pStyle w:val=\"ReadMDMath\"/>".into()}else{String::new()};
                        ppr.push_str("<w:ind");
                        if let Some(left)=value.left{ppr.push_str(&format!(" w:left=\"{}\"",xml_escape(&left).replace('\t',"&#9;").replace('\r',"&#13;").replace('\n',"&#10;")));}
                        if let Some(right)=value.right{ppr.push_str(&format!(" w:right=\"{}\"",xml_escape(&right).replace('\t',"&#9;").replace('\r',"&#13;").replace('\n',"&#10;")));}
                        ppr.push_str("/>");
                    }
                }
                ppr.push_str(&format!("<w:jc w:val=\"{alignment}\"/>"));
                (ppr,content)
            },
            Err(error)=>{self.math_failure=Some(error.message());(String::new(),String::new())},
        }
    }

    fn rel(&mut self, kind: &'static str, target: String, external: bool) -> String {
        let id = if let Some(r) = self.rels.iter().find(|r| r.kind == kind && r.target == target && r.external == external) {
            r.id.clone()
        } else {
            let id = format!("rId{}", self.rels.len() + 1);
            self.rels.push(Rel { id: id.clone(), kind, target, external });
            id
        };
        if let Some(kind)=self.active_note_kind(){self.footnote_rels.entry(kind).or_default().insert(id.clone());}
        id
    }
}

// ------------------------------------------------------------------ inlines

fn run_xml(t: &str, f: &Fmt) -> String {
    if t.is_empty() {
        return String::new();
    }
    let mut rpr = String::new();
    if f.code {
        rpr.push_str("<w:rStyle w:val=\"CodeChar\"/>");
    } else if f.link {
        rpr.push_str("<w:rStyle w:val=\"Hyperlink\"/>");
    }
    if f.bold {
        rpr.push_str("<w:b/><w:bCs/>");
    }
    if f.italic {
        rpr.push_str("<w:i/><w:iCs/>");
    }
    if f.tracking_twips.is_some() || f.baseline_half_points.is_some() {
        if let Some(format) = &f.rich {
            if format.font_variant == crate::publication_rich::RichFontVariant::SmallCaps { rpr.push_str("<w:smallCaps/>"); }
        }
    }
    if f.strike {
        rpr.push_str("<w:strike/>");
    }
    if let Some(format) = f.rich.as_ref().filter(|_| f.tracking_twips.is_none() && f.baseline_half_points.is_none()) {
        use crate::publication_rich::*;
        if format.font_variant == RichFontVariant::SmallCaps { rpr.push_str("<w:smallCaps/>"); }
        if format.text_decoration == RichDecoration::Underline { rpr.push_str("<w:u w:val=\"single\"/>"); }
        match format.vertical_align { RichVerticalAlign::Baseline => {}, RichVerticalAlign::Sup => rpr.push_str("<w:vertAlign w:val=\"superscript\"/>"), RichVerticalAlign::Sub => rpr.push_str("<w:vertAlign w:val=\"subscript\"/>") }
    }
    if let Some(c) = &f.color {
        rpr.push_str(&format!("<w:color w:val=\"{c}\"/>"));
    }
    if let Some(value) = f.tracking_twips { rpr.push_str(&format!("<w:spacing w:val=\"{value}\"/>")); }
    if let Some(value) = f.baseline_half_points { rpr.push_str(&format!("<w:position w:val=\"{value}\"/>")); }
    if let Some(sz) = f.author_size_hp.map(i64::from).or(f.size_hp) { rpr.push_str(&format!("<w:sz w:val=\"{sz}\"/>")); }
    if let Some(sz) = f.author_size_cs_hp.map(i64::from).or(f.size_hp) { rpr.push_str(&format!("<w:szCs w:val=\"{sz}\"/>")); }
    if f.tracking_twips.is_some() || f.baseline_half_points.is_some() {
        if let Some(format) = &f.rich {
            use crate::publication_rich::*;
            if format.text_decoration == RichDecoration::Underline { rpr.push_str("<w:u w:val=\"single\"/>"); }
            match format.vertical_align { RichVerticalAlign::Baseline => {}, RichVerticalAlign::Sup => rpr.push_str("<w:vertAlign w:val=\"superscript\"/>"), RichVerticalAlign::Sub => rpr.push_str("<w:vertAlign w:val=\"subscript\"/>") }
        }
    }
    if let Some(value)=f.rtl{rpr.push_str(&format!("<w:rtl w:val=\"{}\"/>",u8::from(value)));}
    if let Some(value)=f.cs{rpr.push_str(&format!("<w:cs w:val=\"{}\"/>",u8::from(value)));}
    let axes=u8::from(f.author_size_hp.is_some())|u8::from(f.author_size_cs_hp.is_some())<<1|u8::from(f.cs.is_some())<<2|u8::from(f.rtl.is_some())<<3;
    let mut out = if let Some(token)=f.author_token.as_ref().filter(|_|axes!=0){format!("<w:r {}=\"{token}:{axes}\">",crate::docx_local_style::AUTHOR_AXES_ATTRIBUTE)}else{String::from("<w:r>")};
    if !rpr.is_empty() {
        out.push_str("<w:rPr>");
        out.push_str(&rpr);
        out.push_str("</w:rPr>");
    }
    let mut first = true;
    for line in t.split('\n') {
        if !first {
            out.push_str("<w:br/>");
        }
        first = false;
        let mut seg_first = true;
        for seg in line.split('\t') {
            if !seg_first {
                out.push_str("<w:tab/>");
            }
            seg_first = false;
            if !seg.is_empty() {
                out.push_str("<w:t xml:space=\"preserve\">");
                out.push_str(&xml_escape(seg));
                out.push_str("</w:t>");
            }
        }
    }
    out.push_str("</w:r>");
    out
}
fn note_callout(kind:&str,reference:&str,id:u32,f:&Fmt)->String{
    let mut rpr=format!("<w:rStyle w:val=\"{reference}\"/>");
    if let Some(value)=f.author_size_hp{rpr.push_str(&format!("<w:sz w:val=\"{value}\"/>"));}
    if let Some(value)=f.author_size_cs_hp{rpr.push_str(&format!("<w:szCs w:val=\"{value}\"/>"));}
    if let Some(value)=f.rtl{rpr.push_str(&format!("<w:rtl w:val=\"{}\"/>",u8::from(value)));}
    if let Some(value)=f.cs{rpr.push_str(&format!("<w:cs w:val=\"{}\"/>",u8::from(value)));}
    let axes=u8::from(f.author_size_hp.is_some())|u8::from(f.author_size_cs_hp.is_some())<<1|u8::from(f.cs.is_some())<<2|u8::from(f.rtl.is_some())<<3;
    let marker=f.author_token.as_ref().filter(|_|axes!=0).map(|token|format!(" {}=\"{token}:{axes}\"",crate::docx_local_style::AUTHOR_AXES_ATTRIBUTE)).unwrap_or_default();
    format!("<w:r{marker}><w:rPr>{rpr}</w:rPr><w:{kind}Reference w:id=\"{id}\"/></w:r>")
}

fn first_char(v: &[Inline]) -> Option<char> {
    for i in v {
        match i {
            Inline::Text(t) | Inline::Code(t) => return t.chars().next(),
            Inline::Strong(c) | Inline::Emph(c) | Inline::Strike(c)
                | Inline::Styled { children: c, .. } | Inline::AuthorStyled { children: c, .. } | Inline::Display { children: c, .. } | Inline::Citation { children: c, .. } => {
                if let Some(ch) = first_char(c) {
                    return Some(ch);
                }
            }
            Inline::Link { children, .. } => return first_char(children),
            _ => return None,
        }
    }
    None
}

fn last_char(v: &[Inline]) -> Option<char> {
    for i in v.iter().rev() {
        match i {
            Inline::Text(t) | Inline::Code(t) => return t.chars().last(),
            Inline::Strong(c) | Inline::Emph(c) | Inline::Strike(c)
                | Inline::Styled { children: c, .. } | Inline::AuthorStyled { children: c, .. } | Inline::Display { children: c, .. } | Inline::Citation { children: c, .. } => {
                if let Some(ch) = last_char(c) {
                    return Some(ch);
                }
            }
            Inline::Link { children, .. } => return last_char(children),
            _ => return None,
        }
    }
    None
}

impl<'a> Ctx<'a> {
    fn inlines(&mut self, v: &[Inline], f: &Fmt) -> String {
        if f.block_base_hp.is_none(){let mut inherited=f.clone();inherited.block_base_hp=Some(self.author_block_base.unwrap_or_else(||self.native_text_base(None)));return self.inlines(v,&inherited);}
        let mut out = String::new();
        for (idx, i) in v.iter().enumerate() {
            match i {
                Inline::Text(t) => out.push_str(&run_xml(t, f)),
                Inline::Code(t) => {
                    let mut g = f.clone();
                    g.code = true;
                    g.tracking_twips = None;
                    g.baseline_half_points = None;
                    out.push_str(&run_xml(t, &g));
                }
                Inline::Strong(c) => {
                    let mut g = f.clone();
                    g.bold = true;
                    out.push_str(&self.inlines(c, &g));
                }
                Inline::Emph(c) => {
                    let mut g = f.clone();
                    g.italic = true;
                    out.push_str(&self.inlines(c, &g));
                }
                Inline::Strike(c) => {
                    let mut g = f.clone();
                    g.strike = true;
                    out.push_str(&self.inlines(c, &g));
                }
                Inline::Styled { format, children } => {
                    let mut g = f.clone();
                    g.bold = format.font_weight == crate::publication_rich::RichFontWeight::Bold;
                    g.italic = format.font_style != crate::publication_rich::RichFontStyle::Normal;
                    g.rich = Some(format.clone());
                    out.push_str(&self.inlines(children, &g));
                }
                Inline::AuthorStyled {style,children}=>{
                    use crate::publication_inline_text::CascadeSize;
                    let mut g=f.clone();let base=f.block_base_hp.expect("resolved block base");
                    match style.size{CascadeSize::Inherit=>(),CascadeSize::Points(value)=>g.author_size_hp=Some(value.half_points()),CascadeSize::BlockBase=>g.author_size_hp=Some(base.0)}
                    match style.size_cs{CascadeSize::Inherit=>(),CascadeSize::Points(value)=>g.author_size_cs_hp=Some(value.half_points()),CascadeSize::BlockBase=>g.author_size_cs_hp=Some(base.1)}
                    g.cs=style.cs.or(g.cs);g.rtl=style.rtl.or(g.rtl);g.author_token=Some(self.author_token.clone());
                    out.push_str(&self.inlines(children,&g));
                }
                Inline::Display { display, children } => {
                    let block = matches!(display, crate::publication_rich::RichDisplay::Block | crate::publication_rich::RichDisplay::Indent);
                    if block && !out.is_empty() { out.push_str("<w:r><w:br/></w:r>"); }
                    out.push_str(&self.inlines(children, f));
                    if block { out.push_str("<w:r><w:br/></w:r>"); }
                }
                Inline::Citation { source, children } => {
                    let mut citation = f.clone(); citation.tracking_twips = None; citation.baseline_half_points = None;
                    self.citation_depth += 1;
                    let rendered = self.inlines(children, &citation);
                    self.citation_depth -= 1;
                    let generated = children.iter().find_map(|child|match child { Inline::FootnoteRef(label) if self.generated_citation_source(label).as_ref()==Some(source) => Some(label.as_str()), _=>None });
                    out.push_str(&self.citation_control(source,&rendered,generated));
                },
                Inline::Link { href, children, .. } => {
                    let mut g = f.clone();
                    g.link = true;
                    let inner = self.inlines(children, &g);
                    let inner = if inner.is_empty() { run_xml(href, &g) } else { inner };
                    if let Some(anchor) = href.strip_prefix('#') {
                        let slug = percent_encoding::percent_decode_str(anchor).decode_utf8_lossy();
                        if let Some(section_id) = slug.strip_prefix("readmd-section-").filter(|id| self.publication_section_ids.contains(*id)) {
                            out.push_str(&format!("<w:hyperlink w:anchor=\"{}\" w:history=\"1\">{inner}</w:hyperlink>", section_bookmark_name(section_id)));
                        } else if self.labels.get(slug.as_ref()).is_some_and(|text| text == &inline_plain(children)) {
                            out.push_str(&format!("<w:fldSimple w:instr=\" REF {} \\h \" w:dirty=\"true\">{inner}</w:fldSimple>", bookmark_name(&slug)));
                        } else { out.push_str(&format!(
                            "<w:hyperlink w:anchor=\"{}\" w:history=\"1\">{inner}</w:hyperlink>",
                            bookmark_name(&slug)
                        )); }
                    } else if href.is_empty() {
                        out.push_str(&inner);
                    } else {
                        let id = self.rel("hyperlink", xml_escape(href), true);
                        out.push_str(&format!("<w:hyperlink r:id=\"{id}\" w:history=\"1\">{inner}</w:hyperlink>"));
                    }
                }
                Inline::Image { src, alt, .. } => match self.drawing(src, alt, false) {
                    Some(x) => out.push_str(&x),
                    None => {
                        let mut g = f.clone();
                        g.italic = true;
                        out.push_str(&run_xml(&format!("[{}]", if alt.is_empty() { src } else { alt }), &g));
                    }
                },
                Inline::Math(m) => {
                    if f.author_size_hp.is_some()||f.author_size_cs_hp.is_some()||f.cs.is_some()||f.rtl.is_some(){self.bibliography_failure=Some(BibliographyFailure{code:"author_inline_text_unsupported".into(),path:"inline.math".into()});continue;}
                    let m = m.trim();
                    if !m.is_empty() {
                        out.push_str(&self.office_math(&crate::latex2omml::latex_to_omml(m, false), false));
                    }
                }
                Inline::OfficeMath {omml,..} => {if f.author_size_hp.is_some()||f.author_size_cs_hp.is_some()||f.cs.is_some()||f.rtl.is_some(){self.bibliography_failure=Some(BibliographyFailure{code:"author_inline_text_unsupported".into(),path:"inline.officeMath".into()});continue;}out.push_str(&self.office_math(omml, false));},
                Inline::SoftBreak => {
                    // Between two CJK characters a source line break is not a space.
                    let prev = last_char(&v[..idx]);
                    let next = first_char(&v[idx + 1..]);
                    let cjk = matches!((prev, next), (Some(a), Some(b)) if is_cjk(a) && is_cjk(b));
                    if !cjk {
                        out.push_str(&run_xml(" ", f));
                    }
                }
                Inline::HardBreak => out.push_str(&run_xml("\n",f)),
                Inline::FootnoteRef(label) => {
                    let source = if self.citation_depth == 0 { self.generated_citation_source(label) } else { None };
                    let rendered = self.footnote_ref(label, f);
                    if let Some(source) = source { out.push_str(&self.citation_control(&source,&rendered,Some(label))); }
                    else { out.push_str(&rendered); }
                },
                Inline::Html(_) => {}
            }
        }
        out
    }

    fn footnote_ref(&mut self, label: &str, f: &Fmt) -> String {
        let story=self.note_policies.kind_for_label(label);
        let(kind,reference)=if story.is_end(){("endnote","EndnoteReference")}else{("footnote","FootnoteReference")};
        let Some(def) = self.footnote_defs.get(label).cloned() else {
            return run_xml(&format!("[{label}]"), f);
        };
        if !self.fn_stack.is_empty() {
            // Office forbids a footnoteReference inside a note. Keep readable
            // content in the intermediate package, but the export service must
            // reject it before writing any destination bytes.
            if !self.warns.iter().any(|code| code == crate::api_codes::DOCX_NESTED_FOOTNOTE) {
                self.warns.push(crate::api_codes::DOCX_NESTED_FOOTNOTE.into());
            }
            return run_xml(&format!("[{label}]"), f);
        }
        if let Some(id) = self.footnote_ids.get(label) {
            return note_callout(kind,reference,*id,f);
        }
        // Identity follows the first reference in the main document.
        let ordinal=&mut self.note_counts[usize::from(story.is_end())];
        let Some(id)=ordinal.checked_add(1)else{self.warns.push(note_policy::INVALID.into());return run_xml(&format!("[{label}]"),f)};*ordinal=id;
        self.footnote_ids.insert(label.to_string(), id);
        self.fn_stack.push(label.to_string());
        // One structural writer for both stories: lists, tables, quotes, math
        // and local attributes must not degrade to text or disappear in notes.
        // Relationship/media/numbering state stays shared; only the output
        // buffer is isolated from the main document.
        let main_body = std::mem::take(&mut self.body);
        let saved_baseline = self.local_baseline_half_points.take();
        let saved_author_base=self.author_local_base.take();
        let saved_author_block=self.author_block_base.take();
        self.blocks(&def, &Scope::default());
        self.author_local_base=saved_author_base;self.author_block_base=saved_author_block;
        self.local_baseline_half_points = saved_baseline;
        let fragment = std::mem::replace(&mut self.body, main_body);
        let policy=self.note_policies.paragraph_policy(story);
        let xml = match crate::docx_local_style::footnote_with_policy(&fragment,story,&policy).and_then(|xml| note_paragraph_flow_policy(&xml,&policy)) {
            Ok(xml) => xml,
            Err(code) => { self.warns.push(code.into()); fragment }
        };
        self.fn_stack.pop();
        self.footnotes.push((story,id,xml));
        note_callout(kind,reference,id,f)
    }

    /// `<w:drawing>` for an image, or `None` if it cannot be embedded.
    fn drawing(&mut self, src: &str, alt: &str, block: bool) -> Option<String> {
        let entry = match self.image_cache.get(src) {
            Some(e) => e.clone(),
            None => {
                let loaded = (self.images)(src).and_then(|(ext, bytes)| {
                    let mut ext = ext.to_ascii_lowercase().replace("jpeg", "jpg");
                    let print_width=crate::scientific_figures::print_width(&bytes);
                    let svg=if ext=="svg"&&crate::svg_render::validate(&bytes).is_ok(){Some(bytes.clone())}else{None};
                    let bytes = if ext=="svg" {match crate::svg_render::rasterize(&bytes){Ok(png)=>{ext="png".into();png},Err(_)=>{self.warns.push(format!("DOCX 不支持该图片格式（svg），已跳过：{}",src.chars().take(80).collect::<String>()));return None;}}}else{bytes};
                    if !matches!(ext.as_str(), "png" | "jpg" | "gif" | "bmp") {
                        self.warns.push(format!("DOCX 不支持该图片格式（{ext}），已跳过：{}", src.chars().take(80).collect::<String>()));
                        return None;
                    }
                    let (w, h) = image_size(&bytes).unwrap_or((600, 400));
                    let name = format!("image{}.{ext}", self.media.len() + 1);
                    self.media.push((name.clone(), bytes));
                    let rid = self.rel("image", format!("media/{name}"), false);
                    let svg_rid=svg.map(|svg|{let name=format!("image{}.svg",self.media.len()+1);self.media.push((name.clone(),svg));self.rel("image",format!("media/{name}"),false)});
                    Some((rid, w.max(1), h.max(1),svg_rid,print_width))
                });
                self.image_cache.insert(src.to_string(), loaded.clone());
                loaded
            }
        }?;
        let (rid, pw, ph,svg_rid,print_width) = entry;
        // 96 dpi → EMU, then fit into the configured share of the text block.
        let mut cx = pw as i64 * 9525;
        let mut cy = ph as i64 * 9525;
        if let Some(width)=print_width { cx=(width*36000.).round()as i64;cy=(cx as f64*ph as f64/pw as f64).round()as i64; }
        // An image first used in the body can arrive from the cache without a
        // rel() call. Its references still belong to the footnote part too.
        if let Some(kind)=self.active_note_kind(){
            let relationships=self.footnote_rels.entry(kind).or_default();relationships.insert(rid.clone());
            if let Some(id)=&svg_rid{relationships.insert(id.clone());}
        }
        let svg_extension=svg_rid.map(|id|format!("<a:extLst><a:ext uri=\"{{96DAC541-7B7A-43D3-8B79-37D633B846F1}}\"><asvg:svgBlip xmlns:asvg=\"http://schemas.microsoft.com/office/drawing/2016/SVG/main\" r:embed=\"{id}\"/></a:ext></a:extLst>")).unwrap_or_default();
        let max_w = (self.text_w_emu as f64 * num(self.style, &["images", "widthPct"], 92.0) / 100.0) as i64;
        let max_h = (self.text_h_emu as f64 * num(self.style, &["images", "maxHeightPct"], 85.0) / 100.0) as i64;
        let _ = block;
        if cx > max_w && max_w > 0 {
            cy = cy * max_w / cx;
            cx = max_w;
        }
        if cy > max_h && max_h > 0 {
            cx = cx * max_h / cy;
            cy = max_h;
        }
        self.drawing_id += 1;
        let id = self.drawing_id;
        let descr = xml_escape(alt).replace('\t', "&#9;").replace('\r', "&#13;").replace('\n', "&#10;");
        Some(format!(
            concat!(
                "<w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\">",
                "<wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:docPr id=\"{id}\" name=\"Picture {id}\" descr=\"{descr}\"/>",
                "<wp:cNvGraphicFramePr><a:graphicFrameLocks xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" noChangeAspect=\"1\"/></wp:cNvGraphicFramePr>",
                "<a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">",
                "<a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">",
                "<pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">",
                "<pic:nvPicPr><pic:cNvPr id=\"{id}\" name=\"Picture {id}\" descr=\"{descr}\"/><pic:cNvPicPr/></pic:nvPicPr>",
                "<pic:blipFill><a:blip r:embed=\"{rid}\">{svg_extension}</a:blip><a:stretch><a:fillRect/></a:stretch></pic:blipFill>",
                "<pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr>",
                "</pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
            ),
            cx = cx,
            cy = cy,
            id = id,
            descr = descr,
            rid = rid,
            svg_extension = svg_extension
        ))
    }
}

// ------------------------------------------------------------------- blocks

#[derive(Clone, Default)]
struct Scope {
    /// List nesting depth (indent level for continuation paragraphs).
    list_depth: usize,
    in_quote: bool,
    // Local paragraph post-processing has its own precedence. Such hosts do
    // not register a source chain as if it were an untouched body paragraph.
    nested_math_host: bool,
}

impl<'a> Ctx<'a> {
    fn native_text_base(&self,block:Option<&Block>)->(u32,u32){
        if let Some(base)=self.author_local_base{return base;}
        let hp=|size:f64|(size*2.).round().max(1.).min(i32::MAX as f64)as u32;
        if let Some(kind)=self.active_note_kind(){let policy=self.note_policies.paragraph_policy(kind);return(hp(policy.size.unwrap_or(9.)),hp(policy.size_cs.or(policy.size).unwrap_or(9.)));}
        let context=match block.map(Block::unstyled){Some(Block::Heading{level,..})=>AuthorTextContext::Heading(*level),Some(Block::Caption{..})=>AuthorTextContext::Caption,Some(Block::Table{..})=>AuthorTextContext::Table,_=>AuthorTextContext::Body};
        author_text_base(self.style,context)
    }
    fn para(&mut self, ppr: &str, content: &str) {
        self.body.push_str("<w:p>");
        if !ppr.is_empty() {
            self.body.push_str("<w:pPr>");
            self.body.push_str(ppr);
            self.body.push_str("</w:pPr>");
        }
        self.body.push_str(content);
        self.body.push_str("</w:p>");
    }

    fn bibliography_control(&mut self, tag: &str, content: &str) -> String {
        if content.is_empty() { return String::new(); }
        self.bibliography_control_id += 1;
        format!("<w:sdt><w:sdtPr><w:tag w:val=\"{}\"/><w:id w:val=\"{}\"/></w:sdtPr><w:sdtContent>{content}</w:sdtContent></w:sdt>", xml_escape(tag), self.bibliography_control_id)
    }

    fn bibliography_flow(&mut self, nodes: &[RichLayoutNode], step: i64) -> Vec<BibliographyBlock> {
        fn flush(blocks: &mut Vec<BibliographyBlock>, pending: &mut String, visible: &mut bool, indent: i64) {
            if *visible {
                blocks.push(BibliographyBlock::Paragraph { runs: std::mem::take(pending), indent });
            } else {
                // Processor whitespace between display blocks is layout trivia,
                // not an extra empty paragraph. Retain trailing trivia on the
                // preceding paragraph when there is one.
                if let Some(BibliographyBlock::Paragraph { runs, .. }) = blocks.last_mut() { runs.push_str(pending); }
                pending.clear();
            }
            *visible = false;
        }
        fn walk<'b>(ctx: &mut Ctx<'_>, nodes: &'b [RichLayoutNode], step: i64, indent: i64,
            owners: &BibliographyOwners<'b>, blocks: &mut Vec<BibliographyBlock>, pending: &mut String, visible: &mut bool) {
            for node in nodes {
                match node {
                    RichLayoutNode::Inline { children } => {
                        let mut inlines = crate::md_ast::rich_inlines(children);
                        if let Some(href) = owners.link {
                            inlines = vec![Inline::Link { href: href.into(), title: String::new(), children: inlines }];
                        }
                        let mut runs = ctx.inlines(&inlines, &Fmt::default());
                        for entry_index in owners.entry_indices.iter().rev() {
                            runs = ctx.bibliography_control(&format!("readmd-csl-index:{entry_index}"), &runs);
                        }
                        pending.push_str(&runs);
                        *visible |= bibliography_visible_text(children);
                    }
                    RichLayoutNode::Group { children, entry_index } => {
                        let mut owners = owners.clone();
                        if let Some(index) = entry_index { owners.entry_indices.push(*index); }
                        walk(ctx, children, step, indent, &owners, blocks, pending, visible);
                    }
                    RichLayoutNode::Link { href, children } => {
                        let mut owners = owners.clone(); owners.link = Some(href);
                        walk(ctx, children, step, indent, &owners, blocks, pending, visible);
                    }
                    RichLayoutNode::Block { children, indent: indented } => {
                        flush(blocks, pending, visible, indent);
                        let child_indent = indent + if *indented { step } else { 0 };
                        walk(ctx, children, step, child_indent, owners, blocks, pending, visible);
                        flush(blocks, pending, visible, child_indent);
                    }
                    RichLayoutNode::Columns { left, right } => {
                        flush(blocks, pending, visible, indent);
                        let cell = |ctx: &mut Ctx<'_>, children: &'b [RichLayoutNode]| {
                            let mut output = Vec::new(); let mut runs = String::new(); let mut visible = false;
                            walk(ctx, children, step, 0, owners, &mut output, &mut runs, &mut visible);
                            flush(&mut output, &mut runs, &mut visible, 0); output
                        };
                        blocks.push(BibliographyBlock::Columns { left: cell(ctx, left), right: cell(ctx, right), indent });
                    }
                }
            }
        }
        let mut blocks = Vec::new(); let mut pending = String::new(); let mut visible = false;
        walk(self, nodes, step, 0, &BibliographyOwners::default(), &mut blocks, &mut pending, &mut visible);
        flush(&mut blocks, &mut pending, &mut visible, 0); blocks
    }

    fn bibliography_flow_xml(blocks: &[BibliographyBlock], metrics: &BibliographyMetrics,
        width: i64, after_lines: u128, in_cell: bool, label: bool, path: &str) -> Result<String, BibliographyFailure> {
        // A one-twip structural paragraph keeps nested table cells legal and
        // prevents adjacent tables being merged. It carries no entry gap/text.
        const TAIL: &str = "<w:p><w:pPr><w:keepNext w:val=\"0\"/><w:keepLines w:val=\"0\"/><w:spacing w:before=\"0\" w:after=\"0\" w:beforeLines=\"0\" w:afterLines=\"0\" w:line=\"1\" w:lineRule=\"exact\"/><w:rPr><w:sz w:val=\"1\"/><w:szCs w:val=\"1\"/></w:rPr></w:pPr></w:p>";
        let mut xml = String::new();
        for (index, block) in blocks.iter().enumerate() {
            let after = if index + 1 == blocks.len() { after_lines } else { 0 };
            let node_path = format!("{path}[{index}]");
            match block {
                BibliographyBlock::Paragraph { runs, indent } => {
                    let hanging = if metrics.hanging && !in_cell { metrics.indent_step / 2 } else { 0 };
                    let left = indent + hanging;
                    if width - left < metrics.minimum_text { return Err(BibliographyFailure::geometry(&node_path)); }
                    let wrap = if label { "<w:wordWrap w:val=\"1\"/>" } else { "" };
                    let first_line = if hanging > 0 { format!("w:hanging=\"{hanging}\"") } else { "w:firstLine=\"0\"".into() };
                    xml.push_str(&format!("<w:p><w:pPr><w:pStyle w:val=\"BodyText\"/><w:keepNext w:val=\"0\"/><w:keepLines w:val=\"0\"/>{wrap}<w:spacing w:before=\"0\" w:after=\"0\" w:beforeLines=\"0\" w:afterLines=\"{after}\" w:line=\"{}\" w:lineRule=\"auto\"/><w:ind w:left=\"{left}\" w:right=\"0\" {first_line}/><w:jc w:val=\"left\"/></w:pPr>{runs}</w:p>", metrics.line));
                }
                BibliographyBlock::Columns { left, right, indent } => {
                    let available = width - indent;
                    let label_width = metrics.preferred_label.min(available * 35 / 100);
                    let body_width = available - label_width;
                    // One em of text and the actual gutter must fit. Failure is
                    // physical geometry, not an unsupported CSL tree topology.
                    let minimum_text = metrics.minimum_text;
                    if available <= 0 || label_width - metrics.gutter < minimum_text || body_width < minimum_text {
                        return Err(BibliographyFailure::geometry(&node_path));
                    }
                    let left_xml = Self::bibliography_flow_xml(left, metrics, label_width - metrics.gutter, after, true, true, &format!("{node_path}.left"))?;
                    let right_xml = Self::bibliography_flow_xml(right, metrics, body_width, after, true, false, &format!("{node_path}.right"))?;
                    xml.push_str(&format!("<w:tbl><w:tblPr><w:tblW w:w=\"{available}\" w:type=\"dxa\"/><w:jc w:val=\"left\"/><w:tblInd w:w=\"{indent}\" w:type=\"dxa\"/><w:tblBorders><w:top w:val=\"nil\"/><w:left w:val=\"nil\"/><w:bottom w:val=\"nil\"/><w:right w:val=\"nil\"/><w:insideH w:val=\"nil\"/><w:insideV w:val=\"nil\"/></w:tblBorders><w:tblLayout w:type=\"fixed\"/><w:tblCellMar><w:top w:w=\"0\" w:type=\"dxa\"/><w:left w:w=\"0\" w:type=\"dxa\"/><w:bottom w:w=\"0\" w:type=\"dxa\"/><w:right w:w=\"0\" w:type=\"dxa\"/></w:tblCellMar></w:tblPr><w:tblGrid><w:gridCol w:w=\"{label_width}\"/><w:gridCol w:w=\"{body_width}\"/></w:tblGrid><w:tr>"));
                    for (cell_width, margin, content) in [(label_width, metrics.gutter, left_xml), (body_width, 0, right_xml)] {
                        xml.push_str(&format!("<w:tc><w:tcPr><w:tcW w:w=\"{cell_width}\" w:type=\"dxa\"/><w:tcMar><w:top w:w=\"0\" w:type=\"dxa\"/><w:left w:w=\"0\" w:type=\"dxa\"/><w:bottom w:w=\"0\" w:type=\"dxa\"/><w:right w:w=\"{margin}\" w:type=\"dxa\"/></w:tcMar><w:vAlign w:val=\"top\"/></w:tcPr>"));
                        if content.is_empty() { xml.push_str(TAIL); } else { xml.push_str(&content); }
                        xml.push_str("</w:tc>");
                    }
                    // No cantSplit, fixed row height, keepNext chain or table-
                    // wide unbreakable wrapper: long entries may span pages.
                    xml.push_str("</w:tr></w:tbl>"); xml.push_str(TAIL);
                }
            }
        }
        Ok(xml)
    }

    fn bibliography(&mut self, bibliography: &RichBibliography) -> Result<(), BibliographyFailure> {
        let line_spacing = u64::from(bibliography.line_spacing);
        let after_lines = u128::from(line_spacing) * u128::from(bibliography.entry_spacing) * 100;
        if line_spacing == 0 || after_lines > i32::MAX as u128 {
            return Err(BibliographyFailure { code: "academic_rich_spacing".into(), path: "bibliography.spacing".into() });
        }
        let layouts = bibliography.items.iter().enumerate().map(|(index, item)| {
            if bibliography_has_light(&item.first_field) || bibliography_has_light(&item.rich) {
                return Err(BibliographyFailure { code: "academic_rich_unsupported".into(), path: format!("bibliography.items[{index}].font_weight") });
            }
            item.layout().map_err(|error| BibliographyFailure { code: error.code, path: format!("bibliography.items[{index}]") })
        }).collect::<Result<Vec<_>, _>>()?;
        let body_size = (num(self.style, &["typography", "size"], 11.) * 2.).round() / 2.;
        let has_complex = bibliography.items.iter().zip(&layouts).any(|(item, layout)| !bibliography_simple_item(item, layout));
        let preferred_label = if has_complex { bibliography_preferred_label_twips(&layouts, body_size) } else { bibliography_label_tab_twips(&bibliography.items, body_size) };
        // Mixed simple/complex entries in the same physical container must use
        // the same actual label origin. A long nested label must not push an
        // otherwise simple entry's tab outside the page. Legacy-only output
        // retains its exact previous measurement and paragraph path.
        let label_tab = if has_complex { preferred_label.min(self.text_w_emu / 635 * 35 / 100) } else { preferred_label };
        let metrics = BibliographyMetrics { preferred_label, gutter: twips(body_size * 0.75), minimum_text: twips(body_size), indent_step: twips(body_size * 4.), line: line_spacing * 240, hanging: bibliography.hanging_indent };
        let title = run_xml(&bibliography.title, &Fmt { bold: true, ..Fmt::default() });
        self.para("<w:pStyle w:val=\"Heading2\"/>", &title);
        for (index, (item, layout)) in bibliography.items.iter().zip(&layouts).enumerate() {
            let after = if index + 1 < bibliography.items.len() { after_lines } else { 0 };
            if bibliography_simple_item(item, layout) {
                // Preserve the old native tab/hanging path and its line units.
                let mut content = String::new();
                if !item.first_field.is_empty() {
                    content.push_str(&self.inlines(&crate::md_ast::rich_inlines(&item.first_field), &Fmt::default()));
                    content.push_str("<w:r><w:tab/></w:r>");
                }
                content.push_str(&self.inlines(&crate::md_ast::rich_inlines(&item.rich), &Fmt::default()));
                let hanging = bibliography.hanging_indent || !item.first_field.is_empty();
                let indent = if !item.first_field.is_empty() { label_tab } else if hanging { 360 } else { 0 };
                let ppr = format!("<w:pStyle w:val=\"BodyText\"/><w:ind w:left=\"{indent}\" w:hanging=\"{indent}\"/><w:tabs><w:tab w:val=\"left\" w:pos=\"{label_tab}\"/></w:tabs><w:spacing w:line=\"{}\" w:lineRule=\"auto\" w:after=\"0\" w:afterLines=\"{after}\"/>", line_spacing * 240);
                self.para(&ppr, &content);
            } else {
                let flow = self.bibliography_flow(layout, metrics.indent_step);
                let xml = Self::bibliography_flow_xml(&flow, &metrics, self.text_w_emu / 635, after, false, false, &format!("bibliography.items[{index}].layout"))?;
                let controlled = self.bibliography_control(&format!("readmd-csl-entry:{}", item.key), &xml);
                self.body.push_str(&controlled);
            }
        }
        Ok(())
    }

    fn scope_ppr(&self, sc: &Scope) -> String {
        let mut p = String::new();
        if sc.in_quote {
            p.push_str("<w:pStyle w:val=\"Quote\"/>");
        }
        if sc.list_depth > 0 {
            let left = 720 * sc.list_depth as i64 + if sc.in_quote { 240 } else { 0 };
            p.push_str(&format!("<w:ind w:left=\"{left}\"/>"));
        }
        p
    }

    fn blocks(&mut self, blocks: &[Block], sc: &Scope) {
        for (index, b) in blocks.iter().enumerate() {
            if self.bibliography_failure.is_some() || self.math_failure.is_some() { break; }
            let saved_tracking = self.body_tracking_twips;
            let saved_baseline = self.body_baseline_half_points;
            let front = self.tracking_front_blocks_remaining > 0 && self.fn_stack.is_empty() && !sc.in_quote && sc.list_depth == 0;
            if front { self.tracking_front_blocks_remaining -= 1; self.body_tracking_twips = None; self.body_baseline_half_points = None; }
            let figure_caption = matches!(blocks.get(index + 1).map(Block::unstyled), Some(Block::Caption { label, .. }) if label.kind == "figure");
            self.block(b, sc, figure_caption);
            if front { self.body_tracking_twips = saved_tracking; self.body_baseline_half_points = saved_baseline; }
        }
    }

    fn block(&mut self, b: &Block, sc: &Scope, figure_caption: bool) {
        let saved=self.author_block_base;self.author_block_base=Some(self.native_text_base(Some(b)));self.block_inner(b,sc,figure_caption);self.author_block_base=saved;
    }
    fn block_inner(&mut self, b: &Block, sc: &Scope, figure_caption: bool) {
        match b {
            Block::Styled { attributes,block } => {
                let start=self.body.len();
                let saved_tracking = self.body_tracking_twips;
                let saved_baseline = self.local_baseline_half_points;
                let saved_author_base=self.author_local_base;
                if let Some(size)=attributes["text"]["size"].as_f64(){let hp=(size*2.).round()as u32;self.author_local_base=Some((hp,hp));}
                // A caption's author layer is its own block base. Resolve it
                // before writing nested spans, so an inline `base` reset cannot
                // accidentally target the document template or a prior size.
                match crate::publication_blocks::author_style(attributes) {
                    Ok(author) if !author.is_empty() => {
                        let pair=self.author_local_base.unwrap_or_else(||self.native_text_base(Some(block)));
                        let base=crate::publication_inline_text::ResolvedTextStyle::base(pair.0 as f64/2.,pair.1 as f64/2.);
                        let effective=author.resolve(&base,&base);
                        self.author_local_base=Some(((effective.size*2.).round()as u32,(effective.size_cs*2.).round()as u32));
                    },
                    Ok(_)=>(),
                    Err(code)=>{
                        self.author_local_base=saved_author_base;
                        self.bibliography_failure=Some(BibliographyFailure{code:code.into(),path:"text.author".into()});
                        return;
                    },
                }
                match crate::publication_typography::baseline::local(&attributes["text"]) {
                    Ok(Some(value)) => self.local_baseline_half_points = Some(value.docx_half_points()),
                    Ok(None) => (),
                    Err(code) => {
                        self.author_local_base=saved_author_base;
                        self.bibliography_failure = Some(BibliographyFailure { code: code.into(), path: "text.baselineShiftPt".into() });
                        return;
                    },
                }
                match crate::publication_typography::tracking::local(&attributes["text"]) {
                    Ok(Some(value)) => self.body_tracking_twips = Some(value.docx_twips()),
                    Ok(None) => (),
                    Err(code) => {
                        self.author_local_base=saved_author_base;
                        self.bibliography_failure = Some(BibliographyFailure { code: code.into(), path: "text.trackingPt".into() });
                        return;
                    },
                }
                // Every nested/early return completes this recursive call first.
                // Restore the parent value before any fallible post-processing.
                let mut child_scope=sc.clone();child_scope.nested_math_host=true;
                self.block(block,&child_scope,figure_caption);
                self.author_local_base=saved_author_base;
                self.body_tracking_twips = saved_tracking;
                self.local_baseline_half_points = saved_baseline;
                let fragment=self.body.split_off(start);
                // Tracking was consumed by the normal run writer, where code,
                // CSL and OMML stay distinct. Never patch their XML by font.
                let mut post_attributes = attributes.clone();
                if let Some(text) = post_attributes.get_mut("text").and_then(Value::as_object_mut) { text.remove("trackingPt"); text.remove("baselineShiftPt"); }
                match self.apply_local_layout(&fragment,&post_attributes,block,matches!(block.unstyled(),Block::Paragraph(_))&&self.fn_stack.is_empty()&&!sc.in_quote&&sc.list_depth==0){Ok(xml)=>self.body.push_str(&xml),Err(code)=>{if code.starts_with("author_inline_text_"){self.bibliography_failure=Some(BibliographyFailure{code:code.into(),path:"inline.localLayout".into()});}self.warns.push(code.into());self.body.push_str(&fragment);}}
            }
            Block::Heading { level, id, inlines, numbering } => {
                let lvl = (*level).clamp(1, 6);
                self.bookmark_id += 1;
                let bid = self.bookmark_id;
                let runs = self.inlines(inlines, &Fmt { baseline_half_points: self.local_baseline_half_points, ..Fmt::default() });
                let content = format!(
                    "<w:bookmarkStart w:id=\"{bid}\" w:name=\"{}\"/>{runs}<w:bookmarkEnd w:id=\"{bid}\"/>",
                    bookmark_name(id)
                );
                let nested=self.heading_num.is_some()&&(!self.fn_stack.is_empty()||sc.in_quote||sc.list_depth>0);
                let numbered=numbering.as_ref().and(self.heading_num).map(|num|format!("<w:numPr><w:ilvl w:val=\"{}\"/><w:numId w:val=\"{num}\"/></w:numPr>",lvl-1)).unwrap_or_default();
                let paragraph_style=if nested{format!("ReadMDExampleHeading{lvl}")}else{format!("Heading{lvl}")};
                self.para(&format!("<w:pStyle w:val=\"{paragraph_style}\"/>{numbered}"), &content);
            }
            Block::Paragraph(inl) => {
                if let [Inline::Image { src, alt, .. }] = inl.as_slice() {
                    if let Some(d) = self.drawing(src, alt, true) {
                        let mut ppr = self.scope_ppr(sc);
                        ppr.push_str("<w:jc w:val=\"center\"/>");
                        self.para(&ppr, &d);
                        // A real adjacent figure caption already supplies the visual
                        // title. Keep the image's native accessibility description.
                        if !figure_caption && !alt.trim().is_empty() {
                            let cap = run_xml(alt.trim(), &Fmt::default());
                            self.para("<w:pStyle w:val=\"Caption\"/>", &cap);
                        }
                        return;
                    }
                }
                let ordinary = self.fn_stack.is_empty() && !sc.in_quote && sc.list_depth == 0;
                let fmt = Fmt { tracking_twips: if ordinary { self.body_tracking_twips } else { None },
                    baseline_half_points: self.local_baseline_half_points.or(if ordinary { self.body_baseline_half_points } else { None }), ..Fmt::default() };
                let runs = self.inlines(inl, &fmt);
                let mut ppr = self.scope_ppr(sc);
                if !sc.in_quote && sc.list_depth == 0 { ppr.push_str("<w:pStyle w:val=\"BodyText\"/>"); }
                self.para(&ppr, &runs);
            }
            Block::Caption { label, inlines } => {
                self.bookmark_id += 1;
                let bid = self.bookmark_id;
                let fmt = Fmt { baseline_half_points: self.local_baseline_half_points, ..Fmt::default() };
                let prefix = run_xml(&format!("{}{}", label.prefix, label.numbering.as_ref().map_or(" ",|n|n.label_separator.as_str())), &fmt);
                let number = caption_number_xml(label,&|value|run_xml(value,&fmt));
                let rest = self.inlines(inlines, &fmt);
                let runs = format!("<w:bookmarkStart w:id=\"{bid}\" w:name=\"{}\"/>{prefix}{number}<w:bookmarkEnd w:id=\"{bid}\"/>{rest}", bookmark_name(&label.id));
                self.para("<w:pStyle w:val=\"Caption\"/><w:jc w:val=\"center\"/><w:keepNext w:val=\"0\"/>", &runs);
            }
            Block::List { ordered, start, items } => self.list(*ordered, *start, items, sc),
            Block::Quote(inner) => {
                let mut s2 = sc.clone();
                s2.in_quote = true;
                self.blocks(inner, &s2);
            }
            Block::Code { lang, text } => {
                let mut ppr = String::from("<w:pStyle w:val=\"Code\"/>");
                if sc.list_depth > 0 {
                    ppr.push_str(&format!("<w:ind w:left=\"{}\"/>", 720 * sc.list_depth));
                }
                let mut content = String::new();
                if !lang.is_empty() {
                    content.push_str(&format!(
                        "<w:r><w:rPr><w:color w:val=\"8A8F98\"/><w:sz w:val=\"15\"/></w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r><w:r><w:br/></w:r>",
                        xml_escape(lang)
                    ));
                }
                content.push_str(&highlighted_runs(text, lang));
                self.para(&ppr, &content);
            }
            Block::Math(m) => {
                let m = m.trim();
                if m.is_empty() {
                    return;
                }
                let (ppr,omml)=self.office_math_paragraph(&crate::latex2omml::latex_to_omml(m,true),sc);
                self.para(&ppr, &omml);
            }
            Block::OfficeMath {omml,..} => {let(ppr,xml)=self.office_math_paragraph(omml,sc);self.para(&ppr,&xml);},
            Block::Table { aligns, header, rows } => self.table(aligns, header, rows),
            Block::Hr => {
                let c = color(self.style, &["hr", "color"], "#d8dce2");
                self.para(
                    &format!("<w:pBdr><w:bottom w:val=\"single\" w:sz=\"6\" w:space=\"1\" w:color=\"{c}\"/></w:pBdr><w:spacing w:before=\"120\" w:after=\"120\"/>"),
                    "",
                );
            }
            Block::PageBreak => self.para("", "<w:r><w:br w:type=\"page\"/></w:r>"),
            Block::Bibliography(bibliography) => {
                let start = self.body.len();
                if let Err(failure) = self.bibliography(bibliography) { self.bibliography_failure = Some(failure); }
                else {
                    let rendered = self.body.split_off(start);
                    match self.citation_bindings.bibliography(bibliography.source_span.as_ref(), &rendered) {
                        Ok(xml) => self.body.push_str(&xml),
                        Err(code) => { self.body.push_str(&rendered); self.bibliography_failure = Some(BibliographyFailure {code,path:"bibliography".into()}); },
                    }
                }
            }
            Block::Html(_) | Block::SectionStart(_) => {}
            Block::FootnoteDef { .. } => {}
        }
    }

    fn numbering_for(&mut self, ordered: bool, start: u64, depth: usize) -> u32 {
        if !ordered {
            return self.bullet_num;
        }
        let id = self.next_num;
        self.next_num += 1;
        self.nums.push((id, depth as u32, start));
        id
    }

    fn list(&mut self, ordered: bool, start: u64, items: &[crate::md_ast::ListItem], sc: &Scope) {
        let depth = sc.list_depth.min(8);
        let num_id = self.numbering_for(ordered, start, depth);
        for it in items {
            let mut first = true;
            for (index, blk) in it.blocks.iter().enumerate() {
                if first {
                    first = false;
                    let (first_block, attributes) = match blk {
                        Block::Styled { attributes, block } => (block.as_ref(), Some(attributes)),
                        _ => (blk, None),
                    };
                    if let Block::Paragraph(inl) = first_block {
                        let fragment_start = self.body.len();
                        let runs = self.inlines(inl, &Fmt::default());
                        let mut ppr = String::new();
                        if sc.in_quote {
                            ppr.push_str("<w:pStyle w:val=\"Quote\"/>");
                        } else {
                            ppr.push_str("<w:pStyle w:val=\"ListParagraph\"/>");
                        }
                        let content = match it.task {
                            Some(checked) => {
                                let left = 720 * (depth as i64 + 1);
                                ppr.push_str(&format!("<w:ind w:left=\"{left}\" w:hanging=\"360\"/>"));
                                let box_ = if checked { "\u{2611}" } else { "\u{2610}" };
                                format!(
                                    "<w:r><w:rPr><w:rFonts w:ascii=\"Segoe UI Symbol\" w:hAnsi=\"Segoe UI Symbol\" w:eastAsia=\"Segoe UI Symbol\"/></w:rPr><w:t xml:space=\"preserve\">{box_} </w:t></w:r>{runs}"
                                )
                            }
                            None => {
                                ppr.push_str(&format!("<w:numPr><w:ilvl w:val=\"{depth}\"/><w:numId w:val=\"{num_id}\"/></w:numPr>"));
                                runs
                            }
                        };
                        self.para(&ppr, &content);
                        if let Some(attributes) = attributes {
                            let fragment = self.body.split_off(fragment_start);
                            match self.apply_local_layout(&fragment,attributes,first_block,false) {
                                Ok(xml) => self.body.push_str(&xml),
                                Err(code) => { self.warns.push(code.into()); self.body.push_str(&fragment); }
                            }
                        }
                        continue;
                    }
                }
                let s2 = Scope { list_depth: depth + 1, in_quote: sc.in_quote, nested_math_host:sc.nested_math_host };
                let figure_caption = matches!(it.blocks.get(index + 1).map(Block::unstyled), Some(Block::Caption { label, .. }) if label.kind == "figure");
                self.block(blk, &s2, figure_caption);
            }
        }
    }

    fn table(&mut self, aligns: &[Align], header: &[Vec<Inline>], rows: &[Vec<Vec<Inline>>]) {
        let st = self.style;
        let scholarly = crate::export_styles::three_line_table(st);
        let ncols = header.len().max(rows.iter().map(|r| r.len()).max().unwrap_or(0)).max(1);
        let border = color(st, &["table", "borderColor"], "#c8cdd4");
        let bw = ((num(st, &["table", "borderWidth"], 0.75) * 8.0).round() as i64).clamp(2, 96);
        let pad = twips(num(st, &["table", "cellPadding"], 6.0));
        let width_pct = num(st, &["table", "widthPct"], 100.0).clamp(10.0, 100.0);
        let head_bg = color(st, &["table", "headerBg"], "#3b6ef5");
        let head_fg = if scholarly { color(st, &["typography", "color"], "#111111") } else { color(st, &["table", "headerColor"], "#ffffff") };
        let head_bold = flag(st, &["table", "headerBold"], true);
        let banded = flag(st, &["table", "banded"], true);
        let band = color(st, &["table", "bandColor"], "#f3f5f9");
        let cell_hp = (num(st, &["table", "cellSize"], 10.0) * 2.0).round() as i64;
        let default_align = text(st, &["table", "align"], "left").to_string();
        let grid_w = (self.text_w_emu as f64 / 635.0 * width_pct / 100.0) as i64; // EMU → twips
        let col_w = (grid_w / ncols as i64).max(200);

        let mut x = String::new();
        x.push_str("<w:tbl><w:tblPr>");
        x.push_str(&format!("<w:tblW w:w=\"{}\" w:type=\"pct\"/>", (width_pct * 50.0).round() as i64));
        x.push_str("<w:jc w:val=\"center\"/>");
        let edge = |n: &str| format!("<w:{n} w:val=\"single\" w:sz=\"{bw}\" w:space=\"0\" w:color=\"{border}\"/>");
        x.push_str("<w:tblBorders>");
        for n in ["top", "left", "bottom", "right", "insideH", "insideV"] {
            if scholarly {
                if matches!(n,"top"|"bottom") { x.push_str(&format!("<w:{n} w:val=\"single\" w:sz=\"{}\" w:space=\"0\" w:color=\"{border}\"/>",bw*2)); }
                else { x.push_str(&format!("<w:{n} w:val=\"nil\"/>")); }
            } else { x.push_str(&edge(n)); }
        }
        x.push_str("</w:tblBorders>");
        x.push_str("<w:tblLayout w:type=\"autofit\"/>");
        x.push_str(&format!(
            "<w:tblCellMar><w:top w:w=\"{p}\" w:type=\"dxa\"/><w:left w:w=\"{h}\" w:type=\"dxa\"/><w:bottom w:w=\"{p}\" w:type=\"dxa\"/><w:right w:w=\"{h}\" w:type=\"dxa\"/></w:tblCellMar>",
            p = pad / 2,
            h = pad
        ));
        x.push_str("<w:tblLook w:val=\"04A0\" w:firstRow=\"1\" w:lastRow=\"0\" w:firstColumn=\"0\" w:lastColumn=\"0\" w:noHBand=\"0\" w:noVBand=\"1\"/>");
        x.push_str("</w:tblPr><w:tblGrid>");
        for _ in 0..ncols {
            x.push_str(&format!("<w:gridCol w:w=\"{col_w}\"/>"));
        }
        x.push_str("</w:tblGrid>");

        let row_xml = |ctx: &mut Ctx, cells: &[Vec<Inline>], ri: usize, is_head: bool| -> String {
            let mut r = String::from("<w:tr>");
            if is_head {
                r.push_str("<w:trPr><w:tblHeader/><w:cantSplit/></w:trPr>");
            } else {
                r.push_str("<w:trPr><w:cantSplit/></w:trPr>");
            }
            for ci in 0..ncols {
                let align = match aligns.get(ci).copied().unwrap_or(Align::None) {
                    Align::None => default_align.clone(),
                    a => a.as_str().to_string(),
                };
                let fill = if scholarly { None } else if is_head {
                    Some(head_bg.clone())
                } else if banded && ri % 2 == 1 {
                    Some(band.clone())
                } else {
                    None
                };
                let f = Fmt {
                    bold: is_head && head_bold,
                    color: if is_head { Some(head_fg.clone()) } else { None },
                    size_hp: Some(cell_hp),
                    block_base_hp:Some(ctx.author_local_base.unwrap_or((cell_hp as u32,cell_hp as u32))),
                    ..Fmt::default()
                };
                let content = cells.get(ci).map(|c| ctx.inlines(c, &f)).unwrap_or_default();
                r.push_str("<w:tc><w:tcPr>");
                r.push_str(&format!("<w:tcW w:w=\"{col_w}\" w:type=\"dxa\"/>"));
                if scholarly && is_head { r.push_str(&format!("<w:tcBorders>{}</w:tcBorders>",edge("bottom"))); }
                if let Some(fl) = fill {
                    r.push_str(&format!("<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{fl}\"/>"));
                }
                r.push_str("<w:vAlign w:val=\"center\"/></w:tcPr>");
                r.push_str(&format!(
                    "<w:p><w:pPr><w:pStyle w:val=\"TableText\"/><w:jc w:val=\"{}\"/></w:pPr>{content}</w:p></w:tc>",
                    jc(&align)
                ));
            }
            r.push_str("</w:tr>");
            r
        };
        let h = row_xml(self, header, 0, true);
        x.push_str(&h);
        for (ri, r) in rows.iter().enumerate() {
            let t = row_xml(self, r, ri, false);
            x.push_str(&t);
        }
        x.push_str("</w:tbl>");
        self.body.push_str(&x);
        // Word needs a paragraph between adjacent tables and after a trailing one.
        self.para("<w:spacing w:before=\"0\" w:after=\"0\"/>", "");
    }
}

// --------------------------------------------------------- code highlighting

/// Runs for a code block, coloured by [`crate::code_highlight`].
fn highlighted_runs(code: &str, lang: &str) -> String {
    let mut out = String::new();
    for (kind, tok) in crate::code_highlight::tokenize(code, lang) {
        let f = Fmt { color: crate::code_highlight::color_of(kind).map(|c| c.to_string()), italic: kind == crate::code_highlight::Kind::Comment, ..Fmt::default() };
        out.push_str(&run_xml(tok, &f));
    }
    out
}

// ------------------------------------------------------------------- styles

fn styles_xml(st: &Value, language: Option<&str>, body_line: Option<crate::publication_typography::DocxLine>, line_context: crate::publication_typography::LineContext, numbered_headings:bool, math_style:bool) -> String {
    let layout = crate::publication_layout::Layout::from_style(st);
    let body_font = font_family(text(st, &["typography", "font"], "MicrosoftYaHei")).to_string();
    let code_font = font_family(text(st, &["code", "font"], "Consolas")).to_string();
    let body_hp = (num(st, &["typography", "size"], 11.0) * 2.0).round() as i64;
    let body_color = color(st, &["typography", "color"], "#262626");
    let line = (num(st, &["typography", "lineHeight"], line_context.legacy_multiple()) * 240.0).round() as i64;
    let after = twips(num(st, &["typography", "spacing"], 6.0));
    let body_jc = jc(text(st, &["typography", "align"], "left"));
    let first_indent = mm_twips(num(st, &["typography", "firstLineIndent"], 0.0));
    let p04_spacing = body_line.map(|line| format!("<w:snapToGrid w:val=\"0\"/><w:spacing w:line=\"{}\" w:lineRule=\"{}\"/>", line.line, line.rule)).unwrap_or_default();
    let code_hp = (num(st, &["code", "size"], 9.5) * 2.0).round() as i64;
    let code_bg = color(st, &["code", "bg"], "#f5f6f8");
    let code_fg = color(st, &["code", "color"], "#2f3b4a");
    let code_bd = color(st, &["code", "borderColor"], "#dfe3e8");
    let code_bw = ((num(st, &["code", "borderWidth"], 0.5) * 8.0).round() as i64).clamp(2, 48);
    let q_bar = color(st, &["quote", "barColor"], "#3b6ef5");
    let q_bg = color(st, &["quote", "bg"], "#f3f6ff");
    let q_fg = color(st, &["quote", "color"], "#4a5568");
    let link = color(st, &["link", "color"], "#2b6cb0");
    let cell_hp = (num(st, &["table", "cellSize"], 10.0) * 2.0).round() as i64;
    let fonts = |f: &str| format!("<w:rFonts w:ascii=\"{f}\" w:hAnsi=\"{f}\" w:eastAsia=\"{f}\" w:cs=\"{f}\"/>");

    let mut s = String::new();
    let language_properties = language.map(|lang|format!("<w:lang w:val=\"{0}\" w:eastAsia=\"{0}\" w:bidi=\"{0}\"/>",xml_escape(lang)))
        .unwrap_or_else(||"<w:lang w:val=\"en-US\" w:eastAsia=\"zh-CN\"/>".into());
    s.push_str(&format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:styles xmlns:w=\"{W_NS}\">"));
    let body_spacing=format!("<w:spacing w:after=\"{after}\" w:line=\"{line}\" w:lineRule=\"auto\"/>");
    s.push_str(&format!(
        "<w:docDefaults><w:rPrDefault><w:rPr>{}<w:sz w:val=\"{body_hp}\"/><w:szCs w:val=\"{body_hp}\"/><w:color w:val=\"{body_color}\"/>{language_properties}</w:rPr></w:rPrDefault>\
<w:pPrDefault><w:pPr>{}</w:pPr></w:pPrDefault></w:docDefaults>",
        fonts(&body_font),if math_style{""}else{&body_spacing}
    ));
    s.push_str(&format!(
        "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/><w:pPr><w:widowControl w:val=\"{}\"/>{}<w:jc w:val=\"{body_jc}\"/></w:pPr></w:style>",
        u8::from(layout.widow_control),if math_style{&body_spacing}else{""}
    ));
    if math_style{s.push_str("<w:style w:type=\"paragraph\" w:styleId=\"ReadMDMath\"><w:name w:val=\"Equation\"/><w:next w:val=\"Normal\"/><w:qFormat/></w:style>");}
    s.push_str(&format!("<w:style w:type=\"paragraph\" w:styleId=\"BodyText\"><w:name w:val=\"Body Text\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"BodyText\"/><w:qFormat/><w:pPr><w:keepLines w:val=\"{}\"/>{p04_spacing}<w:ind w:firstLine=\"{first_indent}\"/></w:pPr></w:style>",u8::from(layout.keep_lines)));
    for i in 1..=6 {
        let key = format!("h{i}");
        let h = |k: &str| dig(st, &["headings", key.as_str(), k]);
        let size_d = [20.0, 16.0, 14.0, 12.0, 11.0, 10.5][i - 1];
        let hp = (h("size").and_then(|v| v.as_f64()).unwrap_or(size_d) * 2.0).round() as i64;
        let c = color(st, &["headings", key.as_str(), "color"], "#1a1a1a");
        let bold = h("bold").and_then(|v| v.as_bool()).unwrap_or(true);
        let al = jc(h("align").and_then(|v| v.as_str()).unwrap_or("left"));
        let before = twips(h("before").and_then(|v| v.as_f64()).unwrap_or(12.0));
        let aft = twips(h("after").and_then(|v| v.as_f64()).unwrap_or(6.0));
        let pbb = if h("pageBreakBefore").and_then(|v| v.as_bool()).unwrap_or(false) { "<w:pageBreakBefore/>" } else { "" };
        let keep_next = u8::from(h("keepNext").and_then(Value::as_bool).unwrap_or(true));
        let keep_lines = u8::from(h("keepLines").and_then(Value::as_bool).unwrap_or(true));
        let entry=format!(
            "<w:style w:type=\"paragraph\" w:styleId=\"Heading{i}\"><w:name w:val=\"heading {i}\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:uiPriority w:val=\"9\"/><w:qFormat/>\
<w:pPr><w:keepNext w:val=\"{keep_next}\"/><w:keepLines w:val=\"{keep_lines}\"/>{pbb}<w:spacing w:before=\"{before}\" w:after=\"{aft}\" w:line=\"276\" w:lineRule=\"auto\"/><w:jc w:val=\"{al}\"/><w:outlineLvl w:val=\"{}\"/></w:pPr>\
<w:rPr>{}<w:sz w:val=\"{hp}\"/><w:szCs w:val=\"{hp}\"/><w:color w:val=\"{c}\"/></w:rPr></w:style>",
            i - 1,
            if bold { "<w:b/><w:bCs/>" } else { "" }
        );
        s.push_str(&entry);
        if numbered_headings {
            // Examples retain the same typography, without becoming a new
            // chapter for STYLEREF/SEQ or entering the numbered outline.
            s.push_str(&entry.replace(&format!("w:styleId=\"Heading{i}\""),&format!("w:styleId=\"ReadMDExampleHeading{i}\""))
                .replace(&format!("w:val=\"heading {i}\""),&format!("w:val=\"ReadMD example heading {i}\""))
                .replace(&format!("<w:outlineLvl w:val=\"{}\"/>",i-1),"<w:outlineLvl w:val=\"9\"/>"));
        }
    }
    s.push_str(&format!(
        "<w:style w:type=\"paragraph\" w:styleId=\"Title\"><w:name w:val=\"Title\"/><w:basedOn w:val=\"Normal\"/><w:qFormat/><w:pPr><w:spacing w:before=\"2400\" w:after=\"240\"/><w:jc w:val=\"center\"/></w:pPr><w:rPr><w:b/><w:sz w:val=\"56\"/><w:color w:val=\"{}\"/></w:rPr></w:style>",
        color(st, &["headings", "h1", "color"], "#1a1a1a")
    ));
    s.push_str("<w:style w:type=\"paragraph\" w:styleId=\"Subtitle\"><w:name w:val=\"Subtitle\"/><w:basedOn w:val=\"Normal\"/><w:qFormat/><w:pPr><w:spacing w:after=\"240\"/><w:jc w:val=\"center\"/></w:pPr><w:rPr><w:sz w:val=\"30\"/><w:color w:val=\"5C6470\"/></w:rPr></w:style>");
    s.push_str(&format!(
        "<w:style w:type=\"paragraph\" w:styleId=\"Code\"><w:name w:val=\"Code\"/><w:basedOn w:val=\"Normal\"/><w:qFormat/>\
<w:pPr><w:pBdr><w:top w:val=\"single\" w:sz=\"{code_bw}\" w:space=\"4\" w:color=\"{code_bd}\"/><w:left w:val=\"single\" w:sz=\"{code_bw}\" w:space=\"4\" w:color=\"{code_bd}\"/><w:bottom w:val=\"single\" w:sz=\"{code_bw}\" w:space=\"4\" w:color=\"{code_bd}\"/><w:right w:val=\"single\" w:sz=\"{code_bw}\" w:space=\"4\" w:color=\"{code_bd}\"/></w:pBdr>\
<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{code_bg}\"/><w:spacing w:before=\"120\" w:after=\"160\" w:line=\"260\" w:lineRule=\"auto\"/><w:ind w:left=\"120\" w:right=\"120\"/><w:jc w:val=\"left\"/></w:pPr>\
<w:rPr><w:rFonts w:ascii=\"{code_font}\" w:hAnsi=\"{code_font}\" w:eastAsia=\"{body_font}\" w:cs=\"{code_font}\"/><w:noProof/><w:sz w:val=\"{code_hp}\"/><w:szCs w:val=\"{code_hp}\"/><w:color w:val=\"{code_fg}\"/></w:rPr></w:style>"
    ));
    s.push_str(&format!(
        "<w:style w:type=\"character\" w:styleId=\"CodeChar\"><w:name w:val=\"Inline Code\"/><w:rPr><w:rFonts w:ascii=\"{code_font}\" w:hAnsi=\"{code_font}\" w:eastAsia=\"{body_font}\" w:cs=\"{code_font}\"/><w:noProof/><w:color w:val=\"{code_fg}\"/><w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{code_bg}\"/></w:rPr></w:style>"
    ));
    s.push_str(&format!(
        "<w:style w:type=\"paragraph\" w:styleId=\"Quote\"><w:name w:val=\"Quote\"/><w:basedOn w:val=\"Normal\"/><w:qFormat/>\
<w:pPr><w:pBdr><w:left w:val=\"single\" w:sz=\"24\" w:space=\"8\" w:color=\"{q_bar}\"/></w:pBdr><w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{q_bg}\"/><w:ind w:left=\"240\" w:right=\"120\"/></w:pPr>\
<w:rPr><w:color w:val=\"{q_fg}\"/></w:rPr></w:style>"
    ));
    s.push_str(&format!(
        "<w:style w:type=\"character\" w:styleId=\"Hyperlink\"><w:name w:val=\"Hyperlink\"/><w:uiPriority w:val=\"99\"/><w:unhideWhenUsed/><w:rPr><w:color w:val=\"{link}\"/><w:u w:val=\"single\"/></w:rPr></w:style>"
    ));
    s.push_str("<w:style w:type=\"paragraph\" w:styleId=\"ListParagraph\"><w:name w:val=\"List Paragraph\"/><w:basedOn w:val=\"Normal\"/><w:qFormat/><w:pPr><w:spacing w:after=\"60\"/><w:contextualSpacing/></w:pPr></w:style>");
    s.push_str(&format!(
        "<w:style w:type=\"paragraph\" w:styleId=\"TableText\"><w:name w:val=\"Table Text\"/><w:basedOn w:val=\"Normal\"/><w:pPr><w:spacing w:before=\"0\" w:after=\"0\" w:line=\"264\" w:lineRule=\"auto\"/></w:pPr><w:rPr><w:sz w:val=\"{cell_hp}\"/><w:szCs w:val=\"{cell_hp}\"/></w:rPr></w:style>"
    ));
    s.push_str("<w:style w:type=\"paragraph\" w:styleId=\"Caption\"><w:name w:val=\"caption\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/><w:pPr><w:spacing w:before=\"60\" w:after=\"200\"/><w:jc w:val=\"center\"/></w:pPr><w:rPr><w:i/><w:sz w:val=\"18\"/><w:color w:val=\"5C6470\"/></w:rPr></w:style>");
    let note_policies=note_policy::parse_layer(st).unwrap_or_default();
    for kind in configured_note_kinds(st,&note_policies){
        let policy=note_policies.paragraph_policy(kind);let note_style=if kind.is_end(){"Endnote"}else{"Footnote"};
        let note_hp=(policy.size.unwrap_or(9.)*2.).round()as u32;let note_cs=(policy.size_cs.or(policy.size).unwrap_or(9.)*2.).round()as u32;
        let line=policy.line_spacing.unwrap_or(crate::publication_typography::LineSpacing::Multiple(1.)).docx();
        s.push_str(&format!("<w:style w:type=\"paragraph\" w:styleId=\"{note_style}Text\"><w:name w:val=\"{} text\"/><w:basedOn w:val=\"Normal\"/><w:pPr><w:spacing w:after=\"0\" w:line=\"{}\" w:lineRule=\"{}\"/></w:pPr><w:rPr><w:sz w:val=\"{note_hp}\"/><w:szCs w:val=\"{note_cs}\"/></w:rPr></w:style>",note_style.to_lowercase(),line.line,line.rule));
        s.push_str(&format!("<w:style w:type=\"character\" w:styleId=\"{note_style}Reference\"><w:name w:val=\"{} reference\"/><w:rPr><w:vertAlign w:val=\"superscript\"/></w:rPr></w:style>",note_style.to_lowercase()));
    }
    s.push_str("<w:style w:type=\"paragraph\" w:styleId=\"TOCHeading\"><w:name w:val=\"TOC Heading\"/><w:basedOn w:val=\"Heading1\"/><w:next w:val=\"Normal\"/><w:pPr><w:outlineLvl w:val=\"9\"/></w:pPr></w:style>");
    let (toc_width, _) = crate::export_styles::page_dimensions(st);
    let toc_tab = mm_twips((toc_width - num(st,&["page","marginLeft"],18.) - num(st,&["page","marginRight"],18.) - layout.gutter).max(20.));
    let toc_leader = crate::publication_layout::toc_leader(st);
    for i in 1..=6 {
        s.push_str(&format!(
            "<w:style w:type=\"paragraph\" w:styleId=\"TOC{i}\"><w:name w:val=\"toc {i}\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:uiPriority w:val=\"39\"/><w:pPr><w:tabs><w:tab w:val=\"right\" w:leader=\"{toc_leader}\" w:pos=\"{toc_tab}\"/></w:tabs><w:spacing w:after=\"60\"/><w:ind w:left=\"{}\"/></w:pPr></w:style>",
            (i - 1) * 240
        ));
    }
    for (id,size) in [("Header",layout.header_size),("Footer",layout.footer_size)] {
        let hp=(size*2.).round() as u32;
        s.push_str(&format!("<w:style w:type=\"paragraph\" w:styleId=\"{id}\"><w:name w:val=\"{}\"/><w:basedOn w:val=\"Normal\"/><w:pPr><w:spacing w:after=\"0\"/></w:pPr><w:rPr><w:sz w:val=\"{hp}\"/><w:szCs w:val=\"{hp}\"/><w:color w:val=\"808080\"/></w:rPr></w:style>",id.to_lowercase()));
    }
    s.push_str("</w:styles>");
    s
}

fn caption_number_xml(label:&crate::cross_references::Label,run:&dyn Fn(&str)->String)->String {
    let field=|instruction:&str,cached:&str|format!("<w:fldSimple w:instr=\"{}\" w:dirty=\"true\">{}</w:fldSimple>",xml_escape(instruction),run(cached));
    let Some(numbering)=&label.numbering else {return field(&format!(" SEQ {} \\* ARABIC ",label.sequence()),&label.number.to_string());};
    let mut result=String::new();
    if let Some(chapter)=&numbering.chapter {
        result.push_str(&field(&format!(" STYLEREF {} \\s ",chapter.level),&chapter.number));
        result.push_str(&run(&numbering.separator));
    }
    let reset=numbering.chapter.as_ref().map(|chapter|format!(" \\s {}",chapter.level)).unwrap_or_default();
    let format=crate::cross_references::word_number_format(&numbering.format);
    if numbering.start==1 {
        result.push_str(&field(&format!(" SEQ {}{reset} \\* {format} ",label.sequence()),&crate::cross_references::number_text(&numbering.format,label.number)));
    } else {
        // A formula around the native sequence offset remains updateable when
        // captions move. Hard-coding a one-time SEQ reset would not.
        result.push_str("<w:r><w:fldChar w:fldCharType=\"begin\" w:dirty=\"true\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> = </w:instrText></w:r>");
        // Nested complex fields have explicit code/result boundaries. A simple
        // field inserted amongst the outer instruction runs is ambiguous to
        // field updaters and may expose its ordinal as visible duplicate text.
        result.push_str(&format!("<w:r><w:fldChar w:fldCharType=\"begin\" w:dirty=\"true\"/></w:r><w:r><w:instrText xml:space=\"preserve\">{}</w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{}<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",xml_escape(&format!(" SEQ {}{reset} ",label.sequence())),run(&numbering.ordinal.to_string())));
        let offset=if numbering.start==0{" - 1".into()}else{format!(" + {}",numbering.start-1)};
        result.push_str(&format!("<w:r><w:instrText xml:space=\"preserve\">{offset} \\* {format} </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{}<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",run(&crate::cross_references::number_text(&numbering.format,label.number))));
    }
    result
}

fn heading_rules(blocks:&[Block])->Option<crate::cross_references::HeadingNumbering>{
    blocks.iter().find_map(|block|match block.unstyled(){Block::Heading{numbering:Some(value),..}=>Some(value.clone()),_=>None})
}

fn numbering_xml(nums: &[(u32, u32, u64)], bullet_num: u32, headings:Option<(u32,&crate::cross_references::HeadingNumbering)>) -> String {
    const BULLETS: [&str; 3] = ["\u{2022}", "\u{25E6}", "\u{25AA}"];
    let mut s = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:numbering xmlns:w=\"{W_NS}\">");
    // abstract 0: bullets
    s.push_str("<w:abstractNum w:abstractNumId=\"0\"><w:multiLevelType w:val=\"hybridMultilevel\"/>");
    for lvl in 0..9 {
        let b = BULLETS[lvl % 3];
        s.push_str(&format!(
            "<w:lvl w:ilvl=\"{lvl}\"><w:start w:val=\"1\"/><w:numFmt w:val=\"bullet\"/><w:lvlText w:val=\"{b}\"/><w:lvlJc w:val=\"left\"/><w:pPr><w:ind w:left=\"{}\" w:hanging=\"360\"/></w:pPr></w:lvl>",
            720 * (lvl + 1)
        ));
    }
    s.push_str("</w:abstractNum>");
    // abstract 1: decimal / lower-letter / lower-roman cycle
    s.push_str("<w:abstractNum w:abstractNumId=\"1\"><w:multiLevelType w:val=\"hybridMultilevel\"/>");
    for lvl in 0..9 {
        let fmt = ["decimal", "lowerLetter", "lowerRoman"][lvl % 3];
        s.push_str(&format!(
            "<w:lvl w:ilvl=\"{lvl}\"><w:start w:val=\"1\"/><w:numFmt w:val=\"{fmt}\"/><w:lvlText w:val=\"%{}.\"/><w:lvlJc w:val=\"left\"/><w:pPr><w:ind w:left=\"{}\" w:hanging=\"360\"/></w:pPr></w:lvl>",
            lvl + 1,
            720 * (lvl + 1)
        ));
    }
    s.push_str("</w:abstractNum>");
    if let Some((_,heading))=headings {
        s.push_str("<w:abstractNum w:abstractNumId=\"2\"><w:multiLevelType w:val=\"multilevel\"/>");
        for (level,rule) in heading.rules.iter().enumerate(){
            let legal=if rule.is_legal{"<w:isLgl/>"}else{""};
            s.push_str(&format!("<w:lvl w:ilvl=\"{level}\"><w:start w:val=\"{}\"/><w:numFmt w:val=\"{}\"/><w:lvlRestart w:val=\"{}\"/><w:pStyle w:val=\"Heading{}\"/><w:suff w:val=\"{}\"/><w:lvlText w:val=\"{}\"/>{legal}<w:lvlJc w:val=\"left\"/></w:lvl>",rule.start,xml_escape(&rule.format),rule.restart_after,level+1,xml_escape(&rule.suffix),xml_escape(&rule.text)));
        }
        s.push_str("</w:abstractNum>");
    }
    s.push_str(&format!("<w:num w:numId=\"{bullet_num}\"><w:abstractNumId w:val=\"0\"/></w:num>"));
    if let Some((id,_))=headings {s.push_str(&format!("<w:num w:numId=\"{id}\"><w:abstractNumId w:val=\"2\"/></w:num>"));}
    for (id, lvl, start) in nums {
        s.push_str(&format!(
            "<w:num w:numId=\"{id}\"><w:abstractNumId w:val=\"1\"/><w:lvlOverride w:ilvl=\"{lvl}\"><w:startOverride w:val=\"{start}\"/></w:lvlOverride></w:num>"
        ));
    }
    s.push_str("</w:numbering>");
    s
}

// ------------------------------------------------------------------ package

fn configured_note_kinds(style:&Value,policies:&NotePolicies)->BTreeSet<NoteKind>{
    let mut kinds=policies.kind_by_label.values().copied().collect::<BTreeSet<_>>();
    if let Some(configured)=style["footnotes"].get("kinds").and_then(Value::as_object){
        for kind in NoteKind::ALL{if configured.get(kind.as_str()).is_some_and(|v|v.as_object().is_some_and(|o|!o.is_empty())){kinds.insert(kind);}}
    }
    kinds.insert(if policies.position==Some(note_policy::NotePosition::Document){NoteKind::Endnote}else{NoteKind::Footnote});kinds
}
fn note_section_properties(style:&Value,overrides:Option<&note_policy::SectionNotePolicies>)->String{
    let Ok(policies)=note_policy::parse_layer(style)else{return String::new()};
    let footnote_placement=if configured_note_kinds(style,&policies).contains(&NoteKind::Footnote){policies.policy(NoteKind::Footnote).placement}else{None};
    let mut properties=String::new();
    for kind in NoteKind::ALL{
        let mut value=String::new();
        if kind==NoteKind::Footnote{if let Some(placement)=footnote_placement{value.push_str(&format!("<w:pos w:val=\"{}\"/>",placement.word_value()));}}
        if let Some(overrides)=overrides{
            let numbering=if kind.is_end(){&overrides.endnote_numbering}else{&overrides.footnote_numbering};
            if numbering.number_format.is_some()||numbering.number_pattern.is_some(){
                let effective=policies.policy(kind);
                let format=numbering.number_format.as_deref().or(effective.number_format.as_deref()).unwrap_or(if kind.is_end(){"lowerRoman"}else{"decimal"});
                // A format-only override deliberately omits w:format, clearing
                // an inherited pattern. A pattern-only override still needs
                // the effective native val required by CT_NumFmt.
                let pattern=numbering.number_pattern.as_ref().map(|pattern|format!(" w:format=\"{}\"",xml_escape(pattern))).unwrap_or_default();
                value.push_str(&format!("<w:numFmt w:val=\"{format}\"{pattern}/>"));
            }
            if let Some(start)=numbering.start{value.push_str(&format!("<w:numStart w:val=\"{start}\"/>"));}
            if let Some(restart)=numbering.restart{value.push_str(&format!("<w:numRestart w:val=\"{}\"/>",restart.word_value()));}
        }
        if !value.is_empty(){let name=kind.as_str();properties.push_str(&format!("<w:{name}Pr>{value}</w:{name}Pr>"));}
    }
    properties
}
fn note_document_properties(kind:NoteKind,policy:&NotePolicyPatch,notice_id:Option<u32>)->String{
    let name=kind.as_str();let mut value=String::new();
    if kind.is_end(){value.push_str(&format!("<w:pos w:val=\"{}\"/>",policy.placement.unwrap_or(note_policy::NotePlacement::DocEnd).word_value()));}
    let pattern=policy.number_pattern.as_ref().map(|p|format!(" w:format=\"{}\"",xml_escape(p))).unwrap_or_default();
    value.push_str(&format!("<w:numFmt w:val=\"{}\"{pattern}/>",policy.number_format.as_deref().unwrap_or(if kind.is_end(){"lowerRoman"}else{"decimal"})));
    if let Some(start)=policy.start{value.push_str(&format!("<w:numStart w:val=\"{start}\"/>"));}
    if let Some(restart)=policy.restart{value.push_str(&format!("<w:numRestart w:val=\"{}\"/>",restart.word_value()));}
    value.push_str(&format!("<w:{name} w:id=\"-1\"/><w:{name} w:id=\"0\"/>"));
    if let Some(id)=notice_id{value.push_str(&format!("<w:{name} w:id=\"{id}\"/>"));}format!("<w:{name}Pr>{value}</w:{name}Pr>")
}

fn note_story(kind:NoteKind,policy:&NotePolicyPatch,definitions:&[(NoteKind,u32,String)],count:u32)->(String,Option<u32>){
    let name=kind.as_str();let mut value=format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:{name}s xmlns:w=\"{W_NS}\" xmlns:r=\"{R_NS}\" xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\">");
    for(id,ty,marker,on)in[(-1,"separator","separator",policy.separator!=Some(NoteSeparator::None)),(0,"continuationSeparator","continuationSeparator",policy.continuation_separator!=Some(NoteSeparator::None))]{
        value.push_str(&format!("<w:{name} w:type=\"{ty}\" w:id=\"{id}\"><w:p><w:pPr><w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/></w:pPr>{}</w:p></w:{name}>",if on{format!("<w:r><w:{marker}/></w:r>")}else{String::new()}));
    }
    for(story,id,xml)in definitions{if *story==kind{value.push_str(&format!("<w:{name} w:id=\"{id}\">{xml}</w:{name}>"));}}
    let notice=policy.continuation_notice.as_deref().filter(|s|!s.is_empty());
    let notice_id=notice.and_then(|_|count.checked_add(1));
    if let(Some(notice),Some(id))=(notice,notice_id){
        value.push_str(&format!("<w:{name} w:type=\"continuationNotice\" w:id=\"{id}\"><w:p><w:pPr><w:pStyle w:val=\"{}Text\"/></w:pPr>{}</w:p></w:{name}>",if kind.is_end(){"Endnote"}else{"Footnote"},run_xml(notice,&Fmt::default())));
    }
    value.push_str(&format!("</w:{name}s>"));(value,notice_id)
}

/// Note flow defaults never replace explicit local paragraph controls. Keeping
/// paragraphs together still permits a multi-page note to break between them.
fn note_paragraph_flow(fragment:&str,style:&Value)->Result<String,&'static str>{
    let policies=note_policy::parse_layer(style).map_err(|_|note_policy::INVALID)?;
    let kind=if crate::publication_layout::notes_at_end(style){NoteKind::Endnote}else{NoteKind::Footnote};
    note_paragraph_flow_policy(fragment,&policies.paragraph_policy(kind))
}
fn note_paragraph_flow_policy(fragment:&str,policy:&NotePolicyPatch)->Result<String,&'static str>{
    use crate::math_document::{XmlNode,XmlPart};
    let Some(long)=policy.long_note else{return Ok(fragment.into())};let keep=long==LongNote::KeepParagraphs;
    let mut root=crate::math_document::parse_xml(fragment).map_err(|_|"publication_local_style_invalid")?;
    let control=crate::math_document::parse_xml(&format!("<w:keepLines w:val=\"{}\"/>",keep as u8)).map_err(|_|"publication_local_style_invalid")?
        .content.into_iter().find(|part|matches!(part,XmlPart::Element(_))).ok_or("publication_local_style_invalid")?;
    fn walk(node:&mut XmlNode,control:&XmlPart)->Result<(),&'static str>{
        if node.name=="w:p" {
            if !node.children().any(|child|child.name=="w:pPr"){
                let properties=crate::math_document::parse_xml("<w:pPr/>").map_err(|_|"publication_local_style_invalid")?
                    .content.into_iter().find(|part|matches!(part,XmlPart::Element(_))).ok_or("publication_local_style_invalid")?;
                node.content.insert(0,properties);
            }
            if let Some(XmlPart::Element(properties))=node.content.iter_mut().find(|part|matches!(part,XmlPart::Element(child) if child.name=="w:pPr")){
                if !properties.children().any(|child|child.name=="w:keepLines"){
                    let at=properties.content.iter().take_while(|part|matches!(part,XmlPart::Element(child) if matches!(child.name.as_str(),"w:pStyle"|"w:keepNext"))).count();
                    properties.content.insert(at,control.clone());
                }
            }
        }
        for part in &mut node.content{if let XmlPart::Element(child)=part{walk(child,control)?;}}
        Ok(())
    }
    walk(&mut root,&control)?;Ok(root.xml())
}

fn collect_footnote_defs(blocks: &[Block], into: &mut HashMap<String, Vec<Block>>) {
    for b in blocks {
        match b {
            Block::FootnoteDef { label, blocks } => {
                into.insert(label.clone(), blocks.clone());
            }
            Block::Quote(inner) => collect_footnote_defs(inner, into),
            Block::List { items, .. } => {
                for it in items {
                    collect_footnote_defs(&it.blocks, into);
                }
            }
            _ => {}
        }
    }
}

/// Document title used for `core.xml` / the cover page.
pub fn doc_title(doc: &Document, st: &Value, fallback: &str) -> String {
    let t = text(st, &["meta", "title"], "").trim();
    if !t.is_empty() {
        return t.to_string();
    }
    if let Some(t) = doc.meta("title") {
        return t;
    }
    for b in &doc.blocks {
        if let Block::Heading { level: 1, inlines, .. } = b.unstyled() {
            let t = inline_plain(inlines);
            if !t.trim().is_empty() {
                return t.trim().to_string();
            }
        }
    }
    fallback.to_string()
}

fn header_footer(tag: &str, style_id: &str, content: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:{tag} xmlns:w=\"{W_NS}\" xmlns:r=\"{R_NS}\"><w:p><w:pPr><w:pStyle w:val=\"{style_id}\"/>{content}</w:{tag}>"
    )
}

/// A section's properties describe this section (including how it STARTS),
/// while OOXML stores them at its END. Missing running references implement
/// Word's real Link to Previous; an independent blank is a real blank part.
fn publication_section_properties(ctx: &mut Ctx<'_>, parts: &mut Vec<(String, Vec<u8>)>,
    section: &crate::publication_sections::ResolvedSection, index: usize) -> String {
    let st = &section.style;
    let layout = crate::publication_layout::Layout::from_style(st);
    let mut refs = String::new();
    for (kind, tag, style_id, linked) in [("header", "hdr", "Header", section.header_link_previous),
        ("footer", "ftr", "Footer", section.footer_link_previous)] {
        if linked { continue; }
        for mode in ["default", "even", "first"] {
            if mode == "even" && !layout.different_odd_even { continue; }
            if mode == "first" && !layout.hide_first { continue; }
            let key = match mode { "even" => "evenText", "first" => "firstText", _ => "text" };
            let value = text(st, &[kind, key], "");
            let mut runs = run_xml(value, &Fmt::default());
            // A blank first-page part implements differentFirstPage, not an
            // accidentally inherited default header/footer.
            if kind == "footer" && mode != "first" && flag(st, &["footer", "pageNumbers"], true) {
                if !value.is_empty() { runs.push_str(&run_xml("    ", &Fmt::default())); }
                let field = |name: &str, cached: &str| format!(concat!(
                    "<w:r><w:fldChar w:fldCharType=\"begin\" w:dirty=\"true\"/></w:r>",
                    "<w:r><w:instrText xml:space=\"preserve\"> {name} </w:instrText></w:r>",
                    "<w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{}",
                    "<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>"), run_xml(cached, &Fmt::default()), name=name);
                let cached = crate::publication_layout::note_number(&layout.page_format, section.page_start.unwrap_or(1) as usize);
                runs.push_str(&field("PAGE", &cached));
                if layout.page_total { runs.push_str(&run_xml(" / ", &Fmt::default())); runs.push_str(&field("NUMPAGES", "1")); }
            }
            let align = jc(text(st, &[kind, "align"], if kind == "header" { "left" } else { "center" }));
            let xml = header_footer(tag, style_id, &format!("<w:jc w:val=\"{align}\"/></w:pPr>{runs}</w:p>"));
            let name = format!("{kind}-section-{index}-{mode}.xml");
            parts.push((format!("word/{name}"), xml.into_bytes()));
            let relation = ctx.rel(if kind == "header" { "header" } else { "footer" }, name, false);
            refs.push_str(&format!("<w:{kind}Reference w:type=\"{mode}\" r:id=\"{relation}\"/>"));
        }
    }
    let (width, height) = crate::export_styles::page_dimensions(st);
    let orient = if width > height { "landscape" } else { "portrait" };
    let start = section.page_start.map(|n| format!(" w:start=\"{n}\"")).unwrap_or_default();
    let note_properties=note_section_properties(st,section.note_overrides.as_ref());
    format!(concat!("<w:sectPr>{refs}{note_properties}<w:type w:val=\"{}\"/>",
        "<w:pgSz w:w=\"{}\" w:h=\"{}\" w:orient=\"{orient}\"/>",
        "<w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\" w:header=\"{}\" w:footer=\"{}\" w:gutter=\"{}\"/>",
        "<w:pgNumType w:fmt=\"{}\"{start}/><w:cols w:num=\"{}\" w:space=\"{}\" w:equalWidth=\"1\"/>{}<w:docGrid w:linePitch=\"360\"/></w:sectPr>"),
        section.break_kind.word_value(), mm_twips(width), mm_twips(height),
        mm_twips(num(st, &["page","marginTop"],20.)), mm_twips(num(st, &["page","marginRight"],18.)),
        mm_twips(num(st, &["page","marginBottom"],20.)), mm_twips(num(st, &["page","marginLeft"],18.)),
        mm_twips(layout.header_distance), mm_twips(layout.footer_distance), mm_twips(layout.gutter),
        layout.page_format, layout.columns, mm_twips(layout.column_gap), if layout.hide_first { "<w:titlePg/>" } else { "" },
        refs=refs, orient=orient, start=start,note_properties=note_properties)
}

/// Separate bookmark namespace from user-authored heading/caption identifiers.
pub fn section_bookmark_name(id: &str) -> String {
    let mut name = bookmark_name(&format!("section:{id}"));
    name.replace_range(..2, "_s");
    name
}

fn mark_publication_section(ctx: &mut Ctx<'_>, begin: usize, id: &str) {
    if !ctx.body[begin..].contains("<w:p>") { ctx.para("<w:spacing w:before=\"0\" w:after=\"0\" w:line=\"20\" w:lineRule=\"exact\"/>", ""); }
    let at = begin + ctx.body[begin..].find("<w:p>").expect("section paragraph") + "<w:p>".len();
    // pPr must remain the first child of a paragraph, before range markup.
    let at = if ctx.body[at..].starts_with("<w:pPr>") {
        at + ctx.body[at..].find("</w:pPr>").expect("paragraph properties") + "</w:pPr>".len()
    } else { at };
    ctx.bookmark_id += 1;
    let number = ctx.bookmark_id;
    let name = section_bookmark_name(id);
    ctx.body.insert_str(at, &format!("<w:bookmarkStart w:id=\"{number}\" w:name=\"{name}\"/><w:bookmarkEnd w:id=\"{number}\"/>"));
}

/// Build every part of the `.docx` package.
pub fn build(doc: &Document, style: &Value, source_name: &str, images: ImageLoader) -> DocxPackage {
    match crate::publication_typography::ResolvedOptions::from_request(style) {
        Ok(options) => build_with_lines(doc, options.values(), source_name, images, options.line_context()),
        Err(code) => DocxPackage { parts: Vec::new(), warns: vec![code.into()] },
    }
}
pub(crate) fn build_with_lines(doc: &Document, style: &Value, source_name: &str, images: ImageLoader, line_context: crate::publication_typography::LineContext) -> DocxPackage {
    let resolved_math = match crate::math_document_defaults::DocumentDefaultsPlan::analyze(doc)
        .and_then(|plan| plan.render_document(doc)) {
        Ok(value) => value,
        Err(error) => return DocxPackage { parts: Vec::new(), warns: vec![error.message()] },
    };
    let doc = resolved_math.as_ref();
    if let Err(code)=crate::math_word_projection::validate_render_document_with_style(doc,"docx",style){return DocxPackage{parts:Vec::new(),warns:vec![code]};}
    if let Err(error)=note_policy::validate_consumer(note_policy::Consumer::Docx,doc,style){return DocxPackage{parts:Vec::new(),warns:vec![error.code.into()]};}
    if let Err(code) = crate::publication_typography::validate_docx_document(doc, style) {
        return DocxPackage { parts: Vec::new(), warns: vec![code.into()] };
    }
    if let Err(code) = crate::publication_sections::validate_document(doc) {
        return DocxPackage { parts: Vec::new(), warns: vec![code.into()] };
    }
    match crate::publication_metadata_formats::prepare(doc, style, &doc_title(doc,style,source_name), "docx") {
        Ok((prepared,style,metadata)) => {
            // Only the shared preparation may prepend these blocks. Never trust
            // an options key or source-declared context as front-matter identity.
            let Some(front_count) = prepared.blocks.len().checked_sub(doc.blocks.len()) else {
                return DocxPackage { parts: Vec::new(), warns: vec!["publication_metadata_type".into()] };
            };
            if prepared.blocks.get(front_count..) != Some(doc.blocks.as_slice()) {
                return DocxPackage { parts: Vec::new(), warns: vec!["publication_metadata_type".into()] };
            }
            let mut result=build_inner(&prepared, &style, source_name, images, line_context, front_count);
            result.warns.extend(crate::publication_metadata_formats::warnings(&metadata));result
        },
        Err(code) => DocxPackage { parts: Vec::new(), warns: vec![code] },
    }
}
fn native_math_paragraph_sources(doc:&Document)->Result<Vec<crate::math_word::NativeParagraphSource>,&'static str>{
    let mut sources=Vec::new();let mut bytes=0usize;
    // Match the writer's unwrapped body lane. Nested hosts already have
    // explicit geometry/preflight rules and cannot acquire unused styles.
    for block in &doc.blocks {
        if let Block::OfficeMath{omml,..}=block {
            let tree=crate::math_document::parse_xml(omml)?;
            if let Some(source)=crate::math_word::capture_native_paragraph(&tree)? {
                bytes=bytes.checked_add(source.description_bytes()).ok_or("math_budget")?;
                if sources.len()>=4096||bytes>16*1024*1024{return Err("math_budget");}
                sources.push(source);
            }
        }
    }
    Ok(sources)
}

fn build_inner(doc: &Document, style: &Value, source_name: &str, images: ImageLoader, line_context: crate::publication_typography::LineContext, front_count: usize) -> DocxPackage {
    let body_baseline_half_points = match crate::publication_typography::baseline::global(style) {
        Ok(value) => value.map(|value| value.docx_half_points()),
        Err(code) => return DocxPackage { parts: Vec::new(), warns: vec![code.into()] },
    };
    let body_tracking_twips = match crate::publication_typography::tracking::global(style) {
        Ok(value) => value.map(|value| value.docx_twips()),
        Err(code) => return DocxPackage { parts: Vec::new(), warns: vec![code.into()] },
    };
    match crate::math_spacing::document_has_spacing(doc){
        Ok(false)=>(),Ok(true)=>return DocxPackage{parts:Vec::new(),warns:vec!["math_unsupported_extension".into()]},
        Err(code)=>return DocxPackage{parts:Vec::new(),warns:vec![code.into()]},
    }
    let body_line = match crate::publication_typography::global_line_spacing(style) {
        Ok(value) => value.map(|line| line.docx()),
        Err(code) => return DocxPackage { parts: Vec::new(), warns: vec![code.into()] },
    };
    let sectioned = if crate::publication_sections::has_sections(doc) {
        match crate::publication_sections::resolve(doc, style) {
            Ok(sections) => Some(sections),
            Err(code) => return DocxPackage { parts: Vec::new(), warns: vec![code.into()] },
        }
    } else { None };
    let language=doc.publication_language(style);
    let layout = crate::publication_layout::Layout::from_style(style);
    let note_policies=match note_policy::parse_layer(style){Ok(value)=>value,Err(error)=>return DocxPackage{parts:Vec::new(),warns:vec![error.code.into()]}};
    let note_kinds=configured_note_kinds(style,&note_policies);
    let (page_w, page_h) = crate::export_styles::page_dimensions(style);
    let (mt, mr, mb, ml) = (
        num(style, &["page", "marginTop"], 20.0),
        num(style, &["page", "marginRight"], 18.0),
        num(style, &["page", "marginBottom"], 20.0),
        num(style, &["page", "marginLeft"], 18.0),
    );
    let text_w_emu = (layout.column_width(style).max(20.0) * 36000.0) as i64;
    let text_h_emu = (((page_h - mt - mb).max(20.0)) * 36000.0) as i64;

    let mut footnote_defs = HashMap::new();
    collect_footnote_defs(&doc.blocks, &mut footnote_defs);
    fn collect_labels(blocks: &[Block], labels: &mut HashMap<String, String>) {
        for block in blocks { match block {
            Block::Styled {block,..}=>collect_labels(std::slice::from_ref(block.as_ref()),labels),
            Block::Caption {label,..} => { labels.insert(label.id.clone(), label.text()); },
            Block::Quote(inner) | Block::FootnoteDef {blocks:inner,..} => collect_labels(inner, labels),
            Block::List {items,..} => for item in items { collect_labels(&item.blocks, labels); },
            _ => (),
        } }
    }
    let mut labels = HashMap::new(); collect_labels(&doc.blocks, &mut labels);

    let heading_definition=heading_rules(&doc.blocks);
    let native_sources=match native_math_paragraph_sources(doc){
        Ok(sources)=>sources,Err(code)=>return DocxPackage{parts:Vec::new(),warns:vec![code.into()]},
    };
    let math_paragraph_registry=match crate::math_word::NativeParagraphRegistry::prepare(
        &styles_xml(style,language.as_deref(),body_line,line_context,heading_definition.is_some(),true),&native_sources
    ){
        Ok(registry)=>registry,Err(code)=>return DocxPackage{parts:Vec::new(),warns:vec![code.into()]},
    };
    let mut ctx = Ctx {
        author_token:uuid::Uuid::new_v4().simple().to_string(),
        author_block_base:None,
        author_local_base:None,
        line_context,
        body_tracking_twips,
        body_baseline_half_points,
        local_baseline_half_points: None,
        tracking_front_blocks_remaining: front_count,
        style,
        images,
        body: String::new(),
        rels: Vec::new(),
        media: Vec::new(),
        image_cache: HashMap::new(),
        bullet_num: 1,
        heading_num:heading_definition.as_ref().map(|_|2),
        nums: Vec::new(),
        next_num: if heading_definition.is_some(){3}else{2},
        bookmark_id: 0,
        labels,
        publication_section_ids: sectioned.as_ref().map(|sections| sections.iter().filter(|section| section.explicit).map(|section| section.id.clone()).collect()).unwrap_or_default(),
        drawing_id: 0,
        footnote_defs,
        footnotes: Vec::new(),
        footnote_ids: HashMap::new(),
        footnote_rels: HashMap::new(),
        note_policies,
        note_counts:[0,0],
        fn_stack: Vec::new(),
        warns: Vec::new(),
        math_defaults: Default::default(),
        math_paragraph_registry,
        has_math_paragraph_style:false,
        math_failure: None,
        text_w_emu,
        text_h_emu,
        bibliography_control_id: 0,
        bibliography_failure: None,
        citation_bindings: Default::default(),
        citation_depth: 0,
    };
    // Fixed relationships first so their ids are stable.
    ctx.rel("styles", "styles.xml".into(), false);
    ctx.rel("numbering", "numbering.xml".into(), false);
    ctx.rel("settings", "settings.xml".into(), false);
    for kind in &note_kinds{ctx.rel(if kind.is_end(){"endnotes"}else{"footnotes"},format!("{}s.xml",kind.as_str()),false);}

    let title = doc_title(doc, style, source_name);

    // Cover page.
    let cover = flag(style, &["cover", "enabled"], false);
    let toc = flag(style, &["toc", "enabled"], false);
    let front_section = sectioned.is_none() && layout.columns > 1 && (cover || toc);
    if cover {
        let ct = text(style, &["cover", "title"], "").trim();
        let ct = if ct.is_empty() { title.as_str() } else { ct };
        let al = jc(text(style, &["cover", "align"], "center"));
        ctx.para(&format!("<w:pStyle w:val=\"Title\"/><w:jc w:val=\"{al}\"/>"), &run_xml(ct, &Fmt::default()));
        let sub = text(style, &["cover", "subtitle"], "").trim().to_string();
        if !sub.is_empty() {
            ctx.para(&format!("<w:pStyle w:val=\"Subtitle\"/><w:jc w:val=\"{al}\"/>"), &run_xml(&sub, &Fmt::default()));
        }
        let author = text(style, &["meta", "author"], "").trim().to_string();
        if !author.is_empty() {
            ctx.para(&format!("<w:pStyle w:val=\"Subtitle\"/><w:jc w:val=\"{al}\"/>"), &run_xml(&author, &Fmt::default()));
        }
        let date = text(style, &["cover", "date"], "").trim().to_string();
        if !date.is_empty() {
            ctx.para(&format!("<w:pStyle w:val=\"Subtitle\"/><w:jc w:val=\"{al}\"/>"), &run_xml(&date, &Fmt::default()));
        }
        if !front_section || toc { ctx.para("", "<w:r><w:br w:type=\"page\"/></w:r>"); }
    }

    // Table of contents (a TOC field; Word fills it on open / F9).
    if toc {
        ctx.para("<w:pStyle w:val=\"TOCHeading\"/>", &run_xml("目录", &Fmt::default()));
        let depth = crate::publication_layout::toc_depth(style);
        ctx.body.push_str(&format!(concat!(
            "<w:p><w:r><w:fldChar w:fldCharType=\"begin\" w:dirty=\"true\"/></w:r>",
            "<w:r><w:instrText xml:space=\"preserve\"> TOC \\o \"1-{depth}\" \\h \\z \\u </w:instrText></w:r>",
            "<w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>",
            "<w:r><w:rPr><w:color w:val=\"808080\"/></w:rPr><w:t>打开文档后按 F9（或右键“更新域”）生成目录。</w:t></w:r>",
            "<w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:p>"
        ), depth=depth));
        if !front_section {ctx.para("", "<w:r><w:br w:type=\"page\"/></w:r>");}
    }
    let front_end=ctx.body.len();
    let mut section_ends = Vec::new();
    if let Some(sections) = &sectioned {
        for section in sections {
            ctx.style = &section.style;
            let sl = crate::publication_layout::Layout::from_style(&section.style);
            let (_, height) = crate::export_styles::page_dimensions(&section.style);
            ctx.text_w_emu = (sl.column_width(&section.style) * 36000.) as i64;
            ctx.text_h_emu = ((height - num(&section.style, &["page","marginTop"],20.) - num(&section.style, &["page","marginBottom"],20.)) * 36000.) as i64;
            let begin = ctx.body.len();
            ctx.blocks(&doc.blocks[section.block_range.clone()], &Scope::default());
            mark_publication_section(&mut ctx, begin, &section.id);
            section_ends.push(ctx.body.len());
        }
        ctx.style = style;
    } else { ctx.blocks(&doc.blocks, &Scope::default()); }
    if let Some(error) = ctx.math_failure.take() {
        return DocxPackage { parts: Vec::new(), warns: vec![error] };
    }
    if let Some(failure) = ctx.bibliography_failure.take() {
        // No package reaches the atomic exporter on a failed display layout.
        // Keep the existing localized code first and a diagnostic path separate.
        return DocxPackage { parts: Vec::new(), warns: vec![failure.code, failure.path] };
    }
    if ctx.body.is_empty() {
        ctx.para("", "");
    }

    // Header / footer.
    let page_numbers = flag(style, &["footer", "pageNumbers"], true);
    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
    let mut sect_refs = String::new();
    if sectioned.is_none() { for (mode,index) in [("default",1),("even",2)] {
    if mode=="even"&&!layout.different_odd_even{continue;}
    let key=if mode=="even"{"evenText"}else{"text"};
    let header_text=text(style,&["header",key],"").trim().chars().take(500).collect::<String>();
    let footer_text=text(style,&["footer",key],"").trim().chars().take(500).collect::<String>();
    // Explicit empty even parts prevent Word inheriting the odd content.
    if !header_text.is_empty() || mode=="even" {
        let al = jc(text(style, &["header", "align"], "left"));
        let x = header_footer(
            "hdr",
            "Header",
            &format!("<w:jc w:val=\"{al}\"/></w:pPr>{}</w:p>", run_xml(&header_text, &Fmt::default())),
        );
        parts.push((format!("word/header{index}.xml"), x.into_bytes()));
        let id = ctx.rel("header", format!("header{index}.xml"), false);
        sect_refs.push_str(&format!("<w:headerReference w:type=\"{mode}\" r:id=\"{id}\"/>"));
    }
    if !footer_text.is_empty() || page_numbers || mode=="even" {
        let mut runs = String::new();
        if !footer_text.is_empty() {
            runs.push_str(&run_xml(&footer_text, &Fmt::default()));
            if page_numbers {
                runs.push_str(&run_xml("    ", &Fmt::default()));
            }
        }
        if page_numbers {
            let field=|name:&str,cached:&str|format!("<w:r><w:fldChar w:fldCharType=\"begin\" w:dirty=\"true\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> {name} </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{}<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",run_xml(cached,&Fmt::default()));
            runs.push_str(&field("PAGE",&layout.page_start.to_string()));
            if layout.page_total {runs.push_str(&run_xml(" / ",&Fmt::default()));runs.push_str(&field("NUMPAGES","1"));}
        }
        let x = header_footer("ftr", "Footer", &format!("<w:jc w:val=\"{}\"/></w:pPr>{runs}</w:p>",layout.footer_align));
        parts.push((format!("word/footer{index}.xml"), x.into_bytes()));
        let id = ctx.rel("footer", format!("footer{index}.xml"), false);
        sect_refs.push_str(&format!("<w:footerReference w:type=\"{mode}\" r:id=\"{id}\"/>"));
    }
    } }

    let (w_tw, h_tw) = (mm_twips(page_w), mm_twips(page_h));
    let orient = if page_w > page_h { " w:orient=\"landscape\"" } else { "" };
    let note_properties=note_section_properties(style,None);
    let section=|columns:u32,first:bool,start:bool,next:bool|format!(
        "<w:sectPr>{sect_refs}{note_properties}{}<w:pgSz w:w=\"{w_tw}\" w:h=\"{h_tw}\"{orient}/><w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\" w:header=\"{}\" w:footer=\"{}\" w:gutter=\"{}\"/><w:pgNumType w:fmt=\"{}\"{}/><w:cols w:num=\"{columns}\" w:space=\"{}\"/>{}<w:docGrid w:linePitch=\"360\"/></w:sectPr>",
        if next{"<w:type w:val=\"nextPage\"/>"}else{""},mm_twips(mt),mm_twips(mr),mm_twips(mb),mm_twips(ml),
        mm_twips(layout.header_distance),mm_twips(layout.footer_distance),mm_twips(layout.gutter),layout.page_format,
        if start{format!(" w:start=\"{}\"",layout.page_start)}else{String::new()},mm_twips(layout.column_gap),if first{"<w:titlePg/>"}else{""});
    if front_section {ctx.body.insert_str(front_end,&format!("<w:p><w:pPr>{}</w:pPr></w:p>",section(1,layout.hide_first,true,true)));}
    let sect = if let Some(sections) = &sectioned {
        let properties = sections.iter().enumerate().map(|(index, s)| publication_section_properties(&mut ctx, &mut parts, s, index)).collect::<Vec<_>>();
        // Insert backwards so original section-end offsets remain valid. A
        // table-ending section gets its mandatory following paragraph here.
        for index in (0..properties.len().saturating_sub(1)).rev() {
            ctx.body.insert_str(section_ends[index], &format!("<w:p><w:pPr><w:spacing w:before=\"0\" w:after=\"0\" w:line=\"20\" w:lineRule=\"exact\"/>{}</w:pPr></w:p>", properties[index]));
        }
        properties.last().cloned().expect("resolved final section")
    } else { section(layout.columns,layout.hide_first&&!front_section,!front_section,false) };
    // The source section offsets above still refer to the intermediate bytes.
    // Remove private run-axis markers only after every offset is consumed.
    match crate::docx_local_style::strip_author_axes(&ctx.body,&ctx.author_token){Ok(body)=>ctx.body=body,Err(code)=>return DocxPackage{parts:Vec::new(),warns:vec![code.into()]}}
    for(_,_,xml)in &mut ctx.footnotes{match crate::docx_local_style::strip_author_axes(xml,&ctx.author_token){Ok(clean)=>*xml=clean,Err(code)=>return DocxPackage{parts:Vec::new(),warns:vec![code.into()]}}}
    let document = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
            "<w:document xmlns:w=\"{w}\" xmlns:r=\"{r}\" ",
            "xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\" ",
            "xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" ",
            "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" ",
            "xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">",
            "<w:body>{body}{sect}</w:body></w:document>"
        ),
        w = W_NS,
        r = R_NS,
        body = ctx.body,
        sect = sect
    );

    let mut note_parts=Vec::new();let mut note_settings=String::new();
    for kind in &note_kinds{
        let policy=ctx.note_policies.policy(*kind);let(story,notice)=note_story(*kind,&policy,&ctx.footnotes,ctx.note_counts[usize::from(kind.is_end())]);
        note_parts.push((format!("word/{}s.xml",kind.as_str()),story.into_bytes()));note_settings.push_str(&note_document_properties(*kind,&policy,notice));
    }
    let settings = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:settings xmlns:w=\"{W_NS}\">{}<w:defaultTabStop w:val=\"420\"/><w:characterSpacingControl w:val=\"compressPunctuation\"/>\
<w:updateFields w:val=\"true\"/>{note_settings}\
<w:compat><w:compatSetting w:name=\"compatibilityMode\" w:uri=\"http://schemas.microsoft.com/office/word\" w:val=\"15\"/></w:compat>{}</w:settings>",
        format!("{}{}",if layout.mirror {"<w:mirrorMargins/>"}else{""},if layout.different_odd_even{"<w:evenAndOddHeaders/>"}else{""}),
        ctx.math_defaults.settings().map(|n| n.xml()).unwrap_or_default()
    );

    // Relationships.
    let mut rels = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"{REL_NS}\">");
    let mut note_rels=note_kinds.iter().map(|kind|(*kind,rels.clone())).collect::<HashMap<_,_>>();
    for r in &ctx.rels {
        let ty = format!("http://schemas.openxmlformats.org/officeDocument/2006/relationships/{}", r.kind);
        let mode = if r.external { " TargetMode=\"External\"" } else { "" };
        let xml = format!("<Relationship Id=\"{}\" Type=\"{ty}\" Target=\"{}\"{mode}/>", r.id, r.target);
        rels.push_str(&xml);
        for kind in &note_kinds{if ctx.footnote_rels.get(kind).is_some_and(|set|set.contains(&r.id)){note_rels.get_mut(kind).unwrap().push_str(&xml);}}
    }
    rels.push_str("</Relationships>");
    for value in note_rels.values_mut(){value.push_str("</Relationships>");}

    let mut ct = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">");
    ct.push_str("<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/>");
    for (ext, mime) in [("png", "image/png"), ("jpg", "image/jpeg"), ("gif", "image/gif"), ("bmp", "image/bmp"), ("svg","image/svg+xml")] {
        ct.push_str(&format!("<Default Extension=\"{ext}\" ContentType=\"{mime}\"/>"));
    }
    let wml = "application/vnd.openxmlformats-officedocument.wordprocessingml";
    for (part, kind) in [
        ("/word/document.xml", "document.main+xml"),
        ("/word/styles.xml", "styles+xml"),
        ("/word/numbering.xml", "numbering+xml"),
        ("/word/settings.xml", "settings+xml"),
    ] {
        ct.push_str(&format!("<Override PartName=\"{part}\" ContentType=\"{wml}.{kind}\"/>"));
    }
    for kind in &note_kinds{ct.push_str(&format!("<Override PartName=\"/word/{}s.xml\" ContentType=\"{wml}.{}s+xml\"/>",kind.as_str(),kind.as_str()));}
    for (part,_) in &parts {
        if part.starts_with("word/header")||part.starts_with("word/footer") {
            let kind=if part.starts_with("word/header"){"header"}else{"footer"};
            ct.push_str(&format!("<Override PartName=\"/{part}\" ContentType=\"{wml}.{kind}+xml\"/>"));
        }
    }
    let publication=style.get("_publication").cloned().and_then(|value|serde_json::from_value::<crate::publication_metadata::PublicationMetadata>(value).ok());
    if publication.is_some(){ct.push_str("<Override PartName=\"/docProps/custom.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.custom-properties+xml\"/>");}
    ct.push_str("<Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/>");
    ct.push_str("<Override PartName=\"/docProps/app.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.extended-properties+xml\"/>");
    ct.push_str("</Types>");

    let mut root_rels = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"{REL_NS}\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/>\
<Relationship Id=\"rId3\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties\" Target=\"docProps/app.xml\"/></Relationships>"
    );
    if publication.is_some(){root_rels=root_rels.replace("</Relationships>","<Relationship Id=\"rId4\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties\" Target=\"docProps/custom.xml\"/></Relationships>");}
    let now = time::OffsetDateTime::now_utc();
    let stamp = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    );
    let mut core = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\
<dc:title>{}</dc:title><dc:subject>{}</dc:subject><dc:creator>{}</dc:creator>{}\
<dcterms:created xsi:type=\"dcterms:W3CDTF\">{stamp}</dcterms:created><dcterms:modified xsi:type=\"dcterms:W3CDTF\">{stamp}</dcterms:modified></cp:coreProperties>",
        xml_escape(&title),
        xml_escape(text(style, &["meta", "subject"], "")),
        xml_escape(text(style, &["meta", "author"], "")),
        language.as_ref().map(|lang|format!("<dc:language>{}</dc:language>",xml_escape(lang))).unwrap_or_default()
    );
    if let Some(metadata)=&publication{core=crate::publication_metadata_formats::core_properties(metadata,&stamp);}
    let app = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\"><Application>ReadMD</Application></Properties>";

    let mut native_styles=styles_xml(style,language.as_deref(),body_line,line_context,heading_definition.is_some(),ctx.has_math_paragraph_style);
    let definitions=ctx.math_paragraph_registry.style_xml();
    if !definitions.is_empty(){
        let Some(prefix)=native_styles.strip_suffix("</w:styles>")else{
            return DocxPackage{parts:Vec::new(),warns:vec!["math_invalid_source".into()]};
        };
        native_styles=format!("{prefix}{definitions}</w:styles>");
    }
    let mut all: Vec<(String, Vec<u8>)> = vec![
        ("[Content_Types].xml".into(), ct.into_bytes()),
        ("_rels/.rels".into(), root_rels.into_bytes()),
        ("docProps/core.xml".into(), core.into_bytes()),
        ("docProps/app.xml".into(), app.as_bytes().to_vec()),
        ("word/document.xml".into(), document.into_bytes()),
        ("word/styles.xml".into(), native_styles.into_bytes()),
        ("word/numbering.xml".into(), numbering_xml(&ctx.nums, ctx.bullet_num,ctx.heading_num.zip(heading_definition.as_ref())).into_bytes()),
        ("word/settings.xml".into(), settings.into_bytes()),
        ("word/_rels/document.xml.rels".into(), rels.into_bytes()),
    ];
    all.extend(note_parts);
    if let Some(metadata)=&publication{all.push(("docProps/custom.xml".into(),crate::publication_metadata_formats::custom_properties(metadata).into_bytes()));}
    all.extend(parts);
    for kind in &note_kinds{if ctx.footnote_rels.get(kind).is_some_and(|set|!set.is_empty()){all.push((format!("word/_rels/{}s.xml.rels",kind.as_str()),note_rels.remove(kind).unwrap().into_bytes()));}}
    for (name, bytes) in ctx.media {
        all.push((format!("word/media/{name}"), bytes));
    }
    if let Err(code) = ctx.citation_bindings.attach(&mut all) {
        return DocxPackage {parts:Vec::new(),warns:vec![code]};
    }
    DocxPackage { parts: all, warns: ctx.warns }
}

#[cfg(test)]
#[path = "docx_bibliography_layout_tests.rs"]
mod bibliography_layout_tests;

#[cfg(test)]
#[path = "docx_notes_layout_tests.rs"]
mod notes_layout_tests;

#[cfg(test)]
#[path = "docx_note_policies_tests.rs"]
mod note_policies_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::md_ast::parse;

    #[test]
    fn document_language_reaches_default_runs_and_package_metadata_without_changing_undeclared_defaults() {
        let doc=parse("---\nlanguage: de_DE\n---\n# Publication\n\nBody.");
        for (options,expected) in [(serde_json::json!({}),"de-DE"),(serde_json::json!({"meta":{"language":"ja-JP"}}),"ja-JP")] {
            let style=crate::export_styles::sanitize(&options);let package=build(&doc,&style,"Paper",&|_|None);
            let part=|name:&str|String::from_utf8(package.parts.iter().find(|(p,_)|p==name).unwrap().1.clone()).unwrap();
            assert!(part("word/styles.xml").contains(&format!("<w:lang w:val=\"{expected}\" w:eastAsia=\"{expected}\" w:bidi=\"{expected}\"/>")));
            assert!(part("docProps/core.xml").contains(&format!("<dc:language>{expected}</dc:language>")));
        }
        let package=build(&parse("Plain"),&crate::export_styles::sanitize(&serde_json::json!({})),"Plain",&|_|None);
        let styles=String::from_utf8(package.parts.iter().find(|(p,_)|p=="word/styles.xml").unwrap().1.clone()).unwrap();assert!(styles.contains("<w:lang w:val=\"en-US\" w:eastAsia=\"zh-CN\"/>"));
    }

    const RICH: &str = "---\ntitle: 报告\n---\n\n# 第一章\n\n##### 五级\n\n正文 *斜体* **粗体** ~~删除~~ `代码` [外链](https://example.com?a=1&b=2) 与 [内链](#第一章)。\n\n1. 一\n2. 二\n   - 嵌套\n   - [x] 完成\n\n> 引用\n\n| 左 | 中 | 右 |\n|:--|:-:|--:|\n| a | b | c |\n\n```rust\nfn main() {}\n```\n\n$$\\frac{a}{b}$$\n\n行内 $x^2$ 公式[^n]。\n\n![图](a.png)\n\n---\n\n[^n]: 脚注内容\n";

    fn png_1x1() -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        b.extend_from_slice(&200u32.to_be_bytes());
        b.extend_from_slice(&100u32.to_be_bytes());
        b.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
        b
    }

    fn part<'p>(pkg: &'p DocxPackage, name: &str) -> &'p str {
        let (_, b) = pkg.parts.iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("missing {name}"));
        std::str::from_utf8(b).unwrap()
    }

    #[test]
    fn bibliography_entry_spacing_uses_ooxml_line_units_in_the_package() {
        use crate::publication_rich::{RichBibliography, RichBibliographyItem, RichNode};
        // These are OOXML structure assertions. Actual Word line metrics and
        // application round trips require separate native Word acceptance.
        for (line_spacing,entry_spacing,expected_line,expected_after) in [
            (1,0,"240","0"),(1,1,"240","100"),(2,1,"480","200"),
            (2,3,"480","600"),(11,1,"2640","1100"),
        ] {
            for (size,half_points) in [(9.,18),(17.,34)] {
                let mut doc=parse("");
                doc.blocks.push(Block::Bibliography(RichBibliography {
                    title:"References".into(),hanging_indent:true,line_spacing,entry_spacing,source_span:None,
                    items:[("one","1.","First entry"),("two","2.","Second entry")].into_iter()
                        .map(|(key,label,text)|RichBibliographyItem {key:key.into(),first_field:vec![RichNode::text(label)],rich:vec![RichNode::text(text)]}).collect(),
                }));
                doc.blocks.extend(parse("Following paragraph").blocks);
                let style=crate::export_styles::sanitize(&serde_json::json!({"typography":{"size":size}}));
                let package=build(&doc,&style,"paper",&|_|None);
                let xml=part(&package,"word/document.xml");
                crate::math_document::parse_xml(xml).unwrap();
                let mut reader=quick_xml::Reader::from_str(xml);
                let mut spacings=Vec::new();
                loop {
                    match reader.read_event().unwrap() {
                        quick_xml::events::Event::Empty(element) if element.name().as_ref()=="w:spacing" => {
                            spacings.push(element.attributes().map(|attribute|{
                                let attribute=attribute.unwrap();
                                (attribute.key.as_ref().to_owned(),attribute.value.into_owned())
                            }).collect::<std::collections::BTreeMap<_,_>>());
                        }
                        quick_xml::events::Event::Eof=>break,
                        _=>(),
                    }
                }
                assert_eq!(spacings.len(),2);
                for (index, spacing) in spacings.into_iter().enumerate() {
                    assert_eq!(spacing.get("w:line").map(String::as_str),Some(expected_line));
                    assert_eq!(spacing.get("w:lineRule").map(String::as_str),Some("auto"));
                    assert_eq!(spacing.get("w:afterLines").map(String::as_str),Some(if index == 0 { expected_after } else { "0" }));
                    assert_eq!(spacing.get("w:after").map(String::as_str),Some("0"),"The inherited point gap must not survive entry-spacing=0");
                }
                let defaults=part(&package,"word/styles.xml").split("</w:rPrDefault>").next().unwrap();
                assert!(defaults.contains(&format!("<w:sz w:val=\"{half_points}\"/>")));
                assert_eq!(xml.matches("<w:tab/>").count(),2);
                assert!(xml.contains("First entry")&&xml.contains("Second entry"));
                assert!(xml.find("Second entry").unwrap() < xml.find("Following paragraph").unwrap());
            }
        }
    }

    #[test]
    fn bibliography_labels_share_a_measured_tab_without_moving_unlabelled_entries() {
        use crate::publication_rich::{RichBibliography, RichBibliographyItem, RichFontWeight, RichFormat, RichNode};
        fn descendant<'a>(node: &'a crate::math_document::XmlNode, name: &str) -> Option<&'a crate::math_document::XmlNode> {
            if node.name == name { Some(node) } else { node.children().find_map(|child| descendant(child, name)) }
        }
        fn entry_paragraphs<'a>(node: &'a crate::math_document::XmlNode, entries: &mut Vec<&'a crate::math_document::XmlNode>) {
            if node.name=="w:p" && node.text().ends_with("body") { entries.push(node); }
            else { for child in node.children() { entry_paragraphs(child,entries); } }
        }
        // Independently read DejaVu Sans hmtx: UPEM=2048, brackets=799,
        // digits=1303. This checks the package column against those advances.
        for size in [9_f64,17.] {
            for hanging_indent in [false,true] {
                let items=vec![
                    RichBibliographyItem {key:"short".into(),first_field:vec![RichNode::text("[9]")],rich:vec![RichNode::text("Short label body")]},
                    RichBibliographyItem {key:"long".into(),first_field:vec![RichNode::Text {text:"[100]".into(),format:RichFormat {font_weight:RichFontWeight::Bold,..RichFormat::default()}}],rich:vec![RichNode::text("Long label body")]},
                    RichBibliographyItem {key:"none".into(),first_field:vec![],rich:vec![RichNode::text("Unlabelled body")]},
                ];
                let mut doc=parse("");
                doc.blocks.push(Block::Bibliography(RichBibliography {title:"References".into(),hanging_indent,line_spacing:2,entry_spacing:1,source_span:None,items}));
                let style=crate::export_styles::sanitize(&serde_json::json!({"typography":{"size":size,"font":"SimSun"}}));
                let package=build(&doc,&style,"paper",&|_|None);
                let xml=part(&package,"word/document.xml");
                let root=crate::math_document::parse_xml(xml).unwrap();
                let body=descendant(&root,"w:body").unwrap();
                let mut entries=Vec::new();
                entry_paragraphs(body,&mut entries);
                assert_eq!(entries.len(),3);
                let tab=(((2.*799.+3.*1303.)/2048.+0.75)*size*20.).ceil().max(360.) as i64;
                assert!(tab as f64 >= ((2.*799.+3.*1303.)/2048.+0.75)*size*20.);
                for entry in &entries[..2] {
                    let ind=descendant(entry,"w:ind").unwrap();
                    for key in ["w:left","w:hanging"] { assert_eq!(ind.attributes[key],tab.to_string()); }
                    assert_eq!(descendant(entry,"w:tab").unwrap().attributes["w:pos"],tab.to_string());
                }
                assert!(descendant(entries[1],"w:b").is_some(),"Measuring plain text must preserve rich output");
                let no_label=descendant(entries[2],"w:ind").unwrap();
                let original_indent=if hanging_indent {"360"} else {"0"};
                for key in ["w:left","w:hanging"] { assert_eq!(no_label.attributes[key],original_indent); }
                assert!(descendant(entries[2],"w:tab").is_some_and(|tab|tab.attributes.contains_key("w:pos")));
                assert_eq!(xml.matches("<w:tab/>").count(),2,"An unlabelled entry must not acquire a tab run");
                assert!(part(&package,"word/styles.xml").contains("w:ascii=\"SimSun\""),"Keep the original Office family request");
            }
        }
        let item=|label:&str|crate::publication_rich::RichBibliographyItem {key:"width".into(),first_field:vec![RichNode::text(label)],rich:vec![]};
        assert_eq!(bibliography_label_tab_twips(&[item("A\u{0301}")],40.),bibliography_label_tab_twips(&[item("A")],40.));
        assert_eq!(bibliography_label_tab_twips(&[item("\u{10ffff}")],40.),1400,"Unknown characters reserve one em plus the gutter");
    }

    #[test]fn odd_even_running_parts_are_native_and_explicit_empty_does_not_inherit(){
        let doc=crate::md_ast::parse("# Book\n\nBody.");
        for even in ["EVEN & proof",""]{
            let style=crate::export_styles::sanitize(&serde_json::json!({"page":{"differentOddEven":true,"differentFirstPage":true},"header":{"text":"ODD header","evenText":even},"footer":{"text":"ODD footer","evenText":even,"pageNumbers":true,"start":8}}));
            let package=build(&doc,&style,"book.md",&|_|None);
            assert!(part(&package,"word/settings.xml").contains("<w:evenAndOddHeaders/>"));assert!(part(&package,"word/header1.xml").contains("ODD header"));
            for kind in ["header","footer"]{let xml=part(&package,&format!("word/{kind}2.xml"));assert!(!xml.contains("ODD"));assert_eq!(xml.contains("EVEN &amp; proof"),!even.is_empty());assert!(part(&package,"[Content_Types].xml").contains(&format!("/word/{kind}2.xml")));assert!(part(&package,"word/_rels/document.xml.rels").contains(&format!("Target=\"{kind}2.xml\"")));assert!(part(&package,"word/document.xml").contains(&format!("<w:{kind}Reference w:type=\"even\"")));}
            assert!(part(&package,"word/footer2.xml").contains(" PAGE "));assert!(part(&package,"word/document.xml").contains("<w:titlePg/>"));
        }
        let package=build(&doc,&crate::export_styles::sanitize(&serde_json::json!({"header":{"evenText":"unused"}})),"book.md",&|_|None);assert!(!part(&package,"word/settings.xml").contains("evenAndOddHeaders"));assert!(!package.parts.iter().any(|(n,_)|n=="word/header2.xml"));
    }

    #[test]
    fn note_start_uses_native_settings_without_renumbering_reference_identity() {
        for position in ["page","document"] {
            let style=crate::export_styles::sanitize(&serde_json::json!({"footnotes":{"position":position,"start":27,"numberFormat":"upperLetter"}}));
            let package=build(&parse("Body[^a] and repeat[^a], other[^b].\n\n[^a]: A.\n\n[^b]: B.\n"),&style,"notes.md",&|_|None);
            let kind=if position=="page"{"footnote"}else{"endnote"};
            let settings=part(&package,"word/settings.xml");
            assert!(settings.contains("<w:numStart w:val=\"27\"/>"));assert!(settings.contains("<w:numFmt w:val=\"upperLetter\"/>"));
            assert_eq!(part(&package,"word/document.xml").matches(&format!("<w:{kind}Reference w:id=\"1\"/>")).count(),2);
            assert_eq!(part(&package,&format!("word/{kind}s.xml")).matches(&format!("<w:{kind} w:id=\"1\">")).count(),1);
        }
    }
    #[test]
    fn document_endnotes_are_native_unique_and_keep_their_own_relationships() {
        let source="# First\n\nBody[^n] and again[^n].\n\n# Second\n\nLater[^n].\n\n[^n]: Preserve FootnoteText as author content, [evidence](https://example.test/research), and $x^2$.\n\n    > Quoted evidence.\n\n    3. Ordered evidence.\n\n    |A|B|\n    |-|-|\n    |1|2|\n";
        let style=crate::export_styles::sanitize(&serde_json::json!({"footnotes":{"position":"document","numberFormat":"lowerRoman","size":8}}));
        let package=build(&parse(source),&style,"endnotes.md",&|_|None);
        assert!(!package.parts.iter().any(|(n,_)|n=="word/footnotes.xml"));
        let notes=part(&package,"word/endnotes.xml");
        assert_eq!(notes.matches("<w:endnote w:id=\"1\">").count(),1);
        assert_eq!(notes.matches("<w:endnoteRef/>").count(),1);
        assert_eq!(part(&package,"word/document.xml").matches("<w:endnoteReference w:id=\"1\"/>").count(),3);
        for value in ["FootnoteText as author content","Quoted evidence.","<w:tbl>","<w:numPr>","<m:oMath","w:pStyle w:val=\"EndnoteText\""]{assert!(notes.contains(value),"missing {value}");}
        let rels=part(&package,"word/_rels/endnotes.xml.rels");assert!(rels.contains("https://example.test/research")&&rels.contains("TargetMode=\"External\""));
        assert!(part(&package,"word/_rels/document.xml.rels").contains("relationships/endnotes"));
        assert!(part(&package,"[Content_Types].xml").contains("wordprocessingml.endnotes+xml"));
        let settings=part(&package,"word/settings.xml");assert!(settings.contains("<w:endnotePr>")&&settings.contains("<w:pos w:val=\"docEnd\"/>")&&settings.contains("<w:numFmt w:val=\"lowerRoman\"/>"));
        assert!(!settings.contains("<w:footnotePr>"));
    }

    #[test]
    fn structured_notes_keep_quotes_tables_numbering_code_and_math() {
        let source = "Body[^n].\n\n[^n]: First note paragraph.\n\n    > Quoted evidence **strong**.\n\n    3. Ordered evidence.\n    4. Second item.\n\n    | Measure | Value |\n    |---|---:|\n    | Accuracy | 98 |\n\n    ```rust\n    let evidence = 98;\n    ```\n\n    $$x^2$$\n";
        let package = build(&parse(source), &crate::export_styles::sanitize(&serde_json::json!({})), "notes.md", &|_|None);
        let notes = part(&package, "word/footnotes.xml");
        for text in ["First note paragraph.", "Quoted evidence ", "Ordered evidence.", "Second item.", "Measure", "Accuracy", "evidence"] {
            assert!(notes.contains(text), "missing note content: {text}");
            assert!(!part(&package, "word/document.xml").contains(text), "note leaked into body: {text}");
        }
        assert!(notes.contains("<w:tbl>"));
        assert!(notes.contains("<w:numPr>"));
        assert!(notes.contains("<w:pStyle w:val=\"Quote\"/>"));
        assert!(notes.contains("<w:pStyle w:val=\"Code\"/>"));
        assert!(notes.contains("<m:oMath"));
        assert_eq!(notes.matches("<w:footnoteRef/>").count(), 1);
        assert!(part(&package, "word/numbering.xml").contains("<w:startOverride w:val=\"3\"/>"));
    }

    #[test]
    fn locally_styled_note_paragraphs_do_not_disappear_or_change_unstyled_notes() {
        let source = "Body[^n] and other[^other].\n\n[^n]: Local evidence.<!-- readmd:layout v1 {\"id\":\"block-0123456789abcdef01234567\",\"text\":{\"size\":14},\"paragraph\":{\"align\":\"right\"}} -->\n\n[^other]: Other evidence.\n";
        let style = crate::export_styles::sanitize(&serde_json::json!({"footnotes":{"size":8,"lineHeight":1.2},"typography":{"size":12,"lineHeight":1.8}}));
        let package = build(&parse(source), &style, "notes.md", &|_|None);
        let notes = part(&package, "word/footnotes.xml");
        assert!(notes.contains("Local evidence."));
        assert!(notes.contains("w:sz w:val=\"28\""));
        assert!(notes.contains("w:jc w:val=\"right\""));
        let other = notes.split("<w:footnote w:id=\"2\">").nth(1).unwrap().split("</w:footnote>").next().unwrap();
        assert!(other.contains("Other evidence."));
        assert!(!other.contains("w:val=\"28\""));
        assert!(!notes.contains("readmd:layout"));
    }

    #[test]
    fn repeated_notes_reuse_definitions_and_invalid_nested_references_are_detected() {
        let doc = parse("Body[^a], again[^a], and other[^b].\n\n[^a]: First note.\n\n[^b]: Second.\n");
        let package = build(&doc, &serde_json::json!({}), "notes.md", &|_|None);
        let body = part(&package, "word/document.xml");
        let notes = part(&package, "word/footnotes.xml");
        assert_eq!(body.matches("<w:footnoteReference w:id=\"1\"/>").count(), 2);
        assert_eq!(body.matches("<w:footnoteReference w:id=\"2\"/>").count(), 1);
        assert_eq!(notes.matches("<w:footnote w:id=\"1\">").count(), 1);
        assert_eq!(notes.matches("<w:footnote w:id=\"2\">").count(), 1);
        assert_eq!(notes.matches("First note.").count(), 1);
        assert_eq!(notes.matches("Second.").count(), 1);
        assert!(!notes.contains("<w:footnoteReference "));
        let cyclic = build(&parse("Cycle[^a].\n\n[^a]: A[^b].\n\n[^b]: B[^a].\n"), &serde_json::json!({}), "cycle.md", &|_|None);
        let notes = part(&cyclic, "word/footnotes.xml");
        assert_eq!(notes.matches("<w:footnote w:id=\"1\">").count(), 1);
        assert_eq!(notes.matches("<w:footnote w:id=\"2\">").count(), 0);
        assert!(!notes.contains("<w:footnoteReference "));
        assert!(cyclic.warns.iter().any(|code| code == crate::api_codes::DOCX_NESTED_FOOTNOTE));
    }

    #[test]
    fn note_links_and_cached_images_have_part_local_relationships_without_duplicate_media() {
        let doc = parse("# Target\n\n![body](shared.png) [body-only](https://body.invalid)[^n] and again[^n].\n\n[^n]: [source](https://source.invalid?a=1&b=2), [jump](#target), ![note](shared.png).\n");
        let package = build(&doc, &serde_json::json!({}), "note-resources.md", &|_|Some(("png".into(),png_1x1())));
        let notes = part(&package, "word/footnotes.xml");
        let rels = part(&package, "word/_rels/footnotes.xml.rels");
        assert!(rels.contains("https://source.invalid?a=1&amp;b=2"));
        assert!(rels.contains("Target=\"media/image1.png\""));
        assert!(!rels.contains("body.invalid") && !rels.contains("styles.xml"));
        assert!(notes.contains(&format!("w:anchor=\"{}\"", bookmark_name("target"))) && notes.contains("xmlns:wp="));
        assert_eq!(package.parts.iter().filter(|(p,_)|p.starts_with("word/media/")).count(),1);
        let root = crate::math_document::parse_xml(notes).unwrap();
        fn refs(node:&crate::math_document::XmlNode, into:&mut Vec<String>) {
            for (key,value) in &node.attributes { if node.attribute_namespaces.get(key).is_some_and(|ns|ns==R_NS) {into.push(value.clone());} }
            for child in node.children() { refs(child,into); }
        }
        let mut referenced=Vec::new();refs(&root,&mut referenced);assert!(!referenced.is_empty());
        for id in referenced {assert!(rels.contains(&format!("Id=\"{id}\"")),"unresolved footnote relationship {id}");}
    }

    #[test]
    fn math_document_defaults_merge_limits_and_reject_incompatible_fonts_before_parts() {
        let expression = |font: &str, location: &str| format!("row(office.mathPr({font}office.naryLim(val: {location})), office.oMath(sum(x, from: 0, to: 1)))");
        let block = |s: String| format!("```readmd-math v1\n{s}\n```\n\n");
        for with_conflict in [false, true] {
            let font = if with_conflict { "office.mathFont(val: \"Cambria Math\"), " } else { "" };
            let markdown = block(expression(font, "undOvr")) + &block(expression("", "subSup")) + ":math[sum(x, from: 0, to: 1)]";
            let doc = crate::md_ast::parse(&markdown);
            let package = build(&doc, &serde_json::json!({}), "merged.md", &|_|None);
            if with_conflict {
                assert!(package.parts.is_empty());
                assert_eq!(package.warns.len(), 1);
                let details: Value = serde_json::from_str(package.warns[0].strip_prefix("math_document_defaults_conflict: ").unwrap()).unwrap();
                assert_eq!(details, serde_json::json!({"property":"mathFont","firstFormula":1,"formula":2}));
                continue;
            }
            assert!(!package.parts.is_empty(), "{:?}", package.warns);
            assert!(!package.warns.iter().any(|warning| warning.starts_with("math_document_defaults_conflict")));
            let body = part(&package, "word/document.xml");
            assert!(!body.contains("mathPr"));
            assert_eq!(body.matches("m:val=\"undOvr\"").count(), 1);
            assert_eq!(body.matches("m:val=\"subSup\"").count(), 2);
            assert_eq!(part(&package, "word/settings.xml").matches("<m:mathPr").count(), 1);
            for (_, bytes) in &package.parts { if bytes.starts_with(b"<?xml") { crate::math_document::parse_xml(&String::from_utf8_lossy(bytes)).unwrap(); } }
        }
    }

    fn build_rich(style: &Value) -> DocxPackage {
        let img = png_1x1();
        let loader = move |src: &str| if src == "a.png" { Some(("png".to_string(), img.clone())) } else { None };
        build(&parse(RICH), style, "doc", &loader)
    }

    #[test]
    fn structure_is_preserved() {
        let st = crate::export_styles::sanitize_options(None);
        let pkg = build_rich(&st);
        let d = part(&pkg, "word/document.xml");
        assert!(d.contains("<w:pStyle w:val=\"Heading1\"/>"));
        assert!(d.contains("<w:pStyle w:val=\"Heading5\"/>"));
        assert!(d.contains("<w:i/>") && d.contains("<w:b/>") && d.contains("<w:strike/>"));
        assert!(d.contains("<w:rStyle w:val=\"CodeChar\"/>"));
        assert!(d.contains("<w:hyperlink r:id="), "external link");
        assert!(d.contains(&format!("w:anchor=\"{}\"", bookmark_name("第一章"))), "internal link");
        assert!(d.contains(&format!("w:name=\"{}\"", bookmark_name("第一章"))), "bookmark");
        assert!(d.matches("<w:numPr>").count() >= 3, "list numbering");
        assert!(d.contains("\u{2611}"), "task checkbox");
        assert!(d.contains("<w:pStyle w:val=\"Quote\"/>"));
        assert!(d.contains("<w:jc w:val=\"left\"/>") && d.contains("<w:jc w:val=\"center\"/>") && d.contains("<w:jc w:val=\"right\"/>"));
        assert!(d.contains("<w:tblHeader/>"));
        assert!(d.contains("<w:pStyle w:val=\"Code\"/>") && d.contains(">rust<"));
        assert!(d.contains("<m:oMathPara") && d.contains("<m:oMath"));
        assert!(d.contains("<w:footnoteReference w:id=\"1\"/>"));
        assert!(d.contains("<w:drawing>"));
        assert!(d.contains("<w:pBdr><w:bottom"), "hr");
        assert!(part(&pkg, "word/footnotes.xml").contains("脚注内容"));
        assert!(part(&pkg, "word/_rels/document.xml.rels").contains("https://example.com?a=1&amp;b=2"));
        assert!(pkg.parts.iter().any(|(n, _)| n == "word/media/image1.png"));
        assert!(part(&pkg, "docProps/core.xml").contains("<dc:title>报告</dc:title>"));
        // ordered list keeps a numbering instance with its start
        assert!(part(&pkg, "word/numbering.xml").contains("<w:startOverride w:val=\"1\"/>"));
    }

    #[test]
    fn scientific_tables_have_only_three_horizontal_rules_and_repeat_headers() {
        let source="| Measurement | Value |\n|---|---:|\n| Mean | 12.4 |\n| 95% CI | 10.9–13.9 |\n";
        let style=crate::export_styles::sanitize(&serde_json::json!({"table":{"style":"three-line","borderWidth":0.5}}));
        let pkg=build(&parse(source),&style,"Research",&|_|None);let xml=part(&pkg,"word/document.xml");
        for side in ["left","right","insideH","insideV"]{assert!(xml.contains(&format!("<w:{side} w:val=\"nil\"/>")));}
        assert!(xml.contains("<w:tblHeader/>"));assert_eq!(xml.matches("<w:tcBorders>").count(),2);assert!(!xml.contains("<w:shd"));
        assert!(xml.contains("<w:top w:val=\"single\" w:sz=\"8\""));assert!(xml.contains("<w:bottom w:val=\"single\" w:sz=\"4\""));
        let grid=build(&parse(source),&crate::export_styles::sanitize(&serde_json::json!({})),"Grid",&|_|None);
        assert!(part(&grid,"word/document.xml").contains("<w:insideV w:val=\"single\""));
    }

    #[test]
    fn toc_depth_and_leader_change_native_word_fields_and_styles() {
        let doc=crate::md_ast::parse("# One\n\n## Two\n\n###### Six\n");
        for leader in crate::publication_layout::TOC_LEADERS {
            let style=crate::export_styles::sanitize(&serde_json::json!({"toc":{"enabled":true,"depth":6,"leader":leader},"page":{"size":"A4","marginLeft":20,"marginRight":25,"gutter":5}}));
            let package=build(&doc,&style,"TOC",&|_|None);
            let document=part(&package,"word/document.xml");
            assert!(document.contains("TOC \\o \"1-6\""));
            let styles=part(&package,"word/styles.xml");
            assert!(styles.contains("w:styleId=\"TOC6\""));
            assert_eq!(styles.matches(&format!("w:leader=\"{leader}\" w:pos=\"9071\"")).count(),6);
            assert!(!styles.contains("w:styleId=\"TOC7\""));
        }
    }
    #[test]
    fn bookmarks_preserve_exact_case_punctuation_and_unicode_ids() {
        let source = "# First {#API_V1}\n\n# Second {#API-V1}\n\n# Greek {#α}\n\n# Digits {#3b1}\n\n[One](#API_V1) [Two](#API-V1) [Three](#%CE%B1) [Four](#3b1)\n";
        let doc = parse(source);
        let pkg = build(&doc, &crate::export_styles::sanitize_options(None), "doc", &|_| None);
        let xml = part(&pkg, "word/document.xml");
        let mut unique = std::collections::HashSet::new();
        for id in ["API_V1", "API-V1", "α", "3b1"] {
            let name = bookmark_name(id);
            assert!(name.len() <= 40 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
            assert!(unique.insert(name.clone()));
            assert!(xml.contains(&format!("w:name=\"{name}\"")));
            assert!(xml.contains(&format!("w:anchor=\"{name}\"")));
        }
    }

    #[test]
    fn style_options_are_applied() {
        let st = crate::export_styles::sanitize_options(Some(&serde_json::json!({
            "page": {"size": "A4", "orientation": "landscape", "marginTop": 25},
            "typography": {"font": "SimSun", "size": 12, "color": "#112233"},
            "headings": {"h1": {"size": 24, "color": "#ff0000", "align": "center"}},
            "table": {"headerBg": "#00ff00"},
            "header": {"text": "页眉"},
            "footer": {"text": "页脚", "pageNumbers": true},
            "cover": {"enabled": true, "subtitle": "副标题"},
            "toc": {"enabled": true},
        })));
        let pkg = build_rich(&st);
        let d = part(&pkg, "word/document.xml");
        assert!(d.contains("w:orient=\"landscape\""));
        assert!(d.contains(&format!("w:top=\"{}\"", mm_twips(25.0))));
        assert!(d.contains("w:fill=\"00FF00\""));
        assert!(d.contains("TOC \\o"));
        assert!(d.contains("<w:pStyle w:val=\"Title\"/>") && d.contains("副标题"));
        assert!(d.contains("<w:titlePg/>"));
        let s = part(&pkg, "word/styles.xml");
        assert!(s.contains("w:ascii=\"SimSun\""));
        assert!(s.contains("<w:sz w:val=\"24\"/>"));
        assert!(s.contains("<w:color w:val=\"112233\"/>"));
        assert!(s.contains("<w:sz w:val=\"48\"/><w:szCs w:val=\"48\"/><w:color w:val=\"FF0000\"/>"));
        assert!(part(&pkg, "word/header1.xml").contains("页眉"));
        let f = part(&pkg, "word/footer1.xml");
        assert!(f.contains("页脚") && f.contains(" PAGE "));
    }

    #[test]
    fn xml_escape_drops_control_chars() {
        assert_eq!(xml_escape("a\u{1}b<&>\"\u{b}"), "ab&lt;&amp;&gt;&quot;");
    }

    #[test]
    fn publication_controls_write_native_sections_fields_styles_and_column_sized_images() {
        let style=crate::export_styles::sanitize(&serde_json::json!({
            "page":{"columns":2,"columnGap":6,"gutter":5,"mirrorMargins":true},
            "cover":{"enabled":true},"toc":{"enabled":true},
            "header":{"text":"RUNNING_HEADER","distance":8,"size":8},
            "footer":{"start":7,"numberFormat":"upperRoman","showTotal":true,"distance":9,"size":10,"align":"right"},
            "typography":{"widowControl":false,"keepLines":true},
            "headings":{"h2":{"keepNext":false,"keepLines":false}},
            "footnotes":{"size":8,"lineHeight":1.5,"numberFormat":"lowerLetter"}
        }));
        let pkg=build_rich(&style);let document=part(&pkg,"word/document.xml");
        let sections=document.split("<w:sectPr>").skip(1).map(|s|s.split("</w:sectPr>").next().unwrap()).collect::<Vec<_>>();
        assert_eq!(sections.len(),2);assert!(sections[0].contains("w:num=\"1\""));assert!(sections[0].contains("w:start=\"7\""));assert!(sections[0].contains("<w:titlePg/>"));
        assert!(sections[1].contains("w:num=\"2\""));assert!(!sections[1].contains("w:start="));assert!(!sections[1].contains("<w:titlePg/>"));
        assert!(sections[1].contains(&format!("w:gutter=\"{}\"",mm_twips(5.))));assert!(sections[1].contains(&format!("w:space=\"{}\"",mm_twips(6.))));
        assert!(part(&pkg,"word/settings.xml").contains("<w:mirrorMargins/>"));assert!(part(&pkg,"word/settings.xml").contains("<w:numFmt w:val=\"lowerLetter\"/>"));
        let styles=part(&pkg,"word/styles.xml");assert!(styles.contains("<w:widowControl w:val=\"0\"/>"));assert!(styles.contains("<w:keepLines w:val=\"1\"/><w:ind"));
        let h2=styles.split("w:styleId=\"Heading2\"").nth(1).unwrap().split("</w:style>").next().unwrap();assert!(h2.contains("<w:keepNext w:val=\"0\"/><w:keepLines w:val=\"0\"/>"));
        let note=styles.split("w:styleId=\"FootnoteText\"").nth(1).unwrap().split("</w:style>").next().unwrap();assert!(note.contains("w:line=\"360\""));assert!(note.contains("w:sz w:val=\"16\""));
        let footer=part(&pkg,"word/footer1.xml");assert!(footer.contains(" PAGE "));assert!(footer.contains(" NUMPAGES "));assert!(footer.contains("w:val=\"right\""));
        let max_width=(crate::publication_layout::Layout::from_style(&style).column_width(&style)*36000.*0.92) as i64;
        let width=document.split("<wp:extent cx=\"").nth(1).unwrap().split('"').next().unwrap().parse::<i64>().unwrap();assert!(width<=max_width);
    }

    #[test]
    fn image_sizes() {
        assert_eq!(image_size(&png_1x1()), Some((200, 100)));
        let gif = b"GIF89a\x10\x00\x20\x00".to_vec();
        assert_eq!(image_size(&gif), Some((16, 32)));
    }

    /// Writes a sample `.docx` for manual inspection when
    /// `READMD_DOCX_SAMPLE=<path>` is set.
    #[test]
    fn write_sample_when_requested() {
        let Ok(path) = std::env::var("READMD_DOCX_SAMPLE") else { return };
        let opts: Value = std::env::var("READMD_DOCX_SAMPLE_OPTS")
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| serde_json::json!({"header": {"text": "ReadMD"}}));
        let st = crate::export_styles::sanitize_options(Some(&opts));
        let pkg = build_rich(&st);
        let entries: Vec<crate::mdexport::ZipEntry> = pkg
            .parts
            .into_iter()
            .map(|(path, data)| crate::mdexport::ZipEntry { path, data, compress: true })
            .collect();
        std::fs::write(path, crate::mdexport::write_zip(&entries).unwrap()).unwrap();
    }
}
