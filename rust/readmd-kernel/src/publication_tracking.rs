//! Opt-in ordinary body tracking. No manual codepoint spacing or math shaping.
//! DOCX writes native signed twentieth-points. Reader CSS accepts local body
//! paragraphs only; other export/physical-preview consumers still refuse it.
use serde_json::Value;
pub(crate) const INVALID: &str = super::INVALID;
// Root registers/localises this dedicated capability error before applying.
pub(crate) const UNSUPPORTED: &str = "publication_tracking_unsupported";

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TrackingPt(f64);
impl TrackingPt {
    pub(crate) fn requested_pt(self) -> f64 { self.0 }
    /// WordprocessingML CT_ShortTwipsMeasure: 1/20pt, signed. Quantise only
    /// at the native boundary; preserve the requested source value exactly.
    pub(crate) fn docx_twips(self) -> i32 { (self.0 * 20.).round() as i32 }
}
pub(crate) fn parse(value: &Value) -> Result<TrackingPt, &'static str> {
    value.as_f64().filter(|n| n.is_finite() && (-5. ..=20.).contains(n)).map(TrackingPt).ok_or(INVALID)
}
pub(crate) fn global(options: &Value) -> Result<Option<TrackingPt>, &'static str> {
    options["typography"].get("trackingPt").filter(|v| !v.is_null()).map(parse).transpose()
}
pub(crate) fn local(text: &Value) -> Result<Option<TrackingPt>, &'static str> {
    // Null is only a sparse transaction command. Persistent declarations may
    // not contain null; publication_blocks::apply removes the key instead.
    text.get("trackingPt").map(parse).transpose()
}
pub(crate) fn validate_request(options: &Value) -> Result<(), &'static str> { global(options).map(|_| ()) }
pub(crate) fn resolve(inherited: Option<TrackingPt>, text: &Value, ordinary_body: bool) -> Result<Option<TrackingPt>, &'static str> {
    if let Some(value) = local(text)? { return Ok(Some(value)); }
    Ok(if ordinary_body { inherited } else { None })
}
pub(crate) fn inherit_null_overlay(overlay: &mut Value) {
    if overlay.pointer("/typography/trackingPt").is_some_and(Value::is_null) {
        if let Some(values) = overlay.get_mut("typography").and_then(Value::as_object_mut) { values.remove("trackingPt"); }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Consumer { Docx, Reader, Unsupported }
pub(crate) fn validate_reader(source: &str, options: &Value) -> Result<(), &'static str> {
    validate_source_consumer(source, options, Consumer::Reader)
}
pub(crate) fn validate_consumer(source: &str, options: &Value, docx: bool) -> Result<(), &'static str> {
    validate_source_consumer(source, options, if docx { Consumer::Docx } else { Consumer::Unsupported })
}
fn validate_source_consumer(source: &str, options: &Value, consumer: Consumer) -> Result<(), &'static str> {
    validate_request(options)?;
    if !source.contains(crate::publication_blocks::PREFIX) {
        return if global(options)?.is_some() && consumer != Consumer::Docx { Err(UNSUPPORTED) } else { Ok(()) };
    }
    // Parse escaped JSON field names too: substring searches for trackingPt
    // are not a validation boundary and must never bypass format capability.
    crate::publication_blocks::validate_source(source)?;
    validate_document_consumer(&crate::md_ast::parse(source), options, consumer)
}
pub(crate) fn validate_document(doc: &crate::md_ast::Document, options: &Value, docx: bool) -> Result<(), &'static str> {
    validate_document_consumer(doc, options, if docx { Consumer::Docx } else { Consumer::Unsupported })
}
fn validate_document_consumer(doc: &crate::md_ast::Document, options: &Value, consumer: Consumer) -> Result<(), &'static str> {
    use crate::md_ast::{Block, Inline};
    let requested = global(options)?;
    if requested.is_some() && consumer != Consumer::Docx { return Err(UNSUPPORTED); }
    let mut pending = doc.blocks.iter().map(|block| (block, true, false)).collect::<Vec<_>>();
    let mut declarations = 0;
    while let Some((block, ordinary, local_parent)) = pending.pop() {
        if crate::speech::cancelled() { return Err("cancelled"); }
        match block {
            Block::Styled { attributes, block } => {
                let tracking = local(&attributes["text"])?;
                if tracking.is_some() {
                    declarations += 1;
                    if declarations > crate::publication_blocks::MAX_NODES { return Err(INVALID); }
                    if matches!(block.unstyled(), Block::Math(_) | Block::OfficeMath { .. }) { return Err(INVALID); }
                    // This first increment explicitly accepts one local tracking
                    // declaration per ordinary paragraph. The scoped writer
                    // must not broaden that contract; nested declarations stay
                    // unsupported until their precedence is separately covered.
                    if consumer == Consumer::Unsupported || !ordinary || local_parent || !matches!(block.unstyled(), Block::Paragraph(_)) { return Err(UNSUPPORTED); }
                    if matches!(block.unstyled(), Block::Paragraph(values) if values.len()==1 && matches!(values.first(), Some(Inline::Image{..}))) { return Err(UNSUPPORTED); }
                }
                pending.push((block.as_ref(), ordinary, local_parent || tracking.is_some()));
            },
            Block::Quote(children) | Block::FootnoteDef { blocks: children, .. } => pending.extend(children.iter().map(|block| (block, false, false))),
            Block::List { items, .. } => for item in items { pending.extend(item.blocks.iter().map(|block| (block, false, false))); },
            _ => (),
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "publication_tracking_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "publication_tracking_docx_tests.rs"]
mod docx_tests;

#[cfg(test)]
#[path = "publication_tracking_reader_tests.rs"]
mod reader_tests;
