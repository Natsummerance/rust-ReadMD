//! Physical ordinary-text baseline offsets. Positive points raise text.
//! The source value stays exact; only the DOCX boundary quantises to 1/2pt.
use serde_json::Value;

pub(crate) const INVALID: &str = super::INVALID;
pub(crate) const UNSUPPORTED: &str = crate::api_codes::PUBLICATION_BASELINE_UNSUPPORTED;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BaselineShiftPt(f64);
impl BaselineShiftPt {
    pub(crate) fn requested_pt(self) -> f64 { self.0 }
    pub(crate) fn docx_half_points(self) -> i32 { (self.0 * 2.).round() as i32 }
    pub(crate) fn docx_actual_pt(self) -> f64 { self.docx_half_points() as f64 / 2. }
}
pub(crate) fn parse(value: &Value) -> Result<BaselineShiftPt, &'static str> {
    value.as_f64().filter(|n| n.is_finite() && (-72. ..=72.).contains(n))
        .map(BaselineShiftPt).ok_or(INVALID)
}
pub(crate) fn global(options: &Value) -> Result<Option<BaselineShiftPt>, &'static str> {
    options["typography"].get("baselineShiftPt").filter(|v| !v.is_null()).map(parse).transpose()
}
pub(crate) fn local(text: &Value) -> Result<Option<BaselineShiftPt>, &'static str> {
    text.get("baselineShiftPt").map(parse).transpose()
}
pub(crate) fn validate_request(options: &Value) -> Result<(), &'static str> { global(options).map(|_| ()) }
pub(crate) fn used(source: &str, options: &Value) -> Result<bool, &'static str> {
    let mut used=global(options)?.is_some();
    if source.contains(crate::publication_blocks::PREFIX) {
        crate::publication_blocks::validate_source(source)?;
        used |= crate::publication_blocks::analyze(source)?.nodes.iter()
            .any(|node| node.style["text"].get("baselineShiftPt").is_some());
    }
    Ok(used)
}
/// This chooses the mapped compilation pipeline, not permission to consume a
/// declaration. Validation still runs at the public consumer boundary. A zero
/// reset needs no annotation rewrite; a possibly inherited nonzero value does.
pub(crate) fn nonzero_used(source: &str, options: &Value) -> Result<bool, &'static str> {
    let mut nonzero = global(options)?.is_some_and(|shift| shift.requested_pt() != 0.);
    if source.contains(crate::publication_blocks::PREFIX) {
        crate::publication_blocks::validate_source(source)?;
        for node in crate::publication_blocks::analyze(source)?.nodes {
            nonzero |= local(&node.style["text"])?.is_some_and(|shift| shift.requested_pt() != 0.);
        }
    }
    Ok(nonzero)
}
pub(crate) fn resolve(inherited: Option<BaselineShiftPt>, text: &Value, inherit_body: bool)
    -> Result<Option<BaselineShiftPt>, &'static str>
{
    if let Some(value) = local(text)? { return Ok(Some(value)); }
    Ok(if inherit_body { inherited } else { None })
}
pub(crate) fn inherit_null_overlay(overlay: &mut Value) {
    if overlay.pointer("/typography/baselineShiftPt").is_some_and(Value::is_null) {
        if let Some(values) = overlay.get_mut("typography").and_then(Value::as_object_mut) {
            values.remove("baselineShiftPt");
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Consumer { Docx, RustPdf, Typst, Reader, Unsupported }

pub(crate) fn validate_source(source: &str, options: &Value, consumer: Consumer) -> Result<(), &'static str> {
    validate_request(options)?;
    if !source.contains(crate::publication_blocks::PREFIX) && global(options)?.is_none() { return Ok(()); }
    crate::publication_blocks::validate_source(source)?;
    validate_document(&crate::md_ast::parse(source), options, consumer)?;
    if matches!(consumer, Consumer::RustPdf | Consumer::Typst) { validate_source_citations(source,options)?; }
    Ok(())
}
fn validate_source_citations(source:&str,options:&Value)->Result<(),&'static str>{
    let parent=global(options)?;
    if !used(source,options)? { return Ok(()); }
    // This existing bounded scanner skips code/math/URLs and performs no file
    // I/O. Keep a lexical boundary before CSL reads resources or stages output.
    let scan=crate::publication::citations::scan(source,"en").map_err(|_|UNSUPPORTED)?;
    if scan.citations.is_empty(){return Ok(());}
    let nodes=crate::publication_blocks::analyze(source)?.nodes;
    for node in nodes.iter().filter(|node|!node.nested&&matches!(node.kind.as_str(),"paragraph"|"heading"|"caption")){
        let effective=resolve(parent,&node.style["text"],node.kind=="paragraph")?;
        if effective.is_some_and(|value|value.requested_pt()!=0.)&&scan.citations.iter()
            .any(|citation|citation.span.from<node.to&&citation.span.to>node.from){return Err(UNSUPPORTED);}
    }
    Ok(())
}

/// First physical lane: body paragraphs inherit; headings/captions can opt in
/// locally. Dedicated code/table/quote/list/note/CSL stories keep their defaults.
/// Unsupported local stories fail, including explicit zero, before resources.
pub(crate) fn validate_document(doc: &crate::md_ast::Document, options: &Value, consumer: Consumer)
    -> Result<(), &'static str>
{
    use crate::md_ast::{Block, Inline};
    let inherited = global(options)?;
    if inherited.is_some() && matches!(consumer, Consumer::Reader | Consumer::Unsupported) { return Err(UNSUPPORTED); }
    let mut pending = doc.blocks.iter().map(|block| (block, true, None)).collect::<Vec<_>>();
    let mut count = 0;
    while let Some((block, ordinary, parent)) = pending.pop() {
        if crate::speech::cancelled() { return Err("cancelled"); }
        match block {
            Block::Styled { attributes, block } => {
                let value = local(&attributes["text"])?;
                if value.is_some() {
                    // This validator owns the opt-in baseline contract. Direct
                    // legacy writers may preserve unsupported/unknown local
                    // properties while warning; no new field must not turn
                    // that existing text-preserving path into an error page.
                    crate::publication_blocks::validate_style(attributes)?;
                    count += 1;
                    if count > crate::publication_blocks::MAX_NODES { return Err(INVALID); }
                    if matches!(block.unstyled(), Block::Math(_) | Block::OfficeMath { .. }) { return Err(INVALID); }
                    let supported = matches!(block.unstyled(), Block::Paragraph(_) | Block::Heading { .. } | Block::Caption { .. });
                    if !ordinary || !supported || consumer == Consumer::Unsupported
                        || (consumer == Consumer::Typst && !matches!(block.unstyled(), Block::Paragraph(_)))
                    { return Err(UNSUPPORTED); }
                    if matches!(block.unstyled(), Block::Paragraph(values) if values.len()==1 && matches!(values.first(), Some(Inline::Image { .. }))) { return Err(UNSUPPORTED); }
                }
                pending.push((block, ordinary, value.or(parent)));
            },
            Block::Paragraph(values) | Block::Heading { inlines: values, .. } | Block::Caption { inlines: values, .. } => {
                let effective = parent.or(if ordinary && matches!(block, Block::Paragraph(_)) { inherited } else { None });
                // Word's native position + semantic vertAlign interaction has
                // not passed the real Word gate. Refuse that combination rather
                // than flattening superscript/subscript into ordinary text.
                if consumer == Consumer::Docx && effective.is_some_and(|value| value.requested_pt()!=0.) {
                    validate_docx_inlines(values)?;
                }
                if consumer == Consumer::RustPdf && effective.is_some_and(|value|value.requested_pt()!=0.) { validate_pdf_inlines(values)?; }
                if consumer == Consumer::Typst && effective.is_some_and(|value|value.requested_pt()!=0.) {
                    validate_typst_inlines(values)?;
                }
            },
            Block::Quote(children) | Block::FootnoteDef { blocks: children, .. } =>
                pending.extend(children.iter().map(|block| (block, false, None))),
            Block::List { items, .. } => for item in items { pending.extend(item.blocks.iter().map(|block| (block, false, None))); },
            _ => (),
        }
    }
    Ok(())
}
/// Static admission does not replace actual glyph/paint, paper-edge, collision
/// and link-fragment proof in the mandatory mapped Typst compilation pipeline.
fn validate_typst_inlines(values: &[crate::md_ast::Inline]) -> Result<(), &'static str> {
    use crate::md_ast::Inline;
    let mut pending = values.iter().map(|value| (value, false)).collect::<Vec<_>>();
    let mut count = 0usize;
    while let Some((value, inside_link)) = pending.pop() {
        if crate::speech::cancelled() { return Err("cancelled"); }
        count += 1;
        if count > 100_000 { return Err(UNSUPPORTED); }
        match value {
            Inline::Text(_) | Inline::SoftBreak | Inline::HardBreak => (),
            Inline::Strong(children) | Inline::Emph(children) | Inline::AuthorStyled { children, .. } =>
                pending.extend(children.iter().map(|value| (value, inside_link))),
            Inline::Styled { format, children } => {
                if format.vertical_align != crate::publication_rich::RichVerticalAlign::Baseline
                    || format.text_decoration != crate::publication_rich::RichDecoration::None
                { return Err(UNSUPPORTED); }
                pending.extend(children.iter().map(|value| (value, inside_link)));
            },
            Inline::Link { children, .. } => {
                if inside_link || !typst_link_has_visible_text(children) { return Err(UNSUPPORTED); }
                pending.extend(children.iter().map(|value| (value, true)));
            },
            // Dedicated objects retain their native baseline. Every object
            // receives its own source witness, checked in the final PDF.
            Inline::Code(_) | Inline::Math(_) | Inline::OfficeMath { .. } if !inside_link => (),
            Inline::Html(value) if value.trim().is_empty() || value.trim().starts_with("<!--") => (),
            _ => return Err(UNSUPPORTED),
        }
    }
    Ok(())
}
fn typst_link_has_visible_text(values: &[crate::md_ast::Inline]) -> bool {
    use crate::md_ast::Inline;
    let mut pending = values.iter().collect::<Vec<_>>();
    let mut count = 0usize;
    while let Some(value) = pending.pop() {
        count += 1;
        if count > 100_000 || crate::speech::cancelled() { return false; }
        match value {
            Inline::Text(value) if value.chars().any(|ch| !ch.is_whitespace() && !ch.is_control()) => return true,
            Inline::Strong(children) | Inline::Emph(children) | Inline::AuthorStyled { children, .. }
                | Inline::Styled { children, .. } => pending.extend(children),
            _ => (),
        }
    }
    false
}
fn validate_pdf_inlines(values: &[crate::md_ast::Inline]) -> Result<(), &'static str> {
    use crate::md_ast::Inline;
    let mut pending = values.iter().collect::<Vec<_>>();
    while let Some(value) = pending.pop() {
        match value {
            // The legacy render adapter flattens this identity. Until its new
            // projection carries the boundary, refuse rather than shift CSL.
            Inline::Citation { .. } => return Err(UNSUPPORTED),
            Inline::Styled { children, .. } | Inline::Strong(children) | Inline::Emph(children)
                | Inline::Strike(children) | Inline::Link { children, .. } | Inline::Display { children, .. } => pending.extend(children),
            _ => (),
        }
    }
    Ok(())
}
fn validate_docx_inlines(values: &[crate::md_ast::Inline]) -> Result<(), &'static str> {
    use crate::md_ast::Inline;
    let mut pending = values.iter().collect::<Vec<_>>();
    while let Some(value) = pending.pop() {
        match value {
            Inline::Styled { format, children } => {
                if format.vertical_align != crate::publication_rich::RichVerticalAlign::Baseline { return Err(UNSUPPORTED); }
                pending.extend(children);
            },
            Inline::Strong(children) | Inline::Emph(children) | Inline::Strike(children)
                | Inline::Link { children, .. } | Inline::Display { children, .. } => pending.extend(children),
            // CSL, inline code and math retain their dedicated layout.
            _ => (),
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "publication_baseline_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "publication_baseline_docx_tests.rs"]
mod docx_tests;
#[cfg(test)]
#[path = "publication_baseline_typst_tests.rs"]
mod typst_tests;
