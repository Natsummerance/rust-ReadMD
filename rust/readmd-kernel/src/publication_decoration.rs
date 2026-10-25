//! Opt-in shading and per-side borders for ordinary body paragraphs.
//! DOCX writes native `w:pBdr`/`w:shd` inside `w:pPr`; the built-in PDF paints
//! a fill rectangle and border bars *under* the paragraph's line boxes. Every
//! other consumer (Typst/TeX/EPUB/HTML, reader view, HTML preview) rejects a
//! decorated source before any resource, staging or output byte is touched.
use serde_json::Value;

pub(crate) const INVALID: &str = super::INVALID;
pub(crate) const UNSUPPORTED: &str = crate::api_codes::PUBLICATION_PARAGRAPH_DECORATION_UNSUPPORTED;

/// One declared border edge. Source values stay exact; only the DOCX boundary
/// quantises the width to Word's eighths-of-a-point (`w:sz`), rounded half up.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BorderEdge {
    pub(crate) color: String,
    pub(crate) width_pt: f64,
    pub(crate) space_pt: i64,
}
impl BorderEdge {
    /// `w:sz` is in eighths of a point: 2.5pt→20, 2.6pt→21. The rounding rule
    /// is fixed by contract; tests lock it in both directions.
    pub(crate) fn docx_eighths(&self) -> i64 { (self.width_pt * 8.).round() as i64 }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Decoration {
    pub(crate) fill: Option<String>,
    pub(crate) top: Option<BorderEdge>,
    pub(crate) left: Option<BorderEdge>,
    pub(crate) bottom: Option<BorderEdge>,
    pub(crate) right: Option<BorderEdge>,
}
impl Decoration {
    pub(crate) fn is_empty(&self) -> bool {
        self.fill.is_none()
            && self.top.is_none() && self.left.is_none()
            && self.bottom.is_none() && self.right.is_none()
    }
    /// `w:pPr` fragment in CT_PPrBase order (`pBdr` precedes `shd`) with the
    /// CT_PBdr edge sequence top/left/bottom/right. `w:shd` keeps Word's
    /// canonical clear/auto form; the fill stays the author's exact hex text.
    #[allow(non_snake_case)] // OOXML spelling, like the writer's existing XML helpers.
    pub(crate) fn docx_pPr(&self) -> String {
        let mut result = String::new();
        if !(self.top.is_none() && self.left.is_none() && self.bottom.is_none() && self.right.is_none()) {
            result.push_str("<w:pBdr>");
            for (side, edge) in
                [("top", &self.top), ("left", &self.left), ("bottom", &self.bottom), ("right", &self.right)]
            {
                if let Some(edge) = edge {
                    result.push_str(&format!(
                        "<w:{side} w:val=\"single\" w:sz=\"{}\" w:space=\"{}\" w:color=\"{}\"/>",
                        edge.docx_eighths(), edge.space_pt, edge.color
                    ));
                }
            }
            result.push_str("</w:pBdr>");
        }
        if let Some(fill) = &self.fill {
            result.push_str(&format!("<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{fill}\"/>"));
        }
        result
    }
}

/// Exactly six ASCII hexadecimal digits — a bare `RRGGBB`, never `#RRGGBB`.
fn hex6(value: &Value) -> Result<String, &'static str> {
    let text = value.as_str().ok_or(INVALID)?;
    if text.len() == 6 && text.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(text.to_string())
    } else {
        Err(INVALID)
    }
}

/// A strict edge object: exactly `color`/`widthPt`/`spacePt`, no unknown keys,
/// no nulls. `widthPt` ∈ [0.25,12.0]; `spacePt` is an integer in [0,31].
fn parse_edge(value: &Value) -> Result<BorderEdge, &'static str> {
    let object = value.as_object().ok_or(INVALID)?;
    if object.len() != 3 {
        return Err(INVALID);
    }
    let width = object.get("widthPt").ok_or(INVALID)?.as_f64()
        .filter(|n| n.is_finite() && (0.25..=12.).contains(n)).ok_or(INVALID)?;
    let space = object.get("spacePt").ok_or(INVALID)?.as_i64()
        .filter(|n| (0..=31).contains(n)).ok_or(INVALID)?;
    Ok(BorderEdge { color: hex6(object.get("color").ok_or(INVALID)?)?, width_pt: width, space_pt: space })
}

/// The single parse point for both new `paragraph` keys. `None` means the
/// paragraph carries no (effective) decoration; an empty `{}` left behind by a
/// sparse reset is not a declaration.
pub(crate) fn parse(paragraph: &Value) -> Result<Option<Decoration>, &'static str> {
    // A present-but-null key is denied like any other malformed value; only a
    // genuinely absent key (or a fully emptied object after a sparse reset)
    // means "no decoration".
    if paragraph.get("shading").is_none() && paragraph.get("border").is_none() {
        return Ok(None);
    }
    let mut decoration = Decoration::default();
    if let Some(value) = paragraph.get("shading") {
        let object = value.as_object().ok_or(INVALID)?;
        for (key, value) in object {
            if key == "fill" {
                decoration.fill = Some(hex6(value)?);
            } else {
                return Err(INVALID);
            }
        }
    }
    if let Some(value) = paragraph.get("border") {
        let object = value.as_object().ok_or(INVALID)?;
        for (key, value) in object {
            let edge = parse_edge(value)?;
            match key.as_str() {
                "top" => decoration.top = Some(edge),
                "left" => decoration.left = Some(edge),
                "bottom" => decoration.bottom = Some(edge),
                "right" => decoration.right = Some(edge),
                _ => return Err(INVALID),
            }
        }
    }
    if decoration.is_empty() { return Ok(None); }
    Ok(Some(decoration))
}

/// Adapter for the `validate_style` match table: validate one raw
/// `("paragraph", "shading"|"border")` entry through the same single parser.
pub(crate) fn parse_key(key: &str, value: &Value) -> Result<Option<Decoration>, &'static str> {
    parse(&serde_json::json!({ key: value }))
}

/// Did the parsed source declare any paragraph decoration? Substrings inside
/// code examples or prose are not declarations and never count here.
pub(crate) fn used(source: &str) -> Result<bool, &'static str> {
    if !source.contains(crate::publication_blocks::PREFIX) { return Ok(false); }
    crate::publication_blocks::validate_source(source)?;
    let mut used = false;
    for node in &crate::publication_blocks::analyze(source)?.nodes {
        used |= parse(&node.style["paragraph"])?.is_some();
    }
    Ok(used)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Consumer { Docx, RustPdf, Unsupported }

/// Source preflight shares the md_ast the writers will consume. The context
/// rule is the `warnings()` ordinary/container semantics: a decoration lives on
/// an ordinary body paragraph only, never inside quote/list/table/code/footnote/
/// bibliography stories (those parents mark their children non-ordinary).
pub(crate) fn validate_source_consumer(consumer: Consumer, source: &str) -> Result<(), &'static str> {
    if !source.contains(crate::publication_blocks::PREFIX) { return Ok(()); }
    crate::publication_blocks::validate_source(source)?;
    validate_document(&crate::md_ast::parse(source), consumer)
}

pub(crate) fn validate_document(doc: &crate::md_ast::Document, consumer: Consumer) -> Result<(), &'static str> {
    use crate::md_ast::{Block, Inline};
    let mut pending = doc.blocks.iter().map(|block| (block, true)).collect::<Vec<_>>();
    let mut declarations = 0;
    while let Some((block, ordinary)) = pending.pop() {
        if crate::speech::cancelled() { return Err("cancelled"); }
        match block {
            Block::Styled { attributes, block } => {
                if parse(&attributes["paragraph"])?.is_some() {
                    declarations += 1;
                    if declarations > crate::publication_blocks::MAX_NODES { return Err(INVALID); }
                    // Target guard mirrors validate_target: headings, captions
                    // and math never carry a decoration, even via direct ASTs.
                    if !matches!(block.unstyled(), Block::Paragraph(_)) { return Err(INVALID); }
                    if consumer == Consumer::Unsupported || !ordinary { return Err(UNSUPPORTED); }
                    // The core PDF adapter projects an image-only paragraph as
                    // a bare image flowable with no paragraph box to paint on.
                    if consumer == Consumer::RustPdf
                        && matches!(block.unstyled(), Block::Paragraph(values)
                            if values.len() == 1 && matches!(values.first(), Some(Inline::Image { .. })))
                    { return Err(UNSUPPORTED); }
                }
                pending.push((block.as_ref(), ordinary));
            },
            Block::Quote(children) | Block::FootnoteDef { blocks: children, .. } =>
                pending.extend(children.iter().map(|block| (block, false))),
            Block::List { items, .. } => for item in items {
                pending.extend(item.blocks.iter().map(|block| (block, false)));
            },
            _ => (),
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "publication_decoration_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "publication_decoration_docx_tests.rs"]
mod docx_tests;

#[cfg(test)]
#[path = "publication_decoration_pdf_tests.rs"]
mod pdf_tests;
